# SDK 0.9.1 분석 메모

원본 URL: https://mangoslab.blob.core.windows.net/nemonic-web-sdk/v0.9.1/nemonic-web-sdk-0.9.1.zip
ZIP SHA-256: dde3219a1c6b458ac63548fa06d7202f328bb670aa393033c0263d13aae5681f
WASM SHA-256: 2c3cc86266d4241bc855623a955af80f91235d4656220f8d469ed8f9055885f3
배포 checksums.txt의 6개 항목 모두 검증 성공.

## 원본 구조
- Nemonic.js (155954 bytes): USB/BLE 프로토콜, 장치 프로파일, 상태 파서, WASM/Worker 호출.
- Nemonic.wasm (27445 bytes): import 없음, WASI 불필요, API v2.
- nemonic_wasm.js: 난독화된 로더, RGBA 메모리 전달 및 함수 호출.
- print-worker.js: 브라우저 worker 분리 계층, Rust에서는 불필요.

## WASM ABI
- memory
- alloc(length:i32) -> pointer:i32
- dealloc(pointer:i32, length:i32)
- build_print_command(width:i32, height:i32, rgba_ptr:i32, rgba_len:i32,
  copies:i32, dither:i32) -> pointer:i32
- get_last_result_len() -> i32
- get_last_error_code() -> i32
- get_wasm_api_version() -> i32 (2)

## USB 장치 매핑 (Nemonic.js getUsbPrinterProfile)
| VID:PID | 모델 | 선호 Bulk OUT endpoint number |
|---|---|---|
| 1c8a:3a21 | MIP-001 / MIP-001L | 2 |
| 36ec:1001 | MIP-201 | 1 |
| 36ec:1002 | MIP-301 | 1 |

실제 인터페이스/alternate/IN은 descriptor에서 검색하며 interface 0을 우선합니다.
MIP-201은 연결마다 OUT 및 (조회 시 필수) IN CLEAR_FEATURE(ENDPOINT_HALT)를
최대 3회 재시도해야 합니다. 실패 시 인쇄 데이터를 보내지 않습니다.
이는 SDK 0.9.1의 이전 이미지 재출력 방지 수정 사항입니다.
활성 alternate setting을 불필요하게 재선택하지 않습니다.
MIP-201 응답은 512 bytes 단위로 읽어 64-byte 초과 장치 정보의 잔여 패킷을 방지합니다.

## 확인된 인쇄 패킷
8x1 흰색 이미지 1매:
`02 1b40 1b4301 1d763000 0100 0100 00 1b50 03`

구조: STX, ESC @, ESC C copies, GS v 0 mode,
width-in-bytes LE16, height LE16, monochrome raster, ESC P, ETX.
v0.2.0은 아래 분석대로 래스터 생성/디더링을 네이티브 Rust로 재구현합니다.
임의 ESC/POS 텍스트 프린터로 취급하지 않습니다.

## 상태 조회
- 정보: `1d 49 02`, 30초 대기, 10/11개 ASCII 토큰 검사.
- 기본 상태: `10 04 02`.
- bit2=커버, bit3=과열, bit5=용지 없음, bit6=용지 걸림.
- 우선순위 과열 > 커버 > 용지 없음 > 걸림.
- 정보 marker로 이전 응답을 소비합니다. 읽기 실패 후 세션을 계속 재사용하지 않습니다.
- SDK의 모든 복구/명령 변형은 이식하지 않았습니다. README의 한계를 참고하십시오.

## 보안/운용
네이티브 입력 RGBA 128 MiB 제한, CLI 이미지 폭/높이 각각 16384 제한.
너비 바이트 수는 LE16 범위를 검사하고 높이는 2000으로 자릅니다.
WASM 비교 테스트에만 memory 256 MiB 및 fuel 제한을 적용합니다. USB 쓰기는 실패 시 자동 재출력하지 않습니다.
이 구현은 제조사 공식 Rust SDK가 아닙니다. 실물 장치에서 검증하지 않았습니다.


## v0.2.0 네이티브 재구현 근거

개발 환경에서 `wabt`의 `readWasm` → WAT로 `build_print_command`를 분석했습니다.
원본 WASM에는 Rust 컴파일러 심볼이 남아 있지만 원래 소스 파일을 복원한 것은 아닙니다.
WAT/제조사 바이너리는 저장소에 포함하지 않습니다.

### dither=false
1. RGBA를 f32로 읽고, alpha가 255가 아니면 각 채널을
   `a = alpha/255.0; channel = a*channel + (1.0-a)*255.0`으로 흰색 합성.
2. HSL lightness `(max(R,G,B)+min(R,G,B))*0.5`가 **128 미만**이면 검정.
3. 행 우선, MSB-first, 마지막 바이트의 남는 비트는 0(흰색).

주의: 검정 alpha=127은 f32 합성 결과가 128보다 약간 작아서 검정으로 판정됩니다.
이를 정수 합성으로 바꾸면 원본과 달라집니다.

### dither=true
1. RGB 원본의 3x3 sharpening: `(13*center - sum(8 neighbors))/5`,
   signed division은 0 방향 절삭 후 [0,255] clamp. alpha는 center 값을 유지.
   첫/마지막 행과 첫/마지막 열은 그대로 복사. 소스 버퍼와 출력 버퍼는 분리.
2. 각 채널을 `(channel*alpha + (255-alpha)*255)/255` 정수 연산으로 흰색 합성.
3. f32 회색조: `B*0.114 + (G*0.587 + R*0.299)` 순서로 계산 후 i32 절삭.
4. 왼쪽→오른쪽, 위→아래 Sierra 3행 오차 확산:

```text
          X 5 3
      2 4 5 4 2
        2 3 2         / 32
```

현재값 <=127은 검정, 그 외 흰색. 확산 error는 검정이면 max(value,0),
흰색이면 min(value,255)-255. 각 이웃에는 `(error*weight+16)/32`를 더함.
음수에도 +16을 사용하며 signed division은 0 방향 절삭합니다.
이웃 좌표는 행/열 경계에서 건너뜁니다. 회색조 버퍼는 중간에 clamp하지 않습니다.

### 검증과 호환 범위
- 기존 JS glue 기준 8개 golden fixture 일치.
- 원본 WASM과 네이티브 Rust 직접 비교: 1312개 입력 조합.
- 크기 1px/2px, 알파, 경계 회색, 색상, 랜덤, 매수, 디더링, 높이 자르기 포함.
- 전체 가능한 이미지 입력의 동치 증명은 아님. USB 실기 검증은 별도 필요.
- 네이티브 함수는 Result 오류와 메모리 한도를 사용하므로 원본 WASM ABI/에러 코드까지
  재현하지 않으며, 지원 범위 내 정상 입력의 출력 바이트 호환을 목표로 합니다.

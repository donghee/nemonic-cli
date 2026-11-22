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
래스터 생성/디더링은 원본 WASM에 위임합니다. 임의 ESC/POS 텍스트 프린터로 취급하지 않습니다.

## 상태 조회
- 정보: `1d 49 02`, 30초 대기, 10/11개 ASCII 토큰 검사.
- 기본 상태: `10 04 02`.
- bit2=커버, bit3=과열, bit5=용지 없음, bit6=용지 걸림.
- 우선순위 과열 > 커버 > 용지 없음 > 걸림.
- 정보 marker로 이전 응답을 소비합니다. 읽기 실패 후 세션을 계속 재사용하지 않습니다.
- SDK의 모든 복구/명령 변형은 이식하지 않았습니다. README의 한계를 참고하십시오.

## 보안/운용
WASM memory 256 MiB 제한, 입력 RGBA 128 MiB 제한, 출력 64 MiB 제한,
실행 fuel 제한. USB 쓰기는 실패 시 자동 재출력하지 않습니다.
이 구현은 제조사 공식 Rust SDK가 아닙니다. 실물 장치에서 검증하지 않았습니다.

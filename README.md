# nemonic-cli — Native Rust Linux USB CLI (experimental)

Nemonic Web SDK **0.9.1**을 분석한 USB용 Rust CLI 프로토타입입니다.
브라우저, JavaScript 런타임, Node.js는 실행에 필요하지 않습니다.
**v0.2.0부터 인쇄 데이터 생성도 네이티브 Rust**입니다. USB는 **rusb/libusb**입니다.
실행·일반 빌드·기본 테스트에 제조사 WASM 파일이나 WASM 실행기가 필요하지 않습니다.
`wasmi`는 개발용 원본 비교 테스트에서만 사용하는 dev-dependency입니다.

## 설치 / 빌드

Ubuntu/Debian 예시 (최근 stable Rust/Cargo 필요):

```sh
sudo apt-get install build-essential pkg-config libusb-1.0-0-dev
cargo build --release --locked
cargo test --locked
```

SDK 아카이브에서 라이선스 파일을 확인하지 못했으므로 원본 WASM/JS는 재배포하지 않습니다.
원본 비교 테스트가 필요할 때만 다운로드 스크립트로 WASM을 받습니다.
SDK 분석·재구현 및 배포 허용 범위는 제조사 조건을 별도로 확인하십시오.

## 사용

실행 파일은 작업 디렉터리와 무관하게 사용할 수 있습니다.
이전 버전의 `--wasm` 옵션과 `NEMONIC_WASM` 환경변수는 더 이상 사용하지 않습니다.

```sh
./target/release/nemonic-cli list
./target/release/nemonic-cli render image.png -o job.bin --dither --copies 2
# 원본 크기 유지
./target/release/nemonic-cli render image.png -o original.bin --no-resize
./target/release/nemonic-cli --bus 1 --address 5 info
./target/release/nemonic-cli --bus 1 --address 5 status
./target/release/nemonic-cli --bus 1 --address 5 print image.png --dither
./target/release/nemonic-cli --serial YOUR_SERIAL print image.jpg
```

- 장치가 여러 개면 명시적으로 선택해야 합니다. bus/address는 재연결 시 바뀔 수 있습니다.
- `render`는 USB에 접근하지 않습니다. 출력 파일이 이미 있으면 덮어쓰지 않습니다.
- `print`는 즉시 용지 급지/커팅이 발생할 수 있습니다. 먼저 `render`로 확인하십시오.
- PNG/JPEG만 지원합니다. PDF/텍스트 렌더링은 구현하지 않았습니다.
- `render`와 `print`는 기본적으로 이미지를 가로 **576px**로 확대/축소하고 세로 비율을 유지합니다.
  높이는 공식 샘플의 canvas 정수 변환처럼 소수점 이하를 버리며 최소 1px입니다.
  `--no-resize`를 지정하면 원본 픽셀 크기를 유지합니다.
- 보간은 Triangle 필터이며 브라우저 Canvas와 픽셀 단위 동일성을 보장하지 않습니다.
  투명 이미지의 보간은 premultiplied alpha로 처리하고 다시 RGBA로 변환합니다.
- 변환 후 높이가 2000px를 초과하면 경고하며, 렌더러는 위쪽 2000px까지만 출력합니다.
  자동 분할이나 용지별 높이 맞춤은 하지 않습니다. 변환 결과 RGBA는 최대 128 MiB입니다.
- 이 자동 맞춤은 모델과 무관하게 적용됩니다. 576px는 공식 샘플 기준이며 물리 인쇄 폭 보장은 아닙니다.
- 알파 채널은 원본 ImageData처럼 그대로 전달합니다. 투명 배경 처리가 의도와 다른 경우
  미리 흰색 배경으로 합성한 이미지를 사용하십시오.
  네이티브 구현 내부에서도 SDK와 동일하게 흰색에 합성하지만 모드별 반올림 방식이 다릅니다.
- JSON `sent: true`는 USB 전송 성공이지 실제 인쇄 완료 보장이 아닙니다.
- 인쇄 전송 실패 후에는 데이터 일부가 전달됐을 수 있으므로 무조건 재시도하지 마십시오.

## USB 권한 (호스트 Linux에서 설정)

```sh
sudo groupadd -f plugdev
sudo usermod -aG plugdev "$USER"
sudo install -m 644 udev/70-nemonic.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
```

로그아웃/로그인하고 USB를 다시 연결하십시오. 상시 root 실행이나 MODE=0666은 권장하지 않습니다.
다른 프린터 프로그램/CUPS가 인터페이스를 사용하면 충돌할 수 있습니다.
커널 드라이버는 libusb 자동 분리/재연결을 사용하며 시스템 전역 usblp 제거는 하지 않습니다.
Docker에서는 별도로 해당 USB 장치를 컨테이너에 전달해야 합니다.

## 구현 범위와 한계

| 기능 | 상태 |
|---|---|
| 장치 목록 / bus,address,serial 선택 | 구현 |
| PNG/JPEG → SDK 호환 명령 / 매수 / 디더링 | 네이티브 Rust 구현, 원본 WASM과 바이트 비교 테스트 |
| USB 인쇄 | 구현, 실제 프린터 미검증 |
| 장치 정보 / 기본 상태 | 실험적 구현, 실제 프린터 미검증 |
| 배터리 / 카트리지 / printable status | 미구현 |
| Bluetooth / USB-IP 클라이언트 | 미구현 |
| 원본 WASM 없이 네이티브 Rust 래스터 알고리즘 | 구현 |

상태 조회는 SDK 전체를 이식한 것이 아닙니다. GS I marker로 먼저 응답 순서를 맞춘 뒤
기본 명령 10 04 02만 사용하며, SDK의 추가 상태 명령 변형/전체 복구 재시도는 생략했습니다.
실패 시 잘못된 응답을 해석하지 않고 종료합니다. 반복 실패하면 USB를 재연결하십시오.
MIP-201의 printable-status USB 미지원 문제 때문에 해당 명령을 제공하지 않습니다.

상태 조회의 read-before-write는 별도 libusb blocking read 스레드와 5ms 스케줄링
유예를 사용합니다. URB 제출 완료를 보장하는 async transport 구현은 아니므로,
기기/호스트에 따라 타이밍 문제가 남을 수 있습니다. 이 부분은 실기 검증 대상입니다.

## 테스트

`cargo test --locked`는 WASM 파일 없이 동작합니다. 8가지 기존 JS glue 출력 fixture,
임계값/알파/비트 패딩/매수/입력 검증/상태 파서를 검사합니다.
원본 WASM과의 추가 비교는 선택적으로 실행합니다:

```sh
python3 scripts/fetch_sdk.py
cargo test --locked --features wasm-oracle -- --nocapture
```

추가 비교는 결정적 난수 seed로 **1312개 입력 조합**을 검사합니다.
1px 크기, 홀수 폭, 127/128 경계, 투명도, 높이 2001, 576px 폭 및 무작위 크기를
일반/디더링 두 모드에서 비교합니다. 테스트 범위의 바이트 일치는 모든 입력에 대한
수학적 동치 증명이나 실물 출력 검증을 뜻하지 않습니다.

개발용 fixture 재생성에는 원본 SDK dist와 Node.js가 필요합니다:

```sh
node scripts/generate_golden.cjs /path/to/nemonic-web-sdk-0.9.1/dist
```

라이브 USB 호출은 자동 테스트에서 수행하지 않습니다.
분석 근거는 ANALYSIS.md를 참고하십시오.

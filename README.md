# nemonic-cli — Linux Rust USB CLI (experimental)

Nemonic Web SDK **0.9.1**을 분석한 USB용 Rust CLI 프로토타입입니다.
브라우저, JavaScript 런타임, Node.js는 실행에 필요하지 않습니다.
인쇄 데이터 생성은 원본 WASM을 **wasmi**로 실행하며, USB는 **rusb/libusb**입니다.
**WASM 알고리즘까지 순수 네이티브 Rust로 재작성한 것은 아닙니다.**

## 설치 / 빌드

Ubuntu/Debian 예시 (최근 stable Rust/Cargo 필요):

```sh
sudo apt-get install build-essential pkg-config libusb-1.0-0-dev python3
python3 scripts/fetch_sdk.py
cargo build --release --locked
cargo test --locked
```

SDK 아카이브에서 라이선스 파일을 확인하지 못했으므로 원본 WASM/JS는 소스 ZIP에
재배포하지 않았습니다. 다운로드 스크립트가 지정 URL에서 WASM을 받아 SHA-256을 확인합니다.
원본 SDK의 사용/재배포 권한은 제조사 조건을 별도로 확인하십시오.

## 사용

프로젝트 루트에서 실행하거나 `--wasm /absolute/path/Nemonic.wasm` 또는
`NEMONIC_WASM` 환경변수로 경로를 지정합니다.

```sh
./target/release/nemonic-cli list
./target/release/nemonic-cli render image.png -o job.bin --dither --copies 2
./target/release/nemonic-cli --bus 1 --address 5 info
./target/release/nemonic-cli --bus 1 --address 5 status
./target/release/nemonic-cli --bus 1 --address 5 print image.png --dither
./target/release/nemonic-cli --serial YOUR_SERIAL print image.jpg
```

- 장치가 여러 개면 명시적으로 선택해야 합니다. bus/address는 재연결 시 바뀔 수 있습니다.
- `render`는 USB에 접근하지 않습니다. 출력 파일이 이미 있으면 덮어쓰지 않습니다.
- `print`는 즉시 용지 급지/커팅이 발생할 수 있습니다. 먼저 `render`로 확인하십시오.
- PNG/JPEG만 지원합니다. PDF/텍스트 렌더링, 자동 리사이즈는 구현하지 않았습니다.
- 픽셀 크기를 그대로 SDK에 전달합니다. SDK는 유효 높이를 최대 2000px로 제한합니다.
- 알파 채널은 원본 ImageData처럼 그대로 전달합니다. 투명 배경 처리가 의도와 다른 경우
  미리 흰색 배경으로 합성한 이미지를 사용하십시오.
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
| PNG/JPEG → SDK 호환 명령 / 매수 / 디더링 | 구현, 원본 JS glue와 바이트 비교 테스트 |
| USB 인쇄 | 구현, 실제 프린터 미검증 |
| 장치 정보 / 기본 상태 | 실험적 구현, 실제 프린터 미검증 |
| 배터리 / 카트리지 / printable status | 미구현 |
| Bluetooth / USB-IP 클라이언트 | 미구현 |
| 원본 WASM 없이 순수 Rust 래스터 알고리즘 | 미구현 |

상태 조회는 SDK 전체를 이식한 것이 아닙니다. GS I marker로 먼저 응답 순서를 맞춘 뒤
기본 명령 10 04 02만 사용하며, SDK의 추가 상태 명령 변형/전체 복구 재시도는 생략했습니다.
실패 시 잘못된 응답을 해석하지 않고 종료합니다. 반복 실패하면 USB를 재연결하십시오.
MIP-201의 printable-status USB 미지원 문제 때문에 해당 명령을 제공하지 않습니다.

상태 조회의 read-before-write는 별도 libusb blocking read 스레드와 5ms 스케줄링
유예를 사용합니다. URB 제출 완료를 보장하는 async transport 구현은 아니므로,
기기/호스트에 따라 타이밍 문제가 남을 수 있습니다. 이 부분은 실기 검증 대상입니다.

## 테스트

`cargo test --locked`는 8가지 원본 JS glue 출력 fixture를 Rust WASM bridge 결과와
비교합니다 (흰색/검정/홀수 폭/매수 255/알파/디더링/높이 2001/576px 폭).
이는 bridge 호환성을 검증하며 독립적인 인쇄 알고리즘 검증이나 실물 출력 검증은 아닙니다.
개발용 fixture 재생성에는 원본 SDK dist와 Node.js가 필요합니다:

```sh
node scripts/generate_golden.cjs /path/to/nemonic-web-sdk-0.9.1/dist
```

라이브 USB 호출은 자동 테스트에서 수행하지 않습니다.
분석 근거는 ANALYSIS.md를 참고하십시오.

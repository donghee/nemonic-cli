# GeekNews → nemonic-cli

GeekNews(news.hada.io) Atom 피드에서 최근 글을 가져와 576px 폭 1비트 PNG로
렌더링하고 nemonic-cli로 인쇄하는 예제입니다.

## 요구

- Python 3 (표준 라이브러리만 사용)
- ImageMagick 7 (`magick`), Pango delegate 포함 (`magick -list format | grep PANGO`)
- 한글 Bold 폰트 (기본값 `/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc`)
- 빌드된 `nemonic-cli` (`cargo build --release --locked`)

## 사용

```sh
# 최근 2개, 기본 설정
./print.sh

# 3,4번째 뉴스
./print.sh 2 2

# PNG만 생성 (인쇄 없음)
python3 render.py --count 2 -o out.png
```

### 환경 변수

| 변수 | 기본값 | 설명 |
|---|---|---|
| `NEMONIC_CLI` | `../../target/release/nemonic-cli` | CLI 경로 |
| `NEMONIC_SERIAL` | (없음) | 지정 시 `--serial` 로 대상 선택 |
| `OUT` | `./geeknews.png` | 생성 PNG 경로 |
| `FONT` | NotoSansCJK-Bold.ttc | 렌더 폰트 |

USB 재열거로 bus/address가 자주 바뀌므로 `NEMONIC_SERIAL` 지정이 안정적입니다.

## 설계 메모

- Pango의 CJK-aware 자동 줄바꿈을 그대로 사용 (수동 wrap이 "5,000" 같은
  숫자나 영단어 중간을 쪼개는 문제를 피하기 위해).
- 기본은 `-colorspace Gray`로 그레이스케일 PNG를 생성합니다. nemonic-cli의
  `--dither`가 중간톤을 디더 패턴으로 표현할 수 있도록 하기 위함입니다.
  `--threshold 60%` 를 넘기면 순수 1비트 PNG가 되어 디더링은 no-op이 됩니다.
- `-stroke black -strokewidth 1.0` 로 글자 획을 두껍게 하여 디더/threshold
  변환 후에도 선이 끊기지 않게 합니다.
- 각 항목 본문은 기본 260자로 잘라 이미지 높이 2000px 상한(렌더러는 위쪽
  2000px까지만 출력) 안에 들어오도록 합니다.

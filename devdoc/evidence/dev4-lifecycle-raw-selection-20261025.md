# DEV4 P5 원시 번호·selector의 선택 검증

2026-10-25 · **신규 선택 시험 PASS / 최종 독립 D1 제한 수용 / 전체 P5 미완료**.

[소유 계약](../contracts/dev4-lifecycle-raw-selection.md) D4-LM-RAW-001의 세 API만 구현했다.
기준은 codec 커밋 `7b32338d9c1b8e71d4567dc78ab9726cd99dc2db`이며 D2 최종 관측 HEAD는
외부 DEV3 커밋을 포함한 `7aecb9a8b18e203da6236e9989e88b8269d9ec26`이다.
DEV3/COMM의 동시 변경은 이 작업 범위에서 제외한다.

## 입력과 검토

원자료는 `artifacts/dev4-lifecycle-model-20261024/`에 보존한다. `/root`가 계약·통합,
`/root/frontend_research`가 raw index/selector, `/root/frontend_oracle`가 독립 fixture/시험,
`/root/frontend_review`가 독립 D2/D1을 맡는다.

원1533 action 순서와 selector 의미를 분해하고 Prepare의 N 위치·국소 오류 우선순위를 명시했다.
정적 독립 입력 v1은1945개·예정3478호출이었다. 독립 검토에서 FrozenU0가 caller guard를
우회하는 잘못된 구현을 놓치는 공백을 찾아 v2에3개를 추가했다. 이어 전체 ordinal 중복을
nonce 중복보다 먼저 검사하는 witness3개와 actor/slot의 중복 우선 witness5개를 보완했다.
최종 v3은1951개·예정3484호출이다. 기존5개는 입력 배치만 바뀌고 기대 오류는 같으며
기존 기대값 변경·삭제는 없다. 원 v1/v2와 correction/hash를 보존했다.

최종 독립 D2 `review-raw-selector-d2-v1.json` SHA는
`acdc3df977263b49eebdfa823489b4d43ab9ccf0d5041377fa7cffaf3c8d5794`다.
원33입력의 실제 hash를 대조한 owner freeze 뒤 v3의11파일을
`tests/fixtures/dev4-lifecycle-raw-selection`에 byte동일 복사했다. 정적 authoring·hash 대조는
Rust 제품 실행·시험 PASS가 아니다. original51action의 전체 input 조립도 아직 수행하지 않았다.

## 선택 실행 상태

원1533은 각 descriptor의 양방향 결과를 독립 값과 비교한다. index 오류3·필드 오류123,
selector baseline96/history9/namespace12/local errors115/read-set control60을 구분한다.
이1951은 논리 입력 수이고 실제 API 호출은 action_at1536·index_of1656·selector292다.
방향별 완료 ID를 기록해 실패 후 완료 방향을 반복하지 않고 관련 diff와 미실행 suffix를 선택한다.
모든 신규 그룹의 최초 API 호출을256KiB thread 안에서 실행했으며 별도 반복 stack 시험은 없다.
allocator 계측은 계획하지 않았고 heap0는 제품 코드의 정적 근거와 구별한다.

`raw-suite-r1`은 source150파일과 입력을 보존한 뒤 live checkout에서 shared target으로
compile했다. immutable export 실행이나 workspace 전체 fingerprint의 증거로 주장하지 않는다.
source manifest SHA는 `b74b49f0db065864f9962f7ca041125e82ed96275f195a0ca70d65d4fa294ec0`,
보존 ELF SHA는 `1a875feca5320b668bbbf1deebdf128e89b354a24e858b5b096933becd7f98d4`다.
시험 result SHA는 `4148e0b0b2211a49e3414ac3cd8d432162c666de1aad4978a4986e8cec89aa4e`이며 시험 전·후 source hash가 같았다.

| 새 시험 그룹 | 논리 입력 | 실제 API 호출 | 결과 |
| --- | ---: | ---: | --- |
| raw exact mapping | 1533 | 3066 | PASS |
| invalid index | 3 | 3 | PASS |
| invalid fields | 123 | 123 | PASS |
| selector baseline | 96 | 96 | PASS |
| selector history | 9 | 9 | PASS |
| selector namespace | 12 | 12 | PASS |
| selector local errors | 115 | 115 | PASS |
| selector read-set controls | 60 | 60 | PASS |
| 합계 | 1951 | 3484 | PASS |

`run-raw-selection.py raw-suite-r1 build`는 reference의 `cargo test --lib --no-run --offline --locked`로
시험을 컴파일하고 실행0을 보존했다. `test` 단계는 `selection.json`의 새8함수만
보존 ELF에서 `--exact --nocapture --test-threads=1`로 선택했다. 각 ID·방향의
START/COMPLETED3484가 고정 inventory와 정확히 대응하며 추가 호출·재실행·생략은0이다.
기존 codec·앞단·DEV3·COMM 사례는 실행하지 않았다. baseline initial literal 로드는 새 selector
입력 준비이며 기존 reader/codec 시험 반복이 아니다. allocator 동적 계측은 NOT_RUN이다.

Clippy는 `cargo clippy -p xadbtest-reference --lib --tests --offline --locked -- -D warnings`,
fmt는 변경 Rust7파일의 `rustfmt --edition 2024 --config skip_children=true --check`가 PASS다.
둘 다 시험 사례를 실행하지 않았다. 제품 compile·runtime·Clippy·fmt 실패는 이번 source에서0이다.
정적 receipt 집계 r1은 원 fixture의 전역 순서와 그룹별 실행 순서를 같게 비교하여 FAIL했다.
`receipt-audit-r1-failure.json`에 원 AssertionError와 원인을 보존했다. 제품·fixture·시험 수정 없이
r2에서 그룹 내부 원 순서와 전체 membership을 대조하여1951ID·3484방향의 완결을 확인했다.
집계 재검사는 제품 호출0이며 r1을 PASS로 덮어쓰지 않았다.

최종 독립 D1은 source150·fixture11·후보28경로·ELF와 완료3484방향을 대조해 이 세 API만
제한 수용했다. `review-raw-selector-d1-v1.json` SHA는
`1d3ba7e324a6d69ea292524ea3edc58fb0c87c35e294510bcaafa0817c5eb50d`다.
후속 문서 변경은 대기→제한 수용 상태와 이 receipt 연결뿐이며 제품·시험·입력은 그대로다.
첫 codec의 PASS를 새 함수의 실행 결과로 사용하지 않는다.

## 남은 범위

full ResolvedInput·Env1 조립,51action 전체 연결, 모델·독립 edge checker·BFS·파일 재개·FE14는
미완료다. 전체 BFS용10GiB 영속 보존 공간도 아직 미충족이다. 작은 selector의 성공을
실제 handle 인증·SQL/engine·내구성·DEV4/Q1 완료로 확대하지 않는다. 문서 verifier·push는
실행하지 않았다.

# DEV4 P5 독립 edge checker와 모델 차등 판정

2026-10-25 · **checker·모델 선택 시험 PASS / 모델 차등 불일치0 / 독립 검토 D2 수용·D1 제한 수용 / alias·BFS·FE14 미완료**.

[소유 계약](../contracts/dev4-lifecycle-checker.md) D4-LM-CHECK-001의 `verify_edge`를 구현하고 모델 출력을
그 checker로 판정했다. 기준은 `dev4-independent`의 모델 commit `de0a5be`이며 별도 worktree
`/home/et16/work/xadb-dev4`에서 작업했다. 다른 checkout의 DEV3 작업은 읽거나 바꾸지 않았고 원문은 그
checkout의 `artifacts/dev4-lifecycle-model-20261024/`에서 읽기만 했다.

## 역할과 독립성

| 역할 | 담당 | 산출물 |
| --- | --- | --- |
| checker 작성 | 모델 코드를 읽지 않은 별도 작성자 | `checker.rs`·`checker/`·`checker_tests.rs` |
| 계약·모델·차등 판정 | 이번 작업 세션(모델 계약 owner) | checker 계약, 모델 교정, `differential_tests.rs` |
| 독립 검토 | 둘 다에 참여하지 않은 별도 검토자 | D2·D1 |

checker의 기대값은 손 literal과 원문 규칙에서만 나온다. 모델 출력·모델 helper·조립기로 기대값을 만들지
않았다. 차등 불일치는 자동 모델 버그로 보지 않고 원문 순위로 판정했으며 checker 쪽 교정은 그 작성자에게
판정 내용만 전달해 고치게 했다. 검토 원문은 로컬 `artifacts/dev4-lifecycle-assembler-20261025/`에 있다.

| 검토 원문 | SHA-256 |
| --- | --- |
| `review-checker-d2-d1-v1.json` (D2 ACCEPT_WITH_CHANGES, D1 ACCEPT_LIMITED) | `90450674716e0aea5f27d4a60746982c8c732425b1ec5476e7f1270a932642f9` |
| `review-checker-closure-v1.json` (D2 ACCEPT_WITH_CHANGES, D1 ACCEPT_LIMITED) | `ed31d7f7c692ed3910bed0b096ffa54f8329558935146fdf18b955292fa07257` |
| `review-checker-closure-v2.json` (D2 ACCEPT, D1 ACCEPT_LIMITED) | `a0af20b9ddc6b7b6eab2852dc0b5feb3a776a6a1adfc6bb4be310d2ee4dea946` |

## 차등 판정

`differential_tests`가 literal 29 상태와 결정적 walk에서 조립기가 해석한 모든 raw slot의 모델 edge를
`verify_edge`로 판정했다. 처음 walk30에서 실패 key 53개가 원인 4종으로 묶였고 그 교정 뒤 1종이 더 드러났다. walk 확대
(1000×6, 3000×10)에서 3종, 검토 뒤 더한 손상 입력 대조에서 1종, 조립 오류 집계에서 조립기 공백 1종이
더 나왔다. 판정 결과는 모델 계약의
[차등 판정](../contracts/dev4-lifecycle-model.md#checker-adjudication)에 owner 결정으로 남겼다.

| 불일치 | 처음 발견 | 판정 | 교정 |
| --- | --- | --- | --- |
| InspectStatement child 인증 실패의 authenticated | literal 상태 | 모델 계약 Edge 규칙 누락 | 모델 |
| EngineFenced의 engine cause | `rw.recovery_pending` 등 | 원문 미정, first_cause 채택 | 모델 |
| 첫 fence 뒤 ResultOpen slot | `off.select_open` | v3 §2와 맞는 ReadyBound 채택 | 모델 |
| CloseSession Incomplete의 session | `off.select_open` | wire-v2 §4·SQL close_session에 따라 Open 유지 | checker |
| atomic Fetch UndoIncomplete9의 next_work | `off.select_open` | 원 class가 next work를 지정하지 않아 NotConsumed | 모델 |
| fenced CloseSession handoff의 completion_fact | walk1000 | literal-design §4 "종료/미완료를 확인한 fact"가 아님 | 모델 |
| Recovery 성공 certainty | walk1000 | CommitFact·Signal과 같은 외부 fact, NotApplicable | 모델 |
| fenced cursor CloseStatement의 completion_fact | walk3000×10 | Active implicit 거래만 끝냄, 기존 fact 보존 | 모델 |
| continuation의 다른 context identity | 손상 입력 | 공급 envelope 불일치 일반 규칙, Envelope | 모델 |
| 끝난 no-write commit의 fenced Finish NCR 조립 | walk3000×10 | 조립기 InvalidState였으나 도달 상태, row 없는 header | 조립기 |

handoff 판정의 첫 교정은 fenced Finish NCR handoff까지 막아 walk600×14에서 새 불일치를 만들었다. Finish NCR은
NCR을 확인하므로 completion_fact가 맞다고 보고 CloseSession handoff로 범위를 좁혔다. 판정 지점의 walk 접두는
`differential_tests`의 고정 경로로 남겼고, Recovery certainty와 handoff 교정을 되돌린 변형에서
회귀 경로가 각각 불일치를 보고하는 것을 확인했다.

## 독립 검토와 보완

검토는 세 번이었다. 1차는 D2 ACCEPT_WITH_CHANGES·D1 ACCEPT_LIMITED였다. 주요 지적은 v5 §1의 "인증
이후 거절의 인증된 참조" 의무가 모델 단위에서 이 단위로 넘어온 뒤 어디에도 남지 않은 점이었다. 이를 owner 결정으로 정해
모델과 checker가 각자 구현했다. 원 literal `sel.entry_cancel`은 그 규칙보다 앞서 targets를 생략했으므로
(순위5와 순위3의 충돌) 두 시험이 같은 정정으로 대조한다.

Recovery가 끝낸 peer의 completion_fact를 recovery fact로 두는 결정도 시도했으나 checker 시험의 codec loader
검산이 거절했다. codec은 completion_fact가 그 transaction의 fact이기를 요구하고 recovery fact는 transaction이
없으므로 바꾸지 않는 쪽으로 정정했다. 그 밖에 RecoveryPre 수용 범위 축소, checker 정렬의 heap 비의존,
`CheckerCapacity` 분리, 계약 근거·인용 보강, 차등 하네스 보강(조립 오류 집계, 거절 경로 walk, 마지막 상태
판정, coverage 출력, 손상 입력 대조)을 반영했다.

1차 종결 검토(D2 ACCEPT_WITH_CHANGES)는 Recovery fact·fresh identity 불일치에서 checker가 RecoveryPre만 받는
점을 찾았다. 두 규칙이 함께 해당하므로 둘 다 받도록 고쳤고, 인증 참조의 transaction을 wire-v2 §4의 호출별
선택에 맞췄으며 continuation identity 검사의 gate 위치와 판정 입력 영역을 계약에 적었다. 2차 종결은 D2
ACCEPT·D1 ACCEPT_LIMITED이며 D1의 제한은 walk가 smoke이고 alias·BFS·FE14가 남았다는 뜻이다. 그 검토의
minor 셋(조항 인용, envelope 목록 누락 둘, 조립기 분기의 독립 case 부재)은 문구와 근거 기록으로 닫았다.
조립기 분기는 독립 조립 case가 없고 다섯째 고정 경로가 시험 근거다.

## 최종 선택 실행

`run-checker.py checker-final-r2`는 checker·모델·조립기·차등 source와 literal manifest를 보존하고 reference
시험을 한 번 compile한 뒤 보존 ELF에서 변경 코드에 대응하는 시험 함수만 `--exact --nocapture
--test-threads=1`로 실행했다. 조립기는 Finish 분기 변경 때문에, 모델은 교정 때문에 다시 실행했다.

| 항목 | 값 |
| --- | --- |
| 기준 HEAD | `de0a5bed9ebbfc1041c29d4b30a9187da65ae935` + 이 단위의 미commit 변경 |
| source·literal manifest SHA | `bd9cb8a2299a441f1e3e7bc46c227e843b6b97eb37f15f3085032731c7a43ba0` |
| 보존 ELF SHA | `7b4d8adcf9059e4bc2236207bbf04329387327412c41e5fb090c082cf55e8785` |
| receipt SHA | `9a797c578f53d873efa8e033ee196119961961f81e4f08f0f6f70369312aca9e` |
| 실행 전·후 source | 동일 |

| 시험 묶음 | 결과 |
| --- | --- |
| `checker_tests` 10 | PASS. literal 29/29(인증 참조 정정1), known-bad 5 mutant·14 관측 거절, 표적 변형20, 제외11, fenced close, fence·continuation 변형19, owner 결정14, 조립 입력41,685의 total 판정, 손상 입력47,624 무panic, 자기 walk 252상태·278,208입력 |
| `model_tests` 6 | PASS. literal 29 edge·12 trace 51 occurrence·5 known-bad, 고정 trace, census, walk200 |
| `assembler_tests` 11 | PASS. 기존 입력 1994개 무회귀 |
| `differential_tests` 6구성 | 모두 불일치0, 조립 오류는 ProfileLimit뿐 |

| 차등 구성(walk×깊이) | 판정 edge | Admitted | Continuation | NoContextIssue | 제외 | coverage key |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 기본 30×6 | 390,873 | 7,030 | 65 | 57 | 2,663 | 217 |
| 1000×6 | 9,542,841 | 179,598 | 199 | 1,343 | 65,401 | 217 |
| 600×14 | 5,826,996 | 109,843 | 182 | 968 | 42,007 | 218 |
| 3000×10 | 28,747,593 | 537,966 | 646 | 4,613 | 205,210 | 218 |
| 2000×16 | 19,198,683 | 360,842 | 513 | 3,097 | 137,561 | 218 |
| 6000×8 | 57,276,849 | 1,076,816 | 1,305 | 8,902 | 405,443 | 218 |

각 구성은 literal 29 상태, 고정 경로 7개와 손상 입력 69,132건을 포함한다. 나머지 판정 edge는 Rejected와
NoEffect이며 원 로그와 per-(API, outcome, class) coverage는 로컬
`artifacts/dev4-lifecycle-assembler-20261025/checker-final-r2/`에 있다. 같은 시험을 인용 주석 정정 전
source로 실행한 `checker-final-r1`도 같은 ELF SHA로 모두 통과했다.

## 정적 검사와 범위

`cargo fmt --all --check`와 `cargo clippy -p xadbtest-reference --all-targets --all-features --offline --locked
-- -D warnings`가 통과했다. 문서 검사는 push gate 전용이다. 기존 codec·raw/selector·조립기·P1–P4·DEV3·COMM
시험은 실행하지 않았다.

walk는 결정적 smoke 탐색이다. 정상 상태 공간 완주·alias·FE14 known-bad 탐색이 아니며 불일치0은 방문한
상태와 입력에 대한 주장이다.

## 남은 의무

alias, BFS·checkpoint(기본 보존 위치 `artifacts/dev4-lifecycle-bfs/`, 약 10GiB), FE14 완주와 mutant 공간
탐색이 남는다. 실제 SQL/engine 연결과 DEV4/Q1은 DEV3 공급 뒤의 별도 의무다.

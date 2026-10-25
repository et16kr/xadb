# DEV4 P5 BFS 탐색기의 설계 상한·깊이 5 실행

2026-10-25 · **탐색기 선택 시험 PASS / 설계 상한 INCONCLUSIVE / 깊이 5 불일치0·DrainedUnarchived / 독립 D2 변경 후 수용 / archive·FE14 미완료**.

[소유 계약](../contracts/dev4-lifecycle-explorer.md) D4-LM-BFS-001의 `explore`를 controller에 구현하고 설계 상한
profile과 사용자가 승인한 깊이 5 profile을 메모리 안에서 실행했다. 기준은 `dev4-independent`의 checker commit
`8830632`이며 별도 worktree `/home/et16/work/xadb-dev4`에서 작업했다. 다른 checkout의 DEV3 작업은 읽거나 바꾸지
않았고 원문은 그 checkout의 `artifacts/dev4-lifecycle-model-20261024/`에서 읽기만 했다. 실행 산출물은 로컬
`artifacts/dev4-lifecycle-bfs/`에 있다.

## 상태 공간 측정과 범위 결정

탐색기를 만들기 전 모델만 쓰는 임시 측정(commit하지 않음, full canonical bytes 중복 제거)으로 깊이 0–4의 고유
상태가 1·24·606·15,674·414,818개임을 쟀다. 깊이마다 약 26배라 깊이 5는 약 1,100만, 깊이 6은 약 2.9억 개로
추정했고, 깊이 6 전체를 bytes로 저장하려면 약 350GB가 필요해 이 컴퓨터(여유 122GB)에서 불가능하다고 보고했다.
사용자는 21:36 KST에 "깊이 5 정확 비교"(예상 1–2시간·12GB)를 골랐고(답 "2") 거기까지만 하고 멈추라고 했다.
cap 수치는 owner가 그 예상에 맞춰 정했다. 아래 탐색기 실행의 깊이 0–4 고유 상태 수는 측정값과 같다.

## 검토

| 검토 원문 | SHA-256 |
| --- | --- |
| `review-explorer-contract-d2-v1.json` (D2 ACCEPT_WITH_CHANGES, blocker 1) | `69b98050ab23bdb9effd121cda9ca6de3bc5305ed4c3755904837d4099bee7b9` |
| `review-explorer-contract-closure-v1.json` (D2 ACCEPT_WITH_CHANGES, blocker 0) | `85914917096905582fd20037e284cbb4e1e4e4c7b90182040a63969d09b36cb8` |

1차 D2의 blocker는 원문이 전체 BFS에 요구하는 10GiB 영속 archive를 계약이 다루지 않은 점이었다. 메모리 안
실행은 archive가 없으므로 FIFO가 비어도 `DrainedUnarchived`이고 PASS가 아니라는 owner 결정으로 닫았다. 그 밖에
alias 문구, 조립 입력의 Envelope 제외·응답 없는 edge의 FAIL, worker allowance와 peak RSS, descriptor header의
뜻, edge8·discovery32 독립 vector 시험, 반례의 post·경로, rule별 counter, attempt 기록, Markov 근거를 반영했다.
종결 확인의 남은 minor·부분 항목(일부 FAIL 경로와 hash 충돌의 시험 부재, allowance가 상한이 아닌 추정임)은 계약에
그대로 적었다.

## 1차 실행과 판정

`run-bfs.py run-r1`(ELF `08791308…`, 16 worker)은 선택 시험 10개를 통과했다.

| profile | 결과 | 상태 | 분류 slot | 시간 |
| --- | --- | ---: | ---: | ---: |
| 설계 상한 | INCONCLUSIVE, state cap 500,000 | 500,000 | 28,756,073 | 92.9초 |
| 깊이 5 | FAIL, 깊이 4 상태 29,489의 raw 1073 | 810,899 | 45,207,710 | 157.3초 |

깊이 5 반례는 CloseResult 진입 취소 거절이었다. caller가 끝난 implicit ReadOnly 거래의 Eof result를 가진 채
새 명시 거래를 열었을 때, 모델은 응답의 거래 결정 필드를 caller의 현재 거래에서, checker는 인증한 result의
descriptor 거래에서 읽었다. 인증 참조와 같은 선택 거래(wire-v2 §4)에서 읽는 쪽으로 판정해 모델을 고치고 모델 계약의
[차등 판정](../contracts/dev4-lifecycle-model.md#checker-adjudication) 열두째 항목에 남겼다. 같은 상태를 literal
`sel.eof`에서 Begin 한 번으로 만드는 고정 경로를 `differential_tests`에 더했고, 교정을 되돌린 변형에서 이 경로가
CloseResult·CloseStatement 거절 4종을 보고하는 것을 확인했다. 1차 binary의 보고서는 rule별 제외 수와 반례의
post·경로를 쓰지 않아 그 값은 1차 결과에서 인용하지 않는다.

| 1차 원문 | SHA-256 |
| --- | --- |
| `run-r1/receipt.json` | `bb3b00688417bc05bbc355a01becf201531c0b219071b9fb28b24d54daa4eb1b` |
| `run-r1/report-normal.json` | `8347f64495ecbb1cd961bf7e07d67db619de6e8be5546d4613554bf206529761` |
| `run-r1/report-depth5.json` | `e74fcdb9e6b7fe31fa838e46228f5565af2f0fae00796cb86e317f8ae50115aa` |

## 최종 실행

`run-bfs.py run-r2`는 모델·차등·탐색기 source와 독립 initial vector manifest를 보존하고 reference와 controller
시험을 각각 한 번 compile한 뒤 보존 ELF로 실행했다(16 worker). profile마다 실행 전 attempt 기록을 남겼다.

| 항목 | 값 |
| --- | --- |
| 기준 HEAD | `8830632ea8504f1f77ab981d6cb84f642365688d` + 이 단위의 미commit 변경 |
| source·fixture manifest SHA | `6052ed38d913dcf8805ee0d9542a092cc4de9e0796a89bb9f96abe9017a1cd7b` |
| controller ELF SHA | `a3ac13fb43345df3fcdd57493702d9e649d0e0b49708424ad496ab2ed483d4cd` |
| receipt SHA | `e72e15a0828aa8451dc1c871f9d9f079c77f5dea555f5aabda6755502378ad3d` |
| ledger descriptor SHA | `7604cdc56f244701b2845d03065eb6c83653d1b030b51ebf1944734bee07cf78` |
| 실행 전·후 source | 동일 |

| 시험 | 결과 |
| --- | --- |
| 탐색기 선택 시험 10 | PASS(vector·결정성·alias·cap 넷·FAIL 원인·mutant 반례·edge16) |
| `model_tests` 6, `differential_literal_states_and_walks` | PASS(교정한 모델의 literal·trace·census·walk200, 차등 불일치0) |

| profile | 설계 상한 | 깊이 5 |
| --- | --- | --- |
| 결과 | INCONCLUSIVE, state cap | DrainedUnarchived(불일치0) |
| 상태(깊이 0/1/2/3/4/5) | 1/24/606/15,674/414,818/68,877 | 1/24/606/15,674/414,818/11,224,592 |
| 펼친 상태 | 깊이 0–3 전부, 깊이 4 중 2,453 | 깊이 0–4 전부 431,123 |
| 분류 slot | 28,756,073 | 660,911,559 = 1533 × 431,123 |
| success / rejection | 605,921 / 20,146,096 | 14,560,658 / 470,138,094 |
| alias / 제외 | 7,764,420 / 239,636 | 170,548,308 / 5,664,499 |
| checker 호출 | 20,991,653 | 490,363,251 |
| ledger | edge8 `8e8a871b…7d22` | edge16 `b8f2f76c…1734` |
| discovery SHA | `6d4eb03c…9c87` | `fa4c3beb…ebe0` |
| 시간 / 소유 용량 peak / RSS peak | 89.6초 / 1.10GB / 0.51GB | 2,445초 / 13.9GB / 13.2GB |
| 멈춘 frontier | head 18,758 = 16,305 + 2,453, 다음 slot 59 | FIFO 소진 |

깊이 5 실행은 깊이 4 이하 모든 상태의 모든 raw slot을 분류했고 alias가 아닌 4억9천만 slot마다 모델과 checker를
한 번씩 불렀으며 불일치·모델 오류·조립 오류가 없었다. 깊이 5 상태 1,122만 개는 발견 때 codec 검산을 통과했다.
rule별 제외 수와 전체 hash·API별 counter는 보고서에 있다. 설계 상한의 INCONCLUSIVE는 그대로 보존하며 깊이 5
결과는 archive가 없어 PASS가 아니고, 길이 6 경로를 검산하지 않았으므로 FE14는 INCONCLUSIVE다.

| 최종 원문 | SHA-256 |
| --- | --- |
| `run-r2/report-normal.json` | `24b0c0023da5989b545351359c6543b928c796c408d4157863722b668897e4ac` |
| `run-r2/report-depth5.json` | `ef79ffa842bf067d285cf4013f87c7a667692d856c156799dc51c052d28ea122` |

## 정적 검사와 범위

`cargo fmt --all --check`와 `cargo clippy -p xadbtest -p xadbtest-reference --all-targets --all-features --offline
--locked -- -D warnings`가 통과했다. 문서 검사는 push gate 전용이다. 시험은 변경한 탐색기와 교정한 모델에 대응하는
함수만 실행했고 조립기·codec·raw/selector·checker·P1–P4·DEV3·COMM 시험은 실행하지 않았다. binary는 cargo test
기본(최적화 없는) 빌드다.

## 남은 의무

archive와 durable checkpoint(원 checkpoint wire 구현, 10GiB 영속 공간), witness 도달 검사, FE14 mutant 공간 탐색,
깊이 6 완주가 남는다. 깊이 6은 이번 승인 범위 밖이다.

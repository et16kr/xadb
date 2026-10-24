# DEV4 P5 전체 입력 조립의 설계·독립 입력 준비

2026-10-25 · **정적 입력1658개 한정 검토 / 준비 묶음 최종 D1 제한 수용 / 전체 D2·제품·시험 NOT_RUN**.

[선행 계획](../plans/dev4-preparation-plan.md)의 P5를 이어 전체 `ResolvedInput` 조립을 준비했다.
기준은 [원시 action·selector](dev4-lifecycle-raw-selection-20261025.md)의
`1dd7bed1fc03bead2158fba439a06673dff29158`이다. 사용자의 “적당한 단계에서 정리하고 commit”
요청에 따라 현재 단위는 설계·독립 입력 준비로 정리한다. `assemble_input`과 Rust 시험 harness는
구현하지 않았으며 이 기록은 전체 resolver·모델·FE14의 완료나 구현 GO가 아니다.

`/root`는 계약·오류/상한 입력·통합, `/root/frontend_research`는 설계/read-set,
`/root/frontend_oracle`는 정상 literal·중복 대응, `/root/frontend_review`는 독립 검토를 맡았다.
동시 DEV3/COMM 변경과 원 source별 완료 시험은 그대로 보존한다.

## 이번에 보존하는 범위

[fixture manifest](../../tests/fixtures/dev4-lifecycle-assembly/manifest.json)의 SHA는
`6770e07b27462fcd99c06760c78482317cd164ab11822a4f708e433a2611d33d`다.
원 artifact11파일을 byte-identical로 보존하고 새 manifest·README를 더했다. Python 작성 원본은
당시 artifact 배치의 보존 자료이며 복사 위치의 제품 실행 진입점이 아니다.

| 독립 입력 후보 | 개수 | 확인한 범위 |
| --- | ---: | --- |
| 초기 상태 전체 raw |1533|21 family의 prefix/control/target·Bind·Env scalar와 empty census |
| 원 literal의 추가 고유 쌍 |27|원29쌍+U9 중 초기 중복3 제외; 원51위치·7 byte vector 연결 유지 |
| Bind N0/N1 |22|고정 payload, Success/entry의 parameter count 조건 |
| 오류·상한 |76|성공 기대34·오류 기대42; read-set71개와 단계 우선순위·사용 counter 경계 |
| 합계 |1658|정적 대조이며 제품 호출·실행 PASS가 아님 |

오류 입력은 InvalidAction의 새 S0 우선 witness2개와 selector→관계→counter의 복합 경계를
다룬다. 앞서 제안한 기존 raw 오류123개·단순 selector wrapping12개는 새 상위 기능을 이유로
완료 하위 시험을 반복하지 않도록 제외했다. 이들은 원 제안에서 NOT_RUN으로 보존하며 실패 뒤
기준을 줄인 사례가 아니다. 아직 제품·시험 코드와 실행이 없었다.

상한 입력은8개 namespace의 L−1/L/L+1·max-next 구조·미사용 소진, Resource6 block의
마지막53/54/55, Result generation의 retirement 증가를 구분한다. sparse history와 깨진 참조는
필요한 국소 read-set만 검산하는 입력이며 전체 State 유효성·모델 도달성을 주장하지 않는다.
owners54/O7은 container 한도다. 정상 최대 출력이나 Capacity 도달성의 실행 증거는 없다.

## 설계 검토와 보완

원 제안·owner scope·read-set·검토는 `artifacts/dev4-lifecycle-model-20261024/`에 보존했다.
선택 API 후보는 caller out을 받는 `assemble_input(&State, &RawAction, &mut ResolvedInput)`이다.
오류 out은 부분·무효이며 성공 뒤 모든 scalar/options/count를 교체한다. State는 불변이다.
raw 검산→selector/caller→필요 참조와 symbolic birth 계획→모든 사용 namespace 구조→후보 cap→
출력 순서를 정리했다. 새 전체 State validator나 admission/model 실행을 조립기에 넣지 않는다.

정적 검토 중 세 누락을 구현 전에 보완했다. 신규 Result는 current token의 예정 retirement1을
generation에 더하며 이미 retired면 더하지 않는다. CommitFact NoWrite의 receipt 유무3/4 모두
RW를 해제한다. 기존 implicit RW의 rejected Execute Success는 RW3만 해제하고, CommitFact
Published의 별도 RW∪Recovery drain과 구분한다. 원 read-set v1/v2와 v3 교정은 보존했다.

Obligation은 포함 조건과 decision 출처를 분리했다. 원인 갱신은 새 사실, 신규 peer는 T.decision,
기존 peer/handoff/recovery는 기존 O 지식, Published drain은 Committed를 공급한다. 변화 없는
기존 peer O는 출력에서 생략하며 cause-only 갱신의 count0/None은 별도다. 과거 client 응답을
소급 변경하는 의미가 아니다. 상세 선택은 최종 전체 D2에서 실제 모든 입력과 함께 다시 고정한다.

| 한정 검토 원본 JSON | SHA-256 |
| --- | --- |
| `review-assembler-design-v2.json` | `bfe819dfad825a4cd62693dd134b568564c2b8a652679d16e73b451ea7e29b9c` |
| `review-assembler-normal-inputs-v1.json` | `f31294500a9cfa8914b3e53e42ca9f302e91eb7d7247573f2d52447833605e3f` |
| `review-assembler-boundaries-v1.json` | `7e8a9c91ca3a5ad275d10821111786302dbedee25983c36a8c749f55155521aa` |

정상1582와 boundary76의 정적 대조에서 불일치를 찾지 못했다. 설계 검토는 literal 작성 적합성,
입력 검토는 해당 후보 보존 범위에 한정한다. 최종 준비 D1은 실제21경로·원복사11개·1658ID와
미검토304 제외·공유 문서의 DEV4-only 변경을 대조해 제한 수용했다. 원본
`review-assembler-preparation-d1-v1.json` SHA는
`ea35c7473a1e4d9931480a6acdd3a45d930eccc739f704f9db1c5abd7d8f49ee`다.
후속 수정은 이 상태·receipt 연결뿐이며 fixture·기대값·분모는 그대로다.
기존 [로컬 PostgreSQL·SQLite 조사](dev4-lifecycle-inputs-20261024.md)의 원 revision/근거를
참조했으며 이번에 외부 checkout을 수정·빌드·시험하지 않았다.

## 원본 보존과 다음 단계

정상 후보 전체는1886개이며 그중 rich97+77+130=304개가 미검토·NOT_FROZEN이다.
`assembler-independent-inputs-v1/current-snapshot.json` SHA
`321d1ef167ab98fd6017fb7ebaf772497416c6700d34b6aed6fa9501c0991e06`에 정확 파일·ID·후속
범위를 남겼다. 이304개는 이번 tracked subset에 포함하지 않는다. boundary 검토도 receipt·
추가 primitive/duplicate 경계를 모두 닫은 최종 inventory가 아니다.

다음은 미검토 rich와 추가 국소 경계·known-bad 관측을 검토하고 전체 manifest/선택 harness를
동결하는 일이다. 최종 D2 뒤 조립기를 구현하고 해당 새 코드만 선택 검증한다. 모델·독립 edge
checker·환경 소비·alias·원51action replay·BFS/재개는 이어지는 별도 의무다.

이번 단위는 정적 authoring과 읽기 대조만 수행했다. 제품/Rust 시험 변경0, compile/runtime/test0,
완료 무변경 시험 반복0, 문서 verifier·push0이다. 전체 BFS의10GiB 영속 보존 공간 요구는
미충족이며 사용자에게 사용 가능한 다른 영속 경로를 문의했다. 준비 단위의 commit으로
전체 P5/FE14·실제 engine·DEV4/Q1을 완료 처리하지 않는다.

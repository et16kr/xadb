# DEV4 P5 수명 모델 전이의 선택 검증

2026-10-25 · **신규 모델 선택 시험 PASS / 독립 검토 D2 수용·D1 제한 수용 / 독립 checker·BFS·FE14 미완료**.

[소유 계약](../contracts/dev4-lifecycle-model.md) D4-LM-MODEL-001의 `transition`만 구현했다. 기준은
`dev4-independent`의 조립기 commit `4546766`이며 별도 worktree `/home/et16/work/xadb-dev4`에서 작업했다.
다른 checkout의 DEV3 작업은 읽거나 바꾸지 않았고 준비 원자료는 그 checkout의 `artifacts/`에서 읽기만 했다.

## 역할과 기대값

계약·구현은 이번 작업 세션이, 독립 D2/D1은 구현에 참여하지 않은 별도 검토자가 맡았다. 독립 기대값은
기존 손 literal `literals.json`(SHA `ec4a7940a457882c155eb84e2d9e5602eaf880563bab4f6f04d0b643bcec42d2`)의
29 edge·12 trace·5 known-bad다. literal 밖 family의 고정 trace 단언은 구현 측이 원문에서 손으로 유도했으므로
독립 기대값이 아니며, 후속 독립 checker가 대신할 의무다. 모델 출력으로 기대값을 만들지 않았다.

| 검토 원문 | SHA-256 |
| --- | --- |
| `review-model-d2-d1-v1.json` (D2 ACCEPT_WITH_CHANGES, D1 REJECT) | `efd70f07ec1eb36fb728fbfca46a04e63dc92a03444b7b0fab1f86433eeab1ff` |
| `review-model-closure-v1.json` (D2 ACCEPT, D1 ACCEPT_LIMITED) | `6d634f94576e9902706c496c56cd701cb9ab72b73d14e731f52f04240077d722` |

## 개발 중 결과와 교정

첫 구현은 literal 29 edge·12 trace(51 occurrence)·5 known-bad를 모두 통과했다. literal 29 상태 × raw1533
census에서 CloseStatement 뒤 slot binding 연결과 Error가 아닌 descriptor의 terminal payload가 loader 검산을
위반해 고쳤다. 결정적 walk에서 두 actor census의 birth row가 key 순서로 공급될 때 nonce 순서를 요구하던
기록 오류를 찾아 연속 nonce block 검산으로 고쳤다.

1차 독립 검토는 원문이 금지한 전이 두 건을 찾았다. 이미 Fenced인 engine의 CloseSession Incomplete가 알려진
commit 결과를 NotCommittedRecoveryRequired로 덮었고(v3 ENV-FENCED-CLOSE, SQL close_session 규칙), peer fence
아래 parked BeforeAttempt의 Finish Aborted를 받아들였다(owner v5 §2). 둘 다 제외로 바꾸고 고정 trace 회귀
시험을 더했다. 그 밖에 envelope 불일치를 모델 오류 대신 `Excluded(Envelope)`로, 소비 bit를 한 규칙으로,
CommitFact Attempted의 decision을 Unknown으로 정리하고 Env1 kind·transaction·recovery owner 집합 검산을 더했다.
재검토는 D2 수용·D1 제한 수용이며 minor 넷을 남겼다. 그 뒤 계약의 rows 문구 중복(C-01)과 남은 envelope 오류
분류(C-02)를 고치고, cached·멱등 close·Execute Conflict·Fetch Incomplete·recovery 집합 부정·Finish Unknown/
PublicationPending·DurableConfirmed peer drain 고정 trace와 각 후상태 loader 검산을 더했다(C-03 일부). 이
마지막 변경은 재검토를 다시 받지 않았고 아래 선택 실행으로만 확인했다.

## 최종 선택 실행

`run-model.py model-final-r1`은 source·literal manifest를 보존하고 reference 시험을 한 번 compile한 뒤
보존 ELF에서 새 모델 시험 함수만 `--exact --nocapture --test-threads=1`로 실행했다. walk 수는 200이다.

| 항목 | 값 |
| --- | --- |
| 기준 HEAD | `4546766bd3e6992c62de2878e795225523a5dcf5` + 이 단위의 미commit 변경 |
| source·literal manifest SHA | `cc6ff6fd9cac96d34e18f2dd92e30a96e0828fbd2133893c6380ea4d24aa40c2` |
| 보존 ELF SHA | `1aefc0af6891512c85a9996ec81a873b77923bcd3bf9d8ec1b0d272343edecb6` |
| receipt SHA | `07dbe5aabd36a5c3f2efb7a1a49d95fc9283b72c475555f49032b01b7d4ab2b9` |
| 실행 전·후 source | 동일 |

| 시험 함수 | 범위 | 결과 |
| --- | --- | --- |
| `model_literal_edges` | 29 edge의 response·edge 관측·후상태와 loader | PASS(COMPLETED 29) |
| `model_literal_traces` | 12 trace 51 occurrence를 모델 후상태로 연쇄 | PASS(COMPLETED 51) |
| `model_known_bad_differs` | 5 known-bad의 bad response/post와 다른 관측 | PASS, 5개 모두 response 차이 |
| `model_fixed_traces` | literal 밖 family의 고정 trace 단언과 각 후상태 loader | PASS |
| `model_raw_census` | literal 29 상태 × raw1533 | PASS, 모델 오류·loader 위반0 |
| `model_deep_walks` | 결정적 walk200 × 깊이6, 방문 상태마다 raw1533 | PASS, 모델 오류·loader 위반0 |

census는 조립 성공 41,685건과 조립 ProfileLimit 2,772건이며 class 분포와 원 로그는 로컬
`artifacts/dev4-lifecycle-assembler-20261025/model-final-r1/`에 있다.

## 정적 검사와 범위

`cargo fmt --all --check`와 `cargo clippy -p xadbtest-reference --all-targets --all-features --offline --locked
-- -D warnings`가 통과했다. 문서 검사는 push gate 전용이다. heap·재귀·State clone 없음은 제품 경로의 정적
근거다. 기존 codec·raw/selector·조립기·P1–P4·DEV3·COMM 시험은 실행하지 않았다.

census와 walk는 모델 오류 없음과 후상태의 loader 유효성만 보는 smoke 검산이다. literal 밖 응답·후상태의
정답 여부를 판정하지 않으며 정상 공간 완주나 FE14 known-bad 탐색이 아니다.

## 남은 의무

독립 edge checker(`verify_edge`)와 그 known-bad, full input alias, BFS·checkpoint(기본 보존 위치
`artifacts/dev4-lifecycle-bfs/`, 약 10GiB), FE14 완주와 mutant 공간 탐색이 남는다. 실제 SQL/engine 연결과
DEV4/Q1은 DEV3 공급 뒤의 별도 의무다.

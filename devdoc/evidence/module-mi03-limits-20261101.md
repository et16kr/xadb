# MI03 남은 한계 8개: 내용, 시도 방법, 해결 여부

2026-11-01

원 S11 실행은 `25044995` 기준이다. PR #16 리뷰의 후속 수정·선택 검증은 [아래 별도 기록](#pr16-review-followup)에 둔다. 원 receipts를 새 실행 결과로 덮어쓰지 않는다.

8개 가운데 7개를 해결했고(PASS), MCB03-V01 하나는 해결하지 못했다(PARTIAL). 사용자는 그 한계를 기록하고 M5를 완료로 판정하라고 골랐다.

S10이 끝났을 때 [소비자 §9](../contracts/dev3-module-consumers.md#acceptance) 동결 inventory 129 ID 가운데 8 ID가 PASS가 아니었다. 결정 카드에는 7개로 올렸고, 그 뒤 기록 재확인 R11이 MCA05-V32를 PARTIAL로 낮춰 8개가 되었다. S11은 이 8개를 C의 시험 전용(cfg(test)) 경로로 다시 공급했다. 제품 동작은 바꾸지 않았다. A의 판정을 같은 동작의 함수로 뺀 것 하나는 사용자가 허락했다. 계약은 [S11 절](../contracts/dev3-module-boundaries.md#row-mi03-s11)이고 실행과 판정은 [S11 증거](module-independence-survey-20261026.md#s11-mi03-resupply)다.

## 한눈에 보기

| ID | S10 판정 | 한계 | S11 시도 | 해결 |
| --- | --- | --- | --- | --- |
| MCA01-V12-P | PARTIAL | A의 Index 거절 판정을 시험이 부를 수 없었다 | A 판정을 순수 함수로 빼고 C의 census 결과를 넣었다 | 해결 (PASS, C0 입력) |
| MCA05-V01 | PARTIAL | WAL 몫을 정확히 1 모자라게 만들 수 없었다 | 흐름 전체의 WAL 몫을 C의 실제 admission까지 줄이는 guard | 해결 (PASS) |
| MCA05-V02 | NOT_RUN | 256 MiB WAL을 거의 채워야 해 30초 안에 불가 | 같은 guard, 몫 = 필요 | 해결 (PASS) |
| MCA05-V03 | NOT_RUN | V02와 같다 | 같은 guard, 몫 = 필요 + 1 | 해결 (PASS) |
| MCA05-V29 | PARTIAL | count 0인 H를 실제 C 상태로 만들 수 없었다 | A에 빌려주는 before image 사본만 바꾸는 주입기 | 해결 (PASS, 한계 기록) |
| MCA05-V32 | PARTIAL | 인용 시험이 cancel과 finish만 보고 drop은 안 봤다 | drop 경로의 새 시험, 인용 시험 재실행 | 해결 (PASS) |
| MCB03-V01 | PARTIAL | 동결 순서에 live로 닿지 않는다 | 시험이 만든 receipt로 registry를 직접 부름 | **미해결 (PARTIAL)** |
| MCB03-V08 | PARTIAL | 같은 owner의 셋째 claim을 B로 낼 수 없었다 | capture 지점의 C 시험 probe가 claim | 해결 (PASS, 한계 기록) |

129 ID 전체는 PASS 108(실제 C 97, C0 입력 6, 인용 5), PARTIAL 1, N/A 20이다. S10은 PASS 101, PARTIAL 6, NOT_RUN 2, N/A 20이었다.

## MCA01-V12-P — Index가 있는 table의 profile 거절

해결했다(PASS, C0 입력). C의 census가 센 Index 수가 A의 실제 판정 함수를 지나 Unsupported가 된다.

- **요구:** Catalog census가 table T의 Index를 세면 A의 profile 판정이 INSERT를 Unsupported로 거절해야 한다. 동결 문언은 "Index를 허용하는 mutant는 실패해야 한다"이다.
- **S10 한계:** C의 census가 Index 수를 바르게 세는 것까지만 보였다. A의 한 줄 판정(`index_count != 0`)은 C의 preflight가 만든 context 안에서만 불리고, 계약 [§8.4](../contracts/dev3-module-consumers.md#index-negative-input)가 context factory를 금지한다. 그래서 그 판정을 지운 mutant도 통과했다.
- **S11 시도:** 사용자 허락("A 판정을 함수로 분리")으로 A의 `schema` 판정을 같은 조건의 순수 함수 `table_only_profile`로 뺐다. C 시험의 census 결과(parent가 T인 Index 1개, 다른 parent 0, Index 없음 0)를 A 시험이 그 함수에 넣는다.
- **결과:** parent가 T이면 Unsupported, 나머지는 Ok다. 판정에서 `index_count != 0`을 지운 mutant는 이 시험에서 실패했다.
- **남은 한계:** `schema`가 그 함수에 실제 context 값을 넘긴다는 것은 코드를 읽어 확인했다. factory가 없으므로 실행으로 보이지는 않는다.

## MCA05-V01, V02, V03 — WAL 몫 최소 필요 −1, =, +1

셋 모두 해결했다(PASS). 최소 필요는 RowReserve의 admission 1,040,288 B이고, −1은 효과 전에 거절되고 =와 +1은 끝까지 성공한다.

- **요구:** INSERT에 필요한 WAL 몫보다 1 적으면 효과 전에 거절하고, 같거나 1 많으면 끝까지 성공해야 한다.
- **S10 한계:** C는 WAL 파일 cap 256 MiB에 대해서만 비교한다. 정확히 1 모자란 상태를 만들려면 파일을 거의 다 채워야 해서 30초 한도 안에서 불가능했다. V01은 거절 분기를 강제로 한 번 타게 해 PARTIAL, V02와 V03은 NOT_RUN이었다.
- **S11 시도:** WAL 파일에 남은 몫을 시험 thread에서만 줄이는 guard를 두었다. 원래 비교식은 그대로다.
    - 처음 공급(`a81f8975`)은 physical stage 한 곳만 줄이고 필요를 RowReserve frame 57,336 B로 잡았다.
    - 독립 검토(R1)가 그것이 틀렸음을 찾았다. C는 stage 전에 Row WAL admission(`check_progress_room`)에서 frame과 앞으로 갚아야 할 모든 WAL을 합해 본다. 실제 필요는 57,336 + 982,952 = 1,040,288 B다. 57,336 B 몫이면 실제 C는 이미 거절하므로 V02와 V03의 기대가 틀렸다.
    - 고친 guard(`25044995`)는 흐름 전체에서 admission과 stage를 함께 줄인다. 필요는 같은 fixture를 제한 없이 돌려 C의 admission 기록에서 잰다.
- **결과:**
    - −1: 효과 전 Unchanged (Prepare, Capacity)로 거절된다. 파일은 byte 동일이고 제한을 풀면 다시 성공한다.
    - =, +1: Reserve, Forward, 실패 statement와 rollback, full abort가 모두 몫 안에서 끝난다. admission 7번, stage 7번 모두 거절이 없다.
    - admission 비교를 `>=`로 바꾼 mutant는 V02에서 실패했다.
- **남은 한계:** 256 MiB 파일을 실제로 채운 상태는 만들지 않는다. C의 다른 WAL cap 비교 다섯 곳은 줄이지 않지만, 임시 probe로 이 흐름이 거기에 닿지 않음을 확인했다.

## MCA05-V29 — before count 0인 H를 A가 거절

해결했다(PASS, 한계 기록). 실제 C 위에서 A가 출력 전에 거절하고 driver가 예약을 취소한다.

- **요구:** H(다음 Forward가 옮길 Undo)의 before count가 0이면 A가 출력 전에 거절해야 한다(실제 C 위에서).
- **S10 한계:** U의 count는 C만 WAL 아래에서 쓰므로, 이력과 맞으면서 count가 0인 C 상태는 만들 수 없다. frame만 바꾸면 C의 preflight가 먼저 fence한다. 그래서 A의 거절이 실제 C에서 나는 경로가 없었다.
- **S11 시도:** C가 A의 producer에 빌려주는 before image 9장을 시험에서만 사본으로 바꾸고, 그 사본의 count를 0으로 고쳐 다시 seal했다. loan, pool, WAL과 C의 검산은 원본을 본다.
- **결과:** A가 (Produce, OrderMismatch)로 출력 전에 거절하고 driver가 C로 예약을 Cancel한다. 새 frame은 RowReserve(57,336 B)와 Cancel(40,000 B, author literal과 같음)뿐이고 Forward는 없다. Commit이 q 2로 앞 행만 공개한다.
- **남은 한계:** count 0인 H는 A가 읽는 사본에만 있다.

## MCA05-V32 — API drop/cancel의 권한 반환

해결했다(PASS). drop 절반은 새 시험이 보고, cancel 절반은 인용 시험을 같은 commit에서 다시 실행했다.

- **요구:** preflight나 예약을 drop 또는 cancel하면 charge, gate, lease를 돌려줘야 한다.
- **S10 한계:** 인용 시험(S2W-R15, R10, R03)은 cancel과 finish만 보고 drop은 실행하지 않았다(기록 재확인 R11).
- **S11 시도:** Ready preflight를 finish 없이 drop하는 새 시험을 더했다. 활성 예약 drop의 fence는 S2W-R23(2)을 인용하고, 검토 R4에 따라 그 시험도 선택에 넣어 다시 실행했다.
- **결과:** drop이 charge(44,304), gate, lease, loan을 돌려주고 효과와 fence가 없다. 다음 preflight가 같은 loan으로 다시 Ready가 되어 끝난다.

## MCB03-V01 — B의 capture 뒤 retirement가 Busy

해결하지 못했다(PARTIAL). 동결된 순서에 live로 닿는 경로가 없고, 시험용 권한 factory는 계약이 금지한다.

- **요구:** B 읽기가 source capture로 Txn과 U의 pin을 쥔 뒤, 그 U의 retirement가 pin 수 때문에 Busy로 거절되어야 한다.
- **S10 한계:** 이 순서에 live로 닿는 retirement가 없다. rollback CLR의 retirement는 S2-0 gate에서 B 읽기를 기다리고(MCB03-V02), UndoFree retirement는 live Row retirement라 사용자가 범위 밖으로 정했다. S10은 반대 순서만 보였다.
- **S11 시도:** B1을 capture 뒤에서 멈추고, 시험이 직접 만든 `VerifiedRetirement`로 registry의 원 `begin_retirement`를 불렀다(gate를 거치지 않음).
- **결과:** pin을 쥔 동안 Busy, Unchanged이고, B1이 pin을 돌려준 뒤 같은 시도는 받아들여진다. registry의 pin census가 거절한다는 것은 보였다.
- **해결하지 못한 이유:** 독립 검토(R2)가 지적했다.
    - `VerifiedRetirement`는 실제 semantic Before만 발급하는 권한 type이다. 소비자 계약 §8과 §8.4가 시험용 authority factory를 금지한다.
    - S10도 같은 이유로 V12-P를 factory 없이 PARTIAL로 두었다.
    - 은퇴시키는 U는 아직 Active인 거래의 것이라 실제 retirement는 이 모양이 될 수 없다.
- **판정:** PARTIAL. 새 시험은 보조 증거로 남긴다. 2026-11-01 사용자는 "한계 기록하고 M5 완료"를 골랐다.
- **해결하려면:** live Row retirement(DEV3)가 생겨 실제 semantic Before가 이 순서로 receipt를 낼 수 있어야 한다. 이것만으로 충분하지는 않다. 현재 gate는 읽기 종료까지 기다리므로, 정상 동시성 경로가 해당 pin 충돌 순서를 허용하는지도 함께 검토해야 한다. [후속 계약](../contracts/dev3-module-boundaries.md#row-mi03-s11-review)은 통합 대기와 registry 방어의 증거를 구분하며 PARTIAL을 유지한다.

## MCB03-V08 — source pin 2개 뒤 추가 claim Busy

해결했다(PASS, 한계 기록). 추가 claim은 owner별 행 2개의 한도 때문에 Busy가 된다.

- **요구:** 읽기가 source pin 2개를 잡은 뒤 같은 owner의 추가 reference claim은 Busy여야 한다.
- **S10 한계:** B는 읽는 동안 `&mut Owner`를 빌리고 있어 같은 owner로 셋째 claim을 낼 수 없었다. gate가 닫힌 동안의 거절만 보였다.
- **S11 시도:** capture 지점(`RowReadCaptured` 바로 앞)에서 C의 시험 probe가 읽는 Owner로 원 `reserve_references`를 한 번 더 부른다.
- **결과:** owner의 행 2개가 쓰이고 있어 Busy, Unchanged이고 pin을 잡지 않는다.
- **남은 한계:** 추가 claim은 C의 시험 probe가 낸다. B의 API로는 낼 수 없다. probe는 권한을 만들지 않고, 거절하는 것은 동결된 owner별 한도다.

## 근거

최종 실행은 commit `25044995`의 고정 export에서 모두 통과했다(C+A 50/50, C+B 37/37과 11/11, native 전부, 전체 트리 25/25). 독립 검토는 조건부 ACCEPT(필수 R1–R4), 반영 재확인은 ACCEPT다.

- 계약: [S11 절](../contracts/dev3-module-boundaries.md#row-mi03-s11)
- 선택: [S11 addendum](../../tests/fixtures/module-row-integration/single-source-s11-addendum.json)
- 실행 기록: [S11 receipts](../../tests/fixtures/module-row-integration/single-source-s11-receipts.json)
- 증거와 M5 재판정: [S11 증거](module-independence-survey-20261026.md#s11-mi03-resupply), [M5 재판정](module-independence-survey-20261026.md#s11-m5)
- 검토 원문과 실행 artifacts: 공유 폴더 `module-independence-20261029/artifacts/module-s11-20261101/`

<a id="pr16-review-followup"></a>
## PR #16 리뷰 후속 수정

| 항목 | 기록 |
| --- | --- |
| 결정 | XADB-MI03-S11-REVIEW-001 · D2 · 2026-11-01 · ACCEPTED · 구현·선택 검증·독립 재확인 완료 |
| 담당·근거 | 구현·계약·통합 `/root`. 사용자가 PR #16 리뷰 내용을 수정해 그 branch를 대상으로 별도 PR을 만들도록 요청 |
| 기준 | `da435fd6a3c9085320c297b8e8229ec5f38f28f4`, branch `codex/pr16-review-followup`, PR base `claude/project-thread-1jfdtk-s11` |
| 문제 | WAL 경계 시험의 기대값이 제품 계산에서 나와 잘못된 계산을 따라갈 수 있음. before-image 주입이 매번 144 KiB를 leak함 |
| 선택 | [후속 계약](../contracts/dev3-module-boundaries.md#row-mi03-s11-review)의 독립 WAL 식을 사용하고 `reserve_need()` calibration을 제거. cfg(test) 예약이 Box 사본을 소유하고 lane이 대여 |
| 대안 | 관측값을 그대로 상수화하면 산출 독립성이 없음. 실제 256 MiB 채우기는 동일 admission 경계의 시험 비용만 증가. 전역 보관이나 leak은 수명 소유를 해결하지 못함. lane 자체가 Box를 소유하면 consuming `parts()`의 참조 반환 수명을 바꿔야 하므로 원 예약에 소유시킴 |
| 범위 | MCA05-V01..03 oracle와 MCA05-V29의 시험 주입 수명. cfg(test) carrier의 추가 포인터도 census에 포함. 제품 의미·형식·API·gate·charge·동결 inventory·원 PASS/PARTIAL 판정은 유지 |
| 재검토 | fixture 이력/row schema/계약 비용 변경, carrier 상계 초과, 주입 사본이 lane보다 먼저 해제되거나 예약 이후에도 남음, live Row retirement 공급 |

WAL 식은 이전 committed Catalog 정리 367228 + 새 Row transaction 248618 + 새 Row 정리 367106 = 미래982952B이며 RowReserve57336B를 더해1040288B다. `/root/pr16_independent_review`가 계약·fixture에서 별도로 같은 식을 도출했다. 제품의 계산 결과나 descriptor decoder를 oracle로 쓰지 않는다.

검증은 AGENTS.md의 변경 코드 범위로 제한한다. before-image 소유 변경에는 실제 V29 취소 경로와 RowWrite C01/C02/C06, A caller D15(memory/native)를 선택했다. WAL oracle에는 V01/V02/V03과 잘못된 필요량 mutant를 선택한다. 무변경 C+B·native 복구·전체 inventory를 반복하지 않는다. memory 한도30초를 유지한다.

[후속 receipts](../../tests/fixtures/module-row-integration/single-source-s11-review-receipts.json)는 각 실행의 source/ELF/input SHA-256, 정확한 selector, 원 출력과 해시, source 재구성 방법을 보존한다. 구현 commit은 `eeed242ef8f89c2e95e5e673afe2d654474fdce9`이며 이후 증거 정리는 제품·시험 source를 바꾸지 않는다.

| 선택 | 결과·의미 |
| --- | --- |
| 소유 사본 변경: V29, S2W-C01/C02/C06, A D15 memory/native | 6/6 PASS. V29 1.321초, census 각0.003–0.005초. WAL oracle 수정 전 실행이며 변경한 사본 코드의 hash는 최종 source와 같음 |
| 기존 V02 + RowReserve 미래 WAL +1 mutant | 통과(2.500초). 잘못된 미래982953·필요1040289를 기대값이 따라가는 검증 공백을 재현 |
| 보강 V02 + 동일 +1 mutant | 의도한 실패(exit101, 0.907초). 고정 필요1040288에서 Reserve/Capacity로 거절 |
| 보강 V02 + 미래 WAL −1 mutant | 의도한 실패(exit101, 1.474초). 실제982951과 독립 기대982952 불일치를 검출 |
| 오류 주입 제거, `eeed242e` C+A 단독 export의 V01/V02/V03 | 3/3 PASS, 각각1.105/1.502/1.486초. 원30초 한도 유지, B source/fixture 참조0 |

정상 선택9개 PASS와 부정 입력 검출2개를 구분한다. 기존 oracle가 잘못된 구현을 통과시킨 원 결과도 보존한다. mutant가 final source에 남지 않았음을 원 파일 diff로 확인했고, 전체 트리 실행5묶음의1259개 source hash는 commit과 명시한 변경으로 모두 재구성됨을 확인했다. cfg(test) Box 포인터를 포함한 기존 C·caller 상계는 증액 없이 통과했다. 사본 해제는 Box의 원 예약 소유와 borrow 수명으로 보장하며 allocator의 해제 횟수를 계측한 시험이라고 주장하지 않는다.

별도 검토자 `/root/pr16_independent_review`는 계약에서 WAL 식을 독립 도출하고 코드·6개 선택 결과·+1 반례를 읽기 전용 검토해 D1/D2 ACCEPT, 필수 수정0으로 판정했다. 검토자가 지적한 “세 owner” 표현은 하나의 Aging owner에 두 actor의 cleanup이 합산된다는 의미에 맞춰 “세 항목”으로 수정했다. 최종 재확인도 ACCEPT·필수 수정0이다. 검토자가 원 commit archive와 정확한 mutant 치환으로 전체 트리5묶음(각1259파일)과 C+A export(7698파일)의 source hash를 독립 재구성해 모두 일치함을 확인했다. 원 출력·선택 input의 해시와 정상3개·−1 반례를 대조했다. 재구성에 필요한 mutant의 공백·주석까지 receipts에 보존했다.

MCB03-V01은 실제 retirement 구현 및 동시성 경로 검토가 남아 PARTIAL이며, 이번 PR로 해결했다고 판정하지 않는다.

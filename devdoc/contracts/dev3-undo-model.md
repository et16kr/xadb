# DEV3 M01/M02 graph 모델의 두 번째 동결

2026-10-20 · **D3-M01-M02-F02 / r4 탐색 키 동결 / r3 M02 미완료**

[저장 계약](disk-storage-design.md#undo-lifecycle) · [유한 모델 의무](../plans/dev3-implementation-plan.md#state-model) · [병렬 소유](../implementation/dev3-implementation.md#wave-2)

이 profile은 기존 [D3-M02-P01](dev3-snapshot-model.md)의 source와 receipt를 대체하지 않는다. 새 source는 `crates/testing/reference/tests/dev3_undo_model.rs`다. M01과 M02는 같은 후보 graph 표현을 사용할 수 있으나 기대 논리 이력과 incoming-reference 재계산은 후보의 update/count 함수를 사용하지 않는다. 모델 PASS도 실제 thread·WAL/CLR·파일 I/O·제품 S1 인수와 구분한다.

## 의미 범위와 실제 사용

writer A/B, reader 하나, maintenance actor 하나를 둔다. 행0/1, 두 행의 versioned presence를 가진 index 하나, key `0`, `1`, `NULL`이 실제 이력에 등장한다. 초기 행은 `(0,1)`이고 A는 행0→1, 행1→NULL을 쓴다. A의 두 undo는 U0/U1이다. B는 행0→0을 U2로 쓴 뒤, `distinct`이면 행1→1, `same`이면 행0→NULL을 U3로 쓴다. A commit은 q11, B commit은 q12이며 초기 q10의 기대값은 항상 `(0,1)`이다.

4 undo slots는 모두 동시 Live가 되는 fixture가 있다. 각 slot은 generation0에서 Free·generation1 재사용으로 진행하며 더 이상 wrap/reuse하지 않는다. status의 물리 domain은 4 slots다. fixture별로 A/B의 서로 다른 두 slot을 순환 배치하여 네 slot의 생성·참조 제거·generation1 재사용을 검사한다. 동시에 거래 네 개를 실행했다고 주장하지 않는다. 임의의 status slot 이름 변경은 equality와 generation 비교를 보존하는 전단사이므로 별도 case 수를 곱하지 않는다. 사용하지 않은 두 slot의 용량을 상태 탐색 coverage로 세지 않는다.

추상 page identity는 행0/1=0/1, index=2, U0–U3=3–6, status=7이다. 이 모델은 source content latch와 target load의 수명만 나타낸다. 최대3 frame 예약·buffer replacement·try-latch 진행은 M03/M06의 실제 모델 의무이며, page identity 상수를 그 검증으로 세지 않는다. crash/restart는 이 profile의 N/A다. 한 완결 논리 WAL/CLR group의 edge/value/count 적용만 나타내며 group 내 crash atomicity·queue cursor 재실행은 M04의 별도 의무다. 전체 M01/M02 완료 판단에는 이 역할 분리의 계약 담당 확인과 해당 모델 수신이 필요하다.

## 후보 graph와 독립 oracle

행과 index의 각 header에는 현재 key, `Committed(q)` 또는 `TxnRef(slot,generation)`, UndoRef가 있다. undo에는 before key/stamp, **mutable previous-version edge**, previous-transaction edge가 있고 status에는 transaction head가 있다. 같은 UndoRef를 행/index/head/두 previous 필드가 가리키면 그 중복도를 모두 센다. `same`의 U3는 previous-version과 previous-transaction 양쪽에서 U2를 가리킨다.

후보는 각 참조 변경 때 저장 count를 증감한다. 검증기는 모든 실제 source field를 별도의 edge multiset으로 열거하여 target generation·lifecycle·중복도를 다시 확인한다. Free/Reused/Releasing으로 향하는 새 edge, generation mismatch, 음수·누락·중복 count, 과거 방향을 위반하는 undo chain은 실패다.

고정 네 논리 write의 `(writer,row,key,commit q)` 목록과 applied/rolled-back bit가 기대값을 만든다. rollback 기대값은 후보 head/undo payload를 읽지 않고 고정 statement 순서에서 제거할 write를 결정한다. 읽기 기대값은 snapshot q까지 공개될 수 있는 고정 commit 이력과 자기 거래의 남은 write를 적용하여 계산한다. 후보 row와 index 결과를 각각 비교한다. A의 ACK 값과 B의 앞선 statement 보존을 함께 검사한다.

before-image가 처음 캡처한 old link는 고의 `stale_before_link` mutant용 ghost 값으로만 보존한다. 그것은 영속 incoming edge가 아니며 정상 rollback은 aging 후의 현재 previous-version edge를 사용한다. 저장해야 할 실제 edge를 ghost로 빼서 회수하는 모델이 아니다.

## M01 fixture와 분모

정상 분모는 `layout={distinct,same}` × `rollback={statement,full}` × `reader={q10,q11}` × `start={before_b,after_b}`의 **16개**다. `before_b`는 A가 q11로 commit/publication을 마치고 B의 첫 effect 전, `after_b`는 B가 U2/U3를 모두 연결한 상태다. reader는 해당 q의 등록 소유를 가지며 명시적으로 해제할 수 있다. status slot 배치는 fixture ordinal의 `A=i mod4`, `B=(i+1) mod4`다.

`statement` rollback은 U3의 효과만 제거하여 앞선 U2를 유지한다. `full`은 U3→U2 순서로 B 전체를 제거한다. A aging과 B update/rollback, reader 읽기·해제가 모든 enabled interleaving으로 교차한다. `before_b`에서는 두 B update를 순서대로 실행한 뒤 rollback을 시작한다.

M01 고의 결함은 다음 6개를 지정한 두 fixture에서 각각 실행하여 **12개**다. 아래 축은 모두 `after_b`이며 status 배치는 해당 정상 fixture와 같다.

| mutant | 두 fixture | 고의 변경 | 기대 검출 |
| --- | --- | --- | --- |
| `stale_before_link` | distinct/full/q11, same/full/q11 | rollback에서 cut 이전 ghost previous-version을 부활 | dangling edge 또는 보존된 이력/회수 불변식 위반 |
| `missing_increment` | distinct/full/q11, same/statement/q11 | rollback이 새로 연결하는 non-null edge count 증가 생략 | incoming count 불일치 |
| `double_decrement` | distinct/full/q11, same/statement/q11 | queue가 outgoing edge 하나를 제거할 때 count 두 번 감소 | incoming count 불일치 |
| `cut_transaction_link` | distinct/full/q11, same/statement/q11 | aging이 Active B의 previous-transaction edge까지 제거 | B rollback chain/progress 불일치 |
| `cut_needed_history` | distinct/full/q10, same/statement/q10 | snapshot horizon을 무시해 필요한 previous-version/root link 제거 | q10의 독립 reader 이력 불일치 |
| `free_with_incoming` | distinct/full/q11, same/statement/q11 | incoming count가 남은 record를 Releasing/Free로 진행 | dangling/lifecycle edge 또는 이력 불일치 |

## M01 action

| action | 원자 경계와 조건 |
| --- | --- |
| `b_update` | 다음 고정 B write 하나. gate/latch가 비고 writer가 Active일 때 before-image·두 previous link·row/index 효과·head/count를 한 그룹으로 적용한다 |
| `rollback_begin` | 두 B write 뒤 목표를 statement/full로 고정한다. candidate head와 독립 기대 write 순서를 따로 보존한다 |
| `rollback_step` | gate/latch가 비면 한 undo의 before value와 **현재** previous-version link로 row/index를 복원하고 transaction head/progress/count를 함께 전진한다. 목표 도달 시 statement 완료 또는 Aborted로 구별한다 |
| `reader_observe` / `reader_release` | 현재 guard의 두 행/index를 읽거나 등록을 해제한다 |
| `horizon_sample` | gate 획득 후 `H=min(published q,등록 q)`를 별도로 저장한다 |
| `stamp` | gate 아래 하나의 Committed status 참조를 실제 commit q로 치환한다. source latch 보유 중인 header/record는 수정하지 않는다 |
| `cut` | gate 아래 한 root/previous-version edge를 제거한다. source의 현재/before version이 H에서 보이고 old suffix가 불필요할 때만 허용한다 |
| `drop_terminal_head` | terminal 거래의 queue 소유를 유지한 채 status head edge를 제거한다. Active 거래 head는 유지한다 |
| `queue_begin` | incoming0, active rollback 요구·transient pin 없음인 Live record를 Releasing으로 바꾸고 cursor0을 설치한다 |
| `queue_step` | 해당 record의 previous-version, previous-transaction, before-status를 고정 순서로 하나씩 제거하며 cursor를 함께 전진한다. 없는 필드도 cursor 진행으로 표현한다 |
| `queue_finish` | cursor3·모든 outgoing 제거·incoming0·pin 없음이면 Free로 바꾼다 |
| `reuse_undo` / `reuse_status` | 완전히 반환된 generation0 slot만 generation1로 바꾼다. 상태·pin·edge 조건을 만족하지 않으면 disabled |
| `aging_finish` | gate를 놓고 sample을 제거한다. 다음 cut은 새 sample을 필요로 한다 |

ager는 한 번에 한 queue record만 처리한다. record 선택·stamp/cut 대상은 enabled 대상별로 열거한다. frame/thread/I/O 대기는 이 모델에서 성공한 효과로 바꾸지 않는다.

## M02 fixture·action 동결

M02는 기존 P01의 q_read/register/status_install/frontier_publish/horizon_sample/guard 인계·취소·실패를 위 graph에 연결한다. 필수 시작 상태 두 개는 A의 B값이 두 행/index와 U0/U1에 있고 status 설치 전 등록 직전, 또는 status11 설치 후 frontier10·빈 registry다. B writer의 U2/U3와 q12 설치/공개도 별도 action이다.

추가 경로는 `UndoRef→previous-version UndoRef`, `TxnRef→transaction-head UndoRef`, `UndoRef→before-image TxnRef`다. `copy_root/pin_old/unlatch_root/load_old/copy_next/pin_next/release_old/unlatch_next/load_next/release_next`를 분리하고, old/new reference pin이 겹친 뒤만 old를 놓는다. source content latch는 해당 edge 변경과 조정하며, target load 중 재사용을 금지한다. snapshot guard 해제 후에도 복사한 참조 수명은 pin이 소유한다.

정상 분모는 다음 **13개**다.

| family | 변형·분모 | 초기 reader/graph |
| --- | --- | --- |
| `acquire` | registration/unpublished × distinct/same =4 | A 두 write와 U0/U1 연결. registration은 예약 slot·reader gate를 가진 q_read 직전/Active A, unpublished는 Committed11/frontier10·빈 registry다. B는 첫 write 전 |
| `chain` | undo_undo/status_undo/undo_status × row/index root =6 | A/B 네 write 모두 commit·frontier12, reader q12 등록. undo_undo는 distinct의 행1→U3→U1, status_undo는 distinct의 행1→B status→U3, undo_status는 same의 행0→U3→B status |
| `handoff` | 위 세 reference 경로 =3 | chain의 row-root에서 old reference를 복사·pin·unlatch·load한 뒤 아래 유한 prelude로 다른 보존 edge를 정리한 상태. root 이름만 다른 동일 prepared 상태를 다시 세지 않음 |

handoff prelude는 source 원본·trace에 보존한다. `copy_root,pin_old,unlatch_root,load_old,horizon_sample` 뒤, old source의 next edge를 제외한 committed stamp·root/previous-version cut·terminal head drop을 field 순서대로 적용하고 gate를 놓는다. previous-transaction edge를 임의로 삭제하거나 pin을 보유한 record를 queue-release하지 않는다. 각 prelude 전이의 oracle·graph를 검사한다. 이 도달 prefix 자체는 fixture 작성 근거이며 depth16 탐색은 그 결과의 고정 초기 상태부터 시작한다. 동결 뒤 결과에 맞춰 prelude를 바꾸지 않는다.

M02의 추가 action은 다음과 같다. M01의 maintenance edge/count/queue 전이는 그대로 사용하며 reader의 source latch나 transient pin이 금지하는 대상은 disabled다. B rollback은 이 profile의 enabled action이 아니며 M01이 담당한다.

| action | 정확 경계 |
| --- | --- |
| `q_read` | Ready·예약 slot0, 다른 actor의 gate 없음. reader gate를 얻고 frontier를 선택한다. 독립 oracle의 선택 q와 registry 등록 여부는 별도 상태다 |
| `horizon_register` | Selected·reader gate. slot0에 q를 등록하고 gate를 놓는다 |
| `registration_cancel` / `registration_fail` | cancel은 Ready/Selected, fail은 Ready. 예약·선택 q·reader gate를 해제하며 guard는 반환하지 않는다 |
| `status_install:A/B` | gate 없음. A는 초기 두 write 완료, B는 U2/U3 완료·frontier≥11. 해당 Active status를11/12로 설치한다 |
| `frontier_publish:A/B` | 해당 status 설치와 이전 frontier prefix 충족. publication은 reader gate 보유 중에도 가능하다 |
| `horizon_sample` | reader/ager gate 없음. 실제 registry와 published frontier로 H를 계산한다. 선택됐지만 미등록인 q를 registry에 몰래 더하지 않는다 |
| `guard_transfer_begin/register/release_old` | old guard 유지·source latch 없음으로 새 slot 예약/gate 획득, q 등록/gate 해제, old slot 해제를 세 action으로 한다 |
| `guard_release` | live guard·인계 전/완료·source latch 없음. registry만 해제하며 복사한 reference pin은 유지한다 |
| `copy_root` | live guard·source latch 없음·인계 등록 구간 밖. route의 현재 root UndoRef/TxnRef를 source latch 아래 복사한다 |
| `pin_old` / `unlatch_root` / `load_old` | source latch 안 pin 등록, latch 해제, generation 확인과 target load를 각각 수행한다 |
| `copy_next` | loaded old reference·old pin·source latch 없음. next edge를 old record/status의 source latch 아래 복사한다 |
| `pin_next` / `release_old` | next pin을 먼저 등록해 두 pin이 겹친 뒤 old pin을 해제한다 |
| `unlatch_next` / `load_next` / `release_next` | next pin을 유지한 채 source latch 해제, generation 확인/load, 최종 pin 해제를 각각 수행한다 |

snapshot q가 최신이라 불필요해진 undo를 접근하는 경우에도 복사한 reference가 유효해야 한다. source latch는 그 source의 edge/value 변경과 재사용을 막는다. target pin은 해당 target의 Releasing/Free/Reused 진행을 막으며 source의 edge cut/stamp는 허용한다. reader는 하나의 순차 제어 흐름이며 source latch 아래 guard 인계를 시작하지 않는다. 이미 root를 복사한 뒤 snapshot guard가 해제되어도 다음 reference 이동과 load를 계속할 수 있다.

고의 결함 분모는 **23개**다. acq fixture의 두 layout에서 `early_gate_release`, `unpublished_horizon`, `empty_registry_infinity`, `guard_handoff_gap`을 각각 실행하되 첫/마지막은 registration, 중간 둘은 unpublished로 고정한다(8). `pin_after_unlatch`, `snapshot_only_pin`, `ignore_transient_pin`은 chain의 undo_undo/status_undo × row/index root에서 각각 실행한다(12). `next_pin_handoff_gap`은 handoff 세 route에서 실행한다(3).

`early_gate_release`는 선택 후 gate를 놓고 gate 없는 등록을 허용한다. H 두 mutant는 published 대신 installed q 또는 빈 registry의 무한대를 쓴다. guard mutant는 새 등록 전 old 등록을 해제한다. `pin_after_unlatch`는 old pin 등록 전에 source latch를 해제한다. `snapshot_only_pin`은 pin 단계를 완료했다고 진행하면서 실제 pin을 만들지 않는다. `ignore_transient_pin`은 queue/reuse의 pin 조건을 무시한다. `next_pin_handoff_gap`은 새 pin 전에 old를 해제하고 source latch를 놓도록 허용한다. generation/이력/guard 소유 불변식의 지정 반례만 수락한다.

acquire 각 case의 필수 witness는 `q10_selected`, `registration_gate_blocks_aging`, `q10_read`, `registration_cancel_clean`, `registration_failure_clean`, `guard_overlap`, `guard_transfer_complete`, `two_writers_published`다. unpublished는 `q12_read`, `empty_registry_frontier10_retained`도 요구한다. registration은 처음 gate를 가진 q10 선택 경계이므로 같은 guard에서 q12를 새로 선택했다고 요구하지 않는다. chain 각 case는 `root_copied`, `old_pin_before_unlatch`, `old_loaded`, `next_copied`, `two_pins_overlap`, `old_released_after_next_pin`, `next_loaded`, `all_pins_released`를 요구한다. handoff 각 case는 `next_copied`, `two_pins_overlap`, `next_target_reuse_blocked`, `next_loaded`, `next_target_reused_after_pin_release`를 요구한다. target 재사용 차단 witness는 source edge 제거·다른 incoming0·terminal/queue 조건까지 충족하여 **pin만** 남은 상태여야 한다.

case별 status slot 배치는 ordinal을 네 slot으로 순환하며, root와 route 종류를 서로 합치지 않는다. M01의 status 이름 대칭 근거만 공유한다. M02도 full bound depth16·state/transition/time cap을 그대로 사용하며, 반례/필수 witness 미도달을 profile 성공으로 바꾸지 않는다.

## 탐색·예산·receipt

각 완결된 case의 상한은 action 길이16, 고유 상태1,000,000, enabled transitions8,000,000, monotonic60초다. 어느 cap에 도달해도 INCONCLUSIVE다. 실행 후 범위를 줄여 PASS시키지 않는다. 아래 `observed_q` 관측 ghost 예외를 제외한 모든 mutable graph/value/oracle/pin/gate/progress/witness-monitor state를 key로 하는 BFS를 사용한다. 최초 방문은 잔여 깊이가 최대이며, 동일 key의 미래 안전성 상태와 oracle가 trace에 의존하지 않아 중복 경로만 제거한다. actor/row/key/generation을 서로 합치는 축약은 하지 않는다.

정상 case는 전수 완료, 필요한 witness, 독립 oracle와 graph 불변식을 모두 만족해야 PASS다. mutant는 지정한 실제 불변식의 최단 반례가 있어야 EXPECTED_COUNTEREXAMPLE이다. arbitrary panic·disabled action·budget 초과를 기대 반례로 받아들이지 않는다. 정상·negative의 상태수·전이수·최대 깊이·미탐색 frontier·counterexample·witness action trace를 한 줄 JSON receipt로 남긴다. source/contract hash와 실제 build/command/stdout는 실행 조립에서 함께 보존한다.

M01의 각 정상 case 필수 witness는 `four_undo_live`, `both_previous_links`, `reader_observed`, `reader_released`, `rollback_before_cut`, `rollback_after_cut`, `rollback_complete`, `index_matches_history`다. `same`에는 `parallel_edges_same_target`도 요구한다. 전체 campaign에는 `undo_reused_0`–`undo_reused_3`, `status_reused_0`–`status_reused_3`가 모두 필요하다. statement rollback 완료는 앞선 B write 보존, full 완료는 A ACK와 초기부터 변경하지 않은 행 보존을 독립 이력으로 검사한다. depth16에서 필수 도달을 못 하면 정상 case라도 PASS가 아니다. M02도 다음 edge pin 인계와 reader 없는 frontier 보존을 필수로 포함한다. receipt 검사기 변경은 별도 독립 검토 대상이다.

실행 전 수동 도달 계산: `after_b/distinct/q11`에서 sample(1), B before previous-version cut 두 개(3), A head 제거(4), U1 queue begin/세 cursor/finish/reuse(10), U0의 같은 여섯 action(16)이 U0 generation1에 도달한다. `after_b/full`에서는 rollback begin/두 step(3), sample(4), U3 queue begin/세 cursor/finish/reuse(10), U2의 같은 여섯 action(16)이 U2 generation1에 도달한다. A status는 head 제거 후, B status는 full rollback과 U3 before-status 해제 후 각각 참조0이 된다. `after_b`의 홀수 ordinal은 A를 slot1/3, B를 slot2/0에 배치하므로 네 status의 재사용을 실제 경로로 덮는다. reader q10 해제를 포함한 `before_b`의 after-cut rollback은 reader 해제·B 두 update·sample·cut·aging finish·rollback begin·두 rollback step의 9 actions 이내다. 이 계산은 실행 전 도달 가능성 확인이며 실행 receipt가 아니다.

동결 초안에서 q11의 `cut_needed_history`와 same/statement의 stale-link 반례가 성립하지 않음을 실행 전에 식별했다. 소유자 `/root` 확인으로 위 16 normal/12 negative로 정정했다. 최초 실행 전에 고정한 fixture 선택이며 실행 실패 뒤 범위를 줄인 것이 아니다.

## r4 관측 ghost 동치와 보존 기록

r3의 전체 상태 키 탐색은 M02의 acquisition4/handoff3을 완주했으나 chain6 각각에서 고정1,000,000 state cap에 도달했다. 원본 source·contract·hash·실행 log는 `/tmp/xadb-m01-m02-r3`에 보존한다. 이 결과는 INCONCLUSIVE이며 r4가 소급해서 PASS로 바꾸지 않는다. M01 r0 원본과 P01의 별도 identity도 유지한다.

추가 탐색 전 계약 담당 `/root`가 사용처를 직접 확인하고 `observed_q` 하나의 stutter quotient를 D2 승인했다. 이 필드는 이미 실행한 `reader_observe`의 q를 receipt에 연결하는 ghost다. graph·값·registry·pin·gate·aging·독립 기대 이력이나 안전성 oracle를 바꾸지 않으며, 읽기 중복을 막는 조건과 `q10_read/q12_read` witness에서만 사용한다. 모든 비-Observe 전이는 이 필드와 무관하다. 어떤 관측 뒤 suffix도 Observe를 제거한 더 짧은 경로에서 동일하게 실행되므로, 관측 전의 최초 BFS 대표는 동일 이하 깊이로 모든 안전성 후속 상태를 덮는다. Observe 자체는 안전성 상태에 대한 stutter다.

M02의 hash key를 만들 때만 `observed_q=None`으로 정규화한다. node의 실제 state·parent·action trace는 원래 값을 그대로 보관한다. successor의 invariant와 witness를 검사한 뒤에만 key를 병합하며, `q10_read/q12_read`는 실제 Observe successor의 원본 trace로 재생하고 마지막 predicate를 확인한다. 그러므로 관측 receipt를 잃거나 ghost 제거를 실제 reader/guard action 제거로 해석하지 않는다. 다른 state field를 지우지 않고 fixture13/negative23·전체 action·depth16·state/transition/time cap도 그대로 유지한다. M01은 이 정규화를 사용하지 않는다. r4의 새 source/contract hash와 결과는 별도로 보존하며 성공을 미리 가정하지 않는다.

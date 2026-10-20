# DEV3 snapshot 획득·참조 pin의 부분 유한 모델

2026-10-20 · **D3-M02-P01 / 부분 모델의 고정 profile / 실제 실행 상태는 구현 기록**

[저장 소유 계약](disk-storage-design.md#snapshot-acquisition) · [전체 D3-M02 의무](../plans/dev3-implementation-plan.md#state-model) · [작업 소유](../implementation/dev3-implementation.md#wave-1)

이 문서는 D3-M02의 첫 **부분 모델**의 F02/F03 입력이다. production 구현·실제 thread·파일 I/O 시험이 아니며, 이 profile의 PASS가 D3-M02 전체나 DS01/DS04·DEV3·S1의 완료를 뜻하지 않는다. 명시한 유한 상태 공간 안에서 정상 후보의 안전성과 고의 결함의 검출을 판정한다.

## 범위와 독립 기대값

한 commit writer, reader 한 명, aging actor 한 명, 행 한 개의 `A@q10 → B@q11` 이력을 사용한다. B는 시작부터 행 header에 있고 A는 undo에 있다. commit 전에는 B가 미완료이며 status 설치와 frontier 공개를 분리한다. q10의 독립 기대값은 A, q11은 B다. 기대값은 이 고정 논리 이력에서 계산하며 후보의 status/undo 해석 결과를 정답으로 사용하지 않는다.

후보에는 행의 UndoRef/TxnRef edge 각 하나, undo/status 각 한 slot의 generation 0과 1, 저장 incoming count, source latch, gate, snapshot registry 두 slot, 이동용 transient pin 한 개가 있다. generation 1로 재사용하면 undo 값은 C, status의 commit q는 12로 바뀐다. 이전 identity를 가리키는 reader가 계속 진행하면 실패해야 한다. incoming count는 각 전이 뒤 별도 명시 edge에서 다시 계산한 값과 비교한다. 이 작은 graph는 edge 중복도·queue cursor의 전체 증명이 아니다.

Serializable begin·ReadCommitted begin_statement·짧은 catalog snapshot은 저장 계약에서 같은 획득 primitive를 사용한다. 이 모델은 primitive 한 번을 검사하며 같은 상태 공간을 owner 이름만 바꾸어 세 번 집계하지 않는다. 각 isolation/호출 경로가 실제 primitive를 사용하는지와 여러 statement의 수명은 후속 실제 thread 시험 의무다.

전체 D3-M02의 writer 2·행 2·index 1·undo/status 각 4 slots·8 page·3 frame을 모두 탐색한 것으로 보지 않는다. 여러 edge를 따라가는 pin 인계, rollback/retired object/backup owner, queue·count 중복도, buffer load/frame 예약, Fenced, 실제 lock rank와 progress turn은 이 profile 밖이다. 이번에는 한 참조를 source latch 아래 복사해 pin하고 latch를 놓은 뒤 읽는 경계를 검사한다. crash action은 **N/A**다. 이 profile은 runtime snapshot/ref 수명만 다루며 crash 후 pin 소멸·redo·복구는 M04/M06과 실제 I/O 인수에 남는다. 기존 crash≤2 계약을 crash 검증 완료로 집계하지 않는다.

## 고정 fixture

| case | 시작 상태 | 필수 도달 |
| --- | --- | --- |
| `registration` | status Active, frontier10, reader 예약 slot0·gate 보유·q 선택 직전, row의 두 edge 연결 | q10 선택/등록 사이 gate, A 읽기, 등록 취소/실패 반환, guard 인계 겹침과 해제, 최종 undo/status 재사용 |
| `unpublished` | status Committed11, frontier10, registry 비어 있음, reader 예약 slot0·gate 미보유 | reader 없는 horizon10, q10=A, 공개 뒤 q11=B, guard 인계·해제, 최종 undo/status 재사용 |
| `transient_undo` | status Committed11, frontier11, q11 등록 완료, 두 edge 연결 | UndoRef 복사→pin→unlatch→cut, pin이 재사용 차단, load 때 동일 generation, pin 해제 뒤 재사용 |
| `transient_status` | status Committed11, frontier10, q10 등록 완료, 두 edge 연결 | TxnRef 복사→pin→unlatch→stamp, pin이 재사용 차단, load 때 동일 generation, pin 해제 뒤 재사용, q10=A |

앞의 두 fixture는 소유 계약의 두 등록/공개 경계를 구체화한다. 뒤의 둘은 horizon이 pin 결함을 가리지 않게 하는 추가 fixture다. q11에서 undo는 논리 이력에 불필요해도 이미 복사한 참조의 물리 수명은 보호해야 한다. status는 q10 reader가 있어도 commit-q stamp가 edge를 제거할 수 있다.

## action과 원자 경계

enabled action만 탐색한다. 아래에 명시하지 않은 상태 변경은 하지 않는다. 무효 요청·lock 대기는 성공 전이를 만들지 않으며 관련 차단을 witness로 기록한다.

| action | 사전조건과 효과 |
| --- | --- |
| `q_read` | 예약 slot0, reader가 Ready이고 gate가 비었거나 reader 소유. gate를 reader 소유로 하고 published q를 선택한다. 선택과 등록 사이 gate를 유지한다 |
| `horizon_register` | 선택한 q·reader gate. 예약 slot0을 등록 소유로 바꾸고 gate를 놓아 guard를 반환한다 |
| `registration_cancel` | Ready/Selected에서 예약을 모두 반환하고 q·gate를 해제한다. 사용 가능한 guard는 반환하지 않는다 |
| `registration_fail` | q 선택 전 예약/진입 실패를 나타낸다. 예약과 gate를 반환한다 |
| `status_install` | Active, gate·source latch가 비어 있음. commit의 짧은 gate 적용을 하나의 전이로 나타내 Committed11을 설치한다. WAL sync는 이 profile의 선행 가정 |
| `frontier_publish` | status 설치 완료. frontier10을 11로 전진시킨다. 다른 reader가 gate를 가진 동안에도 공개될 수 있다 |
| `horizon_sample` | gate가 비어 있음. ager가 gate를 획득하고 `H=min(published q, 등록 q)`를 저장한다. sample과 cut/reuse 사이 gate를 보유한다 |
| `stamp` | ager gate·sample 존재, status 설치·TxnRef edge, source latch 없음. row stamp를 Committed11로 바꾸고 status edge/count를 함께 제거한다 |
| `cut` | ager gate·sample≥11·status 설치·undo edge, source latch 없음. undo edge/count를 함께 제거한다. transient pin은 edge 제거 자체를 금지하지 않는다 |
| `reuse_undo` / `reuse_status` | ager gate·sample, 해당 edge/count=0, 해당 pin 없음, generation0. undo는 sample≥11도 필요. generation1로 바꾸며 wrap/두 번째 재사용은 허용하지 않는다 |
| `aging_finish` | ager gate. sample을 비우고 gate를 반환한다. 다음 적용은 새 sample을 받아야 한다 |
| `guard_transfer_begin` | live guard, 인계 전, gate·source latch가 비어 있음. slot1 예약·reader gate를 얻고 slot0 등록을 유지한다 |
| `guard_transfer_register` | 인계 중·reader gate. 같은 q를 slot1에 등록한 뒤 gate를 놓는다. slot0 소유를 유지한다 |
| `guard_transfer_release_old` | slot1 등록 완료. slot0만 해제하고 인계를 완료한다 |
| `guard_release` | live guard, 인계 전/완료, reader gate·source latch 없음. 마지막 등록 소유를 해제한다. 이미 복사한 참조의 pin은 별도 소유다 |
| `copy_ref` | 추가 transient fixture의 live guard·현재 대상 edge, 아직 복사하지 않음, 인계 begin과 새 등록 사이가 아님. source latch 획득과 참조/현재 generation 복사를 한 전이로 나타낸다 |
| `pin_ref` | 복사한 참조·source latch. 사전 예약한 pin을 등록한다. 새 메모리/I/O는 추상화하지 않는다 |
| `source_unlatch` | pin 등록 완료. source latch를 해제한다 |
| `load_ref` | source latch 해제·pin 보유·미완료 load. 대상의 현재 generation과 복사한 generation을 비교하고 load 완료를 기록한다 |
| `release_ref` | load 완료. transient pin을 반환한다 |
| `read_value` | live guard, 아직 읽지 않음, 인계 begin과 새 등록 사이가 아님. 후보의 row stamp/status와 undo를 해석한 값을 고정 이력의 기대값과 비교한다 |

publication·registry leaf lock의 짧은 읽기/쓰기는 각 action 안이다. 이 모델은 해당 lock을 동시에 보유하지 않는 Rust 구현을 증명하지 않는다. `q_read/register`, `status_install/publish`, `sample/cut/reuse`, `copy/pin/unlatch/load`는 합치지 않는다.

reader actor는 하나의 순차 제어 흐름이다. source latch를 보유한 채 새 gate를 얻는 guard 인계를 시작하지 않고, 그 latch 구간 안에서 마지막 guard를 해제하지 않는다. guard 인계의 새 등록이 끝나기 전에는 같은 actor가 row 접근을 새로 시작하지 않는다. source latch를 놓은 뒤에는 target load 전에도 snapshot guard를 해제할 수 있고 transient pin만으로 대상 수명을 보호한다. 같은 reader 내부의 재진입·별도 thread가 하나의 guard를 동시에 쓰는 overlap은 이 profile 밖이다.

초기 로컬 실행 뒤 독립 검토에서 위 네 action의 구현 전제와 표의 생략을 발견했다. 첫 stdout은 그대로 보존하고, 2026-10-20 `/root/dev3_review` 지적에 따라 실제 단일 reader 제어 흐름을 표에 명시했다. 실패를 제거하는 범위 축소나 숫자 변경이 아니며, 최종 source/contract identity의 실행은 이 수정 뒤 새로 수집한다.

## 판정과 negative mutants

모든 방문 상태에서 published q 및 선택 후 취소되지 않은 reader q의 값이 논리 이력과 같게 재구성되는지 검사한다. registry 내용·예약/등록 중복·gate 소유·stored count와 별도 edge graph, 반환한 guard의 최소 한 등록 소유, 복사 후 load 전 target generation도 검사한다. 아직 source latch를 보유한 참조도 동일한 generation이어야 한다.

| mutant | 한 변경 | 고정 실행 case·필수 오류 |
| --- | --- | --- |
| `early_gate_release` | `q_read`가 등록 전 gate 해제. 결함 경로의 register는 gate 없이 허용 | `registration`: 선택 q의 A 복원 손실 |
| `unpublished_horizon` | registry와 함께 published q 대신 설치된 commit q로 H 계산 | `unpublished`: frontier10의 A 복원 손실 |
| `empty_registry_infinity` | registry가 비면 H=∞ | `unpublished`: frontier10의 A 복원 손실 |
| `guard_handoff_gap` | transfer begin에서 기존 등록을 먼저 제거 | `registration`: live guard의 등록 소유 공백 |
| `pin_after_unlatch` | pin 전 source unlatch를 허용하고 pin 등록을 unlatch 뒤로 이동 | `transient_undo`, `transient_status`: 복사한 generation 재사용 |
| `ignore_transient_pin` | reuse에서 pin 조건을 무시 | `transient_undo`, `transient_status`: 복사한 generation 재사용 |

총 정상 4 case와 고의 결함 8 case를 실행한다. mutants는 각 case의 최초 최단 반례와 오류 종류를 출력한다. 서로 다른 이름을 같은 변이로 연결하거나 오류 종류가 다른 실패를 기대 반례로 받아들이지 않는다. 정상 후보 반례는 FAIL, mutant의 반례 미검출은 FAIL, 탐색 예산 소진은 INCONCLUSIVE다.

## 전수 범위·정규화·예산

각 case에서 action 길이 **최대16**, 고유 상태 **1,000,000**, enabled transition **8,000,000**, monotonic 시간 **60초**를 실행 전 고정한다. 정상 case는 이 범위의 모든 도달 상태에서 가능한 전이를 전수 방문해야 PASS다. 상태/전이 cap 또는 시간 제한으로 중단하면 미탐색 작업 수와 함께 INCONCLUSIVE이며 실행 뒤 범위를 줄여 PASS시키지 않는다. 깊이16의 상태는 검사하되 그 다음 전이는 선언 범위 밖이다.

BFS가 깊이 오름차순으로 탐색하고 동일한 전체 상태는 최초 방문만 확장한다. 상태 key는 fixture별 mutable field 전부이며 actor 이름 교환·generation 축약·관측값 제거는 하지 않는다. 동일 상태의 미래 enabled action/기대값이 이력 경로와 무관하고 최초 방문의 잔여 깊이가 최대이므로 후속 중복 경로를 생략한다. trace, predecessor, 깊이, elapsed time, receipt 통계는 transition 의미를 바꾸지 않아 key에서 제외한다. 정상 상태의 successor가 이미 방문되어도 전이수와 해당 witness는 기록한다.

시간 예산은 fixture마다 새로 시작한다. cap 자체의 판정 검증은 동일 탐색기를 별도 작은 테스트 예산으로 강제 중단시키며 이 결과를 동결 profile의 실행으로 집계하지 않는다. 정상/결함 profile의 숫자와 fixture는 그 테스트로 변경하지 않는다.

## 실행과 receipt

구현 경로는 [reference 통합 시험](../../crates/testing/reference/tests/dev3_snapshot_model.rs)이다. 다른 crate의 production codec/비교/회수 구현을 import하지 않는다. 새 dependency가 없다.

```text
cargo test -p xadbtest-reference --test dev3_snapshot_model -- --nocapture
```

각 case는 `XADB_DEV3_SNAPSHOT_MODEL ` 뒤 한 줄 JSON으로 scope `D3-M02-P01`, fixture·mutant·상태/전이/깊이·elapsed_ms·미탐색 작업 수·witness·최단 반례를 출력한다. `error`는 실행/예산 오류, `violation`은 반례가 위반한 불변식이다. 정상 status는 `PASS` 또는 `FAIL`/`INCONCLUSIVE`, mutant 반례는 `EXPECTED_COUNTEREXAMPLE`이다. `unexplored`는 중단 시 남은 queue 작업 수의 하한이며 아직 발견하지 않은 전체 상태수를 추정하지 않는다. counterexample trace의 action 문자열을 그대로 재생할 수 있어야 한다. 별도 budget self-test는 campaign의 정상 4·negative 8 분모에서 제외한다. 마지막 `XADB_DEV3_SNAPSHOT_COMPLETE` JSON은 scope·status·normal_cases=4·negative_cases=8을 기록한다.

source/contract SHA-256·빌드 identity·실제 command/stdout와 이 receipt를 함께 보존해야 로컬 실행 증거가 된다. source를 변경하면 이전 결과를 새 source에 승계하지 않는다. 모델 완료 뒤의 독립 검토와 전체 M02/DS04의 잔여 범위는 [구현 기록](../implementation/dev3-implementation.md)에 남긴다.

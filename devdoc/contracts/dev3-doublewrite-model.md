# DEV3 doublewrite batch 유한 모델

2026-10-20 · **D3-M05-F01 / 실행 전 동결 초안 / 실행·독립 검토 미완료**

[소유 계약](disk-storage-design.md#doublewrite-batch) · [모델 인수](../plans/dev3-implementation-plan.md#state-model) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

## 범위와 독립 기대값

서로 다른 두 파일의 home page를 대상으로 한 batch 전체의 내구 장벽을 검사한다. 두 writer가 각 행 하나를 소유하고 reader 하나가 복구 완료 후 두 ACK의 값을 읽는다. maintenance actor 하나가 batch 또는 restart를 수행한다. home/image/control의 volatile 값과 durable 값을 구별한다. page 값 `0/10/20/Torn`은 checksum과 identity를 포함한 완전 image의 동치류이며 delta codec을 모델링하지 않는다.

`batch` fixture는 두 ACK 값10, 초기 home0, 아직 기록하지 않은 DWB 두 image로 시작한다. `retired_newer` fixture는 두 ACK와 home20, 이전 batch의 raw image10, durable pending 없음으로 시작한다. 후자는 정상 두 번의 batch 완료 뒤 도달하는 고정 상태이며 raw image10의 부활을 검사한다. `redirty` fixture는 ACK20, buffer20, 이미 복사한 immutable image10과 home0로 시작한다. repair 뒤 WAL redo가 buffer20을 복원해야 하며 오래된 image flush가 buffer를 clean으로 표시해서는 안 된다.

두 page는 M05가 지정한 두-file batch다. 다른 여섯 page·네 undo/status slot의 비활성 상수를 넣어 그 수용량을 검증했다고 하지 않는다. slot/generation·행/index/undo 변경은 M01/M04의 별도 의무다. target 주소 두 개의 incarnation1/2와 DWB slot sequence0/1은 별도 상태다. retirement 뒤 각각의 재사용을 허용하며 pending selector가 선택한 incarnation/sequence와 다르면 거절한다. 배정된 세 frame은 두 target source pin과 한 repair workspace다. batch capture와 capture_finish 사이에 두 source pin을 명시 보유하고 image·modification counter를 복사한 뒤 둘을 해제하며, restart의 repair는 workspace 하나로 한 page씩 수행한다. buffer replacement의 공정성은 M03/M06에 남는다.

## action

모든 action의 guard가 거짓이면 전이가 없다. 성공 I/O와 명시 torn write를 구분한다.

| action | guard·효과 |
| --- | --- |
| capture / capture_finish | Live, 아직 미복사; source pin mask3을 획득한 상태와 image·modification counter 복사 뒤 pin0 상태를 분리 |
| repair_acquire_0/1 | Repair, 복원이 필요한 page만 빈 workspace frame2를 해당 page 소유로 획득. 이미 올바른 home에는 획득 불가. home write가 소비·해제 |
| target_reuse_0/1 | retirement 뒤 target incarnation1→2; pending 동안 금지 |
| wal_sync | capture 완료. 두 image의 최대 pageLSN10까지 durable WAL 확보 |
| image_write_0/1 | wal_sync 완료; 해당 volatile image10 기록 |
| image_sync | 두 image 기록 완료; 두 durable image10 확보 |
| select_pending | image_sync 완료; volatile control에 두 slot을 함께 선택 |
| control_sync | volatile selector를 durable로 선택. pending 선택 또는 전체 retirement를 구별 |
| home_write_0/1 | durable pending 전체 존재; 해당 volatile home10 기록 |
| home_torn_0/1 | 같은 guard; 해당 volatile home Torn, Fenced |
| file_sync_0/1 | 해당 complete home10, durable pending 전체; 해당 durable home10·sync 완료 bit 기록 |
| select_retired | 두 file sync 완료; volatile control에서 batch 전체 제거 |
| slot_reuse | retirement control sync 완료; 두 슬롯을 새 sequence로 재사용. buffer counter가 image counter와 다르면 dirty 유지 |
| crash(mask) | crash<2. 미동기 home은 stable/volatile/Torn, image·control은 stable/volatile 중 각각 독립 선택. home Torn은 실제 write 이후 해당 file sync 전만 가능. 성공 sync의 bytes는 항상 보존. volatile frame·sync receipt 소멸, Restart 진입 |
| boot_select | Restart. durable control만 읽음. pending 있으면 Repair, 없으면 redo 대기 |
| repair_write_0/1 | 선택된 전체 pending과 valid durable image; workspace 하나로 해당 home을 기록. 더 높은 정상 home을 덮지 않음 |
| wal_redo / redo_ready | pending repair·모든 file sync·retirement 완료. wal_redo가 durable home을 buffer로 읽고 별도 candidate durable WAL의 더 높은 pageLSN record를 replay. redo_ready는 그 buffer를 그대로 reader에게 반환. 고정 ACK oracle은 이 경로에 전달하지 않음 |

repair는 같은 home/file/control action을 사용하며 중간 두 번째 crash에도 전체 pending이 유지된다. sync 오류/selector 불명은 이 모델의 crash(mask)가 가능한 old/new 영속 효과를 펼친다. 실제 errno, namespace 손상 거절·1/8/64-page 실측은 F04/실제 I/O 인수다.

## 불변식과 고의 결함

pending은 항상 전체 두 slot이거나 없음이며, 선택된 image 두 개는 valid/durable이다. home write 전에 durable pending이 있어야 한다. retirement 선택은 두 file sync의 증거를 요구한다. immutable image10은 home20 위에 복원할 수 없다. crash 뒤 repair가 끝나기 전 redo/새 ACK/slot 재사용을 허용하지 않는다. READY의 reader 값은 고정 ACK 이력과 같으며 redirty frame은 dirty다.

| mutant | 위반 |
| --- | --- |
| pending_before_image_sync | durable pending이 미동기 image를 참조 |
| home_before_pending_sync | home write가 durable pending 선택보다 앞섬 |
| retire_after_first_file | 한 file만 sync한 뒤 전체 retirement. `home_write_0/1 → file_sync_0 → select_retired → control_sync → crash(home1=Torn)`이면 file1 sync 누락으로 pending 없는 torn home이 남는다. 검사기는 이미 select_retired에서 `retirement_without_all_file_sync`로 이 경로를 거절 |
| partial_retirement | 일부 성공 slot만 pending에서 제거 |
| reuse_before_retirement | 선택된 slot을 덮음 |
| raw_retired_resurrection | retired_newer에서 raw10을 선택해 home20을 덮으려 함 |
| clear_redirty | image10 완료로 buffer20을 clean 처리 |
| address_reuse_before_retirement | pending인 target incarnation을 바꿔 `pending_address_reused` 위반 |
| omit_wal_redo | durable WAL replay를 생략해 `ack_or_repair_order_lost` 위반 |

정상 case의 기대값은 candidate가 반환한 repair image에서 계산하지 않는다. ACK10/20의 상수 이력과 장벽의 독립 event bit로 검사한다. mutant는 지정 오류와 BFS 최단 trace를 요구하며 trace를 다시 step하여 같은 오류를 확인한다.

## 전수와 receipt

각 case는 action 길이16·crash2, 1,000,000 states·8,000,000 enabled transitions·monotonic60초다. 전체 상태를 key로 쓰는 BFS이며 최소 깊이 방문만 확장한다. actor 대칭 축약·durable bit 삭제·crash 횟수 정규화는 없다. 늦은 재방문의 미래는 같은 state에서 동일하고 잔여 depth가 더 작기 때문에 생략한다. 예산 소진은 INCONCLUSIVE이며 사후 축소하지 않는다.

필수 witness는 complete batch retirement, two-file repair, crash2, retirement 뒤 slot 재사용, pending repair 중 재장애, redirty 보존, raw retired image 무시다. 한 fixture에서 도달하지 않는 항목은 해당 다른 fixture에 명시 연결한다. 모델이 depth16에서 요구 witness를 못 찾으면 미완료로 보고하며 범위를 줄여 PASS하지 않는다.

`crates/testing/reference/tests/dev3_doublewrite_model.rs`가 `XADB_DEV3_DOUBLEWRITE_MODEL ` 한 줄 JSON에 fixture/mutant·상태/전이/최대깊이·crash·미탐색·witness·반례·시간을 기록한다. 별도 작은 예산의 검사기 시험은 본 전수의 분모에서 제외한다. 이 결과는 작은 batch 장벽의 모델이며 DS02/05·실제 torn-page·M05의 제품 실행을 대신하지 않는다.

2026-10-20 독립 검토 수정: 첫 모델은 reader에 oracle ACK를 복사하고 pin/address ownership을 bool로 생략했으므로 해당 실행을 수용 증거로 사용하지 않는다. 정상 fixture3·depth16·crash2·state/transition/time 상한은 유지하며 candidate WAL→home/buffer→read, 명시 pin/workspace·incarnation/sequence와 추가 mutant2를 실행 전에 동결했다. `redirty`의 시작 captured=true는 문서에 이미 선언한 도달 가능한 image10/buffer20 cut을 코드에 맞춘 수정이다.

추가 독립 검토에서 complete home에 repair workspace를 잡고 해제하지 못하는 전이를 발견했다. `repair_acquire`는 복원이 필요한 page에만 허용하며 retirement/READY에서는 모든 pin/workspace가 해제되어야 한다. 고정 mixed-home 도달 trace를 별도 회귀로 보존한다.

receipt의 PASS는 initial invariant, 필수 witness 분모, 저장한 모든 witness trace의 initial부터 각 전이 invariant·마지막 predicate 재실행까지 성공한 뒤에만 출력한다. mutant는 첫 위반이 trace 마지막이며 기대 오류와 같음을 재실행한 뒤 EXPECTED_COUNTEREXAMPLE을 출력한다. 재실행 실패는 FAIL이며 counterexample_replayed/witnesses_replayed를 별도로 기록한다.

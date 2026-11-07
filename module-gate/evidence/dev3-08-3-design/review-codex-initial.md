Verdict: CONDITIONAL

# PR #66 독립 D2 추가 검토 — O0 3판

- 검토자: `/root/overflow_design_review` (작성·수정 담당 `/root`와 별도 실행).
- 대상: `928ec853348a0f197a0da0b65574eb386237499f`, `devdoc/contracts/dev3-row-overflow.md`와 연결된 현재 제품 코드. 초기 판정은 이 source에만 적용한다.
- 범위: 소유권·rollback·Base4·읽기·A/C 호출 경계와 제품 방향. 제품 코드·기존 시험은 실행하거나 수정하지 않았다. 아래는 코드와 설계의 정적 대조이며 제품 실행 증거가 아니다.
- 판단: 뒤에서부터 쓰는 불변 chain, Job→leaf→보상 Job→XTR1의 소유 이전, 실제 Free를 14-2에 남기는 범위는 현재 단일 writer·첫 extent profile과 일치한다. 128 KiB·whole-page WAL·전역 읽기 gate의 한계를 최종 제품 목표의 달성으로 주장하지 않는 것도 적절하다. 그러나 구현에 필요한 세 호출 계약이 빠져 있어 조건부 수용이다.

## R1 — 검증된 길이를 얻기 전 Free page k개를 검사할 수 없음 (필수, P2)

설계 §7.1은 `preflight(k 확인) → A 값 검사 → reserve(plan)`이고 §7.2는 `preflight_row`가 overflow 길이와 k를 검사한다고 한다. 현재 `plan4_apply/row/write.rs:375`의 preflight 입력에는 값이나 길이가 없다. A는 그 반환 이후 `row/insert/driver.rs:785`의 `validate_row`에서 처음 `RowInputPlan`을 얻는다. 입력을 먼저 검사하도록 단순히 뒤집으면 `dev3-module-consumers.md`의 source/schema 검산 우선순위를 바꾼다.

수정 요구:

- 원 preflight에서 원 allocator의 eligible Free 수·Reserved 유무를 안정된 cut으로 관측하고, A가 원 schema로 길이·digest를 검산한 뒤 C Reserve가 `k = ceil(L/16092)`와 대조하도록 두 단계를 구분한다. 모든 부족 판정은 Reserve의 첫 효과 전이다.
- allocator 부족과 leaf 공간 부족을 구분한다. 현재 `plan4_apply.rs:388`의 `row_capacity()`는 `Unchanged(Prepare, Capacity)`를 모두 split 후보로 만들고 `driver.rs:791`이 그 값을 사용한다. 새 allocator 부족을 이 경로에 넣으면 불필요한 split을 시도해 부족한 page를 더 소비할 수 있다. head 행의 실제 leaf 부족만 split 길이 64를 넘긴다.
- 새 인수에는 Free `k−1/k` 경계, invalid 값과 부족 allocator가 겹칠 때 오류 우선순위, 부족 시 split·WAL·issuer 효과 없음이 필요하다.

## R2 — ChunkWrite 뒤의 reservation과 flush 소유 계약 부재 (필수, P2)

설계 §7.1–7.2는 `prepare_chunk → apply`를 k번 하고 중간 FlushRequired를 처리한다. 기존 상태 기계는 한 번의 Forward만 지원한다.

- `plan4_apply/row/write.rs:273–314`: ReservedRow의 draft는 생존 중 바뀌지 않고 하나의 RowSlotReservation·gate·RowIoLease를 쥔다.
- `write.rs:1164–1210`: PreparedRow::apply 성공은 reservation을 discharge하고 RowApplied를 반환한다. 이 성공값으로 다음 ChunkWrite를 할 수 없다.
- `write_operation.rs:35–50`: 공개 sealed trait에도 계속 Reserved 상태로 돌아오는 apply가 없다.
- `write.rs:13–18`: RowIoLease는 원 flusher control을 독점 차용하고 row가 끝날 때까지 flush capture/eviction을 금지한다. 외부 A flush를 그대로 호출할 수 없다.

수정 요구:

- 별도 prepared-chunk 상태의 성공은 원 reservation을 active로 보존한 ReservedRow를 반환하고, 새 Root·Job·allocator·LSN/descriptor에 맞게 C가 draft를 갱신한다. Forward/Cancel만 reservation을 discharge한다. 중간 ChunkWrite를 행 공개·사용자 mutation observation으로 반환하지 않는다.
- FlushRequired 처리에는 원 reservation을 유지하는 전용 경로를 정한다. 모든 page guard·준비 cut을 해제한 안전한 그룹 경계에서 원 flush control과 전용 flusher owner/credit을 사용하고, Ready 복구 뒤 다음 그룹의 source/draft를 재검산한다. 기존의 “row가 끝날 때까지 flush 금지” 계약을 어느 범위에서 대체하는지 명시한다. stage 뒤 실패·active reservation Drop의 fence는 유지한다.
- 원 read gate가 preflight부터 Forward/Cancel까지 이어진다면 §9의 “k+2개 그룹 동안 반복해 닫음”은 실제로 연속 보유임을 적는다. 그룹마다 gate를 풀려면 별도 재진입 계약이 필요하다.
- 새 인수에는 첫/중간/마지막 chunk 뒤 계속 쓰기·취소, chunk 성공 후 stale draft 재사용 거절, 중간 flush 뒤 동일 chunk 재개, stage 뒤 실패 fencing을 포함한다.

## R3 — B의 복사 단계에는 FlushRequired를 반환하는 seam이 없음 (필수, P2)

설계 §7.3은 chunk LOAD에 flush가 필요하면 기존 Step 흐름으로 돌아간다고 한다. 그러나 `row/read/driver.rs:99`의 ReadSource::copy_candidate와 `plan4_apply/row/read.rs:1495`의 실제 메서드는 `Result<Copied, RetentionError>`다. `driver.rs:255`의 copy_selected는 이 결과를 즉시 복사 완료로 취급한다. LiveSource·LocatedSource도 같은 형태로 전달한다.

수정 요구:

- copy_candidate가 residency cursor를 받아 `Result<Step<Copied, Target>, RetentionError>`를 반환하도록 C API·ReadSource·두 adapter·copy_selected/select_row의 변경을 O3 계약에 포함한다. copied_slot은 Ready에서만 채운다.
- FlushRequired와 오류도 `release_selected` 및 operation `finish`의 checked cleanup을 거친 뒤 반환한다. 실패한 visible candidate를 last_examined/last_returned에 기록하지 않는다. 이미 완전히 판정한 invisible prefix는 유지하고 재호출은 같은 행의 chain 처음부터 읽는다.
- 기존 F02 계약(`dev3-task-06-tree-rows.md:285–295`)은 실패 시 output 유효 길이 0을 요구하지만 scratch bytes 원상 보존을 요구하지 않는다. 따라서 128 KiB 사본을 추가할 필요는 없다. 부분 복사 뒤 digest/schema 오류를 Found로 공개하지 않는 것이 중요하다.
- point와 cursor 각각 첫/중간/마지막 chunk copy의 FlushRequired, 마지막 digest/schema 실패, pin/gate 해제, 동일 visible 행 재시도와 중복·누락 없음을 새 시험으로 지정한다.

## 그 밖의 확인과 범위

- statement rollback 뒤 ChainRetire를 terminal까지 미루는 것은 기존 maintenance의 terminal 조건과 일치한다. statement가 계속될 때 compensated chain을 SMO 계수에 넣는 것도 필요하며 설계에 있다.
- 08-3은 committed overflow 행의 UPDATE/DELETE가 없고 같은 statement 호출은 직렬화되므로, 현 read gate 아래 chain 반환과 독자의 경합이 생기지 않는다는 논증은 현재 범위에서 성립한다.
- Base4의 role 7 별도 accessor·CREATE anchor 대조·현재 image와 INIT digest의 일치·exact-once ownership 검산은 현재 불변 chunk 모델과 맞는다. actual Free/reuse를 도입할 때 다시 검토한다는 경계도 명시돼 있다.
- frame·ledger 수치는 부모 검토자의 독립 계산 범위다. 이 검토는 그 계산을 실행했다고 주장하지 않는다.
- 위 필수 세 건의 수정·새 설계 검증을 별도로 대조한 뒤 최종 판정을 기록한다. O1–O4 제품 동작·native fault cut·census는 계속 NOT_RUN이다.

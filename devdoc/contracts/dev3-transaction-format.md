# DEV3 거래 identity·상태 cell 표현

2026-10-20 · **D2 계약·production diff 독립 검토 수용 / 표현 구현·국소 시험 완료 / 실제 거래 효과 미구현**

[디스크 설계](disk-storage-design.md#undo) · [관측 행 버전](disk-storage-design.md#observed-row-version) · [참조 수명](disk-storage-design.md#undo-lifecycle) · [복구 예약](disk-storage-design.md#recovery-reservation) · [page envelope](dev3-page-format.md) · [PageRef·UndoRef](dev3-reference-format.md) · [병렬 소유](../implementation/dev3-implementation.md#wave-2)

이 F01 leaf는 거래 identity, VersionStamp, 논리 행 버전, 거래 상태 cell과 그 읽기 검증만 정의한다. encode/decode는 caller buffer 또는 immutable page bytes만 다루며 I/O·allocation·발급·visibility 판정·pin 등록·상태 전이·commit·rollback·회수·WAL·자원 예약 효과가 없다. 기존 DEV2의 `transaction::TxnId` 값 타입을 같은 crate 안에서 참조하며 public API·DEV2 의미·fixtures·crate 의존성을 변경하지 않는다.

아래 모든 다중 byte 정수는 little-endian이다. raw Rust struct layout을 저장하지 않는다. decode는 정확한 길이만 수락하며 encode의 모든 오류는 output 전체를 보존한다. 현재 개발 형식은 새 fixtures를 사용하고, 이 leaf의 채택·단위 시험은 전체 F01·DEV3·S1 완료나 현재 build의 실제 recovery 인수를 뜻하지 않는다.

2026-10-21 후속 [영속 home 계약](dev3-status-home.md)은 이192-byte core를 그대로 포함한224-byte home와71-slot loader를 정의한다. 이 문서의192-byte page와82-slot 한도는 원래 표현 leaf 범위이며, 새 persistence 소비자는 home loader의 더 좁은 한도를 사용한다. 이전 fixture와 검증 결과를 새 home 실행으로 재표기하지 않는다.

## 1. Identity와 namespace

### TxnId: 40 bytes

기존 `TxnId { database, timeline, writer_epoch, local }`의 값만 사용한다. public field를 직접 조립할 수 있으므로 wire encoder와 이 문서의 모든 wrapper constructor는 timeline·writer_epoch·local의 nonzero 조건을 다시 검사한다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 16 | DatabaseId, 전체 zero 금지 |
| 16 | 8 | timeline, nonzero |
| 24 | 4 | writer_epoch, nonzero |
| 28 | 4 | reserved zero |
| 32 | 8 | local, nonzero |

TxnId는 발급 이후 재사용하지 않는다. rollback은 local을 돌려주지 않는다. epoch/local의 최댓값은 표현 가능한 유효 값이지만, 발급기의 다음 값 overflow는 효과 전에 거절해야 하며 wrap하지 않는다. 이 codec은 다음 ID를 발급하지 않는다.

지속 identity의 namespace는 **DB와 timeline**이다. 재open 이후 현재 writer_epoch보다 오래된 epoch의 TxnId도 읽어야 한다. 따라서 status page/lookup의 namespace 검증은 DB/timeline을 확인하며 현재 writer_epoch와의 일치를 요구하지 않는다. open-instance identity는 이 wire 표현에 없으며 이력 읽기 capability나 관측 행 token을 새로 만들지 않는다.

### 논리 TxnRef: 48 bytes

`0..40=TxnId`, `40..48=slot generation:u64(nonzero)`다. `TxnRef(id,generation)`은 논리 참조이며 file/page/slot 주소를 포함하지 않는다. 같은 TxnId의 임의 다른 generation을 정상 별칭으로 허용하지 않는다. status slot의 현재 점유자와 generation을 함께 재검증해야 한다.

### 물리 StatusSlotRef: 40 bytes

`0..24=PageRef`, `24..26=slot:u16`, `26..32=reserved zero`, `32..40=slot generation:u64(nonzero)`다. UndoRef와 byte 폭은 같아도 서로 다른 Rust 타입과 사용 영역이다. slot은 이 문서의 고정 192-byte status cell용 안정 directory index이며 `0..82`, 즉 0–81만 수락한다. 82는 `(16,352−128)/(directory4+cell192)`의 내림값이다. slot의 실제 존재는 page를 읽을 때 별도 검사한다.

**영속 directory의 후속 의무:** bounded한 영속 status directory가 전체 TxnId에 대한 현재 논리 generation과 StatusSlotRef를 해석해야 한다. `local`에서 page나 slot을 산술 유도하지 않는다. directory entry의 설치/제거와 status INIT·generation 변경·관련 영속 edge는 소유 WAL 변경 규약으로 결합해야 한다. 이 leaf는 directory·cache·lookup I/O·live relocation을 구현하지 않는다. directory를 읽는 쪽도 source latch 해제 전 transient pin과 unlocked interval 뒤 재검증 의무를 갖는다. stale directory entry를 정상 Aborted/NotFound로 바꾸어 처리하지 않는다.

### LogicalRowVersion: 48 bytes

`0..40=TxnId`, `40..48=mutation_sequence:u64(nonzero)`다. sequence는 한 거래의 **모든 사용자 행 변경**에 걸쳐 단조롭게 발급하는 수이며 같은 거래가 같은 행을 여러 번 바꾸어도 서로 다르다. 같은 bytes로 UPDATE해도 새 sequence를 발급한다. sequence를 rollback 때 반환하거나 재사용하지 않으며 `u64::MAX` 다음 발급은 효과 전에 거절한다.

undo는 before-image의 이전 LogicalRowVersion을 복원한다. page 이동·split·compaction·commit-q stamping은 identity를 바꾸지 않는다. status generation을 포함하지 않으므로 status의 회수와 stamping만으로 논리 버전이 변하지 않는다. equality는 전체 TxnId와 sequence의 일치다. 발급·증가·행 적용·undo 복원 자체는 이 codec 밖의 효과다.

이 값만으로 caller가 관측 행 capability를 만들 수 없다. 향후 observation은 DB/timeline/open·table generation/RowId·statement token과 이 값에 결합해야 한다. row/index cell이 VersionStamp와 LogicalRowVersion을 함께 담을 때 필요한 cross-field·owner 검증은 해당 cell 소유 계약에 추가한다.

## 2. VersionStamp: 56 bytes

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 1 | tag: Committed=1, Transaction=2 |
| 1 | 7 | reserved zero |
| 8 | 48 | 아래 variant payload |

Committed는 `8..16=CommitSeq(nonzero)`, `16..56=zero`다. Transaction은 `8..56=TxnRef`다. tag0과 미지원 tag는 손상이다. published frontier의 초기 q=0은 이 leaf의 사용자 행 Committed stamp가 아니므로 q=0을 거절한다.

Committed와 Transaction의 byte 표현/equality만 정의한다. Transaction stamp를 현재 상태와 resolve하거나 snapshot q와 비교하여 가시성을 결정하지 않는다. 다른 거래의 committed before-image를 캡처할 때 resolved q로 저장하는 의무, own 반복 변경의 chain 보존, logged stamping은 [undo 소유 계약](disk-storage-design.md#undo)의 별도 실행 경로다.

## 3. Status cell: 192 bytes

occupied와 Free tombstone은 동일한 크기다. status page는 TransactionStatus(kind9), level0의 공통 envelope이며 0–82개 cell을 가진다. 각 cell은 정확히192 bytes다. empty page는 아직 slot을 설치하지 않은 표현으로만 유효하며 어떤 lookup도 성공하지 않는다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | ASCII `XTS1` |
| 4 | 2 | format version1 |
| 6 | 1 | Free=0, Active=1, Aborting=2, Committed=3, Aborted=4 |
| 7 | 1 | rollback kind: None=0, Statement=1, Transaction=2 |
| 8 | 40 | occupied: TxnId; Free: 전체 zero |
| 48 | 8 | slot generation, nonzero; Free에도 유지 |
| 56 | 8 | Committed의 q(nonzero), 그 밖은 zero |
| 64 | 1 | presence flags: bit0 undo_head, bit1 target, bit2 next |
| 65 | 7 | reserved zero |
| 72 | 40 | undo_head UndoRef |
| 112 | 40 | rollback target UndoRef |
| 152 | 40 | rollback next UndoRef |

presence bit가 0이면 대응40 bytes가 전부 zero여야 한다. 1이면 유효한 nonzero UndoRef 표현이어야 한다. 다른 flag bit는 거절한다. head는 거래의 undo/aging 소유를 나타내는 남은 chain anchor이고 next는 아직 적용해야 할 rollback cursor다. head와 next가 항상 같다고 가정하지 않는다.

### 허용 상태와 전이 의무

| 표현 상태 | q | head | rollback kind/target/next |
| --- | --- | --- | --- |
| Free(g) | 0 | 없음 | None, target/next 없음; TxnId도 zero |
| Active | 0 | 없음 또는 있음 | None, target/next 없음 |
| Aborting Statement | 0 | 없음 또는 있음 | Statement. head가 없으면 target/next도 없음. target이 있으면 next도 있어야 함 |
| Aborting Transaction | 0 | 없음 또는 있음 | Transaction, target 없음. head가 없으면 next도 없음 |
| Committed | nonzero | 없음 또는 있음 | None, target/next 없음 |
| Aborted | 0 | 없음 또는 있음 | None, target/next 없음 |

모든 상태에서 next 또는 target이 있으면 head가 있어야 한다. Statement의 `next==target`는 mark까지 복원을 마쳐 Active로 돌아갈 수 있는 위치다. 둘 다 None인 경우도 빈 mark 도달이다. Transaction의 next=None은 full abort 완료 위치이며 Aborted 전환과 구별해 표현한다. next/target이 실제 같은 거래의 chain에 속하는지, record sequence가 과거로 진행하는지, 해당 CLR과 효과가 일치하는지는 undo loader·WAL 소유자가 검사한다. 물리 주소 숫자를 비교하여 chain 순서를 추정하지 않는다.

Statement rollback 완료 뒤에는 동일 TxnId/generation의 Active로 복귀할 수 있고 이후 새 statement와 새 mutation sequence를 계속 사용할 수 있다. 반복 실패로 소비한 WAL/CLR 비용을 되돌리거나 예약을 초기화하지 않는 의무는 자원 소유자에게 있다. full rollback은 Aborted로 종료한다. Committed/Aborted의 head는 aging이 끝날 때까지 남을 수 있다. terminal을 decode했다는 이유만으로 head를 제거하거나 슬롯을 Free로 바꾸지 않는다.

Free(g)는 직전 점유자의 generation을 보존하는 tombstone이다. 서로 다른 새 TxnId로 재점유할 때 checked `g+1`과 새 INIT이 필요하며 g를 다시 사용하지 않는다. g가 최댓값이면 그 slot은 재사용 불가이며 wrap하지 않는다. 최초 새 slot은 allocator가 generation1로 직접 초기화할 수 있다. codec은 generation을 선택하지 않는다.

slot directory index는 삭제/compaction 때도 안정적이다. cell의 page 내부 byte offset은 바뀔 수 있으나 live index를 앞당기거나 중간 tombstone을 빼서 뒤 index를 이동시키지 않는다. 빈 slot도192-byte tombstone으로 유지한다. 실제 재사용은 관련 TxnRef의 logged stamp/영속 edge 제거·head와 outgoing 회수 완료·horizon/rollback/queue 소유 정리·마지막 transient pin 해제·관련 DWB pending retirement를 모두 요구한다. Free bytes만으로 이 선행조건이 충족됐다고 인정하지 않는다.

### 복구 descriptor와의 경계

이 cell은 상태·head·rollback cursor의 **core 표현**이다. 복구 reservation descriptor의 profile/version·각 resource outstanding/retained 비용·token identity·growth/FILEMAP·checkpoint/ATT 결합은 이192 bytes에 표현되지 않는다. 미동결 descriptor를 opaque zero 필드로 넣고 성공 처리하지 않는다. 향후 descriptor를 별도 영속 record로 결합하거나 cell format version을 올릴 때 별도 F01/F05 검토가 필요하다. 이 core만으로 recovery-ready나 rollback 자원 보장이 성립하지 않는다.

## 4. Private API와 검증 경계

`storage::transaction_record`는 `pub(crate)`이며 public re-export하지 않는다. 실제 첫 소비자가 붙기 전까지 이 파일에만 이유와 제거 조건을 적은 `allow(dead_code)`를 사용한다. 허용 의존은 기존 `crate::transaction::TxnId`, `crate::api::{DatabaseId,Namespace}`, 공통 page/UndoRef, foundation의 Generation/CommitSeq다. 새 crate 의존·allocator·production model import가 없다.

| API | 의미 |
| --- | --- |
| `encode_txn_id(TxnId, &mut [u8])`, `decode_txn_id(&[u8])` | 기존 ID 값의 정확40-byte codec |
| `TxnRef::new`, `id`, `generation`, `encode/decode` | private field의 검증된 논리 참조 |
| `StatusSlotRef::new`, `page`, `slot`, `generation`, `encode/decode` | private field의 검증된 물리 참조 |
| `LogicalRowVersion::new`, `transaction`, `mutation_sequence`, `encode/decode` | nonzero sequence를 포함한 identity 값 |
| `VersionStamp::{Committed,Transaction}`, `encode/decode` | 유효한 CommitSeq/TxnRef만 담는 tagged 값 |
| `RollbackState::{Statement { target, next }, Transaction { next }}` | rollback 종류와 cursor 값 |
| `TransactionState::{Active,Aborting,Committed,Aborted}` | Committed는 CommitSeq, Aborting은 RollbackState를 포함 |
| `TransactionStatus::new(reference,state,undo_head)`와 read-only getters | 위 상태표를 검증한 occupied 값 |
| `TransactionStatusCell::{Free(Generation),Occupied(TransactionStatus)}`, `encode/decode` | 고정192-byte occupied/tombstone codec |
| `decode_status_page(input,expected_page,namespace)` | 전체 envelope와 모든 status cell을 검증한 borrowed StatusPage |
| `StatusPage::len`, `cell(slot)`, `lookup(expected_ref,location)` | immutable 값 읽기와 논리/물리 identity 일치 확인 |

`decode_status_page`는 공통 `decode_page`로 length/digest·full PageIdentity·kind/level을 먼저 검사한다. namespace의 timeline은 nonzero여야 하며 page의 DB와 namespace DB가 같아야 한다. 모든 occupied cell의 DB/timeline도 namespace와 같아야 한다. 선택하지 않은 cell도 exact size·reserved·상태표를 검사하므로 부분 손상을 무시하지 않는다. slot 수는82 이하로 제한한다.

`lookup`은 page header의 exact PageRef와 location PageRef, location generation과 expected TxnRef generation, slot의 존재, occupied cell의 전체 TxnRef를 모두 확인한다. Free·존재하지 않는 slot·다른 점유자·다른 generation·page incarnation 변경은 IdentityMismatch다. full expected page의 DB/owner/object generation은 page decode에서 확인한다. 기존 writer_epoch와 현재 writer_epoch를 비교하는 인자는 없다. 올바른 bytes도 frame pin/content latch·transient pin·live reference capability를 제공하지 않는다.

encoder/constructor의 무효 입력은 InvalidInput, wire의 길이·tag·reserved·nonzero·상태 불변식 위반은 Corrupt, status cell의 미지원 format version은 UnsupportedFormat, namespace/identity 불일치는 IdentityMismatch, 잘못된 page kind는 KindMismatch다. 여러 결함이 동시에 있을 때 오류 우선순서는 계약하지 않는다. 모든 encoder는 입력/길이 검증을 끝내기 전 caller output을 쓰지 않으며 실패 시 sentinel 전체 보존을 시험한다.

## 5. 실행 전 고정 literal

아래 expected bytes는 candidate encoder를 호출하지 않고 offset 표에서 작성한 고정 literal이다. DB는 bytes1–16, timeline17, writer_epoch18, local19, slot generation20, mutation_sequence21, q22다. PageRef는 기존 참조 vector(file epoch1/counter2,page3,incarnation4)다. head/next는 slot5/generation6, statement target은 같은 page의 slot4/generation5다. V06 status location은 slot5/generation20이다.

| vector | 의미 | bytes |
| --- | --- | --- |
| V01 | TxnId | 40 |
| V02 | TxnRef | 48 |
| V03 | LogicalRowVersion | 48 |
| V04 | Committed stamp | 56 |
| V05 | Transaction stamp | 56 |
| V06 | StatusSlotRef | 40 |
| V07 | empty Active | 192 |
| V08 | Aborting Statement, head/target/next | 192 |
| V09 | Committed, retained head | 192 |
| V10 | Aborted, retained head | 192 |
| V11 | Aborting Transaction, completed next=None, retained head | 192 |
| V12 | Free generation20 | 192 |

```text
V01 0102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000
V02 0102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001400000000000000
V03 0102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001500000000000000
V04 0100000000000000160000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V05 02000000000000000102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001400000000000000
V06 02000000010000000300000000000000040000000000000005000000000000001400000000000000
V07 58545331010001000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V08 58545331010002010102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000000000000000000000700000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000200000001000000030000000000000004000000000000000400000000000000050000000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000
V09 58545331010003000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000016000000000000000100000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V10 58545331010004000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000000000000000000000100000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V11 58545331010002020102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000000000000000000000100000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V12 585453310100000000000000000000000000000000000000000000000000000000000000000000000000000000000000140000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
```

## 6. 고정 시험 분모와 oracle

국소 unit test 이름은 아래8개로 고정한다. Rust module 등록·runner 연결과 문서 색인 등록은 통합 소유자가 별도로 수행한다. `/root`가 전체 초안의 byte/API·상태14개·길이1,452 분모와 미완료 경계를 독립 검토하여 구현 진행을 수락했다. 영속 directory의 전역 TxnId 유일성이나 chain 소유권은 local codec이 검증하는 성질이 아니다.

| test | 고정 범위 |
| --- | --- |
| `identity_literals_and_limits_are_canonical` | V01/V02/V06; 각 nonzero 최소1·최댓값; status slot81 수락/82 거절 |
| `stamp_and_logical_version_identities_are_distinct` | V03/V04/V05; 동일 거래 sequence1/2 구별·복사/old identity 복원 표현·stamp 변경 독립성 |
| `status_literals_preserve_rollback_and_terminal_heads` | V07–V12; statement mark 도달·반복 Active 표현과 terminal retained head 구별 |
| `all_length_boundaries_reject_without_output_changes` | V01–V12 각각 길이0..정확길이−1 및 정확길이+1. decoder1,452회·encoder1,452회 거절 |
| `fixed_corrupt_fields_are_rejected` | 아래 N01–N40 정확40개 semantic 손상 |
| `status_lookup_binds_logical_and_physical_identity` | 아래 L01–L24 정확24개 거절과 정상/old epoch/free/0·82cell 경계 |
| `state_and_namespace_matrix_is_exact` | 상태 matrix240개(14수락/226거절), namespace12개(3수락/9거절) |
| `generated_transaction_records_roundtrip_without_effects` | seed0..63 ×각128개 =8,192개 유효 값 roundtrip·wire field 독립 검산 |

길이 corpus의 분모는 `40+48+48+56+56+40+6×192 + 12 = 1,452`다. decoder가 실패한 것을 encoder 성공 증거로 세지 않는다. encoder 실패 검사는 서로 다른 sentinel bytes를 가진 전체 output이 그대로인지 확인한다.

N01–N40은 아래 base literal 한 필드만 변경한다. 지정된 변경 외에는 bytes를 유지하며 모든 항목은 Corrupt, N22만 UnsupportedFormat이다. constructor와 encoder가 받을 수 있는 잘못된 TxnId/sequence/slot 입력은 별도로 InvalidInput과 output 보존을 확인한다.

| ID | base | 단일 손상 |
| --- | --- | --- |
| N01 | V01 | DB16 bytes zero |
| N02 | V01 | timeline0 |
| N03 | V01 | writer_epoch0 |
| N04 | V01 | local0 |
| N05 | V01 | reserved offset28=1 |
| N06 | V02 | generation0 |
| N07 | V03 | mutation_sequence0 |
| N08 | V06 | FileId epoch0 |
| N09 | V06 | FileId counter0 |
| N10 | V06 | page incarnation0 |
| N11 | V06 | slot82 |
| N12 | V06 | reserved offset26=1 |
| N13 | V06 | generation0 |
| N14 | V04 | stamp tag0 |
| N15 | V04 | stamp tag3 |
| N16 | V04 | reserved offset1=1 |
| N17 | V04 | q0 |
| N18 | V04 | inactive payload offset16=1 |
| N19 | V05 | embedded TxnId local0 |
| N20 | V05 | embedded TxnRef generation0 |
| N21 | V07 | magic offset0=0 |
| N22 | V07 | format version2 |
| N23 | V07 | state tag5 |
| N24 | V07 | rollback kind3 |
| N25 | V07 | flags bit3=1 |
| N26 | V07 | reserved offset65=1 |
| N27 | V07 | absent head bytes offset72=1 |
| N28 | V07 | absent target bytes offset112=1 |
| N29 | V07 | absent next bytes offset152=1 |
| N30 | V08 | present head40 bytes zero |
| N31 | V08 | present target40 bytes zero |
| N32 | V08 | present next40 bytes zero |
| N33 | V07 | embedded TxnId local0 |
| N34 | V07 | slot generation0 |
| N35 | V12 | free TxnId region offset8=1 |
| N36 | V12 | free generation0 |
| N37 | V12 | free flags head bit=1 |
| N38 | V12 | free q22 |
| N39 | V12 | free rollback kind1 |
| N40 | V12 | free absent next body offset191=1 |

L01–L24의 base는 DB1–16/timeline17의 page, status location slot5/generation20, slot0–4의 Free와 slot5의 V07이다. full page identity의 owner/owner generation은 고정 nonzero 값이다. semantic 손상 page는 유효 digest를 새로 계산하여 checksum 실패가 목표 검증을 대신하지 않게 한다. L01–L22는 IdentityMismatch이며 L09만 KindMismatch, L23/L24는 Corrupt다.

| ID | 변경하는 하나의 경계 |
| --- | --- |
| L01–L02 | supplied namespace DB / timeline 불일치 |
| L03–L08 | expected page의 DB / FileId / page_no / incarnation / owner / owner generation 불일치 |
| L09 | actual page kind=Undo |
| L10–L14 | supplied StatusSlotRef의 FileId / page_no / incarnation / 존재하지 않는 slot6 / generation 불일치 |
| L15–L19 | expected TxnRef의 DB / timeline / writer_epoch / local / generation 불일치 |
| L20–L21 | selected occupied cell의 유효한 다른 TxnId / 다른 generation |
| L22 | selected cell을 같은 generation의 Free로 변경 |
| L23 | selected cell length191 |
| L24 | unselected Free cell의 reserved offset65=1 |

상태 matrix는 tag0..4 × rollback kind0..2 × q{0,22} × head presence2 × target presence2 × next presence2 =240이다. Free tag는 TxnId region zero, occupied tag는 V02의 TxnId/generation을 사용한다. 존재하는 head/next는 V08의 값, target은 V08의 target이며 absent field는 zero다. 후보 constructor/decoder와 독립인 위 허용표의 boolean 식으로 oracle14개를 계산한다. Free1·Active2·Committed2·Aborted2·Aborting Statement4·Aborting Transaction3이다. matrix 손상226개를 N40에 중복 합산하지 않는다.

namespace matrix는 DB{일치,다름} × timeline{17,18} × historical writer_epoch{1,18,u32::MAX} =12다. expected namespace는 DB1–16/timeline17이며 epoch3개 모두 같은 namespace일 때 수락한다. 현재 writer_epoch를 비교하거나 과거 epoch를 손상으로 취급하는 구현은 이 matrix에 실패해야 한다.

생성 corpus는 seed0..63에서 시작하는 명시적 LCG `x = x×6364136223846793005 + 1442695040888963407 (mod 2^64)`로 각 seed128개를 만든다. 각 iteration은 x를 한 번 갱신하고 nonzero ID/sequence는 `max(x,1)`, epoch는 `max(low32(x),1)`, page_no=x, valid slot은 `x mod 82`, status shape는 `iteration mod 14`의 위 허용 shape 순서로 선택한다. DB는 기본 literal DB를 유지한다. 각 field별 wire offset을 candidate decoder와 독립적으로 읽어 검산하고 roundtrip equality도 검사한다. 생성 corpus는 literal·고정 negative를 대체하지 않는다.

이 시험은 표현과 선택한 page의 identity 검증까지만 인수한다. mutation_sequence 실제 발급/overflow 처리, directory의 crash atomicity, status reference pin 수명, 각 logical version의 row/index 결합, CLR/aging/회수 및 복구 obligation의 실제 동작은 해당 후속 leaf와 모델·실행 증거가 필요하다.

국소 실행은 고정8 tests와 engine Clippy를 통과했다. 첫 실행은 V07/V11의 hex 전사에 여분 zero가 들어가 fixture parser에서 실패했고, byte 배치·상태표·분모를 유지한 채 두 literal 전사만 정정하여 재실행했다. 실패/성공 당시 source·contract 사본과 원로그는 각각 보존하며, 실행 identity와 통합 검토는 [구현 기록](../implementation/dev3-implementation.md)에 연결한다. 이 문단과 상태 헤더는 결과 기록이며 실행 당시 계약 hash를 소급 변경하지 않는다.

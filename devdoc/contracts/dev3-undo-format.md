# DEV3 undo record·before-image chunk 표현

2026-10-21 · **D3-UNDO-F01 상세 계약·독립 vector 수용 / 작성자 고정12시험·Clippy PASS / 최종 독립 검토·통합 인수 미완료**

[undo·aging](disk-storage-design.md#undo) · [참조 수명](disk-storage-design.md#undo-lifecycle) · [변경 그룹](disk-storage-design.md#mutation-protocol) · [복구 예약](disk-storage-design.md#recovery-reservation) · [M01/M02](dev3-undo-model.md) · [M04](dev3-recovery-model.md) · [참조](dev3-reference-format.md) · [거래](dev3-transaction-format.md) · [row/index](dev3-row-format.md) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 계약은 **row/index undo와 그 before-image의 무효과 표현**을 제안한다. catalog target identity까지 표현하지만 catalog before body는 `UnsupportedCatalogBody`로 거절한다. catalog 복구·전체 F01·S1의 완료가 아니다. I/O, pin/lock, undo 발급·적용, incoming count 변경, cut, queue 실행, slot 재사용, WAL/CLR, quota/obligation 복원은 실행하지 않는다. 기존 DEV2와 이미 동결한 PageRef/UndoRef/TxnRef/row/index bytes를 변경하지 않는다.

`/root`는 두 previous edge에 별도 expected sequence를 두는 선택, row before의 별도 undo chunk, own/resolved before stamp, catalog body 미지원 경계와 아래 상세 offset·API·고정 corpus를 독립 검토하여 수용했다. 제품 작성 전 독립 raw vector/hash도 수신 검토했다. 제품 표현과 고정12개 시험을 작성했으며, 실행한 정확한 source와 남은 검토는 아래 실행 상태로 구분한다.

## 1. Identity와 두 edge

`UndoSequence`는 DB/timeline namespace 안에서 undo record마다 발급하는 nonzero u64다. 사용자 mutation sequence와 별개이며 한 사용자 행 변경이 row와 여러 index undo를 만들면 각각 다른 UndoSequence를 가진다. 한 거래의 여러 undo가 같은 사용자 mutation sequence를 공유할 수 있다. sequence를 물리 page/slot 주소에서 계산하지 않는다.

발급은 namespace 전체의 durable high-water를 사용하고 증가/예약 산술이 넘치면 첫 효과 전에 거절해야 한다. 그 발급·high-water 영속 표현은 이 leaf에 없다. `MAX`는 읽을 수 있는 값이며 다음 발급이나 wrap을 허용하지 않는다.

각 previous edge의 값은 `UndoLink { reference: UndoRef, sequence: UndoSequence }`의 정확히48 bytes다. 처음40 bytes는 기존 UndoRef, 뒤8 bytes는 expected sequence다.

| role | 연결 대상 | load 뒤 추가 대조 |
| --- | --- | --- |
| previous-transaction | 같은 creator의 바로 앞 undo; target row/index는 달라도 됨 | 실제 sequence 일치, creator의 TxnId와 원래 status generation 일치, 대상 mutation sequence가 source 이하 |
| previous-version | 같은 논리 target의 이전 version을 제거하는 undo | 실제 sequence 일치, target kind/owner/generation/entity/full key 일치, 대상의 creator TxnId+mutation sequence가 source before logical identity와 일치 |

Linked edge의 sequence는 source sequence보다 **엄격히 작다**. 실제 load에서도 expected sequence와 target의 실제 sequence를 비교하므로 위조한 낮은 숫자만 저장해 cycle을 통과시킬 수 없다. validated view는 decode 때 검증한 실제 UndoRef와 Namespace context를 보존한다. `validate_loaded_edge`는 그 target view의 실제 UndoRef가 source에 저장된 reference와 정확히 같고 namespace도 source와 같음을 먼저 확인한다. generation·Live 상태도 다시 확인한다. 자기 `(FileId,page_no,slot)`을 가리키면 page incarnation이나 slot generation을 다르게 써도 거절한다. 모든 loaded edge가 실제로 감소하면 cycle이 불가능하다. 전역 sequence 유일성·target 존재·모든 graph edge의 검사는 local decoder 하나가 증명하지 않는다.

두 role은 별도 영속 field다. 동일 UndoRef/sequence를 가리켜도 **두 edge로 열거**한다. 중복 제거한 set으로 incoming count를 계산하지 않는다. previous-version의 현재 값만 rollback 복원 후보이며 before payload 안에 별도 옛 UndoRef를 남기지 않는다. cut 전 link를 되살리는 경로를 만들지 않는다.

creator는 TxnId40과 원래 status generation8을 보존한다. 이는 불변 **생성 identity**이며 status lookup edge가 아니다. creator에서 status를 load하는 API는 제공하지 않는다. 실제 before stamp가 `TxnRef`일 때만 그 field가 별도 status 수명 edge다. 이 구분 덕분에 terminal aging 뒤 creator를 식별하면서도 이미 해제한 status slot을 영구 보존하거나 재조회하지 않는다.

## 2. 논리 target와 before version

target은 아래32-byte core와 optional key를 갖는다. standalone target codec의 전체 길이는 `32+K`다. root record에서는 core를 고정 header 안에, key bytes를 header 뒤에 둔다. 정수는 별도 표기가 없으면 little endian이다.

| target offset | bytes | 값 |
| --- | --- | --- |
| 0 | 1 | Row1, Index2, Catalog3 |
| 1 | 1 | index arity1–4; Row/Catalog는0 |
| 2 | 2 | K: index full physical key 길이9–1,032; Row/Catalog는0 |
| 4 | 4 | reserved0 |
| 8 | 8 | target owner ObjectId, nonzero |
| 16 | 8 | target owner Generation, nonzero |
| 24 | 8 | Row/Index의 RowId 또는 Catalog의 entry ID, nonzero |
| 32 | K | index의 검증된 PhysicalKey; 마지막 RowId는 기존8-byte BE |

Row owner는 table, Index owner는 index, Catalog owner는 catalog namespace/object다. catalog entry ID는 catalog tree의8-byte BE key를 구성하는 nonzero u64다. 이름을 table/index identity로 대신하지 않는다. root의 creator DB/timeline이 namespace를 제공하고, 기대 namespace·owner·generation·entity·index arity/full key는 caller의 안정화된 context에서 받는다. 저장값으로 기대값을 다시 만들어 자신과 비교하지 않는다.

index key는 기존 PhysicalKey decoder로 canonical scalar·arity·1,024-byte composite 한도·RowId suffix를 확인한다. suffix RowId와 target entity는 같아야 한다. standalone Catalog target은 정상적으로 검증할 수 있지만 **Catalog target의 occupied undo record는 body 종류와 무관하게 UnsupportedCatalogBody**다. Missing·retire만 성공시키거나 opaque bytes를 catalog before-value로 승인하지 않는다.

| before kind | tag | target | stamp/logical | canonical value |
| --- | --- | --- | --- | --- |
| Missing | 0 | Row 또는 Index | 둘 다 zero | 없음 |
| RowDeleted | 1 | Row | 있음 | 없음 |
| RowValue | 2 | Row | 있음 | 5–131,072 bytes |
| IndexAbsent | 3 | Index | 있음 | 없음 |
| IndexPresent | 4 | Index | 있음 | 없음 |

Missing은 이전 physical entry 자체가 없던 상태다. RowDeleted와 IndexAbsent는 이전 entry가 존재하며 그 version identity를 보존하는 상태다. 이 차이를 상태 tag 하나로 지우지 않는다. target kind와 before kind의 조합이 undo 종류를 정한다. Missing의 previous-version은 Never여야 한다. row/index를 복구할 때 이전 entry를 제거할지 versioned tombstone을 남길지는 별도 WAL/visibility 실행 계약이며 codec이 tree를 변경하지 않는다.

before stamp는 `Committed(q)` 또는 **creator의 TxnId와 원래 status generation이 모두 같은 own TxnRef**만 허용한다. 다른 거래의 TxnRef를 unresolved 상태로 복제하지 않는다. Committed의 q는 nonzero이며 before LogicalRowVersion의 DB/timeline도 root namespace와 같아야 한다. own stamp의 TxnId는 before logical TxnId와 같아야 한다. before logical의 TxnId가 creator와 같으면 그 mutation sequence는 현재 root의 mutation sequence보다 작아야 한다. commit-q stamping은 logical identity를 바꾸지 않는다.

현재 실행의 writer epoch와 과거 creator/before epoch의 일치를 요구하지 않는다. 같은 bytes의 사용자 UPDATE에도 새 mutation identity가 필요하고 undo는 저장한 이전 identity를 복원한다. 하나의 논리 mutation 내부에서 같은 entry를 여러 차례 물리 변경하는 경우 중간 단계를 새 사용자 version으로 가장하지 않고 해당 실행 owner가 합쳐야 한다.

row before에는 기존 CanonicalRow bytes를 저장한다. 기존 `decode_canonical_row`와 supplied Column schema로 type/nullability·value64 KiB·row128 KiB를 검증한다. row OverflowHead를 복제하지 않으므로 별도 row-overflow chain의 수명 edge는 이 record에 생기지 않는다. row/index의 stamp·logical/key codec은 재사용하지만, 기존 RowRecord/IndexRecord 전체를 중첩 저장하여 그 안의 undo pointer를 세 번째 edge로 만들지 않는다.

이 값 복사는 기존 [row overflow 보존](disk-storage-design.md#tree)의 horizon 의무를 없애지 않는다. 교체된 old row-overflow chain의 head/allocator 소유 inventory는 별도 logged retirement owner가 보존하고, 필요한 snapshot/rollback 소유가 끝난 뒤 반환해야 한다. 이 root가 old OverflowHead를 저장하지 않는다고 old chain을 즉시 반환하거나 소유자를 잃어도 되는 것은 아니다. 해당 retirement record의 bytes·effect와의 원자 결합은 후속 미완료 의무다.

## 3. Root record와 slot

root cell은432-byte header, target key K bytes, optional inline value L bytes 순이다. 전체 길이는 정확히 `432+K+L`이다. 별도 cell checksum은 없고 공통16 KiB page checksum과 WAL frame checksum을 소비한다.

| cell offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | `XUR1` |
| 4 | 2 | format1 |
| 6 | 1 | Free0, Live1, Releasing2 |
| 7 | 1 | reserved0 |
| 8 | 8 | slot generation, nonzero |
| 16 | 8 | UndoSequence; occupied는 nonzero |
| 24 | 40 | creator TxnId |
| 64 | 8 | creator의 원래 status generation |
| 72 | 8 | creator의 사용자 mutation sequence, nonzero |
| 80 | 8 | stored incoming-reference count |
| 88 | 32 | target core |
| 120 | 48 | previous-transaction link |
| 168 | 48 | previous-version link |
| 216 | 56 | before VersionStamp |
| 272 | 48 | before LogicalRowVersion |
| 320 | 4 | 전체 canonical before value bytes; 값 없으면0 |
| 324 | 1 | before kind0..4 |
| 325 | 1 | value storage: None0, Inline1, Chunks2 |
| 326 | 1 | previous-transaction edge state |
| 327 | 1 | previous-version edge state |
| 328 | 1 | release cursor0..3 |
| 329 | 3 | reserved0 |
| 332 | 2 | 이미 반환한 chunk 수 |
| 334 | 2 | reserved0 |
| 336 | 64 | Chunks의 UndoPayloadHead; 그 외 zero |
| 400 | 24 | release 작업의 다음 chunk PageRef; 비활성/완료이면 zero |
| 424 | 8 | reserved0 |
| 432 | K | target physical key |
| 432+K | L | inline canonical before value |

Free는 정확432 bytes이며 magic/version/generation 외 모든 byte가0이다. `decode_undo_record`의 inspection은 Free를 반환할 수 있지만 `lookup_undo_record`는 같은 generation이어도 Free를 StateMismatch로 거절한다. 이전 점유자의 generation을 보존하고 재점유 때 checked g+1을 사용한다. 최초 새 slot의 g=1 초기화와 실제 재사용은 allocator/WAL owner의 책임이다. generation MAX는 표현 가능하지만 재점유 불가다. slot directory index는 삭제/compaction 때도 움직이지 않으며 tombstone을 제거하여 뒤 slot을 당기지 않는다.

root record는 kind8 Undo·level0 page의 cell이다. slot당 최소 비용은432+4=436이므로 root page는 최대37 slots다. 기존 UndoRef의 더 넓은 보수적 slot 범위를 변경하지 않고, 실제 root lookup에서 slot<37 및 page의 실제 slot 수를 모두 검사한다. cell offset 이동은 가능하지만 UndoRef의 page/slot/generation은 그대로 유지한다.

공통 page decoder의 성공만으로 모든 root cell의 schema/target 의미가 검증됐다고 하지 않는다. 선택한 cell은 `decode_undo_record`로 추가 검증한다. root page에 보관하는 cell은 XUR1 root만이며 XUC1 chunk와 혼합하지 않는다. page를 작성하는 상위 owner는 모든 cell의 검증된 root bytes를 준비한 뒤 기존 page encoder를 호출한다. 이 leaf는 schema가 서로 다른 전체 page를 검증한 것처럼 표시하는 새 우회 API를 만들지 않는다.

RowValue는 total5–2,048이면 Inline이고 L=total이다. total2,049–131,072이면 Chunks이며 L=0이다. 다른 before kind는 None/total0/L0이고 head도 zero다. Index의 K≤1,032와 Row의 L≤2,048은 서로 배타적이므로 최대 root는 **2,480 bytes**다. 두 상한을 더한 가짜 최대값을 사용하지 않는다.

## 4. Cut와 release cursor

| edge state | tag | link bytes | 의미 |
| --- | --- | --- | --- |
| Never | 0 |48 zero | 처음부터 없는 edge |
| Linked | 1 | 유효 UndoRef/sequence | 현재 incoming count에 반영되는 edge |
| Cut | 2 |48 zero | logged cut으로 이미 제거한 edge |
| Released | 3 |48 zero | Releasing cursor가 이미 처리한 role |

Live는 cursor0/chunks-released0/release-next zero이고 Released edge를 허용하지 않는다. stored incoming은0도 가능하다. 그것만으로 회수 권한이 생기지는 않는다. Cut→Linked 복원이나 Active 거래의 rollback에 필요한 previous-transaction cut의 정당성은 이 codec이 판단하지 않는다. runtime은 horizon/rollback/현재 link를 검산하고 해당 count·queue 변경과 원자 그룹을 만들어야 한다.

Releasing은 incoming0이며 새 edge의 target이 될 수 없다. root의 생성 identity·sequence·target·before logical·body는 처리 중에도 유지한다. cursor는 M01과 같은 고정 순서를 사용한다.

| cursor | previous-version | previous-transaction | before stamp |
| --- | --- | --- | --- |
| 0 | Never/Linked/Cut | Never/Linked/Cut | 기존 유효 값 |
| 1 | Released/zero | Never/Linked/Cut | 기존 유효 값 |
| 2 | Released/zero | Released/zero | 기존 유효 값 |
| 3 | Released/zero | Released/zero |56 zero |

없는 edge도 한 role을 처리했다는 cursor 진행으로 표시한다. cursor0→1,1→2는 해당 edge/count 변경과,2→3은 own-before status 참조 제거와 같은 WAL 그룹이어야 한다. 원래 Committed/Missing이어서 status edge가 없던 경우에도 cursor3에서 stamp region을 zero로 만든다. before logical은 진단 identity로 남지만 Releasing view에는 reader/rollback용 before accessor가 없다. 처리 뒤 남은 raw metadata로 status lookup이나 version 복원을 허용하지 않는다.

chunk가 없으면 chunks-released0/release-next zero다. chunk가 있고 cursor<3이면 둘 다0이다. cursor3이 되는 그룹은 release-next를 head.root로 설치한다. 그 뒤 `0≤released<count`이면 release-next가 유효하고, `released=count`이면 zero다. released0의 next는 head.root와 정확히 같고, released>0의 next는 root 및 root record page와 같은 물리 주소일 수 없다. released>count는 거절한다.

chunk 반환은 현재 chunk의 정확한 identity/header/index를 먼저 검산한 뒤 allocator 반환·next 인계·released+1을 한 그룹으로 처리한다. 앞 chunk를 반환한 뒤 그 page를 다시 읽어 다음 위치를 찾지 않는다. 마지막에는 next zero와 count 완료가 함께 남는다. root의 원래 head는 진단용 descriptor이며 Releasing의 이동 소유자는 release-next다. cursor3/count완료라도 queue 소유·pin·DWB retirement와 모든 관련 참조 제거 검증 없이 root slot을 Free로 바꿀 수 없다.

stored incoming은 외부 row/index header, transaction head, 다른 root의 두 Linked edge 등을 **중복도 그대로** 센 값이다. before-status edge와 chunk page 소유는 별도 자원이다. horizon·rollback·queue·transient pin을 incoming 숫자에 합치지 않는다. codec의 `linked_edges()`는 두 role을 순서대로 열거하며 동일 대상도 두 번 반환한다. count 덧셈/뺄셈·slot 세대 계산 helper는 checked 값 계산만 하고 저장소를 바꾸지 않는다.

## 5. Undo 전용 chunk

UndoPayloadHead는 정확64 bytes이며 row OverflowHead와 다른 타입이다.

| head offset | bytes | 값 |
| --- | --- | --- |
| 0 | 24 | root chunk PageRef |
| 24 | 4 | canonical before total2,049–131,072 |
| 28 | 2 | count=ceil(total/16,060),1..9 |
| 30 | 2 | reserved0 |
| 32 | 32 | 전체 canonical before bytes의 SHA-256 |

row OverflowHead와의 암묵적 타입 변환을 제공하지 않는다. 다만 일부 작은 total에서 두64-byte head가 같은 bytes일 수 있으므로 head 단독으로 domain을 식별했다고 주장하지 않는다. chunk count 계산과 실제 page kind/magic, root-record 결합, namespace/owner를 함께 확인해야 한다. head.root는 root record page와 같은 물리 주소이면 incarnation과 무관하게 거절한다.

chunk는 **kind8 Undo·level0 page의 유일한 cell**이며 아래160-byte header와 payload를 가진다. page envelope의 owner/generation은 undo store의 owner다. target table/index owner를 envelope owner로 대신하지 않는다.

| chunk cell offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | `XUC1` |
| 4 | 2 | format1 |
| 6 | 2 | chunk index0..count−1 |
| 8 | 2 | head count |
| 10 | 2 | reserved0 |
| 12 | 4 | head total |
| 16 | 4 | payload bytes |
| 20 | 4 | reserved0 |
| 24 | 40 | 소유 root record의 정확한 UndoRef |
| 64 | 8 | 소유 root record의 UndoSequence |
| 72 | 24 | head.root |
| 96 | 24 | 마지막은 zero; 그 외 next PageRef |
| 120 | 32 | head digest |
| 152 | 8 | 소유 root record의 timeline, nonzero |
| 160 | payload bytes | canonical before의 해당 연속 구간 |

최대 payload는16,224−4−160=**16,060 bytes**다. 마지막 외 payload는 정확16,060이고 마지막은 `total−index×16,060`이다. index0의 현재 PageRef는 head.root와 같아야 한다. 뒤 current는 root/record page와 같은 물리 주소이면 거절한다. next는 current/root/record page와 같은 물리 주소이면 incarnation이 달라도 거절한다. 중간 missing next·마지막 nonzero next도 거절한다.

expected chunk context는 이미 검증한 root record에서 얻은 namespace, undo store owner/generation, root UndoRef/sequence, head다. 저장 chunk 자체의 필드로 기대 context를 만들지 않는다. page decoder가 DB/PageRef/owner/kind/level을 확인하고 cell decoder가 timeline·record identity·sequence·head·index를 대조한다. 공통 page header에는 timeline이 없으므로 cell의 명시 timeline과 root creator의 timeline을 함께 확인한다.

`UndoChunkVerifier`는 최대9개 방문 물리 주소와 streaming SHA-256만 유지한다. 순서·정확한 next·중복 주소·전체 byte 수·digest를 모두 확인하고 finish는 count개 완료 후만 성공한다. 오류에서 verifier 상태를 보존한다. materialization은 caller의 별도 예약된 buffer에 하고, `decode_materialized_before`가 total/digest와 기존 CanonicalRow schema/type을 함께 검증한다. chunk bytes만 맞다고 row type까지 검증했다고 하지 않는다.

Releasing의 `validate_release_chunk`는 durable released/index와 next를 기준으로 해당 한 chunk를 확인한다. 이미 반환한 앞 chunk를 순회하거나 전체 digest를 다시 계산할 수 있다고 주장하지 않는다. 최초 chain 공개의 전체 검산, 각 page checksum·owner binding, allocator의 정확한 PageRef 소유와 원자 cursor 인계가 별도 선행조건이다. index가 증가하고 남은 count가 줄어드는 표현과 actual exactly-once 반환은 구별한다.

## 6. API와 오류

후속 구현 후보 모듈은 private `storage::undo_record`다. 새 crate·public re-export·production model import가 없다. 구현 파일/등록은 owner 검토 뒤 따로 배정한다. 모든 encoder는 caller buffer를 사용하고 첫 쓰기 전에 전체 입력·shape·capacity를 검산하며 실패 시 output 전체를 보존한다. prefix encoder는 성공 tail도 보존한다. heap·page/frame-size scratch·I/O가 없다.

| API | 책임 |
| --- | --- |
| `UndoTargetView::decode`, `encode_undo_target` | exact32+K와 supplied target 기대값 대조. Catalog target 자체는 지원 |
| `UndoLink`, `UndoEdge::{Never,Linked,Cut,Released}` |48-byte 값과 role별 현재 상태; reference 존재/수명은 인증하지 않음 |
| `UndoCreator { id, status_generation }` | 생성 identity 값; status lookup capability 아님 |
| `UndoRecordExpectation { namespace, reference, target }` | 신뢰하는 context; page load에는 별도 undo store PageIdentity도 필요 |
| `undo_record_encoded_len`, `encode_undo_record`, `decode_undo_record` | exact root cell, 상태·before·edge·sequence·target·schema 검산 |
| `UndoCell::{Free,Live,Releasing}`의 borrowed view | `state()`와 `generation()`은 Free도 inspection 가능. `metadata()`는 점유 record의 creator/sequence/mutation/incoming/target만 반환하며 Free를 거절. `release_progress()`는 Releasing의 cursor/released/next와 총 chunk 수만 반환하며 Free/Live를 거절. Live만 before reader 접근 |
| `lookup_undo_record(page,expected_identity,expectation,columns)` | kind8 root page identity·slot/실제 길이·slot generation과 선택 cell 검증; 다른 cell의 schema 완료를 주장하지 않음 |
| `linked_edges()`, `before_status_ref()` | 현재 role별 참조 열거. 중복 보존; cursor 처리된 role은 반환하지 않음 |
| `validate_loaded_edge(source,role,target)` | 두 검증된 record 사이 actual sequence·Live/namespace·creator 또는 target/before identity 확인. load/pin 자체 없음 |
| `UndoPayloadHead::{new,encode,decode}` |64-byte 값과 count/total 검사 |
| `encode_undo_chunk_cell`, `encode_undo_chunk_page`, `decode_undo_chunk_page` | caller cell/page와 expected owner context, exact chunk shape |
| `UndoChunkVerifier::{push,finish}` | 최대9-page bounded 검산; 오류 시 verifier 보존 |
| `decode_materialized_before`, `validate_release_chunk` | 각각 live canonical value 또는 release 단계의 한 chunk 검증 |
| checked incoming increment/decrement, `next_slot_generation` | overflow/underflow/Free 사전조건 검사만 하는 무효과 산술 |

오류는 InvalidInput, NoSpace, TooLarge, Corrupt, UnsupportedFormat, UnsupportedCatalogBody, IdentityMismatch, SequenceMismatch, StateMismatch, SchemaMismatch, Incomplete, Exhausted로 구분한다. wire version/tag/길이와 shape 손상은 Corrupt 또는 UnsupportedFormat, 유효한 기대값과 identity/sequence/state 차이는 대응 오류다. count underflow·overflow와 generation wrap은 정상 성공이 아니다. caller/schema 불일치와 손상 우선순서의 일반적 총순서는 약속하지 않되 아래 고정 부정 corpus에서는 각 손상을 단독 주입한다.

## 7. 실제 byte 상한과 남은 의무

| 항목 | exact bound |
| --- | --- |
| root fixed / minimum slot cost / max slots |432 /436 /37 |
| Row inline root |432+L,5≤L≤2,048; 최대2,480 bytes |
| Row chunked root |432 bytes; head64는 이미 header에 포함 |
| Index root |432+K,9≤K≤1,032; 최대1,464 bytes |
| chunk cell / page |160+payload≤16,220 /16,384 bytes |
|128 KiB before chunks |9개; 앞8개16,060 bytes, 마지막2,592 bytes |
| 최대 before의 root+chunk cell/slot 합 |436 +131,072 +9×(160+4) =132,984 bytes |
| 독립 새 page가 필요할 때 한 최대 before | root1+chunk9=10 pages=163,840 bytes |
| 그10개 page INIT만 담은 WAL frame 예 |192+10×(64+16,384)=164,672 bytes |

마지막 두 줄은 한 undo의 보수적인 저장 입력이다. 여러 record의 root page 공유를 확인하기 전 감소시키지 않는다. 이것만으로 한 row mutation/transaction의 F05 예약이 완성되지 않는다. 실제 opcode는 최대8 index의 old/new presence, tree split/anchor, 두 previous target count, before-status, status head/progress, allocator token, queue 진행점, old row-overflow의 보존 inventory와 이미 charge된 page, redo-only CLR, terminal 및 영속 obligation bytes를 추가해야 한다. cell/slot bytes와 파일/extent64-page 성장 bytes를 중복 집계하지 않는다.

transaction status192는 recovery reservation descriptor를 포함하지 않는다. undo header에도 그 descriptor를 opaque 필드로 넣지 않는다. effect·새 undo·두 edge/count·status head/progress·잔여 obligation/예약 ownership은 같은 완결 WAL/CLR 그룹으로 인계되어야 한다. BEGIN WAL 삭제 뒤 undo/rollback 가능성, 같은 prefix의 checkpoint/ATT/allocator 복원, bootstrap/rebase와 반복 crash peak는 여전히 [별도 의무](disk-storage-design.md#recovery-reservation)다. M01/M02의 graph와 M04의 원자 전이가 이 byte codec의 실행을 대신하지 않는다.

## 8. 독립 vector 입력과 사전 고정 corpus 제안

기본 namespace는 DB bytes1..16/timeline17이다. creator TxnId는 epoch18/local19, 원래 status generation20, 현재 mutation sequence21, UndoSequence100, 현재 root UndoRef는 file1:2/page30/incarnation4/slot0/generation6이다. undo store owner9/generation10과 target owner5/generation6은 의도적으로 다르다. target entity는258이다.

기본 previous link는 기존 참조 vector(file1:2/page3/incarnation4/slot5/generation6)에 sequence99를 붙인다. duplicate fixture에서는 두 role이 이 값과 정확히 같다. own before는 creator TxnRef와 logical mutation20, committed before는 q22와 namespace가 같은 TxnId(epoch17/local23)/mutation24다. index arity1/full key는 Bool(true),RowId258의 `01010000000000000102`다.

inline C10은 [row literal](dev3-row-format.md)의31 bytes다. 기본 큰 before는 한 Binary 열의 길이2,040으로 canonical2,049 bytes이며 payload byte i=`(17×i+3) mod256`이다. 기존 독립 SHA-256 `bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd`와 같다. chunk root는 file1:2/page40/incarnation4, 뒤 chunk는 page41부터 증가한다. 두-chunk release fixture는 Binary16,052의 canonical16,061 bytes와 같은 payload 식을 사용한다.

| vector | 고정 표현 | bytes |
| --- | --- | --- |
| U01 | Free generation6 |432 |
| U02 | Live Row Missing, incoming1, 두 edge Never |432 |
| U03 | Live RowDeleted, committed before, incoming1, 두 edge Never |432 |
| U04 | Live RowValue inline C10, own before, incoming5, 두 edge Linked duplicate |463 |
| U05 | Live RowValue Chunks2,049, committed before, incoming1, txn Linked/version Cut |432 |
| U06 | Live Index Missing, incoming1, txn Linked/version Never |442 |
| U07 | Live IndexAbsent, committed before, incoming1, 두 edge Never |442 |
| U08 | Live IndexPresent, own before, incoming2, 두 edge Linked duplicate |442 |
| U09–U12 | U04의 Releasing/incoming0, cursor0/1/2/3의 상태표 적용 | 각463 |
| U13–U15 |16,061-byte Chunks Releasing/cursor3, released0/1/2와 next40/41/zero | 각432 |
| U16 | U05의 UndoPayloadHead |64 |
| U17 | Row target standalone |32 |
| U18 | Index target standalone |42 |
| U19 | Catalog target standalone; body 성공 vector가 아님 |32 |
| U20 | U05의 유일한 chunk cell |2,209 |

20개 합계는9,044 bytes다. root page golden은 U04 단일 cell과 pageLSN25, chunk page golden은 U20 단일 cell과 pageLSN25를 사용한다. 둘 다 DB/undo store owner9/10이며 page는 각각30/40이다.

독립 vector는 이 표·offset에서 Python struct/hashlib로 직접 생성했으며 candidate codec을 호출하지 않았다. `artifacts/dev3/undo-format-preimplementation/`에 `vectors.py`, `vectors.json`, `U01.bin`–`U20.bin`, `root-page.bin`, `chunk-page.bin`, canonical 원자료3개와 `generation.log`를 보존했다. `/root`의 수신 검토는 `artifacts/dev3/undo-contract-root-review-r1/review.json`에 보존되어 있다. 이 vector 생성·수신 검토는 제품 검증 PASS를 대신하지 않는다.

생성 script SHA-256은 `c307cdea7c31dd3a154217de33484ef92ca29977532bcb8247b79192fd032d98`, full hex를 포함한 `vectors.json`은 `5df9e0f996611287fa18a442bdab6867def16056583e679189a8982c8b058cb8`이다. 생성 당시 계약 `4adc7aeb6791be707de6a4b0f9f60df98d5ffca738d51c5e36d63c053857f6c9`를 `contract-at-vector-generation.md`에 보존했다. 이후 이 단락의 실제 상태·hash 연결은 그 실행 identity를 소급 변경하지 않는다.

| vector | bytes | SHA-256 |
| --- | --- | --- |
| U01 | 432 | `f897e222e7033965c225fe973cec4af0db948bff5a342dce86d36bc286819be7` |
| U02 | 432 | `a093939a406f521e17f54d4f54e9cbee936c85a4dc849d0955df1511cfb38c5a` |
| U03 | 432 | `7cd841f7a463827643cc469798dcda7eb8abc01cdcf59b53caba3863459d7bb7` |
| U04 | 463 | `c267a18f8ac192c9e4e9908ba54695c18854abe1a11e2fc9a7f88e589269f8be` |
| U05 | 432 | `ddd0de056440803507bc055f3b7ccd81b91f880a772e23bd9c982b1d94cc2c2c` |
| U06 | 442 | `d63139c656444b38c85b0ff4e9b205910395669799a168fa167f4d11fad19ded` |
| U07 | 442 | `34ba5452c0add1f4b3b25cd49be611554d2c5347b2c7924e695d52b7f9f1c166` |
| U08 | 442 | `84bea8378fa74d5830e29c3177c778da2acbc4ba54eaf34ab67a57e0a6e04c3d` |
| U09 | 463 | `9b4d53d36320daca7f19edde090abad813d828deb5e3e3ea267e67a321ff5159` |
| U10 | 463 | `9968132d93477fef4c84626e6b1d3f7ac058337a0f5277d622d49b22ad81a03b` |
| U11 | 463 | `b4ff6ce855858e8e099b4c0891436767378e3035302c73b0535a45e541b9656a` |
| U12 | 463 | `e27fbe50a89eba49508af52d4f9747e05077b241b786c19f5d73b67603afa286` |
| U13 | 432 | `04628be9babed93325eed27f0f77f0e183d60679de90e6813f2f26423365f6da` |
| U14 | 432 | `7bb87e5c9855548ae38462971a4d3b022cbf4b9c1f009b1a07dfe157ba90b468` |
| U15 | 432 | `1f70511604c79dc5bd7faaf3b1ce147cb3ffe967c4b67e4083c3ea79b81f4ea6` |
| U16 | 64 | `dc0f4edfa69b72d3d4fcc816b408f62ad9d7e4b3373949e65e80adaf9cfda523` |
| U17 | 32 | `b0dead0f7d06b790a670262d3335a713eee94ceb5367a30014c52cd1fbe0ae00` |
| U18 | 42 | `8fc4cf31a0929fc00ea8a7127bc8e65dc111ce0f7e64f6ffe095e4190c93651a` |
| U19 | 32 | `09398427aff017090632f025e984446b067287a7920aa09a1c212c20c922d184` |
| U20 | 2209 | `2221bcbac40ed59d5849c11eeb1b05376747052b581af53bed838a49473ea77e` |
| root page | 16384 | `f3a4e11d13bda7c94167632071c86bc85fd41e8c8bc9342dc865b7a0f9ba7580` |
| chunk page | 16384 | `f733be862715750ff978a1b37108984caeed2b76c5e3607796fbeac00304e4ba` |

아래12개 unit test의 이름/분모는 첫 제품 실행 전에 owner가 동결했다. 실패 뒤 줄이지 않는다.

| test | 고정 범위 |
| --- | --- |
| `undo_independent_literals_and_layouts` | U01–U20 exact20 및 root/chunk page golden2. 모든 metadata·borrowed slice·full digest 대조 |
| `undo_all_lengths_preserve_outputs` |20 vector의 모든 짧은 decoder 입력9,044+trailing20=9,064 거절; 짧은 encoder output9,044 거절; 성공 tail20. page 길이0/16,383/16,385×2=6 거절 |
| `undo_targets_bind_namespace_owner_and_key` | Row/Index 각각 기대 namespace DB/timeline·page file/page/incarnation·undo store owner/gen·slot generation·target owner/gen/entity·target kind의12종=24거절. index arity/key/suffix 손상3; Catalog target 정상1/occupied body 미지원5 |
| `undo_before_state_matrix` | Live target2×before-kind5×storage3×stamp{zero,own,committed}3=90; 위 표의 허용12/거절78. own generation/foreign TxnId/zero q/같거나미래 logical sequence의 추가 거절5 |
| `undo_edges_keep_roles_and_multiplicity` | Live의 두 edge state4×4=16: Never/Linked/Cut 조합9 수락/Released 포함7 거절. 동일/서로다른 대상2개 모두 두 edge 유지. loaded-edge 정상4·아래 부정8 |
| `undo_release_cursor_state_matrix` | Releasing cursor0..4×두 edge state4×4=80: 상태표14 수락/66거절. incoming1/MAX2거절, Live cursor1/2/3의3거절. 별도 chunk-release12개 아래 표 |
| `undo_rejects_checksummed_damage` | 아래 root/target48·chunk16=64개; enclosing page checksum을 다시 계산해도 거절 |
| `undo_slot_lookup_checks_generation_and_kind` |37 minimal root cells의 slot36 정상1, slot37/없는slot·stale generation·Free 등 아래12거절. generation1/MAX−1/MAX의 next2/MAX/Exhausted |
| `undo_chunk_boundaries_and_verifier` | total2,049/16,060/16,061/32,120/32,121/131,072의6 chain, 총18chunk. incomplete/skip/duplicate/마지막후push·digest 손상·오류후재시도.4-chunk 반복주소 동일/다른incarnation2거절 |
| `undo_chunk_ownership_and_release_resume` | 아래8 ownership 불일치 거절.2-chunk release의 released0/1/2 상태3개와 현재 chunk 검산. 이미 반환한 앞 page 없이 next에서 재개하는 표현 확인 |
| `undo_failed_shapes_have_no_effects` | frame/heap/I/O 없이 root/target/head/chunk의 shape·identity·sequence·capacity12종 sentinel. incoming0−1/MAX+1 거절2, duplicate2 증가 계산 및 두 번 해제의 순수 값 대조 |
| `undo_generated_profiles_match_independent_fields` | seed0..63×128=8,192의 아래 명시 입력·직접 byte offset oracle·roundtrip. fixed edge/cursor/chunk 경계를 작은 생성 값으로 대체하지 않음 |

### 세부 부정 입력

loaded-edge 부정8개는 target reference의 slot generation, page incarnation, actual sequence≠expected, sequence0, source와같은 sequence, source보다큰 sequence, previous-txn creator 불일치, previous-version target 불일치다. 정상4개는 previous-txn의 같은/다른 target2개, previous-version의 same creator/다른 committed creator2개다. source.before logical은 올바른 loaded previous-version의 creator/mutation과 결합해 준비한다.

chunk-release12개는2-chunk head에 cursor3/released{0,1,2,3}×next{zero,root40,second41}다. `(0,root),(1,second),(2,zero)`3개만 수락하고9개를 거절한다. next를 알려진 실제 chunk와 대조하는 검산은 별도다.

slot 부정12개는 expected DB/file/page/incarnation/store owner/store generation/kind7개, slot37, 실제slot수초과, slot generation 불일치, Free, chunk page를 root로 사용이다. target24 부정과 합쳐 고유 총분모인 것처럼 집계하지 않는다.

ownership8개는 chunk의 DB, undo store owner, store generation, record PageRef, record slot, record slot generation, record sequence, row Overflow(kind7/XOV1)로 치환이다. 각각 outer page checksum과 필요 local checksum을 재계산하며 기대 root context를 바꾸지 않는다.

sentinel12개는 root의 creator0/namespace불일치/last key byte손상/foreign beforeTxnRef/순서역전/잘못된cursor6개, target key초과, head total초과, chunk 잘못된index/next/payload길이3개, 전체output부족1개다. 실패한 count 산술과 wire encoder 실패를 성공 state 전이로 처리하지 않는다.

| root/target corpus | 단독 변경 |
| --- | --- |
| N01–N08 | U04 magic/version/state/reserved7/generation0/sequence0/creator epoch0/creator reserved |
| N09–N16 | U04 creator local0/creator status-generation0/mutation0/target kind0/Row의arity1/Row의keylen1/target reserved/target owner0 |
| N17–N24 | U04 target generation0/entity0/txn edge tag4/version edge tag4/Linked의zero ref/Linked의sequence0/Linked sequence100/Linked sequence101 |
| N25–N32 | U04 txn ref reserved/자기slot의같은generation/자기slot의다른generation/Never인데nonzero link/Cut인데nonzero link/Live의Released/foreign beforeTxnRef/before own generation다름 |
| N33–N40 | U04 before logical foreignTxnId/own logical mutation21/own logical mutation22/RowValue+None/Inline total2049/inline canonical tag5/reserved329/reserved334 |
| N41–N48 | U05 head count불일치/head reserved/head total2048/head root=recordpage/Live release-next nonzero/Live released1; U12 before stamp nonzero; U04 reserved424 |
| C01–C08 | U20 magic/version/index1/count2/reserved10/total2050/payload length2048/reserved20 |
| C09–C16 | U20 record ref다름/record sequence99/root page다름/마지막next nonzero/digest다름/timeline18/payload잘림/payloadtrailing |

N13/N14는 U04 Row의 arity/key length를 각각1로 바꾸며 N17 이후 target 관련 값도 U04의 Row core에서 바꾼다. Row의 keylen1은 root 길이와 별개로 target shape부터 거절한다. N28/N29는 해당 edge tag를 Never/Cut으로 바꾸되 기존 link bytes를 그대로 두는 단일 일관성 손상이다. zero/reference 전체를 바꿀 때에도 다른 field는 유지한다. full checksum을 가진 arbitrary logical history를 codec이 인증한다고 주장하지 않는다.

### 생성 profile

`n=128×seed+sample`이며 n=0..8191이다. 현재 UndoRef는 file1:2/page`100+2n`/incarnation4/slot0/generation6, sequence=`1000+n`, creator epoch18/local=`19+n`, status generation20, mutation sequence=`10+n`다. variant=`n mod7`은 Row Missing/RowDeleted/RowValue Inline/RowValue Chunks/Index Missing/IndexAbsent/IndexPresent 순이다. 모두 Live이며 incoming=`n mod6`이다.

Missing은 before zero/previous-version Never다. 다른 variant는 n의 bit3이0이면 own before(logical mutation=current−1),1이면 Committed(q22,다른TxnId epoch17/local`10000+n`/mutation1)다. previous-txn은 bit4가1이면 Linked, 아니면 Never다. Missing 외 previous-version은 bit5가1이면 Linked, 아니면 Cut이다. Linked ref는 page`99+2n`/incarnation4/slot5/generation6, sequence=`999+n`이며 두 role이 동시에 Linked이면 duplicate를 그대로 보존한다. 이는 local wire profile이며 실제 target record를 생성해 loaded-edge 성공을 주장하지 않는다.

target owner/gen은5/6, RowId=`258+n`이다. index arity1의 Bool(n의bit0)+RowId full key를 typed 입력에서 직접 조립한다. Inline은 한 Binary 열의 length=`n mod97`, byte i=`(31i+n) mod256`이며 total=9+length다. Chunks total은 `2049+(7919n mod129024)`다. 두 Binary 열의 길이를 `a=min(65536,total−14)`, `b=total−14−a`로 만들고 각 payload byte i=`(31i+n) mod256`으로 작성한다. 두 value 모두64 KiB 이하이며 canonical total이 정확히 일치한다. chunk root는 page`1000000+9n`/incarnation4다. head digest는 이 원자료에서 독립 SHA-256으로 계산한다. 생성 profile은 chunk 전체 순회 시험의 분모가 아니며 그 의무는 위6 chain·18chunk에 남긴다.

예상 byte 수와 모든 serialized field는 이 원자료/offset 표에서 계산한다. candidate decoder 출력으로 expected payload·edge·identity를 만들지 않는다.

## 9. 실행 상태와 독립 검토

제품 실행은 working tree 대신 `artifacts/dev3/undo-leaf-r1/source`, `undo-leaf-r2/source`, `undo-leaf-r3/source`에 보존한 export에서 진행했다. 첫 export에만 module wiring을 추가했으며 working-tree 공유 module은 구현 담당이 변경하지 않았다. 각 실행은 contract 사본·입력 hash·실제 stdout·test binary와 `validation.json`을 보존한다.

| 실행 | 실제 결과 | 의미 |
| --- | --- | --- |
| r1 | 고정12 tests PASS, Clippy FAIL | 시험의 HASHES 인덱스 반복문 `needless_range_loop` 1건. 제품12개 성공으로 lint 실패를 지우지 않음 |
| r2 | 같은 고정12 tests PASS, Clippy PASS | enumerate 수정, 기존 고정 literal 시험에 row inline5/2,048·index key1,032의 exact bound assertion과 profile8,192의 전체 bytes 직접 offset oracle 보강 |
| r3 | 같은 고정12 tests PASS, Clippy PASS, 소유 Rust2파일 format PASS | 독립 검토 지적에 따른 state/generation/metadata/release-progress accessor와 기존 literal 시험의 접근 경계 assertion 보완 |

r1/r2 두 실행의 제품 source는 `4dc364010126ece26dc10bfd4bd6271c15c802376be4b9ec28fe00b783ccefa6`로 같고, tests는 각각 `14d9df5510b9a03c430978240f477484027315f8bbcd883f40ba09295c965b7b`, `c7115f4076636552a8ffce765a9b36d1c043dbdfd85a8df0531a9ee7f7219f3f`다. 실행 계약 `766bd2e334017d271877f8f59e45cf3ee346f0b2a1dccacdf19e916532a5d13d`는 각 export 옆 `contract.md`로 보존한다.

`/root`의 제품 정적 검토에서 Free/Releasing metadata를 외부 소비자가 관찰할 accessor가 없음을 지적했다. raw Releasing before image를 노출하지 않고 state/generation/metadata/release-progress accessor를 추가했으며, 동일 literal 시험 안에서 Free inspection·Releasing progress·Free/Live progress 거절과 Releasing before 거절을 보강했다. r3 제품 source는 `1f94a2dacf70807d9b1e0d4fd9db35ad4322baf98b52b249ca4bf80f74cd7079`, tests는 `a4ad1196e6ba40105475f11e3d6fcdfb8768d0913fb17b025aec80cfd6336709`, 실행 당시 계약은 `4e7f082ff85d5c52e499ee6befc9b80b611d713c8941c2437cabe528c2e30f97`다. 정확한12개 test 이름과0 fail/0 ignore, source files·binary·로그 hash를 r3의 `validation.json`에 보존했다.

root가 최종 source를 독립 검토하고 별도 사본에서 고정12시험을 통과했다(`artifacts/dev3/undo-independent-root-idhtzexi/review.json`). 실제 I/O·WAL/CLR·undo 적용·수명 집행·통합 및 DEV3 전체 인수는 미완료다.

<a id="free-initial-page"></a>
## target 없는 Free 슬롯과 초기 root page

2026-10-24 · **D3-UNDO-FREE-PAGE-001 · 독립 D2 수용 / 선택7함수 PASS / 최종 독립 D1 제한 수용**.
기준 `cd8b876`, 구현 `/root`, 독립 검토 `/root/dev3_independent_review`.
기존 XUR1/XDP1 version1 bytes와 앞 절의 source별 인수는 유지한다. 다음 plan3 mixed
Status/Undo INIT·동일 pool·flush/load·현재-build 복구가 요구하는 authentic Free root 표현을 공급한다.
이 내부 선행만으로 해당 소비자나 live undo·DEV3/S5를 완료하지 않는다.

`encode_free_undo_record`는 nonzero Generation과 caller output을 받아432바이트만 쓰고 tail을
보존한다. 길이 부족은 NoSpace이며 전체 output 불변이다. `decode_free_undo_record`는 target/schema
없이 expected Generation과 정확432바이트의 Free 형식만 검산하여 Generation을 돌려준다.
공통 header 순서는 길이/magic→version→reserved→generation이다. 그 뒤 state0은 전체 unused
16..432 zero와 exact length를 확인한다. state1/2는 StateMismatch, 다른 state는 Corrupt다.
StateMismatch는 점유 record body가 유효하다는 뜻이 아니다. 기존 generic codec의 expectation-first
순서·occupied body 의미를 유지하면서 header와 Free body 검사를 공유한다. 기존 Free encoder도
새 encoder로 위임하며 target 문맥·generation 검사와 오류 순서는 유지한다.

`encode_initial_undo_page`는 안정된 owner에서 받은 PageIdentity와 PageLsn을 표현하여
Undo kind8/level0·gen1 Free slot 하나를15920..16352에 쓴다. output은 정확16,384바이트이며
오류 시 불변이다. `initial_undo_page_lsn`은 전체 checksum/envelope/기대 identity와 exact slot1·
위치·canonical Free gen1 body를 검산하고 LSN만 관측한다. 0/MAX LSN 표현은 허용하되 durability나
next LSN 발급을 뜻하지 않는다. DB timeline·plan3 role·membership·PageToken·예약은 이후 owner가
검증한다. 이 helper에 raw identity를 넣었다는 사실로 그 권한이 생기지 않는다.

비교 대안은 가짜 Row/Catalog target, 빈 zero-slot page, generic expectation을 Option으로 완화하는
방식이다. 각각 없는 의미의 위조·필요한 Free slot 미공급·점유 record 불변식 약화 때문에 선택하지
않는다. 기존 모듈의 좁은 codec과 page helper를 선택했다. 새 I/O·WAL·heap·retained buffer가 없으며
실제 mutable 소비자는 자기 전체 F05와 first-effect gate를 별도로 요구한다. 이 pure 선행의 native/
복구는 해당 경로 없음이며 복구 PASS가 아니다. 점유 root·slot 재사용·형식/target 의미 확대 시 재검토한다.

[독립 raw 입력](../../tests/fixtures/dev3-undo-free-page/manifest.json)은 Free1/6/MAX·초기 page와
기존 독립 U02/U13의 byte-exact 사본이다. 새 선택7함수987case 중 static1을 따로 구분하며,
[중앙 inventory](../plans/dev3-test-plan.md#undo-free-page-followup)의 추가49,494는 ignored/NOT_RUN이다.
독립 판정과 원 source별 결과는 후속 evidence에 보존한다. 기존12 Undo suite·무변경 하위 wrapper를
반복하지 않는다. 순수 표현의 변경은 실제 INIT·mixed pool·plan3 복구·live undo를 대신하지 않는다.

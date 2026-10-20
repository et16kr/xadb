# DEV3 extent allocation page 표현

2026-10-20 · **D3-ALLOC-F01 표현 국소8시험·독립 검토 PASS / 실제 allocator 효과 미구현**

[Page·file identity](disk-storage-design.md#page) · [예약 회계](disk-storage-design.md#reserve-accounting) · [복구 예약](disk-storage-design.md#recovery-reservation) · [TxnId·TxnRef](dev3-transaction-format.md) · [M01](dev3-undo-model.md) · [M04](dev3-recovery-model.md) · [M06](dev3-reservation-model.md) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 leaf는 **기존 물리 extent 한 개의 allocation inventory**를 한 page에서 bounded decode/scan하는 표현이다. file 생성·확장·길이 확인·FILEMAP 선택, page 예약/할당/초기화/해제/재사용, WAL·ledger·obligation 효과와 owner directory는 구현하지 않는다. 아래 wire 형식의 수락은 실제 token 소유나 allocator capability 발급이 아니다. 알 수 없는 state/tag·미지원 형식은 거절하며 아직 없는 효과나 owner resolver를 성공 stub으로 제공하지 않는다.

64-page extent는 상위 설계의 요구다. 아래 metadata 주소/incarnation 고정, 자기 소유, 64-byte header·96-byte entry, 예약 시 incarnation 소비, system owner union은 이번 F01의 **새 상세 결정**이다. root가 전체 byte/상태/소유/분모 계약을 독립 검토하여 표현 구현을 수락했다. 채택은 실행 증거가 아니다.

## 1. Extent 주소와 bootstrap

- extent는 같은 FileId 안의 연속64 page, 정확히1,048,576 bytes다. `base=64×extent ordinal`, page 번호는 `base+i`(i=0..63)다. base는64의 배수다.
- entry0의 page `base`가 이 extent의 allocation metadata다. 나머지63개가 일반 data/undo/status/tree 등의 page다. metadata도64개/물리1 MiB 안에 포함하며 별도 추가 page로 계산하지 않는다.
- metadata PageRef는 **`(FileId,base,incarnation1)`**로 파일 수명 동안 고정이다. 해제·재예약·재사용·다른 owner로 이동하지 않는다. file 자체를 제거하려면 WAL/DWB/checkpoint/참조 closure의 해제 조건을 만족해야 하며, 새 파일은 새 FileId를 사용한다.
- 일반 page의 `page_no / 64 × 64`에서 metadata 주소를 산술 유도할 수 있다. metadata의 incarnation을 자기 header나 allocator directory에서 읽어 기대값으로 삼지 않는다. DB·FileId와 allocator ObjectId/Generation은 이미 검증한 control/FILEMAP 소유 문맥에서 받는다.
- 첫 extent의 metadata는 page0이다. 주소 계산은 파일 길이·extent count·FILEMAP 내부 directory 탐색에 의존하지 않는다. 이것이 파일/extent의 존재를 증명하지는 않는다. 실제 file loader는 선택된 FILEMAP과 파일 길이·namespace 영속 장벽을 검사하고 필요한 extent가 완전히 존재한 뒤만 읽기/참조한다.
- 아직 별도 datafile header의 byte 형식은 정의하지 않았다. 후속 file 형식이 page0을 별도 header로 점유하거나 위 고정 관계와 충돌하면 이 F01과 독립 vector를 **구현 전에 다시 검토**해야 한다. 이 표현을 근거로 file header/FILEMAP bootstrap 구현 완료를 주장하지 않는다.

산술은 `base+64`와 그 값의 `×16,384`를 checked u64로 계산한다. exclusive end offset까지 u64에 들어와야 한다. 최대 base는 `2^50−128=1,125,899,906,842,496`이다. 이 wire 주소 한도는 실제8 GiB quota나 지원 file size를 확대하지 않는다. quota와 허용 파일 길이는 외부 효과 계층의 더 작은 제한이다.

공통 page는 Allocation(kind10), level0, incarnation1이다. 기대 allocator ObjectId/Generation과 DB/FileId/PageRef를 기존 envelope decoder로 검증한다. pageLSN은 공통 규약을 따른다. page의 cell은 아래 inventory **정확히 하나**다.

## 2. Inventory header와 entry

모든 다중 byte 정수는 little endian이다. 하나의 cell은 `header64 + entry96×64 = 6,208 bytes`이며 잘림·trailing을 거절한다. slot4를 합친 사용량은6,212 bytes, 공통 cell/slot capacity16,224 안의 나머지10,012 bytes는 기존 envelope 규칙대로 zero다.

| header offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | `XAL1` |
| 4 | 2 | version1; 그 밖은 UnsupportedFormat |
| 6 | 2 | entry count64 |
| 8 | 8 | extent base page number, 공통 header의 page number와 같음 |
| 16 | 2 | extent pages64 |
| 18 | 2 | entry bytes96 |
| 20 | 12 | reserved0 |
| 32 | 24 | metadata 자기 PageRef, 공통 header와 정확히 같음 |
| 56 | 8 | reserved0 |

entry i는 cell의 `64+96×i`에 있다. entry 자체에 page number를 중복 저장하지 않는다. 고정 index와 extent base가 주소를 결정하므로 중간 Free 항목을 제거하거나 compact해 뒤 index를 바꾸지 않는다. 공통 slot directory는 이 단일 cell의 물리 위치만 표현한다.

| entry offset | bytes | 값 |
| --- | --- | --- |
| 0 | 1 | Free=0, Reserved=1, Allocated=2; 그 밖은 Corrupt |
| 1 | 1 | Allocated의 공통 PageKind1..11; Free/Reserved는0 |
| 2 | 1 | reservation owner tag: None=0, Transaction=1, System=2 |
| 3 | 5 | reserved0 |
| 8 | 8 | allocation incarnation, nonzero |
| 16 | 8 | Allocated의 target ObjectId(nonzero), 그 밖은0 |
| 24 | 8 | Allocated의 target Object Generation(nonzero), 그 밖은0 |
| 32 | 48 | reservation owner payload; None이면 전부0 |
| 80 | 16 | reserved0 |

| 상태 | kind | target ObjectId/Generation | reservation owner | incarnation 의미 |
| --- | --- | --- | --- | --- |
| Free(g) | 0 | 없음, zero | None, zero | 마지막으로 소비한 값의 tombstone. MAX도 유효하지만 재예약 불가 |
| Reserved(g,claimant) | 0 | 없음, zero | Transaction 또는 System 필수 | 이 예약에 이미 소비한 새 incarnation. 아직 page INIT/allocated target이 아님 |
| Allocated(g,object,kind) | 유효 PageKind | 둘 다 nonzero | None, zero | 현재 물리 page lifetime. g=MAX도 읽기 가능 |

Reserved는 특정 object가 아직 정해지지 않은 page token pool도 표현한다. `owner`를 예약 claimant와 할당 뒤 target object 두 의미로 혼용하지 않는다. Allocated에는 transaction이 끝난 뒤도 살아야 하는 object 소유만 남으며 마지막 예약 거래를 계속 소유자로 저장하지 않는다.

entry0은 반드시 `Allocated(1, envelope.owner/envelope.owner_generation, Allocation)`이고 claimant는 없다. entry1..63의 Allocated kind10은 거절한다. 각 extent의 metadata는 자기 고정 위치 한 개뿐이며 일반 token을 metadata로 전환해 bootstrap 관계를 바꾸지 않는다. 다른 kind1..9/11은 정상이다. kind만으로 그 target object가 실제 존재하거나 page 내용과 일치한다고 인정하지 않는다.

## 3. 예약 claimant와 namespace

Transaction payload는 [동결된 TxnRef48 bytes](dev3-transaction-format.md) 그대로다. `TxnId40 + slot generation8`을 재정의하거나 축약하지 않는다. public TxnId의 zero counter, reserved bytes, TxnRef generation0은 기존 constructor/decoder가 거절한다.

System payload는 별도 `SystemWorkRef`48 bytes다. TxnRef와 같은 폭이지만 다른 타입이며 tag로 구분한다.

| SystemWorkRef offset | bytes | 값 |
| --- | --- | --- |
| 0 | 16 | DatabaseId, all-zero 금지 |
| 16 | 8 | timeline, nonzero |
| 24 | 1 | work kind: AllocationMaintenance=1, StructuralChange=2, Aging=3, Checkpoint=4 |
| 25 | 7 | reserved0 |
| 32 | 8 | work id, nonzero |
| 40 | 8 | work generation, nonzero |

work kind0 및5..255는 미지원 opaque 작업으로 성공시키지 않고 거절한다. `(DB,timeline,kind,id,generation)` 전체가 system 작업의 논리 identity다. work id/generation 발급·영속 보존·descriptor directory·동시 작업 수 제한은 아직 구현하지 않는다. 기존 ID를 재사용할 때 generation을 checked 증가시키며 wrap은 금지한다. 단순한 thread 번호·현재 writer epoch·메모리 주소는 work identity가 아니다.

Transaction claimant는 해당 거래의 forward/전체 abort obligation에 속한 token이다. System claimant는 거래와 독립적으로 영속 descriptor를 갖는 위 네 종류의 작업 token이다. 거래에 속한 SMO/rollback 작업이라는 이유만으로 TxnRef를 임의 SystemWorkRef로 바꾸지 않는다. 실제 어떤 opcode가 transaction/system 작업을 만드는지는 그 WAL/descriptor·F05 계약이 결정한다.

Recovery는 **실행 주체**이며 기존 durable claimant를 지우는 새 종류가 아니다. crash 뒤에도 TxnRef/SystemWorkRef와 generation을 유지하고 같은 채택 prefix의 descriptor에서 한 번만 복원해야 한다. BOOT_RESERVE의 고정 workspace·두 checkpoint slot·DWB/WRITER 설치 여유는 home allocator를 먼저 읽어 얻는 Reserved token이 아니다. 이를 위해 Bootstrap 또는 임의 Recovery tag를 이 union에 추가하지 않는다.

decode는 caller의 `Namespace { database,timeline }`를 받는다. timeline은 nonzero, DB는 envelope와 같아야 한다. 모든 Reserved 항목의 DB/timeline도 같아야 하며 선택하지 않은 항목도 검증한다. 오래된 writer_epoch의 TxnRef는 같은 DB/timeline이면 허용한다. metadata header에는 timeline을 중복 저장하지 않으므로 Reserved가 없는 inventory에 임의의 과거 transaction namespace를 부여하지 않는다.

이 leaf는 system descriptor나 transaction status를 load/resolve하지 않는다. 후속 효과 계층은 reservation claimant의 directory entry·정확한 generation·미완결 obligation·소유 inventory를 확인해야 한다. 없거나 불명인 owner를 성공적인 복구/회수로 간주할 수 없다. 해당 resolver와 descriptor 형식은 미완료이며 codec의 구조 수락을 그 대체물로 사용할 수 없다.

## 4. Generation·directory·전이의 외부 의무

새 extent의 최초 durable INIT은 entry1..63을 generation1의 명시적 Free/Reserved/Allocated 상태로 만들 수 있다. 이것은 **새 FileId 또는 새 물리 extent의 최초 초기화**일 뿐 기존 inventory의 generation을1로 되돌리는 경로가 아니다. metadata entry0은 항상 위 자기 소유를 유지한다.

기존 extent의 허용 효과는 다음과 같이 해석한다. codec은 before/after 전이나 durability를 실행·인증하지 않는다.

1. `Free(g)→Reserved(g+1,claimant)`는 checked 증가로 새 generation을 소비하고 token과 잔여 descriptor를 같은 소유 WAL 규약으로 기록한다. g=MAX이면 effect 전에 거절한다.
2. 예약 취소 `Reserved(g,claimant)→Free(g)`는 이미 소비한 g를 유지한다. 같은 거래가 같은 page를 다시 예약해도 다음 g+1이 필요하다. 과거 예약 token이 같은 claimant의 재예약과 일치하는 ABA를 허용하지 않는다.
3. `Reserved(g,claimant)→Allocated(g,object,kind)`는 같은 g의 page INIT·allocator 소유 변경·예약 소비/잔여 obligation을 한 완결 WAL mini-transaction으로 결합한다. Reserved를 읽었다는 이유로 아직 이전 incarnation이 남아 있는 home page를 새 page로 노출하지 않는다.
4. `Allocated(g,object,kind)→Free(g)`는 영속 edge·undo/retired owner·horizon/rollback/queue·transient pin·DWB pending 정리 등 [회수 선행조건](disk-storage-design.md#reclaim)을 만족한 효과여야 한다. Free bytes만으로 이를 증명하지 않는다. 다른 object/generation에 양도할 때는 새 예약 lifetime이 필요하다.
5. 같은 object/lifetime 안의 tree 구조 변경 등으로 page kind를 바꾸는 경우에도 allocation entry와 page header를 해당 WAL 그룹에서 일치시켜야 한다. 이는 새 object에 같은 incarnation을 양도하는 허용이 아니다.

old PageRef와 directory entry는 page incarnation·target object/generation·Allocated 상태·kind를 모두 재검증해야 한다. Free(g)에서 generation 숫자가 old PageRef와 같더라도 lookup은 실패한다. Reserved(g)는 일반 allocated lookup으로 성공하지 않는다. 재사용된 g+1은 old g와 다르며, old buffer/header/locator를 정상 현재 page로 간주하지 않는다. TxnRef의 status directory/slot generation과 allocation incarnation은 서로 다른 counter다.

`next_reservation_generation`은 Free 항목에 한해 checked g+1 **값만 계산**한다. MAX는 Exhausted, 다른 상태는 InvalidInput이다. 이 함수는 token 발급·소유 변경·WAL 기록을 하지 않는다. MAX tombstone은 decoder가 거절하는 손상이 아니라 재사용 불가능한 정상 상태이며 scan에서 reusable free와 별도로 센다.

## 5. 바이트 회계와 한 page scan

이미 존재하는 extent의1 MiB는 owned/charged file bytes다. 그 안의 Free/Reserved/Allocated 상태를 바꿔도 파일 bytes가 늘거나 줄지 않는다. Reserved page 수를 다시1 MiB/16 KiB의 신규 bytes로 charge하지 않는다. 아직 물리 파일로 실현되지 않은 성장만 별도 outstanding growth이며 이 inventory에는 **growth intent를 저장하지 않는다**.

새 extent의 물리 확보·length sync·FILEMAP 선택/dirsync·참조 commit 장벽은 [M06 growth graph](dev3-reservation-model.md)의 의미와 실제 파일 효과에서 검증해야 한다. 기존 allocator page를 복제하여 존재하지 않는 extent의 예약을 성공한 것으로 표현하지 않는다.

M01의 참조 회수 조건, M04의 같은 WAL prefix·INIT/obligation 원자 인계와 M06의 actual bytes/outstanding 분리를 수신 조건으로 남긴다. 이 leaf의 단위 시험으로 그 효과·모델의 합성·recovery-ready를 주장하지 않는다. allocation snapshot은 RAM ledger나 전체 recovery descriptor가 아니다.

view는 immutable page bytes를 빌리고 한 번에64개 고정 entry만 검증한다. entry를 읽을 때 고정96 bytes와 최대48-byte owner만 decode한다. scan은 고정64회로 `reusable_free, exhausted_free, reserved_transaction, reserved_system, allocated`를 합산하며 합계64, allocated≥1을 만족한다. whole-file/whole-DB inventory를 RAM에 수집하거나 cache·heap·I/O·lock을 숨기지 않는다. 여러 extent는 외부 iterator가 한 page씩 순회하며 별도 RAM 상한·FD/frame ownership을 갖는다.

## 6. Private API와 오류

`storage::allocation_page`와 `allocation_page_tests.rs`만 추가한다. 기존 page, foundation identity, engine Namespace, 동결 TxnRef를 소비하며 dependency/source root·공유 API는 바꾸지 않는다.

| API | 의미 |
| --- | --- |
| `ExtentAddress::new(file,base)`, `containing(PageRef)` | alignment·u64 end offset 검증, 64-page geometry |
| `ExtentAddress::metadata_ref`, `page_ref(index,generation)` | metadata의 고정incarnation1 또는 index0..63의 주소 계산만 수행. index0은 generation1만 허용 |
| `SystemWorkRef`, `ReservationOwner::{Transaction,System}` | 위 namespace·kind·identity 값과 정확48-byte payload codec |
| `AllocationEntry::{Free,Reserved,Allocated}`의 검증 constructor/codec | 정확96-byte 값, target owner와 reservation claimant를 구분 |
| `AllocationEntry::next_reservation_generation` | Free에 한한 무효과 checked 계산 |
| `encode_allocation_cell(header,namespace,entries,output)` | caller의 정확64개 entry를 전부 검증한 뒤 필요한6,208-byte prefix만 씀 |
| `encode_allocation_page(header,namespace,cell,output)` | 단일 cell을 다시 검산한 뒤 기존 page encoder에 전달 |
| `decode_allocation_page(input,expected_identity,namespace)` | envelope·자기 소유·geometry·64개 entry를 모두 검증한 borrowed view |
| `AllocationPage::entry(index)`, `summary()` | bounded 읽기·위 다섯 상태 수 합계 |
| `lookup_allocated(expected_page_identity,expected_kind)` | 같은 DB/file/extent·현재 generation·Allocated·정확 target object/gen/kind를 모두 확인 |
| `lookup_reserved(expected_page_ref,expected_claimant)` | 같은 file/extent·generation·Reserved·정확 claimant를 모두 확인 |

조회는 capability/pin/latch를 만들지 않는다. root metadata page의 기대 allocator identity는 trusted control에서, data page의 기대 target owner는 안정화한 해당 객체에서 받아야 한다. 입력 page header를 기대값으로 복사하지 않는다.

entry/SystemWorkRef encoder는 정확96/48-byte output만 받는다. cell encoder는 성공 시 prefix 뒤 tail을 보존하고 page encoder는 정확16,384 bytes를 받는다. 모든 encoder는 shape/namespace/마지막 entry 검사를 완료하기 전 output을 쓰지 않으며 모든 오류에서 전체 output을 보존한다. page-size 또는64-entry scratch를 내부에 숨기지 않는다.

오류는 InvalidInput, UnsupportedFormat, Corrupt, IdentityMismatch, Exhausted, Page(PageError)다. caller shape/범위는 InvalidInput, 저장된 길이/tag/reserved/상태/자기 소유는 Corrupt, 유효한 기대 namespace/identity와 불일치는 IdentityMismatch다. envelope 오류는 Page로 보존한다. 여러 손상이 겹칠 때 우선순서는 약속하지 않는다. 미지원 system kind를 임의 generic owner로 변환하지 않는다.

## 7. 독립 literal과 최초 시험 분모

아래 fixture는 **제품 codec 작성 전** Python 표준 struct/hashlib만으로 계산한다. 생성 source·원본 bytes·JSON은 `artifacts/dev3-allocation-format-draft/`에 보존한다. 출력 파일이 이미 있으면 같은 bytes인지 확인하며 덮어쓰지 않는다. 제품 실행 증거는 NOT_RUN이다.

기본 header는 DB=[0x11;16], FileId=0x0000000100000002, metadata page64/incarnation1, allocator owner5/generation6, pageLSN7이다. namespace timeline17, TxnId writer_epoch18/local19·TxnRef generation20, SystemWorkRef kind1/id21/generation22다.

- `free`: entry0=자기 metadata, entry1..63=Free(1).
- `mixed`: 위에서 entry2=ReservedTxn(9), entry3=ReservedSystem(10), entry4=AllocatedRowLeaf(11,owner31/gen32), entry5=AllocatedUndo(MAX,owner33/gen34), entry6=Free(MAX). 나머지는 그대로다. summary는 `(58,1,1,1,3)`이다.
- `allocated`: entry0=자기 metadata. i=1..63은 generation=i+1, owner=31+i/gen32+i, kind는 `[1,2,3,4,5,6,7,8,9,11][(i−1) mod10]`이다.

| fixture | checksum tail | full-page SHA-256 |
| --- | --- | --- |
| free | `c7edca07f3f09e64bcc6c4f5b50953b489584c6830873b0e40b9b9bad38cabe3` | `f370aa51ffe7dd3c690b4d9d97e495abfd6619824fc3c2240c0dd1c10e421a5c` |
| mixed | `9e199b6b6f9302ad1398c93e644a7fc1b2d7dbe96ce84b5f0d78cd07bb5b509a` | `69db7e40f8a9d38b9d5004ebbd10099b158fe2895ea39c57566c39ade3cbcd56` |
| allocated | `f0cab69d683c23bda9fd8d8c458c5185e08e32ffd06406ab7d3b89c091d99c98` | `0acc25b0ff296907c24553905434a5e574dd9d212657964c3167dec508ead002` |

literal V01–V09의 정확 bytes는 다음과 같다.

```text
V01 metadata64 58414c31010040004000000000000000400060000000000000000000000000000200000001000000400000000000000001000000000000000000000000000000
V02 Free1 000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V03 FreeMAX 0000000000000000ffffffffffffffff0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V04 ReservedTxn 010001000000000009000000000000000000000000000000000000000000000011111111111111111111111111111111110000000000000012000000000000001300000000000000140000000000000000000000000000000000000000000000
V05 ReservedSystem 01000200000000000a000000000000000000000000000000000000000000000011111111111111111111111111111111110000000000000001000000000000001500000000000000160000000000000000000000000000000000000000000000
V06 AllocatedRow 02010000000000000b000000000000001f00000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V07 AllocatedUndoMAX 0208000000000000ffffffffffffffff2100000000000000220000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V08 AllocatedSelf 020a00000000000001000000000000000500000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
V09 SystemWorkRef 111111111111111111111111111111111100000000000000010000000000000015000000000000001600000000000000
```

최초 unit test는 아래8개로 고정하며 내부 분모를 assertion한다.

| test | 고정 범위·독립 판정 |
| --- | --- |
| `allocation_golden_pages_and_literals` | full page3개·V01–V09 byte 일치. full page는 literal offsets와 고정 checksum으로 독립 조립하고 후보 encoder를 정답으로 호출하지 않음 |
| `allocation_geometry_and_metadata_ownership` | base0/64/128/MAX_BASE 네 extent×index0..63=256 주소. 미정렬 base1/63/65, MAX_BASE+64, u64::MAX, index64/usize::MAX, index0/generation2 거절8. metadata fixed incarnation1·자기 target owner 검사 |
| `allocation_state_matrix_is_exact` | standalone entry에서 state0..3×kind{0,1,10,12}×claimtag0..3×target owner 없음/있음×generation{0,1,MAX}=384 입력. 상태표의 명시 허용 tuple10만 수락,374 거절. tag1/2는 정상 Txn/System payload, tag0/3은 zero. kind1..11의 Allocated 값도 각각 정상11개; page의 kind10 위치 제한은 별도 |
| `allocation_rejects_checksummed_semantic_damage` | 아래60개 corpus. 정상 envelope checksum을 재계산한 뒤에도 지정 구조/namespace 불일치 거절. 두-cell 부정은 각각 정상 inventory cell로 구성 |
| `allocation_lookup_binds_state_owner_and_generation` | mixed의 정상 allocated/txn/system lookup과 아래 L01–L24 정확24개 mismatch. Free의 같은 generation도 lookup 실패 |
| `allocation_reservation_owners_and_namespaces` | Transaction/System×DB 정상/다름×timeline 정상/다름×identity variant2=16. namespace 정상4 수락/12 거절; Txn의 epoch18/19는 둘 다 허용, System generation22/23도 각각 유효한 다른 identity. System kind1..4 정상4·kind0/5/255 거절3 |
| `allocation_encoders_preserve_outputs_and_generations` | SystemWorkRef 길이0..47/49, entry0..95/97, cell 부족0/6207, page0/16383/16385; 잘못된 header/namespace/63·65entry/마지막 entry·self 소유 각각 sentinel 보존. 성공 cell tail 보존. Free generation1/MAX−1/MAX의 next=2/MAX/Exhausted; Reserved/Allocated의 next 거절. cancel/re-reserve 표현에서 g를 되돌리지 않고 old token mismatch |
| `allocation_bounded_scan_profiles` | seed0..63, 각64 entry=4,096 항목. index0 자기 소유, index1..62 mode=(seed+i) mod4로 Free/ReservedTxn/ReservedSystem/Allocated, index63은 Free(MAX). generation=1+64×seed+i, valid claimant·target은 아래 고정식. 독립 입력 ordinal별 count와 다섯 summary 비교, 모든 entry 및 lookup 검산 |

profile의 Txn claimant는 DB/timeline17·epoch18·local=`1+64×seed+i`·slot generation20이다. System claimant는 kind=`1+(seed mod4)`, id=`1+64×seed+i`, generation22다. Allocated는 owner=`1000+i`/generation=`1+seed`, kind는 위10-kind 순환이다. Free는 같은 산식의 generation이고 index63만MAX다. 기대 summary는 입력 mode에서 별도 가산하며 decode 결과나 후보 summary로 기대값을 만들지 않는다.

64×64 profile은 각 page의 전체64개 항목을 검사하는 고정 상태 corpus다. 128-operation 할당/해제/재예약 이력이나 실제 WAL 전이 시험을 대신하지 않는다.

### Checksummed damage60

각 case는 mixed fixture의 아래 한 의미 항목만 바꾼다. byte count/유효 envelope와 unrelated fields는 유지한다. caller의 기대 namespace/header는 원래 정상 값이다.

| group | 고정 mutation |
| --- | --- |
| H01–H13 header13 | magic; version2; count63; base65; base128; extent pages63; entry bytes95; reserved20=1; self page65; self incarnation2; reserved56=1; cell1-byte 잘림; cell1-byte trailing |
| F01–F06 Free entry1 | kind1; claimtag1; generation0; target owner31/gen32; claim payload byte32=1(tag0 유지); entry reserved80=1 |
| T01–T10 ReservedTxn entry2 | state3; kind1; tag0(payload 유지); generation0; target owner31/gen32; claimant 다른 DB; claimant timeline18; writer_epoch0; local0; TxnRef generation0 |
| S01–S11 ReservedSystem entry3 | kind1; tag3; generation0; target owner31/gen32; claimant 다른 DB; timeline18; work kind0; work kind5; work id0; work generation0; owner payload reserved25=1 |
| A01–A10 Allocated entry4 | state3; kind0; kind12; claimtag1; generation0; ObjectId0; ObjectGeneration0; claim payload byte32=1(tag0 유지); entry reserved3=1; reserved80=1 |
| M01–M08 자기 소유8 | entry0을 정상 Free(1); 정상 ReservedTxn(9); Allocated kindRowLeaf; incarnation2; ObjectId31; ObjectGeneration32; entry1을 완전한 Allocated Allocation; header self PageRef의 FileId를 다른 유효 FileId로 변경 |
| P01–P02 cell 수2 | 정상 envelope의 cell0개; **정상 inventory cell2개** |

합계 `13+6+10+11+10+8+2=60`이다. M04 모델 case ID와 이 지역 mutation 번호는 서로 다른 범주다.

### Lookup mismatch24

L01–L08은 entry4의 정상 allocated 기대값에서 DB·FileId·범위 밖 page128·incarnation10·incarnation12·ObjectId·ObjectGeneration·expected kind를 각각 변경한다. L09–L11은 allocated lookup을 Free entry1·ReservedTxn entry2·ReservedSystem entry3에 요청한다.

L12–L18은 ReservedTxn entry2의 기대값에서 FileId·범위 밖 page128·incarnation8·incarnation10·TxnId.local·TxnRef.generation·claimant System variant를 각각 바꾼다. L19–L21은 ReservedSystem entry3에서 work id·generation·kind를 각각 바꾼다. L22–L23은 reserved lookup을 Allocated entry4·Free entry1에 요청한다. L24는 allocated lookup의 page를 extent 바로 앞63으로 바꾼다.

이 분모는 표현·lookup의 국소 검증이다. 실제 allocator directory, generation 발급/소비, file growth, reservation restore, WAL INIT/CLR, resource bound와 DEV3 DS/PF/V3 인수는 후속 효과 계약·독립 실행에 남는다.

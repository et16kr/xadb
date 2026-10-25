# DEV3 실제 CREATE의 scope2 그룹 표현

2026-10-25 · `D3-CATALOG-CREATE-001` 결합 D2 채택·구현 착수 수용 · [실제 소비 계약](dev3-catalog-create.md). 제품 효과·실행 인수 미완료.

## 2. 실제 표현에서 재사용하는 정수

| 실제 공급 | source symbol | wire bytes |
| --- | --- | ---: |
| outer XWF1 | `wal_frame` header160+trailer32 |192|
| 단일 owner group | XSG header192 + descriptor1536 + OperationCore256 |1984 (outer 합2176)|
| 두 owner group | XHG header256 + descriptors3072 + OperationCore256 |3584 (outer 합3776)|
| native header | `NativeEffect` |128|
| Edge / Count / InventoryDelta | obligation §6 |128 / 96 / 208|
| allocator cell | `allocation_page`, first extent64 |6208|
| Root / Job | `retention_page` |512 / 1024|
| Status home | `status_home::StatusHome` |224|
| Missing/Free Undo | `undo_record::encode_undo_record/encode_free_undo_record` |432|
| TreeAnchor / Row metadata | `anchor::encode_anchor`, `tree_page::encode_metadata` |48 / 32|
| whole-body Patch | header64 + [72,16352) |16344|
| full INIT | header64 + page16384 |16448|

이 후보의 새 opcode는 현재 retention 생산자와 동일하게 **기존 page는 whole-body Patch**를 사용한다. patch 공간을 body cell 길이로 몰래 줄이지 않는다. RollbackStart만 기존 닫힌 status224 Patch가 있어서 3040 식을 별도로 유지한다. 새 before/after 검산도 whole canonical page·slot directory를 검사한다.

Catalog record 길이를 `V`, tree의 실제 cell 길이를 `C`라 둔다.

```
V = 232 + 4 + utf8(table_name).len + Σ(4 + utf8(column_name).len)
C = 4(tree key/value lengths) + 8(ObjectId key) + V = 12 + V
```

최소 실제 schema `T(i Int64 NOT NULL)`이면 V=242, **C=254**다. 최대32개 열/각 이름128bytes이면 V=4588, C=4600이다. NativeEffect는 **실제 tree cell 전체 C**를 담는다. XCT1 값만 담아12bytes를 빠뜨리지 않는다. native Catalog parser는 wrapper/key8과 XCT1 object/key/namespace를 모두 검산한다.

## 3. 정확한 실제 identity와 발급 순서

예시 순서를 숫자로 고정하면 allocator page0, Root1, Jobs2..6, CatalogAnchor7/CatalogLeaf8, 실제 Status9, 실제 FreeUndo container10을 먼저 같은 owner에서 공급한다. table pair 후보는 page11/12다. 이 번호는 literal 예시이며 런타임 권한은 allocator의 실제 Free 두 entry와 원 slot/directory에서 얻는다. 선택은 동적9..63 중 다른 allocated/Reserved와 충돌하지 않는 두 주소의 오름차순이며 인접성을 요구하지 않는다.

- actor는 원 ATT/status가 발급한 `TxnRef`; 원 `Owner`가 동일 core/fence이며 독립 DDL이라는 비공개 capability를 보유한다. caller가 준 TxnRef/PageRef를 authority로 변환하지 않는다.
- pair: `Free(gA/gR) → Reserved(gA+1/gR+1, Transaction(actor))`. 두 checked successor와 주소 중복/physical alias를 **모두** 검사한 뒤 하나의 reserve group에 쓴다.
- Undo slot: 원 allocated object3/gen1 Undo page의 canonical Free slot0이다. `next_slot_generation`의 checked successor를 실제 Reserve에서 채택한다. Free generation1→Free generation2 + Tx UndoSlot inventory item이 실제 예약 표현이다. Free에 별도 Reserved tag를 발명하지 않는다. global inventory/RootJob와 대조하여 다른 owner가 그 slot을 예약하지 않았음을 증명한다. 같은 Free bytes를 보았다는 이유만으로 재발급하지 않는다.
- object: Root의 dynamic ObjectId high-water5에서 첫 새 object6을 발급한다. generation1/schema_epoch1인 새 ObjectHandle을 owner가 만든다. UndoSequence high-water0→1은 별개다. Tx local/LSN/page 번호를 대용 발급기로 쓰지 않는다.
- Reserve에서 ObjectId/UndoSequence와 Undo slot generation을 durable하게 소모한다. Cancel도 이를 되감지 않는다. pair Free 세대도 reserve된 incarnation을 유지한다. 다음 예약은 다시 checked +1이다.
- RootJob은 Terminal Job과 다른 실제 Free slot을 확보하여 checked Job generation successor와 exact source actor/status를 기록한다. 원64-cell census는 같은 Tx의 Terminal1 + Root1을 허용하되 두 Terminal, 중복 planned U, 중복 page/slot reservation은 거절하도록 v2에서 바뀌어야 한다. 기존 v1의 source Tx 중복금지를 그냥 제거하지 않는다.

## 4. 제안 wire dispatch와 두-owner inventory 한정

아래 숫자는 결합 D2에서 채택한 정본이다. A의 `XSG1 version2/flags1`은 SystemInit 전용으로 그대로 유지한다.

- B group는 **version2/flags2 (`CatalogCreateSingleRoot`)**의 명시 scope를 쓴다. XHG와 XSG 모두 같은 closed scope 번호2를 쓴다. group-relative version@4:u16, flags@10:u16이며 absolute XWF offset164/170이다. v1/flags0와 A v2/flags1을 이 observer가 fallback 수용하지 않는다.
- XHG header256의 기존 두 owner/두 previous/hash 필드는 그대로다. owner0은 actual Tx, owner1은 original Aging이다. descriptor 순서도 Tx/Aging이다.
- XHG 새 section count는 기존 unused bytes에 한정한다: `232:u32 aux_bytes=128E+96K+208D`, `244:u16 E`, `246:u16 K`, `248:u16 D`, `250..256=0`. 기존 `228 native_bytes`, `236 native_count`, `238 redo_count`, `240 redo_bytes`는 유지한다. 216/220의 descriptor 길이1536,224의 core 길이256도 그대로다.
- body 순서는 `Tx descriptor1536, Aging descriptor1536, core256, Native[], Edge[], Count[], InventoryDelta[], Redo[]`다. 전체 길이는 count/길이 checked 합으로 재검산한다.
- XSG는 기존192 header의 E/K/D counts와 section lengths를 그대로 사용한다. 단 새 explicit scope에서만 아래 새 의미를 받는다.
- B XHG op는 Begin1/Forward2/Clr4/Commit6/AbortEnd7/Reserve11/Cancel12다. B XSG는 RollbackStart3, AgingStart16, TerminalFinish17, CommittedMissingPrune20, ReleaseRole21, RootFree22다. ReleaseRole은 실제 Undo cursor0→1/1→2/2→3 중 정확 한 전이를 검사하는 같은 entry다. 이 표 밖 opcode/조합은 Unsupported다. 일반 dispatch framework를 만드는 범위가 아니다.

InventoryDelta208은 기존 action1Reserve/2Consume/3Release를 유지한다. 이 첫 두-owner group에서 **모든 item ordinal은0(Tx)이며 Aging inventory는 empty**다. ordinal1을 wire로 수용하거나 item 인계를 흉내내지 않는다. global36-owner inventory의 ordinal은 그 inventory의 owner table에서 separately resolve한다. digest의 ordinal0 정규화와 실제 owner identity 결합을 함께 검사한다. group-local0을 global bank slot0/ATT slot0으로 해석하면 안 된다.

Tx의 원 inventory가 empty라는 첫 범위 사전조건을 사용하므로 세 item의 canonical 순서는 PageToken(A), PageToken(R), UndoSlot(U)다. 각 item은96bytes/units1/changeLSN=ReserveLSN/category=Structural2다. PageToken identity24+zero32, UndoSlot UndoRef40+zero16를 쓴다. Reserve의 D=3은 absent→present, Forward의 D=3은 exact old→absent Consume, Cancel의 D=3은 exact old→absent Release다. CLR에는 이미 소비된 pair/slot item을 재생성하지 않는다(D=0).

Job lease는 native queue record이며 새 inventory item kind를 추가하지 않는다. 원 Tx 물리 allocator reservation과 실제 UndoSlot inventory가 reservation authority다. RootJob의 같은 identity bytes는 durable cleanup/검산 context다.

## 5. OperationCore256와 native identity 교차 검사

기존 generic core의 offset을 재사용한다. `op@0:u16, flags@2:u16, length@4=256, statement@8, mutation@16, UndoSequence@24, currentRef@32(40), nextRef@72(40), rollbackTarget@112(40), q@152, workCursorBefore@160, workCursorAfter@168, logicalBeforeSHA@176(32), logicalAfterSHA@208(32), reserved240..256=0`이다. refs/digest flags는 기존 bit0/1/2/3/4만 쓴다.

| 연산 | 이 후보의 nonzero/meaning |
| --- | --- |
| Reserve | planned statement1/mutation1/sequence1/currentU. before/after logical digest 없음. Tx descriptor last_mutation은 아직0. planned 값은 새 RootJob/Root high-water와 대조 |
| Forward | statement1/mutation1/sequence1/currentU, before Missing/after actual Catalog projection SHA. next/target/q/cursor0. descriptor last_mutation0→1 |
| Cancel | exact planned statement/mutation/sequence/currentU. projection 없음. last_mutation0 유지 |
| RollbackStart | currentU/sequence는 원 head가 실제 Live일 때 해당 proof로 검사; nextU는 after status의 rollback next. target=None. core의 이 B 의미는 old N=0 observer에 몰래 허용하지 않음 |
| CLR | consumed U/sequence, next=None,target=None, before present/after Missing SHA. status.head/next가 같은 group에서 None으로 이동 |
| Commit | q=C+1만 새 값으로 채택; native status/root와 일치. 다른 불필요 ref/digest/work fields는0. Tx current head U는 status native에서 검산 |
| Aging prune/release/free | exact U/sequence, 실제 Root cursor before/after. prune만 Catalog before/after projection SHA. Undo release cursor는 core work cursor와 혼동하지 않고 native Undo field@328에서 검산 |
| AbortEnd/TerminalFinish | unused ref/seq/digest0; native Terminal Job/status/root의 실제 state에서 의미를 얻음 |

logical projection은 canonical `XCP1(4),version1:u16,kindCatalog3:u16,UndoTargetCore32,presence:u8,zero7,[present이면 XCT1 전체 V bytes]`다. Missing은48bytes, present는48+V다. target core는 Catalog namespace owner5/gen1/entity=new ObjectId이고 key/arity는0이다. slot/offset/pageLSN/incoming count/physical directory는 projection에 없다. stamp/logical version/현재 UndoRef/anchor/typed definition은 XCT1에 한 번 들어간다. decoder를 통과한 XCT1를 포함하므로 임의 padding/뒤따르는 bytes가 없다. 이 projection 자체는 아직 제품 공급이 없으므로 독립 literal로 새로 동결해야 한다.

Native kind6Allocator/8Root/9Job/7Catalog/3Undo/5Status는 현재 정수 의미를 유지한다. 새 page native는 별도 kind10TreeAnchor/kind11TreeMetadata를 제안한다. empty Row metadata를 Row record kind1로 속이지 않는다. INIT native는 absent before(slot65535,offset/length/hash0)→after48/32이고 두 full INIT의 actual identity/slot directory와 대조한다.

## 6. Forward의 exact target/native/edge 목록

예시 page9 Status,10 Undo,11 Anchor,12 RowRoot, RootJob page2이면 변경 target 순서는 아래다. 실제 API는 file/page numeric order로 sort하고 물리 주소 중복을 없앤다. 같은 Jobs page에 Terminal/Root Job이 함께 있어도 page는 한 번 pin/redo한다. 한 그룹이 두 Job cell을 바꾸면 native 두 개가 같은 redo를 가리키는 것이며 별도 page를 만드는 것이 아니다. 이 첫 Forward는 RootJob cell 하나만 바꾼다.

| redo index | target | native index/kind | before→after body |
| ---: | --- | --- | --- |
|0|allocator0|0 /6|6208→6208: pair Reserved(actor)→Allocated(object/gen, Anchor/RowLeaf)|
|1|Root1|1 /8|512→512: revision/apply/census/actual Aging debt|
|2|RootJob page2|2 /9|1024→1024: Reserved→Retained, exact U/object/pair adoption|
|3|CatalogLeaf8|3 /7|absent→C: key ObjectId6, Tx stamp actor, logical(actor,1), undo U, anchor A|
|4|Status9|4 /5|224→224: Active headNone→U, incoming0→1|
|5|Undo10|5 /3|Free(g2)432→Live Missing432: seq1,mutation1,incoming2, creator actor, target Catalog5/gen1/entity6, both Never|
|6|Anchor11|6 /10|absent→48: object6/gen1, root R, revision1, RowLeaf/level0|
|7|RowRoot12|7 /11|absent→32: object6/gen1, RowLeaf/level0, arity0, leftmostNone, entries0|

CatalogAnchor7 is the **ninth pin/proof**, with no mutation/native/redo. It proves the trusted current CatalogLeaf root. It is not an invented extra after image. RootJob could be on pages3..6 without changing relative order. U/Status/pair dynamic physical order is not assumed outside this concrete literal; native/redo indices must be recomputed from actual sorted target set.

2026-10-25 오기 정정: RowRoot의 metadata arity는 기존 RowId key 계약에 따라0이다. 최소 TableSpec의 열1개와 Index 전용 metadata arity를 혼동한 표의1을 고쳤다. 독립 검토자 `/root/dev3_review`가 원 tree format·`tree_page::validate_schema`와 대조했으며, 이미 동결한 scope2 leaf Row INIT도 arity0이므로 raw 입력은 바꾸지 않는다.

Edge array sorted by source effect/role has exactly:

- `(native3,CatalogUndo3): None→U(sequence1)`;
- `(native3,CatalogStamp9): None→actor TxnRef`;
- `(native4,TxnHead10): None→U(sequence1)`.

Count array target order has Undo tag1 first: native5 incoming0→2; Tx tag2 second: native4 incoming0→1. The Free before Undo is accepted as the **same group's authenticated new root**, not as an already Live lookup. No count/edge is manufactured for creator, Job root ref, pair identity, transient pin or rollback next. All required fields are independently extracted; listing only counts/deltas from candidate bytes is not validation.

Fresh example native offsets are allocator10144, Root15840, RootJob `16352−1024(slot+1)`, Catalog slot1 offset`16320−C` (metadata slot0 is32bytes), Status16128, Undo15920, Anchor16304, Row metadata16320. Existing nonempty Catalog insertion uses the actual computed slot/offset and verifies every other cell is preserved. No stale hardcoded slot1 assumption is allowed there.


## Root queue cursor와 core unused fields

Root cursor는Aging queue의다음scan위치다. Begin/Reserve는cursor부터원64slots를유한순회하여첫Free를선택하되cursor자체는유지한다. Forward/Cancel/Commit/AbortEnd도cursor를유지한다. AgingStart는0→0이다. Prune/각ReleaseRole/RootFree/TerminalFinish가실제선택Job을처리한뒤cursor=(ordinal+1)%64로전진하며core160/168은그exactbefore/after를담는다. 일반사용자가JobRef를주었다는이유만으로queue순서나선택권한을주지않고originalowner가원census에서선택한다. RollbackStart는Aging/Root를바꾸지않는다.

Begin/AgingStart/AbortEnd의core는op/length외모든field0이다. TerminalFinish는workcursorbefore/after외ref/seq/q/statement/mutation/digest0이다. Reserve/Forward/Cancel/CLR는statement/mutation1,실제planned/actualseq/U만사용하고RollbackStart는statushead가없는Cancel뒤fullabort경로에서는ref/seq0,headU가있는Forward뒤에는currentU/seq/nextU를actualstatus와대조한다. Commit은q외사용하지않는field0이다. Prune/ReleaseRole/RootFree는actualU/seq와workcursor를쓰며statement/mutation0,Prune만Catalogprojection을쓴다. coreflag는실제옵션presence와정확히일치하고다른bit는0이다.

# DEV3 table·index catalog 표현

2026-10-21 · **D3-CATALOG-F01 독립 검토 수용 / 표현 국소 실행·독립 소스 검토 완료 / 실제 catalog 효과 미구현**

[catalog·객체 수명](disk-storage-design.md#ownership) · [index 의미](disk-storage-design.md#keys) · [tree 표현](dev3-tree-format.md) · [거래 표현](dev3-transaction-format.md) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 leaf는 table/index 정의와 versioned catalog cell을 caller bytes로 표현한다. catalog tree의 key는 nonzero ObjectId를 unsigned big endian8 bytes로 저장한다. row key와 같은8-byte 순서 표현을 사용해도 ObjectId를 RowUID로 노출하지 않는다. 이름 조회, namespace 유일성, object 발급·실존, catalog visibility, DDL·drop·build·undo·reclamation의 실제 효과는 수행하지 않는다.

K0의 `TableSpec`·`Column`·`DataType` 이름/타입 의미를 재사용한다. DEV3 catalog가 K0의 resident root에 저장된다고 가정하거나 그 root를 새로운 disk catalog oracle로 사용하지 않는다. 이 형식은 공개 client format이 아니며 개발 fixture를 새로 만든다.

## 1. 공통 catalog cell

고정 header232 뒤 typed body가 오고 별도 cell checksum은 없다. 담는 공통 page의 checksum이 cell 전체를 덮는다. 정수는 little endian이며, key의 ObjectId 순서 bytes만 big endian이다.

| offset | bytes | 의미 |
| --- | --- | --- |
| 0 | 4 | `XCT1` |
| 4 | 2 | format1 |
| 6 | 1 | Table1, Index2 |
| 7 | 1 | bit0 retired, bit1 hasUndo; 나머지0 |
| 8 | 8 | ObjectId, nonzero; tree key와 대조 |
| 16 | 8 | object Generation, nonzero |
| 24 | 8 | SchemaEpoch, nonzero |
| 32 | 56 | 기존 VersionStamp |
| 88 | 48 | 기존 logical mutation token의 TxnId40+mutation sequence8 |
| 136 | 40 | hasUndo이면 UndoRef40, 아니면 전체0 |
| 176 | 24 | 안정 TreeAnchor의 PageRef |
| 200 | 8 | Index의 parent table ObjectId, Table은0 |
| 208 | 8 | Index의 parent table Generation, Table은0 |
| 216 | 8 | Index의 정의가 참조하는 parent table SchemaEpoch, Table은0 |
| 224 | 4 | body bytes |
| 228 | 4 | reserved0 |

logical mutation token은 현재 `LogicalRowVersion` 타입의48-byte codec을 소비한다. 이 타입에는 RowId가 없으며 대상 identity는 별도의 catalog ObjectId/Generation이다. catalog의 변경 태그를 row의 관측 handle이나 다른 catalog 객체에 재사용하는 API를 만들지 않는다. TxnId DB/timeline은 caller가 준 Namespace와 같아야 하며 과거 writer epoch는 허용한다. Transaction stamp는 token과 정확한 TxnId를 공유해야 한다. generation/sequence의 실제 발급·상태 수명은 이 codec이 인증하지 않는다.

`CatalogExpectation`은 trusted Namespace와 tree key에서 얻은 ObjectId를 받는다. 처음 읽는 cell의 object generation/schema epoch는 검증할 데이터이며 자기 bytes를 외부 기대값으로 포장하지 않는다. 이미 획득한 handle을 검증하는 별도 helper는 trusted ObjectId/Generation/SchemaEpoch 전체와 대조한다. 최초 lookup·snapshot에서 어떤 generation을 선택하는지는 catalog/visibility 소유다.

Table의 parent 세 field는 모두0, Index는 nonzero이며 parent ObjectId가 index ObjectId와 같으면 거절한다. anchor의 존재·owner/kind·유효 generation은 실제 load에서 기존 anchor 계약으로 대조해야 한다. 서로 다른 객체가 같은 anchor를 불법 공유하는지 이 단일 cell codec이 인증하지 않는다.

retired cell도 완전한 typed body와 anchor를 유지한다. old snapshot과 후속 회수에 필요한 정의를 빈 bytes로 바꾸지 않는다. retired의 VersionStamp가 보이는 snapshot에서 해당 객체가 없다고 판정하는 것은 후속 visibility다. retired bit만으로 page·anchor·table/index를 반환하지 않는다.

## 2. Table body

`name_bytes u16`, `column_count u8`, `reserved0 u8`, table name bytes 뒤 column을 순서대로 둔다. 각 column은 `name_bytes u16`, `type u8`, `nullable u8`, column name bytes다.

- table/column 이름은 각각 UTF-8 bytes1..128, NUL 없음, byte-exact 비교다. case folding·Unicode normalization을 하지 않는다.
- column count1..32, 같은 table의 column 이름은 byte-exact로 서로 달라야 한다.
- type은 Bool1/Int64 2/Utf8 3/Binary4, nullable은0/1이다. NULL을 별도 column type으로 허용하지 않는다.
- 최소 body는4+1+4+1=10 bytes, 최대는4+128+32×(4+128)=4,356 bytes다. cell 최대는4,588 bytes다.
- caller `TableSpec`와 decode borrowed view의 의미가 같아야 한다. `catalog::validate_spec`의 기존 이름·중복·열 수 의미와 독립 vector를 대조하되 production validator를 expected bytes 생성에 사용하지 않는다.

borrowed view는 name slice와32개 column을 순회한다. 고정33×u16 offset scratch 또는 bounded 재순회를 사용할 수 있고 heap에 전체 schema를 복사하지 않는다. 입력 lifetime 밖으로 bytes를 빌리지 않는다. index의 column 선택은 이 순서의0-based position이다.

## 3. Index body

`name_bytes u16`, `mode u8`, `arity u8`, `columns[4] u8`의8-byte header 뒤 index name bytes다. body 최소9, 최대136 bytes, cell 최대368 bytes다.

mode는 PrimaryKey1/Unique2/Nonunique3이다. arity1..4이고 사용 column position은0..31, 서로 다르며 사용하지 않는 tail position은0이다. column0도 정상 값이므로 arity를 무시하고0을 sentinel로 읽지 않는다. name은 table 이름과 같은 UTF-8/길이/NUL 규칙이다.

`validate_index_against_table`은 검증된 Index cell과 Table cell을 받고 두 view의 DB/timeline namespace 동일성과 parent ObjectId/Generation/SchemaEpoch, column position이 실제 table 열 수보다 작은지, PrimaryKey의 선택 열이 모두 nonnullable인지 대조한다. 다른 schema를 잘못 해석하지 않도록 index cell에 저장한 parent SchemaEpoch와 table cell의 SchemaEpoch도 같아야 한다. Table retired·Index retired의 snapshot 가시성은 이 helper의 판단이 아니며 정의 자체의 정합성만 검사한다.

table당 최대8 indexes·PK 최대1개, 이름 중복·statement-final uniqueness·all-NULL 예외·동일 key 잠금은 단일 index body의 속성이 아니다. catalog set/transaction 소유의 후속 검증으로 남긴다. 이 codec 성공을 index build나 유일성 검사 성공으로 표시하지 않는다.

## 4. API·오류와 검사 경계

구현 파일은 `storage/catalog_record.rs`, `catalog_record_tests.rs`이며 실제 구현과 함께 build inventory에 등록했다. 공개 API·crate·dependency를 추가하지 않는다.

`CatalogHeader`, `CatalogExpectation`, borrowed `CatalogRecordView`와 Table/Index definition view, exact encoded length/encode/decode, handle 대조와 index-table definition 대조 helper를 제공한다. encoder는 입력 전체·모든 열·중복·expected namespace/key·body length·출력 크기를 first write 전에 검사하고 성공 prefix 뒤 tail을 보존한다. decode는 exact cell length와 전체 body를 검증한 뒤 view를 반환한다. 임의 body를 opaque 성공으로 반환하지 않는다.

오류는 InvalidInput, NoSpace, Corrupt, UnsupportedFormat, IdentityMismatch, DefinitionMismatch로 구분한다. caller shape/기대값 오류는 InvalidInput, wire 잘림/길이/tag/reserved/UTF-8/중복은 Corrupt, 다른 version은 UnsupportedFormat, namespace/key/handle 불일치는 IdentityMismatch, 검증된 index/table 관계 불일치는 DefinitionMismatch다. 여러 오류의 우선순서는 고정하지 않는다. error 뒤 전체 output·input이 유지되어야 한다.

## 5. 구현 전 동결할 검증

아래 inventory를 구현 전에 동결했다. 독립 literal bytes/hash를 제품 source 전에 별도 Python struct/hashlib로 생성하여 `artifacts/dev3/catalog-format-preimplementation/golden.py`, `golden.json`과6개 raw binary에 보존했다. generator는 기존 bytes가 다르면 덮어쓰지 않고 실패한다. 아래 literal·고정 부정 목록의 독립 검토 뒤 제품 source를 작성했다. 실행 결과는 마지막 절에 기록한다.

| test | 고정 범위 |
| --- | --- |
| `catalog_literals_and_borrowed_definitions` | table 최소·다중 type/NULL 가능 열·retired table, PK/Unique/Nonunique index의6개 독립 literal 및 borrowed 필드 |
| `catalog_table_name_column_and_size_bounds` | 이름0/1/128/129 bytes, UTF-8 multi-byte/NUL/중복, 열0/1/32/33; 최대4,588-byte cell 정상 |
| `catalog_index_columns_and_parent_matrix` | mode0..4×arity0..5=30 조합; 유효 mode3×arity4=12 정상. column 중복·tail·실제 열 수·nullable PK·parent generation/identity·schema epoch의 별도 고정 부정 목록 |
| `catalog_lengths_damage_and_outputs` | 6 literal의 모든 짧은 input1,545+trailing6=1,551 거절, 짧은 output1,545 거절, 아래 wire 손상48; caller 실패 output 전체와 성공 tail 보존 |
| `catalog_namespace_handle_and_versions` | DB/timeline/key/handle generation/schema epoch, stamp와 token TxnId 불일치; 과거 writer epoch 정상; retired 정의 유지 |
| `catalog_tree_values_and_fixed_profiles` | ObjectId key 순서와 catalog leaf entry 통합,64 seed×128=8,192 table/index 정의의 독립 offset·길이·roundtrip |

 catalog undo의 before body는 이 typed 정의를 소비하는 후속 계약에서 별도로 연결한다. 현재 undo 제안의 UnsupportedCatalogBody를 이 문서가 해소했다고 표시하지 않는다.

### 독립 literal과 상세 입력

기본 DB=[0x11;16],timeline17,TxnId epoch19/local23,logical mutation29,Committed q31,own TxnRef generation37이다. table ObjectId=`0x1300000001`/Generation5/SchemaEpoch7, index는ObjectId+1/+2/+3,Generation5/SchemaEpoch9이며 parent는table의정확세값이다. anchor는file`0x1300000001`,page3(table)또는101/102/103(index),incarnation4다. UndoRef가있는경우file`0x1300000002`,page10,incarnation2,slot1,generation3이다.

최소table은이름T·열i(Int64,nonnullable)다. 다중table은이름테이블, flag(Bool,nullable),balance(Int64,nonnullable),word(Utf8,nullable),data(Binary,nonnullable)의순서다. retiredtable은같은정의에retired/hasUndo,own stamp,logical mutation30이다. PK는pk_i/column1,Unique는uq_flag_word/columns0,2와own stamp/hasUndo,Nonunique는by_data/column3이다. UTF-8 bytes와wire정수는generator의직접표기에서온다.

| literal | bytes | SHA-256 |
| --- | --- | --- |
| table-min | 242 | `433eae5477405a1cc8b561c13da2975eb58912a97f5da4e74d6928814b9616af` |
| table-types | 280 | `15f8948388798e05bdc36a98945bb9e12b17fa13ef84fef04f6c10d4f5a7ba6d` |
| table-retired | 280 | `98cae1d3fd0811d73f968c750e28d5375c9fc5f887241ba0f15375c23a79eb65` |
| index-pk | 244 | `8c6b12dca7e2776418f98d18cdadd734212e22e3a755b50660d12c02dab83f32` |
| index-unique | 252 | `f5d48120011e3dc6ba41076853e83ff06015527ea5cac54e595a2eebeb4b8f30` |
| index-nonunique | 247 | `da199ff61844a63730dfdc49a40ff72881719ac10a568f325e3f2c44370e77ff` |


기본 header 손상 N01–N26과 table 손상 N27–N40은 table-min, N41–N48은 index-pk에서 각각 다른 field를 보존하여 주입한다. enclosing page를 사용할 때는 checksum을 다시 계산한다. cell 자체에는 checksum이 없다.

| 범위 | 단독 손상 |
| --- | --- |
| N01–N08 | magic,version2,kind0,kind3,flags4,ObjectId0,Generation0,SchemaEpoch0 |
| N09–N17 | stamp tag0,stamp reserved,Committed q0,logical DB0,timeline0,epoch0,TxnId reserved,local0,mutation0 |
| N18–N26 | hasUndo0인데UndoRef영역nonzero,anchor FileId counter0,anchor FileId epoch0,anchor incarnation0,Table parentObject1,parentGeneration1,parentSchemaEpoch1,body length+1,reserved228 |
| N27–N34 | table name length0/129,column count0/33,body reserved,table name invalid UTF-8,NUL,column name length0 |
| N35–N40 | column type0/type5,nullable2,column name invalid UTF-8/NUL,body length를함께늘린trailing1 |
| N41–N48 | Index parentObject0,parentGeneration0,parentSchemaEpoch0,parent=selfObject,mode0,arity0,column position32,unused tail nonzero |

index mode0..4×arity0..5의30조합은name x,선택columns0..arity−1,최대4개저장으로만조립한다. arity5는header만5로두어거절한다. 정상12개에서definition을읽을수있어야한다. 추가12개는duplicate positions,unused tail,position32,실제table열수초과,nullable PK 첫열,nullable PK 뒤열,parentObject불일치,parentGeneration불일치,parentSchemaEpoch불일치,서로다른DB,서로다른timeline,Table자리에Index view다. wire shape가잘못된앞3개는decode에서,나머지는검증된view사이helper에서거절한다. 다른Namespace의정상view두개를숫자ID만같다고결합하지않는다. view는검증한Namespace를보존한다.

namespace/handle/version 검사는DB/timeline/keyObjectId/handleObjectId/handleGeneration/handleSchemaEpoch와Transaction stamp TxnId의DB/timeline/epoch/local 불일치10개를각각거절한다. 과거writer epoch1과18의own/committed4개는정상이며,own fixture에서는stamp/token을같은과거TxnId로준비한다. retiredtable의fullbody와anchor는비retired같은정의와일치한다.

생성profile은n=128×seed+sample,0..8191이다. n짝수는Table,홀수는Index다. object=`0x1300000100+n`,generation=`1+n`,schema_epoch=`1+n`,logical mutation=`1+n`,TxnId는기본값,Committed q=`1+n`이고undo없음이다. anchor page=`1000+n`,incarnation4다. Table은이름`t{n}`,열수`1+n mod32`;열i는이름`c{i}`,type=`1+i mod4`,nullable=`i mod2`다. Index는이름`i{n}`,mode=`1+n mod3`,arity=`1+n mod4`,columns0부터arity−1,parent는기본다중table의ID/gen/schema다. 생성roundtrip은wire정의만검사하며이Index의PK정의가항상기본tablenullable조건을만족한다고주장하지않는다. helper의정확parent/nullable판정은별도고정12개와PK정상fixture가검사한다. retired는n의bit4로정한다. typed원자료로길이·field를직접계산하며candidate decoder를expected생성에사용하지않는다.

`/root/dev3_review`의첫개념검토에서Index/Table의독립namespace결합누락을지적하여제품구현전에두view의DB/timeline대조와고정부정2개를추가했다. 최종 계약 지문 `7ab6df3045e046fc9d96a3e223acc7b0ca4a3659bb29be8b1e1dd40ad5931735`의 독립 literal/offset/분모 검토를 수용했다. 원본과 별도 reader·로그는 `artifacts/dev3/catalog-contract-review-r1/review.json`에 보존했다. 이 계약 검토 당시 구현 소유는 root였고 제품 실행 전이었다.

## 국소 실행과 독립 소스 검토

`artifacts/dev3/catalog-leaf-ci_khgn4/validation.json`의 보존 source에서 고정6시험·all-target/all-feature Clippy·소유 formatter를 통과했다. source 지문은 `07502184108c250786418ae57895aa469a591cbedaa068cf0918f182de3794ef`이며 기존 commit `c800e81`과 이 두 source, export의 module 연결만 포함한다. 처음 실행한6시험에 실패·무시는 없었다. `/root/dev3_model`이 별도 사본에서6시험과 독립 raw literal reader6개를 재검증하여 PASS했다(`artifacts/dev3/catalog-independent-model-r1/review.json`).

생성 corpus의 짝/홀수 분할 때문에 생성 table 열 수는 홀수, index arity는2/4다. 전체32열 경계와arity1..4는 별도 고정 boundary/matrix에서 검증했다. 이 제한을 생성 corpus만의 전수 인수로 표시하지 않는다. 통합 source의 하위 회귀·복구는 별도이며 실제 DDL/visibility/catalog undo는 미완료다.

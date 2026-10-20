# DEV3 canonical row·행과 인덱스 version payload 표현

2026-10-20 · **D2 표현 국소10시험·Clippy·독립 검토 PASS / 실제 visibility·변경 효과 미구현**

[행·tree 소유 계약](disk-storage-design.md#tree) · [관측 행 버전](disk-storage-design.md#observed-row-version) · [참조 수명](disk-storage-design.md#undo-lifecycle) · [tree 표현](dev3-tree-format.md) · [거래 identity](dev3-transaction-format.md) · [overflow](dev3-overflow-format.md) · [병렬 소유](../implementation/dev3-implementation.md#wave-2)

이 F01 leaf는 기존 tree leaf의 opaque value를 해석하는 row/index version payload와 canonical row bytes를 정의한다. caller가 제공한 메모리만 읽고 쓰며 heap allocation·I/O·발급·visibility 판정·참조 pin·undo 기록·실제 row/index 변경·WAL·회수 효과가 없다. 기존 DEV2 public API·fixtures·의미를 변경하지 않는다. 상세 byte layout과 아래 corpus를 root가 독립 검토한 뒤 production을 작성했다. 현재 국소 결과와 후속 통합 인수는 [구현 기록](../implementation/dev3-implementation.md)에서 구분한다.

`catalog::validate_row`의 크기 정의 `4 + Σ(1-byte tag + payload)`와 DataType/Value 의미를 그대로 유지한다. 기존 DEV2 durable row payload의 tag와 정수 byte order도 같은 규칙이지만 그 상위 WAL/table record를 이 형식으로 대체하지 않는다. 실제 schema publication·DDL·table identity 및 durability는 후속 leaf의 별도 의무다.

## 1. Canonical row bytes

첫4 bytes는 column count:u32 little endian이다. count는1–32이며 supplied schema의 column 수와 정확히 같아야 한다. 이후 column 순서대로 아래 value가 하나씩 오고 마지막 뒤 trailing byte를 허용하지 않는다. 전체 canonical row는 최대131,072 bytes이며 최소5 bytes다.

| tag | 값 | payload |
| --- | --- | --- |
| 0 | NULL | 없음 |
| 1 | Bool | 정확히1 byte, false0/true1만 허용 |
| 2 | Int64 | 정확히8 bytes, signed two's-complement little endian |
| 3 | Utf8 | byte length:u32LE 뒤 정확한 UTF-8 bytes |
| 4 | Binary | byte length:u32LE 뒤 정확한 raw bytes |

tag5–255는 손상이다. 문자열/binary payload는 각각0–65,536 bytes다. 빈 값·문자열 내부 NUL은 허용하고 UTF-8을 정규화하지 않는다. 문자열 길이는 문자 수가 아닌 byte 수다. NULL은 해당 column의 nullable=true일 때만 허용하며 다른 값은 DataType과 정확히 맞아야 한다. coercion·SQL 삼값 논리·index key escaping은 이 row codec의 기능이 아니다.

schema 입력은 기존 `&[Column]`이다. column count1–32를 확인하고 type/nullability를 소비한다. column/table 이름의 검증은 기존 catalog의 책임이며 row마다 이름 중복을 다시 검사하지 않는다. malformed tag·noncanonical bool·깨진 UTF-8·truncation·trailing bytes와 schema mismatch를 구별한다. 각 value는 wire parsing과 Bool/UTF-8 정규성 검사를 끝낸 뒤 그 value의 schema type/nullability를 확인한다. 따라서 N15/N16의 손상 wire는 바뀐 tag 때문에 schema도 달라졌더라도 먼저 Corrupt로 거절한다.

decoder는 입력을 전부 검산한 뒤 immutable bytes를 빌리는 `CanonicalRowView`를 반환한다. column 값은 `ValueRef::{Null,Bool,Int64,Utf8(&str),Binary(&[u8])}`로 읽으며 문자열/binary를 복사하거나 heap에 저장하지 않는다. bounded offset metadata가 필요하면 최대33개의 u32 offset만 사용한다. caller에게 반환된 view는 input lifetime을 넘지 못하고, mutable frame을 안정화하는 pin/latch를 대신하지 않는다.

encoder는 `&[Value]`와 schema를 받고 전체 type/size/산술 및 output capacity를 최초 쓰기 전에 검사한다. 성공하면 사용한 prefix 길이를 반환하고 output의 나머지 tail을 보존한다. 모든 오류에서 전체 output은 byte 단위로 변하지 않는다. 128 KiB scratch나 heap Vec를 encoder 안에 숨기지 않는다.

## 2. 공통 version 의미

row와 index payload는 다음 값들을 함께 담는다.

- RowId는 nonzero이며 supplied expected RowId와 정확히 같아야 한다. row tree에서는 검증된8-byte BE key, index에서는 검증된 PhysicalKey의 RowId suffix에서 기대값을 얻는다. payload 내부 RowId는 아래 표에 따라 LE로 저장하므로 key bytes를 그대로 복사하지 않는다.
- VersionStamp56과 LogicalRowVersion48은 [거래 표현](dev3-transaction-format.md)의 정확한 형식이다. LogicalRowVersion의 TxnId는 expected DB/timeline에 속해야 한다.
- Transaction stamp이면 그 TxnRef의 전체 TxnId가 LogicalRowVersion의 TxnId와 같아야 한다. 서로 다른 거래의 stamp와 logical identity를 섞지 않는다. status generation은 LogicalRowVersion에 포함되지 않으며 여기서 특정 generation을 발급/선택하지 않는다.
- Committed stamp는 q만 가지므로 LogicalRowVersion의 원래 TxnId를 유지한다. 현재 writer_epoch를 기대값으로 받아 과거 epoch를 거절하지 않는다. q stamping·page 이동·split·compaction은 논리 버전을 바꾸지 않는다.
- UndoRef는 absent이면 정확히40 zero bytes, present이면 기존40-byte UndoRef다. 그 물리 대상의 존재·DB/owner·chain 소유·incoming count·horizon·transient pin·generation 재사용 가능성은 이 local codec이 증명하지 않는다.

expected namespace/RowId는 caller가 안정화한 owner/tree context에서 받아야 하며 저장된 payload에서 기대값을 되읽어 자신과 비교하지 않는다. page의 DB/table 또는 index ObjectId/Generation, page incarnation, kind, key ordering은 기존 tree decoder가 먼저 검증한다. 이 leaf는 그 페이지를 직접 load하거나 모든 sibling/subtree와 전역 row↔index 정합성을 검사하지 않는다. 기대 RowId만 같다고 다른 table/index owner를 허용하는 capability를 만들지 않는다.

## 3. Row tree opaque value

고정 header168 bytes 뒤 optional body가 온다. record length는 정확히 `168+body_length`다. 최대 inline record는2,216 bytes로 기존 tree의 entry 상한 안에 들어간다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | ASCII `XRW1` |
| 4 | 2 | format version1 |
| 6 | 1 | flags: bit0 deleted, bit1 undo-present; 나머지0 |
| 7 | 1 | payload tag: None0, Inline1, Overflow2 |
| 8 | 8 | RowId, nonzero |
| 16 | 56 | VersionStamp |
| 72 | 48 | LogicalRowVersion |
| 120 | 40 | optional UndoRef |
| 160 | 4 | body_length:u32LE |
| 164 | 4 | reserved zero |
| 168 | body_length | 아래 payload |

| deleted | payload tag | body | 의미 |
| --- | --- | --- | --- |
| false | Inline1 | canonical row,5–2,048 bytes | supplied schema/type/nullability까지 검산 |
| false | Overflow2 | 정확히64-byte OverflowHead | canonical row total2,049–131,072의 참조 표현만 검산 |
| true | None0 | 정확히0 bytes | 삭제 tombstone |

다른 조합은 손상이다. live row를 None으로 표현하거나 삭제된 row에 inline/overflow body를 남기는 대체 인코딩을 허용하지 않는다. deleted도 VersionStamp와 LogicalRowVersion을 보존한다. DELETE는 새 사용자 mutation identity를 갖고 이전 row bytes·logical identity·overflow 소유는 undo before-image의 책임이다. 삭제 표현을 허용했다는 이유로 이전 이력이 사라져도 된다는 뜻이 아니다.

undo 유무는 세 payload 형태 모두에서 표현할 수 있다. undo=None이 과거 snapshot 복원에 충분한지는 실행 계층이 증명해야 한다. 이 codec은 insert/new row, 이미 aging된 tombstone 등 서로 다른 실행 이유를 추정하지 않는다. tombstone 제거와 overflow 회수는 실제 owner의 logged 회수 규약에 따른다.

### Overflow materialization 경계

OverflowHead를 읽었다는 것은 row의 실제 value bytes/type이 유효하다는 증명이 아니다. `decode_materialized_row(head,bytes,columns)`는 caller가 준비한 contiguous bytes의 exact total과 SHA-256을 먼저 확인한 뒤 canonical row/type/nullability/각 value64 KiB·row128 KiB를 검증한다. 반환 view는 그 caller buffer를 빌린다.

이 helper는 chunk page·chain을 읽거나 순회하지 않는다. caller는 기존 OverflowChainVerifier로 exact page identity/owner·chunk 순서·중복 주소·link·total/digest를 별도로 검사하고 필요한 materialization 공간을 예약해야 한다. materialized bytes의 digest 검사와 chain lifetime/allocator 소유를 혼동하지 않는다. length/digest가 맞아도 malformed canonical row는 거절한다.

## 4. Index tree opaque value

index record는 정확히160 bytes다. key는 기존 PhysicalKey에 있으므로 payload에 composite key를 중복 저장하지 않는다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | ASCII `XIX1` |
| 4 | 2 | format version1 |
| 6 | 1 | present: false0/true1 |
| 7 | 1 | undo-present: false0/true1 |
| 8 | 8 | RowId, nonzero |
| 16 | 56 | VersionStamp |
| 72 | 48 | LogicalRowVersion |
| 120 | 40 | optional UndoRef |

같은 full key의 제거/재추가는 한 presence undo chain으로 표현한다. present=false도 버전 metadata를 보존하며 과거 snapshot에 필요한 key를 즉시 tree에서 지우지 않는다. 이 codec은 presence가 true라는 이유로 현재 snapshot에 visible하다고 판단하지 않는다.

index LogicalRowVersion은 **그 entry를 마지막으로 바꾼 사용자 행 mutation의 identity**다. 하나의 row mutation에서 제거·삽입한 index entry들은 그 mutation identity를 공유할 수 있다. 이후 unrelated column UPDATE가 이 index entry를 바꾸지 않으면 entry의 identity가 현재 row의 identity와 달라도 정상이다. 따라서 row/index의 LogicalRowVersion이 항상 같다고 강제하지 않는다. 실제 필요한 index 변화 집합·UNIQUE·statement-final 교환·rollback 복원은 해당 상위 owner의 의무다.

## 5. Private API와 오류

소유 모듈은 `storage::row_record`이며 pub(crate)다. public re-export·새 crate 의존·shared source root 변경이 없다. 기존 api Column/DataType/Namespace, foundation Value/RowId/SHA-256, storage transaction_record/UndoRef/OverflowHead를 소비한다. 실제 첫 row/index 소비자가 붙기 전까지 이 파일에만 이유·제거 조건을 적은 `allow(dead_code)`를 사용한다.

| API | 의미 |
| --- | --- |
| `canonical_row_len(columns,values)` | 전체 유효성·정확한 byte 수 사전 확인 |
| `encode_canonical_row(columns,values,output)` | caller buffer prefix에 기록하고 사용 길이 반환 |
| `decode_canonical_row(input,columns)` | 정확한 전체 입력을 검산한 borrowed CanonicalRowView |
| `CanonicalRowView::{as_bytes,len,value}` | 전체 bytes·column 수·index의 borrowed ValueRef; 범위 밖 index는 InvalidInput |
| `RecordExpectation { namespace, row }` | 안정화된 caller context의 DB/timeline와 expected RowId |
| `RecordVersion { row, stamp, logical, undo }` | 이 codec에 제공하는 값; 발급·상태 capability가 아님 |
| `RowPayload::{Deleted,Inline(CanonicalRowView),Overflow(OverflowHead)}` | 검증된 inline bytes 또는 구조만 검증된 head |
| `encode_row_record(expected,columns,version,payload,output)` | cross-field·schema·payload·space 사전 검증 뒤 prefix만 기록 |
| `decode_row_record(input,expected,columns)` | exact length와 모든 local 의미를 확인한 RowRecordView |
| `RowRecordView::{version,payload}` | immutable metadata·borrowed payload 접근 |
| `decode_materialized_row(head,bytes,columns)` | head total/digest와 실제 canonical schema를 함께 검증 |
| `IndexRecord { version, present }`, `encode_index_record`, `decode_index_record` | 같은 expectation 검증과 정확160-byte index 표현 |

inline view가 이미 다른 schema에서 만들어졌어도 row encoder는 supplied schema로 다시 검산한다. deleted/overflow에서도 schema column count1–32는 확인하지만 실제 overflow values의 type/nullability를 확인했다고 주장하지 않는다. `RecordVersion`의 직접 조립은 가능하므로 encoder도 decoder와 동일하게 namespace/RowId/Transaction stamp 대조를 한다.

오류는 `RowError::{InvalidInput,Corrupt,UnsupportedFormat,SchemaMismatch,IdentityMismatch,TooLarge,NoSpace,OverflowMismatch}`다. 무효 caller schema count/namespace timeline0/value index는 InvalidInput, type/nullability/유효 column count와 schema의 불일치는 SchemaMismatch, wire tag/bool/UTF-8/reserved/길이/상태 조합은 Corrupt다. row/index version1 외 형식은 UnsupportedFormat, 유효 ID 간 expected 또는 cross-field 차이는 IdentityMismatch, value/row/inline size 상한 초과는 TooLarge, output 부족은 NoSpace, materialized total/digest 불일치는 OverflowMismatch다. 내장 TxnRef/LogicalRowVersion/UndoRef/OverflowHead의 구조 오류는 Corrupt로 받는다. 여러 결함이 동시에 있을 때 오류 우선순서는 계약하지 않는다.

모든 encoder는 성공 시 tail 보존, 오류 시 전체 output 보존을 보장한다. 큰 scratch·I/O certainty·transaction retry/fence 정책을 이 순수 표현 API에 추가하지 않는다. input과 output의 동시 mutable alias는 safe Rust borrowing으로 허용하지 않는다.

## 6. 독립 literal

production 작성 전에 별도 Python `struct/hashlib`로 offset 표에서 bytes를 계산했다. candidate codec을 import하거나 호출하지 않는다. 원본 generator·16개 binary·JSON·overflow canonical bytes는 `artifacts/dev3/row-format-preimplementation/`에 보존한다. 이 계산은 candidate 실행 결과가 아니다. Rust 시험에는 여기 동결한 literal을 직접 사용하며 encoder 출력에서 expected를 만들지 않는다.

| ID | 값 | exact hex |
| --- | --- | --- |
| C01 | `[NULL]`, nullable Bool | `0100000000` |
| C02 | `[false]` | `010000000100` |
| C03 | `[true]` | `010000000101` |
| C04 | `[i64::MIN]` | `01000000020000000000000080` |
| C05 | `[i64::MAX]` | `0100000002ffffffffffffff7f` |
| C06 | `[Utf8("")]` | `010000000300000000` |
| C07 | `[Utf8("A\0한")]` | `0100000003050000004100ed959c` |
| C08 | `[Binary(empty)]` | `010000000400000000` |
| C09 | `[Binary(00ff)]` | `01000000040200000000ff` |
| C10 | `[NULL,true,-2,Utf8("\0é"),Binary(00ff)]` | `0500000000010102feffffffffffffff030300000000c3a9040200000000ff` |

R01–R04/I01–I02는 RowId258, DB bytes1–16, timeline17, historical writer_epoch18, local19, status generation20, mutation_sequence21, committed q22를 사용한다. UndoRef는 file epoch1/counter2,page3/incarnation4,slot5/generation6이다. R04의 overflow canonical row는 Binary2040 bytes 한 열이다. payload byte i는 `(17×i+3) mod256`, 전체 row 길이는2,049다.

R04의 exact head는 `0200000001000000030000000000000004000000000000000108000001000000bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd`다. digest는 전체 canonical row bytes의 `bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd`다.

| ID | 의미 | bytes | 전체 SHA-256 |
| --- | --- | --- | --- |
| R01 | live inline C10, Transaction stamp, no undo | 199 | `1ba96da7ee0e122922cd9ba69996714aba221e150c979d58264274d63a20d42f` |
| R02 | live inline C01, Committed stamp, no undo | 173 | `d5b1741e41abbaf8c61a2df496e589b8dbb2686be49d4285213d29866f147f91` |
| R03 | deleted, Transaction stamp, undo present | 168 | `09d78bd1a428960cd834ed128f65d729be1821f3526ec31b35c54fe9e973762d` |
| R04 | live overflow, Committed stamp, undo present | 232 | `e2b961af6c3c42bb0032d9bb8a99b1aef362edac867f58b47a94d1bf67a00ea1` |
| I01 | present, Transaction stamp, no undo | 160 | `90707307500f6fbc20655d3576884601704d546ae9ddb203f7fb7e7b5f2d9746` |
| I02 | absent, Committed stamp, undo present | 160 | `94200879e88a3c74d678fbec6301a97bc5042db3ad8e24643ee9b489ffe9b518` |

```text
R01 5852573101000001020100000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001f000000000000000500000000010102feffffffffffffff030300000000c3a9040200000000ff
R02 5852573101000001020100000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000015000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005000000000000000100000000
R03 5852573101000300020100000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001500000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000000000000000000
R04 5852573101000202020100000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000015000000000000000200000001000000030000000000000004000000000000000500000000000000060000000000000040000000000000000200000001000000030000000000000004000000000000000108000001000000bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd
I01 5849583101000100020100000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000150000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
I02 5849583101000001020100000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000150000000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000
```

## 7. 고정 시험과 수신 분모

unit test는 다음10개다. 실행 전에 이름/분모를 동결하며 실패 후 범위를 줄이지 않는다. document verifier·기존 DEV2 fixture 변경은 실행 범위에 넣지 않는다.

| test | 고정 범위 |
| --- | --- |
| `canonical_row_literals_and_borrowed_values` | C01–C10 exact10개, borrowed string/binary slice의 input 범위·내용 검산 |
| `canonical_schema_and_size_boundaries` | 아래 schema64·column4·value8·UTF8 byte3·row3 경계 |
| `versioned_row_and_index_literals` | R01–R04/I01–I02 exact6개와 SHA, stamp·logical·undo·present/deleted 구분 |
| `every_short_buffer_and_trailing_input_is_rejected` | 16 literal의 모든 짧은 decoder 입력+trailing1=1,225; 모든 짧은 encoder output=1,209; 성공 tail 보존16 |
| `fixed_noncanonical_fields_are_rejected` | 아래 N01–N48 exact48 |
| `record_namespace_and_logical_identity_are_bound` | 아래 K01–K24 exact24, old epoch normal12, generation 독립 normal4 |
| `row_payload_and_index_presence_matrices` | row payload12(6수락/6거절), index presence9(4수락/5거절), inline threshold3 |
| `materialized_overflow_requires_digest_and_schema` | canonical total2,049/65,545/131,072 정상3과 각5종 손상=15거절 |
| `tree_values_bind_expected_row_ids` | row/index 각각 positive1·잘못된 expected RowId1, 같은 key의 present/deleted version 표현·unrelated row mutation 뒤 index identity 보존 |
| `fixed_seed_rows_match_independent_wire_fields` | seed0..63×128=8,192 canonical+row+index case, 독립 typed byte 수·field 검산·roundtrip |

### Schema와 크기 경계

schema64는 value `{NULL,false,true,MIN,0,MAX,Utf8("a"),Binary(00)}` × DataType4 × nullable2다. 정상18·SchemaMismatch46이며 `catalog::validate_row`의 실제 결과/byte 수와도 대조하되 그것 하나를 독립 oracle로 삼지 않는다. column 수0/1/32/33은 수락2·거절2다.

Utf8 ASCII/Binary 각각 길이0/65,535/65,536/65,537, 총8개는 수락6·TooLarge2다. 별도 문자열 `"한"×21,845`, 여기에 ASCII a 한 개/두 개를 붙인3개는 byte 길이65,535/65,536/65,537로 수락2·거절1이다. 두 Binary 열의 길이65,536과65,521/65,522/65,523은 총 row131,071/131,072/131,073으로 수락2·거절1이다.

inline threshold는 한 Binary 열의 payload2,038/2,039/2,040으로 canonical2,047/2,048/2,049다. 셋 모두 canonical codec이 수락하지만 row Inline은 앞2개만 수락하고 마지막은 TooLarge다. 마지막은 올바른 OverflowHead로 표현할 수 있다.

### N01–N48: 고정 wire 손상

base literal의 지정 field 외에는 bytes를 유지한다. N11/N12는 TooLarge, N18/N38은 UnsupportedFormat, 나머지는 Corrupt다. 생성된 digest를 재계산하여 손상을 숨기는 것은 materialized suite에서 별도로 다룬다.

| ID | base | 변경 |
| --- | --- | --- |
| N01 | C01 | count0 |
| N02 | C01 | count33 |
| N03 | C10 | 첫 value tag5(offset4) |
| N04 | C02 | bool2(offset5) |
| N05 | C07 | UTF-8 payload 첫 byte ff(offset9) |
| N06 | C07 | UTF-8 마지막 continuation byte를7f로(offset13) |
| N07 | C07 | length4 |
| N08 | C07 | length6 |
| N09 | C09 | length1 |
| N10 | C09 | length3 |
| N11 | C06 | declared UTF-8 length65,537 |
| N12 | C08 | declared Binary length65,537 |
| N13 | C04 | tag255(offset4) |
| N14 | C10 | 마지막 Binary length1(offset25) |
| N15 | C10 | Int64 tag를Bool1로(offset7), 나머지 payload 유지 |
| N16 | C10 | 마지막 Binary tag를Utf8로(offset24), 00ff는 invalid UTF-8 |
| N17 | R01 | magic offset0=0 |
| N18 | R01 | version2 |
| N19 | R01 | reserved flag bit2=1 |
| N20 | R01 | payload tag3 |
| N21 | R01 | RowId0 |
| N22 | R01 | VersionStamp tag0(offset16) |
| N23 | R01 | logical mutation_sequence0(offset112) |
| N24 | R01 | absent undo body offset120=1 |
| N25 | R03 | present undo40 bytes zero |
| N26 | R01 | body_length32 |
| N27 | R01 | reserved offset164=1 |
| N28 | R01 | deleted=true인데 Inline body 유지 |
| N29 | R03 | payload tagInline1인데 body0 |
| N30 | R01 | live payload tagNone0 |
| N31 | R04 | head total2,048(offset192) |
| N32 | R04 | head count2(offset196) |
| N33 | R04 | head reserved offset198=1 |
| N34 | R04 | head FileId counter0(offset168) |
| N35 | R01 | inline 첫 value tag5(offset172) |
| N36 | R01 | inline column count0(offset168) |
| N37 | I01 | magic offset0=0 |
| N38 | I01 | version2 |
| N39 | I01 | present2 |
| N40 | I01 | undo-present2 |
| N41 | I01 | RowId0 |
| N42 | I01 | VersionStamp tag0(offset16) |
| N43 | I01 | logical mutation_sequence0(offset112) |
| N44 | I01 | absent undo body offset120=1 |
| N45 | I02 | present undo40 bytes zero |
| N46 | I01 | VersionStamp reserved offset17=1 |
| N47 | I01 | LogicalRowVersion TxnId reserved offset100=1 |
| N48 | I02 | committed q0(offset24) |

### Identity와 payload matrix

K01–K12는 row R01, K13–K24는 index I01에서 같은12종을 검사한다. 순서대로 supplied namespace DB/timeline, expected RowId, logical TxnId DB/timeline, Transaction stamp의 TxnId DB/timeline/writer_epoch/local, logical TxnId local/writer_epoch를 하나씩 유효한 다른 값으로 바꾼11개, 마지막은 Committed stamp를 사용한 상태에서 logical TxnId DB를 바꾼1개다. 모두 IdentityMismatch다. stamp와 logical의 일치만으로 expected namespace 검증을 생략하지 않는다.

old epoch normal12는 record kind2 × stamp kind2 × historical writer_epoch{1,18,u32::MAX}다. Transaction stamp면 두 TxnId를 함께 바꾸며 현재 writer_epoch 비교 없이 수락한다. generation 독립 normal4는 record kind2 × TxnRef generation{1,u64::MAX}이며 logical identity를 바꾸지 않아도 표현이 유효하다. 실제 status slot의 그 generation 존재 검증을 수행했다는 뜻은 아니다.

row payload matrix는 deleted2 × payload tag3 × undo presence2=12다. 각 body는 해당 tag의 적법한 bytes이며 state 조합만 바꾼다. index matrix는 present byte{0,1,2} × undo byte{0,1,2}=9다. canonical row parser의 schema64 결과나 matrix 거절을 N48에 합산하지 않는다.

materialized overflow3개는 Binary2,040 한 열, Binary65,536 한 열, Binary65,536/65,522 두 열이다. 각 payload byte는 `(17×i+3) mod256`이고 expected digest는 원자료를 독립 SHA-256으로 계산한다. 각 head에 대해 length−1, length+1, 같은 길이의 마지막 payload byte 변경은 OverflowMismatch; supplied schema의 첫 Binary를Utf8로 바꾸는 경우 SchemaMismatch; 첫 tag5로 바꾸고 **head digest도 그 bytes에 맞춰 다시 계산한 경우** Corrupt다. 각 수정은 다른 수정과 섞지 않는다.

### 생성 corpus와 의미 한계

LCG는 seed0..63, `x=x×6364136223846793005+1442695040888963407 mod2^64`이며 iteration0..127마다 한 번 갱신한다. column 수는 `1+x mod32`, column j의 tag는 `(x >> (8×(j mod8))) mod5`다. NULL은 nullable=true, 다른 값의 nullable은 x의 j번째 bit다. Bool은 x의 j번째 bit, Int64는 wrapping `x+j`의 signed 해석, Utf8는 `["","a","\0","é","한","\0é"][(x+j) mod6]`, Binary 길이는 `(x >> (j mod16)) mod18`이며 byte k는 `(x+j+k) mod256`이다.

각 row의 byte 수와 value 내용은 이 typed 원자료에서 독립 계산한다. 모든 생성 row는 inline 상한 안에 들어간다. RowId와 mutation_sequence는 `max(x,1)`, TxnId는 기본 literal 값을 유지한다. stamp는 x의 bit14가0이면 Transaction,1이면 Committed(q22), undo는 bit15, row deleted는 bit16, index present는 bit17이다. deleted case에서도 원자료 canonical codec 검증은 수행한다. 고정 크기 경계·wire 손상·identity 손상 corpus를 이 작은 값 생성으로 대체하지 않는다.

tree 합성 시험은 기존 encoder/decoder로 검증된 row key258 및 index PhysicalKey(Bool true,RowId258)의 value에 위 record bytes를 넣는다. expected RowId는 각각 검증된 key에서 얻고 다른 RowId259를 주면 실패해야 한다. full key가 유지된 presence 표현·undo metadata를 검산하지만 실제 insert/delete·visibility·undo 수행으로 세지 않는다. index를 바꾸지 않는 후속 row mutation의 logical version만 달라도 기존 index record는 계속 decode 가능해야 한다.

실제 행 버전 발급, observed-row token, index 유지 집합, 잠금/가시성, row↔index 전체 검사, overflow/undo 소유와 회수, WAL/CLR/복구·장시간 인수는 모두 후속 실행 의무다. 이 표현 leaf의 PASS를 그 의무의 PASS로 바꾸지 않는다.

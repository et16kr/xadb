# DEV3 WAL frame·page redo 표현

2026-10-20 · **D3-WAL-F01 표현 국소 검증·독립 검토 PASS / 실제 WAL 효과 미구현**

[WAL·commit](disk-storage-design.md#commit) · [recovery](disk-storage-design.md#recovery) · [page envelope](dev3-page-format.md) · [PageRef](dev3-reference-format.md) · [거래 표현](dev3-transaction-format.md) · [M04](dev3-recovery-model.md) · [M06](dev3-reservation-model.md) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 leaf는 interleaved DEV3 WAL의 **byte frame**과 page redo subrecord를 정의한다. 기존 DEV2 `AXF1`·거래 전체 frame·public API를 재사용하거나 변경하지 않는다. `storage::wal_frame` 안에서 caller buffer와 immutable bytes만 다룬다. allocation·파일·append/sync·LSN/epoch 발급·clock·lock·page 적용·transaction publication·recovery 효과가 없다.

검증한 `FrameView`는 byte 형식의 완결성을 나타낸다. M04가 요구하는 effect/undo/head/count/cursor/잔여 obligation의 원자 그룹, M06의 예약 복원, durable commit, 성공 recovery를 인증하지 않는다. 특히 status192-byte core에는 영속 reservation descriptor가 없으며, opaque bytes를 그 descriptor의 구현으로 세지 않는다. 여기서 정한 크기 공식은 후속 opcode/F05 산식의 입력이며 opcode별 참조·page 수·논리 undo·CLR 비용의 전체 상한은 아직 미완료다.

## 1. Frame bytes

다중 byte 정수는 little endian이다. magic은 `XWF1`, format은1이다. 최대 **전체 frame 크기**는1,048,576 bytes로 header·payload·checksum을 모두 포함한다. frame은160-byte header, payload,32-byte trailer다. padding은 없다.

| offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 4 | `XWF1` |
| 4 | 2 | version1 |
| 6 | 1 | Begin1, Update2, System3, Clr4, Commit5, AbortEnd6, Checkpoint7 |
| 7 | 1 | payload format: Opaque0, PageRedo1 |
| 8 | 2 | header bytes=160 |
| 10 | 2 | flags=0 |
| 12 | 4 | 전체 frame bytes=192+payload bytes, 192..1,048,576 |
| 16 | 4 | payload bytes, 0..1,048,384 |
| 20 | 2 | PageRedo의 subrecord 수; Opaque는0 |
| 22 | 2 | reserved0 |
| 24 | 16 | DatabaseId, nonzero |
| 40 | 8 | timeline, nonzero |
| 48 | 4 | 현재 WAL writer epoch, nonzero |
| 52 | 4 | reserved0 |
| 56 | 8 | 이 frame의 LSN, nonzero |
| 64 | 8 | 직전 stream frame 또는 채택 base의 LSN; 최초 stream 시작만0 |
| 72 | 40 | 거래 종류는 기존40-byte TxnId, System/Checkpoint는 전체0 |
| 112 | 8 | previous transaction LSN |
| 120 | 8 | reserved0 |
| 128 | 32 | 앞128 bytes의 SHA-256 |
| 160 | payload bytes | 아래 payload |
| 160+payload bytes | 32 | header160 bytes와 payload 전체의 SHA-256 |

Header digest는 length·identity·순서 필드를 payload보다 먼저 검증하기 위한 것이다. 전체 길이가 있는 frame의 length 단일 bit 손상을 큰 incomplete tail로 오인하여 버리는 것을 막는다. trailer는 header digest까지 포함한다. checksum은 고의로 다시 계산한 임의 논리 이력이나 공격자의 위조를 인증하지 않는다.

### 거래 종류와 predecessor

Begin/Update/Clr/Commit/AbortEnd는 유효한 TxnId가 필수다. System/Checkpoint는 TxnId40 bytes와 previous transaction LSN이 모두0이다. Begin의 previous transaction LSN은0, 나머지 네 거래 종류는 nonzero이며 `previous_txn_lsn ≤ previous_stream_lsn < lsn`이어야 한다. 이 표현은 없는 previous record를 찾아 읽거나 해당 TxnId에 속하는지 증명하지 않는다. 그 검사는 ATT/채택 checkpoint·undo/WAL owner의 별도 의무다.

TxnId의 DB/timeline은 frame과 같아야 한다. TxnId의 writer epoch와 현재 frame의 writer epoch는 같을 필요가 없다. 이전 epoch에서 시작한 loser의 recovery CLR·abort-end를 새 recovery writer가 기록할 수 있다. TxnId의 epoch/local·reserved 검증은 거래 identity codec을 소비하고 별도 wire 표현을 만들지 않는다. 현재 epoch와 TxnId epoch의 숫자 대소만으로 transaction 존재나 수명을 판정하지 않는다.

LSN은 byte offset·file number·commit q가 아니다. 값의 발급·영속 high-water는 이 codec 밖에 있다. `u64::MAX`도 표현은 가능하지만 그 뒤 증가나 wrap은 불가능하다. frame 사이 숫자 gap은 허용하되 순서 검사에서는 caller가 전달한 **정확한 predecessor LSN**과 저장값을 비교한다. frame 하나가 다른 byte 길이를 가졌다는 이유로 LSN을 산술 증가시키지 않는다.

### Payload format

- Opaque는 정확히 선언된 bytes를 대여한다. empty도 허용하고 record count는0이다. 일곱 frame kind 모두 이 표현을 담을 수 있지만 어떤 opcode payload의 의미도 승인하지 않는다. Begin/commit-q/abort-end/checkpoint·undo/obligation body는 후속 소유 계약 없이 실행할 수 없다.
- PageRedo는 아래 subrecord가1개 이상 이어진 정확한 배열이다. 길이 합계·count가 일치하고 trailing bytes가 없어야 한다. 일곱 kind의 frame에서 사용 가능한 공통 byte container이며 kind별 필수 redo·논리 metadata를 충족했다고 판정하지 않는다.
- 알 수 없는 payload format·frame kind·version은 지원하지 않는다. Opaque 내부를 자동으로 PageRedo로 해석하거나 알 수 없는 subrecord를 건너뛰지 않는다.

## 2. Page redo subrecord

모든 subrecord는64-byte header와 data bytes다. frame의 DB를 공유하며 timeline은 WAL stream에만 있다. target의 owner/generation은 page load의 기대 identity가 된다.

| subrecord offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 1 | Patch1 또는 Init2 |
| 1 | 1 | 공통 page kind1..11 |
| 2 | 1 | 해당 kind의 level: internal1..15, 나머지0 |
| 3 | 1 | reserved0 |
| 4 | 4 | subrecord bytes=64+data bytes |
| 8 | 24 | target PageRef |
| 32 | 8 | owner ObjectId, nonzero |
| 40 | 8 | owner Generation, nonzero |
| 48 | 8 | expected prior pageLSN |
| 56 | 2 | Patch의 byte offset; Init은0 |
| 58 | 2 | data bytes; Patch1..16,280, Init16,384 |
| 60 | 4 | reserved0 |
| 64 | data bytes | after bytes 또는 full INIT page image |

한 frame 안의 target은 `(FileId,page_no)`의 엄격한 오름차순이다. 한 물리 page에는 subrecord 하나만 허용한다. 같은 주소의 다른 incarnation도 중복이다. frame당 같은 page의 여러 patch가 필요하면 caller가 그 사이 변하지 않은 bytes를 포함한 **하나의 연속 after-byte 구간**으로 합친다. 다른 FileId의 같은 page number는 다른 target이다. 이 규칙은 순서와 중복을 bounded scratch 없이 한 번 순회하여 검사할 수 있게 한다.

### Patch

`72 ≤ offset`, `1 ≤ data_len`, `offset+data_len ≤ 16,352`이며 checked arithmetic을 사용한다. identity/kind/level·pageLSN·checksum 자체를 Patch data로 바꾸지 않는다. offset72부터의 slot count/bounds·reserved·directory·cell·free space는 포함할 수 있다. `prior_page_lsn < frame.lsn`이고0도 허용한다.

data는 **현재 page 변경의 after bytes**다. logical undo는 현재 row/index 위치를 다시 찾아 새로운 CLR의 redo를 만들어야 한다. 앞서 캡처한 page 전체를 복원하여 완료된 split이나 다른 writer의 ACK를 되돌리는 undo가 아니다. prior pageLSN은 runtime/recovery의 실제 page 상태와 검산할 값이며 이 codec이 그 page를 읽거나 변경하지 않는다.

후속 applier는 target의 DB/PageRef/owner/kind/level과 canonical page를 검증하고, 이미 반영한 frame의 pageLSN이면 재적용을 건너뛰거나 정확한 prior LSN에서 after bytes를 적용해야 한다. pageLSN을 frame LSN으로 설치하고 checksum을 다시 계산하며 kind별 구조를 확인하는 일도 applier의 별도 의무다. Patch를 decode했다는 사실은 임의의 기존 page에 적용한 결과가 canonical하다는 증명이 아니다.

### Init

offset=0, prior pageLSN=0, data length=16,384다. image는 기존 page envelope decoder로 전체 길이·checksum·canonical directory와 frame DB/target PageRef/owner/kind/level을 확인한다. image의 pageLSN은 frame LSN과 정확히 같아야 한다. 공통 envelope 외 row/tree/status/undo/allocator cell의 의미는 해당 kind 소유 decoder가 이후 확인한다.

Init은 새 allocation 또는 generation이 전진한 page 재사용을 위한 redo다. 이미 존재하는 사용자 page를 거래 시작 시점의 전체 image로 되감는 rollback에는 사용할 수 없다. allocator generation 변경·FILEMAP/namespace·DWB pending retirement·관련 edge의 같은 그룹 결합은 이 표현만으로 검증되지 않는다. 손상 home을 INIT으로 복원하려면 그 INIT부터 채택 끝까지의 전체 필요한 WAL이 보존되어야 한다는 복구 계약도 그대로 남는다.

## 3. Stream 기대값·불완전 tail

`FrameExpectation`은 신뢰하는 `WalStreamIdentity { database, timeline, writer_epoch }`와 정확한 `previous_lsn`을 받는다. expected 값을 입력 header에서 그대로 추출하여 검증을 우회하지 않는다. 완결 frame의 stream identity가 다르면 IdentityMismatch, 이전 LSN이 기대 predecessor와 다르거나 현재 LSN이 이전보다 크지 않으면 OrderMismatch다. epoch 전환은 caller가 채택한 WRITER/base/epoch chain에서 별도 확인한 다음 새 기대 stream을 넘긴다. 그때 predecessor LSN은 이전 epoch/base 끝값을 유지한다.

`decode_frame`은 exact one-frame slice만 수락한다. 잘림은 Truncated, 뒤 bytes는 Corrupt다. `scan_frame_prefix`는 여러 frame가 이어질 수 있는 slice의 첫 frame만 반환하며 `consumed`를 명시한다. 다음 bytes를 현재 frame에 포함하지 않는다. empty slice는 End다.

Tail policy는 `ForbidIncomplete`와 `ActiveUnrequiredEof`로 구분한다. 후자는 caller가 **선택된 마지막 active segment의 실제 EOF이며, 그 뒤 bytes가 durable/adopted prefix·ACK·checkpoint closure에 필요하지 않음**을 이미 확인했을 때만 넘길 수 있다. 임의 중간 read buffer나 sealed segment·필수 prefix 부족에 쓰면 안 된다. byte codec은 파일 길이·sync·selector를 관측하지 않으므로 이 policy 자체를 인증하지 않는다.

1. 짧은 header에서도 이미 보이는 magic/version/header-size/flags/reserved byte가 고정값과 다르면 거절한다. 완전히 수신한 length/kind/payload-format/count/TxnId/LSN 필드는 즉시 그 규칙을 검사한다. expected DB/timeline/writer epoch/predecessor와 비교할 수 있는 부분 bytes도 일치해야 한다. System/Checkpoint의 TxnId, Begin/System/Checkpoint의 previous transaction LSN은 보이는 부분부터0이어야 한다.
2. 첫128 bytes를 모두 받으면 모든 header field를 검증하고 header digest를 계산한다. 뒤에 도착한 digest bytes가32개보다 적어도 이미 받은 prefix와 다르면 Corrupt다.
3. header160 bytes가 있으면 digest·identity·순서를 먼저 완전히 확인한다. 아직 `total_length`에 도달하지 않았다면 ActiveUnrequiredEof에서만 Incomplete다. payload/trailer가 미완결인 동안 그 payload의 의미를 부분 채택하지 않는다.
4. `total_length`만큼 있으면 trailer와 전체 payload 구조를 확인한다. full-length checksum·예약 byte·subrecord 손상은 active tail이어도 오류다. checksum 실패를 Incomplete로 바꾸거나 다음 magic을 찾아 resync하지 않는다.
5. ForbidIncomplete에서 모든 부족한 nonempty slice는 Truncated 또는 앞서 발견한 형식/identity/order 오류다. 오류가 여럿일 때 세부 우선순서는 약속하지 않는다.

이 제한적 incomplete 판정은 검증한 완결 frame만 채택하는 prefix parser다. source WAL bytes를 truncate하거나 변경하지 않고 recovery 결정·fencing·certainty를 반환하지 않는다.

## 4. Private API·무효과 경계

구현 경로는 `crates/engine/src/storage/wal_frame.rs`와 `wal_frame_tests.rs`다. 기존 page/PageRef, transaction record의40-byte TxnId codec, foundation SHA-256과 identity만 소비한다. 새 crate/public re-export·공유 source 수정은 하지 않는다. 실제 consumer 연결 전 파일 내부의 이유를 명시한 dead_code 허용은 제거 조건과 함께 둔다.

| API | 책임 |
| --- | --- |
| `WalStreamIdentity`, `WalFrameHeader`, `FrameExpectation`, `FrameKind` | 검증할 값·종류. 발급·전역 순서 저장은 없음 |
| `FramePayload::{Opaque,PageRedo}` | caller의 raw payload 또는 이미 준비한 subrecord slices |
| `frame_encoded_len(header,payload)` | header/payload 전체 사전 검증 후 exact total bytes. F05 입력용 |
| `encode_frame(header,payload,output)` | 필요한 prefix만 쓰고 tail 보존. 모든 실패에서 output 전체 보존 |
| `decode_frame(input,expected)` | exact length의 borrowed `FrameView` |
| `scan_frame_prefix(input,expected,policy)` | End/Incomplete/Complete(view,consumed) 구분 |
| `RedoTarget`, `encode_patch_record(target,prior_lsn,offset,data,output)` | page record의 지역 형식·구간 검사 후 caller prefix에 쓰기. frame LSN과의 비교는 frame 조립 때 다시 검사 |
| `encode_init_record(database,lsn,target,image,output)` | 기대 target의 INIT envelope 확인 후 caller prefix에 쓰기 |
| `FrameView::header/payload/redo_records`, `RedoRecordView` | 검증한 bytes를 대여. heap·page pin·live 참조·transaction 완료 capability 없음 |

encoder는 shape·합계·모든 subrecord·INIT image·output 크기를 첫 쓰기 전에 검사한다. record header용 작은 고정 scratch만 사용하고 page/frame 크기 scratch나 record-count 비례 scratch를 숨기지 않는다. decoder는 전체 payload를 검사한 뒤 view를 공개한다. 따라서 마지막 record 손상도 앞 record를 검증한 것만으로 성공하지 않는다.

오류는 InvalidInput, NoSpace, TooLarge, UnsupportedFormat, Corrupt, IdentityMismatch, OrderMismatch, Truncated다. 잘못된 caller 값/shape는 InvalidInput, caller output 부족은 NoSpace,1 MiB 또는 subrecord 한도 초과는 TooLarge다. wire의 잘못된 length/tag/reserved/nonzero/배열은 Corrupt, 미지원 version/kind/payload-format/subrecord type은 UnsupportedFormat이다. page 검산 오류는 이 무효과 오류로 분류하며 I/O retry·fence 의미를 추가하지 않는다. 모든 encoder의 실패 output 보존을 sentinel로 검사한다.

## 5. 실제 크기 공식

`F=192+P`, `P≤1,048,384`다. PageRedo에서 patch 수 p, INIT 수 i, patch byte 길이 L_j라면:

```text
F = 192 + 64×(p+i) + Σ L_j + 16,384×i
1 ≤ L_j ≤ 16,280
p+i ≥ 1
F ≤ 1,048,576
```

1-byte patch만 있으면 최대16,128 record다. INIT만 있으면 최대63개이고64개는 거절한다. overflow9개 page의 INIT만 담은 frame은148,224 bytes다. 이것은 row/index/undo/status/allocator·예약 descriptor의 추가 비용이 없는 산술 예이며 실제128 KiB row mutation 비용으로 사용하지 않는다. 이미 소유한 page/file의 물리 bytes와 WAL의 INIT/patch bytes도 서로 다른 자원이다.

후속 opcode는 semantic body가 별도로 더해지면 그 bytes와 같은 frame의 mandatory redo를 함께 계산해야 한다. 여러 frame를 써야 하는 큰 작업은 앞 frame까지의 부분 공개를 허용한다는 뜻이 아니다. 실제 atomic group이1 MiB를 넘으면 그 opcode를 첫 effect 전에 거절하거나 별도 검토한 진행·보상 규약을 마련해야 한다.

## 6. 독립 literal·실행 전 고정 시험

첫 literal은 Opaque Begin, payload=`aa00ff`다. DB=bytes1..16, timeline17, 현재 writer epoch18, TxnId local19, frame LSN21, previous stream LSN20, previous transaction LSN0이다. TxnId는 거래 계약 V01과 같다. Python 표준 struct/hashlib로 offset 표에서 계산했으며 production codec을 호출하지 않았다.

```text
header first128
5857463101000100a0000000c300000003000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000150000000000000014000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000000000000000000000000000000000000
header SHA256
872980fc6c3ae7c4631e848f32ae678d47e3752b0d9a6a9d330127c7fd17f2a8
trailer
7b1da8384dd20eb6dc67d326796f9ca09b974cca660dc423f994935b7175f4a5
whole195-byte frame SHA256
90037003e78771f4bb9071829de480fac53ba6acc77b58cae641d026a117f440
```

Rust test11개를 다음 범위로 고정한다. 독립 byte 계산 스크립트·literal source/contract hash는 첫 candidate 실행 전에 보존한다. 이 목록의 채택은 실행 증거가 아니다.

| test | 고정 범위·기대값 |
| --- | --- |
| `wal_frame_literal_and_kind_matrix` | 위195-byte literal과 SHA·borrowed payload. kind7×TxnId 유무2×prev-txn0/19의28개 입력:7수락/21거절. 일곱 kind의 opaque payload는 의미 검증을 주장하지 않음 |
| `wal_frame_identity_and_lsn_chain` | DB/timeline/writer epoch/predecessor 불일치4개; TxnId DB/timeline 불일치2개. old-epoch transaction CLR 수락. LSN0·equal·decreasing·MAX/wrap, prev-txn0/과거/현재/미래 경계 |
| `wal_tail_classification_is_strict` | literal의 모든 prefix0..195, ActiveUnrequiredEof에서는 End1/Incomplete194/Complete1, ForbidIncomplete에서는 End1/오류194/Complete1. 각 incomplete에서 bytes 불변. 이미 보이는 fixed/expected/header-digest byte 손상은 거절 |
| `wal_rejects_complete_checksum_and_semantic_damage` | 완전195 bytes 각각의 단일 bit flip195개는 exact decoder와 active scanner 모두 오류. 아래 N01–N28은 header/frame digest 재계산 후에도 거절 |
| `wal_patch_records_are_bounded_after_images` | offset/length (72,1),(72,16280),(16351,1) 수락3; (71,1),(72,0),(16352,1),(16351,2),(65535,1) 거절5. prior LSN0/frame−1 수락, frame/frame+1 거절. data는 caller after bytes와 직접 비교 |
| `wal_init_records_bind_complete_page_images` | kind1..11, internal level1/나머지0의11 INIT envelope; exact bytes 대조. DB/file/page/incarnation/owner/generation/kind/level/LSN mismatch9개·image checksum·잘린/trailing image 거절 |
| `wal_redo_groups_are_canonical_and_borrowed` | Patch/Init 혼합3개·borrowed data/target. 역순·동일주소·다른incarnation alias·다른file 같은page 정상 구별. count0/부족/초과, record trailing/길이·reserved·unsupported type, 마지막 record 손상 |
| `wal_exact_frame_and_record_limits` | Opaque exact1 MiB 수락/+1 거절,1-byte patch16,128개와 그중1개65-byte로 확대한 exact1 MiB 수락, record16,129개 거절, INIT63개 수락/64개 거절. independent length sums로 대조 |
| `wal_all_encoders_preserve_output` | frame/patch/INIT의 잘못된 header·key identity·마지막 record·image·capacity·output 부족은 sentinel 전체 유지; 성공 prefix 뒤 tail 유지. input bytes도 전후 동일 |
| `wal_generated_profiles_match_independent_fields` | seed0..63×sample0..127=8,192 frame. kind/TxnId·epoch·LSN·payload/tag·byte length를 명시 입력과 독립 offset 판독으로 검산. 짝수 sample Opaque, 홀수 sample sorted Patch1..4개, 길이1..256. candidate decode 출력으로 기대 bytes를 생성하지 않음 |
| `wal_stream_prefix_preserves_following_frames` | Begin(epoch18,LSN21/prev20)→Update(epoch18,LSN30/prev21)→Clr(epoch19,LSN31/prev30,TxnId epoch18)의3 frame. consumed·전 epoch predecessor 유지·old TxnId 허용, 중복/reorder/epoch/뒤 garbage 거절 |

N01–N28은 다음 각 필드 한 개를 손상한 표다. checksum은 손상 후 둘 다 다시 계산한다. N01 magic, N02 version, N03 kind, N04 payload format, N05 header length, N06 flags, N07 total<192, N08 total>1MiB, N09 payload/total 불일치, N10 opaque count1, N11 reserved22, N12 DB0, N13 timeline0, N14 epoch0, N15 reserved52, N16 LSN0, N17 previous stream=LSN, N18 TxnId DB 다름, N19 TxnId timeline 다름, N20 TxnId epoch0, N21 TxnId reserved, N22 TxnId local0, N23 Begin prev-txn1, N24 reserved120, N25 System+TxnId, N26 Clr+absent TxnId, N27 Update+prev-txn0, N28 Update+prev-txn>previous-stream. 전체 길이를 바꾸는 N07–N09는 exact decoder의 length 거절을 검사하며 incomplete tail 성공으로 집계하지 않는다.

생성 profile은 `n=128×seed+sample`, kind=`1+n mod7`, DB=bytes1..16, timeline17, writer epoch18, `lsn=100+2×n`, `previous_stream_lsn=lsn−1`이다. 필요한 TxnId는 epoch17/local=`19+n`이며 Begin의 previous transaction LSN은0, 다른 거래 종류는 previous stream LSN이다. System/Checkpoint는 TxnId/previous transaction LSN이 없다. 짝수 n은 길이 `(17×n) mod257`, byte i=`(31×i+n) mod256`의 Opaque다. 홀수 n은 `1+(floor(n/2) mod4)`개 Patch다. record r의 target은 FileId epoch1/counter2, page=`100+8×n+r`, incarnation4, owner5/generation6, Allocation kind10/level0이다. prior LSN은 previous stream LSN, offset=`72+(n+r) mod64`, 길이=`1+(n+29×r) mod256`, data byte i=`(n+7×r+31×i) mod256`으로 고정한다.

이 시험은 표현의 무효과·byte 경계다. 실제 interleaved append/short I/O·sync·fencing·checkpoint capture·reservation restore·CLR/undo·재장애·DS/F04/F05/V3를 대신하지 않는다.

실행 전 revision: `/root`가 frame/redo byte 계약을 독립 검토하여 D2 수용했다. 최초 candidate 실행 전에 다른 V2 codec profile과 맞춰 생성 분모를64×32에서64×128=8,192로 확대했다. frame/record 범위와 literal은 유지했다. `PageLsn`의 end position은 논리 LSN이며 byte offset 산술을 의미하지 않는다. 채택·구현은 semantic group 인수 또는 실행 증거가 아니다.

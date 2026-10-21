# DEV3 selected doublewrite control 표현

2026-10-20 · **D3-DWB-F01 계약·소스 독립 검토 수용 / 표현 국소시험 PASS / 실제 I/O 미구현**

[DWB 소유 계약](disk-storage-design.md#doublewrite-batch) · [page 표현](dev3-page-format.md) · [M05](dev3-doublewrite-model.md) · [병렬 소유](../implementation/dev3-implementation.md#wave-2)

이 leaf는 selector가 선택할 doublewrite control의 bytes와 그 목록에 대응하는 image 검산만 정의한다. selector 설치, 슬롯 쓰기/sync, WAL 장벽, home repair·file sync·retirement, 실제 slot 소유권은 수행하지 않는다. 선택되지 않은 raw image나 이전 control을 검색하여 복구 후보로 삼지 않는다. M05의 실행 결과를 이 codec의 실제 I/O 증거로 옮기지 않는다.

## 1. 고정 pool과 control

pool의 전체 예산은16 MiB이며 slot/header/control/selector/staging을 모두 포함한다. 먼저64 KiB를 control 두 세대와 교체 staging 각각16 KiB(48 KiB), selector current/staging 각각4 KiB(8 KiB), 미사용8 KiB 여유로 배분한다. image slot은 아래 header128+image16,384+checksum32=16,544 bytes이므로 `floor((16,777,216−65,536)/16,544)=1,010`개이고 잔여2,240 bytes도 일반 page에 대여하지 않는다. control 실제 길이는 아래 최대5,792지만 교체 peak는 위 예약 길이로 계산한다. 실제 파일 배치·선할당·namespace I/O와 F05 회계 집행은 후속 의무다.

단일 flush coordinator가 한 번에 pending batch 하나, 최대64개 image를 소유한다. 이 byte 형식은 pool의 물리 할당·존재·최대 동시 batch 집행을 증명하지 않는다. 슬롯 bytes와 고정 control 예산을16 MiB 외의 추가 허용량으로 중복 계산하지 않는다.

control은 header128 + entry88×N + SHA-25632이며 padding이 없다. exact length는160+88N, 최대5,792 bytes다. 마지막 digest는 그 앞의 모든 bytes를 포함한다. little endian 정수를 사용한다. control 파일은 일반 data page가 아니며 page envelope로 다시 감싸지 않는다.

| header offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 4 | `XDW1` |
| 4 | 2 | version1 |
| 6 | 1 | Empty0, Pending1 |
| 7 | 1 | reserved0 |
| 8 | 16 | DatabaseId, nonzero |
| 24 | 8 | timeline, nonzero |
| 32 | 8 | control generation, nonzero |
| 40 | 8 | batch id; Pending은 nonzero |
| 48 | 2 | image count N |
| 50 | 6 | reserved0 |
| 56 | 8 | Pending의 entry pageLSN 최댓값; Empty는0 |
| 64 | 64 | reserved0 |

Empty는 N=0이다. DB 생성 때 batch id0을 사용할 수 있고, retirement 뒤에는 마지막 batch id를 보존한다. Pending은1≤N≤64이고 batch id>0이다. control generation과 batch id는 별도 단조 발급 값이며 서로의 크기·등가를 가정하지 않는다. MAX 값은 읽을 수 있지만 다음 발급은 checked 증가로 거절해야 한다. 발급·selector와 generation 선택은 이 codec 밖이다.

`ControlExpectation`은 caller가 검증한 Namespace와 선택된 control generation을 받는다. input에서 기대 DB/timeline/generation을 복사하지 않는다. selector의 checksum·파일명·exact generation 결합은 후속 control 소유 계약이다. 손상 selector/control을 거절하며 이전 control로 임의 fallback하거나 raw slot을 검색하여 pending을 되살리지 않는다. 두 control 공간은 교체용이며 어느 쪽이 선택되었는지 추측하는 허용이 아니다. control decode 성공을 selector의 영속 선택이나 pending image의 존재로 취급하지 않는다.

## 2. Pending entry

| entry offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 2 | pool slot index0..1009 |
| 2 | 1 | PageKind1..11 |
| 3 | 1 | PageKind에 맞는 level |
| 4 | 4 | reserved0 |
| 8 | 24 | target PageRef |
| 32 | 8 | target owner ObjectId, nonzero |
| 40 | 8 | target owner Generation, nonzero |
| 48 | 8 | image pageLSN, nonzero |
| 56 | 32 | full16 KiB image bytes의 SHA-256 |

DB는 control namespace에서 온다. image digest는 공통 page 내부의 마지막 checksum32와 다른 값이며 **그 checksum을 포함한 전체 image**를 해시한다. all-zero digest를 특별한 sentinel로 해석하지 않는다. 실제 bytes가 일치하는지는 image 검산 때 판단한다.

목록은 `(FileId,page_no)`의 엄격한 오름차순이다. 같은 물리 주소는 incarnation이 달라도 중복이며 거절한다. 서로 다른 FileId의 같은 page number는 별개다. slot index도 서로 달라야 한다. decoder/encoder는 최대64개 entry 전체를 검산하고 마지막 entry 손상을 앞 entry의 성공으로 덮지 않는다. slot 중복은128-byte 고정 bitset 또는 최대64² 비교로 검사할 수 있으며 heap/전체 pool image를 적재하지 않는다.

### Slot sequence와 envelope

Pending control의 batch id를 그 batch 모든 slot의 **공통 slot sequence**로 사용한다. DB/timeline 안의 새 batch는 이전보다 큰 nonzero id를 영속 발급하며 재사용·wrap하지 않는다. 값이 같아도 다른 physical slot을 허용하는 이유는 slot index를 함께 대조하기 때문이다. batch 발급/epoch 선택·durable high-water는 이 codec 밖이다. control generation은 별도 counter다.

slot header128 뒤 full image16,384와 checksum32가 온다. slot checksum은 header와 image 전체를 덮으며 정확16,544 bytes만 수락한다.

| slot header offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 4 | `XDS1` |
| 4 | 2 | version1 |
| 6 | 2 | slot index |
| 8 | 16 | DatabaseId |
| 24 | 8 | timeline |
| 32 | 8 | slot sequence = 선택 Pending control의 batch id |
| 40 | 24 | target PageRef |
| 64 | 8 | target owner ObjectId |
| 72 | 8 | target owner Generation |
| 80 | 8 | image pageLSN |
| 88 | 1 | PageKind |
| 89 | 1 | level |
| 90 | 6 | reserved0 |
| 96 | 32 | full image SHA-256, control entry와 같음 |

`decode_slot`은 검증된 Pending control의 namespace/batch id와 해당 entry를 기대값으로 받으며 모든 중복 field·slot checksum·image를 확인한 뒤 image slice를 반환한다. slot header의 sequence를 자기 기대값으로 읽지 않는다. 같은 image가 우연히 같은 bytes더라도 stale sequence는 실패한다. control에 없는 slot을 직접 성공 복구 후보로 반환하는 API가 아니다. encoder도 같은 기대값과 image 검사를 마친 뒤 caller prefix를 쓴다. partial/torn slot·sequence mismatch를 Empty로 바꾸거나 이전 slot으로 fallback하지 않는다.

`validate_image(entry,namespace,image)`는 exact16 KiB, 전체 image digest, 공통 page envelope와 expected DB/PageRef/owner/generation/kind/level/pageLSN을 모두 확인한다. control에 담긴 image LSN은0일 수 없다. INIT을 포함한 logged image만 DWB pending으로 선택할 수 있으며 unlogged pageLSN0을 WAL 장벽 없이 쓰는 허용이 아니다.

공통 envelope 밖의 tree/status/undo/allocator cell 의미는 각 소유 decoder가 추가 검증해야 한다. image가 정확해도 WAL durable prefix, FILEMAP 주소/길이, home의 최신 LSN/incarnation, batch 전체 file sync, retirement를 인증하지 않는다. healthy home이 더 높은 LSN/incarnation이면 거절하고 오래된 image로 덮지 않는 판단은 실제 repair의 후속 의무다. 이 codec은 home bytes를 입력받거나 repair 필요 여부를 결정하지 않는다.

## 3. 무효과 API와 실패

소유 파일은 `storage/doublewrite_control.rs`, `doublewrite_control_tests.rs`다. 기존 page/identity/Namespace/SHA-256만 사용하고 새 crate·public API·source root를 추가하지 않는다. 실제 소비자가 연결되기 전 파일 내부에 이유를 적은 dead_code 허용을 둔다.

| API | 의미 |
| --- | --- |
| `ControlHeader`, `ControlExpectation`, `PendingImage` | 제공/기대 값. live slot이나 영속 선택 capability가 아님 |
| `control_encoded_len(header,entries)` | 모든 입력·목록을 검증한 exact length |
| `encode_control(header,entries,output)` | caller prefix만 쓰고 tail 보존 |
| `decode_control(input,expected)` | exact 전체 bytes를 검산한 borrowed ControlView |
| `ControlView::header`, `len`, `entry(index)` | 검증한 header 및 bounds checked entry 읽기 |
| `validate_image(entry,namespace,image)` | trusted control의 해당 image 일치 검산 |
| `encode_slot(control,index,image,output)`, `decode_slot(control,index,input)` | 검증된 Pending control의 index에서 기대 slot/sequence/identity를 얻어 exact slot envelope와 image를 대조 |

encoder는 first write 전에 header·모든 entry·slot/address 중복·maxLSN·capacity를 검사한다. output 오류는 전체 bytes를 보존하며 input을 바꾸지 않는다. 작은 header/entry/bitset scratch만 허용하고 control/page 크기 scratch는 caller 소유다. decoder는 중간 prefix·손상 control을 Empty로 바꾸지 않는다. complete checksum 실패는 항상 오류다.

오류는 InvalidInput, NoSpace, Corrupt, UnsupportedFormat, IdentityMismatch, ImageMismatch다. caller shape/범위와 무효 기대값은 InvalidInput, output 부족은 NoSpace, wire 길이/상태/tag/reserved/count/순서/중복/상한은 Corrupt, version1 외 형식은 UnsupportedFormat이다. 유효 namespace/generation 기대값과 불일치는 IdentityMismatch, image의 길이/digest/envelope/identity/LSN 불일치는 ImageMismatch다. slot의 유효 header가 선택 control의 slot index/namespace/sequence/target/LSN/kind/level/image digest와 다르면 IdentityMismatch다. slot 길이/magic/reserved/checksum 손상은 Corrupt, version은 UnsupportedFormat이다. 여러 오류의 우선순서는 고정하지 않는다.

## 4. 독립 vector와 실행 전 분모

제품 source 전에 Python struct/hashlib로 header·entry·공통 page 표에서 세 control과 image bytes를 생성하여 `artifacts/dev3/doublewrite-format/`에 보존했다. expected를 candidate encoder로 만들지 않는다. Rust 시험은 독립 literal header/entries·고정 digest를 사용한다. `golden.py`와 `golden.json`은 재실행 때 기존 bytes가 다르면 실패하며 덮어쓰지 않는다.

기본 DB=[0x11;16], timeline17, control generation19다. Empty는 batch id0/count0/maxLSN0. mixed Pending은 batch id23, entry0=(slot0,FileId0x100000002,page3,inc4,owner5/gen6,RowLeaf/level0,LSN7), entry1은 slot1/page4/LSN8이며 나머지는 같다. max Pending은64개로 slot=3i, page=100+i, LSN=10+i(i=0..63), 나머지 field는 mixed entry0과 같다. 공통 page image는 empty envelope이며 여기서는 row cell 의미를 주장하지 않는다.

| control | 끝 checksum | 전체 SHA-256 |
| --- | --- | --- |
| Empty160 | `67ef3567910a3f1ccbfe1be4ffca43a8247aae17e28ee31f1a9b22ed4a776ccd` | `7733594cfa9fe7ed3e85a0e4b33d4bb8e4193f0b8eca7669071147c60b227993` |
| mixed336 | `9f64b1698f5aee3ea0fd438c68797055ba4e0a68f59673154125058bc3e96b27` | `722e5b99e2733499af807a639c9238513c343bba389ef40b82b77ddda3e8b6f5` |
| max5792 | `8983bde8f350a29b83d3dcf853890d88acd57e67b1607bf93c00b5c784a7d5db` | `449a1b2801832c98493a926a5004f2657aae0745d5a70d14ab1ea33217882c17` |

동일 image를 감싼 sequence23의 slot envelope는 별도 `artifacts/dev3/doublewrite-format-r1/slots.py`, `slots.json`, `slot-0.bin`, `slot-1.bin`에 보존했다. 첫 raw image vector는 덮어쓰지 않았다.

| slot | 끝 checksum | 전체 SHA-256 |
| --- | --- | --- |
| slot0/page3 | `7e782042cacc679c7ca0d1d7bcc9e9a935dbdff23814e0500a98896023fa4be1` | `e011ce9dd906f7e83cb7dca8b6a28d099410eccde490895a68fe3113fd88bd56` |
| slot1/page4 | `38b245e050a5f7e25c397d14c35e7c57f8aa8861de7f526b5cd6cd1057cff3b3` | `a877adff2af244481d7daf681dd4848257adebfbd2be45f96ddaae82d9bee7a5` |

다음10개 test를 고정한다.

| test | 고정 범위 |
| --- | --- |
| `doublewrite_literal_controls_and_images` | Empty/mixed/max의160/336/5792 bytes·full digest·모든 entry/image 일치 |
| `doublewrite_lengths_and_checksum_damage` | mixed의 잘린 길이0..335와 trailing1 모두 거절337; 각336 byte bit flip 모두 거절; 입력 bytes 보존 |
| `doublewrite_checksummed_semantic_damage` | 아래32개 의미 손상, digest를 다시 계산한 뒤 거절 |
| `doublewrite_addresses_slots_and_expectations` | same address/different incarnation·duplicate slot·descending address 거절3, different FileId/same page 정상1; expected DB/timeline/generation 불일치3, invalid expectation timeline/generation0 거절2 |
| `doublewrite_image_validation_binds_every_field` | 정상2 image. length short/trailing·payload damage·DB/file/page/incarnation/owner/generation/kind/level/LSN 불일치12개; identity 손상은 해당 page의 내부 checksum과 entry full digest를 재계산하여 target 대조 검사 |
| `doublewrite_exact_bounds_and_empty_retirement` | Pending count0/1/64/65, slot0/1009/1010, Empty batch0/MAX와 generation1/MAX.1·64 정상,0·65 거절; slot0·1009 정상/1010 거절. Empty maxLSN/count0 유지 |
| `doublewrite_encoders_preserve_outputs` | mixed의 output 길이0..335 거절336, 모든 caller shape/마지막 entry/중복/maxLSN 오류 sentinel 보존; 성공 prefix 뒤 tail 보존 |
| `doublewrite_fixed_profiles_have_bounded_inventory` | seed0..63×128=8192 control. 아래 원자료와 독립 field/순서/합계 검산·roundtrip |
| `doublewrite_slot_sequence_and_envelope_are_bound` | mixed의 slot2개 literal·exact16,544 bytes, 짧은 입력0..16,543와 trailing1 거절16,545. target·image가 같아도 stale sequence이면 거절. 다른 slot index·DB/timeline·target/owner/LSN/kind/level·digest·reserved·slot checksum 손상을 개별 확인 |
| `doublewrite_slot_encoders_preserve_outputs` | output 길이0/16,543·Empty control·범위 밖 entry index·다른 image는 전체 output 보존; 정상2개 prefix 뒤 tail 보존 |

32개 checksummed 손상은 header14개와 entry18개다. header: magic,version2,state2,reserved7,DB0,timeline0,generation0,Pending batch0,count0,count65,reserved50,maxLSN0,maxLSN7,reserved64. entry0:slot1010,kind0,kind12,level1,reserved4,FileId counter0,FileId epoch0,incarnation0,owner0,owner generation0,LSN0; entry1:duplicate slot0,descending page2,duplicate page3,different-incarnation duplicate page3/inc5,owner0,reserved7,LSN9(maxLSN8 유지). N01–N32처럼 지역 번호를 부여하며 M05 모델 case 분모와 혼동하지 않는다.

image12개는 short16383,trailing16385,원 image의 byte200 손상 및 DB/file/page/incarnation/owner/generation/kind/level/LSN의9개 대조다. level 불일치는 expected target을 유효한 internal kind/level1로 조립하고 image는 같은 internal kind의 level2로 만들어 공통 level 유효성 검사만으로 거절되지 않게 한다. 다른8개 identity/LSN 불일치도 유효 범위의 다른 값이며 내부 checksum과 전체 image digest를 맞춘 뒤 expected 대조로 거절한다.

generated profile의 n=128×seed+sample이다. n mod5=0은 Empty, 나머지는 Pending이며 N=1+(n mod4)다. DB/timeline은 기본값, generation=1+n,batch=1+n, 각 entry의 slot=(n+17i)mod1010,page=100+8n+i,incarnation=1+n,owner=5+i/gen6,RowLeaf/level0,LSN=100+8n+i다. digest byte j는 `(n+7i+31j)mod256`이다. 이 합성 digest는 실제 image 존재를 주장하지 않으며 control metadata 검산만 한다. 별도 고정 image 시험이 실제 bytes를 검사한다.

최초 초안의1,024 image-only slot과 sequence 누락을 `/root/dev3_model`의 독립 검토에서 발견했다. 제품 구현/실행 전에 위 pool 회계와 slot envelope/sequence 및 추가2개 시험을 보강했다. 첫 초안·control/image vector는 `artifacts/dev3/doublewrite-format/contract-draft-r0.md`에 보존한다. control bytes는 기존3개 vector와 동일하지만 slot envelope와1,010-slot 경계는 별도로 생성·동결한다. 이 수정은 기존16 MiB 상한과 M05 sequence 의무를 완화하지 않는다.

실제 partial I/O·WAL-before-page·선택 control·batch 전체 home sync·retirement/재사용·재장애·DWB16 MiB 물리 회계와 bootstrap 인수는 후속 F04/F05·S2/S3의 미완료 의무다.

2026-10-21 실행 전 구현 세부 고정: 두 slot literal 외 metadata 손상19개는 magic/version/slot index/DB/timeline/sequence/FileId/page/incarnation/owner/owner generation/LSN/kind/level/image digest/reserved/image payload/image 내부 checksum/slot checksum이다. 마지막 slot checksum 자체 손상 외에는 slot checksum을 재계산한다. 별도로 같은 target/image에서 선택 control batch id만23→24로 바꾼 stale slot 거절 및 control·slot의 digest가 서로 일치해도 비canonical page를 거절하는 경우를 검사한다. 고정10개 test와 기존분모는 유지한다. 구현 담당은 `/root/dev3_model`, 최종 독립 source 검토 담당은 `/root`이며 이전 계약 검토와 구별한다.

2026-10-21 `/root`가 구현 source·고정10개 시험을 독립 정적 검토하여 차단 지적 없이 수용했다(`artifacts/dev3/doublewrite-independent-root/review.json`). 첫 보존 source에서10tests는 PASS였으나 Clippy의 시험 반복문 지적으로 합산 결과는 FAIL로 보존했다(`artifacts/dev3-doublewrite-model-r1/validation.json`). 반복문만 iterator/enumerate로 고치고 동일분모를 새source에서 재검증한다. 첫 live-cwd 오호출은 별도 NOT_QUALIFIED로 보존했고 clean source 실행으로 세지 않았다. 실제 DWB I/O/복구 인수는 계속 미완료다.

2026-10-21 최종 작성자 국소 검증: `artifacts/dev3-doublewrite-model-r2/validation.json`의 보존 source에서고정10tests·생성8,192 controls·engine all-targets/all-features Clippy·소유파일rustfmt가 PASS다. `/root`가 두 줄 반복문 수정도 독립 확인하여 corpus/판정불변을 수용했다. 제품 codec bytes/조건은 첫 source 검토 때와 같다. 실행 당시 계약 사본은 r2/source에 보존하며 이 문단과 상태만 실행 뒤 갱신했다. 통합 runner·실제 DWB I/O·전체 DEV3 인수는 이 국소 receipt로 대체하지 않는다.

## 최초 DWB 설치의 후속 D2

[설치 계약](dev3-doublewrite-install.md)은 기존 XDW1 Empty와 XCS1 selector를 실제 fresh 파일에 설치하는 순서를 고정한다. d3-dwb-stage의 최초 phase는 installer 전용 XDI1이며 generic XDW1 decoder나 resume authority로 사용하지 않는다. 후속 batch·home write·retirement는 미구현이며 이번 설치도 제품 실행 NOT_RUN이다.

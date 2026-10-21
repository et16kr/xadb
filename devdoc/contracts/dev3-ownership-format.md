# DEV3 ownership inventory의 bytes·물리 충돌 검산

2026-10-21 · **D2 수용 / 제한된 관측 구현·독립 재현·같은 source 통합 회귀 PASS**

[obligation의 §4](dev3-obligation-format.md#4-큰-ownership-inventory) · [control](dev3-control-format.md) · [allocator](dev3-allocation-format.md) · [status home](dev3-status-home.md) · [구현 기록](../implementation/dev3-implementation.md)

작성·구현·통합은 `/root`, 독립 검토는 `/root/dev3_selector_review`다. 이 계약은 immutable ownership root/chunk·Item96의 표현과 전체 bytes·물리 충돌·owner별 budget/digest 검산을 공급한다. intent 유일성은 이 결과에 포함되지 않는다. 실제 선택 파일의 I/O, checkpoint 설치, WAL delta 재생, slot admission과 reference retirement는 포함하지 않는다. root의 owner 상한36은 이 표현의 한도이며 system4·ATT32의 실제 진행 보장이나 F05 충분성의 증거가 아니다. 채택 전 제품 코드를 작성하지 않는다.

## 1. 선택과 경계

obligation 초안의 kind-first 정렬과 원래 Item96/root/chunk layout을 유지한다. 단순 전체 RAM 목록이나 O(N²) cross-kind 재검사 대신 **네 개의 정렬된 kind stream을 병합**한다. 네 cursor가 각각 전체 chunk를 한 번씩 읽고 자기 kind의 항목만 반환한다. cursor마다 전체 checksum·root aggregate를 확인하며 마지막 이후도 drain한다. owner별 digest/count는 첫 sequential pass에서 검산한다. 성공 때 전체 source read는 정확5C회, 각 chunk는5회다. N=0이면 read0이다. 한 kind가 없거나 후반 chunk에 있어도 이 상한은 같다.

이렇게 하면 original canonical bytes와 owner digest를 바꾸지 않고 O(N) 항목 검사를 할 수 있다. 각 lane의 current item 중 physical key가 가장 작은 것을 고르는 비교는 최대4개다. heap/tree나 inventory 크기에 비례한 allocation이 없다. 현재 항목·이전 physical page 상태·최근 growth interval만 유지한다. callback이 관측된 bytes를 다르게 공급하면 각 pass의 전체 root aggregate 검증이 실패하며 부분 성공은 반환하지 않는다. 이미 읽은 bytes의 이후 변경, ABA·마지막 read 뒤 변경을 감지한다고 주장하지 않는다. 선택된 source의 보존·잠금은 caller 책임이다. 검산 성공만으로 그 후 파일 변경을 방지하지 않는다.

비교 자료는 local PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의 `src/backend/utils/sort/tuplesort.c:mergeonerun`·`beginmerge`·`tuplesort_merge_order`다. 2026-10-21 작업 트리는 clean이었다. 각 정렬 입력의 최소값을 뽑고 같은 입력에서 다음 값을 채우는 원리와 입력별 buffer 비용을 확인했다. PostgreSQL 코드·tape 구현·성능 결과를 가져오지 않으며 XADB의 고정4종 검산과 동일한 제품 의미라고 하지 않는다.

## 2. ControlRef 등록과 선택 기대값

기존 ControlKind1..4에 OwnershipRoot5/OwnershipChunk6을 추가한다. 두 kind의 bank는0..2다. Root는 index0·logical length `160+56O`, O=0..36이다. Chunk는 index:u32·logical length `160+96I`, I=1..169다. Root는 chunk index를 u64로 저장하지만 모든 index가 ref의 u32에 들어가도록 C≤2³²를 검사한다. 실제 파일 quota의 허용을 뜻하지 않는다. generation은 nonzero이며 ref digest는 logical bytes 전체 SHA다. 기존 kind1..4의 bank/length/index 규칙은 그대로다. 기존 selector는 FILEMAP/DWB 전용이므로 새 kind를 선택하지 않는다. 새 kind의 물리 파일명은 아직 동결하지 않았으므로 `control_file_name`은 두 새 kind에 UnsupportedFormat을 반환한다. callback의 index 공급과 native namespace 설치를 혼동하지 않는다. CURRENT/WRITER 성공 decoder도 추가하지 않는다.

신뢰한 기대값은 namespace(DB/timeline), bank, inventory generation, capture LSN, exact root ControlRef, canonical owner descriptor slice와 `item_limit:u64`다. item_limit은 caller가 허용한 검산 작업량이며 제품 N_max나 자원 admission 값이 아니다. root N>item_limit이면 source read 전에 LimitExceeded로 거절한다. 0은 빈 목록만 허용하며 무제한을 뜻하지 않는다. expected root는 kind5이고 generation/bank/length가 기대값과 일치해야 한다. 잘못된 기대값은 InvalidInput이다. 오류 우선순위는 §5와 같다. root 입력의 owner를 expected owner로 복사하지 않는다.

owner descriptor는 기존 codec으로 유효함을 재검증하고 apply/change/minimumWal LSN≤capture를 확인한다. Transaction은 Active/Aborting, System은 Prepared/Running/Finishing만 수락한다. terminal descriptor는 이 active closure의 owner가 아니다. 원래 TxnRef/SystemWorkRef를 유지한다. Transaction은 최대32, System은 최대4이며 각 SystemWorkKind는 최대1개다. 모든 owner namespace가 기대값과 같아야 한다. descriptor Profile ref는 서로 같아야 하며 caller의 expected profile과도 같아야 한다. 실제 profile 존재·물리 quota는 이 검사 밖이다.

표의 순서는 Transaction 먼저 `(epoch,local,generation)` numeric, System은 `(kind,id,generation)` numeric다. DB/timeline은 공통 기대값과 결합하므로 정렬에서 생략한다. 동일 full owner는 중복 거절한다. 동일 TxnId의 다른 generation이나 동일 System(kind,id)의 다른 generation도 한 active 표에서 alias로 거절한다. index 변경에 따른 owner digest의 ordinal 정규화 의미는 유지한다.

## 3. Item96의 유효 범위

obligation §4.1의 byte offset을 그대로 사용한다. category1..5, owner ordinal<O, changeLSN는1..capture이다. 각각 owner의 inventory_change_lsn 이하이기도 해야 한다. 별도 item checksum은 없고 enclosing chunk·root aggregate가 보호한다.

| kind | identity56·추가 조건 |
| --- | --- |
| PageToken1 | PageRef24+zero32. page index는 extent 내1..63이며 해당 extent 끝의 byte offset이 u64 범위. units1 |
| UndoSlot2 | UndoRef40+zero16. page는 위와 같은 data page, slot0..36, page incarnation/slot generation nonzero. units1 |
| StatusSlot3 | StatusSlotRef40+zero16. page는 위와 같고 **home224의 slot0..70**. 기존 core192의82-slot 상한을 사용하지 않음. units1 |
| UnrealizedGrowth4 | FileId8/base8/count8/intent8+zero24. base%64=0, count·intent nonzero, end=base+64count와 end×16384가 checked, units=count×1048576 |

PageRef/slot의 envelope 유효성은 actual allocated/native page·slot 존재·admission의 증거가 아니다. Reserved page와 이미 Allocated된 page 내부 slot을 실제로 대조하는 의무는 allocator/slot consumer에 남는다.

정확한 canonical numeric key는 `(kind,file,page_or_base,slot_or_zero,page_incarnation_or_zero,slot_generation_or_zero,intent_or_zero,owner_ordinal,category)`다. 기존 초안의 모호한 generation은 page/slot 두 값을 모두 포함하도록 구체화한다. growth count·units·changeLSN은 key가 아니다. 같거나 감소한 key는 거절한다. 같은 physical resource는 owner/category/generation이 달라도 아래 overlap 검사에서 거절한다.

physical merge key는 `(file,page_or_base,kind,slot_or_zero,page_incarnation_or_zero,slot_generation_or_zero,intent_or_zero,owner_ordinal,category)`다. 각 kind lane에서 canonical 순서가 이 key의 증가 순서와 같으므로4-way merge가 가능하다. 같은 시작점의 growth는 앞의 page/slot과 비교하고 다음 item부터는 interval로 비교한다. 반열린 구간 `[base,end)`를 사용한다.

- 동일 file/page의 PageToken 둘, PageToken과 어느 slot이든 중복이다.
- 같은 page의 동일 종류 slot은 서로 다른 slot 번호만 공존할 수 있다. page incarnation은 같아야 한다. slot generation·owner/category를 바꾼 같은 slot alias는 거절한다.
- 같은 page에 UndoSlot과 StatusSlot이 공존하면 물리 page kind가 양립하지 않으므로 거절한다. slot 번호가 달라도 같다.
- growth끼리 겹치거나 growth 안에 page/slot이 있으면 거절한다. 정확히 end에서 시작하는 다음 extent는 정상이다. file이 다르면 겹치지 않는다.
- owner-local growth intent sequence 재사용 금지 의미는 유지한다. 이 검산기는 같은 owner에 같은 intent가 서로 다른 interval로 나타나는 중복과 과거에 회수된 sequence의 재사용을 검증하지 않는다. 이 한계는 §5의 후속 의무와 명시 정상-범위밖 시험으로 고정한다.

## 4. Root·chunk와 합계

root/chunk offset은 obligation §4.2를 사용하며 magic XOI1/version1, root160+56O, chunk160+96I다. reserved는 전부zero이고 입력은 exact logical length다. unknown version은 UnsupportedFormat, malformed input은 Corrupt다. root owner identity bytes56은 기존 OwnerRef와 동일하고 caller descriptor의 reference와 하나씩 대조한다.

O=0이면 N=C=0이며 aggregate는 SHA256(empty)다. O>0일 때도 N=0을 허용한다. C는 overflow 없이 `N/169 + (N%169!=0)`이며 root chunk logical sum은 checked `160C+96N`이다. 각 chunk index·total C·first ordinal169index·owner-table SHA·namespace/bank/generation/capture가 정확히 root와 같다. 마지막 외에는169개이며 마지막은 N−169index개다. trailer digest를 확인하고 index 순서 전체 logical bytes를 hash한다. last chunk 손상이나 pass별 바뀐 bytes도 실패다.

첫 sequential pass는 owner별 SHA state·count와 category별 page/undo/status/growth 합계를 고정 배열로 유지한다. owner digest에 넣을 item96은 ordinal2 bytes만0으로 만든다. 나머지 bytes는 그대로다. 마지막에 descriptor의 inventory items/digest, 각 budget pages/undo_slots/status_slots/growth와 exact 비교한다. 각 count/sum은 checked u64다. item changeLSN의 최댓값이 descriptor changeLSN와 같을 필요는 없다. 마지막 release 때문에 descriptor 값이 더 클 수 있다. 빈 owner의 SHA는 empty다.

## 5. API·공간·성공 범위

private `storage/ownership.rs`·`ownership_tests.rs`에서 caller 소유의 root 입력·descriptor slice와 `[[u8;16384];4]` scratch를 받는다. `InventorySource::read_chunk(index:u64, output:&mut [u8;16384]) -> Result<usize, InventoryError>`는 요청 index의 exact logical bytes를 공급하며 반환 길이가 예상과 다르면 거절한다. callback 오류는 Source로 전달한다. native I/O adapter나 성공 stub을 추가하지 않는다.

`verify_inventory(root, expected, descriptors, source, scratch)`는 성공 시 `InventoryObservation`에 item/chunk/owner 개수와 read calls를 반환한다. 이 타입은 `InventoryClosureVerified`·`ReservationRestored`가 아니며 그런 성공 capability로 변환하는 API를 제공하지 않는다. 실패는 complete proof를 반환하지 않는다. 모든 기대값·root·descriptor 검산을 read 전에 한다. 그 뒤 오류에서는 scratch가 변경될 수 있고 callback read 효과를 rollback하지 않는다. verifier 자신은 source에 write를 요청할 수 없다. bytes decoder의 잘못된 입력은 panic/무한루프를 만들지 않는다. N비례 heap은0이며 callback 자체 자원은 caller 의무다.

고정 working set은 scratch64KiB, 최대36 owner의 hash·count/budget 통계,4 cursor, validate-only descriptor 검산과 item96이다. root bytes≤2176와 최대36 descriptor는 caller 입력이다. 기존 obligation의 validate를 재사용하는 `validate_descriptor(&DescriptorSpec)` helper를 추가하여 encoder의 중첩1536+1536-byte scratch를 만들지 않는다. implementation size_of 증거에서 private verifier state≤14KiB, 가장 깊은 call의 고정 local/배열 보수적 합≤2KiB로 입력 제외 총 workspace≤80KiB임을 검사한다. compiler stack-frame/OS thread stack의 실측 상한으로 해석하지 않는다. 이 수치는 전체 BOOT_RESERVE12MiB 완료가 아니다.

growth intent의 owner-local 유일성은 이 leaf에서 미검증으로 남긴다(A 선택). sequence 의미를 검산기에 맞추어 resource-local로 바꾸는 대안 B와 immutable intent순 보조 목록 C를 비교했다. canonical key가 interval 순서이므로 intent 중복은 물리 merge에서 드러나지 않는다. 실제 RESERVE_RESTORE/완전한 closure 성공 전에는 **현재 inventory 안의 intent 유일성 검산**과 과거 재사용 금지 검산이 별도로 필요하다. 이후 WAL 적용만으로 checkpoint 이전의 중복을 잡을 수 없으므로 이를 후속 WAL scan 의무로만 전가하지 않는다. C와 같은 추가 표현·경로의 D2 수용과 실제 시험 전에는 현재 Observation을 복구 성공으로 승격하지 않는다. 원래 §4의 모든 의미를 검사하는 것처럼 이름 짓지 않는다.

오류 순서는 (1) 기대 context와 descriptor의 잘못된 값·순서·중복·상한은 InvalidInput, (2) root의 exact length/magic/layout·reserved·count/산술·checksum·owner wire/order 결함은 Corrupt(유효 envelope의 unknown version만 UnsupportedFormat), (3) 구조가 유효한 root의 namespace/bank/generation/capture/전체 digest/owner 목록이 기대값과 다르면 IdentityMismatch다. 기대 identity와 결합된 root의 N>item_limit은 LimitExceeded다. root가 유효해도 descriptor의 counts/digest와 맞지 않으면 첫 pass 끝에서 Corrupt다. callback 오류는 Source, 잘못된 반환 길이는 Corrupt다. chunk의 형식/체크섬을 먼저 확인하고 유효한 header identity가 root와 다르면 IdentityMismatch, item/순서/합계/전체 aggregate 차이는 Corrupt다. 오류 뒤 resume 기능은 없고 새 호출로 시작한다.

## 6. 독립 literal과 고정12시험

독립 원문은 `artifacts/dev3/ownership-vectors-r0/vectors.py`·`vectors.json`이다. product codec을 import하지 않고 정수·offset·SHA로9개 fixture를 생성했다. 이전 obligation r0 fixture를 덮어쓰지 않는다. PROFILE는 기존 descriptor와 같은 reference-only4096-byte fixture의 ref이며 실제 XDC1 profile이라고 주장하지 않는다. 이 계산 결과는 실행 증거가 아니다.

| literal | bytes | SHA-256 |
| --- | --- | --- |
| `empty-root.bin` | 160 | `2363c5ec81801d5d22432f0977e84eaae8a94088e2540bee957c9a67e211f289` |
| `mixed-root.bin` | 272 | `bc6d52386e0b4cf5eec50687eaff3b50c539ed675a6c4ca5cd3a76d733aacd9a` |
| `mixed-chunk.bin` | 640 | `44b1d24778b2f59e4ae84c83b936339f2ecad1735f64cae17e72c36bfcb1db26` |
| `mixed-txn.bin` | 1536 | `5a0fb002850f9d69275ff7a5c7ff27f9c89831614c664b948e693a539bfeb451` |
| `mixed-system.bin` | 1536 | `d5238d0d2bc351a69dacf081e0e2b62e2c83ec06eb7cd1022956c744bcb1f32e` |
| `boundary-root.bin` | 216 | `63df1f0e5c7bf93411466d66082e9929823fc0ec5a1944b7020fa84c1ce7a8f6` |
| `boundary-chunk0.bin` | 16384 | `9ac04781b96bd63f74173e6eb20e71069c413a062fbeef020ea68ad108474cff` |
| `boundary-chunk1.bin` | 256 | `ceecc34fd869d98bdcf5e3c0b70c4bb49bc08c7c789dc6ff5f04859784ecea7a` |
| `boundary-txn.bin` | 1536 | `d43acbc5de5da75354ca799693728da33a3913101c58895ffefda53e418a4a56` |

DB는 bytes1..16, timeline17, FileId=(2<<32)|1, root generation/capture60이다. mixed의 owner는 Transaction(epoch18,local19,generation20), System(AllocationMaintenance1,id21,generation22) 순서다. item은 Page(65,inc9,owner0,cat1,lsn30), Page(66,inc10,owner1,cat2,lsn50), Undo(page67,inc12,slot0,gen11,owner0,cat1,lsn30), Status(page68,inc13,slot70,gen14,owner0,cat4,lsn30), Growth(base128,count1,intent1,owner1,cat2,lsn50)다. 모든 descriptor apply/birth60·groups1, change60이고 count/digest와 category budget은 위 item들에서 독립 계산했다. mixed bank1/C1/N5, empty bank0/O0/C0/N0이다. boundary bank2/O1/C2/N170이며 Page i의 page=64+64floor(i/63)+1+i%63,inc9/owner0/cat1/lsn30이다. chunk0은169개·16384bytes, chunk1은1개·256bytes다.

| 고정 시험명 | 고정 범위 |
| --- | --- |
| `inventory_literals_bind_all_bytes_and_owners` |9 literal의 hash·길이·raw field, mixed5·boundary170·empty0 관측. mixed 기본 read5·boundary10·empty0. descriptor의 원본 fields·budget 비교 |
| `inventory_root_chunk_truncation_and_damage_reject` | root160/272/216의 모든 proper prefix와 trailing1, mixed chunk640·boundary chunks16384/256의 모든 proper prefix와 trailing1 거절. empty/mixed root·mixed chunk의 각 byte bit0 총1072개 변형 거절 |
| `inventory_reserved_and_header_damage_reject_after_checksum` | root58..63/120..127, chunk84..87/120..127, 각 Item4..7/80..95·kind별 reserved, owner prefix1..7의 각byte를 fresh original에서 바꾸고 enclosing checksum/hash 갱신하여 거절. 아래 header matrix도 전부 거절 |
| `inventory_expected_owners_and_capture_are_required` | namespace(DB/timeline), bank/generation/capture/ref digest mismatch와 source read0; 기대namespace를바꾼경우descriptor namespace도같이바꿔유효입력을만든뒤원래root와IdentityMismatch를확인. descriptor profile과expected profile 불일치는InvalidInput/read0; typed invalid descriptor, terminal 상태, apply>capture, owner누락·추가·순서·generation alias 거절. O0/36 정상과37거절, Txn32 정상/33거절, system4 정상/동일kind2거절 |
| `inventory_item_bounds_and_generations_are_checked` | 각kind tag/category/owner/changeLSN·metadata page·slot boundary37/71·zero generations·units·growth 정렬/0/overflow를 거절. 각 경계의 인접 정상값 포함 |
| `inventory_canonical_order_spans_chunk_boundaries` | 정상169/170과 chunk 경계의 duplicate/reverse, ordinal/index/total/count/owner table hash 변경. 첫·중간·마지막 canonical key 감소 거절. little-endian byte 순서와 numeric 순서가 다른 file counter255↔256·page255↔257의 정상 목록을 확인한다(page256은 metadata). incarnation/slot generation/intent255↔256은 나머지 선행 key가 같은 개별 유효 Item.key 순서를 확인하고 두 항목의 결합 목록은 physical alias로 거절한다. 유효 slot은0..36/70이므로 slot의 byte-order 차이를 주장하지 않음 |
| `inventory_owner_digests_and_category_budgets_are_exact` | ordinal 정규화로 다른 owner가 추가되어도 대상 digest 유지. owner0/1 각각 count·digest와 category별 page/undo/status/growth 오차를 아래 유효 descriptor 규칙에 따라 거절. changeLSN은 각owner item max−1(29/49)에서Corrupt, max(30/50)와60에서정상,61은InvalidInput/read0. release 이후 changeLSN>현재item max와 O>0/N0 정상 |
| `inventory_physical_aliases_and_intervals_match_pairwise_oracle` | 아래18개 고정충돌·인접 정상, 독립 pairwise predicate와4-way 결과를 비교하는 유한969개 model corpus를 Rust fixture 입력으로 재구성. pure 모델 계산은 제품 시험으로 대신하지 않음 |
| `inventory_all_lanes_drain_and_bind_repeated_bytes` | C2의 두번째chunk가 각1..5번째 방문에서 논리적으로 달라지되 자체 checksum은유효한5사례 모두 거절. 없는kind lane의 late tail도 검증. root aggregate만 틀림·빠진tail 거절 |
| `inventory_source_failures_and_workspace_are_bounded` | C1의 read1..5 각각 callback실패/짧은반환/긴반환15셀; 오류후 성공 관측없음. 성공5C와workspace size, N0 source미호출. item_limit N−1/N/N+1와0 경계, 초과 시source read0; source input/root/descriptor 불변 |
| `inventory_intent_uniqueness_is_explicitly_not_proven` | 동일 owner+intent의 다른file 또는 비겹침base2사례는 Observation에포함될수있음을 확인. 이를완전한closure/RESERVE_RESTORE성공이라고표시하지않음. 같은physicalrange의alias는여전히거절 |
| `inventory_generated_profiles_match_independent_fields` | seed0..63×sample0..127=8192. 아래 independent raw 생성식으로count·digest·budget·order·5C검산 |

header matrix는 root magic/format/headerbytes/itembytes/length/O/C/N/chunkbytes/aggregate/ownerorder 및 chunk magic/format/headerbytes/itembytes/length/bank/DB/timeline/generation/capture/index/total/firstordinal/count/ownerhash다. 값은 각형식 상수의±1(0미만 제외), identity는다른유효값, count는기대±1이며 checksum과rootaggregate를새로계산해단순SHA실패로대신하지않는다. unknown version의결과는UnsupportedFormat이다. owner prefix와 identity 자체가잘못된것은Corrupt, 유효하지만기대와다른것은IdentityMismatch로고정한다.

owner 합계 오차 시험은 count를±1 바꿀 때 page budget도 같은 방향으로 바꾸어 descriptor cardinality를 유효하게 유지한다. digest 오류는32-byte digest 한 byte 변경이다. category 오차 전용 정상 원자료는 owner별5개category 각각 page/undo/status token2개씩과 count2인 growth item1개로 만든35개 item이다. 각 category의 page/undo/status를±1 바꿀 때 inventory count도±1 바꾸고, growth를±1 **extent(1MiB)** 바꿀 때 inventory.growth_extents도±1 바꾼다. steps1과 다른필드는유지하며 실제 item bytes는그대로여서유효 descriptor와의 불일치가Corrupt다. byte단위growth±1이나count만바꾼invalid descriptor를 first-pass Corrupt 증거로 쓰지 않는다. 추가로 owner2×category5×token3=30개에서 category A의 token을+1, 이웃 B를−1로 바꾼다. inventory count/digest·전체 token 합을 유지한 유효 descriptor이므로 category 비교 자체가 빠진 경우를 드러낸다. 원래80개 오차 사례도 유지한다.

18 충돌은 Page/Page exact·다른inc·다른owner·다른category4, Page/Undo·Page/Status2, Undo/Undo sameSlot diffSlotGen·diffPageInc2, Status/Status sameSlot diffSlotGen·diffPageInc2, Undo/Status sameSlot·differentSlot2, Growth/Growth exact·contained·partial3, Growth/Page·Growth/Undo·Growth/Status3이다. 각 충돌의 second item을다른page/slot(같은kind slot만)/인접growthend로옮긴18정상도확인한다. pairwise corpus는 `artifacts/dev3/ownership-design-review-20261021/r0-merge-model.py`의 pool16종을 사용한다. FileId f는(2<<32)|f로 대응하고 category1,changeLSN30을 붙인다. pool의0..3개 중복조합 전부(1+16+136+816=969)를 raw item으로 구성하며233정상·736거절을 기대한다. 이 범위는 reference 모델의 검토 기록과 제품 입력 시험을 별도로 유지한다.

생성식은 n=128seed+sample, N=1+n%8, owner는위mixed2개, bank=n%3, generation=60+n, capture=100000+n이다. item i의kind=1+i%4,cat=1+(n+i)%5,owner=i%2,lsn=capture−i,inc=9+i,slot=(seed+i)%37(Undo)/%71(Status),slotgen=11+n+i다. non-growth page=65+2i, growth base=256+256i/count=1+sample%3/intent=1+i다. ownerapply/birth=capture/groups1/change=capture이며 raw budget와count/digest는생성rawitems에서별도산출한다. kind/file/page순으로독립정렬한itembytes를chunk에넣는다. count/growth합계는u128원자료로검산하고root/chunk 생성에제품함수를사용하지않는다. 이작은생성목록은169/170고정경계나실제대형I/O시험의대체가아니다.

ControlRef 신규 kind5/6은 기존 `control_selector_bindings_and_lengths` 시험의 알 수 없는 tag 반례를7로 옮기고, 신규 root/chunk 각각 bank0/1/2 정상·3거절, 길이 min/max/잘못된 stride, root index1거절·chunk indexu32MAX 정상, generation0거절을 추가한다. tag5/6이 기존 selector가 소비하는 FILEMAP/DWB role로 들어가면 거절되어야 한다. 나머지 kind1..4 부정 분모는 유지한다. 이 기존 시험 안에서 두 새 kind의 `control_file_name`이 UnsupportedFormat임도 확인한다.

기존 leaf·현재-build DEV2/lower 회귀·Clippy/fmt를 통합지문에서 실행한다. nativefault·thread·write/recovery는 이 read-only primitive의 PASS 범위가 아니다. intent검산·실제slot대조·Nmax/물리profile·selectedsourcepin·WAL/복구의 후속의무는 유지한다.

D2 수용 원문 SHA-256은 `1d67ef69a1e3103c609d2c036a589a364945eda69c86293e59fe81e57a3f70a8`다. 독립 검토 `artifacts/dev3/ownership-design-review-20261021/r1-final-contract-review.md`는9개 literal·생성식·오류 조건을 별도 확인했다. 이 계산·검토는 제품 실행 결과가 아니다.

숫자 순서 corpus의 metadata page·physical alias 구분은 `artifacts/dev3/ownership-design-review-20261021/r2-numeric-corpus-review.md`에서 별도 수용했다. 원래 literal bytes와 충돌·복구 의미는 변경하지 않는다.

제품 구현의 category별 검산 반례30개 보강과 chunk count/length의 Corrupt 선검사를 독립 검토했다. 국소 소스와30개 관련 시험·Clippy/fmt·독립 재현은 [구현 기록](../implementation/dev3-implementation.md#ownership-observation-results)에 있으며 전체 DEV3·예약 복원은 미완료다.

# DEV3 거래 상태의 영속 home

2026-10-21 · **D3-STATUS-HOME-001 D2 계약·구현 독립 검토 수용 / 좁은 표현7시험·같은 소스의 통합 leaf/하위 회귀/현재-build DEV2 복구 PASS**

[거래 core](dev3-transaction-format.md) · [영속 의무](dev3-obligation-format.md) · [참조 수명](disk-storage-design.md#undo-lifecycle) · [공통 변경](disk-storage-design.md#mutation-protocol) · [예약 계산](dev3-resource-bounds.md) · [버전 정책](../policies/version-compatibility-policy.json)

## 1. 결정과 범위

작성·통합 담당은 `/root`, 독립 검토는 `/root/dev3_selector_review`다. 기존192-byte TransactionStatusCell은 상태·rollback cursor의 논리 core로 보존한다. 영속 상태 home는 그 core에 stored incoming count를 결합한 별도224-byte cell로 정의한다. descriptor 안의 core와 실제 status home를 구별한다. 기존192-byte 표현이나 과거 실행 receipt를224-byte 구현으로 재표기하지 않는다.

이 제안은 caller-buffer 표현·읽기 검증만 공급한다. 영속 status directory, count 증감의 실제 저장, WAL 의미 적용, queue·terminal 소유권 인계, pin/lock, slot 재사용·수명 판정은 구현하지 않는다. 특히 count0은 Free 전환이나 재사용 승인이 아니다. 개발 형식은 새 fixture를 사용하고 현재-build 회귀를 별도로 실행한다.

## 2. 정확한 bytes

공통 page는 TransactionStatus kind9/level0이며224-byte home cell만 담는다. 각 cell은 다음과 같다. 정수는 little endian, reserved는0이다. checksum은 page/WAL envelope가 소유한다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | ASCII `XSH1` |
| 4 | 2 | version1 |
| 6 | 2 | home header bytes32 |
| 8 | 8 | stored incoming count:u64 |
| 16 | 16 | reserved0 |
| 32 | 192 | 기존 `XTS1` TransactionStatusCell core |

Free core이면 incoming은 정확0이다. occupied의 Active/Aborting/Committed/Aborted는0..u64::MAX를 표현할 수 있다. 최대값을 읽을 수 있다는 사실은 다음 증가나 재사용을 허용하지 않는다. core의 generation·TxnId·head·rollback 조건은 기존 decoder와 동일하며 새로운 상태 tag를 만들지 않는다. encode는 모든 입력을 검사한 뒤 한 번만 caller output을 바꾸고 오류 때 전체 output을 보존한다. 정확224-byte 출력만 수락한다. decode도 정확224만 수락하며 version 불일치는 UnsupportedFormat, 나머지 잘못된 header/reserved/core는 원래 PageError로 거절한다.

home의 incoming은 Row/Index/Catalog의 Transaction stamp와 undo before Transaction stamp 등 실제 지속 TxnRef edge의 중복도를 센다. undo creator의 불변 TxnId/generation, ATT membership, temporary reader pin, horizon/rollback/queue 소유는 incoming에 합치지 않는다. 어느 native field가 edge인지는 의미 그룹의 별도 필수 section 표가 정한다. 이 leaf는 u64 숫자가 실제 graph의 incoming과 같음을 인증하지 않는다.

같은 atomic group의 removed/added edge 수로 새 count를 계산하는 순수 helper는 `old.checked_sub(removed)?.checked_add(added)`를 사용한다. 실제 removed edge가 old에 포함됐는지는 의미 검증자의 책임이다. underflow/overflow는 InvalidInput이고 wrap/saturate하지 않는다. old=MAX, removed=1, added=1은 MAX다. 변경 순서의 중간 증가 overflow로 이 유효한 원자 순변경을 거절하지 않는다. helper는 cell을 변경하거나 WAL을 쓰지 않는다.

## 3. Page와 물리 참조

home page의 최대 slot 수는 `floor((16,352−128)/(224+4))=71`이며 slot0..70이다. 기존 StatusSlotRef의40-byte 표현과 구 core의 보수적0..81 범위는 유지한다. **home lookup은 slot<71과 실제 page slot 존재를 모두 추가 검사**한다. 새 allocator/directory는 home 한도를 소비해야 하며 old82를 발급 한도로 사용해서는 안 된다. empty page도 검사 가능하지만 모든 lookup은 실패한다.

새 home page loader는 기대 PageIdentity, 기대 Namespace(DB/timeline)를 caller에게 받는다. 기대 namespace timeline0은 InvalidInput이고 page identity DB와 기대 namespace DB 불일치는 IdentityMismatch다. envelope의 모든 identity/kind/level/checksum과 모든 cell을 확인한 뒤 view를 반환한다. 하나의 고른 slot만 맞고 뒤 cell이 손상된 page는 거절한다. 모든 occupied TxnId의 DB/timeline을 기대 namespace와 비교하며 오래된 writer epoch는 유효하다. 한 page 안에서 같은 TxnId가 둘 이상 있으면 generation이 달라도 Corrupt다. 최대71개를 bounded 중첩 순회하여 검사하고 heap inventory를 만들지 않는다. page 밖의 전역 유일성은 directory/full audit 의무다.

home lookup은 caller의 TxnRef와 StatusSlotRef를 받으며 page reference·location generation·core의 전체 TxnRef를 대조한다. Free, 없는 slot, 다른 generation/TxnId/page는 IdentityMismatch다. 성공은 core와 stored incoming의 immutable 값이며 transient pin이나 visibility 권한이 아니다. live directory source latch 해제 전 보호·이후 재검증은 상위 reader가 수행해야 한다.

192-byte old cell과224-byte home를 한 page에 혼합하면 거절한다. old page loader는 그대로 old cell만 읽으며 새 persistence 소비자는 home loader만 사용한다. 자동 판별·변환·양쪽 중 하나 성공 fallback은 제공하지 않는다.

## 4. 내부 API와 자원

private storage의 `StatusHome::new(core, incoming)`, `core()`, `incoming()`, `encode(&mut [u8])`, `decode(&[u8])`와 `adjust_incoming(old, removed, added)`를 제공한다. `decode_status_home_page(bytes, expected, namespace)`는 `StatusHomePage`를 반환하며 `len/cell/lookup`을 제공한다. 오류 타입은 기존 PageError다. source는 구현 시 `storage/status_home.rs`·`status_home_tests.rs`로 배치한다.

encoder는224-byte stack scratch와 기존 core192 scratch 이하를 중첩 사용한다. decoder/page view는 caller bytes를 빌리며 allocator·I/O 호출은0이다. page 전체 Vec 복제나 status 개수 비례 heap 할당이 없다. 최대71 slot의 재decode O(71²)는 bounded 표현 검산이며 실제 status directory 검색 성능의 증거가 아니다.

후속 obligation descriptor의 offset160 core192는 계속 논리 core만 저장한다. NativeEffect Status(kind5)는 **home224 전체 before/after**를 요구하고 group의 Transaction owner 자신의 home를 변경할 때만 descriptor core가 after home의 `[32,224)`와 정확히 같아야 한다. 다른 거래의 TxnRef target home는 안정화한 expected target에 결합하며 count-only 효과이면 core before==after다. actor A가 B의 stamp를 제거하거나 System aging이 B의 count를 바꾸는 경우 B의 core를 A/System descriptor에 복사하지 않는다. B home 변경이 B owner descriptor 변경을 암묵적으로 뜻하지도 않는다. CountChange TxnRef는 동일 group의 해당 home `[8,16)` before/after와 결합한다. core192만 주고 count를 volatile 변수로 별도 적용하는 구현은 승인하지 않는다. 이 세 결합을 실제 검사하는 의미 그룹 codec은 아직 공급하지 않았다.

## 5. Terminal 작업의 명시 경계

Commit/AbortEnd 이후에도 home의 head와 incoming이 남을 수 있다. ATT의 owner descriptor를 제거하고 남은 reservation을 전부 반환하면 queue 작업의 영속 owner/credit을 잃을 수 있다. count0이나 terminal decode 성공으로 이 문제를 해결하지 않는다. durable queue record·소유자·credit의 같은 WAL 그룹 인계가 별도 동결되기 전, 실제 terminal opcode 실행은 Unsupported다. 표현에 Committed/Aborted core가 있다는 사실은 terminal 실행 승인과 다르다.

## 6. 구현 전 고정 시험

독립 literal은 raw offsets와 기존 core의 명시 정수로 작성한다. production encoder 출력이나 decoder 판단으로 기대값을 만들지 않는다. DB=bytes1..16, timeline17, TxnId epoch18/local19, generation20, committed q21이다. 고정224-byte Active/count2, Committed/count1, Free/gen20/count0의3개를 먼저 작성한다. home 자체에 checksum이 없으므로 모든 단일 bit 변경을 무조건 손상으로 간주하지 않는다.

| 고정 시험명 | 분모·검산 |
| --- | --- |
| `status_home_literals_match_core_and_count` | 위3 literal 전체 bytes·직접 정수 offset 검산; occupied count0/1/MAX |
| `status_home_rejects_lengths_headers_and_free_references` |3 literal×모든 prefix0..223 및225, magic4byte 각각·version0/2·header31/33·reserved16byte 각각·Free count1/MAX·embedded core의 기존 알려진 불법 입력. 길이/header 오류와 유효 count 변경을 구별 |
| `status_home_encoder_errors_preserve_output` | 모든 출력길이0..223와225, Free/count1/MAX 생성 거절, 입력 불변과 sentinel 보존; 정확224 정상 |
| `status_home_count_delta_is_checked_and_atomic` | old0/1/MAX−1/MAX × removed0/1/2/MAX × added0/1/2/MAX=64개를 u128 산술 oracle와 대조. removed>old 또는 결과>MAX만 거절 |
| `status_home_page_validates_all_slots_and_namespace` |0/1/70/71 slots 정상; old192혼합·마지막home 손상·마지막 다른DB/timeline·중복TxnId(동일/다른generation) 거절; namespace0/다른DB·오래된epoch·kind/level/page identity 손상 |
| `status_home_lookup_rejects_stale_aliases` | 정상 최초/마지막 slot, Free/slot71/slot81/실제누락, 다른file/page/incarnation/generation/TxnId/DB/timeline 거절. expected 값을 실제 cell에서 복사해 자기비교하지 않음 |
| `status_home_generated_profiles_match_independent_fields` | seed0..63 × sample0..127=8192, Free/Active/Aborting Statement/Aborting Transaction/Committed/Aborted의6상태를 순환하고 nonzero ID/generation/q 및 count 경계를 생성. core/head/cursor의 유효 조건은 입력식에서 정하며 raw offset로 별도 검산 |

신규7시험을 같은 보존 engine binary에서 실행하고 실제 이름·summary·source/binary/log hash를 고정한다. native fault는 이 무효과 codec에 N/A이며 실제 persistence 통합 때 해당 I/O·thread·복구 의무가 다시 적용된다. 현재-build 하위 회귀는 통합 지문에서 별도 실행하고 DEV3 전체 인수로 승격하지 않는다.

### 고정 생성식과 embedded-core 부정 목록

생성식은 RNG 호출 없이 `n=128×seed+sample`, state=`(seed+sample)%6`의 Free0/Active1/AbortingStatement2/AbortingTransaction3/Committed4/Aborted5다. DB는bytes1..16, timeline17, epoch=`18+seed%3`, local=`19+n`, generation=`20+seed`, committed q=`21+sample`, count는 occupied에서 `[0,1,MAX−1,MAX][n%4]`, Free는0이다. 이 식의8,192개 전체를 실행하며 sample을 무작위 일부로 뽑지 않는다.

occupied head는 sample이 홀수일 때만 H다. H/N/T는 모두 FileId(2,1)/incarnation4/slot0/generation20이며 page는 각각31/30/29다. Statement rollback에서 head가 없으면 target/next도None이다. head가 있으면 target은 `sample%4==3`일 때T, 그 밖None이며 next는 `sample%8==7`이면T, 그 밖N이다. Transaction rollback은 targetNone, head가 있고 `sample%4==1`일 때만 nextN이다. Active/Committed/Aborted는 target/nextNone이다. Free는 TxnId/q/head/target/next 전부zero다. core 의미 비교는 이 별도 입력식의 기대 state/identity와 byte offsets를 사용한다.

embedded-core 부정 입력은3 literal의224-byte 원본을 매번 복사하여 다음 단일 변형을 적용한다(표의offset은 home32를 제외한 core 상대값).

| base | 정확한 변형 |
| --- | --- |
| Active | offset0 magic byte를0, version4를0 또는2, state6=5, rollback7=3, DB8..24 zero, timeline24=0, epoch32=0, reserved36=1, local40=0, generation48=0, q56=1, flags64=8, reserved65=1, absent head72=1, absent target112=1, absent next152=1 |
| Committed | q56=0, rollback7=1 |
| Free | DB8=1, q56=1, head72=1, rollback7=1 |

Active17·Committed2·Free4=23개며 version만 UnsupportedFormat, 나머지는 Corrupt다. 별도로 모든 header 오류/길이/Free count 조건을 원래 시험표대로 검산한다. checksum이 없는 count의 유효 변경은 거절 목록에 넣지 않는다.

독립 검토자는2026-10-21 원문 SHA-256 `028fed2728cbf72fdef50dd697b739861e08c6fce578787ecedc409c3636468b`와 정확 corpus를 수용했다. 검토 원본은 `artifacts/dev3/obligation-resume-review-20261021/r1-status-home-review.md`다. 이 수용은 좁은 무효과 home codec/loader에만 적용하며 전체 obligation·terminal 실행은 승인하지 않는다.

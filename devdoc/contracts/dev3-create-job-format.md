# DEV3 실제 CREATE의 Job1024 v2

2026-10-25 · `D3-CATALOG-CREATE-001` 결합 D2 채택·구현 착수 수용 · [실제 소비 계약](dev3-catalog-create.md). 제품 효과·실행 인수 미완료.

2026-10-25 · r0 · `/root/page_lifetime` · **결합 D2 채택 정본**. 실제 CREATE 소비자와 함께 채택하며 별도 codec-only 완료 단위가 아니다. 제품/fixture/시험/commit 0. `dev3-create-wire-candidate.md` r2(`aea14557…1c4836`) 및 B1 commit `35c879e` 이후의 설계 요청을 따른다. 기존 Root/Job v1 decoder·literal은 엄격하게 보존한다.

## 1. 태그·고정 offset

모든 정수는 LE, 정확 길이1024다. page2..6/15+15+15+15+4 slot geometry와 JobSlotRef40은 현재 그대로다. v2 magic `XQJ1`, version2이고 checksum은 상위 canonical page/native SHA가 담당한다. 별도 cell CRC/가변 tail은 없다.

| offset | bytes | 의미 |
| ---: | ---: | --- |
|0|4|`XQJ1`|
|4|2|version=2|
|6|1|state: 아래 kind별 exact tag|
|7|1|kind: Free0 / Terminal1 / CatalogMissingRoot2|
|8|8|Job generation g, nonzero|
|16|48|source TxnRef|
|64|8|lease_lsn|
|72|8|adoption_lsn: Terminal의 Commit/AbortEnd, Root의 Forward|
|80|40|StatusSlotRef|
|120|1|**Terminal만** outcome: absent0 / Committed1 / Aborted2|
|121|1|**Releasing Root만** release_cursor0..3|
|122|6|reserved0|
|128|40|Root planned/actual UndoRef|
|168|8|Root expected UndoSequence|
|176|24|Root ObjectHandle: object:u64,generation:u64,schema_epoch:u64|
|200|24|Root table Anchor PageRef|
|224|24|Root RowRoot PageRef|
|248|8|Root planned/actual statement sequence|
|256|8|Root planned/actual mutation sequence|
|264|8|commit q: TerminalReadyCommitted / RootReleasingCommitted만|
|272|8|Root release_start_lsn: Prune 또는 CLR의 실제 LSN|
|280|4|planned Catalog **tree cell** bytes C (wrapper12 포함)|
|284|4|reserved0|
|288|32|planned present Catalog projection SHA256|
|320|8|reserved0|
|328|32|sender-before descriptor SHA256|
|360|16|reserved0|
|376|8|last_step_lsn|
|384|640|기존 RemainingBudget128×5|

state/kind의 허용 조합은 정확 다음뿐이다. `(state=0,kind=0)` Free, `(1,1)` TerminalLeased, `(2,1)` TerminalReady, `(1,2)` RootReserved, `(2,2)` RootRetained, `(3,2)` RootReleasingCommitted, `(4,2)` RootReleasingCompensated. 다른 조합은 UnsupportedState다. Terminal outcome은 Ready에서만1/2이며 Root state3/4의 disposition을 outcome byte에 중복 저장하지 않는다.

새 presence bitset은 없다. kind/state가 각 영역의 존재를 정한다. present SHA가 우연히 zero32인지로 부재/존재를 추정하지 않는다. absent SHA는 canonical zero32, present SHA는 안정화된 trusted source의 실제 SHA와 semantic verifier가 비교한다.

## 2. 상태별 canonical 내용

- **Free:** magic/version/generation만 남고 `[6,8)`, `[16,1024)` 전부0. 상태 없는 budget 행의 tag도0이다. 이는 occupied Job의 비활성 row tag1..5 유지와 다르다.
- **TerminalLeased:** source/status/lease/laststep와 Terminal budget만 유효. `laststep=lease>0`. adoption/outcome/q 및 모든 Root field, 두 SHA는0.
- **TerminalReady:** source/status/lease 보존, `adoption>lease`, `laststep=adoption`. outcome=Committed1이면 q>0, Aborted2이면 q0. sender SHA는 실제 Commit/AbortEnd sender-before Tx descriptor 전체1536 bytes의 SHA. Root 전용 `[128,264)`, release_start/C/planSHA는0. release_cursor0. Terminal budget은 유지된다.
- **RootReserved:** source/status/lease, U/seq/Object/pair/statement/mutation/C/planSHA 유효. `laststep=lease>0`, adoption/release_start/q/outcome/cursor/senderSHA0. 이는 Free Undo(g+1)의 **Tx inventory 예약**과 결합하는 planned identity이지 Live lookup이 아니다.
- **RootRetained:** Reserved의 immutable identity/plan을 보존하며 `adoption>lease`, `laststep=adoption`. senderSHA는 실제 Forward sender-before Tx descriptor SHA로 처음 설정한다. release_start/q/outcome/cursor0. pair는 정상 catalog object에 채택된 exact identity의 검산 record이며 Job이 소유한 page-return token이 아니다.
- **RootReleasingCommitted:** U/seq/Object/statement/mutation/source/status/lease/Forward adoption/senderSHA는 보존. `release_start>adoption`, q>0, release_cursor0..3. pair48, C, planSHA는0. cursor0이면 `laststep=release_start`, cursor1..3이면 `laststep>release_start`. q는 실제 source Committed(q) 및 Prune가 확인한 값이다.
- **RootReleasingCompensated:** 위와 같되 q0. release_start는 실제 CLR LSN. pair48/C/planSHA는0이며 CLR 같은 그룹에서 pair가 이미 Free로 돌아간다. cleanup은 pair를 다시 읽거나 반환하지 않는다.

C와 planSHA는 Reserved에서 exact planned Forward의 크기/내용과 debt를 묶고, Retained에서 Prune의 exact current Catalog를 확인하는 데 쓴다. Prune/CLR 이후에는 미래 cleanup이 Catalog/pair를 따라갈 이유가 없으므로0으로 지운다. Object/U/seq는 실제 root가 Free가 될 때까지 남겨 원 native identity를 검산한다. 이 선택은 Job cell/Native 길이와 이전 WAL 산술을 바꾸지 않는다.

첫 CREATE만의 고정값은 object≥6/generation1/schema_epoch1, statement1/mutation1, seq>0, C254..4600이다. C 범위 통과만으로 schema가 유효하다고 하지 않는다. actual TableSpec/XCT1/tree wrapper와 projection을 다시 검사하여 exact C/planSHA와 같아야 한다. projection은 r2의 `XCP1/version1/kindCatalog3/UndoTargetCore32/present1+zero7/XCT1 V bytes`다. 물리 slot/offset/LSN/count는 들어가지 않는다.

## 3. Local validation과 원 authority 검산의 분리

Local codec는 length/magic/version/tag/reserved/상태별 zero, existing TxnRef/StatusSlotRef/UndoRef/PageRef/ObjectHandle field codec, namespace timeline1, source.generation=status.generation, Job/source/slot generation≥2(non-Free), LSN 대소 및 budget 형태를 검사한다. 원 Tx writer epoch는 recovery current epoch와 달라도 된다.

Status와 U는 첫 data file `0x0000000100000001`, dynamic page9..63, slot0이다. 각각 actual incarnation은 typed nonzero이며 first plan4 genesis/reservation에서 실제 g≥2임을 allocator provenance가 확인한다. pair도 같은 file/dynamic range이고 A와R는 서로 다른 물리 주소이며 U/Status와도 alias하지 않는다. Object/gen/kind 허가는 raw PageRef에 있지 않다. RootReserved는 actor의 allocator Reserved pair+exact3 inventory, RootRetained는 Allocated(object/gen,TreeAnchor/RowLeaf)+actual tree 관계를 semantic context에서 검사한다.

`decode_job_v2`의 성공은 오직 borrowed view다. role/pin/claim/slot generation/allocator token/horizon 권한을 발급하지 않는다. encode는 full validation 후 caller output을 쓰고 오류에서 output을 보존한다. unknown version은 UnsupportedFormat, known version의 잘못된 tag는 UnsupportedState, reserved/비정규 body는 Corrupt, expected identity 불일치는 IdentityMismatch, 불가능한 LSN/cursor 전이는 OrderMismatch, debt 불일치는 BudgetMismatch를 유지한다. persisted corruption을 허가되지 않은 fallback이나 일반 CallerInput으로 낮추지 않는다.

모든 occupied Job에서 `lease ≤ laststep ≤ Jobs.pageLSN ≤ Root.apply`를 검사한다. **Status.pageLSN≥RootJob.lease/laststep를 강제하지 않는다.** Reserve는 Status를 바꾸지 않으며 release role도 Status를 바꾸지 않아서 정상 Status.pageLSN이 더 작다. Status의 actor/state/count/required predecessor와 실제 apply provenance를 별도로 검사한다.

senderSHA는 opaque 자유 입력이 아니다. TerminalReady는 해당 terminal group의 before Tx descriptor, RootRetained는 Forward before Tx descriptor와 정확히 같아야 한다. Root releasing의 senderSHA는 Forward provenance의 immutable 값으로 유지한다. 일반 release group은 없는 새 Tx before를 만들어 이 SHA를 다시 쓰지 않는다. 선택 base 이후에는 base의 검증된 previous Job bytes/provenance chain이 이를 보존하며 old WAL 없이 임의 현재 descriptor hash로 대체하지 않는다.

## 4. 실제 전이와 LSN/세대

| 실제 group | before→after | v2 Job/원 page 의미 |
| --- | --- | --- |
| Begin | Free(g)→TerminalLeased(g+1) | actual Status successor와 source; Root issued+1 |
| Reserve | Free(g)→RootReserved(g+1) | lease=LSN; U Free(gU+1), pair Reserved, Tx inventory3; Root issued+1/issuers 채택 |
| Forward | Reserved→Retained | adoption=laststep=LSN; 모든 planned 값 보존; pair INIT+Catalog/Undo/Status와 같은 XHG |
| Cancel | Reserved→Free(g) | pair Free/slot reservation release3; Root cancel+1; Root high-water 및 U/pair 세대 되감기0 |
| Commit/AbortEnd | TerminalLeased→TerminalReady | adoption=laststep=LSN; terminal outcome/q/senderSHA; RootJob를 재작성·재청구하지 않음 |
| committed Prune | Retained→ReleasingCommitted(c0) | release_start=laststep=LSN,q; pair/C/planSHA0; actual Catalog/status/U cuts와 같은 Aging group |
| CREATE CLR | Retained→ReleasingCompensated(c0) | release_start=laststep=LSN; pair/C/planSHA0; Catalog 제거+allocator pair Free+head 이동+U Releasing0 |
| ReleaseRole | Releasing(c)→same state(c+1), c<3 | only cursor/laststep/remaining advance; actual Undo cursor/tag 전이와 한 group |
| RootFree | Releasing(c3)→Free(g) | Undo Free(gU 유지)+JobFree+Root/Aging; Root completed+1 |
| TerminalFinish | TerminalReady→Free(g) | status terminal/headNone/incoming0/RootJobs0 + StatusFree; Root completed+1 |

g+1은 첫 effect 전 checked arithmetic으로 검사하며 MAX의 Free를 다시 lease하지 않는다. Free로 돌아갈 때 g를 유지한다. 같은 lease의 Reserved/Retained/Releasing 전이에서는 g가 변하지 않는다. Root issued/completed/cancel 보존식과 각 operation의 full before/after typed state 검산이 잘못된 “Free→Free 세대 증가”를 거절한다.

non-Free의 `laststep`은 해당 Job cell이 실제 변경되는 group LSN만 반영한다. 같은 Jobs page의 다른 cell 변경으로 pageLSN이 더 커져도 그 Job의 laststep을 덮지 않는다. JobSlotRef의 outer location/generation과 body generation도 같다. 원상 복구라는 이유로 lease/generation을 과거 값으로 되돌리지 않는다.

## 5. exact budget640 및 비용 불변

occupied Job의5 rows는 기존 `encode_budget_rows` 순서 tag1Clr/tag2Structural/tag3Terminal/tag4Status/tag5Control을 그대로 쓴다. byte0 tag,1..7zero,8steps,16wal,24control,32growth,40pages,48undoSlots,56statusSlots,64..112 ResourcePeak6,112protected,120..127zero다. 각 row 시작은384+128×i다. 이 첫 Job은 **i=3(Status)**만 활성이고 나머지 row는 tag 외 수치0이다. Free는 예외적으로 budget640 전체0.

| state | row3 steps | row3 wal=protected |
| --- | ---: | ---: |
| TerminalLeased / TerminalReady |1|55112|
| RootReserved / RootRetained |5|`311608+2C` = Prune(89496+2C)+4×55528|
| RootReleasing c0 |4|222112|
| RootReleasing c1 |3|166584|
| RootReleasing c2 |2|111056|
| RootReleasing c3 |1|55528|

control/growth/pages/undoSlots/statusSlots는0이다. page2+UndoSlot1은 RootJob의 자원이 아니라 **Tx Structural inventory**에 한 번만 청구한다. Job rows는 Aging descriptor의 실제 remaining 내역이므로 전역 합계에 두 번째로 더하지 않는다. 모든 non-Free Jobs의 additive 값은 checked sum, Peak는 componentwise max로 Root/Aging와 대조한다. RootFree는 마지막55528을 실제 소비하고, Cancel은 root debt 전체를 증명된 미사용으로 반환한다. CLR은 미사용 Prune(89496+2C)만 반환하고222112를 남긴다. Prune는 그 비용을 실제 소비한 뒤222112를 남긴다.

**ResourcePeak 정본은 `P_B=(9,425984,0,9,5,1)`이다.** 모든 active Status row는 이 trusted selected recipe와 equality를 검산한다. inactive row는0이며 old EMPTY_PEAK를 재사용하거나 caller 임의 Peak를 받지 않는다. 이는 결합 D2의 설계 예약이며 실제 caller/source F05 PASS가 아니다. source census와 독립 입력 동결 전 effect·시험은 미입장이다.

Job 폭1024가 유지되므로 r2의 Native(128+2×1024)=2176과 Reserve86640/Forward153840+C/CLR119984+C/Prune89496+2C/Release55528/Terminal55112는 바뀌지 않는다. C254 root debt312116, C4600 root debt320808이다.

## 6. 전체 census/실제 native의 최소 cross-check

Root masks는 Free=(0,0), leased=TerminalLeased 또는 RootReserved, ready=TerminalReady/RootRetained/두 Releasing으로 분류한다. total64, active Tx≤32, nonfree≤55. 기존 “모든 source/status가 unique”를 삭제하는 대신 아래 typed 관계로 바꾼다.

- actor마다 Terminal 정확1; 첫 independent CREATE 범위에서는 Root≤1. Terminal/Root가 같은 actor/status를 공유하는 것은 의도된 관계다. 서로 다른 actor의 같은 physical Status alias와 한 actor의 다른 Status는 거절한다.
- non-Free Root마다 exact U가 unique이며 첫 slot0/page 범위에서는 **physical Undo page도 unique**다. Terminal이 소비하는 unique Status page와 Root가 소비하는 unique Undo page가 서로 겹치지 않아 nonfree≤55 근거가 된다. Categorical count만 맞춘 중복은 거절한다.
- Reserved pair/Retained pair는 서로와 모든 U/Status 및 다른 유효 pair에 physical alias하면 안 된다. CLR/prune 이후 pair0은 재할당된 페이지를 오래된 RootJob과 오인하지 않게 한다. source ObjectId/UndoSequence의 중복·Root high-water 초과도 거절한다.
- RootReserved: actual TerminalLeased, source Active/headNone/incoming0, actual U Free(gU), 같은 Tx exact3 inventory와 Reserved pair를 검사한다. UndoSlot reservation이 있다고 U를 Live로 읽지 않는다.
- RootRetained: U Live/seq/creator/target Catalog5/gen1/entity=ObjectId/mutation1/incoming2, two Never/Missing/no payload; Catalog own-stamp/U/object/pair; source Active 또는 rollback nextU인 AbortingFull 또는 Committed(q)와 맞는 Terminal phase를 검사한다. Commit은 Job 미변경이므로 Retained 자체에 q가 없는 것이 정상이다.
- RootReleasingCommitted: actual Undo Releasing(cursor),incoming0 및 immutable identity; source Committed(q),headNone/incoming0, TerminalReadyCommitted(q). RootReleasingCompensated c0은 CLR 직후 AbortingFull(head/nextNone)+TerminalLeased가 가능하고 AbortEnd 뒤 Aborted+TerminalReadyAborted다. 첫 release-role 실제 실행은 terminal outcome까지 채택한 뒤 허용한다. c1..3에 source Active/Aborting을 허용하지 않는다.
- RootJob ref는 cleanup ownership이며 Undo incoming에 더하지 않는다. Creator도 별도 incoming edge가 아니다. 실제 incoming은 native fields/EdgeChange multiset/CountChange로 검사한다.
- Prune는 원 gate 아래 H=min(published Q,등록 snapshots), empty H=Q와 q≤H 및 transient U pins 조건을 확인한다. CLR는 자기 rollback transient U pin을 sealed retirement continuation으로 소비/이관하고 다른 U pins0/새lookup 등록 배제, pair Pending/flush quiescence를 확인한다. physical PinSet은 이 semantic pin 증명을 대신하지 않는다.

이 검산이 끝나도 local view는 권한이 아니다. actual original owner의 reservation/stage/apply/recovery consumer가 함께 있어야 사용한다. 독립 literal은 kind/state/reserved/LSN/cursor/세대, root↔Terminal↔Status↔U 교차 불일치, 같은 physical 주소/다른 incarnation alias, 잘못된 C/debt 및 absent 영역 nonzero를 각각 분리해야 한다. 제품·fixture·시험은 전체 B2 D2 후에만 시작한다.

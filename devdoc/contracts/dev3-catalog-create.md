# DEV3 원 저장 세션의 첫 Catalog CREATE

2026-10-25 · `D3-CATALOG-CREATE-001` · 통합 `/root`, 설계 `/root/undo_design`·`/root/page_lifetime`, 독립 검토 `/root/dev3_review` · **결합 D2 채택·구현 착수 수용, 제품 효과·실행 인수 미완료**.

이 단위의 완료 조건은 원 plan4 저장 세션에서 작은 Catalog CREATE를 독립 거래로 Commit하고, 같은 ELF의 새 process가 그 table identity를 읽는 것이다. 후속 별도 Row INSERT 거래가 이 handle을 실제 소비한다. 원 owner/pool/WAL/fence를 유지한다. [공통 Commit 수명](dev3-commit-continuation.md)은 이미 한정 공급되었지만 actual semantic receipt 생성자는 닫혀 있다. 아래 실제 의미·권한·복구·자원 공급 없이 생성자를 열지 않는다.

## 지원 범위와 전이

첫 extent64, index0, 충분한 Catalog leaf 여유, 작은 table schema, Missing Undo 한 root와 독립 CREATE 한 문장을 지원한다. 다중 root·일반 Row/UPDATE/overflow/SMO를 이 단위에서 완료했다고 하지 않는다. 일반 DML 거래 중 DDL, 범위 밖 크기/역할은 첫 effect 전에 typed Unsupported/NoSpace로 거절한다. implicit commit은 없다. 기본 owned8GiB/protected1GiB·buffer4/16/64MiB·metadata8MiB·maintenance12MiB 등 총량은 유지한다.

원 fresh plan4가 v2 Root/Job을 설치하고 scope2 AgingStart16으로 원 Aging을 시작한다. 기존 scope1의 실제 Status/Free Undo container를 발급하고, v2 Begin이 실제 Status slot/actor와 Terminal Job을 함께 lease한다. Reserve→Forward→Commit 또는 Reserve→Cancel, Forward→RollbackStart→CLR→AbortEnd를 같은 원 owner에서 연결한다. lifecycle별 복구와 재장애까지 닫힌 뒤에만 지원 경로로 판정한다.

- Reserve는 두 page token·UndoSlot 하나·RootJob 하나와 앞으로의 Tx/Aging debt를 실제로 확보한다. ObjectId·UndoSequence·Undo slot generation을 checked successor로 채택한다. Cancel은 high-water를 되감지 않는다.
- Forward는 allocator pair의 Reserved→Allocated, Anchor/RowRoot INIT 두 개, Catalog actor stamp/undoU, Status headU/incoming1, Undo Live/incoming2, RootJob Retained를 같은 Tx+Aging 그룹으로 채택한다. 변경8page와 CatalogAnchor proof를 포함해 원 pin9개다.
- Commit은 Root C의 checked successor q, Status Committed(q), TerminalReady와 Tx 종료/credit 인계를 하나의 actual terminal group으로 준비한다. RootJob과 Live Undo는 그대로 보존한다. stage→durable→Drain→whole installation→작은 Q 공개→ACK 순서다.
- CLR은 실제 rollback current Undo pin을 소비하고 원 pair의 Pending DWB/flush/init/handle 조건을 해결한 뒤 Catalog 제거·pair Free·head/nextNone·native counts0·Undo Releasing0·RootJob Compensated를 함께 설치한다. Busy이면 효과 전 원 head와 pin을 보존한다.
- committed Prune는 H=min(publishedQ,등록q), q≤H, 실제 pin/queue/edge 조건을 만족할 때 Catalog Committed(q)/undoNone, Status headNone/incoming0, U Releasing0를 함께 채택한다. empty snapshot registry도 H=Q다.
- 세 ReleaseRole은 absent role도 각각 cursor0→1→2→3으로 처리한다. RootFree는 그 뒤에만 Undo/RootJob을 Free로 만든다. TerminalFinish는 해당 actor의 RootJob0, headNone/incoming0, actual transient Txn pin0 등 원 조건을 모두 검사한다. 즉시 Live→Free나 suffix worklist는 없다.

RootJob은 정확한 U 하나의 cleanup owner다. 이 참조를 native incoming count에 더하지 않으며 creator identity도 숨은 영구 status edge로 쓰지 않는다. Terminal이 RootJobs0 전에 해제되지 않는 실제 census로 source Status 수명을 보호한다.

## 정확 wire와 credit

[Root512 v2](dev3-create-root-format.md), [Job1024 v2](dev3-create-job-format.md)가 exact tag/offset·reserved zero·census·issuer를 정한다. [복구 base](dev3-create-recovery-base.md)는 XRB1 v3 2MiB와 Profile 전용 v2, actual image/provenance/Reserved inventory를 정한다. old pure v1/v2 decoder와 원 증거는 보존한다. current runtime의 형식 전환은 명시적이며 오래된 bytes를 새 source의 PASS로 승격하지 않는다.

scope2의 Tx+Aging XHG는 기존 header256+descriptor2×1536+OperationCore256을 사용한다. Tx 소유 frame 전체 WAL은 Tx에서 한 번 consume하고, Aging chain도 진전하되 같은 bytes를 다시 consume하지 않는다. Aging-only XSG는 Aging에서 한 번 consume한다. Root의 Aging consumed 누계가 실제 descriptor와 같아야 하며 old completed×55112 식은 v2에 적용하지 않는다. [scope2 group 표현](dev3-create-group-format.md)의 exact field grammar·operation tag·native/edge/inventory 순서를 producer와 observer가 함께 검산한다.

C는 Catalog tree cell bytes(기존 wrapper12 포함), 첫 최소 T(i Int64)는 C254, 최대 C4600이다. P=16344는 full Patch, I=16448는 full INIT다.

| group | native bytes | edge/count/inventory 수 | page redo | exact WAL bytes |
| --- | ---: | --- | --- | ---: |
| AgingStart |1152|0/0/0|1P|19672|
| Begin |3904|0/0/0|3P|56712|
| Reserve |16864|0/0/3|4P|86640|
| Forward |17904+C|3/2/3|6P+2I|153840+C|
| Cancel |15872|0/0/3|3P|69304|
| CLR |17568+C|3/2/0|6P|119984+C|
| Commit / AbortEnd |3904|0/0/0|3P|56712|
| committed Prune |5024+2C|3/2/0|5P|89496+2C|
| one ReleaseRole / RootFree |4320|0/0/0|3P|55528|
| TerminalFinish |3904|0/0/0|3P|55112|
| RollbackStart |576|0/0/0|64+224|3040|

Reserve 뒤 Tx remaining은 Forward+CLR+terminal+rollback=`333576+2C`, Aging은 Root cleanup+TerminalFinish=`366720+2C`이다. 서로 다른 owner의 두 branch를 global max로 합치지 않는다. current Reserve 포함=`786936+4C`, 이미 Begin에서 확보한114864를 제외한 증분 admission=`672072+4C`다. C254이면 증분673088, current+future787952B다. Job rows는 Aging remaining의 내역이며 전역 ledger는 ATTsum+Aging으로 한 번만 청구한다. 이미 실현된 WAL/control 및 다른 owner의 debt는 별도로 더한다.

Cancel은 Forward bound 안에서69304를 consume하고 그 잔여·unused CLR·Root cleanup 전체를 반환한다. Forward는 F와 inventory3을 consume한다. CLR은 L을 consume하고 unused Prune만 반환하며 ReleaseRole3+RootFree4step222112를 남긴다. Commit은56712를 consume하고 unused CLR/rollback을 반환하며 Aging cleanup을 유지한다. 모든 반환은 같은 adopted group에서 실제 before/after 보존식으로 검산한다.

## 원 소유와 물리 pair

`D3-MODULE-ADOPTION-001` **D2 ACCEPTED 결정**의 [Catalog claim 경계](dev3-module-boundaries.md#catalog-claim-gap)는 현재 `&mut SharedPool`에 기대는 배제를 원 pool의 기존 TxPair `init_claim`에서 명시한다. 두 clean frame의 content 대여와 단일 registry leaf 아래 기존 target identity·pin/load/flush/init 부재·DPT·generation successor를 재검산한 뒤 두 directory close와 claim 설치를 원자 수행한다. claim 동안 일반 신규 resident pin·load·capture는 Busy이며 같은 key를 가진 Reserve/Cancel의 기존 target 접근과 Forward의 원9-target만 제한적으로 허용한다. 장기 transaction snapshot을 해제하거나 별도 pool/gate owner를 만들지 않는다.

ReservedCatalogCreate는 원 MutationPort/Owner/statement/private와 MaintenancePort의 원 flush control을 가변 대여한 opaque lease를 disjoint 보유한다. no-capture 조건을 숫자 관측만으로 대신하지 않는다. finish/cancel은 같은 key가 배제를 유지한 상태에서 원 directory·claim·key를 함께 종료하고, 미완료 Drop·부분 효과는 원 fence/resolve로 닫는다. 실제 API와 정상/실패·Cancel/Drop·장기 reader 공존 인수는 위 경계 문서가 소유한다. 기존 source의 mutable-pool 인수는 새 공유 경계의 실행 증거가 아니다.

원 directory의 단일 closed_index와 single System InitClaim을 그대로 두 번 호출하지 않는다. 원 고정 directory 안에서 두 슬롯의 joint close/prepare/publish를 수행하고 같은 claim slot의 typed TxPair variant로 두 page를 함께 추적한다. 별도 pool·registry owner·PageToken 복제는 없다. allocator Reserved는 generation+claimant만 저장한다. 역할/object/pair는 RootJobReserved·Tx inventory·private semantic plan과 대조한다.

Reserve Cancel은 allocator pair·UndoSlot inventory·RootJob·closed directory 두 슬롯·원 TxPair claim을 모두 정리한 뒤 원 owner를 반환한다. CLR은 pair의 원 transient handle·init/flush claim·selected Pending DWB 조건을 먼저 닫는다. Catalog 제거와 allocator pair Free 뒤 scoped target guard를 해제하고, exclusive original bound와 sealed cleanup 아래 directory/DPT retirement를 마친 다음에만 새 allocation을 노출한다. partial install/retirement는 fence한다. 이미 Free인 pair를 RootJob cleanup이 다시 반환하지 않는다.

## 실제 Commit factory와 실패 분류

PreparedTerminal은 actual Active actor/status generation, 완료된 statement/head, Terminal lease, Root C+1, exact Root/Job census와 credit/ATT, whole targets/after bytes 및 immutable frame을 묶는다. receipt를 만드는 private bridge는 이 준비와 원 core/key 및 actual WalRequirement만 소비한다. raw LSN, parsed bytes, physical DurableWal 또는 N개 page bit mask로 semantic receipt를 만들지 않는다.

실제 stage call에 진입하기 **전** original stage continuation에 비가역 call marker/guard를 세운다. 원 WAL이 stage 전에 효과 없이 거절했음을 보장한 private rejection receipt만 정상 cancel을 다시 허용한다. 단순 상위 오류 code나 반환 실패만으로 이를 추정하지 않는다. complete stage 뒤 fallible callback 없이 StagedCommit이 결합되고, stage call 도중 unwind/부분 효과/불명은 먼저 fence·Abandoned를 보존한다. 첫 File write 여부는 B1 원 marker로 별도 관측한다.

authentic durable proof 뒤 B1 Drain을 사용한다. 모든 original page/DPT와 descriptor/ATT/terminal bank 설치를 끝낸 동일 producer만 InstalledCommit을 발급한다. page→version/admission→pin 반환 이후 실제 installed frontier receipt를 원 visibility controller가 한 번 소비하여 Q를 공개한다. status q>Q 구간과 snapshot은 아래 visibility 계약으로 처리하고 ACK는 Q 공개 뒤다.

## snapshot·참조 pin·조회

[원 visibility/retirement 계약](dev3-visibility-retirement.md)의 disjoint8192B metadata loan을 기존 original binding/core/fence에 붙인다. DEV2 ArcRoot나 metrics pin, physical page pin으로 의미를 대체하지 않는다. snapshot68행, reference66행은 첫 Missing 범위의 hard cap이며 일반 Row의3-pin traversal 지원으로 확대하지 않는다.

version→Q/H capture→ordered page latch 순서를 지키는 좁은 original target API를 공급한다. source Catalog latch 아래 TxnPin+UndoPin을 일괄 등록한 뒤 source latch 밖에서 실제 Status/Live Missing Undo를 읽는다. Active ATT만 찾는 현재 control::slot은 terminal 조회를 대신하지 않으므로 원 TerminalJob/persistent Status resolver를 함께 구현한다. raw TxnRef/UndoRef를 usable pin으로 바꾸는 공개 factory는 없다.

retirement는 같은 original version gate와 actual before targets 아래 pin census와 retiring marker를 원자 처리한다. CLR 자기 pin은 실제 linear token을 consume하며 actor 숫자로 count를 하나 빼지 않는다. 다른 pin이 있으면 원 token을 반환한다. pre-stage explicit cancel만 marker/자기 pin을 복원하고, stage 이후 실패는 fence와 marker를 보존한다. RootFree/TerminalFinish는 새 실제 census를 다시 한다.

## 자원과 실제 실행 입장

첫 B 활성 Tx CLR/Structural/Terminal/Status와 Aging Status row의 보수적 recipe는 `P_B=(frames9,memory425984,locks0,registry9,files5,doublewrite1)`다. inactive rows는0, 기존 System scope1은 기존 recipe를 유지한다. locks0은 이 좁은 DDL의 logical lock budget이며 내부 mutex가 없다는 뜻은 아니다. directory pair2는 pin9에 이미 포함되어 중복 청구하지 않는다.

425984B는 frame163840+after8page131072+before8page131072의 원 private loan이다. lookup의 Catalog copy 최대4588B는 private loan의 마지막1page `[409600,425984)`에 보존한다. 원 loader에는 겹치지 않는 prefix `[0,409600)`의 필요한 subloan만 빌려 주며 lookup 동안 tail을 재사용하지 않는다. private 동기 resolver가 원 Owner를 단계별 재대여하고 typed split으로 두 borrow를 분리한다. 여러 Tx의 동시 private 예약은 따로 합산하고 단일 Aging의 sequential cleanup scratch는 peak max로 집계한다. metadata8192와 retained bank/control, 원 WAL slot·buffer·DWB·replay는 다른 예산으로 실제 회계한다.

history/replay1572864B 안에서 arena1048576+Jobs65536+bank65536+frame163840+after131072+home16384+FileMap32768=1523712B, 잔여49152B다. 두2MiB base와1MiB physical arena를 분리하면 원 maintenance12MiB의 named end7274496B, 잔여5308416B다. data backing 재분할은 source-value component65536/metadata196608/whole262144 cap 증가가 아니다.

v2는 borrowed Root/Job view, budget streaming, prepare/targets/Commit 단계 분리로 실제 simultaneous values와 Result/return 중복을 줄인다. B1 native source peak65464B의 여유72B를 새 코드에 무검산 전용하지 않는다. exact static source/caller census·boundary−1/동일/+1·FD/queue/loan admission을 새 source에서 검증하기 전 effects/native 실행을 시작하지 않는다. 이 수치는 설계 예약이며 실제 F05 PASS가 아니다.

실행 전 새/변경 selector·독립 byte oracle·known-bad·compiler exact code/span·원 source/ELF·시간/출력/디스크 한도를 동결한다. 실제 CREATE success/cancel/CLR/Prune·snapshot gap·lower-flush/Drain·fault cuts·native 새 process/old WAL 제거·재장애를 선택한다. 단계 사이 완료 무변경 시험은 반복하지 않는다. 최종 DEV3 S5에서는 확정 전체 DEV3 inventory와 current 형식에 영향을 받는 runtime 입력을 함께 실행하고 old pure codec 증거를 보존한다.

## 검토와 남은 의무

독립 검토는 Root/Job 폭·frame/credit 산술, Job cleanup과 incoming edge 분리, H/own-pin 조건, original maintenance 재분할·2MiB profile 경계와 Reserved inventory의 원 checker 재사용을 source에 대조했다. 2026-10-25 독립 `/root/dev3_review`의 결합 검토를 거쳐 `/root`가 이 문서와 연결된5개 계약을 D2 채택했다. 위임 근거는 사용자의 DEV3 완수·병렬 진행 요청과 DP01이다. 비교 대안인 v1 owned 값 확대·별도 pool/WAL·압축 genesis 재생성 대신, 기존 owner의 borrowed v2와 exact-image v3를 선택했다. 기존 A/B1 결정은 해당 한정 범위에 보존한다. 실제 source F05, 독립 fixture/oracle 동결·제품 효과·native 결과는 미완료이며 해당 gate 전에는 grant/effects 입장을 열지 않는다. first-extent/한 CREATE 범위, 자원 한도 또는 owner 수명이 바뀌면 D2를 재검토한다. 문서 수용만으로 CREATE·Commit·복구·DEV3 완료를 표시하지 않는다.

<a id="sequential-create"></a>
## D3-CATALOG-CREATE-002 — 같은 첫 extent의 순차 CREATE

`D3-CATALOG-CREATE-001`의 D2 재검토 조건("first-extent/한 CREATE 범위")에 따라 범위를 바꾼다. "한 CREATE"는 동시에 진행 중인 CREATE가 하나라는 뜻으로 좁혀 읽는다. 001의 완료 조건(첫 CREATE Commit과 새 process 조회)은 바뀌지 않는다. 첫 CREATE가 Commit과 정리를 마친 뒤 같은 epoch·같은 원 owner/pool에서 두 번째 CREATE를 차례로 수행하는 것을 지원 범위에 포함한다. 채택된 메모리 inventory의 MT02와 transition corpus C11..C20(pair 13/14, 첫 64-page extent 안)이 이 경로를 이미 요구한다.

두 번째 CREATE의 Reserve는 보호된 7개 page 밖의 두 victim frame을 고르는데, 그때 victim은 앞선 CREATE의 dirty pair page일 수 있다. Reserve는 dirty victim을 효과 전 `Busy`로 거절하므로, `prepare_catalog_create_targets`가 같은 선택 규칙(`retention_pair_room`, start 1, 같은 7개 보호 집합, clean 우선·최소 recLSN dirty)으로 victim을 미리 판정하고 dirty victim에 대해 `FlushRequired`를 반환한다. 호출자는 원 flusher로 그 page를 정리한 뒤 다시 준비한다. 9 frame·7 보호이면 victim은 2개이고, 각 flush가 page를 정리하면 flush는 최대 2번이다.

이 판정은 권고이며 권한이 아니다. Reserve는 자기 후보 선택과 dirty 거절을 그대로 유지한다. pair identity가 생기기 전이므로 판정은 request 없이 victim만 고르며 아무것도 예약·초기화하지 않는다. victim이 없으면 오류(`Busy` 또는 `CounterExhausted`, Unchanged, phase Load)가 Begin 전 준비 단계에서 나온다. `Busy`는 후보 frame latch 경합에서도 나온다. 이전에는 Begin 뒤 Reserve에서 나왔다. 준비 단계는 호출자가 넘긴 Job page를 믿으며 Reserve의 첫 Catalog Job page와 같아야 한다.

frame 수, owner/pool, 자원 한도·source cap, wire 형식, allocator 범위(첫 extent)는 바뀌지 않는다. 서로 다른 CREATE의 동시 진행, 두 번째 extent, 다중 root는 여전히 범위 밖이다. 앞선 CREATE의 정리 완료 전 두 번째 CREATE와 epoch 경계를 넘는 순차 CREATE는 이 기록의 검증 근거가 아니다.

| 결정 항목 | 기록 |
| --- | --- |
| 상태·분류·일자 | ACCEPTED · D2 · 2026-10-26. 독립 검토(`artifacts/dev3/catalog-create-20261025/work-20261026/dev3-catalog-create-002-review.md`)의 수정 반영 뒤 통합 담당 확정 |
| 권한 | 사용자의 2026-10-26 결정(대화): MT02를 위해 같은 epoch 두 번째 CREATE의 pair frame 준비를 공급 |
| 변경 | `dirty_frame/retention_residency.rs`의 request 없는 victim 선택과 `retention_pair_room`, `plan4_retention/load.rs`의 준비 단계 판정 |
| 검증 | integration-r36에서 MT02 1회 PASS. r33에서 같은 시험이 Reserve17 Busy로 실패한 지점을 통과했으므로, 앞선 CREATE의 dirty pair page에 대한 FlushRequired와 flush 성공은 간접 확인이다(횟수를 직접 계수하지 않음). 첫 CREATE의 `None` 판정은 MT02 첫 주기에서 실행되었다. 완료된 사례의 `None` 판정은 "Reserve 성공이면 victim이 clean 또는 fresh"라는 정적 논증이며 재실행하지 않았다. r36에서 victim 없음과 recLSN 없는 Flush 후보는 정적 검토만 했다. 후속 [PR01–04](../evidence/dev3-catalog-review-fix-20261026.md)에서 둘째 victim 없음·generation 최대·latch Busy의 무효과와 recLSN 누락의 하위 invariant fence를 실행 검증했다. 상위 recLSN 방어 분기는 하위 불변식 때문에 도달 불가이며 실행 PASS로 표시하지 않는다 |
| 검토·보존 | 실행 전 검토 `artifacts/dev3/catalog-create-20261025/integration-r36/admission-evidence/dev3-catalog-remaining-r36-review.md`, 이 기록의 검토는 상태 행의 파일이다. 결과는 [실행 증거](../evidence/dev3-catalog-remaining-20261026.md)에 연결한다. r33 MT02 FAIL은 원 source의 결과로 보존한다 |
| 재검토 | 동시 CREATE, 두 번째 extent, 다중 root, frame 수·보호 집합 변경, Reserve 또는 판정의 victim 선택 규칙 변경, Job page를 내부에서 도출하도록 바꿀 때 |

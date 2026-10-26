# DEV3 실제 snapshot·참조 pin·retirement 경계

2026-10-25 · `D3-CATALOG-CREATE-001` 결합 D2 채택·구현 착수 수용 · [실제 소비 계약](dev3-catalog-create.md). 제품 효과·실행 인수 미완료.

2026-10-26의 [첫 Row Missing 참조 이동](#row-missing-traversal)은 별도 D2 ACCEPTED이며 기존 Catalog 실행 범위와 구분한다.

2026-10-25 · r0 · `/root/undo_design` · **결합 D2 채택 정본**. 제품/fixture 수정·시험 실행/commit 0. B1 공통 공급 `35c879e`의 실제 source를 읽었다. B1 PASS는 실제 Commit/publication/reader/aging 완료가 아니다. Root/Job wire는 root 및 다른 담당자의 정본을 따른다.

## 1. 결론과 실제 공급 경계

권고하는 다음 최소 공급은 **원 Plan4Retention의 original storage binding/fence/PageConcurrency에 묶인 bounded visibility bookkeeping + 실제 Catalog source에서만 발급하는 두 transient pin + 첫 Missing CLR/Prune 안의 retirement intent**다. 새 DB/session/pool/WAL/Tx owner를 만들지 않는다. 새 q나 UndoRef를 caller가 넣어 usable guard로 바꾸는 factory는 없다. 첫 실제 생성자는 B2의 actual installed Commit 및 recovered READY와 함께 닫는다.

확인된 원 symbol은 다음과 같다.

| 원 source | 실제 의미와 재사용 판단 |
| --- | --- |
| `publication.rs::Publication/CommittedRoot` | DEV2의 Arc catalog Root/position이며 DEV3 작은 frontier가 아니다. 재사용·이중 publication 금지 |
| `transaction/metrics.rs::TransactionCounters::pin/PinClock` | 32개 진단 시각 슬롯이다. q/target generation을 보존하지 않음 |
| `durable/retention.rs::Retention/Pin` | 파일명/backup/checkpoint/selection 보존이다. UndoRef/TxnRef 보호를 대신하지 않음 |
| `transaction/concurrency.rs::Owner::try_apply/ApplyAdmission::try_version` | 원 admission 및 version gate 그대로 사용. 새 snapshot도 실제 apply admission을 거친다 |
| `storage/dirty_frame.rs::PinSet/ReadTargets/WriteTargets` | 원 physical frame pin 및 content/version latch. semantic Undo slot의 transient pin과 별도 |
| `wal_io/plan4_retention.rs::Plan4Retention::split` | 같은 pool/storage를 Writer/Reader/Flusher에 대여. shared visibility 접근도 여기서 같은 origin으로 빌려 준다 |
| `plan4_retention/read/catalog.rs::RetentionCatalogGuard::lookup` | allocator+CatalogAnchor+leaf 3개의 실제 read latch에서 typed record를 빌린다. 현재 visibility/UndoRef pin을 발급하지 않음 |
| `plan4_retention/read.rs::RetentionReader::read` | 2-page physical read. Undo는 현재 canonical Free만 수용하며 Live reader는 미공급 |
| `plan4_retention/control.rs/history.rs/recovery.rs` | 실제 ATT 32/Aging/Allocation bank. history는 36×1664B를 비교하고 recovery는 64KiB bank를 복사한다. 빈 것처럼 보이는 bytes에 volatile registry를 몰래 넣으면 안 됨 |

`UndoRef`는 page incarnation/slot/generation 40B, `TxnRef`는 DB/timeline/writerEpoch/local/generation 48B 표현뿐이다. 두 타입의 `new/decode`는 pin·lookup·free 권한이 아니다.

## 2. 같은 owner에 붙이는 작은 공급과 자원

**권고:** 원 assembly의 metadata 예산에서 disjoint caller loan을 한 번 확보하고 Plan4Retention이 소비하여 전용 visibility 부분을 보유한다. `split`의 writer/reader는 같은 controller를 공유한다. 공개 생성 가능한 backing은 zeroed storage일 뿐이고, session-bound controller의 생성자는 original pool binding/fence/core를 검산하는 Plan4 conversion 안에만 둔다. recovered READY 전 usable reader는 없으며 volatile rows는 empty로 시작한다.

- inline `PageConcurrency` 배열 추가는 피한다. B1 native recovery source peak는 65464/65536B라 여유72B다. controller pointer/lock/Result만 추가해도 **새 실제 source census**가 필요하다. backing을 밖으로 옮겼다고 실제 retained metadata를 계산에서 빼지 않는다.
- 기존 control 첫 bank의 history 비교 뒤 여유는 `65536−36×1664=5632B`다. raw bank 전체를 복사/관찰하는 기존 경로도 있으므로 그 공간을 재사용하려면 별도 bank partition 변경이 필요하다. 첫 공급에서는 별도 disjoint loan이 더 작고 명확한 변경이다.
- 후보 hard cap은 **registry backing8192B**, inline bound controller128B 이하, frontier descriptor64B 이하, scalar/guard/Result source를 포함한 각 신규 leaf4096B 이하다. 제품 source 값으로 검산하기 전 이 숫자를 PASS라고 쓰지 않는다. 기존 source64KiB/metadata196608B/whole262144B 및 총 metadata8MiB 상계는 유지한다.
- user snapshot 소유32, handoff의 old/new를 위해 각2행(64행). checkpoint/backup용 각 old/new2행을 별도로 예약(4행), 총 snapshot68행. 첫 CREATE의 system snapshot factory는 미공급으로 닫고 row만 확보할 수 있다. 일반 user 슬롯을 system rollback에 빌려주지 않는다.
- 첫 producer의 transient 이동은 foreground32×2 + maintenance1×2 = **66행**. DedicatedFlusher는 semantic traversal을 하지 않는다. snapshot과 pin은 별도: 마지막 snapshot 해제만으로 reference rows를 지우지 않는다.
- 8192B의 설계상 상계: snapshot68×32=2176, reference66×80=5280, user admission32×16=512, 단일 retirement intent128, 고정 control96 →8192. 이들은 RAM bookkeeping cell 상계이며 새 disk wire 포맷이 아니다. scan은 최대68/66행, no Vec/hash map/target-count table/chain 전체 목록이다. alias 두 pin은 두 행이며 exact key census로 합산한다.
- first Missing에는 Catalog에서 TxnRef+UndoRef 두 개, rollback에서는 currentU+nextU 두 개로 충분하다(next=None인 첫 범위도 실제 토큰 경계를 유지). 일반 Row before에 TxnRef와 nextU를 함께 따라갈 때 currentU까지3개가 필요할 수 있으므로 **두 행을 전체 CRUD의 상계로 선언하지 않는다**. Row D2에서 실제 handoff graph/상계를 다시 닫는다.
- `SnapshotGuard`는 비복제 소유이고 cursor는 이를 borrow한다. raw q 복사로 guard를 만들거나 해제한 old q를 재등록하지 않는다. reference lease는 original `&mut Owner`를 내부에 보유하는 좁은 `CatalogLookup/UndoTraversal` operation이 소유한다. 같은 operation이 필요할 때 그 Owner를 원 pool에 재대여한다. 따라서 reference reservation의 긴 mutable borrow와 physical ReadTargets의 owner borrow가 충돌하지 않게 하고, 외부 caller의 owner 재사용도 막는다.

형식 후보는 `VisibilityLoan`, `BoundVisibility`, `SnapshotReservation`, `SnapshotGuard`, `ReferenceLease`, `UndoPin`, `TxnPin`, private `RetirementIntent`다. 마지막 네 타입은 decoded ref/q로 생성하지 않으며 모두 exact original binding·entry generation을 검산한다. 이 이름의 독립 서비스/trait framework를 먼저 만드는 계획은 아니다.

## 3. 실제 Q/H와 lock 경계

frontier는 original DB/timeline/open binding과 published **Q**만 가진 작은 descriptor다. fresh Q0은 실제 fresh owner proof, restart Q는 selected Root C와 suffix replay/loser cleanup을 마친 **READY proof**로만 초기화한다. status/page decoder가 Q를 발급하지 않는다. Commit의 publication은 actual durable Commit→whole page/control/ATT installation 검산→page/append/registry/version 해제 뒤, 원 commit coordinator에서 sealed installed receipt를 한 번 소비한다. future factory는 같은 원 receipt를 B1 installation 종료와 Q 공개에 정확히 인계해야 한다. raw C/q, DurableWal 단독 또는 별도 atomic.store API는 없다.

Snapshot 획득:

1. locks/pins0에서 user admission/registry 행과 reference capacity를 예약한다. 예약은 아직 사용 가능한 snapshot이 아니다.
2. original `Owner::try_apply()?.try_version()?`를 얻는다. Pending/Drain/Busy면 모든 임시 자원을 정리하고 caller가 원 진행 규약으로 재시도한다. lock/pin을 보유한 새 대기는 없다.
3. version gate 아래 publication lock에서 origin+Q를 복사하고 lock을 놓는다. **이 Q 읽기가 snapshot 선형화다.**
4. gate를 유지한 채 registry leaf lock에서 exact reserved row에 q를 등록한다. publication과 registry를 동시에 잡지 않는다. no allocation/I/O/callback. 등록 완료 뒤 registry→version/admission 순으로 해제하여 guard를 반환한다.
5. handoff는 old를 유지한 채 같은 gate에서 new 등록을 완료한 뒤 old만 놓는다. last Drop은 registry leaf만 사용하며 poison은 fence, ownership을 보수적으로 보존한다.

Prune horizon은 같은 version gate 아래 **page/tree/append/registry 전에** Q를 읽고 publication lock을 해제한 뒤 registry의 `H=min(Q, all registered q)`를 읽는다. empty는 H=Q다. 실제 source/target latch를 얻은 뒤 같은 gate가 이어지는 동안 target pin/edge/owner census를 다시 한다. 등록 해제 및 그 사이의 Q 증가로 처음 얻은 H가 더 보수적으로 되는 것은 안전하다. 새 snapshot 및 old-q handoff는 gate에 막힌다.

**필요한 가장 작은 dirty_frame 추출:** 현재 `PinnedTargets::try_write`는 내부에서 version을 얻고 바로 page latches를 얻는다. 이 API를 호출한 뒤 Q를 읽으면 정본 rank를 어긴다. original PinSet→version 획득→**sealed horizon capture**→기존 page latch 취득의 좁은 두 단계 공급이 필요하다. 후보 `PinnedTargets::try_retirement_write(&BoundVisibility)`는 내부에서 위 순서를 수행하고 `RetirementTargets { original WriteTargets, private Q/H observation }`를 반환한다. public callback/새 pool/별도 version lock을 추가하지 않는다. wrapper가 original gate를 소유하고 drop/install 순서는 기존 targets를 재사용한다. 일반 try_write는 그대로 둔다. root의 실제 target-set owner가 이 exact API를 채택/대체해야 한다.

StatusCommitted(q)에서 **q>Q이면 무조건 prune 금지**이고 q≤H도 단독 허가가 아니다. first Missing은 actual terminal status/head, Catalog stamp/undo, exact RootJob, incoming(U)=2/status=1, previous Never/Never/before Missing, rollback owner 없음과 실제 pin 조건을 함께 검사한다. Q10/status11/empty registry의 H는10이며 새 q10 reader의 Missing before는 보존한다.

## 4. 실제 Catalog source→pin→load 경로

`RetentionCatalogGuard::lookup`의 physical 의미는 유지한다. 첫 actual resolver가 guard 아래 record를 보고 private `capture_catalog_refs`를 호출한다. raw UndoRef를 받는 공개 pin 함수로 노출하지 않는다.

1. snapshot 및 두 reference slots를 먼저 확보하고 원 `prepare_catalog`→`read_catalog` 경로로 actual leaf를 읽는다.
2. source leaf의 content latch를 유지한 채 stored Tx stamp와 UndoRef를 exact namespace/object로 검산하고 registry 아래 **TxnPin+UndoPin 둘 다** 등록한다. 두 자리/세대/target retirement 검산을 모두 마친 뒤 일괄 publish하므로 부분 pin을 반환하지 않는다. source-latch 아래 buffer load/allocator admission은 없다.
3. 짧은 canonical candidate bytes를 이미 예약된 caller buffer로 복사한 뒤 physical Catalog guard를 release한다. 현재 `CatalogRecordView`는 guard를 borrow하므로 latch 밖으로 그냥 보내지 못한다. 첫 정의242B라도 일반 bound는 existing Catalog 최대4588B를 따르며 별도 private loan에서 charged한다.
4. TxnRef를 원 persistent status directory/actual StatusSlotRef로 resolve하여 원 pool/load 경로로 status를 읽고 generation을 검산한다. **현재 `control::slot`은 Active ATT만 찾으므로 terminal Commit의 조회 공급이 아니다.** 첫 scope에서는 RootJob/TerminalJob 및 실제 status home을 포함한 원 bounded directory resolver를 B2/B3에서 공급해야 한다. TxnPin이 TerminalFinish/free를 막으며 caller가 임의 StatusSlotRef를 제시하지 않는다.
5. status committed q≤snapshot q이면 copied latest Catalog를 선택한다. q>snapshot q, Active/Aborting/Aborted의 다른 Tx이면 captured UndoPin으로 Live Missing U를 load·검산하고 None을 반환한다. own-write 판정은 caller가 준 actor 숫자가 아니라 원 actual transaction context로 한다. creator는 새 status edge가 아니므로 Undo.creator로 해제된 status를 다시 조회하지 않는다.
6. target load는 source latch 밖이고, actual allocated identity/owner3/gen1, slot generation, Live 상태, UndoSequence, creator/target/object 및 Missing payload를 확인한다. 현 Free-only `RetentionReader::read`를 무조건 Live 허용으로 넓히지 않고 B2 nonempty canonical replay/load authority와 함께 연결한다.
7. 현재 Reader는 load/scratch/control 권한이 없다. original Writer의 mutable operation loan/prepare 경로를 resolver 단계 사이에 재대여하여 NotResident를 채운다. `CatalogLookup`가 가진 owner를 그 좁은 prepare 호출에 빌려 주고, reference pins만 유지하며 page/version/append/registry locks0에서 I/O한다. Reader에 두 번째 replay scratch/loader owner를 주지 않는다. 원 split의 공유 pool과 원 writer만 쓴다.
8. checked release에서 source/target content guards→physical pins→semantic reference pins를 정리한다. snapshot은 borrowed transaction/statement 소유로 남는다. pin 검산 실패·generation mismatch·필요 Live 부재는 정상 None으로 내리지 않는다.

## 5. atomic retirement intent와 CLR의 자기 pin

Registry에는 한 개의 bounded **retiring key marker**만 둔다. original version gate가 logical retire producer를 직렬화하므로 target-map이나 새 durable Job이 필요 없다. marker는 exact UndoRef 또는 TxnRef + scope generation + source-owner token에 묶인다.

**Prune:** 원 horizon capture와 actual whole write targets를 보유한 producer가 latest semantic Before를 검산한 뒤 registry에서 U의 전체 pin census0를 검사하고 같은 leaf critical section에서 retiring marker를 세운다. marker가 있는 target에 새 pin 등록은 Unchanged/Busy로 거절하며 source latch를 놓고 재준비해야 한다. registry lock을 놓은 다음 원 WAL stage/install을 수행한다. registry lock을 잡고 append/WAL/I/O를 호출하지 않는다.

**CLR:** rollback은 status.rollbackNext를 source status latch 아래 읽어 current U pin을 등록한 뒤 unlatch한다. producer는 `RollbackTraversal`의 **실제 current UndoPin을 consume/transfer**하여 intent를 만든다. registry critical section에서 exact row generation/origin/role/actor/currentRef가 맞고 U의 총 pin 수가 정확히 자기1인지 검사한다. 단순 `count−1` 또는 `ignore_owner=actor` API는 금지한다. 성공 시 자기 row를 intent 소유로 옮기고 새 pin을 차단한다. 다른 Catalog reader의 U pin 하나라도 남으면 token을 그대로 반환하고 WAL0/Aborting/head/pair bytes unchanged다. 첫 current.next=None이지만 후속 next가 있으면 **next pin을 먼저 등록한 뒤** current를 transfer하도록 동일 operation의 두 slots를 쓴다.

Intent가 허가하는 것은 exact same-gate latest source 제거/Live→Releasing0뿐이다. standalone `H, ref, incoming0` free token이 아니며 version gate 해제 뒤 재사용할 수 없다. 구현은 private `apply_prune/apply_clr` 안에서 original `RetirementTargets`를 계속 소유하는 scope로 제한한다. grant가 page guard를 immutable borrow한 채 install의 mutable borrow를 막는 API는 피한다: original targets를 소유한 wrapper가 local intent와 함께 stage/install을 수행하고 **둘을 밖으로 분리해 반환하지 않는다**. 소비 시 actual original target stamps/generations와 exact semantic group identity를 다시 대조한다.

- 정상 pre-stage 거절은 명시 cancel을 통해 marker를 없애고 CLR의 current pin을 같은 row/세대로 복원하여 rollback cursor로 반환한다. 취소 완료 전 해당 owner가 다음 traversal을 시작하지 못한다.
- stage 진입/부분 install/불명 I/O 이후에는 자동 restore/cancel을 하지 않는다. original error/fence와 marker/intent owner를 보존하고 RecoveryRequired다. Drop은 숨은 WAL/ABORT/free를 하지 않으며 완료 불명 intent를 fence 없이 지우지 않는다.
- 실제 complete apply에서 source edges와 U Live→Releasing0를 원자 설치한 뒤만 marker를 정리한다. 그때 모든 legitimate source는 U를 더 이상 가리키지 않고 Releasing은 새로운 source pin/edge 대상이 될 수 없다. 이전에 복사된 유효 ref는 반드시 이미 pin을 소유했으므로 census0와 모순이다. 이 조합이 zero-check 뒤 새 pin race를 닫는다.
- 후속 RootFree와 TerminalFinish는 각 actual gate에서 **다시** target transient0 및 exact queue/cursor/generation을 검사하고 별도 intent를 소비한다. Prune의 오래된 intent를 free까지 보관하지 않는다. Undo container의 physical pin/flush claim/DWB와 pair allocator 반환 증명은 page_lifetime 원 owner가 별도로 담당한다.
- Prune에서 U pin0이어도 TxnPin이 남을 수 있다. status stamp cut은 가능하나 actual TerminalFinish는 status incoming0/headNone/RootJobs0뿐 아니라 **TxnRef transient0**도 필수다. 두 target 종류를 하나의 숫자로 합치지 않는다.

## 6. 최소 구현 분해와 입력 경계

1. 위 원 binding/loan/slots/lock 순서와 narrow retirement target acquisition을 D2에서 동결한다. source census만으로 native source64KiB 안에 들어가지 않으면 API/반환값 liveness를 줄이고 원 상계를 올려서 통과시키지 않는다.
2. original core의 최소 identity/version 검산을 통해 bound publication/snapshot/reference reservation leaf를 공급한다. raw frontier constructor는 없으며 실제 initialization/publication/retirement authority factory는 B2/B3 semantic owner가 발급한다. fake q/Commit/LiveU로 runtime PASS를 만들지 않는다.
3. 실제 CREATE Commit→publication gap 및 restart READY를 연결하고 Catalog lookup의 status resolver/Live Missing read를 완성한다. 그 다음 actual Prune/CLR/RootFree/TerminalFinish가 intent를 소비한다. 원 replay/load/REBASE closure 없이는 resident mutation만으로 인수하지 않는다.

새 필수 입력(기존 완료 시험 반복 없음):

- actual status11/Q10/empty registry → H10/prune WAL0 → 새 snapshot10은 Missing; publish11 후 기존q10 guard가 남으면 H10, 마지막 해제 후H11. 숫자10/11은 독립 source fixture 또는 실제 선행 Commit prefix에서 공급한다.
- Q를 읽고 registry 등록 직전 pause: ager는 version gate에서 대기하고 미등록 q는 외부로 나가지 않는다. 기존 owner/순서의 실제 source cuts를 고정한다.
- Catalog source latch 아래 TxnPin+UndoPin 등록 직후 pause→unlatch→CLR/Prune 시도: other pin 때문에 retirement/WAL0, reader 실제 target load/결과 뒤 pin해제 후 진행. unlatch-before-pin mutant 검출.
- CLR self U pin1만 존재→intent로 move→pin별 duplicate consume/foreign generation/actor 거절. self1+Catalog1이면 Busy이며 self token/next/head/pair unchanged. next pin 선확보 중 오류가 current pin을 잃지 않음.
- pin0 검사와 marker 사이 interleaving이 없고, marker 설치 후 stale source pin 시도는 Busy. pre-stage cancel은 self pin 복원; stage 뒤 panic/error는 marker+fence 보존. RootFree/TerminalFinish에서 새 generation과 다른 target 종류 혼동 거절.
- snapshot32개 및 handoff overlap64행, refs66행, MAX entry generation, quota boundary, poison/drop/fence cleanup; original backing counters/probe로 실제 simultaneous peak와 원128/4096/64KiB 상계를 검산한다.
- compiler: snapshot/lookup이 원 binding보다 오래 생존 불가, active lookup 중 Owner 재대여 금지, source guard release 후 borrowed record로 pin 발급 불가, current pin double-transfer 금지, raw q/UndoRef→guard/intent factory 없음.

독립 expected는 Q/H min, 정확 row-key multiset, actual source edge counts/Jobs/credit를 정수 입력으로 계산한다. 제품 registry census/encoder 반환값을 expected로 복사하지 않는다. thread cut/timeout/반복 수와 compiler exact code/span은 구현 전 해당 F07에서 고정하며, 이 메모에서는 시험을 실행하지 않았다.

## 7. root가 채택할 최소 API shape

아래는 현재 제품 symbol이 아닌 후보 signature다. lifetime은 원 source의 `'a/'s/'c`에 맞춰 구현 때 전개하며, raw/public factory를 추가한다는 뜻이 아니다.

```rust
// actual Plan4 conversion만 생성. loan은 이후 caller에게 재대여하지 않는다.
fn bind_visibility(original: &OriginalPlan4Binding,
                   loan: VisibilityLoan,
                   ready: ReadyFrontier) -> Result<BoundVisibility>;

// lease 예약은 effect/gate 전에; register_current는 original version 아래
// publication Q 선택→registry 등록을 끝내고 gate를 놓은 뒤만 guard 반환.
fn reserve_snapshot(&self) -> Result<SnapshotReservation>;
fn register_current(&self, owner: &mut Owner,
                    reserved: SnapshotReservation)
    -> Result<SnapshotGuard, SnapshotAcquireFailure>;
fn publish_installed(&self, receipt: InstalledFrontier) -> Result<()>;

// source method는 guards를 빌린다. raw refs만 받는 외부 factory 없음.
fn capture_refs(&self /* RetentionCatalogGuard */,
                lease: &mut ReferenceLease,
                object: ObjectId) -> Result<CapturedCatalogRefs>;

// original pool이 version을 취득하고 Q/H를 sample한 뒤 page latches를 취득.
fn try_retirement_write(self /* PinnedTargets */,
                        visibility: &BoundVisibility)
    -> Result<RetirementTargets>;

// wrapper를 mutable borrow하고 intent만 원 install 방법을 호출한다.
fn begin_missing_prune(&mut self /* RetirementTargets */,
                      checked: VerifiedMissingPruneBefore)
    -> Result<RetirementIntent>;
fn begin_missing_clr(&mut self /* RetirementTargets */,
                    checked: VerifiedMissingClrBefore,
                    current: UndoPin)
    -> Result<RetirementIntent, RetirementReject<UndoPin>>;

// 원target을 통한 install/metadata/DPT/control 완료까지 intent가 소유.
fn cancel_unstaged(self /* RetirementIntent */)
    -> Result<Option<UndoPin>>;
fn finish_applied(self /* RetirementIntent */,
                  receipt: AppliedRetirement) -> Result<()>;
```

`ReadyFrontier/InstalledFrontier/Verified*Before/AppliedRetirement`의 constructor는 실제 READY/Commit/semantic producer에만 있다. wrapper가 target guards와 intent를 함께 소유해 borrow 문제를 닫으며, registry helper가 만든 숫자 receipt만으로 B1 `InstalledCommit`을 발급하지 않는다. factory 공개 범위/consumer별 exactreceipt는 root의 B2 terminal factory와 함께 동결한다.

첫 resolver는 caller에게 임의로 중간 state를 넘기는 framework 대신 **하나의 private 동기 함수**에서 original `&mut Owner`를 유지하고 source/read/load 단계마다 재대여할 수 있다. `CapturedCatalogRefs`의 pin은 registry만 borrow하고 raw Owner authority를 소유하지 않는다. 그 함수 바깥으로 pin을 반환하는 API는 없으며 CLR의 내부 traversal→apply는 actual private pin을 move한다. `&mut Owner`를 가진 struct 안에 그 field를 다시 borrow한 `ReadTargets/WriteTargets`를 저장하는 self-reference는 만들지 않는다. checked cancellation/error에서 guard를 먼저 소비한 뒤 같은 원 owner와 traversal lease를 돌려준다.

Catalog copy 최대4588B는 원 B private425984B loan의 tail `[409600,425984)`에 보존한다. 동기 resolver가 original Owner를 유지하며 load에는 disjoint prefix의 필요한 subloan만 제공한다. source latch를 놓은 뒤 loader가 copy를 덮지 못하도록 typed split으로 수명을 닫는다.

<a id="row-missing-traversal"></a>
## 8. 모듈 전환 — 첫 Row Missing의 참조 이동

2026-10-26 · `D3-ROW-REFERENCES-001` · **D2 ACCEPTED / M2 완료 / 제품·시험 NOT_RUN**. 작성·계약 담당 `/root`. [모듈 전환 계획](../plans/module-independence-plan.md#m2-handoff)의 A/B에 필요한 채택 계약이며, 앞선 Catalog 제품 인수를 변경한 기록이 아니다. [최종 독립 검토](../evidence/module-independence-survey-20261026.md#final-design-review)를 수신했으며 구현·실행은 미완료다. 적용 범위는 실제 CREATE가 만든 단일 RowLeaf의 신규 RowId·inline·Missing Undo INSERT와 이 행의 point-read/cursor, 그 거래의 statement/full rollback이다. occupied before·UPDATE/DELETE·overflow에는 적용하지 않는다.

### 필요한 pin과 발급 원천

기존 source당 두 reference 행을 유지한다. 앞 절의 일반 Row3-pin 경고를 첫 Missing 경로의 무조건적인 확대 선행으로 해석하지 않는다.

| 실제 source와 처리 순서 | 동시에 필요한 semantic pin | 발급·반환 규칙 |
| --- | --- | --- |
| RowLeaf의 행 header→Status 판정 | source가 가진 TxnRef와 UndoRef, 최대2 | 검증된 Row target의 content latch 아래 두 target을 먼저 검산하고 일괄 등록. row copy·pin 등록 완료 후 latch 해제. Status target은 exact pin/TxnRef로 읽음 |
| Status 판정→Missing Undo 판정 | 기존 TxnPin+UndoPin, 최대2 | Status를 읽는 동안 기존 두 pin을 보존. row가 snapshot에 보이지 않으면 같은 U의 Missing·row identity·creator/sequence를 검산. Missing에는 beforeTxn/previous-version 참조가 없으므로 추가 pin 발급 없음 |
| CLR의 currentU→previous-transaction H | currentU+nextU(H), 최대2 | 원 actor/status의 rollback authority를 유지. U latch 아래 H를 먼저 pin한 뒤 source 해제. CLR이 head/next·row/edge·U 상태를 원자 채택한 뒤 currentU 반환, H는 다음 원 current가 됨. H=None이면 두 번째 행은 미사용 |

row source 발급기는 실제 Row target guard의 메서드로만 접근한다. 입력은 exact RowId와 같은 원 reference lease이며 raw TxnRef/UndoRef를 받아 pin으로 바꾸는 외부 factory를 추가하지 않는다. guard는 expected table generation/schema·anchor/leaf owner·page incarnation·canonical row/header를 확인한 뒤 source capability를 생성한다. registration 전에 대상 중 하나라도 retirement marker와 충돌하면 둘 다 등록하지 않고 Busy를 반환한다. latch 해제 후 pin 등록을 시도하는 대안은 회수 경합 때문에 배제한다.

`Committed(q)` 행은 q로 snapshot을 판정할 수 있으나 undo가 있으면 필요한 참조 보호를 생략하지 않는다. own actor의 Active row도 같은 statement/view authority 검산을 거친다. 다른 actor의 Active/Aborting/Aborted 또는 snapshot보다 늦은 Commit은 해당 Missing을 따라 부재를 확인한다. 필요한 U가 없거나 owner·generation·row identity가 맞지 않으면 Corruption/RecoveryRequired이며 정상 NotFound가 아니다. status 설치와 Q 공개 사이에도 원 snapshot q를 유지한다.

읽기 결과가 caller buffer로 완전히 인계되거나 오류로 폐기된 뒤 physical guard→semantic pin→reference lease 순으로 정리한다. FlushRequired를 반환할 때 조회용 pins/lease는 해제하고 snapshot·statement token만 유지한다. 재시도는 같은 q에서 새 source capture로 시작한다. poison/fence에서는 기존 보수적 보존 정책을 적용하고 clean release 성공으로 위장하지 않는다.

CLR의 H pin 선취득 실패는 current pin·원 head/next·미소비 rollback 예약을 보존한다. stage 전 cancellation은 원 권한을 돌려주고, 완결 stage 이후 실패는 실제 effect/fence와 원 retained ownership을 인계한다. replay/새 process에는 transient pin을 영속 복원하지 않으며 durable source/edge/job census로 loser를 처리한 뒤에만 새 READY를 발급한다.

### 자원·미공급과 선택 인수

이번 범위의 registry는 기존 snapshot68·reference66·admission32·retirement1, **8192B** loan을 유지한다. 새 Row source guard/copy/Result의 실제 type/source 비용과 loader·cursor 중첩 peak는 별도 F05에 포함해야 하며 기존 Catalog census를 새 Row의 PASS로 사용하지 않는다. Row/Undo2pin은 physical target pin 수와 다른 항이다. reader/rollback의 원 Owner 및 pool을 재대여하는 private operation을 유지하고 두 번째 registry나 전체 chain 목록을 만들지 않는다.

일반 occupied Row before에서 currentU+beforeTxn+nextU의 세 pin이 필요한 경로는 미공급이다. 이를 지원할 때는 handoff graph와 lease/registry·metadata 회계를 별도 D2로 변경한다. 현재 두 행을 일반 CRUD 전체 상계로 승격하지 않는다. 참고 산식으로33 actor×3행을 그대로 채택하면 reference99행·총10832B이지만, 이 산식은 이번 공급 배정이나 실제 지원 한도가 아니다.

구현 시 신규 Row 경로에 한해 source capture 직후 reader/CLR 경합, Status commit-q>snapshot-q, current/next pin 선후와 Busy 반환 보존, exact target 변조·missing U, nonresident load/FlushRequired 재진입, caller output 실패 정리, generation 소진·poison·scope 밖 occupied before 거절을 선택한다. compiler 사례는 source guard를 놓고 source capability 사용, lease 이중 반환, foreign origin pin 사용을 거절해야 한다. Catalog가 같은 추출 API로 바뀌면 그 실제 diff에 영향받는 기존 사례만 선택한다. 독립 검토자는 두 handoff graph의 최대2와 적용 범위·retirement/cleanup·F05 연결을 확인한다.

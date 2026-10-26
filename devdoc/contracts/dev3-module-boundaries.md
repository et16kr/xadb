# DEV3 모듈 독립 개발을 위한 원 소유·소비 경계

2026-10-26 · `D3-MODULE-BOUNDARIES-001` · **D2 PROPOSED / M2 미완료 / 제품 변경·컴파일·시험 NOT_RUN**

[구조 전환 계획](../plans/module-independence-plan.md) · [M1 조사·기준선](../evidence/module-independence-survey-20261026.md) · [구현 계약](implementation-contracts.md) · [저장 정본](disk-storage-design.md) · [원 runtime/core](dev3-storage-session.md#private-runtime-core)

조사 source는 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`다. 조사와 이 문서 작성은 `/root/module_boundary_survey`, 계약·통합과 문서 등록은 `/root`가 담당한다. 이 문서는 현재 소유와 목표 경계를 구별한 설계 후보다. §4–6은 동결을 요청하는 구체 설계 제안이다. 아래 경로·타입·signature는 실제 구현 또는 D2 수용 증거가 아니며, 새 경로를 build inventory에 등록하지 않는다. 기존 source별 PASS·FAIL·UNKNOWN·NOT_RUN과 DEV2/DEV3 미완료 상태를 유지한다.

<a id="scope"></a>
## 1. 첫 전환 범위와 판단

첫 소비 후보는 **A: first Row INSERT**, **B: snapshot row point-read/cursor**다. 두 기능은 원 저장 세션에서 생성·복구된 같은 table/row와 실제 snapshot을 사용한다. 현재 `row_record`와 tree edit의 byte 기능만으로 live INSERT나 snapshot row 조회가 공급됐다고 판단하지 않는다. A/B의 최종 범위·작업 ID·필수 인수는 [DEV3 작업 계획](../plans/dev3-implementation-plan.md#remaining-work) 및 M2의 소비자 작업 명세와 함께 확정해야 한다. RowId·statement·Row Forward/CLR 의미는 소비자 담당의 별도 소유 설계, source/ref handoff는 [Missing Row의 visibility 후보](dev3-visibility-retirement.md#row-missing-traversal), statement/cursor 수명은 [tree·row의 F02 후보](dev3-task-06-tree-rows.md#module-statement-cursor)를 따른다. 이 문서는 그 의미를 중복 정의하지 않는다.

현재 단일 writer/File/slot/fence/pool 소유는 보존할 기반이다. 개선할 대상은 이 소유를 지키는 수단이 초기화 단계의 깊은 자손 모듈과 private 필드 접근에 묶인 부분, 물리 WAL이 Catalog Commit 의미를 수입하는 부분, 소비자 추가 때 반복되는 재노출·permit 등록이다. **원 owner의 책임 배치와 실제 제한 API를 바꾸고, 기존 Catalog가 그 API를 먼저 소비하게 한다.** 새 re-export facade나 파일 이동만으로 M4 공급 완료를 판정하지 않는다.

첫 범위에서 일반 extent 성장·SMO·overflow·index·checkpoint/backup·전체 DEV3 모듈화를 선행으로 추가하지 않는다. 그렇다고 첫 INSERT의 RowId 발급, statement 실패, undo/CLR, terminal, 현재-build 복구를 생략하지 않는다. single RowLeaf·inline2048·index0·여러 statement의 구체 범위와 명시적 거절은 [소비자 명세](dev3-module-consumers.md#profile)의 제안이며 D2 수용 전이다.

<a id="actual-ownership"></a>
## 2. 현재 상태 소유와 권한 인계

| 실제 symbol·source | 소유 상태 | 현재 인계와 생성 제한 |
| --- | --- | --- |
| [`WalRuntime`, `AppendState`](../../crates/engine/src/storage/wal_io.rs) | `Mutex<W>`, `Mutex<W::File>`, 원 mutable WAL slot·slot state·prefix, Condvar, first-write marker, SessionFence, Profile | `GenesisWalSession` 안에 하나. stage는 실제 slot copy 뒤 exact `WalRequirement`를 생성; `DurableWal`은 같은 원 requirement를 확인 |
| [`InstalledStorageCore`, `FreshStorageSession`](../../crates/engine/src/storage/wal_io/bootstrap.rs) | 같은 WalRuntime, immutable BootstrapPlan, reader claim | Fresh는 core만 보유. Status/Undo 역할은 원 plan·fence의 typed borrow이고 file/payload 권한이 아님 |
| [`DoublewriteStorageSession`](../../crates/engine/src/storage/wal_io/doublewrite_install.rs) | Fresh, 원 DWB workspace loan, InstallPlan | 실제 설치를 마친 Fresh를 소비. 다른 namespace/WAL을 생성하지 않음 |
| [`MutableStorageOwner`](../../crates/engine/src/storage/wal_io/mutable_storage.rs) | core, DWB plan, `Mutex<Option<&mut workspace>>`, 유일 binding CAS | `into_mutable_storage`는 기존 core/plan/loan 이동. `BindingClaim` 실패·Drop은 원 fence/ABANDONED를 유지 |
| [`Plan4Pages`](../../crates/engine/src/storage/wal_io/plan4_pages.rs) | Mutable borrow, 실제 SharedPool 하나, operation loan, consuming activation용 DWB loan | `bind_plan4_pages`는 원 AVAILABLE→CLAIMED→BOUND와 원 loan take를 사용. 새 pool/map/CAS owner를 만들지 않음 |
| [`Plan4Retention`](../../crates/engine/src/storage/wal_io/plan4_retention.rs) | 위 Plan4Pages, control bank, RuntimeHistory와 replay loan, flush control, visibility, recovery binding | `into_empty_retention`이 원 pool/loan을 이동. `split`이 writer/read/flusher에 disjoint borrow를 제공 |
| [`RuntimeHistory`](../../crates/engine/src/storage/wal_io/plan4_retention/history/runtime.rs) | 원 core/binding/fence/Condvar의 borrow, installed-source metadata, 원 replay loan의 checkout 상태 | 원 fresh/recovered 전환만 생성. source cut·replay checkout은 page/INIT 권한이 아님 |
| [`BoundVisibility`](../../crates/engine/src/storage/wal_io/plan4_retention/visibility.rs) | 원 core/binding/fence의 origin, 작은 published Q, 별도 caller registry loan | actual READY/installed receipt에서만 Q 초기화·공개. q·TxnRef·UndoRef decoder는 guard를 만들지 못함 |
| [`Plan4Recovery`, `SelectedEpochParts`, `Plan4RecoveryRuntime`](../../crates/engine/src/storage/wal_io/plan4_recovery.rs) | 선택·복원 중 원 W/File/slot/maintenance/flush, 선택 epoch·DWB·base·잔여 obligation | private EpochTransferSeal을 가진 parts를 Mutable에 한 번 이동. Fresh를 합성하지 않음. READY 전 `into_live`는 원 owner를 돌려줌 |

현재 `Plan4Retention::split`은 writer에 operation/control의 가변 borrow를, reader에 pool/history/visibility의 공유 borrow를, flusher에 원 flush control의 가변 borrow를 준다. 이 분할은 유지할 출발점이다. read actor가 storage 전체의 borrow를 가진 것은 writer/File 접근 권한을 기능 소비자에게 공개해야 한다는 뜻이 아니다.

현재 Commit은 [`PreparedCatalog`](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_live.rs)의 immutable private frame/after/before·core/binding/fence·digest에서 시작한다. [`CatalogCommitAttempt/Installed`](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_commit.rs), [`StagedCommit/DurableCommit/InstalledCommit`](../../crates/engine/src/storage/wal_io/commit_bridge.rs), [`PendingCommitTargets`](../../crates/engine/src/storage/dirty_frame/commit.rs)가 exact requirement와 원 PinSet을 연결한다. 임의 page mask·LSN·scalar durable 관측으로 해당 증거를 대체할 수 없다.

<a id="actual-dependencies"></a>
## 3. 실제 Rust 의존과 M1 분류

아래는 물리 파일 배치가 아닌 `mod`/`#[path]`의 논리 모듈 트리다. 같은 디렉터리의 파일이라도 자손은 조상의 private 상태에 접근할 수 있다.

```text
storage::wal_io
  ├─ commit_bridge
  └─ bootstrap
      └─ doublewrite_install
          ├─ allocation_reservation
          └─ mutable_storage
              ├─ plan4_recovery
              └─ plan4_pages
                  └─ retention
                      ├─ create_pair::{catalog_live,catalog_commit}
                      ├─ visibility
                      ├─ history
                      ├─ read
                      ├─ load
                      └─ flush
```

| 실제 의존·근거 symbol | 분류 | 보존하거나 바꿀 이유 |
| --- | --- | --- |
| `WalRequirement` origin/exact, `DurableWal::confirms`, 원 fence | 필수 의미 | WAL-before-page와 same-session receipt. scalar 복사로 권한을 재구성하지 않음 |
| `dirty_frame::WriteTargets`, `CommitStageTargets`→`PendingCommitTargets`→`CommitInstallTargets` | 필수 의미 | 원 physical PinSet·generation·content digest와 DPT를 유지. Pending 동안 latch/admission0, durable 뒤 Drain |
| `RetentionReader`·writer·flusher가 같은 pool/history를 borrow | 필수 의미 | 원 상태·원 유지보수 loan·서로 다른 mutation 권한을 공유. 개발 파일 분리는 runtime 복제를 요구하지 않음 |
| `CatalogSource::from_guard`, source latch 아래 TxnPin+UndoPin 발급 | 필수 의미 | bytes/ref decoder와 사용 권한을 구별. 기존 [visibility 계약](dev3-visibility-retirement.md)의 capture·handoff 순서 유지 |
| `SelectedEpochParts`, `RecoveryRetentionBinding`, `SelectedRecoveryDwb` | 필수 의미 | 실제 선택·잔여 credit·burned issuance를 소비 이동. Fresh/clean 상태로 되돌리지 않음 |
| `wal_io.rs`→bootstrap→doublewrite→mutable→pages의 `CatalogRetirementRejected`, `TxPair*`, visibility 재노출 | 단순 등록 | 동일 타입이 여러 초기화 파일을 관통. 기능별 API의 실제 소유 경로에서 한 번 노출하도록 변경 후보 |
| `RetentionReader::lookup_catalog_empty` 및 `RetentionCatalogGuard`가 `storage.core.plan`/genesis helper 접근 | 불필요한 내부 접근 | 필요한 것은 정확한 namespace·fixed target 기대값·membership. 전체 BootstrapPlan 접근을 좁힐 수 있음 |
| `RuntimeHistory`·catalog_live·visibility binding의 `storage.binding`, `wal().completion` 접근 | 불필요한 내부 접근 | origin 동등성과 fence+notify는 필수. AtomicU8/Condvar 자체를 소비자에게 넘길 필요는 없음 |
| `commit_bridge`가 bootstrap의 `CatalogCommitAttempt/Installed`, `semantic_catalog::Operation::Commit` 수입 | 공통 계약 미완료 | 물리 WAL→Catalog 의미 역의존. semantic receipt 봉인은 유지하고 실제 terminal 조립 책임으로 이동 |
| `dirty_allocation::InstallPermit::from_retention/from_empty_transaction`와 dirty_frame의 wal_io permit 수입 | 공통 계약 미완료 | 실제 semantic stage proof는 필요하지만 generic installer의 권한 타입이 allocation 구현에 묶임. Row 추가 시 공통 등록 반복을 만드는 경계 |
| `dirty_init::tx_pair`의 Catalog 9-target·role·pair 검사 | 현재 제한 범위의 필수 의미 | CREATE가 함께 INIT하는 pair에 관한 공급. 기존 RowLeaf를 바꾸는 first INSERT가 이 claim을 복제할 근거는 없음 |
| `CatalogLookupLoan`, `UndoShape::CatalogRoot`, `CatalogSource`를 일반 Row에 적용 | 공통 계약 미완료 | 현재 Missing Catalog·2-reference 범위. Row traversal의 source/own-write/previous-chain/capacity는 별도 동결 필요 |
| source census·borrow/compiler fixture의 논리 모듈 경로 참조 | 단순 등록 또는 계약 증거 | 경로 적응은 필요하지만 기존 source의 완료 시험을 다시 실행할 이유가 아님. 실제 visibility/lifetime 변경의 선택 시험은 별도 |

`RowRecordView`, `UndoTargetKind::Row`, `LeafEditPlan`이 존재하는 사실은 Row stage·whole install·transaction chain·recovery 공급을 뜻하지 않는다. 순수 codec과 효과 owner의 의존은 실제 Row 공급이 확인되기 전 판단을 보류한다.

<a id="target-imports"></a>
## 4. 목표 책임·허용 import 그래프 제안

다음은 M2에서 동결할 **구체 제안**이며 D2 수용 결과가 아니다. 첫 이행은 legacy scope1의 초기화·복구 전체를 sibling으로 옮기지 않는다. `WalRuntime`, `InstalledStorageCore`, `MutableStorageOwner`, `Plan4Pages`의 원 소유와 초기화 조상은 유지한다. 대신 물리 WAL의 actual method, 정상 live owner·apply 조립, Catalog의 제품 본문을 서로 다른 책임으로 실제 이전한다. 깊은 legacy 내부 관계는 잔여로 명시하며 이 전환으로 저장 모듈 전체가 독립됐다고 주장하지 않는다.

```text
storage
  ├─ disk                         canonical 타입/API 수신 경로
  ├─ catalog::{create,read}        기존 Catalog 제품 본문
  ├─ row::{insert,read}            A/B 제품 본문
  ├─ semantic_catalog             기존 pure observer/materializer
  ├─ semantic_row                 새 pure observer/materializer
  ├─ dirty_frame                  기존 단일 pool/pin/DPT/installer
  └─ wal_io                       원 private WalRuntime/AppendState 소유
      ├─ physical                 Catalog/Row import가 없는 실제 WAL 구현
      ├─ disk_api                 선택 API/type 재노출 한 곳
      └─ bootstrap
          └─ doublewrite_install
              └─ mutable_storage
                  ├─ plan4_recovery   원 selected-epoch/READY 소유 유지
                  └─ plan4_pages     원 단일 SharedPool의 consuming owner
                      └─ apply       실제 DiskSession/ports/grant/receipt 정의
                          ├─ state   원 control/history/visibility/flush
                          ├─ read    실제 source capture/load/resolve
                          ├─ catalog 실제 Catalog before/target/stage 검산
                          ├─ row     실제 Row before/target/stage 검산
                          ├─ terminal 실제 stage/wait/whole install/Q 조립
                          └─ legacy  기존 sealed permit의 한시 consuming 변환
```

`apply`는 C만 수정하는 trusted 하위 트리다. A/B와 Catalog 제품 모듈은 그 자손이 아니다. planned file은 `storage/wal_io/physical.rs`, `storage/wal_io/disk_api.rs`, `storage/wal_io/plan4_apply.rs` 및 `plan4_apply/{state,read,catalog,row,terminal,legacy}*.rs`, `storage/disk.rs`, `storage/catalog/{create,read}.rs`, `storage/row/{insert,read}.rs`다. 논리 `plan4_pages::apply`는 `#[path = "plan4_apply.rs"]`로 선언한다. 새 물리 경로는 구현 때만 build inventory에 등록한다.

허용 호출 방향은 `catalog/row → disk API → apply actual adapter → original dirty_frame/physical`이다. `apply → semantic_catalog/semantic_row`는 pure 검산 호출이다. generic installer와 concurrency continuation은 `disk::{InstallGrant,StagedCommit,DurableCommit,InstalledCommit,UnstagedCommit}`의 제한 관찰 method만 수입한다. 이 proof type 수입은 callback·factory 실행 권한이 아니며 `dirty_frame → catalog/row` 호출을 만들지 않는다. physical WAL은 `WalFrameHeader/WalPrefix`, original I/O, exact requirement/durability만 다루고 Catalog/Row operation·attempt·installed type을 수입하지 않는다.

### 4.1 Rust visibility와 seal의 실제 위치

`pub(in sibling)`은 Rust에서 허용되지 않는다. 따라서 private raw factory를 `pub(in crate::storage::apply)`처럼 unrelated sibling으로 제한하는 안은 사용하지 않는다.

| 정의·경계 | 가시성과 실제 제한 |
| --- | --- |
| `wal_io::WalRuntime/AppendState`의 W/File/slot/fence/Condvar | 정의·필드 private 유지. `physical`은 자손이므로 원 필드를 사용. physical method는 `pub(super)`로 wal_io 내부만 호출; 일반 storage sibling에는 runtime getter 없음 |
| `bootstrap/.../plan4_pages::apply`까지 경로 모듈 | 필요한 모듈 이름만 `pub(in crate::storage::wal_io)`로 열어 `wal_io::disk_api`가 선택 타입을 직접 재노출. 필드·raw factory의 visibility는 넓히지 않음 |
| `disk_api`와 `storage::disk` | `disk_api`는 `pub(in crate::storage)`, `disk`는 crate 내부 API. API type/checked method만 선택 재노출. 초기화 파일 여섯 곳의 같은 `pub use`를 신규 소비자 통로로 사용하지 않음 |
| `apply::DiskSession/ReadPort/MutationPort/MaintenancePort` | 타입 `pub(crate)`, 일반 method `pub(in crate::storage)`, 필드 private. raw backend·pool/control/runtime 반환 method 없음 |
| `apply::InstallGrant`, terminal `Binding`과 receipt | 타입이 필요한 곳만 `pub(crate)`, 필드 private, raw `new/from_parts` 없음. actual validation/stage/install method는 apply의 자손에 둔다. apply root/자손만 struct literal을 만들 수 있음 |
| `apply::{catalog,row,read,terminal,legacy}` | module private. 이 안의 실제 producer/validator가 원 private owner를 검산하여 proof를 생성. 외부는 검산 method만 호출하고 callback으로 검사 구간을 대체하지 않음 |
| `apply::state`의 기존 control/history/visibility 구현 | 다른 apply 자손에서 필요한 기존 필드/method만 `pub(in crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::apply)`로 제한. 이 경로는 실제 조상이다. 전체 storage field 공개나 소비자 getter를 추가하지 않음 |
| `SelectedEpochParts/EpochTransferSeal`, `RecoveryRetentionBinding` | 원 `mutable_storage::plan4_recovery` 위치·seal 유지. `MutableStorageOwner::from_plan4_recovery`만 original parts 소비; `apply`는 raw parts를 받거나 재생성하지 않음 |

Rust privacy는 임의 코드를 trusted apply 자손에 추가하는 repository 변경 자체를 방지하는 보안 경계는 아니다. C 소유·source review와 실제 sibling consumer의 import 제한이 함께 계약이다. 단, 정상 소비자의 raw constructor 호출은 Rust privacy로 거절되어야 한다.

### 4.2 current → target의 실제 이동

| 현재 symbol·본문 | 목표 symbol·경로 | 없어져야 하는 외부 접근 |
| --- | --- | --- |
| `wal_io.rs::WalRuntime::{stage_verified_required,wait_durable,...}`의 물리 구현 | `wal_io::physical`의 같은 original runtime impl | physical의 `bootstrap::CatalogCommitAttempt/Installed`, `semantic_catalog::Operation` 수입0. runtime 정의 자체를 sibling으로 옮기기 위해 scope1 필드를 공개하지 않음 |
| `plan4_retention.rs::Plan4Retention`, `split`, `into_empty_retention` | `apply::DiskSession`, `split`, `Plan4Pages::into_disk_session` | 새 struct가 원 `Plan4Pages` 전체와 control/history/visibility/flush를 실제 소유. 단순 `type DiskSession=Plan4Retention`은 C1 완료가 아님 |
| `RetentionReader/Writer/Flusher` | `apply::{ReadPort,MutationPort,MaintenancePort}` | 새 제품 본문에 `storage`, `pool`, `control`, `history`, `visibility` field가 보이지 않음. port body는 원 borrow를 사용 |
| `read/lookup.rs::lookup_catalog`의 key 선택·version-visible·결과 분기 | `storage::catalog::read::lookup` | 제품 본문의 `storage.profile/core.plan`, `pool.bound_core`, registry·pin 직접 접근0. 실제 Catalog도 아래 source→capture→resolver API 수신 |
| `read/catalog.rs`, `lookup_load.rs`의 actual membership/load/source 생성 | `apply::read::{catalog,load}` | whole plan 대신 원 namespace·fixed target의 checked metadata 사용. `CatalogSource::from_guard`의 source 검산은 C 안에 유지 |
| `catalog_live.rs::prepare_catalog_commit`, `PreparedCatalog` | `MutationPort::prepare_catalog_commit`, `apply::PreparedCatalog` | 제품 본문은 actor와 private loan만 전달. 원 before/control cut/owner/target 검산과 frame materialization을 통과하지 않고 Prepared 생성 불가 |
| `catalog_commit.rs::RetentionWriter::commit_catalog`와 `commit_bridge.rs` | `MutationPort::commit_catalog`, `apply::terminal::{catalog,wal}`의 실제 조립 | 제품·물리 WAL에 raw stage, `Binding`, requirement 조합, whole install/Q factory 없음 |
| Catalog CREATE의 제품 순서·TableSpec 처리 | `storage::catalog::create`가 `disk::DiskSession::reserve_catalog_create` 및 opaque reservation의 checked `apply/cancel` 소비 | 예약 wrapper가 owner/claim/private loan을 보유하는 실제 C 부분만 `apply::catalog`에 남음. `TxPairClaimKey`·original INIT claim을 제품에서 조립하지 않음 |
| `dirty_allocation::InstallPermit`을 plan4 새 producer가 생성하던 부분 | `apply::{catalog,row}`가 `apply::InstallGrant` 실제 생성 | 새 Catalog/Row가 `from_retention`의 중앙 등록을 거쳐 옛 allocation factory를 계속 쓰면 C3 미완료 |
| `Plan4RecoveryRuntime::bind_retention`·`Plan4Recovered::into_live` | 원 위치에서 새 consuming `into_recovered_disk_session`/`DiskSession`을 수신 | 선택·epoch·base·READY owner는 그대로. fresh 합성, 원 W/File/slot/fence/loan 재생성 없음 |

`BootstrapPlan`, `InstallPlan`, scope1 physical installer와 기존 recovery 선택 코드를 한 번에 이동하지 않는다. 기존 plan4 정상 owner에 붙은 미전환 helper는 physical file을 그대로 두고 `plan4_apply.rs`의 `#[path = "plan4_retention/..."]` 자손 선언으로 원 owner를 계속 사용한다. 이것은 C1 owner 정의 이전에 필요한 내부 연결이며 독립 기능으로 승격한 것이 아니다. 현재 `retention` 재노출 이름은 이행 중 타입 alias/shim만 남길 수 있으나 신규 Catalog/A/B가 이를 수입하는 것은 금지한다. 미전환 helper의 명시적 잔여 목록은 C1 diff 기록에 남기며 source별 selector identity는 유지한다. 원 `Plan4Pages`는 Drop이 있으므로 필드를 억지로 destructure하거나 `ManuallyDrop/unsafe`로 옮기지 않고 새 `DiskSession.inner`에 **값 전체를 이동**한다. 기존 `_flush_workspace.take()`는 성공 전환에서 한 번만 수행한다.

<a id="owner-flow"></a>
## 5. 생성·정상·실패·재시작 소유 흐름

```text
Fresh 실제 설치 → DWB 실제 설치 ──┐
                                  ├→ 원 MutableStorageOwner
선택 recovery → original parts ───┘           │
   원 writer/File/slot/fence/loan 한 번 이동   ├→ 외부 PageConcurrency borrow
                                              └→ 원 Plan4Pages/SharedPool bind once
                                                      │ consuming 전환
                                                      ▼
                                               apply::DiskSession
                                         ┌────────────┼─────────────┐
                                      ReadPort    MutationPort   MaintenancePort
                                         │             │              │
                                Catalog/B 제품     Catalog/A 제품   원 flush/reclaim
```

session 안에 자신을 borrow하는 concurrency를 넣지 않는다. 외부 `PageConcurrency<'s>`와 `Owner<'c,'s>`가 원 session fence를 borrow하는 기존 수명을 유지한다. 정상 owner는 원 backing을 소유하는 것이 아니라 caller loan과 원 pool을 소유한다. split은 그 필드의 disjoint borrow이며 원 owner를 복제하지 않는다.

정상 변경은 whole target set→actual before/obligation 검산→private 후보→complete stage→whole install 순서다. Commit은 [원 continuation](dev3-commit-continuation.md)의 stage→durable→Drain→whole installation→작은 Q 공개→ACK를 유지한다. Pending에는 **같은 original PinSet**, immutable plan/loan과 exact requirement만 남는다. page/version/append/registry guard나 shared maintenance workspace를 보유한 채 sync를 기다리지 않는다.

stage 전 actual Unchanged receipt만 원 intent를 취소할 수 있다. complete stage 뒤 오류·partial install·unwind는 원 fence/Abandoned와 obligation을 유지한다. Drop이 sync/commit/재bind/loan 초기화/성공 Abort를 수행하지 않는다. 재시작은 원 selected binding에서 새 owner를 만들고 READY를 통과한 뒤만 foreground port를 제공한다. old open의 statement/cursor는 새 owner에 승계하지 않는다.

<a id="api-candidates"></a>
## 6. M2 동결용 signature·수명 제안

다음 signature의 타입은 구현 전 제안이다. `W: WriterNamespace`와 lifetime을 생략하여 C0에서 해결할 항목으로 남기지 않는다. `std::result::Result`를 `Result`로 표기하며 각 두 번째 인수가 실제 오류 타입이다. 기존 오류의 phase/cause/effect와 cleanup 우선순위는 유지하고 새 Row cause만 해당 owner 계약으로 추가한다. 내부에 쓰인 기존 타입은 현재 source의 실제 제네릭과 일치한다.

### 6.1 원 owner와 제한 port

| lifetime | 실제 대상 |
| --- | --- |
| `'m`, `'w` | 원 WAL `[u8; MAX_FRAME_BYTES]` slot, 원 DWB `[u8;4_194_304]` workspace |
| `'s`, `'c` | stable MutableStorageOwner/fence borrow, 외부 PageConcurrency borrow |
| `'p`, `'d` | original DirtyFrame 배열 loan, 각 frame의 page backing (`'d:'p`) |
| `'o`, `'a` | operation `[u8;114_688]`, retention control/replay/visibility의 caller loan |
| `'b` | 한 번의 `DiskSession::split`에서 원 필드를 대여하는 기간 |
| `'i` | private frame/before/after 또는 read loader/copy loan |
| `'u`, `'g` | operation이 보유한 원 `&mut Owner`, 한 source/load 단계의 짧은 재대여 |
| `'v`, `'r` | 실제 snapshot registry borrow, statement view를 사용하는 borrow |

```rust
pub(crate) struct DiskSession<'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    inner: Plan4Pages<'s,'c,'p,'d,'o,'m,'w,W>,
    control: RetentionControl<'a,'c,'s>,
    history: RuntimeHistory<'a,'c,'s>,
    flush: RetentionFlushControl<'c,'s,'w>,
    visibility: Option<BoundVisibility<'a,'c,'s>>,
    recovery_binding: Option<RecoveryRetentionBinding<'s,'s>>,
}
pub(crate) struct ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m,'w,W>,
    pool: &'b SharedPool<'c,'s,'p,'d>,
    history: &'b RuntimeHistory<'a,'c,'s>,
    visibility: Option<&'b BoundVisibility<'a,'c,'s>>,
}
pub(crate) struct MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m,'w,W>,
    pool: &'b SharedPool<'c,'s,'p,'d>,
    operation: &'b mut &'o mut [u8;114_688],
    control: &'b mut RetentionControl<'a,'c,'s>,
    history: &'b RuntimeHistory<'a,'c,'s>,
    visibility: Option<&'b BoundVisibility<'a,'c,'s>>,
}
pub(crate) struct MaintenancePort<'b,'s,'c,'p,'d,'m,'w,W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m,'w,W>,
    pool: &'b SharedPool<'c,'s,'p,'d>,
    flush: &'b mut RetentionFlushControl<'c,'s,'w>,
}
impl<'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    DiskSession<'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn split<'b>(&'b mut self) -> (
        MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>,
        ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>,
        MaintenancePort<'b,'s,'c,'p,'d,'m,'w,W>,
    );
}
impl<'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    Plan4Pages<'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn into_disk_session<'a>(
        self, owner: &mut Owner<'c,'s>, loan: EmptyRetentionControlLoan<'a>,
    ) -> Result<DiskSession<'a,'s,'c,'p,'d,'o,'m,'w,W>, IntoRetentionError<Self>>;
    pub(in crate::storage::wal_io) fn into_recovered_disk_session<'a,'base: 's>(
        self, owner: &Owner<'c,'s>, loan: EmptyRetentionControlLoan<'a>,
        binding: RecoveryRetentionBinding<'s,'base>,
    ) -> Result<DiskSession<'a,'s,'c,'p,'d,'o,'m,'w,W>, RetentionError>;
}
```

필드는 전부 private이며 위 표시는 소유 설명이다. `EmptyRetentionControlLoan<'a>`는 기존 control196608/replay1572864를 그대로 이동한다. 정상 전환의 `IntoRetentionError::Before{error,bound}`는 **동일 원 `Plan4Pages`**를 돌려주고 Fatal은 원 Drop/fence를 보존한다. recovered 전환은 이미 actual selection/claim 이후이므로 현재 fatal 의미와 원 loan 소비를 유지하며 healthy owner 반환으로 바꾸지 않는다. `Plan4RecoveryRuntime<'a,'m,'w,W>::bind<'s,'c,'p,'d:'p,'o>`의 `'a:'s`를 유지한다. READY handoff의 exact signature는 `Plan4Recovered<'a,'s,'c,'p,'d,'o,'m,'w,W>::into_live(self) -> Result<DiskSession<'a,'s,'c,'p,'d,'o,'m,'w,W>,Self>`이며 READY 전 **같은 Self 반환**을 유지한다.

### 6.2 statement와 snapshot의 실제 수명

```rust
pub(crate) enum StatementFinishError<V> { Before { error: RetentionError, view: V }, Fatal(RetentionError) }
pub(crate) struct RowStatementView<'v,'a,'c,'s> {
    snapshot: Option<SnapshotGuard<'v,'a,'c,'s>>,
    origin: StatementOrigin<'c,'s>, // private core/binding/fence와 owner identity
    own: Option<TxnRef>,
    sequence: u64,
    mark: Option<UndoRef>,
    phase: Cell<StatementPhase>,
    revision: Cell<u64>,
}
impl<'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>
{
    pub(in crate::storage) fn snapshot(
        &self, owner: &mut Owner<'c,'s>,
    ) -> Result<SnapshotGuard<'b,'a,'c,'s>, RetentionError>;
    pub(in crate::storage) fn read_statement(
        &self, owner: &mut Owner<'c,'s>,
    ) -> Result<RowStatementView<'b,'a,'c,'s>, RetentionError>;
}
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn begin_statement(
        &mut self, owner: &mut Owner<'c,'s>, actor: TxnRef,
    ) -> Result<RowStatementView<'b,'a,'c,'s>, RetentionError>;
    pub(in crate::storage) fn finish_statement<'v>(
        &mut self, owner: &mut Owner<'c,'s>, view: RowStatementView<'v,'a,'c,'s>,
    ) -> Result<(), StatementFinishError<RowStatementView<'v,'a,'c,'s>>>;
}
```

`StatementOrigin`·phase/sequence/mark는 private이며 raw 생성자가 없다. view는 `&RetentionControl`을 장기 보유하지 않는다. 원 control의 actual actor 슬롯에는 active statement sequence/mark를 두며 begin/prepare/finish에서 원 origin+actor+sequence를 검산한다. view의 `Cell<phase/revision>`는 그 statement의 RAM 상태로서 별도 manager/registry가 아니다. actual apply receipt를 확인한 C만 갱신하며 A는 `&view`를 전달한다. 따라서 B cursor가 `&view`를 borrow한 상태에서도 같은 직렬 statement의 후속 mutation이 가능하고, B가 기억한 revision과 달라져 무효화된다. Cell 때문에 view는 `!Sync`이며 같은 statement의 동시 실행 허가를 뜻하지 않는다.

`StatementFinishError::Before{error,view}`는 첫 효과 전 거절 때 같은 view를 반환한다. `Fatal(error)`는 mutation pending/미정 obligation과 원 fence를 보존한다. 성공 statement 종료는 실제 mark/token 해제이며 가짜 durable operation을 만들지 않는다. 실패 statement의 CLR/StatementFinish는 [소비자 Row 의미](dev3-module-consumers.md#row-wire)의 별도 mutation이다. finish는 view를 소비하므로 살아 있는 cursor와 함께 종료할 수 없다. 정상 finish는 phase를 닫고 `snapshot.take()`로 같은 guard를 한 번 checked 해제한다. read-only view Drop은 snapshot checked cleanup, mutation 진행/미정 view Drop은 원 fence+resolve 소유 보존을 수행한다. active owner가 사라진 view를 다른 open/control row에 재사용하면 origin+sequence로 거절한다. Cell·원 active-slot 값·Result carrier도 Q06의 비용에 포함한다.

### 6.3 Row read operation: 원 Owner의 단계별 재대여

다음 type의 필드는 `apply::read` 내부다. `RowCandidate`는 private lease generation+capture sequence와 관측 key/version/length만 가진 non-Copy 값이다. source pin이나 slice를 외부로 내보내지 않으므로 mutable resolver와 자기참조 lifetime을 만들지 않는다.

```rust
pub(crate) struct RowReadOperation<'u,'r,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace> {
    backend: ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>, // 원 borrow만 필드별 복사
    owner: &'u mut Owner<'c,'s>,
    statement: &'r RowStatementView<'v,'a,'c,'s>,
    loader: &'i mut [u8;409_600],
    copy: &'i mut [u8;16_384],
    lease: Option<ReferenceLease<'b,'a,'c,'s>>,
    captured: Option<CapturedRowRefs<'b,'a,'c,'s>>,
    capture_sequence: u64,
    stopped: bool,
}
pub(crate) struct RowSearchPage<'g,'b,'a,'s,'c,'p,'d> {
    source: ReadTargets<'g,'b,'c,'s,'p,'d,3>,
    lease: &'g mut ReferenceLease<'b,'a,'c,'s>,
    captured: &'g mut Option<CapturedRowRefs<'b,'a,'c,'s>>,
    copy: &'g mut [u8;16_384],
    capture_sequence: &'g mut u64,
    expectation: RecordExpectation,
}
pub(crate) enum ReadStep<T> {
    Ready(T),
    FlushRequired { target: Plan4Target },
}
impl<'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>
{
    pub(in crate::storage) fn row_operation<'u,'r,'v,'i>(
        &self, owner: &'u mut Owner<'c,'s>,
        view: &'r RowStatementView<'v,'a,'c,'s>,
        private: &'i mut [u8;425_984],
    ) -> Result<RowReadOperation<'u,'r,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>, RetentionError>;
}
impl<'u,'r,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    RowReadOperation<'u,'r,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>
{
    pub(in crate::storage) fn page<'g>(
        &'g mut self, table: &CheckedTable<'c,'s>, residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<RowSearchPage<'g,'b,'a,'s,'c,'p,'d>>, RetentionError>;
    pub(in crate::storage) fn status(
        &mut self, candidate: &RowCandidate, residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<TransactionState>, RetentionError>;
    pub(in crate::storage) fn missing_before(
        &mut self, candidate: &RowCandidate, residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<MissingRowEvidence>, RetentionError>;
    pub(in crate::storage) fn copy_candidate(
        &mut self, candidate: &RowCandidate, output: &mut [u8],
    ) -> Result<RowCopyObservation, RetentionError>;
    pub(in crate::storage) fn release_candidate(&mut self, candidate: RowCandidate)
        -> Result<(), RetentionError>;
    pub(in crate::storage) fn finish(self) -> Result<(), RetentionError>;
}
impl<'g,'b,'a,'s,'c,'p,'d> RowSearchPage<'g,'b,'a,'s,'c,'p,'d> {
    pub(in crate::storage) fn leaf(&self) -> Result<TreePageView<'_>, RetentionError>;
    pub(in crate::storage) fn capture(self, at: usize) -> Result<RowCandidate, RetentionError>;
    pub(in crate::storage) fn release_checked(self) -> Result<(), RetentionError>;
}
```

`page`는 source의 anchor/leaf/allocation whole-set을 검산하고 원 `Owner`를 **`&mut *self.owner`로 `'g` 동안만** pool에 재대여한다. `RowSearchPage.source`가 그 재대여를 가진다. 나머지 `lease/captured/copy/capture_sequence`는 operation의 서로 다른 필드 borrow다. `RowSearchPage`가 `&mut RowReadOperation` 전체와 그 내부를 borrow한 guard를 동시에 보유하는 자기참조 구조는 금지한다. guard 타입의 `'b`는 원 pool, `'g`는 content admission에 대응하며 현재 `ReadTargets<'g,'b,'c,'s,'p,'d,3>`의 실제 제네릭이다.

`capture(self,at)`는 B가 고른 slot을 실제 latched row/key/expectation과 검산한다. source latch 아래 원 lease에 Txn+U를 등록하고 caller copy에 row를 복사한 뒤 physical guards를 checked 해제한다. 이때 semantic refs는 operation에 남고 반환 Candidate에는 lease generation/capture sequence만 있다. 후보의 index는 권한이 아니다. capture sequence overflow는 pin·copy 전 거절한다. decoder가 Candidate/TxnPin/UndoPin을 생성하는 경로는 없다.

`status`와 `missing_before`는 같은 operation 안에서 원 `&mut Owner`를 다시 짧게 reborrow해 location/membership→residency→source read를 수행하고 해당 latch를 놓은 뒤 scalar 검산 결과만 반환한다. first Missing-only의 currentU/nextU handoff는 [visibility owner](dev3-visibility-retirement.md#row-missing-traversal)를 따르며 기존2행/8192B 범위다. `copy_candidate`는 actual capture의 bytes/version을 출력에 복사하는 좁은 효과이며 visible 여부의 제품 결정은 B에 남는다. 임의 RowId/version으로 observation을 생성할 수 없다. 출력·cursor 진전 규칙은 [F02](dev3-task-06-tree-rows.md#module-statement-cursor)를 따른다.

B의 실제 호출 모양은 다음과 같다. C가 point/lower-bound/next 또는 visible 분기 본문을 대신 소유하지 않는다.

```rust
let mut op = read.row_operation(owner, statement, private)?;
let page = match op.page(table, residency)? {
    ReadStep::Ready(page) => page,
    ReadStep::FlushRequired { target } => { op.finish()?; return flush(target); }
};
let at = { let leaf = page.leaf()?; row_read::seek(&leaf, seek_key, last_examined)? };
let Some(at) = at else { page.release_checked()?; op.finish()?; return end(); };
let candidate = page.capture(at)?; // source/Owner의 짧은 borrow 종료
let state = row_read::resolve_if_transaction(&mut op, &candidate, residency)?;
if row_read::visible(statement, candidate.version(), state) {
    let result = op.copy_candidate(&candidate, output)?;
    op.release_candidate(candidate)?;
    op.finish()?;
    return row_read::publish_result(result);
}
let before = op.missing_before(&candidate, residency)?;
row_read::select_missing(before)?;
op.release_candidate(candidate)?;
op.finish()?;
```

위 제품 helper는 `storage::row::read`의 함수다. `resolve_if_transaction`은 필요한 경우만 `op.status`를 호출하고 FlushRequired를 동일 cleanup 경로로 전달한다. `?`를 사용하는 내부 helper도 Drop fallback에만 맡기지 않고 결과와 cleanup을 현재 `merge` 우선순위로 합친다. Ready 반환 이전 normal path는 source guards→captured refs→reference lease 순으로 해제한다. FlushRequired는 physical/semantic refs를 모두 해제하고 operation을 stopped로 만들며 `finish` 뒤만 원 flusher에 넘긴다. error/취소는 같은 checked cleanup을 거치고, cleanup 실패가 원 요청의 Unchanged보다 강한 `CallerMustResolve`이면 원 fence 오류를 반환한다. 출력은 오류 때 유효 길이0이고 candidate/cursor key를 임의 진전시키지 않는다. operation이 원 `&mut Owner`를 보유하므로 caller는 source/ref가 살아 있는 동안 별도 Owner 작업을 수행할 수 없다.

### 6.4 기존 Catalog가 실제로 수신하는 API

Catalog read는 위 row operation의 alias로 바꾸지 않는다. 기존 shape/2-reference 검산을 보존한 `CatalogReadOperation<'u,'r,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>`가 같은 original backend/Owner/loader/copy/lease 소유 모양을 갖고, view 대신 `&'r SnapshotGuard<'v,'a,'c,'s>`를 보유한다. 실제 entry signature는 다음과 같다.

```rust
impl<'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>
{
    pub(in crate::storage) fn catalog_operation<'u,'r,'v,'i>(
        &self, owner: &'u mut Owner<'c,'s>,
        snapshot: &'r SnapshotGuard<'v,'a,'c,'s>, private: &'i mut [u8;425_984],
    ) -> Result<CatalogReadOperation<'u,'r,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>, RetentionError>;
}
impl<'u,'r,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    CatalogReadOperation<'u,'r,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>
{
    pub(in crate::storage) fn capture_catalog(&mut self, object: ObjectId)
        -> Result<CatalogCandidate, RetentionError>;
    pub(in crate::storage) fn status(&mut self, candidate: &CatalogCandidate,
        residency: &mut Plan4ResidencyCursor)
        -> Result<ReadStep<TransactionState>, RetentionError>;
    pub(in crate::storage) fn missing_before(&mut self, candidate: &CatalogCandidate,
        residency: &mut Plan4ResidencyCursor)
        -> Result<ReadStep<MissingCatalogEvidence>, RetentionError>;
    pub(in crate::storage) fn finish_found(self, candidate: CatalogCandidate)
        -> Result<CatalogRecordView<'i>, RetentionError>;
    pub(in crate::storage) fn finish_table(self, candidate: CatalogCandidate)
        -> Result<CheckedTable<'c,'s>, RetentionError>;
    pub(in crate::storage) fn finish_missing(self) -> Result<(), RetentionError>;
}
```

`capture_catalog`만 기존 Catalog fixed-leaf에서 object key를 찾는 작은 source adapter로 남으며, lookup의 snapshot version-visible/retired/Missing 선택은 `catalog::read::lookup`으로 옮긴다. `finish_found`는 refs/lease/Owner borrow를 끝낸 뒤 caller copy만 borrow하는 현재 `CatalogRecordView<'i>`를 반환한다. 이 operation의 copy field는 `Option<&'i mut [u8;16_384]>`로 보유하여 cleanup 성공 뒤 `take()`한다. Drop을 가진 owner의 field를 불법 이동하거나 별도 copy backing을 만들지 않는다. `finish_missing`는 오류/FlushRequired의 checked cleanup에도 쓰며 실제 실패의 cause/effect를 덮어쓰지 않는다. `CatalogCandidate`는 version/retired/undo 유무의 read-only getter만 제공한다. 제품 코드가 현재처럼 `refs.transaction.reference`, `storage.profile`, `read_catalog_undo`, registry를 직접 읽는 것은 금지한다.

`CheckedTable<'c,'s>`는 private core/binding/fence origin, 실제 Catalog에서 확인한 ObjectHandle/SchemaEpoch/anchor identity를 가진 non-Copy 값이다. `finish_table`만 actual captured Catalog candidate의 Table 정의·identity를 검사하고 refs를 해제한 뒤 발급한다. raw TableSpec/RecordView decoder나 caller ObjectId로 생성하지 않는다. Catalog 제품은 snapshot-visible 선택 뒤 이 method를 호출하고, Row C는 사용할 때마다 동일 open·schema·table generation·anchor/membership을 재검산한다. 실제 snapshot/source와 분리된 scalar table handle을 pin·write 권한으로 승격하지 않는다. `finish_found`의 일반 Catalog view와 달리 CheckedTable은 원 `'c,'s` 수명 밖으로 나갈 수 없다.

Catalog Commit의 제품 호출은 다음으로 닫는다. `PreparedCatalog`은 현재 `'i,'c,'s`를 유지하며 원 origin·OwnerIdentity·prefix/control cut·exact target stamps·immutable frame/before/after·digest를 보유한다.

```rust
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn prepare_catalog_commit<'i>(
        &mut self, owner: &mut Owner<'c,'s>, actor: TxnRef,
        private: &'i mut [u8;425_984],
    ) -> Result<PreparedCatalog<'i,'c,'s>, RetentionError>;
    pub(in crate::storage) fn commit_catalog<'i>(
        &mut self, owner: &mut Owner<'c,'s>, prepared: PreparedCatalog<'i,'c,'s>,
    ) -> Result<semantic_catalog::Observation, RetentionError>;
}
// storage::catalog::create의 실제 orchestration
let prepared = mutation.prepare_catalog_commit(owner, actor, private)?;
let committed = mutation.commit_catalog(owner, prepared)?;
```

prepare가 첫 효과 전 거절하면 mutable borrow와 private loan은 호출 종료로 반환된다. Prepared를 버리면 effects가 없고 loan을 재사용할 수 있다. commit이 Prepared를 소비하며 stage 전 실제 Unchanged라면 원 intent/targets를 checked 취소한 뒤 오류를 반환한다. stage 후 오류는 Prepared/loan 소유를 정상 retry로 반환하지 않으며 원 fence/obligation을 유지한다. 기존 Catalog CREATE reservation은 원 `ReservedCatalogCreate<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>` 소유 모양을 그대로 actual apply로 옮긴다. 그 `bound`는 `&'r mut DiskSession<'a,'s,'c,'p,'d,'o,'m,'w,W>`, owner/private/definition은 같은 `'r`이며 cancel의 `Before{reservation}`는 같은 wrapper를 돌려준다. TableSpec→정의/제품 순서와 snapshot-visible 결정만 sibling Catalog에 배치하고, INIT claim/Before/credit 검산은 C가 소유한다.

### 6.5 물리 WAL 밖의 actual terminal과 installer 권한

아래 internal type도 `apply` root에 정의하고 raw field는 private로 둔다. actual factory/검산 impl은 `apply::{catalog,row,terminal}` 자손에 둔다. `TerminalWal`은 원 runtime을 borrow하는 작은 내부 adapter이며 세션/append owner를 추가하지 않는다.

```rust
struct TerminalWal<'s,'m,W: WriterNamespace> { wal: &'s WalRuntime<'m,W> }
struct Binding<'c,'s> {
    core: &'c PageConcurrency<'s>, key: InstallationKey,
    requirement: WalRequirement<'s>, semantic_digest: [u8;32],
}
pub(crate) struct StagedCommit<'c,'s> { binding: Binding<'c,'s> }
pub(crate) struct DurableCommit<'c,'s> { binding: Binding<'c,'s>, durable: DurableWal<'s> }
pub(crate) struct InstalledCommit<'c,'s> { binding: Binding<'c,'s> }
pub(crate) struct UnstagedCommit<'c,'s> { core: &'c PageConcurrency<'s>, key: InstallationKey }
struct CatalogCommitWait<'c,'s> { binding: Binding<'c,'s> }
struct CatalogStageError<'c,'s> { error: WalIoError, unstaged: Option<UnstagedCommit<'c,'s>> }
struct CatalogCommitAttempt<'t,'i,'c,'s> { prepared: &'t PreparedCatalog<'i,'c,'s>, key: InstallationKey }
impl<'s,'m,W: WriterNamespace> TerminalWal<'s,'m,W> {
    fn stage_catalog<'t,'i,'c>(&self, attempt: CatalogCommitAttempt<'t,'i,'c,'s>)
        -> Result<(StagedCommit<'c,'s>,CatalogCommitWait<'c,'s>), CatalogStageError<'c,'s>>;
    fn wait_catalog<'c>(&self, wait: CatalogCommitWait<'c,'s>)
        -> Result<DurableCommit<'c,'s>, WalIoError>;
}
impl<'c,'s> DurableCommit<'c,'s> {
    fn complete_catalog(self, installed: &CatalogCommitInstalled<'c,'s>)
        -> Result<(InstalledCommit<'c,'s>,DurableWal<'s>), WalIoError>;
}
```

이 `fn`들은 apply 내부 접근만이며 실제 정의를 자손에 놓을 때 필요한 범위는 `pub(super)` 또는 **실제 조상** apply 경로다. storage 전체로 넓히지 않는다. `WalRuntime`의 physical entry는 기존 의미 그대로 `stage_verified_required<'s>(&'s self,WalPrefix,WalFrameHeader,&[u8])->Result<WalRequirement<'s>,WalIoError>`와 `wait_durable<'s>(&self,&WalRequirement<'s>)->Result<DurableWal<'s>,WalIoError>`다. Catalog/Row 타입을 입력받지 않는다.

actual terminal의 정상/오류 호출은 다음 순서로 고정한다.

1. `MutationPort::commit_catalog`가 Prepared의 origin/core/actor/control cut/prefix와 before를 재검산하고 원 `WriteTargets::prepare_commit`을 호출한다. actual `stage_call_started()`에서 받은 key만 `CatalogCommitAttempt`에 넣는다.
2. `TerminalWal::stage_catalog`가 operation/digest/origin을 검산하고 **동일 runtime의 physical stage를 직접 호출**한다. 그 호출이 실제 `Effect::Unchanged`를 반환한 분기에서만 `UnstagedCommit{core,key}`를 생성한다. 상위 에러 값을 받아 나중에 receipt로 바꾸는 public 함수는 없다. 성공일 때만 exact requirement와 digest를 Binding에 봉인한다.
3. actual requirement를 control pending에 먼저 기록한 뒤 `stage.suspend(staged)`가 원 PinSet을 `PendingCommitTargets<'u,'b,'c,'s,'p,'d,4>`로 이동한다. 같은 배열·generation이며 새 target set을 예약하지 않는다. 여기의 `'u`는 commit 호출 중 Owner borrow다.
4. physical wait가 같은 requirement를 확인한 DurableWal일 때만 DurableCommit 생성. `pending.resume_durable(&durable)`가 original Drain을 취득하고 before를 다시 검산한다.
5. actual apply adapter가 원 before stamp, complete-stage requirement, immutable after, checked successor를 결합한 `InstallGrant<'i,'c,'s>`를 만들고 page→metadata→DPT 및 원 control/history를 모두 설치한다. whole 성공 뒤만 `CatalogCommitInstalled`를 생성한다. Row도 자신의 whole-effect set과 같은 terminal receipt 계약을 사용한다.
6. `complete_catalog`가 core/key/requirement/digest/whole installed 일치를 확인한다. `install.finish(installed)`로 content/version/pins를 먼저 해제한 후만 `BoundVisibility`가 설치 Q를 공개한다. durable/control pending을 정상 완료하고 Observation/ACK를 반환한다. 중간 오류는 원 fatal/fence와 잔여 obligation이며 가짜 Unchanged/ACK 없음.

`InstallGrant<'i,'c,'s>`는 private `kind`를 가진다. `Plan4` variant의 core/stamp/requirement/after_counter/`&'i [u8;PAGE_BYTES]`는 apply actual adapter만 만든다. generic installer에는 `matches_core/identity/generation/before_counter/after_counter/requirement/after/prior_lsn/first_rec_lsn`의 읽기 method만 제공하고 생성자는 제공하지 않는다. 기존 `dirty_frame`의 stamp·exact requirement·DPT 검산을 생략하지 않는다.

첫 이행의 legacy variant는 원 `dirty_allocation::InstallPermit<'i,'c,'s>` **값을 소비 보유**한다. `apply::legacy::from_legacy(proof)`만 기존 sealed proof를 받아 wrapping하며 raw core/stamp/after를 인수로 받지 않는다. legacy producer의 actual factory는 그대로이고 둘 다 같은 installer 관찰 계약을 만족한다. 새 Catalog/Row는 이 경로를 사용하지 않는다. 이 adapter는 scope1의 모든 actual producer가 apply 검산으로 이전한 때 제거하며, 그 이전은 이번 C1–C3 완료 조건이 아니다. 별도 pool/WAL/loan을 만들거나 기존 proof와 새 proof를 동시에 mutable owner로 유지하지 않는다. enum/변환 Result의 추가 source 비용은 Q06에 포함한다. adapter와 generic installer 호출부를 한 단위로 되돌리며, grant만 되돌리고 새 producer의 설치 경로를 남기는 반쪽 rollback은 금지한다.

### 6.6 Row 후보·apply의 수신 한계

A의 grammar·issuer·statement rollback 본문은 [소비자 명세](dev3-module-consumers.md#division)의 소유다. C의 `prepare_row`는 A의 untrusted frame/target/after 후보를 받되 원 transaction/statement/actual before/target/obligation/RowMissing grammar를 전부 검산하여 `PreparedRow<'i,'c,'s>`만 반환한다. frame bytes를 decode한 사실이나 caller가 조립한 RowId는 usable proof가 아니다.

```rust
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn prepare_row<'i,'v>(
        &mut self, owner: &mut Owner<'c,'s>, view: &RowStatementView<'v,'a,'c,'s>,
        table: &CheckedTable<'c,'s>, candidate: RowChangeCandidate<'i>,
    ) -> Result<PreparedRow<'i,'c,'s>, RetentionError>;
    pub(in crate::storage) fn apply_row<'i,'v>(
        &mut self, owner: &mut Owner<'c,'s>, view: &RowStatementView<'v,'a,'c,'s>,
        prepared: PreparedRow<'i,'c,'s>,
    ) -> Result<RowMutationObservation, RetentionError>;
}
```

`RowChangeCandidate<'i>`의 borrowed frame/before/after backing은 [Row exact shape](dev3-module-consumers.md#row-shape)에 따라 caller의 유한 private lane에서 분리한다. immutable borrow가 살아 있는 동안 A가 bytes를 바꿀 수 없다. Prepared의 before/prefix/control cut은 apply에서 다시 검사하며 stage 전 dropped Prepared는 effect가 없다. complete stage 이후 실패는 healthy candidate/loan 재발급으로 취급하지 않는다. C가 `INSERT(values)`나 leaf edit/row encoding/CLR 후보의 제품 본문을 소유하지 않는다. 같은 validator/actual apply는 B의 독립 literal live fixture도 받을 수 있지만 actual Begin/Reserve/statement capability 없이 제조한 frame은 거절한다. 이 signature는 actual 검산/apply 입구의 제안이며 전체 RowReserve 수명을 닫은 완료 API가 아니다. original INIT claim·before/private producer lane을 보유한 RowReservation이 `&mut MutationPort/Owner`를 장기 borrow할 경우 위 prepare/apply와 겹칠 수 있다. 따라서 reserve 반환·producer loan·prepare/apply/cancel의 consuming wrapper와 동일 권한 반환 오류는 M2 종료 전에 추가로 동결해야 한다. candidate의 exact wire/유한 private bytes·reserve 비용은 소비자 MCQ 및 Q06에서 별도 동결한다.

<a id="common-deliveries"></a>
## 7. 유한한 공통 공급과 C0/C 인계

C는 공통 경계 구현 담당의 역할명이며 새 runtime owner가 아니다. 다음 이행 단위의 범위 후보를 채택할지 독립 D2가 판단한다. 각 단위의 구현·필수 인수·정확 selector는 아직 실행 전 계획 상태다.

| 단위 | 현재→목표·공급 동작 | 직접 선행·실제 수신자 | 제외·되돌림 단위 |
| --- | --- | --- | --- |
| C0 컴파일 계약 구현·배정 | M2에서 동결한 §4–6의 exact type/lifetime/visibility와 Row grammar/owner 의미를 구현. A/B 전용 본문/fixture·효과 없는 최소 대역 제공 | M2 미정 해소·D2; A/B C0 구현 착수 | runtime 효과·durability 증거를 제공하지 않음. 계약 변경은 영향을 받는 A/B 행만 재검토 |
| C1 원 정상 owner·물리 효과 | 원 runtime/초기화 조상 유지; actual WAL impl을 physical로 이전하여 Catalog/Row import0. 실제 Plan4Retention 소유를 apply::DiskSession으로 이전하고 원 Plan4Pages/control/history/flush/visibility를 이동. selected recovery는 새 consuming 전환 수신 | C0, 기존 Catalog 및 plan4 recovery | scope1 전체 이동 제외. 새 owner 정의·원 consuming 전환·selected READY 반환이 한 되돌림 단위. alias만으로 완료 불가; 새 wire 없음 확인 |
| C2 original read/visibility | 실제 Catalog lookup의 visible/결과 본문을 storage::catalog::read로 옮겨 제한 operation 소비. original source/load/capture/resolve는 apply::read. RowReadOperation의 Owner 재대여·Row2pin/statement view 공급 | C1, §6.2–6.4와 Q03; B와 기존 Catalog | B의 point/next/reseek·출력 본문 제외. Catalog/B의 deep import·내부 field 접근0. actual lookup 수신·checked cleanup·원 cap 회계가 한 되돌림 단위 |
| C3 apply/terminal 경계 | actual Prepared/InstallGrant/terminal receipts를 apply private root와 checked 자손 producer로 이전. Catalog actual prepare→stage→same pins→durable→whole install/Q를 MutationPort로 수신. Row observer/adapter와 actual Begin/Reserve 수신 | C1, 동결한 §6.5–6.6·Q04; A/Catalog 및 B live fixture | A의 schema/value→row/Undo/CLR 후보 본문 제외. 새 Catalog/A/B의 deep import·legacy grant 경유0. actual proof producer+generic installer+제품 호출이 한 단위 |
| C4 Row 의미의 실제 복구 수신 | 실제 source before/target/obligation 검산, Row frame replay/CLR target·잔여 예약·READY 인계. 독립 authored row corpus를 원 owner에 적용하는 제한 경로 | C3 및 Q01/Q04/Q05/Q06; A crash/reopen, B 단독 fixture | 일반 Row UPDATE/DELETE/SMO 및 전체 복구 재설계 제외. 형식이 바뀌면 fresh-fixture/현재-build 복구가 한 단위; 무검증 과거 binary downgrade 없음 |
| C5 합류 adapter | 원 session의 A Commit 결과를 B가 같은/새 snapshot과 새 process recovery로 읽는 작은 조립 | 필요한 C1–C4, A/B 국소 구현 | 두 기능 본문을 통합 담당에게 이동하지 않음. 조립·fixture·해당 diff의 인수만 포함 |

C0는 M2의 문서 후보만으로 완료되지 않는다. M3 수용 뒤 M4에서 최소 컴파일 경계가 실제 제공된 범위부터 A/B가 착수할 수 있다. C0의 계약 대역은 canonical row/schema 입력, finite logical Status/Undo 응답, explicit Busy/Capacity/Unsupported 같은 효과 없는 선택을 반환할 수 있다. 실제 `WalRequirement`, pin, snapshot/READY, install receipt, file effects를 제조할 수 없다. 대역으로 시험한 결과는 순수 기능 범위의 결과이며 C 수신 전 실제 저장·가시성·복구 성공으로 승격하지 않는다.

C의 미구현은 M2 종료를 무조건 막지 않는다. 그러나 필수 API·의미·담당·완료 인수가 미정이면 M2를 닫지 않는다. M4 전체 완료를 모든 A/B 구현의 공통 선행으로 두지 않고, C2를 받은 B와 C3를 받은 A의 독립 부분을 진행할 수 있게 배정한다.

<a id="consumers"></a>
## 8. 기존 Catalog·A·B의 실제 수신 계약

| 소비자 | 전용 제품 본문 | 받는 공통 공급 | 상대 작업 없이 가능한 범위·실제 통합 의무 |
| --- | --- | --- | --- |
| 기존 Catalog | current CREATE/lookup 의미와 pure semantic_catalog 유지. 현재 deep actual adapter는 apply로, 제품 본문은 storage::catalog로 실제 이전하여 제한 API를 호출 | C1 원 session, C2 read/ref/lookup, C3 stage/install/terminal, 해당 recovery adapter | 기존 complete-stage·durable·whole install·Q·snapshot 및 fresh/recovered owner를 실제로 통과해야 C의 backward 수신 증거가 됨. re-export 컴파일만으로 끝내지 않음 |
| A first Row INSERT | schema/value 검산→새 RowId 요청·row key/record/leaf edit·Missing Undo 후보→statement 성공/실패·rollback/full abort 제품 흐름. 계획 `storage::row::insert` | C0 후보/오류 계약, C3 원 mutation/terminal, C4 replay/CLR·READY, C2의 필요한 검증된 table/read | B의 cursor 본문·fixture를 선행으로 요구하지 않음. pure producer/경계 오류는 C0부터, 실제 INSERT/Commit·실패 statement·abort·reopen은 C 수신 뒤 |
| B snapshot point-read/cursor | key point/next/reseek, 실제 version·snapshot에 따른 선택, caller output/cursor 수명. 계획 `storage::row::read` | C0 read 계약, C2 original source→pin→Status/Undo/load, C4 실제 source fixture | A producer를 정답 생성기나 유일 fixture producer로 쓰지 않음. independently authored complete WAL/page corpus를 C4의 실제 owner 검산·replay로 복원. A와는 마지막 실제 합류에서 연결 |

B fixture는 checksum/grammar decode만으로 live 권한을 얻지 않는다. C가 untrusted frame의 actual before·target·owner·obligation을 검산하고 원 선택/replay 또는 actual stage 경계에서만 권한을 발급한다. A는 같은 grammar를 생산하는 실제 기능을 별도로 구현한다. 독립 corpus와 actual C 연결을 구체화할 수 없으면 B의 착수·인수 인계가 미정이므로 M2 미완료다.

합류의 최소 관측 후보는 같은 기준에서 A가 별도 DML 거래로 INSERT/Commit한 row를 B가 새 snapshot에서 읽고, Commit 전 snapshot에서는 올바른 Missing/이전 값을 관측하는 것이다. 별도 process의 현재-build recovery 후 같은 RowId/value·Status/Undo 의무를 대조한다. cursor·point 두 경로, statement 실패·full abort, stale table/schema/generation, cancellation/cleanup의 정확 사례 수는 Q07에서 고정한다. root가 두 제품 기능의 전용 source·시작 기준·대기 원인·merge/의미 충돌을 기록한다.

<a id="validation"></a>
## 9. 변경에 대응할 검증·자원·복구 의무

[사용자 시험 범위](../../AGENTS.md)대로 마지막 검증 이후 실제 diff와 선택 case의 관계를 실행 전에 고정한다. 구조 전환·단계 종료·HEAD 변경만으로 완료한 무변경 시험이나 전체 wrapper를 반복하지 않는다. 과거 결과를 새 source 실행의 PASS로 기록하지 않는다. 문서 verifier는 이 문서 편집 단계에 실행하지 않는다.

| 변경 경계 | 필요한 검증 후보 | 독립 기대값·보존 조건 |
| --- | --- | --- |
| C1 consuming owner/ports | writer/File/slot/fence identity·loan 한 번 이동, reader claim·poison/notify·binding 실패, port escape/중복 borrow 거절 | 기존 실제 owner identity와 원 failure effect. raw constructor/fields/Clone이 열리지 않았는지 source 및 compiler exact failure 확인 |
| C2 source/ref/read | actual source latch→일괄 pin→latch release→Status/Undo load, stale generation·wrong core·retirement gap, finite load/flush 진행, snapshot/cursor cleanup | 독립 row/key/version 기대값, pin multiset·reference count. Catalog 2-ref 범위를 Row 전체 상계로 복사하지 않음 |
| C3 apply/terminal | exact requirement/core/key/digest, same PinSet pending, before 재검산, lower flush와 Drain 순서, first-write·Unknown·partial install·Q/ACK | 원 의미와 독립 witness. 실제 stage 전 취소와 stage 후 fence 구분; generic bit mask 완료 거절 |
| C4 recovery | 새 Row operation별 before/after·CLR·예약 복원·재장애·새 process READY/조회, 잘못된 corpus 거절 | A producer와 독립 authored input/expected. 실제 선택 source/ELF·fault cut·원 FAIL 보존 |
| A/B 합류 | 같은 C 기준에서 각각 국소 인수 후 actual insert→snapshot read/cursor→reopen | 대역과 실제 효과 결과 분리. 공통 C만 완료하거나 테스트 도구 병행만으로 M5 완료 판정 금지 |

자원은 `size_of` 한 값만 비교하지 않는다. 기존 owner와 새 view/Result/return carrier의 동시 생존량, pending target 배열·private before/after/frame loan, registry·replay·control·flush backing, concrete memory/native W/File, FD와 I/O phase를 계산한다. 새 borrowed wrapper도 source-value 비용에 포함한다. 기존 component65,536B·metadata196,608B·whole262,144B 등 해당 owner 한도와 총 profile 예산을 임의 확대하지 않는다. 어떤 bound가 A/B에 적용되는지와 필요한 추가 backing/예약량은 Q06에서 동결한다.

필수 case inventory·시간/출력/disk budget·중단 조건·compiler selector·literal oracle가 확정되기 전 새 제품 효과 실행의 PASS를 주장하지 않는다. 실제 변경 때문에 필요한 현재-build 복구는 유지하되 무변경 하위 복구 전체를 일괄 반복하지 않는다.

<a id="alternatives"></a>
## 10. 대안·위험·되돌림

| 대안 | 판단·이유 |
| --- | --- |
| 폴더 이동과 `pub use` facade만 추가 | 기각 후보. descendant private 접근·물리 WAL의 Catalog import·소비자별 permit 등록이 남으면 독립 작업 경계가 달라지지 않음 |
| core/plan/binding/WAL field를 `pub(crate)`로 확대 | 기각. 의미·권한·수명 조립을 각 소비자가 다시 구현하게 하고 raw factory의 범위를 넓힘 |
| 공통 `Arc<Mutex<StorageContext>>`, 새 manager/pool/fence | 기각. 원 owner·loan·pin·CAS를 중복하거나 숨기고 진행/lock 순서를 바꿈 |
| 모든 기능을 trait/callback으로 추상화 | 보류. 실제 대체 구현·effect 경계에 필요할 때만 적용. 현 범위는 concrete borrowed port와 typed grant가 먼저 |
| Catalog semantic 구현을 Row로 rename·복사 | 기각. fixed role/first Missing/sequence·inventory·credit가 Row chain·statement 의미와 다름 |
| 현재 Rust 트리 전체를 한 번에 sibling으로 이동 | 첫 전환에서는 기각 후보. compiler probe·source census·test selector 변경을 불필요하게 확대. C 단위별 실제 private 접근 제거와 소비자 수신을 먼저 수행 |
| 기존 live/recovery 장부 공통 상태 기계 | 기각. 같은 offset·필드 모양이 같은 전이 의미가 아님. phase-specific 소유·잔여 obligation을 유지 |

주요 위험은 grant 생성자의 가시성 확대, 원 owner 이동 중 새 fence/Condvar/slot 생성, 반환 carrier 증가로 인한 자원 상계 초과, Drop·poison·notify 순서 변경, source cut과 physical load 사이의 검산 누락, decoder→READY/semantic grant 승격이다. logical module 경로를 참조하는 기존 compiler fixture와 source 검산 script도 이동 영향에 포함한다. 관련 없는 과거 증거 경로·source identity는 이름 정리로 바꾸지 않는다.

되돌림은 각 C 단위의 API·실제 소비자·해당 proof factory를 함께 이전 구현으로 돌리는 source 단위다. 반쪽 adapter를 남긴 채 이전 raw 접근을 다시 여는 방식은 허용하지 않는다. 영속 Row 형식을 도입하는 C4/A 이후에는 [버전 정책](../policies/version-compatibility-policy.json)의 사내 fresh fixture와 현재-build recovery를 따른다. 이전 binary가 새 형식을 읽을 수 있다는 약속은 하지 않는다.

<a id="open-questions"></a>
## 11. M2 미완료 항목과 종료 조건

| ID | 필수 미정사항 | 담당 역할·닫을 산출물 |
| --- | --- | --- |
| Q01 | [소비자 issuer/profile 제안](dev3-module-consumers.md#row-issuer)의 원 owner·영속 field·checked successor·실패 후 비재사용, single-leaf 범위의 독립 D2 수용 | Row/format 계약 담당 + C/A; F01/F02·wire/version 영향과 독립 oracle |
| Q02 | **구체 설계 제안 작성 완료·독립 D2 대기.** §4의 current→target/실제 조상 seal과 §6.1–6.5의 full generic/lifetime·owner/error/loan 반환으로 기존 owner/Catalog/B 동결안을 제시. §6.6의 RowReserve→producer→prepare/apply consuming wrapper는 미결이며 M2 종료 전 보완. C0에 설계를 미루지 않음 | 경계 담당 + 독립 D2; physical Catalog import0, actual grant 생성 이전, Catalog/A/B 내부 접근0과 오류/복구 반례 검토. 구현·borrow compiler 통과를 이미 확인한 것은 아님 |
| Q03 | **API·Owner 재대여 설계 제안 작성 완료.** §6.2–6.4의 Cell view/원 active-slot/operation별 loan split·capture/resolver/checked cleanup. Missing2pin handoff·own-write·cursor 오류 진전의 owner 의미 수용은 독립 D2 대기. 일반3-reference/occupied before 제외 | visibility/page 담당 + B/C; [visibility](dev3-visibility-retirement.md#row-missing-traversal)·[statement/cursor](dev3-task-06-tree-rows.md#module-statement-cursor)와 source/순서 반례를 대조 |
| Q04 | 첫 Row forward/statement rollback/full abort/CLR/terminal의 grammar·effect·head chain·credit 의미와 A/C 책임 분리 | undo/transaction 담당 + A/C; semantic observer 공급과 제품 producer 본문의 구분 |
| Q05 | [B 독립 fixture 제안](dev3-module-consumers.md#independent-fixtures)의 committed corpus→actual replay와 own-write actual Begin/Reserve→literal Forward→동일 validator/apply를 독립 D2 검토 | 인수 담당 + B/C; A producer 없는 입력·기대값, READY에서 old Active statement를 재사용하지 않음 |
| Q06 | 새 view/grant/Result·Row private loan·ref registry·FD/WAL/maintenance 동시 상계와 회수 진행, concrete source census | resource 담당 + C; 수치·상한·before-effect admission·현재 source 계산 |
| Q07 | C1–C5/A/B 정확 선택 inventory·failure cut·현재-build 복구·합류 관측·시간/출력/disk 한도 | 인수·통합 담당; 실행 전 고정 case/selector/oracle, 원 완료 시험 반복 제외 |
| Q08 | A/B backlog ID·전용 source/fixture·C0 대역 허용과 실제 C 수신 시점, 각 C 단위 제외/되돌림·담당 확정 | root와 A/B/C; [M2 인계표](../plans/module-independence-plan.md#m2-handoff) 완결 |

이 문서의 source 조사와 Q02/Q03의 구체 설계 제안은 다른 미정 해소나 D2 수용을 대신하지 않는다. 선정 범위의 API·권한·필수 공급·인수 배정이 확정되고 별도 검토자가 원 source와 정상/실패/복구 반례를 대조한 뒤만 M2 완료를 판단한다. 확정된 C의 미구현·미실행은 그 뒤 M4/M5 의무로 남기며 M2에서 prototype·컴파일·제품 시험을 수행할 이유가 되지 않는다.

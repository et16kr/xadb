# DEV3 모듈 독립 개발을 위한 원 소유·소비 경계

2026-10-26 · `D3-MODULE-BOUNDARIES-001` · **D2 ACCEPTED / M2 완료 / M4 C0 타입·C1 물리 WAL·원 owner/port·isolation/epoch backing·Catalog epoch 활성화 부분공급 / 공통 공급·제품 인수 미완료**

[구조 전환 계획](../plans/module-independence-plan.md) · [M1 조사·기준선](../evidence/module-independence-survey-20261026.md) · [구현 계약](implementation-contracts.md) · [저장 정본](disk-storage-design.md) · [원 runtime/core](dev3-storage-session.md#private-runtime-core)

조사 source는 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`다. 조사와 이 문서 작성은 `/root/module_boundary_survey`, 계약·통합과 문서 등록은 `/root`가 담당한다. 이 문서는 현재 소유와 목표 경계를 구별한 채택 설계다. §4–6은 [최종 독립 D2 검토](../evidence/module-independence-survey-20261026.md#final-design-review)를 수신해 동결했다. 아래 경로·타입·signature의 채택은 실제 구현 증거가 아니며, 새 경로는 실제 구현할 때 build inventory에 등록한다. 기존 source별 PASS·FAIL·UNKNOWN·NOT_RUN과 DEV2/DEV3 미완료 상태를 유지한다.

<a id="scope"></a>
## 1. 첫 전환 범위와 판단

선정한 첫 소비자는 **A: first Row INSERT**, **B: snapshot row point-read/cursor**다. 두 기능은 원 저장 세션에서 생성·복구된 같은 table/row와 실제 snapshot을 사용한다. 현재 `row_record`와 tree edit의 byte 기능만으로 live INSERT나 snapshot row 조회가 공급됐다고 판단하지 않는다. A/B의 범위·작업 ID·필수 인수는 [DEV3 작업 계획](../plans/dev3-implementation-plan.md#remaining-work) 및 채택한 M2 소비자 작업 명세에 동결했다. RowId·statement·Row Forward/CLR 의미는 소비자 담당의 별도 소유 설계, source/ref handoff는 [Missing Row의 visibility 계약](dev3-visibility-retirement.md#row-missing-traversal), statement/cursor 수명은 [tree·row의 F02 계약](dev3-task-06-tree-rows.md#module-statement-cursor)을 따른다. 이 문서는 그 의미를 중복 정의하지 않는다.

현재 단일 writer/File/slot/fence/pool 소유는 보존할 기반이다. 개선할 대상은 이 소유를 지키는 수단이 초기화 단계의 깊은 자손 모듈과 private 필드 접근에 묶인 부분, 물리 WAL이 Catalog Commit 의미를 수입하는 부분, 소비자 추가 때 반복되는 재노출·permit 등록이다. **원 owner의 책임 배치와 실제 제한 API를 바꾸고, 기존 Catalog가 그 API를 먼저 소비하게 한다.** 새 re-export facade나 파일 이동만으로 M4 공급 완료를 판정하지 않는다.

첫 범위에서 일반 extent 성장·SMO·overflow·index·checkpoint/backup·전체 DEV3 모듈화를 선행으로 추가하지 않는다. 그렇다고 첫 INSERT의 RowId 발급, statement 실패, undo/CLR, terminal, 현재-build 복구를 생략하지 않는다. single RowLeaf·inline2048·index0·여러 statement의 구체 범위와 명시적 거절은 [소비자 명세](dev3-module-consumers.md#profile)의 채택 범위이며 실제 제품 공급·인수와 구별한다.

<a id="actual-ownership"></a>
## 2. 현재 상태 소유와 권한 인계

| 실제 symbol·source | 소유 상태 | 현재 인계와 생성 제한 |
| --- | --- | --- |
| [`WalRuntime`, `AppendState`](../../crates/engine/src/storage/wal_io.rs) | `Mutex<W>`, `Mutex<W::File>`, 원 mutable WAL slot·slot state·prefix, Condvar, first-write marker, SessionFence, Profile | `GenesisWalSession` 안에 하나. stage는 실제 slot copy 뒤 exact `WalRequirement`를 생성; `DurableWal`은 같은 원 requirement를 확인 |
| [`InstalledStorageCore`, `FreshStorageSession`](../../crates/engine/src/storage/wal_io/bootstrap.rs) | 같은 WalRuntime, immutable BootstrapPlan, reader claim | Fresh는 core만 보유. Status/Undo 역할은 원 plan·fence의 typed borrow이고 file/payload 권한이 아님 |
| [`DoublewriteStorageSession`](../../crates/engine/src/storage/wal_io/doublewrite_install.rs) | Fresh, 원 DWB workspace loan, InstallPlan | 실제 설치를 마친 Fresh를 소비. 다른 namespace/WAL을 생성하지 않음 |
| [`MutableStorageOwner`](../../crates/engine/src/storage/wal_io/mutable_storage.rs) | core, DWB plan, `Mutex<Option<&mut workspace>>`, 유일 binding CAS | `into_mutable_storage`는 기존 core/plan/loan 이동. `BindingClaim` 실패·Drop은 원 fence/ABANDONED를 유지 |
| [`Plan4Pages`](../../crates/engine/src/storage/wal_io/plan4_pages.rs) | Mutable borrow, 실제 SharedPool 하나, operation loan, consuming activation용 DWB loan | `bind_plan4_pages`는 원 AVAILABLE→CLAIMED→BOUND와 원 loan take를 사용. 새 pool/map/CAS owner를 만들지 않음 |
| 조사 당시 `plan4_retention.rs::Plan4Retention` · [현재 정의](../../crates/engine/src/storage/wal_io/plan4_apply.rs) | 위 Plan4Pages, control bank, RuntimeHistory와 replay loan, flush control, visibility, recovery binding | 조사 당시 `into_empty_retention`이 원 pool/loan을 이동. `split`이 writer/read/flusher에 disjoint borrow를 제공. C1의 실제 이전은 §4.2의 후속 기록을 따름 |
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
## 4. 목표 책임·허용 import 그래프

다음은 독립 D2 검토를 수신해 M2에서 동결한 **채택 설계**다. 첫 이행은 legacy scope1의 초기화·복구 전체를 sibling으로 옮기지 않는다. `WalRuntime`, `InstalledStorageCore`, `MutableStorageOwner`, `Plan4Pages`의 원 소유와 초기화 조상은 유지한다. 대신 물리 WAL의 actual method, 정상 live owner·apply 조립, Catalog의 제품 본문을 서로 다른 책임으로 실제 이전한다. 깊은 legacy 내부 관계는 잔여로 명시하며 이 전환으로 저장 모듈 전체가 독립됐다고 주장하지 않는다.

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
| `apply::state`의 기존 control/history/visibility 구현 | 다른 apply 자손에서 필요한 기존 필드/method만 실제 apply 조상까지 제한. `pub(in crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::apply)` 또는 같은 실제 조상을 가리키는 `pub(super)`·`pub(in super::super)`를 사용하며 alias를 `pub(in ...)` 조상으로 사용하지 않음. 전체 storage field 공개나 소비자 getter를 추가하지 않음 |
| `SelectedEpochParts/EpochTransferSeal`, `RecoveryRetentionBinding` | 원 `mutable_storage::plan4_recovery` 위치·seal 유지. `MutableStorageOwner::from_plan4_recovery`만 original parts 소비; `apply`는 raw parts를 받거나 재생성하지 않음 |

Rust privacy는 임의 코드를 trusted apply 자손에 추가하는 repository 변경 자체를 방지하는 보안 경계는 아니다. C 소유·source review와 실제 sibling consumer의 import 제한이 함께 계약이다. 단, 정상 소비자의 raw constructor 호출은 Rust privacy로 거절되어야 한다.

C1의 기존 시험 selector 보존을 위해 production에서는 `plan4_pages::apply`, `cfg(test)`에서는 `plan4_pages::retention`이 같은 `plan4_apply.rs`를 읽고 후자에 `use retention as apply`를 둔다. 두 선언은 상호 배타적이고 조상 깊이·visibility가 같다. 이 이름 호환은 시험용 효과·권한·분기 구현을 추가하지 않으며 신규 소비자는 두 cfg 모두 `storage::disk`를 사용한다.

### 4.2 current → target의 실제 이동

| 현재 symbol·본문 | 목표 symbol·경로 | 없어져야 하는 외부 접근 |
| --- | --- | --- |
| `wal_io.rs::WalRuntime::{stage_verified_required,wait_durable,...}`의 물리 구현 | `wal_io::physical`의 같은 original runtime impl | physical의 `bootstrap::CatalogCommitAttempt/Installed`, `semantic_catalog::Operation` 수입0. runtime 정의 자체를 sibling으로 옮기기 위해 scope1 필드를 공개하지 않음 |
| `plan4_retention.rs::Plan4Retention`, `split`, `into_empty_retention` | `apply::DiskSession`, `split`, `Plan4Pages::into_disk_session` | 새 struct가 원 `Plan4Pages` 전체와 control/history/visibility/flush를 실제 소유. 단순 `type DiskSession=Plan4Retention`은 C1 완료가 아님 |
| `RetentionReader/Writer/Flusher` | `apply::{ReadPort,MutationPort,MaintenancePort}` | 새 제품 본문에 `storage`, `pool`, `control`, `history`, `visibility` field가 보이지 않음. port body는 원 borrow를 사용 |
| `read/lookup.rs::lookup_catalog`의 key 선택·version-visible·결과 분기 | `storage::catalog::read::lookup` | 제품 본문의 `storage.profile/core.plan`, `pool.bound_core`, registry·pin 직접 접근0. 실제 Catalog도 아래 source→capture→resolver API 수신 |
| `read/catalog.rs`, `lookup_load.rs`의 actual membership/load/source 생성 | `apply::read::{catalog,load}` | whole plan 대신 원 namespace·fixed target의 checked metadata 사용. `CatalogSource::from_guard`의 source 검산은 C 안에 유지 |
| `catalog_live.rs::prepare_catalog_commit`, `PreparedCatalog` | `MutationPort::prepare_catalog_commit`, `apply::PreparedCatalog` | 제품 본문은 원 Owner·실제 RowTransaction의 공유 borrow·private loan을 전달하고 commit 때 같은 transaction을 소비한다. active statement는 prepare 전에 종료한다. 원 before/control cut/owner/target 검산과 frame materialization을 통과하지 않고 Prepared 생성 불가 |
| `catalog_commit.rs::RetentionWriter::commit_catalog`와 `commit_bridge.rs` | `MutationPort::commit_catalog`, `apply::terminal::{catalog,wal}`의 실제 조립 | 제품·물리 WAL에 raw stage, `Binding`, requirement 조합, whole install/Q factory 없음 |
| Catalog CREATE의 제품 순서·TableSpec 처리 | `storage::catalog::create`가 `disk::MutationPort::reserve_catalog_create`에 원 MaintenancePort·Owner·RowStatementView·Undo target·TableSpec/private를 전달하고 opaque reservation의 checked `initialize/cancel` 소비 | 예약 wrapper가 disjoint mutation/owner/claim/private/원 flush lease를 보유하는 실제 C 부분만 `apply::catalog`에 남음. `TxPairClaimKey`·original INIT claim을 제품에서 조립하지 않음 |
| `dirty_allocation::InstallPermit`을 plan4 새 producer가 생성하던 부분 | `apply::{catalog,row}`가 `apply::InstallGrant` 실제 생성 | 새 Catalog/Row가 `from_retention`의 중앙 등록을 거쳐 옛 allocation factory를 계속 쓰면 C3 미완료 |
| `Plan4RecoveryRuntime::bind_retention`·`Plan4Recovered::into_live` | 원 위치에서 새 consuming `into_recovered_disk_session`/`DiskSession`을 수신 | 선택·epoch·base·READY owner는 그대로. fresh 합성, 원 W/File/slot/fence/loan 재생성 없음 |

`BootstrapPlan`, `InstallPlan`, scope1 physical installer와 기존 recovery 선택 코드를 한 번에 이동하지 않는다. 기존 plan4 정상 owner에 붙은 미전환 helper는 physical file을 그대로 두고 `plan4_apply.rs`의 `#[path = "plan4_retention/..."]` 자손 선언으로 원 owner를 계속 사용한다. 이것은 C1 owner 정의 이전에 필요한 내부 연결이며 독립 기능으로 승격한 것이 아니다. 현재 `retention` 재노출 이름은 이행 중 타입 alias/shim만 남길 수 있으나 신규 Catalog/A/B가 이를 수입하는 것은 금지한다. 미전환 helper의 명시적 잔여 목록은 C1 diff 기록에 남기며 source별 selector identity는 유지한다. 원 `Plan4Pages`는 Drop이 있으므로 필드를 억지로 destructure하거나 `ManuallyDrop/unsafe`로 옮기지 않고 새 `DiskSession.inner`에 **값 전체를 이동**한다. 기존 `_flush_workspace.take()`는 성공 전환에서 한 번만 수행한다.

현재 [C1 원 owner/port 부분공급](../evidence/module-independence-survey-20261026.md#c1-owner-ports)은 실제 `DiskSession`·세 port의 정의/private 필드, 정상 consuming 전환·split 및 selected recovery의 반환 타입을 이전했다. 기존 이름은 새 정의의 역방향 alias와 consuming shim이다. 후속 [control/isolation 분리](../evidence/module-independence-survey-20261026.md#c1-isolation-backing)는 기존 영속 원장을 `RetentionLedger`로 이전하고 같은 원 control 안에서 별도4160B backing과 동일 mutex 대여를 공급했다. 후속 [epoch1600B 소유](../evidence/module-independence-survey-20261026.md#c1-epoch-backing)는 원 publication mutex에 미활성 loan을 연결했다. 후속 [Catalog epoch 활성화](../evidence/module-independence-survey-20261026.md#c1-epoch-activation)는 fresh/최종 복구 seeding과 actual CREATE 변경목록·Commit 공개를 연결했다. [읽기 전용 capsule/view](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)와 장기 credit은 부분공급했다. Reserved 관측 API·쓰기 거래·자동 read-set·short admission과 Row API는 아직 공급하지 않았다. 컴파일4건과 신규 메모리2건의 한정 수용을 전체 C1 또는 복구 handoff runtime 인수로 해석하지 않는다.

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
## 6. M2에서 동결한 signature·수명

다음 signature의 타입은 채택한 구현 계약이다. 실제 코드·compiler 결과를 대신하지 않는다. `W: WriterNamespace`와 lifetime을 생략하여 C0에서 해결할 항목으로 남기지 않는다. `std::result::Result`를 `Result`로 표기하며 각 두 번째 인수가 실제 오류 타입이다. 기존 오류의 phase/cause/effect와 cleanup 우선순위는 유지하고 새 Row cause만 해당 owner 계약으로 추가한다. 내부에 쓰인 기존 타입은 현재 source의 실제 제네릭과 일치한다.

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
    control: RetentionControl<'a,'c,'s>, // ledger와 isolation의 단일 소유
    history: RuntimeHistory<'a,'c,'s>,
    flush: RetentionFlushControl<'c,'s,'w>,
    visibility: Option<BoundVisibility<'a,'c,'s>>,
    recovery_binding: Option<RecoveryRetentionBinding<'s,'s>>,
}
pub(crate) struct ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m,'w,W>,
    pool: &'b SharedPool<'c,'s,'p,'d>,
    history: &'b RuntimeHistory<'a,'c,'s>,
    isolation: &'b Mutex<IsolationLedger<'a,'c,'s>>,
    visibility: Option<&'b BoundVisibility<'a,'c,'s>>,
}
pub(crate) struct MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m,'w,W>,
    pool: &'b SharedPool<'c,'s,'p,'d>,
    operation: &'b mut &'o mut [u8;114_688],
    control: &'b mut RetentionLedger<'a,'c,'s>,
    history: &'b RuntimeHistory<'a,'c,'s>,
    isolation: &'b Mutex<IsolationLedger<'a,'c,'s>>,
    visibility: Option<&'b BoundVisibility<'a,'c,'s>>,
}
pub(crate) struct MaintenancePort<'b,'s,'c,'p,'d,'m,'w,W: WriterNamespace> {
    storage: &'s MutableStorageOwner<'m,'w,W>,
    pool: &'b SharedPool<'c,'s,'p,'d>,
    flush: &'b mut RetentionFlushControl<'c,'s,'w>,
}
// 같은 original control의 두 disjoint field. 별도 manager가 아니다.
struct RetentionControl<'a,'c,'s> {
    ledger: RetentionLedger<'a,'c,'s>,
    isolation: Mutex<IsolationLedger<'a,'c,'s>>,
}
struct IsolationLedger<'a,'c,'s> {
    bytes: &'a mut [u8;4_160],
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
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

필드는 전부 private이며 위 표시는 소유 설명이다. `EmptyRetentionControlLoan<'a>`는 기존 control196608/replay1572864와 별도 isolation4160 loan을 이동한다. 기존 `RetentionControl`의 byte-backed durable fields/method는 `RetentionLedger`로 실제 이전하고 원 `RetentionControl` 안에서 isolation과 disjoint 대여한다. BoundVisibility의 epoch1600도 별도 caller loan이며 기존8192 registry나196608 ATT padding에 숨기지 않는다. source/lifetime 구조 변경은 C1 공급이고 기존 raw-state getter를 늘리는 대체안은 없다. 정상 전환의 `IntoRetentionError::Before{error,bound}`는 **동일 원 `Plan4Pages`**를 돌려주고 Fatal은 원 Drop/fence를 보존한다. recovered 전환은 이미 actual selection/claim 이후이므로 현재 fatal 의미와 원 loan 소비를 유지하며 healthy owner 반환으로 바꾸지 않는다. `Plan4RecoveryRuntime<'a,'m,'w,W>::bind<'s,'c,'p,'d:'p,'o>`의 `'a:'s`를 유지한다. READY handoff의 exact signature는 `Plan4Recovered<'a,'s,'c,'p,'d,'o,'m,'w,W>::into_live(self) -> Result<DiskSession<'a,'s,'c,'p,'d,'o,'m,'w,W>,Self>`이며 READY 전 **같은 Self 반환**을 유지한다.

<a id="isolation-api"></a>
### 6.2 transaction·statement와 snapshot의 실제 수명

[격리 owner](dev3-task-10-concurrency.md#module-row-isolation)의 default Serializable과 RC를 실제 API에서 구별한다. 현재 source에는 독립 CommitCoordinator가 없으므로 새 manager를 합성하지 않는다. 원 `MutationPort.control: &mut RetentionLedger`의 유일 borrow를 validation→stage/durable→whole install→table/catalog epoch와 Q 공개까지 유지한다. `Installation::Open` 복귀는 공개 전일 수 있어 그 값만으로 transaction serialization을 완료했다고 하지 않는다. read-only Serializable 종료도 같은 writer/control 직렬화 경계를 통과한다.

```rust
pub(crate) struct RowTransaction<'v,'a,'c,'s> {
    snapshot: Option<SnapshotGuard<'v,'a,'c,'s>>, // Serializable만 Some
    lease: IsolationLease<'v,'a,'c,'s>,          // 원4160 ledger의 slot/slotgen receipt
    source: CallerCredit<'v,'a,'c,'s>,
    phase: Cell<TransactionPhase>,
}
pub(crate) struct RowStatementView<'t,'v,'a,'c,'s> {
    transaction: &'t RowTransaction<'v,'a,'c,'s>,
    rc_snapshot: Option<SnapshotGuard<'v,'a,'c,'s>>,
    sequence: u64,
    mark: Option<UndoRef>,
    phase: Cell<StatementPhase>,
    revision: Cell<u64>,
    source: CallerCredit<'v,'a,'c,'s>,
}
impl<'t,'v,'a,'c,'s> RowStatementView<'t,'v,'a,'c,'s> {
    pub(in crate::storage) fn q(&self) -> Result<u64, RetentionError>;
    pub(in crate::storage) fn own_actor(&self) -> Option<TxnRef>;
    pub(in crate::storage) fn revision(&self) -> u64;
}
pub(crate) enum StatementFinishError<V> {
    Before { error: RetentionError, view: V },
    Fatal(RetentionError),
}
pub(crate) enum TransactionFinishError<T> {
    Before { error: RetentionError, transaction: T },
    RollbackRequired { error: RetentionError, transaction: T },
    Rejected(RetentionError), // 읽기 전용 conflict: slot/snapshot은 checked 종료됨
    Fatal(RetentionError),
}
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn begin_transaction(
        &mut self, owner: &mut Owner<'c,'s>, options: TransactionOptions,
        private: &mut [u8;425_984],
    ) -> Result<RowTransaction<'b,'a,'c,'s>, RetentionError>;
    pub(in crate::storage) fn begin_statement<'t,'v>(
        &mut self, owner: &mut Owner<'c,'s>, transaction: &'t RowTransaction<'v,'a,'c,'s>,
    ) -> Result<RowStatementView<'t,'v,'a,'c,'s>, RetentionError>;
    pub(in crate::storage) fn finish_statement<'t,'v>(
        &mut self, owner: &mut Owner<'c,'s>, view: RowStatementView<'t,'v,'a,'c,'s>,
    ) -> Result<(), StatementFinishError<RowStatementView<'t,'v,'a,'c,'s>>>;
    pub(in crate::storage) fn finish_read_only<'v>(
        &mut self, owner: &mut Owner<'c,'s>, transaction: RowTransaction<'v,'a,'c,'s>,
    ) -> Result<ReadOnlyCommit, TransactionFinishError<RowTransaction<'v,'a,'c,'s>>>;
    pub(in crate::storage) fn finish_aborted<'v>(
        &mut self, owner: &mut Owner<'c,'s>, transaction: RowTransaction<'v,'a,'c,'s>,
    ) -> Result<(), TransactionFinishError<RowTransaction<'v,'a,'c,'s>>>;
    pub(in crate::storage) fn commit_row<'v>(
        &mut self, owner: &mut Owner<'c,'s>, transaction: RowTransaction<'v,'a,'c,'s>,
        private: &mut [u8;425_984],
    ) -> Result<RowCommitObservation, TransactionFinishError<RowTransaction<'v,'a,'c,'s>>>;
}
```

`TransactionOptions { isolation, read_only }`는 순수 요청 값이다. 기본 isolation은 Serializable이며 raw TxnRef/q로 RowTransaction을 만드는 생성자는 없다. begin은 원 ledger에 slot/slotgen을 선예약하고 Serializable 실제 snapshot을 등록한 뒤, read-write이면 original Status/TerminalJob/Begin을 공급한다. 실패 전의 예약은 checked 반환하고 Begin stage 이후 불확실성은 원 fence/obligation이다. read-only는 durable actor를 제조하지 않고 actor-kind None과 same-open slotgen으로 인증한다. read-only capsule도 실제 isolation slot을 점유하므로 무한 standalone snapshot을 transaction으로 세지 않는다.

`IsolationLease<'v,'a,'c,'s>`는 `&'v Mutex<IsolationLedger<'a,'c,'s>>`와 `&'v BoundVisibility<'a,'c,'s>`, 원 core/binding/fence, slot index/generation과 optional 실제 actor를 가진 private non-Copy receipt다. Serializable snapshot은 caller-owned capsule 안에 있고 view는 capsule을 borrow한다. control이 자신 안의 visibility를 borrow하는 self-reference는 없다. RC view만 statement마다 새 actual snapshot을 소유하며 Serializable view는 capsule의 같은 snapshot을 읽는다. RC snapshot 생성은 transaction lease의 원 visibility에서 수행하여 `'v`를 보존한다. `ReadPort::snapshot`만 호출하는 legacy 경로는 새로운 Serializable transaction 완료 API가 아니다.

isolation backing은 원 control의 별도4160B=`64+32×128`이며 epoch는 원 BoundVisibility의 별도1600B=`64+32×48`다. slot의 actual phase/mode/actor-kind/catalog flags/readmask/writemask/slotgen/startQ/TxnRef/active statement sequence는 [격리 owner](dev3-task-10-concurrency.md#module-row-isolation)에 따른다. active mark는 slot88..128의40B에 canonical Option<UndoRef>로 보유하며 None은 전부0이다. 기존 ATT1664의 zero-padding 규약은 바꾸지 않는다. 이 metadata는 고정 caller loan으로 초기화되고 새 process에는 old capsule/slotgen이 승계되지 않는다.

`ReadPort`와 `MutationPort`는 같은 isolation mutex를 공유하고, mutation만 기존 durable ledger의 exclusive borrow를 가진다. table mapping은 원 epoch32 slots 안에서 open 동안 안정적이며 eviction/DROP은 이번 scope 밖이다. `CheckedTable`은 검산된 table slot을 포함한다. source latches 전에 epoch identity를 preflight하고 실제 source latch 아래 같은 identity/membership을 다시 확인한 뒤 **C가** read/write set을 원 isolation ledger에 등록한다. set lock 아래 I/O·상위 guard·epoch lookup은0이다. point Missing·range End·Catalog absence도 실제 source를 읽었으므로 등록한다. 성공/실패 statement 종료는 transaction read/write mask를 지우지 않는다. caller가 finish 시 선택적으로 merge하는 API는 없다.

view는 `&RetentionLedger`를 오래 보유하지 않는다. 원 isolation slot의 actor+slotgen+active sequence+mark를 begin/reserve/prepare/apply/finish에서 검산한다. view의 Cell은 statement의 phase/revision RAM 상태이며 actual receipt를 확인한 C만 바꾼다. q/own_actor/revision getter는 같은 실제 snapshot/slot의 관측값만 돌려주며 guard/lease를 반환하지 않는다. q는 health/active phase를 검사하고 B는 이 값으로 visibility 분기를 수행한다. C의 source/copy 경계는 별도로 actual view를 재검산한다. B cursor는 `&view`와 생성 revision을 보유하고, 같은 직렬 statement의 A는 `&view`를 C에 전달해 own mutation을 수행할 수 있다. Cell 때문에 capsule/view는 `!Sync`이며 동시 same-statement 실행 권한을 제공하지 않는다.

finish_statement는 view를 소비한다. 첫 효과 전 거절은 같은 view 반환, 성공은 phase와 원 active sequence/mark를 닫고 RC guard를 `take()`하여 한 번 checked 해제한다. 실패 statement의 CLR/StatementFinish는 아래 consuming rollback 경계이며 일반 성공 종료가 가짜 WAL을 만들지 않는다. view가 살아 있으면 Rust borrow 때문에 transaction을 소비 종료할 수 없다. read-only statement Drop은 snapshot·active sequence의 no-I/O cleanup을 하되 transaction 자체의 Serializable 성공을 의미하지 않는다. mutation pending/미정 view Drop은 원 fence+resolve를 보존한다.

finish_read_only는 원 durable ledger borrow를 유지한 채 active statement0, slotgen/origin 및 Serializable read/catalog set의 `last_q≤start_q`를 검산한다. RC는 동일 origin/종료상태만 검사하며 RC에 Serializable 보증을 붙이지 않는다. 성공 또는 읽기 전용 SerializationConflict는 원 snapshot/slot을 checked 반환한다. 후자는 `TransactionFinishError::Rejected(SerializationConflict)`로 반환하며 commit 성공으로 응답하지 않는다. `RollbackRequired` variant는 actual write가 있는 transaction 종료에만 사용한다. read-write commit conflict는 phase를 MustAbort로 고정한 같은 capsule을 반환하여 A rollback만 허용하고 새 statement/mutation을 금지한다. `finish_aborted`는 actual AbortEnd/terminal 폐쇄를 검산한 경우만 같은 capsule을 소비하고 slot/snapshot을 반환한다. `commit_row`는 active statement0과 actual actor를 검산하고 원 ledger borrow를 유지한 채 validation→Commit stage/durable→whole install→epoch/Q 공개를 수행한다. stage 전 Unchanged는 Before, write conflict는 RollbackRequired, poststage 불확실성은 Fatal이며 slot/pending/fence를 정상 성공으로 닫지 않는다. active transaction Drop은 sync/commit/성공 abort 없이 원 no-I/O cleanup 또는 미해결 write의 fence를 수행한다.

현재 [읽기 전용 수명 부분공급](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)은 위 begin/statement/finish_read_only의 원 slot·512B credit·Serializable/RC snapshot·Before/Drop을 연결했다. writable 옵션은 효과 전 Unsupported이며 finish_aborted/commit_row는 아직 없다. actual source adapter/자동 read-set이 없어 현재 종료 인수는 빈 read-set만 해당한다. 거래·view에서 legacy snapshot guard를 꺼내 조회하는 우회는 공급하지 않는다. short aggregate admission·cursor·쓰기 statement/mark·실제 conflict 인수는 남아 있다.

### 6.3 Row read operation: 원 Owner의 단계별 재대여

다음 type의 필드는 `apply::read` 내부다. `RowCandidate`는 private lease generation+capture sequence와 관측 key/version/length만 가진 non-Copy 값이다. source pin이나 slice를 외부로 내보내지 않으므로 mutable resolver와 자기참조 lifetime을 만들지 않는다.

```rust
pub(crate) struct RowReadOperation<'u,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace> {
    backend: ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>, // 원 borrow만 필드별 복사
    owner: &'u mut Owner<'c,'s>,
    statement: &'r RowStatementView<'t,'v,'a,'c,'s>,
    loader: &'i mut [u8;409_600],
    copy: &'i mut [u8;16_384],
    lease: Option<ReferenceLease<'b,'a,'c,'s>>,
    captured: Option<CapturedRowRefs<'b,'a,'c,'s>>,
    capture_sequence: u64,
    stopped: bool,
    source: OperationSourceAdmission<'b,'a,'c,'s>,
}
pub(crate) struct RowSearchPage<'g,'b,'a,'s,'c,'p,'d> {
    source: ReadTargets<'g,'b,'c,'s,'p,'d,3>,
    lease: &'g mut ReferenceLease<'b,'a,'c,'s>,
    captured: &'g mut Option<CapturedRowRefs<'b,'a,'c,'s>>,
    copy: &'g mut [u8;16_384],
    capture_sequence: &'g mut u64,
    expectation: RecordExpectation,
}
impl RowCandidate {
    pub(in crate::storage) fn row(&self) -> RowId;
    pub(in crate::storage) fn version(&self) -> RecordVersion;
    pub(in crate::storage) fn canonical_len(&self) -> usize;
}
pub(crate) enum ReadStep<T> {
    Ready(T),
    FlushRequired { target: Plan4Target },
}
impl<'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>
{
    pub(in crate::storage) fn row_operation<'u,'r,'t,'v,'i>(
        &self, owner: &'u mut Owner<'c,'s>,
        view: &'r RowStatementView<'t,'v,'a,'c,'s>,
        private: &'i mut [u8;425_984],
    ) -> Result<RowReadOperation<'u,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>, RetentionError>;
}
impl<'u,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    RowReadOperation<'u,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>
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

Catalog read는 위 row operation의 alias로 바꾸지 않는다. 기존 shape/2-reference 검산을 보존한 `CatalogReadOperation<'u,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>`가 같은 original backend/Owner/loader/copy/lease 소유 모양을 갖고, `&'r RowStatementView<'t,'v,'a,'c,'s>`를 보유한다. 실제 entry signature는 다음과 같다.

```rust
impl<'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>
{
    pub(in crate::storage) fn catalog_operation<'u,'r,'t,'v,'i>(
        &self, owner: &'u mut Owner<'c,'s>,
        view: &'r RowStatementView<'t,'v,'a,'c,'s>, private: &'i mut [u8;425_984],
    ) -> Result<CatalogReadOperation<'u,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>, RetentionError>;
}
impl<'u,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W: WriterNamespace>
    CatalogReadOperation<'u,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'m,'w,W>
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

`capture_catalog`만 기존 Catalog fixed-leaf에서 object key를 찾는 작은 source adapter로 남으며, lookup의 snapshot version-visible/retired/Missing 선택은 `catalog::read::lookup`으로 옮긴다. `finish_found`는 refs/lease/Owner borrow를 끝낸 뒤 caller copy만 borrow하는 현재 `CatalogRecordView<'i>`를 반환한다. 이 operation의 copy field는 `Option<&'i mut [u8;16_384]>`로 보유하여 cleanup 성공 뒤 `take()`한다. Drop을 가진 owner의 field를 불법 이동하거나 별도 copy backing을 만들지 않는다. `finish_missing`는 오류/FlushRequired의 checked cleanup에도 쓰며 실제 실패의 cause/effect를 덮어쓰지 않는다. `CatalogCandidate::header(&self) -> Option<CatalogHeader>`는 actual copied header의 read-only 관측이며 version/retired/undo와 object identity를 제품 분기에 제공한다. None은 actual complete leaf lookup의 부재이며 profile/identity 오류가 아니다. raw candidate constructor는 없다. 제품 코드가 현재처럼 `refs.transaction.reference`, `storage.profile`, `read_catalog_undo`, registry를 직접 읽는 것은 금지한다.

현재 [C2 동기 Catalog 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)는 기존 `lookup_catalog`가 실제 `storage::catalog::read::lookup`을 호출하고, private `apply::read::operation`이 original source/ref/load/cleanup을 제공한다. 현재 operation은 실제 `SnapshotGuard` borrow를 보유하며 private factory에서 만들어 제품 호출에 즉시 consuming 전달된다. caller-retained factory는 없고 반환값에는 operation/candidate/guard가 남지 않는다. 위 `RowStatementView` 기반 `catalog_operation` signature·자동 read-set·`finish_table`은 아직 미공급이다. 해당 entry는 원 short aggregate 예약과 실제 겹칠 수 있는 writer entry/progress를 함께 수신한 뒤 공개한다. 현 동기 source census를 전체 동시 admission 증거로 확대하지 않는다.

`CheckedTable<'c,'s>`는 private core/binding/fence origin, 실제 Catalog에서 확인한 ObjectHandle/SchemaEpoch/anchor identity를 가진 non-Copy 값이다. `finish_table`만 actual captured Catalog candidate의 Table 정의·identity를 검사하고 refs를 해제한 뒤 발급한다. raw TableSpec/RecordView decoder나 caller ObjectId로 생성하지 않는다. Catalog 제품은 snapshot-visible 선택 뒤 이 method를 호출하고, Row C는 사용할 때마다 동일 open·schema·table generation·anchor/membership을 재검산한다. 실제 snapshot/source와 분리된 scalar table handle을 pin·write 권한으로 승격하지 않는다. `finish_found`의 일반 Catalog view와 달리 CheckedTable은 원 `'c,'s` 수명 밖으로 나갈 수 없다.

Catalog Commit의 제품 호출은 다음으로 닫는다. `PreparedCatalog`은 현재 `'i,'c,'s`를 유지하며 원 origin·OwnerIdentity·prefix/control cut·exact target stamps·immutable frame/before/after·digest를 보유한다. prepare는 capsule을 공유 borrow하여 원 actor/slotgen을 Prepared의 scalar cut에 기록한다. commit은 capsule을 소비하고 active statement0·같은 cut를 재검산한다.

```rust
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn prepare_catalog_commit<'v,'i>(
        &mut self, owner: &mut Owner<'c,'s>, transaction: &RowTransaction<'v,'a,'c,'s>,
        private: &'i mut [u8;425_984],
    ) -> Result<PreparedCatalog<'i,'c,'s>, RetentionError>;
    pub(in crate::storage) fn commit_catalog<'v,'i>(
        &mut self, owner: &mut Owner<'c,'s>, transaction: RowTransaction<'v,'a,'c,'s>,
        prepared: PreparedCatalog<'i,'c,'s>,
    ) -> Result<semantic_catalog::Observation,
        TransactionFinishError<RowTransaction<'v,'a,'c,'s>>>;
}
// storage::catalog::create의 실제 orchestration; 모든 statement/view는 앞서 종료
let prepared = mutation.prepare_catalog_commit(owner, &transaction, private)?;
let committed = mutation.commit_catalog(owner, transaction, prepared)?;
```

prepare의 첫 효과 전 거절 또는 Prepared 폐기는 mutable borrow/loan을 반환하며 transaction은 caller에 남는다. commit의 stage 전 Unchanged는 Prepared/targets를 checked 정리하고 `Before{transaction}`을 반환하므로 재시도는 같은 capsule로 새 prepare부터 한다. write conflict는 MustAbort capsule을 `RollbackRequired`로 반환한다. stage 후 오류는 원 fence/obligation을 유지하며 healthy retry capsule을 반환하지 않는다. 정상 성공은 epoch/Q 공개 뒤 snapshot/slot/credit을 반환한다. Catalog read/write set도 caller 선택 없이 C의 실제 source/apply에서 등록되므로 Catalog read를 한 read-only Serializable transaction 역시 `finish_read_only` 검증을 거친다.

<a id="catalog-claim-gap"></a>
#### Catalog CREATE의 원 pair claim을 공유 세션에서 수신하는 동결안

현재 [`reserve_catalog_create`](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_live.rs)는 actual actor/Undo 이후 `&mut Plan4Retention`을 빌리고 [`prepare_tx_pair`](../../crates/engine/src/storage/dirty_init/tx_pair.rs)의 `&mut SharedPool`로 shared user를 배제한다. `target_mask` 관측→두 clean slot close/generation 소비→`init_claim` 설치는 여러 registry 구간이며 그 사이의 배제는 이 가변 borrow에서 온다. `return_catalog_pair`는 원 flush의 가변 borrow로 `PairIoQuiescence`를 발급하고 Cancel의 실제 allocator-after를 읽는다. 이 의미를 transaction 전 provisioning으로 바꾸지 않는다.

**선택한 최소안:** 실제 원 `PoolRegistry`의 directory/counts/init_claim/DPT/physical claim/load를 한 leaf에서 검산·변경하는 atomic close를 공급하고, 원 `TxPairClaimKey`가 살아 있는 동안 일반 pin/load/capture 진입을 닫는다. 원 `RetentionFlushControl`은 disjoint mutable lease로 같은 예약에 빌려준다. 새 manager·pool·claim registry·WAL/File·fence·전역 mutex를 만들지 않는다. `&self`로 시그니처만 바꾸는 수정도 아니다. 원 methods의 actual 원자 구간과 권한을 바꾸는 **C3 필수 구현**이다.

| 현재 source·역할 | 실제 목표 method·제한 |
| --- | --- |
| `retention_residency::retention_pair_candidates(&mut self)` | `catalog_pair_candidates(&self, pair, start, existing)`는 actual content/registry 아래 읽은 candidate만 반환. cap/claim이 아니며 최종 close에서 모두 재검산. 기존 mutation activation/load 전체를 공유 API로 열지 않음 |
| `prepare_tx_pair(&mut self)`와 `prepare_retention_pair_slots` | `try_prepare_tx_pair(&self, authority, indices)`가 두 victim content `try_write` 뒤 **한 registry leaf**에서 seven-existing identity·두 clean victims·generation successor·quiescence를 재검산하고 close+claim을 원자 설치. 기존 slot/key를 그대로 사용 |
| `reserve_target_capacity`, `reader_load::select_load`, `dirty_flush::{flush_preflight,capture_flush}` | 같은 registry leaf의 active `InitClaimIdentity::TxPair` 검사로 일반 신규 pin/load/capture를 Busy 거절. resident fast path도 검사. claim이 없을 때의 기존 동작과 System INIT 범위 유지 |
| Reserve/Cancel의 `prepare_existing_catalog`·`apply_existing_catalog_n`, `return_catalog_pair`의 allocator read | 아래 `reserve_tx_pair_existing`으로 같은 key를 검산한 기존-target pin을 사용. 일반 pin gate를 끄거나 borrowed key를 raw bool로 대체하지 않음 |
| `claim_tx_pair_targets`→`TxPairWriteTargets::install_forward` | 원9-target pin 배열·generation·whole page/metadata/DPT/control·joint publication gate 유지. actual source/permit 검산 없이 fresh pair를 기존-page write로 다루지 않음 |
| `finish_tx_pair(&mut self)`·`cancel_tx_pair(&mut self)` | 공유 pool에서 key/원 completion 또는 Cancel permit를 소비하는 checked method. active claim이 모든 새 일반 진입을 막는 동안 검산하고 마지막 registry leaf에서 directory+claim+key를 함께 종료 |
| `RetentionFlushControl`·`PairIoQuiescence` | `CatalogPairIoLease`는 원 flush의 mutable borrow를 보유. 현재 Ready/capture0/known generation과 실제 I/O preflight를 검산하고 Cancel 때 같은 원 `PairIoQuiescence`를 짧게 재대여 |

atomic close는 다음 순서다. 그 전의 candidate 관측은 최종 결정이 아니다.

1. 실제 source/actor/statement와 원 source charge를 검산하고, dirty victim이면 claim/Reserve 효과 전에 반환하여 원 maintenance로 flush한다. source guard/pin을 유지한 채 flush 또는 capacity를 기다리지 않는다.
2. 원 flush lease가 Ready이고 capture/durable/authority/retired 잔여0임을 확인한다. 같은 pool/core/binding/fence와 selected generation이 일치해야 한다. 이미 진행 중인 flush는 끝낸 뒤 다시 준비하며 이를 새 lease로 덮지 않는다.
3. 두 victim의 실제 content를 `try_write`하고 registry를 잡는다. registry 아래 total pins0, load0, flush claim0, init claim0, closed pair0, seven-existing의 실제 identity/generation·중복/role, pair 주소 부재, 두 victim의 clean/DPT/requirement0, 각각의 checked generation successor를 모두 확인한다. 기존 pin 또는 reader load가 있으면 **어떤 directory/generation/claim 변경 전 Busy**다. 상대 reader/loan을 취소하거나 snapshot을 버리지 않는다.
4. 모든 검산 후 동일 registry guard 안에서 directory close와 두 RAM generation 소비, fresh content metadata, 원 `InitClaimSlot::TxPair`를 함께 설치한다. `FencedInitRegistry`와 원 unwind fence를 유지하고 이 구간에는 I/O·상위 lock·allocation·대기·callback이 없다. close됐는데 claim이 없는 상태를 다른 user에게 노출하지 않는다. 마지막으로 authentic `TxPairClaimKey`를 발급한다.
5. active key의 원 claim을 유지한 채 actual Reserve의 기존-target read/write→stage→whole apply를 수행한다. 현재 Catalog의 release→actual wait_durable 규약을 그대로 보존한다. RowReserve의 별도 durability 제안을 이 Catalog 경로에 소급 적용하지 않는다.

일반 pin/load/capture는 claim 검사와 quota/claim 변경을 **같은 registry 잠금**에서 수행한다. 따라서 ordinary 진입이 먼저 이기면 close가 Busy, close가 먼저 이기면 ordinary 진입이 Busy다. scalar target lookup만으로 page 사용 권한이 생기지 않는다. claimed load/capture의 잔여0을 close가 요구하므로 뒤늦은 이전 I/O completion과 경쟁하지 않는다. 기존 read operation은 유한 작업을 checked 종료하고 같은 snapshot으로 재시도할 수 있으며 장기 transaction/view/cursor는 그 사이 살아 있다. fresh snapshot admission을 취소하거나 강제 만료하지 않는다. source capacity의 원 progress admission·기존 finite-reader 정리와 commit의 authentic Pending→Durable→Drain은 §6.5/6.9를 유지한다. pair close를 위해 가짜 StagedCommit/DurableCommit/Drain을 만들지 않는다.

다음은 C 내부 exact pin adapter다. 실제 타입은 원 pool의 자손 `dirty_init::tx_pair`에 두어 private PinSet을 재사용한다. key는 apply의 actual factory만 보유하고 sibling 제품에 노출하지 않는다.

```rust
pub(in crate::storage) struct TxPairExistingTargets<'u,'b,'k,'c,'s,'p,'d,const N: usize> {
    inner: PinnedTargets<'u,'b,'c,'s,'p,'d,N>,
    key: &'k TxPairClaimKey<'c,'s>,
}
pub(in crate::storage) struct TxPairExistingRead<'u,'b,'k,'c,'s,'p,'d,const N: usize> {
    inner: ReadTargets<'u,'b,'c,'s,'p,'d,N>,
    key: &'k TxPairClaimKey<'c,'s>,
}
pub(in crate::storage) struct TxPairExistingWrite<'u,'b,'k,'c,'s,'p,'d,const N: usize> {
    inner: WriteTargets<'u,'b,'c,'s,'p,'d,N>,
    key: &'k TxPairClaimKey<'c,'s>,
}
impl<'c,'s,'p,'d> SharedPool<'c,'s,'p,'d> {
    pub(in crate::storage) fn catalog_pair_candidates(
        &self, pair: &[TargetRequest;2], start: usize, existing: &[TargetRequest;7],
    ) -> Result<[StatusCandidate;2], DirtyError>;
    pub(in crate::storage) fn try_prepare_tx_pair(
        &self, authority: &TxPairAuthority<'_,'c,'s>, indices: [usize;2],
    ) -> Result<TxPairClaimKey<'c,'s>, DirtyError>;
    pub(in crate::storage) fn reserve_tx_pair_existing<'u,'b,'k,const N: usize>(
        &'b self, owner: &'u mut Owner<'c,'s>, key: &'k TxPairClaimKey<'c,'s>,
        requests: &[TargetRequest;N],
    ) -> Result<TxPairExistingTargets<'u,'b,'k,'c,'s,'p,'d,N>, DirtyError>;
    pub(in crate::storage) fn finish_tx_pair(
        &self, key: &mut TxPairClaimKey<'c,'s>, completed: CompletedTxPair<'c,'s>,
    ) -> Result<(), DirtyError>;
    pub(in crate::storage) fn cancel_tx_pair(
        &self, key: &mut TxPairClaimKey<'c,'s>, permit: TxPairCancelPermit<'_,'c,'s,'_>,
    ) -> Result<(), DirtyError>;
}
impl<'u,'b,'k,'c,'s,'p,'d,const N: usize>
    TxPairExistingTargets<'u,'b,'k,'c,'s,'p,'d,N>
{
    pub(in crate::storage) fn try_read(self)
        -> Result<TxPairExistingRead<'u,'b,'k,'c,'s,'p,'d,N>, DirtyError>;
    pub(in crate::storage) fn try_write(self)
        -> Result<TxPairExistingWrite<'u,'b,'k,'c,'s,'p,'d,N>, DirtyError>;
    pub(in crate::storage) fn release_checked(self) -> Result<(), DirtyError>;
}
```

`TxPairCancelPermit`의 네 lifetime은 현재 source의 `'i,'c,'s,'w`다. wrapper의 read/write는 원 bytes/observe/matches_stamp/preflight_dpt/install_page/install_metadata/install_dpt와 release_checked만 같은 내부 adapter에 제공한다. raw inner 또는 key를 반환하지 않는다. `N`은1..7이며 모든 요청은 key가 가진 seven-existing mask의 정확한 부분집합이어야 한다. fresh pair 또는 다른 owner/core/주소를 넣으면 pin quota 변경 전 거절한다. registry의 count는 원 PinSet이 계수하며 이 경로는 `initializer_pins_held`를9-target true로 만들지 않는다. 같은 reservation을 `&mut` 재대여하는 C만 사용하므로 subset pin과 Forward의9-target pin을 동시에 만든다고 가정하지 않는다. typed key borrow는 subset read/write가 남은 동안 key의 mutable finish/cancel을 컴파일에서 막는다. actual close/finish/cancel은 total0도 재검산한다.

public-to-storage 소비 경계는 원 session 전체 대신 disjoint port/flush lease를 갖는다. 이 이름은 기존 `ReservedCatalogCreate`의 실제 책임 이전이며 추가 reservation owner가 아니다.

```rust
struct CatalogPairIoLease<'r,'c,'s,'w> {
    original: &'r mut RetentionFlushControl<'c,'s,'w>,
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    selected_generation: u64,
    pair: [TargetRequest;2],
}
pub(crate) struct ReservedCatalogCreate<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    mutation: &'r mut MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    owner: &'r mut Owner<'c,'s>,
    statement: &'r RowStatementView<'t,'v,'a,'c,'s>,
    io: Option<CatalogPairIoLease<'r,'c,'s,'w>>,
    private: Option<&'i mut [u8;425_984]>,
    definition: &'i crate::api::TableSpec,
    plan: CreatePlan,
    key: TxPairClaimKey<'c,'s>,
    reserve: PageLsn,
    source: OperationSourceAdmission<'b,'a,'c,'s>,
    active: bool,
}
pub(crate) struct CatalogCreated<'i> { object: ObjectHandle, private: &'i mut [u8;425_984] }
pub(crate) struct CatalogCancelled<'i> { private: &'i mut [u8;425_984] }
impl<'i> CatalogCreated<'i> {
    pub(in crate::storage) fn into_parts(self) -> (ObjectHandle, &'i mut [u8;425_984]);
}
impl<'i> CatalogCancelled<'i> {
    pub(in crate::storage) fn into_private(self) -> &'i mut [u8;425_984];
}
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn reserve_catalog_create<'r,'t,'v,'i>(
        &'r mut self, maintenance: &'r mut MaintenancePort<'b,'s,'c,'p,'d,'m,'w,W>,
        owner: &'r mut Owner<'c,'s>, statement: &'r RowStatementView<'t,'v,'a,'c,'s>,
        undo: &Plan4Target, definition: &'i crate::api::TableSpec,
        private: &'i mut [u8;425_984],
    ) -> Result<ReservedCatalogCreate<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, RetentionError>;
}
impl<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    ReservedCatalogCreate<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn initialize(self)
        -> Result<CatalogCreated<'i>, RetentionReservationError<Self>>;
    pub(in crate::storage) fn cancel(self)
        -> Result<CatalogCancelled<'i>, RetentionReservationError<Self>>;
}
```

C는 `maintenance.flush`의 짧은 reborrow만 `CatalogPairIoLease.original`로 이동하며 `&mut MaintenancePort` 전체와 그 내부를 함께 저장하지 않는다. 원 mutation/control과 flush는 `DiskSession::split`의 서로 다른 필드이고 capsule의 snapshot/isolation은 공유 필드다. 원 plan/actor는 statement의 actual transaction에서 얻고 raw TxnRef 인자를 받지 않는다. 같은 open/statement/slotgen·pool·selected generation을 확인한 뒤 Catalog read/write set을 C가 자동 등록한다. schema/leaf 용량 검산은 기존처럼 첫 Reserve 전에 수행한다. 제품 TableSpec와 PRIVATE_BYTES를 복사하지 않는다.

정상 Forward는 key의 원9-target whole PinSet을 쓰고 stage→whole install→release→기존 actual durable 확인→CompletedTxPair 검산 뒤 finish_tx_pair를 호출한다. 마지막 registry 구간에서 `directory.finish_retention_pair`·`init_claim=None`·`key.active=false`를 함께 적용한 후만 flush lease/source charge를 반환한다. 그 뒤 일반 reader는 공개된 pair를 읽을 수 있지만 Catalog의 Transaction stamp는 Commit 전 가시성을 그대로 따른다. 제품은 statement를 종료하고 위 prepare_catalog_commit/commit_catalog로 terminal을 수행한다. 그 terminal의 same-PinSet Pending·Drain과 validation→epoch/Q 공개까지 exclusive ledger 보유는 바뀌지 않는다.

Reserve stage가 실제 Unchanged이면 같은 key로 allocator를 읽고 원 `Unstaged` Cancel permit를 발급해 RAM claim을 checked 반환한다. actual Cancel은 key 경로로 원 existing before를 준비하고 stage/whole apply 및 기존 durable receipt를 검산한 뒤 `Cancelled` permit를 사용한다. allocator-after와 두 fresh pair/DPT/requirement0를 content→registry 순서로 확인하고, 마지막 registry에서 directory return+claim 해제+key inactive를 원자 적용한다. 이 동안 active claim의 ordinary gate와 flush lease가 경쟁을 배제한다. view/transaction의 관측 집합과 소비한 영속 발급값은 되감지 않는다.

예약 이후의 Before는 같은 reservation·key·private·flush lease를 반환하며 외부 maintenance를 새로 호출하려 하지 않는다. 예약 전에 필요한 dirty victim flush는 이미 끝났고 Cancel/Forward는 같은 고정 existing targets와 원 rollback/terminal 몫으로 진행한다. stage 후 오류·cleanup 실패·poison은 원 Fatal/fence/잔여 obligation이다. active Drop은 fence/notify를 먼저 수행하고 claim을 정상 완료한 것처럼 지우지 않으며 no-I/O다. private/borrow의 Rust 해제는 claim 완료의 증거가 아니다. 이 경우 reader와 maintenance가 다시 호출돼도 원 health/fence로 거절하며 recovery가 처리한다.

실제 소비 예는 `T1 Serializable read-only begin→Catalog lookup→read operation만 종료(view/Tx snapshot 유지)→T2 begin/statement→reserve_catalog_create(&mut maintenance,owner2,&view2,...)→initialize→T2 statement 종료/Commit→T1 statement 종료/finish_read_only=Rejected(SerializationConflict)`다. T1의 장기 view가 mutation/flush를 가변 대여하지 않으므로 Rust 수명과 격리 의미가 함께 성립한다. T1이 아직 physical pin을 보유했다면 Reserve 이전 Busy이고 T1이 종료한 뒤 같은 view로 재준비한다. 이를 위해 장기 snapshot을 버리거나 read-only 종료 검증을 생략하지 않는다.

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

1. `MutationPort::commit_catalog`가 Prepared의 origin/core/actor/control cut/prefix와 before를 재검산한다. 원 exclusive ledger borrow 아래 Serializable read/catalog set을 검증하고 epoch slot·changed list·terminal receipt·source charge를 durable 이전에 선예약한 뒤 원 `WriteTargets::prepare_commit`을 호출한다. actual `stage_call_started()`에서 받은 key만 `CatalogCommitAttempt`에 넣는다.
2. `TerminalWal::stage_catalog`가 operation/digest/origin을 검산하고 **동일 runtime의 physical stage를 직접 호출**한다. 그 호출이 실제 `Effect::Unchanged`를 반환한 분기에서만 `UnstagedCommit{core,key}`를 생성한다. 상위 에러 값을 받아 나중에 receipt로 바꾸는 public 함수는 없다. 성공일 때만 exact requirement와 digest를 Binding에 봉인한다.
3. actual requirement를 control pending에 먼저 기록한 뒤 `stage.suspend(staged)`가 원 PinSet을 `PendingCommitTargets<'u,'b,'c,'s,'p,'d,4>`로 이동한다. 같은 배열·generation이며 새 target set을 예약하지 않는다. 여기의 `'u`는 commit 호출 중 Owner borrow다.
4. physical wait가 같은 requirement를 확인한 DurableWal일 때만 DurableCommit 생성. `pending.resume_durable(&durable)`가 original Drain을 취득하고 before를 다시 검산한다.
5. actual apply adapter가 원 before stamp, complete-stage requirement, immutable after, checked successor를 결합한 `InstallGrant<'i,'c,'s>`를 만들고 page→metadata→DPT 및 원 control/history를 모두 설치한다. whole 성공 뒤만 `CatalogCommitInstalled`를 생성한다. Row도 자신의 whole-effect set과 같은 terminal receipt 계약을 사용한다.
6. `complete_catalog`가 core/key/requirement/digest/whole installed 일치를 확인한다. `install.finish(installed)`로 content/version/pins를 먼저 해제한 뒤 동일 exclusive ledger borrow 안에서 선예약한 changed-table/catalog epochs를 갱신하고 `BoundVisibility`가 설치 Q를 공개한다. 공개 전에 새 allocation/실패 가능한 receipt 예약을 하지 않는다. durable/control pending을 정상 완료하고 Observation/ACK를 반환한다. 중간 오류는 원 fatal/fence와 잔여 obligation이며 가짜 Unchanged/ACK 없음.

`InstallGrant<'i,'c,'s>`는 private `kind`를 가진다. `Plan4` variant의 core/stamp/requirement/after_counter/`&'i [u8;PAGE_BYTES]`는 apply actual adapter만 만든다. generic installer에는 `matches_core/identity/generation/before_counter/after_counter/requirement/after/prior_lsn/first_rec_lsn`의 읽기 method만 제공하고 생성자는 제공하지 않는다. 기존 `dirty_frame`의 stamp·exact requirement·DPT 검산을 생략하지 않는다.

첫 이행의 legacy variant는 원 `dirty_allocation::InstallPermit<'i,'c,'s>` **값을 소비 보유**한다. `apply::legacy::from_legacy(proof)`만 기존 sealed proof를 받아 wrapping하며 raw core/stamp/after를 인수로 받지 않는다. legacy producer의 actual factory는 그대로이고 둘 다 같은 installer 관찰 계약을 만족한다. 새 Catalog/Row는 이 경로를 사용하지 않는다. 이 adapter는 scope1의 모든 actual producer가 apply 검산으로 이전한 때 제거하며, 그 이전은 이번 C1–C3 완료 조건이 아니다. 별도 pool/WAL/loan을 만들거나 기존 proof와 새 proof를 동시에 mutable owner로 유지하지 않는다. enum/변환 Result의 추가 source 비용은 Q06에 포함한다. adapter와 generic installer 호출부를 한 단위로 되돌리며, grant만 되돌리고 새 producer의 설치 경로를 남기는 반쪽 rollback은 금지한다.

<a id="row-reservation"></a>
### 6.6 RowReserve→A producer→prepare/apply/cancel의 consuming 경계

RowReserve는 **이미 original System INIT가 완료한 Free Undo container**의 slot0·RootJob·RowId를 예약한다. 새 physical page INIT나 Catalog pair claim을 RowReserve에 복사하지 않는다. actual [`prepare_undo_slot/initialize`](../../crates/engine/src/storage/wal_io/plan4_retention/population.rs)와 [`prepare_tx_pair(&mut SharedPool)`](../../crates/engine/src/storage/dirty_init/tx_pair.rs)의 차이를 유지한다. 전자는 C의 별도 provisioning 효과·예산으로 공급하고 후자는 기존 Catalog CREATE에만 남는다. `&SharedPool`인 MutationPort가 `&mut SharedPool`을 다시 얻는 불법 adapter는 없다.

첫 profile의 Free Undo/Status provisioning은 `DiskSession`의 exclusive 준비 경계에서 실제 System INIT로 완료한 뒤 port/transaction을 발급한다. 활성 capsule/view/port가 있는 동안 새 page provisioning을 끼워 넣지 않는다. 기존 prepared FreeUndo가 없으면 RowReserve는 **첫 Row 효과 전 Capacity**이며 이미 예약된 rollback/terminal을 방해하지 않는다. 현재 첫 extent 한도, actual allocated/free/retained census, 초기 provisioning 수와 별도 비용은 [자원 owner](dev3-resource-bounds.md#row-missing-module-budget)를 따른다. Free root의 재사용은 기존 allocation generation과 새 slot generation 검산을 모두 수행한다. 단순히 빈 page를 fixture에서 삽입하는 공급은 없다.

잘못된 type/null/inline2049/index1은 **Reserve 이전 첫 효과0**으로 거절한다. 이를 위해 actual schema/index/leaf read와 A의 값 검산을 먼저 수행하는 consuming preflight를 둔다. 이는 별도 저장 owner가 아니라 뒤의 ReservedRow와 동일 대여의 사전 상태다.

```rust
pub(crate) struct PreflightRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    mutation: &'r mut MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    owner: &'r mut Owner<'c,'s>,
    statement: &'r RowStatementView<'t,'v,'a,'c,'s>,
    table: &'r CheckedTable<'c,'s>,
    private: Option<&'i mut [u8;425_984]>,
    source_cut: RowPreflightCut,
    source: OperationSourceAdmission<'b,'a,'c,'s>,
}
pub(crate) struct RowValidationLane<'l> {
    schema: &'l [u8;16_384],
    leaf: &'l [u8;16_384],
    candidate: &'l mut [u8;16_384],
    context: RowPreflightContext, // actual index count/schema/generation/free capacity
}
pub(crate) struct RowInputPlan { encoded_len: usize, schema_epoch: SchemaEpoch, digest: [u8;32] }
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn preflight_row<'r,'t,'v,'i>(
        &'r mut self, owner: &'r mut Owner<'c,'s>,
        statement: &'r RowStatementView<'t,'v,'a,'c,'s>,
        table: &'r CheckedTable<'c,'s>, private: &'i mut [u8;425_984],
    ) -> Result<PreflightRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, RetentionError>;
}
impl<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    PreflightRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn validation_lane<'l>(&'l mut self)
        -> Result<RowValidationLane<'l>, RetentionError>;
    pub(in crate::storage) fn reserve(self, input: RowInputPlan)
        -> Result<ReservedRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, RowOwnedError<Self>>;
    pub(in crate::storage) fn finish(self) -> Result<&'i mut [u8;425_984], RetentionError>;
}
impl<'l> RowValidationLane<'l> {
    pub(in crate::storage) fn parts(self)
        -> (RowPreflightContext, &'l [u8;16_384], &'l [u8;16_384], &'l mut [u8;16_384]);
}
impl RowInputPlan {
    pub(in crate::storage) fn new(encoded_len: usize, schema_epoch: SchemaEpoch, digest: [u8;32]) -> Self;
}
// A: 순수 schema/value 검산과 canonical candidate encoding. effect 권한 없음.
pub(in crate::storage) fn validate_insert(values: &[ValueRef<'_>], lane: RowValidationLane<'_>)
    -> Result<RowInputPlan, RowError>;
```

C preflight는 actual source를 read-set에 자동 등록하고 index0/profile·schema·identity를 검산해 private에 복사한 뒤 guards를 반환한다. A는 type/null/길이/leaf 필요 공간을 검산하고 candidate lane에 canonical 값을 기록한다. `reserve(self,input)`는 same open/view/source/schema/leaf capacity와 encoded candidate의 digest/길이·profile을 다시 검산한 후만 actual RowReserve를 실행한다. RowInputPlan은 untrusted 값이며 pin/issuer 권한이 아니다. preflight 오류·명시 finish 또는 효과 전 Drop은 no-I/O로 source admission/loan을 반환하고 WAL/page/issuer 변경0이며 실제 관측한 read-set은 유지한다. C는 A의 schema/value 제품 encoder를 대신하지 않지만 forged candidate를 effect로 승격하지 않도록 canonical/schema 일치를 observer에서 검산한다. Before는 같은 preflight를 반환하고 source가 바뀌면 validation_lane부터 재준비한다. 완결 Reserve 뒤의 origin/issued proof와 검산한 input cut는 ReservedRow로 이동하며 private bytes를 자기참조하지 않는다.

Row reservation은 actual control의 Root/Job/UndoSlot 상태를 나타내는 `RowSlotReservation<'c,'s>`를 하나만 보유한다. 그 private 값은 원 core/binding/fence, actor, RootJob identity/generation, RowId, UndoRef, actual Reserve LSN와 active 여부다. 일반 consumer는 scalar issued identity를 읽을 수 있지만 proof를 생성·복제·다른 actor로 옮길 수 없다. 실제 Root/Job/Undo before 검산→원 WAL Reserve→whole installation을 통과한 C만 생성하며, Cancel/Forward는 같은 값과 원 ledger를 대조한다.

```rust
pub(crate) struct ReservedRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    mutation: &'r mut MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    owner: &'r mut Owner<'c,'s>,
    statement: &'r RowStatementView<'t,'v,'a,'c,'s>,
    table: &'r CheckedTable<'c,'s>,
    private: Option<&'i mut [u8;425_984]>,
    reservation: RowSlotReservation<'c,'s>,
    draft: RowDraftCut, // scalar targets/stamps/prefix/control/input cut; self-reference 없음
    source: OperationSourceAdmission<'b,'a,'c,'s>,
}
pub(crate) struct PreparedRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    reserved: ReservedRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    checked: RowPreparedCut, // frame len/digest·정확 target/stamp/credit·source cut
}
pub(crate) enum RowOwnedError<T> {
    Before { error: RetentionError, owned: T },
    Fatal(RetentionError),
}
pub(crate) struct RowApplied<'i> {
    observation: RowMutationObservation,
    private: &'i mut [u8;425_984],
}
pub(crate) struct RowCancelled<'i> { private: &'i mut [u8;425_984] }
impl<'i> RowApplied<'i> {
    pub(in crate::storage) fn into_parts(self) -> (RowMutationObservation, &'i mut [u8;425_984]);
}
impl<'i> RowCancelled<'i> {
    pub(in crate::storage) fn into_private(self) -> &'i mut [u8;425_984];
}
impl<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    ReservedRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn producer_lane<'l>(&'l mut self)
        -> Result<RowProducerLane<'l>, RetentionError>;
    pub(in crate::storage) fn prepare(self, proposal: RowProposal)
        -> Result<PreparedRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, RowOwnedError<Self>>;
    pub(in crate::storage) fn cancel(self) -> Result<RowCancelled<'i>, RowOwnedError<Self>>;
}
impl<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    PreparedRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn apply(self) -> Result<RowApplied<'i>, RowOwnedError<Self>>;
    pub(in crate::storage) fn reopen(self)
        -> ReservedRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>;
}
```

이 API는 이전 초안의 독립 `MutationPort::prepare_row/apply_row(candidate)`를 **대체**한다. port/Owner를 reservation이 이미 대여했으므로 별도로 다시 대여하지 않는다. prepare는 immutable frame slice를 외부에서 받아 reservation 안에 보관하지 않고, reservation이 소유한 private bytes를 그 자리에서 검산한 **scalar cut**만 Prepared에 저장한다. self-borrow하는 Prepared를 만들지 않는다. `reopen`은 stage가 시작되지 않은 Prepared에서 그 scalar cut만 폐기하고 **같은 예약**을 되돌린다. 재준비를 새로운 RowId/UndoSlot 예약으로 바꾸지 않는다.

<a id="producer-input-api"></a>
#### A producer가 읽는 전체 데이터와 borrowed codec

[소비자 입력 대조](dev3-module-consumers.md#producer-inputs)는 실제 `row_record`·`undo_record`·`semantic_catalog::{Inputs,FrameSpec,next_transaction,next_aging}`의 필수 인수를 확인했다. 다음 getter는 C가 보유한 실제 source의 **값/읽기 borrow**만 제공한다. raw control/registry/owner 또는 권한 생성자를 공개하지 않는다. 모든 context 필드는 private이고 constructor는 apply 자손의 actual source adapter에만 있다.

```rust
pub(crate) struct RowPreflightContext { /* 아래 getter에 대응하는 private scalar cut */ }
impl RowPreflightContext {
    pub(in crate::storage) fn profile(&self) -> Profile;
    pub(in crate::storage) fn namespace(&self) -> Namespace;
    pub(in crate::storage) fn table(&self) -> ObjectHandle;
    pub(in crate::storage) fn schema_epoch(&self) -> SchemaEpoch;
    pub(in crate::storage) fn catalog_expected(&self) -> CatalogExpectation;
    pub(in crate::storage) fn schema_cell(&self) -> (u16,u16); // page 내 offset,length
    pub(in crate::storage) fn leaf_expected(&self) -> TreeExpectation;
    pub(in crate::storage) fn leaf_free_bytes(&self) -> usize;
    pub(in crate::storage) fn canonical_capacity(&self) -> usize; // 이 profile에서는2048
    pub(in crate::storage) fn index_count(&self) -> u16;
    pub(in crate::storage) fn catalog_source(&self) -> (FrameStamp,[u8;32]);
}
pub(crate) enum RowSourceRole {
    Root, RootJob, TerminalJob, Status, Undo, RowLeaf, Allocator, TableAnchor, PreviousUndo,
}
pub(crate) struct RowSourceBinding {
    before_index: u8, identity: PageIdentity, kind: PageKind, level: u8,
    cell: Option<(u16,u16,u16)>, // slot,offset,len은 함께 존재하거나 모두 없음
}
impl RowSourceBinding {
    pub(in crate::storage) fn before_index(&self) -> u8;
    pub(in crate::storage) fn expected(&self) -> (PageIdentity,PageKind,u8);
    pub(in crate::storage) fn cell(&self) -> Option<(u16,u16,u16)>; // slot,offset,len
}
// 순수 관측/후보 분류이며 WAL opcode나 wire tag가 아님
pub(crate) enum RowOperation { Forward, Clr }
pub(crate) enum RowPageRole { Root, RootJob, Status, Undo, RowLeaf }
pub(crate) enum RowProducerInput {
    Insert { canonical_len: usize, canonical_digest: [u8;32] },
    Clr { current_row_len: usize, current_projection: [u8;32],
          rollback_target: Option<UndoRef>, next: Option<UndoLink> },
}
pub(crate) struct RowProducerContext { /* private actual scalar cut + role maps */ }
impl RowProducerContext {
    pub(in crate::storage) fn schema(&self) -> &RowPreflightContext;
    pub(in crate::storage) fn operation(&self) -> RowOperation;
    pub(in crate::storage) fn frame_expected(&self) -> FrameExpectation;
    pub(in crate::storage) fn header(&self) -> WalFrameHeader;
    pub(in crate::storage) fn actor(&self) -> TxnRef;
    pub(in crate::storage) fn status(&self) -> StatusSlotRef;
    pub(in crate::storage) fn row(&self) -> RowId;
    pub(in crate::storage) fn undo(&self) -> UndoLink; // actual issued U+sequence
    pub(in crate::storage) fn statement_sequence(&self) -> Option<u64>;
    pub(in crate::storage) fn planned_mutation(&self) -> u64;
    pub(in crate::storage) fn statement_mark(&self) -> Option<UndoRef>;
    pub(in crate::storage) fn predecessor(&self) -> Option<UndoLink>;
    pub(in crate::storage) fn root_job_ordinal(&self) -> u8;
    pub(in crate::storage) fn terminal_job_ordinal(&self) -> u8;
    pub(in crate::storage) fn before_count(&self) -> u8;
    pub(in crate::storage) fn source(&self, role: RowSourceRole) -> Option<&RowSourceBinding>;
    pub(in crate::storage) fn after_order(&self) -> &[RowPageRole];
    pub(in crate::storage) fn input(&self) -> &RowProducerInput;
}
pub(crate) struct RowControlObservation<'l> {
    jobs: &'l [[u8;1_024];64],
    transaction: Option<&'l [u8;1_536]>,
    aging: Option<&'l [u8;1_536]>,
}
impl<'l> RowControlObservation<'l> {
    pub(in crate::storage) fn jobs(&self) -> &'l [[u8;1_024];64];
    pub(in crate::storage) fn transaction(&self) -> Option<&'l [u8;1_536]>;
    pub(in crate::storage) fn aging(&self) -> Option<&'l [u8;1_536]>;
}
```

`catalog_source`는 완결 actual Catalog leaf census의 stamp/digest이고 index_count는 그 census에서 table identity에 맞춰 계산한다. caller가 숫자0을 넣어 index 없음 증거를 만들 수 없다. schema는 `CatalogExpectation`과 cell 위치/길이로 actual copy에서 검산하고 candidate bytes에서 expected identity를 역으로 얻지 않는다. before_count만큼의 physical image가 유효하고 최대9칸의 나머지는 해석하지 않는다. optional H가 없으면 `source(PreviousUndo)=None`이다. RootJob/TerminalJob가 같은 physical Jobs page이면 같은 before_index와 다른 cell 위치를 사용한다. after_order는 physical identity를 중복 제거한 실제 변경 page 순서이며 최대5개다. A는 이 source role map으로 native/redo를 만들고 C는 후보가 선언한 order/count를 독립 재검산한다.

Insert input은 preflight가 검산한 canonical length/digest를, CLR input은 actual current Row/RootJob의 cell/projection·rollback target·next link를 제공한다. CLR에 과거 사용자 값을 다시 주거나 old statement capsule을 복원하지 않는다. recovery에는 live statement_sequence가 없으므로 None이고 actual descriptor의 planned mutation/head/target을 사용한다. header는 원 stream/next LSN/previous complete/previous transaction LSN의 관측일 뿐 append 권한이 아니다.

`RowControlObservation`은 원 `RetentionLedger::jobs()`와 actual actor/AGING row의 첫1536B를 같은 `'l`로 재대여한다. Reserved/rollback task가 원 ledger를 배타 보유하고 private frame/after와 ledger는 disjoint 필드이므로 한 lane 안에서 immutable descriptor와 mutable output을 함께 빌릴 수 있다. 64KiB Jobs를 복사하거나 별도 ledger를 만들지 않는다. 반환된 descriptor borrow가 끝나기 전에는 prepare/apply로 parent를 소비할 수 없다. A는 이 값으로 multi-root debt/descriptor/inventory/Aging after를 계산하며 C는 lane 종료 뒤 원 source revision/prefix/before/descriptor/Jobs를 모두 다시 검산한다. getter/Result/role map/borrow carrier의 동시 비용도 source census에 포함한다.

공통 pure codec `RowSchemaInput::{Columns(&[Column]),Catalog(TableDefinitionView)}`와 `&[ValueRef]` adapter를 C0 공급에 포함한다. 기존 소유형 함수도 동일 pure 규칙에 위임하고 format·오류·output/tail 보존을 바꾸지 않는다. 이름/값/columns를 heap에 복사하지 않는다. first Missing Undo는 actual Missing/payloadNone 경로에서 columns를 사용하지 않으므로 기존 encode/decode에 `&[]`를 전달하며, CLR의 current Row는 borrowed schema로 검산한다. occupied RowValue schema와 일반3pin은 이번 선행이 아니다.

실제 C0 부분공급의 함수는 [row 표현의 borrowed 입력](dev3-row-format.md#borrowed-schema)에 등록했다. `canonical_row_len_borrowed`·`encode_canonical_row_borrowed`와 canonical/record의 `_with_schema` 진입점은 구현·선택6 검증·독립 코드 검토를 마쳤다. 후속 `plan4_apply.rs`는 위 producer context·getter·consuming lane·untrusted 후보 타입을 구현하고 `wal_io::disk_api → storage::disk`에서 선택 재노출한다. private cell tuple은 세 좌표의 동시 존재를 보장하며 위 두 enum은 A의 Forward/CLR 관측만 분류한다. 실제 context/lane 생성·source 포착·예약/설치 adapter는 미공급이다. 타입 경계의 compiler 결과와 원 판정기 실패는 [별도 부분공급 증거](../evidence/module-independence-survey-20261026.md#c0-producer-types)에 둔다.

producer lane은 [채택한 private 분할](dev3-resource-bounds.md#row-missing-module-budget)의 동일 backing을 짧게 나눈다. C가 원 actual before/proof와 schema를 채운 후 source guards를 해제한 뒤만 A에게 빌려준다. loan을 반환하기 전 reader/version/append/registry latch는0이다.

```rust
pub(crate) struct RowProducerLane<'l> {
    context: RowProducerContext,              // actual issued identity의 값 관측
    control: RowControlObservation<'l>,
    before: &'l [[u8;16_384];9],              // Root/Job/Status/U/leaf와 proof
    schema: &'l [u8;16_384],                  // actual CheckedTable source
    frame: &'l mut [u8;98_304],
    after: &'l mut [[u8;16_384];5],
    candidate: &'l mut [u8;16_384],
}
pub(crate) struct RowProposal {
    operation: RowOperation,
    frame_len: usize,
    after_count: usize,
    after_roles: [RowPageRole;5],
}
impl<'l> RowProducerLane<'l> {
    pub(in crate::storage) fn parts(self)
        -> (RowProducerContext, RowControlObservation<'l>, &'l [[u8;16_384];9], &'l [u8;16_384],
            &'l mut [u8;98_304], &'l mut [[u8;16_384];5], &'l mut [u8;16_384]);
}
impl RowProposal {
    pub(in crate::storage) fn new(operation: RowOperation, frame_len: usize,
        after_count: usize, after_roles: [RowPageRole;5]) -> Self;
}
// A의 실제 기능 본문. write/claim/receipt factory가 아니라 후보 producer다.
pub(in crate::storage) fn produce_insert(
    lane: RowProducerLane<'_>, // candidate의 검산된 canonical 값 + actual issued identity
) -> Result<RowProposal, RowError>;
```

`parts(self)`는 C가 제공한 lane의 같은 수명/서로 겹치지 않는 slice를 sibling A에 넘기는 데이터 접근이다. private field를 직접 읽게 하지 않는다. 두 context는 effect 권한 없는 issued/schema/identity의 읽기 getter를 제공하고, RowInputPlan/RowProposal의 `new`는 **untrusted 후보**만 만든다. raw reservation/pin/install/terminal receipt constructor는 여전히 없다. 전체 getter/role/presence와 producer 입력 대조는 위 동결안 및 [소비자 입력](dev3-module-consumers.md#producer-inputs)으로 연결한다. 이 문서상의 일치를 compiler 완료로 주장하지 않는다. producer가 candidate를 바꿔도 reserve가 봉인한 input digest와 schema를 C가 재검산하므로 다른 값을 승인한 것으로 처리하지 않는다. 상기 private 배치는 frame98304+before/proof147456+after81920+candidate16384+schema16384+unused65536=`425984`다. unused lane은 추가 authority/owner 저장소가 아니다. RowProposal은 untrusted scalar이며 raw 생성 가능한 후보여도 C의 effect 권한이 아니다. actual after-role 순서/identity·native/edge/count/inventory·frame size·before digest/obligation·reserved issue 및 statement revision은 prepare와 stage 직전에 다시 검산한다. source stamp가 바뀌면 Before로 같은 Reserved/Prepared를 반환해 producer_lane 재취득부터 반복한다. stored draft/checked carrier 비용은 private backing과 별개로 source envelope에 포함한다.

A의 실제 호출은 다음과 같다. producer error에도 예약이 남아 있으므로 A는 cancel 또는 현재 statement의 rollback을 명시적으로 수행한다.

```rust
let mut preflight = mutation.preflight_row(owner, statement, table, private)?;
let input = row::insert::validate_insert(values, preflight.validation_lane()?)?;
let mut reserved = preflight.reserve(input)?; // 여기서만 첫 Reserve 효과
let proposal = {
    let lane = reserved.producer_lane()?;
    row::insert::produce_insert(lane)?
}; // frame/after의 짧은 mutable borrow 종료
let prepared = reserved.prepare(proposal)?; // Reserved 전체 이동
let applied = prepared.apply()?;            // 같은 proof/loan, 별도 port 재대여 없음
```

위 `?` 표기는 호출 모양이며 A 제품의 실제 error branch는 `RowOwnedError::Before`에서 반환된 owner를 회수하여 retry/reopen/cancel을 선택해야 한다. `producer_lane` 실패도 `reserved`를 caller가 계속 소유한다. automatic Drop이 성공 Cancel을 대신하지 않는다. stage 전 실제 Unchanged이면 원 pending intent와 targets를 checked 반환한 뒤 같은 wrapper를 반환한다. Reserve의 complete stage/whole apply와 그 예약 소유는 이미 성립했으므로 이 Before는 **새 Forward/Cancel 단계의 Unchanged**이며 Reserve/issuer가 되감겼다는 뜻이 아니다. complete stage/partial install/sync Unknown 이후는 Fatal이며 원 active proof Drop/fence·obligation을 보존하고 정상 retry wrapper를 발급하지 않는다.

apply 성공은 원 Slot/RootJob을 Retained로 전환하고 transaction/aging ledger에 같은 obligation을 인계한 다음 proof를 inactive로 만든다. private는 `take()`로 RowApplied에 한 번 반환하고 mutation/Owner reborrow는 끝난다. Cancel 성공도 actual Cancel의 complete stage/whole apply와 원 claim 반환을 확인한 뒤만 proof를 닫고 private를 돌려준다. RowId/UndoSequence/slot generation/mutation high-water·소비한 WAL은 반환하지 않는다. Reserve/Cancel마다 별도 WAL force를 추가하지 않으며 원 WAL-before-page와 terminal durability 규약을 유지한다. 성공/실패 statement의 read/write set 역시 유지한다. C는 이 흐름에서 INSERT 값 검산·row/Undo/leaf encoding이나 A의 오류 정책을 대신 구현하지 않는다.

### 6.7 live rollback과 C/A recovery handler

앞선 성공 statement 보존과 full abort는 실제 Row head chain으로 수행하며 [소비자 상태표](dev3-module-consumers.md#row-states)를 정본으로 삼는다. C는 원 actor/statement mark와 reserved obligations에서 RollbackStart를 적용하고 다음 currentU를 고른다. A는 C가 actual source에서 획득한 before/key/nextU 문맥으로 **CLR 후보**를 생산한다. 원 target/read pins와2-reference handoff, stage/whole install, head 인계, StatementFinish/AbortEnd는 C가 검산·소비한다.

```rust
pub(crate) enum RollbackScope { Statement, Transaction }
pub(crate) struct RowRollback<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    mutation: &'r mut MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    owner: &'r mut Owner<'c,'s>,
    transaction: &'r RowTransaction<'v,'a,'c,'s>,
    statement: Option<&'r RowStatementView<'t,'v,'a,'c,'s>>,
    private: Option<&'i mut [u8;425_984]>,
    selection: RollbackSelection<'c,'s>, // 원 origin/head/mark/next의 scalar cut
    refs: Option<RollbackRefs<'b,'a,'c,'s>>, // 원 visibility의 source-issued currentU/nextU
    source: OperationSourceAdmission<'b,'a,'c,'s>,
}
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn start_row_rollback<'r,'t,'v,'i>(
        &'r mut self, owner: &'r mut Owner<'c,'s>,
        transaction: &'r RowTransaction<'v,'a,'c,'s>,
        statement: Option<&'r RowStatementView<'t,'v,'a,'c,'s>>,
        scope: RollbackScope, private: &'i mut [u8;425_984],
    ) -> Result<RowRollback<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, RetentionError>;
}
impl<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    RowRollback<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn next<'x>(&'x mut self)
        -> Result<RollbackStep<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, RetentionError>;
    pub(in crate::storage) fn finish(self)
        -> Result<RowRollbackDone<'i>, RowOwnedError<Self>>;
}
pub(crate) enum RollbackStep<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    Candidate(RowRollbackTask<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>),
    FlushRequired { target: Plan4Target },
    Complete,
}
pub(crate) struct RowRollbackTask<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    rollback: &'x mut RowRollback<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    draft: RowDraftCut,
}
pub(crate) struct PreparedRollbackTask<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    task: RowRollbackTask<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    checked: RowPreparedCut,
}
pub(crate) struct RowRollbackDone<'i> { private: &'i mut [u8;425_984], scope: RollbackScope }
impl<'i> RowRollbackDone<'i> {
    pub(in crate::storage) fn into_parts(self) -> (RollbackScope, &'i mut [u8;425_984]);
}
impl<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    RowRollbackTask<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn producer_lane<'l>(&'l mut self)
        -> Result<RowProducerLane<'l>, RetentionError>;
    pub(in crate::storage) fn prepare(self, proposal: RowProposal)
        -> Result<PreparedRollbackTask<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, RowOwnedError<Self>>;
}
impl<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    PreparedRollbackTask<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn apply(self) -> Result<RowMutationObservation, RowOwnedError<Self>>;
    pub(in crate::storage) fn reopen(self)
        -> RowRollbackTask<'x,'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>;
}
pub(in crate::storage) fn produce_clr(lane: RowProducerLane<'_>)
    -> Result<RowProposal, RowError>; // A, same semantic CLR producer for live/recovery
```

`RollbackRefs<'b,'a,'c,'s>`는 원 `ReferenceLease`와 source-issued currentU/nextU pins를 보유하는 private 타입이며 원 BoundVisibility의 `'b`를 사용한다. RowRollbackTask는 `&'x mut RowRollback`을 보유하는 짧은 wrapper다. `producer_lane(&mut self)`→`prepare(self,proposal)`→`apply(self)`는 §6.6과 같은 consuming 형태다. 반환 PreparedRollbackTask는 task 전체와 scalar cut만 보유하며 내부 loan을 자기참조하지 않는다. apply 성공은 actual head를 nextU로 옮겨 같은 RowRollback의 다음 재대여를 허용한다. `finish`는 원 mark/None 도달과 StatementFinish/AbortEnd의 actual receipt를 확인하고 private를 반환한다. 실패 statement는 view phase를 FailedFinished로 만들고 `finish_statement(view)`가 token/RC snapshot을 소비한다. full abort는 transaction phase를 Aborted로 만들고 `finish_aborted(transaction)`이 같은 slot/snapshot을 소비한다. 숫자 mark에 도달한 사실만으로 성공 종료하지 않는다.

Recovery는 `Plan4Recovered`의 원 owner/selected epoch/control/private lane을 사용하며 과거 RowTransaction/statement를 재생성하지 않는다. C는 original `recovery_catalog_step`의 selection/effect 결합을 아래 closed action 경계로 나눈다. actual Catalog/공통 CancelReserved/AbortEnd/aging/final VERIFY/STABILIZE는 C가 진행하고 Row loser의 CLR 후보만 A handler에 넘긴다. C가 A module을 import하는 callback dispatch는 만들지 않는다. 제품 open의 작은 driver가 action을 dispatch한다.

```rust
pub(crate) enum RecoveryAction<'r,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    Progress(Plan4RecoveryProgress),
    FlushRequired { target: Plan4Target },
    RowClr(RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>),
    Ready,
}
pub(crate) struct RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    mutation: MutationPort<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    owner: &'r mut Owner<'c,'s>,
    private: &'r mut [u8;425_984],
    progress: &'r mut RecoveryProgressState,
    selected: RecoveryRowSelection<'c,'s>, // origin/head/next scalar; pin 아님
    refs: Option<RollbackRefs<'r,'a,'c,'s>>,
    source: OperationSourceAdmission<'r,'a,'c,'s>,
}
pub(crate) struct PreparedRecoveryRow<'r,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    task: RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>,
    checked: RowPreparedCut,
}
pub(crate) enum RecoveryOwnedError<T> {
    Before { error: Plan4RecoveryError, owned: T },
    Fatal(Plan4RecoveryError),
}
pub(crate) enum RowRecoveryHandlerError<'r,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    Source { error: Plan4RecoveryError, task: RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W> },
    Producer { error: RowError, task: RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W> },
    Prepare(RecoveryOwnedError<RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>>),
    Apply(RecoveryOwnedError<PreparedRecoveryRow<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>>),
}
impl<'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    Plan4Recovered<'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn next_action<'r>(
        &'r mut self, owner: &'r mut Owner<'c,'s>,
    ) -> Result<RecoveryAction<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>, Plan4RecoveryError>;
}
impl<'r,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn producer_lane<'l>(&'l mut self)
        -> Result<RowProducerLane<'l>, Plan4RecoveryError>;
    pub(in crate::storage) fn prepare(self, proposal: RowProposal)
        -> Result<PreparedRecoveryRow<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>, RecoveryOwnedError<Self>>;
}
impl<'r,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    PreparedRecoveryRow<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn apply(self)
        -> Result<Plan4RecoveryProgress, RecoveryOwnedError<Self>>;
    pub(in crate::storage) fn reopen(self)
        -> RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>;
}
// A의 exact handler entry. 오류는 원 task의 소유권을 보존한다.
pub(in crate::storage) fn handle_recovered_clr<'r,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>(
    task: RecoveryRowTask<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>,
) -> Result<Plan4RecoveryProgress, RowRecoveryHandlerError<'r,'a,'s,'c,'p,'d,'o,'m,'w,W>>;
```

Before/Source/Producer 오류는 같은 task와 원 loan의 재시도 소유를 보존하며 Fatal은 원 fence/잔여 복구 의무를 유지한다. handler는 lane을 짧게 빌려 `produce_clr`를 호출하고 반환 후 task.prepare→prepared.apply만 호출한다. raw writer/selected base/phase mutation, row install receipt 생성, 새 owner 생성은 허용하지 않는다.

next_action은 recovered의 `inner`에서 MutationPort를, 원 recovery private에서 짧은 reborrow를, 별도 progress field에서 원 phase/observation/waiting state를 **disjoint** 대여한다. task가 `&mut Plan4Recovered`와 내부를 borrow한 port를 동시에 보유하지 않는다. `RecoveryProgressState`는 기존 phase/observation/waiting/ready fields를 한 값으로 옮긴 것이며 복사된 두 진행 owner가 아니다. source refs는 selected의 raw 숫자에서 만들지 않고 task의 actual source capture에서 원 visibility에 등록하여 lane 반환 전에 physical guards만 해제한다. refs는 same original port lifetime에 묶이며 scalar selection 자체를 pin으로 취급하지 않는다.

완결 replay/검산은 C가 Row grammar와 before/target/owner/obligation을 소비하고 A handler는 **새 loser CLR 생성**만 맡는다. committed Prune/ReleaseRole/RootFree/TerminalFinish의 actual RowMissing maintenance adapter는 C4에 포함한다. CLR/Abort 뒤 재장애의 새 process는 원 selected binding에서 나머지 chain을 선택하며 이미 적용된 LSN·credit를 재소비하거나 old statement snapshot을 복원하지 않는다.

<a id="row-recovery-policy"></a>
#### C+B 최초 analysis의 no-loser gate

[현재 recover](../../crates/engine/src/storage/wal_io/plan4_recovery.rs)는 원 `preinspect` 뒤 reserve_physical→source 안정화→physical→REBASE→log_open을 수행한다. [`analysis::preinspect`](../../crates/engine/src/storage/wal_io/plan4_recovery/analysis.rs)는 selected base와 완결 WAL을 원 replay bank에 재구성하고 canonical 관계/전체 source·home·Pending을 검산한다. no-loser 정책은 RowClr action을 처음 보았을 때가 아니라 **이 검산이 끝난 직후, AnalysisSeal 발급 직전**에 적용한다.

```rust
pub(crate) enum RowRecoveryPolicy { WithRowClr, RejectInitialLosers }
impl<'a,'m,'w,W: WriterNamespace> Plan4Recovery<'a,'m,'w,W> {
    pub(in crate::storage) fn recover_with_rows(self, policy: RowRecoveryPolicy)
        -> Result<Plan4RecoveryRuntime<'a,'m,'w,W>, Plan4RecoveryFailure<'a,'m,'w,W>>;
}
// private 원 recovery owner의 analysis가 발급; 원 seal을 새로 합성하지 않음
struct RowRecoveryAdmission {
    policy: RowRecoveryPolicy,
    state_digest: [u8;32],
    source_facts_digest: [u8;32],
}
```

public enum은 순수 요청 값이다. actual `recover_with_policy` 내부로 기존 recover body를 모으고 legacy `recover()`는 기존 지원 format/scope만 받는다. 새 Row profile/base/scope가 관측되면 legacy entry는 Analysis/Unsupported로 거절하므로 policy를 생략하는 우회가 없다. C+A driver는 WithRowClr와 actual A handler를, C+B driver는 RejectInitialLosers를 선택한다. C가 A module을 수입하는 callback은 추가하지 않는다.

RejectInitialLosers는 final canonical allocator·원 ReservedInventory 관계와 같은 replay의32 ATT/64 Jobs/status graph 전체를 bounded 순회하여 **모든 ReservedInventory=None, Reserved root0, ATT Active/Aborting0**를 요구한다. RowReserved뿐 아니라 기존 SystemPage-only·CatalogCreate Reserved도 거절한다. head가 None인 Active, next가 None인 Aborting도 해당 상태이므로 허용하지 않는다. canonical committed/aborted의 retained cleanup은 허용한다. home page의 겉보기 상태·caller count·부분 Job 목록으로 검사하지 않는다. source 재구성/관계 검산 전에 Unsupported를 먼저 반환하여 corruption을 가리는 순서도 아니다.

정책 통과 때만 같은 actual AnalysisSeal.state_digest와 SourceFacts의 전체 epoch/header_digest/prefix_digest/lsn/end/adopted_local을 canonical 순서로 해시한 값에 policy를 결합해 private admission을 발급한다. raw count/bool/digest를 consumer가 제출해 발급하는 함수는 없다. 원 pre-effect source 재검산도 유지한다. 이 값은 원 runtime→selected binding→Plan4Recovered로 소비 이동하고 first next_action이 actual analyzed/selected handoff와 동일함을 검산한다. 새 owner·별도 replay bank·추가 file lease가 아니며 SourceFacts 전체 복사본을 추가하지 않는다. source fingerprint와 generation이 다른 재진입에는 재사용할 수 없다.

실패는 기존 `Plan4RecoveryFailure`에 원 acquired owner/loan을 보존하고 `phase=Analysis,cause=Unsupported,effect=CallerMustResolve`다. 이 gate는 physical reserve/REBASE/CancelReserved/AbortEnd보다 앞이므로 observation의 **database_effect=Unchanged, mutations=0, READY=0**을 별도로 요구한다. 원 lease/fence 실패 정책을 유지하며 같은 실패 owner를 WithRowClr로 바꿔 healthy 재시도하지 않는다. WithRowClr 재시도는 새 정상 acquire/analysis다. C+B의 live own-write fixture와 재시작 no-loser fixture는 다른 인수이며 이 gate가 live source 의미를 대체하지 않는다.

정책 enum/admission·bounded census locals는 원 source census에 계수한다. gate 자체는 새 disk wire를 추가하지 않고 기존 allocator/ATT/Jobs/selected provenance를 검사하며, 새로운 Row format dispatch는 C4의 별도 수신이다. 독립 D2 검토를 수신해 채택했으며 실제 gate/legacy 차단/negative process 결과는 NOT_RUN이다. M4 공급과 관련 인수는 남아 있다.

<a id="caller-source-credit"></a>
### 6.8 장기 capsule/view/cursor의 bounded source credit

원 fixed68096+isolation/epoch5760=`73856`과 §6.9의 동시 short source 합≤65536을 유지하면서, 장기 caller handle의 합계49152B를 원 IsolationLedger의 byte credit으로 제한한다. [자원 owner](dev3-resource-bounds.md#row-missing-module-budget)의 metadata/whole 상계를 사용하며 이 수치는 type/source 실측 PASS가 아니다. capsule/view/cursor 각각의 보수 charge에는 그 handle의 생성·반환 carrier와 선택 caller peak를 포함한다. 짧은 operation/Result/오류 temporaries는 기존 component source_census에 항상 계산하고 Result마다 별도 credit을 취득하지 않는다. 해당 operation의 고정 nested source charge는 §6.9에서 한 번 예약한다. 실패 Result를 반환하려고 또 실패 가능한 credit을 예약하는 순환은 없다.

private `CallerCredit<'v,'a,'c,'s>`는 원 isolation mutex·slotgen·charge·active를 가진 RAM 회계 값이다. capsule/view/cursor만 이 값을 보유하며 pin/WAL/statement 권한을 발급하지 않는다. 세 handle class의 첫 설계 charge는 각각512B로 고정하여 `32×1536=49152B`의 전체 envelope를 사용한다. 이 산술은32개 최대 거래의 동시 성공 보장이 아니다. actual 타입과 동시 생성/반환 peak가512B 이하인 것은 C0 layout assertion 및 M4 source census에서 확인해야 한다. 초과하면 compact borrower/layout을 수정하거나 효과 실행 전 D2 resource 계약을 다시 검토하며, 측정하지 않은 값을 PASS로 쓰지 않는다. 여러 cursor는 각512B를 추가 예약하므로 one-cursor-per-transaction 제한을 만들지 않는다. aggregate capacity 부족은 constructor의 첫 효과 전 Capacity다.

capsule 생성은 같은 isolation slot과512B를 함께 선예약한다. statement 생성은 같은 원 slot에512B를 추가하며 snapshot/pin/Row 효과 전에 실패를 반환한다. B cursor constructor는 `view.reserve_cursor_credit()`로512B를 먼저 받고 view borrow와 함께 opaque non-Clone cursor에 넣는다. 원 actual view/slotgen을 검산하지 않은 arbitrary byte-count 요청이나 raw credit constructor는 없다. 같은 view의 여러 cursor도 각자 charge를 가진다. B cursor의 Drop/명시 close는 자신의 credit만 checked 반환하며 transaction slot/다른 snapshot을 해제하지 않는다.

```rust
pub(crate) struct CallerCredit<'v,'a,'c,'s> {
    ledger: &'v Mutex<IsolationLedger<'a,'c,'s>>,
    slot: u8,
    generation: u64,
    charge: u32,
    active: bool,
}
impl<'t,'v,'a,'c,'s> RowStatementView<'t,'v,'a,'c,'s> {
    pub(in crate::storage) fn reserve_cursor_credit(&self)
        -> Result<CallerCredit<'v,'a,'c,'s>, RetentionError>;
}
```

credit은 storage authority가 아니므로512B보다 작은 반환 row value/output을 새 owner로 취급하지 않는다. point/cursor의 반환 결과는 독립 pin/snapshot/transaction 권한을 가지지 않는 값과 caller buffer다. 그 결과와 오류·operation carrier의 실제 동시 크기는 해당 함수 source census에 포함하지만 result마다 registry/credit layer를 추가하지 않는다. cursor clone은 허용하지 않으며 새 cursor를 생성하려면 새 credit을 취득한다.

isolation header는 aggregate-used, slot12..16은 source-owned-bytes를 보유한다. release는 same open/slotgen과 checked subtraction을 검사하는 no-I/O leaf 동작이다. cursor는 view를, view는 transaction을 borrow하므로 transaction을 소비 종료하기 전 cursor/view가 모두 끝나야 한다. 정상 종료는 capsule의 마지막512B를 반환한 뒤 source-owned0에서만 slot을 Free로 만들고 slotgen을 갱신한다. 별도 Closing 상태나 종료 후 남은 result lease는 없다. active Drop과 poison/underflow는 원 cleanup/fence 정책을 유지하며 존재하지 않는 capacity를 만들어내지 않는다.

<a id="operation-source-admission"></a>
### 6.9 겹치는 writer/reader의 short source admission

[자원 owner](dev3-resource-bounds.md#row-missing-module-budget)의65536B는 **shared retained 한 번 + 동시에 살아 있는 모든 operation의 nested charge 합**이다. `max(writer,reader)`로 병렬 전체를 대신하지 않는다. 각 class의 charge는 구현 source의 fixed `source_census` 표현식으로 산출하여 entry에서 고정하고 실행 도중 늘리지 않는다. 표현식에는 wrapper/내부 helper/Result·오류/return carrier/선택 caller의 동시값을 합하고, 실제 배타 phase에만 max를 사용한다. 이 값은 M4에 실제 타입으로 검산할 계산 결과이며 그때 설계 예산을 새로 정하는 항목이 아니다. `shared_retained + charge > 65536`인 class는 빌드 인수 실패다.

private `OperationSourceAdmission<'b,'a,'c,'s>`는 원 `&'b Mutex<IsolationLedger<'a,'c,'s>>`, fixed class/charge와 active flag만 보유한다. C public entry의 첫 source guard/pin/effect 전에 원 progress admission과 함께 예약한다. RowReadOperation/CatalogReadOperation은 operation부터 checked finish까지, PreflightRow→ReservedRow→PreparedRow는 같은 한 예약을 이동하여 성공/취소까지, RowRollback/RecoveryRowTask는 선택된 완료 단위까지 보유한다. Before가 wrapper를 반환하면 같은 admission도 남으며 retry 때 중복 청구하지 않는다. 메서드 내부 helper는 상위의 nested charge 안에서 실행하고 새 예약을 요구하지 않는다. typed wrapper를 caller가 오래 보유하면 source 용량도 계속 점유하므로 합계를 벗어나는 재진입은 첫 효과 전에 Busy/Capacity다. primitive Result 반환 함수는 고정 entry/exit carrier 비용까지 예약한 뒤 출구에서 반환한다. 독립 capability가 없는 Result마다 generic credit을 만들지 않는다.

원 isolation header의24..32는 short-used u64로 고정하고32..64는 zero를 유지한다. shared-retained와 class별 nested charge는 source_census의 고정 상수이며 progress 몫은 원 admission phase에 연결한다. 나머지 header와 long source-owned 회계는 [격리 owner](dev3-task-10-concurrency.md#module-row-isolation)를 따른다. 바깥에 새 semaphore/pool/manager를 두지 않으며 원 mutex poison/checked subtraction/fence 정책을 사용한다. 공개 함수의 부족 분기는 source guard/pin/lock0인 상태에서 반환한다. 읽기 guard나 reference를 붙잡고 source capacity를 기다리지 않는다.

rollback/terminal은 원 progress admission이 새 foreground를 닫고 이미 실행 중인 유한 read operation이 checked 반환한 뒤 reserved source 몫으로 들어간다. 그 몫은 `65536-shared_retained` 안의 해당 progress class nested charge이며 foreground와 겹치지 않는다는 actual admission 증거가 있을 때만 max로 계산한다. 새 선택적 foreground 예약 성공을 rollback/terminal의 선행으로 삼지 않는다. 장기 snapshot/cursor를 강제 종료하여 공간을 만들지 않고, flush의 별도65536B와 원 content Drain 순서를 유지한다. Reserved/Before wrapper를 보유한 writer는 같은 원 exclusive control을 점유하므로 다른 writer progress와 공존한다고 계산하지 않는다. caller의 무한 미반환은 진행 보장이 아니라 보유 소유이며 Drop/fence 규약을 따른다.

실제 부분공급으로 [Catalog 조회 수명/Commit Drain](../evidence/module-independence-survey-20261026.md#c2-read-operation-lifetime)을 연결했다. 원 OwnerSlot의 별도 read 표시를 첫 reference 예약 전부터 checked 정리 시도 뒤까지 유지하고 Drain이 표시0을 기다린다. Pending LOAD는 허용하고 Draining의 신규 operation/content 거절은 유지한다. 정상 refs0과 fence 뒤 원 fatal refs 보존을 구분한다. byte admission·동시 합·writer phase/progress 몫은 아직 미공급이며 이 표시만으로 본 절의 인수를 완료하지 않는다.

2026-10-27 [CatalogFull short source 부분공급](../evidence/module-independence-survey-20261026.md#c2-short-source)은 원 isolation header24..32에 고정 source charge를 예약한다. admission 오류 우선순위의 무효과 precheck 뒤 source를 예약하고, 실제 progress 진입에서 다시 확인하며 reference/pin은 그 뒤다. refs/progress의 기존 정리 뒤에도 source를 마지막 decode/Result까지 보유한다. 같은 class의 추가 호출이 부족하면 Capacity/Unchanged이며 이미 보유한 snapshot은 유지한다. poison/fence/underflow는 원 isolation 규약대로 source 잔여를 보존하고 성공 반환으로 표시하지 않는다. 이 한정 범위의 선택17개는 PASS이며 독립 D1/D2를 한정 수용했다. writer·다른 ReadPort 입구·resident-only class·rollback/terminal progress 몫은 미공급이다. 따라서 전체 source aggregate 보장·caller-retained factory·MBC10 V09/V10은 여전히 미완료다.

2026-10-27 [상주 Catalog 부분공급](../evidence/module-independence-survey-20261026.md#c2-resident-read)은 같은 Catalog 본문을 고정 CatalogResident class로 실행한다. 기존 loader loan의 allocator copy와 원 target-set guard를 사용하고 Status inventory 전체 및 마지막 allocator stamp를 확인한다. 비상주는 checked 정리 뒤 Busy, exact 선택 뒤 membership·typed corruption은 Fatal이며 같은 operation의 Full 확대는 없다. source16860B와 shared22256B로 두 상주 조회의 합55976B, 선택9개 PASS를 확인했다. Full41576B와 과거 결과는 유지하며 writer/progress·다른 입구 전체 합산과 caller-retained factory는 여전히 미공급이다. 독립 D1/D2를 이 부분공급에 한정 수용했으며 필수 수정0이다.

후속 [최종 source 반환까지 progress 유지](../evidence/module-independence-survey-20261026.md#c2-source-return)는 위 CatalogFull 공급 당시 정리 순서 중 progress 반환 시점만 늦춘다. refs 정리와 최종 decode/Result 뒤 source를 반환하고 그 다음 원 read_operation 표시를 반환한다. source 실패에도 progress 정리를 시도하며 fence 아래 charge를 보존한다. 선택7개를 검증했지만 source-before-progress 진입 틈·일반 반환 carrier·writer/다른 입구 합산은 남아 있으므로 전체 progress 비중첩 인수가 아니다.

[Commit phase 관측](../evidence/module-independence-survey-20261026.md#c2-commit-phases)은 cfg(test)만 추가하여 현재 Pending의 실제 세 값 필요 하한2984B와 shared+Full 합66816B를 확인했다. 다른 phase 값은 보수 상한이며 최소 필요량과 구별한다. 기존 제품 charge·admission은 유지하고 stage 뒤 준비 배열 수명을 먼저 줄인다. 이 관측1 PASS는 actual Commit·전체 progress 인수가 아니다.

후속 [Pending image 수명 분리](../evidence/module-independence-survey-20261026.md#c2-pending-image)는 stage가 준비 영수증을 소비하고 준비 자료 중568B immutable image만 남긴다. 원 권한·canonical after 순서와 공통 control 검산을 유지하며 선택12 PASS다. Pending wait+Resident2의 보수 합65096B와 stage 반환 상한 증가를 함께 보존한다. 이것은 전체 writer 고정 charge나 Full 동시 허용의 증명이 아니며 본 절의 aggregate·필수 progress 공급은 계속 미완료다.

[source 진입 수명 수정](../evidence/module-independence-survey-20261026.md#c2-source-entry)은 앞선 source-first 부분공급의 순서를 무효과 precheck→원 progress 진입→고정 source 예약→refs로 대체한다. source 부족의 정상 출구에서는 progress0/refs0/pin0과 Capacity/Unchanged를 보존하되 정리 Fatal은 우선한다. core 잠금과 source ledger 잠금은 중첩하지 않고 quota를 기다리지 않는다. Drain이 먼저면 source0 거절, reader가 먼저면 예약 시도도 flag1로 기다린다. source→progress 종료와 함께 healthy source 수명을 덮으며 선택5 PASS다. class/type/census는 그대로이고 예약 전 entry/일반 반환 carrier 및 모든 writer/read/progress 합산 인수는 여전히 미완료다.

후속 [Aging/Root source 계산](../evidence/module-independence-survey-20261026.md#c2-aging-source)은 실제 순차 호출·initial/malformed 경로와 반환값을 보존하여 Full charge를37724B로 계산한다. Resident16860B·shared22256B와 live/recovered 전체 peak는 유지한다. source 관련3 PASS 뒤 import-only compile을 분리 기록했다. shared+Full+Pending wait는69100B로 아직3564B 초과하며 전체 admission은 미완료다.

[WAL wait phase 관측](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source)은 cfg(test)에서 물리 호출·semantic 반환·caller Result 변환의 수명을 구별한다. Pending5320B와 shared+Full 합65300B를 관측했지만 기존 생산 상계/charge를 변경하지 않았고 신규 ticket/progress·entry/exit 전체 공급 전이다. 따라서236B 여유를 실제 Full/Pending admission 증거로 올리지 않는다.

[실제 Drained 소비 경계](../evidence/module-independence-survey-20261026.md#c2-drained-handoff)는 authentic durable Drain 완료와 content guard 재구성을 분리한다. 원 Installing/version/pins를 private Drained가 유지하며 normal unfinished Drop은 installation abandon/fence 뒤 pins를 반환한다. 신규4·관련4·source2 PASS, 새 반환값의 비용 증가를 보존한다. source 예약은 아직 연결하지 않았고, 이미 gate/pins를 가진 이 경계에서는 후속 reserved source의 무대기 checked 전환만 가능하다.

#### 선택 Catalog caller의 source 수명

[C2 CallScope 부분공급](../evidence/module-independence-survey-20261026.md#c2-call-scope)은 원 Owner/backend/snapshot borrow와 기존 progress/source를 private non-Clone `CatalogCallScope`에 보유한다. `ReadPort::catalog_call`과 `catalog_resident_call`은 같은 원 admission을 한 번 취득한다. `scope.lookup<'call>(&'call mut self, object, cursor, private: &'call mut [u8;425984]) -> Result<CatalogLookupStep<'call>>`의 RecordView·복사한 view·definition/name/column은 같은 scope borrow에 묶인다. `finish_checked(self)` 뒤 이 borrow를 사용하는 코드는 컴파일 거절되며, 선택 caller는 마지막 결과 소비 뒤에만 scope를 닫는다. 순차 lookup은 같은 고정 class를 재사용하며 escalation은 없다.

Operation은 scope를 재대여하고 원 refs/lease/loader/copy만 소유한다. refs 정리→최종 decode/typed Result→선택 caller 소비→scope source 반환→progress 반환 순서를 유지한다. 정상 오류 출구도 `finish_error(error)`로 명시 완료하여 원 Fatal을 보존하고 성공/Unchanged보다 cleanup Fatal을 우선한다. Drop은 Operation refs와 Scope source/progress를 각각 정리하며, source 실패에도 progress 반환을 실행한다. 원 `lookup_catalog`/`lookup_catalog_resident`는 같은 제품 본문을 사용하되 내부 wide 결과를 `finish_result`로 정리한 뒤 기존 private-buffer 수명으로 반환하는 호환 entry다. **legacy의 반환 이후 caller 소비는 새 scope 보장 밖**이며 해당 경로를 전체 admission 완료로 승격하지 않는다.

선택 caller는 결과의 유한 header/Table/Column 검사를 마친 뒤 닫고 FlushRequired의 실제 flush는 그 뒤 수행한다. 이 소비 phase는5256B이며 operation 호출과 순차라 고정 class의 max에 포함한다. 현재 Full38076B·Resident17212B·shared22256B, shared+Resident2=56680B다. 기존 Pending wait 관측5320B까지 더한 Full 합65652B는 이미116B 초과하며 신규 writer ticket/entry-tail 비용도 미공급이다. 이전65300B 관측은 원 source의 값으로 보존한다.

원 Owner의 최대32 foreground+maintenance1은 같은 core에 binding된 진입만 제한한다. 타 core의 foreign Owner 입력·거절을33개로 셌다고 주장하지 않으며, source 선예약 전 경로와 해제 후 Scope/Result 반환 tail의 전체 상계는 후속 공급이다. caller가 권한 없는 scalar/데이터를 따로 복사한 이후나 임의로 많은 view를 복제하는 코드를 이 유한 선택 caller 검산으로 보장하지 않는다. 이 한정은 source cap 확대·새 registry·generic Result credit 도입을 허용하지 않는다.

[budget 행의 실제 borrow](../evidence/module-independence-survey-20261026.md#c2-borrowed-budget-rows)는 descriptor 검산의5개 행 복사와 values의 행 복사를 제거했다. 검산 순서·출력은 유지하고 descriptor source8047B·Full36972B로 감소했으며 Resident17212B/shared22256B는 유지한다. 기존 Pending5320B 관측까지의 합64548B는988B 여유이나 실제 writer ticket·bound entry/exit가 빠져 있으므로 전체 admission 증거가 아니다. CallScope의 이전65652B 및 그 이전65300B는 각각 원 source 결과로 보존한다.

[readonly replay source 분리](../evidence/module-independence-survey-20261026.md#c2-readonly-replay-source)는 원 allocation 검산의 streaming membership·descriptor decode·expected 생성 phase를 따로 세고 실제 생산자64행 배열을 replay에 중첩하지 않는다. INIT context의 `analyze→reservation`와 실제 후속 검산/Status/FreeUndo codec도 구별한다. live producer 상계·원 byte/검산 의미는 유지한다. 현재 Full35895B·Resident17212B·shared22256B이며 Pending 관측을 더한63471B는2065B 여유지만 실제 entry/tail floor·writer ticket이 빠져 있어 전체 admission 증거가 아니다.

[작은 Catalog 진입/정리 경계](../evidence/module-independence-survey-20261026.md#c2-compact-call-boundary)는 actual health/source leaf의 작은 오류와 CatalogCallError를 사용한다. 원 poison→fence·즉시 WAL notify·checked charge/active 잔존과 legacy phase/cause/effect를 유지한다. Scope의 원 Owner borrow·ticket core/identity는 바꾸지 않고 field Drop 및 consuming `(source,progress)`의 순서를 보존한다. Scope/ResultScope72B·오류3B·pair48B를 실측했으며 one-call close/legacy wide 변환 식도 따로 계수한다. Full35619B·Resident16936B·shared22256B, 원 Pending 관측까지63195B이나 전체 bound 진입의 대기/반환과 독립 guard leaf·writer ticket이 빠져 있어2341B 여유를 admission 완료로 보지 않는다. entry1004B는 한 호출 식이며33개 전체 floor가 아니다. 선택 query의 작은 begin/finish와 wide lookup·legacy 변환의 범위를 구별한다.

전체 설계 상계는 metadata `73856+65536+49152=188544≤196608`, original flush를 더한 whole `254080≤262144`다. 이 admission·동시 합 검산은 **새 C1/C2/C3 공급**이며 기존 active.max 기반 source 결과로 이미 구현됐다고 하지 않는다. header의 실제 byte 배치와 source charge의 측정·실패/정리 경로는 C1/C4 인수에 포함한다.

<a id="common-deliveries"></a>
## 7. 유한한 공통 공급과 C0/C 인계

C는 공통 경계 구현 담당의 역할명이며 새 runtime owner가 아니다. 다음 이행 단위와 §9의 유한 selector는 독립 D2로 채택한 공급 계약이다. 실제 부분공급과 실행 범위는 아래 증거를 따르며, 결과를 수신하지 않은 의무는 NOT_RUN이다.

| 단위 | 현재→목표·공급 동작 | 직접 선행·실제 수신자 | 제외·되돌림 단위 |
| --- | --- | --- | --- |
| C0 컴파일 계약 구현·배정 | M2에서 동결한 §4–6의 exact type/lifetime/visibility·producer getter/borrowed schema-value codec와 Row grammar/owner 의미를 구현. A/B 전용 본문/fixture·효과 없는 최소 대역 제공 | M2 동결안 수용·D2; A/B C0 구현 착수 | runtime 효과·durability 증거를 제공하지 않음. 계약 변경은 영향을 받는 A/B 행만 재검토 |
| C1 원 정상 owner·물리 효과 | 원 runtime/초기화 조상 유지; actual WAL impl을 physical로 이전하여 Catalog/Row import0. 실제 Plan4Retention 소유를 apply::DiskSession으로 이전하고 원 Plan4Pages/control/history/flush/visibility를 이동. selected recovery는 새 consuming 전환 수신. 원 control 분할·5760B disjoint backing·49152 long/source aggregate admission 수신 | C0, 기존 Catalog 및 plan4 recovery | scope1 전체 이동 제외. 새 owner 정의·원 consuming 전환·selected READY 반환이 한 되돌림 단위. alias만으로 완료 불가; 새 wire 없음 확인 |
| C2 original read/visibility | 실제 Catalog lookup의 visible/결과 본문을 storage::catalog::read로 옮겨 제한 operation 소비. original source/load/capture/resolve는 apply::read. RowReadOperation의 Owner 재대여·Row2pin/statement view 공급. active Catalog pair claim의 Busy/같은 snapshot 재시도 수신 | C1, §6.2–6.4와 Q03; B와 기존 Catalog | B의 point/next/reseek·출력 본문 제외. Catalog/B의 deep import·내부 field 접근0. actual lookup 수신·checked cleanup·원 cap 회계가 한 되돌림 단위 |
| C3 apply/terminal 경계 | actual Prepared/InstallGrant/terminal receipts를 apply private root와 checked 자손 producer로 이전. Catalog actual prepare→stage→same pins→durable→whole install/Q를 MutationPort로 수신. Row observer/adapter·효과 없는 preflight·actual Begin/Reserve·consuming private/claim·Serializable/RC 종료 수신. Catalog CREATE claim 공존은 [원 registry atomic close/key gate/flush lease](#catalog-claim-gap)를 실제 공급 | C1, 동결한 §6.5–6.6·Q04; A/Catalog 및 B live fixture | A의 schema/value→row/Undo/CLR 후보 본문 제외. 새 Catalog/A/B의 deep import·legacy grant 경유0. actual proof producer+generic installer+제품 호출이 한 단위 |
| C4 Row 의미의 실제 복구 수신 | 실제 source before/target/obligation 검산, Row frame replay/CLR target·잔여 예약·READY 인계. §6.7 action/task로 A CLR handler와 C의 CancelReserved/AbortEnd/committed maintenance를 분리하고 최초 canonical analysis policy gate를 공급. 독립 authored row corpus를 원 owner에 적용하는 제한 경로 | C3 및 Q01/Q04/Q05/Q06; A crash/reopen, B 단독 fixture | 일반 Row UPDATE/DELETE/SMO 및 전체 복구 재설계 제외. 형식이 바뀌면 fresh-fixture/현재-build 복구가 한 단위; 무검증 과거 binary downgrade 없음 |
| C5 합류 adapter | 원 session의 A Commit 결과를 B가 같은/새 snapshot과 새 process recovery로 읽는 작은 조립 | 필요한 C1–C4, A/B 국소 구현 | 두 기능 본문을 통합 담당에게 이동하지 않음. 조립·fixture·해당 diff의 인수만 포함 |

현재 C0는 [borrowed codec](../evidence/module-independence-survey-20261026.md#c0-borrowed-codec)과 [생산자 관측·대여 타입](../evidence/module-independence-survey-20261026.md#c0-producer-types)을 부분공급했다. C1의 [물리 WAL 구현 분리](../evidence/module-independence-survey-20261026.md#c1-physical-wal)에 이어 [원 owner/port 이전](../evidence/module-independence-survey-20261026.md#c1-owner-ports)의 컴파일4건·신규 메모리2건을 한정 수용했다. [control/isolation 소유·4160B 분리](../evidence/module-independence-survey-20261026.md#c1-isolation-backing)는 컴파일3·변경/신규 runtime4로 한정 수용했다. [epoch1600B 원 소유](../evidence/module-independence-survey-20261026.md#c1-epoch-backing)는 compiler3·변경 runtime3 PASS·독립 D1/D2 한정 수용이다. [Catalog epoch 활성화](../evidence/module-independence-survey-20261026.md#c1-epoch-activation)는 실제 seeding·CREATE/Commit을 연결하고 source별 선택 결과와 원 FAIL을 보존했다. [읽기 전용 거래/statement 수명](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)은 별도 부분공급했다. [C2 동기 Catalog 제품 본문 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)는 실제 source adapter와 기존 소비 호출을 연결하고 source별 compiler·runtime7을 검증했다. statement factory/자동 read-set은 아직 없다. 쓰기 거래·자동 read-set·short admission·terminal 이전·성공하는 빈 Commit과 전체 복구 handoff 인수는 남아 있으며 전체 C0·C1–C5 공급과 제품 인수는 미완료다. C0는 M2 문서 채택만으로 완료되지 않는다. M3 수용 뒤 M4에서 최소 컴파일 경계가 실제 제공된 범위부터 A/B가 착수할 수 있다. C0의 계약 대역은 canonical row/schema 입력, finite logical Status/Undo 응답, explicit Busy/Capacity/Unsupported 같은 효과 없는 선택을 반환할 수 있다. 실제 `WalRequirement`, pin, snapshot/READY, install receipt, file effects를 제조할 수 없다. 대역으로 시험한 결과는 순수 기능 범위의 결과이며 C 수신 전 실제 저장·가시성·복구 성공으로 승격하지 않는다.

C의 미구현은 M2 종료를 무조건 막지 않는다. 그러나 필수 API·의미·담당·완료 인수가 미정이면 M2를 닫지 않는다. M4 전체 완료를 모든 A/B 구현의 공통 선행으로 두지 않고, C2를 받은 B와 C3를 받은 A의 독립 부분을 진행할 수 있게 배정한다.

<a id="consumers"></a>
## 8. 기존 Catalog·A·B의 실제 수신 계약

| 소비자 | 전용 제품 본문 | 받는 공통 공급 | 상대 작업 없이 가능한 범위·실제 통합 의무 |
| --- | --- | --- | --- |
| 기존 Catalog | current CREATE/lookup 의미와 pure semantic_catalog 유지. 현재 deep actual adapter는 apply로, 제품 본문은 storage::catalog로 실제 이전하여 제한 API를 호출 | C1 원 session, C2 read/ref/lookup, C3 stage/install/terminal, 해당 recovery adapter | 기존 complete-stage·durable·whole install·Q·snapshot 및 fresh/recovered owner를 실제로 통과해야 C의 backward 수신 증거가 됨. re-export 컴파일만으로 끝내지 않음 |
| A first Row INSERT | schema/value 검산→새 RowId 요청·row key/record/leaf edit·Missing Undo 후보→statement 성공/실패·rollback/full abort 제품 흐름. 계획 `storage::row::insert` | C0 후보/오류 계약, C3 원 mutation/terminal, C4 replay/CLR·READY, C2의 필요한 검증된 table/read | B의 cursor 본문·fixture를 선행으로 요구하지 않음. pure producer/경계 오류는 C0부터, 실제 INSERT/Commit·실패 statement·abort·reopen은 C 수신 뒤 |
| B snapshot point-read/cursor | key point/next/reseek, 실제 version·snapshot에 따른 선택, caller output/cursor 수명. 계획 `storage::row::read` | C0 read 계약, C2 original source→pin→Status/Undo/load, C4 실제 source fixture | A producer를 정답 생성기나 유일 fixture producer로 쓰지 않음. independently authored complete WAL/page corpus를 C4의 실제 owner 검산·replay로 복원. A와는 마지막 실제 합류에서 연결 |

B fixture는 checksum/grammar decode만으로 live 권한을 얻지 않는다. C가 untrusted frame의 actual before·target·owner·obligation을 검산하고 원 선택/replay 또는 actual stage 경계에서만 권한을 발급한다. A는 같은 grammar를 생산하는 실제 기능을 별도로 구현한다. 독립 corpus와 actual C 연결은 [채택한 fixture 계약](dev3-module-consumers.md#independent-fixtures)에 구체화했다. 구현에서 그 연결이 성립하지 않으면 원 결과를 보존하고 영향받는 M2 계약을 다시 검토한다.

합류의 최소 관측 후보는 같은 기준에서 A가 별도 DML 거래로 INSERT/Commit한 row를 B가 새 snapshot에서 읽고, Commit 전 snapshot에서는 올바른 Missing/이전 값을 관측하는 것이다. 별도 process의 현재-build recovery 후 같은 RowId/value·Status/Undo 의무를 대조한다. cursor·point 두 경로, statement 실패·full abort, stale table/schema/generation, cancellation/cleanup의 정확 사례 수는 Q07에서 고정한다. root가 두 제품 기능의 전용 source·시작 기준·대기 원인·merge/의미 충돌을 기록한다.

<a id="validation"></a>
## 9. 변경에 대응할 검증·자원·복구 의무

[사용자 시험 범위](../../AGENTS.md)대로 마지막 검증 이후 실제 diff와 선택 case의 관계를 실행 전에 고정한다. 구조 전환·단계 종료·HEAD 변경만으로 완료한 무변경 시험이나 전체 wrapper를 반복하지 않는다. 과거 결과를 새 source 실행의 PASS로 기록하지 않는다. 문서 verifier는 이 문서 편집 단계에 실행하지 않는다.

다음은 **신규 future selector 동결안**이며 모든 실행 상태는 NOT_RUN이다. compiler case는 계획 `crates/engine/tests/module_boundaries_compile/`의 `<ID>.rs`, memory case는 계획 `apply::tests::module_boundary::<ID의 snake_case>`, native case는 계획 leaf `module-boundaries/<ID>`로 식별한다. 아직 없는 파일·CLI를 실행했다고 하지 않으며 build inventory는 구현 때만 등록한다. A/B 고유 사례와 동일한 실행은 이 표에서 링크로 수신하고 중복 실행하지 않는다.

| ID·정확 분모 | 고정 입력·fault cut | oracle·수신 조건 |
| --- | --- | --- |
| MBC00-P01..05, compiler 성공5 | P01 preflight→validation lane종료→reserve→producer lane종료→prepare→apply→statement종료→transaction종료. P02 Before에서 같은 wrapper회수→reopen→cancel. P03 recovered의 disjoint inner/private/progress borrow→A handler→next_action 재호출. P04 장기 T1 view 유지→T2 Catalog reserve/initialize/Commit→T1 읽기 전용 conflict 종료. P05 producer parts의 control/schema/before immutable borrow와 frame/after/candidate mutable borrow→lane 종료→prepare | 최소 concrete memory W/File와 실제 lifetime 타입이 컴파일. 대역 효과0. P01의 invalid type/null/len/index는 효과 없는 분기로 선택 |
| MBC00-N01..14, compiler 거절14 | N01 sibling raw runtime field 접근,N02 InstallGrant raw 구성,N03 bytes/LSN→terminal receipt 생성,N04 Reserved/credit Clone,N05 session보다 오래 port 보관,N06 page guard 생존 중 같은 Owner 재대여,N07 cursor 생존 중 view 소비,N08 view 생존 중 transaction 소비,N09 validation/producer lane 생존 중 parent 소비,N10 Prepared 생존 중 private 수정,N11 consumed recovery task 재사용,N12 같은 &view를 Sync 요구 경계로 공유,N13 Catalog reservation의 flush lease 생존 중 maintenance 재사용,N14 TxPairExistingRead/Write 생존 중 key mutable finish/cancel | 의도한 privacy/borrow/move/trait 지점의 진단을 요구. 다른 오타·미해결 import·fixture 실패를 성공 거절로 세지 않음. stale path만 바뀐 기존 probe는 해당 diff probe만 선택 |
| MBC10-V01..06, C1 memory6 | V01 fresh 원 owner→DiskSession→split→해제,V02 selected recovered→READY 전/후 into_live,V03 원 reader claim/binding 점유 중 재bind,V04 정상 loan반환과 active owner Drop,V05 writer/control mutex poison,V06 poison/실패 대기 notify | writer/File/slot/fence/pool/core 주소·generation·loan 동일, 이동1회. READY 전 같은 owner 반환. poison/Drop은 원 phase/effect/fence·대기 해제, 새 writer 생성0 |
| MBC10-V07..10, C1 admission memory4 | V07 long512B class의 잔여511/512/513를 각각 capsule/view/cursor 생성에 적용,V08 cursor2개 독립 close,V09 writer1+resident reader2가 겹친 nested sum에서 마지막 admission의−1/동일/+1,V10 foreground read2개 실행 중 progress진입 요청→새 reader거절→기존2개 finite반환 | 각 subinput 독립선택. long/short 별도 총계·same slotgen checked 반환. source guard/effect 전 부족거절. rollback/terminal은 신규 foreground credit 없이 진입하고 snapshot/cursor 강제해제0 |
| MBC20-V01..08, C2 Catalog memory8 | V01 actual CREATE 전 missing,V02 Commit후 같은 key found,V03 중간 gap key,V04 own Active,V05 foreign Active,V06 old snapshot/새snapshot,V07 source latch의 Txn/U capture 직후 retirement시도,V08 nonresident Status/Undo→dirtyvictim FlushRequired→재진입 | 기존 `lookup_catalog_empty` 관련 실제 경로를 새 `catalog::read`→CatalogReadOperation으로 수신. 고정 Catalog 정의 bytes/key/version oracle, point부재도 catalog-read-set등록. 원2pin·source latch 순서·cleanup census, decoder권한0 |
| MBC20-V09..11, C2 memory3 | V09 foreign core/old binding handle,V10 capture sequence이후 다른candidate submit,V11 read-only Serializable 종료와 active view/cursor Drop | stale는 typed 오류·output길이0. source/Owner 재대여는 각 guard종료 뒤만. read-only conflict는 Rejected와 slot/snapshot종료, 미해결 mutation Drop은 fence. Row point/cursor 세부는 MCB01–04 수신 |
| MBC30-V01..03와 V04-P/S, C3 preflight5개(1개 C0 pure) | V01 type 불일치,V02 nonnullable null,V03 canonical L2049. V04-P는 독립 valid IndexDefinition을 shared pure 전체 Catalog census에 제출. V04-S는 actual table history와 다른 valid Index cell을 FileIo read에 주입하는 [stored-source 음성 입력](dev3-module-consumers.md#index-negative-input) | V01..03은 Reserve/WAL/page/issuer0·생존Reserved0이며 별도 stale-source cut에서 samePreflight Before를 검산. V04-P는 C0 Unsupported. V04-S는 actual history와 home full equality의 Fatal(Load,Invariant)·원 fence 유지·load install/CheckedTable/Preflight/Reserve/INSERT0·영속 mutation0이며 Before retry가 아님. MCA01의 같은 P/S 실행을 공유 |
| MBC30-V05..10, C3 Catalog terminal memory6 | V05 prepared foreigncore,V06 prefix/digest변조,V07 actual physicalstage의 first-write 전 Unchanged,V08 firstwrite시작 후 error,V09 complete stage→pending→durable,V10 before stamp가 pending 중 변경 | V05/06 stage0. V07 같은 transaction과 원 intent/targets반환. V08 UNKNOWN/fence/ACK0. V09 original PinSet 배열·generation 동일/suspend 중 latch0/Drain 뒤 설치. V10 재검산 실패로 부분성공/가짜ACK0 |
| MBC30-V11..14, C3 memory4 | V11 page설치 후 metadata/DPT전 fault,V12 durable+wholeinstall 뒤 Q공개전 barrier,V13 table/receipt/changed-list 부족,V14 isolation validation뒤 같은 원control의 두번째terminal시도 | V11 fatal과 원pending/복구의무. V12 oldsnp가 미공개q를 보지 않으며 ACK0, publish뒤 새q. V13 stage전거절. V14 validation→publish까지 exclusive원ledger 유지·source ordering oracle. MCI01/03의 동일 interaction 실행과 공유 가능 |
| MBC30-N01..04, C3 Catalog native4 | N01 actual CatalogCommit ACK직후 homeflush전 kill,N02 Commitwrite 첫byte 후 fault,N03 completewrite 뒤 sync error,N04 durable+statuswholeinstall 뒤 Q공개전 kill | 각 동일build3PID produce/recover/reenter. controller ACK만 승자로 강제; UNKNOWN은 actual completeprefix/독립 WAL reader로 outcome결정. 새ACK·세번째PID 재진입 확인. Catalog이전의 바뀐 native driver만 선택 |
| MBC31-V01..06, C3 pair admission memory6 | V01 다른 reader pin 보유,V02 기존 reader load claim,V03 original flush capture/비Ready,V04 active pair key 중 ordinary resident pin/load/capture 각각 진입,V05 candidate 관측 뒤 source/generation 변경,V06 key의 foreign core/다른 existing request/fresh pair 요청 | V01..03은 close/generation/Reserve 전 Busy와 기존 사용자 소유 유지. V04 새 pin/load/capture0. V05 같은 leaf 최종 재검산으로 변화 전 거절. V06 quota 변경 전 거절. 새 owner/다른 pool claim0 |
| MBC31-V07..12, C3 pair completion memory6 | V07 Reserve first-write 전 actual Unchanged,V08 Forward stage 전 actual Unchanged,V09 actual Cancel 완료,V10 Forward whole8images/control/DPT/joint publish→release/durable→finish,V11 active key Drop,V12 claim 설치/종료 leaf의 poison/unwind | V07 같은 key의 Unstaged 정리 뒤 ordinary 진입 재개. V08 Before wrapper에 같은 key/private/flush lease 보존→명시 cancel. V09 원 Cancel receipt/allocator-after·fresh pair를 검산한 뒤 claim과 key 한 번 종료. V10 원9-target identity·generation 보존. V11/12 fence/notify·잔여 claim/복구 의무, healthy read/flush 재개0 |
| MBC31-V13, C3 장기 거래 수신 memory1 | T1 Serializable Catalog 관측 뒤 physical operation만 종료; T2 actual CREATE/Commit; T1 같은 snapshot으로 종료 검증 | snapshot/slot을 강제 해제하지 않고 T1 SerializationConflict. MCI01-V07과 동일 실행을 수신하며 이 기능이 바뀌지 않은 뒤 중복 실행0. 원 native CREATE/Cancel/Commit cut는 MBC30-N의 변경된 실제 driver로 수신 |
| MBC40-V01..06, C4 memory6 | V01 Root3/Job3/Profile3/base4의 현재format dispatch,V02 validSHA scope2→Rowingress,V03 no-loser committed corpus의Prune→ReleaseRole→RootFree→TerminalFinish,V04 Reserved cleanup,V05 Active chain이 C+B의 최초 canonical analysis policy gate에 도달,V06 actual CLR candidate의wronghead/target/debt | C의 observer/원owner가 검산. V02stage0,V03actualREADY,V04C CancelReserved만,V05 physical/REBASE/cleanup0·READY0/Analysis Unsupported,V06Before같은task 또는poststagefence. MCB known-bad12 및 A chain cases는 소비자 inventory를 수신 |
| MBC40-V07..09, C4 census3 | V07 memory W/File,V08 native W/File,V09 반환오류가최대인 Preflight/Reserved/Prepared/rollback/recovery task와residentread2개 동시 생존 | source retained1회+Σactive와caller carrier를 계수, class nested/long512/private425984/8192registry/5760backing/FD7/profile상한검산. 기존 `active.max`를 전체로 오인하면 실패. runtime재사용·Drop·source charge underflow/poison도 같은 finite lifecycle에서 감사 |
| MBC40-N01..03, C4 native3 | N01 C+B committed-only currentbuild READY,N02 실제Reserved→cancel선택 중 재장애,N03 C+A의3행 chain에서 중간CLR 재장애 | 소비자의 **durability witness가 있는 cut 정의**를 수신. actual selectedepoch/새ACK/최종재진입과 잔여credit/issuer/loan을 검산. 같은 source의 MCA03/MCB01 native와 동일사건이면 결과1개로 수신하고 재실행0 |
| C4 gate MCB05-N01..06 수신, native6개 | [소비자 no-loser 입력](dev3-module-consumers.md#acceptance)의 Active Forward, Row Reserved, Active headNone, Aborting nextNone, SystemPage Reserved-only, CatalogCreate Reserved | 각2PID·180초 negative. canonical 전체 검산 뒤 Analysis/Unsupported·CallerMustResolve, 별도 database_effect=Unchanged/mutations0/READY0. 정상 committed cleanup은 MBC40-N01의3PID. legacy recover의 새 Row scope 차단도 같은 source의 memory gate 검사에 포함. 중복 실행0 |
| C5 MCI01-V01..14·MCI02-N01..03·MCI03-V01..04 | [합류 exact inventory](dev3-module-consumers.md#acceptance)의 실제 A→B·isolation·publication·recovery 입력/cut를 그대로 수신 | C1–C4와 두 소비자 단독 완료만으로 합류PASS를 만들지 않음. 새로운 상호작용/diff만 실행하며 기존 완료case 반복0 |

물리 fault와 crash oracle는 stage 성공을 durable로 간주하지 않는다. native cut마다 actual write/sync·same requirement의 witness 유무를 입력에 명시하고, RAM-only stage는 유실될 수 있는 prefix로 판정한다. product API에 매 Reserve force를 추가하여 fixture를 통과시키지 않는다. append/latch 아래 임의 callback·sleep/wait를 삽입하지 않으며 필요한 scheduler hook은 실제 C 효과 경계를 가진 새 fixture 코드로 검토한다.

선택 source·case input/expected hash·ELF·controller ACK·PID/cut를 [소비자 입력 schema](dev3-module-consumers.md#independent-fixtures)에 맞춰 기록한다. 각 compiler/memory case30초, native180초(감사/정리 포함), 공통 C source당45분, C+A/C+B 각각45분, 합류20분을 상한으로 한다. native 정상3PID, 중간복구 재장애는 최대4PID와 마지막새ACK 재진입을 사용한다. cut/known-bad는1회이며 performance반복0. 결과 JSON/trace는 case당16MiB, source 묶음512MiB로 제한하고 cap 초과/TIMEOUT은 실패 또는 미완료로 보존한다. worker namespace/disk·FD는 원 profile cap을 그대로 적용하고 시험아티팩트용512MiB를 제품예산으로 빼지 않는다. 필수 분모를 줄이지 않고 다음 허용 실행으로 분리한다.

source census는 기존/new owner와 view/Result/return carrier의 동시 생존량, pending target 배열·private before/after/frame loan, registry·replay·control·flush backing, concrete memory/native W/File·FD·I/O phase를 모두 센다. 무변경 하위case는 이 목록에 대응된다는 이유만으로 다시 돌리지 않는다. 새/수정된 경계의 관련 원case를 재사용하면 source diff와 그 case의 관계 및 이전결과를 기록하고 같은 실행을 위 MBC 수신증거로 연결한다. 이것은 기존 NT/T06/DIR03·최종S5 전체분모를 취소하거나 완료한 선언이 아니다.

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
## 11. 설계 동결안과 후속 실제 의무

Q01–Q08은 아래 설계로 독립 D2 검토를 수신해 채택했다. 앞선 Q02/Q03 Catalog CREATE claim·producer getter와 Q05 최초 no-loser census 공백은 아래 계약으로 해소했다. 이는 actual compiler/runtime 검증 완료가 아니다. **설계 제안·수용·구현·실행을 구별**하며, 미실행 compiler/source census/current-build recovery를 API 미정과 혼동하지 않는다. 차단 반례가 나오면 해당 설계를 다시 열고 양쪽 소비자에 같은 변경을 인계한다.

| ID | 동결한 설계 계약 | 담당·후속 실제 수신 의무 |
| --- | --- | --- |
| Q01 | [issuer/profile/dispatch](dev3-module-consumers.md#format-dispatch)의 Root3 row high-water·Job3 RowMissingRoot·Profile3/base4/scope3, single leaf/inline2048/index0. 원 issuer의 checked successor와 취소 후 비재사용 | format/C/A; 실제 구현 byte/oracle의 독립 검토, C4 current-build dispatch와 fresh fixture. 과거 format 결과를 새 format PASS로 승격하지 않음 |
| Q02 | §4–6의 original owner 이전·조상 private seal·full generic/lifetime·error/loan 인계와 Row consuming API. [Catalog CREATE claim](#catalog-claim-gap)은 원 registry atomic close+active key gate+disjoint flush lease, [producer 입력](#producer-input-api)은 exact getters/role/presence/control borrow로 동결 | 경계/C; MBC00 compiler·MBC31 claim의 정상/실패/Cancel/Drop·C1 identity·C3 actual grant/terminal·Catalog 제품 수신. raw ctor/field 확대·facade-only 완료 금지. 실제 borrow/authority 검증은 M4 의무 |
| Q03 | 원 Owner의 guard별 재대여·Missing 2pin/8192 registry·Cell view/revision·caller Tx snapshot·RC 각 statement·자동 격리 set·읽기 전용 종료 validation. Catalog claim 동안 short operation Busy/같은 snapshot 재시도, 장기 reader/capsule 유지. 일반3pin/occupied before 제외 | C/B; [visibility](dev3-visibility-retirement.md#row-missing-traversal)·[statement/cursor](dev3-task-06-tree-rows.md#module-statement-cursor)·[격리](dev3-task-10-concurrency.md#module-row-isolation)와 MBC20/31·MCB/MCI 반례 수신 |
| Q04 | [Row 상태/count](dev3-module-consumers.md#row-states)의 head/mark/CLR/debt와 §6.7 closed action. A는 live/recovery CLR 후보, C는 actual source/observer/effect·CancelReserved/AbortEnd/committed maintenance | C/A; MCA02/03과 MBC40의 head/count·재장애·같은 loan 반환. WithRowClr는 actual A handler 수신이며 handler stub로 READY를 만들지 않음 |
| Q05 | [독립 fixture](dev3-module-consumers.md#independent-fixtures)의 actual binding+untrusted literal+same observer. [최초 policy gate](#row-recovery-policy)는 canonical 전체 검산 뒤 ReservedInventoryNone·Reserved root0·ATT Active/Aborting0를 physical/REBASE/cleanup 전에 요구 | B/C; MCB05-N01..06과 C4 gate/legacy 차단·기존 failure owner 보존. live own-write와 committed reopen 분리, 입력/ELF hash·상대 producer/fixture import0. decoded bytes로 source 권한 제조 금지 |
| Q06 | [자원 정본](dev3-resource-bounds.md#row-missing-module-budget)의425984 private/9proof5after·5760 backing·49152 long·65536 동시 short 합·original flush, P_R와 FD/profile 상한. §6.8–6.9의 선예약/진행/회수 | resource/C; MBC10/40과 MCA05 actual census·admission. source 측정은 M4 의무, 초과 시 계약 재검토이며 한도 증액/숨김 금지 |
| Q07 | §9 MBC00–40와 C5 수신의 정확 case 분모·입력/fault cut/oracle·30/180초·45/20분·출력/namespace 예산, [소비자 인수](dev3-module-consumers.md#acceptance)의 MCA/MCB/MCI 연결 | 인수/C/A/B; 실제 diff에 따라 개별 선택·NOT_RUN/FAIL/UNKNOWN 보존·완료한 무변경 case 반복0. 독립 D2 반례로 설계가 바뀌면 해당 case만 함께 수정. 각 case의 실제 source/실행 수신 전에는 PASS를 부여하지 않으며 C0 codec 선택 결과는 구현 색인과 별도 증거로 수신 |
| Q08 | §7의 C0–C5 유한 공급·제외·되돌림, §8과 [소비자 분담](dev3-module-consumers.md#division)의 전용 source/fixture·C0 순수 대역/actual 수신 시점 | root/C/A/B; [M2 인계표](../plans/module-independence-plan.md#m2-handoff)의 담당/기준 source와 대기·충돌·합류 증거 등록. build inventory는 실제 구현 시 등록 |

독립 검토자가 원 source와 정상/실패/복구 반례 및 소비자·자원 owner의 정합을 확인하여 **D2 검토 수용·필수 수정0**으로 판정했다. [검토 기록](../evidence/module-independence-survey-20261026.md#final-design-review)을 수신해 채택 상태를 ACCEPTED로 반영했으며 M2는 완료다. 확정된 C의 미구현·미실행은 M4/M5 의무로 남으며 M2에서 제품 prototype·컴파일·시험을 수행할 이유가 되지 않는다.

[실제 replay 작업 공간 대여](../evidence/module-independence-survey-20261026.md#c2-borrowed-replay-workspace)는 원 WORK_END zero tail의3328B를 semantic observer에 빌리고, 정상/오류/unwind 뒤 원 zero tail을 복원한다. producer의 local 배열과 wire/claims/hash·전체 backing은 유지한다. 실제 guard/반환/helper 계수 뒤 Full33980B·Resident16936B/shared22256B다. Pending 관측까지61556B의3980B 여유는 전체 entry/tail·독립 guard leaf·writer ticket을 제외한 값이며 전체 admission 미완료다. source별 선택9 PASS·import-only build와 원 compile FAIL을 구별해 보존한다.

[exclusive split의 고정 source 계산](../evidence/module-independence-survey-20261026.md#c2-split-source-charges)은 원 DiskSession의 독점 borrow에서 계산을 끝내 ReadPort에 private scalar3개를 보유하고 Catalog entry에서는 원 값을 읽는다. source ticket은 checked<=65536 성공 뒤u32로 보관하며 ledger u64/zero 영역과 권한·정리 순서는 유지한다. shared22400B에는 실제 split3port144B를 한 번 더한다. 별도 계산/tuple 반환과 recovered step caller를 계수한 live peak63328B/recovered64704B·Full33972B/Resident16928B이며 전체 entry/tail·writer admission은 미완료다.

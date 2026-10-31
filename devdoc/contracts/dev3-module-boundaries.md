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

현재 Commit은 [`PreparedCatalog`](../../crates/engine/src/storage/wal_io/plan4_apply.rs)의 immutable private frame/after/before·core/binding/fence·digest에서 시작한다. [`CatalogCommitAttempt/Installed`](../../crates/engine/src/storage/wal_io/plan4_apply.rs), [`StagedCommit/DurableCommit/InstalledCommit`](../../crates/engine/src/storage/wal_io/plan4_apply/terminal.rs), [`PendingCommitTargets`](../../crates/engine/src/storage/dirty_frame/commit.rs)가 exact requirement와 원 PinSet을 연결한다. 임의 page mask·LSN·scalar durable 관측으로 해당 증거를 대체할 수 없다.

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

2026-10-29 [폴더 재정리 후속 계획](../plans/module-independence-plan.md#folder-reorganization)은 이 책임·권한 계약을 물리 배치와 대조하는 작업 순서다. 현재 `plan4_apply.rs`가 `plan4_retention/`의 실제 하위 구현을 연결하는 이행 상태를 확인했으며, 2026-10-29 [L1 `allocation_reservation`](../evidence/module-independence-survey-20261026.md#l1-allocation-move)·[L2 `transaction::concurrency`](../evidence/module-independence-survey-20261026.md#l2-concurrency-move)의 자손과 [L3](../evidence/module-independence-survey-20261026.md#l3-mixed-system-move)의 `mixed_system_owner` 자손·recovery 시험만 논리 경로를 유지해 폴더로 옮겼다. 이 절의 `apply`·`plan4_retention` 파일은 [L4 판정](../plans/module-independence-plan.md#l4-record)에 따라 이행 중 최종 소유가 정해질 때까지 현재 배치를 유지한다. 배치만 정리하는 단위는 production/test cfg의 논리 경로·private seal·시험 selector를 보존한다. 초기화 조상을 바꾸거나 하위 참조를 다른 책임으로 이전하는 단위는 해당 API·수명·권한의 별도 D2 설계를 먼저 확정한다. 이 후속 계획은 위 채택 그래프와 §4.1의 접근 제한을 대체하지 않으며 M4/M5 공급 완료의 증거가 아니다.

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

2026-10-31 후속: 위 C1 당시의 공급 범위와 구분하여 [S3 Row Commit](#row-commit-s3)의 실제 consuming Commit·격리 충돌·table epoch·capsule 종료를 공급했다. `finish_aborted`와 비어 있지 않은 Row rollback/READY recovery는 여전히 후속 단계다.

<a id="writable-empty-capsule"></a>
#### XADB-MODULE-WRITABLE-EMPTY-001 — 실제 Begin과 빈 쓰기 capsule

2026-10-28 · main `b35150c` 기준 · **독립 D1/D2 한정 수용, 필수 수정0·source별 선택 검증 완료**. 위 최종 자동 선택 API의 선행 공급으로 `MutationPort::begin_catalog_transaction(owner,isolation,status,private,flusher)`를 추가한다. Status는 원 exclusive INIT의 `Plan4Target`, flusher는 같은 storage/pool의 원 MaintenancePort다. 자동 Status 선택이 없는 `begin_transaction`의 writable 요청은 여전히 Unsupported다. raw actor/q 생성자나 외부 Prepared는 추가하지 않는다.

원 isolation slot·512B source와 Serializable snapshot을 먼저 확보하고 기존 admitted `catalog_begin`을 호출한다. 성공 직후 실제 actor를 lease에 귀속시키며 원 canonical slot32..80과 매 경계에서 대조한다. Begin 전 Unchanged는 checked 정리하고 issued-local gap은 원 규칙을 보존한다. Begin 성공 이후 actor 결합/검산 실패는 fence/Fatal이다. lease가 거래의512B 반환을 단독 소유해 중복 CallerCredit을 제거하며 statement credit은 별도 원 slot에 남긴다. slot phase는 Active1/MustAbort2/Aborted3이고 Cell과 같은 원 상태를 검산한다. 읽기 전용 lease는 phase1만 받으며 실제 slot/credit 반환은 readonly1 또는 writable3에서만 허용한다.

첫 writable statement는 실제 ATT/descriptor·TerminalLeased와 **빈 undo head·root 없음**만 받는다. 시작과 명시 종료에서 다시 검산하고 mark는 이 실제 빈 상태의 None이다. 원 statement sequence·Serializable/RC snapshot을 유지한다. 미종료 writable view/transaction Drop은 I/O 없이 fence하며 원 obligation을 healthy 상태로 잃지 않는다. `finish_read_only`는 같은 writable capsule을 Before(WrongState)로 반환한다. own-actor 조회 adapter가 없는 현재 Catalog statement 읽기는 writable을 Unsupported로 거절한다. raw 관측 actor는 권한 생성자가 아니다.

`abort_empty_transaction(owner,transaction,private,flusher)`는 실제 empty 상태만 원 RollbackStart→AbortEnd로 진행한다. 첫 효과 전 Unchanged는 Before이고 RollbackStart 뒤 Unchanged는 같은 MustAbort capsule의 RollbackRequired다. 같은 capsule로 재시도할 때 RollbackStart를 중복하지 않으며 새 statement는 금지한다. 실제 AbortEnd observation과 actor/status, ATT 제거, 정확 한 TerminalReady/outcome2/commit_q0·root 없음이 일치한 뒤 Aborted로 전환하고 slot/snapshot을 checked 반환한다. ATT 부재만으로 종료를 인정하지 않는다. 물리 TerminalFinish는 별도 Aging 의무다. 일반 Row rollback/commit_row·독립 finish_aborted 공급을 주장하지 않는다.

wrapper의 인자·관측/오류 Result는 실제 CatalogProgressCharge와 source census의 lifecycle phase에 더한다. descriptor/root scan은 별도 helper에서 끝나고 큰 decoded 값을 nested lifecycle에 보유하지 않는다. long capsule+반환 carrier512B·short65536B 상한은 바꾸지 않는다. 새 empty-actor pre/postflight와 독립 begin/finish_statement helper에는 아직 runtime source admission이 없다. 이번 공급은 내부 lifecycle의 예약에 한정하며, 이 helper들과 진입 전/반환 뒤 전체 동시 floor는 미공급이다.

[선택 manifest](../../tests/fixtures/dev3-module-c3/write-capsule/manifest.json)는 compiler3·runtime14를 고정한다. source3, 신규 writable6(두 mode·Before/Capacity·실제 중간 abort 재시도·Drop/identity·실제 nonempty 반례·새 이미지의 현재-build memory 복구), 변경한 readonly credit/snapshot5다. 총1200초/compiler420초/case45초/로그8MiB/stack32MiB, 첫 실패 중단·원 실패 보존·완료 무변경 재실행 금지다. 후속 phase 불변식 수정은 [별도 선택](../../tests/fixtures/dev3-module-c3/write-capsule/phase-followup.json)의 WT01/WT03·신규 WT07만 검증한다. unchanged native N01은 실행하지 않으며 memory 복구를 native 영속 인수로 부르지 않는다. 실제 결과는 [이번 증거](../evidence/module-independence-survey-20261026.md#c3-write-capsule)에 보존한다. shared CREATE·own write 읽기·일반 mark/rollback·M4/M5 전체는 미완료다.

<a id="row-begin-capsule"></a>
#### XADB-MODULE-ROW-BEGIN-001 — 명시 Row Begin과 Row 쓰기 capsule의 scope 고정

2026-10-29 · branch `claude/project-thread-h2j8oc` 기준 · **독립 D2 2회 검토 수용(필수 수정 9건 반영). 공급 완료: S1-a·S1-b/S1-c·S1-d·S1-e·S1-f와 [RowReserve replay 개정](#row-reserve-replay)의 S1-g([S1-e 증거](../evidence/module-independence-survey-20261026.md#s1e-statement-mark))**. [WRITABLE-EMPTY-001](#writable-empty-capsule)과 같은 방식으로 최종 자동 선택 API의 선행 공급을 정한다. 이 등록은 U2-C4 단일 current 전환([format dispatch](dev3-module-consumers.md#format-dispatch): Profile3 → Root3/Job3 → base4) 뒤에만 적용하며, C의 scope3 순수 producer와 borrowed whole-group observer(`semantic_row::transition`)를 선행 공급으로 쓴다.

**scope를 정하는 시점.** 쓰기 거래의 scope는 Begin에서 진입점이 정한다. `MutationPort::begin_row_transaction(owner,isolation,status,private,flusher)`를 `begin_catalog_transaction` 옆에 추가하고, Begin은 registry 16 Terminal recipe로 scope를 영속 기록한다. 근거: §6.2의 `begin`은 read-write 거래의 Status·TerminalJob·Begin을 begin 시점에 공급한다(§6.2 `TransactionOptions` 문단). Job bank에서 scope는 Terminal registry 9/16 recipe로만 표시되며, root kind와 ATT descriptor 예약 행의 peak recipe는 이와 같아야 한다([Job3 census](dev3-module-consumers.md#job3-census) 첫 항목). 같은 actor의 Catalog/Row 혼합은 금지다(같은 절 셋째 항목). 소비자 계약 operation 표에서 TerminalJob을 바꾸는 Commit·AbortEnd(state 전이)와 TerminalFinish(Free)는 registry recipe를 바꾸지 않고, 같은 actor의 scope 불일치는 Job3 census가 거절하므로(같은 절 첫·셋째 항목) scope는 Begin 뒤 바뀌지 않는다. 자동 Status 선택이 없는 `begin_transaction`의 writable 요청은 여전히 Unsupported다.

- 거절한 대안: (1) 첫 효과에서 scope를 정하는 lazy 방식 — §6.2의 begin 시점 공급과 어긋나고 Begin을 RowReserve 모양 안에 넣어야 한다. (2) `TransactionOptions`의 scope field — §6.2가 동결한 순수 요청 값을 바꾸고, writable `begin_transaction`에는 여전히 자동 Status 선택이 없다(WRITABLE-EMPTY-001). (3) 모든 writable Begin을 Row로 — CREATE의 root kind/recipe 일치를 깨뜨린다.
- 혼합 거절: gate·source 예약·read-set 등록 전에 kind로 Unchanged Unsupported를 반환한다. 대상은 Row capsule의 `reserve_catalog_create`·`commit_catalog_transaction`·`abort_catalog_transaction`, Catalog capsule의 `preflight_row`·`start_row_rollback`·(공급 시) `commit_row`·`finish_aborted`이다. `abort_empty_transaction`·`begin_statement`·`finish_statement`는 kind로 분기하며 Catalog의 기존 첫 view·빈 mark 규칙은 그대로다.

**capsule과 오류.** 원 isolation slot·512B source와 snapshot을 먼저 확보하고, C의 scope3 Begin producer로 만든 Begin group을 기존 gate/progress 본문에서 설치한다. 설치 본문은 Catalog와 같은 하나의 본문에 비공개 payload 구분 {Catalog, Row}만 더하며 복사하지 않는다(§6.4의 한 본문 선례). slot actor kind 1은 Catalog, 2는 Row writable이다(RAM 전용; [동시성 계약](dev3-task-10-concurrency.md#module-row-isolation) slot@2의 값만 확장하며 wire 변경 없음). kind는 Begin 결합 때 한 번 쓰고 바꾸지 않는다. 진입 거절은 kind로 하고, 깊은 검산에서 영속 Terminal recipe와 다르면 Fatal(Invariant)이다. 기존 writable 검산(kind==1)은 {1,2}를 받는다. Begin 전 실패는 Unchanged(Busy, Capacity, Exhausted, WrongState, Membership 등 원 Begin의 Unchanged 원인)이고 slot·credit·snapshot을 checked 반환하며 issued-local gap 규칙을 보존한다. Begin 성공 뒤 actor 결합/검산 실패는 fence/Fatal이다. 빈 Row 거래의 abort는 `abort_empty_transaction`이 kind로 분기해 원 RollbackStart→AbortEnd로 진행한다. Row Begin과 빈 AbortEnd는 epoch·Q를 공개하지 않는다.

**statement mark.** Row 문장은 Active이고 RowReserved root가 없을 때만 시작하며, mark는 ATT descriptor의 `undo_head`(canonical `Option<UndoRef>`)를 slot bytes 88..128에 쓴다. WAL은 쓰지 않는다. view·mark·revision은 매 경계에서 다시 대조하고("view mark == slot mark"; Catalog의 빈 mark는 그 특수형), 끝난 문장은 sequence와 mark를 닫는다. 성공한 Row 문장 종료는 WAL0이고(§6.2), 쓰지 않은 6080B 문장 credit은 다음 mark로 넘기며 확보는 첫 Reserve에서 한다([소비자 계약](dev3-module-consumers.md#row-shape) 문장 credit 규칙). Reserved나 Aborting actor의 `finish_statement`는 Before(WrongState)다.

**CheckedTable 발급.** `CatalogStatementCall::finish_table(self, object, cursor, private) -> Result<TableStep<'c,'s>, CatalogCallError>`(`Table(CheckedTable<'c,'s>)|Missing|FlushRequired{target}`)만 발급한다. 이는 `catalog_operation` 공급 전까지 §6.4의 `finish_table` 위치를 대체하는 한정 공급(STATEMENT-001 선례)이며, 후속 operation은 같은 발급 본문을 쓴다. object는 lookup key일 뿐이고 발급 근거는 같은 호출의 `catalog::read::lookup` Found다(caller ObjectId로 만들지 않는 §6.4 규칙 유지). publication 아래 table slot을 확인하고 lock을 놓은 뒤, isolation leaf 아래 catalog read flag와 table read bit를 등록하고 반환한다. 두 lock은 동시에 잡지 않는다([동시성 계약](dev3-task-10-concurrency.md#module-row-isolation)의 read bit 규칙). lookup Missing(retired 포함)은 catalog flag만 등록한 Missing이고, Found non-Table은 Unsupported다. 자기 미commit CREATE의 Reserved slot은 Unsupported, slot 부재는 Fatal이다.

2026-10-29 정정(S1-f 통합·독립 검토): 위 서명의 오류 형식 `CatalogCallError`는 오기이며 `finish_table`은 `Result<TableStep<'c,'s>, RetentionError>`를 반환한다. 이 문단이 요구하는 Unsupported(Found non-Table·자기 Reserved)·Fatal(slot 부재)과 중첩 `catalog::read::lookup`의 wide 오류(Visibility·Frame·Fenced·CallerMustResolve)는 진입·정리 전용 3B `CatalogCallError`로 표현할 수 없다. §6.4 `finish_table`과 STATEMENT-001의 `lookup`·`finish_error`가 이미 `RetentionError`를 쓰며, 아래 자원 문단도 `size_of::<RetentionError>()` 계수를 요구한다. 정리(`finish_checked`)의 compact 오류만 기존 `From<CatalogCallError> for RetentionError`로 바꾸고, `CatalogCallError`의 3B 배치와 compact class 계수는 바꾸지 않는다. Found non-Table과 자기 Reserved slot의 Unsupported는 catalog read flag를 등록한 뒤 반환한다(동시성 계약 :287, 완료된 lookup 결과). FlushRequired는 관측이 아니므로 아무것도 등록하지 않는다. 공급 증거는 [S1-f](../evidence/module-independence-survey-20261026.md#s1f-finish-table)다.

**replay와 분석 gate.** scope3 group의 live 기록과 복구 replay는 같은 `history/row.rs` 본문 하나를 쓰며 `semantic_catalog`를 부르지 않는다. 이 Row replay arm은 C4의 부분 선행 공급이다(§7 C4 행). Catalog의 per-actor/per-root 선택과 의미 검산만 Row cell을 건너뛴다. Root/Job census·nonfree/Terminal 수·Terminal↔ATT 대응·cleanup debt 합·budget/job digest·Aging peak는 Row codec으로 Row cell을 포함한다(소비자 계약 [Job3 census](dev3-module-consumers.md#job3-census)와 [Aging](dev3-module-consumers.md#row-aging) 규칙). `recover_with_rows` 공급 전에는 §6.7의 [legacy Row scope 차단](#row-recovery-policy)을 구체화한다. U2-C4 전환 뒤 Profile3/base4는 Catalog scope2의 current 형식이기도 하므로([format dispatch](dev3-module-consumers.md#format-dispatch)) 차단 조건은 형식 번호가 아니라 아래 Row scope 상태다. preinspect의 Row frame 재생과 source·home·Pending·remaining 검산이 모두 끝난 뒤, AnalysisSeal·SourceFacts 기록 직전(§6.7 policy gate 위치)에 non-Free Row Job이나 Row recipe ATT 행이 있으면 Analysis/Unsupported·CallerMustResolve·database Unchanged·READY0·mutations0이며, 원 failure owner를 보존한다(§6.7). 정상 abort된 Row TerminalReady도 대상이므로, Row Begin을 기록한 DB는 C4 전까지 재시작 READY가 없다. 이 gate는 C4 공급이 대체한다.

**자원.** Row replay arm은 Catalog arm과 배타이므로 frame 최대항으로만 더한다. Row observer의 큰 배열은 Catalog observer와 같은 WORK_END zero tail(provenance 뒤 10240B)을 배타로 빌리고, 정상·오류·unwind 뒤 zero로 복원해(§11 borrowed workspace 선례, const assert) Row-arm frame이 기존 frame 단계 최대 이하가 되어야 한다. memory·native W와 Commit window·recovered bind/live에서 이를 측정하고, 넘으면 cap을 올리지 않고 Q06 계약 재검토로 간다. RowProgress는 같은 exclusive control 아래 Catalog progress와 배타이므로(§6.9) 별도 고정 class로 두고 `RowProgress ≤ CatalogProgressCharge`를 상수로 확인한다. finish_table carrier, mark를 더한 view(512B), `size_of::<RetentionError>()`도 계수한다.

**범위 밖.** table write bit·Row epoch 공개·RowWrite/RowRead class는 그 호출자가 생기는 S2/S3/S5에서 공급한다(반쪽 adapter 금지, §10). commit_row·finish_aborted·Row maintenance·C4 handler는 이 등록이 아니다.

2026-10-30 개정(S2-0b, [replay 검산 재배치](#row-write-validate-once)): S3이 공급할 `commit_row`의 group 준비는 Commit frame 전에 `history::validate_live` replay를 유지한다. S2-0b 뒤 Row 쓰기의 reserve·prepare·cancel은 replay를 열지 않으므로, replay할 수 없는 Row frame을 덮는 Commit ACK가 없다는 성질은 이 replay와 RollbackStart·AbortEnd lifecycle의 replay에 기댄다. S3이 이를 바꾸려면 자기 D2로 정한다.

<a id="row-reserve-replay"></a>
##### 2026-10-29 개정 — RowReserve replay의 값 commitment 채택(Q1)

**2026-10-29 · 독립 D2 검토 2회(r1 필수 4·권고 12, r2 필수 1·권고 5), 필수 전부 반영·S1-g로 공급([증거](../evidence/module-independence-survey-20261026.md#s1g-reserve-replay)).** 위 "replay와 분석 gate"와 "자원" 문단을 보충한다. Row replay가 입력을 어디서 얻는지를 S1-b 공급분까지 포함해 등록하고, RowReserve의 replay 후속을 정한다.

**결정.** RowReserve frame(57336B 고정, native Root8·Jobs9·Undo3, inventory item1)에는 canonical row 값이 없다. 그래서 S1-b는 이를 Unsupported·무효과로 거절한다([S1-b 증거](../evidence/module-independence-survey-20261026.md#s1b-row-replay), 선택지 B). B는 두 이유로 S2와 공존할 수 없다.
1. S2가 live RowReserve를 history에 쓰면 이후 모든 `validate_live`(`replay_durable`→`apply_frame`)가 `fatal(Source, Unsupported)`로 session을 fence한다.
2. 재시작의 비재사용 검산은 채택된 complete WAL prefix의 issuance를 기준으로 한다([소비자 계약](dev3-module-consumers.md#row-issuer) 재시작 문단). 그런데 prefix에 Reserve가 있으면 분석이 gate 전에 `Retention(Unsupported)`로 멈추어 그 issuance를 확정할 수 없다.

따라서 whole-group observer(`semantic_row::transition`)에 **replay 전용 Reserve 변형**을 더한다. live Reserve에서 값이 후속 상태에 닿는 곳은 두 field뿐이다. 하나는 Job `C=cell_bytes`(280..284), 다른 하나는 `projection`=XRP1(table,RowId,record)(288..320)이다. 나머지 byte는 원 arena·bank·actor와 세 selector(Undo container·table·statement)의 함수다(`transition/prepare.rs:757-849`). 변형은 두 field 36B를 후보의 유일한 kind9 native after cell(정확히 1024B)에서 **값 commitment로 채택**한다. 나머지는 live와 같은 코드로 재계산하여 후보 전체와 byte 비교한다.

**replay 입력의 출처.** Row replay arm은 기대 byte를 언제나 원 arena·bank에서 재계산하여 후보 전체와 비교한다. 후보가 고르는 것은 논리 인자뿐이며, live owner가 RAM에만 가졌거나 allocation으로 고른 값이다. 각 인자는 원 arena·bank가 보여 줄 수 있는 범위에서 검산한다. actor는 모든 Tx 연산에서 frame의 Tx owner0에서 얻는다. Begin·AgingStart 밖에서 Job은 표의 'Job 선택' 열대로 고른다. actor 상태로 고르는 연산(Reserve·Forward·Cancel·RollbackStart·StatementFinish·Commit·AbortEnd)은 원 bank에서 그 상태의 Row Job 정확히 하나와 결합한다. native before로 고르는 연산(Clr·Prune·ReleaseRole·RootFree·TerminalFinish)은 kind9 native의 slot에서 bytes가 그 before와 같은 원 non-Free cell 정확히 하나와 결합한다.

| op | Job 선택 | frame(owner·native)에서 | core에서 | 검산(Job 결합·arena·bank) |
|---|---|---|---|---|
| AgingStart | 없음 | — | — | Aging descriptor는 원 bank |
| Begin | Root cursor의 first-free(arena Root·bank) | actor(Tx owner0), Status 위치(kind9 after) | — | writer epoch=stream, local>adopted, actor ATT 부재·첫 빈 ATT 행, actor의 Terminal·root 0(bank), Status Free(g)→actor generation g+1·incoming0(arena) |
| Reserve(S1-g) | first-free. terminal은 actor TerminalLeased(bank) | container·table·commitment C/projection(kind9 after) | statement | 아래 selector 검산과 채택 조건 |
| Forward | actor RootReserved(bank) | 삽입 cell(kind1 after) | statement, next(H page) | cell은 Job C/projection·version·schema로 연다. statement는 Job statement와 같다. H는 원 bank predecessor와 같다 |
| Cancel | actor RootReserved(bank) | — | — | — |
| RollbackStart | actor TerminalLeased(bank) | — | statement sequence·mark(sequence0이면 full) | 두 mode 모두 Reserved root 없음. full(sequence0): Status Active 또는 Aborting(Statement). statement: Status Active, sequence≠0, 문장 credit(`statement_steps==3`), mark⇒head. 원 sequence와 mark ancestry는 미검산(STATUS-001) |
| Clr | kind9 before와 bytes가 같은 원 non-Free cell(그 slot) | native slot·before | statement sequence, next(H page) | sequence는 Job statement와 같고 mark는 Status target이다. H는 원 bank predecessor다 |
| StatementFinish | actor TerminalLeased(bank) | — | statement sequence·mark | sequence≠0, Status Aborting(Statement{target=head,next=head}), mark=head. 원 sequence는 미검산 |
| Commit | actor TerminalLeased(bank) | — | — | Reserved root 없음, Status Active, q=Root `commit_c+1` 재계산 |
| AbortEnd | actor TerminalLeased(bank) | — | — | Reserved root 없음, Status Aborting(Transaction{next:None}), head 없음, incoming0, retained0. q 없음 |
| Prune | kind9 before와 같은 원 cell | native slot·before | next(H page) | q는 terminal의 Committed q(bank), H는 원 bank predecessor |
| ReleaseRole·RootFree·TerminalFinish | kind9 before와 같은 원 cell | native slot·before | — | 원 cell 상태·cursor |

STATUS/UNDO/LEAF/JOBS-001의 "외부 입력을 후보 core/after에서 역발급하지 않는다"는 각 국소 `observe` 함수의 입력 계약이다. 그 함수들은 replay 경로에 없고 N1 교차 시험만 부른다. 위 표는 그 결정들의 예외가 아니라 whole-group replay의 별도 규칙이다.

**모양.**
- **변형.** 비공개 replay 입력 `Change::ReserveAdopted{terminal, container: PageRef, table: ObjectHandle, statement: u64}`를 둔다. payload는 64B로 Begin의 89B 이하이고, `operation()`은 Reserve다. `size_of::<Change>()`·`size_of::<Inputs>()`가 바뀌지 않음을 const assert로 확인한다.
- **`inserted` 운반.** `observe_with`의 기존 `inserted: Option<&[u8]>`는 다음을 운반한다.
  - Forward: kind1 after cell(기존대로)
  - ReserveAdopted: kind9 after cell. `inputs.change`로 고르며 `layout.operation()`만으로 고르지 않는다. 두 변형은 같은 Reserve layout을 쓰기 때문이다. `JOB_BYTES`가 아니면 InvalidInput이다.
  - `Reserve(plan)`과 그 밖: None. 따라서 owned `observe(Reserve(plan))`는 기존 전체 검산을 그대로 거친다.
- **도달 경로.** `produce`와 owned `observe`는 ReserveAdopted를 출력 byte 전에 Unsupported로 거절한다. 따라서 이 변형에는 `observe_borrowed`로만 도달한다.
- **prepare 분기.** prepare의 Reserve arm 하나가 **기존 `inserted` 인수로** 분기하며, 새 carrier 형식을 두지 않는다.
  - (Reserve(plan), None): 기존 단계 그대로다.
  - (ReserveAdopted, Some(cell)): `cell[280..284]`를 기존 `cell_bytes`로 읽고 Job3 범위 185..=2228을 다시 확인한다(밖은 Corrupt). leaf 적합은 0으로 채운 `record_scratch[..C−12]` 자리표시자로 판정한다. leaf 적합은 key와 길이만의 함수다(`tree_page.rs:238-334`, `tree_page/edit.rs:48-90`). `lease` 뒤 `cell[288..320]`을 Job after 288..320에 복사한다. budget은 C와 h로 재계산되어 다른 byte와 같이 비교된다.
  - 엇갈린 쌍 (Reserve(plan), Some)·(ReserveAdopted, None): InvalidInput이다. 위 운반 규칙은 이 쌍을 만들지 않으며, 이 거절은 방어 검산이다.
- **값 무관 단계.** 다음은 live와 같은 코드로 생략 없이 수행한다.
  - actor TerminalLeased, Status Active, Reserved root0, statement 범위
  - table handle·leaf
  - container 비동일·Undo identity·slot0 Free·g+1
  - RowId/sequence/mutation 발급, `LogicalRowVersion::new`·`UndoRef::new`, lease
  - Root·census·Tx descriptor·`validate_transaction`, aux item·core·redo

**selector 검산.** `history/row.rs`의 Reserve arm은 입력을 다음과 같이 얻는다.
- ordinal은 first-free, terminal은 actor TerminalLeased Job이다.
- 채택 cell은 `decode_job(kind9 native after)`이다. Row RootReserved이고 source==actor여야 한다.
- container는 그 cell의 UndoRef page, table은 그 cell로 부른 `table_source`, statement는 `core.statement()`다.

replay는 원 arena·bank에서 관측할 수 있는 live 선택 조건을 모두 검산한다.
- container: observer 전에 allocator entry가 `Allocated{generation=incarnation, object3, object_generation1, kind Undo}`여야 하고, 아니면 Invariant다. 또한 Undo identity·slot0 Free record·generation g+1·Status/anchor/leaf 비동일을 검산하고, after census가 U/pair alias를 거절한다.
- table: CatalogLeaf page8의 live Table record, retired 아님, full handle·anchor 일치, 첫 profile(generation1·schema_epoch1)을 검산한다. census의 committed creator 순서도 검산하지만, 이는 creator의 Catalog root가 남아 있는 동안만이다.
- statement: live 관측과 같은 ≠0과 actor root 최대 statement 이상(`transition/prepare.rs:763-768`)만 검산한다. 뒤의 Forward는 이 값을 Job statement와의 일치로 이어받는다. replay statement 가운데 범위만으로 받는 것은 이것뿐이다. Forward·Clr는 Job statement와의 일치로 결합한다.

replay가 재현하지 않는 것은 live 선택 owner의 RAM 전용 조건이다.
- `finish_table`의 snapshot 가시성
- publication 아래 table slot(자기 미commit CREATE)
- read/write bit
- isolation slot의 active statement sequence(ROW-WIRE-001: core는 statement 권한을 입증하지 않는다)

이는 기존 Forward/CLR/Prune replay가 Job의 table·statement를 쓰는 것과 같은 한계다. 어떤 소비자(C4 포함)도 replay된 Reserve의 table·statement를 snapshot 검산된 값으로 다루지 않는다.

U allocator 검산은 `validate_rows`에서 모든 non-Free Row root의 U로 넓혀 Forward·CLR·Prune 경로에도 적용한다. 이는 현재의 위조 경로를 닫는 것이 아니다. base4의 allocator·image 대조와 `undo_before`의 page identity 검산이 이미 그 경로를 막는다. 목적은 replay 기준을 provisioning owner의 기준과 맞추는 다층 방어다.

**채택 조건.** 다음 일곱 조건을 모두 지킨다. 하나라도 바꾸거나 commitment 소비자를 더하면 이 개정을 다시 검토한다.
1. 채택은 위 두 field 36B뿐이다. 후보 Job cell 전체를 채택하지 않는다.
2. 변형에는 `observe_borrowed`로만 도달한다.
3. 값과 무관한 실패 단계를 생략하지 않는다.
4. Forward의 commitment 개방은 바꾸지 않는다. 개방은 `check_record`의 원 schema decode·version·`record.len+12==C`·XRP1==projection이다(`transition/prepare.rs:323-352,375-391`).
5. RootReserved Job의 C/projection을 검산된 값으로 읽는 소비자는 없다.
   - Forward replay와 A의 producer(`row/insert/group.rs:645-652`)는 일치 비교로 연다.
   - Cancel·C4 CancelReserved는 읽지 않고 버린다.
   - base4(`plan4_recovery_base/row/actor.rs:282-289`)는 RootReserved에 key 부재만 요구한다.
   - ledger·census는 C를 유계 부채(Job budget `311608+2C+128h`, ATT Reserved-Forward 잔여 `91304+C+128h`)와 범위로만 쓴다.
6. container allocator 검산을 수행한다.
7. selector는 위 검산 범위만 보증한다.

**위협 모형과 손상 거절.**
- **WAL digest의 성격.** WAL digest는 키 없는 SHA-256(`content_digest`)이다. 우발 손상만 검출하며 서명·인증이 아니다(`crates/foundation/src/codec.rs:18`). WAL 무결성은 손상에 대한 checksum이지 출처 인증이 아니다. 이 개정은 그 가정을 바꾸지 않는다.
- **손상 거절.** 채택한 36B는 같은 frame의 여러 carrier에 실린다. kind9 native after cell, 그 native header의 after digest, Jobs page redo, Root `job_digest`/`budget_digest`이며, C는 Tx descriptor 잔여와 Aging after에도 실린다. 각 carrier는 재계산 값과 비교된다. 한 carrier만 다른 frame은 envelope digest를 다시 계산해도 거절된다. commitment 밖의 byte는 모두 재계산·비교한다.
- **위협 모형.** WAL 파일을 쓸 수 있는 행위자의 일관된 재작성은 다른 모든 replay arm과 같이 위협 모형 밖이다. 그런 재작성은 다음 frame들의 before descriptor digest와 Root before를 따라 이후 WAL 전체로 이어지며, 비밀은 필요하지 않다.
- **재작성이 바꿀 수 있는 것.** 재작성된 Reserve는 arena·bank 검산을 통과하는 한 commitment와 세 selector를 바꿀 수 있다. 신뢰 수준은 S1-b replay 및 C1과 같다. S1-b replay도 Begin의 actor/Status 위치, RollbackStart·StatementFinish의 statement sequence, RollbackStart의 mark를 WAL에서 받는다.
- **넓어지는 것.** 이 개정은 commitment의 출처를 base page에서 WAL Reserve frame으로 넓힌다. Forward replay는 commitment를 채택하지 않고 원 bank의 projection으로 연다.
- **추가 상태.** 그 편차가 만드는 추가 상태는 둘뿐이다.
  - (a) 어떤 값으로도 열리지 않는 commitment를 가진 RootReserved Job. Jobs page 2..6과 Root digest에 쓰이고 checkpoint 뒤 base에도 남는다. 그러나 Forward가 거절하고 Cancel·CancelReserved로만 끝나므로 RowLeaf/Undo 행 효과에 닿지 않는다. 부채는 C≤2228로 유계다.
  - (b) arena상 유효한 다른 selector 선택.
- **승격과 재계산.** 임의 cell의 Reserved 승격은 ordinal=first-free, `lease`의 Free before, Jobs page==bank, native slot=ordinal%15로 막는다. Job/Undo slot generation·RowId·sequence·issued·last_mutation은 재계산하며, after census가 중복과 U/Status/pair alias를 거절한다.
- **inventory.** `item_claim(undo_item(reserved))` items1을 재계산한다. `validate_transaction`이 items==reserved 존재, change_lsn==lease_lsn, last_mutation==job.mutation을 검산한다.
- **live 동치.** 진짜 frame의 replay 후속은 live `produce` 출력과 byte 단위로 같다.

- 거절한 대안:
  - (B) 현재의 Unsupported·무효과 유지 — 위 두 이유로 S2와 공존할 수 없다. 검산하는 불변식이 없고, H-505/H-507 이력은 시험이 redo를 직접 설치해서만 이어진다. S1-g 공급 전까지만 유지한다.
  - (C1) wire grammar에 canonical 값 section 추가 — 완전 재계산과 Reserve 시점 schema 검산을 준다. 신뢰 수준은 값이 같은 WAL에서 오므로 이 개정과 같다. 비용은 다음과 같다.
    - Reserve 모양이 값 의존이 되어 57336B 고정 모양을 잃는다. [row-shape 표](dev3-module-consumers.md#row-shape)·WAL 상계·Reserve admission charge·N1 literal/hash·R-H-505-item·wire selector를 다시 만들어야 한다.
    - INSERT마다 값이 WAL에 두 번 실린다(최대 약2KiB).
    - arm stack의 `ReservePlan` 80B가 여유 36B를 넘으므로 같은 replay 변형이 여전히 필요하다.
    - 더 주는 것은 열리지 않는 commitment를 일찍 찾는 것뿐이다. 그런 commitment는 Jobs page·Root digest에만 남고 RowLeaf/Undo 행 효과에 닿지 않는다.
    - ROW-WIRE-001과 row-shape 결정을 바꾸므로 넓은 D2가 필요하다.
  - (C2) 값 digest만 추가 — Reserve에서 열 수 없는 commitment이며, 이미 Job에 있는 projection보다 약하다. 이 개정에 지배된다.
  - (A′) 후보 Job cell 전체 채택 — frame이 generation·mutation·RowId·statement·sequence를 고르게 되고 Job3 codec만 남는다.
  - (D) Reserve redo를 물리 설치하고 검산을 Forward/Cancel로 미룸 — 어떤 Reserve 의미든 받는다.
  - (E) 다음 Forward frame을 미리 읽어 값을 얻음 — 한 frame replay를 깬다. crash가 Forward를 남기지 않을 수 있고, `validate_live` prefix가 Reserve에서 끝날 수 있다.
  - RAM 전용 Reserve — Cancel·CancelReserved로 정리되는 durable issuance를 요구하는 [소비자 계약](dev3-module-consumers.md#row-issuer)에 어긋난다.
  - 36B를 `Change`로 운반 — payload가 100B로 Begin 89B를 넘어 `Change`·`Inputs`가 커진다.
  - C를 Tx 잔여 행에서 역산 — 신뢰 수준은 같고 projection은 여전히 필요하다.

**거절 결과.** Reserve arm의 모든 거절은 arena·Jobs·bank·provenance·`REPLAY_TAIL_BYTES` tail·cursor를 바꾸지 않는다. cause는 다음과 같다.
- 채택 cell의 Job3 decode 실패: `Retention(<Job3 오류>)`(Corrupt·IdentityMismatch·OrderMismatch·BudgetMismatch·UnsupportedState)
- Row RootReserved·scope·source 불일치, allocator·selector 불일치: Invariant
- `table_source`의 non-Table·retired 거절: Reserve arm에서는 Invariant로 바꾼다. Clr/Prune에서는 C4까지 Unsupported로 남는다(S1-b 검토 A8). live `finish_table`은 retired에 Missing, non-Table에 Unsupported를 반환해 CheckedTable을 발급하지 않으므로, 그런 Reserve는 손상이다. 그 밖의 `table_source` 원인은 그대로다.
- 값 무관 단계·범위·byte 비교 불일치: `RowSemantic(…)`

거절이 드러나는 곳은 다음과 같다.
- **복구 분석.** phase Analysis·cause `Retention(cause)`·effect CallerMustResolve·database Unchanged·mutations0·READY0이며, 원 failure owner·loan을 보존한다.
- **live.** `validate_live`의 거절은 `fatal(Source, cause)`로 session을 fence한다(own-WAL 손상의 결과).
- **replay 성공 뒤.** RowReserve로 끝나는 tail은 위 임시 gate가 닫는다. phase Analysis·cause Unsupported(`Retention(Unsupported)` 아님)·effect CallerMustResolve·database Unchanged·mutations0·READY0이다.
- **producer.** `produce`·owned `observe`의 ReserveAdopted는 효과·출력 전 Unsupported다.
- **다른 값의 Forward.** 채택한 commitment와 다른 값으로 연 Forward는 Forward 단계에서 같은 무효과 거절이다.

**이 개정의 자원.** cap과 원 class를 바꾸지 않는다. 구현 조건은 prepare `locals_bytes`·`entry_bytes`·arm outer inventory에 새 계수 항을 더하지 않는 것이다. 항별로는 다음과 같다.
- `Change`·`Inputs` 크기는 const assert로 불변이다(ReserveAdopted 64B < Begin 89B). `ReservePlan`은 live 전용으로 그대로다.
- 값 분기는 기존 `inserted` 인수로 하므로 carrier가 없다. C는 기존 `cell_bytes`, 자리표시자는 기존 `record_scratch`를 쓰고, projection은 backing 간 복사다.
- selector는 `&inputs.change` match로 묶은 word 크기 참조·scalar다. 엄격히 항목별로 계수한 분석 상계는 +24B다. 값으로 묶는 것(+56B, 25763B>25743B)은 금지한다.
- arm outer는 계수된 여유 `Option<usize>`·`Option<JobView>`를 쓴다. lookup의 allocator decode는 observer 항 21323B보다 작다.

따라서 분석상 `history_row`는 25707B에서 최대 25731B로, `history_frame` 25743B(여유 36B) 이하다. 다음 값은 그대로다.
- `history_frame` 25743B·`history_peak` 29124B
- recovered peak memory 65368B/native 65504B, live 64000B/64136B
- `ROW_REPLAY_SCRATCH_BYTES` 9076B(10240B tail 안)

수용 판정은 측정이다. S1-g는 C01/C02 census 검산(`history_row ≤ history_frame`)·`s1a_borrowed_observer_source_bound_covers_measured_frames`와 const assert로 측정한다. +24B는 그 측정이 넘지 않아야 할 분석 상계이며 판정 기준이 아니다. 측정이 넘으면 cap을 올리지 않고 재구성한다.
- adopted slice를 `Prepared`에 두는 것은 대안이 아니다. `size_of::<Prepared>()`가 계수되어 `entry_bytes`를 늘린다.
- tail 여유 1164B를 쓰려면 `ROW_REPLAY_SCRATCH_BYTES`와 그 const assert(`transition.rs:65-75`, `history.rs:52-57`)를 함께 넓혀야 한다. ReplayLoan은 그 범위만 0으로 되돌리고, closure는 tail 전체가 0이기를 요구한다.
- 그래도 넘으면 Q06 계약 재검토로 간다.

`validate_rows`의 U allocator 검산은 Row closure에 JobView 하나와 entry 비교를 더한다. closure는 Catalog/Row 최대항이므로 Catalog 항이 더 크다는 것을 상수로 확인한다(S1-b 검토 A10). owned `produce`/`observe` 상계(30327B)는 S1-d의 `RowProgress ≤ CatalogProgressCharge`에 쓰인다. 그러므로 S1-d와 S1-g 가운데 나중에 공급되는 쪽이 이를 다시 확인한다.

**시험.** 신규 선택은 다음과 같다.
- H08 `retention::recovery::row_tests::s1g_row_reserve_replays_n1_literals` — H-505/H-507가 `apply_frame`으로 redo after와 bank 규칙에 이르고, 시험의 직접 설치 없이 H 이력 전체가 이어진다. fixture는 Status9·Undo10/11/14의 Allocated entry를 각 incarnation으로 담은 독립 page0 allocator image를 가진 새 arena다. Reserve 뒤와 Cancel 뒤에 `retention_replay_closure`를 실행한다.
- H09 `retention::recovery::row_tests::s1g_row_reserve_refuses_without_effect` — R-H-505-item과 digest를 다시 계산한 변형이 모두 무효과로 거절된다. 변형은 다음과 같다.
  - commitment: native만 바꾼 projection 또는 C, C 184·2229, budget 불일치
  - 위치·generation: first-free가 아닌 slot, Job/U generation
  - container: Status/anchor/leaf, 다른 root의 U, 미할당, incarnation 불일치
  - table: 부재·anchor 불일치. retired·non-Table은 각각 Invariant임을 확인한다.
  - actor·statement: actor 비TerminalLeased, Status 비Active, Reserved root 존재, statement<최대
  - leaf: C에서 leaf 가득, key 존재
- H10 `retention::recovery::row_tests::s1g_reserve_commitment_opens_at_forward` — 다음 이력을 확인한다.
  - Reserve(X)→Forward(X)는 재생된다.
  - Reserve(X)→Forward(Y)는 Forward에서 무효과 거절이다.
  - Reserve(X)→Cancel은 재생된다.
  - 모든 carrier와 이후 before digest까지 일관되게 다시 쓴 commitment의 Reserve는 재생되고, 뒤의 Cancel도 재생되며, 진짜 Forward는 거절된다.
- H11 `storage::semantic_row::transition::tests::s1g_reserve_adopted_equals_produced` — N1 계획과 경계(C185/2228, h0/1, 삽입 뒤 정확히 가득한 leaf)에서 `produce(Reserve)` 결과를 ReserveAdopted로 관측하면 after·Observation·`record`가 같다.
- H12 `storage::semantic_row::transition::tests::s1g_reserve_adopted_not_producible` — `produce`·owned `observe`의 ReserveAdopted가 after/frame을 건드리지 않고 Unsupported다. owned `observe(Reserve(plan))`는 N1 Reserve literal에서 `inserted` None을 받아 전체 검산을 거쳐 성공한다. prepare를 직접 불러 엇갈린 쌍 (Reserve(plan), Some)·(ReserveAdopted, None)이 InvalidInput임을 확인한다.
- H13 `plan4_recovery::tests::row_gate::s1g_reserve_tail_stops_at_interim_gate` — RowReserve로 끝나는 WAL에서 replay와 `retention_replay_closure`(U allocator 확장 포함)가 통과한다. 그 뒤 gate가 phase Analysis·cause Unsupported·effect CallerMustResolve·Unchanged·mutations0·READY0으로 닫는다. 같은 prefix의 `replay_durable`은 fence하지 않는다.

변경 경로 재실행은 다음과 같다.
- `transition::tests::n1_*`7, `s1a_*`4
- S1-b H01–H07
- `row_gate` H05·H06·C01–C03. H06의 혼합 fixture는 `validate_rows` 확장에 맞춰 allocator entry가 필요할 수 있다.

신규 literal은 별도 stdlib author가 만든다. 이 author는 N1 author를 import할 수 있고 제품 코드는 import하지 않는다.

**공급 단위와 순서.**
- **의존과 조율.** S1-g는 S1-b에만 기대며 S1-d/S1-e와 병렬로 갈 수 있다. `history/row.rs::record`의 S1-d A2 변경과 조율한다. live 코드는 없다.
- **진입 조건.** live RowReserve를 쓸 수 있는 S2 commit 전에 공급한다. S1-b 검토 A1(`check_ledger` 호출자의 Row descriptor 검사)과 함께 S2 진입 조건이다.
- **되돌림.** S1-g는 단독으로 되돌린다.
  - S2가 있으면 S2를 먼저 되돌린다. S2 위에서 S1-g만 되돌리면 B의 fence가 돌아온다.
  - S1-b를 되돌리려면 S1-g를 먼저 되돌린다.

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

현재 [C2 동기 Catalog 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)는 기존 `lookup_catalog`가 실제 `storage::catalog::read::lookup`을 호출하고, private `apply::read::operation`이 original source/ref/load/cleanup을 제공한다. 현재 operation은 실제 `SnapshotGuard` borrow를 보유하며 private factory에서 만들어 제품 호출에 즉시 consuming 전달된다. caller-retained factory는 없고 반환값에는 operation/candidate/guard가 남지 않는다. 위 `RowStatementView` 기반 `catalog_operation` signature·자동 read-set·§6.4 `CatalogReadOperation::finish_table`은 아직 미공급이다(2026-10-29: 한정 발급자 `CatalogStatementCall::finish_table`은 [ROW-BEGIN-001](#row-begin-capsule)에 따라 공급). 해당 entry는 원 short aggregate 예약과 실제 겹칠 수 있는 writer entry/progress를 함께 수신한 뒤 공개한다. 현 동기 source census를 전체 동시 admission 증거로 확대하지 않는다.

<a id="catalog-statement-supply"></a>
#### XADB-MODULE-STATEMENT-001 — 실제 Catalog statement 읽기의 한정 공급

2026-10-27 · D2 한정 ACCEPTED · 작성/통합 `/root`, 독립 검토 `/root/module_review`. 위 전체 short/writer 공급 뒤 공개 조건은 **`catalog_operation`/CheckedTable 및 Row 전체 수신**에 유지하되, 기존 source/progress를 실제 사용하는 `ReadPort::catalog_statement_call`과 `catalog_resident_statement_call`의 기능 검증을 먼저 공급하도록 한정 재개방한다. C의 미공급 전체가 독립 기능 구현까지 막지 않는 M4 원칙을 적용한다. 전체 조건을 기다리는 대안은 자동 read-set/실제 종료 conflict 검증도 늦추므로, 기존 scoped Catalog 호출의 내부 adapter를 채택한다. 일반 Row factory·CheckedTable·전체 aggregate를 공급한 것으로 표시하지 않는다.

`CatalogStatementCall<'u,'r,'t,'v,'b,'a,'s,'c,'p,'d,'m,'w,W>`는 원 `CatalogCallScope`와 같은 statement의 공유 borrow만 보유한다. apply 내부만 원 ledger/visibility pointer와 active phase를 확인해 원 SnapshotGuard를 잠시 빌릴 수 있으며 consumer에게 snapshot/lease/raw mask를 내보내지 않는다. original source/progress 취득 뒤 slotgen/active sequence를 검산하고 매 lookup 앞에 재검산한다. 실제 제품 lookup의 Found/Missing을 반환하기 전에 원 isolation leaf 아래 catalog read bit를 등록한다. FlushRequired/Busy/오류는 관측 등록이 아니며 등록 실패 때 성공 값을 노출하지 않는다. 정상/오류 checked finish와 Drop은 원 source→progress 정리를 사용한다. statement 종료/Drop은 이미 등록한 bit를 지우지 않으며 readonly Serializable 종료는 같은 원 MutationPort의 실제 epoch 검증으로 conflict를 반환한다. RC의 같은 statement snapshot·다음 statement 갱신은 유지한다.

새 wrapper/Result/registration helper·lookup 중 추가 statement borrow는 fixed class source에 포함한다. 예약 전 wide snapshot/factory Result·source 반환 뒤 tail 및 transaction begin/finish의 자체 pool/visibility 검산은 여전히 전체 uncharged floor의 후속 항이다. 실제 caller loop를 이번에 장기512B로 이전하지 않았으며 여러 공유 view 호출을 한 credit으로 덮지 않는다. writer phase·전체 short 합·Row/terminal/recovery와 M4/M5 미완료는 유지한다. wire/cap/권한 생성·원 epoch 공개 의미는 바꾸지 않는다. 실제 증거는 [statement 읽기](../evidence/module-independence-survey-20261026.md#c2-statement-reads)에 둔다. raw guard 노출·관측 누락/삭제·종료 충돌 미검출·short 비용 누락 시 재검토한다.

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

동결안 작성 당시 [`reserve_catalog_create`](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_live.rs)는 actual actor/Undo 이후 `&mut Plan4Retention`을 빌리고 [`prepare_tx_pair`](../../crates/engine/src/storage/dirty_init/tx_pair.rs)의 `&mut SharedPool`로 shared user를 배제한다. `target_mask` 관측→두 clean slot close/generation 소비→`init_claim` 설치는 여러 registry 구간이며 그 사이의 배제는 이 가변 borrow에서 온다. `return_catalog_pair`는 원 flush의 가변 borrow로 `PairIoQuiescence`를 발급하고 Cancel의 실제 allocator-after를 읽는다. 이 의미를 transaction 전 provisioning으로 바꾸지 않는다. 이후 [원자 leaf](#catalog-atomic-leaf)·[기존 대상 adapter](#catalog-pair-existing)·[공유 pool leaf](#catalog-pair-shared)를 순차 공급했다. 아래 exact candidate signature는 PAIR-SHARED-001에 따라 full stamp/next generation을 가진 opaque token으로 구체화한다. actual reservation의 disjoint borrow/연속 flush lease에 이어 [shared statement admission](#catalog-shared-create)을 구현했다. 아래 동결 당시 책임 sketch와 현재 구현 시그니처를 구분한다.

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
    ) -> Result<CatalogPairCandidates<'_,'c,'s>, DirtyError>;
    pub(in crate::storage) fn try_prepare_tx_pair(
        &self, authority: &TxPairAuthority<'_,'c,'s>, selected: &CatalogPairCandidates<'_,'c,'s>,
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

public-to-storage 소비 경계는 원 session 전체 대신 disjoint port/flush lease를 갖는다. 이 이름은 기존 `ReservedCatalogCreate`의 실제 책임 이전이며 추가 reservation owner가 아니다. 아래는 최초 동결 당시 책임 sketch다. 현재 [구현](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_shared_create.rs)은 mutation 전체 대신 원 storage/pool/control/history/visibility를 분리 대여하고, 실제 view/source/gate를 `CatalogCreateAdmission` 하나에 묶는다. factory는 `owner,view,undo,definition,private,flusher` 순서로 받으며 초기 오류 타입은 원 private를 반환하는 `CatalogCreateError`다. 성공/Before의 원 대여와 Fatal 의미는 [현재 계약](#catalog-shared-create)을 따른다.

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

정상 Forward는 key의 원9-target whole PinSet을 쓰고 stage→whole install→release→기존 actual durable 확인→CompletedTxPair 검산 뒤 finish_tx_pair를 호출한다. 마지막 registry 구간에서 `directory.finish_retention_pair`·`init_claim=None`·`key.active=false`를 함께 적용한 후만 flush lease/source charge를 반환한다. 그 뒤 일반 reader는 공개된 pair를 읽을 수 있지만 Catalog의 Transaction stamp는 Commit 전 가시성을 그대로 따른다. 제품은 statement를 종료하고 현재 `commit_catalog_transaction(owner, transaction, private)`로 terminal을 수행한다. 그 terminal의 same-PinSet Pending·Drain과 validation→epoch/Q 공개까지 exclusive ledger 보유는 바뀌지 않는다.

Reserve stage가 실제 Unchanged이면 같은 key로 allocator를 읽고 원 `Unstaged` Cancel permit를 발급해 RAM claim을 checked 반환한다. actual Cancel은 key 경로로 원 existing before를 준비하고 stage/whole apply 및 기존 durable receipt를 검산한 뒤 `Cancelled` permit를 사용한다. allocator-after와 두 fresh pair/DPT/requirement0를 content→registry 순서로 확인하고, 마지막 registry에서 directory return+claim 해제+key inactive를 원자 적용한다. 이 동안 active claim의 ordinary gate와 flush lease가 경쟁을 배제한다. view/transaction의 관측 집합과 소비한 영속 발급값은 되감지 않는다.

예약 이후의 Before는 같은 reservation·key·private·flush lease를 반환하며 외부 maintenance를 새로 호출하려 하지 않는다. 예약 전에 필요한 dirty victim flush는 이미 끝났고 Cancel/Forward는 같은 고정 existing targets와 원 rollback/terminal 몫으로 진행한다. stage 후 오류·cleanup 실패·poison은 원 Fatal/fence/잔여 obligation이다. active Drop은 fence/notify를 먼저 수행하고 claim을 정상 완료한 것처럼 지우지 않으며 no-I/O다. private/borrow의 Rust 해제는 claim 완료의 증거가 아니다. 이 경우 reader와 maintenance가 다시 호출돼도 원 health/fence로 거절하며 recovery가 처리한다.

실제 소비 예는 `T1 Serializable read-only begin→Catalog lookup→read operation만 종료(view/Tx snapshot 유지)→T2 begin/statement→reserve_catalog_create(owner2,&view2,...,&mut maintenance)→initialize→T2 statement 종료/Commit→T1 statement 종료/finish_read_only=Rejected(SerializationConflict)`다. T1의 장기 view가 mutation/flush를 가변 대여하지 않으므로 Rust 수명과 격리 의미가 함께 성립한다. 이미 admitted된 finite read operation은 원 gate에서 종료를 기다리며 source/WAL 진입은 그 뒤다. 이어 실제 pair admission에서 남은 pin/claim 등으로 거절되면 효과 전 Before Busy이고 원인을 정리한 뒤 같은 view로 재준비한다. CCA08은 원 read ticket의 종료 대기를 검산하며 physical pin을 보유한 실행으로 확대하지 않는다. 이를 위해 장기 snapshot을 버리거나 read-only 종료 검증을 생략하지 않는다.

<a id="catalog-atomic-leaf"></a>
#### XADB-MODULE-PAIR-ATOMIC-001 — exclusive pair의 원자 leaf 선행

2026-10-28 · D2 · main `ef95a8a` · 작성/통합 `/root`, 독립 `/root/source_admission_review`. [공유 claim 동결안](#catalog-claim-gap)의 하위 구현을 먼저 공급한다. 실제 `prepare_tx_pair/finish_tx_pair/cancel_tx_pair`의 `&mut SharedPool` 배제는 유지하고 close/claim·종료가 분리된 registry 구간을 각각 하나로 합친다. 공유 API·일반 pin/load/capture gate·key 기반 subset 공급은 이 단위의 완료 범위가 아니다.

prepare는 두 victim `try_write` 뒤 원 `FencedInitRegistry`에서 health/registry 유효성·retention mode·pins/load/flush/init/closed pair 부재, seven-existing exact identity/role/중복/victim 배제·pair 주소 부재·두 successor를 모두 변경 전에 검산한다. 등록 victim은 원 `pair_in`으로 clean/DPT/requirement 정합성을 확인하고 빈 victim은 fresh/DPT0/requirement0 및 selected closed_index의 원 MissingTarget을 보존한다. 같은 guard에서 directory close·fresh metadata·원 claim을 설치하며 Unchanged는 그 전에만 반환한다. 변경 중 unwind는 guard가 unlock되기 전에 fence하고 generation을 되돌리지 않는다.

finish/cancel의 최종 leaf는 실제 key/core/binding/fence/identity·quiescence·publication·완료/permit을 검산한 뒤 directory/claim/key.active를 같이 종료한다. cancel은 allocator read와 두 victim write를 최종 검산까지 유지하여 원 allocator-after를 같은 registry의 `pair_in`으로 대조한다. 잘못된 key의 indices는 content 접근 전에 검산한다. 원 stage/durable/control/epoch/파일 형식은 바꾸지 않는다.

source는 기존 보수 합과 새 atomic parent+원 entry validation max를 비교한다. 명시 parent에 FencedInitRegistry·victim write2·allocator read1·record/key/Result·완료/permit·배열/반환값을 계수하고 entry의 directory/init/load/flush/DPT 중첩 식을 별도로 더한다. 이전 일반 scalar1024B로 새 packet을 대신하지 않는다. 기존 cap은 유지한다.

실행 전 [manifest](../../tests/fixtures/dev3-module-c3/pair-atomic/manifest.json)에 runtime8·native N01 1회와 총1800초/compiler420초/case45초/native30초/admission600초/로그8MiB/stack32MiB를 동결한다. metadata-only fixture·실제 Unstaged/Cancelled/Forward·두 final unwind·원 Reserve prefix 복구를 구분한다. Native는 원3PID9→19→25다. 공유 경쟁이나 모든 fault 복구를 실행한 것으로 확대하지 않는다. [구현/실행 증거](../evidence/module-independence-survey-20261026.md#c3-pair-atomic)를 따른다.

<a id="catalog-pair-existing"></a>
#### XADB-MODULE-PAIR-EXISTING-001 — key를 빌리는 기존 대상 공급

2026-10-28 · D2 ACCEPTED(이 공급 한정) · 기준 main `4e68a4d` · 작성/통합 `/root`, 독립 `/root/source_admission_review`. [원 claim 설계](#catalog-claim-gap)의 기존7개 대상 adapter를 실제 exclusive CREATE에 연결한다. close/finish/cancel의 `&mut SharedPool`과 원9-target Forward는 유지하며 일반 pin/load/capture gate·shared CREATE admission은 후속이다.

`TxPairExistingTargets/Read/Write<N>`은 원 private PinSet/ReadTargets/WriteTargets와 authentic key borrow만 보유한다. N1..7, key/core/binding/fence·원 active claim·whole9 미보유, 요청의 exact directory registration·existing mask·fresh pair 배제·주소 중복·quota를 변경 전 검산하고 같은 registry에서 pin을 계수한다. N 범위는 runtime Shape로 확인해 원 Ordinary N8/9의 generic 인스턴스도 컴파일한다. 실행 Owner는 기존 binding/idle 권한을 쓰며 key에 없던 OwnerIdentity를 발급하지 않는다. PageIdentity.owner가 다른 요청은 exact membership에서 거절한다. raw inner/key나 bool 우회는 제공하지 않는다. key borrow가 read/write의 checked release/Drop까지 남아 mutable key 종료를 차단한다.

C의 `&mut reservation` 순차 재대여 외에도 whole9 admission의 같은 leaf에서 선택9개 pin count0을 요구한다. subset→whole과 whole→subset 양방향은 Busy다. 원 Init claim 밖·다른 frame의 ordinary pin 정책은 바꾸지 않는다. 이는 독립 검토 반례를 바탕으로 한 capability 조합 구체화이며 공유 제품 인수가 아니다.

공통 semantic preparation/application 본문은 private `CatalogTargetAccess::{Ordinary,Pair(&key)}`와 typed Read/Write dispatch로 한 번만 유지한다. 일반 factory는 Ordinary, active CREATE의 Reserve apply·Forward 준비·Cancel 준비/apply는 Pair를 사용하고 반환용 allocator read도 authentic subset을 쓴다. Reserve 이전 계획은 원 ordinary 읽기, Forward 설치는 원9-target이다. source는 실제 adapter/Result와 pin parent/원 registry validation을 계수하고 원 cap을 유지한다.

비교 대안인 raw bool 우회·inner 노출은 key 수명을 놓치므로 채택하지 않고, semantic 본문 복제는 두 정답 경로가 되므로 채택하지 않는다. 실행 전 [선택 manifest](../../tests/fixtures/dev3-module-c3/pair-existing/manifest.json)에 compiler5/runtime8·원 N01 한 번, 총1800초/compiler420초/case45초/native30초/admission600초/로그8MiB/stack32MiB를 동결한다. 신규 metadata는 설치/복구 증거가 아니며 실제 MC01/MPA01/MC04/N01과 구분한다. [증거](../evidence/module-independence-survey-20261026.md#c3-pair-existing)를 따른다.

<a id="catalog-pair-shared"></a>
#### XADB-MODULE-PAIR-SHARED-001 — 공유 pool leaf와 ordinary gate

2026-10-28 · D2 ACCEPTED(이 공급 한정) · 기준 main `c845ea7` · 담당/통합 `/root`, 독립 `/root/source_admission_review`. 사용자 전체 계획 진행 위임과 [PAIR-EXISTING-001](#catalog-pair-existing)에 따라 pool의 close/finish/cancel을 shared borrow로 바꾼다. actual `ReservedCatalogCreate`는 여전히 전체 bound를 exclusive로 빌린다. 이 단계는 공유 제품 CREATE, statement actor/source/flush lease 공급이나 M4/M5 인수를 뜻하지 않는다.

`CatalogPairCandidates`는 original pool anchor/core/binding/fence, 두 slot의 exact `Option<FrameStamp>`와 next generation을 보관한다. 두 content read guard와 최종 단일 registry에서 관측하며 pin·authority·Drop 효과는 없다. `try_prepare_tx_pair`는 원 registry에서 모든 quiescence/identity/shape와 두 full stamp·next generation을 검산한 뒤 원 FencedInitRegistry의 원자 close/claim을 사용한다. 등록 clean generation 변경, 같은 generation의 content stamp 변경, empty→used→empty ABA는 Stale다. index만 고르고 close 당시 새 generation을 허용하는 대안은 원 선택 identity를 잃어 채택하지 않았다. 실제 selected token 필드는 외부 storage caller가 작성할 수 없다. 과거 metadata fixture용 index adapter는 cfg(test)에만 남는다.

TxPair claim 동안 ordinary pin은 lookup/quota 이전, LOAD는 resident·victim·최종 claim 및 FlushRequired 반환의 원 leaf에서, physical capture는 lookup/attempt 이전에 Busy를 반환한다. preflight도 같은 규칙이다. System INIT의 기존 published ordinary 정책은 유지한다. pin/load/capture 선행이면 close가 거절되고, close 선행이면 일반 접근이 거절된다. authentic subset과 whole9는 원 claim 권한을 사용한다. finish/cancel은 mutable key를 요구하며 동일 최종 leaf의 total0/load0/capture0/initializer0 검산을 보존한다. read/write key borrow 중 종료는 컴파일할 수 없다.

Source는 새 carrier·factory Results·동시 read guards2/registry·next generation·full stamp와 caller carrier를 원 계수에 더한다. borrowed gate는34B이며 LOAD 반환 leaf는 순차 최대 leaf 항에 포함된다. shared typed entry의 추가 Result<Key>/3개 인자는 두 source 상계 후보 모두에 명시한다. 원 cap을 늘리거나 test-only adapter를 제품 경로에 더하지 않는다. 재검토 조건은 pool/token/claim 정체성·등록·generation·ordinary admission·source parent 변경이다. 이는 앞 계약의 exclusive pool 수신자 제한만 구체화하며 원 의미·format·Cancel/Forward 권한을 변경하지 않는다.

[선택 manifest](../../tests/fixtures/dev3-module-c3/pair-shared/manifest.json)는 compiler4/runtime12/원 nativeN01 한 번, total1800초/compiler420초/case45초/native30초/review600초/log8MiB/stack32MiB로 고정한다. 신규 metadata6와 실제 reentry1·실제 dirty hint 경합1·source1·Unstaged1·Drop/recovery1·System CF12-01 한정1을 구분한다. 완료한 atomic/subset 전체 matrix는 반복하지 않는다. [실행·독립 검토 기록](../evidence/module-independence-survey-20261026.md#c3-pair-shared)을 따른다.

<a id="catalog-create-parts"></a>
#### XADB-MODULE-CREATE-PARTS-001 — 원 CREATE 자원의 짧은 borrow

2026-10-28 · D2 ACCEPTED(이 공급 한정) · 기준 main `8613c53`. private `CatalogCreateAccess`는 원 storage/pool/ledger/history/visibility/flush만 직접 빌린다. factory는 원 bound 안에만 있으며 원 Owner/key/plan/private/definition을 호출 인자로 받는다. view에 session/backing/source admission을 새로 소유시키거나 reservation에 추가 저장하지 않는다. 기존 `ReservedCatalogCreate`의 전체 bound borrow와 consuming wrapper의 active/Before/Fatal/Drop은 유지한다. 다음 공유 MutationPort 수신을 위한 본문 분리이며 shared 제품 CREATE·actor/statement/source/flush lease의 완료가 아니다.

원 health→pending/population Fatal→flush Ready Busy 순서, Forward의 stage/install/history→checked release→durable 확인→finish→durable/pending 갱신, Cancel의 epoch delta 보유→apply/durable→source.unchanged→allocator 반환→epoch 적용 순서를 보존한다. allocator 반환은 원 read checked release와 `&mut *view.flush`의 PairIoQuiescence를 사용한다. 큰 reservation 조립은 기존 wrapper에 둔다. 별도 동작 복제나 arbitrary component constructor는 채택하지 않았다.

Source 상계는 기존 공통 prepare/ordinary/lifecycle 식을 유지하고 CREATE에만 factory 두 view carrier/receiver, 새 인자 및 실제 추가 Result를 더한다. Plan의 큰 Result는 plan 두 대안에만, Reserve apply/Unstaged return은 각 호출에만, Forward/Cancel은 body에만 더한다. 기존 Prepared는 move이고 기존 preparation/application view는 원 lower 식에 들어 있다. 작은 view 호출이 끝난 뒤의 원 큰 Before/reservation 반환은 별도 max이며 last-use 추정 할인은 없다. 원 cap은 유지한다.

[선택 manifest](../../tests/fixtures/dev3-module-c3/create-parts/manifest.json)는 compiler2/runtime7/nativeN01 1회와 total1800초/compiler420초/case45초/native30초/review600초/log8MiB/stack32MiB를 동결한다. 실제 변경 본문에 해당하는 Forward/Cancel/Unstaged/Drop·복구 및 새 오류 우선순위/Before 반환만 실행한다. 완료된 pool leaf·System·하위 wrapper는 반복하지 않는다. [실행 기록](../evidence/module-independence-survey-20261026.md#c3-create-parts)을 따른다. 원 자원 정체성·view 인자/반환 깊이·오류/Drop·source 소유가 바뀌면 재검토한다.

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

<a id="apply-install-grant"></a>
#### C3 실제 설치 권한 부분공급 — 2026-10-27

기준 `8c0f252`에서 §6.5의 `InstallGrant`를 apply root의 private `InstallKind::{Plan4,Legacy}`로 구현했다. Catalog ordinary apply·Commit·retirement의 세 실제 생산 지점은 complete stage와 원 pending 채택 뒤 직접 Plan4 권한을 만든다. Commit은 authentic Drain/resume 뒤 `install.before(at)`를 사용해 Pending flush로 recLSN이 해제된 경우도 원 규칙을 유지한다. generic dirty_frame의 page/metadata/DPT와 commit/retirement installer는 canonical disk 경계의 관찰 method만 받으며 원 core/fence·identity/generation·checked successor·exact requirement·before/after/DPT 검산은 유지한다. raw constructor·Clone/Copy·새 pin/owner는 없다.

기존 allocation·empty transaction·scope1 retention·population allocation 네 생산 지점은 sealed `InstallPermit` 값을 WAL-private `legacy::from_legacy`로 소비해 보유한다. System INIT·TxPair·Reload의 별도 권한은 이 enum에 합치지 않는다. enum/소비 carrier와 accessor의 순차 closure를 실제 source 식에 연결하고 기존 한도를 유지한다. 이 구조·선택 결과는 [source별 증거](../evidence/module-independence-survey-20261026.md#c3-install-grant)에 기록한다. 현재 Profile2는 기존 scope1 진입을 효과 없이 거절하며, 과거 Profile1 literal과 섞인 L01 FAIL을 positive 설치 증거로 재분류하지 않는다.

이번 공급은 installer 경계에 한정한다. terminal receipts/Prepared의 전체 이전·writer phase admission·Row producer/복구·C3 전체/M4/M5는 미완료다. 기존 실제 producer와 generic installer·legacy adapter를 함께 되돌리는 원 되돌림 단위를 유지한다. 독립 D1/D2는 실제 source·원 실행·6문서를 대조해 이 한정 공급을 수용했고 필수 수정은0이다.


<a id="apply-terminal"></a>
#### C3 실제 Catalog terminal 소유 이전 — 2026-10-27

기준 `4cf67bf`에서 `Binding`·Staged/Durable/Installed/Unstaged와 actual Attempt/Installed를 apply root의 private field로 이전하고 `TerminalWal::stage_catalog/wait_catalog`와 `DurableCommit::complete_catalog`를 private 자손에 연결했다. 원 `commit_bridge.rs`와 bootstrap 재노출 사슬을 제거했다. 원 `WalRuntime`은 물리 stage/requirement/wait만 제공하고 generic concurrency/dirty_frame은 canonical disk receipt의 관찰 method만 소비한다. 원 core/fence·InstallationKey·exact requirement·semantic digest·whole installation 검산과 Pending/Drain/pin/Q 순서는 유지한다. Attempt/Installed 및 해당 getter는 apply 밖에 노출하지 않는다.

원 stage가 실제 Unchanged를 반환한 경우에만 Unstaged를 만들고 original marker를 restore/cancel한다. terminal adapter의 operation/origin 선검산 위반·진입 중단·I/O 실패는 이 receipt를 발급하지 않는다. physical stage의 Busy/Capacity/OrderMismatch가 실제 Unchanged인 경우는 발급 대상이다. 신규 실제 Busy fixture는 같은 Prepared/C04 frame을 current Catalog decoder로 검산해 원 physical slot에 먼저 stage한 뒤 원 terminal의 Busy를 검산한다. 해당 호출의 효과0는 외부 주입 뒤를 기준으로 하며 외부 complete9/synced8 의무를 지우지 않는다. 원 call의 무 fence·원 Owner/pins/control/epoch/페이지 불변을 확인한 뒤 시험이 직접 fence하여 세션을 폐기한다. 정상 두 semantic writer 지원 증거가 아니다.

임시 TerminalWal와 receiver16B를 source 식에 추가했다. 세분한 wait 관측5336B, 보수적 Pending phase9136B를 구별하며 Full34020B/shared19328B와의 합58684B는 제품 aggregate admission이 아니다. 원 production broad36844B·memory/native source gate는 기존 한도를 유지한다. [source별 선택 증거](../evidence/module-independence-survey-20261026.md#c3-terminal)를 보존한다. Prepared 전체 이전·writer/entry/tail 회계·Row 공급/복구·C3 전체·M4/M5는 미완료다. 독립 D1/D2는 actual diff·r1–r3 전체 source/inventory·fingerprint·manifest·runner/helpers·ELF·원 로그·probe4 exact delta/diagnostic span 및6문서를 대조해 한정 수용했다. 필수 미해결 수정은0이며 검토자는 수정·시험·빌드·verifier를 실행하지 않았다.

<a id="apply-prepared"></a>
#### C3 Prepared의 실제 정의·소비 경계 — 2026-10-27

기준 `4f8afb7`에서 `PreparedCatalog`의 실제 정의를 apply root로 옮기고 canonical `disk::PreparedCatalog`로 노출한다. payload·OwnerIdentity·targets9·roles9·count·stamps9·before loan의 field type/order와 private visibility를 유지한다. 실제 checked factory/검산/소비 impl은 원 `catalog_live` 자손에 남는다. `CatalogPayload`는 apply에서 타입을 지명할 수 있는 범위로만 열며 자체 field·생성자는 비공개다. 기존 create_pair의 type 재노출은 제거하고 terminal/epoch/retirement는 root를 직접 소비한다.

이 한정 diff에는 runtime predicate/body·source 식·WAL/영속 형식 변화가 없다. [선택 compiler 증거](../evidence/module-independence-survey-20261026.md#c3-prepared)는 관찰 성공·private field/Clone/이중 Commit 거절만 판정하며 기존 runtime/source census는 반복하지 않는다. original owner/loan을 재구성하지 않고 원 checked factory와 소비 경계를 함께 되돌린다. 독립 D1/D2는 실제 source/원 compiler evidence와6문서를 대조해 한정 수용했고 필수 수정은0이다. writer/전체 source 회계·Row/복구·C3 전체·M4/M5는 미완료다.

<a id="row-reservation"></a>
### 6.6 RowReserve→A producer→prepare/apply/cancel의 consuming 경계

RowReserve는 **이미 original System INIT가 완료한 Free Undo container**의 slot0·RootJob·RowId를 예약한다. 새 physical page INIT나 Catalog pair claim을 RowReserve에 복사하지 않는다. actual [`prepare_undo_slot/initialize`](../../crates/engine/src/storage/wal_io/plan4_retention/population.rs)와 [`try_prepare_tx_pair(&SharedPool)`](../../crates/engine/src/storage/dirty_init/tx_pair.rs)의 차이를 유지한다. 전자는 C의 별도 provisioning 효과·예산으로 공급하고 후자는 기존 Catalog CREATE에만 남는다. `&SharedPool`인 MutationPort가 `&mut SharedPool`을 다시 얻는 불법 adapter는 없다.

첫 profile의 Free Undo/Status provisioning은 `DiskSession`의 exclusive 준비 경계에서 실제 System INIT로 완료한 뒤 port/transaction을 발급한다. 활성 capsule/view/port가 있는 동안 새 page provisioning을 끼워 넣지 않는다. 기존 prepared FreeUndo가 없으면 RowReserve는 **첫 Row 효과 전 Capacity**이며 이미 예약된 rollback/terminal을 방해하지 않는다. 현재 첫 extent 한도, actual allocated/free/retained census, 초기 provisioning 수와 별도 비용은 [자원 owner](dev3-resource-bounds.md#row-missing-module-budget)를 따른다. Free root의 재사용은 기존 allocation generation과 새 slot generation 검산을 모두 수행한다. 단순히 빈 page를 fixture에서 삽입하는 공급은 없다.

2026-10-29 참조(Q1): 위 "Free root의 재사용은 기존 allocation generation과 새 slot generation 검산을 모두 수행한다"는 S2가 공급할 live RowReserve 의무다. 이 allocation generation 검산은 page header incarnation·slot0 Free record에 더해 allocator entry `Allocated{generation=incarnation, object3/generation1, kind Undo}`를 포함한다. S2 adapter는 container 선택 시점에 이를 검산한다. 복구 replay는 같은 기준을 [RowReserve replay 개정](#row-reserve-replay)의 Reserve arm과 `validate_rows`에서 적용한다(S1-g).

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

2026-10-29 개정(S2-0, [Row 쓰기 개정](#row-write-budget)): 위 `preflight_row`는 마지막 인수로 원 `flusher`를 받고 `RowPreflightStep`(`Ready`·`FlushRequired{target}`)을 반환한다. PreflightRow와 ReservedRow는 terminal read gate와 flush lease를 비공개 field로 더 보유한다. preflight는 쓰기 target의 상주와 Free Undo container 선택까지 마친다. 그 안의 LOAD는 읽기 I/O이며 WAL·page·issuer 효과가 아니다. gate 뒤에는 read operation을 열지 않고 retention LOAD claim과 content admission만 쓴다. dirty victim이 필요하면 효과·read-set 등록 없이 `FlushRequired`를 반환한다. 위 오류·finish·Drop의 no-I/O 반환과 read-set 유지 규칙은 그대로이며 반환 순서는 source→gate→lease다.

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

2026-10-29 개정(S2-0, [Row 쓰기 개정](#row-write-budget)): 위 "동시 비용"은 RowWrite class의 phase별 census에 들어간다. producer phase는 A driver의 producer 호출 지점 몫과 A producer의 합이고, C 단계는 C 본문과 A driver의 그 호출 지점 몫의 합이다. 빌린 Jobs 64KiB와 descriptor는 source 값으로 다시 세지 않는다. A driver 몫의 상한은 C가 정한 `ROW_WRITE_CALLER_BYTES`다.

2026-10-30 개정(S2-0b, [replay 검산 재배치](#row-write-validate-once)): preflight 호출 지점의 A driver 몫 상한은 `ROW_PREFLIGHT_CALLER_BYTES`이고, 나머지 C 호출 지점(validation_lane·reserve·producer_lane·prepare·apply·cancel·finish)은 `ROW_WRITE_CALLER_BYTES`다. 비교하는 것은 호출 지점별 몫이다.

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

2026-10-29 개정(S2-0, [Row 쓰기 개정](#row-write-budget)): 위 `parts(self)`는 8번째 원소 `&'l mut [u8;32_768]`(A scratch)를 더 반환한다. 위 private 배치의 unused65536은 XOI page `[360448,376832)`·C scratch `[376832,393216)`·A scratch `[393216,425984)`로 이름과 수명을 고정했고, authority·owner 저장소가 아니라는 규칙은 그대로다. stored draft/checked carrier인 세 cut은 각각 128B 이하이며 여러 page의 stamp를 하나의 digest commitment로 담는다.

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

2026-10-29 개정(S2-0, [Row 쓰기 개정](#row-write-budget)): 위 호출 모양은 `preflight_row(owner, statement, table, private, flusher)`와 `RowPreflightStep::FlushRequired{target}` 분기를 더한 형태로 대체한다. 그 분기는 효과 전이며, A가 전용 flusher Owner와 같은 flusher로 flush하고 phase를 Ready까지 진행한 뒤 같은 행을 preflight부터 다시 시도한다. preflight가 Ready를 반환한 뒤에는 gate와 lease가 eviction을 막으므로 행 중간의 flush 단계가 없다. 위 Before·Fatal·Drop 규칙은 그대로다.

apply 성공은 원 Slot/RootJob을 Retained로 전환하고 transaction/aging ledger에 같은 obligation을 인계한 다음 proof를 inactive로 만든다. private는 `take()`로 RowApplied에 한 번 반환하고 mutation/Owner reborrow는 끝난다. Cancel 성공도 actual Cancel의 complete stage/whole apply와 원 claim 반환을 확인한 뒤만 proof를 닫고 private를 돌려준다. RowId/UndoSequence/slot generation/mutation high-water·소비한 WAL은 반환하지 않는다. Reserve/Cancel마다 별도 WAL force를 추가하지 않으며 원 WAL-before-page와 terminal durability 규약을 유지한다. 성공/실패 statement의 read/write set 역시 유지한다. C는 이 흐름에서 INSERT 값 검산·row/Undo/leaf encoding이나 A의 오류 정책을 대신 구현하지 않는다.

<a id="row-write-budget"></a>
#### 2026-10-29 개정 — Row 쓰기 자원·lane·preflight 개정(Q06)

**2026-10-29 · S2-0 계약 개정 · 독립 D2 수용(2026-10-30, r3) · 코드 미공급(공급 단위 S2-a~S2-f·S5-a~S5-d, A 단위 A-W1).** 위 §6.6 동결안의 `preflight_row` 서명·`RowProducerLane::parts`·A 호출 모양, [자원 정본](dev3-resource-bounds.md#row-missing-module-budget)의 미사용 lane, §6.9의 class 규칙을 S2(C3 Row Reserve·Forward·Cancel)와 S5(C2 Row 읽기) 공급 전에 보충하고 바꾼다. 측정 근거는 A의 D12 r4(`live_insert=9835`·`live_clr=7184`·`a_validation=1112`·`a_producer=50790`·`live_plus_a=60625`, native W), S1-e 증거의 A driver live insert 9955B, S1-d r1 census(shared retained memory20184/native20320B, `CatalogProgressCharge`44304B, Row lifecycle42089/43113B, Full34570/35594B, Resident17880B)다. 그 밖의 수치는 분석 상계이며 판정 기준이 아니다.

**D2 검토 기록.** 독립 읽기 전용 검토 3회다. r1은 ACCEPT WITH REQUIRED CHANGES(필수5·권고16), r2는 ACCEPT WITH REQUIRED CHANGES(필수1·권고7), r3는 ACCEPT(필수0·권고4)이며 r3 권고 C1·C2·C4는 이 본문에 반영했다. 회차별 대응·반영 commit·B·MCI 재시도 의무의 수용 상태는 [S2-0 계약 반영](../evidence/module-independence-survey-20261026.md#s2-0-q06-amendment)에 있다. 설계와 Q06 판정의 증거는 [S2/S5 설계와 Row 쓰기 예산 판정](../evidence/module-independence-survey-20261026.md#s2-design-q06)이다.

**결정.** 다음 열한 항목을 고정한다. 각 항목의 규칙은 아래 문단이 정한다.
1. 성공 statement 종료는 WAL0이며 영속 operation이 없다(Q1).
2. RowWrite는 `CatalogProgressCharge`, RowRead는 Full lookup charge를 재사용한다. port field를 더하지 않는다(Q2).
3. RowWrite는 preflight부터 finish·apply 성공·cancel 성공까지 terminal read gate와 원 flush lease를 보유한다(Q6·Q7).
4. preflight가 쓰기 target의 상주를 보장하고 dirty victim만 효과 전 `FlushRequired{target}`로 돌려준다(Q7).
5. 미사용 lane을 세 구간으로 나누어 이름·수명·선예약을 고정하고 `RowProducerLane`에 8번째 part를 더한다(Q4 M2).
6. RowWrite census는 phase마다 C 본문과 A driver의 그 호출 지점 몫을 합한다(Q4 M3).
7. 세 cut은 각각 128B 이하다(Q4 M6, Q17).
8. Forward producer 초과를 Q06 재검토로 기록하고 S2-e 진입 조건을 둔다(Q4).
9. table write bit는 Reserve와 apply의 stage 전에 등록한다(Q5).
10. actor당 live Row 예약은 하나다(Q19).
11. MBC10-V09·MBC40-V09의 oracle과 소비자 MCI03-V02의 barrier를 gate 배타에 맞게 대체하고, 소비자 계약의 "합계" 한도는 gate 배타로 읽는다(Q3).

**1. preflight 서명·gate·flush lease(Q6·Q7).** §6.6의 `preflight_row`를 다음으로 대체한다. 다른 wrapper 메서드의 서명은 그대로다.

```rust
pub(crate) enum RowPreflightStep<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace> {
    Ready(PreflightRow<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>),
    FlushRequired { target: Plan4Target }, // 효과·read-set 등록0, source·gate·lease 반환 뒤
}
impl<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>
    MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>
{
    pub(in crate::storage) fn preflight_row<'r,'t,'v,'i>(
        &'r mut self, owner: &'r mut Owner<'c,'s>,
        statement: &'r RowStatementView<'t,'v,'a,'c,'s>,
        table: &'r CheckedTable<'c,'s>, private: &'i mut [u8;425_984],
        flusher: &'r mut MaintenancePort<'_,'s,'c,'p,'d,'m,'w,W>,
    ) -> Result<RowPreflightStep<'r,'t,'v,'i,'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, RetentionError>;
}
// PreflightRow·ReservedRow의 비공개 field 추가(PreparedRow는 ReservedRow를 통해 보유).
// source field 뒤에 gate, io 순서로 선언하여 암묵 Drop도 source→gate→lease 순서다.
//   gate: Option<TerminalReadGate<'c,'s>>,  // CatalogCreateAdmission 선례
//   io: Option<RowIoLease<'r,'c,'s,'w>>,    // 원 RetentionFlushControl의 &'r mut 하나. Drop impl 없음
```

- **순서.** [ADMISSION-002](#operation-source-admission)를 따른다. guardless health·origin → kind와 view(Catalog·읽기 전용 view는 [혼합 거절](#row-begin-capsule)대로 Unsupported) → CheckedTable origin → flusher의 pool/storage binding, flush I/O origin, phase Ready와 잔존 부재(`check_catalog_pair_io`와 같은 검산) → `Owner::close_reads_for_terminal`로 terminal read gate 닫기와 admitted read 반환 대기 → `OperationSourceAdmission::progress(RowWrite)` → 깊은 검산·상주 보장·Free Undo 선택 → read bit·catalog flag 등록 → cut 기록 → source guard 해제 순서다. gate 뒤의 깊은 검산·schema/leaf 복사·LOAD는 read operation을 열지 않고 content/apply admission과 retention LOAD claim(`check_retention_reader`·`try_content`)만 쓴다. 자기 gate가 `try_read_operation`을 거절하기 때문이다.
- **flush lease.** lease는 원 flush의 단 하나인 mutable borrow를 preflight부터 finish·apply 성공·cancel 성공까지 연속 보유하며 Before 반환에서도 놓지 않는다([CREATE-LEASE-001](#catalog-create-lease)·[LIFECYCLE-PROGRESS-001](#lifecycle-progress) 선례). 획득은 `check_catalog_pair_io`와 같은 검산을 쓴다. 원 binding 일치와 phase Ready를 요구하고, flush I/O origin 불일치와 capture·durable·authority·retired가 남은 Ready는 원 fence(CallerMustResolve)이며, flush I/O preflight 오류는 `Flush(…)`로 보고한다. CREATE와 달리 pair·selected generation을 동결하지 않는다. 목적은 행 하나가 끝날 때까지 flush capture와 eviction이 쓰기 target에 닿지 않게 하는 것이며, lease 아래에서 C는 flush I/O를 하지 않는다. `RowIoLease`는 `CatalogPairIoLease`처럼 Drop impl을 두지 않는다. Drop impl이 있으면 step의 match 끝까지 `'r`이 살아 `FlushRequired` 분기에서 flusher를 다시 빌릴 수 없다.
- **gate.** 같은 구간 동안 terminal read gate를 보유한다([원 statement CREATE 예약](#catalog-shared-create)의 admission 선례). gate는 `Owner::close_reads_for_terminal`(foreground Owner만)로 닫는다. 이 입구는 의도한 선택이다. 기존 progress 경로(Row lifecycle·`transaction_check`·shared CREATE·retire)는 maintenance Owner도 받는 `close_reads_for_retirement`를 쓰지만, Row 쓰기는 foreground statement 전용이므로 Commit([WRITER-PROGRESS-001](#writer-progress))과 같은 입구를 쓰고 maintenance Owner는 InvalidRole이다. S2-a는 이 입구를 retirement 입구로 바꾸지 않는다. 이 동안 gate를 가진 Owner를 포함한 모든 새 read operation은 Unchanged `InstallationPending`이며, wrapper가 `&mut Owner`를 보유하므로 같은 Owner의 read는 생기지 않는다. 정상 정리는 source→gate→lease 순서이고, 원 Fatal은 fence 아래 charge를 보존한다(§6.9). 효과 전 Drop과 `finish`는 I/O 없이 같은 순서로 반환한다. active Reserved/Prepared의 Drop은 fence이며 Cancel을 대신하지 않는다. gate와 lease는 latch가 아니므로 위 lane 대여의 "latch0" 조건과 충돌하지 않는다.
- **상주 보장.** preflight는 쓰기 target 집합 T(Root, Reserve RootJob의 Jobs page, TerminalJob page, Status, 선택한 U, RowLeaf, allocator, tableAnchor, actor head의 H Undo page; 물리 중복 제거 뒤 최대9)를 상주시킨다. Free Undo container 선택(위 2026-10-29 참조(Q1) 문단의 allocator·incarnation·slot0 Free 검산)은 preflight에서 하여 cut에 기록하고, Reserve는 같은 container를 다시 검산한다. 비상주 target은 원 retention LOAD 본문(조회 경로와 같은 claim·검산·설치, protected는 이미 상주한 T)으로 적재한다. LOAD scratch 6page는 private의 frame 구간 `[0,98304)`이며 preflight 안에서만, frame 생산 전에 쓴다. 이 적재는 읽기 I/O이며 WAL·page·issuer 효과가 아니다. LOAD가 dirty victim을 요구하면 read bit·catalog flag 등록과 cut 기록 전에 source→gate를 checked 반환하고 lease를 놓은 뒤 `FlushRequired{target}`(그 victim)을 반환한다. 관측이 아니므로 아무것도 등록하지 않는다([S1-f](#row-begin-capsule) 선례).
- **preflight Ready 뒤의 비상주.** preflight가 Ready를 반환한 뒤에는 gate·lease·배타 MutationPort 아래이므로 정상 경로가 T를 evict하지 않는다. reserve·producer_lane·prepare·apply·cancel이 T의 비상주를 관측하면 Unchanged가 아니라 Fatal(Invariant)이며 원 fence를 유지한다. S2-b는 evict할 수 있는 모든 경로(reader LOAD claim·flush capture·allocator residency·Status pressure)가 gate나 lease 아래에 있음을 증거에 적는다. 하나라도 밖에 있으면 이 개정을 다시 연다.
- **관측 동작.** 한 행이 진행되는 동안 다른 thread의 새 Row·Catalog 읽기는 `InstallationPending`이고, preflight는 admitted read의 반환을 기다린다. 같은 thread가 다른 Owner의 read operation을 보유한 채 `preflight_row`를 부르면 미반환 의존이므로 A와 B는 그 operation을 먼저 반환한다([회수 progress](#retirement-progress) 선례). 행 진행 중 A는 flusher를 쓸 수 없다.

2026-10-30 개정(S2-0b, [replay 검산 재배치](#row-write-validate-once)): 위 "순서"의 깊은 검산은 상주 보장과 Free Undo 선택 뒤 `history::validate_live`를 한 번 실행하고, T를 typed read 아래 private의 before 구간에 복사해 replay arena와 byte 비교한 다음 read bit·catalog flag를 등록한다. replay scope는 replay buffer의 checkout이며 read operation이 아니므로 자기 gate 아래에서 연다(Row lifecycle 선례). 상주에 dirty victim이 필요하면 이 replay 전에 `FlushRequired`를 반환한다. reserve·prepare·cancel은 replay scope를 열지 않고 cut commitment의 digest 연쇄로 이 증명을 이어받으며, 그 차이는 같은 wrapper의 Before(Stale)다. validation_lane과 producer_lane은 연쇄를 검산하지 않는다. 이 경로들의 거절은 아래 거절 결과 표의 2026-10-30 보충이 정한다.

**2. A의 호출 모양.** §6.6 끝의 호출 모양을 다음으로 대체한다. `?`의 뜻과 Before·Fatal·Drop 규칙은 §6.6 그대로다.

```rust
let mut preflight = match mutation.preflight_row(owner, statement, table, private, flusher)? {
    RowPreflightStep::Ready(preflight) => preflight,
    // 효과0. A는 전용 flusher Owner와 같은 flusher로 target을 flush하고 같은 행을 preflight부터 다시 시도한다.
    RowPreflightStep::FlushRequired { target } => return flush_then_retry(flusher, flush_owner, target),
};
let input = row::insert::validate_insert(values, preflight.validation_lane()?)?;
let mut reserved = preflight.reserve(input)?; // 여기서만 첫 Reserve 효과
let proposal = {
    let lane = reserved.producer_lane()?;
    row::insert::produce_insert(lane)?
}; // frame/after/scratch의 짧은 mutable borrow 종료
let prepared = reserved.prepare(proposal)?; // Reserved 전체 이동
let applied = prepared.apply()?;            // 같은 proof/loan/gate/lease
```

`flush_then_retry`는 FLUSHER_SLOT Owner(`require_flusher`)와 MaintenancePort를 함께 쓴다. `try_flush(flush_owner, &target)` 뒤 phase가 Ready가 될 때까지 `continue_flush`를 진행한 다음 다시 시도한다. preflight는 Ready가 아닌 flusher를 Busy로 거절하기 때문이다. preflight가 Ready를 반환한 뒤에는 행 중간의 flush 단계가 없다. prepare·apply의 Before는 실제 거절이므로 A의 기존 reopen·cancel 정책을 유지하며, Cancel의 Before도 상주 부족에서 생기지 않는다.

**3. private lane과 8번째 part(Q4 M2).** [자원 정본](dev3-resource-bounds.md#row-missing-module-budget)의 미사용 `[360448,425984)`를 다음 세 구간으로 고정한다. 선예약은 caller가 이미 보유한 425984B loan 자체이며 새 할당·owner·authority는 없다. 여덟 구간의 비중첩과 합425984는 const assert로 확인한다.

| 구간 | bytes | 이름·사용자 | 수명 |
| --- | ---: | --- | --- |
| `[360448,376832)` | 16384 | XOI page(C) | Row layout의 Reserve·Forward·Cancel·CLR 준비에서 XOI inventory projection. 한 C 단계 안 |
| `[376832,393216)` | 16384 | C scratch(C) | `produce_borrowed`·`observe_borrowed`의 `ROW_REPLAY_SCRATCH_BYTES` scratch(9076B, 나머지 미사용). 한 C 단계 안, 진입 때 0 |
| `[393216,425984)` | 32768 | A scratch(A) | `producer_lane`이 발급한 lane borrow 동안만. 발급 전 C가 0으로 채운다 |

- Row layout에서는 Forward frame(최대93660B)의 tail 4644B가 XOI page 16384B보다 작으므로 XOI를 frame tail에 두지 않는다. Catalog layout과 Row lifecycle의 기존 배치는 그대로다.
- frame 구간 `[0,98304)`(6page)은 preflight의 retention LOAD scratch가 먼저 쓰고, Reserve 이후의 frame 생산이 이어 쓴다. 두 사용은 시간상 겹치지 않으며 preflight 반환 뒤 그 bytes는 의미가 없다.
- `RowProducerLane`에 `scratch: &'l mut [u8;32_768]`을 더하고 `parts(self)`는 이를 8번째 원소로 반환한다. 이 규칙은 `RowProducerLane`을 발급하는 모든 곳(§6.6 ReservedRow, §6.7 RowRollbackTask, 복구 task)에 같다.

```rust
pub(crate) struct RowProducerLane<'l> {
    context: RowProducerContext,
    control: RowControlObservation<'l>,
    before: &'l [[u8;16_384];9],
    schema: &'l [u8;16_384],
    frame: &'l mut [u8;98_304],
    after: &'l mut [[u8;16_384];5],
    candidate: &'l mut [u8;16_384],
    scratch: &'l mut [u8;32_768],             // A 전용, lane borrow 동안만
}
impl<'l> RowProducerLane<'l> {
    pub(in crate::storage) fn parts(self)
        -> (RowProducerContext, RowControlObservation<'l>, &'l [[u8;16_384];9], &'l [u8;16_384],
            &'l mut [u8;98_304], &'l mut [[u8;16_384];5], &'l mut [u8;16_384], &'l mut [u8;32_768]);
}
```

- A scratch는 authority·owner 저장소가 아니며 byte만 담는다. slice reference 배열 같은 Rust reference 값은 A의 stack에 두고 raw loan에 unsafe cast하지 않는다([자원 정본](dev3-resource-bounds.md#row-missing-module-budget)의 raw loan 규칙). C는 lane 반환 뒤 A scratch를 읽거나 검산 근거로 쓰지 않는다.
- frame·after·A scratch의 이전 내용과 proposal이 선언한 길이 밖의 bytes는 의미가 없다. C의 observer는 선언한 길이·순서 안의 frame과 after page 전부를 원 before·issued identity에서 독립 재계산해 byte 단위로 대조하며(§6.6 prepare·stage 직전 재검산), A scratch는 읽지 않는다. 따라서 A가 Root after page를 `after`에 직접 써도(M1) 검산 강도는 같다.
- 8-tuple 전환은 A의 기존 두 destructuring 지점을, `preflight_row` 서명 변경은 A의 `Boundary::preflight`와 `LiveCall`(flusher 전달, step 처리)을 깨뜨린다. S2-a의 한 commit이 C 표면과 A의 적응을 함께 바꾸고 A 검토 서명을 받는다. 그 적응은 새 원소를 받아 두고, flusher를 전달하며, A-W1 전까지 `FlushRequired`를 보유 자원 없이 `Failure(Stage::Preflight)`로 끝낸다. C 본문이 Unsupported인 동안 이 분기에는 도달하지 않는다. flush 뒤 재시도 정책은 A-W1이 공급한다.

**4. 계수 규칙(Q2·Q4 M3·M6).**
- RowWrite class는 split 때 계산한 `CatalogProgressCharge`(현재 44304B)를 `OperationSourceAdmission::progress`로 예약한다. RowRead class는 `charges.full`(현재 memory34570/native35594B)을 예약한다. `catalog()`는 RowWrite를 0(fenced Invariant)으로, RowRead를 full로 매핑한다. MutationPort·ReadPort·DiskSession에 field를 더하지 않는다. field 하나는 shared를 늘리며 지금 native recovered 여유는 32B, memory `pending_full` 여유는 84B다.
- RowWrite의 각 phase census는 다음과 같다. 실제 배타 phase만 max로 결합한다(§6.9).
  - C 단계(preflight·reserve·prepare·apply·cancel·finish): C 본문 + wrapper·Result·return carrier + A driver의 그 호출 지점 몫.
  - A 단계(validate·producer): A driver의 그 호출 지점 몫 + A 본문. lane 값은 driver 몫에 들어 있다. control borrow의 Jobs 64KiB와 private 구간은 loan이며 source 값으로 다시 세지 않는다.
- A driver의 몫은 deepest point 하나가 아니라 C 호출 지점과 producer 호출 지점마다 따로 보고한다(M3). C는 그 몫의 상한 `ROW_WRITE_CALLER_BYTES`를 source census 상수로 두고, C 단계마다 `C 본문 + ROW_WRITE_CALLER_BYTES ≤ charge`를 memory·native W에서 확인한다. A는 D12 계열 selector에서 각 C 호출 지점 몫 ≤ `ROW_WRITE_CALLER_BYTES`와 `producer 호출 지점 몫 + A producer ≤ charge`를 확인한다. 상수와 charge 계산 함수는 C가 정의하고 `storage::disk`로 선택 재노출하며, 상수 값은 S2-a가 C 본문 상계와 함께 고정한다. 상수를 바꾸면 이미 commit한 S2-b1~S2-d의 census(S2W-C01·C02)를 새 값으로 다시 통과해야 한다. C는 A의 producer를 계수하지 않고 A는 C census를 계수하지 않는다.
- 세 cut(`RowPreflightCut`·`RowDraftCut`·`RowPreparedCut`)은 각각 128B 이하이며 const assert로 확인한다. scalar와 32B digest만 담고, 여러 page의 stamp·identity는 하나의 SHA-256 commitment로 접어 담는다. raw stamp 배열은 금지한다. A driver가 wrapper 값을 여러 번 계수하므로(현재 구조에서 preflight cut 3회·draft cut 7회·prepared cut 4회, gate·lease 약10회) cut 1B가 RowWrite에서는 여러 B가 된다.
- S2-a는 세 cut·gate·lease·8번째 part를 최종 배치로 먼저 공급하여 A가 실제 타입으로 측정하게 한다.

2026-10-30 개정(S2-0b, [replay 검산 재배치](#row-write-validate-once)): 위 계수 규칙을 다음과 같이 바꾸고 보충한다.
- A driver 몫의 상한은 둘이다. `ROW_PREFLIGHT_CALLER_BYTES`(6144B)는 preflight 호출 지점에, `ROW_WRITE_CALLER_BYTES`(11520B)는 validation_lane·reserve·producer_lane·prepare·apply·cancel·finish 호출 지점에 쓴다. C는 단계마다 `C 본문 + carrier + 그 단계의 상한 ≤ charge`를 memory·native W에서 확인하고 C 쪽 여유를 출력한다. A는 MCA00-D15에서 각 호출 지점 몫(M3)을 그 단계의 상한과 비교하며, 경로 전체 inventory는 비교 대상이 아니다. 두 상수와 charge 계산 함수는 C가 정의하고 `storage::disk`로 선택 재노출한다. 상수 값은 A 쪽 필요에서 정한 고정값이며 맞는 가장 큰 값으로 바꾸지 않는다. 상수를 바꾸면 이미 commit한 RowWrite 단계 census(S2W-C01·C02)를 새 값으로 다시 통과해야 한다.
- C 본문 상계는 한 호출 안에서도 실제 배타인 부분 단계만 max로 결합한다(§6.9). group carrier를 가진 모든 단계(reserve·prepare·apply·cancel)에서 PreparedRowGroup·Observation의 반환·설치 carrier는 조립 뒤 설치·반환 부분 단계에 더한다. 깊은 검산·연쇄 재검산·group 준비 부분 단계에는 그 호출 동안 중단된 호출 단계마다 그 단계의 반환 carrier(`Result<PreparedRowGroup>`·`Result<Observation>`)를 더한다(최소 하나). 그런 중단 단계를 하나로 두는 것은 S2-b2·S2-c·S2-d의 본문 의무이며 각 단위의 본문 census가 확인한다. preflight의 lease 검산은 완결 호출이므로 다른 부분 단계와 max로 결합한다.
- Row lifecycle(S2W-C03)의 계수는 일부러 그대로 둔다. 그 값은 `RowProgress ≤ CatalogProgressCharge` 상수 확인의 입력인 수용된 측정값이다. 위 규칙은 §6.9를 고치는 것이 아니라 RowWrite census에 적용하는 것이며 다른 class의 계수를 바꾸지 않는다.
- 두 상한 모두 그 호출 지점의 A 몫 전체를 class 안에서 센다. caller frame을 빼지 않으며 cap·charge는 그대로다.

**5. Q06 재검토 기록(Q4).** 측정 Forward producer phase는 60625B(A driver 9835 + A producer 50790, D12 r4)이고, S1-e 증거의 driver 9955B로는 60745B다. 둘 다 재사용 charge 44304B와 class 상한 memory45352/native45216B를 넘는다. cap은 올리지 않고 caller frame을 class에서 빼지 않는다(§6.9, 자원 정본의 "caller carrier의 동시 합"). 대응과 분석 상계(native)는 다음과 같다.

| 항목 | 내용 | 담당 | Forward producer phase |
| --- | --- | --- | ---: |
| 측정 | driver 9955(S1-e) + producer 50790 | A | 60745 |
| M1 | Root after page를 `images::Prepared.root` 대신 `after`에 직접 기록 | A | −16384 |
| M2 | leaf scratch4444·Job1024·StatusHome224·Undo432·descriptor3072·`[u8;640]`·`[u8;512]`·`[u8;256]`을 A scratch로 이동(slice reference 약+100) | A(+C lane) | 약−10500 |
| M6 | 세 cut 128B·gate·lease·8번째 part에 따른 driver 증가(S1-e view mark는 9955에 포함) | C 타입, A 계수 | 약+2200 |
| 합 | M1·M2·M6 뒤 | | 약36070(여유 약8230) |
| M7(조건부) | driver를 단계마다 한 wrapper slot만 사는 모양으로 재구성. 아래 C 단계 조건에서 필수 | A | 약−6700 |

- **CLR producer phase.** 같은 producer(`propose`)를 쓰는 CLR phase는 `live_clr` 7184 + 50790 = 57974B로 charge를 넘는다(D12 r4). M1·M2가 함께 줄이며, S4(C4 rollback)의 공급도 아래와 같은 진입 측정(memory·native, 호출 지점 몫 포함)을 조건으로 한다.
- **S2-e 진입 조건.** A-W1이 S2-a의 최종 타입으로 memory·native W 모두 (a) Forward producer phase ≤ charge와 그 margin, (b) MCA00-D15의 C 호출 지점별 몫 ≤ `ROW_WRITE_CALLER_BYTES`를 PASS로 보고한다. margin이 2048B 미만이면 coordinator가 S2-e 착수 전 추가 slimming을 정한다. (b)가 M7 뒤에도 실패하면 새 Q06 기록이며, `ROW_WRITE_CALLER_BYTES`를 바꾸면 이미 commit한 S2-b1~S2-d의 census(S2W-C01·C02)를 새 값으로 다시 통과해야 한다.
- **C 단계 phase의 위험.** Reserve·Forward 준비·Cancel의 C 본문은 Row lifecycle(43113B native)과 같은 준비 본문(`validate_live`의 history peak 29124B 포함)을 쓰므로 분석 상계가 37–43KB이고, preflight는 retention LOAD 본문(측정 33364/34388B)을 포함해 약35.6–37.6KB다. 여기에 A driver의 그 호출 지점 몫이 더해진다. 현재의 경로 전체 inventory(C 호출 지점 약10.9KB)를 그대로 쓰면 분석 상계로는 모든 효과 단계가 charge를 넘을 수 있으나, 호출 지점별 몫은 그보다 작으므로 초과가 확정된 것은 아니다. 특히 preflight는 A가 아직 wrapper를 갖지 않은 첫 호출이다. M7(C 호출 지점 몫 약4.2KB)이면 preflight·apply는 들어가고 Reserve·Forward 준비·Cancel은 ±3KB 경계다. S2-a는 effect 본문을 쓰기 전에 각 C 단계의 상계를 기존 항과 새 타입으로 계산해 출력한다. S2-b1·S2-b2·S2-c·S2-d·S2-e는 자기 phase의 census가 통과해야 그 경로를 commit한다. S2-a 상계와 A-W1의 호출 지점 몫의 합이 charge를 넘으면 A-W1은 M7을 공급한다. 그래도 넘으면 cap이나 charge field가 아니라 새 Q06 기록으로 C 본문 축소(예: 한 행 안에서 `validate_live`의 위치 재배치, 별도 D2)나 class 재구성을 정한다. 그 기록이 먼저 검토할 선택지는 아래 거절한 대안의 "charge max 접기"다.
- **기존 초과(Pending+Full).** native W의 shared+Pending+Full=66612B>65536B이다(memory 65452B, 여유84B). 따라서 [WRITER-PROGRESS-001](#writer-progress) (c)의 "한 Full read는 들어가고"는 native에서 성립하지 않는다. 정정은 Commit 창 owner의 별도 Q06 기록이며, 이 개정은 cap·charge와 그 문장을 바꾸지 않는다.
- **RowRead.** Full 본문 상계에 B driver의 호출 지점 몫을 더한다. B는 호출 지점 inventory를 공급한다(Q14). census가 full을 넘으면 새 Q06 기록 뒤 전용 charge field(+8B shared)나 축소를 정한다. 쓰기 charge 44304B를 읽기에 재사용하지 않는다. Full을 재사용하므로 Commit Pending 창과의 합은 위 기존 초과를 따른다(native Capacity).
- **2026-10-30 정정(기존 초과).** 위 66612B는 시험 전용 native adapter의 `File`이 129B trace 이름을 값 안에 들고 있어 Full 조회 census에 8번 더해진 값이었다([정정 기록](../evidence/module-independence-survey-20261026.md#native-adapter-file-cr1-scope), `94ede44`). 정정 뒤 native shared+Pending+Full은 65460B(여유76B), memory는 65452B(여유84B)로 둘 다 cap 안이므로 [WRITER-PROGRESS-001](#writer-progress) (c)는 native에서도 성립한다. 위 "기존 초과" 문단과 RowRead 문단의 "native Capacity"는 이 정정으로 대체하며 Commit 창의 별도 Q06 기록은 필요 없다. 여유가 76/84B로 얇다는 점은 그대로 남는다.

2026-10-30 개정(S2-0b): 위 "C 단계 phase의 위험"이 정한 새 Q06 기록의 조건은 S2-a(이 branch `dd9bd31`, census `9aeff2a`)의 상계 출력(reserve·prepare·cancel의 A 몫 여유 218·58·178B, `ROW_WRITE_CALLER_BYTES` 58B)으로 성립했다. 그 기록은 [replay 검산 재배치](#row-write-validate-once)다. 위 표 M7 행의 "아래 C 단계 조건에서 필수"와 "S2-e 진입 조건"의 (b)는 그 기록의 두 상한(`ROW_PREFLIGHT_CALLER_BYTES`·`ROW_WRITE_CALLER_BYTES`)으로 읽는다. M7은 MCA00-D15에서 A 몫이 preflight 밖의 C 호출 지점에서 `ROW_WRITE_CALLER_BYTES`를 넘거나, preflight 호출 지점에서 (E)·(d4)를 거친 뒤에도 `ROW_PREFLIGHT_CALLER_BYTES`를 넘을 때만 필수다.

**6. statement 종료·write bit·한 예약·durable wait(Q1·Q5·Q19·Q15).**
- 성공 statement 종료는 WAL0이며 sequence·mark를 RAM slot에서 닫고 쓰지 않은 6080B 문장 credit을 다음 statement로 넘긴다. 별도 StatementFinish-success group은 없다([소비자 계약](dev3-module-consumers.md#row-shape)의 조건문을 이 결정으로 닫는다). 복구에는 live statement가 없고(§6.6 producer 입력 문단) 다음 mark는 다음 statement 시작의 ATT `undo_head`다.
- table write bit는 reserve에서 table 재검산 뒤 WAL stage 전에 등록하고, apply에서 stage 전에 다시 등록한다. 등록 뒤의 Before는 보수 bit를 남긴다. bit는 Before·Cancel·Fatal·statement 종료에서도 지우지 않는다([격리 owner](dev3-task-10-concurrency.md#module-row-isolation)의 read/write bit 규칙). 따라서 효과가 있으면 bit가 있다.
- actor당 live Row 예약은 하나다([Job3 census](dev3-module-consumers.md#job3-census)의 Reserved≤1). preflight는 source 예약 뒤 깊은 검산에서 RootReserved Job을 가진 actor를 Unchanged WrongState로 거절하고 source→gate→lease를 반환한다. 건강한 session에서는 wrapper의 배타 borrow 때문에 도달하지 않는 방어 검산이다.
- Reserve·Forward·Cancel은 기존 ordinary 설치 본문을 그대로 쓴다. 그 본문은 group마다 설치 뒤 durable wait를 하며 이는 Row가 더한 force가 아니다(Catalog CREATE Reserve와 같은 기존 규칙, §6.6의 "별도 WAL force를 추가하지 않으며"). 소비자 RamOnly cut은 이 wait 동안의 kill로 도달한다. 지연은 후속 관측 항목이며 S3이 Commit ACK durability를 정한다.

**7. V09·MCI03-V02의 oracle 대체와 "합계" 한도의 읽기(Q3).** 진행 class writer(RowWrite·RowProgress·CatalogProgress)는 terminal read gate로 reader와 배타다([WRITER-PROGRESS-001](#writer-progress)의 "선택한 수명"). 그 구간의 동시 합은 `shared + max(writer charge, Σ admitted reader)`에 gate 대기 중 writer 진입 floor를 더한 값이다. 그 floor는 WRITER-PROGRESS-001의 미공급 의무로 남는다. 소비자 계약의 "합계" 한도는 이 뜻으로 읽으며 한도 자체는 그대로 참이다. 반면 MBC10-V09·MBC40-V09의 원 문언(겹친 nested sum·동시 생존)은 writer ≤ memory9592/native9456B를 요구하여 어떤 RowWrite phase도 맞지 않고 gate 결정과 양립하지 않는다. 그래서 **두 selector의 oracle을 다음으로 대체**하고 원 문언은 폐기한다. 대체 oracle의 PASS는 writer와 resident reader 2개의 겹친 admission을 증명하지 않는다.
- MBC10-V09(대체 oracle): resident reader 2개가 admitted된 뒤 writer 요청은 gate에서 두 반환을 기다리고, 그동안 새 reader는 `InstallationPending`이며, 반환 뒤 writer가 예약한다. −1/동일/+1은 시험 전용 ledger charge로 남은 몫을 제한해 마지막 admission에 적용한다(소비자 MCA05의 "test admission의 남은 몫을 제한" 선례). cap 변경은 없다.
- MBC40-V09(대체 oracle): census는 (i) shared + 반환 오류가 최대인 writer phase(caller carrier 포함), (ii) shared + resident 2개, (iii) 두 집합이 gate로 동시에 admitted되지 않음, (iv) gate 대기 중 writer 진입 floor + shared + resident 2개를 따로 출력한다. (iv)는 floor 공급 전 PASS로 세지 않는다. rollback·recovery task의 같은 대체는 S4와 C4가 정한다.
- 소비자 MCI03-V02(barrier 대체): 행 진행 중 B의 read operation은 `InstallationPending`이므로 문언의 "B dirty load와 A apply 재검사" 교차는 행 안에서 생기지 않는다. barrier는 A preflight 전(preflight는 B의 반환을 기다린다)이나 apply 뒤에 둔다. prepare와 apply 사이의 변화는 S2W-R21의 두 시험 hook으로 검산한다. source stamp가 바뀌면 같은 Prepared의 Before 뒤 reopen·cancel이고, 원 시나리오의 결과에 해당하는 쓰기 target 비상주는 Fatal(Invariant)이며 fence가 남는다. 두 변화 모두 gate·lease·배타 MutationPort 아래에서는 cfg(test) 주입으로만 만들 수 있다([소비자 합류 표](dev3-module-consumers.md#acceptance)).
- 행 진행 중 `InstallationPending`을 받은 Row 읽기의 재시도는 이 개정이 소비자(B·MCI)에게 새로 더하는 의무이며, 그 담당의 수용을 D2 기록에 적는다.

**거절한 대안.**
- (Q1) StatementFinish-success group — 바뀌는 영속 상태가 없고, 소비자 계약이 가짜 동일 before/after Native를 금지한다.
- (Q2) 새 charge field — shared를 늘려 native recovered 여유 32B와 memory `pending_full` 여유 84B를 줄이고 모든 class 상한을 낮춘다. 측정 초과가 남을 때의 마지막 대안이다.
- (Q3) V09 문언 유지 — writer ≤ native9456B가 필요해 어떤 RowWrite phase도 맞지 않는다.
- (Q4) cap 증액 — §11 Q06이 금지한다. caller frame 제외 — §6.9와 자원 정본에 어긋난다.
- (Q6) gate 없이 source만 예약 — 이미 admitted된 reader가 writer를 Capacity로 굶기고, 뒤에 온 reader도 Capacity라 동시성 이득이 없다.
- (Q7 b) 서명을 유지하고 A가 Before 뒤 flush — Reserve 뒤 eviction이 남아 Cancel의 Before가 A driver에서 Drop·fence로 이어진다.
- (Q7 c) producer 동안 pin 유지 — 대여 전 guard 해제 규칙(§6.6 lane 문단)에 어긋난다.
- lease 아래 C가 victim을 직접 flush — 쓰기 gate·source 창 안에 flush I/O와 durable wait를 넣고, CREATE lease의 "lease 아래 flush I/O 불가" 성질을 깬다.
- `FlushRequired`를 `RetentionError`에 실음 — target을 실을 수 없다. `TableStep`·`RollbackStep`·`ReadStep`과 같은 step 값을 쓴다.
- C가 상주를 적재하지 않고 비상주 target을 그대로 반환 — A에는 LOAD 경로가 없고 flusher는 dirty victim만 처리한다.
- XOI를 크기에 따라 frame tail이나 lane에 둠 — 한 단계가 두 배치를 가진다. Row layout은 언제나 lane이다.
- 7-tuple을 유지하고 별도 accessor를 더함 — 동결 표면이 둘이 되고, 나중의 이름 정리가 다시 A를 깨뜨린다.
- Forward observer scratch로 replay WORK_END tail 재사용 — history의 borrowed workspace를 live 단계에 섞는다. writer 자신의 loan 구간이 있다.
- cut에 raw stamp 배열 — 128B를 넘고 A driver에서 여러 배로 늘어난다.
- (Q19) gate 전 actor 검산 — ADMISSION-002의 깊은 검산 순서와 S1-e `transaction_check` 선례에 어긋난다.
- M2 scratch를 candidate 구간의 남는 tail(약14KB)에 둠 — 8번째 part를 피하지만 `preflight_row` 서명 변경 때문에 A와의 공동 commit은 어차피 필요하다. A scratch의 크기가 cell 최대(자원 정본의 2228B, profile에 따라 바뀜)에 묶이고, 남는 약3.5KB로는 M2′(4072B)를 담지 못하며, observer가 선언 길이만큼 검산하는 입력 구간에 무관한 bytes를 섞는다.
- 상주 적재를 gate 전의 별도 read 단계로 분리 — LOAD는 어느 class에서든 그 호출 지점의 A 몫과 함께 admission되어야 한다. Full은 LOAD 본문 위로 1206B(34570−33364, 35594−34388)만 남고 RowWrite는 9916–10940B가 남는다. 그 호출 지점의 A 몫과 단계 carrier가 1206B를 넘는다면(측정 전의 추론이며 MCA00-D15의 preflight 몫이 확인한다) LOAD를 preflight에서 빼는 것은 더 좁은 class로 옮기는 것이고, 그 class를 넓히려면 새 field나 gate 없는 progress admission(Q6에서 거절)이 필요하다. 거절은 다음 재시도 논거만으로도 선다. 그 단계와 gate 사이에 admitted reader의 LOAD가 clean target을 evict할 수 있어 재시도에 상한이 없다. gate 아래에서는 dirty victim만 preflight를 멈추고 flush가 하나씩 정리하며, 배타 MutationPort 아래에서 다른 foreground writer가 그 victim을 다시 dirty로 만들지 않으므로 재시도가 그때의 dirty page 수로 유한하다. C 단계 위험을 정하는 것은 Reserve·Forward 준비·Cancel이며 preflight가 아니다. gate 안의 읽기 I/O는 최대9page이고 같은 gate가 group마다의 durable wait(6항)도 덮으므로, 그 지연은 후속 관측 항목으로 둔다.
- RowWrite phase를 `CatalogProgressCharge`의 max에 접고 `charge ≤ 65536 − shared`를 명시 검산("charge max 접기") — field·cap 변경 없이 memory1048/native912B를 더 쓰게 한다. 그러나 S1-e가 `.max(row_lifecycle)`를 빼고 상수 확인으로 바꾼 결정(커진 Row 본문이 모든 Catalog progress 예약을 조용히 올리지 않고 이름으로 실패)과 반대다. RowWrite 항을 `catalog_live` peak에 두면 recovery cleanup 항(약880B를 더함)의 한도 memory44472/native44336B에 걸려 여유가 168/32B로 준다. 여유가 ±3KB 불확실성보다 작아 계획(M7·단위별 census)도 바꾸지 않는다. 측정 초과가 이 범위 안이면 새 Q06 기록이 먼저 검토할 선택지로 남긴다(명시 검산과 `catalog_live` 밖의 census 배치를 조건으로).

**거절 결과.** 효과 전 거절은 WAL·page·issuer·Jobs 변경0이며 실제 관측한 read-set은 유지한다.

| 경로 | phase | cause | effect·반환 |
| --- | --- | --- | --- |
| Catalog view·읽기 전용 view | Prepare | Unsupported | Unchanged. gate·source·read-set 전 |
| 다른 pool/storage의 flusher | Prepare | BindingMismatch | Unchanged. gate 전 |
| flush I/O origin 불일치 | Prepare | BindingMismatch | CallerMustResolve·fence. gate 전 |
| flush phase가 Ready가 아님 | Prepare | Busy | Unchanged. gate 전 |
| Ready인데 capture·durable·authority·retired 잔존 | Prepare | Invariant | CallerMustResolve·fence |
| flush I/O preflight 오류 | Prepare | `Flush(…)`(원 오류) | 원 effect. gate 전 |
| gate(`close_reads_for_terminal`) | Prepare | Concurrency(InvalidRole·Busy·InstallationPending) | Unchanged |
| source 부족 | Census | Capacity | Unchanged. gate 재개 |
| preflight의 Free Undo 없음 | Prepare | Capacity | Unchanged. source→gate→lease 반환 |
| 상주에 dirty victim 필요 | — | — | `Ok(FlushRequired{target})`. 효과·등록0 |
| preflight LOAD의 Unchanged claim·frame 거절 | Load | Busy·Membership·Stale 등 원 cause | Unchanged. source→gate→lease 반환 |
| preflight LOAD의 page I/O 오류 | Load | PageIo | Fatal·fence |
| 그 밖의 preflight LOAD 오류 | Load | 원 cause(page decode `Page(…)`, frame stamp 누락·검증 불일치의 Invariant, frame 오류의 Fenced·Poisoned 등) | 원 effect(Fatal·fence). Unchanged로 바꾸지 않는다 |
| RootReserved를 가진 actor | Prepare | WrongState | Unchanged. source→gate→lease 반환 |
| stored Index(MBC30-V04-S) | Load | Invariant | Fatal·fence |
| preflight Ready 뒤 target 비상주 | 그 단계 | Invariant | Fatal·fence |
| reserve의 위조 candidate·leaf 부족·바뀐 source | Prepare | InvalidInput·Capacity·Stale | Before(같은 PreflightRow) |
| prepare·apply·cancel의 stage 전 거절 | 그 단계 | 원 cause | Before(같은 wrapper) |
| complete stage·partial install·sync Unknown 뒤 | Stage·Install·Durable | 원 cause | Fatal. active proof Drop은 fence |

2026-10-30 보충(S2-0b, [replay 검산 재배치](#row-write-validate-once)): 위 표에 다음 행을 더한다. preflight의 행은 모두 read bit·catalog flag 등록과 cut 기록 전이다.

| 경로 | phase | cause | effect·반환 |
| --- | --- | --- | --- |
| preflight `validate_live`의 거절(live ≠ replay, WAL 재읽기·replay 실패) | Source | 원 cause(control·XOI 불일치는 Invariant, frame replay 거절은 그 frame의 cause) | Fatal·fence(own-WAL 손상의 결과, [RowReserve replay 개정](#row-reserve-replay)의 "live" 항목) |
| preflight `validate_live`의 WAL writer·file lock 경합 | Source | Busy | Unchanged. source→gate→lease를 반환하며 등록0이다. 위 "live" 항목의 Fatal은 이 경합에 쓰지 않는다 |
| preflight replay scope 열기·checkout 실패 | Source 등 원 phase | scope: history 건강 검산의 원 cause, `Concurrency(…)`. checkout: 먼저 건강 검산의 원 cause(Poisoned·Fenced)와 replay state lock의 Busy, replay state 정합 위반의 Invariant. 그다음 Busy·Exhausted, 같은 owner가 이미 active replay key를 가지면 Invariant | 원 effect. scope의 `Concurrency(…)`는 그 오류의 effect다. checkout의 Busy·Exhausted는 Unchanged이며 source→gate→lease를 반환하고 등록0이다. 건강 검산 실패와 Invariant는 Fatal·fence(CallerMustResolve)다 |
| preflight arena 불일치(T page ≠ replay arena) | Census | Invariant | Fatal·fence |
| reserve의 preflight commitment 차이(page·control digest 포함) | Prepare | Stale | Before(같은 PreflightRow), 효과0 |
| prepare·cancel의 draft commitment 차이(page·control digest 포함) | Prepare | Stale | Before(같은 ReservedRow), 효과0 |
| reserve 설치 뒤 pool read·draft commitment 계산 실패 | Install | 원 cause | Fatal·fence. 효과 뒤이므로 Unchanged로 바꾸지 않는다 |

**이 개정의 자원.** cap·class 상한·private425984B·long49152B·metadata188544B·whole254080B를 바꾸지 않는다. 더하는 것은 wrapper field 둘(gate 약32B·lease 8B), 128B 이하 cut 셋, lane reference 하나(8B), `RowPreflightStep` carrier다. 모두 RowWrite phase census와 A driver 계수에 들어가며 shared는 바뀌지 않는다. preflight의 LOAD scratch는 기존 frame 구간을 먼저 쓰므로 새 bytes가 아니다.

**시험.** 신규 selector는 다음과 같다. 이름은 ID 접두 test 함수이며 모두 NOT_RUN이다.

| ID | 사례 |
| --- | --- |
| S2W-R01 | Catalog view·읽기 전용 view·stale view의 preflight가 gate·source 전에 Unchanged이고 read-set이 그대로다 |
| S2W-R02 | 시험 전용 charge로 source 부족: gate가 다시 열리고 효과0 |
| S2W-R03 | preflight가 read bit·catalog flag를 등록하고 finish가 source→gate→lease를 반환하며 효과0 |
| S2W-R04 | 위조 candidate(digest·길이·schema epoch)의 reserve가 Before이고 효과0 |
| S2W-R05 | Free Undo container가 없거나 후보가 allocator generation 불일치·slot0 비Free로 모두 제외되면 preflight가 Unchanged/Prepare/Capacity이고 source→gate→lease를 반환한다. reserve 재검산 변화는 Before |
| S2W-R06 | reserve 성공: RowReserve frame SHA가 독립 literal과 같고 write bit와 issued identity가 맞다 |
| S2W-R07 | reserve의 재검산에 한 번만 다른 source stamp를 보이는 cfg(test) 주입이면 reserve가 같은 PreflightRow의 Before(Stale, 효과0)이고 validation_lane부터 재시도해 성공한다 |
| S2W-R08 | Reserve→Cancel: frame SHA, RowId·sequence·high-water 미반환, write bit 유지, private 1회 반환 |
| S2W-R09 | Reserve→producer_lane→A produce→prepare→apply: Forward SHA = literal, Retained, view의 mark·sequence |
| S2W-R10 | A frame·after·roles를 변조한 prepare가 같은 Reserved로 Before이고 이어서 cancel한다 |
| S2W-R11 | apply stage 전 fault가 Before→reopen→cancel이고 같은 예약이다 |
| S2W-R12 | stage 뒤 fault가 Fatal이고 fence가 남으며 active proof Drop이 fence한다 |
| S2W-R13 | 두 statement×3행: 둘째 statement mark = ATT head, 6080 credit은 첫 statement의 첫 Reserve에서 확보하고 둘째 statement는 넘겨받는다 |
| S2W-R14 | Catalog Job을 가진 Row actor가 Reserve 전에 거절된다 |
| S2W-R15 | Reserved 보유 중 reader가 InstallationPending이고 preflight가 admitted reader의 반환을 기다린다 |
| S2W-R16 | 비상주 target은 C가 적재하고, dirty victim이면 `FlushRequired{target}`·효과0·read bit0·source/gate/lease 반환, flush 뒤 Ready |
| S2W-R17 | flusher binding 불일치 Unchanged, flush I/O origin 불일치 fence, phase WaitingWal Busy, Ready인데 capture 잔존이면 fence, I/O preflight 오류는 `Flush(…)` |
| S2W-R18 | 시험 fixture의 RootReserved actor가 WrongState이고 source→gate→lease를 반환한다 |
| S2W-R19 | preflight LOAD의 claim 거절(Busy 등)은 Unchanged/Load로 source→gate→lease를 반환하고, page I/O 오류와 page decode 오류는 각각 원 cause의 Fatal/Load이며 fence가 남는다 |
| S2W-R20 | preflight는 gate 뒤 read operation을 열지 않는다. 진행 중 같은 core의 read operation 표시가 0이고 LOAD claim은 성공한다 |
| S2W-R21 | prepare와 apply 사이의 두 cfg(test) 주입. gate·lease·배타 MutationPort 아래에서는 C 밖의 어떤 경로도 이 변화를 만들 수 없다. (1) apply의 stage 직전 재검산에 한 번만 다른 source stamp를 보이면 apply가 같은 Prepared의 Before(Stale, WAL·page·issuer 변화0)이고, A는 reopen 뒤 cancel하며 cancel은 성공한다. (2) clean 쓰기 target 하나의 frame을 비우면 apply가 Fatal(Invariant)이고 fence가 남는다. 소비자 MCI03-V02의 대체 검산이다 |
| S2W-N01 | compiler 거절: Row wrapper가 사는 동안 같은 flusher 사용 |
| S2W-H01..H05 | Reserve·Forward·Cancel 뒤 `validate_live`, Reserved·Forward 상태 crash의 interim gate 정지, Forward·Cancel의 durable replay = live |
| S2W-C01·C02 | memory·native W에서 RowWrite phase별 census와 `C 본문 + ROW_WRITE_CALLER_BYTES ≤ charge` 출력 |
| S2W-C03 | `RowLoanLayout` 분리 뒤 Row lifecycle 42089/43113B 불변 |
| S2W-C04 | live peak 64000/64136B·recovered peak 65368/65504B 불변 또는 변화 근거 |
| S2W-C05 | private 여덟 구간의 비중첩·합425984, 세 cut ≤128B const assert |
| MCA00-D15(A) | A driver의 C 호출 지점별 몫 ≤ `ROW_WRITE_CALLER_BYTES`, Forward producer phase ≤ charge와 margin(memory·native, S2-a 실제 타입). CLR producer phase도 출력한다(S4 입력). S2-e 진입 조건 |
| MCB00-D13(B) | B driver의 C 호출 지점별 몫(S5 census 입력) |
| S5R-R01..R08·S5R-C01..C03 | Row 읽기 admission·빈 table·FlushRequired·capture·sequence overflow·status/missing·copy·Catalog 불변, RowRead census ≤ full |

읽기가 바뀌는 기존 selector는 MBC00-P01(flusher 인수와 step 두 분기, lease의 Drop impl 부재), MBC00-P05(A scratch mutable borrow), 소비자 MCA05-V31(preflight 재시도), `catalog_source_memory`·`catalog_source_native`(기대값 9792)다. oracle을 대체하는 기존 selector는 MBC10-V09·MBC40-V09와 소비자 MCI03-V02다(위 7항). MCI03-V02의 대체 검산은 S2W-R21이다. 독립 literal은 `tests/fixtures/dev3-module-c3/row-write/`와 `tests/fixtures/dev3-module-c2/row-read/`의 stdlib author가 만든다. author는 N1 author를 import할 수 있고 제품 코드는 import하지 않는다.

2026-10-30 등록(S2-0b, [replay 검산 재배치](#row-write-validate-once)): 위 표에 S2W-R22·S2W-R23·S2W-R24·S2W-C06을 더하고 S2W-C01·C02와 MCA00-D15의 읽기를 바꾼다. 사례와 읽기는 그 기록의 "시험" 문단이 정한다. S2W-H01..H05·S2W-R07·S2W-R21의 기대는 바뀌지 않는다.

**공급 단위와 순서.**
- **S2-0.** 이 개정과 아래 참조 문단·자원 정본·소비자 계약 문단이다. 문서만 바꾸며 독립 D2를 받는다.
- **Q12 정정.** `catalog_source_*` 기대값 정정(자원 정본의 2026-10-29 판정)은 S2-a 전의 작은 시험 commit이다.
- **S2-a(C).** S1-e 병합과 S2-0 수용 뒤. class와 arm, 최종 wrapper field·cut 배치·lane 상수, 8-tuple·서명과 A의 `Boundary::preflight`·`LiveCall` 적응(위 3항의 한 commit), `ROW_WRITE_CALLER_BYTES`, effect 본문 전 C 단계별 상계 출력이다.
- **A-W1(A).** S2-a 뒤. M1·M2, 위 5항 조건의 M7, D12 호출 지점 보고(M3), MCA00-D15, 전용 flusher Owner와 `continue_flush`를 쓰는 `FlushRequired` 뒤 재시도다.
- **S2-b1·S2-b2·S2-c·S2-d(C).** S2-a 뒤 차례로. 각 단위는 자기 phase census가 통과해야 그 경로를 commit한다. S2-c는 S1-g에도 기댄다.
- **S2-e(C).** S2-d와 위 5항 진입 조건((a) Forward producer phase, (b) MCA00-D15 PASS) 뒤.
- **S2-f(C).** S2-e 뒤의 시험·census·증거다.
- **S5-a~S5-d(C).** S2-a 뒤 병렬이다. S5-d는 B inventory(MCB00-D13)가, 행을 가진 읽기 사례는 S2-e가 필요하다.
- **되돌림.** 새 것부터 되돌린다. S2-a는 S2-b~f·S5-a~d·A-W1을 모두 되돌린 뒤에만 되돌리며, 위 3항의 공동 commit은 A 적응과 함께 되돌린다. S2-0은 이에 기댄 코드 단위를 모두 되돌린 뒤 단독으로 되돌린다.

<a id="row-write-validate-once"></a>
#### 2026-10-30 개정 — Row 쓰기 C 단계의 replay 검산 재배치와 A 몫 상한 분리(Q06 재검토)

**2026-10-30 · S2-0b 계약 개정 · 독립 D2 수용(2026-10-30, r3) · 코드 미공급(공급 단위 S2-a′·S2-b1~S2-d·A-W1).**
- 이 기록은 [Row 쓰기 개정](#row-write-budget) 5항의 "C 단계 phase의 위험"이 예고한 새 Q06 기록이다. 그 조건은 S2-a(이 branch의 `bbfec10..dd9bd31`, census는 S2-a branch head `9aeff2a`)의 단계 상계 출력으로 성립했다.
- 이 기록은 다음 문단을 보충하고 바꾼다: 1항의 preflight 순서, 4항의 상수와 계수 규칙, 5항의 M7 조건, 거절 결과 표, 시험 표, 공급 단위.
- 다음은 바꾸지 않는다: cap, class 상한, `CatalogProgressCharge`, port field, private 425984B 배치, cut 크기. caller frame도 class에서 빼지 않는다.
- 아래 "측정 근거" 밖의 수치는 분석 상계이며, S2-a′의 census 출력이 대체한다.
- 설계 근거는 [S2-0b 계약 반영](../evidence/module-independence-survey-20261026.md#s2-0b-validate-once)에 둔다.

**D2 검토 기록.** 독립 읽기 전용 검토 3회다. r1은 ACCEPT WITH REQUIRED CHANGES(필수4·권고12), r2는 ACCEPT WITH REQUIRED CHANGES(필수1·권고3), r3는 ACCEPT(필수0·권고5)이며 r3 권고 A2·A3·A4는 이 본문과 관련 문단에 반영했다. r3 권고 A1(S2W-R24 주입의 실행 세부)은 S2-c가 그 byte를 정할 때 쓰는 설계 메모로 두었다. 회차별 대응과 그 메모의 위치는 [S2-0b 계약 반영](../evidence/module-independence-survey-20261026.md#s2-0b-validate-once)에 있다.

**측정 근거.** [S2-a 증거](../evidence/module-independence-survey-20261026.md#s2a-row-write-surface)의 S2W-C01·C02 출력이다(`9aeff2a`의 r5, 이 branch `dd9bd31`의 i1에서 같은 값). memory와 native 값은 같다.
- caller 0일 때 단계별 C total(본문+단계 carrier, B)은 다음과 같다.

| preflight | validation_lane | reserve | producer_lane | prepare | apply | cancel | finish |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 36742 | 6106 | 44086 | 11034 | 44246 | 36528 | 44126 | 1658 |

- 그래서 `ROW_WRITE_CALLER_BYTES`는 58B다. 남는 A 몫 여유는 reserve·prepare·cancel이 각각 218·58·178B, apply가 7776B다.
- A의 D12 r5 경로 전체 inventory 11355B와 M7 추정 약4.2KB는 모두 reserve·prepare·cancel에 들어가지 않는다.
- reserve·prepare·cancel의 C 본문 42878B는 다음 셋의 합이다.
  - shell 1146B.
  - group carrier 7328B: `Result<PreparedRowGroup>` 3개와 N=9 확장 3개, `Result<Observation>` 3개, `RowLifecycle` 2개, `ExistingGroup`.
  - group 준비 34404B: scalar 1480 + outer(N=9) 3424 + replay scope carrier 376 + `history::validate_live`의 history peak 29124B.
- apply의 C 본문 34984B는 shell 1146 + group carrier 7328 + 깊은 검산 `check` 26510B다. `cf9d40c`가 `check`를 reserve·prepare·apply·cancel에, `group_reassemble`을 apply에 더했다.
- history peak는 outer 2648 + File guard 16 + initialize 26460B다.
  - 그 아래의 population 25743·Row 25707·retention 24239B arm도 1.4KB 안에 몰려 있다.
  - 이 값들은 typed source 값의 합이다. page 크기 버퍼는 이미 replay loan(1572864B)과 writer private loan에 있다.

**결정.**
1. **replay 검산 재배치(C 본문 축소).**
   - `history::validate_live`와 쓰기 target 집합 T의 arena 동일성 검산은 한 행에 한 번, preflight에서 한다.
   - reserve·prepare(Forward)·cancel은 replay scope를 열지 않고, 아래 digest 연쇄로 그 증명을 이어받는다.
   - apply의 stage 직전 재검산은 그대로다. 그 재검산이 draft `source`를 다시 계산하면 page·control digest도 함께 계산한다. apply 본문은 깊은 검산 항이 정하므로 여유는 바뀌지 않는다.
2. **부분 단계의 결합(계수 정정).** RowWrite C 본문 상계를 다음처럼 계수한다.
   - group carrier를 가진 모든 단계(reserve·prepare·apply·cancel)에서 PreparedRowGroup·Observation의 반환·설치 carrier는 조립 뒤 설치·반환 부분 단계에만 더한다.
   - 깊은 검산·연쇄 재검산·group 준비(producer·observer·XOI) 부분 단계에는 그 호출 동안 중단된 호출 단계마다 그 단계의 반환 carrier(`Result<PreparedRowGroup>`·`Result<Observation>`)를 더한다(최소 하나).
   - **본문 의무.** S2-b2·S2-c·S2-d에서 group 준비와 깊은 검산 동안 Row group 반환 carrier를 쥔 중단 단계는 하나뿐이다(wrapper 본문이 준비 함수의 결과를 받는 slot). wrapper 본문과 준비 함수 사이의 helper는 그 준비를 가로질러 group이나 Observation을 반환하지 않는다. 각 단위의 본문 census는 중단 단계와 그 carrier를 이름으로 적는다. 단계가 더 필요한 본문은 그 수만큼 세고 아래 대체 순서를 따른다.
   - preflight의 lease 검산(`check_catalog_pair_io`)은 완결 호출이므로 다른 부분 단계와 max로 결합한다.
   - 이것은 §6.9의 "실제 배타 phase만 max로 결합한다"를 RowWrite 단계에 적용한 것이다. 선례는 Catalog census다. 안쪽 공통 준비 함수는 replay 동안 `PreparedCatalog`를 세지 않고(`catalog_live.rs`의 "There is no PreparedCatalog until replay/reader/inventory have ended"), 그 호출자들은 준비 전체에 걸쳐 자기 반환 carrier를 센다(`commit_prepare_source_bytes`의 `Result<PreparedCatalog>` 2개, Row lifecycle의 `row.carriers`).
   - Row lifecycle(S2W-C03 42089B)의 계수는 일부러 그대로 둔다. 그 값은 `RowProgress ≤ CatalogProgressCharge` 상수 확인의 입력이며 이미 수용한 측정값이다. 이 결정은 §6.9의 정정이 아니라 RowWrite census에만 쓰는 적용이다.
   - 제품 본문은 바뀌지 않는다. census 코드(`RowWriteBodies`·`row_write_phases`·census 시험)는 S2-a′에서 바뀐다.
3. **A 몫 상한 분리.**
   - `ROW_PREFLIGHT_CALLER_BYTES = 6144`: preflight 호출 지점에 쓴다. A가 아직 Row wrapper를 갖지 않는 첫 호출이다.
     - 근거: A의 preflight 몫 추정은 약4.0–4.5KB다(statement carrier 약2.2KB, slack 1408B, `Result<PreflightNext>` 2개 약320B, `insert_row` 인수).
     - 여기에 A-W1의 flush 재시도 경로와 M1·M2의 slice reference(약+100B)를 위한 여유 1.6KB 이상을 더해 1KiB 단위로 올렸다.
   - `ROW_WRITE_CALLER_BYTES = 11520`: validation_lane·reserve·producer_lane·prepare·apply·cancel·finish 호출 지점에 쓴다.
     - 경로 전체 inventory 11355B를 256B 단위로 올린 값이다. 그 inventory도 들어가지만 여유는 165B(M1·M2 뒤 약65B)다.
     - MCA00-D15가 비교하는 것은 4항 M3의 호출 지점별 몫이며 경로 전체 inventory가 아니다. 호출 지점별 몫은 그 호출에서 죽은 slot을 세지 않으므로 경로 전체 inventory보다 작다.
     - D12의 `machine_debug_insert`(11552B)는 비최적화 machine stack 관측이며 source 상한과 비교하지 않는다(D12 문서).
   - 두 값은 "맞는 가장 큰 값"이 아니라 A 쪽 필요에서 정한 고정값이다.
4. **M7.** M7은 MCA00-D15에서 A 몫이 preflight 밖의 C 호출 지점에서 `ROW_WRITE_CALLER_BYTES`를 넘거나, preflight 호출 지점에서 (E)·(d4)를 거친 뒤에도 `ROW_PREFLIGHT_CALLER_BYTES`를 넘을 때만 필수다. 5항 표 M7 행의 "아래 C 단계 조건에서 필수"를 이것으로 대체한다.

**preflight 순서와 cut.** 1항 "순서"의 깊은 검산 부분은 다음 차례로 진행한다.
1. 깊은 검산.
2. 상주 보장(S2-0의 retention LOAD 본문). dirty victim이 필요하면 4의 replay 전에 `FlushRequired`를 반환한다.
3. Free Undo 선택.
4. `history::validate_live` 1회. replay scope는 replay buffer의 checkout이며 read operation이 아니다.
5. T의 typed read. 각 target을 private의 before 구간에 복사하고 replay arena와 byte 단위로 비교한다. 불일치는 지금과 같은 Fatal(Census, Invariant)이다.
6. read bit·catalog flag 등록.
7. cut 기록.

**commitment에 넣는 것과 순서.**
- 세 cut의 크기는 그대로다(56·96·80B).
- `RowPreflightCut.source`와 `RowDraftCut.source`는 `ContentHasher`의 SHA-256이다. 다음 순서로 넣는다.
  1. domain tag `"XRWC"` 4byte, version 1byte(1), 종류 1byte(1 preflight·2 draft).
  2. S2-a cut 문서가 정한 항목, 그 문서의 순서대로.
  3. T의 page digest(32B씩), before 순서대로.
  4. `ControlCut`의 field(bank·jobs·inventory digest, issued, adopted), 선언 순서대로.
- preflight의 page digest는 위 5의 before 사본에서 계산한다. arena와 비교한 바로 그 bytes다.
- draft의 page digest는 설치 뒤 pool page를 typed read하여 계산한다. private의 after image에서 계산하지 않는다. 그래서 pool을 읽는 prepare·cancel과 같은 것끼리 비교한다. 같은 read가 다음 단계의 before 사본을 새로 채운다.
- S2-a′는 이 순서와 항목을 cut 문서에 적는다.

**digest 연쇄.**
- **reserve.**
  - replay 없이 T를 typed read로 before 구간에 다시 복사하며 digest를 계산하고, preflight commitment를 재계산해 비교한다.
  - 값이 다르면 같은 PreflightRow를 Before(Stale, 효과0)로 반환한다(S2W-R07 그대로).
  - 설치와 durable wait를 마치면 위 순서로 draft commitment를 기록한다.
- **lane.**
  - validation_lane과 producer_lane은 연쇄를 검산하지 않는다(page·control digest를 계산하지 않는다).
  - lane 발급은 효과가 아니다. 발급 전후의 변화는 다음 효과 단계가 효과 전에 거절한다: reserve의 commitment 재계산과 candidate 검산(S2W-R04), prepare의 commitment 재계산과 observer의 독립 재계산.
  - S2-a′는 lane이 `source`를 검산한다는 cut 문서의 두 문장을 이에 맞게 고친다. 문맥 값을 cut의 남는 72B에 scalar로 둘지는 S2-a 문서대로 S2-b가 정한다.
- **prepare(Forward)·cancel.** replay 없이 T를 typed read로 before 구간에 다시 복사하고 draft commitment를 재계산해 비교한다. 값이 다르면 같은 ReservedRow를 Before(Stale)로 반환한다(S2W-R21의 첫 hook 그대로).
- **차이가 계속될 때.** gate·lease·배타 MutationPort 아래에서 commitment 차이는 cfg(test) 주입으로만 생긴다. 보장하는 것은 다음 셋이다.
  1. commitment와 다른 bytes로 WAL·page·issuer 효과를 만들지 않는다. 차이는 효과 전의 Before(Stale)다.
  2. 손상된 dirty page는 flush의 `prove_capture`가 거절한다.
  3. 손상된 clean page는 쓰이지 않는다. flush하지 않고, 그 page를 T에 넣는 다음 preflight나 group 준비의 arena 비교가 거절한다.
  - 다음 행의 preflight가 반드시 fence한다고는 보장하지 않는다. `validate_live`는 control과 XOI만 대조하고, arena 비교는 그 preflight의 T만 덮는다. reserve의 Before 뒤 A는 preflight를 finish하고 다시 preflight하지 않을 수 있다.
- **Before로 두는 이유.** "stamp는 같고 내용이 다름"을 Fatal로 가르려면 cut마다 별도 32B stamp digest가 필요하다. draft cut이 96→128B(상한), preflight cut이 56→88B가 된다. A가 이를 각각 7회·3회 세므로 A 몫이 약320B 늘어 경로 전체 inventory가 11520B를 넘는다. 그래서 Before로 둔다.
- **비용.**
  - 한 행의 전체 WAL replay는 2회(Reserve 준비, Forward 또는 Cancel 준비)에서 preflight 1회로 준다. 비상주 target을 적재하는 LOAD의 replay는 따로 센다.
  - 대신 효과 단계마다 T(최대 9page)와 control의 digest를 계산한다.

**live·replay 동치와 받아들인 위험.**
- 다음은 바뀌지 않는다: WAL frame byte, replay arm, N1/S1-g literal, 복구 본문.
- 바뀌는 것: 지금은 모든 Row group 준비가 다음 frame을 만들기 전에 live == replay(durable prefix)를 다시 증명한다. 이 기록 뒤에는 한 행의 Reserve frame(F1)을 같은 행의 Forward·Cancel frame(F2) 전에 다시 증명하지 않는다. 다음 증명은 다음 `validate_live`(다음 행의 preflight, replay를 유지하는 Commit·RollbackStart·AbortEnd의 group 준비)다.
- **어긋남의 두 갈래.** F1의 live 설치가 같은 온전한 F1의 replay와 코드 결함으로 다르다고 하자.
  - **live끼리 맞지 않는 어긋남.** 예: control bank의 Jobs cell이 Jobs page와 다르다. 이 기록 뒤에도 live Forward·Cancel 준비가 F2 전에 거절한다. `row::verify_census`가 bank의 모든 cell을 live Root page의 `job_digest`·`budget_digest`와 대조하고, 선택한 Jobs page를 bank와 cell 단위로 대조하기 때문이다.
  - **live끼리는 맞고 replay와만 다른 어긋남.** 이 D2가 받아들이는 위험은 이 갈래뿐이다. ATT에서는 `history::row::record`가 frame 밖에서 가져오는 값이 이 갈래다: ATT의 `ACTOR`·`LOCATION` field가 되는 `Observation.actor`·`Observation.status`와 받은 ATT slot이다. Aging에는 그런 원천이 없다. Jobs cell은 `record`가 Jobs after page에서 복사하므로, Jobs의 어긋남은 page 경로로 온다(만든 Jobs after page와 Root `job_digest`·`budget_digest`가 함께 어긋남).
  - **어느 갈래도 아닌 것: ATT·Aging descriptor 자체.** `record`는 그 descriptor를 frame에서 decode한 layout에서 복사하므로 live와 replay가 같은 bytes를 쓰고, `observe`는 두 descriptor를 byte 단위로 비교한다. 그 descriptor에서 live `produce_borrowed`와 replay `observe_borrowed`가 어긋나면 F1 자체가 replay되지 않으며, 결과(분석이 F1에서 멈춤, CallerMustResolve)는 이 기록 전후가 같다.
- **live끼리는 맞는 어긋남의 검출.**
  - **지금:** 같은 행의 Forward·Cancel 준비가 F2 전에 찾아 fence한다. control·XOI의 어긋남은 `validate_live`가 Fatal(Source, Invariant)로, page에만 있는 어긋남(예: census digest 밖의 Root bytes)은 group마다의 arena 비교가 Fatal(Census, Invariant)로 찾는다. WAL은 F1에서 끝나고 F1은 그대로 replay된다.
  - **이 기록 뒤:** F2가 어긋난 live 상태에서 만들어져 stage·설치되고 durable이 된다. F2의 before(Root `job_digest`·`budget_digest`, before descriptor digest)는 live 상태와 같다.
    - F2가 어긋난 bytes를 before로 접으면 F2는 replay되지 않는다. 다음 `validate_live`는 F2를 replay하다가 Fatal(Source, c)로 fence한다. c는 F2의 replay cause다(Row observer이면 대개 `RowSemantic(…)`). 재시작 분석은 F1을 replay한 뒤 F2에서 거절한다(Analysis·`Retention(c)`·CallerMustResolve).
    - F2가 그 bytes를 접지 않으면 F2는 replay되고, 다음 `validate_live`의 control 비교(Fatal(Source, Invariant))나 그 page를 덮는 다음 arena 비교(Fatal(Census, Invariant))가 지금처럼 찾는다.
  - [ROW-BEGIN-001](#row-begin-capsule)의 임시 분석 gate가 모든 Row WAL을 멈추는 동안에는 두 경우 모두 READY0·CallerMustResolve다. cause만 Unsupported와 `Retention(c)`로 다르다.
  - **C4가 그 gate를 대체한 뒤에는 결과가 다르다.** 지금은 F1이 replay되고 C4가 RootReserved 행을 CancelReserved로 끝내 READY에 이른다. 이 기록 뒤에 F2가 어긋난 bytes를 접었다면 replay할 수 없는 durable F2가 WAL에 남아 CallerMustResolve(수동 해결)다. 이 기록은 durable frame도 하나 더 만든다.
- **WAL 손상과 RAM 손상.** F1 자체의 WAL 손상은 두 경우 모두 분석이 F1에서 멈춘다. 단계 사이 T page나 control의 RAM 손상은 digest 연쇄가 효과 전에 거절한다.
- **받아들인 위험.** 이 D2는 live끼리는 맞는 어긋남에 대한 위의 C4 뒤 차이를 의도해 받아들인다. 근거는 다섯이다.
  1. 행 안 frame별 live == replay는 S2W-H01..H05(각 단계 뒤 `validate_live`)가 시험으로 증명한다.
  2. Commit·RollbackStart·AbortEnd는 replay를 유지하므로 replay할 수 없는 WAL을 덮는 Commit ACK는 없다(아래 S3 의무).
  3. F2 전의 행 단위 검산을 되살리는 후보는 모두 A 몫에서 bytes를 쓴다(아래 거절·보류한 대안).
  4. C4가 진입 때 다시 정한다.
  5. (부분) live끼리 맞지 않는 어긋남은 live Forward·Cancel 준비의 census digest·Jobs page 대조가 F2 전에 여전히 거절한다.
- **C4 진입 의무.** [§7](#common-deliveries) 표 뒤의 2026-10-30 문단이 정한다.
- **S3 의무.** [ROW-BEGIN-001](#row-begin-capsule)의 "범위 밖" 뒤 2026-10-30 문단이 정한다.

**효과(분석 상계, memory=native).** C total은 C 본문과 단계 carrier의 합이다. C 쪽 여유는 44304 − C total − 그 단계의 상한이다.

| 단계 | S2-a C total | 이 기록 뒤 C total | A 몫 상한 | C 쪽 여유 |
| --- | ---: | ---: | ---: | ---: |
| preflight | 36742 | 36086 | 6144 | 2074 |
| validation_lane | 6106 | 6106 | 11520 | 26678 |
| reserve | 44086 | 31467 | 11520 | 1317 |
| producer_lane | 11034 | 11034 | 11520 | 21750 |
| prepare(Forward) | 44246 | 31627 | 11520 | 1157 |
| apply | 36528 | 29200 | 11520 | 3584 |
| cancel | 44126 | 31507 | 11520 | 1277 |
| finish | 1658 | 1658 | 11520 | 31126 |

- **preflight.** 새 validate 항(scalar + scope + history peak + `Result<()>` 2개, 약31060B)은 기존 LOAD 항 32852B보다 작다. 그래서 C total은 lease 검산을 max로 결합한 몫(−656B)만 준다.
- **reserve·prepare·cancel.**
  - replay를 뺀 group 준비는 약26993B(scalar + outer + Row observer 약22089B)다.
  - 여기에 중단 단계 하나의 반환 carrier 약2120B를 더한다.
  - 그 값(약29113B)을 깊은 검산(26510B)·설치 부분 단계(7328 + 18935 = 26263B)와 max로 결합한다.
- **apply.** 깊은 검산 26510B가 정한다. 반환 carrier와 재조립(2120 + 13897B)·설치 부분 단계(26263B)는 그보다 작다.
- **불확실성.**
  - observer·ledger 추정은 약±0.5KB다.
  - 중단 단계가 Observation도 반환하면 reserve·prepare·cancel의 여유가 296B씩 준다.
  - 중단 단계가 하나 더 있으면 2120B씩 줄어 세 단계 모두 음수다. 그래서 2항의 본문 의무를 둔다.

**측정 조건과 대체 순서.**
- **S2-a′.** S2-a′의 S2W-C01·C02·C06(memory·native)이 위 상계를 대체한다. 어느 단계의 C 쪽 여유가 음수이면, 상수를 바꾸기 전에 다음 순서로 진행한다.
  1. **preflight:**
     - (E) 상주 LOAD가 같은 preflight에서 검산한 arena page로 두 home read를 비교하게 하여 LOAD 안의 replay를 없애고, T 배열을 replay 뒤에 만든다. 1항 "상주 보장"의 LOAD 본문과 LOAD 권한을 바꾸므로 별도 D2가 필요하다.
     - (d4) 그다음 history `initialize` 항을 실제 `initialize` closure로 좁힌다(약−717B). Catalog·복구와 공유하는 census이므로 별도 D2가 필요하다.
     - "(E)·(d4)를 거친다"는 각 D2가 받아들여 적용했거나 거절했다는 뜻이다. 적용하면 preflight 여유가 분석 상계로 (E) 뒤 약11.5KB, (d4)까지 약12.3KB가 되며, `ROW_PREFLIGHT_CALLER_BYTES`를 새 여유 − 1024B 안에서 올릴 수 있다.
  2. **나머지 단계:** (d3) group 항을 단계별 실제 target 수로 계산한다(약−0.5–2.5KB). 이 기록이 미리 승인한다. 그 수는 그 단위 본문이 만들 수 있는 최대 target 수이며 const assert로 고정한다.
  3. 그래도 음수이면 새 Q06 기록이다.
- **A-W1.**
  - MCA00-D15는 memory·native W에서 두 가지를 확인한다: preflight 호출 지점 몫 ≤ `ROW_PREFLIGHT_CALLER_BYTES`, 나머지 C 호출 지점 몫 ≤ `ROW_WRITE_CALLER_BYTES`.
  - 몫은 4항의 M3대로 호출 지점마다 센다.
  - M7은 위 결정 4의 조건에서만 필수다. M7 뒤에도 넘으면 새 Q06 기록이다.
- **S2-b1~S2-d.** 각 단위는 자기 단계의 실제 본문 census가 위 상한으로 통과해야 그 경로를 commit한다. S2-b2·S2-c·S2-d의 census는 결정 2의 중단 단계를 이름으로 적는다.
- **CLR.** §6.7 rollback task의 CLR 준비는 이 기록의 대상이 아니다. S4가 진입 측정 때 같은 재배치(rollback step 발급에서 1회)를 정한다.

**거절·보류한 대안.** 아래 여유는 모두 분석 상계다.
- **`validate_live`·history의 borrowed workspace**(writer private에서 비어 있는 XOI·C scratch·A scratch 구간 사용).
  - replay의 page는 이미 1572864B replay loan에 있고, 남은 byte 배열은 arm마다 1KB 미만이다.
  - typed 값은 raw loan 규칙상 loan에 둘 수 없다.
  - peak는 네 arm이 함께 정한다. 그래서 복구와 공유하는 replay 본문을 넓게 바꾸어도 줄어드는 양은 약0.7–1.5KB다.
- **`validate_live`를 reserve 전의 validation_lane 발급이나 별도 C 호출에서 실행.** reserve·prepare·cancel에 대한 효과는 이 기록과 같다. 대신 A가 wrapper를 가진 상태에서 replay하는 단계가 하나 더 생긴다. preflight에 두면 LOAD 항 아래라 추가 비용이 0이다.
- **F2 전의 행 단위 검산을 유지하는 넷.** C4가 다시 볼 후보다.
  - (H) 단계마다의 `validate_live`를 group 준비 함수 밖으로 올려, group의 outer 배열과 반환 slot이 생기기 전 첫 부분 단계로 둔다. 연쇄가 필요 없고 지금의 group별 증명을 그대로 지킨다. reserve·prepare·cancel 여유는 10890·10730·10850B이고, (d4)까지 더해도 prepare 11447B다. 상한이 약9728B가 되어 A의 경로 전체 inventory가 어느 호출에도 들어가지 않는다.
  - (B′) 이 기록에 더해 reserve 끝(설치와 durable wait 뒤)에서 `validate_live`. reserve 여유가 약10.2–10.7KB다. (H)보다 여유가 적고 연쇄도 필요하다.
  - (ii) reserve와 producer_lane 사이의 별도 C 호출 `ReservedRow::validate()`. `&mut self`이면 그 단계 여유가 약12.0KB, 값으로 받으면 약11.0KB다. §6.6 동결 표면, A 호출 모양, compiler fixture를 바꾸고, prepare·cancel이 검산되지 않은 예약을 거절해야 한다.
  - (i) preflight의 replay loan을 wrapper가 계속 쥐고 F2 전에 새 F1만 `apply_frame`. frame arm이 약28.4KB라 준비 안에서는 prepare 여유가 약6.0KB다. 준비 전 별도 부분 단계로 두어도 약11.5KB이며, 하나뿐인 replay buffer를 A의 lane 동안 wrapper가 쥔다.
- **reserve나 prepare·cancel 한쪽에만 `validate_live` 유지.** 남은 쪽의 여유가 218·58·178B 그대로다.
- **검산 전용 admission이나 class.** A frame은 모든 C 호출 아래에 있으므로 호출별 합이 줄지 않는다. 새 charge field는 Q2에서 거절했다.
- **"charge max 접기".** 늘어나는 여유가 최대 memory1048/native912B로, 필요한 약11KB에 못 미친다.
- **상수 하나 유지.** 상한이 preflight 여유(약8.2KB)에 묶인다. 다른 단계가 남기는 12.6KB 이상을 쓰지 못하고, A의 경로 전체 inventory가 어느 호출에도 들어가지 않는다.
- **`ROW_WRITE_CALLER_BYTES = 12288`(A의 D12 driver 상한).** reserve·prepare·cancel의 C 쪽 여유가 549·389·509B로, 추정 불확실성(약±0.5KB) 안이다.
- **commitment 차이를 Fatal로 처리.** 위 "Before로 두는 이유"와 같다.

**시험.** 다음 selector를 더한다. 모두 NOT_RUN이다.

| ID | 사례 |
| --- | --- |
| S2W-R22 | 모든 target이 상주한 행의 두 흐름(Reserve→producer→prepare→apply, Reserve→Cancel)에서 `RuntimeHistory::replay_scope`와 `ReplayScope::checkout`의 cfg(test) 계수기로 history replay checkout을 센다. preflight는 1회, validation_lane·reserve·producer_lane·prepare·apply·cancel은 0회다. 비상주 target의 LOAD replay는 따로 출력한다 |
| S2W-R23 | C wrapper를 직접 부르는(A driver를 거치지 않는) 계속되는 cfg(test) 손상 주입 두 가지. (1) preflight Ready 뒤, 모든 행의 T에 드는 clean page(Root) 하나의 1byte를 stamp 변화 없이 바꾼다. reserve는 Before(Stale)이고 효과0이다. `finish` 뒤 곧바로 다음 preflight를 부르면 arena 비교에서 Fatal(Census, Invariant)이며 fence가 남는다. (2) Reserve 설치 뒤 control 1byte를 commitment를 다시 접지 않고 바꾼다. prepare와 이어지는 cancel이 모두 Before(Stale)이고, 그 사이 WAL·page·issuer 변화가 없으며, ReservedRow의 Drop이 fence를 남긴다 |
| S2W-R24 | live끼리는 맞는 어긋남의 늦은 검출과 재시작 결과. **주입:** control만 바꾸면 live Forward 준비(`verify_census`, Jobs page·bank 대조)가 F2 전에 거절하므로, live끼리 맞는 주입을 쓴다. 권장형은 Reserve 설치 안의 cfg(test) hook이다. frame stage 뒤 pool 설치와 `history::row::record` 전에, 만든 Jobs after image의 RootJob cell 1byte를 바꾸고 Root after image를 다시 encode하여 `job_digest`(그 byte가 aggregate에 들면 `budget_digest`도)를 맞춘다. `record`가 그 cell을 control로 복사하므로 pool과 control은 서로 맞고 F1의 replay와만 다르다. draft commitment는 설치 뒤 live에서 계산하므로 다시 접을 필요가 없다. 같은 효과의 대안형은 설치 뒤 hook이 pool Jobs page와 `control.jobs`의 같은 byte를 바꾸고, page stamp를 그대로 둔 채 Root digest를 다시 encode한 다음 draft commitment를 다시 접는 것이다. 바꾸는 byte는 F2가 before로 접고, live Forward 검산이 다른 원천과 묶지 않는 byte다(`prepare`·observer가 plan·actor·Status·XOI projection과 같기를 요구하는 Job field는 피한다). S2-c가 시험과 함께 그 byte를 정한다. **live 기대:** producer_lane·producer·prepare·apply가 성공하고 F2가 durable이다. 다음 행의 preflight는 `validate_live`가 F2를 replay하다가 Fatal(Source, c)이며 fence가 남고 그 행의 효과는 0이다. c를 출력하고, 아래 `observe_recovery_frame`이 F2에 보고한 cause와 같음을 단언한다(Invariant로 고정하지 않는다). **재시작 기대:** 분석은 F2에서 멈춘다: Analysis·`Retention(c)`(같은 c)·CallerMustResolve·database Unchanged·READY0·mutations0. 임시 gate는 Row frame replay 뒤에 있으므로 replay되지 않는 frame이 먼저 분석을 멈춘다. 같은 WAL을 `observe_recovery_frame`의 frame별 durable 본문으로 지나가면(S1g H13 방식) F1은 받아들이고 F2는 거절함을 단언하고, F2의 LSN과 c를 출력한다 |
| S2W-C06 | memory·native W에서 단계별로 다음을 출력한다: C total, 그 단계의 상한, C 쪽 여유, 중단 단계 수와 반환 carrier, replay 없는 group 준비 항, preflight의 validate 항과 LOAD 항 |

- S2W-C01·C02는 `cf9d40c`의 단계별 `C 본문 + carrier + 상한 ≤ charge` 단언을 유지하되, 각 단계에 그 단계의 상한을 쓴다. 상수마다 그 상수를 쓰는 단계들의 두 W 최소 여유 이하임을 단언한다.
- MCA00-D15는 A의 preflight 호출 지점 몫을 `ROW_PREFLIGHT_CALLER_BYTES`와, 나머지 C 호출 지점 몫을 `ROW_WRITE_CALLER_BYTES`와 비교한다.
- S2W-H01..H05는 그대로이며 행 안 frame별 동치의 주 증거다. S2W-R07·S2W-R21의 기대도 바뀌지 않는다: 한 번만 다른 stamp는 commitment 차이가 되어 같은 wrapper의 Before로 끝난다.

**공급 단위와 되돌림.**
- **S2-0b.** 이 기록과 관련 문단이다. 문서만 바꾸며 독립 D2를 받는다.
- **S2-a′(C).** S2-0b 수용 뒤, S2-b1 전에 S2-a branch에 새 commit으로 공급한다(amend 없음).
  - 공급하는 것: 결정 1·2의 census 구조(reserve·prepare·apply·cancel과 그 깊은 검산 부분 단계 포함), 두 상수와 `storage::disk` 재노출, 상수 문서, cut 문서(commitment 항목·순서·domain tag, draft digest의 pool read, lane이 연쇄를 검산하지 않음), S2W-C01·C02의 단계별 상한 단언과 여유 출력, S2W-C06.
  - effect 본문은 없다. Row lifecycle(S2W-C03)과 peak(S2W-C04)는 바뀌지 않는다.
- **S2-b1.** preflight의 replay·arena 검산과 commitment를 공급한다.
- **S2-b2·S2-c·S2-d.** replay 없는 digest 연쇄를 공급하고 결정 2의 본문 의무를 지킨다.
- **A-W1.** MCA00-D15가 두 상수를 읽는다.
- **되돌림.** 새 것부터 되돌린다.
  - S2-a′는 이에 기댄 S2-b1~S2-d와 A-W1의 D15 적응을 되돌린 뒤에만 되돌린다.
  - S2-0b는 S2-a′까지 되돌린 뒤 단독으로 되돌린다. S2-0b는 S2-0보다 먼저 되돌린다.
  - S2-0b를 되돌리면 S2-0의 기준(단계마다 `validate_live`를 실행한다는 가정, `ROW_WRITE_CALLER_BYTES` 58B)이 다시 적용된다.

<a id="row-write-s2-supply"></a>
#### 2026-10-30 S2 공급 기록 — 실제 census, 계약 읽기와 문언 정정, 뒤 단계 의무

**2026-10-30 · S2-a′~S2-f 공급 기록 · 독립 검토 수용(2026-10-30, r2; r1 ACCEPT WITH REQUIRED CHANGES 필수 4·권고 11, r2 필수 1 반영).** 이 기록은 [replay 검산 재배치](#row-write-validate-once)·[Row 쓰기 개정](#row-write-budget)의 공급 단위(S2-a~S2-f, A-W1) 증거가 "다음 계약 수정 때" 적기로 한 값과 읽기를 모은다. 새로 정하는 것은 아래 "C 자기 검산" 거절 행과 MCA00-D15의 frame-static coordinator 규칙 둘뿐이다. 그 밖에 단위 검토가 받아들인 두 문언을 고친다: [replay 검산 재배치](#row-write-validate-once) digest 연쇄의 reserve 항목(읽기 1)과 S2W-R23(1)의 Root 기대(읽기 3)다. 넷 모두 이미 적용한 코드를 문서로 고정한다. 공급 증거는 [S2 증거](../evidence/module-independence-survey-20261026.md#s2f-closeout)에 있다.

**실제 효과(memory = native, charge 44304).** 위 "효과(분석 상계)" 표를 다음 census 출력이 대체한다(S2W-C01·C02·C06). finish는 S3의 단계라 상계 그대로다.

| 단계 | 실제 C total | A 몫 상한 | C 쪽 여유 | 중단 단계(결정 2) |
| --- | ---: | ---: | ---: | --- |
| preflight | 37048 | 6144 | 1112 | — |
| validation_lane | 15400 | 11520 | 17384 | — |
| reserve | 31989 | 11520 | 795 | `reserve_group.group` |
| producer_lane | 26761 | 11520 | 6023 | 없음 |
| prepare(Forward) | 30838 | 11520 | 1946 | `prepare_forward.observation` |
| apply | 31366 | 11520 | 1418 | `forward_group.group` |
| cancel | 31205 | 11520 | 1579 | `cancel_group.group` |
| finish(상계) | 1658 | 11520 | 31126 | — |

- 상수·cap·charge는 바꾸지 않았다. 사다리((E)·(d4)·(d3))는 쓰지 않았고 결정 4의 M7 조건은 성립하지 않았다(A-W1 MCA00-D15).
- 관례: 본문이 끝난 뒤 갈래(`arms`)는 그 단계가 부르는 함수 frame과 반환값만 max로 결합한다. 본문 동안 frame에 사는 wrapper 지역값은 shell에 더한다(apply의 `reserved`·`observation`). 이는 결정 2가 적은 §6.9 적용의 관례이며(S2-c·S2-d 통합 검토 A1, `cb9df49`), apply의 31366/1418은 그 뒤 값이다.
- 가장 작은 쓰기 단계는 reserve(795B)다.

**MCA00-D15(최종 타입, caller 항 포함).** S2-e 시험 호출자(`insert_live`) 모양으로 잰 출력이다(memory = native, round e3). caller 항 1296B는 driver frame 아래 호출자가 가진 값이다(행 2개·값 1개, Owner 둘 48B 포함, 행 하나마다 96B). 모든 `insert.*` site에 더하고, rollback·transaction site에는 더하지 않는다(그 호출자는 S3·S4의 것이다).

| site | 상한 | 호출 지점별 + caller | 여유 | frame-static + caller | 여유 |
| --- | ---: | ---: | ---: | ---: | ---: |
| `insert.check` | 11520 | 3936 | 7584 | 4651 | 6869 |
| `insert.preflight` | 6144 | 4560 | 1584 | 5331 | 813 |
| `insert.try_flush` | 11520 | 4624 | 6896 | 5716 | 5804 |
| `insert.continue_flush` | 11520 | 4616 | 6904 | 5708 | 5812 |
| `insert.validation_lane` | 11520 | 5704 | 5816 | 7419 | 4101 |
| `insert.reserve` | 11520 | 5560 | 5960 | 6971 | 4549 |
| `insert.finish_preflight` | 11520 | 5240 | 6280 | 6747 | 4773 |
| `insert.producer_lane` | 11520 | 8032 | 3488 | 9163 | 2357 |
| `insert.forward`(A, charge, A producer 19260 포함) | 44304 | 25708 | 18596 | 26815 | 17489 |
| `insert.prepare` | 11520 | 5528 | 5992 | 7091 | 4429 |
| `insert.apply` | 11520 | 5728 | 5792 | 8371 | 3149 |
| `insert.reopen` | 11520 | 5496 | 6024 | 8219 | 3301 |
| `insert.cancel` | 11520 | 5808 | 5712 | 8731 | 2789 |
| `insert.finish_statement` | 11520 | 4576 | 6944 | 5779 | 5741 |

가장 작은 여유는 frame-static + caller의 `insert.preflight` 813B(행 약 8개)이고, 쓰기 site에서는 `insert.producer_lane` 2357B, `insert.cancel` 2789B다. rollback·transaction site의 호출 지점별 몫은 S2에서 바뀌지 않았다(가장 작은 여유 `rollback.clr_lane` 5296B). frame-static은 S2-e부터 단언하며 가장 작은 여유는 같은 site의 3776B다(caller 항 없음). `insert.validate`·`rollback.clr`(A)은 출력만 한다.

`insert.try_flush`·`insert.continue_flush`는 결정 3의 어느 목록에도 없고 D15가 "나머지 C 호출 지점"으로 11520과 견준다(A-W1 검토 A4). 둘 다 6144 안이다(frame-static + caller 여유 428·436B).

- 계약의 몫은 M3의 호출 지점별 몫이다(그 호출에서 죽은 slot은 빼고 caller frame은 빼지 않는다). D15는 이를 계약 oracle로 계속 단언한다.
- **coordinator 규칙(M3보다 엄격).** 함수 frame은 진입 때 잡히므로, 각 C 호출 지점의 frame-static 몫도 그 상한 안이어야 한다. frame-static 몫은 slack, 그 호출에 이르는 경로의 A frame마다의 자기 slot 전부(그 호출에서 죽은 slot 포함), forwarding frame의 인수와 반환의 합이다. 경로 밖 callee frame은 max로 결합하므로 결정 3이 비교에서 뺀 경로 전체 inventory(D12 `carrier_bytes`)가 아니다. 자기 slot을 세는 규칙은 A의 `call_sites` 문서가 정한다. D15는 단언하는 모든 site에서 이 몫을 caller 항 없이 단언하고, `insert.*` site에서는 caller 항을 더해서도 단언한다.
- 이 규칙은 Q06 기록이나 M7 조건이 아니며 상수·cap·charge를 바꾸지 않았다. A는 Ready 뒤 단계(`write_row`)와 그 각 단계(`reserve_row`·`validate_row`·`forward_row`·`produce_row`·`prepare_row`·`apply_row`), statement 끝(`end_statement`)을 inline되지 않는 함수로 나눠 각 호출 지점의 frame에 다른 단계의 slot이 남지 않게 했다(나머지 never-inline frame은 `call_sites` 문서). 이 규칙은 뒤 A 호출 지점(S3·S4·제품 호출자)에도 같다.
- caller 항은 시험 호출자(행 2개를 stack에 가진 모양)에 맞춘 값이다. 제품 호출자가 생기면 그 단위가 D15를 다시 돌린다.

**계약 읽기와 문언 정정.**
1. **(문언 정정, 보장 같음) [replay 검산 재배치](#row-write-validate-once) digest 연쇄의 reserve 재계산.** "T를 typed read로 before 구간에 다시 복사하며 digest를 계산하고"를 "T를 typed read 하나로 pool page에서 읽어 같은 `PreflightSource::commitment`를 접고(BEFORE·SCHEMA는 쓰지 않는다)"로 고친다. digest가 같고 BEFORE는 타입으로 쓰기가 막혀 있으므로 같은 보장이다. prepare·cancel의 kind-2 재계산(`draft_recompute`)은 문언대로 BEFORE에 다시 복사한다.
2. **S2W-R14.** 거절은 reserve 입구의 기존 깊은 검산(Fatal(Census, Invariant), fence)이다. live 경로에서 Row actor가 Catalog Job을 가질 수 없어 cfg(test) hook으로 닿는다.
3. **(selector 기대 정정) S2W-R23(1) Root.** Root byte의 기대 "다음 preflight를 부르면 arena 비교에서 Fatal(Census, Invariant)"를 "다음 preflight의 Root 해독에서 Fatal(Census, RowSemantic)"로 고친다. page 해독이 끝 SHA-256을 검산하고 preflight가 arena 비교 전에 Root에서 RootJob을 고르기 때문이다. arena 비교의 Fatal(Census, Invariant)은 같은 주입을 Row Status page에 한 사례와 preflight 단독 사례(`s2w_r23_preflight_arena_mismatch_is_fatal`)가 보인다. reserve의 Before(Stale)·효과 0과 fence는 그대로다.
4. **S2W-R05 reserve 절반.** commitment 재계산이 Free Undo 재검산보다 먼저이므로 Stale이다.
5. **index 수.** reserve는 retired index까지 센다. "A producer가 읽는 전체 데이터와 borrowed codec"의 `index_count` 문장("table identity에 맞춰 계산")보다 크거나 같은 보수값이며 producer capacity 입력에만 쓴다. index 수가 capacity 밖의 판정에 쓰이면 다시 정한다.
6. **statement head.** prepare·apply·cancel은 statement head를 검산하지 않는다. actor가 이 예약으로 RootReserved이고, draft commitment가 ATT 행과 Job 64행을 접으므로 Reserve 뒤 변화는 Stale이 된다. Forward·Cancel은 같은 `reserved_entry`를 쓴다.
7. **S2W-R18.** 건강한 session에서는 도달하지 않는 방어 검산이므로 예약을 남긴 채 wrapper를 놓는 cfg(test) fixture로 닿는다.
8. **Fatal 뒤 charge.** Fatal cancel·apply 뒤 admission charge는 fence된 ledger가 checked release를 거절하므로 보인 채 남는다([Row 쓰기 개정](#row-write-budget) 1항(gate)의 "원 Fatal은 fence 아래 charge를 보존한다"). durable 효과 뒤 반환 오류는 fence 아래에서만 생긴다(`release_discharged` 문서).
9. **S2W-R12·R23(2).** R12는 단계를 정하지 않으므로 apply와 cancel 두 절반을 둔다. R23(2)의 control byte는 XOI 구간 첫 byte(131072)다.
10. **S2W-R24.** 바꾸는 byte는 Root cell offset 72의 free cursor다. cursor는 Aging 서술자(control 32행)에 사본과 work digest가 있어 Reserve 안의 권장형 hook은 Reserve 자신이 Before(Prepare, Base(Aging(IdentityMismatch)))로 잡는다. 그래서 대안형을 Root page와 그 control 사본(Aging 서술자)에 적용한다: durable Reserve 뒤 pool Root cursor 이동, Aging 서술자 재encode, draft commitment 재접기. "다른 원천과 묶지 않는 byte" 조건은 사본을 함께 바꿔 맞춘다(S2-0b r3 A1 메모의 후보, S2-c가 정함). live 다음 행 preflight와 frame별 durable replay와 재시작 분석 모두 RowSemantic(Corrupt)다. C4가 interim gate를 바꿀 때 이 byte·주입형·cause를 다시 쓴다.
11. **MCA05-V31.** A driver의 live preflight 재시도는 C 시험 `row_live::mca05_v31_live_dirty_victim_flush_retries_the_same_row`가 공급한다(A의 소비자 selector를 C 시험 tree에서 공급).
12. **MBC10-V09·MBC40-V09 대체 oracle([Row 쓰기 개정](#row-write-budget) 7항).**
    - MBC10-V09(RowWrite writer)는 `catalog_memory::row_v09`이 공급한다. resident reader 둘이 admitted된 동안 writer가 gate에서 기다리고, 그동안 새 reader는 InstallationPending이며, reader를 하나씩 놓아 둘 다 반환한 뒤에야 writer admission의 −1·같음·+1이 정해진다(같음은 65536 − shared = memory 45352). RowProgress writer는 S1E-V09와 `row_progress_reserves_the_same_catalog_progress_charge`가 맡는다. CatalogProgress writer의 V09 사례는 NOT_RUN이다.
    - MBC40-V09는 `plan4_apply::row::v09_census_tests`가 census 코드에서 읽어 출력한다(cap은 ledger 상수 `SHORT_BYTES`). "반환 오류가 최대인 writer phase"는 두 읽기로 출력한다. Err 타입이 가장 큰 단계는 apply(`RowOwnedError<PreparedRow>` 520B)이고, C total + 그 상한이 가장 큰 단계는 reserve(31989 + 11520 = 43509)다. 단계 carrier가 Err 반환 slot을 포함하므로 뒤가 앞을 덮으며, 시험은 total(apply) ≤ total(reserve)를 단언한다. shared + reserve는 memory 63693·native 63701, shared + resident 둘은 55944·55952(여유 9592·9584)다.
    - (iii) RowWrite와 reader의 배제는 측정이 아니라 class 사실로 출력하며, 증명은 S2A-T01(구조)과 MBC10-V09·S2W-R15(동작)다. (iv)는 `floor=NOT_SUPPLIED`로 출력하고 PASS로 세지 않는다(WRITER-PROGRESS-001). 그래서 MBC40-V09는 부분 공급이다. rollback·복구 task 부분은 S4·C4다.
    - 소비자 MCI03-V02의 대체 검산은 S2W-R21이다. 실제 MCI03-V02 실행은 B의 Row 읽기(S5) 뒤라 NOT_RUN이다.
13. **insert_live.** S2에서는 제품 호출자가 없다. S2-e가 C 시험에서 A driver live mode를 실제 C wrapper로 돌려 G2를 닫는다.
14. **apply의 독립 재계산.** [Row 쓰기 개정](#row-write-budget) 3항의 "(§6.6 prepare·stage 직전 재검산)"은 다음으로 읽는다. apply는 observer 없이 loan에서 group을 재조립하고 prepared `after` digest로 검산한다. observer의 독립 재계산은 prepare에서 한다. S2-0b의 apply census(`group_reassemble`)가 이미 이를 전제한다.

**native 수치(측정 갱신).** `94ede44`가 native 시험 adapter의 `File`을 160 → 32B로 줄인 뒤 native shared는 20320 → 20192B다([Row 쓰기 개정](#row-write-budget) 5항의 "2026-10-30 정정(기존 초과)"과 같은 원인). 그래서 이 문서의 native 값은 다음처럼 측정값으로 읽는다. memory 값은 모두 그대로다.
- shared 20320 → 20192(Row 쓰기 개정 머리말), Row lifecycle 43113 → 42089(머리말·5항·S2W-C03, memory와 같다), RowRead full 35594 → 34570(머리말·4항), retention LOAD 34388 → 33364(5항·거절한 대안).
- native class 상한 45216 → 45344(5항, §11 2026-10-29 Q06 재검토 기록), writer 여유 9456 → 9584(7항·거절한 대안 Q3, §9 2026-10-29 개정의 MBC10-V09 행).
- native recovered 여유 32 → 160B(4항·거절한 대안 Q2), "charge max 접기"의 native 912 → 1040B와 `catalog_live` 한도 44336 → 44464B(여유 32 → 160B), 상주 적재 분리 대안에서 RowWrite에 남는 몫 9916–10940 → 10940B.
- S2W-C03의 Row lifecycle은 memory·native 모두 42089B 불변으로, S2W-C04의 native peak는 64136/65504 → 64008/65376으로 읽는다([RowReserve replay 개정](#row-reserve-replay)의 "그대로" 목록도 같다).

**시험 표 보충.** S2W-N01(compiler 거절)은 계약 표에 공급 단위가 없었다. S2-a가 C0 boundary-compiler manifest에 더해 공급했다(E0499 1건).

**C 자기 검산 거절(보충).** [Row 쓰기 개정](#row-write-budget)의 거절 표(와 S2-0b 보충)에 다음 행을 더한다. 두 행 모두 지금 C가 그런 출력을 만들지 않아 시험이 닿지 않는다(S2-d 통합 열린 점). "prepare·apply·cancel의 stage 전 거절" 행은 A의 입력과 상태의 거절(Stale·Busy·Capacity·InvalidInput·A proposal의 RowSemantic)이다.

| 경로 | phase | cause | effect·반환 |
| --- | --- | --- | --- |
| reserve·prepare·cancel에서 C가 만든 출력의 자기 검산 불일치(producer observation, T 밖 role 변화, T target과 다른 after image identity, 범위보다 긴 frame, C 출력 page·layout 해독과 commitment가 증명한 before image 해독) | Prepare | Invariant(해독은 원 cause) | Fatal·fence. stage 전이라도 Before로 바꾸지 않는다 |
| 예약 entry의 RootJob 해독 실패 | Census | Retention(…) | Fatal·fence. 전체 ledger 검산 뒤이므로 방어 갈래다 |

**[Row 쓰기 개정](#row-write-budget) 2항 `flush_then_retry`의 C 표면.** A는 phase를 `MaintenancePort::phase()`로 읽고 그 반환형 `RetentionFlushPhase`를 `storage::disk` 재노출로 쓴다(A-W1 `c21f5f4`).

**뒤 단계 의무.**
- **S3·S4 공통.** rollback·transaction site와 제품 호출자는 자기 caller 항으로 MCA00-D15(호출 지점별·frame-static)를 다시 돌린다.
- **S3.** finish 본문과 census. Cancel 뒤 statement 종료와 `abort_empty_transaction`이 fence 없이 성공하는 동작을 명시할지 정한다. Forward 뒤 건강한 transaction 끝(commit)이 없으므로 S2 시험은 statement를 끝낸 뒤 transaction을 버린다.
- **S4.** `reserved_entry`는 Forward(INSERT) draft만 받는다. 취소할 수 있는 CLR 예약을 들이면 기대 연산을 인수로 받는다. CLR producer phase(MCA00-D15 출력)를 진입 측정한다. MBC10-V09·MBC40-V09의 rollback task 부분([Row 쓰기 개정](#row-write-budget) 7항)도 S4다.
- **C4.** S2W-H01..H05의 crash 정지 부분과 S2W-R24의 재시작 기대는 interim gate가 바뀐 뒤 다시 돌린다. [§7](#common-deliveries) 표 뒤 2026-10-30 문단의 진입 의무(S2-0b 뒤 `validate_live` 배치)와 MBC10-V09·MBC40-V09의 복구 task 부분([Row 쓰기 개정](#row-write-budget) 7항)도 C4다.
- **S5·MCI.** MCI03-V02의 실제 실행과 B/MCI의 InstallationPending 재시도 수용은 B의 Row 읽기 뒤다.
- **담당 미정.** MBC10-V09의 CatalogProgress writer 사례는 NOT_RUN이며 담당 단위가 없다. MBC40-V09 (iv) floor는 [WRITER-PROGRESS-001](#writer-progress)의 미공급 의무다.
- **열린 의무(A-W1 검토 A4, 담당 미정).** flush 동안 RowWrite admission은 없고 flusher의 65536B census에는 호출자 carrier 항이 없다. 그 census가 A의 flush 지점 몫(frame-static + caller 5716B)을 포함하는지는 S2-b 증거가 정하기로 했으나 S2 증거에 그 결정이 없다. C와 coordinator가 정한다.

<a id="row-commit-s3"></a>
#### XADB-MODULE-ROW-COMMIT-001 — S3 Row Commit과 종료

2026-10-31 · **S3 한정 구현·선택 검증·최종 D1/D2 ACCEPT**. 사용자 S3 클라우드 개발 지시와 별도 branch·단계별 commit 지시에 따라 `6ec2d343`에서 `codex/s3-row-commit`을 만들었다. 설계·구현·계약·통합 담당 `/root`, 독립 검토 `/root/s3_independent_review`다. 이 절은 S2 마감의 다음 공급 단위이며 S4 rollback, S5 Row 읽기, C4/S6 READY 복구 또는 전체 M4/M5 인수를 시작하지 않는다.

**기존 공급과 변경.** `PreflightRow::finish`의 source→gate→lease 반환과 Row `finish_statement`의 원 actor/mark 검산·WAL0 종료는 이미 구현되어 있다. S3는 그 실제 census와 Cancel 뒤 종료 의미를 확정하고, 착수 기준에서 Unsupported였던 `commit_row`를 실제 Commit에 연결했다. 공개 wire·format 번호·상수·cap·charge는 바꾸지 않는다.

1. **capsule 검산.** Catalog/read-only capsule은 gate·source 이전 Unsupported, 원 origin/slot generation 불일치와 active statement는 효과 전 거절이다. Row Active, 원 ATT/TerminalLeased와 일치하는 actor, Reserved root 없음이 필요하다. Serializable은 원 read/write mask와 catalog 관측을 실제 table/catalog epoch로 검증한다. 충돌은 같은 capsule을 MustAbort로 고정한 RollbackRequired이며 S4의 full abort 의무를 유지한다. RC는 Serializable 검사를 생략하되 같은 source·terminal·epoch 검산을 수행한다. 유일 `&mut RetentionLedger`를 검증부터 종료까지 보유한다.
2. **준비와 한 Commit 본문.** 기존 `prepare_existing_row`의 `Change::Commit`은 반드시 `history::validate_live` 뒤 실제 Root·Jobs·Status와 Allocation proof를 읽고 독립 Row producer로 frame을 만든다. Catalog Commit의 StageCallStarted→Pending→durable→Drain→whole install→epoch/Q publication 본문을 private 타입 경계로 함께 쓴다. Row를 Catalog observation으로 위장하지 않으며 ordinary installer로 Commit을 대체하지 않는다. 준비 배열은 stage 뒤 소멸하고 Pending에는 immutable payload·물리 after 참조4개와 원 receipt만 남긴다. 원 stage만 Unstaged proof를 발급하고 완전 설치만 Installed proof를 발급한다.
3. **epoch.** 실제 actor의 Row RootRetained가 가리키는 table을 중복 제거하여 준비한다. RootReleasingCompensated는 publication 목록에서 제외하고, Reserved 또는 scope/identity 불일치는 거절한다. 기존 Published table slot만 허용하고 새 slot을 만들지 않는다. conservative write mask는 격리 검증용이며 publication 목록으로 쓰지 않는다. 정확 frame digest/q와 whole-install proof를 결합한 뒤 한 publication lock 안에서 해당 table epoch와 Q를 갱신한다. Row Commit은 catalog namespace epoch를 올리지 않는다. 빈/Cancel-only Row transaction도 같은 Commit 경로를 허용하며 변경 table mask는0이다.
4. **종료·오류.** stage 이전 Unchanged는 원 capsule의 Before, 충돌은 RollbackRequired다. stage 이후 불확실성·부분 설치·publication/반환 실패는 Fatal·fence이고 ACK가 없다. TerminalReady의 scope/actor/status/outcome1/q/LSN, ATT 제거와 공개 Q를 대조한 뒤 capsule slot·snapshot·credit을 checked 반환해야 성공 관측을 만든다. Cancel 뒤 성공 statement 종료와 기존 `abort_empty_transaction`은 정상 동작으로 유지한다. `finish_aborted`와 nonempty rollback은 S4까지 Unsupported다.
5. **자원.** 기존 short65536, capsule512, private425984, A 호출 상한6144/11520을 유지한다. Row prepare/stage/Pending/install/publish/종료와 실제 A caller(호출 지점별·frame-static)를 각각 계수한다. 각 Row phase가 기존 고정 Commit/Progress charge 안인지 이름별 단언하며, 초과를 max 편입이나 cap 상승으로 숨기지 않는다. source→gate 순서·Pending reader 공존·Drain 뒤 install 예약과 latch 없는 durable wait를 유지한다.

**대안.** ordinary Row installer는 Commit 장벽·Q 공개가 없으므로 거절한다. 별도 복사 Commit 본문은 오류·receipt·reader 수명의 이중 소유를 만들므로 거절한다. 공통 public factory나 임의 observation 생성자는 봉인을 깨므로 거절한다. private Commit 준비/설치 타입만 기존 알고리즘의 실제 두 소비에 맞춰 추출한다. 준비와 publisher가 공유하는 의미만 추출하고 Row producer·epoch 선택은 각 owner에 남긴다.

**선택 검증 계획(실행 전 고정).** 새 `s3_` 사례는 아래 분모로 작성한다. 변경이 있는 기존 Catalog Commit의 선택 사례는 shared continuation/terminal/epoch diff와 연결하고, 이미 완료한 Row S1/S2·전체 workspace·wrapper는 반복하지 않는다. compile은 engine lib·test 대상, 기능 실행은 아래 selector만이다. 각 runtime case90초, 실제 thread case180초, native worker/재시작 case180초, compile1200초, 전체 선택 실행1800초, 로그16MiB, Rust 시험 stack32MiB, 첫 실패에서 중단한다. 실패를 보존하고 수정 뒤 해당 실패·영향 diff만 다시 실행한다.

| 묶음 | 새 검증 의무 |
| --- | --- |
| S3-R01 | 실제 INSERT→문장 끝→Commit: Serializable/RC, 독립 기대 Commit frame/Status/Terminal/ATT, q/table epoch/catalog epoch, slot·snapshot·credit 반환 |
| S3-R02 | 빈/Cancel-only Commit과 Cancel 뒤 문장 종료·empty abort; 같은 table의 여러 root publication 중복 제거 |
| S3-R03 | 혼합 scope·origin·active statement·Reserved/Aborting·MustAbort·source capacity의 효과 전 거절과 동일 capsule |
| S3-R04 | 같은 table Serializable 충돌·다른 table 비충돌·RC 허용; conservative mask와 실제 logical 변경 목록 구별 |
| S3-R05 | Commit 이전 replay 불일치 거절: 새 Commit frame0·ACK0·fence; S2W-R24의 원 실패 이력 보존 |
| S3-R06 | stage Unchanged 재시도, 부분 write·sync 오류·부분 install·publication/cleanup 오류의 Fatal·ACK0 |
| S3-R07 | Pending 동안 old Q/read 허용, Drain·완전 install 전 ACK0, Q 공개 전 old snapshot, reader/전용 flusher 진행 |
| S3-C01 | memory/native Row phase census·capsule512·finish 실제 비용·A transaction caller와 frame-static |
| S3-N01 | 실제 파일 I/O의 ACK 뒤 complete/durable Commit prefix, frame별 독립 검산과 별도 재시작에서 현재 Analysis/Unsupported·READY0·mutation0 확인 |

**복구 한계.** S3-N01은 C4/S6 전 Row gate를 유지한다. ACK 뒤 winner를 READY로 복구하는 MCA03-N01/N06–08의 3-process 인수를 PASS로 바꾸지 않는다. native 파일 I/O 결과는 관측한 클라우드 filesystem의 결과이며 전원 장애·공식 ext4 배포 인수를 뜻하지 않는다. 저장한 로그·source hash·실패·review는 새 증거 절에 연결하고 이번에 생성한 build target/export/ELF만 기록 뒤 삭제한다.

**단계별 commit.** S3-a 설계·검증 범위, S3-b 공통 Commit과 Row 준비·epoch·capsule 구현 및 해당 선택 검증, S3-c 실제 consumer/native·census·오류 증거와 독립 최종 리뷰를 각 검토 가능한 commit으로 남긴다. 실제 diff가 다른 유한 단위를 요구하면 owner·시험 대응을 기록하고 나눈다. 통합·기존 검증 반복·원격 push/merge를 자동 실행하지 않는다.

**독립 검토.** `/root/s3_independent_review`가 실제 계약·코드와 이 절의 diff를 읽기 대조하여 S3-a 설계에 한정한 D2 ACCEPT·필수 수정0으로 판정했다. 검토 HEAD `6ec2d3432344f2353e319c7a0eec1955d5a3420a`, 판정 반영 전 파일 SHA-256 `cfc3d017f26411bff3f71a92aae2d0f01de65a333c89b76ccc68c674592c8c9a`. 권고는 실제 두 타입의 봉인과 terminal operation 검산, 두 image의 별도 census, 실행 전 selector/variant manifest 및 실제 Drain 대기 schedule이며 구현·검증 입력에 반영한다. 빌드·시험·자동 문서 검사는 실행하지 않았다.

**S3-b 자원 설계 보완(2026-10-31, 검증 중).** 최초 S3-C01은 Row 창 `[45118,14802,26880,9340]B`가 기존 `[39038,10690,24620,5228]B`를 넘어서 FAIL했다(`artifacts/module-s3-20261031/v1`, memory). 이 실행은 첫 실패에서 중단했으며 R01을 실행한 것으로 표시하지 않는다. 독립 검토에서 일반 Row lifecycle의 `RowLifecycle`/`ExistingGroup`/Observation 반환과9-target ledger 항이 Commit 전용 census에 중복 편입된 점, full Row Observation의 불필요한 `issued` 등 필드를 반환하는 점을 확인했다. 다음 보완은 고정 cap·charge를 바꾸지 않는다.

- 준비는 실제4-target Commit의 selection·replay·producer·inventory·반환 packet만 계수한다. 원 `history::validate_live`는 유지한다.
- Row completion은 원 immutable Jobs after page 참조·terminal ordinal·LSN의 private 요약이다. actor/status는 compact install image에 원 producer 값 그대로 보존하고 원 frame의 Commit/LSN을 해독해 Observation을 복원한다. 완료 authority나 공개 q 생성자가 아니며 ATT·TerminalReady의 전체 expected cell·Status·Q를 checked 종료에서 검산한다.
- `commit_staged`와 `publish_commit`을 non-inline 함수로 분리한다. image·물리 target·install receipt frame은 전자 반환에서 끝나며 후자는 실제 `CatalogCommitInstalled` 완료 증명에서만 투영한 `InstalledFrontier`·`DurableWal`·epochs·요약을 전달받는다. 같은 mutable ledger와 source admission을 계속 보유하고 각 helper의 반환 slot도 계수한다.
- 실제 A `commit_live`는 원 C의 consuming call 뒤 별도 non-inline `map_commit_result`를 호출한다. generic driver도 같은 mapper를 쓰므로 Before/충돌/Fatal 의미는 한 곳이다. mapper는 C 실행 중 호출되지 않는다. 실제 live census는 원 capsule·두 C Result slot·caller 결과·LiveCall·두 Owner와 참조 인수를 센다. tail 전달 결과는 같은 저장 위치의 소유자를 한 번 기록하며 LLVM 최적화를 가정하지 않는다. 기존 generic D15 상계·slack와 원512B long lease는 유지한다.
- Pending의 실제 memory/native WAL wait 호출, 새 install 반환 handoff, 공개만 하는 epoch helper를 별도 계수한다. 기존 광범위 상계와 차이를 출력하고 각 고정 창 및65536B를 단언한다. v8의 memory/native 관측은 Row `[39036,10679,19556,5082]B`, 고정 charge `[39038,10690,24620,5228]B`로 통과했다. 각 source별 실패와 후속 확장 관측은 [실행 증거](../evidence/module-independence-survey-20261026.md#s3-row-commit)에 보존한다.

- Pending은 원 physical backing을 보유한 채 실제 `wait_ready`가 반환한 private borrowed Drain proof만 전달한다. unfinished Drop은 원 core abandon/fence·version 해제 뒤 pin을 반환하고 resume은 동일 pin/stable identity를 한 번만 옮긴다. 원 parent의 고정 배열은 resume·install 동안 별도 계수한다. 물리 leaf와 이후 control/history/completion leaf는 순차 max이며 source 검산·실패 정리 상계는 남긴다. Catalog와 Row를 각각 관측하고 기존 charge 계산은 바꾸지 않는다.
- Serializable 충돌은 capsule을 MustAbort로 바꾸지만 durable 빈 Row actor는 Active다. empty-abort는 scope·ATT·root/undo 부재 검산 뒤 이 조합에서 원 RollbackStart→AbortEnd를 수행한다. durable Aborting 재시도와 Catalog 조건은 유지하며 비어 있지 않은 S4 rollback을 공급하지 않는다.

**S3 최종 증거.** 신규 memory 기능18개·census1개와 변경된 공통 경로6개 selector를 source별로 검증했고 native 두 process를 확인했다. 원 실패9개·NOT_RUN을 보존하며 최종 독립 D1/D2 ACCEPT·필수 수정0이다. [실행·한계·정리 기록](../evidence/module-independence-survey-20261026.md#s3-row-commit)을 따른다.

**재검토 조건.** Row phase가 고정 charge를 넘음, 공개 타입/형식 변경 필요, replay 생략 필요, 실제 producer가 위 빈/다중 root 의미를 지원하지 않음, caller의 보유값 증가, 원 receipt/복구 gate 의미 변경. 필수 결과가 없는 항목은 NOT_RUN이며 단계를 완료로 올리지 않는다.

### 6.7 live rollback과 C/A recovery handler

<a id="row-rollback-s4"></a>
#### S4 구현 입력 — XADB-MODULE-ROW-ROLLBACK-001

2026-10-31. 최신 main `139a05f4`에서 별도 branch `codex/s4-row-rollback`으로 진행한다. D1/D2 담당·통합은 `/root`, 독립 읽기 검토는 `/root/s4_review`다. §6.7의 live 부분을 공급하며 recovery action/READY(C4/S6), Row read(S5), 전체 M4/M5는 포함하지 않는다. wire·cap·charge·공개 API는 바꾸지 않는다.

- 원 transaction/statement binding·scope·phase·slot/mark를 먼저 확인한다. statement rollback은 원 mark까지, full rollback은 None까지 actual retained head를 소비한다. full CLR의 statement/mutation은 소비하는 Job의 원 값이다.
- 원 MaintenancePort와 전용 flusher Owner를 시작 API에 disjoint borrow로 전달한다. 일반 step은 terminal read gate·고정 RowWrite admission을 보유한다. dirty-victim 반환 뒤 task가 없는 `RowRollback::flush`(A의 `flush_rollback_live`)만 원 pin/lease·admission·gate를 checked 반환하고 원 flusher를 구동한다. 이후 반드시 readmission한 뒤 source를 새로 선택한다. 재admission 실패는 Fatal/fence이며 admission 없는 step은 허용하지 않는다. 보유 rollback/caller carrier를 원 flusher census에 더해 65536B를 확인해야 한다. 이 suspension이 초기 "종료까지 보유" 문구를 구체화한다.
- source-issued current pin은 RollbackStart 뒤 실제 Status latch에서, next pin은 durable CLR 뒤 실제 Status latch에서 capture하고 old current를 그 뒤에 반환한다. 두 pin은 원 두-reference lease를 사용한다. 원 latch/actor/head/generation 검산 없이 Jobs 숫자로 발급하지 않는다. 미완료 Drop은 I/O 없이 fence한다. RollbackStart 뒤와 CLR 뒤 오류는 Fatal/fence로 승격한다.
- RollbackStart/StatementFinish/AbortEnd는 원 C producer·canonical replay·whole install을 사용한다. CLR은 A producer와 C observer·draft 재검산·원 stage/whole install을 사용한다. pre-stage Unchanged는 같은 task/prepared wrapper를 반환하며 stage 뒤 불확실성은 Fatal이다.
- StatementFinish receipt 뒤 view를 FailedFinished로 만들어 읽기/쓰기 재사용을 금지하고 원 finish_statement에서 token/snapshot을 소비한다. AbortEnd receipt 뒤 Aborted로 만들고 finish_aborted가 원 capsule을 소비한다. 숫자 mark나 ATT 부재만으로 성공하지 않는다.
- 선택 검증은 다중 statement suffix 보존/후속 Commit, 다중 root full abort·빈 경계, 잘못된 binding/state/후보·조기 finish, stage retry·I/O 실패·Drop, reference/charge 반환, 실제 A caller·CLR memory/native census와 native 재진입 gate다. 실행 전 source별 selector·예산을 고정하고 실패·NOT_RUN을 보존한다. 바뀐 공통 경로만 관련 기존 사례를 다시 선택한다.

비교 대안은 rollback을 A에서 직접 설치하거나 원 statement를 재생성하는 것이다. 이는 원 권한·mark/receipt 수명을 끊으므로 채택하지 않는다. body/caller가 고정 charge를 넘거나 원 latch-issued pin/checked receipt를 만들 수 없으면 설계를 재검토하며 상한을 올려 합격시키지 않는다. 구현·시험·독립 최종 수용은 아직 미완료다.


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

**2026-10-29 수정([C0 Row 경계](#c0-row-boundary)).** 위 signature의 반환을 `CallerCredit<'r,'a,'c,'s>`(`&'r self`)로 좁혔다. `'v`를 그대로 돌려주면 credit이 view·거래보다 오래 남을 수 있고, 그 상태로 거래를 끝내면 slot charge 검산이 Invariant로 fence한다. Catalog Commit에서는 durable Commit·Q 공개 뒤의 거짓 Fatal이 된다. credit이 view를 빌리게 하여 credit이 남은 동안 statement·거래 종료를 컴파일 단계에서 막는다. cursor가 view borrow와 credit을 함께 보유한다는 의미와 charge·Capacity 규칙은 그대로다. `mem::forget`으로 버린 credit은 여전히 거래 종료 때 fence한다.

credit은 storage authority가 아니므로512B보다 작은 반환 row value/output을 새 owner로 취급하지 않는다. point/cursor의 반환 결과는 독립 pin/snapshot/transaction 권한을 가지지 않는 값과 caller buffer다. 그 결과와 오류·operation carrier의 실제 동시 크기는 해당 함수 source census에 포함하지만 result마다 registry/credit layer를 추가하지 않는다. cursor clone은 허용하지 않으며 새 cursor를 생성하려면 새 credit을 취득한다.

isolation header는 aggregate-used, slot12..16은 source-owned-bytes를 보유한다. release는 same open/slotgen과 checked subtraction을 검사하는 no-I/O leaf 동작이다. cursor는 view를, view는 transaction을 borrow하므로 transaction을 소비 종료하기 전 cursor/view가 모두 끝나야 한다. 정상 종료는 capsule의 마지막512B를 반환한 뒤 source-owned0에서만 slot을 Free로 만들고 slotgen을 갱신한다. 별도 Closing 상태나 종료 후 남은 result lease는 없다. active Drop과 poison/underflow는 원 cleanup/fence 정책을 유지하며 존재하지 않는 capacity를 만들어내지 않는다.

<a id="operation-source-admission"></a>
### 6.9 겹치는 writer/reader의 short source admission

[자원 owner](dev3-resource-bounds.md#row-missing-module-budget)의65536B는 **shared retained 한 번 + 동시에 살아 있는 모든 operation의 nested charge 합**이다. `max(writer,reader)`로 병렬 전체를 대신하지 않는다. 각 class의 charge는 구현 source의 fixed `source_census` 표현식으로 산출하여 entry에서 고정하고 실행 도중 늘리지 않는다. 표현식에는 wrapper/내부 helper/Result·오류/return carrier/선택 caller의 동시값을 합하고, 실제 배타 phase에만 max를 사용한다. 이 값은 M4에 실제 타입으로 검산할 계산 결과이며 그때 설계 예산을 새로 정하는 항목이 아니다. `shared_retained + charge > 65536`인 class는 빌드 인수 실패다.

2026-10-29 적용(S2-0, [Row 쓰기 개정](#row-write-budget)): RowWrite class는 `CatalogProgressCharge`를, RowRead class는 Full lookup charge를 재사용하며 port field를 더하지 않는다. 위 "선택 caller의 동시값"은 Row 쓰기에서 A driver의, Row 읽기에서 B driver의 호출 지점별 몫이다. deepest point 하나로 모든 phase를 대신하지 않고 phase마다 그 호출 지점의 몫을 더한다. 측정이 charge를 넘으면 빌드 인수 실패이며 cap이나 charge field가 아니라 Q06 재검토로 간다.

private `OperationSourceAdmission<'b,'a,'c,'s>`는 원 `&'b Mutex<IsolationLedger<'a,'c,'s>>`, fixed class/charge와 active flag만 보유한다. C public entry의 첫 source guard/pin/effect 전에 원 progress admission과 함께 예약한다. RowReadOperation/CatalogReadOperation은 operation부터 checked finish까지, PreflightRow→ReservedRow→PreparedRow는 같은 한 예약을 이동하여 성공/취소까지, RowRollback/RecoveryRowTask는 선택된 완료 단위까지 보유한다. Before가 wrapper를 반환하면 같은 admission도 남으며 retry 때 중복 청구하지 않는다. 메서드 내부 helper는 상위의 nested charge 안에서 실행하고 새 예약을 요구하지 않는다. typed wrapper를 caller가 오래 보유하면 source 용량도 계속 점유하므로 합계를 벗어나는 재진입은 첫 효과 전에 Busy/Capacity다. primitive Result 반환 함수는 고정 entry/exit carrier 비용까지 예약한 뒤 출구에서 반환한다. 독립 capability가 없는 Result마다 generic credit을 만들지 않는다.

#### XADB-MODULE-ADMISSION-002 — 원 admission 뒤 깊은 검산

2026-10-27 · D2 · 내부 Catalog Full/Resident 진입 우선순위의 한정 재개방이다. 작성/통합 `/root`, 독립 설계 검토 `/root/module_review`. 이전 부분공급의 pool/snapshot 전체 검산→progress/source 순서는 source가 부족한 호출도 큰 metadata 검산 closure를 보유하게 했다. 같은 검산을 부족 분기에서 다시 수행하는 대안은 그 비용을 없애지 못하므로 채택하지 않는다. **pool/visibility guard 없는 원 health/origin → 원 core의 Owner/progress 검사 → fixed source 예약 → 원 pool/snapshot 전체 검산 → refs/pins**로 구체화한다. 아래 과거 source 증거와 당시 오류 우선순위는 소급 변경하지 않는다.

storage poison/fence, pool의 fence와 이미 관측된 registry poison, 원 storage binding/core 및 Owner의 기존 mode/generation 검사를 먼저 수행한다. foreign Owner와 잘못된 mode는 기존 Census/Frame projection을 유지하며 Capacity로 바꾸지 않는다. snapshot은 같은 원 registry와 guard 없는 health/poison만 먼저 확인한다. `is_poisoned()==false`는 이후 건강 보장이 아니며 뒤 경쟁의 poison/손상은 예약 성공 뒤 원 guard 검산이 다시 검출한다. 같은 원 core의 read admission·Drain·progress 표시는 source 예약 시도부터 원 source 반환까지 유지한다.

Capacity뿐 아니라 Busy·InvalidRole·InstallationPending·cheap origin/Owner 거절도 아직 확인하지 않은 pool/snapshot 메타데이터 오류보다 먼저다. 이미 관측한 Fatal을 낮추지 않는다. 부족/거절 경로는 pool/visibility registry guard·새 refs/pins/I/O 없이 반환한다. successful source 예약은 기존 pool health/shape·Owner·snapshot 전체 검산을 생략 없이 수행하며 그 closure를 Full/Resident의 고정 source max에 포함한다. 검산 뒤 원 작은 오류를 유지한 채 source→progress checked cleanup을 수행하고, 원 Fatal은 cleanup 오류보다 우선한다. 건강한 거절은 source를 반환하지만 예약 후 실제 Fatal은 원 fence 때문에 charge를 보존할 수 있으며 progress 정리는 별도로 시도한다.

이는 내부 미진입 오류 순서의 D2 변경이다. 공개 SQL·영속 형식·cap·이미 관측한 Fatal·필수 검산·원 Drain의 의미는 변경하지 않는다. 새 semaphore/manager/Result credit 또는 가짜 progress를 만들지 않는다. 실제 구현/선택 실행과 독립 최종 수용은 [admitted validation 증거](../evidence/module-independence-survey-20261026.md#c2-admitted-validation)에 둔다. 전체 uncharged entry/tail floor와 writer phase 예약은 후속 의무이며 이 변경만으로 전체65536B 동시 합을 입증하지 않는다. cheap 실패 projection이 달라지거나 post-admission validator가 빠지거나 실제 source 식이 class 안에 들어가지 않으면 재검토한다.

원 isolation header의24..32는 short-used u64로 고정하고32..64는 zero를 유지한다. shared-retained와 class별 nested charge는 source_census의 고정 상수이며 progress 몫은 원 admission phase에 연결한다. 나머지 header와 long source-owned 회계는 [격리 owner](dev3-task-10-concurrency.md#module-row-isolation)를 따른다. 바깥에 새 semaphore/pool/manager를 두지 않으며 원 mutex poison/checked subtraction/fence 정책을 사용한다. 공개 함수의 부족 분기는 source guard/pin/lock0인 상태에서 반환한다. 읽기 guard나 reference를 붙잡고 source capacity를 기다리지 않는다.

rollback/terminal은 원 progress admission이 새 foreground를 닫고 이미 실행 중인 유한 read operation이 checked 반환한 뒤 reserved source 몫으로 들어간다. 그 몫은 `65536-shared_retained` 안의 해당 progress class nested charge이며 foreground와 겹치지 않는다는 actual admission 증거가 있을 때만 max로 계산한다. 새 선택적 foreground 예약 성공을 rollback/terminal의 선행으로 삼지 않는다. 장기 snapshot/cursor를 강제 종료하여 공간을 만들지 않고, flush의 별도65536B와 원 content Drain 순서를 유지한다. Reserved/Before wrapper를 보유한 writer는 같은 원 exclusive control을 점유하므로 다른 writer progress와 공존한다고 계산하지 않는다. caller의 무한 미반환은 진행 보장이 아니라 보유 소유이며 Drop/fence 규약을 따른다.

실제 부분공급으로 [Catalog 조회 수명/Commit Drain](../evidence/module-independence-survey-20261026.md#c2-read-operation-lifetime)을 연결했다. 원 OwnerSlot의 별도 read 표시를 첫 reference 예약 전부터 checked 정리 시도 뒤까지 유지하고 Drain이 표시0을 기다린다. Pending LOAD는 허용하고 Draining의 신규 operation/content 거절은 유지한다. 정상 refs0과 fence 뒤 원 fatal refs 보존을 구분한다. 당시 byte admission은 미공급이었으며 후속 full 조회 예약은 아래에 기록한다. 전체 동시 합·writer phase/progress 몫은 남아 있어 이 표시만으로 본 절의 인수를 완료하지 않는다.

[Commit phase 비용 구체화](../evidence/module-independence-survey-20261026.md#c2-commit-phases-main)는 기존 stage helper 반환 전후의 source 값을 구분한다. suspend의 Stage+Pending 반환, resume의 Pending+새 WriteTargets/Install 반환은 합산하고, 준비·대기·설치의 배타 호출만 max로 계산한다. 당시 session retained가 원 PageConcurrency의 참조만 보유하므로 실체를 Catalog 비용에 한 번 포함했다. 실제 shared-retained 한 곳으로 옮겨 다른 class와 중복 없이 계수할 후속 의무는 아래 공통 core 계수에 반영했다. 물리64-target의 기존 보수 Capacity gate는 유지하며, 이 부분공급은 고정 charge의 수치 근거이고 동적 예약 크기 변경이나 동시 합 인수가 아니다.

[공통 core source 계수](../evidence/module-independence-survey-20261026.md#c2-shared-core)는 원 `PageConcurrency` 실체를 fresh binding·DiskSession·recovered binding/반환의 shared retained에 한 번 포함하고 Catalog nested phase에서 제외했다. Catalog의 기존 단일 Capacity gate는 `core+operation` 합을 유지한다. 복구 replay cursor는 private restore helper가 반환할 때 끝나며, binding·원 owner와 완료 반환값은 계속 계수한다. Linux 타입의 원65563B 초과 FAIL과 수정 후65512B 상계를 보존한다. 이는 main 수신 source의 단일 실행 회계이며 전체 동시 합/admission 공급은 아니다.

[main의 full Catalog short 예약](../evidence/module-independence-survey-20261026.md#c2-short-source-main)은 원 isolation header24..32를 사용해 고정 lookup charge를 예약하고 checked 반환한다. 원 progress 표시는 예약 시도 전에 시작하며 refs 정리·최종 decode·source 반환 뒤에 끝난다. 같은 class의 추가 조회가 합계를 초과하면 refs/pin/저장 효과 전에 Capacity이고 원 snapshot 재시도는 가능하다. 공통 core816B는 shared retained에 한 번 포함한다. 이 full 조회 부분공급 당시 미예약 writer/progress·resident class·동시 entry/return 전체 합 인수는 남았다. resident 후속 공급은 아래에 구분한다.

[main의 상주 Catalog class](../evidence/module-independence-survey-20261026.md#c2-resident-main)는 동기 `lookup_catalog_resident` 진입에서 고정 charge를 선택한다. 원 allocator를 기존 caller loan에 복사하고 전체 Status에서 exact reference의 유일성을 확인하며, 원 Txn/Undo 참조와 실제 allocator/target guard가 수명을 보호한다. 필요한 페이지의 비상주나 정상 allocator stamp 변화는 checked 정리 뒤 Busy이고 같은 snapshot으로 Full을 새로 호출할 수 있다. 한 operation 안에서 class를 늘리거나 WAL replay·durability wait·reader file·LOAD를 수행하지 않는다. 안정된 source의 exact-ref 부재/중복·선택 뒤 membership 손상은 Fatal/fence를 유지한다. main의 source 예약 시도 전부터 최종 반환 뒤까지 Drain 표시를 유지한다. Resident 두 개의 source 합은 공통 core 포함 memory56792B/native 타입56928B이며 전체 writer/progress·entry/return 동시 합 인수와 구별한다.

[main의 Drain 완료/guard 인계](../evidence/module-independence-survey-20261026.md#c2-drained-handoff-main)는 authentic durable Drain과 content guard 재구성을 별도 consuming 호출로 나눈다. private·nonClone `DrainedCommitTargets`가 원 Installing/version gate·PinSet·StablePage를 인계하며 정상 미완료 Drop도 installation abandon/fence를 먼저 완료한 뒤 원 pins를 반환한다. physical drain은 Pending+Drained+Result 반환을 합산하고 resume은 Drained+WriteTargets+Install 반환을 합산한다. main의 semantic4phase는 drain의 mapped Result까지 포함한 설치 상한을 사용하며 원 core는 shared에 한 번 센다. 이 경계는 후속 reserved source의 무대기 checked 전환 위치이며 새 capacity 대기를 허용하지 않는다. source 예약·전체 동시 합 인수는 후속이다.

#### 선택 Catalog caller의 결과 소비 수명

[main의 CatalogCallScope](../evidence/module-independence-survey-20261026.md#c2-call-scope-main)는 원 Owner/backend/snapshot borrow와 기존 source/progress를 private-field·nonClone scope에 보유한다. `ReadPort::catalog_call`과 `catalog_resident_call`이 고정 class를 한 번 예약하고 `lookup<'call>(&'call mut self, ..., private: &'call mut [u8;425984]) -> Result<CatalogLookupStep<'call>>`가 반환 view·복사한 view·definition/name/column의 수명을 같은 scope borrow에 묶는다. 마지막 결과 소비 뒤 `finish_checked(self)`로 닫고 정상 오류는 `finish_error(error)`로 정리한다. 순차 lookup은 같은 예약을 재사용하며 class를 실행 중 확대하지 않는다.

Operation은 scope를 borrow하고 원 refs/lease/loader/copy만 소유한다. refs 정리→최종 decode/Result→선택 caller 소비→source 반환→progress 반환 순서를 유지한다. 원 Fatal은 cleanup 오류보다 우선하며 성공/Unchanged보다 cleanup Fatal을 우선한다. source 반환 실패에도 progress를 반환하고 Drop도 같은 순서를 유지한다. 기존 `lookup_catalog`/`lookup_catalog_resident`는 같은 제품 본문과 명시 `finish_result`를 사용하지만 **반환 이후 caller 소비는 scope 보장 밖**이다. 이 호환 entry를 전체 admission 완료로 승격하지 않는다.

선택 query는 유한 header/Table/Column 검사와 Missing/FlushRequired 분기를 마친 뒤 scope를 닫고 실제 flush는 그 후 수행한다. consumer phase는 operation과 순차이므로 고정 class의 max에 포함한다. main의 원 core는 shared에 한 번 포함하고 생성/반환의 Result를 함께 계수한다. source 선예약 전 진입·해제 뒤 반환 tail·foreign Owner 거절의 전체 동시 상계와 writer/progress 예약은 후속이다. 임의 caller의 무제한 복사나 장기 보유를 이 유한 소비 검산으로 보장하지 않으며 새 registry·generic Result credit·cap 확대를 허용하지 않는다.

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

[main의 Commit 준비 수명 분리](../evidence/module-independence-survey-20261026.md#c2-pending-image-main)는 stage에서 PreparedCatalog를 소비해 불변 payload와 canonical after 참조4만 Pending에 넘긴다. 원 pin/WAL/install 권한은 그대로이며 준비용9개 target/stamp 배열은 stage 반환 전에 끝난다. stage에는 입력·image·반환 tuple·추출 배열을 함께 청구하고 Pending/install에서만 compact image를 사용한다. 전체 Commit peak32292B와 Pending18532B는 서로 다른 phase다. 초기 memory 검증·독립 한정 수용 뒤 [후속 native N03](../evidence/module-independence-survey-20261026.md#c2-pending-image-native)를 같은 source/ELF로1회 PASS했다. 후속 OCR7파일 독립 D1/D2 수용으로 이 C2 단위의 선정 검증 인수를 마쳤으며 writer/progress 동시 합 인수는 남았다.

[main의 Aging/Root source 계산](../evidence/module-independence-survey-20261026.md#c2-aging-source-main)은 전체 queue 대신 actual Root2 decode와 순차 Aging helper를 계수한다. initial Root 중첩 버퍼·malformed Transaction XOD의 XTS decode·by-value budget 반복·반환 carrier를 포함하고 XOD/XTS 공통 계산은 각 lower owner가 제공한다. Full 고정 charge는37724B, Resident16860B이며 원 core 포함 shared retained·Commit4phase·전체 peak는 유지한다. 실제 Full 예약·반환의 선택 검증이며 writer/progress·전체 동시 admission 공급을 뜻하지 않는다.

[main의 실제 WAL wait source 관측](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source-main)은 physical 호출·semantic 반환·Pending parent를 test-only로 구분한다. main 원 core를 포함한 shared+Full+Pending 합은 memory66116B/Linux 타입66252B로65536B 기준 FAIL이다. 관측 시험 PASS와 구별하며 기존 production 상계·charge·admission은 유지한다. Drain 인계·entry/return ticket·writer/progress 전체 합은 후속 공급이다.

<a id="writer-progress"></a>
#### XADB-MODULE-WRITER-PROGRESS-001 — Catalog Commit의 terminal read gate와 writer source 창

2026-10-28 · D2 · branch `claude/module-independence-remaining`(기준 main `854bd8d`) · 작성·계약·통합 Claude 세션 · 독립 검토와 실행 결과는 [증거](../evidence/module-independence-survey-20261026.md#c2-writer-progress). 이전 Commit은 원 isolation ledger에 예약하지 않고 단일 operation의 정적 `core+source_bytes()≤65536`만 검사했다. stage 창은 Full read와 함께 65536B를 넘으므로 동시 실행의 합계를 보장하지 못했고, 위 terminal 규칙(새 foreground를 닫고 admitted read가 반환한 뒤 전용 몫 사용)은 미공급이었다.

**선택한 수명.** ① 원 health·operation·visibility binding 검사 뒤, 첫 source guard·pin·효과 전에 `Owner::close_reads_for_terminal`이 원 core State에 terminal slot index를 기록한다. 새 read operation의 check/try는 Unchanged `InstallationPending`, 같은 Owner의 read 진행이나 이미 닫힌 gate는 `Busy`, maintenance/flusher·foreign core는 `InvalidRole`, 설치 진행 중은 `InstallationPending`이다. 이어 원 continuation에서 모든 slot의 read 표시가 0이 될 때까지 기다린다. 대기 중 page guard·pin·version gate·ledger 잠금을 보유하지 않고, read는 terminal을 기다리지 않으므로 Drain과 같은 유한 의존이다. fence는 대기 오류로 반환하며 gate는 Drop/정리에서 해제한다. ② read는 source를 progress 표시보다 먼저 반환하므로 표시0은 admitted read의 short charge0을 뜻한다. `OperationSourceAdmission::catalog_commit`이 gate의 core가 ledger의 원 core인지 확인하고 stage 창을 header24..32에 대기 없이 예약한다. 이 시점의 부족은 구조적 상계 위반이나 누수된 loan만 만들 수 있으며 Unchanged Capacity다. 창 순서(Pending≤stage, publish≤install)와 shared+stage·shared+install≤65536도 효과 전에 확인하여 위반을 Unchanged Capacity로 거절한다. 예약 실패도 gate를 checked 정리하여 cleanup 오류를 원 Unchanged보다 우선한다. ③ WAL stage가 Pending을 반환하면 Pending 창으로 축소한 뒤 gate를 연다. 이후 read는 같은 ledger에서 writer 몫과 합산 admission된다. ④ 원 Drain(begin_drain→wait_ready)이 새 read를 거절하고 admitted read 반환을 기다린 뒤 install 창으로 대기 없이 증가한다. 초과는 회계 불변식 위반으로 fenced Invariant다. ⑤ `install.finish`가 read를 다시 열면 publish 창으로 축소한다(shrink only). finish와 축소 사이에 들어온 read는 install 몫을 보고 Unchanged Capacity를 받을 수 있으나 기다리지 않는다. ⑥ 결과 뒤 source→gate 순서로 checked 정리한다. 원 Fatal이 우선하고, 그 밖에는 cleanup 오류가 성공/Unchanged를 대체한다. fence 아래 charge는 보존하며 용량을 제조하지 않는다. unwind의 Drop도 같은 순서다.

**고정 charge.** split에서 한 번 계산하며 caller가 byte를 고르지 않는다. stage=기존 phase의 max(preflight, stage), Pending=stage 반환 뒤 parent+max(WAL wait 단독 bridge·handoff, Drain 호출), install=기존 install phase, publish=완료 receipt·epoch와 publication packet이다. 모든 창에 retained admission/gate carrier와 한 번의 전환·정리 closure를 더한다. Drain 호출은 parent에서 이동한 Pending을 한 번만 센다. 성공은 core begin/wait와 Drained 반환, 실패는 반환 오류가 남은 채 Pending Drop이 core abandon 뒤 원 pin을 반환하는 순차 경로이므로 두 경로의 max를 사용한다. 이를 위해 lower owner가 다음 production 단독 bound를 공급한다: `terminal::catalog_wait_source_bytes`(기존 cfg(test) `catalog_phase_sources()[1]`과 같은 식이며 측정 wait 관측 이상임을 memory/native 양쪽에서 검산), `dirty_frame::commit_drain_call_source_bytes`, `pin_release_source_bytes`(registry guard 아래 원 `pins_valid`·registration 검산은 retention 진입의 guarded bound로 계수), `epochs::publication_source_bytes`(기존 transition의 guard/receipt packet 분리). gate 비용은 core State 검산 closure를 포함한다. 기존 포괄 bound는 유지하고 session census의 Commit 항목은 가장 큰 writer 창과 carrier로 바꾼다. 실측 값은 증거에 둔다.

**관측 동작의 변경.** (a) Commit의 preflight·stage 동안 새 Catalog read는 Unchanged `InstallationPending`이다(이전에는 Intent 중 허용). (b) Commit은 시작 시 이미 admitted read의 반환을 기다린다(이전에는 stage·Pending을 지나 Drain에서 기다림). (c) Pending·publication gap의 read는 writer 몫을 포함한 합으로 admission되어 한 Full read는 들어가고 초과 조합은 Capacity다. (d) 정적 Commit Capacity 검사를 실제 예약으로 대체했고 그 Capacity의 phase는 Prepare에서 Census가 된다. (e) 같은 Owner의 read 진행이나 Open이 아닌 설치는 target 예약의 Frame 오류 대신 gate의 Concurrency `Busy`/`InstallationPending`(Prepare)로 보고된다. gate 거절은 WAL prefix 관측이나 `prepared.check`의 binding 검사보다 먼저다. 무관한 core의 Owner는 이전에도 InvalidRole이었고, 준비 payload의 core에 속하지만 pool의 bound core와 다른 Owner만 BindingMismatch 대신 InvalidRole이 된다. (f) gate가 기다리는 동안 새 Catalog read는 가장 긴 admitted read(caller가 보유한 CallScope 포함)가 반환될 때까지 모두 거절된다. 다른 Owner의 scope를 보유한 같은 thread가 Commit하면 멈추며, 이는 이전에 Drain에서 생기던 의존이 더 이른 시점으로 옮겨진 것이다. gate 보유 중 그 Owner의 Drop은 fatal Invariant로 slot을 보존한다. 공개 SQL·영속 형식·WAL/Commit 순서·원자 공개·pin/fence 의미는 바뀌지 않는다. Commit 전에 admitted된 read가 Drain까지 진행하던 기존 시험 시나리오는 같은 Drain 성질을 Pending에서 admitted된 read로 검산하도록 옮겼고, 원 시나리오의 과거 source별 PASS는 당시 증거로 보존한다.

**대안.** gate 없이 foreground처럼 예약하고 부족 시 Capacity를 반환하는 안은 terminal이 foreground 점유로 거절되어 위 규칙과 맞지 않는다. 용량이 생길 때까지 ledger 조건을 기다리는 안은 core·ledger 잠금 중첩 금지와 충돌하고 알림 누락·새 read에 의한 기아가 생긴다. 전체 peak 하나를 예약하는 안은 Pending 중 Full read를 막는다. core State에 identity 전체를 두는 안은 core 크기를 바꿔 모든 census를 이동시키므로 slot index(`Option<u8>`)와 gate 값의 generation 검산을 택했다.

**범위 밖·남은 의무.** CREATE 등 다른 MutationPort writer·Maintenance·Row reader/writer는 아직 ledger에 참여하지 않는다. gate 대기를 포함한 진입 전·반환 뒤 floor, Row RowReserve/rollback/RecoveryRowTask progress class, C3–C5와 M4/M5 인수는 미완료다. read가 terminal을 기다리는 경로, Drain 뒤에도 short charge를 가진 새 ledger 사용자, source 수명·타입이나 `install.finish` 순서가 바뀌면 이 결정을 재검토한다.

<a id="retirement-progress"></a>
#### XADB-MODULE-RETIREMENT-PROGRESS-001 — Catalog 회수의 고정 progress 예약

2026-10-28 · D2 · 기준 main `c217fbf` · 작성/계약/통합 `/root` · 설계 독립 검토 `/root/retirement_design_review`는 Maintenance 역할 보존을 조건으로 수용했다. 구현·시험·최종 검토는 [회수 progress 증거](../evidence/module-independence-survey-20261026.md#c2-retirement-progress)에 구분한다.

`MutationPort::apply_catalog_retirement`의 CLR/Prune/RootFree/TerminalFinish는 첫 source guard·pin·효과 전에 원 `close_reads_for_retirement`으로 새 read operation을 닫고 이미 진입한 유한 read의 반환을 기다린다. 원 foreground와 Maintenance를 허용하고 dedicated flusher·foreign core를 거절한다. Commit의 foreground 전용 entry는 유지한다. 대기는 원 continuation에서 page/version/ledger 잠금 없이 수행한다. 장기 snapshot은 취소하지 않으며 기존 horizon 검산의 Busy를 유지한다. 같은 thread가 다른 Owner의 read scope를 보유한 채 회수하면 미반환 의존이 생기므로 먼저 반환해야 한다.

고정 `CatalogRetirementCharge`는 exclusive split에서 계산하고 원 isolation ledger24..32에 예약한다. caller는 byte 수를 선택하지 못한다. 준비 receipt와 모든 parent carrier를 포함하고, CLR capture read·retirement write·durable 뒤 allocator read는 실제 checked 반환으로 수명이 분리되므로 물리 closure만 max로 계수한다. 예약은 capacity를 기다리지 않는다. 예약 후에는 refs/page/DPT/control/history·durability·pair retirement·epoch publication·결과 구성을 마칠 때까지 같은 charge와 gate를 유지한다. source를 먼저 checked 반환한 뒤 gate를 열며 원 Fatal 우선·fence 뒤 charge 보존·gate 정리 시도는 Commit과 같다.

이전 CLR은 실행 중인 reader pin 때문에 즉시 Busy일 수 있었다. 이제 유한 reader가 source/ref/pin을 반환한 뒤 진행하며 gate 중 새 조회는 `InstallationPending`이다. stage 전 reservation 부족은 `Census/Capacity/Unchanged`이고 효과·새 refs/pins 없이 gate를 다시 연다. operation/health 다음 gate 검산이 기존 깊은 prepared/visibility/flusher 검산보다 먼저이므로 새 entry 오류가 먼저 관측될 수 있다. 이미 관측한 Fatal은 낮추지 않는다. WAL·영속 형식·원자성·horizon은 변경하지 않는다.

gate 없는 선택적 foreground 예약은 reader 점유 때문에 terminal을 거절하고, ledger capacity 대기는 lock/progress 계약과 맞지 않아 채택하지 않았다. `prepare_catalog_retirement` 등 준비 경로·다른 writer·독립 Maintenance 작업·Row·entry/return floor와 M4/M5 전체 인수는 여전히 미완료다. read가 이 writer를 기다리도록 바뀌거나 물리 closure가 중첩되거나 새 원 ledger holder가 추가되면 재검토한다.

<a id="retirement-preparation"></a>
#### XADB-MODULE-RETIREMENT-PREPARATION-001 — 공유 회수 준비 body의 progress 예약

2026-10-28 · D2 ACCEPTED (준비 body 한정) · 기준 main `fa0f033` · 작성/계약/통합 `/root` · 독립 `/root/source_admission_review`의 사전 설계 조건을 반영했다. 구현·최종 검토·실행 상태는 [해당 증거](../evidence/module-independence-survey-20261026.md#c2-retirement-preparation)를 따른다. 후속 [동기 회수 호출](#retirement-call)이 이 절의 외부 준비 결과·prepare/apply 분리를 대체하며 아래 설명과 원 결과는 당시 범위로 보존한다.

`MutationPort::prepare_catalog_retirement`의 CLR/Prune/RootFree/TerminalFinish는 지원 operation·guardless storage/pool health·origin을 확인하고 원 `close_reads_for_retirement`으로 기존 유한 read를 반환시킨 뒤 source를 예약한다. 그 안에서 기존 selection·registry/control 검산·canonical replay·page/producer·Prepared 구성을 그대로 수행한다. foreground와 Maintenance는 허용하고 flusher·foreign core는 거절한다. 깊은 prepared 검산 전에 gate/Capacity 오류가 먼저 관측될 수 있다. 기존 exclusive `DiskSession` 준비는 해당 공유 진입점의 대상이 아니다.

원 `CatalogRetirementCharge`의 단일 scalar를 split에서 `max(application, preparation)`으로 계산한다. 준비는 caller+공통 body와 selection 중 큰 값에 gate/source·wrapper와 `Result<PreparedCatalog>`의 전달·checked cleanup 비용을 더한다. 같은 `&mut MutationPort`의 준비/적용은 직렬이므로 합하지 않는다. 추가 port field·별도 ledger·caller byte 입력은 없다. 준비 완료 뒤 결과를 보유한 채 source→gate를 checked 반환하며 원 Fatal 우선과 unwind의 charge 보존·gate 정리를 유지한다. gate 중 새 조회는 `InstallationPending`, 예약 부족은 `Census/Capacity/Unchanged`이며 WAL/새 refs/pins/private 쓰기 전에 거절한다. 장기 snapshot과 horizon 검산은 유지한다.

대안인 foreground 방식의 즉시 Capacity는 회수 진행을 기존 reader 점유에 맡기며, ledger capacity 대기는 원 progress/lock 계약과 맞지 않는다. 전체 Prepared에 admission을 넣는 안은 별도 borrow/보유 수명 계약을 요구하므로 이 body 공급과 구별한다. **gate 대기 전 entry, source 반환 이후 tail, caller가 보유하는 하나 이상의 Prepared, prepare→apply 사이 gap의 floor는 계속 미공급이다.** 이 부분공급을 전체 short aggregate 또는 M4/M5 완료로 승격하지 않는다. 원 source holder·Prepared 수명·준비 closure가 바뀌면 재검토한다.

<a id="retirement-call"></a>
#### XADB-MODULE-RETIREMENT-CALL-001 — 회수 준비·적용의 단일 동기 예약

2026-10-28 · D2 · 기준 main `582ccbf` · 작성/계약/통합 `/root` · 독립 설계 `/root/source_admission_review` 수용. [앞선 준비 body 공급](#retirement-preparation)의 caller-retained 회수 Prepared와 prepare→apply gap을 이 결정으로 대체한다. 원 source별 결과는 보존한다. 구현/실행/최종 검토는 [증거](../evidence/module-independence-survey-20261026.md#c2-retirement-call)에 구분한다.

`MutationPort::retire_catalog(owner, ordinal, operation, private, flusher)`는 원 gate/source를 한 번 취득하고 기존 준비→내부 Prepared→적용→durability/epoch→결과 구성→source/gate checked 반환까지 유지한다. 공유 prepare와 exclusive prepare를 제거하고 apply dispatch를 private으로 제한한다. `DiskSession::catalog_retire`도 split 후 같은 동기 writer를 호출하므로 실제 복구 caller가 이를 수신한다. retirement의 중간 Prepared는 외부로 나오지 않는다. Commit의 Prepared 계약은 그대로다. 준비/적용 중 새 조회는 InstallationPending이며 장기 snapshot의 원 horizon Busy, Maintenance 허용, flusher/foreign 거절, 원 Fatal 우선과 fence 아래 charge 보존은 유지한다.

고정 charge는 준비/application 중 큰 값이며 통합 entry 인자·결과 carrier를 포함한다. 준비용 Result 전달 비용도 보수적으로 유지한다. 두 body는 직렬이고 gate/charge를 중간 반환하거나 재취득하지 않는다. 추가 port field·별도 ledger·가변 byte 입력은 없다. `prepared.check`, exact target/stamp, flusher binding/phase, rollback refs·stage cancel·pin 정리와 WAL 순서는 원 구현을 사용한다. 과거의 준비 후 discard·장기 보유는 회수 API에서 제거하는 내부 계약 변경이며 실제 제품 caller에는 그런 사용이 없었다. 독점 recovery도 이제 guardless health 뒤 gate/source 오류를 먼저 받을 수 있다.

준비 receipt에 guard를 넣어 caller에 반환하는 대안은 임의 보유/중도 Drop과 추가 borrow 수명 계약을 늘린다. private callback 대안은 필요 없는 재진입 가능성을 만든다. 실제 caller가 즉시 적용하므로 유한한 동기 호출을 선택했다. 회수의 caller-retained gap만 닫으며 일반 entry-before-reservation/return-tail floor, Commit Prepared lifetime, 다른 writer/Row, C3–C5 및 M4/M5 전체 인수는 미완료다. 새 retirement caller가 준비 후 보유를 요구하거나 body phase·source holder가 바뀌면 재검토한다.

선택 검증은 [동결 manifest](../../tests/fixtures/dev3-module-c2/retirement-call/manifest.json)의 compiler4(정상 동기 호출, 제거된 공유/독점 prepare, private apply), runtime13(source3·동기 schedule/오류/역할·CLR/Prune/root/terminal·unwind/Fatal), native N02 한 번이다. N02는 같은 ELF의 producer→durable CLR 중단→재진입3PID로 실제 recovery 변경을 검산한다. 필수 native 입력·품질·원 controller30초 한도를 유지한다. 변경 없는 Commit/Row·다른 native cut·전체 wrapper는 반복하지 않는다.

<a id="lifecycle-progress"></a>
#### XADB-MODULE-LIFECYCLE-PROGRESS-001 — Catalog rollback·terminal 동기 공급

2026-10-28 · D2 · 기준 main `bf62459` · 작성/계약/통합 `/root` · 독립 `/root/source_admission_review`. 사용자 전체 진행 지시와 DP01 위임 아래 네 lifecycle 경로를 공급한다. 사전 검토의 원 flusher 배제·기존 phase/Cancel predicate 유지 조건을 반영했고 구현/최종 검토/실행은 [증거](../evidence/module-independence-survey-20261026.md#c2-lifecycle-progress)에 구분한다.

`MutationPort`의 `catalog_rollback_start`·`catalog_abort_end`·`catalog_cancel_reserved`·`catalog_release_role`은 owner와 actor/ordinal, private scratch, 같은 원 `MaintenancePort`의 mutable borrow를 받는다. 원 terminal read gate를 닫고 `CatalogProgressCharge=max(retirement,lifecycle)`를 원 isolation ledger에 한 번 예약한 다음 private preparation→Prepared→ordinary apply→durability/epoch→checked source/gate 반환까지 유지한다. 기존 exclusive caller는 split 후 같은 호출에 위임한다. Maintenance 허용·flusher/foreign owner 거절, capacity-before-selection/private/WAL, 원 Fatal 우선·fence 아래 charge 보존을 유지한다.

read gate만으로 별도 flush 호출이 닫히지 않으므로 같은 flusher를 끝까지 빌리고 pool/storage binding을 확인한다. 기존 exclusive API에 없던 Ready phase 제한은 추가하지 않는다. CancelReserved의 `handoff_quiescent`는 원 pins/init/claim/load 조건을 그대로 유지하며 live `ReservedCatalogCreate::cancel`의 pair 반환 뒤 epoch 적용과 구별한다. 성공하는 recovery Cancel은 기존 MC04 corpus로 검산한다. retained flush claim 사례는 Busy 후 selection 재진입까지 검산하며 성공 Cancel 증거로 승격하지 않는다.

ordinary application을 private `CatalogApplication`(원 storage/pool/control/history/visibility의5개 borrow)로 옮긴다. exclusive/shared caller가 이 view를 직접 만들므로 Prepared를 보유한 채 source 계산을 다시 하는 split은 없다. 별도 owner/ledger·가변 charge·외부 Prepared/preparer/application 공개가 없다. view/forwarder·action/인자/결과 carrier를 기존 body 계산에 추가하고 Cancel registry 관측은 직렬 max로 계산한다. gate/source 밖 일반 진입·반환 floor, Begin/AgingStart/CREATE admission, Commit Prepared lifetime·Row·C3–C5·M4/M5 전체는 미완료다.

기존 exclusive 구현을 네 곳 복제하는 대안은 ordinary apply 의미를 중복시킨다. Prepared 아래 split하는 대안은 불필요한 큰 계산 phase를 겹친다. 직접 borrowed view를 선택하며 새 동시 pool 사용 경로나 일반 body/holder 변경 시 재검토한다. [manifest](../../tests/fixtures/dev3-module-c2/lifecycle-progress/manifest.json)에 compiler4·runtime12·같은 ELF native N02(3PID)1회를 diff별 연결했다. 총1800초/compiler420초/runtime45초/native 원30초/사전 독립 admission600초/로그8MiB/stack32MiB를 실행 전에 고정한다. N02는 actual recovery의 변경 경로를 검산하며 native CancelReserved 증거는 아니다. 완료 무변경 시험·wrapper는 반복하지 않는다.

<a id="start-progress"></a>
#### XADB-MODULE-START-PROGRESS-001 — Begin·AgingStart의 동기 source 공급

2026-10-28 · D2 · 기준 main `af78960` · 작성/계약/통합 `/root`, 독립 `/root/source_admission_review`. 사용자 전체 진행 지시와 DP01 위임 아래 [lifecycle progress](#lifecycle-progress)에 두 시작 경로를 연결한다. 구현/실행/최종 검토는 [증거](../evidence/module-independence-survey-20261026.md#c2-start-progress)를 따른다.

공유 `catalog_begin(owner,status,private,flusher)`·`catalog_aging_start(owner,private,flusher)`와 기존 exclusive wrapper는 같은 원 gate/source 안에서 선택→private Prepared→ordinary apply→durability/epoch→checked 정리를 수행한다. Begin의 deep health/control pending/population→free slot→first Job→Status kind/free generation→prefix→checked issued_local 증가→준비 순서를 유지한다. Capacity는 원 admission 앞에서 아직 관측하지 않은 deep 오류보다 먼저일 수 있으며 ADMISSION-002의 내부 입구 순서다. 발급 전 오류는 issued_local을 유지하고 발급 뒤 prepare 오류는 원 gap을 남긴다. 같은 mutable flusher와 binding·Maintenance 허용·Ready 무제한을 유지한다.

Begin/Aging의 gate는 선택적 writer의 유한 직렬화다. rollback/terminal 필수 progress 보장을 새로 주장하지 않는다. 호출 thread가 다른 Owner의 유한 read scope를 보유했다면 먼저 checked 반환해야 하며 장기 snapshot은 강제 해제하지 않는다. 외부 Prepared·새 owner/ledger·가변 charge는 없다. `CatalogPreparation`의 first-job view는 기존 CREATE planner도 직접 빌리며 Prepared 아래 split하지 않는다.

source 상계는 deep health/registry, Root ReadTargets+Root decode/Job scan, Status ReadTargets+전체 page/occupied·duplicate cell decode를 별도 계수한다. caller/result/view와 Begin status reference를 더하고 공통 prepare/apply와 직렬 max로 결합한다. CREATE planner의 새 borrowed first-job helper도 같은 계산에 반영한다. ordinary caller의 준비 후 값만으로 선택 이전 비용을 덮었다고 하지 않는다. 별도 전용 gate/ledger 대안은 원 capacity/reader 정리 의미를 중복시키므로 원 wrapper 재사용을 선택한다. phase/holder 또는 동시 pool 입구가 바뀌면 재검토한다.

[manifest](../../tests/fixtures/dev3-module-c2/start-progress/manifest.json)의 compiler3·runtime5(source3·새 shared start1·기존 MC03 gap/recovery1)·같은 ELF native N02 한 번을 선택한다. 총1800초/compiler420초/runtime45초/native 원30초/admission600초/로그8MiB/stack32MiB를 실행 전에 고정한다. 새 helper 관측은 실제 Aging/Begin 성공의 Preparing/Prepared/Completed를 정지시키고 원 initial/R01 oracle과 gate/charge 연속성을 검산하며 기본 fixture 실행은 유지한다. 완료한 무변경 lifecycle/회수 wrapper는 반복하지 않는다. CREATE admission·Commit Prepared lifetime·일반 entry/return floor·Row/C3–C5·M4/M5는 미완료다.

<a id="commit-call"></a>
#### XADB-MODULE-COMMIT-CALL-001 — Commit 준비부터 같은 source 예약

2026-10-28 · D2 · main `9c334fe` · 작성/계약/통합 `/root`, 독립 `/root/source_admission_review`. [기존 writer 창](#writer-progress)의 외부 Prepared 보유 gap을 닫는 한정 후속이다. 채택/실행은 [증거](../evidence/module-independence-survey-20261026.md#c2-commit-call)로 구분한다. 일반 entry/return floor·CREATE/Row·M4/M5는 남는다.

제품 `MutationPort::catalog_commit(owner, actor, private)`는 guardless storage/pool origin 검산→원 terminal read gate→원 CommitProgress source→원 deep health/Commit 선택/준비→같은 `commit_admitted`의 stage/Pending/Drain/install/publish→checked source/gate cleanup을 수행한다. 원 preparation body는 private borrowed view로 이전하며 제품에서 Prepared를 반환하지 않는다. 과거 source/prefix/stamp 변조 fixture의 분리 준비/적용은 cfg(test)에서만 보존하고 production compiler 음성 사례로 두 구 입구의 부재를 확인한다. 이는 native/public 제품 admission 증거와 구별한다.

준비의 health/Job scan/common closure와 실제 반환 carrier를 stage의 직렬 max에 포함한다. 새 actor/private 인자는 마지막 사용으로 할인하지 않고 네 창 모두 `size_of<TxnRef>+size_of<&mut private>`를 추가한다. 원 `WalPrefix` 취득/Prepared 검산은 실제 작은 block에서 끝내므로 Pending/publish에서만 해당 값을 제거하고 preflight/stage의 기존 비용은 유지한다. 단순 wrapper나 last-use 할인보다 작은 실제 수명 변경이며 한도 확대·새 ledger·port field는 없다. 원 epoch/control 검산·authentic Pending·Drain·Q 공개·Fatal/UNKNOWN/cleanup 우선순위는 유지한다.

원 유한 read operation은 Commit 전 checked 반환하고 장기 snapshot을 강제 종료하지 않는다. Commit 준비 중에도 disjoint 원 MaintenancePort를 사용할 수 있으며 전체 호출이 flusher를 빌리지 않는다. Prepared 뒤 실제 Root flush의 full-stamp Stale/Unchanged와 WAL/control/Q 무효과·checked cleanup, 사전 고정한 새 준비1회의 C04 성공, 새 API의 Full+Pending 합을 검증한다. guardless health/역할 뒤 source Capacity가 깊은 미관측 오류보다 앞설 수 있음은 [ADMISSION-002](#operation-source-admission)의 내부 오류 순서를 따른다.

실행 전 [commit-call manifest](../../tests/fixtures/dev3-module-c2/commit-call/manifest.json)에 runtime11/compiler4/nativeN03 한 번과 총1800초/compiler420초/case45초/native30초/admission600초/로그8MiB/stack32MiB를 동결한다. 새 준비 두 unwind와 세 Fatal 경계, 실제 prepared flush·Full overlap·원 stage rejection/attempted-error를 구분한다. 원 corpus/controller와 자원 한도는 유지한다. source 식/원 gate/WAL/효과 의미가 바뀌거나 필수 상계가 초과하면 이 결정을 재검토한다.

<a id="catalog-create-lease"></a>
#### XADB-MODULE-CREATE-LEASE-001 — 원 CREATE 자원과 연속 flush 대여

2026-10-28 · D2 ACCEPTED(이 공급 한정) · 기준 main `3fa5f6f` · 작성/계약/통합 `/root`, 독립 `/root/source_admission_review`. [원 공유 claim 목표](#catalog-claim-gap)의 실제 flush 대여를 먼저 공급한다. `ReservedCatalogCreate`는 전체 session 대신 원 storage/pool/control/history/visibility와 `CatalogPairIoLease`를 보유한다. 기존 exclusive entry가 private view를 소비하여 같은 Reserve body를 실행한다. 원 `Owner`/private/definition/key는 그대로이며 아직 raw actor를 받는 기존 exclusive API다. shared 제품 API·writable statement·source admission 완료로 표시하지 않는다.

lease는 원 flush의 단 하나인 mutable borrow를 planner 종료부터 Reserve·Forward/Cancel·Before 반환까지 연속 보유한다. 원 core/binding/fence·pair와 selected generation을 동결한다. acquisition은 actual FlushIo origin, Ready, capture/durable/authority/retired 부재와 실제 `preflight`를 검산한다. Forward/Cancel은 동결 identity/generation을 원 owner와 다시 대조하고 호출 동안 같은 borrow를 재대여한다. Cancel 때 generation을 새로 골라 lease를 다시 만드는 대안은 채택하지 않는다. 원 body의 짧은 PairIoQuiescence는 이 연속 lease 아래 만들어지며 flush I/O나 generation 변경은 불가능하다. active Drop은 fence/notify 뒤 claim을 미완료로 보존한다. 정상·Before·Fatal 및 allocator/epoch 종료 의미는 유지한다.

2026-10-29 참조(S2-0): [Row 쓰기 개정](#row-write-budget)의 Row flush lease는 이 연속 대여 규칙을 PreflightRow→ReservedRow→PreparedRow에 적용한다. 획득은 같은 binding·flush I/O origin·Ready·capture/durable/authority/retired 부재를 검산하며 pair와 selected generation은 동결하지 않는다. Row lease도 Drop impl을 두지 않는다. 목적은 행이 끝날 때까지 쓰기 target에 flush capture와 eviction이 닿지 않게 하는 것이고, lease 아래에서 flush I/O는 하지 않는다.

원 pending/population/deep health와 planner 오류는 lease admission보다 먼저다. 그 뒤 flush preflight를 기존-target preparation보다 앞에서 수행하여 lease를 획득하므로, 미관측 preparation 오류와 flush 오류가 함께 있으면 flush 오류가 먼저다. 이는 내부 admission 경계의 한정 D2 구체화이며 Format/WAL 의미는 바꾸지 않는다. 단순 scalar lease나 whole bound+alias 대안은 선택하지 않는다. 원 mutable borrow를 분리한 실제 자원만 저장하며 새 owner/manager/registry는 없다.

source는 확대 reservation과 Result/Before, lease 획득/반환·IoObservation·검산 helper, 원 reserve tuple 및 caller/view를 보수 추가한다. 변경 없는 retirement peak가 더 크더라도 새 CREATE phase를 누락하지 않는다. cap 증액·optimizer 할인은 없다. test-only `CatalogTestState`는 원 pool/bank/jobs를 관측하므로 reservation에 전체 bound를 다시 구성하지 않는다. 독립 oracle의 원 byte/hash/기대값은 유지한다.

[동결 선택](../../tests/fixtures/dev3-module-c3/create-lease/manifest.json)은 compiler3·runtime10·nativeN01 한 번이다. compiler는 actual reserve/consume, 같은 원 flush 재대여 금지, consumed token 재사용 금지다. runtime은 actual FlushIo foreign origin·Ready 잔여 검산과 바뀐 reservation의 source·Forward/Cancel·Unstaged·final unwind·Drop/current recovery·pending 우선순위·Before Drop, 신규 lease identity/Before 후 성공 Cancel과 identity 변조를 연결한다. input/corpus/controller와 total1800초/compiler420초/runtime45초/native30초/review600초/log8MiB/stack32MiB를 유지한다. 완료 무변경 pair leaf/System/하위 전체 wrapper는 실행하지 않는다. [source별 결과](../evidence/module-independence-survey-20261026.md#c3-create-lease)와 실제 검토를 별도 기록하며 일반 floor·writable actor·Row/C3–C5·M4/M5는 미완료다.

<a id="c0-row-boundary"></a>
#### XADB-MODULE-C0-ROW-001 — Row 소비 경계의 컴파일 공급

2026-10-29 · D2(이 공급 한정) · 작성 기준 `76742f3`, 통합 기준 `036d4be` · branch `claude/project-thread-h2j8oc` · 작성·통합 Claude 세션, 독립 검토는 별도 읽기 전용 subagent. 실행 결과는 [증거](../evidence/module-independence-survey-20261026.md#c0-row-boundary)에 둔다.

§6.2·6.3·6.6·6.7의 live rollback·6.8의 wrapper를 실제 Rust 타입으로 공급했다. `plan4_apply/row.rs`와 자손 `row/{write,read,rollback}.rs`에 `CheckedTable`, `RowOwnedError`, `PreflightRow`→`ReservedRow`→`PreparedRow`/`RowApplied`/`RowCancelled`, `RowReadOperation`/`RowSearchPage`/`RowCandidate`/`MissingRowEvidence`/`RowCopyObservation`, `RowRollback`/`RollbackStep`/`RowRollbackTask`/`PreparedRollbackTask`/`RowRollbackDone`, `RowMutationObservation`·`RowCommitObservation`을 두고, `transaction.rs`에 `reserve_cursor_credit`·`CallerCredit::release_checked`·`finish_aborted`·`commit_row`를 더했다. `storage::disk`는 이름만 선택 재노출한다.

**효과 경계.** 모든 factory와 단계는 첫 source guard·pin·admission·WAL·page 효과 전에 Unchanged `Unsupported`를 반환한다. consuming 단계의 거절은 `RowOwnedError::Before`/`TransactionFinishError::Before`로 같은 wrapper·거래를 돌려준다. `reserve_cursor_credit`만 실제 효과(원 slot의512B 예약)를 가진다. private 필드와 외부 생성자 부재로 raw TxnRef/UndoRef/RowId/q에서 wrapper·credit·관측을 만들 수 없다. 실제 preflight/RowReserve·Row read source·rollback·Commit adapter는 C2/C3, recovery 타입은 C4가 공급한다.

**계약과 다른 점.**
- `reserve_cursor_credit`의 반환 수명을 `'r`로 좁혔다(§6.8 수정 기록).
- `RowReadOperation`은 원 lease의 generation을 별도 scalar로 기록하고 `release_candidate`가 lease generation과 capture sequence를 함께 대조한다. `ReferenceLease`의 generation은 private이므로 실제 lease 취득 adapter가 이 값을 채운다.
- `CheckedTable`은 `ObjectHandle`(generation·SchemaEpoch 포함)과 함께 원 epoch의 검산된 table slot(`u8`)을 가진다. 발급자는 2026-10-29 한정 공급 `CatalogStatementCall::finish_table`([ROW-BEGIN-001](#row-begin-capsule), [S1-f 증거](../evidence/module-independence-survey-20261026.md#s1f-finish-table))이고, §6.4 `CatalogReadOperation::finish_table`은 미공급이다.
- 명시 Drop이 없는 wrapper의 필드 순서를 checked 정리 순서와 맞췄다. read는 captured refs→lease→admission, rollback refs는 current/next pin→lease다. 실제 adapter가 자원을 채울 때 Drop 순서가 원 lease보다 pin을 먼저 반환하게 한다.
- copy 배열은 `[u8; PAGE_BYTES]`(=16384), 관측 타입의 `Debug`, `RollbackScope`의 `Copy`/`Eq`는 의미를 바꾸지 않는 표기 차이다.

**C4로 남긴 것.** §6.7의 `next_action`, `RecoveryRowTask`, `RowRecoveryPolicy`/`recover_with_rows`와 MBC00-P03/N11은 `Plan4Recovered`의 진행 상태 분리가 필요하므로 C4 공급에서 함께 구현한다.

**재검토 조건.** 실제 adapter가 Unsupported 전에 효과를 내야 하거나, credit이 view 밖으로 나가야 하는 cursor 설계, 또는 wrapper에 명시 Drop을 추가해 정리 순서가 바뀌면 이 기록을 다시 검토한다.

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

2026-10-30 개정(S2-0b, [replay 검산 재배치](#row-write-validate-once)): 위 C4 행에 진입 의무 하나를 더한다. S2-0b 뒤 Row 쓰기의 reserve·prepare·cancel은 replay를 열지 않으므로, 한 행의 Reserve frame은 같은 행의 Forward·Cancel frame 전에 replay로 다시 증명되지 않는다. [ROW-BEGIN-001](#row-begin-capsule)의 임시 분석 gate가 있는 동안 재시작 결과는 지금과 같은 READY0·CallerMustResolve다. C4가 그 gate를 대체한 뒤에는 다르다. live page와 control은 서로 맞고 replay와만 다른 Reserve 설치의 불일치(코드 결함)가 Forward·Cancel frame이 before로 접는 bytes에 있으면, replay할 수 없는 durable Forward·Cancel frame이 남아 분석이 그 frame에서 CallerMustResolve로 끝난다. 앞의 경우(live끼리는 맞는 불일치)가 S2-0b 전에는 READY에 이른다. live끼리 맞지 않는 불일치는 S2-0b 전후 모두 live Forward·Cancel 준비의 census digest·Jobs page 대조가 그 frame 전에 거절하므로 이 의무의 대상이 아니다. C4는 착수 전에 D2로 다음 둘 가운데 하나를 정한다. (1) 이 위험을 받아들인다. (2) 둘째 frame 전에 도는 행 단위 검산을 되살린다. 후보와 분석 상계 여유는 그 기록의 "거절·보류한 대안"에 있다: (H) 단계마다의 `validate_live`를 준비 함수 밖 첫 부분 단계로 올림(약10.7–10.9KB), (ii) 별도 C 호출 `ReservedRow::validate()`(약11.0–12.0KB, A 호출 모양 변경), (B′) reserve 끝의 `validate_live`(약10.2–10.7KB), (i) 단일 frame 증분 replay(맞지 않음). 판정은 그때의 MCA00-D15 호출 지점별 몫으로 하며, S2W-R24가 출력한 replay cause와 재시작 결과를 입력으로 쓴다.

현재 C0는 [borrowed codec](../evidence/module-independence-survey-20261026.md#c0-borrowed-codec)과 [생산자 관측·대여 타입](../evidence/module-independence-survey-20261026.md#c0-producer-types)을 부분공급했다. 2026-10-29 [Row 소비 경계](#c0-row-boundary)의 wrapper·수명·credit을 컴파일 공급했다(본문은 Unsupported). C1의 [물리 WAL 구현 분리](../evidence/module-independence-survey-20261026.md#c1-physical-wal)에 이어 [원 owner/port 이전](../evidence/module-independence-survey-20261026.md#c1-owner-ports)의 컴파일4건·신규 메모리2건을 한정 수용했다. [control/isolation 소유·4160B 분리](../evidence/module-independence-survey-20261026.md#c1-isolation-backing)는 컴파일3·변경/신규 runtime4로 한정 수용했다. [epoch1600B 원 소유](../evidence/module-independence-survey-20261026.md#c1-epoch-backing)는 compiler3·변경 runtime3 PASS·독립 D1/D2 한정 수용이다. [Catalog epoch 활성화](../evidence/module-independence-survey-20261026.md#c1-epoch-activation)는 실제 seeding·CREATE/Commit을 연결하고 source별 선택 결과와 원 FAIL을 보존했다. [읽기 전용 거래/statement 수명](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)은 별도 부분공급했다. [C2 동기 Catalog 제품 본문 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)는 실제 source adapter와 기존 소비 호출을 연결하고 source별 compiler·runtime7을 검증했다. 한정 내부 statement factory/자동 Catalog read bit는 [후속 공급](#catalog-statement-supply)에 기록하며 CheckedTable/Row 전체 공급과 구별한다. 쓰기 거래·Row read-set·전체 short admission·terminal 이전·성공하는 빈 Commit과 전체 복구 handoff 인수는 남아 있으며 전체 C0·C1–C5 공급과 제품 인수는 미완료다. C0는 M2 문서 채택만으로 완료되지 않는다. M3 수용 뒤 M4에서 최소 컴파일 경계가 실제 제공된 범위부터 A/B가 착수할 수 있다. C0의 계약 대역은 canonical row/schema 입력, finite logical Status/Undo 응답, explicit Busy/Capacity/Unsupported 같은 효과 없는 선택을 반환할 수 있다. 실제 `WalRequirement`, pin, snapshot/READY, install receipt, file effects를 제조할 수 없다. 대역으로 시험한 결과는 순수 기능 범위의 결과이며 C 수신 전 실제 저장·가시성·복구 성공으로 승격하지 않는다.

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

2026-10-29 개정(S2-0, [Row 쓰기 개정](#row-write-budget)): 위 표의 다음 행을 이렇게 바꾼다.
- MBC00-P01은 `preflight_row`에 flusher를 넘기고 `RowPreflightStep`의 두 분기를 컴파일하며, lease에 Drop impl이 없어 `FlushRequired` 분기에서 flusher를 다시 빌릴 수 있음을 함께 확인한다. P05의 mutable borrow에는 A scratch가 들어간다.
- MBC10-V09의 oracle을 다음으로 대체한다. 원 문언(writer1+resident reader2의 겹친 nested sum)은 RowWrite의 gate 결정과 양립하지 않고 writer ≤ memory9592/native9456B를 요구하므로 폐기한다. 대체 oracle: resident reader 2개가 admitted된 뒤 writer는 gate에서 두 반환을 기다리고, 그동안 새 reader는 `InstallationPending`이다. 마지막 admission의 −1/동일/+1은 시험 전용 ledger charge로 남은 몫을 제한해 적용하며 cap을 바꾸지 않는다.
- MBC40-V09의 "동시 생존"을 다음으로 대체한다. census는 writer phase(반환 오류 최대·caller carrier 포함)와 resident 2개를 각각 shared에 더해 출력하고, 둘이 동시에 admitted되지 않음을 검산하며, gate 대기 중 writer 진입 floor를 따로 출력한다. 그 floor는 [WRITER-PROGRESS-001](#writer-progress)의 미공급 의무이므로 공급 전에는 PASS로 세지 않는다.
- 두 대체 oracle의 PASS는 writer와 resident reader 2개의 겹친 admission을 증명하지 않는다. 같은 이유로 소비자 MCI03-V02의 barrier도 바뀐다([Row 쓰기 개정](#row-write-budget) 7항).
- selector 이름 대응. 위 계획 경로와 다른 이름으로 실행된 MBC20·MBC30·MBC31 사례는 증거에 대응표 하나(ID → 실제 test 경로 → round/artifact → 결과 → source commit)를 둔다. 대응된 행은 기존 실행을 인용하고, 대응되지 않은 ID는 NOT_RUN이며, 이름을 맞추려고 다시 실행하지 않는다. 신규 S2/S5 시험은 ID 접두 이름(`mbc30_v01_…`, `s2w_r06_…`, `s5r_r02_…`)을 쓴다.

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

2026-10-29 Q06 재검토 기록(S2-0, D2 수용): A의 측정 Forward producer phase 60625B(S1-e 뒤 driver 9955B로 60745B)가 RowWrite charge 44304B와 class 상한 memory45352/native45216B를 넘었다. 같은 producer를 쓰는 CLR phase도 57974B로 넘으며 S4의 진입 측정 대상이다. cap 증액이나 caller frame 제외 없이 A slimming(M1·M2, 조건부 M7)·lane 8번째 part·호출 지점별 계수·128B cut으로 대응한다. S2-e는 A가 실제 타입으로 memory·native 모두 측정한 Forward producer phase가 charge 이하이고 MCA00-D15의 C 호출 지점 몫이 `ROW_WRITE_CALLER_BYTES` 이하일 때만 착수한다. C 단계 phase는 경로 전체 inventory로 계산한 분석 상계로는 M7 없이 charge를 넘을 수 있고 M7로도 Reserve·Forward 준비·Cancel이 경계이므로, S2-a의 상계 출력과 각 단위의 census 통과가 그 경로 commit의 조건이다. 같은 재검토에서 기존 초과 하나를 그대로 남긴다. native W의 shared+Pending+Full=66612B>65536B(memory 65452B, 여유84B)이므로 [WRITER-PROGRESS-001](#writer-progress) (c)의 "한 Full read는 들어가고"는 native에서 성립하지 않으며 Full을 재사용하는 RowRead도 그 창에서 Capacity다. 정정은 Commit 창 owner의 별도 Q06 기록이고 이 개정은 cap·charge를 바꾸지 않는다. 세부는 [Row 쓰기 개정](#row-write-budget)에 있다. 2026-10-30 정정: 이 기존 초과는 시험 전용 native adapter의 값이었고 `94ede44` 뒤 native는 65460B로 cap 안이다([Row 쓰기 개정](#row-write-budget)의 정정 문단).

2026-10-30 Q06 재검토 기록(S2-0b, D2 수용): S2-a의 C 단계 상계(이 branch `dd9bd31`, census `9aeff2a`)에서 reserve·Forward 준비·Cancel의 A 몫 여유가 218·58·178B였다. cap·charge·field를 바꾸거나 caller frame을 빼지 않고 다음 셋으로 대응한다. (1) `validate_live`와 T의 arena 검산을 한 행에 한 번 preflight에서 하고, 나머지 단계는 cut commitment의 digest 연쇄로 잇는다(세 단계 C 본문 약−7.4KB). (2) group carrier를 가진 단계(apply와 깊은 검산 부분 단계 포함)의 carrier 계수를 중단 호출 단계마다의 반환 carrier로 정정한다(약−5.2KB, 중단 단계 하나는 본문 의무). (3) A 몫 상한을 preflight 6144B와 나머지 11520B로 나눈다. 분석 상계로 C 쪽 여유는 preflight 약2.1KB, apply 약3.6KB, 나머지 효과 단계 약1.2KB 이상이다. M7은 MCA00-D15에서 A 몫이 preflight 밖의 C 호출 지점에서 `ROW_WRITE_CALLER_BYTES`를 넘거나, preflight 호출 지점에서 (E)·(d4)를 거친 뒤에도 `ROW_PREFLIGHT_CALLER_BYTES`를 넘을 때만 필수다. 행 안 Reserve frame의 replay 증명이 다음 `validate_live`로 늦어져, C4 뒤 live끼리는 맞고 replay와만 다른 코드 결함이 replay할 수 없는 Forward·Cancel frame을 남길 수 있다는 위험은 의도해 받아들였고, C4 진입 의무와 S3 의무를 두었다. 세부와 대체 순서는 [replay 검산 재배치](#row-write-validate-once)에 있다.

[실제 replay 작업 공간 대여](../evidence/module-independence-survey-20261026.md#c2-borrowed-replay-workspace)는 원 WORK_END zero tail의3328B를 semantic observer에 빌리고, 정상/오류/unwind 뒤 원 zero tail을 복원한다. producer의 local 배열과 wire/claims/hash·전체 backing은 유지한다. 실제 guard/반환/helper 계수 뒤 Full33980B·Resident16936B/shared22256B다. Pending 관측까지61556B의3980B 여유는 전체 entry/tail·독립 guard leaf·writer ticket을 제외한 값이며 전체 admission 미완료다. source별 선택9 PASS·import-only build와 원 compile FAIL을 구별해 보존한다.

[exclusive split의 고정 source 계산](../evidence/module-independence-survey-20261026.md#c2-split-source-charges)은 원 DiskSession의 독점 borrow에서 계산을 끝내 ReadPort에 private scalar3개를 보유하고 Catalog entry에서는 원 값을 읽는다. source ticket은 checked<=65536 성공 뒤u32로 보관하며 ledger u64/zero 영역과 권한·정리 순서는 유지한다. shared22400B에는 실제 split3port144B를 한 번 더한다. 별도 계산/tuple 반환과 recovered step caller를 계수한 live peak63328B/recovered64704B·Full33972B/Resident16928B이며 전체 entry/tail·writer admission은 미완료다.

[예약 밖 호출 phase 관측](../evidence/module-independence-survey-20261026.md#c2-uncharged-call-phases)은 같은 원 core의34입구(진행 허용33과 구별) 및 네 독립 mutex의 guarded 합을 분리한다. 작은 source 실패 helper는 원 progress를 소비한 뒤 Err<Scope>를 구성하며 원 Fatal/cleanup 우선순위를 유지한다. cfg(test) compact floor15057B와 기존 query의 wide/lexical tail7368B는 넓은 관측식이고 실제 최소치·제품 예약 보증이 아니다. Full33972B/shared22400B 이후9164B에는 이 상계를 그대로 넣을 수 없으며 기존 selected caller와 writer/progress 전체 admission은 미완료다. foreign Owner 수를34로 제한하지 않는다.

[query 정상/오류 종료 경계](../evidence/module-independence-survey-20261026.md#c2-query-tail-boundaries)는 정상 결과 소비의 lexical block을 consuming close 전에 끝내고, 오류 arm은 전체 lookup Result를 보존한다. finish_error는 원 오류를 유지한 작은 close 뒤 wide 변환하며 원 Fatal/cleanup 우선순위를 유지한다. 정상710B/오류1394B는 한 호출 관측식이고34입구 전체 floor나 제품 admission이 아니다. 기존7368B baseline·Full33972B/Resident16928B/shared22400B와 전체 공급 미완료는 보존한다.

[원 DPT 저장 축소](../evidence/module-independence-survey-20261026.md#c2-compact-dpt)는 같은 directory에 full identity를 한 번 보유하고 private 행3072B를 줄였다. expected/원 요구 권한과 publication·generation 검산은 [dirty owner](dev3-dirty-stage.md#compact-dpt)를 따른다. helper660B를 actual source에 연결한 현재 shared19328B/Full33972B/Resident17588B이며 전체 short floor·writer phase는 미공급이다. 과거 shared22400B 수치는 당시 source의 증거다.

[LOAD 검산 claim borrow](../evidence/module-independence-survey-20261026.md#c2-borrowed-load-claims)는 같은 registry 아래 기존 conflict predicate에 원 claim을 참조로 전달한다. 관측 pool guard3307B·guard 합6820B·compact floor14776B는 제품 admission 미연결이다. load_eligible 및 다른 source class의 전체 수명 보증으로 확대하지 않는다.


<a id="catalog-capsule-completion"></a>
## 실제 첫 Catalog statement의 거래 완료 연결

`XADB-MODULE-CATALOG-CAPSULE-001`은 `4ac878c`의 원 Begin/512B capsule에 실제 첫 Catalog Forward의 statement 종료와 consuming Commit/full abort를 연결하는 D1/D2 구현 단위다. source별 compiler3·runtime13을 검증했고 최종 독립 D1/D2 한정 수용·필수 수정0을 확인했다. [증거와 동결 선택](../evidence/module-independence-survey-20261026.md#c3-catalog-capsule)을 따른다.

- wire의 원 statement/mutation은1이다. 첫 빈 view(sequence1/revision0)의 실제 actor를 검산하고, 실제 CREATE initialize의 object와 원 RootRetained를 대조한 뒤 revision1/catalog read-write bits3으로 등록한다. 완료한 view만 소비하며 일반 Row mark를 제조하지 않는다. 이 단위의 실제 CREATE 연결은 cfg(test) bridge이고 제품 shared reservation admission은 미공급이다.
- Commit은 live view가 없는 원 Active capsule, 등록한 실제 root, isolation read/write 집합을 검산한다. Serializable은 원 시작Q 이후 실제 Catalog epoch 변경이면 MustAbort/SerializationConflict를 반환하고 ReadCommitted는 Commit을 계속한다. 검산부터 실제 Commit/Q publication까지 같은 exclusive MutationPort/control borrow를 유지한다.
- 실제 Commit/AbortEnd의 observation·actor·terminal ordinal/status/adoptionLSN/outcome과 실제 Q를 확인한 뒤 각각 Committed4/Aborted3으로 snapshot·slot·512B lease를 반환한다. readonly는 phase1만 정상 반환한다. Commit 이후 검산/정리 오류는 Fatal이며 Before 성공처럼 반환하지 않는다.
- 전체 abort는 원 Active(논리 MustAbort 포함)에서 실제 RollbackStart를 수행하고 즉시 MustAbort로 기록한다. 원 Aborting/head에서 실제 CLR, compensated root 또는 빈 Aborting에서 실제 AbortEnd를 수행한다. 중간 Capacity는 같은 MustAbort capsule을 반환하며 완료한 WAL 단계를 반복하지 않는다. compensated root는 Aging 의무로 남으므로 RootFree를 기다리지 않고 거래 snapshot을 반환한다.
- 깊은 ATT/Job/ledger 검산은 원 read gate와 고정 progress source 예약 안에서만 실행한다. 검산 예약은 각 nested WAL 동작 전에 종료한다. 새 helper 최대값과 nested caller carrier를 기존 고정 비용에 계수하며 장기512B/short65536B 한도는 변경하지 않는다. 전체 entry/return floor와 RSS/stack 인수는 이 한정 source 식의 범위가 아니다.

성공하는 빈 Commit, 두 번째 쓰기 statement, own-actor 읽기, 일반 Row rollback/Commit, shared CREATE 제품 경계와 C4/C5·M4/M5 인수는 미완료다. 원 wire/fixture는 바꾸지 않으며 새 Commit/AbortEnd image의 현재-build 메모리 복구를 선택한다. 기존 native 완료 사례·전체 workspace/lower wrapper는 반복하지 않는다.

<a id="catalog-shared-create"></a>
## 원 statement에 묶인 shared CREATE 예약

`XADB-MODULE-CATALOG-SHARED-CREATE-001`은 `9f36840`의 첫 Catalog 거래 완료 위에 실제 `MutationPort::reserve_catalog_create`를 공급한다. source별 선택 검증과 최종 독립 D1/D2 한정 수용·필수 수정0을 확인했으며 [source별 증거](../evidence/module-independence-survey-20261026.md#c3-create-admission)와 최종 검토 판정을 따른다. 앞 절의 cfg(test) 연결 증거와 이번 제품 진입점의 실행 증거를 구분한다.

- 입력은 실제 `RowStatementView`와 원 private/definition/Undo/Owner/MaintenancePort다. 원 isolation/visibility/slot generation·sequence1/revision0·빈 Active actor를 검산하고 actor는 내부에서 얻는다. raw actor나 caller가 선택한 source byte 수를 권한으로 받지 않는다.
- 얕은 health/origin 뒤 원 read gate를 닫고 split에서 계산한 고정 `CatalogProgressCharge`를 예약한다. 깊은 transaction/planner/ledger 검산은 그 안에서 실행한다. 장기 snapshot/view는 살아 있을 수 있지만 finite read operation의 대여는 gate가 정리한다.
- 기존 `ReservedCatalogCreate` 하나가 원 claim/ReserveLSN/plan/flush lease에 실제 view binding·source·gate를 함께 보유한다. source 재예약·다른 owner·복제 claim을 만들지 않는다. 진행 중에는 원 RootReserved의 actor/object/ordinal/LSN을 검산하며 빈 actor 검산을 재사용하지 않는다.
- Catalog read bit는 인증된 view의 planner 호출 직전에 보수적으로 기록한다. 실제 Reserve 뒤 write bit를 함께 기록하며 그 뒤 기록 실패는 Fatal이다. 초기 planner Before와 Cancel은 관측 bits를 지우지 않는다.
- Forward는 원 Active/root/object를 확인한 뒤 revision1, Cancel은 빈 Active를 확인하고 revision0을 유지한다. 같은 첫 view의 Cancel→재Reserve를 허용하되 발급 high-water·WAL·generation을 되감지 않고 새 실제 발급값을 사용한다.
- 실제 Forward/Cancel 뒤에는 등록·source 반환·gate 재개 오류가 Unchanged여도 Fatal이다. source/gate 양쪽 반환을 시도한 뒤에만 성공한다. 정상 성공은 `CatalogCreated::into_parts` 또는 `CatalogCancelled::into_private`로 원 private 대여를 한 번 반환한다. 초기 Before도 `CatalogCreateError::Before { error, private }`로 원 대여를 반환한다. private/definition 수명은 짧은 writer/owner/view/flush 차용과 분리한다.
- 효과 전 Before는 동일 reservation과 모든 대여를 보존한다. active Drop/unwind는 먼저 session fence·notification을 수행하고 RAII로 source/gate 반환을 시도한다. 이미 fence된 source는 반환을 거절하므로 charge를 보존하고 원 gate 해제도 healthy 재개로 해석하지 않는다. WAL Cancel·claim 삭제·거래 완료를 제조하지 않는다.
- 확대 token/Result/Before·binding·admission·완료 반환·cleanup은 CREATE phase에만 계수한다. 기존 고정 progress의 max에 Reserve/Forward/Cancel을 포함하며 per-call recursive census와 새 retained port 필드를 만들지 않는다. 512B/65536B 한도·복구 의무·기존 source별 FAIL은 유지한다.

일반 statement mark·두 번째 쓰기 statement·own-actor Catalog 읽기·자동 Status/Undo 선택·Row/C4/C5·전체 short floor·M4/M5는 이 공급으로 완료되지 않는다. native 타입 source census와 실제 native durability 실행도 구분한다.


<a id="catalog-own-read"></a>
## 원 writable statement의 첫 Catalog CREATE 조회

`XADB-MODULE-CATALOG-OWN-READ-001`은 `830d053` 위의 한정 공급이다. 앞선 writable-empty 절의 Unsupported는 당시 source의 상태이며 이번 경로에서 대체한다. 구현·source별 선택 검증과 독립 D1/D2 한정 수용·필수 수정0을 확인했으며 [증거](../evidence/module-independence-survey-20261026.md#c3-own-catalog-read)에 기록한다.

- 기존 `CatalogStatementCall`은 원 view를 product lookup에 참조로 전달한다. legacy snapshot lookup은 None을 전달한다. 새 port/scope/operation 필드·raw actor 승인 API를 추가하지 않는다.
- source admission 안에서 원 isolation/visibility와 snapshot 주소, slot generation·Active phase·sequence·빈 mark를 검산한다. canonical mode0/1과 원 SER/RC snapshot 형태도 일치해야 한다. 빈 writable view의 Missing/타인 조회를 허용한다.
- 자기 actor의 후보는 full TxnRef 일치, 첫 statement/revision1, logical actor/mutation1, not-retired, 실제 Undo 참조를 요구한다. 실제 물리 Status를 읽고 guard를 checked release한 뒤 Active와 같은 Undo head를 검산한다. 모순은 Fatal이며 타인 snapshot 경로로 바꾸지 않는다.
- own=true도 기존 TxPin/UndoPin과 실제 CREATE Missing-before의 creator·target object·canonical material을 검산한 뒤 Found를 선택한다. 지속 발급값인 Undo.sequence는 1로 고정하지 않는다. Cancel 뒤 재Reserve의 새 발급을 유지한다.
- 타인은 원 Q/Status/Undo 규칙을 유지한다. 결과 반환 직전에 원 view를 다시 검산하고 read bit를 등록한다. Found는 원 call/private/view 수명 안에서만 사용하고 refs→source→progress의 기존 checked cleanup을 유지한다. 활성 Reserve 중 읽기는 기존 gate가 거절한다.
- 추가 view 참조/Status/tuple Result/helper carrier는 Full/Resident source 식에 계수한다. 기존65536B 한도와 source별 원 FAIL을 보존한다. 새 WAL/영속 형식/복구 상태는 만들지 않는다.

일반 Row, 두 번째 쓰기 statement/일반 mark, CheckedTable, C4/C5, 전체 entry/return floor와 M4/M5는 미완료다. 이번 native-type census는 native 실행 증거가 아니다.

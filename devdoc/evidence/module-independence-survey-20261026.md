# 모듈 독립 개발 전환의 기준선·경계 조사

2026-10-26 · **M0 기준선 확인 / M1 한정 조사·독립 검토 완료 / M2 설계 중·미채택 / M3–M5 미완료 / 제품 변경·새 시험 없음**

[실행 계획](../plans/module-independence-plan.md#execution) · [현재 DEV3](../implementation/dev3-implementation.md) · [파일 소유 정책](../policies/parallel-development-policy.md)

사용자의 “이 계획서를 실행해 주세요” 요청으로 M0–M5를 진행한다. 이 기록은 실제로 확인한 기준선과 조사만 보존하며, 계획 채택·과거 시험을 새 구조의 인수로 사용하지 않는다. 작성/통합은 `/root`, M1 소유 경계 조사는 `/root/module_boundary_survey`, 소비 기능 조사는 `/root/module_consumer_survey`다. 두 조사는 읽기 전용이며 M5의 제품 병렬 인수로 세지 않는다.

<a id="baseline"></a>
## M0 기준선과 동시 작업

2026-10-26 16:58–17:03 KST에 Git 상태·앱 작업 상태·실제 프로세스를 확인했다.

| 대상 | 관측·처리 |
| --- | --- |
| 기준 source | main `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`. tracked dirty/index, 관련 untracked, 미해결 merge index 항목 모두 없음. 따라서 기준 source에 더할 dirty digest 없음 |
| 별도 checkout | `dev3-allocator-eviction/xadb`의 `89cdcbb`, `dev4-merge-review/xadb`의 `6da1a8e`, `/home/et16/work/xadb-dev4`의 `7327e7a`는 각각 `git status --short`가 비어 있음. 해당 파일·branch·artifact는 수정하지 않음 |
| Codex 작업 상태 | 앱 목록에서 이 작업만 active, 최근 DEV3 검증 작업은 idle, DEV4 리뷰 작업은 notLoaded. 두 관련 작업의 최근 turn은 completed로 확인. 관측 시점 상태이며 이후 재개까지 금지됐다는 뜻은 아님 |
| 실행 중 job | `ps`의 XADB/cargo/rustc/시험 명령 대조와 `/proc`의 cwd/exe 대조에서 제품 시험·빌드 process를 관측하지 않음. 검사 자신의 shell/Python 및 앱 browser runtime은 구분 |
| 미확인 외부 작업 | main을 cwd로 가진 Claude PID 15334·15599·16172와 shell은 살아 있음. 이 세션의 편집·보류 여부는 미확인. idle/완료를 추정하거나 연락·종료하지 않음 |
| 작업 격리 | 기준 commit에서 이 채팅에 연결한 `/home/et16/.codex/worktrees/module-independence/xadb`를 생성. 구조 작업의 편집은 이곳에 한정하며 main 및 기존 worktree의 동시 쓰기를 피함. worktree 자체를 독립 기능 인수로 세지 않음 |
| 보류·재개 | 이번 작업에서 선정 DEV3 공유 경계의 새 기능 확대는 M2/M3 채택 및 개별 공급 조건에 맞춰 배정. 다른 채팅을 중지한 사실은 없으며, main 합류 전 source diff와 실제 진행 상태를 다시 확인 |
| 기존 증거 | Catalog CREATE와 후속 v2 Root/Jobs 수정, DEV4 병합 결과를 포함한 현재 commit을 사용. PF04 FAIL·DEV2 미완료, Catalog runtime F05/일부 native 의무, 첫 Row INSERT·DEV3/S5 미완료를 보존 |

`/root`는 계획 실행 기록·이번 조사·향후 선정 소유 계약/정책·공용 색인 수정의 단일 담당이다. M1의 두 조사는 제품·문서 파일을 쓰지 않는다. 실제 A/B 담당·전용 경로·공통 공급·인수는 M2에서 확정하며, 현 후보를 동결 API나 구현 착수 증거로 취급하지 않는다. 외부 세션이 사용할 main과 분리된 checkout에서 문서 작업을 진행하고 M2/M3 채택·M4/M5 착수 때 관련 upstream diff를 대조한다.

<a id="survey"></a>
## M1 조사와 남은 설계

조사 source는 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`이며 후속 `61b386c`는 이 기록·계획 상태만 추가했다. 두 담당의 읽기 조사 결과를 `/root`가 다음과 같이 합쳤다. M2의 구체 계약·독립 D2는 미완료다.

### 원 상태 소유와 실제 접근 경로

| 실제 소유자·source | 보유 상태와 수명 |
| --- | --- |
| [WalRuntime·GenesisWalSession](../../crates/engine/src/storage/wal_io.rs) | 원 WriterNamespace·열린 WAL File·slot/prefix/state·Condvar·SessionFence·Profile. Genesis가 runtime을 한 번 소유 |
| [InstalledStorageCore](../../crates/engine/src/storage/wal_io/bootstrap.rs) | 같은 runtime을 이동받고 BootstrapPlan·reader claim을 소유 |
| [DoublewriteStorageSession](../../crates/engine/src/storage/wal_io/doublewrite_install.rs) | Fresh owner·DWB loan·InstallPlan. 별도 runtime을 생성하지 않음 |
| [MutableStorageOwner](../../crates/engine/src/storage/wal_io/mutable_storage.rs) | 원 core·DWB plan·workspace loan·unique binding. selected recovery도 검증된 SelectedEpochParts를 이동받음 |
| [Plan4Pages](../../crates/engine/src/storage/wal_io/plan4_pages.rs) | Mutable owner를 borrow하고 원 SharedPool·operation/DWB loan을 소유 |
| [Plan4Retention](../../crates/engine/src/storage/wal_io/plan4_retention.rs) | Plan4Pages·control bank·RuntimeHistory/replay·flush control·visibility·recovery credit. `split`이 동일 상태를 writer/reader/flusher에 대여 |

현재 물리 파일 배치와 Rust 모듈의 포함 관계는 다르다. `#[path]`로 연결된 실제 의미 경로는 `storage::wal_io → bootstrap → doublewrite_install → mutable_storage → plan4_pages → retention`이며 `create_pair::{catalog_live,catalog_commit}`, `read`, `load`, `visibility`, `history`, `flush`가 마지막 아래에 있다. descendants가 ancestor private 상태에 접근하고 소비자 권한을 반대 방향으로 여러 단계 재노출한다. 소유자의 유일성은 보존할 기반이며, 이 중첩 전체를 새로운 owner로 복제하는 것은 해결책이 아니다.

### 의존 분류와 소비자 차단

| 관측 경로 | 분류 | 보존 또는 변경할 이유·M2 질문 |
| --- | --- | --- |
| WalRequirement·durable proof와 같은 fence, [원 Commit PinSet](../../crates/engine/src/storage/dirty_frame/commit.rs) | 필수 의미 | 정확 staged frame·원 core·whole target pin이 Commit을 지나 유지돼야 함. generic LSN·page bytes로 권한을 재발급하지 않음 |
| Plan4Retention::split과 source latch 아래 [CatalogSource](../../crates/engine/src/storage/wal_io/plan4_retention/visibility/source.rs) | 필수 의미 | 같은 pool/visibility 수명과 실제 참조 발급을 유지. 단순 decoded TxnRef/UndoRef는 usable pin이 아님 |
| CatalogCommitAttempt/Installed·TxPair·retirement 타입의 중간 re-export | 단순 등록 | 신규 소비자가 정상 연산과 무관한 설치 모듈을 여러 개 수정하게 함. 책임별 좁은 공개 경계와 단일 조립 지점으로 옮길 후보 |
| [reader](../../crates/engine/src/storage/wal_io/plan4_retention/read.rs)의 `storage.core.plan`, [history runtime](../../crates/engine/src/storage/wal_io/plan4_retention/history/runtime.rs)의 binding/Condvar 직접 접근 | 불필요한 내부 접근 | 필요한 것은 검증된 target·namespace·health·원 fence/notify다. 원 필드를 공개하는 대신 동일 수명의 제한 포트가 필요한지 대조 |
| [commit_bridge](../../crates/engine/src/storage/wal_io/commit_bridge.rs)의 CatalogCommitAttempt/Installed import | 공통 계약 미완료 | 물리 WAL이 상위 Catalog 의미를 알고 있음. 실제 semantic receipt와 whole-install/publication 조립을 거래 경계가 소유하도록 변경 필요 |
| [DirtyInstallPermit](../../crates/engine/src/storage/wal_io/dirty_allocation.rs)의 소비자별 factory | 공통 계약 미완료 | A를 추가할 때 또 하위 구현에 업무 등록이 필요. sealed prepared change와 실제 원 target 재검사를 공통 apply에 연결하는 설계 필요 |
| Catalog 9-target INIT·TxPair claim | 필수 의미·지원 범위 한정 | CREATE가 새 page 두 개를 함께 설치하는 의미다. 이미 CREATE된 RowLeaf에 INSERT하면서 이 claim을 복제하면 잘못된 authority가 됨 |
| Catalog-only UndoShape/source/lookup, owner당 reference2행 | 공통 계약 미완료 | row source·Undo 이전 버전·3-pin handoff는 현재 공급되지 않음. signature 이동만으로 행 조회가 가능하다고 표시하지 않음 |

새로운 frame registry·fence·writer owner, 공개 mutable context, 모든 함수를 trait로 바꾸는 방식은 현재 관측된 차단의 해법으로 채택하지 않는다. 상태 복제 없이 실제 소비 경로가 제한 포트를 사용하도록 해야 한다. 정적 경로를 참조하는 시험·fixture도 있으므로 전체 디렉터리 이동을 기본 전제로 삼지 않는다.

### 두 제품 후보와 남은 설계 배정

| 후보 | 관측 가능한 기능과 선택 판단 |
| --- | --- |
| A 첫 영속 Row INSERT · 3.06.2/3.09/3.11/3.12 | 실제 CREATE table에서 INSERT·Commit ACK·새 process 복구 후 행 유지. 실패 statement는 앞선 성공을 보존하고 full abort는 전체를 제거. 우선 설계 후보 |
| B snapshot row point-read·순서 cursor · 3.06.3/3.10 | 같은 실제 page/Status/Undo 경계에서 fixed-q·own write를 판정하고 caller-owned 행을 반환. full key 재탐색과 private revision 무효화. 우선 설계 후보 |
| Catalog DROP | occupied before·종속 index 원자 retire·old snapshot 수명까지 필요. 최초 두 소비자에는 선택하지 않는 대안 |
| 기존 CREATE의 native 잔여 시험 | 신규 제품 기능이 아니므로 M5 두 번째 기능으로 사용하지 않음 |
| codec·oracle 두 작업 | 실제 저장/조회 동작이 없어 M5 기능 쌍을 대체하지 않음 |

A의 실제 INSERT 후보 생산·row CLR/statement 정리와 B의 탐색·visible 판정·출력/cursor 본문은 M5에 남긴다. C가 `insert(values)`나 완성된 row lookup을 제공하여 A/B를 wrapper로 만드는 분해는 사용하지 않는다. C는 원 owner의 적용/참조 권한, 공통 WAL·종료·복구, 실제 CREATE identity를 제공한다.

M2에서 닫을 항목은 (1) RowId 영속 발급·취소/abort 뒤 비재사용, (2) 다중 INSERT head chain·statement mark, (3) source latch를 유지한 Missing read/CLR 최대2-pin handoff와 일반 occupied before3-pin의 제외 근거, (4) statement/cursor identity·revision·오류 진행점, (5) Forward/CLR/terminal/recovery grammar, (6) opcode별 F05와 동시 생존량이다. 각각 실제 owner 계약에 제안하고 독립 검토를 받는다. 이 목록을 미래 구현 의무와 혼동하여 설계 공백이 있는 상태에서 M2를 완료하지 않는다.

B 단독 fixture는 A의 live producer를 호출하지 않고 독립 Row 이력 corpus를 C의 실제 semantic observer/replay로 받아 원 owner에 복원하는 방향을 검토한다. 실제 CREATE table/anchor에서 시작해야 하며 raw RowLeaf·q·TxnRef 주입으로 권한을 꾸미지 않는다. fixture 생성·입력 검산·권한 수신 경로의 명세가 확정되기 전에는 C+B 인수 가능으로 표시하지 않는다.

두 읽기 조사가 끝난 뒤 별도 M2 초안 작성 역할을 배정했다. 이 후속 문서 작성은 M5 제품 개발 겹침의 증거가 아니다.

<a id="review"></a>
## 검토·실행 구분

2026-10-26 `/root/module_handoff_review`가 이번 M1 소유표·모듈 중첩·의존 분류와 `b7ce51d→61b386c`의 제품 무변경을 원 source에서 읽기 전용 대조했다. 함께 [D3-ROW-REFERENCES-001 제안](../contracts/dev3-visibility-retirement.md#row-missing-traversal)과 [D3-ROW-CURSOR-001 제안](../contracts/dev3-task-06-tree-rows.md#module-statement-cursor)을 기존 source/pin/lookup/retirement·Missing Undo 표현·R7/R8/F02에 대조했다.

필수 지적1건은 이 조사에 남아 있던 “M2 필수3-pin”과 Missing 한정 최대2-pin 제안의 모순이었다. `/root`가 조사 항목을 최대2-pin handoff·일반 occupied3-pin 제외 근거로 수정했고, 같은 별도 검토자가 최신 해당 행과 PROPOSED 상태 표기를 다시 읽어 미해결 필수0을 확인했다. Missing 표현이 before stamp/logical·previous-version 참조를 금지하므로 두 handoff의 최대2가 성립한다는 근거를 남겼다.

이 결과는 M1 조사 및 두 제안 절의 기본 불변식 대조에 한정한다. 정확 Rust lifetime·seal 배치, opcode/F05·독립 fixture·M2 전체 수용을 대신하지 않는다. 두 owner 절은 아직 PROPOSED다. 테스트·빌드·문서 verifier는 실행하지 않았고 M4/M5 인수나 실제 기능 제공 상태를 승격하지 않았다.

### M2 초안 저장 시점의 미결 사항

후속 한정 검토에서 `/root/module_handoff_review`는 Row opcode의 byte 산식·H 참조 net0 검산·최대87개 reference scratch의 계산을 대조했으나 전체 F05나 M2를 수용하지 않았다. 같은 table의 부재/range를 읽은 두 거래가 서로 다른 RowId를 INSERT하는 경우, 유일 발급과 row 소유권만으로 기본 Serializable 충돌을 검출할 수 없다는 필수 설계 공백을 확인했다. [MCQ07](../contracts/dev3-module-consumers.md#open-questions)에 미해결로 남겼다.

원 publication owner의 bounded table/catalog epoch와 원 Commit coordinator에서 validation부터 publish까지 직렬화하는 해소 방향을 논의했다. READY 초기화, read-only Serializable 종료, RC statement snapshot, 실패 statement의 관측 집합 보존, CheckedTable 재사용 시 실제 수명 검산, stage 전 slot/변경 table 목록 선예약과 추가 metadata/source F05까지 명세해야 한다. 이는 아직 owner 계약·정확 API에 반영하거나 독립 D2로 채택한 결과가 아니다.

공통 경계 담당은 기존 owner/port의 signature·reborrow·seal 배치를 구체화했지만, A의 RowReserve가 보유할 원 예약 권한·private loan과 MutationPort 준비/적용의 수명을 연결하는 consuming reservation API는 미결로 보고했다. 이 항목과 소비자 명세의 나머지 질문을 닫기 전 M2를 완료하지 않는다. 이번 저장은 조사 및 설계 초안의 중간 기록이며 제품 구현·시험·M3–M5 실행 증거가 아니다.

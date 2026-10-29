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
| 조사 당시 `plan4_retention.rs::Plan4Retention` · [현재 정의](../../crates/engine/src/storage/wal_io/plan4_apply.rs) | Plan4Pages·control bank·RuntimeHistory/replay·flush control·visibility·recovery credit. `split`이 동일 상태를 writer/reader/flusher에 대여. 원 조사 경로·revision은 유지하며 후속 이전은 [C1 기록](#c1-owner-ports)을 따름 |

현재 물리 파일 배치와 Rust 모듈의 포함 관계는 다르다. `#[path]`로 연결된 실제 의미 경로는 `storage::wal_io → bootstrap → doublewrite_install → mutable_storage → plan4_pages → retention`이며 `create_pair::{catalog_live,catalog_commit}`, `read`, `load`, `visibility`, `history`, `flush`가 마지막 아래에 있다. descendants가 ancestor private 상태에 접근하고 소비자 권한을 반대 방향으로 여러 단계 재노출한다. 소유자의 유일성은 보존할 기반이며, 이 중첩 전체를 새로운 owner로 복제하는 것은 해결책이 아니다.

### 의존 분류와 소비자 차단

| 관측 경로 | 분류 | 보존 또는 변경할 이유·M2 질문 |
| --- | --- | --- |
| WalRequirement·durable proof와 같은 fence, [원 Commit PinSet](../../crates/engine/src/storage/dirty_frame/commit.rs) | 필수 의미 | 정확 staged frame·원 core·whole target pin이 Commit을 지나 유지돼야 함. generic LSN·page bytes로 권한을 재발급하지 않음 |
| Plan4Retention::split과 source latch 아래 [CatalogSource](../../crates/engine/src/storage/wal_io/plan4_retention/visibility/source.rs) | 필수 의미 | 같은 pool/visibility 수명과 실제 참조 발급을 유지. 단순 decoded TxnRef/UndoRef는 usable pin이 아님 |
| CatalogCommitAttempt/Installed·TxPair·retirement 타입의 중간 re-export | 단순 등록 | 신규 소비자가 정상 연산과 무관한 설치 모듈을 여러 개 수정하게 함. 책임별 좁은 공개 경계와 단일 조립 지점으로 옮길 후보 |
| [reader](../../crates/engine/src/storage/wal_io/plan4_retention/read.rs)의 `storage.core.plan`, [history runtime](../../crates/engine/src/storage/wal_io/plan4_retention/history/runtime.rs)의 binding/Condvar 직접 접근 | 불필요한 내부 접근 | 필요한 것은 검증된 target·namespace·health·원 fence/notify다. 원 필드를 공개하는 대신 동일 수명의 제한 포트가 필요한지 대조 |
| 당시 `commit_bridge.rs`의 CatalogCommitAttempt/Installed import([현재 이전 위치](../../crates/engine/src/storage/wal_io/plan4_apply/terminal.rs)) | 공통 계약 미완료 | 물리 WAL이 상위 Catalog 의미를 알고 있음. 실제 semantic receipt와 whole-install/publication 조립을 거래 경계가 소유하도록 변경 필요 |
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

### `3c639c3` 뒤의 격리·자원 설계 보완

이전 goal turn은 위 초안을 실제 commit으로 보존한 진행이다. 후속 작업 시작에서 별도 worktree HEAD `3c639c31f808b2ce9504525d6993bcb62d304431`·clean을 확인했다. main은 계속 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`·clean이고 기준 이후 `crates/`·`scripts/` 제품 diff가 없었다. 다른 checkout이나 process에는 변경을 가하지 않았다.

앞 절의 Serializable 공백에 대해 [D3-ROW-ISOLATION-001](../contracts/dev3-task-10-concurrency.md#module-row-isolation)을 제안했다. 실제 source에는 별도 CommitCoordinator 객체가 없고 기존 writer의 유일 mutable ledger borrow가 Commit을 직렬화함을 확인했다. `CommitInstalling::finish`의 Installation Open은 Q 공개보다 앞서므로 이를 직렬화 완료로 오인하지 않는다. 새 MutationPort가 validation부터 실제 epoch/Q publication까지 원 mutable ledger를 보유하고 readonly Serializable 종료도 같은 경계에서 검증한다.

원 control의 mutable ledger와 짧은 isolation 장부를 분리하여 ReadPort가 자동 read-set을 등록하게 한다. Serializable capsule은 원 transaction SnapshotGuard를 보유하고 RC는 statement마다 새 guard를 사용한다. 실패 statement 관측 집합·CheckedTable 재사용 검산을 유지한다. 실제 Row 변경 table만 epoch를 갱신하고 CREATE만 catalog namespace를 갱신하여 독립 table writer의 성공 가능성을 보존한다. 실제 whole-install receipt·선예약한 변경 목록/slot을 Q 공개에 결합하며 durable 뒤 새 할당을 하지 않는다.

`/root/module_handoff_review`가 원 source·계약과 읽기 대조하여 epoch1600+isolation4160=5760B, 32table entry의 first-extent 상계, Missing-only 원 RootJob/비재사용 RowId 배타권의 별도 lock slot0 표현을 한정 수용했다. 이어 실제 작성된 격리·statement·자원 owner 세 절을 검토해 필수1건을 제시했다. 원 source의 `active.max(...)`만으로 writer와 여러 resident reader의 짧은 실행이 동시에 사는 합을 계산할 수 없다는 지적이었다.

`/root`는 [자원표](../contracts/dev3-resource-bounds.md#row-missing-module-budget)의65536B를 공통 retained1회+모든 활성 operation/caller carrier의 동시 합으로 명시하고, 원 admission의 선예약·출구 반환, 부족한 credit을 상위 guard/pin과 함께 기다리지 않음, 예약 rollback/terminal의 progress 및 별도 flusher65536B를 연결했다. 장기 capsule/view/cursor49152B는 이 합과 별도다. 같은 별도 검토자가 이 수정본을 다시 읽어 **해당 owner 세 절의 미해결 필수0**을 확인했다.

이 검토는 전체 M2의 API/fixture/인수 배정 수용이나 실제 type/caller census·runtime F05가 아니다. 원 지적과 `3c639c3`의 미채택 상태를 보존하며, 공통 경계·두 소비자 명세의 후속 완성본까지 별도 D2로 검토한 뒤 M2 종료를 판단한다. 코드·fixture·빌드·시험·문서 verifier는 아직 실행하지 않았다.

### 후속 전체 검토의 중간 기록과 커밋 범위

사용자의 `Commit` 요청에 따라 공통 경계·소비자·격리/statement/자원 owner·계획 연결의 문서7개를 **M2 PROPOSED 중간안**으로 저장한다. 이 저장은 전체 D2 수용이나 M2 종료가 아니다. `/root/module_handoff_review`의 후속 읽기 검토와 경계 담당의 source 대조에서 다음 사항을 확인했다.

- 기존 Catalog CREATE는 actual actor 아래 원 `SharedPool::prepare_tx_pair`의 exclusive mutable claim과 flush를 사용한다. 장기 Row transaction capsule이 같은 session의 field를 빌리는 동안 기존 `&mut DiskSession` CREATE를 호출할 수 없으므로, 이를 단순한 transaction 전 provisioning으로 설명할 수 없다. readonly Serializable과 CREATE의 충돌 사례를 유지하면서 같은 pool의 실제 claim/Drain/flush 권한을 공급하는 API는 **필수 미해결**이다.
- sibling A가 private validation/producer lane을 읽고 untrusted 후보를 돌려줄 데이터 접근 경로가 필요하다. lane parts와 순수 후보 constructor를 명시하는 보완을 시작했으며, 전체 context getter와 Rust 수명의 완전성은 후속 검토 대상으로 남긴다. authority factory를 공개하는 대안은 채택하지 않는다.
- isolation header의 `[32,64)` zero와 boundary의 추가 counter 정의가 충돌했다. 짧은 operation 사용량은 원 `@24` counter, 공통 retained/progress 비용은 실제 source census의 고정 상수와 원 admission으로 연결하는 방향으로 맞춘다. 이는 source census·실행 검증의 완료를 뜻하지 않는다.
- native N03/N05/N09의 RAM stage 완료만으로 재시작 prefix에 Reserve/Forward/CLR이 보존된다고 볼 수 없다. RAM-only와 실제 write/sync로 보존한 prefix를 구분하고, N05의 durable 사례는 whole RAM install·guard 해제 이후 원 flush를 통한 partial home 설치로 구체화한다. RAM-only CLR 유실 후 재생성과 durable CLR의 중복 undo를 구별한다. RowId의 재시작 비재사용도 실제 selected issued prefix를 기준으로 명확히 한다. 기존 fault 사례를 삭제하거나 제품에 매 Reserve 강제 sync를 추가하지 않는다.
- C+B의 committed-only fixture는 A handler 없이 loser를 처리했다고 할 수 없다. 현재 recovery action은 공통 CancelReserved/AbortEnd를 진행하므로, 소비자 문서의 Active/Aborting/Reserved 발견 시 READY 금지와 일치시키려면 최초 actual census에서 no-loser를 확인하는 제한 경계가 필요하다. 이 정확 API는 **필수 미해결**이다.

수정 중인 초안에 대한 지적과 후속 수정은 별도이며, 이번 기록으로 미해결 항목 또는 수정 후 미검토 항목을 수용 처리하지 않는다. M3–M5·제품·fixture·compiler·native 시험은 NOT_RUN이고 전체 실행 목표는 유지한다. 이번 문서 커밋에는 제품 코드 변경이 없어 테스트를 선택하지 않으며 문서 verifier도 push 전까지 실행하지 않는다.

작성자 두 명의 편집 중지 뒤 검토 입력 SHA-256은 경계 문서 `62d45d93204290090b0df571c91d3e45bddbb6df0128a7009e291a5d3a2c333b`, 소비자 문서 `57777b68c7acbdad9d28c02473f1e7711038ca7ef963c8a6d754c61cec9b2e75`, 격리 owner 문서 `e1a2e8b9f30b507b6a4f064c947c63c8467b82abd46364e8197b330a663e823e`다. native MCA03-N은 기존11개를 보존하고 RAM-only/durable 변형5개를 추가한16개이며 모두 NOT_RUN이다.

별도 검토자는 위 세 SHA-256과 실제 파일의 일치를 확인하고 중간 D2를 마감했다. Catalog claim·A context getter 전체 대조·C+B 최초 no-loser gate의 필수3항은 미해결로 유지한다. isolation header 충돌은 문서상 해소됐고 RAM-only/durable prefix·N05 partial home·N09 CLR·16실행 분모·RowId 발급 범위도 수정 문서끼리 일치함을 한정 확인했다. 전체 M2 수용 또는 제품·fixture·runtime 검증 결과는 아니다. 추가로 MCA01-V12의 `index1 existing table`은 현재 table-only CREATE만으로 만들 수 없어, 지원 밖 입력을 actual owner로 받는 제한 fixture인지 순수 preflight 부정 사례인지 공급자·분모와 함께 후속 확인해야 한다. 이 확인점을 index 구현의 신규 선행 조건이나 실행된 거절 증거로 바꾸지 않는다.

### `ae6fb6d` 이후 필수 항목의 해소 작업

직전 turn은 문서7개의 실제 commit `ae6fb6d`로 authoritative state를 변경한 **progress**다. 재개 시 별도 worktree는 그 HEAD·clean, main은 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`·clean임을 다시 확인했다. 경계 담당은 Catalog claim, 소비자 담당은 A context/encoder 입력 및 index1 부정 fixture, `/root`는 최초 복구 census, 별도 검토자는 M3 반영 대상과 후속 D2를 맡는다. 제품·시험·빌드는 시작하지 않았다.

`/root`는 [`recover_physical_and_open`](../../crates/engine/src/storage/wal_io/plan4_recovery.rs)과 [`preinspect`](../../crates/engine/src/storage/wal_io/plan4_recovery/analysis.rs)를 대조했다. 현재 preinspect는 selected base와 complete WAL prefix를 원 replay bank/Jobs로 재구성·검산한 뒤 AnalysisSeal을 만들며, physical 예약·source 안정화·REDO·REBASE·LOG_OPEN은 그 다음이다. 따라서 C+B의 no-loser 선택은 이 최초 canonical 상태에서 Reserved/Active/Aborting을 검사하고 첫 복구 쓰기 전에 거절하는 제한 입구로 공급할 수 있다. base에 남은 과거 loser와 최종 complete prefix의 살아 있는 loser를 혼동하지 않는다. 허용/거절 선택 enum은 권한 증거가 아니고 실제 분석 seal과 원 source identity가 인계를 보호해야 한다. 정확한 반환·seal·case는 경계 문서에 반영해 별도 검토하며, 이 조사만으로 항목을 수용 처리하지 않는다.

독립 검토자는 이 최초 gate의 위치·기존 failure 반환을 source와 대조해 일치함을 확인했지만 **SystemPage Reserved 누락1건**을 지적했다. 원 `ReservedInventory::SystemPage`는 ATT0·RootReserved0이어도 존재할 수 있다. 반영안은 최종 allocator의 모든 Reserved inventory도 None이어야 한다고 명시하고, 소비자의 기존 Active Row native 부정을 보존하면서 RowReserved·Active/headNone·Aborting/nextNone·SystemPageReserved·CatalogReserved를 더한6개 거절 사례를 배정했다. 거절 전용2PID와 정상3PID/재장애4PID는 구별한다. `effect=CallerMustResolve`는 원 lease/fence 소유이고 `database_effect=Unchanged`는 첫 복구 쓰기 이전이라는 뜻이다.

생산자 입력은 실제 row encoder의 `&[Column]/&[Value]`와 actual Catalog의 `TableDefinitionView` 차이를 확인하여, 효과 없는 borrowed schema/value adapter와 전체 scalar getter·descriptor/64 Jobs의 read-only observation을 C0에 배정했다. A는 Forward/CLR 후보와 제품 흐름을 소유하고 C는 실제 source/권한/효과를 검산한다. Missing Undo에서는 현재 `validate_record`가 column을 사용하지 않는 범위를 확인했다.

index1 부정 입력은 현재 table-only CREATE가 합법 indexed table을 제공하지 않는다는 한계를 보존한다. V12-P는 독립 valid Index 정의를 pure profile census가 Unsupported로 거절하는 사례이고, V12-S는 실제 C table/flush 뒤 FileIo가 공급한 well-formed altered CatalogLeaf를 원 loader가 canonical replay bytes와 두 차례 대조하여 Fatal(Load,Invariant)·fence로 거절하는 사례다. 후자를 profile Unsupported나 정상 Before 반환으로 꾸미지 않는다. actual indexed table 지원/DDL 성공 증거는0이고, 영속 mutation·새 load/Row grant 없이 거절해야 한다. 기존 V12를 삭제하지 않고 변형을 추가하여 MCA01의 memory 분모는13이다.

`/root`는 이 M2 해소와 병행하여 [M3 반영안과14행 대응표](../plans/module-independence-plan.md#m3-adoption)를 작성했다. 병렬 정책/JSON·AI 절차/작업 예시, 실제 세션/WAL/Commit/CREATE/복구와 해당 task owner, 시험 계획/모듈 목록에 목표 계약을 **PROPOSED**로 반영했다. 기존 규정으로 충분한 코딩/빌드/단계·DEV4 문서는 미수정 이유를 명시하고 신규 source 경로를 선등록하지 않았다. 문서만 작성했으며 제품·fixture·빌드·시험·verifier는 NOT_RUN이다. 최종 M2/M3 독립 D2 수용은 아직 기록하지 않는다.

<a id="final-design-review"></a>
### 보완안의 최종 독립 검토와 커밋

`/root/module_handoff_review`는 작성자 편집 중지 뒤 경계 SHA-256 `b90ee0e11ad189a0deaa14d3bdfd3b983a75dee34257e302e5adea75358fa692`, 소비자 SHA-256 `96b286086c414e365fe53f11aaf218a228fdf4061baa577a79f4d3d1ff7b53d3`, 기준 `ae6fb6d` 대비24문서 binary diff SHA-256 `8b69a4d61308f1cf40adf8143a85aace18cae8ac214ce89f72f0a9d67747241d`의 실제 일치를 확인했다. 이 identity는 아래 누락 수정과 검토 상태 기록 이전의 검토 입력이며 최종 커밋의 hash가 아니다.

M2는 원 claim/flush lease·producer 입력/borrow·SystemPage를 포함한 최초 canonical no-loser gate의 필수3항 해소를 확인하여 **D2 검토 수용·필수 수정0**으로 판정했다. V12-P/S와 복구 gate6종의 source 경계도 확인했다. M3는 `module-test-matrix.json`의 case_groups에서 MBC31 누락1건을 지적했다. `/root`가 MBC30 다음에 MBC31을 추가했고, 같은 검토자가 해당 행을 한정 재확인하여 **M3 D2 검토 수용·필수 수정0**으로 마감했다. 이전 미해결 판정과 source별 기록은 보존한다.

사용자의 `Commit` 요청에 따라 이 보완안과 검토 결과를 저장한다. 소유 계약·JSON의 채택 상태 PROPOSED와 M4/M5 NOT_RUN을 보존하며 제품 구현에 착수하지 않는다. 문서 밖 코드 diff는0이고, 테스트·빌드·문서 verifier는 실행하지 않았다. 독립 설계 검토는 compiler/runtime 자원 검산·fixture·현재-build 복구·MI01–06의 실행 증거가 아니다.

<a id="c0-borrowed-codec"></a>
### M2·M3 채택 이후 C0 borrowed codec 부분공급

직전 goal turn은 실제 커밋 `66ddf61`로 보완 문서24개를 저장한 **progress**다. 이번 재개 때 own worktree의 HEAD `66ddf61`·clean과 main `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`·clean을 확인했다. 이미 완료한 최종 M2·M3 독립 D2를 수신하여 채택 상태를 반영하고 M4 C0 구현을 시작했다. 앞 커밋 당시 PROPOSED 기록은 소급 수정하지 않는다.

실제 제품 변경은 `crates/engine/src/storage/row_record.rs`와 같은 경로의 `row_record_tests.rs`다. `RowSchemaInput`의 caller columns/validated Catalog view 및 `ValueRef`를 공통 pure codec으로 연결하고 기존 owned API도 같은 경로를 사용하게 했다. 이름·값·column의 heap 복사, 저장·pin·snapshot·READY 권한 발급은 없다. canonical/record bytes와 기존 오류 우선순위·출력/tail 보존을 유지한다. 반환 row view는 input만 빌리며 schema buffer는 먼저 해제할 수 있다.

실행 전에 diff가 이 두 Rust 파일임을 확인하고 새 borrowed 경로4시험과 내부 처리가 바뀐 기존 canonical2시험을 선택했다. 실제 명령은 다음과 같고, 각 사례는1회 실행했다. 첫 명령은 전체 경로의 `tests::borrowed_` prefix이므로 이름 중간에 borrowed가 있는 기존 시험은 포함하지 않는다.

```bash
CARGO_TARGET_DIR=/tmp/xadb-module-c0-target cargo test -p xadb-engine --lib --locked storage::row_record::tests::borrowed_ -- --nocapture
CARGO_TARGET_DIR=/tmp/xadb-module-c0-target cargo test -p xadb-engine --lib --locked storage::row_record::tests::canonical_row_literals_and_borrowed_values -- --exact
CARGO_TARGET_DIR=/tmp/xadb-module-c0-target cargo test -p xadb-engine --lib --locked storage::row_record::tests::canonical_schema_and_size_boundaries -- --exact
```

| 선택 | 결과·관측 범위 |
| --- | --- |
| 신규 borrowed4 | 4 PASS,1282 filtered. 독립 canonical/row literal, Catalog와 caller schema, type/null/count/크기/오류 우선순위, short output·tail 보존, schema 수명 분리 |
| 변경 영향 기존 canonical2 | 각1 PASS,1285 filtered. 기존 literal과 type/null/크기 경계가 같은 공통 구현을 소비 |
| 원 제품·native·전체 workspace | 실행하지 않음. 이 pure codec 결과로 실제 INSERT·조회·복구·MBC00 compiler 또는 MCA00/MCB00 전체를 PASS로 표시하지 않음 |

검증한 source SHA-256은 row_record.rs `492a9b91de6f6baa8195dfb6e36b30fafa7c94fd5902a9b32b2705d688b9088b`, row_record_tests.rs `664edf219028fd2da4555f6e538110f819bf1e7a3556caf705263080e6b77652`다. ELF `/tmp/xadb-module-c0-target/debug/deps/engine-1e4c5f7676c3b262`의 SHA-256은 `0c36bedaf537b4c52e03ef50391049586d62c7052b817a0aabc38dda00322927`이다. 이 binary 경로는 이번 로컬 실행 산출물이며 영구 배포 위치가 아니다.

`/root/module_handoff_review`는 두 source hash를 직접 대조하고 기존 wire·크기·오류/출력 보존, no-heap/source authority 경계, 반환 수명 및 독립 기대값을 읽기 검토하여 **부분공급 D1/D2 수용·필수 수정0**으로 판정했다. 검토자는 실행을 반복하지 않고 총괄의 선택6 PASS 결과를 수신했다. root가 반영을 확정한다. 두 파일만 rustfmt로 정리했고 문서 verifier·전체 fmt/Clippy·push는 실행하지 않았다.

이 결과는 C0 중 borrowed codec만의 공급이다. 전체 C0의 원 owner/port 및 producer/read 관측 경계, C1–C5 actual 공급, M5의 두 제품 본문 중첩·첫 C+A/C+B·실제 합류·MI01–06은 미완료다. 계획의 전체 목표는 이 잔여 작업까지 유지한다.


<a id="c0-producer-types"></a>
### C0 생산자 관측·대여 타입 부분공급

직전 goal turn은 `edc7379`의 실제 borrowed codec 구현·선택6 검증·독립 검토·커밋을 마친 **progress**다. 이번 시작 시 own worktree의 HEAD `edc7379`와 main `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`를 확인했다. main은 clean이며 변경하지 않았다. 이 후속 단위는 새 생산자 타입과 공개 경로에 한정한다.

`storage/wal_io/plan4_apply.rs`에 private 관측 context·getter·source role9·변경 page role5·원 ledger borrow·consuming lane·untrusted 후보를 구현했다. `wal_io/disk_api.rs → storage/disk.rs`가 이 타입만 선택 재노출하고 실제 조상 모듈 이름은 wal_io 내부에만 공개했다. `RowOperation::{Forward,Clr}`와 `RowPageRole`은 wire tag가 아닌 A의 순수 관측 분류다. private cell 좌표는 세 값이 함께 존재하는 tuple로 표현했다. 실제 context/lane 생성·source 포착·예약/설치 권한·제품 본문은 추가하지 않았다.

실행 전에 이번 제품9파일 diff와 새 compiler5 입력의 관계를 대조하고 manifest의 입력 hash·예상 code/line을 동결했다. 정상 consuming split·전체 getter 사용1개와 private field/소비 후 재사용/수명 연장/내부 경로 거절4개다. 각 입력은 고정 export의 storage sibling probe에 연결했으며 원 작업 트리에는 probe를 삽입하지 않았다. 명령은 `cargo check -p xadb-engine --lib --locked --offline --message-format=json`이다. 첫 정상 입력이 변경 제품의 컴파일을 함께 확인하므로 별도 무변경 baseline 실행은 없다. 기존 codec6·제품 runtime·native·전체 workspace 시험은 반복하지 않았다.

| 입력 | 실행·판정 |
| --- | --- |
| producer_parts | 1회, 정상 컴파일 PASS |
| private_context | 1회, E0616·line4로 거절, 자동 PASS |
| consumed_lane | 1회, E0382·line5로 거절, 자동 PASS |
| loan_escape | 1회, E0521·line5로 거절. 자동 판정기 FAIL 보존; 같은 위치의 두 primary span을 독립 수동 검토하여 수명 연장 거절 의미를 수용 |
| private_raw_path | 초기 중단 때 NOT_RUN, 이후 이 사례만1회 실행. E0603·line2로 거절, 자동 PASS |

첫 실행 예산은 총180초·사례당30초·출력8MiB였고19.49초·56,430B에서 판정기 오류로 중단했다. artifact 부모 디렉터리가 없던 최초 준비 명령은 실행 전에 실패했으며 compiler를 시작하지 않았다. 부모 생성 뒤 위 source를 포착했다. 미실행 마지막 사례에는 별도의30초 한도를 적용했다. timeout·미완료를 PASS로 표시하지 않았다.

원 `check_dev3_buffer_borrows.py::check_diagnostics`는 primary span이 정확히1개여야 한다. loan_escape의 Rust 진단은 E0521 한 건에 동일 file/byte170..198/line5/column5..33인 primary span을 label만 달리해2개 보고했다. 제품의 수명 제약은 기대대로 동작했으나 이 판정기의 형태 제약에서 실패했다. 공통 checker·fixture·동결 manifest를 수정하거나 같은 컴파일을 반복하지 않았다. 자동 campaign의 **FAIL을 유지**하고 별도 수동 확인을 기록한다. 이 결과를 compiler5 전체 자동 PASS로 요약하지 않는다.

로컬 산출물은 `artifacts/module-c0-producer-20261026-r1/`에 원 source·각 probe source·JSON/stderr·runner를 보존한다. `run.py`는 첫4개까지, `remaining.py`는 미실행 raw-path만 수행한 정확한 조립 절차다. 공통 source SHA-256은 `bd971fb2d665d80139069f3609bca189c74927cbb1e4d13bfc2c4a627b2f3231`, 동결 manifest는 `81198fcba1cd3135d0d171a8e7bf08cb93cc2f8e73b8fb6420099b61ad2f2901`이다. 각 overlay와 compiler log hash는 원 결과에 포함한다.

| 보존 대상 | SHA-256 |
| --- | --- |
| plan4_apply.rs | `a00c49f4d32331d6212f77d9ef330cb0e92f3f7577ba71009a91678a37d8a278` |
| disk_api.rs | `6e3408a570458dc0153d8ecadee9dc6ff0d598b3b2648a41b2914448cfd92530` |
| disk.rs | `45581c31891ca7657988e94a4f4abd83860e47d63726fac32d8bdd3fede35842` |
| run.py | `384ff52f5881fa41b50ed2b42c0131318d39614154e9d32714ab9dddc365669a` |
| remaining.py | `e771daf1b7bd46dcc869d063bda53bc92e5aace59fc7237121b4e9d20733226f` |
| result.json (FAIL 보존) | `b0f87db45f0607869cadbbf119678b4d7f9bd44854942f0f26fd2e6742267265` |
| remaining-result.json | `6e8620a1ec77f0e8cbd229b47c6d2720d07e5c25d4cb5f58326b07d51964ebce` |
| loan_escape/compiler.jsonl | `1ddacd6593b0da49fc81eff3925b595d84ac1d747aabafac8ebd3ca8ffde912a` |

`/root/module_handoff_review`는 실제 product9·fixture6파일의 r1 baseline 동일성과 contract/diff/원 compiler log를 직접 대조하여 **C0 생산자 타입 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. 자동4 PASS와 E0521 수명 거절의 수동 수용을 구분했고 원 자동 campaign FAIL을 유지한다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았으며 `/root`가 반영을 확정한다. source 포착/authority, read 경계·전체 C0/MBC00, C1–C5 및 M5 두 제품의 겹치는 구현·첫 단독 source·실제 합류는 미완료다. 이 타입 검증으로 runtime 자원 한도나 실제 I/O/복구를 인수하지 않는다. 새3제품 파일과 정상 compiler fixture만 rustfmt로 정리했고 문서 verifier·전체 fmt/Clippy·push는 실행하지 않았다.


<a id="c1-physical-wal"></a>
### C1 원 물리 WAL 구현 분리

직전 goal turn은 C0 생산자 타입·선택 compiler·독립 검토를 `8492073`으로 저장한 **progress**다. 이번 시작 시 own HEAD `8492073`·clean, main `b7ce51d`·clean을 확인했다. main과 기존 증거를 변경하지 않고 M4의 다음 실제 구조 변경을 수행했다.

원 `wal_io.rs::WalRuntime` impl의14개 method를 `wal_io/physical.rs`에 옮겼다. stage/first-write 관측·exact requirement 검산·공동 drainer·wait·write/sync의 실제 본문이며 wrapper가 아니다. 원 runtime/AppendState/File/slot/fence/Condvar/IoFence·생성/Drop과 semantic `commit_bridge`는 유지한다. 새 module은 명시한 물리 타입만 수입하고 Catalog/Row operation·attempt·installed/receipt를 수입하지 않는다. 호출 method12개는 `pub(super)`로 wal_io 내부에 한정하고 `requirement_ready`·`sync_claim`은 private다. 기존 외부 소비자가 runtime을 얻는 통로를 추가하지 않았다.

선택 전에 실제 diff가 경로·import·method visibility와 서명 줄바꿈뿐임을 대조했다. 별도 검토자는14개 method의 중괄호 안 본문이 원 byte와 같음을 확인했다. 전체 impl의 가시성 정규화와 rustfmt 후 동일 SHA-256은 `c4ff7b53d0fa2d083e5daef35fd5320316eb25e642d643b94fe94df994990046`이다. 최초 단순 문자열 대조에서는 두 서명의 자동 줄바꿈 차이만 관측했으며, 본문 차이가 아니므로 가시성 제거 뒤 동일 rustfmt 결과로 대조했다.

변경된 module/visibility와 cfg(test) 지원 경로만 확인하기 위해 정상1입력의 production/test 빌드와 private module import 거절1입력을 실행 전에 동결했다. 각 실행은 고정 source export에 probe를 연결하며 원 작업 트리에 probe를 넣지 않는다.

| 선택·명령 | 결과 |
| --- | --- |
| production · `cargo check -p xadb-engine --lib --locked --offline --message-format=json` |1회 PASS·warning0. 원 wal_io의 sibling에서 물리 method 호출이 컴파일됨 |
| test_build · `cargo test -p xadb-engine --lib --no-run --locked --offline --message-format=json` |1회 빌드 PASS·warning0. 기존 test cfg의 stage callback·durability support import가 연결됨. **테스트 사례 실행0**, 기존 runtime PASS를 새로 부여하지 않음 |
| private_physical · 위 library cargo check |1회 E0603·line2, 정확한 storage sibling import에서 거절되어 PASS |

총180초·사례30초·출력8MiB를 동결했고33.45초·47,208B로 끝났다. Rust1.95.0이며 제품 runtime/native·기존 C0 compiler·codec 시험은 실행하지 않았다. 전체 C1 memory/native·MBC00/MBC10 인수로 승격하지 않는다.

원 source·각 overlay·JSON/stderr·실행 조립과 결과는 로컬 `artifacts/module-c1-physical-20261026-r1/`에 보존한다. 공통 source SHA-256은 `6020bb599e157cff27266d449db8dd6118c1762a6b0403107d7ac2a8edf60ff6`, 동결 manifest는 `e52948aab3bdc7ab1bc7bcf89f3dcc3c6b1dc653243cac4fb9be6c383077c754`다. 실제 결과·각 overlay/log hash는 result.json에 둔다.

| 보존 대상 | SHA-256 |
| --- | --- |
| wal_io.rs | `9b2257de47d5ac7d0fff7c58e01e9839c93a8be967b220b496d562e032ab4e74` |
| wal_io/physical.rs | `19319c5c5fa9a624e02a5418c48878a2853f790b0685f96f31844a952ce6d79a` |
| run.py | `661b857ec529988432f9699c68cc29183c43186baf7f3514a75c8f8b7580446e` |
| result.json | `10a6c144b574617002414312e6e6815b6661de94b4b9afcfdad4ded8e5aadbd2` |
| body-comparison.json | `3654e96b89f12281b4ef3c09e2cfb86c560635198f6dcd109f3b9066e53293f9` |

`/root/module_handoff_review`가 contract/diff와 원 runner·overlay·stdout/stderr를 직접 대조하여 **C1 physical 이전 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. 검토 당시 작업 source도 위 compiler baseline과 같았다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았고 `/root`가 반영을 확정한다. 이 변경은 C1의 실제 물리 구현 분리에 한정한다. DiskSession/control/isolation/port 이전·selected recovery 인계, semantic terminal의 apply 이전, 전체 C0·C1–C5 공급 및 M5 제품 병렬/단독/합류 인수는 미완료다. 변경2개 제품 파일과 정상 compiler 입력만 rustfmt로 정리했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.

<a id="c1-owner-ports"></a>
### C1 원 DiskSession·세 port 정의와 consuming 전환

직전 goal turn은 물리 WAL 본문 이전·컴파일3건·독립 검토를 `177b5dd18b98ad6800ab8bf6ebc0857772b5f601`로 저장한 **progress**다. 이번 시작 시 own HEAD `177b5dd`·clean, main `b7ce51d`·clean을 확인했다. 변경과 검증은 같은 격리 worktree에서 진행했다.

원 `plan4_retention.rs`의 실제 owner·생성·split·자손 연결을 `plan4_apply.rs`로 옮기고 원 파일을 제거했다. 실제 정의는 `DiskSession`이며 `Plan4Retention`은 새 정의의 역방향 alias다. 기존 read/live/flush 자손에 있던 세 port의 정의를 apply root의 private 필드로 옮겼다. `Plan4Pages::into_disk_session`이 원 `Plan4Pages` 값 전체와 control/history/flush/visibility를 실제 이동하고 옛 `into_empty_retention`은 이 전환을 소비 호출하는 shim이다. 새로운 pool/WAL/File/slot/fence/loan이나 raw-state getter를 만들지 않았다.

selected recovery는 원 위치에서 `into_recovered_disk_session`과 canonical `DiskSession` 반환을 사용한다. `Plan4Recovered` 타입만 `storage::disk`에 재노출하며 READY gate·원 Self 반환·binding seal/생성 권한은 유지한다. 이 경로의 컴파일을 확인했으나 실제 복구 handoff runtime 인수를 수행하지 않았다. `lookup_load`는 이전 후 private가 된 자손 import 대신 root `ReadPort`에 impl을 연결했다. 기존 C0 본문은 byte 동일하며 정상 전환/split은 canonical 식별자 치환 뒤 본문 동일이다.

production의 `apply`와 test의 `retention`은 같은 파일을 읽는 상호 배타적 선언이다. test의 `retention as apply`가 기존 동결 selector를 보존한다. 상대 visibility는 실제 조상을 가리키며 alias 경로를 `pub(in ...)`의 조상으로 사용하지 않는다. [소유 계약 §4.1](../contracts/dev3-module-boundaries.md#target-imports)에 이 제한을 반영했다. 제품 효과·권한을 시험 cfg에 별도로 구현하지 않았다. 미전환 helper의 전체 root 목록과 물리 경로는 [배치 정책](../policies/repository-build-plan.md) 및 JSON `dev3_module_c1_owner`에 명시했다.

이번에는 원 control196608B·replay1572864B backing을 유지했다. control/ledger/isolation 분할, 별도 isolation4160B·epoch1600B, long49152B·동시 short65536B 회계와 source admission은 후속 C1 의무다. 현재 세 port 정의를 §6.1의 최종 Row/isolation API 전체 공급으로 표시하지 않는다.

#### 변경별 선택과 원 실패 보존

새 owner/port의 정상 소비·private 필드·borrow 수명만 compiler 입력으로 동결했다. 실제 자원 이동과 Before 반환을 검증하는 신규 메모리 시험2개도 실행 전에 manifest에 고정했다. 기존 시험·전체 wrapper는 선택하지 않았다.

| 보존 source·선택 | 결과와 수정 |
| --- | --- |
| `artifacts/module-c1-owner-20261026-r1/` production | FAIL. 정상 fixture의 미노출 `wal_io::Plan4Recovered` import E0432와 실제 `lookup_load`의 private 자손 import E0603. 기존 READY 타입의 선택 재노출과 root `ReadPort` impl로 수정. 후속 사례는 NOT_RUN |
| `artifacts/module-c1-owner-20261026-r2/` production | FAIL. 정상 fixture에 `disk::Plan4Recovered` import가 빠진 미해결 타입. fixture만 수정했으며 후속 사례는 NOT_RUN |
| r3 production | `cargo check -p xadb-engine --lib --locked --offline --message-format=json` PASS. sibling에서 canonical 생성·split·READY 반환 타입 확인 |
| r3 test_build | `cargo test -p xadb-engine --lib --no-run --locked --offline --message-format=json` 빌드 PASS. 이 명령 자체의 테스트 사례 실행0 |
| r3 private_field | library check가 정확 line5 col31..35의 E0616 하나로 거절하여 PASS |
| r3 session_borrow | library check가 정확 line6 col10..17의 E0505 하나로 거절하여 PASS |
| r3 신규 owner 메모리2건 | 위 test_build의 같은 ELF에서 아래 exact selector를 각각1회 실행하여 모두 PASS |

각 compiler overlay는 원 source에 manifest의 선언과 probe 하나만 추가한다. r1/r2 원 FAIL·입력·source·출력은 보존하며 진단 기대값을 완화하지 않았다. r3 컴파일 예산은 총180초/사례30초/출력8MiB, 실제38.06초/71,974B다. 메모리 시험 예산은 총90초/사례30초/출력8MiB, 실제1.01초다. `RUST_MIN_STACK=33554432`는 시험 thread stack 설정이며 제품 자원 한도 인수가 아니다.

두 exact selector의 공통 prefix는 `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::retention::owner_tests::`다. 실행은 보존 ELF에 각 selector와 `--exact --nocapture`를 전달했다.

- `disk_session_moves_original_resources_and_splits_disjoint_ports`: 실제 fresh storage에서 원 storage/core/WAL/fence/slot/control/replay/operation 주소, WAL prefix, 성공 전환의 flush loan take, 세 port의 공유·분리 대여와 종료 후 handle 해제를 확인했다.
- `disk_session_before_returns_original_owner_for_canonical_retry`: foreign owner의 Before/Unchanged가 원 Plan4Pages·loan을 돌려주는지 확인하고 그 반환값으로 정상 consuming 전환을 수행했다.

원 source·각 overlay·manifest·runner·compiler JSON/stderr·시험 stdout/stderr와 결과는 `artifacts/module-c1-owner-20261026-r3/`에 보존한다. 현재 source와 r3 baseline은 동일하며 결과별 identity는 다음과 같다.

| 보존 대상 | SHA-256 |
| --- | --- |
| 공통 source | `8f583dc22f41858e07fa2a3a4edea476dfc66c8e6f4d0b87d4c20f314f7e1ff0` |
| manifest.json | `ee38d646b7107b8c7ee74a04361c424764d5931a51a41f5fac9cbacdcc46a01f` |
| test overlay | `c076f673d74baefe358309b64e706fee9206cab387ea54dcc6ee08f991499a4d` |
| test ELF | `541023955699a67bc071a7146cdf8c2eda10eb83287a20883d48faf73bb2dd60` |
| result.json | `2d3f2971389fe4cb4247198c86a769a4b9615789223304d1e6ba9973b2f26ebc` |
| runtime-result.json | `60760901af482a1e00af13bb8a6c79fb5121c17f8ce457927033c4c5341798e8` |

`/root/module_handoff_review`는 실제 diff·원 로그·overlay·source/ELF/log hash를 대조하여 **C1 owner/port 이전 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. 검토자는 읽기만 수행했으며 `/root`가 이 범위의 반영을 확정한다. 신규2건 외의 기존 runtime/native·codec·C0/physical compiler는 반복하지 않았다. 전체 fmt/Clippy·문서 verifier·push도 실행하지 않았다. 복구 handoff runtime, control/isolation·source admission, 전체 C0/C1–C5·M5 및 DEV3/S5는 미완료다.

<a id="c1-isolation-backing"></a>
### C1 원 control의 영속 원장·isolation backing 분리

직전 실제 goal 작업은 owner/port 이전·선택 검증·독립 검토를 `ad0c45fb55aca6916a02f713a0a4735ba008eaec`로 저장한 **progress**다. 이번 시작 시 own HEAD `ad0c45f`·clean, main `b7ce51d`·clean을 확인했다. 후속 사용자의 적절한 단위별 commit 지시를 적용하며 전체 M0–M5 목표를 유지한다.

`RetentionControl`의 기존 byte 원장·method를 `control::RetentionLedger`로 이전하고, apply root의 원 control이 이 ledger와 `Mutex<IsolationLedger>`를 직접 소유하게 했다. MutationPort는 `&mut ledger`만 독점하고 ReadPort와 같은 isolation mutex를 공유한다. 기존 Catalog·population·recovery helper의 ledger 접근을 이 구조로 연결했으며, 독립 검토에서 본문은 이름/필드 접근/서식 외 변경0임을 확인했다. 전체 control의 Deref 우회나 새 가변 원장 사본은 없다.

`EmptyRetentionControlLoan`은 별도4160B를 필수로 받는다. 새 초기화는 fresh의 원 preflight 성공 이후, recovery의 원 pool/source/epoch 검산 이후에 수행하며 새로운 실패·I/O·allocation 지점을 추가하지 않는다. header generation은1, 나머지 header/slot은0이다. 원 core/binding/fence를 borrow하며 transaction·snapshot·actor·credit을 발급하지 않는다. 아직 이 필드의 Row adapter가 없다는 한정 dead-code 사유를 선언 위치에 남겼다.

복구 maintenance의 named `ISOLATION_AT=12144576`부터12148736까지4160B를 분리했다. `SOURCE_STAMP_AT+32 ≤ ISOLATION_AT`와 전체 구간 한도를 상수 assertion으로 보장한다. 기존 Catalog private 시작12148736, visibility 시작12574720과 ATT/replay 영역은 유지한다. `RecoveryBindLoans → RecoveryBindingParts → take_recovery_binding → into_recovered_disk_session`에서 같은 loan을 한 번 이동하며 원 Fatal/Drop/fence·READY 의미는 그대로다. 실제 복구 handoff runtime은 이번 실행 범위에 포함하지 않았다.

#### 실제 diff와 선택 검증

원 생성·split의 새 backing/origin과 borrow를 검증하도록 기존 owner 시험2개를 수정했다. 새 source census 시험1개를 작성했고, 새 mutex의 수명 불변성에 맞춰 foreign core를 session 생성 전으로 옮긴 기존 Catalog 오류/cleanup fixture1개를 선택했다. 해당 fixture의 binding mismatch·fence·pin 정리 기대값은 바꾸지 않았다. 다른 완료 사례와 전체 wrapper는 실행하지 않았다.

| 보존 실행 | 결과·처리 |
| --- | --- |
| `module-c1-isolation-20261026-r1` production | FAIL. 이름 치환이 복합 이름까지 바꾼 `EmptyRetentionLedgerLoan` E0425 두 건. 원 `EmptyRetentionControlLoan` 이름과 누락된 test macro 필드를 수정. 후속 사례 NOT_RUN |
| r2 production / test_build | production PASS. test-no-run FAIL: 기존 s5 observation의 `.ledger` 누락 E0609, session보다 짧은 foreign core E0597, match 임시 Result가 살아 있는 동안 loan을 읽은 새 Before assertion E0502. 실제 수명/권한을 약화하지 않고 경로·fixture 선언 scope·assertion 시점을 수정. 후속 사례 NOT_RUN |
| r3 production | `cargo check -p xadb-engine --lib --locked --offline --message-format=json` PASS |
| r3 test_build | `cargo test -p xadb-engine --lib --no-run --locked --offline --message-format=json` 빌드 PASS. 이 명령의 테스트 사례 실행0 |
| r3 isolation_reuse | 위 library check에서 지정 line8의 E0506 하나로 caller backing 재사용을 거절하여 PASS |
| r3 runtime | 같은 test_build ELF에서 아래4개 exact selector를 각1회 실행, 모두 PASS |

compiler overlay는 원 source에 manifest 선언과 probe 하나만 추가한다. r1/r2 source·원 로그·FAIL을 보존했고 기대 진단을 완화하지 않았다. r3 compiler는 총180초/사례30초/출력8MiB 예산에서33.45초, runtime은 총90초/사례30초/출력8MiB 예산에서1.19초에 종료했다. `RUST_MIN_STACK=33554432`는 시험 thread 설정이며 제품 자원 보장으로 사용하지 않는다.

runtime selector의 공통 prefix는 `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::retention::`이며 각각 ELF에 `--exact --nocapture`로 전달했다.

- `owner_tests::disk_session_moves_original_resources_and_splits_disjoint_ports`: 원 storage/WAL/pool/control/replay/operation과 새 caller isolation 주소, 원 core/binding/fence, canonical 빈 header와 동일 mutex·독립 ledger 대여를 확인했다.
- `owner_tests::disk_session_before_returns_original_owner_for_canonical_retry`: Before에서 원 owner/loan과 isolation sentinel이 유지되고, 같은 반환 owner의 정상 전환 뒤 초기화됨을 확인했다.
- `owner_tests::disk_session_isolation_backing_and_carriers_are_accounted`: 실제 backing subtotal과 wrapper 크기를 검산했다. live metadata−peak는72256B=`68096+4160`, recovery metadata−retained_live−directory는77888B=`65536+8192+4160`다.
- `read::catalog::tests::catalog_read_original_owner_binding_and_fence_cleanup`: 변경한 foreign-core fixture의 원 오류·정리 의미를 확인했다.

메모리 Writer의 현재 source census 관측은 live peak63978/metadata136234/whole201770B, recovery peak65338/metadata104760/whole170296B다. recovery peak는65536B에서198B 여유이며 후속 carrier 추가 때 다시 계수해야 한다. 이 관측은 native/실제 recovery 실행·동시 admission 인수가 아니다. 과거 static helper의 baseline 동일성·metadata assertion을 이번 새 backing 시험의 oracle로 복사하거나 PASS로 승격하지 않았다. 기존 static inventory의 현재 source 대응 정리는 전체 C1 인수의 잔여로 유지한다.

원 source·overlay·manifest·runner·결과·stdout/stderr는 `artifacts/module-c1-isolation-20261026-r3/`에 보존한다.

| 보존 대상 | SHA-256 |
| --- | --- |
| 공통 source | `3562a8db5a85c5f91ba31e3db248ef3a055525c427df7da5914001c547448aaf` |
| manifest | `bea310904c719ad37186efffc1ea8f222409ce28b198d5701abbd83dcc6df78e` |
| test ELF | `b4f9d8cdc4273ed7efd9735a80b75e6faae426010478f52781249afff6b804f8` |
| result.json | `f4c45e5a461897c2abf38543e84372130e5112eeb6192af0c22555394218af17` |
| runtime-result.json | `983a3669ea0de15cae31100ba26ab19cd9087aeb2ca673dddc9ab214f0f5eee3` |

`/root/module_handoff_review`는 계약·실제 diff·원 compiler/runtime 로그·source/overlay/ELF/hash를 확인하고 **C1 control/isolation 분리 D1/D2 한정 수용·필수 수정0**으로 판정했다. 검토자는 읽기만 수행했고 `/root`가 이 부분공급을 반영한다. epoch1600B·거래/statement·long/short admission·native/full recovery runtime와 전체 C0/C1–C5·M5·DEV3/S5는 미완료다. 변경 Rust와 정상 fixture만 rustfmt로 정리했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.


<a id="c1-epoch-backing"></a>
### C1 원 publication의 epoch backing 소유

직전 isolation 분리 단위를 `5e63ed2735123a111e0be9c7bd48e00922a160e2`로 저장한 뒤 같은 격리 worktree에서 이어지는 **progress**다. 사용자의 적절한 규모별 commit 지시를 유지한다. 분류 D1/D2, 구현·통합 `/root`, 독립 검토 `/root/module_handoff_review`이며 전체 M4/M5 목표는 미완료다.

기존 `BoundVisibility`의 원 publication mutex가 `Publication { q, epochs }`를 소유하도록 바꿨다. `VisibilityLoan`은 기존 registry8192B와 별도 epoch1600B를 함께 받아 원 source 검산 이후에 초기화한다. 실제 Q는 같은 `InstalledFrontier`에서만 갱신하며 snapshot/horizon은 Q를 복사한 statement에서 publication guard를 해제하고 registry에 진입한다. 새 owner·Q 사본·추가 mutex·heap allocation은 없다. `SourceFrontier`는 기존 `ReadyFrontier`의 이름 정정이며 생성 경로·origin/private 필드의 권한은 그대로다.

복구는 기존 maintenance에서 `EPOCH_AT=12142976`부터12144576까지1600B를 분리하고 `VisibilityLoan`을 `RecoveryBindLoans → RecoveryBindingParts → Plan4Recovered → BoundVisibility`로 한 번 이동한다. 바로 뒤 isolation12144576..12148736·private12148736..12574720·registry12574720..12582912와 원 ATT/replay는 유지한다. 상수 assertion이 source stamp32B와 새 구간의 분리를 확인한다. live/recovery census는 backing1600B와 커진 loan/controller/facade를 포함한다. binding 동안 loan carrier가 원 history replay와 함께 살아 있는 것도 wrapper 회계에 반영했다.

#### 부분공급을 나눈 이유

독립 설계 검토에서 `Plan4Recovered::step`의 첫 visibility binding은 CancelReservedFirst/loser 정리·VERIFY·STABILIZE **이전**임을 확인했다. 그 시점에 모든 Catalog table을 Published로 seed하면 나중에 제거되는 미Commit CREATE가 남을 수 있다. 또한 현재 Catalog Commit receipt는 origin/q만 가지며 CREATE 없는 Commit도 허용되므로, 모든 Commit에서 catalog epoch를 올리는 것은 실제 논리 변화 증거가 아니다. **후속 정정:** 이 당시의 빈 Commit 지원 관측은 queue/core와 actual semantic producer를 혼동한 것이다. 실제 producer는 retained CREATE undo를 요구한다. [실패·정정 근거](#c1-epoch-activation)를 따르며 당시 기록을 실행 근거로 사용하지 않는다.

따라서 이번 epoch bytes는 **전부0인 미활성 backing**이며 의미 있는 catalog epoch·table 집합으로 읽거나 CheckedTable/Serializable 권한으로 노출하지 않는다. 다음 단위는 실제 최종 source의 기존 table/anchor 검산·seeding, actual CREATE slot 예약과 남은 변경목록을 연결해야 한다. 현재 zero 상태 유지 시험은 이 부분공급의 관측이고 최종 격리 의미 인수가 아니다. 기존 Q/snapshot/복구 동작의 유지만 아래 선택 검증으로 확인했다.

#### 동결 입력과 변경별 검증

[epoch compiler manifest](../../tests/fixtures/dev3-module-c1/epoch-compiler/manifest.json)에 코드 diff와 각 selector 관계를 동결했다. 원 source에 probe 선언/파일 하나만 더한 compiler overlay에서 production check, test `--no-run`, 살아 있는 session이 빌린 epoch 재사용의 E0506(line7)를 검증했다. **3건 PASS**, test build 자체의 실행 사례는0이다.

같은 test ELF에서 아래 exact selector만 `--exact --nocapture`로 각1회 실행했다. 원 fixture와 그 oracle를 재사용하되 이번 실제 변경 경로만 선택했으며 전체 wrapper·무변경 하위 회귀는 실행하지 않았다.

| selector의 마지막 부분 | 변경 관계·결과 |
| --- | --- |
| `owner_tests::disk_session_isolation_backing_and_carriers_are_accounted` | 변경된 visibility/controller/live/recovery census와 정확한 backing 증가 확인, PASS |
| `catalog_memory::visibility_gap::catalog_mt01_q0_publication_gap` | 원 Commit의 설치→Q 공개 사이 실제 reader snapshot, 이후 Q/H, disjoint fresh epoch pointer/초기화/Commit 뒤 미활성 유지, PASS |
| `catalog_memory::lifecycle::mc04_reserved_drop_fence_fresh_recovery` | 실제 예약 뒤 fence→원 recovery cancel/loser 정리→READY에서 동일 epoch pointer·미활성 bytes 유지, PASS |

첫 selector의 prefix는 `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::retention::`, 나머지는 같은 mutable_storage 아래 `plan4_recovery::tests::`다. MC04는 변경된 loan 전달의 선택 복구 사례이며 전체 recovery·native 인수가 아니다. 변경 외 기존 시험 결과는 각 원 source의 증거로 보존한다.

컴파일 예산 총180초/사례30초/출력8MiB 중33.58초·59,634B, runtime 총90초/사례30초/출력8MiB 중9.78초를 사용했다. `RUST_MIN_STACK=33554432`는 시험 thread 설정이다. source·ELF·manifest identity를 환경변수로 전달했고 세 runtime 사례 모두 unique PASS와 종료를 확인했다. 현재 계산은 live peak63986/metadata137842/whole203378, recovery peak65362/metadata106384/whole171920이다. live `metadata−peak=73856`, recovery `metadata−retained_live−directory=79488`, visibility backing9792B다. recovery peak 한도65536의 여유174B를 넓히거나 계수에서 빼지 않았다.

원 입력·source/overlay·컴파일 JSON/stderr·실행 stdout/stderr·결과는 `artifacts/module-c1-epoch-20261026-r1/`에 보존한다.

| 대상 | SHA-256 |
| --- | --- |
| source | `9dad65b25f7fb6918c4bef9b46ff4721e1bf276ac754f7d6734100f848fe04c5` |
| manifest | `29933d022f61f3ea1dbe8691e58e7c9ae7467b2dde24475eba371b8d2b71cd33` |
| test overlay | `3805a52bf56c1ffc1c0ea00a50fc49e5aca3eea2b7ab26601c29aa428fc3f4fa` |
| test ELF | `e5c36453eaf6549315d35a225dba0b1584dc44d812960ae2788ffd9bd89fc133` |
| result.json | `06610a26ee6367f2d4acedd74cdcc29cdaa2aa5c5d8445b5ac5d15b2d69172aa` |
| runtime-result.json | `a714b50595ecee32ae323b8469dd885fe0c4e96f4bc6fa26d512e9f617e426f0` |

`/root/module_handoff_review`는 원 diff·동결 probe/overlay·실행 로그·source/ELF/manifest/runner hash를 직접 대조하여 **C1 epoch backing 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. E0506은 정확히 line7 columns5–18이었다. 검토자는 파일 수정·빌드·시험·verifier를 실행하지 않았으며 `/root`가 위 한정 범위를 반영한다. 변경 Rust 파일만 rustfmt로 정리했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. table seeding·변경목록/epoch publication·transaction/source admission·전체 C0/C1–C5·M5는 미완료다. 기존 legacy source census 시험의 오래된 exact oracle 문제는 직전 기록대로 남으며 이번 제한 인수로 전수 PASS를 주장하지 않는다.


<a id="c1-epoch-activation"></a>
### C1 실제 Catalog epoch 활성화와 변경목록 공개

직전 backing 소유 단위 `c9fba16` 뒤의 **progress**이며 전체 M4/M5 완료가 아니다. 분류 D1/D2, 구현·통합 `/root`, 독립 검토 `/root/module_handoff_review`다. 실제 구현 범위는 기존 profile2의 고정 Catalog anchor7/leaf8·Table/RowLeaf와 drained Jobs다. 새 원 owner·mutex·heap allocation·disk wire는 추가하지 않았다.

- fresh activation은 원 replay/source 검산을 거쳐 기존 Table의 Catalog identity·committed stamp·allocator/anchor/RowLeaf를 대조하고 32entry에 seed한다. 복구 cleanup의 최초 binding은 계속 미활성이다. 최종 VERIFY·STABILIZE 뒤 현재 WAL prefix를 원 ledger durable 증거와 대조하고 replay 반환·scope 종료 성공 뒤에만 `epochs_active`를 연다. READY 전 부분 seed는 소비할 수 없다.
- 원 CREATE Reserve adapter는 실제 prepared Job의 identity로 capacity를 사전 확인하고 whole install·durable 완료 후 Reserved를 기록한다. Cancel은 실제 pair 반환 뒤, CLR은 실제 pair retire 뒤에 Reserved를 반환한다. Published slot은 반환하지 않는다. **현재 Reserved는 CheckedTable/read-mask API에 노출되지 않으므로 이 범위에서만 재사용한다.** 후속 own-created table 관측이 붙으면 살아 있는 관측과 slot 수명 계약을 함께 구현·검증해야 한다.
- Commit은 원 ledger에서 해당 actor의 남은 RootRetained CREATE를 고정하고 prepared frame digest/q를 실제 installed origin/digest/q와 대조한다. 같은 원 publication mutex 안에서 해당 table epoch·catalog epoch·Q를 갱신한다. 물리 prune/cleanup은 논리 epoch를 올리지 않는다. 복구 이후에도 `recovery_binding`의 존재가 아니라 활성 상태로 갱신한다.
- prepare/apply/publish helper의 guard·identity·receipt·Result와 Job decoder closure를 실제 각 단계의 source census에 연결했다. 순차 단계는 `max`이며 retained receipt는 호출자 수명에 별도 포함한다. 추가한 현재 prefix carrier도 binding 회계에 넣었다.

#### 실제 생산자 범위 정정

새 r2 시험은 Begin 뒤 CREATE 없는 Commit의 성공을 기대했지만 실제 producer가 `CatalogSemantic(InvalidInput)/Unchanged`로 거절하여 **FAIL**했다. `semantic_catalog/transition.rs`는 Commit status를 만들 때 `root_before.undo`를 필수로 요구하고 `images/status.rs`는 Active·해당 undo·incoming1을 요구한다. queue/core의 Commit codec 허용을 실제 live 기능 공급으로 읽은 구현자·검토자의 앞선 판단을 정정한다. 이번 C1은 기존 producer를 확대하지 않는다.

r3는 새 시험을 `empty_catalog_commit_is_rejected_without_epoch_change`로 정정하여 exact 오류·Unchanged·WAL/ledger/Q/epoch 불변·unfenced·owner/flusher idle을 확인했다. 0-mask publication 분기는 실제 호출 인수가 아니며 **성공하는 변경 없는 Commit은 NOT_RUN/후속 transaction·Row 공급 의무**다. 이 정정은 r2 실패나 원 manifest/기대값을 PASS로 바꾸지 않는다.

#### 변경별 선택 실행과 실패 보존

[동결 inventory](../../tests/fixtures/dev3-module-c1/epoch-activation/manifest.json)는 8개 selector와 compiler 입력을 담는다. 각 실행은 직전 source와의 실제 diff에 필요한 사례만 골랐고 나머지는 반복하지 않았다. compiler는 production check와 test `--no-run`이며 build를 runtime PASS로 세지 않는다.

| 보존 실행 | 선택·결과·후속 |
| --- | --- |
| r1 | compiler2 PASS. runtime census·MT01·MT02·MC01·MC02의5개 PASS 뒤 MC04 FAIL, MC03/빈 Commit NOT_RUN. 복구 cleanup 뒤에도 최초 `cut.seed()`를 넘겨 ledger의 현재 durable prefix와 불일치한 제품 오류 |
| r2 | actual current prefix 수정·helper census 보완 후 compiler2 PASS. 변경 census와 실패 MC04, 미실행 MC03의3개 PASS. 빈 Commit은 위 잘못된 기대값 때문에 FAIL. r1의 무변경 MT01/MT02/MC01/MC02는 반복하지 않음 |
| r3 | 제품 코드는 r2와 동일. 새 시험·manifest 정정에 필요한 test build1 PASS 및 빈 Commit 거절1 PASS. production check와 앞선7개 runtime은 반복하지 않음 |

MT01/MT02는 실제 Q0/Q1 publication gap 및 physical cleanup 전후 table/catalog epoch를 확인한다. MC01은 Reserve/Cancel·slot 재사용, MC02는 CLR·abort 정리 후 빈 집합, MC04는 Reserved drop/fence 뒤 복구에서 미Commit table 제거·READY 활성화, MC03은 committed Table seed 뒤 **복구 READY→새 CREATE→Commit(Q2)**에서 기존 table q1과 새 table q2를 확인한다. recovery의 cfg(test) expected-target 관측은 후보만 돌려주며 원 consumer가 pin/membership을 재검산한다. 이들은 memory Writer 선택 사례이며 native/full recovery 인수를 대신하지 않는다.

r2의 실제 census는 live peak64074/metadata137930/whole203466B, recovery peak65450/metadata106392/whole171928B다. binding closure41274B를 포함하며 recovery peak 한도65536B의 여유는86B다. 한도를 넓히거나 새 helper를 제외하지 않았다. legacy static source-census의 오래된 exact oracle 대응은 앞선 잔여 의무로 보존한다.

각 compiler 예산은 총180초/사례30초/출력8MiB, runtime은 총90초/사례30초/출력8MiB다. `RUST_MIN_STACK=33554432`는 시험 thread 설정이며 제품 메모리 보장으로 읽지 않는다. 실행 전 선택 목록·runner, 보존 source/overlay·ELF, manifest와 stdout/stderr·결과는 각각 `artifacts/module-c1-epoch-activation-20261026-r1/`, `r2/`, `r3/`에 있다.

**r1 identity** — compiler 29.23초/44,851B, runtime 25.42초.

| 대상 | SHA-256 |
| --- | --- |
| source | `9957754ae5dc9d0a1ad97eacf43da50fcfe0048f255d8412b344aab04235805e` |
| test overlay | `61c2ce5070b0ff35dc7dc447728f94261528cab0ba8610bcd3bb9a53e5bef867` |
| manifest | `dc63b596e3656248784cd58c9d14e968fa80567528d73f8b135172cf2cdea37a` |
| test ELF | `687b3abc82e9b082b617e4eeb42c69da73a2cd0bb7d9f4f23c1000cd9df4a768` |
| result.json | `1b3450dbd3408ed2c79a88f4dfb06529e90c94b1a961297d12ac666744e921bd` |
| runtime-result.json | `6e2f905c10419c08bd40e481d74b3e7dda738b4f8ef20e0beb599edbe54ae64c` |

**r2 identity** — compiler 29.10초/44,851B, runtime 16.92초.

| 대상 | SHA-256 |
| --- | --- |
| source | `81f4faba19952114b03640f961944efd269d6b36afa02558b738784295a54ee3` |
| test overlay | `c2c224da15f7b6ee4fdabf867bb41cca6e90af7c867e93bbf58012f4f2ec8c2b` |
| manifest | `dc63b596e3656248784cd58c9d14e968fa80567528d73f8b135172cf2cdea37a` |
| test ELF | `a59119843bdcc7b130f44fdc2ee2f4a8fdc1b2477a51bd55655583bd9af10ed2` |
| result.json | `9dc6c95b9a10f665aa68e9927adccf1bd6a8880cc08e85f616850ee669a0d637` |
| runtime-result.json | `2e3b6054f2c9594db39fcd45040e7a0e24aea3cd209b44fec5f9e224cf6f68c9` |

**r3 identity** — compiler 23.67초/28,594B, runtime 1.40초.

| 대상 | SHA-256 |
| --- | --- |
| source | `7e075c7e39f2f3708fe0c471340bffef165a371ec16c3673a573d2483a1c6811` |
| test overlay | `981cfe3274495dd826967c9e9430890e479bbe484be75010b3f179786f7446ab` |
| manifest | `f212b3689d3781e840c064c3b3ec31827b4c0f0e8d241f2087beb3742331a221` |
| test ELF | `46bad41d31702f67338c5b1f02e5931652b0c8e901332e1c2e894dff4d70bbe5` |
| result.json | `ffcb62ce1488aac26d8a740922a17608b2e84862b1d6c58cbc5815e475f8e372` |
| runtime-result.json | `3e6e4e31f56e1ca4a8f14c89a7e687359db830a7a96ea46cba852324686f8199` |

`/root/module_handoff_review`는 실제 diff와 source/overlay·ELF·manifest·runner·원 로그 해시, r1/r2 FAIL 보존·변경별 선택 및 문서7개를 읽기 대조하여 **C1 Catalog epoch 활성화 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. 현재 source fingerprint가 위 r3 baseline과 일치함도 확인했다. helper census 누락·복구의 초기 cut 사용은 해소됐고 성공하는 빈 Commit을 이번 인수에 포함하지 않는 정정도 수용했다. 검토자는 파일 수정·빌드·시험·verifier를 실행하지 않았으며 `/root`가 위 한정 범위를 반영한다. 거래/statement·source admission·Row API·성공하는 빈 Commit·native/full recovery·전체 C0/C1–C5·M5는 미완료다. 변경 Rust만 rustfmt로 정리했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.


<a id="c1-read-only-capsules"></a>
### C1 읽기 전용 거래·statement 수명과 장기 source 비용

기준 `52f96a5` 이후의 **progress**, D1/D2 부분공급이다. 구현·통합 `/root`, 독립 검토 `/root/module_handoff_review`. `plan4_apply/transaction.rs`에 실제 원 isolation/visibility를 공유 대여하는 `RowTransaction`·`RowStatementView`와 private slot/credit receipt를 연결했다. 새 durable actor·owner·mutex·heap allocation은 만들지 않았다.

- `MutationPort::begin_transaction`은 원 owner/core/binding/활성 epoch를 먼저 확인한다. read-only만 원 32slot 중 하나와 checked nonreused generation, 512B를 예약한다. Serializable은 isolation lock을 놓은 뒤 실제 snapshot을 등록하고 그 guard의 q를 start_q에 기록한다. RC는 transaction snapshot을 만들지 않는다. writable 요청은 첫 slot·snapshot·WAL 효과 전 Unsupported다.
- statement는 같은 capsule을 borrow한다. Serializable은 원 guard를 유지하고 RC는 statement마다 새 actual guard를 등록한다. active sequence는 한 개이고 종료 시0으로 만들지만 capsule의 별도 next sequence는 checked 증가하여 재사용하지 않는다. 원 ledger의 mark는 read-only에서None/zero다. getter는 q/None actor/revision 관측만 반환하고 snapshot guard를 외부에 노출하지 않는다.
- checked finish는 원 MutationPort의 exclusive durable ledger borrow 아래 origin·slotgen·active statement를 확인한다. Serializable은 원 isolation에 저장된 table/catalog mask를 publication 아래 검산한다. 현재 소비 read adapter가 아직 없으므로 **빈 read-set 종료만 인수**했다. legacy Catalog snapshot을 이 transaction의 조회로 연결하지 않았다. caller mask 입력 API와 raw-q/actor constructor는 없다.
- foreign-owner Before는 동일 view/capsule을 반환한다. snapshot 실패는 선예약 slot/credit을 정확히 반환한다. read-only Drop은 I/O·Commit 없이 snapshot/active sequence/credit을 정리하며 Serializable 성공을 알리지 않는다. credit0과 active sequence0일 때만 slot을 Free로 반환하고 generation은 남긴다. poison·underflow·미정 정리는 원 fence를 유지한다.

장기 source ledger는 기존4160B의 header/slot에 실제 사용량을 적으며 각 handle마다512B, 전체49152B를 넘기지 않는다. 생성/반환 carrier를 포함한 source census는 transaction400B·view328B, 짧은 helper closure4564B다. live metadata에49152B envelope를 포함하여 peak64074/metadata187082/whole252618B, recovery는 foreground handle이 없으므로 기존 peak65450/metadata106392/whole171928B다. 기존 한도는 그대로이며 snapshot registry32 admission은 별도 제한이다. 이 계산으로32개 거래·view·legacy snapshot의 모든 동시 조합 성공을 약속하지 않는다. short aggregate admission·cursor charge 및 actual concurrent reader/source 합의 인수는 후속이다.

#### 입력·선택 검증과 원 실패

[동결 manifest](../../tests/fixtures/dev3-module-c1/read-only-transactions/manifest.json)는 정상 API compiler 입력, 살아 있는 view가 있는 transaction 소비의 E0505(line8), runtime7개의 변경 관계를 기록한다. 실제 source를 보존한 overlay의 build와 실행을 구분했다.

| 실행 | 선택·결과 |
| --- | --- |
| r1 | production check PASS, test build FAIL(E0597). 신규 foreign-owner fixture가 original owner보다 안쪽 scope에 core를 선언하여 수명이 짧았음. runtime·negative probe NOT_RUN |
| r2 | 새 fixture에만 optional foreign core를 original owner보다 먼저 선언하도록 수정. 기존 macro 호출의 expansion은 동일. test build 및 미실행 E0505 probe PASS; production source는 r1과 같아 check 반복0. changed census1과 신규 lifecycle5, 총runtime6 PASS |
| r3 | 독립 검토가 RC statement snapshot 실패의 별도 cleanup 분기를 지적. 새 selector1·manifest만 추가, 제품 source는 r2와 동일. test build1 및 추가runtime1 PASS. 앞선 runtime6·production/negative probe 반복0 |

runtime의 공통 prefix는 `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::`다. census는 기존 `plan4_pages::retention::owner_tests::disk_session_isolation_backing_and_carriers_are_accounted`의 변경한 metadata/handle 회계를 확인한다. 나머지는 새 `plan4_recovery::tests::catalog_memory::transactions::` 아래 다음6개다.

| selector | 실제 oracle |
| --- | --- |
| `serializable_keeps_original_snapshot_across_statements` | 원 CREATE Forward와 prepared Commit 뒤 read-only begin, statement q0 종료→actual Commit Q1→다음 statement도 q0, sequence1→2. 빈 read-set 종료 성공; DB 전체Q 동일성을 강요하지 않음 |
| `read_committed_renews_statement_snapshot` | 같은 actual Commit 사이 q0→q1, statement별 guard 반환·sequence 증가 |
| `read_only_capacity_drop_and_generation_are_bounded` | RC32slot 후33번째 Capacity/Unchanged; Drop 후 새 slot generation 비재사용, view Drop 후 sequence2, 최종 slot/credit0·disk images 불변 |
| `foreign_owner_returns_original_handles_and_writes_are_unsupported` | unsupported writable·foreign begin 효과0, foreign finish Before의 동일 view/transaction 정상 재사용·종료, slot/credit0·unfenced |
| `failed_snapshot_registration_returns_slot_and_credit` | 실제 legacy snapshot32 포화로 Serializable begin 실패, 기존 guard 반환 뒤 새 거래 성공, 실패 generation도 재사용0·slot/credit0 |
| `failed_rc_statement_snapshot_returns_credit_without_consuming_sequence` | RC capsule 뒤 실제 legacy snapshot32로 포화, statement Capacity/Unchanged, snapshot 반환 후 같은 capsule의 sequence1 성공·종료, slot/credit0·disk images 불변 |

compiler는 총180초/사례30초/출력8MiB, runtime은 총90초/사례30초/출력8MiB로 동결했다. 시험 thread의 `RUST_MIN_STACK=33554432`는 제품 예산을 변경하지 않는다. 원 FAIL·source·overlay·manifest·runner·결과·stdout/stderr는 `artifacts/module-c1-read-only-20261026-r1/`, `r2/`, `r3/`에 보존한다. r3 선택 목록은 실행 전에 동결했다.

**r1 identity** — compiler 19.47초/50,337B, runtime NOT_RUN.

| 대상 | SHA-256 |
| --- | --- |
| source | `b3b372481d610ca8eef4e4a4859e1124ed81c6106a94ed506c7afe9a31ab73b3` |
| manifest | `1e666feed1b370148c902eca621560362c4fe1be34aac48e48273c357a18e6e4` |
| result.json | `b0c0bc836aeeafffb408ae01fbc6b98c793634929cf4b923f1295f871726c428` |

**r2 identity** — compiler 28.99초/45,649B, runtime 4.17초.

| 대상 | SHA-256 |
| --- | --- |
| source | `11ff3ca990d92b5c9ae08cf3c4b4ae4396f6252dd93e8c2c0f5156c72e74a726` |
| manifest | `1e666feed1b370148c902eca621560362c4fe1be34aac48e48273c357a18e6e4` |
| test overlay | `4bdc27db60f283f5aa95ef4621f2db02307b30975f34a79246c9f5e4601b275e` |
| test ELF | `0f2a58b88d31361c40ea6121179ef00afc3abc7157c4d1332de416197d47ffeb` |
| result.json | `7a84620434539191b83cbc8df498c0349d0b162d67d7c48e14bb04e51b3acd3a` |
| runtime-result.json | `d8da22898281466e2f459923a2fe4c8eafd4ac0a70c377528de045d98538196b` |

**r3 identity** — compiler 23.87초/28,160B, runtime 1.10초.

| 대상 | SHA-256 |
| --- | --- |
| source | `c321eabe3b9c018ce129c1b14c671aa5c337b53319030f8eb354a9307b6b3cc0` |
| manifest | `d33673540fc457a72513bb515445f2fa09a0194352a1c893bfae01c59b8637e8` |
| test overlay | `1fd9552ffe70c0ad54a87f3c12851a06c4c5336d436e327df50718b2626dad4f` |
| test ELF | `f444846b2a9187dc4c4f708b68f95e590c6e2a197a31c7b5dd2e5fa0a9ee043e` |
| result.json | `bf1334fc15ad2fe2395164a408fd4c3b2d72e1cb08236552a3800b89786ed5a4` |
| runtime-result.json | `9adc165d9f137401ebb0cc155f1e7e94282081a6929effc461260697ceefca92` |

`/root/module_handoff_review`는 실제 diff와 source/overlay·ELF·manifest·runner·원 로그 해시, r1 FAIL 보존·r2/r3의 변경별 선택 및 문서8개를 읽기 대조하여 **C1 읽기 전용 capsule/view 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. 실제 snapshot 수명, slot/generation·sequence 비재사용, foreign Before, checked finish/Drop과 private 권한 경계를 확인했다. 추가 RC 실패 사례의 credit 반환·같은 거래의 sequence1 재시도·최종 slot/credit0, 실제 census400/328/4564B 및 현재 source와 위 r3 baseline의 일치도 확인했다. 검토자는 파일 수정·빌드·시험·verifier를 실행하지 않았으며 `/root`가 위 한정 범위를 반영한다. writable Begin/terminal·실제 조회 adapter와 자동 read-set/SerializationConflict·short aggregate admission·cursor·Row A/B·native/full recovery 및 전체 C0/C1–C5/M5는 미완료다. 변경 Rust만 rustfmt로 정리했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 기존 source별 성공·실패는 해당 실행 범위의 증거로 유지한다.


<a id="c2-catalog-read"></a>
### C2 Catalog 조회 제품 본문과 원 source adapter 분리

기준 `c1de7dd` 이후의 **progress**, D1/D2 부분공급이다. 구현·통합 `/root`, 독립 검토 `/root/module_handoff_review`. 기존 동기 `ReadPort::lookup_catalog`의 visible/retired/Missing/FlushRequired 선택을 실제 [`storage::catalog::read::lookup`](../../crates/engine/src/storage/catalog/read.rs)으로 이전했다. 기존 호출은 매번 이 제품 본문을 소비하며 이전 분기를 중복 유지하지 않는다. 제품 모듈의 구현 의존은 `storage::disk`의 opaque operation·관측 타입과 순수 Catalog/transaction 값이다. `pool`, `storage.profile`, deep WAL 모듈과 reference registry 필드는 읽지 않는다.

원 `apply::read`의 [`operation.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/read/operation.rs)가 실제 membership/guard→Txn+Undo capture→caller copy→guard 반환→Status/Undo location/load→검산을 소유한다. `CatalogReadOperation`은 원 ReadPort의 필드별 borrow, 같은 mutable Owner, **실제 SnapshotGuard borrow**, caller loader/copy와 원 reference lease를 보유한다. source/Status/Undo guard는 Owner를 짧게 재대여하고 resolver I/O 전에 source latch가 끝난다. Found는 refs→lease의 checked cleanup 성공 후 copy의 `Option.take()`에서만 나온다. Missing/FlushRequired도 먼저 정리하며 오류는 원 `merge` 우선순위로 `finish_error`에서 정리 오류와 합친다. Drop은 I/O 없이 같은 정리를 수행한다. raw q·pin·새 core/registry/fence/heap allocation은 만들지 않는다.

**이행 한도:** `CatalogReadOperation::begin`은 `apply::read` 내부 private factory이고 기존 `lookup_catalog` 안에서만 생성하여 제품 lookup에 즉시 consuming 전달한다. caller는 operation/candidate/guard를 결과로 얻지 않는다. 따라서 이번 변경은 호출 간 보유 수명을 늘리지 않는다. 새 `ReadPort::catalog_operation(&RowStatementView,...)`, 자동 Catalog read-set, `CheckedTable`, RowReadOperation·cursor와 short aggregate admission은 **미공급**이다. 다음 statement entry 공개 때 원 short source 예약과 실제 공존 가능한 writer entry/progress를 함께 공급한다. 현재 설계의 statement signature를 구현한 것으로 기록하지 않으며, 기존 snapshot 조회에 Serializable 거래 종료 보증을 붙이지 않는다.

사전 독립 검토는 새 caller-retained operation만 먼저 공개하면 `active.max`가 동시 합을 보장하지 못한다고 지적했다. 원 `PageConcurrency` content/apply admission은 여러 reader를 허용하고 lookup의 Owner 재대여/I/O와도 맞지 않아 전체 operation 잠금으로 재사용하지 않았다. 위 동기 staging을 채택했다. 또한 새 consuming owner의 FlushRequired·capture 이후 오류를 기존 MT03/foreign-owner 시험이 검증하지 않음을 지적하여 실제 load 선택·I/O 입력2개를 추가했다.

#### 동결 입력·source별 선택 결과

[manifest](../../tests/fixtures/dev3-module-c2/catalog-read/manifest.json)는 정상 제품 호출 compiler 입력과 factory/field 접근 거절 입력, runtime7개의 변경 관계를 기록한다. compiler는 총180초/사례30초/출력8MiB, 최종 runtime은 총210초/사례30초/출력8MiB로 실행 전에 동결했다. 처음 runtime5개의 총150초에서 신규2개를 추가한 만큼만 늘렸고 개별 한도·제품 한도는 바꾸지 않았다. 시험 thread의 `RUST_MIN_STACK=33554432`는 제품 예산이 아니다.

| 실행 | 결과·후속 선택 |
| --- | --- |
| r1 | production PASS, test build FAIL(E0599): 새 시험이 ReadPort에 없는 physical-pin 관측 method를 호출. 실제 MutationPort 관측으로 수정. runtime/negative probe NOT_RUN |
| r2 | production/test build·private factory E0624와 field E0616 PASS. runtime 첫 census FAIL: Catalog lookup43514B, recovered peak65770B가65536B를234B 초과. 나머지 runtime NOT_RUN |
| r3 | source census의 decode 임시값 중복 합산을 정정하고 실제 FlushRequired/I/O 오류2개를 추가. production PASS, test build FAIL: 새 `evict_undo` helper의 original live/Owner 수명 관계 누락. runtime NOT_RUN. 이미 완료한 privacy2개는 변경 없어 반복0 |
| r4 | 새 helper의 `'s,'c` 관계만 명명. production source는 r3와 동일하므로 check 반복0. test build 및 runtime7 PASS. 앞선 실패는 보존하며 완료한 무변경 runtime 재실행0 |
| r5 | 독립 검토가 `missing_before`의 header192B는 LOAD 뒤에도 사용됨을 지적. 이 값만 held source에 복원. test build·변경 census1 PASS, r4 동작6개·privacy 반복0. 최종 source census는 r5 수신 |

runtime 공통 prefix는 `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::`다. 아래 첫 행은 `plan4_pages::retention::owner_tests`, 나머지는 `plan4_recovery::tests::catalog_memory` 아래 selector다.

| selector | 실제 검산 |
| --- | --- |
| `disk_session_isolation_backing_and_carriers_are_accounted` | 변경된 operation/product/return carrier를 포함한 live·recovered source census와 기존 상한 |
| `visibility_gap::catalog_mt02_q1_publication_gap` | actual Commit 공개 전·후 snapshot, 기존 key Found와 새 key 비가시성, 고정 Catalog 값/epoch |
| `commit_concurrency::catalog_mt03_pending_reader_loads_undo` | Pending 중 원 clean victim에 실제 Undo home LOAD, old snapshot Missing, 원 참조·물리 pin 반환 |
| `reference_pins::catalog_mt04_reader_pins_block_clr` | 실제 source capture 직후 Txn+Undo refs가 CLR을 Busy로 막고, lookup 종료 뒤 같은 원 CLR/cleanup 진행 |
| `read_boundary::missing_leaf_and_foreign_owner_release_original_resources` | actual empty Catalog의6/9 부재, foreign owner 효과 전 거절, refs/pins0·owner idle·disk images 불변 |
| `read_boundary::dirty_victim_returns_flush_then_retries_same_snapshot` | actual Undo flush→clean replacement로 비거주화. 원 pool의 clean frames를 실제 다른 Owner pin으로 보유하여 dirty victim 선택. FlushRequired 직후 lookup refs/pins 반환, 외부 pin 반환 뒤 원 flusher 완료, 같은 snapshot q0 재조회 Missing |
| `read_boundary::captured_undo_read_error_preserves_fatal_cause` | actual Forward/capture 뒤 비거주 Undo의 home page10 read에 BeforeError1회. `Load/PageIo`, CallerMustResolve와 원 fence 보존. fatal registry 잔여를 성공 cleanup으로 표시하지 않음 |

r2 source 계산은 `record/check`의 CatalogRecordView2개·header를 replay/LOAD 동안에도 보유한다고 합산했다. 실제 `check()`는 location 전에 반환하고, capture decode는 source guard 반환 후 별도 phase이며 finish decode도 load와 겹치지 않는다. r3은 이 값을 `decode_source_bytes`로 옮겼으나, r4 실행 후 독립 검토가 `missing_before`의 별도 header는 LOAD 뒤 actor 검산까지 살아 있음을 지적했다. r5는 그192B를 held source에 복원하고 view의 decode phase만 location/load/read와 max로 유지했다. r4의 lookup42666B 계산은 최종 회계로 채택하지 않는다. 최종 Catalog lookup42858B, live retained20880/peak64074/metadata187082/whole252618B, recovery retained22256/peak65450/metadata106392/whole171928B다. 65536/196608/262144B 상한은 그대로다. 이것은 이번 동기 호출 closure의 검산이며 미구현 전체 동시 short 합의 증거가 아니다.

원 source/overlay·manifest·compiler diagnostics·ELF·runner·선택 목록·stdout/stderr와 실패는 `artifacts/module-c2-catalog-read-20261026-r1/`부터 `r5/`에 보존한다.

| 실행 | source SHA-256 | manifest SHA-256 | result.json SHA-256 |
| --- | --- | --- | --- |
| r1 | `7690ca5e00a613a88e83ea77793873476b733661a227e42da3cf6223a78ebc22` | `61b5375efa7ee7e623e106a8aa7faf9720b9df4c872b6bc3a0d5e04fed46efdf` | `41ab00c7e1acd103883f44cec3a816175e23dda1795f5118ad6ee9a91d1ba177` |
| r2 | `1eb24accabb40f0b24421984206840ce9ed6feca9ae97cf60e1222fcf9f4e2ec` | `61b5375efa7ee7e623e106a8aa7faf9720b9df4c872b6bc3a0d5e04fed46efdf` | `1833cf15725c9d15323e8d80f92922ca8bee55881405e338dda9065c6f285809` |
| r3 | `c438836b81ad946f1313cb9063585863bb672f129568cda99ed95e5d7aeb2122` | `3bfdcff7279e8606ff06c2d5eb1fa4ed9a5f230c38480f179b77be9303acfdd2` | `df06a6cd0729ec1dfe562cc05abe921132cc2efa54863bc62bdfb2711038cccf` |
| r4 | `1d5dcc27037f6f2d229c371f7f2fb6efec65c2439d1bf6b21ceb5e2a4dae2642` | `3bfdcff7279e8606ff06c2d5eb1fa4ed9a5f230c38480f179b77be9303acfdd2` | `a55925b66d5d8d21e9d7abb5ac2ba76305bc14576086b59599b42f29af5de22c` |
| r5 | `1bfb9ede0aca0f964406a2fa19de20cd1ce2c7113df4a292ebd375390b7e5c8b` | `3bfdcff7279e8606ff06c2d5eb1fa4ed9a5f230c38480f179b77be9303acfdd2` | `0f64b83d17531fe795867ae926c011c3f78b76aab6a91e86c50c246e447ce89e` |

| runtime | test overlay SHA-256 | ELF SHA-256 | runtime-result.json SHA-256 |
| --- | --- | --- | --- |
| r2 FAIL | `e3872e3b4b199bca77c5518c47dc7075b473ee103309b9a552da84f2732be82c` | `7ee13d4dbeb5214ded02c36e990fac4d30ca87db620a43de5c5b31a0075ee88d` | `06e2539f381377622022612ac3a2aff9aaee78b77ba55e48e23f9d8563ab6865` |
| r4 PASS | `279b3061179a613cb79d66e8418293621dd6c3b24f3ddbc598634a52078a89e1` | `521f12d0c13c9e03ff48c5d3cf4a1357d7d2274f2bde100da6b25f6d6d16ada8` | `950052d5054977f384de9a950767bf921dc21f495103b6df85bd04297295ccd9` |
| r5 census PASS | `293ac22145b6ff98a1b7fe7acd03ecedfb0879362648897397379287171dcde5` | `181a70e0e8574b9f90ea506ce68c34f74d4f2223e787e7d45d578de1ba0d1d0c` | `d5ad00f223ce883cc779549d76dab0755c181c38d78f9640019ce74ad76a421b` |

compiler는 r1 19.54초/54240B, r2 38.59초/71947B, r3 19.52초/91290B, r4와 r5는 각각23.97초/28346B다. r4 runtime7은20.55초, r5 변경 census1은0.69초에 종료했다. 이 수치는 runner 검증 비용이며 성능 인수 결과가 아니다.

`/root/module_handoff_review`는 실제 diff·문서7개와 r1–r5 baseline/overlay·manifest·runner·ELF·원 로그 hash를 직접 대조하여 **C2 동기 Catalog 분리 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. header192B 복원, 원 FAIL 보존, r2 privacy2개·r4 동작6개 반복0과 r5 변경 census1 선택을 확인했다. 현재 source가 위 r5 baseline과 일치함도 확인했다. 검토자는 파일 수정·빌드·시험·verifier를 실행하지 않았으며 `/root`가 이 한정 범위를 반영한다. caller-retained statement factory·자동 read-set·CheckedTable/RowReadOperation·동시 short admission 및 전체 C2·C0/C1–C5·Row A/B·M4/M5는 미완료다. 변경 Rust만 rustfmt로 정리하고 `git diff --check`를 사용했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.

<a id="consumer-c0-start"></a>
### A/B 제품 본문 착수와 C0 단독 검증 경계

2026-10-26 22:53 KST 작업 입력 기록. 기준은 `ea783fe2419e3bdd16cb5938f508a02c73b16cd4`이며 착수 직전 index/working tree는 clean이었다. `/root`가 공통 공급 C·공유 등록·문서·시험 source 동결·통합을 소유한다. 원 작업 디렉터리가 아닌 `/home/et16/.codex/worktrees/module-independence/xadb`에서만 편집한다. 계획의 공급별 착수 규칙에 따라 C0가 이미 제공한 borrowed codec·관측 타입을 소비하는 본문부터 진행하며, 미공급 C1–C4의 전체 완료를 착수 선행으로 늘리지 않는다.

| 담당 | 독점 편집 경로 | 첫 구현·후속 수신 |
| --- | --- | --- |
| A `/root/module_consumer_survey` | `crates/engine/src/storage/row/insert.rs`, `row/insert/` 하위, `tests/fixtures/module-row-insert/` | typed 값·inline 한도 검산과 row/key/leaf 후보. 가능한 MCA00 독립 literal 입력. actual RowReserve/Forward/CLR/terminal·복구 미공급은 별도 유지 |
| B `/root/module_boundary_survey` | `crates/engine/src/storage/row/read.rs`, `row/read/` 하위, `tests/fixtures/module-row-read/` | point/next 재탐색·version 선택·출력과 진행점의 순수 본문. MCB00 독립 literal 입력. actual RowReadOperation/statement/cursor credit·복구 fixture 미공급은 별도 유지 |
| 통합 `/root` | `storage/mod.rs`, `storage/row.rs`, 공용 계약·색인·증거 및 C 공급 | 처음 검증할 C+A/C+B 각각에는 해당 모듈만 등록. 두 제품 파일/fixture를 서로 import하지 않으며 상대 source가 없는 export에서 먼저 검증 |
| 독립 검토 `/root/module_handoff_review` | 읽기 전용 | 원 계약·구현·source별 선택 증거와 미공급 경계를 검토 |

두 담당은 같은 기준에서 제품 전용 구현을 시작하도록 지시받았고 빌드/시험/commit은 통합 담당이 순차 실행한다. 상대 파일을 단지 미등록 상태로 둔 전체 트리를 단독 source라고 세지 않는다. 첫 검증 export에서 상대 전용 코드와 fixture를 제외하고 정확한 파일 inventory·지문을 남긴다. 아직 실행된 단독 인수·실제 저장·pin·snapshot·ACK·READY·복구 증거는 없다. 대역/순수 값으로 이 권한을 제조하지 않는다. 원 MCA/MCB/MCI 전체 인수와 M4/M5 완료 조건은 유지한다.

동시에 C2 후속 source admission을 검토했다. 현재 Catalog full lookup42858B와 retained20880/22256B를 합하면 동시 reader2 또는 commit29680B와 whole-call 공존을 수용할 수 없다. 원 PageConcurrency Drain도 전체 lookup·semantic reference 수명을 세지 않으므로 그대로 source drain으로 간주할 수 없다. 작은 resident class와 큰 replay/LOAD class의 실제 분리 및 원 admission 연결은 후속 설계/구현 의무다. 이번 A/B 순수 본문 착수는 이 미공급을 완료로 표시하지 않는다.


#### 실제 작성과 첫 단독 실행

A는22:52:36 KST에 작업을 시작해22:59:20.777에 첫 제품 파일을 썼고23:07:28에 마지막 코드 수정/파일별 rustfmt를 마친 뒤23:09:05에 동결했다. B는22:52:56에 시작해23:00:12에 첫 제품 파일을 썼고23:03:59에 마지막 수정,23:04:43에 동결했다. 따라서 **제품 본문 작성 구간23:00:12–23:03:59가 실제로 겹쳤다.** 두 작성자는 자신의 코드·fixture만 편집했고 빌드/시험/commit은 하지 않았다. 통합 담당의 공유 제품 변경은 `storage/mod.rs`의 row 등록과 `storage/row.rs`의 두 하위 모듈 등록이다. 이번 부분에서 공용 구현·권한 factory·기존 codec 수정과 텍스트 병합 충돌은0이었다. 전체 M5의 충돌/대기 결과나 효율 개선 측정으로 일반화하지 않는다.

A의 [`insert.rs`](../../crates/engine/src/storage/row/insert.rs)는 typed 값·2048B inline 한도와 leaf 남은 공간을 검산하고 `RowInputPlan`을 생산한다. [`insert/leaf.rs`](../../crates/engine/src/storage/row/insert/leaf.rs)는 모든 기존 row의 typed inline 입력을 확인한 뒤 고정87개 참조 scratch로 새 RowId key/row/leaf after 후보를 생산한다. actual reserve가 필요한 sealed `validate_insert`/`produce_insert_leaf` 진입점은 컴파일·정적 검토만 수신했고, runtime은 그 본문이 사용하는 순수 helper만 검증했다. 반환 `InsertLeafCandidate`는 완결 Forward/CLR나 `RowProposal`·WAL/설치 성공 증거가 아니다.

B의 [`read.rs`](../../crates/engine/src/storage/row/read.rs)는 point/lower-bound·full RowId 재탐색, own/foreign·commit-q 선택과 별도 examined/returned 진행값을 소유한다. 가시성이 없는 transaction version은 즉시 사용자 Missing으로 바꾸지 않고 Status/Missing-before가 필요하다는 순수 선택을 반환한다. 출력 부족·취소 뒤 미완료 visible 행을 넘기지 않으며, EOF 뒤에도 revision 불일치가 먼저다. 이 private 값은 actual cursor·snapshot·pin·credit·RowCopyObservation을 생성하지 않는다. 실제 source와 cleanup 조립은 미공급이다.

| 단독 source | 선택·결과 | 정답·한도 |
| --- | --- | --- |
| C+A, `insert-r1` | production/test build와 MCA00-V01..05, 별도 MCA-LEAF-V01..05 **10개 PASS**, 각1회 | nullable Bool·Int64 극값·UTF8·Binary literal, type/null/2049 거절,2048/공간 경계, 독립 XRW1/leaf bytes 전체 비교,84→85 최소행과 다음 Capacity, duplicate·유효 outer checksum/손상 row·stale LSN/foreign owner/짧은 scratch의 after 보존 |
| C+B, `read-r1` | production/test build와 MCB00-V01..03 **3개 PASS**, 각1회 | 독립 tree cell·XRW1 hex와 literal slot/key/version 기대값, 빈/범위/최대 RowId·own/foreign/generation/q, invisible 뒤 output 부족·취소·EOF/revision |

A의 Python literal 작성기는 표준 byte packing/SHA256만 사용하며 engine/B fixture를 수입하지 않는다. B는 독립 tree cell에 변경 없는 generic page envelope만 씌우고 별도 XRW1 hex를 사용하며 A producer/fixture를 수입하지 않는다. A의85행 입력은 기존84행을 Committed/undoNone·NULL로 두어85개 실제 live Undo의 자원 인수를 가정하지 않는다. source/fixture 제작과 선택 실행을 구분한다. A의 local sizeof inventory와 B의64/16/8B 타입 상한은 전체 shared codec·caller·short aggregate peak의 검산이 아니다.

실행 전 [A manifest](../../tests/fixtures/module-row-insert/manifest.json)와 [B manifest](../../tests/fixtures/module-row-read/manifest.json)에 diff→selector·입력 hash를 동결했다. 개별 compiler/runtime30초, 전체 A390초/B180초, 로그8MiB, 시험 thread stack32MiB이며 제품 cap은 변경하지 않았다. `--no-run`은 test build만 의미한다. 실제 실행은 새 selector10/3만 선택했고 기존 코드의 완료 시험·전체 wrapper는 실행하지 않았다. artifact는 `artifacts/module-row-c0-20261026/{insert-r1,read-r1}/`의 baseline/source·전체 source inventory·배제 파일 목록·등록 diff·runner·compiler/log·ELF·result에 보존한다.

각 첫 source는 상대 기능의 제품 파일 **및 fixture를 물리적으로 제거**하고 row.rs에 자신의 모듈만 등록했다. 두 source inventory의 공통 경로를 대조한 결과 hash 차이는 의도된 `storage/row.rs` 등록뿐이었다. 현재 각 기능 본문·fixture도 해당 실행 source와 일치한다. 이는 C0 순수 부분의 독립 검증이며 실제 C+A/C+B의 저장/ACK/reopen·MCI 합류 인수가 아니다. 전체 MCA00-V06의 H 없음/있음 Forward·CLR, 실제 RowReserve/Forward/CLR·read/statement/cleanup·Commit/rollback/복구 및 MI01–06는 남아 있다.

| identity | C+A `insert-r1` | C+B `read-r1` |
| --- | --- | --- |
| standalone source SHA-256 | `798f6605825ddd78b902e4d99e4c733499c491fb930abfd3fcc74a084cbfbbce` | `a687a0601d9869337c362445570beb4a22789b64c90b6ec3008cf7a25ac31a94` |
| manifest SHA-256 | `910f0aac12d8a5948ea5a473ba18b2e360e6287219d403ebeda879e6e9e7d124` | `3bc172247f2f908319032171f3c9d7f7c45ca1f7bf3774052c7d62fb6d63dd63` |
| inventory SHA-256 | `46ad425d4921691e4d9357fda86461e656713225fb00f842443c1a024ff8000d` | `552674f4622278c99692324b2f6acc5149e99970d29ecbe74585b0bda6fed39e` |
| test ELF SHA-256 | `ae7e88e02dcb7acd3350d1fff5f7ac2bc294d107c316f253da2438dedaadb05a` | `ab1f2dca9fe708a1d0b590995f3b4cc00e57497667a509889c51a318549ab9eb` |
| result.json SHA-256 | `50b4f3a03a668c35614d513c89ee25dbe9b789af925f2d9583d243a1269c4168` | `a7eafed3e2af4765de1025b71ac18b60950682e0a57a7d31d5f9981703486e26` |
| compiler+선택 실행 경과 |30.32초|29.57초|

최종 두 모듈 등록은 `assembly-r1`의 별도 production compile로6.08초에 확인했다. source는 `25658e2d87bb2f7462a6109726bc7a65ab556b2eeff96522f1d01a95ffe91c76`, result.json은 `45a793b8f03af83b1d78cab9d8d0c2fc84135d9f2524bb772acb64ab2597f020`다. 독립 실행 이후 제품 본문은 바뀌지 않아 runtime13개를 반복하지 않았다. 등록의 컴파일은 실제 MCI 상호작용 인수가 아니다.

`/root/module_handoff_review`는 A/B 원 계약·제품 본문과 독립 literal·manifest·runner·source inventory·ELF·원 로그, 문서6개 및 합친 등록의 source/원 compiler 결과를 직접 대조하여 **C0_ONLY 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. 예비 검토가 지적한 소비자 문서의 과거 포괄적 미등록 문구는 미공급 actual 경로/함수/case에 한정하고 이번 C0 기록을 연결했다. 검토자는 파일 수정·빌드·시험·verifier를 실행하지 않았고 `/root`가 한정 수용을 반영한다. 현재 source와 합친 등록 source의 일치, runtime 반복0도 확인했다. 변경 Rust의 파일별 rustfmt와 `git diff --check`를 사용했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 이 현재 부분을 전체 A/B 제품 인수나 M4/M5 완료로 승격하지 않는다.


<a id="c2-read-operation-lifetime"></a>
### C2 실제 조회의 전체 수명과 Commit Drain 연결

기준 commit `278e5e8a8e313979ce1720b21f278df2ac6bec97`에서 원 `PageConcurrency`의 OwnerSlot에 `read_operation` 표시를 추가했다. 기존 Idle/Content 모드와 독립인 표시이며 고정34개 슬롯을 직접 세므로 별도 counter·owner·manager가 없다. actual Catalog lookup은 첫 transient reference 예약 전에 private `ReadOperationAdmission`을 얻고, source capture/Status·Undo 위치 조회/LOAD/결과 선택 후 refs와 lease의 checked cleanup을 시도한 다음 표시를 반환한다. 정상 cleanup은 refs0이며, 기존 visibility가 fence 뒤 보존하는 fatal refs는 그대로 남고 영구 fence가 Commit 설치를 차단한다. 원 core의 known operation 표시 반환을 ref cleanup 성공으로 해석하지 않는다.

Pending 중 신규 조회·Undo LOAD는 계속 허용한다. Draining/Installing에서 신규 operation은 InstallationPending/Unchanged이며 원 `try_content` 거절도 유지한다. source를 보유한 같은 owner의 Apply/Commit 진입은 막고, healthy Owner Drop은 표시가 남은 슬롯을 free/reissue하지 않는다. Commit Drain은 기존 apply/content0에 더해 read-operation0을 확인한다. 이미 시작한 조회가 Draining을 만나면 기존 페이지/claim 거절과 checked cleanup으로 유한 반환하며, 원 Condvar를 깨운다. OS I/O 완료 시간 자체를 보장하지 않는다. 장기 snapshot을 강제 종료하지 않는다.

`CatalogReadOperation`의 ReadPort40B 복사를 같은 짧은 수명의 공유 참조8B로 바꾸고 ticket24B를 보유했다. `Found`는 여전히 caller copy의 원 수명으로 반환하며 operation factory는 private 동기 진입에서만 사용한다. slot16B·test core816B·ticket24B·입구/정리 helper closure280B를 관측했다. Catalog lookup source는42858→42850B다. live peak64074/metadata187082/whole252618B, recovered peak65450/metadata106392/whole171928B는 다른 최대 phase 때문에 그대로이며 기존65536/196608/262144B 한도를 유지한다.

이 단위는 [short source 계약](../contracts/dev3-module-boundaries.md#operation-source-admission)의 **조회 수명/원 Drain 연결 부분공급**이다. isolation header24..32의 byte 예약, 모든 동시 operation의 합, writer의 Pending/설치 phase 비용 분리, rollback/terminal progress 몫과 writer1+resident reader2는 아직 구현·검증하지 않았다. 이 표시를 전체65536B aggregate admission이나 모든 ReadPort entry의 수명 계수로 부르지 않는다. statement factory·자동 read-set·CheckedTable/RowReadOperation과 C1–C5·M4/M5 전체는 미완료다.

#### 변경별 검증과 원 실패

[동결 선택 목록](../../tests/fixtures/dev3-module-c2/read-operation/manifest.json)은 각 selector와 실제 diff의 관계를 명시한다. 한 실행의 총480초·case30초·로그8MiB·test stack32MiB를 사전에 고정했다. compiler는 production check와 test `--no-run`이며 runtime 증거와 구분한다. 기존 native/current-build recovery·전체 workspace·완료한 하위 회귀 wrapper는 실행하지 않았다.

| 실행 | 실제 결과·후속 범위 |
| --- | --- |
| r1 | production PASS. 신규 fixture의 RetentionCause import 누락 E0433로 test build FAIL, runtime0. 원 diagnostics 보존 |
| r2 | test import만 수정. 같은 product의 production은 반복하지 않음. test build PASS, 새 low-level5·실제 healthy Drain1 PASS. fenced Drain fixture가 기존 fatal refs2를0으로 기대하여 FAIL. 이후6개는 NOT_RUN |
| r3 | 원 visibility cleanup의 fence 뒤 ref 보존 의미에 맞게 fenced 기대값과 이름을 정정하고 close 주석을 구체화. 제품 동작 변경0. test build PASS, 실패 fenced1+미실행6 PASS. r2의6 PASS는 반복하지 않음 |

총13개의 최종 선택 결과는 source별 r2의6개와 r3의7개다. r2 FAIL을 r3 PASS로 덮어쓰지 않는다. 새5개는 Content 재대여/중복·Apply Busy/재발급 후 old close·foreign/flusher 거절·Maintenance 허용·forget/너무 이른 반환·fence/poison·unwind를 검산한다. 새 실제2개는 source capture 후 **content0/source1/refs2**에서 Commit이 Drain 대기 조건에 들어간 것을 관측하고, late lookup 효과 전 거절과 정상 정리 후 Commit 또는 fence 뒤 refs2 보존·source0·Abandoned를 확인한다. `DrainWaiting`은 원 wait 조건이 만족되지 않은 뒤 mutex 밖에서만 pause하며 권한을 만들지 않는다.

직접 변경된 기존6개는 MT03 Pending Undo LOAD, empty/foreign, dirty FlushRequired와 같은 snapshot 재진입, capture 뒤 실제 home I/O 오류, MT02 Q 공개 전후 Found/Missing, 변경 carrier/source census다. 기존 MT07의 무변경 복구 검사를 반복하지 않고 새 fenced memory 사례로 변경된 Drain 경계만 검증했다.

원 source inventory·manifest·runner·compiler diagnostics·ELF·stdout/stderr는 `artifacts/module-c2-read-operation-20261026-r1/`부터 `r3/`에 보존한다.

| 실행 | source SHA-256 | manifest SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 FAIL | `99bfc9c3e7860df7e17d918a3dcf10a8140920261048e282584a65d68236b378` | `957c2b4bcd79aaadc68e6afe7fdf5de5dca313f6ebb0e670dd7afc1f944eda7f` | `8884868ec4bd62bda830562abb5e934e30fc74129cbc31a8a587140645bf6503` |
| r2 FAIL | `360fcba40791dee1c88ab8c76973c5ea5256fd211f609d08b709f9c1ab75fe03` | `957c2b4bcd79aaadc68e6afe7fdf5de5dca313f6ebb0e670dd7afc1f944eda7f` | `258138a88b581d2d3f0ff8d3c28af50eb20de06a0dd3de83d1bea5e8af8d2cc5` |
| r3 PASS | `0c7908b29bd1e93d0f763c9bfa86d6acc0d1baf9c1b55cd0818c29d57616429d` | `a5ae75b52fd74e69f207f5ef5b5608b813e5937d32dfeb3f459fb5ee232d9ba2` | `33f4210bf781493aad1f26ce18912e66bd80a9bb975a0c5ddf3393d3977872be` |

r2 ELF는 `a2b2899de3576a2aee43e089b63e5ff2d4b0f7e07da4ea3f58609ee7c3615eaa`다. r3 ELF는 `9401632d5bc94634d72b939a844cecc63a9384eea7edd305297e642f8d70b953`다. r1/r2/r3 실행 비용은 각각19.38/27.00/43.45초이며 성능 인수 수치가 아니다.

`/root/module_handoff_review`는 실제 제품·시험 diff, 원 source/manifest/ELF/result/log identity와 최종 문서7개를 대조하여 **C2 조회 수명·Commit Drain 부분공급 D1/D2 최종 한정 수용·필수 수정0**으로 판정했다. 신규2개·수정7개 경로 등록, r1/r2 FAIL 보존과 r2의6 PASS/r3의7 PASS 분리, unchanged 완료 사례 반복0을 확인했다. 검토자는 파일 수정·빌드·시험·verifier를 실행하지 않았고 `/root`가 이 한정 범위를 반영한다. 변경 Rust 파일만 rustfmt로 정리하고 `git diff --check`를 사용했다. 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.


<a id="c2-catalog-scratch"></a>
### C2 Catalog 후보 출력 loan 재사용 — 2026-10-27

기준 commit `8247686`에서 `semantic_catalog/images/catalog.rs::change_catalog`의 Forward/Prune가 기존 caller after-page의 앞4588B를 record scratch로 사용하도록 바꿨다. 기존 `value[4588]` local은 제거하고 `cell[4600]`은 유지한다. 기존 encoder가 record를 별도 cell에 복사한 뒤 scratch borrow가 끝나며, 마지막 leaf encoder가 전체 after-page를 초기화하고 canonical bytes로 쓴다. immutable before와 alias하지 않고 새 allocation·공유 owner·codec API·wire 형식·물리 효과를 추가하지 않는다. 오류 뒤 after는 부분 후보이며 설치할 page가 아니라는 계약을 함수 주석에 명시했다. 상위 produce/observe도 원래 여러 after-page를 순서대로 작성하므로 오류 때 후보 loan을 그대로 설치하는 계약이 아니다.

`semantic_catalog/resources.rs`는 제거한 local4588B 대신 실제 mutable slice16B를 계산한다. 이는 해당 Catalog image 분기의 직접 비용 감소이며 **전체 source peak 감소가 아니다.** 선택 census에서 history_retention35090/catalog_lookup42850B, live peak64074·metadata187082·whole252618B, recovered peak65450B는 모두 이전과 같았다. 다른 branch/phase가 최대값이므로 이번 변경으로 quota 여유를 확보했다고 하지 않는다. shared retained 한 번과 모든 동시 입구·실행·반환의 합을 닫는 short admission, writer1+resident reader2, 실제 Row/terminal/recovery 공급과 전체 M4/M5는 계속 미완료다.

#### 실제 변경과 선택 검증

[선택 manifest](../../tests/fixtures/dev3-module-c2/catalog-scratch/manifest.json)에 마지막 제품 diff와 selector의 관계 및 총480초·각30초·로그8MiB·test stack32MiB를 고정했다. production check·test `--no-run`은 빌드 증거다. 원 fixture·generator·정답·제품 한도는 바꾸지 않았다.

| 실행 | 선택·결과 | 범위와 보존 |
| --- | --- | --- |
| r1 | production/test build, pure transition9개(C03/C05/C13/C15/P02/R05/N14/N25/N19), MT03, source census 각1회 PASS | Forward/Prune의 기본·기존 다른 항목 보존·최대4588/4600B, CLR, projection/length/비선택항목 오류를 독립 page/frame literal과 대조. 정상6개는 producer와 독립 golden observer를 모두 검산하며 Forward observer는 borrowed View를 사용한다. N14/N25는 입력 거절, N19는 observer의 비선택 항목 변조 거절만 검산한다. MT03는 fresh Pending replay/Undo LOAD와 Commit까지만 실행 |
| r2 | test build와 추가 MF09 한 사례 PASS | 원 memory Directory 이미지에서 durable Commit 중단 뒤 현재-build recovery replay·LSN9→15 committed Prune/cleanup·READY/Found를 확인. 제품 파일은 r1/current와 같으며 r1의11 PASS·production은 반복0 |

r1 manifest의 MT03 선택 이유에는 `current-build recovery/Prune`를 잘못 포함했다. 독립 검토에서 실제 selector가 해당 경로를 호출하지 않는 것을 확인했다. **원 r1 source/manifest/result를 그대로 보존**하고 현재 manifest의 설명을 정정했다. 그11개 PASS를 복구 실행 결과로 확대하지 않는다. 변경 helper의 memory 현재-build recovery 및 실제 committed Prune는 별도 MF09로 선택해 통과했고, 앞선11개와 production check는 반복하지 않았다. native/새 PID 복구·전체 workspace·전체72개 pure transition·완료한 무변경 wrapper를 실행하지 않는다.

원 source·전체 inventory·manifest·runner·compiler diagnostics·ELF·stdout/stderr·result는 `artifacts/module-c2-catalog-scratch-20261027-r1/`과 `r2/`에 보존했다. source별12개 선택 결과를 수신하며, 원 설명 오류나 과거 FAIL을 소급 삭제하지 않는다.

| 실행 | source SHA-256 | manifest SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 PASS | `30400294f4c20117891e500850b5817a43e7403a0c64dc35e0f5fa1f219cad91` | `c996ccee3fbde511e633c7f82aaa16a7b3de41cdca67b3ce632f87ae3bf5cd80` | `74b5c3136d09f04ccd6fab415fc3bc230cbd4f131558824d92e3d55d22f131d3` |
| r2 PASS | `ccc4eac7ffe2d1bc10f5ec1762ce286f2bfea135243f79f97f19d4dca1fa44f3` | `b7b9951ffc622aed18984a00c656fe0764e70863f4f28c24509d23869f47fb44` | `fd5c8276c84319006ee7757f06882a392e5f69916df3818a871420c6cd7d77a1` |

r1 ELF는 `95bc54c4216fe4a503104667281777ba48aa68fc762b44695a078fbc939cc41f`, r2 ELF는 `463ce023e36b350d69fec3ed29e1d3ff900ad7daf0075491ce85552b308b127e`다. 실행 비용34.38/33.58초는 성능 인수가 아니다. 변경 Rust 파일별 rustfmt와 `git diff --check`를 사용했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.

`/root/module_handoff_review`는 제품2파일, r1/r2 원 source5613개·manifest/runner/result/ELF/로그 identity와 문서5개를 직접 대조하여 **C2 Catalog scratch 재사용 D1/D2 한정 수용·필수 수정0**으로 판정했다. MT03 범위 오류와 N19 observer-only 설명 정정, 실제12개 결과·r1→r2 제품 변경0·완료 사례 반복0, 전체 peak 불변과 잔여 aggregate/M4/M5 의무를 확인했다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았고 `/root`가 한정 수용을 반영한다.


<a id="c2-transaction-source-phases"></a>
### C2 Catalog 거래·Aging 메타데이터 생성 수명 분리 — 2026-10-27

기준 commit `e0c1900`의 `semantic_catalog/transition.rs`에서 predecessor descriptor를 읽어 inventory만 반환하는 helper를 추출했다. `ledger.rs`는 단일 mutable descriptor로 다음 거래 후보를 만들고, encoder가 반환한 뒤 기존 validator가 output을 다시 검산한다. 이전 apply를 previous에 먼저 보존하고 terminal/apply·inventory·groups overflow·WAL overflow·Forward mutation 검사 순서를 유지한다. 오류 전에 output을 쓰는 시점과 검증 실패 뒤 후보 output의 의미도 유지한다.

첫 source 관측에서 거래 경로보다 Aging successor가 최대값임을 확인했다. 후속 변경은 실제 `next_aging`을 budget 검산·predecessor 관측·최종 encode로 나눴다. budget helper는 전체640B canonical equality와 Root digest를 검사한 뒤 유일하게 비영(非零)이 될 수 있는 category3의112B 행만 반환한다. 나머지4행의 zero 검사는 그대로이며 canonical640B와 전체 budget560B는 predecessor decode 전에 수명을 마친다. predecessor helper는 기존 decode/descriptor 추출·Root 순서/비용·overflow 검사를 보존하고 chain/inventory만 반환한다. 최종 Root-bound encode와 wire1536B는 유지한다.

`resources.rs`는 실제 helper 호출 경계와 argument·owned descriptor·Option/Result·반환 tuple·budget array 생성 비용을 반영한다. test-only branch 관측은 원 owner census 시험에서 출력하며 제품 권한을 발급하지 않는다. D1/D2 담당·통합은 `/root`다. 단순 수식 차감 대신 실제 지역 값의 수명을 나누는 방식을 선택했고, descriptor 검산을 제거하거나 heap/별도 owner로 옮기는 대안은 채택하지 않았다. 공통 admission 계약·제품 한도·형식·오류 의미는 변경하지 않는다. helper 밖에 다시 큰 값을 보유하거나 호출/반환 경로가 달라지면 해당 source 식을 재검토한다.

#### 변경별 선택 실행

[선택 manifest](../../tests/fixtures/dev3-module-c2/transaction-phases/manifest.json)에 source diff와 selector 관계, 총480초·case30초·로그8MiB·test stack32MiB를 고정했다. 각 source의 production check와 test `--no-run`은 빌드 증거이며 runtime PASS와 구별한다. 기존 fixture·generator·oracle·제품 한도는 변경하지 않았다.

| 실행 | 변경·선택 결과 | 적용 범위 |
| --- | --- | --- |
| r1 | 거래 inventory/encode/validate 분리. production/test build PASS, exact12개 PASS | C01/C02/C03/C04·R04/R05/R06·K03·N13, source census, 실제 MC02 Forward/CLR/Abort cleanup, MF09 현재-build memory recovery |
| r2 | 실제 Aging budget/predecessor/encode 분리. production/test build PASS, exact7개 PASS | G01 최초 descriptor·C03 후속/nonzero aggregate·C05 Prune·C09 RootFree/TerminalJob debt1 유지·N18, 변경 census, MF09 현재-build memory recovery |

r2 frozen manifest/result의 C09 설명은 `last debt cleared, zero aggregate`로 잘못 기록됐다. 최종 독립 검토에서 generator와 golden frame의 descriptor를 대조하여 RootFree 뒤 TerminalJob1개가 남고 category3 steps1/WAL55112임을 확인했다. 현재 manifest 설명만 정정하고 원 r2 source/manifest/result·PASS를 그대로 보존한다. G01의 zero와 C09의 잔여 debt 보존을 구별하며, 제품·fixture·oracle 변경이나 시험 재실행은 없다.

N18은 독립 변조한 후행 Aging 비용을 거절하는 **observer-only** 사례다. producer를 호출하지 않으며 잘못된 aggregate를 `aging_budget`에 직접 입력한 시험으로 기록하지 않는다. r2의 C03/census/MF09는 r1 이후 Aging 제품 코드가 실제 변경되어 다시 선택했다. r1 거래 전용 전체12개를 반복하지 않았다. 두 source의19회 실행·16개 고유 selector 결과이며19개 고유 사례나 최종 source에서19개 실행으로 합치지 않는다. native/새 PID 복구·전체 workspace·하위 wrapper·무변경 완료 시험은 실행하지 않았다.

#### 실제 source 값 상한

| source 경로 | r1 | r2 | 차이 |
| --- | ---: | ---: | ---: |
| Aging successor | 22342B | 21044B | −1298B |
| semantic compute | 26682B | 25384B | −1298B |
| history retention | 35090B | 33792B | −1298B |
| Catalog lookup | 42850B | 41552B | −1298B |
| live peak | 64074B | 63184B | −890B |
| recovered live | 65450B | 64560B | −890B |
| recovered peak | 65450B | 64611B | −839B |

r1은 거래 경로를 실제 분리했지만 전체 peak는 이전과 같았다. r2에서는 Aging predecessor20222B가 해당 branch의 주요 비용이며 recovered 전체 peak는 기존 bind64611B가 지배한다. live retained20880B·recovered retained22256B·Catalog Commit29680B·읽기 handle400/328B는 그대로다. live metadata186192/whole251728B, recovered metadata106392/whole171928B를 관측했다. 이는 **source의 값·수명에 따른 상한이며 RSS나 컴파일러 stack 실측이 아니다.** 실행 시간도 성능 인수로 해석하지 않는다.

recovered retained22256B와 full lookup41552B의 합은63808B로65536B 아래지만, 남은1728B에서 모든 동시 진입/거절/반환 carrier나 writer를 수용했다고 할 수 없다. shared retained 한 번과 모든 동시 operation의 합, writer/reader/progress admission·writer1+resident reader2, caller-retained factory/read-set·실제 Row/terminal/recovery 공급과 전체 M4/M5는 미완료다.

원 source export·inventory·manifest·runner·compiler diagnostics·ELF·stdout/stderr·result는 `artifacts/module-c2-transaction-phases-20261027-r1/`과 `r2/`에 보존한다.

| 실행 | source SHA-256 | manifest SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 PASS | `ec2d8434cab7105f85265f66426adb9520f1ff50bc2b03a843fec550e1879c43` | `3dbf881f159b7663ff6aa76257745a08b79080987ef0c785c99be7ab329bdecd` | `463fa45ac845ac31561da8ccd5f2524af33b862e94f944483d12772b9dc30628` |
| r2 PASS | `e7ae87949cdde22c34841e3b71af3bcc2b4446cc13c40b44f55300cda3948477` | `1ae67aa5b8c4f241777840a7e740d9cf4d59af9ba8a9587a121caef823825fb6` | `40dc2f4d4f4054c3a1b81c9e85a46937d534bf8e64d36eda8a0ac28e96c8423c` |

r1 ELF는 `f19bea4ccb51e2c240221b05d83fcd8f400b3eb0b719ad878de14227ade412dc`, r2 ELF는 `a1daba9e115237e8ee199baf516db17e0c251d8aa416532c35fddb4529c9f3c5`다. 실행 비용은 각각42.58/40.27초다. 변경 Rust 파일별 rustfmt와 `git diff --check`를 사용하며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않는다.

`/root/module_handoff_review`는 제품5파일, 원 source5614개·manifest/runner/result/ELF/로그 identity와 문서5개를 직접 대조했다. C09 설명 오류를 지적했고, 현재 manifest·증거의 정정과 원 frozen 기록 보존을 좁게 재확인한 뒤 **거래·Aging 생성 수명 분리 D1/D2 한정 수용·필수 수정0**으로 판정했다. source별12+7회/16개 고유 selector, source 값 상한 감소와 aggregate/M4/M5 미완료를 확인했다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았고 `/root`가 한정 수용을 반영한다.


<a id="c2-pending-carrier"></a>
### C2 Commit Pending의 원 pin 보유와 stage 수명 분리 — 2026-10-27

기준 commit `a432b46`에서 `dirty_frame::PendingCommitTargets`가 빈 page/version guard를 포함한 `WriteTargets` 대신 **동일 원 PinSet**을 보유하도록 바꿨다. suspend는 기존 순서대로 page guards를 해제하고 authentic StagedCommit으로 Pending을 만든 뒤 pin을 이동한다. 빈 이전 WriteTargets는 함수 반환과 함께 끝난다. resume는 original Durable 확인·Drain 대기 뒤에만 빈 guard storage를 다시 구성하고, 동일 PinSet과 stable bytes를 원 installer에 이동한다. pin 예약·등록·DPT 권한을 새로 만들지 않는다.

Pending의 오류/Drop은 원 Pending guard의 fence 처리를 먼저 수행하고 pin을 checked 반환한다. PinSet의 generation/count 검산·underflow/poison 정책과 failed install의 guard→admission→pin 정리는 유지한다. 기존 test-only release 관측도 보존한다. `catalog_commit.rs`는 실제 preflight/write/Intent/stage/mark_staged/suspend를 private `stage_catalog`로 추출하여 WAL wait 이전에 함수 지역 값이 끝나게 했다. helper는 실제 Pending·Wait·InstallationKey·WalRequirement만 반환한다. PreparedCatalog·원 exclusive control과 epoch 검산은 기존 caller에 남고, stage 거절의 authentic Unstaged restore/cancel 및 오류 우선순위는 유지한다.

D1/D2 담당·통합은 `/root`다. 원 자원을 이동하는 bounded 내부 구조를 선택했다. 비어 있는 guard를 계속 보유하면서 수식에서 제외하거나 별도 owner/heap에 옮기는 대안은 채택하지 않았다. shared-retained 한 번과 모든 동시 entry/실행/return의 합을 닫는 admission, writer/reader/progress별 고정 charge, resident 조회 경로와 실제 Row/terminal/recovery·M4/M5 전체는 아직 미완료다. 이 단위는 원 Pending 수명을 후속 phase 회계에 사용할 구조로 나눈 것이며 admission 완료나 writer1+resident reader2의 용량 증거가 아니다. Pending이 다시 guard/대형 임시 값을 보유하거나 정리 순서가 바뀌면 재검토한다.

#### 선택 검증과 자원 관측

[선택 manifest](../../tests/fixtures/dev3-module-c2/pending-carrier/manifest.json)에 마지막 diff와11개 selector의 관계, 총480초·case30초·로그8MiB·test stack32MiB를 고정했다. production check·test `--no-run`은 각각 빌드 PASS이며 runtime과 구별한다. 다음11개 exact runtime이 각각1회 PASS했다.

| 선택 | 실제 변경과 검증 범위 |
| --- | --- |
| common source bounds·capacity/fence priority2개 | 변경 Pending layout 및 재구성 carrier 계산, 기존64-target capacity 거절과 fence 우선순위 |
| MT05·MT03·MT07 | 동일4pin 유지/Pending lower flush 뒤 recLSN 재검산, Pending Undo LOAD, Drain의 content/fence 오류와 원 정리 |
| MF02·MF03·MF07·MF09·MF10 | MF02는 새 stage helper의 StageCallStarted unwind, MF03/MF07은 RAM stage 뒤 Pending의 file write-before/sync-before error, MF09/MF10은 durable/second-page unwind와 실제 current-build memory recovery |
| 원 owner census1개 | 변경 물리/Catalog source 식의 live·recovered 합계 수신 |

원 r1 manifest/result는 MF03를 `pre-stage rejection/Unstaged restore/cancel`로 잘못 설명했다. 실제 stage는 RAM complete prefix를9로 진행하며 MF03는 `wait_catalog_commit`의 file write-before 실패 후 synced8을 검산한다. 따라서 새 Pending의 오류/Drop 정리 증거이며 Unstaged restore/cancel 실행 증거가 아니다. MF07 reason의 포괄적인 recovery 표현도 정정했다. 실제 현재-build memory recovery는 MT07/MF09/MF10이며 MF03/MF07의 io_failure 함수는 이를 호출하지 않는다. 원 frozen source/manifest/result·로그는 보존하고 현재 선택 이유와 이 기록만 정정했다. 제품·fixture·oracle 수정 및 시험 재실행은 없다.

네 target의 Pending 실제 타입은800B다. 이전 세 field의 크기 합912B와 비교하여112B 줄었다. 일반 물리 chain의 보수 source 합은 N4=11888B/N9=20368B다. 새 helper output/Result/borrowed arguments 및 Drain 후 WriteTargets 구성도 청구했으므로 **Catalog Commit의 포괄 상한은29680→31752B로 증가했다.** 이를 전체 source 비용 절감으로 표현하지 않는다. 실제 배타 phase의 좁은 charge 계산과 admission 연결은 후속이다.

이번 census에서 Catalog lookup41552B, live peak63184/metadata186192/whole251728B, recovered live64560/peak64611/metadata106392/whole171928B는 이전과 같았다. source 값·수명 계산 및 타입 크기이며 RSS/컴파일러 stack 실측이나 처리 성능 결과가 아니다. 변경 없는 semantic transition 시험·전체 workspace·하위 wrapper·native/새 PID 인수는 실행하지 않았다.

원 source export/inventory·manifest·runner·compiler diagnostics·ELF·stdout/stderr·result는 `artifacts/module-c2-pending-carrier-20261027-r1/`에 보존한다.

- source SHA-256: `35114f98d6748893fde839a3d1bd7ad29759c947c65808f6703811b5f40f2b0b`
- manifest SHA-256: `47e1fd89b3a1dc089af53330561a69d15d55e014440e520611fda81ce2388c13`
- ELF SHA-256: `f3ddc9b7a6f6622a7a441bf79c1b3ee01f0842cfb201d33cc75de9def746691c`
- result SHA-256: `09ecad8dec0a40b517a01bd8fd14874ad475905d9fb20d465984eca9e9d600ac`

실행 비용60.33초는 성능 인수가 아니다. 변경 Rust만 파일별 rustfmt로 정리했고 `git diff --check`를 사용했다. 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. `/root/module_handoff_review`는 제품3파일, 원 source5615개·manifest/runner/result/ELF/로그 identity와 문서5개를 직접 대조했다. MF03/MF07 실행 설명 정정과 원 증거 보존을 확인한 뒤 **Pending 보유/stage 수명 분리 D1/D2 한정 수용·필수 수정0**으로 판정했다. compiler2/runtime11 PASS, Pending 크기 감소와 Catalog Commit 포괄 상한 증가, aggregate/M4/M5 미완료를 확인했다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았으며 `/root`가 한정 수용을 반영한다.


<a id="push-20261027"></a>
### main 전송 전 Clippy 보완 — 2026-10-27

사용자가 push를 요청했고 이어서 **main으로 전송**하도록 명시했다. 최초 `36d8e82` 전송 시도의 pre-push 훅은 포맷을 통과했으나 Clippy5개 지적 때문에 원격 전송을 차단했다. 원 로그는 `artifacts/module-push-20261027-r1.log`에 보존하며 성공으로 바꾸지 않는다. 최초 시도 대상이었던 별도 원격 브랜치는 생성되지 않았다.

수정은 제품 동작·형식·원 권한·메모리 배치를 바꾸지 않는3파일에 한정한다. recovery의 두 private codec 오류 variant는 기존 원인과 visibility를 유지하도록 해당 variant에만 `private_interfaces` 사유를 명시했다. statement/transaction 종료의 Before 반환은 동일 inline handle과512B 회계를 유지하도록 두 함수에만 `result_large_err` 사유를 명시했다. Row INSERT의8개 usize carrier는 byte 합을 명확히 나타내는 `size_of::<[usize; 8]>()`로 표현했다. 힙 할당·공개 범위 확대·lint 전역 허용은 추가하지 않는다.

검증 대상은 실제 push할 commit의 포맷·Clippy·문서 pre-push gate다. 위 attribute/동일 크기 표현 변경에 제품 동작 변경이 없어 완료한 runtime 시험은 반복하지 않는다. `/root/module_handoff_review`가 실제3파일 diff와 원 Clippy 지적·기존512B 반환 회계를 대조하여 좁은 D1 수용·필수 수정0으로 판정했다. 검토자는 빌드·시험·문서 검사를 실행하지 않았고 `/root`가 반영한다. 최종 전송 검증은 이 보완 commit 이후 원 pre-push 훅에서 수행하며 실제 전송 로그와 구별한다. 이전 source별 제품 PASS·FAIL과 M4/M5 미완료를 유지한다.

후속 `522ec12` main 전송 시도는 Rust 포맷·Clippy를 통과했지만 문서4개 링크의 `파일.rs:줄번호` 표기를 파일 이름으로 해석하여 차단됐다. 원 로그는 `artifacts/module-push-20261027-r2.log`에 보존한다. 실제 네 소스 파일의 존재를 확인하고 상대 Markdown 링크에서 줄 번호 접미어만 제거했다. 설명·계약 의미·제품 변경0의 D0 정정이며 최종 전송 시 원 훅을 다시 적용한다.


<a id="c2-commit-phases-main"></a>
### C2 Commit 단계별 source 비용 구체화 — 2026-10-27

사용자의 “진행 위치 확인 후 다음 한 단계” 요청에 따라 clean `main`·`0852f06ff9fa2ec42fdb73f4353373b9ef45e78d`에서 재개했다. 기존 모듈 worktree와 main은 같은 HEAD였으며 이번 편집·증거는 `/home/et16/work/xadb`에 둔다. M0–M3는 완료·채택, M4의 마지막 제품 단위는 Pending pin/stage 수명 분리였고 M5는 A/B 순수 본문 착수까지다. 이번 종료 범위는 그 다음 의무인 **Commit의 실제 배타 phase 비용 계산** 한 단위다. 전체 M4/M5 실행 완료로 확대하지 않는다.

`D3-MODULE-COMMIT-PHASES-001` · D2 · **ACCEPTED / C2 한정 부분공급**. 담당·계약·통합 `/root`, 독립 검토 `/root/module_step_review`. 기존 실행 권한·이번 한 단계 지시 아래 §6.9의 고정 source 계산을 구체화한다. 포괄 합을 유지하는 대안은 안전하지만 실제 준비·대기·설치의 배타성을 후속 admission에 설명하지 못한다. stage/Pending/install 타입만 단순 max하는 대안은 전환 시 공존하는 반환값을 누락하므로 채택하지 않았다. source 값의 계산 변경이며 저장 효과·wire·원 pin/owner·Commit 순서 변경은 없다.

물리 `source_phases`는 prepare/suspend 중 Stage와 Pending 반환, resume 중 Pending·새 WriteTargets·Install/Result를 함께 계수한다. 같은 target-call/cleanup envelope를 각 phase에 보수적으로 유지한다. concurrency의 원 `source_bytes`를 core 실체와 nested 호출 값으로 분리하여 prepare 오류·Drain/Condvar·설치 반환을 명시 청구한다. Catalog는 실제 `stage_catalog`의 반환 경계를 기준으로 preflight/stage/Pending/install 비용을 나누고 PreparedCatalog·epochs·늦게 생성되는 완료값·오류/호출자 반환은 모든 phase에서 계속 청구한다. 물리 `prepare_commit`의 기존 포괄 Capacity 식은 바꾸지 않아64-target 거절을 유지한다. 실제 Catalog의 Capacity 검사는 새 phase 최대값을 소비한다.

#### 독립 검토 보완과 변경별 선택

예비 독립 검토는 다음 단위의 적절성을 확인하고 전환 carrier·Capacity gate·nested core 값 누락을 경고했다. 첫 실제 diff 검토에서 `PageConcurrency`를 기존 shared retained가 이미 계수한다는 주석의 근거가 없음을 발견했다. `DiskSession`/`SharedPool`은 core 참조만 보유하므로 Catalog의 공통 phase 비용에 **실체816B를 한 번 추가**하고 주석을 정정했다. 이 보완 전 r1 실행 PASS와 검토 미해결1건을 보존하며 r1만으로 최종 회계 계약 수용을 주장하지 않는다. 후속 전체 동시 admission에서는 원 core를 실제 shared-retained 한 곳으로 이동하고 모든 class와 중복 없이 계산해야 한다.

당시 main 선택 manifest(`cbc09f2:tests/fixtures/dev3-module-c2/commit-phases/manifest.json`; [원본 보존 위치](#merge-main-20261028))는 직전 검증 source를 지정한다. 각 원 실행의 manifest는 해당 export에 보존한다. 실행 전 총480초·case30초·로그8MiB·test stack32MiB를 고정했다. production `cargo check`와 test `--no-run`은 빌드 결과이며 runtime과 구분한다.

| source | 선택·실제 결과 | 변경과의 관계 |
| --- | --- | --- |
| r1 | production/test build PASS, exact3개 각1회 PASS | 신규 물리 phase 하한·64-target 기존 gate, 수정 owner census, 변경 Catalog Capacity를 통과하는 실제 MT05 Commit |
| r2 | production/test build PASS, exact2개 각1회 PASS | Catalog core816B 청구·관측 출력 변경에만 owner census와 MT05 재선택. 완료한 물리 phase 시험 반복0 |

두 source의5회 실행·3개 고유 selector다. r1의 물리 시험과 r2의 Catalog 시험을 최종 source에서5개 실행한 것으로 합치지 않는다. MT05는 기존 fixture의 실제 lower-image flush/Pending/재검산 설치를 사용하며 새로운 동시성 알고리즘 인수가 아니다. native/새 PID 복구·crash cut·기존 조회/semantic codec·전체 workspace·하위 wrapper는 해당 실행 코드가 바뀌지 않아 반복하지 않았다. 기존 source별 FAIL/UNKNOWN/미실행과 DEV2·DEV3/S5 미완료를 유지한다.

#### source 관측과 남은 범위

| 대상 | source 값 상한(B) |
| --- | --- |
| 물리4-target stage / Pending / resume / install | 11260 / 6748 / 10244 / 6876 |
| 물리9-target stage / Pending / resume / install | 17180 / 8188 / 14884 / 8396 |
| 기존 물리 전체 gate4/9-target | 11888 / 20368 — 변경 없음 |
| Catalog preflight / stage / Pending / install | 17448 / 31940 / 20804 / 24324 — 원 core816B 포함 |
| Catalog 단일 operation 최대 | **31752→31940**. phase 구분과 누락 core 명시를 함께 적용했으므로 비용 절감으로 주장하지 않음 |
| 기존 owner census live peak / metadata / whole | 63184 / 186192 / 251728 — 변경 없음 |
| 기존 recovered census live / peak / metadata / whole | 64560 / 64611 / 106392 / 171928 — 변경 없음 |

위 값은 실제 Rust 타입·source 수명에 대한 보수 계산이며 RSS/ABI stack/처리 성능 측정이 아니다. 각 phase는 고정 예약의 근거이며 실행 도중 charge를 늘리고 줄이는 새 정책이 아니다. 단일 Catalog 비용과 기존 owner census를 전체 동시 합 인수로 표시하지 않는다. shared retained 한 번 + 모든 entry/실행/return의 합, resident reader·writer/progress class의 실제 admission과 Row/terminal/recovery·M4/M5는 여전히 미완료다. source 수명이 달라지거나 새 소비자가 phase 계산을 예약에 사용하면 관련 행을 재검토한다.

원 source export/inventory·manifest·runner·compiler diagnostics·ELF·stdout/stderr·result는 다음 위치에 보존한다.

- `artifacts/module-c2-commit-phases-20261027-r1/`: source `1517cbc95c9e4d8d605ce17eb606cbddca9006cf104bab848fd54180313af80b`, manifest `4fd06aedb0b2dad53eee528675304d41c89c7d04169207e1963ea867f89430b3`, ELF `f6f56be7b00be506e1c4fec108de6835b0bb7997f8afa41f164b00b20088937a`, result `831c347543e69d5deefa41bc94e02dcc2d5ab7c523292e12dd35c8eac2ec491b`.
- `artifacts/module-c2-commit-phases-20261027-r2/`: source `5aa4dcd24617c543bd5559c9b1c79a906fc15af844d012937428944e7cd9e1e8`, manifest `5a1269c714962ee36998abd776abd335515448b98247e59ae45ac333ab183084`, ELF `ec8213c47f8af00831217b5433c0b77267a230ca3939ac64c33bd8ddf93c6ed2`, result `97eb0f8e5ff620228316ca6c05a5b0cbc70cdcfd051f418f091221eeb802445e`.

총 실행 경과는 r1 33.11초/r2 32.53초이며 성능 인수가 아니다. 변경 Rust만 파일별 rustfmt로 정리하고 `git diff --check`를 사용했다. 전체 fmt/Clippy·자동 문서 검사·push는 실행하지 않았다. `/root/module_step_review`가 제품8파일·문서6파일·선택 manifest와 r1/r2 각5626개 source/inventory·manifest·runner·ELF·결과·로그 hash 및 현재 source 일치를 직접 대조하여 **D1/D2 최종 한정 수용·필수 수정0**으로 판정했다. core816B 보완, 원 r1 실행/검토 지적 보존, source별3+2회·고유3개와 무변경 물리 시험 반복0, 단일 source 비용과 전체 동시 admission 미완료의 구분을 확인했다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았고 `/root`가 이 한정 수용을 반영한다.


<a id="c2-shared-core"></a>
### C2 공통 core source 계수와 복구 반환 수명 — 2026-10-27

이번 “진행 위치 확인 후 다음 한 단계·OCR delegate 검토·commit” 요청은 `/home/et16/work/xadb`의 clean `main`·`df9881beb8d7d2455760c446551ff22accf4a562`에서 수행한다. main 수신 기준 M0–M3 완료, M4 Commit phase 계산까지 부분공급, M5 A/B 순수 본문 착수 상태였다. 별도 `codex/module-independence-finish`·`76f91e0`에는 후속 C2/C3와 Row 거래 ledger까지 미병합 변경이 있다. 그 worktree는 clean임을 읽기 확인했고 편집·병합·검증 결과 승계·다른 세션 중지는 하지 않았다. 이번 완료 범위는 main의 다음 source 계수 단위이며 별도 브랜치 전체 진행을 main 완료로 표시하지 않는다.

`D3-MODULE-SHARED-CORE-001` · D2 · **ACCEPTED / C2 한정 부분공급**. 구현·계약·통합 `/root`, 독립 검토 `/root/module_step_review`. 직전 단계의 원 core를 공통 보유분으로 옮길 의무를 수행한다. Catalog class에만 core를 두는 대안은 다른 class/전환에서 원 실체를 누락한다. 즉시 전체 admission을 연결하는 대안은 아직 큰 reader/writer의 동시 합과 progress 보장이 닫히지 않아 이번 한 단계에 넣지 않았다. §6.9와 기존64KiB 한도를 유지하며 원 owner·pin·WAL·wire 의미는 변경하지 않는다.

`concurrency::core_source_bytes()`가 원 inline slots·mutex·continuation을 포함한 실제 `PageConcurrency`816B를 정의한다. fresh `Plan4Pages`의 bind/live, `DiskSession`의 fresh/recovered conversion·retained, recovery의 bind_pages/bind_conversion/bind_return/into_live에 같은 실체를 한 번 포함한다. acquire/analysis/handoff는 core를 입력받지 않아 추가하지 않는다. Catalog nested phase에서816B를 빼고 기존 local Capacity는 core+class 합31940B로 유지한다. 독립 low-level Commit closure는 계속 core를 포함한다. 객체 이동·참조 공개 확대·새 owner/heap/byte budget은 없다.

#### 첫 실패와 실제 수명 수정

r1에서 memory source와 fresh/native type 일부는 통과했으나 native recovery bind_conversion이65563B로65536B를27B 초과했다. 원 FAIL은 보존한다. 원 `into_recovered_disk_session`의 replay cursor128B는 restore에만 필요했지만 큰 session 반환까지 같은 함수에 있었다. `restore_recovered_control`로 seed→control/jobs/inventory copy→population→allocator snapshot/equality→restored credit equality를 옮겨 cursor가 실제 helper 반환에서 끝나게 했다. restore phase에는5개 borrow와2개 Result를 추가 계수하고 return에서는 끝난 cursor만 제외했다. linear binding은 계속 보유하여 원 session으로 이동한다. 한도 증가나 근거 없는 상수 축소는 없다.

#### 선택과 실행 기록

[선택 manifest](../../tests/fixtures/dev3-module-c2/shared-core/manifest.json)는 최신 미실행분 선택이며 각 원 manifest는 해당 bundle에 보존했다. 실행 전 총480초·case30초·로그8MiB·test stack32MiB, B08의 첫 적격 padding 후보256/512/1024/2048/4096을 동결했다. r1 static 결과에서2048이 첫 setup 가능/Capacity 거절 후보였고 B08에 그대로 사용했다. native source 사례는 실제 Linux Writer/File **타입 계산**이며 native I/O 시험이 아니다.

| bundle | 실제 실행·판정 | 다음 선택 이유 |
| --- | --- | --- |
| r1 | production/test build PASS; owner·G5 memory/native·recovery memory4 PASS; recovery native1 FAIL(65563B); 나머지3 미실행 | helper 변경 뒤 owner/recovery 계산만 다시 선택. 완료한 G5 두 source 검산 반복0 |
| r2 | production/test build PASS; owner·recovery memory/native·fresh transfer4 PASS. recovery selector의 `tests::cases` 누락으로0개 실행을 runner가 FAIL 처리 | 제품 source 변경0. 같은 frozen source/ELF로 미실행 recovery/B08만 계속 |
| r3 | 정확 B04만1회 FAIL. `support.rs:260`의 fresh Reserve WAL과 고정 oracle byte 불일치이며 recovery/helper 미도달 | 원 B04 FAIL 유지. fixture 수정·B04 재실행 없음. 독립 D2 검토 뒤 현재 Profile2 MF09로 변경 helper 도달·복구 의무 검증 |
| r4 | 같은 r2 ELF의 MF09 실제1회 PASS. runner가 `CATALOG_MEMORY` 대신 `CATALOG` marker를 기대해 집계 FAIL | 원 result/log 보존. 별도 `adjudicate.py`가 exact1·exit0·MF09 BEGIN/PASS·source/ELF/input·로그 hash를 재대조하여 `PASS_LOG_VALIDATED_NOT_RERUN`. 제품 반복0 |
| r5 | 같은 r2 ELF의 미실행 B08만1회 PASS | exact B08 선택으로 다른15개 사례는 SKIP, PASS로 합치지 않음 |

최종 적용되는 검증은 r1의 G5 두 source 검산, r2의 owner/recovery memory/native/fresh4, r4 MF09, r5 B08의 **고유8개**다. 전체 실제 실행은12회(10 PASS·2 FAIL)이며 r2의0개 selector 오류는 별도다. 한 source에서8개를 새 실행한 결과로 합치지 않는다. B04의 early-live 거절 계약은 MF09 PASS로 대체하지 않는다. B04 setup 불일치는 이번 변경 전 경로의 미해결 시험 결함으로 남긴다. MF09는 actual durable unwind images를 고정 C04/N03 oracle로 대조하고 acquire/recover→bind→변경 restore→bounded cleanup→READY/into_live를 확인한다. 이 좁은 목적의 대체는 독립 검토자가 실행 전 수용했다.

#### 최종 source 관측과 한계

| 대상 | byte 상계 |
| --- | --- |
| 원 core / memory session shared retained | 816 / 21696(이전20880+816) |
| Catalog nested preflight / stage / Pending / install | 16632 / 31124 / 19988 / 23508; 원 core는 바깥에서 한 번 계수 |
| memory live peak / metadata / whole | 64000 / 187008 / 252544 |
| native live peak / metadata / whole | 64136 / 187144 / 252680 |
| memory recovered bind / peak / metadata / whole | 65299 / 65376 / 107208 / 172744 |
| native recovered bind / peak / metadata / whole | 65435 / 65512 / 107344 / 172880 |

이는 실제 source/type 수명 상계이며 RSS·컴파일러 stack·성능·동시 admission 결과가 아니다. native peak의 여유24B는 미등록 자원을 허용하는 예산이 아니다. 원 단일-operation max census와 전체 동시 합을 구별한다. runtime short admission·resident reader·writer/progress class, 실제 Row/terminal/recovery 및 M4/M5 전체는 main 기준 미완료다. 별도 미병합 브랜치 수신은 새로운 관련 diff 검토가 필요하다. 타입/수명이 바뀌거나 class 합을 admission에 사용할 때 다시 검토한다.

변경 Rust는 파일별 rustfmt, diff는 `git diff --check`로 확인한다. 전체 workspace/하위 wrapper·완료한 Commit/lookup 시험·native process inventory는 실행하지 않는다. 자동 문서 verifier·전체 fmt/Clippy·push는 요청 범위 밖으로 실행하지 않는다. 원 source별 PASS/FAIL/UNKNOWN과 DEV2·DEV3/S5 미완료는 유지한다.

- `artifacts/module-c2-shared-core-20261027-r1/`: source `e09df954d79070dd9823eb4d7e510c6f979a10769b97c28fb237da6d164bf7af`, ELF `4530abab5e4b58bd898cb8540946307e5083f0dc8bc215ab8a9bcc7997a91732`, manifest `7122d2fa98bf7516225fad521c2e17af629c03d6dbefe0d958cfadb42afa396e`, result `c13be3feb97f8230519377418148febc1f386d589b18eb0406250362e2b541c1`.

- `artifacts/module-c2-shared-core-20261027-r2/`: source `3da609e26656f22fa82d85fe2f2f9ff2344d4e953a1439122a418b0a29475b58`, ELF `7f996234fa42eaaa44a51a8e1e0f668f22a7bbfce1719b54d9b033188b34e965`, manifest `ff5cd226862554ce16be49c65dc84d9e23d6f0d08ddbbb1c8575cc02ac1ed9f6`, result `39776a2a228cac9f77f652d79a94422c9dae9b0fae33d8ae67f7d188742217ea`.

- `artifacts/module-c2-shared-core-20261027-r3/`: source `3da609e26656f22fa82d85fe2f2f9ff2344d4e953a1439122a418b0a29475b58`, ELF `7f996234fa42eaaa44a51a8e1e0f668f22a7bbfce1719b54d9b033188b34e965`, manifest `33868839c688fdd3324662525d51bcb84ec8b7e4abe48d74f1852657c29ad1f3`, result `f1065bca1908c66e785fe55f79f1cba00c3d53feb01db1d2a20cf75467e5569b`.

- `artifacts/module-c2-shared-core-20261027-r4/`: source `3da609e26656f22fa82d85fe2f2f9ff2344d4e953a1439122a418b0a29475b58`, ELF `7f996234fa42eaaa44a51a8e1e0f668f22a7bbfce1719b54d9b033188b34e965`, manifest `8f9ff39fffa6a8ddcf093166e1a3d5a46cf839deb7100e70ffa6b6d8cc8b3af0`, result `e7b343457d8ff6ac60e3dfd23f6f250fa979b1577fa74cb0820838f8f19a2a68`.

- `artifacts/module-c2-shared-core-20261027-r5/`: source `3da609e26656f22fa82d85fe2f2f9ff2344d4e953a1439122a418b0a29475b58`, ELF `7f996234fa42eaaa44a51a8e1e0f668f22a7bbfce1719b54d9b033188b34e965`, manifest `db23cc5f3f4ea4acd0d8087cb7a457f9f0bbf7cd6067d0cced3bc08c7c542eb2`, result `40c366813b947a04a60ecc889f5ee01d78ddcbb997a13aaf9b8abcc134afe0f8`.

r3–r5는 r2의 immutable source/ELF를 재사용하며 새 product build를 수행하지 않았다. runner·selection manifest·원 stdout/stderr·source inventory와 r4 adjudication을 bundle에 보존했다.

#### OCR delegate·독립 D1/D2 최종 검토

`/root/module_step_review`가 `ocr delegate preview --format json`과 `ocr delegate rule --format json`으로 목록·규칙을 확인하고 전체16파일을 검토했다. OCR 대상10개와 제외6개(문서5·fixture1)를 모두 읽어 `total_files=16`, `reviewed_files=16`, `skipped_files=0`, `coverage_rate=1.0`이다. 계약의 직전 Commit phase 문단이 core의 Catalog 청구를 현재 상태처럼 적은 경미한1건을 과거형과 후속 이행 참조로 정정했고 검토자가 재확인했다. 제품 변경0이므로 시험은 반복하지 않았다.

최종 판정은 **D1/D2 한정 수용·미해결 필수 수정0·차단0**이다. r1/r2 각5627파일 source inventory, manifest/runner/ELF/result/log hash와 현재 제품9파일의 r2 일치를 대조했다. 원 core의 단일 계수, helper 수명·검산 순서 보존,65563B FAIL→65512B 상계, 원 fixture/selector/marker FAIL, r4 로그 재판정과 source별 고유8개를 확인했다. B04 setup 결함·native I/O 미실행·전체 admission/M4/M5 미완료는 한정 수용으로 해소하지 않는다.

`artifacts/module-c2-shared-core-review-20261027/`에 `preview.json`, `rules.json`, `checklist.json`, `identity-audit.json`, `report.json`을 보존했다. 최종 report SHA-256은 `e9c60a58c2f103bde5ce976ade4e8a1d56cdc225e90a8c7b1007e835b5ffb52b`, checklist는 `ec8cce739c12ce2a6f9275dcae01d3cbb0b497e358da4fd303caf7899adc3e93`이다. 검토자는 제품/문서 수정·빌드·시험·자동 문서 verifier를 실행하지 않았고 `/root`가 실제 수용 결과를 이 증거·구현 색인·build JSON에 반영했다.


<a id="c2-short-source-main"></a>
### C2 full Catalog short 예약의 main 수신 — 2026-10-27

사용자의 “진행 위치 확인 후 다음 한 단계”와 후속 “OCR delegate 리뷰·수정·commit” 요청에 따라 clean `main`·`fd005ba33a2a0a56b63d91c259698ede2e6547f3`에서 시작했다(전체 기준 identity는 아래 manifest). M0–M3는 완료·채택, M4는 공통 core 계수와 복구 반환 수명까지, M5는 A/B 순수 제품 본문 착수까지 수신했다. 이번 한 단계는 **full Catalog 조회에 원 short source 예약을 연결하고 그 앞/뒤 수명을 Drain에 포함**하는 것이다. 별도 `codex/module-independence-finish`·`76f91e0`의 후속 전체 작업을 main 완료로 합치지 않는다.

`D3-MODULE-CATALOG-SHORT-MAIN-001` · D2 · **ACCEPTED / C2 한정 부분공급**. 구현·계약·통합 `/root`, 독립 검토 `/root/module_step_review`. 원 §6.9·65536B 한도·고정 charge를 적용한다. `ec018d6`의 full 조회 source ledger/시험과 `883c920`·`d9d6a01`의 반환/예약 시도 Drain 수정을 읽고 필요한 코드만 적응했다. 원 core816B를 누락한 당시 shared 계산은 main의 fresh/recovered 보유값 중 큰 값에 한 번 포함하도록 맞췄다. 참고 브랜치의 resident class·precheck API 분리·writer/Row 변경·문서/실행 증거는 가져오지 않았다. 기존 `try_read_operation`이 원 lock 안에서 검산과 표시를 수행하므로 같은 검사를 추가 분리하지 않는다. 다른 worktree 편집·병합·중지 없음.

#### 원 소유·수명과 대안

- 원 isolation header24..32에 동시 short-used를 보관하며32..64는0을 검산한다. private non-Clone ticket은 고정 class/charge를 보유한다. 바깥 owner·semaphore·heap·새 wire는 추가하지 않는다.
- 원 progress 진입 → source 예약 → transient refs/pin 순서다. 용량 초과는 refs/pin/저장 효과 전에 Capacity이고 진입 표시는 checked 반환한다. source 예약 시도 자체를 Drain에서 빼거나 refs 뒤에 예약하는 대안은 수명/진행 경계를 어기므로 제외했다.
- 출구는 refs 정리 → 최종 decode/Result → source 반환 → progress 표시 반환이다. 정상 반환/Drop은 자기 charge만 반환한다. poison·fence·underflow·잘못된 binding·panic은 원 fence와 미해결 charge를 보존하여 용량을 만들어내지 않는다.
- 같은 full class의 중첩 lookup은 용량에 따라 거절하고 원 snapshot으로 반환 뒤 재시도한다. writer/progress는 아직 이 ledger에 등록되지 않아 **전체 writer/reader·entry/return 동시 합 보장으로 승격하지 않는다**. resident class와 caller-retainable statement/Row factory도 후속이다.

#### 실제 변경과 실행 전 선택

당시 main 선택 manifest(`cbc09f2:tests/fixtures/dev3-module-c2/short-source/manifest.json`; [원본 보존 위치](#merge-main-20261028))와 각 bundle의 원 manifest를 구별한다. 최초 총600초·compiler90초·case30초·로그8MiB·test stack32MiB·각 selector1회로 동결했다. 새 ledger9개는 exact capacity·동시 실제 thread·overflow/header·underflow·poison/fence·unwind/forget·memory/native type·stale binding을 검사한다. 실제 full 조회8개는 두 번째 Capacity, 예약 전 Drain 정상/fence/unwind, refs0 뒤 Drain 정상/fence, Found 반환 정상/unwind를 검사한다. 기존4개는 변경 adapter의 Pending Undo LOAD·FlushRequired 재시도·fatal I/O cleanup·source census만 선택했다. 총21개다.

제품 format·recovery helper·물리 Commit 구현은 바뀌지 않아 완료한 core/recovery/Commit 사례·native process inventory·전체 workspace/하위 wrapper를 반복하지 않는다. native 항목은 실제 Linux Writer/File **타입 상계 계산**이며 native I/O 결과가 아니다. 기존 source별 PASS/FAIL/UNKNOWN과 DEV2·DEV3/S5 미완료를 유지한다.

#### r1 원 실패와 기대값 정정

r1은 production/test build PASS, 선택14개 PASS 뒤 Found 정상 반환에서 FAIL했다. 마지막 assertion의 events는1519→1548이며 일반 lookup의 WAL 읽기 이벤트를 포함한다. 원 resident-only 시험의 “조회 전체 이벤트0” 기대를 full 경로로 옮긴 오류다. assertion이 fixture mutex guard 수명 안에서 panic하여 정리 중 poison/abort까지 발생했고 원 stdout/stderr·exit·FAIL을 보존했다. 통과한 앞14개는 반복하지 않는다.

수정은 `source_return.rs`의 Found 정상/unwind 공통 시험에 한정한다. full lookup의 WAL 읽기는 허용하되 저장 이미지 전체가 전후 같고, final-return hook에서 refs0/pins0·progress1·charge>0을 관측한 **그 시점부터** checked 정리 완료까지 이벤트가 늘지 않아야 한다. 기대 이미지나 제품 코드는 바꾸지 않았다. 비교 전에 mutex guard를 끝내 잘못된 assertion이 fixture mutex를 poison하지 않게 했다. r2는 이 수정2개와 원 미실행5개만 선택하고 production build·완료14개는 반복하지 않는다. 이 정정과 독립 검토는 조회의 저장 효과 금지를 완화하지 않는다.

#### 최종 선택 결과와 source 상계

r1의 앞14개 PASS와 r2의7개 PASS를 해당 source의 검증으로 적용한다. 총22회 실행(21 PASS·1 FAIL), 고유21개이며 한 source에서21개를 다시 실행한 것으로 합치지 않는다. r2 test build는 PASS이며 제품 source가 같아 production check는 반복하지 않았다. r1/r2 경과는38.98초/34.89초로 성능 인수 수치가 아니다.

| source 계산 | memory / native 타입(B) |
| --- | --- |
| fresh/recovered 중 큰 shared retained, 원 core816 포함 |23072 / 23208|
| full lookup 고정 charge |41576 / 41576|
| shared + full lookup |64648 / 64784 ≤65536|
| source ledger leaf envelope |1024 / 1024|

charge는41552→41576B이며 실제 ticket/carrier를 포함한다. 기존 memory live peak64000·metadata187008·whole252544, recovered peak65376는 그대로다. 이는 source/type 수명의 보수 상계이며 RSS/ABI stack·성능·전체 동시 admission 측정이 아니다. Capacity 거절 시 이전 charge/refs/pin/image를 보존하고 첫 조회가 반환하면 같은 snapshot으로 재시도하는 실제 시험을 통과했다.

수정 Rust는 파일별 rustfmt와 `git diff --check`를 적용했다. 자동 문서 verifier·전체 fmt/Clippy·push는 실행하지 않았으며 push 시 기존 훅이 적용된다. 새 문서는 없고 기존 계획·계약·증거/구현 색인과 build JSON을 갱신했다.


- `artifacts/module-c2-short-source-main-20261027-r1/`: source `8cf772e035326b4bf16249c9f3ad329f2590a87f70637dc71b10951d87319b2b`, manifest `6863665f3f8b111f8636c0380d6c6e3690d9fdbdc1500c6b740edda3526b1684`, ELF `393e788970a90d1f6b1f35ecfb4fad44bf3551695adc6f04bf4b1dc1bdd1e14a`, result `fe9ae0cbbc23806a2c9adea87c3bcd5deecdc644de7535e20c481855d783a861`.

- `artifacts/module-c2-short-source-main-20261027-r2/`: source `4bf708211c73e0947113e6b0a991354f5a51d39f781044bda03381270ae5187c`, manifest `dbd516d04c23665fca57fb8aeffd786b176d6e46f51b4ca49bd7e929dfc95cb4`, ELF `07b8a942955be4765746de2da034d1399a6f85f2b2ced1d49281ba2e0489e2ff`, result `1460e3038a1a8e7c3f400c7b44d6ebdff473038f79abbd5a830a055a5d73f3ed`.

각 bundle에 immutable source·inventory·선택 manifest·runner·compiler diagnostics·ELF·원 stdout/stderr·result를 보존한다.

#### OCR delegate 최종 독립 검토

`/root/module_step_review`가 `ocr delegate preview --format json`·`ocr delegate rule --format json`으로 파일·규칙을 확인하고 OCR 대상12개와 제외 Markdown6개/fixture1개를 모두 검토했다. `total_files=19`, `reviewed_files=19`, `skipped_files=0`, `coverage_rate=1.0`이다. r1/r2 각각5632개 source inventory와 source/manifest/runner/ELF/result/compiler/stdout/stderr identity를 독립 계산했고, r1→r2의 실제 변경은 시험1개·선택 manifest1개이며 현재 source는 r2와 같음을 확인했다.

최종 **독립 D1/D2 한정 수용·미해결 필수 수정0·차단0**이다. 원 core의 한 번 계수, 예약 시도부터 final decode/checked source 반환까지의 Drain 수명, 용량 거절·원 fence/미해결 charge 보존, Found 기대값 정정·원 FAIL/abort 및 source별14+7의 구분을 수용했다. 앞선 byte admission 미공급 문구는 당시 상태와 이번 full 조회 부분공급으로 구별하도록 정리했고 최종 검토가 이 문구를 포함한다. 전체 writer/progress/entry 합·resident/Row·M4/M5 미완료는 해소하지 않는다.

`artifacts/module-c2-short-source-main-review-20261027/`에 preview/rules/checklist/identity-audit/report를 보존했다. report SHA-256은 `64d864f446e348d8329b75477884619375c2d326cb7de8a9de18574de6349ae5`, checklist는 `1f74bba07dd4093bdfa8b0a1f7f733ec89f2bca1acfbcd867abcab821d28ad22`이다. 검토자는 제품/문서 수정·빌드·시험·자동 문서 검사를 실행하지 않았고 `/root`가 이 한정 수용을 반영한다.


<a id="c2-resident-main"></a>
### C2 상주 Catalog 고정 class의 main 수신 — 2026-10-27

사용자의 “진행 위치 확인 후 다음 한 단계·OCR delegate 리뷰 및 수정·commit” 요청으로 clean `main`·`a7fe5e36748e7b900963f31d839db9d03a6c1556`에서 시작했다. M0–M3는 완료·채택, M4는 full Catalog short 예약과 원 Drain 수명까지, M5는 A/B 순수 본문 착수까지 수신한 상태였다. 이번 종료 범위는 **상주 Catalog 조회의 고정 source class와 그 진입/반환 수명**이다. 별도 `codex/module-independence-finish`·`76f91e0`의 전체 C2/C3·Row 작업을 main 완료로 합치지 않는다. 해당 브랜치의 `788702c`를 읽고 필요한 제품/시험 코드만 현재 main에 적응했다. 다른 worktree 수정·병합·세션 중지·서비스/호스트 재시작은 없다.

`D3-MODULE-CATALOG-RESIDENT-MAIN-001` · D2 · **ACCEPTED / C2 한정 부분공급·선택 검증 완료**. 구현·계약·통합 `/root`, 독립 검토 `/root/module_step_review`. 사용자 요청과 DP01의 기존 위임, [§6.9](../contracts/dev3-module-boundaries.md#operation-source-admission)의 class 고정·65536B 한도를 적용한다. 원 계획/형식·격리·복구 불변식을 대체하지 않는 C2 구체화다. 참고 commit의 과거 시험/검토 결과는 이번 main의 실행 증거로 승계하지 않는다.

#### 선택과 원 소유·수명

- 새 동기 `lookup_catalog_resident`는 기존 Catalog 제품 본문을 호출하며 private ticket의 class만 고정한다. 원 allocator를 기존425984B caller loan에 복사하고, 전체 할당 Status의 exact reference 유일성과 Undo 위치를 원 guard로 확인한다. 새 page backing·directory·authority·crate/dependency를 만들지 않는다.
- WAL replay·durability wait·reader file·LOAD에 진입하지 않는다. 필요한 페이지의 비상주 또는 정상 allocator stamp 변화는 refs/pin/charge를 정리한 뒤 Busy다. 같은 snapshot으로 Full을 새로 호출할 수 있으나 살아 있는 operation의 charge를 늘리지 않는다. stable source에서 exact-ref 부재/중복·손상은 Fatal/fence와 미해결 charge를 보존한다.
- main의 progress 진입 → source 예약 → refs/pin과 refs 정리 → decode/Result → source 반환 → progress 반환 순서를 유지한다. 참고 commit의 오래된 예약 전 progress 검사/예약 뒤 admission 순서는 가져오지 않았다. 원 core816B도 main의 shared retained 한 곳에 유지한다.
- Full만 유지하는 대안은 동시 읽기의 유한 작은 class를 공급하지 못한다. 상주 조회 중 자동 Full 전환은 고정 예약과 정리 경계를 어기므로 제외한다. writer/progress 전체 예약·caller-retainable statement/Row factory는 이번 범위 밖이다. class가 I/O로 확대되거나 source 수명/소유권이 바뀌면 고정 charge와 해당 경계를 재검토한다.

#### 실제 diff와 실행 전 선택

당시 main 선택 manifest(`cbc09f2:tests/fixtures/dev3-module-c2/resident-read/manifest.json`; [원본 보존 위치](#merge-main-20261028))에 baseline·참고 commit·Rust15파일과 각 exact selector의 변경 관계를 고정했다. 총600초·compiler90초·case30초·로그8MiB·test stack32MiB·각 selector1회다. production check와 test `--no-run`은 각각 빌드 검증이며 runtime과 구분한다.

| 선택 | 변경 계약·정답 근거 |
| --- | --- |
| resident 신규9개 | 고정 class/혼합 Capacity·Memory/Linux 타입 상계, actual Active/Commit 전후 snapshot과 literal T/i/Int64, 비상주 Status/후속 Status/Undo의 Busy 및 같은 snapshot Full 재시도, 두 실제 thread의 독립 charge 반환, checksum/부재/중복/선택 뒤 membership 손상 Fatal, allocator flush의 정상 stamp 변화 |
| resident 수명 신규6개 | source 예약 전 Drain 및 fence 실패, refs0 뒤 Drain 정상/fence, Found 최종 decode 뒤 정상/unwind. 원 hook은 관측만 하며 권한을 발급하지 않음 |
| 영향 Full6개 | 변경 class selector/source 계산, 공통 begin_class와 시험 mode helper의 Full 진입·Found 반환, Pending Undo LOAD, FlushRequired 정리/재시도, fatal I/O 원인 유지 |

제품 WAL/Commit/recovery 구현과 wire는 바뀌지 않아 완료한 물리 Commit·복구/native process inventory·무변경 ledger 실패 suite·전체 workspace/하위 wrapper는 반복하지 않았다. Linux 항목은 **실제 타입 상계**이며 native I/O 실행이 아니다. 과거 source별 PASS/FAIL/UNKNOWN과 DEV2·DEV3/S5 미완료를 유지한다.

#### 실행 결과와 source 상계

`r1`에서 production/test build PASS, 선택 runtime **21개 모두 각1회 PASS**, 실패/중단0이다. 변경하지 않은 코드의 완료 시험을 통합 단계라는 이유로 반복하지 않았다. 경과62.55초는 검증 실행 시간이며 성능 인수/개선 수치가 아니다.

| source 계산 | memory / Linux 타입(B) |
| --- | --- |
| fresh/recovered 중 큰 shared retained, 원 core816 포함 |23072 / 23208|
| Resident 고정 charge |16860 / 16860|
| shared + Resident 두 개 |56792 / 56928 ≤65536|
| Full 고정 charge |41576 / 41576|
| shared + Full 하나 |64648 / 64784 ≤65536|

begin/begin_resident → begin_class의 class 값과 반환 carrier를 entry source에 추가했고, entry/실제 codec·관측·반환의 배타 phase만 max로 계수한다. 위 합은 source/type 수명의 보수 상계이며 RSS·ABI stack·성능·전체 writer/reader/entry 동시 합의 실측이 아니다. Memory에서 Full+Resident는81508B이며 `shared+resident+full>65536`을 actual 타입 계산과 원 ledger Capacity/보유값 불변 시험으로 확인했다.

수정 Rust15파일에 파일별 rustfmt를 적용하고 `git diff --check`를 통과했다. 자동 문서 verifier·전체 fmt/Clippy·push는 실행하지 않았으며 push 시 기존 훅을 적용한다. 신규 문서 없이 기존 계획·계약·증거/구현 색인과 build JSON의 실제 경로를 함께 갱신한다.

원 source export/inventory·manifest·runner·compiler diagnostics·ELF·각 stdout/stderr·result는 `artifacts/module-c2-resident-main-20261027-r1/`에 보존한다.

- source SHA-256: `f12407da9a9f5731d7596eeb454028408ec4f7f46a40f682eda5ef42112841db`
- manifest SHA-256: `9bcec2862cc9b79b6930b9d6c7ed5862518b64ea1fa5c653dcc87f374cb154d4`
- runner SHA-256: `16fa11cd9551cc3f783fac56fd1197837e52a56abf823b6a2d5bbe1b9b3c5913`
- source inventory SHA-256: `4c1045bd86481c92f0dbb3f169854e64f8da3a38e997227fd3067bae71dcc4f9`
- ELF SHA-256: `02386d9523c3ac1e3ff43f734137dea9f3a5ebd4ba7b56c4ae5fc1035431d74b`
- result SHA-256: `c4744db11c8ae50cdab47dd88c20b8110786ff75dbb8a1fc00bd95d22fd4d90e`

#### OCR delegate 독립 검토

`/root/module_step_review`가 `ocr delegate preview --format json`·`ocr delegate rule --format json`으로 대상/규칙을 확인했다. OCR 대상16개와 기본 제외7개도 직접 읽어 **전체23파일 검토·누락0·100%**이며 최종 **독립 D1/D2 한정 수용·미해결 필수 수정0·차단0**이다. 원 source5636파일·source/manifest/runner/ELF/result·compiler2 및 runtime21개 로그 identity와 `--exact` 1개 PASS, Catalog BEGIN/PASS 일치를 확인했다. 현재 code/fixture와 frozen export 차이0이며 검토자는 제품·문서 수정, 빌드·시험·문서 verifier를 실행하지 않았다. `/root`가 이 한정 수용을 반영했다.

리뷰 과정에서는 원 full 부분공급 시점의 “resident 미공급”을 현재 상태와 혼동하지 않도록 계약 문구를 정리했다. 제품 수정이 필요한 결함은 발견되지 않았다. 검토자의 identity 감사 첫 시도는 Path 대신 문자열 정렬을 사용해 fingerprint assertion이 실패했고, 기존 원 도구의 Path 정렬로 바로잡아 일치를 확인했다. 이 검토 스크립트 오류는 제품/시험 FAIL이 아니며 원 r1 결과를 재실행하거나 바꾸지 않았다.

`artifacts/module-c2-resident-main-review-20261027/`에 preview/rules/checklist/identity-audit/report를 보존했다. report SHA-256은 `71226df819b56585b4f7e7151a344a83e7ad026fbd46eefb36daa0573f0a301a`, checklist는 `d94dd99914caa48711f6504f1fd60b1882b5e47915a018fbfc33971d63ed2211`이다. 이번 부분공급은 writer/progress·전체 entry/return 동시 합, actual C+A/C+B 및 합류·MI01–06·M4/M5 미완료를 해소하지 않는다.


<a id="c2-pending-image-main"></a>
### C2 Commit 준비 자료의 Pending 수명 종료 — 2026-10-27

아래는 `25ff5e9` 초기 저장 당시의 기록이다. 남은 native N03의 후속 결과는 [별도 실행 기록](#c2-pending-image-native)으로 구별한다.

사용자의 “진행 위치 확인 후 다음 한 단계·OCR delegate 리뷰 및 수정·commit” 요청으로 clean `main`·`8b47533`에서 시작했다. M0–M3 완료, M4 full/상주 Catalog 조회 예약, M5 A/B 순수 본문 착수까지 수신한 상태였다. 이번 한 단계는 **Commit의 stage가 준비 자료를 소비하고 설치에 필요한 불변 자료만 Pending에 넘기는 경계**다. 별도 브랜치의 `a5cfdae5fa00e903c4e198a8a75bbb8005383e77`에서 필요한 제품 코드만 적응하며 main의 기존4-phase 계산·원 core 단일 계수를 유지한다. 다른 worktree나 원 증거를 변경하지 않는다.

`D3-MODULE-PENDING-IMAGE-MAIN-001` · D2 · **구현·선택 memory 검증 완료 / 독립 D1/D2 한정 수용 / native N03 NOT_RUN·단위 전체 인수 미완료**. 구현·계약·통합 `/root`, 독립 검토 `/root/module_step_review`. 원 §6.9의 고정 source 회계를 위한 선행 단위이며 전체 writer/progress admission을 공급하지 않는다. 새 형식·의존성·backing·권한은 추가하지 않는다.

#### 원 수명과 선택

`stage_catalog`는 `PreparedCatalog`를 소유로 받아 원 stage/mark_staged/suspend를 마친 뒤 private `CatalogCommitImage`로 소비한다. 준비용 requests/stamps/roles9·before borrow는 helper 반환 전에 끝난다. 불변 payload와 canonical physical order의 after reference4만 Pending/설치에 남으며 proof-only Allocation은 None이다. 원 pin·Staged/Durable/Installed receipt·control cut·exact requirement·WAL-before-page·전체 설치와 epoch/Q 공개 순서는 유지한다. kind/target4 검사는 Intent 전이며 image 추출은 실패할 수 없어 stage 이후 재시도 분기를 만들지 않는다.

기존 Prepared 전체를 유지하는 대안은 준비용 배열을 WAL 대기까지 보유한다. byte 비용만 줄여 표시하거나 별도 heap/owner로 옮기는 대안은 실제 수명과 회계를 어긋나게 하므로 채택하지 않았다. 공유 `catalog_apply`는 같은 payload를 읽는 한 본문을 유지하며 Forward/CLR/Cancel에서도 같은 필드를 소비한다. stage 입력·image·반환 tuple·추출 reference 배열의 공존은 모두 합산하고, 실제 helper 경계 뒤 Pending/install에서만 Prepared를 image로 바꾼다. payload가 변경 가능해지거나 authority/실제 source 수명이 달라지면 재검토한다.

#### 변경별 선택과 실제 결과

당시 main 선택 manifest(`cbc09f2:tests/fixtures/dev3-module-c2/pending-image/manifest.json`; [원본 보존 위치](#merge-main-20261028))에 변경 Rust3파일·selector별 이유·기준/참고 commit을 실행 전에 고정했다. 총600초·compiler90초·case30초·로그8MiB·test stack32MiB·각 selector1회다. production check와 test `--no-run`은 각각 PASS이며 runtime과 구별한다.

- 신규 source 시험1개: compact image 크기, Memory/Linux 실제 타입의 공유값+Commit phase 및 live/recovery envelope 검산.
- 변경 payload의 MC01 reserve/cancel·MC02 Forward/CLR/abort2개; Commit의 MT05 Pending flush·MT03 Undo LOAD·MT07 Drain fence·MT01 Q 공개4개.
- MF02 stage unwind·MF03 write-before-error·MF09 durable unwind·MF10 second-page unwind4개. MF09/10은 같은 build의 memory 복구 정답까지 검산한다.

위 **runtime11개 각1회 PASS**, 실패/중단0이다. native 타입 계산은 실제 파일 I/O 실행이 아니다. 별도 native N03(실제 Commit sync 성공 뒤 오류·UNKNOWN·복구)은 원 controller의30초·1회와 사전 독립 admission을 계획했으나 **free-floor64,424,509,440B에 대해 실제 여유365,015,040B로 실행 전 중지·NOT_RUN**이다. worker를 만들지 않았고 기준을 줄이지 않았다. 이번 실행에서 생성한 `target/debug/incremental` 캐시만 정리했으며 source·ELF·로그·결과는 보존했다. N03의 새 실행은 원 자료와 독립 admission을 갖춘 후 여유 기준을 충족할 때만 진행한다.

기존 무변경 physical phase·ledger·reader suite, 전체 workspace/하위 wrapper·native N01/N02는 반복하지 않았다. 과거 `catalog_source_native`의 오래된 visibility backing8192 assertion은 이번 검증으로 통과 처리하지 않는다. 새 source 시험은 실제 live/recovery envelope와 공유값+phase만 검증하며 기존 `resources::fits`·fresh binding·visibility 전부의 새 실행을 주장하지 않는다. 원 source별 PASS/FAIL/UNKNOWN 및 DEV2·DEV3/S5·M4/M5 미완료를 유지한다.

#### 실제 source 상계

| 항목 | 이전 main → 이번 source(B) |
| --- | --- |
| 준비 값 → Pending image |2024 →568 (1456 감소)|
| Commit preflight |16632 →16632|
| Commit stage |31124 →32292|
| Commit Pending |19988 →18532|
| Commit install |23508 →22052|
| Commit phase peak |31124 →32292|

새 image/tuple이 전환 중 공존하므로 **전체 Commit peak는1168B 증가**한다. Pending의 국소 감소를 전체 자원 절감으로 표현하지 않는다. Memory/Linux shared retained는23072/23208B, live peak64000/64136B, recovered peak65376/65512B로 기존 envelope 안이다. source/type 수명 상계이며 stack/RSS·성능·동시 admission 측정이 아니다. writer1+resident reader2의 전체 합과 entry/return/progress 공급은 후속이다.

변경 Rust3파일의 파일별 rustfmt와 `git diff --check`를 적용했다. 자동 문서 verifier·전체 fmt/Clippy·push는 실행하지 않으며 push 시 기존 훅을 적용한다. 새 문서 없이 기존 계약/계획/구현/증거 색인과 build JSON의 실제 경로를 갱신한다.

원 immutable source·inventory·manifest·runner·compiler diagnostics·ELF·각 stdout/stderr·result와 native 미실행 관측은 `artifacts/module-c2-pending-image-main-20261027/r1/`에 보존한다.

- source_sha256: `7903ea92922bfc2c25638990320bf520387522be29fec95e6c282b28c5e2288f`
- manifest_sha256: `c1fa5969abf9826a0b065edb66e973f340fd0f03db35347284197ba7b47276d6`
- runner_sha256: `1e8121dd9ee9d570d7f0ec74a960ba3dc26415cb92ebb0d9a1c12519d7da0dd8`
- inventory_sha256: `76e5b6043c4df65c51065bcd571b7d902bed33886f3a22ad9e97b60e212b587b`
- binary_sha256: `988fb5ba15810ddd6bebaaca98231243949a0856883748acd190034f70ddaad6`
- result SHA-256: `7741d2853a7876bf7967b9897f3102625388710742e09d89112aef742a1320fb`
- native-preflight SHA-256: `249474dad5457126bc98927ba626755215f2e41c395704a621928ae0303d5d4d`


`result.json`의 PASS는 build/선택 runtime11개에 한정한다. native NOT_RUN을 포함한 `verification.json`의 전체 판정은 `INCOMPLETE_NATIVE_NOT_RUN`이다.

#### OCR delegate 독립 검토

`/root/module_step_review`가 `ocr delegate preview --format json`·`ocr delegate rule --format json`으로 대상/규칙을 확인했다. OCR 대상4개와 기본 제외7개도 직접 읽어 **전체11파일 검토·누락0·100%**이며 **독립 D1/D2는 구현·memory 검증 저장 범위로 한정 수용·미해결 필수 코드 수정0·차단0**이다. consuming 경계·원 권한·canonical after 순서·공통 payload 적용·stage 공존값·Pending/install 차등 계수 및64KiB envelope를 수용했다. 코드 수정이 필요한 지적은 없었고 `/root`가 리뷰 수신과 native 잔여를 문서에 반영한다.

원 source5637파일·current/export 동일, source/manifest/runner/ELF·compiler2·exact11 각각1회 PASS·MF09/10 memory 복구, native preflight/verification identity를 독립 검산했다. 검토자는 제품/문서 수정·빌드·시험·자동 문서 verifier를 실행하지 않았다. **native N03 필수 잔여와 전체 `INCOMPLETE_NATIVE_NOT_RUN`을 유지하므로 이번 단위 전체 인수 완료를 뜻하지 않는다.** M4/M5도 미완료다.

`artifacts/module-c2-pending-image-main-review-20261027/`에 preview/rules/checklist/identity-audit/report를 보존했다. report SHA-256은 `4d012de12c3356fed94b6093abce79a20d826f7ca147a807e45ea09b65498e57`, checklist는 `760272ceff7a403ce603ee86ee6966e28db5311da96eecd720d829ad6791c946`, identity audit는 `6578a7f557e33fab15e9fc478aa09b728a98f84bb7c3afc9226911d4ddc55cc1`이다.


<a id="c2-pending-image-native"></a>
### C2 Commit 준비 수명 분리의 남은 native N03 — 2026-10-27

사용자의 후속 “디스크 지웠음 진행해” 지시에 따라 clean `main`·`25ff5e9`에서 앞 단위의 미실행 N03만 진행했다. 제품·fixture·controller 변경0이며 완료한 build2/runtime11·native N01/N02·전체 workspace/하위 wrapper는 반복하지 않는다. [앞선 저장 기록](#c2-pending-image-main)의 source별 PASS·원 디스크 미달 NOT_RUN·`INCOMPLETE_NATIVE_NOT_RUN` 파일을 그대로 보존한다. 이번 결과는 그 뒤의 새 실행이며 앞선 기록을 소급 PASS로 바꾸지 않는다.

`D3-MODULE-PENDING-IMAGE-MAIN-001`의 후속 검증 · **native N03 1회 PASS / 독립 D1/D2 수용·이 C2 준비 수명 단위의 선정 검증 인수 완료**. source `7903ea92922bfc2c25638990320bf520387522be29fec95e6c282b28c5e2288f`와 ELF `988fb5ba15810ddd6bebaaca98231243949a0856883748acd190034f70ddaad6`는 원 r1 및 현재 제품과 같다. 재빌드 없이 원 source/ELF를 `artifacts/module-c2-pending-image-main-20261027/native-r1/`로 복사했고 전체 source6156파일(문서 포함)·기존 fingerprint5637파일을 구별하여 검산했다.

#### 실행 전 동결과 독립 admission

원 `scripts/dev3_catalog_native.py`·input/oracle·N03 일정과 한도를 유지했다. 실제 ext4에서 계획 시 여유76,215,279,616B, 독립 재확인76,206,051,328B가 기존60GiB+예약분64,613,253,120B 이상이었다. 한 사례1회·전체30초·worker29초·마지막 audit1초·로그8MiB·trace4MiB·DB32MiB·snapshot128MiB·artifact1GiB·주소공간1GiB·File3/FD6을 그대로 적용한다. `RUST_MIN_STACK=33554432`는 원 선택 manifest와 같다. 실패 시 재반복하지 않는 계획을 `run-plan.json`에 고정했다.

`/root/module_step_review`가 source/ELF·원 실행 증거·controller/계획·공간을 독립 확인한 뒤 admission을 수용했다. 같은 ELF에서 이미 통과한 변경 source envelope를 수신하고, fits 관련9경로가 `8b47533`과 같으며 원 생성/복구 경로의 admission 호출이 유지됨을 정적으로 확인했다. `catalog-source-admission/result.json`의 PASS는 이 **독립 admission 판정**이며 source-native prerequisite suite를 새로 실행한 결과가 아니다. 그 구분과 원8192 assertion 관련 미실행 설명을 유지한다.

#### 실행 결과

- produce PID330248: 실제 Commit WAL sync 성공 뒤 주입된 오류를 `CallerMustResolve / IoWriteUnknown`으로 반환했다. `A_CREATE_UNKNOWN_AFTER_SYNC`에서 ACK 없이 UNKNOWN1개를 관측한 뒤 controller가 이 시험 프로세스만 SIGKILL했고 exit−9를 회수했다.
- recover PID330337: 같은 ELF의 새 프로세스에서 LSN9를 capture하여 epoch2로 복구하고 LSN15까지 진행했다. 원 독립 oracle가 WAL·home·Base/CURRENT·원 WAL unlink/dirsync 순서를 검사했고 query marker는 `6 / 1 / true`, 종료는0이다. handles0·File peak3·FD peak6을 확인했다.
- 전체 **12.352271673초, N03 PASS**다. 원30초·1초 final audit와 모든 공간/자원 한도를 충족했다. 종료 후 여유76,129,370,112B이며 두 worker는 모두 회수됐다. 이 시간은 기능 검증 경과이며 성능 인수 수치가 아니다.

기존 r1의 build2/runtime11 PASS와 이번 N03 PASS를 같은 제품 source/ELF의 서로 다른 실행 증거로 연결한다. native N03의 잔여 실행을 해소했고 독립 D1/D2가 이 C2 준비 수명 단위의 선정 검증 인수를 수용했다. 전체 writer/progress·동시 entry/return 합, actual C+A/C+B/MI01–06·M4/M5·DEV2·DEV3/S5 인수는 여전히 미완료다.

원 source/ELF·admission·계획·effects/entropy/identity·WAL/DB snapshot·stdout/stderr·worker/reaped receipt·result는 위 native-r1 bundle에 보존한다.

- `source-files.json` SHA-256: `8df0ca0e4f954203b97dd6850cfcedd0d63a6c950705919a71abd040956e9051`
- `run-plan.json` SHA-256: `893f3ba213f91d1a07955dfbc52941a39d7d0776ba33f977c3adc1404b8993de`
- `admission.json` SHA-256: `2b74bc75165638e241e839a40dd924f3c53f29744e346e9c014825252a21825f`
- `catalog-source-admission/audit.json` SHA-256: `b208f915e4ef5aec286b24e9ec6a1de308cae1ea5c8784a65afa241a06ada1a2`
- `catalog-source-admission/result.json` SHA-256: `64c8a3dd186f1d20a6a0b729e1ebc77bcc958209d49a89d60edcb646773fab68`
- `catalog-native/N03/result.json` SHA-256: `6669371adcc52b54efb1dc698748a664f057b44a81a217171b20e55b1d42240d`
- `native-output-inventory.json` SHA-256: `4ef931b518d14711ae608f66d9701eb17e6ec43cf089d99e5dcb528ca742542e`

문서·상태만 갱신하므로 새 제품 시험·빌드·자동 문서 verifier를 실행하지 않는다.

#### OCR delegate 후속 독립 검토

`/root/module_step_review`가 `ocr delegate preview --format json`·`ocr delegate rule --format json`으로 목록/규칙을 확인하고 대상1개·기본 제외6개를 모두 읽었다. **전체7파일 검토·누락0·100%·필수 수정0·차단0**이며 **독립 D1/D2는 이 C2 준비 수명 단위의 선정 검증 인수까지 수용**했다. 초기 NOT_RUN/미완료의 문구는 과거형과 후속 링크로 구분하여 수신했다.

전체 source6156파일·fingerprint5637파일·ELF/admission 및 native 출력67파일 identity를 대조했고, 원 oracle로 저장된 WAL9/15·home·Base/CURRENT capture9 epoch2·descriptor/inventory/provenance·원 WAL unlink/dirsync를 읽기 전용 재검산했다. 실제 sync 성공→AfterError→UNKNOWN/ACK0·지정cut kill−9·회수와 별도 PID의 복구/query·handles0/File3/FD6, 원 NOT_RUN 보존을 확인했다. 검토자는 제품·worker·native controller·빌드·시험·문서 verifier를 실행하지 않았다.

`artifacts/module-c2-pending-image-native-review-20261027/`에 preview/rules/checklist/identity-result-audit/report를 보존했다. report SHA-256은 `af99decd42ba0b4e8a7a41779985e44d85a88e2fd9c15bd7f67ec995f77fc20d`, checklist는 `44f3f18145478451c99e844a14ca9373107fcdf18f1cbbde6a21d70b3bcfaad4`, audit는 `e59b1abb28bfed8978555ba17f5dfc7bc814d44737d414b55344c736b9bf0fb2`이다. 새 `native-r1/verification-completion.json`은 원 r1 결과를 변경하지 않고 이번 단위 수용을 연결한다. 전체 M4/M5는 계속 미완료다.

- `verification-completion.json` SHA-256: `c215041079addad433fad2d64e7294457e58f3fb7badf93cd5a0c0d9375bd2aa`


<a id="c2-aging-source-main"></a>
### C2 Aging/Root 실제 source 계산의 main 수신 — 2026-10-27–28

사용자의 “진행 위치 확인 후 다음 한 단계·OCR delegate 리뷰 및 수정·commit” 요청으로 clean `main`·`4ed0cf979bebfe86d6663c84719fea52d1841538`에서 시작했다. M0–M3 완료, M4 full/상주 Catalog 예약과 Commit 준비 수명 분리의 native N03까지 수신했으며 M5는 A/B 순수 본문 착수 상태였다. 이번 한 단계는 **writer/progress 예약의 선행인 Aging 조회 source 회계 구체화**다. 별도 브랜치 `c4b9b6f5946722109f9660259a3d9389168ea53b`의 제품5파일만 적응하고 main의 원 core816B 단일 계수·Commit4phase를 유지했다. 별도 worktree·원 결과를 변경하거나 해당 브랜치 전체를 병합하지 않았다. 실행 bundle 날짜는 착수일20261027이며 정리·리뷰는2026-10-28 KST다.

`D3-MODULE-AGING-SOURCE-MAIN-001` · D2 · **ACCEPTED / 구현·선택 검증 완료 / 독립 D1/D2 한정 수용**. 작성·계약·통합 `/root`, 별도 검토 `/root/module_step_review`. §6.9의 고정 source 회계를 구체화하며 제품 codec/저장 효과·영속 형식·권한·backing·의존성은 변경하지 않는다. 전체 writer/progress admission·M4/M5 인수는 미완료다.

#### 계산 경계와 선택

기존 Aging 계산은 실제 Root2 decode 대신 전체 Job queue census를 포함했다. 실제 encode의 Root2 decode→검산→work hash→XOD encode와 decode의 Root2 decode→XOD decode·검산→work hash를 분리하고, 순차 helper의 임시값에만 max를 적용한다. 살아 있는 parent·Result·반환값은 밖에서 합산한다. initial Root의 canonical512+initial Root512+initial Job1024, 후속 budget rows640, malformed Transaction XOD가 거치는 XTS decode와 고정 SHA closure를 포함한다. XOD/XTS 공통 식은 각 lower owner로 옮기고 XOD validate의 by-value budget5개 IntoIter·현재 row를 보완했다.

전체 queue 비용을 계속 쓰는 대안은 실제 Aging 호출 범위와 맞지 않는다. 성공 경로만 세거나 희망하는 수치에 맞춰 한도를 키우는 대안도 채택하지 않는다. 참조/반환값·codec 호출 관계가 바뀌면 source 식과 소비자의 한도를 다시 검토한다. 이번 감소는 source/type 수명의 보수 상계 변화이며 RSS·stack·성능 개선 측정이 아니다.

| source 항목 | main 적용값(B) |
| --- | --- |
| Root2 decoder / 기존 전체 queue |6540 / 6736|
| XOD / XTS / Aging |8711 / 1411 / 11575|
| Full 조회 고정 charge |41576 →37724 (3852 감소)|
| Resident 고정 charge |16860 (유지)|
| shared retained (memory / Linux 타입) |23072 / 23208 (원 core816 포함·유지)|
| shared + Full (memory / Linux 타입) |60796 / 60932|
| shared + Resident2 (memory / Linux 타입) |56792 / 56928 (유지)|
| Commit preflight / stage / Pending / install |16632 / 32292 / 18532 / 22052 (유지)|
| live peak (memory / Linux 타입) |64000 / 64136 (유지)|
| recovered peak (memory / Linux 타입) |65376 / 65512 (유지)|

공통 XOD 계산 보완으로 transaction inventory/encode/next/validate는 각각2224B 증가하고 Aging predecessor/next는14853/15675B다. Full의 국소 감소를 전체 peak 감소나 writer/reader 동시 합의 인수로 승격하지 않는다. Linux 값은 실제 타입 계산이며 native I/O 실행 결과가 아니다.

#### 변경 범위 선택과 실행

당시 main 선택 manifest(`cbc09f2:tests/fixtures/dev3-module-c2/aging-source/manifest.json`; [원본 보존 위치](#merge-main-20261028))에 변경 Rust6파일과 case별 관계를 실행 전에 고정했다. 총600초·compiler90초·case30초·로그8MiB·stack32MiB, 각 selector1회다. 신규 source 관측은 초기 Root/malformed XOD 경로의 계산과 Memory/Linux 타입 한도를, 기존 owner 관측은 semantic/Commit4phase·live/recovery 한도를, ledger 시험은 변경 Full charge와 Resident 혼합 거절·정확 반환을, actual Full 조회 시험은 두 번째 조회의 refs/pin/효과 전 Capacity·원 예약 반환·같은 snapshot 재시도를 확인한다.

- `r1`: production check PASS, 새 시험의 Linux 타입 경로에 `super` 한 단계가 많아 test build E0433 FAIL, runtime0. 원 로그·source·result를 보존한다.
- `r2`: 해당 test 경로 한 줄만 정정하고 test build PASS, 미실행 runtime **4개 각각1회 PASS**. 변경 없는 production check는 반복하지 않았다. 원 r1 production 결과를 r2의 새 실행으로 표시하지 않는다. 컴파일 경고0이다.

변경 없는 codec 동작·native 복구·전체 workspace/하위 wrapper를 반복하지 않았다. 기존 source별 PASS/FAIL/UNKNOWN·NOT_RUN과 DEV2·DEV3/S5·M4/M5 미완료를 유지한다. r1/r2 경과17.45/23.97초는 기능 검증 경과이며 성능 인수 수치가 아니다. 수정 Rust6파일에 파일별 rustfmt와 `git diff --check`를 적용했다. 자동 문서 verifier·전체 fmt/Clippy·push는 실행하지 않으며 push 시 기존 훅을 적용한다.

원 source/inventory·manifest·runner·compiler diagnostics·ELF·stdout/stderr/result는 `artifacts/module-c2-aging-source-main-20261027/{r1,r2}/`에 보존한다.

| 원 증거 | source SHA-256 | result SHA-256 |
| --- | --- | --- |
| r1 production PASS / test build FAIL |`c4f7582ce916d9ba428b2ffd7bad9158d8e48431ae918a019409f2d2d848f6d8`|`4facfa05f9447a3a585c05772fbafa80b8154e4e55dca0c4a9a2d62c059efb33`|
| r2 test build / runtime4 PASS |`a382bcf5f1ce51c821a6ce3177767ce0834a519179153f88748f6008bc86a74b`|`7b95d95c760a73b0ee55b135927a4bb39a9c3ddfa3b4c9660eea1ca7f07b28a7`|

r2 manifest SHA-256은 `5951f79cad43953e6a2e13609de8483571cc0a1fae153f16c6f8c00b65e6899e`, runner는 `698ae98678339af0c6f670880b06f2b10e6f1d75da0ad1b2a189bc8cbda71da2`, inventory는 `4271f3b5dd309ecc88bd7450faa8610b4eb713af0b687343314431ca3b5b3d28`, ELF는 `1d1f406052368888957399c2cd0e3474dac44f154490481e9a1489b923c980de`다.

#### OCR delegate 독립 검토

`/root/module_step_review`가 `ocr delegate preview --format json`·`ocr delegate rule --format json`으로 대상과 규칙을 확인했다. OCR 대상7개와 기본 제외7개를 모두 읽어 **전체14파일 검토·누락0·100%, 독립 D1/D2 한정 수용·필수 수정0·차단0**이다. `/root`가 이 계산 단위와 선정 검증의 한정 수용을 반영했다. 추가 제품 수정이 필요한 지적은 없었다.

r1/r2 source5638파일·manifest·runner/helper·inventory·compiler/runtime 로그·ELF와 C2_SHORT01 context hash를 읽기 전용 검산했다. 현재 제품/fixture는 r2 export와 같고 r1→r2는 test 경로 한 줄과 manifest 설명만 다르다. 원 E0433 FAIL·runtime0, r1 production PASS와 r2 test build/exact4 PASS를 구별했다. 검토자는 제품·문서·원 실행 결과 수정이나 빌드·시험·문서 verifier를 실행하지 않았다. 검토 identity 스크립트 첫 시도의 문자열/Path 정렬 차이를 원 함수의 Path 정렬로 정정해 일치를 확인했으며 제품/시험 FAIL로 분류하거나 원 결과를 변경하지 않았다.

`artifacts/module-c2-aging-source-main-review-20261027/`에 preview/rules/checklist/identity-audit/report를 보존한다. report SHA-256은 `590066a0d21d6023e5654333b92f2ab8e5f55a180e95b1630b5d1ed008df8072`, checklist는 `8023f7bba3a078b552724710aea554533c30dbb874207d9f1ed8486de153c1a9`, identity-audit는 `6dafbacbb20d52e0981af49f3c8266b5449e3371b8a54da227d6daf460783ec8`이다. 다음은 실제 WAL wait/Drain 호출·반환 수명과 writer/progress 예약이며 전체 동시 admission·실제 Row·M4/M5 인수는 미완료다.


<a id="c2-wal-wait-source-main"></a>
### C2 실제 WAL 대기·반환 source 계산의 main 수신 — 2026-10-28

사용자의 “진행 위치 확인 후 다음 한 단계·OCR delegate 리뷰 및 수정·commit” 요청으로 clean `main`·`e76e8c4050ef597ca2f4c00cc50642b19a917a54`에서 시작했다. M0–M3 완료, M4는 Aging/Root source 계산까지 부분공급, M5는 A/B 순수 본문 착수 상태였다. 이번 한 단계는 **writer/progress 예약 전에 실제 WAL 대기 호출·반환의 source 값을 구분하는 관측**이다. 별도 브랜치 `dfc3bc9b00b025dd12955ed6abee19ef580444e6`의 test-only7파일을 현재 main에 적응하고 원 core816B가 포함된 main shared 값을 사용한다. 다른 worktree·과거 결과를 변경하거나 브랜치 전체를 병합하지 않았다.

`D3-MODULE-WAL-WAIT-SOURCE-MAIN-001` · D2 · **ACCEPTED / 구현·선택 검증 완료 / 독립 D1/D2 한정 수용 / Full 동시 합 기준 FAIL**. 작성·계약·통합 `/root`, 별도 검토 `/root/module_step_review`. §6.9의 동시 source 상계를 구체화할 선행 관측이며 제품 상계·charge·admission·효과·wire·권한은 바꾸지 않는다. 전체 writer/progress admission·M4/M5 인수는 미완료다.

#### 호출·반환 수명과 선택

`physical::wait_source_observations`는 state·requirement predicate·foundation I/O·sync claim·ensure·Condvar·wait를 구분한다. append guard 종료 뒤 sync, file guard 종료 뒤 publication에만 max를 적용한다. wait와 ensure의 별도 UnwindFence, 원 borrowed slot·frame·Result·오류·반환값은 수명에 따라 합산한다. backend/OS stack과 원 File/AppendState/slot backing은 이 호출값의 범위 밖이며 해당 소유자의 회계로 남는다.

`commit_bridge`는 실제 physical 호출과 그 뒤 semantic receipt 초기화를 나눈다. caller는 compact image·epoch packet·원 Pending targets·prefix/requirement/key·outer Result를 계속 보유하고, WAL Result→Retention Result→DurableCommit 추출의 handoff를 별도로 합산한다. stage 준비 자료 전체를 WAL wait와 계속 합하는 대안은 실제 수명과 다르며, main의 공통 core를 빼서 참고 브랜치의 수치를 재현하는 대안도 채택하지 않는다. 관측 함수는 모두 `cfg(test)`이고 기존 생산자·동기화 동작은 그대로다. 실제 호출/반환 타입·수명이 바뀌거나 admission에 연결할 때 재검토한다.

| source 항목 | Memory / Linux 구체 타입(B) |
| --- | --- |
| state / predicate / foundation I/O |336 / 710 / 544 (양쪽 동일)|
| sync claim / ensure / Condvar / physical wait |1160 / 2248 / 1046 / 2808 (양쪽 동일)|
| Pending parent / semantic bridge / 반환 handoff |2064 / 3256 / 1216 (양쪽 동일)|
| Pending parent + max(bridge, handoff) |5320 (양쪽 동일)|
| shared retained (원 core 포함) |23072 / 23208|
| shared + Full37724 + Pending5320 |**66116 / 66252 — 65536 기준 FAIL, 각각580 / 716 초과**|
| shared + Resident16860×2 + Pending5320 |62112 / 62248 (이 관측 합만 한도 이내)|

기존 production Pending 포괄값18532B를 이번 관측5320B로 교체하지 않는다. 이 값은 source/type 수명 계산이며 RSS·실제 stack·native I/O·성능 측정이 아니다. 참고 브랜치의 shared22256B를 쓰는65300B를 main 결과로 승계하지 않는다. **새 관측 시험 PASS는 Full 동시 합 기준 FAIL을 취소하지 않는다.** Resident 합이 작더라도 entry/exit ticket·Drain 전후·writer/progress protocol 전체가 미공급이므로 해당 조합의 실제 admission 성공으로 승격하지 않는다.

#### 변경 범위와 실행

당시 main 선택 manifest(`cbc09f2:tests/fixtures/dev3-module-c2/wal-wait-source/manifest.json`; [원본 보존 위치](#merge-main-20261028))에 변경7파일·신규 exact selector1개·Memory/Linux 타입 관측과의 관계를 실행 전에 고정했다. 총600초·compiler90초·case30초·로그8MiB·stack32MiB, 각 selector1회다. frozen export에서 `cargo test -p xadb-engine --lib --no-run --locked --offline --message-format=json`으로 test build PASS·경고0, 새 `catalog_wal_wait_source_phases_include_claim_and_condvar`를 `--exact --nocapture`로 **1회 PASS**했다. 경과23.52초는 검증 경과이며 성능 수치가 아니다.

제품 동작 diff0이므로 production check·기존 제품 시험·native 복구·전체 workspace/하위 wrapper는 실행하지 않았다. 독립 검토는 새 helper를 기존 `mod tests` 뒤에 둔 배치가 `items_after_test_module` 규칙에 걸림을 정적으로 지적했다. helper 본문을 그대로 테스트 모듈 앞으로 옮겼다. `r2`는 이1파일의 item 순서만 바뀐 source이며 `cargo clippy -p xadb-engine --lib --tests --locked --offline --message-format=json -- -D warnings`를1회 PASS·경고0으로 확인했다. 원 r1 test build/runtime1을 보존하고 r2에서는 runtime을 반복하지 않았다. 원 지적은 정적 검토 결과이며 실행하지 않은 r1 Clippy FAIL을 만들지 않는다. 기존 source별 PASS/FAIL/UNKNOWN·NOT_RUN과 DEV2·DEV3/S5 미완료를 보존한다. 변경 Rust7파일에 파일별 rustfmt와 `git diff --check`를 적용했다. 자동 문서 verifier·전체 fmt/Clippy·push는 실행하지 않으며 push 시 기존 훅을 적용한다.

원 source/inventory·manifest·runner/helper identity·compiler diagnostics·ELF·stdout/stderr/result는 `artifacts/module-c2-wal-wait-source-main-20261028/{r1,r2}/`에 보존한다. 아래 표는 runtime을 실행한 r1 identity다.

| identity | SHA-256 |
| --- | --- |
| source |`664389fd66bf3cdf2ea47ef2d7d7130fa43e21594c0a4ddf99718dcfd5cc696c`|
| manifest |`2e56de1b702669da20e3d273fc51788a891afa126ff5ba852f08994ac0367e9e`|
| runner |`4e49955cf84abfeb199810920a9d08f9258ec101acbdd6823ff1f730da62c350`|
| inventory |`31320b35ace8acfbeecfcfd8fe59b80c069a71056d6a41778c0ae0de28933ca4`|
| ELF |`89df394569bc9c57013289943dce9879bb19fe8259ff4815ac68f1a9c53535ff`|
| result |`5fb3af3c134d5b09794d01e572c166384a29ebbe3b526f8ae55e92b067cad27f`|

r2는 source `8db0cc5d51e2117d469d1bda6e014d2e0cb426623d29fac24443ae2fa5306210`, inventory `6c968242997a5b80b9380caf6ae9049d1694eb84d4481d7b36ca955627aefeb9`, lint runner `ceccebf84bc8aa5ee68f20bdf15c87804f0f453dd315adbc30434ab0d51c04c3`, result `610b88684f47af2d6873255eac7c48749b1eab86f9e796b871b3f8e21767c49a`다. manifest는 r1과 동일하며 r2 경과18.72초다. 이 정적 검증에는 새 runtime/ELF 실행 결과가 없다.

#### OCR delegate 독립 검토

`/root/module_step_review`가 `ocr delegate preview --format json`·`ocr delegate rule --format json`으로 대상/규칙을 확인하고 OCR 대상8개와 기본 제외7개까지 직접 검토했다. **전체15파일·누락0·100%, 지적1건 수정 확인·미해결 필수 수정0·차단0, 독립 D1/D2 한정 수용**이다. `/root`가 이 관측 단위와 선택 검증의 한정 수용을 반영했다. helper의 테스트 모듈 뒤 배치를 고쳤고 함수 내용·제품 동작 변경은 없다.

r1/r2 각각 source5639파일·inventory·manifest·runner/helper·compiler/lint 로그, r1 ELF·exact1 실행 출력의 identity를 읽기 전용 검산했다. 현재 source=r2, r1→r2는 helper 위치1건뿐이고 나머지 bytes·원 r1 source/ELF/runtime 결과를 보존하며 r2 runtime0을 확인했다. 검토자는 제품·문서 수정이나 빌드·시험·자동 문서 verifier를 실행하지 않았다. 사전 대조에서 main core816B를 포함한 Full 동시 합이 참고 브랜치와 다름을 확인했고 위 실행에서 관측한 FAIL을 그대로 유지했다.

`artifacts/module-c2-wal-wait-source-main-review-20261028/`에 preview/rules/checklist/identity-audit/report를 보존한다. report SHA-256은 `9b91897b654d192d2c6500fd729c2f6870dad469c4c2fb2ed04bbc22e8a53946`, checklist는 `e564d60e311e0a8115b762aa77b55bac25613ffec40117b7f2df0312407b505c`, identity-audit는 `969f044245fd647ad2d0f04cb589c0c519d27d2c3b0d32290aeca4ed10cf5e54`다. 다음 단위는 Drain 완료/guard 인계의 수명 분리이며, 이후 writer/progress 예약·entry/return 전체 합을 위 FAIL을 해소하는 계약과 함께 공급해야 한다. 이번 관측을 전체 M4/M5 인수 완료로 표시하지 않는다.


<a id="c2-drained-handoff-main"></a>
### C2 Drain 완료와 content guard 인계의 main 수신 — 2026-10-28

사용자의 “진행 위치 확인 후 다음 한 단계·OCR delegate 리뷰 및 수정·commit” 요청으로 clean `main`·`d2baeb98839cc390804b6737b5989e5479323d0d`에서 시작했다. M0–M3 완료, M4는 실제 WAL wait source 관측까지 부분공급, M5는 A/B 순수 본문 착수 상태였다. 이번 한 단계는 **authentic durable Drain 완료와 content guard 재구성의 consuming 경계 분리**다. 별도 브랜치의 `00feccdfcdf3be062005c50dcb11b1c4160403e2`에서 필요한 코드만 main의 기존4phase·원 core 회계에 적응했다. 참고 브랜치 전체·과거 실행 결과를 합치거나 다른 worktree·세션을 변경하지 않았다.

`D3-MODULE-DRAINED-HANDOFF-MAIN-001` · D2 · **ACCEPTED / 구현·선택11개 검증 완료 / 독립 D1/D2 한정 수용**. 작성·계약·통합 `/root`, 독립 검토 `/root/module_step_review`. 기존 M2/M3·§6.9 안의 내부 경계 구체화이며 원65,536B 한도·WAL/권한·원자 공개·오류/복구 의미를 유지한다. 전체 writer/progress 예약·동시 합·M4/M5 인수는 미완료다.

#### 원 소유·수명과 선택

`PendingCommitTargets::drain_durable`은 원 durable proof로 begin_drain→wait_ready를 마친 뒤 같은 PinSet·StablePage·CommitInstalling을 private·nonClone `DrainedCommitTargets`로 이동한다. actual Catalog는 그 반환 뒤 `resume`를 호출하여 content guard storage를 재구성하고 기존 stamp/digest/recLSN 검산을 수행한다. 이 사이에는 Installing·active_apply1·원 version gate와 pins4가 유지되고 content guard/read 표시가0이다. 새 authority·pin 예약·등록·backing은 없다.

정상 미완료 Drop도 먼저 원 installation을 drop하여 fence/Abandoned·version/admission 반환을 완료한 뒤 원 pins를 반환한다. panic에 의존한 fence나 pin 반환 후 fence는 허용하지 않는다. 새 hook은 실제 carrier의 Drop을 요청하거나 관측할 뿐 권한/receipt를 만들지 않는다. 이 경계에서 후속 source 전환은 이미 예약한 몫의 무대기 checked 동작이어야 하며 capacity 대기를 추가할 수 없다.

한 함수에서 Drain과 guard 재구성을 계속 수행하는 대안은 후속 source 전환의 소비 경계를 제공하지 못한다. 참고 브랜치의 포괄 수식을 통째로 가져오는 대안은 main의 기존 phase·core 회계와 달라 제외했다. physical drain은 Pending+Drained+원 Result 반환을 합산하고 resume은 Drained+WriteTargets+Install·반환값을 합산한다. Catalog는 mapped RetentionResult도 포함해 drain/resume/설치의 max를 계산하며 semantic4phase와 shared original core1회 계수를 유지한다. actual 호출·반환 타입, raw content 사용자, 권한/Drop 순서 또는 source 예약 연결이 바뀌면 해당 경계를 재검토한다.

#### 실행 전 선택·리뷰 수정과 결과

당시 main 선택 manifest(`cbc09f2:tests/fixtures/dev3-module-c2/drained-handoff/manifest.json`; [원본 보존 위치](#merge-main-20261028))는 신규4·직접 영향4·source3의 **11개 exact selector, 각각1회**를 선택한다. 총600초·compiler90초·case30초·로그8MiB·test stack32MiB를 실행 전에 고정했다. 독립 사전 리뷰가 기존 physical source 시험의 신규 drain phase 누락과 resume 하한의 Pending 사용을 지적했다. Drained 입력·반환 하한과 drain max를 보완하고 그 시험을 선택 목록에 추가한 뒤 첫 실행했다. 이는 정적 지적이며 실행하지 않은 이전 시험 FAIL을 만들지 않는다.

- 신규4: 건강한 handoff의 원 pins/version·content0·before bytes, 외부 fence, 정상 unfinished Drop, handoff panic. 정상 Drop은 외부 fence/panic 없이 pin-return hook에서 이미 fenced/Abandoned·active0·version 반환·pins4를 확인한 뒤 pins0/Q0을 검산한다. 오류3개는 원 C04/N03 독립 기대값과 현재-build memory recovery를 사용한다.
- 직접 영향4: MT03 Pending Full Undo LOAD, MT05 lower-image flush/recLSN, MT07 유한 content Drain fence, MF10 partial install/unwind·복구. 새 consuming 경계의 정상·실패·정리 경로만 선택했다.
- source3: fresh/recovered 원 core·carrier 상계, Memory/Linux 실제 타입의 semantic4phase, physical N=4/9 전이의 공존 하한과64-target의 원 Capacity gate 유지.

frozen export에서 `cargo check -p xadb-engine --lib --locked --offline --message-format=json`, `cargo clippy -p xadb-engine --lib --tests --locked --offline --message-format=json -- -D warnings`, `cargo test -p xadb-engine --lib --no-run --locked --offline --message-format=json`이 모두 **PASS·경고0**, 같은 ELF의 exact11개도 모두 **1회 PASS**다. 총90.95초는 검증 경과이며 성능 수치가 아니다. 물리 WAL I/O·wire·native driver·복구 구현 diff0이므로 완료한 native/하위/workspace 시험은 반복하지 않는다. 실제 변경 오류 경계의 현재-build memory 복구와 native 타입 상계를 구별하며 새 native I/O 실행으로 주장하지 않는다. 기존 FAIL·UNKNOWN·NOT_RUN과 DEV2·DEV3/S5 미완료를 보존한다.

#### source 상계와 남은 공급

| 항목 | 이번 source 값(B) |
| --- | --- |
| physical N4 stage / pending / drain / resume / install |11260 / 6748 / 9196 / 10260 / 6876|
| physical N9 stage / pending / drain / resume / install |17180 / 8188 / 12796 / 14900 / 8396|
| physical 포괄 gate N4 / N9 |14336 / 24976|
| semantic preflight / stage / pending / install |16632 / 32292 / 18532 / 22052|
| semantic peak |32292|
| shared retained Memory / Linux type (core816 포함) |23072 / 23208|
| live peak Memory / Linux type |64000 / 64136|
| recovered peak Memory / Linux type |65376 / 65512|

새 carrier/Result 비용을 계수했으며 기존 전체 peak 한도 내 결과를 유지한다. 위 값은 source/type 상계이며 RSS·실제 stack·성능 측정이 아니다. 앞 [WAL wait 관측](#c2-wal-wait-source-main)의 Full+Pending 합66116/66252B·65536B 기준 **FAIL은 그대로**다. 이번 분리로 해당 초과를 해결했다고 하지 않는다. 다음은 writer/progress source 예약·호출 진입/반환 전체 합과 이 FAIL을 해소할 공급이며 실제 Row·C+A/C+B·MI01–06·M4/M5 인수는 남아 있다.

원 source5641파일/inventory·manifest·runner/helper·compiler/lint·ELF·stdout/stderr/result는 `artifacts/module-c2-drained-handoff-main-20261028/r1/`에 보존한다.

| identity | SHA-256 |
| --- | --- |
| source |`3734c6b782611aa91fd35dd43cdafb066f6e8046a3696dc63dcbe9c46d18f7bd`|
| manifest |`1c6a48b14209a97e69b515c1dc54598247ce5dc663ee63916b8fd38533e87f16`|
| runner |`ddc1e7867c77c243b7892f15fe9459603cb275d56edacf161d1d1a8b460e54ca`|
| inventory |`cb942e58fefe17d3c4c68526d37194a918eb9f243e7184108adc53444900a9d1`|
| ELF |`c5a2a5de91a2aa1c47e109a3bf75c69d37136b88f270473286b54e0c692b258b`|
| result |`91c1b2ecd5eeac020e4d777ec6ef435e282491fdbfd93b09d97a02fe497a98e7`|

#### OCR delegate 독립 검토

`/root/module_step_review`가 `ocr delegate preview --format json`·`ocr delegate rule --format json`으로 전체 변경을 검토했다. 기본 대상10개와 기본 제외 중 명시 추가한8개를 모두 읽어 **전체18파일·100%·skip0**, source 시험의 drain/resume 하한 지적1건 수정 확인·미해결 필수 수정0·차단0이다. 독립 D1/D2는 이 Drain 인계 단위의 구현·선택 검증을 한정 수용했으며 `/root`가 반영했다.

원 export와 현재 source5641파일, inventory·manifest·runner/helper·ELF, compiler3개의 진단/로그와 exact11개 stdout/stderr의 identity를 읽기 전용 검산했다. 현재 제품과 실행 source가 같으며 source 시험 보완은 첫 실행 전에 반영됐다. 검토자는 제품·문서 수정이나 빌드·시험·자동 문서 verifier를 실행하지 않았다. 무변경 native/하위 시험을 반복하지 않은 선택과 기존 Full+Pending 기준 FAIL 보존, 원 권한/Drop·main4phase/core 및 후속 source 예약의 무대기 조건을 확인했다.

`artifacts/module-c2-drained-handoff-main-review-20261028/`의 report SHA-256은 `5e680864a51e6424c01d21e37e243cb841fa035ccaf0cb6bcd7fb2c3f7a06738`, checklist는 `a8ccc579c01a11fa32978ab1146881fee77b9f5365272823746c5d7ad2473a36`, identity-audit는 `d865e0b870167bc7bedf8a480076364f1691d1dcea7816b1499dd0df80adfb7a`다. 변경9개 Rust 파일에 파일별 rustfmt와 `git diff --check`를 적용했으며 자동 문서 verifier·전체 workspace fmt/Clippy·push는 실행하지 않는다. 문서 검사는 기존 pre-push 훅에서 수행한다.


<a id="c2-call-scope-main"></a>
### C2 Catalog 결과 소비까지 유지하는 호출 수명의 main 수신 — 2026-10-28

사용자의 “한 단계 더 진행” 요청으로 clean `main`·`ae042aee730fb66c4adde84e6b793ae472c5e894`에서 시작했다. M0–M3 완료, M4 C2는 Drain 완료/guard 인계까지 부분공급, M5는 A/B 순수 본문 착수 상태였다. 이번 한 단계는 **조회 결과를 소비하는 동안에도 원 source/progress 예약을 유지하는 CatalogCallScope**다. writer/progress의 전체 동시 합을 계산하기 전 결과 소비 수명의 공백을 닫는 유한 공급이며, 참고 `847def2aa9d84bd2ade7abe19753caa6b5bf70d6`의 필요한 코드만 main에 적응했다. 참고 브랜치의 전체 코드·과거 실행/실패 결과를 main 결과로 옮기지 않았고 다른 worktree·세션을 변경하지 않았다.

`D3-MODULE-CATALOG-CALL-SCOPE-MAIN-001` · D2 · **구현·선택 검증 완료 / 독립 D1/D2 한정 수용**. 작성·계약·통합 `/root`, 독립 검토 `/root/module_step_review`. 원 §6.9·65,536B 상한·권한·WAL·원자 공개·복구 의미를 유지하며 source 예약의 수신 구간만 구체화한다. 전체 writer/progress·진입/반환 합·statement/자동 read-set·M4/M5 인수는 미완료다.

#### 수명·오류 경계와 대안

`CatalogCallScope`는 원 Owner/backend/snapshot의 borrow와 기존 source/progress를 보유하는 private-field·nonClone 타입이다. `catalog_call`/`catalog_resident_call`이 고정 class를 한 번 취득한다. main의 원 `try_read_operation`이 검산과 표시를 함께 수행하므로 참고 브랜치의 추가 precheck API는 가져오지 않았다. scope의 `lookup`은 원 operation을 재대여하고 반환 RecordView·복사한 view·column.name의 수명을 scope borrow에 묶는다. 순차 lookup은 같은 예약을 사용하며 class escalation은 없다.

Operation은 refs/lease·caller scratch만 소유한다. refs 정리→최종 decode/Result→선택 caller 소비→scope source 반환→progress 반환 순서다. 정상 오류도 `finish_error`/`finish_result`로 명시 정리하여 원 Fatal을 보존하고 성공/Unchanged보다 cleanup Fatal을 우선한다. source cleanup 실패에도 progress를 반환한다. 정상 Drop은 read scope를 정리하고 panic은 원 fence·미해결 source 보존 규약을 따른다. 새 capability·pin/registry·backing을 만들지 않는다.

선택된 실제 `catalog_memory::query`는 Found의 header/Table/Column 또는 Missing/FlushRequired 결과를 소비한 뒤 scope를 닫으며 flush는 그 뒤 수행한다. 기존 synchronous entry는 같은 제품 본문을 사용하고 내부 결과의 cleanup을 명시 완료하지만 **반환 이후 caller 소비는 scope 보장 밖**이다. SQL 연결·statement 수신·전체 source admission 완료를 주장하지 않는다.

결과 반환 전에 예약을 돌려주는 기존 방식은 caller 소비 비용을 포함하지 못한다. Result마다 별도 generic credit/registry를 만들거나 reader backing/상한을 늘리는 대안도 채택하지 않는다. 원 고정 class를 소비 완료까지 유지하고 caller가 borrow를 종료한 뒤 checked finish하는 방식을 선택했다. 임의 caller의 무제한 복사·장기 보유, foreign Owner의 선예약 전 거절과 반환 tail의 전체 동시 상계는 이 유한 caller 검산의 범위 밖이다. 실제 호출/반환 타입·소비 형태·cleanup/authority 또는 writer 예약 연결이 바뀌면 해당 수명과 source 식을 재검토한다.

#### 실행 전 선택·리뷰 보완

당시 main 선택 manifest(`cbc09f2:tests/fixtures/dev3-module-c2/call-scope/manifest.json`; [원본 보존 위치](#merge-main-20261028))에 현재 diff와 **runtime18·compiler4, 각각1회**의 관계를 고정했다. 총600초·compiler90초·case30초·로그8MiB·stack32MiB다. 참고 manifest의 과거 r2/r3 선택·실패 metadata를 제거하고 main baseline·변경 경로를 기록했다. main의 wrapper 반환 Result 계수 관례를 operation/scope entry에도 유지했다.

독립 사전 리뷰가 변경 query helper의 Found 분기를 직접 실행하는 사례가 빠졌음을 지적했다. 신규 Found 시험은 scope API 자체를 검증하고 기존 MC02는 query의 Missing만 검증하므로, 실제 query(..., found=true)를 호출하는 MT01을 실행 전에 추가했다. 이는 정적 시험선택 지적이며 실행하지 않은 FAIL을 만들지 않는다.

| 선택 | 변경과 검증의 관계 |
| --- | --- |
| 신규 scope runtime6 | Found 복사/nested view 소비 중 예약·표시 유지, 순차 reuse·경쟁 Full Capacity, 반환 Missing scope의 실제 Drain 대기, cleanup Fatal 우선, normal Drop/panic, 실제 resident Undo Busy와 실제 Undo I/O Fatal 정리 |
| 기존 직접 영향 runtime10 | source 진입/반환4·foreign Owner/Missing·dirty-victim FlushRequired/재시도·Undo I/O3·Pending Undo LOAD MT03·query Missing MC02·query Found MT01. 같은 source/progress 생성·정리 또는 실제 query 호출 변경에 해당하는 사례만 선택 |
| source runtime2 | 원 shared core/신규 carrier 회계와 유한 caller 소비 phase의 Full/Resident 고정 charge, Memory/Linux 타입의 합계 관측 |
| 신규 compiler4 | 정상 소비 후 finish1, copied RecordView·column.name의 finish 뒤 사용 E0505 각1, 살아 있는 view 중 Owner 재진입 E0499. exact code·primary path/line으로 의도한 거절을 확인 |

#### 실제 실행과 source 상계

frozen export에서 `cargo check -p xadb-engine --lib --locked --offline --message-format=json`, `cargo clippy -p xadb-engine --lib --tests --locked --offline --message-format=json -- -D warnings`, `cargo test -p xadb-engine --lib --no-run --locked --offline --message-format=json` 모두 **PASS·경고0**이다. 같은 ELF의 exact runtime18개와 각각 분리된 probe export의 compiler4개도 **각1회 PASS**다. 총88.01초는 검증 경과이며 성능 결과가 아니다. 제품 source 동결 이후 수정·완료 시험 반복0이다. 물리 WAL I/O·wire·native driver·복구 구현은 바뀌지 않아 무변경 native/하위/workspace 시험을 반복하지 않는다. Linux type 관측은 native I/O 실행 증거가 아니며 과거 FAIL·UNKNOWN·NOT_RUN과 DEV2·DEV3/S5 미완료는 유지한다.

| 항목 | 이번 main source 값(B) |
| --- | --- |
| scope held / entry / 유한 consumer |392 / 2105 / 5256|
| Full / Resident 고정 charge |38076 / 17212 (각 이전보다352 증가)|
| shared retained Memory / Linux type (원 core816 포함) |23072 / 23208|
| shared + Full |61148 / 61284|
| shared + Resident×2 |57496 / 57632|
| shared + Full + Pending 관측5320 |**66468 / 66604 — 65536 기준 FAIL, 각각932 / 1068 초과**|
| Memory live peak / metadata / whole |64000 / 187008 / 252544|
| Memory recovered peak |65376|

semantic Commit4phase의16632/32292/18532/22052B와 원 core 계수를 유지한다. consumer는 operation과 순차이므로 고정 charge 식에 max로 포함한다. main entry2105B는 생성/forward Result와 class를 포함하며 참고 branch1944B를 그대로 승계하지 않는다. 이 entry 식이 선예약 전 모든 caller의 동시 진입이나 반환 이후 tail까지 예약했다는 뜻은 아니다. source 값은 RSS·실제 stack·성능 측정이 아니다.

원 [WAL wait source](#c2-wal-wait-source-main)의66116/66252B FAIL은 해당 source 결과로 보존한다. 이번 caller 수명 비용352B를 포함한 현재 합도 FAIL이며, 시험 PASS와 전체 동시 admission의 FAIL을 구별한다. 다음은 Full 경로의 중첩 source 비용·진입/반환 합을 정리하고 writer/progress 예약을 공급하는 일이다. 실제 Row·C+A/C+B·MI01–06·M4/M5는 미완료다.

원 source5648파일/inventory·compiler probe4개 source·manifest·runner/helper·compiler/lint·ELF·stdout/stderr/result는 `artifacts/module-c2-call-scope-main-20261028/r1/`에 보존한다.

| identity | SHA-256 |
| --- | --- |
| source |`20c5f17a9d6bfe267805f9cb71cb78b2486cde76a3b0c3da7669025380048e34`|
| manifest |`01a7560485288d8b1ce97e50f089ac5c92028d316307d8ee192c5b91c9c4c53c`|
| runner |`e6823b67d8421fa4ee0e97f4bda9c6144347101143faebfb9975a1417a1dda58`|
| inventory |`b3105e38c6cf4a66e38c6b457e146495f5a83d373edb709f8bc1cf3ee8bc39b4`|
| ELF |`2e9e68ad2c341d5a38ab27267ee2d5b43850d4ac80aba398cb38356bbb2d6fd6`|
| result |`d92ad8cd1a3c5593fe62ffeb77cfc90e901da4fb3b6cac7f2044371b32bba4e9`|

#### OCR delegate 독립 검토

`/root/module_step_review`가 `ocr delegate preview --format json`·`ocr delegate rule --format json`으로 전체 변경을 검토했다. 기본 대상11개와 기본 제외 중 명시 추가한12개를 모두 읽어 **전체23파일·100%·skip0**, query Found 직접 선택 지적1건의 실행 전 보완 확인·미해결 필수 수정0이다. 독립 D1/D2는 이 Catalog 결과 소비 수명 단위의 구현·선택 검증을 한정 수용했으며 `/root`가 반영했다.

원 export와 현재 source5648파일, inventory·manifest·runner/helper·ELF, compiler3개 및 runtime18개 로그·진단의 identity를 읽기 전용 검산했다. compiler probe4개는 각각 등록1곳과 정확한 fixture1파일만 달라지며 정상 compile과 의도한 E0505/E0499의 primary path/line을 확인했다. 검토자는 제품·문서 수정이나 빌드·시험·자동 문서 verifier를 실행하지 않았다. 무변경 native/하위 시험을 반복하지 않은 선택, 현재/원 source의 Full+Pending 기준 FAIL, legacy caller 한계와 전체 진입/반환·writer/progress 후속 의무를 확인했다.

`artifacts/module-c2-call-scope-main-review-20261028/`의 report SHA-256은 `d025b567ad278a7e980fe0d9ad836fb6f142a42d99c1abff37cf71ba2a64ac05`, checklist는 `6c4110d51511647823c224306cec87fd3d4bd73fa7415d810317f10156a2102a`, identity-audit는 `6e35d2ca36d640b1ae5d362c24750e025fd3a3f46e9548aca9c0a33b20aa6186`다. 변경10개 Rust 파일은 파일별 rustfmt와 `git diff --check`로 확인했다. compiler 입력4개는 의도한 진단 line/hash를 보존했다. 자동 문서 verifier·전체 workspace fmt/Clippy·push는 실행하지 않으며 문서 검사는 기존 pre-push 훅에서 수행한다.

<a id="c2-short-source"></a>
### C2 CatalogFull short source 예약 — 2026-10-27

기준 `0852f06`에서 계획 완료 지시를 재개했다. 기존 `/home/et16/.codex/worktrees/module-independence/xadb`는 다른 채팅 소유여서 수정하지 않고 같은 commit의 `/home/et16/.codex/worktrees/module-independence-finish/xadb`를 만들었다. 작업·계약·통합 `/root`, 독립 검토 `/root/module_review`다. 원 artifacts는 읽기만 하며 기존 source별 성공·실패·미완료를 유지한다.

D1/D2의 유한 단위는 기존 전체 Catalog 조회의 고정 source 예약이다. 새 private `OperationSourceAdmission`이 원 IsolationLedger header24..32의 합을 checked add/sub로 관리하고 다른 header·장기 credit은 보존한다. class/charge를 caller가 선택하거나 실행 중 늘리는 API는 없다. 같은 원 core의 무효과 admission precheck로 기존 Drain/role 오류 우선순위를 확인한 뒤 source를 예약하며, 실제 progress 진입에서 다시 검사한다. source 부족은 reference/pin/operation 표시 전에 Capacity/Unchanged다. 참조 정리 뒤 마지막 Catalog decode/Result가 끝날 때까지 source를 보유한다. 정상 명시 종료/Drop은 한 번만 반환하고 poison/fence/underflow에서는 잔여 charge와 fatal 오류를 보존한다.

별도 pool/semaphore를 추가하거나 full 조회에 낮은 resident charge를 적용하는 대안은 택하지 않았다. future resident-only class는 miss에서 동일 operation을 확대하지 않고 checked 종료 후 같은 snapshot의 별도 full 조회로 재진입해야 한다. 원 writer·다른 ReadPort entry·rollback/terminal/progress 몫이 아직 예약되지 않으므로 **이 단위는 전체64KiB aggregate 보장, caller-retained factory, writer1+resident reader2 또는 M4/M5 완료가 아니다.** 이 제한을 표시한 설계 순서는 독립 읽기 검토에서 한정 수용했고 실제 diff 검토도 아래 한정 범위로 수용했다.

[실행 전 manifest](../../tests/fixtures/dev3-module-c2/short-source/manifest.json)에 실제 diff와17개 selector, 총600초·compiler90초·case30초·로그8MiB·test stack32MiB를 동결했다. 새 leaf7개는 literal byte 합·동시2thread·overflow/header·underflow·poison/fence·unwind/forget·실제 class/type을 검산한다. 새 precheck1개는 무효과 관측/재검사를, actual 겹침1개는 원 session에서 두 번째 조회가 refs/pin/효과 전에 거절되고 첫 종료 뒤 같은 snapshot으로 재진입함을 확인한다. 기존 영향8개는 정상/fenced Drain, Pending Undo LOAD, Missing/foreign owner, FlushRequired, 실제 I/O 오류, Found/Q 공개와 source census다. 무관한 완료 시험·전체 workspace·하위 wrapper·native/새PID 회복은 실행하지 않았다.

| source 실행 | 결과·범위 |
| --- | --- |
| r1 | production PASS. test build FAIL(E0369): 신규 시험이 PartialEq 없는 RetentionCause를 assert_eq로 비교. runtime17 모두 NOT_RUN |
| r2 | cfg(test) assertion10곳만 matches로 정정, 제품 변경0. production 빌드는 반복하지 않음. test build PASS, 미실행 exact17개 각각1회 PASS |

source census는 shared retained `max(original owner+DiskSession, RecoveryRuntime+Plan4Recovered)`를 원 layout으로 계산한다. 실제22256B, CatalogFull41576B, 합63832B, admission leaf1024B이며 이전 lookup41552B보다 새 handle24B가 늘었다. live peak63184/metadata186192/whole251728B, recovered peak64611B는 그대로다. 이는 Rust source 값·수명 상한이며 stack/RSS 실측이나 성능 인수가 아니다.

원 export/inventory·manifest·runner·compiler 로그·ELF·stdout/stderr·result는 새 작업 공간의 `artifacts/module-c2-short-source-20261027-r1/`·`r2/`에 보존한다.

| 실행 | source SHA-256 | result SHA-256 |
| --- | --- | --- |
| r1 FAIL | `44591b2b774d8f8e369fdaab545cedd4521f4eab575b9f20da4b9be4df862660` | `3331cb769e28599597e9c2c4f7ab63737ea2e6df248ac8a4b9b26f0bbc3cb796` |
| r2 PASS | `30f19b5627da519abca37a7237abbf57f31de5c895dd3967df1f813fcf3dc414` | `b7316a6ef024870e17ff695511bb8033b6ef7cb0a3e51cc21f5f1106089f019e` |

공통 manifest SHA-256은 `810f2025d032eb9183057aab8229371d682f9656fe52c2f789313a798b58e607`, r2 ELF는 `964532d4ad1e9e3cec83553c4947cfaef2359d56945d8cb4c9a2b12bbe2c37e8`다. 경과19.06/43.70초는 성능 결과가 아니다. 변경 Rust만 파일별 rustfmt로 정리했다. 독립 제품/증거 검토는 아래 한정 범위로 수용했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.

독립 검토에서 frozen manifest의 I/O 오류·fenced Drain 두 선택 이유가 기존 read_operation 표시와 새 source byte charge를 혼동할 수 있음을 확인했다. 현재 manifest만 명확히 정정했다. fatal 뒤 반환되는 것은 기존 progress 표시이며 원 refs와 새 byte charge는 fence 아래 보존한다. frozen r1/r2의 manifest·hash·결과는 원문 그대로 유지하고, 제품·시험 변경이나 재실행은 없다. 위 manifest hash는 두 원 실행의 값이다.

`/root/module_review`는 실제 diff·새 source/fixture·문서6개 및 r1/r2의 원 source5618개씩·manifest/runner/ELF/로그 identity를 읽기 전용 대조했다. 두 선택 이유 정정과 frozen 보존을 좁게 재확인한 뒤 **CatalogFull short source 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. 검토자는 파일 수정·시험·빌드·문서 verifier를 실행하지 않았으며 `/root`가 이 단위를 반영한다. 다음 단위는 resident-only 조회, writer/progress phase와 원 ledger의 실제 합산이며 Row/terminal/recovery 및 M4/M5 전체 목표는 유지한다.

<a id="c2-resident-read"></a>
### C2 상주 Catalog 조회와 고정 source class — 2026-10-27

`ec018d6` 뒤 같은 `codex/module-independence-finish` 작업 공간에서 진행했다. **D3-MODULE-RESIDENT-001 · D1/D2 한정 수용 / 선택 runtime9 PASS / 전체 aggregate·M4/M5 미완료**다. 구현·계약·통합은 `/root`, 별도 검토는 `/root/module_review`다. 사용자의 계획 완료 지시와 DP01의 기존 위임으로 진행하며 필수 자원 한도·원 의미를 변경하지 않는다.

선택은 기존 Catalog 제품 본문을 그대로 소비하는 `lookup_catalog_resident` 입구와 immutable `CatalogResident` class다. 원 isolation ledger에 admission을 예약하며 같은 operation 도중 Full로 확대하지 않는다. 비상주는 refs/progress/source/pins의 checked 정리 뒤 Busy/Unchanged이고 같은 snapshot의 Full 재호출이 가능하다. 모든 입력을 Full replay로 보내는 대안은 두 상주 reader의 source 공급에 맞지 않고, 새 주소 map/권한/allocator owner를 만드는 대안은 원 권한의 유일성을 해치므로 채택하지 않았다.

`read/resident.rs`는 원 allocator guard로 읽은 페이지를 기존425984B caller loan의 loader 첫16384B에 복사하고 guard를 반환한다. 원 allocator의64-entry inventory에서 allocated Status를 전부 관측하며, 실제 target-set read가 다시 원 allocator와 페이지 membership을 확인한다. 첫 exact TxnRef를 찾은 뒤에도 후속 Status가 비상주면 Busy이며, 전체 확인 뒤 stable absent/duplicate는 Fatal이다. 마지막 allocator FrameStamp가 바뀌면 Busy다. copied inventory의 후보 Membership은 stamp를 재검산하지만, exact 참조 선택 뒤와 고정 Catalog root의 Membership은 원 참조 pin/고정 identity가 보존해야 하는 mapping 파손이므로 Fatal이다. typed page/Undo corruption을 정상 재시도로 낮추지 않는다. Status/Undo 읽기와 최종 결과 선택은 기존 동기 Catalog 의미를 유지하며 새 경로에서 WAL replay·durability wait·reader file·LOAD는 도달하지 않는다.

원 Full 계산식은41576B로 유지한다. 상주 계산은 실제로 호출하지 않는 Root/Jobs decoder와 WAL LayoutView/native parser의 포괄 비용을 제외하고, 실제 Status/CatalogRoot Undo 물리 read 반환 뒤의 typed decode를 별도 phase로 계산한다. guard·request·StatusSlotRef·TxnRef·UndoRef·PageView·Result 등 동시에 유지되는 parent 값은 합한다. malformed Undo의 CatalogValue before가 도달하는 완전한 Catalog decoder는 포함한다. source class는16860B, shared retained는22256B로 두 resident의 합은55976B다. 고정 caller loan과 원 shared backing은 재복사하지 않는다. 이는 source 값/수명의 보수 합이며 RSS·compiler stack 실측·성능 인수가 아니다. 다른 writer/reader/progress 입구를 포함한 전체64KiB 합산 보장은 아직 미공급이다.

시험은 마지막 검증 `ec018d6` 이후 변경한 class·resolver·오류 분류·source 계산에만 선택했다. 동결 manifest는 `tests/fixtures/dev3-module-c2/resident-read/manifest.json`이며 최초5개에서 검토 반례4개를 추가했고 완료된 기존 시험은 반복하지 않았다. 총600초, compiler90초, 각 exact case30초, 출력8MiB, stack33554432B로 제한했다. r1–r5의 source export/inventory·runner·manifest·compiler·ELF·stdout/stderr·결과를 각각 `artifacts/module-c2-resident-read-20261027-rN/`에 보존한다.

| 원 실행 | 실제 실행·결과 | 후속 변경과 미실행 구분 |
| --- | --- | --- |
| r1 | production PASS, test build E0583 FAIL | cfg(test) module의 path 등록 오류. runtime5개 모두 NOT_RUN |
| r2 | test build PASS, 첫 census FAIL(두 reader68820B) | production은 r1 이후 test-only path 수정뿐이므로 반복 안 함. 나머지 runtime4개 NOT_RUN |
| r3 | production PASS, test build E0502 FAIL | 실제 source phase 정정·Membership Fatal 수정과 추가 반례를 반영. test corruption closure의 borrow 오류. runtime9개 NOT_RUN |
| r4 | test build syntax FAIL | test helper 추출의 여분 brace 오류를 보존. runtime9개 NOT_RUN, production 반복 안 함 |
| r5 | test build PASS, exact runtime9개 모두 PASS | r3 뒤 test-only lifetime/syntax 수정. production 반복 안 함. 미실행·실패를 이 실행의 PASS로 소급하지 않음 |

9개는 고정 class/두-reader/mixed capacity1개, actual active·Commit 전후 snapshot/Found 무 I/O1개, Undo 비상주/Full 재시도1개, 두 actual reader의 독립 예약·반환1개, Status/Undo checksum 파손1개, 원 Status/후속 Status 비상주와 partial hit 거절1개, checksummed Status exact-ref 부재/중복1개, copy 뒤 actual allocator flush의 recLSN 변경1개, 고정 Catalog/선택 뒤 Status/Undo membership 파손1개다. 재생성된 손상 입력도 원 page identity와 checksum을 가진 원 pool frame에만 주입하며 새 권한·directory를 만들지 않는다. 정상 allocator stamp 변화 시험은 실제 flusher를 사용한다. 해당 원 flush 이벤트와 reader 재개 구간의 I/O0을 구분한다. native 파일·새 PID 복구·wire 변경은 이 단위에 없으며 기존 current-build 복구 결과를 새 결과로 표기하지 않는다.

| 실행 | source SHA-256 | result SHA-256 |
| --- | --- | --- |
| r1 | `acea3c9a894f0914e66f033ffec2cb91bbeb292777419f743afb64af37c0d3fe` | `83e6b2928a16be858ca237a63c390907f57ab28d0cdff91291c0d318d6b978f4` |
| r2 | `201b373df3b65232f34a2e0f9b546828f573f98c203b2c85911f9bb7a4bfacde` | `db8220e1347c222027283c7b390a21a0e1db304ff378713fe5d528cae0a2301a` |
| r3 | `a4dc78cc8104072166dd439ddee4fd15867698fc4969dc63364789c7b22ab49d` | `90c9bb36d4780dba22531775b55181be3b0acd6707ea621ce9902064705a5139` |
| r4 | `60c396a2076f94320e08cdd98a8ffe110b416eb55ce00272660a4fccd87fb6e8` | `4e3c7710947b34184ab2da8d57ad8804adad8b1cf2365539ee1c8558a4de8596` |
| r5 | `9e8793e70a36f3e8b8b0e9cc977d75ac24145e0c2013b05ee2f97e0051bb69b5` | `9ab76baa1313927e17ff187b7253f2697d3ccea134a7cd42633bab11acc18eb3` |

r5 ELF SHA-256은 `551330563e88e9eeb598398fe18c4bdd2267a9371fcbea9cefa113c3f3e0a50f`, r3–r5 manifest는 `83adeaa0b04f636bd1d502b3020ec5705fa0ca821d863d939343656b639e433e`다. 변경 Rust만 파일별 rustfmt로 정리했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 독립 검토의 최종 결과는 아래에 기록한다. 재검토 조건은 resolver의 실제 호출 경로, 원 참조 보존/allocator 변경 의미, class source census 또는 caller loan 배치 변경이다.


`/root/module_review`는 실제 제품·source census·9개 시험/manifest와 r1–r5의 원 자료를 읽기 전용 감사했다. r1/r2 각5621개, r3–r5 각5622개 inventory의 source·fingerprint·manifest·runner·compiler/case 로그·존재 ELF를 대조했고, r3→r4→r5는 cfg(test) resident_faults.rs만 바뀌었음을 확인했다. 문서6개·신규4/수정10경로 등록까지 확인한 뒤 **C2 상주 Catalog 부분공급 D1/D2 한정 수용·필수 수정0**으로 판정했다. 검토자는 수정·빌드·시험·문서 verifier를 실행하지 않았고 `/root`가 한정 수용을 반영한다. writer/progress·다른 입구 합산·Row/terminal/recovery·M4/M5 완료로 확대하지 않는다.

<a id="c2-source-return"></a>
### C2 최종 source 반환까지 원 read progress 유지 — 2026-10-27

기준 `788702c`, 동일 작업 공간·브랜치다. **D3-MODULE-SOURCE-RETURN-001 · D1/D2 한정 수용 / production·test build·선택 runtime7 PASS / 전체 source progress 미완료**다. 작성·통합 `/root`, 별도 검토 `/root/module_review`이며 기존 계획 완료 지시 범위의 한정 수정이다.

writer/progress 공급을 준비하며 admitted lookup의 refs/progress가 마지막 decode/Result와 source 반환보다 먼저 끝남을 확인했다. 이 순서로는 read_operation0을 late source0으로 사용할 수 없다. 실제 `CatalogReadOperation`은 `close_refs`와 `close_source`를 나누고 **refs 정리→최종 decode/Result→source checked 반환→progress checked 반환** 순서를 사용한다. Found/Missing/오류·early `?`·Drop·진입 reference 예약 실패에 같은 순서를 적용한다. source 반환 실패도 progress 정리를 건너뛰지 않으며 fatal 효과/원 charge 보존은 그대로다. no-effect source 선예약과 실제 progress 진입 순서는 이번에 바꾸지 않았다.

이 수정은 이미 진입한 lookup의 late source 수명만 닫는다. **source 예약→progress 진입 사이의 틈, 반환 직후 caller의 일반 Result carrier, 다른 입구의 미계수와 writer/progress 원자 인수는 남는다.** 따라서 전체65536B progress 인수가 아니며 기존 Pending reader/Full 동작·same-snapshot retry·wire는 유지한다. owned field/배열/Result type이나 census 식은 바꾸지 않았다. 기존 held4개 Result/2개 오류와 decoder/consumer 반환 계수가 같은 carrier를 덮으므로 이전 type-size/census 시험을 반복하지 않았다.

`tests/fixtures/dev3-module-c2/source-return/manifest.json`은 마지막 검증 뒤 실제 cleanup diff에 해당하는7개만 고정했다. 신규4개는 Full/Resident의 refs0·pin0 뒤 실제 Commit Drain 대기, fence 뒤 source 잔여와 progress 반환, Found 최종 decode 구간의 표시 유지, 그 구간의 panic/Drop이다. 기존3개는 변경된 finish_error/Drop을 실제로 소비하는 Full Undo I/O 실패, captured refs를 가진 fenced Drain, Resident Status/Undo typed corruption이다. 기존 사례의 새 실행은 cleanup 코드가 바뀐 범위에만 해당하며 이전 결과는 원 source대로 유지한다. 수정 없는 하위 wrapper·전체 workspace·native/새 PID 복구는 실행하지 않았다.

원 증거는 `artifacts/module-c2-source-return-20261027-r1/`에 있다. 고정600초/컴파일90초/각 case30초/출력8MiB/stack33554432B 범위에서 production check·test build와7개 exact 사례가 한 번에 PASS다. 경과44.15초는 성능 결과가 아니다.

- source SHA-256: `4be9ff3a89ebac984735e92c952cd20311bdfe4164e4d221d4a794ef73bd7c69`
- manifest SHA-256: `d00d7ca2167ec2ae0c51ede4c51dd4bda3a02081659db9d5768d4706eebccd80`
- ELF SHA-256: `1088ca67daf0c247ae23b97fb9e13a788f058c265fe0bc49a34898099954b123`
- result SHA-256: `39f77f4435a72a80f5ef5675f86a78ec56a8b7a25eba0a03a1b44f4d80a9fc9f`

파일별 rustfmt와 `git diff --check`만 정리에 사용하며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. `/root/module_review`가 실제 diff·시험·manifest 및 원 source5624개·fingerprint·runner·compiler/case 로그·ELF hash를 읽기 전용 대조했고 문서6개와 등록 신규2/수정3경로까지 확인했다. **이 source-return 단위 D1/D2 한정 수용·필수 수정0**을 수신해 `/root`가 반영한다. 검토자는 수정·시험·빌드·문서 verifier를 실행하지 않았다. 전체 writer/reader/progress 합산과 Row/terminal/recovery·M4/M5는 계속 미완료다.


<a id="c2-commit-phases"></a>
### C2 Commit phase source 관측 — 2026-10-27

기준 `883c920`의 실제 타입과 수명 경계를 cfg(test) 함수로 관측했다. 제품 admission·기존 source_bytes31752B·효과 경로는 변경하지 않았다. 작성·통합 `/root`, 별도 검토 `/root/module_review`다. 다음 writer 예약을 설계하기 위한 유한 관측이며 새로운 actual Commit 실행 또는 전체64KiB 인수 결과가 아니다.

shared22256B, Full41576B, Resident16860B를 유지한다. 현재 PreparedCatalog는 실제9-entry requests/roles/stamps를 보유하여2024B다. Pending에서 동시에 살아 있는 PreparedCatalog·원 PendingCommitTargets4·callee의 CatalogCommitWait 세 값만 합한 필요 하한은2984B이고, shared+Full과 합66816B다. 다른 parent/helper를 제외한 이 값만으로도 현재 layout에서 Full과 Pending의 동시 source65536B 보장은 불가능하다. 이는 source 값의 필요 하한이며 stack/RSS 최소치가 아니다.

| phase/값 | source B | shared+Full B | shared+Resident2 B |
| --- | ---: | ---: | ---: |
| PreparedCatalog 실제9-entry 값 | 2024 | 65856 | 58000 |
| 준비 공통9-entry 보수 상한 | 40456 | 104288 | 96432 |
| pre-stage | 13784 | 77616 | 69760 |
| stage-return | 27140 | 90972 | 83116 |
| Pending 세 값 필요 하한 | 2984 | 66816 | 58960 |
| Pending WAL wait 보수 상한 | 10576 | 74408 | 66552 |
| Drain wait | 7876 | 71708 | 63852 |
| resume/install/finish | 24220 | 88052 | 80196 |
| publish/return | 12992 | 76824 | 68968 |
| 변경 없는 제품 포괄 상한 | 31752 | 95584 | 87728 |

세 값 하한과 타입 크기를 제외한 phase 값은 기존 포괄 leaf·반환 carrier를 포함하는 보수 상한이다. Pending wait의 두 Resident 합66552B는 그 상한이1016B 초과한다는 관측이며 실제 최소 불가능 증거가 아니다. 실제 layout/helper 수명을 먼저 줄이고 재계수해야 하며, class를 실행 중 확대하거나 필수 progress를 선택적 Capacity로 바꾸는 근거로 사용하지 않는다.

[실행 전 manifest](../../tests/fixtures/dev3-module-c2/commit-phases/manifest.json)는 신규 관측 selector1개만 선택했다. 총600초·compiler90초·case30초·출력8MiB·stack33554432B다. `artifacts/module-c2-commit-phases-20261027-r1/`은 test build E0425/E0433 FAIL, runtime NOT_RUN이며 잘못된 WalPrefix 한정과 private module 경로를 정정했다. r2는 test build와 신규 관측1개 PASS다. 두 실행 모두 cfg(test)만 바뀌어 production build·기존 제품 시험은 반복하지 않았다. 원 FAIL·export·inventory·compiler·ELF·로그·result는 보존한다.

| 실행 | source SHA-256 | result SHA-256 |
| --- | --- | --- |
| r1 FAIL | `5a313baaac99347162b1438dc1714a285871701ca4a15694719c0f3890002db2` | `98bf22b12d15b67c6cbbcfcfa5121ac83cab1459d3625112d5fbb472db8f3d6b` |
| r2 PASS | `97392730cea35a092410994a10c3f08faba2c9d41eb4dc049faff1fa1b4f99e4` | `752f9bcf284f6057382c1eaa84821a025c5398e059c7e6f91343df557adf058f` |

manifest SHA-256은 `45af3e9e36ec38aac1bbe4ec65edcb376d60396b6ee1c318fb8ccd27dff14efc`, r2 ELF는 `4fe0d26e03890679100f937336f707e4c9d3788580b3c081c46fde980bbdf2ce`다. 경과15.27/25.85초는 성능 결과가 아니다. 변경 Rust의 파일별 rustfmt만 적용하고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. `/root/module_review`는 실제 diff·문서6개·r1/r2 각 source5625개와 fingerprint/inventory/manifest/runner/helper/로그/존재 ELF를 읽기 전용 대조하고 **이 관측 단위 D1/D2 한정 수용·필수 수정0**으로 판정했다. 검토자는 수정·빌드·시험·문서 verifier를 실행하지 않았고 `/root`가 반영한다. 다음 단위는 stage 이후 필요 없는 준비 배열의 실제 수명 종료이며 source-before-progress 진입 틈·writer/progress 전체 합산·Row/terminal/recovery·M4/M5 의무는 유지한다.


이 절의 manifest와 selector는 당시 finish source의 동결 기록이다. 후속 [Pending image 수명 분리](#c2-pending-image)에서 관측 selector를 `catalog_commit_phase_sources_measure_consumed_preparation`으로 바꿨으므로, 예전 manifest를 현재 main의 실행 목록으로 사용하지 않는다. 원 source·선택·결과는 그대로 보존한다.

<a id="c2-pending-image"></a>
### C2 stage 이후 준비 배열 수명 종료 — 2026-10-27

기준 `4403c91`의 다음 유한 단위다. 작성·통합 `/root`, 독립 검토 `/root/module_review`이며 기존 완료 지시 범위에서 진행했다. PreparedCatalog를 private immutable CatalogPayload와 준비 전용 owner/requests9/roles9/stamps9/before로 나눴다. actual stage_catalog가 PreparedCatalog 값을 소비하고 원 mark_staged→suspend 성공 뒤 payload와 기존 after(at)로 뽑은 canonical after 참조4개만 반환한다. Allocation의 proof-only None과 원 target 순서를 그대로 보존한다. 추출은 infallible이며 Intent 전 Commit/target count·source/epoch 검산을 유지한다.

새 CatalogCommitImage는 Clone/Copy·raw 생성자·Prepared 재구성을 제공하지 않는다. 원 pin/StablePage/Wait/설치 영수증이 권한을 계속 소유하며 새 backing·manager·페이지 복사를 만들지 않는다. 공통 catalog_apply는 같은 payload의 requirement/control/pending/layout을 검산하는 단일 본문이다. Begin/Reserve/Forward와 retirement/CLR/Abort도 같은 필드 값을 그 본문에 넘긴다. 제품 wire·원 snapshot/Q·Pending LOAD·Drain 의미는 유지한다.

| 실제 source 관측 | 이전 B | 변경 후 B |
| --- | ---: | ---: |
| PreparedCatalog 전체 | 2024 | 2024 |
| Pending에 남는 준비 자료 | 2024 | 568 |
| Pending 세 값 필요 하한 | 2984 | 1528 |
| Pending WAL wait 보수 상한 | 10576 | 9120 |
| shared+wait+Resident2 | 66552 | 65096 |
| stage 반환 보수 상한 | 27140 | 28308 |
| 기존 방식의 제품 포괄 상한 | 31752 | 32920 |

stage 반환에서는 원 Prepared·compact image·tuple Result·추출 참조 배열32B의 보수 중첩을 계수하여 비용이 늘었다. prepare 공통40456B와 live peak63184B/metadata186192B/whole251728B, recovered peak64611B는 유지한다. shared22256B와 새 세 값 하한+Full 합은65360B이므로 과거 source의 최소 불가능 결론을 새 source에 연장하지 않는다. 반대로 wait 상한+Full72952B는 아직 초과하며, Resident2 대기 합65096B도 **전체 writer class·고정 예약·Full 동시 허용·aggregate 인수 증거가 아니다.** phase별 실제 수명과 class 경계·필수 progress 예약은 후속이다.

[실행 전 manifest](../../tests/fixtures/dev3-module-c2/pending-image/manifest.json)는 이4개 Rust diff에 해당하는12개만 선택했다. phase/layout2, common Begin/Reserve/Cancel·Forward/CLR/Abort2, 실제 Pending flush·Full Undo LOAD·Drain/fence3, Q 공개1, stage unwind·wait write 오류·durable unwind/현재-build memory recovery·partial install/현재-build memory recovery4다. 전체 workspace·변경 없는 하위 시험·native/새 PID 복구는 실행하지 않았다. 총600초·compiler90초·case30초·출력8MiB·stack33554432B다.

`artifacts/module-c2-pending-image-20261027-r1/`의 production compile은 E0599 FAIL(옮긴 inventory_after 호출의 payload 경로 누락), test build/runtime12는 NOT_RUN이다. r2는 누락 경로·최소 가시성과 추출 배열의 source 계수를 반영한 source에서 production/test build와 선택12개 각각1회 PASS다. 원 export/inventory·manifest·runner·compiler·ELF·case 로그·result를 보존하고 r1을 소급 PASS로 바꾸지 않는다.

| 실행 | source SHA-256 | result SHA-256 |
| --- | --- | --- |
| r1 FAIL | `c860f359ebc7a38ed55e1d38b4eacabd6a09512b9a03ae7d1d77e4de4a3b9acc` | `dcb9397abe0f314462626f639ac8bf0bf5aca991c0ad43689477470a7d8cb035` |
| r2 PASS | `a29fe07f0e107222dbf1ba17530784ba58c6107a4da2d2965dd72b9d1c6a1f12` | `c4682806c9bd0e45b36ee4010e2c9847087182aa51a260f75d17d0b4159a64b4` |

공통 manifest SHA-256은 `31d1025a5ada9df51631ce41a39b379c47c1346bb72191aa6d0df807d58bf5d2`, r2 ELF는 `660560b76212f5391c37ef28353db062fb42673be225a9214cfaa4a3ef8c1618`다. 경과5.81/71.57초는 성능 결과가 아니다. 변경 파일별 rustfmt·diff whitespace 확인만 했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. `/root/module_review`는 실제4개 Rust diff·선택 manifest·문서6개와 r1/r2 각 source5626개·fingerprint/inventory/runner/helper/compiler/case 로그·존재 ELF identity를 읽기 전용 대조했다. 준비 자료의 감소와 전체 tuple 반환을 구별하고 과거 충돌의 source를 명시하는 두 표현을 정정했다. **이 수명 축소 단위 D1/D2 한정 수용·필수 수정0**을 수신하여 `/root`가 반영한다. 검토자는 수정·빌드·시험·문서 verifier를 실행하지 않았다. 원 source-before-progress 진입 틈·writer/reader/progress 전체 계수·Row/terminal/recovery·M4/M5는 남아 있다.


<a id="c2-source-entry"></a>
### C2 source 예약 시도 전 원 read progress 진입 — 2026-10-27

기준 `a5cfdae`, 작성·통합 `/root`, 별도 검토 `/root/module_review`다. 이전 short-source 공급의 source→progress 진입 순서를 **무효과 precheck→원 try_read_operation→고정 source 예약→refs**로 변경한다. try_read_operation 내부의 원 core 잠금은 호출 반환 때 끝나며 이후 source의 짧은 isolation leaf와 중첩하지 않는다. 새 mutex·콜백·capacity 대기·core API를 만들지 않는다.

원 try_read_operation이 진입 선형화 지점이다. Drain이 먼저면 InstallationPending을 source 예약 없이 반환하고, reader가 먼저면 source0인 예약 시도 구간도 flag1로 Drain이 기다린다. source 부족은 refs/pin/물리 효과 전에 progress를 checked 반환한다. 정상 정리 때 Capacity/Unchanged를 보존하되 cleanup의 fence/poison/불변식 실패가 겹치면 기존 merge의 Fatal 우선순위를 따른다. progress를 먼저 선언하여 source 전 panic/early-return에서도 원 Drop을 거친다. 이미 구현한 source→progress 종료 순서와 합쳐 healthy read의 source가 flag0으로 남는 진입/종료 틈을 닫는다. fatal fence 아래 charge 보존은 그대로다.

원 source-first 계약의 ‘부족이면 progress 표시도 하기 전’ 부분만 이 단위에서 재개방했다. 오류 출구의 progress0·refs0·pin0 및 물리 Unchanged를 보존한다. 타입/필드·고정 class/charge·source census는 바뀌지 않으며 현재 entry의 기존 operation/Result/오류 상한 안에서 순서만 바꾼다. **일반 caller return·예약 전 entry carrier 전체합·다른 writer/read 입구·필수 progress 예약을 포함한65536B aggregate 인수는 여전히 미완료**다.

[실행 전 manifest](../../tests/fixtures/dev3-module-c2/source-entry/manifest.json)는 실제 진입 순서와 실패 branch diff에 해당하는5개만 선택했다. 새3개는 각각 Full/Resident의 source 전 actual Drain 대기, fence로 source 예약 실패/flag 정리, source 전 panic/Drop이다. 기존2개는 다른 Full의 charge가 남은 Capacity 실패와 실제 Drain에서 source 부족보다 InstallationPending 우선인 입구다. 종료 순서·타입/census·변경 없는 하위/전체 workspace 시험은 반복하지 않았다.

`artifacts/module-c2-source-entry-20261027-r1/`에서 production/test build·선택5개 각각1회 PASS다. 총600초·compiler90초·case30초·출력8MiB·stack33554432B로 제한하고 원 source export/inventory·manifest·runner·compiler·ELF·case 로그·result를 보존한다. 경과38.43초는 성능 결과가 아니다. native/새 PID 복구·wire 변경은 없다.

- source SHA-256: `e9a4766a93a27a5e65f37e34f8bacecbb2499f6cc453e27e4d74147f6281b11a`
- manifest SHA-256: `f17f5642a619f40693309e391e91bfbd3fbfb9574f55480fece56b3097ee12aa`
- ELF SHA-256: `55122e45fc48877994e84103bde590c926dc9221082072976816db81f3fa0855`
- result SHA-256: `e5504c391e9c8c98eff91cbfa5a2093cc0f2d4b39ea844410c9c749e2f89e3f7`

변경 파일별 rustfmt와 diff whitespace 확인만 수행했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. `/root/module_review`는 실제 diff·문서6개·원 source5628개와 fingerprint/inventory/manifest/runner/helper/컴파일·case 로그/ELF identity를 읽기 전용 대조했다. **이 진입 수명 단위 D1/D2 한정 수용·필수 수정0**을 수신해 `/root`가 반영한다. 검토자는 수정·빌드·시험·문서 verifier를 실행하지 않았다. writer/read/progress class 전체 공급·Row/terminal/recovery·M4/M5 의무를 계속 진행한다.

<a id="c2-aging-source"></a>
### C2 Aging/Root 실제 호출의 source 계산 — 2026-10-27

기준 `d9d6a01`, 작성·통합 `/root`, 별도 검토 `/root/module_review`다. 기존 Aging codec의 순차 호출을 실제 수명에 맞춰 계산한다. encode는 Root2 decode→검산→work hash→XOD encode, decode는 Root2 decode→XOD decode·검산→work hash다. 전체 Job queue census 대신 Root2 decoder의 상한을 사용하되 initial Root의 canonical512+initial512+Job1024와 후속 rows640, malformed Transaction XOD가 거치는 XTS decode, parent/Result/반환 복사와 고정 SHA closure를 포함한다. 공통 XOD/XTS 식은 lower owner로 이동하고 XOD validate의 by-value RemainingBudget5 IntoIter·현재 row를 보완한다. 실제 codec·WAL·효과 경로와 backing/wire는 변하지 않는다.

Root decoder6540B, 기존 전체 queue6736B, XOD8711B, XTS1411B, Aging11575B다. XOD 보완으로 transaction inventory/encode/next/validate는 각각2224B 증가하고, 실제 Aging predecessor/next는14853/15675B가 된다. Full 조회 고정 charge는41576→37724B, Resident16860B와 shared22256B는 그대로다. live peak63184B·metadata186192B·whole251728B와 recovered peak64611B는 유지한다. **shared22256+Full37724+Pending wait9120=69100B로3564B 초과**하므로 Full/Pending 동시 공급이나 전체 admission 완료로 해석하지 않는다.

[실행 전 manifest](../../tests/fixtures/dev3-module-c2/aging-source/manifest.json)는 source 계산 diff에 해당하는 신규 관측·owner 합산·고정 class admission3개만 선택했다. `artifacts/module-c2-aging-source-20261027-r1/`에서 production/test build와 exact3개 각각1회 PASS다. compiler의 unused imports(ControlRef/OwnerState) 경고는 원 로그에 보존한다. r2는 두 unused import 제거와 해당 파일 rustfmt만 반영하여 production/test build PASS·경고0이며 runtime은 `NOT_REPEATED_UNCHANGED_CODE`다. r1의 runtime 결과를 r2의 새 실행으로 표시하지 않는다. 총600초·compiler90초·case30초·출력8MiB·stack33554432B이며31.29/31.08초는 성능 결과가 아니다.

| 원 증거 | source SHA-256 | result SHA-256 | ELF SHA-256 |
| --- | --- | --- | --- |
| r1 runtime3/compile | `575a9156256068f58b855a6a0609c327a52c82ad728a819f68c1e9ce4227823f` | `95712ade5eb3272ac3263ad209fa4eaa64f48802a1840cacc2c0ed32a484ae6d` | `60f6f15af747d018d806c637be9221f04a4d12aaf0f6fc7f4837d9c22f469c27` |
| r2 import-only compile | `455f41485cf72b6945defb92dd80941e8dd46aba67b1e9a3c0e45ec646a016d7` | `55d5149365497ce54567027817f5bcb2b67c1b77ba90284403bb82c6a3ddec0d` | `c3915529bc440cb915b7bc15d0d3b5b6f4e9d4ebda98c1112911f815b1738215` |

공통 manifest SHA-256은 `b5c726d4ecd53f3feae1a9273bfa67104725401ba06fd7642e0a3b253c9e21b1`이다. 원 export/inventory·runner/helper·compiler·case 로그와 결과를 보존한다. 변경 없는 codec 동작·하위/전체 workspace·native 복구 시험은 반복하지 않았고 전체 fmt/Clippy·문서 verifier·push도 실행하지 않았다. `/root/module_review`가 source5629개·r1/r2 identity·compiler/ELF/case 로그와 문서6개를 읽기 전용 대조했다. decode/encode 순서 설명1곳을 고쳐 반영했으며 **이 계산 단위 D1/D2 한정 수용·미해결 필수 수정0**이다. 검토자는 수정·빌드·시험·문서 verifier를 실행하지 않았다. writer/progress 실제 phase admission·다른 entry/return carrier·Row/terminal/recovery·M4/M5는 계속 미완료다.

<a id="c2-wal-wait-source"></a>
### C2 WAL 대기의 실제 phase source 관측 — 2026-10-27

기준 `c4b9b6f`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. cfg(test) 관측만 추가했다. 제품 WAL 경로·기존 DURABILITY_CALL_BYTES4096·고정 source class/charge·admission은 바꾸지 않았다. physical wait의 state/predicate/기초 I/O/claim/ensure/condvar, semantic bridge의 physical 호출/receipt 반환, 실제 Catalog Pending parent/Result 변환을 분리한다. FileIo 경계의 XADB source 값이며 backend/OS stack·RSS 추정이 아니다.

wait와 ensure 각각의 UnwindFence, claim의 IoFence·slot borrow·frame은 해당 nested 호출 동안 유지한다. ensure의 append guard block은 sync_claim 전에 끝나고 claim의 file block은 마지막 append publication 전에 끝나므로 그 안쪽에만 max를 쓴다. requirement_ready의 복사 frame/prefix·pending closure·SlotState·interval/Option/오류, foundation의 short-write/Interrupted/오류 결과, condvar의 LockResult/변환/next guard를 포함한다. 물리 slot/File/AppendState backing은 원 owner의 빌림이다.

r1은 test build·신규 관측1 PASS지만 bridge에서 아직 생성되지 않은 semantic receipt까지 물리 대기와 합산했다. 독립 검토가 Catalog caller의 WalResult→RetentionResult→DurableCommit 추출 handoff를 명시하도록 요구했다. r2는 실제 초기화 순서에 따라 bridge call/return을 max로 분리하고, caller handoff를 별도 상한으로 추가했다. 원 r1 결과를 지우거나 r2 증거로 재사용하지 않는다.

r2 물리 관측은 state336/predicate710/foundation_io544/sync_claim1160/ensure_durable2248/condvar1046/wait_durable2808B다. Pending parent2064B, bridge3256B, handoff1216B이므로2064+max(3256,1216)=5320B다. shared22256+Full37724+5320=65300B, Resident2 합61296B를 관측했다. **236B 여유는 새 source ticket·progress 전환·입구/종료 전체 비용을 포함한 인수 결과가 아니다.** 기존 포괄 Pending 관측9120B와 생산 상계·charge는 그대로이며5320B를 실제 예약에 사용하지 않는다.

[동결 manifest](../../tests/fixtures/dev3-module-c2/wal-wait-source/manifest.json)는 신규 `catalog_wal_wait_source_phases_include_claim_and_condvar`1개만 선택했다. `artifacts/module-c2-wal-wait-source-20261027-r1/`와r2는 각각 test build·exact1 PASS, 경고0이다. r2 재실행은 관측 계산/반환 handoff diff에 한정한다. production build는 `NOT_RUN_CFG_TEST_ONLY`이며 변경 없는 기존 동작·하위/전체 workspace·native/복구 시험은 반복하지 않았다. 총600초·compiler90초·case30초·출력8MiB·stack33554432B이며26.40/26.25초는 성능 결과가 아니다.

| 원 증거 | source SHA-256 | result SHA-256 | ELF SHA-256 |
| --- | --- | --- | --- |
| r1 관측 | `2b23512bc13499235749047c76b7cefefb485995e23186aa0f08a8baceb9ec66` | `16ab99347a38f51db031d29f5f3f9931b5f35a3179783bdeee18c14680ac555f` | `ac09f831c72d5c51dccfbcba90dfbc0f4ae952e7b21bb1355914496a8a0962c5` |
| r2 반환 경계 보완 | `c3cbb046a02e47eb1d856814d4e8a1b51416d26396edd45a17fdc72ff87a83b4` | `fc2e57394e2349cbffc90be58100ac63ab7483fc00a49a836456e55a22046241` | `1d1a3fa6318c6504ddd3631471e6985fb32d56fae02ac42c8f2e922a25bb0e6b` |

manifest SHA-256은 `3dfa6a3e5db57048d15decfabaa5045c9a6690f5bb2d10b966e164a371d9ced5`다. 독립 검토자는 각 source5630개·원 identity/로그/ELF를 읽기 전용 대조하여 코드/원 증거를 한정 수용했다. 문서6개의 최종 대조까지 **이 cfg(test) 관측 단위 D1/D2 한정 수용·필수 수정0**을 수신했다. 검토자는 수정·빌드·시험·문서 verifier를 실행하지 않았다. 변경 파일 rustfmt와 diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 실제 writer phase/progress 예약·전체 entry/return 합산·C0–C5·M4/M5는 미완료다.

<a id="c2-drained-handoff"></a>
### C2 실제 Drain 완료와 guard 재구성의 소비 경계 — 2026-10-27

기준 `dfc3bc9`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. 원 `PendingCommitTargets`의 authentic durable begin_drain→wait_ready를 `drain_durable`로 분리하고 같은 pins/StablePage와 원 CommitInstalling을 private·nonClone `DrainedCommitTargets`에 이동한다. actual Catalog는 이 값을 받은 뒤 `resume`로 소비하여 원 WriteTargets guard storage를 재구성하고 기존 stamp/digest/recLSN 검산을 수행한다. 새 등록·pin 재예약·authority 생성은 없다.

Drained 시점은 **Installation::Installing·active_apply1·원 version gate+pins4·content guard0·read flag0**다. Pending/Draining을 임의로 건너뛰거나 새 상태로 위장하지 않는다. 후속 source 전환이 들어갈 위치를 만들었을 뿐 아직 source ticket/upgrade는 연결하지 않았다. 이미 원 version gate와 pins가 있으므로 그 위치의 전환은 reserved 몫에 대한 무대기 checked leaf여야 하며 capacity를 기다릴 수 없다. Drop은 원 installation을 먼저 drop하여 fence/Abandoned·version 반환을 마친 뒤 같은 pins를 반환한다.

새 owned Drained·physical Result2와 Catalog의 mapped RetentionResult를 source 식에 추가하여 포괄 Commit 상계32920→36184B(+3264), drain 포괄관측6420→9684B다. 기존 live peak63184B·metadata186192B·whole251728B와 recovered peak64611B는 유지한다. 관측 prepare36604B는 앞선 Aging 계산 반영분이며 이번 소비 경계의 감소가 아니다. drain 포괄관측은 반환 carrier까지 합한 상한으로, reader가 살아 있는 대기 phase의 실제 예약이나 전체 동시 인수로 사용하지 않는다. Pending 기존포괄9120B와 앞선 실제 wait 관측5320B도 이번에 변경하지 않았다.

[동결 manifest](../../tests/fixtures/dev3-module-c2/drained-handoff/manifest.json)는 신규4·직접 영향4·변경 source2의10개를 선택했다. 신규4는 healthy handoff의 pins/version 유지·content0/원 before bytes, handoff fence, 정상 unfinished Drop, handoff panic이다. **정상 Drop은 외부 fence/panic 없이** 원 pin-return hook에서 이미 fenced/Abandoned·active0·version 반환·pins4를 관측하고 그 뒤 pins0/Q0를 확인한다. hook은 실제 값을 drop하도록 요청할 뿐 receipt/설치 권한을 만들지 않는다. fence/Drop/panic은 원 frozen C04/N03 기대값과 현재-build memory recovery를 사용했다. 기존 MT03 Full Undo LOAD·MT05 lower flush·MT07 Drain fence·MF10 부분 설치 및 source 합산2만 새 경계의 영향으로 선택했고 무변경 하위/전체 workspace는 반복하지 않았다.

`artifacts/module-c2-drained-handoff-20261027-r1/`에서 production/test build·exact10개 각각1회 PASS, compiler 경고0다. 총600초·compiler90초·case30초·출력8MiB·stack33554432B이며77.86초는 성능 결과가 아니다. 원 export/inventory·manifest·runner/helper·compiler·ELF·case 로그와 결과를 보존한다. wire/native/새 PID 복구 변경·실행은 없다.

- source SHA-256: `8704cd09d5bcaa065937131a680b4099014c37214b76ab562f8b97379a814836`
- manifest SHA-256: `c47fc25d38fd8ccffea88ea538cf31ff84f08db0a2e65ab98fe1e33d0e9ab7bd`
- ELF SHA-256: `faf4fd4d1b5c7ff587b5bee9d235ce81ffed6d784295fb5da412206cf12dcf85`
- result SHA-256: `b2b597cfdcb76566a53408216adaa8b89d07365f76b801da1320f38e555dfb04`

독립 검토자는 실제 값 이동·권한·오류/Drop 순서, 선택10의 원 artifact와 최종 문서6개를 대조하고 D1/D2를 한정 수용했다. 필수 수정0이며 source5632개와 증거 식별자·수치를 확인했다. 변경 파일 rustfmt·diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 실제 source phase 예약·선택 caller의 entry/exit 전체 상계·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-call-scope"></a>
### C2 Catalog 결과 소비까지 유지하는 원 CallScope — 2026-10-27

기준 `00feccd`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. [소유 수명](../contracts/dev3-module-boundaries.md#operation-source-admission)에 따라 private non-Clone CatalogCallScope가 원 Owner/backend/snapshot과 source/progress를 소유한다. 기존 operation은 그 source를 borrow하고 refs/lease만 정리하며 Catalog 제품 분기는 단일 본문이다. 실제 selected `catalog_memory::query`는 Found의 header/Table/Column 소비와 Missing/FlushRequired 분기를 마친 뒤 scope를 닫고, 필요한 flush는 그 후 수행한다. legacy entry는 lease 실패까지 내부 Result로 받아 같은 checked cleanup을 수행하지만 반환 이후의 caller 소비는 새 보장 밖이다.

source/progress는 이제 결과를 사용하는 동안에도 남는다. scope의 정상 오류 완료는 원 Fatal을 보존하고 Unchanged보다 cleanup Fatal을 우선한다. source 실패에도 progress를 반환한다. Scope Drop은 source→progress 순서이며 Operation Drop은 refs만 정리한다. schema/wire/backing·원 authority·실제 WAL/설치 의미는 바꾸지 않았다. statement factory/자동 read-set이나 전체 writer/progress admission 완료를 주장하지 않는다.

[동결 입력](../../tests/fixtures/dev3-module-c2/call-scope/manifest.json)의 r1은 production/test build·runtime14·compiler4 PASS/경고0다. 새 runtime4는 Found 복사/nested view 소비 중 admission 유지·순차 lookup 재사용/경쟁 Full Capacity, 반환 Missing scope에 대한 실제 Drain 대기, 정상 오류 cleanup Fatal, normal Drop/panic이다. 변경된 기존 admission/lease/refs/legacy cleanup과 query에 직접 관련된10개만 추가 선택했다. compiler는 정상 소비→finish1과 copied RecordView/column.name의 finish 이후 사용(E0505 각각1), 살아 있는 view 중 Owner 재진입(E0499)의 거절3을 exact code/primary span으로 확인했다. negative compiler 거절은 의도한 PASS이며 제품 컴파일 실패가 아니다.

독립 검토 후 선택 caller 소비 phase를 명시적으로5256B로 계산하여 production class에 순차 max를 추가하고 실제 오류2개를 보완했다. r2 production/test build와 실제 resident Undo Busy→fence→finish_error의 Fatal 승격은 PASS다. I/O 오류 사례는 선행 이벤트 이력을 비우지 않아 지정한 nth1 fault가 걸리지 않았고 lookup이 Ok여서 `.err().unwrap()`에서 FAIL했다. 원 FAIL/출력/source를 보존하며 뒤의 새 source 관측은 NOT_RUN이었다. r3는 이 **I/O fixture branch에 events.clear() 한 줄만 추가**하고 실패했던 I/O 1개와 미실행 source 관측 1개만 실행해 합계2 PASS/경고0다. 실제 Undo I/O Fatal의 Load/PageIo 원인을 cleanup Fenced가 덮지 않으며 최종 progress0·fenced source 보존을 확인했다. production은 r2 이후 test-only diff로 NOT_RUN, 완료한 r1 14/4 및 r2 Busy는 재실행하지 않았다.

현재 Scope held392B·entry1944B·선택 consumer5256B, Full38076B·Resident17212B다. 두 class가 이전보다352B 늘었고 shared22256B와 Full 합60332B, Resident2 합56680B다. live peak63184B·metadata186192B·whole251728B와 recovered peak64611B는 유지한다. Pending 관측5320B를 합한65652B는116B 초과이며 이전236B 여유를 현재 admission으로 사용하지 않는다. source 예약 전 bound entry와 source 반환 뒤 tail·foreign Owner 거절의 전체 동시 상계는 미공급이다. consumer의 finite source max가 현재 두 class보다 작다는 관측은 임의 caller의 무제한 복사 보장이 아니다.

원 artifact는 `artifacts/module-c2-call-scope-20261027-r{1,2,3}/`에 export/inventory·compiler/probe exports·ELF·manifest·runner/helpers·case 로그를 보존한다. 각 실행은 총600초/compiler90초/case30초/출력8MiB/stack33554432B로 제한했다. 경과74.01/33.90/28.22초는 성능 결과가 아니다. wire/native/새 PID 복구 변경·실행은 없다.

| 실행 | source SHA-256 | manifest SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- | --- |
| r1 PASS | `cd68674992ef61ba991794cc6e26771240de745e0dcdfc50f9f58946ea63c03c` | `4b365a2dbed11a296d980a804038c5f1365d5cc54a4183e5c2cab2a3d360dda2` | `77d6f38c46a1ca0d8aaba0bdec9408086b00a34ebe44ae043a5adac9caa16bd3` | `1403c56fc486dc8189b14c9e9efbb93b2d6b8f02023581b55bb33e84dee24180` |
| r2 FAIL·Busy PASS·관측 NOT_RUN | `2162e5b164dd7b1d9f0627e7c5b23857248d9392d04d92b1f1289f41b402d6e6` | `01797e1d7a552a8bb4bd214f157ccf5642c239ae7872f15eac3c75b8d9cf8505` | `40531aada6fc537cbbf4dbad6a60b42e054966be05bf6c2dca5403fbd81de89d` | `28ba11f8e3d0fcbd73ed303e427b5806e04bd370d103980761f16f1a4aa98623` |
| r3 실패 I/O 1개 수정·미실행 관측 1개 PASS | `d774709877c8793aa602b82f5814c582bd957585bfabb23957616a09235864cd` | `0bb4c8b9a95f0935204919ec532a8431437640e087407a1a4aea6170ee82bb7f` | `45df418dd24e44c4833a411be32a4bc33a6a5f123e49ae88e930f184657aef55` | `7a39e8ca1ce39ef67a92c0534cc3271506276a3f5cbb8972bc0a20ecfda4ae05` |

독립 검토자는 코드·r1–r3 각각의 source5639파일/원 artifact·최종 문서6개를 대조하고 D1/D2를 한정 수용했다. r3 실패/미실행 개수 문구를 구별하도록 정정했으며 미해결 필수 수정0이다. 변경 파일 rustfmt·diff whitespace만 확인했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 실제 writer/progress phase 예약·전체 entry/tail·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-borrowed-budget-rows"></a>
### C2 descriptor budget 행의 실제 borrow — 2026-10-27

기준 `847def2`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. `obligation::validate`의5개 RemainingBudget by-value 반복을 원 배열 borrow로 바꾸고 `RemainingBudget::values`도 `&self`로 받는다. 검산 순서·checked arithmetic·반환 `[u64;14]`·encoder의 canonical bytes와 decoder의 원 remaining/spec/Result는 유지한다. source 식은 실제 slice iterator+행 참조를 계산한다. `allocation_budget::DESCRIPTORS`의 두 iterator allowance도 같은 borrow로 수정했으며 기존 cap을 늘리지 않았다.

descriptor source8711→8047B(-664), allocation iterator allowance1152→48B(-1104)로 줄었다. Full38076→36972B, Resident17212B/shared22256B·두 Resident 합56680B는 유지한다. live peak63184B·metadata186192B·whole251728B와 recovered peak64611B는 다른 최대 phase 때문에 그대로다. 기존 Pending 포괄9120B를 쓰면 합68348B, 앞선 실제 wait 관측5320B를 쓰면64548B다. 후자의988B를 실제 admission PASS로 보지 않는다. writer ticket·bound entry/tail은 미공급이다.

[선택 manifest](../../tests/fixtures/dev3-module-c2/borrowed-budget-rows/manifest.json)는 변경된 descriptor의 독립 literal·rechecksummed semantic 손상·budget 합/peak/growth·출력 보존·생성 field5, 같은 codec의 Aging1, 영향 source3, 실제 현재-build memory recovery1의10개만 선택했다. 원 frozen reserve/drop/recovery fixture를 재생해 변경 decoder가 실제 recovery에서 받는 byte 의미를 검산했다. wire·형식·원 authority 변경이나 native/새 PID 복구 실행은 없다.

`artifacts/module-c2-borrowed-budget-rows-20261027-r1/`의 production/test build·첫 descriptor5 PASS/경고0 뒤, Aging harness가 필수 `XADB_G6_SOURCE_SHA256` 부재로 제품 검산 전에 FAIL했다. 원 실패와 뒤의 source3/recovery1 NOT_RUN을 보존한다. r2는 **코드·manifest·source·ELF를 바꾸거나 재빌드하지 않고** 같은 r1 export/binary에 필요한 G6 source/ELF/input identity와 `p1-o00` raw-index hash를 공급했다. 실패했던 Aging1과 미실행4만 PASS이며 완료한5/build는 NOT_REPEATED_UNCHANGED_CODE다. r2가 참조하는 r1 source/binary는 삭제할 수 없는 원 증거다.

두 실행의 공통 source SHA-256은 `e6d01932d56a255b61ac97b1da4805a9181b09abdef144db4cdd5f8e09018064`, manifest는 `f475a3de4e30f58daa0aa36338316be37032c5dd6484baba847bf84f70f9caa7`, ELF는 `ec2b118a6e62cd6dbdf508e5d157818c2bc03ee803c1a274e829bc357a5239a0`다. r1 result `8373cd234c5287b3de4819f81d8efe4200df0b6ce6d8c5c28c751cd882384141`은 FAIL, r2 result `ba08de1a0f6a3e2d85de066e5ccaaadc2a98632d0892c9af0e294a6c7883e9a9`는 선택 continuation PASS다. G6 index SHA-256은 `ce3fd367007a6d798dbb6c2f18367a714d4f105611fb190fe57a1aa72d6d99a8`이며 실제 fixture가 그 index의 각 파일 길이/hash도 검사한다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B 한도를 유지했고 경과31.62/6.93초는 성능 측정이 아니다.

독립 검토자는 actual borrow와 남은 row/descriptor/Result/SHA 계수, 원 source5640파일과 r1/r2 artifact·최종 문서6개를 대조하고 D1/D2를 한정 수용했다. 미해결 필수 수정0이다. 변경 파일 rustfmt·diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. source/progress 전체 admission·Row/terminal/recovery 공급·M4/M5는 미완료다.

<a id="c2-borrowed-progress-rejected"></a>
### C2 progress Owner borrow 축소 후보 기각 — 2026-10-27

기준 `7ce9890`, 작성 `/root`, 독립 검토 `/root/module_review`다. ReadOperationAdmission의 원 core/OwnerIdentity를 `Option<&mut Owner>`로 대체하고 CatalogCallScope의 별도 Owner 필드를 제거하는 후보를 구현했다. 후보 source에서 production/test build·관련 runtime14·compiler3 PASS/경고0를 확인했다. compiler는 정상 Content 반환→close→Owner 재사용1 및 Owner 직접 재진입(E0502)·Content-live close(E0499) 거절2를 포함한다. source 계수는 후보 ticket8B·Scope held368B/entry1816B·Full36948B/Resident17188B였으며 **채택된 제품 값이 아니다**.

독립 D1/D2에서 safe `mem::replace`/`swap` 반례를 발견해 **미수용·원복**했다. `owner_mut() -> &mut Owner`로 전달한 값 자체를 다른 authentic Owner와 교체하면 cleanup이 새 core/index/generation을 따라간다. 다른 read ticket을 forget해 flag1/Idle로 남긴 Owner와 교체하는 경우 원 slot이 아닌 다른 slot을 정상 해제할 수 있으므로 exclusive borrow만으로 exact identity를 고정했다는 주장은 성립하지 않는다. 이 반례는 읽기 분석이며 실행 PASS가 아니다. 기존14/3 PASS가 반례를 덮지 않는다.

원 core+identity를 추가 보유하면 Scope 별도 참조 제거의 크기 이득과 상쇄된다. 실제 Catalog는 기존 Scope와 begin 인자의 독점 Owner borrow로 동시 호출 수명을 제한하므로 low-level ticket 자체의 추가 borrow는 bound entry/tail을 닫는 필수 선행이 아니다. 후보 제품2파일과 fixture는 원복하고, 원 exact-slot ticket·Full36972B·Resident17212B를 유지한다. 무변경 원복 코드의 완료 시험은 반복하지 않았다. compact entry/tail·정확 replay phase 계수와 실제 writer 예약을 후속으로 진행한다.

원 artifact `artifacts/module-c2-borrowed-progress-owner-20261027-r1/`에 **후보** source/inventory·fixture manifest·compiler/probe exports·ELF·exact 로그·runner/helpers를 보존한다. manifest는 이 export 내부 `tests/fixtures/dev3-module-c2/borrowed-progress-owner/manifest.json`이며 현재 제품 경로로 등록하지 않는다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B, 경과60.06초는 성능 결과가 아니다.

- 후보 source SHA-256: `6c105acdb72dbe39bd71a02d6eaf2d015fd4e5d21dd9f6ede1d35d75787f1ac1`
- 후보 manifest SHA-256: `eae3123994950f3890a11cf9a0a45b178c76f65f7d5b0748e49d33f25c6fe5f2`
- 후보 ELF SHA-256: `a4fa6d4167701bf17927429584c5e456209cb956f393ab5fb697e523516e958f`
- 실행 result SHA-256: `f8a8017efa1c78fd0d28ca31c98fc791fc548cf548edd880d28f8bfc67a6927c`

실행 결과 PASS와 설계 기각을 구별한다. 독립 검토자는 제품2파일의 byte-exact 원복, 후보5644 source/inventory와14/compiler3 원 artifact, 문서2개의 사실/식별자를 확인해 기각·원복 기록을 확정했다. 후보 제품 D1/D2는 미수용이며 compact entry/tail·writer phase·M4/M5는 미완료다. 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.

<a id="c2-readonly-replay-source"></a>
### C2 allocation replay의 읽기 검산 source 분리 — 2026-10-27

기준 `f401d51`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. allocation 원 owner에 actual readonly 검산의 membership·descriptor decode·current 보유 중 expected 생성 phase를 분리했다. membership는 원 COMMON/HASH와8개 scalar entry를 유지하며, live encoder만 만드는64개 entry 배열을 포함하지 않는다. decode는 malformed Transaction/XTS까지 포함하는 기존 XOD closure와 추출 Spec·mapped Semantic Result 및 ReservationOwner/ControlRef/Profile/참조·오류 인자를 계수한다. expected는 원 current와 expected/finished wrapper 반환의4개 Spec·remaining5/current row·구성값·item96/Result·hash를 유지한다. 세 phase의 max 밖에 기존 base forwarding 계수는 그대로 둔다.

INIT context는 `analyze → retained_reservation`만 실행하므로 reserve/cancel materialize의 State/Header 반환을 중첩하지 않는 전용 상계를 연결했다. reserve/cancel replay와 INIT replay의 generic TEMPORARIES fallback도 readonly closure로 대체하고 INIT의 실제 Status/FreeUndo codec max는 유지한다. live producer의 generic TEMPORARIES/PLANS와 실제 배열·Prepared는 그대로다. 제품 검산 순서·encoded byte·권한·WAL/설치 의미나 cap은 바뀌지 않았고 source class 계산만 좁혀졌다.

r1 readonly branches는 membership9360B·decode10883B·expected9080B, system base12763B, retained reservation16591B, reserve/cancel materialize17199B, INIT materialize20439B다. population replay27508→24327B·history population28836→25655B이며 현재 history retention/rollback27759B가 최대라 **Full은36972→35895B(-1077)**다. Resident17212B/shared22256B·live peak63184B/metadata186192B/whole251728B·recovered peak64611B는 유지한다. 원 Pending 관측5320B까지의 합63471B(2065B 여유)는 실제 writer ticket·entry/tail floor가 빠져 있으므로 전체 admission 증거가 아니다.

[동결 manifest](../../tests/fixtures/dev3-module-c2/readonly-replay-source/manifest.json)의 신규 source 관측1·영향 live/recovery census1·원 ledger class2·MT03 Full Pending Undo LOAD1·현재-build memory recovery1만 선택했다. production/test build와6 exact 사례 모두 PASS/경고0다. source 함수만 바뀌었으므로 무변경 codec 정답 시험·하위 회귀·workspace를 반복하지 않았다. 실제 Full charge의 예약/거절/반환, Pending LOAD, reserved Drop/fence와 새 live 인계 뒤 조회는 원 frozen fixture에서 검산했다. native/새 PID 복구 실행은 없다.

원 artifact `artifacts/module-c2-readonly-replay-source-20261027-r1/`에 source5641파일/inventory·manifest·runner/helpers·compiler·ELF·case 로그를 보존한다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B, 경과39.20초는 성능 결과가 아니다.

- source SHA-256: `b6debc2165cdc10a9dff5e7cce43c7aa193cefc9da80884a01fe8da6ef91275d`
- manifest SHA-256: `c4239be5e0fc68c661a5571999ce125738581f9409a12bc2366c820cb6e4e3d8`
- ELF SHA-256: `c2833ab6a5a0f6bed7316ecfaf7c2ee34c69f1baee8936b4f0daaa39fa86f3d7`
- result SHA-256: `ecb558d0c7f7caaf6ab6a56eea7928679a21038e9bcc2a7167a85b2e7fc53a78`

독립 검토에서 reserve/cancel transition 동안 유지하는 current DescriptorSpec1개를 membership 식에 명시하도록 보완했다. r2는 production/test build와 바뀐 branch 관측1만 PASS/경고0이며, membership10272B·decode10883B·expected9080B로 전체 max와 Full/Resident는 그대로다. r1에서 완료한 class/MT03/recovery/census5는 NOT_REPEATED_UNCHANGED_BEHAVIOR다. r2 source `efc20ee154d4de8befdbbd65c1e734732409f2afdbb80aaf71634039cd5c7e14`, manifest는 r1과 동일, ELF `557c192b2ffaec788863ee238523e0fbf715b00e85c042fa91977b54949e43ca`, result `f90cbe58276d6eff61adf8e51b81833c3ba79ab7103983fd926da5f9579a7de3`이며 경과31.51초다. 원 r1의 제한 관측과 r2 보완을 각각 보존한다.

독립 D1/D2는 r2 export 5,641파일·inventory/fingerprint·manifest/runner/helpers·compiler/ELF/관측 로그와 현재 source의 byte-exact 일치, r1→r2 DescriptorSpec 항 하나의 변경, 6문서의 원 source별 기록을 대조해 이 단위를 수용했다. 필수 미해결 수정은 0이다. 검토자는 시험·빌드·verifier·수정을 실행하지 않았다. 변경 파일 rustfmt·diff whitespace만 확인했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. compact entry/tail·writer phase 예약·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-compact-call-boundary"></a>
### C2 Catalog 진입·소비 정리 오류의 작은 경계 — 2026-10-27

기준 `514395e`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. actual MutableStorageOwner health leaf를 `HealthFailure`로 옮겨 원 Poisoned→Fenced 우선순위와 fence 효과를 보존했다. 기존 health는 이 값을 원 MutableStorageError로 확장하며, Catalog scope의 실제 작은 진입은 실패 시 원 WAL completion에 즉시 notify한다. 나중 wide 변환까지 통지를 늦추지 않는다. source 예약/검산/반환도 실제 SourceFailure를 반환하며 원 checked add/sub·Capacity/Unchanged·Fatal·active 잔존·poison/fence 규약을 유지한다. 큰 오류를 만든 뒤 축소하는 adapter가 아니다.

`catalog_call`/`catalog_resident_call`과 consuming `finish_checked`는 private 내부 variant를 가진 CatalogCallError를 사용한다. 원 frame/visibility/concurrency 오류는 그대로 보유하고 phase/cause/effect를 정확히 확장한다. legacy lookup과 `finish_result`/`finish_error`는 기존 wide 오류와 원 Fatal 우선/그 밖에는 cleanup 오류 우선 규칙을 유지한다. 검토 중 원 Unchanged와 cleanup Unchanged가 함께 있을 때의 helper 차이를 실행 전 수정했다. selected query는 실제 작은 factory/finish를 사용하지만 lookup의 wide 결과까지 작은 오류로 승격하지 않는다.

Scope의 원 Owner mutable borrow와 ticket의 원 core+OwnerIdentity는 그대로다. custom Drop을 제거하고 source→progress field Drop 순서를 사용한다. consuming finish는 기존 test hook을 self 보유 중 실행한 다음 즉시 `(source, progress)` 묶음으로 이동하여 source를 소비 정리하고 progress를 소비 정리한다. source 실패에도 progress를 반환하며 source 실패의 active/Drop 재시도를 보존한다. hook/fallible 동작을 destructure와 source 소비 사이에 넣지 않는다. 새 권한·manager·byte backing·wire 변경은 없다.

관측은 Scope72B/전체 Result<Scope>72B/CatalogCallError3B/SourceFailure2B/progress ticket24B/cleanup pair48B다. 실제 한 호출 식은 entry1004B·held116B·선택 consumer4980B·close474B·legacy boundary2018B·source leaf368B다. close에는 pair 전체와 progress 정리 중 남는 source 결과를, legacy 식에는 compact→wide 입력/출력과 LookupStep Result를 넣어 actual lookup max에 연결했다. **entry1004B는 전체33 bound 진입 floor가 아니다.** health/pool registry/visibility/core/source의 guard-held leaf 및 대기·실패 반환 carrier 동시 합은 후속 원 호출 수명 검산 대상이고 foreign Owner 수는33으로 제한하지 않는다.

held의 실제 오류 carrier 축소로 Full35895→35619B·Resident17212→16936B, shared22256B다. live peak63184B/metadata186192B/whole251728B·recovered peak64611B는 유지한다. 이전 Pending 관측5320B를 더한 합63195B/여유2341B는 writer ticket과 전체 entry/tail floor가 빠져 있으므로 admission 인수가 아니다. 이전 source별 수치와 결과를 재분류하지 않는다.

[동결 manifest](../../tests/fixtures/dev3-module-c2/compact-call-boundary/manifest.json)는 신규 health/priority/carrier 관측3, 변경 source 예약/반환8, actual Scope6, source entry3, final return2, Full Pending Undo LOAD1, live/recovery census1, 현재-build memory recovery1의25 exact 사례를 선택했다. Scope custom Drop 제거와 consuming finish 변경에 대해 기존 authored compiler4도 같은 borrow 계약을 재검증했다. production/test build·runtime25·compiler4 모두 PASS/경고0다. 완료한 무변경 codec·하위 회귀·전체 workspace는 실행하지 않았다. normal Drop/unwind·fenced cleanup·원 I/O Fatal·실제 Drain 대기·현재 recovery 뒤 조회는 변경 경계에 연결된 원 oracle을 사용했다. native/새 PID 복구는 실행하지 않았다.

원 artifact `artifacts/module-c2-compact-call-boundary-20261027-r1/`에5642파일 source/inventory·manifest·runner/helpers·compiler/probe export·ELF·각 case 로그를 보존한다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B, 실제75.98초는 성능 결과가 아니다.

- source SHA-256: `94a44d1d4e5a786596bb4be7c91706725fb36ff2d152bd057540cfb82d765738`
- manifest SHA-256: `77e6d843e5a31e639ded610eac8050342fba53d8a1b5c0ed622e8be537dd282a`
- ELF SHA-256: `7806bc2b02cb1c1de8a0cb58bc426848ba52b7a5f859d524915917bf80d39120`
- result SHA-256: `ea0a4d1282b915893cac7922af244c769250e39e86cfe9fd730ab2fc070eb1dd`

독립 D1/D2는 코드·원 source5642파일/inventory·build2/runtime25/compiler4의 원 export/진단·ELF·6문서를 대조해 이 한정 단위를 수용했다. 필수 미해결 수정은0이며 검토자는 시험·빌드·verifier·수정을 실행하지 않았다. 변경 파일 rustfmt·diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 bound entry/tail floor·writer/progress phase 예약·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-borrowed-replay-workspace"></a>
### C2 실제 replay 작업 공간 대여 — 2026-10-27

기준 `b304bea`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. semantic Catalog의 실제 replay observer가 원 WORK_END 안의 provenance 뒤 zero tail에서 descriptor2×1536B+core256B를 빌린다. 기존 전체 loan1572864B·WORK_END1507328B·claims22528B와 wire/hash 범위는 그대로다. 새 backing/authority/manager는 없다. producer는 원 local 배열을 보유하며 같은 `run_with_buffers`의 원 compute/ordering/frame encode·verify 본문을 사용한다.

private ReplayScratch는 대여 전 전체3328B가0임을 확인하고 오염 입력은 지우지 않고 거절한다. 생성 뒤 정상/오류/unwind Drop는 실제 빌린 descriptor/core만 byte-wise로 지우며 주변 byte를 변경하지 않는다. history/recovery의 원 tail 검산은 유지한다. actual scope2 replay만 borrowed observer를 수신하며 기존 owned observe는 시험 경계에 남는다.

source 식은 owned wrapper의 인자/배열/반환, borrowed constructor/guard/Drop iterator·slice·Result와 forwarding borrow를 포함한다. owned24335B/borrowed20899B는 각각 현재 helper 식이며, 과거 식에서3328B를 단순 차감하지 않는다. history retention24715B·population25743B·initialize26120B/전체 peak28784B로 최대 경로가 바뀌어 Full35619→33980B다. Resident16936B/shared22256B·live peak63184B/metadata186192B/whole251728B·recovered peak64611B는 유지한다. 원 Pending 관측5320B를 더한61556B/여유3980B에는 전체 bound entry/tail·독립 guard leaf·writer ticket이 빠져 있으므로 전체 admission 인수가 아니다.

[동결 manifest](../../tests/fixtures/dev3-module-c2/borrowed-replay-workspace/manifest.json)는 신규 borrowed 독립 C03/N16 검산1·실제 guard 정상/unwind/주변 보존1·변경 owned C03 1·source 관측4·실제 Full Pending Undo LOAD1·현재-build memory recovery1의9 exact 사례다. C03은 원 독립 전체 after/frame golden 일치를 검산하고, N16은 독립 mutant frame 거절과 scratch zero 반환을 검산한다. N16 오류 뒤 after 전체 비교는 수행하지 않는다. 전체72사례를 실행하지 않았다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B로 동결했다. native/새 PID 복구·하위 회귀·전체 workspace는 실행하지 않았다.

`artifacts/module-c2-borrowed-replay-workspace-20261027-r1/`은 편집 중 transition.rs를 시험 본문으로 잘못 덮은 원 source다. production compile FAIL(E0432/E0425/E0603), test build/runtime9 NOT_RUN을 그대로 보존한다. 원 제품 본문을 기준 commit에서 복원해 의도한 helper/guard만 다시 적용한 r2는 production/test build·runtime9 PASS다. production unused observe import 경고1을 보존하고, r3는 re-export의 cfg(test) 분리만 검증해 build2 PASS/경고0다. r2에서 완료한 runtime9는 r3에서 NOT_REPEATED_UNCHANGED_CODE이며 r2 증거를 새 source의 실행으로 바꾸지 않는다. r1→r2 변경은 transition.rs, r2→r3 변경은 semantic_catalog.rs 한 파일이다.

각 artifact에5643파일 source/inventory·manifest·runner/helpers·원 compiler·ELF(생성 시)·case 로그를 보존한다. 실제 경과 r1 5.72초/r2 39.23초/r3 31.73초는 성능 결과가 아니다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 FAIL | `662f6ee0c73d28cb83ee2a41d98d6d8e67931ac197ce0adbf74cd7aca2cc7e7b` | 미생성 | `bc92439def393e2edee5f2ece38e5d0951d9ce56862a903ee57f6b2c9792a85d` |
| r2 PASS/경고1 | `3e8e08f8a8e426ed8bb8957d20aa0eb5bde0266f5c3d8d46d6082489db83dd0b` | `ee6d27c46b7e42c4bdfca29375d96059332dc03ce55b38e2f47d1cc1bc2a26d1` | `9fbcab7f2d4987fe6288af4e0e04336ee46179a4ebdbf20945ff8c09fe9e3f8b` |
| r3 import-only PASS | `dea3545c6ed6c0657c4b8b219edb8c976aeb8afc65aca381fb64fc15ec8e7906` | `9fbf4135a2176e2b0d9ba272116eda840f72f945ff59a69fc38e223dd278131c` | `4ed50b0321d83361dfcc27f712bc63e2855bc3926949727a3242e5e3e3e0da1e` |

manifest SHA-256은 세 실행 모두 `bd5119987df3f80c7c20169c9245337fa3ff996724b94ac481aad740f61f332d`다. 독립 D1/D2는 제품 diff·r1/r2/r3 원 source/inventory·build/runtime 로그·ELF·6문서를 대조하고, N16 검산 범위와 runtime 미반복의 문구 정정을 수신해 한정 수용했다. 미해결 필수 수정은0이다. 검토자는 수정·빌드·시험·verifier를 실행하지 않는다. 변경 파일 rustfmt·diff whitespace만 확인하고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 bound entry/tail·writer/progress phase·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-split-source-charges"></a>
### C2 exclusive split의 고정 source 계산과 port 계수 — 2026-10-27

기준 `266c9ca`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. 전체 entry floor 조사에서 source 예약 전에 매번 여러 하위 계수 함수를 호출하던 일반 source_census 계산값도 실제 temporaries임을 확인했다. compiler constant-folding을 가정해 제외하지 않고, 원 `DiskSession::split(&mut self)`에서 fixed shared/Full/Resident를 한 번 계산한 후 private immutable CatalogSourceCharges를 ReadPort로 이동한다. 살아 있는 port/Scope와 exclusive split은 borrow로 겹치지 않는다. Catalog entry는 자신의 원 ReadPort에서 scalar만 읽으며 caller byte count·새 manager·authority·backing은 추가하지 않는다.

계산 graph는 size_of/산술과 작은 Census/tuple/iterator 값만 생성한다. [동결 manifest](../../tests/fixtures/dev3-module-c2/split-source-charges/manifest.json)에17단계 실제 deep path, 최대 실제packet History/Load 각12usize=96B, bootstrap [usize;7]/IntoIter/fold와 소유 경로를 기록했다. 각 helper의 인자·지역값·미완료 식·iterator·callee return와 추출값까지128word, 동시24helper의 보수 상한24576B를 사용한다. sizeof의 제품 배열/guard는 계산 때 생성되지 않는다. 전체 split24680B는 계산과 tuple+각 port 반환 phase의 max이며 실제 port는 계산 뒤 생성한다. 이는 source-value 상계이며 stack/RSS나 성능 측정이 아니다.

원 shared 식에 빠졌던 실제 split3port를144B로 한 번 추가했다. conversion에는 아직 port가 없으며 live active에는 ports를 더하고 exclusive split은 별도 max다. recovered step의 waiting_flush/FlushRequired에서도 split이 호출되므로 Step/Result/target/UnwindFence·인자·반환을 추가한47600B 별도 가지를 연결했다. port tuple과 추출한 값의 반환 phase도 보수적으로 보존한다.

source ticket charge는 원 checked next<=available<=65536이 성공한 뒤만u32로 변환한다. 반환은u64로 확장하며 header24..32의u64·32..64zero·실패 우선·poison/fence·active 잔존·source→progress 순서와 원 core+OwnerIdentity는 유지한다. 새 경계 시험은65536 전량 보유/반환,65537·u32::MAX+1·usize::MAX 무효과 거절을 검산했다.

실측 ticket24→16B·Scope/ResultScope72→64B·cleanup pair48→40B다. one-call entry948B/held108B/consumer4972B/close434B/legacy1962B/source leaf336B이며 전체 bound floor와 구별한다. Full33972B·Resident16928B·shared22400B, shared+Resident2=56256B다. 원 Pending 관측5320B까지61692B/여유3844B에는 전체 entry/tail·독립 guard leaf·writer ticket이 빠져 있다. live peak63328B/metadata186336B/whole251872B·recovered peak64704B로 올라갔으며 이전63184/64611B를 현재 값으로 재사용하지 않는다. backing/형식/제품 한도는 유지한다.

선택16은 charge 표현/고정 class9, actual port 전달1, split 계수1, live/recovered source1, Scope 크기1, actual Resident 결과 소비1, Full Pending Undo LOAD1, 현재-build memory recovery1이다. runtime은 r3에서만 실행했다. r1은 MaintenancePort sizeof의 lifetime 인자 누락 E0107, r2는 복구 계수의 private legacy module alias E0603으로 production compile FAIL이며 test build/runtime16 NOT_RUN이다. 두 실패 source/log를 그대로 보존하고 타입 인자와 canonical `storage::disk::RetentionError` 경로를 수정했다. r3 build2/runtime16 PASS·경고0다. borrow API/Drop authority가 바뀌지 않아 이전 compiler probes는 재실행하지 않았다. 무변경 codec/하위 회귀·전체 workspace·native/새 PID 복구는 실행하지 않았다.

각 `artifacts/module-c2-split-source-charges-20261027-r{1,2,3}/`에5644파일 source/inventory·manifest·runner/helper·compiler·ELF(생성 시)·case 로그를 보존한다. 동결 총600초/compiler90초/case30초/출력8MiB/stack33554432B, 실제 r1 5.77초/r2 5.67초/r3 40.42초는 성능 결과가 아니다. r1/r2/r3의 선택16은 같고 manifest의 계산graph 감사 근거가 보완됐다.

| 실행 | source SHA-256 | manifest SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 FAIL | `d892ca2683bd4246a6c9c002bfb870a7c5485f6fe48863ed315806fcdf63a277` | `2e8b5a3d0f07c21f04fb167e0dbbb603bba01059caf808b4f4ae456716d19d4e` | `f832bed02f3c703afb45e582b7440b3c2a02f9dfffca92048104926d38a3462d` |
| r2 FAIL | `bd537f0312325978e5ef2273e4e86adaae7b170332dc11aeb2972ea581fcdee0` | `8e175414b1f77d88e40a989875578ea56c88d746049d48af618831130bb531fb` | `cf404e7e818a0d1c816491856df6f7d3670631c368ac9f7653be31a599e82f03` |
| r3 PASS | `0af267449e387bc19ef806df764ef921178855552b24206bbea11a4c961d6dc0` | `0cd05e4718397b2462cd5d592e2ff59666e254d11daf7e184bd8fb6917d6eba1` | `1e8fdb2fc971b1b5e84442b4f02c582ef78406bb3c46f56c5bf1c579e46048ea` |

r3 ELF SHA-256은 `427a179ba550920ec7cf11f9e8c6feabfe9202fcc312a8a72d619b178f54d4a1`이다. 독립 D1/D2는 현재 제품/계산graph·원r1/r2/r3 source/inventory·build/runtime 로그·ELF·6문서를 대조해 한정 수용했다. 미해결 필수 수정은0이며 실제 비재귀 호출graph라는 문구 정정을 반영했다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았다. 변경 파일 rustfmt/diff whitespace만 확인했으며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 bound entry/tail·writer/progress·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-uncharged-call-phases"></a>
### C2 예약 밖 호출 phase의 관측과 작은 실패 정리 — 2026-10-27

기준 `2273185`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. actual source 예약 실패는 private `finish_failed_source(SourceFailure, ReadOperationAdmission)`가 원 ticket을 소비 정리한 뒤 작은 CatalogCallError를 반환한다. 그 뒤 caller가 전체 `Err<Scope>`를 만든다. 원 SourceFailure와 progress ticket을 다른 권한으로 바꾸지 않으며, 원 Fatal 우선·그 밖에는 cleanup 오류 우선 규칙과 source→progress 순서, fence/poison 정책을 유지한다. 신규 helper의 정상 Capacity·fenced cleanup·원 Fatal 조합과 실제 source 실패 두 경로를 검산했다.

나머지 변경은 **cfg(test) 계수식 관측**이며 제품 charge·source admission에 연결하지 않았다. 원 같은 core의 FOREGROUND32+Maintenance1+DedicatedFlusher 거절 입구1인34개를 사용한다. progress 허용33과 구별하며 다른 core의 foreign Owner 수를34로 제한하지 않는다. 각 owner에서 guard 없는 대기/guard 보유 중 추가값/guard 없는 반환을 구분하고, 독립 core·pool registry·visibility registry·isolation mutex의 guarded 값을 합한다. visibility confirms는 publication mutex나 새 snapshot을 만들지 않는다.

pool에는 allocator/current/prior Registration, map의 TargetRequest, rank의 PageRef/24B/tuple, System INIT→init_generations→directory 검산 중첩, TxPair actor/indices, LOAD의 실제 flush/INIT Option+claim 복사, previous FrameStamp, DPT WalRequirement/WalFrameLocation을 넣었다. visibility에는 원 retirement target과 count의 값 인자·Option 비교, ReferenceTarget의 enum/map/Result, TxnId/DatabaseId의 engine 오류와 Undo/PageRef/Generation의 foundation 오류를 구분했다. 큰 타입·iterator 외의 인자/작은 값은 pool13단계·visibility11단계 각각64B의 보수 packet과 명시된 보조값으로 계수한다. [동결 manifest](../../tests/fixtures/dev3-module-c2/uncharged-call-phases/manifest.json)에 호출 범위와 근거를 보존한다.

| owner | guard 없는 대기 | guarded 추가값 | guard 없는 반환 |
| --- | ---: | ---: | ---: |
| core | 52B | 658B | 136B |
| pool registry | 80B | 3588B | 76B |
| visibility registry | 84B | 2351B | 66B |
| isolation | 50B | 504B | 136B |

독립 guarded 합은7101B다. 원 factory/begin 인자와 작은 실패 helper, **전체 Scope Result**를 유지한 관측은 entry217B·source 실패195B·반환197B·compact tail234B이며 `34×234+7101=15057B`다. 현재 selected query는 같은 for lexical scope의 result/step/target과 오류 시 `finish_error`의 wide 값을 종료 호출 동안 유지하므로, 기존 consumer4972+legacy1962+close434=7368B를 별도 넓은 tail baseline으로 남겼다. 그 단순 조합은257613B다. 두 결과 모두 source 식의 보수 상계이며 실제 최소 비용·machine stack/RSS·성능·불가능성 하한이 아니다. 작은 경계만의 값을 selected caller 전체 보장으로 올리지 않는다.

Full33972B·Resident16928B·shared22400B와 원 charge는 그대로다. shared+Full 뒤9164B의 여유에는 현재15057B 관측을 그대로 예약할 수 없으며 wide caller의 실제 소비 경계도 아직 닫히지 않았다. 따라서 전체 reader floor·writer Pending/progress phase admission은 미공급이다. 기존 one-call 식과 source별 PASS를 이번 관측으로 소급 재분류하지 않는다. 후속은 실제 소비/오류 수명과 guard closure를 좁히거나 원 자원을 보존하는 구조로 해소하며 한도 증액·case 삭제는 하지 않는다.

실행 전 diff→선택을 고정해 신규 helper1·신규 관측1·변경된 실제 source 실패(fence/Capacity)2만 실행했다. r1은 production build PASS 뒤 시험용 ConcurrencyError 경로 미한정 E0425로 test build FAIL/runtime4 NOT_RUN이다. 원 source와 로그를 보존하고 cfg(test)의 해당 한 타입만 `concurrency::ConcurrencyError`로 정정했다. r2 test build/runtime4 PASS·경고0이며 생산 코드가 같아 production check는 NOT_REPEATED_UNCHANGED_CODE다. 무변경 lookup/codec/복구·compiler borrow probe·하위 회귀·workspace 시험은 반복하지 않았다. 이 helper는 영속 의미/byte/복구 경로를 바꾸지 않는다.

원 `artifacts/module-c2-uncharged-call-phases-20261027-r{1,2}/`에 source5645파일/inventory·manifest·runner/helpers·compiler·ELF(생성 시)·case 로그를 보존한다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B, 실제 r1 20.29초/r2 29.06초는 성능 결과가 아니다.

| 실행 | source SHA-256 | result SHA-256 |
| --- | --- | --- |
| r1 test build FAIL | `49e4b60f12adb8a1cdf77dcea9537a8c51314960157de3b0a349601eb65f01af` | `d6d7311e64519c1d7d51f261508f12ad089d1b95b9d58d493b11683e73aa82ab` |
| r2 선택4 PASS | `27d454362ffcf02d9be96869300e2d00e9d1dfe526710d92f1669cc68ee718b8` | `5e10c28db8e301e39997f6756bce1cd94825f138ca999fa1e02f5d28626521b4` |

두 실행의 manifest SHA-256은 `c2aeac954ba47fcd69907d911d692b3cade4aa443dc6284cc69f18e0e3790b38`, r2 ELF는 `41a9e95a09a8cc8f64a9de1fb2898cd5220be2134779251efd8aa8d757de54f5`다. 독립 D1/D2는 현재8파일·각5645파일 원 source/inventory·compiler/ELF/case 로그·6문서의 수치와 해시를 대조해 이 한정 단위를 수용했다. 필수 미해결 수정은0이며 검토자는 수정·빌드·시험·verifier를 실행하지 않았다. 변경 파일 rustfmt/diff whitespace만 확인하며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 entry/tail·writer/progress·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-query-tail-boundaries"></a>
### C2 query 정상/오류 종료 경계 — 2026-10-27

기준 `c8d47ed`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. actual `CatalogCallScope::finish_error`는 원 wide RetentionError를 유지한 채 작은 `finish_checked` 결과를 받고, 이후에만 cleanup 오류를 wide 오류로 변환한다. 기존 중간 `Result<()>`를 종료 호출 동안 만들지 않는다. 원 Fatal이 우선하며 원 오류가 nonfatal이면 cleanup 오류가 우선하는 의미, 원 source→progress 순서와 fence 아래 charge 잔존은 유지한다.

선택 actual `query`는 lookup Result/step/RecordView와 header/Table/Column 소비를 lexical block 안에서 끝내고 정상 출구의 `Option<Plan4Target>`만 반환한다. block 뒤 consuming close를 실행한 다음 FlushRequired를 처리한다. 오류 arm은 같은 block 안에서 `finish_error`를 호출하므로 그때 caller의 전체 lookup Result를 없다고 세지 않는다. authority·snapshot·pin·영속 byte는 바꾸지 않았다.

신규 cfg(test) 관측은 정상 tail710B와 오류 tail1394B를 구별한다. 오류에는 loop 값, 전체 lookup Result의 반환/추출2개, 추출 오류와 finish_error518B를 포함한다. finish_error는 원 RetentionError72B를 유지하며 작은 close 대기와 오류 변환259B를 별도 phase로 센다. 변환에는 원 오류 외에 반환 wide 오류·phase/cause/effect·SourceFailure를 남긴다. 기존 consumer4972+legacy1962+close434=7368B는 `c8d47ed`의 넓은 baseline으로 보존한다. 이 수치는 한 호출의 보수 source 식이며 actual stack/RSS·성능·동시34입구의 전체 floor가 아니다. 제품 charge/admission에는 연결하지 않았고 Full33972B·Resident16928B·shared22400B는 유지한다.

실행 전 [선택 manifest](../../tests/fixtures/dev3-module-c2/query-tail-boundaries/manifest.json)에 diff→5case와 총600초/compiler90초/case30초/출력8MiB/stack33554432B를 고정했다. 신규 original Scope의 caller 입력 오류(Fatal/nonfatal)×healthy/fenced cleanup4조합1건, 실제 query의 Missing/Commit후 Found1건, 새 관측1건과 finish_error가 직접 바뀐 actual Busy/Undo PageIo Fatal2건만 실행했다. synthetic 원 Fatal 입력은 priority 검증이며 actual source fatal이 fence 없이 발생했다는 증거가 아니다. production/test build 및 runtime5 PASS·경고0다. source/progress/refs/pins/I/O 잔존과 오류 phase/cause/effect를 검산했다. 실제 query 검증은 변경한 helper를 호출하며 기존 결과 소비 test로 대체하지 않았다.

`artifacts/module-c2-query-tail-boundaries-20261027-r1/`에 source5646파일/inventory·manifest·runner/helpers·compiler·ELF·5case 로그를 보존한다. 실제38.26초는 성능 결과가 아니다. source SHA-256 `e5bd6ae338ad27ffff7e6268ad243627e8672f713809e98d5ff717a9982bf7d6`, manifest `49543abcaf49de48a2b0fe5db887063cfa84916d6043d309758ebf1cbf3a8414`, ELF `10747a193a83333e655b41f48714d4329da9ba2d35e1778211b247cfe0d1d537`, result `3a05d737bf2006559fd2ebaa698e0dd7cb89a657609d9740194584d62d7dd7a2`다.

독립 D1/D2는 실제3 Rust diff·원5646파일/inventory·manifest/runner/helpers·compiler/ELF/5case와6문서를 대조해 이 한정 단위를 수용했다. 필수 미해결 수정은0이며 검토자는 수정·빌드·시험·verifier를 실행하지 않았다. 무변경 과거 관측/codec/복구·compiler borrow probe·하위 회귀·workspace 시험은 반복하지 않았다. 변경 파일 rustfmt/diff whitespace만 확인하며 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 entry/tail·writer/progress·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-compact-dpt"></a>
### C2 원 DPT의 중복 identity 저장 제거 — 2026-10-27

`XADB-MODULE-DPT-001` · D2 · 기준 `d6d695d` · 작성/통합 `/root` · 독립 검토 `/root/module_review`. 원 source 예약의 retained 비용을 줄이기 위해 live PoolRegistry의 private 행만 `DptEntry {generation,rec_lsn,modification_counter}`로 바꿨다. full `DirtyEntry` 관측·expected·DptStamp·실제 WalRequirement와 historical DirtyRegistry/PriorPool은 보존한다. 비교 대안은 duplicate identity를 계속 저장하거나 cap/frame 수를 바꾸는 방법이었다. 후자는 채택하지 않고 같은 원 directory가 이미 소유한 identity를 사용한다. 새로운 runtime 영속 파일·버퍼·lock·semaphore·WAL/페이지 형식은 없다. 전체 aggregate 공급 완료 결정은 아니다.

생산자는 ordinary install1곳, System INIT2곳, TxPair Forward loop이며 기존 permit/원 full expected를 검산한 뒤 compact 행을 저장한다. `dpt_matches`는 expected를 borrow하고 같은 registry 아래 published registration의 full identity·generation과 stored generation/recLSN/counter를 모두 비교한다. present DPT와 missing publication은 정상 일치가 아니다. `dpt_valid`는 published registration·같은 generation·nonzero recLSN/counter·원 fence/location의 WalRequirement를 계속 요구한다. 같은 slot의 identity를 바꾸는 정상 LOAD/rebind는 기존 clean/DPT 없음/requirement 없음·checked 다음 generation을 선행한다. 새 dirty 상태에서 same-generation identity 교체를 도입하면 이 결정은 재검토한다.

INIT DPT→directory publication은 원 FencedInitRegistry 아래 유지된다. 중단되면 원 claim이 정확한 두 target/generation을 보유한다. cfg(test) 진단만 같은 generation의 published 또는 original claim으로 full identity를 복원하며 둘 다 있으면 일치해야 한다. 둘 다 없으면 panic으로 명시하며 clean으로 숨기거나 다른 generation을 사용하지 않는다. fenced unwind에서 initializer pins가 이미 반환됐어도 원 claim을 읽을 수 있다. 이 진단은 publication/설치 권한이 아니다. 원 요구·DPT clear와 retirement의 순서는 유지한다.

실제 저장 배열5120→2048B로3072B 감소했다. 새 helper closure660B는 `target_set_call_bytes`와 TxPair `source_census`에 직접 연결했다. generic N2 실제 charge2396B, 원 broad physical2352+helper660=3012≤3072이며 원 cap을 늘리지 않는다. CatalogFull33972B는 유지하고 Resident는16928→17588B, shared는22400→19328B다. Full 합53300B·두 Resident 합54504B다. 현재 live peak62236B/metadata185244B/whole250780B, recovered peak63612B를 선택 source census에서 확인했다. 이 식은 source 값의 보수 상계이며 실제 stack/RSS·성능·전체 entry/tail/writer aggregate admission의 증명이 아니다.

System legacy INIT는 기존 physical3072 예약을 사용하며 Plan4 population/flush는 변경한 target-set 식을 직접 사용한다. legacy allocator/status flusher는 별도 capture/finalize8192를 유지한다. 독립 읽기 검산은 기존 타입상한으로 physical3072 + 2×Result<Option<CapturedPage>>512 + 2×Option<CapturedPage>384 + 2×FinalizeContent192 + 2×Result<Option<FinalizedPage>>192 + 2×FinalizedPage128 + 2×FrameStamp112 + 2×DirtyObservation192 + 2×WalRequirement128 + 2×CaptureAttempt64 + 4×TargetRequest80 + 4×MutexGuard32 + 4×Result<()>32 + outer512 =7968≤8192를 확인했다. outer512는 registration128·borrowed reference128·index64·u64 locals32·boolean/discriminant를 포함한다. capture/finalize를 함께 더한 정적 보수식이며 새 실행·실측으로 주장하지 않는다. helper660은 physical3072 안에 들어가므로 이 전체8192에 다시 가산하지 않는다.

[선택 manifest](../../tests/fixtures/dev3-module-c2/compact-dpt/manifest.json)는 총600초/compiler90초/case30초/출력8MiB/stack33554432B로 고정했다. r1 production PASS 뒤 새 test PageLsn import 누락 E0425로 test-build FAIL·runtime13 NOT_RUN. r2는 cfg(test) import만 고쳐 production을 반복하지 않았고 test-build·첫 표현/진단/layout3 PASS 뒤 새 Pair fixture가 owner2를 기대하여 실제6과 불일치 FAIL, 나머지9 NOT_RUN이다. r3는 실제 source 함수 비용 연결·layout 수정과 owner6 정정을 반영해 production/test-build·선택12 PASS·경고0다. 변경 없는 첫 표현/진단2 PASS는 r2 원 source에 유지하며 반복하지 않았다. 서로 다른14selector의 결과를 source별로 보존하며 r2 layout을 r3 source의 실행으로 바꾸지 않는다.

r3 실제 검증은 새 Pair DPT/first-registration/both-before-gate/gate-published4중단에서 page11/12·owner6·incarnation2·generation·recLSN8/counter1/requirement8·fence/claim과 gate 이전3중단의 비공개를 검산했다. 기존 System CF08-05/06만 선택해 두 publication 경계를 검사했다. 실제 Forward→CLR/Abort 정리, reserved Drop 뒤 fresh recovery, FN04만의 capture→redirty→최초 recLSN 유지→clean finalize, DPT/requirement 한쪽 손상 fence, dirty victim flush 뒤 LOAD 재시도, live/recovered 현재 타입 계산·실제 class 예약·generic N2/3/8/64 한도에 한정했다. 무변경 workspace/하위 wrapper/완료 사례를 일괄 실행하지 않았다.

원 artifacts는 `artifacts/module-c2-compact-dpt-20261027-r1/`, `r2/`, `r3/`의 source5649파일·inventory·manifest·runner/helpers·compiler와 각 시도에서 실제 생성된 ELF·case 로그(생성 시)다. r1에는 test-build 실패로 ELF·runtime case 로그가 없다. r3 source `d58d1b751d9abace6bfc04e3926c6068300362565810eedf5f99d6dc7f88c7a9`, manifest `732bd409443ebf71bf5f23125698dd6f8881526a4933a0dfbd3ff27fe960c850`, ELF `30ca8627036b8bc38472fa8473a982417f1cb2902b19be7eb1c8a3a8b2d9be2f`, result `c6a732242aa6884078bbdd08d89eb0d316ba5c98649d290fd1d5f249584d99be`. r1 result `42abfd8036587a6c30555fdcffdd3924255bd82d94f411d1b3575c7b5b49797d`, r2 result `153ec16bb583d20d4563e3fbf2abcfd8a35fe8d92203d3bdd40f649ed5aee6bc`도 보존한다. r3 실제48.91초는 성능 측정이 아니다.

독립 `/root/module_review`는 최종 제품 diff·r1–r3 source/inventory/compiler/ELF/선택 로그와 문서7개를 읽기 전용 대조하여 D1/D2를 한정 수용했다. 필수 제품 수정0이며 영속 파일 범위·r1 미생성 ELF/case 로그·gate 이전3중단 비공개 설명을 정정했고 `/root`가 반영을 확정했다. private compact DPT·실제 helper 비용·source별 선택 검증의 수용이며 전체 aggregate/M4/M5 완료가 아니다. source별 실패·미실행과 과거 검증 범위를 유지한다. 변경 파일 rustfmt/diff whitespace만 확인하고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 short floor·writer phase·Row/terminal/recovery·M4/M5는 미완료다.

<a id="c2-borrowed-load-claims"></a>
### C2 LOAD 검산의 원 claim borrow — 2026-10-27

기준 `1c2f696` · 작성/통합 `/root` · 독립 검토 `/root/module_review`. `LoadClaimRecord::valid`의 원 flush claim과 System/TxPair INIT claim 검사에 `as_ref()`를 사용한다. 기존 `Claim::index`와 `InitClaimSlot::blocks_load`는 이미 `&self`를 받으며, 같은 registry guard·단락평가·검사 순서·원 full target/index/generation을 유지한다. 원 claim/token/pin/WAL 권한을 이동·재구성하지 않는다. 별도 `load_eligible`의 복사는 이 변경 범위 밖이며 모든 LOAD 복사 제거로 주장하지 않는다.

cfg(test) source 관측은 두 copied Option/claim을 각각2×Option<&T>+2×&T로 대체한다. 새 pool guard3307B·독립 guard 합6820B, 원 core34입구의 compact floor14776B다. 과거 pool3588B·합7101B·floor15057B는 원 source의 증거로 유지한다. per-call entry217/failed-source195/return197/compact-tail234B와 기존 broad query tail7368B는 유지한다. source class·shared19328/Full33972/Resident17588은 바꾸지 않았고 관측식을 admission에 연결하지 않았다. 실제 source 최소치·stack/RSS·성능 또는 전체 floor 충족을 뜻하지 않는다.

실행 전 [선택 manifest](../../tests/fixtures/dev3-module-c2/borrowed-load-claims/manifest.json)에 새 관측1·실제 MT03 Pending Undo LOAD·captured Undo PageIo fatal 정리·dirty victim flush 뒤 같은 snapshot LOAD의4selector를 동결했다. 모두 변경 `valid` 또는 변경 관측에 직접 연결된다. MT05 lower flush는 이 변경을 직접 실행하는 근거가 없어 실행 전에 선택에서 제외했다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B, production/test build·runtime4 PASS·경고0다. 성공·오류 실제 LOAD 증거와 Some conflict 조합의 동일 predicate 읽기 검산을 구별하며 Some 조합 전수 실행으로 주장하지 않는다.

`artifacts/module-c2-borrowed-load-claims-20261027-r1/`의 source/inventory/compiler/ELF/4case 로그를 보존한다. source `30ab6d70fa6e99b72b5739708a5989f26d468fc30e4225d4f76421bf95d9d956`, manifest `a34e8c7abcd1d6731328f804e12b7e77c06ea4e5c260008444bfddc9e0c444f9`, ELF `608abbe84e86b62aa9c7d63e1105f4dbde9796341d4977f326bed62dbbdec632`, result `de8011a5c21f9b85b5797f4c31bde3da0d9cbe00a7cf36b0ac3abfeca7ae64cc`다. 실제37.63초는 성능 결과가 아니다.

독립 `/root/module_review`는 실제2파일 diff·5650파일 source/inventory·manifest/runner/helpers·ELF/compiler·exact4와 각 context identity·문서6개를 읽기 전용 대조하여 D1/D2 한정 수용, 미해결 필수 수정0을 확인했고 `/root`가 반영을 확정했다. whole admission이나 Some conflict 전수 인수로 확대하지 않는다. 이전 DPT 표현/진단·class/layout와 무변경 compiler probe·복구·wrapper/workspace는 반복하지 않았다. 원 PASS/FAIL은 source별로 보존하며 변경 파일 rustfmt/diff whitespace 외 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 source floor·writer/progress phase·Row/terminal/recovery와 M4/M5는 미완료다.

<a id="c2-admitted-validation"></a>
### C2 고정 source 예약 뒤 원 pool/snapshot 검산 — 2026-10-27

기준 `7c6318f` · `XADB-MODULE-ADMISSION-002` D2 · 작성/통합 `/root` · 독립 검토 `/root/module_review`. [내부 입구 우선순위](../contracts/dev3-module-boundaries.md#operation-source-admission)를 한정 재개방한다. 원 source·progress의 동일 소유와 source→progress 종료는 유지하며, 원 pool/snapshot metadata 검산을 fixed 예약 안으로 이동했다. 기존 큰 검산을 거절 분기에서 다시 수행하는 대안은 uncharged floor를 줄이지 못하므로 채택하지 않았다.

실제 begin은 guard 없는 storage/pool health와 원 origin, 기존 Owner::check_binding의 Frame 오류 projection, snapshot 원 registry/health/poison 관측, 기존 check/try read progress, source 예약, 원 전체 validate 순서다. 성공 scope가 작은 validation 오류를 받으면 `finish_entry_error`로 원 오류를 보유하고 source/progress를 소비한 뒤 Err<Scope>를 구성한다. Fatal은 기존 fence 아래 charge를 보존하고 원 progress 정리는 시도한다. full/resident source 식은 `validation_source_bytes`를 직접 max에 넣는다. 원 pool/visibility의 세 phase 계산을 제품 source 식에서 사용하며 guard끼리 배타라는 이유로 빼지 않고 양쪽 값을 모두 가산했다.

[실행 전 manifest](../../tests/fixtures/dev3-module-c2/admitted-validation/manifest.json)는 changed-code12selector와 총600초/compiler90초/case30초/출력8MiB/stack33554432B를 동결했다. 신규 Full/Resident×pool bad sum/SnapshotGuard observed q 불일치4fixture는 foreign/flusher admission projection·Capacity의 무효과 거절 후 원 charge 반환, 동일 손상의 재시도 Fatal·잔여 charge를 비교한다. 신규 observed pool poison+Capacity1과 source phase1, 실제 Drain/fence/unwind3·변경 compact health1·foreign/Missing1·변경 class2·필수 Pending Undo LOAD1·Resident visibility1을 선택했다. 변경 없는 failed_source helper·이전 DPT/LOAD 표현/전체 wrapper는 반복하지 않는다.

r1 production PASS 뒤 새 시험 모듈 경로 지정 누락 E0583으로 test-build FAIL·runtime12 NOT_RUN을 보존한다. cfg(test) path만 고친 r2는 production을 반복하지 않고 test-build·exact12 PASS·경고0을 확인했다. 새로운 validation6958B는 원 Full33972B/Resident17588B 안에 들어가며 shared19328B는 유지한다. SnapshotGuard의 observed q를 바꾼 fixture이며 실제 visibility registry row 손상으로 주장하지 않는다. Busy/Drain과 손상 metadata의 결합은 별도 실행하지 않았다. 해당 우선순위는 코드 순서의 읽기 검토로 확인했으며 healthy Drain 실행을 그 조합의 PASS로 확대하지 않는다.

원 `artifacts/module-c2-admitted-validation-20261027-r{1,2}/`의 각 source5652파일/inventory·manifest/runner/helpers·compiler와 r2 ELF·12case 로그를 보존한다. r1에는 test ELF·runtime 로그가 없다. 실제 r1 20.45초/r2 38.85초는 성능 결과가 아니다. 공통 manifest SHA-256은 `caf6666b940e71ed4892b5ce7cbe8d3a7bbcb74d7ef9987eaae72b22633f2bc3`, r2 ELF는 `e8294794933577202c569fe3b481a9aac1f5faf83e29ba12d81e778ac356c3ef`다.

| 실행 | source SHA-256 | result SHA-256 |
| --- | --- | --- |
| r1 test-build FAIL | `db75186dcb56c8d2d6786e12ce3a1876614e99500ea6e4fda3780639f603da7d` | `9dadac48820cdbdba7c987df38805a0392fe3f56c73d6a21337d0358865df9af` |
| r2 선택12 PASS | `a42fa7e538d9d9daed5cd9f6593e5499d2166d1b5dc93c9da86034e786c7768d` | `4920519ad2d9c1fef52a4aacebfb189ebd2dfd4196ae2e39e443196db393259a` |

독립 `/root/module_review`는 실제 제품 diff·원 r1/r2 source5652파일/inventory·manifest/runner/helpers·compiler/ELF/exact12와 문서6개를 읽기 전용 대조해 D1/D2를 한정 수용했다. 제품 필수 수정0이며 예약 전 제거한 guard가 pool/visibility이고 원 core/isolation guard는 남는다는 표현3곳을 정정했고 `/root`가 반영을 확정했다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았다. 무변경 코드·하위 wrapper/workspace를 반복하지 않았고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 floor·writer phase·Row/terminal/recovery·M4/M5는 미완료다.


<a id="c2-statement-reads"></a>
### C2 원 statement 기반 Catalog 조회와 자동 관측 — 2026-10-27

기준 `9f5fca6`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. [한정 공급 결정](../contracts/dev3-module-boundaries.md#catalog-statement-supply)에 따라 원 Full/Resident Scope와 실제 statement snapshot/slotgen/sequence를 연결한다. Found/Missing 전 catalog read bit를 원 isolation backing에 기록하고 기존 readonly 종료의 실제 epoch 검증을 사용한다. raw snapshot·mask 공개, 새로운 source credit, wire/형식 변화는 없다. caller loop를 장기512B로 이동하지 않았고 새 wide entry/return 및 transaction lifecycle의 전체 short floor는 미완료다.

[실행 전 manifest](../../tests/fixtures/dev3-module-c2/statement-reads/manifest.json)에 신규 실제7selector(8case)·신규 source 관측1·변경 Full/Resident class2와 새 compiler borrow3을 동결했다. Missing→statement Drop→실제 CREATE Commit→Serializable conflict, RC 같은/다음 statement, Found와 foreign Owner, 원 visibility 누락 거절, Resident Busy/Undo I/O Fatal, FlushRequired 뒤 같은 view 재시도 및 실제 결과 직후 등록 Fatal의 자동 bit와 원 resource 정리를 검산한다. compiler는 정상 소비 후 종료·종료 후 nested name 사용 금지·call이 살아 있는 동안 statement Drop 금지를 구별한다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B이며 실행 전 상태는 NOT_RUN이다. 무변경 기존 Catalog 기능/codec/전체 wrapper·workspace 시험은 선택하지 않는다.


r1 production PASS 뒤 새 시험 import 누락 E0433·잘못된 DiskSession Commit receiver E0599로 test-build FAIL, runtime9/compiler3 NOT_RUN이다. r2는 cfg(test) import와 실제 MutationPort 호출만 정정해 production을 반복하지 않았으나, 중첩한 두 fresh session을 같은 invariant ledger/core lifetime으로 요구한 fixture가 E0597로 test-build FAIL했다. runtime9/compiler3는 계속 NOT_RUN이다. 실제 두 session의 런타임 origin 시험은 이 증거에서 PASS가 아니다. 정정 fixture는 같은 original reader에서 test-only로 visibility 연결을 누락시켜 factory의 원 origin 거절을 검산하며 reader/guard/admission을 외부로 발급하지 않는다. 실제 foreign core의 Owner 거절은 별도 신규 사례에 유지한다.

독립 검토의 보완으로 새 wrapper 입력3refs+ObjectId와 statement/caller2refs를 `held_source_bytes`에 직접 합하여 큰 physical/replay lookup과 중첩 계수한다. registration/반환 phase의 별도 max로 이 부모 입력을 대신하지 않는다. 신규 원 pause cut에서 실제 Missing을 얻은 직후 bit 등록 전 fence를 세워 성공 노출0/bit0/refs0/pins0과 checked 종료의 progress0/잔여 charge를 검산한다. r3 production PASS 뒤 test-only pause가 반환 step을 borrow한 Scope를 다시 빌려 E0502로 test-build FAIL했다. runtime10/compiler3 NOT_RUN을 보존한다. r4는 cfg(test) 원 core 참조를 lookup 전에 원 수명으로 빌려 결과 뒤 pause에 사용하도록 정정했다. 제품 코드는 같아 production을 반복하지 않고 r4 test-build·exact runtime10/compiler3 PASS를 확인했다. r3 production과 r4 test-build는 경고0이며, compiler 음성 `call_after_statement_drop`은 기대 E0505와 미사용 import 경고1이 함께 존재한다. 이 경고를 지우려고 완료한 probe를 반복하지 않았고 compiler 전체 경고0이라고 표시하지 않는다.


현재 wrapper의 registration/반환 phase6678B·중첩 입력48B가 실제 source 식에 연결되어 Full34020B/Resident17636B다. shared19328B는 유지한다. 새 식은 원 소비자의 short accounting이며 실제 stack/RSS·성능·예약 전/해제 후 전체 floor 충족이나 장기512B 귀속의 증명이 아니다. 기존 snapshot 기반 Catalog 조회/codec·readonly 빈 read-set·전체 wrapper/workspace는 반복하지 않았다. 원 compile 실패와 미실행 범위를 보존한다.

원 `artifacts/module-c2-statement-reads-20261027-r{1,2,3,4}/`마다 source5658파일/inventory·manifest/runner/helpers·compiler 로그를 보존한다. r1–r3에는 test ELF/runtime 로그가 없으며 r4에 실제 ELF·10 exact selector와3개 probe별 source/diagnostics가 있다. r4 ELF는 `392c466eb162d9a983052c71104c5863d5238041b46ba44a4cbe61dce120bc03`, r3/r4 manifest는 `5c9bad4bd296b515c6e5d08ca95d809f7574eb16d87df3d0e45f682daa4d5dc0`이며 이전 r1/r2 manifest `f36a91db4a7d2042ac2b3b5307be4d1ee929614c57843a33d8ac484978e40ca8`도 원 증거에 남긴다. 실제20.73/15.29/20.56/54.83초는 성능 결과가 아니다.

| 실행 | source SHA-256 | result SHA-256 |
| --- | --- | --- |
| r1 test-build FAIL | `90d3968e17827509939bb0378ce9e8655c59694650f8fec4574bf20fd5e10a1b` | `52053434c16bbd78ef10298ec94eb3f34bf2f69f9516633fc7475fb3cfabec86` |
| r2 test-build FAIL | `3d3cabd42d7d9cce930b715251b45d385ee5f67144420512ef02a4d870e7d154` | `49ff937cb6de8289f17d40fe3d8a2259d1beb4dcbb054bc1b7cf7210f9d1c8e6` |
| r3 test-build FAIL | `2dfc91ebf6b7ecfa3630bcd9843a8ae0df48f6a10d083e73c314aa9223d4f6ff` | `f2db79cddf570ff448dcc477afd9b32a810144486e27e2190b0830d04f004844` |
| r4 runtime10/compiler3 PASS | `0b796f755f4237f05c1318024c45885afca47384cad005cff0387c6f8b0b1a71` | `524b3133ee224b1eb806315b154e738b6ef5de4a18454fdac9a13ceb70533295` |

독립 `/root/module_review`는 r1–r4 각5658 source/inventory·manifest/runner/helpers·compiler/ELF/exact10·C2_STMT01–08·probe3의 export delta/fixture hash/진단 primary line 및7문서를 읽기 전용 대조해 D1/D2를 한정 수용했다. 필수 제품·문서 수정0이며 `/root`가 반영을 확정했다. 검토자는 수정·빌드·시험·verifier를 실행하지 않았다. 변경 파일 rustfmt/diff whitespace만 확인하고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 전체 short floor·writer/Row/terminal/recovery와 M4/M5는 미완료다.

<a id="c3-install-grant"></a>
### C3 apply 소유의 실제 설치 권한 — 2026-10-27

기준 `8c0f252`, 작성·통합 `/root`, 독립 검토 `/root/module_review`다. [기존 §6.5 계약](../contracts/dev3-module-boundaries.md#apply-install-grant)의 private `InstallGrant`와 실제 Catalog ordinary/Commit/retirement 세 생산 지점을 연결했다. Catalog는 dirty_allocation의 권한 생성 경로를 거치지 않는다. generic installer는 관찰 method만 받고 원 core/fence·identity/generation·checked successor·page/prior/recLSN·exact requirement와 DPT 검산을 유지한다. Commit은 Drain 뒤 original `before`를 계속 사용한다. legacy allocation/empty/scope1/population 네 경로는 원 sealed proof를 소비해 같은 grant 안에 보유한다. 별도 System INIT/TxPair/Reload 권한은 그대로다.

source에서 grant 구성/소비·반환 carrier와 sequential accessor를 명시했으며 cap·backing·WAL/페이지 형식은 바꾸지 않았다. Grant224B·순차 관찰 closure400B·legacy conversion696B를 확인했다. 관찰은 기존 DPT660과 max로 연결해 generic N2는2396B를 유지한다. conversion696B는 새 여유를 만든 것이 아니라 원 allocator의 별도 moved2048 항 안에서 추가 검산한다. memory live peak62684B/metadata185692B/whole251228B와 recovered peak64060B, native 타입 source live62820B/metadata185828B/whole251364B와 recovered64196B다. 이 값은 source 보수 상계이며 실제 stack/RSS·성능·전체 동시 admission의 증명이 아니다. writer phase/전체 short floor·Prepared/terminal receipts 전체 이전·Row·M4/M5는 미완료다.

[동결 manifest](../../tests/fixtures/dev3-module-c3/install-grant/manifest.json)는 changed-code와 선택 이유, 총600초/compiler90초/case30초/출력8MiB/stack33554432B를 기록한다. r1 production/test build와 allocator lifecycle·empty lifecycle/source3개를 통과한 뒤, legacy L01이 첫 population WAL 비교에서 실패했다. 해당 fixture는 `e443115` 이후 current Profile2를 설치하면서 Profile1/XSG v1/scope0 literal을 비교한다. 실제 scope1 호출까지 도달하지 않았고 `live::change`는 Profile2를 기존부터 WrongState/Unchanged로 차단한다. 원 FAIL을 보존하며 literal을 actual로 덮거나 legacy fresh factory를 추가하지 않았다. 신규 current Profile2의 다섯 legacy entry 효과0 거절은 별도 검증이며 legacy positive 설치의 PASS가 아니다(`NOT_REACHABLE_CURRENT_PROFILE`). actual population 설치는 Catalog fixtures의 실제 초기화로 검산한다.

r2는 새 cfg(test) `page_ref`의 incarnation 인자 누락 E0061로 test build FAIL이며 runtime7/compiler5 NOT_RUN이다. r1 완료3·production build는 반복하지 않았다. incarnation1을 명시한 r3는 test build·남은 runtime7·compiler5 PASS, 경고0이다. r1 완료3과 합쳐 현재 선택10의 source별 결과를 수신하며 원 L01 FAIL은 별도 보존한다. compiler5는 관찰 사용 성공, private kind E0616·Clone 부재 E0599·소비 후 재사용 E0382·WAL-private legacy factory E0603의 정확한 primary span을 검산했다. Commit Pending lower flush와 두 partial install 실패의 원 fence/잔여 의무 및 메모리 FileIo의 현재-build 복구를 검산했다. native 타입 source 검산은 새 native process/I/O 실행 증거가 아니다. 원 source/ELF/로그와 FAIL·NOT_RUN을 각 artifact에 보존한다. 독립 D1/D2는 한정 수용했으며, 검토자는 수정·빌드·시험·verifier를 실행하지 않았다.

원 `artifacts/module-c3-install-grant-20261027-r{1,2,3}/`에 source/inventory·manifest·runner/helpers·compiler·ELF(생성 시)·각 case와 compiler probe source를 보존한다. r1 source5666파일, r2/r3 5667파일이며 실제32.61/15.23/76.91초는 성능 결과가 아니다.

| 실행 | source SHA-256 | manifest SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 build2/runtime3 PASS, L01 FAIL | `0d5b2dd56326e6f78c70af95c9eff1386afd32d18f82b6ddf4e034d560970956` | `6ae0166d863caba2ef8e86ccb1e22fcb717a3ee6769d92d40e0682b88cb4c6e4` | `30198e7d567e130574bf80ca112b56484c7d880a87c29242d4187cf83c8475c4` |
| r2 test build FAIL; runtime7/compiler5 NOT_RUN | `348fd8890123023787eb584edaa42af1cc00db93cb3c4590c889e55a309d397e` | `c259ca0d7980219b82621e16d55fe8b602c9bddb6d6e970b4ff753748264c265` | `a073d6591e05a0845c428792af0b4b2222d81ff415afaafd50ddaeeb155b2d44` |
| r3 test build/runtime7/compiler5 PASS | `b6a202b9dbe64662131d9d9630a9a9b4e94ea46067616e900645eb940eb61bc6` | `c259ca0d7980219b82621e16d55fe8b602c9bddb6d6e970b4ff753748264c265` | `2a4e7ee34a58c2b95b1a445ee67593b49b982b5e8349af9bd319b3641c601ec9` |

r1 ELF `d7349c5cd8a602ff6c2852d17131cef5001c2d58cce4efe2ea223a23e36b8616`, r3 ELF `419bd716615ff5fef8597d2dd15ef172924d3eae9f30e1334c046f33a3567b9a`다. r2/r3 production은 r1과 코드가 같아 NOT_REPEATED_UNCHANGED_CODE다. 완료된 원 runtime3·무변경 하위codec/회귀·workspace 시험은 반복하지 않았으며 변경 파일 rustfmt·diff whitespace만 확인했다. 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 독립 D1/D2는 r1–r3 전체 source/inventory·fingerprint·manifest·runner/helpers·ELF·원 로그 및 compiler probe5의 exact delta/diagnostic span과6문서를 대조해 한정 수용했다. 필수 미해결 수정은0이다.


<a id="c3-terminal"></a>
### C3 실제 Catalog terminal의 apply 소유 — 2026-10-27

기준 `4cf67bf`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. [원 terminal 계약](../contracts/dev3-module-boundaries.md#apply-terminal)에 따라 실제 semantic receipt 정의·factory/검산을 apply root/private terminal로 옮겼다. 물리 runtime과 append/drainer/file/slot·원 core/pins·exact requirement·digest·whole install/Q 조건은 유지한다. canonical disk receipt만 generic core/installer에 재노출하며 실제 Attempt/Installed/getter는 apply 내부로 제한한다. 형식/semantic wire 변경은 없다.

[동결 manifest](../../tests/fixtures/dev3-module-c3/terminal/manifest.json)는 actual Busy 신규1, MF02 stage 진입 중단·MF03 write-before·MF07 sync-before·MF11 전체 설치 후 중단/현재-build memory recovery·MT01 정상 Q 공개 및 변경 source 관측3의 exact runtime9와 compiler4를 선택했다. compiler는 raw LSN factory 부재·Pending 이중 소비·private binding·Installed Clone 부재를 검사한다. 과거 compiler2의 원 fixture는 보존하고 current disk import/drain API를 별도 새 probe로 수신했다. 총600초/compiler90초/case30초/출력8MiB/stack33554432B이며 무변경 codec·MT05/MF10·하위 회귀·전체 wrapper/workspace는 반복하지 않는다.

r1 production build PASS, 새 시험의 `unwrap_err`/직접 Cause 비교가 기존 Debug/PartialEq를 요구해 E0277/E0369 test-build FAIL이다. runtime9/compiler4는 NOT_RUN이다. r2는 cfg(test) 오류 표현만 수정해 test build PASS했지만, 새 competing stage가 legacy physical decoder `try_stage`를 통해 scope2 frame을 읽어 UnsupportedFormat을 반환했다. controller panic/release Disconnected로 신규 Busy fixture FAIL, 후속 runtime8/compiler4 NOT_RUN을 보존한다. r3는 원 current Profile2 `semantic_catalog::decode_layout`로 같은 immutable frame을 검산하고 실제 원 `stage_verified`에 해당 header를 전달한다. 제품 코드는 같으므로 r2/r3 production은 NOT_REPEATED_UNCHANGED_CODE다.

Busy fixture는 actual Prepared.frame과 독립 C04 literal equality를 먼저 검산한다. StageCallStarted pause 동안 원 physical slot에 동일 frame을 점유시키고, 원 terminal의 실제 Busy/Unchanged→authentic Unstaged→restore/cancel을 실행한다. 원 Owner idle·Open/active0·pins0·frame/DPT/control/jobs/epoch/refs/Q/files 불변·I/O0 및 원 call 무 fence를 검산한다. 외부 주입의 complete9/synced8·원 first-write/동기 offset을 보존한 뒤 시험이 명시적으로 fence하고 종료한다. 미설치 물리 의무를 정상 처리한 것으로 기록하지 않으며 다중 semantic writer 지원 증거가 아니다.

r3 test build·exact runtime9·compiler4 PASS, build/probe 경고0이다. 원 MF11 memory FileIo의 현재-build 복구와 정상 MT01의 exact C04/Q 공개를 확인했다. native 타입 source 검산은 native process/I/O 실행 증거가 아니다. 독립 D1/D2는 actual diff·r1–r3 전체 source/inventory·fingerprint·manifest·runner/helpers·ELF·원 로그·probe4 exact delta/diagnostic span 및6문서를 대조해 한정 수용했다. 필수 미해결 수정은0이며 검토자는 수정·시험·빌드·verifier를 실행하지 않았다.

임시 adapter/receiver16B가 원 source 식에 추가되어 세분 wait 관측의 bridge3272B/Pending5336B가 됐다. 보수적인 phase Pending9136B·production broad36844B와 혼합하지 않는다. Full34020B/Resident17636B/shared19328B 및 원 memory live62684B/recovered64060B, native 타입 live62820B/recovered64196B는 유지된다. Full+세분 Pending+shared58684B는 cfg(test) 관측이고 전체 entry/tail·writer/progress admission의 완료 증거가 아니다.

원 `artifacts/module-c3-terminal-20261027-r{1,2,3}/`에 source/inventory·manifest·runner/helpers·compiler·ELF(생성 시)·case logs와 compiler probe source를 보존한다.

| 실행 | source SHA-256 | manifest SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 production PASS/test build FAIL | `2586b276fa41626c8130e8314b9ce9b242389d2418009c656625983422da3cf5` | `47cde5bf37f41342c01276adea30c77bc3d51bfaa59f62efd039c887240bbc0d` | `334fff79ddf5401327aa2ace3de0bdb9efc33aa327f0fc5bdd3bbb064f2e0f12` |
| r2 test build PASS/new Busy FAIL | `60b6f7ebd6de56a47e6be6fb5f445f220bb6b175e6b922a87ef5c31e17351d68` | `47cde5bf37f41342c01276adea30c77bc3d51bfaa59f62efd039c887240bbc0d` | `05295cc7c12d1cb44db2a4d5d423a041c5e21fa3b766239d3497bc0e1f8e9a80` |
| r3 test build/runtime9/compiler4 PASS | `8782501923622d54e70d0e14d553897475cc993e2b4c91e2773621d2e29754c2` | `47cde5bf37f41342c01276adea30c77bc3d51bfaa59f62efd039c887240bbc0d` | `b427d198dbd0fa543784a7e34a04153d93712901fbad46567c9bb810ae38cd05` |

r1 source5673파일·20.28초다. r2 source5673파일·27.65초·ELF `3c8d7a9208244a190bb79fd357e6970c6cd5e9eb786d3815a93c53b937052cd3`다. r3 source5673파일·64.44초·ELF `0fae7eeb9e5cee636b49d32dec494c551644fd7bb0c36090119130059344106b`다. 실행 시간은 성능 결과가 아니다. r1/r2 FAIL·후속 NOT_RUN과 무변경 production 생략을 보존하며 완료된 runtime은 반복하지 않았다. 변경 rustfmt·diff whitespace만 확인하고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다.


<a id="c3-prepared"></a>
### C3 Prepared 실제 정의의 apply 경계 — 2026-10-27

기준 `4f8afb7`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. [소유 계약](../contracts/dev3-module-boundaries.md#apply-prepared)에 따라 PreparedCatalog의 실제 정의·private field를 apply root로 이동하고 canonical disk export·내부 imports만 연결했다. field type/order·생성/검산/소비 impl·source 계산·wire는 그대로다.

[사전 manifest](../../tests/fixtures/dev3-module-c3/prepared/manifest.json)는 production/test target build 및 신규 compiler4(관찰 성공, private Owner field E0616:4, Clone 부재 E0599:4, 실제 commit_catalog에 같은 Prepared 두 번 전달 E0382:10)를 선택했다. 총300초/compiler90초/출력8MiB이며 runtime/source census/하위 codec/회귀는 변경이 없어 NOT_REPEATED_UNCHANGED_CODE다. 실행 결과와 독립 검토는 아래에 기록한다.


r1 production/test build·compiler4 PASS, 모든 build/probe 경고0이다. 원 `artifacts/module-c3-prepared-20261027-r1/`에 5678파일의 immutable source/inventory·runner/helpers·ELF·probe export/diagnostic을 보존한다. source `7a1c1ea402eb1c4e5002a61b1bf7db4f5a574cc4ff6268618c3c019aa82e38ce`, manifest `62b62a797a4fe736d13ad06330212b43c2749e4e510f5ed208e540583dd6fd75`, ELF `3ef827b3b087cdd88fc897694b71bfcec2a9d16a4e0ed878c3dfbb7a2d1184ca`, result `f9b3fde7fce8005f58a75a10f2255a44e99b1e92c362bfdd6a46e7661e9493f1`다. 실제 52.58초는 성능 결과가 아니다. runtime을 실행하지 않았으며 기존 source별 PASS를 새 실행으로 표시하지 않는다. 변경 rustfmt·diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 독립 D1/D2는 제품8경로·source5678파일 전부/inventory/fingerprint·runner/helpers/manifest/ELF·원 compiler logs와4 probe의 exact two-file delta/diagnostic span 및6문서를 대조해 한정 수용했다. 필수 수정은0이며 검토자는 수정·빌드·시험·verifier를 실행하지 않았다.


<a id="row-root3-codec"></a>
### Row Root3의 exact 순수 cell codec — 2026-10-27

기준 `57373f0`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. [Row format owner](../contracts/dev3-module-consumers.md#root3-codec)에 따라 exact Root3/row_high@432/reserved440..512와 canonical Free Job3 digest를 별도 모듈로 구현했다. raw512B view는 원 input을 borrow하며 pin/issuer/reservation/READY 권한이 아니다. active commitments는 전체 Job3 census 전의 opaque 관찰이다. current Profile2·legacy Root2·기존 decoder/선택/복구 호출은 바뀌지 않는다.

[사전 manifest](../../tests/fixtures/module-row-format/root3/manifest.json)에 신규4 selector와 독립 `author.py`/literal3의 hash를 고정했다. Python struct/표준 SHA-256은 XADB encoder를 호출하지 않는다. 초기 byte equality와 borrowed pointer, high-water0/1/MAX·55/56 cap·checked accounting overflow, 짧은 길이0..511/긴513·version0/1/2/4/65535·state/reserved/namespace/6refs/Aging/masks, encode 실패의 output 불변을 검산한다. 전체 Job3/census/page wrapper·current dispatch/복구는 선택하지 않았으며 기존 Root2/하위 codec은 변경이 없어 반복하지 않는다. 총300초/compiler90초/case30초/출력8MiB/stack33554432B다.

r1 production/test build와 신규 첫2 selector PASS 뒤 identity 변형 시험이 FAIL했다. FileId의 낮은 segment1을0으로 바꾸어 구조적으로 잘못된 PageRef를 만들었는데 IdentityMismatch를 기대했으며 실제는 Page(Corrupt)다. 이 원 FAIL과 마지막 output-preservation 사례 NOT_RUN을 보존한다. r2는 유효한 다른 page_no를 identity 변형으로 사용하고, FileId/zero-incarnation 구조 오류12개를 별도 Page(Corrupt)로 검산한다. 제품 코드는 같으므로 production 및 이미 완료된 첫2 selector는 반복하지 않는다. 실행 결과와 최종 독립 검토는 아래에 기록한다.


r2 test build·남은2 selector PASS이며 r1의 첫2와 합쳐 신규4개의 source별 결과를 보존한다. build 경고0, 원 FAIL은 소급 변경하지 않는다. raw cell 검산에 한정하며 actual issuance/current-format persistence/복구 또는 전체 source admission 증거가 아니다.

| 실행 | source SHA-256 | manifest SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 | `53d98cbb387f316535fdb42c6394d32f01c75228bb5ee929586e560f691f3d73` | `6b5f202f31f695e891a45086b57c29ae45607f03012a752bf168a39c03feb89d` | `c65ef265df45ca45524441f63e26f6193c5bce40ff3b959585ba97f01f666d37` |
| r2 | `1c5b45e91a854955f3addd78aa54cb1e29e77531c4298eab08c0cfd22b72bb68` | `6b5f202f31f695e891a45086b57c29ae45607f03012a752bf168a39c03feb89d` | `6e561e3623e973c9dea0ec20d69dd9de0e3195274b942b7709dc2455755278e2` |

r1 source5686파일·ELF `cf28092531fe3749d19db1a0e9bbd81e957e987a1485534be9eac43c7f545a5c`·31.42초다. r2 source5686파일·ELF `c1b8f5184b54dfb238c3d344b47e1ef858ba30ae13b9b8762bb92af30079d37a`·26.66초다. 원 `artifacts/module-row-root3-20261027-r{1,2}/`에 immutable source/inventory·manifest·runner/helpers·ELF·build/case logs를 보존한다. 시간은 성능 결과가 아니다. 변경 rustfmt/diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 독립 D1/D2는 source/원 결과와6문서를 대조해 한정 수용했으며 필수 수정은0이다. 검토자는 source5686파일 전부/inventory/fingerprint·manifest/runner/helpers/ELF/log와 독립 literal3의 전 바이트·hash를 검산했고 제품·fixture 수정/실행·빌드·시험·verifier는 실행하지 않았다.


<a id="row-job3-census"></a>
### Row Job3의 exact cell·64행 census — 2026-10-27

기준 `2c9964d`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. [owner 구체화](../contracts/dev3-module-consumers.md#job3-census)에 따라 별도 exact Job3 decoder/encoder·scope/h recipe 관찰·다중 Row root/actor·unique identity·Terminal/시간 관계·shared table·Root commitment/aggregate를 구현했다. 순수 cell 검산이며 actual Status/Undo/descriptor/allocator/history·page wrapper·issuer·source admission·current Profile3/base4/Row 실행은 미공급이다. 기존 Profile2 및 Root2/Root3 codec 본문은 바꾸지 않았다.

사전 독립 검토에서 same-table Catalog Retained도 미커밋이면 허용할 수 없음을 확인했고 committed terminal/RowReserve LSN 관계를 추가했다. 원 초안의 RootView와 별도 namespace 불일치, Commit 뒤 CLR로 해석되던 positive fixture를 수정하고 namespace 결합·Retained/Prune/CLR/cleanup 시간 표를 동결했다. 테스트 실행 실패를 없애기 위한 사후 기준 변경이 아니며 아래 첫 실행 전 보완이다.

[사전 manifest](../../tests/fixtures/module-row-format/job3/manifest.json)에 신규11 selector·독립 Python author와31 cell/4 complete bank/Root/aggregate literals·index의 hash를 고정했다. 정수 offset/표준 SHA-256으로 작성하며 제품 encoder/decoder/기존 author를 호출하지 않는다. 모든 occupied budget byte30×640 변형, version/identity/시간/alias/namespace 및 actor/census 반례는 해당 신규 코드 검산이다. negative bank는 별도 산식으로 commitment를 정상화하여 hash 실패가 의미 관계 검사를 대신하지 않게 한다. 총300초/build90초/case30초/출력8MiB/stack33554432B, compiler probe0이다. 기존 Root2/완료 standalone Root3/하위 시험은 변경이 없어 NOT_REPEATED다. 실행 결과·최종 독립 검토는 아래에 기록한다.


r1 production/test build·신규 exact runtime11 PASS, build 경고0이다. 원 `artifacts/module-row-job3-20261027-r1/`에 5696파일 immutable source/inventory·manifest·runner/helpers·ELF·원 build/case logs를 보존한다. source `bd1c630474fdc2b8dd18bb94e0b7dc0f49bc5879e328c0e296562a4b09eb8616`, manifest `555708a488d8371bbe2b4750dbb9e4dee5a4c5119ea7481841cac87b5e66cc19`, ELF `40487659b016114c34f1cd614762b99b132c860421de663a6eb00186e1d74054`, result `1c4153182d2026360de53c684b4ba039736eafe10bcfc0632f109406d12eef9e`다. 실제32.34초는 성능 결과가 아니다. 기존 Root2/완료 standalone Root3/하위 시험을 반복하지 않았으며 생략을 새 PASS로 표시하지 않는다. 변경 rustfmt·diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 독립 D1/D2는 한정 수용했으며 필수 수정은0이다. 검토자는 source5696파일 전체/current 일치·fingerprint/inventory·manifest/input/helpers/runner·ELF·13행(build2/runtime11) 원 로그/경고0·6문서/신규10+수정1 경로를 대조했다. author를 실행/import하거나 제품 encoder를 사용하지 않고 표준 offset/SHA 산식으로31 cell·4×64 Job bank·Root4·aggregate4의 전 바이트를 독립 재구성해 일치를 확인했다. README의 직전 Root3 요약은 당시 단위 미포함과 후속 Job3 공급 링크로 구별했다. 검토자는 제품/fixture 수정·빌드·시험·verifier를 실행하지 않았다.


<a id="row-envelope-codecs"></a>
### Profile3·Root/Job3의 별도 envelope 검산 — 2026-10-27

기준 `f8f73b0`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. [owner 구체화](../contracts/dev3-module-consumers.md#row-envelope-codecs)에 따라 pure exact Profile3 입구와 Row page wrapper4를 공급한다. 기존 bootstrap1/2·non-Profile1 의미를 유지하고 actual current Profile2·genesis/selection/base/replay/recovery는 바꾸지 않는다. 원 lower codec은 그대로이며 wrapper 성공은 전체 graph/census/admission 권한이 아니다.

[사전 manifest](../../tests/fixtures/module-row-format/envelopes/manifest.json)에 신규8(selector Profile4/page4)·독립 author/Profiles3/pages12/header-only FileMap1·이전 독립 cell4입력의 hash를 고정했다. 표준 struct/SHA와 hash가 고정된 원 독립 Root3/Job3 literal만 사용하며 제품 encoder/decoder/다른 author를 호출하지 않는다. 새 wrapper의 positive에만 별도 전체 census 호출을 명시하며 wrapper 자체의 권한으로 승격하지 않는다. 공유 header의1B typed mode로 바뀐 원 bootstrap/Profile1/2/non-Profile1의 수신·거절 순서를 신규 selector 하나로 검산한다. 기존 완료 Root2/Root3/Job3 selector 및 무변경 control 전체는 반복하지 않는다. 총300초/build90초/case30초/출력8MiB/stack33554432B다. 실행 결과와 최종 독립 검토는 아래에 기록한다.

사전 검토의 추가 helper carrier 지적 뒤, 첫 실행 전에 nested helper 초안을 없애고 원 함수의 bool을1B enum으로 대체했다. 새 helper frame·추가 Envelope 반환을 만들지 않는다. owned type/charge 수치 변경0과 새 pure codec aggregate 미공급을 구별하며 원 source 값이 전혀 증가하지 않았다는 별도 주장은 하지 않는다.

r1 production/test build·신규 exact runtime8 PASS, build 경고0이다. 원 `artifacts/module-row-envelopes-20261027-r1/`에 5705파일 immutable source/inventory·manifest/input/parents·runner/helpers·ELF·원 build/case logs를 보존한다. source `f6e6666c358972dfeda1ee0304eef0791b1dfd32c99978a4a4e4b515070aae3e`, manifest `d603fbfd26b5308b9f2953516daae467e757e3dd33f2e2619003a89dd603c33d`, ELF `2255a8171805aa4002490d4ba707366159d49b22311fbe89a78be61557dd1e91`, result `ab4814c0a420e2e1400020700920907dd520c01feef74811ef911146fa4b1ccd`다. 실제32.91초는 성능 결과가 아니다. 기존 완료 codec/무변경 control 전체를 반복하지 않았다. 변경 rustfmt/diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 독립 D1/D2를 한정 수용했으며 필수 수정은0이다. 검토자는5705 source 파일 전체/current 일치·inventory/fingerprint·manifest/input4/parent4·runner/helpers·ELF·build2/runtime8 원 로그/경고0·예산·6문서/신규9·수정2 경로를 대조했다. 제품/author를 실행·import하지 않고 표준 offset/SHA 산식으로 Profile3 IDs1–3·pages12·header-only FileMap의 전 바이트를 독립 재구성해 동결 입력과 일치함을 확인했다. 제품/fixture 편집·빌드·시험·verifier를 실행하지 않았다.


<a id="row-base4-layout"></a>
### Base4 별도 raw layout와 공통 image 기록 — 2026-10-27

기준 `7071d21`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. [owner 구체화](../contracts/dev3-module-consumers.md#row-base4-layout)의 별도 Profile3/Base4 입구를 구현한다. 동일 certificate/INIT grammar 본문은 그대로 공통으로 옮기고 typed header mode·exact tuple·legacy 거절 경계를 닫는다. actual current는 Profile2이며 신규 raw layout 성공은 full graph/Job census/descriptor/ownership inventory/history/INIT witness·selected owner·source admission을 뜻하지 않는다.

[사전 manifest](../../tests/fixtures/module-row-format/base4/manifest.json)의 신규8 selector와 stdlib independent author·initial/active 각2MiB 및 hash 고정 parent6을 동결한다. 초기 P00/P07/P08 및 초기/활성 Root/Job3는 원 독립 bytes를 빌린다. 활성 입력은 TerminalLeased2·leased3, Status/Undo 동적 record와 Catalog pair INIT, Reserved13/Free14를 포함한다. generic dynamic page envelope·opaque descriptor/inventory와 prefix/bootstrap/FileMap/INIT digest claim은 실제 graph/역사 proof가 아니다. 부정 사례는 section/page/hash를 다시 정상화해 hash 실패와 의미 경계 거절을 구별한다. 같은 source의 완료 하위 시험을 반복하지 않으며 shared header 변경에 대한 기존1/2/3 gate만 신규 사례에서 검산한다. 총300초/build90초/case30초/출력8MiB/stack33554432B, compiler probe0이다. 최초 실행 전 page kind와 Root mask offset을 정적 대조해 수정했으며 실행 결과를 보고 기대값을 바꾼 것이 아니다. 실행 결과와 최종 독립 검토는 아래에 보존한다.


r1 production/test build와 신규 exact runtime8 PASS, build 경고0이다. 원 `artifacts/module-row-base4-20261027-r1/`에 source5712파일/inventory·manifest/input/parent·runner/helper·ELF·원 build2/case8 로그를 보존한다. source `f471637b20588a8c8ffacd709230bd62c2c93f9cf5fbe2ce1774e4103ba55293`, manifest `cb165fc8fd4fc3ddec38debc6fe84fcfe6bf203e566395ccb4cf68ecf49b2b49`, ELF `1eadbe64c5884428ed59d0dda4bda3871ae6d04718d0a2b0c272032fd5cc6357`, result `6cf242d7bd7c49aaf1347bda1aa4fcb3cf7ea5c04d1465d86734f95b58bb1192`다. 실제39.90초는 성능 결과가 아니다. 첫 실행 전 독립 검토가 찾은 이동 코드의 import3개(PageKind2/PAGE_BYTES1)를 복구했다. 제품 predicate/권한 변경이 아니며 실행 실패 기록은 없다. 완료 하위 시험은 NOT_REPEATED이며 새 PASS로 표시하지 않는다. 변경 rustfmt/diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 최종 독립 D1/D2를 한정 수용했으며 필수 수정은0이다. 검토자는5712 source파일/current 일치·inventory/fingerprint·manifest/input3/parent6·runner/helpers·ELF·build2/runtime8 원 로그/경고0·예산·6문서/신규7·수정2 경로를 대조했다. 원 공통14 accessor/codec 본문은 whitespace 제외 이전과 같음을 확인했다. 제품/author를 실행·import하지 않고 표준 offset/SHA 산식으로 초기/활성 각2MiB의 전 바이트·hash를 독립 재구성해 일치함을 확인했다. 제품/fixture 편집·빌드·시험·verifier는 실행하지 않았다.


<a id="row-aging"></a>
### Root3-bound Aging descriptor와 전체 queue 후보 — 2026-10-27

기준 `922bc62`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. [별도 owner 입구](../contracts/dev3-module-consumers.md#row-aging)를 공급한다. raw encode/decode의 Root3 binding과 next의 after64 Job census를 구별하고 actual opcode/history/admission/current Profile3 복구는 연결하지 않는다. 기존 Catalog/완료 하위 selector를 반복하지 않는다.

[사전 manifest](../../tests/fixtures/module-row-format/aging/manifest.json)는 신규5 selector, stdlib independent author와 pages6/jobs5bank/descriptors5/aggregate5, 원 독립 parent5의 hash를 동결한다. 빈/Catalog-only/Row-only/혼합/마지막빈 현재 관측을 검산하며 후보 사이의1000B 관찰 차이는 실제 WAL opcode가 아니다. 총300초/build90초/case30초/출력8MiB/stack33554432B, compiler probe0이다. 사전 독립 검토에서 첫 Start가 genesis job generation/high-water/cursor를 바꿀 수 있는 경계를 닫고, 형식 유효한 source.local99에 Root commitments까지 정상화한 뒤 full census가 actor 누락을 거절하는 사례를 첫 실행 전에 보완했다. 실행 결과와 최종 독립 검토는 아래에 보존한다.


r1 production/test build·신규 exact runtime5 PASS, build 경고0이다. 원 `artifacts/module-row-aging-20261027-r1/`에 source5720파일/inventory·manifest/input/parent·runner/helper·ELF·build2/case5 원 로그를 보존한다. source `e60d36c0f45be78d88188c4ed955cacf5f9b408516519797e6babc430522acac`, manifest `c84ca2fd8a11923766e556a120cc517e2ce34b0100453262362d24d3e96ad01e`, ELF `a995b541537f84215bb08a1520cdc9100346fe635167f22bd76cc956ba48d037`, result `fd6b2e21d219b58da5917e684517ab9c87bb7ba5725f9b78e1fc3dc3f16d7c29`다. 실제32.61초는 성능 결과가 아니다. 완성한 Base4/Job3/원 codec 및 lower selector를 반복하지 않았으며 생략을 새 PASS로 표시하지 않는다. 변경 rustfmt/diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 독립 D1/D2를 한정 수용했으며 필수 수정0이다. 검토자는5720 source 전 파일/current 일치·inventory/fingerprint·manifest/input5/parent5·runner/helpers/ELF·build2/runtime5 로그/경고0/예산·6문서/신규8·수정1 경로를 대조했다. 제품/author를 실행·import하지 않고 별도 offset/SHA 산식으로 pages6/jobs5bank/descriptors5/aggregate5의 전 바이트를 독립 재구성해 일치함을 확인했다. registry peak `[0,9,16,16,0]`, cleanup steps `[0,6,26,12,0]`, WAL `[0,367228,1584590,734318,0]`를 검산했으며 actual opcode 이력으로 승격하지 않았다. 파일 편집·제품/author 실행·import·build/test/verifier는 수행하지 않았다.


<a id="row-transaction-ledger"></a>
### Row 거래 현재 상태와 예약 잔여 — 2026-10-27

기준 `0aa4bf1`, 작성/통합 `/root`, 독립 검토 `/root/module_review`. 사용자가 다른 에이전트의 수정이 아니라면 작성 중 변경을 완료하도록 요청했다. 이전 이 채팅의 작성 명령과 현재3파일을 대조해 `/root`의 미커밋 초안임을 확인했다. 전체 일시중지 목표를 재개하지 않고 [Row ledger 단위](../contracts/dev3-module-consumers.md#row-transaction-ledger)의 구현·선택 검증·독립 검토·등록·commit을 마무리한다. 최초 구현 검토의 필수 수정0을 수신했으며 실행 수용은 아직 아니다.

r1 최초 동결은 stdlib 독립 author·현재 관측19개(Job bank/StatusHome/5행 budget/descriptor16개와 descriptor 없는3개), hash 고정 parent Job3/Profile3 및 신규8 selector를 명시했다. 현재 [동결 manifest](../../tests/fixtures/module-row-format/ledger/manifest.json)는 보완1개를 포함한 최종9 selector와 r2 선택5개, 원 r1 결과·재선정 근거를 담는다. Active1/3, Reserved h0/h1와 C185/2228, 다중 Retained, statement rollback/mark 도달, full abort/완료, Commit한 Retained와 compensated q0의 공존, Prune/Release 후 관측 및 취소 mutation high를 포함한다. 입력은 실제 WAL trace가 아니며 nonempty inventory digest는 physical owner가 검산할 opaque claim이다. normal/negative input으로 actor/location/order/head/count/h/rollback/terminal·정확 잔여5행/6080·descriptor 수명을 확인하고 다른 actor 전체 census와 physical digest의 한계를 직접 구별한다.

총300초/build90초/case30초/출력8MiB/stack33554432B, r1 최초 선택은 production/test build와 exact8이며 compiler probe0이다. 완료한 Catalog/Root3/Job3/Base4/Aging/하위 selector는 반복하지 않는다. 실제 source별 실행 결과와 최종 독립 D1/D2 수용은 아래에 보존한다. actual graph/history/current 복구·live source/admission·A/B와 M4/M5는 미완료다.


r1의 production/test build와 첫6 selector는 PASS, 7번째 `terminal_descriptor_lifetime_and_gaps`는 FAIL, 8번째 scope selector는 NOT_RUN이다. 실패는 post-abort release를 뜻하는 bank15의 compensated cursor>0 last_step을41..43으로 작성하여 Terminal adoption50보다 앞선 입력에서 옛 apply50 descriptor의 거절을 기대한 것이다. pure ledger는 전체 queue/history chronology의 권한이 아니므로 그 입력에서 apply≥latest가 성립했다. 제품을 바꾸거나 기대 거절을 완화하지 않고 author를 고쳐 release cursor>0 step을61..63으로 만들었다. bank12/13/15만 변하며 homes/descriptors/budgets/index는 불변이다. literal selector는 postterminal step>Terminal adoption이라는 입력 선행조건도 직접 검사한다. 원 source/fixture/FAIL을 r1 export/log에 보존한다.

r2는 cfg(test)와 fixture만 바뀌어 production build 및 미변경 PASS4를 반복하지 않았다. 바뀐 bank를 읽는 literal/rollback2, 원 실패 lifetime1, 미실행 scope1, 독립 검토가 추가한 full-abort next≠head(U12/None)1의 총5 selector와 test build가 PASS이며 경고0이다. 따라서 최종9 selector의 유효 증거는 r1 unchanged4+r2 selected5이고, r1의6 PASS 전체를 최신 source PASS로 복사하지 않는다. actual caller/제품 ledger 의미 변화는0이며 완료한 하위 selector·workspace·wrapper·native test는 실행하지 않았다.

| 실행 | source SHA-256 (각5730파일) | manifest / ELF / result SHA-256 | 실제 범위 |
| --- | --- | --- | --- |
| r1 | `2bf6b31729b5d050ea2b5cd96be113f31f3a202c5a5424bc5a590e8a67792576` | manifest `510b0d1d19eb0b86d3f92fee7b71afc4a53df641cdfb7d576ac83fa081bebad6`; ELF `495c9c88c6429197f16b44caa5b8a5437cbc1c239fc8bfdbacceec99c53609b5`; result `b9ac527739e46bad2089f44d7dafcb0f41ea31aa6d8c412c282e0e1bda299db0` | production/test build2·runtime6 PASS, runtime1 FAIL, 다음1 NOT_RUN |
| r2 | `d93ebe70e99ec2f97eba7971ed6b5d01a1ac6796d730a740c8a313181068b4cd` | manifest `bc79a5b0414a52df6cd4e0eec9cc40964c80296f7c77be5209fe698aa0b4bad3`; ELF `295554b09632d8b6a937f2e7fda82e50d3bcbd59d99efdd7c80cd2d8ee0a4e4f`; result `449620886485efbb45ce8d63fa88928eeb7868463edcd17ca8de27775274054a` | test build1·관련/new runtime5 PASS, production/unchanged4 NOT_REPEATED |

원 `artifacts/module-row-ledger-20261027-r1/`와 `r2/`에 source/inventory·input/parent/manifest·runner/helper·build와 exact case stdout/stderr를 보존한다. 실행31.87/27.38초는 성능 결과가 아니다. 총300초/build90초/case30초/출력8MiB/stack33554432B 안에서 종료했으며 독립 작성 입력과 변경 범위 연결은 manifest에 고정했다. changed rustfmt/diff whitespace만 확인했고 전체 fmt/Clippy·문서 verifier·push는 실행하지 않았다. 최종 독립 D1/D2는 아래 범위로 한정 수용했다.


독립 `/root/module_review`가 제품 구현·입력·선택 인과성·원 실패·6문서를 읽기 검토하여 **D1/D2 한정 수용, 필수 수정0**을 판정했고 `/root`가 반영을 확인했다. 각5730파일의 원 inventory/전체 hash/fingerprint, r2 현재 source 일치, r1→r2의 test/author/jobs/manifest4파일 차이를 대조했다. input6/parent2·runner/helper3·ELF/result·build3/runtime12 원 로그/hash·경고0과 출력38463/26663B를 확인했다. author/product를 import하거나 실행하지 않고 독립 offset/SHA 산식으로 최초4binary 전체를 재구성했으며 r2의4byte LSN 변경과 나머지 binary/index 불변을 확인했다. FullAbort next 새 분기와 r1/r2 시점 문구의 최종 보완도 수신했다. 검토자는 제품/author/build/test/verifier를 실행하거나 파일을 수정하지 않았다. 이 수용은 현재 actor/descriptor 검산 단위에 한정하며 실제 graph/history/mark/정규 inventory/source admission/current Row·A/B·M4/M5 완료가 아니다.


<a id="merge-main-20261028"></a>
## 모듈 개발 브랜치의 main 통합 — 2026-10-28

사용자는 `codex/module-independence-finish`를 `main`에 병합하고 이후 main에서 진행하도록 지시했다. main 부모 `cbc09f2779847e7ee1452a0028d93d0b52fa2083`와 finish 부모 `76f91e00e8cd3b957246f62cf6d1724078658a67`를 두 부모 병합으로 결합한다. 작성/통합 `/root`, 독립 검토 `/root/merge_review`다. 앞서 main에서 후속 작업을 별도로 진행한 중복을 반복하지 않도록 [작업 지침](../../AGENTS.md)에 재개 위치 확인과 이후 `/home/et16/work/xadb`의 main 사용을 기록했다.

기존 병합의 충돌36파일을 이어서 해결했다. main의 원 core816B 단일 계수·물리/의미 Commit4phase·`restore_recovered_control` 반환 경계·조회 forwarding Result 계수를 유지하고, finish의 compact source/오류·split port·statement 읽기·InstallGrant·TerminalWal·Prepared·Row 순수 codec/ledger 부분공급을 결합했다. 병합 중 생긴 시험 모듈 중복 등록과 helper 인자 불일치는 수정했다. Row actual adapter·terminal/recovery 연결, 전체 source 동시 admission·C3 나머지·M4/M5 제품 인수는 미완료다.

### 원 기록과 미완료 초안의 보존

- `stash@{0}`의 C2 미커밋 초안 `7df6e9215877b681f4cb297ba86bf8afa5c1fcf1`는 병합 전에 보관된 별도 진행분이다. 이번 병합에 적용하거나 개발을 재개하지 않았다.
- 충돌 전 파일·선택 내역·양쪽 부모의 충돌 manifest8개는 `artifacts/module-independence-merge-main-20261028/`의 `original-conflicts/`, `resolution-choices.json`, `parent-manifests/`에 보존했다. 현재 fixture manifest는 finish의 후속 선택 정보를 채택하며, 과거 main 실행의 원 manifest로 해석하지 않는다. 두 부모 commit도 원 문서/manifest를 보존한다.
- 기존 main의 원 실행물은 `/home/et16/work/xadb/artifacts/`, finish의 원 실행물은 `/home/et16/.codex/worktrees/module-independence-finish/xadb/artifacts/`에 유지한다. 같은 상대 artifact 이름이 있더라도 부모·source hash·원 작업 위치로 구별하며 덮어쓰지 않는다.
- 두 기록에 존재하던 같은 anchor `c2-commit-phases`는 main의 제품4phase 기록만 [c2-commit-phases-main](#c2-commit-phases-main)으로 옮겼다. finish의 [cfg(test) phase 관측](#c2-commit-phases)은 기존 anchor를 유지한다. 원 내용과 source별 PASS·FAIL·UNKNOWN은 재분류하지 않는다.

### 실제 병합 diff에 한정한 검증

동결 index tree `0a21a0cedaf0668a2a59e29697203abb1352fb21`를6249파일로 export했다. 이번 검증 identity `08368925eaa388e13260d4f149c225d2baed747bff9c97cfce862854b632e777`는 원 archive inventory 순서의 `{path,sha256}` 목록을 compact JSON으로 직렬화한 SHA256이다. 이는 이번 archive 전체의 identity이며 `check_dev0.source_fingerprint`와 같은 알고리즘이라고 주장하지 않는다. 원 `source-r2.tar`, `source-manifest-r2.json`, test ELF `09bc9e482e42cd87280163a26e1b7804aea8dd38a44b167b7ec52f051e522ff2`, 선택 사유와 예산의 `selection-r2.json` 및 원 로그를 함께 보존한다.

| 범위 | 변경과 선택 이유 | 결과 |
| --- | --- | --- |
| production/test compile | 병합한 실제 source의 연결과 타입 검산 | production check PASS; 첫 test build FAIL(중복 module/인자 불일치) 보존; 수정 뒤 r2 test build PASS |
| source 계수10개 | 두 부모의 core/port·Commit4phase·Pending image·Aging·WAL wait·compact scope/Result·recovery/native 타입 계수를 결합한 diff | exact10 각1회 PASS |
| MT03·MF09 | 병합한 조회/Pending/C3 설치와 복구 반환 helper의 실제 memory 연결 | exact2 각1회 PASS |
| entry unwind | main의 선택 가능한 시험과 finish의 Full/Resident unwind helper를 결합 | exact1(두 class 검산)1회 PASS |
| native N03 | 같은 ELF의 UNKNOWN-after-sync와 actual file recovery 연결 | 독립 admission 뒤1회 PASS(produce/recover 두 process) |

runtime은 실행 전 선택13개·case30초·전체600초·출력8MiB·stack33554432B로 한정했으며 `runtime-r2.json` SHA256 `cc375e051cb6eda28365f8c6981266d9160b932c6ff2fa8c994e9e0510a6f1e3`의13개가 모두 PASS다. 완료한 무변경 Row codec/C3 authority 시험·하위 회귀·전체 workspace는 반복하지 않았다. stage/Commit 변경이나 통합 자체를 반복 근거로 삼지 않았다. 생략은 새 PASS가 아니다. 문서 verifier와 전체 fmt/Clippy는 push gate이므로 실행하지 않았고 push도 하지 않았다.

새 source의 memory/native 관측은 shared20144/20280B, Full34020/35044B, Full+Pending59500/60660B, recovered peak64876/65012B다. main 이전 source의 Full+Pending66468/66604B 기준 FAIL 등은 당시 증거로 그대로 보존한다. 현재 관측의 한도 내 수치는 모든 entry/tail·writer/progress를 포함한 동시 admission 구현 또는 M4/M5 완료의 증거가 아니다.

native N03의 별도 계획·원 controller·budget·source/ELF/input identity는 `native-r2/run-plan.json`과 `admission.json`에 고정한다. 전체30초/worker29초/최종 audit1초, 출력8MiB/trace4MiB/DB32MiB/snapshot128MiB/artifact1GiB/address space1GiB/free floor60GiB/file3/FD6,1회·실패 시 중단이다. N01/N02와 이전 source의 완료 사례는 반복하지 않는다. 최종 native 결과와 독립 수용은 아래에 이어 기록한다.


N03는 독립 사전 admission 수용 뒤1회 PASS했다. produce는 실제 WAL sync 성공 뒤 오류가 보고된 UNKNOWN 경계(LSN9)에서 controller가 종료했고 ACK를 만들지 않았다. recover는 동일 ELF의 새 process에서 LSN15까지 완료했으며 oracle이 WAL·home·Base·CURRENT·회수 순서와 원 입력을 검산했다. 실제12.36초는 성능 결과가 아니다. 원 `native-r2/catalog-native/N03/result.json` SHA256 `2933f37b503d36338ce40ac8f1d7b0d33739a9dfebf14dceb3cca60fb0dc2d6a`, admission `d33a3e19428a218ebcd412fbef392c96f92b3adcb5bfffabddebb1d389f44b6e`, 독립 사전 audit `bc447a2ad996940c3ae1d1192332ac49ef0e873fc0a46246d6271dfff1038c1a`와 두 phase의 원 로그·trace·DB snapshot을 보존한다. 사전 검토에서 준비 import가 만든 추가 pycache1개를 제거하고 archive inventory 순서의 identity 설명을 정정한 뒤 승인했다. 실제 제품 source·ELF·선택13 결과는 바뀌지 않았다.

시험 후 변경은 결과·색인·anchor를 정리한 문서9파일뿐이며 제품·fixture·scripts는 동결 source와 동일하다. 독립 최종 D1/D2를 이번 병합 범위에서 수용했으며 미해결 필수 수정은0이다. 검토자는 두 부모 대비 제품 보존·13개 원 로그/identity·N03 실제 trace와 snapshot의 WAL15 frame·최종 allocated home13 page·capture Base64 page 및 순서를 직접 검산했고, 현재 제품/fixture/scripts 동일·문서9개 보완·원 stash hash 유지를 확인했다. `final-review.json` SHA256 `480a75b2bdf02e199fe279a972a16a7105ed1266445a03b7dd8f5f2048d5919a`에 판정을 보존한다. 이 수용은 현재 병합의 관련 검증에 한정하며 전체 M4/M5 인수로 승격하지 않는다.


<a id="merge-review-main-20261028"></a>
### 병합 후 전체 분량 코드 리뷰 — 2026-10-28

사용자의 병합 결과 검토·즉시 수정 지시에 따라 merge `95e6bb41fa874d1d84e2823b949c06187257efc3`를 두 부모 `cbc09f2`·`76f91e0`와 각각 대조했다. OCR delegate preview/rules의 고유 대상100개(Rust99/관리 JSON1)에 기본 제외된 문서12·fixture75를 추가하여 전체187개 경로를 검토했다. 경로 이동 전 `commit_bridge.rs`의 삭제는 `plan4_apply/terminal.rs`와 함께 대조했다. `total_files=187`, `reviewed_files=187`, `skipped_files=0`, `coverage_rate=1.0`이다.

`/root/catalog_merge_audit`가 Catalog/C2/C3의75개 Rust diff 전체와 owner·호출 경계를, `/root/row_merge_audit`가 Row/공통 codec24개 전체와 계약·fixture를 독립 검토했다. `/root`는 main 고유 결합·전체 부모 blob/자동 병합 재구성·JSON key/value·문서/fixture 원본과 참조를 검토했다. finish 단독134경로는 전부 그대로이며 main 단독6경로 중 제품/fixture4개는 그대로, 색인2개만 통합 상태로 갱신됐다. fixture75개는 finish74개/main1개 원본과 일치하고 manifest의 입력/probe 참조69개를 원 bytes/hash와 대조했다. 제품 유실·중복·권한 확대·정리 수명 회귀의 필수 수정은 발견하지 않았다.

문서 참조2종을 수정했다. main 실행 기록8곳이 같은 경로의 finish manifest를 현재 main 선택으로 가리키던 부분은 `cbc09f2:path`와 원 보존 위치를 명시했다. `PreparedCatalog`의 현재 정의 링크는 실제 apply root로 정정했다. 과거 phase 관측 selector가 Pending image 단계에서 변경된 점도 기록해 원 manifest를 현재 실행 목록으로 사용하지 않도록 했다. 원 source별 PASS·FAIL·UNKNOWN·manifest bytes는 바꾸지 않았다.

제품·fixture·시험 코드 변경0이므로 빌드/제품 시험을 반복하지 않았다. 직전13개 runtime·native N03 결과는 같은 원 source/ELF의 증거로만 보존하며 이번 리뷰의 새 PASS로 표시하지 않는다. 문서 verifier·전체 fmt/Clippy·push는 실행하지 않았다. OCR 원 preview/rules·187개 coverage·부모별 diff hash·각 검토자 판단·수정 내역은 `artifacts/merge-main-full-review-20261028/review-report.json` 및 해당 폴더에 둔다. 이 정적 리뷰는 전체 동시 admission·M4/M5 인수를 완료시키지 않는다.

<a id="c2-writer-progress"></a>
## C2 Catalog Commit의 terminal read gate와 writer source 창 — 2026-10-28

사용자의 “main에서 새로 진행”, “남은 작업은 별도 branch를 생성해 적절한 단계마다 commit”, “사용한 자원을 정리해 가면서 진행” 지시에 따라 main `854bd8d`에서 만든 branch `claude/module-independence-remaining`(worktree `/home/et16/work/xadb-module-remaining`)에서 진행했다. `/home/et16/work/xadb`는 main에 두었고 병합 전 C2 stash 초안 `7df6e92`는 적용·검토하지 않았다. 결정은 [XADB-MODULE-WRITER-PROGRESS-001](../contracts/dev3-module-boundaries.md#writer-progress)이며 D2 · 작성·통합 Claude 세션 · 독립 검토 별도 읽기 전용 subagent다. 이번 단위는 C2 writer/progress 예약 가운데 **실제 Catalog Commit writer 하나**에 한정한다.

### 구현

- 원 core State에 terminal slot index(`Option<u8>`)를 두고 `Owner::close_reads_for_terminal`·private non-Clone `TerminalReadGate`를 공급했다. core는 816B로 그대로다. gate가 닫히면 새 read의 check/try는 Unchanged `InstallationPending`, gate를 가진 Owner의 Drop은 fatal로 slot을 보존한다.
- 원 isolation ledger에 `CommitProgress` class와 split에서 한 번 계산해 MutationPort가 보유하는 `CatalogCommitCharges`를 추가했다. 예약은 gate core 일치·창 순서·단독 합을 효과 전에 확인한다. 전환은 Pending·publish 축소와 Drain 뒤 무대기 install 증가뿐이다.
- `commit_catalog`는 gate→stage 창 예약→본문→source→gate checked 정리이며 예약 실패도 gate를 checked 정리한다. 정적 Capacity 검사를 제거했다.
- lower owner의 production 단독 bound: `terminal::catalog_wait_source_bytes`, `dirty_frame::commit_drain_call_source_bytes`(성공/실패 max), `pin_release_source_bytes`(retention 진입 guarded 검산 bound 재사용), `epochs::publication_source_bytes`. session census의 Commit 항목은 가장 큰 창과 carrier다.

| 값(B) | memory | native 타입 |
| --- | ---: | ---: |
| shared retained | 20184 | 20320 |
| stage / Pending / install / publish 창 | 34058 / 10106 / 23818 / 4644 | 같음 |
| 창 원값과 carrier | 32952 / 9000 / 22712 / 3538 + 1106 | 같음 |
| Pending 원값 = parent + max(wait, Drain) | 2376 + max(6624, 6124) | 같음 |
| shared+stage / shared+install | 54242 / 44002 | 54378 / 44138 |
| shared+Pending+Full / shared+publish+Full | 64310 / 58848 | 65470 / 60008 |
| shared+Pending+Resident2 | 65562 — 두 번째는 Capacity | 65698 — 같음 |
| WAL wait production / 측정 관측 | 5408 / 3272 | 같음 |
| live census peak / recovered peak | 63540 / 64916 | 63676 / 65052 |

source-value 상계이며 RSS·machine stack·성능 측정이 아니다. native Pending+Full의 여유는 66B다. 이전 source의 Full+Pending 기준 FAIL과 관측 값은 당시 증거로 보존한다.

### 시험 변경과 선택

실행 전 [동결 manifest](../../tests/fixtures/dev3-module-c2/writer-progress/manifest.json)에 실제 diff와 runtime59개·compiler3개(production check, crate Clippy lib/tests `-D warnings`, test build)의 관계를 고정했다. 총1200초·compiler420초·case30초·로그8MiB·stack32MiB이며 각1회다.

- 신규9: core gate3, ledger 전환·경계·창 fit4, actual Commit의 gate 대기·창 관측 MCW01과 예약 Capacity MCW02.
- 시나리오 이동12: Commit 전에 admitted된 read가 Drain까지 진행하던 MCRD01/02, C2_SCOPE02/03, C2_ENTRY01/02·RESIDENT2, C2_RETURN01/02·RESIDENT2. read 시작을 Pending으로 옮겨 같은 Drain 성질과 writer 몫 합산, Pending pins4를 검산한다.
- 단언 추가10: physical Busy의 stage 몫 반환, C2_DRAIN01–03의 install 몫, MF02/09/10/11/12·C2_DRAIN04 unwind의 창 몫 보존과 gate 해제.
- 무변경 시험의 변경 경로5: MT01(publication gap의 Full read), MT03(Pending의 Full LOAD), MT05–07. 실패 경로3: MF03/06/08.
- census 값 변경20: shared·split·Commit census 항목을 쓰는 owner/source/scope/recovery/free-undo/read 계산.

변경 diff와 관계없는 하위 DEV0/1/2 회귀·workspace·wrapper·Row codec/ledger·native process cut과 경로가 바뀌지 않은 commit_catalog 호출자(MT02, MF04/05/07 등)는 NOT_RUN이다. env 식별 census `catalog_source_{memory,native}`·`retention_source_{memory,native}`는 이번 source의 개발 실행에서 cap/fits 단언은 통과했으나 이 diff와 무관한 오래된 정확 값에서 실패했다(`visibility.backing==8192` 대 9792=+epoch1600, `metadata==peak+36×slot` 대 +63104=visibility8192+isolation/epoch5760+장기 credit49152). 형식 선택에서 제외하고 별도 후속으로 남기며 PASS로 표시하지 않는다.

### 사전 독립 검토와 반영

독립 subagent가 실제 diff·계약·시험을 읽기 전용으로 대조했다. 1차는 **REQUIRED3·ADVISORY10**이다. R1: 예약 실패가 gate cleanup 결과를 버림 → checked reopen과 `merge`, MCW02 추가. R2: PinSet 반환 bound가 원 `pins_valid`·registration 검산을 누락 → retention 진입 guarded bound를 재사용하고 Drain 호출을 성공/실패 max로 재구성. R3: 이 증거 anchor 부재 → 이 절로 해소. ADVISORY는 gate 검산 closure 계수, 창 순서·단독 합의 효과 전 거절, gate 보유 Owner Drop, gate core 확인, census Commit 항목, 관측 동작 목록, wait bound 단언, 문서 주석 위치, pins/unwind 단언 복원, census 시험·split 계산 graph 선택을 반영했다. 재확인에서 R1·R2 해소, 신규 REQUIRED0이며 남은 ADVISORY 가운데 Owner Drop 시험을 추가하고 계약 예시를 정정했다. Drained proof의 core 대조는 미적용 ADVISORY로 남긴다. 검토자는 파일 수정·빌드·시험을 하지 않았다.

### 실행 결과

r1은 동결 export에서 production check·Clippy·test build와 runtime59개가 모두 각1회 PASS다. 실제145초는 성능 결과가 아니다. 사용자 지시에 따라 export·build target·ELF는 결과 기록 뒤 삭제했고 아래 identity·inventory·로그·결과를 보존한다.

| identity | 값 |
| --- | --- |
| workspace | HEAD `c855f1b82bb66b89699fb30d9fe5c22ee716fd2b`, tracked diff SHA-256 `8af84f87e4296d228100a614272ce5b924c0a7ade1a2c4c88213e2123c857e81` |
| source fingerprint (5732파일) | `3e194e532e3665542e71689a6efd543a3a450cecd5287f5a1f269c208f3fbe5a` |
| inventory | `8ee718fbf342f92339d559987792b9841d74750e22e2da6ccecef016d4f45a41` |
| manifest | `fc5418db2a7e78d0d93d53b22aede8b57206f7f805f2d10783563b6f62d4061e` |
| test ELF (182009656B, 삭제) | `a85ef88bf1a5cfb4310a807b2fa0bf4ffa6ae94149cc805dd90bc5618d240b8d` |
| runner / result | `9c055b8468d32375e3c9268c82638eb91fe1995ddadda2b1bf53c9ca1753d0cf` / `c9b93acf7a22e38ebd2c16f7ccc622eeea3b0143be011f1c26c6d3955fbfd091` |

원 runner·result·inventory·compiler 로그·case stdout/stderr는 branch worktree의 `artifacts/module-c2-writer-progress-20261028/`에 둔다(1.3MB). 실행 뒤 문서만 정리했고 제품·시험·fixture·scripts는 동결 source와 같다. 문서 verifier·workspace fmt/Clippy push gate·push는 실행하지 않았다.

### 시험하지 않은 경로와 남은 범위

gate가 닫힐 때 source 예약 직전에 멈춘 read, gate 앞의 resident read, `close_reads`의 InstallationPending 분기, `wait_timeout` poison, gate 불변식의 음성 사례, finish→publish 사이의 Capacity는 이번에 시험하지 않았다. 다른 MutationPort writer·Maintenance·Row reader/writer의 ledger 참여, 진입 전·반환 뒤 floor, Row progress class, C3–C5·M4/M5 인수는 미완료다.

<a id="writer-progress-merge-main-20261028"></a>
## Catalog Commit writer 창의 독립 리뷰와 main 통합 — 2026-10-28

사용자가 branch 리뷰 결과를 받은 뒤 “제대로 만들어진거면 머지하고 브랜치 삭제”를 지시했다. 독립 리뷰·병합 담당은 구현 담당 Claude와 다른 Codex `/root` 세션이다. 리뷰 범위는 main `854bd8d3ca2bc2f4033417a0195a7cdbd1048d88`부터 `claude/module-independence-remaining`의 `af54a4e22b2e3a69e8037d2632ca2d7189d7ea9c`까지다. 변경30파일 전부(OCR 선택24·보충 문서/fixture6)를 실제 diff·소유 계약·기존 결과로 검토했으며 생략0·검토율100%·필수 수정0이다. 읽기 gate와 stage/Pending/install/publish 예약, Drain·오류·unwind 정리를 대조한 이번 공급 단위의 한정 수용이며 M4/M5 완료 판정은 아니다.

리뷰 시 원 실행 inventory의 제품·fixture·도구·Cargo/toolchain5731파일을 현재 파일과 해시 대조하여 불일치0을 확인했다. runtime59개 선택은 manifest와 정확히 일치하며 production check·Clippy·test build3개와 함께 원 result에서 PASS다. runtime stdout/stderr, runner·manifest·inventory·result의 기록된 hash도 일치한다. 이는 원 source 실행 기록의 대조이며 새 테스트 실행이 아니다.

병합 직전 두 checkout의 tracked/untracked 변경이 없고 main이 branch의 조상이며 리뷰 뒤 tip 변경이 없음을 확인했다. 충돌 없는 병합에 제품·시험 수정은 없고, 재개 위치·검토·증거 보존을 설명하는 문서만 추가한다. 무변경 코드의 제품 시험·하위 회귀는 반복하지 않는다. 문서 verifier와 push gate는 실행하지 않았으며 원격 push도 이번 작업에 포함하지 않는다.

작업 트리 정리에 앞서 원 `/home/et16/work/xadb-module-remaining/artifacts/module-c2-writer-progress-20261028/`의128파일(1,023,923B)을 `/home/et16/work/xadb/artifacts/module-c2-writer-progress-20261028/`로 이동하고 모든 파일의 전후 SHA-256 일치를 확인했다. 원 result SHA-256 `c9b93acf7a22e38ebd2c16f7ccc622eeea3b0143be011f1c26c6d3955fbfd091`과 inventory·로그의 내용은 그대로다. 위 원 실행 경로와 source identity는 당시 기록으로 유지한다. 병합 뒤 해당 branch와 별도 작업 트리를 정리하며 이후 재개는 main을 기준으로 한다. 다른 branch·작업 트리·stash와 이전 증거는 이번 정리 대상이 아니다.

다른 writer의 ledger 참여·진입/반환 floor·Row progress와 C3–C5·M4/M5 인수는 계속 미완료다. native 타입 Pending+Full source 상계65470B의 여유66B와 위 미실행 경계도 그대로 보존한다.

<a id="c2-retirement-progress"></a>
## main의 Catalog 회수 progress 예약 — 2026-10-28

사용자의 “이어서 작업을 진행…적당한 단계마다 commit” 지시에 따라 `/home/et16/work/xadb`의 clean main `c217fbf`에서 다음 유한 공급을 시작했다. 별도 C2 초안 stash·다른 branch/worktree는 사용하지 않았다. 작성/계약/통합은 `/root`, 독립 D1/D2 검토는 `/root/retirement_design_review`다. [RETIREMENT-PROGRESS-001](../contracts/dev3-module-boundaries.md#retirement-progress)의 CLR/Prune/RootFree/TerminalFinish만 원 short ledger에 연결하며 전체 M4/M5 완료를 뜻하지 않는다.

### 결정·변경과 사전 검토

독립 설계 검토는 기존 retirement가 Maintenance를 허용함을 확인하여 원 Commit용 gate를 그대로 재사용하면 역할 회귀가 생긴다고 지적했다. 별도 `close_reads_for_retirement`과 공통 private close helper를 두고 State gate 검산에 Maintenance를 포함했다. Commit의 foreground 제한은 유지한다. Catalog retirement의 내부 rollback 참조·물리 read/apply는 read_operation 진입을 다시 하지 않으므로 닫힌 gate에 스스로 대기하지 않는다.

exclusive split에서 계산한 sealed `CatalogRetirementCharge`는 caller byte 입력 없이 사용한다. 원 ledger에 예약한 뒤 refs/page/DPT/control/history·WAL durability·pair retirement·epoch publication·결과 구성까지 유지하고 source→gate 순서로 checked 정리한다. Fatal/panic에는 charge를 보존하면서 gate를 정리한다. parent receipt/Result/carrier는 모두 합산하고 capture read→retirement write→post-durable allocator read의 물리 closure만 실제 소비 반환에 근거해 max로 계산했다. 물리 retirement census 자체가 target-set 호출 비용을 포함하므로 같은 비용을 두 번 합산하지 않는다.

첫 OCR 독립 코드 검토는 Rust10파일10/10, 생략0이다. MT04 reader 반환 직후 전역 refs0 단언은 이미 진행한 retirement의 자기 rollback ref와 경쟁하므로 제거하고 양쪽 완료 후 refs0 단언을 유지했다. 그 수정 뒤 필수 코드 지적0이며 최종 증거 검토는 별도로 기록한다.

### 사전 고정 선택과 실패 보존

[manifest](../../tests/fixtures/dev3-module-c2/retirement-progress/manifest.json)에 실제 변경→runtime13 exact selector의 관계를 실행 전에 고정했다. 원 source/class·memory/native 전체 census와 Commit Pending+Full·split 비용4, 새 Maintenance gate와 변경된 공통 gate3, actual MT04·Capacity·Maintenance lifecycle·CLR panic/fresh recovery·장기 snapshot horizon2의6개다. 총1200초, compiler420초, case30초, 로그8MiB, stack32MiB이며 같은 source의 완료 사례는 반복하지 않는다. workspace/하위 회귀 wrapper와 무관한 Row/DEV 시험은 실행하지 않는다.

`artifacts/module-c2-retirement-progress-20261028/r1`은 production check PASS 뒤 시험 코드 Clippy build에서 FAIL했다. 새 시험의 `RetentionCause` import 누락과 `lookup_catalog_resident` cursor 인자 누락이다. runtime13은 모두 NOT_RUN이며 실패를 PASS로 바꾸지 않는다. cfg(test) 파일의 두 항목만 고친 r2는 Clippy/test build와 미실행 runtime을 진행한다. r1 production PASS는 원 source 결과로 보존하고 r2의 새 실행으로 세지 않는다. 각 run의 frozen inventory·runner·source/ELF/hash·로그·result를 보존하며 새 export/target/ELF는 실행 종료 후 삭제한다.

r2는 Clippy/test build·source/core7개 PASS 뒤 MT04에서 FAIL했다. 실제 `RetirementIntent`는 Apply 중이므로 새 snapshot 생성이 기존 `Concurrency(Busy)`로 거절된 것이며, 뒤 lookup의 gate 거절을 아직 시험하지 못했다. snapshot 생성만 Apply 전으로 옮긴 r3는 원 `Owner`의 `!Send`를 위반하여 Clippy build E0277로 FAIL, runtime은 NOT_RUN이다. r4는 controller thread 안에서 원 gate 대기 hook 이후·Apply 이전에 Owner/snapshot을 생성하도록 수정했다. r2의 완료7개는 그대로 보존하고 r4에는 MT04와 뒤 미실행5개만 선택한다. 이 세 수정은 모두 `catalog_memory/reference_pins.rs`의 cfg(test) 코드이며 production·source/core의 완료 검증 경로를 바꾸지 않는다.

### 실행·최종 인수

r2의 source/core7개와 r4의 실제 Catalog6개로 동결13 selector의 변경 경로 검증을 마쳤다. r4 Clippy(lib/tests, `-D warnings`)·test build와 actual6개는 모두 PASS다. production check는 r1의 원 PASS이며 r4에서 반복하지 않았다. r1→r2→r3→r4의 각5743파일 inventory 차이는 `catalog_memory/reference_pins.rs` 하나뿐이다. r1/r2/r3의 FAIL을 유지하며 source별 완료 범위와 분리한다. 실제 r4 elapsed70.943초는 성능 인수 결과가 아니다.

| source 계수 | memory | native 타입 |
| --- | ---: | ---: |
| shared retained(원 core·192B split ports 포함) | 20192 | 20328 |
| retirement 고정 charge | 42074 | 42074 |
| shared + retirement | 62266 | 62402 |
| Commit Pending + Full + shared | 64342 | 65502 |

위 값은 source-value 상계이며 stack/RSS가 아니다. native Pending+Full 여유는34B다. gate 보유 때 finite read 몫은0이고 long snapshot은 남아 horizon을 제한한다. retirement 준비 단계와 gate 전/반환 뒤 floor를 이 합계에 연결했다고 주장하지 않는다.

| 보존 identity | SHA-256 |
| --- | --- |
| manifest | `e6cbbf3ae5dce650c4826c2e454d25971ab44191eaa0853da1bb5a9dab679e6f` |
| r1 result(compile FAIL·production PASS) | `94d39d8cc40b514359fec9ba9e1196837e07fe6c39ac06e82ebd6f18fc4b9cb0` |
| r2 source / result | `ac6c464fe2c2c11c23fd28e62ba344c3d0f19c01bf5f9fc9fa8dff3858109242` / `3d821a06a1813bf4e0a01d0d847c26516ed699b1b0fe53b7301590d1bfbca161` |
| r3 result(Owner !Send compile FAIL) | `bd3e1046b0d3e9ca86faf5799f074961d0578c02e84dc8fd89b3ca4886883f12` |
| r4 source / inventory | `f78686bfcb2bc68ef5513d4c898e5262f57790a7549435ec2c28e38cc382d00b` / `cbb64a56bcece9d2324b31a145d9fe723d63573ff7cd0e9bc1c6aace3a894c16` |
| r4 runner / result | `63fd6433cf11c45329b60ed4f59d53f65eb77d025fcd1d2b0e1b2c1f076d5da6` / `62e75c662b28468bc27e5fe2fe5a65e97fbc9df20f55fa26bcbda614ba7dbcee` |
| r4 test ELF(삭제) | `3f3ff0e86485aefe8a24e70f20c21e6d0780f129f55a802e1d9a2869be979190` |

검증 뒤 문서와 source.rs의 writer entry 주석 한 줄만 정리했다. `post-run-comment.json`은 그 줄을 원문으로 복원한 SHA가 r4 inventory와 같고 나머지5742파일도 일치함을 기록한다. 실행 토큰 변경이 없어 완료 시험을 반복하지 않았다. 독립 최종 검토 `/root/retirement_design_review`는 OCR11파일과 보충 문서/manifest6파일, 총17/17·생략0·검토율100%를 확인하고 D1/D2 한정 수용·필수 수정0으로 판정했다. r2의7+r4의6 selector 중복 없음과 원 실패·runner/inventory/log/result hash·주석만의 후속 차이를 검산했다. 검토자는 파일 수정이나 시험 실행을 하지 않았다. 검토 receipt `review-final.json` SHA-256은 `12a53460a0e6e18043d64a9c45b41a6a2829a8bb96c842844afe627546c6c5d9`다. 이 단위를 main에 commit하며 전체 M4/M5로 승격하지 않는다.

준비 경로·다른 writer/Maintenance·Row·gate 전/반환 뒤 floor와 C3–C5·M4/M5 전체 인수는 미완료다. 문서 verifier와 pre-push gate·원격 push는 이번 작업에 포함하지 않는다.

<a id="c2-retirement-preparation"></a>
## main의 공유 Catalog 회수 준비 body — 2026-10-28

clean main `fa0f033`에서 계획 잔여 작업을 재개했다. 작성/계약/통합은 `/root`, 독립 D1/D2 검토는 `/root/source_admission_review`다. C2 stash·다른 worktree는 사용하지 않았다. [RETIREMENT-PREPARATION-001](../contracts/dev3-module-boundaries.md#retirement-preparation)은 공유 MutationPort 준비 body의 원 gate/source 공급이며 caller-retained Prepared와 entry/return floor·전체 M4/M5 완료는 아니다.

사전 설계 검토는 원 `health`가 registry guard를 잡는 점을 지적했다. 새 entry는 guardless health/origin만 예약 전에 확인하고 깊은 health는 기존 preparer 안에서 유지했다. 기존 단일 scalar charge를 preparation/application의 max로 계산하여 port 크기를 늘리지 않았다. 준비 결과와 cleanup carrier도 포함한다. MT04의 기존 apply 대조는 준비를 reader 정지 전에 완료하도록 옮기고 새 준비 schedule을 별도 작성했다.

[사전 manifest](../../tests/fixtures/dev3-module-c2/retirement-prepare/manifest.json)의 runtime12 exact selector를 실제 diff에 연결했다. source/공유 port/복구 census3, 변경된 MT04·apply Capacity·Maintenance lifecycle·Prune horizon2의5, 새 준비 대기·오류/역할·두 unwind cut·명시적 Fatal 정리의4이다. 총1200초/compiler420초/case30초/로그8MiB/stack32MiB를 동결했다. 각 실행의 source inventory·hash·로그·결과를 보존하고 종료 뒤 이번 export/target/ELF만 삭제한다. 기존 무변경 하위 회귀·workspace wrapper·문서 verifier·push는 실행하지 않는다. 선택 실행은 아래와 같으며 최종 독립 검토는 별도 기록한다.

### 실패 보존·선택 결과

r1 production check는 PASS이고 test Clippy build는 새 시험의 foreign-core 수명 오류(E0597)로 FAIL했다. r2의 test-only 수정은 macro의 optional foreign 인자를 빠뜨려 E0425로 FAIL했다. 두 source의 runtime12는 전부 NOT_RUN이다. r3는 원 fixture의 optional foreign core를 사용하고, 독립 검토가 지적한 MCRP05의 원 Fatal을 실제 selection→target_request 순서에 맞는 Prepare/Frame(Fenced)로 정정했다. 기대값 정정은 runtime 전에 수행했으며 Storage 예상값의 runtime FAIL을 만들어 기록하지 않는다.

r3는 engine lib/tests Clippy(`-D warnings`)·test build·runtime12 exact 사례를 각1회 PASS했다. r1→r2→r3의 각5745파일 inventory 차이는 새 `retirement_prepare.rs` 시험 하나뿐이다. production 소스는 동일하여 r1의 원 production PASS를 보존하고 반복하지 않았다. 제품 코드는 시험 결과에 맞춰 수정하지 않았다. 실제 elapsed86.403초·case 로그14411B이며 성능 인수 결과가 아니다.

| 관측 source 계수 | memory | native 타입 |
| --- | ---: | ---: |
| shared retained | 20192 | 20328 |
| 공통 preparation body | 32144 | 33168 |
| 준비/application 최대 고정 charge | 42074 | 42074 |
| shared + 고정 charge | 62266 | 62402 |
| live peak / recovered peak | 61770 / 63146 | 61906 / 63282 |

기존 charge가 새 preparation 상계도 덮어 scalar·split port192B가 그대로다. 이 값은 source-value 상계이며 RSS/stack 측정이 아니다. Commit Pending+Full 코드/형식은 변경하지 않았고 기존65502B 결과를 이번 새 실행으로 표시하지 않는다. 새 unwind2cut는 실제 새 runtime에서 원 R04 durable prefix를 MF14 독립 corpus로 복구했다. explicit Fatal2cut는 원 Prepare/Frame(Fenced) 우선과 성공 Prepared를 대체하는 Census/Fenced·charge 보존/gate 정리를 확인했다. 무관한 native process cut·하위 회귀는 재실행하지 않았다.

| 보존 identity | SHA-256 |
| --- | --- |
| manifest | `b8c73af0d3493778a547c905981f64485d4e38b94dec6a9a7fc6a5d6e2fe8924` |
| r1 result (production PASS·compile FAIL) | `8b06b7ea30a425bd35c2dd271e389cd182b5bff68ecd7f22afc097038a6725df` |
| r2 result (compile FAIL) | `9d1556ff933704d04a802e034bf52d11119a8bbe84b74678ea2d2a38d692edfe` |
| r3 result | `393496fd02cd8f86da1bbd327babd78d5ea140b0c2efbe4964d03c3b424914ca` |
| r3 source | `4e872e91f103734e4b728b081733d2bd08aa7914b3ab0f63a1cfe9824e337d08` |
| r3 inventory | `48d9aca23bc30026e4a1ca41cb1b2313196417390959f364742000727abd6bd3` |
| r3 runner | `9b3e7fc9242e1c054550b8e8938773b93bbe6356c57c6ea0d1a725359f77dc58` |
| r3 test ELF (삭제) | `924b9f373f1398637b4f7df162fac6954e61a8fb5fc244a0293da86dfe66ba45` |

원 `artifacts/module-c2-retirement-prepare-20261028/r1`–`r3`의 inventory·runner·compiler/case 로그·결과를 보존했고 각 실행의 export/target/ELF는 종료 후 삭제했다. gate 전 entry/반환 tail·caller-retained Prepared·나머지 writer/Row와 C3–C5·M4/M5 인수는 계속 미완료다. 독립 `/root/source_admission_review`가 변경18/18파일(Rust11·manifest1·문서/JSON6), 생략0·검토율100%를 대조하고 **D1/D2 한정 수용·필수 수정0**으로 판정했다. 원 source5745파일/current fingerprint·각 run의 inventory/runner/helper/log/result hash·현재 코드 일치와 삭제 상태를 검산했고 `/root`가 반영했다. 검토자는 수정·build·시험을 실행하지 않았다. `review-final.json` SHA-256은 `c79d36cf85e6f2a8326613c89838ad3141ca7bd13ccee032f25c1dd68c813c65`다. 변경 파일 rustfmt와 diff whitespace를 확인했으며 문서 verifier·전체 fmt/Clippy·push는 실행하지 않았다.

<a id="c2-retirement-call"></a>
## main의 Catalog 회수 동기 호출 — 2026-10-28

main `582ccbf`에서 앞선 준비 body의 caller-retained gap을 조사하고 [RETIREMENT-CALL-001](../contracts/dev3-module-boundaries.md#retirement-call)을 선택했다. 작성/계약/통합 `/root`, 독립 `/root/source_admission_review`다. C2 stash·다른 worktree는 사용하지 않았다. 회수만 동기화하며 Commit Prepared·일반 entry/return floor와 전체 M4/M5는 미완료다.

사전 독립 D2는 외부 prepare/discard caller가 시험에만 있고 실제 제품 recovery는 즉시 준비·적용한다는 근거로 동기 호출을 수용했다. 공유/독점 prepare 제거, apply private, 원 gate/source 한 번의 연속 수명과 Maintenance·horizon 보존을 요구했다. [manifest](../../tests/fixtures/dev3-module-c2/retirement-call/manifest.json)는 변경 diff에 compiler4·runtime13·native N02 한 번을 연결했다. 총1800초/compiler420초/runtime case30초/native30초/로그8MiB/stack32MiB, native 사전 독립 identity 검토600초를 동결했다. 기존 native controller의 입력·oracle·3PID/자원/30초 한도는 유지한다. 위 값은 실행 전 계획이며 아래 결과와 구분한다. source inventory·hash·로그·실패/미실행을 보존한 뒤 이번 export/target/ELF만 삭제한다. 무변경 wrapper·문서 verifier·push는 실행하지 않는다.

### 선택 검증·source 결과

r1 production check·engine lib/tests Clippy(`-D warnings`)·test build와 compiler4/runtime13을 각각 한 번 PASS했다. 컴파일 검사는 정상 동기 호출1, 제거된 공유/독점 prepare의 E0599 각1, private apply의 E0624 1이며 exact primary span을 검산했다. runtime은 준비 전·Prepared 보유·적용 완료에서 같은 charge/gate를 관측했고, 새 read의 InstallationPending 및 기존 reader 반환 뒤 진행을 확인했다. 준비 두 unwind cut·원 MF14 첫 페이지 unwind는 새 owner의 독립 corpus 복구로 검산했다. Fatal3cut는 준비 Frame(Fenced), apply 입구 Storage Fatal 우선, 성공 적용 결과를 대체하는 Census/Fenced를 구별하고 원 R05까지10frame WAL을 비교했다.

사전 독립 검토는 새 entry/capture packet의 계수가 불명확하다고 지적했다. 실행 전에 inputs4·즉시 closure capture6·exclusive caller3의 참조와 두 caller의 scalar를 따로 보수 계수했다. port 크기는192B 그대로이며 다른 Commit 창은 변경하지 않았다.

| source 계수 | memory | native 타입 |
| --- | ---: | ---: |
| shared retained | 20192 | 20328 |
| 공통 preparation body | 32144 | 33168 |
| 고정 retirement charge | 42198 | 42198 |
| shared + charge | 62390 | 62526 |
| live peak | 61894 | 62030 |
| recovered peak | 63270 | 63406 |

값은 source-value 상계이며 RSS·stack 실측이 아니다. 준비/application은 직렬이며 둘을 합한 예약이 아니다. 기존 Commit Pending+Full65502B는 무변경 원 결과로 보존하고 새 실행으로 승격하지 않는다. 일반 entry-before-reservation/return-tail floor와 Commit Prepared lifetime·나머지 writer/Row·전체 M4/M5는 미완료다.

| r1 보존 identity | SHA-256 |
| --- | --- |
| manifest | `e7063515b6a32bb1196ff6b013761b5cc25dc4a4fc343a8f9f0b747a080515e9` |
| source5750files | `6cd4e3b8df475887795947ecb6990803e4ea2e3f6d05009a0385db64546fa4c2` |
| inventory | `1c6a1ad0942a7aa71052a8501b7b41a8f5894303e82e67c8a980614c8a14ac7e` |
| runner | `ca7fce9421abf0910f3677fdc9a91ca3afaf35e270811a77d7be9c86b83b0311` |
| test ELF | `99158d45058419965b7a593d97835c384c23717507f5c923c45cfe9e9406804b` |
| pre-native result | `39e80fb4d9ab6e02d31cbca8350fba14c422c670bc78f4e8b799cec4f5bb7539` |

같은 source/ELF의 native N02를 독립 admission 뒤 한 번 PASS했다. `/root/source_admission_review`가 compiler/runtime 로그·product5750파일 및 full export6269파일·실제 ELF를 대조하고 원 controller30초/3PID/자원 한도를 유지한 admission만 작성했다. producer PID90856는 LSN8 Catalog home 전, recovery PID90859는 LSN10 CLR durable 뒤 실제 SIGKILL됐고 reentry PID90862는 capture10/epoch3에서 LSN16으로 정상 종료했다. 원 독립 corpus가 WAL·base·home·정리·pin/FD를 검산했다. 실제 native 16.006초, 전체 실행(독립 검토 대기 포함) 220.521초, 선택 runtime 로그15460B다. 성능 인수 결과는 아니다.

| 추가 보존 identity | SHA-256 |
| --- | --- |
| full export inventory | `df4494e0f1490031f55a13e5ca4db9a560bcd161446206019b19e9560a7b9574` |
| 독립 native admission | `0d3f92bc1b35e498167ced98d9fa305eac712777986398dac5611b1314dd7cf4` |
| native N02 result | `abd24e78ff91840d8c62741ef81430ca9dcfe7ba31fcdf799f4a4aee2873e25e` |
| 전체 r1 result | `b4ad6e63436b4b6117abb7c84633d02c5a7171866c11752f1c4f11d7e509de05` |

`artifacts/module-c2-retirement-call-20261028/r1`에 runner·source manifest·compiler/runtime/native 원 로그·database snapshot·결과를 보존했고 이번 export/target/ELF를 삭제했다. 코드 변경 없이 완료 사례를 반복하지 않았고 이전 실패·원 source 결과는 수정하지 않았다. 최종 문서·전체 diff의 독립 검토는 아래에 별도 기록한다.

독립 `/root/source_admission_review`가 변경17/17파일(Rust6·fixture/manifest5·문서/JSON6), 생략0·검토율100%를 확인하여 **D1/D2 한정 수용·필수 수정0**으로 판정했다. 현재 source5750파일/fingerprint·compiler/runtime/helper hash·admission evidence44개·N02의3PID/cut/reaped/base hash·원 예산·삭제 상태를 대조했다. 검토자는 build·시험·native 재실행·제품/시험 수정 없이 receipt만 기록했고 `/root`가 반영했다. `review-final.json` SHA-256은 `c6fefbbcaada835e8484d4ed7ccf8af1fa366806bf82031b3d70c30515cb140f`다. 후속 변경은 이 receipt hash와 채택 상태 기록뿐이며 제품/fixture/manifest는 r1과 같다. 변경 파일 rustfmt와 diff whitespace를 확인했고 문서 verifier·전체 workspace fmt/Clippy·push는 실행하지 않았다.

<a id="c2-lifecycle-progress"></a>
## main의 Catalog rollback·terminal progress 공급 — 2026-10-28

main `bf62459`에서 [LIFECYCLE-PROGRESS-001](../contracts/dev3-module-boundaries.md#lifecycle-progress)을 진행한다. 작성/계약/통합 `/root`, 독립 `/root/source_admission_review`. 원 stash·다른 worktree는 사용하지 않는다. 네 lifecycle와 borrowed ordinary apply만 해당하며 전체 floor/나머지 writer/Row·M4/M5는 미완료다.

사전 검토는 read gate가 별도 flush를 배제하지 않는 점과 Prepared 아래 split의 추가 source 비용을 지적했다. 같은 원 MaintenancePort mutable borrow·pool/storage 검산, 기존 Ready 무제한 및 Cancel quiescence 조건, 원5참조 private application view로 반영했다. 새 입력/기대값은 기존 MC01/02/04·N02 oracle에 연결하고 독립 검토한다.

실행 전 [manifest](../../tests/fixtures/dev3-module-c2/lifecycle-progress/manifest.json)로 compiler4·runtime12·native N02 같은 ELF3PID 한 번을 동결했다. source3·기존 capacity1·기존 lifecycle3·신규 gate/오류/unwind/Fatal/flush5이며 변경 diff와 관계를 각 행에 기록했다. 총1800초/compiler420초/runtime case45초/native30초/로그8MiB/stack32MiB/사전 독립 identity admission600초. native Cancel coverage를 주장하지 않는다. 무변경 회수13·전체 wrapper·문서 verifier·push는 실행하지 않는다. source identity·파일별 hash·로그·결과·native snapshot을 보존한 뒤 이번 export/target/ELF를 삭제한다. 위는 실행 전 NOT_RUN 계획이며 실제 source별 결과는 아래에 추가한다.

실행 전 검토에서 public→private dispatch의 인자 packet 누락을 추가 계수했고, N02 corpus가 단일 복구가 아닌 epoch2(8→10)/epoch3(10→16) 두 복구임을 확인했다. MCLP03은 원 native와 같은 durable CLR·idle 경계에서 실제 memory snapshot을 얻고 새 runtime으로 재진입하도록 고쳤다. 원 corpus의 WAL epoch·raw bytes·최종 checkpoint는 변경하지 않았다. 이 수정 전 제품/시험 실행은0회다.

r1은 production check PASS 뒤 test module의 명시적 path 누락(E0583)으로 Clippy/test compilation에서 중단됐다. runtime/compiler fixture/native는 NOT_RUN이다. 원 FAIL과 inventory/log를 `artifacts/module-c2-lifecycle-progress-20261028/r1`에 보존하고 export/target을 삭제했다. r2는 cfg(test) module path만 고쳐 미완료 Clippy/build·compiler4/runtime12/native1을 진행하며 완료한 무변경 production check를 반복하지 않는다.

r2는 새 flush-claim 시험이 private `Plan4Target.request`에 접근한 E0616으로 test compilation FAIL이었다. 기존 공개 관찰 identity/kind/level로 expected TargetRequest를 구성하도록 시험만 수정했다. r3는 같은 미완료 범위를 진행하고 r1 production PASS를 원 source로만 참조한다. r2 로그/source inventory와 NOT_RUN을 보존하고 export/target을 삭제했다.

사전 D1/D2는 위 packet·N02 연결 보완 후 필수 수정0으로 수용했다. r3의 최종 구현과 선택 검증은 다음과 같다.

- 네 shared lifecycle 입구와 exclusive caller 모두 원 gate/source를 준비부터 checked cleanup까지 유지한다. 원5참조 application view에는 새 owner/ledger가 없고 Prepared 아래 split도 없다. same mutable flusher와 pool/storage binding, 원 Cancel quiescence 및 live Cancel epoch 순서를 보존했다.
- r1 production check PASS는 원 source 결과다. r2/r3의 수정은 cfg(test) 파일에만 있으며 r3에서 engine Clippy(lib/tests), test build, compiler4를 PASS했다. private application/preparer 접근과 flusher borrow 중복은 지정 오류/primary line으로 거절됐다.
- r3 runtime12는 각1회 PASS: source/census3, 변경 fixed capacity1, MC01/02/04 원 lifecycle3, 신규 MCLP01–05의 reader wait·연속 gate/charge·네 capacity-before-effect·역할/오류·두 unwind/새 epoch 복구·세 Fatal 경계·원 flush claim 배제5다. MCLP05는 Cancel Busy 후 selection 재진입이고 성공 recovery Cancel은 MC04다.
- 같은 ELF native N02 한 번 PASS: producer PID106214 LSN8에서 외부 SIGKILL, recover PID106216 LSN10 durable CLR에서 외부 SIGKILL, reentry PID106218 LSN16/READY/최종 checkpoint 뒤 exit0. 각 원 cut와 원 oracle·resource check를 유지했고 약16.095초였다. native CancelReserved 결과로 일반화하지 않는다.

| 실제 source-value 항목 | memory | native 타입 |
| --- | ---: | ---: |
| shared retained | 20192B | 20328B |
| lifecycle body+carrier | 41590B | 42614B |
| fixed max(retirement,lifecycle) | 42198B | 42614B |
| shared+fixed progress | 62390B | 62942B |
| live peak | 61894B | 62446B |
| recovered live peak | 63270B | 63822B |

split port는192B이며 physical stack/RSS 측정이 아닌 source 상계다. 일반 entry/return floor·Commit Prepared lifetime·Begin/AgingStart/CREATE admission·Row/C3–C5·M4/M5 인수는 미완료다. 기존 Commit Pending+Full65502B는 수정 없는 과거 증거로 유지하며 재실행하지 않았다.

| run | source SHA-256 | 결과 |
| --- | --- | --- |
| r1 | `cf5584aded85c1c52adf6023f724b8c016db6e4462aba0aabfd07330d1a1519a` | production PASS·test module E0583 FAIL·나머지 NOT_RUN |
| r2 | `f3f240099bdc8e7c6f809810d8999565ded7634cd92b5e913fc9a4bf53b413aa` | test private-field E0616 FAIL·나머지 NOT_RUN |
| r3 | `9f66e92a233cbe978488ca172c608d79b11ccda14ac2fec343452974ee802220` | Clippy/build·compiler4/runtime12/nativeN02 PASS |

r3 source inventory5757파일 SHA-256 `0c1f5dd646bae526cd6345b569f3cd3fb937f2a7f586209d8fcafed108f91323`, full export6276파일 manifest `f927741478d1dbd9ba999bc5c7bd81bff9175c5a89fd5b052546cd9de5b1e947`, ELF `e9301a3fa6dc30c43c20e917d17f31824f7870255f254a9ecea79833d6e571f3`. r3 결과 SHA-256 `24ba12d1a4c31a0cc6b64113532dd04b4a32f9f591a5dc6f014d52ead06f1085`이며 총241.188초였다. 독립 admission은 source/ELF·raw logs·40개 evidence hash를 대조한 뒤 작성했고 SHA-256 `89a400378ba293b687c3aa14108b3225293107626cb568f20318b1b4c15a8f53`다.

`artifacts/module-c2-lifecycle-progress-20261028/r1`–`r3`에 원 실패/미실행·로그·결과·source hash inventory와 native database snapshot을 보존했다. 각 run의 export/target/ELF는 결과 기록 뒤 삭제했고 기존 증거는 삭제하지 않았다. 최종 문서·전체 diff 독립 수용은 아래에 별도 수신한다.

독립 `/root/source_admission_review`는 변경22/22파일(Rust11·fixture/manifest5·문서/JSON6), 생략0·검토율100%로 **D1/D2 한정 수용·필수 수정0**을 확인했다. source5757·원40개 admission evidence·compiler/runtime/native3PID·cut/base/reaped·예산과 삭제 상태를 대조했다. build/test/verifier/native 재실행 없이 receipt만 작성했으며 `/root`가 반영했다. `review-final.json` SHA-256은 `b504977c49a214ad3218b8fdaba3c4e1edfea5ea6d25576995c759f5eeb9f70c`다. 후속 수정은 이 receipt/채택 상태 기록이며 제품/시험/fixture는 r3와 같다. 변경 파일 rustfmt·diff whitespace를 확인하고 문서 verifier·전체 workspace fmt/Clippy·push는 실행하지 않았다.

<a id="c2-start-progress"></a>
## main의 Begin·AgingStart progress 공급 — 2026-10-28

main `af78960`에서 [START-PROGRESS-001](../contracts/dev3-module-boundaries.md#start-progress)을 진행한다. 작성/계약/통합 `/root`, 독립 `/root/source_admission_review`. 원 Begin health/선택/발급 gap과 Aging 의미를 같은 gate/source에 연결한다. 새 owner·별도 ledger·stash 적용은 없다. 일반 floor·CREATE/Row·M4/M5는 미완료다.

사전 D2는 선택적 writer의 유한 직렬화와 필수 rollback progress 보장을 구분하고, caller의 유한 read scope 선반환·원 장기 snapshot 보존을 요구했다. Root/Job 선택·Status 전체 decode·deep health의 비용을 따로 계수하여 공통 body와 max로 연결했다. 기존 CREATE first-job caller의 새 view 비용도 포함했다. 신규 fixture는 기본 Aging 경로를 유지하면서 요청한 MCSP01에서만 실제 Aging/Begin의 연속 gate/source를 관측한다.

실행 전 [manifest](../../tests/fixtures/dev3-module-c2/start-progress/manifest.json)에 compiler3/runtime5/native N02 oneattempt를 diff별 동결했다. source3·shared start의 성공/두 capacity/역할/Membership/occupied WrongState와 gate·charge 연속1·원 MC03 issued gap/fresh recovery1이다. 총1800초/compiler420초/case45초/native30초/admission600초/로그8MiB/stack32MiB. 기존 controller/corpus/품질은 유지하고 무변경 완료 사례·전체 wrapper·문서 verifier·push는 실행하지 않는다. 각 실행 source identity/file hash·로그·결과·native snapshot을 보존한 뒤 이번 export/target/ELF만 삭제한다. 위는 실행 전 NOT_RUN 계획이며 실제 결과는 아래와 구분한다.

실행 전 D1/D2는 명시적 health/root/status phase와 새 shared 관측·원 MC03/corpus의 선택을 수용했고 필수 수정0이었다. r1 production check PASS 뒤 cfg(test) helper의 Live type alias 수명 인자 수 오류(E0107)로 Clippy/test compilation FAIL이었다. runtime/probe/native는 NOT_RUN이며 원 로그/source inventory를 보존하고 export/target을 삭제했다. r2는 test-only 수명 인자 하나만 제거해 미완료 Clippy/build·compiler3/runtime5/native1을 진행한다. 무변경 production check는 반복하지 않고 원 r1 증거로만 참조한다.

r2에서 engine Clippy(lib/tests)·test build·compiler3·runtime5가 각1회 PASS했다. source3의 Debug 출력에 start_health4631B/start_root8376B/start_status10425B를 보존했다. MCSP01은 실제 Aging 및 shared Maintenance Begin의 Preparing/Prepared/Completed gate·charge 연속, 결과 반환 뒤 source0/gate 재개, 두 action의 capacity-before-effect와 invalid role/Membership/occupied WrongState의 issued/private/WAL 불변을 검산했다. 원 initial/R01 raw oracle는 유지했다. MC03은 실제 Root counter 소진 뒤 `(issued,adopted)=(1,0)` gap과 다음 `(2,2)` Begin, 원 MC03 fresh recovery/후속 Begin을 검산했다. caller read-scope 선반환 조건과 선택적 writer 정책을 유지한다.

같은 ELF native N02 한 번은 약16.123초 PASS였다. producer PID111929 LSN8의 외부 SIGKILL→recover PID111938 LSN10 durable CLR의 외부 SIGKILL→reentry PID111940 LSN16/READY/checkpoint/exit0을 원 controller·corpus·자원 한도에서 확인했다. 이 실행은 변경 Begin/Aging producer의 실제 저장·복구 연결이며 전체 native inventory를 대신하지 않는다.

| source-value 항목 | memory | native 타입 |
| --- | ---: | ---: |
| shared retained | 20192B | 20328B |
| lifecycle body+carrier | 41614B | 42638B |
| fixed max(retirement,lifecycle) | 42198B | 42638B |
| shared+fixed | 62390B | 62966B |
| live peak | 61894B | 62470B |
| recovered live peak | 63270B | 63846B |

port192B·split 계산24824B를 유지한다. source 상계이며 physical stack/RSS 측정이 아니다. Begin/Aging 선택 phase는 공통 prepare보다 작고, native fixed charge의24B 증가는 status argument 계수다. CREATE admission·Commit Prepared lifetime·일반 entry/return floor·Row/C3–C5·M4/M5는 미완료다. 기존 Pending+Full65502B는 원 source의 증거로 보존하며 재실행하지 않았다.

| run | source SHA-256 | 결과 |
| --- | --- | --- |
| r1 | `a2107416882afd388f8fd5584696495eaefc8ce43763020744a64d9f1b6af4c4` | production PASS·test helper E0107 FAIL·나머지 NOT_RUN |
| r2 | `9de46f9f5944f0c3ff8546843094348e7efd0663674a06e98d02d43e818dff2f` | Clippy/build·compiler3/runtime5/native N02 PASS |

r2 source5763파일 inventory SHA-256 `55742f7b9857424f4aeb4307f19579f5f2fa9dd635eff14db6267ce2c85531b6`, full export6282파일 manifest `13b7740e9ea6e32b91f3e0bf7430c4e0c7deaf35e05af20582b6d0315230531c`, ELF `a9312268ba57e1b48ff022f2872f6331865d26f6e6de2bd96346045eb35c5860`. 결과 SHA-256 `b535b445b6b189a5e376f8e2fbc27d21757cce37bbb6128e121cceb6e4e5d8a8`이며 총202.367초였다. 독립 admission은 실제 source/ELF·24개 evidence hash·새 phase 숫자를 확인한 뒤 작성했고 SHA-256 `6dc22948d71564c69b8d25fa207e882cc1b69007a3ffcfc703dd5670ee44e2f6`다.

r1/r2 원 로그·실패/미실행·source/file hash·결과·native database snapshot을 `artifacts/module-c2-start-progress-20261028`에 보존했고 각 run의 export/target/ELF를 삭제했다. 원 production PASS를 새 source PASS로 옮기거나 완료 시험을 반복하지 않았다. 최종 문서·전체 diff 독립 검토는 아래에 수신한다.

독립 `/root/source_admission_review`는 변경16/16파일(Rust6·fixture/manifest4·문서/JSON6), 생략0·검토율100%로 **D1/D2 한정 수용·필수 수정0**을 확인했다. source5763·57개 evidence hash·원 admission24개·compiler/runtime/native3PID·cut/base/reaped와 삭제 상태를 대조했다. 검토자는 build/test/native/verifier를 재실행하지 않았다. `review-final.json` SHA-256은 `ec52ca0b09e04df4fc6446338d081a6613aec843f30aa9a5bcfe684b490bbdc8`다. 후속 변경은 이 receipt/채택 metadata이며 제품/시험/fixture는 r2와 같다. 변경 파일 rustfmt와 diff whitespace를 확인했고 문서 verifier·workspace 검사·push는 실행하지 않았다.

<a id="c2-commit-call"></a>
## main의 Commit 준비·적용 단일 예약 — 2026-10-28

main `9c334fe`에서 [COMMIT-CALL-001](../contracts/dev3-module-boundaries.md#commit-call)을 진행한다. 작성/계약/통합 `/root`, 독립 `/root/source_admission_review`. CREATE는 shared atomic pair claim 선행이 필요하므로 별도 남기고 Commit의 외부 Prepared gap을 먼저 닫는다. 새 제품 API와 native producer를 연결하고 과거 분리 fault adapter는 cfg(test) 한정으로 보존한다.

사전 D2에서 actor/private의 전체 수명 비용을 지적받았다. 원 TxnRef를16B로 가정한 초기 예상은 폐기하고 actual sizeof로 네 창에 모두 계수한다. 기존34B 여유에 의존하지 않는다. 원 prefix 검산을 실제 block에서 종료하여 Pending/publish의 해당 값만 제거하고 stage/preflight 계수와 원 오류 순서는 유지한다. 독립 검토는 이 좁은 대안을 한정 수용했다. 원 preparation의 동시 실제 flush와 새 API Full+Pending을 검증한다.

실행 전 [manifest](../../tests/fixtures/dev3-module-c2/commit-call/manifest.json)의 runtime11/compiler4/nativeN03 1회를 diff별 동결했다. source/상계3, 새 entry3(성공/Capacity/역할/단계/flush·두 unwind의 원 N02 memory 복구·세 Fatal), 새 API의 원 Full overlap1·physical Busy1·stage unwind1·write-before/sync-after error2다. 전체1800초/compiler420초/case45초/native30초/admission600초/로그8MiB/stack32MiB. 기존 완료 사례·전체 wrapper·문서 verifier·push는 실행하지 않는다. build/export/ELF는 결과·hash 보존 뒤 삭제하며 원 evidence는 보존한다. 위는 실행 전 NOT_RUN 계획이며 실제 결과는 후속 기록한다.

사전 구현 검토에서 private writer preparation forwarder의 by-value actor 하나가 빠진 것을 지적받아 준비 stage에 `size_of<TxnRef>`를 추가했다. public actor는 원래 네 창에 유지하며 반환한 helper actor는 stage에서만 센다. 실행 전 보완이며 실행 결과는 아직 없다.

r1 production/Clippy/build·compiler4·source3는 PASS했다. MCC01의 Prepared 뒤 flush가 바로 성공한다는 새 시험 예상이 틀려 실제 Prepare/Frame(Stale)/Unchanged 뒤 Pending 대기 timeout으로 FAIL했다. 원 `stage_catalog`의 Intent/WAL 전 full-stamp 검산은 recLSN도 비교하므로 제품 동작이 맞다. 원 실패/로그/identity를 보존하고 export/target/ELF를 삭제했다. 독립 검토는 제품 검산을 바꾸지 않고 exactStale·flush 후 WAL/control/epochs/Q 불변·source0/gateopen/pins0/healthy와 고정 fresh prepare1회 C04를 검산하는 D2 정정을 수용했다. private scratch는 이미 작성했으므로 호출전 불변을 주장하지 않는다.

r2는 cfg(test) 관측/시험과 manifest만 보완한다. 완료한 production/compiler4/source3는 원r1 source 증거로만 보존하며 반복하지 않는다. 미완료 runtime8 및 nativeN03을 선택한다. native admission은 r1 source3의 원 숫자/로그와 독립 product/type/census 동일성 검토를 actual r2 ELF에 연결하는 파생 admission이며 r2에서 같은 ELF source3 시험을 새 실행한 것으로 쓰지 않는다.

r2 Clippy/build와 MCC01의 exactStale→고정 fresh prepare1회/C04, MCC02의 두 준비 unwind/원N02 memory 복구가 PASS했다. MCC03 첫 Preparing 오류 종류를 lifecycle의 Frame(Fenced)로 예상한 시험은 FAIL했다. Commit의 원 deep `health`는 storage를 먼저 확인하므로 Storage(Fenced)가 정답이며 제품 변경은 필요하지 않다. r3는 이 expectation을 실제 원 오류로 교정하고 Prepared의 원 Wal(Observe,Fenced)도 exact 검산한다. 원r2 FAIL/로그와 두 PASS의 source를 보존하고 export/target/ELF를 삭제했다. 완료 MCC01/02를 반복하지 않고 잔여 runtime6과 nativeN03만 실행한다.

r3 MCC03 exact Storage/Wal/Census Fenced와 새 API MCW01의 Full 동시 조회/창 전환은 PASS했다. Busy 시험은 정확한 Stage/Wal(Busy)/Unchanged와 원 자원/control/파일 이미지 검산 뒤, resident per-frame 후속 비교 전에 `events.is_empty`에서 FAIL했다. 원 시험은 분리 준비 뒤 events를 지웠으나 새 동기 entry는 준비의 read/metadata I/O까지 포함했기 때문이다. 독립 검토에 따라 r4는 StageCallStarted에서 준비 이벤트의 mutation0을 검산·로그로 보존한 뒤 같은 원 stage 구간의 I/O0를 확인한다. 원 WAL/페이지/control/epoch/source 검산은 유지한다. r3 두 PASS·원FAIL/identity를 보존했고 heavy artifact를 삭제했다. r4는 미완료 Busy/MF02/MF03/MF08 네 사례와 nativeN03만 선택한다. r2의 첫 오류 종류는 로그에 미출력이므로 정적 health 순서에 근거한 원인 분석이었고, r3 exact assertion으로 확인했다.

검증 해석 보완: frozen manifest의 MF02 이유에 포함된 current-build recovery는 MF02 자체의 실행 결과가 아니다. 원 StageCallStarted unwind 분기는 복구를 실행하지 않는다. 새 준비 unwind 복구는 MCC02의 두 실제8→10→16 memory 경로, after-sync UNKNOWN native 복구는 N03에 배정했다. 예전 `dev3-module-c3/prepared/commit_twice.rs`의 E0382는 원 source 증거로 남기고, 현재 production의 해당 외부 입구 자체 부재는 새 `old_writer_absent` E0599가 검산한다. 과거 fixture의 E0382를 현재 결과로 주장하지 않는다.

### source별 최종 실행과 한정 결과

r4는 Clippy/test build와 잔여 Busy/MF02/MF03/MF08 네 사례를 통과했다. 준비 중 read/metadata 이벤트는 mutation0이고 실제 Busy stage의 I/O0도 유지됐다. 독립 actual source/ELF admission 뒤 원 N03을 한 번 실행하여 PASS했다. 원 producer PID119388은 LSN9 `A_CREATE_UNKNOWN_AFTER_SYNC`에서 외부 kill(-9), recover PID119390은 LSN15에서 exit0이었다. 12.435492초, UNKNOWN 정확히1·ACK 파일 없음·원 cut/reaped/PID 부재를 확인했다. 이는 두 PID의 UNKNOWN 복구이며 N02의 세 PID 재진입 시험으로 표현하지 않는다.

| 실행 | 그 source에서 실제 PASS | 보존한 전체 실행 상태·범위 |
| --- | --- | --- |
| r1 | production/Clippy/build·compiler4·source3 | MCC01 잘못된 flush 성공 기대 FAIL;64.073초 |
| r2 | Clippy/build·MCC01/02 | MCC03 오류 종류 기대 FAIL;63.639초 |
| r3 | Clippy/build·MCC03/MCW01 | Busy 관측 구간 기대 FAIL;45.617초 |
| r4 | Clippy/build·Busy/MF02/MF03/MF08·nativeN03 | 선택 범위 PASS;278.657초 |

runtime11·compiler4의 선택 범위는 위 네 source에 나뉘어 완료했다. r4 ELF에서11개를 새로 전수 실행한 결과가 아니다. r1 이후 제품 코드/자료형/source 식/원 source3 본문은 동일하고 delta는 `commit_call.rs`, cfg(test) 관측 `recovery_observation_tests.rs`, `terminal.rs`, manifest 네 파일뿐이다. 원 r1 source3 로그를 byte 그대로 복사하고 현재 type/census 동일성을 독립 검토한 파생 admission을 적용했다. `r4/catalog-source-admission/result.json`의 source3 현재 실행 수는0이다. r3 runner의 원 "r2 ELF" 문구도 보존하며 이 절의 source 구분을 따른다. 완료한 사례 반복0, 원3개 FAIL·중단 후 NOT_RUN과 source identity는 소급 변경하지 않았다.

r1 source3의 원 실행 값은 다음과 같다. 이는 source 상계이며 실제 stack/RSS 측정이 아니다. r4에는 위 동일성 검토로 연결된다. actor/private 입력56B를 네 창 모두 계수하고, private 준비 forwarder의 actor는 stage에 추가했다. 실제 block으로 종료한 prefix80B 때문에 Pending+Full은 기존보다24B 감소했다. 원 출력의 `carriers=stage-raw_stage`는 새 준비 max 증가분도 포함하므로 순수 carrier 크기로 해석하지 않는다.

| source 항목(B) | memory | native |
| --- | ---: | ---: |
| shared | 20192 | 20328 |
| stage / shared+stage | 38098 / 58290 | 39122 / 59450 |
| pending / install / publish | 10106 / 23898 / 4644 | 10106 / 23898 / 4644 |
| Full / shared+pending+Full | 34020 / 64318 | 35044 / 65478 |
| shared+publish+Full | 58856 | 60016 |
| live / recovered peak | 61894 / 63270 | 62470 / 63846 |
| split ports / split total | 192 / 24824 | 192 / 24824 |

한도65536B는 유지한다. 두 resident와 Pending을 겹친65570/65706B는 여전히 admission 거절 대상이며 새로 허용하지 않았다. Prepared2024B/image568B, 기존 lifecycle fixed42198/42638B도 유지한다.

artifact는 `artifacts/module-c2-commit-call-20261028/r1`–`r4`다. 각 실행의 source5769 파일별 inventory, runner, compiler/runtime 로그와 결과를 보존한다. full export6288 manifest와 native 로그/결과는 해당 단계까지 완료한 r4에만 존재한다. 각 실행의 export/target/ELF는 결과 기록 뒤 삭제했고 현재 부재를 확인했다. 과거 evidence·native snapshot은 보존했다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 | `f7ab614afd1b6fecf53ec5c53313a873f46fcb44c4c2f044925d9d940b0b6f7e` | `379f1e3ae7c25937db60cb58891d1729b4b6e278a40ccb8d72f9b89b8ba3d001` | `acb86fd021b49ff91f2dfa700102206942077e07a7627ebfbc2b0cabe957199a` |
| r2 | `8a6fef0d6a337c9b1959e9d08a82707fd848617d0b3231d66da7b2b6b1e1825e` | `f541ae1ea2f461f95a799e20db023ba183d6ea95ced2672ef9f02cf07a5c04c9` | `3cfa88e95a2f8a43d3a107575180cb86727e6f99e2df051744ad8be7e777952d` |
| r3 | `5c7c722d5de126649e0e139fa74714c8955ab85db8f9751094d853d4ac820b6a` | `fb8145d5315d4d5805db5cd4a21793aba4d525d2d7f05f19f82a2edfd9c1794e` | `ab5b7aecc64aa49ab134f839024c87c59815738cf8d33cdd62c7bb31654b8af4` |
| r4 | `849cce8cee76a3a8b2ecb83608213d76f99e675b2b09dd80d1d4d653834f4f01` | `6c2bf3fdf941bf7a96e73b126d250254371c87e98125abc113222db81afacdc0` | `1eaaa60c5d2f1a417e3d614f1a6a8f88312b85fd9733aa989c012b5bfde18e23` |

- r1 파일별 source inventory SHA-256: `3ad20fbc30a61597187a5ecd09973a17eb19e8f0d47cc4d2914e08c9c10ddad6`.
- r2 파일별 source inventory SHA-256: `a2e554bbebb0f943ff7b79ae4e71ce745be667d66d4f6cadc0b211c6f5fbe2a7`.
- r3 파일별 source inventory SHA-256: `194b0ec09b491a08cf6e88b96d7eec782a9908167bdfd6e42d9a4ec66b0012bf`.
- r4 파일별 source inventory SHA-256: `df39d777e0f23bba3c2b1799533f6809abd5a3c81be21de971f871743074c093`.
- r4 full export6288 manifest SHA-256: `5c64f8089f7d1647b3bf99df6ed5b73a916466fac4145285628169980f9ea5b4`.
- r4 manifest/runner SHA-256: `cd66371d011a61fa2c3893ce40c3dc07014b1695fe6232bd3fa5795c32517b77` / `0ae6a8749d853e0d3e0789acc6b27787a3b742cb7c90282d2fc034780fe228cd`.
- 독립 native admission SHA-256: `a31454ad703e95985675547083567975c22f7b7db5f25da2ae3e5d0a2623716f`; 현재24·원 source33 evidence hash를 분리 기록했다.
- 원 N03 result SHA-256: `34118938145f748260f311aeecc4493347e05c8adbde348b7d3dfaf59de3aeba`. CURRENT/base SHA-256: `01b4ceb7fdc68c1d6ef63f5efb2edd29de43b6b3b5cb9845c20872b4e34a5125` / `d7ef8013e4acf0aa81ee523b7f5fb9496c8dddc39b74431e1e745d24f9ed38da`.

독립 `/root/source_admission_review`의 최종 receipt `artifacts/module-c2-commit-call-20261028/review-final.json` SHA-256 `88f71a30f49ed7b471aa344f4e1a659e3b9c09615e87da41ea50a75bcb0d278a`는 `ACCEPTED_LIMITED_D1_D2`다. 전체26파일(Rust15·fixture5·문서6) 26/26·생략0·100%, evidence hash67을 검토했고 필수 수정0이다. 검토자 build/test/native/verifier 실행0·제품/시험 편집0이며 총괄이 반영을 확인했다. 한정 구현·실행은 Commit의 외부 Prepared gap을 닫지만 CREATE shared atomic pair claim, 일반 entry/return floor, 다른 writer/Row 연결·C3–C5·M4/M5 제품 인수는 미완료다. 문서 verifier·push·전체 workspace/하위 wrapper는 실행하지 않았다.

<a id="c3-pair-atomic"></a>
## main의 exclusive CREATE pair 원자 leaf — 2026-10-28

main `ef95a8a`에서 [PAIR-ATOMIC-001](../contracts/dev3-module-boundaries.md#catalog-atomic-leaf)을 진행한다. 원 다중 registry close→claim과 finish/cancel 종료를 같은 leaf에 합치되 현재 `&mut` 배제를 유지한다. 작성/계약/통합 `/root`, 독립 `/root/source_admission_review`. 사전 D2는 이 한정 선행안을 수용했고 빈 victim 분기·변경 전 검산·재잠금 금지·Fenced unwind·최종 leaf identity/permit 및 source packet 계수를 필수로 지정했다. 공유 gate/typed subset·CREATE source admission·Row·M4/M5는 미완료다.

변경은 `dirty_init::tx_pair`, 제거한 `prepare_retention_pair_slots`, cfg(test) close/final 훅과 관측 및 metadata/actual fixture다. 제품 새 owner/ledger/format/파일은 없다. 두 content guard와 original registry를 보유하는 새 parent는 명시 `atomic_pair_source_bytes`로 계산하여 원 source 보수 합과 max한다. Cancel allocator read도 종료 leaf까지 계수한다. 독립 구현/상계 검토 뒤 실행한다.

[manifest](../../tests/fixtures/dev3-module-c3/pair-atomic/manifest.json)의 runtime8/nativeN01 한 번을 diff와 매핑해 고정했다. 세 metadata selector는 권한을 발급하지 않는 synthetic fixture이고 actual Reserve/Cancelled/Forward와 분리한다. actual Unstaged Busy는 준비/claim/stamp 검산 후 실제 stage 호출 직전에 원 physical stage 경쟁을 고정하며 정상 재시도를 주장하지 않는다. final finish/cancel unwind 두 경계는 claim 보존만 검사하며 해당 cut의 복구를 새 PASS로 기록하지 않는다. current-build 복구는 원 MC04 Reserve prefix와 N01 세 PID에 배정했다. 총1800초/compiler420초/case45초/native30초/admission600초/로그8MiB/stack32MiB다. 실행 전 NOT_RUN이며 완료한 무변경 사례·전체 wrapper·문서 verifier·push는 실행하지 않는다. export/target/ELF는 결과/hash 보존 뒤 삭제한다.

실행 전 독립 검토에서 MPA01의 경쟁 stage를 호출 전 설치하면 prepare가 바뀐 prefix를 먼저 읽고 목표 Unstaged 경계에 도달하지 않는다고 지적했다. cfg(test) 실제 existing-stage 직전 훅과 원 storage만 빌리는 fault closure로 옮겼다. 경쟁 R02 자체의 complete LSN/offset 증가는 별도로 검산하고 원 durable prefix·control·파일 무변경을 유지하며, 경쟁 obligation을 지우지 않고 마지막에 fence한다. 실행 전 fixture 정정이며 기존 실행 FAIL이 아니다.

사전 검토에서 registered clean fixture가 metadata0이었다는 빈틈도 보완했다. 해당 분기에 실제 nonzero page LSN/counter를 넣고 원 bytes 보존과 metadata0 전환을 별도로 검산하며 빈 victim 분기와 구분한다. 사전 검토 시점에는 실행0이었다.

사전 D1/D2 최종 검토는 위 두 fixture 정정과 precise lifetime capture, 명시 source parent, runtime8/N01 선택을 한정 수용했고 필수 수정0이다. 이는 실행 PASS나 native admission이 아니며 actual source/ELF는 build 뒤 별도 검토한다.

r1의 Clippy compile은 새 `tx_pair_exclusive_claim_in`에서 pool/registry의 세션 lifetime을 별개 익명 lifetime으로 적어 invariance 오류로 FAIL했다. source `52bdb6d1d19aa92a8d7686a6bc896f132b0dfb3ae9f532fe7c2e16eb4823ab27`,12.817초, runtime/native NOT_RUN이다. 원 로그·inventory/result를 보존하고 export/target을 삭제했다. 동일 세션 `'s`를 명시하는 타입 선언만 고치며 동작/상계/선택 범위는 유지한다. r2는 미실행 runtime8/N01을 진행한다.

r2는 Clippy/test build PASS 뒤 첫 metadata selector에서 예상 DuplicateTarget 대신 Invariant/CallerMustResolve로 FAIL했다. 새 fixture가 page9/10 Status/Undo incarnation을1로 만들어 원 retention directory의 최소2 규칙을 위반했기 때문이다. 제품 검산은 유지하고 fixture의9이상 incarnation을2로, missing-target 변조를3으로 고친다. fault 전 원 entry_registry로 fixture 유효성을 확인하고 variant를 출력한다. 원r2 source `8a40cfb2130e6b4fe527d1e0396f791b2171486110d8b3ebfa31ad75b8d1bece`/ELF `6113671dc5f462a22054ae7cadd0e179875a0a0dca11bf1d35035c079dc145b1`,39.799초 FAIL·로그를 보존하고 export/target을 삭제했다. r2의 정확한 variant는 로그에 없으며 정적 순서상 최초 유효성 검사에 도달하는variant2다. runtime PASS0/native NOT_RUN이므로 r3는 동일8개/N01 미완료 범위만 선택한다.

### 선택 실행 완료와 남은 공유 공급

r3는 Clippy/test build와 동결 runtime8을 모두 통과했다. 새 metadata의10거절·latch Busy/손상·등록된 clean/빈 victim 성공·active claim 거절·두 close unwind, 실제 Unstaged 반환·원 MC01 두 Cancelled generation·두 final unwind·MC04 Reserve prefix current-build 복구를 구분해 확인했다. 두 final unwind는 claim/closed generation 보존과 fence를 확인했으며 해당 cut의 복구를 실행한 것으로 주장하지 않는다. fence-before-unlock은 원 FencedInitRegistry Drop 순서의 정적 검토와 unwind 후 관측에 근거하며 공유 reader 경쟁 인수가 아니다.

동일 r3 source/ELF에서 actual parent+entry 상계6829B, 원 보수 합과 max16468B를 확인했다. source live peak는 memory61894/native62470B, recovered peak63270/63846B, shared20192/20328B로 기존65536B cap을 유지했다. Metadata184902/185478B·whole250438/251014B도 원 cap 안이다. 이는 source 값의 상계이며 stack/RSS 실측이나 성능 인수가 아니다. 원 source 식이 같은 다른 완료 사례는 반복하지 않았다.

독립 actual source/ELF admission 뒤 원 N01을 한 번 실행하여 PASS했다. 원 schedule·corpus·controller·BUDGET는 그대로다.

| native 단계 | PID | LSN | 결과 |
| --- | ---: | ---: | --- |
| produce | 127635 | 9 | exit -9; 외부 kill/reaped |
| recover | 127637 | 19 | exit -9; 외부 kill/reaped |
| reentry | 127753 | 25 | exit 0; 정상 완료 |

N01은 24.704468초, 원3PID 9→19→25의 ACK/복구/재진입이다. 완료한 native 재실행0이며 r1 compile FAIL·r2 fixture FAIL을 소급 PASS로 바꾸지 않았다.

artifact는 `artifacts/module-c3-pair-atomic-20261028/r1`–`r3`다. 각 source5772 파일별 inventory·runner·로그·result를 보존하고 r3 full export6291 manifest·native snapshot/trace/ACK/결과도 보존했다. 각 실행의 export/target/ELF는 결과와 hash를 기록한 뒤 삭제했으며 원 과거 evidence는 삭제하지 않았다.

| 실행 | 실제 범위·결과 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- | --- |
| r1 | Clippy compile FAIL; runtime/native NOT_RUN; 12.817초 | `52bdb6d1d19aa92a8d7686a6bc896f132b0dfb3ae9f532fe7c2e16eb4823ab27` | 생성 전 중단 | `9038b02ad38e7aa5fc77c605bf864d2e46d5dd5ab03beb44c843e76249887e19` |
| r2 | Clippy/build PASS, metadata1 FAIL; 나머지/native NOT_RUN; 39.799초 | `8a40cfb2130e6b4fe527d1e0396f791b2171486110d8b3ebfa31ad75b8d1bece` | `6113671dc5f462a22054ae7cadd0e179875a0a0dca11bf1d35035c079dc145b1` | `9f21b2de00a54168888c6ebc920646d1bfd52401b537767ac3583bb70fab0918` |
| r3 | Clippy/build·runtime8·N01 PASS; 238.119초 | `471bcd0348d4b10ddabf72fc9e45a497c1c0f0ea18656180bb0a6e1e270bf64d` | `8d1592dfbb9dc1c69ad9a237b2d3865442f299c2b51756b5bcd44240c0b2bda4` | `2444c9aa1292a8bc75a409b61c9f7930e63fa6992188be98f02688963ca02c89` |

- r1 파일별 inventory SHA-256: `b1e6b59b77cd4463a2ddf5aa4d13745a1140d4dcc7907988946c0633ca64b1b1`.
- r2 파일별 inventory SHA-256: `7b90070243b3f4ce8d736e8c3e6bb0d2a0747c2c188f643300af74e43fc5f7d3`.
- r3 파일별 inventory SHA-256: `861a6f24bcd2693c0c5bb193424f877ea79752b38e8564e67aedafef499e6607`.
- r3 `source-files.json` SHA-256: `64f3a6abe996b9cbf3fce0447aeaa58dacc0fe29758448b5f33efaa1278b5cb6`.
- r3 `native-admission.json` SHA-256: `ffd1c66e26cae24de0b7bdbffb72d2f9c788f49effbd07c61e3a66f82e73ccef`.
- r3 `catalog-native/N01/result.json` SHA-256: `ea0545c89aeec13061965d1704ad63d4c4aa3b5f88cc4f27ee0be8ed79082bc5`.
- r3 manifest/runner SHA-256: `e20094a0ec061e78505543afce84744aedc0f99d1fbbd4abfaa96ba9f88b95e5` / `0562c711af6aa185166aff4306a6999fd5c6945854e7fd88256ba49c758ed3bd`.

최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, required0으로 한정 수용했다. `/root/source_admission_review`가 변경17/17파일(Rust10·fixture1·문서6)과 evidence135개 hash를 대조했으며 검토자 재시험0이다. receipt `artifacts/module-c3-pair-atomic-20261028/review-final.json` SHA-256은 `8105cd199621854f32124e0db296bfc508c695a8efc383ae3ad97b53b126ffb7`다. 이번 공급은 exclusive pool의 원자 leaf에 한정한다. `&mut` 배제를 유지하며 shared `try_prepare_tx_pair`, ordinary pin/load/capture gate, authentic key 기반 subset, CREATE source/flush lease, Row/C3–C5·M4/M5는 여전히 미완료다. 문서 verifier·push·전체 workspace/하위 wrapper는 실행하지 않았다.

<a id="c3-pair-existing"></a>
## 2026-10-28 C3 — authentic key 기반 기존 대상과 실제 CREATE 연결

기준 main `4e68a4d`에서 [PAIR-EXISTING-001](../contracts/dev3-module-boundaries.md#catalog-pair-existing)을 구현한다. 담당/통합 `/root`, 독립 `/root/source_admission_review`다. 이전 atomic leaf와 source별 FAIL/PASS·receipt를 그대로 보존하며 이번 변경은 typed subset과 실제 호출부에 한정한다. close/finish/cancel은 여전히 `&mut`이고 ordinary gate·공유 CREATE source·Row/C3–C5·M4/M5는 미완료다.

실행 전 diff→선택은 [manifest](../../tests/fixtures/dev3-module-c3/pair-existing/manifest.json)에 동결했다. compiler5는 실제 key borrow와 inner/key privacy, 신규 metadata4는 N1..7/잘못된 identity·quota/양방향 whole9 배제/Busy·unwind 정리, source1은 바뀐 adapter와 전체 source 상계, MPA01/MC01/MC04는 바뀐 실제 Reserve/Cancel/현재복구다. 원 N01은 changed Forward 준비와 Reserve 설치/복구의3PID9→19→25 한 번이다. atomic close/final leaf의 완료 metadata·unwind 시험은 바뀌지 않아 반복하지 않는다. 원 실패·중단은 새 PASS로 바꾸지 않고 각 실행의 source manifest/hash/log/result를 보존한 뒤 새 export/target/ELF를 삭제한다. 실행은 아직 NOT_RUN이다.

### 독립 검토 반례와 source별 실행

실행 전 검토에서 Ordinary N8/9도 generic으로 인스턴스화된다는 점을 확인하여 Pair N범위를 runtime Shape로 검산했다. subset→whole9도 같은 leaf의 선택9개 count0 조건으로 닫고 양방향 시험을 추가했다. source 검토에서는 새 adapter의 move를 이유로 lower carrier를 빼던 후보식을 채택하지 않고 원 helper에 enclosing enum/Result를 보수 추가했다. 테스트 입력·제품 cap·품질 기준은 완화하지 않았다.

r1은 Clippy와 test build를 통과했지만 첫 positive compiler probe를 crate root sibling에 주입해 storage 내부 API의 E0603/E0624에서 FAIL했다. 의도한 borrow/privacy 경계에 도달하지 못했고 compiler PASS0·runtime/native0이다. 제품 visibility/source를 바꾸지 않고 runner의 주입 위치를 `storage/mod.rs` 안의 sibling으로 고쳤으며 finally에서 원문/주입 파일을 복구한다. r1은 runner가 ELF hash를 probe 뒤에 기록하던 순서 때문에 test ELF가 생성됐지만 hash를 남기기 전에 삭제했다. **r1 ELF hash는 UNKNOWN**이며 r2 값으로 대체하지 않는다. r2는 hash 기록을 compiler probe 앞으로 옮겼다.

r2의 source5781 파일별 inventory는 r1과 동일하다. 완료한 Clippy를 반복하지 않고 r1의 실제 동일 source PASS를 provenance로 연결했다. r2는 새 export를 build하고 probe별 주입 source의 compiler5를 통과했으며, runtime8은 원 r2 source/ELF에서 통과했다. compiler는 storage sibling의 positive, live read/write 이후 key mutable borrow E0502 두 개, inner/key private field E0616 두 개다. `touch(&mut key)`는 pool의 별도 가변 borrow 충돌과 구별하여 key 수명을 검증하며 직접 finish/cancel 호출을 실행한 것으로 기록하지 않는다.

metadata4는 N1..7 역순 입력/정렬·bytes/copy/observe·stamp/DPT preflight, N0/8·중복/fresh/잘못된 object/incarnation·inactive/foreign-binding/key identity·quota 거절, 양방향 subset/whole9 Busy, content Busy와 read/write unwind의 원 pins0·claim 유지·fence를 확인했다. metadata-only fixture는 실제 WAL/allocator authority나 해당 unwind cut 복구가 아니다. 실제 MPA01 Unstaged 반환·MC01 Cancelled generation 두 번·MC04 현재빌드 Reserve prefix 복구와 구분한다.

r2 source 상계는 memory live61894/recovered63270B, native live62486/recovered63862B로 원65536B cap 안이다. shared20192/20328B·metadata184902/185494B·whole250438/251030B도 원 한도를 유지한다. 실제 typed adapter와 source 식 변경을 계수했으며 stack/RSS 실측·성능 인수는 아니다. 독립 actual source/ELF admission 뒤 같은 ELF의 원 N01을 한 번 실행했다.

| native 단계 | PID | LSN | 결과 |
| --- | ---: | ---: | --- |
| produce | 139531 | 9 | exit -9; 외부 kill/reaped |
| recover | 139533 | 19 | exit -9; 외부 kill/reaped |
| reentry | 139650 | 25 | exit 0; 정상 완료 |

원 N01 3PID9→19→25는 24.516732초 PASS이며 원 controller/corpus/BUDGET를 유지했다. 완료 runtime/compiler/native 사례 반복0이다.

artifact는 `artifacts/module-c3-pair-existing-20261028/r1`–`r2`다. source별 로그/result/runner/inventory와 r2 full6300 manifest·native snapshot/trace/ACK/결과를 보존했다. 이 실행의 source/target/ELF는 정리했고 과거 evidence는 삭제하지 않았다. r1 ELF 식별 미기록 제한은 위에 명시했다.

| 실행 | 실제 범위·결과 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- | --- |
| r1 | Clippy/test build PASS, positive probe 주입 FAIL; runtime/native NOT_RUN; 43.655초 | `6a89f660405dd4d9117d9c87079d1d1c83f1dc1830a0cad9570cf86281382581` | UNKNOWN(빌드 뒤 probe 실패 시 미기록 삭제) | `1018bf7935c32ce7b946c0baa7c8b204ebc8389fff9d30b194cf170d55a65c46` |
| r2 | test build·compiler5·runtime8·N01 PASS; Clippy는 원r1 동일source 증거; 221.218초 | `6a89f660405dd4d9117d9c87079d1d1c83f1dc1830a0cad9570cf86281382581` | `03b90f4242eb961ce13bb52a0ce2396926bbd31f6d5865a236e6e4540b769eac` | `0a71eb0bf5f142367a727167b1447728af0a2e6ff3eedacca0bc317261ac730f` |

- r1 source inventory SHA-256: `b7338d78700831259c599e69697fb072653cace27a8a539b888022056b42ffb3`; runner `56f28b37b26cccdc284b87fa7d545b87f040b9b290faed7fa10b74849d031d73`.
- r2 source inventory SHA-256: `b7338d78700831259c599e69697fb072653cace27a8a539b888022056b42ffb3`; runner `6c09718aa96956b827a68dbb4774a05352e67c45c8eeb9d4615fcedbfddc418f`.
- r2 `source-files.json` SHA-256: `144e6e492fd05020b98b0458bf7f99d5ffc98bfbe3c50088d481dd59ade1e54e`.
- r2 `native-admission.json` SHA-256: `ad4e0984250378dfe16f118c68489920fc276836ed86e4873052bdfec39e8501`.
- r2 `catalog-native/N01/result.json` SHA-256: `8d9efb3a93f191d30b7bb1d0a850c0ef5693b6bbb7464925e3e19d286b7f6b20`.
- 선택 manifest SHA-256: `92fae722631cc9586775a4a828a5c0e767ab3b13ce679ab1556c0dae820db511`.

최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, required0으로 한정 수용했다. `/root/source_admission_review`가 변경21/21파일(Rust9·fixture6·문서6), evidence138개 hash와 native/cleanup을 검산했다. 검토자 재시험0이며 receipt `artifacts/module-c3-pair-existing-20261028/review-final.json` SHA-256은 `9fd97e59a08a13611bd660d7a41edd9eb46890bb7bf96dbcd1bfc8cd1562be76`다. 공급은 authentic existing subset과 실제 exclusive CREATE 연결까지다. ordinary pin/load/capture gate·shared close/finish/cancel·CREATE source/flush lease·일반 entry/return floor·Row/C3–C5·M4/M5는 미완료다. 문서 verifier·push·전체 workspace/하위 wrapper는 실행하지 않았다.

<a id="c3-pair-shared"></a>
## 2026-10-28 C3 — 공유 pool CREATE claim과 ordinary 접근 중재

기준 main `c845ea7`에서 [PAIR-SHARED-001](../contracts/dev3-module-boundaries.md#catalog-pair-shared)을 구현한다. 담당/통합 `/root`, 독립 `/root/source_admission_review`. 기존 source별 PASS/FAIL/UNKNOWN과 artifact는 보존한다. 공유 제품 reservation·writable statement/source/flush lease·Row/C3–C5·M4/M5는 계속 미완료다.

실행 전 [manifest](../../tests/fixtures/dev3-module-c3/pair-shared/manifest.json)에 실제 diff→compiler4/runtime12/nativeN01 한 번과 유한 예산을 동결했다. 새 metadata6는 진짜 pin/LOAD/uncaptured physical claim 선행, close leaf와 contender, ordinary 기존/fresh/resident/missing 요청 차단, stamp/empty ABA/origin을 검산한다. 새 실제 reentry1은 원 oracle Forward/Cancel 후 pin/LOAD/flush 재개를 확인한다. source1은 새 source 계수, MPA01은 바뀐 shared Unstaged cancel, MC04는 바뀐 close와 Drop/현재복구, System1은 ordinary gate가 영향을 주는 CF12-01만 선택한다. N01은 바뀐 actual close/finish/ordinary gate의 원3PID9→19→25다. 완료된 atomic/subset 전체 사례는 반복하지 않는다. 사전 검토 당시 실행은 NOT_RUN이었으며 최종 결과는 아래에 기록한다. 각 fresh source export/target/ELF는 identity/hash/result 기록 후 삭제하며 원 logs/results/native snapshots는 보존한다.

실행 전 독립 검토 보완: 첫 관측을 통과한 resident와 pair 밖10번째 clean Job victim의 final claim 직전에 guardless pause를 추가했다. close가 먼저 이긴 뒤 재개하면 Busy/Unchanged이며 원 snapshot/quota/attempt가 유지되어야 한다. dirty hint는 actual Begin의 dirty Root와 원 WalRequirement를 사용하고 실제 FlushRequired 선택 이후 pause한다. 그 사이 metadata-only authority로 원 pool close만 수행하며 Reserve/Cancel/durable 결과로 주장하지 않는다. 종료 key Drop의 의도된 fence를 확인한다. 추가 forwarding Result<Key>와3refs도 양쪽 source 상계에 명시했다. 사전 검토 시점에는 실행0이었다.

### 동결 선택의 실제 실행과 source 계수

독립 사전 검토는22/22파일(Rust11·fixture5·문서6)을 확인하고 required0으로 한정 수용했다. `review-pre.json` SHA-256은 `6d35c49582cfbdaee5a8b7bc8a7701fbefafa753b3dece9c76f910028792be85`다. source 계수의 추가 반환 층과 첫 LOAD 관측 이후 경합의 검출 공백을 실행 전에 보완했다. clean-final은 victim뿐 아니라 요청도 pair 밖 page13으로 두어 기존 pair 주소 gate만으로 통과할 수 없게 했다. 품질 기준·원 cap을 완화하지 않았다.

r1 engine lib/tests Clippy·test build PASS, 별도 주입 source의 compiler4 PASS, 원 test ELF의 runtime12 PASS다. positive는 실제 shared close/finish/cancel 호출을 컴파일하고 negative는 private candidate field E0616 line6 및 read가 살아 있는 실제 finish/cancel의 mutable key E0502 line8을 각각 확인한다. compiler probe는 실행된 runtime 결과로 표시하지 않는다. System selector는 `XADB_STATUS_INIT_CASES=CF12-01`로 고정해 완료한 다른 cut을 반복하지 않았다.

metadata6는 pin/actual LOAD/capture-before-pin 선행의 변경 없는 거절, 같은 leaf의 close와 contender, ordinary existing/fresh/resident/missing 차단, full stamp/empty-generation ABA와 origin, 관측 후 resident/unrelated clean claim의 재검산을 확인했다. 실제 MPS01은 R01/R02 뒤 Forward R03 또는 Cancel K03 oracle와 ordinary pin/LOAD/preflight/원 selected-DWB flush 후 clean capture 재개를 확인한다. MPS02는 실제 dirty Root/WAL requirement와 FlushRequired 관측 뒤 **metadata-only close**를 사용한 반환 경합 증거다. MPS02를 Reserve·Cancel·durability 실행으로 분류하지 않는다. 원 MPA01 Unstaged 반환·MC04 Drop 및 현재빌드 recovery도 바뀐 경로 한정으로 통과했다.

source 상계는 memory live63496/recovered64872B, native live63632/recovered65008B로 원65536B 이하다. shared20192/20328B·metadata186504/186640B·whole252040/252176B도 원 한도 안이다. Catalog live closure43800B, split ports192B를 포함한다. 이 수치는 source-value 상계이며 stack/RSS/성능 측정이 아니다. 독립 actual source/ELF admission 뒤 원 N01을 같은 ELF로 한 번 실행했다.

| native 단계 | PID | LSN | 결과 |
| --- | ---: | ---: | --- |
| produce | 149994 | 9 | exit -9; 외부 kill/reaped |
| recover | 149996 | 19 | exit -9; 외부 kill/reaped |
| reentry | 150009 | 25 | exit 0; 정상 완료 |

원 N01의3PID9→19→25는 24.332274초 PASS다. 원 controller/corpus/BUDGET와 quality/stop 규칙을 유지했으며 완료 사례 반복0이다.

artifact는 `artifacts/module-c3-pair-shared-20261028/r1`이다. 로그·결과·runner·source5788 파일별 inventory·전체 export manifest·native snapshot/trace/ACK는 보존했다. 이번 source/target/ELF만 identity와 hash 기록 뒤 삭제했으며 과거 evidence는 삭제하지 않았다.

| 실행 | 실제 범위·결과 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- | --- |
| r1 | Clippy/test build·compiler4·runtime12·N01 PASS; 294.086초 | `68c9d3de59b39d1e0228d37687f3ecd722ff998354e217a28c041d1cf54bae26` | `2d608fe6986861a29aa2eeb7f24d84df40f23016e86b2e6cf2a00a20ae013fa0` | `bb15ccb21a87843c73dcecb8e79ac4aaaed658d16f58bab5d3ac16e9e9dc4ab2` |

- r1 `source-inventory.json` SHA-256: `1cb6fbd4ea0ae22f66ba305de0ce8a9b5710334575df504f86431367889bb9e6`.
- r1 `source-files.json` SHA-256: `fd67d9e320e717bd10f6181410bc2ec12d3a6cefd2031f9157f9cf58fb0279ca`.
- r1 `runner.py` SHA-256: `716b1a7f573b3b0df0b148c2e65795847df1de0444f4af6feecb5d51ae6a4a36`.
- r1 `native-admission.json` SHA-256: `d3acda3b705487fc851ff544943fce0b24c5b66b78992078a844028b9435d0a7`.
- r1 `catalog-native/N01/result.json` SHA-256: `0c80a88c520e7c762369828d32e80d3f17b9e4d48f0a99f2d42cf8a436bb31f1`.
- 선택 manifest SHA-256: `ca61a4ef9ef736b564c33a8564ccee303f063b52a59d29d52a9dd40f3ea7ccd9`; ELF 186526424B.

최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, required0으로 한정 수용했다. `/root/source_admission_review`가22/22파일(Rust11·fixture5·문서6), evidence138개 hash와 native/cleanup을 검산했다. 검토자 재시험0이며 receipt `artifacts/module-c3-pair-shared-20261028/review-final.json` SHA-256은 `e4b933fbd250288aed69b35b307e01e2ab442cf12a3d82a1002a7e8a94c0f76c`다. receipt가 허용한 문서/JSON의 수용 metadata만 후속 반영한다. 공급은 공유 pool claim/ordinary 중재와 원 exclusive CREATE 호출부까지이며, 전체 bound를 빌리는 `ReservedCatalogCreate`의 shared 제품 수신·writable actor/statement·원 source/flush lease·일반 entry/return floor·Row/C3–C5·M4/M5는 미완료다. 문서 verifier·push·전체 workspace/하위 wrapper는 실행하지 않았다.

<a id="c3-create-parts"></a>
## 2026-10-28 C3 — CREATE 원 자원의 짧은 borrow 본문

기준 main `8613c53`에서 [CREATE-PARTS-001](../contracts/dev3-module-boundaries.md#catalog-create-parts)을 진행한다. 담당/통합 `/root`, 독립 `/root/source_admission_review`. 실제 CREATE planner/Forward/Cancel/allocator 반환을 원6자원의 private view로 옮기며 기존 exclusive reservation을 유지한다. 공유 제품 admission·writable actor/statement·source/flush lease·일반 entry/return floor·Row/C3–C5·M4/M5는 미완료다.

실행 전 [manifest](../../tests/fixtures/dev3-module-c3/create-parts/manifest.json)에 compiler2/runtime7/nativeN01 1회와 입력·품질·유한 예산·실패 시 중단을 동결했다. 신규 borrow 컴파일2와 실제 pending/flush 오류 우선순위·Cancel Before 반환2, 변경된 본문의 정상 Forward/Cancel·Unstaged·final leaf unwind·Drop/current recovery·source를 선택했다. N01은 변경된 실제 Reserve/Forward의 원3PID9→19→25다. 순수 metadata/ordinary gate/System 완료 사례를 반복하지 않는다. 실행 전 기록 당시 NOT_RUN이었고 독립 사전 검토 뒤 아래 범위를 실행했다. 실행별 source/target/ELF는 identity와 hash 기록 후 삭제하고 원 로그/결과/native snapshot과 이전 source별 실패·UNKNOWN을 보존한다.

### r1 실패 보존과 fixture 한정 수정

r1은 scoped Clippy·test build·compiler2와 runtime1–5(source/정상 Forward·Cancel/Unstaged/final unwind/Drop-current recovery)를 통과했다. 신규 MCP01은 의도한 pending Fatal/fence와 상태 불변 검산을 마친 뒤 fixture가 fenced owner의 `check_idle()` 성공을 요구해 실패했다. MCP02/native는 미실행이며 r1 FAIL을 보존한다. 제품 코드와 source 식은 바꾸지 않고 새 fixture의 fenced owner 검산을 정확한 `Cause::Fenced` 기대로 수정한다. 아직 실행하지 않은 MCP02의 동일 잘못된 종료 기대도 함께 고친다.

r2 실행 전 선택은 다음과 같다. r1 inventory 대비 변경 파일이 `catalog_memory/create_parts.rs` 하나인지 실행 전에 검산하고 changed MCP01 및 미실행 MCP02만 실행한다. 완료 compiler2/runtime1–5는 반복하지 않는다. 원 source bound PASS를 원 source/ELF/hash와 함께 보존하며, 현재 build에서 제품/census/type 소스 불변을 독립 검토해 native source admission을 결정한다. r2 source-case 새 실행 PASS로 표기하지 않는다. N01은 이전 run에서 미실행한 동일 범위1회만 남아 있다. 예산·정답·native controller·품질 한도는 그대로다.

### source별 선택 결과와 한정 공급

사전 독립 D1/D2는 변경16/16파일(Rust7·fixture3·문서6) 및 runner를 검토해 required0으로 한정 수용했다. receipt `artifacts/module-c3-create-parts-20261028/review-pre.json` SHA-256은 `338dac70023ba3388c7ab6b30198202fee477d2fe9490144964737620ea85718`다. r2 fixture 정정·선택도 별도로 한정 수용했다. source 식은 CREATE에만 늘렸고 기존 reservation·split port·공통 preparation/ordinary/lifecycle 크기를 변경하지 않았다.

| 실행 | 실제 결과·범위 |
| --- | --- |
| r1 | scoped engine lib/tests Clippy·test build PASS, 별도 주입 compiler2 PASS, runtime1–5 PASS; MCP01 FAIL 뒤 중단. MCP02/native NOT_RUN |
| r2 | cfgtest1파일만 바뀐 inventory를 검산. scoped Clippy·test build PASS, 수정 MCP01/미실행 MCP02 PASS, 원 native N01 1회 PASS |
| 재사용 | r1의 compiler2/runtime1–5는 원 source/ELF의 PASS로 보존. r2에서 다시 실행하거나 새 PASS로 표시하지 않음. 제품·census/type/입력 소스 불변을 독립 검토한 source admission만 현재 ELF에 적용 |

r1 source5793파일 `9c7b9f12d4848b4aab0ff095eb2b270631b84a6e706ae0c25c12e48890418e90`, ELF `9bdae3eb6b5ab77d87f3b62177515477d268488950ce97b05f97dd38e8918ce4`(187011384B), result `b75bcadc0f8481447b8ffdb387c7550020f1583458f71e7d0a679a6f86206941`이며 원 FAIL을 유지한다. r2 source5793파일 `1481cf2f8e1d3d0be84b89ef373b0d4dd314477c3aa76a008c53d69596d72297`, inventory `f48137586a4fbfef2ce83071ef57e1b4fd24a9cd14c165a973299330e8cdf308`, ELF `ae1b1f035b018288c011456be9f00d4fd6ced7e017ce451adc75e716b53ddd87`(187007968B), manifest `8faeea88d576f55c92b69ca23b415264f2b10201118ac678655bb6dcb38c26a0`, result `843881136902315be7a8ff2bb49fee79447d1f70d62c9e2e15448f875011d7e7`다. 전체 export6312 파일 manifest는 `8eac36dada76a22bda0b84c0987a289235d1e1e9ce18331bc094c497f540ef21`다.

r1의 실제 source 관측은 memory live63496/recovered64872, native live63632/recovered65008B이다. shared20192/20328, metadata186504/186640, whole252040/252176, split ports192B로 원 cap을 유지한다. CREATE phase는 memory plan22571/reserve33256/Forward34468/Cancel33584, native plan22571/reserve34280/Forward34552/Cancel34608B다. 전체 Catalog peak43800B는 기존 retirement가 결정하므로 새 CREATE 호출 비용에도 전체 상계가 같으며, 이 수치는 runtime admission이나 shared 제품 CREATE 완료가 아니다.

r2 native admission `51aeeea0d4e5fbcdf5bd1dee3b7faff804cd84f5299a833edd59590faaa82f9e`은 원 source-case 실행·동일 제품 소스·현재 ELF를 구분해 검산했다. N01 PID 155218,155221,155228, LSN9→19→25, 24.119422초이며 앞2 process는 controller SIGKILL/reap, reentry는 exit0이다. native result `3360079220743de8b823a9b66b28d7f3551f145f10f553c719963e076cb49350`. r2 전체 236.323초다. 새 run의 source/target/ELF는 identity 기록 뒤 삭제했으며 원 로그·결과·runner·hash inventory·native snapshot/trace/ACK는 보존했다. 과거 evidence는 삭제하지 않았다.

최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, required0으로 한정 수용했다. 검토자 `/root/source_admission_review`가16/16파일(Rust7·fixture3·문서6), artifact137개 hash와 native/cleanup을 검산했다. 검토자 재시험0이며 receipt `artifacts/module-c3-create-parts-20261028/review-final.json` SHA-256은 `3bcb430855ac823ab6f97c0c2d536d06d23d9f3cf47b6044fff2dad0b6a6ec06`다. receipt가 허용한 수용/status/hash metadata만 후속 반영한다. 공급은 원 CREATE planner/Forward/Cancel/allocator 반환의 private disjoint borrow 본문과 기존 exclusive wrapper 연결까지다. shared 제품 reservation·writable actor/statement/source/flush lease·일반 writer entry/return floor·Row/C3–C5·M4/M5는 미완료다. 문서 verifier·push·전체 workspace/하위 wrapper는 실행하지 않았다.

<a id="c3-create-lease"></a>
## 2026-10-28 C3 — 원 CREATE 자원과 flush 대여의 연속 소유

기준 main `3fa5f6f`에서 [CREATE-LEASE-001](../contracts/dev3-module-boundaries.md#catalog-create-lease)을 진행한다. 담당/통합 `/root`, 독립 `/root/source_admission_review`. 실제 예약이 원6자원을 빌리고 selected generation·origin·pair를 동결한 원 flush lease를 끝까지 소유한다. 공유 제품 입구를 raw actor로 새로 추가하지 않는다. writable statement/source admission·일반 floor·Row/C3–C5·M4/M5는 미완료다.

실행 전 [manifest](../../tests/fixtures/dev3-module-c3/create-lease/manifest.json)에 compiler3/runtime10/nativeN01 1회와 실제 diff 연결·유한 예산·품질·중단 조건을 동결했다. source 식에는 확대 token/Before/lease 반환·원 reserve tuple·origin/check/preflight 비용을 명시한다. test oracle는 전체 bound 대신 동일 원 pool/bank/jobs를 빌리는 관측 trait를 사용하고 원 byte 기대값은 유지한다. 사전 core 검토의 source acquire/check 누락을 보완하고 flush preflight의 한정 오류 순서를 계약에 명시했다. 실행 전 동결 당시 NOT_RUN이었다. source/target/ELF는 결과/hash 기록 뒤 삭제하며 원 로그/결과/manifest와 과거 증거는 보존한다.

첫 r1은 Clippy/test 정적 빌드에서 source 식의 괄호·원 flush_io 경로·moved preparer 호출을 잘못 연결해 FAIL했다. compiler/runtime/native 실행0이며 원 로그/source identity를 보존했다. r2는 해당 코드 연결과 Reserve 후 검산 실패의 Fatal 승격을 수정하고, 신규 MCL03의 actual FlushIo foreign origin·authentic retained flush under Ready 잔여 두 반례를 실행 전에 추가해 runtime10을 동결한다. 품질 기준과 기존 required selector는 줄이지 않는다.

### source별 결과와 실제 복구

사전 독립 D1/D2는18/18파일을 검토했다. 처음 검토의 source 식/연결 오류와 actual FlushIo origin·Ready 잔여 음성 검증 공백은 r2 실행 전에 보완했다. superseding `review-pre-r2.json` SHA-256 `d3bad7f2d7cd42911975b5a15424926bc0bf8c535337da58a96fe372a460cf1b`는 required0으로 compiler3/runtime10 실행을 수용했다. 검토자 재시험0이다.

| 실행 | 실제 범위·결과 | source SHA-256 | 결과 SHA-256 |
| --- | --- | --- | --- |
| r1 | Clippy/test 정적 빌드 FAIL, compiler probe/runtime/native0; 13.004초 | `aa63cdfb05654a70318f74ce6d289e36db7102e16a77441b8616476a6b3f8347` | `e2578f7267e9c8af73eeb18a854cb17472fd7fee141f0ed8f64db378a842e738` |
| r2 | scoped engine lib/tests Clippy·test build·compiler3·runtime10·nativeN01 PASS; 274.138초 | `7be6bf8cc6188165ba0d795d6d4ddd1d3613d4873251f6c7d7183e8c337b9ca0` | `744891f65b7f4d466dba03b905bae52b050423be98e3f14ef1c91d2358208280` |

r2 ELF SHA-256은 `f231bfdc26802aa74312e3aa184eccfdd9e2f7768134966d52a1a8dd9612e276`(187149920B), source5798 파일별 inventory `b551262ac76624fabeeeb58a411d446eb451c1f8f2efc01e0becf8025ffb40e7`, 전체 export6317 manifest `2e6a3bb1fb544b72c14aa0d56f8c2f7962447d5af4f3c83504ad92ac8bb5fab3`, runner `3ab0b8c5d0ae8648976600ffb54366628cf3a4475c93d7b1974c8e7089be4e95`, 선택 manifest `b26b451a25f52af9a0dd12c807ca7fb715e5ce7fb44184a84b3da193087cc6bb`다. r1은 ELF 생성 전 실패했으며 ELF identity는 N/A다. 원 FAIL과 과거 source 결과는 보존하고 완료 무변경 case 반복0이다.

compiler3은 실제 reserve→initialize 성공과 bound 재사용, private view가 빌린 원 flush의 동시 assignment E0506 line9, 소비한 reservation 재사용 E0382 line10을 확인했다. runtime은 바뀐 원 body의 Forward/Cancel/Unstaged·final unwind·Drop와 현재복구·pending Fatal 우선·Cancel Before Drop을 원 corpus로 확인한다. MCL01은 fault-only WaitingSource 거절이 같은 원 flush 주소/세대·claim/bytes/WAL을 유지한 Before를 돌려주고 원 phase를 복원한 뒤 실제 Cancel K03을 완료한다. MCL02는 동결 세대/쌍/lease origin 변조를 Before로 통과시키지 않고 무효과 Fatal/fence/claim 유지로 확인한다. MCL03은 actual FlushIo의 foreign binding 및 실제 RetiredPendingFinalize capture/receipt 아래 Ready label 변조를 통해 두 신규 acquisition 거절 경로에 도달한다. 이 fault-only 주입을 정상 제품 유지보수 동시 호출로 주장하지 않는다.

source 관측은 memory live63496/recovered64872B, native live63632/recovered65008B다. shared20192/20328B, metadata186504/186640B, whole252040/252176B, split ports192B를 유지한다. CREATE는 memory plan22571/reserve36824/Forward35324/Cancel34440B, native plan22571/reserve37848/Forward35408/Cancel35464B이며 전체 peak43800B는 기존 retirement가 결정한다. 이는 source-value 상계이며 stack/RSS 실측이나 전체 runtime admission이 아니다.

독립 source/ELF admission `b19e1035d38d45b2a67b28a8650ac81adf19d9447b0a553843ba5224b1d3ad43` 뒤 같은 ELF의 원 N01을1회 실행했다. produce PID166321/LSN9와 recover PID166324/LSN19는 controller SIGKILL/reap, reentry PID166335/LSN25는 exit0이다. 24.598205초 PASS이며 native result SHA-256은 `23b5134fb13d180845487bf409479b03ad4f93bb2d1dc0f7c63b86f4ff52e30b`다. 원 corpus/controller/quality/stop 기준은 유지한다.

이번 r1/r2의 source/target/ELF는 hash/result 기록 뒤 삭제했다. 원 로그·runner·결과·파일별 inventory와 native snapshot/trace/ACK는 `artifacts/module-c3-create-lease-20261028`에 남겼으며 과거 artifact는 삭제하지 않았다. 최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, required0이다. `/root/source_admission_review`가18/18파일(Rust8·fixture4·문서/JSON6), artifact147개 hash와 native/cleanup을 검산했다. 검토자 재시험0이며 receipt `artifacts/module-c3-create-lease-20261028/review-final.json` SHA-256은 `1f473b008c7807f8c894b52eb10f07ef643ce325b933d91ffca35a114d88ee8c`다. receipt가 허용한 수용/status/hash metadata만 후속 반영한다. shared CREATE 제품 입구, writable actor/statement 및 source admission·일반 floor·Row/C3–C5·M4/M5는 미완료다. 문서 verifier·push·workspace/하위 wrapper 실행0이다.

<a id="c3-write-capsule"></a>
## C3 실제 빈 쓰기 거래 capsule — 2026-10-28

main `b35150c`에서 [WRITABLE-EMPTY-001](../contracts/dev3-module-boundaries.md#writable-empty-capsule)을 구현한다. 원 독립 D2 설계 검토는 실제 Begin 이후 fence, 원 actor/phase 대조, 명시 statement 종료 재검산, MustAbort 재시도와 AbortEnd terminal handoff 검산을 요구했고 구현에 반영했다. 선택 검증은 아래 source별로 완료했으며 최종 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 새 파일은 기존 문서 ID를 유지하며 source 경로를 build JSON에 등록한다.

선택은 `tests/fixtures/dev3-module-c3/write-capsule/manifest.json`, runner/결과는 `artifacts/module-c3-write-capsule-20261028`이다. r1 visibility/thread capture compile FAIL, r2 Clippy collapsible-if FAIL을 원 source·로그와 함께 보존한다. 이 두 실행의 runtime/compiler probe는 NOT_RUN이다. 각 실행의 source/export/target/ELF는 실행 출력·hash를 수집한 뒤 삭제하고 삭제 목록을 포함한 최종 result를 저장한다. 기존 evidence artifact는 보존한다.

| 실행 | 실제 범위·결과 | source SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 | Clippy 정적 compile FAIL; probe/runtime0; 13.009초 | `7c96afacc2f5bbbb28dc1290e41420a2f4dba2c011870ebb649c12a3e68eab27` | `05429e7ee703bbd508b26d80e587532f74a4e605f2308feb6101b46baa3bc576` |
| r2 | Clippy lint FAIL; probe/runtime0; 18.758초 | `7df0bc4da3ba3c95cf444bc5f729b3242f99eb981389d96274f04f96fee457aa` | `a3144a431258b4845991de9e4e83151d9486d559f2a4016bc7d7e2c76a13892f` |
| r3 | scoped Clippy/test build·compiler3·runtime14 PASS; 72.251초 | `bf2117acbc16582d5fdfccbdb56328015084b942cdbe01d3301f08e4bcbefe29` | `54408348af763a130a192bf8adff14160a45bf743b546b4b047bcfcbca5fbc87` |
| r4 | source 계산만 수정한 scoped Clippy/test build·runtime2 PASS; 41.045초; semantic/compiler probe 재실행0 | `438474f29a97b3f59a5d3c66127f3d8168d4de220e09833ffbd2ecaf06d34537` | `2118189e8c664d76b79459cf8a7ff40c8b2d9a9eeadf753edcc3e8999fbeea30` |

r3의 compiler는 실제 Begin→statement 종료→empty abort 성공, live view가 빌린 transaction 소비 E0505 line10, abort로 소비한 transaction 재사용 E0382 line12를 검산했다. runtime14는 source3·신규 writable6·변경 readonly5다. WT01은 Serializable/RC 각각 원 Begin의 R01 raw oracle과 actor/slot/sequence/512B, readonly 종료 거절·own-read Unsupported·실제 AbortEnd handoff를 검산한다. WT02는 wrong target/role/foreign owner·source/slot Capacity의 무효과와 원 loan 반환이다. WT03은 실제 RollbackStart 뒤 외부 source credit을 보유하여 AbortEnd 진입 Capacity를 만들고, 같은 identity/phase2/512B capsule의 재시도로 LSN7→8을 검산한다. WT04는 미종료 tx/view Drop과 actor/slot/phase fault의 no-I/O fence다. WT05는 fixture 전용 private view로 **실제** CREATE를 수행해 R03 oracle을 맞춘 뒤 nonempty begin/finish/abort를 거절한다. fixture bridge를 제품 shared CREATE API나 source admission으로 주장하지 않는다.

WT06은 새 capsule의 실제 Begin 후 Drop 이미지(capture6)와 실제 AbortEnd 뒤 이미지(capture8)를 원 fresh recovery runtime에 공급한다. 각각 bounded cleanup `[7,8,9]`/`[9]`, Q0, 빈 ATT·old capsule0, 실제 missing query를 검산했다. 이는 **현재-build memory 복구**이며 native crash/durability 인수나 기존 N01 재실행이 아니다. unchanged native·workspace/lower wrapper 실행0이다.

독립 구현 리뷰가 lifecycle wrapper closure 참조와 `check_ledger`의640B aggregate를 명시하도록 요구해 r3 뒤 source 계산만 고쳤다. [후속 선택](../../tests/fixtures/dev3-module-c3/write-capsule/source-followup.json)의 r4는 관련 source2만 실행했고 r3 semantic11/다른 source1/compiler3은 반복하지 않았다. r4 inventory와 r3의 차이는 source 계산 파일 및 후속 manifest뿐이다. 완료한 과거 결과를 새 source의 재실행 PASS로 표시하지 않는다.

r3 source handle/return은 transaction416B·statement328B, wrapper1081B·helper21368B·lifecycle memory42711/native43735B다. r4는 handle416/328B, wrapper1113B·helper22408B·lifecycle42743/43767B이며 retirement43800B와 기존 전체 peak가 유지된다. memory live63496/recovered64872B, native type live63632/recovered65008B이며65536 상한을 늘리지 않았다. source-value 상계이지 stack/RSS나 전체 동시 admission 인수가 아니다. pre/postflight 및 독립 statement helper와 entry/return floor는 아직 미공급이다.

r3 ELF `9ab7318ac0a72a2c7cce0f5a2643ecde82c8f3b8fc8d22a1219ef7dae257d494`(188162192B), source inventory5804개 `3a7987f157d7a514284fc84a9b4e37b8aa4425333a2101ad0a5db5cbd9bc01a8`, 전체 export manifest `2ad34af8f88c15d843fc9ebc134a772348b9d9bb5f8b86619600bc8a54463f74`, 선택 manifest `a8c04f82062b31012a44b4e67773a1e5cdef5f117dfe36cd57e86d2f78397c6a`, runner `45314d08846d07b79377343fd3e2b988cf3089b9a8c2244c698a28fb782e1df3`를 보존한다.

r4 ELF `1b60f3574e96c2f84a3b93309569e5e3590d2fbce2def27855b4e2c11c3064a3`(188199184B), source inventory5805개 `286db736b5d98974336e5ed56e583a0cc7c388ed564670b5ae171b552b8535f8`, 전체 export manifest `6d0b4af6eb505c571f988d9a1cdb93277b9ec82de1a6ca2e4b2e4cbc60741eb6`, 선택 manifest `aa7f34048c7c19308e53d11afd81ef12feacf7c840030147c39d0cf107375751`, runner `86d1917442b9fd8943a2ac43f44adc73ccc6268f33e9f66447a93ba13bf87df6`를 보존한다.

r1–r4의 임시 source/export/target/ELF는 실행 출력·hash를 수집한 뒤 삭제하고 삭제 목록을 포함한 최종 result를 저장했다. 원 로그·결과·runner·파일별 hash inventory는 `artifacts/module-c3-write-capsule-20261028`에 보존한다. 기존 증거 삭제0, 문서 verifier·push0이다. 독립 최종 receipt는 아래에 보존한다. 자동 Status 선택·own actor 읽기·shared CREATE·일반 statement mark/Row/rollback/commit·전체 floor·C3–C5·M4/M5는 미완료다.

최종 identity 확인 도중 검사 스크립트를 `-B` 없이 import하여 ignored `check_dev0` bytecode cache만 갱신됐다. 기존 artifact의 exact SHA-256 `1455a7fc4604e4b0cc15449a532b049bee621f4e91669d95c9165201452fb766` 사본으로 복원했고 새 cache와 `cache-restoration.json`을 이번 artifact에 보존했다. `python3 -B`로 확인한 최종 전체 source fingerprint는 r4와 정확히 같으며 제품/fixture diff와 추가 시험은 없다.

최종 작성자 재검토에서 read-only Drop이 actor=None·slot phase2/3 손상을 healthy 반환할 수 있는 새 불변식 누락을 찾았다. r4 source까지의 독립 수용 receipt는 `review-before-phase-fix.json`으로 보존했다. None actor의 phase1 검산과 실제 lease 반환의 readonly1/writable3 검산을 추가했다. [phase 후속 선택](../../tests/fixtures/dev3-module-c3/write-capsule/phase-followup.json)은 새 WT07(정상 readonly Drop 뒤 phase2/3 손상·no-I/O fence·slot1/credit512 유지)과 변경된 writable 반환 조건을 직접 지나는 WT01·WT03만 고정한다. r5의 결과는 아래에 별도 기록하며 원 r3/r4는 유지한다.

r5는 scoped Clippy/test build와 선택 runtime3을 모두 PASS했다(44.849초). source `dcac813d80839228df3e865b3d2cbfac166111a158bceb45710a737e12d4e9c1`, result `23473b5276e323b590860c55228c7e37dd8db4ec3662fad1dec9e9694d6dedc5`, ELF `d60f518179ef6cc27a8426a69b35edcbc2206c45a60cf7490919aa89baa7fe8b`(188280688B), source inventory5806개 `741fe51d21e452cf9c509d3aa1af902ec9a58aa63c8e7ac62f1dba765427b9c6`, 전체 export manifest `1e6fc0ddc7ae59988346a24338586e767b7501d56eab364b314d91011412460b`, 선택 manifest `fb25d96fe37fe775d3407e89d162019ad1818a04f1deddf81d3eddec699c42e4`다. compiler probe·source census·memory 복구·native를 다시 실행하지 않았다. 추가 타입/필드·source 계산 변경이 없으므로 r4의 구조별 source 결과는 원 source에 보존하고 새 실행으로 승격하지 않는다. r5 임시 source/target/ELF도 출력·hash 수집 뒤 삭제하고 최종 result에 기록했다. 최종 독립 receipt는 아래에 기록한다.

최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, required0이다. `/root/source_admission_review`가 변경21/21파일(Rust8·fixture6·문서/JSON7), artifact88개 hash와 source별 결과·cleanup을 대조했다. 현재 source는 r5와 정확히 같고 source census는 r4 실행 증거로 유지한다. 검토자 재시험0이며 receipt `artifacts/module-c3-write-capsule-20261028/review-final.json` SHA-256은 `272bc57c8e657ac3e12928aff9136c29637c33117829a85d06134c14f10e06bb`다. receipt가 허용한 수용/status/hash metadata만 후속 반영한다. 이번 빈 capsule 공급을 한정 수용하며 자동 Status·own actor 읽기·shared CREATE·일반 Row mark/rollback/commit·전체 floor·C3–C5·M4/M5는 미완료다.


<a id="c3-catalog-capsule"></a>
## 2026-10-28 실제 첫 Catalog statement의 거래 완료 연결

기준은 `main` `4ac878c`다. 앞선 [빈 쓰기 capsule](#c3-write-capsule)의 증거·FAIL·source identity는 보존한다. 기존 미완료 C2 stash는 적용하지 않았다. [첫 Catalog 거래 완료 계약](../contracts/dev3-module-boundaries.md#catalog-capsule-completion)의 구현과 source별 선택 검증을 마쳤고 최종 독립 D1/D2를 한정 수용했다. 아래 실행 전 동결과 source별 실제 결과를 구분한다.

실행 전에 [manifest](../../tests/fixtures/dev3-module-c3/catalog-capsule/manifest.json)를 동결한다. compiler3은 새 consuming Commit 정상 경로/생존 view의 거래 소비 거절/소비 뒤 재사용 거절이며, runtime13은 신규 CT01–CT08, 새 source census, 변경된 nested Commit/retirement 비용 한도, 변경된 writable finish의 WT01/WT05, readonly phase4 오류 WT07이다. CT01은 실제 첫 Forward의 Commit·full abort와 SER/RC를 원 oracle에 대조한다. CT02는 원 statement/transaction의 Capacity·foreign owner 반환, CT03은 실제 RollbackStart/CLR 뒤 실패와 재시도, CT04는 실제 다른 거래의 Commit/Q에 의한 Serializable conflict·RC 성공, CT05는 실제 Commit 뒤 검산 Capacity의 Fatal, CT06은 빈 Commit·후속 statement의 한정 Unsupported, CT07은 새 Commit9/AbortEnd11 image의 현재-build 복구와 원 cleanup15/16·Q·조회 결과를 검산한다. CT08은 독립 검토에서 확인한 isolation mode/mark 손상의 WAL 전 Fatal을 검산한다.

전체1200초, compiler단계420초, case45초, case출력8MiB, Rust test stack32MiB로 제한한다. `python3 -B`로 새 artifact runner를 실행하며 실패 즉시 중단한다. 실행 r1–r5의 실제 순서는 원 source manifest·ELF hash·로그 수집, source/target/ELF 삭제, 삭제 목록을 포함한 최종 result.json 기록이다. 이번 top-level runner에는 후속 실행부터 삭제 전 result-before-cleanup.json을 먼저 기록하도록 보완했다. 이 보완은 이미 완료한 r1–r5의 실행 순서나 결과를 소급 변경하지 않는다. 기존 결과는 삭제·재분류하지 않는다. 무변경 native N01·완료 하위 회귀·workspace 전수는 실행하지 않으며 메모리 복구를 native durability 인수로 표시하지 않는다.

실제 shared CREATE 제품 admission·일반 Row 완료·전체 source floor 및 M4/M5는 계속 미완료다.


첫 실행 r1은 새 시험 helper의 부모 `Result` alias 충돌로 compile FAIL(runtime0), r2는 cfg(test) 항목 순서/범위 패턴 Clippy FAIL(runtime0)이며 모두 보존한다. r3은 static2/compiler3 PASS·새 source case1 PASS 뒤 기존 source case2의 recovered native-type65600B로 FAIL했다(상한65536B보다64B 초과). memory/live64088B·native/live64224B와 원 FAIL을 성공으로 재분류하지 않는다.

후속 변경은 실제 CLR arm이 `if let Err`의 Observation Result 하나만 소비하고 다음 loop로 가는 반면 Commit/AbortEnd는 별도 arm에서 Observation을 보유한다는 차이를 source 식에 반영한다. CLR의 actor·selected 값/반환·참조·UnwindFence를 별도 계수하며 optimizer/last-use 할인이나 상한 변경을 하지 않는다. [후속 선택](../../tests/fixtures/dev3-module-c3/catalog-capsule/source-followup.json)은 이 변경의 source2와 미실행 WT/CT11이다. r3에서 완료한 compiler3은 반복하지 않는다.


r4은 static2와 runtime9(source2, WT01/WT05/WT07, CT01–CT04) PASS 후 CT05의 expected Job 조립 오류로 FAIL했다. CT05가 원 row17 before-bank에 row31 after-image를 반영하지 않은 것이며 cell0의35B 불일치를 보존했다. 기존 `Oracle::check`와 같은 원 C04 Job-page patch를 새 CT05에만 적용하고 byte equality를 유지한다. 수정 근거(로컬 `../../artifacts/module-c3-catalog-capsule-20261028/r4-oracle-correction.json`)와 [후속 선택](../../tests/fixtures/dev3-module-c3/catalog-capsule/oracle-followup.json)은 실패 CT05·미실행 CT06–CT08만 포함한다. 제품 source와 완료한 source/WT/CT01–CT04를 반복하지 않는다.

r4 source 관측은 handle/return416/328B, Commit caller592B·CLR caller504B·helper24274B·progress44304B다. memory live64000/recovered65376B, native-type live64136/recovered65512B로 원65536B 한도 안이며 마지막 여유24B다. 이는 한정 source census이고 전체 entry/return floor·stack/RSS·native 실행 인수가 아니다.


r5은 static2/runtime4(CT05–CT08) PASS다. CT07의 실제 Commit capture9는 cleanup10..15/Q1/Found, AbortEnd capture11은 cleanup12..16/Q0/Missing을 확인했고, 복구된 새 session에는 이전 capsule0이다. CT05의 실제 Commit 뒤 Capacity는 Fatal과 원 C04 WAL/ATT/Job after-image를 대조했으며 mode/mark 손상은 추가 WAL 없이 fence했다. source별 r3 compiler3, r4 runtime9, r5 runtime4로 고정 inventory를 충족했다. r3의 source1 PASS는 당시 source 증거이며 r4 source 수정 뒤 대응2건을 새로 검증한 것이다. 원 r1–r4 FAIL을 지우거나 최종 source의 전수 PASS로 합치지 않는다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r3 | `ae1daf9e136e83895a0ff7bbe35db688e85b17ac16fb99d25b8948fa294c32d9` | `51668e814e7a151589812aa75c7df6d2fca05d31bfede3e0aa6de8bb6b7ca70f` | `69969cbdf125e3aaa0a64302d7b13ecfdbc720f95a588851350284a923739d5f` |
| r4 | `b5f06783f3b4a68dec826692377a2155b38e578c9769374d4b5b9f1d1c2c4cba` | `3b2a60bcdcd4fd2aa02ab74081a044ce0f52688551eaabe6de392ad59f2214e6` | `8bb597767c9752899e89f116b0ee231749e4b764412ae2b3a5cc671a9c5592d9` |
| r5 | `fc4a46ae6447b8cf2a489151026bd4cec31343d99db32ff7dd8b649bcec1f6ad` | `509526fbcdf9506808755c84e4e987f7d5285abebdd580dfefd92a0a3f4c4552` | `1fd1769ac72314aacf990da2a6fc00bee978279da0ba42eb902d146bc1081eba` |

artifact(로컬 `../../artifacts/module-c3-catalog-capsule-20261028/`)의 source manifest·도구/입력 identity·stdout/stderr·결과는 보존했고 각 실행의 source/target/ELF는 삭제했다. 실행 전 D2 한정 수용(로컬 `../../artifacts/module-c3-catalog-capsule-20261028/review-before-run.json`)과 이후 독립 검토에서 mode/mark, source active max, CLR 상위 ordinal 계수를 보완했다. 최종 독립 D1/D2 receipt(로컬 `../../artifacts/module-c3-catalog-capsule-20261028/review-final.json`) SHA-256 `4a26d61fef92b3a44b51a9bcd28c4bddea4c8d3d08e3a9020aa70609620337fe`는25/25파일·artifact78개 검토와 필수 수정0의 한정 수용을 기록한다. 문서 verifier·push·무변경 제품/하위 전체 실행은0이다. 기존 stash 및 다른 증거 artifact는 보존한다.

<a id="c3-create-admission"></a>
## 2026-10-28 원 statement의 shared CREATE 제품 예약

기준은 `main` `9f36840`이다. 기존 C2 stash를 적용하지 않고 [shared CREATE 계약](../contracts/dev3-module-boundaries.md#catalog-shared-create)을 구현한다. 동일 기존 reservation에 원 view/source/gate를 묶고 원 private 반환 수명을 분리했다. 처음 설계의 독립 검토는 Reserve 전·진행 중·완료 후 검산 분리, 중복 admission 방지, 효과 후 Fatal, Cancel 관측 보존과 같은 view 재Reserve를 요구했다. 구현 중 독립 검토가 찾아낸 source 예약 전 deep pool health 검사는 얕은 입구로 바꾸고 기존 transaction entry를 예약 뒤에 실행하도록 교정했다.

[source 선택](../../tests/fixtures/dev3-module-c3/create-admission/manifest.json)은 변경 token/CREATE source 식의 새 한도 사례1이다. [후속 실행 선택](../../tests/fixtures/dev3-module-c3/create-admission/execution.json)은 compiler4(원 private 반환 연쇄/생존 view·flush·private 차용 거절)와 신규 CCA01–06을 고정한다. CCA01은 장기 T1 view와 실제 T2 CREATE/Commit/full abort, CCA02는 Before→Cancel→재Reserve, CCA03은 초기 Capacity/foreign owner와 private 반환, CCA04는 active Drop no-I/O fence, CCA05는 실제 Forward/Cancel 뒤 source 반환 오류의 Fatal, CCA06은 새 출력의 현재-build 메모리 복구다.

각 실행 예산은 전체1200초·compiler420초·case45초·출력8MiB·Rust stack32MiB이며 실패 즉시 중단한다. `python3 -B` runner는 source/ELF/파일별 hash·로그·result-before-cleanup.json을 먼저 보존하고 이번 실행의 source/target/ELF만 삭제한 뒤 최종 result.json에 삭제 목록을 남긴다. 기존 artifact·FAIL은 보존한다. 완료한 무변경 사례·native N01·workspace/lower wrapper·문서 verifier·push는 실행하지 않는다.

r1은 신규 source 식의 괄호 오타와 기존 native fixture의 성공 반환 타입 적응 누락으로 compile FAIL(runtime0)이다. r2는 scoped Clippy/test build와 새 source 사례1 PASS다. CREATE carrier2194B, memory Reserve39074/Forward37574/Cancel36690B, native-type40098/37658/37714B이며 고정 progress44304B다. 기존 memory live64000/recovered65376B·native live64136/recovered65512B를 유지한다. 이는 한정 source census이며 전체 floor·stack/RSS·실제 native 실행의 증거가 아니다. r3 이후 compiler/runtime 결과와 최종 독립 검토는 아래에 기록한다.


r3은 static2/compiler4·CCA01–03 PASS 뒤 CCA04의 source charge 기대에서 FAIL했다. active Drop은 먼저 fence하므로 기존 source 반환 검산이 Fenced로 거절하며44304B를 보존한다. 원 FAIL을 보존하고 fixture를 charge보존·claim보존·WAL0·원 gate 해제·후속 health 거절로 교정했다. source를 정상 반환한 것처럼 조작하지 않는다. 독립 검토도 이 적용을 수용했다.

추가 독립 검토로 shared binding의 mode0|1 검산을 입장과 진행 공통 경계에 넣었다. CCA07은 mode/mark 손상을 Reserve/Forward/Cancel 전 각각 검산한다. actor-stage 본문 분리의 추가 참조/Result도 helper source 식에 계수한다. r4은 legacy raw factory를 cfg(test)로 한정하면서 공용 private access helper까지 한정한 compile FAIL(runtime0)이다. 여전히 쓰는 private helper는 production에 두고 raw actor factory만 cfg(test)로 남겼다.

[남은 선택](../../tests/fixtures/dev3-module-c3/create-admission/remaining.json)의 r5은 static2/runtime8 PASS다. 변경한 mode 검산의 정상 경로 CCA01/02와 source식 사례1만 다시 실행했으며, 완료한 compiler4·CCA03은 반복하지 않았다. CCA04–08을 완료했고, CCA08은 기존 admitted read ticket이 끝나기 전 source/WAL 진입0, 원 gate의 늦은 reader 거절과 Cancel 뒤 재개를 검산한다. CCA06은 shared CREATE에서 만든 실제 Commit9/AbortEnd11 output의 현재-build 메모리 복구와 cleanup15/16·Q1/0·Found/Missing을 검산했다. source 한도 관측은 r2와 같으며 실제 native durability 인수로 승격하지 않는다.

[planner 오류 선택](../../tests/fixtures/dev3-module-c3/create-admission/planner-followup.json)은 추가한 CCA09만 실행한다. 초기 Capacity/foreign owner는 planner 전이라 read0이고, 이 사례는 실제 invalid schema planner Before 뒤 read1/private/no-WAL을 검산한 후 같은 view 재Reserve→Cancel의 bits3을 확인한다. r5 완료 사례는 반복하지 않는다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r2 | `26e9f0e45fb13f64643a8426236a35ae0377e1a22c251616ba6e9592c7170e09` | `6146e1c91e1b13b677c7beae86fbc121b73a439a998989184b7821f2fccbe78b` | `c218cd8582af49a78ccd721fab0cebe02231e2e8c44d4129952132178e1a1dfd` |
| r3 | `27dddbf35010a153c9c6bdc0a094ad32e8c0020b30290c804200ad1eb458e4cf` | `2cae15342a633f66a2bcaf20786838e65d577f55b90b0778779b4d4589d79436` | `3cacfc12783274475e9975240611c489ce6d10b2f6d36bd050755fa8e9da6fa2` |
| r5 | `6f23caf5c7d392c2d62a1163b7210d70bd2d68cc2e8beda97059777f8aaf94d9` | `9a9486d90ca6d9403109147eecfc91a1b37f48ec9653467be98567bec722c68c` | `05e018f6b1b7816c24967d3f9241827b9a4b6a2ea2b89875d46300f4dfbb79da` |
| r6 | `ec9e04c3c4b884693188acbabff9acaa0c9605b4768b8bda6e781d18d4d4c9b1` | `f24525d5e37124a07b5599d6aa981d1c9a4ffb553779426acf3f74d6b83699f6` | `f83aee1776d04c0e7ba1ca6740bfe84e8167112770a2c09493c5da4b86a67d4a` |

r6은 static2/CCA09 PASS다. source별 compiler4·신규 runtime9+source 사례1을 충족했다. r3와 r5의 정상 경로 재실행은 mode 검산 변경에 해당하는 CCA01/02, source 재실행은 actor helper 식 변경에 해당한다. 무변경 완료 사례 재실행0이다. r6 뒤 `transaction.rs`의 `record_catalog_read` Rustdoc만 실제 CREATE planner 호출을 반영하도록 교정했고 comment-only delta(로컬 `../../artifacts/module-c3-create-admission-20261028/comment-only-followup.json`)에 원/수정 hash를 남겼다. 구현 변경과 새 시험은 없으며 r6 source를 최종 source로 바꾸어 쓰지 않는다.

r1–r6의 원 FAIL/로그/hash/runner/result-before-cleanup/result는 artifact(로컬 `../../artifacts/module-c3-create-admission-20261028/`)에 보존하고 이번 export/target/ELF만 삭제했다. 최종 독립 D1/D2 검토는 진행 중이다. M4/M5·Row/C4/C5·own-actor 읽기·전체 short floor·native/S5 의무는 미완료다.


최종 source 검토에서 새 factory closure의 큰 tuple Result가 deep Reserve 계수에 명시되지 않은 점을 찾았다. `ReservedParts`를 실제 반환 타입으로 선언하고 closure 반환+factory local Result 각각1280B, 합2560B를 Reserve 식에 추가했다. factory의 반환 항도 실제 `Result<ReservedCatalogCreate,CatalogCreateError>`로 바꿨다. 앞선 r2/r5 관측을 소급 수정하지 않는다.

[carrier/연결 후속 선택](../../tests/fixtures/dev3-module-c3/create-admission/carrier-followup.json)의 r7은 static2/runtime2 PASS다. 수정 source 사례1과 실제 Missing 관측을 넣어 보강한 CCA01만 실행했다. CCA01은 T1 Missing/readbit1 뒤 read operation만 반환하고 같은 long view를 유지한 채 T2 shared CREATE/Commit을 수행한다. T1 Serializable은 실제 epoch 변경으로 Rejected(SerializationConflict), RC는 정상 종료이며 T2 full abort의 경우 정상 종료한다. 원 CCA01의 빈 read-set 검증 범위는 r3/r5 증거로 보존한다.

최종 CREATE Reserve는 memory41634/native-type42658B이며 Forward/Cancel은 r5와 같다. progress44304B, native recovered65512B/여유24B 한도는 유지한다. r7 source `4e1ccff0d574ed797296e2af3635e0d49adab5c3e2cb4424ef71429a481e175a`, ELF `7b85c148a5bc3510dd956e082279610058040e76639b8eee9a8e8f6f27faea5d`, result `c5e0cc83e7691791735e63013c82729ab8c1e1baa37d958473634cc9c02e26a7`다. r7에는 앞선 Rustdoc 교정도 포함되어 있고 현재 source와 일치한다. 전체 검토 완료 판정은 아래에 기록한다.


최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, 필수 수정0이다. `/root/source_admission_review`가 변경33/33파일과 artifact101개를 대조하고 현재 source=r7, 원 FAIL/로그 보존, 임시 build/export/ELF 삭제를 확인했다. 검토자 실행0이며 최종 receipt(로컬 `../../artifacts/module-c3-create-admission-20261028/review-final.json`) SHA-256은 `d3896261d69ed6d7fbec76665329c735c63e3ac9d989d13f628343c2a89802cd`다. 허용된 receipt/status/hash metadata만 후속 반영했다. 이번 shared CREATE 단위를 한정 수용하며 전체 M4/M5는 미완료다.


<a id="c3-own-catalog-read"></a>
## 2026-10-28 원 writable statement의 Catalog own 조회

기준은 `main` `830d053`이며 stash는 적용하지 않았다. [owner 계약](../contracts/dev3-module-boundaries.md#catalog-own-read)에 따라 기존 call의 실제 view를 lookup에 전달하고 실제 Status head 및 CREATE Undo 검산을 거쳐 자기 쓰기를 읽는다. 독립 설계 검토는 Profile2 첫 CREATE 한정으로 수용했다. raw actor/Status 관측값을 별도 권한으로 공개하지 않는다.

실행 전 [manifest](../../tests/fixtures/dev3-module-c3/own-catalog/manifest.json)는 새 source1, OCR01–03와 변경 WT01만 선택했다. OCR01은 SER/RC×Full/Resident×Cancel 유무8조합으로 own Found/Q0, Reserve gate, 타인 Active Missing, Commit 뒤 원 statement Missing, 다음 RC statement Found/SER Missing·conflict를 검산한다. OCR02는 미등록 Forward/revision0와 mode/mark/원 snapshot 형태 오류, OCR03은 실제 Status frame의 head/state 모순을 검산한다. WT01은 writable Unsupported 기대를 실제 빈 Missing/readbit로 변경했다. 기존 완료 회귀·복구·native 사례는 반복하지 않는다. 새 WAL/형식/복구 코드는 없으며 앞선 current-build 증거는 원 source 범위로 보존한다.

전체1200초/compiler420초/case45초/출력8MiB/Rust stack32MiB이며 실패 즉시 중단한다. artifact(로컬 `../../artifacts/module-c3-own-catalog-20261028/`)의 runner는 source/ELF/hash/log/result-before-cleanup을 기록하고 이번 실행의 source/target/ELF만 삭제한 뒤 최종 result를 보존한다. 문서 검증·push는 실행하지 않는다.

초기 formatter는 새 fixture module의 path 속성 누락으로 FAIL했고 기록을 보존했다(시험0). r1은 private field getter 호출·fixture import/미사용 변수·서로 다른 invariant snapshot lifetime의 typed 포인터 비교로 compile FAIL(runtime0)이었다. 실제 두 guard 참조의 주소만 안전하게 type-erased 비교하도록 고쳤으며 raw pointer dereference/새 권한은 없다. 원 r1 source/로그/FAIL을 보존한다. 후속 실행과 최종 검토는 아래에 추가한다.


r2은 scoped Clippy/build와 선택5개(source1/OCR01–03/변경WT01)를 통과했다. source `57de1262730057847299abfbfcc49b5ab241872da48e889abea5650b7918e13a`, ELF `6c8faa5182e39971b4275cd880595e36be91a23bcaf82202915db161acd42e0c`, result `ce2ea48a939d887781e747a4f6c5149e1d51f6213d0b4b7755f94af5b6a542fe`다. memory Full34230B·Resident17880B·view 검산6072B, native-type Full35254B이며 memory live64000/recovered65376B·native-type live64136/recovered65512B를 관측했다. 전체 short floor·stack/RSS·native 실행 인수가 아니다.

[Undo 후속 선택](../../tests/fixtures/dev3-module-c3/own-catalog/undo-followup.json)은 신규 OCR04만 실행한다. 실제 원 Undo frame의 canonical target entity/creator generation을 바꾸고 page digest를 다시 써서 own 경로가 실제 Undo 검산을 건너뛰지 않는지 확인한다. r2 완료 사례는 반복하지 않는다. 독립 source 검토는 own 검산 중 남는 Status Result carrier를 명시 가산하도록 요청했으며 그 식 변경은 별도 source 선택으로 검증한다.


r3은 static2 PASS 뒤 OCR04 FAIL이다. target entity 변조는 거절됐지만 creator generation을 상수2로 대입한 fixture가 실제 발급 generation2와 같아 음성 입력이 되지 않았다. 관측한 원 양수 generation/entity에1을 더해 실제 변경을 보장하도록 fixture를 수정한다. 제품 조건·정답 기준을 완화하지 않고 원 FAIL을 보존한다. [source/Undo 후속 선택](../../tests/fixtures/dev3-module-c3/own-catalog/source-undo-followup.json)은 변경한 Status Result carrier 식의 source1과 실패 OCR04만 포함한다.


r4은 scoped static2와 변경 source1·수정 OCR04를 통과했다. source `0a9ded37fbe09ccc9a9fe09bdb43396b7199dfd325c415d5900cfde5b29d2cc8`, ELF `cbbfec097565cf43a2d89a1f7adcab5b471f6e8c8ff6e49d0fc41239e04a7c09`, result `84c248523e41d61ac4f1899cab056392ae7229f01eccbf41f213353d235dfb33`다. r2의 OCR01–03/WT01은 반복하지 않았다. source별 고유 runtime6(source1+새 OCR01–04+변경 WT01)를 충족했고 source 식 변경만 r4에서 다시 검증했다. 모든 실행의 자체 source/target/ELF는 삭제하고 원 FAIL/로그/hash/result를 보존했다. 최종 독립 D1/D2 판정은 아래에 기록한다.


최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, 필수 수정0이다. `/root/source_admission_review`가19파일(OCR 제외8 포함)·artifact54개를 검토하고 current source=r4와 원 FAIL·결과·삭제 기록을 확인했다. r4 view 검산은6424B이며 Full/Resident·live/recovered peak는 r2와 같다. 최종 receipt(로컬 `../../artifacts/module-c3-own-catalog-20261028/review-final.json`) SHA-256은 `d15f122567f4e5b3be0780520844e0dc5cd037b1a87a2c5d466c5c54723c91ce`다. 검토자 시험·빌드·제품 수정0이며 허용된 status/receipt/hash metadata만 후속 반영했다.

사용자의 후속 “일단 문제만 해결해줘요, 문제해결 후 작업을 개시하지는 말아요” 지시에 따라 현재 Catalog 조회 수정의 검증·리뷰·기록을 마무리하고 이후 개발은 개시하지 않는다. 전체 M4/M5를 완료로 표시하지 않는다.


<a id="row-actor-graph"></a>
## 2026-10-28 main에서 Row actor의 실제 페이지 관계 검산 재개

사용자의 계획 전체 완료·단계별 commit 요청에 따라 clean `main` `d27bfa4`에서 재개했다. 이전 Catalog own-read 수정 후 중지 기록은 당시 지시로 보존하며 C2 stash는 적용하지 않았다. M0–M3와 기존 source별 검증은 반복하지 않는다. 다음 실제 Row 공급을 위해 [한 actor graph 계약](../contracts/dev3-module-consumers.md#row-actor-graph)을 구현한다. 전체 M4/M5 목표와 C2/C3/C4/C5·A/B 단독/합류 인수 의무는 유지한다.

사전 독립 D2는 한 actor의 원 Root/64 Job census·Status/Undo/Row 양방향 관계에 조건부 수용했다. 전체 Base4 closure/READY나 live 권한으로 사용하지 않는다. [선택 manifest](../../tests/fixtures/module-row-format/actor/manifest.json)는 신규6 selector를 원 diff에 연결하며, 다중 statement/중간 rollback mark/compensated+Active·Commit/prune·RootFree/Reserved와 chain/count/identity/schema/projection/orphan 음성 입력을 검산한다. 전체1200초/compiler420초/case45초/출력8MiB/stack32MiB이며 실패 즉시 중단한다. 같은 코드의 완료 시험·workspace/하위 wrapper·native·문서 verifier·push는 실행하지 않는다. 새 영속 write/형식/current dispatch 변경은 없으며 native/current-build durability 인수는 후속 실제 공급에 남긴다.

artifact(로컬 `../../artifacts/module-row-actor-20261028/`)의 runner는 각 실행 source/hash/ELF/로그/result-before-cleanup을 보존하고 그 실행의 source/target/ELF만 삭제한 뒤 최종 result를 남긴다. r1은 새 test helper의 mutable slice·Box coercion과 새 코드의 Clippy collapsible_if로 compile FAIL(runtime0)이다. r2는 static2 PASS 뒤 독립 raw fixture의 CatalogLeaf kind7 오기로 첫 literal raw-layout FAIL(runtime0 PASS)이다. 실제 계약값5로 고쳤다. 별도 source 검토가 발견한 unconditional statement credit 인자도 원 structural 검산 API로 분리했다. 앞선 FAIL은 보존하고 r3에서 변경/미실행6만 선택한다. 후속 실제 결과와 최종 독립 검토는 아래에 기록한다.


r3은 scoped static2와 최초 신규 runtime6을 통과했다. 독립 리뷰가 지적한 RootFree 뒤 RowId 상한·유일성 및 raw tag0 뒤 비정규 Free body를 추가로 교정했다. [리뷰 후속 선택](../../tests/fixtures/module-row-format/actor/review-followup.json)의 r4는 해당 변경을 지나는 신규3개만 통과했고 r3 완료6개는 반복하지 않았다. 해당 actor의 RowId는 Root.row_high 이하이며 해당 actor의 현재 행끼리 table/leaf를 가로질러 RowId·logical mutation이 중복하지 않는다. 다른 actor와의 전역 유일성은 전체 closure 의무다. allocated Undo의 Free 분기는 tag만 확인하지 않고 canonical body까지 검산한다.

r4 뒤 제품 변경 없이 추가한 [같은 leaf 후속 선택](../../tests/fixtures/module-row-format/actor/same-leaf-followup.json)의 r5는 신규1개를 통과했다. 같은 leaf의 서로 다른 두 행은 허용하고 같은 logical mutation으로 바꾸면 거절한다. 현재 고유 runtime selector10개는 r3의6·r4의3·r5의1이라는 source별 증거다. r3의 `actor_rejects_foreign_actor_and_wrong_status_location`은 실제 내용에 맞게 현재 `actor_rejects_foreign_actor_and_orphan_row`로 이름만 고쳤으며 원 실행명·로그를 보존한다. 최종 source에서 전체10개를 재실행한 것처럼 표시하지 않는다.

fixture author는 제품 encoder/다른 fixture author를 import하지 않는 독립 정수·SHA 조립이다. r4에18page record와 두 table RootFree 사례를 추가했고 기존10사례의 원16page images를 유지했다. fixture의 descriptor/inventory/history는 opaque 부분이므로 전체 Base4/current recovery oracle로 사용하지 않는다. 새 같은-leaf 사례는 독립 원 row bytes에 기존 leaf envelope만 적용한다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r3 | `b85420544dc079d30848f8c0cedaacf9cc6707bfc38c7ea4c60d17d34ab92bf1` | `93d01d5bc389d4d9900bf0e6f693c9353a59960f82eac25d134f17744cd20a79` | `2159b0e7cc7d95ce2b3ea953b2b939d9fe66a43c4b604f8fad7b18030eff0fac` |
| r4 | `737be0986ca9bf0a6b14551cc6578c1dd651a5e3fea4a5fa4d5052d95246c946` | `af6785bae11820387754f41ae58204fad653d175309ba161554c6d44b017ebaf` | `edc389d39c7b8f3d036eddb6c8f8359d7728373ac578f232b9feda4bff9377a0` |
| r5 | `5e18cc410e34b6df4b5b982b3f92abc4d3940432e47a8a5483616a4b0232c828` | `ae71e87853c047f907ec2b5564b37b4eff0dd940f2767be2dbb306ab06d71bbd` | `8816b06fb68a3a9429aeed6f404d335c8117a81b93bbe48b37f27bd631557ec9` |

각 source의 전체 파일별 hash inventory·선택 manifest·도구 identity·stdout/stderr·원 FAIL·result-before-cleanup/result를 artifact에 보존했다. r1–r5의 자체 source/target/ELF는 모두 삭제했다. 무변경 완료 사례·전체 workspace·하위 wrapper·native·문서 verifier·push 실행0이다. caller 원4page 외 별도 page bank는 만들지 않지만 typed value/return/caller의 전체 동시 source census·admission은 아직 공급하지 않았다. 이를 F05/64KiB aggregate나 native/current-build durability PASS로 표시하지 않는다. 독립 최종 D1/D2 검토 결과는 아래에 기록한다.


최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, 필수 수정0이다. `/root/boundary_review`가 변경17파일·artifact64파일의 원 bytes/hash/로그/결과와 정리 기록을 대조했다. 제품/fixture 관련10파일은 r5 source와 같고, 검토자는 수정·빌드·시험·문서 verifier를 실행하지 않았다. 최종 receipt(로컬 `../../artifacts/module-row-actor-20261028/review-final.json`) SHA-256은 `24132533000b2ed5f8a7dafcf31428a39a857c6aac47791681fb4c30db02b7b5`다. 허용된 수용/status·receipt/hash metadata만 후속 기입하며 전체 mixed graph·descriptor/inventory/history·statement/source admission·Profile3 실제 연결·M4/M5는 미완료다.


<a id="row-scope3-wire"></a>
## 2026-10-28 Row scope3 frame의 독립 입력 검산

기준은 `main` `2fdff66`이다. [scope3 wire 계약](../contracts/dev3-module-consumers.md#row-scope3-wire)의 별도 입구를 구현한다. 독립 D2 설계 검토는 다중 statement core 표와 별도 scope3 선택을 수용했고, Forward/CLR/Prune의 nextH↔E=3+h, StatementFinish의 XSG/Tx owner, kind1/3/5/8/9와 정확 Patch span을 요구해 구현에 반영했다. current-generation은 실제 Status/descriptor의 의미 검산 의무이며 header TxnId만으로 원 TxnRef generation을 인증하지 않는다.

[선택 manifest](../../tests/fixtures/module-row-format/wire/manifest.json)는 신규8개 selector만 고정한다. 독립 정수/SHA author의30 literal은14opcode 전체와 C185/2228·h0/1, full/statement RollbackStart, MNone/Some StatementFinish, 마지막/중간 CLR를 포함한다. descriptor/native/auxiliary는 의도한 opaque sentinel이며 성공 의미는 `WIRE_ONLY`다. body 의미·원 stable before·native/redo after equality·history·source/admission·current Profile3 전환을 공급한 것으로 기록하지 않는다. 음성 입력은 outer checksum을 다시 닫은 뒤 scope/op/owner/core/section/native/redo의 해당 오류를 검산한다. 기존 Catalog/Row 완료시험은 반복하지 않는다.

전체1200초/compiler420초/case45초/출력8MiB/stack32MiB, 실패 즉시 중단이다. artifact(로컬 `../../artifacts/module-row-wire-20261028/`) runner는 원 source/파일별hash/ELF/로그/result-before-cleanup을 먼저 보존하고 이번 export/target/ELF만 삭제한다. 원 실패와 source별 결과를 보존하며 전체 workspace/lower wrapper/native·문서 verifier·push는0이다. 실제 실행 결과와 최종 독립 D1/D2는 아래에 기록한다.


r1은 scoped static2와 신규8개를 모두 PASS했다. source `a1781b10a84d2e21903dccb258093b65b874345e54eeb5424a4164e1a9b5f9f8`, ELF `513fddc60481d3fedc61daa880e6a5cc7bf39f653a35c2a924f74eb6fe37ea96`, result `67363558c491756b3ca2bda89aafd3e0bb95ef09580942c1fb068ff1e1eccf9a`다. 독립 구현 리뷰가 Row slot60000도 허용하는 누락을 찾아 원 MAX_PAGE_SLOTS와 최소 directory 종단을 추가했다. 작성자 계약 대조로 유효 envelope의 Profile/dialect/op/outer 오류를 Unsupported로 분리했다. [후속 선택](../../tests/fixtures/module-row-format/wire/review-followup.json)은 두 변경의 신규2개만 실행한다. 앞선8개는 원 source PASS로 보존하며 최종 source 전수 재실행으로 합치지 않는다. slot 경계 정상/거절과 Prune 양쪽, 오류 종류와 op2/op5 정상 인접 사례를 이 신규2개에서 검산한다.


r2도 static2와 신규2개 PASS다. source `df5007ff917fbc23186dc5b3bcef4ad87d9c50673e93c18ef02670e8b6642894`, ELF `afe4589da505fb64782f33115c9b7028f515d7426cfa0fa99fbcc1f47d0fa74e`, result `47dc663f7774c527fd174a222aeda6777bc90a6e51477f141c666070fba6c611`을 보존한다. 고유10개는 r1 source8+r2 source2이며 원 r1 이후 동일 완료 사례 재실행0이다. 두 실행의 임시 source/target/ELF는 결과·identity 수집 뒤 삭제했다. 원 compiler/runtime 로그·source inventory·manifest/runner·result는 보존하며 최종 source의 전체 의미 graph/current recovery 성공으로 승격하지 않는다. 최종 독립 D1/D2 receipt는 아래에 기록한다.


최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, 필수 수정0이다. `/root/boundary_review`가 변경18파일과 artifact39파일을 원 source/독립 literal/실행 결과·hash/cleanup에 대조했다. 제품·fixture11파일은 r2 source와 같으며 검토자 수정·빌드·시험·문서 verifier·author 실행0이다. 최종 receipt(로컬 `../../artifacts/module-row-wire-20261028/review-final.json`) SHA-256은 `432f1972b9df0c46d83f0a46480c788565ce26dc29a2319d50183816eb23f7de`다. 허용된 status·수용·receipt/hash metadata만 후속 기입한다. 전체 semantic graph/descriptor/history·native/redo 의미 일치·source admission·current dispatch/live/recovery·M4/M5는 계속 미완료다.

<a id="row-status-images"></a>
## 2026-10-29 Row Status 실제 before/after와 frame 결합

기준은 `main` `b50e734`이며 C2 stash와 다른 worktree를 건드리지 않았다. [Status 국소 계약](../contracts/dev3-module-consumers.md#row-status-images)은 원 Profile3/actor/StatusSlotRef·외부 statement/mark·Commit/Prune q를 actual pages와 결합한다. 별도 reviewer `/root/boundary_review`의 사전 D2 한정 수용에 따라 전체 graph를 claim하지 않는 국소 Status 관측기를 구현했다. 실제 kind5 native의 양쪽 cell/offset·redo target/priorLSN/span·patch 밖 불변성과 Status의9개 다중 head/count 전이를 검사한다. 새16KiB page 후보나 local bank 없이 immutable before/after와 작은 scalar만 소비한다. 전체 source/caller census·actual admission 수신 전이며 수치 상계 검증을 완료했다고 주장하지 않는다.

[독립 입력](../../tests/fixtures/module-row-format/status/input-plan.json)은 원시 Status24쌍을 explicit before/after로 작성했다. 새 Python author는 제품 encoder/transition을 호출하지 않고 정수/SHA로 cell/page를 만든다. 보존한 wire author의 envelope recipe만 재사용하며 그 source hash도 고정했다. 다른 native·descriptor·auxiliary는 semantic-invalid sentinel이고 성공 범위는 `STATUS_LOCAL_ONLY`다. 실제 Undo U/H/Job/mark ancestry·전체 census·발급 q/horizon/pin·source 예약·WAL stage/install·current Profile3/live/recovery·M4/M5 인수는 미완료다.

실행 전 [선택 manifest](../../tests/fixtures/module-row-format/status/manifest.json)에 신규8개만 고정했다. 예산은 총1200초/compiler420초/case45초/출력8MiB/stack32MiB이고 실패 즉시 중단이다. r1의 scoped Clippy/test-build2와 신규 runtime8은 모두 PASS했다. 이 시험은 새로운 Status 관측기만 대상으로 하며 원 wire10/actor10·workspace/lower wrapper·완료 회귀를 반복하지 않았다. statement markNone/Some·마지막 CLR·잔여head의 Finish·statement→full escalation·full CLR의 이전 statement 번호, count overflow/불일치, actor/page/slot generation·owner·native/redo·LSN 반례를 포함한다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 | `164b6d24330b92e20a43d3da5e6c83705a1197b8172fa33c23d180028f842b2f` | `47d813f5bd2045f4c1d25c07d3f57d09f31d1d2cb6ce50dfb340ac00e928ea42` | `dceba1945dcd313342a394c95b4e682a4af5cba5c359dafb4a45877b4be0da1e` |

실행 artifact(로컬 `../../artifacts/module-row-status-20261029/`)에 compiler/runtime 원 로그·runner/manifest hash·파일별source inventory·result-before-cleanup/result를 보존하고 이번 export/target/ELF는 삭제했다. native process 인수·문서 verifier·push는 실행하지 않았다. 최종 독립 D1/D2의 실제 결과는 아래에 기록한다.


독립 리뷰는 r1의 일부 tag 변조가 canonical decoder에서 먼저 거절되어 전이 guard 검산을 대체하지 못함을 지적했다. [후속 선택](../../tests/fixtures/module-row-format/status/review-followup.json)으로 정식 source상태10개가 lower Status decoder를 통과한 뒤 각 opcode의 금지 전이에서 exact OrderMismatch인 신규1개와, 유효 non-Status5opcode·외부 Profile1/2의 exact Unsupported 신규1개를 추가했다. 제품 코드는 바꾸지 않았고 원8개는 반복하지 않았다. r2의 static2·신규2개도 PASS이며 source `c708d0cc89148ce99f4bdd0f2a1c66a3a453c3fc69d6a645ecf66e24c5a86f75`, ELF `3cd102a0162169b9cb9e81b724b7a17111bd1c04d57b187ec99f146abb541e49`, result `27335e01046342dddd3d3f0adc4cb4d97adae34aa5d3fc77ac97055aff1ee87f`다. 고유10개는 r1 source8+r2 source2이며 최종 source에서 전체10개를 반복한 증거가 아니다. 두 실행의 source/target/ELF는 제거했고 원 결과·로그·hash를 보존했다.

원 r1의 attempted full→statement 변조에서 offset16192는 writer_epoch이고 실제 next 영역은16312..16352다. 그 원 입력은 정식 상태 전이를 증명하지 않으며 decoder/identity 거절 사례로만 보존한다. 신규 canonical matrix가 별도 lower decode 성공과 exact 전이 오류를 검산한다.


최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, 필수 수정0이다. `/root/boundary_review`가 고정 변경16파일과 artifact39파일의 원 source/bytes/hash·선택 로그·manifest/runner·cleanup을 대조했고 불일치0이다. 현재 제품/fixture는 r2 source inventory와 같으며 검토자는 수정·빌드·시험·문서 verifier·author를 실행하지 않았다. 고정 검토 입력(로컬 `../../artifacts/module-row-status-20261029/review-input.json`) SHA-256 `1bde2c3e3fa9cfdf4aaab67671fa49a133bafce55e41c884233c902ed3f43ce1`, 최종 receipt(로컬 `../../artifacts/module-row-status-20261029/review-final.json`) SHA-256 `10cdc6052329d4ff121d92d366a07b89ca55cdf7106bacf8e146cccda29b91fb`를 보존한다. 허용된 status/수용/receipt/hash metadata만 후속 반영한다. 전체 graph·source admission·실제 live/recovery·M4/M5는 미완료다.


<a id="row-undo-images"></a>
## 2026-10-29 Row Undo·원 Job·같은 frame Status의 관계 검산

기준은 `main` `09173aa`이며 원 [Undo 국소 계약](../contracts/dev3-module-consumers.md#row-undo-images)을 구체화했다. `/root/boundary_review`의 사전 D2 의견을 반영하여 Reserve의 독립 계획값과 실제 Free before를 분리하고, Existing은 원64 JobCells/외부 ordinal에서 selected Job을 얻는다. native9의 before bytes/slot/offset/redo page와 원 selected 위치를 결합한다. U material과 가장 가까운 Retained H를 원 bank에서 고르고 H의 자기 table/RowId·실제 Live Missing/incoming2 page를 검산한다. H 자체 prevTxn의 bank 관계도 확인하되 다음 실제 page는 로드하지 않는다. Forward/CLR/Prune는 같은 LayoutView에서 기존 Status observer를 호출한다. native3/redo/실제 U before/after와 Reserve·Forward·CLR·Prune·ReleaseRole·RootFree의6개 전이를 검사한다.

[독립 입력](../../tests/fixtures/module-row-format/undo/input-plan.json)은 정수/SHA와 명시한 before/after 상태표의 raw17쌍이다. 제품 encoder/transition은 호출하지 않고 원 wire/ledger/status author의 낮은 raw byte recipe만 재사용하며 그 source hash를 남긴다. 다른 frame role/descriptor/auxiliary는 semantic-invalid sentinel이고 성공 범위는 `UNDO_LOCAL_ONLY`와 세 opcode의 같은 frame Status 통합이다. 전체 Row·after Job·Root commitment/census·descriptor/edge/count/inventory 의미·mark ancestry·q 발급/horizon/pin·source 상계/admission·current Profile3/live/recovery·M4/M5는 미완료다. A의 Row/CLR 후보 생산을 C가 대신 구현하지 않는다.

실행 전에 [manifest](../../tests/fixtures/module-row-format/undo/manifest.json)로 신규9개를 고정했다. 예산 총1200초/compiler420초/case45초/출력8MiB/stack32MiB, 첫 실패 즉시 중단이다. 초기 작성 중 rustfmt가 닫는 괄호 누락을 보고했고 compiler/runtime 실행 전 교정했다. 준비 실패 기록(로컬 `../../artifacts/module-row-undo-20261029/preparation-failure.txt`)을 보존하며 실행 시험 FAIL로 바꾸거나 숨기지 않는다. r1은 scoped static2·신규 runtime9 모두 PASS다. 원 wire/actor/Status 완료 시험 및 workspace/lower wrapper는 반복하지 않았다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 | `2e9ed00d9d065234a42383ee5b208b9e151da95e63f9185bd2f6f588b686caf2` | `da4114d28d0a1129a5edfc24003b7916c82b2d467026dd63385b6da16ad534e2` | `49b0d88ec3e016fb33762dff24c329c57943f0ad292112b6e976e08ca4316a5a` |

artifact(로컬 `../../artifacts/module-row-undo-20261029/`)에 원 compiler/runtime 로그·source별 파일 hash inventory·runner/manifest identity·result-before-cleanup/result를 보존하고 이번 source export/target/ELF는 삭제했다. 실제 native/복구 인수·문서 verifier·push는 실행하지 않았다. 최종 독립 D1/D2 결과는 아래에 기록한다.


최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, 필수 수정0이다. `/root/boundary_review`가 제품·fixture와 source별 실행 증거를 검토하고, 최종 분리한 index15파일·artifact30파일의 hash 불일치0을 확인했다. 고정 소유 입력(로컬 `../../artifacts/module-row-undo-20261029/review-input-owned.json`) SHA-256 `aae5bf8dcc986f7f42fd37cfef4a3af2804d08d918b8df8d4cc2606ebbde99d1`, 최종 receipt(로컬 `../../artifacts/module-row-undo-20261029/review-final.json`) SHA-256 `2b68fbd1d8ad0f97d04b32c65eafb3c2ff24dd7ba355ac628fef6bb7207c2bba`를 보존한다. 원17파일 입력은 동시 문서 변경 발견 기록이며 이번 commit 범위가 아니다. 검토 후 별도 담당자가 독립성 평가 gate 계획4문서를 `cbff2fd`로 commit했으며, 공유2파일의 Undo 추가2줄과 나머지13파일은 검토 때와 같다. 이번에는 그 위에 소유 변경15파일만 선택한다. 제품·fixture는 r1 source와 같으며 검토자 수정·빌드·시험·문서 verifier·author 실행0이다. 허용된 수용/status/receipt/hash metadata만 후속 반영한다. 전체 graph·source admission·current Profile3/live/recovery·M4/M5는 미완료다.

<a id="post-separation-gate-push-20261029"></a>
## 후속 독립성 평가 계획의 push 문서 정정 — 2026-10-29

사용자가 저장한 후속 계획의 commit/push와 발견된 문서 결함 수정을 요청했다. `cbff2fd` 전송의 원 훅은 Rust 포맷·Clippy를 통과했지만 이 문서의 로컬 artifact 링크16개가 commit export에 없어 차단했다. 원 실패 로그는 로컬 `artifacts/module-independence-gate-push-20261029/push-1.log`에 보존한다. 각 원 파일·디렉터리가 로컬에 있고 `/artifacts/`는 Git 제외 대상임을 확인했다. 검사를 진행하는 동안 다른 작업이 별도 commit한 `f3c3611`의 같은 종류4개를 포함하여 총20개를 링크 대신 로컬 경로 표기로 정정했다. 원 경로·receipt hash·실행 결과·FAIL과 실제 artifact bytes는 유지하며, 원자료가 원격 저장소에 포함된다는 주장을 하지 않는다. 제품·검사기·판정 기준 변경이 없는 D0 정정이며 후속 push는 수정 commit에 기존 훅을 적용한다.


<a id="row-leaf-images"></a>
## 2026-10-29 실제 RowLeaf·원 schema/Job·같은 frame Undo/Status 검산

기준 `main` `7baef05`에서 [국소 RowLeaf 계약](../contracts/dev3-module-consumers.md#row-leaf-images)을 구현했다. 독립 사전 D2의 full ObjectHandle·retired/anchor·physical alias·metadata slot0·원 seal 출처·same-frame 조건을 반영했다. 현재 anchor를 따라 원 Job의 오래된 hint와 다른 leaf도 허용하며, 모든 실제 행의 schema/identity와 비선택 raw cells 보존을 검사한다. Forward/CLR/Prune의 actual Row effect와 native/redo/core projection을 같은 frame의 Undo/Status에 결합한다. 전체 graph/current dispatch/live/recovery·source admission·M4/M5는 미완료다.

[독립 입력](../../tests/fixtures/module-row-format/leaf/input-plan.json)은 정수/SHA와 명시적 schema/typed values로 만든 raw15쌍이다. 처음/중간/끝 삽입·중간 삭제/Prune·canonical5/2048B·혼합32열을 포함한다. 보존한 wire의 raw framing recipe와 원 Undo pairs의 actual local role bytes만 재사용하고 제품 encoder/transition은 호출하지 않는다. Root/afterJob/descriptor/auxiliary는 invalid opaque sentinel로 남으며 성공 범위는 `LEAF_LOCAL_WITH_UNDO_STATUS`다. 준비 중 Prune literal q7이 원 Status q9와 다른 것을 첫 실행 전에 찾아 q9로 교정했고 원 준비 산출물은 `artifacts/module-row-leaf-20261029/preparation-v1/`에 hash와 함께 보존한다. 실행 FAIL/PASS로 바꾸지 않는다.

[선택 manifest](../../tests/fixtures/module-row-format/leaf/manifest.json)는 신규8개만 고정한다. 전체1200초/compiler420초/case45초/출력8MiB/stack32MiB·첫 실패 중단이며, 이번 source export/target/ELF는 원 로그/결과/파일별 hash를 먼저 보존한 뒤 삭제한다. 기존 완료 시험·workspace/lower wrapper·문서 verifier·push는 실행하지 않는다. 실제 결과와 최종 독립 검토는 아래에 기록한다.


r1의 scoped static2·앞4개 선택 시험은 PASS이며, 다섯 번째 시험은 반례 helper가 core digest를 수정한 직후 frame checksum을 갱신하기 전에 operation을 다시 decode하여 `Wal(Corrupt)`로 FAIL했다. 제품 거절 결과가 아니라 시험 준비 경로 실패다. 원 r1 로그/결과를 보존하고 helper가 수정 전에 원 operation을 읽도록 고쳤다. [후속 선택](../../tests/fixtures/module-row-format/leaf/review-followup.json)은 수정한 실패1개와 미실행3개만 고정한다. 완료한 앞4개는 재실행하지 않으며 final source 전수 PASS로 합치지 않는다. 제품 소스는 r1과 같다.


r2의 scoped static2와 선택4개는 모두 PASS다. 고유8개의 source별 증거는 r1의 앞4개+r2의 실패 교정1개/미실행3개이며, r1의 FAIL을 보존하고 final source 전체8개 재실행으로 표시하지 않는다. r1→r2 source 차이는 `leaf_tests.rs`와 `review-followup.json`뿐이다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 | `9871d5074d9ba4ebb45d156055725fa8a250a28c15b7676d3b45b3e9a6b49009` | `eedec7ee7818c0c588970d2f6a852c3dd7e6f3df11c3a84f11115c8621fd7730` | `433a7367fbf616b08e14a18731764fcd374c344eedc9ac3805039d019e7381d9` |
| r2 | `ae7124c18b078a99782ee4bea7f4e12acdb49ccc6e6d7823ae983975f551944d` | `393ec5621b609d14f7430c084157eaa71d685848c81858d1bf36e1b89468291b` | `f83e1d3db8c204bb1945c3d0c8faacff2f173df9f5777117ee2d414ffa334411` |

원 compiler/runtime 로그·source별 파일 hash inventory·runner/manifest·결과를 `artifacts/module-row-leaf-20261029/`에 보존했고 각 실행의 source export/target/ELF는 삭제했다. 실제 native/복구 인수·문서 verifier·push는0이다. 최종 독립 D1/D2 결과는 아래에 기록하며 전체 M4/M5는 미완료다.


최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, 필수 수정0이다. `/root/boundary_review`가 고정 변경16파일·artifact41파일의 실제 bytes/hash, 원15쌍 index와 source별 결과를 대조했고 누락·불일치0이다. r1/r2 inventory5870/5871개와 helper 변경 역구성·양쪽 source fingerprint 재산출도 일치했다. 입력 `artifacts/module-row-leaf-20261029/review-input.json` SHA-256 `28499a3508096464982aa6ea471c7fcf4f3ffe6cf0e1b55796084b4f466e756d`, receipt `artifacts/module-row-leaf-20261029/review-final.json` SHA-256 `f556fd93f309cb40227fc610aca70649bdbe384e0379962007a07e3b48fdd5f7`를 보존한다. 검토자 수정·빌드·시험·author·문서 verifier 실행0이며 후속 수용/status/receipt/hash metadata만 반영한다. 전체 graph·source admission·current Profile3/live/recovery·M4/M5는 계속 미완료다.

<a id="row-job-images"></a>
## 2026-10-29 실제 Job page·원64 bank·국소 전이 검산

기준 `main` `21592e9`에서 중단했던 Job 초안을 이어 [국소 Job 계약](../contracts/dev3-module-consumers.md#row-job-images)을 구현했다. C2 stash와 다른 worktree는 유지한다. 실제 selected Jobs before/after page·원64 bank·kind9 native/redo를 연결하고11opcode의 local state/generation/material·sender SHA·actor terminal/root 요구를 검산한다. 사전 독립 D2의 LSN0 genesis·full generation·RowId 보존·양쪽 sender binding을 반영했다. 첫 구현 리뷰에서 F/CLR/Prune Job C/h와 같은 frame shape의 결합 누락을 발견해 첫 실행 전에 교정하고 별도 신규 반례를 포함했다.

[독립 입력](../../tests/fixtures/module-row-format/jobs/input-plan.json)은 정수/SHA 명시적 전이표의 raw26쌍이다. 원 wire/ledger의 raw recipe 및 Job3 literal source hash를 보존하고 제품 encoder/transition은 호출하지 않는다. slot14/15/63·first Begin LSN0·generation MAX−1→MAX·C185/2228·h0/1·세 release cursor·Committed/Aborted와 Compensated q0 공존을 포함한다. sender descriptor1536B와 다른 frame 역할은 의도한 semantic-invalid opaque 입력이며 성공 범위는 `JOB_LOCAL_ONLY`다. descriptor 의미/잔여·Root commitment/전체 graph·source admission·current Profile3/live/recovery·M4/M5 인수로 확대하지 않는다.

실행 전 [선택 manifest](../../tests/fixtures/module-row-format/jobs/manifest.json)에 변경 Job 코드의 신규10개만 고정했다. 총1200초/compiler420초/case45초/출력8MiB/stack32MiB·첫 실패 중단이다. scoped Clippy와 test-build는 정적 검사이며 runtime은 exact selector만 실행한다. 기존 wire/actor/Status/Undo/RowLeaf 완료 시험과 workspace/lower wrapper는 반복하지 않는다. 결과·로그·파일별source hash·ELF hash를 먼저 보존하고 이번 export/target/ELF만 삭제한다. 실제 native/복구 인수·문서 verifier·push는 실행하지 않는다. 실제 결과와 최종 독립 D1/D2는 아래에 기록한다.

r1의 scoped static2와 앞6개 신규 시험은 PASS다. 일곱 번째 실제 page/native 시험은 반례 offset16000+Job1024가 page끝16352를 넘어서 새 observer 전에 wire decoder에서 Corrupt를 반환하여 FAIL했다. 원 로그·source inventory·FAIL을 보존한다. 반례 offset을 wire-valid14000으로 고치고, 같은 함수의 actual before pageLSN101 음성 입력은 redo prior100을 유지해 wire 검사를 통과한 뒤 page 대조에 도달하도록 교정했다. 제품 코드는 그대로다. [후속 선택](../../tests/fixtures/module-row-format/jobs/review-followup.json)은 수정한 실패1개와 미실행3개만 실행하며, 앞6개를 반복하거나 최종 source 전수 PASS로 합치지 않는다.


r2의 scoped static2와 선택4개는 모두 PASS다. 고유10개는 r1 source의 앞6개와 r2 source의 수정1개/미실행3개로 나뉘며, 원 FAIL을 보존하고 final source 전체10개 재실행으로 표시하지 않는다. r1→r2 source inventory 차이는 `jobs_tests.rs`와 `review-followup.json` 두 파일뿐이며 제품은 같다.

| 실행 | source SHA-256 | ELF SHA-256 | result SHA-256 |
| --- | --- | --- | --- |
| r1 | `0fdeaf5b4e737a4b31c88652e852cc38b6d8da1d7641ab888f14ee481c022b8e` | `8f3d2bb9df9585bffda0921588bbaac6104cbaf2787aa56d5eca25d672f65a86` | `1ff91651a5bca9c55d909ac05f77dbadf84041882ed8b82d0bddf78cb0473913` |
| r2 | `d4df30c09888ff147d311aca5c0824ad9a99f3dc7df4ecc51dcca5aab40f5d9d` | `0a7d7bffce60b0b035d80a5f0d99f9c057c3150d3aee125816301b3432dd0bc7` | `535460838c58febd07f83ca54f58e447781f0cedf32a43d5dec0ea2638ea1677` |

원 compiler/runtime 로그·source별 파일 hash inventory·runner/manifest·r1 시험 원본·결과를 `artifacts/module-row-jobs-20261029/`에 보존했고 두 실행의 export/target/ELF는 삭제했다. 최종 독립 D1/D2 검토는 아래에 기록하며 전체 M4/M5는 미완료다.


최종 독립 D1/D2는 `ACCEPTED_LIMITED_D1_D2`, 필수 수정0이다. `/root/boundary_review`가 고정 변경16파일·artifact43파일의 실제 bytes/hash와 원 source별 결과를 대조했고 누락·불일치0이다. r1/r2 inventory5878/5879개 및 양쪽 source fingerprint 재산출이 원 결과와 일치했다. 검토 입력 `artifacts/module-row-jobs-20261029/review-input.json` SHA-256 `3cb8fea49c2171e6dea4d5080942cc7bf040293fca0575eeeb560308bea88232`, receipt `artifacts/module-row-jobs-20261029/review-final.json` SHA-256 `b9a3915b5ca61048fd577585d362b625037b143cba49b3d46a4bb12aa2ae63a4`를 보존한다. 검토자 수정·빌드·시험·author·문서 verifier 실행0이며 후속 수용/status/receipt/hash metadata만 반영했다. 전체 graph·descriptor 의미·source admission·current Profile3/live/recovery·M4/M5는 계속 미완료다.

<a id="l1-allocation-move"></a>
## 2026-10-29 L1 `allocation_reservation` 폴더 배치

[L1 이동표](../plans/module-independence-plan.md#l1-move-table)의 배치 B를 기준 `main` `feb2b14d868738a7510d57876d7db4bd34108ea7`(tree `4d3a8799838a1966cfc52ea1d4626e78ef61efaf`)에 적용했다. 사용자 지시는 “권장으로 진행해 주세요”다. 작성·통합은 이 Claude 세션이고 독립 검토는 별도 읽기 전용 subagent다. 착수 때 checkout은 clean이었고 L1 파일의 동시 편집은 없었다. 기능 본문·논리 모듈 경로·visibility·cfg·시험 selector는 바꾸지 않았다.

| 원 경로(`crates/engine/src/storage/wal_io/`) | 현재 경로 | 논리 경로(`…doublewrite_install::allocation_reservation`) |
| --- | --- | --- |
| `allocation_reservation.rs` | 유지, `#[path]` 5줄만 변경 | 진입 |
| `allocation_operation.rs` | `allocation_reservation/operation.rs`, `#[path]` 1줄 변경 | `operation` |
| `plan4_allocation_operation.rs` | `allocation_reservation/operation/plan4_retention.rs` | `operation::retention` |
| `allocation_budget.rs` | `allocation_reservation/budget.rs` | `budget` |
| `allocation_reservation_tests.rs` | `allocation_reservation/tests.rs`, `#[path]` 1줄 변경 | `tests` |
| `allocation_budget_profile_tests.rs` | `allocation_reservation/tests/budget_profiles.rs` | `tests::budget_profiles` |
| `allocation_native_tests.rs` | `allocation_reservation/native_tests.rs` | `native_tests` |
| `allocation_resource_tests.rs` | `allocation_reservation/resource_tests.rs` | `resource_tests` |

현재 링크는 contracts 2곳과 본문 표기 1곳, 증거 Markdown 링크 1곳을 갱신했다. 증거 문서는 링크만 바꾸고 원 경로·기준 commit·변경 전 SHA-256과 원 byte 위치를 그 문서에 덧붙였다. 관리 JSON의 경로 문자열 25개를 현재 경로로 바꾸고 `module_folder_reorganization`의 상태를 갱신했다. 증거 JSON과 다른 역사 기록의 원 경로는 그대로다.

| 검증 | 실제 범위·결과 |
| --- | --- |
| 정적 대응 | `crates/` diff는 rename 7개와 `#[path]` 7줄뿐이다. 이동만 한 5개는 기준 SHA-256과 같고 `doublewrite_install.rs`는 바뀌지 않았다 |
| 컴파일 | `cargo check -p xadb-engine --all-targets --all-features --locked` PASS, warning 0. rustc/cargo 1.95.0 |
| 시험 발견 | 이동 전 `feb2b14`와 이동 후 `cargo test -p xadb-engine --lib --all-features --locked -- --list` 출력 1585줄이 byte 단위로 같다(SHA-256 `601bd166c6b31b8bdd9e7361eecb15d5939e7b1153b323f10f9fd5308f30259a`). `allocation_reservation::` 14개와 script selector 3개가 그대로다 |
| 시험 본문 | NOT_RUN. 본문은 그대로이고 위치 attribute만 바뀌었다. borrow probe·mixed-init `owner_drop`도 논리 경로와 probe 파일 위치가 같아 NOT_RUN |
| fmt·Clippy·문서 검사 | 실행하지 않았다. 기존 push gate에서 수행한다 |

두 cargo 명령은 실행마다 scratchpad의 별도 `CARGO_TARGET_DIR`을 썼다. 이동 전 시험 목록은 `feb2b14`의 clean checkout에서 수집했고(`logs/toolchain.txt`의 HEAD), 그 뒤 `crates/` 밖의 source root는 바뀌지 않았다. `scripts/check_dev0.py`의 `source_fingerprint`를 tracked 파일만 내보낸 tree에 적용한 값은 이동 전 `574aa460b9cfab58ad319dd28b1f5b6fb2cf37d06e0b7e5aac2f0cd3d0d00b74`, 이동 후 `81ed7353f7c8302b4f859fba7a76b7771a700b33780ca94d39503bcf82c4fa40`이다. 작업 tree에서 계산한 이동 후 값 `dd713c1004336bd405b964f876266c18e7837d5b722a978f1b38b7073d28862a`에는 추적하지 않는 `scripts/__pycache__`가 포함된다.

현재 상태 문구는 `devdoc/README.md`, `plans/README.md`, 저장소 정책 §5.2, `dev3-module-boundaries.md`와 계획 13절에서 L1 적용으로 갱신했다. 로그·이동 전후 시험 목록·파일별 hash·fingerprint·원 증거 문서 byte를 로컬 `artifacts/module-l1-allocation-20261029/`(`manifest.sha256` SHA-256 `396101b84eecf563181a4b2bae526f5533f200fec53a9d7e442f00aa2e5d054d`)에 보존했고, 이번 실행의 build target 2.0G와 임시 export는 삭제했다.

최종 독립 검토는 실제 diff·문서·JSON·artifact를 읽기 전용으로 대조해 **ACCEPT**, 필수 지적0으로 판정했다. rename 7개와 `#[path]` 7줄, byte 동일 5개, 링크 검사 오류0, JSON 경로25개, 시험 목록 동일, manifest 대조를 확인했다. 선택 지적5건 중 오래된 현재 상태 문구, 검토 기록 문구, 명령·이동 전 fingerprint 기록, 상태 문구 갱신 목록의 4건을 반영했다. fmt·Clippy·문서 검사가 push gate에 남는다는 1건은 그대로다. 이 선택 반영은 다시 검토받지 않았다. L2–L5와 전체 M4/M5는 미완료다.

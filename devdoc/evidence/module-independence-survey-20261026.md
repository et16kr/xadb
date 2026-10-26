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

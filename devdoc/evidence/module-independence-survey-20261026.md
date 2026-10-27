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

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

# DEV3 모듈 전환의 두 제품 소비자

2026-10-26 · `D3-MODULE-CONSUMERS-001` · **D2 ACCEPTED / M2 완료 / A·B 제품·fixture 인수 NOT_RUN**

[전환 계획과 MI01–06](../plans/module-independence-plan.md#acceptance) · [공통 경계](dev3-module-boundaries.md) · [첫 행 완료 계약](dev3-task-11-commit.md#first-row-integration) · [트리·행 정본](dev3-task-06-tree-rows.md) · [undo 정본](dev3-task-09-undo-rollback.md) · [시험 계획 DIR03](../plans/dev3-test-plan.md#direction-integration)

이 문서는 A의 **첫 영속 Row INSERT 거래**와 B의 **snapshot row point-read·순서 cursor**를 M5의 서로 다른 제품 기능으로 선정했다. 최초 조사는 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`, 이번 설계 대조는 `3c639c31f808b2ce9504525d6993bcb62d304431`의 실제 source/owner를 기준으로 한다. 구체 형식·API·수치의 아래 선택은 [최종 독립 D2 검토](../evidence/module-independence-survey-20261026.md#final-design-review)를 수신한 **채택 계약**이며 구현·시험 완료 주장이 아니다. 담당은 C·통합 `/root`, A INSERT `/root/module_consumer_survey`, B read/cursor `/root/module_boundary_survey`, 독립 검토 `/root/module_handoff_review`다. MCQ01–07의 설계 결정과 실제 공급 의무를 구분한다. M2/M3는 완료했고 C0 borrowed codec 부분공급을 진행했으며 C0 전체·C1–C5 및 A/B 제품·인수는 미완료다.

공통 공급과 인수 fixture가 없는 현재 상태에서 독립 구현·복구 가능성을 완료로 선언하지 않는다. 이 문서의 후보 경로·함수·case는 아직 제품 등록이나 실행된 selector가 아니다. 공유 색인·기존 owner 계약의 채택 반영은 통합 담당이 맡는다.

<a id="selection"></a>
## 1. 선정 이유와 원 backlog

| 소비자 | 원 작업·요구 | 담당이 만드는 제품 동작 | 선정 이유 |
| --- | --- | --- | --- |
| A 첫 영속 Row INSERT | 3.06.2·3.06-R4/5/10, 해당 3.09.2/3·3.11.1·3.12, XADB-021/022, DIR03·D3-COMP-01 | 실제 생성된 table에 새 행을 쓰고 statement 성공/실패·commit·full abort·새 process 복구를 연결 | 현재 작은 첫 행 목표 자체이며, 단순 codec/도구 변경으로 대체할 수 없음 |
| B snapshot point-read·cursor | 3.06.3·3.06-R1/7/8, 해당 3.10.1, XADB-021, NT06 | 실제 resident 또는 재적재 page에서 fixed-q·own-write를 판정하고 caller-owned 행·정당한 부재·순서 결과를 반환 | A와 같은 page/참조/statement 경계를 실제 소비하되 A의 row producer를 구현·호출할 필요가 없음 |

CREATE의 남은 native 시험/F05만을 B로 세지 않는다. [현재 CREATE 증거](../evidence/dev3-catalog-remaining-20261026.md)는 기존 제품 본문의 source별 메모리 결과다. 시험 완료는 신규 제품 기능이 아니며 전체 CREATE/native 완료의 대용도 아니다. Catalog DROP은 3.08.2의 occupied before CLR·종속 index 0/1/8의 원자 retire·old snapshot 객체 수명까지 요구하므로 이번 첫 쌍보다 공통 미공급 범위가 넓다. 빈 table 한 cell을 지우는 축소판으로 그 계약을 대신하지 않는다.

<a id="profile"></a>
## 2. 첫 지원 profile과 보존할 기능

이번 A/B의 채택 profile은 **첫 extent64 안, 실제 CREATE의 기존 level0 RowLeaf, canonical inline row 최대2,048B, index0**이다. 스키마는 기존 TableSpec의 Bool/Int64/Utf8/Binary·nullable·1..32열 의미를 보존한다. 실제 leaf의 남은 byte·Undo container·queue·transaction/F05 한도가 허용하는 여러 행, 여러 성공 statement, 한 statement의 여러 INSERT를 지원한다. 임의로 “한 거래 한 행”으로 줄이지 않는다. 수치 capacity는 새 F05 표로 동결하며 현재 Catalog의 pin9/private425,984B를 그대로 허용치로 복사하지 않는다.

- A의 입력은 원 CREATE/선택에서 발급된 `CheckedTable` 후보 권한, C의 실제 transaction/statement 소유, typed 값이다. 동일 statement의 부분 성공 뒤 실패하면 그 statement의 mark까지 undo한다. 앞선 statement 결과·잠금·full abort reserve를 남기고, 이후 새 statement 또는 full abort가 가능해야 한다.
- B는 supplied RowId의 point lookup과 RowId 오름차순 전체/범위 cursor를 제공한다. own INSERT는 같은 거래의 유효 statement에서 보이고, 다른 거래의 Active INSERT와 snapshot보다 뒤의 Committed INSERT는 Missing으로 판정한다. q 이하 committed 값, CLR로 제거된 값, committed stamp로 정리된 값을 구분한다.
- B는 호출마다 현재 anchor와 마지막 full RowId key에서 재탐색한다. 같은 transaction의 후속 mutation은 기존 cursor를 무효화한다. physical slot/page pointer를 호출 간 정답으로 보존하지 않는다. 세부 token·오류 진행점은 [statement/cursor owner 계약](dev3-task-06-tree-rows.md#module-statement-cursor)을 따른다.
- 현재 table에 index가 실제 존재하면 index를 무시하고 쓰지 않는다. nonzero index·overflow 필요·leaf split 필요·다른 tree level·UPDATE/DELETE/occupied before는 effect 전 typed Unsupported 또는 해당 용량 오류로 거절한다. 기존 API 오류 분류와 정확 우선순위는 M2의 F02 결정에 연결한다. 저장 손상은 Unsupported/NotFound로 낮추지 않는다.
- 지원 크기 검산 후에도 WAL stage/apply·commit·rollback 실패는 실제 효과에 따라 판정한다. no-force ACK, uncommitted steal, statement 실패, UNKNOWN, 부분 CLR 재장애, 정상 admitted quota에서의 복구와 새 ACK 의무는 [첫 행 완료 계약](dev3-task-11-commit.md#first-row-integration) 그대로다.

일반 SMO·overflow·UPDATE/DELETE·index·locator cache·모든 isolation 조합을 이번 M4 선행으로 추가하지 않는다. 이 제외는 해당 3.06/T06·DEV3 S5 의무나 기본 Serializable·Read Committed 계약의 취소가 아니다. 선택 범위의 실제 read/write set은 [원 격리 공급](dev3-task-10-concurrency.md#module-row-isolation)의 table/catalog epoch·원 Commit 직렬화·Serializable/RC snapshot으로 검증한다. 유일 RowId 발급만으로 Serializable을 충족한다고 판정하지 않는다. 지원 profile 밖 이미 저장된 입력을 일부만 읽고 완료로 반환하지 않으며, 현재-format 지원 여부를 첫 효과와 사용자 결과 전에 검사한다.

<a id="actual-supply"></a>
## 3. 실제 공급과 아직 없는 공급

| 실제 source | 현재 공급 | A/B에 남은 연결 |
| --- | --- | --- |
| [tree_page.rs](../../crates/engine/src/storage/tree_page.rs), [edit.rs](../../crates/engine/src/storage/tree_page/edit.rs) | `lower_bound`, `route_child`, `prepare_leaf_edit`, `LeafEditPlan`의 borrowed 표현·후보 bytes | 원 table/anchor·전체 pin/재검사·row 의미·stage 권한 |
| [row_record.rs](../../crates/engine/src/storage/row_record.rs) | canonical schema/value·row payload codec | live Row writer/reader, statement identity·원 row source 권한 |
| [anchor.rs](../../crates/engine/src/storage/anchor.rs) | XTA1 root descriptor·identity/level 검산 | runtime qualified table/anchor 획득. 현48B anchor cell에 RowId issuer 없음 |
| [undo_record.rs](../../crates/engine/src/storage/undo_record.rs), [status_home.rs](../../crates/engine/src/storage/status_home.rs) | Row Missing/occupied before·head/progress·count 표현 | Missing row의 실제 head chain·CLR·source pin·retained cleanup |
| [obligation.rs](../../crates/engine/src/storage/obligation.rs) | Descriptor1536의 `last_mutation_sequence`(offset1344), core의 StatementFinish opcode5 | RowReserve/Forward/CLR·multi-statement 실제 전이와 비용 검산 |
| [Root v2](../../crates/engine/src/storage/retention_page/catalog/root.rs), [Job v2](../../crates/engine/src/storage/retention_page/catalog/job.rs) | Undo/Object high-water, C, Terminal/CatalogMissingRoot의 실제 queue 표현 | 별도 RowId high-water, RowMissingRoot의 여러 root·table 소유·예산 의미 |
| [Catalog live](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_live.rs), [commit](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_commit.rs) | 원 owner의 CREATE reserve/initialize·terminal 준비/실행 | 기존 실제 CREATE/native/F05의 해당 미완료 의무; Row 도메인 본문은 없음 |
| [Catalog lookup](../../crates/engine/src/storage/wal_io/plan4_retention/read/lookup.rs), [visibility pins](../../crates/engine/src/storage/wal_io/plan4_retention/visibility/pins.rs) | Catalog source 아래 Txn+Undo pin·실제 snapshot/Status/Undo resolve | Row source 생성과 Missing-only 참조 이동. 기존 CatalogSource를 raw Row bytes로 만들 수 없음 |

원 owner·역참조·공통 파일 변경 원인은 [실제 소유 조사](dev3-module-boundaries.md#actual-ownership)에 둔다. 과거 Catalog의 decoder 결함 중 이미 해결한 항목을 현재 blocker로 복사하지 않는다. 기존 실행 증거는 원 source·실행 범위로만 수신한다.

<a id="division"></a>
## 4. C 공급과 A/B 제품 본문의 분리

| 담당 | 실제 산출물 | 넘겨서는 안 되는 책임 |
| --- | --- | --- |
| C 파일/page/WAL 경계 | 원 owner/pool/WAL/fence를 보존한 checked table·page 획득·전체 target pin·candidate 검증·stage/apply·load/flush·terminal/recovery 조립 | 사용자 값으로 Row INSERT 후보를 생산하거나 B point/cursor를 완성하는 기능 본문 |
| C 참조/transaction 의미 | Missing-only의 snapshot·source reference capture·Status/Undo 읽기·statement/mark/private revision·issuer/예약 소유·후속 종료 권한 | RowId/UndoRef의 raw 값만으로 권한 생성, 새 전역 manager·상태 사본, 상대 소비자 내부 접근 |
| C 의미 observer/재생 | 아래 동결될 Row frame grammar를 실제 before/owner/대상/예산과 검산하고 재생. C+A와 C+B가 같은 validator를 소비 | raw frame의 checksum/codec 성공만으로 actual stage/recovery authority 발급 |
| A | typed 값 검산·canonical row/신규 logical identity·Missing Undo/row 후보 생산, Row INSERT 순서, logical CLR 후보 생산, statement 실패/full abort·commit과 자동 loser undo의 행 handler | B point/next 구현·fixture 호출. 공통 권한·lock·WAL 의미를 A 내부에 재구현 |
| B | point/next의 현재 anchor 재탐색, visibility 판정, caller-owned 결과/observation, cursor token·진행점·cleanup | A row producer/rollback handler·빌드 산출물·fixture에 의존 |
| 통합 | 사전 지정 모듈/등록/assembly 연결, 실제 C+A+B 상호작용 인수 | 기능 본문을 공유 연결 변경으로 위장 |

전용 제품 경로 후보는 A `crates/engine/src/storage/row/insert.rs`와 `row/insert/` 하위, B `row/read.rs`와 `row/read/` 하위다. 시험·fixture도 `row-insert`와 `row-read` 입력으로 분리한다. 실제 내용이 생길 때만 경로를 등록한다. 공유 `storage/row.rs`의 A 또는 B 모듈 한 줄 등록은 통합 담당이 **각 단독 source의 첫 검증 전에** 해당 기능 것만 적용하고, 합류에서 두 등록을 합친다. C0의 빈 모듈/상대 기능 stub은 실제 공급·기능 검증의 증거가 아니다. 다른 공용 의미 변경이 필요하면 원 시도를 보존하고 M2/M4로 환류한다.

C의 Row observer는 필요한 공통 신뢰 경계다. A는 그 grammar에 맞는 실제 forward/CLR producer와 사용자 변경 흐름을 만든다. recovery 조립은 A의 typed Row undo handler를 받으며, C+B의 완결 committed fixture에서 행 undo가 필요하지 않음을 검산할 수 있어야 한다. Row undo handler가 없는 source에 실제 loser가 나타나면 Unsupported/RecoveryRequired로 닫고 READY를 반환하지 않는다. A 없는 fallback으로 CLR를 생략하지 않는다. 이 handler의 consuming 입력·반환·오류/loan·모듈 의존은 [공통 API 명세](dev3-module-boundaries.md#api-candidates)의 RowReservation/recovery 절을 정본으로 따른다. A 없는 source의 READY는 그 절의 no-loser 검산 범위이며 임의 recovery 완결 stub이 아니다.

<a id="row-issuer"></a>
## 5. RowId와 예약의 구체 제안

**추천안:** 원 global Root512를 명시 version3으로 확장하고 `[432,440)`에 `row_high:u64 LE`, `[440,512)`에 reserved0을 둔다. genesis는 row_high0이며 발급값은 checked `row_high+1`이다. table별 재사용 counter를 만들지 않고 DB/timeline 안에서 모든 table의 INSERT가 같은 발급 경계를 소비한다. RowUID는 table generation과 RowId의 조합으로 사용한다. Root의 `undo_high@392`·`object_high@400`·`commit_c@408`와 RowId는 서로 다른 typed 의미다.

RowReserve는 원 Active actor·statement, 이 profile의 actual empty Tx inventory, Free Undo slot의 actual generation, Free RootJob, table/anchor/leaf proof, 충분한 forward/CLR/terminal/cleanup 자원을 함께 검사한다. actor당 미완결 RowReserve는 하나이며 Forward consume 또는 Cancel release 후 다음 Reserve를 시작한다. 이전 성공 INSERT의 retained root는 inventory reservation이 아니라 실제 native/queue 소유로 남는다. 한 원자 그룹에서 다음을 채택한다.

1. `row_high+1`, `undo_high+1`, UndoSlot generation+1, RootJob generation+1. 뒤따르는 RowForward를 위한 계획된 RowId/UndoSequence/slot을 원 예약 owner가 소유한다.
2. transaction Descriptor의 `last_mutation_sequence+1`을 planned mutation으로 소모한다. RowForward는 그 번호를 실제 logical version에 쓰고 다시 증가시키지 않는다. Cancel/CLR/StatementFinish는 이 high-water를 낮추지 않는다. 이는 기존 Catalog Reserve의 last_mutation0 유지와 다른 **새 scope 제안**이다.
3. Tx의 `UndoSlot(U)` inventory item1, RowMissingRoot Reserved Job, exact 향후 debt. 기존 table anchor/RowLeaf는 이미 Allocated이며 PageToken/pair INIT를 다시 예약하지 않는다. Free Undo container의 실제 확보/INIT는 원 C 공급에서 먼저 수행하고 그 효과·비용을 숨기지 않는다.

성공한 Reserve 뒤 Cancel은 slot reservation·Job과 미사용 debt를 정리하지만 **같은 live open에서** RowId/UndoSequence/mutation/slot generation을 반환하지 않는다. CLR·abort도 채택된 high-water를 낮추지 않는다. crash 전 caller에게 성공 RowId를 반환하기 전에는 해당 Forward의 complete stage/whole apply가 필요하지만, 이 성공만으로 transaction ACK나 WAL durability를 주장하지 않는다. `wal_io::stage_verified_required`는 RAM slot copy와 exact requirement 발급이며 실제 write/sync는 별도다.

재시작의 비재사용 검산은 **선택된 base와 실제 채택된 complete WAL prefix에 포함된 issuance**를 기준으로 한다. 해당 prefix의 Reserve가 Cancel/CLR/abort로 끝나도 row_high/UndoSequence/mutation/generation을 되감지 않고, old WAL 제거 뒤에는 선택 base가 그 high-water를 보존한다. RAM에만 존재하여 선택 prefix 밖으로 유실된 Reserve는 durable 발급 증거가 아니므로 재시작이 그 번호 소모를 보존한다고 주장하지 않는다. old open의 RowId 관측·statement/handle은 새 open의 권한이 아니다. native 시험은 아래 [prefix witness](#fault-cuts)로 이 두 상태를 분리한다. 시험용 force는 지정 cut의 보존 조건이며 제품의 매 Reserve/Forward/CLR에 새 강제 sync를 요구하는 변경이 아니다.

| 대안 | 평가 |
| --- | --- |
| 새 global Root field — 추천 | 최초 RowId 의미와 recovery census가 명시적이며 Undo 발급량·table 삭제에 결합하지 않음. Root/current genesis/base/profile/observer의 새 형식이 필요 |
| 새 UndoSequence 값과 RowId를 명시 매핑 | 같은 global 비재사용 counter이면 유일성은 가능. 그러나 두 발급 의미를 결합하고 후속 row/undo reserve 분해를 제약하므로 이번 추천에서 제외 |
| table anchor에 local high-water | table별 연속성은 가능하나 XTA1 변경·모든 anchor reader/CREATE INIT/복구와 추가 native 필요. 이번 first profile에 이점이 부족 |
| 최대 existing RowId+1·RAM counter·LSN/slot 재사용 | abort/aging/복구/old WAL 제거 뒤 비재사용을 증명하지 못하므로 배제 |

이 offset 선택은 아직 F01 채택이 아니다. [개발 형식 정책](../policies/version-compatibility-policy.json)에 따라 새 fixture/current runtime 버전을 명시하며 v2 decoder·원 증거를 자동 승격하지 않는다. 기존 DB 자동 삭제/덮어쓰기·암묵 변환은 없다.

<a id="row-wire"></a>
## 6. RowMissingRoot와 exact 효과 의미 제안

### 6.1 Job와 scope

Job1024는 v3, kind3=`RowMissingRoot`를 제안한다. Free/Terminal/CatalogMissingRoot의 의미는 명시된 version3 선택 아래 보존하고 Row 전용 의미를 기존 Catalog kind2에 얹지 않는다. Row root는 U/sequence@128/168, table ObjectHandle@176, stable table anchor@200, **Row의 물리 위치 권한이 아닌 준비 당시 leaf hint**@224, statement/mutation@248/256, planned tree cell bytes@280, row projection digest@288, **RowId@320**, sender-before descriptor digest@328을 사용한다. status/source·lease/adoption/release/cursor·budget 영역은 기존 의미를 소비한다. kind3 전용 필드는 같은 위치를 쓰는 Catalog/Terminal에 존재한다고 해석하지 않는다.

Job의 state@6/kind@7은 Free=(0,0), TerminalLeased=(1,1), TerminalReady=(2,1)을 유지하고 RowReserved=(1,3), Retained=(2,3), ReleasingCommitted=(3,3), ReleasingCompensated=(4,3)로 고정한다. kind2 Catalog의 기존 state 조합도 version3 아래 별도로 검산하며 kind3로 해석하지 않는다. Releasing은 실제 row를 다시 읽을 근거가 없으므로 hint/cell bytes/projection을 canonical zero로 지우고, U/sequence/table/RowId/statement/mutation/source·cleanup 증거는 Free까지 보존한다. RowReserved마다 unique U와 RowId, actor마다 Terminal 정확1을 요구하지만 Row root≤1을 요구하지 않는다. 원 queue64·nonfree≤55와 원 Status/Undo physical uniqueness는 유지한다. 첫 profile에서 한 Undo page의 slot0 하나를 쓰므로 여러 root는 여러 실제 container를 요구한다.

Row scope의 Compensated(c0)는 실패 statement를 처리한 뒤 Active, AbortingStatement 또는 AbortingFull actor와 공존할 수 있다. 그 뒤 앞선 성공 statement를 Commit하면 source는 Committed(q>0)/TerminalReadyCommitted이지만 **compensated root의 q는0**이다. 전체 abort면 source Aborted/TerminalReadyAborted다. 따라서 Row의 compensated cleanup은 actor가 어느 terminal outcome으로 끝났는지 검산하고 둘 모두에서 진행할 수 있어야 한다. Catalog scope2의 Compensated→AbortingFull/Aborted·headNone 제한을 Row에 복사하지 않는다. RootJob이 남아 있으면 source Status의 TerminalFinish를 막는 원 census로 수명을 보호하며 creator를 숨은 incoming edge로 세지 않는다.

XHG/XSG version2에 explicit scope3=`RowMissing`을 제안한다. scope2 Catalog observer가 이를 fallback으로 받지 않는다. header·두 descriptor(Tx/Aging)·OperationCore256·Native/Edge/Count/Inventory/Redo section layout은 [기존 group envelope](dev3-create-group-format.md)를 재사용한다. current Profile는 version3(같은 ID/용량 tuple), Root/Job는 version3, XRB1 base는 version4(동일2MiB layout에 새 Row state/issuer census)를 추천한다. 형식 폭·기존 backing을 늘리는 결정은 아니다. Row용 body/shape·current genesis/base/profile 수신을 함께 동결해야 한다. 현재 runtime에 old-format fallback을 추가하지 않으며 pure legacy decoder·원 증거를 보존한다. 현재 source와 목표 dispatch의 대응은 다음 표로 고정한다. 숫자1..3의 legacy 해석은 순수 decoder에서만 보존하며 target current runtime이 그 결과를 fallback 권한으로 채택하지 않는다.

<a id="format-dispatch"></a>
| 경계·현재 실제 source | 현재 값 | 목표 current 경로·거절 |
| --- | --- | --- |
| `control::{Profile::base_bytes,profile_prefix,encode_catalog_profile,decode_bootstrap_profile,read_header_for}` | Profile1=1MiB,2=2MiB; ID1/2/3 buffer4/16/64MiB | 명시 `RowMissing` Profile3=2MiB. ID/188B capacity tuple·4096B envelope 유지; current genesis만3 생성. 다른 control envelope는1 유지. parsed Profile는 format을 잃지 않음 |
| `retention_page::catalog::{initial_root,decode_root,free_job,decode_job}` | XQR1/XQJ1 version2, Root reserved432..512, Job reserved320..328; kind0/1/2 | current Root/Job3. Root row_high@432, Job kind3/RowId@320만 새 의미. legacy2 observer를 current3에 재사용하지 않음 |
| `plan4_recovery_base::{BaseFormat,catalog::{decode_layout,encode_layout}}` | V1Status1/V2System2/V3Catalog3; Catalog Profile2·base3 고정 | `V4RowMissing=4`, Profile3·base4만 current 조합. Catalog의2MiB section offsets·64page arena·352B INIT provenance 폭 유지. 같은 page arena에서 Row/Undo/Status/Job 그래프를 추가 검산 |
| `plan4_recovery_select`, original recovery/population/history/read/control/current base inventory | `format()==2` 또는 `V3Catalog` 분기로 Catalog source/census 선택 | target current가 선택한 Profile3→Root3/Job3→base4를 한 closure로 검사. 원 System INIT scope1·Catalog scope2·Row scope3가 같은 **target profile**에서 각자의 closed observer로 분기. raw profile number를 바꿔 v2 검산을 우회하지 않음 |
| `semantic_catalog::{Operation,wire}` 및 original replay dispatch | XHG/XSG version2, flags2; Catalog opcode5 없음 | Row observer만 version2/flags3. XHG:1,2,4,6,7,11,12; XSG:3,5,16,17,20,21,22. StatementFinish5의 outer=`Update`, 나머지는 원 opcode의 outer 유지. scope/op/outer 조합 불일치는 Unsupported; 다른 observer 재시도 없음 |
| native/edge/slot/row lower format | Row1/Undo3/Status5/Root8/Job9, 역할1/4/7/10, UndoRow target1 | 번호/기존 폭 유지. Row metadata는 kind11, anchor는10이며 INSERT에서 변경 native가 아님. native kind2/4/6/7/10/11과 occupied before는 이 Row effect의 shape에서 거절; proof page를 가짜 native로 만들지 않음 |

새 current 설치·선택·base capture·replay·recovery·native worker가 위 조합을 함께 소비한다. old Profile1/2 또는 base1/2/3를 발견하면 명시 UnsupportedFormat/UnsupportedClosure이며 삭제·자동 변환·fresh 재생성하지 않는다. target Profile3 안의 Catalog scope2는 실제 CREATE를 유지하되 Root/Job3 검산을 받는 명시 adapter다. 기존 Profile2의 원 증거와 순수 decoder는 그대로 보존한다. 준비 중 변경할 `format()==2` 분기를 단순 `>=2`로 넓히지 않는다. version0/4/65535 Profile, base0/5/65535, Root/Job2/4, flags0/1/2/4를 Row 입구에 넣는 거절 변형을 둔다.

operation은 Begin1, Forward2, RollbackStart3, CLR4, StatementFinish5, Commit6, AbortEnd7, Reserve11, Cancel12, AgingStart16, TerminalFinish17, Prune20, ReleaseRole21, RootFree22를 후보로 둔다. OperationCore의 기존 statement/mutation/current/next/target/q/digests를 실제 전이에 사용하고 unused field는0이다. 별도 opaque body나 일반 무검산 dispatch를 추가하지 않는다. Native Row kind1·Undo3·Status5·Root8·Job9와 기존 role 번호는 원 [obligation](dev3-obligation-format.md)의 의미를 따른다.

row logical projection 후보는 `XRP1`, version1, kindRow1, UndoTargetCore32(table object/generation,entity RowId,arity0,keylen0), presence1+zero7, present일 때 정확 row record bytes다. Missing은48B, present는48+record bytes다. 물리 slot/offset/pageLSN/count는 projection 밖이다. tree entry의 RowId BE key와 row record의 RowId LE를 각각 해석하여 같음을 검사한다. source slot 위치를 논리 identity로 사용하지 않는다.

### 6.2 Forward

`T`는 원 TxnRef, `H`는 before Status.head, `U`는 Reserved Undo다. 실제 table/anchor/schema와 row key Missing을 원 before 아래 재검사한다. A는 row record `stamp=Transaction(T), logical=(T.id,planned_mutation), undo=U, deleted=false`와 canonical inline payload를 만든다. 새 U는 target Row(table/gen/RowId), before Missing, before stamp/logical 없음, previous-version Never, previous-transaction은 H가 있으면 Linked(H, 실제 sequence), 없으면 Never다.

| 효과 | exact before→after |
| --- | --- |
| RowLeaf | 지정 key Missing→새 row record. 다른 cell·metadata를 보존한 `LeafEdit::Insert`와 whole page Patch |
| Status home | Active의 head H→U, row Transaction stamp 추가로 incoming+1. 기존 actor/generation 유지 |
| Undo U | 동일 예약 generation Free→Live Missing, incoming2, 실제 creator/sequence/mutation/target·위 두 edge |
| RootJob | Reserved→Retained, planned identity/projection·실제 adoption LSN과 sender SHA 결합 |
| Root/Tx/Aging | apply/revision/census/debt/descriptor chain·inventory consume와 actual WAL 비용을 함께 채택. row_high 재증가 없음 |

edge multiset은 RowUndo `None→U`, RowStamp `None→T`, TxnHead `H→U`, U.previous-transaction `None→H`(H가 있을 때)다. U incoming은0→2, T incoming은+1이다. H는 head 제거1과 새 U의 prev-txn 추가1로 net0이지만 **제거 가능한 실제 before edge·Live target·sequence를 검사**한다. net0만 보고 존재 검사를 생략하거나 가짜 count-only Native write를 만들지 않는다. source와 count proof가 되는 page는 전체 target/pin·F05 집합에 포함한다.

### 6.3 CLR와 여러 statement의 head

CLR는 status의 실제 next U를 pin하고 현재 U의 previous-transaction H를 다음 pin으로 확보한다. 원 stable table/anchor에서 RowId를 재탐색하여 현재 row의 logical identity·stamp·undo가 consumed U와 맞음을 검사한다. caller의 옛 page/slot 또는 whole old page 복사는 사용하지 않는다. Row before Missing이므로 해당 entry만 Remove하고 다른 행을 보존한다.

같은 완결 CLR에서 RowUndo/RowStamp edge를 제거하고, Status.head와 rollback next를 U→H로 옮기며, **U.previous-transaction Linked(H)→Cut**을 채택한다. H가 없으면 Never를 유지한다. U는 incoming2→0, `Releasing(cursor0, previous-version Never, previous-transaction Cut 또는 Never)`가 된다. H는 U edge 제거1/Status head 추가1의 net0 proof다. Row root Job은 Compensated(c0)로 옮겨 cleanup을 소유하고, CLR 효과/next/target·descriptor/remaining·DPT는 같은 그룹에 남는다.

이 cut은 아직 undo할 H를 끊는 것이 아니다. 남은 rollback 시작점을 원 Status에 H로 인계하고 **이미 보상한 U suffix**의 불필요한 outgoing edge를 제거하는 것이다. U.prev-txn을 그대로 두면 다음 H의 incoming이 증가하고 terminal 뒤 cleanup 제한과 결합해 full abort 진행을 막을 수 있다. 반대로 head만 H로 바꾸고 U→H 제거를 누락하면 count/graph가 틀린다. 양쪽을 negative case로 둔다.

statement mark M은 [owner 계약](dev3-task-06-tree-rows.md#module-statement-cursor)의 실제 statement token에 결합한다. RollbackStart는 `Aborting(Statement{target:M,next:head})`, 각 CLR 뒤 `head=next`, mark 도달 때 StatementFinish는 Active(head=M)로 복귀한다. 다음 Forward는 M에 연결하고 보상한 suffix U에 연결하지 않는다. full abort는 현재 head에서 None까지 간다. CLR·StatementFinish는 RowId/UndoSequence/mutation high-water, 이미 소비한 WAL/CLR·retained charge를 되감지 않는다. crash의 AbortingStatement는 남은 전체 loser undo와 AbortEnd로 종료하며 old statement를 재개하지 않는다.

compensated U의 cleanup은 queue가 보존한다. 첫 profile에서는 actor terminal 전 새 admission을 위해 이를 무리하게 free하지 않고, 반복 실패로 queue/container가 차면 새 forward만 거절한다. 기존 transaction rollback/terminal의 예약은 남아야 한다. 나중 ReleaseRole은 원0→1→2→3을 각각 실행하며 Cut/Never를 이미 처리된 role로 표시하고 같은 count를 다시 감소시키지 않는다. RootFree는 세 role·모든 pin/DWB/flush 조건 뒤에만 수행한다.

### 6.4 Commit·Prune·복구

Commit은 actual Active actor의 완료 statement·head chain·모든 RootJob을 census하고 C+1·TerminalReady·Status Committed를 원 terminal bridge에 인계한다. 원 stage→durable→Drain→whole install→Q→ACK를 유지한다. Commit은 retained row/U/root를 즉시 Free로 만들지 않는다.

Missing-only committed Prune는 q≤Horizon이고 실제 참조 pin 조건이 맞는 **현재 Status.head에 해당하는 Retained root부터** 처리하는 안을 제안한다. row를 Committed(q)/undoNone으로 stamp하고 logical identity는 보존한다. U의 row/head incoming2를 제거하고 previous-txn H를 cut하면서 head를 H로 인계한다. 뒤 root가 아직 가리키는 과거 U부터 Releasing으로 바꾸지 않는다. queue scan은 eligibility와 실제 진행을 구별하고 bounded scan·old snapshot 해제 뒤의 진행을 검증한다. 공통 aging/회수 담당과 해당 actor의 여러 root 비용을 함께 검토한다.

recovery는 current base/profile의 Root v3·Job v3·high-water·다중 Root/Status/Undo census를 검산한다. Row frame의 의미/descriptor 채택과 pageLSN REDO를 분리하고 same owner LSN의 예약을 두 번 소비하지 않는다. loser의 row CLR 생산은 A handler, 원 stage/redo/REBASE/LOG_OPEN/VERIFY/STABILIZE는 C/복구 owner다. no-force·steal·부분 CLR·old WAL 제거 후 high-water/head 복원·새 ACK/재장애의 실제 연결까지 완료 의무다.

<a id="row-states"></a>
### 6.5 상태와 참조 multiset

한 거래의 Row root 수를 `n`, 아직 보상/Prune되지 않은 행 수를 `v`라 한다. 한 거래에 Catalog/Row root를 함께 만드는 것은 이번 first profile 밖이며 첫 RowReserve 전에 거절한다. 그 제한은 여러 Row/statement를 금지하지 않는다. Reserved U는 `Free(g_new)`의 **실제 bytes+UndoSlot inventory**이며 새 Undo state를 발명하지 않는다.

| 전이 | source Status·head/rollback | Row Job·U와 census |
| --- | --- | --- |
| Begin | Active/headNone | TerminalLeased1, Row root0, Status incoming0 |
| Reserve | Active/headH 그대로, active statement | RootReserved 추가1, U Free(g_new), inventory1; actor의 다른 Reserved0, last_mutation+1 |
| Cancel | Active/headH 그대로 | Reserved→Free Job, inventory1→0, U Free의 g_new 유지; v/head/count 불변 |
| Forward | Active/headH→U | Reserved→Retained; U Free→Live Missing incoming2, U.prevTxn=H 또는 Never; inventory1→0, v+1 |
| StatementRollbackStart | Active/headH→AbortingStatement(headH,nextH,targetM) | Reserved0; retained/compensated roots 유지. targetM은 원 statement mark이고 현재 chain의 suffix 경계 |
| FullRollbackStart | Active 또는 복구된 AbortingStatement→AbortingFull(headH,nextH,targetNone) | 과거 statement를 재개하지 않음. 원 descriptor의 남은 full-abort 예약 소비 |
| CLR | AbortingStatement/Full의 head=next=U→H; statement targetM 유지 | Retained→ReleasingCompensated(q0,cursor0); U incoming2→0, prevTxn LinkedH→Cut; 행 제거, v−1 |
| StatementFinish | AbortingStatement이며 head=next=targetM→Active(headM) | compensated roots 보존; rollback progress 제거, 이전 v개 행/CLR debt 보존. 종료된 statement token 재사용 불가 |
| Commit | Active·Reserved0→Committed(q=C+1), head 유지 | TerminalReadyCommitted; Retained root의 q는 prune 전0, compensated q0 그대로. pending statement 없음·격리 검증 성공 필요 |
| AbortEnd | AbortingFull·head/nextNone→Aborted | TerminalReadyAborted; Retained0, compensated 여러 개 가능 |
| Prune | Committed(q), horizon≥q, 해당 U가 현재 head | Retained→ReleasingCommitted(q,cursor0); row stampCommitted(q)/undoNone, headU→H·U.prevTxn Cut, U incoming0, v−1 |
| ReleaseRole | source terminal이며 Releasing(cursor0/1/2) | 매 group cursor+1. Missing before의 Never/이미 Cut을 검산하고 count를 중복 감소시키지 않음. cursor3 전 RootFree 금지 |
| RootFree | source terminal·Releasing cursor3·incoming0·transient0 | U→Free(같은 generation), Job→Free(같은 generation). source actor의 나머지 root는 독립 유지 |
| TerminalFinish | source terminal·그 actor의 Row/Catalog roots0·Status incoming0·transient0 | Status/Terminal 정리. Job의 source identity를 숨은 incoming edge로 세지 않고 census로 보호 |

동시에 보유하는 **persistent** edge는 행마다 `RowUndo1`, `RowStamp7`이고 chain에 `TxnHead10` 하나 및 각 non-oldest U의 `UndoPreviousTxn4` 하나다. 따라서 모든 retained Live U의 incoming은 정확2(자기 행1+head 또는 후속 U1), source Status incoming은 그 actor를 가리키는 실제 Transaction row stamp 수 `v`다. Reserved/compensated U의 incoming은0이다. `UndoPreviousVersion5/UndoBeforeStamp6`는 Missing-only에서 Never/없음이므로 edge를 넣지 않는다. 각 H의 net0는 old head1 제거+new prevTxn1 추가(Forward), old prevTxn1 제거+new head1 추가(CLR/Prune)라는 multiset이며 실제 before count≥1 증명이 필요하다. net0 target의 CountChange·동일 Native를 추가하지 않는다. target/state/sequence가 다르거나 감소 여유가 없으면 전체 그룹 거절이다.

Prune는 오래된 root부터 임의 제거하지 않는다. 같은 actor의 head가 아닌 Retained Job은 이번 step의 eligible이 아니며 원 bounded queue를 전진한다. actor가 Active/Aborting이면 compensated cleanup도 대기한다. Commit한 actor의 compensated q0는 원 q와 다르지만 정당하며, AbortEnd 뒤 retained root가 남거나 TerminalFinish 때 어느 disposition의 root라도 남으면 손상이다. point/cursor의 transient2pin은 이 persistent count에 더하지 않는다. 두 graph와 exact source latch 이동은 [참조 owner](dev3-visibility-retirement.md#row-missing-traversal), statement/mark 종료는 [F02 owner](dev3-task-06-tree-rows.md#module-statement-cursor)를 따른다.

<a id="row-shape"></a>
### 6.6 첫 profile의 target·WAL shape 입력

다음은 위 전이안을 수치화한 **설계 입력**이며 실제 source/caller F05 PASS가 아니다. canonical inline 길이를 `L=5..2048`, row record를 `168+L`, tree cell을 `C=12+168+L=180+L`로 둔다. 따라서 `C=185..2228`, entry의 cell+slot 비용은 `184+L`이다. leaf의 실제 모든 row 의미를 검산해야 작은 scratch/capacity 계산을 적용할 수 있다. canonical 최소 길이일 때 `floor(16188/189)=85`, 최대 길이일 때 `floor(16188/2232)=7`행이다. 이는 leaf 표현 용량이며 transaction admission 수가 아니다.

`P=16344`는 whole-body Patch, `S=288`은 기존 Status224의 patch header64+body224다. XHG의 outer/header/두 descriptor/core 고정합은3776, XSG는2176이다. Native 하나는128+before body+after body이며 Root는1152, Job는2176, Status는576, Missing Undo는992다. `h=1`은 previous transaction H 존재, 없으면0이다. CountChange는 실제 count가 바뀐 U/T의2행만이며 H net0는 원 [obligation proof 규칙](dev3-obligation-format.md)의 별도 read proof다.

| operation | 변경 native·page | E/K/I·redo | 전체 WAL frame bytes 제안 |
| --- | --- | --- | --- |
| RowReserve | Root·RootJob·U, native합4320;3page |0/0/1,3P|`3776+4320+208+3P=57336`|
| Forward | Root·RootJob·Status·U·RowLeaf, native합5024+C;5page |`(3+h)/2/1`,5P|`91304+C+128h`|
| Cancel | Root·RootJob, native합3328;2page. U generation은 그대로이며 읽기 proof |0/0/1,2P|`40000`|
| CLR | Root·RootJob·Status·U·RowLeaf, native합5024+C;5page |`(3+h)/2/0`,5P|`91096+C+128h`|
| Statement RollbackStart·실패 후 StatementFinish | Status home native576;1page |0/0/0,S, single owner|각`2176+576+288=3040`|
| Begin·Commit·AbortEnd | Root·TerminalJob·Status, native합3904;3page |0/0/0,3P|각`56712`|
| committed Prune | Root·RootJob·Status·U·RowLeaf, native합5024+2C;5page |`(3+h)/2/0`,5P, single Aging|`89496+2C+128h`|
| AgingStart | Root, native합1152;1page |0/0/0,1P, single Aging|`19672`|
| ReleaseRole·RootFree | Root·RootJob·U, native합4320;3page |0/0/0,3P, single Aging|각`55528`|
| TerminalFinish | Root·TerminalJob·Status, native합3904;3page |0/0/0,3P, single Aging|`55112`|

성공 statement의 끝은 F02 token/mark의 정상 해제이며 status state가 바뀌지 않는다면 가짜 동일 before/after Native를 만들지 않는다. 위 StatementFinish3040은 AbortingStatement→Active 복귀다. 성공 statement 종료의 별도 영속 operation이 실제 owner 계약에 필요하다고 결정되면 exact 변화를 명시하여 표를 갱신해야 한다. Cancel/CLR에서 이미 소비한 row reserve/발급 bytes를 반환하지 않는다.

변경 page와 proof page는 다르다. Row 단계의 보수적 전체 target 집합 후보는 `{Root, RootJob page, Status, U, RowLeaf, allocator, tableAnchor, TerminalJob page, H Undo}`의 최대9개다. 같은 Jobs page 등 실제 physical identity 중복을 제거하되 read proof의 의미를 생략하지 않는다. source의 Root/Job 전체 census는 원 bounded bank와 version guard를 통해 검산하며 모든 root U를 동시에 pin하는 안이 아니다. 이9개와 private before9/after5의 설계 배정은 [자원 owner](dev3-resource-bounds.md#row-missing-module-budget)로 동결한다. 실제 준비/검산/적용·return carrier가 이 상계 안에 드는지 source census로 확인하는 일은 해당 C 공급의 구현 의무다. 기존 Catalog pin9와 같다는 이유로 성립을 가정하지 않는다.

첫 profile은 원 **한 Undo page당 slot0 root 하나**, actor당 실제 Status page 하나를 유지한다. Jobs는 page2..6에15+15+15+15+4=64개, cell1024B이며 nonfree≤55다. first extent의 page0 allocator·1Root·2..6Jobs·7CatalogAnchor·8CatalogLeaf 뒤 동적9..63의55page에서 실제 table pair·Status·Undo와 기타 allocated/Reserved를 모두 센다. table 하나와 actor 하나만 있고 다른 동적 점유가 없는 예시라도 U의 물리 상한은52(55−2−1)이며, 모든 source에52를 grant하는 규칙이 아니다. 남아 있는 CREATE root·기존 Status/Undo·pair reservation과 simultaneous owner를 실제 census로 차감한다. 원 queue cap·leaf 남은 bytes·F05 reserve 중 가장 먼저 걸리는 한도가 admission을 닫고 이미 admitted된 rollback/terminal은 계속 진행해야 한다.

statement 실패와 full abort의 같은3040B를 두 번 배정하지 않는다. Active transaction은 기존 retained root 각각의 남은 CLR `ΣL_i`, terminal56712, 필수 full RollbackStart3040을 보존한다. 변경하는 statement의 첫 효과 전에는 추가로 StatementRollbackStart3040+StatementFinish3040=`6080`을 확보한다. current Reserved root에는 앞으로의 Forward와 해당 CLR, 별도 Aging cleanup debt도 남는다. 성공 statement 끝의 WAL0이면 미소비6080을 다음 mark로 인계할 수 있다. 실패 statement는 추가6080과 그 suffix CLR를 소비하고, 앞선 root CLR+full-abort3040+terminal을 남긴다. 이후 새 statement의 첫 Reserve는 소진한6080을 같은 adopted descriptor에서 보충하며, 보충 admission 실패가 기존 full abort를 막아서는 안 된다.

Root cleanup debt 후보는 `Prune(C,h)+4×55528=311608+2C+128h`이며 C2228/h1의 최대는316192B다. CLR는 미사용 Prune 몫만 반환하고 ReleaseRole3+RootFree의222112B를 남긴다. Commit 뒤 미사용 CLR/rollback 반환과 retained Aging debt는 구별한다. Tx 사이 consumable/동시 peak를 sum해야 하는 부분과 원 Aging의 순차 shared peak max를 나누며, Job budget 합을 전역 ATTsum+Aging에 다시 더하지 않는다. 이 식은 실제 opcode source·기존 자원 한도의 수용 여부가 확인되기 전 grant가 아니다.

<a id="api-flow"></a>
## 7. API·오류와 권한 흐름

[공통 API 명세](dev3-module-boundaries.md#api-candidates)의 full lifetime/visibility·RowReservation/recovery consuming wrapper를 정본으로 수신한다. 아래는 그 설계의 호출 순서이며 컴파일·실제 공급을 이미 확인했다는 뜻이 아니다. `PreflightRow`는 첫 Reserve 전에 실제 schema/index/leaf 문맥을 제공한다. A의 `validate_insert(values,lane)`가 canonical candidate와 `RowInputPlan`을 만들고 `preflight.reserve(plan)`이 같은 source/schema/digest/index/공간을 재검사한 뒤에만 첫 효과를 시작한다. 검산한 candidate가 private 안에서 그대로 이동하므로 Reserve 뒤 `produce_insert(lane)`에 values를 다시 전달하지 않는다. `ReservedRow`는 같은 원 MutationPort/Owner/statement/table/private를 보유한다. `producer_lane`이 frame/before/after를 짧게 빌려주고, A가 반환하는 `RowProposal`은 untrusted scalar 후보다. `prepare(self,proposal)`는 동일 backing을 검산한 scalar cut만 저장하므로 self-borrow하는 Prepared나 원 Owner의 중복 mutable borrow가 없다. 이전 독립 `prepare_row/apply_row(RowChangeCandidate)` 초안은 이 consuming 경계로 대체한다.

| 소비자·순서 | 호출·반환 | 소유·실패 의미 |
| --- | --- | --- |
| 공통 진입 | 원 `DiskSession::split`→MutationPort/ReadPort/MaintenancePort; 실제 Catalog source→`CheckedTable` | 같은 core/pool/fence. raw BootstrapPlan/WriterNamespace/AtomicU8 노출0 |
| A 준비·예약 | exclusive `DiskSession`의 실제 Status/Free Undo System INIT→port/capsule→`begin_statement`→`preflight_row`→`validation_lane`→A `validate_insert(values,lane)`→`preflight.reserve(RowInputPlan)`→`ReservedRow` | type/null/inline/index/leaf 공간 실패는 첫 Reserve 효과0. stale source는 같은 Preflight를 돌려준다. 성공 뒤 원 actor/mark/table/issuer/slot/Job/whole debt의 단일 소유이며 scalar로 권한 재구성 불가 |
| A 생산·효과 | `reserved.producer_lane`→A `produce_insert`→`RowProposal`→`reserved.prepare`→`prepared.apply` | boundary의 consuming wrapper/loan 반환형 사용. C는 actual before/issuer/inventory/count/redo 검산 및 원 stage/install만 수행. 미stage Before는 같은 reserved/prepared 반환→reopen/retry/cancel이며 이미 완료된 Reserve를 되감지 않음. stage 뒤 실패는 Fatal·원 fence/의무 보존 |
| A 종료·복구 | C statement/terminal method, A logical CLR producer·typed Row loser handler | TerminalAssembly는 apply private 조립이다. 제품 공개 stage/wait/install/publish 조합이나 arbitrary receipt 생성 없음 |
| B 탐색 | read statement 또는 own statement→`ReadPort::row_operation(owner,&view,private)`→`op.page`→`page.leaf` | B가 leaf의 lower_bound/순서 탐색을 수행; C가 완성 point/cursor를 대신 반환하지 않음 |
| B 판정·반환 | `page.capture`→`op.status/missing_before`→B visibility→`copy_candidate` 또는 `release_candidate`→`finish` | source latch 아래 참조 취득; 복사 전에 view 재검산. op는 원 Owner borrow를 유지하고 짧은 page/source만 재대여; caller로 page slice 탈출 없음 |
| 공통 load | Residency/FlushRequired→원 MaintenancePort→같은 snapshot/statement 재시도 | 모든 partial physical/transient pin 반환 후 I/O. 새 snapshot을 조용히 발급하지 않음 |

오류 우선순위는 **입구의 origin/open/statement identity와 fence → 호출 argument/profile 및 operation의 caller-source credit 선예약 → actual source/schema/canonical/owner 검산과 A 값 검산 → effect admission/actual-before 재검사 → stage 이후 효과 판정**으로 고정한다. 이미 관측한 corruption/fence를 quota/NotFound로 낮추지 않는다. 값 타입/열 수/nullable 오류는 InvalidInput, 지원 밖 index/overflow/level/occupied before는 Unsupported, 실제 leaf 공간·quota 부족은 Capacity, transient 권한 충돌은 Busy, 비거주 준비는 원 Residency/FlushRequired, 유효 token의 own revision 차이는 CursorInvalidated다. 서로 다른 오류 enum은 원 cause/effect/certainty를 보존해 감싸며 문자열로 판단하지 않는다. 외부의 잘못된 RowId0·역전 range는 source I/O 전에 InvalidInput이고, 유효한 key의 부재는 전체 source/visibility 검산 뒤에만 NotFound다. OutputTooSmall/취소/EOF·진행 key·token 종료 순서는 [F02 owner 표](dev3-task-06-tree-rows.md#module-statement-cursor)가 우선한다.

초기 provisioning의 입력은 caller의 `prepare(counts)` 요청이며 C가 실제 census와 System INIT 비용을 검산해 요청한 Status/Free Undo 수만 확보한다. fixture의 일반 상계는 actor3·semantic rows12이고 capacity case만 actual 잔여 수를 별도로 요청한다. 성공한 actual 수량이 원 inventory이며52U를 무조건 선할당하지 않는다. active capsule/view/port가 있는 동안 추가 provisioning을 끼워 넣지 않는다. 부족하면 새 Row 효과 전 Capacity이며 기존 snapshot/reader를 없애고 준비 공간을 만드는 우회가 없다. 필요한 경우 모든 정상 소유가 끝난 뒤 exclusive 준비 경계로 돌아간다. 이미 admitted된 rollback/terminal은 추가 준비 없이 진행한다.

일반 occupied before의3pin을 이번 scope에 추가하지 않는다. [Missing-only 두 handoff](dev3-visibility-retirement.md#row-missing-traversal)와 [private425984/target9](dev3-resource-bounds.md#row-missing-module-budget)을 함께 소비한다. fixture도 더 넓은 constructor/loan을 받지 않는다.

<a id="producer-inputs"></a>
### 7.1 A producer의 전체 입력과 pure codec 연결

`ae6fb6d0a64087eab127d5b52bf640a633a91f39`에서 `row_record.rs`, `undo_record.rs`, `tree_page/edit.rs`, `semantic_catalog/{transition,frame,ledger,inventory}.rs`, `plan4_retention/catalog_live.rs`의 실제 인수를 대조했다. `RowId`와 before page만으로는 완전한 Forward/CLR를 생산할 수 없다. [경계의 producer 입력 API](dev3-module-boundaries.md#producer-input-api)가 아래 관측을 동일 source cut에서 빌려주는 것이 C0/C 공급 의무다. 구체 Rust getter와 lifetime은 그 경계 문서가 정본이며, 여기의 값 목록은 권한이나 두 번째 owner가 아니다. `RowProducerLane::parts`는 context·`RowControlObservation`·before9·schema·frame·after5·candidate를 반환하고, `source(role)`/`before_count`/`after_order`와 `RowProducerInput::{Insert,Clr}`로 유효 입력을 구분한다. recovery의 live `statement_sequence`는 None이며 원 Job에 저장된 statement/mutation 값은 actual before에서 검산한다.

| 입력 | 필요한 관측·borrowed lane | A의 실제 사용 |
| --- | --- | --- |
| preflight identity/schema | Profile/Namespace, 실제 table `ObjectHandle`/SchemaEpoch, 독립 `CatalogExpectation`과 schema cell 위치·길이, 실제 `TreeExpectation`(PageIdentity/kind/level/arity), 완결 Catalog census의 index 관측, leaf 공간 | schema decode, index0/level0/inline profile 검사, type/null/열 수·canonical 길이·leaf 여유 계산. expected identity를 candidate 자체에서 가져오지 않음 |
| issued row/transaction | actor `TxnRef`와 `StatusSlotRef`, issued RowId, U `UndoRef`/sequence, statement/planned mutation, 원 statement mark/rollback target, optional H `UndoLink`(reference+sequence), RootJob/TerminalJob ordinal | `RecordExpectation`, `RecordVersion`, `UndoCreator`, `UndoRecordExpectation`, Row target(owner/generation/RowId/arity0/key0), Txn stamp/logical version 및 prevTxn 인계 |
| physical before | 최대9개 before/proof의 실제 count/presence와 role→index/expected PageIdentity/kind/level/slot·offset, 실제 table anchor/leaf, 최대5개 after의 실제 count/role 순서 | leaf Insert/Remove, Status/Undo/Root/Job after, native slot/offset, `RedoTarget`, count/edge 검산. 같은 RootJob/TerminalJob page의 role alias를 physical index로 dedup하고 미사용 lane을 page로 decode하지 않음. H beforecount를 읽어 net0의 선행 decrement 가능성을 검산 |
| original control 관측 | 동일 control의 `jobs:&[[u8;1024];64]`, Tx descriptor `Option<&[u8;1536]>`, Aging descriptor `Option<&[u8;1536]>`를 lane과 같은 짧은 수명으로 immutable 재대여 | multi-root debt 합계, owner chain/descriptor prior digest·inventory·Aging aggregate와 Forward/CLR after를 A가 계산. descriptor/Job를 후보에서 역으로 before라고 믿지 않음 |
| frame/source | actual Profile, `FrameExpectation`와 제안 frame의 `WalFrameHeader`(stream DB/timeline/writer_epoch, next LSN, previous complete LSN, actor/previous Tx LSN), operation, INSERT의 sealed candidate 길이/SHA 또는 CLR의 실제 current row/Job cell 길이·projection/target mark·nextH | scope3 core·native/edges/counts/items/redo 및 frame encode. 새 WAL 번호를 A가 임의 할당하지 않으며 header 관측도 stage grant가 아님. operation과 무관한 optional 값 조합을 허용하지 않음 |
| caller bytes | actual schema lane, 검산한 canonical candidate, frame98304/after5의 writable lane, before9의 immutable lane | A가 원래 INSERT/CLR body를 생산. Reserve 후 사용자 values 재전달·다른 candidate 승인 없음 |

기존 `semantic_catalog::Inputs`가 요구하는64 Jobs와 두 descriptor는 `control.jobs()`/`control.row(slot)`/`control.row(AGING_SLOT)`의 실제 bytes다. 새 `RowControlObservation`도 이 원 저장소를 읽기만 재대여하며64KiB 복사본이나 별도 ledger를 만들지 않는다. Reserved/rollback task의 exclusive 원 control 대여가 유지되고 physical guards는 해제된 구간이다. A callback이 그 동안 원 owner를 재진입하거나 I/O하는 경로는 없다. lane 종료 뒤 C는 source revision·prefix·before/descriptor/Job census·inventory와 후보 전체를 다시 검산한다. reference carrier·getter scalar의 동시 생존 크기는 [원 source envelope](dev3-resource-bounds.md#row-missing-module-budget)에 포함하며 private425984/unused65536을 늘리지 않는다.

**공통 pure codec의 빠진 인수 adapter도 C0에 포함한다.** 현재 `row_record::{canonical_row_len,encode_canonical_row,decode_canonical_row,encode_row_record,decode_row_record}`는 `&[Column]`/`&[Value]`를 받는다. 새 borrowed 경로는 `RowSchemaInput::{Columns(&[Column]),Catalog(TableDefinitionView)}`와 `&[ValueRef]`를 받도록 같은 pure 구현에 연결한다. 기존 소유형 함수도 같은 규칙에 위임한다. Catalog view의 `len/column(i)`에서 type/nullable을 읽으면 되고 이름/값을 String/Vec로 복제하지 않는다. 기존 canonical tag·UTF8/Bool 검산·MAX_VALUE_BYTES·열 수/nullable·출력 오류 시 보존·성공 시 tail 보존·168B Row header/2048 inline 한도는 동일하다. A는 입력 검산 호출·leaf/row/Undo/Forward/CLR 조립을 맡고 C/B는 이 공유 pure decoder를 소비한다. 이 adapter를 완성 INSERT 제품으로 세지 않는다.

Missing Undo는 실제 `undo_record::validate_record`의 `before_kind=Missing/payload=None` 경로에서 columns를 읽지 않는다. 따라서 이번 A의 Missing Live/Releasing encode/decode는 기존 `&[Column]` 인수에 `&[]`를 사용하고 target/creator/sequence/edge/state 검산은 모두 유지한다. occupied RowValue before의 schema adapter나 일반3pin은 이번 최소 공급의 선행 조건이 아니다. CLR의 RowLeaf 현재 행은 위 borrowed Row decoder로 검산한 뒤 삭제하며, Missing이라는 이유로 current Row의 schema/identity를 생략하지 않는다.

<a id="independent-fixtures"></a>
## 8. C0/C와 단독 fixture

**C0**는 M3 수용 뒤 공급하는 실제 최소 컴파일 경계다. A/B 전용 본문은 이때 병렬 착수하며 서로의 stub·fixture를 import하지 않는다. C0는 pure canonical 값/후보 bytes, bounded leaf·Status/Undo의 **관측값**, 입력 오류·Busy/Capacity/Unsupported 선택, 비영속 호출 순서 기록을 제공할 수 있다. 값은 대역임이 type/module과 결과에 드러나야 하며 actual CheckedTable/RowReservation/SourcePin/WalRequirement/READY/commit receipt를 제조하지 않는다. C0의 pure producer·순서·오류 결과는 `C0_ONLY`로 남긴다. production owner가 필요한 compile signature에는 정의만 제공하고 actual constructor를 대역 성공으로 구현하지 않는다. test용 raw owner/unsafe/private factory 확대를 금지한다.

**C**는 C1–C4 실제 원 owner/CREATE/INIT/참조/statement/terminal/recovery/Row observer를 수신한 공급이다. C0에서 검증한 pure 기능은 코드 diff가 없으면 반복하지 않되, 저장/ACK/recovery/pin/가시성 인수는 처음부터 actual C source에서 새로 수행한다. C0 성공을 C 실제 연결 사례로 승격하지 않는다. 각 공급은 담당·source commit/tree hash·변경 파일·capability/미공급 영역·선택할 case를 기록한다. hash가 미래라는 사실은 설계 미정이 아니며 공급 시 빈값을 채우지 않은 source는 실행 선택하지 않는다.

### 8.1 공통 입력 schema와 신뢰 경계

계획 경로는 A `tests/fixtures/dev3-row-insert/`, B `tests/fixtures/dev3-row-read/`, 합류 `tests/fixtures/dev3-module-row-integration/`이다. 실제 input/oracle가 생길 때만 등록한다. 각 디렉터리는 다음 `input-plan.json` schema를 별도로 보유하며 독립 writer가 typed oracle를 작성한다.

| 필드 | exact 내용·한도 | 권한/정답 출처 |
| --- | --- | --- |
| `schema`/`case`/`source_kind` | `xadb-module-row-v1`, 아래 variant ID, `C0_ONLY`/`C+A`/`C+B`/`C+A+B` 중 하나 | selector와 case 일치, 상대 제품 source 부재는 dependency inventory로 별도 확인 |
| `formats`/`profile` | Profile3/Root3/Job3/base4/group2/scope3, profile ID1 기본;2/3은 현재 capacity tuple의 순수 검산 입력, single extent64/index0 | fixture가 old DB를 current라고 표기할 수 없음 |
| `table`/`values` | TableSpec의1..32열·typed bool/int64/UTF8/binary/null; canonical row length5..2048, 실제 schema SHA | raw target bytes에서 expected table/namespace를 추출하지 않음 |
| `actors`/`history` | 기호 T0..T2 최대3actor, S1..S8 최대8statement/actor, 일반 semantic case≤12행, 단계≤128; capacity case만 admission 성공 누적≤52행·다음1실패 시도 | 값·논리 expected set·own/snapshot 가시성은 독립 typed history에 고정. grant를 요구하는52행 약속이 아님 |
| `bindings` | 실제 CREATE/recovery에서 받은 DB/timeline/open/table/generation/schema/anchor; actual Begin/Reserve의 Txn/Status/U/RowId/Job/sequence/mark·LSN 및 digest | C가 만든 비복제 capability는 worker 안에만 존재. controller에는 관측 scalar/trace만 전달, scalar를 capability로 되돌리는 constructor 없음 |
| `candidate_template` | literal native/edge/count/inventory/core/redo recipe, 기호 binding 위치·길이의 whitelist, group≤94080B; 실제 ID 결합 뒤 exact frame SHA | A producer/B reader를 expected 생성기로 사용하지 않음. late binding은 identity·actual prefix/previous digest·그에 종속된 checksum만 허용, 값/outcome/연산/cut 변경 금지 |
| `schedule`/`fault` | 아래 fixed barrier 이름, 발생 ordinal, before/after 또는 정확 first-byte/complete-boundary, 단일 cut | 도달 trace가 없으면 NOT_REACHED/미완료. 예상값에 맞출 때까지 seed/cut를 바꾸지 않음 |
| `expected`/`budget` | ordered `(table,RowId symbol,typed value,logical version)`·visible/absent·count/owner/high-water·효과/certainty·선택 closure·자원 상한·시간 | 독립 reader/checker와 외부 ACK witness. 제품의 encode/visibility/observer 함수를 oracle에 호출하지 않음 |
| `provenance` | input/template/oracle hash, source/tree/ELF SHA, dependency file list, adapter feature, controller/worker argv, 원 결과 경로 | 실행 직전에 원자료 identity로 채움. native 정상 produce/recover/reenter는 같은 ELF의 서로 다른3PID |

literal schema는 **untrusted 후보**를 기술한다. 원 C ingress는 actual Begin/RowReserve/statement와 current before/issuer/owner/inventory/F05를 먼저 검산하고 생산과 같은 checked observer→원 stage/apply를 사용한다. adapter는 후보 bytes를 kernel page에 쓰거나 레코드 checksum 성공을 stage authority로 승격하지 않는다. test-only ingress의 feature/visibility는 [공통 API](dev3-module-boundaries.md#api-candidates) 범위에 한정한다. C의 실제 INIT는 별도 효과·예약·trace로 나타나며 fixture가 Free Undo page를 raw 생성하지 않는다.

### 8.2 C+A

fresh native namespace를 C의 실제 설치→CREATE→Commit→원 Catalog 선택으로 열어 실제 `CheckedTable`을 A에 넘긴다. native P1이 실제 CREATE와 A의 producer를 실행하고 P2가 현재-build recovery·새 ACK, P3가 재진입을 수행한다. Row INSERT 앞에 별도 CREATE-only recovery PID를 추가하지 않으며 P2/P3의 실제 선택·복구로 table identity와 Row를 함께 검산한다. A가 row/Undo/Forward/CLR를 생산하고 C가 검산·실행한다. B 본문/fixture/빌드 산출물 의존은0이다. 결과는 별도 typed-history checker와 page/WAL reader, controller가 실제 수신한 ACK witness로 검산한다. C0 pure tests만으로 실제 DIR03 완료를 주장하지 않는다. Row loser handler의 actual 호출·반환은 A 전용 source에 포함하고 C 복구 조립이 그 handler를 소비한다.

### 8.3 C+B: raw prebuilt DB로 생성 identity를 대신하지 않음

C+B native P1은 같은 build의 actual 설치·CREATE·Commit을 수행한다. P2의 actual recovery/선택이 그 table/anchor를 인증한 뒤 독립 literal actual apply/Commit을 수행하고, P3가 다시 실제 recovery→B point/cursor를 수행한다. independent literal input은 이 실제 binding과 C Begin/RowReserve의 권한 아래 Forward를 제안한다. A encoder/INSERT/CLR/automatic loser handler를 링크하지 않는다. C observer의 before 검산·원 effects/terminal을 통과한 실제 파일만 reopen corpus가 된다.

- **live fixture:** C Begin/Reserve→independent literal Forward→actual apply 후, own/foreign Active·fixed-q·publication gap를 B가 읽는다. 실패 statement/CLR/Prune 관측은 C의 동일 checked ingress에 독립 literal 전이를 넣는다. 이들은 fixture가 제품 A를 대신 구현한 증거가 아니라 B 입력 생산이며, A 인수에는 사용할 수 없다. 종료 시 literal CLR/AbortEnd 또는 actual Commit/cleanup을 완료해 원 소유를 반환한다.
- **reopen fixture:** C+B는 `recover_with_rows(RowRecoveryPolicy::RejectInitialLosers)`를 고정한다. 원 selected base와 complete WAL prefix를 실제 replay bank/64 Jobs로 재구성·검산한 최초 census에서 ATT Active/Aborting 및 canonical final ReservedInventory의 모든 종류(Row/Catalog/SystemPage)를 거절한다. Row head/rollback next가 None이거나 ATT0/RootReserved0인 SystemPage-only Reserved도 예외가 아니다. 검사는 physical recovery/REBASE/Cancel/Abort 이전이며 원 AnalysisSeal의 state_digest/source facts와 결합한 private admission이 Runtime/Recovered로 이동해 첫 next_action에서도 검산된다. 실패는 원 Analysis/Unsupported·lease fenced·effect=CallerMustResolve·database_effect=Unchanged·mutation0·READY0다. RowClr action이 없었다는 사실로 no-loser 정책을 대신하지 않는다. 이미 Committed/Aborted인 actor와 완결 CLR 뒤의 retained/compensated root의 cleanup처럼 새 logical CLR가 필요 없는 전이는 C 공급으로 허용한다. 상세 seal/반환 API는 [최초 analysis gate](dev3-module-boundaries.md#row-recovery-policy)를 따른다. 같은 ELF의 새 process와 실제 load/pin을 B가 소비한다.
- **관측 정답:** 기호 RowId를 실제 receipt의 scalar에 대입하되, expected order·q 관계·own visibility·고정 tuple는 입력 작성 때 이미 결정돼 있다. B 결과를 읽고 expected value/해당 root의 disposition/q를 고치지 않는다. 재시작이 loser를 제거하므로 reopen corpus만으로 own-write 인수를 대신하지 않는다.

C fixture ingress의 필수 known-bad12는 각각 유효 외부 SHA를 갖춘 단일 변조다: K01 foreign table/generation, K02 forged RowId≠reserved, K03 wrong U/generation, K04 duplicate planned mutation, K05 wrong predecessor H, K06 U incoming1(정답2), K07 H before count0에서 net0 이동, K08 row Transaction stamp≠actor, K09 native와 redo row bytes 불일치, K10 Reserved inventory 누락, K11 decrement된 terminal/full-abort debt, K12 valid old-format/scope2 frame의 Row ingress 제출. K01–12 모두 stage0·page 변경0·원 reservation 반환 또는 명시 cancel 가능을 요구한다. poststage fault와 이 precondition 실패를 섞지 않는다.

<a id="index-negative-input"></a>
### 8.4 index1 부정 입력과 authoritative Catalog source

현재 CREATE는 table-only이며 합법적인 existing indexed table을 만드는 제품 경로가 없다. `MCA01-V12`를 그런 table의 실제 INSERT 시험이라고 쓰지 않는다. 반면 `catalog_record::IndexDefinition`/decoder는 이미 존재하므로 **저장된 Index를 조용히 무시하고 INSERT하는 경로를 허용하지 않는다**는 거절 요구는 유지한다. V12는 다음 두 독립 변형으로 고정한다. P만 성공해도 S를 대체하지 못한다.

| 변형·입구 | 고정 입력 | exact oracle·증거 한계 |
| --- | --- | --- |
| V12-P, pure complete Catalog census | 독립 작성 Table 정의와 유효 Index record 1개. parent=외부 supplied table identity, mode=Nonunique, columns=[0], name/key/header/typed bytes 정상. caller index_count scalar가 아니라 모든 typed cells를 읽는 동일 pure census/profile predicate에 입력 | Unsupported, grant/effect0, `C0_ONLY`. 유효 Index 정의의 profile 거절 증거이며 실제 indexed DB/CREATE INDEX 지원 증거가 아님 |
| V12-S, 실제 FileIo→원 load 입구 | 실제 C 설치/CREATE/Commit/원 flush로 만든 table과 원 canonical history를 유지한다. clean CatalogLeaf를 정상 residency 교체로 비거주화한 뒤, 그 page의 `FileIo::read_at` 공급 bytes에 위 독립 Index cell을 추가한다. 외부 enclosing Catalog page identity/anchor/table binding·LSN은 실제 값이며 tree order/CRC/typed Index encoding도 정상 | actual load가 원 canonical image와 불일치를 발견하여 `Fatal(Load,Invariant)`·원 fence를 보존. load install/새 CheckedTable/Preflight/Reserve/INSERT0, WAL/page/issuer effect0. schema 오류/CRC 실패만으로는 이 exact cut 도달을 대체하지 못함 |

S의 canonical mismatch는 추측한 대체 정답이 아니다. 현재 `plan4_retention/load.rs::load_into`는 원 `history::validate_live` 후 canonical replay image를 정하고, `read_retention_home`의 두 실제 read 각각을 full-byte 비교하여 다르면 `RetentionPhase::Load/RetentionCause::Invariant`로 거절한다. `history.rs`의 image/home proof도 digest와 같은 LSN의 full equality를 요구한다. 변조 Index leaf가 preflight census에 도달하도록 RuntimeHistory/selected base/WAL proof를 함께 바꾸거나 private owner를 제조하지 않는다. actual source가 canonical 검산을 통과한 경우에만 C의 complete Catalog census가 table/profile을 확정한다. 그 census는 visible lookup 하나로 index 부재를 추론하지 않고 closed table-only source의 모든 typed Catalog cells를 검산한다.

fixture의 추가 입력은 `catalog_negative={variant:P|S,parent_binding,object_key,index_definition,source_page_binding,read_ordinal}`이며 모두 untrusted bytes/관측이다. S의 parent/table/source owner는 실제 CREATE receipt에 late-bind하고, index 자체의 발급·anchor가 인증되었다고 주장하지 않는다. 이것은 지원하지 않는 저장 입력을 읽는 부정 시험이지 정상 DB를 raw 조립하는 positive fixture가 아니다. 제품 입력인 FileIo bytes와 pure census 입력을 각각 기록하며 authority/pin/context factory는 추가하지 않는다. 원 read가 성공하여 Index를 무시한 INSERT까지 허용하는 mutant는 S의 효과0·fence oracle에서 실패하고, profile predicate가 Index를 허용하는 mutant는 P에서 실패한다. 인수 source에서 read path가 달라져 exact cut에 도달하지 않으면 NOT_REACHED로 남기고 설계를 재검토하며, 임의 오류를 새 PASS oracle로 고르지 않는다.

<a id="acceptance"></a>
## 9. 단독·합류 인수 inventory 동결안

다음은 **미구현 future selector**이며 실행 결과는 전부 NOT_RUN이다. M2에서 variant의 분모/입력/오류/cut/한도를 고정하고, 구현 때 해당 함수/fixture/worker를 연결한다. memory selector는 A `storage::row::insert::tests::<case_snake>`, B `storage::row::read::tests::<case_snake>`, 합류 `storage::row::integration_tests::<case_snake>`로 배정한다. native selector는 각각 `module-row-insert/<ID>`, `module-row-read/<ID>`, `module-row-integration/<ID>`의 exact case를 선택하는 신규 leaf로 배정한다. 아직 존재하지 않는 CLI flag를 실행 명령처럼 기록하지 않는다. 변형은 개별 선택 가능하게 작성하며 하나의 wrapper로 변경 없는 변형까지 재실행하지 않는다.

### 9.1 C0 전용 선택과 A의 실제 INSERT/rollback

C0의 첫 선택은 `MCA00-V01..06`과 `MCB00-V01..03`이며 아래 actual MCA/MCB와 별도 ID다. MCA00은 nullable Bool, Int64 MIN/MAX, 다중 byte UTF8, zero byte를 포함한 Binary, 잘못된 type/null/L2049 거절, H 없음/있음의 Forward·CLR 후보 literal 대조라는6개 입력이다. MCB00은 lower-bound의 miss/first/middle, own/foreign/q 관계의 순수 선택, invisible→OutputTooSmall/취소 뒤 진행 key라는3개 입력이다. 모두 C0가 준 **권한 없는 관측값**과 독립 literal을 사용하며 실제 pin/저장/ACK/READY 증거는0이다. C 공급 뒤 unchanged MCA00/MCB00를 반복하지 않고 아래 actual 연결 case를 처음 선택한다. MAX의5개 음성 입력도 MCA04에 명시한 C0 범위로만 센다.

| ID·변형 분모 | fixed 입력과 oracle | 원 요구 |
| --- | --- | --- |
| MCA01-V01..12,총13개 memory(1개 C0 포함) | V01..04 Bool/Int64/Utf8/Binary nonnull, V05 nullable null 최소L5, V06 mixed32열, V07 L2048, V08 두 statement×3INSERT 성공. V09 타입 불일치,V10 nonnullable null,V11 L2049. V12-P는 valid Index 정의의 pure census Unsupported, V12-S는 실제 FileIo stored Index 입력의 canonical mismatch 거절로 [입구/정답을 분리](#index-negative-input). V01..08 실제 bytes/logical identity/count/값 일치, V09..12 첫 effect0·각 exact typed 오류 | DIR03,T06-01/07/14 |
| MCA02-V01..06,6개 memory | S1 row a,b 성공; S2 c,d 뒤 실패→c,d만 CLR; V01 S3 e commit,V02 바로 full abort,V03 S3 e 뒤 full abort,V04 S2 첫 effect 전 실패,V05 S2 예약 후 Cancel,V06 3회 실패 suffix 뒤 새 statement 거절→기존 full abort 완료. 성공 이전 행/새 mark·mutation 비재사용·compensated q0와 source commit/abort·모든 count 검산 | NT09,3.09-R10/11/15 |
| MCA03-N01..09, 총16개 native | 아래 cut의 기존11개 분모를 유지하고 N03/N05 및 N09의 첫/중간/마지막에 RAM-only와 Durable 변형을 각각 배정하여5개 추가. 독립 ACK/UNKNOWN·winner/loser·원 PID/current-build 복구·첫/중간/마지막 CLR와 재진입 | DIR03,NT11/12/15,T06-15 |
| MCA03-R01..12,12개 native | 아래 REBASE/종료 barrier 각각1회. Row source에 대한 실제 C+A replay/undo·선택 closure·고정 peak 검산 | DIR03,3.12-R5,D3-COMP-02 |
| MCA04-V01..08,8개 memory(아래5개 C0 포함) + N01 native1개 | V01 Reserve/Cancel,V02 failed-statement CLR,V03 full abort 뒤 다음issuer,V04 RowIdMAX,V05 UndoSequenceMAX,V06 mutationMAX,V07 slot generationMAX,V08 RootJob generationMAX. V04..08은 fresh DB에서 u64MAX까지 발급한 것으로 꾸미지 않으며 C0의 checked-add/semantic-boundary 음성 입력이다(권한/READY/실제 abort 성공 증거0). 실제 ID 비재사용과 기존 abort 진행은 V01..03/MCA02/05에서 검산한다. N01 Commit→Prune/free→old-WAL회수→reopen→새INSERT→새ACK→reopen에서 issuer 비재사용 | 3.06-R4/10,NT03/09/12 |
| MCA05-V01..27,27개 memory | dimensions9=`WAL,protected,owned page,UndoSlot,RootJob slot,frame,private bytes,reference registry,files`; 각 최소 필요−1/동일/+1을 하나씩 바꾸고 나머지는 충분히 제공. under는 첫 효과0, equal/plus는 실제 Reserve→Forward→실패 statement→full abort/terminal 진행. 실제 총량 상한을 바꾸지 않고 test admission의 남은 몫을 제한. owned-page 변형은 선행 System INIT 포함, private는 고정 loan 길이를 위조하지 않고 원 자원 credit을 제한 | NT09/16,T06-14 |
| MCA05-V28..34,7개 memory | V28 한 statement6080소진 뒤 재admission거절·fullabort완료,V29 H net0 beforecount0거절,V30 최대허용 leaf까지 admission/다음split거절,V31 dirtyvictim flush 후 동일예약재시도,V32 API drop/cancel 권한반환,V33 전용FD/파일 상한,V34 DWB slot 부족에서 forward대기와기존rollback몫보호 | NT05/09/16 |

A의 live 지원은 표의 유한 변형에 한정되지 않는다. schema codec의 기존 지원 의미를 사용하며 새 Row glue가 추가한 경계와 실제 흐름을 검증하는 유한 중간 인수다. leaf 최대값은 원 actual admission에서 결정하며 빈 공간만 보고52행을 강요하지 않는다. 최대85개 최소cell의 representation/87reference 검산은 C static/observer leaf 경계 입력으로 별도 수행하며 물리 profile의52root 초과 grant로 가장하지 않는다.

### 9.2 B — actual fixture의 snapshot/point/cursor

| ID·변형 분모 | fixed 입력과 oracle | 원 요구 |
| --- | --- | --- |
| MCB01-V01..08,8개 memory + N01 native1개 | actual receipt로 결합된6행 a..f. V01 empty,V02 없는 gap,V03 first,V04 middle,V05 last point,V06 전체scan,V07 `[b,e)` scan,V08 empty/reversed range(End/InvalidInput 구별). N01 committed corpus reopen→allpoint+scan 일치. 순서/값/observation/중복누락은 독립 typed oracle | NT06,T06-01/12 |
| MCB02-V01..08,8개 memory | V01 ownActive,V02 foreignActive,V03 committedq>snapshot,V04 q=snapshot,V05 q<snapshot,V06 CLR Missing,V07 PrunedCommitted,V08 statusq>publishedQ의 gap. 동일 row에 point+cursor 정답 일치; V01/V02는 live fixture | NT10/11,T06-11 |
| MCB03-V01..08,8개 memory | V01 sourcecapture 뒤retire Busy,V02 U pin보유CLR Busy,V03 row nonresident,V04 Status nonresident,V05 Undo nonresident,V06 dirtyvictim FlushRequired→load→동일snapshot,V07 필요한U누락,V08 source Txn/U pin2 성공·추가claim Busy. 매반환 physical/transient/loan census 원값, corruption은Missing 아님 | NT05/10,T06-11 |
| MCB04-V01..14,14개 memory | V01 foreignDB,V02 oldopen,V03 tablegeneration,V04 schemaepoch,V05 endedstatement,V06 ownrevisionchange,V07 outputrequired−1→같은행retry,V08 outputexact,V09 invisible앞행뒤cancel→진행key보존,V10 EOF재호출,V11 EOF후ownmutation→Invalidated,V12 validSHAwrongRowId,V13 wrongUndotarget,V14 occupiedbefore scope밖. 오류/유효길이0/진행점은 F02표 | T06-02/08/12 |
| MCB05-K01..12,12개 memory | §8의 독립 literal ingress known-bad. 생산자A 없이 실제 C의 actual-before/권한 검산이 거절 | MI03,NT03/09/12 |
| MCB05-N01..06 native6개 | N01 actual Forward의 durable prefix가 있는 Active Row,N02 RowReserved-only,N03 Active/headNone/noReserved,N04 Aborting/nextNone/noReserved,N05 SystemPageReserved-only(ATT0/RootReserved0),N06 CatalogReserved. 각 실제 초기 census를 RejectInitialLosers에 제시→Analysis/Unsupported·lease fenced·effect=CallerMustResolve·database_effect=Unchanged·READY0, physical/REBASE/Cancel/Abort/새CLR0. 아래 입력 provenance/PID를 따름 | MI03,DIR03 범위 경계 |

MCB05-N01–06은 **거절 전용 각2PID**다. P1은 사례별 필요한 actual 설치/CREATE/INIT 및 C Begin/Reserve/System admission/RollbackStart에서 실제 prefix를 만든다. N05는 실제 fresh 설치의 ATT0/RootReserved0 상태에서 원 SystemPage 예약만 만들며 Row/Catalog actor를 인위적으로 지우지 않는다. N01의 Forward는 A 없는 independent literal checked ingress로 만들고, N02는 Forward 전, N03은 Begin 뒤 첫 Reserve 전, N04는 empty head에 대한 RollbackStart 뒤 AbortEnd 전, N05는 원 SystemPage reservation 뒤 INIT/취소 전, N06은 원 Catalog reservation 뒤 Forward/취소 전에서 닫는다. 모두 실제 whole apply/guard 해제 뒤 원 physical WAL write/sync로 exact preserved prefix를 확인한다. P2는 같은 ELF의 실제 selected base+complete prefix를 분석하여 처음부터 거절하고, callback으로 census/ATT/Reserved를 조립하지 않는다. C+B native fixture의 actual INIT-only/System reservation 공급도 C의 필수 준비 경계에 포함한다. 이미 거절해야 할 입력을 정상 READY·새 ACK로 진행시키는 세 번째 PID는 요구하지 않으며, 정상 recovery/새 ACK/재진입의3PID 및 A REBASE4PID 분모는 그대로다. lease/fence의 caller 의무 때문에 전체 effect를 Unchanged라고 기록하지 않고 **effect=CallerMustResolve, database_effect=Unchanged**를 분리한다.

### 9.3 합류 — 신규 실제 상호작용

| ID·변형 분모 | 실제 상호작용·oracle | 원 요구 |
| --- | --- | --- |
| MCI01-V01..14,14개 memory | V01 oldsnapshot→Acommit→oldscan불변/newscan새행,V02 ownBcursor→Ainsert→Invalidated,V03 같은table부재/range를읽은두Serializable writer에서첫Commit성공/둘째fullabort,V04 RC 같은statement안정/다음새q,V05 서로다른table writer둘다성공,V06 readonly관련Row변경충돌,V07 readonly관측Catalog에CREATE변경충돌,V08 failedstatement관측집합보존,V09 CheckedTable재사용시자동의존/stale거절,V10 RowCommit이catalogepoch를올리지않음,V11 physicalcleanup이논리epoch를올리지않음,V12 foreignactor/RowId CLR거절,V13 compensatedroot terminal전free거절,V14 publicationtable/receipt자원부족을Commitstage전에거절. [격리owner](dev3-task-10-concurrency.md#module-row-isolation)의종료규약을따름 | MI04,NT06/10,MCQ07 |
| MCI02-N01..03,3개 native | N01 A noforceACK→C recovery→Bscan,N02 S1성공/S2실패→Acommit→B에S1만,N03 full abort의 부분 CLR을 N09-Durable과 같은 exact prefix witness로 보존한 뒤 재장애→C+A recovery→B 부재. N01–03 각새ACK뒤세번째PID재진입 포함 | MI04,DIR03 |
| MCI03-V01..04,4개 memory | V01 Bsourcecapture의Txn/U pin중 A CLR/aging Busy,V02 Bdirtyload와Aapply 재검사,V03 Acommit durable/status설치뒤Q공개전 B oldsnapshot,V04 stage후오류로원fence→A/B둘다계속읽기/쓰기거절·cleanup보존. 각barrier1회,지원안하는동시write추가0 | MI04,NT05/09/10/11 |

<a id="fault-cuts"></a>
### 9.4 native cut와 유한 실행 한도

각 native case는 작은 실제 table·최대6행/3statement를 사용한다. 정상 produce/recover/reenter는 같은 ELF의 서로 다른3PID다. recovery 도중 다시 kill하는 R01–R11은 복구 완료 뒤 새 ACK와 재진입 검산까지 최대4PID를 배정한다. 필수 재장애/새 ACK 분모를 PID 수에 맞춰 줄이지 않는다. 기존 N01..08과 N09의 첫/중간/마지막11개 cut를 유지하며 N03/N05/N09 각각에 아래 두 보존 변형을 배정하여 **총16실행**으로 늘린다. cut당1회이며 process kill·명시 I/O fault·성공 sync 보존 모델을 구별한다. ACK는 controller가 응답을 실제 수신한 경우만 존재한다.

**Stage/whole apply는 durable prefix witness가 아니다.** 현재 source [`wal_io.rs::stage_verified_required`](../../crates/engine/src/storage/wal_io.rs)는 RAM의 complete slot과 exact requirement를 만들고, 실제 write/sync 성공은 `wait_durable`/`sync_claim` 경로가 확인한다. nonterminal actual apply는 stage→whole install→`release_checked`→wait 순서이므로 page/version/append/registry guard를 가진 partial install 지점에서 sync를 호출하지 않는다. 시험은 다음 필드를 `input-plan.json`의 `fault.preservation` 및 controller witness에 추가한다.

| 필드/변형 | 사전 조건과 실제 증거 | crash 뒤 oracle |
| --- | --- | --- |
| `D` prior preserved prefix | 원 runtime의 앞선 exact requirement에 대한 성공 sync, stream/file/LSN/end/digest와 실제 native write/sync trace. 준비 단계에서 그 prefix를 확인한 뒤 target operation 시작 | 선택 base+`D`의 issuance·앞선 CLR/commit은 반드시 보존 |
| `F` target frame | opcode·actual actor/U·frame LSN/start/end/digest. C가 actual complete stage에서 받은 원 sealed requirement와 결합하며 raw 숫자로 requirement 생성 금지 | 채택된 complete prefix에 F가 포함됐는지 독립 WAL reader가 판정 |
| `RamOnly` | target F complete stage/해당 RAM install cut에 도달했으나 F에 대한 native WAL write0, 관련 home write/flush0. 앞선 `D`만 durable이며 별도 drainer를 해당 cut 전에 진행시키지 않는 고정 schedule | F 유실 허용. 보존된 `D`부터 복구하고 유실된 F의 전이를 이미 수행했다고 계산하지 않음. 실제 syscall/파일 prefix가 이 조건과 다르면 NOT_REACHED/입력 불일치이며 Durable 성공으로 재분류하지 않음 |
| `DurableExact` | **같은 original runtime/requirement**의 actual physical write/sync 완료. nonterminal F는 whole install·guard 해제 뒤 확인하고 Commit은 원 terminal의 durable-before-install 순서를 유지. 반환한 sealed DurableWal의 `confirms(F)`와 stream/file/LSN/end, native successful sync trace 및 controller의 complete-prefix bytes/digest 대조를 함께 요구 | F가 selected complete prefix에 포함돼야 하며 그 의미 전이·issuance/CLR progress를 보존. 제품의 `synced=true` 문자열이나 readback만으로 성공 sync를 대신하지 않음 |
| `partial_home`(N05-Durable) | F DurableExact 뒤 원 DWB/flush 경로로 변경 target의 numeric 순 첫 home page 한 개만 실제 sync. 이 page의 identity/LSN과 다른 변경 home의 이전 LSN, DWB 상태·원 WAL-before-page 증거를 기록 | 현재 home 하나로 전체 group을 skip하지 않고 나머지 page별 REDO 후 loser undo. RAM partial install과 별개의 물리 partial-home cut임을 결과에 표시 |

`RamOnly`의 즉시 kill 지점은 미리 선택한 무할당·무 I/O fault marker만 기록하고 종료한다. guard 아래 controller callback·로그 파일 쓰기·대기·sync를 추가하지 않는다. controller는 사전 확보한 관측 영역/원 native trace와 남은 파일을 읽는다. `DurableExact`는 guards를 모두 놓은 실제 C 경계에서 원 physical drainer를 진행하는 시험 설정이다. 제품 매 Reserve/CLR의 강제 sync나 semantic Commit receipt 생성으로 확대하지 않는다. 이미 같은 requirement가 실제 durable이면 그 원 증거를 확인하고 가짜 sync를 만들거나 필요 없는 재-sync를 요구하지 않는다.

| cut ID·보존 변형 | 정확 사건·기대 |
| --- | --- |
| N01 | no-force Commit ACK 수신 직후, row home flush 전 kill. ACK의 exact Commit durable prefix를 확인하고 모든 ACK행 winner·새 ACK 뒤 재진입 보존 |
| N02 | Active RowLeaf를 실제 steal/home sync한 뒤 Commit 전 kill. 그 page flush가 사용하는 Row Forward의 원 durable requirement와 prefix를 확인. loser 행 제거·채택 issuance 보존 |
| N03-RamOnly | Reserve stage/whole RAM apply 후 physical write 전·Forward 전 kill. prior D에 Reserve가 없으므로 이번 예약/번호 소모가 유실될 수 있음; 없는 예약을 Cancel했다고 주장하지 않음 |
| N03-Durable | Reserve whole apply와 guard 해제 뒤 actual F write/sync를 확인하고 Forward 전 kill. 채택 Reserve의 Cancel·row 부재·해당 issuance 비재사용을 검산 |
| N04 | RowReserve까지 prior D를 보존한 뒤 Forward 첫 physical write의 첫 byte 뒤 종료. incomplete active tail 제외·D의 원 예약 복구/Cancel·row 부재 |
| N05-RamOnly | Reserve까지 prior D 보존. Forward complete stage 뒤 numeric 순 첫 변경 **RAM page** 설치 직후, F write/관련 home flush 전 kill. 유실 Forward를 REDO했다고 요구하지 않으며 D의 예약에서 올바르게 복구 |
| N05-Durable | Forward **whole RAM install 이후** guards 해제→actual F write/sync→원 DWB/flush로 첫 변경 home page만 sync→kill. exact F에 따른 나머지 page별 REDO와 loser undo를 검산. durable-before-partial-RAM-install이나 새 suspend API를 가정하지 않음 |
| N06 | Commit write 호출 시작 후 injected zero/error. caller UNKNOWN이며 recovery가 실제 채택한 complete prefix로 winner/loser 전체 outcome을 판정; controller ACK 없음 |
| N07 | Commit frame 전체 write 후 sync 호출 오류. UNKNOWN·실제 보존 prefix로 독립 outcome 판정; 임의 ABORTED 반환 금지 |
| N08 | Commit DurableExact+Status/Terminal whole install 후 Q 공개 전 kill. recovery winner·ACK 없음·새 snapshot에 완전행만 |
| N09-V01/02/03-RamOnly | 세 행 full abort의 첫/중간/마지막(k=1/2/3) CLR F를 complete stage/whole RAM apply하고 F physical write 전 kill. prior D는 RollbackStart와 앞선 k−1 CLR까지 실제 durable. 유실 F를 포함한 나머지4−k개의 logical CLR 재생성은 합법이며, D의 앞선 CLR를 새로 undo하면 실패 |
| N09-V01/02/03-Durable | 같은 kth CLR F의 whole apply·guards 해제 뒤 actual F write/sync를 확인하고 다음 CLR/AbortEnd 전 kill. 새 process는 D+F에 이미 보상된 k개 root를 **새 CLR로 재undo하지 않고** 남은3−k개만 수행. durable F의 물리 REDO는 허용. 세 번째 PID에서 새 ACK 보존 |

REBASE/종료 R01..12의 각 물리 장벽에서1회 kill한다.

| cut | 사건 |
| --- | --- |
| R01 | 새 base bytes를 전부 write했으나 sync 전 |
| R02 | 새 base sync와 readback 완료 |
| R03 | CURRENT 첫 copy의 `new/old` sync 완료 |
| R04 | 두 copy의 `new/new` sync 완료 |
| R05 | second-copy closure readback 완료 |
| R06 | selected old WAL unlink 후 dirsync 전 |
| R07 | old WAL dirsync 완료 |
| R08 | 새 epoch intent sync 후 create 전 |
| R09 | 새 epoch header sync 후 WRITER 공개 전 |
| R10 | WRITER+CURRENT WalOpen의 최종 dirsync/readback 후 UNDO 전 |
| R11 | UNDO 완료 후 VERIFY/STABILIZE 전 |
| R12 | READY 뒤 새 ACK를 controller가 수신 |

R01–R10의 입력은 nonempty loser의 Reserve/Forward와 이전 winner의 Commit을 actual DurableExact prefix로 보존하고 P1의 종료 전에 독립 대조한다. R11–R12는 그 복구 완료 state를 사용한다. RAM stage만 존재하여 loser가 사라진 입력으로 REBASE/UNDO 도달을 대신하지 않는다. P1이 source를 준비하고 P2가 해당 cut에서 종료된다. R01–R11의 P3는 끝까지 복구하고 새 ACK를 만들며 P4가 그 ACK를 검산한다. R12는 P3가 P2의 새 ACK를 검산한다. 이전 N=0 REBASE PASS를 Row source 결과로 재사용하지 않는다. 구현 때 물리 hook을 위 사건에1:1로 대응하며 실제 원 계약의 필수 단계가 더 세분되면 누락 cut를 추가한다. 기존 필수 분모를 줄이지 않는다.

실행 한도는 **memory case30초, native case180초(감사/정리 포함), 단독 C+A/C+B source 각각45분, 합류20분**이다. 총시간 한도에 도달하면 남은 case를 NOT_RUN/TIMEOUT으로 보존하고 재개 단위를 분리한다. 필수 분모·cut·PID를 줄이지 않는다. performance 반복은 없고 known-bad/cut도 각각1회다. 실패 수정 뒤에는 해당 diff/실패 case만 새 source로 선택하며 원 결과를 보존한다.

자원은 profile ID1의 원 owned/protected/WAL·buffer4MiB·private8MiB/transaction·전체64MiB·metadata8MiB·flush4MiB·maintenance12MiB·DWB16MiB를 유지한다. fixture/process는 원 namespace lease/FD recipe를 사용하며 별도 pool·대체 owner·대역 durability를 만들지 않는다. operation loan은 A/B 각각425984B다. Row recipe `P_R=(9,425984,0,16,5,1)`, 세션 전체 file≤7, 격리 backing5760B, caller 장기 handle envelope49152B와 source typed scratch/return carrier를 [자원 owner](dev3-resource-bounds.md#row-missing-module-budget)의 동시 생존표로 합산한다. source component65536B는 겹치는 writer+resident reader의 **합계** 한도이며 각자65536을 부여하지 않는다. fixture/controller memory는 별도 시험 비용으로 명시하되 worker의 실제 제품 scratch를 그 비용으로 빼지 않는다. case 끝에 namespace 파일/bytes·FD·소유/pin·원 loan 반환을 감사하며 forced Drop으로 누락을 감추지 않는다.

MCI는 source 합침 자체로 PASS하지 않는다. C+A/C+B에서 완료한 변경 없는 case·전체 workspace/하위 wrapper·기존 codec/model은 반복하지 않고 신규 interaction과 실제 합류 diff만 선택한다. MI01 실제 제품 구현 겹침, MI02 공용 의미 변경0, MI05 충돌/대기/재작업, MI06 별도 판정은 전환 계획에 기록한다. 이 중간 선택은 원 NT/T06/DIR03·DEV3 최종 S5 분모의 취소나 일괄 완료가 아니다.

<a id="open-questions"></a>
## 10. 설계 질문의 채택 상태와 수신 의무

**MCQ01–07은 아래 구체 설계로 독립 D2를 수신해 채택했다.** 전체 검토가 드러낸 producer 입력·borrowed codec·Index negative source·최초 C+B census gate의 선택도 포함하며, 이 문서의 설계 미결과 구현 NOT_RUN을 구분한다. M2 설계는 완료했으며 실제 공급·인수는 후속 의무다. source/hash·컴파일·runtime 수치는 실제 공급 때 검산할 구현/증거 의무이며 설계 선택을 그때로 미루지 않는다. independent review가 차단 반례를 찾으면 해당 결정을 다시 열고 A/B 양쪽에 같은 변경을 인계한다.

| 질문 | 동결한 설계안·근거 | 담당·후속 실제 수신 |
| --- | --- | --- |
| MCQ01 형식 | [현재→target dispatch](#format-dispatch), Row issuer@432·Job kind3/RowId@320·Profile3/base4와 exact scope/op/native/role. 현재 source의 format 분기/bytes와 직접 대조 | C/통합이 current genesis·선택·base/replay/CREATE adapter를 같은 source에서 구현. 기존 증거/순수 legacy 보존. 실제 F01 구현 byte의 독립 검토 필요 |
| MCQ02 상태/count | [상태표](#row-states)의 다중 head·mark·Compensated q0/source Commit 공존, H multiset net0·U Cut·head 순 Prune·terminal census. 성공 statement WAL0, 실패6080과 full abort3040 분리 | A는 Row/CLR producer, C는 observer/rollback 조립. MCA02/03·MCB02/03에서 실제 상태/count/재장애 검산 |
| MCQ03 C/A 경계 | [RowReservation](dev3-module-boundaries.md#row-reservation)의 preflight/validation→reserve consuming API·RowProducerLane/Proposal·Before/Fatal, live/recovery task/handler. [실제 encode 입력 대조](#producer-inputs)의 external expected identity·발급 scalar·physical role·원64 Jobs/Tx+Aging descriptor 재대여와 borrowed schema/value adapter를 C0/C에 공급. C는 실제 검산/효과/maintenance, A는 INSERT/CLR 후보 생산 | C0 signature→C actual owner→A handler 수신. 제품 동작을 C0 대역 성공으로 승격하지 않음; A 없는 자동 CLR handler도 없음. 새 scalar/borrow carrier도 원 source envelope 안에서 검산 |
| MCQ04 자원 | [자원 owner](dev3-resource-bounds.md#row-missing-module-budget)의9proof/5after·425984 분할, P_R=(9,425984,0,16,5,1), 세션 file≤7, 격리5760·handle49152, 준비 counts·root별 debt/6080·유한 queue·종료 rank를 채택 수치로 고정 | root/C가 새 type/Result/return/caller 동시 생존량·FD/registry cap을 해당 source에서 검산. bound 초과는 설계 수정/재검토 사유이며 증액 허용이 아님. MCA05 실행은 실제 C 수신 뒤 |
| MCQ05 독립 fixture | [입력 schema](#independent-fixtures)·known-bad12·PID·보존 prefix witness, [Index 음성 입력](#index-negative-input)의 pure profile/actual FileIo canonical mismatch 두 oracle. C+B의 RejectInitialLosers는 최초 actual selected base+complete WAL census의 ATT Active/Aborting와 모든 final ReservedInventory를 physical/REBASE/Cancel/Abort 전에 거절하고 AnalysisSeal/private admission에 연결. MCB05-N01–06으로 head/nextNone·System/Catalog Reserved도 관측 | C/경계가 채택한 exact API와 원 authority를 공급한 뒤 실제 시험. effect=CallerMustResolve/database_effect=Unchanged·fence 보존. RowClr 분기만 검사하거나 C0 pure 입력만으로 actual source 거절을 대신하지 않음. 채택 계약이며 구현/시험 NOT_RUN |
| MCQ06 F02/배정/인수 | [API/오류 순서](#api-flow), 원 statement/cursor·Missing2pin, A/B/C/검토 담당과 전용 경로, C0 허용 효과/실제 C 의무, [선택 variant/cut/시간](#acceptance) 고정 | M3 D2 수용 후 C0가 실제 공급되면 A/B 본문 병렬 착수. M5의 단독/합류 결과와 MI01–06를 원 분모대로 기록 |
| MCQ07 격리 | root의 [격리 owner](dev3-task-10-concurrency.md#module-row-isolation): 원 Q/publication+control/5760B 장부, Serializable Tx snapshot/read-set/readonly 종료, RC statement snapshot, 실제 변경 table만 epoch 공개, 실패 statement 관측 보존 | root/C·독립 검토. MCI01-V03..14의 실제 반례와 F05/source 수신. D2 채택 완료·runtime 인수 미완료 |

byte 산식·지원 범위·owner 신뢰 경계는 이 동결안의 일부다. source/caller census가 설계 상계를 넘거나 actual current-format/fixture/단독 source가 예상과 다르면 원 결과를 보존하고 M2 영향 행만 다시 검토한다. 일반 CRUD/SMO·전체 DEV3/S5를 불필요한 선행 조건으로 추가하지 않으며 구조 전환의 중간 선택을 그 전체 완료로 승격하지 않는다.

## 작성 기록

최초 읽기 전용 조사와 초안 작성 때에는 제품·시험·fixture·등록 파일 변경, 빌드·테스트·문서 verifier·commit/push를 수행하지 않았다. 그 뒤 [최종 독립 검토](../evidence/module-independence-survey-20261026.md#final-design-review)를 수신해 문서 전체를 채택했다. 실제 API 공급·A/B 병렬 구현·native 인수는 미완료다.

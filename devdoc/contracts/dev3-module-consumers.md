# DEV3 모듈 전환의 두 제품 소비자

2026-10-26 · `D3-MODULE-CONSUMERS-001` · **D2 PROPOSED / M2 미완료 / 제품·시험 NOT_RUN**

[전환 계획과 MI01–06](../plans/module-independence-plan.md#acceptance) · [공통 경계 제안](dev3-module-boundaries.md) · [첫 행 완료 계약](dev3-task-11-commit.md#first-row-integration) · [트리·행 정본](dev3-task-06-tree-rows.md) · [undo 정본](dev3-task-09-undo-rollback.md) · [시험 계획 DIR03](../plans/dev3-test-plan.md#direction-integration)

이 문서는 A의 **첫 영속 Row INSERT 거래**와 B의 **snapshot row point-read·순서 cursor**를 M5의 서로 다른 제품 기능으로 제안한다. 기준 source는 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`의 실제 파일을 조사했으며 작성 worktree HEAD는 `61b386cf972ac8a8444a7cd59f9a6a85a2bf24c2`다. 기준 이후 관련 제품 diff는 M2 채택 전에 통합 담당이 대조한다. 구체 형식·API·수치의 아래 선택은 제안이며 기존 소유 계약을 아직 대체하지 않는다. 작성 `/root/module_consumer_survey`, 통합·채택 `/root`, 공통 경계 `/root/module_boundary_survey`; A/B 실제 구현 담당·별도 D1/D2 검토자는 착수 때 기록한다.

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

이번 A/B의 제안 profile은 **첫 extent64 안, 실제 CREATE의 기존 level0 RowLeaf, canonical inline row 최대2,048B, index0**이다. 스키마는 기존 TableSpec의 Bool/Int64/Utf8/Binary·nullable·1..32열 의미를 보존한다. 실제 leaf의 남은 byte·Undo container·queue·transaction/F05 한도가 허용하는 여러 행, 여러 성공 statement, 한 statement의 여러 INSERT를 지원한다. 임의로 “한 거래 한 행”으로 줄이지 않는다. 수치 capacity는 새 F05 표로 동결하며 현재 Catalog의 pin9/private425,984B를 그대로 허용치로 복사하지 않는다.

- A의 입력은 원 CREATE/선택에서 발급된 `CheckedTable` 후보 권한, C의 실제 transaction/statement 소유, typed 값이다. 동일 statement의 부분 성공 뒤 실패하면 그 statement의 mark까지 undo한다. 앞선 statement 결과·잠금·full abort reserve를 남기고, 이후 새 statement 또는 full abort가 가능해야 한다.
- B는 supplied RowId의 point lookup과 RowId 오름차순 전체/범위 cursor를 제공한다. own INSERT는 같은 거래의 유효 statement에서 보이고, 다른 거래의 Active INSERT와 snapshot보다 뒤의 Committed INSERT는 Missing으로 판정한다. q 이하 committed 값, CLR로 제거된 값, committed stamp로 정리된 값을 구분한다.
- B는 호출마다 현재 anchor와 마지막 full RowId key에서 재탐색한다. 같은 transaction의 후속 mutation은 기존 cursor를 무효화한다. physical slot/page pointer를 호출 간 정답으로 보존하지 않는다. 세부 token·오류 진행점은 [statement/cursor owner 제안](dev3-task-06-tree-rows.md#module-statement-cursor)을 따른다.
- 현재 table에 index가 실제 존재하면 index를 무시하고 쓰지 않는다. nonzero index·overflow 필요·leaf split 필요·다른 tree level·UPDATE/DELETE/occupied before는 effect 전 typed Unsupported 또는 해당 용량 오류로 거절한다. 기존 API 오류 분류와 정확 우선순위는 M2의 F02 결정에 연결한다. 저장 손상은 Unsupported/NotFound로 낮추지 않는다.
- 지원 크기 검산 후에도 WAL stage/apply·commit·rollback 실패는 실제 효과에 따라 판정한다. no-force ACK, uncommitted steal, statement 실패, UNKNOWN, 부분 CLR 재장애, 정상 admitted quota에서의 복구와 새 ACK 의무는 [첫 행 완료 계약](dev3-task-11-commit.md#first-row-integration) 그대로다.

일반 SMO·overflow·UPDATE/DELETE·index·locator cache·모든 isolation 조합을 이번 M4 선행으로 추가하지 않는다. 이 제외는 해당 3.06/T06·DEV3 S5 의무나 기본 Serializable·Read Committed 계약의 취소가 아니다. 선택 범위의 실제 read/write set 검증은 MCQ07의 미결 설계이며, 유일 RowId 발급만으로 Serializable을 충족한다고 판정하지 않는다. 지원 profile 밖 이미 저장된 입력을 일부만 읽고 완료로 반환하지 않으며, 현재-format 지원 여부를 첫 효과와 사용자 결과 전에 검사한다.

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

C의 Row observer는 필요한 공통 신뢰 경계다. A는 그 grammar에 맞는 실제 forward/CLR producer와 사용자 변경 흐름을 만든다. recovery 조립은 A의 typed Row undo handler를 받으며, C+B의 완결 committed fixture에서 행 undo가 필요하지 않음을 검산할 수 있어야 한다. Row undo handler가 없는 source에 실제 loser가 나타나면 Unsupported/RecoveryRequired로 닫고 READY를 반환하지 않는다. A 없는 fallback으로 CLR를 생략하지 않는다. 이 handler의 exact 연결 signature·모듈 의존은 공통 경계 담당과 M2에서 확정한다.

<a id="row-issuer"></a>
## 5. RowId와 예약의 구체 제안

**추천안:** 원 global Root512를 명시 version3으로 확장하고 `[432,440)`에 `row_high:u64 LE`, `[440,512)`에 reserved0을 둔다. genesis는 row_high0이며 발급값은 checked `row_high+1`이다. table별 재사용 counter를 만들지 않고 DB/timeline 안에서 모든 table의 INSERT가 같은 발급 경계를 소비한다. RowUID는 table generation과 RowId의 조합으로 사용한다. Root의 `undo_high@392`·`object_high@400`·`commit_c@408`와 RowId는 서로 다른 typed 의미다.

RowReserve는 원 Active actor·statement, 이 profile의 actual empty Tx inventory, Free Undo slot의 actual generation, Free RootJob, table/anchor/leaf proof, 충분한 forward/CLR/terminal/cleanup 자원을 함께 검사한다. actor당 미완결 RowReserve는 하나이며 Forward consume 또는 Cancel release 후 다음 Reserve를 시작한다. 이전 성공 INSERT의 retained root는 inventory reservation이 아니라 실제 native/queue 소유로 남는다. 한 원자 그룹에서 다음을 채택한다.

1. `row_high+1`, `undo_high+1`, UndoSlot generation+1, RootJob generation+1. 뒤따르는 RowForward를 위한 계획된 RowId/UndoSequence/slot을 원 예약 owner가 소유한다.
2. transaction Descriptor의 `last_mutation_sequence+1`을 planned mutation으로 소모한다. RowForward는 그 번호를 실제 logical version에 쓰고 다시 증가시키지 않는다. Cancel/CLR/StatementFinish는 이 high-water를 낮추지 않는다. 이는 기존 Catalog Reserve의 last_mutation0 유지와 다른 **새 scope 제안**이다.
3. Tx의 `UndoSlot(U)` inventory item1, RowMissingRoot Reserved Job, exact 향후 debt. 기존 table anchor/RowLeaf는 이미 Allocated이며 PageToken/pair INIT를 다시 예약하지 않는다. Free Undo container의 실제 확보/INIT는 원 C 공급에서 먼저 수행하고 그 효과·비용을 숨기지 않는다.

성공한 Reserve 뒤 Cancel은 slot reservation·Job과 미사용 debt를 정리하지만 RowId/UndoSequence/mutation/slot generation을 반환하지 않는다. CLR·abort는 논리 행만 보상하고 high-water를 유지한다. crash 전 caller에게 성공 RowId를 반환하기 전에는 해당 Forward가 complete stage/apply되어야 한다. recovery는 선택 base의 row_high와 이후 complete Reserve를 재생하며, 미완결 tail·부분 apply는 원 공통 fence/복구 규칙을 따른다. Cancel·abort 결과를 이유로 초기 high-water를 다시 계산하지 않는다. old WAL 제거 뒤에도 선택된 base가 원 high-water를 보존해야 한다.

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

RowReserved/Retained/ReleasingCommitted/ReleasingCompensated는 원 상태 구분을 유지한다. Releasing은 실제 row를 다시 읽을 근거가 없으므로 hint/cell bytes/projection을 canonical zero로 지우고, U/sequence/table/RowId/statement/mutation/source·cleanup 증거는 Free까지 보존한다. RowReserved마다 unique U와 RowId, actor마다 Terminal 정확1을 요구하지만 Row root≤1을 요구하지 않는다. 원 queue64·nonfree≤55와 원 Status/Undo physical uniqueness는 유지한다. 첫 profile에서 한 Undo page의 slot0 하나를 쓰므로 여러 root는 여러 실제 container를 요구한다.

Row scope의 Compensated(c0)는 실패 statement를 처리한 뒤 Active, AbortingStatement 또는 AbortingFull actor와 공존할 수 있다. 그 뒤 앞선 성공 statement를 Commit하면 source는 Committed(q>0)/TerminalReadyCommitted이지만 **compensated root의 q는0**이다. 전체 abort면 source Aborted/TerminalReadyAborted다. 따라서 Row의 compensated cleanup은 actor가 어느 terminal outcome으로 끝났는지 검산하고 둘 모두에서 진행할 수 있어야 한다. Catalog scope2의 Compensated→AbortingFull/Aborted·headNone 제한을 Row에 복사하지 않는다. RootJob이 남아 있으면 source Status의 TerminalFinish를 막는 원 census로 수명을 보호하며 creator를 숨은 incoming edge로 세지 않는다.

XHG/XSG version2에 explicit scope3=`RowMissing`을 제안한다. scope2 Catalog observer가 이를 fallback으로 받지 않는다. header·두 descriptor(Tx/Aging)·OperationCore256·Native/Edge/Count/Inventory/Redo section layout은 [기존 group envelope](dev3-create-group-format.md)를 재사용한다. current Profile는 version3(같은 ID/용량 tuple), Root/Job는 version3, XRB1 base는 version4(동일2MiB layout에 새 Row state/issuer census)를 추천한다. 형식 폭·기존 backing을 늘리는 결정은 아니다. Row용 body/shape·current genesis/base/profile 수신을 함께 동결해야 한다. 현재 runtime에 old-format fallback을 추가하지 않으며 pure legacy decoder·원 증거를 보존한다. actual source discriminator의 숫자 충돌 여부와 모든 current selector의 일관성을 F01 담당이 채택 전에 대조한다.

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

<a id="row-shape"></a>
### 6.5 첫 profile의 target·WAL shape 입력

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
| ReleaseRole·RootFree | Root·RootJob·U, native합4320;3page |0/0/0,3P, single Aging|각`55528`|
| TerminalFinish | Root·TerminalJob·Status, native합3904;3page |0/0/0,3P, single Aging|`55112`|

성공 statement의 끝은 F02 token/mark의 정상 해제이며 status state가 바뀌지 않는다면 가짜 동일 before/after Native를 만들지 않는다. 위 StatementFinish3040은 AbortingStatement→Active 복귀다. 성공 statement 종료의 별도 영속 operation이 실제 owner 계약에 필요하다고 결정되면 exact 변화를 명시하여 표를 갱신해야 한다. Cancel/CLR에서 이미 소비한 row reserve/발급 bytes를 반환하지 않는다.

변경 page와 proof page는 다르다. Row 단계의 보수적 전체 target 집합 후보는 `{Root, RootJob page, Status, U, RowLeaf, allocator, tableAnchor, TerminalJob page, H Undo}`의 최대9개다. 같은 Jobs page 등 실제 physical identity 중복을 제거하되 read proof의 의미를 생략하지 않는다. source의 Root/Job 전체 census는 원 bounded bank와 version guard를 통해 검산하며 모든 root U를 동시에 pin하는 안이 아니다. 정확 pin9 충분성은 실제 준비/검산/적용 수명과 아래 commit census까지 C/F05 담당이 대조해야 한다. 기존 Catalog pin9와 같다는 이유로 성립을 가정하지 않는다.

첫 profile은 원 **한 Undo page당 slot0 root 하나**, actor당 실제 Status page 하나를 유지한다. Jobs는 page2..6에15+15+15+15+4=64개, cell1024B이며 nonfree≤55다. first extent의 page0 allocator·1Root·2..6Jobs·7CatalogAnchor·8CatalogLeaf 뒤 동적9..63의55page에서 실제 table pair·Status·Undo와 기타 allocated/Reserved를 모두 센다. table 하나와 actor 하나만 있고 다른 동적 점유가 없는 예시라도 U의 물리 상한은52(55−2−1)이며, 모든 source에52를 grant하는 규칙이 아니다. 남아 있는 CREATE root·기존 Status/Undo·pair reservation과 simultaneous owner를 실제 census로 차감한다. 원 queue cap·leaf 남은 bytes·F05 reserve 중 가장 먼저 걸리는 한도가 admission을 닫고 이미 admitted된 rollback/terminal은 계속 진행해야 한다.

statement 실패와 full abort의 같은3040B를 두 번 배정하지 않는다. Active transaction은 기존 retained root 각각의 남은 CLR `ΣL_i`, terminal56712, 필수 full RollbackStart3040을 보존한다. 변경하는 statement의 첫 효과 전에는 추가로 StatementRollbackStart3040+StatementFinish3040=`6080`을 확보한다. current Reserved root에는 앞으로의 Forward와 해당 CLR, 별도 Aging cleanup debt도 남는다. 성공 statement 끝의 WAL0이면 미소비6080을 다음 mark로 인계할 수 있다. 실패 statement는 추가6080과 그 suffix CLR를 소비하고, 앞선 root CLR+full-abort3040+terminal을 남긴다. 이후 새 statement의 첫 Reserve는 소진한6080을 같은 adopted descriptor에서 보충하며, 보충 admission 실패가 기존 full abort를 막아서는 안 된다.

Root cleanup debt 후보는 `Prune(C,h)+4×55528=311608+2C+128h`이며 C2228/h1의 최대는316192B다. CLR는 미사용 Prune 몫만 반환하고 ReleaseRole3+RootFree의222112B를 남긴다. Commit 뒤 미사용 CLR/rollback 반환과 retained Aging debt는 구별한다. Tx 사이 consumable/동시 peak를 sum해야 하는 부분과 원 Aging의 순차 shared peak max를 나누며, Job budget 합을 전역 ATTsum+Aging에 다시 더하지 않는다. 이 식은 실제 opcode source·기존 자원 한도의 수용 여부가 확인되기 전 grant가 아니다.

<a id="api-flow"></a>
## 7. API 후보와 권한 흐름

[공통 API 후보](dev3-module-boundaries.md#api-candidates)의 이름을 사용한다. 아래는 컴파일하지 않은 설계 호출이며 exact lifetime·visibility는 해당 owner의 미해결 질문으로 남는다. `RowInsertCandidate`는 A가 준비한 검산 대상 값이지 write capability가 아니다.

| 소비자·순서 | 제안 호출/반환 | 소유·실패 의미 |
| --- | --- | --- |
| 공통 진입 | 원 `DiskSession::split` → `MutationPort`, `ReadPort`, `MaintenancePort`; 실제 CREATE/선택 → `CheckedTable` | 같은 core/pool/fence의 borrow. BootstrapPlan·raw WriterNamespace·AtomicU8·registry를 소비자에게 공개하지 않음 |
| A 준비 | C statement/mark → RowReserve → A의 canonical row/Undo/leaf 후보 → `MutationPort::prepare_row_change(table,candidate,statement,loan)` | reserved ID/slot/Job와 private loan의 유일 소유. before 변경·Busy는 원 권한 보존 후 재준비, cancel은 명시 consuming 반환 |
| A 효과·종료 | checked `PreparedRowChange` → C stage/apply → statement 성공 또는 A CLR→C apply → `TerminalAssembly::{stage,wait,install,publish}` | stage 뒤 실패/Drop은 원 fence/의무 보존. 물리 DurableWal만으로 semantic commit receipt 생성 금지 |
| B 진입 | `ReadPort::snapshot`·owner statement → `acquire_row_source(CheckedTable,query,loan)` | actual anchor/RowLeaf guard. 기대 namespace/table을 row payload 자체에서 되읽지 않음 |
| B 판정 | source latch 아래 Row Txn/U capture → C Status/Undo resolve → B visible/Missing 판정 | [Missing-only 두 pin 수명](dev3-visibility-retirement.md#row-missing-traversal). source latch 해제 전 참조 확보, 임시 pin을 caller 결과의 수명으로 대체하지 않음 |
| B 반환/next | caller output과 observation, cursor의 full key·진행점 업데이트 → temporary refs/physical guard 반환 | borrowed page slice 탈출 금지. snapshot/token·cursor 실패 진행점·output 효과는 원 F02 의미를 적용 |
| 공통 load | Residency/FlushRequired → original MaintenancePort → 같은 snapshot/statement에서 재시도 | stale view는 재검사. 부분 pin을 잡고 I/O·quota를 기다리지 않음 |

일반 RowValue before의 `currentU+beforeTxn+nextU`3-pin 확대는 이번 Missing-only의 실제 필요가 아니다. B는 source Txn+U, CLR는 currentU+nextU로 닫는 두 그래프를 owner에서 검토한다. scope 밖 before를 받기 위해 registry 한도를 미리 확대하지 않는다.

<a id="independent-fixtures"></a>
## 8. C0/C와 단독 fixture

**C0**는 동결 계약·최소 컴파일 경계와 각 대역의 허용 효과를 받은 구현 기준이다. A/B의 첫 기능 검증부터 각각 C0+A·C0+B를 사용한다. C0 대역은 입력·typed 오류·수명·도메인 후보 검산만 할 수 있고 실제 저장/ACK/recovery를 PASS로 만들지 못한다. C0의 exact source·소유 경로·도구/fixture·제한은 별도 기록한다.

**C**는 선정 경계의 실제 공급을 수신한 인수 기준이다. 기존 CREATE가 만드는 actual 영속 table/anchor 및 해당 native/F05 의무, 원 page/visibility/transaction/terminal/recovery, Row 의미 observer·독립 fixture 입구가 필요하다. C0→C의 공급 diff와 A/B diff를 구분한다. 실제 효과가 필요한 시험만 공급 후 선택하며 이미 완료한 동일 시험을 반복하지 않는다.

### 8.1 C+A

fresh namespace를 C의 실제 설치→CREATE→Commit→현재-build recovery/선택으로 열고 발급된 `CheckedTable`을 A에 제공한다. A가 작성한 INSERT/CLR/종료가 실제 저장을 만든다. B 제품 구현은 넣지 않는다. 결과는 독립 typed history·WAL/page reader·외부 ACK witness로 검산하며 A 자신의 encode/replay 결과를 expected로 쓰지 않는다.

### 8.2 C+B: raw prebuilt DB를 대체로 쓰지 않음

첫 행 정본은 table/anchor·identity를 **실제 영속 생성·선택 경로**에서 공급하라고 요구한다. 독립성이 필요하다는 이유로 임의 handle·RAM table·사전 제작 DB image를 그 생성 증거로 대신하지 않는다. 아래 fixture 설계를 제안한다.

1. C+B와 같은 build의 native worker가 새 namespace에서 원 설치·실제 CREATE·Commit을 수행한다. 외부 controller가 그 결과와 table/anchor identity를 기록하고 새 process가 C의 실제 recovery/선택을 통과한다. 이 actual source closure가 corpus의 기대 owner 문맥이다.
2. 독립 fixture 작성자는 고정 typed tuple/history와 Row scope3의 exact offsets로 **완결 Reserve/Forward/Commit 또는 CLR/terminal frames**를 만든다. 생성기는 A producer나 B reader를 호출하지 않는다. namespace/table 바인딩은 actual CREATE 영수증, actor/예약 identity는 아래 C Begin/RowReserve의 actual 영수증에서만 받되 expected 값·edge·count·delta는 별도 원자료로 계산한다. hash·case/byte 범위·자원·cut를 실행 전에 고정한다.
3. 사전 지정된 시험 전용 입력 adapter가 실제 C Begin과 RowReserve를 통해 actor·statement·U/RowId/Job 예약 권한을 먼저 받는다. 이후 독립 literal Forward 등 untrusted candidate bytes를 C의 **제품과 같은 의미 observer**에 넘긴다. observer는 실제 before page/owner predecessor·예약 inventory/Job·issuer·table/anchor·native/edge/count·F05를 대조한다. 통과한 경우에만 원 owner 내부에서 typed 권한을 만들고 실제 WAL stage/apply/force·terminal bridge를 실행한다. adapter는 private constructor·raw page write·receipt 생성·fence 해제 기능을 갖지 않는다. own-write/다른 Active fixture는 이 live 경로에서 B가 관측한다. recovery READY가 loser를 제거한 corpus만으로 own-write 시험을 충족했다고 하지 않는다.
4. committed native fixture는 완결 실제 상태에서 닫고 같은 ELF의 새 process가 actual selection/analysis/REDO와 필요한 장벽을 통과한 뒤 B로 읽는다. frame grammar와 실제 I/O/owner 연결의 결과이지 A의 INSERT 실행 증거는 아니다. B의 snapshot gap/Active/Missing/cancel fixture도 같은 원 owner 위에서 고정 candidate schedule로 만들고, 실제 정리 frame까지 보존한다.
5. B 단독 source에는 A producer/automatic Row loser handler가 없다. native reopen용 fixture는 row loser가 없음을 C가 검산한 범위이며, 예기치 않은 loser를 만나면 실패해야 한다. A의 recovery·CLR 인수는 C+A에서 수행한다. committed retained root·page load와 B 자신의 읽기 실패/참조 반환은 B에서 실제 검증한다.

이는 아직 구현되지 않은 fixture 경계다. **actual observer가 없는 상태의 codec literal 주입은 이 설계를 충족하지 않는다.** fixture 입구의 exact capability/기능 feature·source inventory·native build 연결을 C 담당이 동결하고 별도 검토해야 한다. 실제 입력이 이 검산을 통과하지 못하면 원 오류를 보존하며 별도 pool·위조 owner·자동 q 교체로 우회하지 않는다.

<a id="acceptance"></a>
## 9. 단독·합류 인수 inventory 제안

아래 ID는 이 설계의 인수 행이며 **아직 실행 selector가 아니다**. 각 행을 구현할 때 test 함수/fixture/oracle·정확 변형·한도·원 source/binary와 연결한다. 원 NT/T06/DIR03 분모는 유지하고 현재 profile 밖 확대 의무는 S5/후속 작업에 남긴다.

| ID·source | 필요한 실제 관측 | 기존 연결 |
| --- | --- | --- |
| MCA01 C+A | 실제 CREATE/recovery table에서 첫 INSERT, 여러 행/statement·연속/건너뛴 발급 RowId, schema/type/null/inline 한도와 effect 전 거절 | DIR03, NT06, T06-01/07/14 |
| MCA02 C+A | 한 statement의 일부 INSERT 뒤 오류→mark까지 CLR, 이전 성공 statement 보존; 새 statement 뒤 full abort가 compensated suffix를 재undo하지 않음 | NT09, 3.09-R10/11/15 |
| MCA03 C+A | no-force ACK→새 process 보존, steal loser 제거, 실제 UNKNOWN, 첫/중간 CLR 뒤 재장애, REBASE/새 ACK 뒤 재시작 | DIR03, NT11/12/15, T06-15 |
| MCA04 C+A | RowReserve→Cancel·CLR·abort·old WAL 제거/reopen 뒤 RowId/UndoSequence/mutation 비재사용, counter MAX와 wrong issuer frame 거절 | 3.06-R4/10, NT03/09/12 |
| MCA05 C+A | 정해진 모든 opcode의 실제 자원−1/동일/+1, 반복 실패 뒤 admission 거절과 기존 rollback/terminal 진행, private/loan/pin/FD 반환 | NT09/16, T06-14 |
| MCB01 C+B | actual fixture/reopen의 empty/missing/first/middle/last point와 range/전체 순서·중복/누락·caller 결과/논리 observation | NT06, T06-01/12 |
| MCB02 C+B | own row·다른 Active·q보다 뒤 Commit·q 이하 Commit·CLR Missing·Pruned committed의 정확 가시성, status q>published Q의 publication gap | NT10/11, T06-11 |
| MCB03 C+B | source Txn/U capture 중 retire/CLR Busy, nonresident Status/Undo/row 실제 load, dirty victim FlushRequired 후 동일 snapshot 재시도·pin 반환 | NT05/10, T06-11 |
| MCB04 C+B | stale DB/open/table/schema/statement/cursor·own mutation invalidation·취소/출력 부족 뒤 진행점·EOF. 유효 checksum의 identity/row/Undo 손상을 NotFound로 숨기지 않음 | T06-02/08/12 |
| MCI01 C+A+B | B old snapshot/cursor→A INSERT/Commit→old 결과 유지·new snapshot 새 행. 같은 transaction의 B cursor→A mutation→CursorInvalidated | MI04, NT06/10 |
| MCI02 C+A+B | A의 실제 ACK→새 process C recovery→B point/cursor가 독립 typed 이력과 일치. A statement 실패/full abort 뒤 B가 앞선 결과/부재를 정확 관측 | MI04, DIR03 |
| MCI03 C+A+B | A commit/CLR와 B 참조/pool load의 정해진 barrier, shared fence와 오류 후 소유 반환. 지원하지 않는 runtime 동시 write를 시험 편의로 도입하지 않음 | MI04, NT05/09/10/11 |

MCI는 A/B source 합치기만으로 PASS하지 않는다. 사전 지정 assembly/연결 diff와 새 interaction case를 실행한다. C+A와 C+B에서 완료한 변경 없는 case, 전체 workspace·하위 wrapper·기존 codec/model은 반복하지 않는다. 합류에서 코드가 바뀌면 그 diff와 관련 실패·미실행 의무만 선택한다. MI01의 실제 제품 구현 겹침·MI02 공용 의미 변경0·MI05 충돌/대기/재작업·MI06 별도 판정은 전환 계획에 기록한다.

<a id="open-questions"></a>
## 10. M2 종료 전 질문과 해소할 근거

| 질문 | 현재 제안·미완료 부분 | 담당·종료 근거 |
| --- | --- | --- |
| MCQ01 형식 선택 | Profile3·Root v3 row_high@432·Job v3 kind3/RowId@320·XRB1 base4 동일2MiB·scope3 RowMissing 추천. 정확 dispatch·current genesis/selector·전체 native enum/role와 충돌 대조 미완료 | 형식/C 담당+통합. 기존 source/owner wire 표의 현재→목표 대조와 independent byte grammar 검토. fresh/current-build 정책·원 evidence 보존 |
| MCQ02 다중 head/CLR | Forward의 H net0와 CLR의 U.prev-txn Cut+head H 인계 추천. compensated suffix·Prune head순서·queue/census·Mark/StatementFinish의 모든 상태 교차 미동결 | transaction/undo 담당+독립 검토. Active/AbortingStatement/AbortingFull/Committed·Reserved/Retained/Releasing 표와 count multiset/재장애 반례 |
| MCQ03 C/A 경계 | C checked observer/replay, A live/CLR producer와 loser handler로 분리. handler 반환·오류/loan 수명·literal adapter 권한·C+B no-loser READY의 exact 조건 미동결 | 공통 경계 담당. [API 후보](dev3-module-boundaries.md#api-candidates)의 exact signature/visibility/import와 C+A/C+B 의존 표 |
| MCQ04 자원 | [opcode 산식](#row-shape)과 [private/target·종료 예약 제안](dev3-resource-bounds.md#row-missing-module-budget)을 작성. 논리 lock/descriptor peak 및 exact API 생존량의 검토는 미완료 | 자원 담당/root. 설계 숫자·선예약/반환·진행 rank를 D2 검토하고, 실제 source/caller census와 boundary 실행은 해당 M4/M5 effect 전에 수행. 기존 Catalog 결과를 새 PASS로 사용하지 않음 |
| MCQ05 actual fixture | C actual CREATE/recovery→independent complete frames→같은 validator/owner→native reopen 설계. generator/adapter/receipt·source/ELF 입력 및 actual Row recovery scope 미동결 | 독립 fixture/C/검증 담당. fixture 신뢰 경계·input schema·known-bad·native 일정과 C+B의 A 부재를 읽기 검토 |
| MCQ06 API/F02와 실행 배정 | owner statement/cursor·Missing2pin 제안 링크 사용. exact 오류 우선순위·지원 profile 한도·A/B 담당·전용 파일·C0/C hash·case selector 미정 | 각 owner+통합. M2 호출 설계/공급표를 상호 대조하고 D2 수용 뒤만 M4/M5 실행 |
| MCQ07 격리 수준 보존 | 같은 table의 부재/range를 읽은 두 거래가 서로 다른 RowId를 INSERT하면 유일 발급만으로 충돌을 검출할 수 없음. 원 publication owner의 bounded table/catalog epoch와 원 Commit coordinator의 검증을 후보로 검토 중이며 아직 owner 계약·API에 반영하지 않음 | concurrency/Commit/C 담당+독립 검토. Serializable의 Tx snapshot·read-only 종료 검증, RC의 statement snapshot, 실패 statement의 관측 집합 보존, durable 전 publication 자원 선예약, 실제 table 수명 검산과 추가 F05를 명세하고 반례로 검토 |

byte 수 계산은 외형적으로 기존 XHG `3776 + Σ(128+before_native+after_native) + 128E + 96K + 208I + Σ(redo)`를 대입할 수 있으나, exact 변경 page/Job 동시 배치·proof target·source loan·error return 생존량이 정해지지 않아 F05 완료가 아니다. initial row는 whole page Patch16,344B를 쓰는 안이며 기존 patch를 cell 크기로 임의 축소하지 않는다. 없는 source proof를 비용0으로 세거나 중복 page의 Native 수와 pin 수를 혼동하지 않는다.

위 질문은 선정 범위의 설계·배정 공백이다. 해소 전 M2를 완료로 판정하지 않는다. 반면 동결된 공급의 실제 구현·시험은 M4/M5 의무이며 M2 문서 단계에서 prototype·테스트를 요구하지 않는다. 일반 CRUD/SMO·전체 DEV3/S5 완료를 이 질문의 해소 조건으로 추가하지 않는다.

## 작성 기록

읽기 전용 조사 뒤 이 Markdown 초안만 작성했다. 제품·시험·fixture·등록 파일 변경, 빌드·테스트·문서 verifier·commit/push는 수행하지 않았다. 설계 검토·실제 API 공급·A/B 병렬 구현·native 인수 결과는 NOT_RUN이다.

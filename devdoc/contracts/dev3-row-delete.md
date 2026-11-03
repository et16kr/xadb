# DEV3 Row DELETE 첫 경로 — 형식·전이 설계

2026-11-03 · `D3-ROW-DELETE-001` · **설계 동결 / D2 ACCEPTED(3회차) / 2-a 형식층·2-b-1 C 전이층·2-b-2 제품 경로 구현(메모리 시험, §17)·2-b-3 시험 전용 관찰자 DELETE 대조(§18) / 3단계 native fault cut RDL-N01..N06 PASS(18 process)·RDL-J11(§19)**

[첫 Row profile](dev3-module-consumers.md#profile) · [RowMissingRoot 전이](dev3-module-consumers.md#row-states) · [WAL shape](dev3-module-consumers.md#row-shape) · [API 흐름](dev3-module-consumers.md#api-flow) · [Row 참조 이동](dev3-visibility-retirement.md#row-missing-traversal) · [Row 격리](dev3-task-10-concurrency.md#module-row-isolation) · [자원 정본](dev3-resource-bounds.md#row-missing-module-budget) · [Undo 형식](dev3-undo-format.md) · [live Row 정리](dev3-task-14-reclamation.md#live-row-retirement) · [명시 거래 실행기](dev3-module-boundaries.md#explicit-transaction) · [행 owner](dev3-task-06-tree-rows.md#algorithms)

명시 거래 실행기(PR #40, main `c7a6425`) 다음 단위로 사용자가 결정 카드에서 "Row DELETE 첫 경로"를 골랐다. 카드 문구는 "DELETE의 WAL·undo·CLR 형식을 새로 정하고, C 적용·복구와 A 실행기, 실제 C 위 시험까지 만듭니다. 형식 변경입니다."이다. 이 문서는 그 형식과 전이를 코드보다 먼저 정한다. 지금의 Row scope3는 모두 Missing before(INSERT)만 다루므로 DELETE는 "occupied before"의 새 전이 묶음이 필요하다. 아래 결정은 독립 D2 검토 3회차에서 ACCEPT됐다. 구현과 시험은 2단계부터이며 이 문서는 제품 PASS가 아니다.

<a id="scope"></a>
## 1. 범위와 제한

첫 경로는 **RowId 하나로 고른 행 하나를 지운다.** 결과는 지운 행 수 0 또는 1이다.

| 포함 | 제외(첫 효과 전 거절하거나 다루지 않음) |
| --- | --- |
| committed이고 이미 Prune된 행(`Committed(q)`, undo None, inline)의 삭제 | 같은 거래가 넣은 행의 삭제(Unsupported) |
| 자동커밋과 명시 거래의 DELETE 문장, statement rollback·full rollback의 복원 | Prune 전 행, 또는 RowId를 아직 root Job이 쥔 행의 삭제(Busy, §5) |
| commit 뒤 live 정리의 DeletePrune·ReleaseRole·RootFree | UPDATE, 술어·범위 DELETE, 여러 행 DELETE 문장 |
| 재시작 replay, 복구 loser의 DeleteClr, 복구 cleanup의 DeletePrune, base4 census | overflow 행, index가 있는 table, level≠0 leaf, split·merge |
| B 읽기의 tombstone 판정(own·committed·미정 거래) | 일반 wait-for·row lock map, SQL/DEV4 연결 |

제한의 이유: 대상이 `Committed(q)`이고 undo가 None이면 새 U의 before는 TxnRef가 없는 stamp와 previous-version Never만 가진다. 그래서 읽기는 지금의 Missing 경로와 같은 pin 둘(TxnPin+UndoPin)로 끝나고, [Row 참조 이동](dev3-visibility-retirement.md#row-missing-traversal)이 배제한 일반 occupied before의 3pin이 생기지 않는다. 미정 행을 거절하면 같은 행의 쓰기 충돌도 첫 효과 전에 거절된다.

단계(§13): **1단계** 이 문서와 독립 검토. **2단계** 제품 경로 전체(C 형식·관찰자·census, A 생산자·실행기·loser, B 읽기)와 메모리 시험. **3단계** native fault cut과 남은 시험. 처음에는 B 읽기와 복구를 3단계로 나누려 했으나, 그러면 2단계 build가 자기 읽기·복구가 손상으로 보는 영속 상태(tombstone, delete U)를 쓰게 된다(D2 1회차 지적). 그래서 한 build 안에서 쓰는 상태는 모두 읽고 복구할 수 있게 2단계에 모은다.

<a id="missing-only-inventory"></a>
## 2. 지금의 Missing 전용 가정

| 영역 | 지금 | DELETE가 바꾸는 것 |
| --- | --- | --- |
| opcode | Forward2·Clr4·Prune20은 INSERT shape(`5024+C`, `5024+2C`) | 새 opcode 3개(§3.4). 기존 셋은 그대로 |
| wire native 크기 | kind1(leaf)은 양쪽 `shape.cell`, kind3(U)은 양쪽 432 | opcode별 크기 규칙(§3.4) |
| Job3 | kind3 RowMissingRoot. `[284,288)` zero, Releasing은 `[280,320)` zero | `@284` 효과 byte(§3.3) |
| Undo codec | RowValue payload는 Inline만, Releasing도 body 유지. payload는 `&[Column]`로 검증 | Releasing RowValue는 None도 허용. 스키마 입력·header 검증 진입(§3.2) |
| U 관찰 | `material(job,…)`이 Missing U를 Job에서 만든다. 현재 U와 H 모두 `INITIAL_UNDO_CELL` | 효과1 U는 `432+L` 셀, before는 leaf 옛 행이나 U image에서(§3.2, §4.4) |
| RowLeaf | 모든 행 Inline. Forward=Insert, CLR=Remove, Prune=Replace(재stamp) | tombstone 행. DeleteForward·DeleteClr=Replace, DeletePrune=Remove |
| leaf 공간 | INSERT Reserve가 새 cell만 확인. edit scratch `ROW_EDIT_CELLS` 87은 entry≥189 B 전제 | 미결 delete의 복원 공간 예약(§4.3) |
| RowId | 모든 non-Free root Job 사이 유일(census.rs `verify_pair`, ledger) | 그대로 지킨다. 대상 RowId를 root가 쥐면 Busy(§5) |
| B 읽기 | capture는 Inline만, `read_missing`은 occupied before를 Unsupported | tombstone capture, RowValue before 읽기(§7) |
| 회수 | Prune(`MissingPrune`)·RootFree(`UndoFree`)·TerminalFinish(`TerminalFree`)가 retiring | DeletePrune도 retiring(`ValuePrune`, §8) |
| 복구 | loser CLR=Remove, cleanup=Prune, base4는 Missing U·Inline 행만 | DeleteClr·DeletePrune, base4가 tombstone·delete U·효과 byte 수용(§9) |
| 비용 | ledger Forward `91304+C+128h`, CLR `91096+C+128h`, cleanup `311608+2C+128h` | delete 식(§3.5) |

<a id="format"></a>
## 3. 형식

L은 canonical inline 길이(5..2048), C는 **지우는 옛 행의 tree cell** `180+L`(185..2228), h는 previous transaction H 존재 여부다.

<a id="tombstone"></a>
### 3.1 tombstone 행

[행 형식](dev3-row-format.md)의 기존 tombstone을 그대로 쓴다. XRW1 flags bit0 deleted, bit1 undo, payload tag 0, body 0이며 record 168 B, tree cell 180 B, slot 포함 entry 184 B다. 새 형식이 아니다. delete의 tombstone은 `stamp=Transaction(T)`, `logical=(T.id, planned_mutation)`, `undo=U`, deleted다. **`Committed` stamp의 tombstone은 leaf에 생기지 않는다.** DeletePrune이 tombstone을 Remove하기 때문이다(§4.1). 그래서 leaf에서 Committed tombstone이나 undo None tombstone을 만나면 손상이다.

tombstone 180 B는 `MIN_CELL` 185보다 작다. `Operation::shape`의 cell 인자는 옛 행 C이며 tombstone 크기는 shape 식 안의 상수 180이다.

<a id="delete-undo"></a>
### 3.2 delete U와 Undo codec

| 상태 | 필드 | 크기 |
| --- | --- | --- |
| Live | target Row(table/gen/RowId), before `RowValue`, before_stamp `Committed(q_old)`, before_logical 옛 logical, payload Inline 옛 canonical, edges `[txn Linked(H) 또는 Never, version Never]`, incoming 2, cursor 0 | `432+L` |
| Releasing cursor0..2 | before kind·stamp·logical 유지, **payload None**, incoming 0, txn Cut 또는 Never(cursor 2부터 Released), version Never(cursor 1부터 Released) | 432 |
| Releasing cursor3 | 위와 같고 before_stamp None(기존 codec 규칙) | 432 |

Live delete U는 slot0 하나의 셀 `[16352−(432+L), 16352)`에 둔다. Releasing U는 432 B로 돌아가 지금처럼 `INITIAL_UNDO_CELL`이므로 ReleaseRole·RootFree의 셀과 shape는 바뀌지 않는다. U의 before_stamp가 `Committed`라 `UndoBeforeStamp` edge가 없고, 대상 행의 undo가 None이라 previous-version은 Never다. 따라서 edge·count multiset은 INSERT와 같다(§4.2).

codec 변경은 둘이다.

1. **Releasing RowValue의 payload None 허용(추가).** 지금 `validate_record`는 Releasing RowValue Inline을 받고(U09–U12 golden이 그 예), None은 받지 않는다. Inline은 그대로 받고 None을 `releasing`일 때만 더 받는다. Live RowValue의 None은 계속 거절한다. [Undo 형식](dev3-undo-format.md)의 "Releasing에서도 body를 유지한다"는 이 profile의 delete U에 한해 개정한다. scope3 Row의 delete U는 Live→Releasing 전이(DeleteClr·DeletePrune)에서 body를 버린다. 다른 Releasing RowValue의 형식과 U09–U12는 바뀌지 않는다.
2. **payload 검증의 두 진입.** 지금 codec은 payload를 `&[Column]`(소유 String)으로 검증하고 A·C·base4는 Missing U만 다뤄 `&[]`를 넘긴다. [Undo 형식](dev3-undo-format.md)은 row before를 공급된 스키마로 `decode_canonical_row` 검증하고 opaque bytes를 before 값으로 승인하지 않는다고 정한다. 이 규칙을 지키며 두 진입을 둔다.
   - (a) **스키마 진입.** encode·decode·lookup의 `columns: &[Column]`을 `RowSchemaInput`으로 바꾼 진입이다. canonical까지 검증하고 `UndoRecordView`(before 값 포함)를 돌려준다. **before 값을 내줄 수 있는 것은 (a)뿐이다.** DeleteForward의 U 생산·관찰, DeleteClr 복원, DeletePrune의 before U, B `value_before`, loser handler, base4 census가 쓴다. 스키마는 각 경로의 실제 table source(`TableSource.schema`, read operation과 base4의 Catalog schema)에서 온다.
   - (b) **metadata 진입.** header와 길이(Live RowValue는 5..2048이고 셀 `432+L`, Releasing RowValue는 None)만 검증하고 metadata 전용 타입(`UndoRecordMetadata`를 넓힌 것: 상태, before kind, stamp 종류, logical 유무, payload 길이, edges)을 돌려준다. payload 접근자나 `live_before`가 없고 스키마 완료를 주장하지 않는다. 다른 table일 수 있는 H의 증명(C transition, C undo 관찰, A producer, §4.4)과 Releasing U의 ReleaseRole·RootFree material이 쓴다. Releasing delete U는 payload가 None이라 (b)와 (a)의 검증 범위가 같다.
   payload의 canonical 정당성은 DeleteForward가 쓸 때 (a)로 확인했고, 값을 쓰는 모든 곳이 다시 (a)로 확인한다. Undo 형식의 개정 문단이 두 진입을 함께 적는다.

<a id="job-effect"></a>
### 3.3 Job3 효과 byte

kind3(Row) root Job의 `@284`를 효과 byte로 쓴다. 0은 INSERT, 1은 DELETE이고 `[285,288)`은 zero다. Reserved·Retained·Releasing 모두 이 byte를 유지하고 Free·Terminal에서는 0이다. Releasing의 zero 규칙은 `[280,284)`와 `[285,320)`로 바뀐다. Releasing에서도 효과를 남겨 ReleaseRole·RootFree 관찰이 U의 before kind(Missing/RowValue)를 Job으로 교차 검산한다. RowId `@320`은 지금처럼 Releasing에서도 남는다.

| 효과 1의 필드 | 값 |
| --- | --- |
| `@280` cell_bytes | 옛 행 cell C(185..2228). Releasing에서는 0 |
| `@288` projection | 계획한 tombstone record의 projection(`Transaction(T)`, `(T.id, mutation)`, undo U, deleted). Releasing에서는 0 |
| `@320` RowId | 지우는 대상 RowId. 발급하지 않는다 |
| budget row3 | `steps 5`, `wal 311608+C+128h`(§3.5) |

`JobView`에 `effect()`를 더하고 비용 식을 `forward_wal()`, `clr_wal()`, `cleanup()`로 Job 한 곳에 모은다. ledger와 A producer는 이 식을 부른다. `has_previous_transaction`은 효과 1에서 `budget@400 == 311736+C`이다. INSERT 식과 숫자가 우연히 같아도 효과 byte가 식을 고르므로 구분된다.

별도 Job kind 4는 쓰지 않는다. `bytes[7] != 3` 검사와 Terminal scope 유도, census 경로가 모두 새 kind를 알아야 하지만 peak recipe(ROW_PEAK)는 같기 때문이다(§14).

<a id="delete-shape"></a>
### 3.4 opcode와 shape

| opcode | outer | 변경 native(합) | E/K/I, redo | frame bytes | 최대(C2228,h1) |
| --- | --- | --- | --- | --- | ---: |
| `DeleteForward = 8` | XHG Update, handoff | Root1152·Job2176·Status576·U `128+432+(432+L)`·Leaf `128+C+180`, 합 `5024+2C` | `(3+h)/2/1`, 5P | `91304+2C+128h` | 95888 |
| `DeleteClr = 9` | XHG Clr, handoff | U `128+(432+L)+432`·Leaf `128+180+C`, 합 `5024+2C` | `(3+h)/2/0`, 5P | `91096+2C+128h` | 95680 |
| `DeletePrune = 23` | XSG System | U `128+(432+L)+432`·Leaf `128+180+0`, 합 `5024+C` | `(3+h)/2/0`, 5P | `89496+C+128h` | 91852 |

세 공통 native(Root·Job·Status)는 INSERT와 같다. RowReserve(11)·Cancel(12)·ReleaseRole(21)·RootFree(22)의 shape도 그대로다. delete Reserve는 Root의 `row_high`를 올리지 않고 `undo_high+1`·`last_mutation+1`과 Free U의 다음 generation만 발급한다. native mask는 기존 Forward/Clr/Prune과 같다. 가장 큰 Row frame은 INSERT Prune 94080에서 DeleteForward 95888로 바뀌며 `FRAME_BYTES` 98304 안이다.

**native record 크기.** `validate_native_shape`는 지금 kind3을 양쪽 432, kind1을 양쪽 `shape.cell`로 고정한다. 새 opcode의 before/after body 크기는 다음과 같다(L=C−180).

| opcode | kind1 leaf | kind3 U |
| --- | --- | --- |
| DeleteForward | C / 180 | 432 / 432+L |
| DeleteClr | 180 / C | 432+L / 432 |
| DeletePrune | 180 / 없음 | 432+L / 432 |

wire decode는 opcode별로 C를 유도한다. DeleteForward·DeleteClr은 `(native−5024)/2`가 정수이고 185..2228이어야 하며, DeletePrune은 `native−5024`다. 홀수나 범위 밖은 Corrupt다. 기존 opcode의 decode는 바뀌지 않는다. 2단계는 [scope3 wire 기록](dev3-module-consumers.md#row-scope3-wire)의 존재 규칙(INSERT absent→present 등)에 새 opcode의 행을 더한다. 옛 binary는 8·9·23을 Unsupported로, 효과 byte가 있는 Job을 Corrupt로 거절한다. [버전 정책](../policies/version-compatibility-policy.json)의 개발 형식 규칙(호환 기준선 없음)에 맞다.

<a id="delete-costs"></a>
### 3.5 비용식

| 항 | INSERT | DELETE |
| --- | --- | --- |
| ledger Forward(Reserved root) | `91304+C+128h` | `91304+2C+128h` |
| ledger CLR(비terminal의 Reserved·Retained root) | `91096+C+128h` | `91096+2C+128h` |
| cleanup(Job budget) | `Prune+4×55528 = 311608+2C+128h` | `DeletePrune+4×55528 = 311608+C+128h` |
| CLR 뒤 남는 cleanup | `4×55528 = 222112` | 같음 |

delete cleanup의 최대는 313964 B로 기존 root당 최대 316192 안이다. ledger의 Forward·CLR 최대는 95888·95680으로 INSERT 최대(93660·93452)보다 크다. 2단계가 효과별로 바꿀 INSERT 식의 자리는 A [`row/insert/group.rs`](../../crates/engine/src/storage/row/insert/group.rs)의 `FORWARD_FIXED`·`CLR_FIXED`와 그 사용처, `semantic_row/ledger.rs`의 CLR·Forward 행, `transition/prepare.rs`의 Reserve `set_budget`, `retention_page/row/job.rs`의 `cleanup`·`has_previous_transaction`이다. 최대에서 유도한 설명은 [`RowLoanLayout::FRAME` 주석](../../crates/engine/src/storage/wal_io/plan4_apply.rs), [소비자 계약의 cleanup 최대](dev3-module-consumers.md#row-shape)와 `candidate_template`의 "group≤94080B" literal fixture 상계, scope3 wire 기록의 "최대 Prune94080B", [Row 쓰기 개정](dev3-module-boundaries.md#row-write-budget)과 [자원 정본](dev3-resource-bounds.md#row-missing-module-budget)의 94080·93660/tail 4644 설명이다. fixture 상계는 95888 DeleteForward fixture가 넘으므로 반드시 고친다. FRAME 구간 98304와 XOI 배치는 바뀌지 않는다(tail은 `98304−95888 = 2416` < 16384라 XOI는 계속 frame tail에 두지 않는다).

<a id="scratch"></a>
### 3.6 private loan과 scratch

frame·before9·after5·XOI·A scratch 구간은 그대로다. 바뀌는 것은 C scratch의 U 버퍼다. 지금 `ROW_REPLAY_SCRATCH_BYTES`는 9076 B이고 replay WORK_END tail 상한 `2×16384−64×352 = 10240`에 대해 여유가 1164 B다. Live delete U는 최대 2480 B라 U 버퍼를 2048 B 늘리면 상한을 넘는다. 기존 `record_scratch`(descriptor 앞부분, `MAX_RECORD` 2216 B)도 2480 B보다 작다. 2단계는 **상한을 올리지 않는다.** U header 432 B만 scratch U 버퍼에 만들고 payload L은 after page image(after5 구간)에 바로 쓴다. 복원 cell(DeleteClr)은 기존 `MAX_CELL` cell 버퍼에 만든다.

<a id="transitions"></a>
## 4. 전이

<a id="delete-states"></a>
### 4.1 상태 전이

| 전이 | Status·head | Row Job·U·leaf |
| --- | --- | --- |
| Reserve(delete) | Active/headH 그대로 | Free→RootReserved 효과1, C=옛 cell, RowId=대상, projection=계획 tombstone. U Free(g_new), inventory1. row_high 불변. after bank에서 대상 RowId를 쥔 다른 root Job이 없어야 한다(census) |
| Cancel | 그대로 | Reserved→Free. 기존 Cancel과 같음 |
| DeleteForward | headH→U, incoming+1 | Reserved→Retained. U Free→Live(§3.2). leaf `Replace(옛 행→tombstone)`. 옛 행은 `Committed(q_old)`·undo None·Inline·cell C여야 한다 |
| DeleteClr | AbortingStatement/Full의 head=next=U→H | Retained→ReleasingCompensated(c0). U Live→Releasing(payload 제거). leaf `Replace(tombstone→복원 행)`. 복원 행은 `Committed(q_old)`, 옛 logical, undo None, U의 payload이며 cell C |
| Commit·AbortEnd·StatementFinish·RollbackStart | 기존과 같음 | 효과와 무관 |
| DeletePrune | Committed(q), q≤Horizon, 대상 U가 현재 head | Retained→ReleasingCommitted(q,c0). U Live→Releasing(payload 제거). leaf `Remove(tombstone)` |
| ReleaseRole·RootFree | 기존과 같음 | 효과 byte 유지·교차 검산 외 같음 |

DeletePrune이 tombstone을 지울 수 있는 이유: `begin_retirement`가 q≤published·q≤horizon을 요구하므로 살아 있는 모든 snapshot과 뒤의 snapshot이 그 행을 지워진 것으로 본다. 키가 없는 것과 같은 답이다. RowId는 `row_high`로 재사용되지 않으므로 tombstone이 신규성을 지킬 필요도 없다. [행 owner](dev3-task-06-tree-rows.md#algorithms)의 "snapshot에 필요한 tombstone을 지우지 않는다"는 이 조건으로 지킨다.

<a id="delete-edges"></a>
### 4.2 edge·count

DeleteForward·DeleteClr·DeletePrune의 edge·count multiset은 INSERT의 Forward·CLR·Prune과 같다. tombstone은 RowUndo(→U)와 RowStamp(→T)를 갖고, 복원 행과 지워진 키는 둘 다 없다. U incoming은 0→2→0, T incoming은 tombstone 수만큼이다. H의 net0 multiset은 그대로이고, H가 효과1 root일 때의 증명은 §4.4다.

<a id="leaf-reservation"></a>
### 4.3 leaf 공간 예약

DeleteClr은 split 없이 `C−180` B를 되찾아야 한다. 그 사이 다른 거래의 INSERT가 그 공간을 쓰면 복원할 수 없다. tree page의 split 판정은 `entry_bytes > TREE_ENTRY_CAPACITY`의 합산식이므로 다음 불변식을 둔다.

> 각 RowLeaf에서 `TREE_ENTRY_CAPACITY − entry_bytes ≥ D`. 여기서 D는 같은 table의 **Retained** 효과1 root 전체(모든 actor)의 `Σ(C_j−180)`이다.

- INSERT Reserve(C preflight·`reserve_leaf`)는 새 cell을 넣은 뒤에도 불변식이 성립하는지 본다. 아니면 첫 효과 전 Capacity다.
- INSERT Forward 관찰도 같은 식을 검산한다. Reserve→Forward는 한 gate 보유 안에서 이어지므로 여기서 어긋나면 Capacity가 아니라 손상이다.
- DeleteForward는 여유와 D를 같은 양(`C−180`)만큼 늘리고, DeleteClr은 같은 양만큼 줄인다. DeletePrune은 D를 `C−180` 줄이고 tombstone entry 184 B를 비운다. INSERT CLR은 여유만 늘린다. 그래서 rollback·복구의 어떤 순서에서도 불변식이 유지된다.
- Reserved 상태의 delete는 아직 공간을 비우지 않았으므로 D에 넣지 않는다. 그 Forward가 비우는 양이 곧 복원 의무이고, Reserve→Forward/Cancel은 한 gate 보유 안에서 일어나기 때문이다.
- commit했지만 아직 Prune되지 않은 delete도 D에 남는다. 보수적이며 live 정리가 commit 직후 줄인다.

이 불변식은 leaf edit scratch 상계의 전제이기도 하다. `ROW_EDIT_CELLS` 87은 entry≥189 B를 전제로 한다(`16188/189+2`). tombstone entry는 184 B라 예약이 없으면 87개가 들어간다(87×184=16008). 모든 tombstone은 Retained 효과1 root의 것이므로 불변식 아래에서는 tombstone 하나가 적어도 `184+(C−180) ≥ 189` B를 차지한다. 관찰자와 base4는 scratch 상계에 기대기 **전에** 이 불변식을 검산한다.

<a id="previous-proof"></a>
### 4.4 previous root H의 증명

Forward·CLR·Prune(INSERT와 DELETE 모두)은 같은 actor의 바로 앞 Retained root H의 U를 net0 proof로 읽는다. 지금은 H의 Job에서 Missing U를 다시 만들고 셀이 `INITIAL_UNDO_CELL`이기를 요구한다. 이것을 효과에 따라 나눈다.

- 효과0 H: 지금과 같다.
- 효과1 H: 셀이 `[16352−(432+L_H), 16352)`(`L_H = H.cell_bytes−180`)이고, §3.2의 metadata 진입(b)으로 Job에서 오는 필드(creator, sequence, mutation, target RowId, edges, incoming 2, Live)와 before kind RowValue, before_stamp `Committed(_)`, before_logical 존재, payload 길이 `L_H`를 본다. q_old·옛 logical·payload 값은 Job에 없으므로 비교하지 않는다.

H는 다른 table의 root일 수 있고 그 스키마는 9 target에 없다. H의 U는 바뀌지 않는 net0 proof page이고 INSERT의 H 증명도 지금 metadata만 본다(Missing은 payload가 없어서 그것으로 완전했다). 그래서 진입 (b)로 충분하며 스키마가 필요 없다. 이 규칙은 C undo 관찰(`proof_page`), C transition의 `live_material`, A producer의 H 증명에 같이 들어간다. base4는 모든 table 스키마를 이미 가지므로 H의 U도 (a)로 본다(§9). 같은 거래에서 DELETE를 두 번 하거나 DELETE 뒤 INSERT를 하면 이 경로를 지난다(RDL-C·J).

<a id="classification"></a>
## 5. 대상 판정과 오류

C의 `preflight_delete`가 대상 행을 statement snapshot S로 판정한다. 다른 거래 T'의 상태는 Job bank에서 T'의 terminal Job(Leased, 또는 Ready의 outcome·commit_q)으로 보며 Status page를 따로 읽지 않는다. Commit·AbortEnd가 Job과 Status를 한 group에서 바꾸고 C의 ledger가 둘을 교차 검산하므로 이 판정은 Status와 같다. AbortEnd는 Retained root가 0일 때만 일어나므로 aborted T'의 `Transaction(T')` 행은 남아 있을 수 없고, C는 그것을 손상으로 본다. B는 Status로 판정하는 INSERT 규칙을 그대로 따라 Aborting·Aborted를 "before를 읽는다"로 보낸다(§7). 두 규칙은 같은 상태 공간에서 충돌하지 않는다.

| leaf의 대상 | 결과 |
| --- | --- |
| 키 없음 | 0행 |
| `Committed(q)`·undo None·Inline, q>S | 0행(S에 없던 행). Serializable은 commit의 table epoch 검증이 잡는다 |
| `Committed(q)`·undo None·Inline, q≤S, 대상 RowId를 쥔 root Job 없음 | 삭제 진행 |
| `Committed(q)`·undo None·Inline, q≤S, 대상 RowId를 쥔 root Job 있음(모든 actor·상태, own 포함) | Busy |
| `Transaction(own)` tombstone | 0행(이미 지움) |
| `Transaction(own)` Inline(own INSERT) | Unsupported |
| `Transaction(T')` tombstone, T' terminal Leased(Active·Aborting) | Busy |
| `Transaction(T')` tombstone, T' committed q'≤S | 0행 |
| `Transaction(T')` tombstone, T' committed q'>S | Serializable: SerializationConflict, ReadCommitted: 0행 |
| `Transaction(T')` Inline(INSERT), T' Leased 또는 committed q'>S | 0행(S에 안 보임) |
| `Transaction(T')` Inline, T' committed q'≤S | Busy(Prune 전 행) |
| Overflow payload | Unsupported |
| Committed tombstone, Committed 행의 undo Some, T'의 terminal이 없거나 aborted인 `Transaction(T')` 행 | 손상(Fatal) |

**RowId를 쥔 root Job.** census는 모든 non-Free root Job 사이에서 RowId가 유일하기를 요구한다(Releasing 포함). 그래서 삭제를 진행하기 전에 Job bank에서 대상 RowId를 쥔 root를 찾고 있으면 Busy로 거절한다. 두 가지 흔한 경우가 있다. 하나는 T0가 INSERT하고 commit해 Prune까지 됐지만 ReleaseRole·RootFree가 아직 끝나지 않은 행이다. live 정리가 끝나면 풀린다. 다른 하나는 같은 거래가 그 행을 지운 statement를 되돌린 경우다. compensated root는 거래가 끝날 때까지 남으므로 그 거래 안에서 같은 행을 다시 지우면 Busy다. census를 "Releasing root 하나와 live root 하나"까지 풀어 주는 방법은 첫 경로에서 쓰지 않는다(§14).

SerializationConflict는 효과 없는 statement 오류다. 명시 거래는 이어 갈 수 있지만 같은 table을 이미 읽었으므로 commit 검증에서 다시 충돌한다. q_old>S인 경우에도 Conflict를 내는 것은 보수적이며 commit 검증과 같은 결론이다. 0행은 효과 없는 성공 statement이고 table read bit는 결과를 돌려주기 전에 등록한다. 자동커밋 0행은 root 없는 Commit으로 끝난다(C의 Commit은 `reserved==0`만 요구한다).

오류 우선순위는 [API 흐름](dev3-module-consumers.md#api-flow)의 순서를 따른다. 입구 origin/open/statement와 fence → RowId 0 등 인자(InvalidInput, I/O 전) → statement credit 선예약 → table·schema·index·leaf level(Unsupported) → 위 판정 → Reserve admission(queue·inventory·F05의 Capacity; leaf 예약의 Capacity는 INSERT에만 있다. delete는 공간을 비운다) → 효과 재검사. 관측한 손상을 Busy·0행으로 낮추지 않는다.

<a id="api"></a>
## 6. API 흐름

INSERT의 consuming 경계를 그대로 따른다. 새 C 진입은 `preflight_delete(table, row)`와 그 결과의 `reserve_delete()`이며 `ReservedRow`·`producer_lane`·`prepare`·`apply`·`Cancel`과 statement/commit/rollback 메서드는 공유한다. `Change`에는 `ReserveDelete(&DeletePlan)`, `DeleteForward`, `DeleteClr`, `DeletePrune` 변형을 더하고, replay의 `ReserveAdopted`는 Job after에서 C·projection과 함께 효과·RowId를 채택한 뒤 나머지를 다시 계산해 비교한다. `Change` 96 B·`Inputs` 448 B 단언은 유지한다.

A는 tombstone cell(DeleteForward)과 복원 cell(DeleteClr)을 만든다. 복원 cell은 lane의 U before image에서 table 스키마로 만든다(§3.2 진입 a). C 관찰자가 각각 Job projection과 U를 독립 검산한다. A의 DELETE 본문은 module A 경로(`storage/row/insert/` 아래)에 둔다. 새 디렉터리는 gate 정책 파일 변경이 필요하기 때문이다. A 진입점은 `autocommit_delete_live`와 `transaction_delete_live`이며 자동커밋·명시 거래 실행기의 단계와 slot을 그대로 쓴다. 효과 전 거절과 실패 statement는 INSERT 실행기와 같은 경로로 끝낸다. 결과는 `Deleted(0|1)`이다.

<a id="read"></a>
## 7. B 읽기

`Committed` tombstone은 없으므로 B가 보는 tombstone은 항상 `Transaction(T)`이고 undo U를 가진다. C의 capture는 Inline 행과 함께 이 tombstone을 받아 TxnPin(T)와 UndoPin(U)을 발급한다(지금과 같은 두 pin).

| 판정 | 결과 |
| --- | --- |
| T가 own | 없음 |
| T committed, q_T≤S | 없음 |
| 그 밖(T Active·Aborting·Aborted, 또는 q_T>S) | C의 새 `value_before`로 U의 RowValue before를 읽는다. `q_old≤S`면 옛 값, 아니면 없음 |

`value_before`는 지금 `missing_before`와 같은 load·latch 순서로 U를 읽고, read operation의 Catalog 스키마로 payload를 검증해(§3.2 진입 a) 옛 canonical을 operation 복사 버퍼에 넣은 뒤 `q_old`를 돌려준다. tombstone은 RowValue U만, Inline `Transaction(T)` 행은 Missing U만 가진다. 다른 조합은 손상이다.

**reader와 writer의 배제.** Row 쓰기(preflight에서 Reserve·Forward/Cancel까지, rollback, live 정리)는 모두 terminal read gate를 닫고 이미 들어온 read operation을 기다린다. B read operation은 `row_operation`부터 `finish`까지 admission을 쥐어 capture·status·`value_before`를 덮는다. 그래서 capture와 `value_before` 사이에 DeleteClr·DeletePrune·RootFree가 끼어들 수 없다. `value_before`가 Releasing U를 보면 지금 `read_missing`처럼 손상(Fatal)이다. 별도의 재capture 경로는 두지 않는다. 시험은 기존 MCB03-V02처럼 읽기 중 DeleteClr이 gate에서 기다리는 경우로 확인한다.

<a id="retirement"></a>
## 8. 회수와 가시성

DeletePrune은 retiring group이다. `visibility/authority.rs`의 `RetirementScope`에 `ValuePrune`을 더하고 `MissingPrune`과 같은 규칙(q≤published·q≤horizon, U에 다른 pin 0)을 쓴다. Prune이 U payload를 버리므로 그 payload를 읽을 snapshot(S<q)이 없어야 한다. 조건이 안 되면 Busy이고 live 정리는 기존 `skip`으로 다음 root로 넘어간다. 정리 순서는 기존대로 actor의 현재 head부터다.

DeleteClr은 INSERT CLR처럼 retiring group이 아니다. reader는 §7의 gate로 배제되고, U payload를 필요로 하는 snapshot은 복원된 행(`Committed(q_old)`)에서 같은 답을 얻는다. ReleaseRole·RootFree·TerminalFinish는 그대로다.

<a id="recovery"></a>
## 9. 복구

- replay: C의 `observe_borrowed`가 DeleteForward·DeleteClr·DeletePrune을 관찰하고, Reserve 관찰은 효과 1을 받는다.
- loser: A의 Row loser handler가 효과 1인 Retained root마다 DeleteClr을 만든다. U는 Live라 payload가 있다. 스키마는 recovery의 table source에서 받는다.
- winner cleanup: C의 `prepare_row_recovery`가 효과 1 root에 DeletePrune을 고른다. 복구 중에는 snapshot이 없으므로 horizon 조건이 항상 성립한다.
- base4 census: leaf의 `Transaction` tombstone, Live·Releasing delete U, 효과 byte와 그 cleanup 식, §4.3 불변식을 받는다. base4는 Catalog를 모두 걸으며 table 스키마(`CatalogDefinitionView::Table`)를 이미 가지므로 Live delete U는 진입 (a)로 canonical까지 본다. loser의 payload가 여기서 걸러지지 않으면 뒤에 A의 `produce_clr`가 "재시도할 거절"로 돌려주게 되어 손상 판정이 늦어지기 때문이다. Releasing delete U는 어느 진입이든 같다. Committed tombstone과 불변식 위반은 손상이다. RowId 유일성은 지금 그대로다.

base4의 지금 `rows()`·`undo()`는 Missing 전용이라 효과1 Job의 상태마다 반대 규칙을 요구한다. 2단계는 효과1에 다음 규칙을 더한다(RowReserve는 따로 된 group이므로 Reserve와 DeleteForward 사이의 crash, statement DeleteClr 뒤의 crash가 실제로 생긴다).

| 효과1 Job 상태 | 키 | 행과 U |
| --- | --- | --- |
| Reserved | 있음 | `Committed`, undo None, Inline, cell = `job.cell_bytes`, logical은 actor의 것이 아님. U는 `INITIAL_UNDO_CELL`의 Free |
| Retained | 있음 | tombstone: `Transaction(actor)`, logical (actor, mutation), undo = `job.undo`, cell 180, projection = Job. U는 `[16352−(432+L),16352)`의 Live(`L = cell_bytes−180`), RowValue, before_stamp `Committed(_)` |
| ReleasingCompensated | 있음 | `Committed`, undo None, Inline. logical = U before_logical(모든 cursor), stamp = U before_stamp(cursor<3일 때만; cursor3은 codec이 before_stamp를 지운다) |
| ReleasingCommitted | 없음 | — |

역방향 scan 규칙: actor가 stamp한 tombstone 하나는 정확히 하나의 Retained 효과1 Job과 짝을 이룬다. Inline `Transaction(actor)` 행은 지금처럼 효과0 Job과 짝을 이룬다.

<a id="resources"></a>
## 10. 자원·F4·F05

- 새 target이나 pin은 없다. Row target 9개, private before9/after5, registry의 snapshot68·reference66과 두 pin이 그대로다.
- A의 DELETE 경로는 기존 F4 상한 안에 들어야 한다. [row_caller_budget.rs](../../crates/engine/src/storage/wal_io/row_caller_budget.rs)의 BEGIN 2,816·STATEMENT_START 3,584·STATEMENT_END 6,144·COMMIT 3,024·ROLLBACK_CALLER 1,536·ROLLBACK_STEP_RESULT 1,024·CLR_PRODUCER 19,456·RECOVERY_HANDLER 4,096·LIVE_ROW_CLEANUP 4,096, ROLLBACK_SITES(rollback.start·next·clr_lane·clr·clr_prepare·clr_apply·clr_reopen·finish·finish_statement, transaction.finish_aborted), 그리고 insert site 상한(`ROW_PREFLIGHT_CALLER_BYTES`, `ROW_WRITE_CALLER_BYTES`, forward의 `charge−producer`)을 delete site에도 같게 적용한다. 넘으면 원 항·여유를 깎거나 상한을 올리지 않고, A 축소 또는 사용자가 확인하는 양쪽 새 계약으로 간다.
- C의 새 관찰·생산 본문은 기존 Row write census(`writable::source_bytes`, 본문 44,304 B charge)에 넣어 검산한다. C 여유가 모자라면 같은 규칙으로 멈추고 묻는다.
- B 읽기 쪽: `value_before`는 B operation 안의 새 C 읽기 관찰(U load, 스키마 검증, 복사)이다. [`plan4_apply/row/read.rs`](../../crates/engine/src/storage/wal_io/plan4_apply/row/read.rs)의 C inventory(`missing` 항)에 `value_before` 항을 더하고, 기존 읽기 census S5R-C01/C02의 `full` 상계(undo_load·undo_location 단계)를 같게 통과해야 한다. B census 시험을 하나 더한다.

<a id="files"></a>
## 11. 변경 파일 inventory

| 모듈 | 파일 | 변경 |
| --- | --- | --- |
| semantic | `semantic_row/operation.rs`, `wire.rs` | opcode 8·9·23, shape, native 크기 규칙, decode |
| semantic | `semantic_row/transition.rs`, `transition/{prepare,cells,graph,frame,source}.rs` | Change 변형, delete Reserve·Forward·Clr·Prune 준비, U 셀 범위, 효과1 H, leaf 예약 |
| semantic | `semantic_row/{undo,leaf,jobs,ledger,status,core}.rs` | delete U material·H proof, tombstone leaf 관찰, 효과 byte 전이, 비용식 |
| codec·C | `undo_record.rs`, `retention_page/row/job.rs` | Releasing RowValue None, 스키마 진입 (a)·metadata 진입 (b), 효과 byte·식 |
| C | `plan4_retention/{row_preflight,row_write,row_forward,row_clr,row_rollback_select}.rs` | preflight_delete, RowId Busy, 예약, 판정 |
| C | `plan4_retention/row_start/{retire,recovery}.rs`, `visibility/{authority,row_retirement,retirement,pins}.rs` | DeletePrune 선택, `ValuePrune`, tombstone capture |
| C | `plan4_apply/row/{read,write,recovery,rollback}.rs`, `plan4_recovery/row_action.rs`, `plan4_retention/history/row.rs` | value_before, replay·recovery 연결 |
| C | `plan4_recovery_base/row/{actor,graph}.rs` | base4 census |
| A | `row/insert/{group,leaf}.rs`, `row/insert/driver/*` | delete 생산자·H 증명·실행기·loser(경로는 module A) |
| B | `row/read/**` | tombstone 판정 |
| 문서 | `dev3-undo-format.md`, `dev3-module-consumers.md`(scope3 wire 기록·`candidate_template`), `dev3-module-boundaries.md`(Row 쓰기 개정), `dev3-resource-bounds.md` | §3.2 개정 표시, 새 opcode 행, 최대 frame 설명·fixture 상계 |

<a id="tests"></a>
## 12. 시험 inventory

| ID | 단계 | 내용 |
| --- | --- | --- |
| RDL-F01..09 | 2 | literal: opcode decode·shape·frame(C 경계 185/2228, h 0/1), native 크기 규칙, 홀수·범위 밖 native 거절, 효과 byte 수용·거절(2, 잘못된 상태, `[285,288)`≠0), 효과별 cleanup·`has_previous`, codec Releasing RowValue None 수용·Live None 거절·U09–U12 그대로, metadata 진입 (b), ledger 행 |
| RDL-C01..12 | 2 | C 관찰: delete Reserve(row_high 불변, RowId 채택), DeleteForward·DeleteClr·DeletePrune 수용과 mutant(cell 크기, projection, payload, 옛 행 미Prune, 다른 RowId, split 필요), 효과1 H proof, leaf 예약으로 INSERT Capacity와 Forward 손상, RowId를 쥔 root(Releasing INSERT root, own compensated root)의 Busy |
| RDL-A01..08 | 2 | A 실행기: 1행·0행(키 없음, own tombstone, 안 보이는 행), Busy, Unsupported(own INSERT), SerializationConflict, 명시 거래 statement 실패 뒤 계속, F4 census |
| RDL-J01..08 | 2 | 실제 C 위: 삭제→commit→정리→읽기 없음, 삭제→rollback→옛 값, 명시 거래 삭제 둘·statement rollback, 삭제 뒤 INSERT(효과1 H)와 leaf 예약, 정리 전 RowId Busy(RDL-J11, §19), commit 뒤 재시작 replay |
| RDL-B01..10 | 2 | B: own tombstone, committed q≤S, 진행 중 거래의 옛 값, q_old>S, 읽기 중 DeleteClr의 gate 대기, Releasing U Fatal, Committed tombstone Fatal, Missing U의 tombstone·RowValue U의 Inline `Transaction` 행 Fatal, `value_before` 읽기 census |
| RDL-R01..10 | 2 | 복구(메모리): loser DeleteClr, winner DeletePrune, 효과1 Reserved·Retained·ReleasingCompensated·ReleasingCommitted의 base4 수용, Committed tombstone·불변식 위반·짝 없는 tombstone·비canonical delete U 거절 |
| RDL-N01..06 | 3 | native fault cut: DeleteForward·DeleteClr·DeletePrune 앞뒤의 crash(§19) |
| RDL-J11 | 3 | 실제 C 위 다른 거래가 쥔 행: Leased tombstone·Prune 전 행·RowId를 쥔 Releasing root의 Busy, 늦게 commit된 tombstone의 SerializationConflict(§19) |

<a id="stages"></a>
## 13. 단계와 PR

| 단계 | 내용 | 끝 조건 |
| --- | --- | --- |
| 1 | 이 문서, 독립 D2 검토 | 검토 ACCEPT, 사용자 병합 |
| 2 | §3–§9 제품 경로 전체, RDL-F·C·A·J·B·R | 변경 시험만 실행, gate record, 독립 검토, PR |
| 3 | native fault cut, 남은 시험 | 같음 |

2단계가 한 PR로 너무 크면 C 형식·관찰자와 A·B를 나누되, **영속 tombstone·delete U를 쓰는 제품 진입점은 그것을 읽고 복구하는 B·base4·loser와 같은 PR에 둔다.**

<a id="rejected"></a>
## 14. 기각한 대안

- **물리 삭제(Remove)를 DeleteForward로**: commit 전 다른 snapshot이 행을 잃는다. MVCC를 깨므로 기각.
- **기존 opcode 2·4·20 재사용**: native 식이 C와 2C로 달라 wire decode가 C를 유도할 수 없다.
- **Releasing에서 payload 유지**: ReleaseRole·RootFree의 U native가 L에 따라 커져 고정 55528 식이 깨지고, cleanup이 Releasing Job에 C를 남겨야 한다.
- **DeleteClr을 retiring group으로**: reader는 이미 gate로 배제되므로 얻는 것이 없고 rollback 경로에 새 거절만 생긴다.
- **DeletePrune을 재stamp로(`Committed` tombstone 유지)**: 180 B tombstone이 leaf에 쌓이고 B에 Committed tombstone 판정이 더 생긴다. horizon 조건 아래 Remove와 답이 같다.
- **Job kind 4**: §3.3.
- **RowId census를 Releasing root 하나 + live root 하나로 완화**: 정리 전 행과 같은 거래의 재삭제를 허용하지만 census·ledger 계약을 바꾼다. 첫 경로는 Busy로 둔다.
- **미Prune 행·own INSERT 행 삭제**: U의 before가 Transaction stamp나 previous-version을 가져 3pin과 version chain이 필요하다. 첫 경로 밖이다.

<a id="open"></a>
## 15. 2단계가 확인할 것

1. Live U payload를 after page image에 바로 쓰는 생산·관찰(§3.6).
2. 효과별 비용식 자리와 최대 frame 설명의 갱신(§3.5).
3. A DELETE 경로의 F4 census와 C 본문 census(§10).
4. leaf 예약 불변식을 INSERT Reserve·Forward와 base4가 같은 식으로 검산하는지(§4.3).
5. 효과1 H의 metadata 증명을 세 곳(C undo 관찰, C transition, A producer)이 같은 규칙으로 쓰고, base4는 (a)로 보는지(§4.4, §9).
6. base4 효과1 상태 규칙과 역방향 scan(§9).

<a id="implementation-2b1"></a>
## 16. 2-b-1 구현 기록

2단계를 둘로 나눴다. **2-b-1**은 C의 whole-group 전이층이다. **2-b-2**는 A 생산자·실행기·loser, scope3 per-native 관찰자(`semantic_row/{undo,leaf,jobs}.rs`), B tombstone 읽기, `ValuePrune`, replay·base4 복구다. 2-b-1에는 DELETE를 쓰는 제품 진입점이 없다. `Change::ReserveDelete`·`DeleteForward`·`DeleteClr`·`DeletePrune`은 시험만 만든다. 그래서 §13의 "쓰는 PR이 읽기·복구도 갖는다" 조건이 지켜진다.

- **전이(§4.1).** delete Reserve는 INSERT Reserve의 단계를 그대로 밟는다. row_high는 올리지 않고, 계획 tombstone의 projection을 Job에 넣는다. 대상은 `Committed`·undo None·Inline이어야 한다. DeleteForward·DeleteClr는 A의 group이라 C 생산자는 Unsupported로 거절하고, 관찰자만 둘을 받는다. DeletePrune은 C가 만든다.
- **U(§3.2, §3.6).** Live delete U의 432 B header는 scratch U 버퍼에서 `encode_undo_record_head`로 만든다. 값 L은 leaf before image에서 빌려 after page에 바로 쓴다. 상한 9076 B는 그대로다. DeleteClr·DeletePrune은 U를 진입 (a)와 table 스키마로 읽는다. ReleaseRole은 `encode_release_step`으로 cursor를 하나 올리며 before stamp·logical을 caller에 넘기지 않는다.
- **효과1 H(§4.4).** `check_outline`은 H의 셀을 진입 (b)로 보고, Job에서 오는 필드를 대조한다.
- **leaf 예약(§4.3).** D는 각 leaf 단계(INSERT Reserve·Forward·CLR·Prune, delete Reserve·DeleteForward·DeleteClr·DeletePrune)가 원래 bank에서 먼저 계산한다. `typed_leaf`가 `entries+D ≤ 16188`을 검산하고, 어기면 Corrupt다. INSERT Reserve와 Forward는 새 cell을 넣은 뒤에도 이 식을 요구하고, 어기면 OrderMismatch다. 이 검사는 예전 split 검사(`needs_split`, OrderMismatch)를 D만큼 넓힌 것이라 D=0이면 INSERT 결과가 예전과 같다. Forward에서 어긋나는 것은 §4.3의 "손상"이며, 관찰자는 그 후보를 OrderMismatch로 거절한다. Capacity 분류는 2-b-2의 C preflight가 한다.
- **RowId 유일성(§5).** 대상 RowId를 다른 root가 쥐면 after bank census가 `Queue(IdentityMismatch)`로 거절한다. Busy 분류는 2-b-2 preflight가 한다.
- **replay(§6).** `ReserveAdopted`는 후보 Job의 효과 byte와 RowId를 채택하고 나머지를 다시 계산한다. 다만 제품 replay(`history/row.rs`)는 2-b-2 전까지 효과1 RowReserve를 Unsupported로 거절한다. 8/9/23 replay와 base4의 효과1 규칙이 같은 단계에 들어오므로, 그 전에 replay가 DELETE root를 설치하지 않게 2-a의 귀납을 그대로 둔다.
- **효과와 opcode.** 공유 Row 연산은 Job의 효과와 opcode가 맞아야 한다(`op != op.role()`이 효과1과 같아야 함). 어긋나면 출력 전 IdentityMismatch다(INSERT Prune으로 delete root, DeletePrune으로 INSERT root).
- **census(§10).** delete 항은 11,269 B다. callee 최대는 ledger 11,811 B 그대로이고 prepare 지역은 +8 B다. 복구 Row 갈래 S1B는 25,731 B로 상한 25,743 B 안이다. RowWrite reserve C 여유는 771 B, row_lifecycle은 40,881 B 그대로이며, live/recovered peak는 64,016/65,392 B(memory), 64,024/65,400 B(native)로 바뀌지 않았다. 측정 모형(`transition_tests.rs` measured chains)은 U 쪽 사슬만 새로 더했다. DELETE leaf frame(`delete_source`·`delete_leaf`·`reserve_delete`)은 셈 모형만 있고, 그 최대 delete 11,269 B는 callee 최대 11,811 B보다 542 B 작다. `leaf_source`의 frame은 예전부터 따로 세지 않았고 새 `reserved` 인자 8 B도 그 여유 안이다. Catalog 쪽 목록(`catalog_before.rs`, `semantic_catalog/resources.rs`)은 새 `encode_record_header`·`check_row_before` frame을 이름으로 적지 않지만 둘은 더 깊은 사슬의 형제라 보수 합이 덮는다.
- **시험.**
  - 독립 저자 [`module-row-format/transition-delete/author.py`](../../tests/fixtures/module-row-format/transition-delete/author.py)는 N1 이력 H를 다시 돌린다. H의 모든 frame은 N1 literal과 바이트가 같아야 한다. 그다음 이력 D 30 group과 변형 11개를 만든다.
  - `transition_delete_tests.rs`(RDL-C)는 C가 만든 25 group을 literal과 바이트 단위로 비교하고, A 몫 5 group(DeleteForward ×3·DeleteClr·효과1 H 위 INSERT Forward)을 관찰하고, 변형 11개를 Corrupt로 거절하는지 본다. 그 밖에 borrowed 관찰자·생산자 쌍, `ReserveAdopted`, RowId 중복, 대상 행 거절(own INSERT 행·없는 RowId), 효과와 opcode 불일치, leaf 예약 경계(INSERT Reserve·delete Reserve·INSERT Forward·DeletePrune)도 시험한다.
  - Row arm replay 시험(`recovery/row_tests.rs`)은 H 뒤 D-516..D-520을 replay로 그대로 적용하고 D-521을 효과 없이 Unsupported로 거절하는지 본다.
  - scope3 per-native 관찰자 대조(N1 cross-check)에서 undo·jobs 관찰자는 효과1 actor의 group에서 건너뛰고 2-b-2에서 켠다. status 관찰자는 DELETE opcode group만 건너뛴다.
  - §12의 RDL-C mutant 가운데 후보 frame의 cell 크기·다른 RowId는 2-b-2의 A producer 시험(RDL-A)과 per-native 관찰자 시험(RDL-J)에서 다룬다. INSERT Forward가 정확히 `entries+cell+D = 16188`인 leaf에서 받아들여지는지도 A가 만든 frame이 있어야 하므로 RDL-A에서 본다.
- **undo outline.** 진입 (b)의 `UndoRecordOutline`은 payload 형태(None·Inline·Chunks)를 따로 돌려준다. Releasing delete U 증명은 형태 None을 요구한다.

<a id="implementation-2b2"></a>
## 17. 2-b-2 구현 기록

2-b-2는 DELETE를 쓰는 제품 진입점과, 그 진입점이 남기는 tombstone·delete U를 읽고 복구하는 경로를 한 PR에 둔다(§13). 메모리 시험까지이며 native fault cut은 3단계다.

- **A 생산자(§6).** group 생산자는 root Job의 효과 byte로 갈린다. INSERT 값은 INSERT root로, delete RowReserve가 봉인한 옛 행(`RowProducerInput::Delete`: cell C와 record projection)은 DELETE root로 가고, CLR은 둘을 모두 보상한다. DeleteForward는 committed·Prune된 행을 actor의 tombstone으로 바꾸고, U를 옛 값을 가진 Live(`432+L`)로 만든다. DeleteClr은 U의 Live before를 table 스키마(진입 a)로 읽어 행을 복원하고, U를 값 없는 Releasing으로 둔다. H 증명은 DELETE 선행 root의 Live U를 outline(진입 b)으로 읽는다. A의 leaf 검사(`leaf::inspect`)는 Inline 행과 함께 `Transaction` stamp와 U를 가진 tombstone을 받는다. 어느 거래의 tombstone이든 받고, 그 밖의 tombstone은 Unsupported다. 효과가 맞지 않는 입력, 다른 봉인 행, `Transaction` before를 가진 U는 출력 전에 거절한다.
- **C preflight·Reserve(§5).** `preflight_delete`는 Free U를 고르기 전에 statement snapshot으로 대상을 판정한다(`target_class`). 판정에 앞서 table 스키마를 읽고, table에 Index 정의가 하나라도 있으면 Unsupported다. 이 수는 INSERT가 `RowPreflightContext::index_count`에 쓰는 `table_index_count`와 같은 보수적 수(retired 포함)다. leaf level 검사(Unsupported)는 그 앞의 `row_shape`가 한다. 0행(NoRow)은 WAL prefix 재검사와 Catalog·table 읽기 기록 뒤에 답한다. `reserve_delete`는 옛 record의 XRP1 projection으로 DeletePlan을 예약한다. Forward·CLR 단계는 root Job의 효과로 DeleteForward·DeleteClr을 고른다. INSERT Reserve의 Capacity는 table의 Retained DELETE root가 쥔 D를 함께 센다(§4.3). leaf 예약의 Capacity는 INSERT에만 있다.
- **C census(§10).** RowWrite charge 44,304 B와 row_lifecycle 40,881 B는 그대로다. preflight 본문의 DELETE 항(`delete`: snapshot, Catalog·RowLeaf typed read, 스키마 decode, index census, `delete_class`, 또는 0행 끝)은 12,734 B로 본문 최대 항(finish 31,510 B)보다 작다. 그래서 preflight C 여유는 728 B로 바뀌지 않았다. RowReserve C 여유는 2-b-1의 771 B에서 595 B가 됐다. 같은 census에 `reserve_delete`의 DeletePlan 갈래와 delete draft의 projection 항, INSERT Reserve의 leaf 예약 D 계산이 더해졌다. 둘 다 S2W-C01/C06이 memory·native에서 본다.
- **A 실행기.** `delete_statement`는 INSERT 행 단계(flush 재시도, Reserve 거절 때 finish, Reserve 뒤 Cancel)를 그대로 밟고 0 또는 1행을 답한다. `autocommit_delete_live`·`transaction_delete_live`는 실행기의 statement frame을 `StatementRows`로 같이 쓴다. driver census의 site는 26에서 39로 늘었다(`delete.*` 13개). 실행기 census는 F4 고정 상한(§10) 안에 들어가며, 상한·원 항·여유는 바꾸지 않았다(RDL-A08, memory·native). DELETE 항 가운데 상한에 가장 가까운 것은 `delete.preflight`로 6,144 B 상한에 여유가 자동커밋 751 B, 명시 거래 968 B다.
- **jobs.rs 관찰자.** DeleteForward·DeleteClr·DeletePrune은 INSERT 역할의 Job 규칙을 따르고, 효과 byte가 opcode와 묶인다. Reserve의 after 효과는 계획의 효과여야 하고 Clr·Prune은 효과 byte를 남긴다. N1 cross-check는 이력 D의 모든 group에서 jobs.rs를 돌린다.
- **live 정리(§8).** DeletePrune은 `ValuePrune` scope(visibility authority 5)의 retiring group이며 `MissingPrune`과 같은 규칙을 쓴다. `row_cleanup_next`가 효과1 Job에 DeletePrune을 고르고 `retire_row`가 Row write로 보낸다.
- **replay(§9).** Row arm이 DeleteForward·DeleteClr·DeletePrune을 관찰한다(history Change 갈래, prepared row_write gate). 2-b-1의 "효과1 RowReserve는 Unsupported" 가드는 걷었다.
- **B 읽기(§7).** C capture는 Inline 행과 함께 DELETE tombstone(`Transaction(T)`, U 있음)을 받는다. 후보는 deleted이고 canonical이 없다. 새 C `value_before`는 `missing_before`와 같은 LOCATION·LOAD 순서로 U를 읽고, operation의 Catalog 스키마로 진입 (a) 검증을 한다. tombstone U의 모양은 `tombstone_before` 한 곳에서 판정한다. 받는 것은 `Transaction(T)` tombstone 아래의 T·Status 세대·mutation의 U이고, before가 RowValue·`Committed(q_old)`·logical·Inline 값인 것뿐이다. 그 밖과 Releasing U는 Fatal이다. `Committed` tombstone은 capture가 먼저 거절하지만 `tombstone_before`도 따로 거절한다. 옛 canonical은 operation 복사 버퍼에 넣고 row·q_old·logical을 돌려준다. `copy_before`는 B가 `q_old≤S`를 판정한 뒤 복사한다. Inline `Transaction(T)` 행의 Missing U는 지금처럼 `read_missing`이 읽는다. 읽기 census S5R-C01/C02에 `value_before`의 load·location 단계(C 항, B 호출자 항)를 더했고, 모든 단계가 바뀌지 않은 34,586 B 안이다. `value_before` 단계 가운데 가장 큰 것은 cursor의 load 34,460 B로 여유는 126 B다.
- **복구(§9).** C 복구는 committed 효과1 root에 DeletePrune을 고른다(RowWrite class, 닫힌 facade 검사는 그 commit Q로). A 복구 handler는 같은 `produce_clr`로 DeleteClr을 만든다. base4는 효과1의 Reserved·Retained·ReleasingCompensated·ReleasingCommitted를 §9 표대로 받는다. Releasing Job은 cell 길이를 남기지 않으므로(`cell_bytes` 0) ReleasingCompensated 행은 cell 크기 대신 U before의 logical·stamp로 확인한다. base4 issuer 관계는 `Objects + RowIds ≤ undo_high`로 바뀌었다. delete Reserve는 UndoSequence만 발급하고 RowId를 발급하지 않기 때문이다. delete Reserve 수를 영속으로 세는 곳이 없으므로, 이 관계로는 UndoSequence가 위로 새는 것을 더 이상 잡지 못한다. base4-closure fixture 개정 d1은 issuer-undo-low를 거절하고 issuer-undo-high를 받는다. 부모 literal은 그대로다.
- **시험.**
  - RDL-A01..A08: literal 대역 위의 A 단계와 F4 census. A가 만든 이력 D의 5 group(DeleteForward ×3, DeleteClr, 효과1 H 위 INSERT Forward)은 독립 literal과 바이트가 같다.
  - RDL-B01..B05: C0 literal tombstone cell(`tests/fixtures/module-row-read/make_delete_cells.py`). `tombstone_before` 단위 시험은 받는 모양 하나와, 거절하는 U 변형 열한 개(chunk 값 포함)·tombstone stamp 변형 셋(다른 거래, 다른 Status 세대, `Committed`)을 본다. C0 cell `delete-cells.txt`는 `make_delete_cells.py`가 바이트 그대로 다시 만들며, `driver-cells.txt`와 달리 동결 manifest는 두지 않았다.
  - RDL-B07..B09, 실제 C 공급 위(`row_join/retire/delete_read.rs`): 명시 거래가 지운 행의 상주 page를 한 곳 고친 뒤 새 statement의 point read가 Fatal(CallerMustResolve, fence, 출력 없음)인지 본다. Releasing U는 Census·Undo, U 없는 `Committed` tombstone은 capture의 Source·Invariant, tombstone 아래 Missing U는 `tombstone_before`의 Census·Invariant다. tombstone을 같은 version의 Inline 행으로 바꾸면(Inline `Transaction(T)` 행 아래의 delete RowValue U) B는 Missing before 경로로 가고, `read_missing`이 스키마 없이 U를 decode해 inline RowValue를 거절한다(Census·Undo). `read_missing`의 typed Unsupported(MCB04-V14)는 유효한 RowDeleted·chunk RowValue before에만 있고, DELETE는 둘 다 쓰지 않는다. 같은 편집 경로로 셀을 그대로 다시 쓴 대조 둘은 옛 값 100을 읽는다.
  - RDL-C(C 쪽): 대상 판정 표, Index가 있는 table의 Unsupported(판정 전), overflow 행(committed·다른 거래)의 Unsupported, U 없는 `Transaction` 행의 손상(`row_delete_tests.rs`), INSERT Capacity가 D만큼 움직이는 경계(`row_write_tests.rs`).
  - RDL-J01..J10, 실제 C 공급 위: 자동커밋 삭제·정리·0행, 명시 거래 rollback 뒤 commit, 정리 전 읽기, Committed·Active·RolledBack·Reserved crash의 재시작, 한 거래의 삭제 뒤 INSERT, own INSERT 행 Unsupported, 두 건 삭제의 rollback·commit. jobs.rs 효과 묶음, Row arm의 이력 D 전체 replay(live record와 같음)도 RDL-J다.
  - RDL-R01..R06: 재시작에서 실제로 잡은 base로 본다. R01은 효과1 네 상태의 base4 수용이다. R02..R06은 Active base를 한 곳씩 고친 손상의 거절이다: actor tombstone을 `Committed`로 고침, U 없는 `Committed` tombstone, 짝 없는 tombstone, 비canonical delete U, leaf 예약. leaf 예약은 entries+D가 16188을 넘으면 Corrupt다. 정확히 16188이면 예약은 통과하고, 채운 RowId가 row_high 위라 Order로 거절된다(시험이 Order를 직접 단언한다).
- **계획과 다른 점.**
  - §12 RDL-J의 "정리 전 RowId Busy"는 실제 C 위에서 다른 거래로 만들 수 없다(§19에서 정정: 시험이 Status page를 더 초기화하면 만들 수 있고, RDL-J11이 본다). 첫 Row profile은 Status slot이 하나라서 앞 거래가 TerminalFinish되기 전에는 새 거래가 Begin하지 못한다(WrongState). J09는 own INSERT 행 Unsupported로 바꿨다. Busy·SerializationConflict 분류는 RDL-A03..A05(literal 대역)와 C 대상 판정 표가 본다.
  - 이력 D replay 시험은 closure mode를 끈다. 이 world의 Status 20과 Undo 15·16은 미리 초기화한 page라 INIT provenance가 없다. 효과1 상태의 base4는 RDL-R이 실제로 잡은 base로 본다.
  - leaf 예약 Capacity를 A로 끝까지 밟으려면 leaf를 거의 채워야 한다(80행 안팎). 그래서 경계는 C 단위 시험과 RDL-C transition 시험으로 본다.
  - §5의 오류 순서에서 Reserve admission은 대상 판정 뒤다. 그러나 Free root Job이 없을 때의 거절(Unchanged, `BudgetMismatch`)은 판정 전 `row_shape`에서 난다. `row_shape`가 RowLeaf 위치와 함께 root Job을 고르기 때문이고, INSERT와 같은 순서다. 효과 없는 거절이라 0행이 될 DELETE도 이 경우 그 거절로 끝난다.
  - RDL-B06(읽기 중 DeleteClr의 gate 대기)은 이 PR에 시험이 없다. 배제 자체는 INSERT와 같은 gate(MCB03-V02)이며, DELETE용 시험은 2-b-3에서 더한다.
  - **per-native 관찰자 `semantic_row/{undo,leaf}.rs`는 이 PR에서 켜지 않았다.** 2-b-1 기록은 undo 관찰자를 2-b-2에서 켠다고 했고, 이 PR은 jobs.rs만 켰다. 두 관찰자는 제품 호출자가 없는 시험 전용 oracle이다(`dead_code` 허용, N1 cross-check만 부름). DELETE를 가르치려면 옛 행 before를 빌려 받는 입력과 스키마 진입이 더 필요하다. 제품 경로는 C transition(독립 저자 literal과 바이트 비교)과 실제 C 위 RDL-J가 덮는다. 다음 PR(2-b-3)이 undo.rs·leaf.rs의 DELETE 대조(undo.rs의 H 증명 `proof_page` 포함)와 N1 cross-check의 효과1 skip 제거, status.rs의 DELETE opcode skip 제거를 한다. 2-b-1이 넘긴 RDL-C mutant 가운데 "후보 frame의 다른 RowId"와, A frame으로 본 "INSERT Forward가 정확히 `entries+cell+D = 16188`인 leaf에서 받아들여짐"도 그 PR에서 본다. release cursor 3에서 U의 before stamp가 지워진 ReleasingCompensated 행을 base4가 확인하는 갈래의 시험도 그 PR에 둔다. "후보 frame의 cell 크기"는 RDL-A의 "another sealed cell"이 본다.

<a id="implementation-2b3"></a>
## 18. 2-b-3 구현 기록

2-b-3은 시험 전용 per-native 관찰자 `semantic_row/{status,undo,leaf}.rs`가 DELETE를 보게 하고, 2-b-1·2-b-2가 넘긴 시험을 채운다. 제품 경로는 바뀌지 않는다. 세 관찰자는 제품 호출자가 없고 N1 cross-check와 시험만 부른다. native fault cut은 3단계다.

- **status.rs.** DeleteForward·DeleteClr·DeletePrune은 INSERT 역할(Forward·Clr·Prune)과 같은 Status 전이를 밟는다. commit Q를 받는 것은 Commit과 Prune 역할(DeletePrune 포함)이다.
- **undo.rs.** 공유 Row 연산은 선택한 Job의 효과와 opcode가 맞아야 한다. 어긋나면 IdentityMismatch이며, 2-b-1 C 전이와 같은 규칙이다. 다만 opcode별 native 크기 규칙(U 432 B와 `432+L` B)이 어긋난 frame을 먼저 거절하므로 이 검사는 이중 방어이고, 이 검사만 거절하는 시험은 없다. 효과1 root의 U는 진입 (b)로 본다(`validate_outline`).
  - Live는 `432+L` B이고(L=`cell_bytes`−180, 5..=2048), before가 RowValue·`Committed` stamp·logical 있음·Inline이다.
  - Releasing은 432 B·payload None이다. before stamp는 cursor 3부터 없다.
  - metadata·edge·release는 INSERT와 같은 `material`에서 온다.
  - U page의 한 칸 검사는 "cell 끝이 `INITIAL_UNDO_CELL`의 끝과 같고 길이가 432 B 이상"으로 넓혔다. 정확한 길이는 record 검사가 본다.
  - H 증명은 DELETE 선행 root의 Live U를 같은 진입 (b)로 그 Job과 대조한다(§4.4).
- **leaf.rs.**
  - **예약(§4.3).** 원래 bank에서 table의 Retained DELETE root가 쥔 D를 센다. before leaf는 entries+D ≤ 16188이어야 하고, 어기면 Corrupt다. after leaf는 그 group이 바꾼 D(DeleteForward +(C−180), DeleteClr·DeletePrune −(C−180))로 같은 식을 만족해야 하고, 어기면 OrderMismatch다. C `typed_leaf`와 같은 식이다.
  - **행.** 모든 행을 table 스키마로 읽는다. tombstone은 `Transaction` stamp와 U가 있을 때만 받고, 그 밖은 Corrupt다.
  - **actor 행.** actor의 record는 `{row, Transaction(actor), logical(actor, mutation), U}`여야 한다. 길이는 INSERT면 `cell_bytes`, DELETE면 tombstone 180 B다. projection은 Job의 것과 같아야 한다.
  - **DeleteForward.** committed 행(`Committed(q_old)`, 옛 logical, undo None, 같은 값, cell C)이 actor의 tombstone이 된다. U after의 Live record를 진입 (a)로 읽는다. 그 record가 Job에서 계산한 delete record와 같아야 하고, 값은 옛 행 값과 같아야 한다.
  - **DeleteClr.** tombstone이 같은 committed 행으로 돌아간다. U before의 Live를 (a)로 읽어 복원 행과 대조한다. U after는 같은 stamp·logical을 가진 Releasing record와 바이트가 같아야 한다.
  - **DeletePrune.** tombstone이 사라진다. U before의 Live와 U after의 Releasing을 DeleteClr과 같은 식으로 본다.
  - U before의 logical을 옛 행이나 복원 행과 대조하는 곳은 이 관찰자뿐이다. undo.rs의 진입 (b)는 logical이 있는지만 본다.
- **N1 cross-check.** 2-b-1·2-b-2의 효과1 skip과 status.rs의 DELETE opcode skip을 걷었다. status·undo·jobs 관찰자는 이력 D의 모든 group을 본다.
  - leaf.rs는 이번에 처음 cross-check에 들어왔다. World가 table의 Catalog record를 만들어 넘기고, INSERT Forward의 봉인 값은 World가 만든 RowReserve에서 가져온다.
  - leaf를 바꾸는 group 15개를 본다: INSERT Forward 2, Clr 4, Prune 3, DeleteForward 3, DeleteClr 1, DeletePrune 2.
  - A literal의 INSERT Forward 셋(A-h-501, A-noh-301, A-ord-301)은 이 World가 RowReserve를 만들지 않아 봉인 값이 없다. 그래서 leaf.rs를 건너뛰고, A literal 대조가 덮는다.
  - DELETE group 여섯(D-524·526·528·533·535·537)마다 후보를 하나 더 만든다. U after record의 before logical mutation을 +1하고 frame째 다시 묶는다(native after body·digest, redo page image, page·frame digest). undo.rs는 이 후보를 받고(진입 b), leaf.rs는 IdentityMismatch로 거절한다. DeleteForward에서는 옛 행과의 대조가, DeleteClr·DeletePrune에서는 Releasing after와의 바이트 대조가 거절한다.
  - DeleteClr(D-528)에서는 복원 행의 logical mutation을 +1한 후보도 만든다. core의 after projection도 그 행에 맞춰 다시 쓰므로, 거절하는 것은 U Live before와의 logical 대조뿐이다(leaf.rs, IdentityMismatch).
  - DELETE 선행 root의 U를 H로 읽는 group 넷(D-526·528·531·536)에서는 H page의 U record incoming을 +1하고 page digest를 다시 봉인한다. undo.rs의 H 증명(진입 b)이 IdentityMismatch로 거절한다. H root의 Job(cell 길이·효과 byte)을 고치면 budget이 깨져 Job codec이 H 증명보다 먼저 거절하므로, H mutant는 U page를 고친다.
  - 선택한 Job을 바꾼 mutant는 leaf.rs까지 가지 않는다. undo.rs가 선택한 Job을 frame에 먼저 묶기 때문이다. 그래서 leaf.rs 쪽 mutant는 U page와 leaf page를 고친다.
  - 각 mutant가 겨누는 검사를 끈 build에서는 해당 시험이 실패한다(H 증명, 복원 행 logical, 옛 행 logical, Releasing 대조, before 예약 거절, D 조정). 작성자와 독립 검토자가 검사를 하나씩 끈 build로 확인했다.
- **2-b-1·2-b-2가 넘긴 시험.**
  - **다른 RowId(RDL-C).** D-523 상태에서 D-524 DeleteForward의 tombstone key와 record row를 2에서 3으로 바꾼 frame을 만든다(`transition_delete_tests.rs`). C 관찰자는 Corrupt, leaf.rs는 IdentityMismatch로 거절한다. A producer는 같은 leaf의 다른 committed 행(9)의 봉인 cell·projection을 받으면 출력 전에 IdentityMismatch로 거절한다(RDL-A, `group_delete_tests.rs`).
  - **A frame의 leaf 예약 경계(RDL-A).** A의 이력 D group 셋의 before RowLeaf를 committed 행으로 채워 예약 경계에 둔다. A는 모든 경우에 frame을 만들고, C 관찰자와 leaf.rs는 같은 답을 낸다.
    - D-531(효과1 H 위 INSERT Forward, 행 2의 Retained delete root가 D=19를 쥠): entries+cell+D가 정확히 16188이면 둘 다 받고, 16189이면 둘 다 OrderMismatch다. 맞음은 같은 gate 아래 Reserve가 이미 증명했다.
    - D-526(행 9의 DeleteForward, D=19)과 D-528(그 DeleteClr, 행 9 root를 더해 D=33): before의 entries+D가 정확히 16188이면 둘 다 받는다. group이 entries와 D를 같은 C−180만큼 옮기므로 after도 16188이다. 16189이면 둘 다 before를 Corrupt로 거절한다. A는 자기 cell이 들어가는지만 본다.
  - **RDL-B06**(`row_join/retire/delete_read.rs`). MCI03-V01과 같은 thread 시험이다. committed·Prune된 행 100·101 가운데 100을 명시 거래가 지운다. 다른 거래의 읽기 B1을 capture 뒤(`RowReadCaptured`)에 멈춰 tombstone의 Txn·U pin을 쥐게 한다. 그동안 rollback이 다음 순서로 진행된다.
    1. rollback은 시작에서 terminal read gate를 닫고 B1을 기다린다. RollbackStart·DeleteClr보다 앞이다.
    2. 기다림 지점과 B1이 pin을 쥐는 동안 WAL은 rollback을 부르기 전과 같다. 늦은 읽기 B2는 Unchanged `InstallationPending`으로 거절된다.
    3. B1이 이어 가면 U의 before로 옛 값 100을 읽는다.
    4. rollback은 RolledBack으로 끝나고, B2의 재시도는 복원된 100을 읽는다.
    5. 정리는 5 group이고, 새 statement는 두 행을 읽는다.
  - **RDL-R07**(`row_join/retire/delete_base.rs`). release cursor 3에서 U의 before stamp가 없어진 ReleasingCompensated 행을 base4가 확인하는 갈래다. RolledBack 파일의 재시작을 정리가 root를 cursor 3으로 옮긴 곳에서 멈추고, 다음 재시작이 잡은 base를 쓴다.
    - cursor 0과 3의 base는 둘 다 그대로 받는다.
    - 복원 행의 logical을 바꾸면 두 cursor 모두 actor 대조가 거절한다(Identity).
    - 복원 행을 `Committed(1)`로 재stamp하면, cursor 0은 U stamp 대조가 거절한다(Identity). cursor 3에는 대조할 stamp가 없고, graph의 committed 행 순서(creator q < q)가 거절한다(Order). 이 world에서 행이 가질 수 있는 committed q는 2뿐이라, 다른 stamp도 두 갈래 어느 쪽에서도 통과하지 못한다.
- **한계.**
  - leaf.rs의 tombstone 판정(`Committed`이거나 U 없는 tombstone은 Corrupt)에는 이 관찰자의 시험이 없다. A는 그런 leaf에서 frame을 만들지 않고(`leaf::inspect`가 Unsupported), frame 밖 이웃 행을 before·after 두 page에서 같이 고쳐야 한다. 같은 규칙은 base4의 RDL-R02·R03과 B capture의 RDL-B08이 본다.
  - undo.rs의 효과·opcode 묶음은 이중 방어다(위).
  - A literal의 INSERT Forward 셋은 leaf.rs를 거치지 않는다(위).
  - 세 관찰자는 여전히 제품 호출자가 없는 시험 전용 oracle이다.

<a id="implementation-3"></a>
## 19. 3단계 구현 기록

3단계는 native fault cut RDL-N01..N06과 남은 시험 RDL-J11을 더한다. RDL-J11은 2-b-2가 실제 C 위에서 만들 수 없다고 적은 Busy 시험이다. 제품 경로는 바뀌지 않는다. 시험, controller, 동결 inventory만 더했다.

- **RDL-N 이력.** 사례마다 이력 하나를 실행 전에 정했다(`tests/fixtures/module-row-delete/native-inventory.json`). CREATE가 q 1을 쓰고, A의 자동커밋 INSERT가 행 100·101을 q 2에 commit한 뒤 정리까지 마친다. 그 뒤 행 100에 사례의 단계를 밟고 cut에서 멈춘다.

  | 사례 | cut | 마지막 단계 | READY q | 복구 CLR | READY 행 |
  | --- | --- | --- | --- | --- | --- |
  | N01 | DeletePrune 앞 | 자동커밋 DELETE와 그 ACK(q 3) | 3 | 0 | 101 |
  | N02 | DeletePrune 뒤 | N01과 live 정리의 첫 단계(DeletePrune) | 3 | 0 | 101 |
  | N03 | DeleteForward 앞 | 명시 거래(Serializable)의 preflight와 RowReserve | 2 | 0 | 100, 101 |
  | N04 | DeleteForward 뒤 | 명시 거래의 DELETE statement, 거래는 Active | 2 | 1 | 100, 101 |
  | N05 | DeleteClr 앞 | N04 뒤 Y(다른 Status)가 102를 INSERT해 q 3에 commit, A의 Commit은 SerializationConflict로 거절, A의 full RollbackStart | 3 | 1 | 100, 101, 102 |
  | N06 | DeleteClr 뒤 | N05와 rollback의 첫 단계(DeleteClr) | 3 | 0 | 100, 101, 102 |

- **세 process.** S7 native(MCI02-N)와 같은 모양이다. 같은 ELF가 phase마다 process 하나로 돈다.
  - `produce`는 cut에서 flush나 shutdown 쓰기 없이 `exit(0)`한다.
  - `recover`는 C 복구와 A의 복구 CLR handler로 READY에 간다. B cursor로 읽은 뒤 A의 새 행(17)과 그 Commit ACK를 낸다.
  - `reenter`는 C+B 복구(RejectInitialLosers)와 두 ACK의 B 읽기다.
  - worker는 `row_join::retire::autocommit::delete::native::rdl_native_worker`(ignored)이고, controller는 `scripts/check_dev3_row_delete_native.py`다.
- **controller 검사.** 기대값은 모두 동결 inventory에서 온다.
  - produce: 결과, q, RowId가 inventory와 같다. in-process WAL witness는 controller가 직접 계산한 WAL file의 SHA-256·길이와 같다. 마지막 단계 앞뒤의 witness가 다르다(마지막 단계는 N01이 RowReserve·DeleteForward·Commit 세 group, N04가 RowReserve·DeleteForward 두 group, 나머지는 group 하나다). effect trace에서 파일마다 마지막 write·resize 뒤에 그 파일의 성공 sync가 있고, 마지막 성공 sync 뒤에는 lease 해제와 close뿐이다.
  - "뒤" 사례의 cut WAL은 짝 "앞" 사례의 cut WAL을 바이트 그대로 앞에 두고, 그 뒤의 WAL write는 하나(그 group)뿐이다(N01→N02 DeletePrune, N03→N04 DeleteForward, N05→N06 DeleteClr). 같은 이력이 그 group만큼 더 간 것이다. "뒤" 사례만 골라 실행해도 controller가 짝 사례를 먼저 돌린다.
  - recover·reenter: READY q는 앞 process의 마지막 ACK q이고 inventory와 같다. 복구 CLR 수와 B가 읽은 값도 inventory와 같다. 각 process는 파일을 바꿨다. 세 process의 pid는 모두 다르다.
  - 자원: 복구 process마다 handle 0, peak 3 이하, fd peak 6 이하, log 4 MiB 이하다(worker 단언).
- **결과.** debug test ELF에서 18 process가 모두 통과했다. 복구 step은 N01이 20, N02가 18이다. N01에서는 복구가 DeletePrune을 했고, N02에서는 live 정리가 이미 했다. 복구 CLR은 N03 0(Reserved root는 Cancel), N04 1, N05 1, N06 0이다. 실행 기록은 [`module-gate/evidence/row-delete-3/`](../../module-gate/evidence/row-delete-3/README.md)에 있다.
- **메모리 쌍둥이.** 같은 이력을 memory directory 위에서 도는 시험 셋이다(`rdl_n01_n02_memory_delete_prune_cuts`, `rdl_n03_n04_memory_delete_forward_cuts`, `rdl_n05_n06_memory_delete_clr_cuts`). cut의 파일로 recover를, recover가 남긴 파일로 reenter를 돌리고, native worker와 같은 기대값을 단언한다. cut의 파일에서 C의 재시작이 base4로 잡는 Row Job 상태도 §9의 기대와 대조한다. 예를 들어 N01은 Retained delete root와 TerminalReady이고, N02는 ReleasingCommitted root와 TerminalReady다. 그래서 "앞"과 "뒤" 사례가 CI에서도 갈린다(N01이 DeletePrune까지 가는 probe는 여기서 실패한다). 각 사례의 기대값이 동결 inventory에 그대로 적혀 있는지도 본다. CI가 도는 것은 이 셋이고 native worker는 ignored다.
- **RDL-J11**(`row_join/retire/delete_busy.rs`). §17은 "정리 전 RowId Busy"를 실제 C 위에서 만들 수 없다고 적었다(Status slot 하나). 실제로는 시험이 Row Status page를 더 초기화하면(`system_init`, RDL-N05와 같다) 다른 거래가 Active인 동안에도 새 거래가 Begin한다. 그래서 §5 표에서 다른 거래가 쥔 행의 판정을 실제 C 위에서 본다. 행 100·101이 q 2에 commit되고 정리된 뒤 A(ReadCommitted)가 100을 지운다.
  1. Y(ReadCommitted, 둘째 Status)의 100 DELETE는 Busy다(A가 Leased인 tombstone). Y는 이어서 102를 INSERT한다.
  2. A의 102 DELETE는 0행이다(Y가 Leased인 INSERT 행).
  3. Y가 q 3에 commit하고 정리는 하지 않는다. A의 102 DELETE는 Busy다(q 3 ≤ S에 commit됐고 Prune 전).
  4. A가 Active이므로 live 정리는 Y의 단계만 고른다: Prune, ReleaseRole 셋, RootFree, TerminalFinish. Prune 뒤와 셋째 ReleaseRole 뒤에는 102가 `Committed(3)`이고 Y의 root가 RowId를 쥐어 Busy다. RootFree 뒤에는 A의 DELETE가 102를 지운다.
  5. Z(Serializable, 셋째 Status)가 q 3에서 Begin하고, A가 q 4에 2행으로 commit한다. Z의 100·102 DELETE는 SerializationConflict다(q 4 > Z의 snapshot 3). Z는 rollback한다.
  6. drain은 12 group이다. A의 delete root 둘(5 + 5), A의 Terminal, Z의 Terminal이다. 새 statement는 101만 읽고, 재시작은 q 4 READY, CLR 0으로 101만 읽는다.

  판정 규칙을 하나씩 끈 build에서 J11은 실패한다. Leased tombstone의 Busy나 Prune 전 행의 Busy를 0행으로 바꾸면, 그 statement가 거절 대신 0행으로 끝난다. RowId를 쥔 root 검사를 끄면 preflight는 통과하고 Reserve가 거절한다(census의 RowId 유일성, 이중 방어). RDL-J09의 주석도 J11을 가리키게 고쳤다.
- **한계.**
  - cut은 process 종료다. 전원 단절처럼 page cache의 쓰기를 잃는 경우는 아니다. 대신 controller가 cut에 sync되지 않은 쓰기가 없음을 보인다(파일마다 마지막 write·resize 뒤의 성공 sync, 마지막 sync 뒤 쓰기 없음). 그래서 파일 내용이 곧 durable prefix다. S7의 같은 검사를 파일 단위로 강화했다.
  - cut은 group 경계에만 둔다. group 안의 torn write, 복구 도중의 crash(복구가 쓴 DeleteClr·DeletePrune과 READY 사이), I/O fault(짧은 write, write·sync 오류)는 이 단계에서 보지 않는다. 일반 재장애는 [일정 12](../plans/dev3-schedule-baseline.md#items), fault 전수 실행은 일정 17의 몫이다.
  - ReadCommitted의 "T' committed q'>S → 0행"은 A 실행기로 만들 수 없다. statement snapshot을 preflight 바로 앞에서 잡기 때문이다. C 판정 표 시험(`row_delete_tests.rs`)이 본다.
  - native 결과는 이 cloud 환경의 실행 한 번이다. CI는 native worker를 돌리지 않는다(ignored).
  - RDL-J11과 RDL-N05의 둘째·셋째 Status page는 시험이 직접 초기화한다(S7 MCI02-N03과 같다). 여러 Status를 쓰는 동시 거래의 일반 공급은 [일정 11](../plans/dev3-schedule-baseline.md#items)이다.

## 작성 기록

- 2026-11-03 r1 초안. main `c7a6425`의 source를 읽고 썼다(operation.rs, job.rs, undo_record.rs, undo.rs, transition/prepare.rs, ledger.rs, read.rs, retirement.rs, tree_page/edit.rs, row_record.rs).
- 2026-11-03 D2 1회차 **REJECT**(독립 검토). 산술(shape·frame·ledger·cleanup·scratch)은 모두 맞다고 확인됐다. 지적과 r2 반영: (1) blocker — RowId는 모든 root Job 사이에서 유일해야 하는데 정리 전 INSERT root·own compensated root가 쥔 행의 삭제가 census를 깬다 → §5 Busy 행, §4.1. (2) 효과1 H의 증명이 Missing·`INITIAL_UNDO_CELL`만 알고 codec이 스키마 없이 RowValue를 못 본다 → §4.4, §3.2 진입 둘. (3) Releasing RowValue Inline은 지금 허용되고 U09–U12가 그 예다 → codec 변경을 추가형으로, Undo 형식 개정 표시. (4) Row 쓰기가 read gate로 reader를 배제하므로 stale capture는 일어나지 않는다 → §7·§8에서 제거, Releasing U는 Fatal. (5) 2단계가 자기 읽기·복구가 거절하는 상태를 쓴다 → B·loser·base4를 2단계로(§1, §13). 그 밖에 native 크기 규칙(§3.4), F4 목록(§10), 식 자리(§3.5), scratch 87 전제(§4.3), §4.3 문구, §7 불가능 상태, 작은 정정들을 반영했다.
- 2026-11-03 D2 2회차 **CONDITIONAL ACCEPT**(같은 검토자). r2가 1회차 지적을 바르게 고쳤음을 확인했다. 남은 것은 문서 보강이며 r3에 반영했다: (1) Undo 형식이 스키마 검증을 요구하므로 두 진입을 형식 개정에 적고, metadata 진입은 before 값을 내주지 않는 타입으로(§3.2). (2) base4는 스키마를 가지므로 (a), DeletePrune의 before U도 (a)(§3.2, §9). (3) base4의 효과1 상태별 규칙과 역방향 scan(§9). (4) `value_before` 읽기 census(§10). (5) 최대 frame에서 유도한 자리 추가(§3.5, §11). (6) Aborted 판정의 C·B 차이 설명(§5). (7) leaf 예약 Capacity는 INSERT 전용(§5). (8) RDL-B 보강(§12).
- 2026-11-03 D2 3회차 **ACCEPT**(같은 검토자). 문구 nit 넷(metadata 진입의 길이 표현, 옛 이름 "header 진입", ReleasingCompensated의 logical은 모든 cursor, RDL-R 번호 범위)을 반영했다.
- 2026-11-03 §18(2-b-3 구현 기록) 추가. 관찰자 status.rs·undo.rs·leaf.rs의 DELETE, N1 cross-check의 효과1 skip 제거, 넘겨받은 시험(다른 RowId, A frame 예약 경계, RDL-B06, RDL-R07)을 적었다.
- 2026-11-03 2-b-3 D2 1회차 **ACCEPT WITH REQUIRED CHANGES**(독립 검토). (1) gate record의 `paths`에 바뀌지 않은 파일 → 제거. (2) H mutant(Job 편집)가 Job codec의 budget 검사에 걸려 H 증명에 닿지 않음 → H page U의 incoming mutant로 바꾸고 오류를 단언. 권고: DeleteClr 복원 행 logical mutant 추가, DELETE group의 예약 경계(D-526·D-528) 추가로 before 거절 시험, tombstone 판정 한계 기록, RDL-B06의 WAL 기준을 rollback 호출 전으로. NIT: undo.rs 효과·opcode 묶음은 이중 방어, 행 번호, 상태 줄.
- 2026-11-03 §19(3단계 구현 기록) 추가. native fault cut RDL-N01..N06(메모리 쌍둥이 셋, native 18 process)과 RDL-J11(실제 C 위 다른 거래가 쥔 행의 Busy·SerializationConflict)을 적었다.
- 2026-11-04 3단계 D2 1회차 **ACCEPT**(독립 검토, 필수 지적 0). 권고 반영: 메모리 쌍둥이가 cut의 base4 Row Job 상태를 대조(N01·N02가 CI에서도 갈림), controller가 파일마다 sync를 보고 짝 사례 뒤의 WAL write가 하나인지 보며 "뒤" 사례만 고른 실행에서도 짝을 먼저 돌림, 한계에 복구 도중 crash와 I/O fault 추가, 일정 문구를 PR #47 병합 시점 기준으로. NIT: J11의 drain 설명, N04 inventory 문구, §12·§17 정정 표시, README·일정 문구.
- 2026-11-04 3단계 D2 2회차 **ACCEPT**(같은 검토자, 필수·권고 0). 1회차 권고가 모두 맞게 반영됐음을 probe로 확인했다. NIT 둘(§19의 마지막 단계 group 수, 한계의 sync 검사 설명)을 반영했다.

# DEV3 Row UPDATE 첫 경로 — 형식·전이 설계

2026-11-04 · `D3-ROW-UPDATE-001` · **설계 초안 / D2 검토 대기 / 구현·시험 NOT_RUN**

[DELETE 첫 경로](dev3-row-delete.md) · [첫 Row profile](dev3-module-consumers.md#profile) · [WAL shape](dev3-module-consumers.md#row-shape) · [scope3 wire 기록](dev3-module-consumers.md#row-scope3-wire) · [API 흐름](dev3-module-consumers.md#api-flow) · [Row 격리](dev3-task-10-concurrency.md#module-row-isolation) · [자원 정본](dev3-resource-bounds.md#row-missing-module-budget) · [Undo 형식](dev3-undo-format.md) · [live Row 정리](dev3-task-14-reclamation.md#live-row-retirement) · [명시 거래 실행기](dev3-module-boundaries.md#explicit-transaction) · [행 owner](dev3-task-06-tree-rows.md#algorithms) · [일정 07](../plans/dev3-schedule-baseline.md#items)

Row DELETE 첫 경로(PR #41–#47, main `5760f30`) 다음 [일정 07](../plans/dev3-schedule-baseline.md#items)이다. 일정 항목 07-1이 이 문서와 독립 검토, 07-2가 C 형식·전이, 07-3이 A/B 제품 경로·Undo/CLR·읽기·reopen·복구(메모리 시험), 07-4가 native fault cut이다. 사용자는 2026-11-03 "다음 작업을 범위를 넓게 잡고 진행"하라고 했고 범위 안내 없이 PR로 판단하겠다고 했다. 그래서 07의 네 항목을 차례로 진행하며 이 문서는 코드보다 먼저 그 형식과 전이를 정한다.

UPDATE의 Undo는 DELETE가 만든 delete U와 **같은 형식**이다(옛 행의 `Committed(q_old)` stamp·logical·inline 값을 가진 Live RowValue). 새로 정하는 것은 leaf의 새 행(옛 행 대신 같은 RowId의 새 inline 행), 크기가 바뀌는 행의 leaf 공간, 두 cell을 갖는 shape·Job, 그리고 지금 "Inline `Transaction(T)` 행이면 INSERT"로 가정하는 곳들이다. 이 문서는 제품 PASS가 아니다.

<a id="scope"></a>
## 1. 범위와 제한

첫 경로는 **RowId 하나로 고른 행 하나의 값 전체를 새 canonical 값으로 바꾼다.** 결과는 바꾼 행 수 0 또는 1이다. 새 값의 크기는 옛 값과 달라도 된다(같은 leaf 안).

| 포함 | 제외(첫 효과 전 거절하거나 다루지 않음) |
| --- | --- |
| committed이고 이미 Prune된 행(`Committed(q)`, undo None, inline)의 값 교체 | 같은 거래가 넣거나 바꾼 행(Unsupported), 같은 거래가 지운 행(0행) |
| 새 값 inline(5..2048 B), 옛 행보다 크거나 작거나 같은 크기 | 새 값이 2048 B를 넘는 행(Unsupported), overflow 옛 행(Unsupported) |
| 자동커밋·명시 거래의 UPDATE 문장, statement rollback·full rollback의 복원 | Prune 전 행, RowId를 root Job이 쥔 행, 다른 거래가 쓰는 중인 행(Busy, §5) |
| commit 뒤 live 정리의 UpdatePrune·ReleaseRole·RootFree | 술어·범위 UPDATE, 여러 행 UPDATE 문장, column 일부 갱신 |
| 재시작 replay, 복구 loser의 UpdateClr, 복구 cleanup의 UpdatePrune, base4 census | index가 있는 table, level≠0 leaf, split·merge(새 값이 leaf에 안 들어가면 Capacity) |
| B 읽기의 바뀐 행 판정(own·committed·미정 거래) | 일반 wait-for·row lock map, SQL/DEV4 연결 |

제한의 이유는 DELETE와 같다([row-delete §1](dev3-row-delete.md#scope)). 대상이 `Committed(q)`·undo None이면 새 U의 before는 TxnRef 없는 stamp와 previous-version Never만 가지므로 읽기가 지금의 pin 둘(TxnPin+UndoPin)로 끝난다. own 행의 재갱신은 U의 before가 `Transaction` stamp와 previous-version을 가져 version chain과 3pin이 필요하므로 일정 08-4(일반 행 변경)로 남긴다. 같은 bytes로 바꾸는 UPDATE도 새 mutation identity를 갖는다([Undo 형식](dev3-undo-format.md)).

<a id="assumptions"></a>
## 2. 지금의 INSERT·DELETE 전용 가정

| 영역 | 지금 | UPDATE가 바꾸는 것 |
| --- | --- | --- |
| opcode | INSERT 2·4·20, DELETE 8·9·23. shape·`frame_bytes`·wire decode는 cell 하나(C) | 새 opcode 3개와 두 cell(C·C')의 shape(§3.4) |
| Job3 | `@284` 효과 0·1, `[285,288)` zero. `@280`은 INSERT의 새 cell 또는 DELETE의 옛 cell | 효과 2, `[286,288)`의 새 cell C'(§3.3) |
| U | 효과1 U는 Live RowValue `432+L`(옛 값), Releasing 432 | 그대로 쓴다. 효과2 U는 효과1 U와 같은 형식·셀 범위(§3.2) |
| RowLeaf | `Transaction(T)` Inline 행은 INSERT 행(Missing U), tombstone은 DELETE(RowValue U) | `Transaction(T)` Inline 행이 RowValue U를 가지면 UPDATE 행(§3.1). Forward·CLR·Prune 모두 Replace |
| leaf 공간 | Reserve·Forward가 INSERT 새 cell만, 예약 D는 Retained DELETE root의 `C−180` | 커지는 UPDATE의 Reserve·Forward 검산, 줄어드는 UPDATE의 복원 예약(§4.3) |
| 대상 판정 | DELETE `delete_class`는 다른 거래의 Inline `Transaction(T')` 행을 INSERT로 본다 | 그 행의 root 효과로 INSERT·UPDATE를 가른다(§5). DELETE 판정도 같이 바뀐다 |
| B 읽기 | 안 보이는 Inline `Transaction(T)` 행은 `missing_before`(스키마 없는 decode, RowValue면 Fatal) | `missing_before`가 RowValue U를 보고하고 B가 `value_before`로 옛 값을 읽는다(§7) |
| 회수 | `row_cleanup_next`·복구 cleanup이 효과로 Prune·DeletePrune을 고른다 | 효과2는 UpdatePrune(`ValuePrune`, §8) |
| 복구 | loser CLR=Clr·DeleteClr, cleanup=Prune·DeletePrune, base4는 효과 0·1 | UpdateClr·UpdatePrune, base4 효과2 규칙과 역방향 scan(§9) |
| 비용 | INSERT·DELETE 식, root당 cleanup 최대 316192 | UPDATE 식, 최대 318240(§3.5) |

<a id="format"></a>
## 3. 형식

L은 **옛 행**의 canonical inline 길이(5..2048), C는 옛 행의 tree cell `180+L`(185..2228)이다. L'은 **새 값**의 길이(5..2048), C'은 새 행의 tree cell `180+L'`(185..2228)이다. h는 previous transaction H 존재 여부다.

<a id="updated-row"></a>
### 3.1 바뀐 행

UpdateForward 뒤 leaf의 행은 새 형식이 아니다. INSERT Forward가 쓰는 것과 같은 inline 행이다: `stamp=Transaction(T)`, `logical=(T.id, planned_mutation)`, `undo=U`, Inline 새 canonical, cell C'. INSERT 행과는 U의 before kind로만 구별된다(INSERT는 Missing, UPDATE는 RowValue). Job bank에서는 그 RowId를 쥔 Retained root의 효과 byte(0 또는 2)로 구별된다(§3.3, §5).

UpdatePrune은 INSERT Prune처럼 이 행을 `Committed(q)`·undo None으로 재stamp한다. logical은 `(T.id, mutation)` 그대로다. UpdateClr은 행을 U의 before(`Committed(q_old)`, 옛 logical, 옛 값, cell C)로 되돌린다.

<a id="update-undo"></a>
### 3.2 update U

효과2 U는 [delete U](dev3-row-delete.md#delete-undo)와 바이트 형식이 같다.

| 상태 | 필드 | 크기 |
| --- | --- | --- |
| Live | target Row(table/gen/RowId), before `RowValue`, before_stamp `Committed(q_old)`, before_logical 옛 logical, payload Inline 옛 canonical(L), edges `[txn Linked(H) 또는 Never, version Never]`, incoming 2, cursor 0 | `432+L` |
| Releasing cursor0..3 | delete U와 같음(payload None, cursor3에서 before_stamp None) | 432 |

Undo codec은 바뀌지 않는다. DELETE 첫 경로가 넣은 Releasing RowValue payload None과 검증 두 진입(스키마 진입 (a), metadata 진입 (b))을 그대로 쓴다. [Undo 형식](dev3-undo-format.md)의 2026-11-03 개정 문단은 "delete U"를 "delete·update U"로 넓힌다(Releasing에서 body를 버리는 scope3 Row U가 둘이 된다). U의 셀은 slot0의 `[16352−(432+L), 16352)`, Releasing은 `INITIAL_UNDO_CELL`이다. U의 before_stamp가 `Committed`이고 대상 행의 undo가 None이라 edge·count multiset은 INSERT·DELETE와 같다(§4.2).

U는 옛 값을 담고 새 값은 담지 않는다. 새 값은 leaf의 새 행에만 있고, Job의 C'과 projection이 Reserve부터 Forward까지 그 값을 봉인한다(INSERT의 [값 commitment](dev3-module-consumers.md#row-scope3-wire)와 같다).

<a id="job-effect"></a>
### 3.3 Job3 효과 2와 새 cell

kind3(Row) root Job의 `@284` 효과 byte에 2(UPDATE)를 더한다. 효과2 root는 새 cell C'을 `[286,288)`(u16 LE)에 둔다. `@285`는 언제나 zero다.

| 필드 | 효과 0(INSERT) | 효과 1(DELETE) | 효과 2(UPDATE) |
| --- | --- | --- | --- |
| `@280` cell_bytes | 새 행 C | 옛 행 C | **옛 행 C** |
| `[286,288)` | zero | zero | **새 행 C'**(185..2228) |
| `@288` projection | 계획한 새 행 | 계획한 tombstone | **계획한 새 행**(`Transaction(T)`, `(T.id, mutation)`, undo U, Inline 새 값) |
| `@320` RowId | 발급한 `row_high+1` | 대상 | **대상**(발급하지 않음) |
| budget row3 | `311608+2C+128h` | `311608+C+128h` | **`311428+C+2C'+128h`**(§3.5) |

Reserved·Retained는 위를 모두 가진다. Releasing은 지금처럼 `[280,284)`와 `[285,320)`를 zero로 두므로 C'도 지워지고 효과 byte와 RowId만 남는다. Free·Terminal·Catalog Job의 `[284,288)`은 zero다. Job codec은 Row root에서 효과 ≤2, 효과2 Reserved·Retained의 C'이 185..2228, 그 밖에서 `[286,288)` zero를 요구한다. `JobView`에 `new_cell()`을 더하고 `forward_wal()`·`clr_wal()`·`cleanup()`·`has_previous_transaction()`이 효과로 식을 고른다. `has_previous_transaction`은 효과2에서 `budget@400 == 311556+C+2C'`이다.

`@280`을 옛 C로 두는 이유: U의 크기(`432+C−180`), H 증명(§4.4), `live_undo_bytes`, base4의 U 셀 범위가 효과 1·2에서 같은 식이 된다. C'은 Forward의 새 행 cell과 UpdatePrune의 재stamp 행 cell에만 쓰인다.

<a id="update-shape"></a>
### 3.4 opcode와 shape

| opcode | outer | 변경 native(합) | E/K/I, redo | frame bytes | 최대(C=C'=2228, h1) |
| --- | --- | --- | --- | --- | ---: |
| `UpdateForward = 10` | XHG Update, handoff | Root1152·Job2176·Status576·U `128+432+(432+L)`·Leaf `128+C+C'`, 합 `4844+2C+C'` | `(3+h)/2/1`, 5P | `91124+2C+C'+128h` | 97936 |
| `UpdateClr = 13` | XHG Clr, handoff | U `128+(432+L)+432`·Leaf `128+C'+C`, 합 `4844+2C+C'` | `(3+h)/2/0`, 5P | `90916+2C+C'+128h` | 97728 |
| `UpdatePrune = 24` | XSG System | U `128+(432+L)+432`·Leaf `128+C'+C'`, 합 `4844+C+2C'` | `(3+h)/2/0`, 5P | `89316+C+2C'+128h` | 96128 |

세 opcode는 core에서 INSERT의 Forward·CLR·Prune과 같은 ref·plan·projection·cursor 규칙을 따르고(`Operation::role`) native mask도 같다. Reserve·Cancel·ReleaseRole·RootFree의 shape는 그대로다. 가장 큰 Row frame은 DeleteForward 95888에서 UpdateForward 97936으로 바뀌며 `FRAME_BYTES` 98304 안이다(여유 368 B). tail `98304−97936 = 368` < 16384라 XOI는 계속 frame tail에 두지 않는다.

**native record 크기.**

| opcode | kind1 leaf (presence, before, after) | kind3 U |
| --- | --- | --- |
| UpdateForward | 3, C, C' | 432 / 432+L |
| UpdateClr | 3, C', C | 432+L / 432 |
| UpdatePrune | 3, C', C' | 432+L / 432 |

**wire decode.** native 합 하나로는 C와 C'을 함께 유도할 수 없다. decode는 native 구간에서 kind1 record 하나의 header를 먼저 읽어 C'을 얻는다(UpdateForward는 after 크기, UpdateClr·UpdatePrune은 before 크기). 그다음 C를 합에서 유도한다: UpdateForward·UpdateClr은 `(native−4844−C')/2`(정수), UpdatePrune은 `native−4844−2C'`. kind1 record가 없거나 둘이거나, header가 native 구간을 넘거나, C·C'이 185..2228 밖이면 Corrupt다. 그 뒤 `validate_native_shape`가 모든 native의 presence·크기를 (C, C')로 다시 검산하므로 먼저 읽은 header도 같은 규칙을 지나간다. `Shape`는 `cell`(C)과 `new_cell`(C', 효과2 opcode만, 나머지 0)을 갖는다. 기존 opcode의 decode는 바뀌지 않는다. 옛 binary는 10·13·24를 Unsupported로, 효과 2인 Job을 Corrupt로 거절한다([버전 정책](../policies/version-compatibility-policy.json)의 개발 형식 규칙). [scope3 wire 기록](dev3-module-consumers.md#row-scope3-wire)의 존재 규칙에 RowLeaf kind1의 present(C)→present(C'), present(C')→present(C), present(C')→present(C')를 더한다(07-2).

<a id="update-costs"></a>
### 3.5 비용식

| 항 | INSERT | DELETE | UPDATE |
| --- | --- | --- | --- |
| ledger Forward(Reserved root) | `91304+C+128h` | `91304+2C+128h` | `91124+2C+C'+128h` |
| ledger CLR(비terminal의 Reserved·Retained root) | `91096+C+128h` | `91096+2C+128h` | `90916+2C+C'+128h` |
| cleanup(Job budget) | `311608+2C+128h` | `311608+C+128h` | `UpdatePrune+4×55528 = 311428+C+2C'+128h` |
| CLR 뒤 남는 cleanup | `222112` | 같음 | 같음 |

세 식은 하나의 규칙으로 읽힌다. Forward·CLR의 native는 `5024 + u + a + b`이고 u는 U가 담는 옛 값 L(INSERT 0), a·b는 leaf의 옛·새 cell(INSERT의 옛 cell 0, DELETE의 새 cell 180)이다. 이 문서는 식을 effect별로 Job 한 곳(`retention_page/row/job.rs`)에 둔다.

**최대가 바뀌는 자리.** ledger Forward·CLR의 최대는 97936·97728, root당 cleanup 최대는 316192(INSERT)에서 **318240**(UPDATE)으로 커진다. 코드에 316192를 상한으로 쓰는 곳은 없고(시험 하나가 INSERT 식의 값을 단언) 합은 모두 검사된 덧셈이다. 07-2가 고칠 설명은 [`RowLoanLayout::FRAME` 주석](../../crates/engine/src/storage/wal_io/plan4_apply.rs)의 최대 frame, [소비자 계약의 cleanup 최대](dev3-module-consumers.md#row-shape)와 shape 표, `candidate_template`의 "group≤95888B" fixture 상계, scope3 wire 기록의 최대 frame, [자원 정본](dev3-resource-bounds.md#row-missing-module-budget)의 frame·tail·cleanup 설명이다. fixture 상계는 97936 UpdateForward fixture가 넘으므로 반드시 고친다.

<a id="scratch"></a>
### 3.6 private loan과 scratch

frame·before9·after5·candidate·schema·XOI·C scratch·A scratch 구간은 그대로다. C scratch(`ROW_REPLAY_SCRATCH_BYTES` 9076 B)도 그대로다.

- UpdateForward(C는 관찰만): U header 432 B는 scratch U 버퍼에, 옛 값 L은 leaf before image에서 빌려 after page에 바로 쓴다(DeleteForward와 같음). 새 행 cell은 후보 frame의 kind1 after에서 채택하고, 그 record의 version·길이 C'·projection을 Job과 대조한 뒤에만 쓴다(INSERT Forward와 같음).
- UpdateClr(C는 관찰만): 복원 cell C를 U before(진입 a)에서 기존 `MAX_CELL` cell 버퍼에 만든다(DeleteClr과 같음).
- UpdatePrune(C가 생산): 재stamp한 cell C'을 record scratch(descriptor 앞부분)와 cell 버퍼에 만든다(INSERT Prune과 같음). U는 DeletePrune처럼 진입 (a)로 읽고 Releasing으로 쓴다.
- A: 새 값은 candidate 구간, 옛 값은 leaf before image다. A scratch 32768 B 안에서 INSERT·DELETE 생산자와 같은 배치를 쓴다.

<a id="transitions"></a>
## 4. 전이

<a id="update-states"></a>
### 4.1 상태 전이

| 전이 | Status·head | Row Job·U·leaf |
| --- | --- | --- |
| Reserve(update) | Active/headH 그대로 | Free→RootReserved 효과2, C=옛 cell, C'=180+L', RowId=대상, projection=계획 새 행. U Free(g_new), inventory1. row_high 불변. 대상은 `Committed`·undo None·Inline·cell C. 대상 RowId를 쥔 다른 root Job이 없어야 한다(census). leaf 공간(§4.3) |
| Cancel | 그대로 | Reserved→Free. 기존 Cancel과 같음 |
| UpdateForward | headH→U, incoming+1 | Reserved→Retained. U Free→Live(§3.2, 옛 값). leaf `Replace(옛 행 C→새 행 C')`. 옛 행은 `Committed(q_old)`·undo None·Inline·cell C, 새 행은 Job의 C'·projection과 같아야 한다 |
| UpdateClr | AbortingStatement/Full의 head=next=U→H | Retained→ReleasingCompensated(c0). U Live→Releasing(payload 제거). leaf `Replace(새 행 C'→복원 행 C)`. 복원 행은 `Committed(q_old)`, 옛 logical, undo None, U의 payload |
| Commit·AbortEnd·StatementFinish·RollbackStart | 기존과 같음 | 효과와 무관 |
| UpdatePrune | Committed(q), q≤Horizon, 대상 U가 현재 head | Retained→ReleasingCommitted(q,c0). U Live→Releasing(payload 제거). leaf `Replace(새 행→재stamp 행)`: `Committed(q)`, logical `(T.id, mutation)`, undo None, 같은 값·cell C' |
| ReleaseRole·RootFree | 기존과 같음 | 효과 byte 유지·교차 검산 외 같음 |

UpdatePrune이 U payload(옛 값)를 버릴 수 있는 이유는 DeletePrune과 같다. `begin_retirement`가 q≤published·q≤horizon을 요구하므로 살아 있는 모든 snapshot과 뒤의 snapshot이 새 값을 본다.

<a id="update-edges"></a>
### 4.2 edge·count

UpdateForward·UpdateClr·UpdatePrune의 edge·count multiset은 INSERT의 Forward·CLR·Prune과 같다. 새 행은 RowUndo(→U)와 RowStamp(→T)를 갖고, 옛 행·복원 행·재stamp 행은 둘 다 없다. U incoming은 0→2→0이다. H의 net0 multiset은 그대로이고, H가 효과2 root일 때의 증명은 §4.4다.

<a id="leaf-reservation"></a>
### 4.3 leaf 공간

[DELETE의 예약 불변식](dev3-row-delete.md#leaf-reservation)을 넓힌다.

> 각 RowLeaf에서 `TREE_ENTRY_CAPACITY − entry_bytes ≥ D`. D는 같은 table의 **Retained** root 전체(모든 actor)에 대해, 효과1이면 `C_j−180`, 효과2면 `max(0, C_j−C'_j)`의 합이다.

| 단계 | entries 변화 | D 변화 | 검산 |
| --- | --- | --- | --- |
| UPDATE Reserve | 0 | 0 | `entries + max(0, C'−C) + D ≤ 16188`. 아니면 첫 효과 전 Capacity(C preflight·reserve), 생산자에서는 OrderMismatch |
| UpdateForward, C'>C | `+(C'−C)` | 0 | 같은 식을 다시 본다. Reserve→Forward는 한 gate 보유 안이므로 어긋나면 손상(OrderMismatch) |
| UpdateForward, C'≤C | `−(C−C')` | `+(C−C')` | 합 불변 |
| UpdateClr | `+(C−C')` | `−max(0, C−C')` | C≥C'이면 합 불변, C<C'이면 합 감소 |
| UpdatePrune | 0(같은 C' 재stamp) | `−max(0, C−C')` | 합 감소 |

Reserved 상태의 update는 D에 넣지 않는다(DELETE와 같은 이유). 그래서 rollback·복구의 어떤 순서에서도 불변식이 유지되고, UpdateClr은 split 없이 옛 행을 되돌린다. INSERT Reserve·Forward와 delete 단계의 기존 검산은 D의 식만 넓어지고 그대로다. UPDATE 행은 inline이라 entry ≥ 189 B이므로 `ROW_EDIT_CELLS` 87의 전제(§4.3 of DELETE)는 바뀌지 않는다.

<a id="previous-proof"></a>
### 4.4 previous root H의 증명

[DELETE §4.4](dev3-row-delete.md#previous-proof)의 효과1 H 규칙을 효과2 H에 그대로 쓴다. 효과2 H의 U는 효과1 H와 같은 Live RowValue `[16352−(432+L_H), 16352)`(`L_H = H.cell_bytes−180`)이고, 진입 (b)로 Job에서 오는 필드와 before kind RowValue, before_stamp `Committed(_)`, logical 존재, payload 길이 `L_H`를 본다. 이 규칙은 C undo 관찰(`proof_page`), C transition의 `live_material`, A producer의 H 증명에 같이 들어가며, 세 곳은 "효과 0인가 아닌가"로 갈린다. base4는 (a)로 본다. 같은 거래에서 UPDATE를 두 번(다른 행) 하거나 UPDATE 뒤 INSERT·DELETE를 하면 이 경로를 지난다(RUP-C·J).

<a id="classification"></a>
## 5. 대상 판정과 오류

C의 preflight가 UPDATE 대상을 statement snapshot S로 판정한다. 판정 표는 [DELETE §5](dev3-row-delete.md#classification)와 같고 결과 이름만 Delete→Update다. 다른 거래의 Inline `Transaction(T')` 행은 이제 INSERT 행일 수도 UPDATE 행일 수도 있으므로, 그 행을 쥔 root Job으로 가른다. 이 갈래는 DELETE 판정에도 같이 들어간다.

| leaf의 대상 | DELETE·UPDATE 결과 |
| --- | --- |
| 키 없음 | 0행 |
| `Committed(q)`·undo None·Inline, q>S | 0행 |
| `Committed(q)`·undo None·Inline, q≤S, 대상 RowId를 쥔 root Job 없음 | 진행 |
| `Committed(q)`·undo None·Inline, q≤S, 대상 RowId를 쥔 root Job 있음 | Busy |
| `Transaction(own)` tombstone | 0행 |
| `Transaction(own)` Inline(own INSERT·own UPDATE) | Unsupported |
| `Transaction(T')` tombstone | DELETE 표 그대로(Leased Busy, q'≤S 0행, q'>S Serializable Conflict·RC 0행) |
| `Transaction(T')` Inline, root 효과0(INSERT), T' Leased 또는 q'>S | 0행 |
| `Transaction(T')` Inline, root 효과0, q'≤S | Busy |
| **`Transaction(T')` Inline, root 효과2(UPDATE), T' Leased** | **Busy** |
| **`Transaction(T')` Inline, root 효과2, q'≤S** | **Busy**(Prune 전 행) |
| **`Transaction(T')` Inline, root 효과2, q'>S** | **Serializable: SerializationConflict, ReadCommitted: Busy** |
| Overflow payload(옛 행) | Unsupported |
| Committed tombstone, Committed 행의 undo Some, terminal이 없거나 aborted인 `Transaction(T')` 행, **Inline `Transaction(T')` 행을 쥔 root가 T'의 Retained 효과0·2 root 하나가 아님** | 손상(Fatal) |

**root로 가르는 이유.** UpdateForward가 행을 `Transaction(T')`로 바꾸는 순간부터 UpdatePrune이 재stamp하기 전까지 그 RowId는 T'의 Retained 효과2 root가 쥔다. INSERT 행도 같은 동안 효과0 root가 쥔다. census가 모든 non-Free root Job 사이에서 RowId 유일성을 요구하므로 짝은 정확히 하나다. preflight는 이미 Jobs bank를 읽으므로(RowId Busy 검사) U page를 더 읽지 않는다. T' UPDATE 행의 옛 값은 S에 보이므로 0행이 아니다. 쓰기 충돌이고 첫 경로에는 대기가 없으므로 Busy다. q'>S의 Serializable은 commit 검증과 같은 결론을 일찍 낸다. ReadCommitted의 q'>S는 A 실행기가 snapshot을 preflight 바로 앞에서 잡으므로 C 판정 표 시험으로만 본다(DELETE와 같다).

**새 값 검사.** 새 값은 A가 table 스키마로 canonical을 만들고 C가 Reserve에서 다시 decode한다(INSERT와 같다). L'이 2048을 넘으면 Unsupported, 스키마에 맞지 않으면 A의 InvalidInput이다. 오류 우선순위는 DELETE와 같고 Reserve admission 단계에 leaf 공간의 Capacity(§4.3)가 더해진다: 입구 origin/open/statement·fence → RowId 0·값 인자(InvalidInput) → statement credit → table·schema·index·leaf level(Unsupported) → 위 판정 → Reserve admission(queue·inventory·F05·leaf 공간의 Capacity) → 효과 재검사. 관측한 손상을 Busy·0행으로 낮추지 않는다.

<a id="api"></a>
## 6. API 흐름

INSERT·DELETE의 consuming 경계를 그대로 따른다. C 진입은 preflight의 대상 인자에 UPDATE를 더한 것과 그 결과의 `reserve_update(canonical)`이다. `ReservedRow`·`producer_lane`·`prepare`·`apply`·`Cancel`과 statement/commit/rollback 메서드는 공유한다. `Change`에 `ReserveUpdate(&UpdatePlan)`, `UpdateForward`, `UpdateClr`, `UpdatePrune`을 더한다. `UpdatePlan`은 `ReservePlan`의 terminal·container·table·statement와 대상 RowId, A의 새 canonical을 갖는다. replay의 `ReserveAdopted`는 Job after에서 효과·RowId·C'·projection을 채택하고 C는 leaf의 옛 행에서 다시 계산해 비교한다. `Change` 96 B·`Inputs` 448 B 단언은 유지한다.

A는 새 행 cell(UpdateForward)과 복원 cell(UpdateClr)을 만든다. 새 행 cell은 statement의 canonical(candidate 구간)로, 복원 cell은 lane의 U before image에서 table 스키마로(진입 a) 만든다. C 관찰자가 각각 Job의 C'·projection과 U를 독립 검산한다. A의 UPDATE 본문은 module A 경로(`storage/row/insert/` 아래)에 둔다. A 진입점은 `autocommit_update_live`와 `transaction_update_live`이며 자동커밋·명시 거래 실행기의 단계와 slot을 그대로 쓴다. 결과는 `Updated(0|1)`이다. 효과 전 거절과 실패 statement는 INSERT·DELETE 실행기와 같은 경로로 끝낸다.

<a id="read"></a>
## 7. B 읽기

C의 capture는 지금처럼 Inline 행과 DELETE tombstone을 받고 TxnPin(T)·UndoPin(U)을 발급한다. B의 판정은 바뀌지 않는다: own이면 지금 값(새 값), committed q_T≤S면 지금 값, 그 밖(T Active·Aborting·Aborted, 또는 q_T>S)은 before를 읽는다. 바뀌는 것은 Inline `Transaction(T)` 행의 before다.

| 단계 | 지금 | UPDATE 뒤 |
| --- | --- | --- |
| B → `missing_before` | U를 스키마 없이 decode. Missing이면 증거, 유효한 RowDeleted·chunk RowValue는 Unsupported, inline RowValue는 codec 오류로 Fatal | U를 먼저 진입 (b)로 본다. Missing이면 지금과 같다. **Live RowValue·`Committed` stamp·logical·Inline이면 `ValueBefore` 보고**(값은 아직 읽지 않음). 그 밖은 지금과 같다 |
| B → `value_before` | tombstone 후보만 | tombstone 후보, 또는 **같은 capture에서 `missing_before`가 `ValueBefore`를 보고한 Inline 후보**. 그 밖은 Unchanged WrongState |
| B 판정 | — | `q_old≤S`면 `copy_before`로 옛 값, 아니면 없음 |

`value_before`의 검증은 그대로다: U를 operation의 Catalog 스키마로 진입 (a) decode하고, `tombstone_before`(이름은 before 판정 일반으로 바꿀 수 있다)가 T·Status 세대·mutation과 RowValue·`Committed(q_old)`·logical·Inline을 요구한다. 첫 경로는 이 행의 U를 두 번 읽는다(진입 b, 그다음 a). 한 번 읽기로 합치려면 `missing_before`가 스키마와 copy 버퍼를 가져야 해 그 단계의 읽기 census가 커진다. 첫 경로는 각 단계의 frame을 지금 크기로 두는 쪽을 택한다(§14).

reader와 writer의 배제는 DELETE와 같다. Row 쓰기(preflight에서 Reserve·Forward/Cancel까지, rollback, live 정리)는 terminal read gate를 닫고 들어온 read operation을 기다린다. B read operation은 capture부터 finish까지 admission을 쥐므로 `missing_before`와 `value_before` 사이에 UpdateClr·UpdatePrune·RootFree가 끼어들 수 없다. `value_before`가 Releasing U를 보면 손상(Fatal)이다.

<a id="retirement"></a>
## 8. 회수와 가시성

UpdatePrune은 retiring group이고 DeletePrune과 같은 `ValuePrune` scope(q≤published·q≤horizon, U에 다른 pin 0)를 쓴다. Prune이 U의 옛 값을 버리므로 그 값을 읽을 snapshot(S<q)이 없어야 한다. 조건이 안 되면 Busy이고 live 정리는 기존 `skip`으로 다음 root로 넘어간다. `row_cleanup_next`와 복구 cleanup은 효과2 root에 UpdatePrune을 고른다. UpdateClr은 retiring group이 아니다(DeleteClr과 같은 이유). ReleaseRole·RootFree·TerminalFinish는 그대로다.

<a id="recovery"></a>
## 9. 복구

- replay: C의 `observe_borrowed`가 UpdateForward·UpdateClr·UpdatePrune을 관찰하고, Reserve 관찰은 효과2를 받는다. 07-2는 제품 replay(`history/row.rs`)에서 효과2 RowReserve와 10·13·24를 Unsupported로 두고 07-3이 연다(DELETE 2-b-1과 같은 귀납).
- loser: A의 Row loser handler가 효과2 Retained root마다 UpdateClr을 만든다. U는 Live라 payload가 있다.
- winner cleanup: C의 `prepare_row_recovery`가 효과2 root에 UpdatePrune을 고른다.
- base4 census가 효과2를 받는다(아래). base4는 table 스키마를 가지므로 Live update U는 진입 (a)로 canonical까지 본다.

| 효과2 Job 상태 | 키 | 행과 U |
| --- | --- | --- |
| Reserved | 있음 | `Committed`, undo None, Inline, cell = `job.cell_bytes`(옛 C), logical은 actor의 것이 아님. U는 `INITIAL_UNDO_CELL`의 Free |
| Retained | 있음 | `Transaction(actor)`, logical (actor, mutation), undo = `job.undo`, Inline, cell = `job.new_cell`(C'), projection = Job. U는 `[16352−(432+L),16352)`의 Live(`L = cell_bytes−180`), RowValue, before_stamp `Committed(_)` |
| ReleasingCompensated | 있음 | `Committed`, undo None, Inline. logical = U before_logical(모든 cursor), stamp = U before_stamp(cursor<3) |
| ReleasingCommitted | 있음 | `Committed(job.commit_q)`, undo None, Inline, logical (actor, mutation) |

역방향 scan: actor가 stamp한 Inline 행 하나는 정확히 하나의 Retained 효과0 **또는 효과2** Job과 짝을 이룬다(지금은 효과0만). tombstone은 지금처럼 효과1과 짝이다. 효과2 짝의 cell은 Job의 C'이다. §4.3의 넓힌 불변식을 base4도 같은 식으로 검산한다. ReleasingCommitted에 남은 키(INSERT와 같음)와 ReleasingCompensated의 복원 행은 Committed 행이므로 다른 root가 같은 RowId를 쥐지 않는지는 지금의 RowId 유일성 검사가 본다.

<a id="resources"></a>
## 10. 자원·F4·F05

- 새 target이나 pin은 없다. Row target 9개, private before9/after5, registry의 snapshot68·reference66과 두 pin이 그대로다.
- A의 UPDATE 경로는 DELETE와 같은 F4 고정 상한 안에 들어야 한다([row-delete §10](dev3-row-delete.md#resources)의 목록, insert site 상한을 update site에도). 넘으면 원 항·여유를 깎거나 상한을 올리지 않고 A 축소 또는 사용자가 확인하는 새 계약으로 간다.
- C의 새 관찰·생산 본문은 기존 Row write census(`writable::source_bytes`, 본문 44,304 B charge)와 preflight·RowReserve census에 넣어 검산한다. DELETE 뒤 C 여유는 preflight 728 B, RowReserve 595 B다. 모자라면 같은 규칙으로 멈추고 묻는다.
- B 읽기 쪽: `missing_before`의 진입 (b) 판정과 `value_before`의 Inline 후보는 기존 읽기 census S5R-C01/C02의 `full` 상계(34,586 B, cursor load의 여유 126 B)를 같게 통과해야 한다.
- frame 최대 97936 B는 FRAME 98304 안이다(§3.4). root당 cleanup 최대 318240 B는 설명만 바뀐다(§3.5).

<a id="files"></a>
## 11. 변경 파일 inventory

| 모듈 | 파일 | 변경 |
| --- | --- | --- |
| semantic | `semantic_row/{operation,wire,core}.rs` | opcode 10·13·24, 두 cell shape, native 크기 규칙, kind1 header로 C' 유도 |
| semantic | `semantic_row/transition.rs`, `transition/{prepare,cells,graph,frame,source}.rs` | Change 변형, update Reserve·Forward·Clr·Prune 준비, 넓힌 D, 효과2 H |
| semantic | `semantic_row/{undo,leaf,jobs,ledger,status}.rs` | 효과2 U·H 증명, 바뀐 행 leaf 관찰, 효과·opcode 묶음, 비용식 |
| codec·C | `retention_page/row/job.rs` | 효과2, `[286,288)` C', 식 |
| C | `plan4_retention/{row_preflight,row_write,row_forward,row_clr,row_rollback_select}.rs` | UPDATE 판정·Busy, 효과로 가르는 Inline 행 판정, 예약·Capacity |
| C | `plan4_retention/row_start/{retire,recovery}.rs`, `recovery/catalog.rs`, `visibility/*` | UpdatePrune 선택, `ValuePrune` |
| C | `plan4_apply/row/{read,write,recovery,rollback}.rs`, `plan4_recovery/row_action.rs`, `plan4_retention/history/row.rs` | `missing_before`의 ValueBefore, `value_before`의 Inline 후보, replay·recovery |
| C | `plan4_recovery_base/row/{actor,graph}.rs` | base4 효과2·역방향 scan·넓힌 D |
| A | `row/insert/{group,leaf}.rs`, `row/insert/group/*`, `row/insert/driver/*` | update 생산자·H 증명·실행기·loser |
| B | `row/read/**` | `missing_before`의 ValueBefore 갈래 |
| 문서 | `dev3-undo-format.md`, `dev3-module-consumers.md`(shape 표·wire 기록·`candidate_template`), `dev3-module-boundaries.md`, `dev3-resource-bounds.md`, `plan4_apply.rs` FRAME 주석 | §3.2 개정 표시, 새 opcode 행, 최대 frame·cleanup 설명·fixture 상계 |

<a id="tests"></a>
## 12. 시험 inventory

| ID | 단계 | 내용 |
| --- | --- | --- |
| RUP-F01..08 | 07-2 | literal: opcode decode·shape·frame(C·C' 경계 185/2228, h 0/1, C<C'·C=C'·C>C'), kind1 header로 C' 유도와 그 거절(kind1 없음, 범위 밖, 홀수 합), native 크기 규칙, 효과2 수용·거절(효과 3, `@285`≠0, C' 범위 밖, 효과0·1의 `[286,288)`≠0, Releasing의 C' 잔여), 효과별 식·`has_previous`, ledger 행 |
| RUP-C01..12 | 07-2 | C 전이: 독립 저자가 만든 이력 U의 literal과 바이트 비교(Reserve(update)·UpdatePrune·Cancel 등 C 몫, UpdateForward·UpdateClr은 관찰), mutant(새 행 cell 크기·projection, 옛 행 미Prune, 다른 RowId, 복원 값), 효과2 H proof, leaf 공간(커지는 Reserve·Forward의 경계, 줄어드는 update의 D로 INSERT Capacity), RowId를 쥔 root, `ReserveAdopted` 효과2, 효과와 opcode 불일치 |
| RUP-A01..08 | 07-3 | A 실행기: 1행·0행(키 없음, own tombstone, 안 보이는 행), Busy, Unsupported(own INSERT·own UPDATE·긴 새 값), SerializationConflict, 명시 거래 statement 실패 뒤 계속, F4 census, A 생산자의 literal 바이트 대조 |
| RUP-J01..10 | 07-3 | 실제 C 위: 갱신→commit→정리→새 값, 갱신→rollback→옛 값(커지는·줄어드는 값), 명시 거래 갱신 둘·statement rollback, 갱신 뒤 INSERT·DELETE(효과2 H), 정리 전 읽기, 재시작 replay, Busy(다른 거래의 UPDATE 행에 대한 DELETE·UPDATE), jobs·status·undo·leaf 관찰자 대조 |
| RUP-B01..07 | 07-3 | B: own 새 값, committed q≤S 새 값, 진행 중 거래의 옛 값, q_old>S 없음, q_T>S 옛 값, 읽기 중 UpdateClr의 gate 대기, Releasing U Fatal·ValueBefore 없이 Inline 후보의 `value_before` 거절, 읽기 census |
| RUP-R01..07 | 07-3 | 복구(메모리): loser UpdateClr, winner UpdatePrune, 효과2 네 상태의 base4 수용, 짝 없는 Inline 행·cell C' 불일치·비canonical update U·넓힌 D 위반 거절, release cursor 3 |
| RUP-N01..06 | 07-4 | native fault cut: UpdateForward·UpdateClr·UpdatePrune 앞뒤의 crash, 사례마다 produce·recover·reenter 세 process(DELETE §19와 같은 controller 규칙) |

<a id="stages"></a>
## 13. 단계와 PR

| 단계 | 일정 항목 | 내용 | 끝 조건 |
| --- | --- | --- | --- |
| 1 | 07-1 | 이 문서, 독립 D2 검토 | 검토 ACCEPT, 사용자 병합 |
| 2 | 07-2 | C 형식·전이층: §3 전부, §4의 C whole-group 전이(UpdateForward·UpdateClr은 관찰만, Reserve(update)·UpdatePrune은 생산), 넓힌 D, 효과2 H, 독립 저자 literal, RUP-F·C. UPDATE를 쓰는 제품 진입점은 없다. 제품 replay는 효과2를 Unsupported로 거절한다 | 변경 시험만 실행, gate record, 독립 검토, PR |
| 3 | 07-3 | 제품 경로: C preflight·reserve_update와 판정(§5, DELETE 갈래 포함), A 생산자·실행기·loser, B 읽기(§7), live UpdatePrune, replay·recovery·base4, 시험 전용 관찰자, RUP-A·J·B·R | 같음 |
| 4 | 07-4 | native fault cut과 남은 시험 | 같음. 07의 마지막 항목이라 [§3 행 조건](../plans/dev3-schedule-baseline.md#completion)을 모두 채워야 적립 |

DELETE와 같은 규칙으로, **영속 update 행·update U를 쓰는 제품 진입점은 그것을 읽고 복구하는 B·base4·loser와 같은 PR에 둔다.** 그래서 2단계는 C 전이만 갖고 3단계가 쓰기·읽기·복구를 함께 연다.

<a id="rejected"></a>
## 14. 기각한 대안

- **UPDATE = DELETE + INSERT(같은 RowId)**: 두 root가 같은 RowId를 쥐어 census의 RowId 유일성을 깨고, 두 group 사이 crash가 행을 잃은 상태를 남긴다.
- **새 RowId로 옮기는 UPDATE**: 3.06-R4의 RowId 영속 locator 의미를 깬다.
- **U에 새 값도 담기**: U가 `432+L+L'`까지 커져 셀·scratch·undo root 최대(2480)를 넘고, 새 값은 leaf와 Job commitment로 충분하다.
- **C'을 core(`[240,256)`)에 싣기**: core grammar에 opcode별 field가 생긴다. kind1 native header가 이미 C'을 운반한다.
- **`@280`을 새 C'으로**: U 크기·H 증명·base4 U 범위가 효과 1과 2에서 다른 식이 된다(§3.3).
- **별도 Job kind**: DELETE §14와 같은 이유.
- **같은 크기 UPDATE만**: 일정 07의 "크기 변화의 leaf 공간"을 빼게 된다.
- **커지는 UPDATE의 split**: 첫 profile은 leaf 하나다. Capacity로 거절하고 SMO는 일정 08이다.
- **`missing_before`가 옛 값까지 읽기(U 한 번 읽기)**: 그 단계가 스키마 decode와 copy 버퍼를 가져 읽기 census 여유(126 B)를 넘을 위험이 크다. 두 번 읽기의 비용은 첫 경로의 한계로 둔다.
- **다른 거래 UPDATE 행을 0행으로**: 옛 값이 S에 보이는 행을 없다고 답한다.
- **own 행 재갱신**: version chain·3pin이 필요하다(일정 08-4).

<a id="open"></a>
## 15. 2단계·3단계가 확인할 것

1. kind1 header로 C'을 먼저 얻는 decode가 `validate_native_shape`와 같은 규칙을 지나는지(§3.4).
2. 효과별 비용식 자리와 최대 frame·cleanup 설명 갱신(§3.5).
3. 넓힌 D를 C(`typed_leaf`·`reserve_leaf`·update Reserve), A, leaf.rs, base4가 같은 식으로 검산하는지(§4.3).
4. 효과2 H 증명을 세 곳이 "효과 0이 아니면" 규칙으로 쓰는지(§4.4).
5. DELETE 판정의 Inline 갈래 변경이 RDL 시험의 기대를 바꾸지 않는지(효과0 행의 결과는 같다, §5).
6. RDL-B09의 후반(Inline `Transaction(T)` 행 아래 delete RowValue U가 Fatal)은 UPDATE 뒤 유효한 모양이 된다. B09는 07-3에서 "Job이 효과1인데 행이 Inline"이 base4·C에서 손상인지로 바꾸고, B 읽기는 그 모양을 UPDATE 행으로 읽는다는 점을 기록한다.
7. A·B·C census(§10).

## 작성 기록

- 2026-11-04 r1 초안. main `5760f30`의 source를 읽고 썼다(operation.rs, wire.rs, core.rs, job.rs, undo_record.rs, transition.rs, transition/prepare.rs, ledger.rs, row_preflight.rs의 `delete_class`, plan4_apply/row/read.rs의 `missing_before`·`value_before`, row/read/driver.rs, plan4_recovery_base/row/actor.rs, plan4_apply.rs의 `RowLoanLayout`).

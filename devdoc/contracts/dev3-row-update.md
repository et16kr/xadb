# DEV3 Row UPDATE 첫 경로 — 형식·전이 설계

2026-11-04 · `D3-ROW-UPDATE-001` · **설계 동결 / D2 ACCEPTED(2회차) / 2단계(C 형식·전이, §16)·3단계(제품 경로, 메모리 시험, §17)·4단계(native fault cut, §18) 구현**

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
| B 읽기 | 안 보이는 Inline `Transaction(T)` 행은 `missing_before`(스키마 없는 decode. inline RowValue before는 codec 오류로 Fatal, 유효한 chunk RowValue·RowDeleted before는 Unsupported) | `missing_before`가 RowValue U를 보고하고 B가 `value_before`로 옛 값을 읽는다(§7) |
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

`@280`을 옛 C로 두는 이유: U의 크기(`432+C−180`), H 증명(§4.4), `live_undo_bytes`, base4의 U 셀 범위가 효과 1·2에서 같은 식이 된다. C'이 쓰이는 곳은 UpdateForward의 새 행 cell, UpdateClr의 before cell·frame·native 크기, UpdatePrune의 재stamp 행 cell, 세 opcode의 shape·wire decode(§3.4), cleanup budget과 `has_previous_transaction`(§3.5), 넓힌 D(§4.3), rollback 선택이 producer에 넘기는 현재 행 길이(§6), base4의 Retained 행 cell(§9)이다. 옛 C와 새 C'은 Releasing에서 둘 다 지워진다(§9).

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

**wire decode.** native 합 하나로는 C와 C'을 함께 유도할 수 없다. decode는 native 구간에서 kind1 record 하나의 header를 먼저 읽어 C'을 얻는다(UpdateForward는 after 크기 `@16`, UpdateClr·UpdatePrune은 before 크기 `@12`). 이 사전 scan은 frame 크기 검산보다 앞서므로 길이 불변식에 기대지 않는다: native 구간 `[native_at, native_at+native)`가 frame 안에 있어야 하고, record를 앞에서부터 **최대 5개**(Row opcode의 native_count)만 걷으며, 각 record 길이(`@20`)는 128 이상이고 구간의 남은 바이트 이하여야 한다. 5개를 걷은 뒤 남은 바이트가 있으면 손상이다. 그다음 C를 합에서 검사된 뺄셈으로 유도한다: UpdateForward·UpdateClr은 `(native−4844−C')/2`(홀수면 손상), UpdatePrune은 `native−4844−2C'`. kind1 record가 없거나 둘 이상이거나, 길이 0·128 미만·남은 구간 초과 record, 음수가 되는 뺄셈, C·C'이 185..2228 밖이면 모두 **Corrupt**다(사전 scan의 kind1 중복도 Corrupt이며, kind1 아닌 kind의 중복은 지금처럼 `validate_native_shape`의 IdentityMismatch다). 사전 scan은 `NativeRecords`를 쓰지 않는다. 그 뒤 `validate_native_shape`가 모든 native의 presence·크기를 (C, C')로 다시 검산하므로 먼저 읽은 header도 같은 규칙을 지나간다. `Shape`는 `cell`(C)과 `new_cell`(C', 효과2 opcode만, 나머지 0)을 갖는다. 기존 opcode의 decode는 바뀌지 않는다. 옛 binary는 10·13·24를 Unsupported로, 효과 2인 Job을 Corrupt로 거절한다([버전 정책](../policies/version-compatibility-policy.json)의 개발 형식 규칙). [scope3 wire 기록](dev3-module-consumers.md#row-scope3-wire)의 존재 규칙에 RowLeaf kind1의 present(C)→present(C'), present(C')→present(C), present(C')→present(C')를 더한다(07-2).

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
| leaf entry가 Value가 아님 | Unsupported(지금과 같음) |
| Committed tombstone, Committed 행의 undo Some, `Transaction` 행의 undo None, terminal이 없거나 aborted인 `Transaction(T')` 행, **Inline `Transaction(T')` 행을 쥔 root가 아래 묶음을 만족하는 root 정확히 하나가 아님** | 손상(Fatal) |

**root로 가르는 이유.** UpdateForward가 행을 `Transaction(T')`로 바꾸는 순간부터 UpdatePrune이 재stamp하기 전까지 그 RowId는 T'의 Retained 효과2 root가 쥔다. INSERT 행도 같은 동안 효과0 root가 쥔다. census가 모든 non-Free root Job 사이에서 RowId 유일성을 요구하므로 짝은 정확히 하나다. 판정은 그 root를 행에 다음 필드로 묶는다(base4 `actor.rs`의 Retained 짝 검사와 같은 묶음): Row scope, `RootRetained`, 효과 0 또는 2, source == T'(id와 Status 세대), object·generation == 대상 table, RowId == 행의 RowId, undo == 행의 undo ref. 이 중 하나라도 어긋나거나, 같은 RowId를 쥔 root가 0개 또는 2개 이상이면 손상(Fatal)이다. 손상된 bank가 결과를 0행·Busy·SerializationConflict 사이에서 바꾸지 못하게 하기 위해서다(RUP-C·A mutant). preflight는 이미 Jobs bank를 읽으므로(RowId Busy 검사) U page를 더 읽지 않는다. T' UPDATE 행의 옛 값은 S에 보이므로 0행이 아니다. 쓰기 충돌이고 첫 경로에는 대기가 없으므로 Busy다. q'>S의 Serializable은 commit 검증과 같은 결론을 일찍 낸다. ReadCommitted의 q'>S는 A 실행기가 snapshot을 preflight 바로 앞에서 잡으므로 C 판정 표 시험으로만 본다(DELETE와 같다).

**새 값 검사.** 새 값은 A가 table 스키마로 canonical을 만들고 C가 Reserve에서 다시 decode한다(INSERT와 같다). L'이 2048을 넘으면 Unsupported, 스키마에 맞지 않으면 A의 InvalidInput이다. 오류 우선순위는 DELETE와 같고 Reserve admission 단계에 leaf 공간의 Capacity(§4.3)가 더해진다: 입구 origin/open/statement·fence → RowId 0·값 인자(스키마 불일치는 InvalidInput) → statement credit → table·schema·index·leaf level과 새 값 L'>2048(Unsupported, INSERT의 긴 값 거절과 같은 자리) → 위 판정 → Reserve admission(queue·inventory·F05·leaf 공간의 Capacity) → 효과 재검사. 관측한 손상을 Busy·0행으로 낮추지 않는다.

<a id="api"></a>
## 6. API 흐름

INSERT·DELETE의 consuming 경계를 그대로 따른다. C 진입은 preflight의 대상 인자에 UPDATE를 더한 것과 그 결과의 `reserve_update(canonical)`이다. `ReservedRow`·`producer_lane`·`prepare`·`apply`·`Cancel`과 statement/commit/rollback 메서드는 공유한다. `Change`에 `ReserveUpdate(&UpdatePlan)`, `UpdateForward`, `UpdateClr`, `UpdatePrune`을 더한다. `UpdatePlan`은 `ReservePlan`의 terminal·container·table·statement와 대상 RowId, A의 새 canonical을 갖는다. replay의 `ReserveAdopted`는 Job after에서 효과·RowId·C'·projection을 채택하고 C는 leaf의 옛 행에서 다시 계산해 비교한다. `Change` 96 B·`Inputs` 448 B 단언은 유지한다.

**A↔C 경계의 UPDATE 값.** 지금 경계의 필드는 cell 길이 하나를 가정한다. 07-3은 다음처럼 넓힌다.

| 자리 | 지금 | UPDATE |
| --- | --- | --- |
| `RowPreflightCut`(`wal_io/plan4_apply/row.rs`) | `delete: Option<RowId>`, INSERT는 None | 대상 인자를 `target: Option<(RowId, effect)>`처럼 효과를 가진 값으로 바꾼다. `reserve`는 None, `reserve_delete`는 효과1, `reserve_update`는 효과2만 받는다 |
| `row_write.rs` reserve의 (input, cut) 짝 | `(Some, None)` INSERT, `(None, Some)` DELETE, 그 밖 Invariant Fatal | `(Some(새 canonical), Some(대상, 효과2))`를 UPDATE로 받는다. 봉인 input은 새 canonical의 길이·digest다(INSERT와 같음) |
| `RowDraftCut.input_len`·`input` | INSERT Forward: canonical 길이·digest. DELETE Forward: 옛 행 cell C·옛 행 projection(`row_write.rs` reserve). CLR(rollback 선택): `job.cell_bytes()`·`job.projection_digest()` | UPDATE Forward: 새 canonical 길이·digest. 효과2 CLR의 현재 행 길이는 `job.new_cell()`(C'), 현재 projection은 `job.projection_digest()`(새 행의 것)다. 효과 0·1은 그대로 |
| `RowProducerInput`(`wal_io/plan4_apply.rs`) | `Insert`·`Delete`·`Clr{current_row_len, current_projection, ..}` | `Update{canonical_len, canonical_digest, old_cell, projection}`를 더한다. `old_cell`은 `job.cell_bytes()`(옛 C), `projection`은 `job.projection_digest()`로 **계획한 새 행**의 projection이다(`Delete.projection`이 옛 행의 것인 것과 다르다). `Clr`의 `current_row_len`은 효과2에서 C', 복원 길이 C는 U before에서 얻고 Job `@280`과 대조한다 |
| `ReservedEntry.delete: bool`(`plan4_retention/row_forward.rs`, `row_clr.rs`) | opcode를 INSERT·DELETE 중에서 고른다 | 효과를 담는 값으로 바꿔 UpdateForward·UpdateClr을 고른다 |

이 넷은 07-3에서 같은 PR로 바꾸며 07-2는 손대지 않는다(제품 진입점이 없다).

A는 새 행 cell(UpdateForward)과 복원 cell(UpdateClr)을 만든다. 새 행 cell은 statement의 canonical(candidate 구간)로, 복원 cell은 lane의 U before image에서 table 스키마로(진입 a) 만든다. C 관찰자가 각각 Job의 C'·projection과 U를 독립 검산한다. A의 UPDATE 본문은 module A 경로(`storage/row/insert/` 아래)에 둔다. A 진입점은 `autocommit_update_live`와 `transaction_update_live`이며 자동커밋·명시 거래 실행기의 단계와 slot을 그대로 쓴다. 결과는 `Updated(0|1)`이다. 효과 전 거절과 실패 statement는 INSERT·DELETE 실행기와 같은 경로로 끝낸다.

<a id="read"></a>
## 7. B 읽기

C의 capture는 지금처럼 Inline 행과 DELETE tombstone을 받고 TxnPin(T)·UndoPin(U)을 발급한다. B의 판정은 바뀌지 않는다: own이면 지금 값(새 값), committed q_T≤S면 지금 값, 그 밖(T Active·Aborting·Aborted, 또는 q_T>S)은 before를 읽는다. 바뀌는 것은 Inline `Transaction(T)` 행의 before다.

| 단계 | 지금 | UPDATE 뒤 |
| --- | --- | --- |
| B → `missing_before` | U를 스키마 없이 decode. Missing이면 증거, 유효한 RowDeleted·chunk RowValue는 Unsupported, inline RowValue는 codec 오류로 Fatal | U를 진입 (b)(`lookup_undo_record_metadata`)로 보고 아래 outline 표로 판정한다. **Live RowValue·`Committed` stamp·logical·Inline 5..2048이면 `ValueBefore` 보고**(값은 읽지 않음) |
| B → `value_before` | tombstone 후보만 | tombstone 후보, 또는 **같은 capture에서 `missing_before`가 `ValueBefore`를 보고한 Inline 후보**. 그 밖은 Unchanged WrongState |
| B 판정 | — | `q_old≤S`면 `copy_before`로 옛 값, 아니면 없음 |

**`missing_before`의 (b) outline 판정.** 아래 표를 위에서부터 적용한다. "codec" 행은 (b) 진입이 outline을 만들기 전에 거절하므로 RUP-B07은 그런 outline을 만들지 않고 해당 바이트를 만든 U로 시험한다. identity 검사는 지금 (a) 결과에서 하던 것(creator id, `Transaction` stamp의 actor id·세대, mutation sequence, sequence≠0)을 `outline.metadata`에서 한다. (b) 진입 자체가 거절하는 header·길이 손상은 지금처럼 codec 오류 Fatal이다.

| outline | 결과 | 거절 층 |
| --- | --- | --- |
| state ≠ Live: Free | Fatal | codec(occupied 진입이 Free를 거절) |
| state ≠ Live: Releasing | Fatal | read.rs 판정 |
| identity 불일치 | Fatal | read.rs 판정 |
| Missing, stamp·logical·payload 모두 없음 | 증거(`MissingRowEvidence`) | — |
| Missing인데 stamp·logical이 있음 | Fatal | codec |
| Missing인데 payload가 있음 | Fatal | codec 또는 read.rs 판정 |
| RowValue, stamp `Committed`, logical 있음, Inline, 길이 5..2048 | `ValueBefore` | — |
| RowValue Chunks, RowDeleted | Unsupported(Unchanged, 지금과 같음) | read.rs 판정 |
| RowValue인데 stamp 없음, logical 없음, creator가 아닌 `Transaction` stamp, Live의 payload None | Fatal | codec |
| RowValue인데 creator의 `Transaction` stamp, Inline 길이 5..2048 밖 | Fatal | read.rs 판정 |

(b) 진입은 [Undo 형식 2026-11-04 개정](dev3-undo-format.md)이 닫은 사용처 목록에 이 용도로 더해진다. outline에는 payload 접근자가 없으므로 이 단계는 값을 내주지 않는다.

**옛 값의 자리.** 지금 `value_before`는 capture가 새 값을 둔 copy 구간(`copy[8192..8192+2048)`)에 옛 값을 쓴다. tombstone 후보는 새 값이 없어서 안전했지만 Inline 후보에서는 `copy_candidate`가 옛 값을 새 version 이름으로 내줄 수 있다. 그래서 `value_before`는 옛 값을 **겹치지 않는 구간 `copy[10240..10240+2048)`**에 쓰고 `copy_before`는 그 구간에서 읽는다. copy 버퍼는 이미 `PAGE_BYTES`(스키마 `[0,8192)`, 값 `[8192,16384)`)라 바이트가 늘지 않고, capture 값과 before 값이 서로를 덮지 않으므로 상태 표시도 필요 없다. `value_before`가 쓰고 release에서 실패해도 capture 값은 그대로다. B의 순서(지금 값이면 `copy_candidate`, before면 `value_before`→`copy_before`)와 무관하게 두 copy는 각자의 값을 내준다(RUP-B07).

`value_before`의 검증은 그대로다: U를 operation의 Catalog 스키마로 진입 (a) decode하고, `tombstone_before`(이름은 before 판정 일반으로 바꿀 수 있다)가 T·Status 세대·mutation과 RowValue·`Committed(q_old)`·logical·Inline을 요구한다. 첫 경로는 이 행의 U를 두 번 읽는다(진입 b, 그다음 a). 한 번 읽기로 합치려면 `missing_before`가 스키마와 copy 버퍼를 가져야 해 그 단계의 읽기 census가 커진다고 **추정**한다(측정하지 않았다). 07-3은 두 번 읽기로 구현하고 그 census를 잰다. 여유가 남으면 진입 (a) 한 번이 `Missing | Value`를 돌려주는 대안을 다시 볼 수 있다(§14).

reader와 writer의 배제는 DELETE와 같다. Row 쓰기(preflight에서 Reserve·Forward/Cancel까지, rollback, live 정리)는 terminal read gate를 닫고 들어온 read operation을 기다린다. B read operation은 capture부터 finish까지 admission을 쥐므로 `missing_before`와 `value_before` 사이에 UpdateClr·UpdatePrune·RootFree가 끼어들 수 없다. `value_before`가 Releasing U를 보면 손상(Fatal)이다.

<a id="retirement"></a>
## 8. 회수와 가시성

UpdatePrune은 retiring group이고 DeletePrune과 같은 `ValuePrune` scope(q≤published·q≤horizon, U에 다른 pin 0)를 쓴다. Prune이 U의 옛 값을 버리므로 그 값을 읽을 snapshot(S<q)이 없어야 한다. 조건이 안 되면 Busy이고 live 정리는 기존 `skip`으로 다음 root로 넘어간다. `row_cleanup_next`와 복구 cleanup은 효과2 root에 UpdatePrune을 고른다. UpdateClr은 retiring group이 아니다(DeleteClr과 같은 이유). ReleaseRole·RootFree·TerminalFinish는 그대로다.

**scope 대응은 빠짐없이 쓴다.** 지금 `visibility/row_retirement.rs`의 `scope(operation)`은 `_ => None`(참조 target 없음)으로 끝난다. UpdatePrune의 arm이 빠지면 `ValuePrune` gate 없이 옛 값을 버려 S<q인 snapshot이 `Committed(q>S)`·undo None 행을 만나고, 읽기 쪽은 이를 MissingUndo 손상으로 본다(`row/read.rs`의 Committed 판정). 그래서 07-3은 `scope()`를 wildcard 없는 match로 바꾸고 UpdatePrune→`ValuePrune`을 넣는다. q>horizon인 UpdatePrune이 Busy이고 live 정리가 `skip`으로 넘어가는 시험을 RUP-J에 둔다.

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

Releasing 두 상태의 Job은 C와 C'을 모두 지운다(`[280,284)`·`[285,320)` zero). Releasing U도 payload가 없다(stamp는 cursor<3까지, logical만 남음). 그래서 복원 행(Compensated)과 재stamp 행(Committed)의 cell 길이는 Job·U가 아니라 그 행의 canonical decode로만 검산되고, U는 복원 행의 stamp·logical만 묶는다. INSERT·DELETE의 Releasing과 같다.

역방향 scan: actor가 stamp한 Inline 행 하나는 정확히 하나의 Retained 효과0 **또는 효과2** Job과 짝을 이룬다(지금은 효과0만). tombstone은 지금처럼 효과1과 짝이다. 효과2 짝의 cell은 Job의 C'이다. §4.3의 넓힌 불변식을 base4도 같은 식으로 검산한다. ReleasingCommitted에 남은 키(INSERT와 같음)와 ReleasingCompensated의 복원 행은 Committed 행이므로 다른 root가 같은 RowId를 쥐지 않는지는 지금의 RowId 유일성 검사가 본다.

<a id="resources"></a>
## 10. 자원·F4·F05

- 새 target이나 pin은 없다. Row target 9개, private before9/after5, registry의 snapshot68·reference66과 두 pin이 그대로다.
- A의 UPDATE 경로는 DELETE와 같은 F4 고정 상한 안에 들어야 한다([row-delete §10](dev3-row-delete.md#resources)의 목록, insert site 상한을 update site에도). 넘으면 원 항·여유를 깎거나 상한을 올리지 않고 A 축소 또는 사용자가 확인하는 새 계약으로 간다.
- C의 새 관찰·생산 본문은 기존 Row write census(`writable::source_bytes`, 본문 44,304 B charge)와 preflight·RowReserve census에 넣어 검산한다. DELETE 뒤 C 여유는 preflight 728 B, RowReserve 595 B다. 모자라면 같은 규칙으로 멈추고 묻는다.
- B 읽기 쪽: `missing_before`의 진입 (b) 판정과 `value_before`의 Inline 후보는 기존 읽기 census S5R-C01/C02의 `full` 상계(34,586 B, cursor load의 여유 126 B)를 같게 통과해야 한다. before 값의 copy 구간은 기존 copy 버퍼 안의 `[10240,12288)`이라 새 바이트가 없다(§7).
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
| A↔C | `wal_io/plan4_apply.rs`(`RowProducerInput`), `wal_io/plan4_apply/row.rs`(`RowPreflightCut`), `plan4_retention/row_rollback_select.rs`(`input_len`), `plan4_retention/{row_forward,row_clr}.rs`(`ReservedEntry.delete`) | §6의 UPDATE 경계 값 |
| C | `plan4_retention/visibility/row_retirement.rs` | `scope()` 빠짐없는 match, UpdatePrune→`ValuePrune` |
| A | `row/insert/{group,leaf}.rs`, `row/insert/group/*`, `row/insert/driver/*` | update 생산자·H 증명·실행기·loser |
| B | `row/read/**` | `missing_before`의 ValueBefore 갈래 |
| 문서 | `dev3-undo-format.md`, `dev3-module-consumers.md`(shape 표·wire 기록·`candidate_template`), `dev3-module-boundaries.md`, `dev3-resource-bounds.md`, `plan4_apply.rs` FRAME 주석 | §3.2 개정 표시, 새 opcode 행, 최대 frame·cleanup 설명·fixture 상계 |

<a id="tests"></a>
## 12. 시험 inventory

| ID | 단계 | 내용 |
| --- | --- | --- |
| RUP-F01..08 | 07-2 | literal: opcode decode·shape·frame(C·C' 경계 185/2228, h 0/1, C<C'·C=C'·C>C'), kind1 header로 C' 유도와 그 거절(kind1 없음·둘, 범위 밖, 홀수 합, 길이 0·128 미만·남은 구간 초과 record, 5개 뒤 남은 바이트), native 크기 규칙, 효과2 수용·거절(효과 3, `@285`≠0, C' 범위 밖, 효과0·1의 `[286,288)`≠0, Releasing의 C' 잔여), 효과별 식·`has_previous`, ledger 행 |
| RUP-C01..12 | 07-2 | C 전이: 독립 저자가 만든 이력 U의 literal과 바이트 비교(Reserve(update)·UpdatePrune·Cancel 등 C 몫, UpdateForward·UpdateClr은 관찰), mutant(새 행 cell 크기·projection, 옛 행 미Prune, 다른 RowId, 복원 값), 효과2 H proof, leaf 공간(커지는 Reserve·Forward의 경계, 줄어드는 update의 D로 INSERT Capacity), RowId를 쥔 root, `ReserveAdopted` 효과2, 효과와 opcode 불일치. 07-3의 판정 시험은 holder root 묶음 mutant(source·상태·효과·table·undo 어긋남, 짝 0·2개)를 Fatal로 본다 |
| RUP-A01..08 | 07-3 | A 실행기: 1행·0행(키 없음, own tombstone, 안 보이는 행), Busy, Unsupported(own INSERT·own UPDATE·긴 새 값), SerializationConflict, 명시 거래 statement 실패 뒤 계속, F4 census, A 생산자의 literal 바이트 대조 |
| RUP-J01..10 | 07-3 | 실제 C 위: 갱신→commit→정리→새 값, 갱신→rollback→옛 값(커지는·줄어드는 값), 명시 거래 갱신 둘·statement rollback, 갱신 뒤 INSERT·DELETE(효과2 H), 정리 전 읽기, 재시작 replay, Busy(다른 거래의 UPDATE 행에 대한 DELETE·UPDATE), jobs·status·undo·leaf 관찰자 대조 |
| RUP-B01..07 | 07-3 | B: own 새 값, committed q≤S 새 값, 진행 중 거래의 옛 값, q_old>S 없음, q_T>S 옛 값, 읽기 중 UpdateClr의 gate 대기, Releasing U Fatal·ValueBefore 없이 Inline 후보의 `value_before` 거절, `value_before` 뒤에도 같은 capture의 `copy_candidate`가 새 값을 내줌(before 구간 분리), (b) outline 표의 각 행(Releasing outline·`Transaction` stamp·logical 없음·길이 범위 밖 Fatal, Chunks·RowDeleted Unsupported), 읽기 census |
| RUP-R01..07 | 07-3 | 복구(메모리): loser UpdateClr, winner UpdatePrune, 효과2 네 상태의 base4 수용, 짝 없는 Inline 행·cell C' 불일치·비canonical update U·넓힌 D 위반 거절, release cursor 3 |
| RUP-N01..06 | 07-4 | native fault cut: UpdateForward·UpdateClr·UpdatePrune 앞뒤의 crash, 사례마다 produce·recover·reenter 세 process(DELETE §19와 같은 controller 규칙) |

<a id="stages"></a>
## 13. 단계와 PR

| 단계 | 일정 항목 | 내용 | 끝 조건 |
| --- | --- | --- | --- |
| 1 | 07-1 | 이 문서, 독립 D2 검토 | 검토 ACCEPT, 사용자 병합 |
| 2 | 07-2 | C 형식·전이층: §3 전부, §4의 C whole-group 전이(UpdateForward·UpdateClr은 관찰만, Reserve(update)·UpdatePrune은 생산), 넓힌 D, 효과2 H, 독립 저자 literal, RUP-F·C. UPDATE를 쓰는 제품 진입점은 없다. 제품 replay는 효과2를 Unsupported로 거절한다. `RowEffect`에 `Update`를 더하고 효과를 읽는 모든 제품 소비자가 효과2를 입구에서 명시적으로 거절하거나 wildcard 없는 match로 다루게 해, 07-3이 여는 곳(A 생산자·H 증명, C forward·clr·live/복구 cleanup 선택, base4 actor·graph, 시험 전용 jobs·undo·leaf 관찰자)은 효과2에서 Unsupported를 돌려준다. 그 목록을 gate record에 적는다(DELETE 2-a record와 같은 방식). `effect() == Delete` 같은 bool 비교는 효과2를 INSERT 쪽으로 보내므로 남기지 않는다 | 변경 시험만 실행, gate record, 독립 검토, PR |
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
- **`missing_before`가 옛 값까지 읽기(U 한 번 읽기)**: 그 단계가 스키마 decode와 copy 버퍼를 가져 읽기 census 여유(126 B)를 넘을 위험이 크다고 추정한다(측정 전). 진입 (a) 한 번이 `Missing | Value`를 돌려주는 형태도 같은 이유로 보류한다. 07-3이 두 번 읽기의 census를 잰 뒤 다시 본다.
- **같은 capture의 `copy_candidate`를 막는 상태 규칙**(r2): 읽기 상태에 표시가 하나 늘고 쓰기 시점·release 실패의 순서를 따로 정해야 한다. copy 버퍼에 이미 남는 구간이 있어 겹치지 않는 구간(§7, r3)이 더 단순하다.
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
8. 효과를 읽는 제품 소비자가 모두 wildcard 없는 match인지(§13 2단계), `scope()`에 UpdatePrune arm이 있는지(§8).
9. B의 (b) outline 표와 겹치지 않는 before copy 구간(§7), holder root 묶음(§5).

<a id="implementation-2"></a>
## 16. 2단계(07-2) 구현 기록

2단계는 C의 형식·전이층이다. UPDATE를 쓰는 제품 진입점은 없다. `Change::ReserveUpdate`·`UpdateForward`·`UpdateClr`·`UpdatePrune`은 시험만 만든다. 그래서 §13의 "쓰는 PR이 읽기·복구도 갖는다" 조건이 지켜진다.

- **형식(§3).** opcode 10·13·24와 두 cell shape(`Shape.new_cell`), 세 frame 식(최대 97936·97728·96128)을 더했다. wire decode는 kind1 record 하나의 header에서 C'을 먼저 읽는다. 사전 scan은 native 구간의 record를 최대 5개만 걷고, 남은 바이트가 있으면 Corrupt다. Job3 효과 2(`@284`)와 `[286,288)`의 C', `JobView::new_cell`·`restore_bytes`, 효과별 Forward·CLR·cleanup 식은 `retention_page/row/job.rs` 한 곳에 있다. UPDATE 식은 DELETE 식에서 tombstone 180을 C'으로 바꾼 것이다(`fixed − 180`, 상수 단언 91124·90916·311428). codec은 Row root의 효과 ≤2, 효과2 Reserved·Retained의 C' 185..2228, 그 밖의 `[286,288)` zero를 요구한다.
- **전이(§4.1).** update Reserve는 delete Reserve의 단계를 밟는다. 대상은 `Committed`·undo None·Inline·Prune된 행이어야 하고, row_high는 올리지 않는다. Job에는 옛 C, 새 C', 계획한 새 행의 projection을 넣는다. UpdatePrune은 C가 만든다. UpdateForward·UpdateClr은 A의 group이라 C 생산자는 Unsupported로 거절하고, 관찰자만 둘을 받는다. `ReserveAdopted`는 후보 Job의 효과 byte·RowId·C'·projection을 채택하고 나머지를 다시 계산하며, C는 leaf의 옛 행에서 얻는다.
- **효과2 H(§4.4).** `check_outline`은 "효과 0이 아니면" 규칙으로 효과1·2 H를 같이 본다(진입 (b)).
- **leaf 예약(§4.3).** D는 같은 table의 Retained root마다 `restore_bytes`(효과1 `C−180`, 효과2 `max(0, C−C')`)를 더한다. update Reserve는 `entries+max(0, C'−C)+D ≤ 16188`을 요구하고, 어기면 OrderMismatch다. 커지는 UpdateForward도 같은 식을 다시 보며 어기면 OrderMismatch다. UpdateClr·UpdatePrune은 D 안에서 되돌리므로 leaf가 D를 넘어 있으면 Corrupt다. Capacity 분류는 3단계의 C preflight가 한다.
- **효과 소비자의 명시 거절(§13).** `RowEffect::Update`를 더한 뒤 효과를 읽는 곳은 모두 효과2를 따로 다룬다. bool 비교(`effect() == Delete`)가 남은 곳은 그 앞에서 효과2를 거절한다. 3단계가 여는 곳과 그 거절은 다음과 같다.
  - A 생산자 `row/insert/group.rs` `observe`: 선택한 RootJob이 효과2이면 Unsupported, H가 효과2이면 Unsupported(출력 전).
  - C `row_forward.rs`: 효과2 Reserved entry와 생산자 입력은 invariant 오류다(제품 경로가 효과2 root를 만들지 않는다). `row_clr.rs`: 효과2 root는 CLR 대상이 아니다(invalid).
  - live 정리 `row_start/retire.rs`와 복구 정리 `recovery/catalog.rs`: 효과2 Retained root의 정리 선택은 Fatal(Unsupported)이다.
  - 제품 replay `history/row.rs`: opcode 10·13·24와 효과2 RowReserve는 Unsupported다(효과 없음).
  - base4 `graph.rs` `check_queue`: 효과2 Job은 Unsupported다(actor·table·ledger 규칙 앞).
  - 시험 전용 관찰자 `semantic_row/{jobs,undo,leaf}.rs`: UPDATE opcode와 효과2 root는 Unsupported다. status.rs는 효과와 무관하다.
  - `visibility/row_retirement.rs`의 `scope()`는 3단계에서 wildcard 없는 match로 바꾼다(§8). 2단계에서는 UpdatePrune을 만드는 제품 경로가 없다.
- **census(§10).** 처음 구현은 복구 Row 갈래 S1B를 25,963 B로 키워 상한 25,743 B를 220 B 넘었다. 원인은 prepare frame에 든 계획 행(`Planned`, A 값의 `CanonicalRowView`를 복사해 가짐)과 `Shape`·`Prepared`의 usize C'이었다. 동작은 그대로 두고 배치만 바꿨다. 계획 행은 `reserve_target` 안에서 만들고 C'=계획 cell 검산도 그 안으로 옮겼다. Reserve의 대상과 효과는 `Option<(RowId, RowEffect)>` 하나로 묶었다. `Shape.new_cell`·`Prepared.new_cell`은 u16(185..=2228)이라 기존 padding에 든다. 그 결과 main 대비 늘어난 것은 prepare 지역 8 B(대상에 붙은 효과)뿐이다.
  - S1B Row 갈래: 25,731 → 25,739 B(상한 25,743, 여유 4 B). history frame 25,743 B와 live/recovered peak 64,016/65,392 B(memory), 64,024/65,400 B(native)는 그대로다.
  - N1 transition bound 30,351 → 30,359 B(prepare 17,651 → 17,659, entry·split·frame은 main과 같다).
  - RowWrite C 여유: reserve 595 → 587 B, cancel 1,539 → 1,531 B, preflight 728 B 그대로다. row_lifecycle 40,881 B와 44,304 B charge도 그대로다.
  - **07-3에 남는 위험.** S1B 여유가 4 B뿐이므로 07-3이 replay에 UPDATE 갈래를 열 때 Row 갈래 frame이 다시 넘을 수 있다. 그때는 같은 규칙으로 배치를 먼저 줄이고, 모자라면 멈추고 묻는다.
- **시험.**
  - 독립 저자 [`module-row-format/transition-update/author.py`](../../tests/fixtures/module-row-format/transition-update/author.py)는 이력 D 끝에서 시작하는 이력 U(U-546..U-579, 34 group)와 변형 13개를 만든다. 세 번째 actor T3이 update Reserve와 Cancel, 커지는 UpdateForward(h0), 효과2 H 위의 줄어드는 UpdateForward와 UpdateClr, 효과0 H 위의 같은 크기 UpdateForward, 효과2 H 위의 INSERT Forward, UpdatePrune 둘과 Prune, committed·compensated update root의 ReleaseRole·RootFree를 지난다.
  - RUP-F01..F08(`wire_tests.rs`, `job_tests.rs`, `ledger_tests.rs`): opcode·shape·frame, C' 유도와 그 거절, native 크기, 효과2 codec, 효과별 식, ledger 행.
  - RUP-C(`transition_update_tests.rs`): C가 만든 29 group은 literal과 바이트가 같고, A 몫 5 group은 관찰자가 받는다. 변형 13개(framing 1개 포함)는 모두 거절된다. borrowed 관찰자·생산자, `ReserveAdopted` 효과2, 다른 RowId, 효과2 H 증명(H page의 incoming +1을 거절), leaf 예약 경계(커지는 Reserve의 정확한 맞춤과 1 B 부족, 줄어드는 Reserve, D 안의 INSERT·커지는 update, UpdateForward 1 B 부족, D 안의 UpdateClr, 가득 찬 leaf의 UpdatePrune)도 본다.
  - N1 cross-check: UPDATE opcode group(U-558·U-560·U-562·U-567·U-569·U-571)에서는 undo·jobs·leaf 관찰자가 Unsupported를 돌려주는지 단언한다. 효과2 root나 H 곁의 다른 group은 status.rs만 대조하고 나머지는 3단계에서 켠다.
  - 거절 지점 시험: `rup_f_insert_producer_refuses_update_root`(A, 선택 root와 H), `rup_f_base4_refuses_update_root_until_stage_3`(base4), `rup_j_row_arm_refuses_update_until_stage_3`(replay: 이력 D 뒤 U-546..U-554를 그대로 replay하고 U-555와 opcode 10·13·24 frame을 효과 없이 거절). 세 가드를 지운 probe build에서는 세 시험이 모두 실패했다(A는 뒤의 H 증명이 Retention(Corrupt), base4는 Queue(Corrupt), replay는 U-555를 받아들임).
  - 기존 시험 가운데 바뀐 코드를 지나는 것: RDL-F·N1 wire·job·ledger·transition·A group·base4 graph, `catalog_memory::row_forward`·`row_recovery`, `row_join::retire`(DELETE RDL-J·R·N 포함), census(s2w_c01..c06, mbc40_v09, S1B 등).

<a id="implementation-3"></a>
## 17. 3단계(07-3) 구현 기록

3단계는 제품 경로 전체다. A의 UPDATE 진입점이 영속 update 행·update U를 쓰고, 같은 PR에서 B 읽기, live·복구 UpdatePrune, replay, base4, loser가 그것을 읽고 복구한다(§13 조건). 2단계의 효과2 명시 거절 목록(§16)은 모두 열렸고, 그 거절 지점 시험은 수용 시험으로 바뀌었다.

- **A 실행기·생산자(§6).** 진입점 `autocommit_update_live`·`transaction_update_live`는 DELETE 실행기의 단계와 slot을 그대로 쓴다. `update_row`는 preflight(FlushRequired면 flush 뒤 재시도, NoRow면 0행) 뒤 `rewrite_row`가 새 값 검사(INSERT의 `validate_insert`, 2048 B 초과는 Unsupported)와 `reserve_update`, INSERT의 Forward 생산·prepare·apply를 잇는다. 각 단계는 never-inlined 함수라 preflight와 flush의 frame에 쌓이지 않는다. A 생산자는 UpdateForward(새 canonical로 만든 새 행 cell)와 UpdateClr(U before에서 table 스키마로 만든 복원 cell)을 만들고, 효과2 H 증명은 "효과 0이 아니면" 규칙이다(§4.4). 생산자 입력 `RowProducerInput::Update{canonical_len, canonical_digest, old_cell, projection}`의 projection은 계획한 새 행의 것이다.
- **A↔C 경계(§6 표).** `RowPreflightCut`은 `target: Option<RowId>`와 `effect`를 갖고, `reserve`·`reserve_delete`·`reserve_update`는 각자의 효과만 받는다(어긋나면 Before InvalidInput). CLR 선택(`row_rollback_select.rs`)의 현재 행 길이는 효과2에서 C'이다. `ReservedEntry`는 bool 대신 효과를 갖고 Forward·CLR opcode를 효과로 고른다. C 설치 본문의 Row write 허용 목록(`row_start.rs` `row_write`)에 UpdateForward·UpdateClr을 더했다. 이 누락은 RUP-J가 처음 실제 C 위에서 UPDATE를 apply할 때 Apply 단계 `Prepare/Unsupported`로 드러났고, 메모리 전이 시험은 설치 본문을 지나지 않아 보지 못했다.
- **판정(§5).** `row_class`는 다른 거래의 Inline 행을 그 RowId를 쥔 root 하나의 효과로 가른다(`holder_effect`). 묶음 필드(Row scope, `RootRetained`, 효과 0·2, source·Status 세대, object·generation, RowId, undo) 중 하나라도 어긋나거나 짝이 0·2개면 손상이다. own Inline 행(own INSERT·own UPDATE)은 Unsupported, own tombstone은 0행이다. DELETE 판정도 같은 갈래를 쓴다(효과0 행의 결과는 그대로, §15 5).
- **B 읽기(§7).** `missing_before`는 진입 (b) outline을 §7 표대로 판정하고, 값 보고는 `MissingRowEvidence{value}`와 capture 번호(`value_capture`)로 남긴다. `value_before`는 tombstone 후보이거나 같은 capture에서 보고된 Inline 후보만 받고(그 밖 Unchanged WrongState), 옛 값을 `copy[10240..12288)`에 둔다. B driver는 `MissingObserved::value_before()`가 참이면 `select_before`로, 아니면 없음으로 간다.
- **회수·복구(§8·§9).** `scope()`는 wildcard 없는 match이고 UpdatePrune은 `ValuePrune`이다. live 정리(`row_cleanup_next`)와 복구 정리(`recovery/catalog.rs`)는 효과2 Retained head root에 UpdatePrune을 고른다. replay는 opcode 10·13·24와 효과2 RowReserve를 받는다. base4 actor는 효과2 root를 상태별로 본다: Reserved는 옛 committed 행(cell C, U Free), Retained는 own `Transaction(T)` 새 행(cell C', projection, undo), ReleasingCompensated는 U before의 logical·stamp로 복원 행, ReleasingCommitted는 `Committed(q)`로 재stamp된 own 행. graph의 D는 `max(0, C−C')`다. 역방향 scan에서 Inline 행은 효과 0·2 root, tombstone은 효과1 root와 짝을 짓는다.
- **RDL-B09(§15 6).** 효과1 Retained root 아래 Inline `Transaction(T)` 행과 delete U는 B에게 UPDATE 행과 같은 모양이라, 다른 거래의 읽기는 U의 옛 값 100을 읽는다(`delete_read.rs`의 B09 후반을 대조군으로 옮겼다). B는 Job을 읽지 않으므로 이 모양이 손상인지는 C가 본다: base4는 효과1 root가 자기 tombstone을 찾지 못해 Identity로 거절하고(RDL-R02에 추가), live preflight는 Inline 행을 쥔 root가 효과1이면 묶음 불일치로 Fatal이다(RUP-C holder mutant).
- **시험 전용 관찰자.** `semantic_row/{undo,leaf,jobs}.rs`가 UPDATE opcode와 효과2 root를 관찰한다. N1 cross-check는 history U의 UPDATE group(U-558·U-560·U-562·U-567·U-569·U-571)을 이제 leaf·jobs·undo로 대조하고, 효과2 H 변형은 거절한다(RUP-J10에 해당).
- **census(§10).** RowWrite charge 44,304 B와 row_lifecycle 40,881 B는 그대로다. UPDATE 판정은 preflight 본문의 `delete` 항(`row_target`·`target_no_row`, 12,734 B)에 같이 들고, 본문 최대 항은 finish 31,518 B다.
  - RowWrite C 여유(S2W-C01/C06, memory·native 같음): preflight 728 → 704 B, reserve 587 → 499 B(update 갈래의 계획 행과 draft), cancel 1,531 B 그대로다.
  - 복구 Row 갈래 S1B는 25,739 B 그대로다(상한 25,743 B). replay의 UPDATE 갈래와 복구 정리의 UpdatePrune 선택은 frame을 키우지 않았다. N1 bound 30,359 B, history frame 25,743 B, live/recovered peak 64,016/65,392 B(memory)·64,024/65,400 B(native), 복구 facade(S6R) 17,224 B와 rollback(S4) 수치도 그대로다.
  - A 실행기: driver census site는 39에서 54로 늘었다(`update.*` 15개). F4 고정 상한과 원 항은 바꾸지 않았다(RUP-A08, memory·native). 상한에 가장 가까운 UPDATE 항은 `update.preflight`로 6,144 B 상한에 여유가 자동커밋 671 B, 명시 거래 904 B다. 자동커밋 commit은 2,889 B(COMMIT 3,024 B)다.
  - B 읽기(S5R-C01/C02): UPDATE 행의 `value_before` 단계는 DELETE 것과 같은 단계이며 cursor load가 34,460 → 34,468 B로 34,586 B 안이다(여유 118 B).
  - base4 actor 사슬은 20,792 B로 상계 21,396 B 안이다(`base4_closure_source_bound_covers_measured_frames`). live 정리 C 본문 39,564 B와 caller 4,096 B의 합(여유 644 B)도 그대로다.
- **시험.** 시험 ID와 사례는 gate record `module-gate/records/20261104-row-update-3.json`에 있다.
  - RUP-A: A 생산자 literal 대조와 출력 전 거절(`group_update_tests.rs`, 효과2 H 포함), 효과로 group을 고르는 INSERT 생산자(`rup_a_insert_producer_binds_update_root`), D 곁에서 커지는 UpdateForward의 경계(2단계 검토 r1 #1·#2), F4 예산(`rup_a08_*`). 실행기의 1행·0행·Busy·Unsupported·SerializationConflict·statement 실패 뒤 계속은 실제 C 위 RUP-J에서 본다.
  - RUP-C(판정): `row_update_tests.rs`의 판정 표와 holder root 묶음 mutant, `row_write_tests.rs`의 Capacity(커지는 값과 D).
  - RUP-J01..J07(실제 C, `row_join/retire/update*.rs`): 자동커밋 갱신·0행·재갱신과 정리, 커지는·줄어드는 값의 rollback과 commit(NULL 5 B↔Int64 13 B), statement rollback, 한 거래의 UPDATE·INSERT·DELETE·UPDATE(효과2·효과1 H)와 own 행 거절, 정리 전 읽기와 붙잡힌 UpdatePrune, 다른 거래의 UPDATE 행에 대한 UPDATE·DELETE의 Busy(정리 단계마다)와 SerializationConflict, 긴 새 값. 각 경우 C 재시작이 같은 행을 읽는다.
  - RUP-B01..B07: B 판정 literal(`update_tests.rs`), UpdateClr의 읽기 gate 대기, 손상 U(Releasing·creator `Transaction` stamp는 Fatal, RowDeleted는 Unsupported), `value_before`의 보고 요구와 capture copy 분리, 보고 뒤 Releasing이 된 U의 Fatal.
  - RUP-R01..R07: 효과2 네 상태의 재시작(loser UpdateClr, winner UpdatePrune), base4 수용(다섯 상태와 줄어드는 Active), 거절(재stamp·짝 없는 행·C' 아닌 cell·비canonical U·넓힌 D), release cursor 3, replay(`rup_r_row_arm_replays_history_u`), base4 graph(`rup_r_base4_update_root_needs_its_old_row`).
  - 바뀐 코드를 지나는 기존 시험: INSERT·DELETE의 A group·실행기·JOIN·base·read(RDL-A·B·C·J·R), LXT, row_forward·row_write·row_recovery·row_rollback 등 Row 묶음 197개, census.

<a id="implementation-4"></a>
## 18. 4단계(07-4) 구현 기록

4단계는 native fault cut RUP-N01..N06이다. 제품 경로는 바뀌지 않는다. 시험, controller, 동결 inventory만 더했다. 방식은 DELETE 3단계([§19](dev3-row-delete.md#implementation-3))와 같다.

- **RUP-N 이력.** 사례마다 이력 하나를 실행 전에 정했다(`tests/fixtures/module-row-update/native-inventory.json`). T는 nullable Int64 열 하나다. CREATE가 q 1을 쓰고, A의 자동커밋 INSERT가 행 100·101을 q 2에 commit한 뒤 정리까지 마친다. 그 뒤 행 100에 사례의 단계를 밟고 cut에서 멈춘다. N01·N02는 100을 200으로 바꾼다(같은 cell, C'=C). N03..N06은 NULL로 바꾼다(C'=C−8). 그래서 leaf가 D=8 B를 예약하고, N06과 복구의 UpdateClr은 그 예약 안에서 행을 다시 키운다.

  | 사례 | cut | 마지막 단계 | READY q | 복구 CLR | READY 행 |
  | --- | --- | --- | --- | --- | --- |
  | N01 | UpdatePrune 앞 | 자동커밋 UPDATE와 그 ACK(q 3) | 3 | 0 | 200, 101 |
  | N02 | UpdatePrune 뒤 | N01과 live 정리의 첫 단계(UpdatePrune) | 3 | 0 | 200, 101 |
  | N03 | UpdateForward 앞 | 명시 거래(Serializable)의 preflight와 RowReserve | 2 | 0 | 100, 101 |
  | N04 | UpdateForward 뒤 | 명시 거래의 UPDATE statement, 거래는 Active | 2 | 1 | 100, 101 |
  | N05 | UpdateClr 앞 | N04 뒤 Y(다른 Status)가 102를 INSERT해 q 3에 commit, A의 Commit은 SerializationConflict로 거절, A의 full RollbackStart | 3 | 1 | 100, 101, 102 |
  | N06 | UpdateClr 뒤 | N05와 rollback의 첫 단계(UpdateClr) | 3 | 0 | 100, 101, 102 |

- **세 process와 controller.** DELETE §19와 같다. worker는 `row_join::retire::autocommit::update::native::rup_native_worker`(ignored)이고, controller `scripts/check_dev3_row_update_native.py`는 DELETE controller의 검사를 그대로 쓴다(사례 ID·inventory schema·worker 이름·환경 변수 이름만 다르다). produce의 결과·q·RowId와 WAL witness, 파일마다 마지막 write 뒤의 성공 sync와 마지막 sync 뒤 쓰기 없음, "뒤" 사례의 cut WAL이 짝 "앞" 사례의 cut WAL을 바이트 그대로 앞에 두고 그 뒤 WAL write가 그 group 하나뿐임(N01→N02 UpdatePrune, N03→N04 UpdateForward, N05→N06 UpdateClr), recover·reenter의 READY q·CLR 수·B가 읽은 값, 세 pid, 복구 process의 자원 한도다.
- **결과.** debug test ELF에서 18 process가 모두 통과했다. 복구 step은 N01 20, N02 18, N03 16, N04 21, N05 28, N06 28이다. N01에서는 복구가 UpdatePrune을 했고, N02에서는 live 정리가 이미 했다. 복구 CLR은 N03 0(Reserved root는 Cancel), N04 1, N05 1, N06 0이다. "뒤" 사례가 더한 WAL group은 UpdatePrune 89,895 B(C=C'=193, `89316+C+2C'`), UpdateForward 91,695 B(C=193, C'=185, `91124+2C+C'`), UpdateClr 91,487 B(`90916+2C+C'`)로 §3의 frame 식과 같다(h=0). 복구 process는 모두 handle 0, peak 3, fd peak 6이다. 실행 기록은 [`module-gate/evidence/row-update-4/`](../../module-gate/evidence/row-update-4/README.md)에 있다.
- **메모리 쌍둥이.** `rup_n01_n02_memory_update_prune_cuts`, `rup_n03_n04_memory_update_forward_cuts`, `rup_n05_n06_memory_update_clr_cuts`가 같은 이력을 memory directory 위에서 돈다. cut의 파일에서 C의 재시작이 base4로 잡는 Row Job 상태를 §9와 대조하므로 "앞"과 "뒤"가 CI에서도 갈린다(N01은 Retained update root와 TerminalReady, N02는 ReleasingCommitted root와 TerminalReady, N05는 Retained, N06은 ReleasingCompensated). 각 사례의 기대값이 동결 inventory에 그대로 적혀 있는지도 본다. CI가 도는 것은 이 셋이고 native worker는 ignored다.
- **한계.** DELETE §19의 한계와 같다. cut은 process 종료이고 group 경계에만 둔다. group 안의 torn write, 복구 도중의 crash, I/O fault는 일정 12·17의 몫이다. RUP-N05·N06의 둘째·셋째 Status page는 시험이 직접 초기화한다. 커지는 UPDATE의 native cut은 두지 않았다. 커지는 값의 Forward·CLR·commit과 재시작은 메모리 시험 RUP-J02가 실제 C 위에서 본다. native 결과는 이 cloud 환경의 실행 한 번이다.

## 작성 기록

- 2026-11-04 r1 초안. main `5760f30`의 source를 읽고 썼다(operation.rs, wire.rs, core.rs, job.rs, undo_record.rs, transition.rs, transition/prepare.rs, ledger.rs, row_preflight.rs의 `delete_class`, plan4_apply/row/read.rs의 `missing_before`·`value_before`, row/read/driver.rs, plan4_recovery_base/row/actor.rs, plan4_apply.rs의 `RowLoanLayout`).
- 2026-11-04 D2 1회차 **ACCEPT WITH REQUIRED CHANGES**(독립 검토). 형식·전이·산술(native 합, frame 세 식과 최대 97936/97728/96128, ledger, cleanup 최대 318240, `has_previous`), Job3 바이트 배치, U 형식, leaf 예약, RowId 유일성, 판정, 회수 scope, 단계 분리는 맞다고 확인됐다. 필수 3건은 모두 B 읽기와 Undo codec의 경계였고 r2에 반영했다: (1) `value_before`가 capture의 새 값 copy 구간을 덮어 같은 capture의 `copy_candidate`가 옛 값을 새 version 이름으로 내줄 수 있다 → §7. (2) [Undo 형식](dev3-undo-format.md) 개정이 닫은 (b) 사용처 목록에 B `missing_before`가 없다 → 개정 문단. (3) `missing_before`의 (b) outline 판정이 "지금과 같다"로만 적혀 Releasing·`Transaction` stamp 등의 결과가 없다 → §7 표. 권고 6건: 효과2를 INSERT 쪽으로 보내는 bool 비교와 2단계의 명시 거절(§13), `scope()`의 wildcard(§8), kind1 사전 scan 경계(§3.4), holder root 묶음 필드(§5), A↔C 경계의 cell 길이 하나 가정(§6·§11), C' 사용처(§3.3). 사소 6건(§2 B 행, §5 Fatal 행, 오류 순서의 L'>2048, §9 Releasing의 C·C', §8 읽기 쪽 근거, 한 번 읽기 근거는 추정)도 반영했다.
- 2026-11-04 D2 2회차 **ACCEPT**(같은 검토자). r2가 1회차 15건을 모두 닫았고 새 필수·권고는 없음을 확인했다. 사소 6건을 r3에 반영했다: §7 표의 거절 층(codec·read.rs) 구분, before 값을 겹치지 않는 copy 구간 `[10240,12288)`에 두어 r2의 상태 규칙을 대체(copy 버퍼가 이미 `PAGE_BYTES`라 바이트 증가 0), §6 표의 DELETE Forward·Update 필드 출처와 `ReservedEntry.delete`, §9 Releasing의 길이 검산은 canonical decode뿐, §10 읽기 census 문장.
- 2026-11-04 §16(2단계 구현 기록) 추가. C 형식·전이층, 효과2 소비자의 명시 거절 목록, census(S1B 상한 초과를 배치 변경으로 해소, 여유 4 B), 독립 저자 이력 U와 RUP-F·RUP-C·거절 지점 시험을 적었다. 07-2가 고칠 설명(§3.5)은 소비자 계약의 shape 표·cleanup 최대·scope3 wire 기록·`candidate_template` 상계, 자원 정본, 모듈 경계의 XOI 배치, `RowLoanLayout::FRAME` 주석에 반영했다.
- 2026-11-04 §17(3단계 구현 기록) 추가. A 실행기·생산자, A↔C 경계(설치 본문 Row write 허용 목록 누락을 RUP-J가 찾음), holder root 판정, B `value_before`, 회수·replay·복구·base4, RDL-B09 대조군 이동, 시험 전용 관찰자, census(C 여유 preflight 704 B·reserve 499 B, S1B 그대로), RUP-A·C·J·B·R 시험을 적었다.
- 2026-11-04 §18(4단계 구현 기록) 추가. native fault cut RUP-N01..N06(UpdatePrune·UpdateForward·UpdateClr 앞뒤, 18 process 통과), 메모리 쌍둥이, 동결 inventory와 controller, 한계를 적었다.

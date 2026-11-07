# DEV3 큰 행 overflow chain — 쓰기·읽기·반환 설계

2026-11-07 · `D3-ROW-OVERFLOW-001` · **설계(O0) / 독립 D2 대기 / 제품 구현·시험 NOT_RUN**

[overflow 표현](dev3-overflow-format.md) · [행 표현](dev3-row-format.md) · [행 owner R6·§5.4](dev3-task-06-tree-rows.md#algorithms) · [첫 Row profile](dev3-module-consumers.md#profile) · [WAL shape](dev3-module-consumers.md#row-shape) · [scope3 wire](dev3-module-consumers.md#row-scope3-wire) · [unlink retired 목록](dev3-tree-smo.md#empty-leaf-unlink) · [자원 정본](dev3-resource-bounds.md) · [목표 등급 SCOPE-002](../plans/project-direction.md#scope-002) · [일정 08-3](../plans/dev3-schedule-baseline.md#items)

일정 08-3 "overflow chain 쓰기·읽기·반환"(2일, 1단위)의 설계다. 지금 Row 경로는 canonical 2,048 B를 넘는 행을 첫 효과 전에 `Unsupported`로 거절한다(A `encode_input`, C RowReserve의 `MIN_CANONICAL..=INLINE_ROW_BYTES`). [overflow 표현](dev3-overflow-format.md)의 head·chunk codec과 streaming 검산기는 있으나 실제 page 할당·WAL·소유·회수가 없다. 이 문서는 코드보다 먼저 그 형식·전이·비용·복구·시험·단계를 정한다. 제품 PASS가 아니다.

<a id="scope"></a>
## 1. 범위

**08-3은 2,049..131,072 B canonical 행의 INSERT가 chain을 쓰고, B가 그 행을 읽고, 그 INSERT가 rollback되면 chain을 반환하는 경로다.**

| 포함 | 제외(첫 효과 전 거절하거나 다루지 않음) |
| --- | --- |
| 자동커밋·명시 거래의 INSERT, canonical 2,049..131,072 B(각 값 ≤ 64 KiB는 기존 codec 검사) | DELETE·UPDATE의 대상이 overflow 행(지금처럼 `TargetClass::Unsupported`) → 08-4 "큰 행 복원" |
| chunk 쓰기 그룹, Forward의 chain 검산과 overflow head 행 | UPDATE의 새 값이 2,048 B 초과(지금처럼 Unsupported) → 08-4 |
| B point·cursor 읽기(own 미확정 행과 committed 행)의 materialization | index가 있는 table(기존 profile 제한), 128 KiB 초과 행 |
| statement·full rollback(CLR)과 복구 loser의 chain 반환, chain 쓰기 도중 실패의 반환 | 반환한 page의 allocator Free·재사용(3.14.1→3.03.4→3.14.2, 일정 14-2) |
| commit 뒤 Prune(chain은 행과 함께 남음), replay·Base4·native fault cut | streaming 읽기 API, 여러 extent(12-1) |

"반환"의 뜻은 [행 owner](dev3-task-06-tree-rows.md#algorithms)의 회수 행 "unlink·overflow 교체 뒤 회수 … 3.14.1→3.03.4→3.14.2. unlink만으로 Free 선언 금지"를 따른다. 08-3의 반환은 chain을 쥔 owner(Job)에서 소유를 내려 그 table의 **retired 목록**(anchor XTR1, [unlink](dev3-tree-smo.md#empty-leaf-unlink)과 같은 목록)으로 옮기는 logged 전이다. page는 `Allocated(g, table, Overflow)`로 남고 실제 Free·재사용은 14-2가 retired 목록에서 한다. 이것은 08-1의 unlink가 퇴역 tree page를 다룬 방식과 같다.

DELETE·UPDATE의 overflow를 08-4로 두는 이유: 그 경우 U의 before가 2,048 B를 넘어 Undo chunk 또는 옛 chain 참조가 필요하고, commit 뒤 옛 chain 반환에는 읽는 중인 reader의 transient pin 규칙이 필요하다. 이것이 일정 08-4의 "Undo chain·큰 행 복원"이다. 08-3의 반환은 다른 reader가 볼 수 없는 chain(아직 Forward 전이거나, 미확정 INSERT가 CLR로 지워진 행의 chain)에서만 일어나므로 reader 경합이 없다(§7.4).

<a id="assumptions"></a>
## 2. 지금의 inline 전용 가정

| 영역 | 지금 | 08-3이 바꾸는 것 |
| --- | --- | --- |
| A 값 검사 | `encode_input`: L>2048이면 Unsupported, leaf 여유 `L+168+16` 검사, 16 KiB CANDIDATE에 canonical 전체를 씀 | L ≤ 131,072이면 overflow 계획(§6.1). leaf 검사는 head 64 B 기준, CANDIDATE는 chunk 한 개 창 |
| C Reserve | RowReserve producer가 L을 `5..=2048`로 묶고, Job `C`(185..2228)와 XRP1 projection으로 값 commitment | overflow root(§4.3)는 C=244와 root를 비운 XRP1(§4.2) |
| 쓰기 그룹 | Row 그룹은 page를 할당·INIT하지 않음(Undo container는 System INIT이 미리 만든다) | ChunkWrite가 allocator Free page 하나를 Allocated(table, Overflow)로 바꾸고 chunk page를 INIT(§4.5) |
| leaf 행 | Inline payload만 쓰고 읽음 | Overflow payload(head 64 B) 행: record 232, cell **C=244**(12+168+64) |
| B 읽기 | `copy_candidate`가 leaf의 inline canonical을 복사 | Overflow 행이면 chain을 읽어 caller output에 materialize(§7.3) |
| 반환 | 없음. unlink의 XTR1은 RowLeaf·RowInternal만 받음 | XTR1이 Overflow level0 항목을 받음(§4.4). OverflowCancel·ChainRetire(§4.5) |
| Base4 | 현재 tree 전체와 retired 목록·allocator 역소유를 검산 | chunk page의 소유자(행 head, Reserved root, 보상 root, XTR1)를 정확히 한 번 센다(§8.3) |
| chunk 형식 | 모든 chunk가 root PageRef를 가짐(@24..48) | 뒤에서 앞으로 쓰므로 root를 모르는 chunk가 먼저 생긴다. 그 칸을 zero로 바꾼다(§4.1) |

<a id="write-order"></a>
## 3. chain을 쓰는 방식

128 KiB 행은 chunk 9개다. 지금 Row 그룹의 frame 상한은 98,304 B([RowLoanLayout](../../crates/engine/src/storage/wal_io/plan4_apply.rs)의 FRAME)이고 private loan은 425,984 B로 caller가 준다. 신규 page 하나의 INIT redo는 16,448 B다. 그래서 한 그룹에 chain 전체를 넣을 수 없고, 그룹마다 chunk 하나를 쓴다. 남은 선택은 다음 chunk의 주소를 어떻게 알게 하느냐다.

| 안 | 내용 | 판단 |
| --- | --- | --- |
| A1 한 그룹 | chain 전체를 Forward 또는 별도 그룹 하나에 | frame ≈ 9×16,448 + allocator·Job·Root ≈ 200 KB, after image 11장. loan·frame 상한과 replay 입력을 모두 키운다. 거절 |
| A2 앞에서부터 + allocator 예약 | Reserve가 k page를 `Reserved(g, Transaction T)`로 잡고 chunk 0부터 씀 | 앞 chunk가 다음 주소를 안다. 대신 Row 그룹에 allocator Reserved·Transaction claimant·미사용 예약 취소가 새로 들어오고, Reserved가 있는 동안의 다른 할당(SystemInit의 linear 예약 규칙, scope4 거절)과 얽힌다 |
| A3 table별 Free overflow container pool | System INIT이 빈 kind7 page를 미리 만들고 chunk 쓰기는 patch만 | 실제 재사용이 된다. 대신 새 system object·SystemInit role·base record·사전 provisioning이 필요하고, [설계 정본](disk-storage-design.md#tree)의 "이전 chain은 개별 allocation으로 보존하고 이후 allocator에 반환"과 다르다 |
| **A4 뒤에서부터(선택)** | chunk k−1부터 0까지. 각 그룹이 가장 낮은 eligible Free page를 할당·INIT하고 next = 앞서 쓴 chunk | 다음 주소가 언제나 이미 쓴 page라 예약이 없다. 중간 상태는 Job의 pending 칸(앞 PageRef·쓴 수) 하나로 표현된다. 대가: chunk가 root(=chunk 0)를 모른 채 써지므로 chunk 형식의 root 칸을 비우고(§4.1), payload 검산을 Forward의 전체 chain 검산으로 미룬다(§6.3) |

A4를 고른다. A2의 장점(쓰는 도중 streaming 검산)은 Forward에서 chain을 앞에서부터 한 번 읽어 얻을 수 있다. 그 chunk들은 방금 이 거래가 쓴 page이고 Job만 소유하므로, Forward까지 아무도 바꿀 수 없다.

<a id="format"></a>
## 4. 형식

`k = ceil(total/16,092)`(1..9), `W`는 ChunkWrite frame bytes, `h`는 previous transaction H 존재(0/1)다.

<a id="chunk-revision"></a>
### 4.1 chunk 형식 개정

[overflow 표현](dev3-overflow-format.md)의 chunk cell `@24..48`(root PageRef)을 **reserved zero**로 바꾼다. head 64 B는 그대로다. 이 codec은 아직 어떤 DB에도 쓰이지 않았으므로 version은 1 그대로 두고, 개발 형식 규칙([버전 정책](../policies/version-compatibility-policy.json))에 따라 개정 전 bytes는 Corrupt로 거절한다.

- cell decoder는 `@24..48`이 zero가 아니면 Corrupt다. "index0의 현재 PageRef는 root와 같다"는 검사는 그대로 남는다: `OverflowChainVerifier`가 받은 expected root(= head.root)가 chunk 0의 page identity다.
- "뒤 chunk는 root와 같은 물리 주소를 가질 수 없다"와 "next가 현재 또는 root와 같은 물리 주소이면 거절"은 verifier의 방문 주소 집합(최대 9개, root 포함)으로 옮긴다. 국소 검사는 next ≠ 현재 page 주소만 남는다.
- 다른 chain의 chunk가 끼어드는 경우: chunk는 같은 count·total·digest·index를 가져야 하므로 끼어든 chunk는 전체 SHA-256이 같은 행, 곧 같은 bytes다. 같은 page를 두 chain이 쥐는 손상은 Base4의 정확히 한 번 소유 검산(§8.3)이 잡는다.
- 고정 시험 8개의 literal 중 page literal·full-page SHA·손상 36종의 root 항목이 바뀐다. 새 기대값은 O1에서 독립 Python `struct/hashlib`로 계약에서 다시 계산하고 원 artifact 옆에 보존한다. 제품 encoder로 기대값을 만들지 않는다.

<a id="overflow-row"></a>
### 4.2 overflow 행과 값 commitment

leaf의 행은 기존 [행 표현](dev3-row-format.md)의 Overflow payload다: record = header 168 + head 64 = 232 B, tree cell **C = 244**. stamp·logical·undo는 INSERT 행과 같다(`Transaction(T)`, `(T.id, mutation)`, undo = U, U는 Missing before).

값 commitment([scope3 wire Q1](dev3-module-consumers.md#row-scope3-wire))는 Reserve의 Job `C`와 XRP1 projection이다. overflow 행의 head.root는 마지막 ChunkWrite가 정하므로 Reserve가 알 수 없다. 그래서 **overflow root의 projection은 record의 head.root 24 B를 zero로 둔 XRP1**이다(head의 total·count·digest는 포함). Forward·CLR·Prune은 현재 record의 root를 zero로 만든 사본으로 같은 XRP1을 계산해 대조한다. root 자체는 Forward에서 Job의 pending front와 같아야 하며(§5), 그 뒤에는 leaf 행의 bytes가 운반한다.

<a id="job-overflow"></a>
### 4.3 Job3 overflow root

| 필드 | 값 |
| --- | --- |
| `@285` payload | **0 inline, 1 overflow.** 1은 효과 0(INSERT)인 Row root에서만. 그 밖의 Job은 지금처럼 0 |
| `@280` cell_bytes | overflow root는 정확히 244 |
| Reserved `@328..360` | 지금은 zero(sender digest는 Forward가 씀). overflow root는 **pending chain**: `[328,352)` front PageRef(아직 쓴 chunk가 없으면 zero), `[352,356)` total u32(2,049..131,072), `[356,358)` written u16(0..k), `[358,360)` zero. front zero ⇔ written 0 |
| Retained `@328..360` | 지금처럼 sender digest. pending은 Forward가 소비한다(written = k이어야 Forward 가능) |
| ReleasingCompensated `@200..248` | 지금은 zero. CLR한 overflow root는 **반환할 chain**: `[200,224)` head.root, `[224,228)` total u32, `[228,248)` zero. ChainRetire가 다시 zero로 만든다 |
| ReleasingCommitted | 바뀌지 않음(chain은 Prune된 행이 쥔다) |

`JobView`에 `payload()`, `pending()`(front·total·written), `retire_chain()`(root·total)을 더한다. Job codec은 위 조합만 받고 나머지를 Corrupt로 거절한다. 옛 binary는 `@285=1`을 Corrupt로 거절한다(개발 형식 규칙).

비용 recipe(§6.2)도 Job에서 나온다: `forward_wal()`은 overflow Reserved root에서 남은 chunk 수 `k−written`을 더하고, `cleanup()`은 ReleasingCompensated에 chain이 남아 있으면 ChainRetire 한 걸음을 더한다.

<a id="retired-overflow"></a>
### 4.4 retired 목록의 Overflow 항목

[XTR1](dev3-tree-smo.md#empty-leaf-unlink) 항목의 kind/level 허용에 **(Overflow 7, 0)**을 더한다. 항목 형식(40 B)과 순서·주소 규칙(첫 file, page 9..63, 엄격 증가, anchor·root·다른 항목과 다른 물리 주소)은 그대로다. n ≤ 55는 첫 extent의 dynamic page 수이므로 목록은 넘치지 않는다.

retire LSN 규칙은 kind별이다. tree page 항목은 지금처럼 Structural owner의 id(≤ Structural high-water)다. Overflow 항목의 retire LSN은 그 항목을 넣은 OverflowCancel·ChainRetire 그룹의 complete LSN이다. Structural owner가 아니므로 Structural high-water와 비교하지 않고, `0 < retire LSN ≤ anchor pageLSN`과 `chunk pageLSN < retire LSN`만 요구한다. 이 LSN도 snapshot q와 비교하지 않는다.

Overflow 항목을 넣어도 anchor root revision은 바뀌지 않는다. revision은 tree 구조(route)의 변화를 뜻하며 cursor·locator가 그것으로 재탐색을 판정한다([cursor C4](dev3-cursor-locator.md#cursor)). chain 반환은 route를 바꾸지 않는다.

<a id="opcodes"></a>
### 4.5 opcode와 shape

| opcode | outer | 바뀌는 page·native | E/K/I, redo | frame bytes(최대) |
| --- | --- | --- | --- | ---: |
| `ChunkWrite = 14` | XHG Update, handoff | Root 1152 · Job 2176 · allocator 항목 320 · chunk 256 | 0/0/1, Root P·Job P·allocator 항목 patch 160·chunk INIT 16,448 | **57,184** |
| `OverflowCancel = 15` | XHG Update, handoff | Root 1152 · Job 2176 · anchor XTR1 `128+b+a` | 0/0/1, Root P·Job P·anchor P | `56,568+b+a` ≤ **60,864** |
| `ChainRetire = 25` | XSG System, single Aging | Root 1152 · Job 2176 · anchor XTR1 `128+b+a` | 0/0/0, Root P·Job P·anchor P | `54,664+b+a` ≤ **59,056** |

P = 16,344(whole-body Patch). b·a는 anchor XTR1 cell의 before·after 길이(목록이 없으면 0, 있으면 `16+40n`)이고 최대는 b = 2,176, a = 2,216이다. 세 opcode 모두 98,304 B frame 안이다.

- **native.** 새 native kind 셋을 더한다(번호는 기존 1·3·5·8·9와 겹치지 않게 O1에서 고정한다). allocator 항목 native는 바뀐 entry 하나의 before·after 96 B씩(offset이 entry 위치), chunk native는 presence 2(after만)이고 after body는 chunk cell의 128 B header다. payload는 INIT redo에만 있고 native에 다시 싣지 않는다([scope4 native](dev3-tree-smo.md#제안-f01-scope4-frame)가 before/after page를 중복 저장하지 않는 것과 같은 이유). anchor native는 XTR1 cell의 before·after다.
- **redo.** allocator는 바뀐 entry 96 B만 Patch한다(Status224 구간 patch와 같은 방식, `64+96=160`). 신규 chunk page는 full INIT이다. 나머지는 지금 Row 그룹의 whole-body Patch다.
- **core·ref.** 세 opcode의 core는 Reserve·Cancel·ReleaseRole과 같은 ref 규칙을 따른다(current 필수, next·target 없음). ChunkWrite·OverflowCancel은 Reserved root의 statement/mutation을, ChainRetire는 Releasing root의 work cursor를 쓴다.
- **wire decode.** 세 opcode의 shape는 opcode와 b·a로 정해진다. ChunkWrite는 고정이고, 나머지 둘은 anchor native header의 before·after 길이로 b·a를 읽은 뒤 전체 크기를 검산한다(UPDATE의 C' 사전 scan과 같은 bounded 규칙). 옛 binary는 14·15·25를 Unsupported로 거절한다.

`Operation::role()`은 셋을 자기 자신으로 둔다. Forward·CLR·Prune·Cancel의 shape는 바뀌지 않는다(overflow root에서 C=244일 뿐이다).

<a id="transitions"></a>
## 5. 전이

| 그룹 | Job | allocator | chunk page | leaf | anchor | Status·U |
| --- | --- | --- | --- | --- | --- | --- |
| Reserve(overflow) | Free→RootReserved, `@285=1`, C=244, root를 비운 XRP1, pending(front 0, total, written 0) | — | — | — | — | U는 지금처럼 claim만 |
| ChunkWrite(i) | pending front := 새 page, written+1. i = k−1−written(쓰기 전) | 가장 낮은 eligible Free(g) → Allocated(g+1, table, Overflow) | INIT: chunk i, next = 이전 front(i=k−1이면 zero), payload = 창 i | — | — | — |
| Forward(overflow) | Reserved→Retained, pending → sender digest | — | (검산만, §6.3) | Missing → overflow 행(C=244), head = (front, total, digest) | — | 지금 INSERT Forward와 같음 |
| OverflowCancel | Reserved(written ≥ 1)→Free(같은 generation) | — | (읽기만) | — | XTR1 + written개 항목 | U Free generation 유지(Cancel과 같음) |
| CLR(overflow) | Retained→ReleasingCompensated, `[200,228)` := (head.root, total) | — | — | overflow 행 → Missing | — | 지금 INSERT CLR와 같음 |
| ChainRetire | `[200,228)` → zero. release cursor는 그대로 0 | — | (읽기만) | — | XTR1 + k개 항목 | — |
| Prune(overflow) | Retained→ReleasingCommitted | — | — | 재stamp(head 그대로) | — | 지금 INSERT Prune과 같음 |

- **ChunkWrite의 검사.** Reserved overflow root이고 written < k, 새 chunk의 index·count·total·payload 길이(`min(16,092, total−i·16,092)`)가 pending과 맞고, next가 pending front와 같다. digest는 head에 들어갈 값(A 계획의 digest)이며 Forward가 전체를 검산한다. 할당할 page는 split의 신규 page와 같은 규칙으로 고른다: 첫 extent에서 가장 낮은 ordinal의 eligible Free, `next_reservation_generation`이 MAX가 아님, 원 pool의 next generation·claim·DWB idle 검사(§7.2). 없으면 첫 효과 전 Unchanged Capacity다.
- **Forward의 검사.** 지금의 INSERT Forward 검사(원 schema·version·`record.len+12 == C`·XRP1)에 더해: written = k, record의 head.root = pending front, head.total = pending total, 그리고 §6.3의 chain 검산이 성공해야 한다.
- **OverflowCancel**은 written ≥ 1인 Reserved overflow root를 지우는 Cancel이다. written = 0이면 지금의 Cancel을 쓴다(OverflowCancel은 Unchanged InvalidInput). front에서 next를 따라 written개 chunk를 읽어 그 page를 XTR1에 넣는다. 마지막 chunk(index k−1)의 next는 zero여야 하고 그 수가 written과 같아야 한다.
- **ChainRetire**는 `[200,228)`이 있는 ReleasingCompensated root의 첫 정리 걸음이다(cursor 0, 다른 정리보다 먼저). root에서 next를 따라 k개 chunk를 읽어 XTR1에 넣는다. chain이 남아 있는 동안 ReleaseRole은 Unchanged InvalidInput이다.
- 반환 두 그룹이 읽는 chunk는 그 그룹의 변경 page가 아니다. 이 chain은 그 root만 쥐고 있고 단일 MutationPort 아래서 바뀌지 않으므로, producer가 읽은 next와 index를 Job·XTR1의 after가 운반하고 replay가 같은 page image로 다시 확인한다(§8.1). 각 chunk는 index·count·total·digest가 첫 chunk와 같아야 하고, 방문 주소가 겹치지 않아야 하며, page identity(DB·owner = table·generation·kind 7·level 0)가 맞아야 한다. 하나라도 어긋나면 손상이며 chain을 일부만 반환하지 않는다.

<a id="costs"></a>
## 6. 비용과 A/C 계획

<a id="plan"></a>
### 6.1 A의 계획과 chunk 생산

A의 값 검사는 canonical 길이 L을 센다. L ≤ 2,048이면 지금과 같다. 2,049 ≤ L ≤ 131,072이면 overflow 계획이다.

- leaf 공간은 head 행 기준이다: `64 + 168 + 16 ≤ leaf 여유`. 모자라면 지금처럼 Capacity이고 A의 자동 split 호출자가 받는다. 이때 `split_row_live`에 넘기는 길이는 canonical L이 아니라 64(head 길이)다.
- digest: A는 값에서 canonical bytes를 차례로 만들어 SHA-256만 갱신하는 streaming 함수(`canonical_row_digest_borrowed`)로 (L, digest)를 얻는다. 128 KiB 사본을 만들지 않는다.
- chunk 생산: chunk i의 payload는 canonical bytes의 `[i·16,092, min(L, (i+1)·16,092))`다. A는 값에서 그 창만 쓰는 windowed encoder(`encode_canonical_window_borrowed`)로 CANDIDATE(16 KiB)에 쓴다. 두 함수는 기존 `encode_canonical_row_borrowed`와 같은 검산·오류 순서를 따르고, 같은 입력에서 전체 encoder의 bytes와 정확히 같아야 한다(O1 시험).
- 그래서 A·C 어느 쪽도 128 KiB buffer를 새로 갖지 않는다. 읽기의 materialization은 caller output이다(§7.3).

<a id="ledger"></a>
### 6.2 ledger와 Job 비용

W = 57,184(ChunkWrite), F = Forward(C=244) = `91,548+128h`, R = 55,528(ReleaseRole·RootFree 한 걸음), X = 59,056(ChainRetire 최대).

| 항 | inline INSERT | overflow INSERT |
| --- | --- | --- |
| Reserve | 57,336 | 57,336 |
| Tx row1(Reserved root의 남은 Forward) | `91,304+C+128h` | `(k−written)·W + F` |
| Tx row0(root CLR) | `91,096+C+128h` | `91,340+128h` |
| Job cleanup(Reserved·Retained) | `311,608+2C+128h` | `312,096+128h`(= 같은 식, C=244) |
| CLR 뒤 남는 cleanup | `4R = 222,112` | `4R + X = 281,168` |
| Prune 뒤 남는 cleanup | `4R` | `4R` |

- Reserve의 admission은 이 표의 Tx row1을 처음부터 k개 chunk로 계산한다. ChunkWrite마다 W를 소비한다.
- OverflowCancel(≤ 60,864)은 Cancel처럼 Reserved root의 남은 Forward 몫에서 낸다. 그 몫은 언제나 F ≥ 91,548을 포함하므로 모자라지 않는다. 쓴 chunk의 W는 반환하지 않는다(이미 소비한 WAL).
- Job cleanup은 Prune 경로(`89,984+128h + 4R`)와 CLR 경로(`X + 4R`) 중 큰 것을 예약한다. Prune 경로가 크므로 inline INSERT 식에 C=244를 넣은 값과 같다. CLR은 미사용 Prune 몫 `30,928+128h`를 반환하고 `4R+X`를 남긴다. ChainRetire는 실제 frame(b·a)만 소비하고 X와의 차액을 반환한다.
- 최대 행(k=9)의 정상 commit 경로 WAL은 Reserve 57,336 + 9W 514,656 + F 91,676 + Prune 이후 정리 ≈ 0.98 MB다. chunk 그룹마다 Root·Job whole-body Patch 32,688 B가 고정 부담이다(§9).

<a id="forward-check"></a>
### 6.3 Forward의 chain 검산

Forward의 producer와 C 검사는 효과 전에 chain을 앞에서부터 한 번 읽는다: `OverflowChainVerifier`(expected root = pending front, head = Job의 total과 A 계획의 digest, owner = table)에 chunk를 차례로 push하고, 같은 payload를 **streaming canonical row 검산기**(새 `CanonicalRowStream`: column 수·tag·길이·Bool·UTF-8을 chunk 경계를 넘어 검사하고 supplied schema의 type/nullability와 각 값 64 KiB를 확인)에 넣는다. 둘 다 끝까지 성공해야 Forward가 열린다. 이것이 inline의 "원 schema decode"를 대신한다. 실패는 첫 효과 전 Unchanged이고 A는 OverflowCancel→statement 실패로 끝낸다.

읽는 chunk page는 Forward의 write target set T(최대 9) 밖이다. LOAD는 기존 retention LOAD 경로를 쓰고 거주에 flush가 필요하면 `FlushRequired`로 돌아간다. 이 page들이 Forward까지 바뀌지 않는 이유는 §5 끝 문단과 같다.

<a id="paths"></a>
## 7. A/B/C 경로

<a id="executor"></a>
### 7.1 A 실행기

자동커밋(`autocommit_insert_live`)과 명시 거래(`RowTransaction`)의 INSERT 한 행은 다음 순서다.

1. preflight → A 값 검사(§6.1) → `reserve(plan)`. overflow 계획이면 Reserve가 overflow root를 만든다.
2. i = k−1 … 0마다 `producer_lane` → A가 창 i를 생산 → `prepare_chunk` → `apply`(ChunkWrite). `FlushRequired`는 flush 뒤 같은 chunk를 다시 부른다.
3. Forward(§6.3 포함) → statement 끝·commit은 지금과 같다.
4. 2나 3이 첫 효과 전 Unchanged로 끝나면(Capacity·검산 실패·취소) written ≥ 1이면 OverflowCancel, 아니면 Cancel로 Reserved root를 지운 뒤 지금의 statement 실패 경로를 탄다. stage 뒤 실패는 지금처럼 Fatal·fence다.

A의 새 frame(chunk 반복, 계획 값)은 [F4 caller 상한](../evidence/module-independence-survey-20261026.md#f4-caller-budgets)(BEGIN 2,816·STATEMENT_START 3,584·COMMIT 3,024·ROLLBACK_CALLER 1,536·LIVE_ROW_CLEANUP 4,096 등)을 지켜야 한다. 지금 live 정리의 A drain은 4,082/4,096 B라 여유가 14 B다. ChainRetire를 drain에 넣는 일은 never-inline 하위 단계로 나누는 것을 전제로 O3에서 census로 확인하고, 상한을 넘으면 그 근거와 새 상한을 O3의 D2에 올린다.

<a id="c-entries"></a>
### 7.2 C 진입

- `preflight_row`: overflow 계획의 길이·index 0·leaf 공간(head 기준)을 검사하고, 첫 extent에 eligible Free page가 k개 있는지 미리 본다(advisory, 실제 판정은 각 ChunkWrite).
- `ReservedRow::prepare_chunk`/`apply`: ChunkWrite의 효과. 신규 page 등록은 split의 신규 page 경로(원 pool의 next generation·claim·등록, mutable flush lease와 DWB idle 검사)를 Row 단계에서 쓰도록 일반화한다. 별도 I/O owner나 새 pool을 만들지 않는다.
- `ReservedRow::cancel`: written에 따라 Cancel 또는 OverflowCancel.
- live 정리(`row_cleanup_next`/`retire_row`)와 복구 cleanup은 chain이 남은 ReleasingCompensated root에서 ChainRetire를 먼저 고른다.
- 지금 "Inline `Transaction(T)` 행이면 INSERT"로 가정하는 곳(DELETE·UPDATE의 `target_class`, B의 `missing_before`, rollback 선택, Prune·CLR producer, base4 행 규칙)은 Overflow payload의 INSERT 행도 같은 INSERT로 다룬다. DELETE·UPDATE의 대상이면 지금처럼 Unsupported다.

<a id="read"></a>
### 7.3 B 읽기

B의 판정(stamp·Status·snapshot)은 inline 행과 같다. 보이는 Overflow 행의 복사만 다르다.

- output이 total보다 작으면 chunk를 읽기 전에 `OutputTooSmall { required: total }`이다.
- C는 leaf latch를 놓은 뒤 head.root부터 chunk를 차례로 LOAD해 `OverflowChainVerifier`로 검산하며 `output[i·16,092..]`에 복사하고, 끝나면 `decode_materialized_row(head, output[..total], schema)`로 total·digest·schema를 확인한다. LOAD에 flush가 필요하면 지금의 `Step` 흐름으로 돌아가고 B가 같은 행을 다시 복사한다. 검산 실패는 손상이며 Missing으로 낮추지 않는다.
- 08-3에서 reader가 읽는 chain은 반환되지 않는다(§7.4). 그래서 chunk를 읽는 동안 별도 transient pin을 두지 않는다. 08-4에서 commit된 chain을 반환할 때 그 pin 규칙을 정한다.
- point 읽기의 locator([08-2](dev3-cursor-locator.md))는 leaf를 찾을 뿐이므로 바뀌지 않는다.

<a id="no-reader-race"></a>
### 7.4 반환과 reader가 겹치지 않는 이유

반환은 두 경우뿐이다. (1) OverflowCancel의 chain은 어느 leaf 행도 가리키지 않는다. (2) ChainRetire의 chain은 CLR로 지워진 미확정 INSERT 행의 것이었다. 다른 거래는 그 행을 Missing으로 판정하므로 chain을 읽지 않는다. 같은 거래의 읽기는 같은 session의 호출이고, 자기 변경 뒤 cursor는 `CursorInvalidated`이며, CLR·정리 그룹과 동시에 진행하지 않는다. 그래서 08-3의 반환은 읽는 중인 chain을 건드리지 않는다.

<a id="recovery"></a>
## 8. 복구

<a id="replay"></a>
### 8.1 replay와 live 검증

`history::validate_live`와 복구 replay는 세 opcode를 지금 Row 그룹처럼 다시 계산한다. ChunkWrite는 allocator entry·chunk INIT image·Job pending을 대조하고, Forward(overflow)는 replay arena(첫 extent 64 page)의 chunk image로 §6.3의 검산을 다시 한다. OverflowCancel·ChainRetire는 arena의 chunk image로 next·index를 다시 따라가 XTR1 after와 같은지 본다.

<a id="loser"></a>
### 8.2 loser·cleanup

- Reserved overflow root를 가진 loser: written ≥ 1이면 OverflowCancel, 아니면 지금의 CancelReserved.
- Retained overflow root를 가진 loser: 지금의 loser CLR. 그 CLR이 Job에 반환할 chain을 남긴다.
- 복구 cleanup은 chain이 남은 ReleasingCompensated root에서 ChainRetire부터 한다. winner의 Prune은 지금과 같다.

<a id="base4"></a>
### 8.3 Base4

현재 Row tree 검산([current-tree-closure](dev3-tree-smo.md#current-tree-closure))에 다음을 더한다. table의 `Allocated(g, table, Overflow)` page는 정확히 하나의 소유자에게 속해야 한다.

1. 보이는 leaf의 Overflow 행: head.root에서 k개 chunk(verifier 전체 검산, digest 포함).
2. Reserved overflow root의 pending: front에서 written개, 마지막의 next는 zero.
3. ReleasingCompensated root의 반환할 chain: root에서 k개.
4. XTR1의 Overflow 항목.

어느 소유자에도 없거나 둘 이상에 있는 page, 소유자가 가리키는데 kind·owner·generation·incarnation이 다른 page는 손상이다. Base4 record·certificate 형식은 바꾸지 않는다(chunk page는 기존 certificate의 kind 7 page다).

<a id="scope-002"></a>
## 9. 목표 등급(SCOPE-002)·A2와의 관계

| 항목 | 이 설계 |
| --- | --- |
| 주소 폭 | head·next·XTR1이 전체 PageRef(FileId·page_no·incarnation 각 u64)를 쓴다. 64 TiB 이상 주소를 막지 않는다. 첫 extent 제한은 profile 한도(12-1) |
| 값 크기 | head는 total u32·count u16이라 형식은 약 1 GiB(65,535 × 16,092 B)까지 표현한다. 계약 상한 128 KiB는 그대로다. 쓰기는 chunk마다 한 그룹, 반환은 cursor(root·total)로 chain을 걸어가며, A·C 어디에도 행 크기의 buffer가 없다. 상한을 올릴 때 바뀌는 것은 반환 한 그룹이 읽는 chunk 수(batch로 나누면 된다)와 읽기 API(streaming)다 |
| 거래 크기 | 거래당 상태는 Job 하나의 고정 칸이다. 행 수·chain 수에 비례하는 메모리 구조를 만들지 않는다 |
| 비용 구조 | chunk의 payload는 INIT redo에 한 번만 실린다(native에는 header만). 그룹마다 Root·Job whole-body Patch 32,688 B가 붙는 것은 지금 Row 그룹 공통의 whole-page WAL 문제이고 A2(redo 형식)가 다룬다. A2가 Root·Job을 바뀐 bytes만 쓰게 바꾸면 chunk 그룹은 거의 payload 크기가 된다. 이 설계는 whole-page Patch에 기대지 않는다 |
| 동시성 | 모든 ChunkWrite가 allocator page 하나를 patch한다. A4가 writer를 병렬화할 때 이 page는 공유 지점이다(PostgreSQL의 relation extension lock과 비슷한 자리). 08-3은 단일 MutationPort라 영향이 없다 |
| 반환 | 실제 Free·재사용은 14-2다. 그 전까지 반환한 chunk page는 첫 extent의 dynamic page를 계속 차지한다. 큰 행을 많이 넣고 rollback하는 작업은 첫 extent 55 page 안에서 Capacity로 끝난다 |

<a id="tests"></a>
## 10. 시험

기대값은 독립 상수, 독립 Python vector, 거래가 쓴 값에서 정한 상태 기계에서 온다. producer가 다시 읽은 값을 기대값으로 쓰지 않는다. 바뀐 코드의 시험만 실행한다.

| ID | 단계 | 사건과 판정 |
| --- | --- | --- |
| OVF-F01 | O1 | chunk 형식 개정: 독립 Python으로 다시 계산한 page literal·full-page SHA, root 칸 nonzero 거절, verifier의 방문 주소 집합(뒤 chunk가 root 주소, next가 root 주소) |
| OVF-F02 | O1 | Job3 overflow root: Reserved pending(front zero ⇔ written 0, written ≤ k, total 범위), Retained, ReleasingCompensated chain, `@285` 1의 효과·state 조합 거절. `forward_wal`·`cleanup` 값 |
| OVF-F03 | O1 | XTR1 Overflow 항목: 정상, kind 7 level ≠ 0, retire LSN 규칙(kind별) |
| OVF-F04 | O1 | 세 opcode의 shape·frame bytes(57,184, b·a 경계의 OverflowCancel·ChainRetire), wire decode의 anchor native 사전 scan 손상, 옛 opcode 거절 |
| OVF-F05 | O1 | A의 `canonical_row_digest_borrowed`·`encode_canonical_window_borrowed`가 전체 encoder bytes·SHA와 같음(2,049/16,092/16,093/32,185/131,072, 여러 열·경계에 걸친 값), `CanonicalRowStream`이 chunk 경계를 가리지 않고 decoder와 같은 판정(손상 vector 포함) |
| OVF-C01 | O2 | ChunkWrite 전이: allocator·INIT·pending, index·next·payload 길이·front 위반 거절 |
| OVF-C02 | O2 | Forward(overflow): written < k, root·total 불일치, chain 손상(digest·schema·순서·중복 주소) 거절, 정상 전이 |
| OVF-C03 | O2 | OverflowCancel·ChainRetire: XTR1 after, 손상 chain 거절(일부 반환 없음), ReleaseRole이 chain 뒤에만 |
| OVF-C04 | O2 | replay가 위 그룹을 같은 결과로 다시 계산하고, 바뀐 frame을 거절 |
| OVF-C05 | O2 | Base4 소유 검산: 네 소유자 각각, 소유자 없는 page, 두 소유자, 다른 incarnation |
| OVF-J01 | O3 | 자동커밋 INSERT 2,049·16,092·16,093·131,072 B 행. 같은 거래와 새 statement의 point·cursor 읽기 값이 쓴 값과 같음, allocator·chunk 수 |
| OVF-J02 | O3 | 명시 거래: 큰 행·작은 행 섞어 넣고 statement rollback(큰 행 CLR → ChainRetire), full rollback, commit 뒤 Prune(chain 유지)과 drain |
| OVF-J03 | O3 | chunk 쓰기 도중 Free page 부족(Capacity) → OverflowCancel → statement 실패, 이후 INSERT 정상. written 0이면 Cancel |
| OVF-J04 | O3 | Forward의 chain 검산 실패(시험 전용 producer가 창 하나를 바꿈) → OverflowCancel, 효과 없음 |
| OVF-J05 | O3 | 다른 거래가 미확정 큰 행을 Missing으로 봄(chunk를 읽지 않음), commit 뒤 snapshot 규칙대로 보임. 출력 부족 `OutputTooSmall{total}` |
| OVF-J06 | O3 | 큰 행이 있는 leaf의 자동 split·unlink, split 뒤 읽기. DELETE·UPDATE 대상이 큰 행이면 Unsupported |
| OVF-J07 | O3 | census: 새 C 진입·Forward 검산·B 복사·drain의 frame이 기존 상한 안(넘으면 근거와 새 상한) |
| OVF-R01 | O3 | 재시작: loser Reserved(written 0·중간·k), loser Retained, winner. 복구 뒤 Base4·XTR1·읽기 |
| OVF-N01..N06 | O4 | native fault cut: ChunkWrite(첫·중간·마지막) 앞뒤, Forward(overflow) 앞뒤, CLR 뒤 ChainRetire 앞뒤, OverflowCancel 앞뒤. 각 사례 produce·recover·reenter 세 process, 독립 기대 상태 |

<a id="stages"></a>
## 11. 단계와 적립

| 단계 | 내용 | 비고 |
| --- | --- | --- |
| O0 | 이 설계 | 문서만. 일정 08-3 = 진행(적립 0) |
| O1 | 형식·순수 codec(§4, §6.1의 A helper·`CanonicalRowStream`)과 OVF-F | 제품 경로는 아직 overflow를 거절 |
| O2 | C 전이·replay·Base4(OVF-C) | 제품 경로는 아직 거절 |
| O3 | 제품 경로: A 실행기·C 진입·B 읽기·live 정리·복구(OVF-J·R), census | 큰 행 INSERT가 처음 성공 |
| O4 | native fault cut(OVF-N) | **08-3 적립(2일)은 O4의 병합** |

각 단계는 PR 하나와 독립 D2이며, 필수 지적을 해소한 뒤 연다. 08-3은 08의 마지막 항목이 아니므로 항목 조건으로 적립한다. 08-1이 U4의 병합으로 적립된 것과 같다.

<a id="alternatives"></a>
## 12. 그 밖의 대안

| 대안 | 판단 |
| --- | --- |
| CLR 그룹 안에서 chain을 바로 XTR1로 반환 | CLR(91,340)에 anchor native·whole-body Patch를 더하면 약 112 KB로 frame 98,304를 넘는다. 그래서 CLR은 반환할 chain을 Job에 남기고 ChainRetire가 따로 한다 |
| chunk마다 반환 그룹 하나(Undo chunk 반환처럼) | Undo chunk 반환은 page를 Free로 돌려 재사용되므로 앞 page를 다시 읽지 않는다. 08-3의 반환은 page를 Allocated로 남기므로 chain 전체를 한 그룹에서 걸어도 안전하고, 그룹 수가 k에서 1로 준다 |
| 반환한 page를 같은 table의 새 chain에 바로 재사용 | 같은 incarnation이면 물리 위험은 적지만, 14-2의 eligibility(horizon·pin·flush claim·DWB)를 08-3이 대신 정하게 된다. retired 목록에 두고 14-2에 맡긴다 |
| chunk native에 payload 전체를 싣기 | chunk마다 16 KB WAL이 는다. replay는 INIT image로 payload를 본다 |
| chunk 그룹 하나에 chunk 둘 | frame은 들어가지만(약 74 KB) 그룹 모양이 둘이 된다. 고정 부담은 A2가 줄이므로 chunk 하나로 둔다 |
| Undo(U)가 쓰는 중인 chain을 소유 | U는 Forward 전에는 Free라 pending을 담을 수 없고, Undo codec을 08-3에서 바꾸게 된다. Job 칸으로 충분하다 |

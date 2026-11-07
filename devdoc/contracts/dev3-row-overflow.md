# DEV3 큰 행 overflow chain — 쓰기·읽기·반환 설계

2026-11-07 · `D3-ROW-OVERFLOW-001` · **설계(O0) 4판 / PR #66 추가 D2 검토 반영 / O1 형식 계층 구현·OVF-F PASS([§14](#o1-record), 독립 D2 4회차 ACCEPT) / O2–O4 NOT_RUN**

[overflow 표현](dev3-overflow-format.md) · [행 표현](dev3-row-format.md) · [행 owner R6·§5.4](dev3-task-06-tree-rows.md#algorithms) · [첫 Row profile](dev3-module-consumers.md#profile) · [WAL shape](dev3-module-consumers.md#row-shape) · [scope3 wire](dev3-module-consumers.md#row-scope3-wire) · [unlink retired 목록](dev3-tree-smo.md#empty-leaf-unlink) · [자원 정본](dev3-resource-bounds.md) · [목표 등급 SCOPE-002](../plans/project-direction.md#scope-002) · [일정 08-3](../plans/dev3-schedule-baseline.md#items) · [D2 1회차](../../module-gate/evidence/dev3-08-3-design/review-r1.md) · [D2 2회차](../../module-gate/evidence/dev3-08-3-design/review-r2.md) · [D2 3회차](../../module-gate/evidence/dev3-08-3-design/review.md)

일정 08-3 "overflow chain 쓰기·읽기·반환"(2일, 1단위)의 설계다. 지금 Row 경로는 canonical 2,048 B를 넘는 행을 첫 효과 전에 `Unsupported`로 거절한다(A `encode_input`, C RowReserve의 `MIN_CANONICAL..=INLINE_ROW_BYTES`). [overflow 표현](dev3-overflow-format.md)의 head·chunk codec과 streaming 검산기는 있으나 실제 page 할당·WAL·소유·회수가 없다. 이 문서는 코드보다 먼저 그 형식·전이·비용·복구·시험·단계를 정한다. 제품 PASS가 아니다.

2판은 D2 1회차의 필수 R1–R5를 반영했다. 1판이 "그대로"라고 적은 기존 불변식 가운데 실제 코드에서 새 전이를 거절하는 것(XTR1 revision, Job3·ledger·descriptor LSN 규칙, Base4 certificate·provenance, split·unlink 검산과 leaf 관측기)을 각각 바꾸는 규칙으로 정했고, 바꾸는 정본 문장은 [§13](#supersedes)에 모았다.

3판은 D2 2회차의 필수 N1·N2와 권고를 반영했다. N1: provenance role 7 claim을 지금 CREATE pair로 읽는 Base4 소비자 네 곳(`InitProvenance::pair`, `validate_pairs`, `relations::provenance`, `relations::payloads`)과 claim 생성 함수를 §8.1·§8.3에 정했다. N2: §4.3 표에서 Reserved·Retained의 `[288,320)`은 XRP1 projection이다.

4판은 PR #66 추가 검토의 R1–R3를 반영한다. R1은 길이를 모르는 preflight와 값 검산 뒤 allocator 용량 판정을 분리하고, R2는 chunk 적용 뒤에도 살아 있는 예약·draft 갱신·flush 진입을, R3는 B 복사 단계의 `ReadStep` 반환과 cursor 재시도를 명시한다. [추가 검토·새 설계 oracle](../../module-gate/evidence/dev3-08-3-design/review-codex.md)은 제품 시험과 구분한다.

<a id="scope"></a>
## 1. 범위

**08-3은 2,049..131,072 B canonical 행의 INSERT가 chain을 쓰고, B가 그 행을 읽고, 그 INSERT가 rollback되면 chain을 반환하는 경로다.**

| 포함 | 제외(첫 효과 전 거절하거나 다루지 않음) |
| --- | --- |
| 자동커밋·명시 거래의 INSERT, canonical 2,049..131,072 B(각 값 ≤ 64 KiB는 기존 codec 검사) | DELETE·UPDATE의 대상이 overflow 행(지금처럼 `TargetClass::Unsupported`) → 08-4 "큰 행 복원" |
| chunk 쓰기 그룹, Forward의 chain 검산과 overflow head 행 | UPDATE의 새 값이 2,048 B 초과(지금처럼 Unsupported) → 08-4 |
| B point·cursor 읽기(own 미확정 행과 committed 행)의 materialization | index가 있는 table(기존 profile 제한), 128 KiB 초과 행 |
| statement·full rollback(CLR)과 복구 loser의 chain 반환, chain 쓰기 도중 실패의 반환 | 반환한 page의 allocator Free·재사용(3.14.1→3.03.4→3.14.2, 일정 14-2) |
| commit 뒤 Prune(chain은 행과 함께 남음), 큰 행이 있는 table의 split·unlink, replay·Base4·native fault cut | streaming 읽기 API, 여러 extent(12-1) |

"반환"의 뜻은 [행 owner](dev3-task-06-tree-rows.md#algorithms)의 회수 행 "unlink·overflow 교체 뒤 회수 … 3.14.1→3.03.4→3.14.2. unlink만으로 Free 선언 금지"를 따른다. 08-3의 반환은 chain을 쥔 owner(Job)에서 소유를 내려 그 table의 **retired 목록**(anchor XTR1, [unlink](dev3-tree-smo.md#empty-leaf-unlink)과 같은 목록)으로 옮기는 logged 전이다. page는 `Allocated(g, table, Overflow)`로 남고 실제 Free·재사용은 14-2가 retired 목록에서 한다. 이것은 08-1의 unlink가 퇴역 tree page를 다룬 방식과 같다.

DELETE·UPDATE의 overflow를 08-4로 두는 이유: 그 경우 U의 before가 2,048 B를 넘어 Undo chunk 또는 옛 chain 참조가 필요하고, commit 뒤 옛 chain 반환에는 읽는 중인 reader의 transient pin 규칙이 필요하다. 이것이 일정 08-4의 "Undo chain·큰 행 복원"이다. 08-3의 반환은 다른 reader가 볼 수 없는 chain(아직 Forward 전이거나, 미확정 INSERT가 CLR로 지워진 행의 chain)에서만 일어나므로 reader 경합이 없다(§7.4).

<a id="assumptions"></a>
## 2. 지금의 inline 전용 가정

| 영역 | 지금 | 08-3이 바꾸는 것 |
| --- | --- | --- |
| A 값 검사 | `encode_input`: L>2048이면 Unsupported, leaf 여유 `L+168+16` 검사, 16 KiB CANDIDATE에 canonical 전체를 씀 | L ≤ 131,072이면 overflow 계획(§6.1). leaf 검사는 head 64 B 기준, CANDIDATE는 chunk 한 개 창 |
| C Reserve·preflight | RowReserve producer가 L을 `5..=2048`로 묶고, Job `C`(185..2228)와 XRP1 projection으로 값 commitment | overflow root(§4.3)는 C=244와 root를 비운 XRP1(§4.2). preflight가 Free 수를 관측하고 reserve(plan)가 k개를 첫 효과 전 판정(§7.2) |
| 쓰기 그룹 | Row 그룹은 page를 할당·INIT하지 않고, anchor는 읽기만(proof page) | ChunkWrite가 allocator Free page 하나를 Allocated(table, Overflow)로 바꾸고 chunk page를 INIT, OverflowCancel·ChainRetire가 anchor를 씀(§4.5) |
| Job3·ledger·descriptor | Reserved는 `last_step == lease_lsn`, descriptor `chain.apply == lease_lsn`, ledger row1은 1걸음 | Reserved overflow root의 LSN·걸음 규칙, ReleasingCompensated의 chain 칸(§4.3, §6.2) |
| leaf 행 | Inline payload와 tombstone만 쓰고 읽음(`leaf.rs` `validate_rows` 등, §7.5 목록) | Overflow payload(head 64 B) 행: record 232, cell **C=244**(12+168+64) |
| B 읽기 | C가 leaf의 inline canonical을 복사(`plan4_apply/row/read.rs`) | Overflow 행이면 chain을 읽어 caller output에 materialize(§7.3) |
| 반환 | 없음. XTR1은 RowLeaf·RowInternal만, revision ≥ 2 anchor에만 | XTR1이 Overflow level0 항목을 revision과 무관하게 받음(§4.4). OverflowCancel·ChainRetire(§4.5) |
| split·unlink 검산 | `reached ⊎ retired == owned`, owned는 tree page뿐 | kind 7 page를 따로 세는 소유 closure(§7.6) |
| Base4 | certificate·provenance가 kind 7 dynamic page를 받지 않음, tree 전체와 retired 목록·allocator 역소유를 검산 | certificate role·provenance role 7, chunk page의 네 소유자를 정확히 한 번 센다(§8.3) |
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

A4를 고른다. A2의 장점(쓰는 도중 streaming 검산)은 Forward에서 chain을 앞에서부터 한 번 읽어 얻을 수 있다. 그 chunk들은 방금 이 거래가 쓴 page이고 Job만 소유하므로, Forward까지 아무도 바꿀 수 없다. 어느 ChunkWrite 사이에서 crash가 나도 front에서 written개를 따라가면 마지막 next가 zero인 완결된 부분 chain이 남으므로, loser 반환(§8.2)이 언제나 성립한다.

<a id="format"></a>
## 4. 형식

`k = ceil(total/16,092)`(1..9), `W`는 ChunkWrite frame bytes, `h`는 previous transaction H 존재(0/1)다.

<a id="chunk-revision"></a>
### 4.1 chunk 형식 개정

[overflow 표현](dev3-overflow-format.md)의 chunk cell `@24..48`(root PageRef)을 **reserved zero**로 바꾼다. head 64 B는 그대로다. 이 codec은 아직 어떤 DB에도 쓰이지 않았으므로 version은 1 그대로 두고, 개발 형식 규칙([버전 정책](../policies/version-compatibility-policy.json))에 따라 개정 전 bytes는 Corrupt로 거절한다.

- chunk가 운반하는 chain 식별은 root 없는 `OverflowChain(total, digest)`이다. head는 `chain + root`다. cell encoder·decoder는 chain을 받고, cell decoder는 `@24..48`이 zero가 아니면 Corrupt다.
- "index0의 현재 PageRef는 root와 같다"는 검사는 `OverflowChainVerifier`가 맡는다: verifier가 받은 expected root(= head.root)가 chunk 0의 page identity다. "뒤 chunk는 root와 같은 물리 주소를 가질 수 없다"와 "next가 root와 같은 물리 주소이면 거절"은 verifier의 방문 주소 집합(최대 9개, root 포함)으로 옮긴다. 국소 검사는 next ≠ 현재 page 주소만 남는다.
- head가 없는 곳(반환할 chain, pending chain, retired chunk)을 위해 두 검사를 더한다. `inspect_overflow_page(page, expected identity)`는 envelope와 cell 문법(XOV1·index < count·count = ceil(total/16,092)·payload 길이·next 유무·reserved zero)을 검사하고 (chain, index, next)를 돌려준다. `OverflowSegmentWalk(identity, chain, first index)`는 앞에서 뒤로 push받으며 index가 하나씩 늘고, chain이 같고, identity가 앞 chunk의 next이고, 방문 주소가 겹치지 않고, 마지막(index k−1)의 next가 zero인지를 검사한다. payload digest는 보지 않는다(부분 chain에서는 정할 수 없다).
- 다른 chain의 chunk가 끼어드는 경우: chunk는 같은 count·total·digest·index를 가져야 하므로 끼어든 chunk는 전체 SHA-256이 같은 행, 곧 같은 bytes다. 같은 page를 두 chain이 쥐는 손상은 Base4의 정확히 한 번 소유 검산(§8.3)이 잡는다.
- 고정 시험 8개의 literal 중 page literal·full-page SHA·root 칸 항목이 바뀐다. 새 기대값은 O1에서 독립 Python `struct/hashlib`로 계약에서 다시 계산한다. 그 script는 먼저 개정 전 literal(checksum `1f97d1d7…`, full-page `cd849926…`)을 다시 만들어 자신을 검산한다. 제품 encoder로 기대값을 만들지 않는다.

<a id="overflow-row"></a>
### 4.2 overflow 행과 값 commitment

leaf의 행은 기존 [행 표현](dev3-row-format.md)의 Overflow payload다: record = header 168 + head 64 = 232 B, tree cell **C = 244**. stamp·logical·undo는 INSERT 행과 같다(`Transaction(T)`, `(T.id, mutation)`, undo = U, U는 Missing before).

digest는 쓰임이 셋이고 섞지 않는다.

1. **값 commitment**([scope3 wire Q1](dev3-module-consumers.md#row-scope3-wire))는 Reserve의 Job `C`와 XRP1 projection이다. overflow 행의 head.root는 마지막 ChunkWrite가 정하므로 Reserve가 알 수 없다. 그래서 **overflow root의 projection은 record의 head.root 24 B를 zero로 둔 XRP1**이다(head의 total·count·digest는 포함). Forward·CLR·Prune은 현재 record의 root를 zero로 만든 사본으로 같은 XRP1을 계산해 Job projection과 대조한다. 이 사본은 projection 대조에만 쓴다.
2. **core의 before·after digest**와 native·redo의 digest는 지금처럼 실제 page·record bytes(root 포함)를 쓴다.
3. **head.digest**는 canonical row 전체의 SHA-256이다. Forward의 chain 검산(§6.3)과 B의 materialization(§7.3)이 payload로 확인한다. root 자체는 Forward에서 Job의 pending front와 같아야 하며(§5), 그 뒤에는 leaf 행의 bytes가 운반한다.

<a id="job-overflow"></a>
### 4.3 Job3 overflow root

| 필드·규칙 | Reserved | Retained | ReleasingCompensated | ReleasingCommitted |
| --- | --- | --- | --- | --- |
| `@285` payload | 1 | 1 | 1 | **0**(Prune이 0으로 씀) |
| `@280` cell_bytes | 244 | 244 | 0(지금처럼) | 0 |
| `[200,248)` | pair(anchor, leaf), 지금처럼 | pair, 지금처럼 | **chain**: `[200,224)` anchor(pair[0] 그대로), `[224,248)` chain root. ChainRetire가 zero로 씀 | zero |
| `[288,320)` | XRP1 projection(§4.2의 root를 비운 XRP1), 지금처럼 | projection, 지금처럼 | CLR이 projection을 지우고 `[288,292)` **chain total** u32(2,049..131,072), `[292,320)` zero. ChainRetire가 total을 zero로 씀 | zero(지금처럼) |
| `[328,360)` | **pending**: `[328,352)` front PageRef(쓴 chunk가 없으면 zero), `[352,356)` total u32, `[356,358)` written u16(0..k), `[358,360)` zero. front zero ⇔ written 0 | sender digest(Forward가 pending을 덮음, written = k이어야 Forward 가능) | sender digest | sender digest |
| `last_step` | written 0이면 `== lease_lsn`, written ≥ 1이면 마지막 ChunkWrite의 LSN(`> lease_lsn`) | `== adoption_lsn` | chain이 있으면 cursor 0이고 `== release_start`, 없으면(ChainRetire 뒤) `> release_start` | 지금처럼 |

- `@285 = 1`은 Row scope의 효과 0(INSERT) root에서만이다. 그 밖의 Job(Free·Terminal·Catalog·DELETE·UPDATE root)은 지금처럼 0이다. Releasing의 zero 규칙은 `[200,248)`·`[280,284)`·`[285,320)`에서 위 표의 칸을 뺀 것이 된다.
- ReleasingCompensated에서 `@285 = 1`이고 chain 칸이 zero이면 "chain을 반환했다"는 뜻이다. 그래서 cursor 0의 `last_step == release_start` 규칙은 chain이 있는 root와 inline root에만, `last_step > release_start`는 ChainRetire를 마친 overflow root에 적용한다. chain이 있으면 release cursor는 0이어야 한다(ReleaseRole은 chain 뒤에만).
- chain의 anchor는 Retained pair[0]을 CLR이 그대로 남긴 것이다. ChainRetire는 이것으로 XTR1을 쓸 anchor를 찾는다(catalog 조회 없음). total은 k와 cleanup 비용을 정하므로 chunk를 읽지 않고도 알 수 있게 Job에 둔다.
- `JobView`에 `payload()`, `pending()`(front·total·written), `chain()`(anchor·root·total), `forward_steps()`를 더한다. Job codec은 위 조합만 받고 나머지를 거절한다(칸 문법은 Corrupt, 주소 alias와 dynamic이 아닌 주소는 IdentityMismatch, LSN 순서와 chain이 있는 root의 cursor ≠ 0은 OrderMismatch로 기존 Job decoder와 같다). 옛 binary는 `@285=1`을 Corrupt로 거절한다(개발 형식 규칙).
- 비용 recipe(§6.2)는 Job에서 나온다: Reserved overflow root의 `forward_wal() = (k−written)·W + F`, `forward_steps() = 1 + k − written`(inline은 지금처럼 1). `cleanup()`은 Reserved·Retained에서 지금 식에 C=244를 넣은 값, chain이 있는 ReleasingCompensated에서 `(5, 4R + X)`, 그 밖의 Releasing에서 지금처럼 `(4−cursor, (4−cursor)·R)`이다.
- "정리 걸음이 시작됐다"는 판정은 `release_cursor != 0` **또는** ChainRetire를 마친 overflow root(ReleasingCompensated·`@285 = 1`·chain 칸 zero)다. `JobView::cleanup_started()`로 두고, 지금 cursor만 보는 곳(`semantic_row/ledger.rs`의 ReleasingCompensated terminal 규칙, `retention_page/row/census.rs`의 terminal 상태 규칙과 terminal 순서 규칙)이 이것을 쓴다. ChainRetire는 terminal 뒤에만 열리므로 이 규칙들이 그대로 성립한다.
- Job 관측기([`jobs.rs`](../../crates/engine/src/storage/semantic_row/jobs.rs))의 바꿀 수 있는 구간: ChunkWrite는 `(328,360)`(pending)과 `(376,384)`(last_step. ChunkWrite는 `cleanup()`을 바꾸지 않으므로 budget 칸은 그대로), ChainRetire는 `(200,248)`·`(288,292)`·`(376,1024)`이다. Forward·CLR·Prune의 구간은 그대로이며 위 표의 칸이 그 안에 있다(Forward `(328,360)`, CLR·Prune `(200,248)`·`(285,320)`).

<a id="retired-overflow"></a>
### 4.4 retired 목록의 Overflow 항목

[XTR1](dev3-tree-smo.md#empty-leaf-unlink) 항목의 kind/level 허용에 **(Overflow 7, 0)**을 더한다. 항목 형식(40 B)과 순서·주소 규칙(첫 file, page 9..63, 엄격 증가, anchor·root·다른 항목과 다른 물리 주소)은 그대로다. n ≤ 55는 첫 extent의 dynamic page 수이므로 목록은 넘치지 않는다.

**revision 규칙은 kind별이다.** 지금 `check_retired`와 `encode_listed_anchor`는 목록이 있으면 anchor revision ≥ 2를 요구한다([tree-smo](dev3-tree-smo.md#empty-leaf-unlink)의 "목록은 unlink 그룹이 revision+1과 함께만 쓴다"). 2판부터는 **목록에 tree 항목(RowLeaf·RowInternal)이 하나라도 있을 때만** revision ≥ 2를 요구한다. Overflow 항목만 있는 목록은 revision 1(CREATE 직후, 한 번도 split되지 않은 table)에서도 유효하다. Overflow 항목을 넣어도 anchor root revision은 바뀌지 않는다. revision은 tree 구조(route)의 변화를 뜻하며 cursor·locator가 그것으로 재탐색을 판정하고([cursor C4](dev3-cursor-locator.md#cursor)), Base4는 `revision ≤ 1 + Structural high-water`로 묶는다. chain 반환은 route를 바꾸지 않는다. revision 1의 Overflow 목록 위에서 첫 unlink가 일어나면 그 그룹이 revision 2와 함께 두 kind가 섞인 목록을 쓴다(`RetiredList::extend`의 물리 순서 병합이 기존 Overflow 항목을 보존한다).

**retire LSN 규칙도 kind별이다.** tree page 항목은 지금처럼 Structural owner의 id(≤ Structural high-water)이고 tree page로 decode한다. Overflow 항목의 retire LSN은 그 항목을 넣은 OverflowCancel·ChainRetire 그룹의 complete LSN이다. Structural owner가 아니므로 Structural high-water와 비교하지 않고, `0 < retire LSN ≤ anchor pageLSN`(codec)과 `chunk pageLSN < retire LSN`(page를 읽는 Base4)만 요구한다. Overflow 항목은 tree page로 decode하지 않고 `inspect_overflow_page`로 읽는다. 이 LSN도 snapshot q와 비교하지 않는다.

<a id="opcodes"></a>
### 4.5 opcode와 shape

| opcode | outer·role | 바뀌는 page·native | E/K/I, redo | frame bytes(최대) |
| --- | --- | --- | --- | ---: |
| `ChunkWrite = 14` | XHG Update, handoff, role 자기 자신 | Root 1152 · Job 2176 · allocator entry 320(kind 6) · chunk 256(kind 12) | 0/0/0, Root P·Job P·allocator entry patch 160·chunk INIT 16,448 | **56,976** |
| `OverflowCancel = 15` | XHG Update, handoff, **role Cancel** | Root 1152 · Job 2176 · anchor XTR1 `128+b+a`(kind 10) | 0/0/1, Root P·Job P·anchor P | `56,472+b+a` ≤ **60,864** |
| `ChainRetire = 25` | XSG System(정리 걸음), role 자기 자신 | Root 1152 · Job 2176 · anchor XTR1 `128+b+a`(kind 10) | 0/0/0, Root P·Job P·anchor P | `54,664+b+a` ≤ **59,056** |

P = 16,344(whole-body Patch). b·a는 anchor XTR1 cell의 before·after 길이(목록이 없으면 0, 있으면 `16+40n`)이고 최대는 b = 2,176, a = 2,216이다. 세 opcode 모두 98,304 B frame 안이다. 식: XHG 3,776 / XSG 2,176 + native(128+before+after의 합) + 보조(edge 128·count 96·item 208) + redo.

- **번호.** 14·15는 handoff Update 계열(1–13)의 다음 빈 번호이고 16부터는 System 계열이다. 25는 정리 걸음(20–24) 뒤에 같은 계열로 둔다. 18·19는 지금처럼 쓰지 않는다.
- **native kind.** 기존 공통 번호를 쓴다: allocator page 6, tree anchor 10(Catalog CREATE와 같은 번호). chunk page는 새 번호 **12**다(1·3·5·6·7·8·9·10·11과 겹치지 않는다). allocator entry native는 바뀐 entry 하나의 before·after 96 B씩이고 header의 slot(@4·@6)은 0, offset(@8·@10)은 그 entry의 page 안 위치(allocation cell의 시작 + `ALLOCATION_HEADER_BYTES + 96·ordinal`)다. chunk native는 presence 2(after만)이고 after body는 chunk cell의 128 B header, after slot 0·offset은 cell 위치다. payload는 INIT redo에만 있고 native에 다시 싣지 않는다([scope4 native](dev3-tree-smo.md#제안-f01-scope4-frame)가 before/after page를 중복 저장하지 않는 것과 같은 이유). anchor native는 XTR1 cell(anchor slot 1)의 before·after이고 목록이 없던 쪽은 presence에서 뺀다.
- **redo.** Row 그룹의 redo는 지금 whole-body Patch뿐이다. ChunkWrite는 두 형식을 Row 그룹에 처음 들인다: allocator는 바뀐 entry 96 B만 Patch하고(Status 224 구간 patch와 같은 방식, `64+96=160`), 신규 chunk page는 full INIT(16,448)이다. `validate_redo_shape`가 opcode별로 이 둘을 받는다. 나머지는 whole-body Patch다.
- **I(inventory item).** Reserve는 U의 inventory item을 만들고 Forward·Cancel이 지운다(`graph.rs` `inventory()`의 action 1·2·3, `prepare.rs` descriptor 분기). ChunkWrite는 inventory를 바꾸지 않으므로 I=0이다(descriptor `inventory.change_lsn`은 lease_lsn 그대로). OverflowCancel은 role Cancel이라 Cancel처럼 action 3으로 item을 지운다(I=1). ChainRetire는 거래 descriptor 밖의 정리 걸음이라 I=0이다.
- **Root 효과.** 세 opcode 모두 Root revision+1·pageLSN·job mask·digest 갱신은 다른 그룹과 같다.

| opcode | descriptor(handoff) | Root 계수 | job cursor `@72` |
| --- | --- | --- | --- |
| ChunkWrite | chain.apply := LSN, groups+1, consumed_wal += 56,976, inventory 그대로, remaining 재계산(row1이 W·1걸음 준다) | 없음 | 그대로 |
| OverflowCancel | Cancel과 같음(inventory 비움) | canceled+1(`prepare.rs`의 Cancel 분기에 OverflowCancel을 더한다) | 그대로 |
| ChainRetire | 없음(non-handoff) | consumed_wal += 실제 frame | 선택한 Job ordinal+1(ReleaseRole과 같음) |

- **core·ref.** 세 opcode의 core는 Reserve·Cancel·ReleaseRole과 같은 ref 규칙을 따른다(current 필수, next·target 없음). ChunkWrite·OverflowCancel은 Reserved root의 statement/mutation을, ChainRetire는 Releasing root의 work cursor를 쓴다.
- **native slot.** `validate_native_shape`는 지금 kind 1·9 밖의 native에 slot 0을 요구한다. allocator(6)·chunk(12)는 slot 0이고, anchor(10)는 XTR1 cell이므로 **slot 1**이다. 이 분기를 더한다.
- **wire decode.** ChunkWrite의 shape는 고정이다. OverflowCancel·ChainRetire는 kind 10 native header의 before·after 길이로 b·a를 읽은 뒤 전체 크기를 검산한다(UPDATE의 C' 사전 scan과 같은 bounded 규칙: 최대 3개 record, 각 128 B 이상·구간 안, 남는 bytes 없음, b·a ∈ {0} ∪ {16+40n}). 옛 binary는 14·15·25를 Unsupported로 거절한다.
- `Operation::role()`: OverflowCancel → Cancel, ChunkWrite·ChainRetire → 자기 자신. role은 chain·statement·descriptor 규칙에만 쓰고 page 효과와 크기는 opcode로 정한다(DELETE·UPDATE의 role과 같은 원칙). OverflowCancel은 `role()`이 자기 자신이 아닌 첫 비-DELETE·UPDATE opcode다. 지금 `op_effect`(`transition/prepare.rs`)와 `opcode_effect`(`semantic_row/undo.rs`)는 `op != op.role()`을 DELETE로 추론하므로, 명시적 `Operation::delete()`(DeleteForward·DeleteClr·DeletePrune)로 바꾼다(O1). Forward·CLR·Prune·Cancel의 shape는 바뀌지 않는다(overflow root에서 C=244일 뿐이다).

<a id="transitions"></a>
## 5. 전이

| 그룹 | Job | allocator | chunk page | leaf | anchor | Status·U |
| --- | --- | --- | --- | --- | --- | --- |
| Reserve(overflow) | Free→RootReserved, `@285=1`, C=244, root를 비운 XRP1, pending(front 0, total, written 0) | — | — | — | — | U는 지금처럼 claim만 |
| ChunkWrite(i) | pending front := 새 page, written+1, last_step := LSN. i = k−1−written(쓰기 전) | 가장 낮은 allocator-eligible Free → Allocated(next generation, table, Overflow) | INIT: chunk i, next = 이전 front(i=k−1이면 zero), payload = 창 i | — | — | — |
| Forward(overflow) | Reserved→Retained, pending → sender digest | — | (검산만, §6.3) | Missing → overflow 행(C=244), head = (front, total, digest) | — | 지금 INSERT Forward와 같음 |
| OverflowCancel | Reserved(written ≥ 1)→Free(같은 generation) | — | (읽기만) | — | XTR1 + written개 항목 | U Free generation 유지(Cancel과 같음) |
| CLR(overflow) | Retained→ReleasingCompensated, `[224,248)` := head.root, `[288,292)` := total, `[200,224)` anchor 그대로 | — | — | overflow 행 → Missing | — | 지금 INSERT CLR와 같음 |
| ChainRetire | chain 칸 → zero, last_step := LSN, cursor 0 그대로 | — | (읽기만) | — | XTR1 + k개 항목 | — |
| Prune(overflow) | Retained→ReleasingCommitted, `@285` := 0 | — | — | 재stamp(head 그대로) | — | 지금 INSERT Prune과 같음 |

- **ChunkWrite의 검사.** Reserved overflow root이고 written < k, 새 chunk의 index·count·total·payload 길이(`min(16,092, total−i·16,092)`)가 pending과 맞고, next가 pending front와 같다. digest는 head에 들어갈 값(A 계획의 digest)이며, 앞서 쓴 chunk가 있으면 front chunk의 digest와 같아야 한다. Forward가 payload 전체로 검산한다. 할당할 page는 split의 신규 page와 같은 allocator-only 규칙으로 고른다(`structure/candidate.rs`의 선택: 첫 extent 9..63에서 가장 낮은 ordinal의 Free, `next_reservation_generation`이 MAX가 아님). allocator에 Reserved entry가 하나라도 있으면(진행 중인 System 예약) ChunkWrite는 열리지 않는다(split audit와 같은 조건). Base4의 System batch 검산이 allocator pageLSN을 batch birth로 쓰므로 그 사이에 Row 그룹이 allocator를 patch하면 안 된다. preflight가 이 상태를 관측하고 reserve(plan)(§7.2)가 첫 효과 전에 allocator 용량 부족으로 거절한다. preflight 뒤에 생기면 ChunkWrite는 Unchanged OrderMismatch이고 A는 §7.1의 4로 OverflowCancel·statement 실패로 끝낸다(재시도하지 않으므로 끝이 있다). 원 pool이 그 page를 지금 받을 수 없으면(next generation claim·mutable flush lease·DWB idle) Unchanged `FlushRequired`/Busy로 돌아가 같은 chunk를 다시 부르며, 다른 page를 고르지 않는다. 그래서 replay가 같은 page를 다시 고른다.
- **Forward의 검사.** 지금의 INSERT Forward 검사(원 schema·version·`record.len+12 == C`·XRP1)에 더해: written = k, record의 head.root = pending front, head.total = pending total, 그리고 §6.3의 chain 검산이 성공해야 한다.
- **OverflowCancel**은 written ≥ 1인 Reserved overflow root를 지우는 Cancel이다. written = 0이면 지금의 Cancel을 쓴다(OverflowCancel은 Unchanged InvalidInput). front에서 `OverflowSegmentWalk`(first index k−written)로 written개 chunk를 읽어 그 page를 XTR1에 넣는다. 마지막 chunk(index k−1)의 next는 zero여야 하고 그 수가 written과 같아야 한다.
- **jobs.rs census.** 지금 census는 Reserve·Forward·Cancel·Commit·AbortEnd·Clr에만 TerminalLeased를 허용하고 나머지 연산에는 TerminalReady를 요구한다. ChunkWrite와 OverflowCancel(role Cancel)을 TerminalLeased 목록에 더하고 둘 다 `reserved == 1`을 요구한다. ChainRetire는 목록에 넣지 않는다. 그래서 기본 분기(TerminalReady)가 "ChainRetire는 terminal 뒤"를 강제한다.
- **ChainRetire**는 chain이 있는 ReleasingCompensated root의 첫 정리 걸음이다(cursor 0, 다른 정리보다 먼저). 그 root의 거래가 terminal Ready가 된 뒤의 live 정리 drain과 복구 cleanup에서만 열린다(Releasing 정리는 maintenance이고 terminal 뒤에만 연다: [module-consumers](dev3-module-consumers.md#profile), `row_start/retire.rs`의 `row_cleanup_next`). 그래서 statement rollback의 CLR 직후가 아니라 거래가 끝난 뒤 반환된다. root에서 k개 chunk를 head 없는 verifier(`OverflowSegmentWalk`, first index 0, chain = chunk 0의 chain, total = Job의 total)로 읽어 XTR1에 넣는다. chain이 남아 있는 동안 ReleaseRole은 Unchanged InvalidInput이다.
- 반환 두 그룹이 읽는 chunk는 그 그룹의 변경 page가 아니다. 이 chain은 그 root만 쥐고 있고 단일 MutationPort 아래서 바뀌지 않으므로, producer가 읽은 next와 index를 Job·XTR1의 after가 운반하고 replay가 같은 page image로 다시 확인한다(§8.1). 각 chunk는 page identity(DB·owner = table·generation·kind 7·level 0), allocator entry(`Allocated(g, table, Overflow)`, incarnation = g)가 맞아야 한다. 하나라도 어긋나면 손상이며 chain을 일부만 반환하지 않는다.
- **소유가 바뀌는 그룹은 원자적이다.** chunk page의 소유자(§8.3)는 ChunkWrite(없음→pending), Forward(pending→leaf 행), OverflowCancel(pending→XTR1), CLR(leaf 행→Job chain), ChainRetire(Job chain→XTR1)에서만 바뀌고, 각 그룹 안에서 옛 소유자를 지우고 새 소유자를 쓴다.

<a id="costs"></a>
## 6. 비용과 A/C 계획

<a id="plan"></a>
### 6.1 A의 계획과 chunk 생산

A의 값 검사는 canonical 길이 L을 센다. L ≤ 2,048이면 지금과 같다. 2,049 ≤ L ≤ 131,072이면 overflow 계획이다.

- leaf 공간은 head 행 기준이다: `64 + 168 + 16 ≤ leaf 여유`. 모자라면 지금처럼 Capacity이고 A의 자동 split 호출자가 받는다. 이때 `split_row_live`에 넘기는 길이는 canonical L이 아니라 64(head 길이)다.
- digest: A는 값에서 canonical bytes를 차례로 만들어 SHA-256만 갱신하는 streaming 함수(`canonical_row_digest_borrowed`)로 (L, digest)를 얻는다. 128 KiB 사본을 만들지 않는다.
- chunk 생산: chunk i의 payload는 canonical bytes의 `[i·16,092, min(L, (i+1)·16,092))`다. A는 값에서 그 창만 쓰는 windowed encoder(`encode_canonical_window_borrowed`)로 CANDIDATE(16 KiB)에 쓴다. 두 함수는 기존 `encode_canonical_row_borrowed`와 같은 검산·오류 순서를 따르고, 같은 입력에서 전체 encoder의 bytes와 정확히 같아야 한다(OVF-F05).
- 그래서 A·C 어느 쪽도 128 KiB buffer를 새로 갖지 않는다. 읽기의 materialization은 caller output이다(§7.3). 뒤에서부터 쓰므로 A는 값 전체에 임의 접근해야 한다. INSERT의 값은 이미 caller memory의 `ValueRef`라 문제가 없지만, 이것은 streaming LOB 쓰기가 아니다(§9).

<a id="ledger"></a>
### 6.2 ledger와 Job 비용

W = 56,976(ChunkWrite), F = Forward(C=244) = `91,548+128h`, R = 55,528(ReleaseRole·RootFree 한 걸음), X = 59,056(ChainRetire 최대).

| 항 | inline INSERT | overflow INSERT |
| --- | --- | --- |
| Reserve | 57,336 | 57,336 |
| Tx row1(Reserved root의 남은 Forward) | `debt(1, 91,304+C+128h)` | `debt(1+k−written, (k−written)·W + F)` |
| Tx row0(root CLR) | `91,096+C+128h` | `91,340+128h` |
| Job cleanup(Reserved·Retained) | `(5, 311,608+2C+128h)` | `(5, 312,096+128h)`(= 같은 식, C=244) |
| CLR 뒤 남는 cleanup | `(4, 4R = 222,112)` | `(5, 4R + X = 281,168)` |
| ChainRetire 뒤 / Prune 뒤 | — / `(4, 4R)` | `(4, 4R)` / `(4, 4R)` |

- **ledger·descriptor 규칙.** `ledger.rs`의 `remaining()`은 row1을 `debt(job.forward_steps(), job.forward_wal())`로 계산한다. `validate_transaction`의 Reserved root 규칙은 `descriptor.chain.apply == job.last_step()`(inline·written 0이면 lease_lsn과 같다), `inventory.change_lsn == job.lease_lsn()`(그대로), `last_mutation_sequence == job.mutation()`(그대로)이다. `validate_admission`은 개수·terminal row·peak만 보므로 바뀌지 않는다. A의 Forward 준비(`row/insert/group.rs`)도 같은 두 식(chain.apply == last_step, row1 == `debt(forward_wal, forward_steps)`)으로 바꾼다.
- Reserve의 admission은 이 표의 Tx row1을 처음부터 k개 chunk로 계산한다. ChunkWrite마다 W와 한 걸음을 소비한다.
- OverflowCancel(≤ 60,864)은 Cancel처럼 Reserved root의 남은 Forward 몫에서 낸다. 그 몫은 언제나 F ≥ 91,548을 포함하므로 모자라지 않는다. 쓴 chunk의 W는 반환하지 않는다(이미 소비한 WAL).
- Job cleanup은 Prune 경로(`89,984+128h + 4R`)와 CLR 경로(`X + 4R`) 중 큰 것을 예약한다. 두 경로 모두 5걸음이다. Prune 경로가 크므로 inline INSERT 식에 C=244를 넣은 값과 같다. CLR은 미사용 Prune 몫 `30,928+128h`를 반환하고 `(5, 4R+X)`를 남긴다. ChainRetire는 실제 frame(b·a)만 Root consumed_wal에 더하고, Job의 다음 recipe `(4, 4R)`와의 차액이 반환된다.
- **보호 WAL.** 큰 행 하나의 Reserve는 처음부터 row1 `9W+F` = 604,460 B와 Job cleanup 312,224 B(h=1)를 보호한다. [자원 정본](dev3-resource-bounds.md)의 "WAL·owned·protected 가운데 가장 작은 여유가 신규 admission을 닫는다"에 이 몫이 그대로 들어가므로, 큰 행을 쓰는 거래가 있는 동안 다른 admission의 여유가 그만큼 준다. ChunkWrite·Forward가 걸음마다 그 몫을 소비한다.
- 최대 행(k=9)의 **행 root에 귀속되는 WAL 소계**는 Reserve 57,336 + 9W 512,784 + F 91,676 + Prune 90,112 + 4R 222,112 = 974,020 B다. 이 소계는 Begin·statement 제어·Commit/AbortEnd·AgingStart·TerminalFinish와 다른 owner의 잔여 의무를 제외하므로 거래 전체 WAL이나 Reserve admission 필요량이 아니다. 실제 admission은 §6.2의 Tx row0–row1뿐 아니라 기존 terminal·control·Aging 의무와 현재 frame도 합산한다. chunk 그룹마다 Root·Job whole-body Patch 32,688 B가 고정 부담이다(§9).

<a id="forward-check"></a>
### 6.3 Forward의 chain 검산

Forward의 producer와 C 검사는 효과 전에 chain을 앞에서부터 한 번 읽는다: `OverflowChainVerifier`(expected root = pending front, head = Job의 total과 A 계획의 digest, owner = table)에 chunk를 차례로 push하고, 같은 payload를 **streaming canonical row 검산기**(`CanonicalRowStream`: column 수·tag·길이·Bool·UTF-8을 chunk 경계를 넘어 검사하고 supplied schema의 type/nullability와 각 값 64 KiB를 확인해, 이어 붙인 bytes에 대한 `decode_canonical_row_with_schema`와 같은 판정·오류)에 넣는다. 각 chunk의 allocator entry도 대조한다(§5). 모두 끝까지 성공해야 Forward가 열린다. 이것이 inline의 "원 schema decode"를 대신한다. 실패는 첫 효과 전 Unchanged이고 A는 OverflowCancel→statement 실패로 끝낸다.

**읽는 원천과 거주.** 읽는 chunk page는 Forward의 write target set T 밖이다.

| 진입 | 원천 | 거주·pin |
| --- | --- | --- |
| live Forward·OverflowCancel·ChainRetire producer | 원 pool의 retention LOAD(기존 경로) | chunk 하나씩: LOAD→검산→pin 해제 뒤 다음 chunk. T의 page를 잡기 전에 끝내므로 ROW_PEAK(frames 9)를 넘지 않는다. 거주에 flush가 필요하면 `FlushRequired`로 돌아가 처음부터 다시 검산 |
| replay·live 검증(`history::validate_live`) | replay arena(첫 extent 64 page) | arena image를 빌려 읽음, pin 없음 |
| Base4 | layout의 page image | 같음 |
| B 읽기 | 원 pool의 read LOAD | §7.3 |

이 page들이 Forward까지 바뀌지 않는 이유는 §5 끝 문단과 같다.

<a id="paths"></a>
## 7. A/B/C 경로

<a id="executor"></a>
### 7.1 A 실행기

자동커밋(`autocommit_insert_live`)과 명시 거래(`RowTransaction`)의 INSERT 한 행은 다음 순서다.

1. preflight(§7.2, allocator 상태 관측) → A 값 검사(§6.1, L·digest 확정) → `reserve(plan)`의 Free page k개 검사 → Reserve. overflow 계획이면 Reserve가 overflow root를 만든다. 원 source·schema 검사 뒤 값 검사라는 오류 순서를 유지한다.
2. i = k−1 … 0마다 `producer_lane` → A가 창 i를 생산 → `prepare_chunk` → prepared-chunk의 `apply`(ChunkWrite, 갱신된 ReservedRow 반환). `FlushRequired`는 예약이 소유한 flush 진입으로 해소하고, Busy와 함께 같은 chunk를 재시도한다. 성공한 chunk만 written을 올리며 stale prepared cut을 다음 chunk에 쓰지 않는다(§7.2).
3. Forward(§6.3 포함) → statement 끝·commit은 지금과 같다.
4. 2나 3이 첫 효과 전 Unchanged로 끝나면(검산 실패·취소) written ≥ 1이면 OverflowCancel, 아니면 Cancel로 Reserved root를 지운 뒤 지금의 statement 실패 경로를 탄다. stage 뒤 실패는 지금처럼 Fatal·fence다.

A의 새 frame(chunk 반복, 계획 값)은 [F4 caller 상한](../evidence/module-independence-survey-20261026.md#f4-caller-budgets)(BEGIN 2,816·STATEMENT_START 3,584·COMMIT 3,024·ROLLBACK_CALLER 1,536·LIVE_ROW_CLEANUP 4,096 등)을 지켜야 한다. 지금 live 정리의 A drain은 4,082/4,096 B라 여유가 14 B다. ChainRetire를 drain에 넣는 일은 never-inline 하위 단계로 나누는 것을 전제로 한다. census는 단계마다 한다(§11). **상한이 맞지 않으면 상한을 올리지 않고 이 계약을 다시 검토한다**(08-1 검토와 같은 규칙).

<a id="c-entries"></a>
### 7.2 C 진입

- **값을 알기 전과 뒤의 용량 검사.** 지금 `preflight_row(table, statement, loan, …)`는 values·L을 받지 않고 A의 값 검사는 그 뒤 `validation_lane`에서 한다. 이 순서를 유지한다. preflight는 기존 source·schema·index0 검산과 함께 allocator-eligible Free 수와 Reserved 유무를 안정된 source cut에 관측한다. A가 L·digest를 확정한 뒤 `reserve(plan)`가 그 검산된 계획의 k와 Free 수를 비교하고 Reserved가 없는지 **첫 durable 효과 전에** 검사한다. 부족하면 typed `OverflowCapacity`(용량 부족 계열)로 거절한다. 지금 A의 `row_capacity()`는 `Unchanged(Prepare, Capacity)`를 모두 leaf split으로 보내므로, 이 새 원인은 그 분류와 구별한다. allocator 부족에는 split을 요청하지 않으며 WAL·Job·issuer를 전진시키지 않는다. leaf 공간 부족만 기존 split 경로로 보내고 길이는 64다. 잘못된 값과 allocator 부족이 함께 있으면 A 값 검사 오류가 먼저다.
- `reserve(plan)` 통과 뒤에는 Reserve와 Forward 사이에 split이 끼지 않고, 단일 MutationPort 아래 다른 writer가 없다. k개 물리 용량 검사는 이미 끝났으며 pool 상태(FlushRequired·Busy)만 같은 chunk에서 재시도한다. §5의 사후 System 예약 충돌은 방어적인 OrderMismatch·취소 경로로 남긴다.
- **chunk 적용의 소유 상태.** 기존 `PreparedRow::apply`는 최종 Forward용이며 성공 시 RowSlotReservation을 discharge하고 loan·gate·I/O lease를 반환한다. ChunkWrite에는 이를 그대로 쓰지 않는다. 별도 prepared-chunk 상태가 ReservedRow 전체를 소유하고 `apply -> Result<ReservedRow, RowOwnedError<Self>>`로 이어진다. 성공 뒤에도 같은 활성 예약·source admission·TerminalReadGate·RowIoLease·loan을 유지하고, checked durable ChunkWrite 결과로 draft commitment·descriptor chain.apply·last_step·pending·다음 target을 갱신한다. 기존의 "RowDraftCut은 예약 수명 동안 불변"은 **성공한 ChunkWrite 사이에서만** 바뀐다. stage 전 Unchanged/reopen은 그 시점의 같은 cut·written을 보존하고, stage 뒤 불명 실패와 활성 예약 Drop은 fence다. chunk 성공은 RowMutationObservation·statement private revision을 발행하지 않는다. 마지막 Forward 또는 Cancel/OverflowCancel만 예약을 discharge한다.
- **예약 중 flush.** RowIoLease가 flusher를 빌리므로 A가 원 MaintenancePort를 다시 빌려 flush할 수 없다. 현재 RowIoLease의 "행 종료 전 flush capture/eviction 금지"를 이 경계에 한해 개정한다. ReservedRow에 보유한 lease와 원 disjoint flusher를 쓰는 flush 진입을 공급하고 A의 sealed Reserved adapter에 연결한다. stage 전 prepared cut을 버리고 모든 LOAD/write guard를 놓은 그룹 사이에서만 열며, durable 예약·Drop fence·read gate를 유지한 채 원 control과 전용 flush owner/credit으로 flush한다. 성공하면 I/O 상태를 Ready로 복원하고 다음 준비가 source/draft를 재검산한다. stage/install 도중에는 열리지 않는다. chunk 준비·Forward 검산·OverflowCancel의 FlushRequired 모두 이 진입을 쓰며, flush 성공은 pending·written·draft의 논리 전이를 만들지 않는다. 재검산 때 resident bytes만 새로 읽고 stage 전 cut을 다시 확인한다. 임의 새 owner나 새 I/O session을 만들지 않는다.
- `ReservedRow::prepare_chunk`/`apply`: ChunkWrite의 효과. 신규 page 등록은 split의 신규 page 경로(원 pool의 next generation·claim·등록, mutable flush lease와 DWB idle 검사)를 Row 단계에서 쓰도록 일반화한다. 별도 I/O owner나 새 pool을 만들지 않는다. 각 ChunkWrite는 preflight에서 닫은 원 TerminalReadGate 안에서 실행하며 다시 획득하지 않는다(`plan4_apply/row/write.rs`).
- `ReservedRow::cancel`: written에 따라 Cancel 또는 OverflowCancel.
- live 정리(`row_cleanup_next`/`retire_row`)와 복구 cleanup은 chain이 남은 ReleasingCompensated root에서 ChainRetire를 먼저 고른다. jobs.rs의 maintenance 연산 목록과 `row_cleanup_next`의 선택 순서에 ChainRetire를 더한다.

<a id="read"></a>
### 7.3 B 읽기

B의 판정(stamp·Status·snapshot)은 inline 행과 같다. 보이는 Overflow 행의 복사만 다르다.

- output이 total보다 작으면 chunk를 읽기 전에 `OutputTooSmall { required: total }`이다.
- C의 row capture(`plan4_apply/row/read.rs`)는 지금 Inline이 아닌 행을 `(0, deleted)`로 처리한다. 이 분기를 Overflow·Deleted·Inline 셋으로 나눈다. Overflow면 head를 기록하고 leaf latch를 놓은 뒤 head.root부터 chunk를 차례로 LOAD해 `OverflowChainVerifier`로 검산하며 `output[i·16,092..]`에 복사하고, 끝나면 `decode_materialized_row_with_schema(head, output[..total], schema)`(Catalog schema 입력을 받는 변형, O1)로 total·digest·schema를 확인한다. LOAD에 flush가 필요하면 지금의 `Step` 흐름으로 돌아가고 B가 같은 행을 다시 복사한다. 검산 실패는 손상이며 Missing으로 낮추지 않는다.
- **복사 단계의 재개 seam(O3).** 현재 C `RowReadOperation::copy_candidate`와 B `ReadSource::copy_candidate`는 각각 `Result<RowCopyObservation>`·`Result<Copied>`여서 LOAD의 FlushRequired를 전달할 수 없다. C는 기존 `ReadStep<RowCopyObservation>`을 반환하고 LOAD에 필요한 residency 입력을 받는다. B trait는 `Result<Step<Copied, Target>, RetentionError>`로 바꾸며 LiveSource·LocatedSource와 `copy_selected`·`select_row`가 같은 Step을 전달한다. `copied_slot`은 전체 chain의 digest·schema 검산까지 성공한 Ready에서만 채운다. inline 복사는 Ready로 감싸며 `copy_before`는 이번에 LOAD를 추가하지 않는다.
- FlushRequired·오류·취소 모두 candidate 해제와 operation `finish`를 거쳐 Txn/U pin·reference lease·source admission·read gate를 반환한 뒤 호출자에게 전달한다. 정리 실패는 기존 오류 우선순위를 따른다. 호출자는 flush 후 **같은 행의 head.root부터** 다시 읽는다. [F02 출력·cursor 규칙](dev3-task-06-tree-rows.md#module-statement-cursor)대로 실패의 출력 bytes는 scratch(변경 가능), 유효 길이는 0이며 Found/Row를 발행하지 않는다. cursor의 last_returned는 그대로, last_examined는 앞서 판정 완료한 invisible prefix까지만 유지하며 복사 미완료인 visible 행은 포함하지 않는다. 출력 부족은 기존처럼 chunk LOAD 전 거절한다.
- 08-3에서 reader가 읽는 chain은 반환되지 않는다(§7.4). 그래서 chunk를 읽는 동안 별도 transient pin을 두지 않는다. 08-4에서 commit된 chain을 반환할 때 그 pin 규칙을 정한다.
- point 읽기의 locator([08-2](dev3-cursor-locator.md))는 leaf를 찾을 뿐이므로 바뀌지 않는다.

<a id="no-reader-race"></a>
### 7.4 반환과 reader가 겹치지 않는 이유

반환은 두 경우뿐이다. (1) OverflowCancel의 chain은 어느 leaf 행도 가리키지 않는다. (2) ChainRetire의 chain은 CLR로 지워진 미확정 INSERT 행의 것이었다. 다른 거래는 그 행을 Missing으로 판정하므로 chain을 읽지 않는다. 같은 거래의 읽기는 같은 session의 호출이고, 자기 변경 뒤 cursor는 `CursorInvalidated`이며, ChainRetire는 그 거래가 끝난 뒤에만 열린다. 게다가 TerminalReadGate가 Row write와 live 정리 동안 닫혀 있다. 그래서 08-3의 반환은 읽는 중인 chain을 건드리지 않는다.

<a id="inline-gates"></a>
### 7.5 inline 전용 지점의 분류

지금 "Deleted가 아니면 Inline"으로 가정하는 제품 지점을 다음처럼 바꾼다. Overflow payload의 INSERT 행은 효과·stamp·undo가 inline INSERT 행과 같으므로, 아래에서 "INSERT 행으로 받음"은 inline 행과 같은 판정에 C=244를 쓰는 것이다.

| 지점 | 지금 | 2판 |
| --- | --- | --- |
| `semantic_row/leaf.rs` `inline`·`validate_rows`(Row 그룹의 모든 leaf before·after) | Inline 아니면 Unsupported | Overflow 행 받음(head decode). 그래야 Forward(overflow) after와, 큰 행과 같은 leaf의 다른 행 변경이 통과한다 |
| `semantic_row/structure/weights.rs`(split weight) | Overflow는 Unsupported | entry bytes(244+4)로 weight |
| `wal_io/plan4_retention/visibility/pins.rs`(B capture 판정) | Overflow 행이 있으면 bad | Overflow 행 받음 |
| `wal_io/plan4_apply/row/read.rs`(row capture 복사) | Inline 아니면 삭제로 처리 | §7.3의 세 갈래. **빠뜨리면 큰 행이 조용히 Missing이 되므로 OVF-J01이 직접 본다** |
| `plan4_recovery_base/row/actor.rs`·`graph.rs`(Base4 행 판정) | Inline 또는 tombstone만 | Overflow INSERT 행 받음, chain은 §8.3 |
| `semantic_row/transition/prepare.rs`(Forward·CLR·Prune producer의 행 bytes·projection) | inline canonical로 record·XRP1 | overflow root는 head record와 root-zero XRP1(§4.2) |
| DELETE·UPDATE의 대상 분류(`row_preflight.rs`·`row_write.rs`의 target class, `semantic_row/undo.rs`·`undo_record.rs`의 before) | Inline만 대상 | Overflow 행 대상은 지금처럼 Unsupported(08-4) |
| A `row/insert.rs`·`insert/leaf.rs`·`insert/group.rs`, C `structural_live.rs`의 길이 범위 | `5..=2048` | overflow 계획 길이 2,049..131,072를 따로 받음(§6.1). split 호출 길이는 64 |

O2·O3의 D2는 비시험 코드의 `RowPayload::`·`INLINE_ROW_BYTES`·`5..=2048`·`185..=2228` 사용처 전체를 이 표의 갈래로 분류한 목록을 함께 검토한다(2판 작성 때 grep으로 센 사용처: 약 60곳, 이 표의 파일들).

<a id="smo-closure"></a>
### 7.6 split·unlink 검산의 kind 7 소유

지금 split·unlink candidate의 `audit`(`semantic_row/structure/candidate.rs`)는 table 소유 Allocated page 가운데 anchor가 아니고 visited에도 없는 page가 모두 retired 목록에 있어야 한다고 본다. commit된 큰 행의 chunk는 visited도 retired도 아니므로 지금 그대로면 그 table의 다음 split·unlink가 막힌다. 2판은 tree 소유와 chunk 소유를 나눈다.

- **tree 소유.** `reached ⊎ retired_tree == owned_tree`. owned_tree는 kind RowLeaf·RowInternal인 소유 page다. `retired_pages`와 `reached`는 tree 항목만 decode한다.
- **chunk 소유(개수 closure, chunk LOAD 없음).** table 소유 kind 7 page 수 = (visited leaf의 모든 Overflow 행 head.count 합) + (그 table의 chain이 있는 ReleasingCompensated root의 ceil(total/16,092) 합) + (XTR1 Overflow 항목 수). pending chain 항은 없다: `audit`는 어느 actor든 RootReserved Job이 있으면 OrderMismatch로 거절하므로(`candidate.rs`), Reserved overflow root가 있는 동안 SMO는 열리지 않는다. XTR1 Overflow 항목은 allocator로만 검사한다(owner·kind 7·incarnation·visited 아님). leaf 행은 audit가 weight를 세며 이미 decode하므로 추가 LOAD가 없다.
- 이 closure는 개수만 본다. 각 chain의 page identity는 Forward·OverflowCancel·ChainRetire의 걸음과 Base4(§8.3)가 본다. SMO는 kind 7 page를 고르지도 바꾸지도 않으므로(신규 page는 Free에서만 고른다) 이 나눔이 tree closure를 약하게 하지 않는다.
- unlink는 XTR1에 tree 항목을 더할 때 기존 Overflow 항목을 보존한다(`extend`의 물리 순서 병합). 쓴 목록은 §4.4의 kind별 규칙으로 다시 검산한다.
- 비용: audit에 행당 비교 몇 번과 u64 계수 몇 개가 는다. C lifecycle census(44,304 중 여유 605)와 preflight C(여유 182)에 들어가므로 O2 census로 확인한다(§11).

<a id="recovery"></a>
## 8. 복구

<a id="replay"></a>
### 8.1 replay와 live 검증

`history::validate_live`와 복구 replay는 세 opcode를 지금 Row 그룹처럼 다시 계산한다.

- **ChunkWrite**는 allocator entry(가장 낮은 allocator-eligible Free였는지 포함)·chunk INIT image·Job pending을 대조하고, 그 page의 **INIT claim**(352 B, role 7, §8.3)을 replay claims에 쓴다. claim을 만드는 것은 Row history(`history/row.rs`)의 새 함수 `overflow_init_record`다. Catalog CREATE의 `init_record`와는 header(`[0,128)`: role·source kind·page·INIT LSN·writer epoch·frame digest·initial digest)와 source `[128,176)`(`Transaction(actor)`), `@176`(= root lease_lsn)만 공유하고 `[184,256)`은 role 7 형식이다. `init_record`는 role 선택과 pair 조건 때문에 role 7을 만들 수 없다. Catalog CREATE처럼 INIT 전에 그 page의 arena slot과 claim이 zero여야 한다(`history/catalog.rs`의 claim 기록 조건).
- **Forward(overflow)**는 arena의 chunk image로 §6.3의 검산을 다시 한다.
- **OverflowCancel·ChainRetire**는 arena의 chunk image로 next·index를 다시 따라가 XTR1 after와 같은지 본다. Row 그룹이 anchor page를 처음 쓰므로 Row history의 anchor image 대조(지금은 proof page로 읽기만)가 anchor after를 받는다.
- 반환한 chunk의 arena image와 INIT claim은 지우지 않는다. page가 Allocated로 남으므로 Base4는 그 provenance를 계속 요구한다(retired tree page가 Structural claim을 유지하는 것과 같다).

<a id="loser"></a>
### 8.2 loser·cleanup

- Reserved overflow root를 가진 loser: written ≥ 1이면 OverflowCancel, 아니면 지금의 CancelReserved.
- Retained overflow root를 가진 loser: 지금의 loser CLR. 그 CLR이 Job에 반환할 chain을 남긴다.
- 복구 cleanup은 chain이 남은 ReleasingCompensated root에서 ChainRetire부터 한다. winner의 Prune은 지금과 같다.

<a id="base4"></a>
### 8.3 Base4

Base4는 형식을 바꾼다. 1판의 "record·certificate 형식은 바꾸지 않는다"는 틀렸다.

- **certificate.** `decode_certificate`의 dynamic role(ordinal ≥ 9)에 `Overflow`(level 0, object ≥ 6)를 더한다. `encode_certificate`도 같은 role을 쓴다.
- **provenance role 7.** `validate_provenance`에 role 7(source kind 2, `TxnRef`)을 더한다: `[128,176)` actor, `@176` root lease_lsn(< INIT LSN), `[184,208)` table object·generation 1·schema epoch 1, `[208,232)` table anchor(dynamic, chunk와 다른 주소), `[232,234)` chunk index, `[234,236)` count, `[236,240)` total, `[240,256)` zero. index·count·total은 chunk page의 cell과 같아야 하고 kind는 Overflow다.
- **role 7을 CREATE pair로 읽는 지금의 소비자.** 지금 Base4는 source kind 2(Transaction)인 claim을 모두 CREATE pair(role 3|4)로 읽는다. role 7을 더하면 다음 네 곳을 함께 바꾼다(O1 codec, O2 검산).
  1. **accessor.** `InitProvenance::pair()`는 role 3|4에서만 `Some`이다(지금은 source kind 2이면 `[232,256)`을 `expect`로 decode하므로 role 7에서 panic한다). role 7은 `chunk()`(anchor·index·count·total) accessor를 따로 둔다. `object()`는 그대로다(`[184,208)`이 같다).
  2. **`validate_pairs`**(`plan4_recovery_base/row.rs`). role 7을 짝 검사에서 건너뛴다. 대신 role 7의 anchor가 그 table의 CREATE role 3 claim의 page이고 `object()`가 그 claim의 object와 같은지 본다. source는 CREATE 거래가 아니라 큰 행을 넣은 거래이므로 CREATE claim의 source와 비교하지 않는다. source kind 2이고 actor가 다른 Transaction source claim과 같은 범위 규칙(`within_source`) 안이면 된다.
  3. **`relations::provenance`**(`catalog/graph/relations.rs`). 지금은 claim에서 canonical 초기 image(anchor 또는 빈 RowLeaf)를 다시 encode해 `initial_digest`와 비교한다. chunk의 INIT image에는 payload가 있어 claim만으로 다시 만들 수 없다. role 7은 다시 encode하지 않고 **certificate pageLSN == `init_lsn`**과 **`content_digest(현재 image) == initial_digest`**를 요구한다. 08-3에서 chunk page는 INIT 뒤 어느 그룹도 쓰지 않으므로(§5) 현재 image가 INIT image다. 14-2가 page를 재사용하면 이 규칙이 바뀐다(§13).
  4. **`relations::payloads`.** ordinal 9..63 certificate의 kind 분기에 Overflow를 더한다(지금은 `_ => Unsupported`). owner = table object·generation 1, level 0만 보고, 소유 검산은 아래 `chunks` closure가 맡는다.
- **allocator incarnation 상계.** `relations::validate`는 각 entry의 `incarnation − 1 ≤ System HW + creates + Structural HW`를 요구한다. ChunkWrite 할당은 이 합에 없으므로 **모든 table의 kind 7 Allocated page 수**를 합에 더한다. 08-3에서 chunk page는 Free로 돌아가지 않으므로(반환은 XTR1, 실제 Free는 14-2) 지금까지의 ChunkWrite 할당 수가 그 page 수와 같다. 그 page가 전에 다른 용도로 할당·해제된 적이 있어도(예: rollback된 CREATE의 page) 그 할당은 원래 항에 이미 들어 있다(`creates`는 취소된 CREATE도 센다). 14-2가 Overflow page를 Free로 돌리면 이 항을 다시 정한다(§13).
- **현재 tree 검산**([current-tree-closure](dev3-tree-smo.md#current-tree-closure))을 kind별로 나눈다. table 소유 kind 7 page는 tree 도달 집합 밖의 별도 집합 `chunks`이고, 각 page의 claim은 role 7·anchor = 그 table의 anchor여야 한다. retired 순회는 kind 7 항목이면 `inspect_overflow_page`로 읽고 `retire_lsn ≤ anchor_lsn`·`pageLSN < retire_lsn`을 보며 Structural high-water와 비교하지 않는다. closure는 `reached ⊎ retired_tree == owned_tree`와 `chain_owned ⊎ retired_overflow == chunks`다.
- **chain_owned**는 다음 셋의 서로소 합이다. 각 page는 정확히 한 번이어야 한다.
  1. reached leaf에 있는 **모든** Overflow 행(미확정·Prune 대기 포함): head.root에서 k개 chunk, `OverflowChainVerifier` 전체 검산(digest 포함).
  2. Reserved overflow root의 pending: front에서 `OverflowSegmentWalk`로 written개, 마지막의 next는 zero.
  3. chain이 있는 ReleasingCompensated root: root에서 k개(`OverflowSegmentWalk`, total = Job의 total).
- 어느 소유자에도 없거나 둘 이상에 있는 page, 소유자가 가리키는데 kind·owner·generation·incarnation이 다른 page, claim이 없거나 role·anchor가 다른 page는 손상이다.

<a id="scope-002"></a>
## 9. 목표 등급(SCOPE-002)·A2와의 관계

| 항목 | 이 설계 |
| --- | --- |
| 주소 폭 | head·next·XTR1·Job chain이 전체 PageRef(FileId·page_no·incarnation 각 u64)를 쓴다. 64 TiB 이상 주소를 막지 않는다. 첫 extent 제한은 profile 한도(12-1) |
| 값 크기 | head는 total u32·count u16이라 형식은 약 1 GiB(65,535 × 16,092 B)까지 표현한다. 계약 상한 128 KiB는 그대로다. A·C 어디에도 행 크기의 buffer가 없다. 그러나 상한을 올리려면 형식이 바뀐다: verifier의 방문 집합은 k개 배열, 반환은 한 그룹이 chain 전체를 걷고 "일부만 반환하지 않는다"이며 Job에 반환 진행 cursor가 없고, XTR1은 anchor cell 하나·첫 extent 55 항목에 묶여 확장되지 않는다. 큰 값을 위해서는 반환 cursor·여러 목록 page·streaming 읽기 API가 필요하다 |
| 쓰기 방식 | 뒤에서부터 쓰므로 값 전체에 임의 접근해야 한다. INSERT 값이 caller memory에 있는 지금은 맞지만 streaming LOB 쓰기(PG의 toast 증분 쓰기)는 아니다 |
| 거래 크기 | 거래당 상태는 Job 하나의 고정 칸이다. 행 수·chain 수에 비례하는 메모리 구조를 만들지 않는다 |
| 비용 구조 | chunk의 payload는 INIT redo에 한 번만 실린다(native에는 header만). 그룹마다 Root·Job whole-body Patch 32,688 B가 붙는 것은 지금 Row 그룹 공통의 whole-page WAL 문제이고 A2(redo 형식)가 다룬다. A2가 Root·Job을 바뀐 bytes만 쓰게 바꾸면 chunk 그룹은 거의 payload 크기가 된다. 이 설계는 whole-page Patch에 기대지 않는다 |
| 동시성 | 모든 ChunkWrite가 allocator page 하나를 patch한다. A4가 writer를 병렬화할 때 이 page는 공유 지점이다(PostgreSQL의 relation extension lock과 비슷한 자리). 또 기존 Row write는 preflight부터 최종 Forward/Cancel까지 TerminalReadGate를 쥔다. §7.2의 chunk continuation도 이를 유지하므로 큰 행 하나의 Reserve + k ChunkWrite + Forward(최대 11개 durable 그룹)와 그 사이 flush·검산 동안 새 읽기를 연속해서 막는다. 그룹 사이 gate를 풀었다가 다시 얻는 설계는 아니다. 08-3은 단일 MutationPort라 정확성 영향은 없지만, A4의 동시 읽기·쓰기 목표에서는 이 gate 범위를 다시 정해야 한다 |
| 반환 | 실제 Free·재사용은 14-2다. 그 전까지 반환한 chunk page는 첫 extent의 dynamic page를 계속 차지한다. reserve(plan)의 allocator 용량 판정(§7.2)이 쓰는 도중의 부족을 막지만, rollback·검산 실패로 반환한 page는 14-2까지 남는다 |

<a id="tests"></a>
## 10. 시험

기대값은 독립 상수, 독립 Python vector, 거래가 쓴 값에서 정한 상태 기계에서 온다. producer가 다시 읽은 값을 기대값으로 쓰지 않는다. 바뀐 코드의 시험만 실행한다.

| ID | 단계 | 사건과 판정 |
| --- | --- | --- |
| OVF-F01 | O1 | chunk 형식 개정: 독립 Python으로 다시 계산한 page literal·full-page SHA, root 칸 nonzero 거절, verifier의 방문 주소 집합(뒤 chunk가 root 주소, next가 root 주소), `inspect_overflow_page`·`OverflowSegmentWalk`(부분 chain, 순서·중복·마지막 next·chain 불일치) |
| OVF-F02 | O1 | Job3 overflow root: §4.3 표의 네 state 조합, Reserved pending(front zero ⇔ written 0, written ≤ k, total 범위)과 last_step 규칙(written 0·≥1), ReleasingCompensated chain(anchor·root·total, cursor 0, last_step, ChainRetire 뒤 `> release_start`), `@285` 1의 효과·state·scope 조합 거절, ReleasingCommitted `@285` 1 거절. `forward_wal`·`forward_steps`·`cleanup` 값 |
| OVF-F03 | O1 | XTR1 Overflow 항목: 정상, kind 7 level ≠ 0, retire LSN 규칙(kind별), **revision 1 anchor의 Overflow 전용 목록 정상·tree 항목이 섞이면 revision 1 거절**, n = 55 경계 |
| OVF-F04 | O1 | 세 opcode의 shape·frame bytes(ChunkWrite 56,976, b·a 경계의 OverflowCancel 56,472+b+a·ChainRetire 54,664+b+a), native kind 6·10·12의 header 칸, 두 새 redo 형식, wire decode의 anchor native 사전 scan 손상, role·inventory action, 옛 opcode 거절 |
| OVF-F05 | O1 | A의 `canonical_row_digest_borrowed`·`encode_canonical_window_borrowed`가 전체 encoder bytes·SHA와 같음(2,049/16,092/16,093/32,184/32,185/131,072, 여러 열·경계에 걸친 값, 오류 순서), `CanonicalRowStream`이 chunk 경계를 가리지 않고 decoder와 같은 판정(모든 byte 손상·잘림·schema 변형) |
| OVF-F06 | O1 | Base4 codec: certificate의 Overflow role(level 0, object ≥ 6) 정상·손상, provenance role 7의 각 칸(actor·`@176 <` INIT LSN·object·anchor·index < count·count = ceil(total/16,092)·total 범위·`[240,256)` zero) 정상·손상, `pair()`가 role 7에서 None이고 `chunk()`가 값을 돌려줌(role 3·4의 `pair()`는 그대로), 그 밖의 role·source kind 조합 거절 |
| OVF-F07 | O1 | 새 codec 함수(`inspect_overflow_page`·`OverflowSegmentWalk`·digest·window·`CanonicalRowStream`·`decode_materialized_row_with_schema`)와 바뀐 `OverflowChainVerifier::push`의 source 모델 값 기록(`assert_eq!`로 고정). O2·O3가 첫 호출자의 census에 더한다 |
| OVF-C01 | O2 | ChunkWrite 전이: allocator(가장 낮은 eligible)·INIT·pending·descriptor(chain.apply·row1)·Root, index·next·payload 길이·front·digest 위반 거절 |
| OVF-C02 | O2 | Forward(overflow): written < k, root·total 불일치, chain 손상(digest·schema·순서·중복 주소·allocator entry) 거절, 정상 전이(descriptor chain.apply == last_step) |
| OVF-C03 | O2 | OverflowCancel·ChainRetire: XTR1 after(revision 1·2 anchor 모두), 손상 chain 거절(일부 반환 없음), ReleaseRole이 chain 뒤에만, Root canceled·consumed_wal·job cursor |
| OVF-C04 | O2 | replay가 위 그룹을 같은 결과로 다시 계산(`overflow_init_record`의 INIT claim 포함, INIT 전 arena slot·claim이 zero가 아니면 거절)하고, 바뀐 frame을 거절 |
| OVF-C05 | O2 | Base4: role 7 claim이 있는 base의 decode 전 과정(`validate_pairs`·`provenance`·`payloads`·incarnation 상계·current tree) 정상과 각 단계의 손상(claim 없는 kind 7, role 7 anchor가 CREATE claim과 다름, role 7 claim의 index·count·total이 chunk cell과 다름, pageLSN ≠ init_lsn, 현재 image digest 불일치), CREATE와 다른 거래가 쓴 chunk의 정상, 네 소유자 각각, 소유자 없는 page, 두 소유자, 다른 incarnation, retired Overflow 항목의 pageLSN ≥ retire LSN |
| OVF-C06 | O2 | split·unlink 검산(§7.6): 큰 행·보상 chain·XTR1 Overflow가 있는 table의 정상 split·unlink, Reserved overflow root가 있으면 audit 거절, 개수가 하나 많거나 적으면 거절, Overflow 항목이 있는 XTR1 위의 unlink 목록 병합 |
| OVF-J01 | O3 | 자동커밋 INSERT 2,049·16,092·16,093·131,072 B 행. 같은 거래와 새 statement의 point·cursor 읽기 값이 쓴 값과 같음(조용한 Missing이 없음), allocator·chunk 수 |
| OVF-J02 | O3 | 명시 거래: 큰 행·작은 행 섞어 넣고 statement rollback(큰 행 CLR, **거래가 끝난 뒤 drain의 ChainRetire**), full rollback, commit 뒤 Prune(chain 유지)과 drain. split한 적 없는(revision 1) table과 split한 table 둘 다 |
| OVF-J03 | O3 | Free 수 k−1/k, allocator Reserved, invalid value와 부족 동시 입력의 오류 순서. allocator 부족은 OverflowCapacity·효과0·split0, leaf 부족만 길이64 split. 각 chunk 앞/뒤 FlushRequired·Busy에서 같은 활성 예약·cut·written을 보존/갱신하며 중복 할당 없음, stage 뒤 실패·활성 Drop fence. Forward 검산·취소의 예약 중 flush. written ≥ 1 취소 → OverflowCancel, written 0 → Cancel |
| OVF-J04 | O3 | Forward의 chain 검산 실패(시험 전용 producer가 창 하나를 바꿈) → OverflowCancel, 효과 없음. Forward 검산 도중 FlushRequired |
| OVF-J05 | O3 | 다른 거래가 미확정 큰 행을 Missing으로 봄(chunk를 읽지 않음), commit 뒤 snapshot 규칙대로 보임. 출력 부족 `OutputTooSmall{total}`. B 읽기의 chain 손상(T06-10 대응)은 손상 오류이고 Missing이 아님. point·cursor 각각 첫/중간/마지막 chunk LOAD의 FlushRequired 및 최종 digest/schema 실패: 출력 유효0, pin·gate 반환, invisible prefix 보존·visible 행 건너뛰기 없음, flush 후 같은 행을 한 번만 반환. inline Ready 회귀 |
| OVF-J06 | O3 | commit된 큰 행이 있는 table의 다음 split·unlink, split 뒤 읽기, 큰 행과 같은 leaf의 작은 행 INSERT·DELETE·UPDATE, Overflow 항목이 있는 XTR1 위의 unlink와 ChainRetire 두 순서. DELETE·UPDATE 대상이 큰 행이면 Unsupported |
| OVF-J07 | O3 | census: 새 C 진입·Forward 검산·B 복사·drain의 frame이 기존 상한 안 |
| OVF-R01 | O3 | 재시작: loser Reserved(written 0·중간·k), loser Retained, winner, ChainRetire 전 crash. 복구 뒤 Base4·XTR1·읽기 |
| OVF-N01..N06 | O4 | native fault cut: ChunkWrite(첫·중간·마지막) 앞뒤, Forward(overflow) 앞뒤, CLR 뒤 ChainRetire 앞뒤, OverflowCancel 앞뒤. 각 사례 produce·recover·reenter 세 process, 독립 기대 상태 |

<a id="stages"></a>
## 11. 단계와 적립

| 단계 | 내용 | census | 비고 |
| --- | --- | --- | --- |
| O0 | 이 설계 | — | 문서만. 일정 08-3 = 진행(적립 0) |
| O1 | 형식·순수 codec(§4, §6.1의 A helper·`CanonicalRowStream`, Base4 certificate·provenance codec)과 OVF-F | 새 codec 함수의 frame | 제품 경로는 아직 overflow를 거절 |
| O2 | C 전이·ledger·descriptor·replay·Base4·split/unlink closure(OVF-C) | C lifecycle·preflight C·live retirement·CLR_PRODUCER·Row write 지점 | 제품 경로는 아직 거절 |
| O3 | 제품 경로: A 실행기·C 진입·B 읽기·live 정리·복구(OVF-J·R) | F4 caller 상한 전부, B read census, A drain | 큰 행 INSERT가 처음 성공 |
| O4 | native fault cut(OVF-N) | — | **08-3 적립(2일)은 O4의 병합** |

각 단계는 PR 하나와 독립 D2이며, 필수 지적을 해소한 뒤 연다. census가 상한을 넘으면 상한을 올리지 않고 이 계약을 다시 검토한다(§7.1). 08-3은 08의 마지막 항목이 아니므로 항목 조건으로 적립한다. 08-1이 U4의 병합으로 적립된 것과 같다.

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
| chain 반환에 anchor revision+1 | revision은 route 변화이고 Base4가 `≤ 1 + Structural high-water`로 묶는다. Row 그룹은 Structural high-water를 올리지 않으므로 맞지 않는다(§4.4) |
| split·unlink 검산이 모든 chain을 따라감 | chunk 최대 55장을 SMO마다 LOAD해야 하고 census가 tight한 경로에 들어간다. 개수 closure로 두고 page identity는 Base4가 본다(§7.6) |

<a id="supersedes"></a>
## 13. 바꾸는 정본 문장

이 설계가 구현되면 다음 정본의 문장이 바뀐다. 각 단계 PR이 해당 문서를 함께 고친다.

| 정본 | 지금 | 바뀌는 것(단계) |
| --- | --- | --- |
| [overflow 표현](dev3-overflow-format.md) | chunk `@24..48` root, index0·root 주소의 국소 검사 | reserved zero, 검사는 verifier, 새 literal(O1) |
| [tree-smo XTR1](dev3-tree-smo.md#empty-leaf-unlink) | 목록이 있으면 revision ≥ 2, RowLeaf·RowInternal만 | tree 항목이 있을 때만 revision ≥ 2, Overflow 7 level 0과 kind별 retire LSN(O1) |
| [tree-smo current-tree-closure](dev3-tree-smo.md#current-tree-closure)와 SMO 검산 | `reached ⊎ retired == owned` | tree·chunk 소유를 나눈 두 closure(O2) |
| [module-consumers row-shape·scope3 wire](dev3-module-consumers.md#row-shape) | 20 opcode, Row native kind 1·3·5·8·9, redo는 whole-body Patch뿐, 행은 inline 5..2048 | opcode 14·15·25, native kind 6·10·12, allocator entry patch·INIT redo, overflow 행(O1·O2) |
| [자원 정본](dev3-resource-bounds.md) | Row WAL recipe | ChunkWrite·OverflowCancel·ChainRetire와 row1 걸음(O2) |
| [행 owner §5.4](dev3-task-06-tree-rows.md#algorithms) transient pin | overflow 읽기 pin | 08-3은 반환 chain을 reader가 읽지 않으므로 pin 없음, 규칙은 08-4(O3) |
| Base4 certificate·provenance | kind 7 dynamic page 없음, source kind 2 claim은 모두 CREATE pair | Overflow role, provenance role 7, `pair()`는 role 3·4만, `validate_pairs`·`provenance`·`payloads`의 role 7 분기(O1 codec, O2 검산). role 7의 "현재 image == INIT image" 규칙은 page를 재사용하지 않는 동안만이고 14-2가 다시 정한다 |
| Base4 allocator incarnation 상계 | `System HW + creates + Structural HW` | kind 7 Allocated page 수를 더함(O2). 14-2가 Overflow page를 Free로 돌리면 다시 정한다 |
| `jobs.rs` census의 TerminalLeased 연산 목록 | Reserve·Forward·Cancel·Commit·AbortEnd·Clr | ChunkWrite·OverflowCancel 추가, ChainRetire는 TerminalReady(O2) |
| 효과 추론 `op_effect`·`opcode_effect` | `op != op.role()`이면 DELETE | 명시적 `Operation::delete()`(O1) |
| [module-boundaries Row read/write](dev3-module-boundaries.md#row-write-validate-once)와 B driver seam | copy는 Step 없음; PreparedRow apply는 예약 discharge; RowDraftCut 불변 | copy의 ReadStep·재시도, prepared-chunk의 활성 예약 반환·성공 후 cut 갱신·예약 중 flush, allocator 부족의 split 분류 분리(O3). 원 수명·오류·F02 출력 규칙은 유지 |

<a id="o1-record"></a>
## 14. O1 구현 기록

2026-11-07. 형식·순수 codec만 연다(§11). 제품 경로는 overflow 행을 지금처럼 첫 효과 전에 거절하고, 원 replay dispatch(`plan4_retention/history/row.rs`)는 opcode 14·15·25를 Unsupported로, Row retirement 관측은 None으로 둔다. Base4의 지금 소비자는 role 7 claim·Overflow certificate를 ProvenanceMismatch·Unsupported로 거절한다(panic 없음). 그래서 사용자에게 보이는 동작은 바뀌지 않는다.

| 범위 | 구현 | 시험 |
| --- | --- | --- |
| §4.1 chunk 개정 | `overflow.rs`: `OverflowChain(total, digest)`, cell·page encoder/decoder가 chain을 받고 `[24,48)` zero(아니면 Corrupt), root 검사를 `OverflowChainVerifier`(expected root·방문 집합)로 옮김, `inspect_overflow_page`, `OverflowSegmentWalk`. `OverflowHead::new`·`count`는 본문을 그대로 두어 행 decode census의 frame이 같다 | OVF-F01: 고정 8개(새 literal)와 `ovf_f01_inspect_and_segment_walk`. literal은 독립 [`overflow_literal.py`](../../module-gate/evidence/dev3-08-3-o1/overflow_literal.py)가 개정 전 literal로 자신을 검산한 뒤 계산 |
| §4.3 Job3 root | `retention_page/row/job.rs`: `PendingChain`·`ReturnChain`, `JobView::{payload, overflow, pending, chain, forward_wal, forward_steps, cleanup_started}`, `cleanup()`의 chain 분기 `(5, 4R+X)`, decode의 `@285` 조합, `validate_root`의 Reserved pending·Releasing chain 규칙. `census.rs`의 두 규칙과 `semantic_row/ledger.rs`의 ReleasingCompensated 규칙이 `cleanup_started()`를 쓴다 | OVF-F02 3개와 ledger의 `ovf_f02_ledger_returned_chain_needs_the_terminal`(fixture의 cursor 0 보상 root를 chain을 반환한 overflow root로 고치면 Active 곁에서 OrderMismatch, Committed·Aborted 곁에서 통과, chain을 쥔 root는 Active 곁에서 통과. 옛 식 `release_cursor() != 0`이면 실패한다) |
| §4.4 XTR1 | `anchor.rs`: `(Overflow, 0)` 항목, revision ≥ 2는 tree 항목이 있을 때만 | OVF-F03 |
| §4.5 opcode | `semantic_row/operation.rs`: 14·15·25, `role()`(OverflowCancel → Cancel), `delete()`·`list()`, `shape_list`, `list_from_native`, `Shape::list_sides`(b·a를 entry 수 u8 둘로 `Shape`의 padding에 둔다, 40 B 그대로). `wire.rs`: list op의 anchor native 사전 scan, native kind 6·10(slot 1)·12, kind 6 entry Patch와 kind 12 INIT redo. `core.rs`: ChunkWrite planned·refs 1, ChainRetire refs 1·cursor < 64. `op_effect`·`opcode_effect`가 `Operation::delete()`를 쓴다 | OVF-F04 5개(`size_of::<Shape>() == 40`, Job의 `CHUNK_WRITE_WAL`·`CHAIN_RETIRE_WAL`이 shape 값과 같음, `graph::inventory`의 action: OverflowCancel 3, ChunkWrite·ChainRetire InvalidInput 포함). 옛 미지 opcode 시험은 18·26으로 옮김 |
| §6.1 A helper | `row_record/stream.rs`: `canonical_row_digest_borrowed`, `encode_canonical_window_borrowed`, `CanonicalRowStream`. `decode_materialized_row_with_schema`(Catalog schema) | OVF-F05 8개(Catalog schema의 stream과 materialize 포함) |
| §8.3 Base4 codec | `image_records.rs`: certificate의 Overflow role(Profile3, object ≥ 6), provenance role 7, `pair()`는 role 3·4만, `chunk()` | OVF-F06 3개 |

census(§11의 O1 열 "새 codec 함수의 frame"). 새 지역값이 생긴 함수 넷의 source 모델에 항을 더했다: XTR1 walk의 tree-entry flag(`anchor.rs` `walk_source_bytes`, +1 B), `validate_root`의 overflow root 칸(`retention_page/row.rs` `source_bytes`), `decode_layout`의 native closure와 list 크기(`transition/source.rs` `layout_decode`, +64 B), provenance role 7 분기(`image_records.rs` `provenance_source_bytes`, +152 B). census 출력의 차이는 이 항뿐이다(UNL-C01 walk 408→409·check_list 586→587, N1 frame callee 4,688→4,752, Base4 layout 20,232→20,384·catalog layout 19,592→19,744·grammar bound 2,252→2,404). peak·charge·상한(Row lifecycle 40,883, C 44,304, Base4 peak 59,675 등)은 바뀌지 않는다. 새 codec 함수 자체의 frame은 각 모듈의 `source` 모델(`overflow::source`, `row_record::source`, `row_record::stream::source`)로 두고 OVF-F07이 기존 page codec 938·canonical check 762·SHA 1,536 위에서 값을 찍는다: `inspect_overflow_page` 1,506, `OverflowSegmentWalk` 상태 352·push 249, `OverflowChainVerifier::push` 2,057, `decode_materialized_row_with_schema` 2,080(`&[Column]` 판 2,336), `canonical_row_input_len` 309, digest 2,064, window 621, `CanonicalRowStream` 상태 200·push 312 B. SHA closure를 포함하는 값은 SHA를 부르는 셋(verifier push, materialize 두 판, digest)이고, 그 SHA 분기는 기존 census의 digest 항(입력 또는 receiver, `ContentHasher` 112 B, 32 B digest, SHA)과 같다(`overflow::source::{content_digest, hasher_digest}`). `inspect_overflow_page`의 page codec 938에는 page digest의 hash state와 출력만 있고 SHA closure는 없다. 그래서 `inspect` 1,506도 SHA를 포함하지 않으며, 호출자 census가 기존처럼 SHA를 한 번 더한다. `OverflowSegmentWalk::new`·`CanonicalRowStream::new`처럼 `Result<Self>`를 돌려주는 생성자는 기존 `returned::<T, E>` 규칙대로 호출자의 모델이 그 Result를 센다. OVF-F07은 이 값들을 `assert_eq!`로 고정하므로 모델이나 항이 바뀌면 시험과 이 기록을 함께 고친다. O1에서 이 함수를 부르는 제품 경로는 없으므로 어느 census에도 아직 들어가지 않는다. O2·O3가 첫 호출자의 census에 이 값을 더한다(§11). census 계열 시험 327개 중 322개 PASS, 남은 5개는 vector·case 선택 입력(`XADB_XRB_INCLUDE`, `XADB_G6_VECTOR_INDEXES`, `XADB_PLAN4_G5_CAPACITY_PADDING`)이 있어야 도는 하네스 시험이다.

정본 문장(§13의 O1 행): [overflow 표현](dev3-overflow-format.md)·[tree-smo XTR1](dev3-tree-smo.md#empty-leaf-unlink)·[module-consumers row-shape·scope3 wire](dev3-module-consumers.md#row-shape)·[Base4 provenance](dev3-create-recovery-base.md)를 개정했다. `op_effect`·`opcode_effect`는 코드 규칙이라 코드에서 바꿨다. O2 행(current-tree closure, 자원 정본, incarnation 상계, `jobs.rs` TerminalLeased 목록)은 O2에서 바꾼다.

O2에 남긴 것(독립 D2 1회차가 짚은 것 포함):

- role 7 claim의 index·count·total과 chunk cell의 대조. page image가 필요하므로 Base4 layout의 role 7 분기(현재 image 규칙과 함께)에서 한다. 그때까지 Base4 소비자는 role 7을 거절한다.
- `semantic_row/ledger.rs`의 Reserved root row1은 아직 `debt(1, forward_wal())`이다. overflow root에서는 `debt(forward_steps(), forward_wal())`로 바꾼다(§6.2).
- XTR1 Overflow 항목의 kind별 retire LSN 규칙. codec은 두 kind에 같은 `1..=anchor_lsn`을 쓰므로 OVF-F03은 그것만 본다. Structural high-water 비교를 tree 항목에만 두는 것과 `chunk pageLSN < retire LSN`은 page를 읽는 Base4·SMO 검산(OVF-C05·C06)이다.
- `JobView::cleanup()`이 부르는 `chain()`의 지역값(`Option<ReturnChain>`, PageRef decode Result)은 `validate_budget`·census `verify_terminal`·ledger observe의 모델에 따로 항이 없고 지금은 합의 여유가 덮는다. 이 세 경로의 census를 O2에서 다시 볼 때 항으로 넣는다.
- replay가 14·15·25를 `RetentionCause::Unsupported`로 거절하는 것(O1 전에는 `RowSemantic(Unsupported)`)과 role 7 claim이 있는 base의 decode 전체는 각각 OVF-C04·C05의 시험에서 연다.

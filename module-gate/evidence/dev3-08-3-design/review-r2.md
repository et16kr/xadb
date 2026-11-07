Verdict: CONDITIONAL

# D2 독립 검토 2회차 — 08-3 overflow chain 쓰기·읽기·반환 설계(O0 2판)

## 대상

- branch `claude/project-thread-y6amas`, commit `f6fbcf9`(`c2efbc4` 위 수정 commit)
- 범위는 `git diff c2efbc4 f6fbcf9`이다.
  - `devdoc/contracts/dev3-row-overflow.md` 2판(391행)
  - `contracts/README.md` 상태 줄, `document-validation.json` 해시
  - `module-gate/evidence/dev3-08-3-design/review-r1.md`·`frames-r1.py` 추가
- 판정 요약:
  - 1회차 필수 R1·R4·R5는 해소됐다. R2·R3은 대부분 해소됐지만 각각 잔여가 하나씩 있다(아래 N1·N2).
  - N1은 형식에 관한 것이다. 2판이 정한 provenance role 7을 지금 Base4의 다른 소비자 네 곳이 CREATE pair로 읽는다. 이대로면 큰 행 하나가 있는 base는 decode 도중 panic하거나 거절된다.
  - N2는 §4.3 표의 칸 하나가 Job codec과 다른 것이다.
  - 권고 8건과 NIT 2건은 수용을 막지 않는다.

## 검토 방법

- **1회차 항목 대조.** 2판 전체를 읽고, 1회차 필수 R1–R5·권고 O1–O11·NIT N1–N3를 각 절과 대조했다.
- **보관본 확인.** `review-r1.md`·`frames-r1.py`가 1회차 산출물과 byte 단위로 같다(`diff`로 확인).
- **등록 확인.** README·`document-validation.json`의 해시가 두 문서의 실제 SHA-256과 같다.
- **새 규칙을 코드와 다시 대조했다**(읽기만).
  - **Job3 codec·관측기.** `retention_page/row/job.rs`의 accessor 위치(`projection_digest` = `[288,320)`, `pair`, `sender_digest`)와 decode·Reserved·Releasing 규칙, `validate_budget`.
  - **jobs.rs.** `observe`, `census`, `unchanged`(바꿀 수 있는 구간 목록), `context`.
  - **ledger.rs.** `remaining`, `validate_transaction`, `validate_admission`.
  - **transition.** `prepare.rs`의 `descriptor_after`(1152–1188)·Root 갱신(2095–2150)·`op_effect`, `graph.rs`의 `inventory`·edge 분기, `operation.rs`의 `role`·`handoff`·`outer`·opcode 표.
  - **wire.rs.** `validate_native_shape`의 kind·slot·offset 규칙. Catalog frame의 native kind 표(`semantic_catalog/frame.rs:25–37`).
  - **anchor.rs.** `RetiredList::extend`·`check_retired`·`encode_listed_anchor`.
  - **split·unlink audit.** `structure/candidate.rs`의 `audit`(274–400, RootReserved 거절 포함)·Free 선택(530–555), `structure/weights.rs`.
  - **replay claim.** `history/catalog.rs`의 `init_record`(187–266)·claim 기록·해제(600–695).
  - **Base4 provenance·certificate.** `plan4_recovery_base/image_records.rs`(`InitProvenance`·`decode_certificate`·`validate_provenance`), `plan4_recovery_base/row.rs`의 `validate_pairs`(294–318).
  - **Base4 그래프.** `plan4_recovery_base/catalog/graph/relations.rs`의 `payloads`·`validate`·`provenance`, `plan4_recovery_base/catalog/graph/ledger.rs`, `plan4_recovery_base/row/{graph,actor,current_tree}.rs`.
  - **allocator.** `wal_io/allocation_reservation/operation.rs`의 `validate_base_allocation`.
  - **그 밖에.** `retention_page/row/census.rs`, `row_start/retire.rs`, `row/insert/group.rs`(780–797 주석), `row_preflight.rs`, `overflow_tests.rs`의 literal.
- **비시험 inline 가정 지점.** `RowPayload::`·`INLINE_ROW_BYTES`·`5..=2048`·`185..=2228` 사용처를 grep으로 다시 셌다. 20개 파일, 82줄이다.
- **수치 재계산.** 독립 script `/tmp/claude-0/-home-user-xadb/4a1e9b70-6d6f-5c82-8466-00f9910fc966/scratchpad/frames-r2.py`로 다시 셌다.
- `/home/user/xadb`와 `/mnt/project-files`는 바꾸지 않았다. build·시험은 돌리지 않았다.

## 판정

- **열린 필수 2**(N1·N2, 둘 다 1회차 항목의 잔여)
- **권고 8**(P1–P8), **NIT 2**(Q1·Q2)
- N1·N2를 고친 수정본에 3회차 D2가 필요하다. 수정 범위는 §4.3 표 한 칸과 §8.3의 Base4 소비자 목록·규칙이다.

## 다시 계산한 수치

모두 문서와 같다.

- **ChunkWrite.** W(I=0) = 3,776 + 1,152 + 2,176 + 320 + 256 + 2×16,344 + 160 + 16,448 = **56,976**이다. I=1이면 57,184다.
- **9W** = **512,784**
- **OverflowCancel.** `56,472+b+a`, 최대 **60,864**
- **ChainRetire.** `54,664+b+a`, 최대 X = **59,056**
- **Forward·CLR·Prune.**
  - F = `91,548+128h`
  - CLR = `91,340+128h`
  - Prune = `89,984+128h`
- **Job cleanup.** Prune 경로는 `89,984+4R` = **312,096**이다. 이것은 inline 식 `311,608+2C`에 C = 244를 넣은 값과 같다.
- **CLR 경로.** 4R+X = **281,168**이다. 그래서 반환은 **30,928**이다. 걸음 수는 Prune 1 + 4, ChainRetire 1 + 4로 둘 다 5다. **(5, 4R+X)**가 맞다.
- **ChainRetire 뒤.** cursor 0이므로 `cleanup()`은 (4, 4R)이다.
- **k=9 commit 경로**(h=1). 57,336 + 512,784 + 91,676 + 90,112 + 222,112 = **974,020 B**다. 이는 약 0.97 MB(10진)이고 0.93 MiB다.
- **OverflowCancel의 지불.** 60,864 ≤ F이므로 Reserved root의 남은 Forward 몫 안에 든다.
- **§12의 대안.** CLR+anchor는 약 112 KB로 98,304를 넘는다. chunk 둘을 한 그룹에 넣으면 73,680으로 약 74 KB다.

## 1회차 항목 상태

### R1 (XTR1 revision) — 해소

- §4.4 109행은 "목록에 tree 항목이 하나라도 있을 때만 revision ≥ 2"로 정했다. 변경 지점(`check_retired`·`encode_listed_anchor`)과 tree-smo 1421행의 대체(§13 386행)를 적었다.
- revision을 올리는 대안은 거절 근거와 함께 §12 375행에 있다. 근거는 `current_tree.rs:130`이다.
- revision 1 목록 위의 첫 unlink는 `RetiredList::extend`의 물리 순서 병합으로 Overflow 항목을 보존한다. 코드를 확인했다(`anchor.rs:176–211`). kind를 보지 않고 old 항목을 그대로 옮긴다.
- retire LSN 규칙도 kind별로 나눴다(111행). Base4 retired 순회의 분기는 §8.3 303행에 있다.
- 시험은 OVF-F03(revision 1 Overflow 전용 목록, 섞이면 거절, n = 55)과 OVF-J02(revision 1·2 table)에 있다.

### R2 (Job3·ledger·descriptor LSN) — 대부분 해소, 잔여 1건(N2)

- **해소된 것**
  - **Reserved `last_step`.** written 0이면 `lease_lsn`, 그 밖에는 마지막 ChunkWrite LSN이다(95행). 관측기 `after.last_step == lsn`(`jobs.rs:123–128`)과 맞는다.
  - **descriptor.** `chain.apply == job.last_step()`이고, `inventory.change_lsn == lease_lsn`은 그대로다(187행).
    - `validate_transaction`의 `change_lsn ≤ chain.apply`(`ledger.rs:305–306`)와 `chain.apply ≥ latest_step`(310행)도 함께 성립한다.
    - Base4 `row/graph.rs:473`의 inventory item도 `change_lsn == lease_lsn`을 요구하는데, 그대로 성립한다.
  - **ledger row1.** `debt(forward_steps, forward_wal)`이다.
    - descriptor의 remaining은 `transaction_remaining`으로 다시 계산되고(`prepare.rs:1182–1188`), ledger가 정확히 대조한다(`ledger.rs:326`).
    - 그래서 W·1걸음씩 줄어드는 식과 맞는다.
  - **Job budget.** `validate_budget`(`job.rs:520–558`)이 budget 칸을 `cleanup()`과 정확히 맞춘다. CLR·ChainRetire의 바꿀 수 있는 구간에 `(376,1024)`가 있어 `(5,4R+X)`에서 `(4,4R)`로 가는 것도 표현된다.
  - **ChainRetire의 cursor 규칙.** chain이 있으면 cursor 0이고 `last_step == release_start`, ChainRetire 뒤에는 `> release_start`다(95·98행).
  - **`@285`.** Releasing에서의 값을 정했다(90행). Prune이 0으로 쓰는데, Prune의 바꿀 수 있는 구간 `(285,320)` 안이다.
  - **anchor ref.** ReleasingCompensated `[200,224)`에 남긴다(1회차 O3).
- **잔여.** N2. `[288,292)` 칸의 Reserved·Retained 값이 Job codec과 다르다.

### R3 (Base4 형식) — 일부 해소, 잔여 1건(N1)

- **해소된 것**
  - 1판 문장 "형식은 바꾸지 않는다"를 철회했다(299행).
  - certificate dynamic role에 Overflow를 더했다(301행).
  - `validate_provenance` role 7을 정했다(302행). 공통 검사인 `@176 < @48`, `raw[256..]` zero, `[184,208)` object와 맞는다.
  - current tree를 kind별 두 closure로 나눴다(303행).
  - retired 순회를 kind 7로 분기했다.
  - replay가 claim을 쓰고, 반환 뒤에도 claim을 유지한다(284·287행).
- **잔여.** N1. role 7을 받는 곳만 정했고, 모든 Transaction source claim을 CREATE pair로 읽는 지금의 소비자 네 곳을 빠뜨렸다.

### R4 (split/unlink audit와 inline 지점) — 해소

- **§7.5 표**(252–261행)는 1회차가 든 지점을 모두 담았다. `leaf.rs`·`weights.rs`·`pins.rs`·`read.rs`(조용한 Missing 경고 포함)·`actor.rs`·`graph.rs`가 그것이다. 여기에 prepare producer, DELETE·UPDATE 대상, A·C 길이 범위를 더했다.
  - grep으로 다시 센 82줄·20개 파일 가운데 표에 없는 파일이 있다. `operation.rs`·`transition.rs`·`job.rs`(185..=2228, C = 244가 범위 안), `row_record.rs`·`overflow.rs`(codec 자신)다. 모두 바꿀 필요가 없는 곳이다.
  - 전체 분류 목록을 O2·O3 D2에 함께 내도록 한 것(263행)도 적절하다.
- **§7.6.** tree closure와 kind 7 개수 closure로 나눴다.
  - leaf 행은 `weights::derive`가 이미 decode하므로(`candidate.rs:340–360`의 `weights::derive`) 추가 LOAD가 없다는 서술이 맞다.
  - Overflow 항목은 allocator로만 본다.
  - SMO는 Free에서만 page를 고르므로(`candidate.rs:540–555`) chunk를 바꾸지 않는다.
- **시험.** OVF-C06, OVF-J06이 이를 본다. 작은 흠은 P3에 적는다.

### R5 (opcode 정의) — 해소

- OverflowCancel은 `56,472+b+a`이고 role Cancel이다.
  - `inventory()` action 3(`graph.rs:176–182`)이 이 role에서 나온다.
  - descriptor의 Forward|Cancel 분기(`prepare.rs:1163–1177`)와 jobs 관측기의 Cancel 규칙(`jobs.rs:83·158–160`, role로 분기)에도 들어간다.
  - Root canceled는 `match op`(`prepare.rs:2109·2127`)이므로 "Cancel 분기에 OverflowCancel을 더한다"가 정확하다.
- ChunkWrite는 I=0, 56,976이다.
- Root·descriptor 효과는 표로 정했다(130–134행). ChainRetire는 non-handoff이므로 consumed_wal과 `@72`가 자동으로 붙는다(`prepare.rs:2135–2150`).
- native kind 6·10·12는 Catalog frame의 번호(`semantic_catalog/frame.rs:25–37`: Allocation 6, TreeAnchor 10, RowLeaf 11)와 겹치지 않는다. Row mask(u16, `1 << kind`)에도 들어간다.

### 1회차 권고·NIT 상태

| 항목 | 상태 | 2판 위치 |
| --- | --- | --- |
| O1 census 단계화·상한 규칙 | 반영 | §7.1 222행, §11 354–362행 |
| O2 읽기 원천·거주, allocator entry 대조 | 반영 | §6.3 196–207행, §5 157행 |
| O3 ChainRetire의 anchor | 반영 | §4.3 92·99행 |
| O4 ChainRetire는 terminal 뒤 | 반영 | §5 156행, OVF-J02. 실제로는 jobs.rs census의 기본 분기가 강제한다(P1) |
| O5 preflight Capacity | 반영 | §7.2 227행. 선택된 Free Undo를 쓰는 지금의 preflight(`row_preflight.rs:592`)와 단일 MutationPort(`&mut RetentionLedger`) 아래서 성립한다 |
| O6 digest 구분 | 반영 | §4.2 79–83행 |
| O7 Base4 소유자 "모든 Overflow 행" | 반영 | §8.3 305행, §5 158행 |
| O8 시험 | 반영 | OVF-F03, J03–J06, R01 |
| O9 §9 정직성 | 반영 | 316–321행 |
| O10 대체 목록 | 반영 | §13 |
| O11 Root 효과 | 반영 | §4.5 표 |
| N1 native header 칸 | 반영. 다만 anchor slot 규칙이 새로 필요하다(P2) | 125행 |
| N2 `decode_materialized_row` | 반영 | 238행, O1 변형 |
| N3 kind 번호·opcode 번호 | 반영 | 124–125행 |

## 필수 (열린 항목)

### N1 (R3 잔여). role 7 claim을 지금 Base4의 다른 소비자 네 곳이 CREATE pair로 읽는다 — 큰 행이 있는 base는 decode 도중 panic하거나 거절된다

**문서**
- §8.3 301–308행
- §8.1 284행 "형식은 Catalog CREATE의 `init_record`와 같다"

**근거**

1. **`InitProvenance`의 accessor.**
   - `pair()`는 source kind가 2이기만 하면 `[208,232)`와 `[232,256)`을 PageRef로 `expect`-decode한다(`image_records.rs:94–101`). `object()`도 같은 조건이다(87–93).
   - 2판 role 7은 source kind 2이고, `[232,240)`에 index·count·total, `[240,256)`에 zero를 둔다(302행).
   - 이 24 B를 PageRef로 읽으면 incarnation이 0이다. 그래서 decode가 실패하고 `expect("validated RowRoot")`가 panic한다.
2. **`validate_pairs`.**
   - Row base layout decode(`plan4_recovery_base/row.rs:235`)는 `validate_pairs`(294–318)를 부른다.
   - 이 함수는 `pair()`가 있는 모든 claim에 대해 짝 claim을 찾는다. 짝은 `pair[role==3]`의 claim이고, 다른 role, 같은 pair, 같은 init LSN·frame digest를 가져야 한다.
   - role 7은 짝이 없으므로 ProvenanceMismatch다. 위 1이 먼저 panic하지 않는다면 그렇다.
3. **`relations::provenance`.**
   - Row Base4(`row/graph.rs:788`)가 부르는 `relations::provenance`(`catalog/graph/relations.rs:586–668`)는 Transaction source claim마다 `pair()`를 꺼낸다(645).
   - 그다음 role 3이면 anchor를, 그 밖이면 빈 RowLeaf를 canonical 초기 image로 다시 encode한다(646–662).
   - 그리고 `initial_digest`와 비교한다(665).
   - chunk INIT image에는 payload가 있어 claim만으로 다시 만들 수 없다. 그래서 role 7은 반드시 ProvenanceMismatch다.
4. **`payloads`.**
   - 같은 Base4 경로의 `relations::validate`(478, Row에서 `row/graph.rs:789`)는 `payloads`(342–473)를 부른다.
   - `payloads`는 ordinal 9..63의 모든 certificate를 kind로 분기한다(356). 그 밖의 kind는 `_ => Unsupported`다(472).
   - §8.3이 certificate role에 Overflow를 더해도, 이 pass가 kind 7 page를 Unsupported로 거절한다.
5. **`init_record`.**
   - 문서는 형식이 `init_record`와 "같다"고 한다(284행).
   - 그런데 `init_record`(`history/catalog.rs:187–266`)의 role 선택에는 Overflow가 없다(209–215).
   - Transaction 분기도 page가 root pair의 일원이어야 하고(243–249), `[208,256)`에 pair를 쓴다(256–262).
   - 그래서 role 7은 이 함수로 만들 수 없다. header 부분만 같고 `[208,256)`은 다른 형식이다.

**고칠 것**
- §8.3에 role 7과 이 네 소비자의 관계를 정한다. 예를 들면 다음과 같다.
  - **accessor.** `pair()`는 role 3|4에서만 `Some`이다. role 7에는 `chunk()`(anchor·index·count·total) accessor를 따로 둔다. `object()`는 그대로 쓴다.
  - **`validate_pairs`.** role 7을 건너뛴다. 대신 role 7의 anchor가 그 table의 CREATE role 3 claim의 page인지 본다.
  - **`relations::provenance`의 role 7 규칙.** 초기 image를 다시 encode하지 않는다. chunk page는 INIT 뒤 바뀌지 않으므로 다음을 요구하면 된다.
    - certificate pageLSN == `init_lsn`
    - `content_digest(현재 image) == initial_digest`
    - 08-3에서는 재사용이 없으므로 성립한다. 14-2가 재사용하면 이 규칙이 바뀐다는 것을 §13에 적는다.
  - **`payloads`의 Overflow 분기.** owner가 table object·generation, level 0이어야 한다. 소유 검산은 §8.3의 `chunks` closure에 맡긴다.
  - **새 claim 생성 함수.** `init_record`와 header 부분만 공유한다고 고쳐 쓴다. INIT 전 arena와 claim이 zero여야 한다는 조건을 둔다(Catalog의 `history/catalog.rs:620–627`과 같은 조건).
- 시험:
  - OVF-C05에 "role 7 claim이 있는 base의 decode 전 과정(`validate_pairs`·`provenance`·`payloads`·current tree) 정상"과 각 단계의 손상을 넣는다.
  - O1에는 `InitProvenance` accessor 시험을 둔다.

### N2 (R2 잔여). §4.3 표가 Reserved·Retained의 `[288,292)`을 zero로 적었는데, 그 자리는 XRP1 projection digest다

**문서**
- 93행 "`[288,292)` | zero | zero | **chain total** | zero"
- 이 표를 그대로 시험하는 OVF-F02 331행("§4.3 표의 네 state 조합")

**근거**
- `job.rs:246–247`의 `projection_digest()`는 `bytes[288..320]`이다.
- Reserve 관측기는 `after.projection_digest() == &plan.projection`을 요구한다(`jobs.rs:146`).
- §4.2 81행도 값 commitment를 "Reserve의 Job `C`와 XRP1 projection"이라고 한다.
- 그러므로 Reserved·Retained의 `[288,292)`는 SHA-256의 앞 4 B이고 zero가 아니다.
- 표대로 codec을 만들면 모든 overflow Reserved root가 Corrupt가 된다. 표대로 시험을 쓰면 기대값이 틀린다.
- CLR은 바꿀 수 있는 구간 `(285,320)`(`jobs.rs:207`)이 있으므로 Releasing에서 `[288,292)`에 total을 쓰고 `[292,320)`을 zero로 둘 수 있다. 이 부분은 맞다.

**고칠 것**
- 표의 Reserved·Retained 칸을 "projection digest `[288,320)`(지금처럼)"으로 고친다.
- ReleasingCompensated 칸에는 "CLR이 projection을 지우고 `[288,292)` total, `[292,320)` zero"를 적는다.
- 97행의 Releasing zero 규칙 문장도 이 칸을 기준으로 다시 확인한다.

## 권고

- **P1. jobs.rs census의 op 목록을 명시한다.**
  - `census`(`jobs.rs:340–361`)는 Reserve·Forward·Cancel·Commit·AbortEnd·Clr에만 TerminalLeased를 허용하고, 나머지는 TerminalReady를 요구한다.
  - 따라서 ChunkWrite는 이 목록에 더해야 한다(`reserved == 1`도 함께). 그렇지 않으면 모든 ChunkWrite가 거절된다.
  - ChainRetire의 "terminal 뒤"는 바로 이 기본 분기가 강제한다. 이 점을 §5 156행에 적는다.
  - 또 `ledger.rs:155–158`과 `retention_page/row/census.rs:200–205`는 "정리 걸음이 시작됐다"를 `release_cursor != 0`로만 판정한다. ChainRetire를 마친 root(cursor 0, 반환된 chain)도 같은 판정에 넣으면 불변식이 더 정확해진다.
- **P2. kind 10 native의 slot 규칙.**
  - `validate_native_shape`는 kind 1·9 밖의 모든 native에 slot 0을 요구한다(`wire.rs:444`의 `_ => slot_value == 0`).
  - 2판은 anchor native가 XTR1 cell, 곧 anchor slot 1이라고 한다(125행). 그러므로 kind 10의 slot 1 분기를 wire 규칙에 더한다고 적는다.
- **P3. §7.6의 Reserved 항은 SMO에서 언제나 0이다.**
  - `audit`는 어느 actor든 RootReserved Job이 있으면 OrderMismatch다(`candidate.rs:375–377`).
  - 그래서 OVF-C06(340행)의 "pending이 있는 table의 정상 split·unlink"는 만들 수 없는 사례다.
  - 항을 지우거나 "항상 0(audit가 Reserved를 거절)"으로 적는다. 시험은 거절 사례로 바꾼다.
- **P4. `role()`이 자기 자신이 아닌 비-DELETE·UPDATE opcode가 처음 생긴다.**
  - `operation.rs:85–87` 주석은 "every other operation is itself"라고 한다.
  - `op_effect`(`prepare.rs:131–139`)와 `opcode_effect`(`undo.rs:319–327`, `jobs.rs:82`에서 무조건 계산)는 `op != op.role()`을 DELETE로 추론한다.
  - 지금은 Forward·Clr·Prune role에서만 쓰여 해가 없지만 함정이다. 명시적 `delete()` 판정으로 바꾸고 주석을 고친다고 적는다.
- **P5. Base4의 allocator incarnation 상계.**
  - `relations.rs:520–533`은 `incarnation − 1 ≤ System HW + creates + Structural HW`를 요구한다. ChunkWrite 할당은 이 합에 들지 않는다.
  - 08-3에서는 page마다 ChunkWrite가 최대 한 번(재사용 없음)이고 table CREATE가 적어도 한 번 있으므로 성립한다고 보인다.
  - 그 근거를 §8.3에 적고, 14-2가 이 상계를 바꿔야 한다는 것을 §13에 남긴다.
- **P6. ChunkWrite는 allocator에 Reserved entry가 있으면 열지 않는다.**
  - split audit는 이미 그렇게 한다(`candidate.rs:310–312`).
  - 또 Base4의 System batch 검산은 allocator pageLSN을 batch birth로 쓴다(`allocation_reservation/operation.rs:720–737`). 진행 중인 System 예약 사이에 Row 그룹이 allocator를 patch하면 그 규칙이 깨진다.
  - §5 153행의 eligibility에 이 조건을 적는다.
- **P7. 보호 WAL 크기를 §6.2나 §9에 적는다.**
  - 큰 행 하나의 Reserve는 row1 `9W+F` ≈ 604 KB와 Job cleanup 312 KB를 Reserve 시점부터 보호한다.
  - resource-bounds 217행은 "WAL/owned/protected 가운데 가장 작은 여유가 신규 admission을 닫는다"고 한다.
  - 동시 admission 계산에 이 몫이 들어간다는 것을 적는다.
- **P8. §8.1 284행의 claim 생성.** N1의 다섯째 근거와 같은 내용이다. 새 함수, 그것이 header만 공유한다는 것, INIT 전 arena·claim zero 조건을 §8.1 본문에 둔다.

## NIT

- **Q1.** 187행은 "`validate_transaction`과 `validate_admission`의 Reserved root 규칙"이라고 한다. 그런데 `validate_admission`(`ledger.rs:340–368`)에는 Reserved root의 chain 규칙이 없다. 개수·terminal row·peak만 본다. `validate_transaction`(316–322)으로 좁힌다.
- **Q2.** ChunkWrite의 바꿀 수 있는 구간 `(376,1024)`(102행)는 `(376,384)`로 충분하다. ChunkWrite는 `cleanup()`을 바꾸지 않는다. 지금도 `validate_budget`이 budget을 고정하므로 해는 없다.

## 건전하다고 본 것 (2회차에 새로 확인)

- **ChunkWrite의 page 선택.**
  - FlushRequired·Busy에서 다른 page를 고르지 않고 같은 chunk를 다시 부른다(153행).
  - eligibility는 allocator만으로 정해지므로 replay가 "가장 낮은 eligible Free"를 다시 확인할 수 있다.
  - preflight를 통과한 뒤 Capacity가 생기지 않는다는 서술도 단일 MutationPort(`plan4_apply.rs:516–526`의 `&mut RetentionLedger`) 아래서 성립한다.
- **XTR1 병합.** `extend`가 기존 항목을 kind와 무관하게 물리 순서로 보존한다. 2판 109·273행의 서술과 같다.
- **ChainRetire의 시점.** jobs.rs census가 TerminalReady를 요구하므로 statement rollback 직후에는 열리지 않는다. 그래서 §7.4의 reader 비경합 논거가 더 강해졌다.
- **인용 확인.** `row/insert/group.rs:791–792`의 "no split separates a Reserve from its Forward" 주석은 실제로 있다. 개정 전 overflow literal `1f97d1d7…`·`cd849926…`도 `overflow_tests.rs:132·136`에 있다.
- **범위·단계·적립.** 1회차와 같다. O4 병합으로 적립하고, 일정 08-3은 진행(적립 0)이다.

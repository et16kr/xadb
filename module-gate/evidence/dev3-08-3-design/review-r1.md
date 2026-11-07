Verdict: CONDITIONAL

# D2 독립 검토 1회차 — 08-3 overflow chain 쓰기·읽기·반환 설계(O0)

## 대상

- branch `claude/project-thread-y6amas`, commit `c2efbc4`. 기준은 `origin/main` `5f70d2b`이다.
- 범위는 `git diff 5f70d2b c2efbc4` 전체다.
  - 새 계약 `devdoc/contracts/dev3-row-overflow.md`(D3-ROW-OVERFLOW-001, 305행)
  - `contracts/README.md` 등록, `documents.json`·`document-validation.json` 등록과 해시
  - `dev3-schedule-baseline.md`의 08-3 행을 "진행(적립0)"으로 바꾼 것
- 판정 요약:
  - 방향은 요구와 맞다. 뒤에서부터 쓰는 chunk(A4), 반환을 Allocated 유지·XTR1 등록으로 한정한 것, 08-4·14-2와 나눈 범위가 그렇다.
  - 다만 문서가 "그대로"라고 한 기존 불변식 몇 개가 실제 코드에서는 새 전이를 거절한다. XTR1 revision, Job3·ledger LSN 규칙, Base4 certificate·provenance, split/unlink audit와 leaf 관측기가 그렇다.
  - opcode 표에는 산술 오류 1건이 있다.
  - 구현(O1) 전에 아래 필수 5건을 설계에서 정해야 한다.

## 검토 방법

- **읽은 문서**
  - 대상 문서 전체.
  - `dev3-overflow-format.md`
  - `dev3-tree-smo.md`: XTR1 1180–1215, current-tree-closure 1322–1330, 1421, census 1470–1500·1795–1830
  - `dev3-resource-bounds.md` 140–224
  - `dev3-module-consumers.md` 549–553·613
  - `dev3-task-06-tree-rows.md` 28·128–133·165·199–200
  - `dev3-cursor-locator.md` C3·C4
  - `project-direction.md` SCOPE-002 31–71
  - `dev3-schedule-baseline.md` 24–177
  - 앞선 검토 예: `module-gate/evidence/dev3-08-1-unlink-design/review.md`
- **읽은 코드**(읽기만 했다):
  - `storage/overflow.rs`, `row_record.rs`
  - `semantic_row/{operation,wire,jobs,ledger,leaf}.rs`
  - `semantic_row/transition/{prepare,graph,cells}.rs`
  - `semantic_row/structure/{candidate,weights}.rs`
  - `retention_page/row/job.rs`, `retention_page/row.rs`, `anchor.rs`, `allocation_page.rs`, `page.rs`
  - `row/insert.rs`, `row/insert/group.rs`
  - `plan4_recovery_base/{image_records,row}.rs`
  - `plan4_recovery_base/row/{current_tree,actor,graph}.rs`
  - `wal_io/plan4_retention/history/{catalog,row}.rs`, `history.rs`(validate_live)
  - `wal_io/plan4_apply.rs`(RowLoanLayout)
  - `plan4_apply/row/{write,read,split}.rs`
  - `plan4_retention/{structural_live,structural_unlink,row_start/retire}.rs`
  - `visibility/pins.rs`, `transaction/concurrency/read.rs`
  - `wal_io/row_caller_budget.rs`, `driver/live.rs`
- **다시 계산한 것.** §4.5·§6.2의 frame과 ledger 수치를 독립 script(`/tmp/claude-0/-home-user-xadb/4a1e9b70-6d6f-5c82-8466-00f9910fc966/scratchpad/frames.py`)로 다시 셌다. 쓴 식은 `operation.rs`의 `frame_bytes_cells`(XHG 3,776 / XSG 2,176 + native 128+before+after + 보조 edge 128·count 96·item 208 + redo)이다. 기존 opcode의 값은 다음과 같이 코드 상수와 맞았다.
  - Reserve 57,336, Cancel 40,000, R 55,528
  - Forward `91,304+C+128h`, CLR `91,096+C+128h`, Prune `89,496+2C+128h`
  - 새 opcode의 결과는 필수 R5에 적는다.
- **등록 확인.** README·documents.json·document-validation.json의 등록과 해시가 문서와 맞다.
- `/home/user/xadb`와 `/mnt/project-files`는 바꾸지 않았다. 설계 검토라서 build·시험은 돌리지 않았다. 아래 수치는 문서와 source에서 읽은 값이며 측정값이 아니다.

## 판정

- **필수 5**(R1–R5), **권고 11**(O1–O11), **NIT 3**(N1–N3)
- 필수를 해소한 수정본에 2회차 D2가 필요하다.

## 필수

### R1. XTR1 Overflow 항목은 revision 1 anchor에서 거절된다 — 한 번도 split되지 않은 table에서 큰 행의 취소·rollback이 막힌다

**문서**
- §4.4 95행: "항목 형식(40 B)과 순서·주소 규칙…은 그대로다."
- 99행: "Overflow 항목을 넣어도 anchor root revision은 바뀌지 않는다."

**근거**
- `anchor.rs:234`(`check_retired`)는 목록이 하나라도 있으면 `root.revision < 2`를 Corrupt로 거절한다.
- `anchor.rs:287–289`(`encode_listed_anchor`)도 "Only a Row anchor of revision 2 or later may carry a list"를 지킨다.
- 근거 계약은 `dev3-tree-smo.md:1421`이다: "목록이 있으면 anchor revision이 2 이상이어야 한다(목록은 unlink 그룹이 revision+1과 함께만 쓴다)."
- CREATE TABLE의 Row anchor는 revision 1이다(`semantic_catalog/images/catalog.rs:254–259`; Base4 대조 `plan4_recovery_base/catalog/graph/relations.rs:650–653`).
- 그러면 split한 적 없는 table에서 08-3의 OverflowCancel·ChainRetire는 모두 실패한다. 첫 큰 행 INSERT의 chunk 부족 취소, statement rollback, loser 복구가 여기에 든다. anchor encode가 InvalidInput을 내고, decode는 Corrupt를 낸다.
- 반대로 revision을 올리는 길도 막혀 있다.
  - `plan4_recovery_base/row/current_tree.rs:130`은 `revision > 1 + Structural high-water`를 IdentityMismatch로 거절한다. Row 그룹은 Structural high-water를 올리지 않는다.
  - revision은 cursor C4의 재탐색 판정이기도 하다.
- 시험 계획에 이 경우가 없다. OVF-F03(264행), OVF-J02·J03(273·274행)은 split 전 table을 따로 고르지 않는다.

**고칠 것**
- revision 규칙을 kind별로 정한다. 예를 들어 "revision ≥ 2는 목록에 tree 항목(RowLeaf·RowInternal)이 하나라도 있을 때만"으로 하고, Overflow 항목만 있는 목록은 revision 1에서도 허용한다.
- 그 변경 지점(`check_retired`, `encode_listed_anchor`, tree-smo 1421행의 대체)을 문서에 적는다.
- 시험에 다음 두 경우를 넣는다.
  - "revision 1 table에서 OverflowCancel·ChainRetire"
  - "revision 1 Overflow 목록 위에서 첫 unlink(revision 2가 되며 목록 병합)"

### R2. 새 전이가 Job3 codec·관측기·ledger·descriptor의 기존 LSN 규칙과 정면으로 맞지 않는다

**문서**
- §4.3 81–85행, §5 125·129행, §6.2 159·165행
- §4.5 106행의 "XHG Update, handoff"

**근거**

1. **ChunkWrite의 Job.** 각 ChunkWrite는 Reserved에서 Reserved로 가는 전이다.
   - 관측기는 Free가 아닌 after마다 `after.last_step() == layout.header().lsn`을 요구한다(`semantic_row/jobs.rs:123–128`).
   - codec은 Reserved에 `last_step == lease_lsn`을 요구한다(`retention_page/row/job.rs:511–512`).
   - 두 규칙을 함께 만족하는 ChunkWrite after는 없다.
2. **descriptor chain.**
   - ledger는 Reserved root가 있으면 `descriptor.chain.apply == job.lease_lsn()`과 `inventory.change_lsn == lease_lsn`을 요구한다(`semantic_row/ledger.rs:316–322`).
   - A의 Forward 준비도 `transaction.chain.apply == job.lease_lsn()`을 요구한다(`row/insert/group.rs:1026–1043`).
   - 그런데 handoff 그룹은 descriptor의 `chain.apply`를 자기 LSN으로 옮긴다(`semantic_row/transition/prepare.rs:1152–1158`).
   - 그래서 첫 ChunkWrite 뒤의 Forward와 OverflowCancel은 모두 OrderMismatch·BudgetMismatch가 된다.
3. **rows[1]의 걸음 수.**
   - ledger는 Reserved root의 남은 몫을 `debt(1, forward_wal)`로 고정한다(`ledger.rs:237–239`).
   - group.rs:1039–1042도 `remaining[1] == debt(row_forward_wal(..), 1)`을 요구한다.
   - 문서의 Tx row1 `(k−written)·W + F`(159행)는 k+1 걸음을 쓴다. 걸음 수와 식 모두 바뀌어야 하는데 바뀌는 곳이 적혀 있지 않다.
4. **ChainRetire의 cursor.**
   - 129행은 "release cursor는 그대로 0"이라고 한다.
   - 그런데 codec은 Releasing에서 `release_cursor == 0 ⇒ last_step == release_start`를 요구한다(`job.rs:471`).
   - 관측기는 ChainRetire after에도 `last_step == lsn`을 요구한다.
   - 그러면 cursor 0인 ChainRetire after는 codec에서 OrderMismatch가 된다.
5. **Releasing의 zero 구간.**
   - codec은 Releasing에서 `[200,248)`·`[280,284)`·`[285,320)`을 zero로 요구한다(`job.rs:463–465`).
   - CLR 관측기가 바꿀 수 있는 구간은 `(200,248)`·`(285,320)`이다(`jobs.rs:196–208`).
   - 문서는 ReleasingCompensated `[200,228)`을 chain으로 쓴다고만 한다.
   - `@285=1`이 Releasing root에 남는지가 정해지지 않았다. 81행 "1은 효과 0(INSERT)인 Row root에서만"은 Releasing root도 포함하는 것처럼 읽히지만, codec은 그것을 Corrupt로 본다.
   - `cleanup()`(91행)이 chain 유무를 무엇으로 판정하는지도 그래서 두 가지로 읽힌다.

**고칠 것**
- §4.3·§5·§6.2에 다음을 적는다.
  - **Reserved overflow root의 LSN 규칙.** 예: `last_step`은 마지막 ChunkWrite LSN이고, written 0이면 `lease_lsn`.
  - **descriptor 규칙.** chain.apply·inventory.change_lsn이 ChunkWrite마다 어떻게 되는지 정한다. handoff로 둘지, 아니면 owner·회계를 다른 방식으로 둘지 고른다.
  - **ledger rows[1]의 걸음 수와 식.** group.rs Forward 검사의 대체 식도 함께 적는다.
  - **ChainRetire의 cursor·last_step 규칙.** 예: cursor 0에서 `last_step > release_start`를 허용하되 `[200,228)`이 zero인 경우로 한정한다.
  - **Releasing에서 `@285`의 값**과, `cleanup()`·`forward_wal()`이 쓰는 판정 기준.
- OVF-F02(263행)의 거절 목록에 위 조합을 넣는다.

### R3. "Base4 record·certificate 형식은 바꾸지 않는다"는 사실과 다르다 — 지금 Base4는 kind 7 dynamic page를 받지 않는다

**문서**
- §8.3 234–241행, 특히 241행 "chunk page는 기존 certificate의 kind 7 page다"
- §8.1 222행

**근거**
- **certificate.** `plan4_recovery_base/image_records.rs:137–151`의 `decode_certificate`는 ordinal ≥ 9에서 kind를 Status·Undo·TreeAnchor·RowLeaf·RowInternal로만 허용한다. Overflow는 Unsupported다. 같은 파일 170–196의 `encode_certificate`도 같은 role을 쓴다.
- **provenance.**
  - `plan4_recovery_base/row.rs:189–194`는 ordinal ≥ 9의 Allocated page마다 `validate_provenance`를 요구한다.
  - `image_records.rs:197–262`의 role은 세 가지뿐이다: 1|2 System(Status·Undo), 3|4 CREATE pair(anchor·leaf), 5|6 Structural.
  - 거래가 INIT한 kind 7 page의 role이 없다.
- **current tree.**
  - `plan4_recovery_base/row/current_tree.rs:194–196`은 table 소유 page의 kind가 RowLeaf·RowInternal이 아니면 Unsupported다.
  - 202–212는 소유 page마다 Structural 또는 CREATE pair claim을 요구한다.
- **retired 순회.**
  - 같은 파일 248행은 모든 retired 항목을 `decode_tree_page`로 읽는다.
  - 251행은 `retire_lsn > high`(Structural high-water)를 거절한다.
  - §4.4 97행의 kind별 LSN 규칙은 이 두 곳을 바꿔야 성립한다. 그런데 §8.3은 소유자 목록만 더하고 이 순회의 분기를 적지 않는다.
- **replay claim.**
  - replay arena의 INIT claim은 System·CREATE·Structural 경로에서만 만들어진다(`wal_io/plan4_retention/history/catalog.rs:600–693`).
  - §8.1은 ChunkWrite INIT의 claim을 누가 어떤 role로 만드는지, 그리고 반환 뒤 그 claim이 retired page에 남는지를 정하지 않는다.

**고칠 것**
- §8.3의 문장을 바꾼다. 다음을 형식 변경으로 정한다.
  - certificate dynamic role에 `Overflow(owner = table object)`를 더한다.
  - 새 provenance role(예: 7, `TxnRef` source, table·chunk index)을 정한다.
  - replay의 INIT claim을 정한다. 만드는 자, role, Base4가 대조하는 칸을 적는다.
  - claim이 retire 뒤에도 유지되는지를 정한다.
- current_tree의 소유 검사를 다음과 같이 나눈다.
  - kind 7은 tree 도달 집합 밖의 별도 소유 집합으로 둔다.
  - retired 순회는 kind 7이면 chunk decode(head 없이 index·count·total·identity), `retire_lsn ≤ anchor_lsn`, `pageLSN < retire_lsn`으로 처리한다.
  - closure는 `reached ⊎ retired_tree ⊎ chain_owned ⊎ retired_overflow == owned`로 바꾼다.
- `dev3-tree-smo.md#current-tree-closure`의 대체를 명시한다.
- OVF-C05(271행)에 "certificate·provenance 손상", "claim 없는 kind 7"을 더한다.

### R4. split/unlink candidate audit와 leaf 관측기가 Overflow 행·chunk page를 거절한다 — 첫 큰 행 뒤에 그 table의 split·unlink가 막히고, Forward 자체도 거절된다

**문서**
- §7.2 199행의 "지금 inline으로 가정하는 곳" 목록
- §6.1 146행
- OVF-J06(277행)

**근거**

- **split audit.**
  - `semantic_row/structure/candidate.rs:313–336`의 `audit`는 table이 소유한 Allocated page 가운데 anchor가 아니고 visited에도 없는 page가 모두 retired 목록에 있어야 한다고 본다.
  - 정상 commit된 큰 행의 chunk page는 visited도 retired도 아니다. 따라서 그 table의 다음 split·unlink 준비는 IdentityMismatch가 된다.
  - 이것은 08-1 1회차 R1과 같은 종류의 누락이다.
- **retired 항목 검사.**
  - `candidate.rs:404–450`의 `retired_pages`는 retired 항목을 `decode_tree_page`로 읽는다.
  - `reached()`(238–272, 특히 256행)는 RowLeaf·RowInternal만 받는다.
  - 그래서 Overflow 항목이 있는 목록 위에서는 split도 거절된다.
- **unlink의 목록 병합.**
  - unlink가 XTR1에 tree 항목을 더할 때 기존 Overflow 항목을 보존해야 한다.
  - 그 항목에는 kind별 LSN 규칙도 적용해야 한다.
  - 이 둘이 적혀 있지 않다.
- **leaf 관측기.**
  - `semantic_row/leaf.rs:445–468`의 `validate_rows`는 Deleted가 아닌 모든 행에 `inline()`(433–440)을 요구하고, 그 밖의 행은 Unsupported다.
  - 이 함수는 leaf를 바꾸는 모든 Row 그룹의 before·after에 불린다(176–177행).
  - 그러므로 Forward(overflow)의 after leaf부터 거절된다. 큰 행이 있는 leaf에 작은 행을 넣거나 지우는 일도 거절된다.
- **그 밖의 inline 전용 지점.** 199행 목록에 없다.
  - `structure/weights.rs:142`: split weight가 Overflow 행에서 Unsupported
  - `visibility/pins.rs:506`: B capture가 Overflow 행이 있는 leaf를 bad로 판정
  - `plan4_apply/row/read.rs:1595`: Inline이 아닌 행을 `(0, true)`, 곧 삭제된 행으로 처리한다. capture를 고치고 이것을 빠뜨리면 조용히 Missing이 된다.
  - `plan4_recovery_base/row/actor.rs:383·431`
  - `plan4_recovery_base/row/graph.rs:327`
- **시험.** OVF-J06은 시험만 있고 그 시험을 통과시킬 설계 변경이 없다.

**고칠 것**
- §7.2에 split/unlink candidate의 소유 closure를 정한다.
  - 예: leaf의 Overflow 행이 가리키는 chain과 Overflow retired 항목을 소유 집합에 더한다.
  - 이때 chain을 따라갈지, head의 count만큼 allocator entry로만 셀지를 정한다.
  - 비용(chunk LOAD 수·frame)과 census 의무를 적는다.
- head 없이 retired chunk를 검사하는 decode 규칙을 R3과 같게 정한다.
- 199행 목록을 위 지점 전부로 바꾼다. 특히 `leaf.rs validate_rows`와 `read.rs:1595`의 B 복사는 꼭 넣는다.
- OVF-J06에 다음 경우를 더한다.
  - "commit된 큰 행이 있는 table의 다음 split"
  - "Overflow 항목이 있는 XTR1 위의 unlink(목록 병합)"
  - "큰 행과 같은 leaf의 작은 행 INSERT·DELETE·UPDATE"

### R5. §4.5 opcode 정의가 산술·role·inventory에서 서로 맞지 않는다

**문서**
- 106–108행 표, 117행 "`Operation::role()`은 셋을 자기 자신으로 둔다"
- 166행, OVF-F04(265행)

**근거**

1. **OverflowCancel 상수.** 다시 세면 기본 frame은 3,776 + 1,152 + 2,176 + 128 + 208(I=1) + 3×16,344 = **56,472**이다.
   - 문서의 상수는 `56,568+b+a`다.
   - 최대값 60,864는 56,472 + 2,176 + 2,216과만 맞는다. 56,568을 쓰면 최대는 60,960이 된다.
   - 문서 안에서 두 값이 서로 다르고, OVF-F04가 이 값을 고정한다.
   - 차이 96은 count record 하나의 크기다. E/K/I = 0/0/1과도 맞지 않는다.
2. **inventory action.**
   - `transition/graph.rs:176–182`의 `inventory()`는 role이 Reserve·Forward·Cancel일 때만 action(1·2·3)을 갖고, 그 밖은 InvalidInput이다.
   - 117행대로 role()을 자기 자신으로 두면 I=1인 ChunkWrite·OverflowCancel은 items record를 만들 수 없다.
3. **Cancel 고유 효과.**
   - descriptor의 inventory 정리(`prepare.rs:1170–1175`, Forward|Cancel 분기)와 Root의 canceled 계수(`prepare.rs:2127`)는 role Cancel에만 붙는다.
   - OverflowCancel이 "Cancel처럼"(127·166행) 되려면 이 분기에 들어가야 한다. 117행과 모순이다.
4. **ChunkWrite의 I=1.** 근거가 없다.
   - Reserved inventory는 바뀌지 않는다. lease 칸은 R2에서 정할 일이다.
   - I=0이면 frame은 56,976이 된다.

**고칠 것**
- OverflowCancel의 상수를 `56,472+b+a`로 고친다. 그렇지 않으면 K=1인 근거를 적고 최대값을 60,960으로 바꾼다.
- 세 opcode의 `role()`을 정한다. 각각 `inventory()` action, descriptor_after 분기, Root 계수(canceled·completed·consumed_wal)에 어떻게 들어가는지 표로 적는다.
- ChunkWrite의 I(0 또는 1)를 근거와 함께 정하고, 57,184 또는 56,976을 OVF-F04 기대값으로 고정한다.

## 권고

- **O1. C쪽 census 여유가 작은 곳을 단계별로 미리 적는다.**
  - tree-smo의 lifecycle 44,304는 여유 605, preflight C는 여유 182다.
  - live retirement는 39,694로 여유 514인데, 2,216 B XTR1 cell의 before·after 복사가 이 경로에 들어온다.
  - 그 밖에 CLR_PRODUCER 19,456, Row write 지점 6,144/11,520(resource-bounds 194행)이 있다.
  - OVF-J07을 O3 하나에 두지 말고, O1(codec frame)·O2(C 전이 frame)마다 census를 두는 편이 낫다.
  - 190행 "상한을 넘으면 그 근거와 새 상한을 D2에 올린다"는 08-1 검토가 쓴 규칙("맞지 않으면 상한을 올리지 않고 계약을 다시 검토한다")보다 느슨하다. 같은 규칙으로 맞추기를 권한다.
- **O2. chunk 읽기의 원천과 거주 상한을 정한다.**
  - Forward(§6.3)·OverflowCancel·ChainRetire는 최대 9개 chunk를 LOAD한다.
  - `retention_page/row.rs:41`의 ROW_PEAK(frames 9, memory 425,984)와 그 LOAD의 protected set이 어떻게 맞는지 적는다.
  - live는 pool, replay는 arena(§8.1)라는 구분을 C 진입별로 표로 적는다.
  - Forward의 verifier는 각 chunk의 allocator entry(`Allocated(g, table, Overflow)`, incarnation = g)도 대조하도록 명시한다. 지금 136행은 page identity만 말한다.
- **O3. ChainRetire가 anchor를 찾는 근거를 정한다.**
  - Releasing에서 `[200,248)` pair는 zero다(`job.rs:463`). 문서는 그 자리를 `(head.root, total)`로 쓴다(85행). 그러면 ChainRetire는 XTR1을 쓸 anchor ref를 Job에서 얻지 못한다.
  - 예: `[200,224)` anchor, `[224,248)` root로 두고 total은 chunk 0 header에서 읽는다. 또는 catalog 조회 경로와 그 비용을 적는다.
- **O4. ChainRetire의 시점을 적는다.**
  - Releasing 정리는 maintenance이고 terminal Ready 뒤에만 열린다(module-consumers 613행, `row_start/retire.rs`의 `row_cleanup_next`).
  - 그러므로 statement rollback 직후가 아니라 거래가 끝난 뒤 drain에서 반환된다.
  - OVF-J02(273행)의 "큰 행 CLR → ChainRetire"를 이 시점으로 고친다.
  - jobs.rs의 maintenance 목록과 `row_cleanup_next` 선택 순서에 ChainRetire를 넣는다고 적는다.
- **O5. preflight의 k개 Free 확인을 advisory가 아니라 Capacity 판정으로 둔다.**
  - 195행이 advisory이면 중간에 Free가 모자랄 때 쓴 chunk가 XTR1로 가고 14-2까지 첫 extent를 차지한다(253행).
  - 미리 거절하면 이 누수가 없다.
  - eligible 조건은 split과 같은 allocator-only 판정으로 정한다(`candidate.rs:532–555`).
- **O6. digest의 쓰임을 구분해 적는다.**
  - core before·after digest는 실제 record bytes를 쓴다.
  - root-zero XRP1(74행)은 Job projection 대조에만 쓴다.
  - 지금 문장은 둘을 섞어 읽힐 수 있다.
- **O7. §8.3 1항의 "보이는 leaf의 Overflow 행"을 "leaf에 있는 모든 Overflow 행(미확정·Prune 대기 포함)"으로 고친다.**
  - Retained 상태의 미확정 행도 chain의 유일한 소유자다.
  - 소유가 바뀌는 그룹이 원자적임을 적는다. 그런 그룹은 Forward(pending→leaf), CLR(leaf→Job), ChainRetire·OverflowCancel(Job→XTR1)이다.
- **O8. 시험을 더한다.**
  - B 읽기의 chain 손상(task-06 T06-10 대응)
  - B·Forward 도중 `FlushRequired`
  - 큰 행과 같은 leaf의 작은 행 DELETE·UPDATE
  - revision 1 table의 rollback(R1)
  - terminal 뒤 ChainRetire 시점(O4)
  - unlink와 ChainRetire의 순서 두 가지(목록 병합)
  - XTR1 n = 55 경계
- **O9. §9를 더 정직하게 적는다.**
  - 각 ChunkWrite는 Row write로서 TerminalReadGate를 닫는다(`plan4_apply/row/write.rs:231–262`). 큰 행 하나가 최대 11개 durable 그룹 동안 읽기를 반복해 막는다. SCOPE-002 동시성 항목에 이것을 적는다.
  - XTR1은 anchor cell 하나와 첫 extent(55)에 묶여 있어 확장되지 않는다.
  - A4는 값 전체에 임의 접근해야 한다. 뒤에서부터 쓰므로 streaming LOB 쓰기가 아니다.
  - verifier의 방문 집합은 O(k) 배열이다.
  - "batch로 나누면 된다"(249행)는 Job에 진행 cursor가 없다. 그리고 135–136행의 "일부만 반환하지 않는다"와 충돌한다. 상한을 올릴 때 필요한 형식 변경으로 적는 편이 맞다.
- **O10. "대체·확장 목록"을 한 절로 둔다.** 이 설계가 다음 정본을 바꾼다.
  - `dev3-overflow-format.md`의 root 칸
  - `dev3-tree-smo.md`의 XTR1 kind·revision·closure
  - `dev3-module-consumers.md`의 opcode 집합, native kind, redo Patch 전용(`wire.rs`의 `validate_redo_shape`). INIT redo와 96 B allocator patch는 Row 그룹에 처음 들어온다.
  - `dev3-resource-bounds.md`의 WAL recipe, 219행
  - task-06 §5.4의 transient pin 규칙. 208행이 08-4로 미룬다.
  - Row 그룹이 처음으로 tree anchor page를 쓴다는 점. `history/row.rs`의 anchor image 대조도 바뀐다.
- **O11. 새 opcode의 Root 효과를 적는다.**
  - OverflowCancel이 canceled를 올리는지
  - ChainRetire(non-handoff)의 consumed_wal 가산과 job cursor(`prepare.rs:2135–2150`)
  - ChunkWrite의 Root 변경 칸

## NIT

- **N1.** 112행 "offset이 entry 위치"는 native header의 어느 칸인지 적는다.
  - header의 slot은 @4/6, offset은 @8/10이고 88..128은 zero다(`wire.rs`의 `validate_native_shape`).
  - allocator entry 하나의 native가 이 칸들을 어떻게 채우는지 적는다.
- **N2.** 207행의 `decode_materialized_row(head, output[..total], schema)`는 실제 signature와 다르다. 실제는 `columns: &[Column]`이다(`row_record.rs:603`). schema 입력 변형이 필요한지, 아니면 Columns로 넘기는지 적는다.
- **N3.** native kind 번호를 O1로 미뤘다(112행). 그런데 OVF-F04가 그 번호에 기대므로, 번호를 문서에 두는 편이 낫다. `ChainRetire = 25`처럼 번호를 건너뛴 이유도 한 줄 적는다.

## 건전하다고 본 것

- **뒤에서부터 쓰는 ChunkWrite(A4).**
  - next는 언제나 이미 쓴 page이므로 예약이 필요 없다.
  - 중간 상태는 pending(front, written) 하나다.
  - crash가 어느 ChunkWrite 사이에 나도 front에서 written개를 따라가면 마지막 next가 zero인 완결된 부분 chain이 나온다. 그래서 loser OverflowCancel(227행)의 순회가 성립한다.
- **Job3 칸 재사용.** Reserved `@328..360`(sender digest 자리)을 pending으로 쓰고 Forward가 sender digest로 덮는다. Forward가 바꿀 수 있는 구간 `(328,360)`(jobs.rs:157)과 맞는다. CLR이 `(200,248)`을 바꿀 수 있는 것도 맞다. 맞지 않는 것은 codec의 zero 규칙과 LSN 규칙뿐이다(R2).
- **Forward의 chain 검산.** verifier, `CanonicalRowStream`, 그리고 Base4의 정확히 한 번 소유 검산이 결합해 남의 chain이 끼어든 경우를 잡는다(§4.1 66행).
- **§7.4 reader와 겹치지 않음.** 다음 셋이 함께 성립하므로 08-3의 반환은 읽는 중인 chain을 건드리지 않는다.
  - TerminalReadGate가 Row write와 live 정리 동안 닫혀 있다.
  - MutationPort가 Reserve에서 Forward까지 배타다.
  - 미확정 INSERT는 다른 거래에 Missing이다.
- **Free page 선택의 경쟁.** ChunkWrite와 split의 Free 선택은 단일 MutationPort 아래에서 같은 규칙(가장 낮은 eligible Free, next generation)을 쓴다. 경쟁이 없다.
- **범위.** 08-3·08-4·14-2의 나눔이 schedule baseline §2.2·§3과 맞는다. 08-3은 08의 마지막 항목이 아니므로 O4 병합으로 적립하는 것도 맞다(285–293행).
- **수치.** R5에 적은 것 외의 수치는 다시 계산한 값과 같다.
  - MAX payload 16,092, k ≤ 9, C = 244, leaf 248
  - W = 57,184(I=1일 때), F `91,548+128h`, CLR `91,340+128h`
  - Job cleanup 312,096, 4R 222,112, 4R+X 281,168, 반환 30,928
  - X 59,056, 9W 514,656, k=9 경로 약 0.98 MB, 고정 부담 32,688
  - OverflowCancel이 F 몫 안에 드는 것(≤ 60,864 < 91,548)
- **F4.** 인용한 caller 상한은 `row_caller_budget.rs`와 같다: BEGIN 2,816, STATEMENT_START 3,584, COMMIT 3,024, ROLLBACK_CALLER 1,536, LIVE_ROW_CLEANUP 4,096. drain 4,082/4,096도 tree-smo census와 같다.
- **인용한 함수 이름.** 모두 실제 이름과 같다: `split_row_live`(`plan4_apply/row/split.rs:37`), `autocommit_insert_live`(`driver/live.rs:700`), `preflight_row`(`plan4_apply/row/write.rs:375`), `producer_lane`.

Verdict: CONDITIONAL

# 독립 D2 검토(1회차): 빈 leaf unlink U1

## 대상

- commit: `09eb773` (local branch `u1-local`, tree `0a865642df5f09a67fcdc7e47980617c13726c2a`)
- base: `b6dc3a2` (설계 commit, 계약 D2 3회차 ACCEPT. `module-gate/evidence/dev3-08-1-unlink-design/review.md`)
- 범위: `git diff b6dc3a2 09eb773`, 32개 파일, +3,085/−117
- 계약: `devdoc/contracts/dev3-tree-smo.md`의 `#empty-leaf-unlink`(1122행부터)와 새 `#unlink-u1` 절
- 검토자는 이 코드를 작성하지 않았다. `/home/claude/xadb`은 읽기만 했고, 빌드·시험은
  `git archive 09eb773`로 만든 scratch 사본과 별도 `CARGO_TARGET_DIR`에서 했다. 사본에 probe를 넣지 않았다.

## 결론

열린 필수는 **1건**이다.

제품 코드에서는 계약 위반이나 결함을 찾지 못했다. 잘못된 입력을 수락하거나 정상 상태를 거절하는 경우도 없었다.
검토한 범위는 XTR1 codec, opcode 3·4 wire grammar와 canonical 비교, split `audit`의 `reached ⊎ retired == owned`
변경, unlink candidate의 전이·부채 규칙, Base4 closure다. fixture author는 제품 코드와 독립이고, 재생성하면 19개
hash가 모두 맞으며 기존 17개 fixture는 base와 byte가 같다. 회계 고정값은 S1B 하나만 바뀌었다(+32B). live split
peak 37,438B와 복구 peak 65,392/65,400B는 그대로다.

필수 1건은 `U1 결과` 절의 시험 증거 기록이다. 시험 식별자, 실행 범위, 결과 수가 실제와 맞지 않아 그대로는
재현할 수 없다. 문서만 고치면 되며, 고치면 ACCEPT로 본다.

## 확인 방법

읽은 것:

- 계약 1122–1450행 전체: 대체 목록, 전이 표, XTR1, scope4 frame, core byte 표, 준비 1..5, Base4 closure, 시험
  계획, 구현 단계, U1 결과.
- 제품 diff 전체: `anchor.rs`, `semantic_row/structure.rs`·`bank.rs`·`candidate.rs`·`candidate/source.rs`·
  `frame.rs`, 새 `unlink.rs`·`unlink/candidate.rs`·`unlink/frame.rs`, `plan4_recovery_base/row/current_tree.rs`,
  이름 변경 6개 파일과 `structural_effect.rs`·`structural_prepare.rs`의 match→if 변경.
- 비교 대상 기존 코드: `tree_page/structure.rs`의 `prepare_internal_edit`(RemoveChild의 separator 선택),
  `structural_effect.rs`의 `check_split_debt`, Base3/Base4 `relations.rs`의 anchor 검산, Base4 `ledger.rs`의
  `structural_provenance`, `actor.rs`·`graph.rs`의 `has_leaf` 소비자, `decode_anchor` 호출 지점 전부.
- 시험 diff 전체: `anchor_tests.rs`, `weights/unlink_tests.rs`, `weights/unlink_frame_tests.rs`, `bank_tests.rs`,
  `current_tree_tests.rs`, `row_gate.rs`. fixture `author.py` diff와 `inputs.sha256`.

실행한 것과 결과(scratch 사본, `CARGO_TARGET_DIR` 분리, XADB_CATALOG_* 자리표시 hash):

| 명령 | 결과 |
| --- | --- |
| 전달받은 filter(`unl_ storage::anchor storage::semantic_row::structure storage::plan4_recovery_base::row s1b_row_arm_census spl_a08 structural_replay_tests structural_effect structural_bank --skip xrb_`) | 141건 선택. **140 passed, 0 failed, 1 ignored**(`structural_effect_native_worker`). 2,234건 filtered out, 391s |
| 같은 filter에 `--list` | 141건이며 `xrb_` 0건(`--skip xrb_`는 아무것도 거르지 않는다) |
| `structural_prepare structural_replay_source structural_admission` (바뀐 `structural_prepare.rs`·`history/structure.rs`의 시험) | 7 passed |
| `spl_a08 s1b_row_arm_census -- --nocapture` | PASS. 값은 측정값 절 |
| `recovery_source_memory recovery_source_native`(Base4 `current_tree::source_bytes` 변경이 들어가는 `resources::fits`, XADB_PLAN4_RECOVERY_* 자리표시) | 2 passed, `base=64772` |
| UNL 시험 16개(P01–P10, F01–F04, B01, B02–B06) | 모두 위 실행에서 PASS |
| `python3 -I -B author.py`(fixture 디렉터리 사본에서 `*.xrb`를 지운 뒤 재생성) | 19개 모두 `inputs.sha256`과 일치. 기존 17개는 base `b6dc3a2`의 파일과 byte가 같다. import는 표준 library(`pathlib`·`hashlib`·`struct`)뿐이다 |
| `cargo fmt --all --check` | PASS |
| `cargo clippy -p xadb-engine --all-targets --all-features --locked -- -D warnings` | PASS(engine crate만. workspace 전체는 돌리지 않았다) |
| `cargo check -p xadb-engine --lib`(cfg(test) 밖의 dead code) | 경고 0. `structure`는 `semantic_row.rs`의 `allow(dead_code)` 아래에 있다 |
| `python3 -B devdoc/verify_documents.py` | FAIL. 원인은 예상한 `module-gate/evidence/dev3-08-1-unlink-u1/review.md` 누락 링크 1건뿐 |

전체 suite는 돌리지 않았다(AGENTS.md). `xrb_*`는 module-gate.json의 `evidence.executor_only`
(`storage::plan4_recovery_base::tests::`) 대상이어서 실행하지 않았다.

초점별 확인(결함 없음):

1. **XTR1**(`anchor.rs:215` `check_retired`). 다음을 모두 검산하고, encoder는 InvalidInput, decoder는 Corrupt
   (version은 UnsupportedFormat)로 거절한다.
   - magic과 version 1, n 1..=55와 정확한 길이, 8..16과 entry 26..32 zero
   - Row만 받고 revision ≥ 2
   - 첫 file, page_no 9..63(`9..64`), (file, page_no) 엄격 증가. 같은 page_no의 다른 incarnation도 거절한다
   - anchor·현재 root와 물리 주소 비교(incarnation alias 포함)
   - (RowLeaf, 0)·(RowInternal, 1..=15)
   - 0 < retire LSN ≤ anchor pageLSN

   `decode_listed`(313행)는 cell 1·2개만 받는다. Base3(`relations.rs`의 `!G::ROWS`, revision == 1)는 revision ≥ 2
   규칙 때문에 목록이 있는 anchor를 decode 단계에서 거절한다. root만 쓰는 나머지 decode 지점(`row/insert/group.rs`,
   `plan4_apply/row/read.rs`, `row_preflight.rs`, `history/row.rs` 등)은 계약대로 목록을 무시한다. Row anchor를
   다시 쓰는 곳은 split candidate(`candidate.rs:673`)와 unlink candidate 둘뿐이고, CREATE·Base INIT anchor는
   `encode_anchor`(빈 목록) 그대로다.
2. **opcode 3·4 wire**(`unlink.rs`). 다음이 계약 표와 일치한다.
   - 길이 35,248·19,032와 산식(160+192+1536+256+384|512+32,688|16,344+32)
   - envelope record_count = redo 수, group 124/132/136/152의 수·bytes, 24..32·88..120 zero
   - `validate_finished_owner` 공유
   - core: 160 RowLeaf, 161 0, 162 zero, 163 root level, 164 separator u16, 166..168 zero, 200..224 zero,
     opcode 3이면 80..104 zero, removed ≤ separators, opcode 4이면 root level 1·separator 1, opcode 3이면
     separator ≥ 2, after 목록 길이 1|2..=55, high-water=0 ⇔ digest zero, 주소 alias, root==P ⇔ root level 1
   - role 집합: u16 bitmask, `role >= 16`을 먼저 거른다
   - role → core 칸과 kind/level: anchor 32, P 104 RowInternal 1, L 56 RowLeaf 0, S 80 RowLeaf 0
   - 물리 순서, 모든 native의 0 < prior < lsn
   - 바뀌지 않는 native: 0xFFFF이고 after = before
   - 바뀌는 native: dense ordinal이고 after ≠ before, redo target·prior 일치, `Patch(72,16280)`, 남는 byte 0

   split `decode_layout`·`decode_core`·`init`은 opcode 3·4를 Unsupported로 막아 두 grammar가 섞이지 않는다.
   `bank.rs:61` 길이→opcode 대응은 split의 두 값과 겹치지 않는다.
3. **canonical 비교**(`unlink/frame.rs:226`). observer는 wire decode, 같은 cut에서 `prepare`, header·operation
   일치, 160..len−32의 모든 byte 비교 순으로 확인한다. envelope은 `decode_semantic_envelope`와 header 비교가 덮는다.
   `NotNeeded`인 cut의 frame은 IdentityMismatch다. producer와 다른 frame을 수락하는 경로는 찾지 못했다.
4. **전이**(`unlink/candidate.rs:101`).
   - L == root, cell 있음, non-root P이면서 separator 1이면 `NotNeeded`
   - separator ≥ 2이면 LeafUnlink. `RemoveChild`는 i>0이면 entry i−1을, i=0이면 entry 0을 지우고 child 1을
     leftmost로 만든다(`structure.rs:220–236`). core 184의 separator 선택 `removed.saturating_sub(1)`과 같다
   - root level 1이면서 separator 1이면 RootCollapse. S = child(removed==0)이고 level 0을 검산한다
   - `added`는 page_no 순으로 정렬한 뒤 `extend`한다(240–250행)
   - retire LSN = 새 frame LSN = owner id. revision+1의 overflow는 `table()`의 `u64::MAX` 거절(Exhausted)이 막는다
5. **부채**.
   - allocator Reserved와 모든 scope의 RootReserved는 공유 `audit`이 OrderMismatch로 거절한다
   - non-Free Catalog Job은 `check_debt`(86행)가 Unsupported로 거절한다. `check_split_debt`와 같은 object·generation
     비교이고, RootReserved 부분은 audit이 맡는다
   - "L로 route되는 Retained Row Job 없음"은 audit의 역방향(각 RootRetained Row Job의 행이 route한 leaf에 있음)과
     L의 cell 0개에서 따라 나온다(149–150행 주석과 같다)
6. **split과의 결합**(`candidate.rs:275`·`399`·`673`·`770`). 목록이 없는 anchor에서는 기존 판정과 같다(`find`가
   None이면 이전과 같은 IdentityMismatch). 목록이 있으면 다음을 확인한다.
   - 소유 4방향: 도달 ⊆ 소유, 소유 ⊆ 도달 ∪ 목록 ∪ {anchor}, 목록 ⊆ 소유, 도달 ∩ 목록 = ∅
   - 항목 decode, pageLSN ∈ (0, retire LSN), leaf는 cell 0개
   - split after anchor는 `retired.cell()`을 그대로 보존하고, `verify_after`가 bytes 동일성을 본다
   - 퇴역 page는 Allocated라 split의 Free 선택에 들지 않는다

   따라서 unlink 뒤의 split은 수락되고(P09) 고아는 거절된다(P10 v0). 목록을 바꾼 split frame은 byte 비교로
   거절된다(F03).
7. **Base4 closure**(`current_tree.rs:215–258`).
   - 항목마다 allocator가 같은 object·incarnation·kind로 Allocated인지 본다. generation 불일치는 앞의 owned
     loop가 거절한다
   - 도달 bit가 없어야 하고, certificate(kind·level)로 decode하며, pageLSN < retire LSN, leaf는 cell 0개다
   - `reached | listed == owned`
   - Row actor·graph·D(`leaf_reservation`)·`each_row`·`unique_row`는 모두 `has_leaf`(도달 leaf)만 쓴다
   - `structural_provenance`는 bank 33의 unlink owner를 같은 id의 split claim이 이름할 수 없게 한다(B08)
8. **fixture·시험 독립성**.
   - P05–P07: 기대 anchor·parent·core가 독립 literal(`xta1`·`xtr1`·`internal_cells`·`core`)이다
   - F01: section serializer는 제품 emitter·descriptor·redo encoder를 쓰지 않는다. core와 after image는 product
     candidate에서 오지만, F01의 두 경우(root 23 [13,20,21,22]에서 21 제거, root 23 [13,20]에서 20 collapse)는
     P05 셋째 경우·P06 variant 0과 같은 입력이라 literal로 따로 검증된다
   - B 시험: Python fixture에 기대값을 두고, 변조는 독립 `xtr1`·`set_cells`로 만든다
9. **회계**. 바뀐 고정값은 S1B 26,420→26,452 하나다. +32B는 split candidate census(`candidate/source.rs`)에
   더한 `RetiredList`(16B) 항목들과 `retired_chain`에서 나온다. 항목별 귀속은 다시 계산하지 않았다. 다른 숫자
   고정 `assert_eq!`는 검색해도 이 변경과 관련된 것이 없다. SPL-A08과 recovery resources는 상한 비교로 PASS다.
10. **"닿지 않는 Exhausted" 주장**. 목록 항목, 도달 page, anchor는 모두 9..63 안에서 서로 다르다. LeafUnlink는
    도달 ≥ 2(root, L)라 before 목록 ≤ 53, after ≤ 54다. RootCollapse는 도달 ≥ 3(P, L, S)이라 before ≤ 52,
    after ≤ 54다. 55를 넘을 수 없으므로 주장은 맞다.

## 필수

1. **U1 결과의 시험 증거 기록이 실제 실행과 맞지 않는다**(`devdoc/contracts/dev3-tree-smo.md:1426–1450`).
   - (a) 시험 표의 식별자 5개가 libtest 경로가 아니라 파일 이름이다. `--list`와 대조하면 표의 경로는 모두
     0건이다.

     | 표의 경로 | 실제 libtest 경로 |
     | --- | --- |
     | `storage::anchor_tests::` | `storage::anchor::tests::` |
     | `semantic_row::structure::weights::candidate_tests::unlink::` | `storage::semantic_row::structure::weights::tests::candidate::unlink::` |
     | `…weights::unlink_frame_tests::` | `…weights::tests::candidate::unlink::frame::` |
     | `semantic_row::structure::bank_tests::` | `storage::semantic_row::structure::bank::tests::` |
     | `plan4_recovery_base::row::graph::current_tree_tests::` | `storage::plan4_recovery_base::row::graph::tests::current_tree_tests::` |
   - (b) "결과 154 PASS, 1 ignored"를 기록된 범위로 재현할 수 없다. 1448–1449행이 적은 범위와 이 검토에 전달된
     명령은 141건을 골라 140 PASS·1 ignored다. 155건은 다른 filter 조합(예: `structural_prepare`·
     `structural_replay`·`s1b`를 더한 경우)에서만 나온다. 실행 명령이 기록되어 있지 않다.
   - (c) "환경 변수가 필요한 `xrb_*`는 base에서도 실패하여 제외했다"는 기록된 filter와 맞지 않는다. 그 filter는
     `xrb_`를 하나도 고르지 않는다. `xrb_*`(`storage::plan4_recovery_base::tests::`)는 module-gate.json
     `evidence.executor_only`의 executor 전용 case이고, 이번에 바뀐 Base4 decode(`current_tree::observe`)를
     지난다. 제외 이유는 "base에서도 실패"가 아니라 executor 전용이라는 점이며, 그 처리(executor 실행 결과 또는
     N/A)를 적어야 한다.
   - (d) "structural … history·prepare"를 함께 돌렸다고 하지만, 기록된 filter는 바뀐 `structural_prepare.rs`·
     `history/structure.rs`의 시험(`structural_prepare_*` 5, `structural_admission_*` 1,
     `structural_replay_source_inventory_*` 1)을 고르지 않는다. 검토자가 따로 돌려 7 PASS를 확인했다.

   실패 시나리오: 이 표를 PR의 module-gate record `cases`나 S5 최종 인수의 selector로 그대로 옮기면 0건이 선택되어
   gate가 "matches nothing"으로 실패한다. 또는 실제와 다른 수의 PASS를 증거로 남기게 된다(development-policy의
   "거짓 PASS·필수 시험 누락 0", AGENTS.md의 원 source·실행 범위 보존).

   필요한 수정: 실제로 실행한 명령(filter 전체와 env 자리표시)과 명령별 결과 수(passed/ignored/filtered)를 그대로
   적는다. 표의 식별자를 `--list` 경로로 고친다. `xrb_*` 문구를 executor 전용 처리로 바꾼다. 154가 다른 명령의
   결과라면 그 명령을 적고, 이 검토가 재현한 140+1(그리고 추가 7)과 구분한다.

## 선택

1. `check_debt`(`unlink/candidate.rs:86–98`)에 U1 시험이 없다(U1 결과에 공개되어 있다). U2의 replay observer도 이
   검산에 기대므로 U3의 UNL-L07까지 기다리지 말고, U2에서 non-Free Catalog Job 한 개를 넣은 단위 시험을 더하기를
   권한다. Unsupported가 나오고 NotNeeded보다 먼저 거절되는 순서를 함께 본다.
2. `structural_effect.rs:383–388`과 `structural_prepare.rs:199–218`은 exhaustive `match`를 `if … RootSplit else`로
   바꿨다. 지금은 split `Observation`이 split opcode만 갖지만, U3에서 opcode 3·4가 잘못 흘러들면 ChildSplit으로
   조용히 처리된다. `debug_assert!(op.is_split())`나 명시적 `LeafUnlink | RootCollapse => Err(Invariant)` 갈래로
   exhaustiveness를 되살리기를 권한다.
3. `RetiredList::extend`(`anchor.rs:179`)는 `added`가 물리 순서로 정렬되어 있고 기존 목록과 겹치지 않는다고 전제한다.
   위반하면 뒤따르는 encoder가 InvalidInput으로 거절하므로 잘못된 목록이 만들어지지는 않는다. 그래도 전제를 doc
   comment에 적거나 `debug_assert`로 두면 U3 호출자가 덜 헷갈린다.
4. commit에 포함된 `devdoc/document-validation.json`은 `status: FAIL`이고 작성자 checkout의 절대 경로
   (`/home/claude/xadb/…`)를 담고 있다. review.md를 둔 뒤 다시 생성할 것.
5. `devdoc/README.md`는 "UNL-…·B01..B09 memory 시험"이라고 적어 B07 재시작과 B09 BaseOnly·rebase가 U2로 미뤄진
   것을 드러내지 않는다. U1 결과처럼 "B07·B09 일부"로 적기를 권한다.
6. PR의 module-gate record: `rust_tests.cases`에는 위 (a)의 실제 경로를 쓴다. 바뀐 Base4 closure를 지나는 XRB
   executor-only case는 `na`에 executor 결과(또는 실행)를 둔다.

## 측정값

- SPL-A08(memory·native 같음): `LiveSource { carriers: 1352, residency: 36900, effect: 37438, peak: 37438 }`,
  effect_entry 2,392, effect_peak 37,550, charge 44,304, 여유 722. 계약 1081행의 37,438과 같다.
- S1B: row 25,739, structural 26,452, phase_max 25,743, frame 26,452, closure 20,951, live_peak 64,016(memory)/
  64,024(native), recovered_peak 65,392/65,400. live·recovered peak은 `dev3-row-update.md:398`의 값과 같다.
- recovery resources: `P4_RECOVERY_COMPONENT base=64772 pool=7048 home=8644 flush=9121`, `fits` PASS.
- 선택 시험 실행 시간: 391s(141건). build 2분 23초, clippy 1분 38초.
- scratch: `…/scratchpad/review-u1/{test.log,census.log,census2.log,extra.log,clippy.log,docs.log}`. build
  target(2.7GB)은 기록 뒤 지운다.

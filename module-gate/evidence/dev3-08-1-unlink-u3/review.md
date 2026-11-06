Verdict: ACCEPT

# U3(08-1 live unlink) 독립 D2 검토 2회차

## 대상

- 범위: 1회차 지적 반영분 `git diff e2822a4 3b0fd72`(10 files, +523/−133)와 unit 전체 `git diff 92b22bf 3b0fd72`
  (branch `u3-main`, HEAD `3b0fd72`). `e2822a4`는 1회차 HEAD `626e062`를 commit 하나로 합친 것이고 tree가 같다
  (`git diff 626e062 e2822a4` 0줄 확인). 작성자 worktree는 읽기만 했다(HEAD `3b0fd72` 그대로).
- 아래 줄 번호는 모두 `3b0fd72` 기준이다.
- 읽은 것
  - 1회차 검토 `u3-review-r1.md`와 그 brief `u3-d2-brief.md`.
  - code: `plan4_retention/structural_unlink.rs`(note 모듈, `unlink_next`, `note_emptied_leaf`, `unlink_leaf_live`,
    `unlink_resident`, `unlink_leaf_empty`, `unlink_parent_shape`, `prepare_unlink`, `read_unlink_before`, `source_bytes`,
    단위 시험), `catalog_lifecycle.rs` `apply_existing_n`, `structural_effect.rs` `check_split_debt`,
    `history/runtime.rs` `replay_scope`, `history.rs` `validate_live`, `catalog_live.rs`의 `BANK_END`와 `ControlCut`,
    `control.rs` 상수, `dirty_frame/reader_load.rs` `load_eligible`·`select_load`, `dirty_frame.rs` `seed_pins`,
    `row_start/retire.rs`(`retire_resident`, `delete_prune_note`), `row_clr.rs`, `row_forward.rs`, `row/insert/driver.rs`
    `unlink_notes`, `row/insert.rs` `local_source_carrier_bytes`의 설명, `plan4_apply.rs` `unlink_busy`.
  - 시험: `plan4_apply/source_tests.rs` `unl_c03_notes_live_in_the_control_loan`, `unlink_rows.rs`(`reachable`,
    `internals`, UNL-L09), `auto_split.rs`(SPL-A03), `split_rows.rs`(`leaves`, `anchor_of`, SPL-J02),
    `catalog_memory.rs` `with_fresh!`의 frame 수.
  - 문서: `dev3-tree-smo.md` 설계 UNL-L09..L15 행(1378), 역사 행 SPL-J02(934)·SPL-A03(1095), 상태 줄(1126),
    `<a id="unlink-u3">`(1716)부터 끝(1933)까지, `dev3-task-14-reclamation.md`의 "자원 (A)" 문단(286),
    `dev3-schedule-baseline.md` 120행, `AGENTS.md`.

## 판정 (필수 0, 권고 0, NIT 2)

- 1회차 필수 둘은 모두 해결됐다.
  - 필수 1: leaf LOAD의 보호 집합이 `[anchor, parent, leaf]`가 되었고(structural_unlink.rs:438-441), 이제
    `unlink_resident`부터 설치까지 page를 읽는 모든 곳이 그 앞의 마지막 LOAD가 보호한 page만 읽는다. 새 UNL-L09는
    leaf LOAD가 고를 수 있는 frame을 깨끗한 P와 dirty X 둘로 정확히 묶고, 결과를 product 보고가 아닌 pool·anchor·읽기
    사실로 판정한다. 내 사본에서 보호 집합을 `[anchor, leaf]`로 되돌리면 L09가 첫 호출(unlink_rows.rs:1878)에서
    `Frame(MissingTarget)` Unchanged로 실패하고, 고친 code에서는 통과한다(둘 다 직접 실행).
  - 필수 2: SPL-A03은 rollback 직후(drain 전)에 split이 남아 있음을 anchor로 보고, drain 뒤 RootCollapse를 product
    보고(`unlinked` 합 1)와 따로 anchor의 root·level·목록으로 본다. 계약 행 1095와 1862-1876 문단도 고쳤다. 내 실행 PASS.
- 권고 6개는 모두 해결됐다. 권고 2의 note·ControlCut 불변식은 계약에 적혔다. 실패 시험은 근거(배타 차용)를 대고
  두지 않았는데, 권고 수준의 요청이므로 받아들인다.
- NIT 7개 가운데 6개는 해결됐다. NIT 5(note 실패 처리)는 두 곳 모두 의도를 적었지만, 새로 적은 문장 하나가 code와
  정확히 맞지 않아 이번 NIT 1로 남긴다.
- 반영분이 새로 만든 결함은 찾지 못했다. 새 `reachable`은 단언이 통과하면 예전 helper와 같은 집합을 돌려주므로,
  어떤 UNL-L 시험도 전보다 덜 판정하지 않는다. 계약에 적힌 census 값은 내가 돌린 출력과 모두 같다(아래 직접 확인).
  상한은 올리지 않았다.

## 1회차 항목별 처리

- 필수 1: **해결.** 보호 집합 `&[anchor, parent, leaf]`(structural_unlink.rs:440). 읽기와 보호를 하나씩 맞춰 보면
  다음과 같다. anchor LOAD `[anchor]` 뒤 `table_root`가 anchor를 읽는다. RowInternal LOAD `[anchor, internal]` 뒤
  `route_step`이 그 page를 읽는다. leaf LOAD `[anchor, parent, leaf]` 뒤 `unlink_leaf_empty`가 leaf를,
  `unlink_parent_shape`가 parent를 읽는다. `check_split_debt`는 control Job만 읽는다(structural_effect.rs:333-354).
  set loop(:458)의 LOAD는 모두 set 전부를 보호한다. 그 뒤 `prepare_unlink`의 `replay_scope`·`validate_live`는 replay
  대여와 WAL만 쓰고, `read_unlink_before`의 typed read와 `apply_existing_n`의 `check`·`access.write`(catalog_lifecycle.rs:587-615)
  사이에는 LOAD가 없다. `select_load`는 보호된 page(reader_load.rs:340-345)와 pin이 있는 frame(`load_eligible`의
  `counts[index] == 0`, :212)을 victim으로 고르지 않는다. 그래서 1회차 시나리오(P가 leaf LOAD의 victim)는 더 이상 생기지
  않는다. residency 항에 보호 배열 셋과 set loop의 복사 `[TargetRequest; 7]` 448B를 더했다(:967-969). 1회차가 요청한
  "다른 이유의 `MissingTarget`" 분류는 따로 적지 않았다. shell 안에서는 read gate가 닫혀 있고(UNL-L10) 보호되지 않은
  읽기 경로가 이제 없으므로 결함으로 보지 않는다.
- 필수 2: **해결.** auto_split.rs:420-449, 469-471. rollback 직후 `anchor_of`가 `(old_root, 1, [])`이고 두 leaf는
  비어 있다. drain 뒤에는 `complete`이고 `unlink_held` 0, `unlinked` 합 1이다. anchor root는 split leaf 둘 중 하나이고
  level 0이며, 목록은 정렬한 {다른 leaf, 옛 root}다. 뒤의 INSERT는 그 root leaf에 들어간다. 시험 이름이 말하는 성질
  (split은 거래의 rollback에서 살아남는다)을 drain 전에 판정한다. 계약 1095에 U3 뒤의 결과를 적었고,
  1862-1876 문단은 "기존 시험 둘"로 SPL-A03을 더했다.
- 권고 1: **해결.** L09를 C 직접 호출 시험으로 만들었다(unlink_rows.rs:1753-1939). NOT_RUN과 "LOAD는 split과 같은 함수"라는
  근거는 지웠고, "계획과 다르게"(1880-1887)에 C 직접 호출로 바꾼 이유와 A 쪽 FlushRequired를 UNL-A01이 본다는 점을 적었다.
- 권고 2: **해결.** (a) note 위치가 설계 문구와 다르다는 점을 "계획과 다르게"(1888-1892)에 적었다. (b) note·ControlCut
  불변식을 계약(1734-1739)과 code comment(structural_unlink.rs:63-67)에 적었다. 실패 시험은 근거를 대고 두지 않았다.
  (c) 경계를 control 상수에서 계산한다: `ROWS_END = BANK_END / SLOT_BYTES * SLOT_BYTES`, named end
  `(STRUCTURAL_SLOT + 1) * SLOT_BYTES`, `BANK_END`는 `ControlCut` digest가 쓰는 catalog_live.rs:38의 상수다. const assert는
  :77-79, 시험은 source_tests.rs:467-495다. 시험의 `36 * 1_664`는 literal이지만 `validate_live` 자신도 literal
  `36 * SLOT_BYTES`(history.rs:604)라 따올 상수가 없다.
- 권고 3: **해결.** 1827-1831에 치환을 U1 미결 항(`decode_anchor`) 하나에만 쓴다고 적었다. `observe`의 다른 중첩 호출은
  U3가 다시 검산하지 않았고 PASS도 주장하지 않는다고 적었다. 인용한 `local_source_carrier_bytes`의 설명(insert.rs:453-455,
  "Shared codec locals ... still require the root's source census")과 맞는다.
- 권고 4: **해결.** `unlink_next` 본문은 charge 밖이고 따로 세지 않는다는 문장을 1805-1806에 넣었다.
  reclamation 286 "자원 (A)" 문단 끝에는 U3 네 지점, frame-static 최대 3,738B, U3 census link를 더했다.
- 권고 5: **해결.** 새 `reachable`(unlink_rows.rs:115-145)은 anchor의 root에서 level마다 `internals`의 child로 내려가
  도달 leaf 집합을 만든다. 등록된 leaf는 모두 도달 집합이거나 anchor 목록 가운데 정확히 하나에만 속해야 하고, 도달한
  leaf는 모두 resident여야 한다고 단언한다. 단언이 통과하면 돌려주는 집합은 "등록 leaf ∖ 목록"(예전 helper)과 같고 순서도
  `leaves` 순서 그대로다. 그래서 판정이 약해진 UNL-L 시험은 없고, separator가 퇴역 leaf를 가리키거나 목록 없이 끊긴 leaf는
  이제 실패한다. SPL-J02도 이제 product 보고와 따로 판정한다(split_rows.rs:749-757). root = 왼쪽 leaf, level 0, 목록
  {오른쪽, 옛 root}를 본다. rollback 직후의 `leaves` 비교(:759-762)는 unlink가 없어도 같은 값이다(빈 leaf가 정렬상 앞).
  하지만 바로 앞의 anchor 판정이 그 차이를 가른다. INSERT 13 뒤의 `[(right, []), (left, 7·8·13)]`(:802)는 unlink가 없으면
  `[(left, 7·8), (right, 13)]`이 되므로 route 판정이다.
- 권고 6: **해결.** `#unlink-u3-runs`(1900-1933)에 321개 batch의 구성, 반영 뒤 실행, 바뀐 기존 code의 시험족을 적었다.
  시험족 수는 `--list`로 확인했다: SPL-J01..J07 7, SPL-A01..A10 10, `catalog_live::structural` 7, `f4_a_` 6,
  `rdl_a08_` 4, `row::census_tests::` 6. 반영 뒤 product code의 동작 변화는 leaf LOAD의 보호 집합 하나뿐이다. 실제로
  unlink하는 시험은 UNL-L, SPL-A03, SPL-J02뿐이다(321 batch에서 `unlinked` 0을 기대하는 다른 drain 판정이 모두 통과했다).
  그래서 반영 뒤 다시 돌린 `unlink_rows:: spl_a03_ spl_j02_` 묶음이 맞는 범위다. fmt·clippy 줄의 commit 표기는 NIT 2.
- NIT 1: **해결.** residency 36,063 → 36,511B(+448B = 64B × 7). peak는 prepare 36,561B 그대로다.
- NIT 2: **해결.** 1802-1804. "frame-static(caller term 472B 포함) + 344B"로 고쳤다. 다섯 값 모두 frame-static + 344와 같다
  (3,738→4,082, 3,730→4,074, 3,249→3,593, 3,289→3,633, 3,635→3,979).
- NIT 3: **해결.** "39행"을 "이 구역에 들어가는 1,664B 행 전부(39개)"로 바꾸었다. 이름 붙은 행은 `STRUCTURAL_SLOT` 34까지,
  canonical 비교는 36행이라고 나누어 적었다(1727-1733, code :54-60).
- NIT 4: **해결.** 정수 필드는 독립 LE bytes, anchor 필드는 `PageRef::encode`와 대조한다고 code doc(:1243-1248)과
  계약 1855에 적었다.
- NIT 5: **부분.** `delete_prune_note`의 Unchanged `Invariant`(retire.rs:458-461)와 `note_emptied_leaf`의 버림
  (structural_unlink.rs:251-253)에 의도를 적었다. 다만 뒤 문장의 "다음 호출이 그 칸을 읽는다"가 code와 다르다(이번 NIT 1).
- NIT 6: **해결.** 1766-1768. cut 불일치는 bare `RetentionCause::Busy`라 note를 지우고, 닿지 않는 이유를 적었다.
  `Frame(Stale)`은 note를 남긴다고 나누어 적었다.
- NIT 7: **해결.** 1126 상태 줄을 "U3 구현·memory 시험 PASS, U4 구현 없음"으로 고쳤고, 934 SPL-J02 행에 "(U3 전)"을 붙였다.
  schedule baseline 120행의 "native fault cut U4"도 설계 구현 단계표(1394)와 맞는다.

## 필수

없음.

## 권고

없음.

## NIT

1. **해독할 수 없는 note 칸을 "다음" 호출이 읽는다는 문장이 code와 다르다.**
   - 위치: `structural_unlink.rs:251-253`("the drain's next `unlink_next` or `unlink_leaf_live` reads it and ends Fatal
     (Census)"), `dev3-tree-smo.md:1742-1743`.
   - code가 하는 일:
     - `unlink_next`는 칸 0만 읽는다(`note::get(self.control, 0)`, :212).
     - `unlink_leaf_live`의 `note::find`(:162-174)는 첫 일치에서 돌아온다.
     - `note::remove`(:179-197)도 일치한 칸에서 멈추고 뒤 칸을 당긴다.
   - 그래서 깨진 칸이 k > 0에 있으면 다음 호출은 그 칸을 읽지 않는다. 앞의 note가 하나씩 소비되면서 칸이 0까지 올라온
     뒤에야 Fatal(Census)이 된다. 그동안 `note::record`(:121-140)는 빈 칸에 닿기 전에 깨진 칸에서 `Err`를 내므로 새 note가
     모두 조용히 버려진다.
   - 영향: 결론(결국 Fatal)은 맞다. 깨진 칸은 `record`만 쓰는 구역에서 메모리 손상 없이는 생기지 않으므로 동작
     결함은 아니다.
   - 고칠 것: "앞의 note가 모두 소비된 뒤 `unlink_next`가 그 칸을 읽어 Fatal(Census)로 끝나고, 그때까지 새 note는 버려진다"로
     적는다.
2. **실행 기록의 fmt·clippy 줄에 commit이 없다.**
   - 위치: `dev3-tree-smo.md:1932`. 같은 절의 다른 행은 실행 시점(반영 전 321 batch, "D2 1회차 반영 뒤" 묶음)을 밝힌다.
     이 줄은 그렇지 않다.
   - 작성자 scratchpad의 `clippy-u3.log`는 06:57에 끝났고 `3b0fd72`는 08:19 commit이다. 그 사이 반영분은 시험 code를
     많이 더했다(`reachable`, L09, SPL-A03·J02 판정, const assert).
   - 내가 `3b0fd72` 사본에서 fmt, clippy(`--workspace --all-targets --all-features --locked -- -D warnings`), 문서 검사를
     돌렸고 모두 PASS다. 그러니 사실은 맞고, 기록에 대상 commit만 적으면 된다.

## 직접 확인한 것

- 사본: `mkdir -p …/scratchpad/review-u3-r2 && git -C …/wt-u3 archive 3b0fd72 | tar -x -C …/review-u3-r2`,
  `CARGO_TARGET_DIR=…/review-u3-r2/target`, `source …/scratchpad/jenv2.sh`. 끝에
  `git -C …/wt-u3 archive 3b0fd72 | tar -d -C …/review-u3-r2`로 내용이 같음을 확인했다(mod time만 다름).
- build: `cargo test -j 2 -p xadb-engine --lib --no-run`, 1분 51초, 성공(`engine-e4e141083d864921`).
- census·단위(live 아님): `engine-… --test-threads 2 --nocapture unl_a03 unl_c03 unl_a02 unl_a01 live_row_retirement_fits
  budget_tests:: lrc_a0 f4_a_ rdl_a08 s2w_c0 unl_r04` → **38 passed**. 작성자 필터(`unl_a03 unl_c03 unl_a02
  live_row_retirement_fits budget_tests::`)는 `--list`로 20개이고 기록과 같다. 출력과 계약 값:
  - `UNL_A03_{MEMORY,NATIVE_TYPE}`: carriers 961, residency **36,511**, prepare 36,561, install 31,053, peak **36,561**,
    shell 928, note 6,724, row_retirement 39,694, charge 44,304, room 3,647, catalog peak 44,304. 계약 1790-1800과 같다.
    36,561 + 4,096 = 40,657, 여유 3,647. 39,694 + 4,096 = 43,790, 여유 514(`LIVE_ROW_RETIREMENT_* room=514`).
  - `UNL_C03_{MEMORY,NATIVE_TYPE}`: shared 20,208/20,216, notes 65,088..65,536, named_end 58,240, rows_end 64,896,
    bank_end 65,536, control 196,608, charge 44,304. 계약 1857과 같다. 59,904 = 36 × 1,664, 합 64,512/64,520.
  - `UNL_C03_A`: decode 1,867, write_pass 4,132, producer 19,436, bound 19,456, room 20.
  - `cleanup_site` share/frame: unlink_next 2,729/3,249, unlink 2,801/3,289, unlink_try_flush 2,993/**3,738**,
    unlink_continue_flush 2,985/3,730, try_flush 3,098/3,635. `cleanup_caller=472`, `drain_caller=816`.
  - drain 항목(autocommit·explicit 계열 12회 같음): try_flush 3,979(117), unlink_next 3,593(503), unlink 3,633(463),
    unlink_try_flush **4,082(14)**, unlink_continue_flush 4,074(22). 계약 표 1808-1814와 reclamation 286과 같다.
  - S2W: preflight c_margin 182, `row_lifecycle=40883`, S2W-C04 recovered_peak 65,392/65,400. 계약 1800·1833과 같다.
- live 시험 3개(한도):
  1. UNL-L09, 고친 code: `--exact …unlink_rows::unl_l09_flush_required_round_trip_keeps_the_parent` → **1 passed**
     (181.6초). `UNL_L09 anchor=12 parent=3 leaf_frame=33 victim=1 victim_page=page 1 other=page 4`. 작성자 기록과 같다.
     `with_fresh!`의 `frames = 34`는 `frames.len()` = 34이고, `select_load`는 1..len만 훑는다(reader_load.rs:249, 318).
     그래서 `(1..34)`의 pin이 anchor·P·X 밖의 모든 frame을 덮고, 보호된 anchor를 빼면 P(clean)와 X(dirty)만 남는다.
  2. UNL-L09, probe(보호 집합을 1회차 `[anchor, leaf]`로 되돌림): `structural_unlink.rs:440`의
     `&[anchor, parent, leaf]`를 `&[anchor, leaf]`로 sed 치환하고 다시 build했다(17.5초). 같은 명령은 **FAILED**(173.2초),
     `unlink_rows.rs:1878:22`(첫 `unlink_leaf_live`의 unwrap)에서 `RetentionError { phase: Prepare, cause:
     Frame(DirtyError { cause: MissingTarget, effect: Unchanged }), effect: Unchanged }`가 났다. 이 원인은 `unlink_busy`의
     목록에 없어 note를 버리는 쪽이다. 1회차 필수 1의 시나리오가 그대로 재현됐다. 끝나고 저장해 둔 원본으로 되돌렸고,
     `cmp`와 `git show 3b0fd72:…`로 같음을 확인했다.
  3. SPL-A03: `--exact …auto_split::spl_a03_split_survives_the_rollback_of_its_transaction` → **1 passed**(36.6초).
  - SPL-J02와 나머지 UNL-L은 한도 때문에 돌리지 않았다. 작성자의 반영 뒤 묶음(13 passed, L09 배치만 실패한 뒤 34 frame에서
    PASS)을 받아들이고, 새 판정은 정적으로 확인했다(권고 5 항).
- push gate 항목(사본, `3b0fd72`):
  - `cargo fmt --all --check` → 0.
  - `cargo clippy -j 2 --workspace --all-targets --all-features --locked -- -D warnings` → 0(1분 59초, warning 0).
  - `python3 -B devdoc/verify_documents.py` → `PASS`, markdown 287. 이 검사는 사본의 `devdoc/document-validation.json`
    날짜를 다시 썼다. 그래서 그 파일을 `git show 3b0fd72:…`로 되돌렸다.
- 정적 확인: `unlink_resident`(structural_unlink.rs:397-462)의 LOAD·읽기 순서, `prepare_unlink`(:705-)와
  `read_unlink_before`, `apply_existing_n`(catalog_lifecycle.rs:566-), `check_split_debt`, `replay_scope`, `select_load`·
  `load_eligible`. 그 밖에 note 모듈 상수와 `ControlCut`(catalog_live.rs:38, 416-431), `validate_live`의 36행 비교
  (history.rs:604)를 보았다. `reachable`·`internals`(unlink_rows.rs:115-175), `leaves`·`anchor_of`(split_rows.rs:39-86),
  SPL-A03·SPL-J02 판정, `unlink_notes`(driver.rs:1664-1706)와 FlushRequired 반복도 보았다. flush마다 clean frame이 늘고
  unlink 재시도 사이에 page 쓰기가 없으므로 수렴한다.
- 끝난 뒤 사본의 target 디렉터리(`…/review-u3-r2/target`, 3.7G)를 지웠다. 사본 source와 로그(`…/scratchpad/r2-*.log`,
  원본 사본 `…/scratchpad/r2-structural_unlink.rs.orig`)는 남겼다.

## NIT 반영 확인 (7e97e61)

`git -C …/wt-u3 diff 3b0fd72 7e97e61`을 읽기만 했다(3 files, +19/−13: `structural_unlink.rs` 주석, `dev3-tree-smo.md`,
`document-validation.json`). code 동작은 바뀌지 않았으므로 build는 다시 하지 않았다.

- NIT 1: **해결.** `structural_unlink.rs:251-255`와 `dev3-tree-smo.md:1742-1744`가 이제 code와 맞는다. 해독할 수 없는 칸에
  닿는 `record`의 새 note는 버려진다. `unlink_next`는 칸 0만 읽고, `unlink_leaf_live`의 `find`는 칸 0부터 처음 맞는
  note까지 읽는다. 그래서 둘은 앞의 note를 다 쓴 뒤에야 그 칸에 닿고, 그때 Fatal(Census)로 끝난다.
- NIT 2: **해결.** `dev3-tree-smo.md:1933-1935`에 fmt·clippy·문서 검사의 대상 commit `3b0fd72`와 D2 2회차의 재실행을 적었다.
  NIT 반영분이 주석과 문서뿐이라는 점도 적었다.
- U3 상태 줄(1719)의 새 `review.md` link는 지시대로 보지 않았다. 다만 그 link 때문에 이 commit의
  `document-validation.json`은 `status: FAIL`(missing link)이다. 그 폴더를 더한 뒤 문서 검사로 다시 만들어야 한다.
  지시대로 finding으로 세지 않는다.
- 판정은 **ACCEPT(필수 0, 권고 0) 그대로**다. 남은 NIT는 없다.

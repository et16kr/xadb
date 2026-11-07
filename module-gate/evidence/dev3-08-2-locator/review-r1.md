Verdict: CONDITIONAL

# 08-2b(Row locator) 독립 D2 검토 1회차

## 대상

- head `fc1fc94`(branch `wip-082b`, worktree `/home/user/xadb-082b`), base `bb5c193`(08-2a branch의 head, 열린 PR #62). 검토 범위는 `git diff bb5c193 fc1fc94` 전체다. 08-2a 자체는 검토하지 않았다.
- 검토 중에 branch가 `38f680a`로 한 commit 나아갔다. `38f680a`는 `dev3-cursor-locator.md` §10의 실행표와 `document-validation.json`만 바꾸고, 아래 Rust blob 13개는 `fc1fc94`와 같다(`check_module_gate.py files`의 두 출력이 같음을 확인했다). 판정은 `fc1fc94`에 대한 것이며, 문서 지적은 `38f680a`에서도 그대로인지 따로 적었다.
- 검토한 Rust blob 13개(`python3 scripts/check_module_gate.py files --base bb5c193 --head fc1fc94`):
  - `crates/engine/src/storage/disk.rs` `41674341cf2a9a0d6eb62ba614bf38245f5b1ec6`
  - `crates/engine/src/storage/row/read/driver/live.rs` `ce139fb337843082602c56c37f02cc42cc86dcd6`
  - `crates/engine/src/storage/tree_page/search.rs` `25a44d1c27a23dc696c2f0ee1cfa6a5240f2a51b`
  - `crates/engine/src/storage/wal_io/disk_api.rs` `05d6fbda8deaff6e6971c831ce4c0f0236d151b5`
  - `crates/engine/src/storage/wal_io/plan4_apply.rs` `4b4261d1e228d5d936c9154e7462c759a34e88c5`
  - `crates/engine/src/storage/wal_io/plan4_apply/row.rs` `0e841c32b9f2c7c98523ea2839d5eef1bfbf73ec`
  - `crates/engine/src/storage/wal_io/plan4_apply/row/locator.rs` `6058f7dca9837ec9ce16f536e79c27ba5099ec03`
  - `crates/engine/src/storage/wal_io/plan4_apply/row/locator_tests.rs` `3279413f9c602463800dda91e23ef0896e3ecf82`
  - `crates/engine/src/storage/wal_io/plan4_apply/row/read.rs` `62eb18ff14c31521cd85d72d10e63f4e93440735`
  - `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/locator_smo.rs` `142ef9568faaff997c3b5754e7fc4ffe69c0619d`
  - `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/unlink_rows.rs` `38c6b72506c2c3b1f35c9e636150e3f415d20a0a`
  - `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_read_census.rs` `84b9f3768fbc07c4a30293027e9e106a788ad9c0`
  - `crates/engine/src/transaction/concurrency.rs` `0edc9854d7fc28f7a92cbd22e044c36e85810c07`
- 읽은 것:
  - diff 전체(Rust 13개, `dev3-cursor-locator.md`, `dev3-concurrency-admission.md` §14, `dev3-schedule-baseline.md`, `document-validation.json`). README와 `documents.json`은 이 diff에서 바뀌지 않았고, 새 문서가 없으므로 바뀔 필요도 없다.
  - `dev3-cursor-locator.md` §1, §5 L1–L12와 근거 문단, §6, §7, §8, §10 전문.
  - 08-2a D2 기록 `module-gate/evidence/dev3-08-2-cursor/review-r1.md`, `review.md`. 그 권고 1(동시성 계약 갱신)과 NIT 4(bounds 손상을 revision 분류보다 먼저, 옛 revision과 같은 revision 모두)는 §14와 `locator.rs:163`, `locator_tests.rs:166-179`로 반영됐다.
  - owner 문서: `dev3-task-06-tree-rows.md` R9(`:31`), §5.5.5(`:141`), §6 자원표(`:153`), T06-12/13/16(`:203`, `:206`); `dev3-tree-smo.md`의 revision 규칙(`:59`, `:1165-1166`)과 current-leaf 소비자(`:909`); `dev3-resource-bounds.md:86`; `dev3-concurrency-admission.md` §3, §7, §12; `dev3-schedule-baseline.md` §3, §4, §6 규칙 2; AGENTS.md; `module-gate/README.md`.
  - code: `read.rs`의 `row_operation`, `page`, `page_located`, `check_table`, `anchor_root`, `route_internal`, `route_root`, `advance_route`, `load_leaf`, `load_leaf_located`, `damaged_hint`, `leaf_holds`, `issue_page`, `row_test_load_inventory`; `search.rs`의 `select_child`, `route_row_step`, `row_leaf_holds`; `locator.rs` 전문; `concurrency.rs`의 `PageConcurrency`; `live.rs`의 `LocatedSource`, `read_point_located_live`, `test_load_callers`, `test_nonload_caller`; `session_fence.rs`(되돌리는 API가 없음).
- 실행. 모두 `git archive fc1fc94`를 scratch에 풀고 따로 둔 `CARGO_TARGET_DIR`로 했다. `jenv.sh`를 읽고 `--exact <전체 경로> --test-threads=1`로 돌렸다. 원 worktree와 `/home/user/xadb`에는 쓰지 않았다(`check_module_gate.py`의 `files`·`plan`·`check`를 읽기 전용으로 돌렸고, 그 뒤 `git status`가 비어 있음을 확인했다).
  - build: `cargo test -p xadb-engine --lib --no-run` 1분 35초.
  - LOC-B01 PASS(0.01 s), LOC-B02 PASS(0.01 s), LOC-J01 PASS(199.2 s), LOC-J02 PASS(170.8 s), LOC-J03 PASS(117.1 s), S5R-C01 PASS(0.01 s), S5R-C02 PASS(0.01 s), S5R-C03 PASS(0.78 s), GA07 PASS(0.23 s).
  - `--nocapture`로 다시 본 값: LOC-B02는 `bytes=30856 header=72 entry=120`, 32개 합 987,392 B다. S5R-C01·C02 두 종류 모두 point.row_load 34,382, cursor.row_load 34,574, located.catalog_load 34,470, located.row_load 34,551, located.row_route 8,284, located.value_before_load 34,334, located.lookup 3,779, located.record 3,330, located.holds 7,726, Full 34,588 B다. §10의 표와 같다.
  - core 크기: compile-time probe `const _: [(); 0] = [(); size_of::<PageConcurrency<'static>>()];`를 base `bb5c193`와 head 사본에 넣고 `cargo check -p xadb-engine --lib`와 `--lib --tests`를 돌렸다. 두 commit 모두 제품 640 B, 시험 cfg 824 B다. "padding에 들어가 크기 불변" 주장은 맞다.
  - `cargo fmt --all --check` PASS, `cargo clippy -p xadb-engine --all-targets --all-features --locked -- -D warnings` PASS(1분 11초), `python3 -B devdoc/verify_documents.py` PASS(사본 안에서).
  - mutant와 probe(세 번째 사본, `XADB_MUT` 환경변수로 하나씩 켬. 결과는 아래 표와 필수·권고에 썼다).
  - 끝난 뒤 build target 셋과 사본 셋(검토용, mutant용, base 크기 probe용)을 지웠다. 실행 log는 이 파일 옆에 남겼다(`log_*.txt`, `mut_*.txt`, `probe_j01_*.txt`, `gate-check.txt`).

| mutant / probe | 시험 | 결과 |
| --- | --- | --- |
| 없음 + J03 probe(level 0 읽기 뒤 entry 출력) | LOC-J03 | PASS(117 s). level 0 읽기가 8개 entry를 revision 3(축소 revision), leaf 33(root leaf)로 다시 기록함 |
| L8대로 level 0 기록을 끔 | LOC-J03 | PASS(116 s). entry는 revision 2로 남음. 시험이 두 동작을 구별하지 못함 |
| recheck 생략(hint leaf에 RowId가 없어도 그 leaf로 발급) | LOC-J03 | FAIL(118 s): Missing(None)이 나옴. 잡힘 |
| recheck의 `confirm` 오류 무시 | LOC-J03 | FAIL(117 s): 행(Some)이 나옴. 잡힘 |
| `lookup`의 `DamagedHint`를 miss로(`read.rs:779`) | LOC-J03 | PASS(117 s). 살아남음 |
| stale을 hit로(`locator.rs:166`) | LOC-J01 | FAIL(170 s): split 뒤 statement의 located read가 `Census/Invariant/CallerMustResolve`로 끝남. 잡힘(LOC-B01도 잡음) |
| J01 끝에 bounds가 RowId를 빼는 손상 entry를 넣은 probe(원 code) | LOC-J01 | PASS(199 s). 손상 읽기는 `Err(Source(Census/Invariant/CallerMustResolve))`, fence됨, 계수는 lookups만 +1 |
| 같은 probe + `lookup` 손상→miss mutant | LOC-J01 | PASS(198 s). 손상 읽기가 `Ok(Some(행 값))`을 돌려주고 fence되지 않음. 이 사례를 단언으로 바꾸면 mutant가 잡힌다 |

## 판정

필수 2, 권고 4, NIT 3. **CONDITIONAL.**

locator의 정답 경로는 건전하다. 다음을 code에서 확인했다.

- key는 anchor의 `PageIdentity` 전체와 RowId다(`locator.rs:162`, `:197`, `:217`). database·file·page_no·incarnation·owner·owner_generation이 모두 비교된다.
- 같은 key의 entry는 먼저 손상 검사를 받는다. bounds가 RowId를 포함하지 않거나 기록 revision이 지금보다 크면 `DamagedHint`이고, 그다음에 hit·stale을 나눈다(`locator.rs:163-177`). `load_leaf_located`는 이것을 fatal Invariant로 바꾼다(`read.rs:779`, `:848-854`).
- hit는 hint leaf만 LOAD하고, `leaf_holds`가 allocator membership을 먼저 본 뒤 `row_leaf_holds`로 `route_row_step`과 같은 decode·bounds 검사와 정확한 검색을 한다(`read.rs:860-915`, `search.rs:165-181`). member가 아니거나 bounds 밖이면 fatal이다. 비어 있거나 RowId가 없으면 root부터 다시 route하고, 그 결과가 hint와 같아야 한다(`read.rs:823-842`, `locator.rs:187-207`). 그 뒤는 `issue_page` 그대로다(anchor revision 재확인, leaf bounds 검사, `record_table_read`).
- 오류가 miss가 되는 경로는 없다. `leaf_holds`의 MissingTarget은 Unchanged Busy이고(`read.rs:830-832`), 나머지 오류는 그대로 돌아간다.
- revision은 operation read gate가 고정하고, 모든 SMO가 정확히 1 올리며(`dev3-tree-smo.md:59`, `:1165-1166`), `SessionFence`는 되돌릴 수 없다. 그래서 같은 operation 안의 recheck에서 revision이 바뀌어 거짓 Invariant가 나는 일은 건강한 세션에서는 없다.
- 행 key는 member leaf 하나에만 있으므로, hint leaf가 RowId를 가지면 그 leaf가 맞는 leaf다. 손상된 hint가 틀린 행을 돌려주거나 행을 Missing으로 숨기는 경로는 찾지 못했다.
- 다른 core의 locator는 `check_table`보다 먼저, 아무 LOAD 전에 Unchanged `BindingMismatch`다(`read.rs:367-372`).
- 회계: 몫은 할당 전에 받고, `try_reserve_exact` 실패면 돌려준다(`locator.rs:95-110`). Drop이 돌려준다(`:85-89`). `fetch_update`로 32를 넘지 않고 0 아래로 가지 않는다(`concurrency.rs:241-256`). lock·fence·admission과 순서가 없다. 표는 `resize`와 `into_boxed_slice`에서 다시 할당되지 않는다(capacity = len). 상수 검사(`locator.rs:24-31`)와 LOC-B02가 한 locator ≤ 32,768 B, 합 ≤ 1 MiB를 보인다.
- census 항은 정직하다. `holds`의 `read(...) + leaf_holds - route_step` 계산은 max 항 안의 치환보다 크거나 같으므로 보수적이다. point·cursor phase는 그대로다(B의 max 항 2128 B와 C의 parent max 640 B가 located 항보다 커서).
- 시험의 기대값은 대부분 독립 입력에서 온다. 행과 값은 상태 기계(`kept_values`, `bigs`)에서, hint leaf는 pool에서 읽은 tree(`catalog_test_with_pool(reachable)`)에서, 계수는 literal에서 온다. 예외는 권고 4다. mutant 가운데 recheck 생략, `confirm` 오류 무시, stale을 hit로는 잡힌다(위 표). "hint 때 기록" mutant는 같은 값을 다시 쓰는 것이라 관측할 수 없는 동치 mutant다. hit의 entry는 route와 같고, `Rechecked`는 `confirm`이 같음을 보장한다.
- disk API 추가는 `RowLocator`, `LocatorCounts` 둘뿐이다. 나머지 새 item(`lookup`, `confirm`, `record`, `serves`, `Hint`, `DamagedHint`)은 `pub(super)`로 `row` 모듈 안에 있다. 최소다.
- 일정 산술은 맞다. 35 + 1 = 36, 36/86 = 41.86%, 미적립 50일, 08 행 적립 3일(08-1 2일, 08-2 1일), 단위 18 = 36일이다. 08-2는 08의 마지막 항목이 아니므로 항목 조건으로 08-2b 병합 때 적립한다는 서술도 §3·§6 규칙 2와 맞는다. 다만 §4의 "각 단계는 독립 D2의 필수 지적을 해소했다"는 아래 필수 둘이 해소되어야 참이 된다.

필수 둘은 정답을 바꾸는 결함이 아니다. 하나는 계약과 code가 다른 것(level 0 기록)이고, 하나는 계약이 주장하는 T06-13의 mutant 거절이 한 종류의 손상에서 성립하지 않는 것이다. 둘 다 작게 고칠 수 있다.

## 필수

1. **level 0 tree의 route도 기록한다. 계약 L8은 "level ≥ 1일 때만"이라고 한다.**
   - code: `load_leaf_located`는 root level이 0이면 조회하지 않지만 `located`를 `Walked`로 둔다(`read.rs:766-767`). `page_located`는 `Walked`이면 level과 무관하게 `record`한다(`read.rs:382-384`). level 0 route의 bounds는 무한이라 `record`의 `contains` 검사도 통과한다.
   - 계약: L8(`dev3-cursor-locator.md:86`)은 "그 route를 기록한다(level ≥ 1일 때만)"이다. L6(`:84`)의 "건너뛸 internal page가 없음"도 같은 취지다. §6 LOC-J03(`:116`)은 "다시 split 뒤 옛 hint는 stale(revision이 두 번 이상 움직임)"이라고 하고, 시험 주석(`locator_smo.rs:686-697`)도 같다.
   - 실제: J03 probe에서 축소 뒤 level 0 읽기가 8개 entry를 모두 revision 3, leaf 33(root leaf)로 덮어썼다. 지운 행의 entry도 그렇다. 그래서 다시 split한 뒤의 stale은 한 revision 차이이고, "옛 hint"는 이미 level 0에서 다시 쓴 것이다. L8대로 기록을 끈 변형에서도 J03은 PASS했다. 즉 시험은 이 규칙을 고정하지 않는다.
   - 영향: 답은 바뀌지 않는다. 그러나 level 0 entry는 hit가 될 수 없고(level이 바뀌려면 revision이 바뀜), 256 slot을 차지해 level ≥ 1 table의 hint를 evict한다. 일정 기록대로 제품 table은 10-1 전까지 split하지 않으므로 지금 제품의 모든 table이 level 0이다. L10의 evictions·working_set은 T06-16·PF09가 쓰는 계측인데, 쓸모없는 entry가 섞인다.
   - 고칠 것: root level이 0이었으면 기록하지 않는다(예: `Located`에 root leaf용 variant를 두거나, `load_leaf_located`가 root level을 함께 돌려준다). LOC-J03에 축소 뒤 level 0 읽기가 entry(revision 2)와 계수를 바꾸지 않는다는 단언을 더한다. code를 그대로 두려면 L6·L8·LOC-J03·시험 주석을 code대로 고쳐야 하지만, 위 영향 때문에 code를 고치기를 권한다.

2. **`lookup`의 손상(bounds 밖, revision 역행)을 miss로 바꾸는 mutant를 어떤 시험도 거절하지 않는다. 계약은 T06-13의 이 거절을 주장한다.**
   - `read.rs:779`의 `Err(_) => return Err(self.damaged_hint())`를 `Err(_) => {}`로 바꾸면 손상 entry가 miss가 되고, root부터 route해 행을 돌려준다. 이것이 L9가 금하는 "오류를 miss로"다.
   - LOC-B01은 `lookup`의 반환값만 본다(`locator_tests.rs:106`, `:171-172`). 그 값을 fatal로 바꾸는 C 경로는 보지 않는다.
   - LOC-J02의 손상은 bounds를 그대로 두고(`locator_smo.rs:647-652`), LOC-J03의 손상은 무한 bounds를 쓴다(`:915-920`). 둘 다 `lookup`이 `DamagedHint`를 내지 않으므로 이 경로에 닿지 않는다. 위 표의 mutant 실행이 이를 보인다(LOC-J03 PASS).
   - 그런데 §6의 T06-13 대응(`dev3-cursor-locator.md:119`)은 "손상을 miss로 숨기는 mutant 거절은 LOC-J02·J03의 두 손상이다(오류를 miss로 바꾸거나 recheck를 생략하는 mutant는 그 시험에서 행이나 Missing을 내어 실패)"라고 한다. T06-13(`dev3-task-06-tree-rows.md:203`)은 인수 의무다. L6·L9의 손상 분류에는 end-to-end 사례가 없다.
   - 고칠 것: LOC-J01은 fence 없이 끝나므로(`locator_smo.rs:379`) 그 끝에 세션 하나를 더 쓰지 않고 사례를 붙일 수 있다. `test_redirect`로 `keys[0]`의 entry에 RowId를 빼는 bounds(예: lower = RowId + 1)를 넣고, located point read가 `Census/Invariant/CallerMustResolve`, 출력 없음, fence임을 단언한다. 내 probe가 이 사례를 원 code와 mutant에서 돌렸다(위 표). 원 code는 `Census/Invariant/CallerMustResolve`와 fence로 맞게 동작하고, mutant는 행 값을 돌려주며 fence하지 않는다. mutant에서는 그 뒤 걸어간 route가 같은 key의 손상 entry를 덮어써서 손상 흔적도 사라진다. 그 뒤 §6의 T06-13 문단에 이 사례를 더한다.

## 권고

1. **`page_located`의 core 검사(`read.rs:367-372`)를 시험이 지나지 않는다.**
   - LOC-B01은 `serves_core`만 부른다(`locator_tests.rs:184-185`). 모든 LOC-J는 table과 같은 core로 locator를 만든다(`locator_smo.rs:168`, `:443`, `:733`). 그래서 이 검사를 지우는 mutant는 살아남는다.
   - 검사가 없으면 다른 세션의 hint가 쓰인다. `leaf_holds`와 recheck 덕분에 틀린 행은 나오지 않지만, 옛 bounds 때문에 건강한 세션이 거짓 fatal로 fence될 수 있다. L4가 막으려는 것이 이것이다.
   - 두 번째 저장 세션 없이 시험할 수 있다. `PageConcurrency::new(&SessionFence::new())`로 만든 core의 locator로 LOC-J01 안에서 `read_point_located_live`를 한 번 부르고, Unchanged `BindingMismatch`, 출력 없음, 그 locator의 계수 0, fence 없음을 단언하면 된다.

2. **module-gate 기록이 아직 없다.**
   - `fc1fc94`에도 `38f680a`에도 record JSON이 없다. `38f680a`의 §10은 `module-gate/records/20261107-dev3-08-2-locator.json`을 가리킨다. 읽기 전용 `check_module_gate.py check --mode merge`는 13개 path 모두에 "no scoped CI evidence"와 "review missing"(D2 common 7개, D1 6개)으로 FAIL한다.
   - 병합 전에 record를 더하고 `rust_tests`를 대응시킨다. 예: `concurrency.rs` → LOC-B02, GA07; `locator.rs`·`locator_tests.rs` → LOC-B01·B02와 LOC-J; `read.rs`·`search.rs`·`live.rs`·`disk.rs`·`disk_api.rs`·`plan4_apply.rs`·`row.rs`·`locator_smo.rs`·`unlink_rows.rs` → LOC-J01..J03; `read.rs`·`live.rs`·`row_read_census.rs` → S5R-C01..C03.
   - review의 `files`에는 필수를 고친 뒤의 blob을 쓴다. 지금 13개 blob은 필수 1·2를 고치면 `read.rs`와 `locator_smo.rs`가 바뀐다.

3. **몫 반환이 type으로 묶여 있지 않다**(`concurrency.rs:251-256`, §14 `dev3-concurrency-admission.md:200`).
   - `return_locator_share`는 `pub(crate)`이고, 쥔 몫이 없어도 조용히 아무것도 하지 않는다. crate의 다른 code가 받지 않은 몫을 돌려주면 33번째 locator가 생기고, 두 번 돌려줘도 보이지 않는다.
   - §14는 "Drop과 실패한 생성만 부른다"고 하지만 이것은 약속일 뿐 강제되지 않는다.
   - `take_locator_share`가 non-Clone 몫 token(core 참조와 private field)을 돌려주고 그 Drop만 반환하게 하면 ledger가 type으로 지켜진다. 최소한 0에서의 반환은 `debug_assert`로 드러내기를 권한다.

4. **LOC-J는 기록 revision을 locator 자신에서 읽는다**(`locator_smo.rs:193-195`, `:465-467`, `:766-768`).
   - 그 값으로 다른 entry와 이후의 `revision + 1`을 단언하므로, 기록 revision이 anchor의 실제 revision과 같다는 것은 hit 계수를 거쳐 간접으로만 보인다.
   - pool의 anchor에서 revision을 읽어(예: `catalog_test_with_pool`의 anchor 관찰자에 revision을 더해) 첫 기록과 split·unlink·축소 뒤 값을 독립 입력으로 대조하기를 권한다.

## NIT

1. slot hash에 anchor의 `page.file`이 들어가지 않는다(`locator.rs:141-146`). §10(`dev3-cursor-locator.md:147`)은 "anchor identity 전체의 hash"라고 한다. key 비교는 file까지 하므로 정답과는 무관하다. file도 섞거나 문구를 "file을 뺀 identity"로 고친다.
2. `fc1fc94`의 `document-validation.json`은 `dev3-cursor-locator.md`와 `dev3-concurrency-admission.md`의 hash·link 수가 낡았다. `38f680a`에서 고쳐졌다. 필수를 고치면 다시 갱신해야 한다.
3. `test_nonload_caller`의 "Opening closure's return" 항은 `Result<LiveSource>`를 센다(`live.rs` `test_nonload_caller`). located 경로의 closure는 `Result<LocatedSource>`를 돌려주어 8 B 크다. 지금은 max 항이 cursor 경로(2128 B)에서 나와 located 항(1968 B)보다 크므로 덮이지만, 그 관계가 바뀌면 덜 센다. 두 경로의 max를 쓰거나 주석에 그 전제를 적기를 권한다.

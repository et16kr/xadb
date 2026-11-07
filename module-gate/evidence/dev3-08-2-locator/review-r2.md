Verdict: ACCEPT

# 08-2b(Row locator) 독립 D2 검토 2회차

## 대상

- head `86c6c2a`(branch `wip-082b`, worktree `/home/user/xadb-082b`), base `bb5c193`(08-2a branch의 head, 열린 PR #62). 1회차는 `fc1fc94`에서 했고, 그 기록은 `module-gate/evidence/dev3-08-2-locator/review-r1.md`에 그대로 있다(sha256 `45b65b36a7a569378b19ba5f35ea6701c4a1b31ba043aaffbc447cf023bfba21`이 내 사본과 같다). 이번에 본 변경은 `git diff fc1fc94 86c6c2a` 전체다. 그 사이의 `38f680a`는 문서 실행표와 `document-validation.json`만 바꾼다.
- Rust blob 13개(`python3 scripts/check_module_gate.py files --base bb5c193 --head 86c6c2a`). 표시가 없는 것은 `fc1fc94`와 같다.
  - `crates/engine/src/storage/disk.rs` `41674341cf2a9a0d6eb62ba614bf38245f5b1ec6`
  - `crates/engine/src/storage/row/read/driver/live.rs` `b9634a3055875776cc2df2fcfa330b8d9b262676`(바뀜)
  - `crates/engine/src/storage/tree_page/search.rs` `25a44d1c27a23dc696c2f0ee1cfa6a5240f2a51b`
  - `crates/engine/src/storage/wal_io/disk_api.rs` `05d6fbda8deaff6e6971c831ce4c0f0236d151b5`
  - `crates/engine/src/storage/wal_io/plan4_apply.rs` `4b4261d1e228d5d936c9154e7462c759a34e88c5`
  - `crates/engine/src/storage/wal_io/plan4_apply/row.rs` `0e841c32b9f2c7c98523ea2839d5eef1bfbf73ec`
  - `crates/engine/src/storage/wal_io/plan4_apply/row/locator.rs` `c4bd41d72221560843d6aab7bb870eb461529129`(바뀜)
  - `crates/engine/src/storage/wal_io/plan4_apply/row/locator_tests.rs` `3279413f9c602463800dda91e23ef0896e3ecf82`
  - `crates/engine/src/storage/wal_io/plan4_apply/row/read.rs` `7d8fe5060e3da8da1e4bb138d1b6fd51375321f9`(바뀜)
  - `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/locator_smo.rs` `e70d78eeb042b1224137dc230ebe2a83f511a725`(바뀜)
  - `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/unlink_rows.rs` `38c6b72506c2c3b1f35c9e636150e3f415d20a0a`
  - `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_read_census.rs` `84b9f3768fbc07c4a30293027e9e106a788ad9c0`
  - `crates/engine/src/transaction/concurrency.rs` `ba31a22aa078b125d2ecdd742a59b21c7f0d2cdd`(바뀜)
- 읽은 것:
  - `git diff fc1fc94 86c6c2a`의 Rust 5개: `concurrency.rs`의 `LocatorShare`와 `take_locator_share`, `locator.rs`의 `RowLocator`와 `new`, `read.rs`의 `Located::Root`, `live.rs`의 `test_nonload_caller`, `locator_smo.rs`의 `anchor_revision`과 새 사례·단언.
  - 문서: `dev3-cursor-locator.md`의 L3·L6, §6(LOC-J01·J03 행, T06-13 문단), §9의 08-2b 문단, §10. `dev3-concurrency-admission.md` §14의 받기·반환 행. `document-validation.json`.
  - 1회차에 읽은 owner 문서와 code는 바뀌지 않은 부분이라 다시 읽지 않았다. 바뀐 곳과 맞닿는 `page.rs`의 `FileId`(값을 꺼내는 accessor가 없음)만 확인했다.
- 실행. 모두 `git archive 86c6c2a`를 scratch에 풀고, 따로 둔 `CARGO_TARGET_DIR`로 했다. `jenv.sh`를 읽고 `--exact <전체 경로> --test-threads=1 --nocapture`로 돌렸다. 원 worktree, 그 target, `/home/user/xadb`에는 쓰지 않았다. `check_module_gate.py`의 `files`·`check`는 읽기 전용으로 돌렸고, 그 뒤 `git status`가 비어 있음을 확인했다.
  - build: `cargo test -p xadb-engine --lib --no-run` 1분 39초.
  - 변경 시험:
    - LOC-B01 PASS(0.01 s), LOC-B02 PASS(0.01 s).
    - LOC-J01 PASS(202.6 s), LOC-J02 PASS(171.3 s), LOC-J03 PASS(118.5 s).
    - S5R-C01 PASS(0.01 s), S5R-C02 PASS(0.01 s), S5R-C03 PASS(0.83 s), GA07 PASS(0.20 s).
  - 출력 값:
    - LOC-B02는 `bytes=30856 header=72 entry=120`, 32개 합 987,392 B다.
    - LOC-J01의 마지막 계수는 lookups 71이다. 70에 손상 조회 하나가 더해졌다.
    - S5R-C01·C02 두 종류 모두 point.row_load 34,382, cursor.row_load 34,574, located.row_load 34,551, Full 34,588 B로 1회차와 같다. nonload_B는 5,688에서 5,696 B가 됐다(NIT 1).
  - 크기 probe: compile-time `const _: [(); 0] = [(); size_of::<T>()];`를 사본에 넣고 `cargo check -p xadb-engine --lib`와 `--lib --tests`를 돌렸다. 두 cfg 모두 `PageConcurrency`는 640 B(제품)와 824 B(시험), `LocatorShare`는 8 B, `RowLocator`는 72 B다. token은 core 크기도 locator header도 바꾸지 않는다.
  - `cargo fmt --all --check` PASS.
  - `cargo clippy -p xadb-engine --all-targets --all-features --locked -- -D warnings` PASS(1분 33초).
  - `python3 -B devdoc/verify_documents.py` PASS. 사본에서 다시 만든 `document-validation.json`의 문서 항목은 commit된 것과 모두 같다.
  - mutant: 네 번째 사본에서 `XADB_MUT` 환경변수로 하나씩 켰다.

| mutant | 시험 | 결과 |
| --- | --- | --- |
| level 0 root(`Located::Root`)도 기록(1회차 필수 1의 되돌림) | LOC-J03 | FAIL(49 s), `locator_smo.rs:947` 새 단언. left `Some((3, leaf 33))`, right `Some((2, leaf 13))`. 잡힘 |
| `lookup`의 `DamagedHint`를 miss로(`read.rs:783`, 1회차 필수 2) | LOC-J01 | FAIL(198 s), `:461` "a damaged entry is never a miss or a row: Some(212039)". 잡힘 |
| `page_located`의 core 검사 삭제(`read.rs:370`, 1회차 권고 1) | LOC-J01 | FAIL(197 s), `:412` "another core's locator: Some(212039)". 잡힘 |

- 끝난 뒤 build target 셋(검토, mutant, 크기 probe)과 사본 셋을 지웠다. 실행 log는 이 파일 옆 `r2-logs/`에, 1회차 log는 `r1-logs/`에 남겼다.

## 1회차 지적의 처리

- **필수 1(level 0 route도 기록해 L8과 다름): 해소.**
  - `load_leaf_located`는 `Located::Root`로 시작하고, root level이 1 이상일 때만 `Walked`로 바꿔 조회한다(`read.rs:769-772`). level 0에서는 `Root`로 돌아간다(`:813`). `page_located`는 `Walked`일 때만 기록한다(`:385`). `Rechecked`와 `Hinted`의 동작은 그대로다.
  - LOC-J03은 축소 뒤 level 0 읽기를 마친 다음 계수가 `(16, 8, 0, 8, 0)` 그대로이고, 모든 entry가 q2의 `(revision, holder(&before, row))`임을 단언한다(`locator_smo.rs:944-951`). 되돌리는 mutant는 이 단언에서 떨어진다(위 표).
  - L6(`dev3-cursor-locator.md:84`), §6 LOC-J03(`:116`), 시험 주석이 code와 맞는다. L8(`:86`)은 원래 문구대로 이제 참이다.
- **필수 2(`lookup` 손상을 miss로 바꾸는 mutant를 거절하지 않음): 해소.**
  - LOC-J01 끝에서 첫 행 entry의 bounds를 그 RowId를 빼는 값으로 바꾸고, located read가 `Census/Invariant/CallerMustResolve`, 출력 없음(macro의 0xA5 검사), fence, 계수는 lookups +1뿐임을 단언한다(`locator_smo.rs:430-480`). mutant는 `:461`에서 떨어진다.
  - §6의 T06-13 문단(`dev3-cursor-locator.md:119`)이 세 손상과 각각이 떨어뜨리는 mutant를 맞게 적는다. J02가 "hint leaf의 오류를 miss로" 바꾸는 mutant를 떨어뜨린다는 서술은 code로 확인했다. 그 오류를 miss로 바꾸면 recheck의 `Census/Invariant`이나 행이 나오고, J02는 `Source`와 `Tree`를 단언한다. 이 mutant는 돌리지 않았다.
- **권고 1(core 검사를 시험이 지나지 않음): 해소.**
  - `with_fresh` 밖에서 따로 만든 fence와 core(`locator_smo.rs:160-162`)의 locator(`:195`)로 같은 statement 안에서 읽는다. Unchanged `Census/BindingMismatch`, 출력 없음, 그 locator의 계수와 working set이 0임을 단언한다(`:401-423`). 검사를 지운 mutant는 `:412`에서 떨어진다.
- **권고 2(module-gate 기록): 미해소.** 작성자는 PR 직전의 record로 미뤘다(§9 `dev3-cursor-locator.md:146`). 읽기 전용 gate check는 `86c6c2a`에서도 13개 path 모두 FAIL이다(26 error). 아래 권고 2로 남긴다.
- **권고 3(몫 반환이 type으로 묶이지 않음): 해소.**
  - `take_locator_share`는 `Option<LocatorShare>`를 돌려준다(`concurrency.rs:268-276`). `LocatorShare`는 private field 하나(core 참조)이고 Clone이 아니며, Drop만 1을 뺀다(`:229-250`). `return_locator_share`는 없어졌다. 0에서의 반환은 `debug_assert`로 드러난다(`:248`).
  - `RowLocator`는 token을 field로 쥔다(`locator.rs:80`). 할당 거절 때는 `?`가 token을 drop해 반환한다(`:89-93`).
  - token은 8 B이고 core와 header의 크기는 그대로다(위 probe). LOC-B02는 32개, 33번째 거절, pop 뒤 31, 다시 생성, 모두 drop 뒤 0을 그대로 통과한다.
  - 새로 본 drop 순서 문제는 아래 권고 3에 적었다.
- **권고 4(기록 revision을 locator에서 읽음): 해소.**
  - `anchor_revision`은 pool에 등록된 T의 anchor image를 `probe::diagnose`로 복사하고 `decode_anchor`로 root revision을 읽는다(`locator_smo.rs:123-139`). locator와 무관한 입력이다.
  - J01은 첫 기록 전(`:198`)과 split 뒤 +1(`:262`), J02는 Prune 뒤 그대로(`:642`)와 unlink 뒤 +1(`:689`), J03은 축소 뒤 +1(`:914`)과 다시 split 뒤 +2 이상(`:1003`)을 pool에서 읽어 entry와 대조한다.
- **NIT 1(slot hash의 file): 해소.** `FileId`는 field가 private이고 값을 꺼내는 함수가 없다(`page.rs:21-30`). L3(`dev3-cursor-locator.md:81`)과 §10, code 주석(`locator.rs:126-129`)이 "file을 뺀 identity"로 맞게 적는다.
- **NIT 2(`document-validation.json`): 해소.** 다시 만든 값과 commit된 값이 같다.
- **NIT 3(census의 closure 반환): 해소.** `test_nonload_caller`는 `LiveSource`와 `LocatedSource` 반환 중 큰 쪽을 센다(`live.rs:478-480`). 다만 문서의 "census 값은 그대로"는 정확하지 않다(NIT 1).

## 판정

필수 0, 권고 3, NIT 1. **ACCEPT.**

1회차의 필수 둘은 code와 시험으로 해소됐고, 각각 되돌리는 mutant가 바뀐 시험에서 떨어진다. 권고 1·3·4와 NIT 셋도 반영됐다. 이번 diff가 정답 경로에 새로 넣은 것은 `Located::Root` 하나다. 이것은 level 0에서 조회와 기록을 빼기만 하고, 발급(`issue_page`)과 판정은 그대로다. 1회차에 확인한 손상 분류, recheck, 오류를 miss로 바꾸지 않음, 회계, census는 그대로 성립한다. 아래 권고는 병합 전에 하기를 권하지만 정답·자원·계약의 결함은 아니다.

일정 서술(08-2b 병합 때 08-2 1일 적립, 36/86 = 41.86%)은 이 판정으로 "각 단계는 독립 D2의 필수 지적을 해소했다"가 참이 된다.

## 필수

없다.

## 권고

1. **§10의 실행표가 아직 `fc1fc94`의 실행이다**(`dev3-cursor-locator.md:181-191`).
   - 표는 "source는 commit `fc1fc94`의 tree"라고 하고, J01 행(`:187`)의 확인 값에는 새 두 사례(다른 core, bounds 손상)가 없다. J03 행(`:189`)에도 level 0 뒤 entry가 그대로라는 단언이 없다.
   - `86c6c2a`에서 J01·J03은 바뀐 시험이다. 다시 돌린 결과(source `86c6c2a`, 시간, J01 lookups 71, 새 사례의 오류 값)로 표를 바꾸기를 권한다. 내 실행에서는 J01 202.6 s, J02 171.3 s, J03 118.5 s였다.

2. **module-gate 기록이 아직 없다(1회차 권고 2).**
   - record를 더하고, review `files`에 `86c6c2a`(또는 그 뒤 최종 head)의 blob 13개를 쓴다.
   - `rust_tests`에는 위 변경 시험을 대응시킨다. 예: `concurrency.rs` → LOC-B02, GA07; `locator.rs`·`read.rs`·`locator_smo.rs` → LOC-J01..J03; `live.rs`·`row_read_census.rs` → S5R-C01..C03.

3. **`RowLocator`의 drop은 몫을 표보다 먼저 돌려준다**(`locator.rs:79-83`).
   - struct field는 선언 순서로 drop된다. `share`가 `entries` 앞에 있으므로 몫 계수가 먼저 줄고, 그다음에 30,720 B 표가 해제된다. 그 사이에 다른 thread의 `RowLocator::new`가 몫을 받아 새 표를 할당할 수 있다.
   - 한 쌍이 겹치면 표가 33개(1,018,248 B)여서 아직 1 MiB 안이다. 둘 이상의 drop과 생성이 동시에 겹치면 잠깐 넘을 수 있다. L2(`dev3-cursor-locator.md:80`)의 "동시에 살아 있는 모든 locator의 합"이 아주 짧은 창에서 깨진다.
   - 1회차의 `impl Drop for RowLocator`도 field drop 전에 반환했으므로 새 회귀는 아니다. 1회차에서 내가 놓쳤다.
   - `share`를 마지막 field로 옮기면 표가 해제된 뒤에 몫이 돌아간다. 주석에 그 이유를 적기를 권한다.

## NIT

1. §9의 "NIT 3은 … 두 열기 closure 반환 중 큰 쪽을 세게 했다(census 값은 그대로)"(`dev3-cursor-locator.md:147`)는 정확하지 않다.
   - nonload_B가 5,688에서 5,696 B로 8 B 늘었다. 그래서 공유 non-LOAD phase 여섯(admission, prepare_pages, capture, decode, copy, cleanup)도 각각 8 B 늘었다. 예를 들어 decode는 26,301 B다.
   - §10 표의 phase(point·cursor·located의 LOAD 쪽)는 그대로다. "표의 phase는 그대로, 공유 non-LOAD phase는 8 B 증가(모두 Full 이하)"로 고치기를 권한다.

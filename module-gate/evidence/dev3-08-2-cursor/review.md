Verdict: ACCEPT

# 08-2a(stable cursor across SMO + locator 설계) 독립 D2 검토 2회차

## 대상

- head `d4d3f2a`(branch `claude/project-thread-gfugms`), base `origin/main` `ba97cd5`. 1회차는 `ca62fda`에서 했다. 1회차 반영은 `daa0c3c`다. `a381417`은 main `ba97cd5`의 병합이다. 이 병합은 `plan4_recovery_base_tests.rs`와 문서만 바꾸고 제품 code는 바꾸지 않는다. `d4d3f2a`는 일정 08-2 행만 고친다.
- 읽은 것:
  - `review-r1.md`, `git diff ca62fda d4d3f2a`(crates·계약·일정), `git diff ba97cd5 d4d3f2a` 전체, `dev3-cursor-locator.md` 전문.
  - owner 문서: `dev3-task-06-tree-rows.md` §5.5·§6 자원표·F02(`:297`)·T06-12/13/16, `dev3-tree-smo.md`(`:926-927`, `:1481`), `dev3-resource-bounds.md:86, :180`, `dev3-schedule-baseline.md` §2.2(08-2·18-1/18-2 행)·§3 항목 조건·A7, `dev3-concurrency-admission.md`(core 소유), AGENTS.md, module-gate/README.md.
  - code: `transaction/concurrency.rs`(`PageConcurrency` 배치), `plan4_apply/row/read.rs`(`row_operation`, `page`, `check_table`, `anchor_root`, `route_internal`, `route_root`, `load_leaf`, `issue_page`), `tree_page/search.rs`(`select_child`, `route_row_step`), `page.rs`(`PageIdentity`·`PageRef`), `row/read/driver.rs`(`RowCursor::next`), `row/read.rs`(`CursorProgress`), `driver/live.rs`, `semantic_row/structure/candidate.rs`(RootSplit이 쓰는 page).
  - 작성자 scratch log(`build3.log`, `r2_*.log`, `census.log`)와 파일 mtime.
- 실행: `git archive d4d3f2a`를 scratch에 풀고, 따로 둔 `CARGO_TARGET_DIR`로 debug build를 했다. jenv.sh를 읽고 `--exact`, `--test-threads=1`로 돌렸다.
  - (1) CUR-J02 157.14 s, CUR-J03 121.13 s, CUR-J04 50.52 s, 모두 PASS.
  - (2) 이 PR이 더한 나머지 사례도 돌렸다. CUR-J01 169.04 s, CUR-J05 16.34 s, CUR-B01·B02(0.00 s), 모두 PASS.
  - (3) L12의 수치를 확인하려고 native census S5R-C02를 한 번 돌렸다(바뀌지 않은 code). point.row_load 34,382 B, Full 34,588 B로 memory(작성자 log)와 같다.
  - (4) 크기 확인: 두 번째 사본에 compile-time size probe를 넣고 `cargo check`를 product와 `--tests` 두 cfg로 돌렸다. field가 없을 때 core는 640 B(제품)·824 B(시험)다. `locator_shares: AtomicU8`를 더해도 640 B·824 B다. `PageIdentity` 56 B, `PageRef` 24 B, `RowRouteBounds` 18 B, entry(`Option`, niche 사용)는 120 B다.
  - 끝난 뒤 build target 둘과 사본을 지웠다. 원 저장소에는 손대지 않았다.

## 판정

필수 0, 권고 4, NIT 4. **ACCEPT.**

1회차 필수 둘은 해소됐다.

- locator 수와 회계는 core당 32 몫으로 정해졌다. 몫 하나는 32,768 B이고, 표는 256 entry로 고정이다. 33번째 생성은 거절된다. LOC-B02가 합계를 확인한다.
- key는 anchor `PageIdentity` 전체와 RowId다.

설계는 건전하다.

- **세션 원 논증(L4)**: locator가 core를 빌리고, `check_table`(`read.rs:358-366`)이 table의 core·binding·fence를 operation과 비교한다. 그래서 hint는 open 경계를 넘지 못한다.
- **hit 논증(L7)**: revision이 같으면 SMO가 없었다. 이 판단은 C4와 R3, `issue_page`의 revision 재확인(`read.rs:781`)에 기댄다.
- **RowId 확인**: RowId는 table의 member leaf 가운데 한 곳에만 있다. RootSplit은 기존 root를 왼쪽 leaf로 다시 쓰고, unlink는 빈 leaf만 retire하며, RootCollapse는 internal page만 retire한다. 그래서 retire된 page에 행의 낡은 사본이 남지 않는다.
- **recheck**: hint leaf가 다른 빈 leaf여도 recheck가 Invariant로 잡는다. LOC-J02·J03의 손상 사례는 "오류→miss", "recheck 생략", "revision 비교 생략" mutant를 모두 떨어뜨린다.
- **"padding에 1 byte" 주장**: 위 (4)로 확인했다.

시험도 계약이 말하는 것을 보인다.

- J02는 OutputTooSmall 직후 (마지막 검사 key, 마지막 반환 key)를 단언한다.
- J03은 범위 cursor도 drain 전에 한 행을 읽는다. 그래서 두 cursor가 모두 root 축소를 건넌다.
- J04는 DELETE 뒤(leaf 1개, level 0)와 UPDATE 뒤(leaf 2개, level 1)를 단언한다. 그래서 split의 원인이 UPDATE다.

## 1회차 지적의 처리

- 필수 1(인스턴스 수·1 MiB 회계): 해소. L1(`:79`)은 core당 32 몫이고 33번째는 None이다. L2(`:80`)는 몫 32,768 B, ledger `locator_shares`, 할당 전 확보와 실패 시 반환, admission과 순서 없음을 정한다. L3(`:81`)은 256 entry 고정이다. §7(`:126`)은 동시 접근 규칙이고, LOC-B02(`:113`)는 합 ≤ 1,048,576이다. core 크기 주장은 내 사본에서 맞았다.
- 필수 2(key): 해소. L5(`:83`)는 전체 `PageIdentity`(database·file·page_no·incarnation·owner·owner_generation)와 RowId를 key로 하고 §5.5.5에 대응시킨다. 대응표는 `:98`이다. LOC-B01(`:112`)에 다른 incarnation·owner_generation·database 사례가 들어갔다.
- 권고 1(Missing 맹점): 해소. L7(`:85`)의 recheck와 Invariant, L9(`:87`), LOC-J03 손상 사례(`:116`), §5.5.5·R9 대응표(`:96-105`)가 있다. 대응표의 "이동" 행이 `:103`이다.
- 권고 2(revision 역행): 해소. L6(`:84`), L9(`:87`), 근거 문단(`:92`).
- 권고 3(table 전체 revision의 hit율): 해소. L10(`:88`)에 적었고, LOC-J01·J02(`:114-115`)의 기대가 "모든 행 stale"이다.
- 권고 4(census 여유): 해소. L12(`:90`)에 여유 206 B, never-inline 분리, Full을 올리지 않는다는 원칙이 있다. 수치는 memory와 native 모두 맞다.
- 권고 5(J04 split 원인): 해소. `cursor_smo.rs:616-617, :632-634`, 계약 `:46`.
- 권고 6(J03 범위 cursor): 해소. `cursor_smo.rs:480-481, :492-493`, 계약 `:45`.
- 권고 7(J02 진행점): 해소. `driver/live.rs:216-226`의 `test_progress`, `cursor_smo.rs:338-344`, 계약 `:44`. accessor는 `CursorProgress`의 실제 두 field를 돌려준다.
- 권고 8(F02 의무): 대체로 해소. §7(`:125`)에 의무와 소유자를 적었다. 다만 일정 항목은 없다(권고 3).
- 권고 9(T06-13 slot reuse·stale incarnation): 해소. LOC-J02의 Prune 사례(`:115`), T06-13 문단(`:119`).
- 권고 10(실행 증거 source): 부분 해소.
  - §3 실행표(`:51-55`)의 blob이 `daa0c3c`·`d4d3f2a`와 같다: `smo_tests.rs` `8bcb5e8`, `cursor_smo.rs` `03297e6`, `live.rs` `ce806ea`.
  - 작성자의 J02–J04 실행(build 00:48:31–00:49:09, 실행 00:49–00:54)은 두 파일의 마지막 수정(00:48:19) 뒤다.
  - J01·J05 줄에는 source가 없다(NIT 1). module-gate record도 아직 없다(권고 4).
- NIT 1(C1): 해소(`:24`).
- NIT 2(C7 순서): 해소(`:30`). `driver.rs:521-528`과 맞는다.
- NIT 3(기록 시점): 해소. L8(`:86`).
- NIT 4(`record_table_read`): 해소. L7(`:85`).
- NIT 5(entry 크기·allocator 오버헤드): 해소. L2·L3(`:80-81`)과 L10(`:88`). entry 120 B는 내 확인과 같다.
- NIT 6(compaction): 해소(`:69`).
- NIT 7(cursor와 hint): 해소. L11(`:89`).

## 필수

없다.

## 권고

1. **core의 몫 계수는 다른 계약이 소유한 type에 들어간다**(L2 `:80`, §1 `:15`).
   - `PageConcurrency`는 `dev3-concurrency-admission.md`가 소유한다(`:34`). manifest에서는 `dev3_concurrency_admission` 모듈의 source다(`repository-build-plan.json:1615-1622`).
   - 이 문서는 그 계약을 고친다고 말하지 않는다.
   - 08-2b에서 그 계약에 field와 규칙을 적기를 권한다. 규칙은 fence·lock 없이 바뀐다, admission과 순서가 없다, Drop이 반환한다, 생성자의 heap 0을 유지한다는 것이다.
   - §1이나 L2에 08-2b가 그 모듈의 source를 바꾼다는 점(검토 범위)을 적기를 권한다.
2. **locator 하나의 용량이 256 hint로 고정이다**(L1 `:79`, L3 `:81`, L10 `:88`).
   - 혼자 읽는 caller도 예산의 1/32만 쓴다. 한 caller의 hot row가 256개를 넘으면 대부분 miss와 eviction이 된다.
   - 몫은 owner마다가 아니다. 그래서 한 caller가 여러 몫을 쥐어 다른 caller가 locator 없이 읽게 될 수 있다. §7 `:127`의 fallback은 있다.
   - "cache 1 MiB"는 32개가 모두 살아 있을 때만 찬다.
   - 이 결과를 L3·L10에 적기를 권한다. T06-16(18-1)·PF09(18-2)가 caller당 hit율과 살아 있는 몫 수를 함께 재게 하기를 권한다.
3. **F02 의무에 일정 항목이 없다**(§7 `:125`, `dev3-task-06-tree-rows.md:297`).
   - 일정 기준선에는 cursor 작업 budget이나 batch 항목이 없다.
   - 미배정 의무로 일정에 적거나, 정할 시점(예: 18-2 PF09 측정 뒤)을 적기를 권한다.
   - 그렇지 않으면 19의 "모든 필수 인수 충족" 판정 때 빠질 수 있다.
4. **module-gate record가 아직 없다**(1회차 권고 10의 남은 부분).
   - 병합 전에 이 PR의 record를 더해야 한다. 바뀐 path는 `live.rs`, `smo_tests.rs`, `driver/tests.rs`, `cursor_smo.rs`, `unlink_rows.rs`다.
   - 이 path들을 CUR-B01·B02, CUR-J01..J05의 `rust_tests`에 대응시킨다.
   - review는 이 검토의 판정과 head blob을 쓴다.

## NIT

1. §3 실행표 `:54`의 CUR-J01·J05 줄에는 실행 source가 없다.
   - 작성자의 J01·J05 log(00:05·00:12)는 `ca62fda` commit(00:14:50) 전이다. 1회차는 이 둘을 다시 돌리지 않았다.
   - 내가 `d4d3f2a`에서 PASS를 확인했으니 그 줄을 이 실행으로 바꾸기를 권한다.
   - `:55`의 J02–J04 줄에는 실행 tree(`daa0c3c`)를 더하면 완결된다.
2. L12(`:90`)의 수치는 memory(S5R-C01)만 말한다. native(S5R-C02)도 34,382/34,588 B로 같다. 두 종류를 함께 적고, 기준을 "`597ccca`(제품 code는 `ba97cd5`와 같음)"로 적기를 권한다.
3. L7(`:85`)의 hint leaf brief read를 "route와 같은 decode와 bounds 검사"라고만 적었다.
   - `route_internal`(`read.rs:521`)처럼 membership도 포함한다고 적기를 권한다.
   - 그러면 member가 아닌 hint page의 오류 종류가 RowId 확인 전에 정해진다.
4. L9(`:87`)의 "bounds가 그 RowId를 포함하지 않으면 Invariant"는 L6(`:84`)의 분류와 순서가 정해져 있지 않다.
   - 기록할 때 bounds는 항상 그 RowId를 포함한다. 그러므로 이 검사는 revision 분류보다 먼저, 같은 key의 모든 entry에 해야 한다.
   - 그렇지 않으면 옛 revision의 손상 entry가 stale로 분류되어 다시 기록되고 사라진다.
   - LOC-B01의 "bounds 밖 entry"도 옛 revision과 같은 revision을 함께 두기를 권한다.

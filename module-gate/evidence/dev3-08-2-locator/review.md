Verdict: ACCEPT (필수 0, 권고 2, NIT 1)

# 08-2b(Row locator) 독립 D2 검토 3회차

## 대상

- worktree `/home/user/xadb-082b`, branch `wip-082b`, base `bb5c193`(08-2a head, 검토 범위 밖), head `668f6ec`, class D2, scope `common`.
- 검토한 차이는 `git diff 86c6c2a 668f6ec`이고 commit은 둘이다.
  - `86afd10`: §10 실행표를 `86c6c2a`의 실행으로 바꿨다. 문서만 바뀌었다.
  - `668f6ec`: 2회차 권고 3을 반영했다. 바뀐 파일은 `locator.rs`, `dev3-cursor-locator.md`(L2·§9·§10), `document-validation.json`, `module-gate/evidence/dev3-08-2-locator/review-r2.md`다.
- 바뀐 Rust 파일은 `crates/engine/src/storage/wal_io/plan4_apply/row/locator.rs` 하나다. blob이 `c4bd41d`에서 `a84c9eb`로 바뀌었다.
  - `share`를 `RowLocator`의 마지막 field로 옮기고 주석을 달았다.
  - 생성자의 field 나열 순서도 같이 바꿨다.
  - 나머지 12개 blob은 2회차와 같다.
- repo에 저장된 `review-r2.md`의 sha256은 `88c951233b184cf95a04921d8026f3a8b82f920795cdb8df544bb67c67b7d917`이다. 2회차에 내가 낸 보고서와 같다.

`python3 scripts/check_module_gate.py files --base bb5c193 --head 668f6ec`가 낸 blob 13개:

| 파일 (`crates/engine/src/` 아래) | blob |
| --- | --- |
| `storage/disk.rs` | `41674341cf2a9a0d6eb62ba614bf38245f5b1ec6` |
| `storage/row/read/driver/live.rs` | `b9634a3055875776cc2df2fcfa330b8d9b262676` |
| `storage/tree_page/search.rs` | `25a44d1c27a23dc696c2f0ee1cfa6a5240f2a51b` |
| `storage/wal_io/disk_api.rs` | `05d6fbda8deaff6e6971c831ce4c0f0236d151b5` |
| `storage/wal_io/plan4_apply.rs` | `4b4261d1e228d5d936c9154e7462c759a34e88c5` |
| `storage/wal_io/plan4_apply/row.rs` | `0e841c32b9f2c7c98523ea2839d5eef1bfbf73ec` |
| `storage/wal_io/plan4_apply/row/locator.rs` | `a84c9ebad42aef94d946b752527896b55fa7d0ce` (바뀜) |
| `storage/wal_io/plan4_apply/row/locator_tests.rs` | `3279413f9c602463800dda91e23ef0896e3ecf82` |
| `storage/wal_io/plan4_apply/row/read.rs` | `7d8fe5060e3da8da1e4bb138d1b6fd51375321f9` |
| `storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/locator_smo.rs` | `e70d78eeb042b1224137dc230ebe2a83f511a725` |
| `storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/unlink_rows.rs` | `38c6b72506c2c3b1f35c9e636150e3f415d20a0a` |
| `storage/wal_io/plan4_recovery/tests/catalog_memory/row_read_census.rs` | `84b9f3768fbc07c4a30293027e9e106a788ad9c0` |
| `transaction/concurrency.rs` | `ba31a22aa078b125d2ecdd742a59b21c7f0d2cdd` |

실행 방법:
- `git archive 668f6ec`를 scratch에 풀고 별도 `CARGO_TARGET_DIR`로 build했다. `jenv.sh`를 적용했다.
- 시험은 `--exact <전체 이름> --test-threads=1 --nocapture`로 돌렸다.
- 크기와 drop 순서 probe는 따로 만든 두 번째 사본에서만 했다.
- 끝난 뒤 사본과 target을 모두 지웠다.
- worktree와 `/home/user/xadb`에는 아무것도 쓰지 않았다. 검토 막바지에 worktree에 commit되지 않은 변경 2개가 보였다(§10 실행표, `document-validation.json`). 나는 읽기만 했고, 내가 만든 변경이 아니다.

## 2회차 지적의 처리

1. **권고 3(drop이 몫을 표보다 먼저 돌려줌): 고쳐졌다.**
   - field 순서는 이제 `entries`, `counts`, `share`다(`locator.rs:79-85`).
   - 다른 경로로 순서가 바뀌는지 crate 안을 grep했다. `impl Drop for RowLocator`가 없다. `RowLocator`를 구조 분해하거나 `mem::forget`·`ManuallyDrop`·`mem::replace`로 다루는 곳도 없다. 따라서 drop glue는 선언 순서를 따른다.
     - 먼저 `entries`(표 30,720 B)를 해제한다.
     - 다음으로 `counts`를 drop한다.
     - 마지막으로 `share`(`LocatorShare::drop`, `fetch_update(AcqRel, Acquire)`)가 몫을 돌려준다.
   - 이 순서면 1 MiB를 넘지 않는다. 근거는 다음과 같다.
     - 해제는 감소보다 먼저 일어나고, 감소의 Release 쪽이 그 순서를 내보낸다.
     - 그 값을 Acquire로 읽은 다음 `take_locator_share`가 성공해야 할당이 일어난다.
     - 그래서 어느 순간에도 할당된 표의 수는 쥔 몫의 수 이하이고, 쥔 몫은 32개 이하다.
     - 합은 늘 32 × 30,856 = 987,392 B ≤ 1,048,576 B다. 2회차의 일시적 초과(drop과 생성이 둘 이상 겹치는 경우)는 없어졌다.
   - `new`(`locator.rs:91-102`)도 확인했다.
     - 몫은 여전히 할당 전에 받는다.
     - 할당이 거절되어 일찍 돌아가면 지역 변수는 선언의 역순으로 drop된다. 빈 `Vec`가 먼저(할당 없음), 그다음 `share`다.
     - 생성자 안의 field 나열 순서는 drop 순서와 무관하다. 바꾼 것은 가독성 때문이다.
   - **동적 probe로 확인했다.** probe 사본에서 다음을 바꿨다.
     - workspace `unsafe_code`를 `forbid`에서 `deny`로 낮췄다.
     - live heap byte를 세는 `#[global_allocator]`를 넣었다.
     - `LocatorShare::drop` 첫 줄에서 그 순간의 live byte를 기록하게 했다.
     - 결과는 아래와 같다.

     | 순서 | drop 직전 | 몫 반환 순간 | drop 뒤 | 반환 전 해제 |
     | --- | ---: | ---: | ---: | ---: |
     | `668f6ec`의 `RowLocator`(share 마지막) | 43,614 | 12,894 | 12,894 | **30,720 B**(표 전체) |
     | `86c6c2a`의 순서(같은 `LocatorShare`·`Box<[Option<Entry>]>`로 만든 mirror struct, share 먼저) | 43,614 | 43,614 | 12,894 | **0 B** |

     probe는 지웠다.
   - **크기는 72 B 그대로다.**
     - `const _: [(); 0] = [(); size_of::<RowLocator<'static, 'static>>()]` probe를 넣고 `cargo check -p xadb-engine --lib`(제품 cfg)와 `--lib --tests`(시험 cfg)를 돌렸다. 둘 다 "found one with a size of 72"였다.
     - LOC-B02의 출력도 `bytes=30856 header=72 entry=120`, `shares=32 total=987392`다.
     - 기본 repr은 field를 어차피 재배치한다. 선언 순서가 바꾸는 것은 drop 순서뿐이다.
   - 문서의 세 곳이 code와 맞다.
     - 코드 주석(`locator.rs:82-83`)
     - L2의 새 문장 "drop은 표를 먼저 놓고 몫을 돌려주므로 … 1 MiB를 넘지 않는다"(`dev3-cursor-locator.md:80`)
     - §10의 `locator.rs` 행

2. **권고 1(§10 실행표가 `fc1fc94`의 실행이었음): `86afd10`이 고쳤다.**
   - 표는 source를 `86c6c2a`로 적는다.
   - J01 행에 lookups 71, 다른 core의 `Census/BindingMismatch/Unchanged`, bounds 손상 결과가 들어갔다.
   - J03 행에 "축소 뒤 조회·기록 0(8개 hint 그대로)"가 들어갔다.
   - 이 값들은 아래 3회차 실행 출력과 같다.
   - 다만 `668f6ec`에서 `locator.rs` blob이 바뀌어 남은 일이 하나 생겼다(아래 권고 1).

3. **권고 2(module-gate 기록 없음): 아직 열려 있다.** §9는 이것을 PR 직전 record의 일로 적는다(`dev3-cursor-locator.md:149`). 아래 권고 2에 상태를 적었다.

4. **NIT(census 문구): 고쳐졌다.**
   - §9 NIT 3 항목(`dev3-cursor-locator.md:147`)은 이제 다음을 말한다. non-LOAD caller 항이 5,688 B에서 5,696 B가 됐고, 그것을 공유하는 non-LOAD phase 여섯이 8 B씩 커졌으며, 모두 Full 안이고, §10 표의 phase 값은 그대로다.
   - `668f6ec`의 S5R-C01·C02 출력으로 확인했다.
     - `nonload_B=5696`, decode 26,301, admission 15,845, prepare_pages 23,418, capture 15,424, copy 14,109, cleanup 14,701이고 Full은 34,588이다.
     - §10 표의 9개 phase는 표와 같다.
     - C01과 C02의 값은 같다.
     - phase 행 전체가 2회차(`86c6c2a`) 출력과 바이트 단위로 같다.

5. §9의 새 2회차 문단(`dev3-cursor-locator.md:149`)을 확인했다.
   - 적힌 판정(ACCEPT, 필수 0, 권고 3, NIT 1), `86afd10`, share를 마지막 field로 옮긴 것, 권고 2의 처리 시점이 2회차 보고서와 사실대로 맞는다.

## 판정

ACCEPT.
- Rust 변경은 field 선언 순서 하나다. 바뀌는 것은 drop 순서뿐이고, 의도한 순서(표 해제 뒤 몫 반환)를 probe로 직접 확인했다.
- 크기(72 B, 30,856 B, 987,392 B)와 모든 시험 출력이 2회차와 같다.
- 정확성·자원·계약 결함은 없다.

3회차 실행(`668f6ec` archive, 별도 target, debug):

| 시험 | 결과 | 확인한 값 |
| --- | --- | --- |
| LOC-B01 | PASS(0.00 s) | 출력 없음. 단언 모두 통과 |
| LOC-B02 | PASS(0.00 s) | `bytes=30856 header=72 entry=120`, `shares=32 total=987392`. 32개, 33번째 거절, pop 뒤 31, 다시 생성, 모두 drop 뒤 (0, 0) |
| S5R-C01 | PASS(0.00 s) | `full=34588 nonload_B=5696`. phase 36행이 `86c6c2a`와 같음 |
| S5R-C02 | PASS(0.00 s) | C01과 같은 값 |
| LOC-J01 | PASS(206.2 s) | `moved=[13, 14, 15]`, lookups 71, hits 36, stale 15, misses 19, evictions 0, rechecks 0, working_set 19, bytes 30856 |
| LOC-J02 | PASS(174.1 s) | `pruned=10`, damaged `Source/Tree(Corrupt)/CallerMustResolve` |
| LOC-J03 | PASS(117.5 s) | lookups 33, hits 9, stale 8, misses 16, rechecks 1, damaged `Census/Invariant/CallerMustResolve` |

- LOC-B02·J01·J02·J03이 출력한 `LOC_*` 행은 2회차 출력과 바이트 단위로 같다.
- J01..J03도 돌린 이유: 실제 locator를 만들고 drop하는 시험이고(J01은 다른 core의 locator도 만든다), field 순서는 `RowLocator`의 drop glue를 바꾸기 때문이다.
- S5R-C03과 GA07은 다시 돌리지 않았다. 두 시험의 파일(`row_read_live.rs`, `concurrency.rs`)은 `RowLocator`를 쓰지 않고, 관련 blob이 2회차와 같다. 2회차에 둘 다 PASS였다.
- 그 밖의 확인:
  - `cargo fmt --all -- --check`가 통과했다.
  - `cargo clippy -p xadb-engine --all-targets --all-features --locked -- -D warnings`가 경고 0으로 통과했다.
  - `python3 -B devdoc/verify_documents.py`가 PASS였다(markdown 288개, errors 0). 다시 만든 `document-validation.json`은 commit된 것과 `date`만 다르다. `dev3-cursor-locator.md`의 sha256 `01abf6ea…a98a7d`가 json의 값과 같다.

## 필수

없음.

## 권고

1. **commit된 §10 실행표가 아직 `86c6c2a`의 실행이다**(`dev3-cursor-locator.md:183`).
   - `668f6ec`에서 `locator.rs` blob이 `c4bd41d`에서 `a84c9eb`로 바뀌었다. 그래서 "Rust 파일 13개의 blob은 08-2b module-gate 기록의 목록과 같다"는 문장은 최종 head로 만든 record와 맞지 않게 된다.
   - 검토 막바지에 worktree에서 commit되지 않은 수정이 보였다. 이 수정은 표의 source를 `668f6ec`로 바꾸고 J01..J03의 시간을 새로 적는다. 그것을 record와 함께 commit하기를 권한다.
   - 참고로 내 `668f6ec` 실행은 J01 206.2 s, J02 174.1 s, J03 117.5 s였고, 값 열은 지금 표와 같다.

2. **module-gate 기록이 아직 없다**(1·2회차 권고 2).
   - `check --base bb5c193 --head 668f6ec`는 FAIL이고 error는 26개다. "no scoped CI evidence"가 13개, "review missing"이 13개(D2 common 7, D1 6)다.
   - record의 review `files`에는 위 13개 blob(`locator.rs`는 `a84c9eb…`)을 쓴다.
   - `rust_tests`에는 2회차에 적은 대응을 쓴다. `concurrency.rs`는 LOC-B02·GA07, `locator.rs`·`read.rs`·`locator_smo.rs`는 LOC-J01..J03, `live.rs`·`row_read_census.rs`는 S5R-C01..C03이다.

## NIT

1. (선택) 동시성 admission §14의 "반환" 행(`dev3-concurrency-admission.md:200`)은 "`RowLocator`는 token을 field로 쥐므로…"라고만 적는다.
   - "마지막 field로 쥐어 표를 먼저 놓는다"를 더하면 L2와 맞는다.
   - 그 순서는 L2와 코드 주석이 정하므로 이 문구가 없어도 계약은 성립한다.

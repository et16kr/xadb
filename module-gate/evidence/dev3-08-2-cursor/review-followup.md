# PR #62 추가 독립 검토와 신규 시험

2026-11-07 · 기준 PR `bb5c193f319732b43610a65aa426f45e492a1f57`, base `ba97cd5b76dba8497fbe4374baf9f4f39fddf865`.

## 판정

구현자 `/root`, 별도 검토자 `/root/cursor_review`. 독립 D1/D2 **ACCEPT**, 필수 지적 0.
검토자는 원 PR와 추가 diff, F02·C1–C7·locator 계약, 실제 C의 `issue_page`, 독립 기대값과 아래 원 로그를 읽었다. 파일 수정·시험 실행은 하지 않았다.

키 기반 재탐색, 검사/반환 진행점 분리, checked cleanup 이후 결과 확정, CursorInvalidated 우선순위는 계약과 일치한다. 운영 코드 변경은 없다.
B03은 flush 뒤 이동한 미반환 행, B04는 copy 뒤 release/finish 실패와 행 이동, B05는 1,536 범위·구조 schedule을 검증한다. J06은 실제 RootSplit/ChildSplit 사이 출력 부족·취소와 End 뒤 자기 변경을 검증한다. J06의 최초 실패 행은 RootSplit 뒤에도 왼쪽에 남고, 실패 행 자체의 이동은 B03·B04가 덮는다.
기존 일곱 사례는 재실행하지 않았다. native fault·내구성·성능·locator 구현 인수 결과가 아니다. locator 08-2b와 F02 작업 budget·batch 의무는 기존 계획대로 남는다.

## 실행과 source

Rust 1.95.0, Linux, debug `xadb-engine --lib --locked`, 한 test thread. B03–B05는 각 정확한 고유 이름 filter로 `cargo test -p xadb-engine --lib --locked <case> -- --nocapture --test-threads=1`을 실행해 각각 1 case임을 확인했다. J06은 같은 빌드의 ELF에 전체 case 이름과 `--exact --nocapture --test-threads=1`을 전달했다. `RUST_MIN_STACK=33554432`, timeout 1,500초는 실행 전에 설정했다.

실행 전 `scripts/check_dev0.py`의 source_fingerprint와 해당 SOURCE_PATHS 파일별 SHA-256 manifest, 실제 ELF SHA-256, selector·시간·stack·기준 PR을 담은 input plan SHA-256을 기록했다. J06은 이 실제 값들을 `XADB_CATALOG_SOURCE_SHA256`, `XADB_CATALOG_ELF_SHA256`, `XADB_CATALOG_INPUT_SHA256`으로 받았다. 로컬 manifest·plan·원 로그는 `/workspace/scratch/pr-review-20261107`에 보존했다.

```json
{
  "source": "f8e46c32f1754db0255efc96ed9ff68b393524943e99c95fb41c39dfcef5688d",
  "elf": "17ecc30c5eeb538c702672b686d5046702a078fa03b1ba304f017f5041941c21",
  "input": "04e1070a093801eb6b8c4115fcf3dec04e7db378799d6261659a3a60b045c239"
}
```

| 검토한 Rust 파일 | Git blob |
| --- | --- |
| `crates/engine/src/storage/row/read/driver/live.rs` | `ce806ea9f2ea7c535090e536437f4dadd70376f7` |
| `crates/engine/src/storage/row/read/driver/smo_tests.rs` | `0273f8c440a73c2353285531bcab91ec1626cbdb` |
| `crates/engine/src/storage/row/read/driver/tests.rs` | `99c52175b8d07424747a0da2a7dc909e07a83b62` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/cursor_smo.rs` | `15d3a2731b3b4513aca2fdb9bdabed43a2be03c6` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/unlink_rows.rs` | `a3b729d1ccc9ce57ced25a990a95b5d64c0d606f` |

## 원 결과와 실패 이력

새 시험 작성 중 첫 compile은 macro 정의 전 사용과 `unwrap_err`의 Debug bound 때문에 실패했다. 시험 정의 위치와 오류 추출을 고친 뒤 compile이 통과했다. J06 첫 실행은 identity 환경변수 누락으로 본문 진입 전에 실패했다. 이 실패를 PASS로 재분류하지 않는다. 실제 source/ELF/input identity를 제공한 별도 실행만 아래 PASS다. 두 초기 로그도 위 로컬 디렉터리에 보존한다.

### cur-b03.log

SHA-256 `8da1ed56b20538fd936ae49fb255601abf3e9cf09f40bfa070211d5614b51333`

```text
   Compiling xadb-engine v0.1.0 (/workspace/xadb/crates/engine)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1m 57s
     Running unittests src/lib.rs (/workspace/scratch/pr-review-20261107/target/debug/deps/engine-e4e141083d864921)

running 1 test
test storage::row::read::driver::tests::smo::cur_b03_flush_retry_reseeks_after_a_split ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2439 filtered out; finished in 0.00s

```

### cur_b04_cleanup_failure_retries_the_moved_unpublished_row.log

SHA-256 `3490686accb74690959191235c6595cd454c321d49ed79e311b8d0401ecf1761`

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s
     Running unittests src/lib.rs (/workspace/scratch/pr-review-20261107/target/debug/deps/engine-e4e141083d864921)

running 1 test
test storage::row::read::driver::tests::smo::cur_b04_cleanup_failure_retries_the_moved_unpublished_row ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2439 filtered out; finished in 0.00s

```

### cur_b05_range_results_ignore_shape_change_sequences.log

SHA-256 `52ea602a79180d30de5c66ef483613650dc659ef648c025f3f8953ef0204d7eb`

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s
     Running unittests src/lib.rs (/workspace/scratch/pr-review-20261107/target/debug/deps/engine-e4e141083d864921)

running 1 test
test storage::row::read::driver::tests::smo::cur_b05_range_results_ignore_shape_change_sequences ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2439 filtered out; finished in 0.41s

```

### cur-j06.log

SHA-256 `f6572467dcf8edb5ef083e7f56f8e6d3f24a9e0f2785a1be14606e07223471d5`

```text

running 1 test
test storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_recovery::tests::catalog_memory::row_join::retire::autocommit::update::split_rows::auto_split::unlink_rows::cursor_smo::cur_j06_failed_and_cancelled_steps_retry_across_real_splits ... CATALOG_MEMORY_CONTEXT	CUR-J06	f8e46c32f1754db0255efc96ed9ff68b393524943e99c95fb41c39dfcef5688d	17ecc30c5eeb538c702672b686d5046702a078fa03b1ba304f017f5041941c21	04e1070a093801eb6b8c4115fcf3dec04e7db378799d6261659a3a60b045c239	f6e0dd554a91931cc8e048e775a666dacd777f0c1c3285abed6773ec15f21eeb	4bf20c59207890d66b5b51d06d7778a478f1226e26ce38b7cc803856fbe12181
CATALOG_MEMORY_BEGIN	CUR-J06
CATALOG_MEMORY_PASS	CUR-J06
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2439 filtered out; finished in 15.68s

```


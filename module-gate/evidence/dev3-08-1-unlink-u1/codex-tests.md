# PR #56 추가 검증 — 2026-11-06

원본 PR tip: `2b399982b77c5e2fa99e6c220d835db2e47f632c`. 기준 main: `1725df0353f497830815b2a2c3f88af9443cbb09`.
담당: `/root`. 기존 시험은 반복하지 않고 새 사례 3개만 실행했다. 아래 원 실패·시험 작성 오류는 보존한다.

## 범위와 결과

- P2: Base4 current-tree closure의 retire LSN이 Structural high-water를 넘는 입력 수락. 기존 계약의 상한을 `current_tree::observe`에 추가했다. 기존 `high` 값을 사용하므로 새 source carrier나 charge 증가는 없다.
- 신규 high-water 사례: 두 독립 Base4 fixture에 capture/anchor LSN 91, Structural high-water 90을 구성했다. retire LSN 90은 전체 decode 성공, 91은 closure와 전체 decode 거절을 기대한다. 수정 전 closure가 수락하여 RED, 수정 후 leaf/collapse 양쪽 GREEN.
- 신규 연속 전이: LeafUnlink 두 번 → RootCollapse → RootSplit. 각 단계의 실제 descriptor와 high-water를 다음 입력으로 전달하고 before-digest 연결, 고정 행·allocator 보존, 정확한 목록·revision·root, 퇴역 페이지 비재사용을 확인했다. PASS.
- 신규 core 변조: 두 opcode의 core 256 bytes 각각 1-bit 변조하고 descriptor/envelope checksum을 다시 계산한 512개 입력을 canonical observer가 모두 거절했다. PASS.
- frame 사례 둘은 제품 수정과 무관한 경로로 성공한 뒤 반복하지 않았다. 최종 high-water 수정에는 그 실패 사례만 재실행했다.
- U2 replay, U3 live 호출자, U4 native crash는 이 검증 범위 밖이다. 기존 PR의 H001 실패 및 g5 executor 조건을 이번 새 시험의 결과로 재분류하지 않는다.

## 실행 환경과 명령

Rust 1.95.0, Linux x86_64; `CARGO_HOME=/workspace/.xadb-env/cargo`, `RUSTUP_HOME=/workspace/.xadb-env/rustup`, `CARGO_TARGET_DIR=/workspace/scratch/pr56-target`.

```sh
cargo test -p xadb-engine --lib --locked unl_review_ -- --nocapture
cargo test -p xadb-engine --lib --locked storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water -- --exact --nocapture
```

첫 명령으로 frame 사례 둘을 실행했다. 두 번째 명령으로 보정한 반례의 RED와 제품 수정 후 GREEN을 실행했다. fmt·Clippy·문서 검사 및 module gate는 commit의 pre-push 절차로 별도 수행한다.

## 최종 Rust 입력 identity

| 파일 | Git blob | SHA-256 |
| --- | --- | --- |
| `crates/engine/src/storage/plan4_recovery_base/row/current_tree.rs` | `83b6daf7b401fa37055ca9874d4550254c6f7161` | `34a956f284cc05908646207f0df6bc892d3bd576c734f1005a558f600c2dbdc5` |
| `crates/engine/src/storage/plan4_recovery_base/row/graph/current_tree_tests.rs` | `e9ee7ebf3e1f90258676be240a486b4393e767f1` | `4acda938059fd11dfc135aeaf24ace984235e42c3b0181604b21a9787df8583a` |
| `crates/engine/src/storage/semantic_row/structure/weights/unlink_frame_tests.rs` | `36f43acf6749f88302465230f482f6cfcba24a76` | `f3148d8f6118a0acbe52f27f1c09aea5cf873ee0ee2a4b5d63f7bccb4c317f00` |

RED 제품 파일은 원본 tip의 `current_tree.rs` blob `6713562c814861e961b341369a5d32c413ef50cb`다.

## 시험 작성 중 오류: 없는 nested inventory를 seal하여 underflow. 제품 결함 증거 아님

원 로그 SHA-256: `4932d1e07a04aa42168dd86f1926fa90843cda94e7a09593f3581ca011adc6f4`

```text
   Compiling version_check v0.9.5
   Compiling typenum v1.20.1
   Compiling autocfg v1.5.1
   Compiling libc v0.2.191
   Compiling generic-array v0.14.7
   Compiling memoffset v0.9.1
   Compiling cfg_aliases v0.2.2
   Compiling cfg-if v1.0.5
   Compiling nix v0.30.1
   Compiling cpufeatures v0.2.17
   Compiling bitflags v2.13.2
   Compiling crypto-common v0.1.7
   Compiling block-buffer v0.10.4
   Compiling digest v0.10.7
   Compiling sha2 v0.10.9
   Compiling xadb-foundation v0.1.0 (/workspace/xadb/crates/foundation)
   Compiling xadb-platform v0.1.0 (/workspace/xadb/crates/platform)
   Compiling xadb-engine v0.1.0 (/workspace/xadb/crates/engine)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1m 40s
     Running unittests src/lib.rs (/workspace/scratch/pr56-target/debug/deps/engine-e4e141083d864921)

running 1 test

thread 'storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water' (1964) panicked at crates/engine/src/storage/plan4_recovery_base/row/graph/current_tree_tests.rs:9:13:
attempt to subtract with overflow
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water ... FAILED

failures:

failures:
    storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2379 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p xadb-engine --lib`
```

## 시험 작성 중 오류: capture의 중복 필드 248 미갱신. 제품 결함 증거 아님. 새 frame 사례 둘 PASS

원 로그 SHA-256: `2c78892b77aef33093c941a25d8b067988db5b7bc565281f1746b9db1c0c83dd`

```text
   Compiling xadb-engine v0.1.0 (/workspace/xadb/crates/engine)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 35.45s
     Running unittests src/lib.rs (/workspace/scratch/pr56-target/debug/deps/engine-e4e141083d864921)

running 3 tests

thread 'storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water' (2431) panicked at crates/engine/src/storage/plan4_recovery_base/row/graph/current_tree_tests.rs:643:9:
assertion failed: decode(&*bytes, expected(), &mut scratch()).is_ok()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water ... FAILED
test storage::semantic_row::structure::weights::tests::candidate::unlink::frame::unl_review_consecutive_unlinks_collapse_then_split_preserve_rows_and_history ... ok
test storage::semantic_row::structure::weights::tests::candidate::unlink::frame::unl_review_every_core_byte_mutation_is_rejected_after_resealing ... ok

failures:

failures:
    storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 2379 filtered out; finished in 0.33s

error: test failed, to rerun pass `-p xadb-engine --lib`
```

## 보정한 반례 RED: closure가 IdentityMismatch 대신 수락

원 로그 SHA-256: `16f37bf7c77926de64a295b333a6ebcb601a63172bb4176ab086cfe8efeb9400`

```text
   Compiling xadb-engine v0.1.0 (/workspace/xadb/crates/engine)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 27.62s
     Running unittests src/lib.rs (/workspace/scratch/pr56-target/debug/deps/engine-e4e141083d864921)

running 1 test

thread 'storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water' (2850) panicked at crates/engine/src/storage/plan4_recovery_base/row/graph/current_tree_tests.rs:651:9:
assertion `left == right` failed
  left: None
 right: Some(IdentityMismatch)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water ... FAILED

failures:

failures:
    storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2381 filtered out; finished in 0.04s

error: test failed, to rerun pass `-p xadb-engine --lib`
```

## 상한 수정 후 high-water 사례 GREEN

원 로그 SHA-256: `46eb0f5a7c2888ab78abc0a906359a2082aeaed43d36c019c7ef9d0005ae3c6d`

```text
   Compiling xadb-engine v0.1.0 (/workspace/xadb/crates/engine)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 21.69s
     Running unittests src/lib.rs (/workspace/scratch/pr56-target/debug/deps/engine-e4e141083d864921)

running 1 test
test storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_review_retirement_cannot_exceed_structural_high_water ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2381 filtered out; finished in 0.10s

```

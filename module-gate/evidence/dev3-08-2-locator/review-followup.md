# PR #65 additional review and tests

2026-11-07 · D1 additional tests; D2/common read-only review of original PR.
Author/integrator: `/root`. Separate reviewer: `/root/independent_review`.

## Scope and outcome

Base `9669c96e514d914f07c279aef63a1fc50944a854`, reviewed original PR head
`f071e06bae19fb08f8d020c7bb74d98014fc20ef`. Product Rust blobs remain unchanged.
Read the owner [contract](../../../devdoc/contracts/dev3-cursor-locator.md),
concurrency admission, source census, and implementation/coding/development policies.

No required production correction found. Hints contain routes only; existing
schema, row identity, visibility and snapshot decisions still apply. Read
admission stabilizes the SMO revision. Invalid bounds/revisions and damaged
leaves remain errors, and a missing hinted row is rerouted and confirmed.
Full anchor identity and core binding isolate tables/sessions. The atomic
32-share ledger and table-before-share drop order preserve the budget.

Direction: this is a bounded optimization within the current profile, consistent
with the disk-engine direction. It does not establish the expanded
XADB-SCOPE-002 capacity/concurrency goals. The 256-entry direct-mapped table and
whole-table revision invalidation may have poor hit rates under broad reads or
append; PF09/T06-16 measurements and DEV4 ownership integration remain open.
No native recovery, performance, or large working-set claim is added.

## New test selection

Only the three newly written tests ran. No original LOC, census, or workspace
suite was repeated. All expectations are literal limits/keys/values or explicit
write history; production reads are compared with these expectations, not used
to produce them.

| Case | Additional coverage | Result |
| --- | --- | --- |
| LOC-B03 | 64 real threads compete for 32 shares, hold allocations while the observer samples the ledger, release concurrently, then repeat one bounded wave to prove reuse. Exact 32 successes and at most 1 MiB; final ledger zero. Assertions occur after releasing/joining workers | PASS, 0.01 s |
| LOC-B04 | Same-slot file and owner identity mismatches, replacement eviction, RowId 1/MAX, inclusive lower and exclusive upper bounds. Owner 192 is a literal collision with owner 6; slot equality is asserted | PASS, 0.00 s |
| LOC-J04 | OutputTooSmall leaves a route hint; precancellation leaves counts/output unchanged; real ChildSplit moves that pending row and retry refreshes stale hint; level-two route on own rows matches literal writes and plain reads; Missing hint rechecks, then same-revision own INSERT becomes visible on a hit; commit/drain leaves jobs free and session unfenced | PASS, 11.83 s |

The first J04 invocation stopped before `CATALOG_MEMORY_BEGIN` because the harness
identity environment was missing. This is retained as a setup failure, not a
product failure or PASS. The corrected invocation used real source/ELF/input
hashes and `RUST_MIN_STACK=33554432`, with a 330 s outer timeout and the test's
preselected 300 s bound. B03 ran before B04's owner collision assertion was
strengthened; only B04 changed afterward, so B03 was not repeated.

## Reproduction and identity

Toolchain: Rust 1.95.0; package `xadb-engine`, lib test, default features, debug.
First commands were `cargo test -p xadb-engine --lib --locked <case-name> -- --nocapture`
with each new name as a unique filter (one test ran per invocation).
The configured J04 ran the built ELF with its full case name, `--exact --nocapture --test-threads=1`.

Local complete manifests, raw logs and runner are retained under
`/workspace/.xadb-env/logs/pr65-followup/`. The source manifest is sorted tracked
files under `crates/`, `tests/`, `scripts/`, `.github/` plus Cargo.toml, Cargo.lock
and rust-toolchain.toml, each line `sha256  relative-path` and newline. Source
identity is SHA-256 of that manifest. Input manifest selects its `tests/` lines
and locator_smo.rs, including the new literal scenario. The ELF hash is computed
from the actual binary. Only this run's target is removed after verification;
existing artifacts and logs are preserved.

```json
{
  "case": "storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_recovery::tests::catalog_memory::row_join::retire::autocommit::update::split_rows::auto_split::unlink_rows::locator_smo::loc_j04_retry_level_two_and_own_write_visibility",
  "source": "4cc41bfc6845b8136abaca360d04ece005bd9d65b2c2227a07033f21bc734810",
  "elf": "99c1b5b1bd29a52e80521c1332aac71a7291f75e179c7e1bd1d9aed041bf1d65",
  "input": "316c79125cc6cce03c58b54fe262964f1e9850300d6cabdcb18d148713524182"
}
```

### LOC-B03

Raw log SHA-256: `d4c9102869fdbd411734f5770b83212a71799826d548fb2df1699e3a793cafcb`.

```text
   Compiling version_check v0.9.5
   Compiling typenum v1.20.1
   Compiling autocfg v1.5.1
   Compiling cfg-if v1.0.5
   Compiling cfg_aliases v0.2.2
   Compiling libc v0.2.191
   Compiling generic-array v0.14.7
   Compiling memoffset v0.9.1
   Compiling nix v0.30.1
   Compiling cpufeatures v0.2.17
   Compiling bitflags v2.13.2
   Compiling block-buffer v0.10.4
   Compiling crypto-common v0.1.7
   Compiling digest v0.10.7
   Compiling sha2 v0.10.9
   Compiling xadb-foundation v0.1.0 (/workspace/xadb/crates/foundation)
   Compiling xadb-platform v0.1.0 (/workspace/xadb/crates/platform)
   Compiling xadb-engine v0.1.0 (/workspace/xadb/crates/engine)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1m 07s
     Running unittests src/lib.rs (/workspace/.xadb-env/logs/pr65-followup/target/debug/deps/engine-e4e141083d864921)

running 1 test
test storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::retention::row::locator::tests::loc_b03_concurrent_share_admission_and_return ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2447 filtered out; finished in 0.01s

```

### LOC-B04

Raw log SHA-256: `dd267e98991ecd03d6073e6b9b06e38be205f825eac1de10c514b8c7cf542133`.

```text
   Compiling xadb-engine v0.1.0 (/workspace/xadb/crates/engine)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 16.34s
     Running unittests src/lib.rs (/workspace/.xadb-env/logs/pr65-followup/target/debug/deps/engine-e4e141083d864921)

running 1 test
test storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::retention::row::locator::tests::loc_b04_file_collision_owner_and_rowid_boundaries ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2447 filtered out; finished in 0.00s

```

### LOC-J04 harness setup failure

Raw log SHA-256: `2a221c50d73992e1e86b0b62de25caefbbd01d0e1648da1bb2bee709419bdcc3`.

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.01s
     Running unittests src/lib.rs (/workspace/.xadb-env/logs/pr65-followup/target/debug/deps/engine-e4e141083d864921)

running 1 test

thread 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_recovery::tests::catalog_memory::row_join::retire::autocommit::update::split_rows::auto_split::unlink_rows::locator_smo::loc_j04_retry_level_two_and_own_write_visibility' (2217) panicked at crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory.rs:488:59:
missing XADB_CATALOG_SOURCE_SHA256
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_recovery::tests::catalog_memory::row_join::retire::autocommit::update::split_rows::auto_split::unlink_rows::locator_smo::loc_j04_retry_level_two_and_own_write_visibility ... FAILED

failures:

failures:
    storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_recovery::tests::catalog_memory::row_join::retire::autocommit::update::split_rows::auto_split::unlink_rows::locator_smo::loc_j04_retry_level_two_and_own_write_visibility

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2447 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p xadb-engine --lib`
```

### LOC-J04 configured

Raw log SHA-256: `b3b9571b4cf51a0619fad535302e7ef2279def45583c6181f77441e0888a3e4c`.

```text

running 1 test
test storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_recovery::tests::catalog_memory::row_join::retire::autocommit::update::split_rows::auto_split::unlink_rows::locator_smo::loc_j04_retry_level_two_and_own_write_visibility ... CATALOG_MEMORY_CONTEXT	LOC-J04	4cc41bfc6845b8136abaca360d04ece005bd9d65b2c2227a07033f21bc734810	99c1b5b1bd29a52e80521c1332aac71a7291f75e179c7e1bd1d9aed041bf1d65	316c79125cc6cce03c58b54fe262964f1e9850300d6cabdcb18d148713524182	f6e0dd554a91931cc8e048e775a666dacd777f0c1c3285abed6773ec15f21eeb	4bf20c59207890d66b5b51d06d7778a478f1226e26ce38b7cc803856fbe12181
CATALOG_MEMORY_BEGIN	LOC-J04
CATALOG_MEMORY_PASS	LOC-J04
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2447 filtered out; finished in 11.83s

```

## Independent review

The separate reviewer examined the original PR and new test diff without
rerunning prior tests. Original D2/common verdict: ACCEPT, required fixes 0.
The reviewer recommended a guaranteed owner-only slot collision; B04 now uses
owner 192 and asserts the collision. Final new-test review and exact blobs are
recorded in the module-gate record alongside the preserved original review.

Final verdict: **ACCEPT, required_open=0**. The reviewer read B03/B04 and the
configured J04 logs, confirmed CATALOG_MEMORY_BEGIN/PASS and recomputed the
source/input manifest hashes. Reviewed test blobs:
`a5143dcfed7e11b35b7847334bb5884be2606f80` (locator_tests.rs),
`79260704ec3821305aa6d80a2b25fd341a710f1b` (locator_smo.rs).
The reviewer found the retained mappings plus the three new exact cases
sufficient and did not edit files or execute tests.

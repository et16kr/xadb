# PR53 review execution receipt

2026-11-05 · Local PR review of `claude/project-thread-e9itre`, starting at
`bd71004bfc2c621bdd6da272bc7aad89584bf256`, against main
`c52e555c89cea3efd4c8993b3cb9647fd13fc312`. Rust 1.95.0, Linux x86_64,
locked dependencies, engine lib test build, no whole-workspace product test run.
[Independent review](review.md).

## Findings and changes

The admission documentation incorrectly described a target-table-only Reserved
refusal. Canonical split preparation retains the global allocator Reserved and
RootReserved guards. Corrected the contract and two function comments; preserved
runtime behavior and the 44,304 B charge. Added four regression cases for missing
recovery, cursor-error, route-model and admission boundaries. No additional
reproducible runtime-result defect was found within this review scope.

The PR does not complete 08-1: empty-leaf unlink, root shrink, retirement opcodes,
and the A caller's automatic split on capacity remain outside this patch.

## Execution

| Selection | Outcome |
| --- | --- |
| Existing SPL-J01..05 and SPL-B01 | 6 PASS |
| New SPL-B02: upper < key and upper == key, checked cleanup, unchanged output/progress | PASS |
| New SPL-J06: post-Commit moved INSERT, DELETE and UPDATE, recovery Prune and repeated reopen | PASS, both modes; q2/q3, CLR0, Row Job0, exact rows |
| New route partition model | PASS, 120 queries and 40 malformed-bound probes |
| New global Reserved admission test | PASS, same-table and other-table Jobs |
| Existing structural / tree search / tree edit / tree audit filters | 125 PASS, 1 ignored native worker; 528.14 s |
| Native SMO-NR4, SMO-NC4 | PASS, 6 separate processes: produce after durable, recover, reenter |

The native controller ran actual file reopen/recovery in separate processes,
checked the fixed row/value oracle and captured file hashes. It is distinct from
in-memory unit checks. The partition model uses an authored page/range mapping,
not a production search result as its expected output. The structural filter also
exercises injected failures and unwind at installation/publication boundaries.

Commands (the libtest ELF was built using `cargo test -p xadb-engine --lib --locked --no-run`):

```text
<elf1> split_rows spl_b0 --test-threads=2 --nocapture
<elf1> structural tree_page::search:: tree_page::structure:: tree_page::audit:: --test-threads=2 --nocapture
<elf2> spl_j06 row_route_steps_match_partition_model_and_inherited_bounds structural_weights_reject_reserved_jobs_of_other_tables --test-threads=2 --nocapture
python3 scripts/check_dev3_structural_effect_native.py --elf <elf1> --output <native-output> --input tests/fixtures/dev3-08-effect/native-inventory.tsv --source-manifest <elf1-source-manifest> SMO-NR4 SMO-NC4
```

`RUST_MIN_STACK=33554432`; the three `XADB_CATALOG_*_SHA256` variables identify
source manifest, ELF and selection/input. Logs and source manifests are preserved
at `/workspace/.xadb-env/logs/pr53-review-20261105/`; generated build/ELF/native DB
artifacts are removed after retaining hashes and receipts.

## Source attribution and original failures

ELF1 SHA-256: `d19f8767a49c7470aec27e795e481b170fb93eab3fcee87c26fbe72a983ab4d4`.
Its source-manifest digest is
`2e75b15a0b9a5aa6672f8325af287b918847216afb68a372e41823fdf6c4bca3`
(`split-and-driver-source.sha256`): original PR plus the initial SPL-J06/SPL-B02
additions. Product code was unchanged. The structural run reused this exact ELF;
its logged SOURCE value `74065ff93506367856bdef4ff974278f3d6d469087aa22574735c650a26887cd`
is only a later working-tree snapshot at invocation, **not its build source**.
The immutable ELF hash and ELF1 manifest above identify that execution. The
native controller used the correct ELF1 manifest.

ELF2 SHA-256: `694678465d75de5528e5d394af05d14b9e11e847758e336974e4e79e5c46748c`.
Its source-manifest digest is
`fab75be2930c1d9963ba7dd510de27280b80282e3420b55a9b30cb2e6f786705`
(`added-final-source.sha256`), covering the final six-file code/test/contract diff.
Later edits only add these receipts and module-gate metadata. Earlier completed
product tests are not rerun for those metadata edits.

Original outcomes are preserved, not relabeled PASS:

- The first existing-SPL invocation failed before product work because the three
  required identity variables were missing; the configured run supplied them.
- Initial SPL-J06 incorrectly expected RootSplit to replace the original leaf.
  It failed that test-only shape assertion. The corrected test instead proves
  both modified rows moved into the new right leaf, before any recovery assertion.
  The initial run remains 7 PASS / 1 FAIL in `split-and-driver.log`.
- The new Reserved test initially did not compile (`&&mut [u8; 1024]` versus
  `&[u8]`); fixed with `as_slice()`. The final build succeeded.
- A temporary recovery source-census probe passed (selection 10,304 B, cleanup
  24,175 B, independent route lower bound 4,517 B). Its filename `census-red.log`
  describes the intended experiment, not an actual RED result. The suspected
  budget defect was unsubstantiated; the probe and no product census change remain.

Log digests are listed in [artifacts.json](artifacts.json). The ignored native
worker in the broad filter is not counted as a libtest PASS; the explicit
controller's two native cases are reported separately. This receipt is local
verification, not a claim that remote CI or final DEV3 acceptance passed.

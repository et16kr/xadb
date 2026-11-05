# PR53 CI receipt collision: independent review

2026-11-05. Implementer `/root`, reviewer `/root/independent_review`.
D1 **ACCEPT**, required_open0. Read-only source, oracle and receipt review;
the reviewer did not edit files or execute tests.

The original CI and local RED both show two legacy S4 selectors colliding
with F4 aliases in one process: the same writer label and PID produced the
same filename. There is no repeated test name: all333 listed names were unique,
and the two failed names alone were absent from passed_names.

The fix atomically creates a numbered file and retries only AlreadyExists
from open. Other open errors and all write errors propagate, preserving any
existing or partial file. The bounded inclusive sequence cannot wrap.
No caller ceiling, DB code, selection, runner or workflow policy is weakened.

Three new regressions verify prior-file preservation and full repeated
outputs, eight concurrent same-label payloads, and NotFound propagation.
The exact F4/S4 aliases share one process and RUNNER_TEMP/receipt, matching CI.
GREEN has7 PASS and four complete census files, with writer-specific suffixes
0 and1 and ten site rows each. Source/ELF/selection digests were checked.
These seven exact selectors and three reviewed blobs provide sufficient
same-record coverage; no external fixture files are consumed.

```json
{
  "crates/engine/src/storage/wal_io/row_caller_budget.rs": "69ae604de630d4019986346760357d52d255a1a2",
  "crates/engine/src/storage/wal_io/plan4_apply/row/rollback_budget_tests.rs": "a0ce511e06ad3155eb4a1c0104825a1ef55cbc7d",
  "crates/engine/src/storage/wal_io/plan4_apply/row/rollback_census_tests.rs": "4be043d6ec6f7f6e6b9c46f06a1b2729f03bbb64"
}
```

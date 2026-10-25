# B1 Commit continuation compiler inputs

2026-10-25 · Independent input author `/root/dev3_review` · FROZEN INPUTS / NOT_RUN.

These six programs plus the unchanged baseline inspect the actual B1 types.
They are compiled as a direct child of `storage`, beside `dirty_frame` and
`wal_io`, in disposable copies of one approved source export. They are not
children of a capability-producing module. No product module is changed by
the input author. The runner only adds the named module declaration and one
probe file to each copy.

The positive input checks original whole-set preparation/cancellation and
core Owner reuse after cancellation. The five negatives check consuming a
physical stage twice, reborrowing its core Owner, resuming a physical pending
carrier twice, issuing a staged grant through an absent raw-LSN constructor,
and substituting a physical DurableWal for DurableCommit. Each rejection has
one frozen error code and exact primary source span. An unrelated error,
missing completion, timeout, or changed span is FAIL, not an accepted rejection.

Opaque grants and pending carriers occur only as function parameters when a
probe needs them. That proves a type restriction, not existence or production
reachability. No fake grant, private-field access, unsafe conversion, simulated
Commit authority, product binary execution, stage, sync, resume, or ACK occurs.
The constructor probe covers the named absent `StagedCommit::new(u64)` entry;
the D1 source review separately checks all constructors and private fields.

The dedicated runner reuses the original bounded process capture and compiler
metadata preservation helpers. It does not invoke the A compiler inventory or
any lower test wrapper. Limits are 30 seconds per Cargo invocation, 180 seconds
aggregate compiler time, 30 seconds audit, 210 seconds aggregate wall time,
8 MiB aggregate diagnostic capture plus reserved receipt space, and 1 GiB
retained artifacts including the isolated target. Prior PASS results must have
the same source, input, loaded tools, and budget; completed, failed, uncertain,
or incompatible prior work is never silently repeated. Raw diagnostics, source
overlays, injection patches, metadata, and FAIL receipts remain available for
the integrating reviewer before any separately verified archival cleanup.

The current method signatures were confirmed by the common-source author before
freezing the manifest SHA256
`491b1b663b7edac154b86bc6586fa0c990e86d878c5fe329593371f0a9252581`.
The integrating root executes these inputs on its coherent export. This file
and the compiler manifest do not claim compilation or runtime PASS.

Invocation uses an existing dedicated history directory and a fresh direct
child output path. `--unit-root` automatically discovers earlier runs there;
`--prior` can add preserved receipts outside it. A changed source or a failed
attempt requires an explicit compatibility/failure audit before a new campaign.
The runner does not grant that audit or reinterpret old results.

```text
python3 -B scripts/dev3_commit_continuation_compiler.py \
  --source-root <coherent-export> --source-sha256 <approved-fingerprint> \
  --unit-root <dedicated-compiler-history> --output <history/new-attempt>
```

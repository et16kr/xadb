# PR53 Actions failure and fix

Failed run: [Module build41991524144](https://github.com/et16kr/xadb/actions/runs/41991524144),
job127124630164, head97830fd6ebddb609c9c8ab8dcdcccb7456497982.
Original artifact13398953757 ZIP SHA-256:
`2dde3afc6d3e767545fa08e5798ea1f281dd8a31f2490a76892af6f3817e2ab3`.
The quick check and head/export-a/export-b compilation passed. Head selection
listed333 unique tests and passed331. The two S4 rollback census failures
were `AlreadyExists` while creating receipt files; their F4 aliases had already
written the same PID/label filenames. The secondary “duplicate execution,
wrong names or cross-source reuse” summary reports the incomplete pass set,
not evidence that test names actually ran twice.

The prior local verification omitted CI's RUNNER_TEMP/receipt directory, so
this evidence-writing branch was not exercised. No product budget assertion
failed in the original CI receipt. The fix preserves all existing files,
uses atomic create_new with numbered names, and propagates other I/O failures.
It is in a cfg(test) helper; caller ceilings, rollback behavior, trusted
runner and workflows remain unchanged.

## Validation

- Original CI artifact and all original failures are preserved.
- Local RED retained old filename semantics with helper extraction and new
  regressions:3 PASS/4 FAIL. Both original S4 aliases reproduced AlreadyExists;
  repeated-label and concurrent-label regressions also failed as expected.
- GREEN uses the same7 exact selectors, one ELF/process, --test-threads=1 and
  a fresh RUNNER_TEMP/receipt:7 PASS. Three regressions cover existing-file
  preservation, eight barrier-start writers and noncollision I/O errors.
- Independently inspected four emitted files, two per writer. Both complete
  bodies per writer match, retain source headers, and include all ten census
  sites. Existing fixed charge44304 is unchanged.
- No full local333-case selection or unrelated suite was rerun. The fix's
  push triggers the repository's existing CI selection; its subsequent status
  is retained with the external completion receipt after publication.

Frozen green build identity:

```json
{
  "source_sha256": "81e9f851621b4a299d8aa64c3d4e02ca007b2b5bd7639ab2484dca8f32c47924",
  "elf_sha256": "ebcf3ea926afd597a8241aeca8f85976959a24142ea0561f0b40db74470a4730",
  "selection_sha256": "a5e00a2123021057178b2a5d8c29139c0171b77eadb8af983567a56bcdf2eef4"
}
```

Exact commands and selectors are in local/green-identity.json and
local/selection.json. Logs, manifests, patches, original artifact contents and
receipt checks are preserved at
`/workspace/.xadb-env/logs/pr53-actions-20261105/`;
see [artifact hashes](artifacts.json). Generated target/ELF are removed only
after hashes and results are preserved. Committed-tip module gate, fmt,
Clippy and documentation checks run through the pre-push hook and their
logs are retained alongside these results.

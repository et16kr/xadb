# F0 current-format inputs

These Profile3 inputs replace the stale Profile1 inputs only in the selected
live and recovery consumers. The historical `vectors/p1-o00` and Free Undo
inputs remain unchanged.

`python3 tests/fixtures/dev3-plan4-empty-retention/current3/generate.py`
reconstructs every byte using hash-pinned, independent Python serializers.
It does not run or import the engine, read a product result, or derive an
expected value from the implementation under test. Existing output must match
before the index is written. Generation is input preparation, not product PASS.

- `raw-index.tsv`: 63 files, 1,028,952 bytes; SHA-256
  `3b9e3ac52864e5d1e6c46071a16936c2a1a8cd44386691c96022494c65851740`.
- `free-undo/raw-index.tsv`: 46 files, 1,296,304 bytes; SHA-256
  `fabf897cf76a7aaccad13b9545ee1339623fa63d4d0db971678d4e46653cdba7`.
- First history: Reserve1, Status INIT2, AgingStart3, Begin4,
  RollbackStart5, AbortEnd6, TerminalFinish7.
- Second history: epoch2/local1, Begin8 through TerminalFinish11. Status9
  advances to generation3. The canonical Job cursor selects ordinal1 at
  generation2; ordinal0 is not reused by this short empty history.
- Free Undo history: independent current-profile native suffixes for epochs1–3,
  separate memory L1–13, and recovery Cancel6. Native inputs being generated
  does not mean the ignored native controllers ran.
- The 2MiB base4 expectation is assembled in the recovery test's explicit-field
  projector from these images and descriptors. INIT2 provenance always uses
  the original INIT frame and image, including later epochs.

The selected cases, limits, failures and independent review are recorded in
[the F0 completion evidence](../../../../devdoc/evidence/main-preexisting-failures-20261030.md#f0-completion).

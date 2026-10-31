# Memory CREATE retire Profile3 expected inputs (U2-C4b)

Profile3 counterpart of `../memory-retire`: MF14, where a live CLR is staged and
unwound, and recovery then writes the original CLR and cleanup. Status: frozen
inputs, not yet executed against product. Runtime admission is `NOT_ADMITTED`
until U2-C4c lands with the U2-C2 base4 capture.

`input-plan.json` (SHA-256
`0a6efde6c8e0c6e39217765af970802dd7a0a184d93e2aa9f98d97dcbf3a8017`) was frozen
after memory-gap3 and before this generation. It copies the v2 setup, scenario
and final. It pins the native3 writer, serializer3 and the Free Undo serializer
as imports, and the version2 transition writer as serializer3's parent. It also
pins these as unchanged inputs: the native3 and memory-gap3 plans and manifests,
and the v2 memory-retire outputs.

`generate_literals.py` is the v2 memory-retire writer. Its only change is the
imported modules: `serializer3.load()` for the transition module and native3
for the reused `Inputs` mechanics. It never calls `serializer3.row_job`. It ran
once: 16 rows, 3 checkpoints, 106 raw files, 2,584,076 bytes, manifest SHA-256
`feb90a36adaf12540f3f618e31820f497e7d1e9241fb67fde5435e522d427ccf`.

`audit_literals.py` is the v2 auditor with these changes:

- version3 Root and Job wire, with `row_high` 0 and bytes 440..512 zero;
- no Job kind3;
- the Profile3 ID1 profile blob.

The sibling comparisons now use the v3 corpora:

- L1..5 equal native3 N01;
- every row state equals native3 N02;
- the L1..8 and L10 frames equal native3 N02;
- L9 and L11..16 differ from native3 N02 only in the outer epoch and its seals;
- checkpoint 5 equals memory-gap3 MC04, and checkpoints 9 and 16 equal native3 N02.

The v2 L6..16 comparison with transition R01..R11 is cross-version and is not
carried over. It is replaced by schedule equivalence with the frozen v2
memory-retire manifest. The auditor ran once:
`INPUT_AUDIT_PASS_NOT_PRODUCT_EXECUTION`.

`checkpoints.tsv` (SHA-256
`c7be1dcf2802b04c205cbb353b0ccc928f8b2c5d33dc55625b03aebf9a307c66`) comes from
`../native3/project_checkpoints.py`. No base image is written here.

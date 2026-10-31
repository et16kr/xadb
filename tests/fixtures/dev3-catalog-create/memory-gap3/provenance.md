# Memory CREATE gap Profile3 expected inputs (U2-C4b)

Profile3 counterpart of `../memory-gap`: the MC03 issued-local gap and the
MC04 dropped reservation. Status: frozen inputs, not yet executed against
product. Runtime admission is `NOT_ADMITTED` until U2-C4c lands with the U2-C2
base4 capture.

`input-plan.json` (SHA-256
`5286a7f6395aff44d7c49825d574ba69138ffa2a0a1e6e097aeabbd01f1bc336`) was frozen
before generation. It copies the v2 setup, scenarios and finals. It pins the
native3 writer, serializer3 and the Free Undo serializer as imports, the
version2 transition writer as serializer3's parent, and the native3 plan and
manifest and the v2 memory-gap outputs as unchanged inputs.

`generate_literals.py` is the v2 memory-gap writer. Its only change is the
imported modules: `serializer3.load()` for the transition module and native3
for the reused `Inputs` mechanics. It never calls `serializer3.row_job`. It ran
once: 26 rows, 6 checkpoints, 137 raw files, 3,382,130 bytes, manifest SHA-256
`9a484fc975e9d83340729bf1d117431abbeb97944e7031b66be62b8ab4635657`.

`audit_literals.py` is the v2 auditor with these changes:

- version3 Root and Job wire, with `row_high` 0 and bytes 440..512 zero;
- no Job kind3;
- the Profile3 ID1 profile blob;
- the MC03 L1..5 and MC04 L1..7 prefix compared with native3 N01.

The v2 MC04 L8..11 comparison with transition K03..K06 is cross-version and is
not carried over. It is replaced by schedule equivalence with the frozen v2
memory-gap manifest. Row IDs, LSN, epoch, kind, operation, frame length, WAL
offsets, page-role sets and checkpoint scalars are equal; every frame differs.
The auditor ran once: `INPUT_AUDIT_PASS_NOT_PRODUCT_EXECUTION`.

`checkpoints.tsv` (SHA-256
`2964e48b3a2114dcb8e075074b24aead4a2d44dc05958c0ba2bb4336e4274b23`) comes from
`../native3/project_checkpoints.py`, after that script reproduced all three v2
projections byte-for-byte. The consumer interface is the v2 one. No base image
is written here.

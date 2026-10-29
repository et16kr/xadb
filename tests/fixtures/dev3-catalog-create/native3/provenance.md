# Native CREATE Profile3 expected inputs (U2-C4b)

Profile3 counterpart of `../native` for the single current-format switch
(Profile3 → Root3/Job3 → base4). Status: frozen inputs, not yet executed against
product. Runtime admission is `NOT_ADMITTED` until U2-C4c lands with the U2-C2
base4 capture.

`input-plan.json` (SHA-256
`f6e0dd554a91931cc8e048e775a666dacd777f0c1c3285abed6773ec15f21eeb`) was frozen
before generation. It copies the v2 plan's setup, scenarios and finals and pins
the v2 plan, writer and auditor it derives from, the serializers it imports, and
the v2 outputs that must stay unchanged. Root adoption of that SHA-256 is
recorded in `input-receipt.json` `plan_adoption` (U2-C4c, after generation).

`generate_literals.py` is the v2 native writer with one change: the transition
serializer module is `module-row-format/catalog3/serializer3.py` `load()`
(Profile3 ID1 envelope, Job3 free cells, Root3 with `row_high` 0 and the Job3
census aggregate; it checks its own version2 parent pin). `serializer3.row_job`
is never called; all histories are Catalog scope2 only. It ran once:
56 rows, 16 checkpoints, 221 raw files, 5,794,810 bytes, manifest SHA-256
`e648b0bc4fb8ccf5f258cec6d521128daef6e59f3f9a20825d19eb81d68555df`.

`audit_literals.py` is the v2 stdlib auditor plus version3 checks (Root3/Job3
cells, `row_high` 0 and bytes 440..512 zero, no Job kind3, the Profile3 profile
blob) and a schedule-equivalence check against the frozen v2 native manifest:
row IDs, LSN, epoch, kind, operation, frame length, WAL offsets, page-role sets
and checkpoint scalars are equal; every frame differs. It ran once:
`INPUT_AUDIT_PASS_NOT_PRODUCT_EXECUTION`.

`project_checkpoints.py` reproduces all three v2 `checkpoints.tsv` files
byte-for-byte from their pinned manifests before writing this directory's
`checkpoints.tsv` (SHA-256
`9501a6934c5c7c759b86250286e2b695a51622ba275e6263b8639612d45d2f5c`). The memory
siblings use the same script.

The consumer interface is the v2 one (`../native/provenance.md`). No base image
is written here. The selected base (XRB1 base4 header, provenance, certificate,
inventory) is product output after U2-C2 and the switch. These literals do not
issue selected-source authority. N03 reuses the N01 prefix raw bytes as in v2.
It still needs its own API UNKNOWN, no-ACK and process history.

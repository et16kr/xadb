# Transition CREATE Profile3 expected inputs (U2-C4c)

Profile3 counterpart of `../transition` in its runtime-oracle role, for the
single current-format switch (Profile3 → Root3/Job3 → base4). The U2-C4 plan
lists it as regeneration item 2 ("a Profile3 transition corpus with its
serializer"); U2-C3 supplied only the serializer and U2-C4b did not carry the
transition cases over, so U2-C4c produced it before the switch commit, in the
native3 way. Status: frozen inputs; runtime admission `NOT_ADMITTED` until the
switch runs its consumers. Root adoption of the plan SHA is pending.

`input-plan.json` (SHA-256
`4bf20c59207890d66b5b51d06d7778a478f1226e26ce38b7cc803856fbe12181`) was frozen
before generation. One earlier draft (`1558b178…`) was replaced before any run
because its native-identity rule was stricter than version2 itself (see its
`revision_note`).

`generate_literals.py` loads the SHA-pinned `serializer3.py` and calls its
`profile()`, `canonical_cases()` and `mutants()` (the unchanged version2 phase
code) once each; its `emit` is the version2 emit with only the directory,
generator id and status text changed. `serializer3.row_job` is never called. It
ran once: 72 cases (46 positive, 26 negative), 324 raw files, 8,857,480 bytes,
manifest SHA-256
`abbe3c3086ce22c89c394a2798a4b5e7479c5af2bfbf5286cb9310aaf4028a6c`.

`audit_literals.py` is a separate stdlib parser. It ran once:
`INPUT_AUDIT_PASS_NOT_PRODUCT_EXECUTION` (47 Root3 cells and 3058 Job3 cells
checked, no version2 queue magic in any blob, Profile3 ID1 profile blob, the
version2 schedule scalars and hash-presence pattern equal, all 46 positive
frames differ from version2, and the 14 native3 steps whose frame equals a
transition case are exactly the version2 set).

The consumer interface is the version2 one (`cases.tsv`, 37 fields).

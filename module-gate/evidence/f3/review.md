# F3 independent review receipt

2026-11-02 · Reviewer `/root/f3_design_review`, author `/root`.

**REVIEW ACCEPT — D1/D2 (`common`, `policy`), required open findings: 0.**

The reviewer independently inspected `b8cd2c9d..26bb4151`, the current contract,
gate record and original native evidence. All 18 blobs in
[review-files-final.json](review-files-final.json) match the files and record.
The list SHA256 is `996b5686265bef7fd2d6764083a7ef4cdabca169664290a7c40c49e9637ba5be`.

CAT consumes checked operations. C retains the original statement/scope lifetime,
private visibility proof, cleanup error priority, read-set ordering and sole
CheckedTable issuance. EX01–EX04 and the actual reverse references are removed;
the catalog_call ownership registration matches the real physical/logical paths.
The original 65,536B cap and source accounting remain intact. The full Status is
checked before retaining only the decision for the immutable snapshot.

The reviewer matched original results for 44 unique Rust cases, 1 positive and
5 rejection compiler probes, and 4 exact Python cases. The six record CI selectors
cover the changed Pending overlap, cleanup priority, returned-view lifetime and
original drain blocking. The disk N/A covers re-exports only and is supported by
CAT/C tests and planned export compilation.

The controller follow-up uses one lstat and permits only disappearance under the
running worker's live database. Logs, snapshots, spawn/reap audits and the oracle
remain strict; budgets are unchanged. The original r5 N01 PASS, N02 FAIL and N03
NOT_RUN are preserved. The new controller schedules were each attempted once.

Native r6 N01/N02/N03 passed in 4.952/3.638/2.836 seconds. The reviewer verified all
253 original artifacts and the 64 retained logs/receipts, eight distinct workers,
identity/cut/recovery exits, ACK versus UNKNOWN, and Base/CURRENT snapshot hashes.
Input identity independently recomputed as
`cba5e8f976185e061c3275bd3fbf67dc9255bb2698936b33d205c795044da537`.
Aggregate result SHA256 independently matched the record:
`45f49511aa39cc430ff75a302bfb2cd8f434fd44f663a571d650e2e74e330250`.

The reviewer executed no runtime tests. Admission writes alone were explicitly
authorized in the two native revision directories. This review does not treat
unexecuted push hooks, PR CI or export compilation as PASS. Raw native artifacts
were moved only after this review; [cleanup.json](cleanup.json) records their
retained paths and the removal of owned build targets, source exports and ELFs.

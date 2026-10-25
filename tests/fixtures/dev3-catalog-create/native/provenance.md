# Native CREATE independent expected inputs

The parent reviewer adopted `input-plan.json` SHA-256
`d6306d7396ad70165a291a14208c35b7e6e8091f0a4c67e870ea9e96d34e0a34`
before generation. Its embedded `PROPOSED_FOR_ROOT_HASH_ADOPTION` label is the
unchanged proposed source; `input-receipt.json` records the subsequent adoption.

The independent Python writer ran once. It imported only the two SHA-pinned
input serializers listed in the plan, without invoking their entry points or
reading product output. All earlier transition literals remain unchanged. A
separate stdlib parser then audited the new bytes once. No Rust build, product
test, native worker, or database effect was executed by either program.

The result contains 56 steps, 16 logical checkpoints, and 221 distinct raw files
totaling 5,794,810 bytes. The manifest SHA-256 is
`87aa46dd81711cc5080097c674e6fb419e371680ea3e19faa5bafbe3e4847b23`.
The generator and auditor source, stdout, stderr, and input hashes are bound by
`input-receipt.json`; its SHA-256 is
`30c0949937671778e17923db745a753a36a6174e986ca0c1c787305dcd466071`.

## Consumer interface

`steps.tsv` has one header and 56 rows. Each `N01-L01`-style ID identifies one
expected group. The `frame`, descriptor, Jobs, and page-map columns refer to
`raw/<sha256>.bin`. A `-` means absent; a page map contains comma-separated
`role:sha256` pairs. Roles are allocator 0, Root 1, changed Jobs page 2,
Catalog leaf 3, Status 4, Undo 5, table anchor 6, and Row root 7. Before maps may
also retain prior pair roles; `after` is exactly the current group's changed
pages. The actual page number is in each page's original header.

The driver must pin the manifest, index, and TSV hashes from the receipt and
verify every selected raw file's length and SHA. `wal_start`/`wal_end` are
absolute byte offsets within that row's epoch file, beginning at 4096 per epoch.
The complete WAL frame is an external oracle for the actual producer. This is
not permission to construct semantic or durable grants from fixture bytes.

`manifest.json` additionally records per-step typed scalar inputs and complete
64-page logical checkpoint maps. Allocated pages carry exact images; Free or
Reserved logical slots use zero images. This does not require stale freed home
bytes to be zero. Selected-base provenance, certificate, original INIT source,
and old-WAL unlink plus directory-sync must be checked by the actual controller.

N01 has 25 steps; N02 has 16. N03 has 15 rows aliasing the exact N01 prefix by
hash, with its own scenario IDs. N03 must keep a separate process/API history:
real successful Commit sync followed by AfterError, UNKNOWN/Fenced, and no
AckObserved. Identical raw bytes cannot substitute for this history.

The input audit is `INPUT_AUDIT_PASS_NOT_PRODUCT_EXECUTION`. Native execution is
still `NOT_ADMITTED` until exact source, ELF, whole-source census, controller,
and cut contracts have been admitted. Only the minimal CREATE checkpoint and
its required rollback/cancel/fault/recrash checks are selected; Row INSERT and
the full DEV3/S5/property inventories remain outside this execution scope.

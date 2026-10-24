# Plan4 independent compact input provenance

Input specification SHA: `a6f0150ce501fa954839440f0cc7db98cf462b72734077f624fca33a608a988c`. Generated from explicit LE offsets and
Python hashlib only; no product encoder, library, binary or test helper imported.
New raw15/161920B plus exact existing independent reference27/112800B =42/274720B.
The dependency path/length/SHA table is in manifest.json. Existing XBS1 literals
provide unchanged control fields; only plan/high-water/allocator digest/trailer
are changed by the independent documented equation. No old product tests ran.

The generator requires the original frozen input document and these retained
dependencies to regenerate an input revision. Runtime tests use only this tracked
directory; ignored artifacts are not runtime dependencies. Generation is
create-new and existing raw/references/manifest refuse overwrite. Failed partial
generation must be preserved and reviewed, never silently replaced.

Full native files are composed from these exact records plus stated zero extents.
Stable15/22069248B includes LOCK; whole lifecycle18 also names writer, FILEMAP and
DWB temporary selector files. Raw generation is not a product PASS.

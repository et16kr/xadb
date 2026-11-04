# DEV3 08 scope4 frame production/comparison — independent review

Date: 2026-11-04. Base: `a5570a38`. Author: `/root`.
Reviewer: `/root/dev3_08_review`; read-only contract/source/log review.

Scope: pure scope4 produce/observe. Each call recomputes a fresh five-image
candidate and privately lends immutable after images to one Write/Compare
emitter. No public emitter accepts an old Candidate. No stage/install or
selected-source/recovery authority is provided.

Pre-D2 accepts this binding through the call. Exact outer/header/trailer checks
come from decode_layout and the computed header; group, descriptor, core and
native compare exact bytes. Redo compares all parsed fields and data, while
the shared parser checks length and zero padding. New-owner previous/digest
are zero; old-bank commitments remain only in core. Short frame preserves the
frame but may follow after preparation; successful output preserves its tail.

r0 passes all6 new cases, build1m39s/runtime0.09s/2245 filtered. The independent
raw section writer receives already separately tested candidate images/core,
but calls no product emitter, descriptor encoder or redo encoder. It is a
frame-format oracle, not a second independent full SMO semantic implementation.
Root/Child leaf/internal frames compare exactly. For Root/Child each of8
checksum-corrected mutations, wire decoding accepts and canonical observation
rejects. Actual before commitments, workspace regeneration, short buffers,
malformed early rejection and source callback errors are covered.

Self-review strengthened the previous-epoch case: apply the first internal
RootSplit's actual five candidate images to the local fixture, then split its
unchanged leaf13 in epoch2. This covers a deeper ChildSplit whose parent is
not the current root. r1 runs only this modified case:1 PASS, build32.77s,
runtime0.03s,2250 filtered. Six unique tests are covered at their actual source
revisions; prior completed suites were not repeated. Current source matches r1.
Engine all-targets/all-features Clippy-r0 passes in1m18s.

Final limited D1/D2 ACCEPT, required_open=0. The reviewer found no required
product defect and checked source, original logs/hashes and scope statements.
structure.rs and candidate_tests.rs only add module declarations; their old
bodies and execution evidence are preserved via follow-up static reviews.
The reviewer ran no tests and edited no files.

The fixture proves local table/tree/Job/U relations, not the complete
Root/Status/ATT/history closure. Return does not protect caller output from
later mutation; actual installation still needs immutable binding or
revalidation. Full canonical closure, source census/F05, live effects and
recovery acceptance remain open.

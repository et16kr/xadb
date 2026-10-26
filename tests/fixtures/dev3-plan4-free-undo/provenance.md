# Plan4 Free Undo independent input provenance

2026-10-25 · D3-PLAN4-FREE-UNDO-001. Prepared by `/root/undo_design` after the
independent D2 and root's fixture assignment. These are new test inputs, **not
product execution, recovery, or DEV3 acceptance evidence**.

The generator uses Python standard-library byte arrays, `struct.pack_into` and
SHA-256. It neither imports nor executes product code, product encoders or
decoders, previous generators, verifiers, or tests. The published field recipes
in the existing Free Undo, mixed System INIT, plan4 population and base inputs
were read as references. The current contract's v2 SystemInit group and v2
payload record are then placed directly into zero-filled arrays.

`input-plan.json` pins 22 original read-only files by repository-relative path,
length and SHA. They comprise 13 plan4 genesis/control references and nine
existing N=0 frames/final pages/descriptors. Their identities are preserved in
the new four-column `raw-index.tsv`; they are not recopied or rerun. Every source
file is checked before and after generation. No original bytes are modified.

The original generator produced **26 new raw files, 1,913,808 bytes**, in 0.014 seconds
under a 30-second timeout and a 64-MiB virtual-memory cap. The index has 48 rows:
26 new inputs and 22 pinned references. `literals/generation-receipt.json` records
the exact generator, input-plan and index SHA values. Receipts are metadata and
are excluded from the raw-file index. No Cargo build or Rust test was run by the
input author during this generation.

The later approved Reserved-recovery input added only two raw files (37,584
bytes), without regenerating or changing those 48 indexed inputs. The current
index has50 rows and28 new raw files totaling1,951,392 bytes. The original
generator, plan and index remain under `literals/history/r0/`, and the original
receipt remains unchanged. `reserved-recovery-generation-receipt.json` records
the append-only generator/input/index identities. The generation-time plan is
preserved under `history/r1/input-plan-at-extension.json`; the current plan also
records the installer owner's subsequently supplied exact observation matrix.

A separate read-only stdlib checker subsequently checked all48 indexed inputs,
22 newly constructed complete frames, seven payload pages, seven allocator
pages and six base sections. It also checked the native chain and the separate
memory gap/cancel chain. The first checker run failed because its check assumed
every odd LSN was Reserve; the memory chain deliberately has Reserve8 after the
N=0 gap. That checker source and failure stdout/stderr were preserved in the
task's `work/` directory. Correcting the checker to use the frozen operation
schedule passed without changing or regenerating any literal. The successful
checker hash and counts are in `literals/independent-check.json`. This is input
consistency evidence only, not a product rejection or runtime PASS.

The original checker sources and both runs' raw stdout/stderr are also retained
under `literals/history/r0/`. Keeping those small files with the inputs preserves
the failed-checker history after the task's disposable work directory is removed.

## Exact inputs

All integer fields are little-endian. Namespace DB is bytes 1 through 16,
timeline1, FileId `0x100000001`; metadata pages0..8 retain their original plan4
bytes and identities. The native sequence follows contract §7:

| Epoch | Work/page | Payload | Reserve/INIT LSN |
| --- | --- | --- | --- |
|1|1/9|Status|1/2|
|1|2/10|Free Undo|3/4|
|1|3/11|Free Undo|5/6|
|2|4/12|Status|7/8|
|2|5/13|Free Undo|9/10|
|3|6/14|Status|11/12|
|3|7/15|Free Undo|13/14|

Each native page incarnation is2; Free slot generation is1. The seven
`native-pageN-init.page` and seven `native-allocator-lN.page` files are full
16,384-byte expected images. Each `native-wal-eN-suffix.bin` is exactly the
complete frames for that epoch, concatenated in order. **It excludes the
4,096-byte physical epoch header and is not a complete WAL file.** Epoch2/3
physical frame headers have the corresponding writer epoch while SystemWorkRef
remains in the same namespace and its work number increases.

The separate memory sequence shares the new native L1/2 Status prefix, uses the
five pinned N=0 frames at L3..7, then independently generates work2/page10/inc2
Reserve8/Undo INIT9, work3/page11/inc2 Reserve10/Cancel11 and work4/page11/inc3
Reserve12/Undo INIT13. Reserve8's physical previous LSN is7 but its allocator
before-image LSN is2. This distinction is intentional. The memory sequence is
never the native crash schedule.

`reserved-cancel-l6.wal` and `reserved-cancel-l6.page` form a third, separate
branch: native L1..5 ends with work3/page11/inc2 reserved, then the original
recovery owner in writer epoch2 supplies Cancel6. The full frame is21,200 bytes;
its descriptor is bytes352..1888, consumed WAL42,400, with no remaining debt.
The16,384-byte allocator retains the page incarnation2 while returning that
slot to Free and preserving allocated Status9/Undo10. A separate read-only
field/hash check verified the new frame, page, previous-descriptor relation and
all48 unchanged inputs; its receipt is `reserved-recovery-independent-check.json`.
This branch does not substitute Cancel6 for the native INIT6 crash schedule.

The installer boundary input in `input-plan.json` reuses native Status9 L1/2
and Undo10 L3/4. Its seven fresh cases include allocator exhaustion, duplicate
registration, dropped reservation, two partial-install panics, WAL write-prefix
Unknown and WAL sync-after-effect Unknown. The fixed WAL file is
`d3-wal-00000001`; the first write at84,496 after Reserve3 trace reset and the
first sync at0 are distinct faults. These add no generated raw files. Their
image/metadata/DPT/descriptor/claim observations remain separate in the plan.

The root's r3 runtime case0 exposed an input setup error: setting only the
allocator frame counter to MAX left its original DPT counter at2, so the actual
pair-invariant check correctly rejected that inconsistent state before the
intended overflow preflight. Root preserved the r3 source and FAIL. The next
input revision copies the original DPT entry and sets both counters to MAX,
preserving pageLSN2/recLSN1/requirementLSN2 and every other field. Its expectation
remains Prepare/Exhausted/Unchanged. The prior plan SHA is recorded in the
current plan, with its bytes under `history/r2/input-plan-before-coherent-counter.json`.
No product code, raw expected bytes, compiler manifest or compiler probe changed
for this correction.

The pure branches share native Reserve1. Status terminal2 is native frame2;
`pure/undo-02.wal` and `pure/cancel-02.wal` supply the other two adjacent outcomes
at page9/inc2/work1. Reserve and cancel are21,200 bytes, Status INIT38,000 and
Undo INIT38,208. XSG header version u16@164 is2 and flags u16@170 is1; the outer
envelope remains v1 and OperationCore reserved fields remain0.

Each reserve descriptor explicitly reserves terminal WAL38,208 and protected
70,976, with step1/page1/frame2/memory114,688/FD1. Finished consumed WAL is42,400
for cancel,59,200 for Status and59,408 for Undo; all numeric remaining fields
become0. The 208-byte difference from the older Status-only scope is not inferred
from a successful decode or a frame length.

Free Undo is a432-byte `XUR1` cell: version1 at4, generation1 at8, every other
byte zero. The page has kind8/level0/object3/gen1, count1/lower132/upper15920,
slot0 offset15920/length432, actual input incarnation and INIT LSN, and SHA-256
over bytes0..16352. An allocated container remains allocated; these bytes confer
no Undo slot reservation, live edge, transaction or reclamation authority.

`base/mixed-l6.xrb` is a whole1-MiB **pure** base v2 input with Status9 and Undo10/11,
allocation work high-water3 and terminal3, all original Root/Jobs pages, and no
active transaction or Aging owner. Its allocation descriptor is terminal work3.
FreeUndo payload records are `[2,8,0,1]`, PageRef at8, object3 at32, generation1
at40, pageLSN=INIT LSN at48/56, page SHA at64..96 and zero96..352. The source
prefix hash is computed from the original epoch1 header plus the six new frames.
Its physical32-MiB/DWB-generation1 values are explicit pure observations, not a
claim that a particular native trace selected this base. Runtime comparisons
must compare independently expected sections and actual control metadata under
the source-owner contract; the fixture cannot issue `VerifiedBase`.

## Helper and semantic mutants

`free_undo_literal_tests.rs` loads the pinned index and returns immutable inputs.
Its frame accessor slices only the frozen suffix file with explicit frame
lengths. Its memory allocator helper copies the complete after-cell from a
frozen independent frame into the original allocator envelope, sets the
input-specified LSN, and seals it. It never accepts a product-produced frame as
an expected-image source.

Frame mutants cover legacy scope, unknown flags, exact208-byte reserve/protected
shortfall, wrong Undo native kind and canonical Free generation2. Descriptor,
native-cell, page and frame seals are repaired as required. Base mutants cover
v1 with Undo, nonzero reserved scope bytes, wrong role tag, wrong ordinal and a
forged original INIT. The forged INIT changes both record and certificate to
canonical page10/LSN3 and repairs all hashes, while the immutable actual INIT
input remains LSN4. Only source provenance rejects that otherwise self-consistent
claim; a resealed digest is not authority. Base section and trailer hashes are
recomputed for every mutant.

These mutations are inputs to newly changed consumers. Their construction is
not evidence that the product rejected them. Product runtime, native crash
worker, compiler and F05 execution belong to the assigned owners and their
separately frozen source/ELF receipts.

The compiler directory freezes one unmodified baseline, one positive and five
negative programs, including their exact Rust diagnostic code and primary
span. The runner pins the manifest SHA, injects only a disposable private child
module of the real population owner, and invokes Cargo check without running a
product binary. A completed diagnostic with invalid evidence blocks automatic
continuation. Successful prior evidence requires the same source, manifest,
budget and tool bindings; the captured raw, overlay and retained compiler
metadata are revalidated. Artifact growth is monitored during compilation and
projected metadata-copy bytes are checked before preservation. Compiler
execution belongs to the root's frozen-source run and is not claimed here.

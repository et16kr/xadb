# Plan4 G5 clean owner inputs

These are new consumer inputs on base `b3098c1ffe0d555c0021ceb9a4ee5ce958dd7634`.
No raw generator is supplied or run. The 42 original independent inputs in
`../dev3-plan4-genesis` remain immutable (274,720 bytes). Their manifest is
`82880ea05480b9779bb2d5764c88f6423d2c74da1d4e68e8bf2bb0092e8eba22` and raw index is
`0c057920532b389a678c4fc1ea74e82d86a4d457dbd90b8d88ff16cfedd09e13`.
The new manifest records every original path, length and SHA. Runtime uses tracked
inputs only. The independent Python whole-file recipe consumes those original
bytes; a product encoder is never the expected-value oracle.

`manifest.json` has 78 mandatory cases (75 memory, two type-only static, one
native) and 128 additional S5 cases (126 memory, two native), 206 total. S5
functions are ignored and separately selected. There are ten mandatory and five
S5 selectors, fourteen distinct because the native worker is shared. Compiler
ten and harness checks are separate inventories. `execution-recipes.json`
resolves all runtime IDs to their actual operation, mutation/fault key and error
or positive postcondition. Test source hashes bind those recipes to code.

All memory setup calls the actual Fresh plan4 constructor and DWB installation
for the new G5 consumer. It never invokes an old completed test function. The
new native worker likewise performs one actual fresh setup, then bind and clean
queue/Catalog/queue use in one process. It has no live-recovery or three-process
claim. Before/after indexes compare the complete fifteen files (22,069,248 bytes)
and their identity; no duplicate database snapshot is needed. Every G5 interval
requires persistent mutations and entropy zero. The first setup entropy result
and original source/ELF/input/PID are recorded separately.

The dirty-bit negative inputs deliberately damage a registered clean frame.
They do not create a valid dirty WAL requirement, a fake flush, or a runtime
proof of dirty-check ordering. Ordering and the complete candidate scan remain
source-review obligations. B09 deliberately forgets a real reader; its one
retained File and reader-active claim are preserved, not labelled cleanup zero.
Q03 and Q04 similarly retain the actual pin/content guard they test.

B08 uses actual forwarding Writer and File types, with a fixed ordered padding
set of 256, 512, 1024, 2048 and 4096 bytes. ST01 reports original Fresh/genesis
admission, original DWB owner/plan sizes and new G5 peak. The first type which
passes every original admission and fails G5 admission is the only permitted
runtime candidate. No candidate means HOLD. `gate-execution-input.json` records
the source/ELF/input identities, original completed ST01 receipt path and SHA,
and chosen padding. The runner checks the original stdout/marker/hash and sets
the environment value without changing source or repeating static cases.

The initial artifact described B08 as an oversized Writer and mentioned
compiler180 seconds. Before any execution these are superseded by the actual
forwarding File candidate rule and compiler120+audit30=150 seconds. Current
static10+audit5=15 and memory180+audit30=210 are separate aggregate budgets;
native30 includes its final one-second reserve. Native DB24MiB, P1 history64MiB,
all-profile history128MiB and logs8MiB remain unchanged. Compiler diagnostics
remain8MiB. Preparation records32MiB and all prepared artifacts704MiB are
independent root-enforced aggregate caps. S5 memory600+audit30=630 seconds and
32MiB output are reserved for final acceptance, not executed here.

Only source and metadata authoring has occurred. Build, product execution,
static measurement, compiler probes and harness checker results belong to the
root's later immutable receipts. No preparation entry is execution evidence.

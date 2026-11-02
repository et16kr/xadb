# F4 Row operation independent review

Date: 2026-11-02. Author, contract owner and integrator: `/root`.
Independent reviewer: `/root/f4_design_review`.
Base: `5a904b09cee433d74440377acfcc41558fb73075`.
Branch: `codex/module-followup-f4`.

**D1/D2 (common, policy): ACCEPT. Required open findings: 0.**
This accepts the design, implementation, changed-code selection and documentation
at the exact blobs below. It is a static review, not an executed test result.
Product test execution remains pending CI; the reviewer executed no tests,
compiler checks, gate checks or document checks.

## Design and implementation

The selected associated-state protocol makes a concrete reduction in change
propagation: three A implementations stop spelling C's thirteen lifetimes and
namespace parameter at every transition. C owns the related next states, while
the initial Boundary GAT still names the concrete preflight. This is a bounded
improvement, not complete concealment of C's borrow graph.

The alternatives in [F4](../../../devdoc/plans/module-independence-followup-plan.md#operation-api)
were considered. Keeping all concrete implementations retains the repeated
consumer coupling; aliases would retain that same dependency. Sealed consuming
traits remove those repeated signatures while preserving static dispatch and
the existing owners. Heap or erased dispatch adds unnecessary lifetime/resource
complexity to this unit.

All three implementations forward to the original inherent methods. The
finish/cancel unit mapping and apply observation mapping are the former A
mappings, now in C. Owned Before errors retain the original state; Fatal errors,
cleanup priority and active-reservation Drop fence are unchanged. The associated
Reserved/Prepared equalities retain the state family. Borrowed concrete lanes
remain tied to the mutable receiver. No new issuance path, allocation, unsafe,
dynamic dispatch, persistent format or WAL effect was introduced.

Rollback, recovery and commit adapters are unchanged. EX05–EX07 and the remaining
initial concrete GAT are explicitly pending. This review does not close all F4.

## Resource and oracle review

The original eight-phase wrapper census and write.rs blob are unchanged. The
new operation census adds each C bridge's receiver/return carrier to that base;
reopen has a separate ninth phase. Moved inputs are not counted as simultaneous
independent values, and A's existing conservative mapping/return slots remain.
The eight names are asserted before positional additions. Fixed A bounds
6,144/11,520 B, charge 44,304 B and shared-plus-charge cap 65,536 B are not reduced.
The new two census cases validate these source inventories for memory/native
types; they do not claim measured stack or RSS.

The compiler wrapper runs one positive and five negative probes in a disposable
copy of the actual crate, with a separate Cargo target. The negative oracle
requires a completed build, exactly one compiler error, the expected E0382/E0505
code and the exact primary probe path/line. Positive compilation precedes the
negative cases. Original/probe/checker hashes, toolchain, compiler JSON/stderr
and probe text are retained. Compiler waits are bounded to 180 seconds each and
the stated total budget of 420 seconds; helper timeout/descendant failures fail
closed. The harness performs cargo check only, not product test execution.

The Rust wrapper captures child output with Command.output. Compiler evidence is
written under an exclusive subdirectory of the existing CI receipt directory;
only copied source and target are deleted. Memory/native census files use
separate create-new names and println uses libtest capture. These changes preserve
the trusted runner's exact test-result lines. The runner and workflow are
unchanged.

Two required review findings were resolved before test execution:

1. Changing the shared eight-phase census would make V09 reject reopen and
   charge CLR/recovery for bridges they do not call. The base was restored and
   the new nine-phase inventory isolated.
2. Inherited OS stdout could split libtest result lines. Captured child output
   and separate receipt files now preserve both result parsing and detail logs.

## Frozen selection and evidence limits

The [record](../../records/20261102-f4-row-operation.json) selects, after deduplication,
14 head cases (nine changed live paths, two A caller inventories, two new C
operation inventories and one new compiler wrapper), plus three of those live
paths on export-a. Export-b selects no product tests. The export-a cases exercise
prepare Before, apply Before/reopen and refused cancel/fencing through the
changed A adapter in the independent C+A source. The six compiler probes are
inside the single wrapper case, not six extra libtests.

Every selected case file and explicit fixture/helper input appears in the
reviewed blob inventory. The fixture loader pins metadata/raw-index hashes and
validates each consumed raw file's hash and size; the full source identity and
reviewed oracle preserve the omitted raw-file identities. No unchanged whole
A/JOIN, owner/view, V09, CLR/recovery or workspace suite was added. Existing
failures and results retain their original source scope.

The reviewed selection's SHA-256 is
`7453d866305a2192fd98606d9c09783d27e823f610e9c19666c0a49bec7038cf`, computed over the record's
`rust_tests` value using UTF-8 JSON with sorted keys, compact separators and
ensure_ascii=False. Review metadata may be populated afterward; changes to this
selection or any reviewed blob require review again.

The contract, plan, evidence and entry-point updates accurately separate this
implemented first unit from pending CI execution and residual F4/F5-b/F6 work.
The earlier compile-r1 result is limited to its partial source and is not
presented as execution of the final harness. CI receipts/logs and exact source
identities must supply the final test verdict. Human PR merge remains required.

## Reviewed blobs

All 40 entries were independently rehashed from the current files and matched
the supplied manifest. Manifest file SHA-256:
`5d61daaeb520f44e8519f6fe0f3f89d4c2db6006a15663350378e6bfd6cd204a`.
The inventory below makes the review independent of the temporary manifest path.

| Path | Git blob |
| --- | --- |
| `crates/engine/src/storage/disk.rs` | `f62ac8bc168cd9002d83bb86f0a650d806f2e94b` |
| `crates/engine/src/storage/row/insert/driver/live.rs` | `c790ce6ce729c6ed49b0f4bbc275cbaa8a958526` |
| `crates/engine/src/storage/row/insert/driver/tests.rs` | `69f4ac97aef329c69b52f6ca7f12fda73352b3e8` |
| `crates/engine/src/storage/wal_io/disk_api.rs` | `453d598c7571116dd03793c1d821a4ba3a969e7f` |
| `crates/engine/src/storage/wal_io/plan4_apply.rs` | `6b7241a981518283f612e86b6678829fc418842a` |
| `crates/engine/src/storage/wal_io/plan4_apply/row.rs` | `fec737925295891d2254b463ec66606beef2bf09` |
| `crates/engine/src/storage/wal_io/plan4_apply/row/write_operation.rs` | `5b67ca6e9fa2a2c0ce05ba94557a5711fde55d46` |
| `crates/engine/src/storage/wal_io/plan4_apply/row/write_operation_tests.rs` | `6d4f1549177da5e993a111a5a5b652c1715d5c69` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory.rs` | `a8b55d9ecaba53e637d7761edd4a6d80a30baf56` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_cancel.rs` | `5cd3dc93290bad9e90848548c59441b2fb3db1ab` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_forward.rs` | `3de238d9083e2e8d0b765f0a07d505e6f8043b0f` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_live.rs` | `670db220b08918af3eef20cfbe2d382c919ddea6` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_preflight.rs` | `d3d25fc5bd7de844ba9008c5c844dd1d9a2b366f` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/support.rs` | `39d7b974eb439baa52e3d81fb3f7d657b359f8cc` |
| `devdoc/README.md` | `bc459d7d1ce71030aaecae3f2632d413a478d749` |
| `devdoc/contracts/dev3-module-boundaries.md` | `32a4bbd01e63175c8175bad9cdc1ef871c35118b` |
| `devdoc/evidence/module-independence-survey-20261026.md` | `23b711e92ab3c6a562c238ea537dffe02bcbca48` |
| `devdoc/implementation/README.md` | `857f0d259eccee6d03340ac060b27a54231880d2` |
| `devdoc/plans/module-independence-followup-plan.md` | `391c3bd2ebfb4b578fc29c5322f52e016ef84af2` |
| `scripts/check_dev0.py` | `e89dc3c2d2bb62f0c457e7d58cdfc522a160306a` |
| `scripts/check_dev3_buffer_borrows.py` | `fe612d5e7b64b1f3c82250010ea1505bad8af6c5` |
| `scripts/check_row_write_operations.py` | `7458072749de013dad86c9644d2916d17d280e01` |
| `scripts/comm_campaign.py` | `6cc6ad6d0c93858058389d91a7b887ad7da6c68b` |
| `tests/fixtures/dev3-catalog-create/memory-gap3/checkpoints.tsv` | `6f3446bf6481a41c600b2efae9f73d2d6e028103` |
| `tests/fixtures/dev3-catalog-create/memory-gap3/input-plan.json` | `fce1344f340501bf5fd70cedf42df08ccec4cb00` |
| `tests/fixtures/dev3-catalog-create/memory-gap3/manifest.json` | `ff503d8d3a09aac04c5f8181d214ad920450b32e` |
| `tests/fixtures/dev3-catalog-create/memory-gap3/raw-index.tsv` | `3a6138e6ee782cf707757c3bc5b14b1cae4349b1` |
| `tests/fixtures/dev3-catalog-create/memory-gap3/steps.tsv` | `2ff4e6e0744491d795ea3316c9e633a904b9e1b5` |
| `tests/fixtures/dev3-catalog-create/native3/checkpoints.tsv` | `87515859d59fc99fdf5e066f2645358841edd7d7` |
| `tests/fixtures/dev3-catalog-create/native3/input-plan.json` | `03c6fbab7925039f01ce8aeb408489b4a562b8f4` |
| `tests/fixtures/dev3-catalog-create/native3/manifest.json` | `d9aa122b21f674ebf5e2a3dedf5997033b009cc7` |
| `tests/fixtures/dev3-catalog-create/native3/raw-index.tsv` | `c11bc84ae98cd0a600c07e07adfdcf8d490da348` |
| `tests/fixtures/dev3-catalog-create/native3/steps.tsv` | `a61d2a8df696e078cf7cc467d57a9f2c504ddb42` |
| `tests/fixtures/dev3-catalog-create/transition3/cases.tsv` | `f01316b40869de202bae06be342136b4631ed0cd` |
| `tests/fixtures/dev3-catalog-create/transition3/input-plan.json` | `d23a955fbb595eb379889acb9c705df5c6338fc8` |
| `tests/fixtures/dev3-catalog-create/transition3/manifest.json` | `3e813d30a0937f51a1c8c056e0702f28178b430b` |
| `tests/fixtures/dev3-catalog-create/transition3/raw-index.tsv` | `784622c0d5eabdd85eb1c9750465b5266d09bf5f` |
| `tests/fixtures/dev3-module-c3/row-write/expected-cancel.json` | `2fe7d124cba1551169abc01dce60ab90bc41ee20` |
| `tests/fixtures/dev3-module-c3/row-write/expected-forward.json` | `99ecadded7ff4431ceae2abb13a8be70cd53a5d3` |
| `tests/fixtures/dev3-module-c3/row-write/expected.json` | `ec4f34706a579035671405bc153ac44e557648de` |


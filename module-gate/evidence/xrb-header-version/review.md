# XB-H001 Base header version expectation: independent review

2026-11-06. Implementer `claude-thread-session/claude-project-thread-m0owu0`,
reviewer `claude-independent-review/xrb-header-version`.
Commit `1766019` on main `1c9486a`, branch `claude/project-thread-m0owu0`.

D1 **ACCEPT WITH REQUIRED CHANGES**, required_open1. The test change is
accepted as is. One sentence in the evidence overstates what the new case
covers. The reviewer did not edit, commit or push anything in the repo. The
reviewer's own diagnostic ran only in a scratch `git archive 1766019` copy.

## What I checked

- Decoder at 1766019: `read_header_format` (plan4_recovery_base.rs:529-623),
  `validate_header` (:394-436), `fixed_kind` (:379-392), `payload_header`
  (:686-697), the length/trailer gate and `validate_allocation` dispatch in
  `decode` (:1367-1371, :1554-1566). Also `validate_plan4_population`/`validate_reserved`
  /`descriptor`/`finished_*_descriptor` in
  wal_io/allocation_reservation/operation.rs:307-364, 509-545, 842-984,
  1074-1113, and `OperationBudget::terminal_wal/terminal_protected` (:63-86).
- Contract devdoc/contracts/dev3-plan4-free-undo.md §2 (:17-32) and §3
  (:34-42). Header-mode matrix in plan4_recovery_base/row_tests.rs:160-215.
  Positive v2 full decode in plan4_free_undo_base_tests.rs:48-53. Free Undo
  evidence dev3-plan4-free-undo-20261025.json: only `free_undo_*` selectors
  ran, so no `xrb_*` selector was re-run.
- Frozen input input-plan.json "XB-H001" (recipe `version@4=2`, expected
  `Unsupported`), provenance.md, vectors/p1/manifest.json:123.
- Shared folder `xrb-header-version-20261105/`. `sha256sum -c SHA256SUMS`
  passes for all 4 files, and the diag log tallies match the evidence table.

## What I ran

- `git status` was clean at HEAD `1766019`. In a separate scratch
  `CARGO_TARGET_DIR`, a fresh `cargo test --locked -p xadb-engine --lib --no-run`
  started at 23:40:34Z, after the 23:36:54Z commit, and compiled the engine
  from source. The ELF `engine-e4e141083d864921` has SHA-256
  `9fbc93c785af9fd779e1a6badb07bdf292f41c8d7134d05ea45bd84200b7612c`,
  464,911,640 bytes. It is bit-identical to the implementer's after-fix ELF and
  to the cached `target/` artifact.
- I ran only XB-H001: `--exact storage::plan4_recovery_base::tests::xrb_header_rejections
  --test-threads=1`, `RUST_MIN_STACK=33554432`, `XADB_XRB_INCLUDE=XB-H001`,
  index `b3f7e7f0…05fef1`, source context = the 1766019 test blob SHA
  `b3c3d537…f5fd5a69`, input `86d008ca…db63fab`. The result was `XB_PASS XB-H001`
  and XB-H002..H016 `XB_SKIP`, with exit 0. My log is byte-identical to
  `after-h001.log` (`841c7804…`).
- Reviewer diagnostic, scratch copy only, never committed:
  - An exhaustive Legacy `read_header` over all 65,536 tags on `active` accepts
    exactly `[(1, V1Status), (2, V2System)]`. Every other tag gives `Unsupported`.
  - A full `decode` of all 15 fixtures relabelled to 0, 1, 2, 3, 4, 5, 6, 256,
    32768 and 65535, then resealed, gives:
    - v1: 15 `Ok(V1Status)`.
    - v2: 13 `Ok(V2System)`, with `reserved` and `reserved32` giving `IdentityMismatch`.
    - All other tags: 120/120 `Unsupported`.

  This reproduces the evidence table independently.
- `cargo fmt --all --check` passed in the repo, read-only. `cargo clippy --locked -p xadb-engine
  --all-targets --all-features -- -D warnings` passed on the scratch copy.

## Judgement

**Root cause: the test expectation is stale, not the decoder.** Contract §3
makes u16@4=2 the Plan4SystemInit scope and has the runtime capture only v2.
§2 says old v1 files are refused at the runtime level, not by the pure codec.
The code matches this. Legacy mode maps 1 and 2 to formats (:537-538), the
header-mode matrix asserts that Legacy accepts V1 and V2 (row_tests.rs:202-209),
and the accepted Free Undo test decodes a real v2 base through the same
`decode` (plan4_free_undo_base_tests.rs:52-53).

V1 and V2 differ only in these places:
- the Undo kind, FreeUndo record and terminal 3 (:386, :602, :692, :417);
- the allocation validator, which is `system=false` or `true`.

None of these touches `active`. Its Finished descriptor is the same in both
formats, because `descriptor(cancel=true)` does not use the budget, and the
Status finish consumes 21200+38000 = 59200 under both.

So relabelling `active` to v2 gives a base that the current runtime could
legitimately produce. Rejecting it would break valid v2 bases.

The two `reserved` exceptions are explained correctly. Only those two
fixtures hold the Prepared slot-32 budget: 38000 at +912 and 70768 at +1008
and +1328. v2 needs `MixedInit114688`, which is 38208, with protected
38208 + 2×16384 = 70976. `validate_reserved` therefore fails, and `decode`
maps that to `IdentityMismatch` (:1566).

**Replacement case: sound and sufficient.** The case clones the pristine
`active` fixture, sets each tag, reseals and requires `Unsupported`. The
values 0, 3, 4, 5 and 65535 are truly unsupported in Legacy mode. 3 and 4
are the useful ones, because they are valid tags in Catalog and Row mode.
The tag check comes right after the length, trailer and magic gates, so no other
check can produce the result. The exhaustive header sweep shows that the
sample covers a closed match. `rejected` still asserts that the input is
unchanged.

The `return` leaves only the `cases` closure. It skips the generic
reseal/reject tail, which would otherwise test the unmodified fixture. It
yields `XB_PASS`, and any panic is still caught as `XB_FAIL` and resumed.
No other case changed. The comment at :365-368 is accurate.

**Frozen input: leaving input-plan.json unchanged is correct.** The harness
never reads the file; `XADB_XRB_INPUT_SHA256` is only format-checked
(plan4_recovery_base_tests.rs:262-273). Its SHA `86d008ca…` is bound by
manifest.json:123, by the approval check at generator.py:455, and by the
module-gate manifests f0-complete/baseline-manifest.json:5056 and
f3/r2-r6 source-sha256.json:5169. The evidence names §9 and the source as the
current recipe, and keeps the 2026-10-24 PASS attached to its own source. The
dev3-recovery-control-base pointer is enough.

**Docs: accurate except as noted below.** The following all check out:
- `:295`, the `evidence.executor_only` path, and the dates: evidence
  2026-10-24, contract 2026-10-25.
- The §3 quote and the §2 budget reference. The 38000/70768 and 38208/70976
  figures.
- The table counts, the log prefixes, the before/after ELF SHAs, the index
  SHA, and 33/86 = 38.37%.

## Findings

**R1 REQUIRED.** devdoc/evidence/main-preexisting-failures-20261030.md:449,
§9.2, says the case covers "Catalog·Row base가 legacy `decode`에 들어오는
경우까지 전체 decode로". That is not what the case covers.

Real Catalog and Row bases are 2 MiB (catalog.rs:28, row.rs:25,
control.rs:188-193). Legacy `decode` rejects them as `Corrupt` at the length
conversion (plan4_recovery_base.rs:1367) before it reads the tag. The case
covers a 1 MiB v1 base carrying tag 3 or 4, which is the Legacy tag dispatch
(:536-542).

Fix: reword, for example "Catalog·Row 형식 번호 3·4를 단 1MiB base도 Legacy header
dispatch가 `Unsupported`로 거절하는지 본다(실제 2MiB Catalog·Row base는 그 전 길이
검사에서 `Corrupt`)", or drop the clause. This is a doc change only. The test
code and the PASS stand.

**O1 OPTIONAL.** Same file :435. "v2는 v1의 Status 전용 내용을 그대로 담을 수 있고
… System descriptor를 더 허용한다" is broader than the code, and :442/:445
contradict it. A v1 base with a Prepared reserve is not valid v2. v1 already
allows System descriptors in slots 32 and 34. What v2 adds is the FreeUndo
record, Undo allocator entries and terminal 3, and it requires the
38208/70976 Prepared reserve. Suggest: "Prepared 예약이 없는 v1 내용은 그대로 유효한
v2이고, v2는 FreeUndo record·Undo 할당·terminal3을 더 허용하며 Prepared 예약은
38208/70976을 요구한다".

**O2 OPTIONAL.** Same file :462. "ELF는 지웠다" is ambiguous. The after-fix ELF
`9fbc93c7…` is still the live cargo artifact in `target/debug/deps/`, and a
fresh `--locked` build reproduces it bit for bit. Something like "ELF 사본은
보존하지 않았다(수정 후 ELF는 재빌드로 같은 SHA 재현)" would be exact. One more
point needs no change: the source-context value `c7eb55ff…` in
`before-h001.log` matches no committed blob. The harness treats it as opaque,
and the evidence does not cite it.

```json
{
  "crates/engine/src/storage/plan4_recovery_base_tests.rs": "e0a4cf18c0fda1bd19487e1b382b1e02166cbe20",
  "crates/engine/src/storage/plan4_recovery_base.rs": "290f035ec4bad2c8b7c1170f4a28637a30f33998",
  "crates/engine/src/storage/plan4_recovery_base/row_tests.rs": "441e4a7a2e6b4f1407420aa1c6de7c4356de34d6",
  "crates/engine/src/storage/plan4_free_undo_base_tests.rs": "482272111ef78aa8d4cf0c33a1e0c731c9b1ffa0",
  "crates/engine/src/storage/wal_io/allocation_reservation/operation.rs": "d4bfb03ec1969a6e5aafe775c7d51789769a5e92",
  "devdoc/contracts/dev3-plan4-free-undo.md": "96ab2101629fc218b306593e3c00f760c398397b",
  "devdoc/evidence/main-preexisting-failures-20261030.md": "0e4bf3a64cfcdac65ae2bf68ccc39122eb51968c",
  "devdoc/evidence/README.md": "f304dc39e1ff8702b2b97ca327f18b803df22c25",
  "devdoc/evidence/dev3-recovery-control-base-20261024.md": "dae60594d7de5ac151b947b16ebc2bbc00b8d673",
  "tests/fixtures/dev3-plan4-recovery-base/input-plan.json": "90ccbb6131482fecbe6a5087cac684ffa035e18c",
  "tests/fixtures/dev3-plan4-recovery-base/provenance.md": "a89764b56fc38f4c5db960e809f1fdefaa2038e6",
  "tests/fixtures/dev3-plan4-recovery-base/vectors/p1/manifest.json": "212286665cb169da98c6dcdb13e1ef8e6844dc6c"
}
```

## Round 2

2026-11-07. Head `c192a4f` on `1766019`, branch `claude/project-thread-m0owu0`.

D1 **ACCEPT**, required_open0. Round-1 R1 is closed, and O1 and O2 are
applied. The reviewer did not edit, commit or push anything in the repo.

What I checked:

- `git diff --name-only 1766019 c192a4f` lists only
  devdoc/evidence/main-preexisting-failures-20261030.md (3 lines changed).
  These blobs are unchanged from round 1:
  - the test file `e0a4cf18…`;
  - the decoder `290f035e…`;
  - input-plan.json `90ccbb61…`;
  - README.md `f304dc39…`;
  - dev3-recovery-control-base-20261024.md `dae60594…`.

  No Rust source embeds this document, so the round-1 build, ELF
  `9fbc93c785af9fd779e1a6badb07bdf292f41c8d7134d05ea45bd84200b7612c`, and
  `XB_PASS XB-H001` still apply. No rerun was needed.
- R1 (:449) now limits the claim to a 1 MiB base carrying tag 3 or 4. It
  states that real 2 MiB Catalog/Row bases are refused as `Corrupt` at the
  length check before the tag is read, and so are not covered. This matches
  plan4_recovery_base.rs:1367, catalog.rs:28, row.rs:25 and
  control.rs:188-193.
- O1 (:435) is now accurate:
  - v2 adds the FreeUndo record, Undo pages and terminal 3.
  - A Prepared reserve needs 38208/70976 under v2 and 38000/70768 under v1
    (operation.rs:63-86).
  - `active` has no Prepared reserve, and its finished Status INIT consumes
    59200 = 21200 + 38000 under both formats (operation.rs:1087, :1111). So
    relabelling it gives a valid v2 base. This agrees with the table at :442
    and :445.
- O2 (:462) now reads "ELF 사본은 보존하지 않았다", which is exact.
- The remaining "ELF는 지웠다" (:245) and "System descriptor" (:403) are in
  earlier sections, outside this change.

```json
{
  "crates/engine/src/storage/plan4_recovery_base_tests.rs": "e0a4cf18c0fda1bd19487e1b382b1e02166cbe20",
  "crates/engine/src/storage/plan4_recovery_base.rs": "290f035ec4bad2c8b7c1170f4a28637a30f33998",
  "devdoc/evidence/main-preexisting-failures-20261030.md": "ded32f2440831b1fede02ea9d6295a969c332e05",
  "devdoc/evidence/README.md": "f304dc39e1ff8702b2b97ca327f18b803df22c25",
  "devdoc/evidence/dev3-recovery-control-base-20261024.md": "dae60594d7de5ac151b947b16ebc2bbc00b8d673",
  "tests/fixtures/dev3-plan4-recovery-base/input-plan.json": "90ccbb6131482fecbe6a5087cac684ffa035e18c"
}
```

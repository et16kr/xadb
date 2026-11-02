# Changed-path CI selection independent review

Author/contract/integration: `/root`. Independent reviewer: `/root/f4_design_review`.
Base: `aa87366d299cb89515161cd46ced7faa1972fd1b`.

The reviewer independently confirmed that the existing touched-A selection expands
to all A/JOIN cases in head and all A cases in export-a, with no narrowing record
mechanism. A Python-only prerequisite PR is appropriate before F4 product edits.
F4's associated-state direction received conditional design acceptance only;
F4 implementation, forwarding-frame census and EX05–07 remain pending.

For the prerequisite the reviewer required strict mode/schema validation,
same-record current-blob review of each changed path and selected case/input,
exact-path N/A, export applicability, preserved deleted identities, mandatory F2
union, receipt source binding and separate trusted-policy adoption.

The implementation review found one required fix: check `..` path segments before
`normalize()` so malformed input yields a FAIL report instead of an uncaught
GateError. The author applied that fix before any test execution. The reviewer
found no other required code issue in that pass. No reviewer tests or edits ran.

Final immutable-blob verdict and CI results are recorded separately; this design
review alone is not execution evidence.

## Immutable implementation verdict

`a8772885c9d09f24bc194a8db54d1c0f7f696d1f`: D2 ACCEPT, required_open 0,
including the three gate-required blobs and the policy/documentation diff.

Before execution the author also found that the legacy literal scanner could name
a test written inside a macro token tree. `a36d71a0dae8adb499e01a7bb19874653394b160`
adds token-tree exclusion only for new scoped mappings; legacy/F2 discovery stays
unchanged. The same selected negative now covers definition and brace/paren
invocations. The reviewer inspected the updated checker/test blobs and returned
**D2 ACCEPT, required_open 0**:

- checker: `f23bdafff758f965392ce192bfca804d3ff31600`
- tests: `7c6a62f0ed86bf4b8f5b39ce4d8a4b0f2b940289`
- manifest (unchanged since first verdict): `6d53b7eac89d5dc810f724d6c4555931af727ea9`

The exact 17 selected Python cases were judged relevant and sufficient for this
diff. This is static review acceptance, not a test PASS. CI will execute them
once on the submitted head, and its source-bound receipt retains the result.

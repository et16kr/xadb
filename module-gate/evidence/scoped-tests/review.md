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

# F4 caller budgets independent D1/D2 review

Reviewer: `/root/f4_design_review`. Author: `/root`. Verdict: **ACCEPT**.
Required open findings: **0**. Scope: implementation, common budget contract,
policy adoption, exact CI selection and legacy wrapper N/A.

All 27 blobs in [review-files.json](review-files.json) match the reviewed files.
Manifest SHA-256: `dccc25781b82318e562d9ca3a9e2965c9acb8df27a173656bf7561e9d6bb207b`.
The selection record before review metadata binding had SHA-256
`d553fbcb854beff4bfd941601a01bd286d3334aa528b5e9459363c9fcb1656be`.

A retains the original slack, dead slots and owned results. C retains all four
Commit windows and rollback/recovery, flush and gate-wait sums. The ten rollback
sites reject missing, duplicate and unbudgeted entries. Both sides use the same
native instrumented Writer type. The fixed budget/actual inventory split removes
real C-to-A dependence; EX05–EX07 may close without changing charges or the cap.

Head 7, C+A 2, C+B 5 and Python 2 exact cases cover this diff. The three legacy
wrapper files are exact N/A because they merely forward to the new tested body;
all-target compilation checks them without executing that body twice. Recovery
C03 is unchanged. The old trusted export may remove the three wrappers while
retaining all new C census implementations. Checker, runner and workflows are
unchanged. Documents correctly mark execution as NOT_RUN pending CI.

The reviewer executed no tests, compiler or gate and edited no source. This is
static acceptance, not numerical or execution evidence. CI receipts must confirm
actual bounds and each selected source. Selection/N/A changes require re-review.

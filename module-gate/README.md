# Module gate records

One JSON file per change (pull request) under `records/`, read by
[the module gate](../devdoc/policies/repository-build-plan.md#module-gate).
The gate reads only the records that the change adds or modifies
(merge-base..head). A record is not an approval: the human merge is
(user decision 2026-10-31). The gate checks that each review covers the exact
blobs of the head and that attested results match the head's inputs.

```json
{
  "schema": "XADB_MODULE_GATE_RECORD_V1",
  "change": "what the change does, one line",
  "python_tests": ["test_module_gate_python.PythonPlanTests.test_python_change_selects_exact_tests_without_rust"],
  "ci_selectors": ["storage::wal_io::...::case_or_module::"],
  "na": [
    {"module": "codec", "reason": "why no test can show this change"},
    {"module": "-", "paths": ["tests/fixtures/dev3-x/**"], "reason": "read by no lib test"}
  ],
  "reviews": [
    {
      "id": "s12-d2",
      "class": "D2",
      "scopes": ["policy"],
      "author": "implementer identifier",
      "reviewer": "separate reviewer run identifier",
      "verdict": "ACCEPT",
      "required_open": 0,
      "files": {"path": "head blob id or deleted"},
      "log": "where the review text and its hash are kept"
    }
  ],
  "attested": [
    {
      "selector": "script:scripts/dev3_catalog_native.py ...",
      "input_identity": "from check_module_gate.input_identity at the head",
      "result": "PASS",
      "log": "artifact path",
      "log_sha256": "64 hex",
      "reviewed_by": "who read the original log"
    }
  ]
}
```

- `files`: print it with
  `python3 scripts/check_module_gate.py files --base origin/main --head HEAD`
  after the last code change; a later change to a listed file invalidates it.
- `class`: `D1` for product, test and script changes; `D2` with scope `policy`
  for the manifest, checker, workflows, export tool, build runner, push hook
  and the two module contracts; `D2` with `joint` when two features (A, B,
  Catalog) change together; `D2` with `common` when the disk API changes or a
  file that defines what it re-exports, re-exports it on the way, or adds an
  `impl` to such a type changes (the gate lists those files in its notes).
- `ci_selectors`: libtest filters the CI build runs on the head in addition to
  the touched modules' own selectors and the exact `#[test]` cases the changed
  files declare. Module paths only (`a::b::` or `a::b::case`); a leading `-`
  is rejected. Every listed case must pass or be `#[ignore]`; a selector that
  matches nothing fails.
- `na`: a touched module with no selector of its own (the common modules, C,
  engine) needs CI evidence: a changed case, a `ci_selectors` entry inside it,
  or an `na` entry with a reason in a record whose review is valid. A changed
  Rust or test input that no module owns (fixtures outside A/B under `tests/`,
  other crates, root Cargo files) needs a `ci_selectors` entry or an `na`
  entry whose `paths` globs name it. A changed file whose cases are listed under
  the manifest's `evidence.executor_only` (they need inputs only their
  executor supplies, such as the XRB fixture selection) needs an `na` entry
  whose `paths` name the file and whose reason gives the executor's result.
  An `na` entry counts only when a valid review in the same record lists
  every path it exempts (the module's changed files, or the changed files its
  `paths` match) at the head's blob: an N/A copied from another change's
  record exempts nothing. Cases whose cfg is not known on the CI runner (a
  cargo feature) run when they exist but never count as evidence.
- `attested`: only selectors the manifest allows (natives that need more than
  30 GB of disk); everything else runs in CI. A change to CAT needs a
  `dev3_catalog_native.py` result at the head's inputs.

- `python_tests` ([F1 contract](../devdoc/policies/repository-build-plan.md#module-gate-python)):
  exact `test_module.Class.test_method` IDs from the changed input's allowed modules
  in the trusted manifest. Declare only cases that directly exercise this diff.
  A registered Python/tool/workflow change without a matching declaration fails.
  The D2 reviewer must check relevance and sufficiency; matching a module name
  alone is not proof of semantic coverage. No whole-suite default, skips or zero
  tests count as PASS. Python-only changes do not run Rust product tests.

F2의 `affected_contracts`는 [공통 계약 선택 규칙](../devdoc/policies/repository-build-plan.md#module-gate-contracts)을 따른다. 신뢰 manifest가 직접 변경 경로·입력에서 안정 사례 ID와 `head`/`export-a`/`export-b` 요청을 정한다. 일반 record `ci_selectors`는 계속 head 대상이다. export 실행이 필요하면 검토된 계약 등록의 `sources`를 사용하며, head가 그 등록을 지워도 현재 PR의 신뢰 의무를 면제하지 않는다. 사례 이름·입력 blob·source가 selection과 receipt에 결속된다.

F5-a부터 `affected_contracts[].owner`는 `test_file`의 등록 소유자와 같아야 한다. 단독 export의 제거·등록 정보도 같은 manifest에서 유도한다([계약](../devdoc/policies/repository-build-plan.md#module-gate-test-ownership)). 수동 실행은 `python3 scripts/module_single_source.py --commit <head> --policy <trusted-policy> --side a --out <export> --record <evidence>`처럼 정책 revision을 명시한다. 과거 결과는 해당 시점 helper/policy로 재현한다.

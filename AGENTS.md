# XADB repository guide

## 테스트 실행 제한 — 2026-10-21 사용자 지시

- **수정이 발생하지 않은 코드에 대해서는 테스트를 실행하지 않는다.** 새로 작성하거나 수정한 코드에 해당하는 테스트만 선택한다. 마지막 검증 이후의 실제 코드 diff를 기준으로 범위를 정하고, 실행 전에 변경 내용과 선택한 테스트의 관계를 확인한다.
- 상위 기능 추가, 작업 단위·commit 변경, 저장소 전체 source fingerprint 변경, 통합 또는 단계 완료라는 이유만으로 변경 없는 하위 코드의 테스트를 반복하지 않는다. 같은 코드의 이미 완료된 테스트도 반복하지 않는다.
- **DEV0·DEV1·DEV2 테스트는 사용자가 금지를 해제하기 전까지 실행하지 않는다.** 해당 코드에 수정이 생겨도 이 별도 금지는 유지한다.
- 금지된 테스트나 변경 없는 코드의 테스트를 포함하는 전체 workspace 테스트·하위 회귀 wrapper·전체 DEV3 leaf 실행도 금지한다. 다른 검사 명령을 통해 간접 실행하거나 필터 없는 일괄 실행으로 우회하지 않는다.
- 기존 테스트 결과는 원래 source·실행 범위의 증거로 보존한다. 생략·금지·중단된 테스트를 새 실행의 PASS로 표시하지 않는다.
- 이 사용자 지시는 아래 안내와 `devdoc/`의 기존 V2/V3·하위 회귀·현재-build 복구·단계 인수 실행 요구보다 우선한다. 기존 요구를 근거로 금지된 테스트를 재개하지 않는다.

## Repository guidance

- Start with `devdoc/README.md`, then read only the task's owner contracts and evidence.
- Apply `devdoc/policies/coding-convention.md`, `development-policy.md`, and `documentation-policy.md`. Existing authorization permits routine work; D1/D2 require independent review. Adoption is not execution evidence.
- Product direction is `devdoc/plans/project-direction.md`: Linux/Rust, a disk-based relational DBMS, understandable internals, broad functionality and measured basic performance. Public DBMS are research inputs, not claims of identical SQL or implementation.
- For public DBMS source research, use [the local source catalog](devdoc/reference/opensource-references.md): ten checkouts under `/home/et16/work`, including PostgreSQL, MariaDB and CUBRID, with source entry points and recorded revisions. Read the relevant parts for the task and record the actual revision used.
- Keep resident-database and mixed-storage product engines out of scope. Ordinary buffers, caches, quotas and the temporary volatile K0 substrate remain necessary.
- Documentation is Markdown only. Keep rules in `policies/`, order in `plans/`, owner contracts in `contracts/`, actual state in `implementation/`, optional detail in `reference/`, inactive examples in `examples/`, and truthful receipts in `evidence/` and `history/`.
- Use XADB terminology in documents and commit messages. Describe migration provenance with neutral source identifiers and hashes; preserve original path inventories in the external local archive.
- Register documents in the directory README and `devdoc/documents.json`. Use relative Markdown links; JSON document IDs resolve through the catalog. Preserve original identity for any historical evidence retained.
- Follow `bottom-up-development.md`, `development-stages.json`, and `stage-completion.json`. DEV2 is partial; volatile tests do not qualify durability.
- Follow `repository-build-plan.md` and its JSON for roles, packages, dependencies, fingerprints and clean exports. Add planned paths only with implementation.
- Read `implementation-contracts.md` before kernel changes and `version-compatibility-policy.json` before published-format or semantic changes. Development builds use fresh fixtures; current-build recovery checks remain mandatory.
- Document verification is a pre-push gate, not an edit or commit requirement. Use the repository's `scripts/git-hooks/pre-push` hook (`git config --local core.hooksPath scripts/git-hooks` when no existing hooks need preserving). It runs `cargo fmt --all --check` and document verification for each pushed ref's tip commit in a temporary export and blocks on either failure. Uncommitted formatting fixes do not satisfy the gate; do not rerun document checks while editing. See `devdoc/policies/documentation-policy.md#push-gate`. Product execution checks retain their stage-specific requirements.
- Historical lower-stage entry points (execution prohibited by the user instruction above): DEV0 `python3 scripts/check_dev0.py`; DEV1 plus DEV0 regression `python3 scripts/check_dev1.py`; DEV2 current P0/K0 leaf plus lower regressions `python3 scripts/check_dev2_reference.py --with-substrate`.
- This is a separate repository. Do not modify the source checkout or import its Git history. Imported prior progress is not a fresh XADB verification result.

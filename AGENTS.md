# XADB repository guide

## 테스트 실행 제한 — 2026-10-21 사용자 지시

- **수정이 발생하지 않은 코드에 대해서는 테스트를 실행하지 않는다.** 새로 작성하거나 수정한 코드에 해당하는 테스트만 선택한다. 마지막 검증 이후의 실제 코드 diff를 기준으로 범위를 정하고, 실행 전에 변경 내용과 선택한 테스트의 관계를 확인한다.
- 상위 기능 추가, 작업 단위·commit 변경, 저장소 전체 source fingerprint 변경, 통합 또는 단계 완료라는 이유만으로 변경 없는 하위 코드의 테스트를 반복하지 않는다. 같은 코드의 이미 완료된 테스트도 반복하지 않는다.
- DEV 단계와 관계없이 새로 작성하거나 수정한 코드에는 필요한 테스트를 수행한다. 특정 단계 자체를 테스트 금지 대상으로 삼지 않는다.
- 전체 workspace 테스트·하위 회귀 wrapper·전체 leaf 실행처럼 변경 없는 코드의 테스트까지 포함하는 일괄 실행은 금지한다. 실행 범위를 변경한 코드에 해당하는 테스트로 좁힌다.
- 기존 테스트 결과는 원래 source·실행 범위의 증거로 보존한다. 생략·중단된 테스트를 새 실행의 PASS로 표시하지 않는다.
- 이 사용자 지시는 아래 안내와 `devdoc/`의 기존 V2/V3·하위 회귀·현재-build 복구·단계 인수 실행 요구보다 우선한다. 기존 요구를 이유로 변경 없는 코드의 테스트를 반복하지 않는다.

## DEV2 PF04 디버깅 — 2026-10-25 사용자 적용 범위 정정

- 사용자는 현재 작업이 개발이 아니라 실패 원인을 조사하는 **디버깅**임을 명확히 했다. 위 개발 테스트 제한을 이 작업의 미실행 원인 구분 진단에 적용하지 않는다. 이는 규정의 예외 승인이 아니라 적용 대상의 구분이다.
- PF04 디버깅에 필요한 새 가설·관측 경계의 계획된 진단과 새 출력의 정답 검산은 기존 원인 분석·해결 지시 안에서 진행한다. 같은 소스라는 이유만으로 매번 추가 허용을 요구하지 않는다.
- 완료한 개발 회귀시험·전체 workspace를 반복하지 않으며, 기존 source별 결과·FAIL·UNKNOWN을 보존한다. 진단의 입력·품질·반복 수·시간 예산·중단 조건을 사전에 정하고, 유리한 결과를 얻기 위한 반복이나 인수 기준 완화로 확대하지 않는다. 재부팅·서비스 재시작 금지는 유지한다.
- 현재 선택한 진단은 [PF04 경계 대조 계획](devdoc/plans/dev2-pf04-resolution-plan.md#boundary-comparison)의 미실행10쌍·최대900초1회다. 기존20회 Off 진단과4개 기능 연결을 반복하는 시험으로 취급하지 않는다.

## 별도 COMM 미완료 시험 — 2026-10-23 사용자 적용 범위 정정

- 사용자는 현재 작업이 개발이 아닌 별도의 미완료 COMM 시험 진행임을 명확히 했다. 위 무변경 코드 시험 제한을 이 작업의 미실행 인수 사례까지 금지하는 것으로 확대하지 않는다. **이 별도 COMM 시험에서는 코드 수정 없이도 미실행·중단된 인수 사례를 진행할 수 있다.**
- 이미 완료한 시험을 반복하지 않으며 원 source·실행 범위의 증거와 FAIL·중단 기록은 보존한다. 이 구분을 다른 DEV 개발의 무변경 하위 회귀 재실행 허용으로 확대하지 않는다.
- 단일 실행이8시간 이상 걸리면 일단 skip한다. 나누어 실행할 수 있으면 결과를 보존하며 진행하되 **2026-10-23 23:00 KST 전에 실행·정리·검토·기록까지 마칠 수 있는 범위만** 선택한다. 필수 inventory·반복 수·측정 길이·품질 기준은 줄이지 않고 SKIP·중단을 PASS로 표시하지 않는다.
- 실제 선택·source별 결과·사용자 정정 기록은 [COMM 분할 시험 증거](devdoc/evidence/comm-performance-batch-20261023.md#standalone-continuation)를 따른다.

## COMM 추가 시험 시간 — 2026-10-23 후속 연장

- 사용자가22:52 KST 무렵 “한시간 정도 테스트를 더” 요청했으므로 이번 추가 시험에 한해 앞선23시 마감을 연장한다. 실행 deadline은23:50 KST, 종료 검토·기록 목표는23:52–23:55 KST다.
- 새5+5 기준·8시간 이상 단일 실행 skip·완료 사례 반복 금지·source별 증거·실패/중단 보존은 유지한다.

## COMM 성능 반복 수 — 2026-10-23 후속 사용자 지시

- 앞으로 COMM 성능 시험은 **기본5쌍(Always/Detailed 각각5회), 편차·판정 불확실성이 있을 때만5쌍 추가, 최대10쌍**으로 한다. 이 범위에서는 위 필수 반복 수 유지와 일반 DP07 최소10쌍보다 최신 사용자 지시를 우선한다.
- 편차·추가·실패·N/A 규칙은 [COMM 적응 반복 계약](devdoc/plans/comm-implementation-plan.md#adaptive-repetitions)을 실행 전에 동결한다. 추가가 필요하면 처음5쌍을 포함한 전체10쌍으로 판정하며 중단·시간 부족을 PASS로 바꾸지 않는다.
- 기존10쌍 결과·원 source/실패/중단은 소급 재분류하지 않는다. inventory·측정 길이·품질 한도·다른 DEV 시험 정책과 당일23시 마감은 유지한다.

## DEV3 테스트 계획 — 2026-10-22 후속 사용자 지시

- DEV3 중간 단계는 새로 작성·수정한 기능의 필수 테스트만 실행한다. 무변경 하위 코드·완료 시험의 반복과 전체 wrapper 실행 금지는 유지한다.
- **DEV3 최종 인수 S5에서는 DEV3의 확정된 전체 테스트 inventory를 전수 실행한다.** 이는 위 전수 금지에 대한 최종 DEV3 인수 한정 예외이며 DEV0/1/2·전체 workspace 전수로 확대하지 않는다.
- 추가 테스트는 각 기능 구현과 함께 새로 작성할 테스트·fixture·oracle·harness를 뜻한다. 중간 필수 사례와 최종 전수용 확대 사례를 각각 선택 가능하게 작성한다. 긴 부하·대규모 데이터·전체 조합 실행은 최종 인수에 배정한다.
- [DEV3 단계별 테스트 계획](devdoc/plans/dev3-test-plan.md)을 따른다. 계획 작성 요청만으로 제품 개발·테스트 실행을 재개하지 않는다. 최종 실행 뒤 수정이 생겨도 관련 diff와 실패 사례만 다시 검증하고 원래 결과를 보존한다.

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
- Document verification is a pre-push gate, not an edit or commit requirement. Use the repository's `scripts/git-hooks/pre-push` hook (`git config --local core.hooksPath scripts/git-hooks` when no existing hooks need preserving). For each pushed ref's tip commit in a temporary export, it runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, then document verification, blocking on any failure. Clippy checks test targets without executing test cases; product tests still follow the changed-code scope above. Uncommitted fixes do not satisfy the gate; do not rerun document checks while editing. See `devdoc/policies/documentation-policy.md#push-gate`.
- Stage entry points are references, not automatic execution requirements; apply the changed-code test scope above before selecting commands. DEV0: `python3 scripts/check_dev0.py`; DEV1 plus DEV0 regression: `python3 scripts/check_dev1.py`; DEV2 current P0/K0 leaf plus lower regressions: `python3 scripts/check_dev2_reference.py --with-substrate`.
- This is a separate repository. Do not modify the source checkout or import its Git history. Imported prior progress is not a fresh XADB verification result.

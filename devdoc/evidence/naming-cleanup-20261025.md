# XADB 내부 명칭과 반복 파일명 정리

2026-10-25 · **구현·변경 범위 검증·독립 검토 완료 / 첫 push gate 문서 오류 보완**

[코딩 규칙](../policies/coding-convention.md#section-3) · [저장소 배치](../policies/repository-build-plan.md) · [관리 JSON](../policies/repository-build-plan.json)

## 결정과 소유

- ID `XADB-NAME-001`, D2. 사용자의 명칭 조사 후 “적용해 주세요, 완료되면 한번 commit” 및 후속 push 요청에 따른 명칭 정리다. 작업·계약·통합 담당은 `/root`, 독립 검토는 `/root/naming_review`다.
- 기준 HEAD는 `f15784600b7c2e61f5daa14901e37a84129bf248`이다. 착수 당시 방향 조사 문서와 README·색인·PF04 계획의 미커밋 변경을 보존했다. 작업 중 다른 DEV2 I/O 코드와 방향 문서의 변경도 확인하여 이 커밋에서 제외한다.
- Cargo package·외부 bin·환경변수·wire·원 실행 증거 식별자는 유지한다. Rust library target과 dependency alias만 책임 이름으로 바꾼다. 패키지명까지 바꾸는 대안보다 외부 실행·증거 결합의 변경 범위가 작다.
- 기존 import 이름 유지 방침은 이 결정의 범위에서 대체한다. 동일 기능 접두어를 파일마다 반복하던 배치는 기능 폴더로 표현한다. 실제 bootstrap 종류인 plan4와 타입의 의미·권한·지원 범위는 유지한다.
- 공유 Cargo manifest·모듈 선언·fixture·checker·정책은 총괄이 순차 수정했다. 독립 검토자는 읽기 전용으로 점검했다. 제품 함수의 동작·저장 형식·의존 방향 변경은 없다.

## 적용 범위

| Cargo package | Rust library / dependency alias |
| --- | --- |
| xadb-foundation | foundation |
| xadb-platform | platform |
| xadb-engine | engine |
| xadb-transport | transport |
| xadb-sql | sql |
| xadbtest-protocol | test_protocol |
| xadbtest-reference | test_reference |
| xadbtest | test_controller |
| xadbtest-engine-worker | engine_worker |

12개 package, 97개 target과 모든 11개 bin 이름·required feature를 Cargo metadata로 대조했다. normal/build/dev 의존 package·feature·optional/default 조건과 Cargo.lock은 유지한다. library 9개의 target 이름만 변경한다. 두 durable source는 명시적 bin target으로 기존 명령에 연결한다.

wal_io의 plan4 retention/recovery/pages·empty transactions·Status home flush child를 재배치했다. `#[path]`와 include 경로를 함께 갱신하고 논리 모듈 경로·visibility·기존 시험 selector를 보존했다. controller의 private 조립 모듈은 runner로 바꿨으며 공개 re-export는 유지한다. 빈 거래 checker/test는 기능 폴더에 묶고 직접 CLI와 package import를 지원한다.

현재 compiler fixture의 import와 bytes/SHA, 관련 runner pin을 갱신했다. fixture의 기대 오류·행·열·사례 ID·반복 수·예산은 줄이지 않았다. G5 compiler의 이전 완료 이력은 보존 source Cargo.toml의 lib.name 또는 원 package 이름으로 target을 확인한다. 원/새 target을 무조건 함께 허용하지 않는다.

DEV2의 `substrate-xadb_engine` receipt ID와 문서의 기존 검증 의무는 유지한다. G5 execution-recipes와 retention-live input-plan의 원 source/source_files·작성 당시 link·provenance는 이전 작성 시점의 identity이므로 보존하고 current_source_paths를 탐색용으로만 추가했다. retention-live의 현재 link는 current_test_link에 별도로 기록한다. source_history·원 generator·supersedes 정보는 새 실행으로 해석하지 않는다. Status flush native의 실제 입력 검사에 쓰이는 source dependency는 현재 경로·bytes·hash로 결합했다. 이미 불일치하던 원 pin과 착수 baseline identity는 source_history에 함께 보존하며 native 실행 결과를 승계하지 않는다.

## 선택 검증과 원 실패

명칭·배치 변경을 직접 검증했다. 제품 전체 시험·DEV wrapper·완료 사례 전수·성능 campaign은 실행하지 않았다. 전체 target check/Clippy는 컴파일만 하며 test body를 실행하지 않는다. 각 결과는 해당 실행 당시 source 범위의 증거이며 단계 인수 완료를 뜻하지 않는다.

| 검증 | 실제 범위·결과 |
| --- | --- |
| Cargo check | `cargo check --workspace --all-targets --all-features --locked` PASS. 최초 실행의 engine protocol module/import 충돌 E0432와 파생 E0614는 FAIL로 보존하고 `::engine` 명시로 수정한 뒤 재확인 |
| Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` PASS |
| Rust format | `cargo fmt --all --check` PASS |
| checker 자기시험 | Python 선택12개 PASS. 이력 target의 원/새 source 수용 및 반대 target 거절, 이동 checker binding, 변경 fixture/metadata target, package dependency alias를 확인 |
| 최초 Python 실행 | 이동 checker binding 1개 PASS, 신규 이력 test는 fixture hash 갱신 미완료로 FAIL. 입력 연결을 수정한 뒤 실패한 test만 재실행해 PASS. 이미 성공한 binding test는 반복하지 않음 |
| 실제 compiler probe | 이동 checker의 `--case bank_release --case result_bank_alias`: baseline·정상 대여 해제·E0499 거절의3개 PASS. 다른 사례 NOT_RUN, runtime 실행0 |
| controller CLI | `cargo test -p xadbtest --features self-test --test controller --locked cli_exit_code_matches_persisted_verdict_and_rejects_unknown_arguments -- --exact`: 1개 PASS, 나머지13개 미실행 |
| 실행 진입점 | 이동 checker/test의 직접 `--help` 실행 PASS |
| 정적 대응 | Cargo package12/target97/bin11, 이동54, 변경 manifest의 fixture173개 bytes/SHA, 최초 대조에서 역사 파일268개·이관 JSON·lockfile 보존 확인. 후속 push gate에서 DEV4 handoff1개는 탐색 링크만 보완하고 원 문서 bytes·SHA·source 경로를 별도 보존 |
| Native 입력 확인 | exact Python 입력 시험1개 FAIL_PROVENANCE_IDENTITY. 변경 없는 page-format 문서 pin의 기존 불일치이며 원 FAIL 보존. 이번 native manifest/runner pin 및41개 dependency는 정적 대조 PASS. native runtime NOT_RUN |
| 문서 검사 | 첫 push gate의 committed export에서 fmt·Clippy PASS, 문서 FAIL. 이동 코드의 탐색 링크1개·시험 파일 색인·export에 없는 로컬 로그 링크를 보완했다. 원 push 실패는 `artifacts/maintenance/naming-20261025/logs/push-1.log`에 보존. 후속 push gate 결과는 `push-2.log`로 확인 |

독립 검토 중 외부 package/CLI 문자열의 과잉 치환을 발견해 복원했다. 이 상태의 runner를 제품 시험으로 실행하지 않았으며 실제 검증은 복원 뒤 수행했다. 이름 충돌이 있던 두 `plan4_recovery_select.rs`의 서로 다른 owner 경로도 구분했다.

추가 native 입력 확인의 기존 불일치는 `devdoc/contracts/dev3-page-format.md`다. 입력은4845 bytes/`145acea1f16da02d6868d0cc38e05f696c3b1b18aed356e8842e11e4d01e5fab`를 요구하지만 착수 baseline과 현재 파일은 모두4870 bytes/`0d53e26ea03d97d684fc9d36035ff3da7db7eff832ef49f901655641e2f65fb0`다. 무관한 원 provenance를 갱신해 PASS로 바꾸지 않았다. 원 FAIL `artifacts/maintenance/naming-20261025/logs/native-input.log`과 변경 metadata만의 대조 `artifacts/maintenance/naming-20261025/logs/native-metadata.json`를 구분한다.

### 실행 명령과 기록

- `PYTHONPATH=scripts python3 -B -m unittest dev3.empty_transactions.test_borrows.EmptyHandoffCompiler.test_inventory_selection_and_source_binding`
- `PYTHONPATH=scripts python3 -B -m unittest test_dev3_plan4_clean_compiler.G5CompilerHarness.test_prior_target_matches_retained_source`
- 후속9개는 allocation metadata/inventory, Status flush/init 입력, mixed owner 입력, recovery prefix/effects/select 입력, G5 완료 이력의 각 exact method다. 선택 로그 `artifacts/maintenance/naming-20261025/logs/python-3.log`를 보존한다.
- `PYTHONPATH=devdoc python3 -B -m unittest test_contracts.PackageAliasChecks.test_aliases_preserve_package_dependency_checks`
- `PYTHONPATH=scripts python3 -B -m unittest test_dev3_status_home_flush_native.StatusFlushNativeChecks.test_selection_marker_and_frozen_input_identity`: 위 기존 provenance 불일치로 FAIL
- `python3 -B scripts/dev3/empty_transactions/check_borrows.py --case bank_release --case result_bank_alias --artifact-parent artifacts/maintenance/naming-20261025/compiler`

compiler 결과 위치 `artifacts/maintenance/naming-20261025/logs/compiler.log`, 원 compile 실패 `artifacts/maintenance/naming-20261025/logs/check-1.log`, 수정 후 compile `artifacts/maintenance/naming-20261025/logs/check-2.log`, Clippy `artifacts/maintenance/naming-20261025/logs/clippy.log`, 최초 Python 실패 `artifacts/maintenance/naming-20261025/logs/python-1.log`, 해당 실패 재검증 `artifacts/maintenance/naming-20261025/logs/python-2.log`, alias checker `artifacts/maintenance/naming-20261025/logs/package-checker.log`, CLI `artifacts/maintenance/naming-20261025/logs/controller-cli.log`, 정적 대응 집계 `artifacts/maintenance/naming-20261025/logs/audit.json`는 로컬 원자료다. artifacts는 Git 제외이며 원 실행 결과를 다른 source의 PASS로 승계하지 않는다.

첫 push gate에서 DEV4 supply handoff의 source 탐색 링크1개가 끊어졌다. 해당 문서의 현재 탐색 링크만 갱신하고 원 source 경로·commit·문서 SHA를 명시했으며 원 byte를 로컬 history에 보존했다. `documents.json`의 기존 `test_xadb_contracts.py` ID는 유지하고 실제 경로만 `test_contracts.py`로 바꿨다. ignored 로그의 경로는 clean export에 존재하는 링크로 주장하지 않고 code 표기로 기록한다.

## 파일 이동 대응

아래는 현재 코드 이동 대응이며 과거 증거의 경로를 다시 쓴 목록이 아니다. 새 Python package marker2개는 실제 작성 경로로 관리 JSON에 등록했다.

| 이전 저장소 상대 경로 | 현재 저장소 상대 경로 |
| --- | --- |
| `crates/engine/src/storage/wal_io/plan4_retention_tests.rs` | `crates/engine/src/storage/wal_io/plan4_retention/tests.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_s5_tests.rs` | `crates/engine/src/storage/wal_io/plan4_retention/s5_tests.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_resources.rs` | `crates/engine/src/storage/wal_io/plan4_retention/resources.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_recovery_test_observation.rs` | `crates/engine/src/storage/wal_io/plan4_retention/recovery_observation_tests.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_recovery.rs` | `crates/engine/src/storage/wal_io/plan4_retention/recovery.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_read.rs` | `crates/engine/src/storage/wal_io/plan4_retention/read.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_population.rs` | `crates/engine/src/storage/wal_io/plan4_retention/population.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_load.rs` | `crates/engine/src/storage/wal_io/plan4_retention/load.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_live.rs` | `crates/engine/src/storage/wal_io/plan4_retention/live.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_literal_tests.rs` | `crates/engine/src/storage/wal_io/plan4_retention/literal_tests.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_lifecycle_tests.rs` | `crates/engine/src/storage/wal_io/plan4_retention/lifecycle_tests.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_history.rs` | `crates/engine/src/storage/wal_io/plan4_retention/history.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_flush.rs` | `crates/engine/src/storage/wal_io/plan4_retention/flush.rs` |
| `crates/engine/src/storage/wal_io/plan4_retention_control.rs` | `crates/engine/src/storage/wal_io/plan4_retention/control.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_support.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/support.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_publication.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/publication.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_physical.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/physical.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_observations.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/observations.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_native.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/native.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_literals.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/literals.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_faults.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/faults.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_closure.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/closure.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests_cases.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests/cases.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_tests.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/tests.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_select.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/select.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_resources.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/resources.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_physical.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/physical.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_epoch.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/epoch.rs` |
| `crates/engine/src/storage/wal_io/plan4_recovery_analysis.rs` | `crates/engine/src/storage/wal_io/plan4_recovery/analysis.rs` |
| `crates/engine/src/storage/wal_io/plan4_pages_tests.rs` | `crates/engine/src/storage/wal_io/plan4_pages/tests.rs` |
| `crates/engine/src/storage/wal_io/plan4_pages_resources.rs` | `crates/engine/src/storage/wal_io/plan4_pages/resources.rs` |
| `crates/engine/src/storage/wal_io/plan4_pages_read.rs` | `crates/engine/src/storage/wal_io/plan4_pages/read.rs` |
| `crates/engine/src/storage/wal_io/plan4_pages_native_tests.rs` | `crates/engine/src/storage/wal_io/plan4_pages/native_tests.rs` |
| `crates/engine/src/storage/wal_io/plan4_pages_load.rs` | `crates/engine/src/storage/wal_io/plan4_pages/load.rs` |
| `crates/engine/src/storage/wal_io/empty_transactions_tests.rs` | `crates/engine/src/storage/wal_io/empty_transactions/tests.rs` |
| `crates/engine/src/storage/wal_io/empty_transactions_pressure_tests.rs` | `crates/engine/src/storage/wal_io/empty_transactions/pressure_tests.rs` |
| `crates/engine/src/storage/wal_io/empty_transactions_pressure.rs` | `crates/engine/src/storage/wal_io/empty_transactions/pressure.rs` |
| `crates/engine/src/storage/wal_io/empty_transactions_load_tests.rs` | `crates/engine/src/storage/wal_io/empty_transactions/load_tests.rs` |
| `crates/engine/src/storage/wal_io/empty_transactions_load.rs` | `crates/engine/src/storage/wal_io/empty_transactions/load.rs` |
| `crates/engine/src/storage/wal_io/empty_transactions_live_tests.rs` | `crates/engine/src/storage/wal_io/empty_transactions/live_tests.rs` |
| `crates/engine/src/storage/wal_io/empty_transactions_live.rs` | `crates/engine/src/storage/wal_io/empty_transactions/live.rs` |
| `crates/engine/src/storage/wal_io/empty_transactions_history.rs` | `crates/engine/src/storage/wal_io/empty_transactions/history.rs` |
| `crates/engine/src/storage/wal_io/status_home_flush_tests.rs` | `crates/engine/src/storage/wal_io/status_home_flush/tests.rs` |
| `crates/engine/src/storage/wal_io/status_home_flush_resource_tests.rs` | `crates/engine/src/storage/wal_io/status_home_flush/resource_tests.rs` |
| `crates/engine/src/storage/wal_io/status_home_flush_race_tests.rs` | `crates/engine/src/storage/wal_io/status_home_flush/race_tests.rs` |
| `crates/engine/src/storage/wal_io/status_home_flush_native_tests.rs` | `crates/engine/src/storage/wal_io/status_home_flush/native_tests.rs` |
| `crates/engine/src/storage/wal_io/status_home_flush_home_tests.rs` | `crates/engine/src/storage/wal_io/status_home_flush/home_tests.rs` |
| `crates/engine/src/storage/wal_io/status_home_flush_authority_tests.rs` | `crates/engine/src/storage/wal_io/status_home_flush/authority_tests.rs` |
| `crates/testing/engine-worker/src/bin/xadbtest-durable-worker.rs` | `crates/testing/engine-worker/src/bin/durable.rs` |
| `crates/testing/engine-worker/src/bin/xadbtest-durable-performance-worker.rs` | `crates/testing/engine-worker/src/bin/durable_performance.rs` |
| `crates/testing/controller/src/controller.rs` | `crates/testing/controller/src/runner.rs` |
| `devdoc/test_xadb_contracts.py` | `devdoc/test_contracts.py` |
| `scripts/check_dev3_empty_transactions_borrows.py` | `scripts/dev3/empty_transactions/check_borrows.py` |
| `scripts/test_check_dev3_empty_transactions_borrows.py` | `scripts/dev3/empty_transactions/test_borrows.py` |

## 독립 검토

별도 검토자 `/root/naming_review`는 최종 diff·현재 source·원 실패와 선택 실행 로그를 읽기 전용으로 확인하여 D1/D2 수용했다. 별칭·모듈 경로·외부 실행 명령·과거 receipt source 결합·직접 스크립트 실행·dependency checker·과거 pin과 현재 경로 분리·native manifest identity를 확인했고 필수 수정 지적은 남아 있지 않다. native 입력의 기존 provenance 불일치도 baseline과 독립 대조했으며 인수 PASS로 해석하지 않는다. 검토자가 파일 수정이나 시험 실행을 하지 않았으며 수용 범위는 이번 명칭·배치 변경이다. 다른 작업의 문서 commit과 DEV2 I/O 변경은 이번 변경 커밋에 포함하지 않는다. 첫 push gate 뒤 문서 ID 유지·로컬 로그 경로 표기·DEV4 handoff의 원 identity를 보존하는 탐색 링크 보완도 같은 검토자가 수용했다. 코드 추가 변경이나 제품 시험 재실행은 없었다.

# XADB 모듈 분리 후 안정화·경계 개선 실행 계획

2026-11-02 · **계획 D2 ACCEPT / F0 수정·이관·선택 검증 완료 / F1 별도 구현·검증 기록 / F2 구현·선택 검증·독립 검토 완료 / F5-a 구현·선택 검증·독립 검토 완료 / F3 구현·선택 검증·독립 검토 완료 / F4 PR #31·#32 병합 / F5-b PR #33 병합 / F6-a 정적 조사·측정 계획 작성(D03 사용자 결정, 측정 NOT_RUN) / F6 측정은 live Row retirement 공급 뒤로 보류(사용자 결정)·F6-b 미완료 / F7 `disk.rs` 현재 주석 정정**

모듈 분리 후속 개발을 위한 개선 순서와 완료 조건을 정한다. 각 단위에 현재 코드·계약의 근거와 비교할 대안을 연결한다. 계획 채택 때 제품·검사기·시험·측정은 시작하지 않았다. 후속 사용자 지시로 [F0 기존 증거 분류](../evidence/main-preexisting-failures-20261030.md#f0-existing-evidence)를 기록했고, 다시 F0 전체 완료 지시에 따라 [수정·이관·선택 검증](../evidence/main-preexisting-failures-20261030.md#f0-completion)을 마쳤다. 과거 원 로그 미확보의 attribution 한계는 보존하며 현재 선택78개의 PASS와 구분한다. F1은 별도로 구현·검증하며 [실행 기록](../evidence/module-independence-survey-20261026.md#f1-python-gate)에 현재 상태를 둔다. 이후 개발 재개 지시가 오면 해당 단위의 기존 위임과 D1/D2 절차 안에서 진행하며 일상 구현마다 재승인을 요구하지 않는다.

[기존 분리 계획](module-independence-plan.md) · [소유 경계](../contracts/dev3-module-boundaries.md) · [소비자 계약](../contracts/dev3-module-consumers.md) · [모듈 gate](../policies/repository-build-plan.md#module-gate) · [개발 방침](../policies/development-policy.md#section-2)

<a id="baseline"></a>
## 1. 기준선과 완료 범위

- 계획 작성 기준은 PR #22가 병합된 `main` `9dc9bf7def25e1bf6bd89f9425f543b789475a7c`다. 제품 코드는 리뷰 기준 `76ec2cf5`와 같고, #22의 마지막 commit `e64ca616`은 보정 실행의 미검증 범위와 기존 실패 분류를 보완했다. 이미 보완된 문구를 신규 미해결 항목으로 다시 만들지 않는다.
- 기존 M5는 선정한 INSERT(A)·읽기(B)의 독립 구현과 합류에 대한 한정 완료다. [MI03 한계](../evidence/module-mi03-limits-20261101.md)의 PASS 108·PARTIAL 1(MCB03-V01)·N/A 20을 유지한다. 이 계획은 M5를 다시 열거나 전체 저장 엔진·DEV3·제품 인수를 완료로 바꾸지 않는다.
- 원격 gate는 #22의 [적용 기록](../evidence/module-independence-survey-20261026.md#s12-remote-rollout)을 따른다. 동일 이름 상태·검토자 문자열·실행 receipt의 신뢰 한계는 기존 수용 범위를 유지한다. 이번 계획이 GitHub App 도입이나 보호 설정 변경을 요구하지 않는다.
- [전체 lib 보정 기록](../evidence/module-independence-survey-20261026.md#s12-full-lib-calibration)은 정확한 사례 1,514개 중 1,488 통과·26 실패, macro 모듈 선택 7개 중 2개 실패다. resident의 `cut!` 8개는 abort 1개·미시작 7개로 미검증이다. 26개 중 이관 전 시험 6개, 같은 Reserve 차이가 기록된 `retention_live_lifecycle` 1개, 나머지 미분류 19개를 구별한다. 두 모듈 선택도 원인 분류가 남는다.
- 리뷰의 직접 확인 범위는 계약·주요 코드 경로·Git 이력·정적 의존 재집계다. 당시 gate의 등록 규칙 위반은 0이었으나 이는 C 내부의 독립성이나 제품 동작 PASS를 뜻하지 않는다. 외부 실행 원본 로그 전체를 독립 검산하거나 제품 시험을 다시 실행하지 않았다.

### 구조의 정량 기준선

위 `main` commit의 추적 파일 중 `crates/engine/src/**/*.rs`를 [검사기](../../scripts/check_module_gate.py)의 `owner()`가 [gate manifest](../policies/module-gate.json)를 적용한 결과로 집계했다. 줄 수는 시험·주석·빈 줄을 포함한 물리 줄 수다. `crates/engine/` 전체의 별도 test target 등은 분모에 포함하지 않는다.

| 범위 | 파일 수 | 줄 수 | 전체 src 줄 수 대비 |
| --- | ---: | ---: | ---: |
| engine src 전체 | 540 | 300,086 | 100% |
| 공통 C | 312 | 178,267 | 59.41% |
| INSERT A | 27 | 21,375 | 7.12% |
| 읽기 B | 16 | 10,450 | 3.48% |
| Catalog CAT | 2 | 115 | 0.04% |

나머지는 disk·codec 등 다른 owner다. 허용 예외는 production 4개(EX01–EX04)·test 5개(EX05–EX09)다. 이 수치는 배치와 조사 범위의 기준선이며 독립 개발 가능 비율이 아니다. 특히 C는 잔여 `storage/**`를 포괄하므로 단일 책임으로 설계됐다고 볼 수 없다. 개선 후에도 줄 수 감소를 목표로 삼지 않는다.

<a id="invariants"></a>
## 2. 유지할 설계와 변경 제한

`DiskSession`의 원 상태 소유, `ReadPort`·`MutationPort`·`MaintenancePort`의 제한된 대여, consuming reservation/prepared/receipt, WAL-before-page, Commit/ACK·UNKNOWN 의미, 실패 시 원 소유 객체 반환과 fence, pin·lease·source admission의 단일 소유를 유지한다. 상대 기능을 물리적으로 제거한 C+A/C+B export와 독립 oracle도 유지한다.

이 계획만으로 다음을 시행하지 않는다.

- 전체 저장 모듈의 일괄 재작성·crate 분할, raw constructor/필드 공개 확대, 공통 가변 context 추가.
- 초기화 조상의 일괄 이동, 무조건적인 lifetime 병합이나 `Box`·`Arc`·trait object 도입.
- 기존 FAIL·PARTIAL·N/A·미검증의 PASS 승격, 필수 시험의 무근거 제외, 원 fixture·증거 덮어쓰기.
- 전체 workspace/lib 재실행, 무변경 하위 회귀 반복, 원격 ruleset/Actions 정책 변경.

새 기능의 실제 요구와 유한한 변경 범위가 확인된 부분만 설계한다. 공통 C는 편의상 남은 파일 전체를 뜻하는 분류에서 점진적으로 책임이 명확한 소유 단위로 바꾼다. 모듈 이름·수·코드 줄 수는 완료 지표로 사용하지 않는다.

<a id="sequence"></a>
## 3. 실행 순서와 인계

| 단위 | 우선순위·선행 | 산출물 | CI 차단과의 관계 | 완료 판정 |
| --- | --- | --- | --- | --- |
| F0 기존 실패·미검증 정리 | 최우선. 기존 로그·source 조사부터 | 사례별 분류·영향 경로·담당·처리 상태 | 기존 실패 14개 파일과 resident 미검증 8개를 추적 | 다음 작업에 해당하는 실패가 수정·검증되거나 확정된 처리 방식이 적용됨. 보류는 차단 해소로 세지 않음 |
| F1 gate 자체 변경 검증 | 최우선. F0 전체 종료를 기다리지 않음 | Python 변경 시험 계획·비권한 실행·결과 확인 | Python-only 변경은 Rust 실패 파일과 독립 | 새 검사기 결함이 해당 변경에서 실패하고, 무관한 제품 시험은 실행하지 않음 |
| F2 공통 변경의 계약 시험 선택 | F1 결과 형식 확정·§4의 결정 D02 적용 | 변경→계약→최소 사례 대응과 export별 선택 | 제어 구현과 제품 적용을 분리. 제품 적용의 실제 선택 결과를 F0와 대조 | 선택 누락·중복·잘못된 source 결속을 거절하며 기존 변경 범위 정책 준수 |
| F3 Catalog 역방향 의존·C 소유 경계 | F0의 해당 경로 처리 및 F2 검증 경로 확보 | 읽기 호출 방향 정리, 좁은 C 책임 하나의 소유표 | 현재 4개 역참조 파일 자체는 실패 14개 파일에 없음. 함께 바꾸는 시험·selector는 별도 대조 | EX01–EX04 해소·원 권한/자원 보존·선택 인수 |
| F4 작업 API·자원 회계 단순화 | 실제 소비자 하나 선정. F3와 같은 파일이면 직렬 | 작업 handle/관련 타입 후보와 독립 예산 계약 | `plan4_apply/row/census_tests.rs` 수정 시 기존 `s2w_c04` 실패 처리 필요 | 소비자 수정 범위 감소, 소유·오류·상한 유지, EX05–EX07 처리 기록 |
| F5 시험 소유·선택 식별 정리 | F2 대응 형식 확정 | 안정 사례 ID→실행 경로, export 제거 정보의 단일 관리 | `plan4_recovery/tests/{cases,closure,faults,physical,publication,free_undo}.rs`·`plan4_pages/tests.rs`를 바꾸면 해당 기존 실패 처리 필요 | 동일 사례 집합·발견·제외 이유와 단독 export 보존, EX08–EX09 처리 기록 |
| F6 WAL 재검증 비용 조사·조건부 개선 | 설계 조사·계측 범위는 조기에 정함. 실행은 해당 F0 처리·결정 D03 적용 뒤 | 유한 측정 계획·결과, 필요할 때 증분 검증 D2 | `plan4_retention/{lifecycle_tests,free_undo_tests}.rs` 변경·선택 시 해당 기존 실패 처리 필요 | 병목 판단 또는 동등 정확성·자원·복구를 만족하는 개선 |
| F7 현재 계약·진입 문서 정리 | 각 단위에 동반. 전면 문서 개편은 하지 않음 | 현재 상태/계약과 역사 증거의 분리 | Markdown-only 변경에는 Rust 시험 없음. `.rs` 주석 변경은 실제 selector 확인 | 작업 진입점과 현재 코드 설명 일치, 과거 identity 유지 |

표의 짧은 Rust 경로는 `crates/engine/src/storage/wal_io/` 아래다. 실패와의 관계는 예상 접점이며 인접 파일을 바꾼다는 이유만으로 실패 시험이 선택된다고 단정하지 않는다. 실제 diff에 대한 gate의 exact case·macro selector와 F2의 계약 선택을 대조해 인계한다. 위에서 직접 배정하지 않은 `mixed_system_owner/tests.rs`, `resident_status/tests.rs`, `recovery_effect/tests.rs`, `status_home_flush/tests.rs`도 F0 추적 대상이다. F5가 나열한 파일 전체를 반드시 이동한다는 뜻은 아니다.

첫 구현 단위는 F1로 잡고 F0의 기존 증거 분류를 함께 진행한다. F0가 끝나기 전에는 영향을 모르는 제품 수정·측정으로 확대하지 않는다. F6의 병목 가설과 계측 설계는 F3–F5 완료를 기다릴 이유가 없지만, 측정·제품 최적화는 별도 단위다. 이 표는 병렬 agent 실행을 지시하지 않는다. 실제 동시 작업은 소유 파일·계약 의존을 확인한 뒤 기존 병렬 정책을 따른다.

각 단위의 착수 입력은 기준 commit, 실제 diff, 소유/소비자, 현 계약과 바꿀 계약, 선택 사례, source별 기존 결과, 시간·disk 예산, 중단 조건이다. 담당·독립 검토자·통합 담당은 실제 착수 때 식별한다. 미배정 역할을 이미 수행한 검토자로 기록하지 않는다.

<a id="failures"></a>
## 4. F0 — 기존 실패와 미검증을 개발 가능한 상태로 정리

**진행:** [F0 완료 증거](../evidence/main-preexisting-failures-20261030.md#f0-completion)에 원인별 수정, D01 current-format 이관, 고유78 tests PASS와 원 FAIL/ABORT/NOT_STARTED 보존을 기록했다. 복구6함수의 세부45개와 resident cut8개를 모두 완료했다. 과거 원 로그의 미확인 이름을 소급 확정하지 않았으며 PR CI·main 병합은 별도다.

**입력:** [보정의 실패 표와 후속 요구](../evidence/module-independence-survey-20261026.md#s12-full-lib-calibration), [기존 실패 조사](../evidence/main-preexisting-failures-20261030.md), 당시 source와 확보 가능한 원 로그다. 먼저 보존 자료를 읽고 원인별로 묶는다. 같은 panic 위치만으로 같은 원인이라고 판정하지 않는다.

각 사례에 `case ID/selector, source, 최초 실패, 현재 계약, 분류, 근거, 영향 경로/차단 작업, 담당, 처리 결정, 다음 조치, 완료 조건`을 대응한다. 사례별 결과의 정본은 기존 증거 문서에 후속 기록으로 두고 이 계획에 복제하지 않는다. 원본을 확보하지 못하면 해당 판단을 미확인으로 남긴다.

1. 미분류 19개와 두 macro 모듈 선택에서 제품 불변식·정리/Drop·자원 상한 문제가 있는지 우선 분류한다. resident abort와 뒤의 7개 미시작을 구별한다.
2. `retention_live_lifecycle`의 알려진 Reserve 차이에 대해 후속 실행 경로까지 원인이 확인됐는지 대조한다. 알려진 첫 차이를 전체 시나리오의 원인으로 확대하지 않는다.
3. format1 이관 전 6개는 2026-10-30의 보존 결정과 최신 보정 기록의 처리 재결정 요구를 함께 기록한다. 후속 F0 전체 완료 지시에 따라 아래 D01의 current-format 이관을 적용했다. current-format 이관안은 새 fixture·독립 oracle·사례 대응표로 준비하며 기대값만 맞추거나 필수 시험을 일괄 제외하지 않는다.
4. 제품 결함·시험 결함·환경 결함의 수정은 원인별 작은 diff로 분리한다. 수정 코드와 원 실패에 해당하는 선택 검증만 수행한다. 변경 없는 원인 조사에 새로운 실행이 꼭 필요하면 입력·관측 질문·유한 범위와 기존 정책 적용 여부를 먼저 정한다. 과거 PF04/COMM의 한정 예외를 일반화하지 않는다.

5. resident `cut!` 8개는 후속 작업의 명시적 산출물로 둔다. allocator abort의 원인을 진단·수정하고 그 사례 및 뒤의 미시작 7개(`dpt_checked`, `dpt_owned`, `metadata`, `payload`, `publication`, `registry`, `stage`)에 도달·완료하는 선택 검증을 수행한다. 가능한 경우 8개를 개별 선택하고, 현재 macro selector가 더 넓으면 선택 기능부터 좁힌다. 이미 실행한 exact 23개와 ignored `s5!` 6개를 자동 재실행·활성화하지 않는다. 코드 수정 전 진단 실행이 필요하면 4항의 적용 근거와 범위를 먼저 정한다. 실행 결과는 당시 source에 붙이며 새 FAIL·abort도 그대로 기록한다.

F0는 **분류 완료**, **수정·검증 완료**, **사용자 처리 결정**, **보류·차단 유지**를 분리한다. 분류와 처리 결정의 기록만으로 CI를 PASS로 만들지 않는다. 실제 차단 해제는 해당 사례가 통과하거나, 유효한 결정에 따른 처리가 적용·검증되어 필수 gate를 충족할 때 확인한다. 보류를 선택하면 영향 작업도 보류한 채 무관한 작업을 진행한다. 정상 동작을 검증하지 못한 경로의 새 기능 인수는 완료로 기록하지 않는다.

<a id="user-decisions"></a>
### 착수 전에 적용할 결정

아래 결정은 후속 구현·진단·측정의 착수 입력이다. 기존 지시의 적용 여부를 먼저 확인하고, 미결정 사항은 구체적인 대상·대안·검증안을 준비한 뒤 확정한다. 유효한 기존 권한을 재사용하며 일상 실행마다 허가를 반복 요청하지 않는다. 원 지시와 충돌하는 범위만 결정을 기다린다.

| ID·대상 | 준비할 대안·권고 | 결정 전 경계 |
| --- | --- | --- |
| D01 이관 전 6개 시험 | 후속 “F0를 끝까지 진행” 지시에 따라 원 증거를 보존한 current-format 이관을 적용. 독립 fixture/base4 oracle·사례 대응표와 memory45개 PASS는 [F0 완료](../evidence/main-preexisting-failures-20261030.md#f0-completion) | 이관·선택 검증 완료. 과거 FAIL과 ignored native/static의 별도 범위 유지 |
| D02 공통 C 변경을 직접 검증하는 A/B 사례 | 실제 C diff→호출 경로→계약→최소 사례를 제시해 변경 코드 시험 범위임을 확인. 이 범위로 해결 가능하면 기존 권한으로 진행. 사용자 규칙의 해석이 미정이거나 무변경 소비자 회귀까지 필요하면 필요한 범위만 사용자에게 확인 | 전이 의존 전체 재실행을 허용하지 않음. 미실행은 NOT_RUN으로 남기고 coverage 완료로 세지 않음 |
| D03 WAL 진단·성능 측정 (2026-11-02 사용자 결정: 정적 조사·측정 계획까지만, 측정은 live Row retirement 공급 뒤로 보류. [F6-a 기록](../evidence/f6-wal-revalidation-survey-20261102.md)) | F6-a의 입력·관측 질문·반복·시간/disk 예산·중단 조건을 구체화. 기존 지시로 허용된 범위면 재사용하고, 무변경 baseline 측정 등 적용 권한이 없으면 유한 측정 범위를 사용자에게 확인 | 정적 조사·계측 설계까지 진행. 미허용 측정을 시작하거나 PF04/COMM의 한정 지시를 전용하지 않음 |

<a id="gate-tests"></a>
## 5. F1 — gate·runner 변경을 해당 PR에서 검증

**진행:** F1 구현·선택 검증과 독립 리뷰는 [실행 기록](../evidence/module-independence-survey-20261026.md#f1-python-gate), 결과 계약과 도입 순서는 [build 계획](../policies/repository-build-plan.md#module-gate-python)을 따른다. F0는 별도 후속 완료 기록을 따르며 F2–F7은 이 F1 작업 범위 밖이다.

**착수 당시 근거:** [`RUST_INPUTS`와 `selection`](../../scripts/check_module_gate.py), [build runner](../../scripts/module_gate_build.py), [build workflow](../../.github/workflows/module-build.yml), [기존 시험](../../scripts/test_check_module_gate.py). Python만 바뀌면 Rust 선택은 비고, 신뢰 판정은 기본 branch의 코드를 사용하므로 head의 새 Python 동작을 자동 검증하지 않는다.

**작업:** 변경한 checker·runner·export·push helper에 대응하는 Python 시험을 선언하고, PR head의 후보 코드를 토큰 쓰기 권한·secret 없는 job에서 실행한다. 신뢰 제어 workflow는 계속 기본 branch 코드를 사용하며 후보 코드를 import하거나 실행하지 않는다. 실행 결과는 변경 범위·head/source·시험 목록·runner 버전에 결속하여 확인한다. 이 작업은 후보 Python의 기능 회귀 검출이며 기존 receipt의 악의적 변조 한계를 해결했다고 주장하지 않는다.

**검증:** 관련 변경에서 새 후보의 syntax/runtime 오류, 잘못된 의존 판정, 잘못된 선택, 누락·불일치 결과가 성공으로 처리되지 않는지 필요한 부정 사례를 고정한다. docs-only 변경은 이 시험을 실행하지 않고, Python-only 변경은 무관한 제품 시험을 실행하지 않는다. no-test/skip/실패/중단을 성공으로 바꾸지 않는다.

**시행:** D2로 planner·runner·workflow와 결과 계약을 함께 검토한다. 새 결과를 요구하는 제어와 이를 내는 runner의 도입 순서를 정하고, 채택 PR을 영원히 대기시키거나 이전 결과로 새 의무를 충족시키지 않게 한다. 원격 보호 설정은 변경하지 않는다. 현재 신뢰/실행 경계를 합치는 안, 수동 리뷰 기록만으로 후보 시험을 대신하는 안은 채택하지 않는다.

<a id="affected-tests"></a>
## 6. F2 — 공통 변경과 최소 계약 사례 연결

**진행:** [F2 실행 기록](../evidence/module-independence-survey-20261026.md#f2-contract-selection)과 [계약 선택 규칙](../policies/repository-build-plan.md#module-gate-contracts)을 따른다. ReservedRow 실패 효과 세 사례의 C+A 대응과 Python 제어 검증이며 이번 단위의 제품 코드·fixture 변경과 제품 시험은 없다.

**착수 당시 근거:** [선택기](../../scripts/check_module_gate.py)의 `changed_cases/selection/coverage_findings`와 [gate manifest](../policies/module-gate.json). 공통 C·codec·semantic 변경은 소비자 A/B의 시험을 자동 선택하지 않는다. record selector는 head에만 추가되므로 상대 기능 부재 조건에서만 드러나는 실패의 요청 경로도 제한된다.

**작업:** 첫 적용은 `ReservedRow`의 prepare/apply/cancel 실패 효과처럼 실제 공통 변경 하나로 제한한다. `변경 경로/의미 → 영향 계약 → 검증 사례 ID → head 또는 C+A/C+B → 기존 실행 재사용 가능 조건`을 등록한다. production 호출과 시험 파일 위치를 구별하고, 해당 사례의 입력·oracle·harness 변경도 영향 판정에 넣는다. 전체 전이 의존을 재귀적으로 시험하는 체계는 만들지 않는다.

**정책 경계:** [AGENTS.md](../../AGENTS.md)의 무변경 코드·완료 시험 반복 금지가 우선한다. 이 계획은 그 규칙을 새 해석으로 대체하지 않는다. 변경된 공통 구현을 직접 검증하는 사례인지 근거를 제시하고 [D02](#user-decisions)를 적용한다. 사례가 A/B 파일에 있다는 이유만으로 무관하다고 보거나 모든 소비자 시험을 허용하지 않는다. 기존 규칙과 충돌하는 소비자 재실행이 필요하면 정책 변경 범위를 먼저 명시하고 확정 전 시행하지 않는다. 현재 정책 안에서 실행할 수 없는 경우 영향과 `NOT_RUN` 이유를 표시하고, 실행되지 않은 사례로 coverage를 채우지 않는다.

**완료 조건:** fixture가 다른 export로 잘못 연결되거나, 필수 사례가 0개 발견되거나, 동일 source의 동일 사례를 중복 실행하거나, 영향에 해당하는 선택이 누락된 경우를 검출한다. export별 요청을 지원할 때도 제거된 상대 기능을 다시 넣지 않는다. Python 제어 검증은 F1을 사용하며 제품 시험은 변경된 계약 경로에 한정한다.

<a id="catalog"></a>
## 7. F3 — Catalog 호출 방향과 C 내부 소유

**실행 상태:** F3의 읽기 호출 방향·C 소유 pilot을 구현하고 변경 경로의 선택 검증과 독립 D1/D2 검토를 마쳤다. [실행 기록](../evidence/module-independence-survey-20261026.md#f3-catalog-call)을 따른다. F5-a 시작 당시 약30GiB 때문에 미실행했던 [환경 근거](../../module-gate/evidence/f5/f3-executor-limit.json)는 보존한다. 이번 실행의 여유 공간은 약132GiB이며 native의 고정60GiB floor를 낮추지 않는다.

**변경 전 근거:** [`catalog/read.rs`](../../crates/engine/src/storage/catalog/read.rs)는 disk API를 소비하지만 C에서도 Catalog 제품을 참조한다. manifest EX01–EX04의 대상은 다음 4개 파일이다. Catalog CREATE도 현재 `catalog/`의 별도 제품 본문으로 완성된 상태가 아니다.

| 예외 | 현재 참조 | 해소 시 확인 |
| --- | --- | --- |
| EX01 | [`read.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/read.rs)의 `CatalogLookupStep` 재노출 | C의 제품 타입 재노출을 제거하고 소비자 타입 경계를 갱신 |
| EX02 | [`read/lookup.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/read/lookup.rs)의 제품 타입·lookup·`source_bytes` | 원 scope가 권한을 유지하며 제품 호출·크기 조회를 공통 계약으로 분리 |
| EX03 | [`read/scope.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/read/scope.rs)의 lookup 호출 | Catalog가 checked operation을 소비하도록 호출 방향 전환 |
| EX04 | [`read/statement.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/read/statement.rs)의 lookup 호출 | statement 읽기의 호출 방향과 결과 조립 소유를 함께 정리 |

첫 단위는 읽기만 대상으로 한다. 원 scope가 admission과 결과 소비 수명을 계속 소유하고, Catalog 제품 쪽이 checked operation을 소비하여 검색·가시성 선택·결과를 조립하도록 signature와 caller 이동표를 설계한다. `finish_table`의 유일 발급 근거, read-set 등록 시점, cleanup 오류 우선순위, 반환된 view 수명은 유지한다. 새 raw factory나 검증 생략 callback을 추가하지 않는다. 같은 검산을 두 계층에 복제하지 않는다.

관련 현재 소비자를 모두 이전한 뒤 EX01–EX04를 경로별로 제거하고, 실제 정적 참조와 생산/시험 컴파일 및 변경된 읽기 인수로 확인한다. EX01의 `review_when`인 “Catalog 읽기가 `plan4_retention::read`에서 호출되는 대신 `ReadPort`를 소비할 때”를 재검토 기점으로 삼되, 조건 문구를 바꾸는 것만으로 완료하지 않는다. 새 역참조·raw 권한 노출이 없고 4개 참조와 예외가 함께 해소되어야 한다. `source_bytes`도 제품 본문을 호출하지 않는 고정 예산 계약으로 정리할 수 있는지 검토한다. CREATE는 실제 다음 기능에서 제품 순서와 설치 권한의 소유를 별도 결정한다. 읽기 단위에 전체 DDL 재설계를 묶지 않는다.

C 내부는 실제 변경하는 책임 하나부터 `소유 상태, 허용 소비, private 생성, 하위 효과, 복구/자원 계약`을 표로 정리하고 manifest에 반영한다. 첫 pilot은 위 Catalog statement 읽기와 checked table 발급 경로의 소유 정리로 한정한다. 변경 전후 실제 수정한 책임·소비자 signature·재개방한 공통 계약·다른 기능 파일·선택 사례와 그 이유를 기록해 변경 전파가 줄었는지 판단한다. 독립된 비교 없이 개발 속도 향상률을 주장하지 않는다. 거래·적용 조립, 페이지/버퍼 효과, 복구 조립, 자원 admission은 이후 실제 수요에 따라 검토한다. 기존 초기화 조상·seal은 유지한다. 잔여 C를 새 이름 여러 개로 나누거나 빈 모듈을 만드는 것은 완료가 아니다.

<a id="operation-api"></a>
## 8. F4 — 작업 API와 자원 회계의 변경 전파 줄이기

**2026-11-02 후속 단위:** PR #31 병합 `e582363c`에서 [caller 고정 예산 계약](../contracts/dev3-module-boundaries.md#f4-caller-budgets)을 구현한다. A 실제 inventory와 C 합계를 독립 검증하여 EX05–EX07을 제거한다. 원 full slack·charge·short cap과 2행×1값 caller 범위를 보존하며 이번 source의 결과는 [실행 기록](../evidence/module-independence-survey-20261026.md#f4-caller-budgets)에서 확인한다. 첫 GAT의 구체 수명은 유지하지만 세 adapter의 관련 상태와 세 census의 역참조 개선으로 이 F4 범위를 닫는다. 채택만으로 시험 PASS를 뜻하지 않는다.

**2026-11-02 첫 구현 단위:** [정확한 CI 선택 선행 작업](../policies/repository-build-plan.md#module-gate-scoped-tests)이 PR #30으로 병합된 `5a904b09`에서 Row 쓰기 관련 상태 계약을 구현한다. [계약](../contracts/dev3-module-boundaries.md#f4-row-operation)과 [실행 범위·리뷰 기록](../evidence/module-independence-survey-20261026.md#f4-row-operation)을 따른다. A의 세 adapter에서 반복하던 13개 lifetime 전달을 C의 sealed 상태 계약으로 모으고 기존 권한·오류·Drop을 유지한다. CI source별 결과가 인수 근거이며 이 단위만으로 전체 F4·EX05–07 해소를 선언하지 않는다.

**현재 근거:** [`PreflightRow`](../../crates/engine/src/storage/wal_io/plan4_apply/row/write.rs)는 13개 lifetime과 namespace 타입을 노출하고 [`A live adapter`](../../crates/engine/src/storage/row/insert/driver/live.rs)가 이를 전달한다. C의 caller byte 예산과 일부 census 시험은 A 구현 크기를 알아야 한다.

첫 후보는 Row preflight→reserve→prepare/apply/cancel 한 흐름이다. 다음 대안을 원 권한과 자원 회계 위에서 비교한다.

| 대안 | 판단 기준 |
| --- | --- |
| 현 타입 유지 | 실제 변경 전파가 작으면 유지 가능. 단순 rename·별칭만으로 독립성이 좋아졌다고 세지 않음 |
| 제한된 작업 handle·관련 타입 | 내부 borrow 관계를 소비자 signature에서 감출 수 있는지, 원 owned error·재시도·cancel을 보존하는지 확인 |
| heap/동적 dispatch | allocation·자원 상한·호출 비용·삭제 비용까지 검토. lifetime 수만 줄이려고 채택하지 않음 |

A와 C는 각자 고정된 예산을 검증하고 조립 담당이 공존하는 phase의 합을 검증한다. 기존 census는 source-bound inventory이며 실제 stack/RSS 측정과 동일시하지 않는다. 상한을 맞추기 위해 누락 항목·임의 여유 축소를 도입하지 않는다. 내부 수명 차이를 무조건 합치지 않는다.

| 예외 | A 크기에 결합된 현재 시험 | 처리 기준 |
| --- | --- | --- |
| EX05 | [`plan4_apply/row/rollback_census_tests.rs`](../../crates/engine/src/storage/wal_io/plan4_apply/row/rollback_census_tests.rs) | rollback census가 A frame 크기를 직접 알아야 하는지 독립 예산·합성 검증으로 대조 |
| EX06 | [`plan4_recovery/row_action_census_tests.rs`](../../crates/engine/src/storage/wal_io/plan4_recovery/row_action_census_tests.rs) | recovery phase의 공존 항목·상한을 보존하며 EX05와 함께 재검토 |
| EX07 | [`plan4_retention/catalog_commit/row_census.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_commit/row_census.rs) | commit phase의 공존 항목·상한을 보존하며 EX05와 함께 재검토 |

예산 계약으로 직접 A 참조를 대체할 수 있으면 해당 예외를 제거한다. 합성 검증에 구현 크기가 여전히 필요하면 D2 근거·담당·대체안·재검토 조건을 남겨 유지한다. 첫 preflight 단위가 rollback/recovery/commit에 닿지 않으면 그 단위만 완료하고 EX05–EX07은 후속 잔여로 표시한다. 좁은 구현 완료를 세 예외의 해소 완료로 확대하지 않는다.

완료는 타입 수 감소가 아니라 소비자 수정 범위 감소와 동작 보존으로 판단한다. 필요한 compiler 거절 사례(다른 owner/view 혼합, 완료 후 재사용, 잘못된 수명)와 실패 반환·취소·Drop fence·자원 상한의 변경된 경로를 확인한다. F3와 공유 파일을 바꾸면 하나씩 통합한다.

<a id="test-ownership"></a>
## 9. F5 — 시험 소유와 안정 식별

**2026-11-02 단계적 시행:** [F5-a 기록](../evidence/module-independence-survey-20261026.md#f5-test-ownership)의 시험 소유/안정 ID·export 단일 등록을 먼저 구현한다. F2 세 사례의 owner 검증과 A/B/JOIN 및 EX05–EX09의 제거 정보 정본을 합치며 물리 시험·fixture는 이동하지 않는다. 전체 F5 완료가 아니다.

**2026-11-02 F5-b 단위:** PR #32 병합 `c0f9e25`에서 scope3 literal 81개를 bytes·git tree 그대로 C 쪽 `module-row-format/scope3/`로 옮기고 EX08–EX09를 제거한다. 생성기·SHA 고정 author는 무변경이며 원 경로 대응을 기록한다. [계약](../contracts/dev3-module-boundaries.md#f5-scope3-fixture)과 [실행 기록](../evidence/module-independence-survey-20261026.md#f5-scope3-fixture)을 따른다. trusted policy 전환 전이라 C+B 실행은 로컬 증거이며 CI는 head·C+A를 실행한다.

F5-b는 scope3 fixture 공통 소유 이관과 EX08–EX09 해소다. 현재 신뢰 gate가 이전 A fixture 경로 삭제를 A 전체·JOIN 시험 선택으로 확대하는 [정적 선택 결과](../../module-gate/evidence/f5/fixture-move-selection.json)를 확인했다. 좁은 변경 시험 원칙을 지키기 위한 [경로별 선택 도구](../policies/repository-build-plan.md#module-gate-scoped-tests)를 별도 PR로 먼저 채택한 뒤, bytes·독립 저작 provenance·원 hash를 보존한 실제 이동과 C+A/C+B 적용성을 확인한다. 위 F5-b 단위 전까지 EX08–EX09는 담당 C/A, 현 예외 근거와 `review_when`을 유지한 **미해소**였다. 복사본 생성이나 기존 증거의 경로 변경으로 대신하지 않는다.

**F5-a 이전 기준선:** A/B의 실제 연결 시험이 C의 `plan4_recovery::tests::catalog_memory` 아래에 있고, [export 도구](../../scripts/module_single_source.py)는 별도 제거 목록을 갖고 있었다. F5-a는 제거 정보를 소유 manifest에서 유도하도록 바꾸며 물리 시험 배치는 유지한다. [`plan4_pages.rs`](../../crates/engine/src/storage/wal_io/plan4_pages.rs)는 selector 보존을 위해 같은 소스를 production `apply`·test `retention`으로 연다.

먼저 F2에서 사용한 소수 사례에 안정 ID→현재 libtest 이름→소유/입력→export 적용성 대응을 만든다. 제품 계약 시험은 A/B 소유, 내부 권한 probe는 C 내부, 합류 시험은 JOIN으로 구별하되 이 단계에서 모두 물리 이동하지 않는다. manifest와 export 제거 정보는 한 곳에서 관리할 수 있도록 바꾸고 변경 자체의 부정 사례를 F1에 포함한다.

후속 물리 이동은 필요해진 시험군 하나만 수행한다. 이전/이후 사례 목록·cfg·fixture 경로·선택/ignore/미검증 상태를 대응하고, source 변경 범위에 맞는 컴파일·선택 검증을 한다. 원 증거의 selector와 hash는 바꾸지 않는다. `apply/retention` 이름 호환 제거는 실제 모든 소비·선택 대응을 확인한 별도 단위이며 이 계획의 선행 조건이 아니다.

EX08은 [`semantic_row/transition_tests.rs`](../../crates/engine/src/storage/semantic_row/transition_tests.rs), EX09는 [`plan4_retention/recovery/row_tests.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/recovery/row_tests.rs)의 A 소유 `module-row-insert` literal fixture 참조다. 공통 semantic/recovery 계약을 검증하는 입력이면 C 소유 fixture로 옮기는 안을 우선 검토하고, A 고유 인수 입력이면 소유 또는 예외 유지 이유를 명시한다. EX08의 `review_when`인 C 소유 fixture 이관과 EX09의 동반 검토를 완료 조건에 연결한다. fixture bytes·작성 provenance·원 hash와 새 경로 대응을 보존하고 독립 oracle을 제품 encoder로 대체하지 않는다. 참조만 숨기는 복사·중복 fixture 생성으로 예외 수를 줄이지 않는다. 제거 시 원 사례의 발견과 C+A/C+B 적용성을 확인하며, 유지 시 담당·근거·재검토 조건과 미해소 상태를 남긴다.

<a id="wal-cost"></a>
## 10. F6 — WAL 재검증 비용 측정과 조건부 개선

**정적 근거:** [`row_preflight::observe_sources`](../../crates/engine/src/storage/wal_io/plan4_retention/row_preflight.rs)가 gate 보유 중 [`history::validate_live`](../../crates/engine/src/storage/wal_io/plan4_retention/history.rs)를 호출한다. `replay_durable`은 prefix hash→재생→prefix hash를 수행한다. WAL 구간이 길어질수록 비용이 증가할 수 있다는 가설이며, 측정된 처리량 회귀로 기록하지 않는다.

**F6-a 조사·측정:** 같은 데이터/작업 의미에서 WAL 길이만 달리한 유한 입력을 구성한다. INSERT 지연, 읽은 WAL byte·재생 frame 수·hash byte, gate 대기/보유 시간, 읽기 지연을 관측한다. baseline/candidate source, I/O adapter와 장치, 캐시·checkpoint/base 조건, 반복 수, 시간·disk 예산, 결과 무효·중단 기준은 실행 전에 [DP07](../policies/development-policy.md#section-8)에 맞춰 동결한다. [D03](#user-decisions)으로 측정 권한·변경 시험 범위를 확인하고, 기존 PF04/COMM 조건을 임의 전용하지 않는다. 현재 계획에서는 임의 성능 합격 수치를 정하지 않는다.

**2026-11-02 F6-a 단위:** 사용자는 D03에서 "정적 조사·측정 계획까지만"을 골랐다. main `d86bafb`의 재검증 경로·1회 비용·거래당 호출 하한·잠금을 코드에서 읽고, 측정 질문·입력·관측·환경·반복·예산·무효/중단 기준의 초안을 [F6-a 기록](../evidence/f6-wal-revalidation-survey-20261102.md)에 남겼다. 지금 제품에서는 commit·rollback 거래와 그 행이 Job cell 64개를 하나씩 쥐고 실행 중에는 비우지 않아(live Row retirement 부재) 한 epoch의 한 행 거래가 32개 이하다. Forward 전 Cancel은 cell을 돌려주므로, 측정 계획은 Reserve→Cancel 순환으로 WAL을 64 MiB까지 늘린다. 측정·계측 코드·제품 변경은 없다. 사용자는 2026-11-02 측정 시기를 live Row retirement(DEV3) 공급 뒤로 미뤘고, 측정은 그때 그 기록 9절의 나머지 결정과 함께 별도 단위로 한다.

**2026-11-02 F6 M0 단위:** live Row retirement가 PR #36으로 병합된 뒤, 사용자의 "이어서 해주세요"에 따라 [M0](../evidence/f6-wal-revalidation-survey-20261102.md#f6a-m0)를 했다. 범위는 결정 카드로 물었고 답을 기다리며 추천안(M0만, 클라우드)으로 진행했다. `history.rs`에 cfg(test) 계수기만 넣었다. 한 행 commit 거래는 전체 replay 4회, 그 live 정리는 6회이며, 정리까지 한 거래 하나가 30·E₀ + 8.7 MB를 읽는다. 시간(M1)과 병목 판단은 하지 않았다.

**F6-b 조건부 설계:** 병목이 확인된 경우에만 아래를 비교한다.

1. 현 전체 검증 유지: 비용이 허용 범위면 가장 단순한 안으로 종료한다.
2. 기존 base/checkpoint 전환으로 재생 구간을 줄임: 현재 공급 여부와 전환·복구 비용을 함께 확인한다.
3. 원 owner가 검증 완료 상태를 보유하고 새 durable 구간만 반영: epoch·LSN·offset·generation·원 fence/namespace와 결속한다. 초기화·recovery·truncation·writer 교체·실패/UNKNOWN·외부 변경 검출 등 무효화 조건을 모두 설계한다. prefix 불변성이 증명되지 않으면 기존 hash를 생략하지 않는다.

제품의 전체 재생 검증은 복구 경로에 유지하고, 독립 oracle은 별도 구현·입력 신뢰 경계를 유지한다. 새 cache가 권한을 발급하거나 stale 상태로 stage/install을 통과시키지 못해야 한다. 복구·fault cut·손상·재시작·자원 상한과 동등 의미를 확인한 뒤 측정 이득을 판단한다. 효과가 없거나 복잡도 대비 이득이 부족하면 후보를 폐기하고 원 설계를 유지한 결과를 남긴다.

<a id="documents"></a>
## 11. F7 — 현재 계약과 과거 기록의 분리

현재 작업의 진입점에는 지금 유효한 소유/API·지원 범위·잔여만 둔다. 단계별 진행 설명은 원 증거로 연결한다. [`disk.rs`](../../crates/engine/src/storage/disk.rs)의 공급 pending 설명처럼 실제 구현과 다른 현재 주석은 해당 단위의 문서 diff로 정정한다. 과거 시점의 pending/FAIL 기록은 수정하지 않는다.

**진행:** 2026-11-02 `disk.rs`·`disk_api.rs`·`plan4_apply.rs`의 머리 주석과 `unused_imports` 허용 사유를 실제 공급 범위에 맞췄고, 필요 없는 `disk_api.rs` 첫 묶음 허용을 지웠다. 경계 밖에서 본 다른 후보는 고치지 않고 기록했다. 확인과 한계는 [F7 기록](../evidence/module-independence-survey-20261026.md#f7-current-comments)을 따른다.

A/B/C·C0·S단계는 기존 인수·이력의 식별자로 유지한다. 새 제품 책임은 읽기/변경/거래/복구 등 실제 역할로 설명하며 진행 ID를 장기 모듈 이름으로 확산시키지 않는다. 문서 상태 줄·탐색 색인·현재 코드가 일치하는지 확인하고, 같은 긴 규범을 여러 JSON/Markdown에 복제하지 않는다.

<a id="acceptance"></a>
## 12. 각 단위의 완료와 되돌림

단위별 완료는 다음 근거가 함께 있을 때 기록한다.

- 목표 동작/경계와 실제 diff가 일치하고, 사전 지정 밖의 다른 기능 내부 수정은 없거나 별도 공통 계약 변경으로 설명된다.
- 변경 코드·계약에 필요한 선택 검증과 독립 D1/D2 검토의 미해결 필수 지적이 없다. 기존 원 실패·미실행·source별 결과는 보존한다.
- 구현된 API·예외·시험 선택·export 정보와 문서 색인이 일치한다. 새 경로는 구현 때만 build inventory에 등록한다.
- 되돌릴 commit과 영향 형식/fixture를 기록한다. wire/semantic 변경이 필요하면 기존 호환 정책의 별도 검토를 거친다. 성능 후보는 채택 전 분리하며 원본 증거는 삭제하지 않는다.

모든 F단위를 새 기능 하나의 일괄 선행으로 강제하지 않는다. F0의 영향 경로 정리·변경 검증 확보 뒤 해당 기능에 필요한 경계 개선만 결합한다. 완료 판단에는 실제 공통 계약 재개방 수, 다른 기능 수정 유무, 선택 시험 범위, review 재작업과 대기 원인을 사용한다. 파일/trait/crate 수나 대조군 없는 속도 배율은 목표로 삼지 않는다.

<a id="decision"></a>
## 13. 계획 결정과 저장 기록

| 항목 | 기록 |
| --- | --- |
| ID·분류 | `XADB-MODULE-FOLLOWUP-001` · D2 후속 작업 순서·비교안의 계획 검토 |
| 근거 | 2026-11-01 사용자의 전체 리뷰·계획 문서 우선 작성 요청, 후속 PR 제출·댓글 확인·개선 요청과 [PR #23 리뷰](https://github.com/et16kr/xadb/pull/23#pullrequestreview-5728953170) |
| 담당 | 작성·기존 계약 대조·통합 `/root` |
| 선택 | 현재 경계와 안전성을 유지하며 실패 분류·검증 공백을 우선 보완하고 실제 소비 경로별로 C 결합과 비용을 줄임 |
| 비교 대안 | 전면 crate 재작성은 권한/복구/시험 이관 비용이 큼. gate 규칙만 확대하면 내부 복잡도와 동작 검증 공백이 남음. 기능 개발만 계속하면 역방향 예외·수명·자원 회계 결합이 누적될 수 있음 |
| 채택 범위 | 실행 순서와 설계 검토 대상. 개별 API·cache·시험 정책 변경안의 확정 또는 구현·측정 승인으로 대체하지 않음 |
| 대체 관계 | 기존 M5·MI03 판정, 원 소유 계약, 시험 제한, 원격 보호 정책을 대체하지 않음. 실행 시 바뀌는 계약만 해당 owner 문서에서 D2로 갱신 |
| 최초 검토 | `/root/followup_plan_review`가 최초 계획·색인과 기존 방침·기준선·관련 코드 경계를 대조해 계획 범위 D2 ACCEPT, 필수 수정 0으로 판정했다. 권고 1건(제품의 복구 재생과 독립 oracle의 구현/입력 신뢰 경계를 분리해 서술)을 §10에 반영했다. 이번 수정과 개별 후속 설계·구현의 검토를 대신하지 않는다 |
| 의견 반영 | 정량 기준선, 분류와 CI 차단 해제의 구분, 작업별 실패 파일, resident 미검증 8개 후속 작업, D01–D03 결정, EX01–EX09 처리 기준과 현재 색인을 보완. [정정 댓글](https://github.com/et16kr/xadb/pull/23#issuecomment-6560583019)에 따라 독립 평가 보고서로 확대하지 않고 실행 계획 범위를 유지 |
| 수정 검토 | `/root/followup_plan_review`가 수정 계획·색인 3개 파일을 기존 정책·최신 증거·manifest와 대조하고 정량 기준선을 독립 재집계해 D2 ACCEPT, 필수 수정 0으로 판정했다. root README의 낡은 현재 안내도 갱신하라는 권고 1건을 반영했고, 평가 추가 권고 철회를 포함한 최종판 재확인에서 ACCEPT·필수 수정 0·추가 권고 0을 확인했다. 계약 대조·통합 담당 `/root`가 반영을 확인했으며 개별 후속 구현의 검토를 대신하지 않는다 |
| 재검토 조건 | 기준선의 관련 코드·정책 변경, 신규 실제 소비자, 측정이 비용 가설을 지지하지 않음, 기존 실패 원인의 영향 확대 |
| 현재 실행 | 문서·색인 보완. 계획 저장 시점의 F0–F7 구현·제품 시험·성능 측정은 NOT_RUN이었다. 후속 F0는 기존 증거 분류 뒤 current-format 이관과 선택 검증까지 마쳤으며 원 기록·역사적 attribution 한계는 보존한다. F1 구현·선택 검증은 위 실행 기록을 따르며, F2의 제어 구현·선택 검증·독립 검토는 [F2 기록](../evidence/module-independence-survey-20261026.md#f2-contract-selection)을 따르며 제품 시험은 NOT_RUN이다. F5-a 제어 구현·선택 검증은 [별도 기록](../evidence/module-independence-survey-20261026.md#f5-test-ownership)을 따른다. F3의 읽기 호출·소유 pilot은 [별도 기록](../evidence/module-independence-survey-20261026.md#f3-catalog-call)을 따른다. F4는 PR #31·#32로 병합됐고, F5-b는 PR #33으로 병합됐고 [별도 기록](../evidence/module-independence-survey-20261026.md#f5-scope3-fixture)을 따른다. F6-a 정적 조사·측정 계획은 [F6-a 기록](../evidence/f6-wal-revalidation-survey-20261102.md)이며 측정은 사용자 결정으로 live Row retirement 공급 뒤로 보류됐고 F6-b는 미완료다. F7은 현재 진입 안내를 동반 정리했고 `disk.rs`·`disk_api.rs`·`plan4_apply.rs` 현재 주석 정정은 [F7 기록](../evidence/module-independence-survey-20261026.md#f7-current-comments)을 따른다. [문서 운영 기준](../policies/documentation-policy.md#push-gate)에 따라 push 직전에 문서 검사와 전송 commit의 필수 훅을 수행하고 실제 결과는 검사 산출물·PR에 기록. Clippy의 시험 target 컴파일은 제품 시험 실행으로 세지 않음 |

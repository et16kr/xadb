# XADB 모듈 분리 후 안정화·경계 개선 실행 계획

2026-11-01 · **후속 계획의 독립 D2 검토 ACCEPT / 아래 구현·진단·시험 NOT_RUN**

사용자의 모듈 분리 전체 리뷰와 “일단 계획 문서부터 만들어 주세요, main에 직접” 요청을 실행 순서로 구체화한다. 이번 작업은 이 문서와 탐색 색인의 작성·로컬 `main` 저장에 한정한다. 아래 제품·검사기·시험·측정 작업을 시작한 것으로 해석하지 않는다. 이후 개발 재개 지시가 오면 해당 단위의 기존 위임과 D1/D2 절차 안에서 진행하며 일상 구현마다 재승인을 요구하지 않는다.

[기존 분리 계획](module-independence-plan.md) · [소유 경계](../contracts/dev3-module-boundaries.md) · [소비자 계약](../contracts/dev3-module-consumers.md) · [모듈 gate](../policies/repository-build-plan.md#module-gate) · [개발 방침](../policies/development-policy.md#section-2)

<a id="baseline"></a>
## 1. 기준선과 완료 범위

- 계획 작성 기준은 PR #22가 병합된 `main` `9dc9bf7def25e1bf6bd89f9425f543b789475a7c`다. 제품 코드는 리뷰 기준 `76ec2cf5`와 같고, #22의 마지막 commit `e64ca616`은 보정 실행의 미검증 범위와 기존 실패 분류를 보완했다. 이미 보완된 문구를 신규 미해결 항목으로 다시 만들지 않는다.
- 기존 M5는 선정한 INSERT(A)·읽기(B)의 독립 구현과 합류에 대한 한정 완료다. [MI03 한계](../evidence/module-mi03-limits-20261101.md)의 PASS 108·PARTIAL 1(MCB03-V01)·N/A 20을 유지한다. 이 계획은 M5를 다시 열거나 전체 저장 엔진·DEV3·제품 인수를 완료로 바꾸지 않는다.
- 원격 gate는 #22의 [적용 기록](../evidence/module-independence-survey-20261026.md#s12-remote-rollout)을 따른다. 동일 이름 상태·검토자 문자열·실행 receipt의 신뢰 한계는 기존 수용 범위를 유지한다. 이번 계획이 GitHub App 도입이나 보호 설정 변경을 요구하지 않는다.
- [전체 lib 보정 기록](../evidence/module-independence-survey-20261026.md#s12-full-lib-calibration)은 정확한 사례 1,514개 중 1,488 통과·26 실패, macro 모듈 선택 7개 중 2개 실패다. resident의 `cut!` 8개는 abort 1개·미시작 7개로 미검증이다. 26개 중 이관 전 시험 6개, 같은 Reserve 차이가 기록된 `retention_live_lifecycle` 1개, 나머지 미분류 19개를 구별한다. 두 모듈 선택도 원인 분류가 남는다.
- 리뷰의 직접 확인 범위는 계약·주요 코드 경로·Git 이력·정적 의존 재집계다. 당시 gate의 등록 규칙 위반은 0이었으나 이는 C 내부의 독립성이나 제품 동작 PASS를 뜻하지 않는다. 외부 실행 원본 로그 전체를 독립 검산하거나 제품 시험을 다시 실행하지 않았다.

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

| 단위 | 우선순위·선행 | 산출물 | 완료 판정 |
| --- | --- | --- | --- |
| F0 기존 실패·미검증 정리 | 최우선. 기존 로그·source 조사부터 | 사례별 분류·영향 경로·담당·수정 또는 보류 조건 | 영향받는 다음 작업의 미분류 차단 원인이 없고, 유지할 실패는 근거·적용 범위가 명확 |
| F1 gate 자체 변경 검증 | 최우선. F0 전체 종료를 기다리지 않음 | Python 변경 시험 계획·비권한 실행·결과 확인 | 새 검사기 결함이 해당 변경에서 실패하고, 무관한 제품 시험은 실행하지 않음 |
| F2 공통 변경의 계약 시험 선택 | F1의 결과 형식 확정 뒤. 정책 충돌 먼저 정리 | 변경→계약→최소 사례 대응과 export별 선택 | 선택 누락·중복·잘못된 source 결속을 거절하며 기존 변경 범위 정책 준수 |
| F3 Catalog 역방향 의존·C 소유 경계 | F0의 해당 경로 분류 및 F2 검증 경로 확보 | Catalog 읽기 호출 방향 정리, 좁은 C 책임 하나의 소유표 | EX01–EX04의 대상 호출 해소·원 권한/자원 보존·선택 인수 |
| F4 작업 API·자원 회계 단순화 | 실제 소비자 하나 선정. F3와 같은 파일이면 직렬 | 작업 handle/관련 타입 후보 하나와 독립 예산 계약 | 소비자에 노출되는 내부 관계 감소, 소유·오류·상한 인수 유지 |
| F5 시험 소유·선택 식별 정리 | F2 대응 형식 확정 | 안정 사례 ID→실행 경로, export 제거 정보의 단일 관리 | 동일 사례 집합·발견·제외 이유 보존, 단독 export 유지 |
| F6 WAL 재검증 비용 조사·조건부 개선 | 설계 조사·계측 범위는 조기에 정함. 실행은 해당 F0 영향 정리 뒤 | 유한한 측정 계획·결과, 필요할 때 증분 검증 D2 | 병목 판단 또는 동등 정확성·자원·복구를 만족하는 개선 |
| F7 현재 계약·진입 문서 정리 | 각 단위에 동반. 전면 문서 개편은 하지 않음 | 현재 상태/계약과 역사 증거의 분리 | 작업 진입점과 현재 코드 설명 일치, 과거 identity 유지 |

첫 구현 단위는 F1로 잡고 F0의 기존 증거 분류를 함께 진행한다. F0가 끝나기 전에는 영향을 모르는 제품 수정·측정으로 확대하지 않는다. F6의 병목 가설과 계측 설계는 F3–F5 완료를 기다릴 이유가 없지만, 측정·제품 최적화는 별도 단위다. 이 표는 병렬 agent 실행을 지시하지 않는다. 실제 동시 작업은 소유 파일·계약 의존을 확인한 뒤 기존 병렬 정책을 따른다.

각 단위의 착수 입력은 기준 commit, 실제 diff, 소유/소비자, 현 계약과 바꿀 계약, 선택 사례, source별 기존 결과, 시간·disk 예산, 중단 조건이다. 담당·독립 검토자·통합 담당은 실제 착수 때 식별한다. 미배정 역할을 이미 수행한 검토자로 기록하지 않는다.

<a id="failures"></a>
## 4. F0 — 기존 실패와 미검증을 개발 가능한 상태로 정리

**입력:** [보정의 실패 표와 후속 요구](../evidence/module-independence-survey-20261026.md#s12-full-lib-calibration), [기존 실패 조사](../evidence/main-preexisting-failures-20261030.md), 당시 source와 확보 가능한 원 로그다. 먼저 보존 자료를 읽고 원인별로 묶는다. 같은 panic 위치만으로 같은 원인이라고 판정하지 않는다.

각 사례에 `case ID/selector, source, 최초 실패, 현재 계약, 분류, 근거, 영향 경로, 담당, 다음 조치, 완료 조건`을 대응한다. 표의 정본은 기존 증거 문서에 두고 이 계획에 실패 목록을 복제하지 않는다. 원본을 확보하지 못하면 해당 판단을 미확인으로 남긴다.

1. 미분류 19개와 두 macro 모듈 선택에서 제품 불변식·정리/Drop·자원 상한 문제가 있는지 우선 분류한다. resident abort와 뒤의 7개 미시작을 구별한다.
2. `retention_live_lifecycle`의 알려진 Reserve 차이에 대해 후속 실행 경로까지 원인이 확인됐는지 대조한다. 알려진 첫 차이를 전체 시나리오의 원인으로 확대하지 않는다.
3. format1 이관 전 6개는 사용자가 그대로 보존하기로 한 결정을 유지한다. 새 current-format 인수가 필요하면 별도 fixture·oracle·대응표를 설계한다. 기존 실패를 끄거나 기대값만 맞추는 변경은 하지 않는다.
4. 제품 결함·시험 결함·환경 결함의 수정은 원인별 작은 diff로 분리한다. 수정 코드와 원 실패에 해당하는 선택 검증만 수행한다. 변경 없는 원인 조사에 새로운 실행이 꼭 필요하면 입력·관측 질문·유한 범위와 기존 정책 적용 여부를 먼저 정한다. 과거 PF04/COMM의 한정 예외를 일반화하지 않는다.

F0의 분류 완료와 제품 수정 완료는 별도 상태다. 정상 동작을 검증하지 못한 경로의 새 기능 인수는 보류하되, 무관한 기능 전체를 정지시키지 않는다. resident의 미검증 8개는 실제 도달·완료 증거가 생길 때만 상태를 바꾼다.

<a id="gate-tests"></a>
## 5. F1 — gate·runner 변경을 해당 PR에서 검증

**현재 근거:** [`RUST_INPUTS`와 `selection`](../../scripts/check_module_gate.py), [build runner](../../scripts/module_gate_build.py), [build workflow](../../.github/workflows/module-build.yml), [기존 시험](../../scripts/test_check_module_gate.py). Python만 바뀌면 Rust 선택은 비고, 신뢰 판정은 기본 branch의 코드를 사용하므로 head의 새 Python 동작을 자동 검증하지 않는다.

**작업:** 변경한 checker·runner·export·push helper에 대응하는 Python 시험을 선언하고, PR head의 후보 코드를 토큰 쓰기 권한·secret 없는 job에서 실행한다. 신뢰 제어 workflow는 계속 기본 branch 코드를 사용하며 후보 코드를 import하거나 실행하지 않는다. 실행 결과는 변경 범위·head/source·시험 목록·runner 버전에 결속하여 확인한다. 이 작업은 후보 Python의 기능 회귀 검출이며 기존 receipt의 악의적 변조 한계를 해결했다고 주장하지 않는다.

**검증:** 관련 변경에서 새 후보의 syntax/runtime 오류, 잘못된 의존 판정, 잘못된 선택, 누락·불일치 결과가 성공으로 처리되지 않는지 필요한 부정 사례를 고정한다. docs-only 변경은 이 시험을 실행하지 않고, Python-only 변경은 무관한 제품 시험을 실행하지 않는다. no-test/skip/실패/중단을 성공으로 바꾸지 않는다.

**시행:** D2로 planner·runner·workflow와 결과 계약을 함께 검토한다. 새 결과를 요구하는 제어와 이를 내는 runner의 도입 순서를 정하고, 채택 PR을 영원히 대기시키거나 이전 결과로 새 의무를 충족시키지 않게 한다. 원격 보호 설정은 변경하지 않는다. 현재 신뢰/실행 경계를 합치는 안, 수동 리뷰 기록만으로 후보 시험을 대신하는 안은 채택하지 않는다.

<a id="affected-tests"></a>
## 6. F2 — 공통 변경과 최소 계약 사례 연결

**현재 근거:** [선택기](../../scripts/check_module_gate.py)의 `changed_cases/selection/coverage_findings`와 [gate manifest](../policies/module-gate.json). 공통 C·codec·semantic 변경은 소비자 A/B의 시험을 자동 선택하지 않는다. record selector는 head에만 추가되므로 상대 기능 부재 조건에서만 드러나는 실패의 요청 경로도 제한된다.

**작업:** 첫 적용은 `ReservedRow`의 prepare/apply/cancel 실패 효과처럼 실제 공통 변경 하나로 제한한다. `변경 경로/의미 → 영향 계약 → 검증 사례 ID → head 또는 C+A/C+B → 기존 실행 재사용 가능 조건`을 등록한다. production 호출과 시험 파일 위치를 구별하고, 해당 사례의 입력·oracle·harness 변경도 영향 판정에 넣는다. 전체 전이 의존을 재귀적으로 시험하는 체계는 만들지 않는다.

**정책 경계:** [AGENTS.md](../../AGENTS.md)의 무변경 코드·완료 시험 반복 금지가 우선한다. 이 계획은 그 규칙을 새 해석으로 대체하지 않는다. 변경된 공통 구현을 직접 검증하는 사례인지 근거를 제시하고, 기존 규칙과 충돌하는 소비자 재실행이 필요하면 정책 변경 범위를 먼저 명시한다. 별도 권한이 필요한 정책 변경은 확정 전 시행하지 않는다. 현재 정책 안에서 실행할 수 없는 경우 영향과 `NOT_RUN` 이유를 표시하고, 실행되지 않은 사례로 coverage를 채우지 않는다.

**완료 조건:** fixture가 다른 export로 잘못 연결되거나, 필수 사례가 0개 발견되거나, 동일 source의 동일 사례를 중복 실행하거나, 영향에 해당하는 선택이 누락된 경우를 검출한다. export별 요청을 지원할 때도 제거된 상대 기능을 다시 넣지 않는다. Python 제어 검증은 F1을 사용하며 제품 시험은 변경된 계약 경로에 한정한다.

<a id="catalog"></a>
## 7. F3 — Catalog 호출 방향과 C 내부 소유

**현재 근거:** [`catalog/read.rs`](../../crates/engine/src/storage/catalog/read.rs)는 disk API를 소비하지만, C의 [`read/scope.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/read/scope.rs)·[`read/statement.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/read/statement.rs)·[`read/lookup.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/read/lookup.rs)가 제품 함수를 역호출한다. manifest EX01–EX04의 기존 예외다. Catalog CREATE도 현재 `catalog/`의 별도 제품 본문으로 완성된 상태가 아니다.

첫 단위는 읽기만 대상으로 한다. 원 scope가 admission과 결과 소비 수명을 계속 소유하고, Catalog 제품 쪽이 checked operation을 소비하여 검색·가시성 선택·결과를 조립하도록 signature와 caller 이동표를 설계한다. `finish_table`의 유일 발급 근거, read-set 등록 시점, cleanup 오류 우선순위, 반환된 view 수명은 유지한다. 새 raw factory나 검증 생략 callback을 추가하지 않는다. 같은 검산을 두 계층에 복제하지 않는다.

관련 현재 소비자를 모두 이전한 뒤 EX01–EX04를 경로별로 제거하고, 실제 정적 참조와 생산/시험 컴파일 및 변경된 읽기 인수로 확인한다. `source_bytes`도 제품 본문을 호출하지 않는 고정 예산 계약으로 정리할 수 있는지 검토한다. CREATE는 실제 다음 기능에서 제품 순서와 설치 권한의 소유를 별도 결정한다. 읽기 단위에 전체 DDL 재설계를 묶지 않는다.

C 내부는 실제 변경하는 책임 하나부터 `소유 상태, 허용 소비, private 생성, 하위 효과, 복구/자원 계약`을 표로 정리하고 manifest에 반영한다. 후보는 거래·적용 조립, 페이지/버퍼 효과, 복구 조립, 자원 admission이다. 기존 초기화 조상·seal은 유지한다. 잔여 C를 새 이름 여러 개로 나누거나 빈 모듈을 만드는 것은 완료가 아니다.

<a id="operation-api"></a>
## 8. F4 — 작업 API와 자원 회계의 변경 전파 줄이기

**현재 근거:** [`PreflightRow`](../../crates/engine/src/storage/wal_io/plan4_apply/row/write.rs)는 13개 lifetime과 namespace 타입을 노출하고 [`A live adapter`](../../crates/engine/src/storage/row/insert/driver/live.rs)가 이를 전달한다. C의 caller byte 예산과 일부 census 시험은 A 구현 크기를 알아야 한다.

첫 후보는 Row preflight→reserve→prepare/apply/cancel 한 흐름이다. 다음 대안을 원 권한과 자원 회계 위에서 비교한다.

| 대안 | 판단 기준 |
| --- | --- |
| 현 타입 유지 | 실제 변경 전파가 작으면 유지 가능. 단순 rename·별칭만으로 독립성이 좋아졌다고 세지 않음 |
| 제한된 작업 handle·관련 타입 | 내부 borrow 관계를 소비자 signature에서 감출 수 있는지, 원 owned error·재시도·cancel을 보존하는지 확인 |
| heap/동적 dispatch | allocation·자원 상한·호출 비용·삭제 비용까지 검토. lifetime 수만 줄이려고 채택하지 않음 |

A와 C는 각자 고정된 예산을 검증하고 조립 담당이 공존하는 phase의 합을 검증한다. 기존 census는 source-bound inventory이며 실제 stack/RSS 측정과 동일시하지 않는다. 상한을 맞추기 위해 누락 항목·임의 여유 축소를 도입하지 않는다. 내부 수명 차이를 무조건 합치지 않는다.

완료는 타입 수 감소가 아니라 소비자 수정 범위 감소와 동작 보존으로 판단한다. 필요한 compiler 거절 사례(다른 owner/view 혼합, 완료 후 재사용, 잘못된 수명)와 실패 반환·취소·Drop fence·자원 상한의 변경된 경로를 확인한다. F3와 공유 파일을 바꾸면 하나씩 통합한다.

<a id="test-ownership"></a>
## 9. F5 — 시험 소유와 안정 식별

**현재 근거:** A/B의 실제 연결 시험이 C의 `plan4_recovery::tests::catalog_memory` 아래에 있고, [export 도구](../../scripts/module_single_source.py)는 별도 제거 목록을 갖는다. [`plan4_pages.rs`](../../crates/engine/src/storage/wal_io/plan4_pages.rs)는 selector 보존을 위해 같은 소스를 production `apply`·test `retention`으로 연다.

먼저 F2에서 사용한 소수 사례에 안정 ID→현재 libtest 이름→소유/입력→export 적용성 대응을 만든다. 제품 계약 시험은 A/B 소유, 내부 권한 probe는 C 내부, 합류 시험은 JOIN으로 구별하되 이 단계에서 모두 물리 이동하지 않는다. manifest와 export 제거 정보는 한 곳에서 관리할 수 있도록 바꾸고 변경 자체의 부정 사례를 F1에 포함한다.

후속 물리 이동은 필요해진 시험군 하나만 수행한다. 이전/이후 사례 목록·cfg·fixture 경로·선택/ignore/미검증 상태를 대응하고, source 변경 범위에 맞는 컴파일·선택 검증을 한다. 원 증거의 selector와 hash는 바꾸지 않는다. `apply/retention` 이름 호환 제거는 실제 모든 소비·선택 대응을 확인한 별도 단위이며 이 계획의 선행 조건이 아니다.

<a id="wal-cost"></a>
## 10. F6 — WAL 재검증 비용 측정과 조건부 개선

**정적 근거:** [`row_preflight::observe_sources`](../../crates/engine/src/storage/wal_io/plan4_retention/row_preflight.rs)가 gate 보유 중 [`history::validate_live`](../../crates/engine/src/storage/wal_io/plan4_retention/history.rs)를 호출한다. `replay_durable`은 prefix hash→재생→prefix hash를 수행한다. WAL 구간이 길어질수록 비용이 증가할 수 있다는 가설이며, 측정된 처리량 회귀로 기록하지 않는다.

**F6-a 조사·측정:** 같은 데이터/작업 의미에서 WAL 길이만 달리한 유한 입력을 구성한다. INSERT 지연, 읽은 WAL byte·재생 frame 수·hash byte, gate 대기/보유 시간, 읽기 지연을 관측한다. baseline/candidate source, I/O adapter와 장치, 캐시·checkpoint/base 조건, 반복 수, 시간·disk 예산, 결과 무효·중단 기준은 실행 전에 [DP07](../policies/development-policy.md#section-8)에 맞춰 동결한다. 측정 권한·변경 시험 범위를 확인하고, 기존 PF04/COMM 조건을 임의 전용하지 않는다. 현재 계획에서는 임의 성능 합격 수치를 정하지 않는다.

**F6-b 조건부 설계:** 병목이 확인된 경우에만 아래를 비교한다.

1. 현 전체 검증 유지: 비용이 허용 범위면 가장 단순한 안으로 종료한다.
2. 기존 base/checkpoint 전환으로 재생 구간을 줄임: 현재 공급 여부와 전환·복구 비용을 함께 확인한다.
3. 원 owner가 검증 완료 상태를 보유하고 새 durable 구간만 반영: epoch·LSN·offset·generation·원 fence/namespace와 결속한다. 초기화·recovery·truncation·writer 교체·실패/UNKNOWN·외부 변경 검출 등 무효화 조건을 모두 설계한다. prefix 불변성이 증명되지 않으면 기존 hash를 생략하지 않는다.

제품의 전체 재생 검증은 복구 경로에 유지하고, 독립 oracle은 별도 구현·입력 신뢰 경계를 유지한다. 새 cache가 권한을 발급하거나 stale 상태로 stage/install을 통과시키지 못해야 한다. 복구·fault cut·손상·재시작·자원 상한과 동등 의미를 확인한 뒤 측정 이득을 판단한다. 효과가 없거나 복잡도 대비 이득이 부족하면 후보를 폐기하고 원 설계를 유지한 결과를 남긴다.

<a id="documents"></a>
## 11. F7 — 현재 계약과 과거 기록의 분리

현재 작업의 진입점에는 지금 유효한 소유/API·지원 범위·잔여만 둔다. 단계별 진행 설명은 원 증거로 연결한다. [`disk.rs`](../../crates/engine/src/storage/disk.rs)의 공급 pending 설명처럼 실제 구현과 다른 현재 주석은 해당 단위의 문서 diff로 정정한다. 과거 시점의 pending/FAIL 기록은 수정하지 않는다.

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
| 근거 | 2026-11-01 사용자의 전체 리뷰와 계획 문서 우선 작성·main 직접 저장 요청 |
| 담당 | 작성·기존 계약 대조·통합 `/root` |
| 선택 | 현재 경계와 안전성을 유지하며 실패 분류·검증 공백을 우선 보완하고 실제 소비 경로별로 C 결합과 비용을 줄임 |
| 비교 대안 | 전면 crate 재작성은 권한/복구/시험 이관 비용이 큼. gate 규칙만 확대하면 내부 복잡도와 동작 검증 공백이 남음. 기능 개발만 계속하면 역방향 예외·수명·자원 회계 결합이 누적될 수 있음 |
| 채택 범위 | 실행 순서와 설계 검토 대상. 개별 API·cache·시험 정책 변경안의 확정 또는 구현·측정 승인으로 대체하지 않음 |
| 대체 관계 | 기존 M5·MI03 판정, 원 소유 계약, 시험 제한, 원격 보호 정책을 대체하지 않음. 실행 시 바뀌는 계약만 해당 owner 문서에서 D2로 갱신 |
| 검토 | `/root/followup_plan_review`가 계획·색인 4개 파일과 기존 방침·기준선·관련 코드 경계를 대조해 계획 범위 D2 ACCEPT, 필수 수정 0으로 판정했다. 권고 1건(제품의 복구 재생과 독립 oracle의 구현/입력 신뢰 경계를 분리해 서술)을 §10에 반영했고 `/root`가 확인했다. 개별 후속 설계·구현의 검토를 대신하지 않는다 |
| 재검토 조건 | 기준선의 관련 코드·정책 변경, 신규 실제 소비자, 측정이 비용 가설을 지지하지 않음, 기존 실패 원인의 영향 확대 |
| 현재 실행 | 문서와 색인 작성만. F0–F7 구현·제품 시험·성능 측정·자동 문서 검사·push NOT_RUN |

이번 문서 작업은 사용자의 명시적 요청에 따라 로컬 `main`에 직접 저장한다. 이는 이번 계획·색인 변경에 한정한 작업 위치 지정이며 앞으로의 제품 작업에 대한 PR 규칙을 없애지 않는다. 원격 `main`의 보호는 문서에도 적용되므로 로컬 저장·commit과 원격 반영을 구별한다. 보호 우회·ruleset 변경·PR 생성·원격 push는 이번 문서 작업에 포함하지 않는다.

문서 자동 검사는 [문서 운영 기준](../policies/documentation-policy.md#push-gate)에 따라 실제 push 직전의 commit에서 수행한다. 이번 로컬 계획 저장에서 기존 검사 결과를 갱신하거나 새 PASS를 만들지 않는다.

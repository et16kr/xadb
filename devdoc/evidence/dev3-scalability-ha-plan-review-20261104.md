# DEV3 개선 계획의 대용량 고성능 고가용성 검토

2026-11-04 · **정적 리뷰와 보완 권고 / D0 기록 추가 / 계약·일정·제품 구현 변경 없음 / 제품 시험 NOT_RUN**

[개선 계획 A0–A7](../plans/dev3-implementation-plan.md#review-adoption-20261104) · [이전 비교 리뷰](dev3-cross-dbms-design-review-20261104.md) · [DEV8 운영 설계](../contracts/dev8-operations-design.md) · [일정 기준선](../plans/dev3-schedule-baseline.md#review-adoption-20261104)

개선된 계획은 이전의 “첫 Row profile을 그대로 키우기 어렵다”는 지적에 대응하는 타당한 경로를 마련했다. WAL 반복 검증·로그 표현·영속 대기·쓰기 직렬화를 분리하고, 안전성 증명과 구현 동반 인수 조건을 붙인 점이 좋아졌다. **개선 설계를 진행하기에 적절한 계획이지만, Oracle·PostgreSQL급 대용량·고성능·고가용성 달성을 판단하려면 목표 규모·유효 처리량·장애별 서비스 중단 기준과 주요 설계 선택이 더 필요하다.**

이는 현재 계획의 개선 방향을 지지하는 평가다. 구체 알고리즘의 승인, 제품 성능 입증, 기존 D2 승인 취소를 뜻하지 않는다. “Oracle·PostgreSQL급”은 단일한 용량·TPS·HA 구성을 뜻하지 않으므로 비교 workload와 운영 구성을 먼저 좁혀야 한다. 현재 [제품 방향](../plans/project-direction.md)은 기본 기능·정확성·합리적인 성능을 지향하며 두 제품과의 전체 동등성을 약속하지 않는다.

<a id="scope"></a>
## 검토 기준과 범위

| 항목 | 기준 |
| --- | --- |
| 검토 source | PR #51 병합 main `8a4f8e4a6fee6b5498145d9ee9b17a7105e1c851` |
| 개선 범위 | PR #51의 DEV3-REVIEW-012 A0–A7, 관련 WAL·동시성·자원·checkpoint·audit·인수·일정 보강 |
| 이전 판단 | PR #48의 [Oracle 방향 리뷰](dev3-oracle-direction-review-20261103.md), PR #50의 [비교 재검토](dev3-cross-dbms-design-review-20261104.md) |
| 추가 대조 | DEV8 H1 복제·ACK·fencing·RPO/RTO·용량 인수, DEV9 성능 인수의 목표 동결 시점 |
| 방법과 한계 | 현재 계획과 소유 계약의 정적 대조. 새로운 전체 코드 감사·벤치마크·장애 시험 결과는 아님 |
| 기록의 권한과 분류 | 사용자의 개선 계획 리뷰 및 문서화·PR 요청. 담당 `/root`. D0 리뷰 기록과 색인 추가이며 채택 계약·형식·기능·완료 상태를 변경하지 않음 |

PR #51은 상세 설계·구현·검증을 완료로 표시하지 않는다. 그 구분을 유지하면서, 아래 권고는 대형 제품 목표에 대한 후속 결정 입력으로 제시한다. 이번 정적 검토를 별도 실행의 독립 D2 승인으로 기록하지 않는다.

<a id="improvements"></a>
## 이전보다 개선된 점

| 개선 | 평가 |
| --- | --- |
| 병목의 분리 | A1 live WAL 재검증, A2 redo·Undo·행 표현, A3 stage/sync, A4 writer·reader·공유 metadata를 분리했다. 로그 byte 감소와 sync 감소, 실제 병렬 실행을 같은 성과로 계산하지 않는다 |
| A1의 명확한 종료 조건 | [A1 비용 목표](../contracts/dev3-task-04-wal.md#a1-cost-target)는 정상 매 연산의 과거 전체 WAL hash/replay를 제거하고, 유지보수로 비용을 옮긴 경우 상각·누적 비용까지 평가한다. 안전한 후보가 목표를 만족하지 못하면 미완료로 남긴다 |
| 안전성 보존 | native 의미 입력, WAL-before-data, durable ACK/UNKNOWN, 같은 page의 설치 순서, snapshot·pin·rollback 예약을 대체 증명 없이 완화하지 않는다 |
| 구현과 검증의 연결 | [D3-ADOPT-01–07](../plans/dev3-test-plan.md#review-adoption-20261104)에 변경 기능별 작은 계수·경계·장애·재진입 사례를 연결하고 대형 확대는 S5로 구분했다 |
| 의존과 일정 | 일반 행·인덱스 형식에 영향을 주는 선택을 해당 형식 동결 전에 처리한다. 모든 후보의 완료를 일괄 선행으로 만들지 않으며, 추가 범위를 기존 일정에 숨기지 않고 재견적한다 |

mutable page·단일 WAL·영속 Undo·논리 rollback·redo-only CLR이라는 뼈대 자체가 대용량 DBMS를 불가능하게 만드는 것은 아니다. 이전 우려는 제한 profile의 정상 실행 비용과 직렬화·표현·운영 경로를 그대로 확대하는 데 있었다. 이번 계획은 그 문제를 설계 작업으로 끌어올렸다.

<a id="findings"></a>
## 남은 보완 사항

아래 우선순위는 제품 목표를 위한 결정 순서다. 현재 제한 구현의 새 정합성 결함이나 이미 채택된 계약 위반으로 분류하지 않는다.

<a id="serializable"></a>
### 1 기본 Serializable의 테이블 단위 충돌

**우선순위 높음.** [동시성 계약](../contracts/dev3-task-10-concurrency.md#serializable-conflict-bound)은 같은 테이블을 read-set에 등록한 k개 거래가 모두 snapshot q0를 잡은 뒤 commit 경쟁하면, 첫 성공의 table write epoch가 나머지 검증을 무효화할 수 있음을 명시한다. 서로 다른 행을 읽고 써도 해당 고정 cohort에서는 성공이 최대 1개이며 k=32이면 3.125%다. 이는 재시도가 계속되는 실제 steady-state TPS나 모든 업무의 성공률을 예측하는 수치가 아니다.

이 제약을 유지하면 실제 writer 준비·적용을 병렬화해도 성공 처리량이 늘지 않을 수 있다. A0/A4가 세밀한 Serializable 검증과 거래 의미를 비교 대상으로 올린 것은 타당하다. 다만 같은 테이블의 비충돌 업무를 중요한 목표로 삼는다면, 유지 여부를 호환성 선택만으로 끝내지 말고 허용 재시도율·durable 성공 TPS·원래 요청부터의 p99로 판단할 것을 권고한다.

후속 결정은 일정 11의 일반 동시성 구현 전, 앞선 행 형식에 영향이 있으면 그 동결 전에 닫는 기존 순서를 따른다. 현재 검증과 세밀한 row/range/absence 추적의 비용·phantom/write skew·DDL·추적 공간 고갈 시 fallback을 비교해야 한다. 기본 격리를 RC로 바꾸는 제품 선택과 Serializable 보장의 성능 개선은 따로 판단한다.

<a id="performance-targets"></a>
### 2 A2부터 A4까지의 개선 성공 기준

**우선순위 높음.** [A2–A4 산출물](../plans/dev3-implementation-plan.md#review-adoption-20261104)은 대안·안전성·자원·실제 동시 실행의 검증을 요구한다. 그러나 A1처럼 특정 구조 비용을 반드시 제거해야 한다는 목표나 대표 업무에서 만족할 처리량·지연 기준은 아직 확정되지 않았다. 안전한 후보를 선택하고도 성능 병목이 상당 부분 남을 가능성이 있다.

[DEV3 인수](../contracts/dev3-task-16-qualification.md#comparison-measurement)는 실제 측정 조건과 회귀 한도를 실행 전에 동결하고, [DEV9 F06](../contracts/dev9-qualification-design.md#performance)은 workload별 TPS·latency·RPO/RTO 목표를 실행 전에 정하도록 한다. 목표 설정 절차가 없는 것은 아니다. 보완 권고는 그중 설계 선택에 필요한 기준을 **후보 채택 전**으로 앞당기는 것이다.

| 후보 선택에 필요한 기준 | 기록할 조건 |
| --- | --- |
| 거래당 WAL bytes·sync 횟수 | row 폭·인덱스 수·변경 열·거래 크기·정리 비용과 실제 batch 크기 |
| 동시 요청별 durable 성공 TPS·요청 전체 p99 | 같은 table 비충돌/실제 충돌, 동일 격리·durability, retry·거절·timeout 포함 |
| reader 차단과 writer 직렬 구간 | 준비·read gate·sync·설치·publication별 보유 시간과 대기 |
| 정상 부하와 유지보수 공존 | cleanup·checkpoint·장기 reader·quota pressure 중 지연과 진행성 |

숫자는 장비·workload·기준선으로 정하며 이 리뷰에서 임의의 TPS나 배수를 확정하지 않는다. 후보가 목표를 충족하지 못할 때 재설계·제한 운영·제품 목표 변경 중 무엇을 택할지도 기록하면, “비교를 끝냄”과 “개선 목표 달성”을 구분할 수 있다.

<a id="availability"></a>
### 3 안전한 장애 전환과 서비스 가용성의 차이

**우선순위 높음.** DEV8에는 [H1 물리 복제](../contracts/dev8-operations-design.md#topology), [원격 영속 ACK](../contracts/dev8-operations-design.md#ack), [fencing과 역할 전환](../contracts/dev8-operations-design.md#failover)이 이미 있다. 복제·고가용성 계획 자체가 누락된 것은 아니다. 초기 H1은 지정 동기 standby 하나를 보호 대상으로 삼으며, 보호 실패 뒤 후속 strict write admission을 닫는다. 추가 async standby가 자동으로 보호 대상을 대체하지 않고, 초기 failover는 운영자 시작과 검증 가능한 외부 fence provider를 전제로 한다.

이는 데이터 안전성을 우선하는 타당한 선택이다. 다만 standby나 연결 장애에도 strict 쓰기가 중단될 수 있고 자동 선출은 초기 전제가 아니므로, 이 범위를 서비스 가용성 목표와 연결해야 한다.

[D8-P04](../plans/dev8-implementation-plan.md#performance)의 RTO 60초는 local harness에서 **fence 요청부터 새 client query 성공까지**다. 장애 발생부터 감지·운영자 대응까지의 시간은 포함하지 않는다. 따라서 전체 서비스 중단 시간이 60초 이내라는 뜻으로 사용할 수 없다. fence 불가 시 승격 거절을 안전성 성공으로 인정하되 RTO 충족으로 세지 않는 기존 구분은 유지한다.

후속 목표에는 장애별 `발생 → 감지 → 전환 개시 → fencing → 복구/readiness → client 재접속`의 시간을 연결하고, primary 장애·지정 standby 장애·네트워크 분리·fence provider 장애·보호 peer 동시 상실에서 읽기/쓰기의 지속·중단 범위를 명시할 것을 권고한다. 자동 전환이 제품 목표라면 승격 권한·partition 모델·보호 peer 교체·운영 제어의 별도 설계와 인수가 필요하다. quorum이나 active-active 자체를 이 리뷰에서 필수 해법으로 채택하지 않는다.

<a id="capacity"></a>
### 4 초기 디스크 시험에서 목표 제품 용량으로의 확대

64GiB qualification 준비와 allocator inventory가 buffer의 4배여야 한다는 조건은 메모리 상한과 실제 디스크 처리를 검증하는 의미 있는 개선이다. 그러나 [대형 인수](../contracts/dev3-task-16-qualification.md)에서 64MiB buffer에 필요한 최소 43,241 extents·45,341,474,816 bytes는 시험 fixture 계산이며, 최종 제품의 최대 용량이나 대형 workload 성능 보장이 아니다. [64GiB profile](../contracts/dev3-resource-bounds.md#qualification-profile-preparation)도 전체 회계·형식 적합성·fixture 공급 전의 준비 방향이다.

현재 기본 owned 8GiB, 최대 32거래, 거래당 256 mutation·1MiB 논리 변경 한도는 유지된다. [자원 보강](../contracts/dev3-resource-bounds.md#review-adoption-20261104)은 최대 크기 거래의 실제 동시 admission을 따로 계산하며, [동시성 보강](../contracts/dev3-task-10-concurrency.md#review-adoption-20261104)은 DEV7의 4,096 mutation·64MiB 확장 목표 비용도 비교하게 한다. [DEV8 P06](../plans/dev8-implementation-plan.md#performance)에는 RAM 초과 시험도 있다. 따라서 확대 계획이 전혀 없는 것은 아니다.

보완할 부분은 **최종 목표 규모와 단계별 지원 전환 조건**이다. 데이터량·table/index/partition 수·tree 높이·동시 거래·거래/LOB 크기·장기 snapshot을 함께 정하고, 각 단계에서 재사용할 형식/API와 변경할 예약·lock/write-set·Undo/WAL·복구 공간을 연결해야 한다. 예컨대 수백 GiB와 TiB 단위는 검토할 후보 규모이지 이 리뷰가 새로 약속하는 지원량이 아니다. 초기 제한을 둔 것 자체를 아키텍처 실패로 판단하지 않는다.

<a id="maintenance"></a>
### 5 Checkpoint와 전체 검사의 반복 I/O

A6는 [checkpoint](../contracts/dev3-task-13-checkpoint.md#resources)의 `ceil(T/B) × (I + W)`와 [full audit](../contracts/dev3-task-15-backup-integrity.md)의 `E × ceil(T/B)` 비용을 명시했다. 여기서 T는 target 수, B는 batch 크기, I/W는 base inventory와 replay WAL, E는 audit edge 수다. E와 T가 함께 증가하고 B가 고정되면 audit의 방문 수는 이차적으로 증가할 수 있다. source-WAL-free restore의 destination 전체 audit에도 같은 비용이 들어간다.

메모리 사용이 bounded라는 사실만으로 대형 운영 시간을 제한할 수는 없다. 현재 계획의 실제 읽기·시간·workspace·공존 disk 예산 동결과 예산 초과를 PASS로 세지 않는 조건은 타당하다. 여기에 A1처럼 **목표 규모에서 비용 예산을 충족하지 못하면 알고리즘 변경 또는 지원 범위 조정이 필요하다**는 후속 처리와 책임을 명확히 할 것을 권고한다.

증분 inventory·외부 인덱스·quota를 둔 spill/sort는 비교 후보이며, 기존 disk 사용 제한과 복구·cleanup 계약을 먼저 검토해야 한다. 정상 open의 선택 복구와 전체 audit의 비용은 계속 구분한다. 모든 정상 open이 전 DB를 검사한다고 해석하지 않는다.

<a id="follow-up"></a>
## 권고하는 후속 순서

| 시점 | 작업과 판단 자료 |
| --- | --- |
| 다음 A1 상세 설계 | 기존 순서대로 진행. 권한 상태·무효화·reopen·독립 replay 대조와 누적 비용 목표를 구체화 |
| 관련 후보 채택 전 | A2–A4가 선택할 수 있는 대표 업무·규모·격리·durability·장비와 성능 판단 기준을 정리. 실제 baseline 측정이 필요하면 유한 범위·예산을 별도로 동결 |
| 해당 형식 동결과 일정 11 전 | A0/A4의 table 충돌 정책·row 잠금·실제 writer/reader 진행을 함께 판단. 필요한 의미 결정과 독립 준비를 구분 |
| 대형 profile·A6 알고리즘 선택 전 | 목표 규모의 회계와 유지보수/복구 비용, 예산 미달 시 처리와 지원 전환 조건을 연결 |
| DEV8 HA 인수 구체화 | 기존 안전성 조건에 전체 서비스 중단 시간·장애별 가용 범위·자동화 여부를 추가 검토 |

이번 리뷰의 결론은 **일반화의 구조적 한계를 개선할 경로가 생겼고 다음 설계를 진행할 수 있다**는 것이다. A1만으로 성능·용량·HA 전체를 해결했다고 판단하지 않으며, 위 모든 권고를 A1 착수의 일괄 선행 조건으로 추가하지 않는다. 권고를 채택해 계약·제품 목표·필수 품질 기준을 바꾸는 경우 해당 owner의 D2/D3 결정과 일정 재견적을 따로 남긴다.

기존 V1 적립 33/86일=38.37%와 PR #49까지의 제한 공급 증거는 유지한다. 이 기록은 제품·모델·성능 시험을 새로 실행한 결과가 아니며, 이 문서 PR의 pre-push 정적·문서 검사 결과는 PR에 별도로 기록한다.

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

2026-11-04 후속: 사용자의 댓글 타당성 점검과 새 commit 반영 요청에 따라 PR #52의 두 리뷰 댓글을 대조해 본문을 보강했다. 최초판 `2dd4b7c`와 Claude 리뷰 추가 `8dad2914`의 기록은 Git 이력에 보존한다. [댓글별 반영 판단](#comment-followup)은 기존 계약의 설명과 권고를 구체화한 D0 기록이며, 새 알고리즘·지원 profile·제품 목표를 채택한 결정은 아니다.

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

[현재 RC](../contracts/dev3-task-10-concurrency.md#review-adoption-20261104)도 변경 대상의 observation mismatch에서는 거래 전체를 abort한다. RC 선택으로 같은 table의 다른 행에 대한 보수적 Serializable 충돌은 줄어들 수 있지만, 같은 행의 관측 불일치까지 해결되지는 않는다. A0의 statement restart·행 재평가 후보는 이 차이와 반환한 결과·외부 효과·rollback 예약을 함께 비교해야 한다. PostgreSQL RC의 [갱신된 행 조건 재평가](https://www.postgresql.org/docs/18/transaction-iso.html)와 Oracle RC의 [특정 UPDATE 충돌 시 문장 재시작](https://asktom.oracle.com/Misc/something-different-part-i-of-iii.html)은 비교 입력이며 동일한 실행 방식은 아니다. 재시작 도입 자체로 성공 TPS 향상을 보장하거나 RC 선택이 모든 workload에서 무효라고 단정하지 않는다.

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
| 지속 부하의 공간·대기열 안정성 | WAL·Undo·복제 backlog의 생성/확인된 반환률·잔량 추세, 지원할 snapshot·peer 중단 시간, admission 차단·해제 조건과 정상화 시간 |

숫자는 장비·workload·기준선으로 정하며 이 리뷰에서 임의의 TPS나 배수를 확정하지 않는다. 후보가 목표를 충족하지 못할 때 재설계·제한 운영·제품 목표 변경 중 무엇을 택할지도 기록하면, “비교를 끝냄”과 “개선 목표 달성”을 구분할 수 있다.

[Undo 보존 압력](../contracts/dev3-task-14-reclamation.md#retention-pressure-model)과 [DEV8 retention](../contracts/dev8-operations-design.md#retention)은 생성이 반환을 계속 웃돌면 유한 공간이 소진되는 조건과 안전한 admission 제어를 이미 다룬다. 이 조건을 위 목표 workload에 연결하여 짧은 성공 구간의 TPS와 지속 가능한 처리량을 구분할 것을 권고한다. A2의 WAL bytes 비교에는 로컬 정리뿐 아니라 복제 전송·보존 공간과 DEV8의 logical supplement 비용도 연결한다. 특정 INSERT profile의 byte 계수를 모든 거래의 복제 대역폭이나 정지 시간으로 일반화하지 않는다.

<a id="availability"></a>
### 3 안전한 장애 전환과 서비스 가용성의 차이

**우선순위 높음.** DEV8에는 [H1 물리 복제](../contracts/dev8-operations-design.md#topology), [원격 영속 ACK](../contracts/dev8-operations-design.md#ack), [fencing과 역할 전환](../contracts/dev8-operations-design.md#failover)이 이미 있다. 복제·고가용성 계획 자체가 누락된 것은 아니다. 초기 H1은 지정 동기 standby 하나를 보호 대상으로 삼으며, 보호 실패 뒤 후속 strict write admission을 닫는다. 추가 async standby가 자동으로 보호 대상을 대체하지 않고, 초기 failover는 운영자 시작과 검증 가능한 외부 fence provider를 전제로 한다.

이는 데이터 안전성을 우선하는 타당한 선택이다. 다만 standby나 연결 장애에도 strict 쓰기가 중단될 수 있고 자동 선출은 초기 전제가 아니므로, 이 범위를 서비스 가용성 목표와 연결해야 한다.

승격 완료와 strict-write readiness도 다르다. [역할 전환 계약](../contracts/dev8-operations-design.md#failover)에 따라 두 노드 중 유일한 standby가 primary로 승격하면, 새 지정 peer가 WAL/control·XA/sequence 의무를 인계받고 ConfigEpoch가 영속 선택될 때까지 strict 쓰기를 열지 않는다. 운영자가 새 ConfigEpoch로 async 전환하는 명시 경로는 있지만, 이때의 쓰기 성공을 strict 보호 복구나 같은 RPO 보장의 성공으로 세지 않는다.

[D8-P04](../plans/dev8-implementation-plan.md#performance)의 RTO 60초는 local harness에서 **fence 요청부터 새 client query 성공까지**다. 장애 발생부터 감지·운영자 대응까지의 시간은 포함하지 않는다. 따라서 전체 서비스 중단 시간이 60초 이내라는 뜻으로 사용할 수 없다. fence 불가 시 승격 거절을 안전성 성공으로 인정하되 RTO 충족으로 세지 않는 기존 구분은 유지한다.

[DEV9 복구 인수](../contracts/dev9-qualification-design.md#recovery-target)는 이미 동결한 사건 시작부터 독립 업무 probe의 정상 완료까지 detection·fencing·replay·undo·검증·reconnect의 전체 시간을 요구한다. 따라서 후속은 이 기존 기준을 D8-P04와 연결하고 장애별 목표·probe를 구체화하는 것이다. 모든 구간은 장애 발생부터 재며 새 peer 준비 시점에 시계를 다시 시작하지 않는다.

| 복구 지표 | 종료 증거와 포함할 구간 |
| --- | --- |
| 읽기 서비스 복구 | 새 역할·identity·허용 가시성을 검증한 client 읽기 업무 probe 성공. 감지·운영자 대응·fencing·복구·재접속 포함 |
| strict 쓰기 서비스 복구 | 보호 mode와 ConfigEpoch가 확인된 첫 strict commit 성공. 위 구간에 새 보호 peer 준비·동기화·의무 인계·영속 선택과 원격 보호 ACK 대기를 포함 |

primary 장애·지정 standby 장애·네트워크 분리·fence provider 장애·보호 peer 동시 상실에서 읽기/쓰기의 지속·중단 범위를 각각 명시할 것을 권고한다. 자동 전환이 제품 목표라면 승격 권한·partition 모델·보호 peer 교체·운영 제어의 별도 설계와 인수가 필요하다.

지정 standby 장애의 대안으로 다중 보호 peer와 교체 정책을 비교할 수 있다. PostgreSQL의 [우선순위 기반 `FIRST`와 quorum 기반 `ANY`](https://www.postgresql.org/docs/18/runtime-config-replication.html)는 서로 다른 후보이며 quorum만이 유일한 해법은 아니다. Oracle의 [복제 보호 모드·SYNC/AFFIRM](https://docs.oracle.com/en/database/oracle/oracle-database/19/sbydb/oracle-data-guard-protection-modes.html)도 실제 영속 ACK와 보호 peer 상실 시 동작을 맞춰 비교해야 한다. Far Sync라는 이름만으로 장애 중 strict RPO=0 지속을 추정하지 않는다. H1 standby는 recovery-only이므로 standby 읽기는 별도 읽기 분산 후보이며 snapshot·redo 경합·지연·자원 계약이 필요하다. quorum·active-active·standby 읽기를 이 리뷰에서 필수 기능으로 채택하지 않는다.

<a id="capacity"></a>
### 4 초기 디스크 시험에서 목표 제품 용량으로의 확대

64GiB qualification 준비와 allocator inventory가 buffer의 4배여야 한다는 조건은 메모리 상한과 실제 디스크 처리를 검증하는 의미 있는 개선이다. 그러나 [대형 인수](../contracts/dev3-task-16-qualification.md)에서 64MiB buffer에 필요한 최소 43,241 extents·45,341,474,816 bytes는 시험 fixture 계산이며, 최종 제품의 최대 용량이나 대형 workload 성능 보장이 아니다. [64GiB profile](../contracts/dev3-resource-bounds.md#qualification-profile-preparation)도 전체 회계·형식 적합성·fixture 공급 전의 준비 방향이다.

현재 [control profile](../contracts/dev3-control-format.md)의 ID1/2/3은 정확한 tuple만 수락한다. 아래는 계약상의 상한이며 모든 조합의 실제 동시 admission이나 일반 기능 구현 완료를 보장하지 않는다.

| 자원 | 현재 지원 tuple의 한도 |
| --- | --- |
| owned / protected | 8GiB / 그 안의 1GiB |
| buffer / 보존·예약 WAL | ID별 4/16/64MiB / 256MiB |
| 거래 / 논리 변경 | 최대 32거래 / 거래당 256 mutation·1MiB |
| private / system | 거래당 8MiB·전체 64MiB / 16MiB(flush 4MiB·maintenance 12MiB) |
| FD | 128 |

설정값만 올려 지원을 확대한 것으로 볼 수 없으며 새 profile ID·tuple, 소비하는 codec/API·고정 배열, 전체 예약·복구 산식을 검토해야 한다. 작은 buffer는 확대 workload의 위험 요소지만 buffer miss가 곧 장치 I/O나 병목이라는 뜻은 아니다. 접근 분포·OS page cache·CPU/I/O 계측을 함께 본다([PostgreSQL의 buffer/OS cache 구분](https://www.postgresql.org/docs/18/runtime-config-resource.html)). 첫 Row profile의 table≤27·32-slot/u32 mask도 [한정 표현](../contracts/dev3-task-10-concurrency.md)이며 일반 설계에 그대로 승계된다고 가정하지 않는다.

[자원 보강](../contracts/dev3-resource-bounds.md#review-adoption-20261104)은 최대 크기 거래의 실제 동시 admission을 따로 계산하며, [동시성 보강](../contracts/dev3-task-10-concurrency.md#review-adoption-20261104)은 DEV7의 4,096 mutation·64MiB 확장 목표 비용도 비교하게 한다. A7에는 위 64GiB qualification 준비가 명시되어 있고 [DEV8 P06](../plans/dev8-implementation-plan.md#performance)에는 RAM 초과 시험도 있다. 확대 준비는 존재하며, 최종 제품 기본 한도의 확대와 지원 workload는 아직 확정되지 않았다.

거래 크기는 잠금 위치와 함께 판단하되 원인을 잠금 표 하나로 축약하지 않는다. 현재 한도에서는 논리 row mutation 300회를 한 거래로 공급할 수 없지만, page에 잠금 소유를 옮겨도 write-set·Undo·WAL·rollback 예약·논리 변경량 한도는 남는다. bounded 메모리 row-lock 표, page 소유, 혼합형 각각을 목표 최대 거래 크기와 동시부하로 평가할 것을 권고한다. 이는 기존 A4의 형식 동결·일정11 전 비교 의무를 구체화하며, 큰 거래를 분할 commit하여 같은 원자성을 제공한 것으로 처리하지 않는다.

보완할 부분은 **최종 목표 규모와 단계별 지원 전환 조건**이다. 데이터량·table/index/partition 수·tree 높이·동시 거래·거래/LOB 크기·장기 snapshot을 함께 정하고, 각 단계에서 재사용할 형식/API와 변경할 예약·lock/write-set·Undo/WAL·복구 공간을 연결해야 한다. 예컨대 수백 GiB와 TiB 단위는 검토할 후보 규모이지 이 리뷰가 새로 약속하는 지원량이 아니다. 초기 제한을 둔 것 자체를 아키텍처 실패로 판단하지 않는다.

<a id="maintenance"></a>
### 5 Checkpoint·전체 검사의 반복 I/O와 쓰기 중단

A6는 [checkpoint](../contracts/dev3-task-13-checkpoint.md#resources)의 `ceil(T/B) × (I + W)`와 [full audit](../contracts/dev3-task-15-backup-integrity.md)의 `E × ceil(T/B)` 비용을 명시했다. 여기서 T는 target 수, B는 batch 크기, I/W는 base inventory와 replay WAL, E는 audit edge 수다. E와 T가 함께 증가하고 B가 고정되면 audit의 방문 수는 이차적으로 증가할 수 있다. source-WAL-free restore의 destination 전체 audit에도 같은 비용이 들어간다.

이 비용은 유지보수 처리 시간뿐 아니라 쓰기 가용성에도 영향을 준다. [checkpoint 불변 입력](../contracts/dev3-task-13-checkpoint.md#immutable-capture-source)은 큰 allocation/ownership inventory를 base+prefix에서 partition별로 재생하며, prefix pin 중에도 `retained+outstanding WAL≤256MiB`를 유지한다. 부족이 예상되면 새 foreground admission을 닫고 기존 의무와 checkpoint를 진행한다. 완료 후에도 current/previous·dirty/flush-inflight·다른 retention owner의 floor와 실제 삭제 확인이 있어야 공간이 반환된다. checkpoint 한 번의 완료가 모든 WAL 회수를 뜻하지 않는다.

WAL 회수가 없는 pin 구간 τ 동안, 기존 retained/outstanding과 필수 유지보수·rollback 예약을 중복 차감 없이 제외한 신규 foreground 여유를 M bytes라 하면 그 구간의 신규 foreground WAL 생성량은 M 안에 들어야 한다. 일정 생성률 λ라면 `λ×τ≤M`이 공간 조건이며 `M/τ`의 단위는 bytes/s다. 이는 TPS의 실측 상한이나 충분한 admission 증명이 아니다. 거래당 WAL·정리량, 다른 자원과 보존 주체, 실제 회수 시점을 함께 알아야 지속 처리량을 판단할 수 있다. 목표 규모별 pin 시간·WAL 잔량·쓰기 거절/대기·해제 후 정상화 시간을 인수 입력으로 권고한다.

[full audit](../contracts/dev3-task-15-backup-integrity.md)은 admission을 drain하고 mutation 중지 소유를 확보한 고정 상태에서 수행한다. 따라서 `E×ceil(T/B)` 재방문은 실제 실행 시간으로 환산해 DB 쓰기 중단 예산에 포함해야 한다. 방문 수 자체를 초 단위 시간으로 표기하지 않으며 drain 대기·필요 flush·검사·서비스 재개까지 따로 관측한다. restore destination의 audit는 그 destination의 readiness를 지연시키는 별도 경로다.

메모리 사용이 bounded라는 사실만으로 대형 운영 시간을 제한할 수는 없다. 현재 계획의 실제 읽기·시간·workspace·공존 disk 예산 동결과 예산 초과를 PASS로 세지 않는 조건은 타당하다. 여기에 A1처럼 **목표 규모에서 비용 예산을 충족하지 못하면 알고리즘 변경 또는 지원 범위 조정이 필요하다**는 후속 처리와 책임을 명확히 할 것을 권고한다.

증분 inventory·외부 인덱스·quota를 둔 spill/sort는 비교 후보이며, 기존 disk 사용 제한과 복구·cleanup 계약을 먼저 검토해야 한다. 정상 open의 선택 복구와 전체 audit의 비용은 계속 구분한다. 모든 정상 open이 전 DB를 검사한다고 해석하지 않는다.

A6의 구조 목표 후보는 **정상 checkpoint마다 변경 없는 전체 inventory를 반복 재생하는 비용을 줄이는 것**이다. 변경량 Δ, 활성 거래·dirty page·필수 소유 metadata와 전체 inventory 크기 T/I의 비용을 분리하고 증분 경로의 유지·재구성·상각 비용까지 평가해야 한다. 모든 일을 Δ에만 비례하도록 요구하거나 타 DBMS의 checkpoint 전체 비용이 buffer 크기 이하라고 일반화하지 않는다. 같은-cut snapshot·pin·복구 증명과 목표 부하의 쓰기 지속 예산을 만족하는 후보를 선택하며, 채택 목표·미달 처리의 확정은 A6 owner의 별도 결정이다.

<a id="follow-up"></a>
## 권고하는 후속 순서

| 시점 | 작업과 판단 자료 |
| --- | --- |
| 다음 A1 상세 설계 | 기존 순서대로 진행. 권한 상태·무효화·reopen·독립 replay 대조와 누적 비용 목표를 구체화 |
| 관련 후보 채택 전 | A2–A4의 대표 업무·규모·격리·durability·장비, 성능·지속 부하의 공간 기준을 정리. 기존 방향 안의 상세화는 D2, 제품 방향·보장·SLA 변경은 D3로 구분. 실제 baseline 측정이 필요하면 유한 범위·예산을 별도로 동결 |
| 해당 형식 동결과 일정 11 전 | A0/A4의 table 충돌과 RC 재시작 의미·목표 최대 거래 크기·row 잠금·실제 writer/reader 진행을 함께 판단. 필요한 의미 결정과 독립 준비를 구분 |
| 대형 profile·A6 알고리즘 선택 전 | profile tuple과 전체 회계, 반복 inventory 비용·WAL pin/admission 차단·full audit 쓰기 중단 예산, 미달 처리와 지원 전환 조건을 연결 |
| DEV8 HA 인수 구체화 | 기존 DEV9 사건 시작 기준과 연결해 읽기·strict 쓰기 복구 probe, 보호 peer 재구성·장애별 가용 범위·자동화 후보를 구체화 |

이번 리뷰의 결론은 **일반화의 구조적 한계를 개선할 경로가 생겼고 다음 설계를 진행할 수 있다**는 것이다. A1만으로 성능·용량·HA 전체를 해결했다고 판단하지 않으며, 위 모든 권고를 A1 착수의 일괄 선행 조건으로 추가하지 않는다. 권고를 채택해 계약·제품 목표·필수 품질 기준을 바꾸는 경우 해당 owner의 D2/D3 결정과 일정 재견적을 따로 남긴다.

[DP01](../policies/development-policy.md#section-2)에 따라 기존 제품 방향 안의 내부 설계·profile 구체화는 D2로 진행할 수 있다. Oracle·PostgreSQL과의 제품 동등성, 사용자 확정 방향·필수 보장의 변경이나 새 고객 SLA는 D3다. 목표 workload를 구체화하는 모든 작업에 사용자 결정을 일괄 요구하거나 “임의 확대 금지”를 검토된 확대 자체의 금지로 해석하지 않는다.

<a id="comment-followup"></a>
## PR #52 댓글 재검토와 반영

입력은 [Claude B1–B6 리뷰](https://github.com/et16kr/xadb/pull/52#pullrequestreview-5772301056)와 [Codex 보완 댓글](https://github.com/et16kr/xadb/pull/52#issuecomment-6636759421)이다. 댓글의 원문은 당시 주장으로 보존하고, 이번 문서에는 아래와 같이 범위를 가려 반영했다. 별도로 추가된 [Claude 구조 리뷰](dev3-improved-plan-structural-review-20261104.md)는 그 저자의 당시 검토 기록이며 이 표가 그 전체를 승인한 것은 아니다.

| 의견 | 반영과 제한 |
| --- | --- |
| B1 고정 profile | §4에 buffer/WAL/FD와 tuple 검토를 명시. buffer 초과 업무 전체가 I/O 병목이라는 단정은 제외하고 기존 A7의 64GiB 준비를 인정 |
| B2 checkpoint·audit | §5에 WAL pin/admission 차단과 full audit 쓰기 중단을 연결. bytes/s와 TPS, 조건부 비용식과 실측을 구별하고 전체 작업의 Δ-only 목표는 미채택 |
| B3 거래 크기·잠금 | §4에 목표 거래 크기와 기존 A4 연결을 보강. 256 한도의 원인을 잠금 표 하나로 설명하거나 page 소유만으로 상한이 사라진다고 하지 않음 |
| B4 RC | §1에 같은 table의 보수 충돌과 같은 행 observation mismatch를 구분. RC 변경·statement restart의 효과를 workload 없이 단정하지 않음 |
| B5 HA 후보 | §3에 FIRST/ANY·standby 읽기 비교 후보와 보호 설정을 보강. 유일 해법·필수 기능으로 채택하지 않음 |
| B6 목표 결정 | 후속 순서와 DP01에 기존 방향 내 D2·제품 약속 변경 D3를 구분. 사용자 결정의 일괄 선행 gate를 추가하지 않음 |
| Codex 1–3 | §3에 사건 발생부터 strict commit까지의 복구와 DEV9 연결, §2·§5에 지속 부하의 생성/반환·차단/정상화 기준을 보강 |

외부 비교는 본문의 PostgreSQL 18 공식 문서, Oracle 19c Data Guard 문서와 Ask TOM의 UPDATE 예제를 사용한 정적 대조다. 외부 제품 실행이나 현재 XADB의 성능 입증이 아니며 제품·모델·성능 시험은 새로 실행하지 않았다.

기존 V1 적립 33/86일=38.37%와 PR #49까지의 제한 공급 증거는 유지한다. 이 기록은 제품·모델·성능 시험을 새로 실행한 결과가 아니며, 이 문서 PR의 pre-push 정적·문서 검사 결과는 PR에 별도로 기록한다.

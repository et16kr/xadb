# XADB 제품 방향

2026-10-18 · **XADB-SCOPE-001 / 사용자 채택 / 구현과 제품 인수는 별도**

2026-11-07 [XADB-SCOPE-002](#scope-002)에서 사용자가 규모·거래 크기·가용성의 목표 등급을 **Oracle·PostgreSQL 단일 인스턴스급**으로 정했다. 아래 SCOPE-001의 "합리적인 성능"은 그 목표로 구체화한다. SCOPE-001의 실험 주제와 정확성 우선 원칙은 그대로다.

## 목적

XADB의 주제는 **공개 오픈소스 DBMS를 참고해 AI 친화적인 DBMS를 AI agent로 개발할 수 있는가**를 시험하는 것이다. 현재는 초기 단계이며, 최종 목표는 기본을 지키는 독립 관계형 DBMS다. 기본 기능과 정확성, 합리적인 성능을 갖추고 내부 구조를 이해·수정·검증할 수 있어야 한다.

## 저장과 SQL

제품 저장 엔진은 DISK 하나다. 페이지·버퍼·인덱스·WAL·checkpoint·복구와 RAM보다 큰 데이터 처리를 개발한다. 버퍼 캐시·쿼리 작업 메모리·snapshot pin은 명시된 자원 한도 안에서 관리한다.

SQL은 넓은 관계형 기능을 지향한다. 기존에 논의한 Oracle 계열 SQL 사용성은 향후 명시적 호환 항목을 검토할 입력으로 남긴다. 현재 Oracle·PostgreSQL·MariaDB dialect 전체 호환은 채택하거나 구현한 상태가 아니다. DEV4 최소 SQL부터 문법·타입·NULL·오류·transaction 의미와 독립 기대값을 동결하고 DEV7에서 확장한다. SQL 엔진은 XADB의 Rust 구현이며 공개 DBMS를 비교·연구한다.

## 참고 자료와 설계 기준

[공개 참고 자료](../reference/opensource-references.md)에 PostgreSQL·MariaDB·SQLite 등의 공식 문서와 저장소를 연결한다. 참고 자료를 열람한 사실, 설계를 채택한 사실, 코드를 작성·검증한 사실을 구분한다. 기존 작업에서 이관한 설계가 이들 구현에서 이미 도출되었다고 소급 주장하지 않는다. 코드 도입 시 실제 출처와 버전·라이선스·notice를 보존하는 DP04를 적용한다.

정확성·실패 원자성·복구·자원 상한을 먼저 만족시킨다. 기본적인 자료구조와 최적화를 갖추되 추가 캐시·lock-free·다중 로그 스트림·특정 CPU 튜닝은 측정된 필요와 검증 비용을 근거로 선택한다. AI 친화성은 작은 문맥에서 이해할 수 있는 모듈, 명시적 계약, 독립 기대값, 재현 가능한 시험, 실패 원인을 추적할 수 있는 증거로 구체화한다. 사람이 방향과 계약을 검토하고 agent가 구현·시험·수정을 반복한다. 계약 충족률, 회귀·결함 발견, 수정에 필요한 문맥과 반복 횟수를 기록해 이 방법의 가능성과 한계를 평가한다. agent가 작성했다는 사실만으로 품질을 인정하지 않는다.

## 공개와 완성도

최종 목표와 개발 소스를 초기 단계부터 공개한다. 현재 초기 구현이라는 상태와 앞으로 완성할 기능을 함께 표시한다. 공개 저장소는 개발 실험의 진행 상황을 보여 준다. 재현 가능한 빌드·시험과 실제 지원 범위·제약을 공개 문서에 유지한다. 저장소 공개 상태는 Public이며 프로젝트 라이선스는 아직 선택하지 않았다. 외부 코드를 도입할 때는 해당 출처·라이선스·notice를 유지한다. DEV9는 선언한 제품 기능·운영·성능의 인수 단계다.

## 이관과 남은 단계

현재 구현한 DEV0·DEV1과 DEV2 P0/K0의 코드·시험을 유지한다. DEV2 K1–K5의 영속성 작업과 DEV3 디스크 저장·인덱스를 거쳐 SQL·서버·클라이언트로 진행한다. 기존 저장소는 수정하지 않고 별도 Git 이력으로 시작한다. [이관 기록](../history/project-migration.md)과 [개발 단계](development-stages.md)를 따른다.

<a id="scope-002"></a>
## XADB-SCOPE-002 — 목표 등급: Oracle·PostgreSQL 단일 인스턴스급

2026-11-07 · **D3 사용자 결정 / 목표 등급과 설계 목표는 이 문서 PR의 병합으로 채택 / 인수 기준 수치는 초안이며 DEV9 F06·DEV8 F10에서 후속 동결 / 계약·형식·구현·일정 변경은 각 owner의 후속 작업**

project thread의 결정 카드 "XADB 최종 목표 등급을 어느 수준으로 정할까요?"에서 사용자가 **"Oracle·PG 단일 인스턴스급"**을 골랐다. 카드에 적힌 결과는 "규모, 거래 크기, HA 목표를 지금 확정하고 계획 전체를 다시 설계합니다. 범위가 크게 늘어납니다."였다. 다른 선택지는 "현재 방향 유지"와 "단계형: 구조는 PG급 대비"였다. 입력은 [대용량·고가용성 검토](../evidence/dev3-scalability-ha-plan-review-20261104.md)와 [구조 검토의 사용자 결정 U1](../evidence/dev3-improved-plan-structural-review-20261104.md#order)이다. 이 기록은 결정과 목표를 정하며, 목표를 만족하는 설계를 승인하거나 현재 구현의 성능을 입증한 기록이 아니다.

### 범위

"단일 인스턴스급"은 primary instance 하나가 모든 쓰기를 처리하고 물리 standby가 보호·전환·읽기를 맡는 구성에서, Oracle Database(단일 instance와 Data Guard)·PostgreSQL(streaming replication)과 같은 등급의 규모·거래 크기·동시성·가용성을 갖춘다는 뜻이다. 다음은 이 결정에 들어 있지 않다.

- 여러 instance의 공유 디스크(RAC)·sharding·분산 query. [DEV8 M1 multi-writer](../contracts/dev8-operations-design.md#topology)는 기존대로 별도 leaf다.
- SQL dialect·기능의 전체 호환. 호환 항목은 기존처럼 DEV4/DEV7에서 정한다.
- 거래 의미의 선택(기본 격리, RC 충돌 처리, Serializable 충돌 단위, deadlock, cursor). 아래 U2로 남긴다.

SCOPE-001의 실험 주제, 정확성·실패 원자성·복구·자원 상한을 먼저 만족시키는 순서, 측정에 근거한 최적화 선택은 유지한다. 자원 상한은 계속 명시하고 증명한다. 달라지는 것은 상한이 고정된 작은 tuple이 아니라 설정값과 장비에 따라 커지는 형태가 된다는 점이다.

### 목표

**설계 목표**는 형식·주소·알고리즘이 막아서는 안 되는 규모와 비용 구조다. 해당 leaf가 설계 목표를 만족하는 후보를 찾지 못하면 [A1 비용 목표](../contracts/dev3-task-04-wal.md#a1-cost-target)와 같이 목표 미달·미완료로 남기고, 목표 완화는 사용자 결정으로 올린다. **인수 기준**은 실제 장비로 시험할 값의 초안이다. 장비·disk·network가 정해질 때 [DEV9 F06](../contracts/dev9-qualification-design.md#performance)과 [DEV8 F10](dev8-implementation-plan.md#freeze)에서 동결한다. 필요한 장비가 없으면 NOT_RUN으로 남기며 작은 결과로 대체하지 않는다.

| 항목 | 설계 목표 | 인수 기준(초안) | 참고 |
| --- | --- | --- | --- |
| 데이터 규모 | 단일 DB 64 TiB 이상을 형식·주소로 표현한다. 파일·extent·catalog 항목 수가 DB 크기 확장을 막지 않는다 | 1 TiB 이상. dataset은 buffer의 10배 이상이며 사용 가능한 RAM의 1.5배 이상 | 두 제품 모두 단일 DB 수십 TB를 운영한다 |
| 메모리·WAL | buffer·작업 메모리·WAL 보존량은 RAM·disk에 맞춰 설정한다. 형식은 최솟값과 표현 상한만 정한다 | buffer 32 GiB 이상 설정에서 동작과 성능 측정 | PostgreSQL `shared_buffers`·`max_wal_size`, Oracle SGA·redo log |
| 거래 크기 | 한 거래의 변경 행 수와 논리 변경량을 메모리가 아니라 Undo·WAL의 disk 공간으로만 제한한다. 잠금·write-set·Undo·WAL·rollback 예약 어느 것도 메모리 상한에 묶이지 않는다 | 단일 거래로 1천만 행 UPDATE, 그 rollback과 crash 뒤 복구 | 두 제품 모두 거래당 행 수 상한이 없다 |
| 동시성 | 서로 다른 행을 쓰는 writer의 준비·적용이 전역 독점으로 직렬화되지 않는다. 무관한 writer의 준비·I/O·sync 동안 reader가 전역 gate로 막히지 않는다. 같은 page의 짧은 content latch 대기는 구분하되, latch를 쥔 채 I/O·sync·거래 대기는 하지 않는다 | 동시 연결 1,000·동시 활성 거래 1,000 설정에서 정확성과 진행. reader 대기를 같은 page와 다른 page로 나눠 원인·시간을 계측한다 | 행 단위 잠금, MVCC. [A4 구조 목표와 계측 경계](../evidence/dev3-improved-plan-structural-review-20261104.md#structural-targets) |
| 비용 구조 | 정상 연산·commit·checkpoint의 일이 WAL 길이나 DB 크기에 비례하지 않는다. 정상 redo는 바뀐 bytes와 고정 header에 비례한다. 여러 거래의 sync를 묶는다 | 작은 계수 사례([D3-ADOPT](dev3-test-plan.md#review-adoption-20261104))로 비례 관계 확인 | [A1·A2·A3·A6 구조 목표](../evidence/dev3-improved-plan-structural-review-20261104.md#structural-targets) |
| 성능 | 같은 장비, 같은 durability(fsync와 동기 commit), 같은 격리의 PostgreSQL과 비교한다 | 단일 행 OLTP(TPC-B류)·읽기 위주·같은 행 경합 workload에서 처리량은 PostgreSQL의 50% 이상, p99 지연은 2배 이내 | 절대 TPS는 장비에 달려 있어 비율로 정한다 |
| 가용성: 복제 | strict RPO=0. 보호 standby 둘 이상의 quorum으로 standby 한 대가 장애여도 strict 쓰기를 이어 간다 | 장애 셀별 RPO=0과 쓰기 지속 확인 | PostgreSQL `synchronous_standby_names`의 `ANY`, Oracle의 여러 SYNC 목적지 |
| 가용성: 전환 | 감시자(witness)와 검증 가능한 fencing을 둔 자동 장애 전환. standby에서 읽기 query를 받는다 | primary 장애 발생(감지 포함)부터 strict 쓰기 재개까지 60초 이내 | Oracle Fast-Start Failover·Active Data Guard, PostgreSQL hot standby |
| 가용성: 운영 | checkpoint·backup·무결성 검사·인덱스 build·보존 압력·dirty backlog가 DB 전체 쓰기를 멈추지 않는다(짧은 metadata 구간 제외). 멈춤이 남으면 그 범위와 최대 시간을 명시한다 | 각 작업 중 쓰기 지속과 최대 정지 시간 측정 | online backup, `amcheck`, `CREATE INDEX CONCURRENTLY`·ONLINE |

### 남은 사용자 결정

| # | 결정 | 입력 | 시점 |
| --- | --- | --- | --- |
| U2 | 거래 의미: 기본 격리, RC에서 같은 행 충돌의 처리(거래 전체 abort·문장 재시작·행 재평가), Serializable 충돌 단위, deadlock 처리 | Oracle·PostgreSQL 모두 RC가 기본이고, 같은 행 충돌을 거래 전체 abort 없이 처리한다. 현재 계약은 Serializable 기본이다([A0](dev3-implementation-plan.md#review-adoption-20261104)) | 일정 11 착수 전. 앞선 행 형식에 영향이 있으면 그 동결 전 |
| U4 | 긴 reader 정책 | 위 "가용성: 운영"에 따라 보존 압력이 DB 전체 쓰기를 멈추면 안 된다. 남은 선택은 긴 reader를 실패시키는 방식(Oracle snapshot too old)과 disk 증가를 허용하는 방식(PostgreSQL) 사이다 | 일정 11 착수 전 |
| U6 | DEV3 V2 재견적 승인 | V1(86일)은 고정 작은 profile을 전제로 했다. 시점은 [구조 검토](../evidence/dev3-improved-plan-structural-review-20261104.md#order)대로 A2 형식 결정 직후로 둔다 | A2 형식 결정 직후 |

구조 검토의 U1은 이 결정으로 닫는다. U3(목표 최대 거래 크기)은 위 "거래 크기"로, U5(가용성 목표)는 위 세 가용성 행으로 정한다. 잠금 위치 자체는 그 목표를 만족하도록 A4 owner가 D2로 비교·결정한다.

### 기존 계획에 미치는 영향

| 대상 | 영향 |
| --- | --- |
| 자원·control profile | [ID1/2/3](../contracts/dev3-control-format.md)(buffer 4/16/64 MiB, owned 8 GiB, WAL 256 MiB, 최대 32거래, 거래당 256 mutation·1 MiB)은 현재 지원 profile로 유지한다. "임의 확대하지 않는다"는 검증 없는 확대의 금지로 읽는다. 위 목표에 맞는 설정형 profile은 owner가 D2로 새로 설계한다 |
| A0–A7 | 위 비용 구조·동시성·거래 크기 목표를 A1과 같은 완료 조건으로 적용한다. A4 잠금 위치와 A2 형식은 거래 크기 목표를 만족해야 한다 |
| buffer manager | 고정 frame 수와 전역 registry([구조 검토 3.2](../evidence/dev3-improved-plan-structural-review-20261104.md#buffer-manager))를 설정형 frame 수와 분할 lookup으로 바꾸는 설계가 필요하다 |
| 쓰기 정지 경로 | [구조 검토 3.3](../evidence/dev3-improved-plan-structural-review-20261104.md#write-stops)의 일곱 경로를 "가용성: 운영" 목표로 판정한다 |
| DEV8 | H1의 단일 보호 peer·운영자 시작 전환·recovery-only standby는 중간 단계다. 목표는 quorum 보호·자동 전환·standby 읽기다 |
| DEV9 | 성능 목표를 위 PostgreSQL 비교 비율로 받는다 |
| 일정 | [V1](dev3-schedule-baseline.md)의 적립 35/86일 = 40.70%는 V1 범위의 진척이며 이 목표 대비 진척이 아니다 |

이 문서는 방향 결정만 기록한다. 각 계약·형식·일정의 변경은 해당 owner의 D2 기록과 독립 검토로 반영하며, 이번에 제품 코드·시험·일정 적립은 바꾸지 않았다.

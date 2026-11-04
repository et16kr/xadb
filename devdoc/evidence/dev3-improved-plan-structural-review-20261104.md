# DEV3 개선 계획 리뷰: 구조 목표·단일 노드 가용성·HA 연결

2026-11-04 · **Claude 정적 리뷰 / D0 기록 추가 / 계약·일정·제품 코드 변경 없음 / 제품 시험 NOT_RUN**

[개선 계획 A0–A7](../plans/dev3-implementation-plan.md#review-adoption-20261104) · [A1 비용 목표](../contracts/dev3-task-04-wal.md#a1-cost-target) · [PR #52 Codex 검토](dev3-scalability-ha-plan-review-20261104.md) · [PR #48 Claude 방향 리뷰](dev3-oracle-direction-review-20261103.md) · [PR #50 비교 재검토](dev3-cross-dbms-design-review-20261104.md) · [DEV8 운영 설계](../contracts/dev8-operations-design.md) · [일정 기준선](../plans/dev3-schedule-baseline.md#review-adoption-20261104)

사용자 요청(2026-11-04, 프로젝트 thread): "앞서 리뷰에서 현재 대로라면 오라클이나 포스트그래스 정도의 대용량 고성능 고가용성 디비는 무리라고 했고 그래서 개선을 했습니다. 개선된 계획서를 리뷰해 주세요, 그 리뷰 결과를 지금 달린 pr에 추가 커밋으로 올려줘요."

검토 대상은 PR #51 병합 main `8a4f8e4a6fee6b5498145d9ee9b17a7105e1c851`의 DEV3-REVIEW-012(초안 `d8b4125`와 PR #51 리뷰 반영 `fe380e2`)다. 같은 PR에 있는 [Codex 검토](dev3-scalability-ha-plan-review-20261104.md)와 겹치지 않는 부분을 중심으로 적는다. 코드 근거는 같은 commit에서 직접 읽었다.

후속 참조: PR #52 댓글의 B1–B6와 Codex 보완 의견을 대조한 [댓글별 반영 판단](dev3-scalability-ha-plan-review-20261104.md#comment-followup)이 추가됐다. 아래 본문은 `8dad2914`에 수록된 Claude의 당시 검토 기록으로 보존하며, 후속 판단이 이 문서 전체의 승인이나 계약·제품 목표의 채택을 뜻하지 않는다.

<a id="summary"></a>
## 1. 결론

**개선된 계획은 방향이 맞다.** 이전 리뷰들이 지적한 live WAL 재검증, redo 형식, 그룹별 영속 대기, 유일 writer와 read gate, 거래 의미, 행 잠금 위치가 모두 작업(A0–A7)과 owner 계약으로 올라갔다. A1에는 "매 연산이 과거 WAL 전체 길이에 비례하는 일을 하지 않는다"는 통과 기준과 미달 처리까지 생겼다. PR #51에 남긴 리뷰 의견 다섯 개도 `fe380e2`에서 모두 반영됐다([2절](#resolved)).

**그러나 이 계획만으로 Oracle·PostgreSQL급 대용량·고성능·고가용성에 도달한다고 판단하기는 아직 이르다.** 이유는 다섯 가지다.

| # | 남은 문제 | 왜 중요한가 | 권고 |
| --- | --- | --- | --- |
| 1 | 통과 기준이 A1 하나뿐 | A2–A4는 "비교했다"로 끝날 수 있다. 안전한 후보를 고르고도 구조 병목이 남을 수 있다 | A1과 같은 형태의 **구조 목표**를 A2·A3·A4에 둔다([3.1](#structural-targets)) |
| 2 | buffer manager 확장이 A0–A7에 없음 | 현재 pool은 64 frame, 전역 registry Mutex 하나, 선형 lookup, load claim 하나다. 대용량의 기본 경로다 | A4 직렬 구간 표에 넣고 일정 12-2를 재견적한다([3.2](#buffer-manager)) |
| 3 | 단일 노드 안에서 DB 전체 쓰기를 멈추는 경로가 여럿 | 긴 reader, 인덱스 build, full audit, WAL 보존 압력이 모두 DB 전체 새 쓰기를 막는다. 장애 전환 RTO만으로는 가용성을 설명할 수 없다 | 쓰기 정지 경로 inventory와 축소 계획을 만든다([3.3](#write-stops)) |
| 4 | A2 redo 크기와 DEV8 HA의 연결이 없음 | 거래당 WAL bytes가 복제 대역폭, standby 지연 시 쓰기 정지까지의 여유, 승격 catch-up, logical supplement 형식을 정한다 | A2의 입력에 DEV8 §5·§6·§9를 넣는다([3.4](#a2-ha)) |
| 5 | 제품 목표가 문서에 없음 | [제품 방향](../plans/project-direction.md)은 "합리적인 성능"이다. 사용자는 두 번 Oracle·PostgreSQL급을 목표로 말했다 | 목표 등급을 사용자 결정으로 기록한다([3.5](#product-goal)) |

다섯 권고 모두 A1 상세 설계 착수의 선행 조건이 아니다. 결정 시점은 [5절](#order)에 정리했다.

<a id="resolved"></a>
## 2. 이전 지적의 반영 확인

| 이전 지적 | 반영 위치 | 판정 |
| --- | --- | --- |
| live 경로가 매 그룹 WAL 전체를 재검증(PR #48 5.2, PR #50 3.1) | A1, [A1 비용 목표](../contracts/dev3-task-04-wal.md#a1-cost-target) | 반영. 목표 미달이면 미완료, 목표 완화는 사용자 결정 |
| page 통째 redo, page당 연속 구간 하나(PR #48 5.1, PR #50 3.2) | A2 | 반영. 다만 목표 없음(3.1) |
| 그룹마다 `wait_durable`(PR #50 3.7) | A3 | 반영. 다만 목표 없음(3.1) |
| 유일 `MutationPort`, Row write의 read gate(PR #50 3.3) | A4 | 반영. 다만 목표 없음(3.1) |
| PR #51 의견 1: A0가 사용자 결정 항목을 확정 | A0를 "현재 작업 기준·호환 결정"으로 바꾸고 결정 시점을 일정 11 구현 전으로 둠 | 반영 |
| PR #51 의견 2: 행 잠금 위치 판정 누락 | "R6 중 행 잠금 위치" 행, A4 비교 후보 | 반영 |
| PR #51 의견 3: ITL/CTS·column delta 표현 불일치 | A2 비교 후보로 통일 | 반영 |
| PR #51 의견 4: A1 "현 방식 유지" 탈출구 | A1 비용 목표와 미달 처리 | 반영 |
| PR #51 의견 5: A1 설계 비적립 | "일정 항목 밖, 적립 없음"과 보고 형식 | 반영 |

<a id="findings"></a>
## 3. 남은 구조 문제

<a id="structural-targets"></a>
### 3.1 A2·A3·A4에도 A1 같은 구조 목표가 필요하다

A1 목표는 벤치마크 숫자가 아니라 **비용이 무엇에 비례하는가**를 정한다. 그래서 장비나 workload 없이 지금 정할 수 있고, D3-ADOPT의 작은 계수 사례로 통과 여부를 판정할 수 있다. A2–A4는 대안 비교와 안전성 증명만 요구하므로, 안전한 후보를 골랐는데 구조 병목이 그대로 남는 결과도 "완료"가 될 수 있다. [PR #52 2절](dev3-scalability-ha-plan-review-20261104.md#performance-targets)은 workload별 숫자 기준을 권고했다. 숫자 기준은 측정 환경이 필요하므로, 그 앞에 아래 구조 목표를 먼저 두기를 권한다.

| 작업 | 지금 비용(근거) | 제안하는 구조 목표 | 판정 사례 |
| --- | --- | --- | --- |
| A2 redo | Forward redo가 page 5개 × 16,344 B = 81,720 B([operation.rs](../../crates/engine/src/storage/semantic_row/operation.rs) `shape`). 한 행 INSERT 거래와 정리가 WAL을 629,363 B 늘림([M0](f6-wal-revalidation-survey-20261102.md#f6a-m0)) | 정상 변경의 redo bytes가 **page 크기가 아니라 실제로 바뀐 bytes와 고정 header에 비례**한다. torn page 보호용 전체 image가 필요하면 그 조건(예: checkpoint 뒤 첫 변경)을 명시한 경우로 한정한다 | D3-ADOPT-02: 같은 연산에서 행 크기만 바꿔 redo bytes의 기울기와 상수를 확인 |
| A3 sync | 설치 뒤 `wait_durable`([catalog_lifecycle.rs](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_lifecycle.rs) `apply_existing`), WAL slot 하나([physical.rs](../../crates/engine/src/storage/wal_io/physical.rs)). 한 행 거래에 영속 대기 경계가 Begin·Reserve·Forward·Commit 4개와 정리 6개([PR #50 3.7](dev3-cross-dbms-design-review-20261104.md#durability-boundaries)) | 한 거래의 사용자 대기 sync가 **거래 안 그룹 수에 비례하지 않는다**(기본은 Commit 한 번). 여러 거래의 Commit이 sync 하나를 공유할 수 있다 | D3-ADOPT-03: 그룹 수만 다른 거래들의 sync 호출 수, 동시 Commit의 sync 공유 수 |
| A4 writer·reader | `MutationPort`가 `&mut RetentionLedger`·`&mut operation`을 독점([plan4_apply.rs](../../crates/engine/src/storage/wal_io/plan4_apply.rs)). Row write가 `close_reads_for_terminal`로 새 읽기를 막음([row/write.rs](../../crates/engine/src/storage/wal_io/plan4_apply/row/write.rs)) | 서로 다른 table, 같은 table의 다른 행을 쓰는 writer의 준비·적용이 **전역 독점 대여 하나로 직렬화되지 않는다**. Row write가 관계없는 행의 새 읽기를 막지 않는다 | D3-ADOPT-04: 독립 writer 2/8/32의 겹친 구간, 무관한 reader의 대기 0 |

처리 규칙은 A1과 같게 둔다. 안전한 후보가 목표를 만족하지 못하면 미달·미완료로 남기고, 목표를 완화하려면 사용자 결정으로 올린다. 이 목표들은 성능 합격 수치가 아니며 PF09나 DEV9의 숫자 기준을 대신하지 않는다.

<a id="buffer-manager"></a>
### 3.2 buffer manager 확장이 A0–A7에 없다

[3.05 계약](../contracts/dev3-task-05-buffer-flush.md#resources)은 buffer 4/16/64 MiB에 256/1,024/4,096 frame과 clock 교체를 정했다. 현재 구현은 첫 profile 크기다.

| 현재 구현 | 위치 |
| --- | --- |
| `MAX_FRAMES = 64` | [dirty_frame.rs](../../crates/engine/src/storage/dirty_frame.rs) |
| pool 전체의 pin 수·directory·DPT·WAL 요구가 `Mutex<PoolRegistry>` 하나에 있음 | 같은 파일 `SharedPool`, `PoolRegistry` |
| 주소 lookup이 `[Slot; MAX_FRAMES]`를 순회 | [registration.rs](../../crates/engine/src/storage/dirty_frame/registration.rs) `RegistrationDirectory` |
| clean victim load claim이 pool에 하나 | [reader_load.rs](../../crates/engine/src/storage/dirty_frame/reader_load.rs) |

frame마다 내용 `RwLock`이 있으므로(`DirtyFrame`, [clean buffer](../contracts/dev3-read-buffer.md)의 `ReadFrame`) page 단위 latch는 이미 있다. 문제는 그 위의 전역 registry다. 4,096 frame이나 그 이상에서 모든 pin·lookup·load·flush가 이 Mutex 하나를 지나고, 서로 다른 page의 load가 동시에 진행되지 못하면 RAM보다 큰 데이터의 읽기 처리량은 writer 병렬화와 무관하게 막힌다. Oracle의 buffer cache와 PostgreSQL의 shared buffers는 hash 분할 lookup과 page별 I/O 진행을 기본으로 한다.

A4의 [소유 경계 표](../contracts/dev3-task-10-concurrency.md#review-adoption-20261104)에는 `MutationPort`, read gate, Root/Job/Status, WAL slot, 설치 장벽이 있지만 buffer registry는 없다. 일정 [12-2](../plans/dev3-schedule-baseline.md#items) "일반 INIT/load/eviction과 압력하 진행"은 2일이다.

권고: buffer registry·lookup·load를 A4의 직렬 구간 표에 추가하고, 구조 목표를 "lookup 비용이 frame 수에 비례하지 않음, 서로 다른 page의 load·flush가 registry 잠금을 I/O 동안 쥐지 않고 동시에 진행"으로 둔다. 12-2의 2일은 이 범위를 기준으로 재견적한다.

<a id="write-stops"></a>
### 3.3 단일 노드 안의 DB 전체 쓰기 정지 경로

[PR #52 3절](dev3-scalability-ha-plan-review-20261104.md#availability)은 장애 전환의 전체 중단 시간을 다룬다. 고가용성에는 장애가 없을 때 운영 작업이나 부하 때문에 쓰기가 멈추는 시간도 포함된다. 현재 계약에는 DB 전체의 새 쓰기를 멈추는 경로가 넷 있다. 각각은 안전을 위한 타당한 첫 선택이지만, 어느 계획도 이것을 가용성 비용으로 추적하지 않는다.

| 경로 | 현재 계약 | 멈추는 범위 | Oracle·PostgreSQL |
| --- | --- | --- | --- |
| 긴 reader의 보존 압력 | snapshot을 시간으로 만료하지 않고 다음 statement 첫 effect 전에 자원 부족 반환. "전체 DB 쓰기 가용성을 약속하지 않는다"([보존 압력](../contracts/disk-storage-design.md#retention-pressure)) | DB 전체 새 쓰기 | Oracle 기본은 undo를 재사용하고 긴 query가 snapshot too old로 실패한다. PostgreSQL은 공간이 늘고 쓰기는 계속된다 |
| 인덱스 build | 새 거래·maintenance admission을 닫고 기존 거래를 drain([3.07](../contracts/dev3-task-07-indexes.md) 5절) | DB 전체 | 두 제품 모두 그 table의 쓰기만 막고, ONLINE·CONCURRENTLY는 쓰기를 막지 않는다 |
| full integrity check | admission drain한 고정 상태에서 전체 순회([설계 정본](../contracts/disk-storage-design.md#recovery-rebase)), audit 시작 전 mutation 중지 소유([3.15](../contracts/dev3-task-15-backup-integrity.md) 3절) | DB 전체, audit 시간 동안 | 운영 중 검사 도구를 둔다(PostgreSQL amcheck 등) |
| WAL 보존 압력 | 보존 WAL 256 MiB([자원 profile](../contracts/disk-storage-design.md#budgets)). DEV8의 peer·capture pin이 floor를 잡으면 압력 시 새 write admission 거절([DEV8 §9](../contracts/dev8-operations-design.md#retention)) | DB 전체 새 쓰기 | PostgreSQL은 `max_slot_wal_keep_size`로 slot을 무효화하고 primary 쓰기를 이어 가는 선택을 둔다 |

full audit은 [PR #52 5절](dev3-scalability-ha-plan-review-20261104.md#maintenance)의 `E × ceil(T/B)` 비용이 그대로 **쓰기 정지 시간**이 된다는 점에서 성능 문제이자 가용성 문제다.

권고: 이 네 경로(와 DEV8 strict 보호 peer 상실)를 "쓰기 정지 경로 inventory"로 모으고, 경로마다 계기·범위·예상 지속 시간·해제 조건·축소 계획을 적는다. 축소 후보는 이미 일부 있다. 인덱스 build는 A5의 "table 단위 build 잠금" 후보를 일반 인덱스(일정 09) 전에 정하고, 긴 reader 정책은 A0의 "snapshot 강제 만료 금지"를 reader 보호와 writer 가용성 사이의 **사용자 정책**으로 다룬다. 이 inventory를 DEV8 HA 가용성 목표의 입력으로 연결한다.

<a id="a2-ha"></a>
### 3.4 A2는 성능 항목이면서 HA 항목이다

DEV8 H1은 physical WAL을 standby로 보낸다([DEV8 §4](../contracts/dev8-operations-design.md#topology)). 그래서 거래당 WAL bytes는 로컬 성능뿐 아니라 HA의 여러 수치를 직접 정한다. 아래는 M0의 한 행 INSERT 거래·정리 629,363 B로 한 **산술**이며 측정값이 아니다.

| HA 수치 | 계산 | 의미 |
| --- | --- | --- |
| standby·capture가 WAL을 붙잡을 때 쓰기 정지까지의 여유 | 268,435,456 B ÷ 629,363 B ≈ 426 거래 | 보존 quota가 그대로면 한 행 거래 약 426개 뒤 새 쓰기 거절. 여유 시간 = quota ÷ 거래당 WAL ÷ TPS |
| 복제 대역폭 | 1,000 TPS × 629,363 B ≈ 629 MB/s(약 5 Gbit/s) | 동기 standby의 Commit 대기와 망 비용 |
| 승격 catch-up·backup·archive | WAL bytes에 비례 | RTO와 보존 공간 |

또 DEV8 §6은 logical capture에 필요한 값을 **같은 WAL의 같은 원자 mini-transaction**에 supplement로 기록하도록 정했다([DEV8 §6](../contracts/dev8-operations-design.md#capture)). A2가 이 자리를 고려하지 않고 형식을 정하면 DEV8에서 형식을 다시 열어야 한다.

권고: A2 비교표의 입력에 DEV8 §5(remote ACK)·§6(supplement)·§9(보존 압력)를 넣고, 후보마다 위 세 수치를 함께 계산한다. 이렇게 하면 3.1의 A2 구조 목표가 HA 목표와 같은 근거를 갖는다.

<a id="product-goal"></a>
### 3.5 제품 목표를 사용자 결정으로 기록해야 한다

[제품 방향](../plans/project-direction.md)의 최종 목표는 "기본을 지키는 독립 관계형 DBMS"이고 성능은 "합리적인 성능"이다. PR #50·#51·#52는 모두 이 문서를 근거로 Oracle·PostgreSQL과의 전체 동등성을 약속하지 않는다고 적었다. 반면 사용자는 PR #48 요청에서 "목표는 오라클", 이번 요청에서 "오라클이나 포스트그래스 정도의 대용량 고성능 고가용성"을 목표로 말했다.

이 차이가 문서에 남아 있는 한 A0의 거래 의미 결정과 A2–A4의 후보 선택에는 판단 기준이 없다. [PR #52 2절·4절](dev3-scalability-ha-plan-review-20261104.md#capacity)의 성능 기준·목표 용량 권고에 동의하며, 그 출발점은 담당이 아니라 사용자가 정할 제품 목표다. 아래 항목을 사용자가 정하면 담당이 각 작업의 기준으로 옮길 수 있다. 숫자는 이 리뷰가 정하지 않는다.

| 정할 항목 | 예시 선택지 |
| --- | --- |
| 목표 등급 | 학습용 기본 DBMS / 단일 서버 OLTP 운영 수준 / Oracle·PostgreSQL 단일 인스턴스 수준 |
| 대표 workload | 단일 행 OLTP, 다중 행 DML, 긴 reader 공존, batch |
| 규모 단계 | 데이터 크기·동시 세션·거래 크기의 단계와 각 단계 진입 조건 |
| 내구성·가용성 | local durable / 동기 standby, RPO·RTO(장애 감지 포함), 계획 작업 중 DB 전체 쓰기 정지 허용 여부 |
| 거래 의미 기준 | Oracle식(RC 기본, statement restart, statement 단위 deadlock rollback) / 현재 계약 유지 |

<a id="progress"></a>
### 3.6 진척률 38.37%의 해석

V1의 86일 분모는 기존 구조를 넓히는 전제로 배정됐다([PR #48 9절](dev3-oracle-direction-review-20261103.md#schedule-impact)). A1–A7의 설계·변경, buffer 확장, 쓰기 정지 경로 축소는 대부분 분모 밖이다. 따라서 38.37%는 "V1 범위의 진척"이며 Oracle·PostgreSQL급 목표 대비 진척이 아니다. 계획도 이 점을 [재견적 경계](../plans/dev3-schedule-baseline.md#review-adoption-20261104)로 밝히고 있다.

PR #51은 영향받는 항목마다 재분할이나 V2를 하도록 정했다. A2의 형식 결정은 일정 08·09·12·14에 한꺼번에 영향을 주므로, 그 결정 직후 DEV3 전체 V2 재견적을 **한 번** 하기를 권한다. 항목마다 분모가 바뀌면 진척률이 PR마다 흔들려 추세를 읽기 어렵다.

<a id="codex-review"></a>
## 4. PR #52 Codex 검토에 대한 의견

| PR #52 권고 | 의견 |
| --- | --- |
| 1 기본 Serializable의 table 단위 충돌 | 동의. 기본 격리가 Serializable이므로 별도 설정이 없는 호출자도 이 규칙을 받는다. 같은 snapshot에서 같은 table을 읽고 쓴 k개 거래가 경쟁하면 서로 다른 행이어도 성공은 많아야 1개다. 결정을 일정 11 전에 닫는 순서가 맞다 |
| 2 A2–A4 성능 판단 기준 | 동의. 숫자 기준 앞에 3.1의 구조 목표를 먼저 두기를 보완한다 |
| 3 장애 전환과 서비스 가용성 | 동의. D8-P04의 RTO 60초가 fence 요청부터라는 지적이 정확하다. 3.3의 단일 노드 쓰기 정지도 가용성 예산에 넣기를 보완한다 |
| 4 목표 용량 | 동의. 목표 자체가 사용자 결정이라는 점(3.5)과 buffer manager(3.2)를 보완한다 |
| 5 checkpoint·audit 반복 I/O | 동의. full audit은 쓰기를 멈춘 상태에서 돌기 때문에 이 비용이 곧 쓰기 정지 시간이다(3.3) |

뼈대(mutable page, 단일 WAL, 영속 Undo, 논리 rollback, redo-only CLR)가 대용량 DBMS를 불가능하게 만들지 않는다는 PR #52의 판단에도 동의한다.

<a id="order"></a>
## 5. 권고하는 결정 시점

| 시점 | 할 일 | 결정자 |
| --- | --- | --- |
| 지금(A1 상세 설계와 병행) | 3.5 제품 목표 기록 | 사용자 |
| A1 상세 설계 PR | 3.1의 A2–A4 구조 목표와 3.2의 buffer registry를 계획 문서에 추가 | 담당, 독립 D2 |
| A2 후보 선택 전 | 3.4의 DEV8 입력과 세 HA 수치 | 담당 |
| 일정 09 착수 전 | 인덱스 build 잠금 범위(3.3) | 담당, 의미 변경이면 사용자 |
| 일정 11 착수 전 | A0 거래 의미, 긴 reader 정책(3.3), Serializable 충돌(PR #52 1절), 행 잠금 위치 | 사용자와 담당 |
| A2 형식 결정 직후 | DEV3 전체 V2 재견적(3.6) | 담당, 승인은 사용자 |

<a id="schedule"></a>
## 6. 진도표 확인

`DEV3 V1 / 2026-11-04·main 8a4f8e4 / 적립 33/86일 = 38.37% / 이번 적립 0(리뷰 문서) / 현재 A1 상세 설계: 착수 전(일정 항목 밖, 적립 없음) / 잔여 ETA 재견적 전`

이 리뷰는 일정 항목을 적립하지 않고 표를 바꾸지 않는다.

<a id="limits"></a>
## 7. 근거와 한계

- 정적 리뷰다. 제품·모델·성능 시험을 실행하지 않았고, 외부 DBMS를 측정하지 않았다.
- 3.4의 값은 M0 계수와 계약 수치로 한 산술이다. INSERT profile 하나의 값이며 UPDATE·DELETE·인덱스가 붙은 경우의 측정이 아니다.
- 코드 근거는 main `8a4f8e4`의 `semantic_row/operation.rs`(`native_mask`, `shape`), `plan4_retention/catalog_lifecycle.rs`(`apply_existing`의 `wait_durable`), `wal_io/physical.rs`(WAL slot 상태), `plan4_apply.rs`(`MutationPort`), `plan4_apply/row/write.rs`(`close_reads_for_terminal`), `storage/dirty_frame.rs`(`MAX_FRAMES`, `SharedPool`, `PoolRegistry`), `dirty_frame/registration.rs`(`RegistrationDirectory`), `dirty_frame/reader_load.rs`다.
- Oracle·PostgreSQL의 동작은 공개 문서 수준의 일반 동작으로 비교했다. 버전별 설정 차이는 PR #50의 [비교 조건](dev3-cross-dbms-design-review-20261104.md#comparison-conditions)을 따른다.
- 이 문서는 Claude의 리뷰 기록이며 독립 D1/D2 승인이 아니다. 권고를 채택해 계약·목표·일정을 바꿀 때는 해당 owner의 결정과 검토를 따로 남긴다.

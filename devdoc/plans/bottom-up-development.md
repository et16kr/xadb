# 하위 계층부터 개발하는 순서

2026-10-18 · **XADB 적용 / DEV2 부분 구현 / 영속 엔진 미구현**

검증 기반 → 공통 의미·effect → SQL 없는 영속 커널 → 디스크 저장·인덱스 → SQL → 서버 → 클라이언트의 실제 통합·인수 순서를 따른다. [D4-PREP-001](../contracts/dev4-independent-frontend.md#scope)의 저장 비의존 SQL 앞단·독립 검증 도구는 leaf 계약 동결 뒤 DEV3와 병행 개발할 수 있다. 실제 저장 통합·Q1 선행은 유지하며 이관으로 기존 구현의 완료 범위를 늘리지 않는다. [단계 JSON](development-stages.json) · [완료 계약](stage-completion.json)

<a id="section-1"></a>
## 1. 순서의 이유

정확성·실패 원자성·복구를 SQL이나 network 없이 검증할 수 있어야 한다. 독립 oracle는 production 알고리즘과 별도이며 실제 thread·파일 I/O·프로세스 재시작을 생략하지 않는다. 하위 기능의 구현·증거가 다음 기능의 입력이다. 설계 채택과 실제 완료는 구분한다.

<a id="section-2"></a>
## 2. K0–K5

| 단계 | 구현할 책임 | 아직 주장할 수 없는 것 |
| --- | --- | --- |
| P0 | 독립 이력 모델과 known-bad history 거절 | production 내구성 |
| K0 | 실제 volatile 행/catalog, snapshot·ID·quota, direct API·native worker | WAL·crash 후 보존 |
| K1 | private transaction batch, read-your-writes, rollback·statement 효과 | durable commit |
| K2 | versioned WAL와 실제 append/sync, 원자 공개·UNKNOWN fencing | 구현되지 않은 recovery의 완료 |
| K3 | complete batch replay·reopen·반복 recovery | 후속 disk page/index 전부 검증 |
| K4 | coarse serializable validation, 실제 concurrency와 statement 실패 | 고성능 병렬 commit |
| K5 | checkpoint·backup/restore·pin/retention·안전한 회수 | 제품 전체 운영 qualification |

K0 배열 구조는 영구 성능 설계가 아니다. 실제 API와 독립 의미 검증을 유지하면서 디스크 표현으로 발전시킨다. 초기 로그는 단일 L0이며 L1/L2 최적화는 측정된 필요가 생긴 뒤 별도 검토한다.

<a id="section-3"></a>
## 3. SQL 이전의 저장 기준선

DEV3에서는 실제 disk page·bounded buffer/locator, row/catalog와 PK·unique·nonunique index, 동일 commit의 crash/reopen·checkpoint/restore를 직접 API로 검증한다. 최소 buffer 4배 working set과 실제 I/O witness가 필요하다. 파티션·global index·모든 확장 타입의 완성을 SQL 착수 조건으로 요구하지 않는다.

<a id="section-4"></a>
## 4. 독립 경로

COMM은 DEV1 다음 frame/transport/backpressure를, PART는 DEV3 다음 disk partition/local index를 구현한다. NG는 PART 뒤 disk global index를 다룬다. DEV5 서버와 연결할 때 실제 request/reply와 SQL·durability 의미를 다시 검증한다.

COMM의 AXF1 stream·Linux local socket·quota·deadline·취소 경계는 [상세 설계](../contracts/comm-design.md), C1/KU12 구현 순서와 인수는 [구현·검증 계획](comm-implementation-plan.md)을 따른다. 설계 작성은 C1 구현·실행 완료가 아니다.

<a id="section-5"></a>
## 5. 진행·완료

[현재 구현](../implementation/README.md)과 [DEV2 계획](dev2-implementation-plan.md)을 따른다. K별 계측·manifest·독립 비교/replay 의무는 유지한다. AI workflow pilot이나 미래 기능 전체 구현을 현재 leaf 착수 조건으로 만들지 않는다. 제품 공개 범위와 안정 qualification은 [제품 방향](project-direction.md)에서 구별한다.

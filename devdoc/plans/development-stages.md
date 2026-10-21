# XADB 개발 단계

2026-10-18 · **디스크 엔진 개발 계획 / 실제 상태는 구현 색인 참조**

[제품 방향](project-direction.md) · [단계 정본](development-stages.json) · [완료 의무](stage-completion.json) · [현재 구현](../implementation/README.md)

<a id="section-1"></a>
## 1. 단계와 의존성

| 단계 | 선행 | 범위 | gate |
| --- | --- | --- | --- |
| DEV0 | 없음 | minimal test foundation | B0 |
| DEV1 | DEV0 | common semantics and Linux effect boundaries | F1 |
| DEV2 | DEV1 | SQL-free durable kernel | B2, D0 |
| DEV3 | DEV2 | finite disk storage and index baseline before SQL | S1 |
| DEV4 | DEV3 | embedded minimal SQL | Q1 |
| DEV5 | DEV4, COMM | server integration | B3 |
| DEV6 | DEV5 | client libraries | CLIENT1 |
| DEV7 | DEV4 | SQL and execution extensions | SQL_EXT1 |
| DEV8 | DEV3, DEV5 | replication operations and performance | OPS1 |
| DEV9 | DEV6, DEV7, DEV8, PART | declared product capability qualification | PRODUCT_QUALIFIED |
| COMM | DEV1 | independent transport branch | C1 |
| PART | DEV3 | SQL-free partition foundation | P1 |
| NG | PART | SQL-free native global engine branch | H2, H3 |

COMM은 [독립 통신 상세 설계](../contracts/comm-design.md)와 [C1/KU12 구현·검증 계획](comm-implementation-plan.md)을 따른다. DEV1 AXF1 bytes를 재사용하고 TCP loopback·소유 Unix socket에서 진행한다. transport·Linux adapter·focused runner는 부분 구현이며 [현재 실행 범위](../implementation/comm-implementation.md)를 따른다. C1 전체 인수는 미완료다.

DEV5의 local Unix 인증·권한·세션·XSQ1·prepared·취소·종료와 외부 ACK는 [서버 설계](../contracts/dev5-server-design.md), SC03/KU14/PA08/PF11과 실제 Q1/C1 수신은 [구현·검증 계획](dev5-implementation-plan.md)을 따른다. 설계 채택과 leaf API/byte fixture 동결·제품 B3 실행은 별개이며 B3는 NOT_RUN이다.

DEV6의 native/C facade→JDBC→ODBC 범위는 [클라이언트 설계](../contracts/dev6-client-design.md), 실제 B3 선행·SC04–06·D6-F01–F07·부분 buffer/취소/UNKNOWN·성능 인수는 [구현 계획](dev6-implementation-plan.md)을 따른다. 공통 계약 동결 후 독립 모듈의 [병렬 개발](dev6-implementation-plan.md#parallel)은 가능하며 SC04→SC05→SC06은 실제 인수 순서다. D2 설계 채택과 leaf 동결·구현·CLIENT1 완료는 별개이며 실행은 NOT_RUN이다.

DEV7의 확장 타입·관계 실행·PSM·LOB/spatial·외부 routine/DB link/XA는 [상세 설계](../contracts/dev7-sql-extensions-design.md)와 [구현·검증 계획](dev7-implementation-plan.md)을 따른다. 9개 요구군의 64개 필수 요구를 배정했으며 F01–F10의 세부 동결과 실제 제품 실행은 미완료다. SC07–SC11·SQL_EXT1은 NOT_RUN이고 PART/NG·server/client·외부 peer의 선행 증거를 해당 leaf에서 요구한다.

DEV8의 복제 H1/L1/M1·capture/retention·온라인 백업/PITR·보안/운영은 [상세 설계](../contracts/dev8-operations-design.md)와 [구현·검증 계획](dev8-implementation-plan.md)을 따른다. R11–R15의 30개 요구와 SC12–15·RC-T24/25·PF08/14를 배정했다. 설계 준비는 가능하지만 실제 S1/B3와 기능별 DEV6/7/PART/NG 공급, F01–F10 동결·전체 variant 실행 전 OPS1은 NOT_RUN이다.

DEV9의 기준선/후보 identity·세부 요구/조합·독립 판정·설치/문서는 [상세 설계](../contracts/dev9-qualification-design.md), 104개 부모 수신·F01–F10·T01–T34·R2-V01–04·P01–P05는 [구현·검증 계획](dev9-implementation-plan.md)을 따른다. DESIGN-002는 실행 분모/전체 시도·ACK 관측·PITR 목적·DP06 배포 요건을 보완한다. XADB-046 준비는 조기 착수 가능하지만 SC16/17과 PRODUCT_QUALIFIED는 NOT_RUN이며 실제 CLIENT1/SQL_EXT1/OPS1/P1 및 필요한 global gate를 요구한다.

<a id="section-2"></a>
## 2. 최소 SQL 범위

DEV4는 비파티션 디스크 table의 CREATE/DROP TABLE/INDEX, INSERT/UPDATE/DELETE, 단일 table SELECT·predicate·ORDER BY·limit, BEGIN/COMMIT/ROLLBACK과 typed prepare/bind/execute/fetch/close를 제공할 계획이다. 최소 타입은 signed 64-bit integer·BOOLEAN·UTF-8 text·binary와 NULL이다. 문법·타입·parameter·수명·오류는 [DEV4 상세 설계](../contracts/dev4-sql-design.md), 구현 순서·SC02/KU13/PF10은 [구현·검증 계획](dev4-implementation-plan.md)을 따른다. [SQL 초안](../reference/sql-language-draft.md)은 이전 제안이며, 실제 leaf API·profile 동결과 DEV3 실행 선행 요건은 남아 있다.

[Oracle 리뷰 보완](../contracts/dev4-sql-design.md#oracle-review)은 대문자 미인용 이름, SQL 빈 text 입력의 typed NULL, 전체 NULL만 중복 허용하는 문장 최종 UNIQUE, 이전 거래와 DDL의 별도 commit 결과·RecoveryRequired를 구체화한다. native 값 표현·독립 DDL API를 유지하며 DEV3-DESIGN-003의 statement token·table 단위 SERIALIZABLE·WAL/영속 undo 공급을 전제로 한다. 실제 공급 검증과 SQL READ COMMITTED 노출은 [후속 계약 의무](dev4-implementation-plan.md#oracle-followup)이며 Oracle 전체 호환·제품 실행 완료를 뜻하지 않는다.

PK·UNIQUE·NOT NULL, statement rollback, transaction snapshot + table read/write-set validation SERIALIZABLE, read-your-writes, schema/object 변경 후 handle 검증을 포함한다. 조인·집계·subquery·window·확장 타입·FK/CHECK·PSM·trigger 등은 DEV7에서 확장한다. 기본 prepare/bind를 DEV7까지 미루지 않는다.

<a id="section-3"></a>
## 3. 커널과 저장

DEV2의 K0–K5는 [하위 계층 계획](bottom-up-development.md)과 [커널 설계](../contracts/dev2-kernel-design.md)를 따른다. 현재 P0/K0 일부가 있으며 WAL/recovery는 없다. DEV3의 실제 disk page·buffer·index는 [상세 설계](../contracts/disk-storage-design.md)와 [구현·검증 계획](dev3-implementation-plan.md)을 따른다. 설계 준비는 DEV2 완료 전 가능하지만 production 착수의 선행 요건과 S1 실제 증거 의무는 유지한다.

<a id="section-4"></a>
## 4. 단계 완료와 공개

단계 완료는 해당 capability의 실제 증거를 요구한다. DEV9는 XADB가 선언한 제품 범위를 검증한다. 연구용 소스 공개는 완성된 DBMS 보장이 아니며, 안정 릴리스 qualification과 별도다. 저장소는 현재 Public이며 최종 목표와 초기 구현 상태를 함께 공개한다.

<a id="section-5"></a>
## 5. 운영·성능·확장

작은 검증 가능 변경을 진행하며 K0부터 계측·재현·독립 비교를 붙인다. 실제 성능 실행기·전체 instrumentation·보호 CI는 아직 없다. DEV7/DEV8의 기능 목록은 [작성 대상](feature-design-inventory.md)과 [제품 요구](product-requirements.md)를 따른다.

<a id="plan-align-001"></a>
<a id="section-6"></a>
## 6. 이관 결정

XADB-SCOPE-001이 이전 제품의 저장 범위와 외부 대체 gate를 대체한다. DEV0/DEV1·DEV2의 현재 코드와 독립 oracle·실제 I/O·동시성·복구의 정확성 의무는 보존한다. 새로운 Git 이력과 새 실행 보고서를 사용한다.

<a id="section-10"></a>
## 10. 단계 완료와 공개 상태

[완료 계약](stage-completion.json)의 모든 필수 task·gate·case와 독립 리뷰를 충족해야 해당 단계를 완료한다. 각 leaf의 PASS를 단계 전체 완료로 확대하지 않는다. DEV0·DEV1 로컬 검증과 DEV2 일부 구현 상태는 [현재 구현](../implementation/README.md)에 기록한다. 공개 저장소의 존재는 제품 qualification이나 안정 릴리스 호환을 보장하지 않는다.

DEV3의 [상태 home·obligation descriptor를 포함한 표현·selector/metadata port·제한된 선택 page 읽기·M02-P01 부분 모델](../implementation/dev3-implementation.md)은 별도 `leaf_progress`로 연결한다. DEV3 `execution_status=NOT_RUN`은 전체 디스크 경로의 단계 인수 상태이며 제한된 표현/model 실행이 없다는 뜻으로 읽지 않는다.

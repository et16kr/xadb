# XADB — 상용 DBMS 수준으로의 확장 가능성 평가

2026-10-19 · **기술 평가 참고 자료 / 구현·성능·제품 인수 증거 아님**

[제품 방향](../plans/project-direction.md) · [현재 구현](../implementation/README.md) · [개발 단계](../plans/development-stages.md) · [제품 기능 요구](../plans/product-requirements.md)

이 문서는 현재까지의 구현과 향후 설계를 바탕으로, XADB가 상용 DBMS 수준의 기능·규모·운영 능력으로 발전할 수 있는지 평가한다. 앞선 기술 평가의 문서화이며, 기존 제품 방향·소유 계약·단계 순서·인수 기준을 변경하지 않는 D0 기록이다. 작성·통합 담당은 `/root`다. 아래 후속 과제의 우선순위는 평가 의견이며 새로운 설계 채택이나 단계 완료 선언이 아니다.

평가 기준은 XADB commit `48fd20771a4c97c5677073e30084630187a363e6`와 평가 시 열람한 미커밋 작업 트리, 기존 설계와 보존된 실행 결과다. 작업 트리 전체를 동결한 실행 증거는 없으며, 각 시험 결과의 적용 범위는 해당 보고서의 원래 source 지문을 따른다. 이번 평가·문서화에서 제품 시험이나 비교 benchmark를 새로 실행하지 않았다.

<a id="judgment"></a>
## 1. 종합 판단

**상용 DBMS 수준으로의 확장 가능성은 조건부로 긍정적이다.** 검토한 범위에서 XADB의 발전을 원천적으로 막는 구조는 발견하지 못했다. 논리 행 정체성, 저장·트랜잭션·SQL·서버의 책임 분리, 실제 디스크 페이지와 제한된 버퍼로의 전환, 독립 검증 기반은 넓은 관계형 DBMS로 발전할 수 있는 방향이다.

현재 구현이나 DEV4–6의 최소 기능만으로 상용 DBMS 수준에 도달했다고 판단할 수는 없다. DEV3 저장 구조의 전환과 후속 SQL·타입·동시성·운영 기능에는 상당한 설계·구현·검증이 남아 있다. 처리량·지연·복구 시간·운영 안정성의 경쟁력도 실제 측정 전에는 판단할 수 없다.

| 판단 항목 | 평가 |
| --- | --- |
| 기능을 넓힐 수 있는 구조인가 | 조건부 긍정. 계층과 의미 경계가 있으며 후속 기능을 넣을 방향이 제시돼 있음 |
| 현재 구현을 그대로 크게 키우면 되는가 | 핵심 저장·복구·동시성 구조의 전환이 필요함 |
| 초기 SQL·서버·드라이버 완성이 상용 수준을 뜻하는가 | 최소 사용 경로의 완성에 해당하며 기능·규모·운영 인수는 별도임 |
| 성능과 안정성이 상용 수준에 도달할 수 있는가 | 목표로 삼을 기술적 근거는 있으나 현재 실행 증거로 확정할 수 없음 |
| 개발을 계속할 가치가 있는가 | 충분함. 다음 판단의 핵심 증거는 DEV3의 실제 저장·동시성·반복 복구 결과임 |

<a id="scope"></a>
## 2. 상용 DBMS 수준의 평가 범위

여기서 상용 DBMS 수준은 선언한 업무 범위에서 관계형 기능, 데이터 규모, 동시 실행, 장애 복구, 접속·보안, 운영 도구와 예측 가능한 성능을 함께 제공하는 수준을 뜻한다. 단일 인증 기준이나 모든 상용 제품 기능의 합집합을 뜻하지 않는다. 특정 제품과 동일한 SQL·ABI·내부 구현·성능을 약속하는 평가도 아니다.

평가 대상은 [제품 방향](../plans/project-direction.md)의 Linux·Rust 기반 DISK 관계형 DBMS다. 메모리 전용·혼합 저장 제품 엔진은 범위 밖이며 버퍼 캐시, 질의 작업 메모리, snapshot pin과 자원 한도는 계속 필요하다. 최종 제품 범위와 완료 여부는 [기능 요구](../plans/product-requirements.md), [단계 완료 계약](../plans/stage-completion.json), 해당 소유 계약이 정한다. DEV9의 선언한 capability 인수와 모든 상용 DBMS 업무의 충족은 구별한다.

<a id="current-state"></a>
## 3. 현재 진행 상태와 남은 거리

아래 표는 평가 시점의 상태 요약이다. 이후 상태는 [구현 색인](../implementation/README.md)과 각 소유 계약에서 확인한다.

| 영역 | 현재 구현·채택된 설계 | 상용 수준으로 확대할 때의 핵심 과제 |
| --- | --- | --- |
| 검증·공통 기반 | DEV0/DEV1 도구·공통 의미·Linux 효과 경계, DEV2 독립 이력 모델과 K0 substrate 구현. 보존된 로컬 검증은 각 source에 한정 | 최종 통합 source의 회귀, 독립 기대값, 장시간·장애 시험과 증거 보존 |
| 트랜잭션·영속성 | DEV2 K1–K5의 WAL·commit·recovery·checkpoint·backup/restore 코드와 부분 검증 존재. 최종 DEV2 인수 미완료 | 전체 필수 복구·동시성·V3·성능 인수와 DEV3로의 의미 인계 |
| 디스크 저장·인덱스 | DEV3 페이지·버퍼·B+tree·영속 undo·doublewrite·공간 회수 설계 채택. 구현·단계 모델·제품 시험 미완료 | RAM보다 큰 데이터, 실제 I/O, 반복 장애와 낮은 여유 공간에서도 안전한 진행 |
| SQL·질의 처리 | DEV4 단일 table SQL, typed prepared 실행, 제한된 access path와 외부 정렬 설계. SQL 미구현 | 조인·집계·subquery·window·확장 제약과 통계·비용 기반 optimizer |
| 타입·서버 프로그램 | 기본 값은 정수·불리언·문자·바이너리·NULL. 확장 타입·PSM·trigger 등은 후속 요구 | 저장·인덱스 비교·표현식·프로토콜·드라이버를 관통하는 의미 정합성 |
| 서버·접속 | COMM 일부 구현, C1 인수 미완료. DEV5/6 서버·native/C·JDBC·ODBC는 설계 단계 | 초기 로컬 접속·부분 API를 넘어 원격 접속·보안·표준 업무 상호운용 확대 |
| 운영·고가용성 | 초기 복구 기반과 후속 요구 존재. 복제·PITR·failover 등 미구현 | 로그 보존·스키마 변경·재동기화·장애 전환·실제 복원 drill의 통합 |

근거는 [DEV2 구현 기록](../implementation/dev2-implementation.md), [COMM 구현 기록](../implementation/comm-implementation.md), [DEV3 저장 설계](../contracts/disk-storage-design.md), [DEV4 SQL 설계](../contracts/dev4-sql-design.md), [DEV5 서버 설계](../contracts/dev5-server-design.md), [DEV6 클라이언트 설계](../contracts/dev6-client-design.md)다. 설계 채택을 실행 성공으로 계산하지 않는다.

<a id="extension-basis"></a>
## 4. 확장 가능성을 긍정적으로 보는 근거

현재 DEV2의 행 구조는 메모리에 유지되며 WAL과 checkpoint가 영속성을 제공한다. 실제 [TableRoot](../../crates/engine/src/catalog.rs)는 행 배열을 보유하고, [쓰기 거래의 commit 검증](../../crates/engine/src/transaction.rs)은 시작 시점과 현재의 전역 commit 위치를 비교한다. 이 구조 자체를 최종 대용량·고동시성 엔진으로 확대하기는 어렵다.

DEV3는 이 한계를 명시적으로 다룬다. mutable disk page·제한된 버퍼·B+tree·WAL·영속 undo로 바꾸고, commit과 데이터 페이지 flush를 분리한다. 전체 DB의 RAM 상주와 매 commit의 전체 데이터 동기화를 요구하지 않는 방향이다. fuzzy checkpoint, 페이지·extent 재사용, 오래된 버전 회수와 bounded 작업 공간도 포함한다. [저장 구조 선택](../contracts/disk-storage-design.md#choice)과 [자원 설계](../contracts/disk-storage-design.md#budgets)는 대용량 DBMS의 기반으로 타당한 방향이다. 이 조합의 안전성·진행성·성능은 아직 검증할 대상이다.

논리 RowUID와 물리 page/slot을 분리하고, SQL이 저장 페이지·WAL 내부에 직접 접근하지 않도록 한 경계도 유리하다. [SQL 실행 설계](../contracts/dev4-sql-design.md#execution)는 parser·binder·planner·executor를 분리하며, [prepared 실행](../contracts/dev4-sql-design.md#prepared)은 타입·metadata·객체 변경·수명을 처음부터 다룬다. 새 실행 연산자와 타입을 추가할 수 있는 경계가 있지만, 타입 확장은 값 표현·키 비교·codec·표현식·클라이언트까지 함께 검증해야 한다.

독립 이력 모델·known-bad 거절·실제 I/O와 외부 ACK 관측을 개발 초기부터 포함한 점도 긍정적이다. [재시작용 용량 예약 결함의 수정 기록](../implementation/dev2-implementation.md#dev2-review-20261019)은 commit 성공 뒤 복구 공간이 부족해 open이 실패하는 반례를 재현하고 고친 사례다. 검증 기반이 실제 문제를 발견했다는 근거이며, 전체 제품의 정확성 보장은 아니다.

DEV2에서 DEV3로 이어지는 것은 주로 의미·플랫폼 경계·검증 기반이다. interleaved WAL, page redo/undo·CLR, doublewrite, 새로운 동시성·checkpoint 경로는 상당 부분 새로 구현해야 한다. [재사용·재구현 구분](../plans/dev3-implementation-plan.md#reuse-boundary)이 이 비용을 명시한다. DEV3를 기존 커널에 저장 함수 몇 개를 추가하는 작업으로 추정하면 안 된다.

<a id="risks"></a>
## 5. 주요 위험과 확대 조건

| 위험 | 현재 제약과 영향 | 확대 판단에 필요한 근거 |
| --- | --- | --- |
| DEV3 교차 상태의 복잡성 | 영속 undo·참조 계수·aging·rollback·doublewrite·복구 예약이 맞물림. 개별 원리만으로 조합의 안전성을 보장할 수 없음 | 독립 상태 모델, 실제 codec 상한을 대입한 자원 계산, 반복 장애·실제 thread 시험 |
| 초기 동시성 | DEV2 전역 q 검증, DEV3의 짧은 version-edit gate, DEV4 table 단위 Serializable은 충돌·대기를 늘릴 수 있음 | 성공 TPS·충돌률·gate/lock/flush 대기 측정, SQL ReadCommitted와 더 세밀한 validation·group commit 등의 별도 검토 |
| 거래·자원 한도 | 대량 UPDATE/DELETE·적재가 초기 mutation·payload·동시 거래 상한에 걸릴 수 있음 | 대상 목록의 디스크 처리, undo/CLR·rollback reserve, 취소·재시작·장기 거래를 함께 확대하는 계약 |
| SQL·optimizer의 작업량 | 최소 SQL은 단일 table과 규칙 기반 access path. 복잡한 질의의 기능과 비용 선택이 대부분 후속 | 조인·집계 의미, 통계·선택도·조인 순서·실행 방식·spill의 독립 정답 및 실측 |
| 운영 기능의 커널 결합 | 복제·PITR·온라인 백업·XA가 commit·로그 보존·스키마·복구 상태에 영향을 줌 | 소비자별 보존 인계, 장애 후 재개·재동기화, 인증·권한, 실제 복원·전환 시험 |

DEV3의 핵심 미완료 의무는 [상태 모델](../plans/dev3-implementation-plan.md#state-model), [자원 계산과 진행 인수](../plans/dev3-implementation-plan.md#resource-proof), [구현 전 동결](../plans/dev3-implementation-plan.md#freeze)에 있다. 일반적인 WAL·undo 원리를 따른다는 사실만으로 XADB 고유의 상태 전이 조합이 안전하다고 판단하지 않는다.

초기 수치는 개발 profile이며 제품 용량이나 SLA가 아니다. 주요 한도는 다음과 같다.

| 범위 | 초기 설계 한도 | 확장 시 고려점 |
| --- | --- | --- |
| 거래 | 최대 32개, 거래당 256 mutation·논리 payload 1 MiB | 최대 크기의 거래 32개를 항상 동시 수용한다는 뜻은 아님. 다중 자원 예약과 rollback 진행을 다시 계산 |
| table·값·index | table 32열, 가변 값 64 KiB, table당 index 최대 8개·index당 1–4열 | schema·행/키 codec·페이지·metadata·프로토콜의 상한과 비용을 함께 확대 |
| 저장 | page buffer 64 MiB, owned disk 8 GiB, 그 안의 maintenance reserve 1 GiB, 보존 WAL 256 MiB | 상한 숫자와 실제 처리 규모를 구별. 큰 데이터와 회수·복구의 고정 메모리 및 공간 peak 검증 |
| 초기 사용 경로 | 최소 SQL, 로컬 Unix socket, JDBC/ODBC 부분 지원 | 원격 접속·인증·TLS·큰 인자·batch·LOB·framework 업무별 후속 인수 |

수치의 정본은 [DEV3 자원 계약](../contracts/disk-storage-design.md#budgets), [인덱스 계약](../contracts/disk-storage-design.md#keys), [DEV4 자원·처리 범위](../contracts/dev4-sql-design.md#resources), [DEV5/6 초기 범위](../contracts/dev6-client-design.md#scope)다. 한도를 늘릴 때에는 처리량뿐 아니라 오류 효과·자원 예약·취소·복구를 함께 보존해야 한다. 큰 DML을 숨은 여러 commit으로 나누는 방식은 기존 원자성을 보존한 확장이 아니다.

DEV7에는 확장 타입, 관계 연산, optimizer, 제약, PSM, LOB, XA 등 큰 과제가 모여 있다. [작성 대상 목록](../plans/feature-design-inventory.md)은 기능 누락을 줄이는 수단이지만 상세 설계·구현 완료량으로 계산할 수 없다. 실제 착수는 기능별 의미·의존성·독립 기대값과 소유 계약을 구체화하는 작업을 포함한다.

DEV3의 초기 exact-cut backup은 admission drain을 거친 offline 복사다. 온라인 백업·PITR·복제의 완료와 구별해야 한다. 후속 archive/RP 소비자가 WAL을 보존하면 회수 가능 범위와 복구 공간 조건도 달라지므로, 기존 예약·복구 진행 증명을 다시 확인해야 한다. [checkpoint·backup 계약](../contracts/disk-storage-design.md#checkpoint)과 [복구 예약 계약](../contracts/disk-storage-design.md#recovery-reservation)이 이 경계를 정한다.

<a id="evidence-limits"></a>
## 6. 현재 증거로 판단할 수 있는 범위

실제 커널 코드와 시험 기반이 존재하고, 일부 정상·장애·경계 사례의 로컬 성공 결과가 보존돼 있다. 이를 근거로 구현이 문서 단계에만 머물러 있다고 볼 수는 없다. 그러나 현재 전체 작업 트리의 내구성·성능·제품 qualification 완료를 주장할 수 없다.

- [DEV2 영속 검증 체크포인트](../evidence/dev2-kernel-xadb-20261018-r1.json)는 일부 성공과 ENOSPC에 따른 후속 캠페인 실패·미실행을 함께 보존한다. 부분 성공으로 전체 반복 의무를 대체하지 않는다.
- [K0 성능 기록](../evidence/dev2-performance-xadb-20261018-r1.json)의 최초 p99 비용 기준 실패는 계측 오버헤드 비교에 관한 것이다. 이를 전체 디스크 DBMS의 성능 판정으로 확대하지 않는다. 성능 인수는 미완료다.
- [최근 DEV2 결함 수정 결과](../implementation/dev2-implementation.md#dev2-review-20261019)도 해당 고정 source와 실제 실행 범위에 귀속된다. 이후 변경을 포함한 현재 source 전체의 PASS는 아니다.
- 같은 workload·데이터·동시성·장치·내구성 조건으로 상용 수준의 비교 목표를 충족했다는 benchmark 증거는 없다. 현재 자료로 완성률·성공 확률·완료 시점을 수치화하지 않는다.

성능 경쟁력은 성공한 작업의 처리량, tail latency, 충돌·거절률, 자원 peak, checkpoint·복구 시간 등을 함께 측정해야 판단할 수 있다. 미지원 기능이나 약한 내구성으로 얻은 수치를 기능·내구성 조건이 다른 결과와 직접 비교하지 않는다. 구체적인 profile과 인수는 [성능 개발 계약](../contracts/ai-performance-engineering.md)과 해당 단계가 소유한다.

<a id="next-evaluation"></a>
## 7. 다음 평가의 우선순위와 재검토 시점

아래는 기존 개발 순서 안에서 다음 판단에 필요한 증거를 정리한 것이다. 새로운 gate나 구현 승인을 추가하지 않는다.

| 순서 | 우선 확보할 결과 | 판단에 주는 의미 |
| --- | --- | --- |
| 1 | DEV2의 현재 범위에 대한 최종 통합·복구·V3·성능 인수. DEV3에서 교체할 구조의 과도한 최적화는 피함 | 영속성의 공통 의미와 검증 기반을 다음 저장 경로에 인계할 수 있음 |
| 2 | DEV3에서 buffer보다 큰 데이터, 실제 I/O, 동시 변경, 반복 장애, 공간 회수·낮은 여유 공간의 진행 검증 | 대용량 디스크 DBMS로의 확장 가능성을 가장 직접적으로 확인 |
| 3 | DEV4–6의 SQL·서버·클라이언트 사용 경로를 실제로 연결 | 값·오류·거래·취소·metadata 의미가 사용자 경로까지 일관되게 전달되는지 확인 |
| 4 | 수치·시간 타입, 조인·집계, 대량 거래와 실용적인 동시성을 기능별로 확대 | 최소 기능을 넘어선 업무 범위와 처리 규모를 확인 |
| 5 | 복제·복원·원격 보안·운영 도구와 선언한 workload의 성능·안정성 인수 | 선언한 제품 범위에서 상용 수준의 운영 능력을 판단 |

DEV3의 상태 모델·자원 계산이 성립하지 않거나, 낮은 공간에서 rollback·복구가 진행하지 못하거나, 올바른 결과를 유지한 상태에서 직렬화 비용이 목표 workload를 막으면 설계 대안을 재검토할 필요가 있다. 반대로 DEV3의 실제 인수와 후속 SQL·운영 시험이 쌓이면, 조건부 가능성 판단을 구현·측정에 근거한 구체적인 지원 범위로 좁힐 수 있다.

현 시점의 판단은 **개발을 계속할 기술적 가치가 충분하며 상용 DBMS 수준으로 발전할 경로가 있다**는 것이다. 그 경로의 실현에는 DEV3의 핵심 전환과 DEV7·DEV8 등 후속 기능의 완성이 필요하다. 도달 여부의 근거는 앞으로의 실제 구현·독립 검증·성능·운영 결과다.

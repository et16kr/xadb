# DEV7 — 확장 기능 구현·검증 계획

2026-10-19 · **DEV7-DESIGN-001·002 채택·독립 정적 검토 PASS 대응 / leaf 동결·구현·SC07–SC11·SQL_EXT1 NOT_RUN**

[상세 설계](../contracts/dev7-sql-extensions-design.md) · [조사·독립 검토](../evidence/dev7-source-research.md) · [단계 JSON](development-stages.json) · [완료 계약](stage-completion.json)

<a id="entry"></a>
## 1. 착수와 완료 조건

설계·독립 expected vector·모델 준비는 지금 가능하다. production SQL 통합은 현재 source identity의 실제 DEV4 Q1·DEV3 S1·하위 회귀를 입력으로 받는다. server 기능은 B3/C1, driver 기능은 CLIENT1의 실제 해당 API, PART/NG 노출은 P1/H2/H3가 추가된다. 코드 존재·채택된 계약·과거 dirty tree 실행을 실제 공급 증거로 대체하지 않는다.

각 leaf는 `의미/오류·수치 동결 → 독립 oracle/known-bad → 하위 native 공급 → SQL 구현 → 해당 server/client 통합 → 현재-build 복구/자원/성능 → 독립 검토` 순서로 인수한다. 부분 내부 공개는 가능하지만 64개 요구의 필수 operation·type·client·fault variant를 제외하여 SQL_EXT1을 닫지 않는다. DEV7 checker/worker command는 아직 없고 `run_entrypoint=null`이다.

<a id="freeze"></a>
## 2. 구현 전에 동결할 입력

F01–F10은 모두 **OPEN_NOT_IMPLEMENTED**다. 설계의 범위 선택과 실제 signature/byte/수치 profile 동결을 구별한다. 각 표를 기존 소유 계약의 leaf로 작성하며 빈 문서·crate·fake runner를 생성하지 않는다.

| ID | 소유 | 필수 입력과 착수 차단 범위 |
| --- | --- | --- |
| D7-F01 | foundation + SQL | 모든 type/함수/연산/overload/cast 결과형·NULL·rounding·overflow·달력/timezone/collation·error vectors, type ID와 persistent/index/spill/wire literal bytes. ComparisonDescriptor의 양쪽 타입/변환·padding·collation·NULL·key/hash 정규화와 저장 index 문맥 호환성. crypto/Unicode/regex dependency와 알고리즘/버전·key 계약. 해당 값 구현 전 |
| D7-F02 | SQL | parser grammar·typed logical/physical node·volatility·관계/재귀/window frame·optimizer rule 전제·cache key/metadata invalidation, parameter support matrix와 독립 bag evaluator. equality 추이성/비교 문맥 호환성, CHAR/VARCHAR 삼항 반례의 hash/merge/index/spill 및 rule on/off 결과. relational 구현 전 |
| D7-F03 | engine + SQL | RC/RR/history·constraint protected probe·savepoint/deferred·알려진 신규 lock closure 정렬과 기존/동적 잠금의 wait-for/deadlock, D7-STORAGE 확장 profile의 opcode별 최대 forward/undo/CLR·WAL/page/lock/FD/재장애 예약, API와 오류. 기본 S1과 다른 format/profile·fresh fixture 선택. engine 확장 전 |
| D7-F04 | catalog + storage + SQL | DDL precommit 두 outcome, dependency graph·view refresh·online job 상태와 delta/catch-up/switch/abort/recovery, PART/NG 실제 API와 공급 evidence. 관련 lifecycle 구현 전 |
| D7-F05 | SQL + server | PSM 전체 구문/overload·권한·exception·OUT/transaction·성공 전 result spool/REF CURSOR 소유·trigger/cascade order·autonomous·sequence allocation·queue/scheduler lease와 job policy, package state·관리/통신 API 목록. atomic 호출의 COMMIT/ROLLBACK/BEGIN/autocommit/거래 특성/DDL 차단, invocation 소유 명시 marker와 숨은 statement/CALL mark 분리·unwind·합산 상한, dynamic SQL/중첩 호출/trigger 및 autonomous 자식의 문맥 전달. programmable 구현 전 |
| D7-F06 | storage + SQL + client | LOB chunk/locator/read-write/Unicode·geometry 함수/형식/SRID/topology/index·shapefile 지원표, 기하 dependency/robustness·streaming bounds. 해당 저장/adapter 구현 전 |
| D7-F07 | engine + server + client | XA flag별 상태 전이·XID bytes·read-only/one-phase/join/resume/heuristic/forget·prepared recovery·session 분리·reserved resolve lane·DDL/backup Busy·shutdown 인계, coordinator journal protocol·peer/TM capability·fault cut; 외부 worker ABI/권한/격리와 DB link mode. StandardXa 정상 receipt retirement/NOTA·TM의 XID 비재사용, HeuristicCompleted의 recover 합집합/scan·영속 forget, DecisionReceiptAck1의 협상·durable ACK/frontier/구멍·quota를 분리. 원자성/peer별 격리/전역 격리 구별·미지원 수준의 effect 전 거절·두 RM 역순 dependency/read-only vote 반례. 외부 효과 구현 전 |
| D7-F08 | DEV5/6 계약 담당 | 확장 capability/profile·message body/type tag·request/result sequence·CALL spool/OUT 공개·REF CURSOR/LOB pin·CLI/C/JDBC/ODBC/embedded SQL/PDO/.NET support matrix·실제 Oracle/JDBC/HDFS/Hive adapter/runtime·DM/TM·TLS 공급·빌드/설치 manifest. 해당 adapter 구현 전 |
| D7-F09 | 실행/검증 담당 | 각 요구→operation→variant→독립 expected→실제 API/worker→case 대응, bounded model 전체 공간·fault reachability·quota 산식/peak·cleanup, 고정 seed/corpus·원시 receipt·부정 self-test. runner 및 인수 전 |
| D7-F10 | 성능/통합 담당 | release/environment/fixture/oracle identity·계측 mode·수치 guardrail·순서·원시 bundle·통계와 replay recipe. 첫 측정 전 |

완료 계약의 `detail_expansion_required=true`는 위 작업이 실제로 끝날 때만 변경한다. 현 계획의 T01–T32는 인수 책임 묶음이며 overload별 전체 leaf 목록이나 실제 시험 수가 아니다. 미결정 byte/ABI를 구현자가 추측해 소비하지 않는다.

<a id="sequence"></a>
## 3. 구현 순서·소유·합류

| 순서 | 구현 묶음 | 선행 / 공급 |
| --- | --- | --- |
| D7-I01 | XADB-037 값/연산/codec·독립 수치 oracle | F01, 실제 foundation/DEV4; SQL 저장 인수는 S1 |
| D7-I02 | XADB-038 관계 기준선·sort/hash/spill·통계/plan/cache | I01·F02/F09, native scan/snapshot 공급; T05–T12 |
| D7-I03 | XADB-038 engine 격리/savepoint/FK·큰 거래·DDL/view/partition SQL | I01·F03/F04; 소비 SQL 전에 native 복구/예약 인수. PART/NG는 해당 작업에만 추가 |
| D7-I04 | XADB-039 PSM·trigger·sequence·queue·scheduler·온라인 lifecycle | I02/I03·F05, 실제 server 권한/worker. 온라인 변경은 F04 공급 |
| D7-I05 | XADB-040 LOB·geometry·spatial index·파일/stream adapter | I01·XADB-029·F03/F06; SQL→B3/CLIENT1 실제 공급과 합류 |
| D7-I06 | XADB-041 외부 worker·link·XA RM/coordinator·외부 변경 adapter/분석 교환 | I04·XADB-036·F07/F08; prepared native 인수 후 두 RM/TM 연결. 변경 추출·logging 내부와 R10-05 상세는 D-LOGGING 보류 |
| D7-I07 | 각 feature의 실제 driver/도구·언어 업무·전체 인수 | 해당 I01–I06·F08/F09/F10. 독립 검토와 모든 하위 회귀 뒤 SQL_EXT1 |

한 함수마다 crate를 만들지 않는다. sql의 값 바인딩/관계/optimizer/PSM과 engine의 거래/catalog/LOB/index, server의 coordinator/worker, client의 adapter를 책임별 파일로 구현한다. 공통 값/오류·wire·engine API는 한 담당이 변경하고 소비 모듈이 별도 정의를 복제하지 않는다. 계획 package와 허용 의존성은 [저장소 정책](../policies/repository-build-plan.md)의 정본이다.

병렬 구현을 실제 수행할 때에는 [소유 정책](../policies/parallel-development-policy.md)에 따라 기준 commit/dirty digest·owned_paths·공유 조립 파일 담당·합류 순서를 먼저 기록한다. F01 뒤 relational과 LOB의 독립 구현, F07 뒤 RM과 coordinator의 별도 모듈 준비는 가능하다. 공유 계약 동결 및 native→SQL→server/client 인수는 직렬이다. 미래 담당은 미배정이며 이번 설계 담당/통합은 `/root`다.

<a id="coverage"></a>
## 4. 요구 배정표

`capability-inventory.json`에서 planned_stages에 DEV7이 있는 R01/02/03/05/06/07/08/09/10의 64개 요구를 모두 배정한다. 아래 행은 **필수 책임의 보존**이며 specification/implementation/equivalence PASS가 아니다. 기존 하위 단계가 담당한 부분은 회귀하고 DEV7의 추가 variant만 확장한다. 상세 함수/API 목록은 F01–F09로 남는다.

| 요구 | 보존하는 기능 | 주 작업 / 인수 | 설계 / 주 시험 |
| --- | --- | --- | --- |
| R01-01 | 관계 조회와 중복·NULL 의미 | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T05 |
| R01-02 | 내부·외부·교차 조인 | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T05 |
| R01-03 | 상관 subquery와 lateral 조회 | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T05 |
| R01-04 | 집계·다중 grouping | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T06 |
| R01-05 | window 분석 | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T07 |
| R01-06 | 집합 연산·정렬·부분 결과 | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T08 |
| R01-07 | 재귀·계층 탐색 | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T08 |
| R01-08 | 행·열 전환 | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T08 |
| R01-09 | 복합 DML·upsert·변경 결과 반환 | XADB-038 / SC08 | [relational](../contracts/dev7-sql-extensions-design.md#relational) · D7-T09 |
| R01-10 | view·이름 간접 참조 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T13 |
| R01-11 | 조회 결과의 물리화·갱신 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T13 |
| R01-12 | 객체 생성·변경·삭제·메타데이터 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T13 |
| R02-01 | 정수·고정소수·근사수 표현 | XADB-037 / SC07 | [types](../contracts/dev7-sql-extensions-design.md#types) · D7-T01 |
| R02-02 | 문자·국제 문자·길이 의미 | XADB-037 / SC07 | [types](../contracts/dev7-sql-extensions-design.md#types) · D7-T02 |
| R02-03 | byte·가변 byte·bit 값 | XADB-037 / SC07 | [types](../contracts/dev7-sql-extensions-design.md#types) · D7-T02 |
| R02-04 | 날짜·시간·표현 변환 | XADB-037 / SC07 | [types](../contracts/dev7-sql-extensions-design.md#types) · D7-T03 |
| R02-05 | 수치·문자·시간·변환 함수 | XADB-037 / SC07 | [types](../contracts/dev7-sql-extensions-design.md#types) · D7-T04 |
| R02-06 | 암호·해시 관련 SQL 기능 | XADB-037 / SC07 | [types](../contracts/dev7-sql-extensions-design.md#types) · D7-T04 |
| R02-07 | 조건·cast·정규식·표현식 | XADB-037 / SC07 | [types](../contracts/dev7-sql-extensions-design.md#types) · D7-T04 |
| R03-01 | 명시 transaction·autocommit·read-only | XADB-038 / SC08 | [constraints](../contracts/dev7-sql-extensions-design.md#constraints) · D7-T11 |
| R03-02 | statement 실패·savepoint | XADB-038 / SC08 | [constraints](../contracts/dev7-sql-extensions-design.md#constraints) · D7-T12 |
| R03-03 | READ COMMITTED·REPEATABLE READ·SERIALIZABLE의 업무 능력 | XADB-038 / SC08 | [constraints](../contracts/dev7-sql-extensions-design.md#constraints) · D7-T11 |
| R03-04 | 명시 잠금·대기 제어 | XADB-038 / SC08 | [constraints](../contracts/dev7-sql-extensions-design.md#constraints) · D7-T11 |
| R03-05 | 세션 상태·동시 사용 | XADB-038 / SC08 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T28 |
| R03-06 | 구조화 오류·재시도 범위 | XADB-038 / SC08 | [constraints](../contracts/dev7-sql-extensions-design.md#constraints) · D7-T12 |
| R05-01 | range·list·hash 분할 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T14 |
| R05-02 | partition pruning | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T14 |
| R05-03 | 디스크 partition과 행 이동 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T14 |
| R05-04 | partition 추가·삭제·분할·병합·교환 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T14 |
| R05-05 | 분할 객체 관리와 관측 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T14 |
| R06-01 | 정렬·보조·복합 index 접근 | XADB-038 / SC08 | [optimizer](../contracts/dev7-sql-extensions-design.md#optimizer) · D7-T10 |
| R06-02 | primary·unique·NOT NULL 제약 | XADB-038 / SC08 | [constraints](../contracts/dev7-sql-extensions-design.md#constraints) · D7-T11 |
| R06-03 | 참조 무결성과 CHECK | XADB-038 / SC08 | [constraints](../contracts/dev7-sql-extensions-design.md#constraints) · D7-T11 |
| R06-04 | local·global index 범위 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T14 |
| R06-05 | index lifecycle·유효성·재구성 | XADB-038 / SC08 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T13 |
| R07-01 | procedure·function·호출 인자 | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T15 |
| R07-02 | 제어 흐름·예외·dynamic SQL | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T15 |
| R07-03 | cursor·record·배열·사용자 타입 | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T15 |
| R07-04 | package·session 상태·의존 객체 | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T16 |
| R07-05 | 독립 transaction을 사용하는 routine | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T16 |
| R07-06 | trigger | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T17 |
| R07-07 | sequence | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T17 |
| R07-08 | 작업 스케줄러 | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T18 |
| R07-09 | 내장 package의 관리·통신 기능 | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T18 |
| R07-10 | 온라인 객체 변경 능력 | XADB-039 / SC09 | [objects](../contracts/dev7-sql-extensions-design.md#objects) · D7-T19 |
| R07-11 | transactional queue·메시지 소비 | XADB-039 / SC09 | [psm](../contracts/dev7-sql-extensions-design.md#psm) · D7-T18 |
| R08-01 | LOB 값과 부분 읽기·쓰기 | XADB-040 / SC10 | [storage-extension](../contracts/dev7-sql-extensions-design.md#storage-extension) · D7-T20 |
| R08-02 | 공간 값·교환 표현 | XADB-040 / SC10 | [storage-extension](../contracts/dev7-sql-extensions-design.md#storage-extension) · D7-T21 |
| R08-03 | 공간 관계·분석·색인 | XADB-040 / SC10 | [storage-extension](../contracts/dev7-sql-extensions-design.md#storage-extension) · D7-T21 |
| R08-04 | 공간 client API와 파일 적재 | XADB-040 / SC10 | [storage-extension](../contracts/dev7-sql-extensions-design.md#storage-extension) · D7-T22 |
| R09-01 | C/C++ 호출형 client | XADB-041 / SC11 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T28 |
| R09-02 | JDBC 표준 업무 | XADB-041 / SC11 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T28 |
| R09-03 | ODBC 표준 업무 | XADB-041 / SC11 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T28 |
| R09-04 | embedded SQL 개발 능력 | XADB-041 / SC11 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T29 |
| R09-05 | PHP/PDO·.NET 및 언어 연결 | XADB-041 / SC11 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T29 |
| R09-06 | XA·외부 transaction manager 연동 | XADB-041 / SC11 | [xa](../contracts/dev7-sql-extensions-design.md#xa) · D7-T25 |
| R09-07 | 큰/배열 인자·결과·취소 | XADB-041 / SC11 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T28 |
| R09-08 | 도구·framework 상호운용 | XADB-041 / SC11 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T29 |
| R09-09 | 로컬·원격 접속과 전송 제약 | XADB-041 / SC11 | [clients](../contracts/dev7-sql-extensions-design.md#clients) · D7-T29 |
| R10-01 | 외부 DB 조회·원격 실행 | XADB-041 / SC11 | [external](../contracts/dev7-sql-extensions-design.md#external) · D7-T24 |
| R10-02 | 외부 C/C++ routine 호출 | XADB-041 / SC11 | [external](../contracts/dev7-sql-extensions-design.md#external) · D7-T23 |
| R10-03 | 이종 DB 변경 전달 | XADB-041 / SC11 | [external](../contracts/dev7-sql-extensions-design.md#external) · D7-T27 |
| R10-04 | 분산 분석 환경 데이터 교환 | XADB-041 / SC11 | [external](../contracts/dev7-sql-extensions-design.md#external) · D7-T27 |
| R10-05 | 변경 분석·구독 기능 | XADB-041 / SC11 | [필수 예약](../contracts/dev7-sql-extensions-design.md#external) · D-LOGGING 보류, 상세 인수 사례 미작성 |

<a id="acceptance"></a>
## 5. 필수 인수 묶음

| ID | 인수 내용 | 독립 기대값·필수 실패/경계 |
| --- | --- | --- |
| D7-T01 | 수치·cast·결과 metadata | 정수/decimal 유리수 검산, 동률·음수 scale·carry·overflow·NaN/-0·mixed type 거절; SQL/key/hash/driver 일치 |
| D7-T02 | 문자/binary/bit/collation | padding·빈 값/NULL·Unicode 길이·부정 UTF-8·비교 문맥별 sort/hash key 동등 class·bit 잔여 padding; CHAR(1)/CHAR(2)/VARCHAR(2)의 삼항 반례와 index/full scan 비교 |
| D7-T03 | 날짜/timezone·함수 | 달력/월말·DST gap/overlap·tzdb 차이·precision 변환; CPU/OS locale에 의존하지 않는 literal expected |
| D7-T04 | 함수·regex·crypto | 함수/overload 전수 domain/NULL/error, lazy CASE, regex pathological input의 종료, 알려진 crypto vector·nonce/권한·부정 tag·secret 비노출 |
| D7-T05 | join/subquery/lateral | 0/1/N match·NULL·중복·outer ON/WHERE·NOT IN(NULL)·scalar 2행 오류, nested/hash/merge/spill/correlated/decorrelated 동등성. 혼합 padding의 A=B AND B=C가 한 행을 유지하고 허용되지 않은 A=C 추론이 거절됨 |
| D7-T06 | aggregate/grouping | 빈 전체/grouped 입력·all-NULL·DISTINCT·rollup/grouping bit·overflow·중복 bag·hash skew |
| D7-T07 | window | peer ties·ROWS/RANGE/GROUPS·EXCLUDE·기본 frame·NULL order·frame 경계·큰 partition/peer·stream prefix 실패 |
| D7-T08 | set/pivot/재귀 | ALL multiplicity·NULL set 동등·pivot 빈 cell/type 결합·tree/diamond/cycle·경로별 cycle·UNION 전역 dedup·한도 초과 incomplete |
| D7-T09 | MERGE/INSERT SELECT/RETURNING | Halloween·동일 target 중복 source·관측 후 concurrent update·unique 최종 상태·trigger/cascade 영향·실패 후 반환 prefix 없음 |
| D7-T10 | optimizer/stats/cache | rule별 전제 위반 known-bad, 비교 문맥을 합친 equality 추론·hash/key 공유 mutant 거절, stats stale/skew, 권한 철회/search path/tz/collation/DDL race, metadata 변경 ReprepareRequired·old plan pin·cache full |
| D7-T11 | FK/CHECK/격리/잠금 | 부모 삭제와 자식 삽입 race, composite NULL·self-reference·cycle/cascade·deferred fix/commit fail, RC/RR/Serializable 허용/금지 history·NOWAIT/cancel/deadlock. 서로 이전 B/A 잠금 뒤 dynamic SQL/trigger로 A/B를 요청하는 역순 반례·기존 잠금 해제 없음·Prepared victim 제외 |
| D7-T12 | savepoint/확장 거래 | marker 재지정/release·LOB/queue/constraint rollback·read/lock 유지·반복 rollback 후 WAL/CLR 잔류 과금·limit/limit+1·undo 재장애. 호출 frame별 같은 이름 분리·다른 frame/implicit marker 접근 거절·return/unwind 정리와 전체64 상한 |
| D7-T13 | DDL/view/materialized result | precommit 성공 뒤 DDL 실패의 두 outcome·generation/권한·refresh 실패 old generation 유지·CTAS/default/type 변환·재시작 |
| D7-T14 | PART/NG SQL | range/list/hash·pruning/no-prune·행 이동·partition lifecycle·local/global index+extended type·DDL와 stale plan, 실제 core 공급 |
| D7-T15 | PSM/exception/cursor/record/array | IN/OUT/INOUT·overload·dynamic SQL 권한·handled/unhandled 오류·CALL 원자성·합산 recursion/step budget·취소. 호출 전 INSERT/사용자 savepoint 후 CALL 안의 전체 ROLLBACK·외부 marker ROLLBACK TO/RELEASE/이름 재지정이 이전 변경을 침범하지 않음. 직접/동적/중첩/trigger 각각 거래 제어 거절·자기 marker만 허용·종료된 거래를 handler가 부활시키지 않음. DML+첫 result spool 뒤 다음 식 오류/취소/quota 초과에서 CALL rollback·OUT/row 공개0; 반환 REF CURSOR의 spool/handle 수명 |
| D7-T16 | package/autonomous | session state 격리·generation invalidation·pool reset·parent rollback과 child commit·self-lock deadlock·child UNKNOWN |
| D7-T17 | trigger/sequence | trigger 순서/mutating table/cascade 최종 제약·재귀·sequence reserve 전후 crash·non-cycle 재사용0·CURRVAL session·gap 허용 |
| D7-T18 | queue/scheduler/built-ins | 다중 consumer·rollback 재소비·lease fencing·duplicate delivery·dead-letter·wall clock jump·권한 철회·external effect UNKNOWN |
| D7-T19 | 온라인 객체 변경 | Building/CatchUp/Switching 매 cut·delta overflow·writer fence·구/신 generation reader·switch 후 취소/cleanup·재시작 |
| D7-T20 | LOB | NULL/empty·64KiB scalar와 구분·큰 값/offset/Unicode 부분쓰기·chunk 누락/중복·stale locator·snapshot aging·cancel/ENOSPC·recrash |
| D7-T21 | geometry/spatial index | 모든 geometry subtype·empty/invalid·SRID mismatch·ring boundary·touch/contain/overlap/distance·연산 한도·bbox false negative0·full exact 대조 |
| D7-T22 | 파일/대형 교환 | WKT/WKB byte order/길이·EWKB SRID·shapefile attribute/encoding·부분 파일/truncation·lossy conversion 명시 거절·export/import |
| D7-T23 | external routine | 실제 C/C++ ABI·worker crash/timeout/취소·oversize·권한/격리·dangling result/old worker ID·외부 효과 uncertainty |
| D7-T24 | DB link | ReadOnly/IndependentRemote/Atomic2pc mode·type/collation 차이·pushdown on/off·remote disconnect/duplicate reply·비2PC peer atomic 거절·미지원 전역 격리의 effect 전 거절 |
| D7-T25 | XA RM | 전체 flags/상태·중복 XID·prepare vote·read-only/one-phase·잠금/Serializable fence 재구성·late/opposite decision. StandardXa 정상 완료가 추가 ACK/forget 없이 ledger 상한을 넘는 누적 거래 수에서도 회수·진행; 보존 중 동일 outcome과 retirement 후 NOTA 구별. Prepared+HeuristicCompleted recover pagination·중간 force/forget·close/restart, force 응답 유실 후 recover→heuristic 결과→forget·재장애 뒤 부활0. prepare/close 경합·idle/300초/권한 철회/pool/shutdown·일반 deadlock victim에서 Prepared 보존 |
| D7-T26 | XA coordinator | 두 RM과 실제 TM·participant YES 유실·durable decision 전후 분리·split response·coordinator/RM 순서별 crash/recrash·decision 불변. StandardXa와 확장 ACK 지원 TM을 별도 인수하고 NOTA/목록 부재만으로 성공 합성0; 확장은 ACK 전 outcome 보존·ACK 유실/중복·frontier 구멍·늦은 XID·epoch/restart. A에서 T1 read→T2 write, B에서 T2 read→T1 write/read-only vote의 전역 cycle을 local Serializable/2PC 성공만으로 PASS하지 않음. Prepared32·DDL/backup drain 중 resolve control 진행/Busy와 종료 인계 |
| D7-T27 | 이종 adapter/분산 교환 외부 계약 | Oracle/JDBC의 필드/타입·binding/batch·원격 오류·단절 certainty; HDFS/Hive 실제 입출력·타입/encoding·완결/부분 실패. 변경 추출·logging 내부·R10-05 상세 인수는 D-LOGGING 보류 |
| D7-T28 | native/C/JDBC/ODBC 확장 | 실제 runtime별 type/Callable/batch/LOB/keys/multi-result/pool/XA·partial buffer·client cancel·외부 ACK·current-build reopen. CALL 실패 전 OUT/result 공개0·spool REF CURSOR stale·성공 뒤 fetch 오류가 CALL outcome을 변경하지 않음·LOB pin 미지원 거절 |
| D7-T29 | embedded SQL/언어/framework/remote | 생성 소스 clean build·host indicator/array/thread·PHP/PDO/.NET 실제 업무·framework metadata·TLS/auth failure와 정상 연결 |
| D7-T30 | 합산 자원·spill/cleanup | 각 node와 다중 query의 resident peak, skew fallback·동시 merge·FD/disk full·unlink 실패 잔류·prepared 압력·cleanup 독립 진행. 정상 XA receipt retirement의 bounded 진행과 heuristic/미ACK 보존 압력 분리·recover scan pin/metadata 합산 |
| D7-T31 | 부정 oracle/검사기·전체 복구 | wrong binary/profile·미도달 cut·누락 variant·빈 trace·stale expected·관계/복구 mutant 거절, 실제 native/SQL/외부 history 비교 |
| D7-T32 | 성능·재현·하위 회귀 | 아래 P01–P05와 관련 최신 source의 하위 단계 전체 필수 검사, 독립 검토 뒤 leaf/단계 상태 분리 |

의미가 같은 지원 path는 동일 fixture와 expected를 통과해야 한다. 독립 oracle은 production expression evaluator·key codec·optimizer를 정답 계산에 import하지 않는다. 작은 finite relation enumerator, decimal/rational 모델, explicit geometry fixtures, 별도 transaction/XA 상태 모델을 사용한다. Oracle/다른 DBMS 실행 비교는 동작이 명세상 일치하는 셀의 보조 근거이며 XADB 독립 정답을 대신하지 않는다. 이번에는 비교 실행하지 않았다.

<a id="faults"></a>
## 6. 필수 fault·동시성·재장애 행렬

| ID | 중단 지점 | 요구하는 외부/복구 결과 |
| --- | --- | --- |
| D7-C01 | DML·cascade·trigger·LOB chunk/descriptor·undo/CLR의 각 영속 경계 | statement prefix만 남는 성공0; ACK 거래 보존·rollback 확정 전 성공 오류 금지 |
| D7-C02 | sequence 범위 WAL 시도/완료/발급·재시작 | non-cycle 확정 발급 재사용0, gap 허용; sync 불명 상태 재발급 금지 |
| D7-C03 | queue claim/ack·scheduler lease/result, consumer 외부 효과 전후 | row 효과와 외부 효과 분리, 중복/UNKNOWN 표시·stale worker fencing |
| D7-C04 | refresh/online job build·delta drain·switch record/sync·공개·old 회수 | complete old 또는 complete new identity, 반쪽 catalog/index0; cleanup 재장애 |
| D7-C05 | RM prepare record 쓰기/sync·응답 직전/유실·cancel/close·checkpoint·reopen | 불완전 prepare는 loser, durable prepare는 in-doubt 보존·잠금/undo/validation fence 복원; prepare 시도 뒤 불명은 close 때문에 abort하지 않음 |
| D7-C06 | coordinator prepare 요청 전후·YES 일부/전체·decision append/sync·전달/ACK | 손상과 decision 부재 구별; 확정 COMMIT을 ABORT로 바꾸지 않음; 두 RM의 상반 확정0 |
| D7-C07 | RM commit/abort intent·CLR·visibility·terminal receipt/retirement·heuristic force/forget·확장 ACK/frontier·DDL/backup drain/shutdown | 보존 중 decision 재전달·회수 뒤 NOTA와 UNKNOWN 구별; 정상 회수에 TM 추가 메시지 불필요. heuristic force 응답 유실 뒤 recover 노출·forget 영속 후 부활0, 미완료/heuristic/미ACK 기록의 조기 삭제0·반복 crash 후 예약 복원·예약 resolve lane 진행·backup Busy·Prepared 영속 인계 |
| D7-C08 | external worker/link/driver의 partial write/read·cancel·close | effect 전 거절/가능한 remote effect/확정 outcome 구별; 무조건 재시도0 |
| D7-C09 | spill create/write/read/unlink·disk/FD/memory 부족·동시 cleanup | truncated/spill checksum 오류 검출·incomplete 결과·실제 회수 전 charge 유지 |
| D7-C10 | Oracle/JDBC adapter batch·분석 교환 file/stream의 요청/remote reply·cancel/close | 완결/부분 실패·remote certainty·필드/타입 무손실 또는 명시 거절; 외부 fixture 성공을 실제 logging 추출 증거로 대체하지 않음 |

F09는 각 cut를 operation별 실제 opcode/fixture/환경/worker에 전개한다. fault hook 호출 횟수가 아니라 실제 도달 witness·native 효과·외부 controller history·current-build reopen 및 integrity 검사로 판정한다. process kill·I/O fault·power-loss model을 따로 보고한다.

bounded model은 (M01) 2 writer+reader의 parent/child/savepoint와 호출 frame 소유, (M02) executor+DDL/권한 변경의 cache/result 및 비교 문맥, (M03) LOB upload+snapshot+cancel/aging, (M04) coordinator+2 RM+재전달/restart/drain과 역순 dependency·정상 retirement/heuristic recover/확장 ACK, (M05) lease owner2개+queue consumer를 분리한다. F09에서 각 모델의 actor·값·operation bound·전체 상태/전이 집합·terminal invariant·state budget을 고정하고 선언한 공간을 완주한다. state budget 도달은 PASS가 아니다.

DP03의 V2 64 seeds×최대128 operations, V3 256×최대1,024를 적용하고 최대 길이 고정 사례도 포함한다. 영향받는 실제 concurrency suite는 V3 최소30분, parser/type/spill/WKT/WKB/worker/XA codec 등 실제 target별 fuzz 최소30분, 비결정 fault 셀은 fresh fixture3회다. suite 분할은 가능하나 미실행/실패/timeout 셀을 제외하지 않는다. 숫자를 실패 뒤 축소하지 않는다.

<a id="performance"></a>
## 7. 성능·계측·재현 profile

정확성/복구를 통과한 동일 capability·fresh fixture·release binary에서 비교한다. P01–P05는 최초 엔지니어링 인수 목표이며 SLA나 실측값이 아니다. DP06 대상 환경의 실제 CPU/kernel/filesystem/device·toolchain/lockfile·feature·host 경쟁 작업, oracle/fixture/seed/hash를 F10에서 고정한다. 성능 측정 중 경쟁 build/stress를 수행하지 않는다.

| ID | workload | 필수 측정·최초 guardrail |
| --- | --- | --- |
| D7-P01 | table256 MiB·1 GiB, buffer32 MiB·workspace8 MiB; uniform/skew/NULL join·aggregate/window·recursive spill | 실제 disk I/O, peak가 설계 ledger 이하, 정답 동일. 선택적 최적화는 주 지표 median10% 이상 개선 및 paired95% CI가 개선 방향; p99/CPU/I/O 어느 하나10% 초과 악화 시 재검토 |
| D7-P02 | cache off/on·반복 prepare·parameter skew·DDL/권한 churn | hit 비용·compile/execute 분리, 잘못된 plan/권한/metadata0. cache 목표 parse+plan CPU20% 감소, end-to-end p99 악화5% 이하; 이득 없으면 cache 기본 off 가능하나 기능 검증은 유지 |
| D7-P03 | multi-session PSM/trigger/cascade·queue·LOB1/8/16 MiB·취소 | latency/throughput·WAL/undo/FD/resident/retained bytes·cleanup drain, 반복100회 종료 후 **모든 논리 owner ledger0**, 유지해야 할 physical 비용은 reclaim 뒤0. RSS는 allocator high-water와 누수 구별 |
| D7-P04 | two-RM XA·prepare32 한도·decision/recover·외부 adapter batch | prepare/decision/end-to-end 분리, prepared retention/재시작 비용·admission 압력. 보호 quota 위반0; phase timeout은 사전10초 profile, timeout의 UNKNOWN/in-doubt는 성능 실패와 별도 정확성 결과 |
| D7-P05 | 계측 off/always/detailed·새 세션 replay | always CPU·median latency 악화5% 이하, sample-p99 악화10% 이하, trace loss 명시. detailed 비용은 별도 보고하며 acceptance에 필요한 event loss는0; bundle만으로 exact 의미/오류 재현 |

workload cell당 warmup30초·측정60초를 기본으로 최소10쌍 AB/BA 교대한다. p99는 각 run10,000개 이상 완료 operation을 확보하며 부족하면 NOT_ESTIMABLE·인수 미완료로 남기고 사전 profile을 D2로 확장한다. LOB/XA 긴 operation도 짧은 표본의 최대값을 p99로 포장하지 않는다. raw timestamp·count·분포·paired CI 계산법을 고정하고 noise가 크면 더 측정한다. 결과가 유리한 workload만 선택하지 않는다. 기능 때문에 필요한 spill/2PC는 단순 구현보다 빨라야만 채택하는 대상이 아니며, 그 비용과 현재 한도 안의 정상 진행을 별도 보고한다.

하위 DEV0/DEV1/현재 DEV2 검사와 해당 시점의 실제 S1/Q1/B3/C1/CLIENT1/PART/NG runner를 입력 manifest로 연결한다. 아직 없는 실행 명령을 성공한 것처럼 적지 않는다. 문서 검사는 [pre-push 규칙](../policies/documentation-policy.md#push-gate)을 따른다.

<a id="completion"></a>
## 8. 완료 증거와 남은 상태

leaf bundle은 source revision+dirty digest·export/binary/runtime/fixture/oracle hash, capability/operation/variant 목록, source-to-case 연결, 외부 ACK·원시 이력·fault reachability·모든 결과, 독립 검토자와 미해결 문제를 포함한다. actual result가 없는 expected 표는 증거가 아니다. 잘못된 binary/미도달/누락을 거절하는 checker 자체의 부정 시험도 필수다.

설계 채택 이후에도 F01–F10 동결과 모든 제품 실행은 NOT_RUN이다. [DEV7-DESIGN-002 재검토](../evidence/dev7-review-002.md)의 네 반례와 위 확장 인수도 실제 실행 의무로 남는다. SC07–SC11 각 leaf PASS를 전체 DEV7 PASS로 확대하지 않는다. PART/NG·언어 adapter·온라인 변경·이종 sink 등 어렵거나 선행이 부족한 항목도 필수 배정표에 남긴다. R10-03 logging 내부·R10-05 상세의 D-LOGGING 보류도 해제·설계·실행 전까지 전체 완료의 미해결 의무다. 전체 overload/variant 확장 완료와 모든 필수 통합·현재-build recovery·자원·성능·하위 회귀가 끝나야 SQL_EXT1을 완료한다.

# XADB 제품 기능 요구

2026-10-18 · **기능·인수 초안 / 전체 구현·검증 아님**

[제품 방향](project-direction.md) · [기능 정본](capability-inventory.json) · [작성 대상](feature-design-inventory.md)

기존 개발 작업의 기능과 독립 인수 아이디어를 디스크 DBMS 범위로 검토해 유지했다. 각 항목은 XADB의 기능과 인수 조건을 제안한다. source 조사·leaf 명세·실제 구현·시험은 따로 기록한다.

## R01 SQL and relational execution

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R01-01 | 관계 조회와 중복·NULL 의미 | 작은 독립 관계 evaluator로 projection/filter/distinct와 bag 결과를 대조하고 NULL·빈 집합·중복 입력을 포함한다. |
| R01-02 | 내부·외부·교차 조인 | 매칭 없음·다중 매칭·NULL key를 가진 입력에서 INNER/LEFT/RIGHT/FULL/CROSS 결과와 행 수를 독립 계산한다. |
| R01-03 | 상관 subquery와 lateral 조회 | 바깥 행별 참조·EXISTS/IN·다중행 scalar 오류를 검증하고 decorrelation 전후 의미를 비교한다. |
| R01-04 | 집계·다중 grouping | 빈 입력·NULL·중복과 소계/총계 조합의 값을 검산하며 grouped NULL과 원래 NULL을 구분한다. |
| R01-05 | window 분석 | partition/order/frame 경계·동률·선행/후행 행을 고정한 expected vector와 대조한다. |
| R01-06 | 집합 연산·정렬·부분 결과 | 중복 보존/제거 및 collation·NULL 위치·offset/limit·정렬 동률의 명세를 검증한다. |
| R01-07 | 재귀·계층 탐색 | 트리·다중 부모·cycle·깊이 제한을 포함해 종료와 결과를 확인한다. 문법 변경 시 같은 탐색을 표현하는 XADB 예를 제공한다. |
| R01-08 | 행·열 전환 | pivot/unpivot 업무를 표현하고 누락 값·중복 집계·자료형 결합의 결과를 검증한다. |
| R01-09 | 복합 DML·upsert·변경 결과 반환 | INSERT SELECT·MERGE와 다중 row 갱신의 충돌·변경 행 수·반환 값·statement 실패 효과를 확인한다. |
| R01-10 | view·이름 간접 참조 | view와 synonym에 해당하는 기능을 제공하고 이름 충돌·의존 객체 변경·권한 검사를 검증한다. |
| R01-11 | 조회 결과의 물리화·갱신 | materialized 결과와 원질의를 대조하고 refresh 전후·실패·참조 객체 변경 시 유효성을 검사한다. |
| R01-12 | 객체 생성·변경·삭제·메타데이터 | table/column/default/comment/rename/CTAS 등 lifecycle의 의존성·실패 효과와 metadata 관측을 검증한다. |

## R02 scalar types, expressions, character sets and collation

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R02-01 | 정수·고정소수·근사수 표현 | 값 범위·precision/scale·rounding·overflow·index 비교·driver 왕복을 독립 수치 vector로 검사한다. FLOAT라는 이름만으로 이진 부동소수라고 가정하지 않는다. |
| R02-02 | 문자·국제 문자·길이 의미 | padding·문자/byte 길이·Unicode·부정 입력·정렬과 equality를 명세하고 원문 데이터의 무손실 입출력을 확인한다. |
| R02-03 | byte·가변 byte·bit 값 | 0 byte·홀수 nibble·bit 길이·최상위 bit·NULL을 포함해 저장/연산/전송 왕복을 검사한다. |
| R02-04 | 날짜·시간·표현 변환 | 달력 경계·윤일·소수초·session 설정·문자 변환의 독립 vector를 둔다. XADB 시간 타입 명칭은 달라도 표현 가능한 값을 잃지 않는다. |
| R02-05 | 수치·문자·시간·변환 함수 | 함수별 domain·NULL·오류·경계 vector와 XADB 대응표를 만든다. 함수 이름 변경이나 SQL 재작성은 실행 가능한 예로 증명한다. |
| R02-06 | 암호·해시 관련 SQL 기능 | 기존 업무의 암호화/복호화/해시 목적을 명시한 API로 제공하고 알려진 vector·키 오류·길이 처리를 검증한다. 알고리즘 개선에 따른 데이터 변환을 명시한다. |
| R02-07 | 조건·cast·정규식·표현식 | 3값 논리·연산 우선순위·명시/암시 변환·정규식 경계·연산 실패를 명세 기반 expected와 대조한다. |

## R03 transaction, concurrency, isolation and error semantics

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R03-01 | 명시 transaction·autocommit·read-only | 여러 session과 table에 대한 commit/rollback·자기 변경 읽기·읽기 전용 쓰기 거절을 검증한다. 영속화 내부 방식은 이번에 정하지 않는다. |
| R03-02 | statement 실패·savepoint | 부분 DML 실패와 savepoint rollback 뒤 유지/제거되는 행·index·자원을 검증한다. |
| R03-03 | READ COMMITTED·REPEATABLE READ·SERIALIZABLE의 업무 능력 | 허용/금지 history와 대기·충돌을 각 수준별로 정한다. 초기 coarse SERIALIZABLE만으로 다른 수준의 업무·동시성까지 대체했다고 판정하지 않는다. |
| R03-04 | 명시 잠금·대기 제어 | 행/범위/table 보호가 필요한 작업, NOWAIT 또는 제한 대기, cancel과 교착 탐지의 결과·오류 효과를 검증한다. |
| R03-05 | 세션 상태·동시 사용 | session/transaction 설정 범위·connection 재사용·다중 connection/thread 사용의 상태 누출을 검사한다. |
| R03-06 | 구조화 오류·재시도 범위 | SQLSTATE/XADB code·statement/transaction effect·재시도 가능성을 매핑하고 오류 메시지 문자열 일치 없이 동일 실패를 처리할 수 있음을 검증한다. |

## R04 Disk storage, pages, buffer, space and resource management

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R04-01 | 영속 DISK 데이터 접근 | 실제 파일 I/O, bounded buffer와 재시작으로 행·인덱스·catalog의 같은 commit 경계를 확인한다. |
| R04-02 | 비영속 저장 기능 | volatile 저장의 사용 목적·수명·제약을 XADB capability로 명시한다. 재시작 관련 세부 계약은 복구 후속 논의와 연결하고 기능을 삭제하지 않는다. |
| R04-03 | transaction·session 임시 table | session 간 격리·transaction 종료 시 보존/제거·임시 index·공간 회수를 검사한다. 기존 물리 tablespace 구성은 강제하지 않는다. |
| R04-04 | 압축을 통한 공간 절감 | 압축 전후 값·index·갱신 의미를 대조하고 중복률별 실제 공간과 쓰기 비용을 측정한다. 기존 dictionary 내부 구조는 요구하지 않는다. |
| R04-05 | 저장 공간의 배치·증설·상한 | 상세 독립 인수 명세 필요 |
| R04-06 | table 이동·공간 재조직 | 이동 전후 행·제약·참조의 동일성과 실패 시 상태를 검사한다. 온라인 수행이 필요한 경우 가용성 조건도 함께 인수한다. |

## R05 Disk partition routing, pruning and lifecycle

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R05-01 | range·list·hash 분할 | 경계·NULL·default partition·hash key를 포함한 독립 routing oracle로 삽입과 조회를 검증한다. |
| R05-02 | partition pruning | 전체 scan과 같은 결과를 내면서 대상 partition만 접근하는지 확인한다. 잘못된 pruning을 검출하는 반례를 둔다. |
| R05-03 | 디스크 partition과 행 이동 | 파티션 간 행 이동과 local/global index를 한 transaction으로 처리하고 crash 후 원자성을 확인한다. |
| R05-04 | partition 추가·삭제·분할·병합·교환 | 지원 동작별 경계 값·row 보존·index/constraint 영향·실패 원자성을 명세한다. 수동 재생성만으로 동일 운영 능력이라고 판정하지 않는다. |
| R05-05 | 분할 객체 관리와 관측 | partition별 상태·크기·정책·통계·권한을 조회하고 관리 작업의 영향 범위를 확인할 수 있어야 한다. |

## R06 Indexes and constraints, including optional disk global indexes

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R06-01 | 정렬·보조·복합 index 접근 | point/range/order 조회를 독립 full scan과 대조하고 다중 key 순서·NULL·중복·갱신을 검사한다. |
| R06-02 | primary·unique·NOT NULL 제약 | 동일 statement와 concurrent transaction의 충돌을 검증하고 실패 후 행·모든 index가 일치하는지 확인한다. |
| R06-03 | 참조 무결성과 CHECK | parent/child 변경·삭제 동작·NULL·순환 의존·위반 오류를 독립 관계 모델로 검증한다. |
| R06-04 | local·global index 범위 | partition key 없는 조회와 table 전체 uniqueness가 필요한 업무를 지원한다. local fan-out은 비용·의미 기준을 만족한 기능적 대응인지 별도 판정한다. |
| R06-05 | index lifecycle·유효성·재구성 | 생성·변경·삭제·재구성 동안 metadata와 query 선택·제약 집행을 검증한다. 미준비 index를 정상 지원으로 표시하지 않는다. |

## R07 stored programs, triggers, sequences and queue objects

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R07-01 | procedure·function·호출 인자 | IN/OUT/INOUT·반환 값·중첩 호출·권한·SQL 내 호출을 검증한다. 서버에서 수행되는 업무를 무조건 client 왕복으로 대체하지 않는다. |
| R07-02 | 제어 흐름·예외·dynamic SQL | 분기/반복·예외 범위·동적 binding·실패 전후 transaction 효과를 작은 프로그램별로 검증한다. |
| R07-03 | cursor·record·배열·사용자 타입 | cursor 수명·bulk 처리·record/associative array·REF CURSOR 전달을 검사한다. 기존 문법과 ABI는 유지 의무가 없다. |
| R07-04 | package·session 상태·의존 객체 | 명세/body·상태 범위·변경 후 invalidation·호출자/정의자 권한·동시 session 분리를 검증한다. |
| R07-05 | 독립 transaction을 사용하는 routine | 부모와 별개인 성공/실패·가시성·자원 경계를 시험한다. 기존 pragma 표기를 그대로 유지할 필요는 없다. |
| R07-06 | trigger | 시점·행/statement 범위·조건·오류·연쇄 효과를 검증한다. 부작용을 조용히 생략하는 변환은 허용하지 않는다. |
| R07-07 | sequence | 동시 발급·범위·cycle·cache 설정·명시값 처리와 application 재사용을 시험한다. 장애 시 세부 계약은 후속 복구 논의에 연결한다. |
| R07-08 | 작업 스케줄러 | 일정·실행 주체·중복 실행 정책·중지·실패 관측을 검증한다. 기존 JOB 명령을 대체하는 API도 같은 기능을 제공해야 한다. |
| R07-09 | 내장 package의 관리·통신 기능 | 알림·사용자 잠금·metadata 추출·통계/plan 관리·파일/네트워크/메일·병렬 routine 기능을 각각 대응표에 등록한다. 외부 효과의 권한·timeout·오류를 검사한다. |
| R07-10 | 온라인 객체 변경 능력 | copy/swap 등 기존 온라인 작업의 목적을 보존하고 concurrent DML·서비스 중단 시간·실패 시 노출 상태를 시험한다. 알고리즘은 XADB가 선택한다. |
| R07-11 | transactional queue·메시지 소비 | queue 생성·변경·삭제, enqueue/dequeue, 조건부 소비, FIFO/LIFO에 해당하는 선택, 대기/timeout·상한을 검증한다. 여러 consumer의 중복 소비·rollback 효과를 독립 이력으로 확인한다. 기존 명령과 queue 내부 표현은 의무가 아니다. |

## R08 LOB and spatial data/functions

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R08-01 | LOB 값과 부분 읽기·쓰기 | 큰 BLOB/CLOB를 제한된 메모리로 왕복하고 byte/character offset·chunk 경계·취소·transaction 효과를 검증한다. |
| R08-02 | 공간 값·교환 표현 | geometry 종류·빈 값·부정 도형·WKT/WKB 입출력·좌표계 정보를 독립 geometry vector로 검증한다. |
| R08-03 | 공간 관계·분석·색인 | 경계 접촉·포함·겹침·거리와 색인/전체 평가 동등성을 검사한다. 내부 tree 종류는 고정하지 않는다. |
| R08-04 | 공간 client API와 파일 적재 | 공간 값의 client 전달과 shapefile 입출력·속성·SRID 매핑을 검증하고 지원하지 않는 표현의 손실을 명시적으로 거절한다. |

## R09 CLI/JDBC/ODBC/APIs/precompiler and XA workflows

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R09-01 | C/C++ 호출형 client | 연결·prepare/bind/execute/fetch·metadata·오류·취소·메모리 소유권을 검증한다. 기존 함수명이나 binary ABI는 의무가 아니다. |
| R09-02 | JDBC 표준 업무 | statement/prepared/callable·batch·생성 key·다중 result·pool·metadata·타입 매핑을 API 지원표와 실행 사례로 검증한다. |
| R09-03 | ODBC 표준 업무 | driver manager와 DSN·binding·cursor·diagnostic·metadata·LOB·cancel 경로를 실제 client에서 확인한다. |
| R09-04 | embedded SQL 개발 능력 | host 변수·NULL indicator·배열·cursor·dynamic SQL·다중 connection/thread를 빌드 가능한 XADB 예제로 대체한다. 단순 삭제나 수작업 전면 재작성만으로 PASS 처리하지 않는다. |
| R09-05 | PHP/PDO·.NET 및 언어 연결 | 문서에 있는 언어별 접속·타입·transaction·batch·pool 업무를 XADB client 또는 검증된 표준 경로로 수행한다. |
| R09-06 | XA·외부 transaction manager 연동 | prepare/commit/rollback·오류·in-doubt 운영 인터페이스의 요구를 보존한다. crash 후 세부 동작·영속 방식 설계는 복구/logging 후속 결정에 의존한다. |
| R09-07 | 큰/배열 인자·결과·취소 | 부분 전송·batch 중간 오류·처리 개수·streaming 한도·cancel 후 연결 재사용을 검증한다. |
| R09-08 | 도구·framework 상호운용 | 표준 metadata로 연결·객체 탐색·편집·SQL 실행·plan 조회를 수행한다. 연결 성공만으로 전체 지원이라고 표시하지 않는다. |
| R09-09 | 로컬·원격 접속과 전송 제약 | 같은 host와 다른 host의 실제 client에서 연결·인증·재접속·큰 메시지·취소·timeout을 검증한다. IPC/IPCDA/Unix socket/TCP의 내부 구현을 모두 복제할 필요는 없지만 로컬 저지연·원격 접속·지원 자료형 업무를 성능과 기능 대응으로 입증한다. |

## R10 external procedures, DB links, log analysis and adapters

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R10-01 | 외부 DB 조회·원격 실행 | 원격 관계 접근·pass-through·binding·batch·결과형·권한·원격 오류를 시험한다. 기존 Java linker/protocol 구현은 요구하지 않는다. |
| R10-02 | 외부 C/C++ routine 호출 | 인자·반환·NULL·timeout·외부 실패 전파를 검증하고 routine 오류가 엔진 무결성을 훼손하지 않도록 한다. |
| R10-03 | 이종 DB 변경 전달 | Oracle/JDBC 대상에 변경을 전달하는 능력을 보존한다. 필드/타입 매핑·오류 보고의 외부 계약만 기록하며 변경 추출·logging 내부는 D-LOGGING에 보류한다. |
| R10-04 | 분산 분석 환경 데이터 교환 | HDFS/Hive 등 문서상 입출력 업무를 지원할 경로와 타입 매핑을 제공한다. 기존 Sqoop 구현을 재사용할 의무는 없다. |
| R10-05 | 변경 분석·구독 기능 | 로그 분석 도구의 사용 목적을 보존할 기능으로 예약한다. 이번에는 상세 명세·logging 조사·인수 사례를 작성하지 않는다. |

DEV8에 배정한 R11–R15의 30개 요구는 [운영 설계](../contracts/dev8-operations-design.md)와 [구현·검증 계획](dev8-implementation-plan.md#coverage)에서 구체화한다. DEV8-DESIGN-001은 이 범위의 recovery/logging·capture/archive·audit/diagnostic 설계를 재개하며, R10-03/R10-05의 별도 보류와 모든 제품 실행 미완료는 유지한다.

## R11 replication, synchronization and failover

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R11-01 | 단일·다중 쓰기 노드 운영 | active-standby와 active-active 업무별 쓰기 소유·충돌·전환의 외부 계약을 명세한다. 단일 writer만으로 모든 기존 복제 업무를 대체했다고 주장하지 않는다. |
| R11-02 | 동기·비동기 복제 모드 | 모드별 성공 응답·지연·연결 단절 시 관측 결과를 인수 조건으로 분리한다. commit/logging·원격 보호 대기의 내부 경계는 DEV8-DESIGN-001을 따른다. |
| R11-03 | 충돌 탐지·처리·관측 | 중복 key·삭제/갱신 경합과 제약 오류의 처리 결과를 확인하고 누락·덮어쓰기·충돌 건수를 운영자가 판별할 수 있게 한다. |
| R11-04 | 초기 동기화·부분 대상·재동기화 | 대상 table/partition·데이터 일치·진행률·중단/재시도를 검증한다. snapshot과 stream pin의 공급 방식은 DEV8-DESIGN-001을 따른다. |
| R11-05 | 복제 중 schema·sequence 운영 | DDL과 sequence의 대응·변경 승인/거절·양쪽 schema 관측을 검증한다. 기존 데이터 해석이 다른 schema에 적용되지 않게 한다. |
| R11-06 | 접속 전환·failback·다중 주소 | client 재접속·진행 중 요청의 불명확 결과·중복 실행 위험·계획 전환과 복귀를 명세한다. 투명 재접속만으로 transaction 연속성을 주장하지 않는다. |
| R11-07 | 복제 관리·상태 조회 | 관계·상태·지연·진행률·오류·start/stop·구성 변경을 운영자가 일관되게 다룰 수 있어야 한다. |

## R12 backup, recovery, restore and required point-in-time behavior

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R12-01 | 백업·복원·시점 복구·DISK recovery | checkpoint·backup·restore·PITR의 cut, 필수 WAL·파일 closure, 손상·I/O 오류와 반복 복구의 독립 인수 기준을 복구 계약에 따라 구현 전에 구체화한다. |

## R13 authentication, authorization and transport protection

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R13-01 | 계정·인증·password 정책 | 계정 lifecycle·잠금/만료·실패 처리·관리자 복구 절차와 비밀 노출 방지를 검증한다. |
| R13-02 | 역할·객체/시스템 권한 | grant/revoke·역할·routine 권한·소유자 변경·session 간 권한 반영을 허용/거절 matrix로 검증한다. |
| R13-03 | 전송 보호와 인증서 관리 | 보호된 연결·인증서 오류·만료·갱신·비보호 연결 정책을 실제 client별로 시험한다. 오래된 암호 설정을 복제하지 않는다. |
| R13-04 | 데이터 보호·보안 모듈 연계 | 암호화 데이터의 사용 목적·키 수명·권한·오류 관측을 명세한다. 기존 보안 플러그인 ABI의 동일성은 요구하지 않는다. |
| R13-05 | 감사 정책·조회 | 대상·성공/실패 조건·주체·시각·행위의 기록과 조회 능력을 보존한다. 감사 intent/outcome·sink 장애·보존 설계는 DEV8-DESIGN-001을 따른다. |

## R14 administration, configuration, diagnostics, monitoring and import/export

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R14-01 | catalog·운영 metadata 조회 | 객체·권한·session·대기·자원·복제 상태를 권한별로 조회하고 API/schema 정의와 실제 응답을 대조한다. |
| R14-02 | 설정·관리 session·시작/종료 | 설정 scope·변경 가능 시점·검증·현재/적용 예정 값·정상 운영 명령을 명시하고 오류 설정의 부분 적용을 방지한다. |
| R14-03 | 대화형·batch SQL 도구 | script·변수·출력·인코딩·exit code·transaction·접속 관리·객체 설명을 검증한다. 기존 화면·명령 이름은 요구하지 않는다. |
| R14-04 | 논리 schema·데이터 export/import | 객체 의존순서·권한·제약·값/NULL/인코딩·LOB를 검증하고 실패 행·부분 완료를 재현 가능한 보고서로 남긴다. |
| R14-05 | 이종 데이터·객체 이관 | 상세 독립 인수 명세 필요 |
| R14-06 | 데이터 비교·선택 동기화 | 누락·추가·상이한 값·NULL·중복을 분류하고 선택한 방향으로 적용한 결과를 독립 재비교한다. |
| R14-07 | 성능 진단·plan·통계 | plan·추정/실제 행 수·통계 수집/관리·plan 제어 목적을 제공하고 튜닝 전후 결과 동등성을 확인한다. |
| R14-08 | plan/result cache 제어 | 재사용·무효화·권한/schema/값 변경 시 결과를 확인하고 cache가 없어도 같은 업무를 만족하는 경우 성능 기준으로 동등성을 검증한다. |
| R14-09 | 모니터링 API·알림·SNMP 연계 | 상태·수치·단위·오류·알림과 기존 SNMP 업무를 지원하는 exporter 등을 제공한다. metric 이름을 바꾸면서 관측 능력을 삭제하지 않는다. |
| R14-10 | 보호된 진단·관리 도구 묶음 | 문제 판별·상태 점검·오류 해석·profile 기능을 권한과 작업 결과를 갖춘 XADB 도구로 제공한다. bounded bundle·권한·redaction·identity 설계는 DEV8-DESIGN-001을 따른다. |
| R14-11 | 외부 관리·directory 연계 | DB 도구의 관리/탐색과 OpenLDAP SQL backend 같은 문서상 연계 목적을 표준 client와 실행 예제로 검증한다. |

## R15 workload performance, capacity, resource limits and RTO/RPO

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R15-01 | 대표 업무 corpus와 결과 기준 | OLTP point/range·복합 query·batch·PSM·LOB·partition·디스크·복제 업무를 등록하고 독립 expected 및 의미 차이 변환을 고정한다. |
| R15-02 | 처리량·tail latency·동시성 | 같은 장비·데이터·결과·격리/내구성 조건에서 baseline과 XADB를 교차 반복한다. 실패/충돌/재시도 포함 처리량과 p50/p95/p99를 보고한다. |
| R15-03 | 표현·구조적 용량 경계 | 아래 capacity_reference의 기존 표현 능력을 XADB 구조나 검증된 대응으로 수용하고 경계/초과·메타데이터·client 경로를 검사한다. |
| R15-04 | 큰 데이터·bounded resource | RAM 초과 DISK/인덱스·큰 결과·긴 transaction의 자원 한도와 응답성을 검증한다. 전체 mapping의 강제 상주로 데이터 규모를 축소하지 않는다. |
| R15-05 | 혼합 workload·운영 중 기능 | 읽기/쓰기·DDL·대량 적재·routine·복제 경합을 조합해 기능별 starvation·cancel·대기·자원 누출을 계측한다. |
| R15-06 | 성능 판정의 사전 등록 | 측정 전 workload별 통과 범위·변동 처리·허용 tradeoff를 기록한다. 측정 후 느린 항목을 삭제하거나 기준을 낮춰 통과시키지 않는다. |

## R16 Linux installation, operation and XADB version upgrades

| 요구 | 기능 | 인수 방향 |
| --- | --- | --- |
| R16-01 | Linux 설치·배포·제거 | 고정된 배포판/CPU/의존성에서 일반 사용자 설치·시작·제거·다중 instance 경계를 검증한다. 사용자 제외인 비Linux OS까지 요구를 넓히지 않는다. |
| R16-02 | 사내 새 구축과 재현 | 안정 릴리스 전에는 새 DB와 fixture로 매번 구축한다. 이전 사내 binary/data/API 변환을 요구하지 않으며 기존 디렉터리를 자동 삭제하거나 덮어쓰지 않는다. |
| R16-03 | 안정 릴리스 이후 XADB 버전 계약 | 최초 안정 릴리스부터 같은 major의 이전 안정 릴리스 버전 호환을 검증한다. 이전 사내 버전은 기준선에서 제외하며 세부 recovery/conversion 구현은 이번에 다루지 않는다. |
| R16-04 | 사용자 문서·기능 대응표 | 상세 독립 인수 명세 필요 |

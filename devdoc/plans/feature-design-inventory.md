# XADB 기능 명세·설계 작성 대상

2026-10-18 · **작성 목록 / 명세·구현 완료 증거 아님**

[기계 판독 목록](feature-design-inventory.json) · [제품 요구](product-requirements.md) · [공개 참고 자료](../reference/opensource-references.md)

기존 비저장매체 전용 항목을 유지하고 디스크 제품 범위에 맞췄다. 각 topic에서 외부 원문 확인 → 기능 의미 → 설계 → 인수 계획 → 실제 구현·증거를 구분한다. 파일 하나씩을 미리 만들지 않는다.

DEV4 최소 subset의 문법·타입·DDL/DML·prepared·CLI는 [상세 설계](../contracts/dev4-sql-design.md)와 [구현·검증 계획](dev4-implementation-plan.md)에서 구체화했다. 아래 SQL·TYPE·CAT·TOOL 항목의 전체 DEV7/driver 범위를 작성 완료한 것은 아니며 실행 상태도 바꾸지 않는다.

## BASE 제품 범위와 공통 계약

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| BASE-01 | 제품 범위와 공개 capability | 지원 범위·차이·명시적 비지원과 공개 조건 | 요구 ID와 명세·설계·인수 증거 추적 체계 |
| BASE-02 | 용어와 객체 식별 체계 | DB·table·row·transaction·session·epoch 식별 의미 | ID 발급·재사용·직렬화·수명 경계 |
| BASE-03 | 계층과 공개 엔진 API | 상위 계층 없이 수행할 엔진 동작·오류·완료 의미 | 모듈 책임·의존 방향·API 소유권과 호출 계약 |
| BASE-04 | 오류와 경고 및 재시도 | 오류 분류·SQLSTATE 대응·statement/transaction 효과 | 오류 전달·원인 보존·경계 변환·재시도 정보 구조 |
| BASE-05 | 메모리와 자원 예산 | row·batch·query·session 상한과 초과 결과 | 예약·소유권·해제·공유 메모리 계상·OOM 경계 |
| BASE-06 | 동시 실행과 취소 공통 계약 | timeout·deadline·취소 시점별 효과 | scheduler·clock·동기화·취소 전달과 수명 |
| BASE-07 | Linux 플랫폼과 파일 효과 | 파일·동기화·시간·프로세스 기능 및 지원 환경 | OS adapter·fault 주입 경계·파일 잠금·directory sync |
| BASE-08 | 빌드와 모듈 배치 | 독립 build·feature·profile·dependency 요구 | Cargo workspace·FFI 경계·생성 코드·의존 검사 |
| BASE-09 | 코딩과 공개 계약 작성 규칙 | 명명·문서화·오류·수명·검토 요구 | 책임별 파일·API 계약·unsafe 검토·정적 검사 체계 |
| BASE-10 | 객체 수명과 자원 회수 | handle 유효성·close·pin·회수 이후 접근 결과 | 소유권·세대·참조·drop·GC와 회수 지연의 책임 |

## TYPE 데이터 타입과 표현식

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| TYPE-01 | 정수와 정확 수치 | 폭·범위·precision·scale·rounding·overflow | 수치 표현·산술·비교·codec·index key 통합 |
| TYPE-02 | 근사 수치 | FLOAT/DOUBLE·특수값·비교·변환·집계 오차 | 부동소수 연산·정렬·hash·직렬화 일관성 |
| TYPE-03 | 문자와 국가별 문자 | 문자집합·길이 단위·빈 문자열·정규화·잘못된 입력 | encoding 변환·문자열 표현·client/server 경계 |
| TYPE-04 | Collation과 문자 비교 | 대소문자·정렬·패딩·LIKE·locale 의미 | 비교·정렬키·hash·색인·버전 영향 |
| TYPE-05 | 이진과 비트 타입 | BYTE/VARBYTE·BIT/VARBIT·길이·padding·표현 | binary codec·비트 연산·binding·key 비교 |
| TYPE-06 | 날짜 시간과 시간대 | 범위·precision·format·calendar·timezone·interval 요구 판정 | 시간 표현·timezone 자료·변환·결정적 평가 |
| TYPE-07 | NULL과 진리값 | 3값 논리·NULL/빈 문자열·조건·제약·집계 결과 | nullable 표현·연산·filter·key·오류 전달 |
| TYPE-08 | 형 변환과 타입 추론 | 명시/암시 cast·우선순위·손실·실패·인자 해석 | type resolver·변환표·상수 folding·binding 공유 |
| TYPE-09 | 수치 문자 날짜 함수 | 함수명·signature·overload·NULL·오류·결정성 | 함수 등록·평가·공통 연산·검증 vector |
| TYPE-10 | 조건식 연산자와 정규식 | CASE·COALESCE·산술·비교·문자 연결·정규식 모드 | 평가 순서·short circuit·pattern 처리·예산 |
| TYPE-11 | 해시 암호화 및 raw 함수 | SQL digest·암복호·raw 변환 기능과 제한 | 암호 provider 경계·키 전달·오류·값 노출 방지 |
| TYPE-12 | 값과 키 및 공개 형식 | 정규 표현·순서·동등성·전송과 저장 가능한 값 범위 | codec·byte order·길이 검사·형식 식별·변경 관리 |

## CAT 카탈로그와 스키마 객체

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| CAT-01 | 카탈로그 모델과 부트스트랩 | 객체 식별·schema·이름·소유자·조회 가능한 속성 | system catalog 저장·초기 생성·schema epoch·복구 |
| CAT-02 | 이름 해석과 객체 의존 | 검색 경로·인용 식별자·대소문자·동명이름·권한 | dependency graph·binding·invalidate·drop 영향 |
| CAT-03 | 테이블 생성 삭제 변경 | CREATE/ALTER/DROP·column·default·rename·truncate·comment | DDL 실행·원자성·catalog/storage 변경·실패 정리 |
| CAT-04 | 제약조건 정의와 검사 | PK·UNIQUE·NOT NULL·FK·CHECK·검증 시점·참조 동작 | 제약 metadata·DML 통합·동시성·cascade·rollback |
| CAT-05 | View와 synonym | 조회·갱신 가능성·의존·권한·이름 간접 참조 | view binding·전개·권한 컨텍스트·무효화 |
| CAT-06 | Materialized view | 결과 물리화·refresh·일관성·실패·관리 동작 | refresh 실행·원본 의존·동시 읽기·저장 수명 |
| CAT-07 | 온라인 객체 변경 | 사용 중 변경·대기·중단·원래 객체 유지 조건 | 변경 상태 전이·쓰기 동기화·교체·재개·복구 |
| CAT-08 | 테이블 복제 교체와 재구성 | CONJOIN/DISJOIN·copy/swap·데이터 이동 업무 | 객체 교체·참조 재연결·공간·index/constraint 처리 |
| CAT-09 | 삭제 객체 보관과 복원 | recycle bin·FLASHBACK·PURGE·이름 충돌·보관 제한 | 보관 metadata·공간 회수·의존 복원·권한 |
| CAT-10 | Directory와 외부 library 객체 | 외부 경로·library 등록·권한·이름 관리 | catalog와 OS 자원 연결·유효성·수명·격리 경계 |
| CAT-11 | 공개 데이터 사전 | system/object metadata·조회 권한·열·타입·일관성 | 내부 catalog와 공개 view 분리·snapshot·필터링 |
| CAT-12 | 객체 간 지원 조합 | DISK·partition·LOB·압축·복제·DDL 조합 | capability 판정·선행 검사·unsupported 오류 경계 |

## TXN 트랜잭션과 동시성

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| TXN-01 | 거래 생명주기 | BEGIN/COMMIT/ROLLBACK·autocommit·read-only·종료 | transaction 상태·owner·handle·commit/abort 전이 |
| TXN-02 | Snapshot과 읽기 가시성 | read-your-writes·가시성 시점·반복 읽기·범위 읽기 | version 선택·snapshot capture·pin·회수 조건 |
| TXN-03 | 격리 수준별 업무 의미 | READ COMMITTED·REPEATABLE READ·SERIALIZABLE과 이상 현상 | 수준별 충돌 검출·검증·읽기/쓰기 책임 |
| TXN-04 | 쓰기 충돌과 직렬화 검증 | 동시 수정·write skew·phantom·재시도 결과 | read/write set·검증·공개 경계·경합 제어 |
| TXN-05 | Statement 원자성 | 다중 row 실패·이전 statement 보존·영향 행 수 | statement 경계·private delta·실패 rollback·예약 회수 |
| TXN-06 | Savepoint와 부분 rollback | 생성·재정의·해제·중첩·rollback 후 cursor 상태 | savepoint 이력·delta 되돌리기·자원·로그 영향 |
| TXN-07 | 잠금과 대기 | 명시 lock·SELECT FOR UPDATE·NOWAIT·대기 제한 | lock 범위·순서·wait queue·deadlock 검출·해제 |
| TXN-08 | 취소와 timeout의 거래 효과 | 실행·commit·응답 유실 시 취소 결과와 재시도 | deadline 전파·commit 경계·UNKNOWN 전달·정리 |
| TXN-09 | 공통 commit과 원자 공개 | DISK·catalog·index의 단일 거래 완료 의미 | 준비·영속화·공개·응답 경계와 불변식 |
| TXN-10 | 오래된 버전과 장기 거래 | 오래된 snapshot·pin 제한·회수 지연·자원 거절 | GC·epoch·수명 추적·admission·reclaim 경계 |
| TXN-11 | 대량 및 장기 변경 | 거래 크기 한도·부분 실패·중단·재개 가능 여부 | batch 구성·private 저장·디스크 예산 |
| TXN-12 | XA와 준비된 거래 | prepare/commit/rollback·XID·in-doubt·heuristic·복구 | 2PC 상태 영속화·TM 연계·재접속·보존·관리 |

## WAL 로그와 재시작 복구

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| WAL-01 | 내구성 실패와 ACK 계약 | 성공·실패·UNKNOWN·process crash·전원 장애 의미 | ACK 관측·sync·공개 순서·장애 경계 |
| WAL-02 | WAL 기록 단위와 형식 | record 종류·완전 batch·순서·checksum·크기 한도 | encoding·decoder·형식 식별·손상 검출·원자 단위 |
| WAL-03 | 로그 기록과 파일 관리 | append·flush·sync·rotate·공간 부족·파일 한도 | writer·segment 수명·flush 진행·오류 전파 |
| WAL-04 | Commit record와 의존성 | 확정 거래 판정·payload·page·catalog 의존 요구 | commit 표시·누락 방지·durable closure·공개 연계 |
| WAL-05 | 재시작 분석과 replay | 재실행 대상·미완료 거래·반복 recovery·서비스 개시 조건 | analysis/replay·적용 순서·멱등성·검증·실패 중단 |
| WAL-06 | 손상과 부분 기록 처리 | torn/short write·corruption·필수 영역/끝부분 구분 | checksum·길이 검사·fail-closed·진단·복구 가능 판정 |
| WAL-07 | Checkpoint와 generation | 일관된 cut·완료·실패·재시도·선택 기준 | snapshot pin·manifest·root 선택·원자 설치·검증 |
| WAL-08 | 로그와 세대 회수 | recovery·backup·reader·replication 보존 요구 | consumer pin·최저 보존점·회수 순서·공간 압력 |
| WAL-09 | Undo와 rollback 보존 책임 | statement/transaction undo·장기 거래·복제 재시작 요구 | DEV2 private 기준선·DEV3 영속 undo/WAL 분리·보존 소유자 |
| WAL-10 | Group commit과 병렬 로그 | 동시 commit 완료·순서·오류·성능 관측 요구 | L1/L2 단계별 작업 분할·durable prefix·의존 검증 |
| WAL-11 | 디스크 행·인덱스·catalog 통합 recovery | 디스크 저장 경로·index·locator·metadata의 동일 복구 cut | row/page replay·재구축·root 검증·공통 commit 연결 |
| WAL-12 | 복구 진단과 점검 도구 | 로그·checkpoint·page 검사·읽기 전용 dump | decoder 재사용 경계·손상 격리·증거 보존·민감값 처리 |

## BACKUP 백업 복원과 시점 복구

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| BACKUP-01 | 전체 물리 백업 | 일관성·온라인/오프라인·시작/완료·취소·공간 한도 | snapshot 고정·파일 수집·manifest·검증·pin 해제 |
| BACKUP-02 | 증분과 차등 백업 | level·기준 백업·chain·변경 범위·지원 조합 | 변경 추적·chain 의존·중복 제거 여부·정합성 |
| BACKUP-03 | 백업 보관과 catalog | 목록·보존·삭제·세대 종속·무결성 조회 | backup metadata·원자 등록·참조 회수·검증 |
| BACKUP-04 | Archive log와 보존 | archive 완료·누락·공간 부족·중복·운영 정책 | archive 전달·검증·retention 연동·재시작 |
| BACKUP-05 | 복원과 다른 위치 재구축 | restore 대상·기존 경로 보호·완료 확인·재개 | stage/install·파일 검증·DB identity·실패 정리 |
| BACKUP-06 | Point in time recovery | 시간/위치 목표·거래 경계·목표 이후 변경 처리 | replay 종료 판정·timeline·복구 이력·검증 |
| BACKUP-07 | 매체 장애와 손상 복구 | 지원 범위·의미·오류를 구현 전에 동결 | 손상 범위 탐지·복원 결합·실패 격리·운영 절차 |
| BACKUP-08 | 백업 복구 인수와 RPO RTO | 복원 drill·일관성·복구 시간·허용 데이터 손실 기준 | 실제 reopen·외부 기대값·측정·장애별 시험 구성 |

## STORE 저장 매체와 공간 관리

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| STORE-01 | 공통 row와 RowUID | row identity·scan·fetch·변경·삭제·재사용 의미 | 논리 row와 물리 위치 분리·locator·수명·version |
| STORE-03 | DISK row와 page | buffer보다 큰 table·variable row·overflow·접근 실패 | mutable page·row format·분할·locator·undo/allocator 수명 |
| STORE-04 | Buffer와 캐시 | capacity·pin·eviction·prefetch·사용량 관측 | buffer 동시성·I/O 요청·교체·오류·회수 |
| STORE-05 | Tablespace와 파일 | 생성·증설·상한·autoextend·online/offline·rename/drop | 파일/공간 metadata·allocation·중복 open·실패 복구 |
| STORE-06 | Extent segment와 빈 공간 | 공간 사용·반환·fragmentation·구조적 한도 | allocator·free-space 관리·안전한 재사용·계량 |
| STORE-07 | Volatile 저장 | 재시작시 소멸·트랜잭션·지원 객체·용량 | 비영속 저장 경계·transaction 연결·초기화·정리 |
| STORE-08 | 임시 table과 작업 공간 | session/transaction 범위·commit/drop·격리·spill | temp row/index·workarea·파일 수명·crash cleanup |
| STORE-09 | 압축 저장 | 압축 대상·DDL/DML·값 보존·제한·공간 효과 | 압축 단위·dictionary·codec·갱신·복구 |
| STORE-10 | Table 이동과 공간 재조직 | DISK 이동·compaction·truncate·재구성 | copy/cutover·RowUID·index 갱신·snapshot·중단 처리 |
| STORE-11 | Bulk load와 direct path | 대량 적재·commit 단위·제약·복제·오류 행 | ingest 경로·page/index 구축·WAL 연결·예산 |
| STORE-12 | 저장 검증과 한계 | row/page/index 일치·최대 크기·full·손상 관측 | scrub·불변식 검사·통계·fault 경계 |

## INDEX 인덱스와 접근 경로

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| INDEX-01 | 인덱스 키 의미 | 복합 key·정렬 방향·NULL·중복·prefix·collation | key codec·비교·동등성·정렬/검색 공유 |
| INDEX-02 | 인덱스 구조와 저장 | DISK·범위/점 접근·지원 index 종류 | 구조 선택·node/page·분할/병합·경합·복구 |
| INDEX-03 | Cursor와 scan | point/range/full·방향·일관성·종료·취소 | cursor state·snapshot·pin·재탐색·수명 |
| INDEX-04 | Unique와 primary key 경합 | NULL·동시 삽입·key 변경·오류·statement 원자성 | 예약/검증·충돌·commit·rollback·전역 일관성 |
| INDEX-05 | 인덱스 생성 삭제 재구축 | offline/online build·진행률·유효/무효·실패 | build scan·delta 반영·공개·재개·정리 |
| INDEX-06 | 함수 기반과 특수 인덱스 | 함수식·내림차순·공간·기타 기존 종류의 대체 범위 | 표현식 의존·determinism·key 생성·접근 경로 연결 |
| INDEX-07 | 색인 커버링과 통계 | index-only 가능 범위·selectivity·통계 관측 | 행 가시성 확인·통계 수집·optimizer 연계 |
| INDEX-08 | 참조 무결성 접근 | FK 조회·delete/update 참조 동작·경합 결과 | parent/child index 연결·locking·cascade·rollback |

## PART 디스크 파티션과 native global index

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| PART-01 | Range list hash 분할 | 분할 키·경계·default·NULL·routing·오류 | partition metadata·routing·snapshot membership |
| PART-02 | 파티션별 디스크 저장 배치 | 한 table의 DISK·tablespace·local index 조합 | 매체 adapter·공통 row/transaction·배치 metadata |
| PART-03 | Partition pruning | 정적/동적 조건·parameter·조인·경계 조회 | 조건 분석·pruning 정보·plan/runtime 연결 |
| PART-04 | Partition 생명주기 | 추가·삭제·truncate·split·merge·exchange·rename | membership 변경·원자 공개·의존 index·실패 회수 |
| PART-05 | Row movement | partition key 수정·매체 이동·동시 조회·제약 | RowUID/locator 변경·local/global 갱신·rollback |
| PART-06 | Local index | 분할별 index·unique 제한·생성/변경·통계 | partition/index 대응·수명·routing·rebuild |
| PART-07 | Native global index | 전 partition key 검색·global unique·저장 매체 조합 | native tree·RowUID resolver·visibility·공통 commit |
| PART-08 | Global index와 객체 변경 | split/drop/exchange/move 중 결과·유효성 | membership epoch·원자 변경·오래된 reader·회수 |
| PART-09 | 디스크 저장 복구와 백업 | 디스크 저장 경로 및 global index의 원자 복구·재개 | root/page/locator closure·checkpoint·replay 검증 |
| PART-10 | 파티션 관측과 운영 | partition별 크기·배치·index 상태·관리 권한 | 공개 catalog·통계·오류·진행률·진단 |
| PART-11 | Global index 추가 확장 판정 | index 자체 partitioning·온라인 재분할의 필요 범위 | 기존 단일 tree 경계와 확장 영향 검토 |

## SQL SQL 문법과 관계 기능

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| SQL-01 | Lexical 문법과 구문 | keyword·identifier·literal·comment·구분자·위치 오류 | lexer/parser·AST·오류 복구·입력 길이/깊이 제한 |
| SQL-02 | SELECT와 filter projection | bag·DISTINCT·별칭·표현식·NULL·빈 입력 | binding·논리 plan·결과 schema·평가 |
| SQL-03 | Join | INNER/LEFT/RIGHT/FULL/CROSS·조건·NULL·중복 | join 표현·알고리즘 선택·출력 의미·자원 |
| SQL-04 | Subquery와 lateral | scalar/EXISTS/IN·상관·lateral·cardinality 오류 | 상관 binding·실행·decorrelation·정합성 |
| SQL-05 | 집계와 grouping | GROUP BY/HAVING·빈 집합·DISTINCT·ROLLUP/CUBE/grouping sets | aggregate state·다중 grouping·NULL 구분·spill |
| SQL-06 | Window 분석 | partition/order/frame·동률·ranking·offset·경계 | window 상태·정렬·frame 평가·디스크 처리 |
| SQL-07 | 집합 정렬과 부분 결과 | UNION/INTERSECT/MINUS·ALL·ORDER BY·LIMIT/OFFSET | 중복 처리·sort/top-N·순서·종료·spill |
| SQL-08 | WITH 재귀와 계층 탐색 | CTE·재귀·CONNECT BY·cycle·depth·종료 | 재귀 실행·작업 집합·cycle 검출·자원 제한 |
| SQL-09 | Pivot와 unpivot | 행열 전환·입력 타입·누락·중복·집계 | 변환 plan·schema 계산·실행·오류 |
| SQL-10 | INSERT와 INSERT SELECT | 단일/다중 row·default·source query·영향 행 수 | 입력 평가·batch·제약·index·statement rollback |
| SQL-11 | UPDATE DELETE와 MOVE | 조건 갱신·변경 중 읽기·다중 row·매체 이동 | 대상 고정·Halloween 방지·쓰기 경계·rollback |
| SQL-12 | MERGE와 변경 결과 반환 | upsert·중복 source·RETURNING·충돌·원자성 | match 분류·쓰기 순서·반환 buffer·실패 처리 |
| SQL-13 | Prepared 실행과 parameter | DEV4 기본 prepare/bind/execute·metadata·타입·오류·수명; DEV5 protocol·DEV6 driver; DEV7 확장 SQL·고급 cache | parameter/result metadata·schema 무효화·session 소유권; 후속 공유 plan cache·재최적화 |
| SQL-14 | SQL 지원표와 오류 위치 | 문법/절/overload·조합별 지원·미지원 오류 | capability registry·진단 위치·명세/시험 대응 |

## PLAN 최적화와 실행 엔진

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| PLAN-01 | 논리 최적화 | 동등 변환·NULL·부작용·순서 유지 조건 | rewrite 규칙·전제·검증·plan 표현 |
| PLAN-02 | 통계와 cardinality | row count·histogram·분포·수집/삭제·오래된 통계 | 통계 저장·sampling·추정·schema/version 연계 |
| PLAN-03 | 비용과 plan 탐색 | scan/index·join order·디스크 비용·상한 | cost model·탐색 공간·중단·fallback·재현 |
| PLAN-04 | 물리 연산자와 iterator | 연산자별 입력/출력·중단·재시작·오류 | operator API·row/batch 흐름·소유권·pipeline |
| PLAN-05 | Join 실행 방법 | nested loop/hash/merge 업무·메모리 부족·spill | build/probe·정렬·partition spill·외부 join 상태 |
| PLAN-06 | Sort hash와 집계 작업 공간 | 정렬/중복제거/집계 용량·spill·취소 | workarea 예산·temp 파일·부분 결과·cleanup |
| PLAN-07 | 병렬 쿼리 | parallel 범위·결과 순서·취소·자원 한도 | worker 분할·exchange·집계·오류 전파·재현 |
| PLAN-08 | Plan cache와 재최적화 | 공유·invalidate·parameter·권한·통계 변경 | cache key·plan 수명·재검증·경합·회수 |
| PLAN-09 | Result cache | 캐시 가능한 query·일관성·무효화·조회 권한 | cache dependency·snapshot·크기·수명·보안 |
| PLAN-10 | EXPLAIN profile와 hint | plan 표시·실측·hint 의미·무시/오류·진단 | plan ID·operator 계측·출력 형식·hint 검증 |

## PSM 저장 프로그램과 확장 실행

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| PSM-01 | Procedure와 function | IN/OUT/INOUT·호출·반환·overload·권한 | 컴파일·호출 frame·인자 binding·return·수명 |
| PSM-02 | 제어 흐름과 예외 | block·분기·loop·label·예외·SQLCODE/SQLERRM | 언어 중간 표현·제어 흐름·예외 stack·source 위치 |
| PSM-03 | Cursor와 REF CURSOR | open/fetch/close·속성·반환·거래 종료 영향 | cursor handle·query 연결·소유권·close 정리 |
| PSM-04 | Record 배열과 TYPESET | 사용자 타입·associative array·VARRAY·한도 | 타입 metadata·값 표현·수명·binding·복사 |
| PSM-05 | Dynamic SQL | EXECUTE IMMEDIATE·OPEN FOR·인자·결과·권한 | 동적 parse/bind·plan 연결·진단·자원 |
| PSM-06 | Package와 session 상태 | spec/body·초기화·전역 변수·재컴파일·의존 | package state·invalidation·session 수명·동시성 |
| PSM-07 | Autonomous transaction | 상위 거래와 독립 commit·가시성·충돌·실패 | 중첩 거래 컨텍스트·잠금·반환·복구 |
| PSM-08 | 내장 관리 package | DBMS_STATS/METADATA/SQL/LOCK/ALERT 등 routine별 기능 | 공개 routine 분류·엔진 연결·권한·자원·오류 |
| PSM-09 | 내장 파일과 통신 package | UTL_FILE/TCP/SMTP/RAW·파일/TCP 처리·취소·제한 | 외부 effect 경계·허용 경로/대상·timeout·cleanup |
| PSM-10 | 프로그램 변경과 보호 | compile 진단·온라인 교체·wrap·source 표시 권한 | dependency·실행중 code 수명·metadata·보호 형식 |

## OBJECT Trigger sequence job queue

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| OBJECT-01 | Trigger 정의와 실행 | timing·row/statement·조건·순서·재귀·오류 | trigger 등록·DML 연결·실행 컨텍스트·rollback |
| OBJECT-02 | Sequence | NEXTVAL/CURRVAL·cache·cycle·gap·rollback·상한 | 발급·예약·동시성·영속화·복제·restart |
| OBJECT-03 | Identity와 default 평가 | 자동 생성 값·default·호출 시점·명시값·변경 | insert 평가·sequence 의존·row/batch 일관성 |
| OBJECT-04 | Job scheduler | 등록·주기·실행 권한·실패·중지·중복 실행 | 일정·worker·상태 저장·retry·shutdown·관측 |
| OBJECT-05 | Transactional queue | ENQUEUE/DEQUEUE·commit·가시성·순서·재전달 | queue 저장·consumer 대기·claim·rollback·복구 |
| OBJECT-06 | 알림과 프로세스 간 업무 동기화 | alert·lock·concurrent execution의 업무 능력 | session/transaction 연결·대기·권한·해제·한도 |

## SPECIAL LOB와 공간 데이터

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| SPECIAL-01 | LOB 타입과 값 의미 | BLOB/CLOB·NULL/empty·길이·비교·변환·한도 | LOB 표현·inline/out-of-line·codec·catalog |
| SPECIAL-02 | LOB 부분 접근과 streaming | offset·read/write/append/truncate·locator·취소 | chunk 저장·buffer·client 흐름·locator 수명 |
| SPECIAL-03 | LOB 원자성과 운영 조합 | rollback·snapshot·backup·복제·적재·삭제 | version·WAL·회수·복제 이벤트·fault 경계 |
| SPECIAL-04 | Geometry 타입과 교환 | geometry 종류·WKT/WKB·SRID·invalid/empty·한도 | geometry 모델·parser/serializer·검증·codec |
| SPECIAL-05 | 공간 함수와 관계 | 거리·면적·관계·분석·생성·정밀도·예외 | geometry 알고리즘 검토·robustness·독립 vector |
| SPECIAL-06 | 공간 인덱스와 plan | 공간 조건·필터와 정확 판정·DML·지원 조합 | index 구조·bounding 처리·정확 재검사·optimizer |
| SPECIAL-07 | 공간 API와 파일 적재 | client geometry·shapefile·encoding·SRID·오류 행 | binding·변환·loader pipeline·진단·정리 |

## SERVER 서버 세션과 실행 제어

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| SERVER-01 | DB 생성과 server 상태 | create/start/open/stop·정상/즉시 종료·실패 상태 | bootstrap·서비스 조립·단계별 readiness·자원 정리 |
| SERVER-02 | DB 경로와 프로세스 소유 | 중복 open·PID/run identity·권한·데이터 경로 | process/file lock·소유 자원·stale 판정·cleanup |
| SERVER-03 | 접속과 session 수명 | connect/disconnect·최대 session·idle·상태 초기화 | session owner·worker 연결·거래 정리·반환 |
| SERVER-04 | Statement와 cursor 수명 | prepare/execute/fetch/close·재사용·동시 호출 | statement state·result ownership·session 정리 |
| SERVER-05 | Admission과 workload 제어 | 접속/query 제한·대기·거절·관리 우선권 | queue·budget·worker 할당·backpressure·통계 |
| SERVER-06 | Session 설정과 transaction context | ALTER SESSION·NLS·schema·격리·변경 시점 | 설정 snapshot·검증·전파·pool reset·plan 영향 |
| SERVER-07 | 질의 취소와 관리자 종료 | cancel/kill·timeout·disconnect·commit 경계 결과 | 취소 경로·실행 interrupt·권한·정리·UNKNOWN |
| SERVER-08 | Background 작업 | GC·checkpoint·통계·정리의 실행/정지·진행 관측 | 작업 lifecycle·retry·shutdown join·예산 |
| SERVER-09 | 복구와 서비스 개시 | recovering/ready/degraded·접속 거절·운영 접근 | startup gate·무결성 확인·readiness·실패 진단 |

## NET 통신과 wire protocol

[COMM 상세 설계](../contracts/comm-design.md)와 [구현·검증 계획](comm-implementation-plan.md)이 NET-01, NET-02/06의 local C1 범위를 구체화한다. NET-03/04/05/07의 업무 protocol과 NET-08 TLS는 후속 계약·인수가 필요하며 이번 작성으로 전체 NET 명세·제품 구현이 완료되지는 않는다.

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| NET-01 | Frame과 메시지 형식 | frame 한도·fragmentation·순서·잘못된 입력 | codec·length 검증·decoder state·buffer 소유 |
| NET-02 | 로컬과 원격 transport | Unix socket·TCP·연결·끊김·주소·timeout | transport 상태·I/O scheduling·자원 수명 |
| NET-03 | Handshake와 capability | 현재 build 기능 협상·인증·지원 불일치 오류 | protocol state·capability 표현·버전 경계 |
| NET-04 | SQL 요청 응답과 결과 전송 | execute/fetch·metadata·값·행 수·오류·종료 | message sequence·result batching·상태 일치 |
| NET-05 | Binding과 대용량 전송 | NULL·array·LOB·길이·부분 입력·오류 위치 | chunk·flow control·메모리 상한·취소 cleanup |
| NET-06 | Backpressure와 느린 client | 지연·buffer 상한·timeout·공정성·중단 | 송수신 queue·read/write throttle·quota 해제 |
| NET-07 | Cancel와 연결 유실 | 요청 식별·응답 유실·재접속·UNKNOWN | cancel channel·request/session 매핑·정리·재전송 경계 |
| NET-08 | TLS 채널 | 인증서 검증·cipher 정책·접속 실패·교체 요구 | TLS adapter·handshake·secret 수명·오류·관측 |

## CLIENT Client API와 driver

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| CLIENT-01 | Native client API | connection/statement/result·binding·metadata·오류·thread 안전 | handle·소유권·비동기/동기 경계·protocol 연결 |
| CLIENT-02 | C ABI와 메모리 계약 | 문자열·buffer·호출 순서·free·callback·ABI 표면 | FFI layout·panic 경계·자원 소유·안전 wrapper |
| CLIENT-03 | JDBC 핵심 API | Connection/Statement/ResultSet·타입·batch·진단 | Java driver·protocol·exception·자원·빌드 |
| CLIENT-04 | JDBC 확장과 pooling | metadata·LOB·generated keys·pool/XA·취소·failover | API별 capability·lifecycle·JTA·통합 시험 경계 |
| CLIENT-05 | ODBC와 CLI | API·SQLGetInfo·descriptor·SQLSTATE·driver manager | driver handle·C 타입 변환·binding·진단·설치 |
| CLIENT-06 | 배열과 bulk binding | row/column binding·NULL indicator·부분 성공·반환 | batch 분할·status array·transaction 단위·예산 |
| CLIENT-07 | Embedded SQL과 precompiler | host variable·cursor·WHENEVER·SQLCA·빌드·오류 위치 | precompile·생성 코드·runtime API·source mapping |
| CLIENT-08 | PHP PDO와 Perl 연결 | 기존 언어별 transaction·type·cursor·오류 업무 | 공용 driver 활용·adapter·설치·runtime 대응 |
| CLIENT-09 | .NET 연결 | ADO.NET·pool·bulk copy·schema·transaction·async 범위 | provider·binding·lifecycle·배포·검증 |
| CLIENT-10 | Python Node와 기타 언어 연결 | 사용 API·encoding·pool·취소·지원 버전 범위 | 직접 driver/공용 adapter 선택·통합·배포 |
| CLIENT-11 | Framework 상호운용 | ORM·application server·BI·connection pool별 업무 | capability 탐지·SQL/metadata 차이·통합 corpus |
| CLIENT-12 | Client 장애와 다중 주소 | connect failover·session failover·재시도·콜백·거래 보존 범위 | 주소 선택·state 복구·UNKNOWN·중복 실행 방지 경계 |

## SEC 보안과 감사

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| SEC-01 | 계정과 인증 | 사용자·password·만료·잠금·실패 횟수·관리 접속 | credential 저장·검증·정책·session 연결 |
| SEC-02 | 외부 인증과 directory 연계 | LDAP 등 기존 연동 업무·실패·fallback·지원 범위 | 인증 adapter·외부 timeout·secret·사용자 매핑 |
| SEC-03 | 권한과 role | system/object 권한·grant/revoke·role·routine/view 권한 | authorization 검사·catalog·plan invalidation·감사 |
| SEC-04 | 통신 보안과 인증서 운영 | TLS 설정·CA·만료·교체·client 검증·진단 | certificate lifecycle·reload·접속 정책·관측 |
| SEC-05 | 데이터 보호와 보안 모듈 | column/data 보호·외부 module·백업/복제 영향 | 암호 경계·키 관리·provider·데이터 노출 통제 |
| SEC-06 | 감사 정책 | 접속·SQL·객체 접근·성공/실패·조건·권한 | audit hook·event 정의·수집·부하·실패 정책 |
| SEC-07 | 감사 기록 보관과 조회 | 검색·export·회전·보존·삭제 권한·무결성 | 저장·전달·retention·검증·도구 연계 |
| SEC-08 | 진단과 비밀정보 보호 | SQL/값/password/key의 로그·dump·bundle 표시 정책 | redaction·권한·접근 기록·최소 수집 |
| SEC-09 | 외부 코드와 파일 접근 제어 | library·file·network 호출의 권한·제한·실패 영향 | 격리·허용 대상·자원 예산·OS 경계 |

## REPL 복제와 고가용성

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| REPL-01 | 복제 topology와 객체 | 단일/다중 writer·대상 table·노드·시작/정지·권한 | replication metadata·역할·상태·제어 경계 |
| REPL-02 | 변경 추출과 commit stream | committed 순서·대상 변경·schema epoch·filter | engine stream port·capture·거래 조립·ownership |
| REPL-03 | 전송과 receiver apply | 전송/수신/반영 진행·중복·재접속·부분 실패 | protocol·apply transaction·재시도·순서·ACK |
| REPL-04 | 동기와 비동기 복제 | 성공 응답·remote durable/apply·장애·지연 의미 | ACK boundary·timeout·local commit 연결·상태 전이 |
| REPL-05 | 초기 동기화와 재동기화 | online sync·부분 대상·일관 cut·재개·오류 | snapshot+stream 연결·진행점·중복·검증 |
| REPL-06 | 복제 재시작과 로그 보존 | capture/apply restart·long transaction·원본 로그 회수 | durable 인계·restart pin·spool·retention·복구 |
| REPL-07 | 충돌 탐지와 해소 | 중복 key·수정/삭제 충돌·policy·결과·감사 | conflict 분류·해소 적용·loop 방지·관측 |
| REPL-08 | DDL sequence와 특수 타입 복제 | schema 변경·sequence·LOB·partition·지원 조합 | schema epoch·order·이벤트 표현·재적용·검증 |
| REPL-09 | 복제 병렬 처리 | transaction 순서·의존·처리량·backpressure | apply 분할·dependency·commit 순서·예산 |
| REPL-10 | Failover failback와 fencing | 역할 전환·분할망·구 primary·재편입·RPO/RTO | 장애 감지·승격·writer 차단·복귀·증거 |
| REPL-11 | 접속 전환과 heartbeat | 다중 주소·전환 callback·서비스 연속성·제약 | client/server 역할 연계·health·전환 상태 |
| REPL-12 | 복제 관측과 관리 | lag·충돌·retention·상태·작업·경보·관리 도구 | 공개 view·metric·명령·진행률·이력 |

## LINK 외부 연계와 변경 구독

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| LINK-01 | DB link 객체와 연결 | 원격 대상·credential·session·연결 공유·오류 | link catalog·connection pool·자원·보안 |
| LINK-02 | 원격 조회와 실행 | remote table·function·binding·batch·타입·결과 | remote plan·pushdown·변환·fetch·취소 |
| LINK-03 | 원격 거래와 불확정 처리 | commit/rollback·분산 실패·force 처리·제약 | remote transaction state·recovery·관리·UNKNOWN |
| LINK-04 | 외부 procedure와 function | C/C++ routine·인자/결과·exception·지원 타입 | 호출 ABI·agent 격리·crash·timeout·resource cleanup |
| LINK-05 | 변경 분석과 구독 API | Log Analyzer 대응 이벤트·metadata·ACK·재시작·지원 변경 | 구독 protocol·cursor·보존·변환·소비자 상태 |
| LINK-06 | JDBC와 Oracle adapter 업무 | 이종 DB 변경 전달·타입·충돌·재개·지원 대상 | capture/transform/apply·transaction·오류 queue |
| LINK-07 | Hadoop과 데이터 교환 | batch/분산 입출력·분할·타입·실패·중복 | connector·split·schema 전달·재시도·검증 |
| LINK-08 | 연계 생태계 지원표 | 외부 제품/버전·인증·schema·transaction 조합 | adapter 계약·통합 corpus·배포·지원 경계 |

## SHARD 샤드와 분산 기능의 범위 판정

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| SHARD-01 | 샤드 업무와 단일 DB 범위 대조 | 기존 shard 기능·고객 업무·XADB 포함/대체/별도제품 판정 | 단일 DB 기준과 분산 확장의 경계·영향 검토 |
| SHARD-02 | 샤드 metadata와 routing | node·shard key·range/list/hash·replica set 요구 | metadata 소유·routing·cache·갱신·실패 경계 검토 |
| SHARD-03 | 분산 SQL과 거래 | node 간 query·join·aggregate·commit·오류 요구 | 실행/거래 coordination·일관성·중단 검토 |
| SHARD-04 | 샤드 재배치와 운영 | 분할·이동·재배치·resync·장애·관측 요구 | migration 상태·routing 전환·복구·관리 검토 |
| SHARD-05 | 샤드 client와 관리 API | client routing·metadata 갱신·관리 procedure 업무 | driver/server 경계·API·호환 아닌 기능 대응 검토 |

## OPS 설정 운영 정보와 진단

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| OPS-01 | 설정 항목 사전 | 속성별 이름·타입·기본값·범위·단위·scope·적용 시점 | 등록·검증·우선순위·snapshot·atomic reload |
| OPS-02 | 공개 운영 view | session·transaction·lock·storage·replication·metric 열 | 관측 snapshot·일관성·권한·부하·형식 관리 |
| OPS-03 | 통계 metric과 wait event | counter/gauge·단위·reset·wait 분류·샘플링 | 계측 지점·동시 갱신·수집·공개·비용 |
| OPS-04 | 메시지 로그와 회전 | severity·error ID·시간·파일·rotate·보존·설정 | 로그 pipeline·backpressure·실패·redaction |
| OPS-05 | 추적과 원인 진단 | query/transaction/object 인과·incident·관측 한계 | correlation·event lineage·bundle·원인 후보 추적 |
| OPS-06 | Health와 모니터링 API | liveness/readiness·metric 구독·poll·오류·인증 | API·snapshot·rate limit·외부 exporter |
| OPS-07 | SNMP와 외부 관리 연동 | MIB/metric·trap·운영 도구·directory 연계 업무 | adapter·mapping·인증·설치·통합 |
| OPS-08 | 운영 CLI와 관리 화면 | 상태·목록·작업 시작/중지·진행·오류·기계 출력 | command/API·비동기 작업·권한·화면 연계 범위 |
| OPS-09 | 성능 진단과 profile | 실행 plan·대기·CPU/I/O·top query·시계열 비교 | sampling·operator profile·비용 제한·민감값 보호 |
| OPS-10 | 장애 수집과 support bundle | 수집 대상·크기·읽기 권한·재현 입력·보존 | manifest·binary/symbol identity·redaction·export |
| OPS-11 | 에러 설명과 운영 절차 | 오류별 원인·재시도·조치·지원 정보·언어 | error registry·문서 생성·CLI 연결·검증 |

## TOOLS SQL 도구 적재 이관과 비교

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| TOOLS-01 | 대화형 SQL CLI | 접속·입력·명령 history·표시·편집·취소 | shell과 SQL 경계·result 표시·session·자원 |
| TOOLS-02 | Batch SQL CLI | script·변수·delimiter·출력·exit code·오류 중단 | 입력 처리·statement 경계·stdout/stderr·재현 |
| TOOLS-03 | Schema export | 객체·권한·의존 순서·선택·복원 가능 표현 | catalog reader·DDL 생성·dependency·검증 |
| TOOLS-04 | 데이터 unload load | text/binary·delimiter·encoding·NULL·LOB·병렬·오류 행 | stream pipeline·commit 단위·재개·bulk·검증 |
| TOOLS-05 | Loader API | library handle·options·callback·취소·thread 안전 | ABI·pipeline 공유·resource lifetime·진단 |
| TOOLS-06 | 이종 DB 이관 | schema/type/SQL/PSM 차이·변환·손실·지원 범위 | 분석·변환 규칙·적재·검증·예외 보고 |
| TOOLS-07 | 데이터 비교 | 키·타입·NULL·float·snapshot·차이 출력 | chunk 비교·독립 normalization·대량 scan·진단 |
| TOOLS-08 | 선택 동기화 | 불일치 해소 방향·충돌·dry run·원자성·취소 | 변경 계획·적용·재검증·이력·권한 |
| TOOLS-09 | 전환과 결과 검증 | 이관 cutover·잔여 변경·검증·rollback 업무 | 전환 절차·증거·정합성 확인·운영 조정 |
| TOOLS-10 | 관리와 dump 도구 묶음 | altierr/passwd/profile/wrap/checkServer/dump 계열 대응 | 공용 API/decoder·읽기 전용·보안·packaging |
| TOOLS-11 | 서비스와 컨테이너 운영 도구 | start/stop/health·service·container/Kubernetes 업무 범위 | lifecycle adapter·설정·영속 volume·복제 연계 |

## PERF 성능 용량과 운영 품질

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| PERF-01 | 대표 workload와 기준 결과 | DEV2 typed API부터 SQL·서버·전체 workload로 확장; 데이터·분포·기준 결과 | 현재 capability의 generator·독립 검산·고정 corpus·recipe identity |
| PERF-02 | 지연 처리량과 동시성 | API·WAL·복구부터 throughput·latency·동시성·측정 구간·계측 비용 | K0–K5 소유 모듈 계측·XADB-032 비교 harness·sampling·분산·간섭 통제 |
| PERF-03 | 구조적 한도 | 객체 수·column/key/row 크기·세션·transaction·파일 상한 | 한도 검증·경계값·거절 경로·표현/저장 일관성 |
| PERF-04 | 큰 데이터와 유한 자원 | RAM 초과 데이터·index·temp·backup·복제 buffer | peak 계량·budget·spill·backpressure·회수 |
| PERF-05 | 혼합 및 운영중 workload | DDL·backup·checkpoint·복제·load·query 병행 | 경합 시나리오·우선순위·관측·복구 영향 평가 |
| PERF-06 | 장시간 안정성 | soak·누수·자원 누적·재시작·성능 저하 | 장시간 harness·주기 작업·trend·회수 검증 |
| PERF-07 | 성능 회귀 판정 | DEV2부터 비교 유효성·허용치·분산·사전 등록·독립 확인; DEV8 장시간 회귀 | baseline/evidence·거짓 개선 거절·독립 비교·새 세션 replay |
| PERF-08 | 서비스 수준과 복구 목표 | 기능별 SLA·RPO/RTO·자원 환경·지원 범위 | 측정 조건·장애/전환 drill·결과 판정 |

## TEST 검증 기반과 개발 진단

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| TEST-01 | XADBTest 시나리오와 verdict | typed case·PASS/FAIL/UNKNOWN·timeout·증거 요구 | controller/worker protocol·검증·소유 자원·receipt |
| TEST-02 | 독립 oracle와 reference | row/transaction/SQL 기대값·known-bad 거절 | production 의미 계산과 분리·reference model·corpus |
| TEST-03 | Fault와 crash 재현 | I/O·allocation·cancel·process kill·power loss 모델 | fault ports·schedule·cut point·ACK history·reopen |
| TEST-04 | 동시성 history 검증 | snapshot·conflict·lock·serializability·불확정 결과 | actor/barrier·history recorder·검증기·최소화 |
| TEST-05 | 기능별 인수 행렬 | 정상·경계·오류·권한·취소·거래·복구 축 | 명세 ID→case→실제 경로→증거 추적 |
| TEST-06 | 표현식과 parser 생성 시험 | 타입/함수/문법의 valid/invalid 입력·예산 | fuzz/property·독립 vector·최소 반례·재현 |
| TEST-07 | 실제 API SQL driver 통합 | engine→embedded SQL→server→native/JDBC/ODBC 동일 업무 | 각 단계 harness·공통 입력·독립 관측·회귀 |
| TEST-08 | 재현 bundle과 minimization | K0부터 seed·schedule·입력·binary·증거·재실행; 자동 축소는 K5 이후 | XADB-014a manifest·XADB-032 replay를 먼저, XADB-014b 자동 최소화를 후속 연결 |
| TEST-09 | AI 작업과 리뷰 흐름 | 작업 계약·소유 경로·공유 변경·oracle 보호·검토 증거 | context pack·review view·변경/시험 추적·통합 절차 |
| TEST-10 | CI와 증거 승격 | required check·보호 기준·신뢰 수준·결과 보존 | job 분리·artifact·source fingerprint·판정·promotion |
| TEST-11 | 전체 기능 조합 인수 | 매체×SQL×driver×복제×권한×운영의 필수 조합 | risk 기반 조합·누락 추적·대체 완료 판정 |

## RELEASE 설치 배포 버전과 사용자 문서

| 항목 | 주제 | 기능 명세 | 설계 |
| --- | --- | --- | --- |
| RELEASE-01 | 지원 환경과 배포 단위 | Linux 배포판·CPU·kernel·FS·장치·runtime·package | build target·qualification matrix·recipe·manifest |
| RELEASE-02 | 설치 제거와 초기 설정 | 권한·경로·service·secret·uninstall·기존 DB 보호 | installer·preflight·파일 소유·rollback·smoke |
| RELEASE-03 | 재현 build와 공급 의존 | toolchain·dependency·license 목록·binary/symbol identity | lockfile·SBOM·소스 hash·clean build·배포 검증 |
| RELEASE-04 | 사내 배포와 새 구축 | stage/capability·fresh DB/fixture·기존 사내 버전 비호환 | 사내 manifest·초기화·현재 build 복구/원자성 검증 |
| RELEASE-05 | 안정 릴리스 버전 계약 | 같은 major 이전 고객 버전의 단방향 호환 범위 | format/protocol/API 변경·변환·baseline fixture·검증 |
| RELEASE-06 | 안정 출시와 capability 인수 | 필수 기능·한도·성능·운영·문서·미결정 해소 | qualification gate·기능 대응·증거 묶음·출시 manifest |
| RELEASE-07 | 사용자와 개발자 매뉴얼 | SQL·타입·API·오류·관리·복구·보안·튜닝·이관 | 명세 기반 문서 구성·예제·교차 참조·지원표 |
| RELEASE-08 | 운영 runbook과 release note | 장애·backup/restore·전환·patch·변경·제약 | 절차·변경 추적·시험된 예제·지원 정보 연결 |

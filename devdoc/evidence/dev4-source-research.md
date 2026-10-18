# DEV4 소스·Oracle/ARIES 조사 기록

2026-10-18 · **실제 checkout·선택 코드 열람 / 외부 코드 도입·DBMS 실행·XADB Q1 검증 아님**

[로컬 소스 catalog](../reference/opensource-references.md) · [SQL 설계](../contracts/dev4-sql-design.md) · [구현·검증 계획](../plans/dev4-implementation-plan.md)

## 조사 질문과 실제 revision

조사자는 `/root`다. SQL 분석/실행 분리, prepared 값·metadata 수명, schema 변경 뒤 실행, statement 실패 효과를 비교했다. 각 checkout에서 `remote get-url origin`, `rev-parse HEAD`, `status --short --untracked-files=no`를 실행했다. 아래 4개 모두 tracked 변경은 없었다. untracked 파일·전체 DBMS 기능·빌드 환경을 감사한 기록은 아니다. 원본 checkout은 수정하지 않았다.

| checkout | 실제 HEAD | origin |
| --- | --- | --- |
| `/home/et16/work/postgres` | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | [공식 소스](https://github.com/postgres/postgres) |
| `/home/et16/work/mariadb` | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | [공식 소스](https://github.com/MariaDB/server) |
| `/home/et16/work/cubrid` | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | [공식 소스](https://github.com/CUBRID/cubrid) |
| `/home/et16/work/sqlite` | `43a48f77585a88940c727f33682151418f544059` | [공식 소스](https://github.com/sqlite/sqlite) |

## 실제 열람 범위

line 범위는 위 revision의 관측 위치이고, hash는 부분 발췌가 아닌 해당 파일 전체 bytes의 SHA-256이다. 검색 결과만으로 기능 전체를 분석했다고 주장하지 않는다. 아래 범위의 코드를 실제 읽었으며 복사/빌드/benchmark는 수행하지 않았다.

| ID | 파일·열람 line | 관측한 책임 | 파일 SHA-256 |
| --- | --- | --- | --- |
| R1 | [postgres/src/backend/parser/analyze.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/parser/analyze.c) · 109–185 | parse_analyze_fixedparams/varparams: raw parse tree에서 분석된 Query로 분리, parameter 타입 검사 | `93eaf63c0f60bc3ca2562f528d81bc6bef9177751273791e26c35f9b6f93e80c` |
| R2 | [postgres/src/backend/parser/parse_param.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/parser/parse_param.c) · 128–255 | variable_paramref_hook/variable_coerce_param_hook: 미정 타입의 문맥 해석, 일관되지 않은 parameter 타입 거절, 길이 검사는 별도 | `3e2c67e7594ac573b14165e518fc4f378ad2c9c4006a1a78cdd1b9307b13a9e9` |
| R3 | [postgres/src/backend/utils/cache/plancache.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/utils/cache/plancache.c) · 679–750, 860–905 | RevalidateCachedQuery: 환경/metadata 유효성 재확인, result descriptor 변경 시 fixed_result 거절 | `51b5941f39cd91b7de11c9701cd4decfd173a3681f05b2be8c013ea21a84bbaf` |
| R4 | [mariadb/sql/sql_prepare.cc](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql/sql_prepare.cc) · 20–100, 4790–4927 | prepared lifecycle 설명과 execute_loop: prepare metadata와 실행 binding 분리, metadata invalidation 뒤 제한된 reprepare 시도 | `1c83c960cac5f4e6be24132045e1d9afd9eed4fd374b464c9245d70a7c670fbd` |
| R5 | [cubrid/src/query/execute_statement.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/query/execute_statement.c) · 4074–4245 | do_prepare_statement/do_execute_statement: 문장 종류별 prepare/execute dispatch, 질의 plan과 DDL/transaction control 경계 | `92ada15819057a15438a502a88578e354be1675de227a728aae33c55fe904d0d` |
| R6 | [sqlite/src/vdbeapi.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbeapi.c) · 120–165, 1730–1835 | sqlite3_reset/clear_bindings와 bindText: reset/bind 수명 구분, busy 상태 binding 거절, byte 길이·destructor 책임 | `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5` |
| R7 | [sqlite/src/vdbeaux.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbeaux.c) · 3240–3305, 3450–3505 | statement savepoint release/rollback과 전체 transaction rollback 경계, 실패 시 change count 처리 | `b5861fefc82cb754987635deab85abc4062883200202e08ff609f185af91bae6` |

## XADB에 반영한 판단과 한계

| 질문 | 비교에서 얻은 입력 | XADB 선택·필수 검증 |
| --- | --- | --- |
| 분석/실행을 어디서 나누는가 | R1/R5는 parse 결과를 분석/plan으로 준비하고 문장 종류에 따라 실행 경로를 구분 | parser→binder→plan→executor와 direct DDL/transaction control. SQL 없는 engine 경계 유지; D4-T01/03/13 |
| parameter 타입과 길이를 언제 고정하는가 | R2는 타입 문맥과 길이 의미가 별도 책임임을 보여 줌. R4는 prepare metadata와 실행 값 전달이 분리됨 | `?` 순번, prepare 타입 고정·불명 hint 요구, bind_all의 원자 소유·목적 열 길이/NULL 검사. PostgreSQL `$n` 문법·cast나 MariaDB의 타입 변경을 채택하지 않음; D4-T02/07 |
| metadata가 바뀐 statement를 어떻게 재사용하는가 | R3는 validity·결과 descriptor, R4는 prepare와 execute 사이 metadata 변경과 재시도 상한을 다룸 | 동일 실행 snapshot에서 identity/schema/index dependency 대조 후 명시 StalePlan. 자동 reprepare·공유 cache를 보류; D4-T08/09 |
| bind와 result 수명을 어떻게 노출하는가 | R6는 busy binding과 reset/clear의 차이, destructor와 길이를 가진 buffer 처리를 보여 줌 | owned binding·명시 result close·EOF·session당 한 result. SQLite reset API·C buffer ABI를 그대로 가져오지 않음; D4-T07/08/11 |
| statement 실패를 거래 전체 실패와 구별하는가 | R7에서 statement rollback과 전체 transaction rollback은 별도 경로이며 오류 승격이 존재 | 최초 선택은 DEV2/DEV3 statement mark·당시 immediate unique·UNKNOWN 사용. unique 검사 시점은 후속 DEV4-REVIEW-002에서 현행 DEV3의 statement-final로 대체. SQLite conflict mode나 저장 구조는 채택하지 않음; D4-T04/09/10 |

외부 소스에서 XADB의 문법/수치·자원 상한·오류 코드·내구성 증명을 도출한 것은 아니다. 이들은 XADB 소유 계약과 D2 판단으로 정하고 실제 구현 시험을 별도로 요구한다. parser 알고리즘·외부 sorter·전체 optimizer에 대한 외부 소스 전수 조사는 수행하지 않았으며 그 설계 선택의 근거는 제한된 기능과 bounded 자원 요구다. 라이선스가 다른 소스를 XADB에 편입하지 않았고 코드 도입이 필요해지면 DP04의 출처·license·notice 검토를 적용한다.

<a id="oracle-aries"></a>
## Oracle·ARIES 후속 리뷰 입력

2026-10-18 · 조사 `/root`, 별도 검토 `/root/dev4_oracle_review`. 사용자의 Oracle 친화성 리뷰와 수정 요청에 따라 추가 확인했다. 앞선 R1–R7의 최초 조사 identity와 실행 범위는 유지한다. Oracle은 **19c 문서**를 기준으로 비교하며 실제 Oracle DB 실행은 하지 않았다. `/root`는 ARIES의 공식 초록·서지를, 별도 검토자는 1992년 원 논문의 대학 공개 사본 PDF 1·4–6쪽(인쇄 94·97–99쪽)을 읽었다. 전문 전체 재검증이나 Oracle과 ARIES 구현 동일성의 증거로 사용하지 않는다.

| 입력·실제 읽은 부분 | 확인한 의미 | XADB 반영·한계 |
| --- | --- | --- |
| [Oracle 19c Object Names](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Database-Object-Names-and-Qualifiers.html), 미인용/인용 이름 | 미인용 이름 대문자와 인용 이름 대소문자 구분 | DEV4 ASCII 대문자 채택·metadata/quoted vector. Oracle 전체 문자 집합·사용자 schema는 미구현 |
| [Oracle 19c Nulls](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Nulls.html), 길이 0 문자·비교 | 해당 버전에서 빈 character는 NULL, 일반 NULL 비교는 UNKNOWN | SQL Utf8 literal/bind 입력만 타입 확인 뒤 NULL 정규화. native empty·Binary를 변경하지 않으며 future Oracle 동작을 예측하지 않음 |
| [Oracle 19c constraint](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/constraint.html), Unique Constraints | 전체 NULL은 중복 가능, 동일 부분 NULL 복합 키는 중복 거절 | 현행 DEV3 정본과 DEV4·독립 vector 일치. Oracle의 NULL index 생략 등 물리 배치는 채택하지 않음 |
| [Oracle 19c Data Integrity](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/data-integrity.html), Nondeferrable Constraints | 기본 제약은 문장 종료 경계에서 검사 | DEV3 statement-final UNIQUE 소비. transaction 종료까지의 deferred constraint는 범위 밖 |
| [Oracle 19c COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html), implicit commit 조건 | 문법적으로 유효한 DDL 전 commit, 성공 DDL 후 commit | 지원 문법에 한해 SQL이 이전 거래 commit→native 독립 DDL로 조립. semantic 오류 전 commit과 두 outcome를 명시; unsupported/진입 거절은 commit하지 않음 |
| [Oracle 19c Concurrency](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/data-concurrency-and-consistency.html), read consistency·Read Committed | 기본 READ COMMITTED는 문장 시작 시점 일관 읽기와 행 단위 쓰기 경합 | DEV4는 DEV3의 table 단위 Serializable을 소비. native RC 설계의 실제 공급 검증 뒤 SQL 노출을 결정하며 잠금 대기 뒤 전체 abort의 차이를 명시 |
| [Oracle 19c Process Architecture](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/process-architecture.html), DBW·LGWR and Commits | data page 쓰기 전 관련 redo 영속화, commit 성공과 data page 기록 시점 분리 | 작업 중 갱신된 DEV3-DESIGN-003의 WAL-before-page·no-force commit 공급 계약으로 정합화. SQL은 page flush/redo를 소유하지 않음 |
| [ARIES, Mohan 외, 1992, DOI 10.1145/128765.128770](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging), 공식 초록·서지와 [원 논문 공개 사본](https://people.eecs.berkeley.edu/~prabal/teaching/resources/eecs582/mohan92aries.pdf) PDF 1·4–6쪽 | WAL, page LSN, repeating history, redo-only CLR와 rollback 연결·반복 장애 | DEV3의 undo/CLR·checkpoint/retention 선행 검증과 SQL R1–R4 연결. ARIES의 WAL 역방향 undo chain과 DEV3 영속 UndoRef를 구별하고 동일 알고리즘/실행 증거로 주장하지 않음 |

공개 소스의 보상 로그 동작도 실제 읽었다. `/home/et16/work/cubrid`의 origin은 `https://github.com/CUBRID/cubrid.git`, HEAD는 `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5`이며 tracked 변경은 없었다. [src/transaction/log_recovery.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/transaction/log_recovery.c)의 270–330, 4579–4665, 4900–4955행을 읽었다. undo의 compensate 기록과 `LOG_COMPENSATE`의 redo-only 처리·`undo_nxlsa` 연결을 확인했다. 파일 전체 SHA-256은 `346275b089af2ff3231b3eb224fb6a7d8e9e5811e958762980dc11b978d28058`이다. 빌드·실행·코드 도입은 하지 않았다.

선택의 정본은 [DEV4-REVIEW-002](../contracts/dev4-sql-design.md#oracle-review), 반례는 D4-V17–24와 [R1–R4](../plans/dev4-implementation-plan.md#rollback-recovery)다. 이 조사와 설계 채택은 XADB의 내구성·Oracle differential·Q1 PASS 증거가 아니다.

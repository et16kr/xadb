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

<a id="work-breakdown-20261022"></a>
## 2026-10-22 — DEV4 세부 작업 분류 조사

**선택 소스·원문 열람 / 작업 분류 근거 / 제품·외부 DBMS 시험 NOT_RUN.** 조사자는 `/root`다. 사용자의 DEV4 분류 요청에 따라 기존 설계를 [18개 작업과 공급·인수 단위](../plans/dev4-work-breakdown.md#catalogue)로 나누기 위해 front end, prepared 수명, statement/transaction 종료, sort/spill, 복구 수신 경계를 다시 읽었다. 앞선 2026-10-18 조사·독립 검토의 identity는 그대로 보존하며 이번 열람을 과거 설계 채택의 새로운 실행 증거로 대입하지 않는다.

### 실제 checkout·열람 범위

각 checkout의 `remote get-url origin`, `rev-parse HEAD`, `status --short --untracked-files=no`를 확인했다. 아래 네 checkout의 tracked 변경은 없었으며 원본 checkout을 수정·빌드·실행하지 않았다. untracked 파일이나 전체 DBMS 기능의 감사는 아니다. catalog의 `/home/et16/work/duckdb`는 이번 환경에서 경로가 없어 열람 대상에 포함하지 않았다. 나머지 catalog 전체를 조사했다고 주장하지 않는다.

| checkout | 실제 HEAD | 확인한 origin |
| --- | --- | --- |
| `/home/et16/work/postgres` | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | [postgres/postgres](https://github.com/postgres/postgres) |
| `/home/et16/work/mariadb` | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | [MariaDB/server](https://github.com/MariaDB/server) |
| `/home/et16/work/cubrid` | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | [CUBRID/cubrid](https://github.com/CUBRID/cubrid) |
| `/home/et16/work/sqlite` | `43a48f77585a88940c727f33682151418f544059` | [sqlite/sqlite](https://github.com/sqlite/sqlite) |

행 범위는 위 실제 revision의 위치다. SHA-256은 파일 전체 bytes이며 열람은 명시한 범위에 한정한다. 주석에 기술된 설계와 구현된 분기에서 관측한 책임을 구분했고 함수 전체·동시성 전체를 검증한 것으로 확대하지 않는다.

| ID | 파일·실제 읽은 행 | 관측·분류 연결 | 전체 파일 SHA-256 |
| --- | --- | --- | --- |
| W01 | [postgres analyze.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/parser/analyze.c), 109–157 | raw tree→Query와 parameter 타입 입력. 4.03 parser·4.05 binder·4.11 parameter를 구분 | `93eaf63c0f60bc3ca2562f528d81bc6bef9177751273791e26c35f9b6f93e80c` |
| W02 | [postgres plancache.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/utils/cache/plancache.c), 679–750·860–905 | 재검증/lock 재획득의 경합 설명, fixed result descriptor 변경 거절. 4.06 dependency·4.11/12 metadata/수명 연결 | `51b5941f39cd91b7de11c9701cd4decfd173a3681f05b2be8c013ea21a84bbaf` |
| W03 | [postgres nodeSort.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/executor/nodeSort.c), 26–80·97–169 | 하위 node 입력→sort→후속 fetch 책임. 4.07 실행과 4.08 sort 공급 경계 | `6ba1b4ff7a3f2e208c8f3b784bff61e182e47a44bd22a88744998b9850c112db` |
| W04 | [postgres tuplesort.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/utils/sort/tuplesort.c), 1–125 | 머리말의 bounded memory·run 생성·k-way merge·최종 merge/임시 저장 설명. 4.08.1/2와 4.14 자원 인수 분해 | `dc140a5018871fe263b585f4e8a7c9ab364b7d9533a6188b4279ab9ce3135641` |
| W05 | [mariadb sql_prepare.cc](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql/sql_prepare.cc), 20–92·4790–4853·4890–4927 | prepare metadata와 execute 값 전달, metadata invalidation의 제한 reprepare. 4.11 별도 수명·4.06 실행 전 검사 | `1c83c960cac5f4e6be24132045e1d9afd9eed4fd374b464c9245d70a7c670fbd` |
| W06 | [mariadb transaction.cc](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql/transaction.cc), 463–586 | trans_commit_stmt/trans_rollback_stmt가 engine transaction 경계와 active multi-statement 상태를 다룸. 4.09 statement와 4.13 전체 거래 조립 구분 | `2f63b3189e257bce3cfe5303f64811ddc774604a9294c92d95b74833208f2dd8` |
| W07 | [cubrid execute_statement.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/query/execute_statement.c), 4074–4245 | prepare 가능한 질의와 direct 처리 문장 종류의 구분·dispatch. 4.10 DDL·4.11 prepared·4.13 거래 제어 분리 | `92ada15819057a15438a502a88578e354be1675de227a728aae33c55fe904d0d` |
| W08 | [cubrid log_recovery.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/transaction/log_recovery.c), 270–330·4925–4955 | 보상 기록의 생성·undo dispatch와 LOG_COMPENSATE의 redo-only/undo_nxlsa 이동. 4.17의 rollback/recrash 수신 경계 | `346275b089af2ff3231b3eb224fb6a7d8e9e5811e958762980dc11b978d28058` |
| W09 | [sqlite vdbeapi.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbeapi.c), 120–174·1730–1809 | reset/clear_bindings 분리, busy bind 거절·길이/destructor. 4.11 owned bind와 4.12 result/handle 수명 | `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5` |
| W10 | [sqlite vdbeaux.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbeaux.c), 3240–3305·3450–3505 | statement savepoint rollback/release, 정리 오류 승격·전체 rollback·change count 처리. 4.09·4.13·4.14의 별도 실패/소유 인수 | `b5861fefc82cb754987635deab85abc4062883200202e08ff609f185af91bae6` |
| W11 | [sqlite vdbesort.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbesort.c), 12–130 | 머리말의 Write/Rewind/Next/Close 순서와 메모리 초과 시 PMA·bounded merge 설명. 4.08 run/merge/cleanup 분리 | `f7d6cc2cd7ea02d4eb15e119d1b29d8aac063ec8a57fe6ea7168d0899c9fef17` |

### Oracle·ARIES 원문

Oracle은 기존 비교 기준인 **19c** 문서를 열람했으며 Oracle DB 실행은 하지 않았다. ARIES는 공식 서지와 CMU 공개 사본의 PDF 1–7쪽(인쇄94–100쪽) 텍스트를 읽었다. 전문·도표 전체를 검토한 기록은 아니다. 웹 도구의 Berkeley/CMU/UMich PDF 읽기가 timeout되어 CMU 사본을 별도 HTTP로 내려받아 `pdftotext`로 해당 쪽만 읽었으며 저장소에는 편입하지 않았다. 사본 SHA-256은 `6131bcee36c21e669d41204ef2b2320cb458b5d3bd9b7cf3e7758afe8ed3495f`다.

| ID·실제 자료/부분 | 확인한 의미 | XADB 작업 분류·한계 |
| --- | --- | --- |
| O01 [Oracle19c SQL](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/sql.html), Stages of SQL Processing·SQL Parsing | parse·optimization·row source·execution 책임, syntax/semantic 검사 구분 | 4.02–07을 분리하는 참고. 기존 recursive-descent·규칙 기반 plan·pull 실행 유지; Oracle optimizer·shared pool 도입 아님 |
| O02 [Oracle19c Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/transactions.html), Statement-Level Atomicity·End of a Transaction | 성공 statement와 commit을 구별하며 실패 문장은 앞선 성공 문장을 보존할 수 있음 | 4.09 runtime rollback·4.13 거래 종료·4.17 crash 인수를 구분. XADB의 전체 conflict abort·fencing은 XADB 정본으로 판단 |
| O03 [Oracle19c COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html), Purpose·WAIT | 유효 DDL 전 commit과 성공 DDL 후 commit, durable 기록과 응답 수신 사이 실패 가능 | 4.10 두 outcome과 4.17 R3/R4. XADB 지원 문법·진입 거절 경계 유지, NOWAIT·자동 replay 도입 없음 |
| O04 [Oracle19c Nulls](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Nulls.html), 길이0 character·비교/조건 | 빈 character/NULL·UNKNOWN과 IS NULL 구분 | 4.04 입력 의미·4.06/07 NULL index 결과·4.11 bind vector 연결. native 빈 Utf8까지 바꾸지 않음 |
| A01 [ARIES 공식 서지](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging), [1992 원문 CMU 사본](https://www.cs.cmu.edu/~15849g/readings/mohan92.pdf), PDF1–7쪽 텍스트 | WAL/page LSN·commit log 영속화, partial/total/restart undo, redo-only CLR와 반복 장애 시 rollback 기록 | 4.01 실제 DEV3 복구 공급과 4.17 R1–R4의 경계. SQL 내 저장 복구 재구현·ARIES와 XADB UndoRef 동일성·SQL SAVEPOINT 추가 근거가 아님 |

### 분류에 반영한 판단

W01/O01을 따라 구문·binding·계획·실행을 별도 산출물로, W02/W05/W09를 따라 metadata·owned binding·result generation을 별도 수명으로 나눴다. W03/W04/W11의 임시 정렬 처리에서는 run 표현·merge·cleanup을 구분해 4.08에 배정하고 기존 합산 quota/old+new peak는 4.14에서 소비자와 연결한다. 외부 시스템의 memory 정책·병렬 sorter·자동 reprepare를 XADB 정책으로 채택하지 않았다.

O02/W06/W10은 문장 실패와 거래 종료를 분리해 검토할 근거다. XADB의 문장 최종 UNIQUE, 실제 잔류 WAL/CLR·undo·잠금 회계, NotCommittedRecoveryRequired는 현행 owner 계약을 유지한다. W07/O03에 따라 DDL을 prepared DML과 분리하고 precommit 이후 새 catalog 검증·두 outcome을 4.10에 배정했다. A01/W08의 복구 경계는 4.17에 배정하되 page/CLR의 구현 owner는 DEV3로 유지한다.

이 판단은 기존 계약을 빠짐없이 작업에 배정하기 위한 것이다. 새 SQL 의미·외부 dialect 동일성·자원 한도 적정성·실제 처리 성능을 증명하지 않는다. 이번 문서의 검토 상태와 commit 범위는 [작업 기록](../plans/dev4-work-breakdown.md#decision)을 따른다.

<a id="work-review-20261022"></a>
## 2026-10-22 — 분배·분량·우선순위 후속 검토

**선택 원문·소스 열람 / 계획 보완의 근거 / 제품·외부 DBMS 시험 NOT_RUN.** `/root`가 이번 대화의 분배 검토에서 읽은 범위를 기록한다. 후속 수정 요청으로 [D4-WORK-BREAKDOWN-002](../plans/dev4-work-breakdown.md#work-review-002)에 연결한다. 기존 001 조사·독립 검토·source identity는 보존한다. 아래 자료가 XADB 작업 수·공수·개발 순서의 최적성을 실측으로 증명하는 것은 아니다.

### 실제 source identity와 열람 범위

검토 때 다섯 checkout의 origin·HEAD·tracked 상태를 확인했으며 수정 반영 때 HEAD·tracked 상태와 아래 전체 파일 SHA-256을 확인했다. 모두 tracked 변경이 없었다. 원본 checkout 수정·빌드·실행·코드 도입은 없으며 untracked 파일·DBMS 전체 구현의 감사는 아니다. `/home/et16/work/duckdb`는 경로가 없어 제외했다.

| checkout | 실제 HEAD | 확인한 origin |
| --- | --- | --- |
| `/home/et16/work/postgres` | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | [postgres/postgres](https://github.com/postgres/postgres) |
| `/home/et16/work/mariadb` | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | [MariaDB/server](https://github.com/MariaDB/server) |
| `/home/et16/work/cubrid` | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | [CUBRID/cubrid](https://github.com/CUBRID/cubrid) |
| `/home/et16/work/sqlite` | `43a48f77585a88940c727f33682151418f544059` | [sqlite/sqlite](https://github.com/sqlite/sqlite) |
| `/home/et16/work/mysql-server` | `3b99be40f8a31d9396cad10accfd3ba51647d613` | [mysql/mysql-server](https://github.com/mysql/mysql-server) |

파일별 행은 위 revision 기준이며 읽은 범위만 기록한다. sorter 머리말의 설명을 구현 전체·최대 자원 보장의 검증으로 확대하지 않는다.

| ID·파일·읽은 행 | 관측·계획 연결 | 전체 파일 SHA-256 |
| --- | --- | --- |
| B01 [postgres execMain.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/executor/execMain.c), 1–52·230–266·486–535 | Start/Run/Finish/End와 snapshot 등록/해제. query 수명과 여러 실행 호출을 구분하는 참고; 4.12/13/14 상태 통합 | `f097f7067f6ceffc00eb601174ea5614cd14c0ab9b1574337614c5cedf75ca49` |
| B02 [postgres nodeSort.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/executor/nodeSort.c), 26–175 | 하위 입력 수집·sort·후속 fetch와 sorter 조립. 4.08 기반과 SQL 소비 분리 | `6ba1b4ff7a3f2e208c8f3b784bff61e182e47a44bd22a88744998b9850c112db` |
| B03 [postgres plancache.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/utils/cache/plancache.c), 679–755·870–902 | 재검증·lock 획득 경합, fixed result type 변경 거절. 초기 bound/dependency·불변 metadata 준비 유지 | `51b5941f39cd91b7de11c9701cd4decfd173a3681f05b2be8c013ea21a84bbaf` |
| B04 [postgres tuplesort.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/utils/sort/tuplesort.c), 1–120 | 공통 정렬과 대상별 variant, 메모리 run·외부 run·merge·최종 소비 설명. 4.08 세분화·F03 재사용 비교 | `dc140a5018871fe263b585f4e8a7c9ab364b7d9533a6188b4279ab9ce3135641` |
| B05 [mariadb transaction.cc](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql/transaction.cc), 463–586 | statement commit/rollback과 multi-statement 거래 상태. 4.09와 4.13의 별도 실패·종료 인수 | `2f63b3189e257bce3cfe5303f64811ddc774604a9294c92d95b74833208f2dd8` |
| B06 [cubrid execute_statement.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/query/execute_statement.c), 4074–4148 | prepare 가능한 statement와 direct dispatch 경계. DDL과 prepared 통합 책임 유지 | `92ada15819057a15438a502a88578e354be1675de227a728aae33c55fe904d0d` |
| B07 [cubrid log_recovery.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/transaction/log_recovery.c), 4920–4965 | LOG_COMPENSATE는 redo-only, undo_nxlsa로 다음 undo 이동. R1/R2 실제 하위 복구 공급 유지 | `346275b089af2ff3231b3eb224fb6a7d8e9e5811e958762980dc11b978d28058` |
| B08 [sqlite vdbeaux.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbeaux.c), 3220–3288·3450–3508 | statement rollback/release와 I/O 실패 시 오류 승격·전체 rollback. 수명 통합 상태표와 독립 실패 판정 | `b5861fefc82cb754987635deab85abc4062883200202e08ff609f185af91bae6` |
| B09 [sqlite vdbesort.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbesort.c), 12–135 | CREATE INDEX/ORDER BY 사용, Init/Write/Rewind/Next/Close와 PMA/merge 설명. comparator·수명 차이를 보존한 재사용 검토 | `f7d6cc2cd7ea02d4eb15e119d1b29d8aac063ec8a57fe6ea7168d0899c9fef17` |
| B10 [mysql sql_parse.cc](https://github.com/mysql/mysql-server/blob/3b99be40f8a31d9396cad10accfd3ba51647d613/sql/sql_parse.cc), 3340–3390 | implicit commit 시작 경계와 metadata lock 해제·후속 실행. DDL precommit의 독립 조립·fault cut 유지 | `87343f539682ebca3620b4ca6966680a6ffcb5ab937768cad3fc28d2ec11fc8d` |

### Oracle·ARIES 원문과 적용 한계

Oracle은 기존 기준인 19c 공식 문서를 웹 도구로 읽었다. [SQL Processing](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/sql.html)의 처리 단계·DDL 차이, [Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/transactions.html)의 Statement-Level Atomicity, [COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html)의 implicit commit·WAIT와 응답 유실을 확인했다. [Nulls](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Nulls.html)의 빈 character/UNKNOWN과 [Data Concurrency](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/data-concurrency-and-consistency.html)의 statement/transaction 읽기 일관성도 대조했다. statement 성공과 거래 commit, DDL 두 결과를 구분할 근거이며 Oracle 실행·전체 호환 검증은 아니다. SQL READ COMMITTED 노출·자동 재시도는 추가하지 않는다.

ARIES는 [IBM 공식 서지](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging)와 [CMU 원문 사본](https://www.cs.cmu.edu/~15849g/readings/mohan92.pdf)을 사용했다. 웹 PDF fetch timeout 뒤 HTTP로 받은 PDF의 SHA-256은 `6131bcee36c21e669d41204ef2b2320cb458b5d3bd9b7cf3e7758afe8ed3495f`다. `pdftotext`로 PDF1–10쪽을 추출하고 실제 검토에는 1쪽 초록·4–7쪽(인쇄97–100쪽)의 WAL, commit log 영속화, partial/total/restart undo·redo-only CLR·latch/lock 설명을 읽었다. 그림의 시각 검토·전문 검증은 하지 않았다. 사본은 저장소에 편입하지 않았다. R1과 R2의 서로 다른 기대값·R3 응답 유실을 일찍 연결할 판단에 참고했으며 XADB UndoRef를 ARIES WAL chain과 동일시하지 않는다.

### 분배 검토에서 채택한 판단

| 지적 | 적용 | 범위·미완료 |
| --- | --- | --- |
| INSERT 값 준비가 SELECT 공급과 묶임 | 4.09.1.1/2·4.09.2.1/2 분리, 작은 DML commit/abort를 4.13에서 별도 공급 | 초기 fixture는 native 준비 가능. SQL DDL 미구현을 완료로 표시하지 않음 |
| sorter·복구·성능 한 작업의 분량 과다 | run/spill/merge/cleanup, R1/R2/R3/R4/restore, PF10-A–E를 별도 배정 | 모든 기존 인수 유지, 공수/기간 최적성 실측 없음 |
| 여러 수명 상태를 각 담당이 따로 종료할 위험 | 4.13 수명 통합 담당·공통 상태/소유 표, 4.16 독립 모델/known-bad | 기존 유한 모델 의무를 실제 산출물에 연결. 모델·제품 실행 NOT_RUN |
| native build·SQL sort의 중복 구현 가능성 | F03 전에 실제 공급·공통 mechanics·comparator/소유 차이를 비교·독립 D2 판단 | 공통 코드/새 crate 채택은 아직 없음. 무조건 공통화하지 않음 |
| 책임 개수를 공수·진행률로 오인 | 기존 ID 유지·자식 단위 배정·부모 중복 집계 금지, 실제 범위/불확실성/검토·시험 비용 기록 | 사람/agent별 실제 구현 배정·일정 추정은 착수 시 수행 |

외부 DBMS의 파일 구성을 XADB의 작업 개수로 복사하지 않았다. 위 소스와 정본을 이용한 작업 조직 판단이며 제품 의미 변경·소스 도입·시험 실행 증거가 아니다. 수정·독립 검토·commit 범위는 [002 작업 기록](../plans/dev4-work-breakdown.md#work-review-002)을 따른다.

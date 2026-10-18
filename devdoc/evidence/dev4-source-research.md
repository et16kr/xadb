# DEV4 소스 조사 기록

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
| statement 실패를 거래 전체 실패와 구별하는가 | R7에서 statement rollback과 전체 transaction rollback은 별도 경로이며 오류 승격이 존재 | XADB DEV2/DEV3 statement mark·immediate unique·UNKNOWN 효과를 그대로 사용. SQLite conflict mode나 저장 구조는 채택하지 않음; D4-T04/09/10 |

외부 소스에서 XADB의 문법/수치·자원 상한·오류 코드·내구성 증명을 도출한 것은 아니다. 이들은 XADB 소유 계약과 D2 판단으로 정하고 실제 구현 시험을 별도로 요구한다. parser 알고리즘·외부 sorter·전체 optimizer에 대한 외부 소스 전수 조사는 수행하지 않았으며 그 설계 선택의 근거는 제한된 기능과 bounded 자원 요구다. 라이선스가 다른 소스를 XADB에 편입하지 않았고 코드 도입이 필요해지면 DP04의 출처·license·notice 검토를 적용한다.

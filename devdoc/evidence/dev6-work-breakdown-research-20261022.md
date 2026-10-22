# DEV6 세부 분류 — ARIES·Oracle·로컬 DBMS 조사

2026-10-22 · **실제 원문·소스 열람 / 작업 분류 근거 / 제품 실행 NOT_RUN**

[18개 세부 분류](../plans/dev6-work-breakdown.md) · [기존 설계](../contracts/dev6-client-design.md) · [기존 조사·검토](dev6-source-research.md) · [로컬 소스 카탈로그](../reference/opensource-references.md)

사용자의 DEV3 같은 DEV6 세부 분류와 ARIES·Oracle·`~/work` DBMS 참고 요청에 따라 `/root`가 아래 자료를 직접 읽었다. 기준 XADB HEAD는 `def35fb1e274040eb55d78fa68948ba1610166c1`이며 착수 작업 트리는 clean이었다. 이번 조사와 과거 DEV6-DESIGN-001/REVIEW-002의 조사·검토는 별개다. 기존 의미를 재배정하는 근거이며 새로운 호환성·API·제품 구현을 채택한 기록이 아니다.

<a id="primary"></a>
## 1. 공식 원문과 분류 적용

| 실제 읽은 자료·범위 | 관찰 | XADB 분류에 적용한 판단 |
| --- | --- | --- |
| [ARIES IBM 공식 서지](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging), [1992 논문 공개 사본](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf), 인쇄97–99쪽의 WAL/commit·partial rollback, 109쪽의 CLR/UndoNxtLSN 설명 | commit 로그의 안정 저장과 page 반영을 구별하고, partial/total rollback과 재장애 때 undo 진행을 기록한다 | 6.05 outcome·6.09 거래와 6.16 외부 ACK/reopen을 별도 분류. ARIES는 wire ACK 전달·client 자동 재시도의 안전성을 보증하지 않으므로 기존 no-replay와 C03/04/08을 유지. engine WAL/undo 구현은 DEV3 소유 |
| [Oracle Database 19c JDBC Coding Tips](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-coding-tips.html), C.1·C.2.1–2 | connection 접근의 직렬화, DB cancel과 thread interrupt, SELECT 결과 소비/close와 완료, fetch 묶음의 책임을 구별 | 6.04 operation/cancel·6.08 result·6.09 mode 조립·6.12 JDBC를 분리. Oracle의 fetch 기본값·batch/cache 기능을 초기 XADB 지원으로 옮기지 않음 |
| [Java SE21 Connection](https://docs.oracle.com/en/java/javase/21/docs/api/java.sql/java/sql/Connection.html), setAutoCommit/commit/rollback/close | 실제 mode 변경과 같은 값 no-op, ON에서 commit/rollback 오류, active close의 구현별 의미 | 6.01.2의 signature별 목록, 6.09.2/6.12.3의 명시 조립과 no-op/거절을 배정. XADB close 규칙은 채택 계약을 유지하고 일반 JDBC 규격만으로 rollback 완료를 추정하지 않음 |
| [ODBC SQLGetData 공식 문서](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetdata-function?view=sql-server-ver17), Arguments·Diagnostics | 고정 C type 크기와 가변 buffer 길이, NULL pointer·indicator·truncation·column 순서가 다른 책임 | 6.13 descriptor/탐지와 6.14 변환/partial copy를 분리. R2-V06·F04의 함수별 A/W 단위·guard bytes를 인수에 연결 |

위 오른쪽 열은 XADB 작업 분류에 대한 판단이다. ARIES를 client 프로토콜 설계서로 취급하거나 Oracle·JDBC·ODBC 전체 호환을 주장하지 않는다. 원문의 서술을 재구현한 코드·벤치마크·실행 결과는 없다.

<a id="local"></a>
## 2. 로컬 소스 실제 열람

2026-10-22에 아래 여섯 checkout의 `origin`·HEAD·`git status --short`를 확인했으며 모두 clean이었다. 경로는 카탈로그의 `/home/et16/work/<checkout>`이다. 원격 갱신·checkout 변경·build·시험을 실행하지 않았으며 파일은 읽기만 했다. 나머지 네 checkout을 이번에 분석했다고 주장하지 않는다.

| checkout·실제 revision | 읽은 파일·함수·줄 | 관찰·분류 연결 |
| --- | --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | [src/interfaces/libpq/fe-exec.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/interfaces/libpq/fe-exec.c), PQsendQueryStart 1690–1752, PQgetResult 2079–2147 | 접속/진행 상태의 admission과 남은 송신·대기·결과 완료 처리를 구별. 6.02 codec과 6.04 진행 owner를 나누고 6.08에 결과 완료를 배정. pipeline 기능은 채택하지 않음 |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | [sql-common/client.c](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql-common/client.c), mysql_free_result 749–775, mysql_close_slow_part/mysql_close와 주석 3468–3507 | unbuffered 결과 정리와 메모리 해제, close의 송신/block 가능 부분을 구별. 6.03 명시 close/local 해제와 6.08 결과 수명 분리. XADB Drop에 blocking을 도입하지 않음 |
| MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613` | [libmysql/libmysql.cc](https://github.com/mysql/mysql-server/blob/3b99be40f8a31d9396cad10accfd3ba51647d613/libmysql/libmysql.cc), stmt_fetch_row/cli_unbuffered_fetch/mysql_stmt_fetch 3908–4005 | row의 NULL 상태·application buffer 복사·truncation·fetch 상태를 구별. 6.08 내부 row 수명과 6.14 caller copy를 분리; MySQL API를 ODBC 표준 oracle로 사용하지 않음 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | [src/broker/cas_handle.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/broker/cas_handle.c), hm_new_srv_handle 진입 58–91, hm_srv_handle_free 149–178 | handle 상한·빈 slot 찾기와 내용/임시 자원/slot 해제를 구별. 6.10.2 registry·6.13 parent/child·6.15 회계에 배정. 이 소스가 XADB generation 안전성을 증명하지 않음 |
| SQLite `43a48f77585a88940c727f33682151418f544059` | [src/vdbeapi.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbeapi.c), sqlite3_finalize/reset/clear_bindings 105–187 | 실행 reset·statement 파괴·binding 정리는 별도 연산. 6.06 binding·6.07 statement·6.08 result 수명을 분리. SQLite clear_bindings의 NULL을 XADB Unbound와 동일시하지 않음 |
| ClickHouse `1d76716b7580f78d0375c626736e49a3e13f099c` | [src/Client/Connection.cpp](https://github.com/ClickHouse/ClickHouse/blob/1d76716b7580f78d0375c626736e49a3e13f099c/src/Client/Connection.cpp), Connection::cancel/reset/disconnect 479–519 | 송신 buffer/socket 종료와 로컬 상태 정리 경계를 관찰. 6.03 연결 폐기·6.04 cancel·6.16 서버 cleanup 증거 분리. 이 경로를 XADB rollback 완료로 해석하지 않음 |

아래는 위에서 읽은 **파일 전체 bytes**의 SHA-256이다. 읽은 부분은 위 표의 함수·줄 범위이며 전체 파일·DBMS 분석을 뜻하지 않는다. XADB build fingerprint나 제품 시험 source identity도 아니다.

| checkout·파일 | SHA-256 |
| --- | --- |
| PostgreSQL · fe-exec.c | `e77c1df9bfd0d350c018b9be51fcb9cf3419ab5d84540bafb23dcc375222bf99` |
| MariaDB · client.c | `40d26fe8713eaa276c8b095ec055034eea4f31162c49e2feb9b7254ec0bdf3a6` |
| MySQL · libmysql.cc | `71ad5800bbf7e4e83c39e8c03b2aa7694d51387d8d4a590166e5928a0bb50531` |
| CUBRID · cas_handle.c | `281bf4ee5cb7bec161cf05ee17d0e1c929a18f493c1af0e57b585152022bc53d` |
| SQLite · vdbeapi.c | `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5` |
| ClickHouse · Connection.cpp | `3ffbcae7e4770e320206b63f92230003b7452f09c1dd9da870c3b95d31b43fa2` |

<a id="coverage"></a>
## 3. 분류 누락·공급 순환을 막는 연결

| 대조 대상 | 배정 |
| --- | --- |
| F01/F02 | 6.01 동결 목록, 6.02/11 codec, 6.04–09 의미, 6.12/13/14 실제 API, 6.16 독립 입력 |
| F03/F04 | 6.01.3 동결, 6.10 Native/C, 6.13/14 ODBC, 6.15 자원, 6.17 ABI/빌드 |
| F05/F06/F07 | 각각 6.17, 6.16, 6.18; 준비를 해당 구현/측정 전에 공급 |
| T01–T05 | 6.02/03/04/07 및 Java·ODBC의 해당 실제 경로 |
| T06–T12 | 6.05–10·6.12–14의 값/bind/result/거래/진단과 6.15 자원 |
| T13/T14/T15 | 각각 6.12 method inventory, 6.14 변환/부분 buffer, 6.10/13/14/17 실제 ABI |
| T16/T17/T18 | 각각 6.16 실제 fault/reopen, 6.17 export/설치, 6.15/18 자원·성능 |
| C01–C08·M01–03 | 6.16이 독립 도구/입력 소유, 6.04/05/08/09 및 각 driver가 생산 경로 소유 |
| R2-V01–V06 | ID/복합취소 6.02/04/11, mode/EOF 6.08/09/12/13, JDBC 결과 소진 6.12.2, GetData 6.14.3 |
| SC04→SC05→SC06 | Native/C·Java·DM의 각각 실제 검증과 순차 인수. Java 개발은 Rust 완료와 독립, ODBC는 native core 공급을 받고 C facade 완료는 구현 선행 아님 |

기존 I01–I05·task DAG·CLIENT1의 필수 품질 기준과 미실행 상태를 유지한다. DEV3 S5 전수 예외는 DEV6에 적용하지 않으며 이번 문서 작업에서 제품 시험을 실행하지 않는다. 원본 PASS/FAIL/중단 증거를 이번 분류의 새 실행 결과로 바꾸지 않는다.

<a id="review"></a>
## 4. 검토와 작업 상태

작업·client 계약·통합 담당은 `/root`다. 변경 범위는 새 분류/조사 문서, 기존 DEV6 계획·설계의 연결, 단계/완료 계약의 문서 참조, README·문서 ID 색인이다. 제품 코드·빌드 등록·실행 상태는 변경하지 않는다.

별도 실행 `/root/dev6_breakdown_review`가 새 분류·조사 문서 전체, 기존 DEV6 설계·구현 계획, README·문서 catalog·단계/완료 JSON의 DEV6 변경분을 읽기 전용 검토했다. 동시 DEV3·DEV7 변경은 검토 대상에서 제외했다.

검토자는 18개 분류·55개 하위 작업, F01–07/T01–18/C01–08/M01–03/P01–04/R2-V01–06의 배정, primitive 공급과 후속 통합의 순환 방지, Java/Native/ODBC의 불필요한 완료 대기 제거, SC 순서와 조기 quota/oracle/environment 공급을 확인했다. 작성자의 자기 검토에서 명확히 한 6.06.3/6.07.1의 초기 primitive 수신, 6.09.3의 driver별 선행, 6.13.1의 local/실제 접속 구분, 6.15.1/6.17.1의 driver별 입력 및 6.12.2 ResultSet 책임도 최신 문구로 확인했다.

또한 실제 여섯 checkout의 origin·HEAD·clean 상태와 위 여섯 파일 SHA-256을 재확인하여 모두 일치함을 확인했다. 인용 함수 범위를 직접 읽고 ARIES·Oracle JDBC·Java Connection·ODBC SQLGetData 원문의 관찰과 XADB 적용 제한을 대조했다. 정적 검토 결론은 **필수 수정 지적 P1/P2 없음·D2 계획 채택 가능**이며 `/root`가 계약·통합 담당으로 반영을 확인했다. 이 결론은 구현 안전성이나 실제 제품 인수 PASS가 아니다.

구현·모델·FFI·driver·DBMS build·성능·복구 시험과 문서 자동 검사는 NOT_RUN이다. 문서 자동 검사는 push 직전 정책을 따르며 이번 요청은 local commit까지다. 기존 조사·검토·실행 원본과 미완료 상태를 보존한다.

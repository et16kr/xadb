# DEV6 — Oracle·ARIES·로컬 DBMS 클라이언트 조사와 검토

2026-10-19 · **실제 문서·선택 소스 열람 / 설계 근거 / 제품 실행·호환·성능 검증 아님**

[DEV6 설계](../contracts/dev6-client-design.md) · [구현·인수 계획](../plans/dev6-implementation-plan.md) · [로컬 소스 catalog](../reference/opensource-references.md)

사용자가 Oracle·REF-C06-02 및 `~/work` DBMS를 참고한 DEV6 설계를 요청했다. `/root`가 아래 공식 원문과 로컬 checkout의 선택 함수를 읽었다. 공개 DBMS 전체를 분석했다거나 이들 제품을 실행하여 결과를 비교했다는 뜻이 아니다. XADB의 engine·SQL·서버 설계와 현재 구현/실행 상태를 분리하고 client API 책임에 필요한 부분만 조사했다.

<a id="official"></a>
## 1. 공식 문서에서 확인한 내용

아래 URL은 2026-10-19 열람했다. 버전은 URL의 Oracle19c·Java21·REF-C06-02 7.3 및 ODBC API 문서 기준이다. 웹 원문은 이후 수정될 수 있으므로 고정 source commit으로 표현하지 않는다. 검색 결과만을 근거로 새로운 DBMS 내부 구현을 추정하지 않았다.

| 원문 | 실제 확인·XADB 선택 |
| --- | --- |
| [Oracle19c JDBC coding tips](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-coding-tips.html) | 연결의 통제된 직렬 접근, Statement cancel과 fetch/batching/caching 구분을 확인. XADB는 단일 ordinary·별도 cancel slot·bounded fetch를 선택하고 pooling/cache/batch는 후속으로 남김 |
| [Oracle19c JDBC getting started](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-getting-started.html) | autocommit 기본·commit/rollback API 제한·DDL 경계 및 close 시 주의사항을 확인. XADB close는 기존 계약의 rollback이며 Oracle의 해당 close 설명을 복제하지 않음 |
| [Oracle19c OCI binding/defining](https://docs.oracle.com/en/database/oracle/oracle-database/19/lnoci/binding-and-defining-in-oci.html) | bind/define·buffer·indicator·길이와 array 구분을 확인. C/ODBC에 NULL/length/ownership을 분리하지만 OCI API/array/LOB까지 초기 지원한다고 주장하지 않음 |
| [Java21 Connection](https://docs.oracle.com/en/java/javase/21/docs/api/java.sql/java/sql/Connection.html) | mode 변경시 transaction commit, 같은 mode no-op·auto ON의 commit/rollback 오류를 확인. DEV6은 명시적 close→COMMIT→SET 조립과 단계별 실패 보존을 설계. SQL/native Active SET 거절 자체는 유지 |
| [Java21 ResultSet](https://docs.oracle.com/en/java/javase/21/docs/api/java.sql/java/sql/ResultSet.html) | 대소문자 무시·첫 matching column, Statement close/reexecute의 ResultSet 종료를 확인. JDBC label lookup을 native exact lookup과 분리하고 대소문자만 다른 두 alias 반례 추가 |
| [ODBC SQLBindCol](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlbindcol-function?view=sql-server-ver17) | 독립 검토에서 column binding 수명을 rebind/unbind/statement free까지로 확인. cursor close와 binding 해제를 분리하고 재실행 vector 추가 |
| [Java21 UnixDomainSocketAddress](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/net/UnixDomainSocketAddress.html) | 표준 Unix domain address와 SocketChannel/ServerSocketChannel 연결을 확인. Java21 순수 Java client의 local transport 근거이며 모든 OS 지원을 주장하지 않음 |
| [REF-C06-027.3 Starting JDBC](#source-ref-c06-02) | Type4·JDBC4.2 부분 지원·driver loading/URL/properties를 확인. XADB는 Java21/JDBC4.3 부분 지원표를 독자 동결하며 REF-C06-02 TCP/password/다중 주소를 가져오지 않음 |
| [REF-C06-027.3 Basic Functions](#source-ref-c06-02) | Statement/PreparedStatement/CallableStatement의 prepare·IN/OUT 차이, 생성시 server prepare·실패 exception을 확인. XADB도 조기 typed prepare를 선택하되 Callable/OUT은 후속 |
| [REF-C06-027.3 ODBC programming](#source-ref-c06-02) | DSN/connection string과 prepare/bind→변수 값 변경→반복 execute·column bind 예제를 읽음. ODBC descriptor는 등록시 값 복사와 구분하고 Execute마다 caller 값을 읽는 계약으로 정함 |
| [REF-C06-027.3 SQLBindParameter](#source-ref-c06-02) | buffer pointer·C/SQL type·length·indicator와 SQL_NTS/명시 길이를 확인. XADB native/JDBC owned setter와 ODBC borrowed descriptor의 차이를 인수 vector에 반영 |
| [REF-C06-027.3 SQLCancel](#source-ref-c06-02) | 다른 thread 취소, 이미 완료한 문장의 효과/결과 유지, 진단 구분을 확인. XADB Requested≠완료·late cancel의 COMMITTED 보존·세대 분리로 구체화 |
| [ODBC SQLSetConnectAttr](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlsetconnectattr-function?view=sql-server-ver17) | manual→auto 전환의 commit과 cursor/plan 영향 탐지 항목을 확인. XADB는 mode 조립·SQL_CB_CLOSE·상태 갱신을 명시 |
| [ODBC SQLDisconnect](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqldisconnect-function?view=sql-server-ver17) | 진행 중 거래의25000과 연결 유지를 확인. auto ON+raw BEGIN도 예외 없이 거절하고 raw control로 거래를 끝낸 뒤 disconnect하는 반례 추가 |
| [ODBC SQLGetData](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetdata-function?view=sql-server-ver17) | 반복 partial get·NULL indicator·terminator·remaining length·01004·ordinal/기능 flag를 확인. XADB는 unbound/increasing ordinal·한 row의 offset·UTF 경계를 설계하며 native row-budget 오류와 구별 |

Oracle/REF-C06-02의 더 넓은 SQL·타입·driver 기능, resident/mixed storage, 내부 알고리즘은 채택 범위가 아니다. 공개 문서의 코드 예제를 복사하거나 proprietary binary의 내부를 추정하지 않았다. 이번 상용 참조의 판단은 공식7.3 문서의 버전과 절로 한정한다.

<a id="source-ref-c06-02"></a>
### 중립 출처 REF-C06-02

사용자 요청에 따라 이 출처의 제품명·원문 URL은 저장소 기록에 남기지 않는다. 원문 대응표와 열람 HTML은 외부 로컬 archive `/home/et16/work/xadb-source-archive/dev6-client-research-20261019`의 `source-manifest.json` 및 아래 source ID 파일로 보존한다. source-manifest SHA-256은 `886976c2945991c80c383e5876862813a3e743ab1d7326cba56f96251509c6b0`이다. 이 지문은 조사 입력 추적용이며 XADB 실행/성능 증거가 아니다. URL 원문을 알아야 하는 재조사자는 외부 archive의 대응표를 사용한다.

| 중립 source ID | 읽은 문서 | 원문 HTML SHA-256 |
| --- | --- | --- |
| REF-C06-02-01 | 7.3 Starting JDBC | `6698ea8a308ccf3a9983a669ef04d63c810fef79df9421e935401fa1c2b6f3b6` |
| REF-C06-02-02 | 7.3 Basic Functions | `de3e32d743388e6598415878f80fc29b36fcbe9bde7aafac2f96ddd08d940ecb` |
| REF-C06-02-03 | 7.3 ODBC Programming | `5e5f312aaceb61425dc3345b83b36538e30ca3a90e5343bebf3c4262c4323a40` |
| REF-C06-02-04 | 7.3 SQLBindParameter | `9e6e100928565e9f7e1d9c522f2a6c7d7b029f21b7770533654ef66ba50b4f0e` |
| REF-C06-02-05 | 7.3 SQLCancel | `d94dce255802efe7d3b39ad2d31c5707c6f2111ec22b872302e0bcc356bece59` |

<a id="local"></a>
## 2. 로컬 소스의 실제 열람 범위

`/home/et16/work`의 catalog 경로를 확인했다. PostgreSQL·MariaDB·CUBRID·MySQL·SQLite·ClickHouse·TiDB·FoundationDB의8개 checkout은 아래 revision에서 `git status --short`가 빈 값이었다. DuckDB와 RocksDB의 catalog 경로는 이번 시점에 없으므로 실제 조사 대상이라고 기재하지 않는다. CUBRID 별도 CCI/client JDBC submodule이 제공된 것으로 가정하지 않고 server broker의 경계만 읽었다. 다른 개발 checkout·외부 archive는 수정하거나 Git 이력을 도입하지 않았다.

| 대상·선택 소스 | 읽은 symbol/부분 | 관찰·설계 반영 |
| --- | --- | --- |
| PostgreSQL `src/interfaces/libpq/fe-exec.c` | PQsendQueryParams, PQgetResult의 flush/read/async-state 경로 | parameter 전송과 결과 완료를 분리. XADB는 complete Reply·CancelAck 장벽으로 구체화하며 PostgreSQL pipeline을 채택하지 않음 |
| MariaDB `sql-common/client.c` | mysql_real_query, mysql_store_result 진입 | send/read result와 buffered materialization·command-state 검사 구분. XADB는 bounded batch/한 active result를 선택 |
| MySQL `libmysql/libmysql.cc` | mysql_stmt_bind_param, stmt_fetch_row, mysql_stmt_fetch | bind descriptor·NULL/length·truncation 상태 구분. XADB는 type/ownership·ODBC truncation을 명시하고 해당 C 코드나 자동 state 전이를 복제하지 않음 |
| CUBRID `src/broker/cas_execute.c`, `cas_handle.c` | ux_prepare, ux_fetch, hm_find_srv_handle, hm_srv_handle_free | server handle·prepare/fetch·cleanup 경계를 확인. XADB는 slot+generation+session incarnation·bounded terminal descriptor를 사용 |
| SQLite `src/vdbeapi.c` | sqlite3_finalize, sqlite3_reset, bindText의 destructor/값 설정 경로 | 실행 reset·해제·binding 수명을 분리. XADB는 원자 owned BindAll과 explicit close를 선택; SQLite statement API와 동일하다고 하지 않음 |
| ClickHouse `src/Client/Connection.cpp` | Connection::cancel/reset/disconnect | socket·송신 buffer 종료가 semantic rollback 증거와 별개임을 검토. XADB local abort와 서버 cleanup outcome을 구분 |
| TiDB `pkg/server/conn_stmt.go`, `conn.go` | handleStmtFetch, writeResultSet의 cursor side-effect/retry 주석 및 초기 분기 | fetch 위치/이미 송신한 결과는 재시도 경계. XADB fetch prefix·EOF와 자동 재실행 금지를 인수 |
| FoundationDB `bindings/c/foundationdb/fdb_c.h` | fdb_future_cancel/release_memory/destroy/block_until_ready 선언 | 취소·메모리 해제·객체 파괴가 서로 다른 API 책임임을 비교. 헤더 선언만 읽었고 FDB future 실행 알고리즘은 조사/채택하지 않음 |

### 실제 revision·파일 SHA-256

아래 hash는 이번에 읽은 파일 bytes의 SHA-256이며 제품 실행 source fingerprint가 아니다. 로컬 경로는 source catalog로, 원격은 각 commit permalink로 연결한다.

- **postgres** · [`45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`](https://github.com/postgres/postgres/tree/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851) · working tree clean
  - [src/interfaces/libpq/fe-exec.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/interfaces/libpq/fe-exec.c) — `e77c1df9bfd0d350c018b9be51fcb9cf3419ab5d84540bafb23dcc375222bf99`
- **mariadb** · [`327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4`](https://github.com/MariaDB/server/tree/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4) · working tree clean
  - [sql-common/client.c](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql-common/client.c) — `40d26fe8713eaa276c8b095ec055034eea4f31162c49e2feb9b7254ec0bdf3a6`
- **mysql-server** · [`3b99be40f8a31d9396cad10accfd3ba51647d613`](https://github.com/mysql/mysql-server/tree/3b99be40f8a31d9396cad10accfd3ba51647d613) · working tree clean
  - [libmysql/libmysql.cc](https://github.com/mysql/mysql-server/blob/3b99be40f8a31d9396cad10accfd3ba51647d613/libmysql/libmysql.cc) — `71ad5800bbf7e4e83c39e8c03b2aa7694d51387d8d4a590166e5928a0bb50531`
- **cubrid** · [`a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5`](https://github.com/CUBRID/cubrid/tree/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5) · working tree clean
  - [src/broker/cas_execute.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/broker/cas_execute.c) — `896a64b253764479e2f9e45c26ceaace5333d50aba10686ba6296e0cdbb31273`
  - [src/broker/cas_handle.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/broker/cas_handle.c) — `281bf4ee5cb7bec161cf05ee17d0e1c929a18f493c1af0e57b585152022bc53d`
- **sqlite** · [`43a48f77585a88940c727f33682151418f544059`](https://github.com/sqlite/sqlite/tree/43a48f77585a88940c727f33682151418f544059) · working tree clean
  - [src/vdbeapi.c](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/src/vdbeapi.c) — `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5`
- **clickhouse** · [`1d76716b7580f78d0375c626736e49a3e13f099c`](https://github.com/ClickHouse/ClickHouse/tree/1d76716b7580f78d0375c626736e49a3e13f099c) · working tree clean
  - [src/Client/Connection.cpp](https://github.com/ClickHouse/ClickHouse/blob/1d76716b7580f78d0375c626736e49a3e13f099c/src/Client/Connection.cpp) — `3ffbcae7e4770e320206b63f92230003b7452f09c1dd9da870c3b95d31b43fa2`
- **tidb** · [`328460a9af032ec6b53823724f87837a6d2d5857`](https://github.com/pingcap/tidb/tree/328460a9af032ec6b53823724f87837a6d2d5857) · working tree clean
  - [pkg/server/conn_stmt.go](https://github.com/pingcap/tidb/blob/328460a9af032ec6b53823724f87837a6d2d5857/pkg/server/conn_stmt.go) — `aa001cd757af7d3b5b484779e91aee4ace071d0ab0ce4b10e3c81bc246033215`
  - [pkg/server/conn.go](https://github.com/pingcap/tidb/blob/328460a9af032ec6b53823724f87837a6d2d5857/pkg/server/conn.go) — `068a60a454ecaae946be6e1990ec28452442f8c1cb15710ae43b53bc29f9c10c`
- **foundationdb** · [`487620d5aa41152b1f034f4e932005a87eb2947f`](https://github.com/apple/foundationdb/tree/487620d5aa41152b1f034f4e932005a87eb2947f) · working tree clean
  - [bindings/c/foundationdb/fdb_c.h](https://github.com/apple/foundationdb/blob/487620d5aa41152b1f034f4e932005a87eb2947f/bindings/c/foundationdb/fdb_c.h) — `d6bddf2e5f38074dab00ab3c7e31e19b4faff4257825d3621e0bbb9da714cd59`

<a id="choices"></a>
## 3. 교차 검토와 XADB 결정

| 질문·반례 | 선택·대안·필수 증거 |
| --- | --- |
| client가 서버 metadata를 추정하거나 query용 API로 DML을 실행한 뒤 오류를 낼 수 있는가 | D6-W01 expected-result를 첫 효과/DDL precommit 전에 검사. raw SQL mode 변경은 authoritative terminal로 수신. server private parser를 client에 복제하지 않음; T05 |
| JDBC/ODBC auto ON이 native Active SET 오류와 충돌하는가 | 명시 COMMIT→SET 조립·단계 outcome, native/raw 규칙 유지. 단순 실패/성공 boolean로 축소하지 않음; T10·C06 |
| bind에서 caller 메모리를 언제 읽는가 | native/JDBC는 값 복사, ODBC는 descriptor 등록 후 Execute마다 복사. 두 경우를 한 규칙으로 합치면 반복 값 또는 수명 버그; T07/14/15 |
| cancel 성공·socket close가 rollback 완료인가 | generation ticket·Requested와 terminal·외부 ACK 분리. 늦은 신호가 다음 fetch에 작용하지 않음; T03/04·C03/08 |
| 단일 공용 binary로 Rust/Java를 묶을 것인가 | Rust core+FFI와 순수 Java codec. JNI 배포/unsafe 경계 비용 대신 독립 wire conformance corpus 비용을 선택; F01/F03/F05 |
| JDBC/ODBC 이름만으로 표준 전체 지원을 표시할 것인가 | API signature/overload·type/length·feature 탐지 inventory. 미지원은 명시 오류, 가짜 성공/빈 catalog 금지; T13/14/17 |
| 메모리를 제한하려면 결과를 항상 caller buffer로 빌려줄 것인가 | native owned batch+quota lease, JDBC local row window, ODBC caller descriptor/부분 copy. ownership별 bytes·변환 중 중첩 예약; T08/15/18 |

외부 DBMS의 성능 수치를 XADB 목표로 옮기지 않았다. 최초 비용 guardrail·부하/seed·인수 프로파일은 설계값이며 D6-F07 실행 전 동결과 actual evidence가 필요하다.

<a id="review"></a>
## 4. 독립 검토와 실행 상태

2026-10-19, 작업·client 계약·통합 담당 `/root`와 다른 실행인 `/root/dev6_design_review`가 읽기 전용으로 독립 검토했다. 검토 범위는 DEV6 설계·계획·조사, DEV5 D6-W01 공급 경계, 단계·완료 계약·feature 목록·색인의 DEV6 변경이며, 사용자 추가 요청으로 작성한 PAR01–06 병렬 계획과 의존 그림도 재검토했다. 검토자는 관련 원문 계약, 공식 Java/ODBC 문서, 선택 소스와 외부 archive 원문을 직접 대조했다.

| 지적 | 반영·재검토 결과 |
| --- | --- |
| P2 · JDBC label lookup의 exact 우선 선택 | 대소문자 무시 첫 ordinal을 선택하도록 수정. `"A"`/`"a"` 두 label·서로 다른 값의 반례 추가·확인 |
| P2 · ODBC SQLGetData ordinal 제한 누락 | SQL_GETDATA_EXTENSIONS=0에 맞춰 마지막 bound column보다 큰 unbound ordinal·오름차순 제한, 실패시 row/offset 유지 반례 추가·확인 |
| P2 · ODBC cursor close와 binding 해제의 수명 혼동 | rebind/SQL_UNBIND/statement free까지 column binding 유지, parameter는 reset/rebind/free까지 유지. close→재실행→fetch 반례 추가·확인 |
| P2 · SQLDisconnect의 auto ON+raw BEGIN 경계 | mode와 무관하게 Active이면25000·연결/거래 유지. raw COMMIT/ROLLBACK 뒤 종료와 ACK/reopen 반례 추가·확인 |
| P2 · 최초 병렬 그림이 JDBC/ODBC를 SC04 선행처럼 표시 | driver별 실제 통합과 단계 인수를 분리. ODBC는 native core 공급을 소비하고 C facade 완료를 개발 선행으로 요구하지 않도록 표·그림 일치 확인 |

검토자는 로컬8개 checkout의 remote/revision/clean 상태와 기록한 파일 SHA-256, REF-C06-02 외부 manifest 및 HTML5개 hash도 원본과 대조하여 일치를 확인했다. `/root`는 이 작업에서 작성·수정한 저장소 문서의 중립 출처 표기를 확인했다. 출처 identity 확인은 제품 실행 증거가 아니다.

최종 판단은 **기본 설계와 PAR01–06을 포함한 D2 채택 가능, 잔여 P1/P2 없음**이다. Native/C 소유권·FFI 경계, partial reply/CancelAck·알려진 commit 보존, JDBC/ODBC 거래 조립, 실제 B3 선행·복구·독립 oracle·성능 인수, 공유 파일의 직렬 반영과 SC04→SC05→SC06 순서를 확인했다. `/root`가 수정을 반영·확인하고 DEV6-DESIGN-001을 **ACCEPTED**로 기록했다. 이는 정적 설계 검토이며 미래 leaf 동결·제품 구현·병렬 개발의 검증을 대신하지 않는다.

이번 작업은 원문·소스 열람과 설계 작성이다. Oracle/REF-C06-02/로컬8개 DBMS build·제품 실행, XADB native/JDBC/ODBC 구현·모델·FFI·성능·실제 DEV5 연동·CLIENT1 인수는 **NOT_RUN**이다. 문서 자동 검사는 push 전용이며 이번 편집 중 실행하지 않았다.


<a id="review-002"></a>
## 5. DEV6-REVIEW-002 재검토와 수정

2026-10-19, 사용자의 Oracle·ARIES·공개 로컬 DBMS 및 추가 로컬 소스 참고 리뷰와 문제 즉시 수정 요청에 따라 `/root`가 다시 검토했다. 최초 설계 채택과 위 독립 검토는 당시 범위의 기록이며 이번 수정의 실행 증거로 승계하지 않는다. 변경은 [REVIEW-002](../contracts/dev6-client-design.md#review-002)에 귀속한다.

### 작업 기준·소유

기준 HEAD는 `795b4c974f44aa73b366f93eeb2dd2f22654ece7`, 수정 직전 tracked diff의 SHA-256은 `02fbf5b7ac42332fa6fceac2f385ed7d916240c2c87fcd50f69e38d7de9d4532`다. 미추적 제품 파일까지 포함한 build fingerprint가 아니다. 검토 시작 시 설계 파일 hash는 `e4ac291a1a883ac989a21531130f0cd60fda6c8485478ee7052af8564d880bff`, 계획은 `72de1af1435749515e63a79e50a7c4c742da027ac14dd51f0924b9387d1814a5`, 이 조사 문서는 `cbe62446db5c68b7054aad2d40e634f1647a4b001db37f1f891b13f659d49044`였다.

작업·계약·통합 담당은 `/root`다. 소유 변경은 DEV6 설계·계획·이 조사 문서이고, 공유 수정은 stage-completion의 DEV6 요구와 문서/폴더 색인의 해당 항목이다. 이미 수정 중이던 engine·transport·성능 도구와 무관한 문서 변경을 보존했다. 공통 의미 수정→계획·공유 색인→별도 읽기 전용 검토 순서로 진행하며 생성물은 만들지 않는다. 원본 checkout을 변경하거나 Git 이력·코드·dependency를 가져오지 않았다.

### 공식 원문·재확인한 공개 소스

이번 열람일은2026-10-19다. 다음은 실제 읽은 범위이며 DBMS 실행 비교가 아니다.

| 자료·부분 | 관찰·적용 |
| --- | --- |
| [Oracle19c JDBC coding tips](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-coding-tips.html), C.1–C.2.2 | 직렬 연결 사용, cancel과 interrupt의 차이, SELECT 완료·fetch 배치 확인. 한 API의 전체 조립과 wire request의 수명을 나누고 implicit SELECT 종료를 재검토 |
| [Java21 Connection](https://docs.oracle.com/en/java/javase/21/docs/api/java.sql/java/sql/Connection.html), setAutoCommit/commit/rollback | 모드가 실제로 바뀔 때의 commit과 같은 모드 no-op, ON의 commit/rollback 오류를 재확인. 추가 로컬 구현의 편의상 no-op을 XADB JDBC 규칙으로 복제하지 않음 |
| [Java21 Statement](https://docs.oracle.com/en/java/javase/21/docs/api/java.sql/java/sql/Statement.html), getResultSet/getUpdateCount/getMoreResults | 결과가 하나여도 close·현재 결과 소진·-1 종료 표시는 필요. API 이름만 나열한 지원표에 구체 상태 전이 추가 |
| [ODBC SQLGetData](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetdata-function?view=sql-server-ver17), Arguments·Retrieving Data·fixed-length | 가변 buffer 길이와 고정 C type 크기를 분리. NULL TargetValuePtr 금지·고정형 반복 NO_DATA를 R2-V06에 연결 |
| [ODBC SQLEndTran](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlendtran-function?view=sql-server-ver17), Comments — 별도 검토자가 확인 | auto ON의 no-op에서 cursor를 폐기하지 않도록 보완. 실제 transaction 종료·이미 완료한 close·Broken/Fenced의 효과와 구별 |
| [ARIES 공식 서지](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging), [1992 원 논문 공개 사본](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf) PDF1·3–6쪽(인쇄94·96–99쪽) | commit log force, WAL, partial/total rollback, redo-only CLR·반복 장애의 근거 확인. DEV6은 client 응답 관측과 engine durable outcome을 구분하고 C03/04/08의 실제 reopen/recrash 의무 유지. ARIES만으로 ACK 전달·재시도 안전성을 증명하지 않음 |

PostgreSQL·MySQL·SQLite·CUBRID는 이번에도 remote/revision과 clean working tree를 실제 확인했다. revision과 파일 hash는 위 [로컬 identity 표](#local)의 같은 네 항목과 일치했다. 이번에 다시 읽은 범위는 다음과 같다.

| checkout·revision | 실제 함수·범위 | 판단 |
| --- | --- | --- |
| postgres · `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | fe-exec.c의 PQsendQueryStart 1690–1735, PQgetResult 2079–2150 | command admission과 flush/read/result 완료 구분. XADB 복합 API guard·송신 실패 경계와 비교 |
| mysql-server · `3b99be40f8a31d9396cad10accfd3ba51647d613` | libmysql.cc의 stmt_fetch_row/mysql_stmt_fetch 3908–4005 | 이미 받은 row와 caller buffer의 NULL/truncation 상태를 별도 관리. client cached row와 server EOF를 구분 |
| sqlite · `43a48f77585a88940c727f33682151418f544059` | vdbeapi.c의 finalize/reset/clear_bindings 진입 105–160 | 실행 종료·재사용·binding 해제를 별도 수명으로 취급. 기존 ODBC binding 보존 규약 유지 |
| cubrid · `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | cas_handle.c의 hm_srv_handle_free 149–180 | handle 내용·임시 자원·slot 해제 구분. XADB generation/owned resource 경계 유지 |

이 네 source 파일의 bytes는 기존 hash와 같지만 이번 열람 자체를 과거에 수행한 것으로 바꾸지 않는다. 다른 공개 checkout을 이번에 다시 분석했다고 주장하지 않는다.

### 추가 로컬 자료: LOCAL-REF-A

사용자가 지정한 별도 checkout은 중립 식별자 `LOCAL-REF-A`로만 인용한다. HEAD `0682bdd8610c3728810df616c95a858cda675706`, working tree의 변경 항목1개를 관측했으므로 clean으로 표시하지 않는다. 선택 소스의 실제 bytes를 아래 SHA-256으로 식별하며 source fingerprint나 실행 결과로 취급하지 않는다. 원문과 원래 경로는 외부 원본에 그대로 두고 제품/패키지 명칭·원문 경로 inventory를 XADB에 전사하지 않는다.

| ID·부분 | 읽은 내용 | SHA-256 |
| --- | --- | --- |
| A1 · C cancel, 294–340·426–474 | statement identity/CID 대상 cancel, 응답 수신과 호출 정리의 분리. 다른 연결 cancel 방식을 XADB에 도입하지 않음 | `50bb22f74e5064eed8d4bfc97a2d2c4ed845773255220e4e246fa2f6b7c83bb7` |
| A2 · C transaction end, 175–235 | DBC context·인자 확인·transaction 종료 main 호출·진단 반환 경계 | `0595ab115969343adc6f703af4f7bded91202ce06d0e3b511d36c9412a580811` |
| A3 · C GetData, 433–500 | column ordinal/target type와 bound descriptor의 구분. 정확한 표준 buffer 의미는 공식 ODBC 문서로 판정 | `996488abdb736bad563358c6737bd1c4ff7973d0b3cdc4ef599fbc0f1b4cdb13` |
| A4 · Java connection, 1182–1208·1569–1594·1627–1678 | commit/rollback·mode 변경 조립. auto ON의 편의 no-op과 재연결 경로는 XADB에 채택하지 않음 | `db59128d6c144787990a6331997846674547cd06de4b52ca399ff824564c89b5` |
| A5 · Java statement, 471–492·623–665·1136–1168 | 결과 정리·cancel·getMoreResults의 경계. 단일 결과도 종료 상태를 독립 명시해야 한다는 비교 입력 | `8f443f61bd6c96fafa2681ef90d1b40d470a8934f4b0db9c3c75a41edaa1a55d` |

### 발견 사항과 수정

| 우선순위·지적 | 반례·수정·인수 연결 |
| --- | --- |
| P2 · request ID의 local 예약/송신 소비 경계 누락 | encode/할당 실패가 ID를 소비하면 다음 요청이 DEV5 연속 ID 검사에서 거절됨. local generation 분리·FIRST enqueue 수락 때 소비; R2-V01/M01 |
| P1 · 복합 실행의 직렬화·취소 범위 누락 | BindAll terminal 뒤 cancel이 사라지거나 다른 호출이 끼면 취소된 Execute가 실행될 수 있음. public API 전체 guard·cancel latch·남은 deadline 유지; R2-V02/C02/M02 |
| P1 · implicit SELECT close 뒤 거래 상태 재판정 누락 | ON SELECT close가 Idle로 만든 뒤 COMMIT을 보내면 TxnState로 모드 변경 실패. close terminal 뒤 Idle/Active를 재판정, JDBC/ODBC 차이와 실패 단계 보존; R2-V03/M03 |
| P2 · server EOF와 client result 수명 경계 불명확 | 마지막 batch를 받은 뒤 rollback/commit해도 남은 local row가 재노출될 수 있음. cached row/offset까지 명시 무효화, implicit EOF의 정상 소비 예외 유지; R2-V04 |
| P2 · JDBC 결과 소진 전이 누락 | getMoreResults=false만 반환하고 count를 남기면 표준 결과 순회가 끝나지 않음. None 전이·null/-1·close 오류 구체화; R2-V05 |
| P2 · SQLGetData buffer0의 적용 범위 오류 | 고정 C numeric도 길이 조회로 처리하면 실제 값 복사를 생략. 가변형 probe·고정형 실제 크기·NULL pointer를 분리; R2-V06 |

ARIES 대조에서는 기존 complete terminal ACK·durable outcome·CLR/recrash 분리의 방향을 유지했다. 이 정적 판단을 복구 시험 PASS로 승격하지 않는다. 기본 wire 변경이나 새 client 기능을 추가하지 않았고 미동결 F01–F07을 완료로 바꾸지 않았다.

### 독립 검토·검증 상태

별도 실행 `/root/dev6_boundary_review`가 수정 전 사본 대비 diff, DEV4/DEV5/COMM 원문, 위 Java21 Statement/Connection·ODBC SQLGetData/SQLEndTran 공식 문서와 반례를 읽기 전용으로 검토했다. 빈 CHAR/WCHAR의 terminator 공간 부족과 기존 빈 값 SUCCESS 문구 충돌(P2)을 지적했고, 본문과 R2-V06의 `01004·미소진 → SUCCESS → NO_DATA` 수정을 확인했다. 이어 no-op/진입 거절의 cached row 보존과 이미 완료한 close의 효과 유지 문구·R2-V04 보완도 확인했다.

최종 판단은 **검토 범위의 잔여 P1/P2 없음·정적 설계 채택 가능**이다. `/root`가 client 계약·통합 담당으로 반영을 확인하고 REVIEW-002를 ACCEPTED로 기록했다. 이번 독립 실행은 ARIES/로컬 소스 원문 재열람이나 source hash 재검증까지 수행한 것은 아니다. 그 조사와 identity 기록은 위 `/root`의 실제 열람 범위에 귀속한다.

코드·제품 동작 변경 없이 계약과 인수 의무를 보완했으므로 제품/FFI/모델 실행은 NOT_RUN이다. 문서 자동 검사도 저장소의 push 전용 정책에 따라 실행하지 않았다. commit/push는 이번 작업 범위에 포함하지 않았다.

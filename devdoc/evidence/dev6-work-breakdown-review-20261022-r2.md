# DEV6 작업 배정 2차 점검 — API 분량과 인수 연결

2026-10-22 · **기존 계약 안의 D0 문서 보완 / 담당 정적 대조 / 제품 구현·시험·CLIENT1 NOT_RUN**

[보완한 분류](../plans/dev6-work-breakdown.md) · [인수 연결](../plans/dev6-work-breakdown.md#handoffs) · [실행 예산](../plans/dev6-implementation-plan.md#execution-budget) · [의미 정본](../contracts/dev6-client-design.md) · [이전 검토](dev6-work-breakdown-review-20261022.md)

사용자의 DEV6 잔여 작업 분배·분량·우선순위·내용·분류 재검증, ARIES·Oracle·로컬 DBMS 참고, 개선 즉시 반영 및 한 번 commit 요청에 따른 후속 점검이다. 작업·문서 담당은 `/root`다. 최초 확인 HEAD는 `ece6600`이고, 착수 사본 보존 시 HEAD는 동시 DEV7 작업이 반영된 `f6cb181a896f647a91fb658a721476a1f4a22137`였다. 보존 시 추적 파일 diff SHA-256은 `f94c3cb635da8c2e545891a2d0f858cc745d03d4df0c58a3336dfaaeebf321e6`이다. 다른 DEV의 변경·commit은 이번 수정 범위에 포함하지 않는다.

## 1. 판단과 수정

상위 18개 책임·55개 기존 묶음과 공통 계약→작은 실제 연결→거래/장애→지원표 확대→잔여 인수의 순서는 유지하는 것이 적절하다. native core와 C/ODBC adapter·독립 Java·검증 도구의 소유를 구별하고, SC04→SC05→SC06을 실제 인수 순서로만 적용한 선택도 유지한다. 다만 다음 항목은 공급·완료 단위가 아직 크거나 담당이 암묵적이었다.

| 관찰 | 수정 | 기존 의미·의무의 출처 |
| --- | --- | --- |
| C header/ABI·출력 소유·모든 실제 C 호출이6.10.3 하나에 있음 | ABI/진입·해제 기반, 값/metadata/진단, API별 실제 연결의3개 자식. header/load·실패/해제 확인을 먼저 공급 | [native](../contracts/dev6-client-design.md#native), F03·T15/17 |
| JDBC6.12.1에 statement·setter·metadata가 섞이고 DatabaseMetaData/Connection 속성의 주 담당이 드러나지 않음 | 기본 실행/descriptor·parameter/prepare·DB capability/속성의3개 자식. 거래/모드 구현은6.12.3.1에 유지 | [JDBC 지원표](../contracts/dev6-client-design.md#jdbc), F02·T05–09/13 |
| ResultSet 상태·실제 fetch·모든 getter 변환을 하나로 배정 |6.12.2를 row 수명/결과 소진과 getter/열 탐색으로 구별. local row vector 준비와 실제 소비 통합을 연결 | [값](../contracts/dev6-client-design.md#values)·[거래/결과 수명](../contracts/dev6-client-design.md#transactions), R2-V04/05 |
| ODBC6.14의 각 항목이 descriptor/복사, 변환/row 소비, offset/UTF 부분 조회를 함께 포함 | 각 부모를2개 자식으로 나눔. 출력 변환의 주 담당은6.14.2.1, Fetch/GetData의 서로 다른 소비 상태는 각 API 담당에 유지 | [ODBC](../contracts/dev6-client-design.md#odbc), F04·T06–09/14/15·R2-V06 |
| SQLGetTypeInfo는 결과 집합을 반환하는데6.13.2.2의 탐지 완료에만 연결됨 | 행/metadata 생산과 describe/fetch/EOF/close 통합을 연결. scalar flag 완료만으로 type-info 완료를 주장하지 않음 | 기존 필수 ODBC 지원표, T08/09/14/15. 정확한 열/type/state는 F04 미동결 |
| native 직접 실행이 facade 목록의 execute에 암묵적으로 포함되고6.07.2가 prepare/binding을 일괄 선행으로 적어 direct fetch/거래도 대기할 수 있음 |6.07.2의 direct/prepared 생산 범위·6.08/09 소비·6.10.1의 execute_direct 공개를 명시. 신규 API를 만들지 않고 기존 direct 경로의 담당을 드러냄 | 기존 native API·D6-W01·T05/11 |
| 성능 예산의 driver3종을 실제 호출 경계3개로 오해할 수 있음 | Rust API/C facade/JDBC/ODBC DM의 실제 entrypoint를 cell에 표시. core 측정으로 adapter 비용을 대체하지 않음 | 기존 F07·T18/P01–04의 actual entrypoint·copy/encoding 측정 의무 |

종전 최하위 배정 단위72개에서6개를14개 자식으로 교체하므로80개다. 부모는 집계만 하며 기존 ID는 보존한다. 이는 기능이나 시험 의무8개를 추가한 것이 아니다. F02/F04 전개 전에는 실제 signature/type/state 조합 수와 구현·검토·도구 준비 비용을 알 수 없으므로 동일 일수나 완료율로 환산하지 않는다. 기계 점유 시간은 별도 계산하고, 기존108시간 예시는 실제 전체 일정으로 쓰지 않는다.

18개를 전면 재분류하거나 함수별 ID를 만드는 대안은 기존 책임과 참조를 흩뜨린다. 반대로 큰 묶음을 그대로 두면 getter·capability·C buffer 인수를 뒤늦게 발견할 수 있다. 선택한 분해는 독립 산출물과 소비 지점을 드러내는 범위에 한정한다. 현 계약에서 합리적인 배정이며, 실제 작업 시간·재작업·병목 관측 없이 유일한 최적 분배나 기간 단축을 입증했다고 주장하지 않는다.

## 2. 이번에 직접 열람한 공식 자료

| 자료·열람 범위 | 확인 내용과 적용 한계 |
| --- | --- |
| [ARIES, 1992](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf), 인쇄96–97·109쪽의 stable log·CLR/UndoNxtLSN | 로그 강제 기록·반복 장애 중 undo 진행은 engine 책임이다. DEV6는 outcome 보존·외부 ACK·실제 reopen 인수를 담당한다. 논문이 driver API 분할이나 응답 유실 후 재전송의 안전성을 정한다고 해석하지 않음 |
| [Oracle19c JDBC Coding Tips](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-coding-tips.html), C.1·C.2.1/2 | 연결 사용·취소, 결과 완료와 auto-commit, fetch 크기의 별도 영향을 확인. XADB의 호출 전체 guard·result 수명·fetch/변환 분리 근거로 비교하며 Oracle의 cache/batch/pool을 초기 범위에 추가하지 않음 |
| [Oracle19c Transaction Guard](https://docs.oracle.com/en/database/oracle/oracle-database/19/adfns/transaction-guard.html), 29.1/29.2 | 응답 유실과 commit 여부는 다르며 안전한 재제출에는 별도의 영속 식별·outcome 확인·진행 차단 장치가 필요하다. XADB의 Unknown/no-replay와 driver별 장애 인수를 유지 |
| [Java SE21 DatabaseMetaData](https://docs.oracle.com/en/java/javase/21/docs/api/java.sql/java/sql/DatabaseMetaData.html), interface 설명 | DBMS와 driver 조합의 capability를 보고하는 책임을 확인. column/parameter descriptor와 구별해6.12.1.3에 배정. XADB의 명시 부분 지원·catalog 열거 미지원은 유지 |
| [ODBC SQLGetTypeInfo](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgettypeinfo-function?view=sql-server-ver17), Summary·Comments·Related Functions | 반환물이 표준 결과 집합이고 column binding/fetch와 연결됨을 확인. 행/metadata 생산뿐 아니라 실제 소비·해제 통합을 기존 인수에 배정. 전체 ODBC 표준 지원이나 정확한 미동결 F04 값을 여기서 확정하지 않음 |
| [ODBC SQLGetData](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetdata-function?view=sql-server-ver17), TargetValuePtr/BufferLength·처리 절차 | 고정 크기 출력과 가변 buffer·terminator의 차이를 재확인. 변환 기반을 공유해도 row 소비/column offset을 한 상태로 합치지 않으며 기존 R2-V06 유지 |

왼쪽은 열람 자료, 오른쪽의 배정은 XADB 정본과 대조한 판단이다. 다른 DBMS의 기능 목록·성능을 XADB의 지원 또는 실행 증거로 전용하지 않는다.

## 3. 로컬 소스의 실제 범위와 identity

[로컬 카탈로그](../reference/opensource-references.md)에서 아래5개 checkout의 origin·HEAD·작업 트리를 이번에 확인했다. 모두 표의 공식 origin이며 clean이었다. 아래 파일의 해당 함수만 읽었고 checkout 수정·갱신·build·실행은 하지 않았다. 카탈로그의 `/home/et16/work/duckdb`는 이번 조회 때 없었으므로 조사 근거에서 제외했다. 이전 조사 당시의 존재·identity 기록을 소급 수정하지 않는다.

| checkout·origin | 실제 HEAD | 읽은 파일·범위 | 배정에 사용한 관찰 |
| --- | --- | --- | --- |
| `/home/et16/work/postgres` · `https://github.com/postgres/postgres.git` | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/interfaces/libpq/fe-exec.c`: PQclear 723–779, PQsendQueryStart 1686–1755, PQgetResult 2079–2185 | 결과 메모리 소유·요청 admission·결과 진행/완료는 구별된다. facade 조립과 owned result/close 담당을 명시 |
| `/home/et16/work/mariadb` · `https://github.com/MariaDB/server.git` | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `sql-common/client.c`: mysql_free_result 749–775, mysql_close_slow_part/mysql_close 3468–3507 | 결과 drain·메모리 해제·blocking 가능 종료를 구별. C 해제 기반과 실제 종료 호출을 함께 추적하되 XADB Drop의 비차단 계약 유지 |
| `/home/et16/work/mysql-server` · `https://github.com/mysql/mysql-server.git` | `3b99be40f8a31d9396cad10accfd3ba51647d613` | `libmysql/libmysql.cc`: mysql_stmt_bind_result 3829–3865, stmt_fetch_row/cli_unbuffered_fetch/mysql_stmt_fetch/mysql_stmt_fetch_column 3908–4051 | output binding·NULL·row 소비·부분 column 조회가 다른 책임이다. ODBC 변환과 소비 상태의 분량 구분에 참고하며 MySQL 반환 규칙을 ODBC oracle로 사용하지 않음 |
| `/home/et16/work/cubrid` · `https://github.com/CUBRID/cubrid.git` | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/broker/cas_handle.c`: hm_new_srv_handle/find/free 58–178, hm_qresult_end 337–385 | handle slot/내용과 query result 수명을 구별. XADB generation·parent/FFI 안전성은 별도 기존 의무이며 이 코드만으로 증명하지 않음 |
| `/home/et16/work/sqlite` · `https://github.com/sqlite/sqlite.git` | `43a48f77585a88940c727f33682151418f544059` | `src/vdbeapi.c`: finalize/reset/clear_bindings 105–187, columnMem/columnMallocFailure 1399–1449 | 실행 reset·binding 초기화·row 접근/변환 실패를 구별. getter/row 상태·parameter 등록을 분리하되 SQLite의 NULL/reset 의미는 이식하지 않음 |

읽은 파일의 전체 bytes SHA-256은 다음과 같다. 전체 파일 hash가 전체 코드 열람·실행을 뜻하지 않는다.

| 파일 | SHA-256 |
| --- | --- |
| PostgreSQL `fe-exec.c` | `e77c1df9bfd0d350c018b9be51fcb9cf3419ab5d84540bafb23dcc375222bf99` |
| MariaDB `client.c` | `40d26fe8713eaa276c8b095ec055034eea4f31162c49e2feb9b7254ec0bdf3a6` |
| MySQL `libmysql.cc` | `71ad5800bbf7e4e83c39e8c03b2aa7694d51387d8d4a590166e5928a0bb50531` |
| CUBRID `cas_handle.c` | `281bf4ee5cb7bec161cf05ee17d0e1c929a18f493c1af0e57b585152022bc53d` |
| SQLite `vdbeapi.c` | `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5` |

## 4. 검토 분류·범위와 미실행 상태

[DP01](../policies/development-policy.md#section-2)의 D0로 분류한다. 기존 공개 API·SQL/wire·FFI/수명·지원 범위·quota·모델 bound·시험/성능 기준·SC 순서를 변경하지 않고, 이미 채택된 의무의 문서상 담당·완료 단위·소비 관계를 풀어 썼기 때문이다. 새 내부 API나 구현 구조를 확정하지 않았다. F02/F04의 실제 signature/열/type/state·C unsafe 경계 변경과 도구 구현은 계속 별도 D2/독립 검토 대상이다. 과거002의 독립 검토를 이번 수정의 독립 승인으로 기재하지 않는다.

담당이 정본·신규 diff·공급/소비 관계를 정적으로 대조했다. C/JDBC/ODBC 분할 후에도 원래 T/C/M/P/R2 의무가 남고, local 준비와 실제 연결·종료 인수가 서로의 전체 완료를 기다리지 않도록 구별했다. 작은 연결 경로·거래/장애 우선순위는 유지하며 ABI 확인과 capability 대조를 그 기존 순서에 명시했다. 원래 source/실행 범위의 증거 보존과 무변경 시험 반복 금지도 유지한다.

수정은 DEV6 분류·구현 계획, 이 조사 기록, README와 문서 catalog 등록이다. 단계 JSON은 기존 DEV6 문서를 이미 참조하고 상태·gate 변경이 없어 수정하지 않는다. 기존 조사 문서는 원래 identity로 보존했다. 제품 코드·dependency·build 경로를 추가하지 않았으므로 제품 시험을 실행하지 않았다. 문서 자동 검사는 push 직전 규칙에 따라 실행하지 않았다. 제품 개발 재개·DBMS 실행·성능/복구 측정·push·CLIENT1 승격은 없다.

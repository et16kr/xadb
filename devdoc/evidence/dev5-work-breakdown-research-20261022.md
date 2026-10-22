# DEV5 작업 분류 — ARIES·Oracle·로컬 DBMS 조사

2026-10-22 · **선택 소스·공식 문서 열람 / 분류 근거 / 제품 실행·B3 검증 아님**

[16개 분류·44개 하위 작업](../plans/dev5-work-breakdown.md) · [서버 소유 계약](../contracts/dev5-server-design.md) · [기존 구현계획](../plans/dev5-implementation-plan.md) · [로컬 source catalog](../reference/opensource-references.md) · [기존 DEV5 조사](dev5-source-research.md) · [기존 Oracle 비교](dev5-oracle-source-review.md)

사용자는 DEV3처럼 DEV5를 세분화하고 ARIES·Oracle·`~/work` DBMS를 참고하도록 요청했다. 기존 서버 의미를 바꾸는 대신 **서로 다른 소유·실패·완료 경계를 개발 단위로 드러내는 것**을 조사 질문으로 삼았다. 과거 문서의 조사 사실을 새 열람으로 대신 기록하지 않고 아래 원문과 실제 checkout을 이번에 확인했다. 제품 빌드·DBMS 비교 실행·성능 측정·보안 감사는 하지 않았다.

<a id="external"></a>
## 1. ARIES·Oracle 공식 원문

2026-10-22 열람이다. Oracle은 Database **19c 문서**이며 현재 최신 제품이나 로컬 DBMS checkout과 같은 버전이라고 주장하지 않는다. ARIES는 서버 protocol 규격이 아니라 recovery 경계를 검토하는 입력이다.

| 원문·읽은 범위 | 관측 | DEV5 분류에 적용·유보 |
| --- | --- | --- |
| [Mohan 외, ARIES, ACM TODS17(1), 1992, pp.94–162](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf), 도입 WAL, §5.2 Total or Partial Rollbacks, §6 restart의 Fig.13 주변 | WAL과 transaction 종료, 부분/전체 rollback 및 CLR의 진행을 구별한다. 재시작은 과거 변경과 보상을 REDO한 뒤 남은 loser undo를 진행한다 | 5.10.3 결과 장벽·5.13 cleanup·5.15.3 반복 recovery 인수를 분리. WAL/CLR 알고리즘은 DEV3 소유로 남기며 socket 송신을 내구성 증거로 보지 않음. 원형 ARIES/분산2PC/선택적 재개를 새로 채택하지 않음 |
| [Oracle COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html), Purpose·Prerequisites·WAIT/NOWAIT | DDL 전후 commit과 durable log 뒤 성공 메시지 유실을 명시한다. 일반 COMMIT에는 추가 권한을 요구하지 않음 | 5.10.2 DDL 두 결과, 5.10.3 engine certainty, 5.15.2 외부 ACK를 별도 작업으로 둠. XADB의 COMMIT 권한 재확인·durable ACK는 유지하고 NOWAIT·정상 close commit은 도입하지 않음 |
| [Oracle Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/transactions.html), Statement-Level Atomicity·Transaction Guard 개요 | 문장 성공과 거래 commit은 다르며 실패 문장의 rollback은 이전 성공 문장을 보존한다. Transaction Guard는 별도 outcome/idempotence 기능 | 5.10.1 statement 효과·5.12.2 요청 취소·5.13.2 전체 cleanup을 분리. XADB의 UNKNOWN을 PK 조회나 자동 replay로 해소하지 않으며 durable request lookup은 후속 의무 |
| [Oracle JDBC Troubleshooting](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-troubleshooting.html), E.1.2·E.1.4 | statement/result 명시 close와 cursor 자원, cancel 신호와 원래 실행 thread의 결과 수신, timeout의 network/server 진행 의존을 구별 | 5.09.3 handle 정리·5.11 result 수명·5.12 취소 신호/결과를 분리. XADB의 동일 연결 generation·CancelAck 규약을 유지하고 Oracle 내부 command replay는 채택하지 않음 |
| [Oracle CREATE PROFILE](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/CREATE-PROFILE.html), resource-limit enforcement·CONNECT_TIME·IDLE_TIME | session 수명 초과와 단일 call 제한의 rollback 범위가 다르고 실행 중 query는 idle 시간에 포함하지 않음 | 5.12.3 timer와 5.13.2 전체 cleanup을 연결. XADB의 초 단위 값·다음 요청 없는 cleanup·300초 거래 수명·running/parked 제외는 기존 자체 계약. Oracle CONNECT_TIME을 XADB transaction timer와 동일시하지 않음 |

ARIES CMU 미러의 첫 열람은 timeout이었고 위 Stanford 공개 원문을 성공적으로 읽었다. 실패한 fetch를 열람 근거로 세지 않는다. 외부 설명으로 XADB의 아직 미동결 byte layout·API·성능 수치를 확정하지 않았다.

<a id="local"></a>
## 2. 로컬 소스 identity와 실제 범위

다섯 checkout의 origin·HEAD·`git status --short`를 실제 확인했다. 모두 변경 출력이 없었다. 저장소에 쓰기·fetch/pull·build/test·Git 이력 도입·코드 복사는 하지 않았다. 아래 경로는 checkout 루트이고 파일 SHA-256은 전체 파일의 identity, 줄 범위는 이번에 읽은 부분이다.

| 대상·로컬 경로 | origin·실제 HEAD |
| --- | --- |
| PostgreSQL `/home/et16/work/postgres` | `https://github.com/postgres/postgres.git` · `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` |
| MariaDB `/home/et16/work/mariadb` | `https://github.com/MariaDB/server.git` · `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` |
| CUBRID `/home/et16/work/cubrid` | `https://github.com/CUBRID/cubrid.git` · `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` |
| MySQL `/home/et16/work/mysql-server` | `https://github.com/mysql/mysql-server.git` · `3b99be40f8a31d9396cad10accfd3ba51647d613` |
| SQLite `/home/et16/work/sqlite` | `https://github.com/sqlite/sqlite.git` · `43a48f77585a88940c727f33682151418f544059` |

| checkout·파일·읽은 줄 | SHA-256 |
| --- | --- |
| PostgreSQL `src/backend/libpq/auth.c` 1888–1955 | `d3ab30f5024f21b3ba60959b760898f8d84ba0a90498c3cde8c24fbb8b6ea734` |
| PostgreSQL `src/backend/tcop/postgres.c` 3530–3675 | `86182ec057c0482d38b9df5d555d4a0a50b003d4051ff2a60914ad9ffa67720b` |
| MariaDB `sql/threadpool_generic.cc` 612–687, 1275–1327 | `4f37bdd44fedd6c8c9284e81c44c5566fc6da87c3522a5688a3d05fb7e8f9632` |
| MariaDB `sql/sql_prepare.cc` 3890–3985 | `1c83c960cac5f4e6be24132045e1d9afd9eed4fd374b464c9245d70a7c670fbd` |
| CUBRID `src/communication/network_sr.c` 138–158 | `1a5b947c75628cb3a015a9c7afdee7715a092b22fc59cc9e1d3bde64d8161616` |
| CUBRID `src/communication/network_interface_sr.cpp` 3369–3442 | `755f3ea598daa9f77deeb14f7df690489fe3ca8cce07068307a9025871bc0f2f` |
| MySQL `sql/sql_prepare.cc` 1590–1660, 1810–1915 | `d9e60c6f7d95b61f31513044c73ecdf3345539c8d7f8975a4a10e009959dbf52` |
| SQLite `src/vdbeapi.c` 95–158 | `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5` |

SQLite는 embedded statement 수명 비교에 한정한다. 이번 서버 분류에는 직접 관련성이 낮은 DuckDB·ClickHouse·TiDB·FoundationDB·RocksDB 소스를 열람하지 않았으며 열 곳 전체 조사로 표현하지 않는다.

<a id="application"></a>
## 3. 관측에서 작업·인수로 연결

| 실제 관측 | XADB에서 분리할 책임 | 관련 반례·인수 |
| --- | --- | --- |
| PostgreSQL `auth_peer`는 socket의 OS credential 조회 실패를 거절하고 user map을 검사 | 5.04 인증은5.05 SQL 권한과 별도. UID는 platform 공급이며 principal 문자열만으로 session 생성 금지. PostgreSQL user map grammar는 복제하지 않음 | T03 claimed UID/peer 불일치·조회 실패·unknown principal |
| PostgreSQL `ProcessInterrupts`는 connection loss·query cancel·idle/transaction timeout의 ERROR/FATAL 경로를 구분 | 5.12의 요청 신호·만료와5.13 전체 정리의 효과를 분리. PostgreSQL 거래 오류 규칙을 DEV4에 덮어쓰지 않음 | T09 request cancel 뒤 명시 거래 보존, 상위 만료 Closing·전체 undo |
| MariaDB `wait_begin/end`는 active thread 회계를 바꾸고 `check_stall`은 worker wake/create를 사용 | 5.08 scheduler를 protocol/SQL 실행과 분리하고 holder 진행을 독립 검증. XADB worker4·parked/resume는 자체 선택이며 동적 worker 확대를 이식하지 않음 | T08/M02 lock holder A, waiter B–E 포화·wake 유실·DDL drain |
| MariaDB reset은 cursor/parameter를 정리해 OK를 보내고 close는 reply를 보내지 않음 | 5.09 handle/binding, 5.11 result와 terminal 수명 분리. XADB 명시 close 결과·generation을 유지하고 무응답 close/RESET wire를 새로 추가하지 않음 | T06/07 잘못된 rebind·close/reexecute·최종 descriptor 보존 |
| CUBRID commit/abort의 request attributes가 다르고 handler는 내부 transaction 결과를 pack한 뒤 송신 | 5.05 자기 cleanup 예외,5.10 결과,5.11 송신,5.15 실제 client ACK를 분리. CUBRID 속성을 XADB 권한표로 복제하지 않음 | T04 철회 뒤 ROLLBACK, T13/C04–08 sync·reply·외부 수신 cut |
| MySQL statement precheck는 session statement map·parameter count를 검사하고 execute는 parameter 설정 뒤 실행 | 5.02 decode와5.09 세션 소유/typed bind·execute를 분리. MySQL query attributes의 추가 parameter 허용은 XADB에 도입하지 않음 | T05/06 교차 handle·count mismatch·실패 binding 보존·문자열 합성 mutant |
| SQLite finalize는 VM reset·delete, reset은 reset·rewind로 재사용을 구분 | 5.09.3 statement 종료와5.11 result/실행 종료를 분리. SQLite pointer API·자동 재실행을 wire handle 의미로 가져오지 않음 | T07 stale/EOF/close/reexecute·pin/plan/bind 누수 |

이 관측들은 기존 계약을 세분화할 이유다. 같은 파일 배치·언어·알고리즘·SQL 호환·보안/성능을 보장하는 근거가 아니다. 요구되는 독립 oracle·실제 socket/thread/reopen은 향후 구현과 함께 공급한다.

<a id="work-record"></a>
## 4. 작업·검토·실행 범위

| 항목 | 기록 |
| --- | --- |
| 소유·권한 | 사용자 분류·참고 조사·완료 후 commit 요청. 작업/계약/통합 `/root` |
| 기준 | HEAD `def35fb1e274040eb55d78fa68948ba1610166c1`, 착수 worktree clean |
| 산출물 | 새 분류 계획·이 조사, 기존 DEV5 계획/설계의 진입 링크, README/catalog·단계 JSON의 분류 참조. 실제 제품 경로·package·run_entrypoint·실행 상태는 추가/변경하지 않음 |
| 독립 검토 | [D5-WORK-BREAKDOWN-001](../plans/dev5-work-breakdown.md#decision)에 실제 검토자·결과·수정 반영 기록. 과거 DEV5 리뷰 PASS를 이번 변경의 승인으로 사용하지 않음 |
| 실행 | 공식 원문·선택 소스·revision/hash 열람. 제품·모델·성능·복구 시험, compiler/Clippy·자동 문서 검사는 미실행. commit은 사용자 요청 범위이며 push는 하지 않음 |

분류 채택 이후에도 DEV5/B3·SC03/KU14/PA08/PF11과 D5-F01–F05의 실제 동결·실행은 미완료다. 과거 조사·시험의 source identity와 결과는 수정하지 않았다.

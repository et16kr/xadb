# DEV5 — Oracle·로컬 DBMS 비교와 후속 설계 검토

2026-10-18 · **DEV5-REVIEW-003 D2 ACCEPTED / 소스·문서 열람 및 독립 설계 검토 / 제품 실행 NOT_RUN**

[서버 계약](../contracts/dev5-server-design.md#oracle-server-review) · [인수 계획](../plans/dev5-implementation-plan.md) · [최초 조사](dev5-source-research.md) · [로컬 source catalog](../reference/opensource-references.md)

사용자의 DEV5 설계 리뷰·문제 즉시 수정·Oracle 및 `~/work` DBMS 참고 요청에 따른 기록이다. Oracle은 공개 공식 문서를, PostgreSQL·MariaDB·CUBRID는 실제 로컬 소스를 읽었다. 제품 실행·보안 감사·성능 비교나 Oracle 내부 구현 확인을 수행한 기록이 아니다.

## 1. 반례와 수정 범위

| ID·중요도 | 수정 전 반례 | 반영·필수 인수 |
| --- | --- | --- |
| D5-R01 · P1 | S0가 BEGIN/UPDATE 뒤 idle로 row lock을 보유한다. S1–S4가 같은 row를 수정하며 worker4를 모두 차지하면 S0의 COMMIT도 queued다. engine graph에는 waiter→S0만 보여 worker 자원 순환을 검출하지 못한다. snapshot을 보유한 result와 DDL drain도 같은 문제가 생긴다 | 동시 DEV5-REVIEW-002의 parked/resume 및 실제 하위 공급 전 통합 금지를 수신·대조. T08/M02에서 holder 종료 요청이 timeout에 의존하지 않고 진행해야 한다 |
| D5-R02 · P2 | Active 거래가 매번 30초 안에 statement를 완료하면 idle을 회피한다. 기존 300초 표에는 만료 후 상태·전체 rollback 소유가 없어 현재 statement 취소 후 Active로 돌아갈 수 있다. 최초 ordinary 전 idle 시작과 running/parked 동안 idle 적용도 불명확했다 | 002의 거래 만료→Closing·전체 cleanup·commit 결과 보존은 유지. 003에서 최초 Ready·terminal·완결 요청 인계와 idle 적용 구간을 명시. T09/T12의 경계·다음 요청 없는 cleanup·Busy/parked 반례 |
| D5-R03 · P2 | 권한 표는 자기 ROLLBACK/cancel/close를 허용하지만 바로 아래 모든 ordinary의 connect/권한 재검사는 이를 다시 거절한다. read/write/connect 철회 후 자원 해제를 요청하면 blanket gate가 cleanup까지 막을 수 있다 | 003에서 정확한 cleanup allowlist와 일반 권한 gate 예외·Closing의 정리 소유를 단일 규칙으로 정의. T04에서 철회 후 cleanup 및 교차 session token 거절을 함께 검증 |

R01의 대안으로 최대32 세션별 전용 worker와 thread/stack·Closing/join 예약도 검토했다. 이는 단순한 다른 설계지만, 같은 문서에 동시 반영된 002의 continuation 계약도 worker 순환을 제거하는 충분조건을 제공한다. 이 후속 작업은 현행 worker4/parked 기준선을 보존한다. continuation 공급·실제 lock/DDL wait 진행·취소 인계는 여전히 F03의 구현 전 의무이며 소스 연구로 충족하지 않는다.

## 2. 로컬 소스 identity와 읽은 범위

세 checkout의 origin·HEAD·`git status --short`를 실제 확인했으며 변경 출력은 없었다. 아래 SHA-256은 해당 파일 전체의 값이다. 줄 범위는 그 revision에서 읽은 부분이다. source checkout에 쓰기·빌드·Git 이력 도입·외부 코드 복사를 하지 않았다.

| 대상·경로 | origin·실제 revision |
| --- | --- |
| PostgreSQL `/home/et16/work/postgres` | `https://github.com/postgres/postgres.git` · `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` |
| MariaDB `/home/et16/work/mariadb` | `https://github.com/MariaDB/server.git` · `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` |
| CUBRID `/home/et16/work/cubrid` | `https://github.com/CUBRID/cubrid.git` · `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` |

| 파일·읽은 줄 | SHA-256 |
| --- | --- |
| PostgreSQL `src/backend/libpq/auth.c` 1888–1955 | `d3ab30f5024f21b3ba60959b760898f8d84ba0a90498c3cde8c24fbb8b6ea734` |
| PostgreSQL `src/backend/tcop/postgres.c` 3540–3700 | `86182ec057c0482d38b9df5d555d4a0a50b003d4051ff2a60914ad9ffa67720b` |
| MariaDB `sql/threadpool_generic.cc` 612–687, 1275–1327 | `4f37bdd44fedd6c8c9284e81c44c5566fc6da87c3522a5688a3d05fb7e8f9632` |
| MariaDB `sql/sys_vars.cc` 4930–5007 | `36c471fe580075ff684ec9d7c107965c04d213dfe1584ec44dca0a7778214990` |
| CUBRID `src/communication/network_sr.c` 132–160 | `1a5b947c75628cb3a015a9c7afdee7715a092b22fc59cc9e1d3bde64d8161616` |
| CUBRID `src/communication/network_interface_sr.cpp` 3369–3465 | `755f3ea598daa9f77deeb14f7df690489fe3ca8cce07068307a9025871bc0f2f` |

| 관측 | XADB에 적용·적용하지 않은 내용 |
| --- | --- |
| PostgreSQL auth_peer는 accepted socket에서 OS peer credential을 얻고 user map을 검사하며 credential 획득 실패를 거절한다 | 현재 UID/principal 허용 목록과 실패 시 거절 선택을 유지. PostgreSQL user-name lookup 방식/설정 grammar를 복제하지 않음. T03 |
| PostgreSQL ProcessInterrupts는 query cancel과 connection lost·idle/transaction timeout의 ERROR/FATAL 경로를 구별한다 | 요청 취소의 statement 효과와 세션 종료의 전체 cleanup을 구분. PostgreSQL 오류/거래 abort 의미 자체는 DEV4 대신 채택하지 않음. T09/T12 |
| MariaDB threadpool은 대기 시작/종료를 active thread 수에 반영하고 stall 시 worker를 깨우거나 생성한다 | 고정 풀의 blocking wait에는 별도 진행 설계가 필요하다는 비교 근거. XADB의 bounded continuation은 자체 선택이며 MariaDB와 같은 알고리즘이라는 주장은 하지 않음. F03/T08/M02 |
| MariaDB autocommit 활성화는 pending 거래의 commit을 호출한다 | XADB의 Active + SET AUTOCOMMIT 거절과 다름을 확인. adapter에서 외부 DBMS 동작으로 바꾸지 않음. T10 |
| CUBRID commit/abort request attributes는 다르고 handler는 내부 거래 결과를 reply에 담아 보낸다 | 정리 경로와 새 변경 권한의 구분, engine outcome과 송신의 분리를 참고. CUBRID의 속성을 XADB 권한 표로 그대로 이식하지 않음. T04/T13 |

## 3. Oracle 공식 문서 대조

모두 **Oracle Database 19c 문서, 2026-10-18 실제 열람**이다. 명시된 절만 근거로 쓰며 local master DBMS 소스와 같은 버전이라고 하지 않는다.

| 원문·열람 절 | 관측과 XADB 판단 |
| --- | --- |
| [SQL Language Reference — COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html), Purpose·Prerequisites·WAIT/NOWAIT | 문법상 유효한 DDL 앞 commit, 일반 거래 commit의 권한 비요구, durable commit 후 응답 유실 가능성을 확인. DEV4의 DDL 두 outcome과 외부 UNKNOWN 구분을 유지. XADB의 commit 권한 재확인과 durable ACK는 자체 계약이며 NOWAIT는 도입하지 않음 |
| [Concepts — Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/transactions.html), End of a Transaction·Statement-Level Atomicity | 문장 실패와 이전 성공 문장의 거래, 문장 성공과 commit을 구분. CloseSession에서 미commit 거래를 rollback하는 XADB 계약을 유지하며 도구별 정상 종료 commit을 가져오지 않음 |
| [JDBC Troubleshooting](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-troubleshooting.html), E.1.2·E.1.4 | 명시 close로 자원을 해제하고 cancel 발송·취소 결과 수신을 구분. 취소는 network/server 진행에 의존한다. XADB CancelAck를 rollback 완료로 취급하지 않고 기존 terminal/CancelAck 장벽 유지. Oracle JDBC 내부 replay 동작은 도입하지 않음 |
| [SQL Language Reference — CREATE PROFILE](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/CREATE-PROFILE.html), Semantics resource-limit enforcement·CONNECT_TIME·IDLE_TIME | 연결/idle 수명 초과는 거래 rollback·세션 종료, call 한도는 해당 문장 rollback과 거래 유지로 구분한다. idle 한도에서 실행 중 작업을 제외한다. XADB의 초 단위 값·즉시성·clock·cleanup deadline은 자체 계약이며 Oracle의 분 단위 profile과 동일하게 보장하지 않음 |
| [JDBC OracleConnection](https://docs.oracle.com/en/database/oracle/oracle-database/19/jajdb/oracle/jdbc/OracleConnection.html), CONNECTION_PROPERTY_AUTO_COMMIT_SPEC_COMPLIANT | 기본 JDBC OFF→ON 전환은 거래를 commit한다. XADB DEV4는 Active에서 TxnState이므로 protocol adapter에서 암묵 commit을 추가하지 않는다. DEV6 JDBC 대응은 별도 계약·인수 의무 |

19c의 server-process architecture URL은 열람 도구가 실패하여 이 조사 근거에 포함하지 않았다. Oracle 제품·DBMS 비교 실행을 하지 않았으므로 동일 동작/보안/성능을 인증한 것이 아니다.

<a id="work-record"></a>
## 4. 작업 소유·동시 변경·독립 검토

| 항목 | 기록 |
| --- | --- |
| 작업·계약·통합 | `/root`, 현재 채팅 `01a14f3d-7736-74c1-94d3-dae23716d7c0`. 사용자 요청과 DP01 D2 위임 |
| 착수 기준 | HEAD `f8b5d8648b55f0242954ee2cdb662926087d9bfa`, DEV5 관련9개 파일 사본 `/tmp/xadb-dev5-review-base-6x087iqg`; identity.json SHA-256 `fb2607b38a63c7aaf042b4a70dba348ea1128b66a15c36afdf6fdb6461eb490e`. 이 identity는 기록한 파일 범위·당시 status이며 전체 작업 트리 지문은 아님 |
| 수정 소유 | 이 조사 파일, DEV5 계약의 cleanup/idle 및 003 결정, 계획의 해당 F02/F03/T04/T09/T12/known-bad, SC03/DEV5 JSON의 해당 의무·design refs, 문서/폴더 색인. 제품 코드·기존 실행 증거 제외 |
| 동시 작업 | 별도 채팅 `01a14f3c-6f7f-7a12-be01-65cc7b793400`도 DEV5를 수정 중임을 쓰기 전에 확인. 최초 patch는 문맥 불일치로 미적용. 002의 worker parked/resume·거래300초·slot/fragment 수정은 보존하고 재검토로 수신. 동일 이름의 agent를 혼동하지 않도록 채팅 identity 병기 |
| 002 수신 기준 | 해당 채팅 완료 뒤 003 직전 관련8개 파일을 `/tmp/xadb-dev5-review003-input-j3tdcxdl`에 보존. 계약 SHA-256 `3c490510c05cd7bf06813023ecca86a19dee045ee69d84541f1bc65b80c08ec8`, 계획 `0e033dd16ff0309a331301ce449628c30efb3f1e8757df95b7ecb65e7fd084dd`. 이 사본과 신규 조사 파일을 003 diff 독립 검토 입력으로 사용 |
| 대조·반영 순서 | 원문/하위 계약·실제 source/Oracle 열람 → 002 변경 수신 → 003의 좁은 계약·인수·색인 반영 → 별도 읽기 검토 → 지적 수정. 변경 중 HEAD 이동·DEV2/COMM/DEV3/DEV4의 별도 변경은 이 작업의 검증/승인 범위 밖 |
| 독립 검토 | 현재 채팅의 `/root/dev5_independent_review`가 원문부터 검토해 R01 및 R02/R03 계약 문제 확인. 002의 실제 공급 gate 포함 충분조건도 대조. 003 기존8개 파일의 DEV5 diff·신규 조사1개, 위3checkout origin/HEAD/status·6파일 hash/선택 코드·초기 identity hash, Oracle19c 5개 URL의 해당 절과 문서 등록을 독립 확인: 채택 가능·잔여 필수 P1/P2 없음. `/root` 계약·통합 담당 반영 확인. 검토자는 파일 쓰기·자동 문서 검사·제품/모델/성능/복구 실행을 하지 않음 |
| 실행 상태 | source identity/hash·선택 소스와 Oracle 문서 열람. 제품·모델·thread/fuzz·성능·복구 실행 NOT_RUN. 문서 자동 검사는 [DOC-002](../policies/documentation-policy.md#push-gate)에 따라 실행하지 않음. commit/push·배포 미수행 |

설계 지적을 수정하거나 독립 검토로 채택해도 D5-F01–F05 동결·Q1/C1/B3 인수·PF11 성능 검증이 완료되지는 않는다. 최초 조사와 동시 002의 기록은 당시 범위로 보존한다.

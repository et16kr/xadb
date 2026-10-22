# DEV5 작업 분배 재검토와 개선 근거

2026-10-22 · **D5-WORK-BREAKDOWN-002 D2 ACCEPTED / 문서 개선 / 제품·B3·PF11 NOT_RUN**

[현재16개 분류·64개 작업](../plans/dev5-work-breakdown.md) · [공급 대장](../plans/dev5-work-breakdown.md#supply) · [이전44개 대응](../plans/dev5-work-breakdown.md#revision-map) · [서버 소유 계약](../contracts/dev5-server-design.md) · [기존 조사 원본](dev5-work-breakdown-research-20261022.md) · [로컬 소스 catalog](../reference/opensource-references.md)

사용자는 DEV5 잔여 작업의 분배·분량·우선순위·내용·분류를 ARIES·Oracle·로컬 DBMS와 비교 검토한 뒤 발견한 개선을 바로 반영하고 완료 후1회 commit하도록 요청했다. 먼저 읽기 전용으로 현재44개·소유 계약·선행/인수와 실제 소스를 대조했고, 후속 지시로 문서를 수정했다. 이 기록은 이전 조사나 구현 결과를 새 PASS로 바꾸지 않는다.

## 1. 판단과 반영

16개 책임 구분과 문장/거래·취소/정리·내부 결과/외부 ACK 분리는 유지한다. 명시된 기존44개 내부 ID 참조에는 없는 번호·순환이 없었지만, 그 사실은 외부 공급의 구현 가능성이나 균등 분량의 증거가 아니다. 하위 owner 연결, 공통 공급의 불필요한 완료 선행, 큰 검증 묶음의 배분을 보완한다.

| 검토 지점·반례/영향 | 반영한 배정 | 보존하는 계약·향후 인수 |
| --- | --- | --- |
| 5.08.2를 server scheduler만의 작업으로 보면 SQL stack/guard와 engine wait 소유가 공급되지 않은 채 많은 소비가 막힘. 고정4 worker 모두 waiter이면 holder가 실행될 수 없음 | D5-S02 engine 등록/DDL drain, D5-S03 SQL continuation, D5-S05 cleanup을 기존 공급 작업에 연결. server5.08.2 등록/깨움과5.08.4 재개를 분리 | SQL 재실행0·deadline 연장0·부분 pin/latch 잔류 금지·Send 검토. 실제 holder/DDL 진행5.08.3·M02/실제 thread는 미실행 |
| peer UID와 정책 파일 로딩은 다른 계층/실패 경계. 현재 ByteStream/Listener만으로 UID를 조회할 수 없음 | COMM-C0/C2의 D5-S01 공급·5.04.1 수신과5.04.4 policy load 분리 | OS accepted socket identity·조회 실패 거절·same-UID 범위, F02·T03/04 유지 |
| 일반 commit 결과 판정을 DDL 전체 완료 뒤에 두면 취소/fence/cleanup의 핵심 입력도 지연 | 5.10.4 공통 표현→5.10.3 일반 거래 장벽→5.10.2 DDL 두 outcome 합성. 실제 engine/SQL receipt는 D5-S05 수신 | durable+publish 판정 뒤 COMMITTED, 필요한 undo 완료 뒤 ABORTED, statement 성공·무변경 receipt 부재와 client UNKNOWN/engine fence 구분. T09/10/13·C02–07 |
| 송신 primitive를 fetch 뒤, client/fault 도구를 전체 Serving/shutdown 뒤에만 배정하면 실제 경계의 피드백이 늦음 | 5.11.2 literal/owned 전송과5.11.3 SQL 통합 분리. 5.15.2/3 도구→5.15.8 일반 cut·5.15.9 DDL→5.15.7 정식 client·5.15.10 종료 재장애 | 실제 Q1/C1/lease/recovery·오류/fence 공급 없는 SQL mock을 복구 증거로 사용하지 않음. 좁은 시험 조립과 정식 Serving/B3 gate 구분 |
| binding과 execute, 모든 timer, oracle·모델·stress/fuzz·복구·성능이 각각 한 행에 섞여 분량을 과소평가 | 44→64개. binding/execute, 공통/요청/idle/거래 timer, byte/SQL/history, M01/M02/thread/fuzz/반복 수명, PF11 도구/통계/진단을 분리 | 기존 T01–14/C01–08/M01–02/P01–04·known-bad·수치 분모 유지. 큰 실행의 미완료를 도구/작은 사례 PASS로 닫지 않음 |
| 기능·공급 조정·시험 도구·실제 실행·증거 집계를 동일 분량으로 계산하면 완료율이 왜곡 | 분류표에 작업 성격, 별도 분량/담당 기준, 우선 묶음A–E와 과거ID 대응표 추가 | 숫자·파일 수로 진행률 산정 금지. 공통 session/scheduler/cancel/cleanup owner 아래 순차 통합·별도 검토 |

고정 worker4/session32·메모리/시간 상한은 기존 개발 기준선이다. 코드 연구만으로 이 수치가 최적이라고 주장하지 않는다. 중단/재개의 실제 소유·진행 가능성을 먼저 확인하고 PF11에서 비용·원인을 측정한다. 전체 async runtime·동적 worker 확대·원격 인증·Transaction Guard/replay를 이번 개선으로 채택하지 않는다.

## 2. ARIES·Oracle 원문 열람

같은 채팅의 선행 검토에서2026-10-22 직접 열람했다. Oracle은 **19c 문서**이며 최신 제품/내부 구현을 조사했다는 뜻이 아니다. 원문은 아래 질문에 필요한 부분만 읽었고 DBMS 비교 실행은 하지 않았다.

| 출처·읽은 범위 | 관측·적용 |
| --- | --- |
| [Mohan 외 ARIES, ACM TODS17(1),1992 pp.94–162](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf), §5.2 pp.117–118·§6.3 pp.126–127·Fig.13 주변 | 부분 rollback 뒤 실행 계속, 보상을 기록한 CLR과 restart undo의 진행을 구분. 요청 취소·전체 cleanup·재장애 인수를 각각 배정하고 engine 알고리즘은 DEV3 소유로 유지 |
| [Oracle COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html), Purpose/Prerequisites/WAIT·NOWAIT | DDL 전후 commit과 durable log 기록 뒤 응답 유실을 명시. 일반 거래 결과 장벽·DDL 두 결과·외부 수신을 다른 완료 경계로 유지. XADB의 COMMIT 권한 재확인·durable ACK는 자체 계약 |
| [Oracle Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/transactions.html), Statement-Level Atomicity·Transaction Guard 개요 | 문장 성공과 거래 commit은 다르며 실패 문장은 이전 성공 작업을 지우지 않음. SQL/outcome oracle와 cleanup 인수를 구별. Transaction Guard는 별도 기능이므로 UNKNOWN을 추측/자동 replay로 해소하지 않음 |
| [Oracle JDBC Troubleshooting](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-troubleshooting.html), E.1.2/E.1.4 | statement/result/connection close 수명, cancel 신호와 원래 실행 결과 수신을 구분. 5.09.2/4와5.12·5.15.14 분해 근거이며 Oracle 내부 command replay를 XADB로 복제하지 않음 |
| [Oracle CREATE PROFILE](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/CREATE-PROFILE.html), resource enforcement·IDLE_TIME | call 제한과 session 수명 초과의 rollback 범위가 다르며 실행 중 query는 idle이 아님. 공통 timer·요청·idle·거래 종료를 분리. Oracle CONNECT_TIME을 XADB transaction300초와 동일시하지 않음 |

ARIES는 wire/scheduler 규격이 아니며 Oracle 의미의 비교는 동일 SQL/보안/성능 호환 주장과 다르다. 원래 XADB의 Active+SET AUTOCOMMIT 거절, COMMIT 열린 result 거절, CloseSession rollback과 두 DDL outcome을 유지한다.

## 3. 로컬 소스 identity와 실제 관측

같은 채팅의 선행 읽기 검토에서 아래5개 origin·HEAD·`git status --short`를 직접 확인했고 모두 clean이었다. 수정 단계에서는 아래9개 파일의 SHA-256을 확인했다. 소스 checkout 수정·fetch/pull·build/test·코드 복사는 없다. 총10개 catalog 중 이번에는 관련5개만 읽었다.

| 대상·경로 | origin·실제 revision |
| --- | --- |
| PostgreSQL `/home/et16/work/postgres` | `https://github.com/postgres/postgres.git` · `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` |
| MariaDB `/home/et16/work/mariadb` | `https://github.com/MariaDB/server.git` · `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` |
| CUBRID `/home/et16/work/cubrid` | `https://github.com/CUBRID/cubrid.git` · `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` |
| MySQL `/home/et16/work/mysql-server` | `https://github.com/mysql/mysql-server.git` · `3b99be40f8a31d9396cad10accfd3ba51647d613` |
| SQLite `/home/et16/work/sqlite` | `https://github.com/sqlite/sqlite.git` · `43a48f77585a88940c727f33682151418f544059` |

| checkout·파일·읽은 줄 | SHA-256 |
| --- | --- |
| PostgreSQL `src/backend/libpq/auth.c` 1880–1960 | `d3ab30f5024f21b3ba60959b760898f8d84ba0a90498c3cde8c24fbb8b6ea734` |
| PostgreSQL `src/backend/tcop/postgres.c` 3530–3665 | `86182ec057c0482d38b9df5d555d4a0a50b003d4051ff2a60914ad9ffa67720b` |
| PostgreSQL `src/backend/access/transam/xact.c` 1528–1603,2410–2465 | `b045e1093767124f3aaf3287eaef9c1b42d1470bf9cf1d576b10860bf0169cda` |
| MariaDB `sql/threadpool_generic.cc` 610–690,1270–1398 | `4f37bdd44fedd6c8c9284e81c44c5566fc6da87c3522a5688a3d05fb7e8f9632` |
| MariaDB `sql/sql_prepare.cc` 3880–3990 | `1c83c960cac5f4e6be24132045e1d9afd9eed4fd374b464c9245d70a7c670fbd` |
| CUBRID `src/communication/network_sr.c` 138–160 | `1a5b947c75628cb3a015a9c7afdee7715a092b22fc59cc9e1d3bde64d8161616` |
| CUBRID `src/communication/network_interface_sr.cpp` 3365–3460 | `755f3ea598daa9f77deeb14f7df690489fe3ca8cce07068307a9025871bc0f2f` |
| MySQL `sql/sql_prepare.cc` 1585–1665,1810–1915 | `d9e60c6f7d95b61f31513044c73ecdf3345539c8d7f8975a4a10e009959dbf52` |
| SQLite `src/vdbeapi.c` 90–160 | `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5` |

PostgreSQL의 peer 인증은 OS credential을 얻은 뒤 user map을 확인하며, interrupt 처리는 connection loss·query cancel·idle/transaction timeout을 나눈다. 동기 commit 경로는 `XLogFlush` 뒤 commit 상태를 반영하고 transaction 공개/후속 정리를 수행한다. XADB의 인증/권한·신호/정리·durable outcome 분리를 유지할 근거이며 PostgreSQL의 async commit은 채택하지 않는다.

MariaDB는 `wait_begin`의 active-thread 회계와 `wake_or_create_thread`, stall 검출을 사용한다. 따라서 이 코드를 XADB 고정4-worker continuation 구현의 완성/낮은 비용 근거로 사용할 수 없다. D5-S02/03의 실제 공급을 초기에 검토하도록 배정한다. prepared reset과 close도 실행/parameter/cursor/statement의 수명이 다름을 보여 준다.

CUBRID의 commit/abort request 속성이 다르고 handler는 내부 거래 상태를 얻어 pack한 뒤 송신한다. MySQL은 session statement map·parameter count 검사 뒤 parameter 설정/실행을 연결한다. SQLite finalize는 reset/delete, reset은 reset/rewind로 수명을 구분한다. 이 관측을 요청·결과·수명 배정에 사용하되 각 제품의 reset wire·raw pointer·추가 parameter 허용 의미를 가져오지 않는다.

## 4. 작업·검토 기록과 실행 범위

| 항목 | 기록 |
| --- | --- |
| 결정·담당 | [D5-WORK-BREAKDOWN-002](../plans/dev5-work-breakdown.md#decision-002); 작업/계약/통합 `/root` |
| 기준 | HEAD `9f79b31b99f558f4e9a98b8bd752220b851e7bbb`. 초기 README/catalog의 DEV6/7 순서 변경은 보존. baseline `/tmp/xadb-dev5-review-r2-mxi6unji` |
| 동시 작업 | 이후 DEV3·DEV4·DEV6·DEV7 계획/조사·공유 색인 변경과 별도 commit을 관측. 공유 파일의 DEV5 치환을 별도 보존하여 이번 commit에 다른 작업을 포함하지 않음. DEV3/DEV4/COMM에는 DEV5 공급 배정·역참조만 추가 |
| 독립 검토 | `/root/dev5_allocation_review`의 별도 읽기 검토·수정 재확인 완료, D2 채택 가능·잔여 필수 지적 없음. 원래44개/새64개·공급/선행·owner 계약·T/C/M/P 분모·Serving/B3·색인/JSON·로컬5개 origin/HEAD/clean 및9개 파일 SHA-256을 대조. `/root` 계약/통합 담당이 반영 확인 |
| 실행 | 원문·소스·현재 API·계획 읽기와 문서 수정만 수행. 제품/모델/복구/성능 테스트·fmt/Clippy·자동 문서 검사·push 미실행. 사용자 지시의1회 commit에는 DEV5 개선과 하위 공급 역참조만 선택 |

수정한 것은 작업 공급·배정·완료 경계다. 실제 signature/byte layout·자원 peak·모델 공간·실행 recipe 동결과 필요한 구현/검증은 여전히 미완료다. 기존 evidence의 source·실행 결과와44개 당시 조사/검토 identity를 보존한다.

독립 검토에서 발견한3건은 다음과 같이 해소했다. D5-S02의 key/UNIQUE 대기는3.07.2, offline build는3.07.3으로 바로잡았다. D5-S06은 platform의 독립 supervisor 공급과5.14.3의 종료 통합을 분리하여 자기 선행을 제거했다. PF11은 P01–03의 paired 비용/CI와 P04의5초 startup 제외60초·매1초 진행 인수를 구분했다. 추가로 outcome 표현이 모든 성공에 commit receipt를 요구하지 않도록 한정하고 실제 commit 소비의 선행 및 좁은 startup/fence의 조기 공급을 명시했다. 검토자는 자동 검사·제품 시험을 실행하지 않았으며 계획 채택을 실제 API 가능성·성능 최적성 인증으로 확대하지 않았다.

<a id="followup-003"></a>
## 5. 003 후속 검토 — 내부 인계·조기 실제 연결·분량

2026-10-22 · **D5-WORK-BREAKDOWN-003 D2 ACCEPTED / 문서 수정 / 제품·B3·PF11 NOT_RUN**

같은 사용자 요청으로 현재64개 작업을 다시 검토했다. 앞의002 당시 관측·검토 기록은 그대로 보존한다. **16개 책임과64개 ID는 적절하지만, 완료 조건까지 따라가면 fetch/close의 숨은 순환과 큰 통합 작업의 불필요한 선행이 남는다.** 따라서 숫자를 더 늘리거나 기능을 이동하는 대신7개 큰 항목에15개 내부 인계·완료 단위를 두었다. 독립 검토와 계약 대조에 기반한 계획 판단이며 구현 가능성·성능의 절대 최적성을 측정한 결과는 아니다.

| 발견·반례 | 수정한 분배·우선순위 | 유지하는 의무 |
| --- | --- | --- |
| 5.09.3은 EOF/재실행 인수를 포함하는데 fetch5.11.1이5.09.3 전체를 선행으로 요구. 완료 조건까지 해석하면 순환하거나 mock EOF로 닫게 됨 | 5.09.3a close/invalidation 공급→5.11.1 실제 fetch→5.09.3b EOF/close/reexecute | stale·old catalog·metadata·pin/plan/bind와10,000회 확대. 좁은 공급으로 부모 작업 전체 완료 금지 |
| 일반 DML/COMMIT terminal 송신5.11.3이 fetch 전체를 기다려 조기 C05/06가 literal reply에 머무를 수 있음 | 5.11.3a 일반 terminal과b fetch batch 분리;5.15.8에 실제a 공급 명시.5.15.9는 실제 DDL outcome 연결 추가 | durable outcome·partial send·CancelAck·외부 완결 ACK 구별, T/C 전체 분모 |
| C08의 cancel/FIN·CLR/recovery도 모두 shutdown5.14.3 뒤. 취소·cleanup의 선행이 다시 fetch/DDL/revoke/모든 timer 전체에 묶임 | 5.12.2a 일반/b result,5.13.2a close/FIN/b 확장 수명,5.15.10a 일반 재장애/b trigger·종료로 분리. 종료 subset만5.14.3 수신 | 같은 cleanup owner·미완료 undo 성공 위장 금지·fence·grace 실패·재복구. 확장 trigger도 해당 실제 공급 직후 인수 |
| 고정4-worker holder 진행이 DDL drain 통합까지 지연되고 서버 DDL adapter의 직접 선행도 불명확 | 5.08.3a 일반 COMMIT/ROLLBACK/close를 조기 인수. b는5.10.2 실제 DDL adapter와 D5-S02 drain 공급을 함께 수신 | timeout/worker 증설로 진행 결함 은폐 금지. 실제4-worker와 M02의 역할 구분 |
| 권한 통합5.05.2 전체가 DDL/handle을 기다림. 이를 cleanup 선행으로 쓰면 result/idle도 DDL 뒤로 돌아감 | a 일반 거래 revoke/b DDL/c prepared-result로 구분. cleanup은a/c 수신, DDL 권한 경로만b 추가 | Oracle와 다른 XADB COMMIT 권한 재확인·자기 cleanup 예외·prior/ddl 두 outcome 유지 |
| PF11 비교1셀30분만으로 전체 분량을 추정하면 workload/계측 조합·준비/분석·재현 비용이 빠짐 | F04/F05별 실행 분모·도구/fixture·실행·분석 대장. 기본14 workload에 비교2종을 택하면14시간이라는 조건부 산술 예시 | 기존 profile/수치·실행 전 동결·P04 구분. 예시로 새 조합 실행을 강제하거나 무변경 완료 시험을 반복하지 않음 |

[첫 실제 연결](../plans/dev5-work-breakdown.md#first-integration)은 인증된 direct DML·COMMIT/ROLLBACK의 socket 결과와 취소/FIN undo·holder 진행·관련 reopen까지 묶는다. 준비→작은 실제 연결→prepared/fetch와 DDL 확장→정식 lifecycle→잔여 확대/PF11 순서이며, SQL/engine/transport 공급과 정식 Serving/B3 gate는 그대로다. 공통 상태 담당과 독립 oracle 담당의 책임은 분리하되 새로운 production 상태 기계를 복제하지 않는다.

### 이번에 직접 다시 읽은 외부 원문

| 자료·버전·열람 부분 | 이번 판단에 사용한 내용·한계 |
| --- | --- |
| [ARIES 원 논문](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf),1992, §5.2·§6.3·Fig.13 주변 | 부분 rollback 이후 진행과 restart의 CLR/남은 undo를 구별한다. cleanup 재장애 검증을 서버 종료 구현과 독립 배정할 근거로 사용. ARIES 알고리즘·선택적 restart·wire protocol을 새로 도입하지 않음 |
| [Oracle19c COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html), Purpose·Prerequisites·WAIT | DDL 전후 commit·영속화 뒤 응답 유실을 대조하여 일반 terminal/DDL 두 결과/외부 ACK를 별도 인수. Oracle 일반 COMMIT의 권한 규칙과 XADB의 선택을 구별 |
| [Oracle19c Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/transactions.html), Statement-Level Atomicity·Transaction Guard 개요 | statement 성공/commit과 실패 문장의 rollback 범위를 대조. 첫 실제 연결에 S1 성공/S2 실패를 포함. 결과 조회·자동 replay는 별도 기능이므로 추가하지 않음 |
| [Oracle19c JDBC Troubleshooting](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-troubleshooting.html), E.1.2·E.1.4 | statement/result/connection 명시 정리와 cancel 신호·실제 실행 결과의 구분. close 공급·EOF 통합과 일반/result 취소를 나누는 근거. timeout을 rollback 완료 보장으로 해석하지 않음 |

### 이번 로컬 소스 범위와 identity

위3절의 다섯 checkout에서 origin·HEAD·작업 트리를 다시 확인했다. PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`, MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4`, CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5`, MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613`, SQLite `43a48f77585a88940c727f33682151418f544059`이며 위 origin과 일치하고 모두 clean이었다. 아래는 이번 직접 열람한5개 파일이며 이전9개 전부의 재열람으로 주장하지 않는다.

| checkout·파일·읽은 줄·관측 | SHA-256 |
| --- | --- |
| PostgreSQL `src/backend/tcop/postgres.c`3530–3675: connection loss·query cancel·idle/transaction timeout의 처리 분리 | `86182ec057c0482d38b9df5d555d4a0a50b003d4051ff2a60914ad9ffa67720b` |
| MariaDB `sql/threadpool_generic.cc`612–687,1275–1327: wait의 active-thread 회계와 wake/create·stall 대처 | `4f37bdd44fedd6c8c9284e81c44c5566fc6da87c3522a5688a3d05fb7e8f9632` |
| CUBRID `src/communication/network_interface_sr.cpp`3369–3442: 내부 commit/abort state를 받아 pack한 뒤 송신 | `755f3ea598daa9f77deeb14f7df690489fe3ca8cce07068307a9025871bc0f2f` |
| MySQL `sql/sql_prepare.cc`1810–1915,2000–2065: parameter 설정 뒤 execute, cursor/parameter reset과 statement deallocate 분리 | `d9e60c6f7d95b61f31513044c73ecdf3345539c8d7f8975a4a10e009959dbf52` |
| SQLite `src/vdbeapi.c`95–158: finalize의 reset/delete와 reset의 reset/rewind | `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5` |

MariaDB의 동적 worker 보충을 XADB 고정4-worker의 해법으로 복사하지 않고 실제 park/resume·holder 인수를 앞당겼다. CUBRID의 결과/송신 구분은 작은 일반 terminal 연결의 근거로, MySQL/SQLite의 수명 구분은 close primitive와 실제 EOF/재실행 인수 분리의 근거로 사용했다. PostgreSQL의 오류 이후 거래 의미·MySQL 무응답 close·SQLite pointer API는 XADB 계약에 이식하지 않는다. 다른5개 로컬 DBMS는 이번 DEV5 서버 인계 질문에 대한 직접 열람 대상이 아니며 전체10개 조사로 표현하지 않는다.

### 작업·독립 검토·실행 기록

| 항목 | 이번003 기록 |
| --- | --- |
| 권한·담당 | 사용자 DEV5 분배 검증·개선 즉시 반영·1회 commit 요청. 작업/계약/통합 `/root`; DP01에 따른 별도 AI 검토 `/root/dev5_dependency_review` |
| 기준·소유 | 수정 전 HEAD `49bb781098c073323a2fe634f527e637cddcfcd4`, 기준 사본·dirty identity `/tmp/xadb-dev5-review-r3-a_1upmzk`. DEV5 계획/상위 안내·이 후속 조사, 공유 README의 DEV5 문장·단계 JSON의003 참조만 변경 |
| 동시 작업 | DEV3 제품/fixture/검증 도구·DEV4/6/7 문서 및 공용 색인의 변경을 관측. 최신본의 좁은 DEV5 수정만 반영하며 이 작업의 commit에 무관한 변경을 포함하지 않음. 기존 문서 ID/경로는 유지하여 documents.json 등록 추가 없음 |
| 독립 검토 | `/root/dev5_dependency_review`가 최초64개·owner·직접/암묵 선행과 수정9개 파일의 DEV5 범위를 독립 대조. close/fetch 순환·일반 송신/취소/cleanup·holder 선행을 보완하고, 권한a/c·DDL만b 수신과 종료 경로만5.14.3 선행을 재확인.7개 부모/15개 내부 단위·F/T/C/M/P·Q1/C1/Serving/B3·조건부14시간 예산과 ARIES/Oracle 원문·로컬5개 지정 파일/hash를 직접 확인. 채택 가능·잔여 필수 P1/P2 없음; `/root` 계약/통합 담당 반영 확인 |
| 실행 상태 | 공식 원문·로컬 소스 읽기와 문서 수정. 제품/모델/복구/성능 시험·fmt/Clippy·문서 자동 검사·push는 실행하지 않음. 제품 변경이 없는 이 요청에 DEV3 동시 변경의 테스트를 섞지 않음 |

전체 시험 의무·미동결 F01–05·제품 미구현/B3 NOT_RUN은 유지한다. 계획 채택과 실제 구현 검증을 구별하며 기존 실행 결과·source identity를 소급 변경하지 않는다.

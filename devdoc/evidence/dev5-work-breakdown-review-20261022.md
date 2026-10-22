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

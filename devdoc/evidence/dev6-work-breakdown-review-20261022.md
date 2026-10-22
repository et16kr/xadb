# DEV6 작업 분배 재검토와 개선 근거

2026-10-22 · **원문·소스 재검토 / D6-WORK-BREAKDOWN-002 D2 ACCEPTED·독립 검토 완료 / 제품 구현·시험 NOT_RUN**

[개선 계획·결정](../plans/dev6-work-breakdown.md#decision-002) · [실행 예산](../plans/dev6-implementation-plan.md#execution-budget) · [의미 정본](../contracts/dev6-client-design.md) · [최초 분류 조사](dev6-work-breakdown-research-20261022.md)

사용자가 DEV6 분배·분량·우선순위·내용·분류의 적절성을 ARIES·Oracle·로컬 DBMS와 대조해 검증하도록 요청했고, 후속으로 수정·개선 뒤 한 번 commit하도록 요청했다. 이번 대화의 읽기 전용 재검토와 그 후 계획 수정의 근거를 기록한다. 최초 분류의 검토 결과를 새 변경의 승인이나 제품 실행으로 승계하지 않는다.

기준 HEAD는 `9f79b31b99f558f4e9a98b8bd752220b851e7bbb`다. 착수 시 `devdoc/README.md`와 `devdoc/documents.json`에는 DEV6/DEV7 항목의 순서 변경이 있었다. 해당 기존 diff의 SHA-256은 `dd6c3fe316d8c95b45ec71b3bee165532e81129f25ac764bf4c0941674949bbf`이며 별도 보존해 이번 내용 수정·commit 범위에서 제외한다. 작업·계약·통합 담당은 `/root`다.

<a id="findings"></a>
## 1. 관찰과 반영

| 재검토 관찰 | 선택·반영 | 유지할 경계 |
| --- | --- | --- |
| 접속·진행·결과 수명·거래·adapter·독립 검증을 나눈 상위 18개 책임은 타당 | 상위 분류와 기존 55개 하위 묶음 ID 유지 | 55개가 최종 API·실행 inventory 또는 동일 작업량이라는 해석 금지 |
| 6.13.3이 6.14 전체, 6.14.2가 parameter 입력 6.14.1을 기다리면 매개변수 없는 실제 조회도 지연 | 기본 실행/parameter 실행/정리/거래를 6.13.3.1–4로 분리. bound fetch는 descriptor·진단·result 수명·출력 변환을 직접 수신 | 실제 native/server 공급·ABI 검증 생략 없음. 정리 primitive와 후속 통합을 별도로 기록 |
| Java 진행·JDBC 거래/예외/abort·ODBC descriptor/진단·모델/fault 도구가 한 하위 항목에 여러 산출물을 포함 | 6.11.2·6.12.3·6.13.1/2/3·6.16.2/3·6.18.2에 추가 완료 단위 | API 전체 guard·cancel latch·deadline 불변식과 소비 경로별 통합 검증 유지 |
| 6.05.3의 진단 구현과 JDBC/ODBC 작업이 중복 집계될 수 있음 | 공통 의미/기대값·구현 주 담당·증거 집계 표 추가. 6.09.2 의미 공급도 Rust 구현 완료와 구분 | driver별 실제 구현/검증은 별도이며 한 driver PASS를 다른 구현에 승계하지 않음 |
| 준비 작업을 조기에 하도록 되어 있으나 작은 실제 연결 경로와 장애 경계의 우선순위가 구체적이지 않음 | 계약/환경/수명 → 접속/SELECT/fetch/close → 거래/유실/복구 → 지원표 확대 → 남은 확대 검증의 공급 순서 | 새 gate 아님. 실제 B3/Q1/S1/C1 및 D6-W01 진입과 SC04→05→06 유지 |
| 성능 cell당 비용과 전체 조합 수가 6.18.2 한 항목에 가려짐 | driver/profile 실행·통계·재현을 나누고 F07의 적용 분모/시간/독점 자원 카드와 조건부 예산 예시 추가 | 20 paired runs·기존 guardrail·DP03 profile 변경 없음. 108시간은 가정 예시이며 측정·확정 일정 아님 |

이 선택은 일정·수명 경계와 소비 관계에 대한 공학적 판단이다. 실제 구현 기간·처리량·병렬 효율을 측정한 결과나 모든 가능한 분할 중 수학적 최적성 증명은 아니다. 작업 카드의 예상 범위·불확실성과 실제 관측을 통해 재평가한다.

<a id="sources"></a>
## 2. 이번 대화에서 직접 읽은 공식 원문

| 자료·범위 | 관찰·XADB 판단 |
| --- | --- |
| [ARIES 원문](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf), stable log 설명(인쇄96–97쪽), CLR/rollback 개요(109쪽)·UndoNxtLSN 설명 | WAL과 CLR로 내구성·rollback 진행을 다룬다. engine 알고리즘을 DEV6에 재배정하지 않고 외부 관측·driver outcome·실제 reopen 검증을 별도 책임으로 유지한다. 네트워크 ACK 유실의 자동 retry 안전성을 이 논문만으로 추론하지 않음 |
| [Oracle19c JDBC Coding Tips](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-coding-tips.html), C.1/C.2.1 | 연결 사용·취소·결과 소비와 완료를 구분한다. Java 진행/결과/거래의 분리와 통합 검증 유지. Oracle 기능 전체를 초기 지원 범위에 추가하지 않음 |
| [Java SE21 Connection](https://docs.oracle.com/en/java/javase/21/docs/api/java.sql/java/sql/Connection.html), setAutoCommit/commit/rollback/close | 실제 모드 변경의 commit과 같은 값 no-op, 자동 commit 모드의 거래 API 거절, active close의 구현별 의미를 구별한다. 현재 XADB 명시 조립·부분 outcome을 보존하고 driver 구현 담당만 명확화 |
| [Oracle19c Transaction Guard](https://docs.oracle.com/en/database/oracle/oracle-database/19/adfns/transaction-guard.html), 29.1–29.3.2 | 응답 유실만으로 commit 여부를 알 수 없고, Oracle의 안전한 재제출은 LTXID 영속화·outcome 확인·진행 중 동일 거래 commit 차단을 포함한다. XADB에 그 기능이 있는 것처럼 자동 재전송하지 않으며 기존 Unknown/no-replay 유지 |
| [ODBC SQLGetData](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlgetdata-function?view=sql-server-ver17), Arguments·Diagnostics·Retrieving Data | 고정형/가변형 buffer·NULL·column 순서·부분 소비는 별도 상태다. 출력 변환/부분 조회를 입력 parameter 완료와 묶지 않고 F04와 R2-V06의 필수 의미 유지 |

자료의 설명과 오른쪽 XADB 배정 판단을 구분한다. 이번 변경은 wire/SQL/표준 준수 범위를 확대하지 않는다.

<a id="local"></a>
## 3. 로컬 소스 재열람 identity

읽기 전용 재검토에서 다음 여섯 checkout의 origin·실제 HEAD·작업 트리를 확인했으며 모두 clean이었다. 원격 갱신·build·제품 실행은 하지 않았다. 카탈로그의 모든 DBMS를 분석했다고 주장하지 않는다.

| checkout·실제 revision | 읽은 범위 | 배정 판단 |
| --- | --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/interfaces/libpq/fe-exec.c` PQsendQueryStart 1690–1752·PQgetResult 2079–2185, `fe-cancel.c` PQcancelStart/Poll 198–290·PQcancel 설명528–546 | admission·I/O 진행·결과 완료·취소 송신은 구별된다. 취소 송신 성공으로 backend 효과를 확정하지 않는다는 주석도 확인. 6.04/08 분리·Java 독립 진행 구현량 유지 |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `sql-common/client.c` mysql_free_result 749–775·mysql_close_slow_part/mysql_close 3468–3507 | 결과 drain/해제와 blocking 가능 close를 구별. 수명 primitive·실제 종료 통합을 나누며 XADB Drop 계약 유지 |
| MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613` | `libmysql/libmysql.cc` stmt_fetch_row/cli_unbuffered_fetch/mysql_stmt_fetch 3908–4005 | row·NULL·buffer 복사·truncation·fetch 상태를 구별. ODBC 입력과 출력 작업 분리의 비교 근거이며 ODBC 규범 oracle로 사용하지 않음 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/broker/cas_handle.c` hm_new_srv_handle/hm_find_srv_handle/hm_srv_handle_free 58–178 | slot/handle 상한·내용 해제·slot 반환. XADB generation/ODBC pointer stale 안전성은 별도 F03/F04 증명 의무 |
| SQLite `43a48f77585a88940c727f33682151418f544059` | `src/vdbeapi.c` sqlite3_finalize/reset/clear_bindings 105–187 | statement 파괴·실행 reset·binding 초기화는 다르다. ODBC CLOSE/UNBIND/RESET_PARAMS·result 수명 분리 유지 |
| ClickHouse `1d76716b7580f78d0375c626736e49a3e13f099c` | `src/Client/Connection.cpp` cancel/reset/disconnect 479–519 | socket 종료와 local 상태 해제를 구별. client 폐기를 rollback 완료로 승격하지 않음 |

여섯 주 파일의 전체 bytes SHA-256을 다시 계산했으며 [최초 조사 표](dev6-work-breakdown-research-20261022.md#local)의 값과 일치했다. 추가로 읽은 PostgreSQL `src/interfaces/libpq/fe-cancel.c`의 SHA-256은 `8a6612e840ec8ada603619737501ec5e589cd2b989981a684fced8ac325deacc`다. 전체 파일 hash는 읽은 부분의 범위를 확대하거나 XADB 실행 source identity를 대신하지 않는다.

<a id="review"></a>
## 4. 수정 범위·별도 검토·미실행 상태

수정 범위는 DEV6 작업 분류·구현 계획, 설계의 연결 문구, 이 재검토 기록과 README/문서 catalog다. 단계 JSON은 기존 DEV6 계획 ID를 이미 참조하며 gate·의미·실행 상태 변경이 없어 유지한다. 기존 조사·실행 증거와 착수 전 항목 순서 변경을 보존한다. 작업 중 들어온 DEV4/5/7 변경과 공유 색인의 해당 변경도 이번 검토·commit 범위에서 제외한다.

별도 실행 `/root/dev6_allocation_review`가 기준 HEAD·착수 사본 대비 DEV6 diff, 해당 의미 정본, 새 근거 기록과 공유 색인의 DEV6 변경을 읽기 전용 검토했다. ODBC 수명 기반을 조회 전에 공급하는 직접 선행·parameter 입력과 출력 분리, Java/JDBC 진단·진행 기반의 선공급, 공통 의미/구현 주 담당/증거 집계, 1시간·108시간 계산의 가정과 무변경 시험 제한을 확인했다. 자기 검토 후 추가한 Java 등록의 좁은 선행·driver별 거래 생산자·ODBC close 기반/BindAll 공급·적용 fault 도구의 5행 보완도 최신 문구로 확인했다.

검토자는 여섯 로컬 checkout의 origin·HEAD·clean 상태와 관련 7개 파일 SHA-256 일치를 확인하고 관련 함수 일부·ARIES·Oracle Transaction Guard·JDBC Connection·SQLGetData의 근거와 적용 제한을 대조했다. 결론은 **필수 수정 P1/P2 없음·정적 D2 채택 가능**이다. `/root`가 계약·통합 담당으로 반영을 확인했다. 최초 분류 검토와 이번 독립 검토는 별개다.

이 검토는 아직 동결하지 않은 F01–F07의 실제 API/ABI/case inventory, 구현 안전성·실행 시간·제품 인수의 검증이 아니다. 검토자는 파일 수정·제품 시험·문서 자동 검사·commit/push를 수행하지 않았고 동시 타 DEV 변경과 기존 항목 순서 변경은 제외했다.

제품 코드·build 등록·dependency·API/wire 형식 변경은 없다. 제품 시험·모델·성능·복구·DBMS 실행과 문서 자동 검사는 NOT_RUN이다. 변경 코드가 없으므로 제품 시험을 실행하지 않으며 문서 자동 검사는 push 직전 정책을 따른다. 이 요청은 local commit까지이며 제품 개발 재개·push·SC/CLIENT1 승격을 포함하지 않는다.

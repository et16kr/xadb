# DEV5 — 서버 소스 조사와 독립 검토 기록

2026-10-18 · **실제 소스 열람 기록 / 외부 코드 도입·제품 실행·B3 검증 아님**

[서버 설계](../contracts/dev5-server-design.md) · [구현·검증 계획](../plans/dev5-implementation-plan.md) · [로컬 source catalog](../reference/opensource-references.md)

## 조사 범위와 identity

서버 request/transaction 응답, prepared/bind/close 수명, 취소의 결과 관측을 비교했다. 아래 checkout의 origin·HEAD·`git status --short`를 실제 확인했으며 세 곳 모두 출력 변경이 없었다. 빌드·제품 시험·전체 보안 감사·성능 비교는 하지 않았다. source checkout에 쓰거나 Git 이력을 가져오지 않았으며 코드를 XADB에 복사하지 않았다.

| 대상·로컬 경로 | origin·실제 revision |
| --- | --- |
| PostgreSQL `/home/et16/work/postgres` | `https://github.com/postgres/postgres.git`, `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` |
| MariaDB `/home/et16/work/mariadb` | `https://github.com/MariaDB/server.git`, `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` |
| CUBRID `/home/et16/work/cubrid` | `https://github.com/CUBRID/cubrid.git`, `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` |

| 파일·실제 읽은 줄 | SHA-256 |
| --- | --- |
| PostgreSQL `src/backend/tcop/postgres.c`: 1600–1660, 1720–1825, 2190–2235, 5000–5160 | `86182ec057c0482d38b9df5d555d4a0a50b003d4051ff2a60914ad9ffa67720b` |
| MariaDB `sql/sql_prepare.cc`: 2670–2735, 3396–3434, 3890–3985 | `1c83c960cac5f4e6be24132045e1d9afd9eed4fd374b464c9245d70a7c670fbd` |
| CUBRID `src/communication/network_sr.c`: 138–158 및 request attribute symbol 검색 | `1a5b947c75628cb3a015a9c7afdee7715a092b22fc59cc9e1d3bde64d8161616` |
| CUBRID `src/communication/network_interface_sr.cpp`: 3369–3462, 9636–9664, 9805–9858 | `755f3ea598daa9f77deeb14f7df690489fe3ca8cce07068307a9025871bc0f2f` |

줄 범위는 해당 revision에서의 열람 위치이고 현재 XADB 소스 검증 지문이 아니다. 초기 탐색에서 존재하지 않는 `.c` 경로를 조회한 뒤 실제 `.cpp` 파일을 찾아 읽었으며, 존재하지 않는 경로는 조사 근거에 포함하지 않았다.

## 관측과 XADB 선택

| 질문 | 실제 관측 | XADB 선택·채택하지 않은 부분 | 필요한 검증 |
| --- | --- | --- | --- |
| prepared/bind와 실행 상태를 어떻게 나누는가 | PostgreSQL bind는 parameter format/count를 검사하고 portal 소유 메모리/plan 수명을 다루며 Close가 statement/portal을 구분한다. Sync는 command transaction 종료와 Ready 응답에 관여한다 | DEV4의 typed owned binding·statement/result 분리는 유지. PostgreSQL portal/Sync·pipeline 거래 의미는 도입하지 않고 요청별 terminal 한 개를 사용 | T01/05/06/07 |
| stale/close·reset의 관측은 같은가 | MariaDB prepare는 THD statement map에 등록하고 실패 시 제거한다. execute는 최소 packet 길이를 검사한다. reset은 cursor/parameter 정리·OK, COM_STMT_CLOSE는 직접 응답을 보내지 않는다 | 세션 소유·길이 검사의 필요성을 참고. XADB close는 DEV4의 generation 검증·명시 결과를 유지하고 MariaDB reset·무응답 close를 그대로 복제하지 않음 | T02/05/06/07 |
| transaction 결과와 전송 호출은 어느 계층인가 | CUBRID commit/abort handler가 내부 transaction state를 받은 후 reply에 pack하고 send한다. request table에는 transaction/검사 attributes가 있고 session 종료와 prepared 정보 요청이 분리된다 | engine outcome을 먼저 확정하고 server가 전달. send 호출 자체를 외부 ACK로 세지 않고 DDL의 두 outcome·RecoveryRequired까지 보존 | T09/10/12/13 |
| cancel 성공을 어떻게 확인하는가 | PostgreSQL 공식 설명은 별도 연결의 secret 기반 CancelRequest와 원래 query 응답으로 결과를 확인하는 방식을 구분한다 | 취소 신호와 SQL terminal 결과 구분을 채택. XADB는 같은 인증 연결·정확한 request generation·bounded I/O owner로 신호를 전달. 별도 cancel 연결/secret 체계는 도입하지 않음 | T08/09, M01/02 |

PostgreSQL 공식 [18 Message Flow §54.2.8–9](https://www.postgresql.org/docs/18/protocol-flow.html#PROTOCOL-FLOW-CANCELING-REQUESTS)은 취소 신호가 이미 끝난 요청에 효과가 없을 수 있고 원래 query 응답을 기다려야 한다고 설명한다. 연결 유실 전에 서버가 이미 commit했을 가능성도 구분한다. [18 Message Formats](https://www.postgresql.org/docs/18/protocol-message-formats.html)는 protocol 바이트 책임의 참고 진입점으로 확인했다. 2026-10-18 열람이며 local master source와 문서18의 버전을 동일하다고 주장하지 않는다. XSQ1의 구체 header/자원 수치는 XADB의 선택이지 이 문서에서 가져온 호환 형식이 아니다.

## XADB 계약 대조

DEV5는 다음 정본을 읽고 소비했다.

- [DEV4 prepared·metadata·result 수명](../contracts/dev4-sql-design.md#prepared), [DDL 두 outcome·권한 위치](../contracts/dev4-sql-design.md#ddl-boundary), [SQL 오류·거래](../contracts/dev4-sql-design.md#transactions).
- [COMM AXF1](../contracts/comm-design.md#frame), [quota](../contracts/comm-design.md#limits), [deadline/drain](../contracts/comm-design.md#lifecycle), [local flush와 업무 결과](../contracts/comm-design.md#outcome).
- [커널 실제 ACK 원칙](../contracts/implementation-contracts.md#section-6), DEV5의 SC03/KU14/PA08/PF11, [버전 정책](../policies/version-compatibility-policy.json).

특히 AXF1 payload1 MiB보다 SQL1 MiB+envelope가 크다는 사실 때문에 bounded logical-body 분할을 선택했다. 큰 bind는 reassembly quota와 SQL prepared quota 모두 검사하고 실패한 bind의 기존 값은 보존한다. COMM begin_drain 이후에는 새 reply enqueue가 불가능하므로 서버가 cleanup/reply를 마친 뒤 transport drain을 호출한다. DDL 권한 거절 전 prior transaction commit, slow peer의 result pin, engine UNKNOWN과 client 응답 유실 UNKNOWN을 별도 경계로 남겼다.

<a id="review"></a>
## 독립 검토 기록

D2의 별도 읽기 전용 검토자는 `/root/dev5_design_review`, 작업·계약·통합 담당은 `/root`다. 최초 검토는 DEV4·COMM·단계 원문에서 시작했으며 실제 ACK, DDL prior commit, 취소/RecoveryRequired, handle 세대/권한, slow peer 자원, 1 MiB envelope와 큰 bind 경계를 지적했다. 이를 설계·인수 행렬에 반영했다.

최종 검토에서 Reply(N)이 CancelAck(N)보다 먼저 도착한 뒤 client가 N+1/Cancel(N+1)을 보내면 이전 Ack slot 때문에 정상 흐름이 Protocol close로 판정될 수 있는 경합을 확인했다. client는 자신이 보낸 CancelAck와 terminal을 모두 수신한 뒤 다음 요청을 보내도록 했고, Reply fragment 사이 Ack 삽입을 금지했다. I/O owner는 RX 분류 전 완료 TX ticket을 회수하여 이미 전달된 Ack의 로컬 정리 지연이 정상 다음 취소를 막지 않게 했다. D5-T05/T09에 해당 정상 trace와 늦은 Cancel(N)의 교차 반례를 추가했다.

자기 검토로는 peer credential 실패의 거절, timeout 공통 prefix, 정책 교체 시 입력/임시 allocation/공존 snapshot의 512 KiB pool, control2 MiB와 그 안의 bounded diagnostic ring을 보완했다. 이들은 독립 검토의 실제 재검토 대상에도 포함했다.

2026-10-18 `/root/dev5_design_review` 최종 재검토 결과는 **설계 채택 가능·잔여 필수 P1/P2 없음**이다. 새 문서3개와 기준 사본 대비 공유 문서12개의 DEV5 변경, DEV4/COMM 원문을 읽었고 source origin/HEAD·위4개 파일 hash·baseline identity hash 일치도 확인했다. `/root`가 계약/통합 담당으로 지적 반영을 확인하여 DEV5-DESIGN-001을 ACCEPTED로 기록했다. 동시 COMM 문서 변경과 기존 제품 dirty 변경은 검토 승인 범위에서 제외했다.

검토자는 읽기 전용으로 작업했고 파일 수정·문서 자동 검사·제품 실행을 하지 않았다. D5-F01–F05·B3·PF11·모델/동시성/보안/복구 실행은 여전히 미완료/NOT_RUN이다. 설계 검토와 source 열람은 제품 실행 PASS를 대신하지 않으며 기존 실행 증거는 변경하지 않는다.

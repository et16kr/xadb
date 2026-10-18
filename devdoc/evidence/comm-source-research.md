# COMM 소스 조사와 설계 검토 기록

2026-10-18 · **선택 소스·공식 문서 열람 / 코드 도입·DBMS 실행·C1 검증 아님**

[로컬 소스 catalog](../reference/opensource-references.md) · [COMM 설계](../contracts/comm-design.md) · [구현·검증 계획](../plans/comm-implementation-plan.md)

## 조사 질문과 실제 revision

조사자는 `/root`다. 길이 검증 시점, 부분 송수신과 queue, 오류 뒤 stream 재사용, transport와 업무/취소 의미의 경계를 조사했다. 세 checkout에서 origin·HEAD·`git status --short`를 읽었으며 출력상 변경은 없었다. 소스 checkout을 수정하거나 build/test하지 않았다. 아래 SHA-256은 읽은 구간을 포함하는 파일 전체의 hash다.

| 대상·local checkout | 실제 HEAD | origin |
| --- | --- | --- |
| PostgreSQL · `/home/et16/work/postgres` | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | [postgres/postgres](https://github.com/postgres/postgres) |
| MariaDB · `/home/et16/work/mariadb` | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | [MariaDB/server](https://github.com/MariaDB/server) |
| CUBRID · `/home/et16/work/cubrid` | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | [CUBRID/cubrid](https://github.com/CUBRID/cubrid) |

| 파일·열람 구간 | 파일 SHA-256 |
| --- | --- |
| PostgreSQL `src/backend/libpq/pqcomm.c` · 1200–1260, 1380–1455 | `d6de729509a2876d6a1c2aacbe9e4fcb17b1420de684f46242b4848a4ecc40cd` |
| MariaDB `sql/net_serv.cc` · 200–247, 814–914, 1080–1115 | `7d7b8b8a8de1834099f350912ed9255a1f376565da407e8dc461f97c72f12c93` |
| CUBRID `src/connection/connection_support.cpp` · 626–723 | `a2248046cab91c0ab7d588658e077a3f5ae55353698fa5c1253673ebae00183a` |
| CUBRID `src/connection/connection_cl.cpp` · 405–459 | `13e93ca0a3cb13e0d38065d28aed055a8f8be4e532909d450a75160117fbd5e5` |

XADB 기준 revision은 `b8b1e60a314d397198e2f2c41205f9e468a6decf`다. 착수 당시 다른 작업의 `devdoc/implementation/dev2-implementation.md` 수정과 `crates/engine/tests/stress.rs`, `scripts/check_dev2_extended.py`, `scripts/test_check_dev2_extended.py` untracked 파일이 있었고 이번 설계에서 변경하지 않았다. [현재 frame codec](../../crates/foundation/src/codec.rs)의 열람 시 SHA-256은 `c659309e96097d8cb2bf4a2a95f77da049dc8eb5b7d7f314bd137ab9278e2e29`다. AXF1 10-byte header·32-byte SHA-256·1 MiB payload 상한과 완결 decoder를 확인했다. foundation runtime의 quota/cancellation/lifecycle은 socket 취소나 stream decoder 구현이 아니다.

## 관측과 XADB의 선택

| 읽은 원문 | 관측한 범위 | XADB 적용·차이 |
| --- | --- | --- |
| [PostgreSQL pqcomm.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/libpq/pqcomm.c) | `pq_getmessage`는 길이 범위 확인 뒤 buffer를 늘리며 body 오류 경로가 있다. nonblocking flush는 일부 진행과 EAGAIN을 구분한다 | header 검증 선행·TX cursor 보존을 채택. PostgreSQL의 OOM 뒤 body discard 정책은 가져오지 않고 XADB는 해당 연결을 닫음 |
| [MariaDB net_serv.cc](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql/net_serv.cc) | `net_realloc`의 max_packet_size 확인, 부분 read loop·packet 번호 확인, 큰 packet 연결 처리 | 길이·상한·부분 I/O를 함께 시험. MariaDB packet layout/번호/압축/대형 message 재조립은 채택하지 않음 |
| [CUBRID connection_support.cpp](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/connection/connection_support.cpp), [connection_cl.cpp](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/connection/connection_cl.cpp) | 길이와 수신 크기 비교·초과분 처리·timeout, header의 transaction/flag 처리와 close가 연결됨 | byte transport와 SQL/transaction metadata를 별도 owner로 분리. 초과 frame drain 및 transaction ID wire 형식을 그대로 가져오지 않음 |

2026-10-18에 PostgreSQL 18 공식 [Message Formats](https://www.postgresql.org/docs/18/protocol-message-formats.html)와 [Message Flow](https://www.postgresql.org/docs/18/protocol-flow.html)를 열람했다. 길이 있는 message와 별도 cancel request, query completion·연결 종료의 구분을 비교 자료로 사용했다. XADB가 PostgreSQL protocol을 구현하거나 그 SQL·인증·취소 의미와 같다고 주장하지 않는다. 위 local HEAD와 문서 버전 18은 서로 다른 연구 입력이다.

1 MiB 프레임과 SHA-256은 XADB의 기존 DEV1 계약에서 가져왔다. 128 연결·queue/시간·poll 선택은 XADB의 초기 공학적 결정이며 위 DBMS의 성능에서 도출한 수치가 아니다. socket local flush가 SQL 성공·durability를 보장하지 않는 경계를 C-V15 및 DEV5 이관 의무로 둔다.

<a id="review"></a>
## 독립 검토와 실행 상태

2026-10-18에 작성자와 별도 AI 실행인 `/root/comm_design_review`가 읽기 전용으로 검토했다. 적용 정책, COMM 설계·구현 계획·조사 기록, 상위 C1/KU12·의존 경계, foundation codec/runtime, 관련 JSON·문서 색인과 공개 소스의 주요 인용 구간·실제 revision을 직접 대조했다. 작성자의 결론만 전달받은 승인이 아니다.

| 지적 | 반영 내용·재검토 |
| --- | --- |
| producer mailbox와 owner enqueue 중 승인점이 불명확하여 drain/cancel 경합 결과가 달라짐 | 일반 mailbox를 제거하고 preallocated TX admission queue의 mutex 아래 enqueue를 승인점으로 고정. drain/cancel 차단과 같은 gate 사용, accepted ticket의 deadline·drain 포함을 명시. 해소 확인 |
| ticket 결과·Closed 연결 metadata의 무제한 잔존 가능성 | terminal 결과 1회, 미회수 결과의 TX slot 유지, 외부 lease/sender/ticket/준비 예약이 남은 Closed 연결의 connection slot 보존. 해소 확인 |
| 6 events×1/2-byte progress는 최소 42-byte frame을 완성하지 못함 | 깊이 8의 경계 progress를 실제 byte I/O에 대응, 완결 RX/TX·두 frame·두 연결·lease/ticket 반환·partial cancel·quota 거절의 필수 도달 witness 추가. 해소 확인 |
| 취소 뒤 모든 syscall 금지와 cleanup, 1초 close와 5초 drain의 충돌 | 신규 connect/accept/read/write와 필수 close/unregister를 구분. abort/cancel과 graceful drain의 별도 시간 판정. 해소 확인 |
| advance의 무조건 nonblocking 표현과 admission mutex 경합, NET 잔여 범위 복사 문구 | blocking I/O/공간 대기 금지와 짧은 mutex 경합 허용을 구분하고 NET-01/02/06의 후속 의무를 각각 기록. 해소 확인 |

최종 검토 결과는 **기본 설계 채택 가능, 잔여 blocker 없음**이다. `/root`가 계약·통합 담당으로 반영하여 COMM-DESIGN-001을 ACCEPTED로 바꿨다. COMM-C0/C4의 실제 API·dependency·runner·native fault/seed/profile 구체화와 구현의 독립 검토는 후속 의무다. 검토자는 파일을 수정하거나 자동 검사·제품 시험을 실행하지 않았다.

이번 작업은 소스 조사·설계 작성·독립 의미 검토만 수행했다. 문서 자동 검사는 DOC-002에 따라 push 직전으로 남겼으며 push하지 않았다. 제품 시험·공식 배포·C1/KU12/C-V01–18은 NOT_RUN이다. 이 기록은 설계 채택과 검토의 증거이며 제품 실행 PASS가 아니다.

<a id="review-corrections"></a>
## 후속 설계 리뷰 — COMM-DESIGN-002

2026-10-18 사용자가 COMM 설계 리뷰와 발견 문제의 즉시 수정을 요청했다. 위 최초 리뷰·조사 identity는 당시 기록으로 보존한다. 이번 작업·계약·통합 담당은 `/root`, 별도 AI 검토자는 `/root/comm_review`다. 문서에 표현된 API·종료 의미를 구체화하므로 DP01 D2로 분류했다. 제품 코드와 실행 profile/runner는 추가하지 않는다.

### 기준과 수정 소유

기준 commit은 `055f744a33ac33a34fada89cba2660588f07fb4c`다. 수정 전 `git diff --binary HEAD`의 SHA-256은 `7d796843442e1f50812ce9ddab0ef982708c3692e575cc262789d53468124bbb`이며, 아래 순서의 `[path, file_sha256]` 배열을 UTF-8 JSON(`ensure_ascii=False`, separators `,`/`:`)으로 직렬화한 dirty manifest의 SHA-256은 `72734eb28e80c02bd884a9d98299c5a6ec83632ed7421392859dacad101ad47c`다. 아래 다른 작업의 파일은 수정 대상에서 제외했다.

| 기존 변경 파일 | 착수 시 SHA-256 |
| --- | --- |
| `devdoc/implementation/dev2-implementation.md` | `f9e3dff3a64a97598322b755203b28ada9e6bfed6c7ffef2fdc4a77a60df8b0a` |
| `crates/engine/tests/stress.rs` | `a5a31010ade42cec4c59ba8a08ac692a4ec8fc20732f7738e5e357029663c29f` |
| `crates/testing/protocol/tests/fuzz.rs` | `83bf8fa120a176f083c2abdb1102b797de31bad6f4953b6b5388bf5d0fd5b5bb` |
| `scripts/check_dev2_extended.py` | `d5c156664b7713b63f0262173932a87de862ff38d8ed59827014f2372f624332` |
| `scripts/test_check_dev2_extended.py` | `76042b8a82d9a45499f7b6df337b59a82c05899eb1a3acf7e68b067873a975c0` |

소유 경로는 COMM 설계·구현 계획·이 조사 기록이며, 공유 수정은 `kernel-test-matrix.json`의 KU12, `stage-completion.json`의 COMM 인수 문구와 contracts/plans/evidence의 README 항목이다. `/root`만 순차 편집하고 검토자는 읽기 전용으로 진행했다. 기존 문서 ID·경로는 유지하므로 `documents.json`에 새 항목을 만들지 않는다. 반영 순서는 의미 계약 → 인수 반례/JSON·색인 → 독립 재검토다. 생성 산출물은 없고 문서 자동 검사는 push 시점으로 남긴다.

작업 도중 다른 작업의 DEV3/DEV4 계약·계획, 공통 구현 계약·문서 색인·evidence README와 DEV2 증거 변경이 추가된 것도 확인했다. COMM의 좁은 항목만 patch하고 이 변경들을 되돌리거나 이번 리뷰의 승인 범위에 포함하지 않았다.

### 반례와 수정

아래는 계약·소스·공식 문서 대조로 구성한 반례이며 XADB 제품에서 재현한 실행 결과가 아니다.

| 우선순위·반례 | 수정과 필수 인수 |
| --- | --- |
| P1 · pending connect에는 TX가 없고 Open 전 enqueue도 불가한데 writable을 TX가 있을 때만 감시하면 완료를 놓침 | Connecting의 writable/error interest를 TX와 분리하고 socket error 확인. C-V09·16 |
| P2 · RX slot 포화 중 HUP를 interest=0으로 계속 감시하면 즉시 반환이 반복되고, HUP를 EOF로 해석하면 buffered frame 유실 | 상태별 대기 목록, HUP 힌트 보관과 `read=0` 구분, non-actionable endpoint 제외·bounded 재시도, full listener pause. C-V09·11·16 |
| P2 · 송신자 clock을 driver보다 10초 늦게 만들면 30초 TX deadline을 driver epoch에서 20초로 판단 가능; 순회 시작 시각 재사용도 늦은 완료를 성공으로 보임 | 동일 Clock instance 주입과 효과 전후 fresh 시각, 늦은 TX byte 진행 보존·RX 미공개. C-V10·16 |
| P2 · RX 시작 t=0, 새 TX enqueue와 drain t=29.9 s, TX 완료 t=31 s에서 잔존 RX timer가 정상 drain을 t=30 s에 중단 | partial RX·예약 반환과 RX/idle timer 해제, 검증 RX 유지, TX/drain deadline만 적용. C-V13 |
| P2 · accepted TX 뒤 owner를 drop하고 ticket만 보유하면 close 전용 결과 정리 규칙으로는 Pending이 영구 잔존 가능 | owner guard Drop도 admission 차단·close·ticket 1회 실패 확정. 실제 prefix 보존, 외부 lease/ledger 수명 유지, terminal 회수는 ticket 소비. C-V06·13·16 |
| P2 · IPv4 listener가 매 순회 accept 8회를 먼저 쓰면 IPv6/Unix가 연결 timeout까지 선택되지 않을 수 있음 | driver 전체 8회 예산 안에서 ready listener별 1 attempt round-robin, 시작 위치 유지. C-V13·scripted 공정성 |

`/root/comm_review`는 원래 계약, C1/KU12, 실제 foundation codec/runtime 및 platform clock을 직접 읽었다. 독립 검토에서 Drop 결과 정리·clock epoch·partial RX drain을 지적했고 listener 간 starvation도 추가했다. `/root`의 Connecting/hangup 반례도 대조했다. 이는 같은 모델의 별도 실행이며 오류가 통계적으로 독립이라는 주장은 하지 않는다.

### 추가 근거와 검증 범위

현재 [foundation Clock](../../crates/foundation/src/runtime.rs)과 [platform MonotonicClock](../../crates/platform/src/runtime.rs)은 서로 다른 instance의 epoch를 비교할 수 없다고 명시한다. 기존 cooperative cancellation은 token 확인과 다음 효과 사이의 원자성을 보장하지 않는다. 이번 설계는 새 clock이나 강제 syscall 중단을 구현했다고 주장하지 않고 조립과 관측 규칙을 고정한다.

2026-10-18에 Linux man-pages 6.19의 [connect(2)](https://man7.org/linux/man-pages/man2/connect.2.html)와 [poll(2)](https://man7.org/linux/man-pages/man2/poll.2.html)를 열람했다. 전자는 nonblocking connect의 writable 대기와 SO_ERROR 확인을 설명하고, 후자는 interest=0에서도 HUP/ERR/NVAL이 반환되며 hangup 이후에도 unread bytes가 남을 수 있음을 명시한다. XADB의 상태별 interest·대기 목록 제외는 이 API 의미에서 도출한 설계 수정이다. 다른 DBMS checkout을 다시 조사·수정하거나 실행하지 않았다.

`/root/comm_review`가 수정본을 독립 재검토하여 **잔여 차단 결함 없음, COMM-DESIGN-002 채택 가능**으로 판정했다. 6개 반례와 C-V 항목의 대응, Closed 연결의 미회수 RX/외부 owner에 대한 slot 유지, 20 ms 재시도 시각, KU12·COMM JSON과 README 연결을 확인했다. `/root`가 계약·통합 담당으로 반영하여 ACCEPTED로 바꿨다. 검토자는 파일 수정·자동 검사·제품 시험을 수행하지 않았다.

기존 변경 파일 hash 재대조에서 네 untracked 소스/스크립트는 착수 시와 같았고 DEV2 구현 문서는 동시 작업으로 추가 변경된 것을 확인했다. 이번 작업은 어느 파일도 수정하지 않았다. COMM 미구현으로 제품 시험·C1/KU12·C-V01–18은 계속 NOT_RUN이다. 문서 검사기는 DOC-002에 따라 실행하지 않았고 commit/push도 하지 않았다. 기존 과거 실행 증거와 `document-validation.json`을 갱신하지 않는다.

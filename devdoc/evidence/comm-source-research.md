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

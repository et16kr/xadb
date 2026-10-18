# DEV5 — 서버·세션·요청 프로토콜 상세 설계

2026-10-18 · **DEV5-DESIGN-001·DEV5-REVIEW-002/003 D2 ACCEPTED / 독립 재검토 반영 / 미구현·B3 NOT_RUN**

[구현·검증 계획](../plans/dev5-implementation-plan.md) · [소스 조사·검토](../evidence/dev5-source-research.md) · [DEV4 SQL](dev4-sql-design.md) · [COMM](comm-design.md) · [단계 완료](../plans/stage-completion.json)

이 문서는 `XADB-012/033`, B3의 SC03·KU14·PA08·PF11을 구체화한다. DEV4의 SQL 의미와 COMM의 전송 계약을 서버에서 조립한다. API·wire ID·수치는 개발 설계이며 현재 호출 가능한 서버나 안정 릴리스 호환 약속이 아니다. DEV2 부분 구현과 진행 중인 미커밋 코드를 DEV3/S1·DEV4/Q1·COMM/C1 완료 증거로 사용하지 않는다. 설계 준비는 선행 단계 완료 전 가능하고 실제 통합 착수·완료는 [계획](../plans/dev5-implementation-plan.md#entry)을 따른다.

<a id="scope"></a>
## 1. 초기 capability와 소유 경계

B3의 첫 서버는 Linux의 **소유 Unix pathname socket**, 단일 DB, peer UID 인증, DB 단위 권한, bounded worker pool과 세션별 직렬 실행을 제공한다. DEV4의 direct SQL·prepare/bind/execute/fetch/metadata/close, autocommit·명시 거래·DDL 두 outcome을 모두 전달한다. DEV6의 완성된 driver 없이 독립 local protocol client로 인수한다.

C1의 IPv4/IPv6 loopback transport 시험은 하위 회귀로 유지한다. DEV5 최초 SQL listener는 Unix만 열며 TCP SQL 접속 설정은 명시 Unsupported다. loopback을 인증으로 취급하지 않는다. 원격/TCP SQL·TLS·password/SASL·계정/GRANT SQL·객체별 ACL·세션 재개·durable 요청 중복 제거·LOB API·multiplexing은 미지원으로 협상하고 후속 의무를 보존한다. 아래의 bounded message 분할은 LOB 저장 기능이 아니다. resident/mixed-storage 엔진과 새로운 SQL dialect는 범위 밖이다.

| 소유자 | 책임·허용 방향 |
| --- | --- |
| foundation | 공통 ID/error/effect, quota, clock, cancellation·peer identity effect port |
| platform | 실제 Linux socket/readiness·peer credential·파일/시간/프로세스 효과. native UID를 typed 값으로 제공 |
| transport | AXF1 frame·partial I/O·byte/slot quota·local flush·connection close. SQL/auth/commit 판정 없음 |
| engine | DISK·WAL/undo·writer lease·snapshot·statement rollback·commit certainty·recovery/fencing |
| sql | DEV4 SQL session, prepared/result metadata·수명, DDL precommit 조립과 권한 검사 호출 위치 |
| server | 설정·인증/권한·wire adapter·요청 admission·scheduler·세션 소유·종료·계측 조립 |
| XADBTest | 독립 client/controller, literal 결과·외부 ACK history·fault 및 부정 사례 판정 |

`xadb-server → foundation, platform, transport, sql, engine`이라는 기존 package 경계를 사용한다. SQL/engine/transport가 server를 import하지 않는다. SQL에는 parser 결과의 statement class와 좁은 authorization callback을 전달하고 callback은 server 내부 정책의 불변 snapshot을 사용한다. 권한 때문에 SQL을 문자열 prefix로 판별하거나 server에 SQL parser를 복제하지 않는다. 실제 callback/peer port signature와 의존성은 D5-F01/F02에서 동결한다.

<a id="lifecycle"></a>
## 2. 서버·세션 소유와 실행

```text
Starting → Recovering → Serving → Quiescing → Closing → Stopped
    └──────── 실패/불변식/engine fence ───────────────→ Failed

Accepted → Handshaking → Ready ↔ Busy → Closing → Closed
                            └────→ Fenced → Closing
```

server 상태와 transport Open/Draining, SQL Idle/Active/Fenced를 별도로 기록한다. transport Open은 인증 성공이나 SQL 진입 가능을 뜻하지 않는다. writer lease·현재 build recovery·DB 불변식 검사를 마치고 worker/자원 예약을 만든 뒤 listener를 열어 Serving을 공개한다. 실패 시 이미 얻은 소유 자원을 역순 정리하며 복구 전 query를 받지 않는다. 두 server가 같은 DB를 동시에 소유할 수 없다.

I/O owner 한 개는 COMM 진행·frame 분해·bounded session mailbox·cancel 신호만 담당한다. SQL 실행·disk I/O·undo·worker join을 수행하지 않는다. worker 4개는 세션을 round-robin으로 선택하며 세션 상태를 한 번에 한 worker만 빌린다. 실행 중에는 연결 owner도 statement/result를 만지지 않는다. request 결과를 고정 completion slot에 반환한 뒤 worker를 풀로 돌려보낸다. output 대기 중 worker를 잡지 않고, 열린 result/transaction은 session에 남아 다음 fetch·rollback 작업으로 이동한다. guard/pin을 thread 간 이동할 수 있는지는 실제 공급 API에서 확인하며 `Send`를 강제로 가정하지 않는다.

<a id="worker-progress"></a>
**다른 세션의 실행이 있어야 풀리는 대기는 worker를 점유하지 않는다.** row/key transaction lock, offline DDL의 기존 거래/result drain, 그 drain에 막힌 신규 admission이 대상이다. SQL/engine은 안전한 중단 지점에서 wait registration과 재개할 실행 소유를 반환하고 scheduler는 해당 request를 `parked`로 둔 뒤 worker를 반환한다. continuation은 동일 statement token·이미 수행한 변경·rollback 예약·transaction/result 수명을 유지하며 SQL을 처음부터 재실행하지 않는다. 짧은 mutex/latch·부분 page pin 집합·worker stack의 대여를 남긴 채 중단하지 않는다. 재개는 같은 request/transaction generation에서 직렬 실행하며 원래 deadline을 연장하지 않는다.

대기 조건 검사와 wait registration 공개는 공급 계층에서 원자적으로 연결하여 unlock이 그 사이 발생해도 깨우기를 잃지 않는다. ready/cancel/deadline/disconnect/shutdown은 generation을 확인해 세션 고정 runnable slot 하나에 합치고, worker가 재개하여 조건을 다시 검사하거나 정리를 수행한다. I/O owner가 continuation을 실행하지 않는다. parked도 세션당 ordinary 1개·전체32개 한도 안에 남고 wait metadata는 control 예산, continuation이 보유한 실행 자원은 원래 SQL/engine 예산에 포함한다. 정상 runnable 세션은 round-robin, cleanup은 기존 우선순위를 따른다. 저장장치 I/O 완료의 실시간 상한까지 보장하지 않는다.

이는 현재 하위 API에 존재한다고 가정하는 기능이 아니다. D5-F03에서 SQL/engine 소유자가 실제 suspend/resume·취소/Drop 인계·thread 이동 계약과 해당 하위 회귀를 먼저 제공하고 별도 검토해야 한다. 공급이 없으면 실제 서버 조립 gate는 열리지 않는다. worker 수 증설·일반 timeout·engine의 transaction wait-for graph만으로 worker↔거래 소유자의 순환을 해결했다고 하지 않는다.

세션당 ordinary request는 assembling/queued/running/parked/reply-pending을 합쳐 한 개다. 다음 ordinary request는 이전 terminal reply의 모든 fragment가 COMM enqueue에 성공한 뒤에만 받는다. 정상 client는 완결 terminal reply와 자신이 보낸 Cancel의 CancelAck를 모두 수신한 뒤 다음 요청을 보낸다. client는 미응답 Cancel을 한 개만 허용하고 송신 상태를 직렬화한다. Reply가 CancelAck보다 먼저 도착해도 이 장벽을 생략하지 않는다. 이보다 빠른 pipelining은 Protocol 오류로 연결을 닫으며 숨은 실행 queue를 만들지 않는다. 종료된 요청의 결과 cache는 없고 고정 slot의 bounded 진단만 남긴다. accepted TX ticket은 회수 전까지 COMM quota에 남는다.

<a id="authentication"></a>
## 3. 인증·권한·정책 갱신

Unix endpoint는 C1과 같이 server가 만든 mode 0700 디렉터리 안에 배타 생성하고 socket은 0600으로 설정한다. symlink·기존 endpoint를 덮거나 타인 경로를 unlink하지 않는다. 초기 접근 대상은 같은 OS 계정의 local client다. OS peer UID를 platform이 실제 accepted socket에서 얻고, 설정의 `(UID, requested_principal, database)` allowlist와 일치할 때만 session을 만든다. peer credential 조회 실패·지원하지 않는 identity는 인증 실패로 닫는다. client가 보낸 UID/PID·principal 문자열만으로 인증하지 않는다. 같은 UID의 여러 principal은 그 OS 계정에 부여한 역할 선택이며 서로 다른 보안 주체라고 주장하지 않는다. 여러 OS UID의 공유 socket 디렉터리는 C1 소유 경로 계약 확장 전 지원하지 않는다.

초기 정책은 최대 64개 principal·단일 DB의 불변 map이다. credential·role은 SQL 데이터 catalog와 분리한 server 소유 설정에서 읽는다. 기본 allow 규칙은 없고 누락은 거절한다. 파일은 server 소유 regular file, symlink 금지, group/other write 금지, 최대 64 KiB와 엄격한 필드/중복 검사를 적용한다. SIGHUP은 signal handler에서 flag만 설정하고 조립부가 새 파일을 열어 전체 검증한 다음 policy epoch를 원자 교체한다. 입력 buffer·파싱 임시값·공존하는 immutable snapshot을 합쳐 policy pool 512 KiB 안에서 예약하며, slot/byte 부족 또는 검증 실패면 기존 정책을 유지하고 bounded 오류를 기록한다. 동작 종료 후 snapshot을 반환하고 transaction에는 필요한 권한 bitset만 보존하여 과거 policy 전체를 붙잡지 않는다. 설정 reload는 DB 거래나 SQL GRANT가 아니다.

| 권한 | 허용 범위 |
| --- | --- |
| connect | 인증 후 이 DB의 세션 생성·유지 |
| read | SELECT 및 그 prepare/bind/execute/fetch/metadata |
| write | INSERT/UPDATE/DELETE와 그 prepare/bind/execute; read를 자동 부여하지 않음 |
| ddl | 지원하는 direct CREATE/DROP TABLE/INDEX; read/write와 별도 |
| 거래·정리 | BEGIN/SET AUTOCOMMIT은 connect, COMMIT은 connect 및 그 거래가 사용한 read/write 권한 재확인. ROLLBACK·cancel·close는 자신의 기존 자원 정리를 위해 허용 |

새 업무 ordinary 동작은 최신 policy epoch에서 connect와 해당 권한을 검사한다. **자기 기존 자원의 ROLLBACK·CloseResult·CloseStatement·CloseSession 및 Cancel은 이 권한 재검사의 예외**다. 인증된 session/handle 소유·framing·문법·상태 검사는 유지하지만 read/write/ddl/connect 철회를 이유로 cleanup을 거절하지 않는다. ExecuteDirect의 ROLLBACK 분류는 SQL parser가 제공하며 문자열 prefix나 client가 선언한 class를 믿지 않는다. 이미 Closing이면 새 ordinary admission을 열지 않고 예약된 cleanup이 같은 작업을 소유한다. 정리에 새 사용자 거래나 새 업무용 권한을 요구하지 않는다.

prepared handle에 과거 권한을 저장해 영구 우회하지 않으며 metadata·bind·execute·fetch에서도 재확인한다. 검사 시 불변 policy snapshot을 고정하는 순간이 해당 동작의 권한 선형화점이다. 그 뒤 revoke가 와도 이미 승인한 동작은 완료될 수 있고 다음 동작은 새 epoch를 쓴다. 실행 중 강제 소급 rollback을 약속하지 않는다. connect 철회를 관측하면 새 업무 admission을 닫고 session cleanup을 예약한다. 기존 승인 동작의 commit 결과는 보존한다. read 철회 후 fetch 거절은 result를 미완료로 닫되 기존 명시 거래의 DML은 유지한다. write 철회 뒤 COMMIT 거절은 거래를 Active로 유지하여 명시 ROLLBACK을 허용하며, session deadline/close 때는 rollback한다.

**direct DDL의 ddl 권한 검사는 DEV4의 precommit 뒤 semantic 단계다.** 인증/connect·문법·상태·진입 취소 검사는 먼저 하지만, Active DML의 precommit이 COMMITTED된 다음 ddl 권한 거절이 날 수 있다. 이 경우 `PermissionDenied`, `prior_transaction_outcome=Committed`, `ddl_outcome=NotAttempted`, Idle을 반환한다. precommit 자체는 COMMIT의 현재 권한 조건도 만족해야 하며 거절하면 prior=NotAttempted·Active다. SC03의 금지 효과 0은 거절된 객체/문장 효과이고 이미 권한을 통과한 이전 거래의 commit을 지우지 않는다. 익명·인증 실패·신규 connect 거절에는 SQL/catalog 접근·거래 시작 효과가 모두 0이다. 기존 인증 세션의 connect 철회 뒤에는 새 업무 효과0을 유지하면서 위 소유 cleanup은 허용한다.

인증 실패의 외부 오류는 principal 존재를 구별하지 않는 `XADB-SRV-AUTH-FAILED`다. auth 성공 후 권한 거절은 `XADB-SRV-PERMISSION-DENIED`이며 자원/거래 상태를 명시한다. SQL 원문·bind 값·설정 내용·peer 민감값은 기본 로그에 남기지 않는다.

<a id="protocol"></a>
## 4. AXF1 위의 XSQ1 메시지

COMM의 AXF1 bytes·checksum·1 MiB payload 한도는 유지한다. 각 AXF1 payload 안에 다음 **40-byte XSQ1 header**와 chunk를 담는다. 모든 수는 unsigned big-endian이며 signed Int64 값만 two's complement다. padding·host struct 직렬화·압축·암묵 charset 변환은 없다.

| offset | bytes | 필드 |
| --- | --- | --- |
| 0 | 4 | ASCII `XSQ1` |
| 4 | 2 | protocol version=1 |
| 6 | 2 | message kind |
| 8 | 2 | FIRST=1, LAST=2 flags; 나머지는 0 |
| 10 | 2 | reserved=0 |
| 12 | 8 | session_id; Hello에는 0 |
| 20 | 8 | request_id; Hello에는 0, Cancel에는 취소 대상 ordinary ID |
| 28 | 4 | 전체 logical body bytes |
| 32 | 4 | chunk offset |
| 36 | 4 | chunk bytes; 실제 payload 길이와 정확히 일치 |

chunk는 0..262,144 bytes, logical request body는 최대 8 MiB다. 빈 body는 FIRST|LAST·offset=0인 한 frame만 허용한다. nonempty body의 chunk는 1 byte 이상이며 FIRST offset=0, 이후 offset은 직전 끝, LAST 끝=total이다. kind/session/request/total은 모든 조각에서 같고 중복·공백·역순·overflow·알 수 없는 flag/version/kind·잔여 bytes는 Protocol 오류 후 close다. ordinary 조립은 한 개만 허용하며 SQL은 모든 조각 수신·검사 후 호출한다. fragment bytes·buffer capacity·임시 복사가 공존하면 모두 quota에 센다.

첫 chunk에서 전체 body 예약과 slot을 확보하고 실패하면 SQL 효과 없이 연결을 닫는다. 최대 256 fragments/message·전체 조립 30초를 적용해 tiny-fragment flood와 deadline 갱신을 막는다. 1 MiB SQL+envelope와 256개 parameter의 큰 bind는 여러 frame으로 운반할 수 있지만, DEV4 8 MiB prepared 예산/전체 SQL quota와 request 8 MiB 한도를 각각 통과해야 한다. 각각 65,536-byte parameter 256개가 반드시 한 번에 bind된다는 보장은 없다. generic body 분할로 SQL·bind 한도를 조용히 축소하거나 모든 조각을 SQL로 따로 실행하지 않는다.

Hello/Welcome 및 Cancel/CancelAck는 단일 frame, body 4 KiB 이하이다. Hello는 정확한 version=1, database/principal UTF-8 이름 각각 128 bytes 이하, 필수 capability bitset만 받는다. 미지원 version/필수 bit는 거절하며 downgrade를 추측하지 않는다. Welcome은 server incarnation, session ID, 실제 capability bitmap·고정 한도·policy epoch·autocommit ON/Idle을 반환한다. server incarnation은 boot별 새 identity이고 session ID는 해당 boot에서 재사용하지 않는 checked counter다. handshake 실패 시 세션/SQL handle은 만들지 않는다.

ordinary request ID는 세션에서 1부터 하나씩 증가하며 FIRST admission에서 소비한다. duplicate/역행/overflow는 close하고 재실행하지 않는다. 이는 한 연결의 순서 검사이며 durable deduplication이 아니다. 재접속은 새 incarnation/session scope이며 과거 ID·handle·취소를 재사용할 수 없다. 서버가 모르는 과거 요청에 COMMITTED/ABORTED를 추측하지 않는다.

### 4.1 동작과 결과

아래 body는 동작별 내용을 뜻한다. ordinary body 앞에는 공통 `timeout_ms:u32`가 있으며 FIRST chunk는 이 4 bytes 전체를 포함해야 한다. CloseSession의 동작별 body가 비어 있어도 공통 prefix는 존재한다. Hello/Welcome/Cancel/CancelAck에는 ordinary timeout prefix를 넣지 않는다.

| kind | 메시지·body의 의미 | 성공 body |
| --- | --- | --- |
| 1 / 2 | Hello / Welcome | 위 협상·인증·세션 정보 |
| 16 | ExecuteDirect: SQL byte length+UTF-8 한 문장 | affected rows 또는 result handle+metadata 또는 거래/DDL outcome |
| 17 | Prepare: SQL+생략 또는 정확한 N개 optional type hint | statement handle+parameter/result metadata |
| 18 | BindAll: statement handle+typed value vector | 원자 binding 교체 완료 |
| 19 | Execute: statement handle | affected rows 또는 result handle+metadata |
| 20 | Fetch: result handle+max_rows+max_bytes | owned row batch+complete; EOF 여부 명시 |
| 21 / 22 | CloseResult / CloseStatement: 해당 handle | 해제 뒤 상태 |
| 23 | Describe: statement handle | immutable parameter/result metadata |
| 24 | CloseSession: 빈 body | cleanup 완료·Closed; 실패 시 RecoveryRequired 등 원래 효과 |
| 32 / 33 | Cancel / CancelAck: 현재 request ID 대상 | Requested 또는 NotActive; SQL terminal 결과가 아님 |
| 128 / 129 | Reply / Error: ordinary request의 terminal | 공통 outcome+동작별 body 또는 stable error |

BEGIN/COMMIT/ROLLBACK/SET AUTOCOMMIT/DDL은 ExecuteDirect의 DEV4 경로 하나로 전달한다. 이들을 별도 wire 동작과 이중 구현하지 않는다. Prepare는 SELECT/DML만 허용한다. Describe는 session·generation·권한을 검사하고 metadata를 반환한다. Invalid stale statement의 metadata는 DEV4처럼 조회 가능하되 현재 권한을 우회하지 않는다.

value는 `type:u8`(Bool=1, Int64=2, Utf8=3, Binary=4), `is_null:u8`(0/1), `length:u32`, bytes다. NULL은 선언 type+length=0, non-NULL Bool은 length=1·0/1, Int64는 length=8, 가변 타입은 명시 byte length다. Utf8 유효성을 검사하되 빈 text의 NULL 정규화는 SQL `bind_all`에서 한 번만 한다. fetch의 native-origin 빈 text는 non-NULL 길이0으로 보존한다. count·length·가변 이름은 할당 전에 전체 남은 길이와 checked 산술로 검사한다. 정확한 metadata/handle/outcome body field 순서와 literal byte fixtures는 D5-F01에서 구현 전에 동결한다.

handle은 connection에 묶인 session incarnation+slot+generation이며 statement와 result를 구별한다. slot 재사용·close·EOF·stale·Rollback 효과는 DEV4 정본을 따른다. 다른 세션의 우연히 같은 정수 slot을 허용하지 않으며 wire token에 주소를 넣지 않는다. CloseSession만으로 hidden commit을 만들지 않는다.

각 ordinary request는 서버 내부에서 terminal을 한 번 확정한다. 연결이 살아 있고 송신이 성공하면 그 terminal 한 개를 전송한다. 전달 exactly-once를 보장하지 않는다. common outcome에는 stable code, phase/span, statement_effect, SQL transaction_state, certainty, retry_scope, engine cause, 실제 제공된 optional commit identity를 둔다. direct DDL은 prior/ddl outcome과 각 certainty를 별도 필드로 유지한다. COMMITTED·ABORTED·UNKNOWN·NotCommittedRecoveryRequired와 NoTransaction/NotAttempted를 합치지 않는다. SQLSTATE mapping은 DEV6 소유다.

한 logical Reply/Error의 fragment 사이에 CancelAck를 끼워 넣지 않는다. 아직 terminal 송신을 시작하지 않았으면 CancelAck가 먼저 나갈 수 있고, 송신 중이면 마지막 fragment 뒤에 배치한다. client는 Reply와 CancelAck의 상대 순서를 가정하지 않으며 target request ID로 연결한다. I/O owner는 수신 frame을 분류하기 전에 완료 TX ticket을 회수해 이미 완결 송신된 CancelAck slot을 반환한다. peer가 Ack를 받은 뒤 보낸 다음 Cancel을 로컬 완료 회수 지연만으로 flood로 판정하지 않는다.

Fetch는 한 번에 DEV4의 1..128 rows·canonical batch 256 KiB 이하이고 serialized reply는 최대 512 KiB다. complete=false는 요청의 fetch 성공이지만 query 전체 완료가 아니다. complete=true는 EOF/LIMIT 완료만 뜻한다. Request Reply와 query completion, transaction commit은 별도다. row가 caller budget보다 크면 RowBufferTooSmall+required_bytes로 위치를 유지한다. 큰 reply는 bounded chunk로 보낼 수 있고 모든 조각 수신 전에는 client가 terminal을 관측했다고 기록하지 않는다.

<a id="resources"></a>
## 5. admission·backpressure·시간 제한

다음은 초기 개발 profile이며 성능 실측·제품 용량/SLA가 아니다. C1·DEV4·DEV3 한도는 별도 원문을 유지하며 server 설정이 몰래 늘리지 않는다.

| 자원 | 초기 상한·책임 |
| --- | --- |
| transport 연결 | C1 전체 128; handshake 중 16, 인증 session slot 32. Closing/Fenced/Closed라도 cleanup·continuation·응답 소유가 남으면 session slot을 유지하며 나머지도 C1 slot에 합산 |
| worker/queue | SQL worker 4; 세션당 ordinary 1, 전체 assembling/queued/running/parked/reply-pending 32. queue·wait/runnable 등록은 session 고정 slot이며 별도 무한 FIFO 없음 |
| request assembly | 세션당 body 8 MiB, 전체 capacity 32 MiB; SQL owned binding으로 복사 중 양쪽 회계 |
| reply staging | 세션당 512 KiB 1개, 전체 16 MiB. SQL batch 256 KiB와 wire 복사 동시 존재를 각각 계산 |
| control/cleanup | session당 16 KiB, server 전체 2 MiB의 고정 metadata/terminal/cancel/cleanup slot·policy pool·diagnostic ring. CancelAck 대기 최대 1개/session |
| SQL | DEV4 compiled/bound+workspace DB 전체 64 MiB, session prepared 64개·result 1개. snapshot 32는 engine이 전체 소비자와 공유 |
| transport payload | C1 64 MiB 및 RX/TX byte·slot 상한 유지. server staging과 소유권이 실제 이동한 allocation만 중복 계산에서 제외 |
| SQL 임시 disk/FD | DEV4 query 256 MiB·DB 1 GiB, query FD10·SQL 전체64. transport·engine·server FD와 실제 합산 |

server 부가 payload 상한은 assembly32+reply16=48 MiB, control2 MiB이고 SQL64·transport64·engine/stack/allocator/kernel socket 메모리는 별도로 합산한다. 동시 복사 peak와 실제 capacity가 예산에 들어가는지 측정한다. 이 숫자로 RSS 상한이나 모든 allocator OOM의 복구를 보장하지 않는다.

SQL 호출 전에 request·completion/cleanup slot·최대 응답 staging을 예약한다. DML/COMMIT/DDL의 작은 terminal은 고정 4 KiB outcome 영역으로 mutation 이전 확보한다. 불필요한 error text는 길이 제한하며 원래 code/effect/certainty는 자르지 않는다. prepare/execute의 metadata도 512 KiB 상한 안에서 예약한다. Fetch는 output credit을 얻기 전 cursor를 진행시키지 않는다. TX Backpressured이면 이미 만든 한 reply를 session에 보유하고 worker를 반환한다. 그동안 새 fetch를 만들거나 row/page guard를 TX queue에 빌리지 않는다.

권한 철회·idle·disconnect 등으로 예약한 내부 cleanup은 새 업무용 assembly·512 KiB reply staging·TX credit을 기다리지 않는다. 기존 cleanup slot과 필요한 작은 terminal 영역을 사용하며, 보류 중인 기존 terminal을 덮어쓰지 않는다. output이 막혀도 정리 소유를 scheduler에 전달하고 새 업무 admission은 닫힌 상태로 유지한다.

I/O owner는 output이 막혀도 현재 연결의 control frame을 읽고 RX lease를 즉시 돌려준다. ordinary pipelining은 거절하므로 worker가 RX를 잡고 기다리는 순환이 없다. Cancel은 동일 인증 연결에서만 현재 request에 신호를 주고 SQL worker slot이 없어도 진행한다. pending CancelAck가 있는 동안 반복 Cancel은 같은 target의 신호로 합치며 추가 ack를 예약하지 않는다. 다른 target/control flood는 Protocol close다. 다른 연결은 C1의 순회 budget으로 진행한다. control의 전달도 OS scheduling·frame 전송 및 deadline 영향을 받으며 무조건 즉시 응답을 약속하지 않는다.

| deadline | 시작·만료 동작 |
| --- | --- |
| handshake 5초 | accept부터 Welcome enqueue까지; 실패/만료는 SQL 효과 없이 close |
| assembly 30초 | FIRST부터 LAST 검증까지, 조각마다 연장 없음 |
| ordinary 기본30초, 요청 지정1..30,000 ms | FIRST 승인부터 queue·실행 포함. 서버 공유 monotonic clock 기준이며 client 절대 시각을 받지 않음 |
| output 10초 | terminal 확정부터 모든 frame local flush 관측까지; 만료는 disconnect cleanup, SQL outcome은 유지 |
| result/Active idle 30초 | 마지막 ordinary terminal 확정부터 다음 완결 ordinary의 worker 인계까지. Cancel/부분 byte는 갱신하지 않음. 만료 시 Closing·session cleanup |
| session 전체 idle 60초 | 최초 Ready부터, 이후 마지막 ordinary terminal 확정부터 다음 완결 ordinary의 worker 인계까지. 만료 시 Closing·session cleanup; C1 idle이 먼저 종료할 수도 있음 |
| active transaction 최대300초 | engine user transaction 생성부터; statement 성공/park·재개로 연장 없음. 아래 거래 수명 만료 규칙 적용 |
| server graceful 30초 | Quiescing 시작부터, C1 drain 5초는 마지막 송신 단계에 포함 |

ordinary timeout field는 D5-F01 body의 공통 prefix에 두며 0은 기본30초다. shared Clock의 동일 epoch를 driver/scheduler/worker가 사용한다. timeout은 취소 신호·admission 차단 시점이며 undo/commit I/O 완료 시간의 hard deadline이 아니다. cleanup이 끝나기 전에 ABORTED를 만들거나 request/transaction 소유 slot을 재사용하지 않는다. worker 반환은 실행이 끝났거나 위 안전한 중단·인계를 마친 경우만 가능하며 실행 중 thread를 강제 회수하지 않는다. terminal 이미 확정 뒤 ordinary deadline은 그 결과를 덮지 않고 output deadline만 적용한다. C1 frame/TX/idle deadline도 동시에 적용한다. SYS clock overflow/역행·engine panic/poison은 adapter/engine 실패로 차단한다.

**idle은 ordinary 실행을 기다리는 구간에만 적용한다.** 최초 Ready 시각은 Welcome enqueue 성공 시 고정한다. output 대기·다음 입력 조립·queued 동안에도 idle 시계는 가고, 완결 ordinary를 worker에 인계하기 직전에 현재 시각과 deadline을 비교한다. 이미 만료한 세션을 늦은 요청으로 되살리지 않는다. running과 parked는 같은 실행 중이므로 idle 대상에서 제외하고 원래 ordinary/transaction deadline을 유지한다. result/Active가 실행 도중 처음 생겨도 idle은 해당 terminal 확정 뒤 시작한다. 새 terminal 뒤 idle 기준을 갱신하되 transaction 생성 시각은 갱신하지 않는다.

두 idle 만료도 다음 client 요청 없이 admission을 닫고 예약된 cleanup을 깨운다. result를 미완료로 닫고 미commit 거래 전체 undo를 확인한 뒤 세션을 종료하며 기존 terminal은 보존한다. 요청이 없는 만료에는 가짜 request ID/terminal을 만들지 않고, 조립/queued가 있으면 SQL 실행 없이 정리한다. undo 실패는 RecoveryRequired/Fenced로 보존하며, 일반 statement timeout처럼 Active로 복귀시키지 않는다. 만료 관측은 I/O·undo 완료의 hard deadline이 아니다.

**거래 수명 만료는 일반 statement timeout과 다르다.** scheduler가 transaction generation과 생성 시각을 추적하고 `now >= created_at + 300초`이면 그 세션의 신규 ordinary admission을 닫고 Closing으로 전이한다. 조립/queued 요청을 제거하고 running/parked 요청에 취소·cleanup 신호를 전달한다. worker가 result를 무효화하고 미commit 거래 **전체** undo 완료를 확인한 뒤 세션을 닫는다. 일반 statement 취소처럼 Active로 복귀하거나 성공 문장으로 타이머를 다시 시작하지 않는다. CommitAttempted가 먼저 선형화되었으면 그 결과를 기다리고 COMMITTED/UNKNOWN을 만료 때문에 ABORTED로 바꾸지 않는다. commit 진입 직전에는 동일 clock의 deadline과 만료 신호를 다시 확인한다. 확정 전 요청의 terminal은 필요한 전체 cleanup 뒤 한 번만 확정하고, 이미 확정된 terminal은 그대로 둔 채 후속 cleanup을 수행한다. 요청이 없는 idle 만료에는 새 request ID/terminal을 만들지 않는다. undo 실패는 RecoveryRequired/Fenced로 보존하고 정상 종료로 세지 않는다. 300초는 새 작업 차단 시점이며 I/O·undo 완료의 hard deadline이 아니다.

<a id="cancel"></a>
## 6. 취소·disconnect·확실성

| 요청 단계 | cancel/timeout/disconnect 뒤 행동·관측 |
| --- | --- |
| assembly | SQL 미호출. Cancel 또는 timeout은 조립 폐기 후 close해 남은 fragment를 새 요청으로 오인하지 않음 |
| queued | worker 인계 전 request를 원자 제거, 효과 없음 terminal. disconnect이면 terminal 전달은 불가하고 세션 cleanup 예약 |
| parked | 기존 wait registration을 취소/깨우고 같은 continuation을 worker에 한 번만 인계. 일반 request 취소는 SQL의 statement/거래 효과, disconnect·거래 수명 만료는 세션 전체 cleanup. wait token 폐기만으로 undo 성공을 만들지 않음 |
| SQL 실행·commit 전 | SQL cancellation token을 신호. statement undo 완료 후 DEV4의 implicit 종료/명시 거래 유지. cleanup 실패는 RecoveryRequired/Fenced |
| CommitAttempted 이후 | engine 결과를 기다림. 늦은 cancel이 COMMITTED를 ABORTED로 바꾸지 않음. engine UNKNOWN/Fenced와 NotCommittedRecoveryRequired를 그대로 전달 |
| terminal 확정·전송 중 | CancelAck=NotActive. 확정된 결과는 불변; 전송 실패 시 client는 실제 완결 수신 이력에 따라 판단 |
| 이전 request ID·다른 session | 현재 요청에 신호를 주지 않음. 같은 세션의 완료 ID는 NotActive, 다른 session token은 Protocol close |

Cancel의 Requested는 token 신호 전달만 뜻한다. 최종 성공·취소·commit 결과는 원래 request의 Reply/Error에서만 안다. signal과 terminal 확정은 request generation을 확인하는 동일 짧은 lock/atomic 상태에서 선형화하여 다음 요청에 오래된 signal이 붙지 않게 한다. worker는 session lock을 잡고 socket/queue 공간·engine blocking I/O를 기다리지 않는다.

peer FIN은 C1의 새 TX admission까지 닫으므로 request delimiter로 쓰지 않는다. 아직 실행되지 않은 완결 RX가 남아 있어도 SQL admission을 닫고 실행하지 않는다. 실행 중 거래는 disconnect token을 관측하며 이미 commit한 결과는 유지한다. unread row prefix·부분 Reply·FlushedLocally·server 로그는 client가 COMMITTED를 받은 증거가 아니다.

client가 terminal을 완결 검증해 기록했으면 그 outcome을 보존한다. mutation/commit 가능 요청을 전송했으나 terminal을 받지 못했으면 **client 관측은 UNKNOWN**이다. 이는 server engine이 반드시 UNKNOWN/Fenced라는 뜻은 아니다. server는 COMMITTED일 수 있고 정상 서비스할 수 있다. 반대로 engine의 durable I/O UNKNOWN이면 engine writer fence를 유지하고 모든 세션의 새 engine 작업을 차단한다. 서버는 새 연결을 만들어 fencing을 우회하지 않는다.

재접속은 새 session이며 자동 SQL 재전송·이전 handle 복원·transaction 재개를 하지 않는다. SELECT나 PK 존재만 보고 과거 요청의 commit 여부를 증명하지 않는다. 초기 capability는 durable request lookup/idempotency를 제공하지 않으므로 미확정 업무는 caller가 해결할 의무로 남긴다. 이 제한은 ACK를 받은 거래의 복구 의무를 낮추지 않는다.

<a id="shutdown"></a>
## 7. 종료·정리·복구

명시 CloseSession은 해당 요청 terminal 공간을 먼저 확보한 뒤 result/statement 무효화, 미commit 거래 undo 완료 확인, 자원 해제, terminal enqueue, transport drain 순서다. 실패하면 원래 cleanup/Fenced 결과를 남긴다. disconnect·idle 만료·owner Drop은 같은 소유 cleanup slot으로 인계한다. slot/transaction/undo 소유를 실제 cleanup 완료 또는 engine recovery 소유 인계 전 재사용하지 않는다. Drop은 새 allocation·hidden commit·blocking join을 하지 않으며 인계 실패는 engine fence/프로세스 실패다.

server shutdown은 다음 순서를 따른다.

1. server admission과 listener를 닫고 새 ordinary request를 거절한다. **이때 C1 begin_drain은 아직 호출하지 않는다.** queued ordinary는 미실행으로 정리하고 running은 cancel 신호를 보낸다.
2. 실행 중 commit은 engine 결과까지 수신한다. parked 요청도 깨워 같은 실행 소유를 cleanup에 넘긴다. result/명시 거래 cleanup을 worker가 수행하며 고정 cleanup slot은 신규 업무 queue 고갈과 별개로 접근 가능하다. 각 worker가 자기 실행 구간을 끝내거나 대기를 등록해 반환한 뒤 cleanup을 먼저 배정한다.
3. worker가 만든 최종 reply를 COMM에 enqueue한 뒤 연결별 begin_drain을 호출한다. COMM Draining은 새로운 reply를 받지 않으므로 순서를 뒤집지 않는다.
4. accepted TX를 남은 shutdown 시간과 C1 drain 상한 안에서 마무리하고 close/unregister, worker·driver 명시 join, engine 명시 close, 소유 socket identity 확인·unlink를 수행한다.

30초 안에 cleanup/join/engine close를 끝내지 못하면 graceful 성공을 보고하지 않는다. 응답을 포기하고 연결을 닫으며 DB writer lease·undo 소유를 임의 해제하지 않는다. 재사용을 차단한 Failed 상태에서 소유 프로세스를 종료하고 다음 startup recovery로 넘기는 정책을 사용한다. 실행 중 thread를 분리한 채 같은 engine을 다시 열지 않는다. 자동 강제 종료의 구체 supervisor API는 D5-F03에서 고정한다. 정상 종료에 추가 checkpoint가 필수인 것처럼 WAL durability를 변경하지 않는다.

<a id="ack"></a>
## 8. ACK·계측·인수 경계

COMMIT/implicit DML/direct DDL 성공 응답은 engine이 durable commit과 원자 공개를 확인한 후에만 작성한다. 명시 거래 안 DML의 affected_rows 성공은 statement 성공이며 COMMITTED가 아니다. direct DDL의 precommit과 DDL commit은 각각 별도 identity/결과로 계측한다. engine이 무변경 거래에 commit_seq/WAL receipt를 주지 않으면 server가 만들어 붙이지 않는다.

외부 controller는 actual socket에서 받은 Reply/Error 전체를 독립 decoder로 검증하고 ACK history를 server 프로세스 밖에 보존한다. 그 뒤 candidate만 kill/reopen하여 row/index/catalog와 literal 기대값을 비교한다. ACK 받은 commit은 모두 보존되고 확인된 abort는 보이지 않아야 한다. UNKNOWN은 commit 또는 abort 중 원자적 결과만 허용하며 부분 상태는 금지한다. precommit만 ACK되지 않은 DDL crash도 두 거래 단위로 판정한다. process kill·I/O fault·전원 장애 모델을 별도 결과로 보고한다.

PF11은 connection/session/request→queue→worker→SQL plan/transaction→WAL 공유 작업→reply enqueue/local flush→외부 수신을 연결한다. client와 server clock을 직접 빼지 않는다. 외부 latency는 controller 자체 시계, 내부 span은 공유 server monotonic clock에서 계산한다. 병렬 worker 시간의 합은 wall latency와 구분하고 shared commit 작업은 중복 원인 귀속을 표시한다. queue/실행/output 대기, quota/권한 거절, cancel 요청·관측·cleanup, partial send, lost trace, successful TPS·attempt TPS·conflict 비율을 기록한다. 본문 SQL/parameter는 기본 기록에서 제외한다.

상시 계측은 event당 최대128 bytes인 고정 ring 4,096 events(512 KiB, control2 MiB 안에 포함)와 bounded aggregate만 보존하며 session/request label을 영구 누적하지 않는다. 유실·미계측을 0으로 만들지 않는다. 수치 profile·SC03/KU14/PA08의 반례·known-bad·실제 disk/소켓 인수는 [D5-T01–T14](../plans/dev5-implementation-plan.md#acceptance)와 [PF11](../plans/dev5-implementation-plan.md#performance)이 소유한다.

<a id="decision"></a>
## 9. DEV5-DESIGN-001 결정 기록

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 작업·계약·통합 담당 | `/root`; 사용자의 DEV5 설계 요청과 DP01의 기존 D2 위임 |
| 선택 | local Unix peer 인증·DB 권한, AXF1 위 bounded XSQ1, 세션별 ordinary 1개·고정 worker, same-connection request cancel, 외부 ACK와 engine 결과 분리 |
| 대안·이유 | TCP token/password/TLS를 임시 자체 crypto로 구현하지 않고 현재 C1 Unix 경계에서 인증을 제공. thread-per-session은 thread/slow peer 비용, 전면 async는 새 runtime/복잡성이 있어 고정 worker 기준선 선택. PostgreSQL형 별도 cancel 연결 대신 항상 읽는 I/O owner와 request-scoped token으로 세션/target 경합을 제한 |
| 적용·대체 | DEV5 미정 leaf 구체화. DEV4 SQL·DDL 두 outcome·COMM 전송·B3 선행·고객 호환 의무를 대체하지 않음. TCP SQL/보안/LOB/driver 후속 범위는 미완료로 유지 |
| 후속 수정 | [DEV5-REVIEW-002](#review-corrections)가 worker 대기·거래 수명 만료·cleanup slot 회계를 보완. 최초 검토 기록은 당시 범위로 보존 |
| 근거·검증 | [실제 source 조사](../evidence/dev5-source-research.md), [동결·인수 계획](../plans/dev5-implementation-plan.md). 외부 코드·새 dependency 도입 없음 |
| 독립 검토 | `/root/dev5_design_review`가 새 문서3개·공유 문서의 DEV5 diff·상위 계약·source identity를 독립 읽기 검토. CancelAck/terminal 경합 수정 후 채택 가능·잔여 필수 P1/P2 없음. `/root` 계약/통합 담당 반영 확인. [검토 기록](../evidence/dev5-source-research.md#review) |
| 구현·실행 | NOT_RUN. Q1/C1/B3·SC03/KU14/PA08/PF11·공식 배포 완료 아님. 문서 자동 검사는 push 전용 |
| 재검토 조건 | 하위 API가 worker 소유 이동/권한 callback/cleanup 인계를 지원하지 못함, control starvation·quota peak 위반, remote/다중 DB/동적 객체 ACL/LOB 도입, 실측에서 worker·연결 한도가 요구를 충족하지 못함 |

사내 개발은 새 fixture와 현재 build protocol을 사용한다. 안정 릴리스의 모든 이전 same-major client→새 server 의무는 [버전 정책](../policies/version-compatibility-policy.json)에 남는다. 설계가 채택되어도 D5-F01–F05의 실제 API·byte fixtures·실행 recipe는 구현 전 별도로 동결한다.

<a id="review-corrections"></a>
## 10. DEV5-REVIEW-002 — 대기 진행과 거래 수명 수정

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 권한·담당 | 사용자의 DEV5 설계 리뷰·문제 즉시 수정 요청, DP01 위임. 작업·계약·통합 담당 `/root` |
| 선택·반례 | worker4의 transaction-lock 대기로 holder COMMIT이 queue에 갇히는 경로는 명시 parked/resume로 수정. 거래300초 만료는 Closing·전체 undo로 정하고 늦은 취소의 commit outcome 불변 유지. cleanup 중 session32 회계와 모든 reply fragment enqueue 경계를 명시 |
| 대안·이유 | worker 증설/timeout 의존은 같은 자원 순환을 반복함. 전면 async runtime 대신 다른 세션 진행에 의존하는 wait만 공급 API에서 중단·재개. 거래 만료를 statement 취소로 처리하면 Active 수명을 제한하지 못하므로 세션 종료로 일관되게 정리 |
| 대체 관계 | DEV5-DESIGN-001의 해당 대기·수명·slot 문구만 대체. SQL 의미·engine commit/undo 원자성·worker4/session32/300초 수치·B3 선행 의무 유지 |
| 검토·실행 | `/root/dev5_independent_review`가 원문/반례·7개 파일의 DEV5 diff·직접 소유 계약을 독립 재검토: 채택 가능, 잔여 필수 P1/P2 없음. `/root`가 계약·통합 담당으로 반영 확인. [반례와 작업 기록](../evidence/dev5-source-research.md#review-corrections). 하위 API·모델·제품/성능 인수 NOT_RUN |
| 재검토 | suspend/resume 공급 불가, wait 등록/취소의 소유 유실, worker↔세션 대기 순환, 거래 수명 만료 뒤 Active 재사용 또는 cleanup quota 초과 |

<a id="oracle-server-review"></a>
## 11. DEV5-REVIEW-003 — 권한 철회 후 정리와 idle 경계

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당·권한 | 작업·계약·통합 `/root`, 채팅 `01a14f3d-7736-74c1-94d3-dae23716d7c0`; 사용자 설계 리뷰·문제 즉시 수정·Oracle/로컬 DBMS 참고 요청과 DP01 위임 |
| 선택·반례 | 일반 권한 재검사에서 자기 cleanup을 명시 제외. Ready/terminal/완결 요청 인계와 idle 적용 구간·Closing을 고정. blanket gate가 철회 후 ROLLBACK/close를 막거나 실행 중 idle로 거래를 끊는 해석을 제거 |
| 대안·이유 | 정리에도 새 권한을 요구하면 기존 소유 해제를 막는다. 모든 byte로 idle 갱신하면 trickle로 수명을 연장하고, running/parked를 idle로 세면 request deadline과 충돌하므로 제외. 내부 cleanup은 기존 예약을 사용 |
| 대체·유지 | 001의 ordinary 권한 검사·idle 문구를 구체화. 별도 채팅에서 수정한 002의 worker4/parked·300초 만료·slot 회계와 선행 gate를 보존. DEV4 SQL/COMM/B3·품질 의무 유지 |
| 근거·검증 | [Oracle 19c·PostgreSQL·MariaDB·CUBRID 실제 조사](../evidence/dev5-oracle-source-review.md), F02/F03·T04/T09/T12·known-bad 보완. 제품/모델/복구/성능 NOT_RUN |
| 독립 검토 | 이 채팅의 `/root/dev5_independent_review`가 원문 반례와 002 수신본 대비 기존8개 파일의 DEV5 diff·새 조사1개, Oracle19c 5개 원문·로컬3checkout/6파일 identity와 선택 코드를 대조: 채택 가능, 잔여 필수 P1/P2 없음. `/root` 계약·통합 담당 반영 확인. 002의 같은 이름 검토자와는 별도 실행 |
| 재검토 | cleanup 권한 우회로 새 업무 실행, 철회/idle 뒤 자원 소유 유실, 실행 중 idle 오판정, 후속 driver와 명시 SQL 의미 불일치 |

Oracle은 일반 COMMIT에 추가 권한을 요구하지 않지만 XADB는 위 권한 재확인을 선택했다. Oracle JDBC의 OFF→ON 암묵 commit과 달리 XADB의 Active + SET AUTOCOMMIT은 DEV4대로 TxnState이고, CloseSession은 미commit 거래를 rollback한다. 이 차이를 protocol adapter에서 조용히 바꾸지 않는다. Oracle의 DDL precommit·문장 원자성·durable commit 응답 유실·취소 결과 구분은 기존 XADB 경계와 대조했으며 공식 원문·버전·선택 이유는 위 조사에 기록한다.

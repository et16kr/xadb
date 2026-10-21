# DEV6 — native·JDBC·ODBC 클라이언트 상세 설계

2026-10-19 · **DEV6-DESIGN-001·DEV6-REVIEW-002 D2 ACCEPTED / 독립 검토 반영 / 미구현·CLIENT1 NOT_RUN**

[구현·검증 계획](../plans/dev6-implementation-plan.md) · [Oracle·ARIES·로컬 소스 조사](../evidence/dev6-source-research.md) · [DEV4 SQL](dev4-sql-design.md) · [DEV5 서버](dev5-server-design.md) · [COMM](comm-design.md) · [완료 계약](../plans/stage-completion.json)

이 문서는 XADB-034/035/036·SC04/05/06의 개발 기준선이다. API 이름은 설계이며 현재 배포되는 driver가 아니다. DEV5 B3와 DEV3/4·COMM의 실제 증거를 받기 전에는 실제 서버 통합이나 CLIENT1 완료를 주장하지 않는다. Oracle 19c·공개 DBMS는 비교 입력이며 SQL dialect·wire·ABI 호환 제품을 만드는 계약이 아니다.

<a id="scope"></a>
## 1. 지원 범위와 소유

첫 profile은 Linux x86_64 GNU, 같은 OS 계정의 소유 Unix pathname socket, 단일 DB, DEV4의 Bool/Int64/Utf8/Binary·NULL과 SQL subset이다. Native Rust 및 C facade → JDBC → ODBC 순으로 인수한다. JDBC는 Java SE 21의 JDBC 4.3 인터페이스에 대한 **명시적 부분 지원**, ODBC는 unixODBC의 ODBC 3.8 API에 대한 **명시적 부분 지원**이다. 전체 JDBC/ODBC 표준 준수 인증을 주장하지 않는다. 최초 인수 환경은 DP06의 Ubuntu 24.04 계열이며 정확한 JDK/vendor/patch·Maven·unixODBC/compiler/header·이미지 digest는 D6-F05에서 실행 전에 고정한다.

| 소유 | 역할·의존 방향 |
| --- | --- |
| SQL/engine | 값·이름·SQL 문법·prepare 타입·statement/transaction·DDL 두 outcome·복구. client에 parser/transaction engine을 복제하지 않음 |
| DEV5 | XSQ1 업무 메시지·권한·session/request/handle·서버 한도. client와 server가 같은 wire 정본을 소비 |
| COMM/platform | AXF1·byte I/O/readiness·Linux socket. Rust core는 기존 `xadb-client → foundation, transport` 경계 사용, OS는 외부 조립에서 주입 |
| native core | 세션 상태, owned binding/row/metadata, bounded codec·요청 진행·취소·오류. engine/sql/server package import 금지 |
| C facade·ODBC | 포인터/ABI·호출자 메모리·진단. native core를 소비하는 좁은 FFI 경계; platform 조립은 바깥 adapter |
| JDBC | 순수 Java codec·동기 API·JDK Unix domain SocketChannel/Selector. JNI·Rust library 로딩을 필수로 하지 않음 |
| XADBTest | 별도 client process·literal protocol/semantic oracle·외부 ACK 이력·fault·reopen 판정. 제품 codec으로 기대 bytes를 만들지 않음 |

기존 계획 위치 `crates/client`, `clients/jdbc`, `crates/odbc`를 따른다. C facade의 별도 FFI package 경계·실제 파일·platform 조립 의존은 D6-F03 구현 때 RB04/06으로 확정한다. 이번 설계로 빈 package·manifest·source fingerprint 경로를 추가하지 않는다. Rust/Java가 wire 구현을 각각 갖지만 field/값 의미의 정본은 DEV5 하나이며 literal conformance corpus를 공유한다. production encoder를 oracle decoder와 공유하지 않는다. 공통 계약 동결 후 Native/C·JDBC·ODBC·독립 검증 도구는 [병렬 개발 표](../plans/dev6-implementation-plan.md#parallel)의 선행 조건·소유 범위 안에서 동시에 개발할 수 있다. SC04→SC05→SC06의 실제 인수 순서는 유지한다.

TCP SQL·TLS·password·원격 인증·Windows/macOS·pool/XA·failover·batch/array·LOB·CallableStatement·generated keys·scroll/update cursor·여러 active result·async public API는 후속 leaf다. 영구 제외나 구현 완료로 처리하지 않는다. 후속 type/SQL은 DEV7, remote 보안은 DEV5/COMM 확장, 안정 client→server 호환은 VERSION_RELEASE에서 인수한다.

<a id="protocol"></a>
## 2. 서버 수신 계약과 wire 경계

AXF1과 XSQ1 version=1·40-byte header·kind·fragment·checksum·request 순서는 DEV5/COMM 정본을 따른다. connect는 endpoint/principal/database 및 필수 capability를 명시한다. password/TCP 주소·알 수 없는 필수 옵션은 거절한다. Hello/Welcome 전체 검증 전 SQL/handle을 공개하지 않는다. protocol 불일치를 best-effort downgrade로 우회하지 않으며 개발 snapshot은 동일 build의 동결 wire profile을 사용한다.

DEV5 D5-F01과 공동 동결할 **D6-W01 공급 요구**는 다음과 같다. 미동결 body를 이미 존재하는 API로 취급하지 않는다.

1. Prepare/Describe에 parameter ordinal/type/nullability/bound byte limit, result ordinal/label/source name/type/nullability/bound byte limit, statement class와 dependency identity를 명시한다. computed/미상 source·nullability는 unknown으로 표현하고 가짜 schema/table을 만들지 않는다. 기존 ordinal과 이름 대소문자를 보존한다.
2. 모든 ordinary Reply/Error에 authoritative `autocommit`, SQL transaction state, result 상태와 기존 effect/certainty/retry_scope·DDL 두 outcome을 담는다. client가 SQL 문자열 prefix나 이전 성공만으로 거래 상태를 추정하지 않는다. 원래 오류와 모드 갱신을 같은 terminal에 적용한다.
3. ExecuteDirect/Execute에 `expected_result = Any | Rows | Update`를 둔다. server가 SQL parser/prepared class로 일치 여부를 **거래 시작·DDL precommit·첫 효과 전에** 검사한다. Rows는 SELECT만, Update는 DML/DDL만 받으며 transaction-control은 Any다. 불일치는 효과0. `executeQuery(UPDATE...)`를 실행·commit한 뒤 오류로 바꾸지 않는다. lower SQL은 기존 class와 진입 검사 순서를 공급하며 새 SQL 의미를 만들지 않는다.
4. Welcome에 실제 SQL/parameter/result 한도와 protocol profile identity·capability를 싣는다. exact enum/field order/metadata overhead와 독립 byte fixtures는 D5-F01/D6-F01에서 공동 동결한다. 모든 frame/body cap에는 header·metadata overhead를 포함한 계산을 남긴다.

단일 연결에 ordinary 한 개와 미응답 Cancel 한 개만 둔다. request ID는 DEV5대로 세션마다1부터 checked 연속 증가하고 overflow면 연결을 닫으며 wrap하지 않는다. local validation·quota 예약·encode·송신 전 취소는 ID를 소비하지 않는다. exclusive owner가 다음 ID를 예약하고 FIRST frame의 transport enqueue 수락 시 소비한다. Backpressured/확정 미수락이면 같은 예약으로 재시도하며 새 ID를 발급하지 않는다. enqueue 수락 뒤 peer 전달을 확인할 수 없거나 일부 전송 뒤 실패하면 연결을 폐기하고 ID를 재사용하지 않는다. wire request ID와 local API operation generation은 별도다. session/server incarnation과 handle generation을 모두 대조한다. terminal의 모든 fragment를 검증한 뒤만 outcome을 공개하고, 자신이 보낸 CancelAck도 수신해야 다음 ordinary를 송신한다. terminal을 먼저 받았으면 결과를 보존한 채 Ack를 기다리고, Ack 대기 실패로 이미 알려진 COMMITTED를 UNKNOWN으로 바꾸지 않는다. 연결은 재사용 불가로 닫는다.

<a id="state"></a>
## 3. 호출·수명·취소 상태

연결의 `Connecting → Ready ↔ InFlight → Closing → Closed`와 `Broken`을 SQL의 Idle/Active/Fenced와 별도로 관리한다. Ready는 마지막 완결 관측이며 서버가 이후 idle 만료하지 않았다는 보장이 아니다. 다음 I/O에서 EOF/만료를 관측하면 Broken으로 전환한다. protocol 오류·유실·terminal decode 불가 뒤 상태를 추측하여 Ready로 복귀하지 않는다.

| 객체·경로 | 계약 |
| --- | --- |
| 연결 동시성 | public operation은 세션당 하나. CloseResult→BindAll→Execute 등 한 API의 전체 조립과 local 상태 갱신을 같은 exclusive guard로 보호. 다른 ordinary/close 및 setter/rebind와의 경쟁은 Busy/HY010이며 숨은 무제한 queue를 두지 않음. JDBC/ODBC는 내부 보호를 하되 애플리케이션이 연결을 직렬 사용 |
| statement | DEV4 ReadyUnbound/ReadyBound/ResultOpen/Invalid/Closed를 소비. 서버 최대64개와 metadata/binding 예산을 적용. stale는 자동 prepare/execute 재시도 없이 명시 오류 |
| result | 연결당 active 하나. forward-only·read-only, 현재 row/batch만 보유. EOF/error/close와 query complete를 구별. EOF 마지막 batch의 남은 row는 client에서 모두 소비한 뒤 next=false |
| 재실행 | JDBC 같은 Statement의 재실행은 자기 이전 ResultSet을 명시 close한 뒤 실행. 다른 statement의 active result는 자동 폐기하지 않고 ResultBusy. native는 명시 close를 요구 |
| close | 명시 close는 실패·부분 cleanup을 보고. native session/JDBC connection close는 미commit 거래 rollback 요청이며 hidden commit 없음. 서버 cleanup 실패를 성공으로 만들지 않음 |
| Drop/최종화 | local 자원을 해제·연결을 닫아 서버의 기존 disconnect cleanup으로 인계. 네트워크 commit·blocking join·신규 allocation을 숨기지 않음. Java GC/Cleaner를 정상 close 증거로 쓰지 않음 |

Rust 동기 호출의 진행 owner가 nonblocking I/O·readiness를 구동한다. 취소 thread는 generation이 고정된 작은 control slot에 신호를 넣고 owner를 깨운다. Java도 Selector wakeup을 사용한다. exclusive operation guard는 소유권 표식이며 I/O 동안 cancel thread가 필요한 mutex를 잡아 두지 않는다. send/receive와 내부 lock을 잡은 채 cancellation 호출·사용자 callback을 기다리지 않는다. native `CancelHandle`은 특정 진행 호출 ticket을 대상으로 하며 여러 clone의 cancel은 한 wire Cancel로 합친다. JDBC/ODBC cancel은 해당 Statement의 **현재 public execute/fetch operation generation**을 짧은 lock 안에서 고정한다. idle result의 과거 Execute를 취소하지 않으며 idle cursor를 끝내려면 close한다.

복합 API는 하위 wire request가 바뀌어도 operation generation·cancel latch를 유지한다. BindAll 완료 직후처럼 wire request가 없는 구간의 cancel도 같은 API에 남아 다음 Execute 송신을 막는다. 이미 송신한 하위 request에만 wire Cancel을 연결하고 terminal/Ack 장벽을 지킨다. 하위 단계 완료가 cancel latch를 지우지 않으며 다음 단계 admission과 cancel을 같은 짧은 동기화 경계에서 판정한다. 최종 효과가 먼저 확정된 경쟁은 원래 outcome을 보존한다. 아직 Execute를 보내지 않았다면 execute=NotAttempted이고, 이미 끝난 binding·cursor close·commit 결과는 별도로 남긴다. 오래된 operation ticket은 다음 public 호출을 취소하지 않는다.

각 public execute/fetch/close에는 새 generation·deadline을 부여한다. query timeout은 한 execute의 local 변환·CloseResult·BindAll·Execute 전체에 적용하며 하위 요청마다 다시 시작하지 않는다. 각 wire timeout은 남은 예산과 서버 한도 이내로 내리고, 남은 시간이1ms 미만이면 다음 요청을 보내지 않는다. 같은 parked/partial-I/O 호출의 재개는 원래 deadline을 유지한다. 이전 execute timeout이 다음 public fetch를 취소하지 않는다. CancelAck=Requested는 신호 접수이고 rollback/ABORTED 증거가 아니다. 완료와 경쟁하면 원래 성공이 반환될 수 있다. user thread interrupt를 DB rollback으로 해석하지 않는다.

connect/handshake 기본 한도5초, ordinary는 DEV5 기본30초·명시1..30,000ms를 사용한다. JDBC queryTimeout=0/ODBC timeout=0은 driver 추가 deadline 없음이지만 **서버30초 한도는 유지**한다. 초 단위1..30은 wire ms로 checked 변환, 30초 초과 요청은 Unsupported로 거절하며 조용히 clamp하지 않는다. query timeout은 execute 및 이후 각 fetch에 독립 적용한다. local deadline이 먼저 오면 Cancel을 요청하고 추가5초 동안 terminal/Ack를 drain한다. 그 안에 정리되지 않으면 연결을 닫고 마지막 완결 outcome 또는 아래 UNKNOWN 규칙을 반환한다. 5초는 client 대기 예산이며 server undo/I/O 완료 SLA가 아니다. 읽기 중 disconnect 시 전달한 row prefix에 complete=true를 만들지 않는다.

송신하지 않은 local validation 오류는 NotAttempted다. mutation/commit 가능 요청에서 일부라도 전송했고 terminal을 받지 못하면 보수적으로 client certainty=Unknown이다. 부분 send만 보고 server 미실행을 추측하지 않는다. SELECT 실패는 query incomplete·08S01이며 기존 미commit 거래 cleanup 상태는 unknown일 수 있다. server의 durable Unknown/Fenced와 client 응답 유실은 별도 cause다. 자동 SQL 재전송·재접속 후 handle 복원·SELECT/PK로 과거 commit 추정은 금지한다.

<a id="native"></a>
## 4. Native Rust와 C facade

설계 API는 connect/close, execute_direct(expected kind), prepare(optional type hints), describe, bind_all, execute, fetch, close_result/statement, begin/commit/rollback, set_autocommit, cancel(ticket), diagnostics다. SQL 거래 동작은 DEV5 ExecuteDirect의 고정 control 문장을 사용한다. 사용자 SQL·parameter를 문자열로 합성하지 않는다. native set_autocommit은 DEV4대로 Active에서 TxnState이며 JDBC/ODBC의 API 조립과 구분한다.

Rust binding은 typed value vector 전체를 검사·quota 예약·복사하고 성공 시 원자 교체한다. 실패하면 기존 vector를 유지한다. 입력 slice는 반환 뒤 수정/해제할 수 있다. row/metadata는 소유 값이며 network buffer/page 주소를 빌려 주지 않는다. 반환 batch에 quota lease를 붙여 보유 중 bytes를 계속 회계한다. caller가 batch를 보유하면 다음 fetch가 quota로 거절될 수 있지만 이전 batch를 무효화하지 않는다. no-allocation 진단/control 여유를 먼저 예약한다.

C facade는 `xadb_*` 접두어와 명시 ABI version·fixed-width tag/ID·pointer+byte length를 사용한다. 연결/statement/result는 주소가 아닌 프로세스 registry의 kind+generation token이다. 0·다른 parent·stale generation은 거절하며 generation overflow시 slot을 폐기한다. data/metadata handle은 API의 전용 free만 허용한다. 호출자 buffer로 복사하는 함수는 required bytes와 NULL 여부를 별도 반환하며 부족하면 값/위치를 바꾸지 않는다. 성공한 반환 메모리를 libc free하거나 다른 library instance로 해제하지 않는다. C 상세 symbol/struct/offset·thread/alias 전제와 C11 header는 D6-F03에서 동결한다.

C input은 호출 동안 유효·정렬·읽기 가능, output은 충분한 쓰기 가능 영역이어야 한다. arbitrary invalid pointer의 복구를 보장하지 않는다. 길이/overflow/NULL 조합과 registry token은 접근 전에 검사한다. Rust enum/String/Vec·unwind는 ABI 밖으로 나가지 않는다. FFI는 별도 package의 좁은 unsafe 모듈로 격리하고 core의 `unsafe_code=forbid`를 유지한다. crate 전체 forbid를 `allow`로 무력화할 수 있다고 가정하지 않는다. 새 FFI package에 한해 deny 기본값과 감사한 함수별 예외를 D2로 검토하며 현재 workspace lint를 바꾸지 않는다. unwind를 잡으면 해당 연결을 재사용 금지로 만들고 고정 오류를 반환한다. allocator abort·잘못된 외부 포인터까지 복구한다고 주장하지 않는다.

<a id="values"></a>
## 5. 타입·binding·metadata

| XADB | JDBC 입출력 기본형 | ODBC SQL/C 기본형 |
| --- | --- | --- |
| Int64/BIGINT | Long, setLong/getLong; byte/short/int의 checked 확대·축소 | SQL_BIGINT / SQL_C_SBIGINT; signed 8/16/32-bit는 checked 변환 |
| Bool/BOOLEAN | Boolean, setBoolean/getBoolean | SQL_BIT / SQL_C_BIT; 0/1만 허용 |
| Utf8/VARCHAR | String, setString/getString; UTF-16↔UTF-8 strict 변환 | SQL_VARCHAR / SQL_C_CHAR(UTF-8), SQL_C_WCHAR(UTF-16) |
| Binary/VARBINARY | byte[], setBytes/getBytes; setter/getter 복사 | SQL_VARBINARY / SQL_C_BINARY |
| typed NULL | setNull(지원 type), getter의 null 또는 primitive 0/false+wasNull | SQL_NULL_DATA indicator, data buffer 내용과 독립 |

숫자↔text·bool↔숫자·binary↔text 암묵 변환은 초기 미지원이다. `getObject(Class)`/setObject는 위 wrapper·String·byte[]와 명시 지원 type만 허용하고 다른 조합은 명시 오류다. `setObject(null)`은 이미 동결된 parameter type의 NULL로 처리한다. numeric 축소 overflow는22003, text malformed surrogate/UTF-8은22021이다. String 길이는 UTF-16 char 수가 아닌 변환 후 UTF-8 bytes로 DEV4 bound를 검사한다. NUL·공백·빈 binary는 값이고 SQL 빈 text 입력의 NULL 정규화는 서버 SQL에서 한 번만 수행한다. native storage-origin의 non-NULL 빈 text를 fetch할 때 NULL로 바꾸지 않는다.

parameter는1-based ordinal·최대256개이며 동일 marker를 이름으로 묶지 않는다. JDBC setter는 local owned vector의 한 slot만 원자 갱신하고 clearParameters는 모두 Unbound로 만든다. Execute 전에 모든 slot을 확인하고 BindAll 성공 terminal 뒤 Execute를 순서대로 전송한다. BindAll 오류 뒤 이전 server binding으로 Execute를 계속하지 않는다. prepare시 타입이 결정되지 않는 `? IS NULL`은 standard prepare에서 오류이며 vendor typed-prepare 확장만 명시 hints를 공급한다. SQL 문자열 치환이나 deferred 타입 추측으로 우회하지 않는다.

Prepare/Describe/ResultSetMetaData는 immutable metadata를 사용한다. stale 뒤 기존 descriptor를 새 schema 값으로 바꾸지 않는다. JDBC label lookup은 ordinal1부터 Java String.equalsIgnoreCase의 locale-independent 비교로 처음 일치하는 열을 반환한다. exact match를 먼저 찾지 않으며 quoted label·non-ASCII도 같은 JDBC 비교를 적용한다. native name lookup은 exact match의 첫 ordinal이고 ODBC는 ordinal을 사용한다. duplicate/대소문자만 다른 label을 모호하지 않게 고르려면 ordinal을 사용한다. column count/type/nullability·byte bound·label/source name, parameter type/count/mode IN을 제공하고 byte limit를 문자 최대치로 단정하지 않는다. JDBC getPrecision(VARCHAR)은 안전한 문자 상한인 byte bound, vendor metadata에는 정확한 octet bound를 별도 노출한다. metadata의 unknown은 표준 unknown 값으로 표현한다.

<a id="transactions"></a>
## 6. JDBC·ODBC의 거래 API 조립

SQL/native의 Active+SET AUTOCOMMIT 거절과 DDL 두 outcome은 유지한다. **DEV6-DESIGN-001과 [REVIEW-002](#review-002)는 JDBC setAutoCommit 및 ODBC SQL_ATTR_AUTOCOMMIT에 아래 명시적 조립을 채택한다.** 이는 wire command의 의미 변경이나 server의 hidden commit이 아니다. Oracle/JDBC·ODBC의 전환 기대를 따르되 각 하위 결과를 보존한다. 연결 exclusive operation guard를 전체 조립 동안 유지하여 다른 ordinary 호출이 사이에 끼지 못한다. 각 하위 request는 별도 ID와 terminal/Ack 장벽을 사용한다.

mode가 실제로 달라지는 호출은 먼저 열린 result를 close하고 **그 terminal의 authoritative state로 Idle/Active 분기를 다시 선택**한다. ON의 implicit SELECT는 close 후 Idle이므로 COMMIT 없이 SET한다. OFF의 SELECT 또는 raw BEGIN의 명시 거래는 close 후에도 Active이므로 아래 API별 규칙을 적용한다. close 실패/Unknown/Fenced이면 COMMIT/SET을 보내지 않으며 성공한 close도 단계별 outcome에 남긴다. server EOF와 driver의 아직 읽지 않은 cached rows는 별도다. client가 보유한 result는 명시 close 또는 규정된 무효화까지 추적하며 commit/rollback·connection close·statement 재실행은 EOF terminal 뒤 남은 local row/column offset도 무효화한다. 단, ON implicit SELECT의 정상 EOF 해제 자체는 마지막 batch를 무효화하지 않는다.

아래 표의 명시 local no-op/진입 거절만 발생한 호출은 cached rows/offset을 보존한다. 실제 거래 종료/rollback cleanup, 이미 수행한 명시 result close, 연결 폐기/Broken/Fenced에 따른 무효화는 각각 관측한 단계 효과를 따른다. 예를 들어 ON의 JDBC commit/rollback 거절과 ODBC SQLEndTran no-op은 row를 폐기하지 않지만, commit 전 close가 완료된 뒤 commit에 실패했으면 닫은 result를 되살리지 않는다.

| API 입력 | 수행·실패 효과 |
| --- | --- |
| 현재와 같은 autocommit | server 명령 없이 no-op. Broken/Fenced는 성공으로 숨기지 않음 |
| result 정리 후 Idle, ON↔OFF | COMMIT 없이 SET. 성공 terminal 뒤만 local mode 변경. implicit SELECT의 close 전 상태로 COMMIT을 결정하지 않음 |
| OFF→ON, result 정리 후 Active | COMMIT→COMMITTED 확인→SET AUTOCOMMIT ON. abort/unknown/cleanup 실패면 SET 미시도. commit 후 SET 실패는 commit 보존·mode는 마지막 확정값, 유실이면 mode unknown/연결 폐기 |
| ON→OFF, result 정리 후에도 Active(raw BEGIN) | JDBC는 명시 COMMIT 후 SET OFF. ODBC는 거래 진행 중 OFF 전환을25000으로 거절하고 DML/기존 mode를 유지; 이미 수행한 cursor close는 보존. native/직접 SQL 의미와 API별 차이를 공개 |
| JDBC commit/rollback, mode ON |25000으로 진입 거절. mode OFF·Idle이면 local no-op, Active이면 아래 cursor 처리 뒤 wire control |
| ODBC SQLEndTran | DBC 범위만 지원. autocommit ON 또는 Idle이면 no-op; OFF·Active는 COMMIT/ROLLBACK. ENV 전체 거래 종료는 HYC00 |
| commit·rollback의 cursor | JDBC holdability=CLOSE_CURSORS_AT_COMMIT만 지원. commit 전 result 명시 close, rollback은 DEV4 전체 result 무효화와 client cached result 폐기를 함께 수행. ODBC SQL_CURSOR_COMMIT/ROLLBACK_BEHAVIOR=SQL_CB_CLOSE. EOF descriptor도 같은 수명 규칙을 적용. prepared plan은 유지하되 다음 실행 dependency 검사 |
| JDBC close/native close | 미commit 거래 rollback 요청, 실패는 원래 outcome·connection closed/recovery 미확인 보존 |
| ODBC SQLDisconnect | autocommit mode와 무관하게 Active이면25000, 연결·거래를 유지. OFF·Active는 SQLEndTran, ON+raw BEGIN의 Active는 raw COMMIT/ROLLBACK으로 먼저 종료한다(ON의 SQLEndTran은 no-op 유지). Idle이면 CloseSession. Broken은 local 해제 가능하나 rollback 성공을 주장하지 않음 |

raw SQL BEGIN/COMMIT/ROLLBACK/SET AUTOCOMMIT은 Any 경로로 기존 DEV4 의미 그대로 처리하고 authoritative reply로 local mode/state를 갱신한다. API 조립과 raw SQL이 동일한 숨은 변환을 한다고 주장하지 않는다. DDL은 native/모든 driver에서 prior_transaction_outcome·ddl_outcome을 별도로 보존한다. 실패한 DDL을 통째로 재시도하거나 이전 DML도 rollback됐다고 알리지 않는다. public isolation은 SERIALIZABLE만 제공하며 다른 isolation setter는0A000/HYC00이다.

<a id="jdbc"></a>
## 7. JDBC 필수 지원표

JAR은 Java SE21/JDBC4.3 표면으로 빌드하고 `java.sql.Driver` service registration·DriverManager 및 pool 없는 DataSource를 제공한다. URL은 `jdbc:xadb:unix`와 필수 properties `socket`, `database`, `principal`로 정한다. URL에 password·SQL·인증 비밀을 넣지 않는다. 명시 property 중복/알 수 없는 이름은 거절하고 Driver.acceptsURL은 다른 vendor URL을 false로 반환한다. UnixDomainSocketAddress/Selector는 JDK21 표준 API를 사용한다. Java runtime heap 전체 OOM 복구를 약속하지 않는다.

| 필수군 | 지원 동작·제한 |
| --- | --- |
| Connection | createStatement, prepareStatement, get/setAutoCommit, commit, rollback, close/isClosed, getMetaData, get/setTransactionIsolation(SERIALIZABLE), get/setHoldability(CLOSE), unwrap/isWrapperFor. readOnly=false만, true는 미지원. 고정 catalog DB·schema PUBLIC 조회, 같은 값 setter no-op·다른 값 거절 |
| Statement | execute/executeQuery/executeUpdate/executeLargeUpdate, getResultSet/getUpdateCount/getLargeUpdateCount, getMoreResults(CLOSE_CURRENT_RESULT), close/isClosed, getConnection, cancel, queryTimeout, fetchSize, warnings. Rows/Update 기대 종류는 D6-W01로 효과 전 검증 |
| PreparedStatement | 위 실행·수명 + 지원 setter/clearParameters, ResultSetMetaData/ParameterMetaData. SQL 인자가 있는 inherited execute 계열은 거절. prepare는 SELECT/DML만 |
| ResultSet | next, getLong/Int/Short/Byte/Boolean/String/Bytes/Object, wasNull, findColumn, getMetaData/getStatement, close/isClosed. forward-only/read-only, getter는 현재 row가 있을 때만. closed/EOF getter 오류 |
| metadata | driver/product/API version·실제 capability·최대값·지원 type/isolation/cursor/transaction/SQL 기능 flag, column/parameter descriptor. getTables/getColumns/getPrimaryKeys/getIndexInfo 등 catalog 열거는 초기0A000이며 가짜 빈 결과로 대체하지 않음 |
| 제한 옵션 | fetchSize=0은 기본32행,1..128은 hint·실제 byte cap 우선, 범위 밖 거절. maxRows=0만, nonzero·scroll/updatable/hold-over-commit·multiple results·savepoint·batch·LOB·generated keys·CallableStatement·pool/XA·SQL escape 변환은0A000 |

모든 JDK21 대상 interface의 overload를 D6-F02에서 method signature별 Supported/Unsupported/LocalNoOp/Conditional로 전개하고 누락된 method가 default 성공을 반환하지 않도록 검사한다. 위 지원표가 전체 Java interface reflection inventory의 실행 증거는 아니다. `jdbcCompliant()`는 false다. unsupported는 SQLFeatureNotSupportedException, local 입력 오류·overflow는 적절한 SQLException이며 기본 Java UnsupportedOperationException을 DB API 밖으로 흘리지 않는다.

Statement의 현재 결과는 `None | Rows | Update(count)`로 관리한다. execute는 Rows일 때만 true, getResultSet은 Rows 외에는 null, getUpdateCount/getLargeUpdateCount는 Update 외에는 -1이다. 단일 결과만 지원해도 getMoreResults()/getMoreResults(CLOSE_CURRENT_RESULT)는 현재 ResultSet을 닫고 성공 시 None으로 전이한 뒤 false를 반환한다. 이전 update count를 남겨 표준 결과 순회가 끝나지 않게 하지 않는다. close 실패는 SQLException이며 정상 소진으로 보고하지 않는다. 정상 소진 뒤 반복 호출은 false/-1을 유지한다. raw 거래 제어처럼 결과가 없는 성공과 DDL의0 update count는 F02에서 서로 구별한다.

query 결과는 첫 row 전/row/EOF/closed/failed 상태를 갖는다. executeQuery 성공만으로 전체 query 완료가 아니며 next/fetch 오류는 SQLException이다. cached batch의 row를 반환해도 close/rollback/새 execute 뒤 무효화된 ResultSet을 되살리지 않는다. Connection.close가 active ordinary와 경쟁하면 Busy이며 강제 중단은 명시 cancel·local abort 경로를 구분한다. `abort(Executor)`는 executor에 연결 폐기를 요청해 진행 호출을 깨우고 모든 handle을 무효화하며 server rollback 완료를 기다렸다고 주장하지 않는다. executor 거절 시 연결 상태와 실패를 보존한다.

<a id="odbc"></a>
## 8. ODBC 지원표·포인터·부분 buffer

첫 ABI는 unixODBC·LP64, sizeof(SQLLEN/SQLULEN)=8, SQLWCHAR=2 bytes다. 정확한 header/configuration을 ABI probe로 확인하고 맞지 않으면 build/인수를 거절한다. iODBC·32-bit·ODBC2는 별도다. ANSI entrypoint/SQL_C_CHAR는 UTF-8, W entrypoint/SQL_C_WCHAR는 strict UTF-16으로 정의한다. ODBC 표준별 length 단위(특히 W 문자열의 chars와 buffer bytes)를 함수별 표로 D6-F04에서 동결하며 일괄 byte 환산하지 않는다.

| 필수군 | 기능·제한 |
| --- | --- |
| 환경/접속 | SQLAllocHandle/SQLFreeHandle ENV/DBC/STMT, SQLSet/GetEnvAttr(ODBC3/3.8), SQLDriverConnect NOPROMPT 및 SQLConnect DSN, SQLDisconnect, SQLSet/GetConnectAttr(autocommit/isolation), DBC SQLEndTran |
| 실행/결과 | SQLPrepare, SQLExecDirect, SQLExecute, SQLBindParameter(INPUT), SQLNumParams/DescribeParam, SQLNumResultCols/DescribeCol/ColAttribute, SQLFetch, SQLBindCol, SQLGetData, SQLRowCount, SQLMoreResults, SQLCloseCursor, SQLFreeStmt(CLOSE/UNBIND/RESET_PARAMS), SQLCancel/SQLCancelHandle(STMT) |
| 진단/탐지 | SQLGetDiagRec/SQLGetDiagField, SQLGetInfo, SQLGetFunctions, SQLGetTypeInfo, SQLGet/SetStmtAttr 및 implicit descriptor의 SQLGetDescField. getInfo의 flag/지원 함수 bitmap은 실제 matrix와 일치. DBC/ENV/statement 관련 attrs의 get/set와 자동 descriptor 최소 필드 포함 |
| 제약 | forward-only/read-only, row array size=1·paramset size=1. SQLFetchScroll은 NEXT만. explicit descriptor allocation/copy, SQLSetPos/Bulk/array/SQLParamData/PutData·output parameter·async·bookmark·catalog 열거·pool/XA는HYC00/IM001로 거절 |

DSN/connection string의 필수값은 socket/database/principal이며 credential UID는 OS가 검증한다. DRIVER/DSN은 driver manager가 해석하고 password/remote options는 성공한 인증처럼 받지 않는다. grammar·duplicate/escape·W variants는 F04에서 동결한다. SQLDisconnect 전에 statement를 해제하는 정상 순서를 예제로 제공하며 active child/거래가 있는 parent SQLFreeHandle은 HY010/25000으로 거절한다. cancel 실행 중 handle free는 Busy이고 control ticket 수명이 끝난 뒤 재사용한다. ODBC handle pointer는 registry membership 확인 후만 역참조하고 외부 invalid pointer 복구를 일반 보장하지 않는다.

**ODBC binding은 native/JDBC의 setter 복사와 다르다.** SQLBindParameter는 caller buffer/type/length/indicator descriptor를 등록한다. buffer와 indicator는 rebind/reset/handle 해제까지 유효해야 하고, 호출자가 값은 SQLExecute 사이에 바꿀 수 있다. 각 Execute가 값을 읽어 검증·복사한 뒤 하나의 BindAll vector로 만든다. 그 호출 동안 caller의 동시 변경은 금지한다. 등록 때 값만 복사하면 반복 Execute에서 바뀐 값을 놓치므로 금지한다. 실패한 local 변환/BindAll 뒤 Execute는 보내지 않는다. column binding도 SQLFetch가 caller 영역에 쓰므로 rebind·SQLFreeStmt(SQL_UNBIND)·statement SQLFreeHandle까지 buffer/indicator의 수명과 경쟁 전제를 유지한다. SQLCloseCursor와 SQLFreeStmt(SQL_CLOSE)는 cursor만 닫고 parameter/column binding을 해제하지 않는다. 다음 Execute/Fetch는 등록된 기존 buffer를 다시 사용한다. parameter 등록 해제는 rebind·SQL_RESET_PARAMS·statement free이며 cursor close가 아니다.

SQL_NTS는 해당 API가 허용하는 text에만 사용하며 최대 선언 한도 내 종결 여부를 확인한다. 명시 byte length는 내부 NUL을 보존한다. SQL_NULL_DATA와 길이0을 구분하고 NULL output에 indicator가 없으면22002다. 고정 C numeric output은 ABI가 요구하는 크기의 유효 pointer를 caller가 제공하며 variable BufferLength 규칙으로 안전성이 보장된다고 하지 않는다.

SQLFetch의 bound text/binary 부족은 row를 소비하고 SQL_SUCCESS_WITH_INFO·01004·전체 원래 길이를 보고한다. SQLGetData는 현재 row에서 **최상위 bound column ordinal보다 큰 unbound column만 증가 ordinal 순서**로 읽는다(SQL_GETDATA_EXTENSIONS=0). bound column·최상위 bound 이하·이전에 읽은 열보다 작은 ordinal은07009이며 row/offset을 바꾸지 않는다. 같은 가변 column의 반복 호출로 offset을 전진시키며 각 호출 전 target C type으로 변환된 남은 bytes(UTF-16이면 UTF-16 bytes)를 indicator로 보고, 잘림은01004, 마지막 chunk는SUCCESS, 이후는NO_DATA다. CHAR/WCHAR는 terminator 공간을 포함해 검사하되 indicator는 terminator를 제외한다. NULL은SQL_NULL_DATA, non-NULL 빈 Binary는길이0이며 첫 호출SUCCESS·다음NO_DATA다. 빈 text는 terminator를 담는 첫 호출이SUCCESS·다음NO_DATA이며 terminator 공간도 없으면01004·미소진 상태를 유지한다. text는 UTF-8 code point/UTF-16 surrogate pair 중간에서 잘라 유효하지 않은 결과를 만들지 않는다. 다음 한 문자를 담을 공간이 없으면01004·진행0이므로 caller가 buffer를 늘린다. next row/close는 offset을 초기화한다. native RowBufferTooSmall의 위치 보존과 ODBC의 이미 소비한 row/column truncation을 혼동하지 않는다.

SQLGetData의 BufferLength=0 길이 조회·위치 유지는 **가변 C type의 non-NULL 미소진 값에만** 적용한다. 남은 데이터 또는 text terminator를 담지 못하면01004이며, NULL과 빈 Binary는 위 첫 호출의 정상 완료 규칙을 따른다. SQL_C_SBIGINT 등 고정 C type은 nonnegative BufferLength를 무시하고 실제 type 크기만큼 한 번 복사하므로0도 길이 조회가 아니다. caller는 충분한 유효 영역을 제공하고 다음 호출은NO_DATA다. 음수 BufferLength는HY090, TargetValuePtr=NULL은 길이0이어도HY009로 쓰기 전 거절하며 위치를 유지한다. length probe를 핑계로 NULL 포인터를 허용하거나 고정 크기 출력을 가변 buffer 규칙으로 구현하지 않는다.

진단은 handle별 snapshot이며 다음 해당 handle의 non-diagnostic 호출이 교체한다. SQLGetDiagRec/Field 반복은 지우지 않는다. record1은 표준 SQLSTATE·native numeric code·짧은 message, 추가 structured outcome은 vendor diagnostic field/record로 노출한다. error chain의 DDL prior/ddl·API mode-change commit/setting outcome은 고정 필드로 보존하고 message substring parsing을 요구하지 않는다. record/총 bytes 상한과 truncation 표시는 별도다.

<a id="errors"></a>
## 9. 오류·확실성 매핑

공통 typed diagnostic은 stable XADB code, SQLSTATE, phase/span, statement_effect, transaction_state, certainty, retry_scope, engine cause, 실제 optional commit identity, DDL 두 outcome, API 조립의 단계별 outcome을 보존한다. 메시지 localization으로 의미를 판정하지 않는다. JDBC는 SQLException subclass와 vendor immutable outcome accessor/nextException, C는 typed struct getter, ODBC는 위 vendor diagnostic fields를 사용한다. exact numeric code·전체 lower code inventory는 F02에서 충돌 없이 동결한다.

| 원인 | SQLSTATE 기본값·추가 규칙 |
| --- | --- |
| Syntax/Name/권한 |42000 / 42S02(table)·42S22(column) 또는42000 / 42501. 분류 정보가 없으면 가짜 세부 원인을 만들지 않음 |
| Unsupported | JDBC/native0A000; ODBC optional HYC00, 함수 미지원IM001 |
| bind 개수/ordinal/type |07001 / 07009 / 07006. parameter 타입 미결정07006 |
| numeric range/길이/encoding |22003 / 22001 / 22021; 서버 row/key/quota 한도는 별도 원인 |
| NOT NULL/UNIQUE |23000 + stable code로 구별; JDBC SQLIntegrityConstraintViolationException |
| TxnState/result busy/stale handle·plan |25000 / HY010 / HY010. ODBC cursor 위치 오류24000·invalid handle은SQL_INVALID_HANDLE 규약 |
| serialization conflict | 전체 abort 완료 확인 때40001; undo 미완료면 이 retryable 분류를 사용하지 않음 |
| cancel/timeout | JDBC/native57014 또는HYT00, ODBC HY008 또는HYT00. 확정 COMMITTED 우선, Unknown은 아래 우선 |
| connect/auth/closed |08001 / 28000 / 08003 |
| 연결 유실·protocol |08S01; mutation/commit 미확정이면08007과 certainty Unknown. 연결 Broken, 자동 재시도 없음 |
| server Unknown·RecoveryRequired | Unknown은08007, 확정 NotCommitted+cleanup 미완료는HY000·Fenced. 둘을 ABORTED로 합치지 않음 |
| quota/allocation·기타 engine |HY001 또는HY000 + 정확한 stable cause/effect; Java VM OOM 복구 약속 아님 |

로컬 timeout·cancel code가 이미 받은 commit/DDL outcome보다 우선하지 않는다. `isTransient`/JDBC exception class나 SQLSTATE40001은 client의 자동 재실행 허가가 아니며 caller가 전체 거래 범위와 외부 효과를 판단한다. Fenced/Broken에서는 새 업무 호출을 거절하고 진단·local 해제만 허용한다. 손상된 reply 안의 일부 commit field만 보고 ACK로 기록하지 않는다.

<a id="resources"></a>
## 10. 자원·계측·패키징

초기 driver 소유 quota는 연결당64 MiB, 프로세스 runtime당256 MiB·최대32연결, control/진단 reserve는 연결당64 KiB(전체 quota에 포함)다. JVM 전체 heap·caller가 복사한 값·ODBC application buffer는 별도이며 driver 소유와 RSS를 분리 관측한다. 각각 allocation capacity, retained binding, UTF 변환 중 양쪽 bytes, frame/reassembly, row batch와 held lease, handle/diagnostic/tombstone을 센다. 최소 8 MiB retained bindings+8 MiB replacement+8 MiB request body+2 MiB text 변환+1 MiB frame staging+2 MiB result/metadata+64 KiB control의 중첩을 예산에 포함한다. 이는 구현별 최악 계산의 출발값이며 F03/04에서 숨은 복사/Java object overhead를 포함한 실제 상한을 증명하기 전 기본 한도 충족을 주장하지 않는다. 부족하면 효과 전 local Limit으로 거절한다.

server의1 MiB SQL·8 MiB logical request·64 prepared/8 MiB prepared state·256 parameter·256 KiB canonical fetch/512 KiB serialized reply·128 rows 상한을 각각 검사한다. 32개 연결이 서버32slot을 모두 쓸 수 있다는 예약 보장은 없다. fetch 기본32행·one batch이며 unbounded prefetch/cache·암묵 전체 결과 materialization을 하지 않는다. client buffer가 cap을 넘으면 다음 fetch를 멈추고 cleanup용 reserve는 보존한다. 안정적인 무제한 처리량·임의 수의 descriptor를 약속하지 않는다.

계측은 API→encode/copy→TX/RX 대기→decode/변환→caller 반환 및 server request ID를 연결한다. successful/attempt TPS·row/bytes·allocation/copy·retained bytes·quota·cancel/cleanup·UNKNOWN·trace loss를 기록한다. client/server clock을 직접 빼지 않는다. SQL·bind·password 원문은 기본 trace에서 제외한다. 독립 정확성·비교·replay 의무와 수치 profile은 [계획](../plans/dev6-implementation-plan.md#performance)이 소유한다.

Rust rlib/C library+header/JDBC JAR+service descriptor/ODBC shared library와 DSN 예제·symbol·notice·checksum은 실제 capability 구현 후 생성한다. JDBC 빌드는 Maven의 정확한 distribution/plugin/dependency 버전을 고정하고 runtime dependency는 JDK를 기본으로 한다. ODBC driver manager를 제품에 임의 번들하지 않고 시험 조합을 명시한다. 새 clients root·FFI/OS 조립 package가 생길 때 RB05의 source fingerprint·clean export·build identity를 같은 변경에서 확장한다. 개발 build는 fresh fixtures·동일 wire profile이며 안정 릴리스부터 모든 이전 same-major client→새 server 계약을 별도 인수한다.

<a id="decision"></a>
## 11. DEV6-DESIGN-001 결정 기록

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-19 |
| 담당·권한 | 작업·client 계약·통합 `/root`; 사용자의 DEV6 설계 및 Oracle·로컬 DBMS 참고 요청, DP01 위임 |
| 선택 | 좁은 native core·C/ODBC FFI 경계·순수 Java, bounded 동기 API·generation 취소, typed owned binding, 표준별 buffer 의미, 명시 거래 API 조립·outcome 보존 |
| 대안·이유 | JDBC JNI 공용 core는 배포/FFI 비용; driver SQL 합성·server private import는 의미 경계 훼손; 자동 retry/pool/pipeline은 초기 UNKNOWN·자원 상태 확대. native 모드 규칙을 JDBC에도 그대로 적용하는 대안보다 명시 commit+SET 조립을 선택 |
| 공급 영향 | D6-W01의 metadata/outcome/expected-result를 DEV5 F01에 연결. native SQL·DDL/engine commit·안정 호환·단계 gate는 유지. DEV5의 adapter 의미를 조용히 바꾸지 않는 원칙을 명시적 driver 조립으로 구체화 |
| 조사·검증 | [실제 열람·identity](../evidence/dev6-source-research.md). 코드·외부 dependency 도입 없음. 제품·성능·모델 실행 NOT_RUN, 문서 자동 검사는 push 전용 |
| 독립 검토 | `/root/dev6_design_review`가 기본 설계·DEV5 공급·공유 색인과 병렬 계획을 정적 독립 검토. 최초 P2 4건 및 병렬 의존 그림 P2 1건 수정 확인, 잔여 P1/P2 없음. 계약·통합 담당 `/root` 반영 확인. [실제 검토 기록](../evidence/dev6-source-research.md#review) |
| 대체 관계 | DEV6 최초 상세 설계이며 대체한 DEV6 결정 없음. DEV5 수신 경계는 D6-W01로 구체화하고 기존 단계·내구성·SQL/native 계약은 유지 |
| 재검토 | 실제 DEV5 공급 불충족, API/SQL 의미 불일치·응답 유실, quota/FFI 수명 위반, JDK/DM/OS 확대, DEV7 type·batch/LOB·remote/pool 또는 측정 기반 최적화 |

DESIGN-001의 최초 검토 기록은 당시 범위로 보존한다. 아래 REVIEW-002가 request 발급·API 조립·모드 전환·결과/버퍼 경계의 해당 문구를 보완·대체한다.

<a id="review-002"></a>
## 12. DEV6-REVIEW-002 — 클라이언트 상태 경계 보완

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-19 |
| 담당·권한 | 작업·client 계약·통합 `/root`; 사용자의 Oracle·ARIES·로컬 구현 기반 리뷰 및 발견 문제 즉시 수정 요청, DP01 위임 |
| 반례 | local 실패 뒤 wire ID gap; BindAll/Execute 사이 취소 유실·호출 혼입; implicit SELECT close 뒤 불필요한 COMMIT; EOF cached rows의 무효화 누락; JDBC 결과 소진 뒤 count 잔류; ODBC 고정 C type에 길이0 probe 오적용 |
| 선택 | 연속 wire ID의 예약/송신 수락 분리, public operation 단위 guard/cancel/deadline, close terminal 뒤 거래 상태 재판정, client 결과 수명·소진 전이, SQLGetData 가변/고정형 분리 |
| 대안·이유 | 서버 ID gap 허용은 기존 wire 순서 계약을 불필요하게 변경. 요청별 guard만으로 복합 API 취소를 보장할 수 없음. client의 SQL prefix/거래 종류 추측 대신 기존 authoritative close reply 사용. ODBC의 모든 길이0을 probe로 통일하면 고정형 ABI와 충돌 |
| 범위·대체 | DESIGN-001의 해당 경계만 대체. DEV4/5/COMM wire·SQL 의미, 내구성·기능 범위·단계 gate 유지. 새로운 package/코드/형식 없음 |
| 근거·검증 | [이번 실제 조사·identity·검토](../evidence/dev6-source-research.md#review-002), [R2-V01–V06](../plans/dev6-implementation-plan.md#tests), 기존 T01–T18/C02/M01–M03에 인수 의무 연결. 제품·모델·FFI 시험 NOT_RUN |
| 독립 검토 | 별도 실행 `/root/dev6_boundary_review`가 기준 사본 대비 diff·DEV4/5/COMM 계약·공식 Java/ODBC 원문·반례를 읽기 전용 검토. 빈 text terminator P2 및 no-op/진입 거절의 row 보존 보완 확인, 검토 범위의 잔여 P1/P2 없음·정적 채택 가능. 계약·통합 담당 `/root` 반영 확인. 제품 실행/소스 hash 재검증이 아님 |
| 재검토 | F01–F04 동결 시 구현이 위 전이를 표현하지 못함, 새 API/type/transport, 취소·모드·cursor·buffer 반례 재현, 기준/fixture 변경 |

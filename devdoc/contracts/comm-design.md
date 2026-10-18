# COMM — 독립 통신 계층 상세 설계

2026-10-18 · **D2 / ACCEPTED · 독립 검토 반영 / 미구현·제품 시험 NOT_RUN**

[구현·검증 계획](../plans/comm-implementation-plan.md) · [소스 조사](../evidence/comm-source-research.md) · [단계 완료 계약](../plans/stage-completion.json) · [저장소 배치](../policies/repository-build-plan.md)

<a id="scope"></a>
## 1. 목적과 소유 범위

COMM은 DEV1 이후 engine·SQL·server 없이 구현하는 bounded byte transport다. `XADB-031 / C1 / KU12`를 구체화하며 DEV2–DEV4 완료를 선행 조건으로 추가하지 않는다. 현재 존재하는 것은 foundation의 완결 프레임 codec·quota·cooperative cancellation이며 streaming decoder, socket adapter, readiness wait와 COMM runner는 없다.

| 범위 | 이번 소유 계약 | 후속 소유자와 의무 |
| --- | --- | --- |
| NET-01 | AXF1 stream framing, 길이·checksum·부분 I/O·순서 | 업무 message kind·request ID는 DEV5 |
| NET-02 | Linux TCP loopback·소유 Unix pathname socket, connect/accept/close | 원격 배포·client OS 지원표는 DEV5/DEV6 |
| NET-06 | 송수신 quota, read/write throttle, deadline, 공정한 진행 | SQL 결과 생산 중단·cursor/snapshot 해제는 DEV5 |
| NET-03/04/05/07 | opaque payload의 전달·실패 정보와 연결 취소 경계 | 인증·capability handshake·execute/fetch·LOB chunk·요청 취소·결과 불명 해소는 DEV5/DEV6 |
| NET-08/SEC-04 | 암호화 adapter를 넣을 경계를 보존 | TLS 인증서 검증·교체·인증과 실제 원격 시험은 후속 필수 의무 |

C1은 localhost와 시험이 소유한 Unix socket만 허용한다. wildcard·비-loopback 주소·DNS·외부 서비스 연결은 이 capability에서 거절한다. loopback 자체를 사용자 인증으로 간주하지 않는다. TLS나 인증을 지원한다고 협상·광고하지 않으며 원격 노출은 해당 보안 계약과 인수를 갖춘 뒤 연다. 기능 목록의 후속 NET 항목을 C1 완료만으로 완료/N/A 처리하지 않는다.

<a id="architecture"></a>
## 2. 책임과 의존 방향

```text
시험 조립부 / 향후 server·client 조립부
  ├─ transport: frame stream, connection state, admission, queue, deadlines
  │    └─ foundation: AXF1 codec, quota, cancellation, clock, byte-I/O ports
  └─ platform: Linux socket, readiness wait, monotonic clock
       └─ foundation
```

`xadb-transport`의 normal workspace dependency는 `xadb-foundation` 하나다. engine·SQL·platform·시험 protocol을 import하지 않는다. `xadb-platform`은 foundation의 작은 effect port를 구현하고 transport를 import하지 않는다. 조립부가 구현 객체와 설정을 전달한다. 실제 native 통합 시험은 조립부에서 두 라이브러리를 연결하며 production 역참조를 추가하지 않는다.

한 연결의 socket·RX decoder·현재 TX offset·deadline 진행은 한 driver owner만 변경한다. 외부 producer와 driver가 공유하는 **TX admission queue만** 짧은 admission mutex 아래 변경한다. queue는 4개 slot을 미리 확보하며 `try_send`의 enqueue와 `begin_drain/cancel`의 admission 차단을 같은 mutex로 직렬화한다. 일반 payload mailbox를 별도로 두지 않는다. 읽기/쓰기 별 task가 connection state를 임의 변경하거나 socket clone으로 우회하지 않는다. admission/lifecycle/quota lock을 보유한 채 socket I/O·wait·사용자 callback·checksum·allocation을 수행하지 않는다. enqueue timestamp를 위한 nonblocking 단조 clock 읽기만 admission 임계 구역에서 허용한다.

초기 Linux driver는 nonblocking socket과 level-triggered readiness, bounded round-robin 순회를 쓴다. 최대 128개 연결에서는 단순 poll 방식부터 구현한다. readiness는 진행 가능성의 힌트이며 실제 read/write의 `WouldBlock`이 정상이다. ready list만 고집하지 않고 TX admission queue·종료 신호·deadline도 매 순회 확인한다. writable interest는 TX가 있을 때만 켠다. read interest를 끄더라도 취소·deadline·TX 진행은 계속한다.

기존 `nix`의 safe wrapper와 표준 라이브러리를 먼저 검토한다. socket/poll 관련 feature 추가는 실제 API·MSRV·전이 의존·license/security 확인과 DP04 리뷰를 거쳐 구현한다. 이번 문서는 dependency를 추가·승인한 기록이 아니다. `unsafe_code = forbid`를 유지한다. Tokio·io_uring·edge-triggered epoll·다중 reactor·zero-copy는 필요성과 측정 없이 도입하지 않는다.

<a id="frame"></a>
## 3. 바이트 형식과 streaming decoder

[현재 codec](../../crates/foundation/src/codec.rs)의 AXF1을 그대로 사용한다. 개발 build의 형식 ID이며 WAL 형식이나 공개 안정 wire 호환 약속이 아니다. [버전 정책](../policies/version-compatibility-policy.json)을 따른다.

| offset | bytes | 내용 |
| --- | --- | --- |
| 0 | 4 | ASCII `AXF1` (`41 58 46 31`) |
| 4 | 2 | unsigned little-endian format = 1 |
| 6 | 4 | unsigned little-endian payload length `L` |
| 10 | L | opaque payload, 0–1,048,576 bytes |
| 10+L | 32 | SHA-256(header + payload) |

총 wire bytes는 `L+42`, 최대 1,048,618이다. 빈 payload도 유효한 42-byte 프레임이다. 압축·application message 분할/재조립·request multiplexing·frame 내부 의미 해석은 없다. TCP의 분절과 합쳐진 read는 application fragmentation과 별개다. 한 read가 여러 프레임을 주더라도 순서대로 각각 반환하며 trailing bytes를 이전 프레임의 일부로 해석하지 않는다.

상태는 `Header(filled 0..10) → Body(filled 10..L+42) → Verified → Header(0)`다. header는 고정 배열로 읽는다. magic·format·L과 checked `L+42`를 검증한 뒤에만 quota 예약과 fallible allocation을 한다. 잘못된 header로 body allocation이나 attacker 지정 길이만큼의 discard를 하지 않는다. 완결된 bytes에 foundation `decode_frame` 검증이 성공해야만 payload를 공개한다. checksum은 손상 검출이며 peer 인증이 아니다.

foundation에 header 길이·전체 길이 검증을 공유하는 작은 API를 추가하여 transport와 완결 codec이 magic/상한을 따로 구현하지 않도록 한다. 정확한 함수 signature는 C0에서 동결하되 AXF1 bytes와 기존 encode/decode 의미는 유지한다. 이 변경에는 DEV1 회귀와 별도 D2 리뷰가 필요하다.

RX는 다음 부족한 부분까지만 읽어 이미 읽은 뒤 처리하지 못하는 대형 spill buffer를 만들지 않는다. header/body의 성공한 byte 수만 cursor를 전진한다. `Interrupted`는 syscall 예산·취소·deadline 재확인 후 재시도한다. `WouldBlock`은 상태를 보존하고 scheduler에 반환한다. nonempty buffer에 대한 `read=0`은 EOF다. 부분 header/body EOF는 `TruncatedFrame`으로 닫는다. checksum·version·길이 오류 뒤 같은 stream을 재동기화하지 않는다.

검증된 `ReceivedFrame`은 backing buffer와 quota lease를 함께 소유하며 payload는 그 안의 slice로 빌린다. decoder buffer를 빌린 채 다음 read가 덮어쓰게 하지 않는다. 복사·clone은 추가 예약 없이 허용하지 않는다. 수신 순서는 byte stream 순서이며 delivery event는 정확히 한 번 발생한다. 이것은 업무 요청의 exactly-once 실행 보장이 아니다.

<a id="ports"></a>
## 4. API와 effect port의 계약

아래 이름은 구현할 책임이며 현재 Rust API가 아니다. 공통 port는 foundation, 연결 의미·오류·frame lease는 transport, native handle은 platform이 소유한다.

| 경계 | 계약 |
| --- | --- |
| `ByteStream::try_read/try_write` | nonblocking, short I/O 허용, 0..buffer length만 반환. 성공한 prefix만 유효. 오류에서 숨은 prefix 진행을 허용하지 않으며 socket adapter가 만족하지 못하면 연결 종료·송신 결과 불명 |
| `ByteStream::close` | 소유 handle을 닫는 명시 동작. Drop은 최종 handle 반환만 하며 flush/drain하지 않음 |
| `Connector` / `Listener` | numeric loopback 또는 소유 Unix endpoint, nonblocking connect/accept. connect ready 뒤 실제 socket error를 확인해야 성공 |
| `ReadinessWait` | bounded endpoint 목록·interest와 상대 대기 상한. stale event는 `(slot,generation)` 검증 후 폐기; raw fd 재사용을 연결 재사용으로 해석하지 않음 |
| `ConnectionSender::try_send(payload)` | quota·slot 예약 후 lock 밖에서 AXF1을 만들고 admission mutex 안에서 Open 재확인·FIFO enqueue·timestamp/ticket 부여. 이 enqueue가 승인 선형화점. 실패하면 queue·송신 효과 없음. 공간 대기 없이 거절하며 짧은 mutex 대기는 가능 |
| `Connection::advance(ready, now)` | 한정한 byte/frame/syscall 예산 안에서 RX/TX 진행·deadline 확인. blocking I/O·공간 대기·unbounded loop 없음; 짧은 admission mutex 경합은 허용. 단일 owner만 호출 |
| `Connection::take_received()` | 다음 검증된 frame lease를 소유권 이전. queue에서 꺼내도 outstanding RX charge는 유지 |
| `Connection::begin_drain()` | 새 송수신 admission을 닫고 이미 accepted TX만 정해진 기한까지 송신 |
| `Connection::cancel()` | local 연결 전체 취소. 부분 송수신이 있으면 폐기 후 close, stream 재사용 금지. 업무 request cancel을 대신하지 않음 |

readiness wait port의 관심 대상은 opaque registration token이다. fd·OS flag·nix 타입은 platform 밖의 공개 API에 노출하지 않는다. transport는 `Clock`의 단조 시간과 `Cancellation` 관측을 사용한다. 기존 `Cancellation::check()`는 syscall을 중단하지 않으므로 wait 상한과 매 순회의 확인을 함께 적용한다. 오래된 sender handle은 generation 불일치나 terminal admission 상태로 거절한다. generation/ticket overflow는 재사용하지 않고 해당 handle을 닫는다.

TX는 현재 frame의 offset을 유지하며 일부를 쓰고 난 뒤 반드시 나머지를 먼저 보낸다. concurrent producer 간 FIFO 순서는 admission mutex 아래 enqueue를 확정한 순서다. driver가 아직 queue에서 꺼내지 않았어도 성공 ticket이 반환된 frame은 accepted TX이며 30초 deadline과 drain 대상에 포함한다. frame 준비 중 admission이 닫히면 재확인에서 거절하고 임시 예약을 반환한다. `write=0`은 `WriteZero`로 닫는다. frame 중간에 다음 frame을 끼워 넣거나 timeout 뒤 남은 bytes를 버리고 같은 stream에서 다음 frame을 보내지 않는다. 소비자가 sender 입력을 수정해도 accepted frame은 변하지 않는다.

각 accepted ticket은 `FlushedLocally` 또는 `Failed(kind, written_bytes)`라는 terminal 결과를 정확히 한 번 저장한다. 결과 cell은 미리 확보한 TX slot 안에 있으며 완료돼도 소비자가 결과를 회수하거나 ticket을 drop할 때까지 slot을 유지한다. 진행 중 ticket drop은 암묵 취소가 아니며 frame이 terminal이 된 뒤 slot을 반환한다. 모든 pending/queued/current ticket은 close 시 실제 byte 진행량과 함께 실패로 해결한다. 이미 확정한 local-flush 결과는 연결 종료로 rollback하지 않는다. driver 밖으로 전달한 RX lease·sender·ticket·준비 예약이 남아 있는 Closed 연결은 자원 slot을 보존하여 무제한 terminal metadata를 만들지 않는다.

<a id="limits"></a>
## 5. 메모리·queue·공정성

다음 값은 C1의 초기 동결 프로파일이며 고객 SLA·최대 제품 용량의 실측값이 아니다. 구현 중 변경이 필요하면 실행 전에 D2 기록을 갱신한다. 아래 `M=1,048,618`은 최대 wire frame 크기다.

| 항목 | C1 값 | 계산 범위 |
| --- | --- | --- |
| 연결 수 | 128 | connecting·accepted·draining 및 외부 lease/sender/ticket/준비 예약을 보유한 Closed 연결 포함; listener 별도 |
| listener 수 | 3 | IPv4 loopback, IPv6 loopback, Unix pathname |
| RX bytes / frame slots | 연결당 `2M` / 2 | 부분 body + 검증 queue + 외부가 보유한 lease |
| TX bytes / frame slots | 연결당 `4M` / 4 | encoding 중 예약 + admission queue/current frame; 미회수 terminal ticket도 slot 점유 |
| process payload quota | 67,108,864 bytes (64 MiB) | 모든 RX/TX backing allocation의 예약 합계 |
| control 예산 | 연결당 4,096 bytes + driver 전체 65,536 bytes | 고정 header·queue/ticket metadata·timer·event metadata; 구현 시 실제 layout이 이 안인지 확인 |
| 한 연결의 한 방향/순회 | 65,536 bytes 또는 8 completed frames 또는 32 syscall attempts 중 먼저 도달 | zero progress·Interrupted도 attempt에 포함; RX/TX 모두 진행 기회 보장 |
| accept 처리 | 각 순회 accept 8회 | listener flood가 기존 연결·timer를 독점하지 않음; TX queue는 연결별 예산으로 소비 |

byte quota와 slot quota를 모두 적용한다. 빈 frame·작은 frame flood도 slot 상한으로 제한된다. TX는 `L+42`를 예약한 뒤 encode하며 실제 capacity가 더 크면 차액을 공개 전에 추가 예약하거나 buffer를 버리고 실패한다. RX도 실제 capacity를 charge한다. frame owner가 이동할 때 lease를 함께 옮기고 해제 때 한 번만 반환한다. codec 임시 allocation과 복사본이 공존하면 둘 다 계산한다. caller가 원래 보유한 입력·SQL 실행 메모리·kernel socket buffer·thread stack은 이 payload quota의 범위 밖이며 별도로 관측한다. 이 quota를 RSS 또는 전체 OOM 복구 보장으로 표시하지 않는다.

RX slot이 없거나 local byte 예산에 최대 frame 하나가 들어갈 여유가 없으면 **다음 header를 읽기 전에** read interest를 끈다. 외부 lease 반환도 read 재개 조건이다. header를 읽은 뒤 실제 L에 대한 global quota/allocator 예약이 실패하면 해당 연결을 `ResourceExhausted`로 닫는다. 작은 header를 무한히 쌓거나 모든 연결이 미완결 body 예약을 기다리는 상태를 만들지 않는다. idle connection은 최대 body buffer를 미리 점유하지 않는다. 송신 quota 부족은 caller에 `Backpressured`를 반환하며 자동 재시도·숨은 대기 queue를 만들지 않는다.

global quota 아래 연결 간 자원 할당은 보장된 균등 몫이 아니다. quota 확보 실패에 의한 연결 종료를 overload 결과로 관측한다. 실행 가능한 연결의 진행 공정성은 round-robin 예산으로 검사한다. 느린 peer 하나가 생겨도 다른 연결이 I/O 기회를 잃지 않아야 한다. 양쪽 application이 서로 읽지 않는 논리 deadlock은 자동 해결하지 않으며 deadline으로 정리한다. DEV5는 결과 생산·입력 소비의 순환 대기를 별도 검증한다.

<a id="lifecycle"></a>
## 6. 연결 상태·시간 제한·종료

```text
Connecting ── 실제 connect 성공 ──> Open
Connecting ── 오류/timeout/cancel ─> Closed
Open ── begin_drain / 경계 EOF ──> Draining ── TX 소진 ──> Closed
Open/Draining ── 오류/timeout/cancel ──────────────────> Closed
```

`Closed`는 terminal이며 재연결은 새 generation·새 connection이다. accepted socket은 바로 Open으로 들어간다. 이 상태는 transport 사용 가능만 뜻하며 인증된 SQL session이 아니다. RX/TX cursor는 연결 상태 아래 독립적으로 유지한다. 기존 `Lifecycle`은 admission guard 계수만 담당하며 socket close·flush·join을 수행한 것으로 해석하지 않는다.

| deadline | 값 | 시작·만료 의미 |
| --- | --- | --- |
| connect | 5,000 ms | connect 시작부터 실제 성공 판정까지 |
| frame 수신 | 30,000 ms | 첫 header byte부터 checksum 완료까지; byte마다 연장하지 않음 |
| TX frame | 30,000 ms | enqueue 승인부터 전체 bytes의 local write 완료까지; queue 대기도 포함 |
| idle | 60,000 ms | Open 진입 또는 마지막 완결 RX/TX부터; 부분 byte trickle은 갱신하지 않음 |
| drain | 5,000 ms | admission 닫힘부터 accepted TX 소진·native close까지 |
| wait 상한 | 20 ms | 가장 가까운 deadline과 이 상한의 최솟값; enqueue/cancel 관측 기회 |

시간은 monotonic clock으로 계산한다. 덧셈 overflow·역행 clock은 adapter 오류로 닫고 무한 deadline으로 바꾸지 않는다. 각 효과 전에 `now >= deadline`이면 timeout을 먼저 관측한다. 효과 직후에도 시간을 확인하며 deadline 이후에 발견한 완결 I/O를 시간 안 성공으로 소급하지 않는다. 늦은 성공 bytes는 progress에 남겨 결과 불명을 보존한다. wall clock 조정으로 deadline이 연장되지 않는다. 비실시간 Linux에서 20 ms는 scheduling 지연의 절대 상한을 보장하지 않는다.

경계 EOF는 `PeerReadClosed`를 보고하고 이미 accepted TX만 drain한다. 이후 새 송신·수신 admission은 없다. 완결 RX lease는 이미 관측된 입력으로 유효하지만 아직 처리하지 않은 업무를 완료할 연결이 보장되지는 않는다. C1은 FIN을 요청 종료 delimiter로 쓰는 half-close request/reply protocol을 제공하지 않는다. partial EOF, reset, 잘못된 frame은 즉시 close하며 queued TX를 성공 처리하지 않는다.

graceful shutdown은 listener admission 중단 → 각 연결 drain → 기한 내 close → registration 제거 → driver join 순서다. deadline 이후 강제 close는 `DrainTimedOut`이며 graceful 성공으로 보고하지 않는다. Drop만으로 drain 성공을 주장하지 않는다. 외부 RX lease는 close 후에도 유효하므로 모든 transport 자원이 0이라는 검사는 application lease를 모두 반환한 뒤 한다.

Unix socket은 시험이 새로 만든 mode 0700 디렉터리 아래 생성하며 endpoint 존재 시 bind 실패한다. 이전 파일·symlink·타인의 endpoint를 자동 unlink하지 않는다. shutdown 때 소유한 socket 경로의 identity를 확인한 뒤 자신이 만든 경로만 제거한다. shared directory를 지원해야 하면 경로 경합을 막는 별도 계약이 필요하다. TCP는 OS가 배정한 loopback port를 기록하며 TIME_WAIT 등 kernel 자원을 즉시 0이라고 주장하지 않는다.

<a id="outcome"></a>
## 7. 오류·취소와 결과의 확실성

transport 오류에는 `kind`, 연결 generation, phase, local send ticket, 성공한 wire byte 수, 원래 I/O error kind를 남긴다. payload·자격 증명·SQL text는 기본 진단에 넣지 않는다. 외부 공개 SQL 오류 ID는 DEV5/DEV6가 정한다. foundation의 파일용 `IoWriteUnknown`을 socket 성공/실패의 업무 판정으로 재사용하지 않는다.

| 관측 | transport가 보장하는 것 | 상위가 추정하면 안 되는 것 |
| --- | --- | --- |
| enqueue 전 한도/취소/Closed 거절 | 이 호출에서 bytes를 보내지 않음, `NotStarted` | 다른 호출/이전 연결의 업무도 취소됨 |
| Enqueued | bounded queue에 소유권 확보 | peer 수신·실행 |
| 송신 중/모두 local write 후 단절 | 성공한 prefix 또는 전체를 OS에 넘김, peer 관측 불명 | 실패 응답이므로 rollback 완료, 자동 재실행 안전 |
| `FlushedLocally` | 해당 frame 전체를 local write가 수락 | peer 수신 확인, SQL 성공, durable commit ACK |
| 완결 수신 | frame 무결성·길이·순서 확인 | 인증, payload 의미 유효성, commit 내구성 |
| local cancel/deadline | 더 진행하지 않고 연결 정리 | 이미 전달된 업무의 중단·rollback·비커밋 |

오류 종류는 `InvalidFrame`, `UnsupportedFormat`, `TruncatedFrame`, `ResourceExhausted`, `Io`, `WriteZero`, `ConnectTimedOut`, `FrameTimedOut`, `SendTimedOut`, `IdleTimedOut`, `DrainTimedOut`, `Cancelled`, `Closed`, `AdapterViolation`으로 구분한다. nonterminal `Backpressured`는 enqueue 거절의 typed 결과다. 종료 사유를 한 번만 확정하며 반복 close/cancel은 자원을 중복 반환하지 않는다. 내부 panic/poison은 운영 정상 오류로 덮지 않고 해당 driver 재사용을 금지한다.

COMM은 payload를 자동 재전송하지 않는다. `SendTicket`은 durable transaction/request ID가 아니며 재연결에서 재사용하지 않는다. DEV5는 요청 ID·session generation·인증된 cancel·terminal reply·commit certainty를 별도로 설계한다. commit 후 응답 유실은 상위에서 UNKNOWN일 수 있으며 C1 echo 성공으로 ACK→WAL→reopen을 검증했다고 하지 않는다.

<a id="observability"></a>
## 8. 계측과 검증 경계

연결/admission/close 사유, RX/TX bytes·frames·short I/O·WouldBlock, 현재/최대 queue bytes·slots, RX pause 기간, quota 거절, enqueue→local flush 지연, frame 수신 지연, deadline 초과·cancel 관측 지연을 기록한다. byte counters의 overflow는 saturating+overflow flag로 드러낸다. diagnostic ring은 driver 전체에 고정 256 event로 제한하고 위 driver control 예산에 포함하며 lost event 수를 별도로 남긴다. per-connection metric label을 무제한 보존하지 않는다.

필수 fault receipt 유실·fault 미도달·peer 미기동·timeout을 시험 PASS로 처리하지 않는다. correctness용 독립 oracle는 production codec·state transition을 정답 계산에 import하지 않는다. bounded model, native socket, 실제 process 종료, 성능·계측 비용은 서로 다른 증거다. 구체적인 C-V01–18, fault와 수치 판정은 [구현·검증 계획](../plans/comm-implementation-plan.md#acceptance)을 따른다.

<a id="decision"></a>
## 9. COMM-DESIGN-001 결정 기록

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 작업·계약·통합 담당 | `/root` |
| 권한 근거 | 사용자의 COMM 설계 요청, AGENTS 및 DP01의 기존 routine/D2 위임. 제품 방향·호환 의무·품질 gate는 유지 |
| 선택 | AXF1 재사용, 단일 owner의 nonblocking 진행, byte+slot quota, 절대 deadline, local-only C1 |
| 대안·이유 | 새 wire header는 DEV1과 중복되어 제외. blocking thread-per-connection은 제한된 연결에서 가능하지만 slow peer·취소·thread 비용을 함께 관리해야 하므로 단일 readiness owner를 기준으로 선택. async runtime·복잡한 reactor는 측정 후 검토 |
| 의미 근거 | [소스 조사](../evidence/comm-source-research.md)의 길이 검사·부분 I/O·오류/취소 분리와 현재 foundation 계약. 외부 코드 도입이나 wire 호환은 없음 |
| 대체 관계 | 기존 COMM의 미정 frame/queue/deadline leaf를 구체화. DEV1 AXF1 bytes·C1 선행 단계·DEV5 업무 의미 소유권은 대체하지 않음 |
| 독립 검토·통합 | `/root/comm_design_review`가 실제 문서·상위 계약·소스·색인을 읽기 전용 검토. 승인 선형화점·ticket 자원·모델 도달성 등 지적을 수정한 뒤 재검토: 기본 설계 채택 가능, 잔여 blocker 없음. `/root`가 계약 담당·통합 담당으로 반영. [실제 검토 기록](../evidence/comm-source-research.md#review) |
| 구현·실행 | transport/socket ports/runner 미구현. C1·KU12·C-V01–18 NOT_RUN. DEV0/DEV1 과거 PASS를 이번 검증으로 재사용하지 않음 |
| 재검토 조건 | AXF1 digest/복사 비용, 128 연결/poll 공정성의 한계, 원격/TLS 도입, 대용량 chunk·multiplexing·request cancel 필요, 현재 한도로 필요한 workload 수용 불가 |

구현할 경로는 실제 코드 도입 때만 package·fingerprint·clean export 목록에 추가한다. 이번 문서는 기존 `crates/transport` 계획을 연결하며 빈 crate·runner·실행 증거를 만들지 않는다.

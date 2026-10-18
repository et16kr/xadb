# COMM — 구현·검증 계획

2026-10-18 · **기본 설계 독립 검토·채택 / 구현·C1 인수 NOT_RUN**

[COMM 소유 계약](../contracts/comm-design.md) · [단계 완료 계약](stage-completion.json) · [KU12](../contracts/kernel-test-matrix.json) · [개발 운영 방침](../policies/development-policy.md)

<a id="sequence"></a>
## 1. 구현 순서와 공급 계약

COMM은 DEV1 뒤 시작하며 DEV2·DEV3·SQL을 기다리지 않는다. XADBTest frame은 시험 controller의 별도 protocol이므로 제품 AXF1과 혼동하거나 production transport로 교체하지 않는다.

| leaf | 구현할 책임 | 완료에 필요한 증거 |
| --- | --- | --- |
| COMM-C0 계약·port | foundation header inspection, ByteStream/Connector/Listener/ReadinessWait의 signature·오류·generation, 공유 Clock instance, fixed limit 상수 | 기존 DEV1 bytes 불변의 독립 vector, 상태별 readiness·owner Drop 정리 계약의 D2 리뷰, dependency feature 결정. 현재 미구현 |
| COMM-C1 codec·lease | incremental header/body, validated frame lease, TX cursor·FIFO, byte/slot quota | C-V01–06, 모든 실패 경로의 charge 반환. 아직 socket 통합 PASS 아님 |
| COMM-C2 Linux 효과 | 소유 TCP/Unix endpoint, nonblocking connect/accept, wait·close, generation·clock | C-V07–09, 실제 OS 관측·short I/O witness |
| COMM-C3 연결 조립 | round-robin, TX admission gate, absolute deadlines, backpressure, cancel·drain·join | C-V10–15, scripted schedule와 실제 socket/process 시험 |
| COMM-C4 인수·handoff | 독립 peer/oracle, fault receipt, metrics·성능·runner·clean export | C-V01–18 + DEV1/DEV0 회귀 + 별도 검토, C1 gate의 closure 기록 |

leaf `COMM-C0`–`COMM-C4`를 순서대로 통합한 뒤 완료 gate C1을 판정한다. 본문의 C0–C4는 이 leaf의 약칭이다. 파일·package·실행 명령은 실제 구현과 함께 등록한다. 지금 `run_entrypoint`는 null이며 아래 예산은 실행한 결과가 아니다.

작업 담당은 `/root`; 구현 착수 때 실제 구현자·독립 검토자·통합 담당을 기록한다. 기본은 순차 통합이다. foundation 계약을 consumer가 독립적으로 복제하지 않고 C0에서 먼저 동결한다. actual API가 설계와 다르면 관련 owner 계약을 수정·검토한 뒤 진행한다. 별도 검토 없는 구현은 `READY_FOR_REVIEW`이며 C1 PASS로 승격하지 않는다.

<a id="acceptance"></a>
## 2. KU12 상세 인수 목록

아래는 모두 `NOT_RUN`이다. C-V01–18 전부와 DEV1/DEV0 회귀가 C1의 현재 local capability closure다. 최소 범위만 실행하면 해당 leaf 결과만 기록한다.

| ID | 입력·장애·독립 기대값 | 필수 판정 |
| --- | --- | --- |
| C-V01 | 고정 AXF1 vector: L=0,1,2,1,048,576; L=1,048,577/u32::MAX, 잘못된 magic/version | endian·길이·checksum bytes 일치. 큰 L은 body allocation 전 거절. fixture expected는 독립 도구/수계산으로 고정 |
| C-V02 | 0/1/2-byte payload frame의 모든 cut·byte당 input, 두 frame coalescing; 큰 frame의 header/digest 전후 및 64 KiB 경계 cut | chunking과 무관한 동일 payload·순서·정확히 한 번 delivery; 끝 frame EOF와 partial EOF 구별 |
| C-V03 | 작은 vector의 각 byte/bit 손상, max-frame의 header/body/digest 대표 손상, invalid frame 뒤 valid frame | invalid payload 미공개, connection terminal, 뒤 frame으로 임의 resync 안 함 |
| C-V04 | 각 위치 short read/write·Interrupted·WouldBlock·write=0·잘못된 adapter count·reset | offset 보존, double send/skip 없음, zero progress는 예산 내 반환, 오류에서 성공 조작 없음 |
| C-V05 | RX 2 slots/TX 4 slots와 byte 한도 ±1, 128→129 연결, 작은/빈 frame flood, global 64 MiB 경쟁 | byte와 slot 모두 상한 준수, rejected enqueue 무효과, body 예약 실패 연결 종료 |
| C-V06 | reserve/allocate/encode/admission 실패, queue→consumer lease 이동, close 후 미회수 RX queue/외부 Connection 보유, owner Drop 후 외부 lease/sender/ticket 보유, ticket 회수 뒤 slot 재사용 | capacity charge 누락·double release 없음. 보유 Closed 항목의 connection slot 유지, 미회수 terminal ticket의 4-slot 상한, 소비한 ticket의 재접근 불가, 모든 handle/lease 반환 후 quota=0·handle baseline 복귀 |
| C-V07 | 실제 IPv4·IPv6 loopback 및 소유 Unix endpoint의 양방향 전송·동시 연결·connect refusal | 주소/port/path/process identity 기록, engine 없는 default binary 사용. 비-loopback/wildcard/DNS 거절 |
| C-V08 | 새 private Unix directory, 이미 있는 socket/file/symlink, 다른 소유 endpoint, bind failure·반복 close | 기존 객체 보존, 소유 경로만 정리, fd leak/다른 endpoint 삭제 없음 |
| C-V09 | TX 없는 pending connect의 writable 성공/오류, fd 번호 재사용·stale readiness·stale sender, buffered frame+hangup·FIN/RST/process kill | Connecting의 독립 interest와 실제 connect error 확인, generation 검증, HUP만으로 EOF/입력 유실 없음, 이미 deliver한 frame을 반복하지 않음, partial EOF 실패, 경계 EOF의 drain 정책 준수 |
| C-V10 | header/body trickle·부분 write, 시계 경계 직전/같음/직후, 마지막 I/O나 checksum 검증 중 deadline 통과, 서로 다른 epoch를 주입하려는 조립과 concurrent timestamp 수집 순서 역전 | absolute 30 s 유지, connect 5 s·idle 60 s·drain 5 s. 동일 clock 주입 강제, fresh 효과 전후 시각 사용, 늦은 RX 미공개·TX Failed(전체 진행량 가능), overflow/인과적 역행 거절 |
| C-V11 | 읽지 않는 peer+정상 peer, RX lease 미반환+hangup·양방향 queue 포화, hint 뒤 WouldBlock, 128 slot 점유+listener backlog | read throttle·write 진행·정상 peer 공정성, quota 유지, deadline cleanup. non-actionable registration의 제거/재개, empty-list wait, lease/slot 반환 후 20 ms 관측 주기로 재개; busy loop 없음 |
| C-V12 | enqueue 전/후·첫 write 전/후·마지막 write 전/후·partial receive·wait 중 cancel | 관측 시점 이후 새 connect/accept/read/write 없음; 필수 close/unregister 허용. partial frame 뒤 재사용 없음, flushed-local≠peer ACK, 반복 취소 idempotent |
| C-V13 | 한 listener flood+다른 listener의 connect, producer encoding/enqueue와 shutdown 경합, partial RX t=0→drain t=29.9 s→TX 완료 t=31 s, queued/partial/완결 TX 중 owner Drop, 반복 drain·Connecting drain | listener round-robin, admission 승인/차단 선형화. partial RX 해제·RX/idle timer 중단·검증 RX 회수 유지, TX별/drain 기한 적용. Drop도 ticket별 terminal 1회·실제 byte 진행 보존, 명시 join·정리, flush 없음 |
| C-V14 | 아래 유한 모델 전수 탐색과 property seed, 실제 thread에서 producer/lease release/cancel 경쟁 | 독립 모델의 상태/정답과 일치; 선언 공간 완주·counterexample 보존·30분 stress |
| C-V15 | 생산자 accepted/byte-progress/local-flush·peer-observed 이력을 별도로 기록; peer 관측 후 reply 유실 | 단절을 미실행으로 단정하거나 payload 자동 재전송하지 않음. SQL durable outcome은 판정 범위 밖 |
| C-V16 | wrong checksum/endian, quota+1 허용, offset reset, byte마다 deadline 연장, stale event 수락, cancel 후 다음 frame 전송, connect interest 누락, HUP를 EOF로 처리, stale clock 재사용, Drop 뒤 Pending 유지 mutant | 각각 독립 checker가 거절. fault 미도달·lost receipt·wrong binary·peer 미기동도 FAIL/INCOMPLETE |
| C-V17 | codec fuzz + connection event-sequence fuzz, 보존 corpus 전부 replay | panic/OOB/unbounded allocation/무한 loop 없음. invalid 입력이 정상 성공으로 관측되지 않음 |
| C-V18 | 크기·연결 수별 정상/slow peer 부하와 Always/Detailed 계측 비교 | throughput·p50/p95/p99·CPU/RSS·copy/queue·cancel 지연 원자료, 아래 수치 인수와 overhead 판정 |

TCP fragmentation은 packet 경계를 제어했다고 주장하지 않는다. scripted port에서 모든 선택 cut를 강제하고, native socket에서는 제한된 peer write와 작은 OS buffer로 실제 short I/O/WouldBlock 관측을 기록한다. 특정 fault가 OS에서 발생하지 않으면 그 셀은 미도달이며 재현 가능한 다른 recipe를 고정해야 한다. native receipt에 실제 byte 수·error·timestamp와 peer log를 남긴다.

<a id="profile"></a>
## 3. 동결할 시험 프로파일

| 항목 | 값·규칙 |
| --- | --- |
| 기본 환경 | Linux x86_64, Rust 1.95.0, 고정 Cargo.lock, local loopback/Unix만. 실행 시 kernel·CPU·OS·지원 IPv6·socket buffer 실제 값을 manifest에 기록 |
| deterministic model | 2 connections, 방향별 2 slots, payload 최대 2 bytes, trace 최대 8 events. event alphabet은 enqueue(빈 값/00/0001), read/write progress(1 byte/다음 header·payload·digest 경계까지), WouldBlock, EOF, cancel, drain, take_received, lease/ticket release, deadline advance. 두 연결의 모든 enabled event를 깊이 8까지 BFS로 탐색하고 상태·잔여 깊이를 키로 중복 제거; budget 초과는 INCOMPLETE |
| 모델 도달성 | header/body/digest 경계 progress는 실제 bytes의 합법적인 short I/O에 대응시킴. 완결 RX→take→release, TX→local flush→ticket 회수, 두 frame 순서·두 연결 진행, partial cancel, full queue 거절의 최소 witness가 각각 도달해야 PASS. 도달 상태/전이 수와 witness를 manifest에 기록 |
| 모델 범위 한계 | 축소 payload/slot은 모델 전용. production 최대 frame·실제 2/4 slots·128 연결은 별도 경계/native 시험으로 필수 검증 |
| 추가 고정 schedule | TX 없이 Connecting→writable→Open; RX pause→buffered frame+HUP→lease 반환→frame→read=0; RX 시작 0 s→TX enqueue/drain 29.9 s→완료 31 s; queued/partial TX→owner Drop→ticket 회수; 3 ready listener 중 하나 flood. C-V09–13의 독립 필수 반례이며 8-event 축소 모델만으로 검사했다고 하지 않음 |
| property V2/V3 | 64 seeds×최대128 operations / 256 seeds×최대1,024 operations. seed 목록·generator hash를 실행 전에 고정, 정확한 경계 길이 case 추가 |
| fuzz V3 | parser와 event-sequence target 각각 최소 30분, corpus·seed·crash 최소화 결과 보존; V2에서도 보존 corpus 전부 실행 |
| thread stress V3 | producer·receive lease return·cancel/shutdown 경쟁 최소 30분; payload pattern·sequence는 독립 기대값과 비교 |
| fault 반복 | 각 필수 비결정적 native fault 셀 3회 새 fixture. address family별 수행; unsupported IPv6는 full C1 미완료로 남기고 IPv4/Unix leaf만 보고 |
| scripted 공정성 | 계속 ready인 N connections가 N회 선택 안에 각각 RX/TX 기회를 얻음. accept 여유가 있는 동안 ready listener J개 각각이 J accept attempts 안에 선택됨. 한 순회 bytes/frames/attempts 및 driver 전체 accept 8회 상한 assertion |
| real liveness | 제공 부하가 정상 peer를 포화시키지 않는 fixture에서 정상 peer 5초 무진행=FAIL. abort/cancel은 요청 후 1초 내 native handle close·driver 반영. begin_drain은 5초 기한과 이후 close 처리 1초를 구분하여 관측; 결과는 DrainTimedOut. OS 지연도 해당 run 실패로 기록; realtime SLA 아님 |
| resources | 모든 시점 quota/slots/connection 상한, 종료 후 전체 lease 반환·join 뒤 fd/예약 baseline 복귀. kernel TIME_WAIT 제외 |
| fault matrix | Header(0/partial), Body(partial/complete), TX(queued/partial/last-byte), Connecting, Draining 각각의 EOF/error/cancel/deadline 중 유효 조합. 불가능한 조합의 이유도 manifest에 남김 |

fault 위치의 정확한 call number와 native buffer 설정은 구현된 port/runner에 의존한다. 구현 전 이 표를 완성한 것으로 추정하지 않고 C0/C4 manifest에 receipt 위치·expected error·byte progress·반복·명령을 추가해 독립 검토한다. 현재는 설계 수치와 case 목록이 정해졌으며 실행 profile/runner는 미구현이다.

<a id="performance"></a>
## 4. 성능과 관측 가능성

PE01–10의 [성능 개발 계약](../contracts/ai-performance-engineering.md)을 적용한다. 0/64/4,096/65,536/1,048,576-byte payload × 1/16/128 connections, TCP IPv4/IPv6·Unix별 echo 및 단방향 bulk를 측정한다. peer는 bounded 소비만 하며 application queue 무제한 복사를 허용하지 않는다. slow-peer는 peer 1개 읽기 정지와 나머지 정상 조건으로 분리한다. 각 run은 warmup 5초+측정 30초이며 Offered/Admitted/Completed/Dropped를 별도 기록한다.

release default/Always와 Detailed 계측을 동일 조건에서 순서를 번갈아 최소 10쌍 비교한다. 초기 overhead guardrail은 Always 대비 Detailed completed throughput 하락 10% 이하, p99 local round-trip 상승 15% 이하, peak payload quota 동일 상한이다. 원자료·분포·환경 잡음과 신뢰구간을 함께 보고하고 경계가 불확실하면 미인수로 남긴다. 이 수치는 첫 비용 인수 기준이며 DB TPS/SLA를 약속하지 않는다. checksum 비용을 없애는 등의 최적화는 같은 의미·무결성·quota를 유지하는 대안으로 DP07 리뷰한다.

idle 연결에서 TX 없이 writable wait 반복이 발생하지 않고, WouldBlock 직후 같은 연결의 무제한 재시도가 없음을 deterministic syscall ledger로 확인한다. CPU/RSS는 그 외 runtime/kernel 비용까지 포함하므로 payload ledger와 분리한다. poll/AXF1이 병목이면 먼저 bytes/hash/copy/wait/queue 비용을 계측한 뒤 reactor 변경이나 새 frame format의 필요성을 판단한다.

<a id="handoff"></a>
## 5. 완료·증거·DEV5 이관

C4에서 실제 runner 명령과 구현 상태 문서를 추가하고 `stage-completion.json`의 entrypoint/evidence를 연결한다. runner는 DEV1/DEV0 회귀, engine 없는 독립 build, default build, scripted/native/fuzz/stress/performance case, checker 부정 시험을 조립한다. 시험 feature는 release 제품 capability와 구분한다. foundation 공유 변경이 K0 소비자에 영향을 주면 현재 DEV2 leaf 회귀도 실행하며, 이 결과를 COMM의 내구성 증거로 해석하지 않는다.

source revision/dirty digest, binary/lockfile/profile/contract/oracle hash, 정확한 command와 환경, seed와 fault receipt, peer/driver 이력, 자원·cleanup, 검토 결과를 불변 run bundle로 남긴다. 실행 log에서 source 식별을 바꿔 쓰지 않는다. checksum/로그 누락, required case 미실행, 미해결 blocker, 실행 entrypoint 부재는 C1 PASS를 막는다. 공식 배포에는 기존 protected evidence·환경 qualification 의무가 추가된다.

DEV5는 C1 bytes/limits/version과 아래 의무를 인수한다.

- 인증/handshake와 지원 capability 거절; TCP 원격 공개와 TLS는 별도 SEC/NET 인수.
- SQL request/session ID, metadata/value/NULL/error encoding, batching·LOB chunk·fetch 흐름.
- 결과 producer backpressure와 snapshot/cursor/transaction cleanup. slow client가 engine lock을 잡은 채 기다리지 않음.
- 인증된 cancel request와 완료 경합, 재연결·중복 요청 정책, COMMITTED/ABORTED/UNKNOWN 표시.
- 실제 SQL→WAL→publish→reply 및 reply 유실 뒤 같은 build recovery. transport local flush를 durable ACK로 바꾸지 않음.

이번 변경은 설계 문서 작업이다. 제품 코드·dependency·crate·runner를 추가하지 않았으며 C1 실행은 NOT_RUN이다. 문서 자동 검사는 DOC-002에 따라 push 직전 훅에서 수행하고 편집 중 실행하지 않는다.

# COMM — 구현·검증 계획

2026-10-18 · **기본 설계 독립 검토·채택 / 핵심 구현·C1 인수 INCOMPLETE**

[COMM 소유 계약](../contracts/comm-design.md) · [단계 완료 계약](stage-completion.json) · [KU12](../contracts/kernel-test-matrix.json) · [개발 운영 방침](../policies/development-policy.md)

2026-10-23 [미완료 인수 재개·source별 잔여 대장](../evidence/comm-acceptance-resume-20261023.md): 기존30분3개·11 mutant 완료 증거를 원 source에 유지하고, RX-prefix/process-kill 신규36셀과 변경 checker/reader만 선택 검증했다. 전체 C1은 INCOMPLETE다. 아래 전체·하위 회귀·장시간 인수 요구에도 [변경 코드 시험 우선 지시](../../AGENTS.md)를 적용하며, DEV3 S5 전수 예외를 COMM으로 확대하지 않는다. 새 좁은 진입점은 `python3 -B scripts/check_comm_rx_kill.py`이며 `--cell`로 변경/실패 셀만 선택할 수 있다. 기존 전체 leaf의 자동 실행 목록을 늘리지 않는다.

<a id="time-bounded-batches"></a>
### 2026-10-23 시간 제한과 분할 실행

사용자의 후속 지시에 따라 단일8시간 이상 실행은 일단 건너뛰며, 분할할 수 있는 시험은 결과를 보존해 진행한다. **2026-10-23 23:00 KST 전에 실행·종료 정리·기록을 마칠 수 있는 범위만** 선택한다. 이 제한은 완료 기준·필수 분모를 줄이지 않으며 SKIP/중단을 PASS로 판정하지 않는다. 무변경 코드·완료 시험 반복 금지도 유지한다.

후속 사용자는 "당신은 테스트만 하잖아요 개발중이 아닌데"라고 현재 작업의 적용 범위를 정정했다. **현재는 개발 중 하위 회귀 재실행이 아닌 별도 미완료 COMM 인수 시험 작업이므로, 코드 수정 없이도 미실행 조합을 진행한다.** 위 무변경 코드 제한을 이 독립 시험의 미실행 사례까지 확대해 중단했던 해석은 이 작업에서 대체한다. 완료한 시험 반복 금지·8시간 이상 단일 실행 skip·23시 마감·source별 증거·원 profile/품질 기준은 유지한다. 이 구분은 다른 DEV 개발의 회귀 범위를 확대하지 않는다. [후속 실행 기록](../evidence/comm-performance-batch-20261023.md#standalone-continuation)은 해석 정정·미실행 선택·원 보류 기록을 구분한다.

새 `scripts/check_comm_performance_batch.py`는 원 profile의0-based `--cell-index`와 timezone-aware `--deadline`을 필수로 받는다. 신규 실행은 아래 적응 반복 계약에 따라 기본5쌍·조건부 추가5쌍을 선택한다. Always/Detailed 순서·5초 준비+30초 측정·guardrail과 전체135조합은 유지하며, 원10쌍/2,700 mode 결과는 당시 profile로 보존한다. default batch wall budget은3,600초이고8시간 이상 budget은 거절한다. build600초·mode120초+정리10초·최종 기록120초를 포함해 시작 가능 여부를 보수적으로 판단하고, 부족하면 build 전에 `SKIPPED_TIME_BUDGET`으로 남긴다. mode의 one-shot alarm은 기존 실행기의 worker kill/reap·임시 경로 정리에 연결한다. OS 정지까지 포함한 realtime 보장은 아니다.

일반 조합의 기본10 mode는 nominal350초이며 조건부 최대20 mode는700초다. 추가가 필요할 수 있으므로 시작 예산은 최대20 mode·보수3,320초를 예약한다. healthy peer 없는 slow1 조합은 추가 없이10 mode·보수2,020초다. 선택 결과는 `BATCH_PASS` 또는 FAIL이며 C1 전체는 INCOMPLETE다. 같은 source로 후속 조합을 진행하려면 보존 export 안의 이 runner를 실행해 live DEV3 변경을 섞지 않는다. 각 mode의 불변 receipt와 source/binary hash를 보존하고 source가 다른 원74 mode를 새 결과와 합쳐 현재 전체 PASS로 표시하지 않는다. [구현·실행 증거](../evidence/comm-performance-batch-20261023.md)를 따른다.

<a id="extra-hour-20261023"></a>
### 2026-10-23 후속 한 시간 시험 연장

사용자가22:52 KST 무렵 “한시간 정도 테스트를 더 해 주세요”라고 요청해 이번 추가 실행에 한해 앞선23시 마감을 연장했다. 실행 deadline은 **23:50 KST**, 종료 검토·기록 목표는23:52–23:55 KST다. 신규5+5 반복·8시간 이상 단일 실행 skip·완료 사례 반복 금지·source별 원자료·실패/중단 보존은 유지한다. 원 보류와23시 전 종료 기록을 덮지 않는다. 이번 실제 선택·결과는 [추가 한 시간 증거](../evidence/comm-performance-batch-20261023.md#extra-hour-20261023)에 따로 기록한다.

이번 연장의 별도 시간 단위 continuation은 첫 배치가 종료·정리된 뒤 같은 보존 source/worker를 재사용한다. 후보 순서를 실행 전에 고정하고, 셀 시작에 nominal 첫5쌍350초+mode120초+정리10초+최종120초=600초가 남아야 한다. **이는 셀 전체 완료의 보수 보장이 아니며** 기존 batch의 최대 예약3,320초와 다르다. 매 mode 전에는 기존120+10+120초 reserve와120초 alarm을 적용한다. 첫5쌍 추가 결정을 불변 저장하고, 추가가 필요하거나 일부 mode만 완료된 채 시간이 끝나면 INCOMPLETE/FAIL로 남긴다. 실행 도구·신규 controls의 독립 수용 뒤 진행하며 품질·5/10쌍·기존 정리 oracle를 줄이지 않는다.

<a id="adaptive-repetitions"></a>
### 2026-10-23 신규 COMM 성능 반복 계약

사용자의 “앞으로는 줄이고 나서 편차가 있을 때에만5회 더” 지시에 따라 `comm-five-plus-five-v1`을 신규 COMM C-V18 실행에 적용한다. 1쌍은 Always/Detailed 각1회다. 기본5쌍에서만 첫 판정을 하고 추가가 선택되면 pair6–10을 한 번 실행한다. 6–9쌍 중간 판정·불리한 pair 교체·추가 재시도는 하지 않는다. 기존10쌍 결과를 새5쌍 결과로 소급 분류하지 않는다.

- quota·정확성·실행 실패 또는 판정 구간 하한의 한도 초과는 terminal FAIL이다. 추가 측정으로 실패를 지우지 않는다.
- 첫5쌍의 설명용95% bootstrap 구간 폭이 처리량0.05(5%p), ECHO p99 0.075(7.5%p)를 **초과**하거나, 판정 구간이 `lower ≤ limit < upper`이면5쌍을 추가한다. 명백한 실패가 이 조건보다 우선한다.
- 두 판정 시점의 구간은 paired 효과5개/10개를 단위로 동일 seed·10,000회 재표본한 percentile bootstrap의 `[q1.25, q98.75]`(양측 명목97.5%)다. 기존 단일 상한의 명목2.5%를 look마다1.25%씩 배분한 Bonferroni 조정이며, 설명용95% 구간은 따로 기록한다. 작은 표본의 근사이므로 엄밀한 유한표본 오류율이나 고정10쌍과 같은 검출력·정밀도는 보장하지 않는다.
- 추가 조건이 없고 적용 지표의 판정 상한이 모두 원 한도(TP하락10%, p99상승15%) 이하면 PASS5다. 추가 후 처음5쌍을 포함한 전체10쌍의 판정 상한이 한도 이하면 PASS10, 아니면 FAIL이다. 추가가 필요하지만 중단되면 미완료이며 첫5쌍 PASS로 복귀하지 않는다.
- healthy peer 없는 `slow_peer && connections==1`은 처리량/p99 N/A를 inventory에서 파생하며15조합이다. quota·역압·취소·정리 의무를 유지한5쌍으로 끝낸다. BULK는 p99만 N/A이며 TP 편차 판정은 유지한다.

전체135조합의 기본 실행은1,350 mode이고 실제 요구량은 `1,350 + 10×추가 선택 조합 수`다. 추가 대상은 최대120조합이므로 이 규칙의 가능한 최대는2,550 mode다(모든 조합에10쌍을 가정한 공통 상한은2,700). source·binary·profile ID·실행 전 반복 규칙·첫5쌍 결정·추가 사유·두 구간·실제 요구량을 보존한다. 시간 계획은 선택 조합의 조건부 최대를 예약한다. 당일23시 마감과8시간 이상 단일 실행 skip은 유지한다. 신규 규칙의 checker 검증과 native 미실행은 [증거](../evidence/comm-performance-batch-20261023.md#adaptive-repetitions)에 구분한다.

<a id="sequence"></a>
## 1. 구현 순서와 공급 계약

COMM은 DEV1 뒤 시작하며 DEV2·DEV3·SQL을 기다리지 않는다. XADBTest frame은 시험 controller의 별도 protocol이므로 제품 AXF1과 혼동하거나 production transport로 교체하지 않는다.

| leaf | 구현할 책임 | 완료에 필요한 증거 |
| --- | --- | --- |
| COMM-C0 계약·port | foundation header inspection, ByteStream/Connector/Listener/ReadinessWait의 signature·오류·generation, 공유 Clock instance, fixed limit 상수 | 기존 DEV1 bytes 불변의 독립 vector, 상태별 readiness·owner Drop 정리 계약의 D2 리뷰, dependency feature 결정. 실제 port/header 구현·검토는 구현 기록 참조 |
| COMM-C1 codec·lease | incremental header/body, validated frame lease, TX cursor·FIFO, byte/slot quota | C-V01–06, 모든 실패 경로의 charge 반환. focused 실행을 전체 C-V 완료로 해석하지 않음 |
| COMM-C2 Linux 효과 | 소유 TCP/Unix endpoint, nonblocking connect/accept, wait·close, generation·clock | C-V07–09, 실제 OS 관측·short I/O witness |
| COMM-C3 연결 조립 | round-robin, TX admission gate, absolute deadlines, backpressure, cancel·drain·join | C-V10–15, scripted schedule와 실제 socket/process 시험 |
| COMM-C4 인수·handoff | 독립 peer/oracle, fault receipt, metrics·성능·runner·clean export | C-V01–18 + DEV1/DEV0 회귀 + 별도 검토, C1 gate의 closure 기록 |

leaf `COMM-C0`–`COMM-C4`를 순서대로 통합한 뒤 완료 gate C1을 판정한다. 본문의 C0–C4는 이 leaf의 약칭이다. 파일·package·실행 명령은 실제 구현과 함께 등록한다. 현재 focused leaf entrypoint는 `python3 -B scripts/check_comm.py --snapshot --with-regressions`이며 아래 전체 예산을 완료한 결과는 아니다. [현재 구현·잔여 인수](../implementation/comm-implementation.md)를 따른다.

작업 담당은 `/root`; 구현 착수 때 실제 구현자·독립 검토자·통합 담당을 기록한다. 기본은 순차 통합이다. foundation 계약을 consumer가 독립적으로 복제하지 않고 C0에서 먼저 동결한다. actual API가 설계와 다르면 관련 owner 계약을 수정·검토한 뒤 진행한다. 별도 검토 없는 구현은 `READY_FOR_REVIEW`이며 C1 PASS로 승격하지 않는다.

<a id="dev5-supply"></a>
### DEV5 peer identity 공급 배정

[DEV5 D5-S01](dev5-work-breakdown.md#supply)은 foundation/platform의 기존 COMM-C0/C2 책임에 배정한다. 현재 ByteStream/Listener의 socket read/write/accept 공급은 peer UID 조회 공급과 다르며, 이 추가 수신 경계는 OPEN_SUPPLY다. 새 SQL/auth policy를 transport에 넣거나 기존 C1의 실행 결과를 소급 변경하지 않는다.

- COMM-C0/foundation 담당: accepted connection 수명과 결합한 typed peer identity·조회 실패/미지원 경계·소유 API를 D5-F02와 함께 동결한다. private fd를 server로 노출하지 않는다.
- COMM-C2/platform 담당: 실제 accepted Unix socket의 OS credential 조회를 safe adapter로 공급하고 UID 불일치·조회 실패·소켓 종료/재사용의 좁은 native 사례와 dependency/feature·오류 검토 증거를 제공한다.
- DEV5 5.04.1 담당: 검토된 API·source/시험 범위를 수신하여 principal/DB allowlist와 연결한다. 정책 파일 검증은5.04.4, handshake는5.04.2 소유다. 포트 구현·server adapter·정식 Serving 인수를 따로 기록한다.

실제 구현자·별도 검토자는 공급 착수 때 배정하고 변경한 경로의 필요한 시험만 실행한다. 이 문서 편집은 port 구현·C1/B3 완료나 시험 재개가 아니다.

<a id="acceptance"></a>
## 2. KU12 상세 인수 목록

아래는 전체 C1 인수 inventory다. 일부 반례의 실제 실행은 [구현 기록](../implementation/comm-implementation.md)에 한정하며 각 C-V 전체 완료로 승격하지 않는다. C-V01–18 전부와 DEV1/DEV0 회귀가 C1의 현재 local capability closure다. 최소 범위만 실행하면 해당 leaf 결과만 기록한다.

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

fault 위치의 정확한 call number와 native buffer 설정은 구현된 port/runner에 의존한다. 구현 전 이 표를 완성한 것으로 추정하지 않고 C0/C4 manifest에 receipt 위치·expected error·byte progress·반복·명령을 추가해 독립 검토한다. 현재 focused runner와 receipt는 구현했으나 이 표의 전체 fault profile과 장시간 인수는 미완료다. 실제 실행 범위는 구현 기록을 따른다.

<a id="model-run-profile"></a>
## 3.1 COMM-TEST-002 유한 모델 실행 프로파일

2026-10-19 구현한 `crates/transport/tests/model.rs`와 `tests/exploration/mod.rs`는 기존 2 connections·RX/TX 각 2 slots·최대 payload 2 bytes·깊이 8을 구체화한다. 기본 제품의 TX4는 유지하고 test-support 생성자가 같은 production quota 경로의 TX slots만 2로 고정한다. RX peer 입력은 독립 SHA-256 encoder로 만든 `empty`, `00`, `0001` 순서다. TX enqueue는 세 payload를 모두 열거한다.

- 이벤트: 연결별 enqueue 3종, 방향별 1 byte/다음 header·payload·digest 경계 진행, 방향별 WouldBlock, EOF, cancel, drain, take_received, 각 외부 lease 반환, 각 live ticket Drop·terminal 수집, 가장 이른 deadline 이동. API상 가능한 무효과 cancel/drain/take/enqueue도 열거한다.
- I/O 이벤트 직전 공유 단조 시각을 20 ms 진행하여 이전 retry를 만료시킨다. 선택한 성공 progress 뒤 다음 I/O는 WouldBlock으로 막는다. deadline 이동은 `max(now, min(active deadlines))`이고 두 owner가 같은 시각을 관측한다. retry 이전 무재시도·deadline 직전/같음/직후·u64 overflow는 별도 고정 반례다.
- BFS는 상태와 남은 깊이로 중복 제거하며 두 연결 정체성을 구분한다. actor permutation·partial-order pruning은 하지 않는다. 각 enabled 전이에 대표 parent trace와 전이 하나를 더해 fresh 실제 Connection 두 개에서 재생한다. quota/slots·phase·오류·성공 prefix를 비교하고, trace 종료에서 남은 RX payload/순서와 모든 live ticket Pending/terminal/진행량까지 관측한다.
- `comm-model-key-v1`: 현재 시각을 모든 timer/retry와 함께 뺀 남은 지연으로 표현한다. Closed의 재사용 불가능한 입력·timer·다음 sequence는 지우되 RX 진행 witness bits, 오류·취소·기존 lease/ticket은 보존한다. 현재 유한 범위에서는 시간 평행이동이 deadline 선택/비교·I/O 시간 증가를 보존한다. 각 packed key는 경계를 확인하는 고정 80 bytes이며 hash digest로 상태 동일성을 추정하지 않는다. 서로 다른 원래 state가 같은 key로 합쳐질 때 enabled event·각 observation·successor key 동등성을 확인하는 별도 회귀를 둔다.
- 도달 witness: RX 완료→take→release, TX local flush→ticket 수집, RX partial cancel, TX partial cancel, full queue 거절, 두 연결 진행, 두 frame delivery 순서. cleanup cancel 뒤 quota=0 검사는 owner Drop의 ticket 해결 시험을 대신하지 않는다.
- 공식 후보 안전 예산은 누적 360,000,000 states, 5,000,000,000 transitions, 21,600 seconds다. 상태 추가 직후와 완료 직전에도 검사한다. 예산 초과·미도달은 INCOMPLETE다. 실행 로그·generator/key source hash·정확한 명령·원본 사본을 보존하며 전체 공간 완주 전 PASS로 올리지 않는다.

초기 exploratory r1은 정규화 전 depth 5까지 529,148 states·누적 1,171,355 transitions를 수행하고 4,000,000 누적 상태 안전 예산에서 INCOMPLETE였다. 원본 `artifacts/comm/model-exploratory-n6imzvzv/execution.log`를 유지한다. 새 key·source의 실행으로 이전 실패를 덮어쓰지 않는다. 실행 전 독립 검토는 구현 기록에 별도로 남긴다.

<a id="performance"></a>
## 4. 성능과 관측 가능성

PE01–10의 [성능 개발 계약](../contracts/ai-performance-engineering.md)을 적용한다. 0/64/4,096/65,536/1,048,576-byte payload × 1/16/128 connections, TCP IPv4/IPv6·Unix별 echo 및 단방향 bulk를 측정한다. peer는 bounded 소비만 하며 application queue 무제한 복사를 허용하지 않는다. slow-peer는 peer 1개 읽기 정지와 나머지 정상 조건으로 분리한다. 각 run은 warmup 5초+측정 30초이며 Offered/Admitted/Completed/Dropped를 별도 기록한다.

release default/Always와 Detailed 계측을 동일 조건에서 순서를 번갈아 [적응 반복 계약](#adaptive-repetitions)의 기본5쌍·조건부 추가5쌍으로 비교한다. 초기 overhead guardrail은 Always 대비 Detailed completed throughput 하락 10% 이하, p99 local round-trip 상승 15% 이하, peak payload quota 동일 상한이다. 원자료·분포·환경 잡음과 신뢰구간을 함께 보고하고 경계가 불확실하면 미인수로 남긴다. 이 수치는 첫 비용 인수 기준이며 DB TPS/SLA를 약속하지 않는다. checksum 비용을 없애는 등의 최적화는 같은 의미·무결성·quota를 유지하는 대안으로 DP07 리뷰한다.

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

최초 계획 작성은 설계 작업이었다. 후속 핵심 구현과 focused runner는 [구현 기록](../implementation/comm-implementation.md)에 연결하며 C1 전체 인수는 INCOMPLETE다. 문서 자동 검사는 DOC-002에 따라 push 직전 훅에서 수행하고 편집 중 실행하지 않는다.

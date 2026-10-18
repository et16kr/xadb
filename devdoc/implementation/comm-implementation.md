# COMM 구현 기록

2026-10-18 · **C0–C3 핵심 구현·C4 일부 / C1 전체 인수 INCOMPLETE**

[소유 계약](../contracts/comm-design.md) · [구현·검증 계획](../plans/comm-implementation-plan.md) · [저장소 배치](../policies/repository-build-plan.md)

## 실제 구현과 사용

- [foundation network](../../crates/foundation/src/network.rs): `ByteStream`, `Connector`, `Listener`, `ReadinessWait`, opaque `Registration`과 readiness hint. fd·nix 타입은 공통 API에 노출하지 않는다.
- [foundation codec](../../crates/foundation/src/codec.rs): AXF1 header inspection과 이미 예약한 buffer에 대한 allocation-free encode. 기존 AXF1 bytes·상한·완결 decode 계약은 유지한다.
- [transport](../../crates/transport/src/lib.rs): incremental RX, quota를 소유하는 `ReceivedFrame`, 4-slot FIFO 송신과 소비형 ticket, Connecting/Open/Draining/Closed, 공유 clock, 절대 deadline, 취소·Drop 결과 정리, 128 연결/3 listener의 bounded round-robin driver.
- [Linux network](../../crates/platform/src/network.rs): numeric IPv4/IPv6 loopback, 새 private Unix directory capability, nonblocking connect/accept, safe `ppoll`, 소유 경로의 device/inode 검사와 정리. `LocalNetwork` 하나를 해당 driver의 connector/listener/wait에 함께 사용한다. 기다리는 동안 같은 소켓을 다른 thread에서 닫는 multi-reactor 사용은 지원하지 않는다.
- [시험 worker](../../crates/testing/transport-worker/src/main.rs): engine 없는 default echo binary. 업무 protocol·인증·SQL·재전송을 구현하지 않는다.
- [검증 실행기](../../scripts/check_comm.py): 고정 Rust 시험 목록, 독립 Python peer, binary source/profile/PID 확인, socket family별 echo/checksum 오류/partial EOF/실제 process kill, 보존 receipt와 실패 폐쇄 판정. `--snapshot`은 source-only 사본을 보존하고 해당 사본에만 결과를 귀속한다.

```bash
cargo test -p xadb-transport --locked
python3 -B scripts/check_comm.py --snapshot --with-regressions
```

`Transport::new(Arc<Clock>)`로 하나의 process quota/clock domain을 만들고 `Driver::new(transport.clone(), network.clone())`에 전달한다. `transport.connect(&mut network.connector(endpoint))`는 slot 예약 후 connect를 시작한다. listener는 `network.listen(endpoint)` 뒤 driver에 등록한다. 조립 thread가 `Driver::step`을 반복하고 application이 수신 lease와 송신 ticket을 회수한다. `begin_drain` 후 Closed를 확인하고 조립부가 thread를 명시적으로 join한다. 직접 `Transport::accept`는 이미 생성된 caller-owned stream을 채택하는 경계이며, 외부에서 수행한 accept의 자원 admission까지 소급 보장하지 않는다.

sender·RX lease·ticket은 owner 종료 뒤에도 유효하며 해당 connection slot을 점유한다. terminal ticket은 `try_take(self)` 또는 Drop으로 회수한다. local flush는 peer 수신·SQL 성공·durable ACK가 아니다. 읽기/쓰기 bytes·frames·short I/O·WouldBlock·attempt·overflow 기본 계수는 제공하지만 전체 diagnostic ring·queue/latency profile과 Always/Detailed 비용 인수는 남아 있다.

## COMM-IMPL-001 결정과 변경 소유

| 항목 | 내용 |
| --- | --- |
| 분류·상태 | D2 · ACCEPTED; 구현/시험 결과와 C1 gate는 별도 |
| 담당·권한 | 작업·계약·통합 `/root`; 사용자 COMM 구현 요청, AGENTS·DP01의 routine/D2 위임 |
| 기준 | commit `3b5fad8b6df97ddb0d72f8d6361f563e3f2b59ce`; 착수 중 보존한 tracked diff SHA-256 `2b5b8517a2d429033318a432b5a49c23d91c44f93ef23d595f36fd60161e6b08` |
| 소유 경로 | `crates/transport`, `crates/testing/transport-worker`, foundation/platform의 `network.rs`, `scripts/check_comm.py`, `scripts/test_check_comm.py`, 이 구현 기록 |
| 공유 수정 | root Cargo manifest/lock, foundation codec·lib, platform manifest·lib, COMM owner/plan·상태·package·문서 색인. 최신 내용을 읽고 좁게 추가 |
| 동시 변경 | DEV2 durable/transaction, foundation/platform namespace, DEV3/DEV4 문서가 동시에 변경됨. 타 작업 원문·hash·진행 상태를 되돌리지 않음. 합쳐진 사본의 회귀와 COMM 전용 기준 사본의 회귀를 구별 |
| 순서 | C0 port/header → C1 framing/lease → C2 Linux 효과 → C3 driver/종료 → C4 focused runner → 별도 리뷰·수정·실행 증거 |
| 선택·대안 | trait effect port + 단일 owner + 고정 slot. 별도 async runtime이나 blocking thread-per-socket은 추가하지 않음. TX quota 뒤 `encode_frame_into`를 사용해 임시 복사본 이중 예약을 피함 |
| poll 보완 | millisecond `poll`로 짧은 timeout을 내림하면 busy loop가 생겨 같은 level-triggered 의미의 nanosecond `ppoll` 사용. 기존 safe nix wrapper와 signal feature를 사용하며 unsafe 예외 없음 |
| 대체 관계 | COMM-DESIGN-001/002의 byte·quota·시간·수명 의미를 유지하며 실제 API·효과 공급을 구체화. 제품 범위·C1 인수 의무 축소 없음 |
| 재검토 | allocator capacity/Drop ledger 누락, readiness spin, clock 재사용, native path 경쟁, 새로운 reactor/TLS/원격 endpoint 도입, 인수 측정에서 초기 한도 부적합 |

문서 검증기의 단계 상태 허용 목록에도 COMM의 PARTIAL_IMPLEMENTATION을 반영했다. 이는 검사 실행이나 C1 인수 승격이 아니다. 새 실행 입력은 기존 `crates`·`scripts` roots 안에 있다. fingerprint/build identity/clean export roots를 늘리지 않고 모든 새 파일을 포함한다. package의 normal 의존은 transport→foundation, worker→transport/platform/foundation이며 platform→transport 역참조는 없다. platform은 transport의 dev-dependency로 native 통합 시험에만 참여한다.

## 의존성 검토

기존 고정 `nix 0.30.1`에 `socket`, `net`, `poll` feature를 platform의 Linux target에서 추가한다. 별도 작업의 `dir` feature를 보존한다. safe `socket/connect`, `SockaddrIn/In6/UnixAddr`, `ppoll`만 사용하며 public API에는 외부 타입이 없다. 표준 라이브러리의 blocking connect 대신 nonblocking socket 생성이 필요하고, 직접 FFI는 `unsafe_code = forbid`와 충돌한다.

registry 원문의 Cargo.toml·사용 API·변경 내역을 열람했다. nix는 MIT/MSRV 1.69, 새 전이 의존 `memoffset 0.9.1`은 MIT, build dependency `autocfg 1.5.1`은 MIT 분기를 선택하며 MSRV 1.0이다. memoffset은 offset 계산, autocfg는 build 시 compiler 기능 탐지이며 thread/runtime 서비스를 추가하지 않는다. 실제 Rust 1.95 Linux build로 확인한다. Cargo.lock에서 버전/checksum을 고정한다. 배포 notice와 전체 dependency 보안 감사는 공식 배포 전 별도 의무다.

[공식 nix 변경 내역](https://github.com/nix-rust/nix/blob/v0.30.1/CHANGELOG.md)과 [RustSec RUSTSEC-2021-0119](https://rustsec.org/advisories/RUSTSEC-2021-0119.html)를 확인했다. 후자는 0.23.0 이상에서 수정된 getgrouplist 문제로 사용 중 0.30.1의 socket 경로에 해당하지 않는다. 전체 advisory database 감사를 수행했다는 뜻은 아니며, 미확인 보안 범위는 UNKNOWN이다. 제거 경로는 platform network adapter 교체이며 transport ABI에 nix 타입을 남기지 않는다.

## 검증 범위와 잔여 인수

실제 최종 run과 별도 검토 결과는 아래 실행 기록에 연결한다. 작은 frame의 모든 cut/bit 손상, 최대 frame 경계, 자원·종료·deadline·readiness 반례, 64 seeds×128 operations, thread 경쟁, 실제 IPv4/IPv6/Unix peer는 focused leaf 검증이다. source가 검증 중 바뀌거나 lower regression이 실패하면 전체 실행은 FAIL로 남긴다.

C1 완료에는 여전히 다음이 필요하다.

- 선언한 두 연결·깊이 8의 모델 전수 탐색과 모든 도달 witness.
- allocator/encode 실패 주입·fd 재사용·reset·trickle·slow peer 등을 포함한 전체 native fault matrix와 각 비결정적 셀 3회.
- parser와 event-sequence fuzz 각각 30분, thread stress 30분, corpus replay.
- C-V16 전체 production mutant 검출. 현재 Python checksum/endian/한도/receipt/binary/inventory 부정 검사는 일부만 담당한다.
- 전체 진단 event ring·queue/latency profile 및 C-V18의 크기×연결 수×socket 45 조합마다 echo/bulk(90 mode별 사례)·10 paired runs·Always/Detailed overhead 인수.

현재 process-kill receipt는 ready PID, peer의 성공한 `sendall(512)`, SIGKILL과 reap을 증명한다. worker가 그 512 bytes를 실제 읽었거나 partial RX에 도달했다는 증거는 아니므로 그 fault cell의 PASS로 계산하지 않는다. 영속성·SQL·TLS·제품 qualification은 주장하지 않는다.

## 실행·독립 검토 기록

`/root/comm_review`가 owner 계약·실제 API/구현·시험·runner·의존/문서 diff를 읽기 전용으로 독립 검토했다. 구현자의 자체 검토와 구분하며, 동일 모델의 별도 실행이 오류의 통계적 독립성을 뜻하지는 않는다. 최종 drain 수정까지 확인하여 **C0–C3 코드/API/의존 경계와 focused C4 checker의 잔여 차단 결함 없음, 해당 범위 D2 채택 가능**으로 판정했고 `/root`가 반영했다.

| 실제 지적 | 수정·회귀 |
| --- | --- |
| clock 역행을 admission/interest에서 한 번 거절한 뒤 재사용 가능 | shared ClockDomain의 sticky 오류와 gate fatal. sender 및 interest 경유 회귀 |
| 일반 readiness→WouldBlock, error-only 힌트, listener 무진행 반복 | 실제 wait 목록 제외, 20 ms retry, invalid listener 종료 |
| 보관한 HUP가 preflight와 post-wait에서 각각 I/O budget을 사용 | preflight를 timer/cancel 전용 `poll_control`로 분리; 한 step의 실제 byte 상한 회귀 |
| 1 ms 미만 timeout을 정수 poll 시간으로 내리면 busy loop | safe ppoll의 nanosecond timeout, 실제 빈 목록 대기 시험 |
| wait 실패가 Cancelled로 바뀌거나 오래된 wait 기준 시각 사용 | 원래 I/O error kind 보존, wait 직전 fresh 시각 재계산 |
| drain deadline을 admission mutex 대기 전에 시작 | gate 차단과 시각 읽기를 같은 임계 구역에 둠; clock이 gate 보유를 검사하는 회귀 |

최종 [r2 실행 보고서](../evidence/comm-isolated-xadb-20261018-r2.json)는 **focused leaf PASS / C1 전체 INCOMPLETE**다. `python3 -B scripts/check_comm.py --with-regressions`를 보존한 COMM 전용 사본에서 실행했다. 그 사본의 source SHA-256은 `e873f96b2b791d0c655dfb4019de4db6209a4f1ab02359f57e4f861815b62ad1`이며 실행 전·후와 실제 worker ready identity가 일치한다.

| 실제 실행 | 결과·범위 |
| --- | --- |
| Rust 고정 시험 inventory | 28개 전부 PASS; default와 all-features에서 각각 실행 |
| Python checker 부정 시험 | 3개 test method PASS; 잘못된 byte/size/digest, 누락·ignored inventory, 잘못된 binary/receipt/resource 정답을 거절 |
| native worker/독립 peer | IPv4·IPv6·Unix × echo/corrupt/truncated/kill 12개 사례 PASS. echo는 0/1/2/64/4,096/65,536/1,048,576-byte payload; kill 도달 한계는 위 설명 적용 |
| build·경계·lint | engine 없는 normal dependency closure, fresh target의 transport 독립 build, default worker build, 관련 crate의 all-targets/all-features clippy PASS |
| 하위 회귀 | [DEV0 8개](../evidence/comm-dev0-xadb-20261018-r1.json), [DEV1 8개](../evidence/comm-dev1-xadb-20261018-r1.json), [DEV2 P0/K0 leaf 28개](../evidence/comm-dev2-xadb-20261018-r1.json) check PASS. DEV2 전체·내구성 증거가 아님 |

[실행 범위 색인](../evidence/comm-execution-scope-20261018.json)에 각 실행의 원래 execution root, source 구성 recipe, 원본 보고서 hash, COMM 소유 파일 18개의 hash를 남겼다. 복사한 JSON의 상대 artifact 경로는 그 실행의 root에서 해석한다. raw report는 바이트를 변경하지 않았다. 최종 COMM 소유 파일 18개는 live와 검증 사본에서 일치한다. 공유 파일에서는 별도 작업의 namespace export·ContentHasher/content_digest·nix dir feature를 제외했다. 따라서 위 PASS는 기록한 기준 commit에 COMM 변경을 적용한 사본의 결과이며 현재 혼합 작업 트리 전체에 대한 PASS가 아니다. 기존 current-source DEV0/DEV1/DEV2 문서 ID와 다른 작업의 미검증 상태를 대체하지 않는다.

마지막 drain 수정 전의 [r1 27-test PASS](../evidence/comm-isolated-xadb-20261018-r1.json)도 원래 source identity로 보존한다. 앞서 현재 변경들을 함께 담은 [혼합 사본 r1](../evidence/comm-mixed-xadb-20261018-r1.json)은 COMM focused 검사에는 통과했으나 DEV0의 format 검사에서 하위 회귀가 실패해 전체 FAIL이다. 그 실패를 삭제하거나 새 사본의 PASS로 덮지 않았다. 독립 검토자는 r1·혼합 보고서 원본 일치, 로그·binary hash와 source identity, 최종 소유 파일 일치를 별도로 확인했다. 최종 r2와 DEV0/DEV1/DEV2 하위 보고서의 원본·hash, 28개씩의 feature별 시험·12개 native 사례·회귀 PASS 및 scoped 상태 표기도 다시 확인하여 수정 필수 사항 없음으로 판정했다.

전체 C1의 성능 프로파일만 순차 실행 기준 90개 workload 사례 × 10쌍 × 2개 계측 설정(Always/Detailed) × 35초 = 63,000초(17시간 30분)다. 여기에 fuzz 두 target·thread stress 각 30분, 남은 구현·fault/model 인수 시간이 추가된다. focused runner의 짧은 echo timing은 이 인수를 대체하지 않는다. 문서 자동 검사는 DOC-002에 따라 push 때 수행하며 이 작업의 편집 중에는 실행하지 않는다.

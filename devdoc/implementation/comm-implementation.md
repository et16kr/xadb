# COMM 구현 기록

2026-10-18 · **C0–C3 핵심 구현·C4 일부 / C1 전체 인수 INCOMPLETE**

[소유 계약](../contracts/comm-design.md) · [구현·검증 계획](../plans/comm-implementation-plan.md) · [저장소 배치](../policies/repository-build-plan.md)

2026-10-19 [코드 리뷰·수정 기록](../evidence/comm-code-review-20261019.md): driver 취소를 기존 sender·각 I/O 검사에 전달하고, accept의 peer 중단 오류를 listener별 backoff로 격리했다. 관련 COMM 시험·독립 검토는 PASS이며 리뷰 당시 혼합 사본은 별도 DEV2 시험의 lint 실패로 하위 통합 gate가 FAIL이었다. 두 수정만 분리한 commit 대상 사본의 transport 시험은 기본/전체 feature 각각 35개 PASS다. 전체 C1 상태는 유지한다.

2026-10-23 [COMM 리뷰 재검증과 오류 격리 수정](#review-fixes-20261023): peer 주소 거절과 terminal sender의 자원 부족 오류 분류를 수정했다. 변경 범위의 Rust11·inventory2·native1, 관련 Clippy·포맷은 PASS이며 최종 독립 검토는 아래 상태를 따른다. 전체 C1 인수 상태는 유지한다.

## COMM-TEST-002 진행 계약

2026-10-19 COMM 코드리뷰 수정 commit `91cefa6`을 확인한 뒤 사용자의 명시적 재개 요청에 따라 인수 작업을 재개했다. 해당 commit의 driver 취소 전달·accept 오류 격리를 유지한다. 진단 ring의 enqueue 기록은 gate 해제 뒤 실행되므로 flush 기록보다 늦을 수 있다. ring을 FIFO 승인 순서나 fault 도달의 독립 oracle로 사용하지 않는다. allocation 실패 주입은 실제 allocator 호출 직전의 합성 실패이며 OS OOM을 검증했다는 의미가 아니다.

2026-10-19 사용자 `comm 테스트 완료` 요청에 따른 D2 인수 실행기·checker·계측 보완이다. 기존 C1 품질 기준은 유지한다. 작업·계약·통합 담당은 `/root`, 독립 검토 담당은 `/root/comm_acceptance_review`다. 검토 전 변경은 `READY_FOR_REVIEW`이며 이 기록만으로 C1을 승격하지 않는다.

- 기준 commit: `fe8d3f5d2a7744a1c54a26fa9d30c656b974fa7c`; 착수 tracked diff SHA-256: `7fb1a39c51a65d9de0db91616211bd6390286e6c56ed279144bbd9025065dfb7`.
- 소유 경로: `crates/transport`, `crates/testing/transport-worker`, `scripts/check_comm*.py`, `scripts/test_check_comm*.py`와 실제 추가하는 COMM 전용 실행기·corpus, 이 구현 기록 및 신규 COMM 증거.
- 공유 변경: transport/worker manifest와 모듈 등록, `devdoc/plans/stage-completion.json`, 관련 단계·계약·문서 색인은 최신 원문의 COMM 항목만 순차 반영한다. foundation AXF1 형식·engine 의미는 변경하지 않는다.
- 추가 공유 API: `foundation::runtime::Quota::peak()`는 성공한 예약 CAS의 이전 값에서 high-water를 계산한다. 예약 후 별도 `used()` 표본은 동시 release로 실제 peak를 놓칠 수 있다는 독립 검토 지적을 반영한 선택이다. 기존 예약/반환·한도 의미는 유지하고 foundation 회귀 및 DEV2 소비자 회귀를 요구한다. 사후 collector 표본과 중복 ledger 대안을 제외한다.
- 동시 작업: 착수 때 engine substrate·controller·protocol·engine-worker의 durable performance 변경과 미추적 시험/실행기를 확인했다. 해당 변경을 보존하고 COMM 전용 파일과 섞어 되돌리지 않는다. 최종 회귀는 실제 보존한 source snapshot에 귀속한다.
- 순서: 미검증 inventory/반례와 독립 검토 → 실패 주입·계측 → 모델/fault/mutant/fuzz/stress/성능 실행기 → profile 동결·source snapshot → 실제 실행·하위 회귀 → 증거 재검토 → C1 판정.
- 대안과 선택: 기존 focused runner의 PASS를 전체 완료로 재해석하지 않고, 동일 nonblocking port와 독립 peer를 확장한다. 새 async runtime·외부 dependency·wire 형식은 추가하지 않는다. 장시간 검사는 동결한 job별 receipt로 합치고 미실행·미도달·실패를 보존한다.
- 재검토 조건: 검사기의 거짓 PASS, 필수 도달 witness 부재, 모델 공간 미완주, source identity 변화, 자원/성능 guardrail 실패.

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


<a id="review-fixes-20261023"></a>
## 2026-10-23 COMM 리뷰 재검증·오류 격리 수정

**D1 제품 수정 / D2 inventory 추가 / 선택 검증 PASS · 최종 독립 검토 ACCEPTED · C1 INCOMPLETE.** 사용자가 open-code-review-delegate 스킬 리뷰의 판단을 재점검한 뒤 수정·commit하도록 요청했다. 작업·계약·통합 담당은 `/root`, 별도 검토자는 `/root/comm_fix_review`다. 이전의 2026-10-18·19 실행과 원래 source identity는 바꾸지 않는다.

### 범위와 판단

- 기준 commit은 `d31cfb9bcd97e64554e0d97aa47fe3a3cbe70160`이다. 착수 때 engine residency의 미commit 변경을 확인했고, 이후 controller·DEV0 계약·문서 색인 변경도 확인했다. 이 작업은 해당 변경을 수정하거나 commit에 포함하지 않는다.
- 소유 경로는 `crates/platform/src/network.rs`, `crates/transport/src/sender.rs`, 새 `crates/transport/tests/sender_rejection.rs`, `scripts/check_comm.py`의 추가 inventory 항목과 이 구현 기록이다. 새 package·dependency·wire bytes·quota·deadline·공개 API signature 변경은 없다.
- `LocalListener`가 거절한 non-loopback peer는 해당 accepted socket의 문제다. 기존 `InvalidInput`은 driver의 치명 오류로 전파되어 정상 연결까지 닫았다. 실제 host의 non-loopback source에 bind한 로컬 client로 이 경로를 재현했다. 원격 서비스 공격으로 범위를 확대하지 않는다.
- terminal ticket도 회수 전까지 TX slot을 유지한다는 [소유 계약](../contracts/comm-design.md#ports) 때문에, full slot 뒤의 Closed/Cancelled/원래 I/O 오류가 nonterminal `Backpressured`로 바뀌었다. [오류 의미](../contracts/comm-design.md#outcome)에 따라 종료 사유·재시도 판단 오류로 확정했다. 두 항목 모두 별도 검토자가 계약·실제 경로에서 독립 재점검했다.

### 선택과 대안

peer 주소 검사는 작은 private helper로 옮기고 **그 peer 거절만** `ConnectionAborted`로 반환한다. 기존 listener별 20 ms retry가 적용되어 정상 연결을 보존한다. 일반 `InvalidInput`, listener 자체 고장·권한 오류까지 transient로 넓히는 대안은 제외했다.

송신 slot/byte/allocator 준비 실패 시 기존 admission mutex에서 cancellation → fatal → open 순으로 확인한다. 계속 Open이면 `Backpressured`를 반환·계수하고, terminal이면 원래 오류를 보존한다. 정상 enqueue의 마지막 재검사와 선형화점은 유지한다. 새 preflight lock을 모든 정상 송신에 추가하는 대신 실패 경로에만 재검사를 추가했다. lock 보유 중 allocation·checksum·socket 효과는 없다. 새 token·queue·상태 복제도 없다.

`check_comm.py`에는 새 기본6·test-support1 시험 이름만 추가한다. 이는 D2 inventory 보완이며 checker 알고리즘·기존 필수 항목·인수 예산을 완화하지 않는다. 새 이름7개를 실제 Rust 시험 정의와 정적으로 대조하고, 기존 두 부정 시험으로 inventory 누락/ignored 거절과 기본·계측 필수 집합 처리를 확인했다. 재검토 조건은 peer 거절의 전역 전파 재발, 종료 사유 은폐, 준비 실패의 lock 순환·자원 누수, inventory 누락이다.

### 실행 범위와 원 FAIL

로컬 사본은 `artifacts/comm/fix-20261023-bor7fvcx/source`다. 기준 commit의 Git archive에 이번 소유 파일만 적용하여 engine의 동시 변경을 분리했다. `baseline.json`, `red-results.json`, `red-open-control-corrected.json`, `green-results.json`, `validation-identity.json`과 개별 로그를 그 상위에 보존했다. 수정 전 세 소유 파일은 `red-owned/`에 원래 hash와 일치하도록 별도 보존했다. 이 로컬 산출물은 Git 배포 대상이 아니다.

peer 분류의 선택 시험을 위해 private helper만 먼저 추출하고 원래 `InvalidInput` 동작을 유지한 사본을 수정 전 기준으로 사용했다. 그 source identity에서 peer 분류1·terminal sender5·준비 실패1의 **7개 제품 반례가 FAIL**이었다. 실제 native peer 거절도 worker의 `Io(Some(InvalidInput))`와 기존 정상 연결 종료로 FAIL했다. 최초 정상 대조 시험은 manual clock을 기존 20 ms readiness retry로 전진시키지 않아 Pending을 성공으로 기대한 fixture 오류로 FAIL했다. 그 시험만 바로잡아 제품 수정 전 PASS를 확인했고 원 FAIL은 보존했다. 당시 native worker binary의 별도 hash는 미수집이며 사후 값으로 채우지 않는다.

제품 수정 뒤 마지막 실제 diff와 아래 사례의 관계를 확인하여 선택 실행했다. 이미 완료한 무변경 하위 코드·전체 leaf/workspace wrapper·30분 부하·전체 C1 인수는 실행하지 않았다.

| 선택 실행 | 실제 결과와 변경 관계 |
| --- | --- |
| platform `--lib network::tests::` | 신규2 PASS: 변경한 peer 분류의 IPv4/IPv6/IPv4-mapped 거절과 IPv4/IPv6 loopback 허용 |
| transport `--test sender_rejection` default | 신규6 PASS: full slot의 Closed·local cancel·driver cancel 전 step·drain·원 I/O cause와 Open backpressure/회수 후 재승인. terminal 계수 불증가·ticket 결과·최종 자원 반환 포함 |
| test-support `preparation_failures_preserve_terminal_errors_and_return_every_charge` | 신규1 PASS: SendSlot/LocalBytes/GlobalBytes/Allocation × Closed/Cancelled/Io의12조합, 준비 실패 뒤 원인·자원 반환 |
| test-support `every_send_preparation_failure_returns_all_charges_without_admission` | 관련 기존1 PASS: 변경한 준비 실패 분기의 Open 거절과 다음 정상 admission, encode/admission 인접 오류 |
| `concurrent_producers_and_cancellation_return_every_ticket_and_charge` | 관련 기존1 PASS: 변경한 sender gate 경로의 실제8 producer ×64 rounds, cancel/Drop 경합·ticket/charge 반환 |
| Python inventory 선택2 | 추가한 default/test-support 이름의 필수 포함·누락/ignored 거절·두 inventory 구분 PASS. 다른 checker 시험은 미실행 |
| `native_probe.py`와 default transport worker | native1 PASS: source-bound non-loopback peer의 실제 거절 전후 기존 정상 연결 echo 각1개, 최종 frames=2/flushed=2/failed=0와 payload/connection=0 |
| 관련 정적 검사 | `cargo clippy -p xadb-platform -p xadb-transport --all-targets --all-features --locked --offline -- -D warnings`, 변경 Rust3파일의 `rustfmt --edition 2024 --check` PASS. Clippy는 시험을 실행하지 않음 |

따라서 수정 후 **Rust11개 method + Python2개 method + native1개 시나리오 PASS**다. default6을 test-support에서 반복하지 않았으며, 기존 시험 전부를 이번 결과로 보고하지 않는다. 실행 명령은 `green-results.json`에 개별 argv로 보존했다. 관련 manifests·lock/toolchain·foundation/platform/transport/worker·변경 inventory의55파일이 사본과 현재 원문에서 같은 것을 확인했다. 다른 engine/controller 변경이나 저장소 전체의 검증 결과로 승계하지 않는다.

| 최종 검증 파일 | SHA-256 |
| --- | --- |
| `crates/platform/src/network.rs` | `22fb01ca8574440bc661127bcfda1f6c0ba19b438cabf10afa371b6ff0111e51` |
| `crates/transport/src/sender.rs` | `c6f1a14422655274bdd41b3af64d17f8ac96af8dc66d965e905811cec4721823` |
| `crates/transport/tests/sender_rejection.rs` | `567948e52d46e4e0a85bc73b9c999921535a2ddb7290dc50ca89fa2f6afd8474` |
| `scripts/check_comm.py` | `98f575b40bdf537a4dc06886c1f5f6564c3b9b245a285b90b8adfbeb6958f20d` |

### 독립 검토와 통합

별도 검토자 `/root/comm_fix_review`는 최초 지적의 타당성과 최소 수정 방향, 최종 patch·새 시험·원 FAIL·선택 PASS·source 일치와 문서의 D1/D2 범위를 읽기 전용으로 확인하여 **수용 가능, 차단 지적 없음**으로 판정했다. OCR 대상4/4와 unsupported_ext로 제외된 구현 문서1/1을 추가 검토했고 소유 범위의 생략은 없다. 제품3파일·inventory·로그16개·원 source3개·worker hash와 관련55파일 일치를 직접 대조했다. 검토자는 시험을 재실행하거나 파일을 수정하지 않았다. inventory 시험은 합성 로그 검사이고 새 Rust 이름의 존재는 정적 대조라는 문구 보완도 확인했다. `/root`가 계약·통합 담당으로 D1 제품 수정과 D2 inventory 추가를 수용했다. 문서 자동 검사는 [push gate](../policies/documentation-policy.md#push-gate)에 남기며 이 편집/commit 작업에서는 실행하지 않는다. 전체 C1 인수와 DEV 단계 상태는 바꾸지 않는다.

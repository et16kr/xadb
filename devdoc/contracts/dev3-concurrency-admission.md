# DEV3 3.10.1.a.1 — 적용 admission·version gate와 후속 Commit 설치 명세

2026-10-22 · `/root/allocation_compiler` 작성 · `/root` 계약/통합 담당 · **D2 채택 / 제한 구현·검증 완료 / 최종 단위 판정은 인수 기록 참조**

채택 원본 SHA는 `088d1f3399ce79e4e152000b24e98048184c09c470799d35907ec8451c4454fc`이며 `/root/allocation_review`의 독립 D2 수용 원본은 `artifacts/dev3/resume-20261022/review/guard-draft-review-r1.md`(SHA `38dbc47488324c78a5d45828ea6fae2dd1251b9237e70167446d4c2ff92d5907`)에 보존했다. [동시성 소유 계약](dev3-task-10-concurrency.md)과 [단계별 시험 계획](../plans/dev3-test-plan.md)을 따른다. 구현 중 확정 signature·case·상한은 해당 실행 전에 기록한다. guard 동작의 공급 완료와 실제 page/WAL/Commit 통합 완료를 구분한다.

## 1. 이번 leaf의 공급과 경계

첫 leaf **3.10.1.a.1**의 실제 제품 공급/완료 분모는 **bounded apply/content admission과 실제 exclusive version mutex**다. 기존 3.10.1.a에 포함된 실제 tree/page/append rank와 bounded transaction lock은 남으므로 .a.1 완료를 .a 전체 완료로 승격하지 않는다. Commit 설치 소유는 **후속 결합 명세와 cfg(test) seam의 모의 상태 검증만** 제공하며 이번 제품 완료 분모에서 제외한다. primitive 자체에는 page bytes/WAL/FD 효과가 없다. source 정본은 `dev3-task-10-concurrency.md` R1/R8/R9/R10와 freeze, `disk-storage-design.md` mutation-protocol/commit-installation-order, REVIEW-010 R2-F1이다.

q 선택/registry 등록, transient UndoRef/TxnRef pin, FIFO/두 단계 drain, 실제 tree/page/append rank, row/key lock, dirty buffer와 Commit producer는 이번 완료 분모가 아니다. 이들을 token 이름이나 test-only pending 전이로 구현했다고 표시하지 않는다. **state mutex 진입과 Drop은 blocking `lock()`이며 짧고 bounded한 scalar/배열 임계구간만 보호한다. version mutex만 `try_lock()`을 사용한다.** Condvar·상위 admission 대기·FIFO·재시도 loop는 없다. 짧은 state mutex의 OS 스케줄링 대기를 상위 작업의 진행 보장이나 bounded wall-clock 지연으로 주장하지 않는다. Busy/모의 InstallationPending을 반환하는 좁은 경계이며 예약 maintenance의 진행 보장은 .1.c의 실제 FIFO 연결 뒤에 판정한다.

실제 consumer 없는 가짜 `mark_commit_durable()`를 만들지 않는다. 모의 installation 전이의 진입은 cfg(test) seam으로 제한하고, 아직 공급되지 않은 WAL producer의 stage/완료 receipt를 대신하는 일반 `pub(crate)` 생성자·bool·LSN 입력 API는 없다. 모의 token/state는 실제 production 완료 또는 durable 성공 권한을 발급하지 않는다. 후속 WAL/turn 구현이 실제 producer 경계를 연결할 때 해당 제품 코드와 별도 인수를 추가한다.

## 2. 소유·경로·공급 수신

| 경로 | 작성자·역할 |
| --- | --- |
| `crates/engine/src/transaction/concurrency.rs` | root가 지정한 단일 구현자. DEV3 전용 실제 gate/admission/state; 기존 DEV2 Publication/Coordinator와 별개 책임 |
| `crates/engine/src/transaction/concurrency/tests.rs` | 별도 시험 담당. 고정 소규모 thread/cleanup/독립 oracle 시험 |
| `crates/engine/src/transaction/concurrency/tests/resource.rs` | root가 typed F05 layout·동시34-owner 회계 검증 공급 |
| `crates/engine/src/transaction/concurrency/test_support.rs` | 제품 구현 담당. cfg(test) 전용 관측·fault·모의 설치 seam |
| `crates/engine/src/transaction.rs` | root만 좁은 module 등록. 기존 volatile public 의미 변경 없음 |
| `crates/engine/src/storage/session_fence.rs` | root만 `SessionFence` 및 필요한 메서드의 `pub(crate)` 가시성을 좁게 확대. 동작·atomic ordering 변경 없음 |
| compiler runner/fixture | API 최종 동결 후 전용 담당. `scripts/check_dev3_concurrency_borrows.py`, `scripts/test_check_dev3_concurrency_borrows.py`, `crates/engine/tests/fixtures/dev3-concurrency/compiler/` |
| shared 문서/색인/build inventory | root만 반영. 이번 초안에서는 수정하지 않음 |

`SessionFence`는 기존 저장 owner에 embedded 상태로 남기고 core가 `&SessionFence`를 빌린다. core를 그 저장 owner 안에 self-reference로 넣지 않는다. 후속 상위 session assembly가 기존 storage owner를 먼저 생성하고 그 안정된 borrow 기간에 core를 외부 소유한다. storage owner의 move/drop은 core borrow가 막는다. 실제 borrow accessor/조립은 해당 consumer 변경 때 root가 추가한다. 이번에 임의 새 fence를 복제하여 두 정상 상태를 만들지 않는다.

기존 foundation `Lifecycle`은 단방향 stop이어서 재사용하지 않는다. `Quota/Reservation`은 상위 선예약용이며 admission state mutex 안에서 호출하지 않는다. `SessionFence` atomic·표준 Mutex만 소비하고 새 crate·unsafe·platform import·TLS 전역 rank map·동적 collection은 없다.

## 3. F02: 자료형·정확한 수량

`PageConcurrency<'f>`는 한 storage session에 하나다. 생성은 상위 transaction/session 조립의 비공개 함수와 test assembly로 제한한다. 외부 field 접근·Clone·재초기화/clear-poison API는 없다. 동일 namespace라도 다른 core의 token은 같지 않다. token은 자신이 빌린 core 참조에 묶이며 cross-core operation을 허용하지 않는다.

| 상태 | 고정 표현·범위 |
| --- | --- |
| owner slots | `[OwnerSlot; 34]`; 0..31 Foreground, 32 Maintenance, 33 DedicatedFlusher. 역할 간 fallback/대여 없음 |
| OwnerSlot | 실제 `generation:u64`, `mode:Free/Idle/Apply/Content/Retired`; generation0은 미사용, 재발급 시 checked+1. CommitStage/CommitPending/CommitInstalling은 후속 명세·cfg(test) 모의 mode이며 이번 제품 공급으로 세지 않음 |
| active_apply | `u16`, 0..33; Foreground32+Maintenance1만 진입. DedicatedFlusher는 apply/version 불가 |
| active_content | `u16`, 0..34; healthy 상태에서 슬롯당 최대1. Apply/Content/Commit mode의 중복 없음 |
| version gate | 실제 `Mutex<()>` 1개, 살아 있는 `VersionEditGuard` 최대1. 모의/후속 Commit stage/install guard도 같은 gate를 사용하며 별도 gate를 추가하지 않음 |
| installation(모의/후속) | `Open`, `Intent{key}`, `Pending{key,lsn}`, `Installing{key,lsn}`, `Abandoned{key,lsn?}` 중 하나. pending 총수≤1. 실제 WAL 결합 전 release API에는 Open에서 나가는 producer 진입점 없음 |
| InstallationKey(모의/후속) | owner slot + 해당 owner generation + checked 단조 `installation_generation:u64`. namespace/open 소유는 core borrow가 공급 |
| owner/guard 성질 | 모두 non-Clone/non-Copy, `!Send`/`!Sync`; allocation 없는 `PhantomData<Rc<()>>` 후보. core는 표준 Mutex/borrow의 auto trait로 공유 가능 |

`active_apply + active_content <= 34`이고 각 카운터는 실제 mode 배열에서 독립 재계산할 수 있어야 한다. 모의/후속 CommitStage/Installing은 active_apply에1을 더하며 CommitPending은 카운터0으로 남고 기존 owner slot을 계속 보유한다. 따라서 모의 continuation은 새 owner slot/foreground quota를 요청하지 않지만 실제 Commit 공급 완료를 뜻하지 않는다. DedicatedFlusher slot은 foreground 포화에도 빌려주지 않는다. 이34개는 admission handle 한도이며32개의 최대크기 거래나 모든 reader/cursor 동시 수용 보장이 아니다.

owner 반납은 generation을 되돌리지 않는다. generationMAX를 발급받은 owner의 정상 Drop은 mode를 Retired로 놓고, 다른 정상 Drop은 Free로 놓는다. 따라서 정상 Free slot의 generation은MAX보다 작으며 **Free/MAX 관측은 Invariant/fence**다. try_owner는 실패 때 slot을 바꾸지 않는다. 영역에 가용 Free가 있으면 그 중 번호가 가장 작은 slot을 고른다. 사용할 Free가 없고 점유 slot이 있으면 Capacity, 해당 영역이 모두 Retired이면 GenerationExhausted다. installation generationMAX면 Intent 생성 전에 GenerationExhausted/Unchanged다. max 값 fixture는 test-only이며 native 도달 이력으로 표시하지 않는다.

## 4. F02: 실제 API 형태와 lock 규칙

아래 수명 표기는 의미를 드러내는 초안이며 Rust 구현 전 borrow 검토로 확정한다. 반환 guard는 owner를 독점 대여하며 self-referential 객체를 만들지 않는다.

```rust
PageConcurrency::try_owner(&self, role: OwnerRole) -> Result<Owner<'_>>;
Owner::try_apply(&mut self) -> Result<ApplyAdmission<'_>>;
Owner::try_content(&mut self) -> Result<ContentAdmission<'_>>;
ApplyAdmission::try_version(self) -> Result<VersionEditGuard<'_>>;
```

`Owner`는 core/slot/generation만 보유한다. `ApplyAdmission`은 `&mut Owner`, `VersionEditGuard`는 실제 mutex guard와 그 admission을 함께 보유한다. `try_version(self)` 실패는 self를 소비·정리하여 active_apply를 반환한다. 호출자는 오류 값을 받은 뒤 원래 Owner를 다시 사용할 수 있다. version guard와 content guard가 살아 있는 동안 같은 Owner의 다른 admission/반납을 컴파일러가 막는다.

1. owner/admission 진입은 atomic fence 검사→blocking `state.lock()`→fence 재확인→아래 API별 순서다. state lock 밖에서 gate를 요청한다. state mutex를 가진 채 version mutex를 요청하지 않는다. state 임계구간 안에 I/O·allocation·Quota·외부 callback·다른 admission 대기를 넣지 않는다.
2. `try_apply`는 installation Open만 통과한다. 다른 상태는 **InstallationPending/Unchanged**이며 owner는 Idle, counter 변화0이다. DedicatedFlusher의 apply는 InvalidRole/Unchanged다.
3. version 진입은 state에서 installation/mode를 확인 후 state를 해제→실제 `try_lock(version)`→gate 아래 state를 다시 확인한다. **이미 admission을 얻은 producer도 두 번째 검사를 생략할 수 없다.** Commit intent가 먼저 생겼으면 gate/admission 모두 반환한다. 먼저 한 번 Open을 본 사실은 stage 권한이 아니다.
4. version Mutex WouldBlock은 Busy/Unchanged다. Poisoned 또는 state/mode/key 불일치는 fence+CallerMustResolve다. 두 번째 검사 동안의 `version → state`만 허용하며 state→version 역순을 만들지 않는다.
5. content admission은 InstallationPending 중에도 허용한다. 이는 이미 설치된 image를 읽거나 copy하는 유한 content 구간의 진입 허가일 뿐 page pin/content RwLock 자체가 아니다. caller가 I/O·callback·다른 admission 대기를 그 구간에 넣지 않도록 실제 consumer 검토/시험이 필요하다.
6. **unwind이면 모든 guard는 gate/admission/counter를 반환하기 전에 shared SessionFence를 먼저 set한다.** version guard는 그 다음 actual version lock을 놓고 blocking `state.lock()`을 획득하여 apply counter/mode를 반환한다. 정상 Drop도 gate 반환→blocking state lock→정확한 회계 반환 순서다. Drop에 `try_lock()`/Busy를 사용해 반납을 생략하지 않는다. test-only barrier는 production API의 callback이 아니다.
7. 정상 guard Drop은 자신의 slot generation/mode를 확인한 뒤 정확히1을 반환한다. state poison이면 fence 후 poisoned guard의 내부 상태를 얻어 확인 가능한 자기 회계만 정리하며 poison을 clear하지 않는다. 손상/underflow면 fence하고 임의 counter 조작·panic 없이 종료한다. Drop은 allocation·숨은 sync/commit이 없다. **fenced/poison/손상 이후** 신규 서비스 재사용은 금지하며, 건강한 정상 Drop 뒤에는 정해진 generation 규약으로 재사용할 수 있다. 오류 반환 중 admission Drop이 같은 state mutex를 다시 잡지 않도록 **모든 error branch는 현재 state guard를 먼저 해제**한다.

API별 오류 우선순위는 다음과 같다. 공통 선행은 entry에서 이미 관측한 Fenced → state lock에서 실제 관측한 Poisoned(동시에 fence) → lock 뒤 관측한 Fenced → 자기 slot/key/mode 및 counter 범위 Invariant(동시에 fence)다. 이 선행을 통과한 뒤 아래 순서를 따른다. Poisoned/Invariant는 CallerMustResolve, 건강한 expected rejection은 Unchanged다. concurrent fence가 아직 관측되지 않은 이미 승인된 구간의 종료를 새 admission 성공과 혼동하지 않는다.

| API | 공통 선행 뒤의 우선순위·실패 소유 |
| --- | --- |
| `try_owner` | 역할 영역의 Free/Retired/occupied 조사→Capacity 또는 GenerationExhausted→발급. **Pending 중에도 healthy owner 발급은 허용**한다. 발급 자체는 apply/stage 권한이 아니며 신규 owner는 content만 즉시 진입할 수 있다. 실패 slot/counter 변화0 |
| `try_apply` | InvalidRole(DedicatedFlusher)→InstallationPending→checked counter/mode 변경. Idle이 아닌 자기 mode는 사전 Invariant다. 실패 owner Idle/counter 변화0 |
| `try_content` | installation은 검사하지 않음→checked counter/mode 변경. healthy Pending에서도 허용한다. Abandoned는 반드시 fence를 동반하며 건강한 Abandoned 관측은 Invariant다 |
| `try_version(self)` | gate 전 InstallationPending 검사→state 해제→version try. WouldBlock 뒤 fence 재확인: 보이면 Fenced, 아니면 Busy. version poison이면 Poisoned+fence. gate 성공 뒤 공통 선행/InstallationPending을 재검사. 모든 실패는 actual gate와 admission을 정리한 뒤 반환 |

checked counter overflow/owner modes와 불일치는 healthy capacity 거절로 숨기지 않고 Invariant/fence다. 실제 Capacity는 owner 슬롯 admission에서 결정한다. 새 owner 발급 중 Pending 허용과 신규 apply 차단은 GA03/04의 서로 다른 assertion으로 검증한다.

첫 leaf는 tree/page/append lock을 취하지 않는다. 후속 실제 consumer의 순서는 coordinator→version→정렬된 tree→정렬된 page→append→registry다. 공유→배타 승격·부분 pin 보유 추가 frame 대기·latch 안 I/O는 계속 금지다. admission token만으로 이 후속 순서를 이미 검증했다고 하지 않는다.

## 5. F02: 후속 Commit 설치 명세와 cfg(test) 모의 경계

2026-10-25 후속: [D3-COMMIT-CONTINUATION-001](dev3-commit-continuation.md)이 실제 원 pool/WAL을 위한 비공개 공통 소유를 구체화한다. Pending의 원 physical PinSet 보유, durable 뒤 새 content 진입을 닫는 Drain, 같은 state mutex의 Condvar와 외부 fence 재검사를 한정 채택했다. 아래 최초 모의 명세의 “pin 미소유”와 Pending→Installing의 무조건 try/retry는 이 실제 continuation에 한해 후속 계약을 따른다. 최초 .a.1 증거는 원 source 범위로 보존하며 실제 terminal authority·Commit/recovery 검증이 완료된 것은 아니다.

다음 전이는 이번 .a.1 제품 완료 분모에서 제외하며 명세와 cfg(test) seam으로만 검증한다. 일반 caller가 LSN/true를 주어 권한을 얻는 product API는 제공하지 않는다. **이 절 전체를 WAL owner와 함께 동결하고 실제 private receipt 공급을 추가해야 실제 Commit stage에 연결할 수 있다.**

| 전이 | 필요한 실제 권한·효과·반환 |
| --- | --- |
| Open→Intent | Foreground의 실제 VersionEditGuard 소비. installation generation을 checked 증가하고 CommitStage가 version/admission을 유지한다. 이후 producer의 stage 시도 전까지 소유 공백 없음 |
| Intent→Open | stage가 아직 시도되지 않았거나, 실제 producer가 전부 Unchanged임을 확인한 private rejection receipt만 허용. gate/admission 반환, 기존 owner Idle. generation 되감기 없음 |
| Intent→Pending(c) | 실제 WAL stage의 완결 prefix와 Commit semantic validation이 성공한 **private StageReceipt** 필요. 임의 `WalPrefix`/raw bytes/Idle만으로 생성 불가. stage 성공 직후 상태를 Pending으로 인계한 다음 gate/admission 반환 |
| Pending(c)→Installing(c) | 동일 CommitInstallation token만 가능. 새 slot/Quota/turn 소유를 요구하지 않는 continuation 진입. version try 실패면 원래 Pending token을 그대로 반환; 다른 producer에게 설치 권한을 주지 않음 |
| Installing(c)→Open | producer 전용 완료 receipt: status, terminal queue/credit, descriptor, dirty/DPT 등록 전부 완료. 설치 guard/token 소비 후 gate/admission 반환. 이 receipt는 상태 bit mask나 caller bool로 조립할 수 없음 |
| Pending/Installing→Abandoned | sync 불명·부분 설치·poison·예상 밖 token Drop/unwind. **gate/admission 반환 전에 먼저 fence**하고 pending identity/LSN을 보존. 정상 취소/재시도 성공으로 낮추지 않음 |

CommitStage에는 private `NotAttempted/Attempted` 구분을 둔다. producer의 stage 호출 직전에 Attempted로 바꾸고, Unchanged 반환의 실제 검산 뒤에만 취소를 허용한다. Attempted 중 unwind는 fence한다. **stage 뒤 정상 cancel API는 첫 버전에 없다.** 잠재적으로 나중에 WAL로 쓸 수 있는 pending slot을 남긴 채 장벽을 Open으로 돌릴 수 없다.

CommitInstallation은 owner의 독점 borrow와 pending key/LSN을 유지하며 mutex·page pin·workspace를 소유하지 않는다. sync 동안 physical WAL slot이 Idle이어도 barrier는 Pending이다. token Drop은 단순 예약 반환이 아닌 fence/Abandoned 전이이며 장벽을 해제하지 않는다. owner Drop은 control slot 반환만 할 수 있고 Abandoned identity를 지우지 않는다. fenced core는 신규 owner/admission 자체를 거절한다.

실제 stage→Pending 인계 사이 unwind에도 fence해야 한다. 따라서 actual WAL adapter가 core의 private transition을 호출할 수 있도록 위치/가시성을 정할 때 arbitrary sibling 호출을 허용하는 공개 setter로 확대하지 않는다. root 단일 owner가 adapter를 같은 private 책임 아래 두거나, 실제 producer만 생성 가능한 receipt를 소비하는 좁은 경계를 택한다. 첫 primitive의 cfg(test) seam은 `test_stage_rejected`, `test_stage_completed`, `test_install_completed`처럼 모의 전이를 명시하며 release build에 포함하지 않는다. 이 seam 결과는 실제 WAL/dirty/crash 증거가 아니다.

## 6. FIFO 후속이 지켜야 할 결합 계약

이번 API에 상위 작업 wait/FIFO/head 기능은 없다. §4의 짧은 state mutex blocking 획득과 구별한다. 따라서 모의/후속 `InstallationPending`을 받은 caller가 무제한 try loop로 진행을 주장하지 않는다. 후속 .1.c는 이 오류를 받으면 적용 lock·실제 pin·공유12MiB·선택된 head를 반환하고 재진입해야 한다. 이미 head인 maintenance도 같은 규칙을 따른다.

Commit continuation은 자기 token으로 barrier를 통과하며 다른 대기 head에 막혀 자기 장벽을 해제하지 못하는 순환을 만들지 않는다. Prepare에서 신규 apply/snapshot만 닫고 기존 reader/전용4MiB flusher는 허용; Apply에서만 content를 닫고 유한 drain; latch/turn 반환 뒤 I/O한다. 실제34-owner core에 FIFO queue/대기 슬롯을 추가할 때 F05를 다시 동결한다. 여기에는 ticket 수·K·FIFO 성공 수치를 넣지 않으며 아직 완료되지 않은 진행성을 작은 guard 시험으로 대체하지 않는다.

## 7. F05: first-effect 전 고정할 control budget

이 leaf의 첫 effect는 volatile admission/state 전이다. disk/WAL 효과는0이다. 생성/상위 admission 전에 다음 **정확한 개수와 타입 크기 상한**을 확인한다. 값은 제안 ceiling이며 아직 sizeof 실측 PASS가 아니다.

| 항목 | 최대 개수·개별 상한·동시 생존 |
| --- | --- |
| PageConcurrency | 1 × 2048bytes. state `[OwnerSlot;34]`, 두 mutex, fence borrow, generation/installation 포함. 별도 heap 없음 |
| Owner | 34 × 64bytes =2176. mode와 무관하게 slot당1; non-Clone |
| 일반 apply/content guard | 나머지33 ×80bytes =2640. 슬롯당1이며 version/Commit wrapper 안에 든 admission을 중복 합산하지 않음 |
| 가장 큰 exclusive guard | 최대1 ×384bytes. 실제 VersionEditGuard와 내장 admission의 보수 ceiling. 모의 CommitStage/Installing도 이 ceiling으로 별도 측정하되 후속 실제 token 크기/자원 검증을 대신하지 않음 |
| 동시에 살아 있는 core 호출 임시 상태 | owner당 호출의 전체 nested-local 합≤1024bytes, 최대34개로34816bytes. 각 helper마다1024bytes를 별도로 허용하는 뜻이 아니다. Owner 없는 constructor는 호출1개·전체 nested-local≤8192bytes이며 사용 중 호출과 겹치지 않음. state 배열 전체 clone·per-call 동적/큰 배열 생성을 하지 않음 |
| component 보수 합 | 사용 중2048+2176+2640+384+34816 = **42064bytes**. constructor는 결과 core2048+임시8192≤10240bytes. 따라서 control admission charge **64KiB**를 권장. underlying std/OS thread stack·OS mutex 구현 메모리 전체를 이 수치로 인증하지 않음 |
| 별도/미소비 | 기존 SessionFence backing은 원래 storage owner에서 한 번만 계산. FD0/WAL slot0/page payload0, tree/key/registry map0, q/pin/FIFO queue0. caller pin/frame/flush4MiB/maintenance12MiB는 실제 consumer가 별도 선예약 |

타입 size assertion만으로 nested-local 상한을 통과시키지 않는다. 구현에서 각 branch의 동시 locals·Drop/오류 반환 객체를 조사하고 concrete Rust size/배열 및 최대 guard 교차 상태를 확인한다. 모듈 자체 Vec/Box/Arc/format allocation0, state mutex 안에 외부 호출0을 source review로 검산한다. 위 temporary 상한을 실제 표현으로 증명하기 어렵다면 effect 전에 ceiling/charge를 D2 수정하고 기록하며 실행 뒤 수치를 낮춰 PASS시키지 않는다.

**동시 호출 전제:**42064bytes 합계는 상위 assembly가 이 core에 진입하는 실행 주체를 foreground32+maintenance1+flusher1로 이미 제한한 profile에 한정한다. Owner slot34개만으로 slot 발급 전 `try_owner` 호출 thread 수까지 제한된다고 가정하지 않는다. slot 발급 전/거절 경로의 caller stack은 그 실행 주체의 사전 예약에 포함한다. 이 상위 제한을 아직 공급하지 않는 primitive 단독 시험은 동시 소유 control7248bytes 및 호출당 temporary1024bytes까지만 검증하며64KiB 전체 admission 완료를 주장하지 않는다. 실제 소비 전에 root가 제한된 caller assembly를 연결하거나 별도 ingress 제한과 추가 비용을 동결해야 한다.

owner pool 포화에서도 기존 Pending token은 원래 slot을 보유하고 continuation은 slot 추가0이다. DedicatedFlusher1은 별도 예약되어 새 foreground32개에 빌려주지 않는다. barrier 검사 이전/이후 counter 검사와 반환을 독립 배열 oracle로 대조한다. 이 control budget은 global32-transaction/frame/abort 자원 인수나64KiB allocator component 증명을 대체하지 않는다.

## 8. F03/F04: 구현 동반 중간 필수 inventory

아래는 frozen candidate IDs다. 실제 함수명·probe source/diagnostic·expected trace·timeout을 구현 직전 fixture로 보존하고 각각 selector로 고를 수 있게 한다. sleep에 의존하지 않고 채널/barrier 도달을 기록한다. **GA03/04와 GA05의 Commit 부분은 cfg(test) 모의 결합 명세 시험이며 .a.1의 실제 Commit 제품완료 분모에는 넣지 않는다.** 실제 WAL에 연결하지 않은 상태는 trace에 `physical_stage=SIMULATED`로 표시한다.

| ID | 고정 입력·독립 판정 |
| --- | --- |
| GA01 capacity/reuse | Foreground32+Maintenance1+DedicatedFlusher1 정상, 같은 role +1 거절, 하나 반환 뒤 그 slot 재발급·generation+1, 다른 역할의 빈 slot 대여 금지. 별도 단일-slot 역할 fixture에서 Free/MAX−1→MAX 발급→정상 Drop/Retired→GenerationExhausted를 확인. 비정상 Free/MAX는 별도 Invariant/fence fixture. installationMAX는 모의 경계. expected rejection은 원래 slot/owner/counter 보존 |
| GA02 lifetime/exclusion | 실제2 threads, Apply와 Content의 중복 슬롯0, 두 version 시도의 첫 성공/두 번째 Busy, 첫 guard 반환 후 두 번째 성공. content 진입은 version과 별개. 실패 때 active_apply와 actual mutex 모두 반환 |
| GA03 admission race(모의) | A가 Open에서 Apply admission을 얻고 version 전 pause. B의 cfg(test) seam이 Intent/Pending으로 전이한 뒤 gate 반환. A의 후속 version 진입은 InstallationPending으로 거절·counter 반환. raw Idle 관측을 바꿔도 결과 동일. gate 전·후 검사 둘 중 하나 생략 mutant 검출 |
| GA04 pending owner(모의) | owner32+maintenance+flusher 포화에서 모의 Pending(c101)은 새 일반 apply 차단, 기존 content·flusher 허용. 하나 반환→Pending 중 새 owner 발급→content 성공/apply 거절. 원래 모의 token만 Installing, 추가 owner 요청0, 완료 seam 뒤 일반 apply 성공. 다른 core/token identity 거절. 실제 commit durability 성공을 주장하지 않음 |
| GA05 cleanup/fence | 실제 guard unwind에서 **fence→gate/admission 반환** 순서를 barrier로 관측. 별도 모의 NotAttempted cancel/Unchanged rejection은 Open/Idle 복원, Attempted unwind/Pending Drop/Installing 실패는 fence/Abandoned/key 보존. sync 표지 하나만으로 Open 금지. Drop panic0/새 allocation0 source 검토 |
| GA06 poison/lock order | state mutex poison와 version mutex poison 각각 permanent fence, common fence가 모든 신규 API를 거절. state lock이 잠시 점유된 동안 Drop이 기다린 후 정확히1을 반환하며 Busy 누수가 없음을 barrier로 관측. poisoned lock 회계 정리는 poison을 clear하지 않음. version→state만 존재하고 state→version 역순 부정 schedule 검출. tree/page rank 인수는 아님 |
| GA07 resource | supported concrete type sizes,34 live owners·33일반guard+1큰guard 상한 및 임시 locals review. active_apply+content/mode 독립 합, 실패/취소 뒤 counters0·slot 반환. cap−1/equal/+1은 새 control ceiling·실제계수에만 적용 |
| GA08 compiler | 정상 baseline/순차 release 성공; Owner/guard thread 이동 E0277 후보, 살아 있는 guard 중 같은 Owner 재사용 E0499 후보, parent drop-before-child E0505 후보, token Clone/일반 pending setter 접근 거절. **코드/span은 실제 probe를 작성한 뒤 실행 전에 고정**하며 무관한 privacy/import 오류를 intended lifetime 거절로 세지 않음 |

thread step watchdog5초, 전체 narrow runtime 묶음120초, compiler 누적30초 후보. timeout/미도달/0건/중복은 FAIL 또는 NOT_RUN이며 PASS로 집계하지 않는다. compiler runner/checker를 새로 만들면 새 선택·진단·budget·identity 분기만 selftest한다. source/build/fixture/schedule/log hash, 각 case 선택 이유와 실제 diff를 보존한다.

## 9. S5 확대와 후속 인수

S5는 확정 DEV3 inventory 전수라는 사용자 지시를 따른다. 이 leaf에서 추가하는 S5 입력은 role 점유0/1/32·34-owner 조합, generation/오류 위치 전체, 취소/poison 교차의 고정 expanded schedule이다. GA02/03/04/05 각128개, 합512개 interleaving을 [확대 fixture](../../tests/fixtures/dev3-concurrency/expanded-schedules.tsv)와 별도 ignored selector로 준비했다. 각 schedule의 action 열·정답·도달 가능한 조합과 생성 규칙은 아래 r2 입력에 동결했다. fixture 존재·census 검산은512개 실행 성공이 아니며 S5 전까지 NOT_RUN으로 남긴다. 임의 시간 연장이나 무한 stress는 없다.

다음 공급 연결 때 새 diff로 추가하는 필수 case는 q10 등록/C10-01–05, 실제 pin/latch rank/C10-06, FIFO/prepare-flusher/head 양도/C10-07–09, 그리고 **3.11-T11 실제 Commit@c→물리 Idle→같은 status page 후속@c+1 차단→status/terminal/DPT 전체 설치→이전 image flush/redirty→crash/reopen winner**다. 이 중 첫 leaf가 실행하지 않은 부분은 계속 NOT_RUN으로 남긴다. unchanged 기존 buffer/WAL/compiler/model suite를 첫 guard 단계라는 이유로 반복하지 않는다.

## 10. 검토 요청

root가 정한 .a.1 부분 공급·shared fence borrow·non-Send·no callback/dynamic-map 방향과 1차 검토 지적을 반영했다. 독립 검토에서 (1)34-slot 역할 분할과 API별 error 우선순위, (2)self-referential 구조 없는 owner/admission/guard 수명, (3)blocking state lock/try-version/Drop 정확 반환 및 **반환 전 unwind fence**, (4)Commit 명세·test-only seam의 제품완료 분모 제외, (5)42064bytes 산식/64KiB control charge 및 temporary 실제 증명 방법을 수용하거나 수정해야 한다. 위 D2 검토는 계약을 수용했다. 실행 증거는 이후 구현 기록에 별도로 남기며 계약 채택을 PASS로 세지 않는다.

## 11. 구현 동반 동결 — GA08 compiler 입력

실제 API는 `PageConcurrency::new(&SessionFence) -> Result<PageConcurrency<'f>>`, `try_owner -> Result<Owner<'c,'f>>`, `Owner::try_apply/try_content(&mut self)`, `ApplyAdmission::try_version(self) -> Result<VersionEditGuard<'o,'c,'f>>`다. 오류는 `ConcurrencyError { cause, effect }`이며 위 우선순위를 따른다. 실제 Commit 설치로 가는 일반 setter는 없다.

[compiler fixture](../../crates/engine/tests/fixtures/dev3-concurrency/compiler/manifest.json)는 baseline·release 성공2개와 negative10개를 동결한다. manifest SHA는 `bc657bb94f20dde79cbe74c9cf0804c2939876355c94d8dbe2f185c7ff8d70a2`다. `owner_send/owner_sync`는 E0277, `apply_reborrow/content_reborrow/version_reborrow`는 E0499, `owner_drop/core_drop/fence_drop`는 E0505, `owner_clone/pending_setter`는 E0599를 각각 입력10행의 단일 primary span에서 요구한다. guard는 실제 Owner의 독점 borrow를 포함하여 non-Send/non-Sync를 이어받으며 세 reborrow 입력이 그 borrow의 생존을 검증한다.

[새 runner](../../scripts/check_dev3_concurrency_borrows.py)는 보존 source에서 먼저 원래 lib baseline을 컴파일하고, lib에 probe module 하나만 추가한 사본을 검사한다. cargo-check metadata이며 런타임 실행 파일이 아니다. 기존 공통 diagnostic/artifact 보존 helper를 재사용한다. [새 checker 시험](../../scripts/test_check_dev3_concurrency_borrows.py)은 manifest의 누락/중복·오류코드/span/예산 변조·fixture 변조, baseline을 한 번 포함하는 선택, 누적30초 budget만 검증한다. 입력과 checker의 독립 검토 뒤 실행하며 채택 자체를 PASS로 계산하지 않는다.


## 12. 구현 동반 동결 — cleanup·F05·runtime 수신

`try_version`은 실제 gate를 획득하기 전에 admission을 반환 wrapper에 옮기고 gate를 얻는 즉시 그 wrapper에 부착한다. 이후 재검사나 test-only fault가 unwind해도 fence를 먼저 설정한다. 예상 Busy/InstallationPending을 반환하는 경로도 state guard를 놓은 뒤 실제 gate→명시 admission cleanup 순서로 끝낸다. cleanup이 처음 관측한 Fenced/Poisoned/Invariant는 Unchanged를 CallerMustResolve로 높이며 기존 fatal cause는 보존한다. Drop과 명시 cleanup은 같은34-slot 독립 회계 검사 뒤 정확한 자기 회계만 반환한다. cleanup의 마지막 관측 뒤 외부 fence까지 소급 보장하는 의미는 아니다.

동시 locals의 source 검토 경로는 `try_version→acquire_version→entry_state→valid→accounting_valid`와 `drop/reject→release_checked→release_admission→cleanup_state/accounting_valid`다. retained control7248 외에 가장 큰 guard의 move/return scratch384와 다음 네 묶음 각128을 합한896bytes로 호출당1024 ceiling을 보수 검산한다. borrowed lock/result carriers는 State guard2×16, lock result24, version try result24, cleanup tuple24, small result/error3×8이다. census는 iterator24·item24·slot16·index4×8·u16 count2×2·bool8로108, borrowed reference/option은16×8, 나머지 scalar/error/branch carrier는16×8이다. State/배열 clone은 없다. 이는 명시 Rust 값의 논리 상한이며 컴파일러 stack spill·unwind machinery 실측으로 해석하지 않는다.

constructor는 slots544+State2×1024+Mutex<State>1040+core2048+Result<core>2056+scalar96=7832bytes≤8192로 검토한다. [자원 시험](../../crates/engine/src/transaction/concurrency/tests/resource.rs)은 각 concrete carrier 크기를 실제 target에서 검산하고34-owner·33-content·1-version 교차 상태와 완전 반환을 관측한다. 상위 caller34 제한과 전체64KiB ingress는 별도 소비 공급 의무다.

[runtime runner](../../scripts/check_dev3_concurrency_runtime.py)는 정상 fixture의 각 GA01–07 그룹을 exact selector로 실행한다. 실제 실행 checker/helper hash와 보존 source의 동일성, source+실행 파일을 연결한 no-run build receipt, 실행 파일·fixture hash를 먼저 확인한다. 전체 runtime은120초 누적 process-group watchdog이며 성공도 정확한 selector·libtest1건 PASS·fixture case 마커·완료 마커 census가 일치해야 한다. [새 checker 시험](../../scripts/test_check_dev3_concurrency_runtime.py)은 좁은 선택·잘못된 fixture·누락/중복/타 selector·불일치 build receipt를 검증한다. S5 확대512 입력은 별도 ignored selector로 남기며 중간 필수 실행과 합산하지 않는다.


## 13. Runtime 입력 r2 동결

실제 [정상 fixture](../../tests/fixtures/dev3-concurrency/normal-cases.tsv)는 GA01–07의45개 named case(10/2/4/4/10/12/3)다. [manifest](../../tests/fixtures/dev3-concurrency/manifest.json)는 normal45·expanded512와 각 파일의 SHA를 기록한다. 별도 입력 원본 `artifacts/dev3/resume-20261022/guard-tests-input-r2/plan.md`의 SHA는 `4eb5fff1d191e7a49aafe223109fcd983b6b04615039f9f5b3a52c726c569bb2`이며 r0/r1을 보존한다. 정상45의 실행은 `transaction::concurrency::tests::concurrency_ga01_capacity_and_generations`부터 GA07의 해당 exact selector7개다. source·ELF·log와 실제 완료 수는 실행 receipt가 소유하며 이 입력 동결로 PASS를 만들지 않는다.

GA02는 실제 version 보유 뒤 새 content 진입을, GA04는 Pending 뒤 foreground/maintenance/flusher 새 content 진입과 flusher InvalidRole 우선순위를 관측한다. GA05는 owner 포함 모든 actual guard의 unwind에서 fence-before-release를 관측한다. GA06은 Busy/Pending 결정 후 cleanup poison/overcount4개·첫 fatal cause 보존·cleanup 완료 뒤 fence를 별도 case로 둔다. blocked Drop은 state mutex 소유 중5초 noncompletion과 반환 뒤5초 이내 completion을 관측하며, blocking lock 실제 호출은 별도 source 검토로 확인한다. 이 관측을 OS 공정성 증명으로 확대하지 않는다.

GA07의 새 독립 oracle 부정 입력은20개다. 상태7·action trace5·selection/inventory4·관측 witness4(빠진 precheck, 빠진 postcheck, unlock-before-fence, state-held→version edge)를 각각 거절한다. 실제 controller ACK와 관측 cut 뒤만 trace를 기록한다. 동결512행은 iterator로 순회하며 decoded Schedule 최대2개(각384 ceiling), action trace10개의 bounded harness 공간만 사용한다. test harness 채널·OS stack은 product F05와 별개다.


실제 source별 결과와 남은 경계는 [단위 인수](../evidence/dev3-concurrency-admission-20261022.md)를 따른다. 계약의 후보/미실측 문구는 당시 동결 입력이며 실행 성공은 해당 receipt만 증명한다.

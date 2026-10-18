# DEV2 — SQL 없는 영속 커널 상세 설계

2026-10-18 · **D2 기본 설계 ACCEPTED / leaf 형식·실행 profile 동결 미완료 / P0/K0 일부 구현 / 영속 경로·제품 시험 NOT_RUN**

[구현 순서와 검증 계획](../plans/dev2-implementation-plan.md) · [커널 소유 계약](implementation-contracts.md) · [복구 소유 계약](recovery-logging-design.md) · [DEV2 완료 의무](../plans/stage-completion.json)

사용자의 DEV02 조사·설계 요청을 저장소의 **DEV2**로 해석한다. 대상은 SQL 없는 행 저장, 거래, 단일 WAL, 실제 재시작 복구, 동시성, checkpoint/restore다. 이 문서는 기존 K0–K5·B2/D0를 구체화한다. DEV3의 DISK/index, DEV4 SQL, DEV5 서버, L1 group commit, L2 multi-stream을 이번 단계에 추가하지 않는다. 향후 기능은 각 단계의 필수 의무로 유지한다.

아래 API 이름과 내부 형식은 구현을 위한 설계다. 현재 호출 가능한 API가 아니다. 특히 K2/K5의 byte offset·고정 hex vector와 실행 환경별 fault profile은 [구현 전 동결 항목](../plans/dev2-implementation-plan.md#freeze)을 완료해야 한다. 설계 검토와 계약 채택은 제품 gate 통과가 아니다.


<a id="basis"></a>

## 1. 조사 결과와 설계 출발점

| 확인한 현재 기반 | DEV2에서 필요한 추가 책임 |
| --- | --- |
| [foundation ID](../../crates/foundation/src/identity.rs): nonzero u64, checked overflow, generation/schema 검사 | 발급·재사용 금지·재시작 high-water, DB/거래 identity, 빈 commit 위치 |
| [값·codec](../implementation/dev1-implementation.md#section-3): 최소 scalar, 64 KiB 값, 1 MiB AXF1 payload | table schema와 행 직렬화, 완결 WAL batch·chain·commit 의미 |
| [FileIo](../../crates/foundation/src/io.rs): positional read/write/sync/set_len | namespace·create/open·writer lease·file inventory·원자 selector 교체 |
| [Linux adapter](../../crates/platform/src/linux.rs): 실제 파일 효과·directory sync | DB 디렉터리 소유, 잠금, rename·directory fault 관측, 재시작 연결 |
| [runtime](../../crates/foundation/src/runtime.rs): quota·lifecycle·취소·제한 event | DB 자원 회계, transaction/statement 효과, 실제 thread barrier와 pin/GC |
| [XADBTest protocol](../../crates/testing/protocol/src/lib.rs): CheckedAdd, 64 KiB 전체 입력, 최대 128 step | 여러 요청·응답의 실시간 이력, actor·crash barrier·큰 값·scan 페이지 |
| DEV0/1의 로컬 증거 | 독립 행/거래 oracle, 실제 engine worker, K0–K5와 PF 증거는 새로 필요 |

`FileIo::sync_all` 성공을 파일 이름·selector의 영속화까지 확장하지 않는다. Linux의 파일 fsync와 디렉터리 fsync는 별도이고, rename의 원자적 이름 교체만으로 crash 후 영속 선택이 완성되지 않는다. 이 차이를 namespace port와 checkpoint 순서에 반영한다. [fsync(2)](https://man7.org/linux/man-pages/man2/fsync.2.html), [rename(2)](https://man7.org/linux/man-pages/man2/rename.2.html)


<a id="architecture"></a>

## 2. 배치와 책임

기존 [저장소 정책](../policies/repository-build-plan.md#section-7)의 하나의 `xadb-engine`을 사용한다. 아직 빈 crate·폴더를 만들지 않는다.

```text
xadbtest controller ──→ xadbtest-reference (독립 상태/직렬 이력)
       │ typed protocol / 외부 ACK 이력
       └──→ xadbtest-engine-worker (OS 조립)
                    ├──→ xadb-engine ──→ xadb-foundation
                    └──→ xadb-platform ──→ xadb-foundation

xadb-engine: api → transaction coordinator
                  ├→ catalog / storage::volatile (준비된 변경)
                  ├→ durable::{wal,recovery,checkpoint}
                  └→ publication (하나의 불변 committed view)
```

`transaction`이 validation→준비→WAL→공개 순서를 소유한다. WAL이 storage manager를 호출하거나 storage가 WAL commit을 직접 실행하지 않는다. 같은 commit 상태를 여러 모듈에 복제하지 않는다. API는 원자 작업을 제공하고 caller에게 내부 단계 호출 순서를 맡기지 않는다.

`xadb-engine`은 platform·XADBTest·SQL·server를 import하지 않는다. 파일 effect trait는 foundation, 실제 Linux 효과는 platform, 조립은 worker에 둔다. reference는 protocol의 단순 입력/관측 타입만 공유할 수 있고 production 값 비교·row·codec·replay를 정답 계산에 사용하지 않는다. 현재 normal dependency 검사에 더해 새 crate의 normal/build/dev·feature 경로와 내부 module 참조를 구현 때 대조한다.

초기 자료구조는 **정렬된 행 참조 배열과 불변 row**다. table별 `Arc<TableRoot>` 아래 `Vec<(RowId, Arc<Row>)>`를 두고 변경 table의 참조 배열만 복사한다. 바뀌지 않은 table/row는 공유한다. root descriptor는 catalog와 table root의 작은 참조 집합이다. 이 선택은 표준 라이브러리로 구현·관측하기 쉽고 숨은 balancing 경로를 줄이지만, 변경 table마다 O(N_table) 참조 복사와 snapshot 메모리 비용이 있다. 이 비용을 PF workload로 측정한다. persistent tree는 B2의 필수 조건이 아니며 측정 후 D2로 비교한다. K0 volatile 행의 O(N_rows) 비용을 디스크 인터페이스의 강제 상주 조건으로 만들지 않는다.

<a id="api"></a>

## 3. 행·catalog API와 의미

DEV2는 고정 열 schema의 비파티션 table만 제공한다. schema는 1–32개 열, `Bool/Int64/Utf8/Binary`와 nullable flag다. NULL은 nullable 열의 값이며 독립 열 타입이 아니다. 값·UTF-8 byte 비교·overflow는 DEV1을 따른다. 자동 형 변환·SQL expression·secondary index·unique constraint·ALTER는 미지원이다. 중복 RowUID 방지는 기본 무결성이며 SQL UNIQUE 기능과 구분한다.

table/column 이름은 UTF-8 1–128 bytes, NUL 없음, byte 단위 대소문자 구별, 정규화 없음으로 시작한다. 열 이름 중복·table 이름 중복·schema와 값 수/타입 불일치는 effect 전 오류다. 이 내부 이름 규약이 DEV4 SQL 식별자 규약을 대신하지 않는다.

| 계획 API | 성공·실패 의미 |
| --- | --- |
| `Engine::create(directory, options, effects)` | 호출자가 제공한 빈 소유 디렉터리에 새 DB 생성. 기존 DB·알 수 없는 파일을 지우지 않음. bootstrap 선택 완료 후만 성공 |
| `Engine::open(directory, options, effects)` | 단일 writer lease→형식 검사→복구→새 epoch 연결→READY. 손상·미지원 형식은 정상 query 개방 전 거절 |
| `create_table(spec)`, `drop_table(handle)` | 각각 하나의 catalog 원자 거래. DML transaction 내부 DDL 요청은 거절. create는 새 identity 발급, drop은 새 root에서만 제거 |
| `begin(ReadOnly | ReadWrite)` | admission과 pin budget 확인 뒤 한 snapshot 고정. 같은 transaction을 동시에 여러 thread에서 변경하지 못하게 함 |
| `lookup_table(name)` | transaction의 catalog에서 handle을 얻음. 새 이름 조회가 오래된 객체를 되살리지 않음 |
| `read(table, row_id)` | snapshot+자신의 private 변경에서 조회. 없는 행은 `None`; stale/wrong-table handle은 오류 |
| `scan(table, after_row_id, limit)` | RowId 오름차순의 제한된 페이지. 모든 페이지가 동일 guard·snapshot·private overlay를 사용. EOF를 명시하며 resume token은 transaction 밖에서 재사용 불가 |
| `execute_statement(mutations)` | typed insert/update/delete 목록 전체를 한 statement로 준비. 결과 RowUID·영향 행 수 반환. 중간 오류는 statement 진입 상태로 복원 |
| `commit(transaction)` | 읽기 전용은 snapshot 위치로 종료. 쓰기는 아래 commit 상태 기계. transaction을 소비하고 결과를 하나만 반환 |
| `rollback(transaction)` | commit 시도 전 private 상태 폐기. 완료된 commit을 rollback하지 않음 |
| `checkpoint()`, `backup_at_checkpoint()`, `restore_new()` | K5에서 도입. 선택한 exact cut과 파일 closure를 결과에 명시. restore는 별도의 빈 소유 경로만 사용 |

mutation은 `Insert(table, values)`, `Update(RowUID, replacement_values)`, `Delete(RowUID)`다. Update/Delete의 없는 행은 명시적 `RowNotFound`로 그 statement를 실패시킨다. 덮어쓰는 upsert를 insert에 숨기지 않는다. 같은 statement가 한 행을 여러 번 바꾸면 입력 순서로 해석한다. 반환한 private RowUID는 commit 전 provisional이며 rollback 후 read 성공을 보장하지 않지만, 발급 번호 자체는 재사용하지 않는다.

`SnapshotGuard`와 scan cursor는 owning pin을 가진다. 공개 API에 row pointer·배열 offset·WAL 위치를 노출하지 않는다. table handle에는 DB identity·timeline과 현재 open instance의 비직렬화 자격 token, `ObjectHandle`을 묶는다. 유효성은 engine instance와 해당 transaction의 catalog에 대조한다. 재시작 뒤 handle은 다시 획득해야 한다. drop 이전 snapshot의 reader는 old table을 계속 읽고, 새 snapshot에서 old handle은 stale이다. 이름이 같은 새 table은 다른 identity다.

K0는 이 row/catalog 구현을 직접 시험하는 **내부 volatile substrate**다. K1의 volatile publication 결과는 내부 시험 결과 타입으로 제한하고 durable `CommitReceipt`를 반환하지 않는다. 최종 `Engine::create/open/commit`의 정상 성공은 K2 이후 local durable 의미만 갖는다. K0/K1용 성공을 사내 영속 DB capability로 승격하지 않는다.

<a id="identity"></a>

## 4. identity와 빈 DB

| 값 | 초기 설계 |
| --- | --- |
| `DatabaseId` | 생성 때 platform에서 얻는 128-bit random incarnation, bootstrap에 영속화. all-zero 거절. 외부 DB handle 혼용 검사 |
| `TableUID` | DB 범위의 ObjectId+Generation. DEV2는 ObjectId를 재사용하지 않고 Generation=1; schema epoch는 create 때 1 |
| `TxnId` | `(DatabaseId, timeline, writer_epoch, local_counter)`; 복구·진단 identity. SQL request 재전송의 exactly-once 키가 아님 |
| `CommitPosition` | `Empty` 또는 `At(CommitSeq)`. 파일의 0은 Empty, 첫 committed batch q=1. 기존 nonzero CommitSeq 생성자에 0을 넣지 않음 |
| `writer_epoch` | 새로운 writable open마다 증가시키고 admission 전 영속 예약. ID high-water와 recovery resume 연결의 소유자 |

ID는 **epoch별 큰 범위를 먼저 영속 예약**하는 안을 선택한다. ObjectId와 RowId는 각각 `epoch_u32 << 32 | local_u32`로 만들고 양쪽은 1부터 시작한다. epoch 예약은 모든 table의 RowId 발급 범위를 포함하므로 새 table마다 allocator 파일을 만들지 않는다. local counter는 프로세스 수명 동안만 증가하며 statement/transaction rollback으로 되돌리지 않는다. 빈칸을 허용하고 overflow는 wrap 대신 명시적 발급 중단이다. 한 boot의 전역 RowId 발급량은 2^32−1이며, 이 제한은 제품 용량 약속이 아니다.

새 epoch 번호와 연결 metadata의 파일 sync·원자 선택·directory sync를 모두 끝낸 뒤 handle을 외부에 반환한다. 예약 결과가 불명확하면 admission을 열지 않는다. crash 뒤 예약 범위는 전부 소비된 것으로 취급한다. 따라서 abort된 private 번호도 이후 재사용하지 않는다. checkpoint는 사용한 최대 epoch를 포함하고 recovery는 checkpoint·선택한 epoch chain·남은 생성 파일의 epoch와 충돌하지 않는 다음 값을 쓴다. 단순 `max(committed RowId)+1` 복구는 채택하지 않는다.

정상 restart는 같은 timeline, 새 writer epoch다. restore는 보존 row/table 숫자 identity를 유지하되 새 timeline과 checkpoint에 기록된 high-water보다 큰 epoch를 사용한다. RowUID의 비교·조회 namespace는 DB/timeline이며 그 안의 `(TableUID, RowId)`는 안정적이다. 서로 다른 복원 branch에서 같은 숫자를 발급할 수 있으므로 timeline을 제거한 전역 identity로 비교하지 않는다. 원본 handle·transaction·cursor를 복원 DB에 전달하면 instance/timeline 검사에서 거절한다. 이전 timeline의 transaction 결과/재전송은 자동 해석하지 않는다. DB 식별·epoch 산술·고정 vector는 K0에서 검증하고 실제 예약 영속화는 K2/K3의 완료 의무다.

<a id="transaction"></a>

## 5. snapshot·동시성·실패 효과

`CommittedRoot`는 commit 위치·catalog·row roots를 함께 가진다. reader는 짧은 `RwLock<Arc<CommittedRoot>>` read lock에서 Arc를 얻고 즉시 lock을 해제한다. root 아래 committed 데이터는 불변이다. transaction의 모든 읽기와 scan은 이 root와 private overlay를 사용한다.

writer는 commit coordinator에서 `start_position == current_position`을 확인한다. 다르면 **전체 transaction을 SerializationConflict로 ABORTED** 처리한다. 읽은 행만 검사하지 않고 DB 전체 epoch를 검사하므로 write skew·phantom·schema 변경을 보수적으로 막는다. 같은 snapshot에서 시작한 비충돌 writer도 한 명 외에는 실패할 수 있다. ReadOnly는 pin한 snapshot 시점에 직렬화할 수 있으므로 종료 때 최신 epoch와 같을 필요가 없으며 WAL/q를 만들지 않는다. ReadWrite의 무변경 종료도 validation 후 WAL 없이 종료한다.

statement는 private overlay의 savepoint/undo mark와 reservation 목록을 가진다. statement의 결과와 rollback 경로 자원을 먼저 확보하고, 오류가 나면 해당 statement에서 만든 변경·예약만 제거한다. 앞서 성공한 statement의 private 결과는 유지한다. rollback 자체가 새 allocation을 요구하는 설계는 거절한다. 전체 transaction 종료는 모든 private 예약과 pin을 해제하되 ID counter는 돌려놓지 않는다.

```text
Active → Prepared → CommitAttempted → Synced → Published → Replied
   └─ 준비/validation 오류 → Aborted
CommitAttempted 뒤 불확실한 I/O → Unknown + engine Fenced
Synced 뒤 내부 실패 → durable 결정을 취소하지 않고 Fenced/프로세스 종료
```

Prepared에는 전체 새 root·WAL bytes·결과 receipt·quota·계측 버퍼의 준비가 포함된다. q는 coordinator에서 `current+1`로 잠정 계산하고 준비 실패로 소비하지 않는다. CommitAttempted 이후에는 정상 writer를 차단해 q 구멍을 건너뛰지 못하게 한다. commit batch 첫 byte 쓰기를 시도하기 **직전** CommitAttempted로 전이한다. L0는 단일 완결 frame이 commit 단위이므로 본문 쓰기 오류도 보수적으로 UNKNOWN이다. append→sync 확인→전체 root 교체→성공 응답 순서다. publication lock 안에는 allocation·I/O·callback·await가 없고, 교체된 old Arc의 해제는 lock 밖에서 한다.

lock 순서는 `commit coordinator → retention ledger → publication lock`이다. snapshot/pin 취득은 `retention ledger → publication lock`을 사용하고 그 뒤 coordinator를 얻지 않는다. checkpoint의 chunk/manifest 작성 I/O는 pin 등록 뒤 lock 밖에서 한다. 최종 CURRENT 선택 I/O만 [선택 규약](#checkpoint-selection)에 따라 coordinator 안에서 직렬화하며 ledger/publication lock은 해제한다. transaction은 publication lock을 보유한 채 quota 예약·WAL 대기를 하지 않는다. coordinator는 L0의 WAL I/O 동안 보유하므로 latency를 계측한다. 진단 수집기나 callback이 이 lock을 역으로 얻지 않게 한다.

| 오류 분류 | transaction·엔진 상태 | 재시도 |
| --- | --- | --- |
| 입력·타입·RowNotFound·statement quota | 현재 statement만 복원, transaction Active | 입력 수정 후 새 statement 가능 |
| stale snapshot의 쓰기 validation | 전체 Aborted, 외부 공개 없음 | 새 snapshot으로 transaction 전체 |
| commit 전 취소/준비 부족 | 전체 Aborted, 예약 해제 | caller가 새 transaction 판단 |
| bootstrap/epoch/rotation/selector의 metadata 효과 불명 | 해당 transaction의 commit 미시도는 Aborted로 판정할 수 있으나 engine은 Fenced. 파일 chain 확인 전 새 write 없음 | recovery 후 새 engine |
| commit 시도 이후 write/sync 불명 | Unknown, 새 admission 차단, 기존 reader 종료/drain | recovery 후 결과 조사. 자동 재전송 금지 |
| sync 성공 뒤 publish 불능·panic/poison | commit 취소 불가, Fenced 또는 crash | 새 엔진으로 recovery |
| 필수 파일 손상·지원하지 않는 형식 | open 실패, 정상 서비스 없음 | 원인 해결. 자동 salvage/삭제 금지 |

engine 오류에는 code·statement/transaction effect·certainty·retry scope와 선택적 TxnId를 둔다. foundation의 `CallerMustResolve`를 무조건 `Aborted`로 번역하지 않는다. 내부 오류 ID의 최종 번호는 K0 vector에 고정하며 고객 공개 오류 catalog라고 표시하지 않는다. `Drop`은 private 메모리/lease/pin의 반환만 하고 commit·필수 sync를 하지 않는다. 취소가 CommitAttempted 이후 도착하면 현재 commit 판정을 끝내며 취소를 성공한 rollback으로 응답하지 않는다.

<a id="filesystem"></a>

## 6. filesystem port와 writer 소유

foundation에 작은 namespace effect 계약을 추가하고 platform이 구현한다. 임의 shell·범용 VFS/plugin framework를 만들지 않는다. 아래 동작은 계획이며 실제 Rust signature·오류 receipt는 K2 전에 고정한다.

| port 동작 | 의미 |
| --- | --- |
| `acquire_writer` → owning `WriterLease` | 영구 lock 파일의 nonblocking exclusive OS lock. 같은 DB의 두 번째 engine/process는 Busy. 파일 존재 자체는 소유권 증거가 아님 |
| `create_new`, `open_existing`, `metadata`, `list_owned` | 상대 파일명·종류·크기 확인, 배타적 생성, 순서에 의존하지 않는 제한 inventory. 기존 DB에서 open이 create로 바뀌지 않음 |
| `sync_file`, `sync_directory` | 파일 bytes와 namespace의 별도 영속 효과. 성공/실패 후 상태 불명을 보존 |
| `publish_selector(temp, selected)` | 같은 소유 디렉터리의 rename. temp sync와 이후 directory sync는 caller 프로토콜에 명시 |
| `remove_retired` | ledger에서 retiring으로 확정된 내부 파일만 제거. unlink 후 directory sync 실패도 기록 |

lease의 기반은 표준 `File::try_lock`을 우선 사용한다. Rust 공식 API의 안정화 표시는 1.89.0으로 현재 고정 toolchain 1.95.0보다 이르다. 복제 handle과 lock 수명은 platform 안에 감추고 지원 조합의 실제 두-process 배제 시험을 요구한다. 새 lock dependency는 이 설계에서 추가하지 않는다. [Rust File locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)

lock 파일을 정상 close/cleanup에서 unlink하지 않는다. 같은 경로에 다른 inode가 생겨 두 writer가 각기 잠금을 얻는 오류를 막는다. lease handle은 다른 소유 child로 넘기지 않으며 engine 수명 내내 유지한다. advisory lock을 무시하는 외부 writer·원격 FS는 이 지원 계약 밖이다. 프로세스 내부 여러 handle도 독립 lease로 중복 open을 막는다.

DB는 caller가 소유한 디렉터리이며 엔진이 생성한 단일 구성요소 파일명만 사용한다. 절대 경로·`..`·symlink·예상하지 않은 종류를 거절한다. 비협조적인 외부 프로세스가 동시에 디렉터리를 바꾸는 공격에 대한 완전한 sandbox라고 주장하지 않는다. unsupported format은 lock 소유 이외 DB 파일 변경 전에 거절한다. 생성 중 실패한 bootstrap은 명시적으로 불완전한 생성으로 남기고 자동 기존 파일 덮어쓰기로 재시도하지 않는다.

<a id="wal"></a>

## 7. L0 WAL 형식과 rotation

기존 [AXF1](../implementation/dev1-implementation.md#section-3)을 bounded outer envelope로 재사용하고 **별도 engine payload format**을 정의한다. AXF1에 WAL 의미가 이미 있다고 해석하지 않는다. SHA-256은 기존 채택 dependency를 사용하며 압축·암호화·알 수 없는 extension은 DEV2에서 거절한다.

| record | 반드시 저장·검증할 필드 |
| --- | --- |
| segment header | kind, engine format, required features, DB/timeline, writer epoch, segment ID, 이전 segment/epoch 연결, 시작 q, header digest |
| commit batch | kind, TxnId, prev_q, q, schema/catalog 상태, mutation 수·총 bytes, 확정 object/row identity와 after-image/tombstone, commit trailer |
| commit trailer | 고정 commit tag, 반복 q·mutation 수·batch 길이. outer checksum이 전체 header/payload/trailer를 검증 |
| segment seal | 마지막 유효 offset·q와 앞선 완결 frame chain의 digest. seal 뒤 일반 record 금지 |
| epoch resume | 이전 epoch tip의 파일 identity·검증 끝 offset·q·digest, 허용한 incomplete tail의 경계, 새 epoch/segment identity |

숫자는 little endian, bool은 0/1, value tag는 NULL/Bool/Int64/Utf8/Binary를 명시한다. 가변 값은 u32 byte 길이+원본 bytes이며 key encoding을 row 저장 codec으로 사용하지 않는다. count·length는 checked 연산 후 할당하고 row→mutation→batch 각 상한을 모두 적용한다. padding·unknown tag·중복 identity·trailing bytes·q 구멍은 오류다. 최종 field width/순서·magic·golden hex는 K2 codec leaf에서 동결한다. 여기의 필드 표만으로 구현 간 byte 호환을 주장하지 않는다.

하나의 transaction을 **하나의 AXF1 commit frame**에 넣는다. engine payload 전체가 1 MiB 이하이고 row의 canonical bytes는 128 KiB 이하, transaction 최대 256 mutation이다. 어느 한도라도 넘으면 CommitAttempted 이전 거절한다. 큰 transaction을 여러 부분 commit으로 분해하지 않는다. 입력의 중복 row 변경은 최종 after-image로 정리할 수 있지만 오류/statement 결과는 원래 입력 순서와 같아야 한다.

segment 목표 크기는 16 MiB이며 batch를 segment 사이에 나누지 않는다. header·seal 공간과 다음 batch가 들어가지 않으면 현재 segment의 seal→sync, 새 segment 배타 생성→header sync→directory sync, 이어서 **새 active segment를 WRITER에 원자 선택하고 directory sync**한 뒤만 다음 batch를 기록한다. 새 header는 직전 seal의 identity·끝 offset·q·digest를 참조한다. WRITER는 현재 epoch와 active segment를 선택하고 header의 predecessor chain으로 필요 prefix를 도달 가능하게 만든다. WRITER 선택 전 crash면 old sealed segment를 tip으로 복구하고 아직 ACK가 없는 새 파일은 orphan이다. 선택 후 crash면 새 segment부터 predecessor closure를 검증한다. 선택 결과가 불명이면 write를 받지 않는다.

같은 epoch의 segment 연결은 ID·prev digest·q로 확인하고 mtime이나 숫자가 가장 큰 파일만으로 committed tip을 고르지 않는다. 선택된 chain 밖 생성 파일은 orphan이며 추측 replay하지 않는다. checkpoint가 필요한 prefix를 덮기 전에는 손상 segment를 건너뛰지 않는다.

현재 build에서는 format 1만 허용하고 required features=DEV2 DISK L0만 지원한다. 미지원 format/capability는 변경 전에 거절한다. 이전 사내 format의 decoder·변환기는 만들지 않아도 되지만 현재 build의 손상·복구 검사는 유지한다. 고객 기준선은 [중앙 버전 정책](../policies/version-compatibility-policy.json)을 따른다.

<a id="recovery"></a>

## 8. recovery와 재장애

복구 순서는 `LEASE → PREFLIGHT → SELECT → LOAD → SCAN → APPLY → VERIFY → STABILIZE → RESUME → READY`다. SELECT에서 CURRENT가 선택한 checkpoint의 전체 closure와 [replay anchor](#replay-anchor)를 확인한다. K2/K3는 empty checkpoint C=0부터 전체 WAL을 replay하고 K5에서 실제 checkpoint를 추가한다. APPLY는 private recovery state에만 수행하며 READY 이전 정상 query를 받지 않는다.

`q = last_applied+1`, `prev_q = last_applied`인 완결 batch만 적용한다. stored after-image를 적용하고 SQL·현재 시각·ID 발급·증가 연산을 다시 실행하지 않는다. 동일 q/TxnId의 충돌은 오류다. `q≤C`는 checkpoint에 포함된 영역이며 올바른 manifest resume 위치에서 시작한다. 임시 apply progress를 영속 정답으로 사용하지 않으므로 재장애 때 같은 원본에서 다시 시작한다.

active 최후 segment의 미완결 frame은 **성공 sync bytes 보존을 전제한 동결 fault profile**에서만 무시한다. 필수 seal/resume 경계 뒤이고 후속 유효 record가 없음을 검사해야 한다. 완전한 길이의 checksum 불일치, 중간 구멍, sealed 영역 손상, 참조된 파일 누락, 잘못된 DB/epoch는 fail-closed다. 마지막 ACK bytes가 장치에서 사후 완전히 유실되는 경우를 checksum만으로 판별할 수 있다고 주장하지 않는다. 정상 open에서 원본 tail을 truncate하거나 재작성하지 않는다. 허용 결과는 [장애 판정표](#fault-outcomes)로 구분한다.

<a id="recovery-stabilize"></a>

### 8.1 채택한 이전 prefix의 영속화

**읽어서 검증한 bytes를 이미 영속화된 bytes로 간주하지 않는다.** 프로세스만 종료되면 sync 전 완결 frame이 OS cache에 남을 수 있다. 이 frame을 T1로 채택한 뒤 새 파일의 T2만 sync·ACK하면, 다음 전원 장애에서 T1만 사라져 T2의 필수 chain을 복구하지 못할 수 있다. 새 segment·WRITER·directory의 sync는 이전 WAL 파일의 data sync를 대신하지 않는다. [Linux fsync 계약](https://man7.org/linux/man-pages/man2/fsync.2.html)

STABILIZE는 writer lease와 source pin을 유지한 채, 선택한 checkpoint의 anchor부터 채택 끝 R까지 필요한 기존 WAL 파일 및 epoch 연결 metadata를 명시적으로 sync하고 필요한 namespace sync도 확인한다. 새 프로세스에는 이전 실행의 sync receipt가 없으므로 초기 구현은 해당 보존 파일을 보수적으로 모두 동기화한다. 대상 목록·identity·검증 끝 offset·digest를 고정하고, sync 결과가 불명이면 복구를 실패시키며 새 WRITER 선택·root 공개·정상 admission을 진행하지 않는다. sync는 내용을 바꾸는 truncate/overwrite가 아니므로 원본 bytes 보존 정책과 양립한다. 이전 I/O 오류 뒤의 재open을 포함하여 이 장벽이 검증한 bytes의 영속성을 보장하는지는 F03/F04의 실제 adapter·장애 profile에서 확인해야 하며, 단순 fsync 재시도 성공을 오류 복구 증거로 대체하지 않는다.

앞서 읽은 bytes의 영속성을 sync 성공으로도 보장하지 못하는 adapter/오류 상태는 정상 open 지원 범위로 승격하지 않는다. 명시 오류로 남기거나, 검증한 prefix를 새 파일에 영속 재작성·선택하는 별도 경로를 D2로 설계·검증해야 한다. 현재 L0에 그 대체 경로가 구현됐다고 가정하지 않는다.

```text
VERIFY(Checkpoint(C), complete batches C+1..R)
→ STABILIZE(required old WAL/epoch files and namespace)
→ RESUME(new epoch metadata, new segment, durable WRITER selection)
→ publish recovered root R → READY
```

장벽 전/중간/후 재장애 때는 기존 선택 자료에서 다시 복구한다. 새 epoch를 먼저 선택한 뒤 이전 prefix를 동기화하는 순서는 금지한다. K3는 `미sync 완결 T1 → process crash/cache 유지 → reopen → 새 T2 ACK → power-loss model/cache 소실 → reopen`을 필수 사례로 실행한다. T1이 최초에는 UNKNOWN이어도 첫 READY 이후 관측한 read와 T2의 의존성을 포함해 하나의 연속 이력으로 검사한다. 두 번째 복구에서 T1을 다시 임의로 제외하면 안 된다. 이 사례는 실제 power-loss qualification과 구분한다.

### 8.2 새 writer epoch 연결

**이전 tail을 그대로 둔 채 새 WAL을 쓴 뒤 다음 restart가 그 새 WAL을 놓치는 오류를 막는다.** STABILIZE 성공 뒤 RESUME은 검증한 old tip과 허용된 tail 경계를 가진 불변 epoch-resume record, 새 segment header, 새 epoch 예약을 생성·sync하고, `WRITER` selector를 temp sync→rename→directory sync로 선택한다. 이 과정 전체가 완료된 뒤만 recovered root를 공개하고 새 ID·거래를 받는다. 다음 recovery는 선택한 resume chain을 따라 old tip을 검증한 뒤 새 epoch로 이어 간다. successor가 있는 old incomplete tail을 허용하는 근거는 이 명시적 경계이며 임의 파일 검색은 아니다.

WRITER는 선택한 active segment·epoch·header/연결 identity를 소유하며 append 중 변하는 tip 전체의 고정 digest를 요구하지 않는다. 실제 완결 끝 R은 그 segment와 선행 chain을 검증해 결정한다. CURRENT는 checkpoint·fallback 및 각각의 replay anchor를 소유한다. K5는 WRITER를 checkpoint 시점의 값으로 되돌리거나 교체하지 않는다. 과거 chain의 검증 종료와 회수는 CURRENT가 인증한 anchor를 기준으로 한다. 두 selector의 조합과 전환 순서는 [선택·회수 상태표](#checkpoint-selection)에 따른다.

UNKNOWN은 자동 재전송하지 않는다. XADBTest는 controller가 알고 있는 TxnId와 독립 decoder/논리 상태로 전체 포함/미포함을 판정한다. DEV2에 무기한 transaction 결과 조회 API나 request dedup store를 약속하지 않는다. checkpoint 전후의 UNKNOWN 판정을 시험할 때 필요한 이력과 archive 범위를 controller가 보존한다. 운영 재전송 정책은 후속 API 계약이다.

<a id="fault-outcomes"></a>

### 8.3 정상 복구와 개방 거절의 인수 결과

| 장애 입력 | 필수 결과 | 시험 판정 |
| --- | --- | --- |
| 완결 frame만 남음; 미sync frame도 전체 생존 가능 | STABILIZE 후 연속 prefix로 개방. ACK 보존·ABORT 제외·UNKNOWN 전체 또는 없음 | `RECOVER_OK`; open 오류도 실패 |
| 필수 경계 뒤 active 최후 frame의 허용된 미완결 tail; 후속 record 없음 | tail bytes 보존, 앞선 prefix 복구·영속화, 새 epoch 연결 | `RECOVER_OK`; 허용 근거를 profile과 receipt로 확인 |
| 최후 미응답 거래라도 full-length frame의 checksum 불일치 | 원본 보존·명시 오류·정상 query 및 새 ACK 없음 | `REJECT_CORRUPTION`; 정상 복구 성공으로 집계하지 않음 |
| sealed/interior 영역 손상, anchor 불일치, 필수 chain 누락 | 명시 오류. 검증된 전체 fallback 경로가 있는 경우만 아래 fallback 규약 적용 | `REJECT_CORRUPTION`, 또는 사전 고정한 fallback 사례의 `RECOVER_OK` |
| STABILIZE/selector sync 오류·결과 불명 | admission 차단·GC 중단·원본 보존 | `REJECT_IO`; 오류 해제 뒤 별도 reopen 사례 필요 |
| 최초 create/restore 중단으로 선택에 필요한 파일/selector가 실제 누락; 이전 유효 DB 없음 | 불완전한 생성으로 거절. 기존 파일 자동 덮어쓰기·부분 DB 개방 없음 | `REJECT_INCOMPLETE_CREATE`; 완결 선택이 남은 경우의 정상 reopen과 별도 fixture |

full-length torn tail은 전원 장애의 결과일 수도 있다. 따라서 이 정책은 모든 전원 장애에서 자동 open을 보장하지 않는다. F04는 syscall write 길이와 실제 잔존 bytes·파일 길이를 분리하고 각 입력의 기대 결과를 실행 전에 고정한다. `RECOVER_OK` 사례를 사후 `REJECT_CORRUPTION`으로 바꾸어 PASS시키지 않는다. checksum 손상을 무조건 tail로 삭제하는 완화도 금지한다. 지원 환경의 자동 복구 범위를 확대하려면 torn-write 경계를 식별할 추가 영속 증거·형식과 동일 ACK oracle의 D2 검토가 필요하다.

<a id="checkpoint"></a>

## 9. checkpoint·backup·retention

K5는 `coordinator → retention ledger → publication`의 짧은 경계에서 **root C, C 직후 replay anchor, allocator high-water, 해당 WAL/epoch closure pin을 함께** 획득한다. 그 뒤 lock을 해제하고 catalog+rows를 불변 chunk로 직렬화한다. root를 pin한 뒤 나중의 WAL tail로 anchor를 바꾸지 않는다. checkpoint 중 새 commit이 같은 row를 바꿔도 pin한 C만 읽는다. chunk는 canonical bytes 최대 1 MiB, 하나의 row는 chunk를 넘기지 않으며 마지막 chunk는 작아도 된다. manifest는 DB/timeline·generation·C·allocator epoch high-water·schema·정렬된 파일 inventory/길이/digest·replay anchor·지원 feature를 담는다.

영속 순서는 **chunk create/write/sync → chunk 이름 directory sync → manifest create/sync → directory sync → CURRENT.tmp write/sync → rename(CURRENT) → directory sync → 새 선택 확정 → 회수**다. bootstrap에도 이 선택 규약을 적용한다. 각 syscall 전후와 일부 효과 뒤 error를 fault cut으로 둔다. CURRENT/manifest가 손상되면 자동으로 가장 오래된 generation을 열어 성공을 꾸미지 않는다.

<a id="replay-anchor"></a>

### 9.1 checkpoint가 인증하는 replay 경계

L0 manifest의 `replay_anchor`는 `(DB, timeline, writer_epoch, segment_id, header_digest, replay_offset, prefix_digest, q=C)`다. offset은 pin 시점에 선택된 segment에서 C 이후를 scan할 첫 frame 경계이며, prefix digest는 해당 파일의 `[0, replay_offset)` bytes를 검증한다. writer가 완결 경계마다 유지한 digest와 위치를 C의 pin과 함께 고정한다. header만 있는 segment나 C=0도 같은 규칙을 쓴다. bootstrap C=0의 header는 predecessor가 없는 genesis임을 명시한다. digest 계산은 streaming으로 할 수 있고 전체 segment를 RAM에 올리지 않는다. anchor 구간은 이후 append·seal·rotation에도 수정하지 않는다. field의 byte width·고정 vector는 F04/F05에서 동결한다.

recovery는 WRITER가 선택한 tip에서 predecessor를 역으로 확인해 **정확한 anchor segment에 도달할 때까지** 파일·epoch 연결을 검증한다. anchor의 header/identity·offset 범위·prefix digest를 검사한 뒤, 그 header가 가리키는 **더 이전 predecessor edge는 따라가지 않는다.** C 이하의 논리 상태는 검증된 checkpoint가 소유한다. 그 다음 anchor offset부터 순방향으로 `C+1..R`의 완결 frame·seal·epoch-resume을 검증하고 적용한다. q가 작다거나 파일이 없다는 이유만으로 중간에서 탐색을 종료하지 않는다. loop·anchor 미도달·q 역행·digest 불일치는 오류다. frame chain의 누적 digest는 anchor의 검증된 prefix 상태에서 계속 계산할 수 있도록 codec vector에 고정한다.

anchor segment는 C 이전 bytes를 포함해 **파일 전체를 보존**한다. 과거 파일의 제거는 그 segment의 predecessor부터 가능하지만, previous checkpoint나 다른 owner가 필요로 하면 제거하지 않는다. 각 checkpoint anchor에서 현재 WRITER까지의 segment·epoch metadata 구간과 consumer pin의 합집합이 실제 보존 범위다. manifest의 parent digest는 이력 identity이며, current/previous로 선택되지 않은 모든 과거 manifest의 재귀 보존 의무가 아니다.

예를 들어 `S0 ← S1 ← S2`에서 current anchor가 S2이고 previous anchor가 S1이면 S1·S2와 그 사이 epoch 연결을 보존한다. S0를 요구하는 다른 pin이 없으면 S0를 회수할 수 있다. S1 header의 S0 참조는 previous checkpoint가 인증한 경계에서 종료된다. 삭제 후에도 current와 previous 각각에서 최신 tip까지 복구해야 한다.

<a id="checkpoint-selection"></a>

### 9.2 CURRENT 선택과 WRITER 전진의 직렬화

checkpoint 하나만 build하고, 파일 작성은 coordinator 밖에서 수행한다. 선택 직전에 coordinator를 얻어 build pin·anchor·현재 writer chain의 연결, C가 현재 checkpoint보다 후퇴하지 않음, 엔진이 Fenced가 아님을 확인한다. 조건 불일치면 선택을 시작하지 않는다. retention ledger 안에서 old/new CURRENT가 요구하는 current·previous closure의 합집합을 임시 pin하고 lock을 해제한다. 이어 **coordinator만 보유한 채** CURRENT temp sync→rename→directory sync를 수행한다. commit/rotation/RESUME 및 GC의 Retiring 승인은 같은 coordinator를 사용하므로 이 구간과 겹치지 않는다. publication/ledger lock을 보유한 채 I/O하지 않는다. 기존 reader는 계속 자신의 root를 읽을 수 있다. 선택 지연은 checkpoint·commit wait 지표로 기록한다.

성공 뒤 ledger에서 새 current와 previous(전환 직전 current)를 설치하고 임시 pin을 해제한다. selector I/O 오류는 효과 유무와 관계없이 보수적으로 Fenced로 처리하고 old/new 합집합 pin을 유지한다. restart는 실제 남은 selector를 검증하며 디렉터리의 가장 큰 번호로 대체하지 않는다. checkpoint가 WRITER를 쓰지 않으므로 두 selector를 한 번에 atomic replace할 필요가 없다.

| crash cut / selector 조합 | 복구와 회수 의무 |
| --- | --- |
| build 또는 CURRENT rename 전 | 기존 CURRENT와 실제 WRITER로 복구. 새 generation은 미선택 staging |
| CURRENT rename 후 directory sync 전/불명 | 남은 old 또는 new CURRENT의 전체 closure로 복구. 양쪽 선택에서 가능한 ACK를 보존하도록 합집합 pin 유지; 새 write·GC 금지 |
| 새 CURRENT 선택 확정 후 GC 전/중 | 새 current·previous의 anchor 이후 chain을 보존. 이전 파일이 더 남아도 안전; 선택된 closure 누락은 오류 |
| build 중 rotation 완료 또는 선택 후 WRITER 전진 | old/new CURRENT 어느 쪽이든 해당 anchor에서 실제 WRITER까지 연결되어야 함. checkpoint 시작 때의 WRITER를 복원하지 않음 |
| rotation의 WRITER rename 후 directory sync 전/불명 | 기존 또는 새 segment tip 모두 checkpoint anchor로 이어져야 함. 새 segment ACK는 선택 확정 이후만 가능; write·GC 중단 |
| CURRENT는 유효하나 current manifest/chunk 손상 | CURRENT에 명시된 previous의 전체 상태와 그 anchor부터 같은 WRITER tip까지 검증·재생 가능한 경우에만 전체 fallback |
| CURRENT/WRITER 자체 손상, anchor 미도달, fallback 이후 chain 누락 | 정상 open 실패. 임의 generation 선택·손상 지점 건너뛰기 금지 |

최초 정책은 current와 직전의 검증된 checkpoint를 보존한다. CURRENT selector에는 current manifest와 정확한 previous manifest의 generation·파일 identity·digest를 함께 기록한다(bootstrap/restore 및 아래 fallback 복구 직후의 previous는 없음). 이 목록만 fallback 후보의 권위이며 디렉터리 검색으로 후보를 바꾸지 않는다. fallback에는 그 checkpoint 이후 모든 commit/epoch chain을 현재 tip까지 복원할 수 있는 closure가 필요하다. CURRENT의 directory sync가 불명이면 old/new 선택 양쪽에서 이후 ACK 전체를 복구할 closure를 보존하고 GC·새 write를 중단한다. fallback 후보 자체가 유효해도 chain이 빠졌다면 정상 recovery 실패다. 이름·mtime이 최신이라는 이유만으로 candidate를 선택하지 않는다. fallback 선택 규칙과 selector 손상 시 명시 오류를 K5 고정 vector로 대조한다.

fallback을 실제 사용했다면 SELECT에서 선택한 previous로 전체 상태를 재구성·검증하고 STABILIZE를 완료한 뒤, RESUME 전에 CURRENT를 `current=검증한 fallback, previous=없음`으로 temp sync→rename→directory sync한다. 원래 두 후보와 chain pin은 이 전환이 확정될 때까지 유지하며, 실패하면 READY/GC를 허용하지 않는다. 손상된 원래 current를 다음 checkpoint의 previous로 다시 등록하지 않는다. 이 복구 경로만 checkpoint cut의 후퇴를 허용하며, replay 끝 R과 모든 기존 ACK는 후퇴하지 않는다. fallback 부재 상태를 진단하고 다음 정상 checkpoint가 검증된 previous를 다시 만든다. 원본 손상 파일의 자동 수정·삭제는 하지 않는다.

retention ledger의 owner는 current/fallback generation, active snapshot, checkpoint build, backup, replay range다. snapshot reader는 old row memory를 pin하고 필요하지 않은 WAL까지 무조건 pin하지 않는다. backup은 해당 checkpoint의 manifest·chunk closure를 pin한다. WAL은 checkpoint/fallback/replay 의무의 보수적 low watermark로 whole segment 단위 보존한다.

GC는 `coordinator → retention ledger`에서 Fenced가 아님과 pin 없음·모든 anchor closure 제외를 확인한 후 `Retiring`으로 바꾸고, 이후 새 pin을 거절한다. 실제 삭제는 lock 밖에서 수행하고 완료 상태를 다시 기록한다. Fenced 이전에 승인된 삭제는 old/new 합집합 밖의 파일에 한정되므로 완료되어도 안전하며, Fenced 뒤 새 Retiring 승인은 금지한다. 실패·crash로 남은 파일은 누수 후보일 뿐이며 live 파일을 대신 지우지 않는다. 파일 이름과 generation ID를 재사용하지 않는다. restart는 실제 selector·anchor·필수 chain으로 ledger를 재구성한 뒤만 GC를 허용한다. checkpoint 도중 새 pin과 retiring 경합은 독립 schedule 사례로 검사한다.

backup은 copy 전 pin을 얻고 immutable inventory+checksum을 검증한다. `restore_new`는 빈 소유 디렉터리에 복사→전체 검증→새 timeline/epoch→원자 선택을 마친 뒤 개방한다. exact cut C만 복원하므로 source WAL/replay anchor나 backup 후 ACK를 복원 의무로 가져오지 않는다. 복원 manifest는 C의 상태와 source provenance를 담고, 새 timeline의 빈 segment header 뒤를 q=C인 새 anchor로 삼는다. 새 header의 predecessor는 명시적 restore 시작이며 source epoch chain에 연결하지 않는다. 새 CURRENT와 WRITER 및 namespace가 모두 영속화되기 전에는 성공·새 write를 허용하지 않는다. 최신 R/PITR/복제는 DEV8의 별도 closure·인수 의무다. source backup은 restore 중 crash에도 수정하지 않는다.

<a id="budgets"></a>

## 10. 초기 한도와 자원 정책

다음은 **첫 구현과 합성 인수용 공학적 기본값**이다. 실측 제품 최대 용량·SLA가 아니며 실행 후 유리하게 축소하지 않는다. 적용할 값·feature·환경은 run profile에 별도로 고정한다.

| 항목 | 기본값·정책 |
| --- | --- |
| table/column/name | DB당 64 table, table당 32열, 이름 128 UTF-8 bytes |
| 값/row | 기존 가변 값 64 KiB; canonical row 128 KiB |
| transaction | 256 mutation, encoded WAL payload 1 MiB, private workspace 8 MiB |
| 동시 사용자 | active transaction 최대 32, user snapshot/cursor pin 합계 32; cursor는 transaction pin 공유 |
| committed+old row state | 회계한 고유 allocation 합계 64 MiB; 공유 row bytes는 한 번 계산, 새 참조 배열은 별도 계산 |
| 전체 private 예약 | 64 MiB; transaction별 한도와 둘 다 적용 |
| checkpoint/recovery scratch | 별도 16 MiB; 복구된 row 상태는 resident 예산에 포함. decoder/manifest 크기도 제한 |
| 시스템 pin | checkpoint 1, backup 1; user pin과 별도 예약. recovery source pin은 시간으로 해제하지 않음 |
| pin 시간 | 첫 버전은 자동 reader 강제 만료 없음. count/byte 한도로 새 pin/write를 거절; 60초 초과 pin은 진단만 함 |
| WAL/metadata | segment 목표 16 MiB, 보존 WAL 256 MiB, manifest 1 MiB, 내부 파일 inventory 4,096개 |
| 진단 | event 최대 4,096개, trace 총 4 MiB; drop count 공개. 필수 fault receipt 유실은 시험 실패 |

예약 한도는 실제 소유 allocation과 연결한다. root를 교체해도 old pin이 있으면 memory를 반환한 것으로 세지 않는다. 동시 복구·checkpoint workspace를 중복 예약하지 않으며 부족하면 작업 시작 전 거절한다. WAL 포화 시 checkpoint 시도 또는 새 write 거절로 처리하고 필수 WAL 삭제로 계속 쓰지 않는다. manifest/inventory 한도가 서비스 상태에서 돌연 초과되지 않도록 새 객체 생성 전 계산한다. metadata/정리용 공간도 예약하지만 실제 장치의 ENOSPC는 정상 실패 모델에 남긴다.

`Vec::try_reserve`와 quota가 Arc/allocator·OS OOM 전체를 복구 가능하게 만들지는 않는다. allocation inventory와 별도 RSS 관측으로 이 차이를 드러낸다. OOM/비정상 panic은 owned process crash로 다루고 ACK→reopen으로 검사한다. 공유 host를 소진시키는 시험을 하지 않는다.

<a id="oracle"></a>

## 11. 독립 oracle와 외부 history

XADB-004를 production K0에 앞서 구현한다. reference는 작은 명시적 row/catalog 집합과 commit history를 유지하고 표준 scalar를 자체 해석한다. literal boundary vector와 수작업 직렬 이력에서 출발한다. production root·codec·compare·replay를 호출해 기대값을 만들지 않는다. 같은 codec의 round-trip만으로 판정하지 않는다.

각 operation에는 actor, transaction, statement, invocation/response 순서, snapshot token, 실제 값/오류를 남긴다. controller는 **응답을 읽는 즉시** 자신의 append-only history에 기록한 뒤 다음 crash를 허용한다. worker의 `synced=true`·q·snapshot token은 진단 witness다. oracle는 begin의 외부 호출/응답 구간과 그때 가능한 committed 상태에서 snapshot을 독립 판정하고 이후 모든 read와 대조한다. history와 fixture는 worker 권한/입력과 분리하며 expected 값·최종 verdict를 worker에 주지 않는다. 로컬 controller 분리는 보호 CI 자격이 아니다.

```text
A = controller가 성공 응답을 관측한 write transaction 집합
B = 확정 ABORT 응답 집합
U = 응답 미관측 또는 UNKNOWN인 시도 집합
C = 독립 검증한 완결 commit 집합
A ⊆ C, B ∩ C = ∅, C ⊆ A ∪ U
RecoveredState = ApplyInCommitOrder(C), 모든 transaction은 전체 또는 없음
```

행의 최종값만 비교하면 뒤 transaction이 덮어쓴 ACK 유실을 놓칠 수 있다. controlled workload는 transaction별 식별 가능한 witness row를 남기며, 독립 WAL decoder의 q/TxnId 집합도 비교한다. 이 decoder는 production recovery를 대신하지 않고 **실제 open 결과와 둘 다** 확인한다. checkpoint가 로그를 회수한 사례는 cut C의 독립 기대 상태와 보존된 controller history/closure를 사용한다.

UNKNOWN 가능한 여러 이력 중 하나라도 허용되는지 판정하되, 후보 공간을 초과하면 INCONCLUSIVE/오류로 남긴다. 전부 abort로 가정하거나 어떤 상태든 허용하지 않는다. serializable 검사는 실제 invocation/response의 선후 관계, 고정 snapshot 읽기, committed mutation의 직렬 순서와 statement effect를 함께 대조한다.

XADBTest engine protocol은 기존 fixture protocol과 별도 versioned envelope로 확장한다. 요청/응답 한 frame 최대 4 MiB, outstanding request 최대 32, run operation 최대 1,024, scan 응답 canonical payload 256 KiB 이하의 페이지로 제한한다. JSON 문자열 escaping을 포함한 wire 크기를 effect 전에 검사하고 큰 statement는 protocol 한도를 넘으면 실행 전 거절한다. 64 KiB 값 경계는 작은 단독 요청으로 검증한다. 모든 engine 내부 한도 조합을 단일 protocol request로 보낼 수 있다고 약속하지 않는다.

controller의 현재 worker 종료 후 전체 stdout 판정을 그대로 쓰지 않는다. bounded streaming reader·request ID·부분 frame·timeout·유실/중복 응답 검증을 추가하고 stdout 총량/이력 저장량도 run profile에 제한한다. 다른 actor의 응답 역전은 허용하며 ID 매칭과 해당 actor/transaction의 요구 순서를 검증한다. named fault point의 도달 확인과 controller의 release/kill handshake를 사용한다. `sleep` 시간이나 worker 자기 보고만으로 특정 crash cut을 달성했다고 판정하지 않는다.

<a id="decision"></a>

## 12. D2 결정 기록 DEV2-DESIGN-001

| 항목 | 내용 |
| --- | --- |
| 상태·일자 | ACCEPTED · 2026-10-18. 이 문서의 기본 설계·단계별 구체화 방향 채택; F01–08의 byte vector/실행 profile 완성이나 구현 착수 gate 통과를 뜻하지 않음 |
| 담당 | 작업·계약·통합 `/root`; 별도 검토 `/root/dev2_design_review` |
| 권한 근거 | DEV02 조사·설계 요청과 DP01의 기존 방향 안 D2 자율 구체화. 제품 방향·품질·고객 호환 의무 변경 없음 |
| 선택 | 하나의 engine, 표준 배열 COW, coarse validation, epoch range ID, L0 완결 frame, immutable generation, 독립 streaming history |
| 비교 대안 | 모듈별 crate는 초기 조립/순환 관리 증가; lock-free/persistent tree는 초기 증명·dependency 비용; fine-grained/group commit은 predicate/merge 계약 추가; in-place undo는 기존 private/COW 계약 변경 필요 |
| 선택 이유 | 한 commit·snapshot·recovery 경계에서 반례를 빨리 확인하고 현재 사용처에 필요한 port만 추가. O(N) 복사와 직렬 sync 비용을 명시적으로 측정 |
| 기존 계약과 관계 | implementation/recovery/disk의 불변식과 stage-completion 의무를 유지. 새로운 ID 배치·상한·API·filesystem 프로토콜의 구체안이며 과거 결정을 소급 대체하지 않음 |
| 새 의존성·unsafe | 이번 작업에서 없음. 구현도 표준 라이브러리+기존 dependency 우선; 추가가 필요하면 DP04 검토 |
| 검토 | `/root/dev2_design_review`가 소유 계약·실제 소스·초안/수정본을 별도 대조. rotation/선택, checkpoint cut, restore 자격, fallback identity, metadata 불명 처리 보완 후 기본안 채택을 막는 추가 사항 없음. [상세 기록](../plans/dev2-implementation-plan.md#review) |
| 검증 | V0 문서 검증 PASS. 실제 engine·fault·성능은 전부 NOT_RUN |
| 재검토 조건 | clone/pin 비용으로 기본 예산 초과, epoch/selector fault 반례, protocol 한도 불일치, 원본과 맞지 않는 오류 효과, D0 확장 경계 위반 |

byte format·selector 상태 공간·실행 profile의 미완성 항목은 [동결 목록](../plans/dev2-implementation-plan.md#freeze)에 남긴다. 구현이 없으므로 B2/D0·공식 배포는 계속 미완료다.

이 결정의 기본 구조는 유지하며, recovery/selector/회수의 상세 절차는 아래 DEV2-DESIGN-002가 보완·대체한다. 위 검토·검증 기록은 최초 설계 당시 범위다.

<a id="recovery-review-decision"></a>

## 13. D2 결정 기록 DEV2-DESIGN-002

| 항목 | 내용 |
| --- | --- |
| 제목 | 재장애 영속화와 checkpoint replay·회수 경계 구체화 |
| 상태·일자 | ACCEPTED · 2026-10-18; 독립 검토 후 계약·통합 담당 채택. byte vector·제품 실행은 NOT_RUN |
| 담당 | 작업·계약·통합 `/root`; 별도 검토 `/root/dev2_recovery_review` |
| 권한 근거 | 사용자의 DEV2 설계 리뷰와 문서 수정·개선 요청, DP01의 D2 위임 |
| 범위 | 이전 채택 prefix의 STABILIZE, L0 replay anchor, CURRENT 선택·GC 직렬화, fallback 재선택·restore 시작점, 장애별 인수 결과 |
| 선택 | 기존 WAL 파일의 명시 sync를 새 epoch 선택 전에 수행. checkpoint는 CURRENT만 갱신하고 manifest가 인증한 anchor에서 predecessor 검증을 종료. checksum 손상 거절 정책 유지 |
| 비교 대안 | 새 metadata만 sync하면 이전 미sync prefix가 소실될 수 있음. 매번 recovery 전체 checkpoint를 강제하면 K3가 K5에 의존. checkpoint마다 WRITER를 함께 교체하면 두 selector 갱신과 concurrent tip 보존의 상태 공간 증가. 과거 모든 WAL 보존은 유한 예산과 충돌 |
| 선택 이유 | 기존 private write·단일 L0·K0–K5 순서를 유지하며 ACK 의존성 영속화와 실제 회수 경계를 명시. selector 선택 동안의 coordinator I/O 비용은 측정 대상으로 노출 |
| 대체 관계 | DEV2-DESIGN-001의 §8 RESUME 선행 조건과 §9의 미정 chain 축약 절차를 구체화·대체. 기존 데이터/commit 의미·고객 호환·품질 의무 유지 |
| 관련 계약 | RC01/05/06/07/08, RC-T02–04/06–11의 필수 variant. 상세 [구현·검증 배정](../plans/dev2-implementation-plan.md#recovery-review) |
| 검토·검증 | `/root/dev2_recovery_review`가 계약·diff·반례·시험 기대 결과를 독립 검토. STABILIZE 전/중간의 torn image와 장벽 완료 후 RESUME cut을 구분하도록 RC-T04 수정 후 미해결 채택 차단 의견 없음. V0 문서 검증·diff 공백 검사 PASS. 실제 engine·fault·전원 장애·성능 시험은 NOT_RUN. 과거 모델/조사 증거 변경 없음 |
| 재검토 조건 | STABILIZE의 adapter 보장 불충족, anchor 단절·selector 경합 반례, 긴 선택 지연, full-length torn 자동 복구 확대, L1/L2로의 적용 확대 |

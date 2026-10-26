# XADB Rust 구현 계약

<a id="dev3-profile"></a>
## DEV3 적용 경계

[DEV3-DESIGN-003](disk-storage-design.md#decision)이 DISK 구현의 저장·동시성·복구 세부 규범이다. 아래 private/COW·global-q·불변 root 교체·exact checkpoint 설명은 **DEV2 기준선**으로 한정한다. DEV3와 후속 disk 소비에서는 다음을 적용하며, 기존 DEV2 코드/시험을 소급 변경하지 않는다.

| 경계 | DEV3 규약 |
| --- | --- |
| snapshot/공개 | 작은 q descriptor·undo horizon. page는 mutable이며 PageGuard pin과 content latch가 별개. TxnStatus와 commit frontier로 다중 객체 가시성 원자 공개 |
| 실패/statement | 영속 undo mark와 CLR로 복원 완료 후 오류 확정. rollback I/O 실패는 RecoveryRequired/Fenced이며 private 폐기로 대체하지 않음 |
| validation | 기본 Serializable의 table read/write-set 검증, 명시 ReadCommitted statement snapshot·row/key 잠금. 두 isolation 모두 [관측 행 버전](disk-storage-design.md#observed-row-version)을 잠금 획득 후 대기 여부와 무관하게 검증. DB 전체 q 비교 제거 |
| durability | update/undo/CLR redo를 포함한 WAL-before-page·steal/no-force·선택된 pending doublewrite. commit은 data 전체 sync를 요구하지 않음 |
| recovery | source WAL은 보존, home page는 redo/CLR로 변경 가능. [bootstrap/영속 예약 복원](disk-storage-design.md#recovery-reservation) 뒤 CLR 전에 recovery epoch/WRITER를 영속 선택. 정상 recovery와 full audit 분리 |
| checkpoint/backup | capture LSN과 ATT/DPT의 fuzzy metadata checkpoint; admission drain한 exact-C backup 별도 |
| 회수 | allocator·undo/status·삭제 항목의 horizon/reference aging. COW root/pack GC 없음 |

SQL 소비자의 기존 private/COW·global-q 전제도 이 경계로 대체한다. SQL 기본 isolation 변경이나 READ COMMITTED 문법 추가는 별도 SQL 결정이며, 인자를 생략한 API는 Serializable이다. DEV4의 Oracle 관련 값/이름/DDL 변경은 해당 소유 계약을 따른다. 상세 byte/API/fault 동결과 제품 실행은 미완료다.

[DEV3-REVIEW-004](disk-storage-design.md#architecture-review)는 다중 page 변경 순서·transient 참조 수명·undo 회수 상태·자원 벡터 예약·bounded DWB batch를 구체화한다. 세부 정본은 DEV3에 두며 이 문서의 아래 DEV2 전용 lock/COW 설명으로 덮어쓰지 않는다. [DEV4 초기 처리 범위](dev4-sql-design.md#capacity-boundary)는 해당 실제 공급과 cleanup 소유를 소비한다. 두 단계의 모델·제품 실행 상태는 각각 미완료다.


2026-10-18 · **DEV1 공통 기반 로컬 구현·검증, DB 커널은 설계 단계** · 주 개발 언어 Rust는 사용자 확정 사항이다.

[종합 설계](../reference/architecture-overview.md) · [실현 가능성 리뷰](../reference/architecture-overview.md) · [AI 개발 방법](../policies/ai-development-method.md)

이름·폴더·Rustdoc·검사 구분은 [코딩 컨벤션](../policies/coding-convention.md)을 따른다. 제품 소스 폴더는 `crates/foundation`·`crates/platform`처럼 역할로 명명하고 Cargo 패키지명 `xadb-foundation`·`xadb-platform`과 구분한다. 후속 커널 폴더는 `crates/engine`이며 아래 API·원자성·오류 계약은 이름 변경과 독립적으로 유지한다.


DEV0의 Rust 1.95.0·lockfile·최소 XADBTest는 [별도 계약](../implementation/dev0-implementation.md)으로 구현·로컬 시험했다. 공통 ID·오류·값/key/frame·Linux effect·quota·lifecycle의 최소 범위는 [DEV1 계약](../implementation/dev1-implementation.md)과 [기계 판독 계약](dev1-contract.json)으로 구현·로컬 시험했다. 아래 DB 커널 계약은 여전히 미구현이며 foundation 결과로 DB 검증 완료를 주장하지 않는다.


<a id="section-1"></a>

## 1. 언어와 구현 범위

엔진, XADBTest 실행기, 진단 도구의 주 언어는 **Rust**로 한다. Python은 설계 모델·분석·보고서 생성에 사용할 수 있다. 기존 C/C++ 엔진을 통째로 FFI로 감싸는 방법은 이 설계의 구현으로 보지 않는다. 외부 라이브러리가 꼭 필요하면 별도 adapter에서 소유권·오류·thread·버전 계약을 검증한다.

초기 core는 safe Rust로 작성한다. `unsafe`는 명시적으로 허용한 경계 모듈에서만 사용한다. 일반 core crate는 `forbid(unsafe_code)`를 기본으로 하고, 외부 의존성의 unsafe까지 없다는 뜻으로 표현하지 않는다. 예외에는 safety 조건, caller 의무, 동시성 모델, 검사 방법, 성능 근거가 필요하다. Rust toolchain·lockfile·feature 조합은 첫 빌드에서 고정한다. 이 문서가 특정 최신 compiler 버전을 곧바로 지원 제품 버전으로 지정하지는 않는다.

Rust가 줄이는 것은 일부 수명·aliasing·data race 오류다. commit 순서, snapshot 의미, deadlock, 저장 장치 영속성은 별도의 DB 계약이다. 안전한 Rust 코드도 잘못된 거래 결과를 만들 수 있다.

사용자 입력·I/O·quota의 정상 오류 경로에서는 `unwrap/expect`를 쓰지 않는다. 내부 불변식 assertion이 필요하면 실패 시 엔진을 재사용하지 않는 정책과 오류 증거를 정한다. 테스트 전용 assertion과 제품 오류 처리를 구분한다. hot path의 lock 보유 중 blocking/await·로그 allocation도 명시적으로 검토한다.

<a id="section-2"></a>

## 2. 모듈 의존성과 공통 의미

다음 화살표는 호출/의존 방향이다. 구현 파일마다 crate를 만들지 않고, 독립 계약과 빌드 경계가 필요한 묶음만 crate로 분리한다. 아래 이름은 종합 설계 모듈을 묶어 설명한 역할 이름이며 확정 Cargo package 이름은 아니다. [하위 계층 개발 결정](../plans/bottom-up-development.md)에 따라 storage core의 build/test는 SQL·network·server·driver·RP를 요구하지 않는다. 최소 row/catalog는 SQL 이전 typed API로 생성한다.

[저장소 구성과 빌드 정책](../policies/repository-build-plan.md)은 DEV2의 초기 배치를 `xadb-engine` 내부 api/catalog/storage/transaction/publication/durable 모듈과 별도 `xadbtest-reference`·`xadbtest-engine-worker`로 정한다. 이는 crate 배치 결정이며 아래 원자성·수명·영속성 계약을 변경하지 않는다. 역할별 crate 분리는 독립 빌드 경계가 필요할 때 검토한다.

<a id="module-transition"></a>
**DEV3 첫 Row의 모듈 전환 반영안 — `D3-MODULE-ADOPTION-001`, D2 ACCEPTED.** [공통 경계](dev3-module-boundaries.md#target-imports)는 같은 engine 안에서 물리 WAL과 semantic stage/install의 책임을 분리하고, 정상 원 owner·ledger·history·terminal 조립을 원 `plan4_pages::apply` 경계에 모은다. Catalog 및 두 Row 제품 본문은 각각 `storage::catalog`와 `storage::row::{insert,read}`가 소유하며 `storage::disk`의 제한 API만 수신한다. 이 배치는 신규 crate·전역 manager·두 번째 writer/pool을 요구하지 않는다. bootstrap의 legacy 내부 관계는 남는 범위로 기록한다.

물리 WAL의 complete stage/durable 관측은 semantic 설치 권한과 구분한다. apply의 private producer가 actual source·actor·whole targets·exact requirement를 검산하여 설치/terminal 권한을 만들고, 소비자는 raw scalar나 decoded bytes로 그 증거를 만들 수 없다. 공통 코드가 A의 INSERT/CLR 후보 또는 B의 snapshot/cursor 본문을 대신 소유하지 않는다. 구체 API·수명·오류·자원과 실제 인수는 [경계](dev3-module-boundaries.md#api-candidates)와 [두 소비자](dev3-module-consumers.md)에 둔다. 아직 채택·구현·제품 단계 완료의 증거가 아니다.

```text
server / CLI ──→ SQL binder / planner / executor ──→ transaction API
                                                   │
                               commit coordinator ─┼─→ private batch / snapshot
                                                   ├─→ WAL / checkpoint / recovery
                                                   └─→ storage / index / catalog
all core modules ──→ typed IDs / value semantics / errors / effect ports

XADBTest controller ──→ isolated worker ──→ same production core
         └──────────→ protected oracle / fixture / external observations
```

현재 공통 의미·effect trait는 `xadb-foundation`, 실제 Linux 파일·시간·scheduler 구현은 `xadb-platform`에 둔다. `xadb-platform → xadb-foundation`만 허용하고 향후 engine은 platform을 직접 import하지 않는다. worker/CLI/server 조립 지점에서 구현을 전달한다. 현재 OS 지원은 Linux이며 다른 OS는 adapter와 실제 인수 검증을 별도로 추가한다. [배치 정책 4.1절](../policies/repository-build-plan.md)을 따른다.

공통 의미 계층에는 다음을 한 번 정의한다. 공통 crate에는 의미와 경계 타입만 두고 상위 manager나 전역 가변 상태를 넣지 않는다.

| 계약 | 최초 결정 | 독립 검증 |
|---|---|---|
| 값과 비교 | 지원 타입, NULL, 정수 overflow, 문자열 정렬, 동일성 | 소수의 손으로 검산한 expected vector와 별도 단순 evaluator |
| key encoding | equality·ordering·hash·NULL 정책과 index byte 순서의 관계 | `cmp(a,b)`와 encoded key 순서 일치, hash/equality 일치. SQL NULL 비교와 unique의 NULL 정책은 별도 명세 |
| persistent codec | byte order, 길이·범위, schema/format ID, 지원하지 않는 값 | 경계값·잘린 레코드·과거 XADB vector. 임의 Rust struct layout 저장 금지 |
| 식별자 | ObjectId, generation, schema_epoch, commit_seq, row_id | 재사용·drop/recreate·오래된 plan/handle의 거절 또는 고정 snapshot 유지 |
| 오류 | code, transaction effect, retry scope, certainty | 같은 오류가 모듈·SQL·client에서 같은 의미를 전달 |

비교 함수와 index codec을 각각 개발한 뒤 우연히 맞기를 기대하지 않는다. 그러나 production codec을 그대로 reference oracle로 사용하면 공통 결함을 놓친다. codec round-trip만으로 SQL 의미가 맞다고 판정하지 않는다.

<a id="section-3"></a>

## 3. snapshot 공개와 객체 수명

초기 구현은 DISK row·index·locator·partition membership·catalog root를 모은 작은 **불변 `CommittedRoot` view descriptor**를 두고, `RwLock<Arc<CommittedRoot>>`와 같은 safe Rust 수단으로 공개하는 안을 사용한다. 이는 측정 전의 단순 구현 후보이며 lock-free가 요구사항은 아니다.

1. reader는 짧은 read lock 안에서 Arc를 clone하고 lock을 해제한다. 이후 transaction/cursor는 고정된 snapshot만 읽는다.
2. writer는 commit coordinator 안에서 시작 epoch를 검증하고 새 row·index·locator·membership·catalog root를 완성한다. 외부 입력, quota, allocation 등 실패 가능한 준비를 WAL commit record 시도 전에 끝낸다.
3. 완결된 WAL batch의 sync가 확인된 다음, 짧은 write lock 안에서 **전체 root 하나**를 교체한다. 이 구간에 I/O·callback·사용자 코드·await·일반 allocation을 넣지 않는다.
4. 교체한 이전 Arc는 lock 밖에서 처리한다. 마지막 reader/backup pin이 해제될 때까지 참조 대상은 살아 있어야 한다. 큰 소멸 작업이 commit critical section에 들어가지 않게 한다.

raw pointer를 먼저 읽고 나중에 refcount를 증가시키는 방식으로 최적화하면 그 사이 객체가 해제될 수 있다. lock-free 변경은 별도 ADR과 수명 증명이 필요하다. Arc만 쓴다고 interior mutability의 의미까지 안전해지는 것도 아니다. 공개 root 아래의 데이터는 논리적으로 불변이어야 한다.

`SnapshotGuard`는 타입 또는 owning Arc로 cursor 수명을 보호한다. RowId를 포인터로 캐스팅하지 않는다. 존재하는 객체의 state_version과 drop/recreate generation은 다른 축이다. catalog 변경 후 새 transaction은 새 root를, 기존 reader는 자신이 pin한 root를 사용한다. DDL이 오래된 plan을 재사용하게 하지 않으며, coarse epoch conflict가 schema 변경도 포함해야 한다.



RowUID와 수명에 묶인 직접 row reference도 구분한다. 이미 선택한 row는 같은 guard 안에서 빌린 참조를 재사용할 수 있으며 매번 RowUID resolver를 거치게 하지 않는다. 공개 API에 수명 없는 raw pointer를 노출하거나 물리 주소를 WAL identity로 쓰는 것은 허용하지 않는다. register/cache 최적화도 가시성·동시성에 필요한 load/동기화를 제거하는 근거가 아니다.

초기 lock 순서는 `commit coordinator → root publication lock`이다. reader는 root lock을 쥔 채 coordinator를 요청하지 않는다. snapshot acquisition은 짧게 끝내고 무한 reader 유입에 따른 writer 지연을 실측한다. poison/panic 이후 불변식을 증명하지 않고 shared state를 재사용하지 않는다.

<a id="section-4"></a>

## 4. statement, transaction, DDL의 실패 경계

private write batch만으로 statement atomicity가 완성되지는 않는다. 첫 Pilot의 기본안은 statement별 overlay/savepoint mark를 두고, 복구 가능한 statement 오류는 그 statement의 변경·할당·잠정 index 항목을 되돌리는 것이다. 이전에 성공한 statement의 private 변경은 유지한다. serialization conflict와 불변식 위반은 transaction 전체를 종료한다. 명시적 SAVEPOINT SQL은 별도 기능으로 출시할 수 있다.

| 사건 | 허용되는 결과 | 필요한 확인 |
|---|---|---|
| multi-row INSERT 중 unique/type 오류 | 해당 statement 전 상태 복원, transaction은 명시한 정책대로 계속 가능 | row·모든 index·quota·임시 객체 함께 복원 |
| commit validation conflict | 전체 transaction ABORTED, 전체 재시도 | 오래된 epoch의 쓰기 공개 없음 |
| commit record 시도 전 취소 | 미공개 batch 폐기, ABORTED | 이미 수행된 외부 효과 없음 |
| commit record 기록/flush 결과 불명 | UNKNOWN, 새 writer 차단 후 recovery로 판정 | ABORTED라고 확정하거나 무조건 재전송하지 않음 |
| durable commit 후 client disconnect | commit 유지, 응답 관측은 UNKNOWN일 수 있음 | 중복 요청 처리와 durable request ID 정책 |
| DDL 준비 실패 | 새 metadata/index 폐기, 이전 catalog 유지 | old/new 객체 수명과 생성 자원 회수 |
| DROP 후 이전 reader | 고정 snapshot 범위에서 유효, 새 접근은 generation 검증 | 오래된 cursor와 새 이름의 객체 혼동 없음 |

초기 **native DDL API**는 독립적인 원자 거래로 제한한다. 일반 DML transaction을 가진 채 native DDL을 호출하면 명시적으로 거절하며 engine에 숨은 implicit commit을 넣지 않는다. [DEV4-REVIEW-002](dev4-sql-design.md#oracle-review)의 SQL direct DDL은 별도 상위 조립 계약이다. SQL session이 지원 문법·진입 검사를 마친 뒤 이전 거래를 명시 commit하고 COMMITTED를 확인한 경우에만 독립 native DDL을 호출한다. 두 거래의 결과를 분리하며 DDL 실패가 이전 COMMITTED를 되돌리지 않는다. 세부 validation·취소·UNKNOWN 경계는 [SQL DDL 계약](dev4-sql-design.md#ddl-boundary)이 소유한다. 이 구체화는 DEV2 native API 동작을 바꾸지 않는다. 초기 offline index build·bulk partition DDL은 DB 전체 admission을 먼저 닫고 기존 사용자 drain을 확인한 다음 시작한다. timeout이면 build를 시작하지 않는다. online DDL은 이후 독립 상태 기계로 설계한다.

<a id="section-5"></a>

## 5. quota, allocation failure, panic을 구분한다

Rust의 일반 allocation failure는 보통 정상적인 `Result` 오류가 아니다. 표준 `handle_alloc_error`의 std 기본 동작은 프로세스 abort이며 구성에 따라 다를 수 있다. 따라서 quota를 설계했다는 이유로 OOM을 모두 복구 가능한 오류라고 약속할 수 없다. [Rust 공식 문서](https://doc.rust-lang.org/std/alloc/fn.handle_alloc_error.html)

| 분류 | 처리 계약 | 테스트 |
|---|---|---|
| 예상 가능한 query/transaction quota 초과 | commit record 시도 전 typed error; reservation 해제 | 작은 합성 budget, 각 단계 초과 후 자원 ledger |
| fallible allocation의 예약 실패 | 준비 단계에서 오류; 공개 상태 유지 | 준비 단계별 allocation failure injection |
| 숨은 allocation·OS OOM·비정상 panic | 일반 rollback 성공으로 표현하지 않음; 프로세스 장애로 취급 | 소유 worker의 제한된 crash, 재시작·ACK 보존 확인 |
| durable 이후 내부 실패 | commit 취소로 바꾸지 않음; 쓰기 차단 또는 프로세스 종료·복구 | UNKNOWN/ACK history와 recovery 대조 |

`try_reserve`를 일부 호출했다고 Arc/Box/formatting/dependency allocation까지 fallible해지지 않는다. 준비 경로의 allocation inventory를 만들고 실제 커밋 경로와 진단 경로를 조사한다. 예기치 않은 panic을 `catch_unwind`로 잡아 같은 엔진을 계속 쓰는 것은 기본 정책이 아니다. `Drop`에서는 새 allocation·panic·숨은 commit을 수행하지 않는다.

보수적인 메모리 계획은 다음과 같다. 공유 Arc가 가리키는 물리 메모리를 중복 합산하지 않는 측정 방식도 정의해야 한다.

```text
M_peak ≤ M_live + M_pinned_old + M_private_reserved
       + M_checkpoint_recovery_reserved + M_runtime + M_diagnostics_margin
```

이는 각 항의 상한을 구현·측정했을 때만 의미가 있다. snapshot/backup의 최대 유지 시간과 pin budget, recovery의 별도 headroom, 지나치게 큰 row/batch의 거절, 예약의 취소·해제를 포함한다. 오래된 snapshot을 종료시키는 정책은 사용자에게 명시하고 데이터 손상처럼 처리하지 않는다. 공유 host 전체 메모리를 소진시켜 시험하지 않는다.

<a id="section-6"></a>

## 6. 영속성의 관측 가능한 계약

`Prepared → commit-record-attempted → synced → published → reply`의 순서는 종합 설계를 유지한다. replay는 확정된 ID·값·catalog delta를 처리하고 SQL이나 시간을 다시 평가하지 않는다. 복구가 끝나 모든 불변식을 검사하기 전에는 정상 query를 받지 않는다. 필요한 durable 영역의 손상을 조용히 tail로 버리지 않는다.

H1/DEV3의 `Prepared`와 영속 순서는 [DEV3-DESIGN-003](disk-storage-design.md#commit)을 따른다. data page sync 대신 redo 가능한 update/undo·namespace를 준비하고 commit WAL sync로 ACK를 보호한다. WAL-before-page·doublewrite·CLR/recovery 인수는 필수이며 작은 view descriptor가 전체 disk metadata의 RAM 상주를 허용하지 않는다.

XADBTest controller가 실제 client reply를 받아 history에 기록한다. candidate가 스스로 출력한 `wal.synced=true`는 진단 witness이며 ACK 영속성의 독립 증명은 아니다. controller는 candidate 프로세스와 분리되고 crash 대상에서 제외된다. controller가 관측한 ACK를 보존한 뒤 candidate만 종료하고, 재시작 결과를 외부 expected history와 비교한다. controller까지 함께 잃은 경우 ACK 관측 여부를 미확정으로 남긴다.

process kill, 파일 API fault, power-loss model은 다른 시험이다. kernel/filesystem이 보유한 cache가 살아 있는 process kill로 전원 장애 내구성을 주장하지 않는다. 실제 OS/장치 조합의 지원 범위, flush·rename·directory sync 계약, fault model을 release별로 기록한다. synthetic short write/sync error도 성공한 write가 일부 남을 수 있다는 모델을 포함한다.

SQLite의 allocation/I/O 실패 주입과 FoundationDB의 simulation/native 시험 분리는 참고할 만하다. XADB에서는 지정한 failure model을 실제 core와 연결하고 동일 workload를 실제 OS에서도 확인한다. 기존 제품들의 검증 규모를 XADB가 이미 달성했다고 표현하지 않는다. [SQLite testing](https://www.sqlite.org/testing.html), [FoundationDB testing](https://apple.github.io/foundationdb/testing.html)

### 6.1 공통 recovery·logging·checkpoint 상세 계약

[통합 영속성 설계](recovery-logging-design.md)와 [기계 판독 계약](recovery-logging-contract.json)을 이 절의 상세 제안으로 적용한다. K2–K5는 단일 redo/commit stream인 L0를 사용한다. checkpoint는 정확한 committed cut의 불변 generation을 생성하고, 참조 파일·manifest·CURRENT 선택의 directory sync를 완료한 뒤에만 회수한다. 이전 generation으로 fallback하려면 이후 journal과 COW page closure까지 유지한다.

DEV2 recovery는 채택한 기존 prefix를 [STABILIZE](dev2-kernel-design.md#recovery-stabilize)로 영속화한 뒤 새 writer epoch를 선택하고 READY로 전이한다. checkpoint의 [replay anchor·선택 규약](dev2-kernel-design.md#replay-anchor)은 선행 WAL 검증 종료점과 current/previous의 보존 범위를 정하며 checkpoint가 WRITER tip을 되돌리지 않게 한다. [장애 결과](dev2-kernel-design.md#fault-outcomes)의 정상 복구와 손상/I/O 거절을 각각 인수하고, 거절 검출 PASS를 정상 reopen 증거로 바꾸지 않는다.

아래 L1/L2는 이전 private/COW 프로파일의 후속 제안이며 DEV3에 자동 적용하지 않는다. L1은 병렬 private encoding과 group commit, L2는 여러 payload lane과 전역 commit journal을 제안한다. L2의 commit record는 모든 payload와 COW 의존성이 영속화된 뒤에만 기록한다. lane별 최대 예약 offset을 durable prefix로 사용하지 않는다. fine-grained serializable validation과 직전 accepted root에 대한 merge 없이 coarse epoch 방식을 병렬 group commit으로 교체하지 않는다.

L1/L2는 첫 B2/S1/SQL의 선행 조건이 아니다. [28개 인수 계획](recovery-test-matrix.json)은 모두 제품 시험 미실행이며, 작은 Python 모델의 결과와 구분한다. 교차 버전 호환 대상과 VERSION_RELEASE 적용 여부는 중앙 [버전 정책](../policies/version-compatibility-policy.json)을 따른다.

### 6.2 Undo와 RP의 로그 보존 책임

[로그 보존 검토](replication-undo-analysis.md)는 undo 정보의 redo 기록과 rollback의 원본 로그 의존을 구분한다. undo tablespace 대안을 채택하면 undo chain·거래 상태·checkpoint를 통해 과거 로그 없이 undo할 수 있음을 검증해야 한다. RP의 capture restart pin은 별도이며 열린 거래의 분석 상태를 영속 인계하기 전에는 read/sent 위치만으로 회수하지 않는다. DEV2 private/COW 기준선은 유지한다. DEV3의 저장 ADR는 [DESIGN-003](disk-storage-design.md#undo)이며 영속 undo closure와 실제 fault 시험은 필수 미완료 의무다. [세부 계획](replication-undo-plan.json)은 초기 B2/SQL에 RP 구현을 요구하지 않는다.

<a id="section-7"></a>

## 7. 초기 구현과 전체 제품의 완료 기준을 분리한다

DEV2부터 실제 API의 계측·재현·비교 의무는 [AI 성능 개발 계약](ai-performance-engineering.md)을 적용한다. 각 소유 모듈은 시간·작업량·대기 의미와 코드/계약/benchmark 연결을 제공하고, WAL batch·공유 작업과 요청의 관계를 보존한다. 계측은 원자 공개·commit·오류/취소 의미를 바꾸거나 필수 동기화 역할을 해서는 안 된다. 실제 release의 계측 비용·유실·관측 상한과 관련 정확성/복구를 함께 검증한다. 기존 B0–B3의 의미를 대신하지 않으며 단계별 PF 인수 의무는 stage-completion.json에 연결한다.

하위 계층부터 개발하는 순서를 채택했다. 기존 XADB-007 예시는 첫 WAL 구현에 아직 없는 SQL server의 G2를 요구했다. 이 조건을 그대로 쓰면 후속 server가 WAL을 기다리고 WAL 작업도 server를 기다린다. 다음 **B 게이트**는 초기 개발용이며, 종합 보고서의 G0–G4 제품 gate를 대체하지 않는다.

| 게이트 | 선행 | 완료 범위·증거 |
|---|---|---|
| B0 | XADB-002/003 | 최소 build/runner, binary/case identity, wrong PASS 거절 |
| B1 | B0 + 해당 task | 해당 실제 모듈·독립 oracle·오류 경계. 모든 모듈의 일괄 완료를 뜻하지 않음 |
| F1 | B1, XADB-026 | 공통 ID/오류/값/codec/effect·Linux 경계, 상위 계층 없는 build |
| D0 | B1 | stable RowUID·snapshot·공통 commit·매체 독립 scope·versioned format |
| B2 | F1, K0–K5 + XADB-032 | 실제 row/txn/WAL/recovery/concurrency/checkpoint, SQL 없는 ACK→reopen |
| H1 | B2, XADB-021/022 | 실제 disk row page·locator·공통 commit/recovery |
| S1 | B2 + D0 + H1, XADB-028/029 | 유한한 DISK row/index/catalog·bounded RAM, 직접 API 통합 |
| Q1 | S1, XADB-011 | embedded 최소 SQL/CLI → 실제 디스크 저장 경로. network 불필요 |
| C1 | F1, XADB-031 | frame/transport/backpressure 독립 시험; storage 선행 조건 아님 |
| B3 | Q1 + C1, XADB-012/033 | 실제 server/session/client reply→SQL→WAL→reopen |
| CLIENT1 | B3 | 현재 build의 native/JDBC/ODBC 필수 API·오류·metadata·취소; 고객 교차 버전은 VERSION_RELEASE |
| SQL_EXT1 | Q1 + 기능별 task 선행 | DEV7 전체 required leaf의 의미·저장·서버·client 통합 |
| OPS1 | S1 + B3 + 기능별 task 선행 | DEV8 복제·PITR·운영·장시간·사전 등록 성능 기준 |
| P1 | S1, XADB-030 | SQL/global 없는 partition/local-index/row-move |
| H2 | H1 + P1, XADB-023 | SQL 없는 native global tree·unique·snapshot resolver |
| H3 | H2, XADB-024/025 | global과 membership·row-move·회수·용량 통합 |
| GLOBAL_INDEX_QUALIFICATION | B3 + H3 | 실제 mixed SQL/server/global/복구·해당 인수 사례. 이전 고객 버전 변환은 안정 릴리스 시 VERSION_RELEASE에서 추가 |
| VERSION_RELEASE | B0 + 현재 capability 시험 | 안정 릴리스 후보만 적용. 이전 안정 릴리스 same-major 계약 검증; 사내 배포는 비요구 |

단계별 필수 시험·합격 기준·미확정 입력·증거 규칙은 [개발 단계 10절](../plans/development-stages.md#section-10)과 [완료 계약 JSON](../plans/stage-completion.json)을 따른다. PRODUCT_QUALIFIED는 CLIENT1/SQL_EXT1/OPS1/P1 및 동결한 XADB capability inventory를 요구하며, 안정 릴리스 때 EXTERNAL_RELEASE가 VERSION_RELEASE를 추가한다.

첫 사내 커널은 B2/D0에서 현재 capability·복구·원자성을 검증하며 이전 사내 버전 호환 시험은 요구하지 않는다. SQL은 S1 저장 기준선 뒤 Q1에서, 서버는 Q1/C1 뒤 B3에서 연결한다. S1은 PART/NG·고급 저장 기능을 기다리지 않는다. H1/P1/H2/H3는 SQL이나 server를 기다리지 않는다. D0의 [7개 조건](../plans/phase1-product-baseline.md#section-7)은 설계 경계 검증이며 실제 미구현 기능의 PASS가 아니다.

XADB-007은 B0/B1에서 `MODULE_VERIFIED`가 될 수 있지만 `ACK-DURABLE-001`의 crash/reopen 증거는 B2까지 **미검증 의무**로 남는다. B2는 제품에서 사용하는 recovery 구현을 포함한다. 독립 log decoder가 맞다는 것만으로 실제 recovery 검증을 대체하지 않는다. B3 이후 해당 G gate를 적용하고, release가 요구하는 검증을 이후로 미루지 않는다. 사내 배포의 교차 버전 시험은 `NOT_REQUIRED_INTERNAL_FRESH_BUILD`로 기록한다. 안정 릴리스 후보에서 이전 안정 릴리스 binary가 없으면 `NOT_APPLICABLE_NO_RELEASE_BASELINE`이며, 이후 고객 minor에서는 모든 이전 same-major 안정 릴리스 버전의 보존 fixture/binary로 검증한다.

진단은 마지막에 붙이지 않는다. XADB-003과 XADB-005부터 manifest, stable error ID, effect receipt, 최소 bundle을 만든다. XADB-014a는 이 초기 기반이고, XADB-014b에서 자동 minimization을 추가한다. XADB-019a의 diff·계약·oracle 묶음도 초기부터 제공하고, XADB-019b에서 더 큰 읽기 전용 review view를 완성한다.

외부 출시는 별도 PRODUCT_QUALIFIED/EXTERNAL_RELEASE gate를 적용한다. [개발 단계](../plans/development-stages.md)의 기술적 선행 관계와 초기 DEV4 SQL scope를 따른다.

<a id="section-8"></a>

## 8. 구현 착수 시 남은 결정

DEV2의 구체화는 [커널 상세 설계](dev2-kernel-design.md)와 [구현·검증 계획](../plans/dev2-implementation-plan.md)을 따른다. API·ID 예약·L0·checkpoint의 설계 상태와 byte vector/실행 profile의 동결 의무를 구별하며, 아래 B2 이전 결정과 실제 제품 검증을 문서 작성으로 완료 처리하지 않는다.

| 결정 | 시점 | 판단 자료 |
|---|---|---|
| Linux x86_64·toolchain/배포판 pin 및 ext4 검증 | DP06에서 첫 공식 사내 대상 Ubuntu 24.04/x86_64/ext4 채택. 정확한 host image pin은 미완료. B2 WAL부터 I/O 검증·H1에서 disk page 확장 | [운영 방침](../policies/development-policy.md). Ubuntu 26.04.1/Rust 1.95.0의 DEV0 로컬 결과는 보존. 새 대상 qualification과 XFS 제품 검증은 미실행 |
| persistent map 구조와 root clone 비용 | B1 | correctness를 유지한 크기별 memory/latency 측정 |
| SQL 타입·collation·statement error 명세 | 각 기능의 B1 이전 | 손으로 검산한 examples와 명시적 unsupported 목록 |
| batch·snapshot·recovery budget | B2 이전 | peak memory, 오래된 pin, 실패 후 회수 측정 |
| 선언한 workload·SLA·RPO/RTO·driver 범위 | 외부 출시 전 | 제품 기능 inventory·실측. 미확정 항목은 외부 출시 차단 |

Rust 선택은 확정됐다. 세부 구현은 이 계약을 만족하는지 작은 커널에서 검증하고 결정한다. 이 문서의 검토 완료는 위 게이트의 실행 완료가 아니다.

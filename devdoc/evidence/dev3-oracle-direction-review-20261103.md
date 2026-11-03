# DEV3 설계 방향 리뷰 — Oracle 목표 기준

2026-11-03 · **정적 설계 리뷰 / 문서만 작성 / 제품 코드·시험 실행 없음 / 권고의 채택은 사용자 결정**

[저장 설계 정본](../contracts/disk-storage-design.md) · [16개 작업 명세](../contracts/dev3-task-designs.md) · [우선 실행 묶음](../plans/dev3-implementation-plan.md#work-priority) · [DEV3 진도표](../plans/dev3-schedule-baseline.md) · [10-25 방향 점검](xadb-direction-review-20261025.md) · [10-22 ARIES·Oracle 비교 리뷰](dev3-task-design-review-20261022.md) · [10-25 비교 분석 반영](dev3-comparison-adoption-20261025.md)

사용자 요청(2026-11-03, 프로젝트 thread): "dev3 전체의 설계를 리뷰해 주세요. 이미 구현된 부분과 남아 있는 부분에 대한 설계를 리뷰해서 문서로 PR 등록. 지금 맞는 방향으로 가고 있는 것인지가 중점. 목표는 오라클, 참고는 포스트그레스나 알티베이스."

<a id="summary"></a>
## 1. 결론

**설계 정본의 뼈대는 Oracle 방향과 맞다. 그러나 실제로 구현한 첫 Row profile의 모양과 몇 가지 거래 의미의 선택은 Oracle에서 멀어지고 있다.** 남은 번들 4·5를 지금 구조 그대로 넓히면 기능 목록은 채울 수 있어도, 로그량·동시성·공간 효율에서 Oracle과 비교할 수 있는 DBMS에 도달하기 어렵다. 방향을 바꾸는 비용은 지금이 가장 작다. UPDATE·overflow·index·split 형식이 아직 동결되지 않았고, 개발 fixture는 매번 새로 만들기 때문이다.

| 영역 | 판정 | 근거 |
| --- | --- | --- |
| 저장 구조의 뼈대(in-place page, 단일 redo, 영속 undo, steal/no-force) | 맞음 | Oracle의 datafile·redo·undo 분리와 같은 계열 |
| 행 단위 MVCC(VersionStamp, Status page, commit 번호 q, statement·거래 snapshot) | 맞음 | Oracle의 ITL·transaction table·SCN과 대응 |
| 복구(ARIES식 REDO, redo-only CLR, logical undo, redo-only split) | 맞음 | Oracle의 instance recovery, split을 따로 commit하는 recursive transaction과 대응 |
| UNIQUE NULL 규칙, statement 끝 제약 검사, DDL 전 commit | 맞음 | 이미 Oracle 의미를 택했다(PostgreSQL과 다름) |
| 구현된 로그 모양(바뀐 page마다 전체 image) | 이탈 | 정리까지 한 한 행 거래가 WAL 629,363 B. Oracle은 수백 byte 수준 |
| live 경로의 전체 WAL 재검증 | 이탈 | group마다 epoch WAL 전체를 다시 읽는다. 한 epoch 안 누적 비용은 거래 수의 제곱 |
| 전역 Root/Job page, 행마다 Undo page 하나 | 이탈 | 모든 Row 연산이 page 1을 바꾼다. 동시성 번들의 구조적 장벽 |
| 행·index 표현 크기 | 보정 필요 | 행 header 168 B, index 항목 값 160 B. Oracle 행 header는 수 byte |
| 거래 의미(RC 쓰기 충돌, 기본 격리, Serializable, cursor, deadlock, 거래 크기) | 결정 필요 | 여덟 항목이 Oracle 동작과 다르다([6절](#semantics)) |
| 남은 번들 일정 | 위험 | 08·11·14의 배정이 지금 구조를 넓히는 전제다 |

권고는 세 가지로 모인다.

1. **목표를 결정으로 남긴다.** 현재 [제품 방향](../plans/project-direction.md)과 [저장 정본](../contracts/disk-storage-design.md#scope)은 "Oracle과의 전체 SQL/내부 구현 일치를 선언하지 않는다"고 쓴다. Oracle을 목표로 한다면 따를 Oracle 동작([6절](#semantics)의 S1–S8)을 사용자 결정으로 고정한다.
2. **UPDATE(진도표 07) 전에 일반 경로 형식 단위를 하나 둔다.** 로그 모양, live 재검증의 위치, 정리 장부의 위치, 행 header의 거래 정보 위치, undo 배치를 일반 경로에 맞게 다시 정한다([5절](#structure)). 진도표는 V2로 다시 견적한다.
3. **동시성 의미를 07-1 UPDATE 계약 전에 정한다.** UPDATE가 RC 쓰기 충돌을 처음 만나는 연산이다.

정확성·복구·자원 한도를 먼저 보는 원칙, 독립 기대값, native fault cut은 그대로 유지할 가치가 있다. 이 리뷰가 권하는 것은 검증을 줄이는 것이 아니다. 검증 장치를 제품 경로에서 시험·검증 경로로 옮기고, 제품 형식을 일반 경로에 맞추자는 것이다.

<a id="scope"></a>
## 2. 범위·기준·방법

- **기준 source:** main `7692631`(PR #45 병합). 열린 PR #47(DELETE 3단계 native fault cut)은 반영 전이다.
- **읽은 문서:** 저장 정본, 16개 작업 명세 가운데 3.06·3.07·3.08·3.10·3.11·3.13·3.14·3.15, [자원 계약](../contracts/dev3-resource-bounds.md), [첫 profile 소비자 계약](../contracts/dev3-module-consumers.md#profile), [DELETE 계약](../contracts/dev3-row-delete.md), [F6 WAL 재검증 조사](f6-wal-revalidation-survey-20261102.md), 진도표, 이전 리뷰 세 건.
- **대조한 코드:** [첫 profile의 WAL 모양](../../crates/engine/src/storage/semantic_row/operation.rs)(`shape`·`native_mask`), Row 장부 상수(`retention_page/row/job.rs`의 `FORWARD_FIXED`, `semantic_retention.rs`의 `REDO_BYTES`).
- **외부 근거:** Oracle은 공개 문서(Database Concepts 11g R2 "Data Concurrency and Consistency", Administrator's Guide 21c "Managing Undo")와 Ask TOM, PostgreSQL은 공식 문서의 Transaction Isolation이다([11절](#sources)). Altibase는 이번에 source·문서를 다시 읽지 않았다. Altibase 쪽 비교(TSS·CTS·지연 stamping)는 [10-25 비교 반영](dev3-comparison-adoption-20261025.md)이 기록한 altidev4 조사(`0682bdd8`)의 범위에 기댄다.
- **실행:** 제품 build·시험·측정은 하지 않았다. XADB 수치는 문서·코드 상수와 기존 [F6 M0 실측](f6-wal-revalidation-survey-20261102.md#f6a-m0)에서 읽었다. Oracle·PostgreSQL 쪽 크기는 공개 자료 기준의 크기 차수이며 이 리뷰의 측정값이 아니다.
- **이전 리뷰와의 관계:** [10-22 비교 리뷰](dev3-task-design-review-20261022.md)는 구현 전 설계의 정확성 반례 8개를, [10-25 방향 점검](xadb-direction-review-20261025.md)은 README 취지와 절차를, [10-25 비교 반영](dev3-comparison-adoption-20261025.md)은 자원 산식을 다뤘다. 이번 리뷰는 처음으로 **구현된 모양**을 일반 설계·Oracle과 대조한다.

<a id="current"></a>
## 3. 현재 위치

진도표 V1 기준 적립은 29.5/86일 = 34.30%다(main `7692631`). PR #47이 병합되면 06-6이 적립돼 30/86 = 34.88%가 된다. 남은 중앙 예상은 56.5일, 중앙 완료 예상일은 2026-12-30이다([진도표](../plans/dev3-schedule-baseline.md#schedule)).

구현된 것은 [첫 지원 profile](../contracts/dev3-module-consumers.md#profile)이다.

- DB 전체가 첫 extent 64 page(1 MiB)다. page 0이 allocator, 1이 정리 큐 Root, 2–6이 Job, 7–8이 Catalog이고, 나머지 55 page에 table·Status·Undo가 들어간다.
- table 하나는 level0 leaf 하나다. split이 없으므로 table당 행 수는 짧은 행 85개, 2 KiB 행 7개가 상한이다.
- 지원 기능은 inline 행(≤2,048 B), index 0개, INSERT·DELETE, Commit, statement·full rollback, point·cursor 읽기, 복구 READY(REBASE·새 epoch·복구 CLR), live 정리, 자동커밋·명시 거래 실행기다.
- 규모는 `crates/engine` Rust 325,473줄(이름에 test가 든 파일 176,016줄), `storage/wal_io` 317개 파일, `devdoc` Markdown 8.4 MB·JSON 12.1 MB다. 10-25 점검 때는 engine 약 15.9만 줄, `wal_io` 124개 파일, Markdown 5.2 MB·JSON 11.3 MB였다.

<a id="aligned"></a>
## 4. Oracle 방향과 맞는 선택 — 유지

| XADB 선택 | Oracle | PostgreSQL·Altibase 참고 | 판단 |
| --- | --- | --- | --- |
| mutable page, 단일 WAL, steal/no-force. commit 때 data page flush 없음 | datafile과 online redo. DBWR 쓰기는 commit과 무관 | PostgreSQL도 같다 | 유지 |
| 영속 undo의 UndoRef chain이 rollback의 정본(WAL 역방향 chain이 아님) | undo segment가 rollback과 읽기 일관성의 정본 | Altibase 디스크 테이블도 undo tablespace 기반 MVCC. PostgreSQL은 heap 다중 버전 | Oracle 계열의 핵심. 유지 |
| 행 header의 VersionStamp(`Committed(q)` 또는 `TxnRef`)와 commit 뒤 stamp | ITL과 delayed block cleanout | Altibase의 CTS와 지연 stamping(altidev4 조사) | 방향은 맞다. 위치와 비용은 [5.5](#representation) |
| Status page의 거래 slot | undo segment header의 transaction table | Altibase의 TSS | 유지 |
| commit 번호 q. RC는 statement snapshot, Serializable은 거래 snapshot | SCN과 statement-level read consistency | PostgreSQL snapshot | 유지 |
| 읽기가 쓰기를 막지 않음 | 같다 | 같다 | 유지 |
| redo-only CLR, 현재 tree에서 대상을 찾는 logical undo, 사용자 abort와 무관한 redo-only split | index split을 recursive transaction으로 따로 commit | ARIES | 유지 |
| UNIQUE는 전체 NULL만 중복 예외이고 부분 NULL이 같은 tuple은 충돌 | 같다 | PostgreSQL은 NULL을 모두 다른 값으로 본다 | Oracle 의미. 유지 |
| UNIQUE를 statement 끝에 검사해 key 교환 허용 | statement 끝 제약 검사 | PostgreSQL은 행마다 검사한다(DEFERRABLE 제외) | Oracle 의미. 유지 |
| statement 실패는 mark까지 되돌리고 거래는 유지 | statement-level rollback | PostgreSQL은 거래 전체가 실패 상태 | Oracle 의미. 유지 |
| DDL 전에 진행 중 거래를 commit하고 DDL은 독립 거래([DEV4 경계](../contracts/dev4-sql-design.md#ddl-boundary)) | DDL implicit commit | PostgreSQL은 거래 안 DDL | Oracle 의미. 유지 |
| fuzzy checkpoint, DPT 최소 recLSN에서 redo 시작 | incremental checkpoint(checkpoint queue) | PostgreSQL checkpoint | 유지 |

이 판단은 10-22 리뷰의 "타당한 초기 선택"과 같다. 문제는 이 뼈대가 아니라 아래의 구현 모양과 의미 선택이다.

<a id="structure"></a>
## 5. 구조 — 구현된 첫 profile이 일반 경로와 Oracle에서 멀어지는 곳

<a id="wal-volume"></a>
### 5.1 로그량: 바뀐 page마다 전체 image

**관찰.** 첫 profile은 바뀐 page마다 16,344 B whole-body Patch를 쓴다. `operation.rs`의 `shape`에서 Forward의 redo는 5 page × 16,344 = 81,720 B다. frame마다 native before/after와 고정 header 3,776 B도 붙는다([WAL 모양 표](../contracts/dev3-module-consumers.md#row-shape)). 그래서 한 행 INSERT의 Forward 하나가 91,304+C B이고, Begin과 Commit이 각각 56,712 B, RowReserve가 57,336 B다. commit 뒤 live 정리도 AgingStart 19,672 B, Prune 89,496+2C B, ReleaseRole 3회와 RootFree 1회(4×55,528 B), TerminalFinish 55,112 B를 쓴다. F6 M0 실측으로 정리까지 한 한 행 거래는 WAL을 **629,363 B** 늘린다.

**비교.** Oracle의 작은 한 행 INSERT redo는 data block change vector와 undo block change vector를 합쳐 보통 수백 byte 수준으로 알려져 있다. PostgreSQL의 heap insert 기록도 행 크기에 수십 byte를 더한 정도이고, 전체 page image는 checkpoint 뒤 그 page의 첫 변경에만 붙는다. 같은 일에 XADB는 세 자릿수 이상 많은 로그를 쓴다.

**영향.** WAL 상한 256 MiB는 checkpoint·WAL 회수 없이 약 420개 거래면 찬다. index가 붙으면(09) 바뀐 page가 늘고, UPDATE(07)와 split(08)이 같은 모양을 따르면 더 커진다. 이 비용이 PF09(18)에서 처음 드러나면 그때는 형식이 모두 동결된 뒤다.

**권고.** 일반 [WAL 형식](../contracts/dev3-wal-format.md)은 이미 1..16,280 B의 작은 Patch를 지원한다. 형식이 막는 것이 아니라 첫 profile의 선택이다. UPDATE 전에 "바뀐 byte 범위만 Patch"를 일반 경로의 규칙으로 정한다. native before/after·inventory delta처럼 REDO에 필요 없는 검증 내용은 시험·검증 모드로 분리한다. PostgreSQL의 `wal_consistency_checking`처럼 검증 모드에서만 page image를 붙여 replay 결과와 대조하는 방식이 참고가 된다. 지금 frame의 항목 가운데 REDO에 꼭 필요한 것이 무엇인지는 이 리뷰에서 끝까지 추적하지 않았으므로 D2 검토 대상이다.

<a id="live-replay"></a>
### 5.2 live 경로의 전체 WAL 재검증

**관찰.** Row group을 만들 때마다 `replay_durable`이 현재 epoch WAL을 처음부터 다시 읽고, hash하고, 적용한다. 한 행 commit 거래는 전체 replay를 4회 하고 live 정리가 6회를 더한다. 정리까지 한 거래 하나가 30·E₀+8.7 MB를 읽고, WAL이 약 17 MB이면 516 MB를 읽는다. 재검증 동안 WAL File 잠금을 쥔다([F6 요약](f6-wal-revalidation-survey-20261102.md#f6a-summary)). 진도표는 개선(F6-b)을 18에 두고 "M1에서 병목이 아니면 개선 없이 18을 닫는다"고 적었다.

**비교.** Oracle과 PostgreSQL은 정상 운영 중 redo·WAL을 쓰기만 한다. live 상태는 latch로 보호한 메모리 구조를 믿고, 검증은 block check(Oracle `DB_BLOCK_CHECKING`)나 디버그 옵션으로 한다. WAL을 다시 읽는 것은 복구와 복제뿐이다.

**영향.** 한 epoch 안의 누적 비용이 거래 수의 제곱으로 는다. 동시 writer(11)는 WAL 잠금 때문에 재검증 시간만큼 서로 기다린다. checkpoint(13)를 해도 epoch의 E가 줄지 않으면 이 비용은 그대로다(지금은 reopen만 E를 4,096 B로 되돌린다).

**권고.** 측정으로 정할 문제가 아니다. 비용이 WAL 길이에 비례한다는 것은 코드로 이미 확인됐다. 재검증은 시험 oracle·검증 모드로 옮기고, 제품 경로는 메모리 장부와 page latch를 믿게 한다. 이 변경을 18이 아니라 07 전의 일반 경로 형식 단위에 넣는다.

<a id="global-root"></a>
### 5.3 전역 Root/Job page

**관찰.** 정리 큐는 page 1의 Root와 page 2–6의 Job cell 64개다. 모든 Row 연산(Begin, Reserve, Forward, CLR, Commit, Prune 등)이 Root와 Job page를 바꾼다. `native_mask`가 모든 연산에 Root·Job bit를 넣는다. 바뀐 행마다 RootJob cell을 하나 잡고 정리가 끝나야 돌려준다. Root는 64 cell 전체의 digest를 다시 계산한다.

**비교.** Oracle은 거래 정보를 여러 undo segment header의 transaction table에 흩어 두고, 정리는 거래 단위로 한다. DML이 매번 바꾸는 전역 page는 없다. PostgreSQL(CLOG·ProcArray)과 InnoDB(undo log header·history list)도 거래별 구조에 commit 순서 목록을 둔다.

**영향.** version-edit gate를 나중에 나누어도 모든 writer가 page 1에서 직렬화된다. 동시성 번들(11)의 "Status slot 하나를 넘는 동시 거래"는 이 page를 그대로 두고는 처리량을 얻기 어렵다.

**권고.** 정리·보존 장부를 거래별 구조(Status slot 또는 undo segment header)로 옮기고, 전역에는 commit 순서의 정리 목록만 둔다. 전역 page는 allocation metadata에 한정한다.

<a id="undo-shape"></a>
### 5.4 Undo의 배치와 정리 방식

**관찰.** 첫 profile은 Undo page 하나에 root 하나를 둔다([WAL 모양 표 아래 설명](../contracts/dev3-module-consumers.md#row-shape)). 일반 형식은 page당 6–37개 root를 허용한다. before-image는 canonical 행 전체다(최대 128 KiB). 정본은 undo record마다 영속 incoming-edge count를 두고, 링크가 바뀔 때마다 그 count를 같은 WAL 그룹에서 바꾸며, 오래된 suffix를 logged cut으로 끊는다([undo 수명](../contracts/disk-storage-design.md#undo-lifecycle)).

**비교.** Oracle의 undo block은 한 거래의 record 여러 개를 이어 담고, 한 번 쓴 undo record를 고치지 않는다. UPDATE의 undo는 바뀐 열의 before 값만 담는다. undo는 retention이 지나면 extent 단위로 다시 쓴다. InnoDB도 purge가 commit 순서로 거래 단위 undo를 지운다.

**영향.** count 갱신은 오래된 undo page를 다시 dirty하게 만들어 쓰기를 늘린다. 행 전체 before-image는 넓은 행의 작은 UPDATE를 비싸게 만든다. 14의 정리 모델이 record 단위 count·cut에 묶이면 3일 배정은 모자랄 가능성이 크다.

**권고.**

1. undo를 거래별 append-only segment로 묶어 page 하나에 여러 record를 담는다.
2. UPDATE의 undo와 redo는 바뀐 열의 delta로 한다.
3. 정리는 horizon 아래로 내려간 거래를 commit 순서로 통째로 회수하는 방식을 먼저 검토한다. reader는 자기 snapshot에서 보이는 버전에서 chain 따라가기를 멈추므로, commit 순서로 회수하면 이미 회수된 record를 따라갈 일이 없다. UndoRef에 slot generation이 있으니 잘못 따라간 경우는 검출된다. 이 방식이 record 단위 count를 대체할 수 있는지는 M01–M06 모델로 확인할 D2 후보다.

<a id="representation"></a>
### 5.5 행·index 표현 크기

**관찰.** [행 header](../contracts/dev3-row-format.md)는 168 B다(magic 4, VersionStamp 56, LogicalRowVersion 48, UndoRef 40 등). Int64 한 열 행은 cell 185 B이고 leaf 하나에 85행, 사용자 payload 비율은 2.59%다. index 항목 값은 고정 160 B다([표현 비용](../contracts/dev3-resource-bounds.md#representation-cost)). VersionStamp와 TxnId에는 DB·timeline namespace가 들어간다.

**비교.** Oracle 행 header는 flag·lock byte·열 수로 약 3 B이고, 그 뒤에 열 길이가 온다. 거래 정보는 block header의 ITL(slot당 약 24 B)에 두고 행은 1 B lock byte로 ITL slot을 가리킨다. PostgreSQL tuple header는 23 B와 line pointer 4 B다. Altibase 디스크 테이블도 page의 CTS에 거래 정보를 둔다(altidev4 조사).

**영향.** 같은 데이터에 page가 수십 배 필요하면 buffer 적중률, I/O, backup 크기, checkpoint 비용이 모두 그만큼 나빠진다. namespace가 행마다 들어가면 restore가 모든 page를 다시 써야 한다([7절](#remaining)의 15–16).

**권고.** UPDATE·overflow·index 형식을 더 만들기 전에 거래 정보를 page 단위 slot(ITL·CTS)으로 옮기고, 행에는 slot 번호와 짧은 local id만 남기는 형식을 검토한다. DB·timeline namespace는 file header·control·WAL에만 둔다. 개발 fixture를 매번 새로 만드는 [호환 정책](../contracts/disk-storage-design.md#scope)상 지금 형식을 바꾸는 비용이 가장 작다.

<a id="census"></a>
### 5.6 정확 byte census와 상한 재계약

**관찰.** 연산마다 Rust 값 크기를 합산해 고정 상한과 대조한다. Commit frame ceiling 3,024 B는 stage 여유 2 B로 잡혔다. Begin 상한을 넣으려고 C의 frame 배치를 바꿔 본문을 42,089 B에서 40,881 B로 줄였다. 표 발급은 Pending Commit 옆 여유가 44–52 B뿐이라 닫지 못하고 C 잔여로 남았다([F4 상한](../contracts/dev3-module-boundaries.md#f4-caller-budgets), [Begin·statement 상한](../contracts/dev3-module-boundaries.md#begin-statement-ceilings)). 진도표 05-4·05-5의 상당 부분이 이 재계약이었다.

**비교.** Oracle과 PostgreSQL도 메모리를 pool이나 `work_mem` 같은 한도로 묶는다. 하지만 기능 하나마다 byte 단위 상한을 다시 계약하지는 않는다. stack은 PostgreSQL의 `check_stack_depth()`처럼 실행 중에 검사한다.

**권고.** 자원 상한 원칙은 유지하되, 구성 요소별로 여유(예: 25%)를 둔 상한과 측정 감시로 바꾼다. 기능 하나가 수십 byte를 더할 때마다 계약을 다시 여는 구조는 남은 번들(index 유지, split 전파, overflow)마다 같은 지연을 만든다.

<a id="txn-size"></a>
### 5.7 거래 크기 상한

**관찰.** 거래당 mutation 256개, 논리 payload 1 MiB, private 8 MiB, 동시 거래 32개다([자원 profile](../contracts/disk-storage-design.md#budgets)). SQL의 UPDATE·DELETE는 대상 행을 먼저 제한된 목록으로 모으고, 이를 넘으면 statement를 실패시킨다([DEV4 실행](../contracts/dev4-sql-design.md#execution)). 큰 거래는 DEV7의 7.26으로 미뤘다.

**비교.** Oracle 거래는 undo tablespace 공간으로만 묶인다. 행 잠금 정보를 data block 안에 두므로 잠금 수가 메모리를 쓰지 않고, lock escalation도 없다.

**영향.** 257행을 바꾸는 UPDATE 한 문장이 실패한다. 상한을 풀려면 잠금과 write-set이 어디에 사는지가 바뀌는데, 이는 동시성의 핵심이다. DEV7에서 고치면 DEV3의 11을 다시 하게 된다.

**권고.** 11에서 행 잠금을 행의 TxnRef로 표현한다. 이미 VersionStamp에 TxnRef가 있으므로 Oracle의 in-block 잠금과 같은 구조가 된다. 잠금 대기는 그 거래의 종료를 기다리게 하고, 행마다 메모리 잠금 항목을 두지 않는다. statement 대상 목록은 임시 공간으로 넘길 수 있게 하고, 거래 크기는 undo·WAL 예약으로만 묶는다.

<a id="semantics"></a>
## 6. 거래 의미 — Oracle 동작과 다른 선택

| # | 항목 | XADB 현재 계약 | Oracle | PostgreSQL | 권고 |
| --- | --- | --- | --- | --- | --- |
| S1 | RC의 쓰기 충돌 | X lock을 얻은 뒤 관측 버전이 다르면 SerializationConflict로 **거래 전체 abort**([3.10 격리](../contracts/dev3-task-10-concurrency.md#isolation)) | 막은 거래가 끝나기를 기다린 뒤 바뀐 행에 계속 적용한다. 행을 찾은 열이 바뀌었으면 statement를 내부에서 되돌리고 다시 시작한다(restart). 사용자에게 오류가 없다 | 기다린 뒤 새 버전으로 WHERE를 다시 평가한다 | Oracle처럼 statement restart. XADB는 이미 statement를 mark까지 되돌릴 수 있으므로 새 statement snapshot으로 다시 실행하면 된다 |
| S2 | 기본 격리 | `begin()` 기본 Serializable | READ COMMITTED 기본 | READ COMMITTED 기본 | Oracle 목표라면 RC 기본 |
| S3 | Serializable | 읽고 쓴 table의 마지막 commit q ≤ start q를 commit 때 검사한다. 같은 table의 다른 행도 충돌한다. 같은 snapshot의 k개 경쟁에서 성공은 많아야 1개다([충돌 상한](../contracts/dev3-task-10-concurrency.md#serializable-conflict-bound)) | snapshot isolation. 바꾸려는 행을 거래 시작 뒤 commit한 다른 거래가 바꿨을 때만 ORA-08177 | REPEATABLE READ는 같은 행 충돌만 오류, SERIALIZABLE은 SSI | Oracle 의미(행 단위 충돌)를 SERIALIZABLE로 두고, 지금의 table 검증은 더 엄격한 별도 모드로 남길지 사용자 결정 |
| S4 | 같은 거래 안 cursor | 같은 거래가 뒤에 행을 바꾸면 기존 cursor는 CursorInvalidated([cursor 계약](../contracts/dev3-task-06-tree-rows.md#module-statement-cursor)) | cursor는 open 시점에 일관되고 자기 뒤 DML은 보이지 않는다. PL/SQL의 cursor loop 안 UPDATE가 흔한 사용 방식이다 | cursor는 open 시점 snapshot과 command id로 고정 | 거래의 mutation sequence(PostgreSQL의 command id에 해당)를 cursor·statement에 묶어 "open 뒤 자기 변경"을 보이지 않게 한다. Halloween 문제도 같은 장치로 막힌다 |
| S5 | deadlock | 가장 큰 TxnId를 victim으로 골라 **거래 전체 rollback** | deadlock을 감지한 쪽의 statement 하나만 rollback(ORA-00060). 거래는 남는다 | 거래 abort | Oracle처럼 statement rollback |
| S6 | 긴 reader와 undo 공간 | snapshot을 시간으로 만료하지 않고 새 쓰기를 거절([보존 압력](../contracts/disk-storage-design.md#retention-pressure)) | 기본은 undo를 덮어쓰고 긴 query가 snapshot too old로 실패한다. RETENTION GUARANTEE를 켜면 DML이 실패한다 | 공간이 늘어난다(bloat) | 정책으로 드러내고 Oracle 기본(reader 실패)을 둘지 사용자 결정 |
| S7 | index 생성 중 DML | DB 전체의 새 거래 admission을 닫고 기존 거래가 모두 끝나기를 기다린다([3.07 인덱스](../contracts/dev3-task-07-indexes.md) 5절) | 그 table의 DML만 막는다. ONLINE이면 막지 않는다 | 그 table의 쓰기만 막는다. CONCURRENTLY면 막지 않는다 | table 단위 DDL 잠금 |
| S8 | 거래 크기 | 256 mutation·1 MiB([5.7](#txn-size)) | undo 공간만큼 | 사실상 제한 없음 | [5.7](#txn-size) |

S1·S2·S3는 목표 결정과 같이 정해야 한다. 3.10 계약은 "기본 Serializable을 RC로 바꾸거나 read-set을 생략하여 성능 통과로 만들지 않는다"고 적었다. 하지만 이것은 성능 문제가 아니라 목표 의미의 문제다. Oracle을 목표로 한다면 RC 기본이 맞는 의미다.

<a id="remaining"></a>
## 7. 남은 작업별 검토(진도표 07–19)

| 진도표 | 현재 전제 | 위험 | 착수 전에 정할 것 |
| --- | --- | --- | --- |
| 07 UPDATE(3일) | DELETE 모양 재사용(바뀐 page 전체 Patch, 행 전체 before) | 넓은 행의 작은 UPDATE가 비싸다. RC 쓰기 충돌 의미(S1)가 아직 없다 | [5.1](#wal-volume)의 Patch 규칙, 열 delta undo, S1·S2 |
| 08 tree·overflow·일반 행 변경(7일) | byte 균형 split([tree 형식](../contracts/dev3-tree-format.md)의 `choose_split`), 영속 sibling link 없음, underfull merge 없음 | RowId가 단조 증가하므로 split 뒤 왼쪽 leaf는 다시 채워지지 않고 약 50%로 남는다. 범위 scan은 leaf마다 root부터 다시 내려간다. 지운 공간은 leaf가 완전히 빌 때만 돌아온다 | 오른쪽 끝 삽입의 비대칭 split(Oracle의 90-10 split, PostgreSQL의 rightmost split), leaf right-link, 14의 leaf 병합 |
| 09 index·PK/UNIQUE(6일) | 항목마다 versioned presence(값 160 B)와 undo | 행과 index가 각각 version chain을 가진다. 조회마다 tree 두 개와 chain 두 개를 지난다 | 둘 중 하나를 고른다. (a) 항목은 delete-mark만 두고, 가시성은 RowId로 찾은 행에서 판정한 뒤 key를 다시 확인한다(PostgreSQL·InnoDB 방식). (b) page 단위 CTS(Altibase 방식). 그리고 S7 |
| 10 Catalog·DDL(4일) | Oracle식 DDL 전 commit, versioned catalog | 방향은 맞다. ALTER는 DEV7이다 | 행에 열 수가 있으니 "열 추가는 metadata만"이 가능한 형식인지 지금 확인한다 |
| 11 동시성·다중 객체(7일) | 전역 version-edit gate, Commit의 전역 설치 장벽([commit 설치 순서](../contracts/disk-storage-design.md#commit-installation-order)), 메모리 행 잠금, 거래 32개 | page 1([5.3](#global-root)), WAL 잠금([5.2](#live-replay)), 설치 장벽이 모든 writer를 직렬화한다. S1–S5 의미가 미결이다 | 의미 결정과 구조 변경을 나눠 견적한다. 아래 주석 참고 |
| 12 일반 저장 수명(6일) | DWB batch마다 control 교체와 dirsync 두 번([batch flush](../contracts/disk-storage-design.md#doublewrite-batch)) | flush 한 번에 sync가 4–6회다 | PostgreSQL식 full-page write(WAL 안)나 control 없는 InnoDB식 DWB를 PF09에서 비교한다. group commit 설계도 함께 |
| 13 checkpoint·WAL 회수(4일) | fuzzy checkpoint | 설계는 맞다. 다만 [5.1](#wal-volume)·[5.2](#live-replay)가 남으면 checkpoint를 매우 자주 해야 한다 | 5.1·5.2를 먼저 |
| 14 aging·공간 반환(3일) | record 단위 count·logged cut, 행마다 즉시 Prune | 정리 WAL이 forward보다 크다([5.1](#wal-volume)). 모델이 복잡해 3일이 모자랄 가능성이 크다 | [5.4](#undo-shape)의 거래 단위 회수, Oracle의 delayed block cleanout처럼 다음 접근 때 stamp |
| 15–16 audit·backup·restore(6일) | exact-C drain, 모든 page의 namespace rewrite([typed rewrite](../contracts/dev3-task-15-backup-integrity.md#namespace-rewrite)) | restore가 DB 전체를 다시 쓴다 | Oracle처럼 block은 그대로 복사하고 새 incarnation은 WAL·control에만 둔다. [5.5](#representation)가 전제다 |
| 17–19 S5(10일) | PF09는 18에서 처음 실행 | [5.1](#wal-volume)–[5.4](#undo-shape)가 마지막에 드러난다 | 구조 변경 직후 작은 PF09 smoke(한 행 거래 1,000개의 거래당 WAL bytes와 거래/초)를 앞에 둔다 |

11 주석. 정본은 Commit의 Status 설치를 WAL sync 뒤로 두기 위해, stage부터 설치까지 page 효과를 가진 다른 그룹을 막는다. 그런데 reader의 가시성은 이미 published frontier(`commit_q ≤ snapshot_q`)로 정해지고, page flush는 WAL-before-page 규칙을 따르며, sync 실패는 fence한다. 그렇다면 Status의 Committed를 stage 시점에 메모리에 설치하고 가시성만 frontier로 늦추는 방식(Oracle·PostgreSQL에 가까움)으로 이 장벽을 없앨 수 있을 가능성이 있다. 이 리뷰는 반례를 끝까지 찾지 않았으므로 D2 후보로만 적는다. 이 장벽이 남으면 group commit도 어렵다.

<a id="recommendations"></a>
## 8. 권고와 결정 항목

**P0 — 07 착수 전(사용자 결정 포함)**

| ID | 권고 | 결정 |
| --- | --- | --- |
| R1 | Oracle을 목표로 [제품 방향](../plans/project-direction.md)에 기록하고, 따를 동작을 S1–S8에서 고른다 | 사용자 |
| R2 | 진도표에 "일반 경로 형식" 단위를 새로 둔다. 범위는 [5.1](#wal-volume)(Patch 규칙과 검증 내용 분리), [5.2](#live-replay)(live 재검증을 제품 경로에서 제거), [5.3](#global-root)(거래별 정리 장부), [5.4](#undo-shape)(undo 배치), [5.5](#representation)(ITL·CTS 형식)다. V2로 다시 견적한다 | 사용자 승인 뒤 D2 |
| R3 | 07-1 UPDATE 계약 전에 S1·S2를 정한다 | 사용자 |

**P1 — 해당 행 착수 전**

| ID | 권고 | 진도표 |
| --- | --- | --- |
| R4 | 오른쪽 끝 삽입의 비대칭 split 규칙과 leaf right-link | 08 |
| R5 | index 항목의 버전 방식 선택([7절](#remaining) 09의 a·b), table 단위 build 잠금 | 09 |
| R6 | 행 잠금을 TxnRef로, 거래 크기 상한 해제, deadlock은 statement rollback, Commit 설치 장벽 재검토 | 11 |
| R7 | 거래 단위 commit 순서 회수와 지연 stamp | 14 |
| R8 | restore는 block 복사와 새 incarnation | 16 |

**P2 — 측정으로 정할 것**

| ID | 권고 |
| --- | --- |
| R9 | DWB control 비용을 full-page write·InnoDB식 DWB와 비교(PF09) |
| R10 | 자원 census를 여유 있는 구성 요소별 상한과 감시로 바꾸는 범위 |
| R11 | 구조 변경 직후 작은 PF09 smoke를 18보다 앞에 둔다 |

R2는 이미 병합된 01–06의 결과를 무효로 하지 않는다. N=0 복구, Catalog CREATE, 첫 Row 거래, 복구 READY의 상태 전이·실패 의미·독립 시험은 형식이 바뀌어도 그대로 쓰인다. 바뀌는 것은 byte 모양과 장부의 위치다. 진도표 [§6](../plans/dev3-schedule-baseline.md#progress-rule)의 완료 취소(5번)가 아니라 범위 변경(6번, V2)으로 다룬다.

<a id="schedule-impact"></a>
## 9. 진도표 확인

- 보고 형식: `DEV3 V1 / 2026-11-03·main 7692631 / 적립 29.5/86일 = 34.30% / 이번 완료 없음 / 다음 06-6(PR #47)·07-1 / 잔여 ETA 45~68일(중앙 56.5일)`.
- 이 리뷰는 일정 항목을 적립하지 않고 표를 바꾸지 않는다. 권고는 채택 전이다.
- R2를 채택하면 새 일정 항목과 분모 변경이 생기므로 [§6](../plans/dev3-schedule-baseline.md#progress-rule)의 V2 절차로 고친다. 이 리뷰는 일수를 제안하지 않는다. 다만 R2 없이 가면 08·11·14의 배정(7·7·3일)이 지금 구조를 넓히는 전제라 모자랄 위험이 크고, R2를 하면 그 세 행의 위험이 줄어든다고 판단한다.

<a id="limits"></a>
## 10. 한계

- 정적 문서·코드 대조다. 제품 build·시험·측정을 하지 않았다.
- Oracle 내부 동작은 공개 문서와 Ask TOM 기준이다. Oracle 실행 비교는 하지 않았다. Oracle·PostgreSQL의 로그·header 크기는 크기 차수 비교이며 측정값이 아니다.
- Altibase는 이번에 다시 확인하지 않았다. TSS·CTS·지연 stamping 비교는 altidev4 조사(`0682bdd8`)를 따른 것이며, 사용자 로컬 checkout에서 다시 확인할 후속 항목이다.
- [5.1](#wal-volume)의 frame 항목 가운데 REDO에 꼭 필요한 것, [5.4](#undo-shape)의 거래 단위 회수, [7절](#remaining) 11 주석의 설치 장벽 제거는 반례 탐색을 끝내지 않은 후보다. 채택 전에 D2와 M01–M06 모델 검토가 필요하다.
- 한 작성자의 리뷰이며 별도 독립 검토를 거치지 않았다.

<a id="sources"></a>
## 11. 외부 출처

- [Oracle Database Concepts 11g R2 — Data Concurrency and Consistency](https://docs.oracle.com/cd/E15586_01/server.1111/e25789/consist.htm): READ COMMITTED 기본, 쓰기 충돌 시 대기 뒤 진행, SERIALIZABLE의 ORA-08177, block 안 행 잠금과 lock escalation 없음, deadlock의 statement rollback.
- [Ask TOM — Seeing a restart](https://asktom.oracle.com/Misc/part-ii-seeing-restart.html): 행을 찾은 열이 consistent read와 current read에서 다르면 UPDATE가 다시 시작되고, 사용자에게 보이지 않는다.
- [Oracle Database Administrator's Guide 21c — Managing Undo](https://docs.oracle.com/en/database/oracle/oracle-database/21/admin/managing-undo.html): snapshot too old와 RETENTION GUARANTEE(기본 꺼짐, 켜면 DML 실패 가능).
- [PostgreSQL 문서 — Transaction Isolation](https://www.postgresql.org/docs/current/transaction-iso.html): READ COMMITTED의 WHERE 재평가, REPEATABLE READ의 동시 갱신 오류, SERIALIZABLE의 SSI.

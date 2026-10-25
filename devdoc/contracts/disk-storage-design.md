# DEV3 — 디스크 저장·인덱스 상세 설계

2026-10-25 · **DEV3-DESIGN-003·REVIEW-004–011 ACCEPTED / 전체 DEV3 인수 미완료**

[구현 계획](../plans/dev3-implementation-plan.md) · [이번 조사](../evidence/dev3-wal-undo-research.md) · [이전 조사](../evidence/dev3-source-research.md) · [커널 계약](implementation-contracts.md) · [DEV2 계약](dev2-kernel-design.md) · [복구 계약](recovery-logging-design.md) · [인수 항목](disk-test-matrix.json)

[아키텍처 비교 보완](../evidence/dev3-dev4-architecture-review.md)은 아래 잠금·undo 수명·배치 flush·자원 예약의 근거다. 기본 설계의 채택과 실행 가능한 알고리즘의 검증을 구분한다.

[첫 구현 기록](../implementation/dev3-implementation.md)은 공통 [page envelope bytes](dev3-page-format.md)와 [snapshot/pin 부분 모델](dev3-snapshot-model.md)의 범위·소유·검증을 기록한다. 전체 M01–M06·실제 disk path·DS 인수는 미완료다.

후속 [DEV8 운영 설계](dev8-operations-design.md)는 이 저장 계약 위의 physical standby·capture/retention·온라인 backup/PITR를 구체화한다. D8-F01의 추가 port·WAL supplement·durable consumer pin과 REBASE/재장애 예약 증명은 해당 기능 구현 전 의무이며, 현재 S1의 offline backup·기본 recovery 완료 조건을 대체하지 않는다.

<a id="scope"></a>
## 1. 범위와 대체 관계

사용자가 선행 리뷰 보완 뒤 COW pack/GC의 재설계를 요청했다. [DEV3-REVIEW-002](#review-followup)를 먼저 검토·반영했고, 이어 **mutable disk page + 단일 WAL + 영속 undo + bounded dirty buffer**를 선택한다. 제품 저장 엔진은 [제품 방향](../plans/project-direction.md)에 따라 DISK 하나로 개발한다. 기존 DEV0/1·DEV2 P0/K0 구현과 실행 증거를 바꾸지 않는다. DEV2 영속 구현·잔여 인수는 [현재 DEV2 기록](../implementation/dev2-implementation.md)을 따르며, DEV3의 제한 allocator/Status/공유 버퍼·[N=0 plan4 복구](../evidence/dev3-recovery-resumed-20261025.md)는 source별 선택 검증되었으나 일반 row/index/catalog·Undo/CLR/commit·checkpoint/backup 경로와 전체 인수는 미완료다.

DEV3-DESIGN-003은 DEV3-DESIGN-001의 COW tree·sealed pack·root closure GC·전체 재작성 compaction·page-before-commit·root-only disk recovery를 대체한다. DEV3-PARALLEL-001의 pack/COW 작업 분할도 새 계획으로 대체한다. DEV2의 private/COW substrate·AXF1 한 거래 frame·exact checkpoint는 **DEV2 전용 기준선**으로 남으며 DEV3 page 경로에 적용하지 않는다. 상위 계약의 불변 CommittedRoot는 DEV3에서는 논리 snapshot/commit frontier를 뜻한다. mutable page 내용의 불변성이나 root graph 보존을 요구하지 않는다.

유지할 의미는 RowUID, statement 원자성, atomic multi-table row/index/catalog commit, durable ACK/UNKNOWN fencing, 독립 oracle, 현재 build의 crash/reopen, exact-cut backup/restore, bounded RAM이다. 호환 정책상 새 development fixture를 사용하고 기존 형식을 자동 변환/삭제하지 않는다. 다른 format은 변경 전에 거절한다. Oracle과의 전체 SQL/내부 구현 일치를 선언하지 않는다.

<a id="choice"></a>
## 2. 선택과 비용

| 대안 | 판단 |
| --- | --- |
| in-place + WAL + 영속 undo | 채택. commit이 모든 data page sync를 기다리지 않고, 기존 datafile 공간을 재사용. 대신 dirty-page flush, undo/CLR, torn-page repair를 필수 구현 |
| 이전 COW + immutable pack | snapshot/원자 root 교체가 단순하지만 path copy·여러 sync·root 보존·whole-pack GC/전체 compaction 비용. 이번 선택으로 대체 |
| WAL-first + private/COW 유지 | 가능하지만 redo page 재생성과 COW 회수를 모두 부담. 초기 단일 경로로 선택하지 않음 |
| LSM 또는 별도 resident 엔진 | 이번 범위에 추가하지 않음 |

ARIES의 WAL-before-page, pageLSN, analysis/redo/undo, redo-only 보상, fuzzy checkpoint를 참고한다. transaction undo의 정본은 **WAL 역방향 chain 대신 영속 UndoRef chain**이다. 오래 열린 거래가 시작 WAL 전체를 요구하지 않도록 한 선택이며 원형 ARIES와 동일 알고리즘이라고 부르지 않는다. undo store 자체의 변경과 rollback 진행점은 WAL로 redo 보호한다. 실제 성능 우월성은 미측정이다.

<a id="ownership"></a>
## 3. 책임·API·공개

기존 engine 안에서 storage는 page/heap-row-tree/index/allocator/buffer, transaction은 잠금·가시성·statement/undo·commit, durable은 WAL·flush 장벽·checkpoint·restart, catalog는 versioned 정의/객체 수명을 소유한다. platform 효과는 foundation port로 전달한다. 실제 구현 전 빈 소스 경로·crate를 추가하지 않는다.

| 계획 API/개념 | 계약 |
| --- | --- |
| `begin(access, isolation)` | 기존 인자 생략 API는 `Serializable` 유지. `ReadCommitted`는 명시 선택. 두 모드를 S1에서 직접 API로 검증 |
| `begin_statement` / `finish_statement` | ReadCommitted는 진입 때 published q를 pin; Serializable은 transaction 시작 q 사용. 한 SQL statement의 scan과 mutation은 같은 token 공유 |
| `execute_statement` | 입력 순서로 row 변경, statement 종료에 최종 unique 집합 검사. 성공 뒤 이전 statement undo를 유지 |
| `read/scan/index_range` | snapshot q와 own transaction의 변경을 함께 해석. visible 값을 caller-owned buffer에 복사 |
| `PageGuard` / content latch | frame pin과 page 내용 동기화 분리. pin은 page bytes 불변 보장이 아님 |
| `SnapshotGuard` | [snapshot 획득 규약](#snapshot-acquisition)으로 q 선택과 undo/객체 수명 horizon 등록을 aging에 대해 원자 처리. DB 전체 page frame 또는 이전 root closure를 pin하지 않음 |
| `checkpoint` / `backup_exact` | fuzzy recovery metadata checkpoint / admission drain을 거친 exact C 복사. 서로 다른 작업 |
| `reclaim` | eligible undo·삭제 row/index·retired object·free extent를 제한된 batch로 aging. 전체 root mark/sweep 없음 |

CommittedView는 DB/timeline·published q·catalog 접근 identity의 작은 descriptor다. 각 변경은 TxnRef에 종속되고 commit 상태의 원자 공개로 row/index/catalog 전체의 가시성이 전환된다. 하나의 writer coordinator가 q를 부여하고 durable commit→transaction committed 상태→published frontier를 순서대로 설치한다. frontier 공개 lock 안 allocation/I/O/callback을 금지한다. commit metadata와 설치 자원은 미리 예약한다.

<a id="page"></a>
## 4. 페이지·파일·identity

page는 16 KiB, header 예약 128 bytes, SHA-256 32 bytes, `(offset:u16,length:u16)` slot directory와 cell 영역을 가진다. header는 DB/file/page identity, page incarnation, owner ObjectId/Generation, kind/level, pageLSN, bounds/count, reserved=0을 포함한다. 공통 kind는 row/index/catalog internal/leaf, overflow, undo, transaction status, allocation metadata다. raw Rust struct dump를 금지한다.

PageRef는 `(FileId, page_no, incarnation)`이며 ObjectId/Generation은 기대 owner와 header에서 대조한다. 물리 slot은 RowUID가 아니다. 파일은 영속 예약 writer epoch+counter로 고유 생성하며 재사용하지 않는다. 같은 파일 안 page/extent/slot은 generation을 전진시켜 재사용한다. stale buffer/locator/undo 참조와 wrap을 거절한다. 재사용 WAL의 init image와 allocation generation 변경은 한 완결 mini-transaction에 담는다.

파일은 engine-owned data/undo 공간이며 고정 크기 extent(초기 64 pages)를 늘려 사용한다. allocation bitmap/free-space metadata는 disk에 두고 bounded cache로 읽는다. 새 파일/길이 확보·header·namespace 영속화가 끝나기 전 그 공간을 참조하는 로그의 durable commit을 허용하지 않는다. 이 장벽은 파일 최초 생성/확장에만 필요하며 매 commit의 새 pack 생성이 아니다. 공개되지 않은 생성 파일은 별도 owned staging 목록으로 관리한다. 선택된 checksummed FILEMAP control에는 DB/FileId·파일명·고정 bootstrap page 주소·예약된 길이가 들어간다. FILEMAP은 data/allocator page에 의존하지 않는 작은 chunked manifest이며 atomic selector+dirsync로 설치한다. 새 파일은 FILEMAP 영속 선택 뒤만 WAL에서 참조한다. drop한 파일 mapping은 필요한 WAL·DWB·checkpoint가 사라질 때까지 남긴다. FILEMAP 손상은 임의 directory 탐색으로 복구하지 않고 거절한다.

page load마다 길이·checksum·identity·owner·kind·slot overlap·정렬·참조 level과 산술 상한을 검사한다. checksum은 torn write를 검출할 뿐 복원 수단이 아니다. [flush 규약](#buffer)의 doublewrite가 복원을 맡는다. F01에서 offsets·record IDs·digest 범위·독립 hex vector를 동결한다.

공통 envelope의 offsets·kind ID·compact slot 배치는 [F01 page 형식](dev3-page-format.md)으로 동결했다. 이 codec은 opaque cell의 key 정렬·참조 child level을 판단하지 않는다. 해당 검증은 tree/undo 등 소유 decoder와 실제 page load의 미완료 의무로 유지한다.

<a id="tree"></a>
## 5. 행·B+tree·cursor

row tree는 `TableUID, RowId → versioned row`의 영속 locator를 겸한다. RowId는 논리적으로 재사용하지 않는다. leaf는 row identity·VersionStamp·undo ref·삭제 표시·canonical row를 담는다. 2 KiB 초과 row는 최대 128 KiB의 overflow chain을 사용하고 가변 값 상한 64 KiB를 유지한다. overflow 교체 시 이전 chain은 undo horizon까지 개별 allocation으로 보존하고 이후 allocator에 반환한다. 전체 tree COW를 만들지 않는다.

각 tree의 **안정 anchor page**가 현재 structural root를 가리킨다. catalog/old snapshot은 anchor identity를 보존하며 split 전 root pointer를 snapshot 정본으로 쓰지 않는다. internal separator·byte 기반 split·높이 16·key 한도는 유지한다. 초기 구조 변경은 tree 단위 짧은 exclusive latch 아래 serialize하고 일반 탐색은 shared tree latch와 page content latch로 보호한다. latch 보유 중 I/O·quota 대기·transaction lock 대기를 금지하고, 필요한 page pin/예약 후 latch를 재획득하여 generation/version을 재검사한다. 재시도는 bounded이며 부족하면 명시 오류다.

split·새 root 설치·allocator 구조 변경은 완료된 redo-only mini-transaction으로 묶는다. 사용자의 abort가 이미 완결된 split을 되돌리지 않는다. 사용자 undo는 RowUID/논리 key로 현재 tree에서 대상 항목을 찾고 원래 mutation의 효과만 취소한다. 이전 전체 page image를 복사하여 다른 거래 변경까지 되돌리는 undo를 금지한다. 로그의 page-oriented redo와 논리 undo가 다른 대상 page를 사용할 수 있음을 명시한다. 최소 구현은 underfull merge를 생략하고 빈 leaf 제거/공간 반환을 안전한 구조 작업으로 수행한다.

index의 물리 key는 `(encoded composite, RowId)`이며 presence/end 상태도 versioned다. 같은 full key를 제거 후 다시 추가하면 한 항목의 presence undo chain으로 표현한다. 이전 snapshot에 필요한 deleted key를 조기 제거하지 않는다. cursor는 마지막 full key보다 큰 항목을 **현재 구조에서 재탐색**한 뒤 고정 q로 가시성을 검사한다. 호출 간 tree stack/page pointer를 정본으로 유지하지 않는다. split·key 이동·aging과 함께 중복/누락을 검사한다. sibling link는 latch 안 탐색 보조로만 사용할 수 있다.

locator cache는 DB/timeline·table generation·RowId에 대한 물리 hint다. hit마다 page incarnation/slot RowId를 검증하고, 이동된 hint는 row tree 재조회로 처리한다. snapshot 적합성은 항상 version/undo로 판정하며 cache hit로 생략하지 않는다. cursor는 소유 transaction/statement/private revision에 묶이고 같은 transaction의 후속 mutation 뒤 기존 cursor는 CursorInvalidated다.

<a id="keys"></a>
## 6. UNIQUE와 statement 원자성

IndexSpec은 PK/Unique/Nonunique, 1–4개 서로 다른 열, table당 최대 8개, PK 최대 1개, ascending binary 비교다. Bool/Int64/Utf8/Binary/NULL, escaping 후 composite 1,024 bytes, RowId tie-breaker를 유지한다. PK는 이미 nonnullable인 열만 허용한다. expression/partial/descending/collation은 후속이다.

Unique는 **전체 NULL만 중복 예외**, 부분 NULL은 NULL 위치를 포함한 동일 tuple이면 충돌한다. `(1,NULL)` 두 행은 거절, `(NULL,NULL)` 두 행은 허용한다. SQL의 삼값 논리를 바꾸지 않는다. final uniqueness는 mutation마다가 아니라 **statement 종료**에 검사한다. 다른 거래의 최신 committed entries에 이전 성공 statement와 이번 statement를 포함한 **거래 전체의 own 최종 presence**를 반영한 집합을 검사하여 key 교환을 허용한다. 이 own 상태에서 실제 대체·삭제된 old full-key entries만 제외하며, 이번 statement가 건드리지 않은 own entries도 검사한다. 같은 거래 S1의 `A:key7` 삽입 성공 뒤 S2의 `B:key7` 삽입은 S2만 rollback하고 A를 보존해야 한다. 반대로 committed `A:key7`을 S1에서 삭제하거나 다른 key로 이동한 뒤 S2에서 `B:key7`을 삽입하는 것은 이전 own absent를 반영하여 허용한다.

같은 unique key에 대한 경쟁은 `(index generation, encoded key)` transaction lock으로 serialize한다. 영향받는 old/new unique keys를 정렬해 예약/잠금한 뒤 검사하고 commit/abort 완료까지 보유한다. 검사 대상은 다른 거래의 최신 committed 상태에 거래 전체의 own 최종 상태를 반영한 결과이며, 이번 statement의 delta만 보거나 오래된 statement snapshot만으로 duplicate를 허용하지 않는다. all-null 예외 key는 unique conflict lock 대상에서 제외한다.

statement 시작 시 undo-head savepoint·오류 결과·rollback 예약을 확보한다. 타입/키 크기/unique/quota/취소 실패는 해당 mark까지 undo하고 이전 성공 statement를 보존한다. rollback은 새 undo 생성 없이 기존 record와 사전 예약한 CLR/WAL/작업 공간을 사용한다. 디스크/로그 오류 때문에 rollback을 마치지 못하면 성공 ABORT를 응답하지 않고 RecoveryRequired/Fenced로 전이한다. statement 실패와 transaction 전체 abort를 구별한다.

<a id="rollback-accounting"></a>
**statement rollback은 논리 효과의 복원이며 자원 ledger의 과거 사본 복원이 아니다.** 실제 해제한 statement staging·미소비 예약만 반환한다. 이미 기록한 WAL/CLR, 아직 회수하지 않은 undo/page·완결 SMO와 transaction 종료까지 유지하는 row/key lock은 해당 소유 규약에 따라 계속 charge한다. 같은 실패를 반복해도 각 시점의 `charged + outstanding_reserved <= capacity`를 만족해야 하며, 다음 statement의 최악 비용을 확보하지 못하면 첫 effect 전에 거절한다. 남은 전체 거래의 abort/terminal 예약은 유지한다. 소비한 CLR bytes를 예약으로 되살리거나 SQL이 quota counter를 mark 시점으로 되감지 않는다. 논리 데이터·affected_rows의 원자성과 물리 사용량·잠금 수의 원상복구를 혼동하지 않는다. DS03/DS06으로 인수한다.

회수량을 `G_n`, 실패 문장 i의 forward WAL·CLR 비용을 `f_i,c_i`라 하면 `W_n=W_0+Σ(f_i+c_i)-G_n`이다. 논리 값이 같아도 WAL floor가 pin되어 `G_n=0`이면 사용량은 증가한다. 예를 들어 capacity20·기존 charge5·전체 abort 예약3·실패당 비용3이면 4회 뒤 `17+3=20`으로 다음 admission을 거절해야 한다. 매번 charge를5로 되감으면 5회째 실제 `20+3>20`을 허용한다. 이는 단위 없는 회계 반례이며 실제 opcode별 bytes 상한은 F05에서 동결한다.

<a id="concurrency"></a>
## 7. 가시성·동시성

ReadCommitted는 statement마다 published q를 pin하고 own writes를 읽는다. 다른 거래의 uncommitted/나중 commit은 undo를 따라 제거한다. 행 쓰기는 stable RowUID X lock을 transaction 종료까지 보유한다. 관측한 행의 UPDATE/DELETE는 **잠금 대기 여부와 무관하게 X lock을 얻은 뒤, 해당 행의 첫 변경 전에** 아래 논리 버전을 검사한다. 즉시 획득·이미 소유한 잠금에도 같은 검사를 적용한다. 불일치나 삭제이면 자동 predicate 재평가 대신 SerializationConflict로 **전체 거래 abort**한다. 상위 SQL이 수정 predicate를 숨겨 재실행하지 않는다. 같은 q의 서로 다른 행 writer는 key/schema 충돌이 없으면 모두 commit할 수 있다.

<a id="observed-row-version"></a>
관측값은 engine이 발급한 opaque row observation으로 전달한다. DB/timeline/open identity, table generation/RowId, statement token 및 관측한 **논리 행 버전 identity**에 결합하며 caller가 값/버전을 새로 꾸며 만들 수 없다. native 단건 변경도 같은 statement view에서 기대 버전을 얻어 검사하며 lock 획득 시점의 최신 버전을 기대값으로 덮어쓰지 않는다. 직접 scan한 결과를 넘기는 경로와 같은 의미다. 새 RowId를 생성하는 INSERT에는 기존 행 observation 대신 RowId 신규성·최종 UNIQUE 규약을 적용한다.

논리 버전 identity는 각 사용자 행 변경을 구별하고 undo가 이전 identity를 복원한다. 같은 bytes로 UPDATE해도 새 버전이며 값 비교만으로 동등하다고 하지 않는다. 이전 성공 statement의 own-write 버전은 유효한 관측값이다. page 이동/split/compaction·commit-q stamping만으로 논리 identity를 바꾸지 않고, TxnRef 하나만으로 같은 거래의 여러 행 변경을 구별하려 하지 않는다. 구체 인코딩과 generation/overflow 처리는 F01/F02에서 동결한다. 잠금 아래 현재 terminal/own 버전과 관측 버전을 비교하며, 검사를 통과한 잠금은 변경·transaction 종료까지 유지한다. 기존 page version·undo head 재검사는 물리 적용 준비의 유효성을 검사할 뿐 이 논리 비교를 대체하지 않는다. 이 규약은 Serializable에도 적용하며 commit 시 table validation도 유지한다.

Serializable은 transaction snapshot을 유지하고 read/write table 집합·absence/range 조회의 table generation을 등록한다. coordinator에서 각 table의 마지막 committed 변경 q가 start_q 이하여야 한다. own writes는 별도이므로 validation에서 제외한다. 성공 commit은 변경 table의 q를 전진시킨다. catalog 조회는 해당 catalog namespace/generation의 변경을 같은 방식으로 검증한다. table 단위 false conflict는 초기 보수적 선택이다. 독립 table writer는 모두 성공할 수 있고 같은 table의 다른 행은 충돌할 수 있음을 문서/시험에 명시한다. DB 전체 q 동일 검사는 제거한다. 더 세밀한 predicate validation은 후속 측정 대상이다.

transaction lock과 page latch는 분리한다. 기다리는 lock을 가진 채 page/tree latch·WAL append gate·publication lock을 보유하지 않는다. 최대 32 transaction wait-for graph를 검사하고 cycle의 가장 큰 TxnId를 victim으로 골라 전체 undo 후 lock을 해제한다. timeout/cancel도 undo 완료 뒤 결과를 확정한다. pinned snapshot을 시간만으로 강제 만료하지 않는다.

DDL은 독립 원자 거래다. offline index build는 admission close/drain 후 bounded external sort·검증·신규 object를 build하고 catalog 등록을 commit한다. build 중 crash의 미공개 object는 allocation owner 기록으로 정리한다. drop은 versioned catalog에서 retire 상태를 공개하고 old snapshot의 객체/anchor를 horizon까지 유지한다. table drop은 현재 종속 index 전체(최대8개)와 table 정의를 같은 DDL commit으로 retire하며, index만 drop할 때에는 table을 유지한다. 잠금/guard를 얻은 뒤 종속 집합과 generation을 재검사하여 동시 index 생성·삭제를 놓치지 않는다. drop 전 snapshot에는 기존 table/index 집합을 함께 보존하고 drop 이후에는 보이는 index의 table이 사라진 상태를 허용하지 않는다. 같은 이름의 재생성은 다른 generation이다. online index build는 후속이다.

offline index build의 capacity 예약은 workspace/turn 독점과 다르다. 기존 거래/cursor를 drain할 때 공유12MiB·turn·pin·적용 lock을 보유하지 않는다. 미효과 build 작업 owner는 기존 거래의 drain 집합에 넣지 않고, build DDL 거래는 drain 완료 뒤 시작한다. 기존 사용자 거래를 숨은 commit 또는 재분류로 제외하지 않는다. sort/tree·catalog commit·실패 rollback/cleanup은 같은 drain owner의 하위 권한으로 진행하여 닫힌 외부 admission을 다시 기다리지 않는다. timeout/cancel은 자기 drain 소유만 반환한다.

<a id="mutation-protocol"></a>
### 7.1 다중 페이지 변경과 잠금 순서

초기 구현은 row/index/catalog·undo/status·allocator의 **관계가 바뀌는 짧은 적용 구간을 하나의 version-edit gate로 직렬화**한다. split·aging·CLR도 같은 gate를 사용한다. 여러 writer의 입력 준비·page read·WAL sync는 겹칠 수 있다. 이 선택은 회수와 rollback의 서로 다른 page latch 순서를 우선 단순화하며, 높은 동시 처리 성능의 증거는 아니다. gate 분할은 실제 대기 측정과 새 수명 증명 뒤 별도 D2다.

| 순서 | 실행 규칙 |
| --- | --- |
| 1. 준비 | transaction row/key lock 대기, 전체 자원 예약, 예상 page 집합의 load/pin, 완결 WAL/CLR buffer 준비를 gate·tree/page latch 밖에서 수행. 일부 page를 pin한 채 추가 frame을 기다리지 않고 전체 pin 집합을 예약하거나 모두 놓고 재시도 |
| 2. 재검사 | version-edit gate 획득 뒤 tree gate는 ObjectId/Generation 순서, page content latch는 `(FileId,page_no,incarnation)` 순서로 획득. shared→exclusive 승격은 해제 후 재시도. page latch는 try 방식으로 얻고 실패하면 이번 적용 구간의 gate/latch를 모두 해제 |
| 3. 적용 집합 확정 | identity·page version·undo head·horizon·예약 토큰을 다시 확인. 추가 page/space가 필요하면 변경 없이 gate/latch를 놓고 1로 복귀. 기존 latch를 보유한 채 tree 경로를 추가 탐색하거나 I/O하지 않음 |
| 4. 로그와 메모리 적용 | 예약된 WAL append 구간에 완결 mini-transaction을 복사한 뒤 모든 page/link/count/progress와 pageLSN을 적용. append 뒤 부분 적용 실패는 Fenced. ATT/head/DPT 등록까지 마친 뒤 gate 해제 |
| 5. 후속 효과 | WAL sync·doublewrite/home I/O·transaction wait·사용자 응답은 위 gate/latch 밖. foreground retry 한도 소진은 첫 변경 전 오류 또는 이미 변경한 statement의 예약된 rollback으로 처리. rollback/필수 유지보수의 재시도는 아래 progress turn으로 전환 |

commit/checkpoint가 여러 경계를 함께 잡을 때 순서는 `commit coordinator → version-edit gate → tree gates → page latches → WAL append gate → 짧은 ATT/DPT/참조 registry leaf lock`이다. 사용하지 않는 중간 항은 생략한다. registry leaf lock에서는 상위 lock 요청·I/O·allocation을 하지 않으며 buffer map/예약 잠금은 위 적용 구간과 함께 잡지 않는다. flusher는 content latch에서 이미지를 복사한 뒤 해제하고 I/O하므로 역순으로 gate를 요청하지 않는다. DWB control은 별도 단일 작업 소유권으로 조정하며 그 상태 mutex를 I/O 중 보유하지 않는다.

commit의 status 설치는 WAL sync 뒤 위 순서로 수행하고, page/tree/append/registry latch를 놓은 뒤 coordinator 아래 publication lock에서 frontier를 전진시킨다. capture는 coordinator와 version-edit gate 아래 **등록까지 끝난** prefix만 선택한다. status가 먼저 Committed가 되어도 `commit_q > snapshot_q`인 reader는 새 값을 읽지 않는다. 실제 Rust guard·rank 표·try 실패 cleanup은 D3-F02에서 동결한다.

<a id="commit-installation-order"></a>
**WAL의 물리 sync 완료와 page 효과의 설치 완료를 분리한다.** Commit@c는 sync 뒤 status·terminal queue/credit·descriptor·dirty/DPT를 설치하므로, stage에서 얻은 전역 의미 설치 소유를 그 등록 전체가 끝날 때까지 유지한다. 이 동안 물리 slot이 Idle로 돌아와도 후속 update/CLR/SMO/aging 등 page 효과를 가진 그룹의 stage/apply를 허용하지 않는다. 같은 status page의 다른 slot을 Commit보다 먼저 @c+1로 변경하면 `pageLSN>=c`여도 Commit 효과가 없는 image가 flush되어 REDO가 이를 건너뛸 수 있다. 초기 구현은 단일 WAL의 전역 장벽을 선택하며 page별 미설치 목록이나 sync 전 Committed 설치를 도입하지 않는다. 이미 설치된 이전 image의 읽기/flush는 허용하고, 모든 page의 LSN은 그 page에 대한 선행 효과가 빠짐없이 설치되었음을 뜻해야 한다.

이 설치 소유는 I/O 동안 보유하는 version/page/append/coordinator mutex나 FIFO turn이 아니다. 기다리는 producer는 적용 lock·turn·pin·공유 workspace를 반환한다. Commit continuation은 자기 장벽을 통과하여 선예약한 자원으로 설치할 수 있어야 하며, 먼저 선택된 다른 maintenance도 장벽을 기다리며 turn을 붙잡지 않고 양도·재진입한다. stage 이후 취소/실패는 pending Commit을 나중에 쓸 수 있는 상태로 남긴 채 장벽을 정상 해제하지 않는다. 첫 byte 미시도와 pending 제거가 증명된 취소의 정확 경계는 F02에서 동결하고, 불명/부분 설치·poison은 fence한다. 설치 완료 뒤 장벽을 놓아도 frontier/ACK는 기존 coordinator 순서를 따른다. 물리 WAL API의 Idle/prefix 관측은 이 의미 권한을 발급하지 않는다.

**예약된 rollback/필수 유지보수는 try-latch의 공정성에 의존하지 않는다.** F02에서 먼저 고정한 유한 재시도 횟수 K 뒤 FIFO progress turn을 요청한다. 모든 적용 진입과 reader/flusher의 content-latch 구간은 turn admission에 참여하며, admission 요청/대기는 coordinator·version/tree/page/append/registry 잠금 밖에서 한다. turn 대기자는 실제 maintenance workspace와 page pin을 모두 반환하고 자기 자원 예약 token·논리 작업 소유만 유지한다. 공유 12 MiB workspace는 **선택된 turn owner만 획득**하며 이를 사용하는 유지보수 단계는 빠른 try 경로부터 먼저 turn에 참여한다. 일반 경로도 workspace를 가진 채 turn/admission을 기다릴 수 없다. 별도 4 MiB flusher workspace는 이 획득 순서에 종속되지 않는다. 진행 중 latch 구간은 유한하며 I/O·사용자 callback·다른 admission 대기를 포함하지 않는다. 한 turn은 다음 두 단계로 실행한다.

1. head ticket은 신규 foreground/다른 maintenance의 적용 진입을 닫고 기존 적용 구간을 drain한다. reader와 별도 예약 flusher는 계속 허용한다. head 작업은 구조가 더 바뀌지 않는 상태에서 재탐색하고 전체 page 집합을 load/pin한다. 이때도 부분 pin을 보유한 추가 frame 대기와 latch 안 I/O는 금지하며, 전체 frame 예약과 flusher의 전용 진행 자원을 사용한다. turn 소유권은 admission 상태이며 I/O 중 보유하는 mutex가 아니다. transaction lock/사용자 응답이나 active 거래 종료를 이 turn 안에서 기다리지 않는다.
2. 필요한 집합·입력 준비 뒤 신규 reader/flusher의 content 구간도 잠시 닫고 기존 구간을 drain한다. 기존 잠금 순서로 **한 bounded WAL/CLR 적용 또는 flush image copy**를 완료하고 모든 latch와 turn을 해제한다. 후속 sync/I/O는 content admission을 다시 연 뒤 수행한다. 여러 CLR이 필요하면 단계마다 ticket을 양도한다. 준비 단계의 flusher까지 먼저 막거나 상위 잠금을 가진 채 turn을 요청하는 구현은 금지한다.

이 규약의 진행 조건은 종료하는 I/O와 공정한 actor 실행이며 I/O 장애는 Fenced로 구별한다. 무한히 유입되는 reader가 매 try 순간을 점유하는 공정한 starvation schedule도 turn의 drain 뒤에는 배제해야 한다. F02는 K·admission guard/rank·전체 pin 예약과 drain의 종료 조건을 동결하고, D3-M03/M06은 단순 action-depth 검사와 별도로 이 진행성을 판정한다.

<a id="snapshot-acquisition"></a>
### 7.2 최초 snapshot 획득과 horizon 등록

`begin`의 transaction snapshot, ReadCommitted의 새 statement snapshot, idle prepare의 짧은 catalog snapshot은 같은 획득 규약을 사용한다. **published q만 읽은 뒤 gate 밖에서 나중에 horizon을 등록하는 경로는 금지한다.** 작은 CommittedView를 복사한 사실만으로 undo·객체 수명이 보호되지는 않는다.

1. snapshot/registry slot과 guard 메모리를 먼저 예약하고, 적용 경로의 turn admission에 참여한다. admission·gate 대기는 coordinator·publication·tree/page/append/registry 잠금과 page pin을 보유하지 않은 상태에서 한다. 새 snapshot 등록은 turn 준비 중 허용하는 기존 reader의 content 접근과 구별하며, 신규 적용 진입이 닫혔으면 기다린다.
2. version-edit gate를 획득한 뒤 publication lock에서 DB/timeline/open identity와 published `q`를 복사하고 publication lock을 해제한다. gate는 계속 보유한 채 registry leaf lock에서 예약한 slot에 그 `q`의 horizon 소유를 등록한다. 등록이 완료된 뒤 registry lock과 gate를 해제하고 guard를 반환한다. 등록 전에 catalog/row 접근을 시작하지 않는다. 이 구간에는 allocation·I/O·callback·다른 admission 대기가 없다.
3. 획득의 선형화 시점은 gate 안에서 published `q`를 읽은 순간이다. commit frontier가 그 뒤 전진해도 aging은 같은 gate를 얻기 전에는 cut/reuse를 적용할 수 없고, gate를 얻으면 이미 등록된 `q`를 관측한다. 예약/진입 실패는 guard나 사용 가능한 미등록 `q`를 반환하지 않으며 예약을 해제한다.

snapshot 경로의 잠금 순서는 `version-edit gate → publication lock 해제 → registry leaf lock`이다. publication과 registry lock은 동시에 보유하지 않으며, 이 경로는 coordinator·tree/page/append lock을 요청하지 않는다. commit도 publication lock을 잡은 채 version-edit gate를 요청하지 않는다. DEV2의 `retention ledger → publication` 순서를 DEV3 registry에 그대로 복사하지 않는다.

aging은 version-edit gate를 보유하고 **tree/page/append/registry lock을 얻기 전에** publication lock에서 `Q = published q`를 읽고 해제한다. 이어 registry에서 등록된 snapshot들의 최소값과 rollback/객체/backup 소유를 확인한다. snapshot 보존 기준은 `H = min(Q, 등록된 snapshot q들)`이며 reader가 없으면 `H = Q`다. `H`와 더 오래된 소유가 요구하는 모든 버전을 재구성할 수 있는 link/값을 유지한다. 단순히 undo record의 q가 H 이하라는 이유로 그 record를 지우지 않는다. 후보를 gate 밖에서 준비했으면 실제 cut/reuse를 적용하는 gate 구간마다 이 검사를 다시 한다. 별도 incoming count·rollback·transient pin 조건도 계속 적용한다.

이 `Q` 기준은 `TxnStatus=Committed(q=11)` 설치 후 frontier가 아직10인 구간에도 적용한다. reader가 없어도 새 snapshot은10을 선택할 수 있으므로10의 복원에 필요한 undo/객체를 남겨야 한다. frontier가 전진하면 이전 Q로 판정한 보존은 보수적일 뿐이다. 아직 공개하지 않은 commit q나 활성 reader가 없다는 이유의 무한대 값을 H로 사용하지 않는다. engine이 Fenced이면 새 snapshot admission과 회수를 진행하지 않는다.

guard의 마지막 사용·소유가 끝난 뒤 등록을 해제한다. 해제는 registry leaf lock만으로 가능하며 보존 요구를 줄일 뿐 오래된 q를 새로 등록하지 않는다. Serializable statement/cursor는 기존 transaction guard의 소유를 끊지 않고 공유한다. 별도 slot으로 인계할 때에는 원래 소유를 유지한 채 위 gate/registry 순서로 새 소유를 등록한 뒤 이전 소유를 해제한다. ReadCommitted의 다음 statement는 해제한 q를 재등록하지 않고 최신 published q를 새로 획득한다. 이 규약은 [개별 참조의 transient pin](#undo-lifecycle)을 대체하지 않는다.

필수 반례는 `reader q=10 선택 → writer q=11 공개 → aging → reader horizon 등록`과 `status11 설치 → frontier10인 채 aging → snapshot10 획득`이다. 전자는 등록까지 gate를 유지하여 aging을 차단하고, 후자는 Q 기준으로10을 보존해야 한다. [D3-M02와 실제 thread 인수](../plans/dev3-implementation-plan.md#state-model)에서 acquisition/등록/공개/cut의 별도 action과 도달 barrier로 판정한다. 현재 모델·제품 실행은 NOT_RUN이다.

<a id="undo"></a>
## 8. 영속 undo와 aging

UndoRef는 `(undo file/page incarnation, slot generation)`의 안정 참조다. record는 TxnId·previous transaction UndoRef·대상 RowUID/논리 index key/catalog identity·before version·previous version ref·undo kind를 담는다. 128 KiB 값은 bounded chunks로 나누고 chain/count/digest를 검사한다. transaction status에는 TxnId/generation·Active/Aborting/Committed/Aborted·commit q·undo head·rollback target/progress가 있다.

VersionStamp는 `Committed(q)` 또는 `TxnRef(id,generation)`다. 다른 거래의 committed 값을 before-image로 캡처할 때는 commit q를 해석해 저장한다. 미완료 거래의 상태 slot을 무조건 복제하지 않는다. own 연속 변경은 자신의 chain을 유지한다. terminal 거래는 durable aging queue와 undo chain에 변경 대상을 남긴다. horizon을 통과하면 현재 header에 남은 자기 TxnRef를 logged commit-q stamp로 바꾸고, 이미 다른 거래가 대체한 경우 그 before-image의 resolved stamp를 확인한다. 관련 참조 제거가 완료된 뒤만 status/undo slot을 generation 증가와 함께 재사용한다.

버전 chain의 obsolete suffix는 owner aging 작업이 **logged cut**으로 끊는다. before-image 안 previous-version ref와 Active 거래의 rollback chain도 영속 참조다. undo record마다 disk에 저장하는 incoming-reference count를 관리하며 row/index header·undo record·transaction head의 link 변경과 count 증감을 같은 WAL 그룹으로 처리한다. 0-count이고 horizon/rollback pin이 없는 record만 반환한다. chain은 단조로운 record sequence로 과거만 참조하고 cycle/overflow는 거절한다. cut 전 참조가 남은 suffix는 더 오래 보존할 수 있다. rollback은 오래된 before-image에 남은 link를 무조건 되살리지 않고 현재 cut과 참조 이전을 같은 CLR에 반영한다. record 반환은 그 outgoing reference를 batch로 해제하는 logged queue 작업이며 full-root tracing을 만들지 않는다. `A commit→B update→A aging→B abort→slot 재사용`의 chain·count·full-check를 필수로 검증한다.

undo horizon은 [현재 published q와 등록된 snapshot](#snapshot-acquisition), rollback chain, retired object/overflow, backup/recovery 작업 중 가장 오래 필요한 소유를 포함한다. reader horizon만 지났다고 아직 Active인 거래의 undo를 반환하지 않는다. ReadCommitted의 statement 사이에는 reader pin을 놓을 수 있으나 자기 rollback chain은 거래 종료까지 유지한다. disk undo/page/tss 회수는 owner별 큐·segment cursor의 작은 batch로 수행하며 전체 root graph나 immutable pack을 추적하지 않는다.

WAL 보존과 undo 보존을 분리한다. checkpoint의 transaction head/progress와 필요한 undo/status/allocator page가 redo 가능해야만 오래된 WAL을 놓는다. dirty undo page의 recLSN은 data page와 동일하게 WAL을 pin한다. 조건 충족 후에는 오래된 Active 거래의 시작 WAL이 없어도 UndoRef chain으로 rollback/restart undo가 가능해야 한다. WAL의 prevLSN은 진단용 chain이며 삭제한 로그를 undo의 필수 입력으로 사용하지 않는다. RP/archive/PITR 보존은 별도 owner다.

<a id="undo-lifecycle"></a>
### 8.1 참조 이전·rollback·회수 상태

영속 incoming count는 row/index의 version link, undo의 previous-version/previous-transaction link, transaction head 등 **실제 영속 edge의 중복도를 포함한 개수**다. 같은 대상에 두 필드가 연결되면 2개로 센다. snapshot horizon, active rollback 소유, 회수 queue의 작업 소유, 일시적인 reader pin은 별도 회수 금지 조건이며 incoming count 하나로 합치지 않는다. TxnRef/status 재사용도 현재 header와 undo before-image의 참조 제거 및 일시 pin 해제를 요구한다.

reader가 source header에서 UndoRef/TxnRef를 복사한 뒤 source latch를 놓으면 그 사이 대상이 재사용될 수 있다. 필요한 대상에는 **source content latch를 놓기 전에 bounded transient reference pin을 등록**한다. 등록은 I/O 없는 leaf registry에서 수행하며, 대상 page를 load하는 동안 generation 재사용을 막는다. 다음 edge로 이동할 때 새 pin을 먼저 등록하고 이전 pin을 해제한다. snapshot 하나 때문에 전체 chain을 pin하지 않으며 이력 길이와 무관한 소수의 이동용 pin만 보유한다. pin 공간은 read 진입 때 예약한다. 회수자는 source edge 제거 및 마지막 transient pin 검사를 같은 변경 규약으로 조정한다. crash 뒤 transient pin은 사라지지만 영속 edge·rollback 소유·queue 진행점은 redo로 복원한다.

| 전이 | 사전조건·한 완결 WAL 그룹의 효과 | crash 뒤 처리 |
| --- | --- | --- |
| Prepared → Linked | before-image와 필요한 모든 edge/slot 공간 예약. undo 생성, 기존/새 edge count 증감, transaction head와 row/index 효과를 함께 기록·적용 | 완결 그룹만 redo. 미완결 그룹은 연결된 record가 아님 |
| Linked → CommittedRetained | durable commit q 확정. terminal status·aging queue 소유를 기록; snapshot/rollback 필요 record 보존 | commit을 되돌리지 않고 상태/queue 재구성 |
| Linked → Undoing → mark/terminal | statement mark 또는 전체 abort target까지 논리 undo. 현재 tree에서 대상 검색 후 data/index 효과·head/edge/count·UndoNextRef를 같은 CLR에 기록 | CLR은 redo-only. 다음 UndoRef부터 계속; statement mark 도달과 전체 abort-end를 구별 |
| Retained → Cut | 모든 live snapshot이 필요한 버전을 재구성할 수 있고 Active rollback이 필요한 link도 유지됨을 재검사. owner edge 절단·count 감소·회수 queue 인계를 한 그룹에 기록 | cut과 queue가 함께 복구. 오래된 before-image가 절단된 edge를 무조건 부활시키지 않음 |
| zero incoming → Releasing | horizon/rollback/transient pin이 없고 queue가 작업을 소유. outgoing edge를 bounded batch로 해제하면서 durable 처리 cursor를 함께 기록 | cursor와 count 변경을 함께 redo하여 중복 감소 금지 |
| Releasing → Free → Reused | outgoing 해제 완료, pin 없음, 관련 DWB pending retirement 완료. allocator free/generation과 새 INIT은 기존 재사용 규약 적용 | old generation 참조 거절; 새 incarnation은 완전 INIT로 복원 |

rollback은 before-image의 값만이 아니라 **현재 보존된 link와 참조 소유**를 확인한다. aging이 이전 link를 절단했으면 필요한 버전/rollback chain이 여전히 존재하는 경로로만 복원한다. 참조를 새로 만들면서 대상 count만 누락하거나, count를 먼저 줄이고 source edge를 남기는 중간 상태는 허용하지 않는다. 이미 Releasing인 record에는 새 edge를 연결하지 않는다. 대상이 아직 필요한데 유효 참조를 찾지 못하면 정상 NotFound/ABORTED 대신 invariant failure/Fenced다.

부분 rollback을 끝내 Active로 복귀할 때에도 **이미 보상한 suffix를 이후 full abort에서 다시 undo하지 않는 상태**를 영속 head/link·소유 인계로 보존한다. rollback 대상 chain과 보존/회수용 anchor를 구별하고, head만 복사해 full rollback을 시작하는 경로가 보상된 record를 다시 실행하지 못하도록 한다. 새 statement·checkpoint·BEGIN WAL 회수·재시작 뒤에도 같은 의무가 유지되어야 한다. 필요한 head/link/queue 표현은 F01/F02에서 동결하며 휘발성 완료 표시에 의존하지 않는다.

위 표는 알고리즘의 의무이며 모델 실행 결과가 아니다. [D3-M01–M06](../plans/dev3-implementation-plan.md#state-model)의 cut/rollback/reader-pin/재사용·재장애 탐색과 독립 graph 검산을 해당 production leaf 전에 수행한다. 참조 계수 정합성만 맞아도 필요한 과거 버전을 잃을 수 있으므로 logical snapshot/rollback 결과를 따로 검산한다.

<a id="buffer"></a>
## 9. buffer·WAL-before-page·torn-page

buffer는 Empty/Loading/Clean/Dirty/Flushing 상태와 frame generation·pin·content latch·modification counter를 가진다. 같은 page의 중복 load/예약을 조정한다. clock은 pinned/I/O frame을 건너뛰고 bounded sweep/deadline 뒤 BufferBusy를 반환한다. dirty victim은 flush 완료 전 재사용하지 않는다. file read/flush/checksum은 map/state mutex 밖에서 수행한다.

**steal/no-force**를 선택한다. uncommitted page도 undo와 redo가 보호되면 flush 가능하고, commit 때 data page 전체 flush는 요구하지 않는다. 각 page 변경은 완결 WAL mini-transaction을 append한 후 메모리에 적용하고 pageLSN을 해당 기록 끝 LSN으로 올린다. undo 생성·transaction head 갱신·data effect는 완결 redo 그룹과 상태 전이로 연결하며 중간 undo ref가 보이는 순서를 금지한다. WAL 공간/undo/rollback 예약은 latch 전에 확보하고 append gate 안에서는 I/O 대기를 하지 않는다. append 뒤 적용 불변식 실패는 fence다.

flush는 다음 순서를 지킨다.

1. 복사 전에 물리 주소 `(DB,FileId,page_no)`의 flush claim을 얻고 allocator free/reuse와 배타적으로 조정한다. latch 안에서 identity/incarnation/pageLSN/modification counter와 완전 page image를 사전 예약한 I/O buffer로 복사한다. claim은 incarnation이 바뀌어도 같은 주소의 후속 flush/재할당을 막으며 Pending 선택 이전부터 유지한다.
2. latch 밖에서 **그 pageLSN까지 WAL sync**를 확인한다. 실패하면 home page를 쓰지 않고 fence한다.
3. 고정 크기 doublewrite 슬롯에 identity·pageLSN·slot sequence·checksum과 image를 기록하고 sync한 뒤, 해당 pending을 선택한 DWB control의 atomic replace+dirsync까지 완료한다.
4. 원래 data/undo/status/allocator page 위치에 image를 write하고 해당 file sync를 완료한다.
5. 그 뒤 DWB control에서 해당 슬롯을 retired로 제거한 선택을 sync하고, 그 뒤만 doublewrite 슬롯을 재사용한다. 현재 frame의 counter/identity가 같을 때만 clean 전환/DPT 제거; 재갱신됐으면 dirty와 보수적 recLSN을 유지한다.

doublewrite는 16 MiB의 영속 고정 pool이며 디스크 quota에 포함한다. bounded DWB control manifest는 **현재 pending 슬롯**의 sequence·DB/FileId/page/incarnation·pageLSN·image hash만 선택한다. image sync 뒤 control을 atomic replace+dirsync로 영속 선택한 후 home write를 시도한다. home file sync 뒤 해당 pending을 제거한 control 선택을 영속화해야 재사용할 수 있다. 선택이 불명이면 fence하고 그 슬롯을 덮어쓰지 않는다. 동일 주소의 flush/재사용은 **복사 전 claim 취득부터 전체 pending retirement 영속화와 frame/DPT finalize 완료까지** serialize한다. allocator는 마지막 pin·영속 edge·pending뿐 아니라 Prepared/inflight claim도 적용 시 재검사한다. 준비 취소는 home/Pending 효과가 없고 지연 I/O·completion이 더 이상 실행되지 않음을 확인한 뒤만 claim을 반환하며, 불명 효과는 fence와 복구 소유로 인계한다. crash 뒤 RAM claim이 사라져도 선택된 pending 복원을 먼저 마친다. control 갱신도 단일 coordinator로 serialize하며 lock 안 I/O를 하지 않는다. 이전 control로의 임의 fallback이나 raw 슬롯 scan으로 후보를 부활시키지 않는다. 손상 selector/control은 거절한다. 이 보수적 control I/O 비용을 PF09에 포함하며 최적화는 별도 검토한다.

<a id="doublewrite-batch"></a>
### 9.1 bounded batch flush

위 5단계 장벽은 batch에 함께 적용할 수 있다. 초기 인수 profile은 **batch당 1–64 page**, deadline에서 덜 찬 batch도 flush하며 full batch를 무한히 기다리지 않는다. 1-page 경로를 fault 기준선으로 유지하고 동일 계약의 8/64-page 경로를 비교한다. 처리량 향상은 미측정이다. 64개의 page image와 descriptor·codec workspace·진행 중 복사를 모두 고정 flush memory 4 MiB에 포함하며, 실제 slot header/control을 뺀 16 MiB DWB pool 용량을 초과하지 않도록 F01/F05에서 계산한다.

1. 같은 물리 주소는 한 batch에 한 번만 넣고 이미 Prepared/inflight claim 또는 pending이 있는 주소는 제외한다. 각 주소의 claim을 확보한 뒤 latch 아래 각각 image를 복사하고 page latch 없이 batch를 봉인한다. batch가 여러 시점의 image를 포함해도 되며 transaction snapshot으로 취급하지 않는다.
2. batch 최대 pageLSN까지 WAL sync → 모든 DWB image write/sync → 전체 batch pending 목록의 control 선택+dirsync 순서다. 일부 image만 완성되었으면 pending으로 선택하거나 home write하지 않는다.
3. home write는 page별이며 **모든 대상 file의 sync가 확인된 뒤만 batch 전체 retirement**를 선택한다. 일부 file만 성공했으면 모든 슬롯을 pending으로 보존하고 fence한다. 성공한 일부 슬롯을 먼저 재사용하지 않는다.
4. retirement의 영속 선택 뒤 슬롯을 반환하고, page별 modification counter/identity가 같은 frame만 clean 처리한다. 나중 수정된 frame은 dirty/DPT에 남는다. batch control 갱신 실패·불명도 기존 pending 합집합을 보존한다.
5. restart의 bootstrap repair도 선택된 batch 전체의 home 복원·file sync·retirement를 완료한 뒤 해당 주소의 후속 redo write/새 flush/재사용을 허용한다. 이 중 재장애가 나면 남은 pending batch를 다시 적용한다. raw 슬롯이나 이전 control을 검색하여 retired batch를 부활시키지 않는다.

이 단락은 페이지별 안전 장벽을 유지한 배치화만 허용한다. commit group sync, 여러 flush coordinator, 일부 성공 슬롯의 조기 retirement는 채택하지 않는다. exact batch deadline·부분 I/O/파일별 sync·control 교차 cut는 F04/F05에서 실행 전에 고정한다.

FILEMAP과 선택된 DWB control은 home allocator에 의존하지 않고 읽을 수 있다. 첫 bootstrap 단계에서 두 control의 identity/digest를 검증한 뒤 **pending image만** FILEMAP의 주소에 복원한다. pending이 있는 주소의 재할당은 retirement 전 금지되므로 slot의 incarnation을 bootstrap 근거로 사용할 수 있다. allocator/status/anchor도 동일 방식으로 먼저 복원하고, WAL allocation/init redo를 적용하면서 일반 page owner/incarnation을 검증한다. healthy home이 더 높은 LSN/incarnation이면 불변식 위반으로 거절하며 오래된 image로 덮지 않는다. 유효 pending 없는 기존 initialized home의 torn/손상은 원칙적으로 REJECT_CORRUPTION이다. 예외는 FILEMAP·allocation generation과 일치하는 **완전한 INIT image 및 그 이후 채택 끝까지의 WAL 전체**가 보존된 경우다. 새 page가 아직 zero/uninitialized home인 no-force 상태도 이 경로에서 INIT부터 재구성한다. delta redo만으로 checksum 오류를 숨기거나 WAL이 끊긴 INIT을 복원 근거로 쓰지 않는다. INIT 복원 대상은 redo에서 초기화하므로 bootstrap 단계의 blanket page-load 거절을 적용하지 않는다. @10 사본이 retired된 뒤 @20을 쓰고 슬롯을 재사용한 경우 @10 raw bytes가 남아 있어도 복구 후보가 아니다. arbitrary cold-page 손상까지 수리한다고 주장하지 않는다. control torn·retirement 불명·allocator torn·page reuse·두 번째 crash가 F04 필수 vector다.

<a id="flush-progress"></a>
### 9.2 교체 flush와 checkpoint 진행

frame 확보를 위한 교체 flush와 오래된 recLSN을 해소하는 checkpoint flush는 서로 다른 진행 목적이다. dirty page 수만 줄었다고 recovery WAL floor가 전진했다고 판단하지 않는다. F05에서 한 scheduling round의 page 탐색·선택 상한과 두 종류의 작업에 대한 유한 service 간격을 정한다. 지속적인 hot-page 변경/교체 요청 아래에도 오래된 eligible dirty page의 flush 기회를 보장한다. pin·진행 중 DWB 등으로 대상이 불가능하면 이유를 기록하고 다른 bounded 작업 또는 깨움/deadline 대기로 전환한다. 동일한 불가능 후보나 clean 항목만 재선택하는 무진행 busy loop를 금지한다. 별도 thread 수나 다중 coordinator는 여기서 정하지 않는다.

page가 재수정되어 dirty에 남는 경우 image sync 자체와 recLSN/floor 해소를 구별한다. **모든 새 쓰기를 수용하면서 checkpoint가 따라잡는다는 보장은 하지 않는다.** 보존 WAL/dirty backlog가 F05의 사전 고정 한도에 가까워지면 새 foreground 예약을 제한하여 기존 flush·rollback 몫과 진행 기회를 유지한다. 필요한 WAL을 지우거나 WAL-before-page 장벽을 약화하지 않는다. 기존 progress turn·공정한 실행·정상 I/O 전제를 적용하며, latch 보유 I/O나 무제한 dirty-page 목록을 추가하지 않는다. PF09에는 oldest recLSN lag, flush 선택/완료/재dirty 수, 작업별 최대 service gap, 무진행 round와 admission 거절 원인을 포함한다.

초기 recLSN 보수 유지 규약에서는 scheduling만으로 hot page의 floor 전진을 보장하지 않는다. pressure 시 새 forward effect의 admission을 닫고 이미 admitted된 유한 작업의 완료/rollback과 예약 flusher는 계속 허용하여, 재수정되지 않는 image의 sync·DPT 제거·checkpoint 인계가 가능한 구간을 만든다. 이 drain은 client의 다음 COMMIT 요청을 기다리는 offline DDL drain과 다르며 active transaction의 uncommitted page도 기존 steal 규약으로 flush할 수 있다. live retention owner가 여전히 WAL을 요구하면 floor는 그대로이고 새 write 거절을 유지한다. 안전한 post-copy redirty LSN 인계로 drain을 줄이는 최적화는 후속 D2이며 여기서 암묵적으로 도입하지 않는다.

<a id="commit"></a>
## 10. WAL·commit·rollback

L0는 단일 ordered WAL stream이며 여러 transaction의 update 기록이 섞일 수 있다. WAL 레코드는 begin/update(undo+page redo)/system mini-transaction/CLR/commit/abort-end/checkpoint를 구분한다. frame마다 DB/timeline·writer epoch·LSN·TxnId·length·checksum·종류를 검증한다. 하나의 transaction을 한 AXF1 frame으로 넣는 DEV2 규약을 DEV3에는 적용하지 않는다. mini-transaction은 최대 1 MiB 완결 frame이며 128 KiB row 분할과 overflow/undo chain을 다룬다. transaction의 논리 mutation 256개·canonical after-image 합계 1 MiB 상한은 유지하고 실제 WAL/undo/CLR bytes는 별도 예약한다.

commit은 statement-final 검사/잠금·schema validation→필수 namespace 확인→q/commit record·응답/상태 설치 자원 예약→**첫 commit record byte 시도에 CommitAttempted**→commit LSN까지 WAL sync→TxnStatus Committed(q)와 frontier 공개→ACK 순서다. 데이터/undo page가 아직 disk에 없어도 redo가 복원할 수 있어야 한다. q를 WAL LSN 또는 wall clock과 혼용하지 않는다. LSN은 한 DB의 epoch 사이에도 단조 증가하는 논리 로그 위치이며 FileId/offset과 별개다. wrap과 이미 영속 page가 가진 LSN 재사용을 금지한다. group commit은 후속 최적화이며 초기 단일 coordinator로도 여러 nonconflicting 거래의 update 준비는 겹칠 수 있다.

| 실패 경계 | 결과 |
| --- | --- |
| update WAL이 존재하지만 commit record 미시도 | undo 완료 후 ABORTED. rollback I/O 실패면 NotCommitted/RecoveryRequired + Fenced; 공유 변경이 사라졌다고 답하지 않음 |
| commit append/sync 결과 불명 | UNKNOWN + Fenced. row/index/catalog 전체를 recovery에서 판정; 재전송 금지 |
| commit sync 후 publish/reply 실패 | commit 유지. 외부 응답 미관측은 UNKNOWN |
| rollback/CLR 중 crash | redo가 CLR 효과와 UndoNextRef 진행점을 함께 복원하고 남은 chain만 undo |

CLR은 redo-only이며 되돌린 논리 효과의 **현재 물리 page들**에 대한 redo와 다음 UndoRef/target 상태 갱신을 한 완결 그룹으로 기록한다. CLR 자체를 undo하지 않는다. undo progress를 먼저 저장하고 data 복원을 생략하는 경로를 금지한다. transaction abort는 모든 undo 후 durable abort-end·상태 확인 뒤 잠금을 해제한다. statement rollback은 savepoint까지만 수행하고 거래를 Active로 돌린다. CLR 완료에 필요한 로그/작업 공간은 처음 변경을 admission할 때 예약한다.

<a id="recovery"></a>
## 11. 정상 recovery와 전체 검사의 분리

`LEASE → PREFLIGHT/SELECT → BOOT_RESERVE → DOUBLEWRITE_REPAIR → ANALYSIS → RESERVE_RESTORE → REDO → RECOVERY_REBASE → RECOVERY_LOG_OPEN → UNDO → VERIFY_RECOVERY → STABILIZE → RESUME → READY`다. 이전 성공 sync의 bytes/namespace 보존, active tail의 제한적 incomplete frame 제외, 완전 길이 checksum 오류 거절, UNKNOWN 합법 이력은 DEV2와 동일하게 검사한다. 소스 WAL bytes를 수정하지 않는다. 아래 REBASE가 필요 closure를 영속 인계한 뒤의 파일 삭제는 회수 규약을 따른다. recovery 중 page는 WAL/CLR 규약으로 변경할 수 있으므로 DEV2의 '모든 source page 불변'을 적용하지 않는다.

BOOT_RESERVE는 page repair보다 먼저 [영속 예약 인계](#recovery-reservation)의 고정 bootstrap 몫을 확보한다. ANALYSIS는 checkpoint와 WAL의 예약 상태도 복원하며, RESERVE_RESTORE에서 loser·미완결 system 작업의 잔여 예약과 같은 prefix의 논리 allocator 소유를 대조하고 **REDO의 추가 효과 전에** 복구 작업에 배정한다. 아직 redo하지 않은 home allocator의 값을 최신 소유로 사용하지 않는다. runtime ledger를 0으로 초기화하고 UNDO 진입 때 새 quota 요청을 하는 경로는 금지한다. admission은 모든 장벽 동안 닫혀 있다.

ANALYSIS는 선택 checkpoint의 capture LSN 상태에서 active transaction table을 seed하고 **capture LSN 직후**부터 완결 WAL 끝까지 거래 상태·최신 UndoRef·commit q를 재구성한다. checkpoint 이전 상태를 다시 덮어쓰거나 이미 끝난 거래를 ATT에 부활시키지 않는다. terminal commit을 undo하지 않으며 미완료 거래를 loser로 정한다. runtime DPT는 buffer/flush-inflight page의 recLSN을 보유하고 checkpoint에 기록한다. 재시작에는 커진 전체 DPT를 RAM에 만들지 않는다. `redo_start=min(checkpoint capture LSN, checkpoint DPT의 최소 recLSN)`부터 모든 관련 page redo를 streaming 읽고 incarnation/pageLSN으로 이미 반영한 기록을 건너뛴다. 이는 ARIES의 선택적 redo보다 보수적인 bounded-memory 시작안이다. ATT 최대치는 admission된 32개와 system 작업 예약으로 제한하며 commit/abort 완료 시 제거한다.

REDO는 commit 여부와 관계없이 이력을 반복한다. 이 단계는 source WAL만 사용하며 page flush가 필요하면 먼저 채택 prefix의 해당 LSN/namespace를 sync하고 doublewrite를 따른다. allocation/init·undo·status·CLR·tree 구조 기록도 포함한다. 뒤의 RECOVERY_REBASE는 [고정 bootstrap 두 슬롯](#recovery-rebase)에 그 끝의 home/undo/ATT/예약 closure를 인계하며, **새 WAL을 쓰기 전에** 이전 재시작의 epoch 설치 비용을 회수한다. RECOVERY_LOG_OPEN은 그 인증된 base anchor의 완결 LSN 다음에 새 recovery epoch/header를 연결하여 sync하고 **WRITER 선택까지 영속화**한다. admission은 계속 닫는다. 이 선택이 끝나기 전 새 CLR append 및 그 효과의 home write를 금지한다. 다음 restart는 선택된 base anchor와 그 뒤 epoch chain에서 recovery epoch를 발견한다. 기존 recovery epoch의 incomplete tail도 같은 규약으로 판정한다. UNDO는 loser의 durable UndoRef head부터 역순으로 처리하고 선택된 recovery epoch에 새 CLR을 남긴다. 이미 완료한 undo는 CLR의 UndoNextRef로 건너뛴다. recovery 중 두 번째 장애에도 선택된 base closure+그 뒤 WAL/CLR에서 다시 실행할 수 있어야 한다.

WAL 내부의 envelope·native/redo·descriptor 결합 검산, capture 이후 ANALYSIS의 owner/예약 전이 채택, 대상별 page REDO를 구별한다. REDO는 runtime stage의 현재 before descriptor/page 일치 검사를 그대로 호출하지 않는다. capture 이전에 종료된 owner가 ATT에 없어도 필요한 dirty page를 redo하며, 이미 반영한 대상은 identity/incarnation과 pageLSN에 따라 skip하고 뒤처진 대상만 적용한다. 여러 page 중 하나가 최신이라는 이유로 전체 그룹을 skip하거나, 그 page가 WAL의 before image와 다르다는 이유만으로 정상 복구를 거절하지 않는다. owner/ledger를 page마다 다시 채택하지 않으며 필요한 선행 identity·INIT 연속성의 결함은 계속 거절한다.

crash 당시 `AbortingStatement`인 loser도 최종적으로 전체 거래를 abort한다. 부분 rollback mark에 도달한 사실은 사용자 거래의 성공이나 복구 완료가 아니다. 이미 보상한 suffix를 제외하고 이전 성공 statement의 남은 undo까지 처리한 뒤 durable AbortEnd로 종료한다. 중간 전환의 정확 상태/표현은 F01/F02에서 동결하며 mark 도달만으로 Active 거래나 READY를 공개하지 않는다.

<a id="recovery-rebase"></a>
RECOVERY_REBASE는 정상 fuzzy checkpoint와 구별되는 **admission이 닫힌 복구 전용 장벽**이다. 새 WAL begin/end record에 의존하지 않는 전용 checkpoint 두 슬롯과 selector 교체 공간을 DB 생성 때부터 fixed maintenance에 선할당한다. source WAL을 채택 끝 L까지 sync하고, REDO 관련 dirty home data/undo/status/allocator를 DWB로 전부 flush+sync한 뒤 pending batch retirement를 영속화한다. 변경 없는 cold table을 전수 순회하지 않는다. ATT/UndoRef/진행점/잔여 obligation·예약 ownership root·채택 commit/publication 상태·DPT empty·L 및 predecessor tip identity를 묶은 self-contained descriptor를 비선택 슬롯에 쓰고 sync한다. namespace도 완결되어야 한다. 이 descriptor는 L까지의 과거 WAL bytes를 다시 요구하지 않는 **replay base anchor**이며 timeline을 바꾸거나 LSN을 되감지 않는다.

selector 인계는 `current=새 base, previous=기존 closure`를 atomic replace+dirsync한 뒤, `current=previous=새 base`를 다시 영속 선택하여 기존 closure의 bootstrap 슬롯 참조를 먼저 해제한다. 그 뒤 비선택인 다른 bootstrap 슬롯에도 같은 L의 완전 descriptor를 써서 sync하고 `current=두 번째 base, previous=첫 base`를 다시 영속 선택한다. selected current/previous가 참조하는 슬롯을 덮어쓰지 않는다. 둘이 이미 같은 L을 인증하면 재기록을 생략할 수 있다. 이전 restart의 두 bootstrap 슬롯이 모두 선택 중이면 먼저 source WAL/namespace를 포함하는 최신 current closure를 current와 previous 양쪽의 공통 근거로 영속 선택하여 다른 슬롯을 비운 뒤 위 순서를 시작한다. 그 단일 fallback 선택이 durable이 되기 전에는 슬롯을 쓰지 않으며, 모든 중간 단계에서 최소 한 개의 완전 recovery closure를 유지한다. REBASE selector는 base identity와 `base-only` writer continuation을 함께 원자 선택한다. 이전 CURRENT/WRITER를 따로 따라가 삭제한 epoch를 다시 요구하지 않으며, 다음 WRITER는 이 base identity를 명시적으로 참조해야 한다. 새 WRITER 선택 전 crash는 base-only에서 다시 시작한다. source cut/슬롯 identity와 허용 selector 상태는 F01/F04에서 고정한다.

두 새 base가 영속 선택된 뒤에만 이전 checkpoint/epoch predecessor를 요구하는 recovery floor를 해제하고, 다른 live owner도 요구하지 않는 WAL/control 파일을 회수+dirsync한다. 삭제 실패 bytes는 여전히 charged다. selector 불명은 old/new 합집합 closure를 보존한다. 별도의 bootstrap 임시 파일을 재시작 횟수만큼 만들지 않으며, selector의 교체 staging도 고정 소유 슬롯으로 처리한다. **회수가 끝나 새 epoch와 잔여 UNDO의 예약이 동시에 확보되기 전에는 RECOVERY_LOG_OPEN에 진입하지 않는다.** 중간 crash는 선택된 old 또는 new closure로 REDO/REBASE부터 다시 시작하므로 새 epoch bytes를 더 쌓지 않는다. epoch 선택 직후 crash해도 다음 REBASE가 그 epoch를 포함한 L로 같은 두 슬롯을 재사용한다. 이때 기존 loser의 완료는 기다리지 않는다. 현재 offline backup profile에서 살아남는 소유만 적용하며, 향후 durable archive/RP owner가 base 이전 WAL을 요구하면 그 pin과 bootstrap 한도의 양립을 별도로 증명해야 한다.

VERIFY_RECOVERY는 WAL/selector/allocator·거래 terminal 상태·실제로 redo/undo한 page의 구조와 효과를 검증한다. **변경 없는 전체 table/index 양방향 검사를 정상 open의 필수 작업으로 하지 않는다.** 모든 page read에서 checksum/identity/bounds를 검사하며 읽지 않은 임의 cold page 손상은 첫 접근 또는 전체 검사에서 발견할 수 있다는 범위를 공개한다. 필요한 WAL/복구 page 손상은 정상 open을 거절한다. checksum을 맞춘 논리 오염까지 online read 하나로 검출한다고 하지 않는다.

`check_integrity(full)`은 admission drain한 고정 상태에서 전체 row↔index·unique·catalog·allocation/undo 참조를 bounded 순회하는 별도 작업이다. 추가 disk spill은 요구하지 않는다. S1/fault 인수는 정상 reopen 성공 후 이 검사와 독립 이력 검산을 함께 실행한다. 따라서 검증을 생략하여 PASS시키지 않으며 정상 서비스의 startup 비용과 audit 비용을 분리한다. 같은 0/소량 WAL·같은 dirty-page 수의 256 MiB/1 GiB fixture에서 정상 reopen I/O가 전체 DB scan으로 증가하지 않는지 검사한다.

STABILIZE는 이미 영속 선택된 base closure와 그 뒤 새 WAL/CLR·epoch/namespace의 영속성을 확인한다. 필요 page write는 doublewrite 규약을 지킨다. 이 장벽 실패 시 사용자 admission/READY/reclaim을 금지한다. 이미 durable 선택된 recovery WRITER를 되돌리지 않는다. RESUME은 그 epoch를 계속 쓰거나 tip에 새 epoch를 연결하며 다음 restart가 앞선 UNKNOWN의 채택과 새 ACK를 놓치지 않아야 한다. 반복 recovery의 transient workspace는 사전 예약된 memory/doublewrite pool만 사용하고 DB 크기만큼 scratch를 생성하지 않는다.

<a id="checkpoint"></a>
## 12. fuzzy checkpoint·WAL 회수·backup

checkpoint는 전체 data snapshot이 아니다. begin-checkpoint 기록 뒤 commit coordinator와 append/registry/DPT 경계에서 **capture LSN**과 그 위치까지의 ATT(UndoRef/progress·잔여 recovery obligation 포함)·DPT·allocator high-water/예약 소유·published q를 함께 복사한다. capture는 CommitAttempted 거래의 sync/상태 설치가 끝날 때까지 기다리며 불명 commit이면 checkpoint를 진행하지 않는다. 이미 append된 update·CLR·commit·abort의 DPT/head/status/예약 등록이 끝나야 capture할 수 있다. lock 밖에서 end-checkpoint metadata chunks를 쓰고 sync한 뒤 CURRENT가 begin/capture/end LSN·chunk digest·current/previous identity를 atomic replace+dirsync로 선택한다. dirty pages를 모두 flush할 필요는 없다. WRITER는 checkpoint가 되돌리지 않는다.

mutable datafile을 공유하므로 previous checkpoint는 **이전 데이터 이미지가 아니라 더 이른 analysis/redo 시작점**이다. fallback은 그 metadata와 필요한 WAL/undo/namespace가 모두 남고 현재 page를 redo로 설명할 수 있을 때만 허용한다. datafile 손상을 previous selector만으로 치료하지 않는다. CURRENT 손상 때 임의로 큰 파일 번호를 선택하지 않는다.

WAL 삭제 floor는 current/previous checkpoint 각각의 redo_start, live dirty recLSN, checkpoint 진행, 미완결 system 작업, backup/archive/RP owner의 최소 필요 LSN이다. Active 거래의 최초 LSN은 undo store 독립성 조건을 만족하면 floor에 넣지 않는다. free/undo/status 변경도 home flush와 checkpoint 인계 전까지 WAL을 보존한다. 단조 floor 확정과 Retiring 등록은 coordinator→retention 경계에서 수행하고 unlink/dirsync는 lock 밖이다. old/new selector 불명 때 합집합 floor를 보존하고 새 삭제를 막는다.

exact-cut backup의 초기 방식은 **offline admission drain**이다. 새 거래/maintenance의 외부 admission을 닫고 기존 거래·cursor의 정상 종료와 진행 중 aging/DDL의 완료를 기다린다. backup의 capacity 예약과 공유12MiB workspace/진행 turn의 실제 독점 획득을 구별하며, 기존 작업의 종료에 필요한 workspace/turn을 가진 채 drain을 기다리지 않는다. 기존 commit/rollback/terminal·이미 시작한 작업의 예약된 하위 단계와 전용 flusher는 drain 중 허용하며 timeout이면 gate를 복원하고 Busy를 반환한다. reader 강제 만료나 숨은 commit을 하지 않는다. drain 뒤 C를 고정하고 모든 dirty data/undo/status/allocator를 WAL-first/doublewrite로 flush+sync한다. backup 소유의 flush/checkpoint는 명시적 drain 권한으로 진행하여 자신이 닫은 외부 admission을 다시 기다리지 않는다. 완결 checkpoint를 선택하고 mutation을 중지한 상태에서 allocated pages/파일 metadata를 streaming 복사한다. backup 파일과 manifest·namespace를 sync한 뒤만 성공을 알리고 admission을 복원한다. clone reflink나 immutable live datafile을 가정하지 않는다. 취소/복사 실패는 source C를 유지하고 불완전 destination은 성공 backup으로 선택하지 않는다.

restore는 빈 소유 경로에 완전 backup closure 검증→전체 무결성 검사→새 timeline·writer epoch·base q=C와 genesis WAL/checkpoint/control·고정 bootstrap 슬롯 선택→sync 후 개방한다. drain한 backup에는 loser가 없으므로 검증된 **loser abort obligation**은0이다. terminal retained undo/status·aging/retirement queue의 소유·credit·실제 charged bytes까지0이라는 뜻은 아니다. 남은 작업과 예약을 새 namespace의 base ledger에 검증하여 인계하고, 모든 해당 소유의 종료를 확인한 경우에만 전체 obligation ledger를 빈 것으로 시작한다. source WAL 없이도 이 인계를 복구할 수 있어야 하며 live page나 잔여 예약을 임의로 Free/0으로 만들지 않는다. backup manifest는 source의 완결 WAL high-water LSN을 포함한다. 새 timeline의 첫 LSN은 그보다 크게 시작하여 복사된 pageLSN을 새 WAL이 잘못 건너뛰지 않게 한다. 모든 복사 pageLSN이 manifest high-water 이하인지 검증하고 이 baseLSN은 새 control에서 인증한다. source WAL 파일 자체는 필요하지 않다. source WAL 없이 복원→새 commit ACK→두 번째 restart를 검사한다. online backup/PITR/replication은 후속 계약이며 offline exact cut을 그것들의 완료로 주장하지 않는다.

backup의 논리 C는 drain 직후 고정하지만 manifest의 물리 WAL high-water L은 flush/retirement·checkpoint 선택이 끝난 뒤 확정한다. checkpoint begin/end 등 그 사이의 완결 WAL도 포함해 sync하고 ownership/base ledger를 같은 최종 cut에 결합한다. 추가 page 효과가 있으면 그 dirty와 closure도 먼저 안정화한다. 이후 복사 동안 WAL/page/ownership은 고정하며 restore 첫 LSN은 이 L보다 커야 한다. drain 때 관측한 LSN을 최종 high-water로 재사용하지 않는다.

<a id="reclaim"></a>
## 13. 공간 재사용·진행 보장

whole-pack GC, 모든 root 도달성 mark, 전체 DB를 새 pack으로 쓰는 필수 compaction을 제거한다. allocator는 free page/extent를 재사용하고 aging은 owner queue에서 undo·삭제 row/index·retired object를 horizon 기준으로 정리한다. index key를 먼저 정리하고 row/overflow 참조를 제거한 뒤 slot/extent를 반환한다. page 재사용은 durable allocation generation과 WAL init으로 보호한다. page 내 빈 cell 정리는 logged page compaction이며 전체 DB rewrite와 다르다.

장기 reader/Active transaction은 undo와 retired space를 보존한다. quota 부족 때 새 statement/write를 거절하고 원인을 oldest snapshot/undo owner와 함께 보고한다. reader를 몰래 만료하거나 필요한 undo를 덮어쓰지 않는다. crash 뒤 orphan build 정리는 allocation ownership·transaction status를 검증한 뒤 batch로 수행한다. 낮은 free space에서도 이미 시작한 rollback·flush·checkpoint·aging은 아래 reserve 안에서 진행해야 한다.

초기 전체 owned disk quota 8 GiB 안에 **1 GiB를 maintenance/rollback 전용**으로 예약한다. foreground가 이 reserve를 소비하지 못한다. transaction admission은 실제 undo bytes·최악 CLR encoded bound·rollback 재삽입/split에 필요한 page·WAL/checkpoint 여유를 계산하고 reserve와 중복 없이 선점한다. bound를 계산할 수 없는 operation은 시작 전에 거절한다. fixed doublewrite 16 MiB, checkpoint metadata 두 세대와 회수 진행점 공간도 이 예산에 포함한다. 작은 rollback page pool을 일반 insert에 대여하지 않는다. OS의 실제 ENOSPC/I/O 불명은 여전히 Fenced 모델이며 quota가 장치 장애를 해결한다고 주장하지 않는다.

<a id="reserve-accounting"></a>
### 13.1 admission과 진행용 자원의 계산

예약은 디스크 bytes 한 숫자가 아니라 `data/undo/status/allocator pages, WAL bytes, control bytes, frame pins, memory, lock/registry slots, FD, DWB slots`의 자원 벡터다. F05의 opcode별 표는 입력 한도, forward/undo/CLR 최댓값, 구조 변경·재시도 비용, 소비/반환 시점을 갖는다. 표가 없는 operation이나 overflow한 산식은 첫 effect 전에 거절한다. 단순히 논리 after-image 1 MiB를 WAL/CLR 상한으로 복사하지 않는다.

| 계산 항목 | 보수적 산정 규칙 |
| --- | --- |
| page 수 | row/index 각 경로의 최대 높이 16, 최대 8 index, 128 KiB row의 overflow·undo chunks, root split/anchor, 참조 count·status·allocator page를 합산. 공유 page 감소는 실제 할당을 확인한 경우만 인정 |
| WAL bytes | 실제 codec의 길이 상한으로 forward update + 필요한 redo-only 구조 그룹 + 각 undo의 CLR/진행점 + abort-end를 계산. undo 중 발생할 split/allocator 그룹과 frame header/checksum도 포함. 이미 소비한 예약은 잔여 예약과 중복 집계하지 않음 |
| 물리 disk | 이미 할당한 파일 bytes와 신규 extent/control의 성장 예약을 구분. 새 extent는 64-page 단위로 올림. 기존 파일의 free page token은 소유권 자원이며 파일 bytes를 다시 추가하지 않음. DWB 고정 pool·선택된 checkpoint 두 세대·새 checkpoint 설치 peak도 각각 한 번 집계 |
| 메모리/frame | mini-transaction 적용의 전체 pin 집합, 최대 CLR encode/decode, 논리 재탐색·split 및 transient reference pin을 계산. frame 예약은 전체 집합 단위이며 부분 pin 보유 중 대기 금지. 고정 flush 몫은 항상 별도 보존 |
| 실행 중 잔여량 | 성공 statement 뒤에도 전체 transaction abort에 필요한 예약은 유지. statement rollback의 소비량과 Active 복귀 후 필요한 잔여량을 다시 계산. 취소·deadlock victim·Drop 인계도 같은 예약을 넘겨받음 |

모든 자원 `r`에 대해 `charged_r + outstanding_reserved_r <= capacity_r`를 동시에 만족해야 admission한다. disk에는 별도로 `owned_bytes + reserved_growth_bytes <= 8 GiB`, protected reserve에는 `fixed_maintenance_bytes + sum(transaction_abort_reserve_bytes) <= 1 GiB`, WAL에는 `retained_WAL_bytes + outstanding_WAL_reservations <= 256 MiB`를 적용한다. 1 GiB는 8 GiB 안의 분할이며 별도 추가 용량이 아니다. fixed_maintenance에는 DWB·checkpoint 교체 peak·aging 진행점과 작업 완료에 필요한 여유가 들어간다. 두 슬롯에 같은 free page/byte를 예약하지 않는다. 물리 선할당이 필요한 새 extent는 effect 전에 확보하며 OS/장치의 외부 ENOSPC·I/O 장애는 여전히 fence 대상이다.

rollback/flush/recovery가 요구하는 frame·memory·FD·WAL 최소 몫을 foreground와 분리한다. 12 MiB 유지보수 workspace는 rollback을 포함하여 한 작업만 사용하고, 대기 작업은 tree/page latch나 부분 pin 집합을 보유하지 않는다. rollback의 예약 완료 경로는 자기 완료를 위해 checkpoint/aging의 선행 공간 회수를 요구해서는 안 된다. 반대로 checkpoint/aging은 active 거래의 정상 종료를 기다리며 작업 workspace를 붙잡지 않는다. 진행 작업은 [progress turn](#mutation-protocol)으로 안정된 재탐색·유한 latch drain을 확보한다. try 재시도만으로 진행을 주장하거나 예약된 rollback을 일반 BufferBusy로 버리거나 성공 ABORT로 답하지 않는다.

F05는 4/16/64 MiB buffer 각각에서 이 부등식과 opcode bound가 성립하는 실제 최대 동시 admission을 산출한다. 최대 32 transaction은 허용 상한이며 모든 최대 크기 거래 32개를 반드시 동시 수용한다는 뜻이 아니다. 지원 profile에서 필수 progress reserve가 들어가지 않으면 profile 미충족으로 기록하고, 수치를 실행 후 줄여 PASS로 바꾸지 않는다. [최악 조건 인수](../plans/dev3-implementation-plan.md#resource-proof)를 통과하기 전 진행 보장을 검증 완료로 표시하지 않는다.

aging cursor/진행점은 재실행 가능한 logged state다. 메모리 batch·disk 진행점은 고정 상한이며 오래된 전체 mark/sort scratch를 만들지 않는다. 거래 시작 WAL을 실제 제거한 상태의 rollback/recovery, 긴 reader 해제 뒤 재사용 증가, quota 경계의 반복 crash 및 rollback pool 고갈 거절을 필수 인수한다. online shrink/OS 반환·재배치 compaction은 S1 필수가 아니다. 내부 재사용량과 filesystem 반환량은 구별한다.

<a id="recovery-reservation"></a>
### 13.2 crash 뒤 예약 복원

예약의 휘발성 계수만으로 진행을 보장하지 않는다. **잔여 recovery obligation을 WAL·checkpoint로 영속 인계**하는 방식을 채택한다. 거래/미완결 system 작업별 descriptor는 owner/generation, undo head·target/progress, 아직 필요한 CLR/구조 변경/abort-end/status/control의 상한, 이미 소유한 page/extent token과 미소비 성장/WAL 예약을 연결한다. 최대 32 ATT와 bounded system 작업 외에 전체 undo/page 목록을 RAM에 올리지 않으며 큰 소유 inventory는 allocator의 영속 구조로 순회한다. descriptor의 구체 bytes·최대 크기·검산 규칙은 F01/F05 입력이다.

1. **runtime 기록:** forward effect를 허용하는 완결 WAL 그룹은 해당 effect로 늘어난 obligation과 예약 소유를 함께 기록한다. CLR/rollback split 그룹은 적용 효과·UndoNextRef와 함께 소비/잔여 obligation을 갱신한다. CLR의 구조 변경과 논리 undo가 다른 그룹이면 중간 상태도 자기 잔여 비용을 보존한다. statement 성공은 전체 abort 몫을 해제하지 않으며, statement rollback 완료 뒤에는 남은 앞선 statement의 obligation을 유지한다. terminal commit/abort의 채택이 확정된 뒤만 해당 abort 몫을 반환한다. record 누락·중복 적용을 owner generation과 적용 LSN으로 검출한다.
2. **capture와 복원:** checkpoint는 같은 capture LSN의 descriptor와 allocator ownership을 인계한다. ANALYSIS는 capture 직후의 완결 WAL을 순서대로 적용하여 채택한 끝의 잔여량을 만든다. 이 계산은 아직 redo하지 않은 home undo/status의 최신성에 의존하지 않는다. begin WAL을 삭제하려면 이 closure와 소유 inventory의 redo 경로가 남아야 한다. RESERVE_RESTORE는 checkpoint+WAL에서 복원한 같은 채택 prefix의 예약 소유 inventory와 descriptor를 검산하고 recovery owner에게 한 번만 인계한다. 이 inventory는 예약 소유의 bounded/streaming 표현이며 전체 allocator의 RAM 복제가 아니다. REDO가 끝나면 home allocator/undo/status와도 대조한 뒤 UNDO에 진입한다. REDO가 home page를 따라잡게 하더라도 논리 예약을 두 번 차감하지 않는다.
3. **이중 계산 금지:** 이미 존재하는 extent/file bytes는 charged이고 그 안의 예약 page token은 별도 소유다. 아직 파일로 실현되지 않은 growth만 outstanding growth다. 부분 recovery가 남긴 실제 WAL/CLR/header bytes는 retained/charged에 포함하고 같은 bytes를 잔여 WAL 예약으로 재사용하지 않는다. 끝난 undo는 남은 obligation에서 제외하되 그 CLR bytes가 실제 회수되기 전까지 charged에서 빼지 않는다. runtime의 모든 자원 부등식을 다시 동시에 확인하며 protected 1 GiB를 전체 8 GiB에 추가하지 않는다.
4. **bootstrap과 재장애:** DB 생성/foreground admission 때부터 fixed maintenance에 DWB repair·streaming analysis/redo의 frame/memory/FD·전용 checkpoint 두 슬롯/selector staging·기존 checkpoint와 공존하는 peak·recovery epoch/header/WRITER 설치 여유를 포함한다. BOOT_RESERVE는 선택 control의 고정 profile·검증한 FILEMAP/namespace로 이 몫을 확보하며 home allocator/undo를 먼저 읽지 않으므로 undo를 읽기 위해 먼저 undo 예약이 필요한 순환이 없다. 새 epoch/header도 쓰기 **전** WAL256 MiB와 owned/protected quota에 charge한다. [RECOVERY_REBASE](#recovery-rebase)는 새 WAL append 없이 두 슬롯을 인계/재사용하고 필요한 bytes를 회수하여 다음 epoch의 예약을 복구한다. 채택된 prefix나 current/previous가 아직 요구하는 bytes는 재사용할 수 없다. 이 bootstrap 인계는 loser undo 완료나 foreground admission에 의존해서는 안 된다.
5. **진행과 실패:** runtime admission bound는 모든 허용 crash cut에서 bootstrap+복원한 잔여 작업이 기존 예약 안에 들어간다는 귀납 조건을 만족해야 한다. 단지 restart 때 부족을 발견해 거절하는 것을 정상 admission의 성공 증명으로 삼지 않는다. 정상 durable 이미지·고정 profile의 **모든 유한 crash prefix에서 closure·예약 안전성과 고정 peak**를 유지한다. 완료 보장은 유한한 잔여 obligation, 충분히 긴 무장애 실행 구간, 종료하는 정상 I/O와 공정한 actor 실행을 전제한다. 매 CLR 영속화 직전에 무한히 중단되는 실행의 완료를 보장하지 않는다. crash 횟수에 따른 공간 누적 방지와 마지막 장애 뒤 rollback 종료는 별도 증명 의무다. 손상/불명 I/O/필수 자원 확보 실패는 open 거절·Fenced이며 READY나 ABORTED 성공이 아니다. 임의 손상된 descriptor를 축소하거나 foreground 몫을 빌려 맞추지 않는다.

F05는 각 forward/CLR/SMO/terminal/capture/bootstrap 전후의 `charged + outstanding` 보존과 반복 restart의 고정 peak를 수치로 증명한다. F04는 BEGIN WAL 삭제·부분 CLR·새 epoch 선택·bootstrap checkpoint 인계/회수의 재장애를 포함한다. 이 동결·모델·실제 I/O 인수는 **NOT_RUN**이며 구현 전 미완료 의무다.

<a id="retention-pressure"></a>
### 13.3 보존 압력과 운영 진단

오래된 snapshot 때문에 undo가 회수되지 않으면 [예약 회계](#reserve-accounting)에 따라 **다음 statement의 첫 effect 전에** 자원 부족을 반환한다. 해당 statement가 이미 적용 중인 예외는 기존 undo/오류 효과를 따른다. 공간이 풀릴 때까지 같은 요청을 내부에서 무한 재시도하거나, live snapshot을 시간만으로 만료시키거나, 필요한 undo를 덮어써 진행시키지 않는다. timeout/cancel은 해당 소유자 종료 경로를 거쳐 실제 pin/undo 소유가 해제된 뒤만 회수 조건을 바꾼다. 이미 예약된 rollback·flush·recovery 몫은 새 write가 빌릴 수 없다.

bounded 진단에는 부족한 자원과 `capacity/charged/reserved/requested`, 보존 이유별 owner 종류·generation, snapshot q 또는 WAL LSN, oldest-owner age, 재사용 가능/실제 회수량을 담는다. q와 LSN을 섞지 않으며 reader가 언제나 WAL도 pin한다고 표시하지 않는다. 최대 항목 수·잘림 표시를 고정하고, 합계는 겹친 보존 범위를 중복 합산하지 않는다. 진단은 회수 결정의 정본이 아니며 pressure 상태에서도 사전 예약된 제한 공간으로 생성한다. 진단 실패가 rollback 진행이나 기존 오류의 확실성을 바꾸지 않는다. 원문 SQL·행·bind 값은 넣지 않는다.

DS03/04는 긴 reader를 유지한 채 churn→명시적 admission 거절→기존 reader 결과 보존→reader 해제→aging/공간 재사용→새 admission 성공을 확인한다. 충분한 공간이 없으면 새 admission은 계속 거절할 수 있다. 보호할 데이터 유실 없이 실제 자원이 반환된 경우의 재개를 검증하며, 전체 DB 쓰기 가용성을 약속하지 않는다. 향후 archive/RP owner는 [별도 확장 의무](../evidence/commercial-extensibility-research.md#future-operations)를 충족하기 전 현재 REBASE의 고정 peak 증명에 포함된 것으로 취급하지 않는다.

<a id="budgets"></a>
## 14. 자원·성능

| 자원 | 초기 공학적 profile |
| --- | --- |
| page buffer | 64 MiB/4,096 frames; 4/16 MiB fault profile. dirty/clean 합계 |
| metadata/cache | 합계 8 MiB; frame map/DPT·bounded lock table·catalog/locator·FD cache. locator 1 MiB, FD 최대 128 |
| transaction | 최대 32; mutation 256, 논리 payload 1 MiB, private 8 MiB/거래·전체 64 MiB. 추가 잠금/undo staging을 포함 |
| system workspace | 합계 16 MiB: flush/DWB I/O buffer 4 MiB 고정 + rollback/recovery/checkpoint/build/full-check/aging 중 한 작업 12 MiB. foreground가 고정 flush 몫을 빌리지 않음 |
| disk/WAL | owned 8 GiB·maintenance reserve 1 GiB·보존 WAL 256 MiB·doublewrite 16 MiB. 전체 quota와 하위 한도를 동시에 적용 |
| snapshot/diagnostics | user snapshot 32, backup/checkpoint 각 1; trace 4 MiB/4,096 events. 필수 receipt 유실은 실패 |

대형 index build의 run/disk 목록은 별도 사전 예약·owned staging이며 외부 정렬의 FD/fan-in·취소 정리가 bounded해야 한다. data/index/row locator/할당 inventory 각각 accessed bytes ≥ configured buffer 4배 기준을 유지한다. inventory는 이제 파일 pack 목록 대신 page/extent allocation inventory다. 필요한 큰 inventory fixture와 quota는 실행 전에 F05에서 정한다. 단지 작은 metadata cache를 넘었다는 것으로 인수를 대체하지 않는다.

PF09는 성공 TPS/충돌률, commit·WAL sync p99, background data flush/doublewrite bytes, page read/hit/pin/latch/lock wait, undo 생성/보존/aging량, 재사용 가능/실제 재사용 공간, checkpoint·정상 reopen·full-check·backup 중단시간, RSS/FD/rollback reserve peak를 기록한다. page-cache miss와 실제 장치 I/O를 구별한다. baseline/candidate는 동일 의미·durability·fixture/환경에서 비교하고 성능 개선을 미리 선언하지 않는다.

<a id="capacity-profile"></a>
### 14.1 형식 한도·실행 한도·복구 요구량

초기 수치에는 서로 다른 의미가 있다. F01/F05에서 각 한도를 **형식/알고리즘의 표현 상한**, **신규 foreground admission 한도**, **이미 영속 인계된 복구 요구량**으로 분류한다. page/slot/길이/ID 산술과 codec 상한은 검증된 표현의 범위이며 설정 숫자만 올려 확대하지 않는다. 유효한 신규 작업은 표현 상한과 검증된 실행 profile 양쪽을 만족해야 하고, 여러 statement의 거래 누적량도 검사한다. 모든 API 소비자는 engine이 정한 유효 한도를 사용하며 SQL이 더 큰 engine 용량을 추정하지 않는다. 실제 설정 API나 wire capability 필드를 미리 추가하는 결정은 아니다.

같은 build 재시작에서도 **새 foreground 한도는 이미 admitted된 거래의 복구 한도가 아니다.** 초기 구현은 DB 생성 시 선택된 복구 자원 profile을 checksummed control로 식별하고, 해당 profile을 충족하지 못하는 실행 설정은 PREFLIGHT에서 DB 쓰기 전에 거절한다. BOOT_RESERVE/RESERVE_RESTORE는 그 profile과 실제 잔여 obligation을 사용한다. 예를 들어 새 거래 한도를 낮췄다고 checkpoint의 ATT·undo/CLR 잔여량을 자르거나 더 적은 거래만 복구하지 않는다. 같은 profile 안의 신규 admission 축소는 허용할 수 있으나 복구에 필요한 자원을 반환하지 않는다. profile 변경·더 작은 복구 설정 지원은 별도 증명/동결 대상이며 자동 변환하지 않는다.

F01은 profile identity·필수 항목·무결성/지원 여부의 검사 위치를, F05는 실제 opcode bound와 동시 admission 표를 고정한다. 식별자만 일치하고 실제 memory/frame/FD/disk 예약이 부족한 경우도 open 성공으로 표시하지 않는다. 미지원 설정과 손상 descriptor를 다른 원인으로 진단한다. 현재 build에서 정상 profile로 다시 open할 때의 ACK·loser undo 보존을 DS05로 검증하며, 사내 이전 버전 호환 구현을 요구하는 규칙은 아니다.

<a id="acceptance"></a>
## 15. 구현 전 동결과 인수

[계획](../plans/dev3-implementation-plan.md#acceptance)과 DS01–06·KU09/10·RC-T12–14·PF09가 소유한다. F01 byte layout/feature, F02 API·isolation·잠금/undo 상태표, F03 독립 oracle/vector, F04 실제 I/O/fault/doublewrite·반복 복구, F05 자원·환경·성능, F06 DEV2 실제 공급, F07 파일/작업 소유를 leaf 전에 동결한다. 설계만으로 제품 PASS를 기록하지 않는다.

<a id="parallel-development"></a>
## 16. 작업 경계

공통 identity/page/WAL/undo/visibility 계약→page/allocator·WAL·buffer/flush와 현재 연산의 제한 복구→일반 buffer·row/undo/status와 구조 tree→첫 행 대상의 statement rollback·commit·전체 복구 장벽→index/catalog의 나머지 기능과 대상별 거래·복구→checkpoint/aging/backup 순으로 합친다. 첫 행의 실제 table/anchor 공급은 필요하지만 index/catalog 전체 완료를 commit·복구의 선행으로 두지 않는다. [방향 리뷰 후속 결정](../plans/dev3-implementation-plan.md#direction-review-adoption)과 [현재 우선순위](../plans/dev3-implementation-plan.md#work-priority)가 선택 순서를 소유한다. 제한 복구는 일반 READY가 아니며 전체 task DAG·S1 인수는 유지한다. pack 전용 병렬 역할은 폐기한다. undo와 WAL, dirty buffer와 doublewrite는 독립으로 성공했다고 durability를 주장할 수 없다. 실제 소유와 최소 선행 leaf는 [구현 계획](../plans/dev3-implementation-plan.md#parallel-development)을 따른다.

<a id="review-followup"></a>
## DEV3-REVIEW-002 — 먼저 반영한 리뷰 보완

2026-10-18 D2 ACCEPTED. 부분 NULL UNIQUE·statement-final 검사, global-q의 초기 한계, full VERIFY 비용 시험, 파일 수 기반 GC scratch 상한/반복 장애 정리, compaction 전체 공간 계산을 먼저 반영했다. `/root/dev3_redesign_review`가 원본 대비 세 파일을 검토하여 PASS, 통합을 막는 P1/P2 없음. `/root`가 확인했다. 제품/문서 자동 검사 미실행. UNIQUE/vector와 성능 관측 의무는 유지한다. 중간 COW GC/scratch/compaction은 아래 003으로 대체하고 동시성과 정상 recovery 분리는 7·11절로 닫는다. 선행 보완 snapshot 지문은 [이번 조사](../evidence/dev3-wal-undo-research.md)에 보존한다.

<a id="decision"></a>
## DEV3-DESIGN-003 — in-place·WAL·영속 undo

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당/계약/통합 | `/root`; 별도 검토 `/root/dev3_redesign_review` |
| 위임 근거 | 사용자가 Oracle·ARIES 및 로컬 디스크 구현 비교를 요청하고, 리뷰 보완을 먼저 반영한 뒤 COW pack/GC 재설계를 명시 지시 |
| 선택 | in-place B+tree·dirty bounded buffer·WAL-before-page·disk undo·redo-only CLR·doublewrite·fuzzy checkpoint·offline exact-cut backup·horizon aging |
| 대체 | DEV3-DESIGN-001/DEV3-PARALLEL-001의 COW/pack/closure/공개·복구·회수 규약. REVIEW-002의 UNIQUE 보완 유지. 과거 소스 조사·검토 사실과 실제 DEV2 코드/증거는 보존 |
| 적용 | DEV3와 후속 disk 소비 경로. DEV2 전용 private/COW 기준선과 format은 별개. 기존 과거 COW 설명은 DEV3 규범보다 우선하지 않음 |
| 한계 | READ COMMITTED 명시 선택, 기본 Serializable의 table 단위 false conflict, 단일 commit coordinator·직렬 SMO, offline backup. 모든 byte/API/fault profile 미동결·제품 실행 NOT_RUN |
| 독립 검토·확인 | `/root/dev3_redesign_review` 최종 PASS. 초안 P1 3건·P2 2건과 INIT 복원 경계 해소, 통합 차단 지적 없음. `/root` 반영 확인. 제품/문서 자동 검사 미실행; [기록](../evidence/dev3-wal-undo-research.md#review) |
| 재검토 | reserve/undo 회수 진행 불능, dirty-page/CLR 재장애 반례, RTO/I/O 과다, Oracle 유사 의미 확대, online backup/고급 동시성 요구 |

<a id="architecture-review"></a>
## DEV3-REVIEW-004 — 아키텍처 비교 후 구현 경계 보완

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당·권한 | 작업/계약/통합 `/root`; 사용자의 DBMS 비교 리뷰 후 문서 수정 지시와 DP01 위임 |
| 선택 | 짧은 version-edit gate와 다중 page 순서, transient reference pin·영속 edge/queue 상태, 자원 벡터 예약, 최대64 page DWB batch, DEV2 재사용/재구현 inventory |
| 대안·이유 | 모듈별 독립 latch/비공식 refcount는 교차 증명이 어려움. 초기 적용 직렬화로 명시적 순서를 먼저 검증. DWB의 page별 동기화만 고정하면 기본 I/O 비용이 커질 수 있어 같은 장벽의 bounded batch 허용. undo segment 방식으로 전면 재설계하는 선택은 이번에 채택하지 않음 |
| 적용·대체 | DESIGN-003의 저장 구조·공개 SQL 의미·stage DAG 유지. 7–9·13절의 적용/수명/flush/예약 세부를 구체화. 단일 page flush만을 요구하는 해석은 9.1절로 대체. DEV2 코드/형식/과거 receipt와 앞선 독립 검토 identity는 보존 |
| 검증·독립 검토 | [이번 근거와 작업 기록](../evidence/dev3-dev4-architecture-review.md). `/root/dev34_architecture_review`가 P1 진행 규약 보완 뒤 설계 채택 PASS·추가 필수 P1/P2 없음 확인. `/root` 반영 확인. 모델·제품·성능 시험 NOT_RUN, 문서 자동 검사는 push gate |
| 남은 입력 | 실제 opcode/codec bound, Rust guard/API, 상태 탐색 action/corpus, fault byte images·deadline·성능 수치. 문서 보완은 해당 계산·실행의 완료가 아님 |
| 재검토 | gate 병목, bounded model 반례/미완주, 예약 미충족·rollback 진행 실패, 배치 crash 반례, DEV2 경로 중복 유지비용 |

<a id="snapshot-review"></a>
## DEV3-REVIEW-005 — snapshot 등록과 aging의 원자 경계

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당·권한 | 작업/계약/통합 `/root`; 사용자의 DEV3·DEV4 리뷰 지적 수정 및 commit 요청. 전체 문서 검사·push는 후속 지시로 제외 |
| 문제 | 최초 q 선택과 horizon 등록 사이의 회수 경쟁, status 설치 후 frontier 공개 전의 새 reader 보호가 미명시 |
| 선택·대안 | 기존 version-edit gate로 q 선택/등록과 aging 결정을 조정하고 published q를 보존 기준에 포함. 새 전역 lock 추가나 DEV2 retention/publication 순서 재사용 대신 기존 순서를 구체화 |
| 적용·대체 | DESIGN-003/REVIEW-004의 snapshot 획득·horizon 규약을7.2절로 구체화. WAL·격리·SQL 의미·단계 DAG·과거 evidence는 유지 |
| 검증·검토 | D3-M02·DS04와 DEV4 수신/동시성 인수 보강. `/root/snapshot_design_review`의 독립 설계 검토 PASS·필수 P1/P2 없음, `/root` 반영 확인. [작업 기록](../plans/dev3-implementation-plan.md#snapshot-review). 모델·제품·전체 문서 검사 미실행 |
| 재검토 | snapshot/registry/gate 경계 변경, frontier 이전 회수 반례, pin 인계 유실 또는 새 admission 순환 |

<a id="conflict-reserve-review"></a>
## DEV3-REVIEW-006 — 관측 행 버전과 crash 뒤 예약 인계

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당·권한 | 작업/저장 계약/통합 `/root`; 사용자의 DEV3·DEV4 리뷰 후 “수정해 주세요” 지시 |
| 반례 | 관측 뒤 타 거래가 commit/unlock하여 잠금 대기가 없으면 stale write 검사가 생략될 수 있음. crash로 runtime 예약 ledger가 사라지면 부분 CLR 뒤 남은 rollback 자원 소유가 불명 |
| 선택 | [행 관측 버전](#observed-row-version)을 모든 X-lock 획득 후 검사. [영속 obligation](#recovery-reservation)을 effect/progress와 함께 기록하고 bootstrap·예약 복원·새 epoch 전 고정 두 슬롯 REBASE를 recovery 장벽으로 추가 |
| 대안·이유 | wait-only 또는 payload/page 비교는 논리 변경을 놓침. commit-only 검사는 native RC를 보호하지 못함. restart에서 undo를 읽은 뒤 새 예약을 요청하는 방식은 bootstrap 순환·quota 부족을 남기므로 checkpoint/WAL에서 복원 가능한 descriptor 선택. 새 epoch를 무조건 추가하면 재장애 비용이 누적되므로, 기존 WAL을 수정하지 않는 고정 슬롯 base 인계를 먼저 수행 |
| 적용·대체 | DESIGN-003/REVIEW-004의 “잠금 대기 뒤” 조건과 runtime-only로 해석될 수 있는 예약 인계를 구체화. REVIEW-005 snapshot 소유, 저장 구조·격리 기본값·SQL 범위·stage DAG 유지. 이전 검토/증거 identity는 보존 |
| 인수·검토 | DS03/05/06·RC-T14, M03/M04/M06, F01/F02/F04/F05. `/root/dev34_conflict_reserve_review` 최종 독립 설계 검토 PASS·필수 P1/P2 없음, `/root` 반영 확인; [작업 기록](../plans/dev3-implementation-plan.md#conflict-reserve-review). 모델·제품·전체 문서 검사 NOT_RUN |
| 재검토 | 관측 버전의 ABA/own-write 오판, descriptor/allocator 불일치, bootstrap 재장애 누적 또는 quota 내 admitted rollback의 진행 실패 |

<a id="mathematical-review"></a>
## DEV3-REVIEW-007 — 문장 rollback 자원 회계와 복구 진행 전제

| 항목 | 기록 |
| --- | --- |
| 분류·일자·담당 | D2 · 2026-10-19 · 작업/계약/통합 `/root` |
| 권한·선택 | 사용자의 DEV3·4·5 수학적 재검토·문제 즉시 수정 요청과 DP01 위임. 6·13.2절의 자원 소유/진행 보장을 구체화. 기존 page/WAL/undo 형식·격리·stage gate 유지 |
| 대안·대체 | 논리 rollback 때 ledger까지 되감는 해석은 위 보존식을 위반하므로 실제 반환만 차감. 기존 예약·잠금 소유를 유지하며 무조건 복구 완료라는 해석은 13.2절의 진행 전제로 대체. 새 저장 형식/정책은 추가하지 않음 |
| 상태·인수·독립 검토 | ACCEPTED. DS03/DS06·M03/M06·F02/F05에 반영. `/root/dev345_independent_review`가 원문·반례·세 단계 계약/계획/인수 diff를 독립 검토하고 회계 등 축소 계산을 재실행: 필수 P1/P2 잔여 없음. `/root` 반영 확인. 이전 review identity·실행 결과 유지 |
| 참고 근거 | 2026-10-19 [ARIES 원 논문](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf)의 WAL/CLR·공간 예약 원리와 로컬 소스 선택 구간 대조. PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의 `src/backend/storage/buffer/bufmgr.c` FlushBuffer, CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5`의 `src/transaction/log_recovery.c` undo/compensate, MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613`의 `storage/innobase/buf/buf0dblwr.cc` batch 완료, SQLite `43a48f77585a88940c727f33682151418f544059`의 `src/wal.c` reader/checkpoint. 외부 구현과 동일하다는 주장이나 실행 증거가 아님 |
| 실행·재검토 | 산술 반례·축소 계산만 실행. 기존 단계 모델/제품/복구/성능은 NOT_RUN. 잔류 비용 누락·진행 가정 변경 또는 실제 leaf 반례가 생기면 재검토 |

<a id="extensibility-review"></a>
## DEV3-REVIEW-008 — 규모 확대 전 자원·진행 경계

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-19 |
| 담당·권한 | 작업/저장 계약/통합 `/root`. 사용자의 상용 수준 확장 한계 점검·선제 보완 요청과 DP01 위임 |
| 선택 | 9.2절의 flush service/backlog, 13.3절의 retention pressure/진단, 14.1절의 형식·admission·복구 profile 분리. 기존 DS03–06/PF09에 반례를 연결 |
| 대안·이유 | 한도 일괄 증가는 복구/예약 증명을 대신하지 못함. 전역 gate 분할·group commit 즉시 구현은 DEV3 기준선 실측과 하위 인수 이전에 복잡성을 늘림. 기존 단일 저장/commit 구조 안의 경계부터 명시 |
| 적용·대체 | REVIEW-004–007의 자원·진행 규약을 구체화. DEV2 코드·격리·durable ACK·기존 상한·stage DAG 유지. 후속 복제/온라인 백업을 S1 선행 조건에 추가하지 않음 |
| 근거·검토 | [소스 조사·반례·독립 검토](../evidence/commercial-extensibility-research.md). `/root/extensibility_review`의 독립 D2 설계 검토 PASS·잔여 필수 P1/P2 없음, 계약/통합 담당 `/root` 반영 확인. 제품·모델·성능 NOT_RUN |
| 재검토 | profile 변경/대량 거래·다중 flush 도입, 장기 pin의 quota 압력, flush starvation, durable archive/RP owner 추가 |

<a id="task-review-009"></a>
## DEV3-REVIEW-009 — 16개 작업의 DBMS 비교·반례 보완

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22 |
| 담당·권한 | 작업/저장 계약/통합 `/root`. 사용자의 ARIES·Oracle·로컬 DBMS 비교, 병렬 리뷰 및 문제 즉시 수정 요청 |
| 기준·범위 | HEAD `678168ebc519f2d2ff932c1cc07d84e639e24c6e`, 착수 clean. 16개 작업 설계와 직접 소유 계약·인수 연결. 제품 코드·원본 DBMS·기존 실행 결과 미변경 |
| 선택 | copy 전부터 finalize까지 flush 주소 claim, 거래 전체 own UNIQUE, 종속 index와 table의 원자 drop, 부분 rollback의 영속 skip·loser 전체 종료, WAL 의미/analysis/page redo 분리, restore의 retained 의무 인계, backup drain의 workspace/진행 권한 |
| 대안·이유 | pending만 검사하거나 이번 statement delta만 비교하면 각각 재사용·중복 반례가 남음. runtime before 검사를 replay에 재사용하면 정상 mixed page를 거절. loser0으로 전체 ledger를 지우거나 workspace를 가진 drain은 필수 작업의 진행을 잃음. 기존 구조 안의 명시 불변식으로 수정 |
| 적용·대체 | 위 소유 절과 해당 상세 설계·테스트 계획을 보완. 특히 §12의 restore 전체 빈 ledger 단정은 loser abort obligation0과 retained maintenance 인계로 정정. 저장 구조·NULL/격리·ACK 의미·실행 한도·단계 DAG·과거 결정 identity 유지 |
| 근거·독립 검토 | [8개 반례·16문서 판정·실제 소스 revision·교차검토](../evidence/dev3-task-design-review-20261022.md). `/root/review_storage`·`/root/review_tree`가 본인 작성 외 상세 수정·관련 정본을 교차검토. `/root/review_storage`가 공통 정본·신규 보고서·인수/색인 연결 최종 검토 PASS, 잔여 필수 P1/P2 없음. `/root` 반영 확인 |
| 실행·남은 의무 | 문서만 수정. 제품 시험·compiler/Clippy·문서 자동 검사·commit/push 없음. 새 case는 구현 동반 명세. F01/F02/F05·terminal queue/credit·복구/restore 형식의 미동결과 DEV3/S1 미완료 유지 |
| 재검토 | claim/free 경합·own presence 누락·DDL 의존 집합 변화·보상 suffix 재실행·정상 REDO 거절·복원 credit 소실·drain 교착 또는 새 운영 경로/형식 도입 |

<a id="task-review-010"></a>
## DEV3-REVIEW-010 — Commit 설치 순서와 유지보수 인계

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22 |
| 담당·권한 | 작업/계약/통합 `/root`, 독립 검토 `/root/independent_review`. 사용자의 16개 설계 비교 리뷰·즉시 수정·단일 commit 요청, AGENTS/DP01 독립 검토 규칙 |
| 기준·범위 | HEAD `def35fb1e274040eb55d78fa68948ba1610166c1`, 착수 clean. 기존 REVIEW-009 이후 16개 문서 재검토. 동시 DEV4/DEV5 작업은 별도이며 이번 변경·commit에서 제외 |
| 선택 | Commit stage부터 sync 뒤 전체 의미 설치까지 전역 stage/apply 장벽, drain 중 workspace/turn 반환과 build owner의 하위 진행 권한, flush/checkpoint 뒤 최종 backup WAL high-water와 ownership cut |
| 대안·이유 | 단일 commit coordinator/물리 Idle만으로 일반 producer의 shared-page 추월을 막을 수 없음. page별 미설치 목록은 초기 복잡성을 늘리고, sync 전 Committed 공개·I/O 동안 latch 보유는 기존 불변식 위반. 전역 장벽의 대기 비용은 PF09에서 측정. drain과 backup은 기존 소유/상한 안에서 순서를 보완 |
| 적용·대체 | §7/7.1/12와 3.04/05/07/08/10/11/12/13/15/16·테스트 계획. REVIEW-009를 유지하며 물리 sync 뒤 의미 admission 재개 조건과 backup 최종 L 확정 시점을 구체화. byte 형식·격리·ACK 의미·기본 한도·단계 DAG 변경 없음 |
| 근거·독립 검토 | [반례·16문서 판정·실제 source revision](../evidence/dev3-task-design-review-20261022-r2.md). `/root/independent_review`가 최종 설계 diff·보고서·case/NT/FT·PF09·색인 연결을 독립 검토: PASS, 잔여 필수 P1/P2 없음. `/root` 반영 확인 |
| 실행·남은 의무 | 문서만 변경. 제품 시험·compiler/Clippy·문서 verifier·push 미실행. 3.11-T11·3.07-T11·3.15-T12는 구현 동반 인수 명세. 일반 Commit/terminal·F01/F02/F05와 DEV3/S1은 미완료 |
| 재검토 | 지연 설치 추월·장벽/turn 교착·staged Commit 취소 오류, build 종료 정체, 복사 cut 불일치 또는 page별 병렬 설치/group commit 도입 |

<a id="task-review-011"></a>
## DEV3-REVIEW-011 — DBMS 비교 보고서의 설계·인수 반영

2026-10-25 · D2 · **ACCEPTED / 제품·모델·성능 미실행**. 독립 검토 `/root/dev3_design_review`: 최종24개 문서와 후속6문서 delta PASS, 잔여 필수 P1/P2 없음. 작업/계약/통합 `/root` 반영 확인. [반영 근거와 검토 기록](../evidence/dev3-comparison-adoption-20261025.md).

기존 mutable page·단일 WAL·durable Undo·default Serializable·durable ACK/UNKNOWN·기본 quota와 단계 DAG를 유지한다. 다음 상세 소유 절을 이 정본의 보강으로 채택하며, 실제 F01/F02/F05 수치/codec/API 동결·구현·인수를 대신하지 않는다.

| 설계 의무 | 상세 정본과 완료 경계 |
| --- | --- |
| opcode admission·rollback/restart | [자원 벡터](dev3-resource-bounds.md#resource-vector-proof). 모든 성분의 peak·확인 반환·잔여 예약 보존, 종료 의무는 새 admission에 의존하지 않음 |
| 최초 행 수명 | [plan4 이후 추가 의무](dev3-task-12-recovery.md#post-plan4-obligations). 최소 공통 terminal→Catalog CREATE/실제 table→하나의 row opcode→Undo/CLR/ACK·새 프로세스 복구. 완료 N=0을 일반화하지 않음 |
| 의미 설치·병렬 성능 | [Serializable 충돌](dev3-task-10-concurrency.md#serializable-conflict-bound), [Commit 서비스](dev3-task-11-commit.md#commit-service-bound). 현 장벽 보존, batch의 prefix validation 필수. group commit/미세 validation은 측정 뒤 별도 D2 후보 |
| normal checkpoint 입력 | [immutable capture](dev3-task-13-checkpoint.md#immutable-capture-source). pinned base+complete prefix, 같은-cut bounded ATT/DPT/volatile 소유 snapshot, partition 재생·turn 간 snapshot 수명·선택 불명 합집합 보호 |
| offline restore | [typed namespace rewrite](dev3-task-15-backup-integrity.md#namespace-rewrite). 고정 폭 identity 변환·참조 동등성/다중도 보존·digest coverage 분류·Releasing/credit 인계·새 high-water/ACK 재장애 |
| 공간·운영 비용 | [표현 비용](dev3-resource-bounds.md#representation-cost), [보존 압력](dev3-task-14-reclamation.md#retention-pressure-model), [측정](dev3-task-16-qualification.md#comparison-measurement). 산술과 실측 구분,64GiB profile은 기존 준비 방향이며 현재 지원 아님 |

계획은 [실행 순서와 완료분 보완](../plans/dev3-implementation-plan.md#comparison-adoption), 인수는 [D3-COMP-01–10](../plans/dev3-test-plan.md#comparison-adoption)을 따른다. 후속 의무가 기존 완료 codec/pool/plan4의 원 source·PASS를 소급 취소하거나 무변경 시험 반복을 요구하지 않는다. 현재 범위 밖 row semantics에 plan4 newer-home 예외를 그대로 확대하지 않는다. checkpoint/restore의 이번 선택은 이전 상세 문서의 미선택 대안을 구체화하며, byte 형식·실행 지원은 소비 구현 전에 별도 동결한다.

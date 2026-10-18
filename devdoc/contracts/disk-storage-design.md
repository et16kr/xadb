# DEV3 — 디스크 저장·인덱스 상세 설계

2026-10-18 · **DEV3-DESIGN-003 D2 ACCEPTED / in-place·WAL·영속 undo 설계 / 미구현·제품 시험 NOT_RUN**

[구현 계획](../plans/dev3-implementation-plan.md) · [이번 조사](../evidence/dev3-wal-undo-research.md) · [이전 조사](../evidence/dev3-source-research.md) · [커널 계약](implementation-contracts.md) · [DEV2 계약](dev2-kernel-design.md) · [복구 계약](recovery-logging-design.md) · [인수 항목](disk-test-matrix.json)

<a id="scope"></a>
## 1. 범위와 대체 관계

사용자가 선행 리뷰 보완 뒤 COW pack/GC의 재설계를 요청했다. [DEV3-REVIEW-002](#review-followup)를 먼저 검토·반영했고, 이어 **mutable disk page + 단일 WAL + 영속 undo + bounded dirty buffer**를 선택한다. 제품 저장 엔진은 DISK 하나이며 resident/mixed 엔진은 추가하지 않는다. 기존 DEV0/1·DEV2 P0/K0 구현과 실행 증거를 바꾸지 않는다. 영속 DEV2/DEV3는 미구현이다.

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
| `SnapshotGuard` | q와 undo/객체 수명 horizon 등록. DB 전체 page frame 또는 이전 root closure를 pin하지 않음 |
| `checkpoint` / `backup_exact` | fuzzy recovery metadata checkpoint / admission drain을 거친 exact C 복사. 서로 다른 작업 |
| `reclaim` | eligible undo·삭제 row/index·retired object·free extent를 제한된 batch로 aging. 전체 root mark/sweep 없음 |

CommittedView는 DB/timeline·published q·catalog 접근 identity의 작은 descriptor다. 각 변경은 TxnRef에 종속되고 commit 상태의 원자 공개로 row/index/catalog 전체의 가시성이 전환된다. 하나의 writer coordinator가 q를 부여하고 durable commit→transaction committed 상태→published frontier를 순서대로 설치한다. frontier 공개 lock 안 allocation/I/O/callback을 금지한다. commit metadata와 설치 자원은 미리 예약한다.

<a id="page"></a>
## 4. 페이지·파일·identity

page는 16 KiB, header 예약 128 bytes, SHA-256 32 bytes, `(offset:u16,length:u16)` slot directory와 cell 영역을 가진다. header는 DB/file/page identity, page incarnation, owner ObjectId/Generation, kind/level, pageLSN, bounds/count, reserved=0을 포함한다. 공통 kind는 row/index/catalog internal/leaf, overflow, undo, transaction status, allocation metadata다. raw Rust struct dump를 금지한다.

PageRef는 `(FileId, page_no, incarnation)`이며 ObjectId/Generation은 기대 owner와 header에서 대조한다. 물리 slot은 RowUID가 아니다. 파일은 영속 예약 writer epoch+counter로 고유 생성하며 재사용하지 않는다. 같은 파일 안 page/extent/slot은 generation을 전진시켜 재사용한다. stale buffer/locator/undo 참조와 wrap을 거절한다. 재사용 WAL의 init image와 allocation generation 변경은 한 완결 mini-transaction에 담는다.

파일은 engine-owned data/undo 공간이며 고정 크기 extent(초기 64 pages)를 늘려 사용한다. allocation bitmap/free-space metadata는 disk에 두고 bounded cache로 읽는다. 새 파일/길이 확보·header·namespace 영속화가 끝나기 전 그 공간을 참조하는 로그의 durable commit을 허용하지 않는다. 이 장벽은 파일 최초 생성/확장에만 필요하며 매 commit의 새 pack 생성이 아니다. 공개되지 않은 생성 파일은 별도 owned staging 목록으로 관리한다. 선택된 checksummed FILEMAP control에는 DB/FileId·파일명·고정 bootstrap page 주소·예약된 길이가 들어간다. FILEMAP은 data/allocator page에 의존하지 않는 작은 chunked manifest이며 atomic selector+dirsync로 설치한다. 새 파일은 FILEMAP 영속 선택 뒤만 WAL에서 참조한다. drop한 파일 mapping은 필요한 WAL·DWB·checkpoint가 사라질 때까지 남긴다. FILEMAP 손상은 임의 directory 탐색으로 복구하지 않고 거절한다.

page load마다 길이·checksum·identity·owner·kind·slot overlap·정렬·참조 level과 산술 상한을 검사한다. checksum은 torn write를 검출할 뿐 복원 수단이 아니다. [flush 규약](#buffer)의 doublewrite가 복원을 맡는다. F01에서 offsets·record IDs·digest 범위·독립 hex vector를 동결한다.

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

Unique는 **전체 NULL만 중복 예외**, 부분 NULL은 NULL 위치를 포함한 동일 tuple이면 충돌한다. `(1,NULL)` 두 행은 거절, `(NULL,NULL)` 두 행은 허용한다. SQL의 삼값 논리를 바꾸지 않는다. final uniqueness는 mutation마다가 아니라 **statement 종료**에 검사한다. 영향 old entries를 제외한 최종 new 집합 내부 및 다른 visible committed entries와 대조하여 key 교환을 허용한다.

같은 unique key에 대한 경쟁은 `(index generation, encoded key)` transaction lock으로 serialize한다. 영향받는 old/new unique keys를 정렬해 예약/잠금한 뒤 검사하고 commit/abort 완료까지 보유한다. 검사 대상은 다른 거래의 최신 committed 상태와 own final delta이며 오래된 statement snapshot만으로 duplicate를 허용하지 않는다. all-null 예외 key는 unique conflict lock 대상에서 제외한다.

statement 시작 시 undo-head savepoint·오류 결과·rollback 예약을 확보한다. 타입/키 크기/unique/quota/취소 실패는 해당 mark까지 undo하고 이전 성공 statement를 보존한다. rollback은 새 undo 생성 없이 기존 record와 사전 예약한 CLR/WAL/작업 공간을 사용한다. 디스크/로그 오류 때문에 rollback을 마치지 못하면 성공 ABORT를 응답하지 않고 RecoveryRequired/Fenced로 전이한다. statement 실패와 transaction 전체 abort를 구별한다.

<a id="concurrency"></a>
## 7. 가시성·동시성

ReadCommitted는 statement마다 published q를 pin하고 own writes를 읽는다. 다른 거래의 uncommitted/나중 commit은 undo를 따라 제거한다. 행 쓰기는 stable RowUID X lock을 transaction 종료까지 보유한다. lock 대기 뒤 대상 행이 statement가 관측한 버전과 다르면 자동 predicate 재평가 대신 SerializationConflict로 **전체 거래 abort**한다. 상위 SQL이 수정 predicate를 숨겨 재실행하지 않는다. 같은 q의 서로 다른 행 writer는 key/schema 충돌이 없으면 모두 commit할 수 있다.

Serializable은 transaction snapshot을 유지하고 read/write table 집합·absence/range 조회의 table generation을 등록한다. coordinator에서 각 table의 마지막 committed 변경 q가 start_q 이하여야 한다. own writes는 별도이므로 validation에서 제외한다. 성공 commit은 변경 table의 q를 전진시킨다. catalog 조회는 해당 catalog namespace/generation의 변경을 같은 방식으로 검증한다. table 단위 false conflict는 초기 보수적 선택이다. 독립 table writer는 모두 성공할 수 있고 같은 table의 다른 행은 충돌할 수 있음을 문서/시험에 명시한다. DB 전체 q 동일 검사는 제거한다. 더 세밀한 predicate validation은 후속 측정 대상이다.

transaction lock과 page latch는 분리한다. 기다리는 lock을 가진 채 page/tree latch·WAL append gate·publication lock을 보유하지 않는다. 최대 32 transaction wait-for graph를 검사하고 cycle의 가장 큰 TxnId를 victim으로 골라 전체 undo 후 lock을 해제한다. timeout/cancel도 undo 완료 뒤 결과를 확정한다. pinned snapshot을 시간만으로 강제 만료하지 않는다.

DDL은 독립 원자 거래다. offline index build는 admission close/drain 후 bounded external sort·검증·신규 object를 build하고 catalog 등록을 commit한다. build 중 crash의 미공개 object는 allocation owner 기록으로 정리한다. drop은 versioned catalog에서 retire 상태를 공개하고 old snapshot의 객체/anchor를 horizon까지 유지한다. 같은 이름의 재생성은 다른 generation이다. online index build는 후속이다.

<a id="undo"></a>
## 8. 영속 undo와 aging

UndoRef는 `(undo file/page incarnation, slot generation)`의 안정 참조다. record는 TxnId·previous transaction UndoRef·대상 RowUID/논리 index key/catalog identity·before version·previous version ref·undo kind를 담는다. 128 KiB 값은 bounded chunks로 나누고 chain/count/digest를 검사한다. transaction status에는 TxnId/generation·Active/Aborting/Committed/Aborted·commit q·undo head·rollback target/progress가 있다.

VersionStamp는 `Committed(q)` 또는 `TxnRef(id,generation)`다. 다른 거래의 committed 값을 before-image로 캡처할 때는 commit q를 해석해 저장한다. 미완료 거래의 상태 slot을 무조건 복제하지 않는다. own 연속 변경은 자신의 chain을 유지한다. terminal 거래는 durable aging queue와 undo chain에 변경 대상을 남긴다. horizon을 통과하면 현재 header에 남은 자기 TxnRef를 logged commit-q stamp로 바꾸고, 이미 다른 거래가 대체한 경우 그 before-image의 resolved stamp를 확인한다. 관련 참조 제거가 완료된 뒤만 status/undo slot을 generation 증가와 함께 재사용한다.

버전 chain의 obsolete suffix는 owner aging 작업이 **logged cut**으로 끊는다. before-image 안 previous-version ref와 Active 거래의 rollback chain도 영속 참조다. undo record마다 disk에 저장하는 incoming-reference count를 관리하며 row/index header·undo record·transaction head의 link 변경과 count 증감을 같은 WAL 그룹으로 처리한다. 0-count이고 horizon/rollback pin이 없는 record만 반환한다. chain은 단조로운 record sequence로 과거만 참조하고 cycle/overflow는 거절한다. cut 전 참조가 남은 suffix는 더 오래 보존할 수 있다. rollback은 오래된 before-image에 남은 link를 무조건 되살리지 않고 현재 cut과 참조 이전을 같은 CLR에 반영한다. record 반환은 그 outgoing reference를 batch로 해제하는 logged queue 작업이며 full-root tracing을 만들지 않는다. `A commit→B update→A aging→B abort→slot 재사용`의 chain·count·full-check를 필수로 검증한다.

undo horizon은 active transaction snapshot, 열린 ReadCommitted statement/cursor, rollback chain, retired object/overflow, backup/recovery 작업 중 가장 오래 필요한 소유를 포함한다. reader horizon만 지났다고 아직 Active인 거래의 undo를 반환하지 않는다. ReadCommitted의 statement 사이에는 reader pin을 놓을 수 있으나 자기 rollback chain은 거래 종료까지 유지한다. disk undo/page/tss 회수는 owner별 큐·segment cursor의 작은 batch로 수행하며 전체 root graph나 immutable pack을 추적하지 않는다.

WAL 보존과 undo 보존을 분리한다. checkpoint의 transaction head/progress와 필요한 undo/status/allocator page가 redo 가능해야만 오래된 WAL을 놓는다. dirty undo page의 recLSN은 data page와 동일하게 WAL을 pin한다. 조건 충족 후에는 오래된 Active 거래의 시작 WAL이 없어도 UndoRef chain으로 rollback/restart undo가 가능해야 한다. WAL의 prevLSN은 진단용 chain이며 삭제한 로그를 undo의 필수 입력으로 사용하지 않는다. RP/archive/PITR 보존은 별도 owner다.

<a id="buffer"></a>
## 9. buffer·WAL-before-page·torn-page

buffer는 Empty/Loading/Clean/Dirty/Flushing 상태와 frame generation·pin·content latch·modification counter를 가진다. 같은 page의 중복 load/예약을 조정한다. clock은 pinned/I/O frame을 건너뛰고 bounded sweep/deadline 뒤 BufferBusy를 반환한다. dirty victim은 flush 완료 전 재사용하지 않는다. file read/flush/checksum은 map/state mutex 밖에서 수행한다.

**steal/no-force**를 선택한다. uncommitted page도 undo와 redo가 보호되면 flush 가능하고, commit 때 data page 전체 flush는 요구하지 않는다. 각 page 변경은 완결 WAL mini-transaction을 append한 후 메모리에 적용하고 pageLSN을 해당 기록 끝 LSN으로 올린다. undo 생성·transaction head 갱신·data effect는 완결 redo 그룹과 상태 전이로 연결하며 중간 undo ref가 보이는 순서를 금지한다. WAL 공간/undo/rollback 예약은 latch 전에 확보하고 append gate 안에서는 I/O 대기를 하지 않는다. append 뒤 적용 불변식 실패는 fence다.

flush는 다음 순서를 지킨다.

1. latch 안에서 identity/incarnation/pageLSN/modification counter와 완전 page image를 사전 예약한 I/O buffer로 복사한다.
2. latch 밖에서 **그 pageLSN까지 WAL sync**를 확인한다. 실패하면 home page를 쓰지 않고 fence한다.
3. 고정 크기 doublewrite 슬롯에 identity·pageLSN·slot sequence·checksum과 image를 기록하고 sync한 뒤, 해당 pending을 선택한 DWB control의 atomic replace+dirsync까지 완료한다.
4. 원래 data/undo/status/allocator page 위치에 image를 write하고 해당 file sync를 완료한다.
5. 그 뒤 DWB control에서 해당 슬롯을 retired로 제거한 선택을 sync하고, 그 뒤만 doublewrite 슬롯을 재사용한다. 현재 frame의 counter/identity가 같을 때만 clean 전환/DPT 제거; 재갱신됐으면 dirty와 보수적 recLSN을 유지한다.

doublewrite는 16 MiB의 영속 고정 pool이며 디스크 quota에 포함한다. bounded DWB control manifest는 **현재 pending 슬롯**의 sequence·DB/FileId/page/incarnation·pageLSN·image hash만 선택한다. image sync 뒤 control을 atomic replace+dirsync로 영속 선택한 후 home write를 시도한다. home file sync 뒤 해당 pending을 제거한 control 선택을 영속화해야 재사용할 수 있다. 선택이 불명이면 fence하고 그 슬롯을 덮어쓰지 않는다. 동일 page의 flush/재사용은 앞선 pending retirement 완료까지 serialize한다. control 갱신도 단일 coordinator로 serialize하며 lock 안 I/O를 하지 않는다. 이전 control로의 임의 fallback이나 raw 슬롯 scan으로 후보를 부활시키지 않는다. 손상 selector/control은 거절한다. 이 보수적 control I/O 비용을 PF09에 포함하며 최적화는 별도 검토한다.

FILEMAP과 선택된 DWB control은 home allocator에 의존하지 않고 읽을 수 있다. 첫 bootstrap 단계에서 두 control의 identity/digest를 검증한 뒤 **pending image만** FILEMAP의 주소에 복원한다. pending이 있는 주소의 재할당은 retirement 전 금지되므로 slot의 incarnation을 bootstrap 근거로 사용할 수 있다. allocator/status/anchor도 동일 방식으로 먼저 복원하고, WAL allocation/init redo를 적용하면서 일반 page owner/incarnation을 검증한다. healthy home이 더 높은 LSN/incarnation이면 불변식 위반으로 거절하며 오래된 image로 덮지 않는다. 유효 pending 없는 기존 initialized home의 torn/손상은 원칙적으로 REJECT_CORRUPTION이다. 예외는 FILEMAP·allocation generation과 일치하는 **완전한 INIT image 및 그 이후 채택 끝까지의 WAL 전체**가 보존된 경우다. 새 page가 아직 zero/uninitialized home인 no-force 상태도 이 경로에서 INIT부터 재구성한다. delta redo만으로 checksum 오류를 숨기거나 WAL이 끊긴 INIT을 복원 근거로 쓰지 않는다. INIT 복원 대상은 redo에서 초기화하므로 bootstrap 단계의 blanket page-load 거절을 적용하지 않는다. @10 사본이 retired된 뒤 @20을 쓰고 슬롯을 재사용한 경우 @10 raw bytes가 남아 있어도 복구 후보가 아니다. arbitrary cold-page 손상까지 수리한다고 주장하지 않는다. control torn·retirement 불명·allocator torn·page reuse·두 번째 crash가 F04 필수 vector다.

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

`LEASE → PREFLIGHT/SELECT → DOUBLEWRITE_REPAIR → ANALYSIS → REDO → RECOVERY_LOG_OPEN → UNDO → VERIFY_RECOVERY → STABILIZE → RESUME → READY`다. 이전 성공 sync의 bytes/namespace 보존, active tail의 제한적 incomplete frame 제외, 완전 길이 checksum 오류 거절, UNKNOWN 합법 이력은 DEV2와 동일하게 검사한다. 소스 WAL은 고치지 않는다. recovery 중 page는 WAL/CLR 규약으로 변경할 수 있으므로 DEV2의 '모든 source page 불변'을 적용하지 않는다.

ANALYSIS는 선택 checkpoint의 capture LSN 상태에서 active transaction table을 seed하고 **capture LSN 직후**부터 완결 WAL 끝까지 거래 상태·최신 UndoRef·commit q를 재구성한다. checkpoint 이전 상태를 다시 덮어쓰거나 이미 끝난 거래를 ATT에 부활시키지 않는다. terminal commit을 undo하지 않으며 미완료 거래를 loser로 정한다. runtime DPT는 buffer/flush-inflight page의 recLSN을 보유하고 checkpoint에 기록한다. 재시작에는 커진 전체 DPT를 RAM에 만들지 않는다. `redo_start=min(checkpoint capture LSN, checkpoint DPT의 최소 recLSN)`부터 모든 관련 page redo를 streaming 읽고 incarnation/pageLSN으로 이미 반영한 기록을 건너뛴다. 이는 ARIES의 선택적 redo보다 보수적인 bounded-memory 시작안이다. ATT 최대치는 admission된 32개와 system 작업 예약으로 제한하며 commit/abort 완료 시 제거한다.

REDO는 commit 여부와 관계없이 이력을 반복한다. 이 단계는 source WAL만 사용하며 page flush가 필요하면 먼저 채택 prefix의 해당 LSN/namespace를 sync하고 doublewrite를 따른다. allocation/init·undo·status·CLR·tree 구조 기록도 포함한다. RECOVERY_LOG_OPEN은 채택한 이전 prefix와 namespace를 먼저 STABILIZE한 뒤 그 완결 끝에 연결한 새 recovery epoch/header를 sync하고 **WRITER 선택까지 영속화**한다. admission은 계속 닫는다. 이 선택이 끝나기 전 새 CLR append 및 그 효과의 home write를 금지한다. 다음 restart는 선택된 recovery epoch도 정상 predecessor chain에서 발견한다. 기존 recovery epoch의 incomplete tail도 같은 규약으로 판정한다. UNDO는 loser의 durable UndoRef head부터 역순으로 처리하고 선택된 recovery epoch에 새 CLR을 남긴다. 이미 완료한 undo는 CLR의 UndoNextRef로 건너뛴다. recovery 중 두 번째 장애에도 source WAL+새 CLR에서 다시 실행할 수 있어야 한다.

VERIFY_RECOVERY는 WAL/selector/allocator·거래 terminal 상태·실제로 redo/undo한 page의 구조와 효과를 검증한다. **변경 없는 전체 table/index 양방향 검사를 정상 open의 필수 작업으로 하지 않는다.** 모든 page read에서 checksum/identity/bounds를 검사하며 읽지 않은 임의 cold page 손상은 첫 접근 또는 전체 검사에서 발견할 수 있다는 범위를 공개한다. 필요한 WAL/복구 page 손상은 정상 open을 거절한다. checksum을 맞춘 논리 오염까지 online read 하나로 검출한다고 하지 않는다.

`check_integrity(full)`은 admission drain한 고정 상태에서 전체 row↔index·unique·catalog·allocation/undo 참조를 bounded 순회하는 별도 작업이다. 추가 disk spill은 요구하지 않는다. S1/fault 인수는 정상 reopen 성공 후 이 검사와 독립 이력 검산을 함께 실행한다. 따라서 검증을 생략하여 PASS시키지 않으며 정상 서비스의 startup 비용과 audit 비용을 분리한다. 같은 0/소량 WAL·같은 dirty-page 수의 256 MiB/1 GiB fixture에서 정상 reopen I/O가 전체 DB scan으로 증가하지 않는지 검사한다.

STABILIZE는 채택한 이전 WAL 끝과 새 CLR·epoch/namespace를 sync한다. 필요 page write는 doublewrite 규약을 지킨다. 이 장벽 실패 시 사용자 admission/READY/reclaim을 금지한다. 이미 durable 선택된 recovery WRITER를 되돌리지 않는다. RESUME은 그 epoch를 계속 쓰거나 tip에 새 epoch를 연결하며 다음 restart가 앞선 UNKNOWN의 채택과 새 ACK를 놓치지 않아야 한다. 반복 recovery의 transient workspace는 사전 예약된 memory/doublewrite pool만 사용하고 DB 크기만큼 scratch를 생성하지 않는다.

<a id="checkpoint"></a>
## 12. fuzzy checkpoint·WAL 회수·backup

checkpoint는 전체 data snapshot이 아니다. begin-checkpoint 기록 뒤 commit coordinator와 append/registry/DPT 경계에서 **capture LSN**과 그 위치까지의 ATT(UndoRef/progress 포함)·DPT·allocator high-water·published q를 함께 복사한다. capture는 CommitAttempted 거래의 sync/상태 설치가 끝날 때까지 기다리며 불명 commit이면 checkpoint를 진행하지 않는다. 이미 append된 update·CLR·commit·abort의 DPT/head/status 등록이 끝나야 capture할 수 있다. lock 밖에서 end-checkpoint metadata chunks를 쓰고 sync한 뒤 CURRENT가 begin/capture/end LSN·chunk digest·current/previous identity를 atomic replace+dirsync로 선택한다. dirty pages를 모두 flush할 필요는 없다. WRITER는 checkpoint가 되돌리지 않는다.

mutable datafile을 공유하므로 previous checkpoint는 **이전 데이터 이미지가 아니라 더 이른 analysis/redo 시작점**이다. fallback은 그 metadata와 필요한 WAL/undo/namespace가 모두 남고 현재 page를 redo로 설명할 수 있을 때만 허용한다. datafile 손상을 previous selector만으로 치료하지 않는다. CURRENT 손상 때 임의로 큰 파일 번호를 선택하지 않는다.

WAL 삭제 floor는 current/previous checkpoint 각각의 redo_start, live dirty recLSN, checkpoint 진행, 미완결 system 작업, backup/archive/RP owner의 최소 필요 LSN이다. Active 거래의 최초 LSN은 undo store 독립성 조건을 만족하면 floor에 넣지 않는다. free/undo/status 변경도 home flush와 checkpoint 인계 전까지 WAL을 보존한다. 단조 floor 확정과 Retiring 등록은 coordinator→retention 경계에서 수행하고 unlink/dirsync는 lock 밖이다. old/new selector 불명 때 합집합 floor를 보존하고 새 삭제를 막는다.

exact-cut backup의 초기 방식은 **offline admission drain**이다. 새 거래/maintenance admission을 닫고 기존 거래·cursor의 정상 종료와 진행 중 aging/DDL의 완료를 기다린다. 기존 commit/rollback은 drain 중 허용하며 timeout이면 gate를 복원하고 Busy를 반환한다. reader 강제 만료나 숨은 commit을 하지 않는다. drain 뒤 C를 고정하고 모든 dirty data/undo/status/allocator를 WAL-first/doublewrite로 flush+sync한다. 완결 checkpoint를 선택하고 mutation을 중지한 상태에서 allocated pages/파일 metadata를 streaming 복사한다. backup 파일과 manifest·namespace를 sync한 뒤만 성공을 알리고 admission을 복원한다. clone reflink나 immutable live datafile을 가정하지 않는다. 취소/복사 실패는 source C를 유지하고 불완전 destination은 성공 backup으로 선택하지 않는다.

restore는 빈 소유 경로에 완전 backup closure 검증→전체 무결성 검사→새 timeline·writer epoch·base q=C와 genesis WAL/checkpoint/control 선택→sync 후 개방한다. backup manifest는 source의 완결 WAL high-water LSN을 포함한다. 새 timeline의 첫 LSN은 그보다 크게 시작하여 복사된 pageLSN을 새 WAL이 잘못 건너뛰지 않게 한다. 모든 복사 pageLSN이 manifest high-water 이하인지 검증하고 이 baseLSN은 새 control에서 인증한다. source WAL 파일 자체는 필요하지 않다. source WAL 없이 복원→새 commit ACK→두 번째 restart를 검사한다. online backup/PITR/replication은 후속 계약이며 offline exact cut을 그것들의 완료로 주장하지 않는다.

<a id="reclaim"></a>
## 13. 공간 재사용·진행 보장

whole-pack GC, 모든 root 도달성 mark, 전체 DB를 새 pack으로 쓰는 필수 compaction을 제거한다. allocator는 free page/extent를 재사용하고 aging은 owner queue에서 undo·삭제 row/index·retired object를 horizon 기준으로 정리한다. index key를 먼저 정리하고 row/overflow 참조를 제거한 뒤 slot/extent를 반환한다. page 재사용은 durable allocation generation과 WAL init으로 보호한다. page 내 빈 cell 정리는 logged page compaction이며 전체 DB rewrite와 다르다.

장기 reader/Active transaction은 undo와 retired space를 보존한다. quota 부족 때 새 statement/write를 거절하고 원인을 oldest snapshot/undo owner와 함께 보고한다. reader를 몰래 만료하거나 필요한 undo를 덮어쓰지 않는다. crash 뒤 orphan build 정리는 allocation ownership·transaction status를 검증한 뒤 batch로 수행한다. 낮은 free space에서도 이미 시작한 rollback·flush·checkpoint·aging은 아래 reserve 안에서 진행해야 한다.

초기 전체 owned disk quota 8 GiB 안에 **1 GiB를 maintenance/rollback 전용**으로 예약한다. foreground가 이 reserve를 소비하지 못한다. transaction admission은 실제 undo bytes·최악 CLR encoded bound·rollback 재삽입/split에 필요한 page·WAL/checkpoint 여유를 계산하고 reserve와 중복 없이 선점한다. bound를 계산할 수 없는 operation은 시작 전에 거절한다. fixed doublewrite 16 MiB, checkpoint metadata 두 세대와 회수 진행점 공간도 이 예산에 포함한다. 작은 rollback page pool을 일반 insert에 대여하지 않는다. OS의 실제 ENOSPC/I/O 불명은 여전히 Fenced 모델이며 quota가 장치 장애를 해결한다고 주장하지 않는다.

aging cursor/진행점은 재실행 가능한 logged state다. 메모리 batch·disk 진행점은 고정 상한이며 오래된 전체 mark/sort scratch를 만들지 않는다. 거래 시작 WAL을 실제 제거한 상태의 rollback/recovery, 긴 reader 해제 뒤 재사용 증가, quota 경계의 반복 crash 및 rollback pool 고갈 거절을 필수 인수한다. online shrink/OS 반환·재배치 compaction은 S1 필수가 아니다. 내부 재사용량과 filesystem 반환량은 구별한다.

<a id="budgets"></a>
## 14. 자원·성능

| 자원 | 초기 공학적 profile |
| --- | --- |
| page buffer | 64 MiB/4,096 frames; 4/16 MiB fault profile. dirty/clean 합계 |
| metadata/cache | 합계 8 MiB; frame map/DPT·bounded lock table·catalog/locator·FD cache. locator 1 MiB, FD 최대 128 |
| transaction | 최대 32; mutation 256, 논리 payload 1 MiB, private 8 MiB/거래·전체 64 MiB. 추가 잠금/undo staging을 포함 |
| system workspace | 합계 16 MiB: flush/DWB I/O buffer 4 MiB 고정 + recovery/checkpoint/build/full-check/aging 중 한 작업 12 MiB. foreground가 고정 flush 몫을 빌리지 않음 |
| disk/WAL | owned 8 GiB·maintenance reserve 1 GiB·보존 WAL 256 MiB·doublewrite 16 MiB. 전체 quota와 하위 한도를 동시에 적용 |
| snapshot/diagnostics | user snapshot 32, backup/checkpoint 각 1; trace 4 MiB/4,096 events. 필수 receipt 유실은 실패 |

대형 index build의 run/disk 목록은 별도 사전 예약·owned staging이며 외부 정렬의 FD/fan-in·취소 정리가 bounded해야 한다. data/index/row locator/할당 inventory 각각 accessed bytes ≥ configured buffer 4배 기준을 유지한다. inventory는 이제 파일 pack 목록 대신 page/extent allocation inventory다. 필요한 큰 inventory fixture와 quota는 실행 전에 F05에서 정한다. 단지 작은 metadata cache를 넘었다는 것으로 인수를 대체하지 않는다.

PF09는 성공 TPS/충돌률, commit·WAL sync p99, background data flush/doublewrite bytes, page read/hit/pin/latch/lock wait, undo 생성/보존/aging량, 재사용 가능/실제 재사용 공간, checkpoint·정상 reopen·full-check·backup 중단시간, RSS/FD/rollback reserve peak를 기록한다. page-cache miss와 실제 장치 I/O를 구별한다. baseline/candidate는 동일 의미·durability·fixture/환경에서 비교하고 성능 개선을 미리 선언하지 않는다.

<a id="acceptance"></a>
## 15. 구현 전 동결과 인수

[계획](../plans/dev3-implementation-plan.md#acceptance)과 DS01–06·KU09/10·RC-T12–14·PF09가 소유한다. F01 byte layout/feature, F02 API·isolation·잠금/undo 상태표, F03 독립 oracle/vector, F04 실제 I/O/fault/doublewrite·반복 복구, F05 자원·환경·성능, F06 DEV2 실제 공급, F07 파일/작업 소유를 leaf 전에 동결한다. 설계만으로 제품 PASS를 기록하지 않는다.

<a id="parallel-development"></a>
## 16. 작업 경계

공통 identity/page/WAL/undo/visibility 계약→page/allocator·WAL·buffer/flush 표현→row/undo/status와 구조 tree 통합→index/catalog/statement→recovery/checkpoint/aging/backup 순으로 합친다. pack 전용 병렬 역할은 폐기한다. undo와 WAL, dirty buffer와 doublewrite는 독립으로 성공했다고 durability를 주장할 수 없다. 실제 소유와 최소 선행 leaf는 [구현 계획](../plans/dev3-implementation-plan.md#parallel-development)을 따른다.

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

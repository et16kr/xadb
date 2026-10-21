# DEV3 16개 상세 설계 — ARIES·Oracle·로컬 DBMS 비교 리뷰

2026-10-22 · **DEV3-REVIEW-009 D2 ACCEPTED / 설계 수정·독립 검토 완료 / 제품 코드·시험 실행 없음**

[16개 상세 설계](../contracts/dev3-task-designs.md) · [저장 정본의 결정](../contracts/disk-storage-design.md#task-review-009) · [추가 인수 명세](../plans/dev3-test-plan.md#design-review-009) · [소스 목록](../reference/opensource-references.md)

## 1. 결론과 검토 범위

mutable page·단일 WAL·영속 undo·steal/no-force·doublewrite라는 방향은 XADB의 디스크 관계형 DBMS 목표에 타당한 초기 선택이다. ARIES의 page-oriented REDO와 logical UNDO, Oracle의 undo/redo 분리, InnoDB의 doublewrite·purge와 비교할 수 있다. 다만 동일 알고리즘이나 성능 우월성을 뜻하지 않는다. 단일 적용 gate·commit coordinator, coarse Serializable, 매 batch control 선택, cursor 재탐색의 비용은 PF09 전까지 미측정이다.

16개 문서와 관련 소유 계약을 모두 검토하여 **8개 설계 보완 항목**을 반영했다. 아래 P1/P2는 잘못 구현될 경우의 영향과 수정 우선순위다. 아직 없는 운영 경로에서 실행 결함을 재현했다는 기록이 아니다. 명시된 F01/F02/F05 미동결이나 이미 남겨 둔 인수 의무는 새 결함으로 세지 않았다.

- 기준 XADB HEAD: `678168ebc519f2d2ff932c1cc07d84e639e24c6e`. 착수 시 `git status --short`는 빈 결과였다.
- 사용자 권한: 16개 설계를 ARIES·Oracle·`~/work` DBMS와 비교 리뷰하고, 발견 문제를 즉시 수정하며 병렬 리뷰하라는 요청.
- `/root/review_storage`: 3.01–05. `/root/review_tree`: 3.06–08/10/14. `/root/review_recovery`: 3.09/11–13/15. `/root`: 3.16·공통 정본·인수 연결·통합.
- 각 담당은 자기 상세 문서만 수정하고 공통 계약은 `/root`가 수정했다. 코드·원본 DBMS checkout·기존 실행 receipt를 변경하지 않았다.

## 2. 발견 사항과 수정

### F1 · P1 · flush 복사와 Pending 선택 사이의 주소 재사용

**반례:** generation g의 page image를 복사한 뒤 reader pin이0이고 DWB control에 아직 Pending이 없을 때 allocator가 g+1로 재사용한다. 지연된 g image가 뒤늦게 Pending/home write로 진행하면 새 incarnation을 덮을 수 있다. 기존 3.05는 claim을 reader pin과 구별했지만 3.03/14의 free 조건은 pin·영속 edge·pending 위주여서 claim의 시작/끝과 재사용 배타성이 불명확했다.

**수정:** image copy 전 물리 주소 claim을 allocator free와 원자적으로 조정한다. Pending 이전부터 전체 retirement 영속화와 frame/DPT finalize까지 주소를 보호한다. DWB 슬롯 반환과 물리 주소 claim 반환을 구별하고, 준비 취소는 지연 I/O가 더 실행되지 않는 무효과 상태에서만 허용한다. same-incarnation redirty와 latch 밖 I/O는 유지한다.

정본 [buffer](../contracts/disk-storage-design.md#buffer), 작업 [3.03](../contracts/dev3-task-03-allocation.md)·[3.05](../contracts/dev3-task-05-buffer-flush.md)·[3.06](../contracts/dev3-task-06-tree-rows.md)·[3.07](../contracts/dev3-task-07-indexes.md)·[3.14](../contracts/dev3-task-14-reclamation.md)에 연결했다. 새 **3.05-C16·R14-17**, 기존 T06-06을 보완했다. 직접 비교 근거는 MySQL `buf0flu.cc`의 flush 시작 전 IO_FIX와 `buf0lru.cc`의 I/O/pin 동시 검사다.

### F2 · P1 · UNIQUE 검사에서 이전 성공 statement의 own 상태 누락

**반례:** 같은 거래에서 S1이 A:key7을 삽입하고 성공한 뒤 S2가 B:key7을 삽입한다. 이번 statement delta와 다른 거래 committed 항목만 비교하면 S1의 미commit A를 놓친다. 반대 방향으로 committed A:key7을 S1에서 삭제/이동한 뒤 S2가 key7을 재사용할 때는 S1의 own absent를 반영해야 한다.

**수정:** 최신 committed 기반에 이전 성공 statement와 현재 statement를 포함한 **거래 전체 own 최종 presence**를 반영한다. 실제 대체·삭제된 exact full key만 제외하고 같은 full key self 재검사와 key 교환을 보존한다. 실패 시 S2만 rollback한다. PostgreSQL UNIQUE 검사에서 자기 거래를 포함한 SnapshotDirty 의미와 대조했다.

[UNIQUE 정본](../contracts/disk-storage-design.md#keys), [3.07](../contracts/dev3-task-07-indexes.md)·[3.10](../contracts/dev3-task-10-concurrency.md)에 반영하고 **3.07-T10**에 두 방향의 반례와 정상 사례를 추가했다. 전체 NULL/부분 NULL 계약은 변경하지 않았다.

### F3 · P2 · table drop의 종속 index retire 집합 누락

**반례:** table 정의만 retire하면 새 snapshot에서 index 정의는 보이지만 parent table이 없는 catalog가 될 수 있다. 집합을 수집한 뒤 commit 전 index 생성이 끼어드는 경우도 동일하다.

**수정:** 현재 parent identity에 속한 최대8 index와 table을 같은 DDL transaction으로 retire한다. 마지막 집합 재검사부터 공개까지의 보호를 F02에 요구하고, old snapshot에는 기존 집합을 함께 보존한다. 단독 index drop은 table/다른 index를 유지한다. F05에도 종속 정의·undo·retirement 비용을 합산한다. PostgreSQL의 index→table 의존 등록과 종속 삭제 수집은 비교 근거이며 PostgreSQL의 DDL 잠금 정책을 그대로 도입한 것은 아니다.

[동시성·DDL 정본](../contracts/disk-storage-design.md#concurrency), [3.08](../contracts/dev3-task-08-catalog-ddl.md)에 반영하고 **3.08-T6**으로0/1/8 index·동시 생성/삭제·WAL/commit cut를 지정했다.

### F4 · P1 · 부분 rollback 뒤 이미 보상한 suffix의 재undo

**반례:** S1 성공→S2 변경 후 mark까지 rollback→Active 복귀→S3 변경→전체 abort. Active에서 rollback next가 사라지고 다음 full RollbackStart가 head를 복사할 때, 그 head/chain이 보존용 S2 record까지 다시 실행하면 잘못된 이전 값을 복원하거나 참조를 중복 해제할 수 있다. BEGIN/옛 CLR WAL을 회수한 뒤 휘발성 완료 표시는 도움이 되지 않는다.

**수정:** StatementFinish 전에 effective undo chain과 보존/회수용 anchor의 차이가 영속 head/link·소유로 유지되도록 요구한다. 보상된 suffix를 다시 undo하지 않는 불변식을 Active 복귀·후속 변경·checkpoint·재시작에도 적용한다. 새 wire 형식은 채택하지 않으며 구체 표현은 기존 F01/F02 의무다. ARIES의 UndoNxtLSN과 InnoDB의 partial/full rollback·undo top 처리에서 비교한 핵심이다.

[undo 수명 정본](../contracts/disk-storage-design.md#undo-lifecycle), [3.09](../contracts/dev3-task-09-undo-rollback.md)에 반영하고 **3.09-C17**을 추가했다.

### F5 · P1 · runtime before 검사를 REDO에 그대로 적용하는 위험

**반례:** capture 전에 commit하여 ATT에 없는 owner의 data page는 뒤처져 있고 같은 그룹의 status/count page는 더 최신이다. runtime의 nonterminal owner·현재 before page 일치 검사를 재사용하면 정상 복구를 거절하거나 최신 page를 되감는다. page마다 owner ledger를 재채택하면 예약도 중복 차감한다.

**수정:** WAL 내부 의미 결합 검산, capture 이후 ANALYSIS의 owner/ledger 전이, 대상별 page REDO를 분리한다. 동일 incarnation에서 이미 반영된 page에는 과거 before 전체 일치를 요구하지 않고 미반영 page만 prior/page-local 조건으로 처리한다. page 간 관계는 관련 prefix 복구 뒤 검사하며 checksum/identity/INIT 연속성 검사를 생략하지 않는다. ARIES repeat-history와 PostgreSQL `XLogReadBufferForRedoExtended`의 block별 완료 판정에 대응한다.

[복구 정본](../contracts/disk-storage-design.md#recovery), [3.12](../contracts/dev3-task-12-recovery.md)의 3.12.4와 인수 표에 terminal owner·mixed page 상태·ledger 중복 반례를 추가했다.

### F6 · P1 · AbortingStatement loser의 전체 종료 조건

**반례:** S1이 성공한 거래가 S2 부분 rollback 중 crash했다. 복구가 mark 도달을 정상 StatementFinish처럼 처리하면 미commit S1을 남긴 Active 거래나 READY를 허용할 수 있다.

**수정:** 재시작의 AbortingStatement도 loser이며, 이미 보상한 suffix를 제외한 **거래 전체** undo와 durable AbortEnd까지 끝내야 한다. mark 도달은 복구 성공이 아니다. 일반 실행에서 statement 실패 후 Active로 돌아가는 의미는 유지한다.

[복구 정본](../contracts/disk-storage-design.md#recovery), [3.12](../contracts/dev3-task-12-recovery.md)의 3.12.7, **3.09-C17**에 연결했다. F4는 보상 기록을 건너뛰는 문제이고 F6는 남은 거래를 어디까지 종료하는지의 문제다.

### F7 · P1 · loser 부재를 전체 restore ledger0으로 해석

**반례:** ACK가 끝나 Active 거래는 없지만 terminal undo/status/retirement queue와 처리 credit은 남아 있는 backup. queue를 복사하면서 전체 obligation ledger를0으로 초기화하면 필수 정리 작업의 자원이 새 foreground에 재할당될 수 있다. 기존 restore의 빈 ledger 문구는 terminal queue/credit 인계 계약과 충돌했다.

**수정:** drain이 증명하는0은 **loser abort obligation**이다. 남은 maintenance ownership/credit과 실제 charged bytes는 새 namespace의 인증된 base ledger에 인계한다. 전체 empty는 모든 관련 소유의 종료를 확인한 경우에만 허용한다. source WAL 없이 이 인계가 불가능한 미동결 상태는 Unsupported다. InnoDB의 commit 이후 별도 purge queue는 거래 종료와 회수 완료가 다르다는 비교 근거이며 XADB의 credit 인계 방식 자체를 증명하지는 않는다.

[backup/restore 정본](../contracts/disk-storage-design.md#checkpoint), [3.15](../contracts/dev3-task-15-backup-integrity.md)의 R10·restore·**T9**에 반영했다.

### F8 · P2 · backup drain의 workspace·admission 교착

**반례:** backup이 준비 단계에서 공유12MiB workspace를 독점한 채 기존 거래 종료를 기다리고, 기존 거래의 rollback은 같은 workspace를 기다린다. 또는 외부 maintenance admission을 닫은 backup이 자기 checkpoint를 같은 문으로 요청해 진행하지 못한다.

**수정:** capacity 예약과 실제 workspace/turn 독점을 구별한다. drain 동안 기존 commit/rollback/terminal·예약된 하위 단계·flusher를 허용하고, backup의 flush/checkpoint에는 명시적 drain owner 권한을 준다. 기존 작업 완료를 기다리며 workspace/turn을 보유하지 않는다. 이는 XADB의 공유 workspace 계약에서 직접 도출한 반례다.

[backup 정본](../contracts/disk-storage-design.md#checkpoint), [3.15](../contracts/dev3-task-15-backup-integrity.md)의 R11·알고리즘·**T6**에 반영했다.

## 3. 16개 문서별 판정

아래의 “유지”는 이번 범위에서 수정할 새 결함을 확인하지 못했다는 뜻이며 구현·인수 PASS가 아니다.

| 작업 | 판정·후속 경계 |
| --- | --- |
| 3.01 형식·정체성 | 유지. trusted expected와 bytes 검산/권한 분리 적절; 미지원 opcode/control은 계속 거절 |
| 3.02 파일·읽기 | 유지. selected FILEMAP·handle identity·공통 fence 적절; 일반 open/성장 인계는 미동결 의무 |
| 3.03 공간 할당 | F1 수정. native/compiler·64KiB 증명·INIT/growth/reuse 인수 미완료 유지 |
| 3.04 WAL | 유지. stage/apply·durable prefix·first-byte certainty 분리 적절; 일반 dispatcher F01/F02/F05 필요 |
| 3.05 buffer·flush | F1 수정. WAL→DWB→home·whole retirement·보수 recLSN·pressure drain 유지 |
| 3.06 트리·행 | 구조 선택 유지. F1의 claim 소비와 T06-06 연결. byte split·stable anchor·논리 undo 적절 |
| 3.07 인덱스 | F2 수정, F1 회수 경계 연결. statement-final·NULL·offline build 유지 |
| 3.08 catalog·DDL | F3 수정. generation·versioned 정의·공통 commit 공개 유지 |
| 3.09 undo·rollback | F4/F6 인수 보완, F1 회수 경계 연결. terminal queue/credit·head 표현 동결은 남음 |
| 3.10 동시성 | F2 검사 범위 연결. snapshot 등록·published horizon·lock 후 논리 버전·FIFO 진행 유지 |
| 3.11 commit | durable sync→status→frontier→ACK·UNKNOWN 유지. F1의 재사용 경계 연결만 반영 |
| 3.12 recovery | F5/F6 수정. REBASE·epoch/WRITER 순서와 고정 peak 요구 유지; 실제 fault 인수 미완료 |
| 3.13 checkpoint | 유지. capture 등록 prefix·current/previous closure·합집합 보존·WAL floor 적절 |
| 3.14 회수 | F1 수정. published q horizon·영속 edge/순간 pin·queue 소유·logged release 유지 |
| 3.15 백업·무결성 | F7/F8 수정. full audit/normal open 분리·exact C·baseLSN·source-WAL-free 인수 유지 |
| 3.16 자원·성능 | 유지. 전체 자원 벡터·반복 실패 ledger·4배 산술·성능 분모·최종 S5 전수 경계 적절 |

## 4. 설계 선택의 비교 판단

| 선택 | 유지 이유 | 남은 비용·검증 |
| --- | --- | --- |
| in-place+WAL+영속 undo | commit마다 datafile 전체 sync가 필요하지 않고, logical undo가 split 뒤 현재 위치를 다룰 수 있음 | undo store/CLR/참조 수명·torn repair 복잡성. ARIES WAL chain과 동일 알고리즘 아님 |
| doublewrite | checksum 검출에 복원 경로를 결합. InnoDB와 비교 가능한 보수적 방식 | image/home 이중 쓰기와 control sync 비용. PostgreSQL의 full-page image 등 대안보다 항상 빠르다고 주장 불가 |
| 단일 version gate·coordinator | 초기 변경·참조/회수 경계를 설명하고 검증하기 쉬움 | 높은 동시성·group commit 부재·같은 table Serializable false conflict 비용. PF09 측정 뒤 세분화 |
| 안정 anchor·현재 구조 cursor 재탐색 | split 후 과거 물리 경로를 정본으로 유지하지 않아 수명 검증이 단순 | full scan의 반복 탐색 비용. PostgreSQL B-link/right-link 방식과 비용이 다름 |
| strict live snapshot 보존·새 write 거절 | 필요한 undo를 덮어쓰지 않고 기존 rollback/복구 자원 보호 | 긴 reader가 쓰기를 막을 수 있음. Oracle retention guarantee의 공간 tradeoff와 비교 가능하나 동일 정책은 아님 |
| bounded streaming REDO·full audit 분리 | DB 크기 비례 RAM DPT와 정상 open의 전체 논리 scan을 피함 | 보수 REDO 추가 I/O·audit 재순회 CPU. 실제 256MiB/1GiB 및 fault 자료 필요 |
| offline exact-C backup | 정지한 source에서 완전 closure를 만드는 초기 구현 경계가 명확 | 중단 시간·drain timeout·새 namespace 인계 미완료. online backup을 완료한 것으로 보지 않음 |

F05의 미완성은 여전히 실제 effect 허용을 막는 의무다. 8GiB 기본 quota와 현재 FILEMAP 형식만으로16/64MiB buffer의 allocator inventory4배를 충족할 수 없다는 [기존 계산](../contracts/dev3-resource-bounds.md)은 정확히 남겨 둔다. 64GiB qualification은 표현·전체 회계·실제 fixture가 동결되지 않은 준비 방향이다. 이를 새 결함이나 지원 완료로 바꾸지 않았다.

3.16의 성능 분모는 [성능 정본](../contracts/ai-performance-engineering.md)과 함께 검토했다. 도착 예정 시각부터의 지연·offered/admitted/completed·backlog/timeout, 고정 부하와 포화 비교 분리는 이미 정본에 있으므로 중복 정책을 만들지 않았다. PostgreSQL pgbench의 scheduled latency/lag 처리와도 대조했다.

## 5. 직접 확인한 외부 근거

### 로컬 소스

아래5개 checkout의 실제 HEAD와 작업 트리 상태를 이번에 확인했다. 모두 clean이며 원격은 [카탈로그](../reference/opensource-references.md)의 공식 저장소와 일치했다. 경로는 각 checkout 기준이다. 외부 코드나 Git 이력은 도입하지 않았다.

| DBMS·실제 revision | 읽은 경로·행 | 질문·적용 |
| --- | --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/backend/storage/buffer/bufmgr.c`4520–4645; `src/backend/access/transam/xlogutils.c`263–315/418–454 | WAL-before-page·I/O 독점, page별 REDO 완료 판정(F1/F5) |
| 동일 PostgreSQL | `src/backend/access/nbtree/README`1–150; `nbtinsert.c`388–610; `src/backend/access/heap/heapam_visibility.c`740–840; `src/backend/catalog/index.c`1120–1170/2170–2207; `dependency.c`270–315 | tree 대안, UNIQUE own 거래(F2), index/table 의존 수집(F3) |
| 동일 PostgreSQL | `src/bin/pgbench/pgbench.c`4760–4805 | 예정 도착부터의 latency·lag·실패/skip 분모(3.16) |
| MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613` | `storage/innobase/buf/buf0flu.cc`965–1015/1060–1125; `buf0dblwr.cc`2250–2383/2667–2720; `buf0lru.cc`1288–1318 | WAL force·flush IO_FIX·DWB 이후 home sync·I/O/pin 없는 재사용(F1) |
| 동일 MySQL | `storage/innobase/trx/trx0roll.cc`78–128/283–322/870–1010/1090–1190; `trx0purge.cc`120–170 | partial/full rollback와 undo top(F4/F6), 별도 purge queue(F7의 비교) |
| 동일 MySQL | `storage/innobase/log/log0buf.cc`169–207; `storage/innobase/include/log0constants.h`330–379 | latch 보유 중 WAL 공간 대기의 교착, writer/concurrency margin과 XADB 자원 벡터 비교 |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `storage/innobase/buf/buf0flu.cc`875–925; `storage/innobase/row/row0ins.cc`2070–2195; `row0purge.cc`180–220/460–530/715–762 | WAL/DWB, 중복 검사, snapshot을 고려한 purge |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/transaction/log_manager.c`3037–3089; `log_recovery.c`146–174/270–325; `src/storage/btree.c`32460–32558 | logical undo·redo-only CLR/next, B-tree 구조와 회수 비교 |
| SQLite `43a48f77585a88940c727f33682151418f544059` | `src/pager.c`26–100; `src/wal.c`101–143; `src/backup.c`337–380 | journal/WAL snapshot의 다른 선택, backup source 거래·BUSY 비교 |

`/home/et16/work/rocksdb`와 `duckdb`는 이번 환경에서 없었다. 카탈로그의 과거 clone 기록을 현재 열람 증거로 사용하지 않았다. 나머지 분산/분석 DBMS까지 모두 검토했다는 주장도 하지 않는다. 이번 저장 커널 질문에 직접 대응하는 위5개를 선정했다.

### 논문·Oracle 공식 문서

- [Mohan 외, ARIES, ACM TODS 17(1), 1992](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf): §4의 pageLSN/transaction/DPT, §5.2 rollback/CLR, §6 restart 및 부분 rollback 뒤 후속 전체 rollback. WAL-first·repeat-history·redo-only·보상 suffix skip을 비교했다.
- [Oracle 26 Process Architecture](https://docs.oracle.com/en/database/oracle/oracle-database/26/cncpt/process-architecture.html): DBW/LGWR와 data/redo 쓰기 역할. XADB의 WAL-before-page/no-force 비교이며 Oracle 내부 DWB 동일성을 주장하지 않는다.
- [Oracle 26 Managing the Redo Log](https://docs.oracle.com/en/database/oracle/oracle-database/26/admin/managing-the-redo-log.html): data와 undo의 redo 보호, durable redo 이후 commit 통지. XADB의 ACK 장벽을 대조했다.
- [Oracle 26 Managing Undo](https://docs.oracle.com/en/database/oracle/oracle-database/26/admin/managing-undo.html): rollback·read consistency·retention guarantee의 공간 tradeoff. XADB는 시간 만료로 live snapshot을 버리지 않는 자체 계약을 유지한다.
- [Oracle 26 Data Concurrency and Consistency](https://docs.oracle.com/en/database/oracle/oracle-database/26/cncpt/data-concurrency-and-consistency.html)와 [constraint](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/constraint.html): 읽기 일관성과 UNIQUE/NULL 비교. 전체 SQL 호환 또는 동일 격리 구현으로 해석하지 않는다.

## 6. 독립 검토와 실행 상태

| 범위 | 작성/수정 | 독립 검토 |
| --- | --- | --- |
| 3.03/05 | `/root/review_storage` | `/root/review_tree`가 claim·free·취소·redirty·C16의 최종 diff를 검토: PASS |
| 3.06/07/08/10/14 | `/root/review_tree` | `/root/review_storage`가 반례·정본·최종 diff를 검토: PASS. 이전 own absent 정상 사례 보완 뒤 재확인 |
| 3.09/12/15, 3.11 수명 연결 | `/root/review_recovery`; 3.15 drain 문구 최종 통합 `/root` | `/root/review_tree`가 최종 diff와 11/13 유지 판단을 검토: PASS. 3.15 drain 단계의 대기 obligation 보존 문구는 `/root/review_storage`가 최종 재확인: PASS |
| 정본·3.16 판단·인수 연결·이 기록 | `/root` | 관련 정본은 위 범위별 교차검토. `/root/review_storage`가 정본 전체 diff·3.16 판단·보고서·case 범위·README/catalog 연결을 최종 독립 검토: PASS |

최종 검토에서 이전 own absent를 반영하는 UNIQUE 정상 사례와, backup drain의 대기 유지보수 obligation 보존 문구를 추가 보완한 뒤 재확인했다. 잔여 필수 P1/P2 수정 요구는 없다. 독립 검토는 설계 논리의 검토이며 제품 실행 PASS가 아니다. 구체 형식/자원표의 동결·구현·실행 인수는 여전히 남아 있다.

제품 코드·테스트·fixture·runner는 변경하지 않았다. 제품 시험·compiler·Clippy·문서 자동 검사·commit·push는 수행하지 않았다. 추가한 case는 앞으로 해당 기능 구현과 함께 작성·검증할 명세다. 중간에는 변경 경로 필수 사례만, 최종 S5에서는 확정 DEV3 inventory 전수를 수행하는 사용자 지시를 유지한다. DEV2 미완료·DEV3/S1 미완료·기존 FAIL/중단 receipt는 그대로다.

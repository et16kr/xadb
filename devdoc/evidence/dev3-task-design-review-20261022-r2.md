# DEV3 16개 상세 설계 — 2차 비교 리뷰와 설치 순서 보완

2026-10-22 · **DEV3-REVIEW-010 D2 ACCEPTED / 수정 후 독립 검토 완료 / 제품 코드·시험 실행 없음**

[16개 상세 설계](../contracts/dev3-task-designs.md) · [저장 정본의 결정](../contracts/disk-storage-design.md#task-review-010) · [추가 인수 명세](../plans/dev3-test-plan.md#design-review-010) · [이전 리뷰](dev3-task-design-review-20261022.md) · [외부 소스 목록](../reference/opensource-references.md)

## 1. 결론과 범위

16개 설계와 관련 소유 계약을 다시 검토했다. 디스크 중심의 in-place B+tree·단일 WAL·영속 undo·steal/no-force·doublewrite는 이해 가능한 내부 구조와 기능 확장을 목표로 하는 XADB의 초기 선택으로 타당하다. 다만 **현재 설계가 성능까지 포함한 최선임을 입증한 것은 아니다.** 단일 적용/commit 경계와 반복 탐색·동기화의 비용은 PF09 실측으로 판단해야 한다.

이번에는 기존 REVIEW-009에서 보완한8개 항목을 보존하면서 **새 P1 1개·P2 2개**를 수정했다. P1/P2는 설계대로 구현할 때 가능한 실패의 우선순위다. 아직 없는 운영 경로에서 실제 장애를 재현했다는 의미가 아니다. F01/F02/F05의 명시된 미동결 의무는 별도 구현 선행 조건으로 유지했다.

- 검토 기준 HEAD: `def35fb1e274040eb55d78fa68948ba1610166c1`, 착수 시 작업 트리 clean.
- 권한: 사용자의 DEV3 16문서·ARIES·Oracle·로컬 DBMS 비교, 문제 즉시 수정, 완료 후 단일 commit 요청.
- 작성·수정 `/root`, 작성자 외 독립 검토 `/root/independent_review`. AGENTS와 DP01의 D2 독립 검토 규칙을 적용했다. 검토자는 문서를 수정하지 않았다.
- 검토 도중 다른 작업이 반영한 DEV4/DEV5 및 진행 중인 DEV6/DEV7 문서는 이번 검토·변경·commit 범위가 아니다. 기존 리뷰/실행 receipt와 외부 checkout은 변경하지 않았다.

## 2. 발견 사항과 수정

### R2-F1 · P1 · Commit의 지연 설치를 후속 pageLSN이 추월

**반례:** T1의 Commit@c를 sync했지만 Committed status와 terminal 인계를 아직 page P에 설치하지 않았다. 물리 WAL slot은 Idle이므로 T2가 같은71-slot status page의 다른 slot을 @c+1로 갱신하고 P를 flush한다. crash 뒤 P의 pageLSN이 c+1이라 REDO가 Commit@c를 skip하면 winner의 status/terminal 효과가 빠진다. commit coordinator는 다른 commit의 순서를 보호할 뿐 일반 update/CLR/aging의 stage를 자동으로 막지 않는다.

이 간극은 기존 [3.11](../contracts/dev3-task-11-commit.md)의 sync 뒤 설치와 [물리 WAL](../contracts/dev3-wal-io.md)의 Idle 복귀를 결합해 도출했다. 현재 `crates/engine/src/storage/wal_io.rs`의 stage Idle 검사와 sync 뒤 slot 반환은 물리 공급의 동작이며, 아직 구현되지 않은 일반 Commit 자체의 실행 버그로 보고하지 않는다.

**수정:** Commit stage와 원자적으로 전역 의미 설치 소유를 얻고, sync 뒤 모든 status/terminal·descriptor·dirty/DPT 등록이 끝날 때까지 후속 page-effect 그룹의 stage/apply를 차단한다. 물리 Idle과 의미 설치 완료를 구별한다. 이전 image의 읽기/flush는 허용하고 이후 설치는 기존 redirty 규약을 따른다. 설치 실패·불명 상태는 fence한다.

장벽을 기다리는 다른 producer는 turn·pin·공유 workspace·적용 lock을 반환한다. 이미 선택된 maintenance head도 양도·재진입하여 Commit continuation이 자기 장벽을 통과할 수 있어야 한다. I/O 동안 mutex/page latch를 보유하거나, staged Commit을 나중에 기록할 수 있는 채로 정상 abort/장벽 해제를 허용하지 않는다. 구체 취소·깨움·자원 소유는 F02/F05에서 동결한다.

[정본](../contracts/disk-storage-design.md#commit-installation-order), 3.04/05/10/11/12/13과 PF09에 연결했다. 새 **3.11-T11**은 같은 status page 추월, FIFO head 양도, 이전 image flush, 설치 오류, crash 후 winner/terminal 일치를 독립 검산한다. ARIES §5.1의 같은 page에 대한 log/update 순서, PostgreSQL의 WAL 삽입→page LSN 설정→buffer unlock, InnoDB의 mtr log/dirty 등록→latch 반환과 대조했다.

### R2-F2 · P2 · offline build의 drain과 자원·하위 admission 순환

**반례:** build가 준비 때 공유12MiB workspace를 독점하고 기존 거래의 종료를 기다리지만, 그 거래의 rollback/terminal도 같은 workspace를 요구한다. build DDL 거래를 drain 대상에 미리 넣거나 닫은 외부 admission으로 자기 catalog commit/실패 cleanup을 다시 요청해도 종료하지 못한다. 이전 리뷰는 backup에 이 경계를 명시했지만 build에는 적용 방식이 빠져 있었다.

**수정:** capacity 예약과 workspace/turn 독점을 구별하고 drain 동안 실제 공유 자원을 반환한다. 미효과 build owner는 기존 거래 drain 집합에서 제외하며 DDL 거래는 drain 뒤 시작한다. 기존 사용자 거래를 재분류하거나 숨은 commit으로 제외하지 않는다. sort/tree·catalog commit·rollback/cleanup은 같은 drain owner의 하위 권한을 소비하며, timeout/cancel은 자기 소유만 해제한다.

[정본의 DDL](../contracts/disk-storage-design.md#concurrency), [3.07](../contracts/dev3-task-07-indexes.md)·[3.08](../contracts/dev3-task-08-catalog-ddl.md)에 반영하고 **3.07-T11**을 추가했다. PostgreSQL의 build 잠금/기존 writer 대기와 Oracle의 DDL 정의 보호를 비교했지만 XADB에 online build나 Oracle의 DDL 자동 commit 정책을 도입하지 않았다. 반례의 직접 근거는 XADB의 공유 workspace·admission 계약이다.

### R2-F3 · P2 · backup checkpoint 전의 WAL 위치를 최종 cut으로 확정

**반례:** drain 직후 논리 C와 WAL 위치 L0를 최종 경계로 고정한 다음 checkpoint begin/end가 L1>L0까지 기록된다. 복사 시점 closure와 manifest가 인증하는 물리 high-water가 달라진다. checkpoint가 data page를 바꾸지 않는 경우에도 이 순서만으로 최종 WAL cut이 L0라는 주장은 성립하지 않는다. 이를 곧바로 실제 데이터 유실이 재현된 것으로 확대하지 않는다.

**수정:** C는 drain 직후 고정하지만 L은 flush/retirement·checkpoint 선택까지 끝난 뒤 확인·sync한다. manifest와 ownership/base ledger는 같은 최종 cut에 결합한다. 그 사이 추가 page 효과가 있으면 dirty와 closure도 먼저 안정화한다. 복사 중 WAL/page/ownership 변경은 금지하며 restore 첫 LSN은 최종 L보다 커야 한다.

[정본의 backup](../contracts/disk-storage-design.md#checkpoint)과 [3.15](../contracts/dev3-task-15-backup-integrity.md)의 요구·알고리즘·그림을 함께 수정했다. 새 **3.15-T12**는 C 불변/L0→L1, 오래된 L0 manifest 거절, source WAL 없는 restore→새 unflushed ACK→재open을 요구한다. C·LSN·ledger를 구별하는 XADB 계약에서 도출한 수정이며 SQLite의 고정 reader 경계와 backup 거래 제어는 비교 자료다.

## 3. 16개 문서별 판정

“유지”는 이번 범위에서 새 수정 필요 사항을 확인하지 못했다는 뜻이다. 제품 구현·인수 PASS가 아니다.

| 작업 | 판정·검토 경계 |
| --- | --- |
| 3.01 형식·정체성 | 유지. trusted expected·identity·checksum과 의미 권한 분리, 미지원 opcode 거절. F01 미동결 유지 |
| 3.02 파일·읽기 | 유지. selected FILEMAP·handle identity·fence·전체 closure 기반 읽기, 일반 open/성장 인계 선행 유지 |
| 3.03 공간 할당 | 유지. reserve/charge/실제 반환 분리, INIT-before-use·claim/pin/edge 없는 reuse. native/compiler·64KiB 잔여 의무 유지 |
| 3.04 WAL | R2-F1. 물리 Idle과 의미 stage admission 분리, stage 취소·미설치 소유·fence를 F02/F05로 연결 |
| 3.05 buffer·flush | R2-F1. pageLSN의 선행 효과 설치 전제, 이전 image flush/redirty. 기존 WAL→DWB→home/claim 유지 |
| 3.06 트리·행 | 유지. byte 기준 split·안정 anchor·논리 undo·구조 변경 후 cursor 재탐색. 반복 탐색 비용 미측정 |
| 3.07 인덱스 | R2-F2. offline build drain/하위 권한. 전체 own UNIQUE·NULL·row/index 양방향 검산 유지 |
| 3.08 catalog·DDL | R2-F2. drain 뒤 DDL와 commit/cleanup 진행. 원자 종속 index retire·generation·schema 재검사 유지 |
| 3.09 undo·rollback | 유지. partial rollback의 영속 skip·복구 loser 전체 종료·terminal credit. 새 opcode/수치 동결 필요 |
| 3.10 동시성 | R2-F1. 의미 장벽과 FIFO turn을 분리하고 head 양도/재진입. snapshot 등록·published horizon·잠금 후 버전 재검사 유지 |
| 3.11 commit | R2-F1. sync 뒤 전체 의미 설치가 완료되기 전 후속 stage 금지. status→frontier→ACK·UNKNOWN 유지 |
| 3.12 recovery | R2-F1. page별 skip의 생산 전제와 독립 known-bad 연결. ANALYSIS/REDO 분리·REBASE·loser 종료 유지 |
| 3.13 checkpoint | R2-F1. 물리 Idle만으로 capture 금지, 기다리며 continuation 자원 점유 금지. closure/floor 유지 |
| 3.14 회수 | 유지. published horizon·지속 edge·pin·flush claim·영속 queue·logged release와 실제 반환 구별 |
| 3.15 백업·무결성 | R2-F3. 최종 L/ownership cut을 checkpoint 뒤 확정. exact C·retained credit 인계·독립 audit 유지 |
| 3.16 자원·성능 | 구조 유지, R2-F1 장벽 대기/양도 비용 계측 추가. 자원 벡터·4배 계산·S5 전체 DEV3 인수 유지 |

RC 중 schema/drop 경합도 재검토했으나 기존 schema 최종 재검사·단일 coordinator의 공개 순서를 함께 따르면 보호할 수 있어 새 결함으로 세지 않았다. 그 보호의 구체 guard/rank는 기존 F02 의무다.

## 4. 선택의 타당성과 남은 비용

| 선택 | 이번 판단 | 남은 비교·검증 |
| --- | --- | --- |
| in-place+WAL+영속 undo·doublewrite | 디스크 관계형 DB의 원자성·snapshot·복구를 분리해 설명할 수 있는 초기 기준선 | undo/CLR/참조 수명과 이중 쓰기의 구현 복잡성. 다른 DBMS와 동일 알고리즘·성능을 주장하지 않음 |
| 전역 의미 설치 장벽 | 단일 WAL 기준선에서 같은 page의 설치 공백을 막는 가장 단순한 수정 | page별 pending 효과 추적은 병렬성 대안이나 복잡성 증가. sync 전 Committed 공개·I/O 중 latch 유지는 기존 계약 위반. stage부터 설치까지의 producer 대기를 PF09에 포함 |
| offline build·backup | drain 뒤 안정 집합을 처리하므로 초기 정확성 경계가 명확 | 긴 거래/reader의 중단 시간·timeout, 별도 owner 권한의 진행 증명. online 방식은 후속 |
| 엄격한 live snapshot 보존 | 필요한 undo를 덮어쓰지 않고 기존 읽기/rollback을 보호 | 긴 reader가 새 write를 막는 비용. Oracle retention guarantee의 공간 부족 tradeoff와 비교 가능하나 동일 정책은 아님 |
| bounded REDO와 full audit 분리 | DB 크기 비례 RAM 목록 없이 정상 open과 전체 검사를 구별 | 보수 REDO I/O와 target-batch graph audit 재순회 비용. 실제 DB256MiB/1GiB와 장기 부하 측정 필요 |
| 단일 gate·cursor 재탐색·batch control sync | 초기 소유/수명 경계가 단순 | 동시성·scan 비용·dirsync, coarse Serializable 충돌 비용. PF09 전 병렬 분할/최적화를 최선으로 단정하지 않음 |

현재8GiB quota로16/64MiB buffer의 allocator inventory4배를 충족할 수 없다는 [기존 계산](../contracts/dev3-resource-bounds.md)은 유지한다. 64GiB qualification은 아직 채택된 지원 profile이 아니다. 형식·실제 자원 회계·독립 fixture를 먼저 동결해야 한다.

## 5. 직접 확인한 비교 근거

### 로컬 소스

아래6개 checkout은 이번에 HEAD·공식 remote·clean 상태를 확인하고 명시한 구간을 직접 읽었다. 경로는 각 `/home/et16/work` checkout 기준이다. 외부 DBMS를 build하거나 실행하지 않았다.

| checkout·실제 revision | 읽은 경로·행 | 확인한 경계 |
| --- | --- | --- |
| `postgres` · `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/backend/access/heap/heapam.c`2040–2120/2168–2206; `src/backend/access/transam/xlogutils.c`270–294 | critical section의 WAL 삽입·pageLSN·buffer unlock 순서, block별 redo 판정 |
| 동일 PostgreSQL | `src/backend/commands/indexcmds.c`670–714/1690–1734 | 표준 build의 ShareLock과 concurrent build의 기존 writer 대기. concurrent 방식을 채택한 것은 아님 |
| `mysql-server` · `3b99be40f8a31d9396cad10accfd3ba51647d613` | `storage/innobase/mtr/mtr0mtr.cc`640–704/772–816; `storage/innobase/trx/trx0trx.cc`2227–2251; `sql/sql_table.cc`11945–11974 | 변경 page 조기 release 제한, log/dirty 등록과 latch 반환, mtr/file commit과 durability 단계, MDL 검산 |
| `mariadb` · `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `storage/innobase/buf/buf0flu.cc`875–925 | WAL-before-home·doublewrite 경계 |
| `cubrid` · `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/transaction/log_manager.c`3037–3089 | logical undo·redo-only CLR와 다음 undo 위치 |
| `sqlite` · `43a48f77585a88940c727f33682151418f544059` | `src/wal.c`101–143; `src/backup.c`337–380 | reader의 고정 mxFrame과 checkpoint, backup source 거래·BUSY |
| `tidb` · `328460a9af032ec6b53823724f87837a6d2d5857` | `pkg/domain/schema_checker.go`55–103 | schema version·관련 table 변경/만료 검사. XADB의 최종 schema 재검사와 비교 |

ClickHouse·FoundationDB는 디렉터리/HEAD/remote 상태만 확인했고 코드 분석 근거로 사용하지 않았다. RocksDB·DuckDB 디렉터리는 없었다. 카탈로그의 과거10개 clone 기록을 이번에10개 모두 읽었다는 증거로 바꾸지 않는다.

### 논문·Oracle 공식 문서

- [Mohan 외, ARIES, ACM TODS 17(1), 1992](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf): 초록과 §5.1 인쇄115쪽의 같은 page에 대한 log/update 순서·latch 경계를 직접 확인했다. pageLSN skip의 전제를 비교한 것이며 XADB의 별도 undo store가 ARIES 그대로라는 뜻은 아니다.
- [Oracle 26 Data Concurrency and Consistency](https://docs.oracle.com/en/database/oracle/oracle-database/26/cncpt/data-concurrency-and-consistency.html): statement 읽기 일관성, latch/lock 구분, DDL 동안 정의를 보호하는 DDL lock을 대조했다. XADB의 versioned DDL·독립 원자 거래 계약을 유지한다.
- [Oracle 26 Managing Undo](https://docs.oracle.com/en/database/oracle/oracle-database/26/admin/managing-undo.html): rollback/read consistency와 retention guarantee에서 공간 부족 시 새 쓰기가 실패할 수 있는 tradeoff를 확인했다. Oracle 실행 성능이나 내부 소스 구조를 확인한 것은 아니다.

## 6. 독립 검토와 실행 상태

독립 검토자 `/root/independent_review`가 16개 문서·정본과 실제 WAL/status 공급을 읽고 R2-F1의 공유 page 반례를 확인했다. build drain 및 backup cut도 별도로 대조했다. 수정 후 최종 설계 diff·보고서·case/NT/FT·PF09·색인 연결의 독립 검토는 **PASS, 잔여 필수 P1/P2 없음**이다. 물리/의미 장벽 분리, FIFO head 양도·continuation 진행, staged 취소/fence, 이전 image flush/redirty, build 거래 예외와 최종 backup cut을 재확인했다. 지적된 ARIES의 PDF 페이지 표기는 인쇄115쪽으로 정정했고 `/root`가 최종 반영을 확인했다.

제품 코드·테스트·fixture·runner를 변경하지 않았다. 제품 시험·compiler·Clippy·문서 verifier·push는 **NOT_RUN**이다. 추가 case는 향후 해당 구현과 함께 작성할 명세다. 변경 없는 코드 재시험 금지와 최종 S5의 확정 DEV3 inventory 전수라는 사용자 지시를 유지하며, 문서 자동 검사는 기존 pre-push gate에 둔다. 이번 요청의 마지막 작업은 이 문서 수정만 묶는 단일 commit이다.

DEV2·DEV3/S1 미완료, F01/F02/F05·일반 Commit/terminal·복구/restore의 남은 동결/구현/인수와 과거 FAIL·중단·NOT_RUN은 그대로다. 설계 검토 수용은 제품 실행 증거가 아니다.

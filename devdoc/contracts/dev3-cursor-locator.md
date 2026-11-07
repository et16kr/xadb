# DEV3 SMO를 건너는 stable cursor와 locator

2026-11-07 · `D3-CURSOR-LOCATOR-001` · **08-2a cursor 인수 시험 PASS(CUR-B01·B02, CUR-J01..J05)와 locator 설계(PR #62) / 08-2b locator 구현 PASS(LOC-B01·B02, LOC-J01..J03, LOC-C01, PR_082B) / 독립 D2 08-2a 2회차 ACCEPT / 08-2 완료(PR_082B 병합 시)**

[행 owner §5.5](dev3-task-06-tree-rows.md#algorithms) · [인수 T06-12·T06-13·T06-16](dev3-task-06-tree-rows.md#acceptance) · [statement/cursor F02](dev3-task-06-tree-rows.md#module-statement-cursor) · [current-leaf 소비자](dev3-tree-smo.md#current-leaf-consumers) · [빈 leaf unlink·root 축소](dev3-tree-smo.md#empty-leaf-unlink) · [자원 정본](dev3-resource-bounds.md) · [일정 08-2](../plans/dev3-schedule-baseline.md#items)

일정 08-2는 plan 3.06.3을 공급한다. 3.06.3.a는 split·delete·이동 아래에서 고정 q의 순서 결과를 내는 stable cursor와 자기 변경 뒤의 `CursorInvalidated`이고, 3.06.3.b는 한정된 hint cache인 locator와 그 재탐색·계측, 실제 locator working set과 1 MiB 상한이다. [08-1](dev3-tree-smo.md#unlink-u4)이 끝난 뒤 cursor가 이미 호출마다 key로 다시 route한다는 점은 [current-leaf 소비자](dev3-tree-smo.md#current-leaf-consumers)에 적었으나, 그 문서는 이것이 08-2의 인수를 대신하지 않는다고 했다. 이 문서가 그 인수와 locator 설계를 소유한다.

<a id="scope"></a>
## 1. 범위와 PR 나눔

| 단위 | 내용 | 제품 코드 |
| --- | --- | --- |
| 08-2a(PR #62) | cursor 불변식의 기록(§2), B literal 시험 CUR-B01·B02, 실제 C 위 시험 CUR-J01..J05(§3), T06-12 대응(§4), locator 설계(§5)와 시험 목록(§6) | 동작 변경 없음. B `LiveCursor`에 시험 전용 accessor 하나(CUR-J02) |
| 08-2b(PR_082B, [구현 기록](#implementation-2b)) | §5의 core locator 몫, C `RowLocator`, `page_located`, B `read_point_located_live`, census 항, LOC 시험 | core(`dev3_concurrency_admission` 모듈의 `transaction/concurrency.rs`와 그 계약 [동시성 admission](dev3-concurrency-admission.md)에 몫 계수와 규칙 추가)·C·B |

08-2a에 제품 변경이 없는 이유: cursor는 호출 사이에 page·slot·latch를 쥐지 않고, 호출마다 마지막 검사 key로 현재 tree를 route하며, `CheckedTable`은 revision이나 root를 갖지 않는다(§2). 그래서 split·unlink·root 축소가 호출 사이에 끼어도 다음 호출은 지금의 tree에서 같은 다음 key를 찾는다. 08-2a는 이것을 실제 SMO 위에서 확인한다. 일정 적립은 08-2 전체(cursor와 locator)가 끝나는 08-2b의 병합 때 한 번 한다(§8).

<a id="cursor"></a>
## 2. cursor 불변식(지금 코드)

| ID | 불변식 | 근거 |
| --- | --- | --- |
| C1 | cursor가 호출 사이에 갖는 상태는 range, 열 때의 statement private revision, 마지막 검사 RowId, 마지막 반환 RowId, End 기록, cursor credit과 빌린 view·table뿐이다. page·slot·path stack·sibling을 보관하지 않는다. caller의 `Plan4ResidencyCursor`도 호출 사이에 남지만 그 값은 frame 순번(`next`)뿐이다 | `row/read.rs` `CursorProgress`, `row/read/driver.rs` `RowCursor`, `plan4_pages.rs` `Plan4ResidencyCursor` |
| C2 | `next` 한 번은 read operation 하나를 연다. 그 operation의 첫 route key는 마지막 검사 key, 없으면 range 시작, 없으면 1이다. leaf에 다음 key가 없으면 C가 발급한 그 leaf의 upper bound로 다음 leaf를 route하며, upper ≤ key이면 `InvalidProgress`다 | `route_key`, `capture_next` |
| C3 | operation 안의 여러 route는 같은 tree를 본다. split·unlink는 `close_reads_for_retirement` gate로 열린 read operation과 배타이고, C는 leaf 발급 때 anchor root revision이 route 때와 같은지 다시 확인한다. 두 호출 사이에는 어떤 SMO도 끼어들 수 있다 | [current-leaf 소비자](dev3-tree-smo.md#current-leaf-consumers), `issue_page` |
| C4 | SMO(RootSplit, ChildSplit, LeafUnlink, RootCollapse)는 행의 집합과 key 순서를 바꾸지 않고 행을 다른 leaf로 옮기거나 빈 leaf를 뗀다. 모두 anchor root revision을 1 올린다 | `semantic_row/structure/candidate.rs`(split), `structure/unlink/candidate.rs`(unlink·collapse) |
| C5 | 보이는지는 statement의 고정 q로 판정한다. cleanup drain은 열린 snapshot이 볼 수 있는 tombstone·U를 지우지 않으므로(held), 다른 거래의 변경이 끼어도 cursor는 snapshot의 값을 읽는다 | [UNL-L14](dev3-tree-smo.md#unlink-u3), `visible` |
| C6 | 같은 statement의 row 변경은 private revision을 올리고, 다음 호출은 효과 전에 `CursorInvalidated`다. SMO는 row 변경이 아니어서 revision을 바꾸지 않는다 | `check_revision`, [F02](dev3-task-06-tree-rows.md#module-statement-cursor) |
| C7 | `next`는 먼저 private revision을 확인하고(F02의 "End보다 CursorInvalidated가 먼저"), 그다음 End를 기록한 cursor는 operation을 열지 않고 End를 낸다. 취소·출력 부족·오류는 반환 key를 옮기지 않고, 그 호출에서 검사한 보이지 않는 key만 검사 key로 남는다 | `next`, `finish_invisible`, `finish_returned` |

C1–C4에서 다음 호출의 결과는 "현재 tree에서 마지막 검사 key보다 큰 첫 key"이고, C4가 key 집합과 순서를 지키므로 이것은 SMO가 없었을 때와 같은 key다. 반환한 key를 다시 내지 않고(C2의 strict 증가와 `check_advance`), 아직 반환하지 않은 보이는 key를 건너뛰지 않는다.

<a id="cursor-tests"></a>
## 3. cursor 시험

B literal 시험은 `row/read/driver/smo_tests.rs`에, 실제 C 위 시험은 `row_join/retire/cursor_smo.rs`에 있다. literal 시험의 기대값·호출 순서는 모두 상수이고, 실제 시험의 기대값은 각 거래가 쓴 값과 commit q에서 정한 상태 기계, pool에서 읽은 tree 모양, 새 statement가 읽는 값이다. producer가 다시 읽은 값을 기대값으로 쓰지 않는다. 각 CUR-J에서 SMO는 cursor의 두 호출 사이에 일어난다.

| ID | 사건 | 확인 |
| --- | --- | --- |
| CUR-B01 | q10에서 10은 보이지 않고 20·30·40이 보인다. 호출 사이 tree가 한 leaf → 30에서 split → 40에서 split(30이 왼쪽 leaf로) → 한 leaf → 다시 split | 20, 30, 40을 한 번씩, 그다음 End. 30·40은 왼쪽 leaf에 다음 key가 없어 upper bound로 오른쪽 leaf에서 찾음. End 뒤 split은 operation을 열지 않음 |
| CUR-B02 | [10, end) cursor, 20에서 split. 20이 12 byte 출력에 안 들어감. 20이 왼쪽 leaf로 옮김(30에서 split). 미리 취소된 호출, 30의 capture에서 취소된 호출. 20에서 다시 split | 출력 부족은 10만 검사 key로 남기고 20을 반환하지 않음. 다음 호출이 옮긴 leaf에서 20을 반환. 취소는 key를 옮기지 않음. 30, 40, End |
| CUR-J01 | 한 leaf의 네 행(q2). Serializable R(q2)이 두 행을 읽은 뒤, 다른 거래의 자동커밋 11행이 RootSplit·ChildSplit 둘로 네 leaf [3,3,3,6]을 만들고 q3 commit(drain은 R이 hold). R의 cursor가 나머지를 읽음. q4의 split(두 번째 Status) 뒤 다시 호출 | R은 네 행만, 네 번째 행은 첫 split이 만든 leaf에서. q3·q4의 행은 보이지 않음. End 뒤 split은 cursor를 다시 열지 않음. R은 MCI01-V06대로 SerializationConflict로 끝나고, drain이 끝난 뒤 새 statement가 19행을 읽음 |
| CUR-J02 | q2에 commit된 네 leaf [3,3,3,6]. 명시 거래가 두 번째 leaf의 세 행을 DELETE하고 q3 commit(drain 없음). Serializable R(q3)이 첫 leaf의 세 행을 읽고, 16 byte 출력으로 다음 행을 요청 | 세 tombstone을 검사하고 세 번째 leaf의 첫 행이 출력보다 커서 `OutputTooSmall`. 그때 cursor의 마지막 검사 key는 두 번째 leaf의 마지막 행, 마지막 반환 key는 첫 leaf의 마지막 행이다(시험 전용 `LiveCursor::test_progress`). drain이 tombstone을 Prune하고 바로 그 leaf를 unlink(anchor 목록에 그 leaf). R의 다음 호출이 세 번째 leaf의 첫 행을 반환하고 남은 행을 한 번씩 읽고 End |
| CUR-J03 | 두 leaf(q2), 명시 거래가 왼쪽 leaf를 모두 DELETE하고 q3 commit. ReadCommitted R(q3)이 전체 cursor와 [삭제된 첫 key, 남는 leaf의 셋째 key) 범위 cursor를 열고, 둘 다 한 행(남는 leaf의 첫 행)을 읽음. 범위 cursor는 그 전에 삭제된 행을 검사 | drain이 Prune 뒤 root를 축소해 남는 leaf가 level 0 root. 두 cursor가 그 위에서 계속: 전체 cursor는 남은 행 뒤 End, 범위 cursor는 남는 leaf의 둘째 행 뒤 range 끝에서 End. q4의 8행이 다시 split한 뒤에도 전체 cursor는 End. R은 Ok로 끝남 |
| CUR-J04 | 큰 행 7개와 100 byte 행 하나(q2). Serializable R(q2)이 두 행을 읽음. 명시 거래가 넷째 행 DELETE(T는 한 leaf, level 0), 작은 행을 가장 큰 값으로 UPDATE(그 statement 직후 두 leaf, level 1: UPDATE의 값 검사가 leaf 부족을 보고 C가 split), 새 행 INSERT, q3 commit(drain은 R이 hold) | R은 나머지 여섯 행을 q2 값으로 두 leaf에 걸쳐 읽음: 지운 행과 작은 행의 옛 값은 U에서, 새 행은 보이지 않음. R은 SerializationConflict로 끝나고 새 statement가 q3 상태를 읽음 |
| CUR-J05 | 명시 거래의 statement가 int table에 7..10을 C의 apply로 넣고 자기 cursor로 7을 읽음. 호출 사이 C의 원 split이 RootSplit, 두 leaf의 ChildSplit, level 1 root의 RootSplit으로 level 2 tree [1,1,1,1]을 만듦 | split은 statement의 row 변경이 아니어서 revision이 그대로이고 cursor가 8, 9를 읽음. 같은 statement의 다음 apply(11)가 revision을 올리면 같은 cursor의 다음 호출은 `CursorInvalidated`이고 출력을 건드리지 않음. 새 cursor가 다섯 행을 네 leaf에서 읽고, commit(q2) 뒤 새 statement도 같음 |

실행(2026-11-07, debug, 정확한 이름 filter와 `--test-threads=1`). 실제 시험의 결과 행은 각 단계에서 전체 순서와 같음을 비교하므로 중복·누락이 있으면 실패한다. 실행한 source는 시험 파일 blob으로 적는다.

| 시험 | 결과 | source |
| --- | --- | --- |
| CUR-B01·B02 | PASS | `smo_tests.rs` blob `8bcb5e815fc2`(commit `ca62fda`와 같음, D2 1회차와 2회차가 다시 실행) |
| CUR-J01·J05 | PASS(D2 2회차가 `d4d3f2a`에서 169.0 s, 16.3 s) | `cursor_smo.rs` blob `03297e64ffab`. J01·J05 본문은 `ca62fda`와 같다 |
| CUR-J02·J03·J04 | PASS(161.8 s, 121.7 s, 50.7 s. D2 2회차가 `d4d3f2a`에서 다시 PASS) | 작업 tree = commit `daa0c3c`: `cursor_smo.rs` blob `03297e64ffab`, `live.rs` blob `ce806ea9f2ea` |

### PR #62 추가 검증(2026-11-07)

PR 리뷰에서 cursor의 key 재탐색과 locator의 세션·identity·revision·회계 설계를 대조했다. 제품 동작을 고칠 결함은 발견하지 않았고, 실패 후 재시도와 범위 경계의 추가 시험 넷을 공급했다. 기존 일곱 시험은 다시 실행하지 않았으며 위 원 source의 결과를 보존한다.

| 시험 | 새 검증 범위 | 결과 |
| --- | --- | --- |
| CUR-B03 | invisible 10 검사 뒤 Status FlushRequired(73). split으로 pending 20이 다른 leaf로 옮긴 뒤 재시도해 같은 행 반환·target·진행점·credit 반환 확인 | PASS |
| CUR-B04 | copy 뒤 candidate release 또는 operation finish 실패. 미공개 30이 다른 leaf로 옮긴 뒤 재시도해 누락 없이 반환 | PASS(실패 경계 둘) |
| CUR-B05 | 전체/20·30·40 split 네 모양의 4회 호출 조합 × 범위 여섯 = 1,536 schedule. 논리 행 literal을 필터한 독립 기대값으로 시작 포함·끝 제외·빈 결과·End 뒤 revision/cancel 우선순위 확인 | PASS(0.41 s) |
| CUR-J06 | 실제 C의 RootSplit 전 OutputTooSmall, ChildSplit 뒤 취소, 재시도와 전체 결과, End 뒤 자기 변경과 취소가 겹칠 때 CursorInvalidated 우선 확인 | PASS(15.68 s) |

B03–B05 source blob은 `0273f8c440a73c2353285531bcab91ec1626cbdb`, J06은 `15d3a2731b3b4513aca2fdb9bdabed43a2be03c6`이다. 실행·identity·독립 검토는 [PR 추가 검증 기록](../../module-gate/evidence/dev3-08-2-cursor/review-followup.md)에 보존한다. 이는 memory/literal 검증이며 native fault·성능·locator 구현 인수가 아니다. J06은 실제 SMO 전후 재시도를 확인하고, 실패한 행 자체의 leaf 이동은 B03·B04가 검증한다.

처음 실행에서 고친 시험 쪽 사실 세 가지를 적는다. (1) Row Begin은 Free인 Status home을 요구한다(`semantic_row/transition/prepare.rs`의 Begin 전이). q3 거래의 Status는 R이 hold한 cleanup이 끝날 때까지 Free가 아니어서, 같은 Status로 Begin한 q4 자동커밋은 `NotBegun(Begin, Prepare/WrongState, Unchanged)`로 무효과 거절됐다. 그래서 CUR-J01의 q4 거래는 두 번째 Status로 Begin한다. 여러 Status의 동시 거래 일반화는 일정 11에 남아 있다. (2) T를 읽은 read-only Serializable 거래는 다른 거래의 commit이 snapshot을 넘으면 `Rejected(SerializationConflict)`로 끝난다(MCI01-V06). cursor가 이미 반환한 행은 snapshot의 값이다. CUR-J03은 ReadCommitted로 두어 두 격리 수준을 모두 덮는다. (3) 메모리 file 상한은 시험 harness의 경계이며 CUR-J01·J03은 64 MiB를 쓴다.

<a id="cursor-t06-12"></a>
## 4. T06-12 대응

| T06-12 요소 | 공급 |
| --- | --- |
| cursor next 사이 split | CUR-B01·B02, CUR-J01(RootSplit·ChildSplit), CUR-J04(UPDATE 값 검사의 split), CUR-J05(level 2까지) |
| 삭제/aging | CUR-J02(tombstone Prune과 그 leaf의 LeafUnlink), CUR-J03(Prune과 RootCollapse), CUR-J04(DELETE·UPDATE를 old snapshot에서 U로) |
| root 변경 | CUR-J01(level 0→1), CUR-J03(1→0), CUR-J05(0→1→2) |
| fixed-q typed 결과와 전체 순서·중복·누락 대조 | 모든 CUR-J가 cursor의 결과 전체를 상태 기계 값과 비교 |
| own mutation 뒤 CursorInvalidated | CUR-J05(같은 statement의 apply 뒤), 기존 F02 시험 |
| compaction | N/A. DEV3에는 별도 compaction SMO가 없다(SMO는 C4의 네 가지). leaf 편집마다 page를 다시 encode해 leaf 안 slot 위치는 바뀌지만, cursor는 slot이 아니라 key로 seek하므로 영향이 없다. compaction이 생기면 C4의 key 집합·순서 보존과 revision 증가를 같은 방식으로 확인한다 |
| index range 소비 | 3.07(일정 09)이 소유. 이 문서의 대상은 row tree의 RowId key |

<a id="locator"></a>
## 5. locator 설계(08-2b)

[§5.5.5](dev3-task-06-tree-rows.md#algorithms)와 R9의 locator는 "RowId → 그 RowId를 route하는 leaf"의 한정된 물리 hint다. hint는 internal page의 route만 건너뛴다. leaf의 검사와 행의 판정은 cache가 없을 때와 같은 경로로 한다.

| ID | 결정 |
| --- | --- |
| L1 소유·수 | C의 `RowLocator<'c, 's>`(`wal_io/plan4_apply/row/locator.rs`). caller가 소유하고 B의 point read 호출에 `&mut`으로 빌려준다. `RowLocator::new(core)`는 그 core(저장 세션 하나의 `PageConcurrency`)의 locator 몫 하나를 받고, drop이 돌려준다. core의 몫은 `LOCATOR_SHARES` = `FOREGROUND_OWNERS` = 32개다. 33번째 생성은 아무것도 할당하지 않고 거절된다(None). locator는 core를 빌리므로 그 세션보다 오래 살 수 없다 |
| L2 회계 | 몫 하나는 32,768 B(1,048,576 / 32)다. locator 하나의 header, 표, allocator 여유 64 B가 그 안에 든다(상수 검사). 그래서 한 세션에서 동시에 살아 있는 모든 locator의 합은 1,048,576 B 이하이고, 이것이 metadata/cache 8 MiB 안의 기존 locator 배정 1 MiB다([자원 정본](dev3-resource-bounds.md), [§6 자원표](dev3-task-06-tree-rows.md#resources)). 새 예산이 아니다. ledger는 core의 몫 계수 `locator_shares` 하나다. `PageConcurrency`는 [동시성 admission](dev3-concurrency-admission.md)이 소유하므로 08-2b가 그 계약 [§14](dev3-concurrency-admission.md#locator-shares)에 이 field와 규칙(fence·lock 없이 바뀜, admission과 순서 없음, drop이 반환, 생성자는 여전히 heap 0)을 적는다. 이 1 byte는 core의 기존 padding에 들어가 core 크기(제품 640 B, 시험 cfg 824 B)를 바꾸지 않는다. 몫은 할당 전에 받고, 할당이 거절되면 돌려준다. 계수는 lock·fence·I/O 없이 원자적으로 바뀌며 어떤 admission과도 순서를 맺지 않는다. locator를 쓰는 읽기는 그와 별도로 모든 세션 검사를 그대로 거친다 |
| L3 용량 | locator마다 256 entry의 direct-mapped 표를 생성 때 한 번 할당한다(`try_reserve_exact`, 실패는 생성 거절). 표는 자라지 않는다. entry는 120 B이므로 표는 30,720 B, header를 더해 약 30,800 B이고 여유 64 B를 더해도 몫 안이다. slot은 RowId에 anchor identity의 hash를 더한 값의 하위 8 bit이므로, 한 table의 연속 RowId는 서로 다른 slot에 든다. 다른 key가 차지한 slot에 기록하면 그 entry를 덮고 eviction으로 센다. 그 결과 locator 하나는 hint 256개까지만 갖는다. 혼자 읽는 caller도 예산의 1/32만 쓰고, hot row가 256개를 넘으면 대부분 miss와 eviction이 된다. 몫은 owner마다가 아니라 생성마다이므로 한 caller가 여러 몫을 쥐면 다른 caller는 locator 없이 읽는다(§7). 1 MiB가 다 차는 것은 32개가 모두 살아 있을 때뿐이다 |
| L4 세션 원 | `CheckedTable`의 core가 locator의 core와 다르면 `page_located`는 아무것도 읽기 전에 `BindingMismatch`(Unchanged)를 돌려준다. read operation의 `check_table`이 table의 core·binding·fence를 operation의 pool·저장소와 비교하고, pool은 저장 origin 하나에 묶이므로 같은 core는 같은 저장 세션이다. 복구와 다시 open은 새 core를 만들므로 hint가 open 경계를 넘지 않는다 |
| L5 key와 entry | key는 table anchor의 전체 `PageIdentity`(database, file, page_no, incarnation, owner ObjectId, owner_generation)와 RowId다. §5.5.5의 "DB/timeline·table generation·RowId"에 대응시키면 DB는 database, table generation은 owner ObjectId·owner_generation과 anchor page의 incarnation, timeline은 L4의 세션 원이다. entry는 key, 기록 때의 anchor root revision, leaf `PageRef`, 그 leaf의 route bounds(`RowRouteBounds`)다. slot·행 version·값은 두지 않는다 |
| L6 조회 | C의 `page_located`는 `page`와 같이 Catalog LOAD와 schema 확인 뒤 anchor를 LOAD하고 root를 brief read로 읽는다. root level이 0이면 조회하지 않는다(건너뛸 internal page가 없음). level ≥ 1이면 entry를 찾는다. 같은 key의 entry는 먼저 손상 검사를 받는다: bounds가 그 RowId를 포함하지 않거나 기록 revision이 지금보다 크면 revision 분류 없이 Invariant다(기록은 항상 RowId를 포함하는 bounds만 남기므로, 옛 revision의 손상 entry도 stale로 덮여 사라지지 않는다). 그다음 revision이 지금 root revision과 같으면 hit, 같은 key이고 기록 revision이 더 작으면 stale, 그 밖에는 miss다. 기록 revision이 지금보다 큰 것을 Invariant로 두는 근거는 건강한 세션에서 revision이 줄지 않는다는 것이다(아래) |
| L7 hit | 기록한 leaf만 LOAD하고, 그 leaf를 brief read로 읽어 RowId가 있는지 본다. 이때 `route_internal`과 같이 membership을 먼저 확인하고, route와 같은 decode와 bounds 검사를 한다. 그래서 member가 아닌 hint page의 오류 종류는 RowId 확인 전에 정해진다. RowId가 있으면 그 leaf가 RowId를 route하는 leaf다(행 key는 tree에 한 번만 있다). 그다음은 `issue_page` 그대로다: anchor와 leaf의 membership, page LSN, 같은 read set에서 anchor revision 재확인, leaf의 key가 bounds 안인지, `record_table_read(table.slot)`(Serializable read set). RowId가 없거나 leaf가 비었으면 root부터 다시 route하고(recheck), 그 route의 leaf·bounds가 hint와 같아야 한다. 다르면 Invariant이고, 같으면 그 route로 발급한다(행이 정말 없거나, 그사이 들어온 행을 B의 seek가 찾는다). 그래서 hint가 다른 빈 leaf를 가리켜도 Missing으로 숨지 않는다. recheck의 비용은 hint leaf에 RowId가 없을 때만 든다. hit의 근거는 C4다: 한 세션 원 안에서 revision이 같으면 SMO가 없었으므로 root에서 key를 route한 결과가 기록한 leaf·bounds와 같다. 행 전이(INSERT·DELETE·UPDATE·Prune)는 구조를 바꾸지 않고 leaf 내용만 바꾸며, leaf는 매번 다시 읽는다 |
| L8 stale·miss | 지금처럼 root에서 route하고, `issue_page`가 leaf를 발급하면 그 route를 기록한다(level ≥ 1일 때만). 기록 시점은 leaf 발급의 성공이다. 그 뒤 Status·Undo 단계가 FlushRequired나 오류로 끝나도 기록은 남는다. route는 구조에 대한 사실이어서 그 뒤 단계와 무관하다. 발급 전에 실패한 호출은 기록하지 않는다 |
| L9 오류 | locator는 오류를 miss로 바꾸지 않는다. hit 경로의 LOAD·membership·decode·bounds 실패는 cache가 없을 때 같은 page에서 나는 오류와 같은 종류로 그대로 돌려준다. 같은 key의 entry인데 bounds가 그 RowId를 포함하지 않거나, 기록 revision이 지금보다 크거나, recheck의 route가 hint와 다르면 locator 손상이므로 Invariant(fatal)다 |
| L10 계측 | lookups, hits, stale, misses, evictions, rechecks 수와 `working_set()`(차 있는 entry 수), `bytes()`(header·표·allocator 여유)를 caller에 보인다. 정답에는 쓰지 않는다. revision은 table 전체의 값이므로 어떤 SMO든 그 table의 모든 entry를 stale로 만든다. 예를 들어 append가 만드는 오른쪽 split 하나가 먼 leaf의 hint까지 무효로 한다. 동시 append 아래의 hit·stale 비율, caller당 hit율과 살아 있는 몫 수는 T06-16(18-1)·PF09(18-2)에서 함께 잰다 |
| L11 소비자 | B의 새 진입 `read_point_located_live`와 그 adapter `LocatedSource`(page만 `page_located`, 나머지는 `LiveSource`로 전달). cursor(`next_live`, `LiveSource`, `RowCursor`)의 type과 frame은 바꾸지 않는다. cursor는 C2대로 key로 route하므로 정답에 hint가 필요 없다. 성능에서는 호출마다, 그리고 보이지 않는 행마다 root부터 route한다(§7) |
| L12 census | point 경로에 `page_located`·hit LOAD의 C 항과 B adapter·인자 항을 더한 `located.*` phase를 S5 RowRead census에 넣고, 모두 Full 이하임을 확인한다. main `597ccca`(제품 code는 `ba97cd5`와 같음)에서 point.row_load는 34,382 B이고 Full은 34,588 B여서 여유가 206 B다(S5R-C01 memory와 S5R-C02 instrumented native가 같은 값). 그 안에 locator 참조, (route, 경로 표시) carrier, `page_located`·`load_leaf_located` frame이 LOAD와 함께 산다. 조회·기록·recheck의 brief read는 LOAD 옆이 아니라 별도 never-inline 호출이며 각각 phase로 센다. cursor phase와 공유 bytes는 그대로다. 넘으면 Full을 올리지 않고 frame을 다시 나눈다 |

세션 원에 묶는 이유: anchor revision은 영속 값이지만, crash로 잃은 구조 변경의 revision 값을 복구 뒤 다른 구조 변경이 다시 쓸 수 있다. hint가 open 경계를 넘지 않으면 L7의 "같은 revision이면 같은 구조"가 한 세션 안에서만 쓰인다. 한 세션 안에서 완료된 SMO는 사용자 abort로 되돌리지 않고(R3), stage 뒤 불명 오류·부분 설치는 fence한다([SMO-E05](dev3-tree-smo.md#structural-effect)). 그래서 건강한 세션에서 revision은 줄지 않고, 줄어든 revision은 L6대로 Invariant다.

§5.5.5와 R9의 요구를 위 결정에 대응시킨다.

| 요구 | 대응 |
| --- | --- |
| cache key는 DB/timeline·table generation·RowId | L5, timeline은 L4 |
| hit에서 선택 DB/open 문맥 검산 | L4(locator의 core가 table의 core), `issue_page`의 membership |
| hit에서 page incarnation·owner 검산 | hint leaf의 요청 identity는 anchor의 database·owner·owner_generation과 hint의 `PageRef`(incarnation 포함)다. `issue_page`의 membership과 page decode가 이것을 확인한다 |
| hit에서 slot RowId 검산 | locator는 slot을 기록하지 않는다. L7의 RowId 확인, B의 key seek, C capture의 slot RowId 확인 |
| version/undo 판정 수행 | L7: leaf 발급 뒤의 모든 판정은 cache가 없을 때와 같은 경로 |
| 이동·slot 불일치면 hint를 버리고 tree로 다시 찾음 | 다른 leaf로의 이동은 SMO뿐이고 revision을 올린다(C4). 그래서 revision 불일치를 stale로 보고 다시 route한다(L6·L8). 같은 revision에서 leaf 안 slot 위치만 바뀌는 것(Prune·INSERT·다시 encode)은 key seek에 영향이 없다. hint leaf에 RowId가 없으면 recheck한다(L7). 즉 "이동" 검출을 revision 일치와 RowId 존재로 대신한다 |
| page checksum·owner 손상을 miss로 숨기지 않음 | L9 |
| 한정된 hint | L1–L3 |

<a id="locator-tests"></a>
## 6. locator 시험(08-2b)

| ID | 사건 | 확인 |
| --- | --- | --- |
| LOC-B01 | 표 단위 시험: 기록·조회의 hit/stale/miss, 다른 key의 slot 충돌, 다른 anchor identity(같은 page의 다른 incarnation, 같은 `PageRef`의 다른 owner_generation, 다른 database), 연속 RowId의 slot, revision 역행, bounds 밖 entry(같은 revision과 옛 revision), 다른 core | 분류와 계수, eviction, 연속 RowId 256개가 서로 다른 slot, 역행과 bounds 밖 entry는 revision과 무관하게 손상(Invariant), 다른 core의 table은 쓰지 않음 |
| LOC-B02 | 회계 | locator 하나의 `bytes()` ≤ 32,768, 256 entry. 한 core에서 32개가 동시에 살고 33번째 생성은 거절, drop한 몫은 다시 생성 가능, 32개의 `bytes()` 합 ≤ 1,048,576. 다른 core의 몫은 따로 셈 |
| LOC-J01 | level 1 tree(네 leaf)의 모든 행을 located point read 두 번. 다른 거래의 INSERT가 ChildSplit을 만든 뒤 옛 행과 새 행을 다시 읽음. q3 statement가 열린 채 UPDATE가 q4 commit | 첫 번째는 miss와 기록(pool이 보이는 leaf), 두 번째는 hit. split 뒤 옛 행은 모두 stale(옮긴 행은 새 leaf로 갱신), 새 행은 miss, 그다음 모두 hit. q3 statement는 hit로 옛 값, q4 statement는 새 값(판정은 cache 밖). 모든 결과가 locator 없는 point read와 같음 |
| LOC-J02 | 네 leaf의 모든 행을 기록. 한 leaf의 행 하나를 DELETE하고 drain이 Prune(그 leaf는 비지 않아 unlink 없음, revision 그대로). 다음에 두 번째 leaf의 행을 모두 DELETE하고 drain의 Prune·LeafUnlink. 끝으로 hint 손상 | Prune 뒤 같은 leaf의 다른 행은 hit(leaf 안 slot은 바뀜), 지운 행은 hit 뒤 recheck로 Missing. unlink 뒤 모든 행이 stale, 지운 행은 Missing이고 그 RowId를 route하는 leaf로 갱신. 손상: hint leaf를 bounds 밖 key를 가진 다른 leaf로 바꾸면 C의 bounds 검사가 fatal Tree 오류를 돌려줌(miss·행이 아님, 출력 없음, 세션 fence) |
| LOC-J03 | 두 leaf의 모든 행을 기록하고 hit. 왼쪽 leaf DELETE와 drain의 RootCollapse. 새 8행이 다시 split. 끝으로 hint 손상 | 축소 뒤 level 0에서는 조회하지 않음. 다시 split 뒤 옛 hint는 stale(revision이 두 번 이상 움직임), 새 행은 miss. 손상: hint의 leaf를 RowId가 없는 다른 leaf로, bounds를 무한으로 바꾸면 bounds 검사는 통과하지만 recheck가 다른 route를 보고 fatal Invariant(Missing으로 숨지 않음, 세션 fence) |
| LOC-C01 | census | `located.*` phase ≤ Full, point·cursor phase 불변 |

T06-13 대응: hit·이동·stale은 LOC-J01·J02·J03, eviction은 LOC-B01, cache on/off 결과 동일은 LOC-J01..J03(모든 located read를 locator 없는 읽기와 비교), 손상을 miss로 숨기는 mutant 거절은 LOC-J02·J03의 두 손상이다(오류를 miss로 바꾸거나 recheck를 생략하는 mutant는 그 시험에서 행이나 Missing을 내어 실패). slot reuse는 LOC-J02의 Prune 사례가 덮는다: 같은 revision에서 대상 행의 slot 위치가 바뀌어도 hit가 맞는 행을 읽는다. locator는 slot을 기록하지 않고, B가 leaf에서 key로 다시 seek하며 C의 capture가 slot의 RowId를 확인한다. RowId는 재사용하지 않는다(R4). stale incarnation: leaf page 재사용은 3.14 전에는 없다. 재사용이 생겨도 그 page는 먼저 unlink되어 revision을 올리므로 옛 hint는 stale이고, hint leaf의 incarnation은 L7의 membership이 확인한다. table 재생성은 DROP이 아직 없어 실제 시험은 N/A이고, key가 anchor의 incarnation·owner·owner_generation·database를 모두 포함하므로 LOC-B01이 다른 identity의 miss를 확인한다. T06-16(locator 접근 data가 buffer의 4배인 고정 fixture, 실제 working set·RSS)은 [A7](../plans/dev3-schedule-baseline.md)대로 대형 실행 18-1에 한 번 배정하고, 08-2b는 작은 검증(LOC-B02, LOC-J01의 working set)만 한다.

<a id="limits"></a>
## 7. 제한

- cursor batch([비교 분석 후속](dev3-task-06-tree-rows.md#cursor-batch-bound))는 측정 뒤의 별도 후보다. 지금 cursor는 호출당 한 행을 반환한다.
- F02의 미공급 의무는 열려 있다: 일반 여러 leaf/Undo chain의 작업 budget·batch API([F02](dev3-task-06-tree-rows.md#module-statement-cursor)). 08-1 이후 cursor는 실제로 여러 leaf를 읽는다. 한 호출은 보이지 않는 행을 개수 제한 없이 검사할 수 있고, 행마다 root부터 다시 route하며 매번 Catalog LOAD, schema 복사, anchor·경로 LOAD를 한다. 그동안 read admission을 쥐므로 SMO와 terminal gate가 기다린다. 이 의무는 08-2의 범위가 아니며 아직 일정 항목이 없다(미배정 의무). 소유자는 F02의 statement/cursor owner이고, 측정과 함께 정할 batch 후보(위)와 묶는다. 배정은 18-2의 PF09 측정 뒤에 정하고, 19의 최종 인수가 이 의무의 닫힘을 확인한다.
- 같은 statement의 호출을 여러 thread에서 동시에 하지 않는다(F02). locator는 `&mut`으로 한 호출만 쓰고, 조회·기록은 그 locator의 RAM만 만지며 lock·I/O가 없다. 여러 owner는 각자의 locator를 쓰고 그 합은 L2가 묶는다.
- 제품 SQL 실행기가 locator를 소유하는 연결(어느 세션 값이 언제 만들고 버리는지)은 DEV4 hookup 때 한다. 08-2b의 소비자는 B 진입과 시험이다. 몫이 없으면 caller는 locator 없이 같은 point read를 한다.

<a id="schedule"></a>
## 8. 일정

08-2(1일)는 08의 마지막 항목이 아니므로 [§3의 항목 조건](../plans/dev3-schedule-baseline.md)으로 적립한다. cursor(08-2a)와 locator(08-2b)가 모두 끝나야 하므로 08-2b의 병합 때 1일을 적립한다(35 → 36/86 = 41.86%). 08-2a의 병합은 적립하지 않는다.

<a id="review"></a>
## 9. 검토

독립 D2 기록은 `module-gate/evidence/dev3-08-2-cursor/`에 있다. 1회차(`review-r1.md`, commit `ca62fda`)는 CONDITIONAL이었다(필수 2: locator 인스턴스 수와 1 MiB 회계, key 정의). 이 판은 필수 둘을 L1–L5와 LOC-B01·B02로, 권고 열 가지와 NIT 일곱 가지를 C1·C7, §3, §4, L6–L12, §5의 대응표, §6, §7로 반영했다. 2회차(`review.md`, commit `d4d3f2a`)는 ACCEPT였다(필수 0, 권고 4, NIT 4). 문서만 고치는 권고 2·3과 NIT 1–4는 이 판에 반영했고, 권고 1(동시성 admission 계약의 갱신)은 08-2b가, 권고 4(module-gate 기록)는 PR #62의 record가 했다.

<a id="implementation-2b"></a>
## 10. 08-2b 구현 기록

§5의 L1–L12를 그대로 구현했다. 바꾼 곳은 다음과 같다.

| 위치 | 공급 |
| --- | --- |
| `transaction/concurrency.rs` | `LOCATOR_SHARES`(= `FOREGROUND_OWNERS` = 32), core의 `locator_shares` 1 byte, `take_locator_share`·`return_locator_share`. 계수는 `fetch_update`로 32를 넘지 않고, 0 아래로 내려가지 않는다. core 크기는 제품 640 B, 시험 cfg 824 B로 그대로다(크기 probe로 바꾸기 전과 뒤를 쟀다). 규칙은 [동시성 admission §14](dev3-concurrency-admission.md#locator-shares) |
| `wal_io/plan4_apply/row/locator.rs` | `RowLocator::new(core)`·`Drop`(몫 반환), 256 entry 표, `serves`(core identity), `lookup`(역행·bounds 밖은 `DamagedHint`), `confirm`(recheck), `record`, 계수 6개, `working_set`·`bytes`. slot은 anchor identity 전체의 hash에 RowId를 더한 값의 하위 8 bit |
| `tree_page/search.rs` | `row_leaf_holds`: `route_row_step`과 같은 decode·bounds 검사 뒤 정확한 검색. `leaf_holds_source_bytes`(census) |
| `wal_io/plan4_apply/row/read.rs` | `page_located`(다른 core면 읽기 전 `BindingMismatch`), `load_leaf_located`(`Located::{Hinted, Walked, Rechecked}`), `leaf_holds`(hint leaf의 brief read와 membership), `damaged_hint`(fatal Invariant). 기록은 `Walked`일 때만 |
| `row/read/driver/live.rs` | `LocatedSource`, `read_point_located_live`(B 진입). cursor 경로는 그대로 |
| `plan4_apply.rs`, `wal_io/disk_api.rs`, `storage/disk.rs` | `RowLocator`, `LocatorCounts` 재수출 |

크기(LOC-B02): locator 하나의 `bytes()`는 30,856 B다(header 72 B, entry 120 B × 256 = 30,720 B, allocator 여유 64 B). 한 core의 32개 합은 987,392 B로 1,048,576 B 이하다. 33번째 생성은 None이고 계수는 32로 남는다.

census(S5R-C01 memory, S5R-C02 instrumented native, 같은 값, Full 34,588 B):

| phase | bytes |
| --- | ---: |
| point.row_load(그대로) | 34,382 |
| cursor.row_load(그대로) | 34,574 |
| located.catalog_load | 34,470 |
| located.row_load | 34,551 |
| located.row_route | 8,284 |
| located.value_before_load | 34,334 |
| located.lookup | 3,779 |
| located.record | 3,330 |
| located.holds | 7,726 |

located.row_load의 여유는 37 B다. recheck의 다시 route는 `located.row_route`와 같은 frame을 쓰고, hint leaf의 brief read는 LOAD가 끝난 뒤 별도 never-inline 호출이라 `located.holds`로 센다.

실행(2026-11-07, debug, 정확한 이름 filter와 `--test-threads=1`):

| 시험 | 결과 | 확인한 값 |
| --- | --- | --- |
| LOC-B01 | PASS | 분류·계수, 다른 incarnation·owner_generation·database의 miss, 연속 RowId 256개의 slot, 역행과 bounds 밖의 손상(기록 revision과 그보다 뒤 revision 모두, hit·stale로 세지 않음), recheck의 confirm, 다른 core |
| LOC-B02 | PASS | 위 크기, 32개와 33번째 거절, drop 뒤 다시 생성, 다른 core의 따로 센 몫 |
| LOC-J01 | PASS(206.2 s) | lookups 70, hits 36, stale 15, misses 19, rechecks 0, working set 19. split이 옮긴 행 3개 |
| LOC-J02 | PASS(176.3 s) | Prune 뒤 hit 6(그중 recheck 1), unlink 뒤 stale 15와 hit 15(recheck 4), 손상은 `Source/Tree(Corrupt)/CallerMustResolve`와 fence |
| LOC-J03 | PASS(122.9 s) | 축소 뒤 조회 0, 다시 split 뒤 stale 8, 손상은 recheck의 `Census/Invariant/CallerMustResolve`와 fence |
| LOC-C01 | PASS | S5R-C01·C02 위 표, S5R-C03(원 Full admission 경계) PASS |

실행 뒤 `concurrency.rs`와 `locator_smo.rs`에 rustfmt만 적용했다(동작 무관). 08-2b의 독립 D2 기록은 `module-gate/evidence/dev3-08-2-locator/`에 둔다.

일정: 08-2a의 cursor 인수와 08-2b의 locator가 모두 끝났으므로 PR_082B의 병합으로 08-2의 1일을 적립한다(35 → 36/86 = 41.86%, [진도표](../plans/dev3-schedule-baseline.md#items)).

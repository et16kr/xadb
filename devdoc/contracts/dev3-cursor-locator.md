# DEV3 SMO를 건너는 stable cursor와 locator

2026-11-07 · `D3-CURSOR-LOCATOR-001` · **08-2a cursor 인수 시험 PASS(CUR-B01·B02, CUR-J01..J05) / 08-2b locator 설계 / 독립 D2 대기**

[행 owner §5.5](dev3-task-06-tree-rows.md#algorithms) · [인수 T06-12·T06-13·T06-16](dev3-task-06-tree-rows.md#acceptance) · [statement/cursor F02](dev3-task-06-tree-rows.md#module-statement-cursor) · [current-leaf 소비자](dev3-tree-smo.md#current-leaf-consumers) · [빈 leaf unlink·root 축소](dev3-tree-smo.md#empty-leaf-unlink) · [자원 정본](dev3-resource-bounds.md) · [일정 08-2](../plans/dev3-schedule-baseline.md#items)

일정 08-2는 plan 3.06.3을 공급한다. 3.06.3.a는 split·delete·이동 아래에서 고정 q의 순서 결과를 내는 stable cursor와 자기 변경 뒤의 `CursorInvalidated`이고, 3.06.3.b는 한정된 hint cache인 locator와 그 재탐색·계측, 실제 locator working set과 1 MiB 상한이다. [08-1](dev3-tree-smo.md#unlink-u4)이 끝난 뒤 cursor가 이미 호출마다 key로 다시 route한다는 점은 [current-leaf 소비자](dev3-tree-smo.md#current-leaf-consumers)에 적었으나, 그 문서는 이것이 08-2의 인수를 대신하지 않는다고 했다. 이 문서가 그 인수와 locator 설계를 소유한다.

<a id="scope"></a>
## 1. 범위와 PR 나눔

| 단위 | 내용 | 제품 코드 |
| --- | --- | --- |
| 08-2a(이 PR) | cursor 불변식의 기록(§2), B literal 시험 CUR-B01·B02, 실제 C 위 시험 CUR-J01..J05(§3), T06-12 대응(§4), locator 설계(§5)와 시험 목록(§6) | 바꾸지 않음 |
| 08-2b(다음 PR) | §5의 C `RowLocator`, `page_located`, B `read_point_located_live`, census 항, LOC 시험 | C·B |

08-2a에 제품 변경이 없는 이유: cursor는 호출 사이에 page·slot·latch를 쥐지 않고, 호출마다 마지막 검사 key로 현재 tree를 route하며, `CheckedTable`은 revision이나 root를 갖지 않는다(§2). 그래서 split·unlink·root 축소가 호출 사이에 끼어도 다음 호출은 지금의 tree에서 같은 다음 key를 찾는다. 08-2a는 이것을 실제 SMO 위에서 확인한다. 일정 적립은 08-2 전체(cursor와 locator)가 끝나는 08-2b의 병합 때 한 번 한다(§8).

<a id="cursor"></a>
## 2. cursor 불변식(지금 코드)

| ID | 불변식 | 근거 |
| --- | --- | --- |
| C1 | cursor 상태는 range, 열 때의 statement private revision, 마지막 검사 RowId, 마지막 반환 RowId, End 기록, cursor credit뿐이다. page·slot·path stack·sibling을 보관하지 않는다 | `row/read.rs` `CursorProgress`, `row/read/driver.rs` `RowCursor` |
| C2 | `next` 한 번은 read operation 하나를 연다. 그 operation의 첫 route key는 마지막 검사 key, 없으면 range 시작, 없으면 1이다. leaf에 다음 key가 없으면 C가 발급한 그 leaf의 upper bound로 다음 leaf를 route하며, upper ≤ key이면 `InvalidProgress`다 | `route_key`, `capture_next` |
| C3 | operation 안의 여러 route는 같은 tree를 본다. split·unlink는 `close_reads_for_retirement` gate로 열린 read operation과 배타이고, C는 leaf 발급 때 anchor root revision이 route 때와 같은지 다시 확인한다. 두 호출 사이에는 어떤 SMO도 끼어들 수 있다 | [current-leaf 소비자](dev3-tree-smo.md#current-leaf-consumers), `issue_page` |
| C4 | SMO(RootSplit, ChildSplit, LeafUnlink, RootCollapse)는 행의 집합과 key 순서를 바꾸지 않고 행을 다른 leaf로 옮기거나 빈 leaf를 뗀다. 모두 anchor root revision을 1 올린다 | `semantic_row/structure/candidate.rs`(split), `structure/unlink/candidate.rs`(unlink·collapse) |
| C5 | 보이는지는 statement의 고정 q로 판정한다. cleanup drain은 열린 snapshot이 볼 수 있는 tombstone·U를 지우지 않으므로(held), 다른 거래의 변경이 끼어도 cursor는 snapshot의 값을 읽는다 | [UNL-L14](dev3-tree-smo.md#unlink-u3), `visible` |
| C6 | 같은 statement의 row 변경은 private revision을 올리고, 다음 호출은 효과 전에 `CursorInvalidated`다. SMO는 row 변경이 아니어서 revision을 바꾸지 않는다 | `check_revision`, [F02](dev3-task-06-tree-rows.md#module-statement-cursor) |
| C7 | End를 기록한 cursor는 operation을 열지 않고 End를 낸다. 취소·출력 부족·오류는 반환 key를 옮기지 않고, 그 호출에서 검사한 보이지 않는 key만 검사 key로 남는다 | `next`, `finish_invisible`, `finish_returned` |

C1–C4에서 다음 호출의 결과는 "현재 tree에서 마지막 검사 key보다 큰 첫 key"이고, C4가 key 집합과 순서를 지키므로 이것은 SMO가 없었을 때와 같은 key다. 반환한 key를 다시 내지 않고(C2의 strict 증가와 `check_advance`), 아직 반환하지 않은 보이는 key를 건너뛰지 않는다.

<a id="cursor-tests"></a>
## 3. cursor 시험

B literal 시험은 `row/read/driver/smo_tests.rs`에, 실제 C 위 시험은 `row_join/retire/cursor_smo.rs`에 있다. literal 시험의 기대값·호출 순서는 모두 상수이고, 실제 시험의 기대값은 각 거래가 쓴 값과 commit q에서 정한 상태 기계, pool에서 읽은 tree 모양, 새 statement가 읽는 값이다. producer가 다시 읽은 값을 기대값으로 쓰지 않는다.

| ID | 사건 | 확인 |
| --- | --- | --- |
| CUR-B01 | q10에서 10은 보이지 않고 20·30·40이 보인다. 호출 사이 tree가 한 leaf → 30에서 split → 40에서 split(30이 왼쪽 leaf로) → 한 leaf → 다시 split | 20, 30, 40을 한 번씩, 그다음 End. 30·40은 왼쪽 leaf에 다음 key가 없어 upper bound로 오른쪽 leaf에서 찾음. End 뒤 split은 operation을 열지 않음 |
| CUR-B02 | [10, end) cursor, 20에서 split. 20이 12 byte 출력에 안 들어감. 20이 왼쪽 leaf로 옮김(30에서 split). 미리 취소된 호출, 30의 capture에서 취소된 호출. 20에서 다시 split | 출력 부족은 10만 검사 key로 남기고 20을 반환하지 않음. 다음 호출이 옮긴 leaf에서 20을 반환. 취소는 key를 옮기지 않음. 30, 40, End |
| CUR-J01 | 한 leaf의 네 행(q2). Serializable R(q2)이 두 행을 읽은 뒤, 다른 거래의 자동커밋 11행이 RootSplit·ChildSplit 둘로 네 leaf [3,3,3,6]을 만들고 q3 commit(drain은 R이 hold). R의 cursor가 나머지를 읽음. q4의 split(두 번째 Status) 뒤 다시 호출 | R은 네 행만, 네 번째 행은 첫 split이 만든 leaf에서. q3·q4의 행은 보이지 않음. End 뒤 split은 cursor를 다시 열지 않음. R은 MCI01-V06대로 SerializationConflict로 끝나고, drain이 끝난 뒤 새 statement가 19행을 읽음 |
| CUR-J02 | q2에 commit된 네 leaf [3,3,3,6]. 명시 거래가 두 번째 leaf의 세 행을 DELETE하고 q3 commit(drain 없음). Serializable R(q3)이 첫 leaf의 세 행을 읽고, 16 byte 출력으로 다음 행을 요청 | 세 tombstone을 검사하고 세 번째 leaf의 첫 행이 출력보다 커서 `OutputTooSmall`. drain이 tombstone을 Prune하고 R의 마지막 검사 key를 가진 leaf를 unlink(anchor 목록에 그 leaf). R의 다음 호출이 바로 그 행을 반환하고 남은 행을 한 번씩 읽고 End |
| CUR-J03 | 두 leaf(q2), 명시 거래가 왼쪽 leaf를 모두 DELETE하고 q3 commit. ReadCommitted R(q3)이 전체 cursor와 [삭제된 첫 key, 남는 leaf의 셋째 key) cursor를 열고 전체 cursor가 한 행을 읽음 | drain이 Prune 뒤 root를 축소해 남는 leaf가 level 0 root. 두 cursor가 그 위에서 계속: 전체 cursor는 남은 행 뒤 End, 범위 cursor는 두 행 뒤 End. q4의 8행이 다시 split한 뒤에도 전체 cursor는 End. R은 Ok로 끝남 |
| CUR-J04 | 큰 행 7개와 100 byte 행 하나(q2). Serializable R(q2)이 두 행을 읽음. 명시 거래가 넷째 행 DELETE, 작은 행을 가장 큰 값으로 UPDATE(값 검사가 leaf 부족을 보고 C가 split), 새 행 INSERT, q3 commit(drain은 R이 hold) | R은 나머지 여섯 행을 q2 값으로 두 leaf에 걸쳐 읽음: 지운 행과 작은 행의 옛 값은 U에서, 새 행은 보이지 않음. R은 SerializationConflict로 끝나고 새 statement가 q3 상태를 읽음 |
| CUR-J05 | 명시 거래의 statement가 int table에 7..10을 C의 apply로 넣고 자기 cursor로 7을 읽음. 호출 사이 C의 원 split이 RootSplit, 두 leaf의 ChildSplit, level 1 root의 RootSplit으로 level 2 tree [1,1,1,1]을 만듦 | split은 statement의 row 변경이 아니어서 revision이 그대로이고 cursor가 8, 9를 읽음. 같은 statement의 다음 apply(11)가 revision을 올리면 같은 cursor의 다음 호출은 `CursorInvalidated`이고 출력을 건드리지 않음. 새 cursor가 다섯 행을 네 leaf에서 읽고, commit(q2) 뒤 새 statement도 같음 |

실행(2026-11-07, debug, 정확한 이름 filter와 `--test-threads=1`): CUR-B01·B02 PASS, CUR-J01·J02·J03·J04·J05 PASS. 실제 시험의 결과 행은 각 단계에서 전체 순서와 같음을 비교하므로 중복·누락이 있으면 실패한다.

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
| compaction | N/A. DEV3에는 compaction SMO가 없다(SMO는 C4의 네 가지). compaction이 생기면 C4의 key 집합·순서 보존과 revision 증가를 같은 방식으로 확인한다 |
| index range 소비 | 3.07(일정 09)이 소유. 이 문서의 대상은 row tree의 RowId key |

<a id="locator"></a>
## 5. locator 설계(08-2b)

[§5.5.5](dev3-task-06-tree-rows.md#algorithms)와 R9의 locator는 "RowId → 그 RowId를 route하는 leaf"의 한정된 물리 hint다. hint는 internal page의 route만 건너뛴다. leaf의 검사와 행의 판정은 cache가 없을 때와 같은 경로로 한다.

| ID | 결정 |
| --- | --- |
| L1 소유 | C의 `RowLocator<'c, 's>`(`wal_io/plan4_apply/row/locator.rs`). caller(세션)가 소유하며 B는 point read 호출에 빌려준다. 처음 쓰는 `CheckedTable`의 원 core·binding·session fence를 기억하고, 다른 원을 가진 table이 오면 모든 entry를 비운다. lifetime이 그 원에 묶여 세션보다 오래 살 수 없다 |
| L2 key와 entry | key는 table anchor의 `PageIdentity`(file·page·incarnation)와 RowId. entry는 key, 기록 때의 anchor root revision, leaf `PageRef`, 그 leaf의 route bounds(`RowRouteBounds`)다. slot·행 version·값은 두지 않는다 |
| L3 용량 | 고정 8,192 entry의 direct-mapped 표 하나를 생성 때 한 번 할당(`try_reserve_exact`, 실패는 생성 거절). slot은 key의 hash로 고른다. 다른 key가 차지한 slot에 기록하면 그 entry를 덮고 eviction으로 센다. 전체 bytes(header + 8,192 × entry) ≤ 1,048,576을 시험과 상수 검사로 확인한다. 이 1 MiB는 metadata/cache 8 MiB 안의 기존 locator 배정이며 새 예산이 아니다([자원 정본](dev3-resource-bounds.md)) |
| L4 조회 | C의 `page_located`는 `page`와 같이 Catalog LOAD와 schema 확인 뒤 anchor를 LOAD하고 root를 brief read로 읽는다. root level이 0이면 조회하지 않는다(건너뛸 internal page가 없음). level ≥ 1이면 entry를 찾는다: 같은 key이고 revision이 지금 root revision과 같으면 hit, 같은 key이고 revision이 다르면 stale, 그 밖에는 miss다 |
| L5 hit | 기록한 leaf와 bounds로 route를 만들고 그 leaf만 LOAD한다. 그다음은 `issue_page` 그대로다: anchor와 leaf의 membership, page LSN, 같은 read set에서 anchor revision 재확인, leaf의 key가 bounds 안인지. hit의 근거는 C4다: 한 세션 원 안에서 revision이 같으면 SMO가 없었으므로 root에서 key를 route한 결과가 기록한 leaf·bounds와 같다. 행 전이(INSERT·DELETE·UPDATE·Prune)는 구조를 바꾸지 않고 leaf 내용만 바꾸며, leaf는 매번 다시 읽는다 |
| L6 stale·miss | 지금처럼 root에서 route하고, `issue_page`가 성공하면 그 route를 기록한다(level ≥ 1일 때만). 실패한 호출은 기록하지 않는다 |
| L7 오류 | locator는 오류를 miss로 바꾸지 않는다. hit 경로의 LOAD·membership·decode·bounds 실패는 cache가 없을 때 같은 page에서 나는 오류와 같은 종류로 그대로 돌려준다. 같은 key의 entry인데 bounds가 그 RowId를 포함하지 않으면 locator 자체의 손상이므로 Invariant다 |
| L8 계측 | lookups, hits, stale, misses, evictions, resets 수와 `working_set()`(차 있는 entry 수), `bytes()`(할당 bytes)를 caller에 보인다. 정답에는 쓰지 않는다 |
| L9 소비자 | B의 새 진입 `read_point_located_live`와 그 adapter `LocatedSource`(page만 `page_located`, 나머지는 `LiveSource`로 전달). cursor(`next_live`, `LiveSource`, `RowCursor`)의 type과 frame은 바꾸지 않는다. cursor는 C2대로 key로 route하므로 hint가 필요 없다 |
| L10 census | point 경로에 `page_located`·hit LOAD의 C 항과 B adapter·인자 항을 더한 `located.*` phase를 S5 RowRead census에 넣고, 모두 Full 이하임을 확인한다. cursor phase와 공유 bytes는 그대로다 |

세션 원에 묶는 이유: anchor revision은 영속 값이지만, crash로 잃은 구조 변경의 revision 값을 복구 뒤 다른 구조 변경이 다시 쓸 수 있다. hint가 open 경계를 넘지 않으면 L5의 "같은 revision이면 같은 구조"가 한 세션 안에서만 쓰인다. 한 세션 안에서 완료된 SMO는 사용자 abort로 되돌리지 않고(R3), stage 뒤 불명 오류·부분 설치는 fence한다([SMO-E05](dev3-tree-smo.md#structural-effect)). 그래서 건강한 세션에서 revision은 줄지 않는다.

<a id="locator-tests"></a>
## 6. locator 시험(08-2b)

| ID | 사건 | 확인 |
| --- | --- | --- |
| LOC-B01 | 표 단위 시험: 기록·조회의 hit/stale/miss, 다른 key의 slot 충돌, 다른 anchor identity(같은 page, 다른 incarnation), 원 변경 | 분류와 계수, eviction, reset 뒤 비어 있음, bounds 밖 entry의 Invariant |
| LOC-B02 | 크기 | `bytes()` ≤ 1,048,576, 용량 8,192 |
| LOC-J01 | level 1 tree(네 leaf)의 모든 행을 located point read 두 번 | 첫 번째는 miss와 기록, 두 번째는 hit. 모든 결과가 locator 없는 point read와 같음. working set = 행 수 |
| LOC-J02 | 다른 거래의 INSERT가 ChildSplit을 만든 뒤 옛 행을 다시 읽음 | stale, 다시 route, 옮긴 행은 새 leaf로 갱신, 그다음 hit. 결과 동일 |
| LOC-J03 | DELETE commit과 drain의 Prune·LeafUnlink 뒤, 그리고 RootCollapse 뒤 | unlink 뒤 stale, 지운 행은 Missing(locator 없는 읽기와 같음). level 0에서는 조회하지 않음 |
| LOC-J04 | old snapshot의 located read가 다른 거래의 UPDATE 뒤 옛 값을 읽음 | cache 유무와 무관하게 같은 값(판정은 cache 밖) |
| LOC-J05 | hint의 leaf를 다른 leaf로 바꾼 손상(시험 hook) | `issue_page`의 bounds 검사가 fatal을 돌려주고 miss로 숨기지 않음. 같은 hook으로 오류를 miss로 바꾸는 mutant는 이 시험에서 실패 |
| LOC-C01 | census | `located.*` phase ≤ Full, cursor phase 불변 |

T06-13 대응: hit·이동·stale은 LOC-J01·J02·J03, eviction은 LOC-B01, cache on/off 결과 동일은 LOC-J01..J04, 손상을 miss로 숨기는 mutant 거절은 LOC-J05다. slot reuse는 N/A다: locator가 slot을 기록하지 않고 B가 leaf에서 다시 seek하며 C의 capture가 slot의 RowId를 확인한다. RowId는 재사용하지 않는다(R4). table 재생성은 DROP이 아직 없어 실제 시험은 N/A이고, key의 anchor identity가 incarnation을 포함하므로 LOC-B01이 다른 identity의 miss를 확인한다. T06-16(locator 접근 data가 buffer의 4배인 고정 fixture, 실제 working set·RSS)은 [A7](../plans/dev3-schedule-baseline.md)대로 대형 실행 18-1에 한 번 배정하고, 08-2b는 작은 검증(LOC-B02, LOC-J01의 working set)만 한다.

<a id="limits"></a>
## 7. 제한

- cursor batch([비교 분석 후속](dev3-task-06-tree-rows.md#cursor-batch-bound))는 측정 뒤의 별도 후보다. 지금 cursor는 호출당 한 행이다.
- index range의 composite key cursor는 3.07(일정 09)이 이 문서의 C1–C7을 따라 공급한다.
- 같은 statement의 호출을 여러 thread에서 동시에 하지 않는다(F02). locator도 `&mut`으로 한 호출만 쓴다.
- 제품 SQL 실행기가 locator를 소유하는 연결은 DEV4 hookup 때 한다. 08-2b의 소비자는 B 진입과 시험이다.

<a id="schedule"></a>
## 8. 일정

08-2(1일)는 08의 마지막 항목이 아니므로 [§3의 항목 조건](../plans/dev3-schedule-baseline.md)으로 적립한다. cursor(08-2a)와 locator(08-2b)가 모두 끝나야 하므로 08-2b의 병합 때 1일을 적립한다(35 → 36/86 = 41.86%). 08-2a의 병합은 적립하지 않는다.

<a id="review"></a>
## 9. 검토

독립 D2 기록은 `module-gate/evidence/dev3-08-2-cursor/review.md`에 있다.

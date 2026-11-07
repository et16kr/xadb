Verdict: CONDITIONAL

# 08-2a(stable cursor across SMO + locator 설계) 독립 D2 검토 1회차

## 대상

- commit `ca62fda`(branch `claude/project-thread-gfugms`), base `origin/main` `597ccca`. 변경 파일 8개: `devdoc/contracts/dev3-cursor-locator.md`(신규), `row/read/driver/smo_tests.rs`(신규)와 `driver/tests.rs`의 `mod` 줄, `row_join/retire/cursor_smo.rs`(신규)와 `unlink_rows.rs`의 `mod` 줄, `devdoc/contracts/README.md`·`documents.json`·`document-validation.json`(등록만).
- 읽은 것: AGENTS.md, module-gate/README.md, 위 diff 전체. B는 `row/read.rs`(`seek_next`, `CursorProgress`), `row/read/driver.rs`(`RowCursor::next/scan/capture_next/publish_next`), `driver/live.rs`, `driver/tests.rs`의 literal 하네스. C는 `plan4_apply/row/read.rs`(`row_operation`, `page`, `check_table`, `capture_schema`, `anchor_root`, `route_root`, `advance_route`, `load_leaf`, `issue_page`, `capture`), `plan4_apply/row.rs`의 `CheckedTable`, `tree_page/search.rs`(`route_row_step`, `select_child`). SMO는 `semantic_row/structure/candidate.rs`, `structure/unlink/candidate.rs`, `plan4_retention/structural_effect.rs`(`structural_shell`)와 그 호출자 셋(`split_row`, `split_row_tree`, `unlink_leaf_live`). 그 밖에 `transaction/concurrency/read.rs`(read gate), `plan4_apply/transaction.rs`(`row_applied`), `anchor.rs`, `session_fence.rs`, `mutable_storage.rs`(binding), `plan4_pages.rs`(`Plan4ResidencyCursor`), 시험 helper(`split_rows.rs`, `auto_split.rs`, `unlink_rows.rs`).
- 대조한 문서: `dev3-task-06-tree-rows.md` §5.5·R8/R9·§6 자원표·T06-12/13/16·F02, `dev3-tree-smo.md`(current-leaf 소비자, UNL-L14, SMO-E05), `dev3-resource-bounds.md:86`, `dev3-module-consumers.md`(MCI01-V06), `dev3-schedule-baseline.md` §3·08-2행·A7.
- 실행: 별도 clone(scratch `review-082a/src`, `ca62fda`, debug, jenv.sh, `--exact`, `--test-threads=1`).
  - (1) 진단: 내 사본의 CUR-J04에 leaf 모양 출력 두 줄만 넣어 실행. DELETE 뒤 `[8]`, UPDATE 뒤 `[4, 4]`(level 1), 끝에 `[4, 5]`로 PASS.
  - (2) 출력 줄을 되돌리고 `ca62fda` 그대로 다시 build해 CUR-B01·CUR-B02·CUR-J02를 실행. 모두 PASS(J02 162.75 s).
  - census와 그 밖의 시험은 실행하지 않았다(변경 없는 코드). 검토 뒤 scratch build target은 지웠다.

## 판정

필수 2, 권고 10, NIT 7. **CONDITIONAL.**

cursor 쪽(08-2a)은 건전하다. C1–C7은 인용한 코드에서 모두 성립한다.

- B는 호출 사이에 `CursorProgress`의 key만 남긴다(`driver.rs:478-483`, `read.rs:221-227`).
- 각 호출은 `route_key`(마지막 검사 key, 없으면 range 시작, 없으면 1)로 route한다. leaf에 다음 key가 없으면 C가 그 호출에서 발급한 `upper`로 다음 leaf를 route한다(`driver.rs:576-602`).
- split과 unlink는 모두 `structural_shell` → `close_reads_for_retirement`로 열린 read operation과 배타다(`structural_effect.rs:298-304`, 호출자 `structural_effect.rs:254`, `structural_live.rs:69`, `structural_unlink.rs:324`). 게다가 `issue_page`가 같은 read set에서 revision을 다시 확인한다(`read.rs:781`).
- 네 SMO는 모두 `revision + 1`이다(`candidate.rs:663`, `unlink/candidate.rs:207, 225`). unlink는 빈 leaf만 뗀다(`unlink/candidate.rs:151`).
- `CheckedTable`에는 revision도 root도 없다(`row.rs:71-78`).
- 호출 사이에 남는 다른 상태는 caller의 `Plan4ResidencyCursor`(`next: usize` frame 순번)뿐이다.

중복이나 누락을 만드는 경로, 호출 사이에 tree 위치를 캐시하는 경로, revision을 올리지 않는 구조 변경은 찾지 못했다. 그래서 3.06.3.a에 제품 변경이 필요 없다는 결론은 받아들인다.

시험도 대체로 계약이 말하는 것을 보인다.

- 각 CUR-J에서 SMO는 cursor의 두 호출 사이에 일어난다. 예외는 J03의 범위 cursor로, 권고 6에 적었다.
- 기대값은 상태 기계 값(`tagged`), pool에서 decode한 tree 모양, 새 statement의 읽기에서 온다.
- B의 OutputTooSmall·취소가 key를 보존하는 것, End 뒤 operation을 열지 않는 것, J05의 CursorInvalidated가 출력을 건드리지 않는 것은 실제로 단언된다.

CONDITIONAL인 이유는 locator 설계(08-2b로 동결하는 L1–L10)의 두 가지다. 1 MiB 상한의 근거인 인스턴스 수와 회계가 정해지지 않았다. key 정의가 owner 계약 §5.5.5의 key("DB/timeline·table generation·RowId")보다 좁게 적혀 있다.

"같은 session origin 안에서 같은 anchor root revision이면 같은 route"라는 핵심 논증 자체는 맞다.

- binding은 한 번만 `BOUND`/`ABANDONED`가 된다.
- fence는 단조이고, fence 뒤에는 `row_operation`의 health 검사가 모든 읽기를 막는다.
- recovery는 새 `MutableStorageOwner`를 만들므로 lifetime으로 묶인 locator가 그 경계를 넘지 못한다.
- `route_root`의 brief read와 `issue_page` 사이에는 read admission이 SMO를 배제한다.
- FlushRequired 재시도는 새 operation에서 revision을 다시 비교한다.

T06-16을 18-1에 배정한 것은 A7("작은 검증은 해당 구현에 한 번, S5 확대는 18에 한 번")과 맞는다. §8의 적립 산술(35 → 36/86 = 41.86%)과 "08-2a 병합은 적립하지 않음"도 §3 항목 조건과 맞는다.

## 필수

1. **locator 인스턴스 수와 1 MiB 회계가 정해지지 않았다**(`dev3-cursor-locator.md:73`의 L1, `:75`의 L3, `:107-108`의 §7).
   - L1은 "caller(세션)가 소유"하고, L3은 생성 때 `try_reserve_exact`로 할당하면서 "이 1 MiB는 … 기존 locator 배정이며 새 예산이 아니다"라고 한다.
   - 그러나 정본의 locator 1 MiB는 metadata/cache 8 MiB 안의 **고정 배정 하나**다(`dev3-resource-bounds.md:86`, `dev3-task-06-tree-rows.md:153`). foreground owner는 32개다(`transaction/concurrency.rs:57` `FOREGROUND_OWNERS = 32`).
   - caller나 연결마다 하나씩 두면 최대 32 MiB가 되어 L3의 주장이 거짓이 된다. storage origin마다 하나라면, §7의 "`&mut`으로 한 호출만"과 여러 owner가 어떻게 함께 쓰는지(배타 대여·잠금의 위치, read admission과의 순서)가 빠져 있다.
   - heap 할당이 어느 ledger 또는 loan으로 차감되는지도 없다. 비슷한 배정은 "disjoint loan으로 확보"로 적는다(`dev3-resource-bounds.md:180`).
   - 고칠 것: (a) 인스턴스 수를 고정한다(storage origin당 하나, 또는 owner별로 나눠 합이 1 MiB 이하). (b) 차감할 ledger와 거절 조건을 적는다. (c) 동시 접근 규칙을 적는다. (d) LOC-B02가 인스턴스 하나가 아니라 합계 상한을 확인하게 한다. 최종 소유자 연결을 DEV4로 미루는 것은 괜찮지만, 상한이 성립하는 구조는 지금 동결해야 한다.

2. **key 정의가 §5.5.5에 못 미친다**(`dev3-cursor-locator.md:74`의 L2, `:91`의 LOC-B01, `:100`의 T06-13 대응).
   - §5.5.5는 "locator cache key는 DB/timeline·table generation·RowId"라고 한다(`dev3-task-06-tree-rows.md:141`). L2는 key를 "table anchor의 `PageIdentity`(file·page·incarnation)"로 적어 `PageRef`의 세 칸만 명시한다.
   - 실제 `PageIdentity`에는 `database`·`page`·`owner`·`owner_generation`이 있고(`page.rs:69-74`), `CheckedTable.anchor`가 그 전체 값이다(`row.rs:77`). 그런데 문구대로 구현하면 table generation이 key에서 빠질 수 있다.
   - T06-13의 table 재생성 N/A 논거도 "anchor identity가 incarnation을 포함하므로"에만 기대고 있다.
   - 고칠 것: key를 전체 anchor `PageIdentity`(database·file·page_no·incarnation·owner ObjectId·owner_generation)와 RowId로 명시한다. timeline은 L1의 origin reset이 덮는다는 대응을 적는다. LOC-B01에 "같은 anchor `PageRef`, 다른 owner_generation(그리고 다른 database)" 사례를 더한다.

## 권고

1. **hit 결과가 Missing일 때의 맹점**(L5·L7 `:77,:79`, LOC-J05 `:97`).
   - `issue_page`의 leaf bounds 확인은 `select_child`가 entry가 있을 때만 한다(`tree_page/search.rs:75` `if view.len() != 0`). 그래서 hint가 다른 빈 leaf를 가리키면 오류 없이 Missing이 된다.
   - 따라서 LOC-J05의 "bounds 검사가 fatal을 돌려주고 miss로 숨기지 않음"은 바꿔 넣은 leaf가 비어 있지 않을 때만 성립한다.
   - 권고: hit에서 leaf가 비었거나 RowId가 없을 때는 root부터 route해 확인하고, route가 다르면 Invariant로 한다. 비용은 Missing일 때만 든다. 이렇게 하지 않으려면 LOC-J05의 hook 대상을 "비어 있지 않은 leaf"로 한정하고 한계를 적는다.
   - 아울러 §5.5.5·R9를 L 항목에 대응시키는 표를 둔다. 특히 "이동·slot 불일치면 hint를 버리고 row tree를 다시 찾는다"를 revision 일치로 대신한다는 점을 명시한다.

2. **revision 역행 처리**(L4 `:76`). 같은 key에서 지금 revision이 기록보다 작으면 stale로 넘어간다. 그러나 `:84`의 논거대로라면 건강한 session에서 revision은 줄지 않는다. 역행은 Invariant로 분류하는 편이 "손상을 숨기지 않는다"와 맞는다.

3. **revision이 table 전체 값이다**. 어떤 SMO든 그 table의 모든 entry를 stale로 만든다. 예를 들어 append로 생기는 오른쪽 split 하나가 먼 leaf의 hint까지 무효로 한다. 이 hit율 특성을 §5나 §7에 적고, T06-16·PF09 측정 항목(동시 append 아래 hit/stale 비율)에 넣기를 권한다. LOC-J02의 기대도 "옮긴 행만"이 아니라 그 table의 전 entry가 stale임을 반영해야 한다.

4. **census 여유**(L10 `:82`). 문서상 point.row_load는 34,380 B이고 Full은 34,586 B다(`dev3-tree-smo.md:926-927`). 여유는 약 200 B다. 그 안에 `LocatedSource`의 locator 참조, entry·route 복사, `page_located` frame이 LOAD와 함께 살아야 한다. 이 여유와 frame 분리(never-inline) 계획, 초과하면 Full을 올리지 않고 다시 설계한다는 원칙을 L10에 적기를 권한다.

5. **CUR-J04가 split의 원인을 단언하지 않는다**(`cursor_smo.rs:591-648`). 계약(`:46`, `:58`)은 UPDATE의 값 검사가 split을 만든다고 한다. 그러나 시험은 commit 뒤에 leaf가 둘이라는 것만 본다. 그래서 INSERT가 split했더라도 통과한다. 내가 사본으로 확인한 결과 주장 자체는 참이다(DELETE 뒤 `[8]`, UPDATE 뒤 `[4, 4]` level 1). `update_statement!` 바로 뒤에 leaf 수 2·level 1 단언을 넣기를 권한다.

6. **CUR-J03의 범위 cursor는 SMO를 "건너지" 않는다**(`cursor_smo.rs:466`, `:471-483`). 범위 cursor는 drain 전에 열기만 하고 첫 호출은 root 축소 뒤에 한다. `RowCursor::open`은 tree를 읽지 않으므로 이것은 축소된 tree에서 새로 시작한 읽기다. `:45`의 "두 cursor가 그 위에서 계속"은 과장이다. 둘 중 하나를 권한다.
   - 문구를 고친다.
   - 시험을 고친다: drain 전에 범위 cursor로 한 행을 읽는다(삭제된 key를 검사한 뒤 kept[0]). 축소 뒤에는 kept[1]을 읽고 range 끝에서 End가 나는지 본다.

7. **CUR-J02의 "마지막 검사 key를 가진 leaf를 unlink"는 단언되지 않는다**(`cursor_smo.rs:330-355`, 계약 `:44`). 출력 부족 호출이 검사 key를 지운 leaf까지 옮기지 않았더라도 결과 행은 같다. 이 성질은 CUR-B02가 B 수준에서 보이고, J02에서는 추론이다. `LiveCursor`의 진행점을 보는 시험 전용 accessor로 단언하거나, 문구를 "CUR-B02의 진행 규칙에 의해"로 고치기를 권한다.

8. **§7에 남은 F02 의무를 적기를 권한다**.
   - F02는 "일반 여러 leaf/Undo chain의 작업 budget·batch API는 별도 미공급 의무"라고 했다(`dev3-task-06-tree-rows.md:297`). 08-1 이후 cursor는 실제로 여러 leaf를 읽는다.
   - 한 호출이 보이지 않는 행을 개수 제한 없이 검사할 수 있고, 행마다 root부터 다시 route한다. 이때 Catalog LOAD, schema 복사, anchor·경로 LOAD를 매번 한다(`driver.rs:645-675` → `read.rs:320-335`).
   - 그동안 read admission을 쥐므로 SMO와 terminal gate가 기다린다(`concurrency/read.rs:187`).
   - "호출당 한 행"(`:105`)과는 별개로, 이 의무가 아직 열려 있다는 것과 그 소유 항목을 적기를 권한다.

9. **T06-13 대응 보강**(`:100`).
   - "slot reuse"를 N/A로 두기보다 같은 revision에서 대상 행의 slot 위치가 바뀌는 사례를 LOC-J에 넣기를 권한다. hint leaf 안에서 앞 행이 Prune되고 unlink는 없는 경우다. 그러면 slot이 다른 hit를 보이게 된다.
   - "stale incarnation"(T06-13 원문)도 명시적으로 대응시킨다. leaf page 재사용은 3.14 전에는 없다. 재사용이 생겨도 그 앞에 unlink가 revision을 올린다는 근거를 적는다.

10. **실행 증거의 source 식별**(`:49`).
    - PASS 줄에 source(commit 또는 blob)가 없다. 작성자의 scratch log에서 J02 log의 마지막 기록은 00:02:18이고, 마지막 engine 재build log는 00:05:22다. 즉 J02는 마지막 재build 전의 binary에서 돌았다.
    - 내가 `ca62fda` 그대로 다시 실행한 J02·B01·B02는 PASS였다.
    - 결과 줄에 실행한 commit을 적고, module-gate record(7개 사례의 `rust_tests` 대응, 시험 파일 blob을 덮는 review)를 PR에 더해야 한다. record는 이 commit에 아직 없다.

## NIT

1. C1(`:24`)의 "…뿐이다": `RowCursor`는 view와 table도 빌린다(`driver.rs:478-483`). 또 caller의 `Plan4ResidencyCursor`가 호출 사이에 남지만 frame 순번(`plan4_pages.rs:108-115`)뿐이라는 점을 덧붙이면 C1이 완결된다.
2. C7(`:30`): revision 확인이 End보다 먼저다(`driver.rs:523-528`, F02 `dev3-task-06-tree-rows.md:291` "End보다 CursorInvalidated가 먼저"). 이 순서를 적어 두기를 권한다.
3. L6(`:78`)의 "실패한 호출은 기록하지 않는다": 기록 시점은 `issue_page` 성공이다. 그 뒤 Status·Undo 단계가 FlushRequired나 오류로 끝나도 기록은 남는다는 뜻으로 읽히게 고치기를 권한다.
4. L5(`:77`): hit 경로도 `issue_page`의 `record_table_read(table.slot)`(`read.rs:726`)을 그대로 거친다는 점(Serializable read set)을 명시하기를 권한다.
5. L3(`:75`): entry 크기를 적기를 권한다. `PageIdentity`는 56 B(`DatabaseId` 16 B, `api.rs:8`)이므로 entry는 약 120 B이고, 8,192개면 983,040 B다. §6 자원표가 요구하는 allocator 오버헤드(`dev3-task-06-tree-rows.md:153`)도 `bytes()`에 포함한다고 적기를 권한다.
6. §4의 compaction N/A(`:63`): 별도 compaction은 없지만 leaf 편집마다 page를 다시 encode해 slot 위치가 바뀐다. cursor는 key로 seek하므로 영향이 없다는 한 줄을 더하면 정확하다.
7. L9(`:81`)의 "cursor는 … hint가 필요 없다"는 정답의 근거에 대한 말이다. 성능상으로는 호출마다, 그리고 보이지 않는 행마다 root부터 route한다(권고 8). "정답에 필요 없다"로 좁히기를 권한다.

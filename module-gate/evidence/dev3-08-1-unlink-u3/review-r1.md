Verdict: CONDITIONAL

# U3(08-1 live unlink) 독립 D2 검토 1회차

## 대상

- 범위: `git diff 92b22bf..626e062`(branch `u3-main`). 처음 받은 범위 `92b22bf..44a921f`에 coordinator가 전한 작성자
  commit `44a921f..626e062`(SPL-J02 기대값·doc comment, `#unlink-u3` 문단, SPL-J02 역사 행, document-validation)을 더했다.
- HEAD: `626e062`. 작성자 worktree에는 `626e062` 뒤에 commit되지 않은 수정(dev3-tree-smo.md, document-validation.json,
  dev3-schedule-baseline.md)이 있다. 이 수정은 범위 밖이라 보지 않았다. 아래 문서 줄 번호는 모두 `git show 626e062:…` 기준이다.
- 읽은 것
  - 설계: `devdoc/contracts/dev3-tree-smo.md` `<a id="empty-leaf-unlink">`부터 "### 구현 단계"까지. 작성자 결과
    `<a id="unlink-u3">`부터 끝까지(census, 계획과 다르게 남긴 것, `#unlink-u3-runs`). 역사 행 SPL-J02(934), SPL-A03(1095),
    상태 줄(1126).
  - `dev3-module-boundaries.md`의 바뀐 두 행(3305, 3418), `dev3-task-14-reclamation.md`의 호출 위치 문장(284)과 바로 뒤
    "자원 (A)" 문단(286), `AGENTS.md`.
  - code: `plan4_retention/structural_unlink.rs` 전체(note 모듈, `unlink_next`, `note_emptied_leaf`, `unlink_leaf_live`,
    `unlink_resident`, `unlink_leaf_empty`, `unlink_parent_shape`, prepare/install, census 함수, 단위 시험),
    `row_start/retire.rs`(DeletePrune note), `row_clr.rs`(INSERT CLR note), `row_forward.rs`(census), `catalog_live.rs`
    (`ControlCut`, `check_group`, row_retirement census), `structural_live.rs`(split residency 분리),
    `catalog_lifecycle.rs`(`ExistingGroup::Structural`), `plan4_apply.rs`(`unlink_busy`),
    `dirty_frame/reader_load.rs`(`select_load`), `lookup_load.rs`, A 쪽 `row/insert/driver.rs`·`driver/live.rs`·
    `group.rs`(`anchor_decode_terms`)·`driver/tests.rs`(UNL-A01, UNL-C03 A 쪽), `plan4_apply/source_tests.rs`(UNL-A03,
    UNL-C03), 시험 `unlink_rows.rs`(UNL-L01..L15), `split_rows.rs`(`leaves`, `shape`, SPL-J02), `auto_split.rs`(SPL-A03).

## 판정

- 필수 2: (1) unlink residency 단계가 P를 보호하지 않은 채 leaf를 LOAD하고 그 뒤 P를 typed read하므로, P가 victim이
  되면 Busy가 아닌 `Frame(MissingTarget)`이 나와 note를 버리고 drain을 멈춘다(정확성 결함, 시험 없음).
  (2) SPL-A03의 계약 행과 시험 이름은 "split이 rollback 뒤에도 남는다"고 하지만 U3 뒤에는 rollback drain이
  RootCollapse를 한다. 시험은 퇴역 leaf까지 세는 helper로 "leaf 2개"를 보므로 이름이 주장하는 것을 판정하지 않는다
  (틀린 명시 값, 시험 판정 결함). 같은 경우인 SPL-J02는 `626e062`에서 고쳤고 SPL-A03은 남았다.
- 권고 6: L09 NOT_RUN 근거와 LOAD 경로 증거, C03 note 위치의 기록·불변식·자기참조 시험, U1 decode_anchor 치환 논증의
  범위, `unlink_next` 본문의 census 처리와 reclamation 문단, unlink 판정 helper가 unlink 여부를 가르지 못함(SPL-J02
  새 기대값 포함), 실행 기록이 고른 시험족의 범위.
- NIT 7: residency frame 항 384B 누락(구속 아님), 3,738B+816B 표현, "bank 39행", UNL-A02 "독립" 표현, note 실패
  처리의 비일관성, `check`의 "→ Busy" 표현, 상태 줄과 SPL-J02 역사 행의 모순 문장.
- UNL-C03 의무는 실질적으로 이행됐다고 본다. note 8칸은 caller가 빌려주는 control 대여 bytes [65,088..65,536) 안에
  있어 owner·session·recovered runtime·port 어느 타입에도 inline 필드가 늘지 않는다. shared layout 20,208/20,216B와
  charge 44,304B, 합 64,512/64,520B ≤ 65,536B가 그대로이고, `Retired`는 24B 그대로이며, `unlink_notes`
  (`#[inline(never)]`)와 새 cleanup 지점 넷이 A census에 들어가 있다. 상한은 올리지 않았다. 문서에 적힌 census 값은 내가
  돌린 출력과 모두 같다. 다만 이 위치는 설계 문구("shared owner/port charge에 포함")와 다른 선택이므로 권고 2로 기록을
  요청한다.
- `626e062`(SPL-J02) 판정: 새 기대값은 맞고, product 값을 복사하지 않은 literal이라 독립이다. `unlinked: 1`(split_rows.rs:718)은
  오른쪽 leaf가 CLR 4개로 비고 root가 separator 1개인 UNL-L06과 같은 경우라 RootCollapse 한 번이 맞다. INSERT 13 뒤
  `([0, 3], …)`(768)도 맞다. `leaves`는 첫 key로 정렬하고(63) 빈 leaf는 `None`이라 맨 앞에 오므로, unlink가 없으면
  `[7,8]`, `[13]` 순서인 `[2, 1]`이 되고 unlink가 있으면 빈 퇴역 leaf가 앞에 와 `[0, 3]`이 된다. 작성자 문장(1840)과
  같다. 다만 rollback 직후 `([0, 2], …)`(727)는 unlink가 있든 없든 같은 값이라 unlink를 판정하지 않는다(권고 5).

## 필수

### 필수 1. leaf LOAD가 P를 보호하지 않아, P가 victim이면 note가 버려지고 drain이 멈춘다

- 위치: `crates/engine/src/storage/wal_io/plan4_retention/structural_unlink.rs:418`(leaf LOAD, 보호 집합 `&[anchor, leaf]`),
  `:424-426`(`let parent = path[depth - 1]`, `unlink_parent_shape(owner, parent, leaf)`), `:486`
  (`CatalogTargetAccess::Ordinary.read(self.pool, owner, &[parent])`), `:308-314`(drop 판정과 `note::remove`),
  `crates/engine/src/storage/wal_io/plan4_apply.rs:371-383`(`unlink_busy`).
- 결함: P는 route 단계(`:405-407`)에서 `&[anchor, internal]` 보호로 resident하게 되지만, 그 뒤 leaf의 LOAD(`:418`)는
  `[anchor, leaf]`만 보호한다. `dirty_frame/reader_load.rs`의 `select_load`(226-375)는 보호되지 않은 frame 가운데 빈 것,
  그다음 처음 만나는 clean frame을 victim으로 고르고, clean이 없으면 처음 만나는 dirty frame을 `FlushRequired`로
  돌려준다. 그래서 P는 이 LOAD의 victim이 될 수 있다. P의 모양은 LOAD 직후가 아니라 leaf LOAD와 leaf 검사 뒤
  (`:426`)에 읽으므로, 그 사이 P가 빠지면 `:486`의 typed read는 Unchanged `Frame(MissingTarget)`을 낸다. 이 원인은
  `unlink_busy`의 목록(Concurrency Busy, Frame Stale·Busy, Visibility Busy)에 없으므로 `:310`에서 `drop`이 true가 되어
  note가 지워지고, 오류는 A로 올라가 drain이 `RetireStop`(`Stage::Unlink`)으로 끝난다(UNL-A01이 보는 "다른 거절"
  경로, driver/tests.rs:3983-4040).
- 구체 시나리오(작은 pool, 결정적): pool의 빈 frame이 없고, 보호되지 않은 frame이 모두 dirty이며 scan 순서에서 P가
  처음이다. L은 resident가 아니다(DELETE 뒤 다른 statement가 L을 밀어냈다. note는 statement를 넘어 남는다).
  1회차: route에서 P는 hit, leaf LOAD가 victim으로 P를 골라 `FlushRequired(P)`를 돌려준다. A는 P를 flush하고 같은 note로
  다시 부른다. 2회차: P는 이제 clean이고 보호되지 않은 유일한 clean frame이므로 leaf LOAD가 P를 내보내고 L을 올린다.
  `unlink_leaf_empty`(`:421`)는 참, `unlink_parent_shape`는 `MissingTarget`. note는 사라지고 빈 leaf는 tree에 남으며,
  같은 leaf를 다시 비우는 DELETE나 CLR이 없으면 다시 note되지 않는다. P가 먼저 clean이고 scan 순서에서 처음인
  경우에는 1회차에서 바로 같은 결과가 난다.
- 대조: live split은 같은 `split_load`를 쓰지만 각 LOAD 직후에 그 page를 읽는다(`structural_live.rs:158`의
  `separator_room`, `:168`의 `leaf_needs_split`). unlink만 읽기를 다른 page의 LOAD 뒤로 미뤘다. 그래서 "C의 LOAD 단계는
  live split의 `split_load`와 같은 함수다"(dev3-tree-smo.md:1850)는 이 경로의 근거가 되지 못한다.
- 시험: 이 경로를 보는 시험이 없다. UNL-L09는 NOT_RUN이고(1845), UNL-L 시험은 모두 29 frame 이상에서 anchor·P·L·S가
  resident인 상태로 돈다(작성자 문장 1847-1848).
- 고칠 것: leaf LOAD의 보호 집합에 P를 넣거나(`&[anchor, path[depth - 1], leaf]`, depth ≥ 1은 `:414`에서 이미 보장),
  split처럼 P의 모양을 P의 route LOAD 직후에 읽는다. C 직접 시험을 하나 더한다. 위 시나리오처럼 P가 첫 victim 후보인
  pool에서 `FlushRequired(P)`, flush, 재시도를 거쳐 `Unlinked`가 되고, 그 전까지 note가 남는지 본다. `MissingTarget`이
  다른 이유로 나는 경우를 note 유지 쪽에 둘지도 계약에 적는다.

### 필수 2. SPL-A03의 계약 행과 시험이 U3 뒤에 거짓인 "split이 남는다"를 주장한다

- 위치: `devdoc/contracts/dev3-tree-smo.md:1095`(SPL-A03 행 "rollback 뒤 행은 없고 split(두 leaf, 빈)은 남음"),
  `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/auto_split.rs:379`
  (`spl_a03_split_survives_the_rollback_of_its_transaction`), `:424`(rollback 뒤 `drains!` 결과를 버림), `:426`·`:446`
  (`assert_leaves(…, 2, …)`), `split_rows.rs:39-64`(`leaves`는 pool에 등록된 RowLeaf 전부를 읽는다), dev3-tree-smo.md:1837
  ("기존 시험 하나의 기대값이 바뀌었다").
- 결함: UNL-L06(`unlink_rows.rs:826-836`, doc comment가 "SPL-A03's state"라고 밝힌다)은 같은 상태에서 rollback drain이
  RootCollapse 한 번을 설치하고 anchor가 다른 leaf와 옛 root를 목록에 올린다고 판정한다. 내가 돌린 UNL-L06 출력도
  `root=page 13, listed=[23, 24]`였다. 그러니 SPL-A03 상태에서 "split(두 leaf)"은 남지 않는다. 시험은 drain 결과를 보지 않고
  (`:424`), `leaves`가 퇴역 leaf까지 세므로 `assert_leaves(…, 2, &[], true)`가 그대로 통과한다. 시험 이름과 계약 행이
  주장하는 성질을 이 시험은 판정하지 않는다. 작성자는 `auto_split.rs`의 doc comment에 "the rollback's drain then
  collapses the root over them"을 더했지만 이름, 계약 행 1095, 판정은 그대로 두었다. `626e062`는 같은 종류의
  변화(SPL-J02)만 역사 행 934와 1837 문단에 반영했으므로, "하나"라는 1837 문장도 SPL-A03을 빠뜨린다.
- 실패 시나리오: 나중에 RootCollapse가 깨져서(예: drain이 unlink를 부르지 않게 되어) split이 정말 남아도 SPL-A03은
  통과하고, 반대로 지금 상태에서 계약 1095를 믿은 독자는 rollback 뒤 tree가 level 1이라고 본다. 둘 다 시험이
  못 잡는다.
- 고칠 것: 1095에 SPL-J02 행(934)처럼 U3 뒤의 결과를 적고, 1837 문단에 SPL-A03을 더한다. 시험은 rollback drain의
  `Retired`를 `unlinked: 1`로 판정하고 anchor의 root·level·목록을 보거나(`unlink_rows.rs:114`의 `anchor_of` 같은 것),
  아니면 이름과 설명을 "split의 행이 rollback되고 tree가 RootCollapse된다"로 바꾸고 판정을 UNL-L06에 맡긴다고 적는다.

## 권고

1. **UNL-L09 NOT_RUN의 근거와 LOAD 경로 증거.** dev3-tree-smo.md:1845-1850은 "이 틀에서는 dirty victim 상태를 만들 수
   없다"와 "C의 LOAD 단계는 split과 같은 함수"를 근거로 든다. 첫째는 A를 거치는 JOIN 틀의 한계일 뿐이다. C 직접 시험
   (`unlink_live`를 바로 부르고 pool을 미리 채우는 것)으로는 만들 수 있다. 둘째는 필수 1 때문에 성립하지 않는다(보호 집합과
   읽는 순서가 다르다). 필수 1의 시험으로 L09의 FlushRequired 왕복을 함께 판정하기를 권한다.
2. **C03 note 위치: 정당하지만 기록과 불변식이 모자란다.** control 대여 안에 둔 것은 정당하다. 대여는 caller의
   `RETENTION_CONTROL_BYTES`이고, fresh·recovered writer 모두 같은 대여를 받으며, `validate_live`의 canonical 비교는
   앞 36행만 본다. 그러나 (a) 설계 UNL-C03 문구는 "shared owner/port charge에 포함한 산정"이라 이 선택은 설계와 다른
   것인데, "계획과 다르게 남긴 것"(1843-)에 없다. (b) note 구역은 `BANK_END`(catalog_live.rs:38 = 65,536) 안이라
   `ControlCut::capture/matches`(catalog_live.rs:416-431)의 bank digest에 들어간다. "note는 group 설치 뒤에만 쓰고 drain의
   단계만 지우므로 준비한 cut이 그 쓰기를 넘지 않는다"는 불변식이 code comment(structural_unlink.rs:55-62)에만 있다.
   이 불변식이 깨지면(준비와 설치 사이에 note를 쓰는 호출자가 생기면) `check_group`이 Busy로 거절한다. 계약에 적고 시험
   하나로 고정하기를 권한다. (c) `unl_c03_notes_live_in_the_control_loan`(plan4_apply/source_tests.rs:465)이 비교하는
   `slots_end`·`jobs_start`는 `note::region()`(structural_unlink.rs:138-144)이 literal `39 * SLOT_BYTES`와 `65_536`으로
   돌려주는 값이다. control 모듈의 `BANK_END`나 실제로 쓰는 마지막 slot(`STRUCTURAL_SLOT` 34)에서 오지 않으므로, control의
   배치가 바뀌어도 이 시험은 그대로 통과한다. `const _` assert(structural_unlink.rs:69)도 같다.
3. **U1 open item(decode_anchor) 치환 논증의 범위.** `unl_c03_a_producer_bounds_the_anchor_decode`(driver/tests.rs:4080)는
   `decode`(1,867B) ≤ `write_pass`(`images`+`frame` carrier 4,132B)만 본다. 이 논증은 write-pass 몫을 `observe`의 중첩 호출
   하나로 대신 쓰는 것인데, `observe`(group.rs:697-)는 그 밖에도 `row::decode_job`을 두 번 이상(root, terminal) 부르고
   leaf 검사도 한다. 그 호출들이 같은 몫에 기대는지, 따로 세어졌는지가 계약(1804-1807)과 시험에 없다. "`observe`의 중첩
   호출 가운데 가장 큰 것 ≤ write pass"로 시험을 넓히거나, 다른 호출이 어느 항에 들었는지 적기를 권한다. 숫자는
   맞다(inventory 19,436B ≤ 19,456B, 여유 20B).
4. **`unlink_next` 본문의 census 처리와 reclamation 문단.** C의 `unlink_next`(structural_unlink.rs:194-204)는 note마다
   `catalog_debt`로 control Job 전부를 `job3::decode_job`한다. reclamation의 같은 종류 본문(`row_cleanup_next`)은 "charge 밖,
   따로 세지 않음"이라고 명시돼 있는데(dev3-task-14-reclamation.md:286), U3 결과에는 `unlink_next` 본문을 어떻게 셌는지가
   없다. 또 같은 파일 286의 "자원 (A)" 문단은 여전히 "A의 네 C 호출 지점"과 그 네 값만 적는다. U3 뒤에는 여덟 지점이고
   최대가 `unlink_try_flush` 3,738B(frame-static)다. 작성자가 고친 문장은 바로 앞 "호출 위치" 문단(284)이라, 이 문단에 U3 지점을
   더하거나 dev3-tree-smo.md의 U3 census로 가는 link를 두기를 권한다.
5. **unlink 판정 helper가 unlink 여부를 가르지 못한다.** `unlink_rows.rs:134`의 `reachable`은 "현재 tree가 닿는 leaf"라고
   설명하지만 실제로는 "등록된 leaf에서 anchor 목록을 뺀 것"이다. separator가 퇴역 leaf를 가리키는 결함이나, 목록에 없는데
   tree에서 끊긴 leaf는 잡지 못한다. `internals`(144)가 있으니 root에서 route를 따라 닿는 leaf 집합과 비교하기를 권한다.
   `626e062`의 SPL-J02도 같은 약점이 있다. rollback 직후 `([0, 2], ids(&kept))`(split_rows.rs:727)는 unlink가 없어도 같은
   값이고(빈 오른쪽 leaf가 정렬상 맨 앞), 그 시점의 collapse는 product가 보고하는 `unlinked: 1`만 판정한다. doc comment
   (636)의 "the retired right leaf stays Allocated"는 판정하지 않는다. anchor의 root가 왼쪽 leaf이고 level 0이며 목록이
   {오른쪽 leaf, 옛 root}인지 보면 `626e062`의 기대값이 product 보고와 독립인 판정이 된다. INSERT 13 뒤의 `[0, 3]`은 이미
   독립적인 route 판정이다.
6. **실행 기록이 고른 시험족의 범위.** `#unlink-u3-runs`의 표는 새 시험(`unl_l01..l15`, `unl_a0`, `unl_c03`)만 적는다.
   미완성은 판정하지 않지만, 바뀐 기존 code에 닿는 시험족이 목록에 없다. retire.rs·row_clr.rs의 note 기록(LRC-J01·J02,
   SPL-J01..J05, rollback 계열), structural_live.rs의 residency 분리(SPL-A01..A04, SPL-J), catalog_lifecycle.rs의
   `ExistingGroup::Structural`(기존 Catalog 설치 시험), A의 cleanup 지점(`f4_a_*`, `rdl_a08_*`, LRC-A01..05), census
   (S2W-C01..C06, `live_row_retirement_fits`)가 그렇다. coordinator가 전한 작성자 batch(319 passed, 1 failed SPL-J02, 1 ignored)와
   `626e062` 뒤의 SPL-J02 PASS를 명령과 함께 기록에 넣고, 위 시험족이 그 batch에 들었는지 적기를 권한다. census 쪽은 내가
   돌렸고 모두 통과했다(아래).

## NIT

1. **residency frame 항 384B 누락.** `source_bytes`의 frame 항(structural_unlink.rs:931-944)은 `path`·`set`·cursor를 세지만
   `split_load`에 넘기는 보호 배열 임시값(`[anchor]`, `[anchor, internal]`, `[anchor, leaf]`)과 마지막 loop의 `*request` 복사를
   세지 않는다. 내 셈으로는 `TargetRequest` 64B × 6 = 384B다(probe로 `size_of::<TargetRequest>()` = 64 확인). residency 36,063B에
   더해도 36,447B < prepare 36,561B(peak)이라 구속하지 않는다.
2. **"3,738B에 816B를 더한" 표현(dev3-tree-smo.md:1785-1786).** 표 값은 frame-static에 344B를 더한 값이다(3,738+344=4,082,
   3,249+344=3,593, 3,289+344=3,633, 이전 try_flush 3,635+344=3,979). 3,738+816=4,554B로 읽히는 문장이라, frame-static이 이미
   든 caller term 472B를 실행기 term 816B로 바꾼 것(차 344B)으로 보인다는 점을 적어 주면 좋겠다.
3. **"bank 39행"(1727, 그리고 code comment structural_unlink.rs:55-56·1219).** control bank가 이름 붙여 쓰는 마지막 행은
   `STRUCTURAL_SLOT` 34이고(control.rs:20) canonical 비교는 36행이다. 39는 65,536B 안에 1,664B 행이 들어가는 최대 개수다.
   "39행"이라는 표현은 bank가 39행을 쓰는 것처럼 읽힌다.
4. **UNL-A02의 "independent little-endian bytes"(structural_unlink.rs:1222, 계약 1830).** slot[24..48]은 product
   `PageRef::encode`(`anchor(2).encode`, 1257-1259)로 만든 bytes와 비교한다. 이 필드는 독립 대조가 아니다.
5. **note 관련 실패 처리가 고르지 않다.** `delete_prune_note`(row_start/retire.rs:460-462)는 범위 밖 ordinal을 Unchanged
   `Prepare(Invariant)`로 돌려주고, `note_emptied_leaf`는 `note::record`의 모든 오류(slot 해독 실패 포함)를 버린다
   (structural_unlink.rs:266). 같은 파일의 다른 control 손상은 Fatal Census다. 어느 쪽이 의도인지 한 줄 적어 주면 좋겠다.
6. **`check`의 "→ Busy"(1751-1752).** prefix·cut 불일치는 bare `RetentionCause::Busy`(catalog_live.rs:658-659)인데, 이것은
   `unlink_busy`(plan4_apply.rs:371-383)의 목록에 없어 note를 버리는 쪽이다. 같은 줄의 `Frame(Stale)`은 note를 남긴다.
   지금 single-writer shell 안에서는 닿지 않는다고 보지만, 문장만 보면 둘 다 Busy 계열로 읽힌다.
7. **상태 줄과 SPL-J02 역사 행.** 1126은 여전히 "U3..U4 구현 없음"이다. 934의 SPL-J02 행은 "다음 INSERT(q3)는 오른쪽
   leaf"를 그대로 두고 뒤에 "U3 뒤에는 … root leaf로 들어간다"를 붙여, 한 행 안에서 두 문장이 서로 어긋난다. 앞 문장에
   "(U3 전)"을 붙이면 된다.

## 직접 확인한 것

- 사본: `git -C …/wt-u3 archive u3-main | tar -x -C …/scratchpad/review-u3`(그때 `44a921f`). coordinator 전달 뒤
  `git -C …/wt-u3 diff 44a921f..626e062`를 사본에 `git apply`했고, `626e062`의 archive와 `diff -rq --exclude=target`으로
  비교해 IDENTICAL을 확인했다. 작성자 worktree는 읽기만 했다.
- build(사본, `source …/scratchpad/jenv2.sh`, `CARGO_TARGET_DIR=…/review-u3/target`):
  `cargo test -j 2 -p xadb-engine --lib --no-run`. `44a921f` 2분 17초, probe를 넣고 40초, `626e062` 적용 뒤 47.5초, 모두 성공.
- census(live 아님, `engine-e4e141083d864921 --test-threads 2 --nocapture …`):
  - `unl_a03_live_unlink unl_c03 live_row_retirement_fits unl_a01 unl_a02 lrc_a0`: 16 passed.
    `UNL_A03_{MEMORY,NATIVE_TYPE}`: carriers 961, residency 36,063, prepare 36,561, install 31,053, peak 36,561, shell 928,
    note 6,724, row_retirement 39,694, charge 44,304, room 3,647. `UNL_C03_MEMORY` shared 20,208, `UNL_C03_NATIVE_TYPE` shared 20,216,
    notes 65,088..65,536, slots_end 64,896, jobs_start 65,536, control 196,608, charge 44,304. `UNL_C03_A` decode 1,867,
    write_pass 4,132, producer 19,436, bound 19,456, room 20.
  - `f4_a_ rdl_a08`: 모두 ok. `cleanup_site` share/frame: next 2,664/2,840, retire 2,868/3,164, try_flush 3,098/3,635,
    continue_flush 3,090/3,627, unlink_next 2,729/3,249, unlink 2,801/3,289, unlink_try_flush 2,993/3,738,
    unlink_continue_flush 2,985/3,730. drain 항목(autocommit·explicit·autocommit-delete 같음): try_flush 3,979(여유 117),
    unlink_next 3,593(503), unlink 3,633(463), unlink_try_flush 4,082(14), unlink_continue_flush 4,074(22). 문서 표와 같다.
  - `s2w_c0 unl_r04`: 8 passed. preflight c_margin 182, 두 writer 같음. 문서와 같다.
- 임시 probe(사본에서만): `structural_unlink.rs` tests 모듈 끝에 `review_probe_unlink_sizes`를 더해 크기를 찍었다.
  `TargetRequest` 64, `Result<RowUnlinkStep>` 72, `RowUnlinkStep` 64, `Plan4Target` 64, `Result<UnlinkResidency>` 72, `Note` 56,
  `RowTree` 80. 그 뒤 원래 파일로 되돌리고 `cmp`로 같음을 확인했다. NIT 1의 셈에 썼다.
- live 시험 3개(한도):
  - `unlink_rows::unl_l06_` + `unlink_rows::unl_l05_l12_`: 2 passed, 39.4초. `UNL_L06 root=page 13 listed=[23, 24]`(필수 2 근거),
    `UNL_L05_L12 leaves=[13, 27, 25, 28, 31] p1=26 p2=29`.
  - `unlink_rows::unl_l08_l13_`: passed, 309.5초. `UNL_L08 emptied=page 39 fresh=page 40 leaves=[3, 3, 3, 5]`.
  - SPL-J02(`626e062`)는 한도 때문에 돌리지 않았다. 작성자 보고 PASS를 받아들이고 기대값은 code와 helper로 판정했다
    (판정 절).
- 정적 확인: 필수 1은 `select_load`(reader_load.rs:226-375)의 victim 규칙, `lookup_load.rs:166`(cursor는 Claimed일 때만
  전진), `unlink_resident`(structural_unlink.rs:378-442)와 `split_resident`(structural_live.rs:150-170)의 읽기 순서 대조,
  `unlink_busy` 목록으로 판정했다. 실행 재현은 하지 않았다(live 한도, 작성자 틀에서 pool 상태를 만들 수 없다는 L09 기록).
- 끝난 뒤 사본의 target 디렉터리(`…/review-u3/target`, 3.3G)를 지웠다. 사본 source와 적용한 diff(`…/scratchpad/u3-626e062.diff`)는 남겼다.

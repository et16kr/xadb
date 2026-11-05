Verdict: ACCEPT

# 독립 D2 재검토(3회차): A 자동 split 호출자

- 대상: commit `8dbe79ab4363e26dc186549412573a393fae7ce4`(worktree `/home/claude/xadb-impl`, branch `autosplit-impl`). 기준은 origin/main `2102038`이다. 2회차 대상 `3204cb4`와의 차이(`git diff 3204cb4 8dbe79a`)를 중심으로 보았고, 전체 변경(`git diff 2102038 8dbe79a`) 안에서 확인했다.
- 결론: 2회차 필수 1은 해소됐다. 새 필수는 없다. **열린 필수 0건, 판정 ACCEPT.** 남은 것은 선택뿐이고, 계약이 미작성 D2 선택으로 명시했다.
- worktree는 고치지 않았고 commit·push도 하지 않았다. 검토 시점 `git status`는 clean이다.

## 확인 방법

1. **차이 읽기.** `3204cb4..8dbe79a`의 9개 파일을 모두 읽었다: driver.rs 주석, plan4_apply.rs 주석 위치, auto_split.rs, structural_live.rs census, README, dev3-row-update.md, dev3-tree-smo.md, document-validation.json, module-gate 기록.
2. **시험 실행.** `git archive 8dbe79a`로 scratch 사본(`scratchpad/review-autosplit/r3src`)을 만들었다. 거기에 `note_full`과 `leaf_needs_split` 값을 찍는 `eprintln!` probe만 넣고, 별도 target(`scratchpad/review-autosplit/tgt`)에서 다음 시험을 돌렸다.
   - SPL-A10, SPL-A07·A08·A09
   - MCA00-D12, MCA00-D15(memory·native)
   - `f4_a_*` 6개, `rup_a08_*` 4개
   - 결과: 18개 모두 통과(`probe-r3.txt`).
3. **문서 검증기.** probe 없는 별도 사본(`r3docs`)에서 `devdoc/verify_documents.py`를 돌렸다. errors가 없다(markdown 287개, `docs-r3.txt`).

## 측정값

| 항목 | 값 |
| --- | --- |
| SPL-A10 probe | C 직접 호출 `need len=380 D=2048 free=2612 short=false`(NotNeeded). 거래 INSERT는 `note_full stage=Reserve len=381` 뒤 `need len=381 D=2048 free=2612 short=true`, Split. leaves [3,5], Commit q 9, 재시작 q 9 |
| SPL-A08 (MEMORY·NATIVE_TYPE) | carriers 1,352 / residency 36,900 / effect 37,438 / peak 37,438. 37,438 + 6,144 = 43,582 ≤ 44,304, room 722. 계약 1079–1082행과 일치 |
| A executor | insert.preflight 5,716/5,483, insert.split 5,388/5,155, update.split 5,161/4,928. 2회차와 같고 계약과 일치 |
| MCA00-D12 | live_insert 11,644 / 12,288 |

## 2회차 필수 1: 해소

**요구.** Reserve Capacity(D>0) 경로를 실제 C 위에서 검증하고, SPL-A03·`auto_split.rs`의 틀린 서술을 바로잡는다.

**새 시험.** `spl_a10_reservation_capacity_splits_beside_a_retained_delete`(`auto_split.rs` 끝, 계약 SPL-A10 행)가 요구를 모두 충족한다.

- **D>0 상태.** 7행을 채운 뒤(q 2..8) 명시 거래가 keys[0]을 DELETE한다. 이 Retained DELETE root가 D = 2,228−180 = 2,048을 만든다. tombstone(record 168B) 뒤 여유는 564+2,216−168 = 2,612다. 둘 다 probe로 확인했다.
- **경계 NotNeeded.** C 직접 호출이 정확히 경계인 L=380(380+184+2,048 = 2,612)에서 NotNeeded를 답한다. WAL file·Jobs·leaf는 불변이다. D를 크게 읽으면 이 단언이 깨진다.
- **1B 초과 split.** 같은 거래가 L=381 행(`cut(7, 372)`, 4+5+372)을 INSERT한다.
  - 값 검사는 565 ≤ 2,612로 통과한다. 따라서 split은 Reserve 발동으로만 일어날 수 있다. probe도 `stage=Reserve`다.
  - Reserve는 565+2,048 > 2,612다. C 필요 검산이 short이므로 RootSplit하고 Done 1이 된다.
  - D를 0으로 읽으면 NotNeeded가 되어 INSERT가 Reserve Capacity로 실패한다. 그래서 1B 위아래로 D 배선이 양쪽 모두 고정된다.
- **상태 기계 사실.** leaf 2개와 key 집합(tombstone key 포함), Commit (9, 2), drain 뒤 Row Job Free, 새 statement read(keys[1..]와 새 행), 재시작 (9,0,0)과 같은 행을 단언한다.

**서술 정정.**

- `auto_split.rs` 머리 주석(5–10행)은 이제 "INSERT root는 D=0, Reserve 발동은 D>0(SPL-A10)"으로 맞다.
- SPL-A03 주석(369–372행)과 계약 SPL-A03 행은 "8번째 행의 값 검사, D를 만들지 않는 INSERT root 7개 옆"으로 probe 결과와 맞다.
- 계약 발동 2(965–970행)에 D>0 조건을 적었다.

## 2회차 선택의 상태

| # | 내용 | 상태 |
| --- | --- | --- |
| 1 | SPL-A05가 A의 split flush를 직접 단언 | 미작성. 계약 1104행 "남은 D2 선택"에 명시. 2회차 probe로 동작은 확인됨 |
| 2 | C census의 빠진 slot | **반영.** `Result<Plan4Target>`·`Result<AnchorRoot>`·`Option<TargetRequest>`를 더해 residency가 36,692에서 36,900이 됨(+208, 2회차 추정 약 210). peak는 effect라 room 722 그대로 |
| 3 | A `refuse_frame` 이중 계수 | **반영.** 보수적 계수라고 주석에 명시(`driver.rs:1985-1990`) |
| 4 | 문안 1078행에 `split_leaf` 결과 slot | **반영** |
| 5 | 07 편차의 범위와 기록 | **반영.** `dev3-row-update.md`의 RUP-DEV01(anchor `known-deviation-validate-capacity`): 같은 크기·줄어드는 UPDATE와 증가분+D가 들어가는 UPDATE, 안전 영향 없음, 수정 방향, 담당, 기한(08-4 전). tree-smo는 링크와 비용 문구로 정정 |
| 6 | NotNeeded·부채 답의 비용, SQL 대응 | **반영**(계약 989–992행) |
| 7 | 최소 pool 경계 시험 | 미작성. 남은 선택으로 명시 |
| 8 | FlushRequired 왕복 사이 경합 | 미작성. 남은 선택으로 명시 |
| 9 | split만 있는 상태의 재시작 leaf 수 | 미작성. 남은 선택으로 명시 |
| 10 | 발동·split 종류 직접 관측 | 부분. 발동은 SPL-A10이 산술로 Reserve를 강제한다. A01..A04의 split 종류 관측은 남은 선택으로 명시 |
| 11 | README | **반영.** 현재 작업 문단 갱신, 띄어쓰기 정정 |
| 12 | plan4_apply.rs 주석 위치 | **반영** |
| 13 | module-gate 기록 | **반영.** commit에 포함, SPL-A10 목록 있음. `reviews`는 비어 있으며 이 판정으로 채운다 |
| 14 | 18-frame pool의 table issue MissingTarget | 부분. 계약 1106–1107행에 "C table issue의 작은 pool 한계로 남긴다"고 기록. 담당 이름은 없음(선택) |

## 새 지적

- **필수: 없음.**
- **선택 A.** README 문단의 작업 branch 표기 `claude/project-thread-e9itre`(main `2102038`)가 검토 대상 worktree branch `autosplit-impl`과 다르다. 실제로 push할 PR branch 이름에 맞춘다.
- **선택 B.** 남은 선택 7–10·14는 계약에 미작성 D2 선택으로 적혀 있다. 08-1 적립(완료) 판정 전에 처리하거나 그때 다시 판단하면 된다. 이번 반영을 막지 않는다.

## 판정

**ACCEPT.** 열린 필수 0건.

- 1차 필수 2건(F4 예산, A literal 시험)과 2회차 필수 1건(D>0 Reserve 경로의 실제 C 증거, 시험 서술)이 모두 해소됐다.
- 설계 변경 (a) 값 검사 Capacity 발동과 (b) 필요 검산 뒤 부채 검사·`Stage::Split` Unsupported는 2회차대로 받아들인다.


# 2회차 (3204cb4, 3회차로 대체됨)


- 대상: `git diff 2102038 3204cb4`(worktree `/home/claude/xadb-impl`, branch `autosplit-impl`). 계약은 `devdoc/contracts/dev3-tree-smo.md` 947–1097행 `#automatic-split`이다.
- 1차 보고: `review.md`(계약 초안만 보고 REJECT, 필수 2건과 선택 15건).
- 저장소 파일은 고치지 않았고 commit·push하지 않았다. 작업 트리의 미추적 파일 `module-gate/records/20261105-dev3-08-1-auto-split.json`은 검토 시작 전(12:46)부터 있던 작성자 파일이다.
- 방법은 넷이다.
  1. 코드를 읽었다.
  2. 공유 target에서 표적 시험을 돌렸다: SPL-A07·A08·A09, MCA00-D04·D05·D12·D15(memory), `f4_a_*`, `rup_a08_*`.
  3. `git archive 3204cb4`로 만든 scratch 사본(`scratchpad/review-autosplit/r2src`, 별도 target `tgt`)에 `eprintln!` probe를 넣었다. 그 사본에서 live SPL-A01..A06과 검토자 probe 시험 하나를 돌렸다.
  4. `-Zprint-type-sizes`(`RUSTC_BOOTSTRAP=1`, 별도 target `tsz2`)로 type 크기를 쟀다.
- 결론
  - 1차 필수 2건은 해소됐다.
  - 발동 판정·진행성·필요 검산·대상 선택·look-ahead·거래 의미·F4 수치는 맞다.
  - live SPL-A 시험은 모두 A의 값 검사 경로로만 발동한다. Reserve Capacity 경로(D>0)는 실제 C 위의 증거가 없다. 그런데 계약 표와 시험 주석은 SPL-A03이 그 경로를 덮는 것처럼 틀리게 적었다(필수 1).
  - **필수 1건, 선택 14건. 판정 CONDITIONAL.**

## 측정값 (근거)

| 항목 | 값 |
| --- | --- |
| SPL-A08 (MEMORY·NATIVE_TYPE 동일) | carriers 1,352 / residency 36,692 / effect 37,438 / peak 37,438. 원 effect entry 2,392·peak 37,550. 37,438 + 6,144 = 43,582 ≤ 44,304, **room 722** (계약 문안과 일치) |
| A executor autocommit/explicit | insert.preflight 5,716/5,483 (여유 428/661), insert.split 5,388/5,155 (756/989), update.preflight 5,665/5,432 (479/712), update.split 5,161/4,928 (983/1,216), insert.validate_finish 7,670/7,437 (Write 11,520). 모두 계약 문안과 일치 |
| MCA00-D12 | live_insert 11,644 / 12,288 (여유 644). 1차 11,427 대비 +217 |
| MCA00-D15 memory | insert.split share 3,304, frame_static 3,923 (6,144 대비). with_caller frame_static 5,219 (여유 925). insert.preflight with_caller frame_static 5,547 (여유 597) |
| A preflight 여유 소모 | autocommit insert 620 → 428 (+192B = split 상태 24B + `Result<bool, Failure>` 168B) |
| type size | `RowSplitStep` 64, `SplitResidency` 64, `Failure` 160, `RetentionError` 72, `TargetRequest`/`Plan4Target` 64, `AnchorRoot` 40, `RowRouteCursor` 56, `RowTree` 80, `Result<Observation>` 488(SPL-A08 식에서 역산) |
| probe: SPL-A01 | `note_full stage=Validate len=2048` → need D=0 free=564 → target at=0(RootSplit) → Split, leaves [3,5] |
| probe: SPL-A02 | Validate 발동 3회, 매번 D=0 free=564. RootSplit 1회(at=0), ChildSplit 2회(at=1, room [true,false]). leaves [3,3,3,6] |
| probe: SPL-A03 | Validate 발동 1회, **D=0 free=564**(leaf에 7행, 곧 8번째 행에서 발동). RootSplit |
| probe: SPL-A04 | Validate 발동, row=Some(1) old_cell=289 D=0 free=271 → RootSplit |
| probe: SPL-A05 | 직접 호출의 FlushRequired는 **victim look-ahead**에서 나옴. A INSERT는 Validate → look-ahead FlushRequired → `split_leaf` flush → Split |
| probe: SPL-A06 | 부채 table: need short → `check_split_debt` Unsupported. 109B는 NotNeeded. 정리 뒤 UPDATE(old_cell 2228)는 NotNeeded, 2048 INSERT는 Split |
| probe: 검토자 D>0 시험 | 7행 commit 뒤 명시 거래가 keys[0] DELETE, 같은 거래가 큰 행 INSERT. **`note_full stage=Reserve`, need D=2048 free=2612 short**, RootSplit → Done 1 → Commit (9, 2), leaves [3,5] |

---

## 0. 1차 지적의 반영

**[1차 필수 1] F4 예산 실현성: 해소.**

- (a) `RowSplitStep::Split`은 관측을 싣지 않는다(`structural_live.rs:28-38`). `RowSplitStep`은 64B다.
- (b) split 호출과 그 flush는 never-inline `split_leaf`(`driver.rs:1170`)에만 있다. 바깥 frame은 `full`과 `splits`만 더 쥔다(`driver.rs:609`, 948).
- (c) census 계획도 갖췄다.
  - `budget_tests.rs:172`에 명시 arm `insert.split|update.split => ROW_SPLIT_CALLER_BYTES`가 있다.
  - `CALL_SITES` 62(`driver.rs:1765`)에 새 지점 8개(split·split_try_flush·split_continue_flush·validate_finish × INSERT/UPDATE)가 들어갔다.
  - D15 `bound(Bound::Split) == 2`(`tests.rs:3102`), D12 재통과, C 단계별 census(`structural_live.rs:384-472`, SPL-A08)를 확인했다.
  - 측정값은 위 표와 같고 계약 1067–1081행과 숫자가 일치한다.

**[1차 필수 2] A literal 시험: 해소.**

- `spl_a09_row_loop_splits_and_retries_the_same_row`(`tests.rs:2073`)가 다음을 모두 고정한다.
  - 값 검사 Capacity와 Reserve Capacity의 발동(L=22)
  - Split 뒤 같은 RowId 7·같은 input·proposal로 재시도
  - FlushRequired flush 뒤 같은 split(미계수, 16 split 사이 16 flush, Split 32회·Reserve 17회)
  - NotNeeded에서 원 실패(Stage·cause·effect)
  - Unsupported에서 `Stage::Split`, Fatal에서 `must_resolve`·MustResolve
  - split flush 거절의 `Stage::Flush`
  - 16회 상한(Split 16·Reserve 17, 마지막 FinishPreflight)
  - 뒤 행의 split과 RollbackRequired
  - 재시도의 preflight Busy(`full` None, split 미요청)
  - 비발동: Preflight Capacity, Reserve Busy·Unsupported, finish 거절, Fatal finish
- MCA00-D04·D05의 호출 기록 갱신(`tests.rs:1579-1595`, 1671-1678)도 맞다.

**1차 선택의 반영.** 1·2·3·4·5·6(문안)·7·9·10·14·15는 반영됐다. 8은 다른 방식으로 반영했고 아래 4절에서 판단한다. 11·13은 남았다(선택 8·9). 12는 순수 함수로만 반영됐고 live D>0 쪽은 필수 1이 된다.

## 1. 발동 정밀도와 진행성: 맞다

**(a) 값 검사 Capacity의 발동은 건전하다.**

- validate 경로에서 맨 `InsertError::Capacity`가 나오는 곳은 사실상 `encode_input`(`row/insert.rs:196-197`) 하나다.
  - 이 지점은 `canonical_row_len_borrowed`가 schema·값 한도를 통과하고 `INLINE_ROW_BYTES` 검사도 지난 뒤다. 따라서 `canonical_values_len(values)`(`row_record.rs:227`)는 그 길이, 곧 Reserve 경로의 `encoded_len`과 같다(`spl_canonical_values_len_is_the_literal_length`).
  - 다른 출처는 `leaf::inspect`의 free underflow(`row/insert/leaf.rs:81-83`)뿐이다. C preflight의 `leaf_free_bytes`(`row_preflight.rs:1535-1545`)가 같은 합을 먼저 Invariant로 거절하므로 도달할 수 없다.
  - `From<…> for InsertError`에 Capacity로 바뀌는 변환은 없다.
- INSERT에서 값 검사는 D 없이 `L+184 > free`를 보고, Reserve는 D를 더해 본다. 그래서 값 검사가 모자라면 Reserve도 모자란다. 이 경로의 split 요청은 낭비가 없다.
- UPDATE에서는 값 검사가 07 규칙보다 엄격하다. 그래서 C가 NotNeeded를 낼 수 있다(선택 5).

**(b) Reserve Capacity의 발동도 정확하다.**

- `row_capacity`(`plan4_apply.rs:367`)는 `Unchanged ∧ Prepare ∧ RetentionCause::Capacity`만 받는다. Frame(Capacity)·다른 phase·CallerMustResolve는 받지 않는다(`spl_row_capacity_is_exactly_the_unchanged_prepare_capacity`).
- `note_full`(`driver.rs:795`)은 finish가 cleanup 오류 없이 끝나고 `must_resolve`가 아닐 때만 `full`을 채운다.

**진행성.**

- `full.take()` 뒤 `splits >= 16`이면 원 실패로 끝난다(`driver.rs:1176-1181`). Split만 센다.
- FlushRequired는 매번 dirty page 하나를 clean으로 만들고, LOAD·look-ahead는 page를 dirty로 만들지 않는다. 그래서 dirty 수로 묶인다.
- `ROUTE` 16과 `depth + 1 >= ROUTE`의 Invariant(`structural_live.rs:150`)는 root level ≤15(`RowRouteCursor::start`, `row_preflight.rs:465`)와 맞는다.
- probe로 보면 실제 split 횟수는 행당 1회다.

## 2. 필요 검산, routing key, UPDATE 대상: 맞다

- **공간 규칙.** `leaf_short`(`row_write.rs:327-340`)를 `check_candidate`와 필요 검산이 같은 함수로 쓴다. 세 입력도 Reserve와 같은 원천이다.
  - free: `leaf_free_bytes`. preflight cut의 `leaf_free`와 같은 계산이다.
  - D: `row_group::leaf_reservation(control.jobs(), namespace, handle, (leaf bytes, leaf identity))`. `reserve_sources`(`row_write.rs:1234-1239`)와 인자 모양이 같다.
  - old cell: `row_cell`(`structural_live.rs:324-353`). `delete_input`과 같은 `value.len() + 12`이고, 없으면 None이라 NotNeeded가 된다.
- **INSERT key.** `next_row_key`(`structural_live.rs:217-244`)는 preflight `row_shape`(`row_preflight.rs:1032-1047`)와 같은 Root `row_high+1`이다. overflow도 똑같이 Unchanged Capacity다. Root의 `row_high`는 split으로 바뀌지 않으므로 재시도의 RowId가 같다(literal에서 RowId 7).
- **UPDATE.** 대상 RowId로 route한다. leaf에 없으면 NotNeeded다(문안 999–1000행과 일치).

## 3. look-ahead와 producer target의 동치: 맞다

- producer는 `structural_candidates(&init[..count], 1, &prepared.requests)`를 부른다(`structural_effect.rs:415`). requests는 observed.targets 5개다.
  - ChildSplit은 [0] allocator, [1] anchor, [2] 대상, [3] INIT, [4] 부모다.
  - RootSplit은 [0..3]에 allocator·anchor·root, [3]·[4]가 INIT다(`structural_prepare.rs:296-297`, `read_split_before`의 roles).
- look-ahead(`structural_residency.rs:82-105`)는 required = allocator + set(`structural_live.rs:185-189`)으로 같은 `retention_candidate_at`을 같은 순서(first, 그다음 `first.next`에서 `first.index` 제외)로 부른다.
- INIT page는 Free page라 등록된 frame이 없다. 그래서 required에서 빠져도 keep 판정이 같다.
- clean/dirty 결과는 `start`에 의존하지 않는다. dirty victim의 선택은 rec_lsn 최소다.
- 단위 시험이 같은 victim(`structural_residency_tests.rs:145-150`), 깨끗한 경우, Busy를 대조한다. live probe(SPL-A05)도 look-ahead가 FlushRequired를 냄을 보였다.

## 4. 부채 검사 위치(설계 변경 b): 받아들인다

- **배치.** `check_split_debt`는 필요 검산 바로 뒤, 대상 선택·split 집합 LOAD 앞에 있다(`structural_live.rs:168-171`). `split_row_admitted`의 준비 cut에서 한 번 더 한다(`structural_effect.rs:374`). `split_row`(SPL-J05)의 의미는 그대로다.
- **이 선택이 맞는 이유는 셋이다.**
  1. 필요 없는 행(WAL 상한의 Reserve Capacity, 07 편차의 UPDATE)은 부채가 있어도 원 실패를 유지한다. 그래서 부채가 다른 원인을 가리지 않는다.
  2. 필요한 행은 canonical 준비 없이 거절된다. 1차 선택 8의 첫 우려(매번 전체 준비)가 해소됐다.
  3. `Stage::Split` Unsupported는 "split이 필요하지만 이 table에서는 10-1 전까지 할 수 없다"는 실제 원인을 그대로 보인다. 원 Capacity를 남기면 split이 없는 엔진처럼 보여, 사용자가 오히려 오해할 수 있다.
- 둘 다 무효과다. driver의 `Stage`를 쓰는 제품 소비자도 아직 없다(`Stage::` 사용처는 driver와 시험뿐이다). MCA05-V30 기대를 날짜와 이전 기대를 적어 바꾼 것도 절차에 맞다.
- **남는 비용.** 부채 table의 찬 leaf INSERT는 거절 전에 다음을 다 치른다. 계약은 이 비용을 Split의 비용으로만 적었다(선택 6).
  - shell(`close_reads_for_retirement`가 모든 owner의 admitted read를 기다림, `read.rs:177-190`)
  - `catalog_progress` admission
  - anchor·Root·route·leaf LOAD와 그 FlushRequired flush

## 5. LOAD 보호와 작은 pool: 맞다. 최소 pool 주장은 시험이 없다

- 보호 집합은 다음과 같다.
  - route LOAD는 [anchor, 그 page]다(`structural_live.rs:129-166`). 1차 선택 6대로다.
  - split 집합 LOAD는 set 전체를 서로 보호한다(176-180행).
- 단계 1과 2 사이에는 다른 LOAD가 없다. `read_split_before`의 `reserve_target_set`은 상주를 요구할 뿐 frame을 새로 쓰지 않는다. 그래서 look-ahead 판정이 producer 선택까지 유지된다.
- 1016–1017행의 "최소 pool = 집합 + INIT, 그보다 작으면 원 무효과 거절"은 시험이 없다(선택 7).

## 6. 거래·rollback·복구: 맞다

- **split의 위치.** split은 행 사이(Reserve가 Before로 실패하고 finish된 뒤)에서만 일어난다. A의 RootReserved는 없고, owner는 Idle이다.
- **statement view와 applied 수.** split은 statement view를 받지 않고, revision은 Forward만 센다. applied 수도 바뀌지 않는다(literal에서 뒤 행의 split과 RollbackRequired).
- **rollback 뒤의 split.** SPL-A03 rollback 뒤 split은 남고 두 leaf는 빈다. 재시작 read도 맞다.
- **loser가 split을 건너는 복구.** 기존 SPL-J04가 덮는다.
- **split 뒤·행 전 crash.** auto-split 경로로는 시험이 없다(선택 9).

## 7. F4 census: 수치는 맞다. inventory에 작은 누락이 있다

**A 쪽(`call_sites`)은 관례에 맞거나 보수적이다.**

- `splitting`(`driver.rs:1916`)은 statement, rows_state(시도의 Failure를 쥔 결과 slot 포함), `split_leaf` 결과 slot·인자·길이를 센다. forward는 C 결과 두 번에 `split_forward`를 더한다.
- `split_frame`(2020행)은 SplitNext 결과, target, flush 결과를 센다.
- `refusing`(1900행)과 `refuse_frame`(1988행)은 보수적이다.
  - `refuse_frame`은 P를 다시 센다. 관례는 insert_row의 frame에서 한 번만 센다.
  - `validate_finish`는 `finish_preflight`의 Failure 인자를 `refusing`의 인자와 따로 한 번 더 센다.
  - 넘치게 세므로 안전하지만, 주석과 식이 어긋난다(선택 3).

**C 쪽(`source_bytes`)은 `split_resident` frame 목록(403–419행)에 빠진 slot이 있다.**

| 빠진 slot | 위치 | 크기 |
| --- | --- | --- |
| `target_from_source` 결과 `Result<Plan4Target>` | 135행 | 약 72B |
| `table_root` 결과 `Result<AnchorRoot>` | 144행 | 약 72B |
| `route.internal` 결과 `Option<TargetRequest>` | 149행 | 64B |

합해 약 210B다. room 722B 안이므로 결론은 바뀌지 않는다(선택 2).

## 8. 시험 적정성: 필수 1

**SPL-A01..A06은 상태 기계 사실을 단언한다.** commit q, leaf별 key 집합, 새 statement read, 재시작 read, WAL file·Jobs·leaf 불변을 본다. 같은 producer가 다시 읽은 값으로 기대를 만들지 않는다.

**SPL-A05는 실제로 flush 경로를 탄다(probe로 확인).** 직접 호출의 FlushRequired는 victim look-ahead에서 나온다. A INSERT의 `split_leaf`도 FlushRequired를 받아 flush한 뒤 Split한다. 다만 시험 자체는 이를 단언하지 않는다. "같은 상태"라는 추론과 commit 결과에만 기댄다(선택 1).

**[필수 1] Reserve Capacity 경로(D>0)의 실제 C 증거가 없고, 계약 표와 시험 주석이 그 경로를 덮는 것처럼 틀리게 적었다.**

- **사실.** Retained INSERT root는 D에 아무것도 더하지 않는다(`semantic_row/transition/cells.rs:500-509`, "A Retained INSERT root restores nothing"). 따라서 INSERT만 쓰는 SPL-A01·A02·A03·A05는 늘 D=0이다.
  - D=0이면 찬 leaf는 값 검사가 먼저 잡는다. probe에서 여섯 live 시험의 발동은 모두 `stage=Validate`, `D=0`이었다.
  - Reserve의 leaf Capacity는 D>0, 곧 같은 leaf에 Retained DELETE·축소 UPDATE root가 있을 때만 난다. 이것이 발동 조건 2(계약 962–966행)의 leaf 쪽 전부다.
  - 그런데 이 경로를 실제 C 위에서 지나가는 시험이 하나도 없다. 남는 것은 A literal(SPL-A09, C는 stand-in)과 순수 함수 `spl_leaf_short`뿐이다.
  - 그래서 필요 검산이 jobs에서 그 leaf의 D를 읽는 배선(`leaf_needs_split`, `structural_live.rs:297-303`)이 시험되지 않는다. D>0 leaf의 split 뒤 재시도도, 진행성 문안의 "split마다 D가 준다"도 마찬가지다. 배선이 틀려 D를 0으로 읽으면 Reserve Capacity 행이 늘 NotNeeded로 끝난다. 그래도 지금 어느 시험도 실패하지 않는다.
- **틀린 문안.**
  - 계약 1087행(SPL-A03): "큰 행 8개가 7번째에서 split(거래의 Retained root 6개 옆)". 실제로는 7행이 들어가고 8번째 행의 값 검사에서 split한다(probe: free=564, D=0). 옆의 Retained INSERT root 7개는 D를 만들지 않는다.
  - 시험 주석 `auto_split.rs:6-8`: "an uncommitted row takes more: one statement's seventh row no longer fits".
  - 시험 주석 `auto_split.rs:368-371`: "splits T's leaf at its seventh row (a RootSplit beside the transaction's six Retained INSERT roots)".
  - D2 기록인 계약 시험표가 없는 증거를 있다고 적은 상태다.
- **동작 자체는 맞다(검토자 probe).** 7행 commit 뒤 명시 거래가 keys[0]을 DELETE하면 D=2048, free=2612다. 같은 거래의 큰 행 INSERT는 값 검사를 통과하고 Reserve에서 Capacity를 받는다. C 필요 검산은 short이고 RootSplit 뒤 Done 1, Commit (9, 2), leaves [3,5]다(`r2src/.../auto_split.rs` 끝의 `r2_probe_reserve_capacity_with_d`).
- **고칠 것.**
  - (a) D>0 live 시험을 SPL-A10 같은 번호로 추가한다. 위 probe 모양이면 된다. 가능하면 같은 상태에서 C 직접 호출로 `필요+D = 여유`이면 NotNeeded, 1B 초과면 Split을 확인하고, 재시작 read도 둔다.
  - (b) SPL-A03 표 행과 `auto_split.rs`의 두 주석을 사실대로 고친다: 8번째 행, 값 검사 발동, INSERT root는 D=0.
  - (c) 계약 시험표에 "Reserve Capacity(leaf) 발동은 D>0일 때만이며 SPL-A10이 덮는다"를 적는다.

## 9. 계약 문안 대조

다음 문안은 코드와 맞는다.

- 이름: `split_row_live`·`split_row_tree`·`split_live_admitted`·`split_resident`·`leaf_needs_split`·`check_split_debt`·`search::separator_room`·`split_target`·`structural_flush_victim`·`refuse_validation`·`note_full`·`split_leaf`·`MAX_ROW_SPLITS`·`Stage::Split`·`Bound::Split`·`ROW_SPLIT_CALLER_BYTES`
- 수치: 6,144 / 1,352 / 36,692 / 37,438 / 43,582 / 722 / 62 / A 지점 여덟 값 / 11,644
- 경계: separator 경계 len 404, `TreePageView::len`의 metadata 제외(`tree_page.rs:96-98`)
- stage 이름: `Stage::Flush`/`ContinueFlush`(`driver.rs:1137-1146`)

불일치는 다음과 같다.

- SPL-A03(필수 1).
- 1078행 "바깥 행 frame은 `full`과 횟수, 그 시도의 결과만 쥐고": `split_leaf`의 결과 slot `Result<bool, Failure>`(168B)도 statement frame에 있다. census는 이를 센다(선택 4).
- 1053–1057행의 07 편차 범위를 좁게 적었다(선택 5).

---

## 선택

1. **SPL-A05 자기 단언.** A의 `split_leaf`가 look-ahead의 FlushRequired를 받아 `split_try_flush`를 불렀음을 시험이 직접 단언하도록 test hook이나 계수를 둔다. 지금은 probe로만 확인된다.
2. **C census 누락 slot.** `split_resident`의 `Result<Plan4Target>`(135행), `Result<AnchorRoot>`(144행), `Option<TargetRequest>`(149행)를 `source_bytes` frame(403–419행)에 넣는다. 아니면 `[usize; 8]`·참조 여유가 덮는다고 적는다. 약 210B이고 room 722 안이다.
3. **A census 주석과 식 일치.** `refuse_frame`(`driver.rs:1988`)의 `size_of::<P<B>>()`와 `validate_finish`의 Failure 이중 계수를 정리한다. 넘치게 세므로 안전하다. 지우거나, 보수적으로 남기는 이유를 주석에 적는다.
4. **문안 1078행.** 바깥 frame 구성에 `split_leaf` 결과 slot(`Result<bool, Failure>`)을 더한다.
5. **07 편차의 범위와 기록.**
   - 범위: `encode_input`은 UPDATE에도 `L+184 > free`를 본다. 그래서 같은 크기 UPDATE만이 아니다. 줄어드는 UPDATE와 `growth+D ≤ free`인 커지는 UPDATE도 꽉 찬 leaf에서는 `Stage::Validate` Capacity로 거절된다. 이들은 C에서 NotNeeded가 된다.
   - 비용: 이제 이 행들은 거절 전에 shell(read gate 대기)과 LOAD를 치른다.
   - 기록: 소유 계약인 `dev3-row-update.md`에 ID·담당·수정 일정을 가진 편차로 기록한다. tree-smo의 "별도로 다룬다"만으로는 추적되지 않는다.
6. **부채 table과 NotNeeded의 비용(설계 b).** 계약 1026행 부근의 "매 split은 reads를 닫고 fsync" 옆에 한 줄을 더한다. NotNeeded·부채 Unsupported 답도 shell(모든 owner의 admitted read 대기), `catalog_progress`, route LOAD와 그 FlushRequired flush를 치른다는 내용이다. 앞으로 SQL 계층에서 `Stage::Split` Unsupported를 "split 필요, 이 table은 10-1 전 불가"로 옮기도록 사용자 오류 대응도 적어 둔다.
7. **최소 pool 주장(1016–1017행).** 집합+INIT 정확히 그만큼과 하나 적은 pool의 시험(SPL-J07 fixture 응용)을 두거나, 미검증이라고 적는다.
8. **1차 선택 11(경합).** FlushRequired 왕복 사이에 victim이 다시 dirty가 되면 `Stage::Split` Busy·무효과임을 보이는 시험이 아직 없다.
9. **1차 선택 13(split 뒤 crash).** auto-split의 split만 있고 행이 없는 상태(SPL-A06 마지막 단계)를 재시작해 두 leaf와 행 집합을 확인한다. `join_restart_with`는 행만 비교하므로 leaf 수 단언을 더한다.
10. **발동·split 종류 단언.** SPL-A01..A04는 발동 경로(값 검사/Reserve)와 split 종류(RootSplit/ChildSplit)를 leaf 수로만 간접 확인한다. 계약 표가 적는 "값 검사가 Capacity", "RootSplit 한 번", "RootSplit과 ChildSplit"을 시험이 직접 고정하도록 관측을 하나 둔다. 예: 시험용 계수, 또는 anchor root level과 page 변화.
11. **README 갱신.** `devdoc/README.md:17`의 "A의 자동 split 호출자는08-1 잔여"는 이제 사실이 아니다. 띄어쓰기도 "08-1 잔여라 적립은 0"으로 고친다.
12. **주석 위치.** `plan4_apply.rs:149-150`의 "The live split step … for A's row loop" 주석이 `ReservedCatalogCreate` use 앞에 붙어 있다. 157행 `pub(in crate::storage) use row::RowSplitStep;` 앞으로 옮긴다.
13. **module-gate 기록.** `module-gate/records/20261105-dev3-08-1-auto-split.json`이 미추적이다. AGENTS.md 57행과 development-policy.md 44행에 따라 PR에 함께 commit한다. 기록은 "필수 지적 0"을 요구하므로 필수 1 해소 뒤 확정한다.
14. **18-frame 한계의 기록.** SPL-A05 주석은 split 뒤 새 statement의 table issue가 MissingTarget이라고 적는다. split이 작업 집합을 frame 하나 늘린다는 뜻이다. 시험 주석에만 두지 말고, table issue의 작은 pool 한계로 담당과 함께 기록한다.

---

## 필수 요약

1. **Reserve Capacity(D>0) 발동 경로의 실제 C 증거 부재와 틀린 시험 서술.**
   - INSERT root는 D=0이다. 그래서 live SPL-A 여섯 개는 모두 값 검사 경로로만 발동한다(probe 확인).
   - Reserve의 leaf Capacity는 Retained DELETE·축소 UPDATE root가 있을 때만 나는데, 이를 실제 C 위에서 지나가는 시험이 없다.
   - 계약 1087행(SPL-A03)과 `auto_split.rs:6-8`, 368-371행은 그 경로를 덮는 것처럼 틀리게 적었다.
   - 고칠 것: D>0 live 시험(검토자 probe 모양, NotNeeded/Split 경계 포함)을 추가하고, SPL-A03 행과 두 주석을 바로잡는다.
   - 검토자 probe로는 동작이 맞다. 결함은 증거와 문안이다.

판정: **CONDITIONAL** (필수 1건). 필수 1을 고치고 SPL-A10(가칭) 통과를 확인하면 ACCEPT로 본다. 설계 변경 (a) 값 검사 Capacity 발동과 (b) 필요 검산 뒤 부채 검사·`Stage::Split` Unsupported는 둘 다 받아들인다.


# 1회차 (계약 초안, 2회차로 대체됨)


- 대상: `devdoc/contracts/dev3-tree-smo.md` 947–1039행, 미커밋 절 `<a id="automatic-split"></a> ## A 자동 split 호출자`
- 기준: branch `claude/project-thread-e9itre`, HEAD = origin/main `2102038`. 저장소 파일은 고치지 않았고 커밋하지 않았다.
- 방법: 코드를 읽었다. 측정은 표적 `--nocapture` 시험의 출력(`STRUCTURAL_EFFECT_SOURCE`, `MCA00_D12`, `LIVE_ROW_RETIREMENT`, `S2W_C06`, A preflight 지점 census)과, 별도 scratch target dir에서 뽑은 `-Zprint-type-sizes`로 했다. 전체 suite는 돌리지 않았다.
- 결론: 발동 판정, 진행성, 대상 선택, admission, 거래 의미는 건전하다. 그러나 F4 예산은 계약 문안대로는 맞지 않는다(필수 1). A 행 loop의 결정 규칙을 고정하는 literal 시험도 없다(필수 2). **필수 2건, 선택 15건.**

## 측정값 (근거)

| 항목 | 값 |
| --- | --- |
| `STRUCTURAL_EFFECT_SOURCE` (memory·native W 동일) | entry 2,392 / prepared 36,500 / jobs 10,772 / selection 19,992 / **installation 37,550 (peak)** / replay 36,164 / returning 3,594 / charge **44,304** |
| A preflight 지점 (`ROW_PREFLIGHT_CALLER_BYTES` 6,144 대비) | autocommit insert 5,524 (**여유 620**) · autocommit update 5,473 (671) · explicit insert 5,291 (853) · explicit update 5,240 (904) |
| A `insert.check` autocommit | 4,820 |
| `MCA00_D12` live_insert path-wide | 11,427 / `DRIVER_SOURCE_BYTES` 12,288 (**여유 861**) |
| `LIVE_ROW_RETIREMENT` | row_retirement 39,564, caller 4,096에서 room 644. retirement 44,304 = charge |
| `S2W_C06` | preflight_body 37,384, C preflight 여유 184, reserve 여유 499 |
| type size | `Observation` 488, `Failure` 160, `RetentionError` 72, `Plan4Target` 64, `TargetRequest` 64, `PageRef` 24, `CheckedTable` 112, `RowStatementView` 168, `RowRetireStep` 296 |

---

## 1. 발동 정밀도와 loop 진행성

**발동 판정은 정확하다.**

Reserve 단계의 `Before`/`Unchanged(Prepare, Capacity)`가 생기는 곳은 둘뿐이다.

- `check_candidate`의 leaf 공간: `crates/engine/src/storage/wal_io/plan4_retention/row_write.rs:300-326`. Capacity는 323행이다.
- `check_progress_room`의 WAL 상한 또는 LSN overflow: `plan4_retention/catalog_live.rs:853-903`. Capacity는 870-878행이다. counter 부족은 Exhausted다.

나머지는 Capacity가 되지 않는다.

- producer 오류는 `row_start.rs:76-78`의 `row_semantic_error`를 거쳐 RowSemantic이다. structural 쪽도 `structural_prepare.rs:54-56`에서 같다.
- frame 오류는 `plan4_apply.rs:423-429`에서 `RetentionCause::Frame(DirtyError)`가 된다.

`reserve_sources`는 `leaf_reservation`(row_write.rs:1220) 뒤에 `check_candidate`(1233)를 먼저 부르고, 그 뒤에야 `prepare_reserve`의 `check_progress_room`(1139)이 돈다. 그래서 같은 입력으로 같은 식을 다시 보면 두 원인이 정확히 갈린다. 957-958행의 서술과 일치한다.

A의 Reserve와 `split_row_live` 사이에 다른 owner가 같은 leaf를 바꿀 수 있지만, 어느 방향이든 안전하다.

- 그 사이 여유가 생기면 `NotNeeded`가 나오고 원 Capacity로 행을 끝낸다. 그 순간 기준으로는 정당한 거절이다.
- 더 차면 split 한 번이 더 일어날 뿐이다.

preflight의 Capacity는 `Stage::Preflight`이므로 발동 조건에서 빠진다. 해당 지점은 `row_preflight.rs:605`(Free Undo 없음)와 1044행(`row_high` overflow)이다. 다만 이를 고정하는 시험은 없다(필수 2).

**진행성도 성립한다.**

- 최대 행 하나는 `INLINE_ROW_BYTES` 2048(`overflow.rs:17`) + `ROW_HEADER_BYTES` 168(`row_record.rs:20`) + 16 = 2,232B다.
- 예약 D는 leaf 안 RowId의 RootRetained restore 합이므로 entry 수에 비례해 줄어든다. entry 1개 leaf에서는 D가 대략 2,052B 이하다.
- 따라서 entry 1개 leaf는 `필요+D+기존 ≤ 16,188`을 언제나 만족한다. `NotSplittable`(`tree_page/structure.rs:114`, `len < 2`)에 닿기 전에 `NotNeeded`가 먼저 나온다.
- `MAX_LEAF_ROWS = 16188/184 = 87`(`semantic_row/structure/candidate.rs:36`)이다. weighted split은 key 쪽 leaf의 entry와 D를 매번 줄이므로 leaf 쪽은 약 7회로 수렴한다.
- 부모가 차서 위쪽부터 나누는 cascade는 깊이로 묶인다(level 15에서 거절, candidate.rs:448). 실제 첫 extent(64 page)에서는 깊이가 2 이하다. 16회 상한은 충분하다.
- `FlushRequired`의 반복은 매번 dirty page 하나를 clean으로 만든다(`row/insert/driver.rs` flush 884-896). 그래서 dirty 수로 묶이고, 세지 않아도 끝난다.

- **[선택 1]** C가 `length`를 `MIN_CANONICAL..=INLINE_ROW_BYTES`로 검증해야 한다. A의 발동 match는 `Before` + `Unchanged(Prepare, RetentionCause::Capacity)`의 정확한 형태여야 하고 `_`나 Frame(Capacity)를 포함하지 않는다고 문안에 적는다.
- **[선택 2]** 16을 `MAX_ROW_SPLITS` 같은 이름 있는 상수로 둔다. 위의 진행성 논거(1-entry leaf에서는 항상 NotNeeded, 깊이 ≤15)를 한 문장으로 계약에 남긴다.

## 2. 대상 선택과 공간 식

**대상 선택은 producer와 일치한다.**

- Row internal cell은 `4+8+24 = 36`(`tree_page.rs:463`)에 slot 4(`tree_page.rs:358`)를 더해 40B다. 식 `(len+1)·40 ≤ TREE_ENTRY_CAPACITY(16,188)`(`tree_page.rs:22-26`)는 producer의 `ROW_REFS = 16188/40 + 2`(candidate.rs:37), 그리고 parent 편집 뒤 `needs_split() → Exhausted`(candidate.rs:543, 805)와 같은 byte 기준이다.
- `len`은 `TreePageView`의 entry 수다. metadata entry를 포함해야 producer와 같아진다.
- 첫 extent(64 page)에서 부모의 child 수는 64 이하다(`SPLIT_REFS = 88`, candidate.rs:41-44 주석). 그래서 "부모가 참" 분기는 실제로는 닿지 않고 SPL-A07의 순수 함수 시험으로만 덮인다. 이 점은 문제가 아니지만 계약에 적어 두는 편이 맞다.
- **[선택 3, 문안]** 996-997행의 "첫 extent의 Free page가 모자라면 producer의 Capacity"는 틀렸다. 코드는 `selected != needed → Error::Exhausted`(candidate.rs:494)이고 C에서 RowSemantic(Exhausted)이 된다. level 15의 RootSplit도 Exhausted(448)다. A의 `Stage::Split` 처리에는 영향이 없지만, 사용자에게 보이는 원인과 SPL 시험의 기대가 달라진다. "producer의 Exhausted(RowSemantic)"로 고친다.
- **[선택 4]** 부모 판정이 `TreePageView`의 entry 수를 쓴다는 것과, metadata entry 포함 여부를 명시한다. SPL-A07의 경계값에 `(len+1)·40 = 16,188`(정확히 맞음)과 +1을 넣는다.

## 3. 거주, 사전 검사의 동치, 경합

**gate 아래 LOAD는 선례가 있다.**

- retire의 `row_cleanup_resident`(`row_start/retire.rs:372-435`)가 LOAD + route + `FlushRequired`를 같은 구조로 한다.
- shell 안에서는 `MutationPort`가 배타이므로 다른 owner가 LOAD하거나 evict할 수 없다.
- 경합은 `FlushRequired`를 A로 돌려주고 다시 부르는 왕복 사이에서만 생긴다. 그때 단계 1은 처음부터 다시 돈다.

**사전 검사는 조건부로 동치다.**

- producer의 `structural_candidates`(`dirty_frame/structural_residency.rs:29-71`)는 `retention_candidate_at(Some(init), start, targets, 5, None)`(55행)을 부른다. 두 번째(61행)는 `first.next`에서 시작해 `first.index`를 제외한다.
- `retention_candidate_at`(`dirty_frame/retention_residency.rs:395-529`)의 clean/dirty 판정은 `start`에 의존하지 않는다. kept = excluded 또는 registered ∈ required(480), 반환은 `resident.or(clean).or(dirty)`(523)다.
- 따라서 사전 검사가 보호하는 집합이 producer의 targets와 **정확히** 같을 때만, 즉 RootSplit이면 {allocator, anchor, root}이고 ChildSplit이면 {allocator, anchor, 대상, 부모}일 때만, 판정이 같다.
- 더 많이 보호하면(예: route 경로 전체) producer가 clean을 고를 자리에서 dirty를 보고 쓸데없는 `FlushRequired`를 낸다. flush 한 번이 더 생길 뿐 무해하다.
- 덜 보호하면 사전 검사는 clean이라 보고 통과하는데, producer는 dirty victim을 만난다. 그러면 `requires_flush → Busy`(`dirty_init/structural/live.rs:173`)나 `content.dirty → Busy`(290)가 나고, 행이 `Stage::Split` Busy로 거절된다. 사용자에게 보이는 거짓 실패다.

- **[선택 5]** 988-990행에 사전 검사의 required 집합을 위 둘로 명시한다. `retention_pair_room`(retention_residency.rs:298, `None`, required 7)처럼 `structural_candidates`와 **같은 함수**로 판정하도록 적는다.
- **[선택 6]** 976-979행의 "각 LOAD는 그때까지 고른 집합을 보호한다"는 route 경로 전체를 핀으로 쌓게 한다. 작은 pool(SPL-J07의 12-frame + 외부 pin 9개)에서는 불필요한 Capacity나 FlushRequired가 생긴다. route LOAD는 현재 page와 anchor만 보호하고 split 집합 LOAD에서만 서로 보호하도록 하자. 필요한 최소 frame 수(allocator·anchor·대상·부모 + INIT 2)와 그보다 작을 때의 오류(`reserve_target_capacity`의 Capacity)도 적는다.
- **[선택 7, 문안]** 994행 "단계 1과 2 사이에 다른 owner가 residency를 바꾸면"은 같은 port 호출 안이라 일어날 수 없다. "FlushRequired 왕복 사이의 변경은 단계 1이 다시 보며, 그래도 놓친 경우 2가 무효과 Busy로 거절한다"로 고친다. 가능하면 producer의 `requires_flush`도 Busy 대신 `FlushRequired`로 돌려줄 수 있는지 검토한다.

## 4. 거래 의미

**거래 의미는 안전하다.**

- **admission:** 행 사이에서 owner는 Idle이고 read operation이 없다. 그래서 `close_reads_for_retirement`(`transaction/concurrency/read.rs:126-131`)를 통과한다.
- **charge:** `catalog_progress`는 `progress(RowWrite)`와 같은 `CatalogProgressCharge` 44,304를 잡는다(`plan4_apply/source.rs:132-165`, 363-399). `begin_statement`의 CallerCredit도 긴 source를 쓴다(`plan4_apply/transaction.rs:823-900`).
- **RootReserved:** Reserve가 `Before`로 실패했으므로 A의 RootReserved는 없다. RootReserved는 한 C 호출 안에서만 존재하므로, canonical 준비의 "모든 RootReserved 거절"이 A 자신을 막지 않는다.
- **CheckedTable:** 안정 anchor `PageIdentity`만 쥔다(`plan4_apply/row.rs:67-74`). split 뒤에도 유효하다.
- **statement view:** revision은 Forward만 센다(transaction.rs:672-679). split은 statement view를 무효로 만들지 않는다. 문안의 "applied 수 불변"과도 맞는다.
- **복구:** redo-only이고, rollback·CLR·Prune은 current-leaf 규칙을 따른다. 새 WAL 형식이 없다는 서술도 맞다.

선택 사항은 다음과 같다.

- **[선택 8]** 1037-1039행이 인정하듯 제품 table은 모두 CREATE Catalog Job을 갖고 있어 10-1 전에는 자동 split이 Unsupported다. 그런데 그 판정은 `split_row_admitted`에서 `prepare_row_split`(`structural_effect.rs:311`)의 canonical 준비·validate_live가 끝난 뒤인 321-336행에서 나온다. 결과는 둘이다.
  1. 제품에서는 찬 leaf에 INSERT할 때마다 reads를 닫은 채 단계 1 LOAD와 전체 준비를 한 뒤 Unsupported가 된다.
  2. 사용자에게 보이는 오류가 오늘의 `Stage::Reserve` Capacity에서 `Stage::Split` Unsupported로 바뀐다.

  단계 1은 이미 control Jobs를 읽으므로(980행) non-Free Catalog Job을 거기서 먼저 보도록 하자. 이때 A가 낼 오류를 정하고 계약에 적는다. 원 Reserve Capacity를 유지하는 쪽이 사용자에게 덜 놀랍다.
- **[선택 9]** SPL-A01..A05가 통과하려면 시험 helper가 CREATE Job을 정리해야 한다(`plan4_recovery/tests/catalog_memory/row_join/retire/split_rows.rs:76-93`의 `retire_create!`). 시험표에 "시험 전용 정리 뒤"라고 적는다.
- **[선택 10]** D>0 때문에 생기는 split은 cleanup으로 D가 사라지면 불필요했던 것이 된다. 그런데 unlink가 없어 비가역이고, 첫 extent의 가용 page는 약 55개다. 열린 거래가 많은 부하에서 extent를 소모하는 위험을 적어 두자. 가능하면 "D>0이고 D 없이 들어가면 먼저 cleanup drain을 시도"하는 규칙을 검토한다.

## 5. 예산 실현성과 census 계획 — 필수 1

**C 쪽:**

- charge 44,304 − 기존 peak 37,550(installation) − `ROW_SPLIT_CALLER_BYTES` 6,144 = **610B**. 이것이 `split_row_live`가 `split_row` 본문 위에 더할 수 있는 frame과 carrier의 전부다.
- 기존 entry는 이미 `3 × Result<Observation>`을 센다(`structural_effect.rs:726-737`, 729행). 따라서 `split_row_admitted`의 결과는 포함되어 있다.
- 그러나 `RowSplitStep::Split(Observation)`을 담는 `Result<RowSplitStep>`은 약 496B(`Observation` 488 + tag)의 새 slot이다. 저장소 관례(by-value 결과는 callee와 forward 두 번, `plan4_apply/row/write.rs`의 `RowWritePhase` carrier 문서)로는 약 990B가 되어 **610B를 넘는다**.
- 한 번만 세도 남는 약 110B로 `CheckedTable` 참조, flusher, `Option<RowId>`, length, private, 단계 1이 넘기는 대상, never-inline 단계 frame을 담아야 한다. 현실적이지 않다.

**A 쪽:**

- **split 지점:** 비슷한 `insert.check` autocommit이 4,820B다. 여기에 바깥 행 frame(마지막 `Failure` 160, 횟수, L 등)과 `Result<RowSplitStep>` 두 slot(약 992)을 더하면 약 6.3–6.5KB로 **6,144를 넘는다**. Observation이 없으면 약 5.6KB로 들어간다.
- **preflight 지점:** frame-static 몫이다(`row/insert/driver.rs:1596-1650` 문서: frame의 모든 slot을 진입 시 센다). 바깥 행 frame이 `RowSplitStep` slot(약 496)과 보관 `Failure`(160)를 쥐면 그것만으로 656B다. 이는 autocommit insert의 여유 **620B를 넘는다**. 문안(1020-1022행)은 "다시 통과해야 한다"고만 할 뿐, 바깥 frame이 무엇을 쥘 수 있는지는 정하지 않는다.
- **D12:** live_insert path-wide 11,427 / 12,288(여유 861)에도 바깥 frame이 더해진다. 그런데 census 계획(1017-1024행, SPL-A08)은 이를 말하지 않는다.
- **budget_tests:** `row/insert/driver/budget_tests.rs:165-173`의 match는 이름이 없는 statement 지점을 기본 arm `_ => ROW_WRITE_CALLER_BYTES`(172행)로 묶는다. arm을 추가하지 않으면 `insert.split`/`update.split`은 6,144가 아닌 11,520으로 **조용히 통과한다**.
- **call site 목록:** `CALL_SITES = 54`(driver.rs:1593), `call_sites`(1653), `live_call_sites`에 새 지점(split, split의 flush)을 넣어야 한다. 계획에는 이 언급이 없다.
- **C 확인 방식:** 기존 structural census 시험은 `charge ≥ peak`만 보고 caller 항이 없다. `live_row_retirement_fits_the_progress_charge`(`plan4_apply/source_tests.rs:390`)처럼 `split_row_live`의 단계별 이름과 각 단계 + 6,144 ≤ 44,304를 두 writer에서 확인하는 새 census가 필요하다. SPL-A08이 결론은 적었지만 무엇을 단계로 세는지는 적지 않았다.

**[필수 1] 다음을 계약에 고정해야 한다.**

- (a) `RowSplitStep::Split`은 Observation을 싣지 않는다. A는 그 내용을 쓰지 않는다. 표의 `Split(Observation)`은 `Split`으로 바꾼다. Observation을 유지하려면 610B 안에 들어감을 보이는 명시적 inventory를 대신 적는다.
- (b) A에서 `split_row_live` 호출과 그 `FlushRequired`·flush 처리는 하나의 never-inline 단계에 둔다. 바깥 행 frame의 상한(예: 512B 이하, 횟수·L·보관 Failure만)을 적는다. 이것이 preflight 지점 여유 620B를 보장하는 조건이다.
- (c) census 계획에 다음을 더한다.
  - `budget_tests`의 `insert.split`/`update.split` 명시 arm(6,144)과 split-flush 지점의 이름·arm(11,520)
  - `CALL_SITES`·`call_sites`·`live_call_sites` 갱신
  - MCA00-D12 재통과
  - C의 단계별 census(기존 7단계 + `split_row_live` 거주 단계 + carrier, 두 writer)

## 6. 빠진 실패 모드, 시험, 문안 — 필수 2

SPL-A01..A06은 live·통합 시험이고, A07은 순수 함수, A08은 census다. A 행 loop의 결정 규칙을 Boundary stand-in(`LiteralCall`)으로 고정하는 driver 시험은 하나도 없다. live로는 Reserve Busy 같은 비대상 오류나 16회 상한을 만들 수 없으므로 literal 시험이 유일한 수단이다.

또한 기존 `mca00_d05_c_refusals_keep_busy_capacity_unsupported_distinct`(`row/insert/driver/tests.rs:1596`)는 Reserve(7) Capacity의 호출 기록을 `[.., Reserve(7), FinishPreflight]`로 고정한다(1617-1620행 부근). 새 계약에서는 그 뒤에 `SplitRowLive`가 붙거나 stand-in이 `NotNeeded`를 돌려야 하므로, 이 시험이 바뀐다. 문안은 이를 언급하지 않는다.

**[필수 2]** `driver/tests.rs`에 literal 시험을 SPL-A 표에 추가하고 D05 갱신을 명시한다. 최소 범위는 다음과 같다.

1. 발동은 Reserve `Before`/`Unchanged(Prepare, Capacity)`와 깨끗한 finish일 때만 일어난다. 다음 경우에는 `split_row_live`를 부르지 않는다.
   - Preflight Capacity
   - Reserve의 Busy·Unsupported·Frame(Capacity)·`After` 계열
   - cleanup 오류가 있는 finish
   - `must_resolve`
   - DELETE
2. 16회 상한: 매번 `Split`을 돌리는 stand-in이면 17번째 split은 부르지 않고 마지막 Reserve 실패(`Stage::Reserve` Capacity)를 돌려준다.
3. `FlushRequired`는 세지 않는다. 16회 `Split` 사이에 `FlushRequired`를 섞어도 16번째까지 split한다. split 안의 flush 실패는 정해진 Stage로 끝난다.
4. `NotNeeded`는 원 Reserve `Failure`를 그대로(같은 Stage·cause) 돌려준다.
5. split 거절은 `Stage::Split`과 그 cause로 끝난다. Fatal·CallerMustResolve는 `must_resolve`다. split 뒤 재시도가 성공하면 applied 수가 1이다.
6. MCA00-D05의 기대 호출 기록을 갱신한다.

선택 사항은 다음과 같다.

- **[선택 11]** live 경합 시험: `FlushRequired` 왕복 사이에 다른 owner가 victim을 dirty로 만들면 무효과 `Stage::Split` Busy가 나고 WAL·tree가 불변이다.
- **[선택 12]** `NotNeeded` 경계 시험: 공용 공간 함수로 `필요+D = 여유`(NotNeeded)와 1B 초과(split)를 INSERT·UPDATE 각각 확인한다.
- **[선택 13]** split 성공 뒤 재시도가 다른 이유(preflight Busy, WAL 상한 Capacity)로 실패하는 시험: 행은 그 오류로 끝나고 split은 durable로 남으며 재시작 뒤에도 두 leaf다.
- **[선택 14, 문안]** 1005행 "flush 실패는 그 `Stage`로"가 어느 Stage인지(기존 preflight flush의 Stage인지 `Stage::Split`인지) 명시한다. 매 split이 reads를 닫고 durable wait(fsync)를 한다는 비용도 한 줄 적는다.
- **[선택 15, 문안]** 띄어쓰기를 고친다: 953행 "아니며08-1", 1037행 "이 절로08-1을", 1039행 "(일정10-1의".

---

## 필수 요약

1. **F4 예산 실현성:** C 여유 610B에 `Split(Observation)` carrier(약 496B, 관례상 두 번)가 들어가지 않는다. A의 split 지점(약 6.3–6.5KB)과 preflight 지점(여유 620B)도 바깥 frame 구성에 따라 넘친다. Observation을 빼거나 inventory를 적고, split 호출을 별도 never-inline 단계에 두고, 바깥 frame 상한을 적고, census 계획(budget_tests arm, CALL_SITES, D12, C 단계별 census)을 완성한다.
2. **A literal driver 시험 부재:** 발동 정밀도, 16회 상한, FlushRequired 미계수, NotNeeded 통과, Stage::Split·must_resolve 대응, DELETE 비발동을 Boundary stand-in으로 고정한다. MCA00-D05 갱신을 명시한다.

판정: REJECT

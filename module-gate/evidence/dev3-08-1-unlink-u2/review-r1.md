Verdict: CONDITIONAL

# 독립 D2 검토(1회차): 빈 leaf unlink U2

## 대상

- commit: `1a82225` (local branch `u2-local`, tree `94a5e6b693c8528285f1ea0f3ea0660746e25e81`)
- parent: `98faf08` (U1 module gate record. U1은 `module-gate/evidence/dev3-08-1-unlink-u1/review.md`에서 2회차
  ACCEPT)
- 범위: `git diff 98faf08 1a82225`, 21개 파일, +1,234/−50
- 계약: `devdoc/contracts/dev3-tree-smo.md`의 `#empty-leaf-unlink`(1122행부터. 특히 "준비·효과·복구"의 복구와
  Base4 closure, 시험 계획, 구현 단계)와 새 `#unlink-u2` 절(1476–1542행)
- 검토자는 이 코드를 작성하지 않았다. `/home/claude/xadb`은 읽기만 했다. 빌드·시험은 `git archive 1a82225`로 만든
  scratch 사본과 별도 `CARGO_TARGET_DIR`에서 했고, 사본에 probe를 넣은 경우는 아래 "확인 방법"에 따로 적었다.
  target과 사본은 기록 뒤 지웠다.
- 참고: 검토 시점의 `/home/claude/xadb` 작업 트리에는 커밋되지 않은 변경 2개가 있다. `dev3-tree-smo.md`에
  "실행한 명령" 표 12행이 더해졌고, `document-validation.json`이 다시 생성되었다. 검토 대상은 commit이며, 이
  미커밋 표는 필수 1에서만 다룬다.

## 결론

열린 필수는 **1건**이다.

제품 코드에서는 계약 위반이나 결함을 찾지 못했다.

- `apply_unlink`는 모든 fallible 검사가 끝난 뒤에만 arena와 bank를 바꾼다. claim과 Job은 건드리지 않는다.
- 관측의 `retired`·`initialized`는 비어 있다. 그래서 퇴역 page는 Allocated로 남고 claim·target·certificate도
  유지된다.
- 길이 dispatch는 두 문법의 길이가 겹치지 않는다. 고른 decoder가 첫 단계에서 길이와 opcode를 다시 거절하므로,
  잘못 고른 frame이 검사를 건너뛰는 경로가 없다.
- source 회계는 split 회계와 같은 단계 규칙을 쓰고 코드 순서와도 맞는다. split 값은 바뀌지 않았다. 26,452 / 65,392·65,400 / 40,881B
  고정값과 C 여유 184B를 이번 실행에서 다시 확인했다.
- fixture는 다시 생성하면 byte가 같다. 기존 19개는 U1과 같다.
- R01..R03의 기대값은 독립 fixture와 literal에서 온다. 기록된 세 명령의 선택 수와 결과는 115 / 79+1 ignored / 2로,
  주장과 같다.

필수 1건은 증거 기록이다. 검토 대상 commit의 U2 결과 절에는 실행 명령과 결과 수가 없다. 그 표는 작성자 작업 트리에만
있다. 표를 commit하고 그 안의 SPL 범위 한 곳을 고치면 ACCEPT로 본다.

## 확인 방법

읽은 것:

- 계약 1122–1542행. 대체 목록, 전이, XTR1, scope4 frame, 준비·효과·복구(복구·Base4 closure·소비자), 시험 계획,
  구현 단계, U1 결과, U2 결과.
- 제품 diff 전체: `recovery.rs` dispatch, `history/structure.rs`의 `is_unlink`·`apply_unlink`·`unlink_source_bytes`·
  `source_bytes`, `history.rs`, `unlink/source.rs`, `candidate.rs`·`candidate/source.rs`·`frame.rs`의 회계 추출.
- 비교 대상 기존 코드:
  - split `apply`(`history/structure.rs:116–238`), `apply_frame`·`observe_recovery_frame`(`plan4_retention/recovery.rs:462–633`)
  - unlink `decode_layout`·`decode_core`·`validate_records`(`unlink.rs`), `unlink/candidate.rs`의 `build`·
    `verify_after`·`check_debt`, `unlink/frame.rs`의 `produce`·`observe`·`body`·`order`
  - split `decode_layout`의 길이 검사(`semantic_row/structure.rs:322`), `Cursor::part`·`patch`의 비교(`frame.rs:84–193`)
  - 관측 소비자 `plan4_recovery/analysis.rs:540–616`과 `capture_base`·`pack_capture`(1055–1215행), `epoch.rs:309`
  - `history.rs`의 `validate_current`·`validate_bijection`, `history/catalog.rs`의 `state_digest`
- 시험 diff 전체: `structural_replay_tests/unlink.rs`, `structural_replay_tests.rs`의 harness(`World`·
  `capture_after`), `weights/unlink_tests.rs`의 P08, `unlink/source_tests.rs`, `history/structure.rs` tests,
  `row_gate.rs` 주석. fixture `author.py` diff와 `inputs.sha256`.
- 정책: `coding-convention.md`(독립 oracle·기대값 출처), `development-policy.md`(D1/D2·거짓 PASS),
  `documentation-policy.md`(push gate), AGENTS.md(변경 코드 시험 범위). U1 검토 기록 2개.

실행한 것과 결과(scratch 사본, `CARGO_TARGET_DIR` 분리, 64자 자리표시 hash):

| 명령 | 결과 |
| --- | --- |
| `cargo test -p xadb-engine --lib --locked --no-run` | 빌드 4분. libtest 전체 2,381건(U1 검토 때 2,375건 + 새 시험 6개) |
| 기록된 세 filter에 `--list` | cmd1 **115**건, cmd2 **80**건(그중 ignored 1: `structural_effect_native_worker`), cmd3 **2**건. cmd1과 cmd2는 겹치지 않는다. U2 결과 표의 경로 6개는 모두 실제 libtest 경로와 맞는다 |
| cmd2 실행(`retention::history::structure:: … retention::owner_tests::`, CATALOG·PLAN4_RECOVERY 자리표시) | **79 passed, 0 failed, 1 ignored**, 2,301 filtered out, 1,116.6s. UNL-R01..R03, 두 R04 중 history 쪽, SPL-A01..A06·A08·A10, SPL-J01..J07, SPL-REVIEW 3개, structural effect, S1B, S2W-C01..C06, owner_tests 10개를 포함한다 |
| cmd1 실행(`storage::anchor::tests:: storage::semantic_row::structure:: storage::plan4_recovery_base::row`, CATALOG 자리표시) | **115 passed**, 0 ignored, 5.3s. UNL-R04 source와 새 P08이 여기 있다 |
| cmd3 실행(`plan4_recovery::tests::recovery_source_`, PLAN4_RECOVERY 자리표시) | **2 passed** |
| `unl_r04 s1b_row_arm_census s2w_c0 structural_replay_source_inventory candidate::source_tests frame::source_tests --nocapture --test-threads=1` | 13 passed. 출력값은 측정값 절. U2 결과의 24,876 / 25,036 / 6,026 / 10,170 / 26,452 / 65,392·65,400 / 40,881 / 184B와 여유 160B가 모두 출력과 같다 |
| fixture: `module-row-format`를 새 디렉터리에 복사하고 `current-tree/*.xrb`·`*.sha256`을 지운 뒤 `python3 -I -B author.py` | 21개 생성. `inputs.sha256`·`parents.sha256`이 commit과 같다. 21개 모두 commit의 `.xrb`와 byte가 같다. 기존 19개는 `98faf08`의 blob과 sha256이 같다 |
| before/after fixture 구간 비교(검토자 script, 제품 코드 미사용) | `unlink-leaf-before`→`unlink-leaf`에서 다른 곳은 arena page 10·17, certificate 10·17, bank 33, header의 96/216/248과 section digest, inventory LSN·digest, 끝의 seal뿐이다. `unlink-collapse-before`→`unlink-collapse`는 page 10, certificate 10과 같은 나머지 칸만 다르다. claim section과 allocator page는 같다. R01의 "바뀐 page [10, 17] / [10], claim 불변"과 맞는다 |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS(경고 0, 2분 31초) |
| `python3 -B devdoc/verify_documents.py` | FAIL. 원인은 예상한 `module-gate/evidence/dev3-08-1-unlink-u2/review.md` 누락 링크 1건뿐이다. 다시 생성한 파일별 `links`·`sha256`은 commit된 `document-validation.json`과 모두 같다 |
| 사본 probe(아래 선택 7): `history/structure.rs:446`의 unlink `semantic`에 +1,096B를 더해 25,972B로 만든 뒤 `s1b_row_arm_census s2w_c0 unl_r04_unlink_replay --nocapture --test-threads=1`. 끝난 뒤 파일을 commit blob과 같게 되돌렸다(`cmp` 확인) | history frame 27,388B, preflight c_margin −744B, row_lifecycle 41,809B. 실패는 S2W-C01·C02·C03·C06과 S1B memory·native다. C04·C05와 history R04는 통과했다. 계약의 처음 계산 서술과 같다 |

전체 suite는 돌리지 않았다(AGENTS.md). XRB executor 전용 시험은 돌리지 않았다. U2는 `plan4_recovery_base`를
바꾸지 않는다(`git diff --stat 98faf08 1a82225`에 해당 경로가 없다).

초점별 확인(결함 없음):

1. **`apply_unlink`의 원자성**(`history/structure.rs:255–340`). fallible 단계의 순서는 다음과 같다.
   - arena·bank 길이 검사
   - `unlink::decode_layout`
   - `observe_structural_bank_row`, `raw_descriptor`
   - `unlink::frame::observe`(같은 cut에서 `prepare`, header·opcode 비교, 160..len−32 byte 비교)
   - 바뀐 target마다 `request_from_image`와 검사

   쓰기는 그 뒤에만 한다. arena에는 바뀐 page만 쓰고 bank 34를 고친다. `cells`는 불변 대여이고 `claims`는 이 arm에
   넘어가지 않는다. 실패 때 바뀌는 것은 `after` 작업 공간뿐이다. R02는 opcode마다 거절 8개, 그리고 cross-cut 1개에서
   `state_digest`(arena·Job·bank·claim)와 cursor가 그대로임을 본다.
2. **target 검사.**
   - `after[0]`/`after[1]`과 `observed.targets[0]`/`[1]`은 candidate slot ANCHOR=0·PARENT=1(`unlink/candidate.rs:27–28`)이다.
     `Observation.targets`는 `candidate.pages` 그대로다(`unlink/frame.rs:27,35`).
   - 검사는 page_no < 64, 바뀐 page LSN = frame LSN, after image의 identity·kind·level = candidate header다.
   - split의 "같은 page_no 중복" 검사(`history/structure.rs:194–197`)는 이 arm에 없다. 하지만 `body`의 `order`가 native의
     (file, page_no) 중복을 IdentityMismatch로 거절하고(`unlink/frame.rs:89–93`), `decode_core`도 주소 alias를
     거절한다(`unlink.rs:175–196`). anchor와 P가 같은 page인 frame은 비교 전에 거절된다.
3. **bank 행.** split과 같은 4줄이다(`history/structure.rs:232–236`과 `334–338`). `view.descriptor()`는 frame의
   352..1888이고, 비교 단계에서 다시 만든 `finished_descriptor`와 byte로 같음이 확인된 값이다. R01은 capture 전체를
   독립 after fixture와 비교하므로 bank 33의 descriptor(child.xod 기반, LSN 90, WAL 35,248/19,032)도 독립 bytes로
   확인된다.
4. **split `apply`에 있고 unlink arm에 없는 검사.** INIT target의 arena·claim이 0인지 보는 검사와 `initial_record`가
   없다. unlink에는 INIT이 없으므로 해당하지 않는다. allocator Reserved guard는 dispatch 앞에서 두 arm이 같이
   쓴다(`plan4_retention/recovery.rs:581–583`).
   - `apply_frame`의 `previous+1 == lsn`(627행)과 `observe_recovery_frame`의 `refresh_jobs`·`validate_current`는
     arena를 바꾼 뒤에 돈다. 이는 모든 arm에 원래 있던 구조다.
   - unlink candidate는 lsn = previous+1로 만들고(`unlink/candidate.rs:114–119`) header를 비교한다
     (`unlink/frame.rs:236`). 그래서 수락된 unlink frame이 이 사후 검사에서 실패할 수는 없다.
5. **길이 dispatch.** `is_unlink`는 35,248·19,032만 고르고, `Operation::frame_bytes`의 split 값 84,744·84,640과
   겹치지 않는다.
   - unlink 길이에 opcode 1·2가 오면 unlink `decode_layout`이 길이·envelope·group 검사 뒤 `!is_unlink()`로
     Unsupported를 낸다(313–315행). 길이와 opcode가 맞지 않으면 Corrupt다(318행).
   - split 길이에 opcode 3·4가 오면 split decoder가 Unsupported를 낸다. 그 밖의 길이는 split decoder가 Corrupt로
     거절한다(`semantic_row/structure.rs:322`).
   - 어느 쪽이든 `decode_layout`이 첫 fallible 단계라 상태가 바뀌기 전이다.
6. **관측 소비자와 capture.**
   - `analysis.rs:540–557`은 `retired`의 page를 target·match bit에서 지우고 target 기록을 0으로 만든다. unlink는
     `retired`를 비우므로 퇴역 page의 target과 match가 남는다.
   - 바뀐 anchor·P는 split의 target과 같은 `request_for_kind`·`observe_home`·`note_target`·`observe_pending`
     경로를 지난다.
   - `pack_capture`의 certificate는 `target_mask`의 page마다 만든다(1153–1170행). 그래서 퇴역 page도 certificate를
     받는다. Structural 칸 검사는 같은 `observe_structural_bank_row`다(895행). R01 재시작 world의 `input()`이 이 함수로
     unlink descriptor를 받는다.
   - 원천이 있는 복구는 매번 `capture_base`를 부른다(`epoch.rs:309`). 따라서 U3의 UNL-L 재시작 read가 제품 rebase를
     지난다는 U2의 미룸 근거는 맞다.
7. **회계 단계와 코드 순서.**
   - `build`: `previous` → allocation → `table` → `check_debt` → `audit`(prefix), 그 뒤 reached·tree decode·parent
     edit·list extend·anchor encode·`decode_core`(draft)다. `unlink/candidate.rs:120–311`의 순서와 `candidate_census`의
     `prefix_calls`·`draft_calls`가 맞는다.
   - `verify_after`: allocation → `table` → `decode_anchor` → `audit`. `verifying`과 맞는다.
   - `produce`·`observe`: `LayoutView`가 `prepare`·`body` 동안 살아 있고 `Observation`은 그 뒤 만든다.
   - `apply_unlink`: bank 행·`inputs`·관측 결과는 block 안에 있고, `result`는 관측이 돌아온 뒤 선언된다. compare와
     finishing의 분리가 split `apply`의 규칙과 같다.
   - split 회계 추출(`previous_chain`, `descriptor_chain`, `redo_chain`)은 식이 글자 그대로 같다.
   - S1B 출력(row 25,739, structural 26,452, phase_max 25,743, frame 26,452, closure 20,951, live 64,016/64,024,
     recovered 65,392/65,400)은 U1 검토의 측정값과 같다.
8. **fixture.**
   - `recapture`는 header 96/248, inventory LSN과 그 digest, section digest, seal만 바꾼다. 216(Structural
     high-water)과 bank 33은 그대로다.
   - `leaf_unlink_before`의 root 17 [2,3,4]→11,18,16,19와 claim 19(role5, 85, ChildSplit, peer 없음, left 16)는
     `leaf_unlink`(after)의 같은 값과 맞는다. `root_collapse_before`도 `root_collapse`의 앞부분과 같은 page·claim을
     만든다.
   - 표준 library만 import한다.
9. **시험 기대값의 출처.**
   - R01은 제품 producer의 frame을 쓰지만, 기대 page와 capture는 author.py fixture와 시험 쪽 `capture_after`(제품
     certificate encoder 미사용)에서 온다.
   - R02의 기대 오류 종류를 실행으로 확인했다. 다시 hash한 core 3칸, native digest, redo payload는 Corrupt로
     거절된다. 이 Corrupt는 `Cursor::part`·`patch` 비교에서 나오며, 다섯 경우 모두 `decode_layout`을 통과한다. 범위
     밖 행이 든 leaf와 P 손상은 `Tree(_)`, bank 칸 손상은 Invariant, cross-cut은 IdentityMismatch다.
   - R03의 "새 page가 11·16·17이 아님"은 literal이다.
   - P08의 Job은 `job3/cells.bin` 9번 cell이다. (2,2) = RootRetained·Catalog, 176..200 = (6,1,1)을 literal로
     대조한다. database 16 byte만 바꾼다.

## 필수

1. **검토 대상 commit에 U2 시험의 실행 명령과 결과 수가 없다**(`devdoc/contracts/dev3-tree-smo.md:1520–1542`).
   U2 결과는 "시험(모두 PASS, memory)"와 시험별 경로만 적는다. U1 결과에는 명령 표(1452–1459행)가 있지만 U2 결과에는
   그 표가 없다. 그 표는 `/home/claude/xadb` 작업 트리의 미커밋 변경으로만 있다(`git diff HEAD`, 1543행 뒤 +12행).
   commit만 보면 어떤 명령으로 몇 건이 통과했는지 재현할 수 없다. U1 1회차 필수 1(시험 증거 기록이 실제 실행과 맞지
   않음)과 같은 종류다(development-policy의 거짓 PASS·필수 시험 누락 0, AGENTS.md의 원 source·실행 범위 보존).

   미커밋 표의 내용은 이 검토의 재현과 같다. 선택 수 115 / 80 / 2, 결과 115 passed·0 ignored / 79 passed·1
   ignored / 2 passed가 맞고, XRB 미실행 사유도 맞다. 다만 한 곳이 실제 선택과 다르다. cmd2 설명의 "SPL-A01..A10"과
   달리 cmd2는 SPL-A07(`storage::tree_page::search::tests::spl_a07_*` 2개)과 SPL-A09
   (`storage::row::insert::driver::tests::spl_a09_*`)을 고르지 않는다. 두 시험은 U2가 바꾼 코드에 닿지 않으므로
   고르지 않은 것은 맞다. 설명만 틀렸다.

   필요한 수정: 미커밋 명령 표를 commit한다. 그 안의 "SPL-A01..A10"을 "SPL-A01..A06·A08·A10"으로 고친다.

## 선택

1. **README의 B09 표현**(`devdoc/README.md:18`). "UNL-R01..R04(B07 재시작·B09 capture 포함)"만 보면 B09가
   끝난 것으로 읽힌다. U2 결과(1533–1535행)에 따르면 R01의 B09는 시험 쪽 raw capture 대조이고, 제품 rebase
   (`capture_base`, BaseOnly)는 U3로 넘어갔다. U1 결과(1446–1447행)는 이 일을 U2로 미뤘다고 적고 있다. 다시 미룬
   것이므로 README에 "B09는 raw capture 대조, 제품 rebase는 U3"처럼 적기를 권한다. 계약의 구현 단계 U3 행이나
   UNL-L 행에도 "B09 제품 rebase"를 적어 두면, U3 검토가 그 의무를 시험 계획에서 바로 볼 수 있다.
2. **R03 문구와 판정 방법**(`dev3-tree-smo.md:1527`, `structural_replay_tests/unlink.rs:274,285`).
   - "목록 [11, 17] bytes 유지"라고 적지만 시험은 decode한 `RetiredPage` 목록을 비교한다. codec이 0 padding을
     검사하므로 효과는 같다. 그래도 anchor cell 1의 bytes를 비교하거나 문구를 "항목 유지"로 바꾸는 편이 정확하다.
   - 계획의 "연속 split→unlink→split"에서 첫 split은 replay한 frame이 아니라 fixture 상태(RootSplit 70)다. 표의
     설명은 정확하지만, "계획과 다르게 남긴 것"에 한 줄 적기를 권한다.
3. **P08 주석의 순서 주장**(`weights/unlink_tests.rs:442–446`). "before any unlink or NotNeeded decision (and
   before the tree audit)"라고 적는다.
   - 시험이 보이는 것은 두 가지다. 빈 leaf 21의 unlink 판단보다 먼저인 것, 그리고 행이 있는 leaf 13의 NotNeeded보다
     먼저인 것이다.
   - RootRetained Catalog Job은 audit을 실패시키지 않는다. 그래서 `check_debt`가 audit 뒤에 있어도 같은 결과가
     나온다. root leaf의 NotNeeded보다 먼저인지도 보지 않는다.
   - 코드 순서(`unlink/candidate.rs:127–130`)는 주석과 맞다. 괄호 부분을 "코드 순서상"으로 낮추거나, audit을
     실패시키는 다른 조건과 함께 넣는 경우를 더하기를 권한다.
   - 다른 table의 Catalog Job은 거절하지 않는다는 반대 대조도 한 줄이면 된다.
4. **회계 목록의 빈칸.** 값에는 영향이 없다.
   - `build`의 `anchor::retired` decode(`unlink/candidate.rs:178`)는 `unlink/source.rs`에 이름이 없다.
     page decode라 `tree_chain`·`reached_chain` 최대 안에 들지만, `anchor_chain` 주석에 적기를 권한다.
   - 독립 parser 목록 `parsing()`(`unlink/source_tests.rs:50–83`)은 `decode_core` 자체 frame을 세지 않는다.
     `[PageRef; 4]`와 scalar로 약 200B이고(`unlink.rs:128–198`), layout의 2,048B 여유 안이다.
   - 이 "독립 목록"은 회계와 거의 쌍둥이다. 출력은 wrapper 882/884, frame outer 1,984/1,988이다. split의 기존
     시험(962/964)과 같은 관행이라 문제 삼지 않는다.
   - history 쪽 R04(`history/structure.rs:571`)는 `arm ≤ all`만 본다. "scope4 = 둘 중 큰 쪽"은 row_gate의
     26,452 `assert_eq`가 간접으로 지킨다.
5. **dispatch 교차 시험.** split opcode frame을 unlink 길이로(또는 그 반대로) 바꿔 `apply_frame`에 넣고, 상태
   불변 거절을 보는 replay 시험이 없다. decoder 단위는 U1 F01·F02가 보고, 위 5의 코드 읽기로 안전하다. R02에 한 경우를
   더하면 dispatch 자체가 고정된다.
6. **`document-validation.json`**. commit된 파일은 `status: FAIL`이고, 오류에 작성자 checkout의 절대 경로
   (`/home/claude/xadb/…`)를 담는다. U1 선택 4와 같다. 파일별 sha는 commit과 맞으므로, review.md를 둔 뒤 다시
   생성하면 된다. PR의 module-gate record `rust_tests.cases`에는 위 실제 libtest 경로를 쓴다.
7. **처음 계산의 −744B**(`dev3-tree-smo.md:1508–1510`). 지금 코드에서는 재현할 수 없는 과거 값이라 사본 probe로
   확인했다. 25,972B면 history frame 27,388B, C 여유 −744B이고 S2W-C01·C02·C03·C06이 실패한다. 서술과
   같다. 다만 같은 probe에서 S1B 고정값 시험(`s1b_row_arm_census_memory`·`_native`, 26,452 `assert_eq`)도 실패하므로,
   실패 목록에 S1B를 더하면 정확하다. 고칠 필요가 크지는 않다.

## 측정값

- unlink 회계(이 사본 출력):
  - candidate: prefix 1,160/1,224, draft 6,808/7,080, wrapper 882/884, after 1,040/1,304, peak 21,096
  - frame: parser 11,991/17,495, emitter 13,193/15,567, outer 1,984/1,988, return 1,952/1,988, peak 23,084
  - replay arm: compare 24,396/24,876, finishing 4,762/6,026. scope4 25,036/10,170
- split 회계(같은 실행): candidate peak 20,972, frame peak 23,148(layout 17,495, body 16,799), replay compare
  24,780/25,036, finishing 6,442/10,170. unlink candidate peak(21,096)은 split(20,972)보다 124B 크다. 하지만 frame
  outer가 작아서 frame peak은 64B 작고, replay compare는 160B 작다.
- S1B(memory·native): row 25,739, structural 26,452, phase_max 25,743, frame 26,452, closure 20,951, frame_return
  14,532, live 64,016/64,024, recovered 65,392/65,400
- S2W: row_lifecycle 40,881(두 writer), preflight c_total 37,976, bound 6,144, charge 44,304, c_margin 184
- cmd2 1,116.6s(79 passed, 1 ignored), cmd1 5.3s(115), cmd3 0.1s(2). build 4분, clippy 2분 31초
- log: `/tmp/claude-0/-home-claude-xadb/484d24b6-02c4-5c37-84ef-21ff3507de06/scratchpad/review-u2/logs/`의
  `cmd1.log`, `cmd2.log`, `cmd3.log`, `census-nocapture.log`, `list-*.txt`, `clippy.log`, `fmt.log`, `docs.log`,
  `regenerated-inputs.sha256`, `regenerated-xrb.sha256`, `probe-1096.log`, `probe.diff`, 검토자 script `fixdiff.py`·`run-tests.sh`. build target(약 2.5GB)과 사본은
  기록 뒤 지웠다.

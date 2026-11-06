Verdict: ACCEPT

# 독립 D2 재검토(3회차): 빈 leaf unlink U2 — U1 위 rebase와 UNL-C01·C02 census

## 대상

- branch `u2-main`, HEAD `db0ac7d`(tree `5ac81672…`). base는 `u1-main` `2b39998`(PR #56, U1 최종)이다.
- 2회차(`review.md`, ACCEPT)는 옛 U1 위의 `fa766b5`를 보았다. 이번 범위는 세 가지다.
  1. rebase. 충돌은 `dev3-tree-smo.md`와 `document-validation.json` 두 파일뿐이었다.
  2. `7f63c50`: code와 시험, 6개 파일, +122/−12.
  3. `db0ac7d`: 계약 문구와 `document-validation.json`.
- 계약은 `git diff u1-main..u2-main -- devdoc/contracts/dev3-tree-smo.md` 전체를 읽었다. 특히 본 곳:
  - `#unlink-u2-census`(1591–1623행)
  - U2 결과의 값 서술(1574–1582행)
  - 재실행 기록(1661–1674행)
- 검토 의무: `af6a31e`의 UNL-C01(XTR1 decoder·encoder의 중첩 비용을 영향받는 census마다 센다. 타입 크기 불변 논법은 쓰지 않는다)과 UNL-C02(U2: opcode dispatch·canonical replay·base capture census를 fresh·recovered writer 양쪽에서 검산한다. 상한을 올리려면 별도 D2).
- 검토자는 이 코드를 작성하지 않았다.
  - 작성자 worktree는 읽기만 했다. 그 안에서 `python3 -B scripts/check_module_gate.py check`를 읽기 전용으로 한 번 돌렸고, 뒤에도 status는 깨끗했고 HEAD는 `db0ac7d`였다.
  - 빌드·시험은 `git archive u2-main` 사본과 별도 `CARGO_TARGET_DIR`에서 했다.
  - 사본에 넣은 probe 2개는 "직접 확인한 것"에 따로 적었다. 둘 다 끝난 뒤 파일을 commit blob과 같게 되돌렸다(`cmp`).
  - target과 사본은 기록 뒤 지웠다.

## 판정: **ACCEPT**. 필수 0, 권고 4(1건은 절차), NIT 3

근거:

- **rebase가 U2 code를 바꾸지 않았다.** 두 diff의 +/− 줄이 글자까지 같다: `git diff u1-main be60ad7 -- crates`와 2회차가 본 `git diff 98faf08 fa766b5 -- crates`(각 1,364행).
  - U1 최종의 C01 항은 rebase 뒤에도 남아 있다: split `table_chain`의 `decode_source_bytes`, `retired_chain`, `build`의 `anchor_codec`.
  - U2가 split census에 한 일은 공개 범위 변경과 `previous_chain`·`descriptor_chain`·`redo_chain` 추출뿐이다. 값은 같다.
- **UNL-C01: unlink candidate의 anchor 호출이 모두 셈에 들어 있다.** `candidate.rs`의 호출과 그 census 항은 다음과 같다.
  - `build`:
    - `candidate::table`(127행, 안에서 `decode_anchor`): split `table_chain`, prefix 단계
    - `candidate::audit`(130행, 안에서 `retired`): `audit_chain` ⊇ `retired_chain`, prefix 단계
    - `anchor::retired`(178행): 새 whole-call 항 `retired_source_bytes(2,474)` = 3,843B, draft 단계
    - `RetiredList::extend`(247·249행)와 `encode_listed_anchor`(255행): `anchor_chain`의 `extend_source_bytes().max(listed_encode_source_bytes(page))`, draft 단계
  - `verify_after`:
    - `candidate::table`(352행): `table_chain`
    - `anchor::decode_anchor`(354행): 새 항 `whole_decode_source_bytes(2,474)`
    - `candidate::audit`(359행, 바뀐 anchor에 대한 `retired`): `audit_chain`
  - 단계도 맞다. `retired`·`extend`·encode는 audit가 돌아온 뒤의 draft 값이고, decode는 verifying이다.
  - U2의 다른 code에는 anchor 목록 decode가 없다. frame body·`decode_layout`·`apply_unlink`의 `request_from_image`는 `decode_page`만 부르고, 그 항은 기존 page codec 항이다.
- **UNL-C02 dispatch: 셈과 배치가 맞다.**
  - unlink 경로에만 있는 값은 둘이다: after 2장 대여(8B)와 그 `try_into` Result(관례대로 ×2).
  - `is_unlink`는 두 경로 모두에서 arm보다 먼저 돌아온다. 그래서 split 쪽에서는 max로 덮인다. unlink 쪽에 합으로 더한 것은 보수적이다.
  - 두 단계에 다 더한 것도 맞다. 대여는 `apply_unlink` 호출 전체 동안 살아 있다.
  - 더한 뒤 compare는 24,924 < 25,036B(여유 112B)이고, finishing은 6,074 < 10,170B다.
  - 그래서 scope4는 계속 split arm이다. history frame 26,452B는 S1B가 `assert_eq!`로 고정한다(`row_gate.rs:630`). live Row lifecycle 40,883B는 S2W-C03이 고정한다.
- **UNL-C02 base capture: capture 회계가 목록 읽기까지 담는다.**
  - `capture_encode`는 `decode(…)`로 Base4 row codec 59,675B를 담는다.
  - 그 codec은 current-tree closure 28,928B를 담는다(`graph/source.rs:205,323`). closure 안에 `retired_source_bytes(2,240)` = 3,609B가 있다(`current_tree.rs:305`).
  - unlink 상태에서 packing(`pack_bank`)이 지나는 Structural 칸 경로는 split 상태와 같은 함수·타입이다. 다른 것은 `bank::validate`의 `consumed_wal` match 갈래뿐이다.
    - `check_structural_bank` → `observe_structural_bank_row` → `bank::observe`/`validate`
    - `decode_ram_descriptor` → `SystemWorkRef::decode` + `decode_descriptor`
    - 이 경로는 `packing_source_bytes`의 `structural_bank_source_bytes()` 항(analysis.rs:1246)이 덮는다.
  - certificate loop와 `pack_inventory`는 anchor를 decode하지 않는다. 빠진 항은 찾지 못했다.
- **바뀐 값은 계약과 모두 같다**(probe와 시험 출력). 상한은 하나도 바뀌지 않았다.

## 필수

없음.

## 권고

1. **계약 U2 절의 값 두 곳이 rebase와 C02 뒤의 값이 아니다.**
   - 1578행 "preflight C 여유 184B"는 이제 **182B**다.
     - U1 UNL-C01이 preflight 본문을 37,384B에서 37,386B로, 합계를 44,120B에서 44,122B로 올렸다(같은 문서 U1 표 1472행).
     - S2W-C01·C02가 이번에 `c_margin=182`를 찍었다(memory·native).
     - 같은 문장의 40,881B는 40,883B로 고쳤는데 이 값은 남았다.
   - 1582행 "unlink compare의 split 아래 여유는 160B다"는 dispatch 48B를 더한 지금 **112B**다(25,036 − 24,924). U3가 이 여유를 보고 값을 더할 수 있으므로 지금 값으로 적는다.
2. **UNL-C02 base capture 서술(1607–1612행)에 packing 단계가 없다.**
   - capture는 `base_capture = capture_encode.max(owner + common + capture_outer + packing_source_bytes())`의 두 단계다. 계약과 시험은 encode 단계만 다룬다.
   - unlink descriptor가 든 Structural 칸은 packing 단계에서 `pack_bank`가 읽는다(analysis.rs:946·987). 위 "근거"처럼 같은 frame이고 `structural_bank_source_bytes` 항 안에 든다. 이 검산을 한 줄로 적는다.
   - `unl_c02_base_capture_holds_the_row_codec`에 `c.owner + packing_source_bytes() <= c.base_capture` 같은 한 줄을 더하면 두 단계가 모두 시험에 나온다. `packing_source_bytes`는 `pub(super)`라 시험 쪽 공개 범위는 맞춰야 한다.
   - 1611행 "capture encode 단계는 owner + codec(64,203/64,211B)"도 정확하지 않다.
     - owner + codec은 61,259/61,267B다(1,584/1,592 + 59,675).
     - 64,203/64,211B에는 rebase Current·Header·Expected·Sections·오류 둘·참조 20개·u64 16개와 `decode`의 Result·`VerifiedPlan4Base`도 들어 있다.
     - "owner + capture 인수 + codec"처럼 적는다.
3. **새 시험은 산술과 배선을 확인할 뿐, 새 항이 빠지는 것을 잡지는 못한다.** 계약 표(1621–1623행)와 commit `7f63c50`의 설명("UNL-C02 tests pin the dispatch term", UNL-R04가 UNL-C01(U2)의 시험)은 실제보다 강하게 말한다.
   - probe A로 확인했다. unlink `source.rs`에서 C01 항 셋(`retired` whole-call, `anchor_chain`의 extend/listed encode, `whole_decode`)을 모두 옛 식으로 되돌려도 census 시험 9개가 모두 통과한다.
     - 대상 시험: 두 UNL-R04, 세 UNL-C02, S1B memory·native, S2W-C03, split replay inventory
     - 새 항은 모두 binding 항(build 11,705 / verify 18,908B) 아래라 peak가 같기 때문이다. max 모델의 성질이다.
   - dispatch 시험의 "독립 dispatch 목록"(`history/structure.rs:594–597`)은 `unlink_dispatch_bytes()`와 같은 식을 그대로 옮긴 것이다. 그래서 `dispatch <= unlink_dispatch_bytes()`는 늘 48/48로 같다. 뒤의 strict `<` 두 줄은 의미가 있다(split arm이 최대라는 주장을 고정한다). 하지만 `all.semantic`은 dispatch를 넣든 빼든 split 값이라, `source_bytes()`가 dispatch를 더한다는 사실은 시험되지 않는다.
   - `unl_c02_current_tree_closure_counts_the_retired_list`의 `list <= closure`는 closure가 목록 항을 잃어도 참이다(3,609 ≪ 28,928).
   - 제안:
     - `CandidateCensus`에 build·verify 단계의 호출 최대값을 split `SourceCensus`(table·audit·parent…)처럼 필드로 낸다. 그리고 R04에서 `retired_source_bytes(page) <= c.draft_calls`, `after_decode() <= c.verify_calls`처럼 항마다 단언한다.
     - dispatch 목록은 `is_unlink` 안의 `len()`·`frame_bytes()` usize 임시값처럼 census와 다르게 센 목록으로 바꾼다.
     - 고칠 때까지 계약 표의 설명을 "각 항이 binding 항 아래라는 산술과 배선 확인"으로 바꾼다.
     - 값 자체는 이번 probe와 code 읽기로 확인했으므로 필수로 두지 않는다.
4. **(절차) module gate record `20261106-dev3-08-1-unlink-u2.json`이 낡았다.**
   - `check_module_gate.py check --base u1-main --head u2-main` 결과는 FAIL, 오류 29건이다.
   - 원인은 `7f63c50`과 rebase 뒤 head blob을 review `files`가 담지 않은 것이다. 거기서 `rust_tests[0..1]` 무효와 "no scoped CI evidence" 21건이 따라 나온다.
   - `rust_tests[1].reason`의 "live Row lifecycle 40,881 (C margin 184)"도 40,883 / 182로 고친다.
   - `candidate/source.rs`는 rebase로 blob이 바뀌었다. U2가 U1 위에 바꾼 것은 공개 범위와 `previous_chain` 추출뿐이고 값은 같다는 것을 이번에 확인했으므로, 이 review가 그 blob을 덮어도 된다.
   - 이번 검토가 본 head blob(앞 12자리):

| 파일 | blob |
| --- | --- |
| `storage/anchor.rs` | d9c91ca5432a |
| `plan4_recovery_base/row/graph/current_tree_tests.rs` | b80ca6d8d550 |
| `semantic_row/structure/candidate/source.rs` | 61799eac7778 |
| `semantic_row/structure/unlink/source.rs` | 8ba424438902 |
| `semantic_row/structure/unlink/source_tests.rs` | c019401812df |
| `wal_io/plan4_recovery/tests.rs` | 7391020e3665 |
| `wal_io/plan4_retention/history/structure.rs` | 4361f5aa1fe0 |
| `devdoc/contracts/dev3-tree-smo.md` | 6c464323efbf |
| `devdoc/document-validation.json` | f92230b09140 |

## NIT

1. **"unlink 쪽에만 있는 값"이라는 표현이 정확하지 않다.** 해당 위치는 `unlink_dispatch_bytes` doc comment("They exist only on the unlink path", `history/structure.rs:401–404`)와 계약 1602–1603행이다.
   - `is_unlink`는 split frame에서도 돌고 false를 낸다.
   - unlink에만 있는 것은 after 2장 대여와 `try_into` Result다.
   - `is_unlink`는 arm보다 먼저 돌아오므로 split 쪽은 max로 덮이고, unlink 쪽에 더한 것은 보수적이다. 이렇게 적으면 정확하다.
2. **1649행 "U2 최종 code" 명령 표의 선택 수는 rebase 전 값이다.**
   - 표의 수: 115 / 79 passed + 1 ignored
   - 지금 같은 filter가 고르는 수: 117(+`unl_c01_xtr1_codec_cost_per_decoder_path`, +current-tree UNL-C02) / 81(+dispatch UNL-C02)
   - 재실행 문단(1661행)이 이를 대신하지만, 표 제목을 "rebase 전 U2 code"로 고치면 혼동이 없다.
3. **2회차 선택 2의 뒷부분이 남아 있다.** parser 목록에 `decode_core` 자체 frame을 적는 일이다. 앞부분(`anchor_chain` 주석에 목록 읽기를 적는 것)은 이번에 반영됐다. 값에는 영향이 없다.

## 직접 확인한 것

- **diff 전체를 읽었다.** `git show 7f63c50 db0ac7d`와 계약 diff `u1-main..u2-main` 전체다.
  - `whole_decode_source_bytes`는 `retired_source_bytes` 안의 기존 식을 함수로 뽑은 것이다. `retired(938)` = 2,307B로 U1 3회차 probe와 같다.
- **probe 1**(사본의 unlink `source.rs`에 출력 시험 추가, 뒤에 원복):
  - page 항 2,474B. tree 1,869B, digest 1,680B.
  - draft 단계 호출: reached 5,906, tree chain 3,869, parent 11,705, anchor chain 2,661 → 3,790, retired 3,843, decode_core 280B.
  - prefix 단계 호출: table 9,148, debt 7,132, audit 18,908, previous 10,831, allocation 5,106B.
  - verify decode 2,698 → 3,403B. extend 409B, listed(2,474) 2,998B.
  - 옛 식과 새 식의 building은 20,284 = 20,284, verifying은 21,096 = 21,096, returning은 10,084다. 그래서 candidate peak 21,096, frame peak 23,084B로 그대로다. 계약 1594–1600행의 모든 수가 맞다.
- **probe A**(C01 항 셋을 옛 식으로 되돌림): census 시험 9개 PASS. 권고 3의 근거다.
- **시험 출력값**(memory·native):
  - UNL-C02 dispatch: 48/48. unlink compare 24,876(+48)/25,036, finishing 6,026(+48)/10,170.
  - UNL-R04 history: compare 24,396/24,876, finishing 4,762/6,026, scope4 25,036/10,170.
  - UNL-R04 source:
    - parser 11,991/17,495, emitter 13,193/15,567
    - prefix 1,160/1,224, draft 6,808/7,080, wrapper 882/884, after 1,040/1,304
    - frame outer 1,984/1,988, return 1,952/1,988, peak 21,096 / 23,084
  - UNL-C02 capture: list 3,609 ≤ closure 28,928 ≤ codec 59,675. owner 1,584/1,592 + codec ≤ encode 64,203/64,211 = capture ≤ peak 65,392/65,400.
  - S1B: row 25,739, structural = frame 26,452. recovered analysis 62,819/62,827, peak 65,392/65,400.
  - S2W-C01..C06: row_lifecycle 40,883, preflight c_total 37,978, total 44,122, **c_margin 182**. S2W-C04 live 64,016/64,024.
  - UNL-C01 anchor: page 938·1,869·786에서 decode +2B, retired 2,307·3,238, listed 1,462·2,393.
- **dispatch 경로**(`recovery.rs:579–594`): 4 arm에서 `is_unlink` 다음 unlink면 after 2장 대여 후 `apply_unlink`, 아니면 기존 `apply`다.
  - `frame_outer`(history.rs)의 dispatch 층 허용량은 두 갈래에 같다.
  - `apply_unlink`의 반환은 split의 `finishing`이 2개씩 세는 `RetentionFrameObservation`/Result 안에 든다.
- **capture 경로**: `capture_base` → `pack_capture`(`pack_bank`, certificate loop, `pack_inventory`, root decode) → `base::row::encode` = `encode_layout` + `validate`다. `validate`는 decode와 같은 Graph closure이고 그 안에 `current_tree`가 있다.
  - packing에서 anchor decode는 없다.
  - `CaptureInventoryRows`는 Structural wire 칸(33)을 읽지 않는다.
  - Structural 칸의 `bank::validate`는 opcode 3·4에서도 같은 frame이다.
- **실행**(사본, `jenv2.sh` 자리표시 hash, `RUST_MIN_STACK=33554432`):

| 명령 | 결과 |
| --- | --- |
| `cargo test -p xadb-engine --lib --locked --no-run` | 1분 52초. libtest 전체 2,388건 |
| cmd1·cmd3 filter + `unlink_replay::` + `retention::history::structure::` + `row_gate::` + `row::census_tests::` + `structural_split_source_inventory` + `unl_c02` | **141 passed**, 0 failed, 2.99s. UNL-R01..R04, UNL-C01, UNL-C02 셋, UNL-P08, recovery_source_ 둘 포함 |
| cmd2 filter(1650행과 같은 문자열) | 81건 선택. **80 passed, 1 ignored**(`structural_effect_native_worker`), 588.2s |
| `s1b_scope_routes_only_to_its_own_arm`, `mbc40_v01_profile2_binding_rejected_at_owner_grammar_sites` | 둘 다 FAIL. s1b는 `row_tests.rs:2019`에서 left `Some(RowSemantic(Corrupt))`, right `Some(Unsupported)`다. mbc40은 `format_tests.rs:490`(scope 4가 Unsupported가 아님)이다. s1b의 frame은 91,506B라 unlink 길이가 아니다. 그래서 U2에서도 main과 같은 split arm으로 간다 |
| 작성자 592개 목록과 대조 | 세 filter와 UNL-C02가 고르는 203건이 모두 목록에 있다. 위 두 실패 시험은 목록에 없다(계약 1668행에 사유가 있다) |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS, 경고 0, 1분 59초 |
| `python3 -B devdoc/verify_documents.py` | PASS. 다시 생성한 파일별 `links`·`sha256`가 commit된 `document-validation.json`과 모두 같다(date만 다르다). module-boundaries 행이 바뀐 것은 u1-main의 낡은 값을 바로잡은 것이다 |
| `check_module_gate.py check --base u1-main --head u2-main`(작성자 worktree, 읽기 전용) | FAIL 29건(권고 4) |

- **계약의 다른 서술도 대조했다.**
  - 시험 경로 세 개는 `--list`의 이름과 같다.
  - "U2는 capture code를 바꾸지 않았다"는 참이다. `analysis.rs`와 `resources.rs`는 u1-main과 같다.
  - `37a0465`가 split의 scope4 arm commit인 것도 맞다.
  - 상한·charge 상수는 바뀌지 않았다.

## 믿고 받은 것

- 작성자의 592개 `--exact` 실행(584 passed, 1 failed, 7 ignored). 이번에는 그중 겹친 시험 222건(141 + 81)만 다시 돌렸다.
- executor 전용 `g5_bind_boundaries`가 padding 2048에서 16/16이라는 것. XRB H 시험은 다시 돌리지 않았다. U2는 `plan4_recovery_base` 제품 code를 바꾸지 않는다(시험 파일에 case 하나만 더했다).
- 두 실패 시험이 main `1725df0`에서도 같은 값으로 실패한다는 것. U2 사본에서 실패 모양을 보았고, s1b는 frame 길이로 경로가 main과 같음을 확인했다. main은 빌드하지 않았고, mbc40 Catalog frame의 길이는 확인하지 않았다. 고치는 일은 별도 작업으로 넘어갔다.
- U1까지 수용된 census:
  - `anchor.rs`의 비용 함수들
  - split `table_chain`·`audit_chain`·`retired_chain`
  - Base4 `current_tree`의 퇴역 항목 loop가 closure 안에 든다는 것
  - packing·certificate의 기존 허용량
  - live Row lifecycle이 history frame을 포함한다는 경로(1회차 처음 계산의 C 여유 −744B 기록과 S2W 고정값)

## 작업 흔적

- 작성자 worktree는 수정하지 않았다(HEAD `db0ac7d`, status 깨끗).
- scratch 사본과 build target(약 3.0GB)은 지웠다.
- log: `/tmp/claude-0/-home-claude-xadb/484d24b6-02c4-5c37-84ef-21ff3507de06/scratchpad/review-u2c/logs/`
  - 시험·검사: `u2c-build.log`, `u2c-census.log`, `u2c-run1.log`, `u2c-cmd2.log`, `u2c-main-fail-u2.log`, `u2c-fmt.log`, `u2c-clippy.log`, `u2c-docs.log`, `u2c-docval.committed.json`, `u2c-gate-u1base.log`, `u2c-gate.log`(`--base main`. local main이 낡아 쓰지 않았다)
  - 선택 목록: `u2c-cmd1.txt`, `u2c-cmd2.txt`, `u2c-cmd3.txt`, `u2c-c02.txt`, `u2c-mine.txt`, `u2c-author-run.sorted`
  - probe: `u2c-probe1.log`, `u2c-probe-mutA.diff`, `u2c-probe-mutA.log`, 원본 `u2c-source.rs.orig`
  - rebase 대조: `u2c-old-u2.diff`, `u2c-new-u2.diff`

## 작성자 후속(3회차 뒤)

- 권고 1: 계약 U2 절의 preflight C 여유를 182B로, unlink compare의 split 아래 여유를 "arm만으로 160B, dispatch를
  더한 지금 72B"로 고쳤다.
- 권고 2: 계약 UNL-C02 base capture 서술에 packing 단계(`pack_bank`의 Structural 칸 경로, packing 24,887B 안의
  structural bank 항 12,679B)를 적고, encode 단계 64,203/64,211B의 구성을 "owner + capture 인수 + codec + decode
  Result·`VerifiedPlan4Base`"로 고쳤다. `unl_c02_base_capture_holds_the_row_codec`에 `bank <= packing`과
  `owner + packing <= base_capture`를 더했다(`packing_source_bytes`는 `pub(super)`라 공개 범위는 바꾸지 않았다).
- 권고 3:
  - `CandidateCensus`가 `prefix_calls`·`draft_calls`·`verify_calls`와 UNL-C01 세 항(`retired`·`anchor`·
    `anchor_decode`)을 필드로 낸다. UNL-R04 source는 독립 호출값 ≤ 그 항 ≤ 단계 최대, 단계 최대가 peak 식 안에 듦을
    항마다 단언한다(3,843/3,843, 3,502/3,790, 3,403/3,403B).
  - dispatch 시험의 독립 목록을 code의 값 하나씩(`is_unlink`의 인수·`len()`·`frame_bytes()` 둘·bool, slice·Result×2·
    묶은 참조)으로 바꿨다. 81B가 나와 옛 48B보다 컸다. 그래서 `unlink_dispatch_bytes`를 `(&[u8], [usize; 3], bool)`
    + slice + 참조 + Result×2 = 88B로 고쳤다. 더한 뒤 compare 24,964 < 25,036B, finishing 6,114 < 10,170B이고 scope4
    최대·history frame 26,452B는 그대로다.
  - `unlink_arm_bytes`(arm + dispatch)를 뽑아 scope4 회계가 그 값을 쓰고, 시험이 `unlink_arm_bytes` = arm +
    dispatch(두 단계)를 `assert_eq`로 본다.
  - 계약 시험 표에 각 시험의 판정을 적고, max 회계에서 binding 아래 항이 빠지는 것은 peak로 드러나지 않으므로 항을
    필드로 내어 이름으로 짚는다고 적었다.
- 권고 4(gate record): 이 기록 뒤의 commit에서 다시 만든다. U1(#56)이 main에 들어간 뒤 base를 main으로 다시 확인한다.
- NIT 1: `unlink_dispatch_bytes` doc comment와 계약을 "`is_unlink`는 두 경로 모두 arm보다 먼저 돌아오므로 split 쪽은
  max가 덮고, unlink 쪽에 더한 것은 보수적이다. unlink에만 있는 것은 after 2장 대여다"로 고쳤다.
- NIT 2: 명령 표 제목을 "U1 위 rebase 전 U2 code"로 고쳤다.
- NIT 3: UNL-R04 source의 독립 parser 목록에 `decode_core` 자체 frame을 더했다. parser 값 11,991/17,495B는 그대로다.
- 재실행: 바뀐 회계·시험에 닿는 10개 filter로 45 passed, 0 failed. fmt·clippy(`-D warnings`)·문서 검사 PASS.

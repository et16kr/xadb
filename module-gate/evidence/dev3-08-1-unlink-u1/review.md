Verdict: ACCEPT

# D2 검토 기록: U1 4회차 (ec198b3 위 미커밋 diff, branch u1-main)

## 판정: **ACCEPT**. 남은 필수 0, 권고 1(절차), NIT 2

3회차의 필수 1건과 권고·NIT는 모두 반영됐다. 아래 두 NIT는 문서 문구 문제이고, 제품 코드나 census 값에는 영향이 없다.

## 3회차 지적 대조

1. **필수 1, `history/row.rs:1082–1089` `table_source_bytes` — 해소.**
   - 새 식은 `frame + (search + tree.max(catalog)).max(decode_source(tree))`로, 수용된 묶음이 그대로 돌아왔다.
   - 값은 바뀌지 않은 성분(3회차 probe: selected 1,752, frame 600, search 3,384, tree 1,869, catalog 3,012)으로 계산했다: 1,752 + 600 + max(6,396, 1,871) = **8,748 B**. 문서의 "8,748B 그대로"와 맞다.
   - 이 값을 직접 찍는 시험은 없어서, 산술로만 확인했다.
2. **Undo 항 816 → 786 — 해소.** 새 시험 `…retention::read::catalog::tests::unl_c01_undo_decode_term_covers_the_list_check`는 PASS했고 `page=786 decode=788`을 출력했다.
3. **candidate build의 page 항에 `+1536` 추가 — 해소.** 같은 파일의 `retired_chain`과 일관된다.
   - anchor_codec은 3,843(listed 2,998)로 weights 15,820보다 작아 peak는 그대로다.
   - `verifying`에 붙은 주석 내용은 맞다.
4. **`check_list`에 `#[inline(never)]` — 반영.**
5. **A 생산자 — 타입 크기 논법을 미결 항목으로 바꿨다.** 공백이 이 단위 전부터 있었다는 점과 U3(UNL-A03·C03)로 넘긴다는 점이 정확히 적혀 있다.
6. **`extend_source_bytes` 행 — 해소.** 제품 호출자가 없다는 사실이 정확하다.
7. **NIT 3건(Result 1 B×2, `retired` 값을 쓰는 곳마다 표시, module-boundaries의 "위 표와 아래") — 반영.**

## 권고 (절차)

1. **module gate record `20261106-dev3-08-1-unlink-u1.json`을 아직 갱신하지 않았다.** 3회차에 `check` 결과 FAIL 54건이었다.
   - 이번에 새로 바뀐 `read/catalog_tests.rs`(새 case)까지 포함해, rust_tests와 review `files`를 commit 뒤 head blob으로 갱신해야 한다.
   - 범위 구분: `plan4_apply/row/read.rs`는 D2 common 대상이다(census 식만 바뀌었고 API·re-export 변화 없음). `dev3-module-boundaries.md`는 D2 policy 대상이다(날짜 단 메모뿐이고 규칙 변화 없음). 이 판정을 그 범위로 기록해도 된다.
   - 검토한 working-tree blob(앞 12자리):

| 파일 | blob |
| --- | --- |
| anchor.rs | 335bf6ce6e0f |
| anchor_tests.rs | 2cab0b0c5b83 |
| relations.rs | a0bee4f7dce1 |
| current_tree.rs | 6713562c8148 |
| semantic_catalog/resources.rs | 43da66652eed |
| candidate/source.rs | 5c330e11fc8e |
| transition/source.rs | 702a1458c183 |
| census_tests.rs | 82aa2cfba8b4 |
| plan4_apply/row/read.rs | f1d97673c4fd |
| read/census.rs | f58ff3e8c411 |
| plan4_genesis.rs | cb4f429d3f6b |
| plan4_pages/resources.rs | a942ae585312 |
| history/row.rs | 770bef2d24b9 |
| read/catalog.rs | 4ca8dd282ec4 |
| read/catalog_tests.rs | b42d5407266a |
| row_preflight.rs | 5d2f82bce1bb |
| epochs.rs | 0f7c5725891c |
| dev3-module-boundaries.md | c91dda92017c |
| dev3-tree-smo.md | d0ce17cd75e0 |

## NIT

1. **`dev3-tree-smo.md`의 `listed_encode_source_bytes` 행 "page 938B에서 1,462B"가 낡았다.** build가 이제 2,474를 넘기므로 값은 **2,998B**다. `retired` 행처럼 고친다.
2. **A 미결 문단 끝 문장이 앞뒤가 어긋난다.** "상한을 올리지 않고 별도 D2·사용자 결정으로 올린다"를 "임의로 올리지 않고, 올리려면 별도 D2·사용자 결정을 거친다"로 바꾼다.

## 직접 확인한 것

- **diff 전체를 읽었다.** 바뀐 곳은 7개 파일이다. anchor_tests.rs의 변경은 줄바꿈뿐이다.
- **`cargo fmt --all --check` 통과.**
- **working tree에서 5개 시험을 돌려 모두 PASS했다:**
  - `anchor::tests::unl_c01_xtr1_codec_cost_per_decoder_path`
  - `…read::catalog::tests::unl_c01_undo_decode_term_covers_the_list_check`
  - `s2w_c03` (memory·native 40,883)
  - `structural_split_source_inventory`
  - `row_replay_tree_selector_fits_existing_source_admission`

## 믿고 받은 것

- coordinator가 돌리고 있는 전체 영향 시험(576개 + UNL-C01 2개)의 결과.
- g5_bind_boundaries는 executor 전용이라 직접 확인하지 않았다.
- XRB H001 실패는 main과 같은 것으로 보고, 별도로 넘어간 일로 받았다.

repo는 수정하지 않았다(HEAD ec198b3, 미커밋 diff는 coordinator의 것 그대로).

## 작성자 후속(4회차 뒤)

- NIT 1·2는 `4ef9577`에 반영했다. `listed_encode_source_bytes` 행은 "page codec + SHA scratch 2,474B에서 2,998B",
  A 미결 문단 끝은 "상한을 임의로 올리지 않고, 올리려면 별도 D2·사용자 결정을 거친다"다.
- 3회차 NIT 5(UNL-C01의 `retired >`·`listed > page` 단언은 smoke 수준)는 그대로 두었다. 값 자체는 census 시험과
  계약 표가 고정한다.
- 권고 1(gate record)은 이 기록 뒤의 commit에서 갱신했다. review 범위는 D2 common(`plan4_apply/row/read.rs`의
  census 식만)과 D2 policy(`dev3-module-boundaries.md`의 날짜 단 메모만)로 적었다.
- 계약의 U1 상태 줄과 "실행한 명령"에 4회차 결과와 재실행 결과(578개 중 570 passed, 실패 1은 executor 전용
  g5_bind_boundaries로 padding 2048에서 16/16 PASS, ignored 7, XRB는 H001만 XB_FAIL)를 적었다.

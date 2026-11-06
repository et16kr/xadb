Verdict: CONDITIONAL

# D2 검토 기록: `ec198b3` (U1 UNL-C01 anchor codec census, 3회차)

## 판정: **CONDITIONAL**. 필수 1, 권고 5, NIT 5

근거가 된 것들:
- `encode_anchor`는 main과 같다.
- `check_list`는 실제로 `decode_page`가 돌아온 뒤에 불린다.
- root만 읽는 경로의 증분이 Result 하나(2 B)라는 모델은 이 codebase의 관례에 맞다.
- 새 U1 경로(Base4, split candidate)는 call 전체를 세는 함수로 셌다.
- 바뀐 뒤 값은 작성자 표와 모두 맞다.

다만 원래 범위에 없던 census 감소가 하나 있다.

## 필수

**1. `crates/engine/src/storage/wal_io/plan4_retention/history/row.rs:1082–1086` `table_source_bytes()`: 괄호를 다시 짜면서 이미 수용된 항이 1,143 B 줄었다.**
- cfbf279의 식은 `selected + (frame + search + tree.max(catalog))`이다. method 호출이 `+`보다 먼저 묶이므로 catalog read codec이 search frame 옆에 있다.
- 새 식은 `selected + (frame + (search+tree).max(decode(tree)).max(catalog))`이고, catalog 분기에서 search가 빠졌다.
- 실측값: frame 600, search 3,384, tree 1,869, catalog_read 3,012. 그래서 **8,748 → 7,605 B**다.
- 이 감소는 XTR1과 관계없다. 그런데 문서(dev3-tree-smo.md `unlink-u1-census`)는 이곳을 "넣은 곳"이라 하고, 바뀐 값을 모두 +2로 적었다.
- 소비자: `row_start.rs:933`(row_group_terms produce)와 `history/row.rs:1017`(lookup).
- 지금은 어느 peak도 이 항으로 정해지지 않는다. scratch 사본에서 옛 식으로 바꿔 census 시험 62개를 돌렸고 출력이 같았다.
- 하지만 수용된 census가 근거 없이 낮아진 것이다.
- 수정안: `frame + (search + tree.max(catalog)).max(anchor::decode_source_bytes(tree))`. 이러면 6,996 / 8,748로 돌아온다. 또는 줄인 근거를 따로 D2로 받는다.

## 권고

1. **module gate가 지금 FAIL이다.** `check_module_gate.py check --base origin/main --head HEAD` 결과 오류 54개.
   - 원인: record `20261106-dev3-08-1-unlink-u1.json`에 새로 바뀐 Rust 경로 13개의 rust_tests가 없고, review blob이 낡았다.
   - `plan4_apply/row/read.rs`는 D2 common을, `dev3-module-boundaries.md`는 D2 policy를 요구한다.
   - 이번 검토에서 두 파일을 확인했다. read.rs의 변경은 1480–1486의 census 식뿐이고 타입·API·re-export 변화가 없다. module-boundaries는 날짜를 단 메모뿐이고 규칙 변화가 없다. 이 내용을 record에 반영해야 한다.
2. **dev3-tree-smo.md:1448, 1454의 "816B(Undo decode 항)"은 틀렸다.** 실제 `undo_decode_source_bytes()`는 786 B다(probe 값, P4_LIVE_CENSUS의 `history_metadata: 786`과도 같다).
   - 586 < 786이라 결론은 같다.
   - 그러나 UNL-C01 시험은 938·1,869만 대조한다. 정작 C의 binding 지점(row_preflight `anchor_decode`·`table_root`, B read helper)이 넘기는 786 항은 확인하지 않는다.
   - plan4_retention 안에 `decode_source_bytes(undo) == undo + 2` 검사를 추가하기를 권한다.
3. **`semantic_row/structure/candidate/source.rs:286–288`: build의 `anchor_codec`에 SHA scratch 1,536이 빠졌다.** 같은 파일의 `retired_chain`(134)과 `allocation_codec`는 `+1536`을 넣는다.
   - 값은 2,307 B와 3,843 B 차이지만 binding이 아니다(weights 15,820, peak 20,972 그대로).
   - 일관성을 위해 `+1536`을 넣는다.
   - 문서 1450행의 "page 938B에서 2,307B"는 build만 설명한다. Base4는 2,240을 넘겨 3,609, audit는 2,474를 넘겨 3,843이다.
4. **A producer(`row/insert/group.rs:1154`)를 설명하는 문장 "carrier는 같으므로 그 값은 바뀌지 않았다"는 af6a31e가 배제한 타입 크기 논법 그대로다.**
   - A의 `decode_anchor` 중첩 비용(decoder frame 703 + page codec, 이번에 +2 B와 page 이하인 `check_list`)은 원래부터 어떤 census에도 없다(기존 공백, CLR_PRODUCER 여유 20 B).
   - "공백이며 U3 UNL-A03/C03에서 다룬다"로 고쳐 쓰고, 미결 항목으로 등록하기를 권한다.
5. **unlink candidate/frame은 U1 코드이고 시험에서만 불리는데 source census가 없다.**
   - `extend_source_bytes`는 시험에서 출력할 때만 쓰인다.
   - 표의 "U2 unlink candidate"는 오해를 부른다. "U1 코드, 제품 호출자 없음, U2/U3 census가 쓸 항"으로 적는다.

## NIT

1. `dev3-module-boundaries.md:3454`의 "아래 40,881 B": 위 3448행 표에도 40,881과 607이 있으므로 "위 표와 아래"로 고친다.
2. "Result 하나(2 B)": `size_of`는 1 B이고, 관례상 2배로 센 값이다.
3. `verify_after`(`candidate.rs:770–772`)의 `retired` 두 번과 `decode_anchor`는 `verifying` 안에서 `audit ≥ retired_chain`으로만 암묵적으로 덮인다. 주석을 다는 게 좋다.
4. `check_list`에 `#[inline(never)]`가 없다. source census 규약상 필요는 없지만, machine frame에서도 분리되기를 원한다면 붙인다(`issue_page`가 선례).
5. UNL-C01의 `retired >` 와 `listed > page` 단언은 자기 자신을 참조하므로 smoke 시험 정도의 의미다.

## 직접 확인한 것

- **`encode_anchor`**: 본문이 origin/main과 같다(awk로 뽑아 diff).
- **`decode_anchor`**: `decode_page` → 검증 → `validate` → `check_list` 순서다. 오류와 동작은 U1과 같다.
- **`retired`와 `encode_listed_anchor`**: `retired`는 decode한 뒤 다시 `decode_page`한다. 빈 목록일 때 `encode_listed_anchor`는 `encode_anchor`를 부르고 bytes가 같다.
- **제품 호출자 전수 grep**:
  - census에 들어간 곳: relations 186·237, current_tree 126·127, plan4_apply read 490(helper)·764(`issue_page`→schema), history 265, row_preflight 793·1051, epochs 379·451, read/catalog 271, genesis 514, candidate 216·284·672–673·770–772, transition prepare 376, semantic_catalog images 82.
  - "시험에서만 불린다"는 주장은 참이다: retention `read.rs:281`과 `plan4_pages/read.rs:73`의 `lookup_catalog_empty`, `semantic_row::leaf::observe`, unlink candidate/frame.
  - A `group.rs:1154`에는 중첩 census가 없다.
- **평평한 합 5곳**: 모두 codec 허용량(2048, 1536+512 등)이 938 이상이다.
- **probe 값**(scratch 사본): decoder_frame 703, check_list 586, walk 408, entry 78, extend 409, `Result<(),PageError>` 1 B, `retired`(938) 2,307, listed(938) 1,462. transition `before_page` 1,434 > 940. candidate peak 20,972와 Base4 current_tree 28,928은 바뀌지 않았다.
- **census 시험 62개 PASS**(memory·native, S2W-C01..C06, S3/S4, F4 C·A, MBC40-V09, recovery/retention/runtime_core/g5/plan4 source, S5R/S6R 등). 바뀐 뒤 값이 작성자 표와 맞다: 40,883; 37,386/44,122; 23,507·14,658; 39,566(여유 642); 34,588(여유 50/42); 복구 select 37,534, control 37,264, cleanup 39,426, clr_lane 27,987.
- **repo 안에서 직접 실행해 PASS**: unl_c01, s2w_c03, `anchor::tests` 9개, `structural_split_source_inventory`, `base4_closure`.

## 믿고 받은 것

- 바뀌기 전 값들(S2W-C03의 기존 고정값 외). cfbf279로 다시 빌드하지 않았다. 다만 필수 1을 뺀 모든 편집은 단조 증가(`max`로 감싸거나 +2)임을 식으로 확인했다.
- 기존 평평한 합 허용량이 정직하다는 점(이전 D2들이 수용함).

## 작업 흔적

- repo는 손대지 않았다. HEAD는 ec198b3이고 status는 깨끗하다.
- scratch 사본과 build target(3.1 GB)은 지웠다.
- 공유 scratchpad에 다음 파일을 썼다. 같은 이름의 파일이 있었다면 덮어썼을 수 있다: `/tmp/claude-0/-home-claude-xadb/484d24b6-02c4-5c37-84ef-21ff3507de06/scratchpad/{r3_t1.log,r3_probe.log,list.txt,new.log,old.log,b_new.log,b_old.log}`.

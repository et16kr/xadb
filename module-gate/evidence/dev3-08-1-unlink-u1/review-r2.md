Verdict: ACCEPT

# 독립 D2 재검토(2회차): 빈 leaf unlink U1

## 대상

- commit: `d1cb74f` (local branch `u1-local`, parent `09eb773`). 이번 차이는 `git diff 09eb773 d1cb74f`이고,
  전체 변경 `git diff b6dc3a2 d1cb74f` 안에서 읽었다.
- 1회차: `review.md`(CONDITIONAL, 필수 1, 대상 `09eb773`)
- 바뀐 파일 6개: `anchor.rs`(doc comment), `structural_effect.rs`, `structural_prepare.rs`, `devdoc/README.md`,
  `devdoc/contracts/dev3-tree-smo.md`, `devdoc/document-validation.json`
- 검토자는 이 코드를 작성하지 않았다. `/home/claude/xadb`은 읽기만 했고, 시험은 `git archive d1cb74f` 사본과
  별도 `CARGO_TARGET_DIR`에서 했다. H001 기준선은 `git archive b6dc3a2` 사본에서 확인했다. 두 target과 사본은
  기록 뒤 지웠다.

## 결론

열린 필수는 **0건**이다.

1회차 필수 1의 네 부분 (a)–(d)가 모두 닫혔다. 이번에 새로 적은 실행 명령 3개는 기록된 선택 수와 같고, 바뀐 코드에
닿는 cmd2를 다시 돌려 기록된 결과와 같음을 확인했다. XRB executor 결과도 사본에서 재현했다. H001 실패는 U1의
base(`b6dc3a2`)에서도 같은 위치로 실패한다. `b6dc3a2`는 main `1c9486a`과 `crates/`·`tests/`가 같으므로 U1 전부터
있던 실패다. 선택 2·3·5의 반영도 맞고, 새 match 갈래는 stage 전 Unchanged 거절이라 안전하다. 남은 것은 아래 선택
2건뿐이다.

## 확인 방법

읽은 것:

- `git diff 09eb773 d1cb74f` 전체
- 바뀐 계약 절(1399–1473행): split `decode_layout` 문장, U2 dispatch 문장, 시험 표, 명령 표, XRB 표
- 비교 대상 코드: split `decode_layout`의 길이 검사, unlink `decode_layout`의 opcode 검사, `structural_effect.rs`의
  `split_row_admitted`(새 갈래 앞뒤), `structural_prepare.rs`의 `semantic`, `plan4_recovery_base_tests.rs`의
  `cases`·`rejected`·`xrb_header_rejections`, `plan4_recovery_base.rs`의 `read_header_format`

실행한 것과 결과:

| 명령 | 결과 |
| --- | --- |
| `cargo test -p xadb-engine --lib -- --list`(d1cb74f) | 2,375건. 기록된 cmd1 filter는 **113**건, cmd2는 **60**건, cmd3는 **2**건을 고른다. UNL 16개는 모두 cmd1에 있고, cmd1과 cmd2는 겹치지 않는다. 시험 표의 6개 경로는 모두 실제 libtest 경로와 맞는다 |
| cmd2 실행(바뀐 `structural_effect.rs`·`structural_prepare.rs`·`history/structure.rs`에 닿는다) | **59 passed, 0 failed, 1 ignored**(`structural_effect_native_worker`), 560s. 기록과 같다. S1B census, SPL-A08, `structural_prepare_*`, `structural_admission_*`, `structural_replay_source_inventory_*`, SPL-A01..A06·J를 포함한다 |
| cmd1·cmd3 | 다시 돌리지 않았다. 두 범위의 코드는 1회차 뒤 바뀌지 않았고(anchor.rs는 doc comment만 바뀜), 1회차에 같은 시험이 PASS했다. 선택 수만 `--list`로 대조했다(AGENTS.md) |
| XRB 13개(d1cb74f). 작성자 방법대로: fixture `vectors/p1`, `XADB_XRB_INDEX_SHA256` = `raw-index.tsv`의 sha256, 자리표시 hash, 시험마다 자기 ID 전부, `--exact`. 시험 binary를 직접 실행했다 | C 15/15, S 16/16, G 24/24, I 8/8, E 8/8, ST 1/1, BC 6/6, BA 24/24, BB 1/1, BI 8/8, BST 1/1 XB_PASS. `xrb_s5_reserved_header_bytes`는 ignored. `xrb_header_rejections`는 H001에서 XB_FAIL(`plan4_recovery_base_tests.rs:295:18`, "invalid base accepted") |
| 같은 binary, `XADB_XRB_INCLUDE=XB-H002..XB-H016` | 15/15 XB_PASS |
| U1 base `b6dc3a2` 사본, `XADB_XRB_INCLUDE=XB-H001` | 같은 위치 `plan4_recovery_base_tests.rs:295:18`에서 XB_FAIL. `git diff --stat 1c9486a b6dc3a2 -- crates tests`가 비어 있으므로 main 코드와 같다. 두 사본의 `raw-index.tsv`도 같다 |
| `cargo fmt --all --check`(d1cb74f) | PASS |
| `cargo clippy -p xadb-engine --all-targets --all-features --locked -- -D warnings`(d1cb74f) | PASS(engine crate만) |
| `python3 -B devdoc/verify_documents.py`(d1cb74f 사본) | FAIL. 원인은 예상한 `module-gate/evidence/dev3-08-1-unlink-u1/review.md` 누락 링크 1건뿐 |

1회차 필수 1의 처리:

- **(a) 시험 식별자:** 닫힘. 표의 경로 6개는 `storage::anchor::tests::`,
  `storage::semantic_row::structure::weights::tests::candidate::unlink::`(`…::frame::`),
  `storage::semantic_row::structure::bank::tests::`, `storage::plan4_recovery_base::row::graph::tests::current_tree_tests::`
  이며, `--list`에서 각각 해당 UNL 시험을 고른다.
- **(b) 결과 수:** 닫힘. 기록된 명령 3개(113 / 59+1 / 2)를 `--list`로 대조했고, cmd2는 실행해서 같은 결과를
  얻었다. 154 주장은 지워졌다.
- **(c) xrb 문구:** 닫힘. module-gate.json의 `evidence.executor_only` 근거로 바뀌었고, executor 역할로 돌린 결과와
  방법이 적혀 있다. 재현 결과도 표와 같다.
- **(d) history·prepare 범위:** 닫힘. cmd2가 그 시험들을 고르고 실행했다.

선택 반영:

- **선택 2(exhaustive match):** 맞음. `structural_effect.rs:383–391`과 `structural_prepare.rs:199–221`이 `match`로
  돌아갔고, `LeafUnlink | RootCollapse` 갈래는 `semantic(row::Error::Unsupported)`를 돌려준다.
  `structural_prepare.rs:54`에 따르면 이는 `unchanged(Prepare, RowSemantic(Unsupported))`, 곧 stage 전 Unchanged
  거절이다. `structural_effect.rs`의 갈래는 `check_split_debt`와 `SystemWorkRef` 생성 뒤, stage 앞에 있다.
  `structural_prepare.rs`의 갈래는 `read_split_before` 대신 오는 것이라 pin이나 read를 쥐지 않는다. 지금은 닿지
  않는 갈래다(split `Observation`은 split candidate만 만든다).
- **선택 3(`extend` 전제):** 맞음. `anchor.rs:176–180`이 (file, page_no) 정렬과 기존 목록과의 물리 page 비중첩을
  적고, 위반하면 encoder가 InvalidInput으로 거절한다고 적는다. 1회차에서 확인한 동작과 같다.
- **선택 5(README):** 맞음. "B01..B06·B08, B07·B09는 일부, 재시작·BaseOnly capture는 U2"로 바뀌었다.
- **계약의 split `decode_layout` 문장:** 맞음. 코드(`structure.rs` `decode_layout`)는 84,744·84,640이 아니면
  Corrupt이고, 그 길이에서 opcode 3·4이면 Unsupported다. UNL-F01의 `structural::decode_layout(unlink frame) ==
  Corrupt` 단언과도 맞는다.
- **U2 dispatch 문장:** 실현 가능하다. 두 문법의 길이 {84,744, 84,640}과 {35,248, 19,032}는 서로 겹치지 않는다.
  잘못된 opcode는 각 decoder가 다시 거절한다. split 길이에 opcode 3·4가 오면 split decoder의 `!is_split()`가,
  unlink 길이에 opcode 1·2가 오면 unlink decoder의 `!is_unlink()`가 Unsupported로 거절한다.
- **선택 1(`check_debt` 시험):** U2의 할 일로 계약 1441–1443행에 기록되었다("NotNeeded보다 먼저 거절"). U2 검토에서
  확인한다.
- **선택 4·6:** review 파일을 둔 뒤 하기로 했다. 실제로 커밋된 `document-validation.json`은
  `dev3-tree-smo.md`의 sha가 현재 파일과 다르다(`d66ec962…` 대 실제 `6dd32b78…`). 나머지 문서 sha는 맞는다. 다시
  생성할 때 함께 맞춰진다.

## 필수

없음.

## 선택

1. **XRB H002..H016의 방법 기록.** 계약 표는 "시험마다 자기 ID 전부를 `XADB_XRB_INCLUDE`로 주고"라고 적고, 그
   결과로 `xrb_header_rejections` H002..H016 XB_PASS를 적는다. 그런데 `cases`는 첫 실패에서 `resume_unwind`로
   멈춘다. 그래서 H001..H016 전부를 준 실행은 H001에서 끝나고 H002..H016은 돌지 않는다. 작성자 log(`xrb-H.log`)도
   XB_PASS가 0개다. 그 PASS는 `XADB_XRB_INCLUDE=XB-H002..XB-H016`으로 따로 돌린 결과여야 하며, 검토자가 그렇게 돌려
   15/15 PASS를 확인했다. 결과는 맞으므로 필수는 아니다. 다만 그 별도 실행을 한 줄 적어 두면 증거가 방법과 맞는다.
2. **H001 기존 실패의 후속.** U1 범위 밖이고 base에서도 같은 위치로 실패한다. 다만 Base 형식 version 2가
   Unsupported여야 한다는 XRB 기대와 `read_header_format`(Row가 아닌 mode에서 2를 `V2System`으로 받음)이 다르다.
   그 실패를 소유할 별도 항목(담당·재현 명령)을 남기기를 권한다. development-policy는 불안정·실패 시험에 결함·담당·
   재현 자료를 요구한다.

## 측정값

- cmd2: 59 passed / 1 ignored / 2,315 filtered out, 560.11s(이 사본, 다른 실행과 CPU를 나눠 씀)
- XRB(d1cb74f): 통과 ID 112개(C15 S16 G24 I8 E8 ST1 BC6 BA24 BB1 BI8 BST1) + H002..H016 15개, 실패 H001 1개,
  ignored 1개(RH)
- H001(b6dc3a2): XB_FAIL, `plan4_recovery_base_tests.rs:295:18`
- build 약 2.5분 ×2, clippy 1분 33초
- log: `scratchpad/review-u1/`의
  `r2-cmd2.log`, `r2-list.txt`, `r2-clippy.log`, `r2-docs.log`, `xrb-r2/`(summary.txt와 시험별 log),
  `xrb-base/H001.log`

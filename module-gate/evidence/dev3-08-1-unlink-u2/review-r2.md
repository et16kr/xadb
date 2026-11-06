Verdict: ACCEPT

# 독립 D2 재검토(2회차): 빈 leaf unlink U2

## 대상

- commit: `fa766b5` (local branch `u2-local`, parent `1a82225`). 이번 차이는 `git diff 1a82225 fa766b5`, 6개 파일,
  +282/−9다. 전체 변경 `git diff 98faf08 fa766b5` 안에서 읽었다.
- 1회차: `module-gate/evidence/dev3-08-1-unlink-u2/review-r1.md`(CONDITIONAL, 필수 1, 대상 `1a82225`). commit된
  파일은 검토자가 쓴 1회차 원본과 byte가 같다(`cmp`).
- 바뀐 파일은 다음과 같다.
  - 시험: `weights/unlink_tests.rs`(P08 doc comment 1행), `structural_replay_tests/unlink.rs`(R02 +31행)
  - 문서: `devdoc/README.md`, `devdoc/contracts/dev3-tree-smo.md`, `devdoc/document-validation.json`
  - 기록: `review-r1.md`
- 제품 코드는 바뀌지 않았다. `git diff --stat 1a82225 fa766b5 -- crates tests scripts Cargo.toml Cargo.lock`은 위
  시험 파일 2개만 보인다.
- 검토자는 이 코드를 작성하지 않았다. `/home/claude/xadb`은 읽기만 했다. 작업 트리는 깨끗하고 HEAD가 `fa766b5`다.
  - 시험은 `git archive fa766b5` 사본과 별도 `CARGO_TARGET_DIR`에서 했다.
  - 사본에 넣은 mutation probe 2개는 "확인 방법"에 따로 적었다. 두 probe 모두 끝난 뒤 파일을 commit blob과 같게
    되돌렸다(`cmp`).
  - target과 사본은 기록 뒤 지웠다.

## 결론

열린 필수는 **0건**이다.

1회차 필수 1(U2 시험의 실행 명령·결과 수 미기록)은 닫혔다.

- 명령 표가 commit되었다(`dev3-tree-smo.md:1544–1554`).
- 1회차에 지적한 SPL 범위 표기도 "SPL-A01..A06·A08·A10"으로 고쳐졌다. 고르지 않은 A07·A09의 사유도 함께 적혔다.
- 세 명령의 선택 수를 이번 사본에서 다시 셌고, 115 / 80(ignored 1) / 2로 같다.
- 바뀐 R02를 포함하는 cmd2를 다시 돌려 기록과 같은 79 passed·1 ignored를 얻었다.

선택 1·2·3·5·7의 반영은 맞다. 특히 새 R02의 교차 opcode 두 경우는 dispatch를 고정한다. dispatch를 opcode 기준으로
바꾼 mutant 두 개가 각각 새 assert 한 줄(258행·272행)에서 실패하고, 기존 R01·R03은 두 mutant 모두에서 통과한다.

미룬 선택 4·6의 사유는 받아들인다. 남은 것은 아래 선택 3건뿐이다.

## 확인 방법

읽은 것:

- `git diff 1a82225 fa766b5` 전체.
- 바뀐 계약 행과 절:
  - 시험 계획 UNL-L01..L08 행(1367행)
  - U2 결과의 처음 계산 서술(1508–1511행)
  - R02·R03 행(1526–1527행)
  - 새 명령 표(1544–1554행)
  - README 18–20행
- 새 R02 경우의 경로를 확인하려고 다시 읽은 코드(모두 `fa766b5`에서 1회차와 같은 blob):
  - `apply_frame`(`plan4_retention/recovery.rs:509–595`). dispatch 전에는 frame byte 164..166(version 2)과
    170..172(scope)만 읽는다. opcode(166..168)는 읽지 않는다.
  - `is_unlink`(`history/structure.rs:244–247`)
  - unlink `decode_layout`(`semantic_row/structure/unlink.rs:287–318`). 길이가 아니면 Corrupt(300행),
    `!is_unlink()`이면 Unsupported(313행)다.
  - split `decode_layout`(`semantic_row/structure.rs:309–336`). 길이가 아니면 Corrupt(322행), `!is_split()`이면
    Unsupported다.
  - 시험 harness `World::apply`·`frame`·`expectation`(`structural_replay_tests.rs:76–131`). `apply`(123행)는
    `observe_recovery_frame`을 그대로 부른다.
- `row_gate.rs:635–646`의 S1B 시험 doc comment(`/// C01: memory writer census`, `/// C02: native writer census`).
  계약 1510행의 "S1B 회계 C01·C02"가 가리키는 ID다.
- `documentation-policy.md#push-gate`와 `parallel-development-policy.md`의 XADB-LAYOUT-006(document-validation.json
  생성 규칙), README 154행(저장된 결과의 의미).

실행한 것과 결과(scratch 사본 `fa766b5`, `CARGO_TARGET_DIR` 분리, CATALOG·PLAN4_RECOVERY에 64자 자리표시 hash):

| 명령 | 결과 |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo test -p xadb-engine --lib --locked --no-run` | 빌드 3분 18초. libtest 전체 2,381건(1회차와 같다) |
| cmd2 filter에 `--list`, `--list --ignored` | **80**건, 그중 ignored 1(`structural_effect_native_worker`). 고른 시험 목록이 1회차 `list-cmd2.txt`와 줄 단위로 같다 |
| `unlink_replay::`(UNL-R01..R03) | **3 passed**, 2.15s. 바뀐 R02 포함 |
| `unl_p08`(P08 두 시험. 주석만 바뀜) | **2 passed** |
| cmd2 실행(`retention::history::structure:: … retention::owner_tests::`) | **79 passed, 0 failed, 1 ignored**, 2,301 filtered out, 1,128.2s. 계약 1551행의 기록과 같다 |
| cmd1·cmd3 | 다시 돌리지 않았다. 두 범위의 코드는 1회차 뒤 바뀌지 않았다(P08은 doc comment뿐이고 위에서 따로 돌렸다). 1회차에 115 passed / 2 passed였다(AGENTS.md) |
| mutation probe A: `is_unlink`를 `raw.len() >= 168 && matches!(u16(raw[166..168]), 3 \| 4)`로 바꾸고 `unlink_replay::` 실행 | R01·R03 통과. R02는 `unlink.rs:258`(unlink 길이 + opcode 1 → Unsupported 기대)에서 실패. 그 frame이 split decoder로 가서 길이 Corrupt가 된다 |
| mutation probe B: `is_unlink`를 "unlink 길이 또는 opcode 3·4"로 바꾸고 `unlink_replay::` 실행 | R01·R03 통과. R02는 `unlink.rs:272`(split 길이 + opcode 3 → Unsupported 기대)에서 실패. 그 frame이 unlink decoder로 가서 길이 Corrupt가 된다 |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS(경고 0, 2분 28초) |
| `python3 -B devdoc/verify_documents.py` | FAIL. 원인은 예상한 `module-gate/evidence/dev3-08-1-unlink-u2/review.md` 누락 링크 1건뿐이다. 다시 생성한 파일별 값은 commit된 `document-validation.json`과 2개 파일에서 다르다(선택 3) |

전체 suite는 돌리지 않았다(AGENTS.md). XRB executor 전용 시험은 돌리지 않았다. 두 commit 모두
`plan4_recovery_base`를 바꾸지 않는다.

1회차 필수 1의 처리:

- **닫힘.** 명령 표(1544–1554행)가 commit되었다. 1회차 검토의 재현 결과와 비교하면 다음과 같다.
  - 세 filter 문자열은 1회차에 돌린 filter와 글자가 같다.
  - 결과 수 115 passed·0 ignored / 79 passed·1 ignored / 2 passed가 맞다.
  - cmd1 설명 "U1의 113개 + UNL-R04 source·UNL-P08 Catalog 부채"가 맞다.
  - cmd2 설명의 SPL 범위는 "SPL-A01..A06·A08·A10·SPL-J01..J07·structural effect"로 고쳐졌다. "SPL-A07·A09는 U2
    code에 닿지 않아 고르지 않았다"는 사유도 적혔다. 이번 `--list`로 A07·A09가 고르지 않음을 다시 확인했다.
  - XRB 미실행 사유(1554행)가 맞다.

1회차 선택의 처리:

1. **README의 B09 표현과 UNL-L 행: 반영됨.**
   - README 18–19행이 "B07 재시작과 B09의 raw capture 대조. 제품 rebase `capture_base`는 U3의 재시작 read"로
     바뀌었다.
   - UNL-L01..L08 행(1367행)에 "WAL의 unlink frame을 지나는 제품 rebase(`capture_base`)의 목록·claim 보존(U2에서
     넘어온 B09 부분)"이 더해졌다. U3 검토가 시험 계획에서 그 의무를 바로 볼 수 있다.
2. **R03 문구: 반영됨.** 1527행이 "첫 split(RootSplit 70)은 before fixture의 상태"와 "decode한 목록 항목 [11, 17]
   유지"로 바뀌었다. 시험(`unlink.rs` R03)이 하는 일과 같다.
3. **P08 주석: 반영됨.** "(and before the tree audit)"이 지워졌다(`weights/unlink_tests.rs:442–444`). 남은 문장
   "before any unlink or NotNeeded decision"은 시험이 보이는 것(빈 leaf 21의 unlink, 행이 있는 leaf 13의
   NotNeeded)과 맞다. 다른 table의 Catalog Job은 거절하지 않는다는 반대 대조는 더하지 않았다. 1회차에서도 덧붙인
   제안이었으므로 다시 올리지 않는다.
4. **회계 목록의 빈칸: 미룸.** 값에 영향이 없다는 1회차 판단이 그대로다. 제품·시험 회계 코드가 바뀌지 않았으므로
   이전 측정값도 그대로다. 받아들인다.
5. **dispatch 교차 시험: 반영됨.** R02에 두 경우가 더해졌다(`unlink.rs:246–276`).
   - LeafUnlink frame(35,248B)의 opcode를 1로 바꾼 경우, 그리고 RootSplit frame(84,744B)의 opcode를 3으로 바꾼
     경우다.
   - 둘 다 `finish_semantic_envelope`로 다시 hash한다. record 수 인수는 원래 값(2·5)이므로 opcode 2 byte만 다르다.
   - 두 경우를 각각 새 world에 넣어 `RowSemantic(Unsupported)`와 digest·cursor 불변을 본다.

   이 판정은 길이 dispatch를 가려낸다. 두 decoder 모두 길이가 맞지 않으면 Corrupt, 길이는 맞고 opcode가 다른
   문법이면 Unsupported를 낸다. dispatch 전에는 opcode byte를 읽지 않는다. 그래서 Unsupported는 frame이 길이로 고른
   decoder에 갔을 때만 나온다. 위 probe A·B가 실행으로 이를 확인했다. 계약 1526행의 서술도 시험과 같다.
6. **`document-validation.json`: 미룸.** review.md를 둔 뒤 다시 생성한다는 작성자 계획을 받아들인다. 다만 이번
   commit에서는 파일별 값도 맞지 않는다. 아래 선택 3에 적었다.
7. **처음 계산의 실패 목록: 반영됨.** 1510행이 "S2W-C01·C02·C03·C06과 S1B 회계 C01·C02 실패"로 바뀌었다.
   - C01·C02는 `s1b_row_arm_census_memory`·`_native`의 doc comment ID다(`row_gate.rs:635,642`).
   - 1회차 probe(+1,096B)에서 실패한 시험 목록과 같다.

## 필수

없음.

## 선택

1. **cmd2 설명의 "UNL-R01..R04"**(`dev3-tree-smo.md:1551`). cmd2가 고르는 R04는 history 쪽
   (`…retention::history::structure::tests::unl_r04_*`) 하나뿐이다. source 쪽 R04
   (`storage::semantic_row::structure::unlink::source_tests::unl_r04_*`)는 cmd1이 고르고, cmd1 설명(1550행)에
   적혀 있다. 두 행을 함께 읽으면 틀리지 않는다. 그래도 "UNL-R01..R03·R04 history"처럼 적으면 행 하나만 읽어도
   정확하다.
2. **1회차 선택 4의 회계 목록 빈칸**은 그대로 남는다(미룸에 동의). 다음 회계 변경 때 `anchor_chain` 주석에
   `anchor::retired` decode를 적고, parser 목록에 `decode_core` 자체 frame을 적기를 권한다.
3. **`document-validation.json`이 이 commit의 문서와 맞지 않는다**(`devdoc/document-validation.json:11–14,
   467–470`). 1회차에는 파일별 `links`·`sha256`이 commit과 모두 같았지만, 이번에는 두 곳이 다르다.
   - `devdoc/README.md`: `ba18b13d…`는 `1a82225`의 blob 값이다. `fa766b5`의 README는 `124a2327…`다.
   - `devdoc/contracts/dev3-tree-smo.md`: `57bd7ca3…`는 `1a82225`(`0c118b72…`)의 값도 아니고 `fa766b5`
     (`4644964f…`)의 값도 아니다. commit되지 않은 중간본에서 생성한 값으로 보인다.

   README 154행과 XADB-LAYOUT-006에 따르면 저장된 결과는 그 실행 당시 입력에만 귀속한다. 그리고 push 검사는 임시
   사본에서 다시 생성한다. 그래서 push를 막지는 않는다. 또 1회차 선택 6에서 미룬 재생성 때 함께 바로잡힌다. 그 재생성은
   작업 트리가 아니라 review.md를 넣은 최종 commit의 `git archive` 사본에서 하기를 권한다.

## 측정값

- libtest 전체 2,381건. cmd2 선택 80건(ignored 1). 선택 목록은 1회차와 같다.
- cmd2 1,128.2s(79 passed, 1 ignored). `unlink_replay::` 2.15s(3 passed). `unl_p08` 0.08s(2 passed).
- build 3분 18초, clippy 2분 28초, fmt 8.9초.
- probe A·B의 `unlink_replay::` 실행: 각각 2 passed·1 failed(R02, 258행 / 272행). 증분 빌드 포함 약 41초.
- 회계 값(unlink compare 24,876 / finishing 6,026, S1B 26,452, recovered 65,392/65,400, live Row lifecycle 40,881,
  C 여유 184B)은 1회차 측정과 같다. 회계 코드가 바뀌지 않았고, cmd2에 든 S1B·S2W·history R04가 이 값의
  `assert_eq`와 함께 통과했다.
- log: `/tmp/claude-0/-home-claude-xadb/484d24b6-02c4-5c37-84ef-21ff3507de06/scratchpad/review-u2/logs-r2/`
  - 시험·검사: `fmt.log`, `build.log`, `list-cmd2.txt`, `list-cmd2-ignored.txt`, `unlink-replay.log`, `p08.log`,
    `cmd2.log`, `clippy.log`, `docs.log`
  - probe: `probe-dispatch.diff`·`.log`, `probe-dispatch2.diff`·`.log`
  - `document-validation.committed.json`
  - 검토자 script: `/tmp/claude-0/-home-claude-xadb/484d24b6-02c4-5c37-84ef-21ff3507de06/scratchpad/review-u2/scripts/`의
    `run-r2.sh`, `probe-dispatch.sh`, `probe-dispatch2.sh`
  - build target(약 3.9GB)과 사본은 기록 뒤 지웠다.

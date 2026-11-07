# DEV3 08-3 O1 module-gate record 독립 검토 3회차 (D2, mapping)

## 대상

- 브랜치: `claude/project-thread-y6amas`, HEAD `ac0795e`.
- record: `module-gate/records/20261107-dev3-08-3-o1.json`, blob `5dcfe95353fe0b774d834959daff37041f998caa`.
- `cfac98c`..`ac0795e`에서 바뀐 것은 record 하나다. crates와 devdoc은 그대로다.
- record에서 바뀐 것은 네 곳뿐이다. 스크립트로 비교했다.
  - group [6]의 `paths`
  - group [0]·[2]·[7]의 reason
  - 사례 122개(이름 120개), na, reviews, change는 `cfac98c`와 같다.
- 시험 파일 다섯 개의 head blob은 2회차와 같다.

| 파일 | head blob |
| --- | --- |
| `crates/engine/src/storage/semantic_row/ledger_tests.rs` | `d8c16145d3dbb114fbdf3deb4b205b632c061cf0` |
| `crates/engine/src/storage/retention_page/row/source_tests.rs` | `97d1e793cb710493fbfca37d5eb4d01471f110f9` |
| `crates/engine/src/storage/plan4_recovery_base/row/graph/source_tests.rs` | `cffd31782fe8cd6c709add3238d8023d65cd9436` |
| `crates/engine/src/storage/semantic_row/transition_delete_tests.rs` | `62255821879777437c6d87c8d0016600e516c9d2` |
| `crates/engine/src/storage/semantic_row/undo_tests.rs` | `0a5117c8266d439b475d3413161ee98724c9fd27` |

## 검토 방법

- `git diff cfac98c..HEAD`로 record의 바뀐 줄을 읽었다.
- T3 문장을 확인하려고 `retention_page/row.rs`의 `source_bytes` 식을 읽었다. 선택된 사례 하나를 `--nocapture`로 돌려 출력값도 봤다.
- 선택된 사례 120개를 같은 환경과 같은 target dir에서 `--exact`로 다시 돌렸다.
- HEAD에서 gate를 돌렸다.
- 저장소를 scratchpad에 `--no-hardlinks`로 clone했다. 그 clone에서 조정자가 말한 review 항목을 그대로 넣고 gate를 돌렸다. 원래 저장소는 건드리지 않았다.

## (a) 네 곳의 수정

- **R2([6] paths)**: 맞다. `paths`가 `[ledger.rs, ledger_tests.rs]`가 됐다. 다른 group처럼 자기 test 파일을 덮는다. R2는 닫혔다.
- **T1([0])**: 맞다. `unl_c01`는 `check > walk`만 확인하고 독립 측정이 없다. 새 문장이 이것을 그대로 쓴다.
- **T2([7])**: 맞다.
  - `opcode_effect`에 닿는 undo 사례만 INSERT 분기를 보인다고 쓴다.
  - 두 사례를 이름으로 적는다. `undo_release_cursor_material_and_same_generation_free`는 role 검사에서 끝나고, `undo_rejects_operations_without_undo_native`는 Unsupported로 먼저 끝난다. 둘 다 코드와 맞다.
- **T3([2])**: 결론은 맞지만 이유가 틀리다. 이것은 내 2회차 T3 문장이 r1 S6의 표현을 그대로 옮긴 탓이다.
  - 식은 `root_decode_source_bytes().max(frames + pair_and_cell.max(hashing))`이다. 새 `validate_root` 항은 `pair_and_cell` 안에 있다.
  - `hashing`은 2·112 + 4·32 + 1,536 = 1,888 B다.
  - `pair_and_cell`은 일부 항만 더해도 1,888 B를 넘는다. 아래 크기는 모두 타입 정의에서 그대로 나온다.
    - `2·IntoIter<(usize,u64),14>` = 480
    - `3·RemainingBudget` = 336
    - `8·PageRef` = 192
    - `4·Option<TxnRef>` ≥ 192
    - `4·Option<StatusSlotRef>` ≥ 160
    - `4·Option<UndoRef>` ≥ 160
    - `2·Option<(UndoRef,UndoRef)>` ≥ 160
    - `4·Option<[PageRef;2]>` ≥ 192
    - `2·Enumerate<IntoIter<PageRef,2>>` ≥ 144
    - 이것만 더해도 2,016 B다.
  - 그래서 안쪽 `max(hashing)`은 새 항을 가리지 않는다.
  - 가리는 것은 바깥 `root_decode_source_bytes()`다. `row_codec_source_bounds_cover_measured_frames`를 `--nocapture`로 돌리면 `root_decode=6540 … whole=6540`이 나온다.
  - 고칠 문장: "…with the new validate_root term, which the outer max with root_decode_source_bytes() (6,540 B) hides, so its output is unchanged."
  - 근거가 되는 사례와 판정에는 영향이 없다. gate에도 영향이 없다(아래 U1).

## (b) 모의 gate

- HEAD `ac0795e`를 그대로 돌리면 오류는 24개다. 모두 이 record의 review가 없어서 생긴다.
  - `rust_tests[2]` 1개, `[7]` 16개, `[8]` 1개가 "case/fixture inputs lack same-record review at head blobs"다.
  - `[6]` 1개는 "changed paths lack same-record review"다. ledger_tests.rs가 이제 `paths`에 있기 때문이다.
  - "no scoped CI evidence"가 4개다(ledger.rs, ledger_tests.rs, prepare.rs, undo.rs).
  - "review missing: ledger_tests.rs needs D1"가 1개다.
- clone에서 `ac0795e` 위에 다음을 넣고 commit했다.
  - review 항목 하나를 넣었다.
    - `id`는 `dev3-08-3-o1-record`, `class`는 `D2`, `scopes`는 `["policy"]`다.
    - `verdict`는 `ACCEPT`, `required_open`은 0이다.
    - `author`와 `reviewer`는 서로 다르게 했다.
    - `files`에는 위 다섯 blob을 넣었다.
    - `log`에는 세 evidence 파일과 그 sha256을 적었다.
  - `module-gate/evidence/dev3-08-3-o1/`에 `record-check.md`, `record-check-r2.md`, `record-check-r3.md`를 더했다. r3는 자리 채움 파일이었다.
- 결과: `check --base cb48f49 --head HEAD`가 **PASS (0 error(s), 0 pending)**다.
- evidence 파일을 더해도 새 오류는 없다. gate는 review의 `log` 내용을 해시로 검사하지 않는다. 그래도 sha256은 실제로 넣는 파일의 값과 맞춰야 한다.

## (c) 그 밖의 확인

- 사례 실행: **120 passed, 0 failed, 0 ignored, 2351 filtered out** (2.51 s). 코드와 사례 목록이 2회차와 같으니 결과도 같다.
- review 항목의 `id`는 `dev3-08-3-o1-record`로 해야 한다. 기존 D2 review의 log가 "reviewed in dev3-08-3-o1-record"라고 쓰기 때문이다.
- 다른 문제는 찾지 못했다.

## 필수 지적

- 없다. R2는 닫혔다.

## 권고

- **U1.** [2]의 T3 문장을 위 (a)의 고칠 문장으로 바꾼다. 지금 문장은 안쪽 `max(hashing)`이 가린다고 쓰지만 사실은 바깥 `root_decode_source_bytes()`가 가린다. 결론("output is unchanged")은 맞다.
- **U2.** `ac0795e`에서 record 파일 끝의 줄바꿈이 빠졌다(`\ No newline at end of file`). gate는 상관없다. 다음에 다시 만들 때 넣으면 된다.

## 판정

**ACCEPT** — required_open 0.

- R1(1회차)과 R2(2회차)는 닫혔다.
- U1·U2는 권고다. 사례, 경로, N/A 판정에는 영향이 없다.
- 조정자가 말한 review 항목을 넣으면 gate가 0 error로 통과한다. clone 모의 실험으로 확인했다.

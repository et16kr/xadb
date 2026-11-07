# DEV3 08-3 O1 module-gate record 독립 검토 2회차 (D2, mapping)

## 대상

- 브랜치는 `claude/project-thread-y6amas`이다.
  - HEAD `cfac98c`는 record commit이다. record blob은 `b3633b4d4e56996ffffe0708b32a7f07ef99239f`이다.
  - HEAD~1 `cde4166`은 ledger 시험 하나와 계약 §14 한 줄을 더한다.
- 1회차 이후 바뀐 것은 셋이다.
  - `ledger_tests.rs`에 `ovf_f02_ledger_returned_chain_needs_the_terminal`이 생겼다. 새 blob은 `d8c16145d3dbb114fbdf3deb4b205b632c061cf0`이다.
  - record를 다시 만들었다. 사례 항목은 122개이고 이름은 120개다. `ovf_f07_new_codec_frames`가 [1]·[3]·[5]에 세 번 나온다.
  - 계약 §14의 §4.3 행이 새 시험을 이름으로 적는다.
- 이번 회차에 head blob으로 읽은 시험 파일:

| 파일 | head blob | 상태 |
| --- | --- | --- |
| `crates/engine/src/storage/semantic_row/ledger_tests.rs` | `d8c16145d3dbb114fbdf3deb4b205b632c061cf0` | 바뀜 (cde4166) |
| `crates/engine/src/storage/retention_page/row/source_tests.rs` | `97d1e793cb710493fbfca37d5eb4d01471f110f9` | 안 바뀜, 새로 선택됨 |
| `crates/engine/src/storage/plan4_recovery_base/row/graph/source_tests.rs` | `cffd31782fe8cd6c709add3238d8023d65cd9436` | 안 바뀜 |
| `crates/engine/src/storage/semantic_row/transition_delete_tests.rs` | `62255821879777437c6d87c8d0016600e516c9d2` | 안 바뀜 |
| `crates/engine/src/storage/semantic_row/undo_tests.rs` | `0a5117c8266d439b475d3413161ee98724c9fd27` | 안 바뀜 |

- 안 바뀐 넷은 모두 `origin/main`과 blob이 같다.
- 1회차에 본 바뀐 Rust 경로 26개는 blob이 그대로다. 목록은 맨 끝에 있다.

## 검토 방법

- `git diff HEAD~2..HEAD~1`로 새 시험과 계약 줄을 읽었다.
- 새 시험이 고치는 fixture cell 네 개를 Python으로 풀었다. Status 상태도 풀었다.
- `ledger.rs`에서 OrderMismatch를 내는 곳을 모두 따라갔다. 고친 칸(`@285`, `@376`)이 어느 검사에 닿는지 봤다.
- record의 group 9개와 N/A의 reason을 1회차 S1–S7과 맞춰 봤다.
- 사례 실행: 같은 환경과 같은 target dir에서 이름 120개를 `--exact`로 돌렸다.
  - 결과: **120 passed, 0 failed, 0 ignored, 2351 filtered out** (2.88 s).
  - 새 두 사례(`ovf_f02_ledger_returned_chain_needs_the_terminal`, `row_codec_source_bounds_cover_measured_frames`)도 통과했다.
- gate: `check --base origin/main --head HEAD`를 돌렸다. 오류는 35개다(아래 R2).
- 저장소 밖에서 모의 실험을 했다. 저장소를 scratchpad에 `--no-hardlinks`로 clone했다.
  - 그 clone에서만 record를 고치고 commit했다. 그리고 `--base cb48f49 --head HEAD`로 gate를 돌렸다.
  - 원래 저장소는 건드리지 않았다. 모의 스크립트는 `sim/patch.py`에 있다.

## 1회차 지적의 처리

### R1(ledger.rs 바뀐 동작): 닫혔다

- 새 시험이 고치는 cell은 fixture에서 모두 RootReleasingCompensated, cursor 0, effect 0, `@285 = 0`이다. chain 칸은 zero이고 `last_step == release_start == 40`이다.
  - 대상은 case 6 job 2, case 11 job 3, case 14 job 1·2다.
  - 시험도 이 전제를 먼저 `assert_eq!`로 확인한다.
- Status는 TS cell `@6`으로 확인했다. case 6은 Active(1), case 11은 Committed(3), case 14는 Aborted(4)다. 시험 설명과 맞다.
- 반환된 chain: `@285 = 1`, `@376 = release_start + 1`로 고친다.
  - decode 뒤 `chain()`은 None이고 `cleanup_started()`는 참이다. 시험이 이것을 확인한다.
  - budget은 그대로다. chain이 없는 Releasing root의 `cleanup()`은 inline과 같기 때문이다.
- case 6에서 OrderMismatch가 나는 곳은 RootReleasingCompensated arm 하나뿐이다. 이유는 다음과 같다.
  - `transaction_remaining`은 `observe`와 `remaining`만 부른다.
  - `remaining`은 BudgetMismatch·IdentityMismatch만 낸다.
  - `observe`의 다른 OrderMismatch는 sequence·mutation·statement·state·head를 본다. 고친 두 칸은 이것들에 닿지 않는다.
  - `latest_step`은 `validate_transaction`의 descriptor 검사에서만 쓴다. 이 시험은 그것을 부르지 않는다.
- 그래서 옛 식 `release_cursor() != 0`으로 돌리면 case 6이 Ok가 되어 첫 assertion이 실패한다. 조정자가 말한 mutant 결과와 맞다. 나는 mutant를 직접 돌리지는 않았다.
- Committed·Aborted 곁의 통과는 옛 식과 새 식이 같은 쪽이다. 경계의 반대편을 보이는 대조로 맞다.
- chain을 쥔 root도 대조로 맞다.
  - 고친 칸은 anchor `page(10)`, root `page(41)`, total 16,093, budget `(5, 281,168)`이다.
  - 281,168 = 4·55,528 + 59,056, 곧 `4R + CHAIN_RETIRE_WAL`이다.
  - job_tests의 `chain_root`와 같은 값이고 `page()` 도우미도 같다.
  - Active 곁에서 통과한다. cursor 0과 chain 있음이면 두 식 모두 거짓이다.
- [6]의 reason은 사실과 맞다. 계약 §14의 새 문장도 시험 내용과 맞다.

### S1–S7

- S1: 닫혔다. `ovf_f07`가 [1]·[3]에 들어갔다. gate는 중복을 group 안에서만 보므로 세 번 나와도 오류가 아니다.
- S2: 닫혔다. `row_codec_source_bounds_cover_measured_frames`가 [2]에 들어갔다.
  - 이 사례는 `retention_page::row::source_bytes()`를 직접 부르고 `whole <= catalog_whole`을 본다.
  - 새 `validate_root` 항은 계산된다. 하지만 r1 S6과 §14에 적힌 대로 `max(hashing)`에 가려 출력은 같다. reason은 "calls … with the new term"이라고만 쓰므로 틀리지 않다(아래 T3).
- S3: [5]·[8]은 닫혔다. 둘 다 "measured side has no new term"이라고 쓴다. [0]에는 아직 "UNL-C01 re-measures"가 남아 있다(아래 T1).
- S4: 대부분 닫혔다. DELETE 분기의 근거를 `prepare::op_effect`와 cross-check observer로 적었다. 하지만 "the undo cases show the INSERT branch"는 undo 사례 두 개에는 맞지 않는다(아래 T2).
- S5: 닫혔다. [3]이 기존 row_record 사례를 oracle로 적는다.
- S6: 닫혔다. [8]이 "소비자의 role 7 거절은 여기서 시험하지 않는다, OVF-C05(O2)"라고 쓴다.
- S7: 닫혔다.
  - "O2 (OVF-C04)"로 고쳤다.
  - durable replay와 C가 준비한 live 그룹을 나눠 적었다.
  - `RowSemantic(Unsupported)`에서 `RetentionCause::Unsupported`로 바뀐 원인과 그 시험이 없다는 것을 적었다.

## 필수 지적

### R2. 바뀐 경로 `ledger_tests.rs`가 어느 group의 `paths`에도 없다

- cde4166 뒤로 `crates/engine/src/storage/semantic_row/ledger_tests.rs`는 바뀐 Rust 경로다.
- gate는 바뀐 경로를 group의 `paths`(`covered |= paths`)나 N/A로만 덮는다(`check_module_gate.py:1392`). 사례의 `file`은 덮지 않는다.
- [6]의 `paths`는 `ledger.rs` 하나다. 다른 group은 자기 test 파일을 `paths`에 넣는다(anchor_tests, overflow_tests, job_tests, wire_tests 등).
- 그래서 지금 gate는 "no scoped CI evidence for … ledger_tests.rs"를 낸다. 이 오류는 review를 더해도 남는다.
- clone에서 모의로 확인했다.
  - 변형 A: 다섯 시험 파일을 `files`에 둔 D2 ACCEPT review만 더했다. 결과는 FAIL 1개다. 남은 오류는 "no scoped CI evidence for ledger_tests.rs"다.
  - 변형 B: A에 더해 [6]의 `paths`에 `ledger_tests.rs`를 넣었다. 결과는 **PASS (0 error)**다.
- 고침: [6]의 `paths`에 `crates/engine/src/storage/semantic_row/ledger_tests.rs`를 넣는다.

## 권고 (필수 아님)

- **T1.** [0]의 reason은 아직 "UNL-C01 re-measures walk_source_bytes"라고 쓴다.
  - `unl_c01`는 `check > walk`만 확인하고 값을 출력한다. 독립 측정이 없다.
  - [5]·[8]처럼 "checks its bound with the +1 B term"으로 고치기를 권한다.
- **T2.** [7]의 "the undo cases show the INSERT branch unchanged"는 두 사례에 맞지 않는다.
  - `undo_release_cursor_material_and_same_generation_free`는 role 검사에서 짧게 끝나 `opcode_effect`를 부르지 않는다.
  - `undo_rejects_operations_without_undo_native`는 :114-123의 Unsupported로 먼저 끝난다.
  - 두 사례를 빼거나 "모듈 회귀용, 바뀐 줄에 닿지 않음"이라고 적기를 권한다.
- **T3.** [2]의 새 사례 설명에 "새 항은 `max(hashing)`에 가려 출력이 같다"를 더하면 정확하다.
- 참고(범위 밖): `devdoc/document-validation.json`의 `dev3-row-overflow.md` sha256(`836b6d5c…`)은 d69a65b 판의 값이다.
  - HEAD 판(`5286700b…`)과 다르고, cde4166 직전 판과도 이미 달랐다.
  - 이 파일을 읽는 script나 workflow는 찾지 못했다. gate와 이 검토 범위 밖이다.

## gate 결과

- HEAD의 오류는 35개다.
  - `rust_tests[2]` 1개, `[6]` 12개, `[7]` 16개, `[8]` 1개가 "case/fixture inputs lack same-record review at head blobs"다.
  - "no scoped CI evidence"가 4개다. ledger.rs, prepare.rs, undo.rs는 review가 없어서 생긴다. **ledger_tests.rs는 R2 때문에 생긴다.**
  - "review missing: ledger_tests.rs needs D1 at blob d8c16145d3db"가 1개다.
- 예상과 다른 것은 ledger_tests.rs의 "no scoped CI evidence" 하나다. 나머지는 예상대로 review가 없어서 생긴다.
- 기존 D2 review의 log는 "reviewed in dev3-08-3-o1-record"라고 쓴다. 그런 review 항목은 아직 record에 없다. 그 항목을 더해야 log 문장도 참이 된다.
  - 그 항목은 D1 이상이어야 한다. D2이면 된다. ledger_tests.rs에 그 조건이 걸려 있다.
  - `files`에는 위 표의 blob 다섯 개를 넣는다.

## 판정

**CONDITIONAL**

- R1은 닫혔다. 새 시험과 reason은 정확하다.
- 새 필수 R2가 1개 열려 있다. [6]의 `paths`에 `ledger_tests.rs`를 넣는다.
- 그 다음 위 다섯 blob을 담은 same-record D2 review(ACCEPT, required_open 0)를 더한다. 모의 실험에서 이 두 가지로 gate가 PASS했다.
- 그렇게 하면 ACCEPT로 본다. T1–T3은 권고다.

## 검토한 파일의 head blob 전체 (HEAD `cfac98c`)

| 파일 | head blob |
| --- | --- |
| `crates/engine/src/storage/anchor.rs` | `c8be688ef004866834f4682c504e9ab0ae925857` |
| `crates/engine/src/storage/anchor_tests.rs` | `ebfaee651f446afb715324c52bb27e6ea29979f8` |
| `crates/engine/src/storage/overflow.rs` | `ed8d7279a5945920283c6dab5ad15f06da732b9d` |
| `crates/engine/src/storage/overflow_tests.rs` | `2b8a13a99877c87e1d52e04fad0f100789a6524b` |
| `crates/engine/src/storage/plan4_recovery_base/image_records.rs` | `0ce3ef29790a76a5bacc9baf5bb8dcc6efc16f67` |
| `crates/engine/src/storage/plan4_recovery_base/row_overflow_tests.rs` | `e716b83897c28dddc0d5fabed4f289c12ab19635` |
| `crates/engine/src/storage/plan4_recovery_base/row_tests.rs` | `1a9d99f167df96ac90e4efc8ca28d742e6f9d2b9` |
| `crates/engine/src/storage/retention_page/row.rs` | `d21f319ed4bc66df3a44c2795cb5006382ed672c` |
| `crates/engine/src/storage/retention_page/row/census.rs` | `147f88ed9a27a474775807a83351b9b899e6f029` |
| `crates/engine/src/storage/retention_page/row/job.rs` | `a1a5c443f46e2d50d2906a1c780c0ad3f8a2c901` |
| `crates/engine/src/storage/retention_page/row/job_tests.rs` | `4ac3432fa341afc1a081bdca4b0b0228c34c128d` |
| `crates/engine/src/storage/row_record.rs` | `f58dc456dce3a42da7b9ae966dc79a474e928135` |
| `crates/engine/src/storage/row_record/stream.rs` | `979796c49e1d102e8e8836b82aa5f5c37ea4b395` |
| `crates/engine/src/storage/row_record/stream_tests.rs` | `d4b83196c84773a0d7c4745adaad7037b90a7f77` |
| `crates/engine/src/storage/row_record_tests.rs` | `4c877feaa24d4a30515b05ea2ae0481b896d3c9b` |
| `crates/engine/src/storage/semantic_row/core.rs` | `a790ff08b6f811177ba36a7c22781069e6378d70` |
| `crates/engine/src/storage/semantic_row/ledger.rs` | `6c000047b266394486f947440b313d778a551ddf` |
| `crates/engine/src/storage/semantic_row/ledger_tests.rs` | `d8c16145d3dbb114fbdf3deb4b205b632c061cf0` |
| `crates/engine/src/storage/semantic_row/operation.rs` | `155462fbba2835e9fee37f60a10f3afe11330c77` |
| `crates/engine/src/storage/semantic_row/transition/prepare.rs` | `3c294bd9890409ab98a62302fa617cca557f22d9` |
| `crates/engine/src/storage/semantic_row/transition/source.rs` | `6eb3145b0f04341a8bbc6ebee4bbdbc9ccef5a34` |
| `crates/engine/src/storage/semantic_row/transition_tests.rs` | `0a470ee62b00c519a4dffb53d9c7e50e6f551360` |
| `crates/engine/src/storage/semantic_row/undo.rs` | `6f665ff909a51b7d8f80818d7a85c45fbe6c7fe9` |
| `crates/engine/src/storage/semantic_row/wire.rs` | `1f2b8c9a0ac8b7a9fe9d2d2a1f19ac94ccfc0dde` |
| `crates/engine/src/storage/semantic_row/wire_tests.rs` | `f7c778c1f1523b7a5c1205cd6351cfedea3c1935` |
| `crates/engine/src/storage/wal_io/plan4_retention/history/row.rs` | `d389ab9264c69b15316e81720ab583010d95a7c6` |
| `crates/engine/src/storage/wal_io/plan4_retention/visibility/row_retirement.rs` | `f07f9190912732b6e162c0c012e08502716a7a95` |
| `crates/engine/src/storage/retention_page/row/source_tests.rs` | `97d1e793cb710493fbfca37d5eb4d01471f110f9` |
| `crates/engine/src/storage/plan4_recovery_base/row/graph/source_tests.rs` | `cffd31782fe8cd6c709add3238d8023d65cd9436` |
| `crates/engine/src/storage/semantic_row/transition_delete_tests.rs` | `62255821879777437c6d87c8d0016600e516c9d2` |
| `crates/engine/src/storage/semantic_row/undo_tests.rs` | `0a5117c8266d439b475d3413161ee98724c9fd27` |
| `module-gate/records/20261107-dev3-08-3-o1.json` | `b3633b4d4e56996ffffe0708b32a7f07ef99239f` |

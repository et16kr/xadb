# DEV3 08-3 O1 module-gate record 독립 검토 (D2, mapping)

## 대상

- record: `module-gate/records/20261107-dev3-08-3-o1.json`. HEAD `09a96cc`의 blob은 `9c37a07738392436dc6f71fa34e6fe6f5b06bfcb`다.
- 코드 diff: `origin/main`(`cb48f49`)..`HEAD~1`(`8747538`). 바뀐 Rust 경로는 26개다.
- record 내용: `rust_tests` 9개 group, 사례 118개. `na` 1개, 경로 2개.
- 범위: 경로와 사례의 연결만 본다. 코드가 맞는지는 r1–r4 D2(ACCEPT)를 따른다.
- 바뀌지 않은 사례 파일 4개를 head blob에서 읽었다. 넷 다 `origin/main`과 blob이 같다.

| 파일 | head blob |
| --- | --- |
| `crates/engine/src/storage/plan4_recovery_base/row/graph/source_tests.rs` | `cffd31782fe8cd6c709add3238d8023d65cd9436` |
| `crates/engine/src/storage/semantic_row/ledger_tests.rs` | `48b308ef04664962e9905698feb5a594fc736d11` |
| `crates/engine/src/storage/semantic_row/transition_delete_tests.rs` | `62255821879777437c6d87c8d0016600e516c9d2` |
| `crates/engine/src/storage/semantic_row/undo_tests.rs` | `0a5117c8266d439b475d3413161ee98724c9fd27` |

## 검토 방법

- group마다 `git diff origin/main..HEAD~1 -- <paths>`를 읽었다. 선택된 사례 본문도 읽었다.
- 바뀐 함수의 호출자를 따라가서 사례가 바뀐 줄을 실제로 지나는지 봤다.
- ledger fixture(`tests/fixtures/module-row-format/ledger/jobs.bin`)를 Python으로 풀었다. 각 case의 Job state, cursor `@121`, `@285`를 봤다.
- N/A: `ChunkWrite`·`OverflowCancel`·`ChainRetire`와 opcode 14·15·25를 crates와 fixture author에서 grep했다.
- 사례 실행: 주어진 환경에서 `cargo test -p xadb-engine --lib -j 2 -- <118 names> --exact --test-threads 2`를 돌렸다. target dir은 `/home/user/xadb-target`이다.
  - 결과: **118 passed, 0 failed, 0 ignored, 2352 filtered out** (2.59 s).
  - 이름 118개는 모두 다르다. manifest의 `executor_only`에 걸리는 사례는 없다.
- gate: `python3 scripts/check_module_gate.py check --base origin/main --head HEAD`.
  - 오류는 31개다. `rust_tests[6]` 11개, `[7]` 16개, `[8]` 1개가 "case/fixture inputs lack same-record review at head blobs"다.
  - 나머지 3개는 ledger.rs, prepare.rs, undo.rs의 "no scoped CI evidence"다. 앞의 오류 때문에 생긴다.
  - 예상한 실패뿐이다. 다른 실패는 없다.

## group별 판단

- **[0] anchor**: 충분하다.
  - `ovf_f03`가 Overflow 항목, revision 1의 Overflow 전용 목록, revision 1에서 tree 항목이 섞인 목록의 거절, n = 55를 본다.
  - UNL-P01..P04는 바뀐 `check_retired`를 지나는 회귀 사례다.
  - `unl_c01`는 `walk_source_bytes`(+1 B)를 계산한다. 하지만 값을 고정하지 않고 관계만 본다.
- **[1] overflow**: codec 쪽은 충분하다.
  - 고정 literal 8개와 `ovf_f01`가 새 bytes, verifier, inspect, segment walk를 본다.
  - `overflow::source`(inspect, segment_walk, segment_push, content_digest, hasher_digest, verifier_push)에 닿는 사례는 `ovf_f07_new_codec_frames` 하나다. 이 사례는 [5]에만 있다(권고 S1).
- **[2] Job3·census·row.rs**: 충분하다.
  - OVF-F02 사례 3개가 pending, chain, `cleanup_started`, `forward_wal`·`forward_steps`, `cleanup()`의 chain 분기를 부른다.
  - `ovf_f02_returned_chain_starts_cleanup`는 `verify_census`를 부른다. 그래서 census.rs의 두 규칙을 overflow root로 지난다. TerminalLeased는 OrderMismatch이고, TerminalReady에서는 last_step 45가 거절되고 55가 통과한다.
  - 기존 job3·census 사례는 inline root에서 값이 그대로인 것을 보인다.
  - `retention_page/row.rs`의 `source_bytes` 항은 [2]의 어느 사례도 부르지 않는다. 간접 경로는 있다. [5]의 n1이 transition 모델의 `ledger_observe`·`reservation`을 거쳐 이 값을 계산한다(권고 S2).
  - `CHUNK_WRITE_WAL`·`CHAIN_RETIRE_WAL`의 cfg(test) re-export는 `ovf_f04_chain_shapes_and_frame_equations`가 쓴다.
- **[3] row_record·stream**: 충분하다.
  - stream_tests 7개, `ovf_f05_materialized_row_against_catalog_schema`, `materialized_overflow_requires_digest_and_schema`가 바뀐 줄을 지난다.
  - 나머지 기존 row_record 사례 12개 정도는 바뀐 row_record.rs 줄을 지나지 않는다. 이들은 stream helper가 비교하는 전체 encoder literal(oracle)을 검산한다. 이 역할로는 받아들일 수 있다(권고 S5).
  - `row_record::source`와 `stream::source`도 `ovf_f07`만 부른다(권고 S1).
- **[4] wire·operation·core**: 충분하다.
  - 기존 wire 사례도 다시 짠 `decode_layout`(shape 분기, native kind, redo kind 검사)을 지난다. 그래서 회귀 사례로 의미가 있다.
  - OVF-F04 사례 4개가 새 opcode, shape, `list_from_native` 손상, core 규칙(planned, refs 1, cursor < 64)을 본다.
  - `ovf_f04_chain_opcodes_decode_role_outer_handoff`는 opcode 1..=25 전부에서 `delete()`가 `8 | 9 | 23`일 때만 참임을 확인한다. 이 표가 [7]의 근거를 보탠다.
- **[5] transition source.rs**: 바뀐 줄은 지난다.
  - n1은 `layout_decode`(+64 B)를 계산한다. 하지만 시험 쪽 `measured::frame_callee`에는 새 항이 없다. 그래서 n1은 늘어난 모델이 옛 측정값을 덮는다는 것만 보인다(권고 S3).
- **[6] ledger.rs**: 회귀는 덮지만 바뀐 동작이 비어 있다. 아래 필수 R1이다.
- **[7] undo.rs·prepare.rs**: 충분하다.
  - 기존 opcode에서는 옛 식과 새 식이 같다. `role()`은 DELETE·UPDATE만 바꾸고 `update()`를 먼저 본다. 값이 달라지는 것은 OverflowCancel(role Cancel) 하나다. 옛 식은 Delete, 새 식은 Insert를 낸다.
  - 호출자는 네 곳이다: undo.rs:148, prepare.rs:1786, leaf.rs:100, jobs.rs:82→101. 넷 다 role이 Forward·Clr·Prune일 때만 이 값을 쓴다. 그래서 OverflowCancel의 값은 아무도 쓰지 않는다.
  - DELETE 분기는 `transition_delete_tests` 7개가 지난다. 넷은 history D를 돌리고 셋은 `history_d_with`를 쓴다.
    - `world.observe`·`produce`는 `prepare::prepare`로 가서 `op_effect`에 닿는다. DeleteForward ×3, DeleteClr, DeletePrune ×2와 INSERT Forward·Prune으로 닿는다.
    - `cross_check`는 `undo::observe`(:148), `jobs::observe`, `leaf::observe`를 DELETE op로 부른다. 그래서 undo.rs의 `opcode_effect`도 참 분기로 지난다.
  - `undo_tests`는 INSERT 분기만 지난다. fixture에 op 2·4·20·11·21·22만 있고 DELETE가 없다. 두 사례는 바뀐 줄에 닿지 않는다(권고 S4).
- **[8] image_records.rs**: codec 쪽은 충분하다.
  - `row_overflow_tests` 3개가 certificate role(format 3, object ≥ 6, level, 고정 ordinal), role 7의 각 칸, `pair()`(role 3·4·7), `chunk()`를 본다.
  - `base4_closure_source_bound_covers_measured_frames`는 `provenance_source_bytes`를 직접 부른다. catalog·row layout census를 거쳐서도 부른다.
  - 하지만 시험 쪽 `birth_chains`에는 role 7 분기가 없다. 그래서 새 +152 B는 독립 측정과 비교되지 않는다(권고 S3).
  - Base4 소비자가 role 7과 Overflow certificate를 거절한다는 것은 시험이 없다. 소비자 코드는 바뀌지 않았지만 이제 이 경로에 닿을 수 있다. r1 S1이 짚었고 OVF-C05로 미뤘다(권고 S6).
- **na (history/row.rs, row_retirement.rs)**: 정직하다.
  - `history::row::apply`는 recovery.rs:566의 durable replay에서만 불린다. 새 arm은 `match op`의 맨 앞에 있다. 이전 arm은 바뀌지 않았다.
  - 그런 그룹을 쓰는 producer는 없다.
    - transition의 `Change`에는 chain 변형이 없다.
    - prepare.rs:321은 `RowPayload::Overflow`를 Unsupported로 거절한다.
    - A는 첫 효과 전에 overflow 행을 거절한다.
  - 14·15·25 frame을 만드는 시험은 wire_tests(`decode_layout`만)와 transition_tests(`graph::inventory`)뿐이다. wal_io 시험이나 fixture author에는 없다.
  - `row_retirement::scope`는 C가 준비한 live 그룹의 `retirement_observation()`에서 불린다(:122, :192). C는 chain 그룹을 준비할 수 없다. 그래서 이 arm에도 닿지 않는다. 표현만 고칠 곳이 있다(권고 S7).

## 필수 지적

### R1. ledger.rs의 바뀐 동작(overflow root)에 닿는 사례가 없다

- 바뀐 줄: `ledger.rs:156`, RootReleasingCompensated arm. `job.release_cursor() != 0`이 `job.cleanup_started()`로 바뀌었다.
- overflow가 아닌 root에서는 두 식이 같다. 선택된 사례가 두 결과에 모두 닿는다. fixture로 확인했다.
  - `row_ledger_literal_states_and_residuals`는 19개 case를 다 돈다.
    - case 6·8·10·11·14·18: Compensated root, cursor 0. 조건이 거짓이다.
    - case 12·13·15: cursor > 0, terminal Status. 조건이 거짓이다.
  - `row_ledger_rollback_and_cleanup_states`는 case 6(non-terminal)의 `bank[2]`를 `jobs(13)[2]`(cursor 1)로 바꾼다.
    - 두 cell은 `@121`, `@376`, budget 칸에서만 다르다.
    - 그래서 앞의 identity·순서 검사를 지나고 이 arm의 참 분기에서 OrderMismatch가 난다.
- 값이 달라지는 입력은 하나다. overflow root(`@285 = 1`)이고, chain 칸이 zero(ChainRetire로 반환됨)이고, cursor가 0인 경우다.
  - 옛 코드는 non-terminal Status에서도 이것을 통과시켰다. 새 코드는 OrderMismatch로 거절한다.
  - 이 입력을 ledger observe(`transaction_remaining`, `validate_transaction`)에 넣는 사례는 없다.
  - ledger fixture에는 `@285 = 1`인 cell이 없다. 저장소에서 `[285]`를 쓰는 시험은 job_tests뿐이고, job_tests는 `verify_census`만 부른다.
- 계약 §14는 이 ledger 규칙을 O1 구현으로 적고 시험을 "OVF-F02 3개"라고 쓴다. 하지만 그 세 사례는 ledger.rs에 닿지 않는다.
- 그래서 [6]은 "inline root는 그대로"만 보인다. 이 변경이 하려는 일은 CI 증거가 없다.
- 고침(권함): ledger_tests에 사례를 하나 더한다. job_tests가 쓰는 cell 수정 방식을 그대로 쓴다.
  - case 6의 Compensated root `[2]`에 `@285 = 1`, `@376 > release_start`를 넣는다. `residual(6, …)`이 `Err(OrderMismatch)`인지 본다.
  - 같은 수정을 terminal case(예: 14)에 넣으면 통과하는지 본다.
  - chain이 남은 root(cursor 0, `[200,248)`·`[288,292)`·budget)도 non-terminal에서 통과하는지 본다.
  - 그 사례를 [6]에 넣는다. 바뀐 ledger_tests.rs를 review에 넣는다.
- 차선: [6]의 reason과 §14 "O2에 남긴 것"에 이 분기가 O1에서 시험되지 않는다고 분명히 적는다. 시험 ID(OVF-C 계열)도 적는다.
  - r1의 S1·S2를 O2로 미룬 전례와 같은 꼴이다. O1에는 이런 root를 쓰는 경로가 없다.
  - 이렇게 하면 R1은 닫힌 것으로 본다.

## 권고

- **S1.** 다음 source 모델은 [5]의 `ovf_f07_new_codec_frames`만 부른다: `overflow::source`(overflow.rs), `row_record::source`(row_record.rs), `row_record::stream::source`(stream.rs).
  - [1]과 [3]의 사례로는 닿지 않는다.
  - `ovf_f07`를 [1]·[3]에도 넣거나, 세 경로를 [5]의 `paths`에 더하기를 권한다. 그래야 group 하나를 바꿔도 이 증거가 조용히 빠지지 않는다.
- **S2.** [2]의 reason에는 `retention_page/row.rs` `source_bytes`의 새 항이 빠져 있다.
  - 이 모듈의 bound 시험 `storage::retention_page::row::source_tests::row_codec_source_bounds_cover_measured_frames`를 [2]에 넣기를 권한다.
- **S3.** [5]와 [8]의 reason은 "re-measures"라고 쓴다. 하지만 시험 쪽 측정 모델에는 새 항이 없다.
  - `measured::frame_callee`에는 native closure·list 항이 없다. `birth_chains`에는 role 7 분기가 없다.
  - 그래서 두 사례는 늘어난 모델이 옛 측정값을 덮는다는 것만 보인다. 값은 출력되고 §14에 적혀 있다.
  - reason을 "모델 값을 다시 계산하고 기존 측정 chain과 비교한다"로 고치기를 권한다. 측정 모델에 새 분기를 더하면 더 좋다.
- **S4.** [7]의 reason은 "undo cases show the DELETE branch unchanged"라고 쓴다.
  - 하지만 `undo_tests`에는 DELETE fixture가 없다. INSERT 분기만 지난다.
  - `undo_release_cursor_material_and_same_generation_free`는 role 검사에서 짧게 끝나서 `opcode_effect`를 부르지 않는다. `undo_rejects_operations_without_undo_native`는 :114-123에서 Unsupported로 먼저 끝난다.
  - DELETE 분기의 근거가 transition_delete_tests의 `cross_check`(undo·jobs·leaf observer)라고 적기를 권한다. 닿지 않는 두 사례는 빼거나 회귀용이라고 적기를 권한다.
- **S5.** [3]의 기존 row_record 사례는 대부분 바뀐 줄을 지나지 않는다.
  - reason에 "stream helper가 비교하는 전체 encoder literal의 검산"이라고 역할을 적기를 권한다.
- **S6.** [8]의 reason이나 record의 change 문장은 "Base4 consumers refuse role 7"이라고 쓴다. 이 주장은 시험이 없다.
  - r1 S1이 짚었고 OVF-C05로 미뤘다.
  - reason에 "소비자 거절은 시험하지 않았고 OVF-C05에서 연다"고 적기를 권한다.
- **S7.** na reason의 표현을 고치기를 권한다.
  - "O2c"는 devdoc 어디에도 없다. 계약에는 O2와 OVF-C04만 있다. "O2 (OVF-C04)"로 고친다.
  - "reached only by a durable … group"은 history/row.rs에만 맞는다. row_retirement.rs는 C가 준비한 live 그룹으로 닿는다. 두 경로를 따로 적는다.
  - replay의 거절 원인은 `RowSemantic(Unsupported)`에서 `RetentionCause::Unsupported`로 바뀌었다(§14, r1 S2). 시험이 없다는 점도 함께 적는다.

## 판정

**CONDITIONAL**

- 필수 1개(R1)가 열려 있다. R1을 고치거나 위의 차선대로 분명히 적으면 ACCEPT로 본다.
- 절차 조건: 위 표의 head blob 4개를 이 record의 `reviews[].files`에 넣고 gate를 다시 돌린다.
  - 그러면 남은 31개 오류가 풀려야 한다.
  - R1에서 ledger_tests.rs를 고치면 그 파일은 바뀐 경로가 된다. 새 blob으로 review해야 한다.
- 다른 group과 na는 연결이 맞고 충분하다. 118개 사례는 head에서 모두 통과했다.

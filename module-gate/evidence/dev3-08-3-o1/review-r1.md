Verdict: CONDITIONAL

# 08-3 O1 독립 검토 (D2, 1차)

## 대상

- worktree `/home/user/xadb-083`, branch `wip-083-o1`.
- diff `928ec85..9d23f38`. commit 7개, 파일 33개.
- 계약: `devdoc/contracts/dev3-row-overflow.md` 3판. §4, §6.1, §8.3, §10(OVF-F01..F06), §11의 O1 행, §13의 (O1) 행, §14 구현 기록.
- 개정된 정본: `dev3-overflow-format.md`, `dev3-tree-smo.md`, `dev3-module-consumers.md`, `dev3-create-recovery-base.md`, `README.md`.

## 검토 방법

- diff 전부를 읽었다. 계약 문장마다 코드 위치를 찾아 맞대었다.
- 정수를 손으로 다시 셈했다. 모두 코드와 frames-r2.py에 맞는다.
  - ChunkWrite: 3,776 + 3,904 + 49,296 = 56,976.
  - OverflowCancel: 3,776 + 3,456 + b + a + 208 + 49,032 = 56,472 + b + a. 최대 60,864.
  - ChainRetire: 2,176 + 3,456 + b + a + 49,032 = 54,664 + b + a. 최대 59,056.
  - P 16,344. INIT 16,448. entry Patch 160. R 55,528.
  - F = 91,548 + 128h. CLR = 91,340 + 128h. cleanup = 312,096 + 128h. 4R + X = 281,168.
  - C = 244. `size_of::<Shape>() == 40`.
- 닫혀 있어야 할 경로를 따라갔다.
  - replay는 14·15·25를 거절한다.
  - Row retirement 관측은 None이다.
  - transition observe는 Change 불일치로 거절한다.
  - ReserveAdopted는 @285 = 0을 다시 계산하므로 byte 비교에서 거절된다.
  - Base4 소비자는 role 7을 `relations` provenance에서 ProvenanceMismatch로 거절한다. Overflow certificate·allocation은 Unsupported로 거절한다.
  - SMO audit은 Overflow XTR1 항목을 `decode_tree_page` 오류로 거절한다.
- decoder의 panic 경로를 찾았다. 입력으로 닿는 panic은 없다.
  - `history/row.rs:439`, `operation.rs:220`, wire redo의 `unreachable!`는 앞에서 막힌다.
  - `pending()`·`chain()`·`chunk()`의 `expect`는 `decode_job`·`validate_provenance` 검사 뒤에만 불린다.
- 시험과 도구를 돌렸다. 바뀐 코드의 시험만 돌렸다.
  - `cargo test -- ovf_ overflow_ stream`: 35 PASS.
  - 이웃 시험: 203 PASS. `XADB_*SHA256`은 'a' 64자로, `RUST_MIN_STACK=33554432`로 두었다.
  - census 계열: 322 PASS, 5개는 돌리지 않았다. `xrb_bridge_source_layout`, `xrb_global_census_rejections`, `xrb_partial_source_layout`, `retention_census_inputs`, `g5_bind_boundaries`다. 이 다섯은 `XADB_XRB_INCLUDE`, `XADB_G6_VECTOR_INDEXES`, `XADB_PLAN4_G5_CAPACITY_PADDING`이 있어야 돈다. §14의 기록과 같다.
  - census 출력: UNL-C01 walk 409·check_list 587, N1 frame callee 4,752, Base4 layout 20,384·catalog layout 19,744·grammar bound 2,404, peak 59,675. §14와 같다.
  - `cargo clippy -D warnings`: 깨끗하다.
  - `verify_documents.py`: PASS. 이 도구가 `devdoc/document-validation.json`의 날짜만 바꿨다. `git checkout`으로 되돌렸다. worktree는 깨끗하다.
  - `python3 -I module-gate/evidence/dev3-08-3-o1/overflow_literal.py`: 개정 전 literal을 재현한다. 개정 값(cell header, checksum `6f1967a9…`, 전체 `4c6ecd86…`)이 `overflow_tests.rs`의 literal과 같다.

## 필수 지적

### R1. role 7의 index·count·total과 chunk cell의 일치를 아무도 보지 않는다. 정본은 본다고 쓴다.

- 위치:
  - 계약 `devdoc/contracts/dev3-row-overflow.md:308`.
  - `crates/engine/src/storage/plan4_recovery_base/image_records.rs:293-295`.
  - `crates/engine/src/storage/plan4_recovery_base/row.rs:188-194`.
  - `devdoc/contracts/dev3-create-recovery-base.md:77`.
- 무엇이 틀렸나:
  - §8.3은 "index·count·total은 chunk page의 cell과 같아야 한다"고 한다. 이것은 O1의 Base4 provenance codec 범위다.
  - `validate_provenance`의 (7, 2) 분기는 이 검사를 하지 않는다. 주석은 "the layout's check"라고 미룬다.
  - 그런데 layout(`row.rs` `decode_layout`)도 하지 않는다. `row.rs:188`에서 image가 손에 있지만 `row.rs:194`의 `validate_provenance` 뒤에 cell 비교가 없다. 저장소 어디에도 없다.
  - §14는 이 미룸을 적지 않았다.
  - 정본 `dev3-create-recovery-base.md:77`은 현재형으로 "index·count·total이 chunk cell과 같은지는 layout이 본다"고 쓴다. 지금 코드에서 이 문장은 거짓이다.
- 왜 중요한가:
  - 지금 소비자가 role 7을 거절하므로 바로 생기는 손상은 없다.
  - O2가 소비자를 열 때 이 검사가 있다고 믿는다. 정본 문장과 코드 주석이 그렇게 말하기 때문이다. 그러면 cell과 다른 claim이 통과한다.
- 기대하는 고침 (둘 중 하나):
  - `row.rs` `decode_layout`에서 role 7이면 `inspect_overflow_page`로 image의 cell을 읽고 index·count·total을 비교한다. 다르면 ProvenanceMismatch를 낸다. 음성 시험을 더한다.
  - 또는 이 검사를 O2로 미룬다고 §14에 적는다. 그리고 `dev3-create-recovery-base.md:77`의 문장과 `image_records.rs:295`의 주석을 "O2에서 layout이 본다"로 고친다.

### R2. OVF-F04의 "inventory action" 시험이 없다.

- 위치:
  - 계약 `devdoc/contracts/dev3-row-overflow.md:345`.
  - `crates/engine/src/storage/semantic_row/transition/graph.rs:176-182`.
- 무엇이 틀렸나:
  - §10의 OVF-F04는 "role·inventory action"을 시험 대상으로 적는다.
  - `inventory()`는 `operation.role()`로 action을 고른다. 그래서 OverflowCancel은 action 3이 되고, ChunkWrite·ChainRetire는 InvalidInput이 된다.
  - 이것을 확인하는 시험이 없다. 지금 시험은 `role()` 값과 shape의 items(I = 1)만 본다. `inventory()`가 내는 action byte는 아무도 보지 않는다.
- 왜 중요한가:
  - §10에 있는 O1 시험이 빠졌다.
  - `inventory()`가 opcode로 분기하도록 바뀌면, OverflowCancel이 조용히 InvalidInput이 되거나 다른 action을 받을 수 있다. 그것을 잡을 시험이 없다.
- 기대하는 고침:
  - transition 시험에 직접 시험을 더한다.
    - OverflowCancel이면 `delta[0] == 3`, `delta[1] == 1`, item이 `[16,112)`에 있다.
    - ChunkWrite·ChainRetire이면 `Err(InvalidInput)`이다.
  - §14의 OVF-F04 행을 고친다.

### R3. §11 O1 적립 "새 codec 함수의 frame"이 새 함수에 대해 없다.

- 위치:
  - 계약 `devdoc/contracts/dev3-row-overflow.md:370`, `:423`.
  - `crates/engine/src/storage/row_record/stream.rs:42`, `:58`, `:148-200`.
  - `crates/engine/src/storage/overflow.rs:352`, `:385-450`. `seen: [Option<PageRef>; 9]`가 `:390`에 있다.
  - `crates/engine/src/storage/row_record.rs:616`.
- 무엇이 틀렸나:
  - §11은 O1의 적립을 "새 codec 함수의 frame"으로 정한다.
  - §14(`:423`)는 이것을 "새 지역값이 생긴 기존 함수 넷의 source 모델"로만 풀었다.
  - 아주 새로 생긴 codec 함수에는 frame 모델도 크기 기록도 없다. 그 함수는 다음과 같다.
    - `canonical_row_digest_borrowed`, `encode_canonical_window_borrowed`, `CanonicalRowStream`.
    - `inspect_overflow_page`, `OverflowSegmentWalk`.
    - `decode_materialized_row_with_schema`.
  - 기존 `decode_materialized_row`(`row_record.rs:606-612`)도 이제 새 함수를 한 겹 더 거친다. 이것도 기록이 없다.
- 왜 중요한가:
  - O2·O3의 census는 여유가 작다. A drain 여유는 14 B, preflight C는 182 B, C lifecycle은 605 B다.
  - 그 census는 이 함수들의 frame을 callee로 더해야 한다. O1이 적립을 남기지 않으면 O2가 이 크기를 처음 재게 된다. 계약이 O1에 둔 일이 옮겨진다.
- 기대하는 고침 (둘 중 하나):
  - 새 함수마다 explicit `size_of` 합의 `source_bytes`(또는 같은 꼴의 크기 기록)를 더한다. census 시험이 그 값을 찍게 한다. §14에 값을 적는다.
  - 또는 계약 주인이 "첫 호출자의 census(O2·O3)에서 센다"를 받아들이게 한다. 그리고 그것을 §11 또는 §14에 분명히 적는다.

## 권고

### S1. Base4 decode 경로로 role 7을 거절하는 음성 시험

- `plan4_recovery_base/row_overflow_tests.rs`는 `validate_provenance`를 직접 부른다.
- Overflow page와 role 7 claim이 든 layout을 decode 끝까지 넣는 시험이 없다. 거절하고 panic이 없는지를 보는 시험이다.
- §14(`:412`)와 `dev3-create-recovery-base.md:77`은 이것을 주장한다. 거절 위치는 `catalog/graph/relations.rs:223-224`와 `row/current_tree.rs:195`다. 시험으로 고정하기를 권한다.

### S2. replay 거절 원인의 시험

- `wal_io/plan4_retention/history/row.rs:352`는 14·15·25를 `RetentionCause::Unsupported`로 거절한다.
- O1 전에는 원인이 `RowSemantic(Unsupported)`였다. 원인이 바뀌었는데 시험이 없다.
- 세 opcode가 `RetentionCause::Unsupported`를 내는 시험을 더하기를 권한다.

### S3. `CanonicalRowStream`의 Catalog schema 시험

- `stream_tests.rs`의 OVF-F05 여섯 개는 모두 `RowSchemaInput::Columns`만 쓴다.
- B의 copy는 Catalog schema로 읽는다(`row_record.rs:614-615`의 주석). Catalog 입력에서도 결합 decoder와 결과가 같은지 시험하기를 권한다.

### S4. WAL 상수를 shape에 묶는 const assert

- `retention_page/row/job.rs:120-121`의 `CHUNK_WRITE_WAL`(56,976)·`CHAIN_RETIRE_WAL`(59,056)은 손으로 쓴 값이다.
- `semantic_row/operation.rs`의 shape와 묶는 const assert가 없다. 지금 값은 맞다. 한쪽만 바뀌면 조용히 어긋난다.

### S5. `ledger.rs:238`의 forward debt

- `semantic_row/ledger.rs:238`은 `debt(1, job.forward_wal())`를 쓴다.
- overflow root에서는 이 값이 `forward_steps()`와 맞지 않는다. 고치는 일은 O2다.
- §14의 O2 남김 목록에 이 줄을 적기를 권한다.

### S6. callee frame을 따로 세지 않은 곳

- `cleanup()`(`job.rs:352`)은 `chain()`(`job.rs:235`)을 부른다. `chain()`은 `Option<ReturnChain>`과 `Result<PageRef>` 둘을 둔다.
- 이 frame을 부르는 쪽은 `validate_budget`, `retention_page/row/census.rs`의 `verify_terminal`, ledger observe다. 셋 다 이 frame을 따로 세지 않는다.
- 지금은 max·sum의 여유가 덮는다. `retention_page/row.rs:158-166`의 retention 모델 출력은 `max(hashing)`에 가려 값이 바뀌지 않는다.
- 이것이 덮였을 뿐이라는 점을 §14에 적기를 권한다.

### S7. 오류 종류의 계약 문장

- 계약 `:102`는 "나머지를 Corrupt로 거절한다"고 쓴다.
- `job.rs` `validate_root`의 overflow root 검사는 일부를 OrderMismatch·IdentityMismatch로 낸다. 기존 decoder의 관례와는 맞는다.
- 계약 문장을 코드에 맞추기를 권한다. 예: "나머지를 거절한다(Corrupt·IdentityMismatch·OrderMismatch)".

## NIT

### T1. wire 시험 주석

- `semantic_row/wire_tests.rs:1846`의 주석은 "no after side" anchor native 경우를 말한다.
- 그 아래에 (Some, None) 경우가 없다. 경우를 더하거나 주석을 고친다.

### T2. replay의 두 번째 `unreachable!`

- `history/row.rs:439`의 `unreachable!`는 닿지 않는다.
- `decode_layout` 바로 뒤에서 14·15·25를 거절하면 이 줄이 필요 없다.

### T3. `OverflowSegmentWalk::finish`의 반환형

- `overflow.rs:444`는 `&[Option<PageRef>]`를 돌려준다.
- 성공이면 모든 칸이 `Some`이다. 호출자가 다시 풀어야 한다.

### T4. OVF-F03의 "kind별 retire LSN"

- codec 단계에서는 두 kind가 같은 `1..=anchor_lsn` 규칙을 쓴다. 그래서 이 항목은 O1에서 비어 있다.
- kind별 검사는 O2다. 시험 이름이나 §14에 그렇게 적는다.

## 판정 근거

- 형식, 정수, 닫힌 경로, decoder 견고성은 계약에 맞는다. 입력으로 닿는 panic은 없다. 시험·clippy·문서 검증은 통과한다.
- `Shape`는 40 B 그대로다. census 출력의 차이는 §14가 적은 항뿐이다.
- §13의 O1 행은 모두 개정됐다. O2 행은 그대로다.
- 하지만 세 가지가 남는다.
  - R1: 정본 문장 하나가 거짓이다. 계약의 O1 검사 하나가 구현되지 않았고, 그 미룸이 기록되지 않았다.
  - R2: §10의 OVF-F04 시험 항목 하나가 빠졌다.
  - R3: §11의 O1 적립이 새 함수에 대해 없다.
- 셋 다 국소적으로 고칠 수 있다. 설계를 바꿀 일은 없다. 그래서 CONDITIONAL이다.
- R1~R3을 고치면(또는 R1·R3을 계약 주인의 기록된 미룸으로 바꾸면) ACCEPT로 본다.

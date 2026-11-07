Verdict: CONDITIONAL

# 08-3 O1 독립 검토 (D2, 2차)

## 대상

- worktree `/home/user/xadb-083`, branch `wip-083-o1`.
- diff `9d23f38..d69a65b`. commit 1개, 파일 14개.
- 1차 검토문은 `module-gate/evidence/dev3-08-3-o1/review-r1.md`에 있다. 내 scratchpad의 1차 파일과 byte 단위로 같다.
- 계약: `devdoc/contracts/dev3-row-overflow.md`. 바뀐 곳은 §4.3(:102), §10(:348 OVF-F07), §14(:419-434).
- 정본: `devdoc/contracts/dev3-create-recovery-base.md:77`.

## 검토 방법

- diff 전부를 읽었다. 구현자가 주장한 항목을 하나씩 코드와 맞대었다.
- 새 `source` 모델 세 묶음을 함수 본문과 줄마다 맞대었다. 셈 규칙은 이 저장소의 기존 census 모델에 견주었다.
  - `crates/engine/src/storage/overflow.rs:539-616`
  - `crates/engine/src/storage/row_record.rs:671-721`
  - `crates/engine/src/storage/row_record/stream.rs:351-427`
- `ContentHasher`의 크기를 쟀다. 작업 공간 밖 scratch binary로 쟀고, repo toolchain 1.95.0과 `target/debug/deps`의 rlib을 썼다. 결과는 112 B(`Sha256`과 같음)다. 추적 파일은 건드리지 않았다.
- Base4의 role 7 거절 경로를 다시 따라갔다.
  - `RowBaseView`는 `row/graph.rs:762-785`의 `decode`·`encode`에서만 나온다.
  - 둘 다 `validate`를 거친다. 거기서 `seam::provenance`가 `catalog/graph/relations.rs:645`의 `pair().ok_or(ProvenanceMismatch)`로 role 7을 거절한다.
  - "모든 Base4 소비자가 role 7을 거절한다"는 문장은 참이다.
- 새 codec 함수에 제품 호출자가 없는지 grep으로 확인했다. 대상은 `inspect_overflow_page`, `OverflowSegmentWalk`, `OverflowChainVerifier`, `decode_overflow_page`, `decode_materialized_row*`, digest, window, `CanonicalRowStream`이다. 시험 밖 호출자는 없다. §14의 "제품 경로는 없다"는 참이다.
- 시험·도구 (바뀐 코드의 시험만 돌렸다):
  - `cargo test -p xadb-engine --lib -- ovf_ overflow_ stream --nocapture`: 38 PASS. 1차의 35개에 새 시험 3개가 더해졌다.
  - OVF_F07 출력: page=938 canonical=762 sha=1536 inspect=1506 segment_walk=352 segment_push=249 verifier_push=1889 materialized=1920 materialized_columns=2176 input_len=309 digest=2004 window=621 stream=200 stream_push=288. §14(:424)의 값과 같다.
  - `-- semantic_row::wire semantic_row::transition_tests::ovf retention_page::row`: 51 PASS.
  - `-- census source _c0 bound row_record`: 342 PASS, 1 ignored.
    - 하네스 입력(`XADB_XRB_INCLUDE`, `XADB_G6_VECTOR_INDEXES`, `XADB_PLAN4_G5_CAPACITY_PADDING`)이 있어야 도는 5개는 `--skip`으로 뺐다. `xrb_bridge_source_layout`, `xrb_global_census_rejections`, `xrb_partial_source_layout`, `retention_census_inputs`, `g5_bind_boundaries`다.
    - `XADB_*SHA256`은 'a' 64자로 두었다. `RUST_MIN_STACK=33554432`.
  - `cargo clippy -p xadb-engine --lib --tests -- -D warnings`: 깨끗하다.
  - `python3 devdoc/verify_documents.py`: PASS. `document-validation.json`의 날짜만 바뀌어 `git checkout`으로 되돌렸다. worktree는 깨끗하다.
  - 시험 개수를 셌다. OVF-F04 5개(wire 4, transition 1), OVF-F05 8개(stream 7, row_record 1), OVF-F07 1개. §14와 같다.

## 필수 지적

### R1. 새 source 모델 셋이 digest의 hasher 사본을 세지 않는다. §14의 값 넷이 작다.

- 위치:
  - `crates/engine/src/storage/row_record.rs:708-712` (`materialized`). 본문은 `row_record.rs:621`의 `content_digest(bytes)`.
  - `crates/engine/src/storage/overflow.rs:605-614` (`verifier_push`). 본문은 `overflow.rs:504`의 `hasher.digest()`.
  - `crates/engine/src/storage/row_record/stream.rs:372-380` (`digest`). 본문은 `stream.rs:50`의 `hasher.digest()`.
  - 기록: `devdoc/contracts/dev3-row-overflow.md:424`.
- 무엇이 틀렸나:
  - `ContentHasher::digest`(`crates/foundation/src/codec.rs:35-37`)는 `self.0.clone().finalize()`다. 그 frame에 `Sha256` 사본 112 B가 있다.
  - `content_digest`(`codec.rs:41-43`)는 `Sha256::digest`다. 그 frame에도 `Sha256` 112 B가 있다.
  - 이 저장소의 census는 digest 하나를 "SHA closure + hasher + 입력 + 출력"으로 센다.
    - `plan4_recovery_base/row/graph/source.rs:55-58`: `&[u8] + ContentHasher + [u8; 32] + SHA`.
    - `retention_page/row/source_tests.rs:30-32`: `content_digest_frame`을 `+ SHA`와 따로 더한다.
    - `semantic_catalog/resources.rs:126`: page codec이 `2 * ContentHasher // content_digest -> digest return`을 센다. 이 page codec은 `inspect`가 쓰는 938이다.
  - 새 모델은 SHA closure(`sha`)만 더하고 이 hasher 항을 뺐다.
    - `materialized`: SHA 분기에 `content_digest`의 frame(16 + 112 + 32 = 160 B)이 없다. 1,920은 2,080이어야 한다. `&[Column]` 판 2,176은 2,336이어야 한다.
    - `verifier_push`: 지역 `hasher` 사본 하나만 센다. `digest()` 안의 두 번째 사본과 그 출력(8 + 112 + 32 = 152 B)이 없다. 1,889는 약 2,041이어야 한다.
    - `digest`: `hasher.digest()` 분기(8 + 112 + 32 = 152 B)가 callee max에 없다. `emit` 분기(약 92 B)만 있다. 약 60 B가 모자란다.
  - 같은 commit의 `inspect`는 hasher를 센 page codec 위에 서 있다. 그래서 새 모델 사이에서도 규칙이 다르다.
- 왜 중요한가:
  - §11은 이 frame을 O1의 적립으로 둔다. §14는 O2·O3가 이 값을 첫 호출자의 census에 그대로 더한다고 쓴다.
  - 그 census의 여유는 작다. A drain 14 B, preflight C 182 B, C lifecycle 605 B다. 60~160 B가 모자라면 통과와 실패가 뒤바뀔 수 있다.
  - 모델의 doc 주석은 "explicit sums of each frame's typed carriers"라고 한다. 본문의 hasher 사본은 typed carrier이므로 이 주석과도 맞지 않는다.
- 기대하는 고침:
  - 세 모델의 SHA 분기에 기존 규칙의 digest 항을 넣는다. 예: `size_of::<&[u8]>() + size_of::<ContentHasher>() + size_of::<[u8; 32]>() + sha`. `digest()` 호출은 `&ContentHasher`를 쓴다.
  - OVF-F07을 다시 돌려 §14(:424)의 값을 고친다.
  - 각 모델의 doc 주석에 hasher 항을 적는다.

## 권고

### S1. OVF-F07이 값을 고정하게 한다

- `semantic_row/transition_tests.rs:4342-4372`은 값을 찍기만 한다. 대소 관계 몇 개만 assert한다.
- §14가 그 값을 적어 두었다. 구조체 크기가 바뀌면 문서 값이 조용히 낡는다.
- `assert_eq!`로 값을 고정하거나, §14 값이 시험 출력이라는 점을 적기를 권한다.

### S2. 어느 값이 SHA closure를 품는지 §14에 적는다

- `inspect`(1,506)는 page codec 위의 값이다. 이 page codec은 SHA closure를 품지 않고, 기존 census는 SHA를 맨 위에서 한 번 더한다(`transition/frame.rs:383`).
- `verifier_push`·`materialized`·`digest`는 `sha` 1,536을 안에 품는다.
- §14는 이 값들을 나란히 적는다. O2 작성자가 SHA를 두 번 더하거나 빠뜨릴 수 있다. 값마다 SHA 포함 여부를 적기를 권한다.

### S3. role 7 cell 대조를 OVF-C05의 시험 항목에 넣는다

- §14(:430)는 이 대조를 O2로 미뤘다. 1차 R1의 둘째 고침이므로 받아들인다.
- §10 OVF-C05(:353)의 손상 목록에는 "index·count·total이 cell과 다름"이 없다. O2의 시험 의무로 남기려면 그 행에 넣기를 권한다.

### S4. 생성자의 Result 보유를 적는다

- `segment_walk()`·`stream()`은 상태 크기만 준다.
- `OverflowSegmentWalk::new`·`CanonicalRowStream::new`를 부르는 frame은 기존 `returned::<T, E>` 규칙대로 `Result<Self, _>`도 함께 가진다. 상태가 352·200 B이므로 이 항은 작지 않다.
- 호출자가 `returned`로 센다는 점을 모델 주석이나 §14에 적기를 권한다.

## NIT

### T1. closure 항의 불일치

- `overflow.rs:598`의 `segment_push`는 방문 집합 scan closure의 `(&PageRef, &PageRef)`를 센다.
- 같은 꼴의 scan을 하는 `verifier_push`(`overflow.rs:605-614`)는 세지 않는다. 둘을 맞춘다.

### T2. `push` 모델의 `consume`

- `stream.rs:406`의 `consume`은 `&[u8]`만 센다. `&mut self`와 `match self.field`의 `Field` 사본이 없다.
- 기존 `slack()`이 덮을 크기다. 그래도 주석이 "arguments"를 센다고 하므로 맞춘다.

### T3. OVF-F07 행의 "새 codec 함수"

- §10(:348)은 `OverflowChainVerifier::push`를 새 함수로 적는다. 이 함수는 O1 전부터 있었고 O1이 고쳤다. "새로 생기거나 바뀐"으로 고친다.

### T4. §4.3의 오류 종류 문장

- §4.3(:102)은 "주소 alias는 IdentityMismatch, LSN 순서는 OrderMismatch"라고 쓴다.
- 코드는 dynamic이 아닌 주소도 IdentityMismatch로 낸다(`retention_page/row/job.rs:584-591`, `:661-667`). chain 있는 root의 cursor ≠ 0도 OrderMismatch로 낸다(`job.rs:594`).
- "주소 규칙", "순서 규칙"처럼 넓히면 정확해진다.

## 판정 근거

- 1차 지적의 처리:
  - 1차 R1(role 7 cell 대조): 닫혔다. 미루는 고침을 골랐다. 다음 세 곳이 모두 O2로 고쳐졌다.
    - `image_records.rs:295-297`의 주석.
    - `dev3-create-recovery-base.md:77`.
    - §14 "O2에 남긴 것"(:430).
    - "그때까지 Base4 소비자는 거절한다"는 코드에서 참이다.
  - 1차 R2(inventory action): 닫혔다. `ovf_f04_inventory_action_follows_the_role`이 다음을 직접 본다.
    - OverflowCancel의 action 3, 개수 1, item `[16,112)`.
    - ChunkWrite·ChainRetire의 InvalidInput과 무변경 buffer.
    - 기존 세 role.
  - 1차 R3(새 codec 함수의 frame): 형식은 닫혔다. 모듈마다 `source` 모델이 생겼고 OVF-F07이 값을 찍으며 §14가 그 값을 적는다. 하지만 모델 셋의 값이 기존 셈 규칙보다 작다. 이것이 이번 R1이다.
- 1차 권고와 NIT의 처리:
  - S3(Catalog stream 시험): 반영됐다.
  - S4(WAL 상수와 shape의 묶음): 시험으로 반영됐다.
  - T1(`(Some(56), None)`): 반영됐다.
  - S7(§4.3 문장): 반영됐다. 남은 작은 부정확은 이번 T4다.
  - S1·S2·S5·S6·T4: §14 "O2에 남긴 것"에 적혔다. 적힌 내용은 코드와 맞는다.
- 이번 commit은 제품 경로를 바꾸지 않는다. 새 panic 경로는 없다. 시험·clippy·문서 검증은 통과한다.
- 남은 필수는 R1 하나다. 국소적인 모델 고침과 §14 값 갱신으로 닫힌다. 그래서 CONDITIONAL이다. R1을 고치면 ACCEPT로 본다.

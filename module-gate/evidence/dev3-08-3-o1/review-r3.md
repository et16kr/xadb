Verdict: CONDITIONAL

# 08-3 O1 독립 검토 (D2, 3차)

## 대상

- worktree `/home/user/xadb-083`, branch `wip-083-o1`.
- diff `d69a65b..11cd532`. commit 1개, 파일 6개.
- 2차 검토문은 `module-gate/evidence/dev3-08-3-o1/review-r2.md`에 있다. 내 scratchpad의 2차 파일과 byte 단위로 같다.
- 바뀐 코드:
  - `crates/engine/src/storage/overflow.rs:602-630`
  - `crates/engine/src/storage/row_record.rs:704-713`
  - `crates/engine/src/storage/row_record/stream.rs:369-412`
  - `crates/engine/src/storage/semantic_row/transition_tests.rs:4372-4393`
- 바뀐 계약: `devdoc/contracts/dev3-row-overflow.md`의 :102, :348, :353, :424.

## 검토 방법

- diff 전부를 읽었다. 구현자가 주장한 항목을 하나씩 코드와 맞대었다.
- 새 값을 2차의 값에서 손으로 다시 셈했다. 모두 맞다.
  - `content_digest(sha)` = 16 + 112 + 32 + 1,536 = 1,696.
  - `hasher_digest(sha)` = 8 + 112 + 32 + 1,536 = 1,688.
  - `materialized`: 자기 항 384 + max(1,696, 762) = 2,080. `&[Column]` 판은 +256이므로 2,336.
  - `verifier_push`: 자기 항 353 + closure 16 = 369. 여기에 max(update 24 + 1,536, 1,688) = 1,688을 더해 2,057.
  - `digest`: 자기 항 376 + max(309, emit 92 + 1,536, 1,688) = 2,064. 2차에서 내가 어림한 +60 B와 같다.
  - `push`: `consume`에 receiver 8과 `Field` 16을 더해 288 + 24 = 312.
- 새 항을 함수 본문과 맞대었다.
  - `content_digest`는 `foundation/src/codec.rs:41-43`과 같다.
  - `hasher_digest`는 `codec.rs:35-37`의 clone과 같다.
  - verifier의 update 분기는 `overflow.rs:503`의 `hasher.update(view.payload)`와 같다.
  - 기존 규칙과도 같다: `plan4_recovery_base/row/graph/source.rs:55-58`, `retention_page/row/source_tests.rs:30-32`.
- 새 모델 함수는 OVF-F07과 서로만 부른다. 기존 census 출력은 바뀌지 않는다(grep으로 확인).
- §14(:424)의 새 문장을 page codec 모델(`semantic_catalog/resources.rs:121-136`)과 SHA를 더하는 기존 census(`semantic_row/transition/frame.rs:383`, `transition_tests.rs:3054`, `:3075`)에 맞대었다.
- 시험·도구 (바뀐 코드의 시험만 돌렸다):
  - `cargo test -p xadb-engine --lib -- ovf_ overflow_ stream row_record:: semantic_row::wire --nocapture`: 72 PASS.
    - `XADB_*SHA256`은 'a' 64자로 두었다. `RUST_MIN_STACK=33554432`.
    - OVF_F07 출력: page=938 canonical=762 sha=1536 inspect=1506 segment_walk=352 segment_push=249 verifier_push=2057 materialized=2080 materialized_columns=2336 input_len=309 digest=2064 window=621 stream=200 stream_push=312.
    - 이 출력은 `assert_eq!`의 13개 값(`transition_tests.rs:4391`)과 §14(:424)의 값과 같다.
  - `cargo fmt --check`: 깨끗하다.
  - `cargo clippy -p xadb-engine --lib --tests -- -D warnings`: 깨끗하다.
  - `python3 devdoc/verify_documents.py`: PASS. `document-validation.json`의 날짜만 바뀌어 `git checkout`으로 되돌렸다. worktree는 깨끗하다.

## 필수 지적

### R1. §14의 "page codec 938에는 page digest가 이미 들어 있다"가 SHA closure에 대해 거짓이다.

- 위치: `devdoc/contracts/dev3-row-overflow.md:424`, 문장 "`inspect_overflow_page`의 page codec 938에는 page digest가 이미 들어 있다."
- 무엇이 틀렸나:
  - 같은 문단은 바로 앞에서 "digest 항"을 "입력 또는 receiver, `ContentHasher` 112 B, 32 B digest, SHA"로 정의한다.
  - 938은 `semantic_catalog/resources.rs:121-136`의 `page_codec`이다. 이것은 hash state(`2 * ContentHasher`)와 digest 출력만 센다. SHA closure 1,536은 없다.
  - 기존 census는 이 값 위에 SHA를 따로 한 번 더한다.
    - `transition_tests.rs:3054`: "without the SHA closure".
    - `transition_tests.rs:3075`: "one SHA closure and one measured callee chain".
    - `semantic_row/transition/frame.rs:383`: `+ super::source::SHA`.
  - 그래서 문단의 정의대로 읽으면 이 문장은 거짓이다. `inspect_overflow_page` 1,506에는 page digest의 SHA closure가 없다.
- 왜 중요한가:
  - 2차 S2는 SHA를 빠뜨리거나 두 번 세는 일을 막으려고 SHA 포함 여부를 적게 했다. 이 문장은 그 반대로 읽힌다.
  - O2 작성자가 이 문장을 믿고 `inspect`에 SHA를 더하지 않으면 1,536 B가 모자란다. 이 단계의 census 여유(14 B, 182 B, 605 B)보다 훨씬 크다.
- 기대하는 고침:
  - 문장을 사실대로 고친다. 예: "`inspect_overflow_page`의 page codec 938에는 page digest의 hash state와 출력이 들어 있고 SHA closure는 없다. 호출자 census가 기존처럼 SHA를 한 번 더한다."
  - 코드는 고칠 것이 없다.
  - `overflow.rs:539-541`의 모듈 주석("`page` is that census's page codec")에도 같은 한 줄을 적으면 좋다.

## 권고

### S1. digest 분기의 셈 꼴을 맞춘다

- `verifier_push`는 update 분기에 `(&mut ContentHasher, &[u8])` 24 B를 센다(`overflow.rs:629`).
- stream `digest`의 `emit() + sha` 분기는 closure와 `update`의 인자를 세지 않는다(`stream.rs:381-383`).
- 지금은 `hasher_digest` 분기(1,688)가 더 커서 값이 바뀌지 않는다(92 + 24 + 1,536 = 1,652 < 1,688). 그래도 두 모델의 꼴을 맞추면 나중에 `emit`이 커질 때 안전하다.

## NIT

### T1. §14의 생성자 예

- §14(:424)는 생성자 규칙의 예로 `OverflowHead::new`를 든다.
- 2차 S4가 짚은 것은 상태가 큰 `OverflowSegmentWalk::new`(352 B)와 `CanonicalRowStream::new`(200 B)다. 예를 이 둘로 바꾸면 더 분명하다.

## 판정 근거

- 2차 지적의 처리:
  - 2차 R1(digest hasher 항): 닫혔다.
    - 세 모델이 기존 census의 digest 항을 쓴다. `overflow::source::{content_digest, hasher_digest}`는 기존 규칙과 같은 꼴이다.
    - 새 값 넷(2,057, 2,080, 2,336, 2,064)은 내 손셈과 같다.
    - §14와 OVF-F07의 `assert_eq!`가 그 값을 함께 적는다.
  - S1(값 고정): 닫혔다. 13개 값을 `assert_eq!`로 고정한다.
  - S2(SHA 포함 여부): 대체로 반영됐다. 하지만 `inspect`에 대한 문장 하나가 거짓이다. 이것이 이번 R1이다.
  - S3(OVF-C05): 반영됐다(:353).
  - S4(생성자 Result): 반영됐다. 예만 이번 T1이다.
  - T1(closure 항): 반영됐다.
  - T2(`consume`): 반영됐다.
  - T3(OVF-F07 행): 반영됐다(:348).
  - T4(§4.3 오류 종류): 반영됐다(:102). `job.rs:584-594`, `:661-667`과 맞는다.
- 이번 commit은 제품 경로와 기존 census 출력을 바꾸지 않는다. 새 panic 경로는 없다. 시험·fmt·clippy·문서 검증은 통과한다.
- 남은 필수는 R1 하나다. 계약 문장 하나를 고치면 닫힌다. 그래서 CONDITIONAL이다. R1을 고치면 ACCEPT로 본다.

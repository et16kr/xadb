Verdict: ACCEPT

# 08-3 O1 독립 검토 (D2, 4차)

## 대상

- worktree `/home/user/xadb-083`, branch `wip-083-o1`.
- diff `11cd532..deef07d`. commit 1개, 파일 4개.
- 3차 검토문은 `module-gate/evidence/dev3-08-3-o1/review-r3.md`에 있다. 내 scratchpad의 3차 파일과 byte 단위로 같다.
- 바뀐 곳:
  - `crates/engine/src/storage/overflow.rs:539-543`: 모듈 주석.
  - `crates/engine/src/storage/row_record/stream.rs:382`: digest의 emit 분기.
  - `devdoc/contracts/dev3-row-overflow.md:424`: §14 census 문단.

## 검토 방법

- diff 전부를 읽었다. 구현자가 주장한 세 항목을 코드와 문서에 맞대었다.
- 3차 R1의 문장을 page codec 모델에 다시 맞대었다.
  - 모델은 `semantic_catalog/resources.rs:121-136`의 `page_codec`이다.
  - SHA를 더하는 곳은 `semantic_row/transition/frame.rs:383`과 `transition_tests.rs:3054`, `:3075`다.
- emit 분기의 값을 손으로 셈했다.
  - 고친 분기: 92 + 24 + 1,536 = 1,652.
  - `hasher_digest` 분기: 1,688.
  - 큰 쪽은 그대로 1,688이다. 그래서 digest 2,064는 바뀌지 않는다.
- 시험·도구 (바뀐 코드의 시험만 돌렸다):
  - `cargo test -p xadb-engine --lib -- ovf_ overflow_ stream row_record:: semantic_row::wire --nocapture`: 72 PASS.
    - `XADB_*SHA256`은 'a' 64자로 두었다. `RUST_MIN_STACK=33554432`.
    - OVF_F07 출력과 `assert_eq!`로 고정한 13개 값은 3차와 같다. §14와도 같다.
  - `cargo fmt --check`: 깨끗하다.
  - `cargo clippy -p xadb-engine --lib --tests -- -D warnings`: 깨끗하다.
  - `python3 devdoc/verify_documents.py`: PASS. `document-validation.json`의 날짜만 바뀌어 `git checkout`으로 되돌렸다. worktree는 깨끗하다.

## 필수 지적

없다.

## 권고

없다.

## NIT

없다.

## 판정 근거

- 3차 지적의 처리:
  - 3차 R1(page codec 938과 SHA): 닫혔다.
    - §14(:424)는 이제 938에 "page digest의 hash state와 출력만 있고 SHA closure는 없다"고 쓴다. 그리고 `inspect` 1,506도 SHA를 포함하지 않으며, 호출자 census가 SHA를 한 번 더한다고 쓴다.
    - 이 문장은 `resources.rs:121-136`의 모델과 기존 census가 SHA를 더하는 방식에 맞는다.
    - `overflow.rs:539-543`의 모듈 주석도 같은 말을 한다.
  - 3차 S1(digest 분기의 셈 꼴): 반영됐다. `stream.rs:382`의 emit 분기가 이제 `(&mut ContentHasher, &[u8])`를 센다. 이것은 `verifier_push`와 같은 꼴이다. 값은 바뀌지 않는다.
  - 3차 T1(생성자 예): 반영됐다. 예가 `OverflowSegmentWalk::new`·`CanonicalRowStream::new`가 되었다.
- 1차부터 3차까지의 필수 지적은 모두 닫혔다.
  - 1차 R1: role 7 cell 대조를 O2로 미루고 기록했다. OVF-C05의 손상 목록에도 들어갔다.
  - 1차 R2: inventory action 시험이 생겼다.
  - 1차 R3: 새 codec 함수의 source 모델과 OVF-F07이 생겼다.
  - 2차 R1: digest의 hasher 항을 넣었다. 값 넷을 고쳤다.
  - 3차 R1: SHA 포함 여부의 문장을 고쳤다.
- O2로 넘긴 일은 §14 "O2에 남긴 것"에 적혀 있다. 그 내용은 코드와 맞는다.
- 이번 commit은 주석, 문서, 값이 바뀌지 않는 모델 항 하나만 바꾼다. 제품 경로와 census 출력은 그대로다.
- 시험·fmt·clippy·문서 검증은 통과한다. 그래서 ACCEPT다.

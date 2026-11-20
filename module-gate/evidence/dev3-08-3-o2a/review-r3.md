Verdict: ACCEPT

# DEV3 08-3 O2a 독립 D2 검토 (3회차)

2026-11-07. 검토자는 구현자와 독립이다. 소스는 고치지 않았고, 이 디렉터리에는 이 파일만 더한다. mutation과 시험은 scratch 복사본(`git archive 7064605`, 별도 `CARGO_TARGET_DIR`)에서 했고 끝난 뒤 지웠다. Row 제품 경로 run은 돌리지 않았다.

## 대상

- 범위: `git -C /home/user/xadb-083 diff 267270d 7064605`(branch `wip-083-o2a`, head `7064605` `DEV3 08-3 O2a: apply the second D2 review`).
- 바뀐 것:
  - `semantic_row/jobs.rs`: ReleaseRole 주석만 바뀌었다. 주석이 아닌 줄의 변경은 0이다.
  - `semantic_row/transition_overflow_tests.rs`: OVF-C02의 "duplicate address" 입력을 바꾸고 "chunk 0 read again as chunk 2"를 더했다. 문서 주석도 고쳤다.
  - 계약 §15: ChunkWrite·Forward(overflow) 행, census 시험 수, 실행 문단(R2 음성 시험이 없는 이유, `267270d`의 Row 제품 경로 결과), O2d 항목(§13).
  - 2회차 산출물(`review-r2.md`, `review_r2_probes.patch`) 등록. 내가 쓴 파일과 같다.
- 기준: 2회차의 필수 R2-F1, 권고 R2-R1, NIT N-a–N-d. 규범은 계약 §4.1(73행), §10 OVF-C02, §15다.

## 돌린 것(scratch, 계약이 정한 환경 변수와 `RUST_MIN_STACK=33554432`)

| 명령(`cargo test -p xadb-engine --lib -- …`) | 결과 |
| --- | --- |
| `transition::tests::overflow` | 5 passed. `OVF-C02 duplicate address`와 `OVF-C02 chunk 0 read again as chunk 2` 모두 `OrderMismatch` |
| `storage::semantic_row:: storage::retention_page::row storage::row_record` | 218 passed |
| census 묶음(2회차와 같은 6 filter) | 11 passed. `N1_SOURCE_TERMS`와 `OVF_C_CENSUS` 줄이 2회차와 같다 |
| `cargo fmt --check` | 깨끗 |
| `cargo clippy -p xadb-engine --lib --tests -- -D warnings` | 깨끗 |
| `python3 devdoc/verify_documents.py`(worktree, 이 파일 포함) | PASS. `devdoc/document-validation.json`은 `git checkout`으로 되돌렸다 |

`267270d`에서 `7064605`까지 제품 코드는 바뀌지 않았다(jobs.rs는 주석뿐이다). 그래서 §15에 기록된 `267270d`의 Row 제품 경로 결과(264 passed, 0 failed, 6 ignored, 1,996.61 s)가 이 head의 제품 코드에도 해당한다.

### mutation

| | 바꾼 것 | 결과 |
| --- | --- | --- |
| M4 | `storage/overflow.rs:494–498` `OverflowChainVerifier::push`의 방문 주소 집합 삭제 | OVF-C02가 `OVF-C02 duplicate address: prepare`에서 FAIL(`Ok(())` ≠ `Err(OrderMismatch)`). 2회차에는 PASS였다 |
| M7 | `storage/overflow.rs:293` `parse_cell`의 `index != expected_index` 삭제 | OVF-C02 PASS. "chunk 0 read again as chunk 2"는 index 검사가 없어도 verifier(index와 방문 집합)가 잡는다. 시험 주석의 "the chunk index … refuses it first"와 맞는다. 이 입력은 특정 검사를 지킨다고 주장하지 않는다 |

## 2회차 항목 처리

| 항목 | 판정 | 근거 |
| --- | --- | --- |
| R2-F1(F2 잔여) 중복 주소 시험 | **해소** | `transition_overflow_tests.rs:993–998`이 2회차 입력(chunk 2의 image를 page 17로 옮기고 page digest를 다시 봉인)을 만들고, `1034`의 "duplicate address"가 그 입력을 `a_refuses(.., Some(&frame), OrderMismatch)`로 돌린다. 관측기도 같은 prepare를 부른다. M4에서 이 label이 실패하므로 거절은 의도한 방문 집합에서 나온다. 예전 입력은 `1041` "chunk 0 read again as chunk 2"로 남았다. §15 OVF-C02 행은 두 입력을 각각 어느 검사가 잡는지 정확히 적고, 수(거절 14개, 입력 쪽 여덟)도 맞다 |
| R2-R1 leaf.rs R2의 음성 시험 | **이월 정당** | 구현자의 판단이 맞다. leaf.rs는 판정 전에 `undo::observe`를 부르고, 그 `bind_job`(`undo.rs:717`)이 frame의 Job native before bytes와 bank Job이 같기를 요구한다(`IdentityMismatch`). 그래서 2회차에 내가 제안한 입력(bank의 Job만 inline으로 바꿈)은 R2 앞의 코드에서도 그 묶음에서 거절되고, R2 갈래에 닿지 않는다. 그 제안은 잘못이었다. R2에 닿으려면 frame의 Job native before까지 일관되게 바꾼 후보가 필요하다. §15가 그것을 O2c replay 관측기 작업으로 옮기고 이유를 적었다. R2는 권고였고 코드는 2회차에 확인했으므로 받아들인다 |
| N-a | **해소** | §15 ChunkWrite 행: Free인 경우는 출력 뒤에 거절되고, 나머지 둘은 page 16을 골라 받아들여진다 |
| N-b | **해소(이월)** | §15 O2d 항목이 "그 단계 PR에서 §13 표(Base4 행)에도 적는다"고 한다 |
| N-c | **해소** | §15 census 시험 11개 |
| N-d | **해소**(계약 쪽 문장은 N-f) | `jobs.rs:251–256`의 주석이 "[200,248)을 두면 decode되지 않고, 지우면 `unchanged`에서 거절"로 바뀌었다 |

## 필수 지적

없음.

## 권고

없음.

## NIT

- **N-e.** `transition_overflow_tests.rs:986`의 `dev3-row-overflow.md#overflow-chain`은 어느 문서에도 없는 anchor다. 방문 주소 집합을 정한 곳은 §4.1이고 그 anchor는 `#chunk-revision`이다. crates의 doc 참조 74개를 확인해 보니, 해결되지 않는 것은 이것과 이 단계 밖의 오래된 둘(`dev3-row-delete.md#effect-1-h`, `#formats`)이다. `verify_documents.py`는 코드 주석의 anchor를 보지 않는다.
- **N-f.** §15 "국소 관측기" 문단(462행)에는 아직 예전 문장 "after는 `[200,248)`을 cursor 1로 가져가므로 decode되지 않는다"가 있다. jobs.rs 주석처럼 "지우면 `unchanged`에서 거절"을 더할 것.

두 NIT 모두 다음 commit에서 고치면 되고, 판정에는 영향이 없다.

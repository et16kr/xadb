Verdict: CONDITIONAL

# DEV3 08-3 O2a 독립 D2 검토 (2회차)

2026-11-07. 검토자는 구현자와 독립이다. 소스는 고치지 않았다. 이 디렉터리에는 이 파일과 probe patch `review_r2_probes.patch`만 둔다. probe와 mutation은 모두 scratch 복사본(`git archive 267270d`, 별도 `CARGO_TARGET_DIR`)에서 했고, 끝난 뒤 지웠다. worktree의 `target`과 그 안에서 돌던 Row 제품 경로 run에는 손대지 않았다.

## 대상

- 범위: `git -C /home/user/xadb-083 diff 6afdbe9..267270d`(branch `wip-083-o2a`, head `267270d` `DEV3 08-3 O2a: apply the first D2 review`). 주변 코드는 단계 전체(`wip-083-o1..wip-083-o2a`)에서 필요한 만큼 읽었다.
- 파일 9개(+730 / −36):
  - 소스: `semantic_row/jobs.rs`(주석), `semantic_row/leaf.rs`(R2), `transition/overflow.rs`(R6 source 항, `clr_chain_bytes`), `transition/prepare.rs`(scrutinee 빌림), `transition/source.rs`(leaf 항)
  - 시험: `semantic_row/transition_overflow_tests.rs`(+313). 추가된 것: `rechunk`, `restamped`, `chunk_job_refusals`, OVF-C01의 front 위반 셋·Reserve total 둘·chunk cell mutant 넷, OVF-C02의 중복 주소·LSN 둘, N2, `ovf_c_head_record_is_the_schema_encoders_overflow_arm`
  - 계약: `devdoc/contracts/dev3-row-overflow.md`(§6.2 한 문장, §15의 표·국소 관측기·census·실행 문단과 단계별 목록)
  - 1회차 산출물(`review-r1.md`, `review_r1_check.py`) 등록
- 기준: 1회차 지적 F1–F2, R1–R8, N1–N4. 규범은 계약 §4.1(73행 방문 주소 집합), §4.2–§4.5, §5, §6.2, §10(OVF-C01·C02), §11이고, 구현자의 주장은 §15다.

## 검토 방법

### 읽은 것

- diff 전체. 새 시험마다 그 거절이 어느 검사에서 나오는지 보려고, 그 앞에 있는 검사의 순서를 읽었다.
  - `chunk_write`(`transition/overflow.rs` 165–250)와 `allocator()`·`lookup_allocated`(`allocation_page.rs` 527)
  - `forward_chain`(`transition/overflow.rs` 376–416), `decode_overflow_page`·`parse_cell`(`storage/overflow.rs` 260–345), `OverflowChainVerifier::push`(483–513)
  - `jobs.rs` `observe`의 ChunkWrite·ReleaseRole 갈래와 Job codec(`retention_page/row/job.rs` 555–670의 chain·pending 규칙)
  - whole-group `observe_with`(`transition.rs` 666–709), `frame::verify`(`frame.rs` 412), `decode_layout`의 native digest 검사(`wire.rs` 463)
  - 시험 helper `refuses`(`transition_tests.rs` 2062), `rebind`(368), `a_refuses`, `a_group`
- `leaf.rs`의 `observe`·`insert_rows`·`sealed_value`·`validate_rows` 전부. R2 변경이 바른 inline·overflow 경우를 거절하지 않는지 보았다.
- R5 문장을 대조하려고 `row/insert/group.rs` 662·1036–1042와 `semantic_row/ledger.rs` 213–245를 읽었다.

### 돌린 것(scratch, 계약이 정한 환경 변수와 `RUST_MIN_STACK=33554432`)

| 명령(`cargo test -p xadb-engine --lib -- …`) | 결과 |
| --- | --- |
| `transition::tests::overflow` | 5 passed |
| `storage::semantic_row:: storage::retention_page::row storage::row_record` | 221 passed(218 + 내 probe 3) |
| census 묶음(`n1_…`, `s2w_c0`, `live_row_retirement…`, `unl_a03…`, `f4_c_begin…`, `ovf_c_census`, `--nocapture --test-threads 1`) | 11 passed |
| 같은 census 묶음을 `6afdbe9` tree로 | 11 passed. 출력을 줄 단위로 비교했다. 다른 줄은 `N1_SOURCE_TERMS`의 `leaf=10044 → 10300` 하나와 `OVF_C_CENSUS`의 `chunk_write 4937 → 5105`, `forward_chain 5776 → 5832`, `leaf 10044 → 10300`뿐이다. N1 source bound, S2W 묶음, C charge, live retirement·unlink charge 줄은 모두 같다 |
| `cargo fmt --check` | 깨끗 |
| `cargo clippy -p xadb-engine --lib --tests -- -D warnings` | 깨끗(경고 없음) |
| `python3 -I review_r1_check.py`(정수 모드) | 23 PASS. 계약 정수는 바뀌지 않았다 |
| `python3 devdoc/verify_documents.py`(worktree, 이 파일 포함) | PASS. `devdoc/document-validation.json`은 `git checkout`으로 되돌렸다 |

Row 제품 경로 run(`catalog_memory::row_join storage::row::`)은 지시대로 돌리지 않았다. 구현자가 worktree에서 돌리는 run의 결과를 기록에 붙인다. 이 commit의 제품 코드 변경은 두 가지다. `leaf.rs`의 match 두 곳은 도달할 수 있는 입력에서 결과가 같고(아래 R2), `prepare.rs` scrutinee의 빌림은 의미가 같다.

### probe(`review_r2_probes.patch`, 결과만 찍고 단언하지 않는 시험 셋)

`267270d`에 `git apply`로 붙고, `transition::tests::overflow::r2_probe --nocapture`로 돌린다.

| probe | 입력 | 지금 코드 |
| --- | --- | --- |
| `r2_probe_front` | 첫 ChunkWrite 뒤, 두 번째 ChunkWrite의 allocator(page 15 entry를 바꿈) | 대조 Ok(56,976). front Free, 다른 object, 다른 kind, 다른 incarnation(3), 다른 object generation(2)은 모두 `IdentityMismatch`이고 출력이 없다 |
| `r2_probe_jobs` | 두 번째 ChunkWrite의 Job after를 `rebind`로 바꾸고 jobs.rs만 부름 | noop Ok. written +2, written +0, front 그대로, total −1은 `IdentityMismatch`. front None은 Job decode가 `Corrupt`(관측기 `Queue(Corrupt)`). front = page 30(Allocated 아님)은 jobs.rs 단독으로는 Ok(R3(d), O2c 몫으로 기록됨) |
| `r2_probe_forward` | Forward(overflow)의 `ChainImages.pages`(lease 507, previous 510) | 대조 Ok(91,676). 구현자의 중복 주소 `OrderMismatch`, 서로 다른 image의 중복 주소 `OrderMismatch`. chunk 1 LSN = lease는 `OrderMismatch`, lease+1은 Ok, previous+1은 `OrderMismatch`, previous는 Ok |

### mutation(각각 따로 build했고 끝나면 되돌렸다. M5와 M6은 서로 다른 시험이 보므로 한 build에서 돌렸다)

| | 바꾼 것 | 결과 |
| --- | --- | --- |
| M1 | `transition/overflow.rs:202–209` front 검사 삭제 | OVF-C01이 "front Free"에서 FAIL(`InvalidInput`, 출력 뒤). probe: Free는 출력 뒤 `InvalidInput`이고, 다른 object·kind·incarnation·object generation은 page 16을 골라 Ok(56,976)으로 받아들여진다 |
| M2a | `jobs.rs:182` `new.written == old.written + 1` 삭제 | OVF-C01이 "jobs.rs ChunkWrite written +2"에서 FAIL |
| M2b | `new.total == old.total` 삭제 | "jobs.rs ChunkWrite total"에서 FAIL |
| M2c | `new.front != old.front` 삭제 | "jobs.rs ChunkWrite front kept"에서 FAIL |
| M3a | `transition/overflow.rs:404` chunk LSN 검사 삭제 | OVF-C02가 "chunk at the lease LSN"에서 FAIL |
| M3b | `lsn > lease_lsn`을 `>=`로 | 같은 곳에서 FAIL |
| M3c | `lsn <= previous_lsn` 삭제 | "chunk after the previous LSN"에서 FAIL |
| M4 | `storage/overflow.rs:494–498` `OverflowChainVerifier::push`의 방문 집합 삭제 | **OVF-C02 PASS.** OVF-F01의 `overflow_rejects_links_and_identity`·`overflow_verifier_requires_complete_order_and_digest`는 FAIL. probe: 서로 다른 image의 중복 주소가 Ok(91,676)로 받아들여진다(아래 R2-F1) |
| M5 | `leaf.rs`를 `6afdbe9`로 되돌림(R2 취소) | 221개 가운데 실패는 M6의 것 하나다. R2 변경을 지키는 시험이 없다(아래 R2-R1) |
| M6 | `reserve_total`의 범위 검사 삭제 | OVF-C01이 "Reserve total 2048"에서 FAIL(`InvalidInput` ≠ `Unsupported`) |

## 1회차 항목 처리

| 항목 | 판정 | 근거 |
| --- | --- | --- |
| F1 front 검사 시험 | **해소** | `transition_overflow_tests.rs:753–795`의 `refuses` 셋(front Free, 다른 object, 다른 kind). 셋 다 `IdentityMismatch`이고 after·frame이 바뀌지 않는다. M1에서 셋 모두 이 검사 하나에 기대고 있음을 확인했다. 앞의 검사들(state, pending, digest 봉인, payload 길이, allocator decode·LSN 5 ≤ previous)은 셋에서 모두 통과한다. 같은 검사가 incarnation과 object generation 위반도 거절한다(probe). §15의 거절 목록(12개)도 맞게 고쳐졌다 |
| F2 OVF-C01 index·next 등 | **해소** | `transition_overflow_tests.rs:877–911`의 mutant 넷(index @6, next @48..72, count @8, total @12)을 `rechunk`가 native kind-12 after와 INIT image에 함께 넣고 page·native·frame digest를 다시 봉인한다. `decode_layout`이 통과하는데(`wire.rs:463`이 native digest를 검사하므로 재봉인이 맞다는 증거다), ChunkWrite의 prepare는 frame을 읽지 않으므로(`inserted = None`) 거절은 `frame::verify`의 byte 비교에서만 나온다. 의도한 관측기 검사다 |
| F2 OVF-C02 중복 주소 | **부분 해소** | 시험은 생겼고 producer와 관측기가 같은 오류로 거절한다. 그러나 그 거절은 방문 집합이 아니라 앞선 `parse_cell`의 index 검사에서 나온다. M4에서 OVF-C02가 PASS한다. → R2-F1 |
| R1 국소 관측기·LSN 음성 시험 | **해소** | jobs.rs의 written·front·total 조건은 각각 자기 label 하나가 지킨다(M2a–c). 바꾸지 않은 rebind는 통과한다. front None은 Job codec(`job.rs:656`, `front.is_some() != (written != 0)` → Corrupt)이 먼저 막으므로 jobs.rs 규칙의 시험은 필요 없다. ReleaseRole의 `before.chain().is_none()`(`jobs.rs:258`)은 실제로 중복이다. chain은 cursor 0에서만 decode되고(`job.rs:594` OrderMismatch), ReleaseRole의 `unchanged(.., [(121,122),(376,1024)])`(`jobs.rs:263`)가 `[200,248)`을 그대로 두게 한다. 그래서 chain이 있는 before의 after는 `[200,248)`을 두면 decode되지 않고, 지우면 `unchanged`에서 거절된다. 음성 시험을 만들 수 없다는 판단에 동의한다(N-d). Forward chunk LSN의 두 경계는 M3a–c가 확인했다 |
| R2 leaf.rs payload 판정 | **해소**(시험 없음 → R2-R1) | `leaf.rs:260–270`과 `sealed_value`(`leaf.rs:363–371`). 도달할 수 있는 경우를 모두 따졌다. INSERT inline은 `(Inline, false)`라 projection으로 간다. INSERT overflow의 Forward·Clr·Prune은 `(Overflow, true)`다. `job`은 before bank의 Job(Reserved 또는 Retained, `@285` 유지)이다. DELETE는 `(Deleted, false)`다. Job codec이 `@285 = 1`을 effect byte 0(INSERT)에만 허용한다(`job.rs:491`). UPDATE는 `(Inline, false)`다. Prune의 `sealed_value(new, job.overflow())`도 overflow 행의 new(Overflow)를 받는다. 따라서 바른 inline·overflow 경우는 거절되지 않고, 221개 시험(OVF-C02의 overflow Forward·Clr·Prune을 leaf.rs가 받는 것 포함)이 통과한다. 바뀐 것은 overflow가 아닌 root의 Overflow 행이 projection 대조에 기대지 않고 바로 `IdentityMismatch`가 된다는 것뿐이다 |
| R3 빠진 교차 검사 | **기록됨** | §15 "O2c" 항목이 (a)–(e) 다섯 가지와 allocator entry·INIT image의 별도 관측기 계획을 모두 적는다. probe의 "front = page 30"이 (d)가 아직 열려 있음을 보인다(계획대로) |
| R4 O2b·O2d가 만날 거절 | **기록됨** | §15 "O2b"(ChainRetire 뒤 U pageLSN, `prepare.rs` ReleaseRole의 U 검사, `undo.rs`)와 "O2d"(`actor.rs`, `catalog/graph/ledger.rs`). §13 표에는 없다(N-b) |
| R5 A의 ledger 식 | **해소** | §6.2 문장이 `steps = forward_steps, wal = forward_wal, undo_slots = 1`로 바뀌었고, `group.rs`의 `debt(wal, undo_slots)`에 그대로 옮기지 말라고 적는다. `ledger.rs:213`의 `debt`와 `group.rs:662`의 `debt`는 같은 칸(steps, wal, protected = wal, peak = ROW_PEAK, undo_slots 1)을 만든다. §15 "O3"에 `group.rs`의 두 식이 있다 |
| R6 census source 모델 | **해소** | 1회차가 든 지역값이 모두 모델에 들어갔다. 증분은 `size_of` 합과 맞는다: `chunk_write` +168 = 24 + 56 + 56 + 24 + 16 − 8(`&mut [u8]` 16 → `&mut Prepared` 8), `forward_chain` +56 = 24 + 8 + 24, leaf +256 = 64 + 168 + 24. prepare의 두 scrutinee는 `&inputs.change`를 빌린다(`prepare.rs:1490`·`1561`). 다른 tuple scrutinee는 1431의 `(&inputs.change, inserted)` 하나이고 이미 빌린다. census 대조로 제품 bound가 그대로이고 출력 차이가 leaf 항뿐임을 확인했다. leaf 10,300은 N1 항의 최대(ledger 11,811) 아래다(여유 1,511) |
| R7 O2c 메모 | **기록됨** | §15 "O2c" |
| R8 작은 시험 | **해소, 이월은 정당** | total 2,048·131,073은 `Unsupported`다. 두 plan은 통과하는 plan과 total만 다르고, M6이 이 시험을 실패시킨다. 동치 시험(`transition_overflow_tests.rs:1330`)은 Reserved root 판(Transaction stamp, undo Some)의 bytes를 비교한다. 이월 둘: leaf Capacity는 O3로 간다(C 쪽 판정은 inline과 같은 검사이고, split은 A의 호출이다). `reserve_target`의 Overflow 행은 08-4로 간다(두 번째 transaction이 필요하고, 08-4가 그 갈래를 바꾼다). 이유가 §15에 적혀 있고 받아들인다 |
| N1 | **해소** | §15 OVF-C02 행에 "`check_record`의 projection 대조가 먼저 잡는다, `forward_chain`의 total 비교는 닿지 않는 방어"라고 적었다 |
| N2 | **해소** | `transition_overflow_tests.rs:1145–1149`가 `Err(Error::Row(RowError::Corrupt))`과 정확히 비교한다 |
| N3 | 미반영(선택) | 받아들인다 |
| N4 | **해소** | `chunk_write_bytes`가 `&mut Prepared<'_, '_>`를 센다 |

## 필수 지적

### R2-F1(F2 잔여). OVF-C02의 "중복 주소" 시험이 중복 주소 규칙(verifier의 방문 집합)에 닿지 않는다

- 위치: 시험 `transition_overflow_tests.rs:980–987`(`cycle`, `duplicate`)과 `1021–1027`.
  - 거절이 실제로 나오는 곳: `storage/overflow.rs:293` `parse_cell`의 `index != expected_index`. `forward_chain`(`transition/overflow.rs:408`)의 `decode_overflow_page`를 거쳐 나온다.
  - 의도한 검사: `storage/overflow.rs:494–498` `OverflowChainVerifier::push`의 방문 집합. 계약 §4.1(73행)은 "뒤 chunk는 root와 같은 물리 주소를 가질 수 없다"를 이 집합으로 옮겼고, §10 OVF-C02가 "중복 주소" 거절을 요구한다.
- 원인: 시험 입력 `[p17, p16'(next = 17), p17]`의 세 번째 image는 chunk 0의 image 그대로라서 cell index가 0이다. `forward_chain`은 index 2를 기대하고 decode하므로 `parse_cell`이 먼저 `OrderMismatch`를 낸다. 방문 집합이 내는 값과 같아서 시험은 둘을 구분하지 못한다.
- 실패 입력: `written(&value)` 상태(k = 3, chain 17 → 16 → 15)에서 allocator와 row cell(`row_after`)은 그대로 두고, `ChainImages.pages`만 다음 셋으로 바꾼다.
  - p17: chunk 0, 그대로
  - p16': chunk 1, next := `page_ref(17, 2)`, page digest 재봉인
  - p17'': chunk 2의 image(`refs[2]`)에서 page header의 page_no @32를 15에서 17로 바꾸고 page digest를 다시 봉인한 것. cell index는 2, next는 없고, payload는 창 2다.

  지금 코드에서는 이 입력이 방문 집합에서 `OrderMismatch`로 거절된다. M4(방문 집합 삭제)에서는 Ok(91,676 B Forward frame)로 받아들여져, page 17이 chunk 0이면서 chunk 2인 chain이 Retained가 된다. payload 창 0·1·2를 이어 붙인 digest가 head와 같기 때문이다. 그런데 이때도 OVF-C02는 PASS한다.
- 영향: 제품 코드의 결함은 아니다. verifier 자체는 OVF-F01 두 시험이 지킨다(M4에서 실패한다). 그러나 §10이 OVF-C02에 둔 "중복 주소"를 C 전이 수준에서 보여 주는 시험이 없는데, §15와 계약 머리 줄은 그 항목을 PASS로 적는다. 1회차 F2가 요구한 것이 정확히 이 항목이다.
- 바꿀 것: OVF-C02 loop에 위 입력을 "duplicate address"로 넣는다(`a_refuses(.., Some(&frame), Error::OrderMismatch)`. 관측기도 같은 prepare를 부르므로 같은 값이 나온다). 만드는 법은 `review_r2_probes.patch`의 `r2_probe_forward`에 있는 `rehomed`다. 지금 입력은 "chain walks back (index)" 같은 이름으로 남겨도 된다. §15 OVF-C02 행의 "중복 주소(…세 번째에 page 17을 다시 읽음: OrderMismatch)"도, 어느 검사가 어느 입력을 잡는지에 맞게 고친다.

## 권고

- **R2-R1. R2 변경을 지키는 음성 시험이 없다.** M5(`leaf.rs` 되돌림)에서 모든 시험이 통과한다. R2는 1회차에서 권고였고 시험을 요구하지 않았으므로 필수는 아니다. 만든다면 `leaf.rs` 단독 시험이 된다. OVF-C02의 받아들인 Forward frame에, Job bank의 그 Job을 `@285 = 0`, `[328,360)` zero, projection = root를 포함한 head record의 XRP1로 바꾼 bank를 준다. 그러면 R2 앞의 코드는 받아들이고 지금 코드는 `IdentityMismatch`를 낸다. §15 O2c 국소 관측기 계획과 함께 두어도 된다.

## NIT

- **N-a.** §15 ChunkWrite 행의 "front 검사가 없으면 page 15를 다시 고른 뒤에야 거절되므로"는 front Free인 경우에만 맞다. 다른 object·kind(그리고 incarnation·object generation)인 경우는 front 검사가 없으면 page 16을 골라 그대로 받아들여진다(M1 probe Ok(56,976)). "없으면 Free는 출력 뒤에야 거절되고, 나머지는 받아들여진다"로 고칠 것.
- **N-b.** R4의 규칙 변경(ChainRetire 뒤 U pageLSN 규칙, Base4 `actor.rs`의 `header.lsn < last_step`, `catalog/graph/ledger.rs`의 `chain.apply != lease_lsn`)은 §15 단계 목록에만 있고 §13 "바꾸는 정본 문장" 표에는 없다. 해당 단계 PR에서 §13의 Base4 행에 더할 것.
- **N-c.** §15는 "census 시험 10개"라 적지만 그 묶음은 11개다(`s2w_c01`–`c06` 여섯 포함. 1회차 표도 11).
- **N-d.** `jobs.rs:251–255`의 주석 "so such an after does not decode"는, `[200,248)`을 지운 after가 decode는 되지만 `unchanged`(`jobs.rs:263`)에서 거절된다는 경우를 빠뜨린다. 결론은 같다. "or fails `unchanged`"를 더할 수 있다.

Verdict: CONDITIONAL

# DEV3 08-3 O2a 독립 D2 검토 (1회차)

2026-11-07. 검토자는 구현자와 독립이다. 소스는 고치지 않았고, 이 파일과 독립 검산 script `review_r1_check.py`만 이 디렉터리에 둔다. probe·mutation은 모두 scratch 복사본에서 했고 worktree에는 남기지 않았다.

## 대상

- 범위: `git -C /home/user/xadb-083 diff wip-083-o1..wip-083-o2a`(branch `wip-083-o2a`).
  - c273449 `DEV3 08-3 O2a: C overflow Reserve, ChunkWrite, Forward, CLR and Prune`
  - e63d4d1 `Merge 08-3 O1 (with the O0 4th edition) into O2a`. 충돌 해소는 계약 §5 ChunkWrite 문단(“front chunk digest” 문장을 O2a의 digest 봉인 문장으로 바꾸고 4판의 reserve(plan) 문장은 유지)과 머리 줄뿐이다. 새 문장이 더 강하므로 받아들일 수 있다.
  - 6afdbe9 `DEV3 08-3 O2a: record the Row path and census runs`
- 파일 15개(+2,210 / −71):
  - 신규: `crates/engine/src/storage/semantic_row/transition/overflow.rs`(552), `crates/engine/src/storage/semantic_row/transition_overflow_tests.rs`(1,109)
  - C 전이: `semantic_row/transition.rs`, `transition/prepare.rs`, `transition/frame.rs`, `transition/source.rs`
  - 국소 관측기·ledger: `semantic_row/jobs.rs`, `semantic_row/leaf.rs`, `semantic_row/undo.rs`, `semantic_row/ledger.rs`, `semantic_row/jobs_tests.rs`
  - harness: `semantic_row/transition_tests.rs`
  - codec: `storage/row_record.rs`(`encode_overflow_record`), `storage/retention_page/row.rs`(re-export)
  - 계약: `devdoc/contracts/dev3-row-overflow.md`(§15 신설, 머리 줄, §5 문장)
- 기준(규범): 계약 §4.2–§4.5, §5, §6.2, §6.3, §10(OVF-C01·C02), §11, §14. 구현자의 주장은 §15.

## 검토 방법

### 읽은 것

- diff 전체와 그 주변: `overflow.rs` 전부, `prepare.rs`의 `typed_leaf`·`check_record`·`prepare_leaf`(Forward·CLR·Prune 갈래)·`reserve_target`·Reserve·ChunkWrite·Cancel·Forward·CLR·ReleaseRole/RootFree 갈래·`write_pages`·`locals_bytes`, `frame.rs`의 native·body·redo 경로, `source.rs`, `ledger.rs`의 `debt`·`remaining`·`validate_transaction`, `jobs.rs`·`leaf.rs`·`undo.rs`의 바뀐 갈래와 census.
- O1 쪽 대조 대상: `semantic_row/wire.rs` 389–540(native kind 6·10·12, slot 규칙, redo shape), `semantic_row/operation.rs`의 `native_sides`, `retention_page/row/job.rs`의 `pending()`·`chain()`·`forward_wal()`·`forward_steps()`·`validate_root`(670–680), `storage/overflow.rs`의 chunk cell codec, `structure/candidate.rs` 535–550(split 후보 선택).
- 이후 단계가 만날 곳: `row/insert/group.rs` 662·1036–1042, `plan4_recovery_base/row/actor.rs` 228, `plan4_recovery_base/catalog/graph/ledger.rs` 255–258, `wal_io/plan4_retention/history/row.rs` 352·438.

### 돌린 것(worktree, 계약이 정한 환경 변수와 `RUST_MIN_STACK=33554432`)

| 명령(`cargo test -p xadb-engine --lib -- …`) | 결과 |
| --- | --- |
| `transition::tests::overflow` | 4 passed |
| census 묶음(`n1`, `s2w_c0`, `live_row_retirement`, `unl_a03`, `f4_c_begin`, `ovf_c_census`) | 11 passed |
| `storage::semantic_row:: retention_page::row row_record`(바뀐 모듈) | 217 passed |
| `catalog_memory::row_join storage::row::`(Row 제품 경로. 이유: 바뀐 `prepare.rs`·`ledger.rs`·`undo.rs`·`jobs.rs`·`leaf.rs`를 live·replay가 부르므로 inline 행의 회귀 여부를 직접 보려고) | 264 passed, 0 failed, 6 ignored(원래 `#[ignore]`), 2,567.65 s |
| `cargo fmt --check` | 깨끗 |
| `cargo clippy -p xadb-engine --lib --tests -- -D warnings` | 깨끗(경고 없음) |
| `python3 devdoc/verify_documents.py` | PASS(289 md). `devdoc/document-validation.json`은 `git checkout`으로 되돌렸다 |

### census 대조

`wip-083-o1` tree를 scratch에 꺼내 별도 target으로 같은 census 시험을 돌리고, O2a의 출력과 tag 줄 단위로 비교했다. 다른 줄은 `N1_SOURCE_TERMS`의 `leaf=9924 → 10044` 하나다. N1 source bound 30,359, S2W Row lifecycle 40,883, C charge 44,304, live retirement·unlink charge는 같다. §15의 census 주장은 사실이다. scratch 복사본에 probe를 넣어 각 최대 항과의 여유를 찍었다: leaf의 다른 최대 항(reservation) 6,756 vs `forward_chain` 5,776(여유 980), write 단계의 다른 최대 항(aging) 10,911 vs `write_chunk` 7,264(3,647), prepare callee 최대(ledger) 11,811 vs `chunk_write` 4,937(6,874).

### 독립 검산 `review_r1_check.py`

`struct`·`hashlib`와 계약의 정수만 쓰고 제품 encoder·decoder는 쓰지 않는다.

- 정수 모드(`python3 -I review_r1_check.py`): 23 PASS. W = 56,976(native 3,904 + redo 49,296 + frame 머리·XHG·descriptor·core·digest 3,776), F = 91,548+128h, R = 55,528, X = 59,056, CLR 뒤 `(5, 4R+X) = (5, 281,168)`, k = 3 행의 written별 row1 `(4, 262,604)·(3, 205,628)·(2, 148,652)·(1, 91,676)`, k = 9의 `9W+F = 604,460`·root 소계 974,020, Forward·CLR·Prune(overflow) frame 크기.
- frame 모드(`python3 -I review_r1_check.py DUMP_DIR`): 93 PASS, 0 failure. DUMP_DIR은 scratch 복사본에만 넣은 probe 시험이 40,018 B 행(k = 3)의 ChunkWrite 셋에서 떨어뜨린 frame·allocator·Job image다. 검산한 것: allocator가 고른 page(가장 낮은 9 이상 Free = 15·16·17), 바뀐 entry bytes(`[0]=2, [1]=7, generation, object 6, object gen 1`)와 다른 63개 entry 불변, redo의 96 B Patch를 before allocator에 글자 그대로 적용한 결과 = after allocator, native kind 6(before·after 96 B, 같은 offset)·kind 12(after만 cell header 128 B) header·body·digest, redo header 넷의 kind·offset·길이와 순서(allocator, Root, Jobs3, chunk), INIT image의 page header·slot·chunk cell(XOV1, index, count, total, payload 길이, `[20,48)` zero, next = 이전 front, digest, `[104,128)` zero)·payload = 값의 창 i·page digest, Job after = before에서 pending의 front·written과 last_step만 바뀜(total 그대로), `lease < LSN`.

### scratch 전용 probe와 mutation(worktree 밖 복사본, 끝난 뒤 버림)

- 두 번째 ChunkWrite에 첫 ChunkWrite 전의 allocator image를 준 입력: `Err(IdentityMismatch)`, after loan·frame 불변.
- Forward(overflow)의 `ChainImages.pages`에 같은 chunk page를 두 번 넣은 입력: `Err(OrderMismatch)`.
- mutation A: `overflow.rs:202–209`의 front 검사 삭제 → overflow 시험 4개 모두 PASS. 위 stale allocator 입력은 page 15를 다시 고른 뒤 `InvalidInput`으로 거절되는데, 그때 after loan이 이미 바뀌어 있다(출력 뒤 거절).
- mutation B: `overflow.rs:404`의 chunk LSN 검사, `jobs.rs:174–189`의 written+1·front 조건, `jobs.rs:255`의 `before.chain().is_none()`을 함께 삭제 → overflow 시험 4개와 `jobs_tests` 16개 모두 PASS.

### 질문별 판단

1. **전이와 guard.** Reserve(overflow)(`prepare.rs:1490`의 `reserve_total`, C = 244, `encode_overflow_record`의 root placeholder, Job `@285 = 1`·`[352,356)` total), ChunkWrite(`overflow.rs:165` 이하), Forward(overflow)(`forward_chain`, `prepare.rs:483`), CLR(overflow)(`clr_chain`, `prepare.rs:505`, budget `(5, 4R+X)`), Prune(overflow)(`prepare.rs:512`, `@285 := 0`), guard(ReleaseRole·RootFree의 chain → InvalidInput `prepare.rs:2080`, Cancel의 written ≥ 1 → InvalidInput `prepare.rs:1696`, plain Forward·ForwardOverflow의 짝 → InvalidInput)는 §5·§6의 검사와 순서에 맞는다. `chunk_write`는 모든 검사를 끝낸 뒤 `out.job`과 역할을 쓰고, after page는 `write_pages → write_chunk`에서만 쓴다. front 검사가 있는 지금 코드에서 출력 뒤에 거절되는 입력은 찾지 못했다. 다만 그 성질이 시험되지 않은 front 검사 하나에 달려 있다(F1).
2. **native·redo.** kind 6 native는 바뀐 entry 96 B(before·after 같은 offset, `allocator_entry`), kind 12 native는 chunk cell header 128 B(after만), redo는 allocator 96 B Patch(entry offset)와 chunk INIT(offset 0, 16,384 B)이다. `wire.rs`의 shape 규칙과 `operation.rs` `native_sides`의 `(6, ChunkWrite) = (3, 96, 96)`, `(12, ChunkWrite) = (2, 0, 128)`과 맞고, 위 검산이 bytes로 확인했다. O2c replay가 쓸 칸(entry Patch, INIT image, kind 12 digest)이 모두 frame에 있으므로 형식 변경 없이 replay할 수 있다. allocator 선택은 `structure/candidate.rs:535–550`(9 이상에서 가장 낮은 Free, `next_reservation_generation` Ok, Reserved가 있으면 거절)과 같은 규칙이다.
3. **ledger·descriptor.** row1 = `debt(forward_steps, forward_wal)`, `undo_slots = 1`(`ledger.rs:240–245`)이고 값은 위 검산과 같다. Reserved root의 `chain.apply == last_step()`은 inline에서 `last_step == lease_lsn`(`retention_page/row/job.rs:676–677`, written 0)이므로 예전 규칙과 같고, inline의 `forward_steps = 1`·`forward_wal`도 예전 값과 같다.
4. **국소 관측기.** `jobs.rs`·`leaf.rs`·`undo.rs`는 prepare를 부르지 않고 자기 규칙으로 판정한다. `undo.rs:209–216`의 Forward 규칙 `≥ lease_lsn`은 inline에서 약해지지 않는다: inline Reserved는 `last_step == lease_lsn`(위 job.rs)이라 두 식이 같고, 뒤의 `== lease_lsn` 정확 검사도 그대로다. 규칙의 내용은 맞지만 음성 시험이 없고(R1), overflow가 아닌 root의 Overflow payload를 projection 하나로만 거절하는 곳이 생겼다(R2).
5. **census.** 제품 census는 leaf 항 하나만 바뀌고 bound는 같다. source 모델에 빠진 지역값이 있으나 모두 여유 안이다(R6).
6. **시험.** OVF-C01의 front 위반(F1)과 index·next 위반, OVF-C02의 중복 주소(F2)가 실행되지 않았는데 §15는 OVF-C01·C02 PASS라 적는다. §15의 “head total” 거절은 실제로는 다른 검사가 잡는다(N1).
7. **이후 단계.** O2b·O2d와 O3에서 지금 규칙이 overflow root를 거절할 곳이 §15의 “남긴 것”에 없다(R4, R5).

## 필수 지적

### F1. ChunkWrite의 front 검사가 시험되지 않는다. 출력 전 거절을 지키는 검사가 이것 하나다(OVF-C01 “front 위반”)

- 위치: `crates/engine/src/storage/semantic_row/transition/overflow.rs:202–209`
  ```rust
  if let Some(front) = pending.front {
      alloc.lookup_allocated(cells::table_identity(table, namespace, front), PageKind::Overflow)
          .map_err(allocation_error)?;
  }
  ```
  시험 `transition_overflow_tests.rs:484`(`ovf_c01_chunk_write_allocates_and_inits_back_to_front`)의 거절 9개(§15: inline root, payload 길이, digest, Reserved entry, eligible Free 없음, allocator LSN, Retained root, 모두 씀, 쓴 root의 Cancel)에 front가 없다. §10 OVF-C01은 “front 위반 거절”을 요구한다.
- 실패 입력: k = 3 행에서 chunk 2(page 15)를 쓴 뒤, 두 번째 ChunkWrite의 `ChunkPlan.allocator`로 첫 ChunkWrite **전**의 allocator image를 준다. 그 pageLSN은 여전히 `1..=previous_lsn` 안이고 page 15는 Free다.
  - 지금 코드: front lookup이 실패해 `IdentityMismatch`, after·frame 불변(probe).
  - front 검사를 지운 코드(mutation A): 시험 4개가 모두 PASS하고, 같은 입력은 page 15를 다시 골라 after page를 쓴 뒤에야 `InvalidInput`으로 거절된다. 출력 전 거절(§5, 질문 1)이 깨지는데 어떤 시험도 실패하지 않는다.
- 바꿀 것: OVF-C01에 front 위반 거절을 `refuses`(after loan·frame 불변 확인, `transition_tests.rs:2062`)로 더한다. 적어도 (a) front가 Free인 stale allocator, (b) front entry가 다른 object 또는 다른 kind로 Allocated인 allocator. §15의 거절 목록도 고친다.

### F2. §10 OVF-C01·C02의 일부 항목이 실행되지 않았는데 §15와 계약 머리 줄은 “OVF-C01·C02 PASS”라 적는다

- **OVF-C01 “index·next 위반 거절”.** `ChunkPlan`에는 index·next가 없고 written·front에서 나온다. 그래서 이 항목은 관측기 쪽 시험, 곧 받아들인 frame의 INIT image에서 chunk cell index(cell @6)나 next(@48..72)를 바꾸고 page digest와 frame digest를 다시 맞춘 frame의 거절로만 확인할 수 있다. 지금 시험(`transition_overflow_tests.rs:659–673`)은 “chunk payload”와 “allocator entry”(generation @8) 두 mutant만 만든다. count·total 칸의 mutant도 없다.
- **OVF-C02 “중복 주소”.** §15의 거절 10개(head root, head total, chunk digest, 순서, 수, allocator entry, plain Forward, written < k, schema, inline root)에 없다. probe에서 같은 page를 두 번 넣은 `ChainImages`는 `OrderMismatch`로 거절되므로 구현 결함은 아니다. 그러나 시험이 없다. §15가 정한 대로 입력 쪽 항목이면 producer와 관측기가 같은 오류로 거절하는지도 보아야 한다.
- 바꿀 것: 두 항목의 시험을 더한다(C01은 관측기 mutant 셋 이상: index, next, count 또는 total. C02는 producer `a_refuses`와 관측기). 아니면 §15와 머리 줄에서 그 항목을 NOT_RUN으로 고친다.

## 권고

- **R1. 국소 관측기 규칙과 Forward chunk LSN 검사에 음성 시험이 없다.** mutation B에서 `jobs.rs:174–189`(ChunkWrite의 `new.written == old.written + 1`, `new.front.is_some() && new.front != old.front`), `jobs.rs:255`(ReleaseRole의 `before.chain().is_none()`), `overflow.rs:404`(`lease_lsn < pageLSN ≤ previous_lsn`)를 지워도 모든 시험이 PASS했다. ReleaseRole 거절 시험(root 17)은 producer 쪽 `prepare.rs:2080`이 먼저 잡으므로 `jobs.rs:255`를 지켜보지 못한다. Job before·after 쌍을 `jobs.rs`에 직접 주는 시험(written +2, front 그대로, front None, chain이 남은 ReleaseRole)과 chunk pageLSN 경계(= lease_lsn, > previous_lsn) 시험을 권한다. 관측기의 독립성은 그 규칙이 실제로 거절하는 것을 보여야 성립한다.
- **R2. `leaf.rs`의 payload 판정을 닫을 것.** `leaf.rs:263–268`의 `_ => projection(...)`은 overflow가 아닌 root(`job.overflow() == false`)의 `RowPayload::Overflow`도 projection 대조 하나로 판정한다. `insert_rows`의 `sealed_value`(`leaf.rs:337`·`345`, 정의 `358`)와 `validate_rows`(`leaf.rs:501`)도 root 종류와 무관하게 Overflow를 받는다. O1까지는 `inline(record)?`가 Unsupported로 거절했다. 그런 Job을 만드는 producer 경로가 없어 지금 뚫리지는 않지만, 거절이 Job projection 하나에 달려 있다. `(Overflow, !job.overflow()) => IdentityMismatch`를 명시하고, `sealed_value`가 `job.overflow()`와 payload 종류의 일치를 요구하게 할 것.
- **R3. 빠진 교차 검사.** (a) Forward(overflow)에서 `leaf.rs`·`jobs.rs`는 head root = pending front, written = k를 직접 보지 않는다(`forward_chain`만 본다). (b) `jobs.rs` CLR(overflow)(`jobs.rs` 238–247)는 chain anchor = pair[0]은 보지만, Job after의 chain root·total을 지운 행의 head(root·total)와 맞추는 국소 관측기가 없다(`leaf.rs`는 Job chain을 보지 않는다). (c) Cancel의 written = 0은 `prepare.rs:1696`만 본다. (d) ChunkWrite의 Job `new.front`와 kind 12 INIT page의 page 번호를 맞추는 국소 관측기가 없다. `cross_check`가 ChunkWrite에서 status.rs·undo.rs를 건너뛰므로(`transition_tests.rs`), allocator·chunk after는 whole-group 관측기(같은 prepare)와 시험의 재계산만 본다(§15도 인정). O2c(§8.1 replay)에서 allocator entry·INIT image를 따로 판정하는 관측기를 둔다고 계획에 적을 것. (e) OVF-C01은 Root page의 descriptor 칸 밖 bytes가 불변인지 보지 않는다.
- **R4. O2b·O2d가 만날 거절을 §15 “남긴 것”(그리고 §13)에 적을 것.**
  - ChainRetire 뒤에는 `last_step > release_start`(§4.3 표 99행)인데, ChainRetire는 Root·Job·anchor만 쓰므로 U의 pageLSN은 CLR의 LSN(= release_start)에 머문다. 그러면 ReleaseRole의 U 검사 `prepare.rs:2092`(`page_lsn(U) >= job.last_step()`), `undo.rs:209–216`(Forward가 아닌 역할은 `≥ last_step`), Base4 `plan4_recovery_base/row/actor.rs:228`(`header.lsn < job.last_step()` → Identity)이 그 root를 거절한다. 계약은 이 규칙을 바꾼다고 어디에도 적지 않는다.
  - written ≥ 1인 Reserved overflow root(OVF-R01의 “loser Reserved 중간”)를 Base4가 읽으면 `actor.rs:228`(U LSN = lease_lsn < last_step)과 `plan4_recovery_base/catalog/graph/ledger.rs:255–258`(`RootReserved && spec.chain.apply != root.lease_lsn()` → OrderMismatch)이 거절한다. 뒤의 것은 O2a가 `ledger.rs` `validate_transaction`에서 바꾼 규칙의 Base4 쌍이다.
- **R5. A(O3)의 ledger 식.** `row/insert/group.rs:1036–1042`은 아직 `chain.apply == lease_lsn`, `debt(row_forward_wal(..), 1)`이다(O3 몫이라 지금 결함은 아니다). 그런데 `group.rs:662`의 `debt(wal, undo_slots)`는 steps를 1로 고정한 A 전용 함수다. 계약 §6.2(194행)의 “row1 == `debt(forward_wal, forward_steps)`”를 이 함수에 글자대로 옮기면 forward_steps가 undo_slots 자리에 들어간다. `ledger.rs:213`(`debt(steps, wal)` + `undo_slots = 1`)과 같은 값이 되도록 계약 문장을 `steps = forward_steps, wal = forward_wal, undo_slots = 1`로 고치고, `group.rs`의 두 식을 §15 “남긴 것”에 적을 것.
- **R6. census source 모델.** 결과(bound)는 바뀌지 않지만 다음 항이 모델에 없다.
  - `chunk_write_bytes`(`overflow.rs:465`): `Namespace`(24), `PageIdentity` 임시값(56, 둘), 두 번째 `PageRef`(24), `Range`(16). `forward_chain_bytes`(`overflow.rs:529`): `Namespace`(24), image 참조(16), `Some(head.root())`(`Option<PageRef>` 24). 각 여유(6,874·3,647·980) 안이다.
  - `prepare.rs:1490`·`1561`의 scrutinee tuple `(value, inserted, inputs.change)`·`(canonical, inputs.change)`이 `Change`(96 B)를 복사하는데 `locals_bytes`(`prepare.rs:2540–2574`)는 그대로다. prepare 단계(17,659)가 N1에서 가장 큰 단계(write 15,391보다 크다)이므로 이 항은 N1 bound에 그대로 더해진다. 크기로 보아 `slack()`(`source.rs:82`, 512 B + α) 안일 것이나, 모델에 세거나 `&inputs.change`로 빌릴 것.
  - leaf edit 모델은 head `OverflowHead`(64), Prune의 `RowPayload`(168), `clr_chain` frame, `sealed_projection`의 24 B를 세지 않는다.
- **R7. O2c 메모.** Observation에서 ChunkWrite는 STATUS·UNDO 역할을 allocator·chunk로 재사용한다. replay의 `observe_with`는 ChunkWrite에서 INIT image(payload)와 kind 12 digest를 받아야 한다. 칸은 frame에 이미 있으므로 형식 변경은 필요 없다. `wal_io/plan4_retention/history/row.rs:352·438`은 role을 page image로 대응시키므로 INIT의 `initialized` 처리를 더해야 한다.
- **R8. 작은 시험 보강.** Reserve(overflow)의 total 범위 밖(2,048·131,073) → Unsupported, 232 B leaf의 Capacity, `reserve_target`의 Overflow → Unsupported(`prepare.rs:653–657`), `encode_overflow_record`와 `encode_row_record_with_schema`(Overflow 갈래)의 bytes 동치. 동치는 읽어서 같음을 확인했지만 시험은 없다.

## NIT

- **N1.** `overflow.rs:388`의 `head.total() == pending.total`은 도달하지 않는다. Job projection이 head를 묶으므로 다른 total의 head는 먼저 `check_record`(`prepare.rs`)의 projection 대조에서 `IdentityMismatch`가 된다. §15의 “head total” 거절은 그 검사가 잡은 것이라고 적을 것.
- **N2.** `transition_overflow_tests.rs:884`의 schema 거절이 `Err(Error::Row(_))`만 본다. §15는 `Row(Corrupt)`라 적으므로 정확한 값으로 비교할 것.
- **N3.** `entry_offset`(`overflow.rs:272`)은 부를 때마다 allocator page를 decode(SHA-256 포함)하므로 frame 하나에 여러 번 decode한다. `chunk_write`가 찾은 offset을 실어 나르면 된다.
- **N4.** `chunk_write_bytes`가 `&mut Prepared`를 `&mut [u8]`(16 B)로 센다. 과대 계상이라 안전하지만 이름을 맞출 것.

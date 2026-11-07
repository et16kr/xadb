Verdict: CONDITIONAL

# DEV3 08-3 O2a module-gate record 독립 검토 (D2, mapping)

2026-11-07. 검토자는 record 작성자와 독립이다. 이 파일 말고는 `/home/user/xadb`를 고치지 않았다. clone, probe, mutation build는 모두 scratchpad(`o2a-check/`)에서 했고, 큰 복사본은 끝난 뒤 지웠다.

## 대상

- record: `module-gate/records/20261107-dev3-08-3-o2a.json`. 아직 commit하지 않은 working tree 초안이다. blob은 `25c4acf52728b3e2741cd8f45d7561aa68b52f86`, sha256은 `0fa8a6b17d0a0f7253cba6a051d40dfc3ec6f86e0d82e95c7f6058e1b1c4a64d`다.
- 코드 diff: `origin/main`(`e0723ae`)..HEAD(`d6d9802`). HEAD는 stage branch `wip-083-o2a`(`01fee56`)를 merge한 commit이다. 바뀐 Rust 경로는 14개다.
- record 내용: `rust_tests` 7개 group, 사례 항목 82개(이름 71개), `na` 1개(경로 1개), review 2개(`dev3-08-3-o2a-d2`, `dev3-08-3-o2a-record`).
- 범위: 경로와 사례의 연결, reason, N/A, review 범위를 본다. 코드가 맞는지는 D2 r1–r3(r3 ACCEPT)를 따른다. 예외는 r3 뒤에 바뀐 한 줄이다(아래 R1).
- 이 record가 고르는, 바뀌지 않은 시험 파일 7개를 head blob에서 읽었다. 일곱 다 `origin/main`과 blob이 같다.

| 파일 | head blob |
| --- | --- |
| `crates/engine/src/storage/semantic_row/transition_delete_tests.rs` | `62255821879777437c6d87c8d0016600e516c9d2` |
| `crates/engine/src/storage/semantic_row/transition_update_tests.rs` | `e38b688a476934c18602f8104e56ffbf161270a0` |
| `crates/engine/src/storage/semantic_row/leaf_tests.rs` | `08003546e9ceb4e024bf191ac9d6c33915b87b89` |
| `crates/engine/src/storage/semantic_row/undo_tests.rs` | `0a5117c8266d439b475d3413161ee98724c9fd27` |
| `crates/engine/src/storage/semantic_row/ledger_tests.rs` | `d8c16145d3dbb114fbdf3deb4b205b632c061cf0` |
| `crates/engine/src/storage/wal_io/plan4_apply/row/census_tests.rs` | `82aa2cfba8b44863e23c498cb2ba4835be9ab2df` |
| `crates/engine/src/storage/wal_io/plan4_apply/source_tests.rs` | `349a3a15f61b9d26e14dee6acd283c24688fee98` |

## 검토 방법

- group마다 `git diff origin/main HEAD -- <paths>`를 읽었다. 선택된 사례 본문도 읽었다. `transition_overflow_tests.rs`는 전부 읽었다.
- **도달 확인(gdb).** head 시험 binary(`engine-e4e141083d864921`)에 바뀐 줄 70곳의 breakpoint를 걸었다. 각 breakpoint는 멈추지 않고 횟수만 센다. 나눠서 돌렸다.
  - 실행 단위: overflow 사례 5개, 나머지 66개, 그리고 시험 파일별(jobs_tests, leaf_tests, undo_tests, ledger_tests)이다.
  - 선택되지 않은 같은 파일 사례도 돌렸다: ledger_tests 6개, jobs_tests 2개, leaf_tests 3개, undo_tests 4개.
  - 결과: 선택되지 않은 사례 가운데 바뀐 줄에 닿는 것은 leaf_tests 3개뿐이다. 이 셋은 leaf.rs가 먼저 부르는 `undo.rs:211`에만 닿는다. 그 줄은 [3]·[4]의 선택된 사례도 지난다.
  - 값 확인: OVF-C02의 `undo.rs:208`에서 U pageLSN·`lease_lsn`·`last_step`을 찍었다. overflow root의 Forward는 507·507·510이다. 옛 규칙 `U ≥ last_step`이면 거절된다.
- **mutation.** `git archive HEAD`를 scratch에 풀었다. 14곳을 환경 변수로 켜는 변형으로 바꾸고 별도 `CARGO_TARGET_DIR`로 build했다. 변형마다 71개를 돌렸다. 표는 아래 (c)에 있다.
- **사례 실행.** 주어진 환경(`envpr.sh`)에서 돌렸다. binary는 `cargo test -p xadb-engine --locked --lib --no-run`이 그대로(Fresh) 쓰는 것을 확인했다.
  - 명령: `engine-e4e141083d864921 --exact --test-threads=1 <71 names>`
  - 결과: **71 passed, 0 failed, 0 ignored, 2408 filtered out** (17.57 s)
- **gate.** `git clone -q /home/user/xadb`로 scratch clone을 만들었다. 그 clone에 record를 넣고 commit했다.
  - 명령: `python3 scripts/check_module_gate.py check --base e0723ae --head HEAD --mode merge`
  - 결과: **PASS (0 error(s), 0 pending)**
  - `plan`은 71개 사례, `--test-threads=1`, export 두 쪽은 build만(사례 없음)이다.

## (a) group별 판단

- **[0] overflow.rs·transition_overflow_tests.rs·row_record.rs**: 충분하다.
  - 바뀐 줄에 닿는 횟수:
    - `reserve_total` Unsupported(`overflow.rs:146`) 2
    - payload 길이 1
    - Reserved entry 1
    - front 조회 33
    - `clr_chain` 2
    - `forward_chain` 21
    - `encode_overflow_record` 82
  - reason이 적은 거절 수가 시험과 맞다. 출력 전 ChunkWrite 거절 12개, OVF-C02 거절 14개다.
  - `ovf_c_census`가 단언하는 것도 reason과 같다. chunk_write와 leaf가 ledger 최대 아래, write_chunk가 다른 write 항 아래, forward_chain이 leaf 아래다.
  - 표현 문제 둘(권고 S2):
    - "chunk_write (the actor's Reserved root, …)": actor의 Reserved root 검사는 `chunk_write`가 아니라 prepare.rs의 ChunkWrite 갈래(`prepare.rs:1682–1685`)에 있다. "Retained root" 거절도 거기서 난다. 그 갈래에는 59번 들어가지만 `chunk_write` 호출은 58번이다.
    - "ReleaseRole of the chained root refused beside the inline root's": inline root 16의 ReleaseRole은 같은 상태에서 **통과**한다.
- **[1] transition.rs·prepare.rs·frame.rs·transition_tests.rs**: 충분하다.
  - overflow 갈래는 OVF-C01·C02와 twin이 지난다. 지나는 곳:
    - `check_record` overflow 26
    - Cancel written 1
    - 짝 검사 4(prepare·observer 양쪽)
    - CLR budget overflow 2
    - ReleaseRole chain 1
    - `write_chunk` 48
    - kind 6·12 각 48
    - INIT 43
    - `refuse_unproduced`의 ForwardOverflow(`World::observe`가 produce 거절을 확인)
  - inline 갈래는 나머지 66개가 지난다. CLR budget `(4, 4R)` 418, 기본 Patch 9,264다.
  - reason이 적은 미도달 넷은 모두 0회다. `check_record`의 종류 불일치(`:434`), Prune Deleted(`:513`), `reserve_target` Overflow(`:656`), kind 10(`frame.rs:162`)이다.
  - 그 밖에도 0회인 곳이 있다. 모두 방어용이다(권고 S4).
    - `frame.rs`의 INIT 길이 검사 두 곳(`:91–93`, `:97–99`)
    - `overflow.rs:303`·`:379`의 `let … else`
    - `:379`는 짝 검사를 지우면 닿는다(MUT_PAIRING). 그때 plain Forward 거절은 이 줄이 대신 InvalidInput으로 잡는다.
  - mutation: Cancel written(MUT_CANCEL), CLR budget(MUT_CLR_BUDGET), 짝(MUT_PAIRING), ReleaseRole chain(MUT_RELEASE_CHAIN)을 지우면 각각 OVF-C01이나 C02가 실패한다. 나머지 70개는 통과한다.
- **[2] jobs.rs·jobs_tests.rs**: 바뀐 줄에는 닿는다. 하지만 reason의 미도달 목록이 모자라다. 아래 필수 R2다.
  - 도달:
    - ChunkWrite 갈래 22
    - census `reserved == 1` 22
    - overflow CLR·Prune 규칙 2
    - Reserve pending 비교(jobs_tests 8, overflow 21)
    - ReleaseRole(jobs_tests 35, overflow 1. chain 없는 inline root뿐)
  - ChunkWrite의 written·front·total 조건은 OVF-C01의 jobs.rs 단독 mutant가 잡는다. r2 M2a–c가 확인했다. TerminalLeased 목록의 ChunkWrite도 시험이 지킨다(MUT_JOBS_LEASED에서 overflow 4개 FAIL).
- **[3] leaf.rs**: 충분하다.
  - overflow 갈래는 OVF-C02만 지난다.
    - root-free projection 3(Forward·CLR·Prune)
    - `sealed_value` overflow 4
  - inline 갈래는 leaf_tests가 지난다. projection 40, `sealed_value` 13·52, `validate_rows` inline 222다.
  - reason이 적은 미도달(`:268`, `:366`)은 0회다. r2 M5와 r3의 이월 판단과 맞다.
  - Prune payload 비교를 지우면(MUT_LEAF_PRUNE) `leaf_current_version_and_prune_payload_are_exact`만 실패한다. reason의 "Prune restamp and its payload mutants"가 맞다.
  - 빠진 것 하나(권고 S3): match 안으로 옮긴 `validate_rows`의 Deleted 갈래(`:500–506`, 본문 그대로)에 [3]의 사례는 닿지 않는다(leaf_tests 0회). 닿는 것은 [1]의 RDL-C·RUP-C history다(각 35회, cross_check의 leaf.rs).
- **[4] undo.rs**: 충분하다. reason이 사실이다.
  - undo_tests 5개는 Forward 분기(`:211`) 34회, 다른 역할(`:213`) 83회를 지난다.
  - 옛 규칙으로 되돌리면(MUT_UNDO) OVF-C02만 `OVF-forward: undo.rs OrderMismatch`로 실패한다. inline 70개는 통과한다.
- **[5] ledger.rs**: 충분하다.
  - 선택된 ledger 사례 6개는 모두 바뀐 줄(`:241` row 1, `:325` Reserved root 규칙)에 닿는다.
    - 둘 다: literal_states(5·2), exact_budget(5·7), observation_scope(1·1)
    - `:325`만: descriptor_identity(3)
    - `:241`만: rdl_f09(4), rup_f07(8)
  - 선택되지 않은 6개(O1의 `ovf_f02_ledger_returned_chain_needs_the_terminal` 포함)는 둘 다 0회다. 그래서 선택이 정확하다.
  - `row_ledger_descriptor_identity_and_order`의 `changed_descriptor(5, chain.apply = 21)`는 Reserved root에서 바뀐 비교의 참 갈래를 지난다.
  - mutation:
    - row1을 `debt(1, …)`로 되돌리면(MUT_LEDGER_ROW1) OVF-C01이 **Reserve에서** 실패한다(`transition_overflow_tests.rs:364`, (1, 262,604) ≠ (4, 262,604)). 더 뒤의 overflow 셋도 실패한다.
    - `chain.apply == lease_lsn`으로 되돌리면(MUT_LEDGER_APPLY) 첫 ChunkWrite가 실패한다(`OVF-chunk2: produce OrderMismatch`, after 상태 검사).
    - 두 경우 모두 inline 67개는 통과한다.
  - 표현 문제(권고 S1): "the old debt(1, ..) row and chain.apply == lease_lsn rule differ as soon as one chunk is written"은 chain.apply에만 맞다. row 1은 Reserve부터 다르다(steps 1+k).
- **[6] source.rs**: 충분하다. reason이 사실이다.
  - 선택된 census 사례 12개가 모두 `source::leaf`, `ledger_remaining`, `chunk_write_bytes`, `write_chunk_bytes`, `forward_chain_bytes`, `clr_chain_bytes`를 부른다. S2W-C01–C04·C06, F4-C, live retirement, UNL-A03은 각각 40–896회다.
  - S2W-C05는 하나도 부르지 않는다. reason대로 빼는 것이 맞다.
  - `--nocapture` 출력: `N1_SOURCE bound=30359`, `N1_SOURCE_TERMS … leaf=10300 … ledger=11811`, `OVF_C_CENSUS chunk_write=5105 write_chunk=7264 forward_chain=5832`. §15의 수와 같다.
  - `source::leaf`·`ledger_remaining`은 각각 `max(ledger …)`와 `max(ledger_observe …)` 아래에 있다. 그래서 다른 census 값은 바뀌지 않는다.
- **느린 Row 제품 시험을 뺀 것**: 단위 선택으로 충분하다.
  - 제품 경로는 overflow를 만들지 않는다. 새 `Change` 세 갈래는 시험만 만든다.
  - inline에 대한 변경은 동작이 같다. 근거는 셋이다.
    - N1·RDL-C·RUP-C의 literal 대조
    - Job codec의 불변식: inline Reserved는 `last_step == lease_lsn`이고(`job.rs:676`) `forward_steps == 1`이다.
    - 위 mutation에서 inline 사례가 모두 통과한 것
  - 제품 경로 결과는 log에 있다. r1은 `6afdbe9`에서, 구현자는 `267270d`에서 각각 264 passed, 0 failed, 6 ignored였다.
  - 그 뒤 제품 코드는 바뀌지 않았다. `7064605`는 jobs.rs 주석, `01fee56`는 시험 주석 한 줄이다. merge가 가져온 main 변경은 시험 파일 셋(overflow_tests.rs, stream_tests.rs, ledger_tests.rs)뿐이다.
  - 참고로 merge로 들어온 main 시험 4개도 head에서 돌렸다. `pr67_*` 3개와 `ovf_f02_ledger_returned_chain_needs_the_terminal`이고, 4 passed다. 이 record의 선택에는 넣을 필요가 없다. 바뀐 줄에 닿지 않는다.
  - `storage::row::` 전체를 뺀 데에는 예외로 볼 만한 것이 하나 있다(권고 S5).
    - `storage::row::insert::group_tests`는 느리지 않다. 15개가 0.17 s다.
    - 그 가운데 `mca00_v06_produced_groups_pass_c_observers`는 A가 만든 inline Forward·CLR group을 C의 국소 관측기에 넣는다.
    - gdb로 보면 바뀐 `undo.rs` 규칙에 닿는다(`:211` 3회, `:213` 2회).
    - inline에서는 옛 규칙과 새 규칙이 같으므로 필수는 아니다.

## (b) N/A(`retention_page/row.rs`)

N/A는 정당하다.

- 바뀐 것은 `use` 셋과 doc 주석 위치뿐이다.
  - `CHAIN_RETIRE_WAL`이 `#[cfg(test)]`에서 나왔다.
  - `OVERFLOW_CELL`과 `PendingChain`을 다시 내보낸다.
- 항목, 값, 실행 경로는 바뀌지 않는다.
- re-export가 있는지는 lib·test build가 컴파일되는지로만 드러난다. 시험 결과로는 보일 수 없다.
- cfg(test)에서는 `CHAIN_RETIRE_WAL`이 이미 보였다. 그래서 cfg를 바꾼 영향은 non-test lib에만 있고, 시험이 관측할 수 없다.
- 이름 뒤의 값(C = 244, pending, X)은 [0]–[2]의 사례가 쓰는 곳에서 확인한다. reason의 문장도 맞다.
- 같은 record의 D2 review가 이 경로를 head blob `e7c31646…`으로 나열한다. gate도 이 N/A를 받아들인다.

## (c) mutation 표

scratch의 `git archive HEAD`에서 했다. 변형마다 71개를 돌렸다. 모든 변형을 끄면 71 passed다.

| 스위치 | 바꾼 것 | 71개 결과 |
| --- | --- | --- |
| MUT_UNDO | `undo.rs:210` Forward도 `≥ last_step` | OVF-C02만 FAIL |
| MUT_LEDGER_ROW1 | `ledger.rs:241` steps = 1 | overflow 4개 FAIL(C01, C02, twins, head_record) |
| MUT_LEDGER_APPLY | `ledger.rs:325` `chain.apply != lease_lsn` | 같은 4개 FAIL |
| MUT_CANCEL | `prepare.rs:1696` 거절 삭제 | OVF-C01 FAIL |
| MUT_CLR_BUDGET | `prepare.rs:2017` overflow도 `(4, 4R)` | OVF-C02 FAIL(`A's CLR: Queue(BudgetMismatch)`) |
| MUT_PAIRING | `prepare.rs:1869–1873` 짝 검사 삭제 | OVF-C02 FAIL(inline root의 ForwardOverflow가 Ok) |
| MUT_RELEASE_CHAIN | `prepare.rs:2080` 거절 삭제 | OVF-C02 FAIL(`Queue(OrderMismatch)` ≠ InvalidInput) |
| MUT_LEAF_PRUNE | `leaf.rs:354` payload 비교 삭제 | `leaf_current_version_and_prune_payload_are_exact` FAIL |
| MUT_JOBS_LEASED | `jobs.rs:402` TerminalLeased 목록에서 ChunkWrite 삭제 | overflow 4개 FAIL |
| MUT_JOBS_PENDING | `jobs.rs:152–157` Reserve pending 비교 삭제 | **71 PASS** |
| MUT_JOBS_STMT | `jobs.rs:298` statement·mutation 목록에서 ChunkWrite 삭제 | **71 PASS** |
| MUT_JOBS_CENSUS | `jobs.rs:410` `reserved == 1` 삭제 | **71 PASS** |
| MUT_JOBS_CLR | `jobs.rs:241–248` overflow CLR anchor·Prune 규칙 삭제 | **71 PASS** |
| MUT_JOBS_RELEASE | `jobs.rs:259` `before.chain().is_none()` 삭제 | **71 PASS**(r1 mutation B와 같다) |

## (d) review 항목

### `dev3-08-3-o2a-d2`

- evidence 세 파일의 sha256은 head와 working tree에서 같다. record에 적힌 값과도 같다.
  - `review-r1.md`: `88b96a6b…a6cf`, CONDITIONAL, F1·F2
  - `review-r2.md`: `7a4dd781…f9948`, CONDITIONAL, R2-F1
  - `review-r3.md`: `e48ee3f8…a4f31`, ACCEPT, N-e·N-f
- verdict와 기준 commit(6afdbe9, 267270d, 7064605)도 각 파일 첫 줄·대상 절과 맞다.
- 실행 수도 각 파일과 맞다.
  - r1: overflow 4, census 11, 217, 제품 경로 264/0/6, 2,567.65 s, `review_r1_check.py` 23+93, mutation A/B
  - r2: 5, 218(+3), 11, M1–M6
  - r3: 5, 218, 11, M4/M7
  - fmt·clippy·verify_documents는 세 회차 모두 깨끗했다.
- `files`(14개)는 `check_module_gate.py files --base origin/main --head HEAD`의 출력과 같다.
- 14개 blob은 `01fee56`과 `d6d9802`에서 같다. merge가 바꾼 것은 없다.
- 14개 blob은 `7064605`와도 같다. 예외는 하나다. `transition_overflow_tests.rs`가 `f1634dd6` → `40455318`로 바뀌었다(아래 R1).
- log의 서술은 정직하다. 바뀐 것과 그 범위(주석 한 줄)를 그대로 적는다.
  - 하지만 `40455318`을 누가 검토했는지는 적지 않는다. r3가 본 것은 `f1634dd6`이다.
  - README에 따르면 나열한 파일이 뒤에 바뀌면 그 검토는 무효다. 그래서 이 항목만으로는 `40455318`이 덮이지 않는다.
- 그 diff(`git diff 7064605 01fee56 -- crates`)는 이번에 내가 읽었다.
  - `:986` 주석의 `dev3-row-overflow.md#overflow-chain` → `#chunk-revision` 한 줄이다. 그 밖에는 바뀐 것이 없다.
  - `<a id="chunk-revision">`은 계약 67행(§4.1)에 있다. r3 N-e가 가리킨 곳이다.
  - 같은 commit의 §15 문장(N-f)도 r3가 요구한 문장이다.
  - 코드 판단에는 영향이 없다. 남은 것은 record에 그 검토를 적는 일이다.
- `ledger_tests.rs`의 head blob `d8c16145…`는 O1이 더한 시험을 포함한다. 이 blob은 `origin/main`과 같다. 그래서 바뀐 경로가 아니라, 선택된 사례가 있는 바뀌지 않은 시험 파일이다. 아래 record 항목의 `files`에 있는 것이 맞다.

### `dev3-08-3-o2a-record`

- 선택된 사례가 있는 시험 파일은 10개다.
- 그 가운데 바뀐 셋(jobs_tests, transition_overflow_tests, transition_tests)은 D2 항목이 덮는다.
- 바뀌지 않은 일곱은 이 항목의 `files`에 있고, blob도 head와 같다(위 표). 그 범위에서는 완전하다.
- R1을 고치면 `transition_overflow_tests.rs`(`40455318…`)도 이 항목에 들어가야 한다.
- log는 구현자가 이 결과로 채운다.

## 필수 지적

### R1. r3 ACCEPT 뒤의 `transition_overflow_tests.rs` blob을 아무 review도 덮지 않는다

- `dev3-08-3-o2a-d2`의 `files`는 `40455318…`(01fee56)을 적는다. 하지만 ACCEPT한 r3가 본 blob은 `f1634dd6…`(7064605)다. r1·r2는 그보다 앞의 blob을 보았다.
- 차이는 주석 한 줄이다. 이번 검토에서 그 diff를 읽었고 문제가 없다(위 (d)). 하지만 record의 어떤 review도 그 검토를 주장하지 않는다.
- O1에서는 같은 경우를 "reviewed in dev3-08-3-o1-record"로 처리했다(뒤에 더한 ledger 시험).
- 고침:
  1. `dev3-08-3-o2a-record`의 `files`에 `"crates/engine/src/storage/semantic_row/transition_overflow_tests.rs": "404553181fd82dc463c6e9a5d6f09d6f9b5f7a81"`을 더한다. 그 log에 "이 record check가 7064605..01fee56의 NIT diff(N-e 주석 한 줄, §15 N-f 문장)를 읽었다"고 적는다.
  2. `dev3-08-3-o2a-d2`의 log에서 "saves r3;" 뒤에 "(that diff reviewed in dev3-08-3-o2a-record)"를 넣는다.
- clone에서 두 가지를 넣고 gate를 돌렸다. **PASS (0 error(s), 0 pending)**다.

### R2. [2]의 reason이 jobs.rs의 새 거절 일곱 가운데 하나만 "no case"로 적는다

- [2]는 "ChunkWrite's no-pending IdentityMismatch is reached by no selected case"만 적는다. 이 갈래는 0회다.
- 나머지 여섯(아래 표)도 거절 쪽에 닿는 사례가 없다. 지워도 71개가 모두 통과한다(위 (c)). CLR anchor와 Prune은 스위치 하나(MUT_JOBS_CLR)로 함께 지웠다.
- 저장소 전체로 넓혀도 같다.
  - `jobs::observe`를 부르는 시험은 jobs_tests, transition_tests(cross_check), transition_overflow_tests, `row/insert/group_tests`뿐이다.
  - 그 가운데 overflow Job(`@285 = 1`)을 넣는 것은 transition_overflow_tests뿐이다.
  - 그래서 선택을 넓혀도 이 거절에 닿는 사례는 없다.

| 거절 | 위치 | 더 넓은 판단 |
| --- | --- | --- |
| Reserve pending 불일치 | `jobs.rs:152–157` | whole-group 관측기만 본다 |
| ChunkWrite의 statement·mutation | `:298` | whole-group 관측기만 본다 |
| census `reserved == 1` | `:410` | whole-group 관측기만 본다 |
| overflow CLR의 anchor ≠ pair[0] | `:244` | whole-group 관측기만 본다. Job codec은 anchor가 dynamic page인지만 본다(`job.rs:585–591`) |
| overflow Prune 뒤 `@285 = 1` | `:246` | Job codec의 last_step 규칙(`job.rs:600–608`)이 먼저 거절할 것으로 본다(추론, 돌리지 않음) |
| chain을 쥔 root의 ReleaseRole | `:259` | Job codec과 `unchanged`가 이미 함의한다(§15, jobs.rs 주석) |

- [2]는 "OVF-C02 reaches the overflow CLR anchor and Prune rules"와 "chain None at ReleaseRole"이라고 쓴다. 읽는 사람은 이 규칙들이 시험된다고 받아들이기 쉽다.
- [3]은 leaf.rs의 미시험 거절을 이유와 함께 적는다. [2]만 빠져 있다.
- O1 R1과 같은 경우다. 바뀐 규칙이 거절하는 쪽에 사례가 없다.
- 고침(둘 중 하나):
  - (b, 최소) [2]의 마지막 문장을 정확한 목록으로 바꾼다. 예: "Reached by no case (each deleted alone, the 71 selected cases still pass): ChunkWrite's no-pending IdentityMismatch, the Reserve pending compare's refusal, ChunkWrite in the statement/mutation list, census reserved == 1, the overflow CLR anchor and Prune refusals, and ReleaseRole's chain conjunct. The Job codec already refuses the ReleaseRole case (and, by its last_step rule, the Prune one); the others are seen only by the whole-group observer (the same prepare) until O2c's replay observers." §15 "실행" 문단에도 같은 한 문장을 둔다. 지금 §15는 leaf.rs R2만 "음성 시험이 없다"고 적는다.
  - (a) OVF-C01·C02에 jobs.rs 단독 mutant를 더해 [2]에 넣는다. `chunk_job_refusals`의 `rebind` 방식을 쓴다. 대상은 Reserve pending total, CLR anchor다.
    - 이렇게 하면 시험 파일이 바뀐다. D2 review를 다시 받고 `files`를 다시 출력해야 한다.
- (b)이면 record(과 계약 한 문장)만 바뀐다. 다시 볼 것은 그 문장뿐이다.

## 권고

- **S1.** [5]: "differ as soon as one chunk is written"을 고친다. row 1은 Reserve부터 다르다(OVF-C01 `reserve()`, (4, 262,604)). `chain.apply` 규칙은 첫 ChunkWrite의 after 검사부터 다르다. 이렇게 나눠 적는다.
- **S2.** [0] 문장 둘을 고친다.
  - actor의 Reserved root·Retained root 거절은 prepare.rs ChunkWrite 갈래의 몫이다.
  - "ReleaseRole of the chained root refused beside the inline root's"는 "…refused, while the inline root 16's ReleaseRole at the same state is accepted"로 쓴다.
- **S3.** [3]: `validate_rows`의 Deleted 갈래는 [3]의 사례가 닿지 않는다. RDL-C 사례 하나(예: `rdl_c_delete_history_matches_literals`, 이미 [1]에 있음)를 [3]에 더하거나, 그렇다고 적는다. 본문은 그대로이므로 판정에는 영향이 없다.
- **S4.** [1]: 0회인 방어 줄을 미도달 목록에 더하면 정확하다.
  - frame.rs INIT의 길이 검사 두 곳
  - overflow.rs의 `let … else` 둘. `forward_chain`의 것은 짝 검사가 있어 닿지 않는다.
  - 필수는 아니다.
- **S5.** [4]: `storage::row::insert::group_tests::mca00_v06_produced_groups_pass_c_observers`(0.03 s)를 더하기를 권한다.
  - 이 사례는 A가 실제로 만든 Forward·CLR frame으로 바뀐 `undo.rs` 규칙을 지난다. 그래서 A 소비자 쪽 회귀를 CI에서 본다.
  - O3에서 A의 `group.rs`를 §6.2 값으로 바꿀 때도 같은 사례가 쓰인다.
  - 더한다면 `crates/engine/src/storage/row/insert/group_tests.rs`의 head blob(`fac592565693837077857e6fee9019c1faca92ab`)을 이 record check 항목의 `files`에 넣는다.
  - clone에서 R1 수정과 함께 넣어 보았다. gate는 PASS다.

## 판정

**CONDITIONAL** — 필수 2개(R1, R2)가 열려 있다.

- R1: record 항목 두 곳의 수정이다(위 문장). 기계적이다.
- R2: (b)이면 reason과 §15의 한 문장이다.
- 둘을 고치면 ACCEPT로 본다. 다시 볼 것은 record diff(과 §15 한 문장)뿐이다.
- 그 밖의 판단:
  - 경로 14개는 모두 대응되거나 N/A다. 각 group의 사례는 바뀐 줄에 실제로 닿는다.
  - [1]·[2]·[3]이 적은 미도달 갈래는 모두 실제로 0회다. 목록이 모자란 곳은 [2]다(R2).
  - N/A는 정당하다.
  - 느린 Row 제품 시험을 빼도 단위 선택으로 충분하다.
  - 71개는 head에서 모두 통과한다(17.57 s).
  - gate는 지금 record로 PASS다. R1을 고친 record로도 PASS다.

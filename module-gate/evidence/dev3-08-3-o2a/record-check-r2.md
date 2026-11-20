Verdict: CONDITIONAL

# DEV3 08-3 O2a module-gate record 독립 검토 2회차 (D2, mapping)

2026-11-07. 검토자는 record 작성자와도, 1회차 검토자와도 독립이다. `/home/user/xadb`에서는 이 파일만 만들었다. clone과 gdb 실행은 scratchpad(`o2a-r2/`)에서 했고, clone은 끝난 뒤 지웠다.

## 대상

- record: `module-gate/records/20261107-dev3-08-3-o2a.json`(working tree, untracked)
  - blob: `b2e438260d16c728e56361e8c5914c2aa9969706`
  - sha256: `4fe025bbcb746dc97b13f8a3a26c940c2b8bbd32f3ef5ab1dd0751ca9b1b09fc`
  - 1회차가 본 판(sha256 `0fa8a6b1…4a64d`)과 필드 단위로 비교했다. 그 판은 구현자의 scratch clone(`o2a-record/clone`)에 남은 사본이다. sha256이 1회차가 적은 값과 같다.
  - 바뀐 곳:
    - `rust_tests[0]`–`[5]`의 reason 여섯
    - `[4]`의 사례 하나(MCA00-V06)
    - `reviews[0].log`
    - `reviews[1].files`의 두 항목
  - 경로, 다른 사례, N/A, D2 `files`는 그대로다.
  - `dev3-08-3-o2a-record`의 log는 아직 "PENDING: record check"다. 지시대로 지적하지 않는다.
- 계약: `devdoc/contracts/dev3-row-overflow.md` §15 "실행" 문단(465행)
  - 더한 것은 jobs.rs 거절 일곱에 대한 세 문장이다.
  - 파일 sha256: `89b03393f4d8fb4b1a67e43c65067a354cff4828451a76769b71555b88f0e16f`
- `devdoc/document-validation.json`
  - sha256: `239cffc9560913365be4913ad1269f6e0fc3f1a38cdc9f6331e6bda673c390da`
  - 이 계약 항목의 sha256은 위 계약 값과 같다.
  - HEAD의 json에 있던 값(links 45, `836b6d5c…`)은 HEAD 계약(`91df2ca3…`)의 값이 아니었다. 이번에 다시 만든 것이다.
- 1회차: `module-gate/evidence/dev3-08-3-o2a/record-check.md`, sha256 `0a9b94007bd05eb6eecbd47bea149d9e5e3e20ce4b1ab19488ddaf8920da8f6d`
- 코드: HEAD `d6d9802`이다. `git diff --quiet HEAD -- crates`가 깨끗하므로 코드는 1회차와 같다.

## 검토 방법

- record 두 판을 JSON 필드로 펼쳐 비교했다. 바뀐 문장마다 해당 코드를 읽었다.
  - `semantic_row/transition/prepare.rs`
    - `:69`(`op: inputs.change.operation()`)
    - `:1679–1688`(ChunkWrite 갈래)
    - `:1869–1873`(ForwardOverflow 짝)
    - `:2442`(`write_chunk` 호출)
  - `transition/overflow.rs`: `:303`(`place_chunk`), `:379`(`forward_chain`)
  - `transition/frame.rs`: `:78–108`(INIT)
  - `semantic_row/jobs.rs`: `:97–98`(before·after decode), `:175`, `:241–248`, `:250–264`, `:290–302`, `:410`
  - `retention_page/row/job.rs`: `:475–491`(`@285` 규칙), `:564–608`(Releasing 규칙)
  - `retention_page/row/job_tests.rs`: OVF-F02(`:1073–1093`). fixture `cells.bin`의 23–26번 cell도 풀어 보았다.
  - `transition_overflow_tests.rs`: `:30–35`(W·F), `:360–366`(OVF-C01 row 1), `:1319–1342`(OVF-C02 ReleaseRole)
  - `row/insert/group_tests.rs`: `:780–880`(MCA00-V06)
- **gdb.** head binary `engine-e4e141083d864921`로 두 가지를 돌렸다.
  - MCA00-V06에 1회차의 breakpoint 70개(`o2a-check/gdb.cmd`, `bps.txt`)를 그대로 걸었다.
  - 선택된 72개를 하나씩 돌렸다. breakpoint는 `leaf.rs:501`(Deleted 갈래)과 `:509`(Inline 갈래)만 걸었다.
- **사례 실행.** 환경은 `envpr.sh`, `CARGO_TARGET_DIR`는 `/home/user/xadb-target-pr`다.
  - `cargo test -p xadb-engine --locked --lib --no-run`: Fresh다. binary도 같다.
  - `--exact --test-threads=1 storage::row::insert::group_tests::mca00_v06_produced_groups_pass_c_observers`
    - 결과: **1 passed, 0 failed, 0 ignored, 2478 filtered out** (0.02 s)
    - `cargo test … -- --exact --test-threads=1`로 돌려도 같다(0.03 s).
  - 선택된 72개 전부: **72 passed, 0 failed, 0 ignored, 2407 filtered out** (19.05 s)
- **gate.** `git clone /home/user/xadb`로 scratch clone을 만들고 `d6d9802`에서 branch를 땄다.
  - 넣은 파일: record, 계약, document-validation.json, record-check.md, 이 파일. 넣고 commit했다.
  - `python3 scripts/check_module_gate.py check --base e0723ae --head HEAD --mode merge`: **PASS (0 error(s), 0 pending)**
    - 이 파일의 최종판을 넣고 다시 돌려도 같다.
  - `plan` 결과:
    - 사례 72개. 중복이 없고, 위에서 돌린 72개와 같은 집합이다.
    - scoped 항목 83개, `--test-threads=1`
    - export 두 쪽은 build만 한다.
  - `files --base e0723ae --head d6d9802`: D2 항목의 `files` 14개와 같다.
- **verify_documents.** clone에서 `python3 devdoc/verify_documents.py`를 돌렸다.
  - 결과: PASS, markdown 289개, 오류 0
  - 다시 만든 json과 넣은 json은 `date` 한 줄만 다르다.

## 판단

### R1: 닫혔다

- `dev3-08-3-o2a-record`의 `files`에 `transition_overflow_tests.rs` `404553181fd82dc463c6e9a5d6f09d6f9b5f7a81`이 있다. head blob과 같다.
- D2 log에도 "(that diff reviewed in dev3-08-3-o2a-record)"가 1회차가 정한 자리("saves r3" 뒤)에 들어갔다.
- 그 diff를 이번에도 읽었다.
  - `git diff 7064605 01fee56 -- crates`는 `:986` 주석 한 줄이다. `#overflow-chain`이 `#chunk-revision`으로 바뀌었다.
  - `<a id="chunk-revision">`은 계약 67행에 있다.
- 남은 일은 하나다. record check log를 채울 때 이 diff를 읽었다는 문장을 넣는다(1회차 R1 고침 1). 두 회차 모두 이 diff를 읽었다.

### R2: 목록은 닫혔다. 이유 문장에 틀린 곳이 있다(R3)

- [2]의 목록 일곱은 1회차 (c) 표의 여섯에 no-pending IdentityMismatch를 더한 것이다. 빠진 것도, 더 들어간 것도 없다.
- "OVF-C02 passes … (accepting side)"로 바꾼 문장도 맞다. 1회차 gdb를 보면 overflow 사례 5개에서 `jobs.rs:242`는 2회, anchor 비교 `:244`는 2회, Prune 비교 `:246`은 1회 지났고 모두 통과했다. 거절로 끝나는 사례는 없다(MUT_JOBS_CLR에서 71 PASS).
- "(each deleted alone, the 71 selected cases still pass)"는 1회차 측정이 뒷받침한다. 다만 측정 방식은 문장과 조금 다르다.
  - 지운 스위치는 다섯이다. CLR anchor와 Prune은 함께 지웠다(MUT_JOBS_CLR).
  - no-pending 갈래는 지우지 않았다. gdb에서 0회임을 확인했다.
  - 그래도 결론은 맞다.
    - 거절만 지우는 변이이므로, 함께 지워서 통과하면 하나씩 지워도 통과한다.
    - 0회인 갈래는 지워도 결과가 같다.
  - 이번에 더한 MCA00-V06도 결론을 바꾸지 않는다.
    - 1회차가 jobs.rs의 바뀐 줄에 건 breakpoint 9곳 가운데 어디에도 닿지 않는다(아래 S5).
    - 이 사례는 inline Forward·CLR만 돌린다. 목록의 ChunkWrite, `reserved == 1`, ReleaseRole conjunct를 지워도 그 판정은 바뀌지 않는다.
    - 그래서 72개로도 같다고 본다. 다만 문장의 숫자 71은 이제 낡았다(권고 T1).
- 틀린 곳은 목록 뒤의 이유 문장이다(R3).
  - Prune 거절을 "Job codec의 last_step 규칙"이 막는다는 말은 근거가 틀렸다. 실제로 막는 것은 codec의 `@285` 규칙이다.
  - "overflow Job을 만드는 시험은 transition_overflow_tests뿐"이라는 말은 글자 그대로는 틀렸다.
- §15의 새 문장에도 같은 두 곳이 있다.

### S1–S5

- **S1: 반영됐다.** [5]가 둘을 나눠 적는다.
  - row 1은 Reserve부터 다르다.
    - `(4, 262,604)`는 k = 3일 때의 값이다. steps는 `1 + k = 4`다. wal은 `3·56,976 + 91,676 = 262,604`다(W·F, `transition_overflow_tests.rs:30–32`).
    - 옛 규칙으로는 `(1, 262,604)`가 된다.
  - `chain.apply`는 첫 ChunkWrite의 after 검사부터 다르다.
    - 첫 ChunkWrite는 chunk 2(`OVF-chunk2`)다. back-to-front 순서다.
    - 1회차 MUT_LEDGER_APPLY의 실패 위치와 같다.
- **S2: 반영됐다.**
  - actor의 Reserved root 검사와 Retained root 거절은 prepare.rs `:1679–1688`의 `require(… summary.reserved == selected)`다. `chunk_write`를 부르기 전에 있고, [1]의 범위다.
  - OVF-C02 `:1332–1342`는 같은 상태에서 inline root 16의 ReleaseRole을 `step`으로 돌려 통과시킨다. 새 문장과 맞다.
- **S3: 반영됐다(사례를 더하지 않고 적는 쪽).**
  - `validate_rows`의 Deleted 갈래(`leaf.rs:501`)에 72개를 하나씩 돌렸다. 닿는 사례는 15개다.
    - `transition_delete_tests`의 RDL-C 7개, `transition_update_tests`의 RUP-C 8개다.
    - 대부분 35회다. RUP-C borrowed 둘은 70회, `rdl_c_delete_forward_of_another_row_is_refused`는 36회다.
  - 15개는 모두 [1]의 사례다. [3]의 사례와 MCA00-V06은 0회다.
  - 다섯 사례의 첫 회 backtrace를 보았다(rup_c_effect2, 두 leaf_reservation, 두 forward_of_another_row).
    - 모두 `history_d_world` 안에서 `World::observe_leaf` → `leaf::observe` → `validate_rows`로 들어간다.
    - 넷은 `World::observe` → `cross_check`를 거친다.
    - `rdl_c_delete_forward_of_another_row_is_refused`만 첫 회를 시험 closure가 `observe_leaf`로 직접 부른다. 이 사례만 36회인 것도 그 1회 때문으로 보인다.
    - 그래서 "(cross_check's leaf.rs)"는 대체로 맞다.
  - 15개 가운데 일부는 이름에 history가 없다(leaf_reservation, forward_of_another_row, effect2). 이것들도 history world를 거치므로 "histories"도 맞다.
- **S4: 반영됐다.**
  - 넷이 각 경로의 group으로 나뉘어 들어갔다.
    - overflow.rs의 `let … else` 둘은 [0]에 있다.
    - frame.rs INIT 길이 검사 둘(`:91–93` write, `:97–99` compare)은 [1]의 미도달 목록에 있다.
  - 1회차 측정에서 넷 다 0회다. MCA00-V06도 넷에 닿지 않는다.
  - 괄호 "(prepare pairs the change first)"는 둘에 맞는 정도가 다르다.
    - `forward_chain`에는 맞다. `:1869–1873`의 짝 검사가 먼저다.
    - `place_chunk`는 이유가 다르다. `prepared.op`는 `inputs.change.operation()`(`:69`)이고, `write_chunk`는 `op == ChunkWrite`(`:2442`)일 때만 불린다.
    - 틀렸다고 할 정도는 아니다(권고 T2).
- **S5: 반영됐다.**
  - [4]에 `storage::row::insert::group_tests::mca00_v06_produced_groups_pass_c_observers`가 들어갔다.
  - `group_tests.rs`의 head blob `fac592565693837077857e6fee9019c1faca92ab`이 record check 항목의 `files`에 있다. origin/main의 blob과 같다.
  - 사례가 하는 일: A가 만든 Forward 셋(ForwardNoH, ForwardH, ForwardOrd)과 CLR 둘(ClrNoH, ClrH)을 C의 관측기에 넣는다. 관측기는 status·undo·jobs와 `ledger::validate_transaction`이다.
  - gdb 결과: 바뀐 줄 70곳 가운데 닿는 것은 `undo.rs:211` 3회와 `:213` 2회뿐이다.
    - 3회는 Forward 셋, 2회는 CLR 둘에 맞는다.
    - 그래서 새 문장 "through the C observers and so through the changed rule from the A side"가 맞다.
  - 위 실행에서 통과했고, gate의 `plan`에도 들어 있다.

### §15 새 문장

- 첫 문장의 목록 일곱은 record와 같고, 맞다.
- "(record check가 하나씩 지워 보았다)"는 위에 적은 실제 측정 방식과 조금 다르다(권고 T1).
- 둘째·셋째 문장은 R3다.

## 필수 지적

### R3. jobs.rs 미시험 거절의 이유 문장이 코드와 다르다([2] reason, §15)

1. **Prune 뒤 `@285 = 1`을 막는 것은 codec의 last_step 규칙이 아니라 `@285` 규칙이다. 그리고 확실히 막는다.**
   - jobs.rs는 `:98`에서 after를 `decode_job`으로 읽는다. 그래서 `:246`의 Prune 규칙보다 codec이 먼저 판정한다.
   - codec `job.rs:480–491`: `@285`는 Reserved·Retained·ReleasingCompensated 상태의 INSERT Row root에서만 1일 수 있다. 다른 Job에서 1이면 Corrupt다.
   - jobs.rs는 Prune의 after에 RootReleasingCommitted를 요구한다. 그러니 `@285 = 1`인 Prune after는 모두 decode에서 Corrupt로 끝난다.
   - 이 규칙은 O1의 OVF-F02(`job_tests.rs:1088–1093`)가 시험한다.
     - 주석은 "ReleasingCommitted (Prune wrote 0)"이다.
     - fixture cell 23–26(Row, ReleasingCommitted, effect 0)에 `@285 = 1`을 넣고 Corrupt를 확인한다. cell을 직접 풀어 상태를 확인했다.
   - last_step 규칙(`:600–608`)은 그보다 뒤에 있다. 또 chain 칸을 남긴 변형에서는 `retired`가 거짓이 되므로 last_step 규칙은 거절하지 않는다.
   - 그래서 다음 두 곳은 근거가 틀렸다.
     - record: "(and, by its last_step rule, the Prune one)"
     - §15: "(Prune도 last_step 규칙으로 그럴 것이다)". 게다가 확실한 것을 추정으로 적는다.
2. **"overflow Job을 만드는 시험은 transition_overflow_tests뿐"에는 범위가 빠졌다.**
   - 다른 파일도 `@285 = 1`인 Job을 만든다.
     - `retention_page/row/job_tests.rs`: OVF-F02 등 열두 곳
     - `semantic_row/ledger_tests.rs`: `:596`, `:607`
   - 맞는 말은 이렇다.
     - `jobs::observe`를 부르는 시험은 jobs_tests, transition_tests의 World, transition_overflow_tests, row/insert/group_tests(`:852`)다.
     - 그 가운데 overflow Job을 넣는 것은 transition_overflow_tests뿐이다.
     - 1회차 R2의 근거도 이 범위였다.
   - 결론("선택을 넓혀도 닿지 않는다")은 그대로 맞다.
3. **(같은 문장의 정확성) ReleaseRole은 codec 혼자 막지 않는다.**
   - chain을 cursor 1로 가져간 after는 codec이 거절한다(`job.rs:593–596`).
   - chain을 지운 after는 decode된다. `retired`가 참이고 cursor는 1이다. 이 after는 jobs.rs의 `unchanged`(`:264`)가 거절한다.
   - jobs.rs 주석과 §15 국소 관측기 항목은 이 둘을 다 적는다. 새 문장도 그와 맞춘다.

고침(예시다. 뜻만 맞으면 된다):

- record [2]: "The Job codec already refuses …"부터 끝까지를 다음으로 바꾼다.
  > Two are implied before jobs.rs's rule: the Job codec refuses @285 = 1 outside Reserved/Retained/ReleasingCompensated (Corrupt, so no Prune after keeps it; OVF-F02 checks ReleasingCommitted), and a ReleaseRole after that keeps the chain at cursor 1 does not decode while one that clears it fails unchanged. The other five are seen in this stage only by the whole-group observer (the same prepare). Of the tests that call jobs::observe (jobs_tests, transition_tests' World, transition_overflow_tests, row/insert/group_tests) only transition_overflow_tests feeds it an overflow Job (job_tests and ledger_tests build such Jobs without jobs.rs), so no wider selection reaches them.
- §15: "마지막 것은 Job codec이 먼저 거절하고"부터 "선택을 넓혀도 닿지 않는다."까지를 다음으로 바꾼다.
  > Prune 뒤 `@285 = 1`은 Job codec이 먼저 거절한다: `@285`는 Reserved·Retained·ReleasingCompensated에서만 1일 수 있으므로 그런 after는 decode되지 않는다(Corrupt, OVF-F02가 ReleasingCommitted로 확인). chain을 쥔 root의 ReleaseRole은 위 국소 관측기 항목대로 Job codec과 `unchanged`가 함의한다. 나머지 다섯은 이 단계에서 whole-group 관측기(같은 prepare)만 본다. `jobs::observe`를 부르는 시험 가운데 overflow Job(`@285 = 1`)을 넣는 것은 `transition_overflow_tests`뿐이라(`job_tests`·`ledger_tests`는 그런 Job을 만들지만 jobs.rs를 부르지 않는다) 선택을 넓혀도 닿지 않는다.
- 계약을 고친 뒤 `devdoc/verify_documents.py`로 json을 다시 만든다.
- 이 고침으로는 사례, 경로, `files`가 바뀌지 않는다. gate 결과도 그대로다.

## 권고

- **T1. 숫자 71과 "하나씩"을 고친다.**
  - [2]의 "(each deleted alone, the 71 selected cases still pass)"는 지금 record가 고르는 72개와 맞지 않는다.
    - 예: "(deleted, the 71 cases selected before MCA00-V06 still pass; MCA00-V06 runs only inline Forward/CLR and reaches none of the overflow refusals)"
  - D2 log의 "This record's 71 selected cases at d6d9802: 71 passed … 17.32 s"도 지금은 72개다.
    - 이번 실행 결과는 72 passed, 19.05 s다. record check log에 적어도 된다.
  - §15의 "(record check가 하나씩 지워 보았다)"는 이 정도로 쓰면 정확하다: "(record check가 지워 보거나(anchor·Prune은 함께) gdb로 0회임을 확인했다)"
- **T2. [0]의 괄호를 나눠 적는다.**
  - "(prepare pairs the change first)" 대신 이렇게 쓰면 정확하다: "(prepare reaches place_chunk only for op ChunkWrite, the change's own operation, and forward_chain only after the ForwardOverflow pairing)"
  - 같은 문장 앞의 "byte for byte; The two change let-else refusals"도 고친다.
    - 세미콜론 뒤가 대문자로 시작한다.
    - "change"를 형용사로 쓴 것이 어색하다.
    - 예: "…byte for byte. The two let-else refusals on the change (…)"
- **T3. record check log를 채울 때 넣을 것.**
  - 1회차 R1 고침 1의 문장: "7064605..01fee56의 NIT diff를 읽었다(주석 한 줄, §15 N-f 문장)"
  - 두 회차 파일의 sha256과 verdict

## 판정

**CONDITIONAL**: 새 필수 1개(R3)가 열려 있다.

- R1은 닫혔다. log를 채울 때 T3의 문장을 넣으면 된다.
- R2는 목록이 완전하고 맞다. 이유 문장의 근거 두 곳만 틀렸다(R3).
- S1–S5는 모두 반영됐다. 새 문장은 코드, 1회차 측정, 이번 gdb 결과와 맞는다. T1·T2는 문구 문제다.
- 새 사례 MCA00-V06은 통과한다. 선택된 72개도 모두 통과한다.
- gate는 PASS(0 error, 0 pending)다. verify_documents도 PASS다.
- R3를 고치면 ACCEPT로 본다. 그때 다시 볼 것은 셋뿐이다.
  - [2]의 해당 문장
  - §15의 해당 문장
  - 다시 만든 json

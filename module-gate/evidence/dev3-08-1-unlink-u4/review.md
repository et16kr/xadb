Verdict: ACCEPT

# U4(08-1 live unlink native fault cut UNL-N01..N06) 독립 D2 검토 2회차

## 대상

- 범위: 1회차 지적 반영분 `git diff cf46cc6 e3efd16`(6 files)와 unit 전체 `git diff 3c9d796 e3efd16`(10 files, +1543/−39).
  branch `u4-main`, HEAD `e3efd16`이다. 작성자 worktree `wt-u3`는 읽기만 했다. 내 사본 `review-u4/src`는 `git fetch` 뒤
  `e3efd16`을 checkout했고, 깨끗한 상태였다.
- 아래 줄 번호는 모두 `e3efd16` 기준이다.
- 읽은 것
  - 1회차 검토 `u4-review-r1.md`, brief `u4-d2-brief.md`.
  - code
    - controller `scripts/check_dev3_unlink_native.py` 전체(346줄).
    - worker `row_join/retire/unlink_native.rs`의 바뀐 부분: `inspect`, `expect`, `Before`, `Sink::save`, `produce`, `Ready::at_ready`.
    - 근거 확인용 product code: `structural_unlink.rs`의 `unlink_row_admitted`·`prepare_unlink`(validate_live 호출 :730-733),
      `catalog_lifecycle.rs`의 `apply_existing_n`(stage hook 앞까지), `history.rs` `validate_live`, `tree_page.rs`
      `TreeExpectation`·`validate_schema`(Row page의 arity 0).
  - 문서
    - `dev3-tree-smo.md` 머리(:1-16), 구현 단계(:1402), `<a id="unlink-u4">`(:1944)부터 끝까지.
    - `dev3-schedule-baseline.md` §4 표와 문단(:198-214), §7(:300).
    - 새 `module-gate/evidence/dev3-08-1-unlink-u4/README.md`, `devdoc/document-validation.json`.
  - 선례: `module-gate/evidence/dev3-08-1-unlink-u3/review.md`, `module-gate/records/20261106-dev3-08-1-unlink-u3.json`.
  - 실행 기록(읽기만): `unl-final`(cf46cc6)과 `unl-final2`(e3efd16, 내 검토 중 12 사례 완료).

## 판정 (필수 0, 권고 2, NIT 1)

- 1회차 필수 둘은 해결됐다. 일정의 날짜·경과·합계·§7은 §4의 KST 날짜 차 규칙과 맞는다. "2단위"는 문서에 남지 않았다.
- 1회차 권고 4개 가운데 셋(1, 2, 4)과 NIT 5개는 모두 해결됐다.
  - 새 controller 검사는 각각 겨냥한 잘못된 trace를 거절한다. 실제 process(P5)와 offline mutation 9개로 확인했다.
  - 고친 code의 실제 실행은 12 사례 38 process가 모두 PASS다. 작성자 실행 `unl-final2`와 내 UNL-N05-R이다.
- 1회차 권고 3(gate 기록·실행 증거)은 미완이다. 작성자의 계획(최종 판정과 최종 실행 뒤 기록·증거 추가)은 U3 선례와 같고
  일정 규칙에도 맞아 받아들인다. 다만 병합 전에 해야 할 일로 이번 권고 2에 남긴다.
- 새 권고 1은 결과 절이 아직 cf46cc6 실행을 가리킨다는 것이다.
  - 이것은 1회차 brief가 지적에서 빼라고 한 RESULT_PENDING과 같은 처지다. `e3efd16` 시점에 최종 실행이 돌고 있었다.
  - 필요한 값은 내가 확인해 아래에 적었으므로, 고치는 일은 기계적이다. 그래서 필수가 아니라 권고로 둔다.
  - 다만 병합 전에 고쳐야 한다.
- 고친 code에서 잘못된 cut이나 잘못된 복구가 통과하는 경로는 찾지 못했다. 하나 남는 것은 hook cut의 위치(N01 대 N02,
  그리고 prepare 뒤·stage 앞)를 trace로 가를 수 없다는 점이다. 이것은 계약 한계(:2045-2047)에 정직하게 적혀 있다.

### 1회차 지적의 처리

| 1회차 | 처리 | 근거 |
|---|---|---|
| 필수 1 (§4·§7 날짜) | 해결 | 아래 1 |
| 필수 2 (08-1 단위) | 해결 | 아래 2 |
| 권고 1 (N03 ATTEMPT) | 해결 | 아래 3 |
| 권고 2 (hook cut 관찰) | 해결 | 아래 4 |
| 권고 3 (gate·증거) | 미완, 계획 수용 | 이번 권고 2 |
| 권고 4 (계약 머리) | 해결 | 아래 5 |
| NIT 1 (receipt q) | 해결 | 아래 6 |
| NIT 2 (N02 쓰기 없음) | 해결 | 아래 7 |
| NIT 3 (DWB 수리) | 해결 | 아래 8 |
| NIT 4 (§4 이력 문장) | 해결 | 아래 9 |
| NIT 5 (목록 page decode) | 해결 | 아래 10 |

1. **필수 1.**
   - `dev3-schedule-baseline.md:209`는 `11-04→11-07 / 3일`, `:210`은 `약 18일`(14+1+0+3), `:300`은 `2026-11-07 / PR #59 병합`이다.
   - `:212`에 11-07 KST 병합을 전제로 적었다는 문장이 더해졌다. 그 문장의 표현은 이번 NIT 1이다.
2. **필수 2.**
   - `dev3-tree-smo.md:1402`와 `:1949`는 `08-1(2일, 1단위)`이다. `schedule:58`("1단위 = 2일")·`:81`("08-1 1단위")과 맞는다.
   - grep으로 확인한 결과, 두 문서에 `2단위`는 없다.
3. **권고 1.**
   - controller `:145-150`은 N03의 `effects[-3]`이 `['write', wal_name, str(start), str(frame), 'ATTEMPT']`이어야 한다고 본다.
   - N05도 `:141-144`에서 anchor page 전체(16,384 B, 같은 `d3-data-` 파일, 같은 offset)의 ATTEMPT를 본다.
   - 1회차 probe P1처럼 길이를 2,144로 바꾼 mutation은 거절됐다. N05의 offset을 0으로 바꾼 mutation도 거절됐다.
4. **권고 2.**
   - (a) hook cut의 arm 뒤에는 어떤 변경 행도 없어야 한다(`:156`). 또한 arm 뒤의 성공한 WAL read가 [0, wal_bytes)를 끊김
     없이 덮어야 한다(`:162-169`).
   - (b) 모든 phase의 test log에 `CRASH_MARKS`가 없어야 한다(`:59`, `:314-315`).
   - (c) 계약 한계에 bullet이 더해졌다(`:2045-2047`).
   - 1회차 P5(drain 전 abort)를 다시 넣고 UNL-N01-R을 실제로 돌렸다. 1회차에는 PASS였는데, 이번에는 `:169`에서
     `('the WAL read after the hook arm', 0, 7948646)`으로 FAIL이다.
   - arm 뒤 WAL read가 prepare_unlink의 것인지도 확인했다. arm 뒤에는 replay 묶음(4,096 B read 하나 + 전체 pass 둘)이
     정확히 하나 있다. prepare_unlink는 stage hook 전에 반드시 `validate_live`를 부른다(structural_unlink.rs:730-733).
     그러니 이 묶음은 prepare_unlink의 것이다.
   - 계약 `:1994-1996`의 "drain이 unlink 준비의 같은-cut canonical replay까지 갔다"는 그래서 맞다.
5. **권고 4.**
   - 계약 머리 `:5-9`에 "현재 상태(2026-11-07)" 문단이 더해졌다. `:3`과 `:7-16`이 2026-11-04 설계 시점의 기록임을 밝힌다.
   - 문단의 링크 넷(structural-effect :786, current-leaf-consumers :882, automatic-split :953, empty-leaf-unlink :1128)은 모두
     있는 anchor다.
   - 문단의 "U1–U4 … 독립 D2를 거쳐 닫혔다"는 U4 상태 줄을 고친 뒤에야 참이다. 이것은 이번 권고 2(d)에 넣었다.
6. **NIT 1.**
   - worker는 q를 DELETE의 `committed(out)`에서 잰다(`unlink_native.rs:587`, `:610-612`). 그 값을 `before.tsv`(`:509`)와
     receipt(`:514`)에 쓰고, READY는 `before.q`로 기대한다(`:712`).
   - controller의 기대 5(`:122`)와 READY 5·6(`:229`)은 literal로 남아, worker와 독립인 기대값이 된다. 내 N05-R receipt의
     q는 5였다.
7. **NIT 2.** arm 뒤에는 WAL뿐 아니라 어떤 `MUTATIONS` 행도 없어야 한다(`:156-157`). 그래서 home 쓰기도 막힌다. arm 뒤에
   `d3-data` write를 하나 넣은 mutation은 거절됐다.
8. **NIT 3.**
   - `check_repair`(`:209-224`, 호출 `:336`)는 두 가지를 본다.
     - N05 recover: 첫 DWB image read < 첫 home write < 첫 공개이고, 첫 home write가 anchor page 전체다.
     - 그 밖의 모든 recover·reenter: DWB image read가 없다.
   - 이 순서 검사가 수리 여부를 가르는지 trace로 확인했다.
     - N04-R recover의 첫 home write는 offset 0이고, 공개(1637행) 뒤(1722행)에 온다.
     - N05-R recover는 DWB read(1349·1606행) 뒤, 공개(1666행) 앞(1615행)에 offset 196,608을 쓴다.
     - 그래서 수리 write를 지우면 `homes[0] > publishes[0]`이 되어 거절된다(mutation으로 확인).
9. **NIT 4.** `schedule:214`에 "PR #59(08-1)의 병합으로 40.70%"가 더해졌다.
10. **NIT 5.**
    - `inspect`는 목록 page를 그 kind·level로 decode하고, 목록 leaf의 cell이 0개임을 단언한다(`unlink_native.rs:340-353`,
      Row page는 arity 0이 맞다: `tree_page.rs:179-185`).
    - `expect`는 "뒤" 목록 page가 produce image와 byte 단위로 같음을 단언한다(`:403-409`). 1회차에 제안한 것보다 강하다.
    - CI 쌍둥이(N04 상태)와 native N05-R에서 통과했다.

## 필수

없음.

## 권고

1. **U4 결과 절이 cf46cc6 실행을 가리킨다** (`dev3-tree-smo.md:2022-2031`, `:2037`).
   - 결과 절에는 cf46cc6 실행의 값이 적혀 있다: `commit cf46cc6`, 합계 768초, produce leaf 75.8~77.4초·root 45.6~47.4초,
     crash 1.2~1.5초, recover 0.7~2.0초, 복구 trace 최대 741,243 B, "fmt·clippy·문서 검사는 cf46cc6에서 PASS". 이 값은
     cf46cc6 실행 `unl-final`로는 모두 맞다(직접 대조).
   - 그러나 `:1994-2011`의 controller 검사 목록은 `e3efd16` controller의 것이다(ATTEMPT, hook read 덮음, DWB 수리,
     test log mark). cf46cc6 실행의 controller(`734169e7…`)는 이 넷을 돌리지 않았다.
   - 증거 README(`module-gate/evidence/dev3-08-1-unlink-u4/README.md:17-19`)는 `run/`이 `run/source-commit.txt`의 실행이라고
     적는다.
   - 실패 시나리오는 둘이다.
     - 계획대로 `unl-final2`를 `run/`에 복사하면, 계약은 768초·741,243 B·cf46cc6이라 하고 증거는 764.42초·743,576 B·
       e3efd16이라 하여 서로 어긋난다.
     - `unl-final`을 복사하면, 계약이 적은 검사 넷을 기록된 실행이 한 번도 돌리지 않았는데 계약은 PASS를 주장하게 된다.
   - 고칠 값(내가 `unl-final2`에서 확인했다). 결과 절을 `e3efd16` 실행으로 바꾼다.
     - source: manifest hash `236119bd…`. 내가 `e3efd16` 사본에서 다시 만든 manifest와 byte 단위로 같다.
     - hash: controller `f58893b3…`, input `dfba91c2…`, ELF `246c4f37…`.
     - 결과: 12 PASS, 38 process, 합계 764.42초.
     - 시간: produce leaf 74.3~77.0초, root 46.0~47.5초. crash 1.1~1.4초, recover 0.5~2.1초, reenter 0.6~0.7초.
     - 크기: WAL prefix와 cut이 더한 bytes는 그대로다. 복구 trace 최대는 743,576 B(N03-L recover), produce trace 최대는
       6,022,921 B(약 6 MB 그대로)다.
     - 38 process의 `files.json`은 cf46cc6 실행과 모두 같다.
   - `:2030-2031`의 시범 실행 문장은 이제 앞선 실행 둘(시범 `unl-run1`, cf46cc6 `unl-final`)을 말해야 한다.
   - fmt·clippy·문서 검사(`:2037`)는 최종 commit에서 다시 적는다.
2. **병합 전 gate·증거·상태 갱신** (1회차 권고 3의 남은 부분, 계획은 받아들임).
   - 현재 상태: `check_module_gate.py check --base 3c9d796 --head e3efd16 --mode merge`는 1회차와 같은 9 error로 FAIL이다.
     - scoped CI evidence 없음 4건: `native_tests.rs`, `unlink_native.rs`, `unlink_rows.rs`, `native-inventory.tsv`.
     - D1 없음 5 blob: `native_tests.rs` `1c670e7c57d1`, `unlink_native.rs` `f90fd044c5cc`, `unlink_rows.rs` `ea24eb82aef9`,
       controller `5b5d640af5c4`, inventory `efdb828bc841`.
   - 일정은 PR #59의 병합으로 적립한다(`schedule:212`). 그러니 §6 2의 항목 조건(독립 검토 필수 지적 해소, 증거 링크)과
     gate는 병합하는 head에서 채워져야 한다. 병합 전에 다음을 한 commit에 넣으면 된다.
   - (a) `module-gate/records/`의 기록.
     - 위 4 경로의 `rust_tests`(CI 셋 3 case)와 위 5 blob의 D1을 적는다.
     - 이 검토 뒤 다섯 파일 가운데 하나라도 바뀌면, D1 blob도 다시 맞춰야 한다.
   - (b) `run/`은 `unl-final2`에서 복사하고, `run/source-commit.txt`는 `e3efd16`으로 한다.
     - README 첫 문단의 hash 재현 주장은 내가 확인했다. `e3efd16`의 manifest가 `236119bd…`와 같다.
   - (c) `review-r1.md`(1회차)와 `review.md`(이 2회차)를 넣는다.
   - (d) U4 상태 줄(`:1947`)의 "독립 D2 RESULT_PENDING"을 "독립 D2 2회차 ACCEPT(필수 0)"로 고치고 검토 기록에 링크한다.
     U3 `:1725`와 같은 모양이다. 그때 비로소 머리 문단 `:7-8`의 "U1–U4 … 독립 D2를 거쳐 닫혔다"와 `schedule:212`의
     "각 단계는 독립 D2의 필수 지적을 해소했다"가 참이 된다.

## NIT

1. **병합일 변경 문장** (`dev3-schedule-baseline.md:212`).
   - 문장은 "병합이 그날을 넘기면 다음 기록 때 날짜만 고친다"이다.
   - 병합이 11-08 KST로 넘어가면 바뀌는 것은 날짜만이 아니다. §4 행의 경과(3일 → 4일), 합계(약 18일 → 약 19일),
     §7 날짜가 함께 바뀐다.
   - "다음 기록 때"라면 그 사이 main에 틀린 값이 남는다.
   - 고칠 문장 예: "병합일이 바뀌면 이 PR에서 §4 행·합계·§7을 함께 고친다."

## 직접 확인한 것

1. **사본.** `review-u4/src`에서 `git fetch origin u4-main`과 `git checkout e3efd16`을 했다. `git status` 0줄이었다.
2. **build.** `cargo test -p xadb-engine --lib -j 2 --no-run`이 성공했다(1분 56초). ELF sha256은 `6147056833ea916d…`다.
   경로가 달라 작성자 ELF와 hash가 다르다.
3. **CI 셋.** `cargo test -p xadb-engine --lib -j 2 -- unlink_rows::native:: --test-threads 2`의 결과는 3 passed, 1 ignored
   (worker)였다(77.06초).
4. **controller 1번째 사례(고치지 않은 code).**
   - 명령: `python3 -B scripts/check_dev3_unlink_native.py --elf ../elf-e3efd16 --output ../ctl-r2-n05r --input
     tests/fixtures/dev3-08-unlink/native-inventory.tsv UNL-N05-R`. 결과는 PASS였다.
   - process: produce −6 48.2초, recover 0 1.57초, reenter 0 0.52초.
   - produce receipt: q 5, lsn 145, wal_bytes 7,948,646, emptied 13, root 34, anchor 12, flushed 12.
   - trace의 마지막 세 행: `write d3-data-0000000100000001 196608 16384 ATTEMPT`, `FIRST_BYTE:Ok(1)`, cut.
   - 세 phase의 `files.json`은 cf46cc6 실행 UNL-N05-R과 같다.
5. **controller 2번째 사례(probe P5 재적용).**
   - 내 사본의 `unlink_native.rs`에서 `let armed = sink.save(...)` 바로 뒤(:634 뒤)에 다음 줄을 넣었다:
     `if matches!(sink, Sink::Native { .. }) && case.cut == "stage_before" { std::process::abort(); }`
   - 다시 build해(17초) 돌린 UNL-N01-R은 FAIL이었다. produce는 −6으로 끝났고, controller `:169`에서
     `AssertionError: ('the WAL read after the hook arm', 0, 7948646)`이 났다.
   - trace는 54,668행이고 마지막 행이 arm(HOOK)이다.
   - 파일은 미리 복사해 둔 원본으로 복원했다. `cmp`로 `git show e3efd16:<file>`과 같음을 확인했고, `git status`는 0줄이다.
6. **작성자 최종 실행 `unl-final2`(읽기만, e3efd16).**
   - 12 PASS, 38 process(produce·crash −6, recover·reenter 0), 합계 764.42초. 사례마다 pid가 모두 다르다.
   - `execution-plan.json`의 source hash `236119bd…`는 내 사본에서 controller와 같은 방식으로 만든 manifest의 hash와 같다.
     두 manifest는 byte 단위로 같다. controller hash `f58893b3…`는 내 사본 controller의 sha256과 같다.
   - 38 process의 `files.json`은 cf46cc6 실행 `unl-final`과 모두 같다.
   - N01-x와 N02-x의 produce trace는 inode 번호와 arm 이름만 다르다.
7. **offline replay(내 script `review-u4/probe-py/replay_r2.py`).**
   - script는 `e3efd16` controller의 `check_cut`·`check_repair`·`check_ready`·`check_crash`와 `CRASH_MARKS`를 import해,
     실행 디렉터리를 읽기만 한다. `check_wal`은 WAL이 최종 상태라 제외했다. live 실행 `unl-final2`는 이것을 이미 돌렸다.
   - 깨끗한 replay: `unl-final`과 `unl-final2` 모두 12/12 통과했다.
   - mutation은 둘 다 같은 결과였다. 아래 9개는 모두 거절됐다.
     - N03-L: ATTEMPT 길이를 2,144로.
     - N05-R: ATTEMPT offset을 0으로.
     - N05-R recover: anchor 수리 write를 지움.
     - N01-R·N02-L: arm 뒤 read를 모두 지움.
     - N01-R·N02-L: arm 뒤에 `d3-data` write를 더함.
     - N04-R·N06-L recover: DWB image read를 더함.
     - N06-L reenter: log에 `panicked`를 더함.
   - 가운데 WAL read 하나만 지운 mutation은 통과했다. 원인을 확인했다.
     - arm 뒤 `validate_live` 한 번이 WAL을 두 번 읽는다. pass A는 frame 단위로 read 392·339회, pass B는 163,840 B
       단위로 58·49회다.
     - 각 pass가 혼자서 [0, wal_bytes)를 덮는다. 그래서 이것은 중복 덮음이지 약점이 아니다.
   - 그 결과, prepare_unlink 전의 abort는 read 0으로 거절된다(P5). prepare 뒤·stage 앞의 abort는 가를 수 없다. 이것은
     계약 한계 `:2045-2047`이 적은 대로다.
8. **기타 검사(내 사본·e3efd16).**
   - `cargo fmt --all --check` PASS.
   - `cargo clippy -p xadb-engine --all-targets --all-features --locked -j 2 -- -D warnings` PASS. engine package만이고 workspace
     전체는 아니다.
   - `git archive e3efd16` 사본에서 `devdoc/verify_documents.py` PASS(markdown 287).
   - `dev3-tree-smo.md`·`dev3-schedule-baseline.md`의 sha256(`5669afa8…`, `a25ffe05…`)은 `document-validation.json`과 같다.
   - `check_module_gate.py check --base 3c9d796 --head e3efd16 --mode merge`는 FAIL 9 error였다(권고 2).
9. **일정 산술.**
   - 11-07 − 11-04 = 3일이다. 합계는 14+1+0+3 = 18일이다.
   - 35/86 = 40.70%이고 17.5단위 = 35일(`:81`)이다. 둘 다 그대로다.
   - §7 행 날짜는 2026-11-07이다.
10. **정리.**
    - 내 target 디렉터리(3.4 GB)와 ELF 둘(`elf-e3efd16`, `elf-p5r2`), 문서 검사 사본을 지웠다.
    - controller 출력 디렉터리(`ctl-r2-n05r`, `ctl-r2-p5-n01r`)와 log는 `review-u4/`에 남겼다.

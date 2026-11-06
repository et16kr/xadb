Verdict: CONDITIONAL

# U4(UNL-N01..N06 native fault cut) 독립 D2 검토 1회차

## 대상

- 범위: `git diff 3c9d796..u4-main`(9 파일). HEAD `cf46cc6`(author worktree `wt-u3`, branch `u4-main`). base `3c9d796`(PR #58 병합).
- 검토 사본: `git clone --local --no-checkout`으로 author 저장소를 `scratchpad/review-u4/src`에 복제한 뒤 `cf46cc6`을 checkout했다.
  build는 `scratchpad/review-u4/target`에서만 했다. author worktree에서는 아무것도 build·수정하지 않았다.
- 읽은 것:
  - 바뀐 파일 전부: `unlink_native.rs`(951행 전체), `unlink_rows.rs`의 선언, `plan4_pages/native_tests.rs` diff와 cut 기계
    (`row_cut_before`/`row_cut_after`/`row_cut_stop`, FirstByte write, `group_op`), `scripts/check_dev3_unlink_native.py` 전체,
    inventory TSV, `dev3-tree-smo.md`의 `#empty-leaf-unlink`~구현 단계와 `#unlink-u4`, 일정 기준선 diff와 §2.1·§2.2·§3·§4·§6·§7,
    `devdoc/README.md` 현재 작업 문단, `AGENTS.md`, `module-gate/README.md`, `repository-build-plan.md#module-gate`.
  - cut 경로의 제품 코드: `structural_unlink.rs::unlink_leaf_live`→`unlink_row_admitted`→`apply_existing(ExistingGroup::Structural)`(384행),
    `catalog_lifecycle.rs::apply_existing_n`(633–637행 `ExistingStageCallStarted`, 655–695행 `ExistingInstalledFirstPage`,
    716행 `wait_durable`), `catalog_test_support.rs`/`test_support.rs`의 `arm_abort`·`hit_catalog_pause`(skip 0이면 즉시
    `std::process::abort`), `row/insert/driver.rs::retire_finished`→`unlink_notes`, `wal_frame.rs`(445·452행 길이·LSN),
    `semantic_row/structure/unlink.rs::decode_layout`(group = payload[..192], payload는 160부터), `semantic_row/structure.rs`
    (42–61행 opcode·frame 길이), `plan4_recovery/epoch.rs::publish_current`, 선례 `row_inventory_a/native.rs`의 MCA03-R04 barrier.
  - author 실행 결과(읽기만): 시범 실행 `unl-run1/`, 최종 실행 `unl-final/`(검토 중 완료됨).
- author worktree에는 검토 중에 커밋되지 않은 변경(`dev3-tree-smo.md`의 결과 채움, `module-gate/evidence/dev3-08-1-unlink-u4/`)이
  생겼다. 검토 범위 밖이라 판정에 쓰지 않았다.

## 판정

필수 2, 권고 4, NIT 5. **CONDITIONAL.**

- 여섯 cut의 위치는 두 모양 모두 계약 표대로다. 코드 경로와 trace로 확인했다.
  - N01·N02의 hook은 unlink group 안에서 맞는다. drain 안의 유일한 existing group이 unlink이기 때문이다. worker가 arm 직전에
    `assert_row_jobs_free`를 단언하므로 `cleanup_next`가 None이다. 최종 실행의 N03·N04 trace에서도 drain의 첫 WAL append가
    35,248/19,032 B unlink frame이다.
  - N03·N04는 drain 시작 WAL 끝에 붙는 그 frame의 write·sync에서 멈춘다. N05는 anchor의 DWB image(16,544 B) sync와 DWB
    control 공개 뒤, home write(page 12)의 첫 byte에서 멈춘다. N06 crash는 선례 MCA03-R04와 같은 CURRENT(new, new) barrier다.
- READY의 전/뒤 기대도 복구 의미와 맞는다. stage 전·RAM install·1 byte torn tail은 옛 tree이고, sync된 scope4 frame과 DWB
  수리·CURRENT 공개는 새 tree다.
- oracle은 독립적이다. key와 행은 literal이다. 기대 tree는 produce pool image를 같은 walker로 걸어 만든다. 뒤 기대도 제품
  candidate를 부르지 않고 before tree의 route[1..]·survivor·목록 literal로 만든다.
- 내 사본에서 CI 셋 3개가 통과했다. controller의 UNL-N05-L도 통과했고, 세 phase의 namespace 파일이 author 최종 실행과 byte 단위로
  같았다.
- 필수 둘은 모두 문서 값이다.
  - 일정의 실제 구간과 날짜가 일정 문서 자신의 KST 규칙과 맞지 않는다.
  - 08-1을 "2단위"라고 쓴 값이 틀렸다(1단위 = 2일).
- 권고는 controller가 cut을 독립적으로 묶는 힘에 관한 것이다. 지금 코드에서 잘못된 cut이 통과하지는 않는다. 다만 worker가
  다르게 바뀌면 controller가 잡지 못하는 경우가 probe로 둘 확인됐다. 나머지 권고는 병합 전 gate·증거와 계약 머리말의 오래된
  상태다.

## 필수

### 필수 1. 08-1의 실제 구간·합계·개정 날짜가 일정 문서의 KST 규칙에 어긋난다

- 위치: `devdoc/plans/dev3-schedule-baseline.md:209`(`11-04→11-06 / 2일`), `:210`(`약 17일`), `:300`(`2026-11-06 / PR #59 병합`).
- 규칙: §4 첫 문단은 "실제 구간은 KST의 주요 작업 경계를 날짜 단위로 반올림했다. 날짜 차이를 쓰며"라고 정한다. §6 4번은
  "병합 commit/PR, 실제 착수/종료"를 갱신하라고 정한다.
  - 문서 자신도 KST를 쓴다. PR #47은 2026-11-04 02:45 KST(11-03 UTC)에 병합됐고 06의 끝과 §7 행이 `11-04`다.
- 사실:
  - PR #58은 2026-11-07 00:03 KST에 병합됐다(`3c9d796`).
  - U4 commit `cf46cc6`은 2026-11-07 00:50 KST다.
  - 검토 시점은 이미 11-07 01시 이후다.
  - 그러므로 PR #59는 빨라야 11-07 KST에 병합된다.
- 실패 시나리오: 그대로 병합하면 적립 대장이 08-1의 경과를 2일로, 합계를 약 17일로, V1 개정일을 11-06으로 남긴다. 실제는
  11-04→11-07 이후 3일 이상이고, 합계는 약 18일 이상, 개정일은 11-07 이후다. 날짜가 정확해야 하는 §4·§7의 기록 의무를 어긴다.
- 고칠 것: 병합 시점(KST)으로 세 값을 채운다. 병합일을 아직 모르면 "PR #59 병합일"로 두고 병합 때 갱신한다.

### 필수 2. "08-1(2단위)"가 일정의 단위 정의와 모순된다

- 위치: `devdoc/contracts/dev3-tree-smo.md:1944`(U4 결과, 이번에 새로 쓴 문장 "이 단계의 병합으로 08-1(2단위)을 완료로 적립한다").
  같은 값이 설계 `:1397`("U4가 끝나면 08-1(2단위)을 완료로 적립한다")에도 있다.
- 모순되는 정의:
  - 일정 §2.1(`dev3-schedule-baseline.md:58`)은 "1단위 = 2일"이다.
  - 같은 문서 `:81`은 "진행 중인 08의 08-1 1단위 포함"이다.
  - §2.2의 08-1 배정은 2일이다.
  - README는 "08-1(2일)"이다.
- 실패 시나리오: 계약이 08-1을 2단위(4일)로 적고 일정은 1단위(2일)로 적립한다. 적립값의 원 기록끼리 모순된다(잘못 쓴 값).
- 고칠 것: 두 곳을 "08-1(2일, 1단위)"로 바꾼다.

## 권고

### 권고 1. N03에서 torn write가 unlink frame의 것인지 controller가 보지 않는다

- 위치: `scripts/check_dev3_unlink_native.py:124-125`, `:145-146`.
- 현재 검사는 FIRST_BYTE write 행, 마지막 cut 행, WAL prefix hash, 그리고 끝에 붙은 1 byte `X`뿐이다. adapter는 막힌 write의
  ATTEMPT 행에 전체 길이를 남긴다. 최종 실행 N03-L은 `write d3-wal-00000001 9434850 35248 ATTEMPT`였다. 그러나 controller는 이
  행을 읽지 않는다.
- probe P1이 이를 보였다. drain 시작 WAL 끝에 붙는 **어느 frame**이든 첫 byte만 쓰고 멈추면 N03으로 통과한다. 예를 들어 drain이
  unlink 앞에 cleanup group을 하나 쓰는 이력이라면, 그 group에서 cut이 걸려도 READY "전"과 함께 통과한다.
  - 지금은 worker의 `assert_row_jobs_free`(`unlink_native.rs:581`) 덕분에 그런 group이 없다.
- 고칠 것: `effects[-3]`가 `write`·`wal_name`·offset = `receipt['wal_bytes']`·length = `want['frame']`·`ATTEMPT`인지 단언한다.
  N04·N05·N06의 frame 대조(길이·LSN·op·scope)와 같은 수준이 된다.

### 권고 2. hook cut(N01·N02)의 위치를 trace로 확인할 수 없는데, 한계에 적혀 있지 않다

- 위치: controller `:126-133`, worker `unlink_native.rs:489-499`, 계약 `dev3-tree-smo.md:1957-1958`·`:1987`·한계(`:2009~`).
- controller가 받는 조건은 셋이다: SIGABRT, arm 행(HOOK), arm 뒤 WAL 변경 없음.
- probe P5: worker가 hook을 arm한 뒤 drain을 시작하기 **전에** `std::process::abort()`하도록 바꿨다. 그 ELF로 controller를
  UNL-N01-R에 돌렸더니 **PASS**였다(produce -6 46.15 s, recover·reenter 0).
  - stage 호출과 무관한 SIGABRT도 같은 판정을 받는다는 뜻이다. stack overflow나 panic 중 panic도 SIGABRT다.
  - N01과 N02도 서로 구별되지 않는다. 최종 실행에서 두 사례의 trace는 inode 숫자만 다르다. 두 produce의 namespace 파일도
    같다(files.json 일치).
  - 따라서 hook의 위치는 worker 코드(Point 대응)와 in-process 전제(Row Job 없음)로만 묶인다.
- 고칠 것:
  - (a) arm 뒤에 unlink의 같은-cut canonical 준비가 WAL을 읽은 흔적을 요구한다. 최종 실행에서 arm 뒤 WAL read 성공 행은
    leaf 451, root 389였고, P5에서는 0이었다.
  - (b) `test.log`에 `panicked`, `overflowed its stack`, `fatal runtime error`가 없는지 본다.
  - (c) 계약 한계에 다음을 적는다: "hook cut의 위치와 N01·N02의 구분은 trace로 관측되지 않고 worker 코드와 arm 직전의 Row Job
    부재 단언으로만 결속된다."

### 권고 3. 병합 전 gate 기록·실행 증거·검토 연결이 없다(`cf46cc6` 기준)

- 내 사본에서 `check_module_gate.py check --base 3c9d796 --head cf46cc6 --mode merge`를 돌리니 **FAIL, 9 error**였다. 등록·의존
  오류는 없었다.
  - 4개 경로에 scoped CI evidence가 없다: `plan4_pages/native_tests.rs`, `unlink_native.rs`, `unlink_rows.rs`,
    `tests/fixtures/dev3-08-unlink/native-inventory.tsv`.
  - 5개 blob에 D1 review가 없다.
- 계약은 "CI는 다음 셋을 돈다"(`dev3-tree-smo.md` 메모리 쌍둥이 bullet)라고 쓴다. changed-paths 모드에서는 기록의 `rust_tests`가
  세 사례를 정확히 나열해야 그 문장이 참이 된다.
  - `native_tests.rs`에는 정확한 사례나 정확한 경로의 N/A가 필요하다. 이 파일의 타입을 쓰는 ignored가 아닌 사례로는
    `s6r_c02_native_recovery_row_census`가 있다. Trace는 `Arc` 뒤에 있어 그 크기가 바뀌지 않는다.
- RDL-N·RUP-N 선례처럼 실행 receipt와 source commit을 `module-gate/evidence/...`에 commit한다. U4 결과와 일정 08-1 행에서 그
  기록과 이 검토 기록을 연결한다.
- 일정 `:212`의 "각 단계는 독립 D2의 필수 지적을 해소했다"는 U4를 포함하므로 이 검토의 필수가 해소된 뒤에야 참이다.
- (author worktree에 커밋되지 않은 해당 작업이 보이지만 검토 범위 밖이다.)

### 권고 4. 계약 머리말의 오래된 상태가 이번에 더한 적립 문장과 한 문단에 있다

- 위치: `dev3-tree-smo.md:3`("실제 타입·F05·효과·fault 인수 미완료")와 `:7-11`("live admission은 아직 수용되지 않았다. 효과
  구현은 … 닫은 뒤 연결한다 … 08-1 항목은 U4 결과의 병합으로 적립한다").
- 효과·fault·live admission은 아래 절들(`#structural-effect` E10, `#current-leaf-consumers`, `#automatic-split`, U1–U4)에서
  닫혔다고 적었다. 그런데 머리말은 11-04 상태 그대로다. 08-1 적립을 선언하는 같은 문단이 "인수 미완료"라고 말하는 셈이다.
- 고칠 것: 머리말 상태를 현재 상태로 갱신하거나, 그 문장이 11-04 기준의 기록임을 표시한다.

## NIT

1. `unlink_native.rs:481`은 receipt의 `q`에 literal 5를 쓴다. 그래서 controller `:106`의 `int(receipt['q']) == 5`는 메아리만
   확인한다. 측정값을 쓰거나 검사를 뺀다. q 3..5는 produce 안에서 `committed(out)` 단언으로 이미 확인된다.
2. 계약 표의 N02는 "WAL·home 쓰기 없음"인데 controller(`:131-132`)는 WAL 변경만 본다. arm 뒤에 변경이 전혀 없음을 보면 표와
   같아진다. 실제 trace에는 변경이 없다.
3. N05에서 torn home page의 DWB 수리를 controller나 READY가 단언하지 않는다. 판정은 READY tree뿐이다.
   - 최종 실행의 recover trace에는 수리가 있다: `d3-dwb-images` read(16,544 B, event 1832), page 12 home read(1839), page 12
     write(1841). 이는 N04 recover의 일반 page 쓰기(3290 read, 3292 write)보다 앞선다.
   - 결과에 이 관측을 적거나, controller가 recover trace에서 이 순서를 단언하면 "DWB/home" cut의 수리 경로가 증거로 남는다.
   - torn 상태 자체는 진짜다. 수리 전 home page 12는 0 page(초기 image hash = zero 16 KiB의 SHA-256)였고, `X` 1 byte가 실제로
     page를 깨뜨렸다.
4. 일정 `:214`의 적립 이력 문장("V1은 34.30%에서 … PR #49(07)의 병합으로 38.37%가 된다")에 PR #59 → 40.70%가 없다.
5. walker는 XTR1 목록의 page를 decode하지 않는다(`unlink_native.rs:336-340`, identity·kind·level만 본다).
   - 목록의 RowLeaf가 cell 0개인지, RowInternal(RootCollapse의 옛 root)이 produce image와 같은지 보면 oracle이 제품 Base4 closure와
     따로 같은 사실을 확인한다.
   - internal separator는 따로 보지 않는다. 그래도 B cursor가 매 행을 직전 key로 다시 route하므로(`row/read/driver.rs`
     `capture_next`) 잘못 남은 separator는 행 누락으로 드러난다. 이 부분은 지적하지 않는다.

## 직접 확인한 것

명령은 모두 내 사본(`scratchpad/review-u4/src`)과 내 target에서 실행했다. `jenv2.sh`를 source했고 `-j 2`로 돌렸다.

1. **build.** `cargo test -p xadb-engine --lib --locked --no-run -j 2`가 성공했다(1분 52초). ELF sha256 `f68430cc…59a6`
   (`elf-orig`로 복사).
2. **source identity.** controller와 같은 방식(`git ls-files --cached --others --exclude-standard`)으로 manifest hash를 구했다.
   source `e09f199a…3566`, controller `734169e7…a37`, input `dfba91c2…d6dd`이다. 셋 다 author 최종 실행 `unl-final/execution-plan.json`과
   같다. 파일 목록도 `source-files.sha256`과 같다.
3. **CI 셋.** `elf-orig unlink_rows::native:: --test-threads 2`의 결과는 3 passed, 1 ignored(worker)였다(76.81초).
   `unl_n_inventory_states_the_module_rule`·`unl_n_root_memory_twin`·`unl_n_leaf_memory_twin` 모두 ok다.
4. **controller 1(무수정).** `python3 -B scripts/check_dev3_unlink_native.py --elf elf-orig --output review-u4/ctl-n05l --input
   tests/fixtures/dev3-08-unlink/native-inventory.tsv UNL-N05-L`의 결과는 PASS였다.
   - process: produce -6 76.55 s, recover 0 1.87 s, reenter 0 0.67 s.
   - produce receipt: lsn 167, wal_bytes 9,434,850, wal_sha256 `1a0b9476…`, anchor 12, emptied 13, root 34, flushed 12.
   - recover receipt: route 3,5, level 1, listed 13, high_water 168, rows 8, ack_q 6, handles 0.
   - 세 phase의 `files.json`이 author 최종 실행 UNL-N05-L과 **모두 같다**(15·18·18 파일). 다른 것은 log_bytes(733,490 대 731,199)뿐이다.
5. **author 최종 실행(읽기만).** `complete.json` 12 PASS, `results.json` 38 process(produce·crash −6, recover·reenter 0, 합 768.07초).
   N04와 N06 produce의 파일이 같고, N01과 N02 produce의 파일도 같다.
6. **trace 대조(최종 실행·시범 실행).**
   - N03-L: arm 뒤 read뿐이다. 이어서 `write d3-wal-00000001 9434850 35248 ATTEMPT`, `FIRST_BYTE:Ok(1)`, cut 순이다.
   - N04-L·R: 35,248·19,032 B write, sync Ok, cut 순이다.
   - N05-L: DWB stage·slot·select 공개, `d3-dwb-images` 16,544 B write·sync, 두 번째 select 공개, `d3-data` offset 196,608
     (page 12) FIRST_BYTE 순이다. 최종 db의 page 12는 `XDP1`로 시작한다.
   - N06 crash: `d3-current-stage` publish 3회, `d3-rebase-0` sync 1회, 마지막 dirsync 뒤 cut, remove 없음이다. 선례
     `row_inventory_a/native.rs`의 MCA03-R04(`[current×3]`, dirsync After)와 같다.
   - N01·N02: arm 뒤 변경이 없다.
7. **frame 형식.** controller의 offset은 맞다.
   - `wal_frame.rs:445`가 길이 [12..16], `:452`가 LSN [56..64]다.
   - `decode_layout`의 group은 payload[..192]이고 payload는 160부터다. 그래서 op = group[6..8] = 절대 166..168, scope =
     group[10..12] = 170..172(값 4)다.
   - `structure.rs:42-61`: LeafUnlink 3 = 35,248 B, RootCollapse 4 = 19,032 B. 계약 표의 산술(160+192+1536+256+384+32,688+32,
     160+192+1536+256+512+16,344+32)과도 같다.
8. **Trace 초기화.** plan4_pages `Trace`를 만드는 곳은 둘뿐이다(`native_tests.rs:735`, `:807`). 둘 다 4 MiB다.
   `raise_log_bound`는 `events == 0`일 때만 올리고 내리지 못한다. 다른 이름 같은 `Trace`들은 별개 타입이다.
9. **probe(내 사본만, 끝난 뒤 `git checkout`으로 복원, `cmp`로 원본과 같음 확인).**
   - P1(Python, controller 함수 직접 호출): 합성 N03 입력에서 ATTEMPT 길이 35,248과 2,144가 둘 다 `check_produce`를 통과했다
     (권고 1).
   - P2(Rust): `memory_twin`이 옛 사례(N01)를 unlink **뒤** 파일에서 재시작하게 바꿨다. `unl_n_root_memory_twin`이 FAILED였다
     (`expect`: left `[[4, 5, 6, 7, 8]]`, right `[[], [4, 5, 6, 7, 8]]`). drain image를 unlink 뒤에 잡으면 쌍둥이가 실패한다
     (`assert_ne!(drain, after)`도 먼저 막는다).
   - P5(Rust와 controller, 두 번째 controller 사례): `produce`에서 `sink.save` 직후, drain 전에 stage_before일 때
     `std::process::abort()`하게 바꿨다. `check_dev3_unlink_native.py … UNL-N01-R`의 결과는 **PASS**였다(권고 2). arm 뒤 read 행은
     0이었다(정상 N01-R은 WAL read 389).
10. **기타 검사.**
    - `cargo fmt --all --check` PASS.
    - `cargo clippy -p xadb-engine --all-targets --all-features --locked -j 2 -- -D warnings` PASS. engine package만이고
      workspace 전체는 아니다.
    - `git archive cf46cc6` 사본에서 `devdoc/verify_documents.py` PASS(markdown 287).
    - `check_module_gate.py check --mode merge`는 FAIL 9 error였다. 기록·검토 부재만이다(권고 3).
11. **일정 산술.** 다음 값을 모두 확인했다: 35/86 = 40.70%, 잔여 59.30%, 미적립 51일, 17.5단위 = 35일(16.5 + 08-1 1),
    (33+2)/86, §7 행. 날짜와 단위만 틀렸다(필수 1·2). 08-1이 08의 마지막 항목이 아니라 항목 조건으로 적립한다는 판단과,
    "병합 시" 표기는 §6 2번 및 06·07 선례와 맞는다. 다만 §3 첫 문단의 "독립 검토 필수 지적 해소"와 "증거 링크"는 이 검토의
    필수 해소와 권고 3의 증거 연결이 끝나야 충족된다.
12. **정리.** 내 target, ELF 두 개(`elf-orig`, `elf-probe`), 문서 검사 사본, controller 출력의 db 디렉터리는 삭제했다. receipt와
    trace는 `scratchpad/review-u4/ctl-*`에 남겼다.

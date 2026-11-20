Verdict: ACCEPT

# DEV3 Catalog eviction(catalog-capture-load) 독립 D2(common) 검토 (3회차: 보완분과 module gate record)

2026-11-08. 검토자는 구현자와 독립이며 1·2회차(`review-r1.md`, `review-r2.md`)와 같다. 소스는 고치지 않았다. 이
checkout에는 이 파일만 더한다. mutation·probe·base 비교·단독 export 빌드는 scratch(`git worktree add … HEAD`,
별도 `CARGO_TARGET_DIR`)에서 했다. 끝난 뒤 worktree, target, export tree를 지웠다. `scripts/dev3_catalog_native.py`는
지시에 따라 돌리지 않았다.

## 대상

- (a) 보완분 `git diff f8f4ea89..73a51529`: 파일 4개(+143 / −9). `catalog_eviction.rs`에 CEV-J06과 N7 주석 수정,
  `read/lookup.rs` 시험 모듈의 CEV-C01에서 `assert_eq!(full, peak + held)` 한 줄 삭제(N8), 계약
  `dev3-visibility-retirement.md`의 시험 목록 한 줄, `review-r2.md` 등록. 제품 코드 변경은 없다.
- (b) `08dc579e` "wip record": `module-gate/records/20261108-catalog-capture-load.json` 한 파일만 더한다(+929).
  코드는 `73a51529`와 같다. `reviews[0].log`는 `PLACEHOLDER`이다(이 파일과 sha256으로 채울 예정).
- 전체 범위 `origin/claude/project-thread-y6amas`(`0197f199`)..HEAD: 파일 12개, 그중 Rust 7개(+746 / −38).
- 기준: 2회차 R4'(J06 요청), N7, N8. record 형식은 `module-gate/README.md`와
  `repository-build-plan.md#module-gate-scoped-tests`(`reason`은 변경과 시험의 관계를 설명한다)를 따랐다.

## 돌린 것

환경은 1·2회차와 같다(lib test binary, `XADB_CATALOG_SOURCE/ELF/INPUT_SHA256`, `RUST_MIN_STACK=33554432`,
`--exact --nocapture --test-threads=1`). 전체 suite는 돌리지 않았다.

| 실행 | tree | 결과 |
| --- | --- | --- |
| record의 고유 사례 114개(mapping 0: 72, 1: 43, 2: 7, 중복 제거) | HEAD(main checkout의 head binary, elf `f43e7de9…`, 시험 2,486개) | 114 passed, 0 ignored. 모든 이름이 binary에 정확히 있다 |
| census 43개(mapping 1) | HEAD | 43 passed. 값 줄 65종이 아래 base 출력과 한 줄도 다르지 않다. 추가된 줄은 `CEV_C01 catalog_load=34452 frame=280 other_peak=34540 margin=88 held=48 full=34588` 하나다. `CATALOG_PRODUCT_SOURCE actual=2042 bound=2042 step=328 result=328`, `pending_full=65486`(native type 65,494) ≤ 65,536 |
| census 42개(CEV-C01 제외) | base `0197f199`(scratch) | 42 passed |
| **M0**: `select`의 `load_catalog` 호출 삭제 | scratch | J01 FAIL(`[]` pass, `[7]`에서 104행), J02(216행), J03(323행), J04(390행), J05(`first=6`, 482행), **J06(541행)**, SPL-A05(`auto_split.rs:660`) FAIL. 모두 `Frame(MissingTarget)` Unchanged. CEV-C01 pass |
| **M2+M5**: 보호 집합 `&requests[1 - at..2 - at]` → `&[]`, `catalog_load_frame_source_bytes`에 +89 B | scratch | J04 FAIL(390행), CEV-C01 FAIL(393행 "the Catalog LOAD phase raises the Full charge"; `catalog_load=34541 margin=0 full=34589`). J01–J03·J05·J06 pass |
| **M4**: `resident_address_present` 검사 삭제 | scratch | J01만 FAIL(`evicted=[]`, 157행, `issue=(4, 1) lookup=(4, 1)` ≠ `(2, 1)`). J02–J06 pass |
| probe: `load_catalog` 입구와 `catalog_requests`에 `eprintln!` | scratch | mapping 0의 72개 중 catalog_memory 61개: 53개가 `load_catalog`에 들어간다(Full 또는 resident). `table_issue::capture_without_visibility_proof_cannot_issue_a_table`은 `catalog_requests`(`read_catalog`)만 지난다. **7개는 둘 다 지나지 않는다**(R1). 단위 사례 11개 중 `read::catalog`의 original_owner 2개만 `catalog_requests`를 지난다 |
| 단독 export `module_single_source.py --side a|b --policy origin/claude/project-thread-y6amas --commit HEAD` | scratch | 둘 다 `cargo test -p xadb-engine --locked --no-run` 성공. `catalog_eviction.rs`·`row_preflight.rs`는 양쪽에 남고 `auto_split.rs`는 양쪽에서 빠진다. export-a: F2 `S2W-R10/R11/R21-LIVE`(`catalog_memory.rs` 변경으로 필수) 3 passed, CEV-J01..J06 6 passed. export-b: CEV-J01..J06 6 passed |
| `check_module_gate.py check --base origin/claude/project-thread-y6amas --head HEAD` | HEAD | FAIL 1건: `CAT changed: attested script:scripts/dev3_catalog_native.py* at this head's inputs e0bd06141702 is missing`. NOTE 2건(`catalog/read.rs`, `read/operation.rs`: D2 common). 다른 오류는 없다 |
| `check_module_gate.py plan` | HEAD | head 사례 114개, 계약 3개(export-a), export a(사례 3개)·b(빌드만) |
| `reviews[0].files` 대조 | HEAD | 24개 모두 `git rev-parse HEAD:<path>`와 같다. 변경 Rust 7개와 선택 시험 파일 17개를 덮는다. mapping의 `paths`·`cases[].file` 중 빠진 것은 없다 |

돌리지 않은 것: `scripts/dev3_catalog_native.py`(지시), `devdoc/verify_documents.py`(1회차와 같이 `markdown_it` 모듈 없음).

방법 메모: 같은 `CARGO_TARGET_DIR`을 다른 tree에 쓰면, 상대 경로가 같고 원본 mtime이 이전 산출물보다 오래된 경우 cargo가
다시 빌드하지 않는다. 처음 돌린 HEAD census는 base binary(elf `093e707a…`)를 재사용했으므로 버렸다. 위 표의 HEAD census는
main checkout의 head binary로 다시 돌린 결과다(elf와 시험 수 2,486으로 확인). mutation·probe·export 실행은 각각 파일을 바꾼
뒤 빌드했고, elf가 매번 다르며 head에만 있는 행(J06 541행)에서 실패하는 것으로 새 binary임을 확인했다.

## 판단

### (a) 보완분

- **CEV-J06**(2회차 R4' 해소): T의 CREATE가 Active일 때 leaf만 flush·evict하고, anchor는 resident임을 단언한다. 새
  snapshot의 legacy `lookup_catalog(6)` 한 번이 leaf를 LOAD(replay 이미지와 home 이미지가 같아야 함)하고 Active
  Status·Undo를 읽어 `Missing`을 답한다. 뒤에는 두 page가 resident이고, 파일은 그대로이며, reference pin 0,
  `active_read_operations` 0, fence 없음을 단언한다. M0에서 541행 `Frame(MissingTarget)`으로 실패한다. 계약 시험 목록의
  설명과 같다.
- **N7**: `STATUS_READ`와 J01 주석이 이제 "thread에서 연 replay scope, history의 replay checkout"이라고 센 대상을 정확히
  말한다.
- **N8**: CEV-C01은 이제 `load <= peak`만 단언한다(값은 출력한다). 지운 등식은 공식의 항등식이라 아무것도 잡지 못했다. M5
  (+89 B)는 여전히 393행에서 잡힌다.
- 제품 코드는 2회차와 같다. 위 실행에서 `load_catalog`·`select`의 동작과 census 값이 바뀌지 않았다.

### (b) record

**형식과 범위.** schema `XADB_MODULE_GATE_RECORD_V1`. `rust_tests` 3개, `reviews` 1개(`catalog-capture-load-d2`, D2
`common`, ACCEPT, `required_open` 0). `na`·`attested`·`python_tests`는 없다. 변경 Rust 7개는 모두 mapping에 들어 있고
(`read/operation.rs`는 mapping 0과 1 양쪽), 문서·record·evidence 파일은 Rust 입력이 아니다. gate도 경로 누락을 보고하지
않는다. `change`가 인용한 `e0723ae`는 `origin/main`이고 `0197f199`의 조상이며, 그 사이에 `read/catalog.rs`와
`dirty_frame/`이 바뀌지 않았다(1회차 확인).

**경로별 충분성.**

- `catalog/read.rs`(`select`가 `load_catalog`를 먼저 부름, `source_bytes`에 load step 결과 carrier 반영): M0을 J01–J06과
  SPL-A05 7개가 잡는다. `source_bytes`는 `product_carriers_fit_the_lower_fixed_budget`가 묶는다(`actual=bound=2042`,
  base와 같다).
- `read/operation.rs`의 `load_catalog`: 각 분기에 실패하는 사례가 있다. resident 검사 생략(M4)은 J01이, 보호 집합(M2)은
  J04가, dirty victim의 `FlushRequired`는 J02·J04가, resident 클래스의 즉시 `Ready`는 J03이 잡는다. probe 결과 기존 53개가
  page가 resident인 `load_catalog`를 지나며 모두 통과한다. 그러므로 "resident 조회는 전과 같다"는 실제로 실행되어 확인된다.
  `held_source_bytes` 분해(`held_before_capture` + `captured_copy`)는 census 43개가 base와 같은 값으로 묶는다.
- `read/catalog.rs`의 `catalog_requests`: `read_catalog` 경로는 `row_read_live` 9개(사례당 `catalog_requests` 2–10회),
  `read::catalog`의 original_owner 단위 2개, `table_issue` capture 사례가 지난다. `load_catalog` 경로는 위와 같다.
- `read/lookup.rs`의 phase 공식: CEV-C01(M5로 확인)과 census 42개. base와 값이 같다.
- 시험 파일 3개: mapping 2의 7개가 새·바뀐 사례 전부다. `catalog_memory.rs` 변경(`mod catalog_eviction;` 선언 3줄)은
  신뢰 manifest의 F2 계약 3개를 export-a에서 자동으로 부른다. scratch에서 export-a·b 빌드와 해당 사례가 통과했다.
- 선택 밖 기존 시험 중 Full 조회의 `MissingTarget`을 기대하는 것은 없다(1회차 검색). 따라서 빠진 회귀 사례는 찾지 못했다.

**`reason` 정확성.** mapping 0의 M0·M2·M4 설명("J01 (evicted), J02–J06 and SPL-A05 fail Frame(MissingTarget)
Unchanged", J04 "fails with the protected set emptied", J01 "fails with the probe removed", 수정 전 `(2, 1)`)은 위 실행과
같다. mapping 1의 34,452 / 34,540 / 34,588 B와 "CEV-C01 … fails with the frame +89 B", "the same values as the base
0197f199", 65,486 ≤ 65,536도 같다. mapping 2의 "its own files helper"(`catalog_eviction.rs:27`)와 SPL-A05의
`[false, true]`와 뒤의 두 resident 단언(`auto_split.rs:658–664`)도 같다. `catalog_eviction`이 쓰는 `row_preflight`
helper는 export-b(A 제거)에도 남아 빌드되므로 "no A import"도 성립한다. 예외는 R1이다.

**`reviews[0].files`.** 24개가 HEAD blob과 같다. 변경 Rust 7개와 선택 사례가 있는 시험 파일 17개(변경 안 된 것 포함)를
덮는다. README의 "unchanged test/input blobs도 넣는다" 요구를 충족한다. 명시 `inputs`는 없다. 이후 24개 중 하나라도 바뀌면
이 검토는 무효다.

## Findings

필수 없음.

**R1 (권고) mapping 0의 `reason`이 select에 닿지 않는 사례를 "select users"로 적는다.**
`module-gate/records/20261108-catalog-capture-load.json` `rust_tests[0].reason`은 "The unchanged select users are re-run
so a resident lookup keeps its behaviour: … admitted_validation, … commit_concurrency (lookups beside Catalog commits)
…"라고 적는다. probe로 보면 다음 catalog_memory 7개는 `load_catalog`(= `select`)와 `catalog_requests` 어느 쪽에도
들어가지 않는다.
`admitted_validation::{already_observed_pool_poison_precedes_capacity, capacity_and_original_admission_precede_unobserved_metadata_faults}`,
`call_scope::direct_error_finish_keeps_priority_and_original_accounting`,
`commit_concurrency::{catalog_commit_reservation_capacity_reopens_reads, catalog_mt05_pending_lower_flush, catalog_mt06_flush_capture_then_commit}`,
`statement_reads::missing_original_visibility_is_rejected_before_admission`.
이들은 고정 Full charge의 admission·capacity 순서(예: `admitted_validation.rs:33`의 `catalog_test_fixed_charges`,
`commit_concurrency.rs:886–887`의 commit reservation capacity)나 commit·flush 순서를 묶는다. 변경과의 관계는 `select`가 아니라
`read/lookup.rs`의 census(값 34,588 유지)다. `read::catalog` 단위 4개(`catalog_read_nonempty_physical_versions_and_absence`,
`catalog_read_rejects_corrupt_unselected_values_and_identity`, `catalog_read_source_reservation_fits_existing_peak`,
`unl_c01_undo_decode_term_covers_the_list_check`)와 `read::statement` 단위 4개도 실행 중에 바뀐 함수에 닿지 않는다.
이들은 carrier·decoder census 사례다. 충분성에는 영향이 없다(위 53개가 바뀐 경로를 실제로 지나고, 추가 사례가 증거를 약하게
하지는 않는다).
실패 시나리오: 이 record를 근거로 삼는 이후 변경의 작성자나 검토자가 "admission·commit 사례가 Full 조회의 resident 경로를
확인한다"고 읽는다. 그러면 그 사례만 남기고 실제로 `select`를 지나는 사례를 빼도 된다고 판단할 수 있다. 계약은
"`reason`은 변경과 시험의 관계를 설명한다"를 요구한다.
고칠 방향: reason에 "admitted_validation 2, statement_reads의 before-admission 1, call_scope의 direct-error 1,
commit_concurrency의 MCW02·MT05·MT06은 select에 닿지 않는다. 이들은 census가 유지하는 Full charge와 admission·commit
순서를 묶는다"를 적는다. 또는 이 사례들을 mapping 1(`read/lookup.rs`)로 옮긴다. 프로젝트의 "바뀐 코드의 시험만" 규칙을 엄격히
보면 빼도 된다. 어느 쪽이든 `reviews[0].files`의 blob은 바뀌지 않는다.

**N1 (NIT) SPL-A05 설명의 "anchor or leaf".** `auto_split.rs:586–587` doc 주석은 "the Catalog anchor or leaf has left
the pool"이라고 적는다. 그런데 같은 사례는 `[false, true]`(anchor만 빠짐, 658행)를 단언하고 바로 위 주석과 record는 "took
the anchor's frame"이라고 적는다. "the Catalog anchor has left the pool"로 맞추면 된다. "split의 page가 anchor의 frame을
가져갔다"는 단언되지 않은 설명이다(단언은 anchor 비상주·leaf 상주뿐이다). 동작 문제는 아니다.

## 남은 것(이 검토의 판정과 별개)

- gate의 유일한 오류는 CAT native attestation이다(`dev3_catalog_native.py`, head 입력 `e0bd06141702`). 60 GiB 디스크가
  필요한 실행이라 이 검토는 돌리지 않았다. controller가 결과를 `attested`에 넣어야 gate가 통과한다.
- `reviews[0].log`의 `PLACEHOLDER`를 이 파일 경로와 sha256으로 채운다. R1·N1을 반영하면 record 문장이나 주석만 바뀐다. 주석을
  고치면 `auto_split.rs` blob이 바뀌므로 `files`를 다시 출력하고 그 한 파일을 다시 확인해야 한다.
- 실제 승인은 사람의 merge다. 이 record는 승인이 아니다.

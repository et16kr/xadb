Verdict: ACCEPT

# DEV3 Catalog eviction(catalog-capture-load) 독립 D2(common) 검토 (2회차)

2026-11-08. 검토자는 구현자와 독립이며 1회차(`review-r1.md`)와 같다. 소스는 고치지 않았다. 이 checkout에는 이 파일만
더한다. mutation·probe는 scratch worktree(`git worktree add … HEAD`, 별도 `CARGO_TARGET_DIR`)에서 했고, 끝난 뒤
worktree와 target을 지웠다.

## 대상

- 이번 범위: `git -C /home/user/xadb diff 814628bd..f8f4ea89`(1회차 head → head `f8f4ea899dd403fbb7a12cbdbe38a06cd14fc8e7`
  "apply the first D2 review"). 파일 8개(+429 / −30). 1회차 `review-r1.md` 등록이 들어 있다.
- 전체 범위 `origin/claude/project-thread-y6amas`(`0197f199`)..HEAD도 다시 읽었다. 이번 commit의 제품 변경은 census 함수
  이름 변경(`catalog_load_frame_source_bytes`)과 `lookup_source_bytes` 분해뿐이다. `load_catalog`·`select`의 동작은
  1회차와 같다.
- 기준: 1회차 F1–F2, R1–R5, N1–N6.

## 돌린 것

환경: 1회차와 같다(lib test binary, `XADB_CATALOG_SOURCE/ELF/INPUT_SHA256`, `RUST_MIN_STACK=33554432`,
`--exact --nocapture --test-threads=1`). 전체 suite는 돌리지 않았다.

| 실행 | tree | 결과 |
| --- | --- | --- |
| CEV-J01..J05, CEV-C01, `product_carriers_fit_the_lower_fixed_budget` | HEAD | 7 passed. `CEV_C01 catalog_load=34452 frame=280 other_peak=34540 margin=88 held=48 full=34588`. J01 `issue=(2,1)/(3,2)/(3,2)/(4,3)`, `lookup=(2,1)`(evicted `[]`/`[7]`/`[8]`/`[7,8]`) |
| census 묶음 43개 | HEAD | 43 passed. 출력이 1회차 HEAD와 줄 단위로 같다(CEV-C01 줄 제외). base `0197f199` 출력과도 같다. Full 34,588, `pending_full` 65,486(native type 65,494) |
| table issue·lookup 관련 68개 | HEAD | 67 passed, 1 ignored(`s5r_n01_native_worker`) |
| SPL-A05 | HEAD | passed. `catalog_resident_before_read=[false, true]` |
| **M0**: `select`의 `load_catalog` 호출 삭제 | scratch | J01 FAIL(`[]`는 pass, `[7]`에서 104행), J02 FAIL(215행), J03 FAIL(322행), J04 FAIL(389행), J05 FAIL(`first=6`, 481행), SPL-A05 FAIL(660행 `read_big!`). 모두 `Frame(MissingTarget)` Unchanged. SPL-A05의 새 `assert_eq!(catalog, [false, true])`(658행)는 수정 전에도 성립한다(fixture 전제). CEV-C01 pass(census는 호출과 무관) |
| **M2**: 보호 집합 `&requests[1 - at..2 - at]` → `&[]` | scratch | **J04 FAIL**(389행 `Frame(MissingTarget)` Unchanged). J01·J02·J03·J05 pass |
| **M5**: `catalog_load_frame_source_bytes`에 89 B 더함(위 M2와 같은 build) | scratch | **CEV-C01 FAIL** "the Catalog LOAD phase raises the Full charge"(`catalog_load=34541 other_peak=34540`). 같은 build에서 `repro_full_read_beside_pending_commit_fits_actual_charges`는 pass. 1회차 F1에서 말한 대로, 이 크기의 증가를 잡는 것은 CEV-C01뿐이다 |
| **M4**: `resident_address_present` probe 삭제 | scratch | **J01 FAIL**(`evicted=[]`에서 `issue=(4,1) lookup=(4,1)`, 156행). J02..J05 pass |
| probe(scratch 전용, 아래 R4): 미commit CREATE 뒤 leaf eviction | scratch | pass. leaf를 LOAD한 뒤 Status·Undo를 읽어 `Missing`. 끝난 뒤 leaf·anchor resident, data image 불변, reference 0, fence 없음 |
| `cargo fmt -p xadb-engine --check` | HEAD | 깨끗 |
| `python3 scripts/check_module_gate.py check --base origin/claude/project-thread-y6amas --head HEAD` | HEAD | FAIL 14건. 1회차와 같은 종류다. scoped CI evidence 없음 6. review 없음 7(`catalog/read.rs`·`read/operation.rs`는 D2 common, 나머지 5개는 D1, 새 blob 기준). CAT native attestation 없음 1(inputs `20e6b4774f7f`) |

구현자 log(`review-r2-inputs/`의 m2·m4·j01-m0·r1fix-cev)와 내 실행 결과가 같다.

## 1회차 지적의 처리

| 지적 | 처리 | 판단 |
| --- | --- | --- |
| F1(필수) CEV-C01 동어반복, 계약 수 | `lookup.rs:292` `lookup_phase_source_bytes`(LOAD 단계를 뺀 phase peak, held 제외)를 분리했다. `286` `lookup_source_bytes = phase.max(catalog_load) + held`. CEV-C01은 `load <= peak`(394)와 `full == peak + held`(397)를 단언한다. 계약 84행은 1,044 + 2,042 + 280 + 31,086 = 34,452, 다른 단계 peak 34,540, 여유 88 B, held 48 B, Full 34,588 B로 고쳤다 | **해소.** `lookup_phase_source_bytes`는 수정 전 공식에서 LOAD 항만 뺀 것이다(max의 순서만 다르다). 그러므로 `load ≤ peak`이면 charge가 수정 전과 같다. M5(+89 B)가 이 단언에서 깨진다. 397행은 394행이 참이면 구조상 참이지만 해는 없다. 계약 수는 내 보조 출력과 같다 |
| F2(필수) 보호 집합 미시험 | CEV-J04(349행): 두 page를 내보내고 clean frame 하나만 남긴 채 Full `lookup` → `CatalogLookupStep::FlushRequired`(dirty page), anchor는 남긴 frame에 그대로, leaf 비상주, 효과 없음. flush 뒤 같은 statement가 leaf를 LOAD하고 T를 찾는다 | **해소.** M2에서 J04만 `Frame(MissingTarget)`으로 깨진다. 기대값(`Some(kept)`, dirty 목록 포함, 0 등록)은 fixture 사실이다. 같은 시험이 R4의 조회 FlushRequired 왕복도 덮는다 |
| R1 SPL-A05 전제 | `auto_split.rs:658` `assert_eq!(catalog, [false, true])` | 해소. M0에서도 성립하므로 fixture 전제이고 producer 값이 아니다 |
| R2 경합·결과 목록 | 계약 81행 "Full 클래스의 결과": FlushRequired, Load 단계 Busy, probe/LOAD→capture 창의 `Frame(MissingTarget)`, Status LOAD와 같은 창, 같은 statement 재시도 | 해소. `select_load`(Busy: 후보 없음, `state.load`, init claim)와 `reserve_target_capacity`의 동작과 맞다 |
| R3 resident 조회 관측 | J01에 `evicted=[]`를 더하고 replay (scope, checkout) 증가량을 단언한다. issue = `STATUS_READ + (n, n)`, resident lookup = `STATUS_READ = (2,1)`. `STATUS_READ`는 `load_catalog` 호출을 지운 코드(수정 전 동작)에서 잰 값이다 | 해소. M4가 J01에서 깨진다. 이 기준값의 주장은 "수정 전과 같다"이므로, 수정 전 코드에서 잰 값은 producer 값이 아니라 기준이다 |
| R4 빠진 사례 | J05(eviction 뒤 첫 Full 조회, 6 Found·99 Missing, catalog flag 등록), J04(조회 FlushRequired). 남은 것: 다른 Owner claim 아래 Busy, 미commit CREATE | 권고 수준으로 받아들인다. 아래 R4' |
| R5 Begin | 계약 85행을 "미배정 의무"로 바꿨다. System 1–6 eviction과 Begin MissingTarget 시나리오, 소유자·일정 없음 | 해소 |
| N1·N2·N4·N5·N6 | 4장 1단계(66행), `read_catalog` 주석(B 포함), `catalog_load_frame_source_bytes` 이름 변경, UNL-L09 "수정 전 측정" 표기, 08-3 예 삭제 | 해소 |
| N3 request builder 중복 | 하지 않음(B의 `#[inline(never)]` census 대상 코드) | 받아들인다. NIT였다 |

## Findings

필수 지적은 없다.

### R4' (권고) 남은 두 사례

- **미commit CREATE 뒤의 leaf LOAD.** scratch probe(`create_forward`로 Active CREATE만 둔 12-frame pool에서 leaf를 flush·evict한 뒤,
  새 snapshot의 legacy Full `lookup_catalog(6)`)가 통과한다. leaf LOAD의 replay=home 검산(`lookup_load.rs` 250–271)이
  미commit record가 든 image에서도 맞고, 이어진 Status·Undo 읽기가 `Missing`을 낸다. 이 fixture에서는 비상주 Jobs page가
  하나뿐이라 anchor는 내보내지 못했다(anchor는 resident). 결과가 맞으므로 필수는 아니다. 다만 이 경우는 불일치가 곧 fatal
  (fence)인 경로라 CEV에 넣기를 권한다(J06 후보).
- **다른 Owner의 claim 아래 Busy.** C 경로는 `load_lookup_target`의 Unchanged 오류를 `finish_error`로 넘길 뿐이다. LOAD 선택 뒤
  INIT pair close와의 중재(Busy)는 pool 수준 시험 `pair_shared_close_after_load_observation_rechecks_resident_and_unrelated_victim`
  (`dirty_init/tx_pair_shared_tests.rs`, `LoadCleanSelected` 지점)이 본다. 다른 Owner의 LOAD claim(`state.load`) 아래 Busy는
  `select_load`의 같은 Unchanged 반환이고, C 쪽에서 따로 볼 동작이 없다. 비워 둬도 된다.

### N7 (NIT) replay 수 주석

- `catalog_eviction.rs:150`의 "(replay scopes, checkouts) of this thread"는 scope 수에만 맞다. scope는 thread-local
  `REPLAY_SCOPES`이고 checkout은 history의 `last_attempt`(`history/runtime.rs:101`)다. 한 case가 한 thread에서 도는 지금은
  값이 같다.

### N8 (NIT) CEV-C01의 두 번째 단언

- `lookup.rs:397` `assert_eq!(full, peak + held)`는 394행이 참이면 `lookup_source_bytes`의 정의상 참이다. 남겨도 해는 없다.
  고정값 확인이 필요하면 base에서 잰 34,588을 fixture로 두면 된다. 이번 판정에는 영향이 없다.

## 판정

ACCEPT. F1·F2는 mutation(M5·M2)으로 깨지는 시험이 생겨 해소됐다. R1·R3도 mutation(M0 전제, M4)으로 확인했다. 계약의 수와
결과 목록이 code·출력과 맞는다. 남은 것은 권고 R4'와 NIT 둘이다. module gate의 record·review 등록·scoped CI evidence·CAT native
attestation(새 frame 280 B를 포함한 native 측정)은 이 검토와 별도로 merge 전에 채워야 한다.

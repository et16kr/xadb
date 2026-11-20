Verdict: CONDITIONAL

# DEV3 Catalog eviction(catalog-capture-load) 독립 D2(common) 검토 (1회차)

2026-11-08. 검토자는 구현자와 독립이다. 소스는 고치지 않았다. 이 checkout에는 이 파일만 둔다. revert·mutation·probe는 모두
scratch worktree(`git worktree add … HEAD`, 별도 `CARGO_TARGET_DIR`)에서 했고, 끝난 뒤 worktree와 target을 지웠다.

## 대상

- 범위: `git -C /home/user/xadb diff origin/claude/project-thread-y6amas..HEAD`
  (base `0197f199`, branch `claude/project-thread-fk09g4`, head `814628bdcdf19b8cebc1ee95738fabd09ae2fc2b`). commit 셋:
  `9d527c96`(repro), `0943b9af`(수정), `814628bd`(시험 helper 이동). 파일 9개(+494 / −39).
- 제품: `catalog/read.rs`(`select`가 `load_catalog`를 먼저 부름, product carrier), `plan4_retention/read/operation.rs`
  (`load_catalog`, held 분리, frame 항), `read/catalog.rs`(`catalog_requests` 추출), `read/lookup.rs`(LOAD 단계 census, CEV-C01).
- 시험: `catalog_eviction.rs`(CEV-J01..J03), `auto_split.rs` SPL-A05의 read 추가.
- 계약: `dev3-visibility-retirement.md#catalog-capture-load`, `dev3-tree-smo.md` 1103·1121–1125·1899–1901행.
- 같이 읽은 것: `lookup_load.rs` 전부(`load_lookup_target`, `resolve_lookup_claim`, `target_membership`, census),
  `dirty_frame/reader_load.rs`(`claim_retention_load`, `select_load`, `load_eligible`), `dirty_frame.rs`
  `reserve_target_capacity`, `status_residency.rs` `resident_address_present`, `registration.rs` `address_present`·`resolve`,
  `read/statement.rs`·`read/scope.rs`의 finish 경로, B의 `plan4_apply/row/read.rs` `load_catalog`(446–466),
  writer의 `plan4_retention/load.rs` `prepare_catalog`, `row_start.rs` `row_begin`.

## 돌린 것

환경: 지정된 runner와 같은 방식(lib test binary, `XADB_CATALOG_SOURCE/ELF/INPUT_SHA256`, `RUST_MIN_STACK=33554432`,
`--exact --nocapture --test-threads=1`). 전체 suite는 돌리지 않았다.

| 실행 | tree | 결과 |
| --- | --- | --- |
| CEV-J01·J02·J03, CEV-C01, `product_carriers_fit_the_lower_fixed_budget` | HEAD | 5 passed. `CEV_C01 catalog_load=34452 frame=280 full=34588`, `CATALOG_PRODUCT_SOURCE actual=2042 bound=2042` |
| census 묶음 43개(`census-list.txt`) | HEAD | 43 passed. Full 34,588, `pending_full=65486`(native type 65,494) |
| 같은 묶음에서 CEV-C01을 뺀 42개 | base `0197f199` | 42 passed. HEAD 출력과 줄 단위 비교: 다른 줄은 filtered-out 수(2478→2482)와 CEV-C01 줄뿐. Full·resident·shared·pending·commit 창 수치는 모두 같다 |
| table issue·lookup 관련 68개(`related.txt`) | HEAD | 67 passed, 1 ignored(`s5r_n01_native_worker`, 측정 전용) |
| small pool 8개(SPL-A01..A06·A10, UNL-L09) | HEAD | 8 passed(29–186 s). `SPL_A05 … catalog_resident_before_read=[false, true]` |
| **revert**: `select`의 `load_catalog` 호출만 지움 | scratch | CEV-J01 FAIL(96행 `Frame(MissingTarget)` Unchanged), CEV-J02 FAIL(192행 같은 오류), CEV-J03 FAIL(299행, 뒤쪽 Full 호출; resident 부분은 통과), SPL-A05 FAIL(657행 `read_big!`, 같은 오류), CEV-C01 pass |
| **M2**: `load_catalog`의 보호 집합 `&requests[1 - at..2 - at]` → `&[]` | scratch | CEV-J01·J02·J03 **모두 pass**(살아남음). 아래 probe는 FAIL |
| **M4**: `resident_address_present` probe 삭제(항상 `load_lookup_target`) | scratch | CEV-J01..J03 + 관련 68개 **모두 pass**(살아남음) |
| probe(scratch 전용 시험, 아래 F2) | HEAD / HEAD+M2 | HEAD pass(FlushRequired page 9, anchor는 frame 5에 남음) / M2 FAIL `Frame(MissingTarget)` Unchanged |
| 보조 출력(scratch, CEV-C01에 print 추가) | scratch | `statement_held=48 copies=368 held_before=1044 held=1412 product=2042 lookup_load=31086` |
| `cargo fmt -p xadb-engine --check` | HEAD | 깨끗 |
| `python3 devdoc/verify_documents.py` | scratch | 돌리지 못함(`markdown_it` 모듈 없음) |
| `python3 scripts/check_module_gate.py check --base origin/claude/project-thread-y6amas --head HEAD` | HEAD | FAIL 14건: scoped CI evidence 없음 6(`catalog/read.rs`, `catalog_eviction.rs`, `auto_split.rs`, `read/catalog.rs`, `read/lookup.rs`, `read/operation.rs`), review 없음 7(`catalog/read.rs`·`read/operation.rs`는 D2 common, 나머지 5개는 D1), CAT native attestation 없음 1(`scripts/dev3_catalog_native.py` inputs `22c1bd46d852`). NOTE: 두 파일이 disk re-export 대상이라 D2 common |

## 판단 요약

- 결함 재현과 수정은 맞다. revert하면 CEV-J01/J02/J03(뒤쪽)과 SPL-A05가 정확히 보고된 `Frame(MissingTarget)` Unchanged로 깨지고,
  수정 뒤 통과한다.
- 코드: `load_catalog`의 보호 집합(각 LOAD가 다른 page를 지킴)은 맞고, B의 prefix 보호(`&requests[..at]`)보다 엄격하지만
  둘 다 옳다. resident page는 registry만 보고 LOAD claim·replay scope·content admission을 만들지 않는다. FlushRequired는 claim
  전 반환이라 효과가 없다(J02가 files·pins·registration·source·reference를 확인). error는 `finish_error`→call `finish_error`,
  FlushRequired는 `finish_flush`→`finish_checked`로 기존 Status FlushRequired와 같은 길이다. resident class는 `check_statement`
  한 번이 더해졌을 뿐 Ready로 빠지고 capture miss는 `selected_error`의 Busy다. `load_lookup_target` 재사용은 Catalog page에도
  성립한다(`accepts_retention_payload` 7=TreeAnchor/owner 5, 8 포함, level 0, `target_membership`, 6-page scratch prefix는
  Catalog copy와 분리).
- census: 모형 수치는 맞다(아래 F1 계산). Full 34,588과 Pending 옆 창 65,486 ≤ 65,536은 base와 같다. 다만 이를 지키는 시험이 없다.
- 설계: LOAD를 고른 것에 동의한다. 이미 B의 Row read가 같은 page를 같은 LOAD 본문으로 올리고, writer도 `prepare_catalog`로
  LOAD한다. pin은 frame 수·trace를 바꾸고 open 직후 non-resident는 못 푼다.
- 남은 것: 핵심 성질(보호 집합)과 자원 주장의 증거가 비어 있다(F1, F2).

## Findings

### F1 (필수) CEV-C01은 실패할 수 없고, 계약의 자원 문장이 다른 층위의 수를 비교한다

- `read/lookup.rs:358`이 `catalog_load_source_bytes(resident)`를 max 안에 넣고 `363`에서 `statement::held_source_bytes()`(48 B)를
  더한다. 그래서 `full = max(…, load, …) + 48`이고, `lookup.rs:378`의 `assert!(load < full)`은 `load`가 어떤 값이어도 참이다.
  CEV-C01이 "below the full lookup charge, not its peak"를 보이지 못한다. 코드 어디에도 34,588을 고정한 단언이 없다(grep).
- 실제 수(보조 출력): Full의 peak는 Status/Undo LOAD 단계 `held 1,412 + product 2,042 + lookup_load 31,086 = 34,540`, +48 = 34,588이다.
  새 Catalog LOAD 단계는 `1,044 + 2,042 + 280 + 31,086 = 34,452`, 과금되는 값은 +48 = **34,500**, 여유 **88 B**다.
  `dev3-visibility-retirement.md:83`의 "34,452 B로 Full 조회 charge 34,588 B 아래"는 48 B를 뺀 값과 뺀 적 없는 값을 비교한다.
- 실패 시나리오: 이후 `operation.rs:740` `catalog_load_source_bytes`가 native 측정에 맞춰 커지거나(예: B의 `prepare_catalog`처럼
  allocator까지 `[TargetRequest; 3]`), frame에 carrier가 하나 더해져 89–130 B 늘면 Full이 조용히 34,589–34,630이 되고
  `pending_full`이 65,487–65,528로 ceiling 안에 남는다. 어떤 시험도 깨지지 않고 계약의 "charge·ceiling·Pending 옆 Full 창은
  바뀌지 않는다"만 거짓이 된다. CEV-C01은 그보다 큰 증가에도 계속 통과한다.
- 요구: CEV-C01을 fixture 사실에 대어 단언한다. 예: base `0197f199`에서 잰 `full == 34_588`과 `load + statement::held_source_bytes()
  < 34_588`(여유 출력), 또는 LOAD 항을 뺀 max를 따로 노출해 `load ≤ peak_without_catalog_load`. 계약 83행의 수를 34,500 B(여유
  88 B, peak는 Status/Undo LOAD 단계)로 고친다.

### F2 (필수) 보호 집합이 시험되지 않는다: M2가 CEV 전부에서 살아남는다

- `operation.rs:285` `&requests[1 - at..2 - at]`를 `&[]`로 바꿔도 CEV-J01·J02·J03이 모두 통과한다. J02의 `victim != LEAF`
  (`catalog_eviction.rs:201`)는 scan(cursor 1부터)이 leaf보다 앞선 dirty frame을 먼저 고르므로 보호 여부와 무관하게 참이다.
  docstring(148–150행)의 "never the dirty leaf (the anchor's LOAD keeps the leaf)"는 이 시험이 보이지 않는다.
- 보호가 없으면 보고된 결함이 그대로 남는 상태가 있다. probe(scratch 전용): 12-frame, CREATE q1 뒤 anchor·leaf를 둘 다 내보내고,
  clean frame 가운데 첫 하나(frame 5)만 남기고 나머지를 `with_pins`로 잡은 채 `finish_table`.
  anchor LOAD는 유일한 clean frame 5에 들어간다. leaf LOAD의 clean 후보는 방금 올린 frame 5뿐이다.
  - HEAD: frame 5가 보호되어 `TableStep::FlushRequired`(page 9). anchor는 frame 5에 남고 files 불변. flush 뒤 재시도가 T를 발급한다.
  - M2: leaf가 frame 5의 anchor를 밀어내고 capture가 `Frame(MissingTarget)` Unchanged로 거절된다(보고된 결함과 같은 결과).
- 요구: 위와 같은 판별 사례를 CEV에 넣는다(anchor를 올린 frame이 leaf LOAD의 유일한 clean 후보인 배치). 가능하면 anchor LOAD가
  resident leaf를 지키는 쪽(at=0)도 같은 방식으로 판별한다. UNL-L09에서 같은 종류의 보호 누락이 D2 필수였고 mutation으로
  확인한 선례(`dev3-tree-smo.md:1929`)와 같은 수준의 증거를 요구한다.

### R1 (권고) SPL-A05가 전제를 찍기만 하고 단언하지 않는다

- `auto_split.rs:655`의 `catalog`는 `664`의 println에만 쓰인다. 이번 실행은 `[false, true]`(anchor 비상주)였고, 계약
  `dev3-tree-smo.md:1103`의 "split 뒤 Catalog anchor가 pool을 떠나며"는 이 출력에만 기댄다. 뒤쪽 `resident_index(7/8).is_some()`은
  B의 `scan_big!`도 7·8을 LOAD하므로 C의 LOAD를 따로 보이지 않는다.
- 시나리오: 이후 pool 배치나 victim 순서가 바뀌어 split 뒤에도 7·8이 resident면, SPL-A05는 LOAD 경로를 지나지 않고도 통과하고
  계약 문장은 확인 없이 남는다. `assert_eq!(catalog, [false, true])`(fixture 사실)로 고정하기를 권한다.

### R2 (권고) probe→LOAD→capture 사이의 경합과 결과 목록이 계약에 없다

- `load_catalog`(259–293)는 page를 resident로 만든 뒤 pin 없이 끝나고, `capture_catalog`(303) `read_catalog`가 다시
  `reserve_target_set`한다. 그 사이 다른 Owner의 LOAD(다른 reader의 Status LOAD `lookup_resident`, 407·510행, 보호 집합 `&[]`,
  또는 Row preflight)가 clean anchor frame을 victim으로 고르면 capture는 여전히 `Frame(MissingTarget)` Unchanged다. 기존
  Status LOAD→`read`도 같은 창이 있으므로 새 결함은 아니지만, 이제 MissingTarget은 "작은 pool의 고정 거절"이 아니라 재시도로
  풀리는 일시 결과다. 또 clean·dirty 후보가 모두 없으면 `select_load`는 Load 단계 `Busy`(Unchanged)를 준다.
- 계약 80행은 FlushRequired만 적는다. Full 클래스의 가능한 결과(FlushRequired, Load `Busy`/`Stale`/`Capacity` Unchanged,
  경합 시 Census `MissingTarget` Unchanged → 같은 statement 재시도)를 적거나, Full 클래스에서 LOAD 뒤 capture의 MissingTarget을
  한 번 더 LOAD하고 다시 capture하도록 하기를 권한다.

### R3 (권고) "resident 조회는 전과 같다"를 보는 관측이 없다

- M4(probe 삭제, resident page에도 `load_lookup_target`)가 CEV와 관련 68개를 모두 통과한다. 코드상 probe는 resident 경로에서
  `CatalogLookupOp`·`replay_scope`·`try_content`·`select_load`(test point `LoadResidentObserved`)를 피하므로 의미는 있다.
  다만 그 차이를 보는 시험이 없어 계약 80행 "두 page가 resident인 조회는 전과 같이 실행된다"가 code 읽기에만 기댄다.
  resident lookup 전후 `test_replay_scopes()`(`history/runtime.rs:42`) 같은 관측이 그대로임을 단언하기를 권한다.

### R4 (권고) 빠진 사례

- Full `lookup()`(`CatalogStatementCall::lookup`/`CatalogCallScope::lookup`)이 eviction 뒤 첫 호출로 LOAD하는 경우와 그
  `CatalogLookupStep::FlushRequired`. 지금은 `finish_table`만 LOAD를 지나고 J01의 lookup은 이미 resident일 때 돈다.
- LOAD 뒤 Missing 대상(없는 object)의 `finish_missing`(catalog flag 등록).
- LOAD 오류 경로: 다른 Owner의 load claim이 걸린 동안(`select_load` Busy) Unchanged, 등록·source·reference 불변.
- Active(미commit) CREATE가 flush된 뒤 leaf가 밀려난 상태에서 다른 transaction의 조회: replay=home 검산(`resolve_lookup_claim`
  250–271, 불일치는 fatal)이 미commit image에서도 맞는지, 뒤따르는 Status LOAD가 같은 cursor로 7·8을 밀어내도 결과가 맞는지.

### R5 (권고) Begin의 남은 한계에 소유자가 없다

- 계약 84행은 Begin(`row_begin`, Root/Jobs/Status resident 전제)을 범위 밖으로 둔다. System page 1–6도 clean이면 같은 victim
  선택으로 밀려나므로, 같은 small-pool 흐름(SPL-A05 같은 split 뒤 새 Row transaction)이 이번에는 Begin에서 MissingTarget으로
  거절될 수 있다. "범위 밖"이 아니라 미배정 의무로(소유자·일정 또는 "미배정" 표기) 남기기를 권한다.

### N1 (NIT) 계약 §4 1단계가 옛 경로를 적는다

- `dev3-visibility-retirement.md:66`은 여전히 "원 `prepare_catalog`→`read_catalog` 경로로" 읽는다고 한다. reader의 Full 경로는
  이제 `load_catalog`→`read_catalog`다(`prepare_catalog`는 writer 전용). 새 절로 연결하면 된다.

### N2 (NIT) `read_catalog` 주석이 B의 caller를 빠뜨린다

- `read/catalog.rs:206–208`의 "(`prepare_catalog`, or the Full class's `load_catalog`)"에 B의 `plan4_apply/row/read.rs:446`
  `load_catalog`가 없다. `load_catalog`가 둘이라 이름만으로는 어느 것인지 모호하다.

### N3 (NIT) 같은 request 구성이 세 곳에 있다

- `read/catalog.rs:188` `catalog_requests`, B의 `row/read.rs:425–444` `catalog_request(s)`, test 전용 `plan4_retention/read.rs:294–321`
  `lookup_catalog_empty`. 하나로 모으면 identity·kind·level 규칙이 한 곳에 남는다. B는 prefix, C는 상대 page를 보호하는 차이도
  주석으로 남기면 좋다.

### N4 (NIT) 같은 이름의 census 함수 둘

- `operation.rs:740` `catalog_load_source_bytes()`(자기 frame 280 B)와 `lookup.rs:265` `catalog_load_source_bytes::<W>(bool)`(단계 전체
  34,452 B). 하나를 `catalog_load_frame_source_bytes` 같은 이름으로 바꾸면 F1 같은 혼동이 줄어든다.

### N5 (NIT) UNL-L09 문단의 측정값은 수정 전 값이다

- `dev3-tree-smo.md:1897–1901`의 "table 발급(28 frame)이 MissingTarget으로 먼저 거절"과 그래서 A의 drain을 쓰지 않는다는 근거는
  수정 전 측정이다. 괄호 문장이 더해졌지만 25–28 frame에서 A의 drain으로 L09 상태를 만들 수 있는지는 다시 재지 않았다.
  "수정 전 측정"이라고 적거나 다시 잰다.

### N6 (NIT) 근거 없는 예

- 계약 78행의 "작은 pool에서 08-3 overflow 행을 쓸 때"는 이 tree에 대응 시험·기록이 없다. 출처를 달거나 SPL-A05만 남긴다.

## 확인한 계약 문장

| 문장 | 결과 |
| --- | --- |
| 고치기 전 page 7/8이 떠나면 Full 조회·table issue가 `MissingTarget` Unchanged | revert로 확인(J01·J02·J03 뒤쪽·SPL-A05) |
| resident 클래스는 LOAD하지 않고 Busy | J03 앞쪽, code(`load_catalog` 조기 Ready, `capture_catalog`의 `selected_error`) |
| 한 page를 올리는 동안 다른 page는 보호 | code는 맞다. 시험 증거 없음(F2) |
| FlushRequired는 효과 없음, 같은 statement 재시도 | J02와 code로 확인 |
| 후보 복사본 368 B, frame 280 B, 34,452 B | 수치는 맞다. 비교 대상은 F1 |
| charge·ceiling·Pending 옆 Full 창 불변 | base와 HEAD census 출력이 같음(65,486 ≤ 65,536). 이를 지키는 단언은 없음(F1) |
| SPL-A05 split 뒤 anchor가 pool을 떠남 | 이번 실행 출력 `[false, true]`. 단언 없음(R1) |

## 판정

CONDITIONAL. F1과 F2를 고치면(시험 둘과 계약 83행) ACCEPT할 수 있다. 제품 코드의 수정 방향과 동작에는 필수 지적이 없다.
module gate의 record·review·scoped CI evidence·CAT native attestation은 별도로 채워야 한다.

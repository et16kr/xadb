Verdict: ACCEPT

# D2 독립 검토 3회차 — 빈 leaf unlink·root 축소·retirement 등록 설계

- **대상:** `d3d7a03`(`64bc5bd` 위 수정 commit). 이번 수정분은 `git diff 64bc5bd d3d7a03`(dev3-tree-smo.md 17줄, document-validation.json 해시)이고, 전체 변경은 `git diff 1c9486a d3d7a03`이다.
- **판정:** 2회차의 열린 필수 N1이 해소됐다. 선택 N2·N3도 반영됐다. 열린 필수는 0이다. 이 판정은 설계 문서 범위에 한한다. U1–U4 각 단계의 구현·시험·census는 계획대로 단계마다 따로 D2를 받아야 한다.

## 검토 방법

- `git diff 64bc5bd d3d7a03`의 바뀐 줄 전부를 2회차 지적과 대조했다.
- 새 서술이 기대는 코드 동작을 다시 확인했다(읽기만).
  - read gate의 대기와 Busy 조건: `transaction/concurrency/read.rs:144–145·177–212`
  - test point `ReadGateWaiting`: `read.rs:191–199`, cfg(test)
  - `apply_existing_n` 안의 순서: `access.write`, `matches_stamp`의 `Stale`, 그리고 stage 전 실패는 pending이 없어 Unchanged로 남는다는 점(`catalog_lifecycle.rs:612–624·704–711`, `dirty_frame.rs:1121–1123`, `plan4_apply.rs:435–441`)
- `/home/claude/xadb`는 바꾸지 않았다. 설계 검토라서 build·시험은 돌리지 않았다.

## 2회차 항목 상태

### N1 (R2 잔여) — 해소

- **read gate 서술(1286–1290행).** 이제 코드와 맞다.
  - shell은 다른 Owner의 열린 읽기가 끝나기를 기다린 뒤 gate를 닫는다(`wait_admitted_reads`).
  - `Concurrency(Busy)`는 이 Owner 자신의 읽기가 열려 있거나 다른 gate가 이미 있을 때만 나온다. 이는 `read.rs:144–145`의 `slots[self.index].read_operation || read_gate.is_some()`와 같다.
- **stamp 서술.** 준비와 설치 사이에 stamp가 바뀌면 `matches_stamp`의 `Frame(Stale)` Unchanged라고 고쳤다.
  - `matches_stamp`와 `access.write`는 `stage_verified_required`보다 앞에서 실패한다.
  - 그 시점에는 pending이 없으므로 `catalog_lifecycle.rs:708`이 fatal로 바꾸지 않는다. 따라서 Unchanged라는 서술이 맞다.
- **drain의 Busy 계열(1327행).** `Concurrency(Busy)`, `Frame`의 Stale·Busy, `Visibility(Busy)`로 고쳐, 고른 설치 경로가 실제로 내는 오류를 포함한다.
- **UNL-L10 대체(1361행).** 두 사례 모두 만들 수 있는 경계다.
  - 다른 Owner의 열린 읽기를 `ReadGateWaiting`으로 관측해 기다린 뒤, 읽기가 끝나면 unlink한다.
  - 준비와 설치 사이에 stamp를 바꾸는 seam을 넣어 `Frame(Stale)` Unchanged가 note를 남기는지 본다.

### N2 — 해소

opcode 3의 P를 RowInternal 1로 좁혔다(1223행). 기존 대응 `core.level()+1`과 같다.

### N3 — 해소

`retired`의 근거로 `plan4_recovery/analysis.rs`(관측)와 `history/catalog.rs`(arena·claim 정리)를 함께 적었다(1297행).

## 1회차 항목

- R1·R2·R3과 O1–O10은 2회차에 해소된 상태 그대로다.
- 이번 수정은 그 부분을 바꾸지 않았다. 바뀐 곳은 상태 줄, role 표의 한 칸, 거절 문단, Busy 계열, 복구 근거 문구, UNL-L09..L14 행뿐이다.

## 선택 (새 항목, 수용을 막지 않음)

- **S1.** UNL-L09..L14는 ID가 6개인데 사례가 7개가 됐다. 나열된 사례는 FlushRequired, 읽기 대기, stamp seam, level-2, 가장 오른쪽 leaf, 오래된 snapshot, Reserved 거절이다. L09..L15로 늘리거나 사례를 묶는다.
- **S2.** 1297행이 원문의 줄 바꿈 폭을 넘는다. 이는 형식 문제일 뿐이고, document-validation은 PASS로 갱신돼 있다.

## 수용 범위와 남은 의무

- **수용한 것:** 설계 내용 전체다. 전이, XTR1 목록, opcode 3·4 wire, 준비·설치·복구, Base4와 split candidate의 closure, 호출자, F4 경계, 시험 계획, U1–U4 단계가 여기에 든다.
- **이 수용이 아닌 것:** 구현과 그 증거다. 시험은 모두 NOT_RUN이다. 08-1 적립은 계약대로 U4 뒤에만 한다.
- **실측으로 넘긴 조건:** 다음 두 가지는 설계가 U3 census에서 실측하기로 한 조건이다. 맞지 않으면 상한을 올리지 않고 계약을 다시 검토한다.
  - C 본문 peak + 4,096 ≤ 44,304
  - A drain census가 4,096 안에 듦(현재 3,979)


# 2회차 (64bc5bd, 3회차로 대체됨)


# D2 독립 검토 2회차 — 빈 leaf unlink·root 축소·retirement 등록 설계

- 대상: `64bc5bd`(1회차 `bfae4f7` 위 수정 commit). 전체 변경은 `git diff 1c9486a 64bc5bd`, 이번 수정분은 `git diff bfae4f7 64bc5bd`다.
- 실질: `devdoc/contracts/dev3-tree-smo.md` `#empty-leaf-unlink`(1122–1378행). `document-validation.json`은 해시만 바뀌었다.
- 요약:
  - R1·R3과 선택 O1–O10은 해소됐다.
  - R2는 설치 경로·control part·준비 검산이 해소됐다.
  - 남은 것은 R2의 하위 항목 하나다. 새로 적은 "Busy 원천" 서술(1286–1289행)이 코드와 다르고, drain의 Busy 분류(1326행)와 시험 UNL-L10(1360행)이 그 서술에 기대고 있다. 설계 방향은 바꿀 필요가 없고 문서와 시험만 고치면 된다.
- 아래 행 번호는 모두 `64bc5bd`의 `dev3-tree-smo.md` 기준이다.

## 검토 방법

- 수정된 절 전체를 1회차 지적과 하나씩 대조했다.
- 새로 나온 주장은 코드로 확인했다(읽기만).
  - 설치 경로: `wal_io/plan4_retention/catalog_lifecycle.rs`의 `apply_existing`·`apply_existing_n`·`ExistingGroup`
  - stamp 대조: `wal_io/plan4_retention/catalog_targets.rs:89–94`, `dirty_frame.rs:1112–1129`
  - 오류 변환: `wal_io/plan4_apply.rs:435–441`
  - read gate: `transaction/concurrency/read.rs:126–213`
  - split의 Busy: `structural_effect.rs:629`
  - 복구의 `retired` 처리: `history/catalog.rs:690–694`, `plan4_recovery/analysis.rs:540–556`
  - wire 검증: `semantic_row/structure.rs:417–486`
- frame 표의 group 152(redo bytes) 값을 다시 계산했다.
  - opcode 3: 35,248 − (2,144+384) − 32 = 32,688
  - opcode 4: 19,032 − (2,144+512) − 32 = 16,344
- `/home/claude/xadb`는 바꾸지 않았다. 설계 검토라서 build·시험은 돌리지 않았다.

## 1회차 필수 상태

### R1 — 해소

- 대체·확장 목록에 split 공급 변경과 F05 재검토 조건이 들어갔다(1140–1141행).
- 준비가 candidate `audit`를 split과 공유한다고 적었다(1260–1261행).
- Base4 closure 문단(1304–1306행)이 다음을 적는다: split candidate `audit`(`build`·`verify_after`)도 `reached ⊎ retired == owned`와 같은 항목 검산을 쓴다, `verify_after`는 after anchor 목록을 before와 bytes로 비교한다, 빠뜨리면 첫 unlink 뒤 모든 split이 거절된다.
- U1 목록(1372행), UNL-P09..P10(1354행), 바뀐 코드의 기존 시험 선택(1364–1366행)이 추가됐다.

### R2 — 대부분 해소, 하위 1건 미해소(아래 N1)

해소된 부분:
- 설치 경로를 하나로 고정했다(1278–1284행).
  - `ExistingGroup::Structural` + `apply_existing_n::<3|4>`, Structural INIT claim 없음.
  - after는 opcode 3의 anchor·P, opcode 4의 anchor뿐이고 나머지는 stamp만 대조한다.
  - control part는 bank34 descriptor·HW·STABLE과 XOI successor, staged 규칙은 split과 같다.
  - durable 뒤 replay를 하지 않는 이유를 적었다.
- 준비 4항에 `check_progress_room`, `inventory_after`, 바뀌는 page마다 counter+1을 넣었다(1267–1269행).
- 코드와 맞는다.
  - `apply_existing_n`은 N 2..9를 받는다(`catalog_lifecycle.rs:550–563`).
  - after가 없는 target은 stamp만 대조한다(623–625행).
  - stage 뒤에는 pending이 있으면 fatal로 바꾼다(708행).
  - 새 physical 주소가 없으므로 split의 `StructuralIoLease`가 필요 없다는 것도 맞다.

미해소: 1286–1289행의 Busy 원천 서술(N1).

### R3 — 해소

- opcode별 record_count, native 수·bytes(group 124/136), redo 수·bytes(132/152)를 표로 고정했다(1211–1214행). 값은 frame 길이와 맞는다.
- native role 표(1220–1228행): opcode 3은 {1, 4, 6}, opcode 4는 {1, 8, 6, 7}이다. role mask를 u16으로 넓혀 중복 없이 core 칸 대응이 하나로 정해진다. provenance role5/6과는 다른 이름공간이라고 적었다.
- redo ordinal(1230–1233행): 바뀐 native의 물리 순서로 0부터 dense, 바뀌지 않은 page는 0xFFFF다.
- 복구 관측(1295–1297행): 바뀐 page만 `changed`에 넣고 `retired`에는 넣지 않는다.

## 1회차 선택 상태

| ID | 상태 |
| --- | --- |
| O1 | 해소. TreeAnchor는 그대로 두고 목록은 별도 `anchor::retired(..) -> RetiredList<'_>`로 읽는다(1185–1189행). |
| O2 | 해소. cell 1은 Row anchor에서만 받고 Profile2/Base3는 목록 있는 anchor를 거절한다(1188–1189행). 항목 검산을 넣었다: 첫 file, 9..63, LSN ≤ HW, LSN > 그 page의 pageLSN(1180–1184행). |
| O3 | 형식은 반영됐다(1324–1330행): Busy 계열, 그 밖의 Unchanged는 note를 지우고 RetireStop 한 번, Fatal은 `must_resolve`, `unlink_held`. Busy 계열의 내용은 N1로 이어진다. |
| O4 | 해소(1159–1160·1179행). |
| O5 | 해소(1128–1141행). |
| O6 | 해소(1342–1346행). |
| O7 | 해소(1332–1340행). 별도 never-inline frame, try/continue 두 flush 지점, 비용을 적었다. |
| O8 | 해소. UNL-B07..B09, L11..L14, A02가 추가됐고 ID 수가 맞는다. 기존 시험 선택 규칙도 들어갔다. 단 L10은 N1의 영향을 받는다. |
| O9 | 해소(`table: ObjectHandle`, 1272–1274행). |
| O10 | 해소(1192–1193행). |

## 필수 (열린 항목)

### N1 (R2 잔여). Busy 원천 서술이 코드와 다르다

drain의 Busy 분류(1326행)와 UNL-L10(1360행)이 이 서술에 기대고 있다.

**근거 (a): 다른 Owner의 읽기는 거절되지 않고 기다려진다.**
- 1287–1288행은 "다른 Owner의 열린 읽기 연산이 있으면 그 gate가 `Concurrency(Busy)` Unchanged로 거절"한다고 적는다.
- 실제 `close_reads`(`concurrency/read.rs:144–145`)가 `Cause::Busy`를 내는 경우는 둘뿐이다.
  - 이 Owner 자신에게 읽기 연산이 열려 있을 때(`slots[self.index].read_operation`)
  - 이미 다른 gate가 있을 때(`read_gate.is_some()`)
- 다른 Owner의 admitted read는 `wait_admitted_reads`(177–212행)가 끝날 때까지 기다린다. 같은 문서의 automatic-split 절도 "모든 owner의 admitted read 대기"라고 적는다.
- 그래서 UNL-L10("다른 Owner의 열린 읽기가 read gate Busy로 note를 남김")은 적힌 대로 만들 수 없다. 단일 thread 시험이면 멈추고, 다중 thread면 Busy가 아니라 기다린 뒤 진행한다.

**근거 (b): 고른 설치 경로에서 stamp 변화는 `RetentionCause::Busy`가 아니다.**
- 1288–1289행은 "target stamp가 바뀌면 split과 같은 `RetentionCause::Busy` Unchanged"라고 적는다.
- `RetentionCause::Busy`는 split의 `install_structural`이 만드는 오류다(`structural_effect.rs:629`).
- 이번에 고른 `apply_existing_n`에서는 다음과 같다.
  - `matches_stamp` 불일치는 `DirtyError { cause: Stale, effect: Unchanged }`다(`dirty_frame.rs:1121–1123`).
  - target 경합은 `access.write`(`reserve_target_set`·`try_write`)의 dirty_frame 오류다.
  - 둘 다 `frame()`을 거쳐 `RetentionCause::Frame(..)`이 된다(`catalog_lifecycle.rs:612–615·622–624`, `plan4_apply.rs:435–441`).
- 1326행의 Busy 계열(`Concurrency(Busy)`·`RetentionCause::Busy`·`Visibility(Busy)`)에는 `Frame`이 없다. 실제 경합은 "그 밖의 Unchanged"로 분류되어 note가 지워지고 `RetireStop`이 된다.

**영향.** 데이터와 복구에는 영향이 없다. 다만 계약의 동시성 서술이 틀렸고, 시험 하나가 성립하지 않으며, drain 분류가 의도와 다르게 구현된다.

**고칠 것.**
- 1286–1289행을 실제 동작으로 고친다.
  - shell은 다른 Owner의 admitted read가 끝날 때까지 기다린다(그동안 새 읽기는 막힌다).
  - `Concurrency(Busy)`는 이 Owner에게 열린 읽기 연산이 있거나 gate가 이미 있을 때만 나온다.
  - 설치 단계의 stamp·target 경합은 `RetentionCause::Frame`의 Unchanged(`Cause::Stale` 등)다.
- 1326행의 Busy 계열에 대해 둘 중 하나를 정한다.
  - `Frame`의 Unchanged `Stale`/`Busy`를 Busy 계열에 넣는다.
  - 또는 unlink 본문이 `apply_existing` 전에 stamp를 다시 대조해 `RetentionCause::Busy`로 바꾼다고 적는다.
- UNL-L10을 만들 수 있는 경계로 바꾼다. 예:
  - (i) 준비와 설치 사이에 test seam으로 target stamp를 바꾸고, Busy 계열로 분류되어 note가 남는지 본다.
  - (ii) 같은 Owner에 읽기 연산이 열린 상태에서 C를 직접 불러 `Concurrency(Busy)` Unchanged를 본다.
  - (iii) 다른 Owner의 읽기가 열려 있으면 unlink가 그 읽기가 끝난 뒤 진행되는지 본다. 기존 test point `ReadGateWaiting`(`read.rs:196`)을 쓸 수 있다.

## 선택 (새 항목)

- **N2.** native role 표의 opcode 3 P가 "RowInternal 1..15"로 적혀 있다(1223행). L은 언제나 RowLeaf 0이므로 P는 정확히 level 1이다. 기존 대응도 `core.level()+1`로 고른다(`structure.rs:441`). 고정되는 wire 문법이니 "RowInternal 1(= L level+1)"로 좁힌다. canonical 재계산이 어차피 거절하므로 위험은 작다.
- **N3.** 1295–1296행은 `retired`가 arena와 claim을 지운다면서 근거로 `plan4_recovery/analysis.rs`만 든다. analysis(540–556행)가 지우는 것은 target record이고, arena·claim은 `history/catalog.rs:690–694`가 지운다. 두 곳을 함께 적는다.

## 건전하다고 본 것 (2회차에 새로 확인)

- **설치 경로.** 위 R2 근거대로 `apply_existing_n`의 동작(N 범위, stamp만 대조, stage 뒤 fatal)이 설계의 "stage 전 Unchanged, stage 뒤 fence"와 맞다.
- **durable 뒤 replay 생략.** 같은 경로의 기존 Row 그룹도 durable 뒤 replay를 하지 않는다. 다음 그룹 준비의 `validate_live`가 bank·Jobs·XOI를 대조한다. 비용 서술(준비 replay 1회와 fsync 1회, 1339행)과도 맞는다.
- **retire LSN 검산.** "retire LSN > 그 page의 pageLSN"은 성립한다. 퇴역시킨 그룹은 그 page를 바꾸지 않고, 그 뒤에도 쓰는 곳이 없으며, flush는 pageLSN을 바꾸지 않는다.
- **note 처리.** 그 밖의 Unchanged는 note를 지우고 한 번만 알리므로, 거절 하나가 이후 모든 drain을 막지 않는다.
- **1회차의 건전 항목.** 전이와 key 범위, Allocated 유지와 CREATE pair·role5/6 claim, split의 Free 선택, 대안 표, anchor writer, F4 산술, 단계별 D2는 이번 수정으로 바뀌지 않았다.


# 1회차 (bfae4f7, 2회차로 대체됨)


# D2 독립 검토 — 빈 leaf unlink·root 축소·retirement 등록 설계

- 대상: branch `claude/project-thread-e9itre`, commit `bfae4f7`, 기준 `origin/main` `1c9486a`
- 범위: `git diff 1c9486a bfae4f7`. 실질은 `devdoc/contracts/dev3-tree-smo.md`의 새 절 `#empty-leaf-unlink`(1122–1314행)이다. README·baseline은 상태 줄, `document-validation.json`은 해시 갱신만 있다.
- 판정 요약: 방향(빈 non-root leaf만 떼기, level-1 root 축소, page를 Allocated로 두고 anchor 목록으로 퇴역을 등록)은 요구와 기존 불변식에 맞다. 다만 구현 전에 고쳐야 할 설계 누락이 3건 있다. split candidate가 퇴역 page를 고아로 거절하는 문제, live 설치 경로가 실제 pool API로 성립하지 않는 문제, opcode 3·4 wire 문법이 덜 정해진 문제다.

## 검토 방법

- 요구 문서: task-06 §5.2(106–115행, 6항과 그다음 문단), 표의 41·165·176·196행(구조 준비·적용, unlink·overflow 교체 뒤 회수, 3.06.1.c, T06-06), task-14 12행(3.14.1→3.03.4→3.14.2), 139행(4항), `#live-row-retirement`·`#live-row-cleanup-caller`(237–290행).
- 같은 문서의 앞 절: 결정과 범위, System owner·회계, nonempty INIT, pool-lifetime, base-structural-claims, structural-bank, current-tree-closure, structural-effect, current-leaf-consumers, automatic-split.
- 형식 문서: dev3-tree-format.md `#structural-candidates`.
- 코드는 읽기만 했다. 과제에 적힌 파일과 함께 다음을 봤다.
  - `wal_io/plan4_retention/catalog_lifecycle.rs`(`apply_existing`)
  - `dirty_init/structural.rs`, `dirty_init/structural/live.rs`
  - `plan4_recovery_base/row.rs`, `catalog/graph/{ledger,relations}.rs`
  - `history/catalog.rs`, `plan4_recovery/analysis.rs`
  - `transaction/concurrency/read.rs`, `plan4_apply.rs`(`visibility_busy`)
  - B read census(`plan4_apply/row/read.rs`), `row_start/{retire,recovery}.rs`
- 확인한 것: `decode_anchor`/`encode_anchor`의 26개 non-test 줄(정의 2, 호출 24)과 `size_of::<TreeAnchor>()` census 지점을 전부 열거했다. frame 산술은 직접 다시 셌다.
- `/home/claude/xadb`는 바꾸지 않았다. 설계 검토라서 build·시험은 돌리지 않았다. 아래 수치는 문서와 source에서 읽은 값이며 측정값이 아니다.

## 필수 (REQUIRED)

### R1. split candidate의 closure가 퇴역 page를 고아로 거절한다 — 첫 unlink 뒤 모든 split이 막힌다

**근거**
- `semantic_row/structure/candidate.rs:271–323`의 `audit`는 table이 소유한 allocator page 가운데 anchor가 아닌 page가 모두 visited 집합에 있어야 한다고 본다: `same(visited[..count].contains(&(ordinal as u8)))?`(320행).
- 이 audit를 쓰는 곳:
  - `build`(433행)와 `verify_after`(717행)
  - `frame::produce`·`frame::observe`(frame.rs:276·307의 `candidate::prepare`). 이것을 다시 live split 준비(`structural_prepare.rs:189`)와 scope4 canonical replay(`history/structure.rs:162`)가 쓴다.
- 설계가 바꾸는 closure는 Base4 `plan4_recovery_base/row/current_tree.rs:214`(`reached != owned`) 하나다(1254행). U1 목록(1308행)의 split 쪽 항목도 "split anchor 보존" 하나뿐이다.
- 그대로 구현하면 unlink 뒤 첫 split은 canonical 준비에서 `IdentityMismatch`로 거절된다. A는 그 행을 `Stage::Split` 거절로 끝내므로, 이후 그 table의 찬 leaf INSERT·UPDATE는 계속 실패한다. UNL-L08·UNL-R03이 이를 잡겠지만 설계 자체에 변경 지점이 빠져 있다.
- 1126행 "앞 절들의 split 공급을 바꾸지 않는다"도 사실과 다르다. split candidate의 anchor encode(candidate.rs:616)와 audit가 바뀌고, split frame의 anchor after bytes가 목록을 싣는다.

**고칠 것**
- U1 변경 지점에 다음을 적는다.
  - candidate `audit`의 소유 검산을 `reached ⊎ retired == owned`로 바꾼다. `build`와 `verify_after` 두 호출이 모두 해당한다.
  - 항목마다 Base4 closure와 같은 검산을 한다: 같은 object·incarnation·kind·level의 Allocated, RowLeaf는 cell 0, RowInternal은 decode만 하고 child를 따라가지 않음, 도달 page와 alias 없음.
  - split은 목록을 바꾸지 않으므로 `verify_after`에서 after anchor의 목록이 before 목록과 byte 단위로 같아야 한다.
  - unlink candidate가 같은 audit를 공유한다고 적는다.
- 1126행 문구를 고친다.
- 시험: UNL-P/B에 "XTR1 목록이 있는 tree에서 Root/ChildSplit 준비 수락, 목록 밖 고아는 거절"을 넣는다. audit와 anchor encode가 바뀌므로 그 코드에 닿는 기존 split candidate·canonical 비교 시험도 선택 대상으로 적는다(AGENTS.md 5행 "새로 작성하거나 수정한 코드에 해당하는 테스트").

### R2. "split 순서에서 INIT만 뺀" live 설치는 현재 pool 기구로는 만들 수 없다 — 설치 경로를 하나로 정해야 한다

**근거**
- split의 claim·latch·설치·완료는 모두 `StructuralClaimKey`에 묶여 있다.
  - `SharedPool::prepare_structural`은 `count`가 1이나 2일 때만 받는다(`dirty_init/structural/live.rs:166`).
  - `StructuralClaimRecord`는 `init_count` 1|2(`dirty_init/structural.rs:25`)와 target mask 정확히 5개(93·107행)를 요구한다.
  - 그 key에서만 `claim_structural_targets`(live.rs:344) → `try_write`(485) → `install`(599) → `finish_structural`(791)이 이어진다.
- INIT이 없으면 key가 없고, key가 없으면 whole-target write continuation도 install permit도 없다. 그래서 1241–1243행의 "stage→install→`wait_durable`→canonical replay 확인→finish 순서 가운데 INIT을 뺀 부분"에 대응하는 API가 없다.
- INIT 없이 기존 page만 바꾸는 기존 설치 경로는 `apply_existing`/`apply_existing_n::<N>`(`catalog_lifecycle.rs:545–`)다.
  - N은 2..9이고, `group.after(at)`가 None인 target은 stamp만 대조한다(623–625행).
  - stage·page/metadata/DPT 설치·`history.installed`·`wait_durable`을 한다.
  - 그러나 `ExistingGroup` 변종만 받는다(745행, `ordinary()` 577행). Structural bank34·XOI 갱신(split의 `StructuralControlCommit::apply`, `structural_effect.rs:179–186`)도, durable 뒤 `history::validate_live`(`structural_effect.rs:556`)도 하지 않는다.
- 준비 목록(1230–1236행)에는 split admission이 하는 다음 두 검산이 없다.
  - WAL·수정 counter·RetentionRoot revision 여유: `check_progress_room`(`structural_prepare.rs:235`)
  - XOI successor: `inventory_after`(`structural_prepare.rs:267`). validate_live는 매 그룹 뒤 XOI cache를 complete LSN으로 다시 encode해 비교한다(history.rs의 `encode_row_current` 비교).
- 1245행 "그래도 pin이 보이면 `Visibility(Busy)` Unchanged다"를 내는 검사도 설계 어디에도 없다. Row 정리에서는 `begin_retirement`가 이 오류를 낸다. unlink 설계는 그 protocol을 쓰지 않는다.

**고칠 것**
- 설치 경로 하나를 정해 계약에 적는다. 권장안:
  - 새 `ExistingGroup` 변종(예: `Structural`)과 `apply_existing_n::<3|4>`를 쓴다.
  - after를 갖는 page: opcode 3은 anchor·P, opcode 4는 anchor만. L(그리고 opcode 4의 P·S)은 stamp만 대조한다.
  - control part는 bank34 descriptor·HW·STABLE과 XOI successor를 split과 같은 규칙으로 쓴다. mark_staged/check_staged도 같은 규칙을 따른다.
  - durable 뒤 `validate_live` 재생은 INIT 발급이 없으므로 필수가 아니다. 유지할지(전체 replay 1회 비용) 정해서 적는다.
- 대안: `StructuralClaimRecord`를 `init_count` 0·mask 3/4로 일반화한다면 다음을 적는다. 그 claim이 INIT 없는 상태에서 무엇을 막는지(ordinary gate의 대상), registry·identity 크기 census.
- 어느 쪽이든 준비 단계에 `check_progress_room`, 바뀌는 page의 counter+1, XOI successor 검산을 넣는다.
- 1245행 문장은 실제 검사 기구(어느 함수가 어떤 pin을 보는지)를 지정하거나 지운다.

### R3. opcode 3·4 wire 문법이 덜 정해졌다 (role 중복, redo ordinal, role과 core 칸의 대응)

**근거**
- 현재 `validate_records`의 규칙(`semantic_row/structure.rs`):
  - role을 bitmask로 중복 거절한다(417행). 끝에서 `roles != 31`을 요구한다(486행).
  - native의 redo ordinal이 native index와 같아야 한다(427행).
  - role마다 kind·level·page를 core 칸에서 고른다(437–442행).
- 설계와의 충돌:
  - 1201행 "opcode 4에서는 … P와 L이 모두 role 6"은 role 중복 거절 규칙과 충돌한다. role 6 하나가 core의 L(56..80)인지 P(104..128)인지는 kind/level로만 갈라야 한다.
  - 바뀌는 page의 redo ordinal 값이 없다(0xFFFF만 정했다). unchanged native가 물리 순서상 바뀌는 page 사이에 끼면 native index와 redo index가 달라진다.
  - opcode별 `envelope.record_count`(319행), group 124/132/136/152 칸의 값이 표로 고정돼 있지 않다.
- 이름 충돌: 같은 문서의 "role5/6"은 provenance claim의 역할(split right / new root)인데, 새 native role 6도 "퇴역"이다(1199행, 1252행 "role5/6"). 두 이름공간이 한 절에 섞인다.

**고칠 것**
- opcode별 native role 집합과 대응을 표로 고정한다.
  - opcode 3: {1 anchor→core 32, 4 P→core 104 RowInternal level 1, 6 L→core 56 RowLeaf 0}
  - opcode 4: {1 anchor, P(core 104), L(core 56), 7 S→core 80}
- P와 L에 서로 다른 role을 주기를 권장한다(예: 6 = 퇴역 leaf, 8 = 퇴역 internal). 그러면 bitmask 규칙을 그대로 쓸 수 있다. provenance role과의 혼동은 이름이나 문장으로 구분한다.
- redo ordinal: 바뀌는 page는 redo record의 0부터 dense index, 바뀌지 않는 page는 0xFFFF, redo record 순서는 바뀌는 native의 물리 순서로 고정한다.
- opcode별 수치를 표에 적는다.

  | opcode | record_count | native 수·bytes | redo 수·bytes |
  | --- | --- | --- | --- |
  | 3 | 2 | 3 · 384 | 2 · 32,688 |
  | 4 | 1 | 4 · 512 | 1 · 16,344 |

- 복구 관측(`RetentionFrameObservation`)에 대해 명시한다.
  - `changed`에는 바뀐 page만 넣는다.
  - 퇴역 page를 `retired`에 넣지 않는다. 그 칸은 Free 의미로 쓰여 arena와 claim을 지운다(`plan4_recovery/analysis.rs:540–556`, `history/catalog.rs:690–694`).
  - 이 점을 적지 않으면 CREATE pair·role5/6 claim이 지워져 아래 "건전" 항목의 전제가 깨진다.

## 선택 (OPTIONAL)

- **O1. TreeAnchor의 운반 형태와 census.**
  - `TreeAnchor`는 lifetime 없는 Copy 값이다(`anchor.rs:66–70`).
  - `size_of::<TreeAnchor>()`를 더하는 census가 여러 곳 있다: current_tree.rs:241, row_preflight.rs:1916·2205, plan4_apply/row/read.rs:1476, plan4_pages/resources.rs:127, epochs.rs:542, history/row.rs:1076, read/catalog.rs:337, plan4_genesis.rs:786, candidate/source.rs:102, row/graph/source.rs:89·191.
  - 1172행처럼 `TreeAnchor::retired()`로 목록을 빌려주려면 `TreeAnchor<'a>`(+16B 이상)가 되어 호출 지점 24곳과 위 census가 함께 바뀐다. 여유가 작은 census가 있다: B cursor.row_load 14B, C preflight 184B, split 722B.
  - 권장: TreeAnchor는 그대로 둔다. `decode_anchor`는 cell 1 문법만 검산한다. 목록은 별도 borrowed 함수(예: `anchor::retired(input, expected) -> RetiredList<'_>`)로 closure와 candidate만 읽는다. U1 설계에 이 결정과 census 영향을 적는다.
- **O2. XTR1을 받는 범위.**
  - `decode_anchor`는 profile·tree kind를 모른다. 그대로 넓히면 Profile2/Base3 경로와 Catalog anchor(page 7, `plan4_genesis.rs:133·510`)도 cell 1을 받는다. Row tree·Profile3가 아니면 cell 1을 Corrupt로 거절한다고 적는다.
  - 값싼 검산 추가 후보: 항목의 file = 첫 file, page_no 9..63, retire LSN ≤ Structural high-water, retire LSN > 그 page의 pageLSN.
- **O3. A drain의 Busy 분류.**
  - 기존 drain은 `visibility_busy()`(`plan4_apply.rs:356`, Unchanged `Visibility(Busy)`)만 Held로 본다.
  - unlink에서 나올 수 있는 Busy는 cause가 다르다: read gate의 `Concurrency(Busy)`(`concurrency/read.rs:145`), stamp 재검산의 `RetentionCause::Busy`(split의 `structural_effect.rs:629`와 같은 형태), LOAD 경합 Busy.
  - 어느 오류를 "note를 남기고 unlink를 멈춤"으로 볼지 정확히 정의한다. drain 문맥은 owner Idle·단일 writer이므로 UNL-L10의 "read gate Busy"가 실제로 생기는지도 확인한다.
  - Busy가 아닌 Unchanged 거절 때 note를 지울지 정한다. 남기면 원인이 사라질 때까지 정리가 끝난 drain도 매번 `RetireStop`으로 끝난다.
- **O4. 문구 정정.**
  - 1148–1149행 "높이 2 이상인 tree는 level 1 아래로 줄지 않는다"는 RootCollapse(높이 2→1)와 모순된다. "root level 2 이상(높이 3 이상)인 tree는 줄지 않는다"로 고친다.
  - 1168행의 8,090B는 tree cell 한도(dev3-tree-format.md F05)다. anchor page는 generic page이므로 page 여유만 본다(`page.rs:182–205`). 2,216B는 어느 쪽이든 들어간다.
- **O5. 대체 서술 목록 보강.** 1126–1130행은 두 곳만 대체한다고 적는다. 다음도 이 절이 바꾸거나 미룬다고 적는다.
  - base-structural-claims의 "CREATE pair의 현재 상호 존재 규칙은 … 실제 unlink/retirement 연결 때 따로 일반화한다", current-tree-closure의 "최초 leaf 자체의 retirement와 그때의 역사 pair 승계는 뒤의 retirement 단위에서". page를 Allocated로 남기므로 `validate_pairs`(`plan4_recovery_base/row.rs:294–318`)가 그대로 성립하고, 일반화는 3.03.4로 미룬다.
  - structural-bank의 "WAL은 Root84,744 또는 Child84,640B", F01의 "native5·redo5". 이 절이 확장한다.
  - F05 절의 재검토 조건("unlink/retirement debt 추가")에 해당한다.
- **O6. 사용자 영향 서술(1282행).** unlink는 page를 돌려주지 않으므로 용량을 늘리지 않는다. 오히려 RootCollapse 뒤 다시 자라면 RootSplit이 새 page 2개를 쓴다. 예: 큰 거래가 split한 뒤 rollback → collapse → 다시 삽입. 이런 부하에서는 unlink가 없을 때보다 첫 extent를 먼저 소진한다. "변화 없다"보다 이 점을 적는다.
- **O7. F4와 비용.**
  - 묶이는 곳은 C 본문만이 아니다. autocommit 실행기 호출자 term을 넣은 drain census가 이미 3,979/4,096B다(여유 117B, task-14 284행). `Retired`에 필드가 늘면 그 frame(`driver.rs:2720`의 `drain_live`)이 커진다.
  - split처럼 flush 지점을 try/continue 둘로 세는지 정한다(`cleanup.unlink_flush` 하나로 적혀 있다).
  - 비용: unlink마다 준비 replay 1회, durable 뒤 replay를 유지하면 1회 더, 그리고 fsync 1회다. drain당 최대 8회가 붙는다. F6-a 비용 서술에 적는다.
- **O8. 시험 보강.**
  - (a) level-2 tree(SPL-J07 fixture)에서 non-root P(separator ≥2) 아래 LeafUnlink. 지운 separator와 같은 key, 이웃 경계 key의 point/cursor 읽기(T06-06 "ancestor 경계").
  - (b) 가장 오른쪽 leaf를 unlink한 뒤 새 INSERT(`row_high+1`)의 route.
  - (c) CREATE leaf(pair[1])를 퇴역시키는 경우의 Base4 `validate_pairs`와 재시작.
  - (d) 오래된 snapshot이 DeletePrune을 Busy로 잡는 동안에는 unlink가 없고, snapshot이 끝난 뒤에 unlink(§5.2 6항).
  - (e) C 직접 호출에서 RootReserved·allocator Reserved 거절.
  - (f) unlink 뒤 base 재작성(BaseOnly, 다음 rebase)에서 목록과 claim 보존.
  - (g) bank33에 unlink descriptor가 있고 그보다 오래된 role5/6 claim이 있는 Base4(`catalog/graph/ledger.rs:135–176`).
  - (h) 9번째 note를 버리는 경우.
  - 기존 시험 선택: 1301행 "새 시험만 선택해 돌린다"를 AGENTS.md 5행에 맞춘다. U1에서 바뀌는 split candidate, anchor codec, Base4 closure, `decode_layout`, bank의 기존 시험을 선택 대상으로 적는다.
  - UNL-B01..B05는 ID가 5개인데 사례는 6개다.
- **O9. `unlink_leaf_live`의 `table` 형.** drain에는 statement view가 없어 `split_row_live`가 받는 `CheckedTable`을 발급할 수 없다. note의 ObjectHandle을 받아 같은 cut의 Catalog로 검산한다고 적는다.
- **O10. 퇴역 page의 회수 credit.** task-14 39·155행은 retirement에 "영속 owner와 credit"을 요구하고 형식은 미동결로 둔다. 이 설계의 Structural owner는 잔여 0이라 장래 Free 그룹의 WAL credit을 예약하지 않는다. 그 credit은 3.03.4 admission의 몫이라고 한 줄 적는다.

## 건전하다고 본 것

- **전이와 key 범위.**
  - RemoveChild는 i>0이면 separator i−1을 지우고 왼쪽 child가 범위를 잇는다. i=0이면 child 1을 leftmost로 올린다(`tree_page/structure.rs:220–239`).
  - separator가 1개면 Collapse이고 zero-separator internal을 만들지 않는다(229–232행).
  - non-root unary를 NotNeeded로 두는 것은 tree-format `#structural-candidates`(143–146행)와 task-06 115행에 맞다.
  - P는 언제나 level 1이고 S는 leaf다. leaf page에는 parent·sibling link가 없으므로 S bytes가 바뀌지 않는다는 설명이 맞다. 두 child가 모두 비면 빈 root leaf가 되는 것도 맞다.
- **동시 예약.** 전역 RootReserved·allocator Reserved 배제와 Forward의 pair[1] 규칙의 결합이 split과 같다. 그래서 Reserve와 Forward 사이에 unlink가 끼지 않는다. "빈 leaf이면 L로 route되는 Retained root가 없다"는 추론도 맞고, candidate audit의 역방향 검사(candidate.rs:360–383)가 다시 확인한다.
- **Allocated 유지.**
  - 3.14.1→3.03.4→3.14.2 순서와 task-14 4항(retirement 등록까지 공급)에 맞다.
  - page가 할당된 동안 다음이 그대로 성립한다: CREATE pair 상호 존재(`plan4_recovery_base/row.rs:294–318`), `relations.rs:444–470`의 provenance 검사(도달성을 요구하지 않음), `current_tree.rs:203–212`의 Structural claim 조건(revision은 단조 증가).
  - split의 Free 선택(candidate.rs:472–496)은 Free만 고르므로 퇴역 page를 쓰지 않는다.
  - recovery 정리는 pair[1] 대신 route한 leaf를 쓴다(`row_start/recovery.rs:392–396`).
  - epoch seed는 root만 다시 decode한다(`visibility/epochs.rs:451–480`).
- **대안 표.** 서술이 코드와 맞다: Structural 칸은 하나(`ledger.rs:265–277`), bank는 빈 inventory만 받음(`bank.rs:66–77`), 완료·잔여0일 때만 교체(System owner·회계 절).
- **frame 산술.**
  - opcode 3: 160+192+1,536+256+3×128+2×16,344+32 = 35,248
  - opcode 4: 160+192+1,536+256+4×128+16,344+32 = 19,032
  - 둘 다 split 크기와 겹치지 않으므로 `bank::validate`의 consumed_wal 대응(bank.rs:61–65)에 그대로 더해도 모호하지 않다.
  - retire LSN = group LSN은 stage 전에 알 수 있다. candidate는 `previous_lsn+1`을 미리 정하고(candidate.rs:417), Patch는 72..16352만 싣는다.
- **anchor writer.** 기존 Row anchor를 다시 쓰는 곳은 split candidate(candidate.rs:616) 하나다.
  - CREATE Forward(`semantic_catalog/images/catalog.rs:246`)와 Base의 INIT digest 재구성(`relations.rs:647`)은 초기 image라 목록이 원래 비어 있다.
  - genesis(`plan4_genesis.rs:133`)는 Catalog anchor다.
  - 나머지 decode 지점은 root만 쓰므로 목록을 무시해도 맞다.
- **read gate.** shell의 `close_reads_for_retirement`가 durable 대기까지 새 읽기를 막는다(`concurrency/read.rs:133–162`, `structural_effect.rs:262–317`). B는 매 operation마다 anchor에서 route하므로 퇴역 page에 닿지 않는다.
- **F4 산술.** 44,304 − 4,096 = 40,208B는 맞다. split 효과 단계 37,438B와 live 정리 본문 39,564B에 비추어 들어갈 가능성이 높다. 실측을 U3 census로 미루고, 넘치면 상한을 올리지 않고 계약을 다시 보는 방식은 task-14 270행 규칙과 같다.
- **기타.** 복구는 unlink를 시작하지 않는다. 단계마다 D2·PR 하나를 두고 U4 뒤에만 적립한다. README·baseline 상태 줄은 본문과 맞다.

<a id="pr55-followup"></a>
## PR #55 후속 설계·코드 검토와 신규 시험 — 2026-11-06

입력은 PR head `b6dc3a25dac26c45f55c42d83d22f533ccbd4552`, base `1c9486a8f91c62c9c7224b38c652858ea432dcb2`다.
사용자가 설계·방향·코드 검토, 새로운 시험과 문제 수정 commit을 요청했다. 담당 `/root`, 별도 D1/D2 검토자는
`/root/unlink_design_review`다. 위 1–3회차 기록은 당시 증거로 보존한다.

### 설계 판정과 수정

즉시 Free하지 않고 object가 XTR1으로 퇴역 page를 소유하며, 기존 RemoveChild와 root level 1→0 축소를 쓰는
방향은 현재 DISK·회수 계약에 맞는다. `reached ⊎ retired == owned`, 기존 birth claim 보존, INIT 없는 설치 경로는
필요한 변경이다. 다만 다음 두 계약 오류를 수정했다.

1. **retire LSN은 snapshot horizon이 아니다.** 기존 "그 LSN보다 오래된 reader"는 WAL 순번과 commit sequence q를
   혼동할 수 있다. 회수 정본 `dev3-task-14-reclamation.md` §5.1·5.6의 `H=min(Q, 등록 q들)` 및 현재 edge·pin·
   flush claim·DWB·generation 확인을 명시했다. XTR1은 퇴역 이력이며 Free 권한은 아니다. LSN 하나로 반환을
   결정하는 대안은 기존 회수 불변식에 어긋나므로 채택하지 않았다.
2. **TreeAnchor 크기 불변은 decoder 비용 불변이 아니다.** `row_preflight.rs`의 `anchor_decode`는 중첩 decode
   비용도 계산한다. XTR1의 borrowed view·Result·항목을 포함해 U1 codec/candidate/Base4, U2 replay, U3
   writer의 상주 note·caller frame을 각각 검증하도록 UNL-C01..C03을 추가했다. U3까지 산정을 미루거나 charge를
   자동 증액하지 않는다. fresh/recovered writer 둘을 포함한다.

이는 기존 제품 방향 안의 D2 구체화이며 unlink의 실제 구현·공간 반환·더 깊은 root 축소를 시작하지 않는다.
read gate, Retained/RootReserved, scope4 frame 산술·role·dense redo, Base4의 CREATE pair/role5·6 claim 소비자를
정적으로 대조했으며 추가 필수 수정은 발견하지 않았다. 8개 RAM note의 누락은 이미 허용된 hint 의미다.
그룹별 canonical replay·fsync와 extent 조기 소진 위험은 남고, 성능 개선을 입증한 것으로 주장하지 않는다.

### 새 시험의 실제 결과

수정한 파일은 `crates/engine/src/storage/tree_page/structure/tests.rs`다. 제품 코드는 변경하지 않았다.
이번 사용자의 추가 시험 요청에 따라 새 `pr55_*` 두 개만 실행하고 기존 2,359개는 실행하지 않았다.
기대 page는 기존 독립 literal serializer로 조립하고, 기대 route는 명시한 구간 표로 주며 제품 encoder/search로
정답을 만들지 않는다. serializer가 공유하는 digest는 checksum 구성용이고 route 정답과 분리된다.

- `pr55_unlink_below_level_two_root_preserves_all_key_ranges`: 두 parent 각각의 leftmost·중간·마지막 child 제거
  6조합. before audit 9 page → after 8 page, parent after bytes, ancestor 경계 100을 포함한 조합당 key 1..180
  (총 1,080회)의 route·기존 행 존재 여부와 퇴역 leaf를 load하지 않음을 확인했다.
- `pr55_root_collapse_does_not_follow_retired_parent_or_empty_leaf`: 양쪽 제거 × survivor의 빈/비지 않은 상태
  4조합. survivor 1 page만 audit·검색하고, key 경계와 `u64::MAX`, scratch 불변을 확인했다.

Rust 1.95.0에서 신규 **2/2 PASS**, 실패·skip 0. 명령과 원 출력 발췌는 [실행 로그](pr55-tests.log)에 있다.
시험 파일 Git blob `2197b9dd0bfe33d03bfb84bc2a4f9999aa45d00d`, SHA-256
`ca419e73a0a3e6625833a694d286f39934c9d42cfda6aaabb392d921622ca679`.
로그 SHA-256 `7442ea83e6475019c8c1f988842b5da6775368cd02aac439297b0481c8ece1c3`.
소스 기준은 입력 head와 이 시험 파일 blob의 조합이다. 테스트 target은 기록 뒤 삭제한다.

이 결과는 현재 structural primitive·audit·search의 결합 검증이다. XTR1·unlink candidate·WAL replay·live drain·
crash recovery는 아직 구현되지 않았으므로 UNL-P/F/B/R/L/A/C/N의 PASS가 아니며 모두 NOT_RUN을 유지한다.
08-1 일정 적립도 0 그대로다. 미구현 runtime census나 crash 안전성은 이 검토만으로 인증하지 않는다.

### 후속 독립 검토 결론

`/root/unlink_design_review`의 최종 판정은 **ACCEPT — D2 문서·D1 신규 시험, 미해결 필수 지적 0**이다.
계약 blob `b56bab546b9ce910721574647a30e2a555ed94c4`와 위 시험 blob, 로그·SHA-256 및 NOT_RUN 경계를 직접
대조했다. 검토자는 재실행하지 않았다. 담당 `/root`가 지적 두 건의 반영과 새 시험 결과를 확인했다.
향후 XTR1·호출 경로·note 저장 위치·charge가 바뀌면 해당 단계의 census와 독립 검토를 다시 수행한다.

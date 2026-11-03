# 명시 거래 실행기와 Begin·statement 시작 상한

2026-11-03

**상태: 구현·선택 검증 완료 · 독립 검토 ACCEPT(2회차) · PR에서 사용자 확인 대기.** 계약은 [명시 거래 실행기](../contracts/dev3-module-boundaries.md#explicit-transaction)와 [Begin·statement 시작 상한과 표 발급 잔여](../contracts/dev3-module-boundaries.md#begin-statement-ceilings)다. 기준 source는 main `65d473e`(PR #39 병합)이고, branch는 `claude/project-thread-9inq97`이다.

<a id="ext-why"></a>
## 1. 왜 이 단위인가

- [자동커밋 INSERT 실행기](autocommit-insert-20261102.md)는 INSERT 문 하나를 자기 거래로 끝냈다. SQL 세션은 BEGIN과 COMMIT 사이에 여러 문장을 실행하고, 문장 사이에 거래를 쥐어야 한다. 실패한 문장은 그 문장만 되돌리고 거래는 이어 가야 한다.
- 자동커밋 단위가 남긴 잔여 셋(Begin·statement 시작·표 발급 지점은 C census에 A 항이 없어 상한이 없다)은 SQL 연결 전에 닫아야 한다고 계약에 적었다.
- 사용자는 결정 카드에서 "명시 거래 + 상한 잔여 닫기"를 골랐다(카드: BEGIN 뒤 INSERT 여러 문장, 실패 문장만 되돌리고 COMMIT/ROLLBACK하는 실행기와 Begin·문장 시작·표 발급의 C 상한을 만든다. 형식 변경 없음). 다른 선택지는 SQL INSERT 연결 시작(DEV4 진입 조건 예외가 필요)과 DELETE 설계 문서였다.

<a id="ext-change"></a>
## 2. 바꾼 것

| 쪽 | 파일 | 내용 |
| --- | --- | --- |
| A | `row/insert/driver/autocommit.rs` | 단계(`refused`, `fence`, `statement_ended`, `end_failed_statement`, `commit_answer`, `abort_transaction`)를 결과 slot 형에 대해 일반화(trait `Ending`·`Committing`). `Autocommit`의 동작은 같다 |
| A | `row/insert/driver/explicit.rs` (새) | 결과 slot `Explicit`(`Idle`, `Begun`, `NoTransaction`, `Done`, `Failed`, `Committed`, `RolledBack`, `Aborted`, `Fenced`)과 두 trait 구현 |
| A | `row/insert/driver/live.rs` | 진입점 `begin_transaction_live`, `transaction_insert_live`, `commit_transaction_live`, `rollback_transaction_live`. census를 공통 loop `executor_sites`와 `autocommit_sites`·`explicit_sites`로 나눔. Begin·statement 시작을 상한 항목으로 옮김 |
| A | `driver.rs`, `driver/budget_tests.rs`, `driver/tests.rs` | 모듈 등록, census 검사 공통 함수와 `f4_a_explicit_budgets_{memory,native}`, Begin·statement 시작 항목, LXT-A01..A07 |
| C | `plan4_retention/row_start.rs` | Row lifecycle의 세 adapter가 inline되지 않는 frame으로 `Result<PreparedRowGroup>` 대신 입력 묶음 `RowLifecycleInput`을 돌려주고, shell이 공통 준비를 직접 부름(동작 같음). census 항 `row_lifecycle_carriers` |
| C | `plan4_retention/catalog_live.rs` | Row lifecycle 본문과 그 drift 사본이 `row_lifecycle_carriers`를 쓰고, adapter의 진입 packet 항을 준비 옆이 아니라 동작 단계 옆에 더함 |
| C | `wal_io/row_caller_budget.rs` (`cfg(test)`) | 새 상한 `BEGIN` 2,816 B, `STATEMENT_START` 3,584 B |
| C | `plan4_apply/source_tests.rs`, `plan4_apply/row/census_tests.rs` | C 대조 `f4_c_begin_statement_budget`, S2W-C03 기대값 42,089 → 40,881 B |
| JOIN | `row_join/retire/autocommit.rs` | 실제 C 위의 LXT-J01..J05 |

WAL 형식·frame·opcode·page 형식·오류 종류·disk API·C 입구는 바뀌지 않았다. 기존 상한 값도 바뀌지 않았다(새 상한 둘만 더했다). C 제품 코드의 변경은 Row lifecycle adapter의 반환 모양 하나다. 각 동작의 검사·선택·identity 발급·공통 준비의 순서, 인자, 오류, 효과는 같다.

<a id="ext-entries"></a>
## 3. 명시 거래 진입점

거래는 호출자의 holder(`&mut Option<RowTransaction>`)에 있고, 진입점은 결과를 호출자의 slot 하나(`Explicit`)에 쓴다. 단계는 자동커밋 실행기와 같다(statement는 하위 frame에서 실행하고 그 frame이 돌아온 뒤 끝낸다. C가 해소할 Failure 뒤에는 C를 더 부르지 않는다. rollback 도중에는 `FlushRequired`만 이어 간다).

- `begin_transaction_live`: holder가 비어 있어야 한다. C Begin의 Unchanged 거절은 `NoTransaction`, 그 밖은 `Fenced`.
- `transaction_insert_live`: holder의 거래를 빌린다. 성공은 `Done { rows }`. 실패한 statement는 자기 mark까지 rollback하고 finish한 뒤 `Failed`이며, 거래와 앞 statement는 그대로다. statement나 그 rollback이 fence되면 entry가 holder를 비운다(`Fenced`).
- `commit_transaction_live`: holder에서 거래를 꺼내 C `commit_row`. 거절은 자동커밋과 같이 전체 rollback 뒤 `Aborted`, Rejected는 `Aborted`, Fatal은 `Fenced`.
- `rollback_transaction_live`: holder에서 거래를 꺼내 전체 rollback과 `finish_aborted` 뒤 `RolledBack`.
- holder가 기대와 다르면 C를 부르지 않고 `NoTransaction`(`OrderMismatch`). Begin에서 차 있으면 그 거래는 holder에 그대로 남는다.
- `Fenced` 뒤 호출자는 C의 복구 전까지 진입점을 다시 부르지 않는다(빈 holder의 Begin은 C를 부르고, C가 fence로 거절한다).

정리는 자동커밋과 같이 진입점이 돌아온 뒤 호출자의 `retire_finished_live`다.

<a id="ext-census"></a>
## 4. 자원 — A 쪽

셈은 F4·자동커밋과 같다(source-bound inventory, stack 측정이 아님, 표준 slack 1,920 B 한 번, C 호출 결과 두 번, 경로 위 never-inlined frame의 모든 slot). 두 실행기의 site 계산은 공통 loop `executor_sites` 하나로 하며, 자동커밋의 모든 값은 이 단위 전과 같다(정렬한 census 출력이 같음을 확인했다). memory·native Writer 값이 같다.

명시 거래의 frame 항(B):

| frame | 항 | 크기 |
| --- | --- | ---: |
| 호출자 | LiveCall, Owner 둘, `Explicit` slot, `Plan4Target`, 참조 12개(하나는 holder) | 408 |
| `begin_transaction_live` | 참조 다섯, isolation, C Begin 결과(거래) | 209 |
| `transaction_insert_live` | 참조 여섯, 두 slice, 다시 빌린 LiveCall, 하위 frame의 답, 끝의 답 | 자동커밋 `statement_live` + 참조 하나 + `Option<usize>` |
| `commit_transaction_live` + `commit_answer` | 참조 넷과 행 수, 꺼낸 거래, C 답 / 참조 셋과 행 수 | 자동커밋 commit 단계와 같은 모양 |
| `rollback_transaction_live` | 참조 넷, 꺼낸 거래 | |

holder는 이 stack 밖(세션 저장소)에 있다고 둔다. holder를 stack에 두는 호출자는 `Option<RowTransaction>`(168 B, census가 출력)을 자기 항에 더해 다시 센다. 그러면 정리 drain의 여유 117 B를 넘으므로 그 배치는 새 계약이 필요하다.

| A 의무 | 자동커밋 몫 | 명시 몫 | 상한 | 최소 여유 |
| --- | ---: | ---: | ---: | ---: |
| Commit frame | 2,889 | 2,808 | 3,024 | 135 |
| statement end | 5,948 | 5,715 | 6,144 | 196 |
| rollback base caller | 1,377 | 1,272 | 1,536 | 159 |
| `rollback.next` | 6,361 | 6,256 | 6,528 | 167 |
| `rollback.clr_lane` | 12,185 | 12,080 | 12,416 | 231 |
| `rollback.finish` | 6,057 | 5,952 | 6,144 | 87 |
| 정리 drain 최대(`try_flush`) | 3,979 | 3,979 | 4,096 | 117 |
| Begin | 2,625 | 2,577 | **2,816** (새) | 191 |
| statement 시작 | 3,409 | 3,176 | **3,584** (새) | 175 |
| 표 발급 | 3,425 | 3,192 | 없음(C 잔여) | — |

statement 시작과 표 발급의 몫은 C 호출의 receiver를 참조로 센다(Begin·Commit과 같다). 자동커밋 단위의 기록(3,401·3,417 B)은 receiver를 빼고 세어 8 B 작다(검토 1차 지적).

명시 거래의 첫 배치는 호출자가 거래의 `Result`와 statement 끝·거래 끝의 답을 따로 쥐었다(752 B). 그때 `rollback.finish`(statement 경로)가 6,160 B, drain `try_flush`가 4,323 B로 상한을 넘었다. 상한을 올리지 않고 결과 slot 하나와 stack 밖 holder로 바꿨다. 지금 명시 거래의 모든 몫은 자동커밋보다 작거나 같다(실행기 frame이 없다).

<a id="ext-ceilings"></a>
## 5. 자원 — C 쪽 상한

C 시험 `f4_c_begin_statement_budget`의 출력(B):

| | memory | native-type |
| --- | ---: | ---: |
| Row lifecycle 본문(이전 42,089) | 40,881 | 40,881 |
| + `BEGIN` 2,816, charge 44,304 대비 여유 | 607 | 607 |
| Row writer 본문 `writable::source_bytes()` | 23,027 | 23,027 |
| retained + split port + 그 본문 + `BEGIN` / 세션 peak 대비 여유 | 44,675 / 19,341 | 44,683 / 19,341 |
| retained + split port + 그 본문 + `STATEMENT_START` / 여유 | 45,443 / 18,573 | 45,451 / 18,573 |
| 세션 peak | 64,016 | 64,024 |
| CatalogFull charge = lookup 본문 | 34,586 | 34,586 |
| short cap − shared − Pending 창 − Full | 52 | 44 |

**Begin.** C는 Row lifecycle 본문 ≤ CatalogProgressCharge만 보았고, Begin 아래 A frame의 항이 없었다. 원 본문 42,089 B에 A 몫을 더하면 charge를 362·410 B 넘었다. 상한 2,816 B(큰 몫 2,625 + 128을 128 단위로 올림)를 두려면 본문이 601 B 이상 줄어야 했다. 본문을 이루는 항을 재어 보니 shell의 carrier(4,808 B)가 `Result<PreparedRowGroup>`(1,264 B)를 셋(공통 준비, adapter, match 출력) 세고, adapter의 진입 packet(728 B)을 준비 옆에 더하고 있었다. adapter가 공통 준비를 부르고 그 결과를 shell로 전달했기 때문이다. adapter를 inline되지 않는 frame으로 두고 입력 묶음(`RowLifecycleInput`, 392 B)만 돌려주게 해, shell이 공통 준비를 직접 부른다. carrier는 입력 둘과 Prepared 둘이 되고(−480 B), 진입 packet은 동작 단계(start selection 10,441 + 728 B) 옆으로 옮겨 준비 단계에서 빠진다(−728 B). 1,208 B가 줄었다. Begin의 admission 밖 부분(그 앞의 진입·capsule 예약과 그 뒤의 binding)은 Row writer 본문 안에 있어 statement 시작과 같은 세션 창에 대조한다.

검토 1차에서 고친 것: 첫 공급은 입력 묶음을 하나만 셌고(첫 match에도 동작의 결과와 match 출력이 있다) 진입 packet을 그대로 두어 본문이 41,217 B였다. 입력을 둘로 세면 41,609 B로 상한과 합쳐 charge를 121 B 넘는다. adapter를 inline되지 않게 하고 진입 packet을 동작 단계로 옮겨 40,881 B가 됐다.

**statement 시작.** Row `begin_statement`는 short admission이 없다. 그 본문은 Row statement head(ledger·descriptor·Job 검산)를 포함하며, C 세션 census가 단일 operation 창의 후보로 세는 Row writer 본문 `writable::source_bytes()` 안에 있다. C는 그 본문과 상한의 합을 retained·split port 옆에서 세션 peak에 대조한다. peak는 바뀌지 않는다. 첫 공급은 읽기 전용 statement 본문(`transaction::source_bytes()`, 5,149 B)을 썼는데, Row statement head의 항이 거기 없어 검토 1차에서 고쳤다.

**표 발급.** CatalogFull의 charge가 lookup 본문과 같아 A frame의 자리가 없다. charge에 A 항을 넣으면 Pending Catalog Commit 옆 Full read의 여유(52·44 B)가 A 몫보다 훨씬 작아(slack 1,920 B만으로도 넘는다) Pending 옆 Full read가 Capacity로 거절된다. C의 shared·Pending 창·Full 본문 축소나 Full class 계약 재정의가 필요해 이 단위에서 닫지 않았다. 계약에 C 잔여로 남겼다. 사용자의 결정 카드는 세 지점 모두의 C 상한을 말했으므로 카드보다 좁은 공급이며, PR에서 확인을 받는다.

<a id="ext-tests"></a>
## 6. 시험과 결과

변경 코드에 닿는 시험만 했다(debug build, 클라우드). JOIN과 catalog_memory 시험은 catalog 식별 환경변수 셋과 `RUST_MIN_STACK` 32 MiB로 돌렸다.

| 시험 | 확인하는 것 | 결과 |
| --- | --- | --- |
| LXT-A01 | 두 statement가 모든 행을 적용하고 각자 끝남, 호출자의 행 수로 C commit | PASS |
| LXT-A02 | 적용한 행 뒤 실패한 statement를 자기 mark까지 rollback하고 끝냄(앞 statement의 행은 남음), 다음 statement와 commit | PASS |
| LXT-A03 | 두 statement 뒤 호출자 rollback: 전체 rollback이 두 statement의 행을 모두 되돌리고 `finish_aborted` | PASS |
| LXT-A04 | 실패한 statement 뒤 commit의 `RollbackRequired`: 서 있는 행의 전체 rollback 뒤 `Aborted` | PASS |
| LXT-A05 | statement 검사 Fatal은 `Fenced`이고 더 부르지 않음, CLR 적용 Busy는 `Fenced`와 cleanup 기록 | PASS |
| LXT-A06 | 시작 거절·표 미발급은 행 없는 `Failed`이고 거래는 commit으로 이어짐, 호출자 rollback의 거절은 그 거절 자체로 `Fenced` | PASS |
| LXT-A07 | slot 읽기: statement 실패는 `Failed`, commit 실패는 `Aborted`, `Aborted`·`RolledBack`만 거래를 끝냄, Failure는 한 번만 꺼냄 | PASS |
| LXT-J01 | 실제 C에서 두 statement(2행, 1행)와 commit(q 2), drain 한 번 16그룹(5×3+1), Row Job 없음, 새 statement가 3행을 읽음, 재시작 복구 READY q 2·CLR 0·같은 행 | PASS |
| LXT-J02 | 1행 statement, NULL로 실패한 statement(1행 적용 뒤 mark까지 rollback), 1행 statement, commit. drain 15그룹(5×2+4+1), 새 statement와 재시작 복구가 두 행만 읽음 | PASS |
| LXT-J03 | 2행 statement 뒤 호출자 rollback, drain 9그룹(4×2+1). 같은 Status의 새 명시 거래가 1행 commit(6그룹), 새 statement가 그 행만 읽음 | PASS |
| LXT-J04 | 차 있는 holder의 Begin은 C 호출 없이 `OrderMismatch`(거래 유지), 없는 표의 statement는 행 없는 `Failed`(Table `InvalidInput`), commit. 빈 holder의 statement·commit·rollback은 C 호출 없이 거절. drain 6그룹 | PASS |
| LXT-J05 | Root target의 Begin은 C가 Unchanged(Membership)로 거절해 `NoTransaction`이고 holder는 빈 채로 남음. Status page의 Begin 뒤 live control byte 하나를 뒤집으면(R1D-A5N과 같은 `ATT_ACTOR` 위치) statement 시작을 C가 CallerMustResolve로 거절해 `Fenced`(Statement), holder는 비고 적용한 행은 없음 | PASS |
| `f4_a_{autocommit,explicit}_budgets_{memory,native}` | 4절 표 전부 상한 이내(Begin·statement 시작 포함), rollback 지점 집합이 상한 표와 같음 | PASS |
| `f4_c_begin_statement_budget` | 5절 표의 세 합 | PASS |
| S2W-C03, `row_progress_reserves_the_same_catalog_progress_charge`, `live_row_retirement_fits_the_progress_charge`, `writable_capsule_and_wrappers_stay_within_original_limits`, `f4_c_*` | 줄어든 Row lifecycle 본문과 그 drift 사본, charge·peak 불변, 기존 C 대조 | PASS |
| catalog_memory `row_transactions`, `row_statements`, `row_commit`, `row_live`, `row_preflight`, `write_transactions` | Row lifecycle 세 동작(Begin, 빈 actor의 RollbackStart·AbortEnd)을 지나는 기존 시험 | 71 PASS, 1 ignored(`s3_n01_native_worker`, 원래 무시) |
| `storage::row::insert::driver::`, `row_join::retire::autocommit::` | 바뀐 단계 위의 LAC-A·LXT-A·LRC-A·census와 LAC-J·LXT-J | PASS |

<a id="ext-review"></a>
## 7. 독립 검토

별도 agent가 읽기만으로 두 번 검토했다(D2, `policy`·`common` 범위). 기록은 [`module-gate/evidence/explicit-transaction/review.md`](../../module-gate/evidence/explicit-transaction/review.md)다.

- 1회차(`c7fcdab`): **ACCEPT WITH REQUIRED CHANGES.** A 쪽과 C 재구성은 맞다고 확인했다(진입점마다 holder·slot 상태, 첫 Failure 유지, 자동커밋 동작 불변, C 세 동작의 검사·순서·target·Change·오류 전파 불변). 필수 두 가지: (1) shell의 첫 `match`도 둘째와 같은 모양이라 입력 묶음을 두 번 세야 하고, 그렇게 세면 본문 41,609 B로 BEGIN이 charge를 121 B 넘었다. (2) STATEMENT_START 검사가 읽기 전용 statement 본문(5,149 B)을 썼는데 Row `begin_statement`는 `row_statement_head`도 지난다. 권고: Begin의 비허가 부분도 세션 창과 대조, 실제 C에서 holder 비움 시험, 카드보다 좁은 공급과 C 제품 변경을 PR에서 묻기, gate 시험 선택 조정. NIT: statement 시작의 receiver 참조(+8 B), stack holder 168 B 대 drain 여유 117 B, `Fenced` 뒤 호출 금지, slot 재사용 시험 없음.
- 고침(`286b90c`): 세 동작을 `#[inline(never)]`로 두고 `caller` 항을 동작 단계로 옮겨 Row lifecycle 40,881 B(BEGIN 여유 607 B), statement 시작은 `writable::source_bytes()`(23,027 B)와 대조해 여유 18,573 B, `window(BEGIN)` 검사 추가(여유 19,341 B), LXT-J05 추가, 문서·PR에 카드보다 좁은 공급 기록, gate 선택 조정, NIT 반영.
- 2회차(`286b90c`와 문서): **ACCEPT.** 필수·권고는 모두 고쳐졌다. 세 동작이 준비 전에 돌아오므로 728 B `caller` 항은 준비·설치 아래에 없고, `start_*`가 동작 단계를 보수적으로 덮는다(42,089 − 1,208 = 40,881 B, 동작 단계 11,169 B < 준비 33,724 B, drift 사본도 같은 식). 남은 NIT 넷(Begin 결합은 허가 뒤, 증거 한 행의 "두 합", LXT-J05의 하드코딩 offset과 stage 미고정, 자동커밋 기록의 receiver 없는 값)은 마지막 commit에서 모두 반영했다. 검토자는 C가 찍는 값을 다시 돌리지 않았고(UNCONFIRMED), 그 값으로 만든 합은 모두 맞았다.

<a id="ext-limits"></a>
## 8. 남은 한계

- 표 발급 지점의 상한이 없다(C 잔여, 5절). SQL 연결 전에 C가 정한다.
- SQL·DEV4 engine 연결이 없다. 지금 진입점을 부르는 것은 JOIN 시험뿐이다. UPDATE·DELETE는 범위 밖이다.
- holder가 stack 밖에 있다는 가정은 census의 가정이다. SQL 세션이 holder를 어디에 둘지는 연결 단위에서 정하고, stack에 두면 다시 센다.
- 대기·재시도 정책은 자동커밋과 같이 없다. rollback 도중 거절은 fence로 간다.
- Begin의 C 여유가 607 B다. Row lifecycle 본문이 그만큼 늘면 계약을 다시 연다.
- commit 거절 뒤 rollback, CLR 적용 Busy, statement 검사 Fatal은 literal stand-in에서만 시험했다. 실제 C 위에서 이 거절을 일으키는 JOIN 사례는 없다. JOIN 매크로는 진입점마다 새 `Idle` slot을 쓰므로, census가 가정하는 slot 재사용은 읽어서만 확인했다(모든 경로가 `fence`·`take_first` 전에 slot을 쓴다).
- RollbackStart·AbortEnd의 준비 쪽 거절을 일으키는 기존 시험은 없다(동작 쪽 거절과 성공 경로만 있다).

<a id="ext-checks"></a>
## 9. 저장소 검사

제품 시험과 export는 `286b90c`(검토 2회차 대상)에서 했다(클라우드). 마지막 commit은 검토 NIT(LXT-J05의 `ATT_ACTOR`와 stage 단정, `row_transactions.rs`의 `ATT_ACTOR` 공개, `source_tests.rs` 주석), 문서와 기록만 바꾼다.

- 변경 코드 시험: 6절의 시험(catalog_memory `row_transactions`·`row_statements`·`row_commit`·`row_live`·`row_preflight`·`write_transactions`, `storage::row::insert::driver::`, `row_join::retire::autocommit::`, `f4_`, `s2w_c0`, `retention::source::tests::`, `writable_capsule_and_wrappers_stay_within_original_limits`)을 한 번에 돌려 161개 통과, 1개 무시(`s3_n01_native_worker`, 원래 무시). JOIN과 catalog_memory는 catalog 식별 환경변수와 `RUST_MIN_STACK` 32 MiB. NIT 뒤 LXT-J05, `row_live_replay_mismatch_is_fatal`(`ATT_ACTOR`를 쓰는 기존 시험), `f4_c_begin_statement_budget`를 다시 돌려 통과.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: 최종 tree에서 통과.
- 단독 export: `scripts/module_single_source.py`(정책 `origin/main` `65d473e`)로 만든 a(C+A)와 b(C+B)에서 `cargo check -p xadb-engine --lib --tests --all-features`가 경고 없이 통과. a에서 LXT-A01..A07, LAC-A01..A07·A09..A12, `f4_a_{autocommit,explicit,caller}_budgets_{memory,native}`, `mca00_d15_*` 26개 통과. b에서 `f4_c_begin_statement_budget`, S2W-C03, `row_progress_reserves_the_same_catalog_progress_charge` 3개 통과. b에는 A 파일과 JOIN 시험이 없다.
- `devdoc/verify_documents.py`: 통과.
- module gate: [`20261103-explicit-transaction.json`](../../module-gate/records/20261103-explicit-transaction.json). D2 `policy`·`common` 검토가 바뀐 Rust 파일 13개와 정책 문서를 최종 blob으로 덮는다. CI 선택은 A 실행기 시험과 census(head와 C+A), JOIN LXT-J01..J05·LAC-J01·J02(head), C lifecycle census와 Begin·statement 시작 대조(head와 C+B), 그리고 Row lifecycle 세 동작을 지나는 `row_transactions` 기존 시험 여섯(head)이다. `check_module_gate.py check --mode push`: 통과.

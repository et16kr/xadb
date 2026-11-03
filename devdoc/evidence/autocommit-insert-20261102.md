# 자동커밋 INSERT 실행기와 rollback 상한 재계약

2026-11-02

**상태: 구현·선택 검증 완료 · 독립 검토 세 차례(ACCEPT) · PR #39로 병합(main `65d473e`).** 계약은 [자동커밋 INSERT 실행기](../contracts/dev3-module-boundaries.md#autocommit-insert)다. 기준 source는 main `5f478d0`(PR #38 병합)이고, branch는 `claude/project-thread-9inq97`이다.

<a id="aci-why"></a>
## 1. 왜 이 단위인가

- [live 정리 호출자](live-row-cleanup-caller-20261102.md)까지 A statement driver에는 insert·commit·rollback·정리 단계가 따로 있었지만, 이것들을 Begin부터 거래 끝까지 이어 부르는 제품 실행기는 없었다. 시험만 단계를 하나씩 불렀다.
- 실행기를 설계하다 보니 A의 Commit frame 몫이 상한 `COMMIT` 3,024 B에 정확히 닿아 있고, C Commit window의 여유도 2·11·146 B뿐이었다. 사용자에게 결정 카드로 물었고, 사용자는 "C 커밋 여유 확보 후 실행기"를 골랐다(카드: C commit 경로의 중복 slot을 줄이고 COMMIT 상한을 새 계약으로 올린 뒤 실행기를 만든다. 여유가 모자라면 조사 기록만 남긴다).
- 실제로는 commit 쪽을 A에서 정리해 C Commit window와 COMMIT 상한을 바꾸지 않고 들어갔다(3절). 원 상한을 넘은 것은 rollback 쪽 셋이었고, 이것을 F4 규칙의 "양쪽의 새 계약"으로 다시 정했다. 상한 값은 census로 잰 뒤 정했다. C 쪽 여유가 남는다(바뀐 상한에서 최소 1,732 B, 전체 최소는 바뀌지 않은 `rollback.finish`의 994 B).
- 카드는 COMMIT 상한의 새 계약을 다뤘고 rollback 상한은 다루지 않았다. 그래서 rollback 상한 개정은 카드 밖의 정책 변경이며, 근거는 F4 규칙과 독립 D2 검토(6절)이고, PR에서 사용자의 확인을 받는다.

<a id="aci-change"></a>
## 2. 바꾼 것

| 쪽 | 파일 | 내용 |
| --- | --- | --- |
| A | `row/insert/driver/autocommit.rs` (새) | 결과 slot `Autocommit`(`Idle`, `Committed`, `NotBegun`, `Aborted`, `Fenced`)과 단계: `statement_ended`, `statement_refused`, `table_not_issued`, `end_failed_statement`(mark까지 rollback 뒤 `finish_statement`), `commit_answer`, `abort_transaction`(전체 rollback 뒤 `finish_aborted`), `refused`, `fence` |
| A | `row/insert/driver/live.rs` | 표 발급 trait `LiveTables`, 실행기 `autocommit_insert_live`, statement 단계 `statement_live`와 그 하위 frame `start_statement_live`, commit 단계 `commit_live_step`, `RowRollback`의 `flush`, census `autocommit_sites` |
| A | `row/insert/driver.rs` | `Rollback::flush`(정지한 rollback이 쥔 call의 flusher로 target flush), `Stage::Begin`·`Table`·`RollbackFlush`·`RollbackFinish`, `start_statement_rollback`·`start_full_rollback`·`rollback_step`을 `#[inline(never)]`로(동작 같음)와 그 census 주석 |
| A | `driver/tests.rs`, `driver/budget_tests.rs` | literal stand-in의 rollback flush와 한 번만 거절하는 fault, LAC-A01..A12(A08 없음), `f4_a_autocommit_budgets_{memory,native}` |
| C | `wal_io/row_caller_budget.rs` (`cfg(test)`) | `ROLLBACK_CALLER` 512 → 1,536, `rollback.next` 6,144 → 6,528, `rollback.clr_lane` 11,520 → 12,416 B와 그 근거 주석 |
| JOIN | `row_join/retire/autocommit.rs` (새), `row_join/retire.rs` | 실제 C 위의 LAC-J01·J02와 시험용 표 발급(`Tables`: Catalog 표를 읽어 `CheckedTable`을 낸다) |

C 제품 코드, WAL 형식·frame·opcode·page 형식·오류 종류, disk API는 바뀌지 않았다. 기존 insert·commit·rollback·정리 입구와 그 census도 그대로다(`Rollback::flush`는 기존 `flush_rollback_live`와 같은 continuation이다). rollback 시작·단계 세 함수의 `#[inline(never)]`는 census가 이미 가정하던 배치를 보장할 뿐 셈 값을 바꾸지 않는다.

<a id="aci-flow"></a>
## 3. 실행 순서와 frame 배치

실행기는 다음을 차례로 한다. 단계마다 inline되지 않는 frame이고, 결과는 호출자가 쥔 slot 하나에 쓴다. 그래서 한 단계의 slot이나 Failure가 뒤 단계의 C 호출 아래에 놓이지 않는다.

1. C `begin_row_transaction`. Unchanged 거절이면 `NotBegun`(거래 없음), 아니면 `Fenced`.
2. `statement_live`: call의 port와 Owner를 다시 빌린 `LiveCall`을 만들고(statement view가 거래를 빌린다), 하위 frame `start_statement_live`의 답을 자기 slot 하나에 받는다. 하위 frame은 C `begin_statement`, 호출자의 `LiveTables::issue`, 기존 driver `insert_statement`(tail)를 부른다. 시작 거절은 `statement_refused`(`After::Closed`), 표 미발급은 `table_not_issued`(`After::Returned`)가 같은 답 모양으로 돌려준다.
3. 하위 frame이 돌아온 뒤 `statement_ended`가 답을 읽는다. 실패면 `end_failed_statement`가 statement를 끝낸다(`RollbackRequired`면 mark까지 rollback한 뒤 `finish_statement`, `Returned`·`FinishPending`이면 `finish_statement`만). Failure를 C가 해소해야 하면(Fatal·CallerMustResolve) C를 더 부르지 않고 statement를 fence에 넘긴다. driver는 statement 검사의 CallerMustResolve도 `Returned`로 돌려주므로 이 검사가 필요하다.
4. 성공이면 `commit_live_step`이 C `commit_row`를 부르고 `commit_answer`가 답을 읽는다. 실패면 `abort_transaction`.
5. `abort_transaction`: `start_full_rollback`, 단계 반복(`FlushRequired`만 `Rollback::flush`로 이어 감), `finish_rollback`, `finish_aborted`.

거래의 수명은 port의 수명이라, 거래 끝 단계는 실행기의 call을 그대로 쓴다. `Committed`·`Aborted` 뒤의 정리는 실행기가 돌아온 뒤 호출자가 같은 call로 `retire_finished_live`를 부른다(drain이 complete가 아니면 다시. commit은 12행, abort는 15행을 넘으면 두 번 이상). `Aborted` 뒤에도 `slots`는 실패 전에 적용한 행을 가리키며 그 행은 되돌려졌다.

commit이 상한 안에 든 이유: commit 단계가 자기 frame에 C 답(`Result<Committed, TransactionFinishError>`) 하나만 쥐고, 실행기 상태(결과 slot)는 호출자에, 행과 slot 배열은 호출자의 stack 밖에 있다. 기존 `commit_live`가 쥐던 `DriveError` 결과와 TFE의 전달 slot이 이 경로에 없다. census는 2,889 B다.

<a id="aci-census"></a>
## 4. 자원

셈은 F4와 같다(source-bound inventory, stack 측정 아님, 표준 slack 1,920 B 한 번, C 호출 결과 두 번, 경로 위 never-inlined frame의 모든 slot, 자기 결과를 쓰는 tail 호출은 slot 없음). memory·native Writer 값이 같다. frame 항(B):

| frame | 항 | 크기 |
| --- | --- | ---: |
| 호출자 | LiveCall, Owner 둘, `Autocommit` slot, `Plan4Target`, 참조 12개 | 408 |
| `autocommit_insert_live` | 참조 다섯, isolation, 두 slice, Begin 결과(거래), statement 단계 결과 | 257 |
| `statement_live` | 참조 다섯, 두 slice, 다시 빌린 LiveCall, 하위 frame의 답(`Result<usize, DriveError<V>>` 344) | 448 |
| `start_statement_live` | 참조 넷, 두 slice, statement 시작 결과(view 168), 표 발급 결과(120) | 352 |
| `statement_ended`, `end_failed_statement` | 참조 넷씩 | 32, 32 |
| `commit_live_step`, `commit_answer` | 참조 셋과 행 수, C 답 / 참조 셋과 행 수 | 280, 32 |

| A 의무 | A 몫 | 상한 | 여유 |
| --- | ---: | ---: | ---: |
| Commit frame | 2,889 | 3,024 | 135 |
| statement end | 5,948 | 6,144 | 196 |
| rollback base caller | 1,377 (거래 경로 1,353) | 1,536 (원 512) | 159 |

insert 지점(실행기의 statement 경로, frame-static):

| 지점 | 몫 | 상한 |
| --- | ---: | ---: |
| `insert.check` | 4,820 | 11,520 |
| `insert.preflight` | 5,500 | 6,144 |
| `insert.try_flush` / `continue_flush` | 5,885 / 5,877 | 11,520 |
| `insert.validation_lane` | 7,588 | 11,520 |
| `insert.reserve` / `finish_preflight` | 7,140 / 6,916 | 11,520 |
| `insert.producer_lane` | 9,332 | 11,520 |
| `insert.forward` | 7,724 | 25,044 (charge − producer) |
| `insert.prepare` / `apply` / `reopen` / `cancel` | 7,260 / 8,540 / 8,388 / 8,900 | 11,520 |
| `insert.finish_statement` | 5,948 | 11,520 |

rollback 지점. statement 경로는 호출자 + 실행기 + `statement_live` + `statement_ended` + `end_failed_statement` 아래, 거래 경로는 호출자 + 실행기 + `commit_live_step` + `commit_answer` 아래다(statement 실패 뒤 거래 rollback은 statement 단계가 돌아온 뒤라 거래 경로보다 얕다).

| 지점 | statement 경로 | 거래 경로 | 상한 |
| --- | ---: | ---: | ---: |
| `rollback.start` | 5,602 | 5,578 | 6,144 |
| `rollback.next` | 6,361 | 6,337 | **6,528** (원 6,144) |
| `rollback.clr_lane` | 12,185 | 12,161 | **12,416** (원 11,520) |
| `rollback.clr` | 10,577 | 10,553 | 11,520 |
| `rollback.clr_prepare` | 11,017 | 10,993 | 11,520 |
| `rollback.clr_apply` | 11,089 | 11,065 | 11,520 |
| `rollback.clr_reopen` | 10,937 | 10,913 | 11,520 |
| `rollback.finish` | 6,057 | 6,033 | 6,144 |
| `rollback.finish_statement` | 5,089 | — | 6,144 |
| `transaction.finish_aborted` | — | 4,985 | 6,144 |

정리 drain(호출자 caller term 816 B = 실행기 호출자 + drain 결과 둘 + 참조 둘 + `Retired`): `cleanup.next` 3,184, `retire` 3,508, `try_flush` 3,979, `continue_flush` 3,971 B, 상한 `LIVE_ROW_CLEANUP` 4,096 B.

상한이 없는 지점(출력만): `begin_row_transaction` 2,625, `begin_statement` 3,401, 표 발급 3,417 B. C의 census에 이 입구들의 A 항이 없다. C가 상한을 정할 잔여로 계약에 남겼다(SQL 연결 전).

C 쪽 대조(`f4_c_rollback_budget_{memory,native}`, `f4_c_commit_budget_memory_and_native`, C census 코드는 바뀌지 않음):

| 상한 | C가 대조하는 합 | 원 여유 | 새 여유 |
| --- | --- | ---: | ---: |
| `ROLLBACK_CALLER` 1,536 | flush 단계 56,628 ≤ 65,536 (native) | 9,932 | 8,908 |
| 같은 값 | flush 뒤 다시 닫기 61,034 ≤ 65,536 (native) | 5,526 | 4,502 |
| `rollback.next` 6,528 | 선택 본문 36,044 + 상한 ≤ 44,304 | 2,116 | 1,732 |
| `rollback.clr_lane` 12,416 | lane 본문 27,297 + 상한 ≤ 44,304 | 5,487 | 4,591 |
| `rollback.finish` 6,144 (그대로) | control 본문 37,166 + 상한 ≤ 44,304 | 994 | 994 |
| `COMMIT` 3,024 (그대로) | Commit window, gate wait cap | 그대로 | 그대로 |

memory Writer는 shared가 20,208 B로 8 B 작아 flush 두 줄의 여유가 8 B씩 크다. 새 상한 값은 실행기의 statement 경로 몫에 128 B 이상을 더해 128 B 단위로 올렸다(1,377 → 1,536, 6,361 → 6,528, 12,185 → 12,416).

배치의 경과. 첫 배치(`7b9ed2b`)는 statement의 시작·표 발급·행 적용(`run_statement`)과 실패한 statement의 끝을 `statement_live` 아래에서 해서, statement view(168 B)와 표(120 B)의 slot과 `run_statement`의 frame이 rollback 아래에 남았다. 그때 몫은 caller 1,713, next 6,697, CLR lane 12,521, finish 6,393 B였고(실행기 참조를 6개로 센 값), 넷 다 원 상한을 넘어 1,920·6,912·12,672·6,528 B로 올렸다. 독립 검토 1차가 참조 셈을 바로잡았고(−8 B), 2차가 이 배치의 축소 여지를 지적해, 행 적용을 하위 frame으로 옮겼다. statement 경로가 328 B 줄었고 `rollback.finish`는 원 상한 안에 들며, 나머지 셋의 새 상한도 낮아졌다. insert 지점과 statement end는 8 B 줄었고, 상한이 없는 statement 시작·표 발급 지점은 하위 frame과 답 slot이 그 아래에 놓여 408 B 늘었다.

남은 셋을 A 쪽에서 더 줄일 수 없는 이유(계약 참조). 실행기 frame에 옮긴 slot은 commit 경로에도 놓이는데 commit 여유가 135 B다. commit 단계 두 frame(312 B)을 빼고 호출자·실행기·거래 rollback frame만 남겨도 rollback base caller는 1,041 B, `rollback.clr_lane`은 11,849 B로 원 상한을 넘는다. `rollback.next`는 거래 경로(6,337 B)에서 넘는다.

<a id="aci-tests"></a>
## 5. 시험과 결과

변경 코드에 닿는 시험만 했다(debug build, 클라우드).

| 시험 | 확인하는 것 | 결과 |
| --- | --- | --- |
| LAC-A01 | 모든 행 적용, statement 끝, C commit → `Committed { rows: 2 }` | PASS |
| LAC-A02 | 적용한 행 뒤 A가 거절한 행: statement를 mark까지 rollback하고 끝낸 뒤 거래 rollback과 `finish_aborted`. slot은 첫 Failure | PASS |
| LAC-A03 | 적용한 행이 없는 statement(`Returned`)는 rollback 없이 끝내고 거래는 rollback | PASS |
| LAC-A04 | commit의 `RollbackRequired`·Unchanged 거절 뒤 전체 rollback, 그 도중의 FlushRequired를 rollback의 flush로 이어 감 | PASS |
| LAC-A05 | Fatal commit은 `Fenced`이고 더 부르지 않음. Rejected는 `Aborted`이고 더 부르지 않음 | PASS |
| LAC-A06 | 끝으로 가는 도중의 거절(rollback next·task·flush·finish, `finish_statement`, `finish_aborted`)은 재시도 없이 `Fenced`, 첫 Failure 유지와 cleanup 기록 | PASS |
| LAC-A07 | 표 미발급(`Ok(None)`)은 statement 끝과 거래 rollback, Fatal 발급 거절은 아무것도 부르지 않고 `Fenced` | PASS |
| LAC-A09 | Begin 거절은 `NotBegun`/`Fenced`, statement 시작 거절은 행 없는 거래 rollback 뒤 `Aborted`, Fatal이면 `Fenced` | PASS |
| LAC-A10 | `FinishPending` statement는 한 번 더 끝냄. 두 번째 거절은 `Fenced` | PASS |
| LAC-A11 | statement 검사의 CallerMustResolve(driver는 `Returned`로 돌려줌) 뒤 C를 더 부르지 않고 `Fenced`. 호출은 검사 하나뿐 | PASS |
| LAC-A12 | `FinishPending` statement의 두 번째 finish가 받아들여지면 `Aborted`(첫 Failure)이고 거래 rollback이 적용한 행 7을 되돌림 | PASS |
| LAC-J01 | 실제 C 위에서 13행 거래 commit(q 2). drain 두 번 64·2그룹(5×13+1), 뒤에 Row Job 전부 Free. 새 statement가 13행을 읽음. 재시작 복구가 READY q 2, CLR 0, 남은 Row cell 0, 같은 13행 | PASS |
| LAC-J02 | NULL 행 거절 뒤 `Aborted`(Insert)와 drain 5그룹, 없는 표의 `InvalidInput` 뒤 `Aborted`와 1그룹, 한 행 commit 뒤 6그룹. 새 statement는 commit된 행만 읽음 | PASS |
| `f4_a_autocommit_budgets_{memory,native}` | 4절 A 표 전부 상한 이내, rollback 지점 집합이 상한 표와 같음 | PASS |
| `f4_a_caller_budgets_{memory,native}`, `mca00_d15_*` | 기존 A 상한(기존 호출자) | PASS |
| `f4_c_rollback_budget_{memory,native}`, `f4_c_commit_budget_memory_and_native` | 4절 C 표 | PASS |
| LRC-J01·J02 | 바뀐 driver(rollback 함수 배치) 위의 기존 정리 drain | PASS |

LAC-J01의 재시작은 C Progress 11단계로 READY에 닿는다(출력만, 정리할 Row Job은 없다). 첫 Row profile은 행마다 Free Undo page를 하나씩 쓰므로(후보가 없으면 preflight가 Capacity) J01은 setup에서 page 12개를 더 만든다.

<a id="aci-review"></a>
## 6. 독립 검토

별도 agent run이 읽기만 하고(cargo 없이) 세 차례 검토했다(D2, 정책·공통 범위). 기록은 [검토 기록](../../module-gate/evidence/autocommit-insert/review.md)에 있다.

- 1차(`7b9ed2b`): ACCEPT WITH REQUIRED CHANGES. 필수 3건은 (1) driver가 statement 검사의 CallerMustResolve를 `Returned`로 돌려줄 때 실행기가 fence된 session에 `finish_statement`를 부르던 것, (2) census가 가정한 별도 frame(`start_statement_rollback`·`start_full_rollback`·`rollback_step`)이 보장되지 않던 것, (3) 정책 문서의 기록이 부정확했던 것(카드는 COMMIT만 다뤘고 근거 없는 설계 추정치를 인용했다)이다. 권고 4건(A 쪽 축소를 고르지 않은 이유, 상한 없는 지점의 C 소유 잔여, 두 번째 finish가 성공하는 시험, 얇은 여유), NIT 3건(`debug_assert!`, 실행기 참조 6→5, drain·slot 문구)도 있었다. `44f684d`에서 모두 고쳤다(LAC-A11·A12 추가, 모든 몫 −8 B).
- 2차(`44f684d`): ACCEPT WITH REQUIRED CHANGES. 1차 10건은 해결됐다. 새 필수 1건: "A 쪽에서 줄일 수 없다"는 주장이 `rollback.finish`에는 틀렸다. statement view(168 B)와 표(120 B)의 slot은 시작·발급을 `statement_live` 자신의 frame에서 해서 rollback 아래에 남았을 뿐이다. `4d7d31a`에서 행 적용을 하위 frame으로 옮겨 statement 경로를 328 B 줄였고, `rollback.finish`는 원 상한으로 돌아가고 나머지 셋의 상한도 낮아졌다(1,920·6,912·12,672 → 1,536·6,528·12,416 B).
- 3차(`4d7d31a`): ACCEPT, 열린 필수 0. 하위 frame의 tail 호출, statement 시작 거절의 의미 보존, 셈(statement 448, 하위 352, rollback 아래 1,177 B), 반올림 규칙, C 합을 확인했다. D2 정책 변경은 받아들일 만하다고 판정했다: 세 상한 모두 어떤 A 배치로도 원 상한을 넘고, C의 charge·cap·식·slack은 그대로이며, 기록이 측정 뒤 정한 값임과 카드 밖임을 밝힌다. NIT 2건(지점 수 "일곱", README의 최소 여유 표현)과 미확인 1건(거래 handle 168 B > commit 여유 135 B)을 마지막 commit에서 문서에 반영했다.

<a id="aci-limits"></a>
## 7. 남은 한계

- SQL·DEV4 engine 연결이 없다. 지금 실행기를 부르는 것은 JOIN 시험뿐이다. 명시 거래(BEGIN … COMMIT)와 여러 statement, UPDATE·DELETE는 범위 밖이다.
- 대기·재시도 정책이 없다. rollback 도중의 Unchanged 거절도 fence로 간다(복구가 끝을 맺는다). Begin과 statement 시작의 Unchanged 거절은 handle을 남기지 않거나 거래를 rollback하므로 호출자가 다시 부를 수 있다.
- begin·statement 시작·표 발급 지점은 C census에 A 항이 없어 상한이 없다(2,625·3,401·3,417 B, 출력만). C가 상한을 정할 잔여로 계약에 남겼고, SQL 연결 전에 정한다. 2026-11-03 [다음 단위](explicit-transaction-20261103.md#ext-ceilings)가 begin·statement 시작 상한을 두었고, 표 발급은 C 잔여로 남았다. 그 단위는 C 호출의 receiver를 참조로 세어 statement 시작·표 발급이 8 B 크다(3,409·3,425 B).
- A의 여유가 얇다(`rollback.finish` 87, Commit 135, rollback base caller 159, next 167, CLR lane 231 B). 이 경로의 type 크기가 늘어 몫이 상한을 넘으면 계약을 다시 연다.
- caller term은 호출자가 참조 12개와 상태 target만 쥔다고 가정한다. 행·slot 배열을 stack에 두는 호출자는 다시 세야 한다.
- 다음 경로는 literal stand-in에서만 시험했다: commit의 `RollbackRequired`·Unchanged 거절 뒤 전체 rollback(LAC-A04), rollback flush, Begin·statement 시작 거절(LAC-A09), statement 검사의 CallerMustResolve(LAC-A11), `FinishPending`(LAC-A10·A12). 실제 C 위에서 이 거절을 일으키는 JOIN 사례는 없다.
- 12행을 넘는 commit(abort는 15행)은 정리 drain을 두 번 이상 부른다(J01이 13행 commit으로 확인). 정리 비용(그룹마다 전체 WAL replay)은 그대로다.
- 첫 Row profile의 행당 Free Undo page 하나는 그대로다.

<a id="aci-checks"></a>
## 8. 저장소 검사

시험과 export는 `4d7d31a`(최종 제품 배치)에서 했다(클라우드). 그 뒤 commit은 lint 속성(`clippy::result_large_err`의 이유 단 allow 세 개), 문서와 기록만 바꾼다.

- 변경 코드 시험: `storage::row::insert::driver::`(LAC-A01..A12, LRC-A, `mca00_d15_*`, `f4_a_*`와 기존 driver 시험), `f4_c_rollback_budget_*`, `f4_c_commit_budget_memory_and_native`, `row_join::retire::autocommit::`(LAC-J01·J02), `row_join::retire::driver::`(LRC-J01·J02)를 한 번에 돌려 45개 통과(118.48 s, JOIN은 catalog 식별 환경변수와 `RUST_MIN_STACK` 32 MiB). census 출력은 4절 표와 같다.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo fmt --all -- --check`: 최종 tree에서 통과. `4d7d31a`의 clippy는 새 함수 셋(`statement_refused`, `table_not_issued`, `start_statement_live`)의 `result_large_err`를 지적했고, driver와 같은 이유를 단 allow로 고쳤다.
- 단독 export: `scripts/module_single_source.py`(정책 main `5f478d0`)로 만든 a(C+A)와 b(C+B)에서 `cargo check -p xadb-engine --lib --tests --all-features`가 경고 없이 통과. a에서 LAC-A01..A12, `f4_a_autocommit_budgets_*`, `f4_a_caller_budgets_*`, `mca00_d15_*` 17개 통과. b에는 A 파일과 JOIN 시험이 없다.
- `devdoc/verify_documents.py`: 통과.
- module gate: [`20261102-autocommit-insert.json`](../../module-gate/records/20261102-autocommit-insert.json), D2 `policy`·`common` 검토가 바뀐 Rust 파일 8개와 정책 문서, 그리고 사례 입력인 바뀌지 않은 C census 파일 `rollback_budget_tests.rs`(검토가 읽었다)를 최종 blob으로 덮는다. CI 선택은 LAC-A01..A12·census 시험(head와 C+A), C rollback census와 LAC-J01·J02(head)다. LRC-J01·J02와 C commit census는 위에서 로컬로만 돌렸다(그 시험 파일은 검토 대상이 아니고, COMMIT·STATEMENT_END는 바뀌지 않았다). `check_module_gate.py check --mode push`: 통과.

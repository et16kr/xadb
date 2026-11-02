# 자동커밋 INSERT 실행기와 rollback 상한 재계약

2026-11-02

**상태: 구현·선택 검증 완료 · 독립 검토 · PR 대기.** 계약은 [자동커밋 INSERT 실행기](../contracts/dev3-module-boundaries.md#autocommit-insert)다. 기준 source는 main `5f478d0`(PR #38 병합)이고, branch는 `claude/project-thread-9inq97`이다.

<a id="aci-why"></a>
## 1. 왜 이 단위인가

- [live 정리 호출자](live-row-cleanup-caller-20261102.md)까지 A statement driver에는 insert·commit·rollback·정리 단계가 따로 있었지만, 이것들을 Begin부터 거래 끝까지 이어 부르는 제품 실행기는 없었다. 시험만 단계를 하나씩 불렀다.
- 실행기를 설계하다 보니 A의 Commit frame 몫이 상한 `COMMIT` 3,024 B에 정확히 닿아 있고, C Commit window의 여유도 2·11·146 B뿐이었다. 사용자에게 결정 카드로 물었고, 사용자는 "C 커밋 여유 확보 후 실행기"를 골랐다(카드: C commit 경로의 중복 slot을 줄이고 COMMIT 상한을 새 계약으로 올린 뒤 실행기를 만든다. 여유가 모자라면 조사 기록만 남긴다).
- 실제로는 commit 쪽을 A에서 정리해 C Commit window와 COMMIT 상한을 바꾸지 않고 들어갔다(3절). 원 상한을 넘은 것은 statement rollback 쪽이었고, 이것을 F4 규칙의 "양쪽의 새 계약"으로 다시 정했다. C 쪽 여유가 남으므로(최소 610 B) "조사 기록만" 경우가 아니다.

<a id="aci-change"></a>
## 2. 바꾼 것

| 쪽 | 파일 | 내용 |
| --- | --- | --- |
| A | `row/insert/driver/autocommit.rs` (새) | 결과 slot `Autocommit`(`Idle`, `Committed`, `NotBegun`, `Aborted`, `Fenced`)과 단계: `run_statement`, `table_failed`, `end_failed_statement`(mark까지 rollback 뒤 `finish_statement`), `commit_answer`, `abort_transaction`(전체 rollback 뒤 `finish_aborted`), `refused`, `fence` |
| A | `row/insert/driver/live.rs` | 표 발급 trait `LiveTables`, 실행기 `autocommit_insert_live`, 그 statement 단계 `statement_live`와 commit 단계 `commit_live_step`, `RowRollback`의 `flush`, census `autocommit_sites` |
| A | `row/insert/driver.rs` | `Rollback::flush`(정지한 rollback이 쥔 call의 flusher로 target flush), `Stage::Begin`·`Table`·`RollbackFlush`·`RollbackFinish` |
| A | `driver/tests.rs`, `driver/budget_tests.rs` | literal stand-in의 rollback flush와 LAC-A01..A10(A08 없음), `f4_a_autocommit_budgets_{memory,native}` |
| C | `wal_io/row_caller_budget.rs` (`cfg(test)`) | `ROLLBACK_CALLER` 512 → 1,920, `rollback.next` 6,144 → 6,912, `rollback.clr_lane` 11,520 → 12,672, `rollback.finish` 6,144 → 6,528 B와 그 근거 주석 |
| JOIN | `row_join/retire/autocommit.rs` (새), `row_join/retire.rs` | 실제 C 위의 LAC-J01·J02와 시험용 표 발급(`Tables`: Catalog 표를 읽어 `CheckedTable`을 낸다) |

C 제품 코드, WAL 형식·frame·opcode·page 형식·오류 종류, disk API는 바뀌지 않았다. 기존 insert·commit·rollback·정리 입구와 그 census도 그대로다(`Rollback::flush`는 기존 `flush_rollback_live`와 같은 continuation이다).

<a id="aci-flow"></a>
## 3. 실행 순서와 frame 배치

실행기는 다음을 차례로 한다. 단계마다 inline되지 않는 frame이고, 결과는 호출자가 쥔 slot 하나에 쓴다. 그래서 한 단계의 slot이나 Failure가 뒤 단계의 C 호출 아래에 놓이지 않는다.

1. C `begin_row_transaction`. Unchanged 거절이면 `NotBegun`(거래 없음), 아니면 `Fenced`.
2. `statement_live`: C `begin_statement`, 호출자의 `LiveTables::issue`, 기존 driver `insert_statement`. statement view가 거래를 빌리므로 이 단계는 call의 port와 Owner를 다시 빌린 `LiveCall`로 부른다. 실패하면 `end_failed_statement`가 statement를 끝낸다(`RollbackRequired`면 mark까지 rollback한 뒤 `finish_statement`, `Returned`·`FinishPending`이면 `finish_statement`만).
3. 성공이면 `commit_live_step`이 C `commit_row`를 부르고 `commit_answer`가 답을 읽는다. 실패면 `abort_transaction`.
4. `abort_transaction`: `start_full_rollback`, 단계 반복(`FlushRequired`만 `Rollback::flush`로 이어 감), `finish_rollback`, `finish_aborted`.

거래의 수명은 port의 수명이라, 거래 끝 단계는 실행기의 call을 그대로 쓴다. 정리는 실행기가 돌아온 뒤 호출자가 같은 call로 `retire_finished_live`를 부른다(drain이 complete가 아니면 다시).

commit이 상한 안에 든 이유: commit 단계가 자기 frame에 C 답(`Result<Committed, TransactionFinishError>`) 하나만 쥐고, 실행기 상태(결과 slot)는 호출자에, 행과 slot 배열은 호출자의 stack 밖에 있다. 기존 `commit_live`가 쥐던 `DriveError` 결과와 TFE 두 번째 slot이 이 경로에 없다. 설계 단계의 추정은 2.7–2.9 KB였고 census는 2,897 B다.

<a id="aci-census"></a>
## 4. 자원

셈은 F4와 같다(source-bound inventory, stack 측정 아님, 표준 slack 1,920 B 한 번, C 호출 결과 두 번, 경로 위 never-inlined frame의 모든 slot). caller term 408 B = LiveCall + Owner 둘 + `Autocommit` slot + `Plan4Target` + 참조 12개. memory·native Writer 값이 같다.

| A 의무 | A 몫 | 상한 | 여유 |
| --- | ---: | ---: | ---: |
| Commit frame | 2,897 | 3,024 | 127 |
| statement end | 5,964 | 6,144 | 180 |
| rollback base caller | 1,713 | 1,920 (원 512) | 207 |

insert 지점(실행기의 statement 경로, frame-static):

| 지점 | 몫 | 상한 |
| --- | ---: | ---: |
| `insert.check` | 4,836 | 11,520 |
| `insert.preflight` | 5,516 | 6,144 |
| `insert.try_flush` / `continue_flush` | 5,901 / 5,893 | 11,520 |
| `insert.validation_lane` | 7,604 | 11,520 |
| `insert.reserve` / `finish_preflight` | 7,156 / 6,932 | 11,520 |
| `insert.producer_lane` | 9,348 | 11,520 |
| `insert.forward` | 7,740 | 25,044 (charge − producer) |
| `insert.prepare` / `apply` / `reopen` / `cancel` | 7,276 / 8,556 / 8,404 / 8,916 | 11,520 |
| `insert.finish_statement` | 5,964 | 11,520 |

rollback 지점. statement 경로는 실행기 caller + 실행기 + `statement_live` + `run_statement` + `end_failed_statement` 아래, 거래 경로는 실행기 caller + 실행기 + `commit_live_step` + `commit_answer` 아래다(statement 실패 뒤 거래 rollback은 statement 단계가 돌아온 뒤라 거래 경로보다 얕다).

| 지점 | statement 경로 | 거래 경로 | 상한 |
| --- | ---: | ---: | ---: |
| `rollback.start` | 5,938 | 5,586 | 6,144 |
| `rollback.next` | 6,697 | 6,345 | **6,912** (원 6,144) |
| `rollback.clr_lane` | 12,521 | 12,169 | **12,672** (원 11,520) |
| `rollback.clr` | 10,913 | 10,561 | 11,520 |
| `rollback.clr_prepare` | 11,353 | 11,001 | 11,520 |
| `rollback.clr_apply` | 11,425 | 11,073 | 11,520 |
| `rollback.clr_reopen` | 11,273 | 10,921 | 11,520 |
| `rollback.finish` | 6,393 | 6,041 | **6,528** (원 6,144) |
| `rollback.finish_statement` | 5,425 | — | 6,144 |
| `transaction.finish_aborted` | — | 4,993 | 6,144 |

정리 drain(호출자 caller term 816 B = 실행기 caller + drain 결과 둘 + 참조 둘 + `Retired`): `cleanup.next` 3,184, `retire` 3,508, `try_flush` 3,979, `continue_flush` 3,971 B, 상한 `LIVE_ROW_CLEANUP` 4,096 B.

상한이 없는 지점(출력만): `begin_row_transaction` 2,633, `begin_statement` 3,001, 표 발급 3,017 B. C의 census에 이 입구들의 A 항이 없다.

C 쪽 대조(`f4_c_rollback_budget_{memory,native}`, `f4_c_commit_budget_memory_and_native`):

| 상한 | C가 대조하는 합 | 원 여유 | 새 여유 |
| --- | --- | ---: | ---: |
| `ROLLBACK_CALLER` 1,920 | flush 단계 57,012 ≤ 65,536 (native) | 9,932 | 8,524 |
| 같은 값 | flush 뒤 다시 닫기 61,418 ≤ 65,536 (native) | 5,526 | 4,118 |
| `rollback.next` 6,912 | 선택 본문 36,044 + 상한 ≤ 44,304 | 2,116 | 1,348 |
| `rollback.clr_lane` 12,672 | lane 본문 27,297 + 상한 ≤ 44,304 | 5,487 | 4,335 |
| `rollback.finish` 6,528 | control 본문 37,166 + 상한 ≤ 44,304 | 994 | 610 |
| `COMMIT` 3,024 (그대로) | Commit window, gate wait cap | 그대로 | 그대로 |

memory Writer는 shared가 20,208 B로 8 B 작아 flush 두 줄의 여유가 8 B씩 크다. 새 상한 값은 실행기의 statement 경로 몫에 128 B 이상을 더해 128 B 단위로 올렸다(1,713 → 1,920, 6,697 → 6,912, 12,521 → 12,672, 6,393 → 6,528).

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
| LAC-J01 | 실제 C 위에서 13행 거래 commit(q 2). drain 두 번 64·2그룹(5×13+1), 뒤에 Row Job 전부 Free. 새 statement가 13행을 읽음. 재시작 복구가 READY q 2, CLR 0, 남은 Row cell 0, 같은 13행 | PASS (124 s) |
| LAC-J02 | NULL 행 거절 뒤 `Aborted`(Insert)와 drain 5그룹, 없는 표의 `InvalidInput` 뒤 `Aborted`와 1그룹, 한 행 commit 뒤 6그룹. 새 statement는 commit된 행만 읽음 | PASS |
| `f4_a_autocommit_budgets_{memory,native}` | 4절 A 표 전부 상한 이내, rollback 지점 집합이 상한 표와 같음 | PASS |
| `f4_a_caller_budgets_{memory,native}` | 기존 A 상한(기존 호출자) | PASS |
| `f4_c_rollback_budget_{memory,native}`, `f4_c_commit_budget_memory_and_native` | 4절 C 표 | PASS |

LAC-J01의 재시작은 C Progress 11단계로 READY에 닿는다(출력만, 정리할 Row Job은 없다). 첫 Row profile은 행마다 Free Undo page를 하나씩 쓰므로(후보가 없으면 preflight가 Capacity) J01은 setup에서 page 12개를 더 만든다.

<a id="aci-review"></a>
## 6. 독립 검토

(검토 뒤 채움)

<a id="aci-limits"></a>
## 7. 남은 한계

- SQL·DEV4 engine 연결이 없다. 지금 실행기를 부르는 것은 JOIN 시험뿐이다. 명시 거래(BEGIN … COMMIT)와 여러 statement, UPDATE·DELETE는 범위 밖이다.
- 대기·재시도 정책이 없다. rollback 도중의 Unchanged 거절도 fence로 간다(복구가 끝을 맺는다). Begin과 statement 시작의 Unchanged 거절은 handle을 남기지 않거나 거래를 rollback하므로 호출자가 다시 부를 수 있다.
- begin·statement 시작·표 발급 지점은 C census에 A 항이 없어 상한이 없다(2,633·3,001·3,017 B, 출력만).
- caller term은 호출자가 참조 12개와 상태 target만 쥔다고 가정한다. 행·slot 배열을 stack에 두는 호출자는 다시 세야 한다.
- commit의 `RollbackRequired`와 Unchanged 거절 뒤 전체 rollback은 literal stand-in(LAC-A04)에서만 시험했다. 실제 C 위에서 commit을 거절시키는 JOIN 사례는 없다. rollback flush도 stand-in에서만 시험했다.
- 12행을 넘는 commit은 정리 drain을 두 번 이상 부른다(J01이 13행으로 확인). 정리 비용(그룹마다 전체 WAL replay)은 그대로다.
- 첫 Row profile의 행당 Free Undo page 하나는 그대로다.

<a id="aci-checks"></a>
## 8. 저장소 검사

(검사 뒤 채움)

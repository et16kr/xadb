# live 정리 호출자: A statement driver의 정리 drain

2026-11-02

**상태: 구현·선택 검증 완료 · 독립 검토 두 차례 · PR 대기.** 계약은 [3.14 live 정리 호출자](../contracts/dev3-task-14-reclamation.md#live-row-cleanup-caller)다. 기준 source는 main `372b22e`(PR #37 병합)이고, branch는 `claude/project-thread-9inq97`이다.

<a id="lrc-why"></a>
## 1. 왜 이 단위인가

- [live Row 정리](live-row-retirement-20261102.md)는 C 입구 두 개(`row_cleanup_next`, `retire_row`)만 공급했다. 호출자가 둘을 직접 부르고 Busy와 `FlushRequired`를 스스로 처리해야 했다.
- 정리 본문이 43,924 B라 `CatalogProgressCharge` 44,304 B에 380 B만 남았다. 어떤 driver 호출도 이 몫에 들지 않았다.
- 사용자는 F6 측정에서 "M0만"을 골랐다. 다음 작업을 이어 달라고 해서, 남은 후보 중 정리 호출자를 골랐다. 이 단위는 A의 drain 입구와 그 자원 몫까지다. 제품 실행기가 없어 이 입구를 자동으로 부르는 곳은 아직 없고, 지금은 시험만 부른다.

<a id="lrc-change"></a>
## 2. 바꾼 것

| 쪽 | 파일 | 내용 |
| --- | --- | --- |
| C | `plan4_retention/row_start/retire.rs` | `retire_resident`가 준비 결과를 slot 하나에 두고 설치는 그 자리를 빌려 쓴다. `?`의 분기 값과 묶은 값(각 2,072 B)이 없어졌다. Observation은 반환 slot에 바로 쓴다. `carrier_source_bytes`가 바뀐 코드를 따른다 |
| C | `row_caller_budget.rs` | 호출자 상한 `LIVE_ROW_CLEANUP` 4,096 B (cfg(test), F4 상한과 같은 규칙) |
| C | `plan4_apply/source_tests.rs` | 본문 + 상한 ≤ charge를 memory·native type에서 확인 |
| C | `plan4_apply.rs` | `RetentionError::visibility_busy()`: Unchanged `Visibility(Busy)` 판별과 그 시험 LRC-C01 |
| C | `row_start/retire.rs`, `catalog_live.rs` | `row_cleanup_next`의 `skip` 유효 조건을 정확히 적음(호출 사이에 Job 할당이 없으면 설치 뒤에도 유효), census 주석 |
| disk API | `disk_api.rs`, `disk.rs` | `RowRetireStep`을 `pub(in crate::storage)`로 다시 내보냄(A가 없는 C+B export에서는 쓰이지 않아 이유를 단 `allow(unused_imports)`) |
| A | `row/insert/driver.rs` | `Boundary`에 `cleanup_next`·`retire`, generic `retire_finished`·`retire_group`, `Retired`·`RetireStop`, `Stage::CleanupNext`·`Retire`, census `cleanup_sites` |
| A | `row/insert/driver/live.rs` | `LiveCall`의 두 메서드(Busy를 `RetireNext::Held`로), `retire_finished_live`, census `cleanup_caller_source`·`live_cleanup_sites` |
| A | `driver/tests.rs`, `driver/budget_tests.rs` | literal stand-in의 정리 단계와 LRC-A01..05, cleanup 네 지점의 상한 검산 |
| JOIN | `row_join/retire/driver.rs` | LRC-J01·J02 (실제 C 위에서 A의 drain) |

제품 동작이 바뀌는 곳은 A의 새 입구 하나와 C의 `retire_resident`뿐이다. `retire_resident`는 같은 함수를 같은 순서로 부르고 같은 오류를 돌려준다. commit·abort·rollback 입구와 그 census는 바뀌지 않았다.

<a id="lrc-census"></a>
## 3. 자원

C 본문(memory·native type 같음):

| 항 | 전 | 후 |
| --- | ---: | ---: |
| `retire_resident` carrier | 6,576 | 2,216 |
| 준비 경로(준비 34,404 + carrier) | 40,980 | 36,620 |
| residency 경로 | 35,844 | 35,844 |
| shell·입구 항(caller·commit carrier·fence·entry) | 2,944 | 2,944 |
| 본문 | 43,924 | 39,564 |
| charge − 본문 | 380 | 4,740 |
| 호출자 상한 뒤 여유 | — | 644 |

A의 몫(상한 4,096 B, caller term 472 B, memory·native type 같음):

| 지점 | per-call | frame-static |
| --- | ---: | ---: |
| `cleanup.next` | 2,664 | 2,840 |
| `cleanup.retire` | 2,868 | 3,164 |
| `cleanup.try_flush` | 3,098 | 3,635 |
| `cleanup.continue_flush` | 3,090 | 3,627 |

첫 구현은 `retire_group`이 flush 결과를 `map_err`와 `?`로 넘겨 flush 지점의 frame-static 몫이 4,275 B였다. 상한을 올리지 않고, 실패를 직접 돌려주게 바꿔 결과 slot 두 개를 없앴다.

commit 입구 안에서 drain을 부르는 형태도 검토했다. A의 Commit frame은 C의 Commit window가 `row_caller_budget::COMMIT` 3,024 B로 묶고 A의 몫이 이미 그 값이라(A 여유 0 B, 2 B는 C의 stage 여유), drain의 slot을 더할 수 없다. 그래서 호출자가 `commit_live`·`finish_aborted_live`가 돌아온 뒤 같은 port·Owner로 `retire_finished_live`를 부른다. census는 frame 단위이므로, 한 함수에서 두 입구를 함께 부르는 제품 호출자는 drain을 inline되지 않는 별도 frame에서 부르거나, drain의 결과 slot(`Result<Retired, RetireStop>`)과 인자를 넣어 commit census를 다시 해야 한다. abort 입구는 `transaction.finish_aborted` 상한 6,144 B에 묶여 이 이유가 그대로 들지는 않지만, 두 경우를 같은 모양으로 두어 drain의 몫을 `LIVE_ROW_CLEANUP` 하나로만 본다.

<a id="lrc-tests"></a>
## 4. 시험과 결과

실행은 변경 코드에 닿는 시험만 했다(debug build, memory adapter, 클라우드).

| 시험 | 확인하는 것 | 결과 |
| --- | --- | --- |
| LRC-C01 | `visibility_busy`는 Unchanged `Visibility(Busy)`만 참. CallerMustResolve Busy, 다른 visibility 원인, visibility의 Concurrency Busy, Retention의 Concurrency Busy와 Busy는 거짓 | PASS |
| LRC-A01 | 한 commit의 6그룹(Prune, 같은 root의 ReleaseRole 셋, RootFree, TerminalFinish)을 C가 고르는 순서로 하나씩 설치하고 None에서 끝남. 정리할 것이 없으면 C 호출 한 번 | PASS |
| LRC-A02 | Busy 단계는 ordinal로 건너뛰고 다음 drain에 남김. `skip`에는 held ordinal만 | PASS |
| LRC-A03 | residency victim을 flush하고 Ready까지 계속한 뒤 같은 그룹을 다시 시도 | PASS |
| LRC-A04 | 다른 거절(Unchanged Capacity, Fatal, `row_cleanup_next`의 Fatal, flush 거절)은 그때까지의 수와 함께 drain을 끝냄. Fatal만 must-resolve | PASS |
| LRC-A05 | 9번째 Busy와 64그룹에서 drain이 끝나고 C를 더 부르지 않음(`complete: false`) | PASS |
| LRC-J01 | 한 Status에서 commit → abort → commit. drain이 6·5·6그룹을 설치하고 매번 Row Job이 전부 Free. 남은 것이 없는 drain은 0. 새 statement가 commit된 두 행을 읽고 rollback된 행은 없음 | PASS |
| LRC-J02 | 오래된 statement snapshot이 Prune을 잡는 동안 drain은 held 1, 설치 0으로 끝나고 WAL 파일·Job이 그대로(두 번). snapshot이 끝난 뒤 6그룹 | PASS |
| `f4_a_caller_budgets_{memory,native}` | A의 기존 상한과 cleanup 네 지점 ≤ 4,096 B | PASS |
| `live_row_retirement_fits_the_progress_charge` | 본문 39,564 + 4,096 ≤ 44,304 B (memory·native type) | PASS |
| A driver 기존 시험, LRR-02..06, F6 M0 CI 사례 | 바뀐 driver·`retire_resident`의 회귀 | PASS (`a1d962a`에서 위 사례와 함께 한 번에 34개 통과, F6 M0 측정 사례 1개는 `#[ignore]`) |
| LRR-01 | 33개 거래와 재오픈, `retire_resident` 회귀 | PASS (`a1d962a`, 526.55 s) |

clippy(`-D warnings`, 모든 target)와 fmt, 두 single-source export(a, b)의 `cargo check --lib --tests`, 문서·gate 검사는 7절에 적는다.

<a id="lrc-review"></a>
## 5. 독립 검토

별도 agent run이 읽기만 하고(cargo 없이) 두 차례 검토했다. 기록은 [검토 기록](../../module-gate/evidence/live-row-cleanup-caller/review.md)에 있다.

- 1차(`a1d962a`): ACCEPT WITH REQUIRED CHANGES. 필수 4건은 `call_sites` 문서가 떨어져 나간 것, commit 밖에서 부르는 이유가 frame 단위 census를 빠뜨린 것, "한 commit의 정리는 6그룹"이 한 행에만 맞는 것(5n+1, abort 4n+1), 기록·검사 미완이었다. 권고 5건(C의 skip 규칙 문구, "자동" 표현, stand-in이 같은 쌍을 한꺼번에 지우던 것, `visibility_busy` 시험, 실제 C 위의 flush)과 NIT 3건이 있었다. 모두 `d565c60`에서 고치거나 문서에 적었다(flush는 6절의 한계).
- 2차(`d565c60`): ACCEPT, 열린 필수 0. 1차 12건이 모두 해결됐고, `a1d962a` 이후 제품 코드는 주석·시험 코드·lint 속성만 바뀌어 LRR-01 결과가 그대로 유효하다고 확인했다. NIT 2건(stand-in의 `expect`, "같은 call로" 문구)은 `5712388`에서 고쳤다.
- 확인한 것: `retire_resident`의 호출·순서·오류·효과가 같다, C·A census 셈이 맞다, Busy만 붙잡는다, skip 목록을 drain 내내 쓰는 것이 안전하다(건너뛴 cell은 그 자신의 단계로만 비워지고 정리는 cell을 할당하지 않는다), 금지 import가 없다.

<a id="lrc-limits"></a>
## 6. 남은 한계

- 제품 statement 실행기(SQL 연결)가 아직 없어 commit·abort 뒤 이 단계를 부르는 곳은 시험뿐이다. 그 실행기는 자기 caller term으로 cleanup census를 다시 해야 한다. background cleaner도 없다.
- 정리 비용은 그대로다. 그룹마다 전체 WAL replay를 1회 하고, 한 commit 뒤 drain은 6회다([F6 M0](f6-wal-revalidation-survey-20261102.md#f6a-m0)).
- `row_cleanup_next`의 본문(control Job decode)은 live 정리 단위와 같이 charge 밖이고 census가 세지 않는다.
- 한 drain에서 붙잡힌 단계가 8개를 넘으면 drain이 끝난다. 그 actor들이 풀리기 전에는 뒤의 단계도 다음 drain을 기다린다. drain마다 붙잡힌 단계를 최대 8개까지 다시 준비하므로, 그때마다 전체 WAL replay가 더 든다.
- Busy가 아닌 Unchanged 거절도 drain을 끝낸다. C는 다음 drain에서 같은 단계를 다시 먼저 고르므로, 그 원인이 없어질 때까지 뒤의 단계도 기다린다.
- n행 거래는 commit 뒤 5n+1그룹, abort 뒤 4n+1그룹을 남긴다. 12행을 넘는 commit은 drain 한 번(64그룹)에 끝나지 않으므로, 호출자는 `complete`가 false인 동안 다시 drain해야 한다. 시험은 한 행 거래만 쓴다.
- 실제 C 위의 정리 `FlushRequired`는 시험하지 않았다. LRC-J01·J02는 frame 16이라 victim이 필요 없고, [live 정리](live-row-retirement-20261102.md)의 LRR-06대로 10 frame 이하는 준비가 먼저 실패하고 11 frame 이상에서는 victim이 생기지 않는다. drain의 flush 경로는 literal stand-in(LRC-A03, A04)에서만 시험한다.
- R14 인수 사례의 PASS를 주장하지 않는다.

<a id="lrc-checks"></a>
## 7. 저장소 검사

모두 최종 code commit `5712388`에서 했다(클라우드).

- 변경 코드 시험: `storage::row::insert::driver::`, `row_join::retire::driver::`, `visibility_busy_tests::`, census 사례를 한 번에 돌려 29개 통과. census 출력은 3절 표와 같다(본문 39,564 B, 여유 644 B, A 네 지점, caller 472 B, memory·native type 같음). LRR-01..06은 `a1d962a`에서 통과했고 그 뒤 C 제품 코드는 주석만 바뀌었다.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo fmt --all -- --check`: 통과. 첫 clippy가 시험 helper의 `result_large_err`와 J01의 `drop`을 지적해 고쳤다.
- 단독 export: `scripts/module_single_source.py`(정책 main `372b22e`)로 만든 a(C+A)와 b(C+B)에서 `cargo check --lib --tests --all-features`가 경고 없이 통과. 처음에는 b에서 `disk.rs`의 `RowRetireStep` 재수출이 쓰이지 않아 경고가 났고, 이유를 단 `allow(unused_imports)`로 고쳤다.
- `devdoc/verify_documents.py`: 통과.
- module gate: [`20261102-live-row-cleanup-caller.json`](../../module-gate/records/20261102-live-row-cleanup-caller.json), D2 `common` 검토가 바뀐 Rust 파일 13개를 최종 blob으로 덮는다. `check_module_gate.py check --mode push`: 통과.

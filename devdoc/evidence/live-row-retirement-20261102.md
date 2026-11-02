# live Row 정리: 구현·선택 검증·독립 검토

2026-11-02

**상태: 구현·선택 검증 완료 · 독립 검토 반영 · PR #36으로 병합(main `8fa96c5`).** 계약은 [3.14 live Row 정리](../contracts/dev3-task-14-reclamation.md#live-row-retirement)다. 기준 source는 main `f1600a2`(PR #35 병합)이고, branch는 `claude/project-thread-9inq97`이다.

<a id="lrr-why"></a>
## 1. 왜 이 단위인가

- 열린 session에서 Row 거래의 Job을 비우는 경로가 없어, 한 행 거래는 한 session에 32개까지만 들어갔다(Job cell 64개, 거래마다 Terminal 하나와 넣은 행마다 Root 하나). [F6-a](f6-wal-revalidation-survey-20261102.md)가 이 한도를 확인했고, 사용자는 F6 측정을 live Row retirement 공급 뒤로 미뤘다.
- 정리 frame은 이미 복구가 쓰므로 새 WAL 형식이 필요 없다. 3.14 계획의 bundle 5에 속하지만, DAG 선행은 3.11.1 첫 행 commit뿐이라 앞당길 수 있다.

<a id="lrr-change"></a>
## 2. 변경

| 경로 | 변경 |
| --- | --- |
| `plan4_retention/row_start/retire.rs`(새 파일) | `row_cleanup_next(skip)`(다음 정리 단계 선택), `retire_row`(한 그룹)와 준비·설치 frame `retire_resident`, target 집합 residency, retirement 설치 `apply_row_retirement_n`, 봉인 receipt 셋, 자원 census 함수 |
| `plan4_retention/visibility/row_retirement.rs`(새 파일) | 잡은 Job·Status에서 scope·target을 읽는 `verify_row_retirement`, 표지 검사, stage·apply receipt, stage 거절 때 표지 되돌림 |
| `plan4_retention/row_start/recovery.rs` | `prepare_row_recovery`를 live에서도 부르게 열고, 한 그룹의 target 요청을 주는 `row_cleanup_requests` 추가 |
| `plan4_retention/catalog_lifecycle.rs` | `ExistingGroup::RowRelease`(live ReleaseRole) |
| `plan4_retention/catalog_live.rs`, `plan4_apply/source.rs` | live 정리 본문 `row_retirement`의 계산과 `CatalogProgressCharge` 안 확인 |
| `dirty_frame/retirement.rs` | `reject_row_stage` |
| `wal_io.rs` 등 재노출 6개 파일, `plan4_apply.rs` | `RowRetirementRejected`·`RowRetireStep` 재노출(crate 안) |
| 시험 `catalog_memory/row_join/retire.rs`(새 파일), `row_join.rs`, `catalog_memory.rs` | LRR-01–06. A의 INSERT와 B의 읽기를 함께 쓰므로 C+A+B 합류(JOIN) 시험 아래에 두었다(C 시험은 A·B를 쓸 수 없다). `with_fresh!`에 선택 인자 `file_bytes`(기본 32 MiB 그대로) |
| `plan4_apply/source_tests.rs` | census 시험 하나 |

제품 동작 변경은 새 입구뿐이다. 기존 입구(commit, rollback, 복구 정리)는 바뀌지 않았고 새 입구를 부르지 않는다.

<a id="lrr-tests"></a>
## 3. 선택 시험과 결과

AGENTS.md 규칙대로 바뀐 코드의 시험만 돌렸다. 아래 결과는 Rust가 마지막으로 바뀐 commit `e21e39c`의 build에서 차례로 한 번씩 돌린 것이다. debug build, `--test-threads=1`, `RUST_MIN_STACK=33554432`, catalog identity 환경값 셋을 주었다. LRR 사례의 libtest 경로 prefix는 `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_recovery::tests::catalog_memory::row_join::retire::`, census 사례는 `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::retention::source::tests::`이다.

| 사례 | 확인하는 것 | 결과 |
| --- | --- | --- |
| LRR-01 `lrr01_live_cleanup_reuses_jobs_status_and_undo_beyond_32_transactions` | 같은 Status에서 한 행 거래 33개를 차례로 commit하고, 각 거래 뒤 정리 순서가 Prune, ReleaseRole 3번, RootFree, TerminalFinish이며 Row Job이 모두 Free가 된다. 끝에 새 read-only statement가 33행을 값까지 읽고(Q 34), 파일을 다시 열면 같은 Q 34에서 READY이고 남은 Row Job이 없다 | PASS (547.35 s) |
| LRR-02 `lrr02_older_snapshot_keeps_prune_busy_until_it_ends` | commit 전에 잡은 statement snapshot이 살아 있는 동안 Prune은 `Visibility(Busy)`(Unchanged)이고 WAL·Job·fence가 바뀌지 않는다. 그 statement가 끝나면 정리 전체가 돌고, 새 statement가 행을 읽는다 | PASS (2.85 s) |
| LRR-03 `lrr03_running_actor_non_head_and_early_terminal_are_refused` | 두 행을 넣고 아직 실행 중인 actor는 선택되지 않고, 그 root의 Prune, Terminal의 TerminalFinish, 정리가 아닌 Cancel·Commit의 직접 호출이 거절된다. commit 뒤에는 head가 아닌 root의 Prune, root가 남은 TerminalFinish, Prune 전 RootFree가 거절된다. 거절 7개 모두 Unchanged이고 WAL·Job·fence가 그대로다. 그 뒤 정리는 head Prune이 먼저이고 Prune 2, ReleaseRole 6, RootFree 2, TerminalFinish 1로 끝난다 | PASS (4.54 s) |
| LRR-04 `lrr04_aborted_actor_is_cleaned_and_its_status_reused` | rollback된 actor는 ReleaseRole 3번, RootFree, TerminalFinish로 정리되고, 같은 Status로 다음 거래가 commit된다 | PASS (4.65 s) |
| LRR-05 `lrr05_busy_prune_does_not_hide_another_finished_actor` | A가 오래된 statement snapshot 뒤에 commit해 A의 Prune이 Busy다. 두 번째 Status의 B(Job이 A 뒤)는 전부 rollback한다. 첫 단계는 A의 Prune이고 거절되며 WAL·Job이 그대로다. A의 ordinal을 skip하면 B의 정리 전체(ReleaseRole 3번, RootFree, TerminalFinish)가 돌고 그 밖에는 남는 것이 없다. A의 Prune은 여전히 Busy이고, snapshot이 끝나면 A의 정리가 돈다 | PASS (5.63 s) |
| LRR-06 `lrr06_recovery_finishes_a_partial_live_cleanup` | live 정리를 Prune과 ReleaseRole 하나 뒤에 멈춘다(root cursor 1). 그 파일을 복구하면 나머지를 끝내고 Q 2에서 READY이며 Row Job이 남지 않는다 | PASS (4.21 s) |
| `live_row_retirement_fits_the_progress_charge` | 본문 43,924 B ≤ charge 44,304 B(memory·native type 모두, 여유 380 B), shared+charge ≤ 65,536 | PASS |

LRR-01의 비용. 한 그룹마다 WAL 전체를 다시 검증하므로 거래 수의 제곱으로 는다. 최종 build에서 round 0·8·16·24·32의 누적 시간은 1.5 s·45 s·148 s·314 s·537 s, 파일 합은 23.3 MB·28.3 MB·33.4 MB·38.4 MB·43.4 MB(round마다 약 0.63 MB)였고 다시 열기는 5.3 s였다. 4 core container에서 다른 build·시험과 일부 겹쳐 돌았으므로 시간은 참고값이다. 검토 반영 전 build에서 frame 수 16과 64로 같이 돌렸을 때 시간이 같았으므로 비용은 residency가 아니라 재검증에서 나온다. `with_fresh!`의 파일 합 상한 32 MiB와 사례 상한 30 s(기본)를 이 사례만 48 MiB, 1,200 s로 올렸다. 이것은 측정이 아니다. F6 측정은 별도 단위다.

처음 실행에서 LRR-01은 33개 거래·정리·읽기를 통과한 뒤 시험 틀의 파일 합 상한(32 MiB)에서 실패했다. 제품 오류가 아니라 시험 틀의 상한이었다.

공통 파일(`catalog_memory.rs`)이 바뀌어 gate가 [F2 필수 계약](../policies/repository-build-plan.md#module-gate-contracts)으로 고르는 S2W-R10/R11/R21-LIVE(`catalog_memory::row_live::s2w_r10_live_prepare_before_is_cancelled_by_the_driver`, `s2w_r11_live_apply_before_is_reopened_and_cancelled`, `s2w_r21_live_refused_cancel_is_fenced_and_must_resolve`)도 head에서 통과했다(각 1.2 s). CI는 이 셋을 C+A export에서도 돌린다. 로컬 `check_module_gate.py check --mode push`는 PASS(오류 0, 대기 0)다.

그 밖에 `cargo fmt --all --check`와 `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, C+A·C+B export의 `cargo check -p xadb-engine --lib --tests --all-features`(경고 0)가 통과했다. Clippy의 `large_enum_variant`(RowRetireStep)는 다른 Row step enum과 같은 이유(inline, heap 없음)로 허용했다.

<a id="lrr-review"></a>
## 4. 독립 검토

작성은 이 thread의 Claude session(`claude-thread-session/claude-project-thread-9inq97`), 검토는 별도로 띄운 검토 agent(`claude-independent-review/live-row-retirement`)다. 검토자는 코드를 읽기만 했고 cargo를 돌리지 않았다. 원문은 [검토 기록](../../module-gate/evidence/live-row-retirement/review.md)에 있다.

1차 판정은 ACCEPT(필수 0)였고 권고 4개와 NIT 7개가 나왔다. 반영한 것:

| 지적 | 반영 |
| --- | --- |
| 1 Busy Prune 하나가 다른 actor의 정리를 가림 | `row_cleanup_next(skip)`. 거절된 ordinal을 건너뛴다. LRR-05가 확인한다 |
| 2 residency 단계의 census 누락 가능성 | 준비·설치를 `#[inline(never)]` `retire_resident`로 분리해 residency 동안 prepared group이 없게 했다. 그 frame의 인자를 census에 더해 43,516 → 43,852 B(2차 검토 뒤 43,924 B) |
| 3 호출자 몫이 거의 없음 | 계약에 남은 몫(지금 380 B)이 호출자 몫 전부임과 자동 호출 단위의 의무를 적었다 |
| 4(d)(e) 시험 공백 | LRR-05(두 actor, 하나 Busy), LRR-06(중간 상태 뒤 복구) 추가 |
| 5 TerminalFinish actor를 잡은 page로 재확인하지 않음 | 모든 scope에서 잡은 Job의 scope·source를 확인한다 |
| 8 모순 Job의 효과 | 한 actor의 TerminalReady 둘, 같은 sequence의 head root 둘을 Fatal로 |
| 10 계약 문구 | 선택 순서(ordinal 순 첫 후보), capture Busy 범위, Fatal 예외를 고쳤다 |

반영하지 않은 것: 4(a) stage Unchanged 분기와 4(c) 닫힌 facade 거절은 주입 지점이나 복구 중간 hook이 필요해 이 단위에서 시험하지 않았다. 4(b) `FlushRequired`는 LRR-06이 수를 출력하지만 11 frame 이상에서 0이고 10 frame 이하는 준비 단계가 먼저 실패해 닿지 않았다. 6(표지의 Undo target 재계산)은 intent의 target 비교가 이미 막고, 7(Job 상태 재검사)은 producer가 하므로 그대로 두었다. 9(`row_cleanup_requests`의 target 선택 중복)는 복구 경로의 `prepare_row_recovery`를 바꾸면 복구 시험까지 다시 골라야 해서 남겼다. 어긋나면 모든 호출이 Unchanged로 실패하므로 조용히 틀리지는 않는다. 11(LRR-01을 느린 단계로)은 gate가 `#[ignore]` 사례를 받지 않아 그대로 두었다.

2차 확인(head `c7c3bd2`)은 ACCEPT, 열린 필수 0이었다. 수정이 맞고 새 정확성 문제가 없다고 했고, NIT 6개를 남겼다. 반영한 것: closure frame의 두 반환 slot(`Result<Option<Plan4Target>>`, `Result<Observation>`)을 모든 단계에 세도록 `caller_source_bytes`로 옮겼다(43,924 B, 여유 380 B). TerminalFinish 선택에서도 한 actor의 TerminalReady 둘을 Fatal로 잡는다. `skip`이 다음 설치까지만 유효함을 문서에 적었다. `retire_row`의 주석을 Fatal 예외에 맞췄다. LRR-05는 Job 할당 순서를 가정하지 않고, B 정리 뒤 남은 Row Job이 A의 root와 Terminal 둘뿐임을 확인한다. 나머지 NIT는 자리표시(이 문서·검토 기록)였다.

<a id="lrr-limits"></a>
## 5. 남은 한계

- 자동 호출이 없다. commit·rollback 뒤 호출자가 `row_cleanup_next`·`retire_row`를 불러야 한다. statement driver나 background cleaner 연결은 다음 단위이고, 그 호출자의 자원 몫은 지금 380 B뿐이다. (다음 단위 [live 정리 호출자](live-row-cleanup-caller-20261102.md)가 본문을 39,564 B로 줄여 호출자 상한 4,096 B를 두고, A statement driver의 정리 단계 `retire_finished_live`를 공급했다.)
- 첫 Row profile의 INSERT 거래만이다. UPDATE·DELETE·index·overflow 정리, allocator의 실제 page 반환(3.03.4), checkpoint·WAL 절단은 범위 밖이다.
- 시험이 닿지 않는 분기: stage의 Unchanged 거절(`reject_row_stage`), 정리 residency의 `FlushRequired`, 닫힌 facade에서의 거절.
- R14 인수 사례의 PASS를 주장하지 않는다.
- Codex가 로컬 main checkout에서 DEV3를 진행 중이므로, 병합 전에 겹치는 변경이 있는지 확인이 필요하다.

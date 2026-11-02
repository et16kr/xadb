# live Row 정리: 구현·선택 검증·독립 검토

2026-11-02

**상태: 구현·선택 검증 완료 · 독립 검토 반영 · PR 대기.** 계약은 [3.14 live Row 정리](../contracts/dev3-task-14-reclamation.md#live-row-retirement)다. 기준 source는 main `f1600a2`(PR #35 병합)이고, branch는 `claude/project-thread-9inq97`이다.

<a id="lrr-why"></a>
## 1. 왜 이 단위인가

- 열린 session에서 Row 거래의 Job을 비우는 경로가 없어, 한 행 거래는 한 session에 32개까지만 들어갔다(Job cell 64개, 거래마다 Terminal 하나와 넣은 행마다 Root 하나). [F6-a](f6-wal-revalidation-survey-20261102.md)가 이 한도를 확인했고, 사용자는 F6 측정을 live Row retirement 공급 뒤로 미뤘다.
- 정리 frame은 이미 복구가 쓰므로 새 WAL 형식이 필요 없다. 3.14 계획의 bundle 5에 속하지만, DAG 선행은 3.11.1 첫 행 commit뿐이라 앞당길 수 있다.

<a id="lrr-change"></a>
## 2. 변경

| 경로 | 변경 |
| --- | --- |
| `plan4_retention/row_start/retire.rs`(새 파일) | `row_cleanup_next`(다음 정리 단계 선택), `retire_row`(한 그룹), target 집합 residency, retirement 설치 `apply_row_retirement_n`, 봉인 receipt 셋, 자원 census 함수 |
| `plan4_retention/visibility/row_retirement.rs`(새 파일) | 잡은 Job·Status에서 scope·target을 읽는 `verify_row_retirement`, 표지 검사, stage·apply receipt, stage 거절 때 표지 되돌림 |
| `plan4_retention/row_start/recovery.rs` | `prepare_row_recovery`를 live에서도 부르게 열고, 한 그룹의 target 요청을 주는 `row_cleanup_requests` 추가 |
| `plan4_retention/catalog_lifecycle.rs` | `ExistingGroup::RowRelease`(live ReleaseRole) |
| `plan4_retention/catalog_live.rs`, `plan4_apply/source.rs` | live 정리 본문 `row_retirement`의 계산과 `CatalogProgressCharge` 안 확인 |
| `dirty_frame/retirement.rs` | `reject_row_stage` |
| `wal_io.rs` 등 재노출 6개 파일, `plan4_apply.rs` | `RowRetirementRejected`·`RowRetireStep` 재노출(crate 안) |
| 시험 `catalog_memory/row_retire.rs`(새 파일), `catalog_memory.rs` | LRR-01–04. `with_fresh!`에 선택 인자 `file_bytes`(기본 32 MiB 그대로) |
| `plan4_apply/source_tests.rs` | census 시험 하나 |

제품 동작 변경은 새 입구뿐이다. 기존 입구(commit, rollback, 복구 정리)는 바뀌지 않았고 새 입구를 부르지 않는다.

<a id="lrr-tests"></a>
## 3. 선택 시험과 결과

AGENTS.md 규칙대로 바뀐 코드의 시험만 돌렸다. debug build, `--test-threads=1`, `RUST_MIN_STACK=33554432`, catalog identity 환경값 셋을 주었다.

| 사례 | 확인하는 것 | 결과 |
| --- | --- | --- |
| LRR-01 `lrr01_live_cleanup_reuses_jobs_status_and_undo_beyond_32_transactions` | 같은 Status에서 한 행 거래 33개를 차례로 commit하고, 각 거래 뒤 정리 순서가 Prune, ReleaseRole 3번, RootFree, TerminalFinish이며 Row Job이 모두 Free가 된다. 끝에 새 read-only statement가 33행을 값까지 읽고(Q 34), 파일을 다시 열면 같은 Q 34에서 READY이고 남은 Row Job이 없다 | LRR01_RESULT |
| LRR-02 `lrr02_older_snapshot_keeps_prune_busy_until_it_ends` | commit 전에 잡은 statement snapshot이 살아 있는 동안 Prune은 `Visibility(Busy)`(Unchanged)이고 WAL·Job·fence가 바뀌지 않는다. 그 statement가 끝나면 정리 전체가 돌고, 새 statement가 행을 읽는다 | PASS (2.85 s) |
| LRR-03 `lrr03_running_actor_non_head_and_early_terminal_are_refused` | 두 행을 넣고 아직 실행 중인 actor는 선택되지 않고, 그 root의 Prune, Terminal의 TerminalFinish, 정리가 아닌 Cancel·Commit의 직접 호출이 거절된다. commit 뒤에는 head가 아닌 root의 Prune, root가 남은 TerminalFinish, Prune 전 RootFree가 거절된다. 거절 7개 모두 Unchanged이고 WAL·Job·fence가 그대로다. 그 뒤 정리는 head Prune이 먼저이고 Prune 2, ReleaseRole 6, RootFree 2, TerminalFinish 1로 끝난다 | PASS (4.86 s) |
| LRR-04 `lrr04_aborted_actor_is_cleaned_and_its_status_reused` | rollback된 actor는 ReleaseRole 3번, RootFree, TerminalFinish로 정리되고, 같은 Status로 다음 거래가 commit된다 | PASS (4.57 s) |
| `live_row_retirement_fits_the_progress_charge` | 본문 43,516 B ≤ charge 44,304 B(memory·native type 모두, 여유 788 B), shared+charge ≤ 65,536 | PASS |

LRR-01의 비용. 한 그룹마다 WAL 전체를 다시 검증하므로 거래 수의 제곱으로 는다. 같은 build에서 round 0·8·16의 누적 시간은 1.2 s·45 s·151 s, 파일 합은 23.3 MB·28.3 MB·33.4 MB(round마다 약 0.63 MB)였다. LRR01_COST frame 수 16과 64의 시간이 같아서 비용은 residency가 아니라 재검증에서 나온다. `with_fresh!`의 파일 합 상한 32 MiB와 사례 상한 30 s(기본)를 이 사례만 48 MiB, 1,200 s로 올렸다. 이것은 측정이 아니다. F6 측정은 별도 단위다.

처음 실행에서 LRR-01은 33개 거래·정리·읽기를 통과한 뒤 시험 틀의 파일 합 상한(32 MiB)에서 실패했다. 제품 오류가 아니라 시험 틀의 상한이었다.

그 밖에 `cargo fmt --all --check`와 `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`가 통과했다. Clippy의 `large_enum_variant`(RowRetireStep)는 다른 Row step enum과 같은 이유(inline, heap 없음)로 허용했다.

<a id="lrr-review"></a>
## 4. 독립 검토

LRR_REVIEW

<a id="lrr-limits"></a>
## 5. 남은 한계

- 자동 호출이 없다. commit·rollback 뒤 호출자가 `row_cleanup_next`·`retire_row`를 불러야 한다. statement driver나 background cleaner 연결은 다음 단위다.
- 첫 Row profile의 INSERT 거래만이다. UPDATE·DELETE·index·overflow 정리, allocator의 실제 page 반환(3.03.4), checkpoint·WAL 절단은 범위 밖이다.
- R14 인수 사례의 PASS를 주장하지 않는다.
- Codex가 로컬 main checkout에서 DEV3를 진행 중이므로, 병합 전에 겹치는 변경이 있는지 확인이 필요하다.

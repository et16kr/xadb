# DEV3 구현 코드 리뷰 — 2026-10-23

**현재 상태: D3-OCR-001 수정 / 관련 신규 시험 5건 PASS / D1 독립 검토 수용 / 전체 DEV3·S5 미완료 유지.**

사용자의 후속 수정 요청으로 [오류 경계 수정과 선택 검증](#fix-followup)을 수행했다. 아래 최초 리뷰의 소스 identity·정적 판단·당시 미실행 기록은 원래 시점의 증거로 보존한다.

## 최초 OCR 리뷰 기록

**당시 상태: 정적 리뷰 완료 / high 결함 1건 미수정 / 제품 시험 미실행.**

사용자 요청에 따라 `open-code-review-delegate`로 현재 DEV3 제품 코어 51개 파일을 검토했다. reserve/cancel이 WAL 영속화 뒤 정상 flusher와 경합할 때, 이미 적용된 변경을 `Unchanged`로 반환하는 오류 경계 문제를 확인했다. 아래 경합 순서는 정적 코드 분석이며 실제 재현 시험 결과가 아니다. 리뷰 요청 범위에서 제품 코드와 시험 코드는 수정하지 않았다.

## 범위와 방법

| 항목 | 기록 |
| --- | --- |
| 검토자·문서 담당 | `/root`; OCR delegate의 host agent가 직접 리뷰 |
| 작업 분류 | D0 리뷰 근거와 탐색 등록. D1/D2 구현·계약 채택 또는 인수 판정 아님 |
| 착수·최종 대조 HEAD | `b9d36416415d762474b4073f60f9f95d6e1b5f2d` |
| 코어 범위 | DEV3 디스크 표현, WAL, fresh bootstrap, selected page reader, dirty allocation, allocator/Status flush, closed recovery, 등록·세대 reload, concurrency admission |
| 도구 | 스킬 v1.0.0, OCR CLI v1.9.10. OCR는 파일 선택·규칙 해석만 수행; 외부 LLM 호출 없음 |
| 상태 기준 | [DEV3 실제 구현](../implementation/dev3-implementation.md), [시험 계획](../plans/dev3-test-plan.md) |
| 공통 기준 | [구현 계약](../contracts/implementation-contracts.md), [코딩 기준](../policies/coding-convention.md), [개발 운영](../policies/development-policy.md), [문서 운영](../policies/documentation-policy.md) |

현재 작업 트리에는 별도 COMM·DEV2 문서 작업이 있었다. 이미 commit된 DEV3 누적 구현을 검토하기 위해 대상 원문을 별도 로컬 Git 사본의 빈 baseline 위에 복사했다. 합성 diff의 `added`는 전체 원문을 선택하기 위한 상태이며 원본 저장소의 신규 변경이나 개발 이력이 아니다. 원본 ref·index·제품 소스를 변경하지 않았다.

`ocr delegate preview --format json` → 원본 경로의 `ocr delegate rule --format json` → `git --no-pager diff --no-ext-diff --no-textconv --no-color` → 전체 원문과 호출 경로·소유 계약 대조 → 파일별 coverage 기록 순서로 진행했다. 합성 patch는 전체 원문 추가이므로 동일 bytes의 원문을 나누어 모두 읽었다. 적용 규칙은 `system / **/*.rs` 한 그룹이며 오류 효과, 소유권, 동시성, 취소, 자원·입력 경계를 확인했다.

| OCR coverage | 수치 |
| --- | --- |
| `total_files` | 51 |
| `reviewable_files` | 51 / 28,259행 |
| `reviewed_files` | 51 |
| `skipped_files` | 0 |
| `excluded_files` | 0; 선택한 사본 안의 수치 |
| `coverage_rate` | 100% = 51 / 51. DEV3 지원 도구 전체의 coverage가 아님 |

DEV3 첫 구현 직전 `d58c26b^`부터 위 HEAD까지의 연결부 diff도 추가로 읽었다: `crates/engine/Cargo.toml`, `crates/engine/src/lib.rs`, `crates/engine/src/transaction.rs`, `crates/foundation/src/codec.rs`, `crates/foundation/src/namespace.rs`, `crates/platform/src/namespace.rs`. 이 6개는 보조 문맥이며 위 51개 전수 원문 coverage에 합산하지 않는다.

별도 unit/integration/native 시험 파일·fixture·compiler 음성 사례, Python oracle/harness, finite reference model 전체, test-support facade 전체의 파일별 감사는 범위 밖이다. 관련 시험 코드 일부는 경합 주입점과 기존 검증 범위를 이해하기 위해 열람했다. inline 시험이 있는 코어 파일은 원문에 포함하되 시험 실행으로 세지 않는다. 일반 buffer/eviction, 행·index 거래·commit/undo, 일반 recovery/READY와 최종 S5는 아직 완성되지 않은 범위이며, 미구현 자체를 새 결함으로 중복 등록하지 않았다.

## 발견 사항

| ID | severity / category | 위치 | 상태 |
| --- | --- | --- | --- |
| D3-OCR-001 | high / bug (P1) | [dirty_allocation.rs](../../crates/engine/src/storage/wal_io/dirty_allocation.rs), 717행 | 미수정; 정적 확인, 재현 시험 미실행 |

critical 0, high 1, medium 0, low 0이다.

<a id="d3-ocr-001"></a>
### D3-OCR-001 — 영속화 후 최종 snapshot 경합이 거짓 Unchanged를 반환

`AllocationWriter::change()`는 stage 완료 뒤 `control.installation`을 보유하고 page·metadata·descriptor·DPT를 설치한다(634–685행). 설치 구간 오류는 의무가 있으면 `CallerMustResolve`로 승격하고 fence를 세운다(690–695행). content/version/pin을 반환한 후 `wait_durable()`도 성공한 상태에서 최종 snapshot을 다시 얻지만, 717행은 그 실패를 그대로 `?`로 반환한다.

현재 허용된 writer/flusher 동시 사용만으로 다음 순서가 가능하다.

1. reserve 또는 cancel writer가 설치를 마치고 guard를 반환한다. WAL requirement와 설치 의무는 남는다.
2. 전용 flusher가 해당 dirty image를 capture하고 같은 WAL requirement를 영속화한다. writer가 먼저 sync했어도 성립한다.
3. flusher가 DWB·home sync·retirement를 마친 후 `finalize_flush()`에서 해당 frame의 content write guard를 얻는다.
4. writer의 `wait_durable()`은 성공하고 마지막 `snapshot()`이 실행된다. write guard가 아직 있으므로 `PinnedTargets::try_read()`가 `Busy / Unchanged`를 반환한다.
5. 717행에서 그 오류가 반환되어 성공 끝의 `installation = None`과 cancel의 batch/terminal 정리가 생략된다. 이 경로에는 fence 승격이 없다.

[dirty_frame.rs](../../crates/engine/src/storage/dirty_frame.rs)의 612–634행은 content `try_read`의 WouldBlock을 정상 `Busy / Unchanged`로 만든다. [dirty_flush.rs](../../crates/engine/src/storage/dirty_flush.rs)의 240–316행 및 355–389행은 finalize 동안 실제 write guard를 보유한다. finalize는 bytes를 다시 바꾸지 않더라도 이 read와 상호 배제된다. 정상 스케줄링에서 가능한 경합이며 외부 파일 변조나 test-only 상태 손상을 요구하지 않는다.

이때 WAL과 allocator 변경은 이미 영속화됐으므로 API 전체 결과를 `Unchanged`로 표시할 수 없다. 설치 의무가 남아 다음 writer 호출은 경합이 끝난 뒤에도 `Invariant / CallerMustResolve`로 fence된다(같은 파일 509–510, 578–579행). 따라서 일시적인 정상 경합이 잘못된 재시도 안내와 후속 세션 중단으로 이어진다. [status_init.rs](../../crates/engine/src/storage/wal_io/status_init.rs)의 `reserve_one()`도 429–432행에서 이 효과를 그대로 전달하므로, reserve가 영속화됐는데 `ReservedStatus`를 반환하지 않는 경로가 있다.

이는 [dirty stage의 최종 observation까지 의무 유지 계약](../contracts/dev3-dirty-stage.md) 및 [allocator flush의 writer/flusher WAL 인계·조건부 finalize 계약](../contracts/dev3-allocator-flush.md)을 함께 사용할 때 발생한다. WAL이 사라지거나 중복 할당이 이미 재현됐다는 주장은 하지 않는다. 문제는 완료된 효과의 오분류와 남은 의무를 가진 세션의 처리다.

수정 시 마지막 snapshot을 포함한 모든 post-stage 오류에서 원래 phase/cause와 의무를 보존하고 `CallerMustResolve`로 승격하며 공통 fence를 세워야 한다. 또는 post-apply 완료를 명시적으로 재개하는 경로를 설계해야 하며, 단순히 의무를 지우거나 실패를 무시해서는 안 된다. 기존 `BeforeSync`와 `Finalized` pause 지점으로 reserve/cancel 각각의 이 순서를 고정하는 좁은 회귀 사례를 추가하는 것이 적절하다. Status reserve wrapper의 효과도 함께 확인해야 한다. 이는 후속 수정·시험 제안이며 이번 실행 결과가 아니다.

## 그 밖의 주요 확인 범위

| 경계 | 정적으로 대조한 내용 |
| --- | --- |
| bytes·소유 identity | page/record/control 길이·checksum·canonical padding, expected identity와 generation, namespace·owner 대조 |
| WAL staging·durability | 실제 stage가 만든 requirement, origin/interval 결합, 단일 I/O 소유, Condvar predicate 재검사, sync 오류의 fence·원인 보존 |
| dirty 설치·flush | page/descriptor/DPT 설치 순서, latch 밖 I/O, WAL 선행, DWB Pending→home→retirement, redirty 시 counter 조건부 clean |
| Status INIT | 실제 예약과 role, 두 after-image/descriptor 결합, publication과 두-pin reader, 영속화 전후 오류 효과 |
| closed recovery | selected source 안정화, canonical prefix 분석과 실제 home 분리, Pending 수리 뒤 인계, page별 redo와 최종 상태 대조 |
| allocator residency | 실제 원본 storage와 clean home 대조, 단일 등록 lookup, generation 갱신, 오래된 pin/permit과 frame 내용의 혼동 방지 |
| 자원·범위 | fixed workspace와 checked counter, source-value 회계와 실제 stack/성능 측정의 구분, 제한 owner의 권한을 일반 READY로 확장하지 않는지 확인 |

위 표는 별도 PASS 판정이나 완전성 증명이 아니다. 확정한 결함 외에 추가로 보고할 구체적 결함을 찾지 못했다는 범위의 결과다.

## 파일별 coverage 대장

각 identity는 `(path, added)`이고 처리 상태는 모두 `reviewed`다. 아래 경로는 저장소 루트 기준이다.

| path | 행 | 상태 |
| --- | ---: | --- |
| `crates/engine/src/storage/allocation_page.rs` | 618 | reviewed |
| `crates/engine/src/storage/anchor.rs` | 152 | reviewed |
| `crates/engine/src/storage/catalog_record.rs` | 483 | reviewed |
| `crates/engine/src/storage/control.rs` | 1169 | reviewed |
| `crates/engine/src/storage/dirty_flush.rs` | 550 | reviewed |
| `crates/engine/src/storage/dirty_frame.rs` | 1293 | reviewed |
| `crates/engine/src/storage/dirty_frame/registration.rs` | 192 | reviewed |
| `crates/engine/src/storage/dirty_frame/residency.rs` | 144 | reviewed |
| `crates/engine/src/storage/dirty_init.rs` | 1046 | reviewed |
| `crates/engine/src/storage/doublewrite_control.rs` | 411 | reviewed |
| `crates/engine/src/storage/key.rs` | 195 | reviewed |
| `crates/engine/src/storage/mod.rs` | 25 | reviewed |
| `crates/engine/src/storage/obligation.rs` | 457 | reviewed |
| `crates/engine/src/storage/overflow.rs` | 388 | reviewed |
| `crates/engine/src/storage/ownership.rs` | 797 | reviewed |
| `crates/engine/src/storage/page.rs` | 323 | reviewed |
| `crates/engine/src/storage/page_io.rs` | 357 | reviewed |
| `crates/engine/src/storage/read_buffer.rs` | 545 | reviewed |
| `crates/engine/src/storage/recovery_select.rs` | 530 | reviewed |
| `crates/engine/src/storage/row_record.rs` | 505 | reviewed |
| `crates/engine/src/storage/semantic_group.rs` | 656 | reviewed |
| `crates/engine/src/storage/session_fence.rs` | 33 | reviewed |
| `crates/engine/src/storage/status_home.rs` | 158 | reviewed |
| `crates/engine/src/storage/transaction_record.rs` | 505 | reviewed |
| `crates/engine/src/storage/tree_page.rs` | 527 | reviewed |
| `crates/engine/src/storage/undo_record.rs` | 1190 | reviewed |
| `crates/engine/src/storage/undo_ref.rs` | 74 | reviewed |
| `crates/engine/src/storage/wal_frame.rs` | 847 | reviewed |
| `crates/engine/src/storage/wal_io.rs` | 978 | reviewed |
| `crates/engine/src/storage/wal_io/allocation_budget.rs` | 103 | reviewed |
| `crates/engine/src/storage/wal_io/allocation_operation.rs` | 1007 | reviewed |
| `crates/engine/src/storage/wal_io/allocation_reservation.rs` | 534 | reviewed |
| `crates/engine/src/storage/wal_io/allocator_flush.rs` | 385 | reviewed |
| `crates/engine/src/storage/wal_io/allocator_residency.rs` | 292 | reviewed |
| `crates/engine/src/storage/wal_io/bootstrap.rs` | 983 | reviewed |
| `crates/engine/src/storage/wal_io/dirty_allocation.rs` | 733 | reviewed |
| `crates/engine/src/storage/wal_io/doublewrite_install.rs` | 669 | reviewed |
| `crates/engine/src/storage/wal_io/flush_io.rs` | 1534 | reviewed |
| `crates/engine/src/storage/wal_io/mutable_storage.rs` | 534 | reviewed |
| `crates/engine/src/storage/wal_io/recovery_effect/bootstrap.rs` | 164 | reviewed |
| `crates/engine/src/storage/wal_io/recovery_effect/mod.rs` | 311 | reviewed |
| `crates/engine/src/storage/wal_io/recovery_effect/physical.rs` | 480 | reviewed |
| `crates/engine/src/storage/wal_io/recovery_effect/port_io.rs` | 228 | reviewed |
| `crates/engine/src/storage/wal_io/recovery_effect/resources.rs` | 307 | reviewed |
| `crates/engine/src/storage/wal_io/recovery_effect/selection.rs` | 251 | reviewed |
| `crates/engine/src/storage/wal_io/recovery_effect/types.rs` | 193 | reviewed |
| `crates/engine/src/storage/wal_io/recovery_prefix.rs` | 1252 | reviewed |
| `crates/engine/src/storage/wal_io/status_home_flush.rs` | 585 | reviewed |
| `crates/engine/src/storage/wal_io/status_init.rs` | 1007 | reviewed |
| `crates/engine/src/storage/wal_io/status_init_operation.rs` | 956 | reviewed |
| `crates/engine/src/transaction/concurrency.rs` | 603 | reviewed |

## 소스 identity와 실행 상태

착수 사본과 리뷰 종료 시 작업 트리의 51개 파일을 SHA-256으로 대조했으며 모두 일치했다. 원본 HEAD도 바뀌지 않았다. 파일별 hash는 로컬 `artifacts/dev3-ocr-review-20261023-rp94je0l/source-manifest.json`, OCR 원문은 같은 디렉터리의 `preview.json`·`rules.json`, OCR comment schema와 coverage는 `review.json`에 남겼다. 이 산출물은 로컬 보조 자료이며 Git 배포 문서는 아니다.

- 소스 집합 SHA-256: `2b8b70944df25af7054f8e68a42f6fd9bf86581974f2a965bf1ad260b40e736d`.
- 계산 규칙: 위 대장 순서로 UTF-8 `path + NUL + 파일 SHA-256 hex + LF`를 연결한 bytes의 SHA-256.
- 합성 baseline: `9ccccaf2882869a467ba6d7ab78a3aebafd933f8`.
- 합성 tip: `7ed4552cb8c8521916f42402946df70d31ed7317`.

| 항목 | 이번 상태 |
| --- | --- |
| 제품·시험·harness 변경 | 없음 |
| 제품 시험·재현 시험·wrapper·부하 시험 | NOT_RUN. 제품 코드가 바뀌지 않아 [사용자 시험 제한](../../AGENTS.md)에 따라 실행하지 않음 |
| fmt/Clippy·문서 자동 검사 | NOT_RUN. 코드 변경·push 없음. 문서 자동 검사는 [push gate](../policies/documentation-policy.md#push-gate)에 남김 |
| OCR preview/rule·Git diff·소스 hash 대조 | 실행 완료; 제품 검증 PASS로 집계하지 않음 |
| 기존 DEV3 실행 증거 | 원래 source·실행 범위와 FAIL/NOT_RUN을 그대로 보존. 이 리뷰의 새 실행으로 승계하지 않음 |
| 문서 변경 | 이 리뷰 원문, evidence README, devdoc README, 문서 catalog 등록 |
| commit·push | 미실행 |
| DEV3·S5 | 미완료 유지. 이 리뷰는 최종 인수가 아니며 D3-OCR-001의 수정 검증도 남아 있음 |

<a id="fix-followup"></a>
## 후속 수정·선택 검증 — 2026-10-23

사용자의 “수정해 주세요” 요청으로 D3-OCR-001을 D1 범위에서 수정했다. `AllocationWriter::change()`의 WAL durable 이후 최종 `snapshot()` 오류를 기존 `fatal(e.phase, e.cause)`로 처리한다. 원 phase와 하위 원인·효과를 보존하면서 외부 효과만 `CallerMustResolve`로 승격하고 공통 fence를 세운다. installation·batch·cancel terminal 의무는 성공한 최종 observation에서만 제거한다. 최초 snapshot과 stage 이전 거절 경로, WAL/page 형식, 복구 알고리즘, dependency는 변경하지 않았다.

실제 writer를 기존 `BeforeSync` cut에서 멈추고 전용 flusher가 같은 page를 영속화한 뒤 `Finalized` cut에서 content write guard를 보유하도록 했다. 이때 writer를 재개하는 실패 스케줄과 flusher guard를 먼저 반환하는 정상 스케줄을 각각 공급했다. Status 예약 wrapper에는 같은 기존 cut을 공유하는 `cfg(test)` 전용 `BeforeSyncPause`를 추가했다. 임의 effect/LSN 주입이나 sleep 기반 경합이 아니다.

### 실행과 원 실패

| 구분 | 실제 결과·범위 |
| --- | --- |
| 수정 전 반례 | 새 `post_durable_snapshot_reserve_busy` exact 1건 FAIL, exit101. `(Snapshot, Unchanged)`가 기대 `(Snapshot, CallerMustResolve)`와 불일치. compile 성공 후 실제 경합에 도달했다 |
| 수정 후 | `post_durable_snapshot_` selector 5건 PASS, 실패0, ignored0, filtered out457. test profile compile2.38초·시험2.17초, 명령 전체4.57초 |
| 정상·오류 단언 | reserve/cancel의 고정 literal page·descriptor, complete/synced LSN1/2, pageLSN, pins0. 실패의 원 `Frame(Busy/Unchanged)`·외부 fatal 효과·반환 시 fence·retained obligation/terminal, 후속 관측/Status 재예약의 I/O0. 정상 완료의 의무 제거·cancel batch 해제 |
| 포맷 | 변경 Rust 4파일만 `rustfmt --check --edition 2024 --config skip_children=true` PASS |
| 관련 crate 정적 검사 | `cargo clippy -p xadb-engine --all-targets --all-features --locked --offline -- -D warnings` PASS. 시험 target을 검사했으며 시험을 실행하지 않음 |
| 실행하지 않은 것 | 기존 하위 시험·wrapper·workspace 시험, native crash/reopen, 장기 stress, 최종 S5. 문서 검증은 push gate에 남기고 실행하지 않음 |
| 독립 검토 | `/root/dev3_fix_review` 별도 AI 실행이 계약·원 반례·4파일 diff·현재 SHA·실행 로그를 직접 대조하여 D1 수용. 추가 결함0. 검토자의 시험 실행·파일 변경 없음 |
| 통합 상태 | 로컬 수정 검증. commit/push 없음. 일반 복구·전체 DEV3/S5 인수로 승격하지 않음 |

실행 명령은 다음과 같다. 수정 전 명령은 아래 `A` 접두사와 `post_durable_snapshot_reserve_busy`를 합친 전체 이름 및 `--exact`를 사용했다.

```sh
cargo test -p xadb-engine --lib --all-features --locked --offline post_durable_snapshot_ -- --nocapture --test-threads=1
```

<a id="fix-inventory"></a>
### 신규 5개 case inventory

주 소유는 NT03이며 NT04/05·FT03/05/09에 연결한다. 모두 profile1·reserve count1의 결정적 실제 thread 사례로, 수정 시 중간 필수이며 최종 S5에도 포함한다. 기존 dirty 필수53·Status flush 필수43의 원래 분모와 receipt는 변경하지 않는다. 이번 추가5를 별도로 등록하며 확대 조합·native 복구 완료로 집계하지 않는다. fixture는 기존 `tests/fixtures/dev3-dirty-allocation/p1-n1-{reserve,cancel}.{page,descriptor}`의 frozen bytes와 fresh Status memory port이며, oracle는 원인·효과·LSN·의무·fence·정상 해제 단언이다. 각 채널 ACK의 도달 제한은5초이며 누락/timeout은 실패다.

정확 selector는 다음 접두사와 표의 함수명을 연결한다. package는 `xadb-engine`, target은 `--lib`, feature는 `--all-features`, runner는 Rust test harness이며 반복은 각1회다.

- `A` = `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::dirty_allocation::tests::`
- `S` = `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::status_init::status_flush::tests::races::`

| case ID | 접두사 + 함수명 | 독립 기대값 |
| --- | --- | --- |
| D3-OCR-001-RB | A + `post_durable_snapshot_reserve_busy` | reserve WAL1 durable 후 snapshot Busy → 외부 fatal·fence, batch/installation 유지 |
| D3-OCR-001-CB | A + `post_durable_snapshot_cancel_busy` | cancel WAL2 durable 후 snapshot Busy → 외부 fatal·fence, batch/terminal/installation 유지 |
| D3-OCR-001-RR | A + `post_durable_snapshot_reserve_released` | flusher guard 반환 뒤 reserve 정상, fence 없음·installation 제거·batch 유지 |
| D3-OCR-001-CR | A + `post_durable_snapshot_cancel_released` | flusher guard 반환 뒤 cancel 정상, fence 없음·installation/batch/terminal 제거 |
| D3-OCR-001-SB | S + `post_durable_snapshot_status_reserve_busy` | Status Reserve wrapper가 Allocation Snapshot의 fatal 효과와 원 Frame Busy를 보존; 예약 handle 미반환 |

### 후속 source·실행 identity

기준 HEAD는 최초 리뷰와 같은 `b9d36416415d762474b4073f60f9f95d6e1b5f2d`다. 현재 product diff와 신규 시험만 검증했으며 별도 COMM·DEV2 작업은 유지했다. Rust/Cargo1.95.0, `x86_64-unknown-linux-gnu`, debug test binary의 SHA-256은 `198f41b423eac393ac2d3a0969b832192784b4740aa3869fc017f78048a85b98`다.

| 변경 source | 최종 SHA-256 |
| --- | --- |
| `crates/engine/src/storage/wal_io/dirty_allocation.rs` | `c51f5e2b417b82eb4243387585f16fd7e9e2ef23512d207fbb4f0ee72e989cbd` |
| `crates/engine/src/storage/wal_io/dirty_allocation_tests.rs` | `52a0d4a9ed0ed4463592c66ee66f89fe7db5e0ac649f2c328b092905505ab075` |
| `crates/engine/src/storage/wal_io/dirty_allocation_test_support.rs` | `be19f69bb8405121f8a5cc00f8745bbb0ffee723ed8e2d9ea6d62401b4e28535` |
| `crates/engine/src/storage/wal_io/status_home_flush_race_tests.rs` | `ec20c5e53467597f2ca87bbec135aa8cd5784beb61695df5e1240375ac9140e8` |

로컬 원자료는 `artifacts/dev3-ocr-fix-20261023-ebs7gn1u/`의 `baseline.log`, `fixed.log`, `fmt.log`, `clippy.log`, 각 `*-run.json`, `before-product-fix.json`, `after-product-fix.json`, `environment.json`, `baseline.patch`, `fix.patch`다. 원 FAIL은 보존하며 fixed PASS로 덮어쓰지 않았다. 이 로컬 보조 자료는 Git 배포 문서가 아니다.

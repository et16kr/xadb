# F6-a WAL 재검증 비용: 정적 조사, 측정 계획, M0 결과

2026-11-02

**상태: 정적 조사 완료 · 측정 계획 초안 · M0(계수) 실행([12절](#f6a-m0)) · M1(시간) NOT_RUN.** 측정은 2026-11-02 사용자 결정으로 live Row retirement 공급 뒤로 보류됐고, 그 공급(PR #36) 뒤 M0를 했다. 이 문단 아래의 원래 상태 설명은 F6-a 단위 당시의 것이다. 사용자는 2026-11-02 05:59 UTC 프로젝트 thread의 [D03](../plans/module-independence-followup-plan.md#user-decisions) 결정 카드에서 "정적 조사·측정 계획까지만"을 골랐다. 그래서 이 단위는 코드 읽기와 계획 작성만 한다. 측정, 계측 코드 추가, 제품 변경, 무변경 제품 시험은 하지 않았다. 아래 수치는 모두 코드·기존 기록에서 읽은 값이거나 그 값으로 계산한 분석 추정이며, 측정값은 하나도 없다. 측정은 [9절](#f6a-decisions)의 결정을 받은 뒤 별도 단위로 한다.

기준 source는 PR #33 병합 뒤 main `d86bafb`다. 계획의 위치는 [후속 계획 §10](../plans/module-independence-followup-plan.md#wal-cost), 측정 원칙은 [DP07](../policies/development-policy.md#section-8)이다. 초안은 네 차례의 독립 검토 지적을 반영했고 마지막 검토는 ACCEPT(열린 필수 0)다([검토](#f6a-review)).

<a id="f6a-summary"></a>
## 1. 요약

- **H1 한 번의 재검증은 WAL 전체에 비례한다.** `replay_durable`은 현재 epoch WAL의 `0..E`를 SHA-256으로 두 번 hash하고, 그 사이에 `4096..E`의 모든 frame을 다시 적용한다. 읽는 양은 약 3E, hash하는 양은 약 2E다. frame마다 64 page arena 전체의 census도 다시 한다.
- **H2 쓰기 거래 하나가 이 전체 재검증을 여러 번 한다.** 행 하나를 넣고 commit하는 Row 거래에서 코드로 확인한 것은 Begin group, 쓰기 preflight, Commit group의 3회다. retention flush가 일어나면 flush마다 1회(`prove_capture`), 그 뒤 다시 하는 preflight 1회가 더해진다. 비상주 page LOAD와 rollback·abort group도 각각 더한다.
- **H3 한 epoch 안의 누적 비용은 거래 수의 제곱에 비례한다(분석).** WAL은 거래마다 늘고 재검증은 매번 처음부터 한다. commit이나 rollback으로 끝나는 Row 거래는 Terminal Job cell 하나와 넣은 행마다 RootJob cell 하나를 쥐고 실행 중에는 비우지 않는다. Job cell은 64개라 그런 한 행 거래는 한 epoch에 많아야 32개이고, 그 WAL은 몇 MiB 수준이다(추정). 반면 Forward 전에 Cancel한 예약은 RootJob을 돌려주므로, Reserve→Cancel을 되풀이하면 한 거래 안에서 WAL을 cap까지 늘릴 수 있다(검사하지 않은 다른 한도가 없다면). 다시 열면 새 epoch로 E가 4096 B로 돌아간다(4절).
- **H4 재검증 동안 WAL File 잠금을 쥔다.** 다른 thread가 같은 storage의 WAL을 기록(`sync_claim`)하려 하면 그 시간만큼 기다린다. 다만 현재 계약은 병렬 writer를 범위 밖에 둔다.

이 넷은 가설이다. 처리량 회귀로 기록하지 않는다. 확인은 [7절](#f6a-plan)의 측정으로 한다.

- **M0 결과(12절, 계수만, pool frame 16·flush 없음·Int64 한 열 행).** H1·H2를 셌다. 한 행 commit 거래는 전체 replay를 4회(Begin, table 발급, preflight, Commit) 하고, 그 live 정리 6그룹이 1회씩 6회를 더한다. Begin과 preflight의 replay는 그때의 WAL 파일 끝까지 읽었다. 정리까지 한 거래 하나는 30·E₀ + 8.7 MB를 읽으며, WAL을 629,363 B 늘린다. L3(E 약 17 MB)에서는 516 MB다. 시간은 M1에서 잰다.

<a id="f6a-paths"></a>
## 2. 정적 경로: 전체 replay를 부르는 곳

전체 replay는 [`history.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/history.rs)의 `replay_durable` 하나다. 이를 부르는 입구는 `validate_live`(빈 WAL이면 seed만 한다), `replay_lookup_cut`, `prove_capture` 셋이다. 경로는 모두 `crates/engine/src/storage/wal_io/plan4_retention/` 아래다.

| 호출 지점 | 입구 | 함수 | 언제 | replay 버퍼 |
| --- | --- | --- | --- | --- |
| [`row_start.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/row_start.rs) | `validate_live` | `prepare_existing_row` | Row group마다: Begin, Commit(`row_start/commit.rs`), RollbackStart, AbortEnd, StatementFinish 등 | scope·checkout |
| [`row_preflight.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/row_preflight.rs) | `validate_live` | `observe_sources` | 행 쓰기 preflight마다 1회. reserve·prepare·apply·cancel은 replay하지 않는다([S2W-R22](../contracts/dev3-module-boundaries.md#row-write-validate-once)) | scope·checkout |
| `flush.rs` | `prove_capture` | `advance` | retention flush마다. preflight가 `FlushRequired`를 돌려주면 호출자가 flush하고 preflight를 다시 한다 | flush I/O workspace |
| `load.rs` | `validate_live` | `load_into` | 비상주 target LOAD마다 | `exclusive()` |
| `load.rs` | `validate_live` | `system_target`(`status_target`·`undo_target`) | 제품 호출자는 `recovery.rs`의 정리 단계 | `exclusive()` |
| `live.rs` | `validate_live` | `change_with_replay` | Catalog 쪽 거래 변경(Aging, Begin, Rollback, Abort, Finish)마다. Profile1 전용이며 Profile3에서는 `change`가 `WrongState`로 거절해 Row 경로에 없다 | scope·checkout |
| `catalog_live.rs` | `validate_live` | `prepare_existing_catalog` | Catalog group마다 | scope·checkout |
| `population.rs` | `validate_live` | `prepare_system_slot`, `allocate` | 호출마다 | `exclusive()` |
| `row_rollback_select.rs` | `validate_live` | `row_rollback_select` | rollback 선택마다 | scope·checkout |
| `visibility/binding.rs` | `validate_live` | `enable_catalog_visibility`, `activate_recovered_epochs` | open·recovery 활성화 | scope·checkout |
| `recovery.rs`, `row_start/recovery.rs` | `validate_live` | `refresh_recovered`, `prepare_row_recovery_n` | recovery 뒤 정리 단계마다 | `exclusive()`, scope·checkout |
| `read/lookup.rs`, `lookup_load.rs` | `replay_lookup_cut` | Undo·Status 위치 찾기, 비상주 읽기 load | 호출마다. 설치된 cut의 durable 끝까지 replay한다 | scope·checkout |

<a id="f6a-cost"></a>
## 3. 한 번의 재검증 비용

`replay_durable`은 durable 끝 offset E에 대해 다음을 순서대로 한다. WAL File mutex(`wal().file`)를 `try_lock`으로 잡고 1–4 내내 쥔다.

1. header 4096 B를 읽는다.
2. `hash_prefix(0..E)`: 163,840 B 단위로 읽으며 SHA-256을 계산한다.
3. seed 뒤 `reconstruct`: offset 4096부터 E까지 frame마다 160 B와 나머지를 읽는다. 그리고 `recovery::apply_frame`, `refresh_jobs`(Jobs page 5개 decode), `validate_bijection`(Profile3은 `validate_current`, 즉 Catalog·Row·ATT 전체 census)를 한다.
4. `hash_prefix(0..E)`를 다시 해 2의 값과 비교한다.

그 앞뒤로 namespace writer mutex를 `try_lock`으로 잡고 metadata를 확인한다(앞뒤 각 1회). 그래서 읽기는 약 3E B, SHA-256 입력은 약 2E B, frame 적용은 epoch의 frame 수 F번이다. frame마다의 census는 arena가 64 page로 고정이라 frame 수에만 비례한다. 그 한 번의 크기는 정적으로 정하지 않았다.

`validate_live`는 replay 뒤 control 사본과 replay bank를 비교하고, Profile3이면 XOI cache를 한 page 다시 encode해 비교한다. 이 부분은 WAL 길이와 무관하다.

<a id="f6a-growth"></a>
## 4. 거래당 호출 수, WAL 증가, epoch 상한

**호출 수(정적 하한).** 행 하나를 넣고 commit하는 Row 거래는 적어도 다음 3회의 전체 재검증을 한다.

- Begin group(`catalog_start.rs`의 `begin_selection` 뒤 `Change::Begin` → `prepare_existing_row`)
- 쓰기 preflight(`row_observe_cut` → `observe_sources`)
- Commit group(`row_start/commit.rs` → `prepare_existing_row`)

`prepare_existing_row`는 조건 없이 `validate_live`를 부른다. retention flush(`prove_capture`)와 그 뒤의 preflight 재시도, statement 종료·rollback·abort group, 비상주 target LOAD는 각각 더한다. 정확한 수는 [M0](#f6a-plan)에서 센다.

**WAL 증가(하한).** 기존 기록에서 읽은 frame 크기는 다음과 같다. Forward·CLR은 fixture 행 모양 기준이다. 실제 크기는 Forward 91,304 B, CLR 91,096 B에 행 몫이 더해진 값이며, 큰 행이면 frame 상한 98,304 B까지 커진다.

| frame | 크기 | 출처 |
| --- | ---: | --- |
| RowReserve | 57,336 B | [MI03 한계 기록](module-mi03-limits-20261101.md) |
| Forward | 91,506–91,634 B | `tests/fixtures/module-row-format/scope3/*-forward.wal` |
| CLR | 91,298–91,426 B | 같은 디렉터리의 `*-clr.wal` |

commit된 한 행 거래의 WAL은 적어도 RowReserve와 Forward의 합인 148,842 B다. Begin·Commit 등 나머지 frame은 빼고 셌다. insert 뒤 rollback하는 거래는 CLR이 더해져 적어도 240,140 B다.

**WAL cap.** [`wal_io.rs`](../../crates/engine/src/storage/wal_io.rs)의 `WAL_FILE_CAP`은 256 MiB다. `catalog_live.rs`의 `check_progress_room`(Row·Catalog group의 admission)과 `population.rs`가 이 cap을 넘는 쓰기를 미리 거절하고(Profile1 전용 `live.rs`도 같은 비교를 한다), [`physical.rs`](../../crates/engine/src/storage/wal_io/physical.rs)의 stage도 cap을 넘으면 `Unchanged(Capacity)`로 거절한다. 그래서 재검증 1회는 많아야 약 768 MiB를 읽고 512 MiB를 hash한다. cap만 보면 한 행 commit 거래가 1,803개까지 들어가지만, commit·rollback 거래는 아래 Job cell 상한이 먼저 닿는다.

**epoch 상한(지금 제품).**

- Row Begin은 `begin_selection`에서 `first_catalog_job`으로 Free Job cell 하나(Terminal)를 고른다. Job cell은 64개(`retention_page.rs`의 `JOB_COUNT`)이며, 없으면 `Unchanged(Capacity)`다.
- 행을 넣을 때마다 RowReserve가 같은 64개에서 RootJob cell 하나를 더 빌린다(`row_preflight.rs`가 부르는 `semantic_row/transition.rs`의 `reserve_job`, [소비자 계약](../contracts/dev3-module-consumers.md)의 "Free RootJob").
- 거래가 commit하거나 abort를 끝내도 이 cell들은 Free로 돌아가지 않는다. Forward 뒤 root는 RootRetained, CLR 뒤에는 Releasing으로 남는다. Row의 Prune·RootFree·TerminalFinish는 recovery에서만 돈다. 실행 중 Row 정리(live Row retirement)는 [S9 기록](../contracts/dev3-module-boundaries.md#row-m4-s9)대로 DEV3 기능이며 아직 없다. 예외는 Forward 전의 Cancel이다. `semantic_row/transition/prepare.rs`의 `Change::Cancel`은 RootReserved Job을 `row::free_job`으로 Free로 돌린다([소비자 계약](../contracts/dev3-module-consumers.md)의 "Reserved→Free Job").
- open은 `plan4_recovery.rs`에서 `rebase()`와 `log_open()`을 차례로 해 새 epoch를 만든다. E는 4096 B로 돌아간다. history source의 Base·새 epoch는 이 recovery·활성화에서만 정해지며, 실행 중 전환은 없다.

그래서 commit·rollback으로 끝나는 한 행 거래는 한 epoch에 많아야 32개다(Catalog가 쥔 cell은 더 뺀다). 32개면 E는 하한으로 4.5 MiB(commit)나 7.3 MiB(insert 뒤 full rollback)다. 한 거래 안에서 "INSERT → statement rollback"을 되풀이하면 순환당 cell 하나라 63회, E 하한 약 14.4 MiB까지다. 반면 Reserve→Cancel 순환은 cell을 남기지 않는다. 순환마다 RowReserve 57,336 B와 Cancel 40,000 B([MI03 한계 기록](module-mi03-limits-20261101.md)), 합 97,336 B를 쓰고 preflight에서 재검증을 1회 한다. 그래서 Terminal cell 하나를 쥔 거래 하나로 cap까지 약 2,757회 갈 수 있다. UndoSlot generation, high-water, statement credit 같은 다른 한도가 먼저 닿는지는 확인하지 않았다. Catalog 쪽 거래로 WAL을 늘리는 경우는 조사하지 않았다.

<a id="f6a-locks"></a>
## 5. 잠금과 대기

- `replay_durable`은 WAL File mutex를 `try_lock`으로 잡고 두 번의 hash와 frame 재적용 내내 쥔다. 잡지 못하면 `Unchanged(Busy)`로 돌아간다.
- WAL 기록 경로 `physical.rs`의 `sync_claim`은 같은 mutex를 `lock()`으로 기다린 뒤 write와 fsync를 한다. 그래서 다른 thread가 같은 storage의 WAL을 기록하려 하면 재검증이 끝날 때까지 기다리고, 반대로 기록 중이면 재검증이 Busy로 거절된다. 한 thread만 쓰는 지금 계약([S7 한계: 병렬 writer 범위 밖](../contracts/dev3-module-boundaries.md#row-join-s7))에서는 자기 `ensure_durable`이 먼저 끝나므로 이 대기가 생기지 않는다.
- Row 쓰기 preflight는 호출자가 terminal read gate, RowWrite admission, flush control borrow를 쥔 채 `validate_live`를 부른다(`row_preflight.rs` 머리 주석). 이들의 보유 시간도 재검증 시간만큼 길어진다.
- 읽기의 `replay_lookup_cut`은 먼저 `ensure_durable`로 설치된 cut의 WAL durable을 확보한 뒤 같은 전체 replay를 한다.

<a id="f6a-estimate"></a>
## 6. 분석 추정 (측정 아님)

**모형.** 거래당 재검증 3회(4절 하한), 재검증마다 3E 읽기와 2E hash. k번째 거래의 세 재검증은 모두 E = 4096 + (k−1)·W에서 한다고 둔다(실제 Commit은 그보다 긴 E를 본다). W는 commit 거래 148,842 B, rollback 순환 240,140 B다. 실제 호출 수와 frame은 이보다 크므로 아래는 하한이다.

| WAL 길이 E | 재검증 1회 읽기 / hash | 거래 1개 읽기 / hash (3회) |
| --- | ---: | ---: |
| 1 MiB | 3 / 2 MiB | 9 / 6 MiB |
| 4 MiB | 12 / 8 MiB | 36 / 24 MiB |
| 16 MiB | 48 / 32 MiB | 144 / 96 MiB |
| 256 MiB(cap) | 768 / 512 MiB | 2,304 / 1,536 MiB |

빈 epoch에서 한 행 거래 n개를 차례로 할 때(n은 4절 상한 32 이하):

| 거래 | n | 마지막 E | 누적 hash | 누적 읽기 |
| --- | ---: | ---: | ---: | ---: |
| commit | 16 | 2.3 MiB | 0.10 GiB | 0.15 GiB |
| commit | 32 | 4.5 MiB | 0.41 GiB | 0.62 GiB |
| insert 뒤 full rollback | 32 | 7.3 MiB | 0.67 GiB | 1.00 GiB |

Reserve→Cancel 순환(순환당 97,336 B, 재검증 1회)으로 빈 epoch에서 각 길이에 닿을 때까지의 누적량은 다음과 같다.

| 목표 E | 순환 수 | 누적 hash | 누적 읽기 |
| ---: | ---: | ---: | ---: |
| 1 MiB | 11 | 0.02 GiB 미만 | 0.03 GiB 미만 |
| 4 MiB | 44 | 0.2 GiB | 0.3 GiB |
| 16 MiB | 173 | 2.7 GiB | 4.0 GiB |
| 64 MiB | 690 | 43.1 GiB | 64.7 GiB |
| 256 MiB(cap) | 2,757 | 688.8 GiB | 1,033.2 GiB |

frame 재적용과 census의 CPU 비용은 이 표들에 없다. 시간으로 바꾸는 처리량도 가정하지 않는다.

<a id="f6a-plan"></a>
## 7. 측정 계획 (초안, 실행 전 동결)

측정은 [9절](#f6a-decisions)의 결정을 받은 뒤 별도 단위로 한다. 실행 전에 아래 값을 그 단위의 계획 기록에 동결하고, 바꾸면 새 계획으로 다시 동결한다. 기존 PF04 조건과 COMM C-V18의 5+5쌍 예외는 쓰지 않는다. 이 단계는 baseline만 재며 합격 수치를 정하지 않는다.

### 7.1 질문

- **Q1** 거래 하나가 단계별·입구별로 전체 replay를 몇 번 하는가(H2)? flush가 끼면 몇 번 더 하는가?
- **Q2** 재검증 1회의 읽기·hash byte와 frame 수가 3절의 식(3E, 2E, F)과 같은가(H1)?
- **Q3** E가 다를 때 INSERT 거래 지연과 그 안의 재검증 비중은 어떻게 변하는가(H1·H3)?
- **Q4** 재검증 1회 동안 WAL File mutex 보유 시간과, preflight가 gate를 쥔 시간은 얼마인가(H4)? 다른 thread의 `sync_claim` 대기는 병렬 writer가 계약 밖이라 NOT_RUN으로 둔다.
- **Q5** 같은 E에서 읽기 지연은 어떻게 변하는가? 동시 읽기 없이 수준마다 순차 읽기로 잰다.

### 7.2 단계

| 단계 | 내용 | 실행 형태 |
| --- | --- | --- |
| M0 계수 | Q1·Q2. 시간은 재지 않는다. L3(16 MiB)까지만 간다. debug build에서 L4까지는 frame 재적용이 약 48만 번이라 예산 추정이 없다. L4의 계수는 M1이 함께 기록한다 | memory adapter, cfg(test) 시험, debug build |
| M1 시간 | Q3·Q4·Q5 | native adapter(실제 파일), cfg(test) 시험, release build |

### 7.3 고정 조건과 입력

- **baseline source.** 측정 단위의 main HEAD에 cfg(test) 계측 diff만 더한 것이다. cfg(test) 밖 제품 줄은 바꾸지 않으며, 그 diff를 검토로 확인한다. commit, 계측 diff의 hash, 실행 파일 hash를 기록한다.
- **history source와 epoch.** run마다 새 DB를 만들고 Genesis source에서 시작한다. run 중에는 다시 열지 않는다(다시 열면 새 epoch로 E가 4096 B로 돌아간다).
- **buffer pool.** 시험 기반의 기본 pool frame 수와 flush 정책을 그대로 쓰고 그 값을 기록한다. 측정 거래 시작마다 상주 상태, `FlushRequired` 횟수, flush 수를 기록한다.
- **Job 예산.** 시작 때 Free Job cell 수 J를 기록한다. commit·rollback 거래마다 Terminal 1개, 넣은 행마다 RootJob 1개를 쓴다고 세고(Cancel한 예약은 세지 않는다), 모든 거래의 합이 J를 넘지 않게 짠다.
- **WAL 늘리기.** 수준 사이마다 build 거래 하나를 둔다. build 거래는 "BEGIN → (같은 table에 행 1개 Reserve → Cancel)을 N회 → full rollback"이다. 기존 `row_cancel.rs`의 Reserve→Cancel 흐름을 쓴다. table의 행은 바뀌지 않고, retained root와 그 정리 부채는 남지 않으며, cell은 build 거래마다 Terminal 1개만 쓴다. abort로 끝난 build 거래의 Terminal은 자기 정리 몫을 남긴다. 다만 UndoSlot generation과 high-water는 순환마다 오르고 build 거래마다 Aborted Status cell이 남는다. 수준마다 이 값과 Job·Status 점유를 기록한다.
- **수준.** L0은 빈 epoch에서 warmup 거래 하나를 버린 직후다(실제 E 기록, 0.25 MiB 안팎). 그 뒤 측정 거래 시작 시점의 E가 L1 ≥ 1 MiB, L2 ≥ 4 MiB, L3 ≥ 16 MiB, L4 ≥ 64 MiB에 처음 닿을 때 잰다. 실제 E를 기록한다. L5(cap 근처 256 MiB)는 누적 hash가 약 0.7 TiB라(6절) 빼 둔다. 실제로는 Reserve admission이 앞으로 쓸 WAL 몫(MI03 기록으로 약 1.04 MB)까지 보므로 cap 직전 몇 순환은 거절된다. 넣으려면 따로 시간 예산을 승인받는다.
- **측정 거래.** 각 수준에서 "BEGIN → INSERT 1행 → COMMIT"을 3번 연속 한다. 거래마다 cell 2개라 수준 5개면 cell 30개, build 거래 4개가 4개, warmup이 2개로 합 36개를 쓰고 행 15개(warmup 포함 16개)가 늘어난다. 이어서 같은 table의 행 하나를 순차로 한 번 읽는다(Q5).
- **시험 기반.** 기존 `plan4_recovery/tests/catalog_memory/`의 `row_commit.rs`·`row_rollback.rs`·`row_live.rs` 흐름으로 새 C 시험 파일 하나를 만든다. 30초 사례 한도 때문에 `#[ignore]`와 명시 selector로 돌리고, module gate의 시험 소유(C)에 등록한다. M1의 native 실행은 기존 native worker처럼 ignored worker와 controller, 독립 admission을 둔다([기존 기록](main-preexisting-failures-20261030.md)). 기존 시험과 [후속 계획의 기존 실패 파일](../plans/module-independence-followup-plan.md#failures)(`lifecycle_tests.rs`, `free_undo_tests.rs`)은 바꾸지 않는다.

### 7.4 관측값과 계수기

| 관측 | 단위 | 지금 있는 것 | 새로 필요한 것 |
| --- | --- | --- | --- |
| replay 호출 수(단계·입구별) | 회 | `history::runtime::test_replay_scopes()`, `test_replay_checkouts()`. scope·checkout을 쓰는 입구(`replay_lookup_cut` 포함)만 센다 | `replay_durable` 자체의 계수와 입구 tag. `exclusive()` 입구(`load.rs`, `population.rs`, `recovery.rs`)와 flush workspace를 쓰는 `prove_capture`는 지금 세지 않는다 |
| 읽기·hash byte, frame 수 | B, 개 | 없음 | `replay_durable`·`hash_prefix`·`reconstruct`의 cfg(test) 계수 |
| 거래 단계별 시간(주 지표: BEGIN→COMMIT 전체) | ns | 없음 | 시험 쪽 timer(Begin, preflight, reserve~apply, finish, commit) |
| File mutex 보유, preflight의 gate 보유 | ns | 없음 | `replay_durable` 보유 구간 cfg(test) timer, preflight 구간 timer |
| 읽기 지연 | ns | 없음 | 시험 쪽 timer와 그 읽기의 replay 계수 |
| 수준별 WAL 기록량, Job·Status 점유 | B, 개 | `WalPrefix`, control 사본 | 시험 쪽 기록 |

새 계수기·timer는 모두 `#[cfg(test)]`로 두고 제품 동작과 크기 census를 바꾸지 않는다. `history.rs`는 C(DEV3) 소유 파일이라 추가 전에 9절의 승인이 필요하다.

### 7.5 환경

- M0는 결정적이라 기계를 가리지 않는다. 클라우드에서 돌려도 된다.
- M1은 한 기계에 고정한다. CPU 모델·core 수, kernel, filesystem과 장치, 여유 disk, Rust toolchain, build profile, CPU governor, 시작 시 load average를 결과에 기록한다.
- release build로 잰다. debug build는 workspace crate가 opt-level 0이라 CPU 비용이 왜곡된다(`Cargo.toml`은 sha2만 opt-level 3).
- page cache는 warm 상태 그대로 잰다(방금 쓴 WAL). cold cache는 root 권한의 cache 비우기가 필요해 범위 밖이다.

### 7.6 반복과 통계

- M0: 같은 입력으로 2회 돌려 모든 계수가 같은지 본다.
- M1: 새 DB로 독립 run 10회. L0 앞의 warmup 거래 1개는 버린다. 수준마다 측정 거래 3개의 중앙값을 run 대표값으로 쓴다. 10개 run 대표값의 중앙값, p90, 최댓값과 중앙값의 bootstrap 95% 신뢰구간을 낸다. 재검증 시간 비중과 E에 대한 기울기도 함께 낸다.
- 수준별 run 대표값의 변동계수가 20%를 넘으면 10 run을 한 번 더 한다. 그래도 넘으면 그 수준은 "결론 없음"으로 기록한다.
- F6-b에서 후보를 만들면 그때 DP07대로 baseline/candidate를 번갈아 최소 10쌍 잰다. 이 baseline 결과를 그 비교에 재사용하지 않는다.

### 7.7 시간·disk 예산

- M0: L3까지 전체 30분 이내(추정 근거 없음. 넘으면 L2에서 멈추고 기록한다). M1: run 하나 30분 이내(빈 epoch에서 L4까지 build 누적 hash 약 43 GiB, 6절), 수준 하나를 만드는 데 15분 이내, 첫 10 run 5시간 이내, 변동 때문에 10 run을 더 해도 전체 10시간 이내.
- disk: DB 하나는 WAL 약 70 MiB 이하와 data·control 몇 MiB다. run이 끝나면 지운다. 동시에 하나만 두며 전체 1 GiB를 넘지 않는다. 30GB 기준 아래라 클라우드에서도 허용 범위다. 다만 기존 native 검사의 "여유 60 GiB" 전제를 그대로 쓰면 그 시험은 로컬에서만 돈다.

### 7.8 무효·중단 기준

- **중단:** fatal·fence, 예상 밖 거절(build와 측정 거래 모두에서 Busy·Capacity 포함; build 중 Capacity는 확인하지 않은 한도에 닿았다는 뜻이므로 그 사실과 위치를 기록한다), 시간·disk 예산 초과, 여유 disk 2 GiB 미만. 부분 결과는 그 사실과 함께 남긴다.
- **무효:** 동결한 commit·계측 diff·실행 파일과 실제가 다름, M0 두 run의 계수 불일치, run 중 reopen, 측정 중 다른 무거운 작업의 개입(시작 load average 기록으로 판단).
- 무효 run은 지우지 않고 무효 사유와 함께 남긴다. 결과를 덮어쓰지 않는다.

### 7.9 산출물

run별 JSON(commit, 환경, 수준별 E·기록량·점유, 계수, 시간)과 원 log, 집계 Markdown 하나. 이 문서에 결과 절을 더하고, 원 결과 파일은 별도 evidence 디렉터리에 둔다.

<a id="f6a-candidates"></a>
## 8. F6-b 후보에 대한 정적 메모

F6-b는 측정이 병목을 확인한 경우에만 연다. 여기서는 설계하지 않고, 코드에서 읽은 제약만 적는다.

1. **현 전체 검증 유지.** 측정 비용이 허용 범위면 이 안으로 끝낸다.
2. **base·checkpoint 전환으로 재생 구간 축소.** Base와 새 epoch는 recovery에서만 만들어지고 실행 중 전환은 없다. 이 안은 새 epoch 공급, 즉 DEV3 기능에 의존한다.
3. **검증된 상태를 보유하고 새 구간만 반영.** 지금의 두 번 hash는 "읽는 동안 prefix가 바뀌지 않았다"를 증명한다. 이전 prefix를 다시 읽지 않으면 그 증명이 빠진다. 그래서 무효화 조건(epoch·LSN·offset·generation·fence·namespace, truncation, writer 교체, 실패·UNKNOWN, 외부 변경)을 계약으로 정하는 D2가 먼저 필요하다. frame마다 하는 전체 census를 줄이는 것도 같은 수준의 계약 변경이다.

2와 3은 C(DEV3) 제품 코드를 바꾸므로 사용자 승인과 DEV3 담당과의 조율이 먼저다. 또 4절대로 commit·rollback만 하는 workload는 Job cell 상한 때문에 한 epoch의 WAL이 짧게 머문다(추정). 긴 WAL은 Cancel이 많은 workload에서 생기고, live Row retirement가 공급되면 일반 workload에서도 생긴다.

<a id="f6a-decisions"></a>
## 9. 측정 전에 필요한 결정

1. 측정 시기: 지금 측정할지, live Row retirement(DEV3)가 공급되어 일반 workload의 WAL이 길어질 수 있게 된 뒤로 미룰지. 지금도 Reserve→Cancel build로 긴 WAL을 만들어 잴 수는 있지만, commit·rollback만 하는 workload의 WAL은 Job cell 상한 때문에 몇 MiB 수준에 머문다(추정). **2026-11-02 06:44 UTC 사용자 결정: live Row retirement 공급 뒤로 미룬다.** 2–6은 측정을 다시 열 때 정한다.
2. 측정 승인과 범위: M0만, 또는 M0+M1. **2026-11-02: 사용자가 "이어서 해주세요"라고 한 뒤, 이 thread가 결정 카드(M0만 지금 / M0 다음 M1 / 자동 정리 호출자 먼저)로 물었다. 답을 기다리는 동안 추천안 "M0만, 클라우드"로 진행했다([12절](#f6a-m0)). 사용자는 2026-11-02 14:15 UTC에 "M0만 지금 (클라우드)"를 골랐고, M0는 PR #37로 병합됐다(main `372b22e`). M1은 하지 않는다.**
3. C(DEV3) 소유 파일 `history.rs`에 cfg(test) 계수기·timer를 넣는 것과 그 담당. **M0는 이 thread가 cfg(test) 계수기만 넣었다(timer 없음). 위 카드의 추천안 설명에 포함했다.**
4. M1 기계: 사용자 로컬 기계(시간 측정 안정성 때문에 권장) 또는 고정한 클라우드 환경.
5. (M0는 L0–L3과 Reserve→Cancel build를 계획대로 쓰고, 다른 점은 12.1절에 적었다.) 수준 L0–L4(64 MiB까지, cap 근처 L5 제외)와 Reserve→Cancel build 방식, 7.7의 시간·disk 예산, 그리고 계획 §10의 "읽기 지연"을 동시 읽기 대신 순차 읽기로 재는 것.
6. 병목 판단 기준. 제안: L3·L4에서 재검증 시간이 측정 거래 지연의 절반 이상이고, 거래 지연이 E에 거의 선형(회귀 R² ≥ 0.9)이면 "병목 확인"으로 본다. 합격 수치가 아니라 F6-b를 열지 정하는 기준이며, 사용자가 확정하기 전에는 쓰지 않는다.

<a id="f6a-limits"></a>
## 10. 한계

- 12절 앞의 수치는 모두 정적 하한이나 분석 추정이다. 실제 호출 수와 frame 크기는 12절의 M0가 셌고, frame당 CPU 비용과 시간은 아직 재지 않았다.
- 거래당 3회는 코드에서 조건 없이 도는 것만 센 하한이다. statement 종료 group이 모든 INSERT에 붙는지는 정적으로 확정하지 않았다. (12절 M0: 한 행 commit 거래는 4회였고, statement 종료는 replay도 frame도 없었다.)
- Reserve→Cancel build도 상태를 완전히 고정하지는 못한다. UndoSlot generation·high-water와 build 거래의 Status가 바뀐다. 측정 거래가 남기는 Job·Status 점유도 수준마다 쌓인다. (12절 M0는 거래마다 live 정리를 해서 Job 점유가 쌓이지 않았다. 모든 수준에서 Free 62였다.)
- Job cell 밖의 한도(UndoSlot generation, high-water, statement credit, Undo INIT의 owned page)가 먼저 닿는지는 확인하지 않았다. 닿으면 4절의 도달 범위와 L4가 낮아진다.
- Catalog 쪽 거래로 한 epoch의 WAL을 늘리는 경우는 조사하지 않았다.
- 측정 계획은 SQL 층이 아니라 저장소의 Row 쓰기 시험 경로를 쓴다. cold cache와 여러 writer의 동시 경쟁은 이 계획 밖이다.

<a id="f6a-review"></a>
## 11. 독립 검토

작성은 Claude thread session이고, 독립 검토는 같은 session이 띄운 별도 읽기 전용 검토자다. 검토자는 문서의 사실 주장을 `d86bafb` 코드와 기존 기록에 대조하고 산식을 다시 계산했다.

- 1차: ACCEPT WITH REQUIRED CHANGES(필수 5, 권고 12). 측정값으로 보이는 서술은 없었고, 산식과 링크는 모두 맞았다.
- 필수 반영: `prove_capture`(flush) 입구 추가, Job cell 64개·live Row retirement 부재·reopen의 새 epoch 때문에 L3 16 MiB·L4 64 MiB와 n ≤ 1,803 표가 도달 불가인 점, Genesis·reopen 없음·buffer pool·baseline source 동결 조건, Q4의 단일 thread 한계와 gate 보유 관측, `system_target`·`exclusive()` 입구·`replay_lookup_cut` 계수 정정.
- 권고 반영: stage의 cap 거절 위치, 행 모양별 frame 크기, 6절 모형 명시, MI03 관찰 문장 삭제, 실패 파일 링크 문구, 추가 10 run을 포함한 예산, `#[ignore]`·selector·시험 소유 등록, native controller, 주 지표와 수준별 기록량, 순차 읽기를 결정 항목으로, D03 카드 시각, 색인 요약.
- 2차(재확인): ACCEPT WITH REQUIRED CHANGES(필수 1, 권고 5). 1차 필수 5건은 해결로 확인했다. 새 필수는 행마다 RootJob cell을 하나 더 쓰므로 한 행 거래 상한이 64가 아니라 32이고, 16 MiB 불가는 추정으로 써야 한다는 점이었다. 4·6·7.3·9·10절과 색인 요약을 고쳤다. 권고(6절 반올림, `live.rs`의 Profile1 전용, flush 함수 이름, 큰 행의 frame 크기)도 반영했다.
- 3차: ACCEPT WITH REQUIRED CHANGES(필수 2). 2차 필수는 해결로 확인했다. 새 필수는 Forward 전 Cancel이 RootJob을 돌려주므로 Job cell이 WAL 증가를 막지 못한다는 점과, build 중 Capacity의 처리가 중단 기준과 어긋난다는 점이었다. H3·4·6·7.3·7.6–7.8·8·9·10절과 색인 요약을 고쳐 build를 Reserve→Cancel로 바꾸고 L3 16 MiB·L4 64 MiB를 되살렸다. 권고(warmup 뒤 L0)도 반영했다.
- 4차: ACCEPT(열린 필수 0). 3차 필수 2건의 해결과 6절 build 표·7.3 cell 수를 다시 계산해 확인했다. 권고(cap 행을 2,757 순환으로, build 거래 Terminal의 정리 몫, 색인 요약의 단서, M0를 L3까지로 제한)를 반영했다.

<a id="f6a-m0"></a>
## 12. M0 결과: 재검증 횟수와 양 (2026-11-02)

**상태: M0 실행 · 두 run의 계수 일치 · 합격 수치 없음 · M1(시간) NOT_RUN · PR #37로 병합(main `372b22e`), 사용자 결정 "M0만".** live Row 정리가 PR #36(main `8fa96c5`)으로 병합되어 9절 1번의 보류 조건이 풀렸다. 사용자가 "이어서 해주세요"라고 해서, 이 thread는 M0를 다음 단위로 골랐다. 2026-11-02 결정 카드로 범위(M0만, M0+M1, 자동 정리 호출자 먼저)를 물었고, 답을 기다리는 동안 추천안 "M0만, 클라우드"로 진행했다(9절 2·3·5번). 시간은 재지 않았다. 아래 값은 모두 계수기가 센 값이거나 그 값으로 계산한 것이다.

<a id="f6a-m0-setup"></a>
### 12.1 실행 조건

- **source.** main `8fa96c5`에 cfg(test) 계측과 시험만 더했다. 제품 줄은 바꾸지 않았다. 계측은 [`history.rs`](../../crates/engine/src/storage/wal_io/plan4_retention/history.rs)의 `revalidation` 모듈이다. 세 입구(`validate_live`, `replay_lookup_cut`, `prove_capture`)가 `replay_durable`을 부르기 직전에 자기 이름을 남긴다. `replay_durable`은 들어오자마자 그 이름을 가져가고(거절되거나 이름 없는 호출이 이전 이름을 물려받지 않게), WAL File을 잡고 header를 읽은 뒤 그 이름으로 한 번을 센다. 함께 세는 것은 E, E까지의 frame 수(LSN − anchor), 실제로 읽은 WAL byte(header, hash chunk마다, frame마다), chunk마다의 SHA-256 입력 byte, 재적용한 frame 수다. 빈 epoch에서 seed만 하는 입구 호출은 검사를 통과한 뒤 seed로 따로 센다. 시험은 [`plan4_pages::apply::test_revalidations`](../../crates/engine/src/storage/wal_io/plan4_apply.rs)로 thread의 누적값을 읽고, 단계 앞뒤의 차이를 기록한다.
- **시험.** [`row_join/retire/revalidation.rs`](../../crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/revalidation.rs)의 `f6_m0_levels_to_l3`(`#[ignore]`, 명시 selector로 실행)과 CI가 도는 짧은 확인 `f6_m0_step_counts_match_the_contract_and_repeat`다. 짧은 확인은 warmup, L0 거래 하나, 2 MiB까지의 작은 build, 거래 하나를 두 번 돌려 줄이 같은지, 단계별 0회·1회 이상(S2W-R22, 4절)과 build 순환당 1회를 확인한다. 실행 명령과 run 기록은 [원 결과](f6-m0-20261102.json)에 있다.
- **환경.** memory adapter, debug build, 클라우드 container(4 vCPU, Intel Xeon 2.10GHz, rustc 1.95.0). run마다 새 디렉터리의 Genesis epoch에서 시작하고 다시 열지 않는다.
- **run.** commit `b9f5603`에서 만든 시험 실행 파일(sha256 `62c9df6b…`, 계측 diff `git diff 8fa96c5 b9f5603 -- crates/`의 sha256 `02395dfd…`)로 2026-11-02 11:21–11:27 UTC에 두 run을 별도 process로 동시에 돌렸다(각 약 360초). 두 run의 결과 줄 20개가 모든 필드에서 같았다. 그 전에 검토 전 실행 파일로 한 시운전 1회도 warmup 줄의 시작 WAL 기록 오류(검토 1차 지적)를 빼고 같았다.
- **입력.** 표 T(Int64 열 하나)를 만든 뒤 warmup 거래 하나를 하고 L0을 잰다. 수준마다 "BEGIN → INSERT 1행 → COMMIT"을 3번 하고, 거래마다 바로 그 live 정리(Prune, ReleaseRole×3, RootFree, TerminalFinish)를 끝까지 한다. 수준 L1 ≥ 1 MiB, L2 ≥ 4 MiB, L3 ≥ 16 MiB는 측정 거래가 시작할 때의 WAL 길이로 판단한다. 모자라면 그 앞에 build 거래(BEGIN → Reserve→Cancel 반복 → full rollback)를 하고 그 Terminal을 live TerminalFinish로 정리한다.

**7절 계획과 다른 점.**

1. 측정 거래마다 live 정리 6그룹을 더해 그룹별로 셌다. 계획을 쓸 때는 live 정리가 없었다. 지금은 일반 workload의 거래가 정리까지 가야 Job cell을 돌려받는다.
2. pool frame을 기본 9가 아니라 16으로 했다. 정리는 frame 11개 이상이 있어야 돈다([live Row 정리 기록](live-row-retirement-20261102.md)의 LRR-06). live Row 정리 시험과 같은 값이다.
3. 시험 소유는 C가 아니라 JOIN(`row_join/**`)이다. A의 `validate_insert`·`produce_insert`와 C의 정리를 함께 쓰기 때문이다.
4. L0이 계획 추정(0.25 MiB 안팎)보다 길다. 표를 만든 직후 WAL이 615,142 B, warmup 뒤 1,244,505 B다. 측정 거래 하나가 정리까지 629,363 B를 더하므로 L1과 L2는 측정 거래만으로 넘었다. 그래서 L1·L2는 build 없이 그 시점에서 쟀고, build는 L3 앞에서 한 번(101 순환)만 했다.
5. 읽기 지연(Q5)과 시간(Q3·Q4)은 M1 항목이라 넣지 않았다. L4는 계획대로 M0에서 뺐다.
6. 7.3절의 기록 중 Status 점유, UndoSlot generation·high-water, 측정 거래 시작 때의 상주 상태는 기록하지 않았다. Job cell 점유와 flush 수(`FlushRequired`가 난 단계 이름)만 기록했다. table 발급 안의 flush와 build의 Begin·rollback 단계 안의 flush는 그 단계에 함께 들어간다(입구별 `capture` 수로는 구분된다). 이번 run에서는 flush가 없었다.

<a id="f6a-m0-q1"></a>
### 12.2 Q1: 거래 하나가 하는 전체 replay

warmup과 L0–L3의 측정 거래 13개가 모두 같았다. flush는 한 번도 없었다(frame 16에서 preflight·정리 모두 `FlushRequired`가 없었다). 그래서 "flush가 끼면 몇 번 더 하는가"는 이 조건에서 관측하지 못했다.

| 단계 | 입구 | 전체 replay |
| --- | --- | ---: |
| Begin group | `validate_live` | 1 |
| statement 시작 | — | 0 |
| table 발급(`catalog_statement_call` → `finish_table`) | `replay_lookup_cut` | 1 |
| 쓰기 preflight | `validate_live` | 1 |
| validation lane, reserve, producer lane, prepare, apply | — | 0 |
| statement 종료 | — | 0 |
| Commit group | `validate_live` | 1 |
| **거래 합** | | **4** |
| Prune, ReleaseRole×3, RootFree, TerminalFinish | 각 `validate_live` | 각 1, 합 6 |
| **정리까지 합** | | **10** |

4절의 정적 하한은 3회였다. 차이는 table 발급의 `replay_lookup_cut` 1회다. 계수기는 `replay_lookup_cut`의 호출 위치를 구분하지 않으므로, 이것이 2절 표의 `read/lookup.rs` 행이라는 것은 추정이다. reserve·prepare·apply가 replay하지 않는 것은 [S2W-R22](../contracts/dev3-module-boundaries.md#row-write-validate-once)와 같다. live 정리는 거래가 쓴 replay의 1.5배를 더 쓴다.

build 거래는 Begin·statement·table 발급 구간에서 2회(live 1, lookup 1), Reserve→Cancel 순환마다 1회(101 순환에 101회), statement 종료와 full rollback(시작, 끝, `finish_aborted`)을 묶은 구간에서 2회, TerminalFinish에서 1회를 썼다. 순환 안에서는 preflight만 replay한다는 것(Reserve·Cancel은 0)은 S2W-R22와 측정 거래의 단계별 값에서 온 추정이다. build의 순환은 한 구간으로 셌다.

<a id="f6a-m0-q2"></a>
### 12.3 Q2: 재검증 한 번의 양

모든 구간에서 읽기 = 3E, hash = 2E, 재적용 frame = E까지의 LSN 수였다. 다만 이 등식은 계수기가 3절이 말하는 자리에 붙었는지를 확인할 뿐이다. `reconstruct`가 끝 offset과 LSN을 스스로 검사하므로, 등식이 깨지는 replay는 제품이 먼저 거절한다. 그래서 독립 측정은 아니다. 스스로 실패할 수 있는 확인은 따로 있다. 모든 측정 거래에서 Begin과 쓰기 preflight의 replay가 읽은 E는 그 시점의 WAL 파일 길이와 같았다. 즉 한 번의 재검증이 WAL 전체를 처음부터 끝까지 다시 읽는다(H1).

측정 거래 하나와 그 정리(replay 10회)의 양은 시작 WAL 길이 E₀에 대해 정확히 다음과 같았다. 거래 안의 10회가 E₀ 뒤 0, 56,712, 56,712, 205,545, 262,257, 352,139, 407,667, 463,195, 518,723, 574,251 B에서 일어났기 때문이다.

- 읽기 = 30·E₀ + 8,691,603 B, hash = 20·E₀ + 5,794,402 B, 재적용 frame = 10·F₀ + 44(F₀는 E₀까지의 frame 수)
- 정리를 빼면(거래의 4회) 읽기 = 12·E₀ + 956,907 B

| 수준 | 첫 측정 거래의 E₀ | 거래+정리 읽기 | hash | frame 재적용 |
| --- | ---: | ---: | ---: | ---: |
| L0 | 1,244,505 B | 46.0 MB | 30.7 MB | 274 |
| L1 | 3,132,594 B | 102.7 MB | 68.4 MB | 574 |
| L2 | 5,020,683 B | 159.3 MB | 106.2 MB | 874 |
| L3 | 16,911,284 B | 516.0 MB | 344.0 MB | 3,234 |

<a id="f6a-m0-growth"></a>
### 12.4 WAL 증가와 점유

- 한 행 commit 거래 262,257 B: Begin 56,712, RowReserve와 Forward 합 148,833, Commit 56,712 B. 4절이 fixture 행으로 잡은 RowReserve+Forward 148,842 B보다 9 B 작다. 행 모양이 달라서다.
- 그 정리 367,106 B: Prune 89,882, ReleaseRole 55,528×3, RootFree 55,528, TerminalFinish 55,112 B. 정리가 거래 자체의 1.4배다.
- build 순환은 정확히 97,336 B(4절의 RowReserve 57,336 + Cancel 40,000)씩 WAL을 늘렸다. 순환들의 replay E 합 1,195,060,684 B = 101 × 6,965,484 + 5,050 × 97,336이다. build 거래 전체는 Begin·rollback·정리 frame까지 10,002,512 B를 늘렸다.
- 측정 끝 WAL 18,799,373 B, 파일 합 40,864,525 B다. Job cell은 시작과 모든 수준에서 Free 62, Row 0, 그 밖 2였다. 정리가 매번 Row Job을 비웠다.

<a id="f6a-m0-meaning"></a>
### 12.5 무엇을 말하는가 (분석)

- 정리까지 하는 한 행 거래의 비용은 E에 선형이다(읽기 30E). 한 epoch에서 이런 거래를 이어 하면 E가 거래마다 629,363 B씩 늘므로, n개의 누적 읽기는 약 30·629,363·n²/2 ≈ 9.4 MB·n²다. 예를 들어 빈 epoch 뒤 32개면 약 10 GB, 100개면 약 94 GB를 읽는다(추정; 앞의 표 생성분은 뺐다).
- 정리를 commit마다 바로 하면, 정리 없는 거래와 비교해 거래당 replay가 2.5배(4 → 10), 거래당 WAL 증가가 2.4배(262,257 → 629,363 B)다. 누적 읽기의 n² 계수는 둘의 곱으로 약 6배가 된다. 지금 계약(정리 그룹마다 replay 1회)에서는 정리를 모아 해도 그룹 수가 같으므로 replay 횟수는 그대로다. 미루면 거래 쪽 replay의 E는 작아지지만, 미룬 정리 replay의 E는 그만큼 커지므로 n² 계수는 거의 같다(분석). 그래서 자동 정리 호출자의 호출 시점은 누적 재검증 양을 크게 바꾸지 못한다(분석). 그 양이 실제 시간으로 문제인지는 아래와 같이 아직 판단하지 않는다.
- 이것이 병목인지는 아직 말하지 않는다. 시간(Q3·Q4)은 M1이 재고, 판단 기준(9절 6번)은 사용자가 정한다.

<a id="f6a-m0-limits"></a>
### 12.6 M0의 한계

- 계수만 셌다. 시간, File mutex 보유, gate 보유, 읽기 지연은 M1이다.
- 한 행 INSERT의 commit만 쟀다. 여러 행 거래, rollback으로 끝나는 측정 거래, UPDATE·DELETE, Catalog 거래는 재지 않았다.
- frame 16에서는 flush가 없었다. frame이 적거나 page가 많은 workload의 flush·LOAD 추가분은 관측하지 못했다.
- memory adapter라 실제 파일 I/O는 없다. 계수는 adapter와 무관하지만 시간은 다르다.
- 두 run은 같은 실행 파일로 했다. 계수가 같다는 것은 결정성 확인이지 다른 기계·build에서의 재현은 아니다.

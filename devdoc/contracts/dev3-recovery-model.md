# DEV3 WAL·CLR·복구 예약 유한 모델

2026-10-20 · **D3-M04-F01 / 실행 전 동결 초안 / 실행·독립 검토 미완료**

[복구 계약](disk-storage-design.md#recovery) · [예약 복원](disk-storage-design.md#recovery-reservation) · [모델 인수](../plans/dev3-implementation-plan.md#state-model) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

## 범위와 고정 이력

두 writer A/B, reader 하나, maintenance actor 하나를 사용한다. 두 행·하나의 논리 index에 key0/key1/NULL을 둔다. A는 winner이며 B는 두 변경을 가진 loser다. 여덟 page 역할은 rowA, rowB, index, undo, status, allocator, retirement queue, checkpoint metadata다. undo/status 각 네 slot을 두 owner의 과거·현재 상태로 사용하며 generation0/1을 구분한다. page home은 pageLSN과 payload를 가진 별도 durable image다. 모델의 frames3은 group 작업의 고정 workspace token 카운터이며 실제 page별 buffer pin mapping을 나타내지 않는다.

기본 이력은 `A rowA:key0→key1 commit`, `B rowB:NULL→key0→key1`이다. A의 ACK는 보존해야 하며 B의 두 CLR 뒤 rowB/index는 NULL이어야 한다. checkpoint는 BEGIN WAL보다 뒤에서 시작하는 fixture도 포함한다. BEGIN 존재를 recovery undo의 필수 입력으로 삼지 않는다. 논리 기대값·남은 undo 횟수는 고정 owner 이력에서 계산하고 candidate reservation ledger를 정답으로 사용하지 않는다.

`clr_chain`은 두 B undo가 남은 capture에서 시작한다. `partial_clr`는 마지막 B undo가 이미 완결 CLR로 채택됐지만 일부 home이 이전 LSN인 상태다. `cut_queue`는 A의 terminal undo 두 slot을 queue가 소유하며 cut/count/cursor의 한 그룹 인계를 검사한다. `commit_capture`는 A의 commit record가 append되었으나 status/frontier/ATT·DPT capture가 끝나지 않은 상태다. `begin_removed`는 BEGIN 없는 checkpoint가 B의 head·두 undo·잔여 descriptor를 인계한다.

`partial_clr`는 첫 crash의 BOOT/ANALYSIS/RESTORE/REDO/REBASE/WRITER를 마친 뒤 CLR 하나를 append/apply/sync하고 rowB home만 flush한 도달 가능 cut이다. 시작 crash count는1, writer_selected=true다. 두 번째 crash를 depth1부터 탐색하며 시작 전 bootstrap을 이번 trace의 실행 횟수로 다시 세지 않는다. 다른 fixture의 crash count는0이다. `begin_removed`는 Restart, 나머지는 runtime rollback/maintenance가 가능한 상태로 시작한다.

| fixture | 고정 후속 group 순서 | 필수 witness |
| --- | --- | --- |
| clr_chain | CLR-last, CLR-first, abort-end | append/apply gap, CLR atomic apply, page별 home flush, checkpoint capture, READY의 winner/loser 결과, crash2 |
| partial_clr | 이미 채택한 CLR-last 뒤 CLR-first, abort-end | recovery WRITER 아래 CLR, 부분 CLR 뒤 두 번째 crash, 남은 예약3, READY |
| cut_queue | cut, release-slot0, release-slot1 | cut/count/queue 원자 인계, cursor1/2, slot0/1 generation1, home flush, crash2 |
| commit_capture | commit-A, CLR-last, CLR-first, abort-end | commit sync 전 capture 차단, status/ATT 반영 뒤 capture, winner ACK, crash2 |
| begin_removed | CLR-last, CLR-first, abort-end | BEGIN 없이 analysis, BOOT_RESERVE, RESERVE_RESTORE, page별 redo, restore 뒤 crash |

## 기록·byte·예약

forward·CLR·cut/release·terminal commit/abort를 완결 group 종류로 구분한다. 하나의 group은 prepare/apply 사이에 workspace token3개를 명시 보유한다. home flush·redo는 token 하나를 잠시 사용하며 group token과 중첩하지 않는다. 하나의 group은 논리 effect, transaction head/progress, 명시 edge와 stored count, queue cursor, ATT/DPT, residual obligation을 함께 바꾼다. candidate stored count는 별도의 edge 목록에서 다시 계산한 incoming count와 대조한다. 순차 page flush로 home이 서로 다른 LSN인 것은 허용하지만 recovery REDO 완료 시 같은 채택 prefix로 수렴해야 한다.

자원은 실제 retained WAL/header bytes, 이미 할당한 file bytes, 기존 extent의 owned page token, outstanding growth, 잔여 CLR/abort-end 예약, volatile runtime ledger를 별도 필드로 둔다. 작은 수치는 단위화한 모델 비용이며 제품 F05의 실제 byte 산식을 대신하지 않는다. 각 undo마다 CLR2단위·page token1개, abort-end1단위, epoch header1단위, bootstrap fixed2단위를 사용한다. 초기 WAL 실제4단위·owned file8단위, hard WAL16단위·owned16단위·protected8단위다. 기존 page token을 owned bytes에 다시 더하지 않으며 소비한 CLR bytes를 outstanding으로 되돌리지 않는다.

## action과 crash

| action | 의미 |
| --- | --- |
| append_group | 사전 예약과 owner/head 확인 뒤 complete WAL record append; 아직 live 적용 전 |
| prepare_group_frames | append된 미적용 group에 row/index/metadata 작업 frame3개를 획득. apply 또는 crash가 해제 |
| apply_group | WAL group의 모든 논리 effect/progress/descriptor·ATT/DPT를 한 번에 live 적용 |
| wal_sync | 완결 prefix를 durable로 보존 |
| home_flush(page) | 해당 pageLSN까지 WAL sync 후 page 단위 home image 보존; M05의 검증된 DWB 장벽을 추상 전제로 사용 |
| checkpoint_capture | append/apply gap·CommitAttempted가 없음. 같은 LSN의 ATT/DPT/owner/descriptor를 복사 |
| checkpoint_select | capture bytes/control sync. BEGIN 삭제는 이 closure와 needed redo를 유지한 뒤 가능 |
| crash(prefix) | crash<2, synced prefix 이상인 완결 prefix 중 하나를 선택. 모든 volatile ledger/frame·live 상태 소멸 |
| boot_reserve | home allocator/undo를 읽기 전에 고정 bootstrap2단위를 확보 |
| analysis | checkpoint+선택 WAL의 owner/head/잔여 descriptor를 재구성 |
| reserve_restore | 같은 prefix의 descriptor와 ownership을 검산해 recovery owner에게 한 번 인계 |
| redo(page) | 페이지별 LSN을 검사하며 채택된 group의 해당 page effect를 replay. 여덟 page를 모두 끝내기 전 UNDO 진입 금지. 예약은 재차 차감하지 않음 |
| rebase | 현재 prefix를 새 base closure로 인계하고 이미 소유한 header 공간을 회수. 실제 두-slot selector graph는 M06 별도 정본 |
| writer_select | header byte를 쓰기 전에 charge, durable WRITER 선택 뒤만 CLR 허용 |
| verify_ready | winner 보존, loser terminal, row/index 일치·edge/count·예약 확인 뒤 reader 관측 |

append/apply/WAL sync/home flush와 checkpoint capture를 합치지 않는다. crash는 중간 home 혼합과 미동기 완결 prefix의 old/new 효과를 펼친다. 일부 CLR 뒤 두 번째 crash·BOOT_RESERVE 뒤 crash·예약 복원 뒤 crash·WRITER 선택 뒤 crash를 witness로 요구한다. 고정 cut fixture는 전체 파이프라인을16단계 안에 억지로 합치는 대신 실제 도달 가능한 중간 상태를 시작점으로 둔다.

## 불변식·mutants·한계

| mutant enum | fixture | 기대 오류 |
| --- | --- | --- |
| progress_without_effect | clr_chain | clr_effect_progress |
| count_without_cut | cut_queue | edge_count |
| release_without_cursor | cut_queue | queue_release_once |
| omit_commit_status | commit_capture | commit_status |
| capture_missing_dpt | clr_chain | capture_lsn_closure |
| zero_restored_ledger | begin_removed | reservation_restore |
| reuse_consumed_clr | partial_clr | remaining_obligation |
| stale_home_ownership | begin_removed | reservation_restore |
| redo_double_charge | begin_removed | remaining_obligation |
| clr_before_writer | begin_removed | writer_not_selected |

CLR의 progress만 전진하면 필요한 row/index effect 유실로 거절한다. count/cut 또는 cursor/release를 분리하면 edge count 또는 exactly-once release 위반이다. terminal commit 누락은 winner 보존 위반이다. capture의 ATT/DPT/descriptor LSN 불일치는 거절한다. runtime ledger를0으로 초기화하거나 소비한 CLR 예약을 재사용하는 경우 독립 remaining-cost 비교가 실패해야 한다. WRITER 선택 전 CLR, home allocator에서 stale ownership 복원, redo의 double charge도 거절한다.

한 case에서 checkpoint capture/select는 각 한 번이며 여덟 page redo는 role0→7 순서로 수행한다. home flush 순서는 모두 펼친다. 서로 다른 payload를 가진 home은 pageLSN별 독립 image로 저장한다. 상태 slot0/2는 보존한 terminal 과거 owner, slot1/3은 현재 A/B owner이며 generation0/1을 구분한다. cut/release는 undo slot0/1의 generation을 실제로0→1 변경한다.

기본 graph는 depth16/crash2, case별1,000,000 states·8,000,000 transitions·60초를 유지한다. 모든 mutable field를 hash key로 사용하고 BFS 최초 최소 깊이만 확장한다. 전수 cap·시간 소진은 INCONCLUSIVE다. 형식 codec, 실제 DWB write, 무한 restart의 두-slot bootstrap 귀납 graph, M06 전체 자원/진행, full table integrity scan은 각각 담당 계약/실제 시험에 남긴다. **rebase의 단일 전이 추상은 M06 두-slot graph가 모든 중간 crash의 closure·peak 보존을 확인한다는 조건부 합성 가정**이며 M04 자체가 그 결과를 증명하지 않는다. M06 미완료이면 이 합성 의무도 미완료다. 이 모델의 fixed 비용을 제품 수용량이나 실제 byte bound로 제시하지 않는다.

`crates/testing/reference/tests/dev3_recovery_model.rs`가 `XADB_DEV3_RECOVERY_MODEL ` JSON으로 고정 fixture·mutant·상태/전이/미탐색·필수 witness·최단 반례·시간을 남긴다. 정상 PASS는 해당 유한 모델에만 귀속하며 전체 DEV3/S1·실제 recovery qualification으로 승격하지 않는다.

## D2 — 검산 전용 상태 투영 (2026-10-20)

계약 담당 `/root`가 실행 전 조건부 승인했다. 최초 계약 SHA256 `a875a890710369f85009668ee836b71350753fdc3359eeeb1eedb7fa02c72037`, 전체 분모 실행 source `99b67b77198a1f88b65ebfb6c2594b47c7ae759ef32b124baf32d866ecbf3198`의 정상4개는 각각1,000,000 state cap으로 INCONCLUSIVE였다. 당시 로그·source·계약은 `artifacts/dev3-wave2-supply-review-r3/`에 보존한다. begin_removed의 recovery 초기 flag 오류와 witness 확인 전 PASS 출력도 수정하며 앞선 결과를 수용 증거로 바꾸지 않는다.

새 투영은 다음 검산 전용 정보를 제외한다. 실제 page/home payload·pageLSN, WAL prefix와 실제 byte, persisted checkpoint의 head/progress/ATT/DPT/descriptor, runtime ledger, phase/frame/selector, crash count와 fixture·예산은 그대로다.

- `Checkpoint.home_lsn[8]`은 persisted checkpoint가 아니라 capture 시 DPT를 검산하기 위한 복사였다. 이를 `transition_invariant(before, after, Capture)`의 독립 expected-page 비교로 옮긴다. capture 성공 시 정합성을 확인하고 immutable persisted CP의 DPT는 계속 보존한다. Select가 사용하던 CP equality에서 이 배열을 지우면 모든 persisted 필드가 같은 selector 재선택이 한 번 더 허용되던 무효 no-op만 사라진다. 이 배열은 analysis·redo·restore의 입력이 아니었다.
- `crash_after_restore/boot/writer`, `partial_recrash` 네 누적 bit는 다음 guard나 불변식의 입력이 아니었다. witness는 crash 직전의 restore_count/bootstrap/writer_selected/progress·crash count와 실제 crash 전이에서 곧바로 기록한다. 저장 trace를 initial부터 재실행해 모든 전이와 마지막 predicate를 확인하므로 과거 조건은 이 bit 없이도 입증한다.

단계별 immutable CP 검산 실패와 DPT omission mutant는 계속 실패한다. checkpoint capture 한 번, page redo0→7, home flush 모든 순서·depth16/crash2와1,000,000 states·8,000,000 transitions·60초를 유지한다. 수정 후에도 cap이면 INCONCLUSIVE이며 사후 fixture/상한 축소는 허용하지 않는다. normal PASS는 initial invariant와 필수 witness 분모·모든 저장 trace 재실행 이후에만 출력한다. mutant는 최초 오류가 trace 마지막이며 기대 위반과 같을 때만 EXPECTED_COUNTEREXAMPLE이다.

기존 page token을 소비하는 현재 M04 고정 이력의 outstanding growth는0이다. 비영 growth의 admission→실제 file byte 확장→crash/reconcile 합성 의무는 M03/M06 담당에게 별도로 연결했으며 그 graph의 실제 결과가 없으면 전체 복구 자원 인수도 미완료다. M04의0만으로 비영 growth를 검증했다고 하지 않는다.

## D2 — 사용이 끝난 휘발 상태와 visited key (2026-10-20)

앞선 r4 투영 뒤에도 정상4개는 state cap에 도달했다. r4 source·계약·로그는 `artifacts/dev3-wave2-supply-review-r4/`에 그대로 보존한다. 계약 담당 `/root`가 각 필드의 읽기 위치를 독립 확인하고 아래 네 항목을 승인했다. 실제 nodes와 predecessor trace는 원래 concrete state를 보존하며 visited key만 명시 변환한다. 모든 successor의 독립 invariant·capture-time 검산을 병합보다 먼저 실행한다.

| 필드 | 변환 | 미래 의미가 같은 이유 |
| --- | --- | --- |
| analysis descriptor | Restore가 ledger로 인계한 직후 None으로 해제 | 이후 Restore guard는 Analyzed에서만 성립하며 다음 Analysis가 항상 새 descriptor를 저장한다. 다른 전이·불변식은 이 해제된 복사본을 읽지 않는다 |
| selected_prefix | Run/Ready의 key에서만0 | 이 phase의 전이·불변식은 applied/complete/synced를 사용한다. Restart 진입은 Crash 인수의 prefix로 항상 덮는다. Restart/Booted/Analyzed/Redo/Rebase/Writer에서는 원래 값을 유지 |
| captured==checkpoint | key에서만 None | capture_used가 이미 capture를 금지하고 Select의 cp!=checkpoint도 거짓이다. persisted CP와 DPT는 유지하며 불변식의 같은 CP 중복 확인만 사라진다 |
| frame_peak | key에서만0 | guard는 실제 frames를 읽고 peak는 단조 검산 이력이다. concrete successor의 frames/peak≤3을 병합 전에 검사하므로 잘못된 peak가 숨지 않는다. frame 획득·flush·redo action은 제거하지 않는다 |

restore_count는 crash witness의 과거조건을 보존하기 위해 그대로 둔다. 미래 동작의 key가 같은 더 늦은 경로는 더 짧은 concrete 대표 경로와 같은 enabled action·key successor를 가지며 잔여 depth가 더 작다. 위 필드들은 정상 invariant의 결과도 같으므로 새 오류를 삭제하지 않는다. counterexample과 모든 witness는 저장 concrete trace를 initial부터 실행해 각 전이 검사와 마지막 predicate를 다시 확인한다. fixture·depth16/crash2·실제 home/CP-DPT/WAL/ledger·예산은 변경하지 않는다.

## 검토 제안 — 정확한 page-local 집합 표현 (미승인·미구현)

단순 Cartesian product나 서로 독립인 page 시험은 제안하지 않는다. 같은 action 수·prefix·checkpoint capture 시점이 만드는 page 간 상관을 보존하려면 **각 shared core에 8변수 reduced ordered multi-valued decision diagram(MDD)이 나타내는 정확한 tuple 집합**이 필요하다. 이 절은 아직 구현·실행·인수된 계약이 아니다.

shared core는 phase, Image의 행/index/edge/count/status/obligation, complete/applied/synced/selected prefix, checkpoint와 captured의 DPT 이외 모든 필드, 실제 WAL/file/header bytes, ledger/analysis, frame/restore/capture-used/recovery/writer, crash count다. 각 role 변수는 `(실제 home payload+LSN, 실제 live page payload+LSN, selected CP DPT bit, captured DPT bit)`다. payload를 버리거나 임의 page 상태를 추가하지 않으며 고정 fixture의 실제 finite page image domain에 bijection으로 부호화한다.

| 전이·검사 | 결합 위치와 정확한 집합 연산 |
| --- | --- |
| Append/Prepare/WalSync/Boot/Analysis/Rebase/Writer | shared guard와 shared state만 변경. 해당 core의 tuple 집합을 그대로 운반 |
| Apply | 하나의 group이 shared Image·descriptor를 원자 변경. 8개 live page component를 실제 candidate effect에 따라 동시에 치환. home과 두 DPT component는 유지 |
| HomeFlush(i) | shared WAL/frame guard 뒤 i의 home!=live 조건으로 tuple을 필터하고 i의 home만 live로 치환. 다른 page component·상관은 그대로 |
| Capture | complete==applied 및 capture-used guard. 각각의 captured DPT bit를 실제 homeLSN<liveLSN으로 치환하며 같은 tuple의 나머지 값을 유지. 독립 expected-page 식과 비교한 위반 tuple이 하나라도 있으면 실패 |
| Select | shared CP 필드가 다르면 전체 집합, 같으면 `OR_i(captured DPT_i != selected DPT_i)` subset만 활성화. 따라서 equality에 필요한 공통 조건도 생략하지 않음. 활성 tuple의 selected bit 전체를 captured bit로 치환 |
| Crash(prefix) | 공통 prefix/crash guard. 모든 home·selected DPT를 유지하면서 live pages와 volatile capture를 동시에 제거. 서로 다른 page가 독립적인 prefix를 택하지 못함 |
| Redo(i) | 고정 redo cursor의 i만 실제 pageLSN guard로 치환하고 cursor를 전진. 다른 page는 유지. 모든 role이 끝난 공통 phase에서 page별 expected image의 conjunction 검사 |
| Restore | 정상은 shared descriptor에서 ledger로 인계. stale_home_ownership mutant는 allocator role의 실제 payload 값별 subset으로 분기하여 candidate ledger를 만들고 독립 ownership oracle와 대조 |
| 불변식·reader | edge/count·obligation·물리 byte는 shared 검사. home/WAL·page convergence는 role별 predicate의 conjunction. READY의 reader는 shared candidate row를 읽되 모든 redo role 완료 phase를 요구 |

BFS의 각 depth에서 정확한 tuple 집합을 union하고, 같은 shared core의 이미 방문한 정확한 집합을 difference로 뺀다. action relational image를 component substitution·predicate restriction·union으로만 구성하며 product closure나 widening은 하지 않는다. 따라서 초기 singleton에서 귀납적으로 모든 저장 tuple은 실제 같은 action 길이의 predecessor를 가지며, 모든 원래 successor는 대응 relational image에 포함된다. action·fixture·depth/crash를 제거하지 않는다는 조건 아래 과대·과소생성이 없다. 공유 core가 같아도 서로 다른 page 조합을 임의로 곱하지 않는 것이 핵심이다.

반례·witness 복원에는 depth별 shared-core/MDD frontier와 incoming `(이전 core, action)` edge를 보존한다. 위반 subset에서 concrete tuple 하나를 선택하고 해당 edge의 **역상**과 이전 exact frontier의 교집합에서 predecessor를 찾는다. initial singleton까지 반복해 원래 Action trace를 만든 뒤 기존 concrete step/transition_invariant로 전부 다시 실행한다. witness의 restore 과거조건은 실제 복원 trace에서 다시 검산한다. 이 경로는 새로운 oracle을 candidate에 복사하지 않는다.

계산 한도는 추가 결정이 필요하다. `represented concrete states`, `shared core 수`, `MDD node 수`, `symbolic edge 수`를 구별해야 하며 MDD node를 기존의 concrete state count라고 부르면 안 된다. 원래1,000,000 concrete-state cap의 INCONCLUSIVE는 그대로다. 별도 owner D2 없이 cap의 단위나 case 분모를 바꾸지 않는다. 이 표현 자체는 별도 독립 검토와 작은 concrete graph 대비 exact set/edge 동등성 시험이 필요하다.

## D2 — DPT visited-key 정규화 (2026-10-20)

r5의 dead-state 투영만으로도 정상4개는 cap INCONCLUSIVE였다. 그 source·계약·로그는 `artifacts/dev3-wave2-supply-review-r5/`에 보존한다. `/root`가 실제 `.dpt`의 모든 읽기 위치를 독립 확인하고 이번 투영을 승인했다. 앞 절의 MDD 제안은 채택하지 않았으며 구현하지 않았다.

현재 모델은 selected/captured CP의 DPT를 Capture의 독립 before.home/expected-page 검산과 Select의 구조체 equality 외에는 읽지 않는다. Analysis/Restore/Redo/reader/자원 guard와 invariant는 DPT에 의존하지 않는다. **이 모델은 여덟 page를 전부 redo하므로 DPT에 따른 redo 작업 선정과 복구 I/O bound를 검증하지 않는다.** ATT/DPT가 같은 capture LSN에서 맞게 만들어진다는 의무는 계속 Capture 전이에서 직접 검사한다.

visited key에 한해 selected/captured DPT를0으로 정규화한 뒤 captured==checkpoint이면 key의 captured를 None으로 한다. 실제 persisted CP 값과 concrete node는 수정하지 않는다. CP의 at/head/progress/descriptor/ATT가 모두 같고 DPT만 다른 Select는 DPT만 바꾸는1회 stutter다. 해당 Select 전후에서 다른 모든 enabled action과 안전 predicate가 같고 다음 crash/redo도 DPT를 읽지 않는다. 그러므로 그1step을 생략한 더 짧은 원래 concrete trace가 동일한 관측 결과에 도달하여 depth≤16의 안전성 위반을 없애지 않는다. 실제 의미가 다른 CP 선택은 at/descriptor 등의 key가 그대로 달라 계속 전개한다.

모든 concrete successor와 Capture-time 독립 DPT 비교를 visited 병합보다 먼저 검사한다. DPT omission mutant는 정규화 전에 반드시 실패한다. home/live page payload와 LSN·WAL·ledger·crash/phase·fixture·상한은 그대로이며 saved witness/반례는 원래 concrete step으로 재실행한다. graph가 다시 cap에 닿으면 INCONCLUSIVE다.

## 독립 검토의 합성 경계 보완 (2026-10-20)

M04 source r6 `98f4abe4084ab17ad108808b34f3ebcf659b15d480a0d1d910f3a3b1922f6319`는 동결한다. r6 실행 전 계약 `fb9ec7671fdc0a57ff87cd53aaa1634a18a74442f029c2bddad4939a49c00334`와 이번 문서 보완 bytes를 구별한다. PrepareFrames=3 뒤 Apply는 row/index/undo/allocator/metadata 등 여러 논리 page를 한 원자 semantic step으로 바꾼다. 따라서 세 token의 검산으로 실제 다중 page 전체 pin 집합이나 buffer3-frame 실행을 검증했다고 세지 않는다. 동일 page의 여러 logical pin과 실제 frame 한 개의 관계도 이 모델이 검증하지 않는다. 실제 대상 page pin/workset 소유·부분 pin 대기 금지·buffer replacement는 M03/M06와 제품 시험의 합성 의무다.

Rebase의 actual_wal=4는 closure를 새 base로 인계한 의미 투영이다. 실제 파일의 retained bytes 회수, 두-slot selector와 header byte의 handoff·중간 crash proof는 M06에서 수신해야 한다. M04 PASS만으로 이 물리 동작이나 합성 수신 완료를 주장하지 않는다.

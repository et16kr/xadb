# DEV3 M06 예약·복구 bootstrap 모델

2026-10-20 · **D3-M06-F01 고정 유한 모델 계약 / 실제 실행·독립 검토 상태는 구현 기록**

[예약 회계](disk-storage-design.md#reserve-accounting) · [영속 복원](disk-storage-design.md#recovery-reservation) · [두 슬롯 인계](disk-storage-design.md#recovery-rebase) · [진행 모델](dev3-progress-model.md) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 모델의 단위는 추상 자원 단위다. 실제 8 GiB/1 GiB/256 MiB·opcode byte bound의 F05를 계산한 것으로 표시하지 않는다. production ledger/codec를 import하지 않고 실제 남은 논리 undo 목록·별도 byte inventory에서 비용을 다시 계산한다.

## 영역과 분리

두 writer×두 변경은 undo 네 slot과 행 두 개·index 한 개를 실제 사용한다. status는 writer별 두 generation의 네 slot이다. page 여덟 개는 두 row·한 index·네 undo·한 status/allocator page의 실제 token inventory다. frame 세 개에서 두 작업 pin과 고정 flush pin을 구분한다. key 두 값·NULL, generation 두 값을 고정 fixture에서 모두 사용한다. reader 한 명은 snapshot/보존 floor를 유지하거나 해제하고, maintenance 한 명만 flush/checkpoint/recovery workspace를 사용한다.

아래 runtime 안전성, pressure 진행, 반복 bootstrap, growth 경계은 다른 graph다. 각 graph의 projection은 명시된 관계만 검증하며 서로의 PASS를 전체 product recovery에 합성하지 않는다. M01/M02의 참조 회수, M04의 WAL 그룹 원자성과 M05의 DWB 이미지 복원은 이 모델의 선행 불변식이다. 그 모델 또는 실제 효과가 미검증이면 DEV3 완료는 미완료다.

## runtime 자원과 action

자원 벡터의 정확한 열 순서는 `page token, WAL bytes, control bytes, frame, memory, lock, registry, FD, DWB, growth`의10차원이다. page token은 data/undo/status/allocator의 소유 bitmap에서 각각 식별하고 총8개다. undo/status slot은 별도 네-slot 배열로 검산한다. 고정 footprint는 한 번 charge하고 이미 소유한 page token의 예약이 file bytes를 다시 더하지 않는다. foreground 몫과 보호 몫은 전체 capacity의 분할이다. 각 차원에서 필요량−1/동일/+1 fixture를 실행하고 부족하면 첫 effect 전에 거절한다.

| operation | page | WAL | control | frame | memory | lock | registry | FD | DWB | growth |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| forward 하나 | 1 | 2 | 0 | 2 | 2 | 1 | 1 | 1 | 0 | 0 |
| CLR 하나 | 1 | 3 | 0 | 2 | 2 | 1 | 0 | 1 | 0 | 0 |
| terminal 하나 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 1 | 0 | 0 |
| growth가 필요한 forward | 1 | 2 | 0 | 2 | 2 | 1 | 1 | 1 | 0 | 8 |
| 고정 flush 경로 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 1 | 1 | 0 |

WAL/control/growth는 소비 후 회수 전까지 남는 자원이고 frame/memory/FD/DWB는 실행 구간의 peak다. 각 거래가 같은 maintenance frame을 중복 소유하지 않는다. 별도 FIFO owner 한 명이 공유 progress frame2/memory2/FD1을 받고 flusher의 frame1/memory1/FD1/DWB1을 보존한다. boundary fixture는 위 operation 각각×10차원×3개 여유값, 총150개다. 다른 차원의 여유는 정확한 요구량으로 둔다. 부족한 차원이 실제로0 요구인 경우에는−1 fixture를 capacity0/request1로 바꾸지 않고 **N/A-zero-demand**로 별도 집계하며 정상 실행 분모에서 제외한다.

bootstrap에는 이미 소유한 data/undo/status page8단위, checkpoint slot 각각4단위×2, selector staging2단위가 있다. control peak는10이다. recovery header는2 WAL단위, undo4개의 CLR 비용은 각각3, 두 terminal은 각각1이다. `remaining_cost=3×미완료undo+미완료terminal`이고 최대14다. **REBASE 이전 source forward bytes8도 charged**다. active/both-slots initial은 `forward8+old-header2+미완료14=24`, partial-CLR initial은 `forward8+old-header2+완료CLR6+미완료8=24`다. 따라서 WAL peak는24, owned peak는 `page8+control10+WAL24=42`다. header+control+undo용26단위는 owned42 안의 protected 몫이며 source forward8과 기존 page8에 추가된 별도 capacity가 아니다.

초기 두 closure는 source forward8을 요구할 수 있다. 새 self-contained base 두 개가 선택되기 전에는 이 bytes를 회수하지 않는다. BOOT_RESERVE는 home undo를 읽기 전에 이미 소유한 control/profile과 retained byte inventory로 위24/42 상한 및 남은 예약을 검산한다. REBASE 뒤만 source8·완료CLR·이전header를 반환하며 그때 `new-header2+미완료≤14`이므로16 이하가 된다.16은 restart 전체 peak가 아니다. 공유 recovery frame2+flush frame1=3, memory2+flush memory1=3, FD1+flush FD1=2, DWB1이다. slot 재작성은 선할당4단위를 재사용하여 새 growth를 만들지 않는다. epoch 사전 예약2는 기존 header2를 REBASE에서 회수한 뒤 인계하며 동시에 두 epoch를 새로 보유하지 않는다.

forward byte cost=2, CLR=3, terminal=1을 고정한다. writer별 forward 두 개와 CLR 두 개·terminal의 최대 WAL 비용은11이다. 실제 저장한 byte inventory가 charged WAL이며 아직 쓰지 않은 forward/CLR/terminal만 outstanding이다. split page token·control·workspace 비용은 해당 operation table의 별도 성분이며 동일 token을 두 owner에 주지 않는다.

`admit`, `forward_append`, `forward_apply`, `statement_fail`, `cancel`, `deadlock_victim`, `clr_append`, `clr_apply`, `terminal`, `snapshot_release`, `flush`, `checkpoint`, `crash`, `boot_reserve`, `restore_reserve`, `redo`를 분리한다. forward/CLR의 완결 WAL group은 effect description·progress·잔여 descriptor를 함께 저장한다. append 후 home apply 이전 crash는 redo가 따라잡는다. volatile ledger는 crash에서 소멸하며 checkpoint+완결 WAL descriptor로 한 번만 복원한다. BEGIN WAL은 시작 fixture에서 이미 없을 수 있다.

statement rollback은 앞선 성공 statement를 보존하고 실제 누적 WAL/CLR·미회수 undo/page·완결 split·거래 잠금을 charge한다. 전체 abort 예약을 유지한다. reader가 WAL floor를 pin한 반복 실패 fixture는 다음 admission이 공간 부족으로 거절될 때까지 진행하며 mark 시점 ledger 복원·CLR 소비량 재사용·retained lock 조기 반환 mutant를 거절한다.

## pressure와 진행

old dirty page, redirty hot page, pinned candidate, DWB 진행 중, dirty 없음의 시작 fixture를 사용한다. 세 frame을 round-robin으로 유한 스캔하며 copy와 I/O completion을 분리한다. pressure에서는 새 forward admission을 닫고 기존 유한 작업/rollback/flusher를 계속 실행한다. snapshot floor가 남으면 WAL 회수를 거절한 채 안전하게 quiescent로 끝날 수 있다. owner 해제·실제 byte 회수 뒤만 admission을 다시 연다. q와 LSN을 비교하는 mutant, maintenance까지 admission 차단, 영구 busy/no-op을 검출한다.

진행 fairness·K=2·FIFO/drain은 [M03 진행 graph](dev3-progress-model.md)의 같은 전제다. M06의 pressure graph는 actor별 weak-fair SCC를 독립 검사하며 M03의 성공 결과를 단순히 import하지 않는다. 종료하는 정상 I/O와 유한 obligation을 전제로 한다. ENOSPC·손상·불명 I/O는 성공 진행과 별도 거절 fixture다.

## 반복 crash bootstrap

두 fixed checkpoint slot과 고정 selector staging만 사용한다. state에는 각 slot의 완결 descriptor/capture cut, current/previous 선택, base-only/WRITER continuation, immutable source WAL prefix, 실제 home progress, 실제 retained epoch/CLR bytes, remaining obligation, volatile reservation, recovery phase가 따로 있다.

초기 fixture는 (a) BEGIN이 없는 Active 두 writer, (b) 일부 CLR이 이미 영속화된 loser, (c) 두 slot 모두 current/previous로 선택된 상태다. 각 시작 fixture에서 safety depth≤16/crash≤2를 검사하고, 별도 graph는 crash 횟수 제한 없이 모든 도달 상태를 닫는다. 새로운 epoch의 이름은 현재 선택 epoch/미선택 orphan의 구별만 쓰므로 fresh opaque 이름을 alpha-renaming한다. 실제 LSN·generation wrap을 허용하는 전이가 아니다. undo4개와 terminal2개의 진행 cut은0..6으로 남겨 절대 되감지 않는다.

전이는 `boot`, `repair`, `analysis`, `restore`, `redo`, `sync_source`, `sync_home`, `retire_dwb`, `select_common_fallback`, `write_free_slot`, `sync_free_slot`, `select_new_base_old_previous`, `select_new_common`, `write_second_slot`, `sync_second_slot`, `select_two_bases`, `reclaim_old_wal`, `charge_epoch`, `sync_epoch`, `select_writer`, `append_clr`, `apply_clr`, `terminal`과 각 durable cut의 `crash`다.

선택된 slot을 덮어쓰지 않는다. 처음 두 slot이 모두 선택되어 있으면 최신 current closure를 양쪽 공통 fallback으로 먼저 선택한다. 새 base를 한쪽에 쓴 뒤 `current=new/previous=old→current=previous=new`를 선택하고 다른 slot에 같은 cut을 쓴 다음 두 base를 선택한다. 그 전에는 필요한 WAL을 회수하지 않는다. 새 epoch는 이전 epoch 회수가 끝나고 epoch+잔여 undo 비용을 함께 확보한 다음 charge한다. WRITER 선택 전 CLR/home 효과를 허용하지 않는다. crash는 volatile phase/ledger를 잃지만 영속 selector·complete slot·source bytes·durable home은 보존한다.

모든 상태에서 current/previous 중 최소 한 recovery closure가 채택 undo progress와 ACK를 복원한다. 정상 후보는 양쪽 선택 closure가 모두 유효해야 한다. 실제 bytes+outstanding가 고정 peak를 넘지 않고 descriptor의 잔여 비용은 독립 미완료 undo 목록의 비용과 일치해야 한다. 재시작 횟수만큼 orphan header가 누적되는 mutant, selected slot overwrite, previous WAL 조기 삭제, base-only를 무시한 old WRITER 추적, epoch 사전 charge 누락, CLR 소비량 이중 예약, descriptor 초기화/축소, home redo에 따른 예약 이중 차감을 각각 검출한다.

profile의 foreground 한도 감소는 기존 recovery obligation을 자르지 않는다. 부족한 recovery memory/frame/FD/disk, 누락/손상/미지원 profile은 첫 write 전에 거절한다. 정상 profile의 정상 admitted fixture를 부족 오류로 거절하여 성공 복구로 세지 않는다.

## 탐색·receipt·미완료 기준

모든 case의 cap은 states1,000,000·transitions8,000,000·monotonic60초다. bootstrap safety depth16/crash2와 무제한 crash graph·무장애 진행 graph를 각각 집계한다. runtime은 crash≤2의 전체 유한 graph를 닫으므로 depth16 부분 그래프도 포함하며, 반복 statement 실패·거절까지의 depth16 이후 경로를 별도로 잘라 버리지 않는다. cap/timeout은 INCONCLUSIVE이며 failure 뒤 축소하지 않는다. 같은 semantic state의 BFS 최초 방문을 사용하고 모든 예약/선택/persistent byte/소유/reader field를 key에 포함한다. 누적 crash 수는 bounded safety에만 포함하고 무제한 graph에서는 제외하되 crash 전이가 남는다.

`XADB_DEV3_M06 ` JSON receipt는 suite·fixture·mutant·실행 결과·state/transition·depth·unexplored·최단 반례·peak를 기록한다. 정상 모델 실패·필수 mutant 미검출·미도달 fixture·미완주 graph·행/byte/예약 검산 불일치 중 하나라도 있으면 전체 모델 완료로 표시하지 않는다. source·contract hash/실제 명령·stdout은 통합 receipt에서 고정한다.

## 고정 case 분모와 필수 도달

| suite | 정상 fixture | mutant/반례 | required witness |
| --- | --- | --- | --- |
| admission | 위5 operation×10차원×−1/0/+1=150 입력; zero-demand−1은 N/A | ignore_dimension: 각 양의 요구 차원, borrow_protected, duplicate_page_token | exact/+1 수락, 양의−1 첫 effect 전 거절, 10차원 식별, frame3/FD2의 고정 flush 몫 |
| runtime | begin_pruned_active, partial_clr, cancel, deadlock_victim, statement_failure, 각각 key0/1/NULL와 두 generation=30 | reset_descriptor, reuse_clr_reservation, double_redo_charge, reset_statement_ledger, release_retained_lock | 두 writer/네 undo, append-before-home crash, restore/redo, 앞선 statement 보존, 전체 abort, 실제 charge 누적 |
| pressure | old_dirty, hot_redirty, pinned_candidate, dwb_inflight, no_dirty 각각 reader 유지/해제=10 | block_maintenance, busy_noop, confuse_q_lsn | bounded scan, 실제 flush 완료, reader 유지 시 거절, 실제 회수 뒤 재개, no-dirty 종료 |
| bootstrap bounded | active_begin_pruned, partial_clr, both_slots_selected 각각 depth16/crash2=3 | 아래 bootstrap mutant와 같은 초기 상태 | crash 두 번, BOOT→restore→redo, 각 selector 중간 cut |
| bootstrap closed | 같은 시작3, crash 무제한 closed graph | accumulate_epoch, overwrite_selected_slot, reclaim_previous_wal, ignore_base_only, omit_epoch_charge, reset_descriptor, double_redo_charge, reuse_clr_reservation | 반복 epoch 선택 직후 crash cycle, 양쪽 slot 인계, base-only restart, current+previous closure, ready, fixed peak |
| recovery profile | foreground_limit_lowered, memory_short, frame_short, fd_short, owned_short, missing, corrupt, unsupported=8 | truncate_obligation_to_foreground | 낮춘 foreground 정상 복구, 나머지 첫 write 전 거절, 원래 profile 복구 |

반례의 mutation 지점과 예상 violation은 구현에서 위 이름과1:1로 고정한다. 최초 실행 전에 contract hash와 source hash를 수집하고, 코드에서 아직 구현되지 않은 표 항목을 NOT_RUN으로 남긴다. 일부 suite PASS를 전체 M06 PASS로 승격하지 않는다.

## 최초 실행 직전 추가 동결

- admission은 정상130, zero-demand−1 N/A20, ignore-dimension 반례30이다. 별도 token-partition 정상1/반례2는 실제 page/owner 배열 중복과 고정 flush frame 소유 침범을 검사한다.
- runtime30은 위 동일 fixture/key/generation 그대로 두며, complete raw forward/CLR/terminal 목록을 oracle 입력으로 사용한다. candidate의 row/index·home 적용 위치·descriptor·volatile ledger·charged bytes와 분리한다. statement fixture는 초기 앞선/현재 statement 두 개에서 시작해 실패 CLR을 남기고 다음 두 statement를 시도한 뒤 총 charged+outstanding23에서 다음5 요구를 거절한다. 기존4 undo slot을 실제 사용한다.
- pressure10은 유한 기존 hot write2와 reader release1을 허용한다. 새로운 쓰기 admission은 닫고, scan/copy/I/O/완료·checkpoint를 별도 action으로 둔다. reader-hold fixture에서 pin/floor를 보존한 quiescent refusal은 허용 종료다. reader-release fixture는 실제 회수 후 reopen해야 한다. 정상 closed SCC를 모두 검사하고 deadlock/self-noop 반례도 replay한다.
- bootstrap은 무장애 구간의 정수 rank도 각 non-crash edge에서 엄격 감소해야 한다. closed crash graph의 crash cycle은 남으며 restart 무한 반복 자체의 완료를 주장하지 않는다. epoch 실제 header 수와 charged header 수를 별도 field로 검사한다.
- M04가 growth=0만 검증하는 한계를 보완하기 위해 growth3 case를 추가한다. baseline file8, target16, capacity15/16/17, crash≤2의 closed graph다. reserve intent→physical extent8 추가/charge 전환→file length sync→FILEMAP 선택/dirsync→reference commit을 분리한다. crash 뒤 raw physical extent inventory에서 미실현 growth를 다시 계산한다. 정상은 cap15 첫 효과 전 거절, cap16/17 두 crash 뒤 reserve0와 실제16bytes·commit이 필수다. double-growth, reset-growth, FILEMAP 영속화 전 commit 반례3을 고정한다. 성장 projection은 bootstrap의 page8 고정 base와 별도이며 전체 파일포맷 인수를 주장하지 않는다.

정상 receipt 총188: admission130+token1+runtime30+pressure10+bootstrap bounded3+closed3+profile8+growth3. 반례 총52: admission30+token2+runtime5+pressure3+bootstrap8+profile1+growth3. bootstrap cap self-test1은 분모 밖이다. 모든 normal 필수 witness와 shortest safety counterexample·fair SCC walk를 후보 전이로 다시 실행한 후 receipt를 출력한다.

## 최초 실행 후 독립·자체 검토 보완

첫 source 실행은 normal188/negative52를 통과했으나 그 identity의 실행은 최종 source 검증과 구분한다. 독립검토가 발견한 일괄 charged1/reserved1 baseline은 frame 요구를4, FD를3으로 계산하여 물리 분할과 혼동되므로 수정했다. 10차원 순서 그대로 baseline charged=`[1,2,1,1,1,0,0,1,0,8]`, reserved=`[1,1,1,0,0,1,1,0,0,0]`이다. forward/CLR exact는 fixed flush1+작업2=frame3, fixed flush FD1+작업 FD1=FD2다. +1은 요구량보다 큰 quota의 산술 여유이며 네 번째 frame이나 세 번째 FD를 실제 생성하지 않는다. token graph에서 실제 frame3/FD2의 owner를 검사한다. 정상130/N/A20/negative30의 입력 분모·operation 요구량은 변경하지 않았다.

남은 비용 oracle는 candidate의 active/planned cache 대신 immutable admission/cancellation 입력과 complete Forward/CLR/Terminal 목록에서 계산한다. candidate status cache는 실제 generation별 네 status slot에 연결하고 raw terminal inventory와 검산한다. slot 재사용 수명은 M01/M02의 의무이며 M06의 generation 입력 변형을 별도 수명검증으로 세지 않는다. 정상 witness path와 끝 상태를 replay하고 bootstrap의 required selector/crash/ready predicate 및 pressure fair-cycle의 semantic closure와 계속 enabled인 actor 실행도 재검산한다.

독립검토로 각 explorer 종료 시각 검사와 witness/SCC 검산을 포함한 case 전체60초 검사도 추가했다. 마지막 확장 뒤 예산을 넘겨도 INCONCLUSIVE이며 PASS를 출력하지 않는다. generic explorer에는 state1/transition0/time0의 세 실패 경계를 직접 실행하고, 기존 bootstrap state cap self-test를 유지한다. 이 harness 검사는 실제 정상/반례 case 분모에 포함하지 않는다.

최종 mutant의 `reset_descriptor`는 복원 ledger만0으로 만드는 경우와 구별하여 실제 persistent descriptor도0으로 바꾼다. bootstrap은 `persistent_descriptor_mismatch`, runtime은 독립 raw-intent/log와의 `remaining_obligation_mismatch`가 필수 반례다. 나머지7 bootstrap mutant의 의미·분모는 그대로다.

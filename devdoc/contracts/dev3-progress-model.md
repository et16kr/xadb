# DEV3 M03 변경 순서·행 관측·진행 모델

2026-10-20 · **D3-M03-F01 고정 유한 모델 계약 / 실제 실행·독립 검토 상태는 구현 기록**

[소유 계약](disk-storage-design.md#mutation-protocol) · [행 관측](disk-storage-design.md#observed-row-version) · [전체 모델 의무](../plans/dev3-implementation-plan.md#state-model) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 모델은 production 구현을 import하지 않는다. 작은 유한 영역의 모델 검증이며 실제 thread·disk·DS 인수를 대신하지 않는다. 아래의 네 서로 다른 상태 기계를 합산한 숫자를 하나의 동시 전수 공간이라고 부르지 않는다. 각 투영의 연결 전제와 미검증 사항을 receipt에 남긴다.

## 영역과 분해

writer 두 명, reader 한 명, maintenance 한 명, 행 두 개, index 한 개, key `{0,1,NULL}`, page 여덟 개, frame 세 개를 사용한다. 관측 모델은 두 writer의 두 행과 index를 실제 변경하며, 논리 version과 물리 page revision을 구분한다. page 준비 모델은 여덟 page 중 서로 다른 모든 두 page 조합·두 발견 순서·각 incarnation 두 값을 fixture로 고정하고 세 frame의 소유를 실제 계산한다. reference/slot 수명은 M01/M02가 소유하며 M03이 그 검증을 중복 주장하지 않는다. M03에서 undo 네 slot은 두 writer의 앞선/현재 행 변경을 저장하며 status 네 slot은 두 writer의 generation별 상태를 표현한다. 한 fixture에서 활성인 generation은 각 writer당 하나다.

독립 행 oracle는 fixture의 논리 사건에서 기대 성공/충돌과 값·index를 계산한다. candidate가 선택한 validation 결과, page snapshot, rollback 결과를 기대값으로 사용하지 않는다. page 준비 oracle는 명시 page/owner 집합과 sorted numeric rank에서 계산한다. 실제 namespace·WAL crash atomicity는 M04 및 제품 I/O의 별도 의무이며 M03 안전성에는 crash action을 추가하지 않는다.

## 관측·rollback fixture와 action

격리 두 값 `ReadCommitted`, `Serializable` 각각에 다음 fixture를 적용한다. key 두 값과 NULL을 모두 초기 값으로 사용한다. 앞선 T1 statement는 다른 행을 이미 변경했다. conflict이면 그 own write까지 전체 rollback하고 T2가 ACK한 행을 보존한다.

| fixture | 독립 기대 |
| --- | --- |
| immediate_update | observe 뒤 T2가 새 논리 version을 commit/unlock한 다음 T1이 대기 없이 X-lock 획득: conflict |
| waited_update | T2 X-lock 보유 중 T1 lock 시도·wait, commit 뒤 재획득: conflict |
| delete | T2 delete commit 뒤 변경 전 conflict |
| same_bytes | 같은 bytes여도 새 logical version이면 conflict |
| abort_restore | T2 abort가 이전 version/value 복원 후 T1 성공 |
| own_write | T1이 앞선 own-write version을 관측한 경우 성공 |
| physical_move | split/이동으로 page revision만 바뀌면 성공 |
| wrong_row / wrong_statement | 다른 행/statement용 observation 재사용은 효과 전 거절 |

`observe`, `other_lock`, `other_apply`, `other_commit`, `other_abort`, `lock_attempt`, `validate`, `apply`, `rollback`, `reader_check`를 별도 action으로 둔다. fixture가 요구하는 순서는 observe→타 writer 효과이며, wait fixture는 첫 lock attempt를 commit 앞에, immediate fixture는 commit 뒤에 둔다. 이 순서 제약은 대상 반례를 고정하며 모든 가능한 application 이력을 탐색했다는 뜻이 아니다. validate와 apply 사이에는 T1이 행 X-lock을 계속 보유한다. `reader_check`는 선행 own-write의 undo 보존을 검사한다. snapshot 해석 자체는 M02의 의무이며 이 projection에서 중복 PASS로 세지 않는다.

별도 split fixture는 `T1 변경→완결 split→T2 다른 행 변경/commit→T1 rollback`을 수행한다. `split`, `other_apply`, `other_commit`, `rollback`을 분리하고, reader 확인을 교차시킨다. page 준비는 별도 graph가 소유하고 이 split graph와 Cartesian product를 만들지 않는다. split/root 구조는 rollback 뒤에도 유지하며 T2 ACK를 보존한다. before-image의 전체 page를 복사하는 mutant를 거절한다.

## 준비·latch fixture

두 writer는 반대 순서로 같은 page 집합을 발견한다. `reserve_all`, `acquire_gate`, `try_first`, `try_second`, `apply`, `cleanup`을 분리한다. reader의 세 번째 frame 점유/해제·page content latch 진입/해제를 교차한다. 전체 frame 집합을 확보하지 못하면 보유 pin은 0이다. try 실패는 gate/latch/pin을 모두 반환한다. acquired latch rank는 단조 증가이며 gate 안 추가 page 발견은 무효과 cleanup 뒤 재탐색으로만 처리한다.

## 닫힌 진행 graph

K=2를 고정한다. writer의 논리 작업은 각각 CLR 한 단계다. 빠른 try 실패 두 번 뒤 FIFO ticket을 요청하며, ticket 대기는 workspace·pin·상위 lock을 보유하지 않는다. head ticket은 `prepare`에서 신규 적용/새 snapshot admission을 닫고 기존 reader content와 전용 flusher를 허용한다. 필요한 clean frame을 flusher가 공급한 뒤 `drain`으로 이동하여 새 content 진입을 닫는다. 기존 유한 content가 끝나면 한 적용을 완료하고 ticket/workspace를 반환한다.

reader는 `q_read→register→content_enter↔content_exit→release`이며 새 snapshot 획득·기존 content 접근을 구분한다. `q_read` 뒤 등록 전에는 gate가 reader 소유다. flusher는 `dirty→copy→io→clean`이며 copy와 I/O는 분리한다. 실제 I/O 성공과 유한 종료를 가정하지만 단계가 scheduler action인 점은 숨기지 않는다. reader의 새 획득과 content 반복, writer retry를 graph cycle로 유지한다.

fairness는 actor별 weak fairness다. 어떤 actor의 action이 영원히 계속 enabled이면 그 actor는 결국 실행한다. actor의 각 phase에는 완료 의미가 같은 유한 선택만 둔다. 따라서 actor 이름만 실행하는 no-op이 진행 증거가 되지 않는다. 닫힌 graph의 미완료 상태 SCC에 cycle이 있고, SCC 모든 상태에서 enabled인 actor마다 SCC 내부 실행 edge가 있으면 fair noncompletion counterexample다. 내부 cycle의 모든 상태/edge를 반복 방문하는 walk가 이러한 약한 공정 실행을 구성한다. 완료 상태로만 나가는 continuously-enabled actor가 있으면 그 SCC는 공정하지 않다.

mutants는 unbounded try(포화 retry를 cycle로 표현), prepare에서 flusher 차단, 상위 lock을 가진 ticket 대기, FIFO overtaking, partial pin 대기, 역순 latch, stale 전체 page rollback, wait-only/physical-only/value-only/scope-ignoring observation 검사다. 각각 지정한 safety violation 또는 fair SCC가 실제 도달해야 한다.

## 탐색과 receipt

안전성 BFS는 depth≤16, case별 states≤1,000,000·transitions≤8,000,000·monotonic 60초다. 진행 graph는 depth 제한 없이 같은 state/transition/time cap으로 도달 graph를 닫고 SCC를 검사한다. cap/timeout은 INCONCLUSIVE이며 줄인 범위로 재실행해 이전 실패를 지우지 않는다. state key는 mutable semantic field 전부다. 동일 state의 최초 최소-depth 방문만 확장하며 관측값/예약/guard를 key에서 제거하지 않는다. progress graph에서는 retry≥K를 K로 정규화한다. 이후 정상은 ticket으로만 전진하고 unbounded-try mutant만 retry self-cycle을 남기므로 이 quotient는 미래 enabled action을 보존한다.

stdout `XADB_DEV3_M03 ` JSON에 suite, fixture, mutant, status, states, transitions, depth, unexplored, witness/반례를 기록한다. 정상 안전성·진행의 전수 완료와 모든 필수 반례 검출을 따로 확인한다. 한 suite 실패·미완주·필수 witness 미도달은 전체 모델 완료를 막는다. 실제 source/contract hash와 전체 명령·stdout은 통합 receipt가 보존한다.

## 실행 전 고정 분모

| suite | 정상 case | mutant case | 필수 witness |
| --- | --- | --- | --- |
| observed_row_version | 9 fixture×2 isolation×3 key×2 generation=108 | wait-only immediate, physical-only move, value-only same-bytes, scope-ignoring wrong-row/wrong-statement=5 | 각 terminal/row-index; waited 실제 대기; 성공 commit |
| page_preparation | 28 page쌍×2 발견순서×2 incarnation=112 + stale incarnation 1+reader content 경합1=114 | partial pin, 역순 latch, cleanup 누락=3 | apply, gate 안 추가 page 발견 후 cleanup/reprepare; stale은 latch 전 거절 |
| split_rollback | 3×3 key쌍=9 | 같은 key쌍 stale page restore=9 | 완결 split 유지, T2 ACK 보존, 논리 undo |
| closed_progress | 두 writer/reader/flusher=1 | unbounded try, prepare flusher 차단 fair SCC2 + 상위lock/FIFO safety2=4 | 둘 모두 완료, dirty prepare, 기존 reader drain, fair pending SCC 없음 |

정상232, mutant21의 case receipt를 각각 남긴다. cap self-test는 case 분모 외 한 테스트다. 관측 safety·준비·split은 depth16의 모든 도달 상태, progress는 닫힌 graph다. 서로 다른 그래프의 상태 수를 하나의 전수 동시 이력으로 주장하지 않는다.

## 최초 exploration 발견과 수정

첫 실행은 safety 정상230·negative17을 완료했으나 정상 progress에 fair pending SCC를 발견했다. 첫 원로그는 `artifacts/dev3/m03-m06-20261020/m03-first.log`에 유지한다. 초안에서 queued head가 신규 q-read를 닫지 않았고, prepare flusher copy가 반복 reader content 진입과 순수 try로 경쟁했다. 소유 계약의 모든 content admission 규칙에 따라 **대기 ticket이 생긴 시점부터 신규 snapshot admission을 닫고**, prepare의 flusher copy 요청은 기존 reader content 종료 후 우선 진입하도록 구체화한다. 기존 reader content는 종료하며 flusher copy 후 다시 허용된다. 새 reader/retry cycle 자체와 K2·cap·fixture 분모는 유지한다. weak fairness를 strong fairness로 바꾸지 않는다.

독립검토에서 pin 수 합을 물리 frame 수로 센 결함을 발견했다. 소유자별 pin 집합은 유지하되 실제 frame 사용량은 모든 writer/reader가 pin한 **서로 다른 page의 합집합**으로 계산한다. 두 writer의 같은 page 동시 pin을 필수 replay witness로 추가하며 reader는 별도 page를 pin한다. 발견 순서는 실제 `discovery` 배열이고 candidate는 그 입력을 rank 정렬한다. 두 발견 순서는 입력 경계의 동작을 검증하지만 정렬 후에는 같은 enabled action인 대칭이며 두 개의 다른 논리 이력이라고 해석하지 않는다. 정상 witness마다 원본 trace를 replay하고 마지막 predicate를 검산한다. fair SCC에는 cycle 시작 위치를 기록하고 반복 후 같은 semantic state 복귀 및 계속 enabled인 actor의 cycle 내 실행을 재검산한다. 이전 실행의 pin 부분을 최종 검증으로 승격하지 않는다.

공유 frame 수정 후 세 번째 실행은 cleanup mutant의 reader 대상이 비경합 page로 바뀐 fixture 오류를 드러냈다. 최초 frozen 경합 대상page0으로 복구하고, 같은 경합의 정상 try-failure cleanup을 별도 정상1 case로 추가했다. 기존 정상/반례 분모를 줄이지 않았으며 정상232·반례21이다. 모든 graph의 종료 시각과 witness/SCC 검산까지의 case 시각을 확인해 60초 이상이면 PASS를 내지 않는다. state/transition/time cap self-test는 모두 INCONCLUSIVE를 확인한다.

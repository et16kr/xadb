# DEV3 3.16 — 자원 종합·성능·최종 인수 개발 상세 명세와 설계도

2026-10-22 · **작업 상세 설계 / 항목별 일부 증거 존재 / global F05·PF09·S1 미완료**

[공통 규칙](dev3-task-designs.md) · [수치 계산 정본](dev3-resource-bounds.md) · [자원 인수](../plans/dev3-implementation-plan.md#resource-proof) · [PF09](../plans/dev3-implementation-plan.md#performance) · [완료 정본](../plans/stage-completion.json)

## 1. 목적과 진입 경계

각 opcode가 첫 effect 전에 제출한 F05를 종합하여 동시에 성립하는 engine 한도·진행 조건을 확인하고, 실제 disk 기능의 남은 correctness·fault·성능 의무와 증거를 대응시킨다. 3.16이 다른 작업의 자원 계산·oracle·fault 설계를 뒤늦게 처음 수행하는 단계가 되어서는 안 된다.

제품 gate S1과 DEV3 내부 구현 묶음 S1(표현 leaf)은 다르다. 표현·모델·component의 PASS로 제품 S1을 승격하지 않는다. DEV2의 미해결 PF04·현재 단계 미완료도 그대로 보존한다. 새 문서·새 commit·전체 source fingerprint 변경 자체는 재시험 사유가 아니다.

## 2. 요구와 인수 입력

| 요구 ID | 명세 |
| --- | --- |
| 3.16-R1 | opcode마다 forward·실패 cleanup·전체 abort/CLR·SMO·terminal·bootstrap의 실제 encoded bound와 소유/반환을 갖는다. 미정 항목은 effect 허용 불가 |
| 3.16-R2 | 모든 자원에서 charged+outstanding≤capacity를 동시에 만족. 공유 workspace·page token·이미 소유한 file bytes를 중복 차감하거나 누락하지 않음 |
| 3.16-R3 | 4/16/64MiB buffer에서 실제 최대 동시 admission을 계산. 최대32 transaction은 상한이며 최대 크기32개를 모두 허용한다는 약속 아님 |
| 3.16-R4 | 정상 admitted 작업의 rollback/flush/recovery는 protected 몫과 progress turn으로 종료 가능. corrupt/I/O 거절을 정상 진행 성공으로 바꾸지 않음 |
| 3.16-R5 | 독립 correctness history가 확보된 동일 의미·durability·환경에서만 PF09 비교. 시도 TPS·durable 성공·충돌·거절/재시도 비용을 분리 |
| 3.16-R6 | 기존 PASS는 원래 source/case 증거로 재사용. 새/수정 경로와 남은 의무를 명시하고 omitted/중단/FAIL을 현재 실행 PASS로 표시하지 않음 |
| 3.16-R7 | 모든 필수 DS01–06·KU09/10·RC-T12–14·PF09 의무의 충족 근거와 독립 검토를 연결. 미실행/미해결 실패가 남으면 S1 미완료 |

입력 자료는 실제 opcode/codec revision, 자원 ledger·수명 표, runtime/native/thread/reopen 기록, 외부 ACK/UNKNOWN 이력, 독립 oracle·mutant 결과, fixture/환경·binary/source/로그 identity다. 후보 설계의 성능 추정·문서 정합성 검토는 제품 실행 결과 열에 넣지 않는다.

## 3. 자원 회계 설계

```mermaid
flowchart TD
  O[각 opcode 실제 bytes와 상태 전이] --> B[forward CLR SMO terminal bound]
  B --> L[단계별 charge reserved 반환과 소유]
  L --> P[profile별 동시 admission과 진행 reserve]
  P --> F[경계와 fault의 실제 peak witness]
  F --> R[변경 경로 correctness 인수]
  R --> M[동일 조건 PF09 측정]
  R --> C[필수 의무별 기존과 신규 증거 대응]
  M --> C
  C --> D[독립 검토와 S1 판정]
```

각 opcode 표는 다음 열을 가진다. 실행 전 구체 수치·source를 고정하며 논리 payload 길이를 그대로 WAL 비용으로 쓰지 않는다.

| 열 | 계산 내용 |
| --- | --- |
| 입력과 표현 상한 | row/key/index 수·tree 높이·논리 mutation 수·실제 codec revision |
| page/slot | data·index·overflow·undo root/chunk·status·allocator·anchor·count/queue page, rollback 재삽입 split 포함 |
| WAL/control | frame header+native before/after+redo+edge/inventory+descriptor, forward/CLR/system/abort-end, 설치·교체 peak |
| growth/token | 신규 extent64-page 올림, 실제 파일 bytes, 기존 extent의 page token, 미소비 intent를 구분 |
| 동시 생존 자원 | 전체 pin 집합·encode/decode 중첩 locals·caller/owner/scratch·FD·lock/registry·DWB slot |
| 수명·반환 | admission→prepare→stage/apply→sync→statement 복귀→terminal→retirement→recovery 각 소유자/실제 반환 사건 |

전역 제약은 owned8GiB, 그 안 protected1GiB, retained/outstanding WAL256MiB, buffer4/16/64MiB, metadata/cache8MiB, private 거래당8MiB/합64MiB, system flush4MiB+단일 maintenance12MiB, FD128·snapshot32 등 기존 profile을 따른다. component WAL1MiB+metadata64KiB·locator1MiB를 metadata에서 한 번씩 빼고 나머지5.9375MiB 안에 frame header/ATT/locks/cache 등이 실제로 들어가는지 확인한다. source-owned 타입 회계·compiler stack·실제 RSS를 서로 다른 측정으로 보고한다.

## 4. 회계 반례와 진행 검증

statement 논리 rollback은 ledger를 mark로 되감지 않는다. `W_n=W_0+Σ(forward_i+CLR_i)−실제회수_n`을 독립 계산하고, retained lock·undo/page·완결 SMO·전체 abort 예약을 계속 소유한다. reserve의 이미 소비된 CLR bytes가 다시 remaining으로 나타나는 mutant를 거절한다.

모든 자원에 필요량−1/동일/+1, row128KiB·index8·tree 높이 경계, buffer4/16/64MiB·최대32 요청을 사용한다. 서로 양립하지 않는 극값은 실제 유효한 별도 fixture로 나눈다. 계산된 수까지만 admission하고 나머지는 첫 effect 전에 거절한다.

rollback이 자기 완료를 위해 checkpoint/aging 회수를 기다리는 순환, maintenance workspace를 가진 채 turn을 기다리는 경로, turn 준비에서 flusher까지 막는 경로, 부분 pin 보유 frame 대기, 무한 try-latch 재시도는 금지한다. 정상 I/O와 공정 actor 아래 유한 작업의 ticket/drain/완료 witness를 검증한다. 외부 ENOSPC/I/O 불명은 fence/복구 사례로 구분한다.

crash 뒤에는 고정 bootstrap+동일 prefix에서 복원한 obligation+실제 retained bytes가 기존 한도 안에 들어가야 한다. BEGIN WAL 제거, 부분 CLR, 두 슬롯 current/previous, 새 epoch 선택 직후 반복 crash를 포함한다. restart마다 누적되는 임시 bytes를 허용하지 않으며 충분한 무장애 구간에서 loser 완료를 별도로 검증한다.

## 5. 4배 working set과 대형 inventory

row/index/locator/allocation inventory의 **각 실제 접근 집합**이 configured buffer의4배 이상이어야 한다. 동일 metadata 재읽기 I/O·zero padding·sparse apparent size로 독립 inventory bytes를 늘리지 않는다. page-cache miss와 실제 device I/O witness도 구별한다.

| buffer | allocator cell6208 bytes 기준 최소 extent | extent 물리 bytes |
| --- | --- | --- |
| 4MiB | 2,703 | 2,834,300,928 |
| 16MiB | 10,811 | 11,336,155,136 |
| 64MiB | 43,241 | 45,341,474,816 |

기본8GiB만으로16/64MiB profile의 allocator4배 요구를 충족했다고 주장할 수 없다. [64GiB qualification 전용 준비 방향](dev3-resource-bounds.md)은 전체 회계·profile/형식 적합성·실제 fixture·실행 전 단계이며 현재 지원 profile이 아니다. 제품 기본 한도를 조용히 올리거나 allocator4배 분모를 줄이지 않는다. 대형 fixture에는 실제 FILEMAP/extent 확장 표현과 control/WAL/protected·다른 작업 공간도 포함해 D2 동결한다.

## 6. PF09 설계·측정 명세

| workload | 독립 조건·측정 |
| --- | --- |
| CRUD·point/range | 1/2/4 writer/reader, 요청·시도·durable 성공 TPS, 성공 latency와 원래 요청부터 retry 포함 latency |
| 충돌 분해 | 독립 table / 같은 table 다른 행·비충돌 key / 같은 행·unique key × RC/Serializable. 실제 read/write/key/catalog 의존으로 true/false conflict 판정 |
| WAL·DWB | commit/WAL sync p99, batch1/8/64의 실제 평균 크기·pages/s, image/home/control bytes와 file sync/dirsync 횟수 |
| Commit 설치 장벽 | stage→sync→전체 status/terminal 등록 시간, 후속 producer 대기 시간·양도/재진입 횟수. 물리 sync와 의미 설치 대기를 분리하고 원래 요청 latency에는 둘 다 포함 |
| flush·진행 | oldest recLSN lag·dirty age·선택/완료/redirty·서비스 최대 간격·무진행 round·admission 거절 원인·rollback/eviction latency |
| snapshot/aging | 긴 reader·Active 거래, undo 생성/보존/회수·owner/horizon lag, 실제 재사용, WAL floor와 undo 수명 분리 |
| checkpoint/reopen/audit | 같은 WAL/dirty 양에서 DB256MiB/1GiB, 정상 read bytes/시간과 REBASE flush/selector/회수, full audit 별도 |
| backup/restore | exact C 일치·중단 시간·copy bytes·취소/복구와 source-WAL 없는 restore 뒤 새 ACK |
| 자원 | RSS/FD/pin/lock·private/system/protected peak, 장기 기울기, runtime baseline과 사전 고정 margin |

실행 전에 환경·filesystem/장치·build/features·fixture/seed·부하율·queue 상한·retry 정책·warm/cold 조건·반복/시간·batch deadline·비교 regression 한도·계측 비용 허용치를 동결한다. 이 문서는 미측정 TPS/p99 목표를 새 SLA로 만들지 않는다. 성능 job과 장치 부하 작업을 겹치지 않는다.

Serializable 같은 table false conflict를 허용하더라도 충돌률/재시도 비용을 숨기지 않는다. native RC와 SQL Serializable의 수치를 동일 의미 성능 개선으로 비교하지 않는다. group commit·gate 분할·부분 batch retirement는 측정 뒤 별도 설계 변경이다.

<a id="observation-delivery"></a>
### 6.1 계측의 공급 시점과 집계 계약

[방향 리뷰 후속 결정](../plans/dev3-implementation-plan.md#direction-review-adoption)에 따라 계측을 최종 PF09 직전에 붙이지 않는다. 각 기능 구현과 함께 아래 관측을 공급하고 작은 fixture에서 필수 집계 분기를 검증한다. 긴 부하·대형 working set·profile 전체 조합의 성능 판정은 S5다.

| 공급 owner | 구현 동반 관측 |
| --- | --- |
| 3.04/3.11 WAL·commit | 요청→시도→WAL sync→status/terminal 의미 설치→frontier/응답을 구별한다. WAL 대기와 실제 sync, Commit 설치 장벽에서 막힌 후속 producer 대기를 각각 기록하며 실패·UNKNOWN·취소도 해당 분모에 남김 |
| 3.05 flush | image write/sync·Pending control/dirsync·home write/sync·retirement control/dirsync별 횟수/bytes/시간, 실제 batch page 수와 selected/completed/redirtied를 기록. batch 설정값을 실제 평균으로 보고하지 않음 |
| 3.10/3.16 충돌·진행 | 원래 요청·재시도·성공 commit·충돌·quota 거절·fence를 분리하고 대기/실행 시간과 최종 outcome을 결합한다. true/false conflict는 독립 workload 관계로 판정하며 실패를 제외한 latency만으로 개선을 주장하지 않음 |

F02/F05에는 monotonic clock·구간 시작/끝·중첩 구간의 중복 합산 방지·counter overflow·trace 누락/잘림·샘플링·메모리 상한을 고정한다. 미계측 값은0으로 출력하지 않는다. latch 안 I/O·동적 allocation·진단 대기를 추가하지 않으며 기존 진단/metadata 예산에 포함한다. clock/진단 실패가 원래 거래 결과를 바꾸거나 숨기지 않도록 실패 의미도 고정한다. 계측 자체의 비용과 활성화 조건을 남기고 같은 조건에서만 성능을 비교한다. 이 절은 계측 구현 완료나 측정 전 gate 분할/group commit 채택을 뜻하지 않는다.

64GiB 준비의 담당·시점·지원 승격 조건은 [자원 정본](dev3-resource-bounds.md#qualification-profile-preparation)에 둔다. 각 기능 owner의 미정 F05를 이 작업으로 이월해 effect를 먼저 허용하지 않는다.

## 7. 인수 추적 행렬

| 의무 | 공급·통합 담당 작업 | 결과에서 확인할 핵심 |
| --- | --- | --- |
| DS01·KU09 | 3.01/3.06/3.07/3.10 | 실제 tree/overflow/cursor·row↔index, split/delete 중 고정 snapshot |
| DS02·KU10·RC-T12 | 3.02/3.03/3.04/3.05/3.08/3.11/3.12/3.15 | multi-object 원자성·steal/no-force·WAL/DWB·ACK/UNKNOWN·외부 이력 |
| DS03 | 3.03/3.05/3.06/3.07/3.10/3.12/3.14/3.16 | 4배 각각·bounded RAM/FD·pressure/drain·startup/audit 비용 분리 |
| DS04·RC-T13 | 3.03/3.06/3.08/3.09/3.10/3.12/3.14 | snapshot/Active undo·pin/ABA·begin-WAL 제거·old object/overflow |
| DS05·RC-T14 | 3.03/3.04/3.05/3.09/3.12/3.13/3.14/3.15 | 예약 복원·fuzzy/fallback/floor·DWB repair·REBASE·restore 재개 |
| DS06 | 3.06/3.07/3.08/3.09/3.10 | 부분 NULL·최종 UNIQUE·key 교환·이전 statement·관측 행 full abort |
| M01–M06 | 해당 producer/consumer와 독립 모델 | 실제 declared 공간/반례/진행 범위. 모델 PASS와 native/thread/disk 결과 분리 |
| PF09·S1 | 3.16 종합, 각 기능 owner 확인 | 같은 의미의 성능 분모·필수 의무/증거 누락 없음·미해결 FAIL 없음 |

## 8. 증거 취합·완료 판정

2026-10-22 후속 사용자 지시에 따라 [단계별 테스트 계획](../plans/dev3-test-plan.md#final-suite)의 **최종 S5 DEV3 전수 실행**을 수행한다. 중간에는 변경 기능의 필수만 실행하고, 추가 테스트·큰 fixture·계측은 해당 구현과 함께 준비한다. 아래 과거 증거 재사용은 중간 실행 선정과 최종 전수 뒤 후속 diff의 판정에 적용하며 최종 전수 실행을 생략하는 근거가 아니다. DEV0/1/2·workspace 전체 wrapper 실행으로 확대하지 않는다.

증거 대응표는 `의무 ID / 사례·변형 / 실제 source·binary·fixture / 마지막 실행 범위·원래 결과 / 이후 관련 diff / 신규 실행 필요 여부·이유 / 독립 검토 / 남은 의무`를 갖는다. 재사용한 과거 결과는 그대로 표시한다. root fingerprint만 달라진 경우 source의 관련 경로를 대조하고 무변경 case를 재실행하지 않는다. 새로운 연결 코드에는 그 연결을 검증하는 case를 선택한다.

문서 검사는 push gate에서 수행한다. 원본 FAIL·중단 receipt를 수정하거나 omitted case를 새 PASS로 만드는 집계는 금지한다.

완료를 선언하려면 실제 DS/KU/RC/PF 의무를 모두 추적하고, source별 적용 범위·변경 경로 증거·검토 결과·정상/거절/미실행 분모를 명확히 해야 한다. 단순 완료 문서 수/16, 모델 case 수, 코드량으로 진행률을 만들지 않는다.

<a id="comparison-measurement"></a>
## 비교 분석 후속 — 병목 판정과 확대 인수

[표현/예약 산식](dev3-resource-bounds.md#resource-vector-proof), [Serializable cohort](dev3-task-10-concurrency.md#serializable-conflict-bound), [Commit 서비스 시간](dev3-task-11-commit.md#commit-service-bound), [보존 압력](dev3-task-14-reclamation.md#retention-pressure-model)은 측정 가설이다. 실제 TPS/p99·타 DBMS 우열로 재표기하지 않는다. PF09는 같은 durability·isolation·data/index 분포·buffer/profile·retry 정책에서 비교한다.

| 판단 | 먼저 수집할 값 | 후속 후보·선행 조건 |
| --- | --- | --- |
| 같은 table 병렬 요청 실패 | 원래 요청/attempt/성공·충돌·거절·UNKNOWN,1/2/8/32 요청의 실제 dependency와 retry 비용 | bounded fine validation은 phantom/absence/write skew/DDL 증명 뒤 별도 D2 |
| Commit 장벽 포화 | sync와 의미 설치 S의 평균·분산·분위수, queue/installation 대기와 원래 요청의 전체 latency | group commit은 batch prefix validation/설치·UNKNOWN·checkpoint·F05 증명 뒤 |
| scan 비용 | visible 결과·candidate·Undo hop·root reseek·read bytes·시간 | candidate/byte bounded batch, snapshot/own mutation/full-key 보존 뒤 |
| 공간·retention | 사용자 bytes와 row/index/overflow/Undo/WAL/control의 allocated/reserved/retained, horizon lag와 실제 반환 | 형식 최적화는 measured 비용과 compatibility 검토 뒤 |
| full audit 비용 | target T·edge E·batch B, 방문 수·재순회·bytes·시간 | batch별 incoming 계산이 전체 E를 읽으면 `E×ceil(T/B)` visits. E≈T이면 고정 B에서 이차 증가 가능. RAM 상한과 I/O 예산 동시 검토 |
| 대형 allocator | encoded cell inventory≥4×buffer, 실제 extent 수·전체 disk 공존·growth 배열/FD/복구 workspace | [64GiB 준비](dev3-resource-bounds.md#qualification-profile-preparation). 지원 tuple/43,241 extents와 전체 회계·fixture 없이는 PASS 금지 |

T=E=1,000,000에서 audit B=4096/32768이면245,000,000/31,000,000 edge visits라는 예시다. B=32768을 승인된 capacity로 쓰지 않는다. 실제 entry bytes·hash/cursor·local 합계로 B를 먼저 동결한다. measured 개선이 필요할 때만 다른 bounded 알고리즘을 검토하며 무제한 map/spill을 숨기지 않는다.

PF09 반복은 일반 DP07의 최소10쌍을 따른다. COMM 전용5+5 규칙을 적용하지 않는다. 평균뿐 아니라 분포·실패/재시도·누락 이력과 계측 비용을 보존하며 원래 요청의 최종 결과까지 latency를 집계한다. 미실행은 NOT_RUN이며 docs 채택·산술·모델·memory/native 실행을 각각 구분한다.

# XADB — AI가 분석·수정·검증하는 성능 개발 구조

2026-10-18 · **사용자 요구에 따른 설계 계약 / 성능 실행기·전체 계측·AI 성능 pilot 미구현·미실행**

[관리 JSON·인수 계획](ai-performance-engineering.json) · [개발 운영 방침](../policies/development-policy.md) · [단계 완료 계약](../plans/stage-completion.json) · [현재 DEV1](../implementation/dev1-implementation.md) · [AI 개발 방법](../policies/ai-development-method.md)

**AI가 직접 성능을 분석하고 병목을 찾고, 원인을 구분하는 실험을 수행하고, 코드를 개선한 뒤 실제 개선 여부와 회귀를 확인하기 쉬운 구조를 만든다.** 사람이 매번 측정 명령·통계 의미·코드 위치·재현 환경을 설명해야 하는 절차는 목표를 충족하지 않는다. 이 문서는 그 요구를 계측 계약, 재현 가능한 작업 묶음, 독립 비교 판정, 단계별 완료 의무로 구체화한다.

Markdown은 규범 원문이며 JSON은 요구 ID·필드·단계·미실행 사례의 색인이다. 이는 성능 도구 구현이나 제품 성능 향상의 실행 증거가 아니다. 기존 커널의 정확성·격리·내구성·오류 계약, 개발 순서, 사내 새 DB 정책을 유지한다. 기능 구현과 측정을 같이 누적하며 모든 최종 성능 목표를 SQL 착수의 선행 조건으로 만들지 않는다.

<a id="section-1"></a>

## 1. 현재 상태와 출발점

| 대상 | 확인한 내용 | 이번 설계에서의 의미 |
| --- | --- | --- |
| XADB DEV1 | 단조 시계 port와 Instant 구현, operation/object/time/event/error, 제한된 EventBuffer와 drop 집계 | 재사용할 최소 기반. parent/link·CPU/대기 집계·분포·비교 실행기까지 완성된 것은 아님 |
| XADB 종합 설계 18절 | 동일 의미, 반복 paired 비교, tail·메모리·I/O·복구 확인 | 이 문서에서 AI가 사용할 구체적인 입력·출력·판정·인수 의무로 확장 |

기존 소스의 `idvOperTimeIndex`, `IDV_SQL_OPTIME_BEGIN/END`, 대기 이벤트와 `iduMutex::lock`의 통계 문맥을 확인했다. 계측 지점과 문맥 전달에 따라 관측 범위가 정해지며, 누적 통계만으로 모든 비동기 인과관계나 개별 요청의 tail 원인이 복원되지는 않는다. `idvProfile`의 공유 버퍼·뮤텍스가 실제 병목이라는 실측 주장은 하지 않는다. 상세 프로파일의 비용을 확인해야 한다는 근거다. 로컬 원본 경로·revision·파일 지문은 JSON에 보존한다.


<a id="section-2"></a>

## 2. 설계 요구와 완료의 의미

| ID | 요구 | 구현 시 확인할 계약 |
| --- | --- | --- |
| PE01 | 계측 항목에서 코드·계약·실험으로 이동 | 항목 ID, 소유 모듈, source/build identity, API·불변식·benchmark 연결 |
| PE02 | 시간과 작업량의 의미 고정 | 단위·범위·분모·중첩·병렬성·시계·오류/취소 집계 정의 |
| PE03 | 요청과 비동기 작업 연결 | request/operation/transaction, parent·link, batch·worker·자원 관계 |
| PE04 | 제한된 저비용 계측 | 모드별 비용·메모리 상한, 비차단 기록, drop·표본·누락 표시 |
| PE05 | 동일 조건 재현과 비교 | 고정 workload·환경·의미, baseline/candidate 별 지문과 실제 경로 |
| PE06 | 가설과 원인을 구분 | 관측·후보·반증·구분 실험·원인 근거를 분리 |
| PE07 | 의미를 보존하는 국소 수정 | 독립 정답, 관련 동시성·실패·복구 시험, 변경 경로 확인 |
| PE08 | 독립적 개선·회귀 판정 | 효과·불확실성·허용 회귀·유효성을 controller가 계산 |
| PE09 | 새 AI 세션에서 이어 하기 | 원본 증거·계약·실행 recipe·가설 이력·다음 실험을 묶음 |
| PE10 | 하위 단계부터 측정 | DEV2 실제 API부터, DEV3 저장·DEV4 SQL·DEV5 서버·DEV8 전체 검증 |

성능 변경의 완료에는 수정 코드 외에 유효한 비교, 주장한 원인에 대한 증거, 적용 가능한 정확성 회귀, 별도 검토가 필요하다. 기존 위임 범위에서 AI가 반복 실험·수정을 진행한다. 공개 의미·품질 기준을 낮추거나 고객 SLA를 약속하는 판단은 [DP01](../policies/development-policy.md)의 권한 구분을 따른다. 일상 실험마다 사용자 재승인을 추가하지 않는다.

<a id="section-3"></a>

## 3. 계측 계약: 모듈 이름보다 작업과 자원

`SM 50ms` 같은 모듈 합계는 탐색 입구다. 호출이 많아졌는지, 호출당 연산이 비싸졌는지, 기다리는 시간이 늘었는지를 구분할 수 있어야 한다.

| 소유 영역 | 시간·대기 구간 | 함께 기록할 작업량·자원 |
| --- | --- | --- |
| foundation/platform | codec·비교·해시·파일 I/O | 호출 수, 처리 bytes, 명시적으로 계측한 allocation/copy, 오류·partial I/O |
| transaction/publication | prepare·검증·충돌·commit 대기·공개 | 성공·abort·retry 수, batch 크기, lock wait와 hold 구분 |
| durable | encode·append·sync·복구·checkpoint | WAL bytes, flush 횟수, batch 크기, backlog, 읽기·쓰기 증폭 |
| storage/index/buffer | row·index·page 접근, pin·eviction 대기 | 방문 row/page, 반환 row, version chain 길이, hit/miss, split·spill |
| SQL | parse·bind·plan·연산자 open/next/close | plan/node ID, 추정/실제 rows, loops, memory·spill, 하위 호출 수 |
| server/transport | admission·queue·dispatch·응답 | offered/admitted/completed load, queue depth, timeout·취소·bytes·backpressure |
| GC/checkpoint/replication | 백그라운드 처리·대기 | backlog, oldest snapshot, retained WAL, 전경 요청과 공유하는 CPU/I/O·자원 |

항목 정의는 소유 모듈에 둔다. 공통 registry는 안정 ID와 공통 의미만 조립하며 거대한 중앙 계측 구현 파일을 만들지 않는다. 각 항목에는 `metric_id`, 의미 revision, kind(counter/gauge/histogram/span), unit, scope, 집계·reset 규칙, 분모, label 허용 목록, sample 규칙, coverage, source locator, contract/benchmark ref를 둔다. 전체 할당을 포착하지 않는 계측은 범위를 명시한다. 사용자 입력·SQL 원문·row ID를 무제한 metric label로 사용하지 않는다. 요청별 ID는 제한된 trace에 둔다.

API 계약에는 성능상 중요한 allocation·복사·blocking·lock 보유·I/O 경계를 드러낸다. AI가 통계만 보고 실제 효과를 추측하지 않게 한다. CPU sample과 source locator는 최적화·inline 및 정확한 binary/symbol identity에 연결하며 stale 지문이면 다시 해석한다.

<a id="section-4"></a>

## 4. 시간·대기·인과관계를 오해하지 않는 계약

- **경과 시간과 CPU 시간:** elapsed에는 실행·대기·OS 스케줄링 지연이 섞인다. `elapsed - 알려진 wait`를 CPU 시간이라고 표시하지 않는다. CPU는 별도 계측/표본으로 보고하고, 미관측 구간은 unknown으로 남긴다.
- **중첩:** inclusive는 하위 작업을 포함한다. exclusive는 동일 실행 문맥에서 중첩 구간의 합집합을 제거할 수 있을 때만 계산한다. 중첩 child 합을 단순 차감하지 않으며 비동기 child의 시간을 부모 CPU에서 빼지 않는다.
- **병렬:** worker 시간 합은 자원 소비량이다. 응답 시간과 동일하지 않다. 요청 완료를 결정한 선행 관계와 경로를 별도로 기록한다. trace가 불완전하면 정확한 critical path라고 주장하지 않는다.
- **공유 작업:** 여러 commit이 같은 WAL flush를 기다리면 요청별 대기와 물리 flush 작업을 다른 항목으로 기록한다. flush 비용을 각 거래에 전액 중복 귀속하거나 근거 없이 균등 배분하지 않는다. batch ID로 연결하고 귀속 정책을 밝힌다.
- **시계:** clock domain·process/run ID·기준점을 기록한다. 현재 Instant 구현은 인스턴스 기준이므로 비교할 문맥은 공유 기준 또는 명시 변환을 사용한다. 다른 process/host 시각은 보정 오차 없이 빼지 않는다. 순서·인과 link와 시간 비교 가능성을 구분한다.
- **분포:** count/sum/max와 histogram의 bucket·정밀도·표본 수를 보존한다. percentile끼리 평균내지 않는다. 표본에서 계산한 p99에는 표본 정책·오차를 표시하고 원본 전체 분포로 오인하지 않는다.
- **대기 원인:** wait event는 증상이다. 잠금을 오래 쥔 소유자·대기한 자원·깨운 작업을 가능할 때 연결하고, 빠진 경계를 표시한다. 누적 system 통계는 동일 측정 구간의 delta와 reset 여부를 사용한다.

추가 문맥은 request/operation/transaction ID, parent span, async link, worker ID, plan/node ID, object generation, resource ID, batch ID, outcome이다. 전부 모든 이벤트에 강제하지 않고 소유 작업에 필요한 필드를 계약으로 정한다. 통계와 trace는 정답 oracle이 아니며 계측 자체가 동기화·commit 순서를 결정해서는 안 된다.

<a id="section-5"></a>

## 5. 계측 모드와 비용 예산

| 모드 | 용도 | 비용·관측 계약 |
| --- | --- | --- |
| 최소/off 기준 | 계측 추가 비용의 대조 | 제거할 수 없는 기본 비용을 명시. correctness 기능은 유지 |
| 상시 | 기본 counter·선택한 분포·제한된 최근 이벤트 | 사전 할당, 스레드별 집계 우선, cardinality·메모리 상한, hot path의 동기 출력·전역 lock 회피 |
| 범위 지정 | 특정 operation·객체·시간 창 상세 추적 | sample/filter/기간/bytes 상한, 비차단 유실과 drop 수, 부분 trace 표시 |
| 재현 lab | CPU·off-CPU·allocation·cache/스케줄링 분석 | 가용 profiler와 권한을 사전 확인. 미지원 수집은 UNSUPPORTED로 남김 |

각 모드는 처리량·p99·CPU·메모리·기록량 예산을 workload별 실행 전에 고정한다. 보편적인 1% 같은 수치를 성능 보장으로 발명하지 않는다. native release의 계측 수준을 기록하고 off/상시/상세를 따로 비교한다. 수집기 지연·버퍼 포화·비활성 수집기에서도 엔진의 정답·오류·내구성 의미가 같아야 한다. 상세 trace에서만 발견되는 현상은 계측 효과와 분리해 검증한다.

유실 자료는 0으로 채우지 않는다. 필수 증거가 유실되면 해당 원인 판정은 불충분하다. 독립적으로 완전한 end-to-end 관측이 있으면 성능 효과만 따로 평가할 수 있다. 수집 실패가 정답 실패를 숨기거나 제품 오류를 성공으로 바꾸면 안 된다.

<a id="section-6"></a>

## 6. AI가 받는 작업 묶음과 실행 경계

아래는 **구현할 논리 산출물**이며 현재 존재하는 CLI나 파일을 실행했다고 뜻하지 않는다. runner는 실제 capability 목록·schema·필수 입력·실행 가능 recipe를 기계 판독 형태로 제공한다. 존재하지 않는 명령을 AI가 추측하게 하지 않는다. 최초 native API fixture는 SQL/server 없이 실행한다.

| 산출물 | 필수 내용 |
| --- | --- |
| manifest | baseline/candidate source commit+dirty digest, binary/symbol/toolchain/config, host·CPU/NUMA·OS/FS·storage, fixture·oracle·recipe 지문 |
| workload/profile | 데이터 크기·분포·seed, concurrency, warm/cold 조건, warmup·측정 길이, 도착률/closed-loop, timeout, 의미·주 지표·허용 회귀·반복/중단 규칙 |
| metric catalog | 항목 의미·단위·coverage·모드·소유 모듈·실제 코드·계약·benchmark 연결 |
| observations | 원본 구간·run별 latency 분포, 작업량, 성공·실패·retry·backlog, 계측 손실, 선택적 profile/trace와 지문 |
| hypotheses | 관측과 추론 구분, 후보·반례·빠진 증거, 구분 실험과 예상 결과, 실행 결과·기각/유지 이유 |
| change | 한 원인에 대한 diff, 예상 비용 변화, 건드린 불변식·API·실패 경계, 실제 변경 경로 witness |
| comparison | paired 결과, 효과 크기·불확실성, 회귀·정확성·실행 유효성의 별도 판정, 분석 도구 버전 |
| resume | 유효한 증거 목록, 재실행 recipe, 이미 기각한 가설, 남은 의무, 다음 구분 실험 |

대화에는 요약·우선 후보·다음 실험을 제공하고 원본은 탐색 가능한 bundle로 보존한다. bounded query/filter로 해당 지표·operation·시간 구간을 꺼낼 수 있어야 한다. 후보 우선순위에서 빠진 모듈을 원인 불가능으로 처리하지 않는다. source map이 없으면 위치 미확인으로 남긴다.

XADBTest controller는 process 밖의 경과 시간·종료·산출물 identity와 정답을 확인하고 worker는 관측을 제출한다. 성능 workload는 실제 production core를 호출하며 IPC 왕복을 매 연산에 강제해 microbenchmark를 지배하지 않는다. 내부 loop timing과 외부 완료량을 교차 확인하고 setup·통신·측정 구간을 분리한다. fault/scheduler 계측 build와 실제 release 비교는 별도다. benchmark 때문에 모든 제품 호출에 동적 dispatch나 직렬 JSON 출력을 추가하지 않는다.

환경 검사는 필요한 도구·자원·고정 조건을 확인한다. 환경을 맞추려고 공유 host 설정을 임의 변경하지 않는다. 기존 자율 작업 범위의 합성 fixture·소유 process·자원 안에서 반복하며, 재현 불가·미지원 도구·비용 한도는 실행 상태로 남긴다. 이 문서는 예약 실행을 만들지 않는다.

<a id="section-7"></a>

## 7. 분석 → 원인 검증 → 개선 → 재검증

1. **비교 계약 동결:** 해결할 workload와 주 지표, guardrail, 정확성·내구성·격리, 환경, 반복·중단 규칙을 고정한다. baseline도 정답 검사를 통과해야 한다.
2. **기준 측정:** 반복 run에서 현상이 재현되는지 확인한다. 성공 TPS·end-to-end p99·오류·backlog와 CPU/I/O를 보고 연산·대기·과도한 작업량을 구별한다.
3. **병목 후보 선정:** operation별 시간·호출 수·단위 작업 비용·경합, CPU/off-CPU profile과 비동기 link를 대조한다. 큰 시간 비중 하나만으로 원인 확정을 하지 않는다.
4. **가설 구분:** 후보별로 한 요인을 바꾼 실험과 예상 counter 변화를 미리 기록한다. workload 축소는 같은 병목 기전을 보존해야 하며 원본 workload에서도 재확인한다. 상관·추정과 검증한 원인을 구분한다.
5. **국소 수정:** 의미를 유지하는 가장 작은 변경부터 한다. 구조 복잡화는 DP07의 대안·비용·재검토 조건을 따른다. 실험 중 바꾼 workload/계측 설정을 제품 개선으로 세지 않는다.
6. **전후 비교:** 동결 조건으로 A/B 순서를 교차·무작위화하고 원인 지표와 실제 응답 효과를 모두 확인한다. CPU sample만 좋아진 결과를 제품 성능 개선으로 승격하지 않는다.
7. **회귀·재진입:** 독립 정답, 해당 동시성·자원·fault·복구, 인접 workload, 원본 통합 경로를 검사한다. 별도 검토와 새 세션 replay를 거쳐 bundle을 보존한다.

가상 예: commit p99 증가의 후보를 장치 sync 지연, batch 형성 대기, commit lock 경합으로 나눈다. 각각 sync 분포, enqueue→batch 연결 시간, lock wait/hold와 소유자 작업이 구분 증거다. batch 크기만 늘려 flush/transaction이 줄어도 응답 p99가 나빠지면 해당 latency 목표의 개선은 아니다. 실제 데이터·개선 수치를 관측한 예가 아니다.

실험 예산을 소진하거나 가설이 기각되면 그대로 보고한다. 의미 있는 개선이 없으면 후보를 채택하지 않고, 충분한 비교가 안 되면 INCONCLUSIVE로 남긴다. 원인을 확인하지 못했는데 ‘원인 해결’이라고 기록하지 않는다.

<a id="section-8"></a>

## 8. 비교 통계와 독립 판정

DP07의 성능 복잡화 변경은 최소 10쌍 비교를 적용한다. 이는 통계적 충분성 보장이 아니다. 다른 기준 측정의 반복 수·기간도 실행 전에 정한다. 중간 결과를 보고 유리할 때만 중단하거나 실패 run을 삭제하지 않는다. 추가 측정은 사전 규칙 또는 이유·새 campaign으로 기록한다.

run/block을 표본 단위로 paired 효과와 신뢰구간을 계산한다. 개별 요청 수백만 건을 독립 실험 수백만 회로 세지 않는다. 주 지표·최소 유용 효과·guardrail·통계 방법을 사전 고정하고 다수 지표 중 우연히 좋아진 하나만 선택하지 않는다. 신뢰구간이 판정 경계를 넘나들면 INCONCLUSIVE다. 온도·CPU 주파수·NUMA·동시 작업·cache drift 같은 환경 차이는 관측·배제 규칙과 함께 보존한다.

**AI의 후보 탐색과 최종 확인 campaign을 분리한다.** 탐색한 후보·실패·측정 수를 모두 남기고, 탐색에서 가장 좋았던 수치를 채택 근거로 재사용하지 않는다. 기본 경로는 후보 binary와 분석 규칙을 고정한 뒤 같은 workload 계약에서 탐색에 쓰지 않은 새 run으로 baseline과 비교하는 것이다. 별도 seed/data 표본을 쓸 때도 분포·선정 규칙을 사전 고정한다. DP07의 최소 10쌍은 성능 복잡화 후보의 이 최종 확인에 적용한다. 확인 결과로 코드를 다시 조정하면 그 결과는 탐색 자료가 되며 새 확인이 필요하다. 여러 후보/확인 campaign 중 하나라도 유리하면 채택하는 반복을 막도록 확인 시도 수·전체 오류율/다중비교 또는 순차검정 정책·중단 규칙을 미리 정한다. 그런 정책이 없거나 실패한 확인을 지운 경우 개선을 확정하지 않는다. 확인 실행은 candidate의 기준 변경 권한과 분리하고 원본 결과를 보존한다.

도착률을 고정한 부하와 closed-loop를 구분한다. intended arrival→완료 지연, offered/admitted/completed rate, backlog·timeout을 함께 보고하며 load generator가 목표를 못 지킨 run은 유효성 판정에 반영한다. timeout·실패·retry를 지연 표본에서 조용히 빼거나 처리 부하를 낮춰 p99 개선을 만들지 않는다. 고정 부하 latency 비교와 포화 처리량 비교는 별도 campaign이다.

| 판정 축 | 값 | 의미 |
| --- | --- | --- |
| validity | VALID / INVALID / UNKNOWN | 동일 계약·환경·입력·경로·필수 관측 조건 충족 |
| correctness | PASS / FAIL / UNKNOWN / NOT_RUN | 기존 독립 정답·관련 회귀의 결과 |
| performance_effect | IMPROVED / NO_MATERIAL_CHANGE / REGRESSED / INCONCLUSIVE / NOT_RUN | 동결한 주 지표의 효과·불확실성 |
| guardrails | PASS / FAIL / UNKNOWN / NOT_RUN | tail·메모리·오류·쓰기량·복구 등 등록한 허용 회귀 |
| cause_status | SUPPORTED / REJECTED / UNRESOLVED / NOT_RUN | 구분 실험이 뒷받침하는 원인. 다른 범위까지 일반화하지 않음 |
| promotion | READY_FOR_REVIEW / REJECTED / INCOMPLETE | 주장에 필요한 모든 축·별도 검토 전 승격 상태 |

INVALID/UNKNOWN 비교는 성능 효과를 INCONCLUSIVE로 제한한다. 성능 수치가 좋아도 correctness FAIL이나 guardrail FAIL이면 채택하지 않는다. 모듈 비용 감소와 제품 응답 향상은 별도 결과이며 병목 이동도 기록한다. target attainment와 상대 개선을 구분하여, 새 capability의 최초 기준선은 비교 대상이 없다는 이유로 개선 PASS를 만들지 않는다.

candidate worker는 baseline·허용 회귀·oracle·최종 판정을 자기 출력으로 교체할 수 없다. 분석기·runner 변경도 known-bad와 별도 검토 대상이다. 로컬 controller 분리는 보호 CI 권한 분리가 완료됐다는 뜻이 아니다. 승인된 개선도 기준선 갱신은 별도 기록으로 남기고 이전 근거를 보존한다.

<a id="section-9"></a>

## 9. 배치와 개발 단계 연결

공통 ID·시계·계측 전달 의미는 foundation, OS 수집은 platform, operation 정의와 저비용 수집은 소유 engine/SQL/server 모듈, 실행·비교·증거 검증은 XADBTest가 맡는다. analyzer가 engine 내부 구현을 호출해 정답을 계산하지 않는다. 파일·crate는 실제 구현 시에만 추가하고 [저장소 배치 정책](../policies/repository-build-plan.md)을 따른다. 별도 거대 telemetry 서비스나 상용 대시보드를 커널 선행 조건으로 만들지 않는다.

| 적용 시점 | 최소 의무 | 아직 요구하지 않는 것 |
| --- | --- | --- |
| DEV1 후속 보완 | 현재 clock/EventBuffer의 범위 표시, 실제 소비자가 생길 때 공통 의미 추가 | 과거 F1_LOCAL_VERIFIED를 새 성능 시험 PASS로 변경 |
| DEV2 K0–K5 구현·완료 | 각 실제 API의 비용·작업량 기준선, WAL/commit/복구 연결, 재현·비교 controller와 PF01–07/12/13 | SQL·네트워크·최종 SLA, 모든 미래 계측 완성 후 K0 착수 |
| DEV3 | PF09: 크기·분포·경합·working set별 저장/index 병목과 원본 통합 확인 | 모든 최종 성능 목표를 SQL 착수 gate로 추가 |
| DEV4 | PF10: SQL plan/node와 실제 연산·저장 작업 연결 | 미래 고급 SQL 연산자까지 구현 완료 |
| DEV5 | PF11: admission·통신·worker·비동기 경로와 외부 응답 연결 | 초기 커널의 서버 의존 |
| DEV8 | PF08/14: DP05의 성능 과제 5개와 장시간 회귀·보호 판정 검증 | pilot 결과를 일반적 AI 성능 보증으로 해석 |

단계 완료 의무는 [stage-completion.json](../plans/stage-completion.json)에 연결한다. 미래 runner의 실제 명령·수치 예산·case recipe는 해당 구현 전에 확정한다. 현재 `run_entrypoint`는 null이며 기존 DEV0/DEV1 checker가 이 계획을 실행한다고 표시하지 않는다. COMM/PART/NG 등 후속 경로도 PE01–10을 적용하고 해당 기능의 완료 시 실제 하위 통합·계측 회귀를 누적한다.

K0–K5의 제품 계측은 각 K의 제품 작업, manifest·재현 bundle은 XADB-014a, 독립 비교·replay는 XADB-032가 담당한다. [단계 JSON](../plans/development-stages.json)의 `kernel_build_steps.performance`가 담당·PF variant 연결의 정본이다. XADB-014b 자동 최소화와 XADB-015의 K5 이후 benchmark campaign/AI pilot은 이미 있는 초기 계측·비교·재현을 확장한다. 이들 전체의 완성을 K0 또는 DEV2의 새 선행 조건으로 만들지 않는다.

<a id="section-10"></a>

## 10. 미실행 인수 계획

모든 사례는 **NOT_RUN**이다. 문서 검사와 runtime/AI pilot의 성공을 구분한다. PF08의 원인 정답은 담당 AI에게 숨기며, 알려진 인위적 원인을 실제 운영 장애 전체의 정답률로 일반화하지 않는다.

| ID | 단계 | 사례와 합격 기준 |
| --- | --- | --- |
| PF01 | DEV2 | 항목 단위·범위·분모·소유 코드 연결, 중복 ID·stale source·미계측 값의 0 대체를 거절 |
| PF02 | DEV2 | 중첩·병렬·공유 flush·다른 clock domain의 고정 이력으로 중복 합산·잘못된 차감을 검출 |
| PF03 | DEV2 | 버퍼 포화·수집 지연·수집기 부재에서도 상한·drop 공개, 동일 정답·오류·내구성 보존 |
| PF04 | DEV2 | off/상시/상세의 실제 release 측정, 사전 비용 예산 준수 또는 명시 실패·불충분 |
| PF05 | DEV2 | 각 run의 등록 manifest와 다른 binary/fixture, 서로 다른 의미·부하·미도달 경로의 비교를 INVALID로 거절 |
| PF06 | DEV2 | sync 생략·요청 누락·오류/timeout 은폐·부하 감소로 만든 거짓 개선을 채택하지 않음 |
| PF07 | DEV2 | 알려진 동등·개선·악화·불확실·순서 drift 및 동등한 다수 후보의 우연한 winner에서 판정·독립 확인·탐색 선택 편향 처리 검증 |
| PF08 | DEV8 | 원인을 숨긴 CPU·대기·queue·과도한 작업량 사례에서 후보·구분 실험·수정·독립 판정, DP05 성능 과제 5개 기록 |
| PF09 | DEV3 | 데이터/동시성/편향/working set 변화의 작업량과 단위 비용, memory·I/O 회귀 확인 |
| PF10 | DEV4 | plan/node→실제 rows/loops/spill→storage 연결, 변경 전후 결과 동등성과 비용 설명 |
| PF11 | DEV5 | thread/queue/async 경계를 넘는 요청 연결, 외부 응답과 worker 합산 시간 구분 |
| PF12 | DEV2 | 국소 수정 전후 원인 지표·end-to-end 효과와 독립 정답·관련 fault/recovery를 함께 확인 |
| PF13 | DEV2 | 새 세션이 bundle만으로 baseline/candidate와 가설 이력을 재현, 미지원 수집·손실은 명시 |
| PF14 | DEV8 | 장시간 tail·backlog·메모리·복구 회귀와 candidate의 baseline/기준 자기 갱신을 검출 |

실행 전 각 사례의 정확한 fixture·환경·필수 지표·수치 허용치·반복·known-bad를 정하고 recipe 지문을 고정한다. 미구현·미지원 필수 입력은 해당 완료 판정을 막으며 성공으로 바꾸지 않는다. 이 유한한 계획이 모든 workload의 성능 증명은 아니다.

AI 사용성은 성공 판정뿐 아니라 첫 재현까지의 시간, 원인 검증까지의 실험 수·시간, 사람이 추가 설명한 횟수, 새 세션 재현율, 거짓 개선 채택·원인 잘못 제외·필수 검증 누락, 전체 비용으로 평가한다. 중단·실패도 분모에 포함한다. 현재 이 지표의 실측이나 효율 향상률은 없다.

<a id="section-11"></a>

## 11. 결정·검토·문서 검증 기록

PE-D01은 사용자 요구를 내부 계측·성능 개발 계약으로 구체화한 D2 결정이다. 작업·계약·통합 담당은 `/root`다. 대안은 (a) 운영 통계와 수동 분석만 유지, (b) 커널부터 공통 계측·재현·독립 비교를 함께 개발, (c) 모든 함수의 상시 상세 trace다. (b)를 선택한다. AI의 반복 실행과 원인 검증에 필요한 문맥을 제공하면서 hot path의 비용·기록량을 제한할 수 있기 때문이다. 계측 예산 실패·원인 오판·새 세션 재현 실패가 반복되면 항목과 수집 전략을 재검토한다.

기존 편집 중인 운영 방침·매뉴얼 관련 변경을 보존하고 이 계약과 참조를 좁게 추가한다. 공유 색인·단계·검증기는 최신 Markdown과 JSON에 맞추고 문서 검사를 실행한다. 제품 코드·저장 형식·기존 실행 증거를 변경하지 않는다. 독립 검토와 문서 검사 결과는 관리 JSON의 `review`·`validation`에 기록하며, PF 사례의 실행 결과와 구별한다.

문서 검증은 요구·사례 ID, 단계 연결, 미실행 상태, 파일 참조와 Markdown 지문을 검사한다. checker의 부정 사례도 문서 무결성 검사이며 제품 성능 시험이 아니다. 저장소 루트에서 다음을 실행한다.

```bash
python3 devdoc/verify_documents.py
```

# DEV3 AI 개발 효율 개선 계획

2026-10-23 · **AI 작업 방식의 개선 후보 / 첫 운영 적용 진행 / 효과 비교 NOT_RUN**. 기준 commit은 `00a35487b29a`다. 그 뒤 작업 트리에서 진행 중인 DEV3 변경은 완료된 기능이나 이 분석의 실행 결과로 세지 않는다. 제품 계약, 필수 시험, 독립 검토와 [DEV3 최종 S5 전수 인수](dev3-test-plan.md#final-suite)는 그대로 적용한다.

2026-10-23 후속 사용자 지시로 [첫 빈 거래 통합 작업](../evidence/dev3-empty-transaction-20261023.md)에
입력 묶음·단일 통합 담당·독립 fixture 준비·선택 실행 사전 점검·시간 기록을 적용하기 시작했다.
이는 기존 품질 기준 안의 운영 적용이며 제품 범위·필수 시험·정책을 바꾸지 않는다. 2026-10-24 새 process 복구/재진입의 필수 선택 검증과 최종 독립 D1을 수용하여 소비자 단위
완료1/3에 도달했다. 효과 비교는 NOT_RUN으로 유지한다. 아래 가설과 일정 단축은 아직 검증되지 않았다.

## 목표와 성공 기준

**목표는 AI를 사용해 지금보다 적은 시간으로 검증된 DEV3 기능을 다음 소비자에게 전달하는 작업 방식을 찾는 것**이다. 결과물에는 제품 코드와 함께 어떤 AI 입력·역할 분담·도구가 실제 시간을 줄였는지에 대한 재현 가능한 판단이 포함된다. 성공은 같은 품질 기준에서 `작업 착수 → 실제 소비자 연결 → 필수 시험·독립 검토 수용`까지의 시간이 줄고, 제품 결함·시험 도구 오류·미실행 인수 부채가 늘지 않는 것이다. commit 수, 생성한 코드량, 하위 시험 case 수는 속도 지표로 쓰지 않는다.

DEV3의 설계와 공개 DBMS 조사는 구현의 입력을 마련했으나, 현재 완료된 단위는 주로 제한된 저장 owner·표현·시험 경계다. 제품 수준의 **첫 행 거래 영속 ACK와 재시작 복구**는 아직 연결되지 않았다. AI 작업의 최적화 대상은 더 많은 단위를 동시에 시작하는 것이 아니라 **선행 작업에서 실제 소비 가능한 기능을 만드는 데 걸리는 총 시간**이다. [현재 16개 분류와 미공급 경계](../implementation/dev3-implementation.md#dev3-work-breakdown), [기존 우선순위](dev3-implementation-plan.md#work-priority).

## 현재 AI 개발에서 시간을 쓸 가능성이 큰 지점

| 확인한 사실 | 속도에 대한 해석과 확인할 가설 |
| --- | --- |
| 2026-10-18의 [DESIGN-003](../evidence/dev3-wal-undo-research.md)은 in-place page·WAL·영속 undo 방향을 채택했지만 제품 구현·시험은 당시 `NOT_RUN`이었다. 이전 [COW 기반 소스 조사](../evidence/dev3-source-research.md)의 일부 결론은 설계 변경으로 대체됐다. | 설계 확정은 구현 의사결정 비용을 줄이지만 실제 상태 전이, 오류 효과, 재장애와 자원 수명은 코드와 독립 시험으로 다시 닫아야 한다. 조사 산출물을 현재 계약에 연결하는 비용도 있다. |
| [공개 소스 catalog](../reference/opensource-references.md)에는 10개 checkout이 있다. 이번 질문과 관련해 확인한 PostgreSQL `45fc51fc…`의 `bufmgr.c`, MariaDB `327437ad…`의 `buf0buf.cc`, SQLite `43a48f77…`의 `pager.c`는 각각 자체 page 식별·버퍼·거래/복구 전제에서 동작한다. [실제 비교 기록](../evidence/dev3-wal-undo-research.md)은 XADB에 채택한 원칙과 가져오지 않은 전제를 분리한다. | 소스는 실패 순서와 구현 대안을 빠르게 찾는 자료다. XADB의 Rust 소유권, 고정 자원 한도, 영속 형식, WAL/DWB와 독립 oracle에 맞게 변환·검증하지 않고 직접 붙일 수 있다는 근거는 없다. 광범위한 재조사보다 현재 막힌 상태 전이 하나를 겨냥한 열람이 유리하다는 가설이다. |
| 10월 23일의 [등록·Status·복구 공급](../implementation/dev3-implementation.md#dev3-work-breakdown)은 각 제한 단위에서 독립 검토와 선택 시험을 통과했다. 한편 일반 pool, live undo/commit, tree·row·index/catalog, 일반 recovery/READY는 [단계 기록의 DEV3 항목](stage-completion.json)에서 미완료다. | 하위 작업의 완료 수가 늘어도 최장 선행 경로가 짧아지지 않을 수 있다. 다음 소비자가 실제 API를 사용하고 disk/reopen 결과를 얻는 순간을 진척 기준으로 삼아야 한다. |
| [Status load 실행 기록](../evidence/dev3-status-load-20261023.md)은 r0~r7에서 제품·fixture·I/O ordinal·자원 계산의 수정을 구별한다. [대상 집합 기록](../evidence/dev3-target-set-20261023.md)은 제품 변경 뒤 compiler runner의 출처·정확 진단 검산을 보완했다. | 일부 반복은 필수 결함 발견이었고 일부는 시험 도구의 입력 오류였다. 시험 도구의 구조·출처 검사를 제품 실행 전에 자동화하면 후자만 줄일 가능성이 있다. 실제 시간 절감량은 기록되어 있지 않다. |
| 각 단위는 source·binary·fixture·실패 원본·독립 D1/D2를 보존한다. [S5](dev3-test-plan.md#final-suite)는 기능·재장애·자원·성능과 긴 fuzz/stress·큰 fixture까지 포함한다. | 이 검증은 영속 DBMS의 완료 조건이다. 중간에는 변경 기능의 필수 사례만 선택하고, 반복을 줄일 대상은 검증 자체가 아니라 수동 준비·중복 빌드·출처 대조다. |

위 표의 병목은 아직 **시간 계측으로 확정되지 않았다**. AI 효과도 과제에 따라 다르다. [작은 JavaScript HTTP 서버 대조 실험](https://www.microsoft.com/en-us/research/publication/the-impact-of-ai-on-developer-productivity-evidence-from-github-copilot/)과 [숙련된 공개 저장소 개발자 현장 실험](https://metr.org/Early_2025_AI_Experienced_OS_Devs_Study-paper.pdf)의 결과를 XADB 속도 배율로 대입하지 않는다.

## 현재 개발 방향에서 바꿀 결정

[10월 22일 방향 점검](../evidence/dev3-direction-review-20261022.md#assessment) 이후 **제한 복구 → 일반 page 수명·live undo → 첫 행 거래 → index/catalog 확장 → checkpoint·회수·backup → S5** 순서는 이미 [구현 계획](dev3-implementation-plan.md#work-priority)에 반영됐다. in-place page·WAL·영속 undo, 실패 후 fence, 자원 한도와 독립 검증도 유지한다. 따라서 새 설계안을 늘리는 대신, 채택한 순서가 실제 소비자 기능으로 이어지도록 다음 운영 결정을 제안한다. 이 절은 **미채택 제안**이며 제품 계약이나 단계 인수를 수정하지 않는다.

| 우선 | 바꿀 운영 결정 | 바로 적용해 볼 경계와 판정 |
| --- | --- | --- |
| 1 | **작업 완료의 기준을 하위 공급 수에서 다음 소비자 도달로 이동한다.** 제한 owner의 D1 수용은 그대로 기록하되 DEV3 기능 진척과 구별한다. | 현재 진행 중인 제한 Status 공급은 그 자체의 범위로 닫는다. 다음 통합 작업은 [3.12.1–4 제한 복구](dev3-implementation-plan.md#work-priority)의 실제 producer 파일을 새 process에서 복원·검산하는 종료점을 먼저 고정한다. 열기·decode·mock replay만 성공하면 소비자 미도달로 남긴다. |
| 2 | **선행 차단이 확인된 코드만 확장한다.** 일반 helper·추상 API·추가 opcode는 다음 종료점을 막는 상태 전이가 명시될 때 착수한다. | 제한 복구 이후 [3.05 일반 pool](../contracts/dev3-task-05-buffer-flush.md#operational-pool)은 target 숫자 확대 대신 load→dirty→flush→evict→reload, N+1 압력과 반복 Status까지 한 수명으로 닫는다. 동시에 필요한 live undo·guard·tree의 최소 공급을 정의하되 index/catalog 전체 완료를 첫 행의 선행으로 만들지 않는다. |
| 3 | **첫 외부 가치를 하나의 작은 거래로 증명한다.** 새 기능별 완료 표 대신 실제 table 생성과 한 행의 실패·재시작 이력을 연결한다. | [첫 행 거래 계약](../contracts/dev3-task-11-commit.md#first-row-integration)의 최소 catalog→row/undo/commit→일반 recovery 순서로 `Begin→변경→Commit ACK→kill→reopen`, loser 제거와 부분 CLR 재장애를 판정한다. F05·terminal queue·REBASE/READY를 건너뛴 축약 경로는 이 목표의 완료로 세지 않는다. |
| 4 | **공유 상태의 동시 구현을 제한하고 검증 준비를 병행한다.** 두 작업이 같은 page/WAL/transaction 의미를 바꾸면 한 통합 담당이 순차 병합한다. | 다른 담당은 독립 oracle·fault schedule·fixture·참고 코드의 특정 질문을 먼저 준비한다. 별도 worktree여도 같은 불변식을 고치면 병렬 성과로 계산하지 않는다. 병합 대기·재작업 시간이 길면 구현 동시 수를 줄이고 준비 역할을 늘린다. |
| 5 | **인수 준비와 증거 정리를 코드 작업과 함께 자동화한다.** selector·fixture hash·source/binary·실패 ordinal 사전 점검과 공통 receipt 형식을 우선한다. | 다음 변경 코드의 필수 사례에 한해 사전 점검→한 번의 동결 build→정확 selector→원 PASS/FAIL/NOT_RUN 보존을 실험한다. 같은 규칙은 owner 문서 한 곳에 두고 작업 기록에는 변경점·원본 결과·링크만 남긴다. checker/oracle 의미·시험 분모·S5 전수 의무를 바꾸는 제안은 별도 검토 대상으로 남긴다. |
| 6 | **일정을 완료된 소비자 단위의 실측으로 다시 만든다.** 담당자·모델·코드 생성량을 속도 배율로 가정하지 않는다. | 아래 세 작업에서 lead time, AI 재작업, 독립 검토 대기, runner 오류를 기록한다. 64GiB profile·계측은 해당 소비자 착수 전에 별도 준비하고 긴 실행은 S5에 둔다. 측정 전에는 이번 주 완료나 일정 단축을 약속하지 않는다. |

이 순서를 실행할 때 매 작업 착수 기록에 **이번 소비자, 막는 선행 하나, 변경할 상태 전이, 필수 case, 독립 검토자, 미실행 인수**를 한 화면으로 고정한다. 선행을 해소하지 않는 새 하위 작업은 대기 목록으로 돌린다. 기존 [작업 분해](dev3-implementation-plan.md#work-organization)와 소유 계약을 버리는 변경은 아니며, 작업 선택과 완료 보고의 기준을 더 엄격하게 적용하는 제안이다.

AI 개발 방법의 학습을 먼저 보고하려면 **중간 산출물의 이름과 분모를 따로 둔다**. 위 제한 복구·일반 page·첫 행 거래는 각각 방법 실험과 제품 capability의 중간 결과가 될 수 있지만 DEV3/S5 완료가 아니다. 완성 목표 자체를 줄여 빠른 출시를 원한다면 DEV3 계약·확정 시험 inventory·제품 범위를 바꾸는 별도 방향 결정과 검토가 필요하다. 현재 제안은 그 축소를 이미 채택한 것으로 간주하지 않는다.

## AI 작업 방식의 변경 후보

### 1. 에이전트에게 작은 작업 입력을 준다

각 구현 작업에는 `기준 source·바꿀 상태 전이·실제 다음 소비자·관련 계약 절·원 실패·정확한 필수 case·완료 후 남는 의무`만 묶어 전달한다. 에이전트가 전체 설계와 과거 대화를 다시 탐색하는 시간을 줄이고, 계약과 무관한 구현 확장을 막기 위한 입력이다. 발견한 결함·변경된 API·시험 결과는 같은 묶음에 갱신해 다음 실행이 즉시 이어받게 한다. [기존 AI 작업 절차](../policies/ai-development-method.md#section-2)를 실제 작업 시간과 재탐색 횟수로 평가한다.

### 2. 다음 소비자가 실행할 수 있는 세로 단위로 작업을 고른다

기존 [병합 순서](dev3-implementation-plan.md#parallel-waves)를 유지하되 작업 시작 시 `이번 변경의 실제 소비자 → 파일 효과 → 재시작 또는 실패 후 관측 → 독립 판정`을 한 줄로 고정한다. 다음 소비자가 없는 공통 helper는 특정 차단 결함이 있을 때만 시작한다. 제한 단위의 D1과 제품 기능의 완료는 계속 구별한다.

다음 종료점의 후보는 (1) 일반 page가 load·dirty·flush·evict·reload되고 N+1 압력과 반복 Status를 견디는 경로, (2) 실제 영속 table에서 첫 행 `Begin→변경→Commit ACK→process kill→reopen` 및 loser rollback/CLR, (3) 그 경로를 index/catalog와 checkpoint·회수·backup으로 확장하는 것이다. 각각 필요한 [F05 자원 계산](dev3-implementation-plan.md#resource-proof)과 [복구 장벽](dev3-implementation-plan.md#recovery-remaining)을 effect 전에 준비한다. 첫 행의 실제 선행을 건너뛰거나 준비용 Begin을 거래 완료로 세지 않는다.

### 3. 참고 코드 조사를 에이전트의 한 질문에 연결한다

작업마다 현재 XADB 계약의 미해결 질문을 먼저 적고, 관련 공개 코드의 **실제 revision·함수·채택/기각 이유**를 짧게 기록한다. 예를 들어 eviction 중 WAL 선행이 쟁점이면 버퍼와 flush의 관련 경로만 읽는다. 같은 질문의 자료가 이미 [DEV3 조사](../evidence/dev3-wal-undo-research.md)에 있으면 재사용하고, 설계가 바뀐 결론만 갱신한다. 참고 구현의 코드·형식·시험 결과를 XADB 실행 증거로 승계하지 않는다. 외부 코드나 crate 도입이 이득이라고 판단하면 기존 [DP04](../policies/development-policy.md#section-5) 검토를 거친다.

### 4. 시험 준비와 증거 수집을 자동화한다

제품 실행 전에 manifest의 selector·case 수·fixture hash·source/binary 관계·예상 I/O ordinal을 검사하는 공통 사전 점검을 시범 적용한다. 한 동결 source에서는 필요한 binary를 한 번 만들고 해당 변경의 정확 selector를 선택하며, 종료 뒤 명령·exit·case·hash·시간을 동일한 형식의 receipt로 수집한다. 의미 oracle과 known-bad 판정은 독립 검토에 남기고 제품 출력에서 기대값을 자동 생성하지 않는다. checker/oracle 의미를 바꾸는 자동화는 D2로 검토한다. 이미 완료한 무변경 시험의 재실행 금지와 원 FAIL 보존을 유지한다.

### 5. 통합 작업과 AI 독립 검토를 병행한다

공유 page/WAL/transaction 상태 전이는 한 통합 owner가 순서대로 반영한다. 동시에 진행할 수 있는 두 번째 작업은 독립 literal·fault recipe·compiler probe·reference 질문처럼 공유 상태를 수정하지 않는 준비로 고른다. 독립 검토자는 계약·부정 입력을 구현과 병행해 먼저 검토하고, 마지막에 실제 diff와 결과를 별도로 판정한다. 서로 다른 파일이어도 같은 상태 전이를 바꾸면 병렬 구현으로 세지 않는다. 작업별 source와 build/output 디렉터리를 격리해 DEV2·COMM의 동시 변경이 DEV3 증거에 섞인 부분을 명확히 한다. 이는 [기존 병렬 정책](dev3-implementation-plan.md#parallel-execution-plan)의 실제 적용 제안이며 인원만 늘려 속도가 오른다는 주장은 아니다.

### 6. 모델 배정을 실제 결과로 조정한다

[현재 모델 배정](dev3-implementation-plan.md#model-roles)의 Sol 구현·별도 검토를 기준으로, 작업 종류별 문맥 수집·초안·수정·리뷰에 걸린 시간과 발견 결함을 기록한다. 강한 모델은 미정 설계·복구/동시성 반례에 먼저 사용하고, 정해진 입력의 fixture·receipt 정리는 필요한 정확성 검사와 함께 더 가벼운 방식이 가능한지 비교한다. 모델이나 agent 수만 바꾼 결과를 생산성 향상으로 선언하지 않고, 유사 난도·같은 품질 기준의 [DP05 비교](../policies/development-policy.md#section-6)로 판단한다.

## 다음 세 작업에서 측정할 운영 실험

<a id="bounded-application"></a>
### 기존 기록을 활용하는 적용 범위

2026-10-25 [D3-DIRECTION-002](dev3-implementation-plan.md#direction-refinement)는 이미 시작한 작업 입력·`timing.jsonl`·source별 실패/수정 기록을 활용하는 범위를 수용한다. 새 제품 작업의 소비자·차단 원인·실제 경과·수정 원인을 기존 기록에 남기고, 아래 세 단위의 결과로 다음 개선을 고른다. 과거 미계측 시간을 추정값으로 채우거나 문서/코드 줄 수 비율을 생산성 판정으로 사용하지 않는다.

별도 계측 플랫폼·주간 보고 자동화·새 공통 runner를 이 기록을 위해 만들지 않는다. 기존 자료로 설명할 수 없는 병목은 미확정으로 남기고, 추가 도구가 필요하면 실제 소비자·비용·중단 조건을 먼저 정한다. 세 단위 집계·효과 비교 완료는 첫 행 기능 착수의 선행이 아니다. 이 한정 수용은 위 운영 후보 전체나 효과·일정 단축의 채택을 뜻하지 않으며, 현재 소비자1/3·효과 비교 NOT_RUN을 유지한다.

다음에 착수하는 **서로 다른 세 개의 DEV3 통합 단위**를 한 실험 묶음으로 삼는다. 통합 담당은 착수 전에 소비자·필수 case·독립 검토자·완료 경계를 고정한다. 첫 단위부터 기존 규칙 안에서 가능한 AI 작업 방식을 적용하고, 각 단계의 실제 시작/종료 시각을 기록한다. checker/oracle 의미 또는 개발 방침 변경은 적용 전에 D2로 검토한다. 기존 기록은 source별 결과가 충실하지만 단계별 wall time이 없어 대조군의 수치로 쓰지 않는다. 세 작업의 차이를 보지 않고 전후 속도 향상을 확정하지 않는다.

| 기록할 값 | 정의와 용도 |
| --- | --- |
| 인수 lead time | 작업이 실행 가능해진 시각부터 필수 시험·독립 D1을 수용한 시각까지. 준비 대기와 재작업 포함. commit 수 대신 사용 |
| 단계별 시간 | 작업 문맥 수집·계약/참고 코드, 제품 구현, fixture·oracle, build/시험, 실패 분석, 독립 검토 대기·수정, 증거 정리. 사람·agent 대기와 기계 실행을 분리 |
| AI 재작업 | 같은 계약을 다시 읽은 횟수, 잘못된 source/소비자 가정, 리뷰에서 발견한 결함, 모델별 수정 왕복. 입력 묶음과 역할 배정의 개선 지점 파악 |
| 첫 시도와 수정 횟수 | 제품 결함, 시험/runner 결함, 계약 불명확, 자원/환경 문제를 구분. 모든 원 FAIL과 미실행을 보존 |
| 소비자 도달 | 해당 단위가 실제 다음 owner와 disk/reopen 경로에서 사용됐는지, 어떤 선행을 열었는지. 제한 helper만 통과하면 미도달 |
| 품질·인수 부채 | 새 결함, 변경 코드 필수 case, 독립 검토, 미실행 S5 목록과 자원 한도. 줄어든 시험 분모를 속도 이득으로 계산하지 않음 |

세 단위 종료 후에는 가장 긴 대기/재작업 원인 한 가지를 선정해 다음 세 단위에서 수정한다. 자동화 작업이 한 단위의 인수 lead time보다 커지면 범위를 축소하거나 중단한다. 효과가 보이면 [DP05의 사전 고정 비교 절차](../policies/development-policy.md#section-6)에 따라 유사 난도의 작업과 같은 품질 기준으로 별도 비교한다. 세 단위의 관측만으로 AI 모델·병렬 인원·일정 단축의 인과 효과를 선언하지 않는다.

## 일정 재산정 기준

앞서 대화에서 제시한 **8~12주**는 작업별 착수·구현·검토·재작업·시험 대기 시간을 측정하지 않은 판단이다. 일정 약속이나 관리 기준선으로 사용하지 않는다. AI 작업 방식의 실험 결과가 먼저 있어야 일정 개선 폭도 주장할 수 있다.

일반 page 수명과 첫 행 거래가 각각 실제 새 process 복구까지 닫히면, 남은 선행 경로의 작업마다 관측된 `구현 + 시험·fixture + 독립 검토 + 수정` 시간을 사용해 범위를 다시 계산한다. S5는 [FT01–FT10](dev3-test-plan.md#final-suite)의 확정 inventory, 환경 준비, 장시간 profile과 실패 후 영향 재검증을 별도로 더한다. 그 전에는 범위의 상·하한과 미측정 항목을 보고하고 날짜를 약속하지 않는다.

기존 방침을 바꾸는 경우에는 [DP01의 D2 독립 검토](../policies/development-policy.md#section-2)로 결정한다. 이 제안만으로 제품 개발·시험 실행이나 단계 완료 상태가 바뀌지 않는다.

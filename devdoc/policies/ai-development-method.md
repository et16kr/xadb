# XADB의 AI 개발·검증 방법

2026-10-18 · **작업 절차 채택 / 보호 실행 기반·AI 개발 pilot 미실행**

[종합 설계](../reference/architecture-overview.md) · [Rust 구현 계약](../contracts/implementation-contracts.md) · [실현 가능성 리뷰](../reference/architecture-overview.md)

AI에게 개발을 맡길 수 있으려면 문제, 변경 범위, 실패 증거, 완료 판정을 코드와 함께 제공해야 한다. 긴 설계 보고서와 큰 권한만 주는 방식으로는 충분하지 않다. Rust 채택, 모듈 core, 독립 oracle, 짧은 작업 계약, 보호된 검증 경로를 함께 운영한다.

사용자 위임으로 [개발 운영 방침 DP01–DP07](development-policy.md)을 채택했다. 이 문서의 작업·증거·검토 절차를 적용하며 결정 권한, 시험 기본값, AI pilot 채택 기준은 해당 방침을 따른다. 보호 runner·예시 실행 profile·모델 routing의 구현 및 실제 효과는 계속 미검증이다.


**기능 선택 기준:** 기술 의존성·독립 검증·AI 재현 비용으로 [사내 개발 단계](../plans/development-stages.md)를 진행한다. 제품 기능 검증 전에는 외부 출시하지 않는다. 사내 버전 완료를 판매 가능으로 해석하지 않는다.

<a id="section-1"></a>

## 1. 공식 자료에서 채택한 것과 XADB가 직접 설계할 것

| 자료 | 확인한 내용 | XADB에 적용할 방법과 한계 |
|---|---|---|
| [OpenAI: Iterating Development Workflows with Codex](https://developers.openai.com/cookbook/examples/codex/iterating-development-workflows-with-codex) | 저장소 지침, 단계별 목적·문맥·인수 조건, 실행 이력을 사용 | 작업별 계약과 짧은 재진입 자료. 예시의 PLANS/GOALS 파일명이나 매 단계 수동 승인은 필수 제품 규칙으로 복제하지 않음 |
| [OpenAI: Testing Agent Skills Systematically with Evals](https://developers.openai.com/blog/eval-skills) | 결과물·과정·스타일·효율을 명시하고 trace/artifact와 검사로 평가 | XADB의 기능 추가·디버깅·재현·시험·최적화별 corpus. 설명이 그럴듯한지보다 실제 반례와 결과를 평가 |
| [OpenAI: Codex as a platform](https://developers.openai.com/blog/codex-as-a-platform) | 비대화형 실행, SDK, 지속 세션용 app-server 등 기존 harness 활용 경로 | XADBTest는 DB 실행·oracle·증거에 집중. 범용 모델 대화 loop와 UI를 새로 만드는 것을 첫 목표로 삼지 않음 |
| [OpenAI: Evaluation best practices](https://developers.openai.com/api/docs/guides/evaluation-best-practices) | task-specific 평가를 일찍 시작하고 실제 분포·사람의 판단과 연결 | 명세 오류는 전문가 review, 재현·불변식은 결정적 검사. 한 점수로 correctness를 상쇄하지 않음 |
| [Anthropic: Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents) | 환경 초기화와 점진적 기능 작업, 진행 기록, 실행 검증의 중요성 | 깨끗한 시작·작은 변경·재진입 smoke. 웹 앱 사례의 효과를 DB에 그대로 일반화하지 않고 XADB pilot에서 검증 |

아래 작업 절차의 채택과 커널·진단·보호 검증 기반의 구현·실행 상태는 별개다. 커널 설계의 승인 범위는 해당 소유 계약을 따른다.

**서비스 의존성 주의:** 2026-10-18 확인한 공식 공지에 따르면 OpenAI Evals 플랫폼은 2026-10-31 읽기 전용 전환, 2026-11-30 dashboard/API 종료 예정이다. 평가 방법론은 참고하되 해당 서비스에 신규 필수 의존성을 만들지 않는다. XADB의 task corpus와 결과는 자체 versioned 파일로 보존하고 모델 실행은 교체 가능한 adapter로 연결한다. [OpenAI deprecations](https://developers.openai.com/api/docs/deprecations)

<a id="section-2"></a>

## 2. 에이전트가 매번 읽을 작업 묶음

전체 보고서는 설계 배경이다. 실제 작업에는 다음을 source revision에 맞춰 생성한다. 고정 토큰 수를 품질 목표로 삼지 않고, 변경에 필요한 계약과 경계를 빠뜨리지 않는 범위에서 작게 만든다.

| 항목 | 내용 |
|---|---|
| task | 목적, 명세 변경 여부, 범위, 제외 범위, 선행 작업, 해당 B/G gate |
| context | 정확한 source/build hash, 관련 ADR·module contract·공개 API, 읽을 파일과 근거 |
| change | 현재 실패와 최소 입력, 기대 결과, 원인 후보와 미확인 경계 |
| verification | 독립 oracle, 실행할 case ID, witness, 제한 자원, 완료 및 중단 분류 |
| continuation | 지금까지 실행한 명령의 receipt, 결과 지문, 남은 의무, 다음 한 단계 |

의존·상호참조 정보는 `source hash + build configuration + index version`에 귀속한다. 다른 revision의 call graph를 최신 사실처럼 사용하지 않는다. dynamic dispatch·generated code·외부 I/O·미관측 background worker는 unknown edge로 남긴다. old artifact를 새 build의 성공 증거로 재사용하려면 동등성이 검증되어야 한다.

repo의 AGENTS.md에는 탐색·빌드·검증 entry point와 최소 규칙만 둔다. 이름·폴더·API 문서·오류·수명·리뷰는 [코딩 컨벤션](coding-convention.md)을 따른다. 모듈 계약은 owner, inputs/outputs, invariant, effects, error, lifetime, concurrency, test ID 중 해당 항목을 Rustdoc과 소유 문서에 기록한다. 단순 helper마다 빈 템플릿이나 별도 JSON을 만들지 않는다. 이름·경로로 책임을 찾고 실제 symbol/reference와 테스트로 확인한다. 전체 symbol index는 구축 범위와 미확인 경계를 별도로 표시한다.

구조·API·시험 경계를 바꾸는 작업에서는 [XADB-METHOD-001의 선별 적용 기준](coding-convention.md#selective-design-reference)에 따라 다음 세 질문을 착수와 리뷰 때 확인한다. 답은 기존 `context`·`change`·`verification`에 연결하며 별도의 중복 문서나 모든 작업에 대한 빈 체크리스트를 추가하지 않는다.

1. **이번 변경이 추가하는 제품·시험 의존성은 무엇인가?** 직접·전이 경로와 적용 build 조건을 소유 계약의 허용 범위와 대조한다. 아직 확인하지 못한 경로를 명시한다.
2. **이 기능에 내부 불변식 접근이 필요한가, 기존 계약의 조합으로 가능한가?** 공유 의미의 하위 추출, 상위 조립, 지역 구현을 비교한다. API 분할로 호출자의 원자성·수명 의무가 늘거나, 작은 중복 제거로 무거운 의존성이 생기는지 확인한다.
3. **해당 계약을 재현하는 데 불필요한 상위 시스템이 필요한가?** 실제 production core의 최소 시험 진입점과, 실제 I/O·동시성·복구 때문에 남겨야 하는 통합 범위를 함께 기록한다.

평가에는 이미 사용하는 탐색 시간·새 세션 재현 시간·재작업·통합 유출 결함을 활용한다. 읽어야 했던 모듈 수와 의존 범위 변화는 원인을 설명하는 보조 관측이며 작을수록 무조건 좋다는 점수가 아니다. 필요한 계약을 읽지 않거나 통합 시험을 생략해 얻은 단축은 개선으로 인정하지 않는다. 책의 재사용·CCD 논의가 AI 효율을 입증하지 않으며, 효과 비교는 기존 DP05 pilot에서 수행한다.

<a id="section-3"></a>

## 3. 하나의 작업이 끝나는 절차

1. **계약 확정:** 현재 실패 또는 추가할 동작을 한 문장과 반례로 만든다. 일반 구현은 이미 정한 계약 안에서 자율 실행한다. SQL 의미·영속 형식·격리 수준·권한 범위 변경은 별도 설계 review 대상이다.
2. **baseline 확인:** 정확한 checkout/build와 fixture로 실패를 확인한다. baseline이 통과하면 실패를 꾸며내지 말고 재현 조건을 조사한다. 기존 정상 범위를 좁은 smoke로 고정한다.
3. **원인 후보 축소:** 오류를 낸 모듈과 원인을 만든 모듈을 구분한다. 입력/출력 invariant와 object generation, mutation owner, 시간 순서로 후보를 줄인다. 관측이 없으면 후보를 제거하지 않는다.
4. **모듈 재현:** 같은 production core를 호출하는 작은 case를 만든다. state/seed/schedule/fault를 저장한다. 모듈로 줄여지지 않는 ordering·filesystem·cross-module 문제는 필요한 component/system 범위를 유지한다.
5. **국소 수정:** 한 책임 경계와 한 의미 변경을 기본 단위로 삼는다. 관련 없는 정리·성능 변경을 섞지 않는다. 후보 확신이 부족하면 가설을 가르는 실험을 먼저 한다.
6. **반증:** 수정 전 fail·수정 후 pass, 인접 정상 case, 알려진 잘못된 구현 또는 변형의 검출을 확인한다. 오류 문구만 바꾼 통과를 허용하지 않는다.
7. **승격:** 필요한 통합·durability·버전·성능 gate와 자원 정리를 검증한다. exact diff, 명세, oracle, 결과 bundle을 리뷰한다. 제품 release gate는 보호된 runner가 판정한다.
8. **재진입 가능하게 종료:** 작은 commit, 남은 의무, 유효한 증거 지문을 남긴다. 후속 작업은 대화 전체 대신 이 자료와 실제 코드에서 시작한다.

명령 예시는 제품 구현 뒤 실제 runner가 제공해야 한다. 존재하지 않는 `xadb test` 명령을 실행했다고 기록하지 않는다. B1 모듈 완료와 B2 커널·H1–H3 디스크 저장·B3 서버 통합 완료는 분리하고, 미실행을 PASS로 취급하지 않는다.

<a id="section-4"></a>

## 4. AI가 검사 대상과 정답을 함께 바꿀 때의 신뢰 경계

에이전트가 테스트를 추가·수정할 수 있어야 한다. 그러나 candidate 구현의 출력으로 expected 파일을 자동 교체한 다음 통과했다고 하면 독립 검증이 사라진다. **테스트 수정 금지**가 아니라 **정답의 출처와 승격 권한 분리**가 필요하다.

| 영역 | 에이전트 작업 | 승격 조건 |
|---|---|---|
| 작업 branch의 제품·회귀 테스트 | 구현, 새 반례, test bug 수정 가능 | 의미 명세와 연결, 변경 이유 및 이전 실패 증거 |
| 독립 reference와 expected vector | 변경 제안 가능 | 제품 변경과 별개 근거·review, candidate 출력 자동 채택 금지 |
| baseline/held-out corpus | 일상 candidate worker에는 읽기 전용 또는 비공개 | 보호된 job이 실행, 접근 정책을 명시 |
| 최종 verdict·release artifact | worker는 관측을 제출 | controller가 protected oracle·실제 관측으로 계산, 기록을 덮어쓰지 않음 |

해시는 동일성 지문이며 작성자의 신뢰성 증명이 아니다. 같은 worker가 binary·oracle·manifest를 마음대로 바꾸면 해시가 모두 맞아도 검증이 무의미하다. 보호된 CI 권한, immutable baseline, 실행 controller의 별도 관측을 결합한다. 내부 witness는 경로 도달 확인에 유용하지만 외부 결과 검증을 대신하지 않는다.

agent가 자기 PR을 검토한 설명은 보조 자료다. 중요 계약 변경에는 독립 reviewer와 보호된 corpus를 사용한다. reviewer가 같은 모델이면 독립적인 오류 확률을 가정하지 않는다. 검토자는 원래 구현 계획 대신 명세·diff·실제 증거에서 판단한다.

<a id="section-5"></a>

## 5. 결과를 서로 다른 축으로 보존한다

| 축 | 예 | 해석 |
|---|---|---|
| `case_validity` | VALID / INVALID / UNKNOWN | 정확한 binary·fixture·case·필수 경로인가 |
| `product_verdict` | PASS / FAIL / UNKNOWN / NOT_EVALUATED | 유효한 관측에서 제품 계약을 만족했는가 |
| `execution_state` | COMPLETED / ERROR / TIMEOUT / EFFECT_UNKNOWN | 실행과 관측이 완결됐는가 |
| `cleanup_state` | PASS / FAIL / UNKNOWN | 소유 프로세스·파일·자원 계약을 지켰는가 |
| `workflow_state` | ACTIVE / READY_FOR_REVIEW / BLOCKED_SAFETY / AGENT_LOST_UNKNOWN | agent 작업을 승격할 수 있는가 |
| `case_verdict` | PASS / FAIL / INVALID / ERROR / TIMEOUT / SKIP / UNSUPPORTED | CI용 집계. 원래 축과 실패 사실은 함께 보존 |

product FAIL 뒤 cleanup ERROR가 발생해도 제품 결함을 지우지 않는다. 유효성이 불명확하면 PASS를 만들지 않는다. FLAKY는 여러 실행을 비교한 campaign 속성으로, 단일 실행 결과나 이전 실패를 대체하지 않는다. failure→rerun pass는 두 관측을 모두 남긴다. 이미 완료된 시험과 그 뒤의 agent 중단도 각각 기록한다.

최종 승격은 `valid ∧ product_pass ∧ execution_complete ∧ cleanup_pass ∧ required_gates ∧ workflow_allowed`다. 하나의 종합 점수가 내구성 위반이나 미실행을 상쇄할 수 없다.

<a id="section-6"></a>

## 6. 보안 중단을 유발할 불필요한 행동을 없애는 개발 경로

정상 개발은 합성 입력, 소유한 임시 디렉터리·프로세스, scoped recipe, 엔진 내부 typed fault, 읽기 전용 증거로 수행한다. 고객 운영 DB·비밀·host 보안 설정을 만질 이유가 없도록 제품에서 지원한다. 데이터는 경계에서 최소화하고 필요한 field만 typed view로 제공한다. artifact-only는 정책의 예외나 탐지 회피 수단이 아니다.

일반 오류는 오류 코드→계약→모듈 fixture로 들어간다. 소유 subprocess의 종료·재시작이 필요한 durability 시험은 명시된 runner 기능으로 수행한다. 공유 host 설정 변경이나 다른 process 탐색을 정상 디버깅의 필수 단계로 만들지 않는다. worker의 실제 filesystem/network/process 권한이 실행 요청의 범위와 일치해야 한다.

Astra와 Luna 역할은 작업 시작 전 corpus 성능과 비용으로 배정한다. 정상 tool 실패·일시적 인프라 장애·제품 테스트 실패는 각 유형의 복구 절차를 사용한다. **정책/안전 중단이나 원인 불명 중단을 Luna 자동 전환으로 계속 실행하지 않는다.** 필요한 경우 중단 이유를 확인하고 원래 작업을 승인된 정상 개발 절차로 재설계한다. 파일에만 쓰고 나중에 다른 모델에게 읽히는 방식도 안전 경계를 우회하는 해결책이 아니다.

중단 예방은 measurable 목표다. 과거 Astra/Luna 차이가 모델 정책, 문맥, 도구 동작 중 무엇 때문이었는지는 실제 통지·trace 없이 단정할 수 없다. 기존 보고서의 [실행 설계](../reference/architecture-overview.md#execution-design)와 [모델 역할](../reference/architecture-overview.md#agent-routing)을 유지한다.

<a id="section-7"></a>

## 7. AI 개발 방법 자체를 평가하는 pilot

XADBTest는 제품을 검사하고, **AI delivery pilot**은 에이전트가 주어진 제품 작업을 얼마나 정확히 끝내는지 검사한다. 둘은 같은 pass rate가 아니다. 처음부터 거대한 별도 평가 제품을 만들 필요는 없다. versioned task corpus와 결과 schema를 XADBTest artifact에 연결하는 것으로 시작한다.

| 과제군 | 입력 | 성공 증거 |
|---|---|---|
| 기능 추가 | 작은 명세, 기존 core·API | 독립 acceptance, rollback·recovery까지 필요한 gate |
| 재현 | 합성 실패 bundle, 제한된 오류 정보 | 원인 조건을 보존한 최소 fixture와 새 세션 replay |
| 원인 분석·수정 | 겉으로 같은 증상의 서로 다른 원인 | 범인 후보의 근거, discriminating test, held-out bug 검출 |
| 테스트 추가 | 명세와 known-bad mutant | 의미 있는 검출·false positive 통제, oracle 자체 오류 검토 |
| 성능 개선 | 고정 workload·성능 기준 | 같은 결과·내구성, 반복 paired benchmark, 메모리·p99·복구 회귀 |
| XADB minor 진화(안정 릴리스 버전 간) | 이전 안정 릴리스 XADB 형식·API fixture; 사내 배포는 새 구축 | 새 버전의 old reader/변환, 중간 장애 복구, 역방향은 비요구 |

초기 pilot은 [DP05](development-policy.md)에 따라 기능 추가·재현·원인 수정·시험 추가의 4개 과제군에 각각 개발용 3개와 held-out 2개, 총 20개를 채택한다. 성능 과제 5개와 고객 버전 과제 5개는 각각 측정 기반과 고객 기준선이 준비될 때 추가한다. 이는 탐색적 평가의 계획 수이며 실제 확보한 corpus나 통계적 충분성을 뜻하지 않는다. 실행 예산·품질 조건·잠정 채택과 실제 작업 10개 확인 기준도 DP05를 따른다. 현재 pilot은 미실행이다.

동일 task·revision·시간/자원 budget으로 baseline workflow와 제안 workflow를 비교하고 순서를 무작위화한다. 모델 snapshot, reasoning 설정, 도구 버전, context package, 재시도 횟수를 보존한다. task 간 난이도를 혼합한 총점만 발표하지 않는다. 작성자가 이미 본 반례는 held-out이라고 부르지 않는다.

```text
verified_completion_rate = 검증을 끝낸 작업 수 / 시도한 모든 작업 수
cost_per_verified_task = 실패·중단·재시도를 포함한 총비용 / 검증 완료 작업 수
reproduction_rate = 새 환경에서 같은 위반을 재현한 bundle 수 / 재현 대상 bundle 수
false_exclusion_rate = 실제 원인을 후보에서 잘못 제거한 case 수 / 원인 정답을 가진 case 수
```

완료 수가 0이면 비용 비율은 정의되지 않으므로 수치와 함께 그대로 보고한다. unresolved, timeout, safety stop, harness error를 제외해 성공률을 높이지 않는다. 원인이 알려진 synthetic corpus의 진단 정확도를 운영 장애 전체의 정확도로 일반화하지 않는다.

독립이고 동일한 조건의 n회에서 중단 0건을 관측했다면, 중단 확률의 단측 95% 상한은 `1 − 0.05^(1/n)`이다. n=30이면 약 9.5%, n=100이면 약 3.0%다. 실제 작업은 상관되고 분포도 바뀌므로 이 계산은 가정 아래의 참고치다. **0건 관측 ≠ 중단 불가능**이며, 필수 검증을 삭제해 0건을 달성하면 실패로 판정한다.

<a id="section-8"></a>

## 8. 병렬화·성능·일정의 현실적 운영

성능 과제는 [AI 성능 개발 계약](../contracts/ai-performance-engineering.md)의 작업 묶음과 독립 판정을 사용한다. AI는 관측·병목 후보·원인 가설을 구분하고 구분 실험→국소 수정→paired 비교→정확성/회귀→새 세션 재현을 수행한다. 계측 항목에서 실제 소유 코드·계약·실행 recipe로 이동할 수 있어야 한다. 명령·단위·환경을 사람이 매번 설명해야 하는 구조를 사용성 결함으로 취급한다. PF08은 DP05의 성능 과제 5개에 연결하며 아직 미실행이다.

독립 모듈과 read-only 검토는 병렬화할 수 있지만, WAL·catalog·transaction의 공통 계약 변경을 여러 agent가 각자 진행하지 않는다. 계약 owner와 병합 순서를 정하고 각 worktree의 source hash를 증거에 남긴다. agent 수를 늘린 비율만큼 개발 속도가 빨라진다고 추정하지 않는다.

파일 구성·수정 범위·공유 파일 조정의 채택 기준은 [병렬 개발을 위한 파일 구성·소유 정책](parallel-development-policy.md)이다. [작업 계약](../examples/task-contract.example.json)의 `change_coordination`에 담당, 기준 소스, 수정 경로, 공유 계약, 동시 작업의 경로·의미 중복, 병합 순서와 통합 검사를 기록한다. 생성물은 합쳐진 최신 원문에서 재생성한다. 해당 정책의 PD01–PD03은 미실행 개발 절차 인수 계획이며 실제 병렬 개발의 효과를 이미 검증했다는 뜻은 아니다.

`D3-MODULE-ADOPTION-001`의 **D2 ACCEPTED 보완**은 [모듈 계약 인계](parallel-development-policy.md#module-contract-handoff)를 실제 작업 입력에 연결한다. C0의 효과 없는 착수 경계, 실제 C 수신, A/B 전용 제품 본문과 독립 기대값, 첫 C+A/C+B 검증, 합류의 새로운 상호작용을 각각 배정한다. 공통 owner의 단일 작성 원칙을 모든 제품 본문의 순차 개발로 확대하지 않는다. 계약 변경은 원 실패·대기 기록을 보존하며 영향 diff만 다시 검증하고, 제품 구현 중첩과 독립 리뷰·시험 동시 실행은 구별한다. [최종 독립 검토](../evidence/module-independence-survey-20261026.md#final-design-review)를 수신해 이 보완을 채택했으며 실제 병렬 인수는 미완료다.

작업 DAG의 각 소요 시간을 dᵢ라 할 때 무한 병렬 자원에서도 기간은 `T ≥ max_path Σ dᵢ`다. 첫 사내 커널은 B2/D0에서 검증하고, S1 저장 기준선→Q1 embedded SQL→B3 server로 확장한다. COMM은 공통 기반 뒤 독립 경로이며 PART/NG의 H2/H3 core는 SQL을 요구하지 않는다. GLOBAL_INDEX_QUALIFICATION에서 실제 서버와 합친다. 후속 기능의 구현 시간을 초기 사내 버전의 선행 경로에 자동으로 더하지 않지만 각 기능의 필수 통합 gate와 재작업은 해당 경로에 남는다. 현재 dᵢ 실측이 없으므로 달력 출시일을 계산하지 않는다. 초기 slice에서 소요 시간·실패 위치를 수집해 계획을 갱신한다.

성능 개선은 correctness를 고정한 뒤 baseline/candidate를 같은 머신 조건에서 번갈아 측정한다. warmup·입력 seed·concurrency·dataset·build 옵션을 기록하고 평균뿐 아니라 tail latency, memory, write amplification, recovery 시간을 본다. 가속 효과의 분산과 신뢰구간을 함께 보고한다. 작은 benchmark 하나를 위해 제품 계약을 약화시키지 않는다.

실행 권한 안의 일상 구현·테스트·수정은 AI가 계속 수행한다. DP01의 D1/D2 기술 결정은 별도 검토와 계약 담당 확인으로 진행하고, 사용자 확정 방향·품질 의무 변경·외부 출시·위임 밖 외부 효과인 D3는 기존 위임을 확인한 뒤 권한 있는 사람의 판단을 적용한다. 목표는 AI가 자율 실행하고 결과를 독립적으로 확인할 수 있는 체계다.

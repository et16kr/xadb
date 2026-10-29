# XADB 전체 방향성 점검 — README 취지 대비

2026-10-25 · **정적 검토 결과·개선 제안 / 설계·계획 변경 채택·제품 개발·새 시험 실행 없음**

위 상태는 최초 검토 당시 기록이다. 후속 판단은 [8절](#review-disposition), 재점검으로 좁힌 현재 수용 범위와 소유 문서 반영은 [9절](#review-adoption)을 따른다. 제품 개발·새 시험 실행 결과와 구별한다.

[제품 안내](../../README.md) · [개발 문서](../README.md) · [현재 구현](../implementation/README.md) · [개발 단계](../plans/development-stages.md) · [이전 DEV3 방향 점검](dev3-direction-review-20261022.md)

사용자의 "README의 취지대로 설계·계획·개발되고 있는지" 점검 요청에 대한 결과를 저장한다. 검토 기준 HEAD는 `360961bc965d27480cbba321824b7db54bc613da`다. 검토 당시 `devdoc/contracts`·`devdoc/plans`의 문서 5개 수정과 `scripts/dev2_io_*` 미추적 파일 5개가 있었으며, 이를 검토 대상 제품 변경으로 계산하지 않는다.

검토 방법은 README·AGENTS.md·개발 단계·구현 색인·구현 상태 JSON·DEV3 분류표와 `crates/` 구조·규모·일부 소스의 정적 대조다. 제품 빌드·시험은 실행하지 않았다. 이 문서는 독립 인수나 제품 실행 증거가 아니며, 제안의 채택은 해당 소유 계약·계획이 결정한다.

<a id="assessment"></a>
## 1. 전체 판단

**정확성·복구·정직한 상태 공개라는 원칙은 매우 충실하게 지켜지고 있다. 그러나 README의 핵심인 "AI가 이해·개발하기 좋은 단순한 구조"와 "필요한 기능을 만족하는 단순한 구조"에서는 벗어나는 중이다.** 코드·문서·절차가 정확성 증명에 최적화되면서 복잡도가 빠르게 늘고, 실제 DB 기능(디스크에 행을 쓰고 읽는 경로)의 도달은 늦어지고 있다.

| README 원칙 | 판정 | 근거 요약 |
| --- | --- | --- |
| 정확성·원자성·복구·자원 한도 우선 | 부합 | WAL→DWB→home flush, 제한 복구·새 PID 재진입, 고정 frame·quota·64KiB stack 증명 |
| 독립 정답·상태 모델·실제 I/O 시험 | 부합 | `crates/testing/reference` 독립 이력 모델, known-bad·mutant 거절, native PID 시험 |
| 실제 지원 상태 공개 | 부합(모범적) | 제한 PASS와 단계 미완료, FAIL·중단·NOT_RUN을 일관되게 구분 |
| 하위 계층부터 검증 가능한 단위 | 부분 부합 | 순서는 지켜지나 단위가 지나치게 잘고 수직 관통이 늦음 |
| 필요한 기능·성능을 만족하는 단순한 구조 | 괴리 | `storage/wal_io/` 124파일, 퇴화 사례 전용 모듈, 계획 버전 이름 |
| AI가 이해·수정하기 좋은 구조 | 괴리 | 문서 5.2MB + JSON 11.3MB, 내부 약어 밀도가 높은 색인 |
| 결함·재작업·개발 비용으로 효과 평가 | 미흡 | 효율 실험 계획은 있으나 비용·재작업 지표의 집계가 보이지 않음 |

결론적으로 전체 구조를 다시 설계할 근거는 없다. 다만 다음 마일스톤을 "첫 행의 수직 관통"으로 고정하고, 문서·절차·명명의 비용을 줄이는 방향 보정이 필요하다.

<a id="intent"></a>
## 2. README가 정한 취지

README는 XADB를 두 겹의 목표로 정의한다.

- **제품:** Linux·Rust로 구현하는 디스크 기반 관계형 DBMS. RAM보다 큰 데이터, 폭넓은 기능, 기본 성능.
- **실험:** 오픈소스 DBMS를 참고하여 AI가 이해하고 개발하기 좋은 DBMS를 AI agent로 만들 수 있는지 시험.

두 목표는 같은 방향을 요구한다. AI에게 좋은 구조란 책임이 작고 이름이 의미를 드러내며, 짧은 문맥으로 현재 상태와 다음 작업을 파악할 수 있는 구조다. 실험의 성패는 결함·재작업·개발 비용으로 평가한다고 README가 명시한다.

<a id="status"></a>
## 3. 현황: 규모와 단계별 진행

2026-10-18 이관 후 8일간 173개 commit이 쌓였다. 현재 Rust는 이관분을 포함해 약 30.5만 줄이지만, 디스크에 행 하나를 넣고 다시 읽는 경로는 아직 없다. [구현 상태 JSON](../implementation/implementation-status.json)도 `disk_pages_and_indexes = PAGE_ENVELOPE_ONLY_NO_DISK_PATH_OR_INDEX`, `sql = NOT_IMPLEMENTED`로 기록한다.

| 항목 | 규모 |
| --- | --- |
| `crates/engine` | 약 15.9만 줄, 436개 파일 (`storage/wal_io/`만 124개 파일) |
| `crates/testing` | 약 12.1만 줄 |
| `crates/sql` · `transport` · `foundation` · `platform` | 약 1.36만 · 0.66만 · 0.24만 · 0.19만 줄 |
| 별도 시험 파일(`*test*.rs`) | 약 8.5만 줄 |
| `devdoc/*.md` | 221개, 약 5.2MB (최대 파일 약 181KB) |
| `devdoc/*.json` | 약 11.3MB |
| `artifacts/` 실행 산출물 | 약 7.2GB |
| commit 유형 | docs 51 · feat 45 · fix 12 · test 5 · refactor 4 · 기타 4 · 형식 없는 제목 52 |

| 단계 | 상태(저장소 기록 기준) |
| --- | --- |
| DEV0 시험 기반 | 로컬 검증 완료 |
| DEV1 공통 타입·Linux I/O | 로컬 검증 완료 |
| COMM | frame·socket·취소 구현, C1 인수 미완료 |
| DEV2 커널 | K0 volatile 기반과 K1–K5 `durable/` 구현, PF04 계측 비용 한도 FAIL로 인수 미완료 |
| DEV3 저장 | 16개 분류 중 형식·읽기·할당·WAL·flush·복구의 제한 단위만 연결. 트리 변경·인덱스·DDL·live undo·commit·checkpoint 미완료 |
| DEV4 SQL 앞단 | lexer·splitter·parser·typed predicate 구현, engine 연결 없음 |
| DEV5–DEV9 | 미구현. 상세 설계와 하위 작업 분해(DEV4 55·DEV5 64·DEV6 80·DEV7 140개)는 작성됨 |

검토 시점의 미커밋 작업은 DEV2 PF04 원인 규명을 위한 bpftrace 기반 ext4 journal I/O 추적 도구(`scripts/dev2_io_trace.bt` 등)다.

<a id="aligned"></a>
## 4. 취지에 맞게 가고 있는 점

- **복구·원자성 우선:** DEV3는 행보다 먼저 WAL→doublewrite→home flush, WAL prefix 분석, genesis 복구, 새 PID 재진입을 실제 I/O로 검증했다. DPT·recLSN·CLR·fuzzy checkpoint 등 ARIES 계열 구조도 정통적이다.
- **자원 한도:** 고정 크기 frame, 64KiB stack 경로 증명, quota·budget 검산이 코드 수준에서 지켜진다. 워크스페이스의 `unsafe_code = "forbid"`와 의존성 정확 버전 고정도 타당하다.
- **독립 판정:** 독립 이력 모델, known-bad 거절, 제품 변조(mutant) 거절 시험이 README의 "독립 정답·상태 모델" 원칙에 실체를 준다.
- **정직한 상태 표기:** 모든 기록이 제한 단위 PASS와 단계 미완료를 구분하고 FAIL·중단을 PASS로 승격하지 않는다.
- **하위 계층 우선:** DEV0→DEV1→DEV2→DEV3 순서와 저장 비의존 DEV4 앞단만 병행하는 예외가 실제로 지켜진다.
- **공개 DBMS의 비판적 참고:** PostgreSQL·MariaDB·CUBRID 등 10개 로컬 checkout과 revision 기록 방식이 취지와 맞다.
- **push gate:** fmt·Clippy(`-D warnings`)·문서 검사를 push 직전에 묶어 편집 중 비용을 줄였다.
- **코드 품질:** 타입 기반 소유권, raw constructor 금지, 오류 확실성(`Certainty`)·효과(`Effect`) 표기 등 개별 코드의 엄밀성은 높다.

<a id="gaps"></a>
## 5. 취지와 어긋나는 점

가장 큰 괴리는 "AI가 이해·개발하기 좋은 단순한 구조"다. 문제는 품질이 아니라 도달 속도와 구조 복잡도다.

| 취지 | 관찰된 현상 | 영향 |
| --- | --- | --- |
| 단순한 구조 | `storage/wal_io/` 한 디렉터리에 124개 파일. `empty_transactions_{live,load,history,pressure}`처럼 변경 없는 거래라는 퇴화 사례에 전용 모듈 계층이 있음. `mixed_system_*`·`plan4_*`도 같은 디렉터리에 평면 배치 | 일반 행 변경 경로가 오면 특수 경로를 다시 일반화·통합할 가능성이 큼 |
| 이해하기 쉬운 이름 | `plan4_*`, `genesis`, `high-water5`처럼 계획 버전이 제품 모듈 이름에 들어감. page 오프셋이 `1536..1576` 같은 상수로 각 모듈에 흩어짐 | 새 agent는 문서 이력을 읽어야만 코드 의미를 알 수 있음 |
| 필요한 만큼의 문서 | 설계·증거 문서 5.2MB + JSON 11.3MB, 단일 문서 최대 약 181KB. `devdoc/README.md`의 앞 200줄이 날짜별 변경 기록이고 작업 시작 안내는 그 뒤에 있음. 색인 문장이 "필수75 source별 PASS·최종 독립 D1 제한 수용"처럼 내부 약어로 채워짐 | 문서가 문맥 창을 먼저 소비해 오히려 AI 친화성을 낮춤 |
| 검증 가능한 작은 단위 | 단위가 "빈 거래 Begin 준비", "Status 상주 후보 선택"처럼 지나치게 잘게 쪼개짐 | 수직 관통(CREATE→INSERT→COMMIT→장애→재시작→SELECT)이 늦어져 구조 결함 발견이 뒤로 밀림 |
| 폭넓은 기능을 단계적으로 | DEV5–DEV9 상세 설계와 수백 개 하위 작업 분해가 DEV3 S1 전에 작성됨 | DEV3 결정이 바뀌면 재작업. 미검증 상위 계획의 유지 비용 발생 |
| 비용·재작업으로 효과 평가 | 단위별 독립 리뷰(D1/D2)·source별 증거는 많지만, 개발 시간·재작업률·문서 대비 코드 비용을 집계한 지표가 보이지 않음. [AI 효율 개선 계획](../plans/dev3-ai-throughput-improvement.md)은 소비자 1/3 도달·효과 비교 미완료 | 실험(AI 개발 방법론)의 결론을 내리기 어려움 |
| 제품 가치에 비례한 투자 | [DEV3 계획](../plans/dev3-implementation-plan.md)은 DEV2 영속 커널을 DEV3와 복구 알고리즘이 다른 기준선이며 "두 번째 제품 엔진으로 확대하지 않는다"고 정함. 그런데도 그 계측 p99 비용 한도(PF04)를 위해 PMU·ext4 journal·커널 기계어 수준 추적을 여러 차례 진행 중 | 제품에 남지 않을 경로에 큰 비용을 씀 |
| 확정 계약 안의 자율 실행 | `AGENTS.md` 상단이 날짜별 사용자 지시(시험 금지·예외·마감 연장)로 누적되고 "사용자 요청으로 중단" 기록이 반복됨 | agent가 읽어야 할 예외 규칙이 계속 늘고 규칙 간 우선순위 판단 비용이 커짐 |

<a id="recommendations"></a>
## 6. 개선 권고

우선순위 순서다. 각 항목의 채택은 해당 소유 계획·계약의 결정 절차를 따른다.

1. **다음 마일스톤을 "첫 행의 수직 관통"으로 고정한다.** 단일 table CREATE→한 행 INSERT→COMMIT→프로세스 kill→재시작 복구→SELECT를 가장 좁은 형태로 먼저 연결한다. 그 경로가 요구하지 않는 일반화(퇴화 사례 pressure, 추가 Status 수명 변형 등)는 보류한다. 첫 관통 뒤에 page 수명·tree split·인덱스를 넓힌다.
2. **DEV2 PF04를 결정으로 닫는다.** 제품 엔진이 아닌 기준선의 계측 비용 한도는 waiver 또는 재정의로 처리하고, 커널 수준 추적을 중단한다. 계측 비용 인수는 DEV3 PF 항목에서 실제 제품 경로로 다시 수행한다.
3. **명명과 모듈 구조를 정리한다.** `plan4_*`·`genesis`·`high-water5` 같은 계획 버전명을 도메인 이름으로 바꾸고, `wal_io/`를 `allocation/`·`status/`·`recovery/`·`retention/` 등 하위 모듈로 나눈다. page 레이아웃 오프셋은 page 종류별 한 곳의 레이아웃 정의로 모은다.
4. **문서를 줄이고 진입점을 짧게 만든다.** `devdoc/README.md`의 날짜별 기록을 `history/`나 증거 색인으로 옮기고, 평문으로 쓴 "현재 상태 한 페이지"를 둔다. 문서 크기 상한(예: 40KB)과 약어 용어집을 정한다.
5. **DEV5–DEV9 상세 계획의 추가 세분화를 동결한다.** DEV3 S1 이후 실제 공급 API를 보고 재검토한다.
6. **AI 실험 지표를 정기 집계한다.** 단위별 소요 시간, 재작업·원복 횟수, 리뷰가 찾은 결함 수, 문서/코드 변경 비율을 주 단위로 기록해 README의 효과 평가 원칙을 실체화한다.
7. **`AGENTS.md`를 짧게 유지한다.** 날짜별 지시는 해당 정책 문서로 흡수하고, AGENTS.md에는 현재 유효한 규칙만 남긴다.
8. **산출물 보존 정책을 정한다.** `artifacts/` 7.2GB의 보존 기간·요약 보존·삭제 기준을 문서화한다.

<a id="limits"></a>
## 7. 검토 한계

- 제품 빌드·시험을 실행하지 않았으며 기존 PASS/FAIL 기록을 재확인하지 않았다.
- 코드 판단은 디렉터리 구조·규모와 일부 파일 표본에 근거하며 전체 코드의 정확성 검토가 아니다.
- 규모 수치는 검토 시점 작업 트리의 줄 수·바이트 수이며 이관분을 포함한다.

<a id="review-disposition"></a>
## 8. 후속 검토 — 수용·거부 판단과 이유

2026-10-25 · 사용자의 “받아들일 것은 받아들이고, 거부할 것은 해당 문서 하단에 이유를 추가” 요청에 따른 후속 판단이다. 담당은 `/root`, 대조 기준 HEAD는 `214f5a1e5835ccbec808d34a96fca413dd2544ea`다. 위 1–7절은 최초 검토 당시의 기록으로 보존하고, 이 문서의 현재 권고 범위는 아래 판단으로 보완한다. 수용은 명시한 개선 과제를 받아들이는 뜻이며, 실제 파일 재배치·정책 변경·제품 개발을 이번에 실행했다는 뜻은 아니다. 소유 계약과 기존 인수 의무는 유지한다.

### 8.1 전체 판단 보정

**현재 설계 방향은 README와 대체로 일치한다. 다만 구현의 세분화와 문서 누적으로 탐색·통합 비용이 커질 위험이 있으므로, 이미 채택한 첫 행 통합의 실제 선행 작업에 집중하고 구체적인 중복과 탐색 부담을 줄이는 개선은 수용한다.** 전체 구조 재설계가 필요하다는 근거는 없으며, 파일 수·문서량·첫 행 미완료만으로 “README 취지에서 벗어나고 있다”거나 “검증 절차가 개발 지연의 원인”이라고 확정하는 판단은 받아들이지 않는다.

정확성·복구·자원 한도를 우선하고 PASS/FAIL·미완료를 구별하는 방향은 유지한다. 다만 이번 정적 대조로 전체 코드의 정확성이나 모든 증거의 적정성을 재인증한 것은 아니다. 실제 비용은 [DP07](../policies/development-policy.md#section-8)과 [AI 효율 계획](../plans/dev3-ai-throughput-improvement.md)의 작업별 탐색·구현·검토·재작업 시간으로 판단한다.

### 8.2 권고 8개에 대한 처리

| 원 권고 | 판단 | 받아들이는 범위 또는 거부 이유 |
| --- | --- | --- |
| 1. 첫 행 수직 관통 우선 | **부분 수용** | 첫 행의 실제 영속 생성·변경·commit·장애 복구 연결에 집중한다. 이는 새 방향이 아니라 2026-10-22 채택된 [D3-DIRECTION-001](../plans/dev3-implementation-plan.md#direction-review-adoption)의 실행을 강화하는 것이다. 첫 행에 필요한 page 수명·eviction/reload·live undo·실패 복원을 뒤로 미루는 해석은 거부한다. [첫 행 완료 조건](../contracts/dev3-task-11-commit.md#first-row-integration)의 ACK/loser/statement 실패·부분 CLR 재장애·quota 경계를 유지한다. DEV3 engine 경로의 연결과 DEV4 SQL 연결을 구별하고, SQL 전체를 새 선행으로 추가하지 않는다. |
| 2. PF04를 waiver·재정의로 종료하고 추적 중단 | **거부** | [PF04 분석](dev2-pf04-analysis-20261025.md)은 영속 p99뿐 아니라 K0의 CPU·경과 시간과 실행 간 변동도 확인한다. DEV2를 두 번째 제품 엔진으로 확대하지 않는다는 경계가 공통 계측·진단·실패 의미의 가치를 없애지는 않는다. 비용 한도를 면제해 닫거나 DEV3 인수로 자동 대체할 근거가 부족하다. 원 FAIL·DEV2 미완료를 유지한다. 추가 조사의 투자 대비 가치·예산·중단 조건은 [해결 계획](../plans/dev2-pf04-resolution-plan.md)에서 판단할 수 있지만, 이번 문서로 진행 중인 작업을 중단하거나 기준을 낮추지 않는다. |
| 3. 명명·모듈·레이아웃 정리 | **부분 수용** | `plan4_*`의 역할 설명, 깊은 소유권 접근, 같은 내부 장부 레이아웃의 중복을 구체적인 개선 대상으로 수용한다. 실제 변경 이유·수명·의존 경계를 확인하여 관련 작업 안에서 정리한다. 파일 수를 줄이기 위한 일괄 재배치와 `genesis`·`high-water5`를 계획 버전명으로 보고 일괄 변경하는 제안은 거부한다. 아래 사실 보정을 적용하고 [코딩 기준](../policies/coding-convention.md#section-4)에 따라 공유 의미와 조립 절차를 구분한다. |
| 4. 문서 진입점 단축·용어 설명·크기 제한 | **부분 수용** | 작업 시작 안내와 현재 상태를 먼저 찾도록 진입점을 줄이고, 날짜별 상세 기록은 기존 증거 색인으로 연결하며 내부 약어를 설명하는 개선을 수용한다. 근거 없는 일률적 40KB 상한은 거부한다. 분량을 맞추려고 계약을 분산하거나 원본 증거를 삭제하지 않으며, 필요한 절만 읽는 [문서 운영 기준](../policies/documentation-policy.md)을 적용한다. |
| 5. DEV5–DEV9 상세 계획 추가 세분화 동결 | **일괄 동결은 거부** | 현재 소비자나 미해결 질문이 없는 추가 세분화는 미루되, 상위 요구에서 하위 API·실패·자원 계약의 누락을 발견하는 검토까지 막지 않는다. DEV3 S1 뒤에만 재검토한다는 일률적인 시점도 채택하지 않는다. 작업 필요성은 실제 공급 의존성과 변경 영향으로 판단하며, 이 결정으로 상위 제품 구현을 시작하지 않는다. |
| 6. AI 실험 지표 집계 | **수용** | 이미 시작한 시간 기록을 이용해 작업별 소요 시간, 수정·원복, 리뷰 발견 결함, 다음 소비자 도달 여부를 집계한다. 현재의 [세 통합 단위 실험](../plans/dev3-ai-throughput-improvement.md)을 우선 완성한다. 주간 요약과 문서/코드 변경 비율은 보조 자료로 사용할 수 있지만, 난도·대기·품질 조건 없이 생산성을 판정하거나 별도 대형 자동화 과제를 만들지 않는다. |
| 7. AGENTS.md 단축 | **부분 수용** | 중복 안내와 종료된 한시 지시를 정리하고 유효한 규칙의 소유 문서로 연결하는 개선을 수용한다. 정리 과정에서도 변경 코드 시험 제한, COMM 한정 예외, DEV3 최종 S5 예외와 사용자 지시의 우선순위·적용 범위를 보존한다. 사용자의 중단 요청을 따른 사실 자체를 자율 실행 실패의 근거로 삼는 해석은 거부한다. |
| 8. 산출물 보존 정책 | **보완 과제로 수용** | 보존 기준이 전혀 없는 것은 아니다. [DP06](../policies/development-policy.md#section-7)의 배포·결함 증거 보존과 [문서 증거 보존 규칙](../policies/documentation-policy.md)을 전제로, 로컬 중간 build/cache와 재현에 필요한 source/binary/fixture·원 FAIL을 구별하는 적용 기준을 보완한다. 7.2GB라는 크기만으로 삭제하지 않으며, 요약으로 원본 증거를 대체하지 않는다. 이번에 산출물을 삭제하지 않는다. |

### 8.3 근거의 사실·해석 보정

- **규모의 분모:** 후속 정적 집계에서 `crates/engine`의 Rust 159,391줄 중 파일명에 `test`가 들어간 파일은 82,604줄이다. 이는 이름 기준 분류이며 제품/시험 코드의 완전한 의미 분석은 아니다. `devdoc` JSON 약 11.3MB 중 약 10MB는 `evidence/`에 있다. 제품·시험·보존 증거를 구분하지 않은 총량을 제품 복잡도나 매 작업의 필수 독서량으로 해석하지 않는다. 원 검토 시점 수치는 당시 기록으로 유지한다.
- **이름과 레이아웃:** [초기 배치 계약](../contracts/dev3-plan4-genesis.md)은 plan version 4와 ObjectId high-water 5를 구별한다. `genesis`는 초기 생성 상태를 뜻한다. [빈 거래 장부 정의](../../crates/engine/src/storage/wal_io/empty_transactions.rs)는 디스크 형식이 아닌 메모리 bookkeeping임을 명시한다. 따라서 `1536..1576`은 이 예시에서 page 오프셋이 아니라 장부 내 위치다. [live 구현](https://github.com/et16kr/xadb/blob/f15784600b7c2e61f5daa14901e37a84129bf248/crates/engine/src/storage/wal_io/empty_transactions_live.rs)·[복구 구현](../../crates/engine/src/storage/wal_io/recovery_prefix/transactions.rs)·[retention 장부](https://github.com/et16kr/xadb/blob/214f5a1e5835ccbec808d34a96fca413dd2544ea/crates/engine/src/storage/wal_io/plan4_retention_control.rs)의 중복 여부를 해당 수명·의미 기준으로 검토할 수는 있다. 원 근거의 `wal_io/empty_transactions_live.rs`·`wal_io/plan4_retention_control.rs` 경로는 당시 HEAD의 identity로 보존하며, 위 두 탐색 링크는 commit 준비 시 기준 revision의 고정 주소로 연결했다. 9절 반영 중 관측한 별도 경로 이동의 제품 검증을 이 기록으로 주장하지 않는다.
- **작업 크기와 측정 상태:** 빈 거래도 WAL·Status·dirty/flush·복구 상태를 변경한다. 행 변경이 없다는 이유만으로 불필요한 경로로 분류하지 않는다. [빈 거래 통합 기록](dev3-empty-transaction-20261023.md)에 작업 입력과 `timing.jsonl` 관측이 이미 있으므로 효율 측정은 “없음”이 아니라 “시작했으나 집계·효과 비교 미완료”다. 작은 작업 단위가 지연을 일으켰다는 인과 판단에는 추가 실측이 필요하다.
- **상태 요약의 한계:** `implementation-status.json`의 포괄적인 `PAGE_ENVELOPE_ONLY_NO_DISK_PATH_OR_INDEX`·`NOT_IMPLEMENTED`만으로 최신 제한 구현을 설명하지 않는다. [DEV3 구현](../implementation/dev3-implementation.md)과 [DEV4 구현](../implementation/dev4-implementation.md)의 실제 공급 범위를 함께 읽는다. 제한된 디스크 I/O·복구와 SQL 앞단이 있어도 전체 행 저장·SQL 실행 경로가 완료됐다는 뜻은 아니다.

이번 반영은 원문 하단의 후속 판단과 증거 색인의 설명 수정에 한정한다. 제품 코드·계약·인수 기준 변경, 빌드·시험·성능 추적·문서 자동 검사·commit/push는 수행하지 않았다. 기존 결과의 source·범위·FAIL·중단·NOT_RUN과 사용자의 시험 실행 제한을 보존한다.

<a id="review-adoption"></a>
## 9. 재점검 확정 범위와 계획·설계 반영

2026-10-25 · 사용자의 재점검과 “리뷰 문서에 명시하고, 계획·설계 등의 문서에 반영” 지시에 따라 아래 범위로 좁힌다. 8절의 구조·명명·장부 정리를 모두 즉시 구현 대상으로 읽는 해석을 대체한다. **첫 행 통합은 우선 실행하고, 구조·명명·공통화는 실제 소비자 연결에서 필요성이 확인된 국소 후보로 제한한다.** 판단을 기록하는 데서 끝내지 않고 아래 소유 문서의 실행·설계 기준에 반영한다. 제품 코드 변경이나 첫 행 기능 완료는 이번 요청의 산출물이 아니다.

| 수용 대상 | 재점검 결론과 적용 한도 | 반영한 소유 문서 |
| --- | --- | --- |
| 1. 첫 행 거래 통합 | 최우선 유지. 실제 engine의 영속 table·행 변경·commit·새 process 복구·읽기와 실패/재장애를 연결한다. 필요한 page/undo/guard 공급을 생략하지 않고 SQL 전체를 새 선행으로 추가하지 않음 | [실행 범위](../plans/dev3-implementation-plan.md#direction-refinement), [첫 행 완료 조건](../contracts/dev3-task-11-commit.md#first-row-integration) |
| 2. 저장 경로 구조 단순화 | 조건부 후보로 축소. 실제 소비자 연결에서 불필요한 내부 수정·동일 책임 중복이 확인될 때 기존 구조 유지와 좁은 추출을 비교. 전체 재배치·wrapper 제거를 별도 선행 작업으로 만들지 않음 | [저장 세션 개선 경계](../contracts/dev3-storage-session.md#consumer-driven-refinement) |
| 3. 명명·장부 레이아웃 | 새 이름은 역할 중심으로 작성. 기존 이름은 혼동·의존 문제의 근거가 있을 때만 변경 후보. 같은 의미의 필드 표현만 공통화할 수 있으며 live/recovery의 상태 규약·지원 version·독립 oracle는 보존 | [저장 세션 개선 경계](../contracts/dev3-storage-session.md#consumer-driven-refinement) |
| 4. 문서 진입점 | 기존 작업 안내를 먼저 배치하고 이력을 뒤에 보존. 새 상세 기록은 증거 색인으로 연결. 별도 상태 요약 문서·전면 재배치·40KB 상한을 추가하지 않음 | [진입점 유지 기준](../policies/documentation-policy.md#entry-maintenance), [실제 읽기 진입점](../README.md) |
| 5. 개발 효율 측정 | 이미 시작한 입력·시간·실패/수정 기록을 활용. 별도 계측 플랫폼·주간 자동 보고·공통 runner 신설과 효과 비교 완료를 첫 행 착수의 선행으로 삼지 않음 | [기존 기록의 적용 범위](../plans/dev3-ai-throughput-improvement.md#bounded-application) |
| 6. AGENTS.md 정리 | 종료된 한시 안내·중복 설명만 정리 후보. 유효한 핵심 시험 제한·예외·지시 우선순위는 루트에서 바로 확인하도록 유지. 이번에는 실제 지시 문구를 이동·삭제하지 않음 | [작업 지침 유지 기준](../policies/documentation-policy.md#entry-maintenance) |
| 7. 산출물 관리 | 기존 방식으로 재생성 가능한 cache와 보호 증거를 구별. 실제 사용·hardlink·증거 참조 확인 후 정확한 대상만 정리. 자동 만료·크기 기준 일괄 삭제·새 보관 서비스를 추가하지 않음 | [로컬 산출물 보존](../policies/repository-build-plan.md#local-artifact-retention)·[관리 JSON](../policies/repository-build-plan.json) |

범위를 줄인 직접 근거는 두 가지다. [기존 runtime/core 설계](../contracts/dev3-storage-session.md#private-runtime-core)의 wrapper 일부는 Fresh/recovered 권한과 원 자원 수명을 보존한다. 또한 기준 HEAD `f15784600b7c2e61f5daa14901e37a84129bf248`의 원 `wal_io/empty_transactions_live.rs`([기준 revision 소스](https://github.com/et16kr/xadb/blob/f15784600b7c2e61f5daa14901e37a84129bf248/crates/engine/src/storage/wal_io/empty_transactions_live.rs))에서 offset1624는 phase1–5, [복구 장부](../../crates/engine/src/storage/wal_io/recovery_prefix/transactions.rs)의 같은 위치는 present0/1이므로 전체 장부를 동일한 상태 기계로 공통화할 근거가 없다. 산출물 관리는 [실제 ENOSPC·선택 캐시 회수 기록](dev3-runtime-core-20261024.md)이 필요성을 뒷받침하지만 기존 보존 절차를 재사용할 범위다. 검토 중 별도 작업의 제품 경로 이동·배치 목록 수정을 관측했으며, 이를 이번 문서 변경이나 제품 검증 범위에 포함하지 않는다.

2026-10-29 탐색 링크 보완: 8.3절의 [복구 구현]과 위 [복구 장부] 링크만 현재 위치로 연결했다. 원 source 경로는 `crates/engine/src/storage/wal_io/recovery_empty_transactions.rs`이고, 기준 commit은 `47e39949d2fa199b30d9a41b47182e551b656d6a`이다. 변경 전 이 문서의 SHA-256은 `cc80dce298ee473e453dba51d2d9e6c8df8727bf94e5706a32b60bd7fc5caab5`이며, 원 byte는 로컬 `artifacts/module-l5d-recovery-20261029/history/xadb-direction-review-20261025.md`에 보존했다. 원 판단·검토 기록은 갱신하지 않는다. 이동 대응은 [L5-D 기록](module-independence-survey-20261026.md#l5d-recovery-move)을 따른다.

원 권고의 PF04 면제·DEV3 인수로 자동 대체, 필수 복구·자원 검증 생략, DEV5–DEV9 계획 일괄 동결은 계속 거부한다. 4–7번을 기능 연결의 선행 장벽으로 만들지 않으며 실제 공간 부족·입력 오류는 해당 작업의 차단 원인으로 처리한다. 코드가 변경되면 해당 diff의 필요한 시험·독립 검토를 수행하고, 완료된 무변경 시험을 반복하지 않는 사용자 지시와 DEV3 최종 S5 예외를 유지한다.

### 반영·검토 기록

- 결정 정본은 [D3-DIRECTION-002](../plans/dev3-implementation-plan.md#direction-refinement)다. 작업·계약·통합 담당은 `/root`, 기준 HEAD는 `f15784600b7c2e61f5daa14901e37a84129bf248`이며 편집 전13개 파일·hash·dirty 목록을 `/tmp/xadb-direction-adoption-3epaicy7/`에 보존했다.
- 소유 문서·폴더 색인과 빌드 정책 JSON을 함께 반영했다. `devdoc/README.md`는 기존 작업 안내·문서 역할·편집 기준을 앞으로 옮기고 기존 날짜별 기록을 뒤에 보존했다. 기존 문서 ID·경로·제목은 유지하므로 `documents.json`의 기존 등록을 그대로 사용한다.
- 독립 검토자 `/root/direction_adoption_review`가 고정한13개 파일의 반영안과 후속 링크 보완을 대조해 **최종 D2 PASS**로 판정했고 통합 담당이 확정했다. 최초 검토의 이동 전 소스 링크 지적은 원 경로·revision을 보존하고 현재 링크를 연결해 해소했다. 의미상 차단 지적은 없으며 필수 첫 행·F05·S5 의무, 권한 경계, live/recovery 상태 차이, PF04 실패와 기존 README 이력 보존을 확인했다. 반영안·patch와 링크 보완 hash는 위 임시 보존 경로에 있으며, 동시 제품 변경은 검토 범위에 포함하지 않았다.
- 위 문서 반영·독립 검토 시점에는 제품 코드·시험 inventory·지원 상태를 변경하지 않았고 빌드·제품 시험·벤치마크·문서 자동 검사·commit/push·산출물 삭제를 수행하지 않았다. 병행 DEV2 도구 수정과 원 source별 PASS/FAIL·중단·NOT_RUN은 보존한다. 후속 commit/push 준비에서는 별도 미커밋 제품 이동을 제외하고 소스 링크를 기준 revision의 고정 주소로 보완했다. push 검사는 전송할 commit의 임시 사본에서 별도로 수행한다.

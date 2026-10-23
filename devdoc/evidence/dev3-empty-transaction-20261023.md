# DEV3 빈 거래 통합 — 작업·선택 증거

2026-10-23 · **통합 진행 / 소비자 미도달 / DEV3·S5 미완료**. 기준 제품 `4037cb6`,
[운영 계획](../plans/dev3-ai-throughput-improvement.md) `f2adfe7`. 사용자 요청에 따라 단계별
commit과 OCR delegate 독립 검토를 사용한다. 첫 단계는 [빈 AbortEnd 준비 계약](../contracts/dev3-obligation-format.md#empty-abort-preparation)이다.

## 완료 경계와 현재 작업

실제 resident owner의 Begin→RollbackStart→AbortEnd, Status flush/load와 새 process의 동일-build
닫힌 복구·재진입을 이번 통합 종료점으로 고정했다. 현재 순수 bytes 준비의 선택 시험을 마쳤으며 이 결과로
live 거래 완료·예약 반환·일반 READY를 주장하지 않는다. 구현 담당은 `/root`, 독립 계약/fixture/
최종 검토는 `/root/pool_design_review`다. 공유 page/WAL/transaction 수정은 통합 담당만 한다.

독립 사전 검토는 전역 WAL tip과 allocator pageLSN 분리, typed live Status, 새 transaction
원장과 원 system descriptor 분리, F05의 wide reader 동시 보유 배제, 주소별 canonical before
재생성 및 historical Pending 검산을 통합의 필수 조건으로 정리했다. 실패로 소모된 local 번호의
gap은 정상 replay에서 허용하되 채택 번호의 strict 증가·중복 금지를 보존한다. 결과는
`artifacts/dev3/empty-transaction-20261023/review/pre-design.{md,json}`에 보존한다.

## 준비 단계의 검증

D2는 순수 빈 AbortEnd 경계로 수용했다. 사전 검토에서 before pageLSN≥RollbackStart.apply와
S5의 Free 세대/actor 세대 구분을 보완했다. 동결 source `3297a4e9d3a4eb763b3d6fd2606ceb22e92353607c1ec36d48cd4ea31fafc165`에서
첫 test binary build13.637초, 신규13·공통 검산의 직접 소비자2는 첫 실행 모두 PASS(합1.291초),
static1과 fmt·engine Clippy도 PASS다. 새 source 수정·동일 시험 반복은 없었다. component는
41,640/65,536바이트, caller frame/page loan35,808바이트는 별도다. 이 계산은 실제 통합 owner
전체 F05를 대신하지 않는다. 독립 fixture3종의30파일237,792바이트를 사용했다.

`r0`의 [source별 색인](dev3-empty-transaction-20261023.json)에 command·source/binary/hash·정확
case·시간과 원 로그를 연결했다. 최종 독립 D1은 순수 준비 범위로 수용했다. staged47경로를 검토하고 OCR50경로 중 무관3을
명시적으로 제외했으며 차단 결함은 없다. 원 판정은 `review/empty-abort/final-d1.json`이다. S5의639조합은 authored NOT_RUN이다.
실행하지 않은 native·live owner/compiler·통합 F05를 이 단계의 PASS에 포함하지 않는다.

## AI 작업 방식 관측

사용자의 계획 적용 지시 후 22:14:16 KST부터 `artifacts/dev3/empty-transaction-20261023/`
`task-input.md`와 append-only `timing.jsonl`에 소비자·차단 선행·필수 case·검토자·미실행 의무를
묶었다. 그 이전 탐색 시간은 미계측이고 전체 lead time을 이 시점으로 축소하지 않는다.
제품 작성과 독립 raw literal 준비를 병행한다. selector·fixture length/hash를 build 전에 대조하고
동결 source에서 binary를 한 번 만들어 정확 selector만 실행하는 방식으로 준비 오류를 줄인다.
검증 분모/의미 oracle/완료 기준은 줄이지 않는다.

이번은 서로 다른 세 통합 단위 관측 중 첫 작업이다. 준비 helper는 소비자 도달이 아니며 완료
통합 단위는0/3이다. 기존 이력의 단계별 시간이 없어 대조군 수치로 사용하지 않고, 지금의 병행
작업이나 build 횟수를 속도 개선의 인과 증거로 세지 않는다. native/recovery 통합의 실제 종료
시각, 재작업·검토 대기·runner 오류와 S5 부채를 이어서 기록한다.


최종 D1 시점 case/fixture/receipt/review 집계는 752,992바이트(8MiB 한도), 별도 source export·ELF·
compiler cache는1,004,713,374바이트다. 후속 metadata addendum은 별도 원본으로 보존한다.

<a id="owner-handoff"></a>
## 원 Status owner의 소유권 인계

준비 commit `11ab0bf` 다음으로 [§18](../contracts/dev3-obligation-format.md#empty-transaction-handoff)의
원 owner consuming 인계와 고정 bank 보존을 구현했다. 원 allocator/system descriptor는
유지하고 successful owner에서 system split·INIT를 감춘다. issued/adopted0의 진입 근거는
fresh-only 단방향 타입 수명이며, 거래 발급을 아직 구현한 것은 아니다. 같은 통합의 다음 단계는
live Begin·RollbackStart·AbortEnd·typed load/flush·새 process 복구다. 소비자 도달0/3을 유지한다.

별도 D2에서 zero-pin 상황에도 INIT/DPT claim이 남을 수 있다는 지적을 반영했다. validated
pin·INIT·flush claim 검산을 추가하고 기존 capture의 token을 잃은 실제 경로도 검증했다.
전환만의 source-value 정적 검산은 h2에서64,856/65,536바이트, bank 포함 retained115,168,
metadata180,024/196,608, flush call 여유 포함245,560/262,144다. 미래32거래의 live F05는
NOT_SUPPLIED이며 이 값으로 live effects를 허용하지 않는다.

| source | 선택·관측 | 결과 |
| --- | --- | --- |
| h0 `2b656d96…` | 새 시험의 channel 타입2건·foreign core 수명1건 | compile FAIL, runtime 미실행 |
| h1 `006cbe8a…` | foreign core 수명 수정의 추가 compile 진단 | compile FAIL, runtime 미실행 |
| h2 `394c5f83…` | build·source-value static1 | PASS; runtime 미실행 |
| h3 `0eb44930…` | actual capture 시험 추가 후 build·새 runtime13 | 첫 실행 PASS,1.182초 |
| h3 | compiler baseline1+positive2+negative5, 새 checker2 | PASS; compiler46.538초 |
| h3 | fmt·engine Clippy | fmt PASS, 시험의 intentional `forget_non_drop` lint FAIL |
| h4 `5e7c3e2…` | 그 시험 표현에 이유를 명시한 좁은 lint annotation만 추가 | fmt/Clippy PASS; runtime/compiler 재실행 없음 |

source별 원 binary·command·진단·PASS/FAIL·원 로그 hash는 [색인](dev3-empty-transaction-20261023.json)의
`owner_handoff`와 `artifact_files`를 따른다. h2→h3은 시험1개 추가이며 제품·자원식은 동일하다.
h3→h4는 lint annotation만 달라 정적·runtime·compiler 원 결과를 원 source에 그대로 보존한다.
이미 완료한 AbortEnd/하위 시험은 반복하지 않았다. S5의 profile3×capacity4인12조합은 실제
fixture 생성 코드까지 authored ignored/NOT_RUN이다. 최종 독립 D1은 제한 handoff 단위로 수용했다.
staged30경로와 원 source별 증거를 확인하고 OCR36항목 중 무관6을 명시적으로 제외했다.
원판정은 `review/handoff-final-d1.json`, 정확 문서 상태 반영은 별도 addendum으로 보존한다.
검토자의 첫 hash 감사는 경로 정렬 방식 오류로 중단됐으며 원기록을 보존한 수정 감사에서
source5·staged code/tool/fixture21·색인 asset50을 대조했다. 제품 시험을 다시 실행하지 않았다.
최종 리뷰 후 보존 집계는 case 1,525,780바이트/8MiB, 별도 source·ELF·cache 3,436,429,857바이트다.
집계 receipt와 후속 addendum은 이 시점 이후 추가되며 이전 집계를 덮어쓰지 않는다.

handoff의 memory-port fixture는 기존 Status bootstrap을 사용하며4MiB caller workspace,
1MiB WAL slot,114,688바이트 operation,3개page49,152바이트와 bank53,248바이트를 둔다.
§17의 순수 literal1MiB 한도를 이 전체 fixture에 적용했다고 주장하지 않는다. 테스트 port의
파일·쓰기 이력·snapshot 메모리는 제품 F05와 별도이고 전체 RSS는 측정하지 않았다. 별도 page/WAL
literal 파일은 없다. runtime30초/case·120초전체, compiler180초와 case 증거8MiB 한도 및 별도
source/ELF/cache 보존을 적용했다. 이 인계 성공은 native/current-build 거래 복구의 증거가 아니다.

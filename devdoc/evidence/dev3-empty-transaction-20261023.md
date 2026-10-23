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

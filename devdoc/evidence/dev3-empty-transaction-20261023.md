# DEV3 빈 거래 통합 — 작업·선택 증거

2026-10-23 · **통합 진행 / 소비자 미도달 / DEV3·S5 미완료**. 기준 제품 `4037cb6`,
[운영 계획](../plans/dev3-ai-throughput-improvement.md) `f2adfe7`. 사용자 요청에 따라 단계별
commit과 OCR delegate 독립 검토를 사용한다. 첫 단계는 [빈 AbortEnd 준비 계약](../contracts/dev3-obligation-format.md#empty-abort-preparation)이다.

## 완료 경계와 현재 작업

실제 resident owner의 Begin→RollbackStart→AbortEnd, Status flush/load와 새 process의 동일-build
닫힌 복구·재진입을 이번 통합 종료점으로 고정했다. 순수 bytes 준비·owner 인계에 이어 아래 실제
WAL/RAM 설치를 선택 검증했으며 live flush/load·현재-build 복구·일반 READY는 남아 있다. 구현 담당은 `/root`, 독립 계약/fixture/
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

<a id="live-installation"></a>
## 실제 빈 거래 WAL·RAM 설치

2026-10-24 · handoff commit `7909f9e` 이후 [§19](../contracts/dev3-obligation-format.md#empty-transaction-live)의
실제 Begin→RollbackStart→빈 AbortEnd를 원 owner/pool/WAL에 연결했다.32slot 원장·issued와
adopted 분리·directory census·WAL/LSN/counter 사전 예약·stage 뒤 fence/pending 보존과
durable terminal 최종 검산 뒤 bank 반환을 구현했다. system descriptor는 보존하며 selected
Status는 resident만 지원한다. live flush/load·새 process closed recovery/reentry와 소비자 도달은
미완료다. memory-port I/O의 PASS를 Linux fsync/reopen 또는 전체 DEV3/S5 인수로 세지 않는다.

독립 D2에서 terminal pending과 stable census 분리, 양방향 bank↔home, canonical empty budget,
exact staged descriptor와 실제 stamp/receipt 최종 결합, reader constructor 자원식을 보완했다.
effect 전 l5 static은 live48,728/65,536, bank 포함 retained115,264, metadata163,992/196,608,
flush 여유 포함229,528/262,144바이트를 확인했다. pending96 추가의 handoff conversion64,968은
l1의 변경 소비자 static에서 별도로 확인했다. 이 수치는 source-value이며 물리 stack/RSS나
향후 live flush/recovery F05는 아니다. l6은 자원식/타입이 같아 static을 반복하지 않았다.

| source | 변경·선택 | 원 결과 |
| --- | --- | --- |
| l0 `ff74c3bb…` | 새 live 모듈 export/상수 경로 | compile FAIL3진단, runtime 미실행 |
| l1 `8c7573e6…` | 경로 수정, live/변경 handoff static | build·static2 PASS; 효과 runtime 미실행 |
| l2 `3c6dcbfb…` | 새 시험 helper alias·snapshot·foreign core 수명 | compile FAIL, runtime 미실행 |
| l3 `6a96d8de…` | 남은 test helper 이름 | compile FAIL, runtime 미실행 |
| l4 `725c1fe2…` | 수정·독립 fixture 채택·finish/자원식 보강 | build PASS; runtime 미실행 |
| l5 `9a3c72ac…` | retained pending fence 보강 | build·변경 live static1·fmt·초기checker1 PASS |
| l5 | 신규 compiler2+baseline | baseline/live_sequence PASS; 실제 E0451의4primary를 단일span으로 가정해 sealed_install 검사 FAIL |
| l5 | engine Clippy | 신규 export unused/finish 인수수/macro unused mut FAIL |
| l6 `81c131f9…` | narrow lint annotation, adopted 갱신을 STAGED test cut 앞으로 이동, exact4span checker | 신규20+직접 관련2 PASS,5.371초; fmt/Clippy·변경checker1 PASS |
| l6 | 실패 compiler만 exact 재검증 | sealed_install+새baseline PASS; 원 live_sequence PASS는 반복하지 않음 |
| l7 `94246835…` | seen-mask 도달 marker·고아 원장과 final receipt/stamp 직접 반례2 추가 | 신규2 PASS,0.429초; 기존20+2·static/compiler/checker 반복 없음; fmt/Clippy PASS |

runtime은22개 신규 함수와2개 직접 관련 함수가 각 원 source에서 통과했다. 여러 fault variant를
포함하는 함수를 단일case로 세며 S5의12조합은 NOT_RUN이다. raw offset/SHA 독립 fixture12개
160,800바이트로 실제 INIT2→Begin3→Rollback4→Abort5의 전체 WAL·page·descriptor를 대조했다.
독립 작성자는 제품 encoder/import/output을 사용하지 않았고 통합 담당은 generator와 원 input
hash/offset을 별도로 검토했다. fixture manifest의 상대 생성 경로는 원 `live-literals/` artifact의
identity이며 repository의 복사된 fixture 옆에 생성 스크립트가 있다는 뜻은 아니다.

첫 effect gate는 `review/live-effect-gate-l6.json`으로 수용했다. stage의 adopted 갱신과 panic
hook 순서 문제는 실행 전 독립 리뷰에서 발견·수정하여 l5를 실행하지 않았다. 부정 compiler의
원 FAIL과 초기 검사기 PASS를 보존하고, case-specific exact4span·path/line/column/count/code/
completion/extra-error 부정 검산을 D2로 확인했다. 최종 독립 D1은 제한 WAL/RAM 설치 단위로
수용했다. staged42경로와 OCR44항목(관련42 검토·무관2 제외)을 회계했고 제품/시험/fixture/tool33개는
l7와 일치한다. 문서9개는 최신 source별 상태와 대조했다. 원판정은 `review/live-final-d1.json`,
증거 감사는 `review/live-final-receipt-audit.json`이다.1GiB 보호량은 WAL cap에 따른 정적 상계
273,727,488바이트로만 확인했으며 독립 실패분기 runtime PASS는 주장하지 않는다.

source·binary·정확 command·원 로그 hash·case·시간은 [색인](dev3-empty-transaction-20261023.json)의
`live_installation`과 `artifact_files`를 따른다. timing 로그는 append-only로 이어 기록한다.
memory-port의 최대34page,4MiB workspace·1MiB WAL·114,688 operation·53,248 bank와 파일/쓰기
이력은 시험 메모리이며 전체 RSS는 미측정이다. source/ELF/cache는 case8MiB와 별도 집계한다.
같은 통합의 완료는0/3이며 native 복구가 연결되기 전 helper/commit 수를 효율 향상으로 세지 않는다.

최종 D1 뒤 보존 snapshot은 case 3,504,604바이트/8MiB, 별도 source·ELF·cache 5,937,815,646바이트다.
후속 metadata 영수증은 별도로 추가하며 원 snapshot을 덮어쓰지 않는다.

<a id="live-history-flush"></a>
## 실제 WAL 이력과 live Status flush

2026-10-24 · base `72c4b52` 이후 [§20](../contracts/dev3-obligation-format.md#empty-transaction-flush)의
원 WAL 이력 검산과 기존 DWB/home flush 연결을 추가했다. canonical provisioning 재생과
선택 거래 full materializer로 만든 INIT/Active/Aborting/Aborted의 exact page digest만
private History로 보유한다. 현재 RAM·nonterminal bank와 끝까지 비교하고 역사에 없는
checksum-valid home은 거절한다. fresh live owner 한정이며 일반 recovery 채택은 아니다.

독립 리뷰에서 oldLSN 비교만으로 허용하지 않을 것, INIT full semantic 재생, 현재 bank의
exact descriptor 비교, retained History144바이트와 nested replay 자원식, 완료 prefix의 EOF
fence를 보완했다. 첫 effect D2는 p5에서 수용했다. EOF·invalid adapter count는 세션을
차단하며 알려진 일시적 read 오류는 Unchanged 재시도를 유지한다. 최종 독립 D1은
이력/flush/continuation 한정 수용했다. staged25경로, OCR 대상14/14와 제외 자료11개를
검토했고 blocking finding은0이다. 원판정은 `review/history-flush-d1-final.md`에 보존한다.

| source | 실제 선택과 원 결과 |
| --- | --- |
| p0 `f65e7448…` | 새 이력 helper build PASS; effect 미실행 |
| p1/p2/p3 | test OwnerRole·private import·ancestor 경로 compile FAIL; 원 로그 보존 |
| p4 `bf6db3b7…` | build·초기 static PASS; retained144 누락 발견으로 현재 F05 증거로 쓰지 않음 |
| p5 `c52e0a15…` | 수정 static1·새 runtime6 PASS; prefix fixture의 bytes/length 불일치로 exit−6, 완료 variant0/5 |
| p6 `0d48769d…` | EOF fence·fixture 수정·semantic 무효과 검산, 직접 관련3+신규3 PASS; 변경 layout static2 PASS |
| p6 | continuation은 INIT-only reader 사용으로 FAIL·대기 worker timeout; native는 copied fixture 경로 오류로 FAIL |
| p7 `47ec8cf9…` | 위 시험 observer/path만 수정; 실패 continuation1·native1만 PASS, 정상 함수 반복 없음 |
| compiler export | sequential flush API·private History2span E0451의 신규2 PASS, 진단 checker1 PASS; fmt PASS·Clippy 인수 수 lint FAIL |
| p8 | immutable witness와 loan을 분리 유지하는 이유를 좁은 lint annotation으로 명시; fmt·engine Clippy PASS, runtime/compiler 반복 없음 |

신규 memory 함수11개와 Linux 함수1개가 각 원 source에서 완료됐다. semantic14·prefix5·
home4·I/O6ordinal×before/after12 등 변형 수는 함수 수와 별도로 구분한다. p5의6개 중
수정된 semantic 검산과 EOF 분류의 직접 소비자 retry2개만 p6에서 재선택했다. 이전 source의
PASS/FAIL은 대체하지 않았다. private proof compiler는 실제 실패 completion과 단일 E0451의
정확2개 field span을 확인하고 다른 오류·파일·행·열·개수·성공 completion을 거절한다.

Linux worker는 actual Active→Aborting→Aborted3회 flush의31effect·전체 home page·
WAL prefix/bytes 불변을 확인했다. 기존 독립 raw literal과 전체5frame WAL을 비교했고,
child 종료 뒤 별도 parent가3개 관측 page와 닫힌 WAL/최종 disk page를 다시 비교했다.
최대 wrapped handle2·DB fd4, 종료 후 각각0이다. **이는 closed recovery/reentry 시험이 아니다.**
원 native 실패는 첫 flush 후 fixture read가 실패했으며 수용된 native 사례로 세지 않는다.

source-value는 call53,384/65,536, retained115,408, conversion65,400/65,536,
whole234,328/262,144바이트다. 변경된 ResidentStatus와 기존 load layout static은 p6에서
직접 확인했다. 물리 stack/RSS는 미측정이다. runtime30초/case·120초batch, compiler180초와
case/review8MiB를 적용했다. native DB는 각각15파일이며 성공22,137,568·실패22,131,488바이트를 시험 객체로 별도 보존한다.
source·ELF·cache 및 원 실패 DB도 삭제하거나 PASS로 소급 재분류하지 않는다.

정확 command/source/binary/log hash와 시간·보존량은 [색인](dev3-empty-transaction-20261023.json)의
`live_history_flush`와 `artifact_files`에 있다. design→검증 시각을 append-only timing에 남겼고
상세 탐색/구현/대기 시간을 사후 추정하지 않았다. 같은 통합의 다음 소비자는 live load와 새
process 현재-build 복구·재진입이다. S5 profile3×상태4의12조합은 authored NOT_RUN이며
소비자 도달0/3·효율 개선 비교·DEV3/S5 미완료를 유지한다.

<a id="live-load"></a>
## 최신 live Status의 실제 재적재

2026-10-24 · base `a7af5ab` 이후 [§21](../contracts/dev3-obligation-format.md#empty-transaction-load)의
clean frame 재적재를 같은 owner에 연결했다. 최초 destination snapshot을 마지막 설치까지
유지하고, 전체 directory와 원 WAL full history가 확인한 최신 exact page만 private proof로
기존 leaf installer에 전달한다. 마지막 disk read·allocator digest/stamp·prefix 재검산을 거친다.
기존 provisioning-only load 의미는 유지하며 새 pool·retained field·page-sized local은 없다.

독립 D2는 최초 snapshot 수명, latest-only 권한, sealed6필드, required-home EOF 분류와
source-value 합산을 검토했다. q2의 첫 effect gate를 수용했고, 후속 리뷰에서 retained DPT
claim의 새 load 소비자 시험을 추가했다. 최종 독립 D1은 live load 한정 수용했다. staged24경로·OCR13/13과 제외11개를
검토하고 제품 blocker0을 확인했다. 원판정은 `review/load-d1-final.md`에 보존한다.

| source | 실제 선택과 원 결과 |
| --- | --- |
| q0 `07dbaf22…` | enum 오류 변환 compile FAIL; runtime 미실행 |
| q1 `d36eba90…` | fixture의 잘못된 flush API compile FAIL; runtime 미실행 |
| q2 `87eaac0c…` | build·새 resource1·occupied3상태 재적재와 같은 actor 계속 진행1 PASS |
| q3 `864a6863…` | diagnostic type·foreign core 수명 compile FAIL; runtime 미실행 |
| q4 `af893319…` | admission/retry/EOF·count·corrupt/final drift/unwind 신규5 PASS; 변경 census 직접1 PASS |
| q4 | stale Free의 마지막 fenced healthy getter와 pin holder의 조기 Drop 때문에 시험2 FAIL; 제품 rejection 앞선 단언은 성공 |
| q4 | Linux native1과 별도 parent whole-page oracle PASS; 새 process recovery는 미실행 |
| compiler export | 새 load_sequence/sealed_load2만 PASS; 기존 flush/history probes 반복 없음 |
| q5 `cacbfc78…` | 실패2의 observer/owner 수명 수정만 재검증 PASS; 새 retained claim은 API의 Busy 대신 Frame(Busy)를 기대해 FAIL; 변경 verify checker2 PASS |
| q6 `5e62ecf5…` | claim 기대 cause만 정정해 신규1 PASS; fmt PASS, 의도적 non-Drop token forget의 Clippy lint FAIL |
| q7 | 그 한 문장에 이유와 narrow lint annotation 추가; fmt/engine Clippy PASS; runtime/compiler 반복 없음 |

신규 runtime9개는 각 source에서 완료했다. 알려진 read 실패는 최초/최종 read의4변형,
EOF/count는6변형+checksum 손상, pin/poison은2변형으로 함수 수와 구별한다. final drift는
선택 page3회 읽기 중 마지막에 변경을 주며 WAL12회 읽기가 앞서 도달했음을 단언한다.
terminal 후 예전 Free home을 되돌리는 반례는 broad directory 통과 뒤 full history 거절을
검산한다. claim은 원 allocator의 actual capture token을 잃은 채 clean 목적지에 load를 요청하며
공통 진입의 Busy/Unchanged·무I/O·frame/bank/prefix 보존을 확인한다. 원 FAIL은 삭제하지 않는다.

Linux native는 Active/Aborting/Aborted 각각 cold page와 occupied page를 교체하는6load를
실행했다. 각 직전 실제 home 전체와 loaded 전체가 같고 DB inventory가 바뀌지 않았으며
별도 parent가3개 input/loaded page와 닫힌 최종 disk page를 대조했다. wrapped handle 최대3·
DB fd 최대5, 종료 후 각각0이다. 이는 새 process recovery/reentry 증거가 아니다.
compiler 부정 사례는 실제 실패 completion·단일 E0451·정확6필드 primary span을 확인한다.
checker2는 변경된 verify의 기존2span 및 새6span/부정 입력 소비자로만 선택했다.

source-value는 memory call64,776/65,536·native Trace Writer call64,840/65,536,
retained115,408·metadata180,184/196,608·whole245,720/262,144·conversion65,400이다.
물리 stack/RSS는 미측정이다. runtime30초/case·batch120초·compiler180초·case/review8MiB를
유지하고 실제 native DB15파일 및 source/ELF/cache는 별도 보존한다. source별 command·binary·
원 로그 hash·시간과 보존량은 [색인](dev3-empty-transaction-20261023.json)의 `live_load`에 있다.
S5 profile3×state4의12조합은 authored ignored/NOT_RUN이다. 다음 소비자는 새 process의
현재-build 빈 거래 복구·재진입이며 통합 소비자 도달0/3·효율 비교·전체 DEV3/S5는 미완료다.

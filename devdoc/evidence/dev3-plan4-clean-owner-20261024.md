# DEV3 plan4 clean owner — source별 선택 검증

2026-10-24 · **필수78 source별 PASS / 최종 독립 D1 제한 수용**.
기준 `b3098c1ffe0d555c0021ceb9a4ee5ce958dd7634`, 통합·실행 `/root`,
제품 준비 `/root/mixed_history_implementation`, 독립 시험 준비 `/root/mixed_owner_test_preparation`,
독립 검토 `/root/dev3_independent_review`.
[소유 계약](../contracts/dev3-plan4-clean-owner.md),
[중앙 시험 목록](../plans/dev3-test-plan.md#plan4-clean-owner-followup),
[source별 원본 색인](dev3-plan4-clean-owner-20261024.json)을 따른다.
[최종 독립 D1](../../artifacts/dev3/plan4-clean-owner-20261024/review/final-d1.md)은
변경47개 경로와 source별 결과를 검토하여 이 clean 소비 범위를 제한 수용했다.
원 compiler campaign HOLD와 준비 peak FAIL은 그대로 유지한다.

원 Mutable owner의 CAS, fence, SharedPool, caller backing과4MiB DWB loan을 유지하며
canonical plan4 page를 bind/load한다. 실제 Catalog 조회는 allocator+anchor+leaf의3pin,
정리 큐 검산은 allocator+root+jobs의4/4/3pin 창을 사용한다. 원 역할에서 나온 private
Plan4Target은 값 기대값이며 살아 있는 owner가 namespace·membership를 다시 검산한다.
G5는 clean genesis에 한정된다. dirty issuer·가짜 flush·recovered owner·live 거래를
공급하거나, 깨끗한 조회 결과를 G6의 task12 복구·live 재개 증거로 바꾸지 않는다.

독립 입력은 기존 plan4 raw/참조42개274,720bytes를 불변 재사용했다. 새 raw 생성은0이다.
[manifest](../../tests/fixtures/dev3-plan4-clean-owner/manifest.json)의 SHA는
`5f979e7f9fff83ec97da42a5bfd4722d1b2b8f74faa95468f02883b17af44b05`이며,
제품 encoder를 기대값으로 쓰지 않는다. 기존 raw manifest
`82880ea05480b9779bb2d5764c88f6423d2c74da1d4e68e8bf2bb0092e8eba22`와
각 raw의 길이·SHA를 원 source-bound recipe에 연결한다. 완료한 하위 시험은 호출하지 않았다.

| frozen source | 원 준비·실행 결과 |
| --- | --- |
| r0 `be21c2e8…` | no-run compile FAIL. 제품 allocator mutable reference 이동 E0382와 시험 foreign core 수명 E0597 보존. 제품 실행0 |
| r1 `86a5103f…` | compile/fmt PASS, Clippy FAIL. test-only census import와 range-loop 진단 보존. 새 mocked checker13 PASS, 제품 실행0 |
| r2 `7d620404…` | compile/fmt/Clippy PASS. static2 PASS. memory B01–03 PASS 뒤 B04 oracle ABORT; 별도 B08 PASS. native P1 PASS. compiler10 exact 결과 뒤 최종 receipt 예약 초과 HOLD |
| r3 `ed706d14…` | setup와 G5 mutation trace 기준 및 guard 밖 assertion 수정. compile/fmt/Clippy PASS. 원 완료4를 제외한 memory71 선택에서32 PASS, C02 FAIL,38 NOT_RUN |
| r4 `36d5645c…` | candidate의 eligible empty 우선 선택 보완. compile11.881199초/fmt0.966456초/Clippy10.994014초 PASS. 실패 C02와 미실행38의39 입력만 선택하여 모두 PASS |

r4 source는 `36d5645c2e5f3b5342b2e168a6acf388c35b729a415a9ee04689d4170612e7b9`,
ELF는 `8db64629269cf10ff48300e3f88a3f17a02d4dcb65e095530387baf95735390e`,
43,582,032bytes다. 각 source/export/ELF/원 stdout·stderr는 JSON에 개별 결합했다.
이전 genesis export의 불변 backing과 정확 overlay를 사용하며 동시 DEV4 변경은
시험 source에 섞지 않았다. source view의 논리 길이와 공유 backing은 구분한다.

필수78은 memory75+static2+native1이며10 exact selector로 선택한다.
S5 추가128은 memory126(SL10·SS53·SI56·SH7)+native profile2/3이며5 selector로
작성·ignored/NOT_RUN이다. native worker 하나가 공유되어 전체206은14 unique selector다.
compiler10과 checker13은 제품 case 분모에 더하지 않는다.

r3 종료 당시 memory 원장은 **서로 다른 PASS36, C02 FAIL1, NOT_RUN38**였다.
r2의 완료 B01–03/B08(4)와 r3의 B04–07/B09–16(12), M01–08(8),
J01–11(11), C01(1)은 중복되지 않는다. 미완39는 C02–05, I01–14, Q01–08,
R01–13이었다. [정확 ID 원장](../../artifacts/dev3/plan4-clean-owner-20261024/memory-r3-ledger.md)에
source별 PID·marker·전체 목록·원 log SHA를 보존한다. r4는 완료36을 제외한39를 모두 PASS했다.
따라서 memory75와 원 r2의 static2/native1을 합한 필수78이 source별로 완료됐다.
r4 결과 SHA는 `b511a2d954387f5371e35f1655e404dbdbed36b861877851432b2157a090dbb3`다.

원 r2 B04는 fresh setup mutation375를 baseline으로 잡은 뒤 trace를 비워 실제 관측0과
비교한 시험 oracle 오류였다. mutex를 잡은 assertion의 panic이 Drop 중 poison을 만나
SIGABRT(exit−6)가 됐다. BEGIN은 있으나 FAIL marker가 없어 원 receipt의
NOT_RUN/active B04를 유지한다. r3은 setup 종료 직후 기준을 잡고 각 중간 reset 전
G5 mutation0을 검사하며 assertion 전에 guard를 해제했다. fault 순번과 제품 source를
바꾸지 않았다. r3의 B04 PASS는 원 ABORT를 삭제하거나 소급 PASS로 바꾸지 않는다.

원 r3 C02는64frame의 실제 소비자 경로 뒤 등록 page 수 `left5/right9`에서 FAIL했다.
기존 candidate가 unused slot보다 먼저 만난 occupied slot을 선택하는 경우가 있었다.
r4의 변경은 기존 candidate Option 안에서 eligible empty를 먼저 선택하도록 좁혔으며
full scan·dirty 검사·required full-address 보호·MAX 제외의 검산은 유지한다.
r3 C01과 r2 native4frame 완료를 반복하지 않고 C02와 미실행 입력만 준비했다.
r4의39 PASS는 원 C02 FAIL을 보존한 후속 결과다.
[독립 후보 선택 D2와 원 결과 감사](../../artifacts/dev3/plan4-clean-owner-20261024/review/empty-first-candidate-d2.md)는
이 변경·39개 선택·이전 완료36개 제외를 수용했다. 기존 static/native/compiler의 source별
증거와 새 source의 영향 분석을 구별하며 최신 ELF에서 native를 실행했다고 주장하지 않는다.

최초 r3 CLI 선택은 `empty selection or completed case replay`로 exit1이었다.
그 시점 output directory 생성과 제품 invoke는0이며 별도 rawlog/receipt는 없다.
root의 원 도구 관측 wall0.076933153초를 runtime case FAIL과 분리해 기록한다.
r4 선택 기록은 이를 manual memory audit에 보수1초로 charge한다. 뒤 실제 memory-r0가
생긴 사실과 구별하고, 원 제품 receipt 시간을 변경하지 않는다. memory 원 wall 합은
11.701755초, process8.495655초, audit3.206101초다. 이 별도1초를 더한
보수 aggregate12.701755/audit4.206101초는 process180/audit30/aggregate210초 한도 안이다.

static2는 r2에서 처음 한 번 실행했다. actual memory/native의 source-value peak는
47,908/49,068bytes로65,536 한도 안이며 whole 값은113,444/114,604bytes다.
retained owner와 active pool을 식에 포함했고 원 core load2824/candidate2040,
pool17216/directory5160/mode1 및 old common delta0을 별도 출력했다.
caller operation114,688와 원 DWB4,194,304bytes는 loan으로 구분한다.
이는 실제 타입과 source 수명의 회계이며 전체 stack/RSS 상한 증명이 아니다.
후속 source에서 같은 완료 static을 다시 실행한 것으로 표시하지 않는다.

B08은256/512/1024/2048/4096의 유한 actual File wrapper 후보 중 첫 적격2048을
원 ST01 receipt에 결합했다. 실제 Fresh/genesis/DWB admission은 통과하지만 G5 peak
66,340으로65,536을 넘는다. 실제 setup 뒤 bind Capacity가 I/O 없이 거절됨을 별도
B08 실행으로 확인했다. 가짜 Fresh owner나 runtime 타입 탐색은 사용하지 않았다.

native P1은 r2의 새 worker PID247411 한 개가 실제 fresh setup 후 G5 clean consumer를
수행했다. stable15file/22,069,248bytes를 독립 full-byte oracle로 검산하고 before/after의
이름·길이·inode·SHA가 같음을 확인했다. setup mutation375/entropy1과 G5 mutation0/
entropy0을 분리했다. actual File peak3·DB FD peak5·종료0이다. 제품1.530637초,
전체1.900796초, 보수 receipt reserve 포함 retained22,434,609/log365,361bytes로
native30초·DB24MiB·P1 history64MiB·log8MiB 한도 안이다. 독립 읽기 audit는 실제
retained22,390,486/log321,238bytes를 확인했다. G5는 이 한 PID의 clean 소비자 증거이며
3PID 복구/live 재개를 주장하지 않는다. 완료 P1은 r4에서 반복하지 않는다.

compiler CP01/02 positive와 CP03–10 negative의 code/file/count/primary span10개는
원 r2 source에서 각각 한 번 검산됐다. 제품 process 실행은 아니다. 원 report는 PASS를
썼으나 실제 최종 receipt91,400bytes가 내부 예약65,536을25,864bytes 초과하여
별도 `finalization-failure.json`이 HOLD를 남겼다. 독립 read-only audit는 열 개의
정확 진단을 수용하고 원 campaign HOLD를 유지했다. process18.873050/audit6.459620/
aggregate25.332670초 및 실제 diagnostics+run/result/HOLD225,018bytes는 원
120/30/150초·8MiB 한도 안이다. 완료 probe를 다시 실행하지 않았다.

후속 도구 수정은 내부 최종 receipt 예약만131,072bytes로 바꿨다. 총8MiB와 시간 한도는
유지한다. 첫 mocked checker13은 r1에서0.113920초 PASS였고, 이 변경에 대응하는 H12만
별도 checker-r1 source에서0.113898초 PASS했다. 13 unique checker와 변경 함수1회이며
14개 새 checker로 세지 않는다. 원 예약 초과 HOLD를 새 도구의 PASS로 덮지 않는다.

준비 총704MiB는 source160/ELF192/compile192/records32/native128MiB로 분리한다.
원 compiler metadata copy 때문에 compile backing221,215,769bytes가201,326,592 한도를
초과한 **원 preparation FAIL**을 보존한다. 독립 D2 뒤122개 불변 metadata pathname/
bytes/SHA를 유지하면서 같은 내용만 공유했다. 논리103,185,015bytes, distinct
19,494,744bytes, 줄어든 backing83,690,271bytes와 inode/link 변경을 전후 기록했다.
mutable target/source/ELF/log/receipt와 alias하지 않는다. 새 현재 회계의 compile
137,525,498bytes PASS는 과거 peak FAIL을 소급 변경하지 않는다.
r4 build 후 현재 backing은 source113,448,391/ELF174,322,008/compile137,528,050/
records10,662,555/native22,396,428bytes로 각 한도 안이다. 원 preparation FAIL과
후속 현재 PASS를 함께 보존한다. 최종 D1 뒤의 실제 보관량은 JSON의
`final_retention_accounting`으로 별도 결합했으며 각 한도 내임을 확인했다.

원 source·ELF·fixture·receipt·리뷰는
`artifacts/dev3/plan4-clean-owner-20261024`에 보존하며 JSON은 개별 원본과
독립 리뷰를 hash로 연결한다. 독립 D2·실행 gate 수용과 실제 시험 결과를 구별했고,
시험 작성·제품 구현·독립 검토를 병렬로 분담했다. AI 완료 소비자는1/3을 유지한다.
G5 최종 독립 D1은 제한 수용했다. G6 live 큐/empty loser 복구·새 epoch·같은 pool 인계,
Catalog CREATE/CLR·첫 행·전체 DEV3/S5는 미완료다. 이 문서 작성 중 제품·시험·checker·
문서 verifier·commit·push를 실행하지 않았다.

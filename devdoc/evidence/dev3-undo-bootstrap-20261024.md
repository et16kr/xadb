# DEV3 Undo bootstrap 역할 — 제한 선행 인수

2026-10-24 · **독립 D2 수용 / 신규33assertion PASS / native1·compiler4+baseline·checker7 PASS / 최종 독립 D1 제한 수용**.
기준 commit `47a4194`, 구현 `/root`, 독립 검토 `/root/dev3_independent_review`.
[소유 계약](../contracts/dev3-status-bootstrap.md#undo-bootstrap),
[source별 색인](dev3-undo-bootstrap-20261024.json),
[중앙 시험 계획](../plans/dev3-test-plan.md#undo-bootstrap-followup)을 따른다.

새 plan3의 immutable Undo3/gen1/kind8/level0 역할을 공급한다. 같은 namespace의 Status2를
유지하며 기존 설치/readback 뒤만 borrowed UndoRole을 발급한다. old allocator/Status adapter와
raw recovery inspector의 plan1/2 거절 경계는 유지한다. 이 단위는 다음 mixed root INIT/flush/load/
현재-build 복구를 위한 선행이며 그 소비자 도달이나 live undo·DEV3 완료가 아니다.

| source | 실제 선택 결과 |
| --- | --- |
| r0 `fcdb2ab5…` | 새 native 시험 DirectoryPort import 누락 compile FAIL; runtime0 |
| r1 `7299bceb…` | compile PASS. UB01 static/literal4와 독립 first-effect gate 뒤 UB02–05 29assertion PASS; 총5함수33assertion |
| r2 `9e23f731…` | 같은 Rust ELF. 신규 checker6·fmt PASS. Clippy의 scalar8개 source-value식을 bits 계산으로 해석한 lint FAIL |
| r3 `4438242b…` | source-value 의미를 설명하는 narrow allow만 Rust에 추가. 새 총시간 deadline checker1·fmt·Clippy PASS. 신규 native1·compiler4+baseline PASS; 완료 runtime33/checker6 재실행 없음 |

UB01의 profile별 literal3과 layout1은 successful create4건이 아니다. UB04는 invalid profile,
readback의 plan2 drift, read 오류, descriptor write 뒤 unwind의4개 실패 경계를 독립 raw 파일·효과
prefix와 대조한다. UB05 consuming 거절은 파일/scratch 불변과 원 owner 소비/CallerMustResolve를
구분한다. Source-value branch는 UndoRole16/Result48/MutableResult56, fresh712/mutable1080이다.
원 component65,536·caller scratch49,152·generic alignment/File 한도를 유지하며 stack/RSS 주장은 없다.

입력은 원 plan1의 독립 literal·effect model에서 plan3의120/280/checksum만 바꾸어 만들었다.
새 제품 encoder나 실행 결과로 기대값을 만들지 않았다. generator·dependency31·raw6의 hash와
독립 read audit를 보존한다. S5 tuple90=mandatory alias12+추가78은 작성·ignored/NOT_RUN이다.
compiler UC00–03+baseline 및 실제 Linux creator1·checker는 각각 별도 분모로 기록한다.
전체 DEV3/S5·plan3 recovery·AI 소비자2/3·효율 비교를 승격하지 않으며 문서 verifier는 push 전에만 실행한다.

실행과 review 원본은 `artifacts/dev3/undo-bootstrap-20261024`에 source·ELF·명령·시간·stdout/stderr와
함께 보존한다. r0 compile FAIL과 r2 Clippy FAIL을 삭제하거나 최종 source PASS로 덮지 않는다.
새 creator의 실제 install/readback 시험과 pure inspector의 unsupported 거절을 current-build recovery
성공으로 혼동하지 않는다. 다음 작업은 authentic Free root bytes·PageToken consume·native UndoRoot·
allocator delta·같은 pool·복구 census를 실제 연결해야 한다.

실제 native PID72219는 새 DB의9파일5,296,128바이트·350효과를 대조했고 wrapped File peak3/final0,
DB FD live3/final0을 확인했다. 전체 identity/input/파일 검산을 포함한 실행 시간은0.456388초였다.
compiler는 UC00–03과 실제 baseline의5회 호출을 실행했고 합 compiler시간10.387155초로 통과했다.
source/probe별 Cargo metadata·정확 진단·toolchain/checker identity를 별도 보존한다.
r2 checker6와 r3 deadline checker1은 서로 다른 신규 선택이며 합7이다.
최종 독립 D1은37경로 전수 검토·누락0으로 제한 수용했다. 원 native 파일과 compiler 진단을
독립 대조했으며 보관량 집계 지적을 해소했다. 원 판정은
`artifacts/dev3/undo-bootstrap-20261024/review/d1-final.md`에 보존한다.

보관량은 물리 disk block이 아닌 파일의 논리 bytes로 구분한다. r0–r3 source/ELF753,729,787,
compiler baseline source100,047,309·probe source400,191,065·보존 metadata55,296,538·전용 target cache162,086,428,
native DB5,296,128바이트다. receipt/review/tool의 최종 snapshot과 합계는 source별 JSON에 기록한다.
공유 build cache는 원 `empty-transaction-recovery-20261024/target`에 별도 보존하며 이 단위 합계에
중복 합산하지 않는다. 정적/실행 증거를 삭제하거나 cache를 실행 결과로 세지 않는다.

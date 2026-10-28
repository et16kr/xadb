# 현재 구현

2026-10-29 [Row Undo·원 Job·Status 관계](../evidence/module-independence-survey-20261026.md#row-undo-images): main `09173aa`에서 Undo6개 전이·실제 H/nearest Job·같은 frame Status 연결을 구현하고 raw17쌍·신규9개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-29 [Row Status의 실제 페이지/frame 결합](../evidence/module-independence-survey-20261026.md#row-status-images): main `b50e734`에서 Status9개 전이와 native/redo 관계를 구현하고 독립 literal24쌍·source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [Row scope3 frame 검산](../evidence/module-independence-survey-20261026.md#row-scope3-wire): main `2fdff66`에서 별도14opcode·다중 statement core·native/redo 입구를 구현하고 source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [Row actor의 실제 페이지 관계 검산](../evidence/module-independence-survey-20261026.md#row-actor-graph): 사용자의 전체 계획 완료 요청에 따라 `main` `d27bfa4`에서 재개했다. 한 actor의 Base4/Status/Undo/Row graph를 구현했고 source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [shared CREATE 제품 예약](../evidence/module-independence-survey-20261026.md#c3-create-admission): 원 statement·source/gate를 기존 예약에 묶고 원 private 반환을 공급한다. source별 compiler4·신규 runtime9/source1과 최종 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 전체 M4/M5는 미완료다.

2026-10-28 [첫 Catalog statement의 거래 완료 연결](../evidence/module-independence-survey-20261026.md#c3-catalog-capsule): main `4ac878c`에서 실제 Forward를 원 statement에 등록하고 consuming Commit/full abort를 연결한다. r3 compiler3·r4 runtime9·r5 runtime4 PASS와 원 FAIL을 보존했으며 최종 독립 D1/D2 한정 수용·필수 수정0이다. shared CREATE 제품 admission·Row·M4/M5는 미완료다.

2026-10-28 [실제 빈 쓰기 capsule](../evidence/module-independence-survey-20261026.md#c3-write-capsule): main `b35150c`에서 원 Begin actor·빈 statement·소비형 빈 abort를 연결한다. 원 static2 FAIL을 보존하고 r3 compiler3/runtime14·source 보완 r4 runtime2·phase 보완 r5 runtime3을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 자동 Status/일반 mutation·shared CREATE·M4/M5는 미완료다.

2026-10-28 [CREATE 원 flush 대여](../evidence/module-independence-survey-20261026.md#c3-create-lease): main `3fa5f6f`에서 예약의 disjoint 원 자원·연속 flush lease를 구현했다. r1 compile FAIL을 보존하고 r2 compiler3/runtime10/nativeN01을 통과했다. 최종 독립 D1/D2 한정 수용이며 shared 제품 admission·M4/M5는 미완료다.

2026-10-28 [CREATE 원 자원의 borrow 본문](../evidence/module-independence-survey-20261026.md#c3-create-parts): main `8613c53`의 원6자원을 빌리는 private view로 실제 본문을 분리한다. r1 fixture FAIL을 보존하고 r1 compiler2/runtime5 및 r2 수정·미실행 runtime2/nativeN01을 검증했다. 최종 독립 D1/D2 한정 수용이며 shared 제품 CREATE와 M4/M5는 미완료다.

2026-10-28 [공유 pool CREATE claim](../evidence/module-independence-survey-20261026.md#c3-pair-shared): main `c845ea7`에서 exact 후보·공유 종료·ordinary gate를 구현한다. r1 compiler4/runtime12/nativeN01 3PID PASS, 최종 독립 D1/D2 한정 수용이며 actual shared reservation과 M4/M5는 미완료다.

2026-10-28 [CREATE 기존 대상 key 접근](../evidence/module-independence-survey-20261026.md#c3-pair-existing): main `4e68a4d`에서 typed subset·실제 Reserve/Forward 준비/Cancel 연결을 진행한다. r2 compiler5/runtime8/nativeN01 3PID PASS·r1 주입 FAIL 보존, 최종 독립 D1/D2 한정 수용이며 shared admission·M4/M5는 미완료다.

2026-10-28 [exclusive CREATE pair 원자 leaf](../evidence/module-independence-survey-20261026.md#c3-pair-atomic): 원 가변 배제를 유지하며 close/claim·finish/cancel을 같은 registry leaf에 모았다. r3 runtime8/nativeN01 3PID PASS·원2개 FAIL 보존, 최종 독립 D1/D2 한정 수용, 공유 gate/typed subset·M4/M5 미완료.

2026-10-28 [C2 Commit 준비·적용 단일 예약](../evidence/module-independence-survey-20261026.md#c2-commit-call): 외부 Prepared 보유 경로를 닫고 준비부터 원 Pending/Drain/publish까지 source를 유지한다. source별 runtime11/compiler4·원 N03 2PID PASS, 원3개 FAIL 보존·최종 독립 D1/D2 한정 수용다. CREATE/일반 floor/Row·M4/M5는 미완료다.

2026-10-28 [C2 Begin·AgingStart progress](../evidence/module-independence-survey-20261026.md#c2-start-progress): 두 시작 경로를 원 gate/source의 동기 MutationPort로 공급하고 원 발급 gap·deep health·Root/Status 선택을 유지했다. r2 compiler3/runtime5/native N02(3PID) PASS·r1 compile FAIL을 보존하며 shared+fixed memory62390B/native62966B, recovered63270B/63846B다. CREATE admission·일반 floor·Commit Prepared lifetime·Row와 전체 M4/M5는 미완료다.

2026-10-28 [C2 Catalog lifecycle progress](../evidence/module-independence-survey-20261026.md#c2-lifecycle-progress): RollbackStart/AbortEnd/CancelReserved/ReleaseRole을 원 gate/source의 동기 MutationPort로 공급했다. 원 flusher borrow·quiescence·ordinary apply를 유지하며 r3 compiler4/runtime12/native N02(3PID) PASS·r1/r2 compile FAIL을 보존한다. shared+fixed memory62390B/native62942B·port192B이며 일반 floor·Begin/Aging/CREATE·Row와 전체 M4/M5는 미완료다.

2026-10-28 [C2 Catalog 회수 동기 호출](../evidence/module-independence-survey-20261026.md#c2-retirement-call): 준비·적용·결과 정리를 원 gate/source 한 번에 연결하고 독점 recovery도 수신했다. compiler4/runtime13·native N02(3PID) 각1회 PASS. 회수 caller-retained gap을 닫았고 고정 charge42198B/port192B이며 일반 entry/return floor·Commit lifetime·다른 writer/Row와 전체 M4/M5는 미완료다.

2026-10-28 [C2 공유 Catalog 회수 준비 body](../evidence/module-independence-survey-20261026.md#c2-retirement-preparation): MutationPort 준비의 replay/page 접근·결과 cleanup까지 원 gate/source 예약을 유지한다. 선택12 PASS·원 compile FAIL2 보존. 고정 charge42074B·port192B 유지, caller-retained Prepared·entry/return floor·전체 M4/M5는 미완료다.

2026-10-28 [C2 Catalog 회수 progress](../evidence/module-independence-survey-20261026.md#c2-retirement-progress): main `c217fbf`에서 CLR/Prune/RootFree/TerminalFinish를 원 read gate와 고정 source 예약에 연결했다. Maintenance 역할·장기 snapshot horizon을 유지하고 source→gate 정리를 수행한다. source별 선택7+6개 PASS·원 compile/시험 FAIL을 보존하며 native shared+회수62402B, Pending+Full65502B다. 준비 경로·다른 writer·entry/return floor·Row progress·M4/M5는 미완료다.

2026-10-28 [Catalog Commit writer 창의 main 통합](../evidence/module-independence-survey-20261026.md#writer-progress-merge-main-20261028): `854bd8d..af54a4e`의 변경30파일을 독립 리뷰하여 필수 수정0으로 확인하고 사용자 병합·branch 삭제 지시를 반영한다. 제품·시험 소스는 리뷰한 branch와 같고 기존 runtime59·빌드/Clippy3 결과를 재실행 없이 보존한다. 재개 위치는 `/home/et16/work/xadb`의 main이며 M4/M5는 미완료다.

2026-10-28 [C2 Catalog Commit writer 창](../evidence/module-independence-survey-20261026.md#c2-writer-progress): branch `claude/module-independence-remaining`에서 Commit이 새 read를 닫고 admitted read 반환을 기다린 뒤 원 isolation ledger에 stage 몫을 예약하고, Pending·publication 축소와 Drain 뒤 install 증가를 연결했다. r1 compiler3·runtime59 각1회 PASS, 사전 독립 검토 REQUIRED3 해소·재확인 신규 REQUIRED0이다. native Pending+Full65470B이며 다른 writer·진입/반환 floor·Row progress·M4/M5는 미완료다.

2026-10-28 [모듈 개발 브랜치의 main 통합](../evidence/module-independence-survey-20261026.md#merge-main-20261028): `cbc09f2`의 공통 core·Commit phase·복구 반환 수명 보완과 `76f91e0`의 후속 C2/C3·Row 순수 검산을 결합한다. 이후 개발 기준은 main이며 진행 중 C2 초안은 별도 stash로 보존한다. 변경된 연결에 한정한 runtime13·같은 ELF의 native N03 1회 PASS를 보존하며, 독립 최종 D1/D2 한정 수용·필수 수정0이다. M4/M5 완료를 주장하지 않는다.

2026-10-28 [C2 Catalog 결과 소비 수명](../evidence/module-independence-survey-20261026.md#c2-call-scope-main): 원 source/progress를 private CatalogCallScope에 유지하고 선택 query의 마지막 결과 소비 뒤 checked finish한다. runtime18·compiler4 각1회·production/test build·선택 Clippy PASS. 독립 OCR 전체23파일 한정 수용·시험선택 지적1건 보완·필수 수정0. 현재 Full+Pending66468/66604B는65536B 기준 FAIL이며 writer/progress·전체 진입/반환 합·M4/M5는 미완료다.

2026-10-28 [C2 Drain 완료/content guard 인계](../evidence/module-independence-survey-20261026.md#c2-drained-handoff-main): 원 pin·Installing을 private Drained로 이동하고 정상 미완료 Drop의 fence-before-pins를 검증했다. 변경 경계 선택11개 각1회·production/test build·선택 Clippy PASS, main4phase/core·전체 peak 상계 유지. 독립 OCR 전체18파일 한정 수용·지적1건 수정·필수 수정0이며 writer/progress·전체 동시 합·M4/M5는 미완료다.

2026-10-28 [C2 실제 WAL wait source 관측](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source-main): test-only 호출·반환 계산과 신규 exact1개1회 PASS. main core 포함 Full+Pending 합66116/66252B는65536B 기준 FAIL이며, 관측 PASS를 동시 admission 인수로 승격하지 않는다. 리뷰 지적1건의 배치를 수정하고 선택 Clippy PASS·OCR 전체15파일 독립 D1/D2 한정 수용·필수 수정0이다. 제품 상계·동작은 유지하며 Drain 인계·writer/progress·M4/M5는 미완료다.

2026-10-28 [C2 Aging/Root source 계산](../evidence/module-independence-survey-20261026.md#c2-aging-source-main): 실제 Root2 decode·순차 Aging 수명과 공통 XOD/XTS 계산을 연결했다. main 원 core·Commit4phase를 유지하며 Full charge41576→37724B, 전체 peak는 그대로다. r1 test build 경로 오류를 보존하고 test-only 정정 뒤 선택4개 각1회 PASS·OCR 전체14파일 독립 D1/D2 한정 수용·필수 수정0이다. writer/progress·전체 동시 admission·M4/M5는 미완료다.

2026-10-27 [C2 Commit 준비 자료 수명 분리](../evidence/module-independence-survey-20261026.md#c2-pending-image-main): stage가 준비 자료를 소비하고 compact 불변 image를 Pending에 넘긴다. 선택 memory11개 각1회 PASS·OCR 전체11파일 구현/memory 한정 수용 뒤, [남은 native N03](../evidence/module-independence-survey-20261026.md#c2-pending-image-native)를 같은 source/ELF로1회 PASS했다. 후속 OCR7파일 독립 D1/D2 수용·필수 수정0으로 이 C2 단위의 선정 검증 인수를 마쳤다. 원 권한·공개 순서를 유지하며 전체 writer/progress admission·M4/M5는 미완료다.

2026-10-27 [C2 상주 Catalog 고정 class](../evidence/module-independence-survey-20261026.md#c2-resident-main): 원 allocator/Status/Undo의 I/O 없는 조회와 고정 source 예약을 연결했다. main의 Drain 수명·core 회계를 유지하며 선택21개 각1회 PASS, 독립 OCR 전체23파일 한정 수용·필수 수정0이다. 두 상주 조회 source 합은 memory56792B/Linux 타입56928B이며 전체 writer/progress admission·M4/M5는 미완료다.

2026-10-27 [C2 full Catalog short 예약](../evidence/module-independence-survey-20261026.md#c2-short-source-main): main의 원 isolation ledger에 고정 바이트 예약을 연결하고, 예약 시도부터 최종 decode/반환까지 원 Drain 표시를 유지한다. source별 선택14+7개 PASS·원 Found 시험 FAIL을 보존했고 OCR 전체19파일·독립 D1/D2 한정 수용·필수 수정0이며 전체 동시 admission·M4/M5 인수는 미완료다.

2026-10-27 [C2 공통 core 계수와 복구 반환 수명](../evidence/module-independence-survey-20261026.md#c2-shared-core): main의 fresh/세션/복구 공통 보유분에 원 core816B를 한 번 포함했다. native 타입의65563B 초과를 실제 cursor helper 수명으로 정리해65512B를 확인했고 source별8개 선택 사례의 최종 적용 결과를 검증했다. OCR 전체16파일 검토와 계약 문구1건 정정 뒤 독립 D1/D2 한정 수용·필수 수정0이다. 원 Capacity·fixture·runner FAIL과 전체 aggregate/M4/M5 미완료를 보존한다.

2026-10-27 [C2 Commit 단계별 source 비용](../evidence/module-independence-survey-20261026.md#c2-commit-phases-main): 실제 전환 carrier를 포함해 준비·대기·설치의 비용을 구분했다. 독립 검토에서 찾은 core816B 누락을 보완하고 source별 선택3+2회 PASS를 보존했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 Catalog 최대31940B·전체 동시 admission·M4/M5 미완료를 구별한다.

2026-10-27 [Row 거래 ledger](../evidence/module-independence-survey-20261026.md#row-transaction-ledger): Profile3 actor/Status/Job3의 다중 root·statement 상태와 현재 descriptor 잔여5행을 검사하는 pure 모듈을 구현했다. source별9 selector PASS(r1 unchanged4+r2 selected5), build 결과와 r1 fixture LSN 오류 FAIL을 보존한다. 독립 D1/D2 한정 수용·필수 수정0이며 actual graph/history/source admission/current 복구·A/B·M4/M5는 미완료다.

2026-10-27 [Root3 Aging descriptor](../evidence/module-independence-survey-20261026.md#row-aging): 별도 Profile3/Root3 codec과 전체 after64 Job census를 거친 후보 생산자를 구현했다. r1 production/test build·신규5 PASS·독립 D1/D2 한정 수용이며 actual opcode/history/admission/current 복구·A/B·M4/M5는 미완료다.

2026-10-27 [Base4 raw layout](../evidence/module-independence-survey-20261026.md#row-base4-layout): 별도 Profile3/Base4 layout와 공통 certificate/INIT grammar를 구현했다. r1 production/test build·신규8 PASS이며 독립 D1/D2를 한정 수용했다. full graph/Job census/current 복구/source admission·Row A/B·M4/M5는 미완료다.

2026-10-27 [Profile3·Row page 입구](../evidence/module-independence-survey-20261026.md#row-envelope-codecs): 별도 exact Profile3 codec과 Root/Job3 page wrapper4를 구현했다. r1 production/test build·신규8 PASS이며 독립 D1/D2를 한정 수용했다. 기존 bootstrap1/2와 current Profile2를 유지하며 actual Profile3/base4/scoped observer·Row A/B·M4/M5는 미완료다.

2026-10-27 [Job3 순수 cell·census](../evidence/module-independence-survey-20261026.md#row-job3-census): exact Job3/다중 root/actor·Terminal·동일 table·Root commitment/debt 검산을 구현했다. r1 production/test build·신규11 PASS이며 독립 D1/D2를 한정 수용했다. current Profile2와 기존 증거를 유지하며 pure page/Profile3 후속 공급은 [별도 envelope](../evidence/module-independence-survey-20261026.md#row-envelope-codecs)에 기록한다. actual graph/current Profile3/base4/Row A/B·M4/M5는 미완료다.

2026-10-27 [Root3 순수 codec](../evidence/module-independence-survey-20261026.md#row-root3-codec): exact version3/row_high와 초기 Free Job3 commitment의 raw cell을 구현했다. r1 첫2/r2 남은2 PASS·원 FAIL을 보존하며 독립 D1/D2 한정 수용이다. current Profile2는 유지한다. 당시 단위에 포함하지 않은 occupied Job3/census는 [후속 공급](../evidence/module-independence-survey-20261026.md#row-job3-census)에 기록한다. page wrapper의 후속 순수 공급은 [별도 envelope](../evidence/module-independence-survey-20261026.md#row-envelope-codecs)에 둔다. issuer·current Profile3/base4/actual Row 복구·M4/M5는 미완료다.

2026-10-27 [C3 Prepared 경계](../evidence/module-independence-survey-20261026.md#c3-prepared): 실제 정의·private field를 apply root로 이동하고 canonical disk 타입을 공급한다. 원 검산·효과/field 배치는 유지하며 build2/compiler4 PASS·독립 D1/D2 한정 수용이다. writer/전체 source·Row/복구·C3 전체·M4/M5는 미완료다.

2026-10-27 [C3 Catalog terminal 소유](../evidence/module-independence-survey-20261026.md#c3-terminal): 실제 stage/durable/whole-installed receipt와 조립을 apply private 경계로 이전했다. 원 physical WAL·core/pins·정리/Q 규칙을 유지하며 실제 Busy/Unchanged 분기를 추가했다. r3 runtime9/compiler4 PASS·r1/r2 FAIL을 보존하며 독립 D1/D2 한정 수용이다. C3 전체·Row/전체 회계·M4/M5는 미완료다.

2026-10-27 [C3 apply 설치 권한](../evidence/module-independence-survey-20261026.md#c3-install-grant): Catalog ordinary/Commit/retirement는 apply private 권한을 만들고 legacy4경로는 원 proof를 소비한다. 원 installer 검산·Pending flush 후 stamp를 유지한다. source별 runtime3+7·compiler5 PASS와 원 legacy fixture/test-build FAIL을 보존한다. Grant224B/legacy conversion696B이며 독립 D1/D2 한정 수용이다. terminal/Row·C3 전체·M4/M5는 미완료다.

2026-10-27 [C2 statement Catalog 조회](../evidence/module-independence-survey-20261026.md#c2-statement-reads): 실제 Full/Resident Scope를 원 statement에 연결하고 Found/Missing 반환 전 catalog read bit를 자동 등록했다. r4 runtime10/compiler3 PASS와 r1–r3 test-build FAIL을 보존한다. Full34020B/Resident17636B이며 전체 short floor·writer·Row/terminal/recovery·M4/M5는 미완료다.

2026-10-27 [C2 예약 뒤 원 검산](../evidence/module-independence-survey-20261026.md#c2-admitted-validation): pool/visibility guard 없는 health/origin과 원 core의 Owner/progress 검사 뒤 fixed source 안에서 pool/snapshot 전체 검산을 수행한다. r2 선택12 PASS·r1 test-build FAIL 보존. validation6958B는 Full33972B/Resident17588B에 포함되며 전체 entry/tail·writer phase·M4/M5는 미완료다.

2026-10-27 [C2 LOAD claim borrow](../evidence/module-independence-survey-20261026.md#c2-borrowed-load-claims): 실제 LOAD 검산의 두 claim 복사를 원 참조로 바꾸고 build2/선택4 PASS를 확인했다. pool guard3307B·compact floor14776B는 관측이고 전체 admission은 미연결이다.

2026-10-27 [C2 compact DPT](../evidence/module-independence-survey-20261026.md#c2-compact-dpt): 원 registry의 중복 identity 저장3072B를 제거하고 추가 비교 비용660B를 source 식에 연결했다. r3 build2/선택12 PASS와 r2 원 표현·진단2 PASS·기존 FAIL을 보존한다. shared19328B/Full33972B/Resident17588B이며 전체 floor·writer phase·M4/M5는 미완료다.

2026-10-27 [C2 query 종료 경계](../evidence/module-independence-survey-20261026.md#c2-query-tail-boundaries): actual 정상 결과 소비 block과 finish_error의 작은 정리를 분리했다. production/test build·선택5 PASS. 정상710B/오류1394B는 한 호출 관측이고 전체 floor/admission은 미연결이다.

2026-10-27 [C2 예약 밖 호출 phase](../evidence/module-independence-survey-20261026.md#c2-uncharged-call-phases): 실제 source 실패를 작은 consuming helper로 정리하고34입구·네 독립 guard·기존 wide tail을 cfg(test)로 관측했다. 선택4 PASS·원 test compile FAIL 보존. compact floor15057B는 미연결 상계이며 Full/Resident/shared와 전체 admission 미완료 상태를 유지한다.

2026-10-27 [C2 split 고정 charge](../evidence/module-independence-survey-20261026.md#c2-split-source-charges): source 계산을 exclusive port 생성으로 옮기고 실제3port와 복구 caller를 계수했다. ticket16B·Full33972B/Resident16928B/shared22400B, 선택16 PASS·원 compile FAIL2 보존. live/recovered peak63328/64704B이며 전체 entry/tail·writer phase·M4/M5는 미완료다.

2026-10-27 [C2 replay 작업 공간 대여](../evidence/module-independence-survey-20261026.md#c2-borrowed-replay-workspace): 원 zero tail3328B를 actual observer에 빌려 Full33980B로 줄였다. 선택9 PASS·정상/오류/unwind 복원·현재-build memory recovery를 확인하고 원 compile FAIL/경고·import-only build를 보존한다. 전체 entry/tail·writer phase·M4/M5는 미완료다.

2026-10-27 [C2 작은 진입·소비 정리](../evidence/module-independence-survey-20261026.md#c2-compact-call-boundary): 실제 health/source 오류·Catalog factory/finish를 줄이고 원 authority·정리 순서와 wide legacy 변환을 보존했다. production/test build·runtime25·compiler4 PASS, Full35619B/Resident16936B다. 전체 bound entry/tail floor와 writer/progress·M4/M5는 미완료다.

2026-10-27 [C2 readonly replay source](../evidence/module-independence-survey-20261026.md#c2-readonly-replay-source): 실제 allocation 검산 phase와 INIT context를 생산자 배열에서 분리해 Full35895B로 계산했다. source별 선택6+계수 보완1 PASS·MT03 Full Undo LOAD와 현재-build memory recovery를 확인했다. entry/tail·writer phase와 M4/M5는 미완료다.

2026-10-27 [C2 budget 행 borrow](../evidence/module-independence-survey-20261026.md#c2-borrowed-budget-rows): descriptor 검산/encoder의 실제 행 복사를 제거해 Full source36972B로 줄였다. 같은 source의5+5 선택 PASS·현재-build memory 복구와 harness identity 원 FAIL을 보존한다. Pending 관측 합64548B의988B 여유는 전체 admission 증거가 아니며 entry/tail·writer phase·M4/M5는 미완료다.

2026-10-27 [C2 Catalog CallScope](../evidence/module-independence-survey-20261026.md#c2-call-scope): 원 source/progress가 borrowed 결과의 실제 소비까지 유지되며 selected query를 이동했다. source별 runtime14+1+2 PASS·compiler4 PASS와 I/O fixture 원 FAIL을 보존한다. Full38076B·Resident17212B이고 전체 entry/tail·writer phase admission·M4/M5는 미완료다.

2026-10-27 [C2 실제 Drained 소비 경계](../evidence/module-independence-survey-20261026.md#c2-drained-handoff): authentic Drain 완료값이 원 pins/version을 보유한 뒤 content guard를 재구성한다. production/test build·선택10 PASS이며 정상 Drop의 fence-before-pins와 memory 복구를 확인했다. source phase 예약·entry/exit 합산·M4/M5는 미완료다.

2026-10-27 [C2 WAL wait source 관측](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source): cfg(test)의 실제 호출/반환 수명을 분리해 Pending5320B·Full 합65300B를 관측했다. source별 신규1 PASS이며 제품 charge/상계는 유지한다. 실제 phase admission·전체 entry/exit·M4/M5는 미완료다.

2026-10-27 [C2 Aging/Root source 계산](../evidence/module-independence-survey-20261026.md#c2-aging-source): 실제 호출 수명과 malformed 경로를 반영해 Full charge37724B를 검산했다. source 관련3 PASS와 후속 import-only compile을 구분한다. Full/Pending 합69100B·전체 admission 및 M4/M5 미완료다.

2026-10-27 [C2 source 진입 수명 수정](../evidence/module-independence-survey-20261026.md#c2-source-entry): 원 progress 표시를 source 예약 시도 전부터 유지하고 실패 출구에서 checked 정리한다. production/test build·선택5 PASS다. 전체 entry/return carrier·writer/progress 합산과 M4/M5는 미완료다.

2026-10-27 [C2 Pending image 수명 분리](../evidence/module-independence-survey-20261026.md#c2-pending-image): actual stage가 Prepared를 소비하여 Pending 준비 자료2024→568B, wait+Resident2 보수 합65096B를 확인했다. production/test build·관련12 PASS와 원 compile FAIL을 보존한다. stage 반환 비용 증가와 전체 writer/progress·Full 동시 인수 미완료를 구분한다.

2026-10-27 [C2 Commit phase 관측](../evidence/module-independence-survey-20261026.md#c2-commit-phases): cfg(test) 신규1 PASS와 원 compiler FAIL을 보존한다. Pending 세 값의 source 필요 하한2984B로 shared+Full 합66816B이므로 실제 보유 수명 축소가 선행한다. 제품 charge·admission·효과는 변경하지 않았으며 전체 writer/progress·M4/M5는 미완료다.

2026-10-27 [C2 최종 source 반환 수명](../evidence/module-independence-survey-20261026.md#c2-source-return): refs가0이 된 뒤에도 최종 decode/Result·source 반환까지 원 read_operation 표시를 유지한다. 신규4·cleanup 변경 영향3의 선택7개와 production/test build PASS다. 독립 D1/D2 한정 수용이며 진입 틈·writer/progress 전체 합산·M4/M5는 미완료다.

2026-10-27 [C2 상주 Catalog 조회](../evidence/module-independence-survey-20261026.md#c2-resident-read): 새 무 I/O 경로와 고정16860B class, 두 조회의 실제 동시 예약·합55976B를 선택9개로 확인했다. 비상주/정상 allocator 변경과 membership/typed 손상을 구분하며 원 compile·census FAIL을 보존한다. 독립 D1/D2 한정 수용이며 writer/progress 전체 합산·Row·M4/M5는 미완료다.

2026-10-27 [C2 CatalogFull source 예약](../evidence/module-independence-survey-20261026.md#c2-short-source): 원 isolation 원장에 전체 조회41576B를 선예약하고 마지막 decode/Result 뒤 반환한다. shared retained22256B와 합63832B이며 신규9·변경 영향8의 선택17 PASS, 원 test compile FAIL을 보존한다. 독립 D1/D2 한정 수용이며 writer/resident/progress·전체 aggregate·M4/M5는 미완료다.

2026-10-27 [C2 Commit Pending 보유 분리](../evidence/module-independence-survey-20261026.md#c2-pending-carrier): 동일 pin만 WAL 대기를 통과하고 Drain 뒤 설치 guard를 재구성한다. stage helper를 나눠 준비 로컬 수명을 끝냈으며 관련11개 선택 PASS와 MF03/MF07 설명 정정을 보존하고 최종 독립 D1/D2를 한정 수용했다. Pending 타입112B 감소와 Catalog Commit 포괄 상한 증가를 구별하며 aggregate admission·실제 Row 연결·M4/M5는 미완료다.

2026-10-27 [C2 거래·Aging 생성 수명 분리](../evidence/module-independence-survey-20261026.md#c2-transaction-source-phases): 실제 helper 경계를 나눠 Catalog lookup source 상한42850→41552B, live peak64074→63184B, recovered peak65450→64611B를 확인했다. source별 선택12+7회 PASS(16개 고유 selector)와 C09 설명 정정을 보존하며 최종 독립 D1/D2를 한정 수용했다. short aggregate admission·실제 Row/terminal/recovery·M4/M5는 미완료다.

2026-10-27 [C2 Catalog 후보 scratch](../evidence/module-independence-survey-20261026.md#c2-catalog-scratch): 기존 출력 loan을 재사용하여 중복 local buffer4588B를 제거했다. source별 선택12 PASS이며 원 MT03 설명 오류를 정정하고 별도 memory 복구 MF09를 검증했다. 전체 source peak는 다른 최대 경로 때문에 그대로다. short admission·실제 Row 연결·M4/M5는 미완료다.

2026-10-26 [C2 조회 수명/Commit Drain](../evidence/module-independence-survey-20261026.md#c2-read-operation-lifetime): 원 슬롯에서 페이지 접근 사이의 actual Catalog 조회를 추적하고 정리까지 Commit이 기다리도록 연결했다. Pending LOAD를 유지하고 source별 선택13 PASS와 원 fixture FAIL을 보존한다. short byte admission·Row 실제 연결·전체 M4/M5는 미완료다.

2026-10-26 [모듈 독립 개발 전환](../plans/module-independence-plan.md#execution): M0–M3 완료, M2/M3 독립 D2 수용·필수 수정0으로 소유 계약·정책·시험 JSON을 **ACCEPTED**로 반영했다. M4 C0의 [borrowed codec](../evidence/module-independence-survey-20261026.md#c0-borrowed-codec)은 선택6 PASS·독립 D1/D2 수용이며, 후속 [생산자 관측·대여 타입](../evidence/module-independence-survey-20261026.md#c0-producer-types)은 compiler4 자동 PASS·수명 거절1 독립 수동 수용과 원 판정기 FAIL을 보존한다. C1의 [실제 물리 WAL 본문 분리](../evidence/module-independence-survey-20261026.md#c1-physical-wal)는 production/test cfg·privacy 컴파일3 PASS이며 runtime 시험은 실행하지 않았다. [원 owner/port 이전](../evidence/module-independence-survey-20261026.md#c1-owner-ports)은 실제 DiskSession·세 port 정의와 consuming 전환을 공급하고 컴파일4·신규 메모리2 PASS 및 독립 D1/D2 한정 수용을 기록했다. [control/isolation 소유와4160B backing 분리](../evidence/module-independence-survey-20261026.md#c1-isolation-backing)는 compiler3·변경/신규 runtime4 PASS 및 독립 D1/D2 한정 수용이다. [epoch1600B 원 publication 소유](../evidence/module-independence-survey-20261026.md#c1-epoch-backing)는 compiler3·변경 runtime3 PASS·독립 D1/D2 한정 수용이다. [Catalog epoch 활성화](../evidence/module-independence-survey-20261026.md#c1-epoch-activation)는 실제 fresh/READY seeding·CREATE/Commit을 연결했다. source별 선택 검증과 복구 prefix 오류 수정·빈 Commit 지원 범위 정정·원 FAIL을 보존했다. [읽기 전용 거래/statement 수명](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)과512B 장기 credit을 연결하고 source별 compiler·선택runtime7을 확인했다. [C2 동기 Catalog 조회 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)는 visible/retired/Missing 본문을 실제 `storage::catalog::read`로 이전하고 원 adapter가 source/ref/load/cleanup을 소유한다. source별 compiler·runtime7 PASS와 원 FAIL을 보존하며 caller-retained statement factory는 미공급이다. 쓰기 거래·자동 read-set·short admission·실제 context/lane 생성·source/권한 adapter·성공하는 빈 Commit과 전체 복구 handoff 인수는 남아 있다. [A/B C0 제품 본문](../evidence/module-independence-survey-20261026.md#consumer-c0-start)을 같은 기준에서 병행하고 상대 source/fixture가 없는 사본에서 A10·B3개의 새 순수 시험을 통과했다. 실제 lane 진입·저장/조회/복구 인수와 구분한다. **전체 C0·C1–C5 공급과 A INSERT·B snapshot 조회/cursor의 M5 제품·fixture 인수는 미완료**이며 과거 source별 결과를 승격하지 않는다.

- [DEV3 원 Commit 공통 continuation](dev3-implementation.md#commit-continuation) — 공통 수명·변경 선택 검증·독립 D1 한정 수용; actual Commit·nonempty 복구 미완료.

2026-10-25 [DEV2 PF04 사용자·커널 CPU 관측](dev2-implementation.md#dev2-pf04-cpu-split-20261025): 기본6event를 유지하고8event profile을 추가했다. 선택6함수·실제2연결 완료, 원 PF04·DEV2 미완료.

2026-10-25 [DEV3 원 Catalog guard의 typed 조회](dev3-implementation.md#typed-catalog-read): 기존3pin 안의 physical record·빌림 수명 선택 검증 완료, 실제 nonempty Catalog·첫 행·DEV3/S5 미완료.

2026-10-25 [DEV2 PF04 C2 후보 기각](dev2-implementation.md#dev2-pf04-single-writer-20261025): 단일 writer 통계 후보의 선택 검증·40개 비교를 완료했으나5% 기준에 미달하여 제품에 반영하지 않았다. PF04·DEV2는 미완료다.

2026-10-25 [DEV3 첫 행의 leaf 후보 편집](dev3-implementation.md#leaf-edit): Insert/Replace/Remove 후보·수명 선택 검증 완료, 실제 행 거래·DEV3/S5 미완료.

- [DEV3 plan4 빈 거래 live·prefix](dev3-implementation.md#retention-live) — 필수75 source별 PASS; fresh 수명·flush·재적재 연결, 복구/DEV3/S5 미완료

- [DEV3 복구 이미지·제어 레코드](dev3-implementation.md#recovery-control-base) — 필수272 source별 PASS·원 실패 보존; 실제 복구 owner와 DEV3/S5 미완료

- [DEV3 빈 거래·정리 큐 순수 의미 처리](dev3-implementation.md#empty-retention-semantics) — 필수140 source별 PASS·최종 독립 D1 제한 수용; 원 실패·실제 연결 잔여 범위 구분

2026-10-24 [DEV3 원 WAL runtime/core 추출](dev3-implementation.md#private-runtime-core): private core와 원 소유권 이동·전달을 새7개 사례로 확인했다. 최종 독립 D1 제한 수용, recovered constructor·G6/DEV3/S5는 미완료다.

2026-10-24 [DEV4 P6 공급/API 수합](../evidence/dev4-supply-handoff-20261024.md): DEV3 clean owner의 제한 진전과 공개 disk 거래·live 큐·Catalog CREATE/CLR·첫 행 ACK의 미제공을 대조했다. 제품·시험 실행0, 실제 engine 연결·P5 모델·Q1은 미완료다.

2026-10-24 [DEV4 나머지 raw 문장 parser](dev4-implementation.md#remaining-parser): SELECT/DML/DDL 구문 전체를 기존 source·arena·WHERE·marker에 연결하고 새 semantic8·자원13함수를 선택 검증했다. 원 실패와21제품 변조 거절을 보존하고 최종 독립 D1을 제한 수용했으며 typed 문장 binding·P5/P6·engine·Q1은 미완료다.

2026-10-24 [DEV4 typed predicate](dev4-implementation.md#typed-predicate): 전체 Name/Type/marker 검산과 borrowed eager3VL을 구현하고 source별 필수 선택·18제품 변조 거절을 보존했다. 최종 독립 D1을 제한 수용했으며 실제 SQL 실행·Q1은 미완료다.

2026-10-24 [DEV4 predicate AST/parser](dev4-implementation.md#predicate-parser): 원 stack 실패를 수정하고 source별 필수15함수·제품 mutant6의 선택 증거를 보존했다. typed compile/eval·실제 SQL 실행·Q1은 미완료다.

2026-10-24 [DEV4 scalar 입력·typed 값 소유](dev4-implementation.md#scalar): checked 값 변환·SQL/native empty 경계·목적 타입 fixture와 retained quota를 선택 검증했다. 최종 독립 D1 제한 수용이며 predicate·실제 SQL 실행·Q1은 미완료다.

2026-10-24 [DEV3 Catalog typed Undo before](dev3-implementation.md#catalog-before): historical Table/Index 정의와 Missing/Value 복원 경계를 구현했다. 필수123 PASS·최종 독립 D1 제한 수용이며 실제 catalog/row/queue·전체 DEV3/S5는 미완료다.

2026-10-24 [DEV4 P2 parser·owned AST](dev4-implementation.md#parser): INSERT·거래 구문, 단일 문장·marker/node 한도와 원문/typed arena 예약을 구현·선택 검증했다. 값/predicate·나머지 문법·모델·실제 SQL 실행·Q1은 미완료다.

2026-10-24 [DEV4 splitter·원문 예약](dev4-implementation.md#splitter): streaming chunk/EOF·반환 source 수명·session/DB 공유 quota를 구현하고 새 경로를 선택 검증했다. parser/값·모델·실제 SQL 실행·Q1은 미완료다.

2026-10-24 [DEV4 독립 앞단](dev4-implementation.md): borrowed lexer·고정 이름·호출 제어와 독립 literal 판정기를 구현했다. source별 선택 시험·리뷰 교정은 증거에 보존하며 splitter/parser/값·실제 SQL 실행·Q1은 미완료다.

2026-10-24 [DEV3 Free Undo 초기 page](dev3-implementation.md#undo-free-page): authentic Free432·초기 slot1 page 표현을 구현하고7함수987case를 검증했다. 최종 독립 D1을 제한 수용했으며 mixed INIT/pool/복구·DEV3/S5 미완료.

2026-10-24 [DEV3 불변 Undo 역할](dev3-implementation.md#undo-bootstrap): plan3 설치/readback에 연결한 역할 선행을 구현했다. 선택 검증과 최종 독립 D1을 제한 수용했으며 mixed INIT/pool/복구·DEV3/S5 미완료를 유지한다.

2026-10-24 [DEV3 Status 상주 후보 선택](dev3-implementation.md#status-residency-pressure): 원 pool에서 자동 후보→flush/load와 같은 actor 계속을 연결했다. source별 신규33scenario·static3·checker7을 보존한다. native oracle 원 FAIL과 실제3PID의 source별 결과를 보존하며 최종 독립 D1을 제한 단위로 수용했다. S5 확대438은 작성·NOT_RUN, 일반 pool/live undo·전체 DEV3/S5는 미완료다.

2026-10-24 [DEV3 빈 거래의 닫힌 복구](dev3-implementation.md#empty-transaction-recovery): 실제 producer→flush/load→새 PID 복구/재진입 소비자를 연결하고 제한 독립 D1/D2를 수용했다. source별 memory104variant·native6history21PID와 원 실패를 보존한다. AI 운영 실험 소비자1/3 도달이며 효과 비교·일반 pool/live undo·전체 DEV3/S5는 미완료다.

2026-10-23 [DEV3 전체 대상 pin 집합](dev3-implementation.md#bounded-target-set): 기존 단일 SharedPool에서 정확1..64개 대상을 함께 확보하도록 확장했다. 신규15·직접 관련4·static2·compiler7·검사기3/변경1의 source별 PASS와 원 compile FAIL을 보존한다. 최종 독립 D1을 제한 단위로 수용했으며 일반 pool·live 거래·DEV3/S5는 미완료다.

2026-10-23 [DEV3 최초 Begin 준비](dev3-implementation.md#begin-preparation): Free 슬롯의 빈 Active 전이를 위한 WAL·페이지 producer/observer/materializer를 추가했다. 새 runtime12·static1·fmt/Clippy PASS와 원 fmt FAIL을 보존하며 최종 독립 D1을 순수 준비 단위로 수용했다. 실제 발급·stage/apply·일반 pool·DEV3/S5는 미완료다.

2026-10-23 [DEV3 현재 membership Status load](dev3-implementation.md#status-load): dirty allocator의 권한을 보존하며 실제 A/B home을 두 번째 frame에 재적재한다. source별 runtime24·native1·compiler3+baseline·자원/체커와 원 FAIL을 보존하며 최종 독립 D1을 이번 제한 load 단위로 수용했다. 일반 pool·live Status 변경·DEV3/S5는 미완료다.

2026-10-23 [DEV3 반복 Status 이력 복구](dev3-implementation.md#operational-recovery): 첫 extent의 주소별 canonical census·Pending 수리·REDO와 fresh producer/reentry를 연결했다. source별 신규65·관련3·native4/14worker·compiler3+control1의 선택 결과와 원 FAIL을 보존한다. OCR delegate 최종 독립 D1을 제한 복구 단위로 수용했으며 P3·일반 pool·DEV3/S5는 미완료다.

2026-10-23 [DEV3 진행 현황 후속 리뷰](../evidence/dev3-progress-review-20261023.md): [allocator 동일 주소 reload](dev3-implementation.md#allocator-residency)와 [영속화 후 snapshot 오류 수정](../evidence/dev3-code-review-20261023.md#fix-followup)까지 요약·단계 JSON·색인에 연결했다. 기존 source별 선택 검증을 보존하며 새 제품 실행·일반 pool·DEV3/S5 완료를 주장하지 않는다.

2026-10-23 [DEV3 고정 등록 코어·세대 pin](dev3-implementation.md#registration-core): 단일 등록 lookup·Copy 요청·pin 세대 검산의 [선택 증거](../evidence/dev3-registration-core-20261023.md)를 보존했다. 신규11·관련10·native1·compiler2+control1 및 source별 자원 회계·독립 D1 수용. 일반 pool·DEV3/S5는 미완료다.

2026-10-23 [DEV3 현재 genesis의 실제 닫힌 복구](dev3-implementation.md#recovery-effects): 고정 두 슬롯 provisioning·source 안정화·historical Pending 수리·예약 한 번 인계·page별 REDO를 연결했다. [source별 검증](../evidence/dev3-recovery-effects-20261023.md)은 필수393의 완결 job386·실패 job 내 완료 단언7과 native31/95worker·compiler9+control1·checker30/변경1을 구별한다. 제한 단위 최종 독립 D1을 수용했으며 일반 복구/READY·전체 DEV3/S5는 미완료다.

2026-10-22 [DEV3 현행 WAL prefix 분석](dev3-implementation.md#recovery-prefix): 실제 reserve/cancel·Status INIT의 canonical RAM 상태와 잔여 의무를 닫힌 owner에 재구성했다. [source별 검증](../evidence/dev3-recovery-prefix-20261022.md)은 새 unit231의 완료 단언(완결 job223·실패 job 내8), native31·compiler8+control1·checker15·변경 수신6을 구별한다. 제한 단위의 최종 독립 D1을 수용했으며 실제 Pending 수리·물리 REDO·전체 DEV3/S5는 남아 있다.

2026-10-22 [DEV3 첫 복구 입력 선택](dev3-implementation.md#recovery-select): 초기 control·exclusive lease·header-only WAL의 [닫힌 선택](../contracts/dev3-recovery-select.md)을 구현하고 [source별 선택 인수](../evidence/dev3-recovery-select-20261022.md)와 독립 D1을 수용했다. BOOT_RESERVE·nonempty 복원·일반 recovery/READY·S5는 미완료다. 아래 종료·인계는 당시 기록으로 보존한다.

2026-10-22 [DEV3 작은 단위 종료·인계](dev3-implementation.md#paused-after-status-home-flush): [Status home flush](dev3-implementation.md#status-home-flush)의 제한 인수와 commit을 마쳤다. 복구 진입은 사전 열람 상태로 보류하며 새 하위 작업을 시작하지 않는다. 전체 DEV3·최종 S5는 미완료다.

2026-10-22 [DEV3 폐쇄 두-frame Status INIT](dev3-implementation.md#status-init): 실제 reserve→두 RAM image 공개→동일 WAL requirement의 durable 확인과 두-pin reader를 연결했다. 필수93·shared1·compiler13·native3의 source별 결과와 원 FAIL을 보존한다. 제한 단위의 최종 독립 D1을 수용했으며 Status home flush·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 불변 Status bootstrap](dev3-implementation.md#status-bootstrap): plan2 영속 역할과 typed borrow·old adapter 거절을 구현했다. 새 필수32·변경 parser1·native1·compiler semantic3와 별도 control1을 검증했다. 제한 단위의 최종 독립 검토를 수용했으며 INIT·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 allocator 한 페이지 flush](dev3-implementation.md#allocator-flush): 실제 capture·WAL force·DWB/home·whole retirement·조건부 RAM finalize를 연결했다. 필수76의 source별 범위·native3·compiler12·WAL30과 자원 전제를 검증했으며 원 FAIL/관측 불완전을 보존한다. 제한 단위의 독립 검토를 수용했으며 INIT·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 첫 dirty frame·stage 소비자](dev3-implementation.md#dirty-stage): 실제 allocator1의 pin/content·DPT·first recLSN과 guarded reserve/cancel을 연결했다. mandatory53의 source별 범위·compiler18·native3을 검증했고 INIT·flush·일반 recovery와 전체 DEV3는 미완료다.

2026-10-22 [DEV3 적용 admission·version gate](dev3-implementation.md#concurrency-admission): 3.10.1.a.1의 실제34-owner·gate와 신규 runtime7그룹/45사례·compiler12를 연결했다. 최종 단위 판정은 인수 기록을 따르며 dirty/Commit 통합과 전체 DEV3는 미완료다.

2026-10-22 [DEV3 예약·취소 잔여 인수](dev3-implementation.md#allocation-reservation-accepted): native13DB·compiler baseline+6·관련 WAL4 및64KiB stack 경로를 검증하고 원래 실패와 source별 결과를 보존했다. 제한 단위의 독립 판정은 인수 기록을 따르며 전체 DEV3/S5는 미완료다.

2026-10-22 [DEV3 분류별 현재 상태](dev3-implementation.md#dev3-work-breakdown)와 [잔여 작업 계획](../plans/dev3-implementation-plan.md#remaining-work)을 연결했다. 예약·취소의 남은 검증·64KiB 증명 및 [작업 크기·우선순위·병렬 배치](../plans/dev3-implementation-plan.md#work-organization)를 명시했다. 분류 수로 진행률을 계산하지 않으며 새 제품 실행·완료 승격은 없다.

2026-10-21 [DEV3 페이지 예약·취소 부분 구현 및 중단](dev3-implementation.md#allocation-reservation-partial): 메모리 포트의 새 경로와 검토 수정만 제한 검증했다. native/compiler·변경 WAL 관련4시험·전체 단위 인수는 남겨 두고 사용자 요청으로 중단한다. [하부 작업 16개 분류 초안](dev3-implementation.md#dev3-work-breakdown)으로 진행·잔여 범위를 구분한다.

2026-10-21 [DEV3 최초 doublewrite 설치](dev3-implementation.md#doublewrite-install-results): XDI1·고정 images 공간·Empty DWB 선택을 구현했다. 기존 동일 source의 runtime10·native3·compiler9·checker31 및 Clippy/fmt 결과를 보존했고 수정 없는 코드는 재시험하지 않는다. 마지막 하위 회귀를 사용자 지시로 중단한 통합 원본은 FAIL이며 dirty/home/recovery·전체 DEV3는 미완료다.

2026-10-21 [DEV3 최초 저장 세션](dev3-implementation.md#storage-session-results): extent·FILEMAP 설치와 reader/WAL 공통 fence를 구현했다. 같은 source의 runtime12·native3·compiler8·leaf59·하위28·현재-build DEV2 복구18check와 Clippy/fmt PASS이며 dirty buffer·home write·일반 복구·전체 DEV3는 미완료다.

2026-10-21 [DEV3 RollbackStart WAL·page 생산](dev3-implementation.md#rollback-producer-results): typed before 상태에서 caller frame과 상태 page를 구성했다. 같은 source의 semantic22·WAL13·leaf54·하위 회귀28·DEV2 복구18check와 Clippy/fmt PASS이며 tag2 append·live 적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 RollbackStart page 구성](dev3-implementation.md#rollback-materialization-results): 검증된 WAL과 before page로 caller scratch의 after page를 구성했다. 같은 source의 관련17시험·leaf54·하위 회귀28·DEV2 복구18check와 Clippy/fmt PASS이며 live dirty frame·WAL 적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 fresh genesis WAL](dev3-implementation.md#genesis-wal-results): 첫 epoch·WRITER 설치와 한 slot의 physical write/sync를 구현했다. 같은 source의 runtime10·native3·compiler4·유한 모델·leaf54·하위 회귀28·DEV2 복구18check가 PASS이며 일반 recovery·dirty page·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 selected clean page buffer](dev3-implementation.md#read-buffer-results): 실제 reader 위에 bounded frame·pin·clock 교체를 구현했다. 같은 source의 leaf49·하위 회귀28·DEV2 복구18check(native99셀)와 수명 compiler·Clippy/fmt PASS이며 dirty buffer·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 RollbackStart 결합 검산](dev3-implementation.md#rollback-start-results): 거래 descriptor·상태 page·WAL의 전체 rollback 시작 전이를 검산했다. 같은 source의 leaf44·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 실제 append/적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 현재 growth intent 검산](dev3-implementation.md#ownership-intent-results): 실제 Profile과 inventory를 결합하고 owner별 현재 intent 중복을 고정 공간으로 거절한다. 같은 source의 leaf43·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 실제 예약 복원·DEV3 인수는 미완료다.

2026-10-21 [DEV3 ownership 관측](dev3-implementation.md#ownership-observation-results): 고정 공간으로 inventory bytes·owner별 합계·물리 충돌을 검증했다. 같은 소스의 leaf43·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 intent 유일성·실제 예약 복원·DEV3 인수는 미완료다.

2026-10-21 [DEV3 상태 home·obligation descriptor](dev3-implementation.md#obligation-descriptor-results): incoming count와 복구 budget의 고정 표현을 구현·독립 검토했다. 같은 통합 소스의 leaf42·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 실제 예약 복원·DEV3 인수는 미완료다.

2026-10-21 [DEV3 재개·선택 page 읽기](dev3-implementation.md#page-reader-results): selector와 열린 handle metadata를 연결하여 실제 선택 page 읽기를 검증했다. 같은 보존 소스의 leaf·하위 회귀·DEV2 복구99셀 PASS이며 전체 DEV3 인수는 계속 미완료다.

2026-10-20 [DEV3 첫 leaf](dev3-implementation.md#validation): 내부 page envelope codec과 D3-M02-P01 부분 모델을 구현·독립 검토했다. page7·모델 정상4/결함8·DEV0/1/2 회귀와 같은 소스의 DEV2 복구99셀 PASS. 실제 디스크 경로·전체 모델·S1 인수는 미완료다.

2026-10-20 [DEV2 journal binary 대응](dev2-implementation.md#dev2-journal-binary-mapping-20261020): 설치 커널의 build ID와 기계어 흐름으로 두 대기 위치를 metadata·commit record 버퍼에 대응했다. 추가 측정 없이 코드 위치만 구분했으며 PF04·DEV2 인수는 미완료다.

2026-10-20 [DEV2 journal 대기 진단](dev2-implementation.md#dev2-journal-diagnostic-20261020): 1회 정답 PASS와 측정 512개 fsync의 실제 journal 대기 TID·단계 연결을 확인했다. 추적 영향·환경 확인 조건 FAIL과 기존 PF04 실패를 유지하며 DEV2/B2/D0 최종 인수는 미완료다.

2026-10-20 [DEV2 PF04 가설 검토](dev2-implementation.md#dev2-pf04-hypothesis-review-20261020): 새 제품 실행 없이 기존 추적과 커널 소스를 독립 대조했다. 설정 변경이나 추가 후보의 효과를 확정하지 않았으며, PF04와 DEV2/B2/D0 최종 인수는 미완료다.

2026-10-20 [DEV2 fsync 진단](dev2-implementation.md#dev2-fsync-diagnostic-20261020): Off/On/On/Off 4개 실행의 정답과 2,048개 측정 fsync 연결을 확인했다. 이 추적 결과는 PF04 인수 결과를 대체하지 않으며, 원래 네 campaign의 비용 한도 FAIL과 DEV2 미완료를 유지한다.

2026-10-20 [DEV2 성능 후속 실행](dev2-implementation.md#dev2-performance-followup-20261020): 고정 소스의 9개 campaign·180개 run 정답 PASS. PF12 국소 개선·PF13 재현 기능을 개별 검증 완료했으며, PF04 계측 비용 한도와 DEV2/B2/D0 최종 인수가 남아 있다.

2026-10-19 [DEV2 계측 수정](dev2-implementation.md#dev2-instrumentation-fix-20261019): CPU 관측과 상세 진단 비용을 수정하고 같은 동결 source에서 ext4 복구 99셀·하위 회귀·필수 성능 시험 36개를 통과했다. 최종 로컬 성능 인수는 INCOMPLETE이며 전체 DEV2 인수는 미완료다.

2026-10-19 [DEV2 남은 로컬 시험](dev2-implementation.md#dev2-tests-20261019)을 완료했다. 영속 99셀·9개 30분 target·P0 전수·하위 회귀와 K0 후보의 확인/재현 결과를 새 receipt에 연결했다. 일부 계측 비용 한도 FAIL로 전체 인수는 INCOMPLETE다.

2026-10-18 · **이관한 실제 코드 / XADB 실행 결과는 아래 증거 참조**

| 범위 | 현재 상태 | 상세 |
| --- | --- | --- |
| DEV0 | controller·fixture worker·receipt·artifact·cleanup 구현 | [DEV0](dev0-implementation.md) |
| DEV1 | 최소 ID·값·오류·codec·quota·clock/scheduler·Linux 파일 효과 구현 | [DEV1](dev1-implementation.md) |
| DEV2 P0 | 독립 history 모델과 known-bad 거절 구현 | [DEV2](dev2-implementation.md) |
| DEV2 K0 일부 | volatile 행/catalog·snapshot·ID·quota·frame·native worker 구현 | [DEV2](dev2-implementation.md) |
| 외부 streaming controller | `xadbtest::native`의 부정 시험·실제 worker 연결 로컬 검증 | [DEV2](dev2-implementation.md) |
| 순차 catalog oracle·recipe | 독립 기대값·실제 이력·저장 recipe 실행/재검증 로컬 검증 | [DEV2](dev2-implementation.md) |
| K0 성능 진단·audit/replay | Off/Always/Detailed와 독립 정답·통계·원시 bundle 검증 구현; 최초 p99 비용 한도 FAIL, PF 인수 미완료 | [DEV2](dev2-implementation.md) |
| COMM 핵심 | frame·Linux local socket·quota·deadline·취소/종료·focused runner 구현, C1 인수 미완료 | [COMM](comm-implementation.md) |
| 영속 transaction/WAL/recovery | 구현·정적 통합 완료, 최종 인수 대기 | [DEV2 K1–K5](dev2-implementation.md) |
| DEV3 저장 표현·선택 page 읽기·상태 모델 | 고정 표현·bounded 읽기 leaf 검증, 실제 쓰기·복구·S1 인수 미완료 | [DEV3](dev3-implementation.md) |
| SQL 앞단 | [raw 문장·typed predicate 제한 구현](dev4-implementation.md) | DEV4 독립 선행; P4 제한 D1 수용·실제 SQL/Q1 미완료 |
| server/client·replication | 미구현 | DEV5 이후 |

[기계 판독 상태](implementation-status.json) · [새 실행 증거](../evidence/README.md) · [이관 기록](../history/project-migration.md)

기계 판독 상태의 `evidence_refs`는 현재 소스 지문과 일치하는 검증 주장만 담는다. `historical_evidence_refs`는 원래 소스·명령·check 결과를 그대로 보존한 이전 소스의 증거이며, 현재 소스 검증을 대신하지 않는다. `PREVIOUS_SOURCE_VERIFIED`는 이 구분을 표시한다. 영속 커널의 `IMPLEMENTED_VALIDATION_PENDING`은 코드 구현과 부분 개발 시험이 있어도 최종 인수가 남았다는 상태다. 계획표의 기존 로컬 개발 선행권한이나 DEV2 완료·제품·릴리스 인수 조건을 변경하지 않는다.

P0/K0 시험 성공은 B2/D0나 DEV2 전체 완료를 의미하지 않는다. e3 동결 소스의 bounded concurrent 전수·영속 복구·11개 V3와 PF12/PF13 결과는 위 후속 기록에 연결한다. PF04 계측 비용 한도와 최종 인수는 미완료이며 과거 source snapshot의 PASS를 전체 현재 소스의 PASS로 취급하지 않는다.

<!-- catalogue -->

- [DEV3 구현·검증 현황](dev3-implementation.md)

- [COMM 구현 기록](comm-implementation.md)

- [DEV0 — Rust 빌드와 XADBTest 최소 구현](dev0-implementation.md)
- [DEV1 — 공통 의미와 Linux effect 기반](dev1-implementation.md)
- [DEV2 구현 기록](dev2-implementation.md)
- [implementation-status.json](implementation-status.json)

K0-004 순차 catalog 판정·recipe 실행/재검증과 K0-005 병렬 구현·검증의 현재 계약은 [DEV2 기록](dev2-implementation.md)을 따른다.

K0-005 통합 후 새 r3 실행에서 DEV0 8개·DEV1 8개·DEV2 leaf 28개 check가 PASS했다. 성능 진단 r1은 고정 시험35개와60개 release 실행·원시 재검증을 확인한다. 최초 두 비교의 sample-p99 guardrail FAIL은 유지하며 PF 인수를 완료로 표시하지 않는다. [현재 증거](../evidence/README.md)는 실행 전후 같은 source hash에 적용한다. 과거 r1·r2와 중단된 실행을 현재 코드의 PASS로 재사용하지 않는다.

COMM 반영 당시 commit 소스의 DEV0/DEV1/DEV2 회귀 참조는 [증거 색인](../evidence/README.md)의 COMM 보고서로 연결했다. 그 실행 전후 source 지문은 당시 문서 commit 대상의 지문과 일치했다. 이 보고서와 앞 문단의 r3·성능 보고서는 각각 당시 source의 기록으로 보존하며, 현재 DEV2 변경의 검증으로 재사용하지 않는다.

## XADB-EVIDENCE-SCOPE-001 — 현재 검증과 이전 소스 증거

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 근거·범위 | 사용자 문서 재검사·수정·commit 요청. 기준 `8550f55b2829a3792614e2853d19060bdc1d231c`의 push 검사에서 영속 상태 거절 1건과 이전 DEV0/DEV1/DEV2 source 지문 불일치 3건 재현 |
| 선택·대안 | 구현 상태에 최종 검증 대기를 명시하고 현재/이전 소스의 증거 참조를 분리. 이전 보고서의 hash를 현재 값으로 고치거나 구현 상태를 미구현으로 되돌리는 대안은 실제 상태·증거를 왜곡하므로 제외 |
| 유지 조건 | 현재 검증 주장은 현재 source 지문 일치가 필요. 이전 증거도 원래 실행 전후 hash·PASS·필수 check·exit code 검사를 유지. 기존 P0/K0 보고서로 내구성·DEV2 완료·제품/릴리스를 승격하지 않음 |
| 구현·독립 검토·통합 | `Complete DEV2` 작업의 `/root/dev2_completion_review`가 검사기·반례·상태 분리를 구현. 별도 실행인 이 문서 수정 작업의 `/root`(chat `01a14f79-b353-7c33-819a-147676267378`)가 계약·실제 diff·원래 실패와 반례 결과를 독립 검토하고 채택. 추가 README 문구는 같은 증거 범위에 맞춘 D0 정정 |
| 소유·병합 | 위 구현의 미커밋 문서 수정 4개를 이어받아 문서·검사기만 통합. 제품 코드·장시간 DEV2 검증은 기존 작업 소유. 기존 파일과 색인을 사용하며 원본 실행 보고서의 byte·ID·경로·hash는 유지 |
| 검증 | 사용자 요청에 따라 현재 작업 트리의 문서 검사 58개 PASS, `python3 -B -m unittest discover -s devdoc -p 'test_*.py' -v` 33개 PASS. 이전 증거를 현재 PASS로 바꾸기, 내부 hash 불일치, 실패/누락 check, 참조 중복, 미선언 보고서와 완료 주장 거절을 확인. 제품 시험 결과가 아님 |
| 대체·재검토 | 문서 검사기의 K0 미구현 전용 상태 제한과 모든 참조를 현재 소스로 취급하던 방식을 대체. 새 영속 보고서 profile·현재 source 검증·단계 완료 상태를 도입할 때 재검토 |

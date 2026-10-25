# XADB 조사·실행 증거

- [DEV2 PF04 P1 실제 PMU 수집 후속](dev2-pf04-pmu-20261025.md) · [source별 선택 검증](dev2-pf04-pmu-20261025.json) — 명시적 CAP_PERFMON 수집·정리 검증 완료, PF04 FAIL 유지.

- [DEV2 PF04 P1 측정 구간 관측 도구](dev2-pf04-window-20261025.md) · [source별 선택 검증](dev2-pf04-window-20261025.json) — 당시 일반 사용자 PMU UNAVAILABLE 증거 보존, PF04 FAIL 유지.

- [DEV2 PF04 원인 분석과 해결 후보](dev2-pf04-analysis-20261025.md) · [재분석·source 기록](dev2-pf04-analysis-20261025.json): 기존 K0 80·영속60 실행 자료의 한정 재분석, 해결 후보 PROPOSED·신규 시험0·원 실패 보존.

- [DEV3 복구 작업 중간 저장·자료 정리](dev3-recovery-paused-20261025.md) · [source별 상태 색인](dev3-recovery-paused-20261025.json): 사용자 요청 중지, WIP·FAIL/HOLD·NOT_RUN 보존.

- [COMM 시험 산출물 정리](comm-artifact-cleanup-20261025.md): 전용 Cargo cache35개와 검산된 임시 사본을 정리했다. 원 증거·실행물 보존, 저장소 약3.76GiB·tmpfs 약1.62GiB 절감, 시험 실행0.

- [DEV4 P5 전체 입력 조립 준비](dev4-lifecycle-assembly-inputs-20261025.md) — 독립 후보1658개 정적 한정 검토; 미검토 rich304·전체 D2·제품·시험 NOT_RUN.

- [DEV3 plan4 빈 거래 live·prefix](dev3-retention-live-20261025.md) · [source별 색인](dev3-retention-live-20261025.json) — 신규75 PASS·완료 반복0·원 실패 보존; 최종 독립 D1 제한 수용

- [DEV4 P5 원시 번호·selector](dev4-lifecycle-raw-selection-20261025.md) — 신규1951입력·3484호출/compile/Clippy/fmt PASS; 정적 집계 원 실패 보존·최종 독립 D1 제한 수용.

- [DEV3 복구 이미지·제어 레코드](dev3-recovery-control-base-20261024.md) · [source별 색인](dev3-recovery-control-base-20261024.json) — 신규 필수272 PASS·원 실패 보존·최종 독립 D1 제한 수용; live/복구·S5 미완료

- [DEV4 P5 상태·입력 codec](dev4-lifecycle-codec-20261024.md) — source별 선택 검증·원 compile/실행/fmt 실패 보존; 모델/FE14 미완료.

- [DEV4 P5 수명 모델 독립 입력](dev4-lifecycle-inputs-20261024.md): 12 trace·51 action 논리 기대값과 state4/input7 byte vector의 정적 한정 검토 수용. 전체 D2·모델·시험 NOT_RUN.

- [DEV3 빈 거래·정리 큐 순수 의미 처리](dev3-empty-retention-semantics-20261024.md) · [source별 색인](dev3-empty-retention-semantics-20261024.json) — 필수140 source별 PASS, 원 실패 보존, 최종 독립 D1 제한 수용; live/복구·S5 미완료

- [COMM slow-peer 실패 수정](comm-failure-fix-20261024.md): Unix prefix·TCP 시험 협상·실제 TX 정체 판정과 실패 원자료를 보강. source별 두 실패 조합 각각5쌍 PASS, 관련 단위10개 PASS, 최종 독립 D1/D2 수용. 원FAIL·C1 INCOMPLETE 보존.

- [DEV3 원 WAL runtime/core 추출](dev3-runtime-core-20261024.md) · [source별 색인](dev3-runtime-core-20261024.json): 새7개 선택 PASS, 원 compile 실패·증거 보존, 최종 독립 D1 제한 수용.

- [DEV4 P6 실제 공급·연결 준비](dev4-supply-handoff-20261024.md) · [관측·독립 검토 색인](dev4-supply-handoff-20261024.json): 시점별 DEV3 source·공개 API·P1–P4 증거 수합. 독립 사실 검토 수용이며 새 제품 시험·engine 공급 인수 아님.

- [DEV4 나머지 raw 문장 parser](dev4-remaining-parser-20261024.md) · [source별 실행 색인](dev4-remaining-parser-20261024.json): 정상 semantic8·자원13 선택 검증, 원 M21 compile·fmt FAIL 보존과21제품 변조 거절·최종 독립 D1 제한 수용.

- [DEV3 plan4 초기 page의 원 공유 버퍼 소비](dev3-plan4-clean-owner-20261024.md) · [source 색인](dev3-plan4-clean-owner-20261024.json): 필수78의 source별 결과·원 실패·HOLD와 최종 D1 제한 수용.

- [DEV4 typed predicate](dev4-typed-predicate-20261024.md): 전체 타입·3값 평가, 원 fixture/compile FAIL·좁은 재검증과 [source별 실행 색인](dev4-typed-predicate-20261024.json).

- [DEV3 plan4 정리 큐·빈 Catalog 최초 설치](dev3-plan4-genesis-20261024.md) · [source 색인](dev3-plan4-genesis-20261024.json): 필수304·compiler9·checker15, 원 실패·source별 결과와 S5 미실행. 최종 D1 제한 수용.


- [DEV4 predicate AST/parser](dev4-predicate-20261024.md): D2 입력 동결·원 stack/Clippy FAIL·선택 수정과 [source별 실행 색인](dev4-predicate-20261024.json).

- [DEV4 scalar 입력·typed 값 소유](dev4-scalar-20261024.md): 독립 D2·필수23함수·11범주/12제품 변조와 [source별 선택 증거](dev4-scalar-20261024.json).

- [DEV3 Catalog typed Undo before](dev3-catalog-before-20261024.md) · [source 색인](dev3-catalog-before-20261024.json): 필수123 선택 결과·원 실패·S5 미실행과 제한 선행 범위.

- [DEV4 owned AST·INSERT/거래 구문](dev4-parser-20261024.md): 독립 D2 동결·typed arena/shared quota·[source별 선택 실행](dev4-parser-20261024.json).

- [DEV3 혼합 Status·Free Undo owner](dev3-mixed-system-owner-20261024.md) · [source 색인](dev3-mixed-system-owner-20261024.json): 원 pool·canonical mixed 복구의 선택 결과, 원 FAIL과 native 이어가기·잔여 인수.

- [DEV4 streaming 문장 경계·원문 소유](dev4-splitter-20261024.md): 입력 동결·공유 quota·[선택 실행 색인](dev4-splitter-20261024.json).

- [DEV4 독립 앞단 개발·선택 검증](dev4-frontend-20261024.md): lexer/이름 입력 동결·병렬 구현·[source별 실행 색인](dev4-frontend-20261024.json)과 잔여 의무.

- [DEV4 독립 앞단 — Oracle·로컬 DBMS 조사와 계약 검토](dev4-preparation-research-20261024.md): 공식19c와 사용자 제공 OracleManual·PostgreSQL/SQLite/MariaDB/CUBRID의 실제 revision/hash·차이 해결·D2 검토. 후속 P2 발견·진단 효과 교정과 D4-PREP-002 독립 수용을 별도 보존. 제품/Oracle 실행 NOT_RUN.

2026-10-24 [DEV3 혼합 INIT 준비](dev3-mixed-system-init-20261024.md) · [source 색인](dev3-mixed-system-init-20261024.json): 독립 raw285·source별246case·compiler6+baseline·checker6 PASS, 원 실패/중복2 범위 일탈 보존·최종 D1 제한 수용; 실제 mixed 소비자·DEV3/S5 미완료.

2026-10-24 [DEV3 Free Undo 초기 page](dev3-undo-free-page-20261024.md) · [source 색인](dev3-undo-free-page-20261024.json): 독립 raw6·pure codec/page 선행의7함수987case PASS·최종 독립 D1 제한 수용. 실제 INIT·mixed pool·현재-build 복구·전체 DEV3/S5 미완료.

2026-10-24 [DEV3 Undo bootstrap 역할](dev3-undo-bootstrap-20261024.md) · [source별 색인](dev3-undo-bootstrap-20261024.json): 신규33assertion·source별 원 실패를 보존한다. native1·compiler4+baseline·checker7 PASS, 최종 독립 D1 제한 수용, S5 추가78과 mixed 소비자 미완료.

2026-10-24 [DEV3 Status 상주 후보 선택](dev3-status-pressure-20261024.md) · [source별 색인](dev3-status-pressure-20261024.json): 신규11함수33scenario·static3·checker7을 선택 검증했다. native 원 oracle FAIL과 저장 결과의 수정 검산·같은 ELF의 새 재진입을 구분하며 실제3PID 인계를 확인했다. 최종 독립 D1을 제한 수용했고 S5 438은 작성·NOT_RUN이다.

[DEV3 빈 거래 통합 작업](dev3-empty-transaction-20261023.md) · [source별 색인](dev3-empty-transaction-20261023.json): 순수 AbortEnd 준비·원 Status owner 인계·실제 빈 거래 WAL/RAM·이력 기반 flush·최신 live load·새 PID 복구/재진입의 source별 결과와 원 FAIL을 보존한다. 제한 D1 수용·소비자1/3 도달, 효율 비교·전체 DEV3/S5 미완료.

[DEV3 resident Status source·receipt 색인](dev3-resident-status-20261023.json)은 정확 선택·원 실패·미실행 상태를 보존한다.

2026-10-23 [DEV3 여러 Status의 실제 상주 수명](dev3-resident-status-20261023.md): 기존 SharedPool에서3frame/4주소 INIT·read·flush·퇴출·load와 별도 현재-build 복구/재진입을 연결했다. 신규31·직접 관련4·static2·compiler9·checker8·native1의 source별 결과와 원 FAIL을 보존한다. 최종 독립 D1을 제한 단위로 수용했으며 일반 pool·live 거래·전체 DEV3/S5는 미완료다.

[DEV3 전체 대상 source·receipt 색인](dev3-target-set-20261023.json)은 정확 선택·원 실패·미실행 상태를 보존한다.

2026-10-23 [DEV3 전체 대상 pin 집합](dev3-target-set-20261023.md): 기존 단일 SharedPool에서 정확1..64개 대상을 함께 확보하도록 확장했다. 신규15·직접 관련4·static2·compiler7·검사기3/변경1의 source별 PASS와 원 compile FAIL을 보존한다. 최종 독립 D1을 제한 단위로 수용했으며 일반 pool·live 거래·DEV3/S5는 미완료다.

[DEV3 Begin source·receipt 색인](dev3-status-begin-20261023.json)은 frozen source와 정확 선택·원 실패·미실행 상태를 보존한다.

2026-10-23 [DEV3 최초 Begin 준비](dev3-status-begin-20261023.md): Free 슬롯의 빈 Active 전이를 위한 WAL·페이지 producer/observer/materializer를 추가했다. 새 runtime12·static1·fmt/Clippy PASS와 원 fmt FAIL을 보존하며 최종 독립 D1을 순수 준비 단위로 수용했다. 실제 발급·stage/apply·일반 pool·DEV3/S5는 미완료다.

2026-10-23 [DEV3 현재 membership Status load](dev3-status-load-20261023.md) · [source별 실행 색인](dev3-status-load-20261023.json): dirty allocator의 권한을 보존하며 실제 A/B home을 두 번째 frame에 재적재한다. source별 runtime24·native1·compiler3+baseline·자원/체커와 원 FAIL을 보존하며 최종 독립 D1을 이번 제한 load 단위로 수용했다. 일반 pool·live Status 변경·DEV3/S5는 미완료다.

2026-10-23 [DEV3 반복 Status 이력 복구](dev3-operational-recovery-20261023.md) · [source별 실행 색인](dev3-operational-recovery-20261023.json): 첫 extent의 주소별 canonical census·Pending 수리·REDO와 fresh producer/reentry를 연결했다. source별 신규65·관련3·native4/14worker·compiler3+control1의 선택 결과와 원 FAIL을 보존한다. OCR delegate 최종 독립 D1을 제한 복구 단위로 수용했으며 P3·일반 pool·DEV3/S5는 미완료다.

2026-10-23 [DEV3 반복 Status INIT owner](dev3-operational-status-20261023.md) · [source별 실행 색인](dev3-operational-status-20261023.json): 원 flush bound의 consuming 전환·clean Status 퇴출·반복 INIT을 공급했다. source별 static6·신규 runtime14·관련1·native1과 원 compile FAIL을 보존하며 OCR delegate 독립 D1을 live owner 한정 수용했다. 반복 sequence 복구·P3·DEV3/S5는 미완료다.

2026-10-23 [DEV3 일반 page 등록 코어 P1](dev3-operational-registration-20261023.md) · [source 색인](dev3-operational-registration-20261023.json): 기존 directory를 슬롯별 publication으로 전환. static5·runtime20·Linux2 PASS, OCR delegate 기반 독립 D1 수용·P2/P3 미공급.

- [DEV3 allocator 실제 퇴출·재적재](dev3-allocator-eviction-20261023.md) · [source별 실행 색인](dev3-allocator-eviction-20261023.json): 신규18·관련7·static5·native1·compiler3+control1과 원 FAIL·수정 기록. 일반 pool/DEV3/S5 미완료.

2026-10-23 [DEV3 진행 현황 후속 리뷰](dev3-progress-review-20261023.md): allocator reload commit의 OCR 대상19파일과 snapshot 수정4파일을 정적 검토했다. 진행 설명·단계 JSON·색인의 누락을 수정하고 새 제품 결함0건·제품 시험 NOT_RUN 및 검토 한계를 기록한다.

2026-10-23 [DEV3 구현 코드 리뷰·후속 수정](dev3-code-review-20261023.md): `open-code-review-delegate`의 코어51파일 정적 리뷰에서 확인한 D3-OCR-001을 수정했다. 영속화 후 snapshot 경합의 오류 효과·fence 보완, 신규 관련5건 PASS·별도 D1 수용이며 최초 정적 리뷰와 수정 전 FAIL을 보존한다. 전체 DEV3/S5는 미완료다.

2026-10-23 [COMM 시간 제한·조합별 성능 인수](comm-performance-batch-20261023.md): 별도 미완료 시험의 원 profile·10쌍·측정·2,700분모를 유지하며 같은 보존 source18조합360mode의 BATCH_PASS와 원자료·통계·누적목록의 독립 수용을 확인했다. 21:55:52 KST 선택 실행 종료 뒤 다음119는원보수3,320초>가용2,908.24초로 SKIPPED_TIME_BUDGET·실행0을 보존한다. slow1의 처리량/p99 미적용과 quota·역압·취소/정리 의무를 구분하며 원보수식/품질기준과23시 hard마감은 유지한다. 모델 원 FAIL·oracle/exact dedup 선택 수정·중단과 새6시간 모델의 시간상 skip을 보존한다. 다른 source의 원74를 합산하지 않고 완료 시험 재실행 금지·8시간 skip/23시 마감·C1 INCOMPLETE를 유지한다. 후속 사용자 지시로 [신규 성능5+5 계약](../plans/comm-implementation-plan.md#adaptive-repetitions)을 적용했다. 기본5쌍·편차 시 추가5쌍이며 선택 checker14개 PASS이며22:47 채택 시점의 native는 NOT_RUN으로 기록했다. 과거10쌍 결과는 소급 재분류하지 않는다. 이후 한 시간 추가 요청으로 이번 후속 실행 deadline을23:50·기록 목표23:55 KST로 연장했다. 추가 native는23:41 종료했으며 신규 profile6조합60mode BATCH_PASS,부분5mode·실패2mode·시간상91 SKIP을 구분해 보존한다. 통과6조합은모두5쌍이며추가5쌍은없다. 전체C1은미완료다.

2026-10-23 [COMM 미완료 인수 재개·RX-prefix/process-kill](comm-acceptance-resume-20261023.md): 원 source의30분3개·11 mutant 완료 증거를 보존하고 신규36셀·checker6→4→1·reader 변경 후 native1을 구분한다. 선택 D1/D2 수용, 모델·전체 fault·성능 및 C1 INCOMPLETE.


2026-10-23 [DEV3 allocator 동일 주소 reload](dev3-allocator-residency-20261023.md) · [source별 색인](dev3-allocator-residency-20261023.json): 신규19·Fixed mode 직접2·static4와 native1/compiler3/checker10의 source별 검증·원 FAIL·미실행 보존. 제한 단위의 최종 D1 수용, 일반 pool·전체 DEV3/S5 미완료.

2026-10-23 [DEV3 고정 등록 코어·세대 pin](dev3-registration-core-20261023.md) · [source별 색인](dev3-registration-core-20261023.json): r6 신규11·관련10·native1/두batch·compiler2+control1과 r5/r6 정적 회계를 구별한다. 제한 단위 D2·제품 D1 수용이며 일반 pool·DEV3/S5는 미완료다.

2026-10-23 [DEV3 현재 genesis의 실제 닫힌 복구 인수](dev3-recovery-effects-20261023.md) · [source별 실행 색인](dev3-recovery-effects-20261023.json): 필수393의 완결 job386·실패 job 내 완료 단언7과 native31/95worker·compiler9+control1·checker30/변경1 및 원 FAIL을 보존한다. 제한 단위 최종 D1 수용·일반 recovery/READY·전체 DEV3/S5 미완료다.

2026-10-22 [DEV3 현행 WAL prefix 분석 인수](dev3-recovery-prefix-20261022.md) · [source별 색인](dev3-recovery-prefix-20261022.json): 새 unit231의 완결 job223·실패 job 내 완료 단언8을 구별하고 native31·compiler8+control1·checker15·변경 수신6 및 원 FAIL을 보존한다. 제한 단위 D1 수용·전체 DEV3/S5 미완료다.

2026-10-22 [DEV3 초기 복구 입력 선택 인수](dev3-recovery-select-20261022.md) · [source별 실행 색인](dev3-recovery-select-20261022.json): 필수170·변경 helper4·native8·compiler5+control1·checker15와 원 FAIL 보존. 일반 recovery/READY·S5와 구별한다.

2026-10-22 [DEV3 진행 방향·설계 점검](dev3-direction-review-20261022.md) — Status home flush 이후의 정적 검토와 [후속 반영 추적](dev3-direction-review-20261022.md#adoption-followup). 제한 복구·일반 buffer·첫 행 거래·64GiB/계측 준비의 소유 계약을 연결하며 최초 리뷰와 후속 D2 상태·남은 구현을 구별한다.

2026-10-22 [DEV3 Status home flush](dev3-status-home-flush-20261022.md) · [source별 실행 색인](dev3-status-home-flush-20261022.json): 필수43·compiler6·native3와 실제77 layout를 확인했다. 원 FAIL/완료 prefix를 보존하며 최종 독립 D1 수용, S5 추가588·전체 DEV3 미완료다.

2026-10-22 [DEV3 폐쇄 두-frame Status INIT](dev3-status-init-20261022.md) · [source별 실행 색인](dev3-status-init-20261022.json): 실제 reserve→두 RAM image 공개→동일 WAL requirement의 durable 확인과 두-pin reader를 연결했다. 필수93·shared1·compiler13·native3의 source별 결과와 원 FAIL을 보존한다. 제한 단위의 최종 독립 D1을 수용했으며 Status home flush·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 불변 Status bootstrap](dev3-status-bootstrap-20261022.md) · [실행 색인](dev3-status-bootstrap-20261022.json): 새 필수32·변경 parser1·native1·compiler semantic3/control1과 source별 checker를 검증했다. 제한 단위의 최종 독립 검토를 수용했으며 INIT·전체 DEV3는 미완료다.

2026-10-22 [DEV3 allocator 한 페이지 flush](dev3-allocator-flush-20261022.md) · [source별 실행 색인](dev3-allocator-flush-20261022.json): 필수76의 source별 실행·native3의 bytes/FD·WAL30·compiler12·자원 전제를 검증했다. 원 FAIL·관측 불완전과 완료 prefix를 구분하며 제한 단위의 독립 검토를 수용했다. 전체 DEV3는 미완료다.

2026-10-22 [DEV3 첫 dirty frame·stage 소비자](dev3-dirty-stage-20261022.md) · [source별 실행 색인](dev3-dirty-stage-20261022.json): mandatory53의 source별 완료 범위, compiler18·native3·새 oracle/checker와 원본 실패를 보존한다. INIT·flush·일반 복구와 전체 DEV3는 미완료다.

2026-10-22 [DEV3 적용 admission·version gate](dev3-concurrency-admission-20261022.md) · [source별 실행 색인](dev3-concurrency-admission-20261022.json): 신규 runtime7그룹/45사례·compiler12·자원27 layout과 원래 실패를 연결한다. Commit은 test-only 모의 명세, 확대512와 전체 DEV3는 미완료다.

2026-10-22 [DEV3 예약·취소 잔여 인수](dev3-allocation-reservation-acceptance-20261022.md) · [source별 실행 색인](dev3-allocation-reservation-acceptance-20261022.json): native13DB·compiler·WAL4·64KiB 검증과 원래 실패를 연결한다. 제한 단위 판정과 전체 DEV3/S5 미완료를 구분한다.

2026-10-22 [DEV3 예약·취소 중첩 자원 검토](dev3-allocation-resource-review-20261022.md) — aggregate 타입/수명 guard와 native/compiler 잔여 검증. 실제 stack 실패와 수정·재검증 결과를 별도로 보존하며 전체 DEV3 완료와 구별한다.

2026-10-22 [DEV4 분배 재점검 003 조사](dev4-source-research.md#work-review-003-research) — ARIES 원문 일부·Oracle19c·로컬5개 DBMS의6개 파일을 대조해 앞단 선행·오류 소유·시험 준비 분량을 보완. 기존18개 분류와 원래 조사 identity 유지, 제품/외부 DBMS 시험 미실행.

2026-10-22 [DEV7 분배 후속 검토](dev7-work-breakdown-review-20261022-r2.md) — ARIES·Oracle·로컬4개 실제 소스 대조,135→140개 책임과 제약 설치·온라인 복구·queue/worker·분량/실행 비용 보완. 제품 실행 증거가 아니다.

2026-10-22 [DEV7 작업 분배 검토·보완](dev7-work-breakdown-review-20261022.md) — ARIES·Oracle·로컬8개 DBMS 대조, 36개 분류 유지·115→135개 하위 작업과 공급/인수·위험 우선순위 보완. 제품 실행 증거가 아니다.

2026-10-22 [DEV3 테스트 계획 항목별 재점검](dev3-test-plan-review-20261022.md) —16개 작업의 중간 필수 누락 경계·공동 case·실행 분모·시간/공간 비용 보완. 최종 S5 전수와 기존 budget 유지, 제품·시험 실행 없음.

2026-10-22 [DEV3 16개 상세 설계 2차 비교 리뷰](dev3-task-design-review-20261022-r2.md) — ARIES·Oracle 공식 자료와 로컬6개 DBMS의 실제 revision 대조, Commit 설치 순서·build drain·backup cut의3개 보완. 제품 실행 증거가 아니다.

2026-10-22 [DEV3 16개 상세 설계 비교 리뷰](dev3-task-design-review-20261022.md) — ARIES·Oracle 공식 자료와 실제 PostgreSQL·MySQL·MariaDB·CUBRID·SQLite revision을 대조한8개 설계 보완·문서별 판정·교차검토. 제품 실행 증거가 아니다.

2026-10-21 [WAL·할당·행·DWB 표현 소스의 DEV2 복구 receipt](dev3-storage-formats-dev2-kernel-xadb-20261021-r1.json)는 동일 보존 source에서18개 check와 native99셀을 통과했다. 기존 DEV2 capability 회귀이며 DEV3 실제 디스크 복구 인수는 아니다.

2026-10-20 [tree·overflow·거래 표현 통합 receipt](dev3-expressions-leaf-xadb-20261020-r1.json)와 [동일 소스의 하위 회귀](dev3-expressions-dev2-regression-xadb-20261020-r1.json)를 보존했다. engine 표현43개·foundation scalar3개·부분 모델 정상4/결함8 및 DEV0/1/2 회귀 PASS이며, 아래 복구 receipt와 같은 지문이다. DEV3 전체 인수는 미완료다.

2026-10-20 [tree·overflow·거래 표현 소스의 DEV2 복구 receipt](dev3-expressions-dev2-kernel-xadb-20261020-r1.json)는 동일 보존 source에서18개 check와 native99셀을 통과했다. 새 DEV3 디스크 복구 증거가 아니며 [표현 통합 범위](../implementation/dev3-implementation.md#tree-overflow-transaction)를 따른다.

2026-10-20 [M01–M06 원본 receipt](dev3-models-xadb-20261020-r1.json)는 동일 보존 source에서 정상457·지정 반례127·예산 검사6·M05 workspace 회귀를 통과했다. M06의20개 비적용 경계는 실행 PASS와 구분한다. 유한 모델 검증이며 실제 disk·thread·전체 S0/F05·DEV3 인수가 아니다. [독립 검토·범위](../implementation/dev3-implementation.md#models-all)를 따른다.

2026-10-20 [M04 원본 receipt](dev3-m04-xadb-20261020-r1.json)는 정상5·결함10과 예산 검사를 별도 보존 source에서 실행했다. cap 미완주 이력을 보존한 정확 상태 동치 적용이며, DPT 기반 I/O 선정·실제 frame mapping은 이 모델의 증거가 아니다. [범위·제한](../implementation/dev3-implementation.md#model-4)을 따른다.

2026-10-20 [참조 표현 leaf](dev3-reference-leaf-xadb-20261020-r1.json)는 page7·key5·scalar3·anchor4·reference2·P01 정상4/결함8을 검증했다. 별도 [M01/M02/M05 원본 receipt](dev3-m01-m02-m05-xadb-20261020-r1.json)는 정상32·결함44와 budget·workspace 회귀를 보존 source/release binary로 독립 실행했다. 전체 모델·실제 disk·S1 인수는 미완료이며 [실행 범위](../implementation/dev3-implementation.md#models-125)를 따른다.

2026-10-20 DEV3 [첫 leaf PASS](dev3-leaf-xadb-20261020-r1.json), [하위 회귀](dev3-dev2-regression-xadb-20261020-r1.json), [초기 Clippy 실패](dev3-leaf-xadb-20261020-r0.json)와 [동일 최종 소스의 DEV2 복구 회귀](dev3-dev2-kernel-xadb-20261020-r1.json)를 보존했다. page7·부분 모델 정상4/결함8·DEV0/1/2 회귀·DEV2 native99셀을 검증했다. 실제 DEV3 디스크 복구·전체 모델·S1은 미완료이며 [구현 기록](../implementation/dev3-implementation.md#validation)의 범위에 한정한다.

2026-10-18 · COMM 최종 사본의 DEV0/DEV1/DEV2 보고서는 해당 실행 당시 소스의 회귀 근거이며, 이후 K1–K5 변경을 포함한 현재 소스의 검증 결과가 아니다. K0-005의 별도 성능 근거는 당시 source의 r1 보고서로 보존한다. DEV0 8개·DEV1 8개·DEV2 P0/K0 leaf 28개 check와 새 성능 반례 35개를 실행했다. release 계측 비교 2개와 재현 1개는 각각20개 실행의 정답과 원시 이력 보존을 확인했다. 최초 Always·Detailed 비교의 sample-p99 guardrail은 FAIL이며 성능 인수·승격은 INCOMPLETE다. DEV2·K0 전체, 내구성·protected CI 인수는 완료하지 않았다. r1·r2 회귀 보고서는 당시 source snapshot의 결과로 보존한다. 보고서는 실행기가 생성한 바이트 그대로 복사했다.

r1 당시 DEV0/DEV1은 로컬 범위 PASS다. 당시 DEV2 reference/substrate/frame/worker leaf는 PASS이나 DEV2와 K0 전체는 INCOMPLETE다. r1 당시 영속성·streaming controller·bounded concurrent exploration·PF 인수·30분 stress/fuzz·protected CI는 NOT_RUN이며 안정 제품 qualification이 아니다. `artifacts/` 경로는 해당 로컬 실행 산출물이며 Git으로 배포하지 않는다. 각 보고서의 source hash로 적용 소스를 확인한다.

재현: `python3 -B scripts/check_dev2_performance.py` (저장소 루트, 하위 회귀 포함). P0/K0 회귀만 실행할 때는 `python3 scripts/check_dev2_reference.py --with-substrate`를 사용한다. 과거 실행 결과는 원래 작업 공간에 남기고 새 XADB 결과로 재표기하지 않았다.

DEV3의 [소스 조사](dev3-source-research.md)는 실제 revision·열람 범위의 기록이며 DBMS build·제품 실행 또는 S1 PASS 증거가 아니다.

2026-10-22 [DEV4 작업 분류 추가 조사](dev4-source-research.md#work-breakdown-20261022)는 실제 4개 checkout의 11개 파일 열람 범위·hash, Oracle19c 문서와 ARIES 원문 일부를 기록한다. [18개 작업 분류](../plans/dev4-work-breakdown.md)의 근거이며 제품·외부 DBMS 실행은 없다.

2026-10-22 [DEV4 분배 검토 후속 조사](dev4-source-research.md#work-review-20261022)는 로컬5개 DBMS·10개 파일, Oracle19c·ARIES 일부 원문을 작업 크기·초기 영속 경로·수명 모델·정렬 재사용 판단에 연결한다. 최초 조사와 실행 상태는 보존하며 제품 시험·성능 측정은 없다.

DEV4의 [소스·Oracle/ARIES 조사](dev4-source-research.md)는 PostgreSQL·MariaDB·CUBRID·SQLite의 SQL/prepared 및 복구 경계와 Oracle 19c 공식 문서·ARIES 초록/원 논문의 명시한 부분을 읽은 기록이다. SQL 구현·Oracle 실행 비교·Q1 PASS 증거가 아니다.

COMM 반영 commit과 당시 DEV3/DEV4 문서 commit 대상의 source SHA-256은 `e873f96b2b791d0c655dfb4019de4db6209a4f1ab02359f57e4f861815b62ad1`이며, 보존된 COMM 회귀 보고서의 실행 전후 지문과 일치한다. `dev0-validation.json`·`dev1-validation.json`·`dev2-validation.json` 문서 ID는 각각 `dev0/1/2-current-source-20261018.json`을 참조한다. 이 세 파일은 `comm-dev0/1/2-xadb-20261018-r1.json`의 byte-identical 사본이며, 원래 COMM 문서 ID·상대 artifact 경로·실행 root와 원본 hash를 유지한다. catalog의 단일 경로 규칙 안에서 기존 참조를 보존하기 위한 채택 사본으로 새 실행이나 재검증 보고서가 아니다. `dev2-performance-validation.json`은 source `6e58ed3534a1c996a0d5bf9ea56e3f4e81d6dc95bc2b67fc704589f12b4a0e8f`의 별도 성능 진단 r1을 유지한다. 이전 회귀 r1·r2·r3는 원래 파일 경로 ID와 바이트로 보존한다. 이 참조 갱신은 기존 실행의 적용 소스를 확인한 것이며 새 제품 시험 실행이나 진행 중인 DEV2 혼합 작업 트리의 PASS가 아니다. 이전 시험 패키지 재구성의 범위·리뷰는 [경로 재구성 기록](../history/project-migration.md#testing-layout)을 따른다. 이번 병렬 구현·검토·실측 한계는 [K0-005 기록](../implementation/dev2-implementation.md)을 따른다.

<!-- catalogue -->

- [DEV3 복합 key 표현·page·부분 snapshot 모델](dev3-key-leaf-xadb-20261020-r1.json) — 보존 소스의 key5·scalar3·page7·P01; 전체 모델·disk 인수·하위 전체 회귀 제외

- [DEV3 page envelope·부분 snapshot 모델·하위 회귀](dev3-leaf-xadb-20261020-r1.json) — 보존 소스의 실제 leaf 결과
- [DEV3 첫 leaf와 같은 소스의 DEV2 P0/K0·DEV0/1 회귀](dev3-dev2-regression-xadb-20261020-r1.json) — 28개 check와 하위8/8개 PASS
- [DEV3 첫 통합 실패 기록](dev3-leaf-xadb-20261020-r0.json) — Clippy map_entry 실패 보존
- [DEV3 첫 leaf와 같은 소스의 DEV2 복구 회귀](dev3-dev2-kernel-xadb-20261020-r1.json) — 현재-build 공급 회귀, DEV3/S1 인수 미완료

- [DEV9 — Oracle·ARIES·로컬 소스 조사와 설계 검토](dev9-source-research.md) — DESIGN-002 원장·ACK 관측·PITR·공식 배포 재검토, 공개 로컬4개·LOCAL-Q01의 실제 revision/hash; 제품 실행 아님

- [DEV8 — Oracle·ARIES·로컬 소스 조사와 설계 검토](dev8-source-research.md) — 공개 로컬4개·LOCAL-R01 실제 revision/hash와 설계 선택; 제품 실행 아님
- [DEV8 — 운영 설계 2차 검토](dev8-review-002.md) — domain별 중복 제거·sequence 보호·partition 이동·WAL/control cut 보완, Oracle/ARIES·로컬 소스 근거; 제품 실행 아님

- [DEV7 세부 분류 — ARIES·Oracle·로컬 DBMS 조사](dev7-work-breakdown-research.md) — 2026-10-22 실제 로컬8개 revision/열람·공식 자료와 36개 분류의 근거; 제품 비교 실행 아님

- [DEV7 — 비교 문맥·CALL·XA 설계 재검토](dev7-review-002.md) — Oracle·ARIES·실제 로컬 소스 revision, 네 계약 반례와 DESIGN-002 보완·독립 검토 상태; 제품 인수 아님

- [DEV7 — Oracle·로컬 DBMS 조사와 설계 검토](dev7-source-research.md) — Oracle26ai 등 공식 문서와 실제 로컬8개 revision; 실행 비교·제품 인수 아님

- [상용 수준 확장 한계 조사와 선제 보완](commercial-extensibility-research.md) — Oracle·공개 로컬3개·SRC-L04 실제 revision/hash, DEV3/4 자원·진행·설정/복구 경계와 독립 검토; 제품 인수 증거 아님

- [DEV6 세부 분류 — ARIES·Oracle·로컬 DBMS 조사](dev6-work-breakdown-research-20261022.md) — 2026-10-22 실제 원문·6개 checkout revision/hash·18개 분류 적용과 정적 검토; 제품 실행 없음
- [DEV6 작업 분배 재검토와 개선 근거](dev6-work-breakdown-review-20261022.md) — ARIES·Oracle·로컬6개 재열람, 추가 완료 단위·ODBC 선행·담당/분량/실행 예산 보완과 별도 검토; 제품 실행 없음
- [DEV6 작업 배정 2차 점검 — API 분량과 인수 연결](dev6-work-breakdown-review-20261022-r2.md) — ARIES·Oracle·로컬5개 대조, 72→80개 배정 단위·capability/type-info 소비 연결·성능 호출 경계; D0 문서 보완·제품 실행 없음
- [DEV6 — Oracle·ARIES·로컬 DBMS 클라이언트 조사와 검토](dev6-source-research.md) — DEV6 설계·REVIEW-002 상태/취소/결과/버퍼 보완·인수 의무; CLIENT1 미실행

- [DEV5 — 서버 소스 조사와 독립 검토 기록](dev5-source-research.md) — PostgreSQL·MariaDB·CUBRID 실제 revision/열람 근거와 DEV5-REVIEW-002 대기·수명 반례/수정; 제품 실행 증거 아님
- [DEV5 — Oracle·로컬 DBMS 비교와 후속 설계 검토](dev5-oracle-source-review.md) — Oracle 19c·로컬3개 DBMS의 실제 열람, cleanup 권한·idle 경계와 002 수신 검토; 제품 실행 증거 아님
- [DEV5 작업 분배 재검토와 개선 근거](dev5-work-breakdown-review-20261022.md) — 최초44→64개 하위 단위와 후속003의 close/fetch 순환·일반 거래/cleanup 조기 인수·실행 분량 보완, ARIES·Oracle·로컬5개 소스 및 별도 검토; 제품 실행 없음
- [DEV5 작업 분류 — ARIES·Oracle·로컬 DBMS 조사](dev5-work-breakdown-research-20261022.md) — 2026-10-22 원문·로컬5checkout/8파일의 revision·hash·선택 범위와16개 분류의 근거; 제품 실행 증거 아님

- [DEV3·DEV4 아키텍처 비교와 설계 보완](dev3-dev4-architecture-review.md) — 실제 source revision·교차 상태/자원/batch 보완과 독립 검토; 모델·제품 실행 증거 아님
- [COMM 소스 조사와 설계 검토 기록](comm-source-research.md) — 최초 DBMS 조사와 후속 readiness·clock·종료 수명 리뷰/수정 근거; C1 실행 증거 아님
- [COMM 코드 리뷰와 수정 기록](comm-code-review-20261019.md) — driver 취소·accept 오류 격리 결함, DBMS 소스 비교와 해당 회귀·독립 검토; 전체 C1 인수 아님
- [dev0-xadb-20261018-r1.json](dev0-xadb-20261018-r1.json)
- [dev1-xadb-20261018-r1.json](dev1-xadb-20261018-r1.json)
- [dev2-xadb-20261018-r1.json](dev2-xadb-20261018-r1.json)
- [implementation-import-inventory.json](implementation-import-inventory.json)
- [DEV3 소스 조사 기록](dev3-source-research.md)
- [DEV4 소스·Oracle/ARIES 조사 기록](dev4-source-research.md)

- [후속 구현 이관 inventory](implementation-incremental-import-20261018.json)

후속 코드의 source fingerprint에는 r1의 PASS를 적용하지 않는다. 중단한 `artifacts/dev2-reference/check-kecbzonw`의 부분 결과는 완료 증거가 아니다.

- [dev0-xadb-20261018-r2.json](dev0-xadb-20261018-r2.json)
- [dev1-xadb-20261018-r2.json](dev1-xadb-20261018-r2.json)
- [dev2-xadb-20261018-r2.json](dev2-xadb-20261018-r2.json)

- [DEV3 WAL·undo 재설계 조사와 검토](dev3-wal-undo-research.md): 선행 리뷰 보완 후 저장 재설계; 소스 열람/설계 검토이며 제품 실행 NOT_RUN.


- [dev0-xadb-20261018-r3.json](dev0-xadb-20261018-r3.json)
- [dev1-xadb-20261018-r3.json](dev1-xadb-20261018-r3.json)
- [dev2-xadb-20261018-r3.json](dev2-xadb-20261018-r3.json)
- [dev2-performance-xadb-20261018-r1.json](dev2-performance-xadb-20261018-r1.json)

- [K0-006 중단 보고서](dev2-extended-xadb-20261018-r1.json): 사용자 요청으로 약464초에 중단; FAIL 원본 보존, 30분 의무와 하위 회귀 미완료. K1–K5 구현 완료 전 테스트 금지.

- [COMM 전용 기준 사본 r1](comm-isolated-xadb-20261018-r1.json): 27개 시험 및 하위 회귀 PASS. 마지막 drain 경합 보완 전 source에만 적용.
- [COMM 혼합 사본 실패 r1](comm-mixed-xadb-20261018-r1.json): COMM focused 검사는 통과했으나 함께 포함된 DEV2 변경의 format 검사에서 하위 회귀 FAIL. 원본 실패를 보존.
- [COMM 전용 DEV0 회귀](comm-dev0-xadb-20261018-r1.json) · [COMM 전용 DEV1 회귀](comm-dev1-xadb-20261018-r1.json): 최종 COMM 전용 기준 사본에서 각 8개 check PASS. 당시 소스 지문 대조 뒤 current-source 문서 ID로 연결했으며 원본 실행 identity는 유지.
- [COMM 전용 최종 사본 r2](comm-isolated-xadb-20261018-r2.json): 28개 Rust 시험(기본·전체 feature 각각), 12개 native 사례, lint·독립 build·하위 회귀 PASS. 전체 C1은 INCOMPLETE.
- [COMM 전용 DEV2 회귀](comm-dev2-xadb-20261018-r1.json): 최종 COMM 전용 기준 사본의 P0/K0 leaf 28개 check PASS; 내구성·현재 혼합 작업 트리의 검증 아님.
- [COMM 실행 범위 색인](comm-execution-scope-20261018.json): 각 원본의 execution root·recipe·hash와 소유 파일 일치 기록. 위 COMM 보고서의 상대 artifact 경로는 이 색인의 해당 execution root 기준이다.

- [DEV0 당시 source 채택 사본](dev0-current-source-20261018.json) · [DEV1 당시 source 채택 사본](dev1-current-source-20261018.json) · [DEV2 당시 source 채택 사본](dev2-current-source-20261018.json): 기존 COMM 회귀 JSON과 바이트 동일. 상대 artifact 경로는 [원래 실행 범위](comm-execution-scope-20261018.json)의 root에서 해석한다.

- [K1–K5 영속 커널 검증 체크포인트](dev2-kernel-xadb-20261018-r1.json): 고정 source에서 engine 22·transaction 7·cache-loss 15·동시성 64 seeds와 첫 native 33 cases strict ACCEPTED. 2회차 ENOSPC로 FAIL, 3회차 미실행. 원래 빈 report와 로그를 보존한 사후 재구성 receipt이며 DEV2 완료 증거가 아님.

- [DEV2 남은 시험 실행 — 2026-10-19](dev2-completion-xadb-20261019-r1.json): 동결 source `47c84c22…`의 native 33셀×3회, 9개 30분 target, P0 9,604,608건, 하위 회귀와 변이/JSON 반례 완료. K0 후보의 새 확인·재현은 개선·한도 PASS이며, 일부 계측 비용 한도 FAIL로 DEV2 전체 인수는 INCOMPLETE. 최초 source 지문·빌드 불일치 실패도 보존.

- [DEV2 계측 비용·CPU 관측 수정 — 2026-10-19](dev2-instrumentation-fix-xadb-20261019-r1.json): ext4 kernel 99셀·하위 회귀·36개 필수 성능 시험 PASS, 최종 로컬 성능 인수 INCOMPLETE. 원래 guardrail 실패·탐색·quota 실패와 tmpfs 범위 정정 보존.

- [DEV2 삭제 후 새 시험과 일시 중지 — 2026-10-19](dev2-resumed-tests-xadb-20261019-r1.json): e3 소스의 11개30분 target 중 11개 PASS, 보완 복구·격리·36개 정답 검사. 6개 target 종료 뒤 Python 캐시로 생긴 집계기 FAIL은 독립 소스 대조와 구분해 보존. 사용자 요청으로 새 성능은 미실행, DEV2 인수 미완료.

- [DEV2 성능 후속 실행](dev2-performance-followup-xadb-20261020-r1.json) — 고정 소스9개 campaign·180run 정답 PASS, PF12/PF13 개별 완료와 PF04 비용 한도 FAIL; 동일 Off 대조40run은 진단 전용

- [DEV2 fsync·스케줄링 진단](dev2-fsync-diagnostic-xadb-20261020-r1.json) — 고정4run의 정답·추적 연결과 측정2,048개 fsync 시간 분해; 기존 PF04 실패를 유지하는 진단 전용 증거

- [DEV2 PF04 가설 검토](dev2-pf04-hypothesis-review-xadb-20261020-r1.json) — 기존 추적·설치 버전 커널 소스의 독립 대조, 추가 Arc 비용과 max_batch_time 설정 가설의 한계; 새 제품 실행·인수 승격 없음

- [DEV2 journal 대기 진단](dev2-journal-diagnostic-xadb-20261020-r1.json) — 고정 Off 1회 정답·512개 fsync/TID/단계 연결; 추적 영향·환경 확인 FAIL, 계측 없는 실행의 원인·성능 인수로 확대하지 않음

- [DEV2 journal binary 대응](dev2-journal-binary-mapping-xadb-20261020-r1.json) — 실행 커널과 build ID가 같은 ELF에서 두 stack 위치를 metadata·commit record 버퍼 대기로 대응; 추가 제품 실행과 성능 인수 승격 없음

- [DEV3 WAL·할당·행·DWB 표현 통합 원본 실행 결과](dev3-storage-formats-leaf-xadb-20261021-r1.json) — 원본 PASS, source e419f2d7…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 WAL·할당·행·DWB 표현 소스의 DEV2 P0/K0·DEV0/1 회귀](dev3-storage-formats-dev2-regression-xadb-20261021-r1.json) — 원본 PASS, source e419f2d7…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 catalog·undo·control 표현 통합 원본 실행 결과](dev3-metadata-formats-leaf-xadb-20261021-r1.json) — 원본 PASS, source f54bf874…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 catalog·undo·control 표현 소스의 DEV2 P0/K0·DEV0/1 회귀](dev3-metadata-formats-dev2-regression-xadb-20261021-r1.json) — 원본 PASS, source f54bf874…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 catalog·undo·control 표현 소스의 DEV2 복구 원본 실행 결과](dev3-metadata-formats-dev2-kernel-xadb-20261021-r1.json) — 원본 PASS, source f54bf874…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 selector·표현·부분 모델 통합 원본 실행 결과](dev3-selector-leaf-xadb-20261021-r2.json) — 해당 보존 소스의 leaf 검증; DEV3 전체 인수 아님
- [DEV3 selector와 같은 소스의 DEV0/1·DEV2 하위 회귀](dev3-selector-dev2-regression-xadb-20261021-r2.json) — 해당 보존 소스의 leaf 검증; DEV3 전체 인수 아님
- [DEV3 selector와 같은 소스의 현재-build DEV2 복구 회귀](dev3-selector-dev2-kernel-xadb-20261021-r2.json) — 해당 보존 소스의 leaf 검증; DEV3 전체 인수 아님
- [DEV3 열린 handle metadata port 원본 실행 결과](dev3-metadata-port-xadb-20261021-r0.json) — 해당 보존 소스의 leaf 검증; DEV3 전체 인수 아님

- [DEV3 선택 page reader 통합 원본 실행 결과](dev3-page-reader-leaf-xadb-20261021-r2.json) — source8e86232e…의 고정 범위; DEV3 전체 인수 아님.

- [DEV3 선택 page reader 소스의 DEV0/1·DEV2 회귀](dev3-page-reader-dev2-regression-xadb-20261021-r2.json) — source8e86232e…의 고정 범위; DEV3 전체 인수 아님.

- [DEV3 선택 page reader 소스의 현재-build DEV2 복구](dev3-page-reader-dev2-kernel-xadb-20261021-r2.json) — source8e86232e…의 고정 범위; DEV3 전체 인수 아님.

- [DEV3 선택 page reader 소스의 Clippy·fmt·build feature 기록](dev3-page-reader-quality-xadb-20261021-r2.json) — source8e86232e…의 고정 범위; DEV3 전체 인수 아님.

- [DEV3 상태 home 표현의 국소 실행 결과](dev3-status-home-local-xadb-20261021-r0.json) — 고정된 해당 소스·검증 범위; 실제 상태 갱신·DEV3 복구 인수 아님.

- [DEV3 상태 home 통합 소스의 현재-build DEV2 복구](dev3-status-home-dev2-kernel-xadb-20261021-r1.json) — 고정된 해당 소스·검증 범위; 실제 상태 갱신·DEV3 복구 인수 아님.

- [DEV3 상태 home 통합 소스의 Clippy·fmt 실행 결과](dev3-status-home-quality-xadb-20261021-r1.json) — 고정된 해당 소스·검증 범위; 실제 상태 갱신·DEV3 복구 인수 아님.

- [DEV3 상태 home 표현 통합 원본 실행 결과](dev3-status-home-leaf-xadb-20261021-r1.json) — source dd7163fb…의 고정 범위; 실제 DEV3 복구·S1 인수 아님.

- [DEV3 상태 home 통합 소스의 DEV0/1·DEV2 회귀](dev3-status-home-dev2-regression-xadb-20261021-r1.json) — source dd7163fb…의 고정 범위; 실제 DEV3 복구·S1 인수 아님.

- [DEV3 obligation descriptor의 국소 실행 결과](dev3-obligation-local-xadb-20261021-r0.json) — 고정된 해당 소스·검증 범위; 실제 예약 인계·DEV3 복구 인수 아님.

- [DEV3 obligation descriptor 통합 소스의 현재-build DEV2 복구](dev3-obligation-dev2-kernel-xadb-20261021-r1.json) — 고정된 해당 소스·검증 범위; 실제 예약 인계·DEV3 복구 인수 아님.

- [DEV3 obligation descriptor 통합 소스의 Clippy·fmt 실행 결과](dev3-obligation-quality-xadb-20261021-r1.json) — 고정된 해당 소스·검증 범위; 실제 예약 인계·DEV3 복구 인수 아님.

- [DEV3 obligation descriptor 통합 원본 실행 결과](dev3-obligation-leaf-xadb-20261021-r2.json) — source 7dabd13a…의 고정 범위; 실제 DEV3 복구·S1 인수 아님.

- [DEV3 obligation descriptor 통합 소스의 DEV0/1·DEV2 회귀](dev3-obligation-dev2-regression-xadb-20261021-r2.json) — source 7dabd13a…의 고정 범위; 실제 DEV3 복구·S1 인수 아님.

- [DEV3 ownership 관측 국소 실행 결과](dev3-ownership-local-xadb-20261021-r2.json) — source31ab0f…의 고정 범위; 실제 DEV3 예약 복원·S1 인수 아님.

- [DEV3 ownership 소스의 DEV2 복구 실행 결과](dev3-ownership-dev2-kernel-xadb-20261021-r1.json) — source31ab0f…의 고정 범위; 실제 DEV3 예약 복원·S1 인수 아님.

- [DEV3 ownership 통합 원본 실행 결과](dev3-ownership-leaf-xadb-20261021-r1.json) — 동일 source의 제한된 실행 범위 PASS; 전체 DEV3 인수 미완료

- [DEV3 ownership 소스의 DEV0/1/2 회귀 실행 결과](dev3-ownership-dev2-regression-xadb-20261021-r1.json) — 동일 source의 제한된 실행 범위 PASS; 전체 DEV3 인수 미완료

- [DEV3 현재 intent 검산 국소 실행 결과](dev3-ownership-intent-local-xadb-20261021-r0.json) — 실제 profile/current intent의 국소 범위 및 같은 source의 기존 복구 회귀; 전체 DEV3 인수 미완료

- [DEV3 현재 intent 소스의 DEV2 복구 실행 결과](dev3-ownership-intent-dev2-kernel-xadb-20261021-r0.json) — 실제 profile/current intent의 국소 범위 및 같은 source의 기존 복구 회귀; 전체 DEV3 인수 미완료

- [DEV3 현재 intent 통합 원본 실행 결과](dev3-ownership-intent-leaf-xadb-20261021-r0.json) — source e83efc08…의 고정 범위 PASS; 실제 예약 복원·전체 DEV3 인수 미완료

- [DEV3 현재 intent 소스의 DEV0/1/2 회귀 실행 결과](dev3-ownership-intent-dev2-regression-xadb-20261021-r0.json) — source e83efc08…의 고정 범위 PASS; 실제 예약 복원·전체 DEV3 인수 미완료

- [DEV3 RollbackStart 결합 검산 국소 실행 결과](dev3-rollback-start-local-xadb-20261021-r1.json) — source99bec497…의 제한된 결합 관측·같은 build 복구 회귀; 실제 DEV3 적용·전체 인수 미완료

- [DEV3 RollbackStart 소스의 DEV2 복구 실행 결과](dev3-rollback-start-dev2-kernel-xadb-20261021-r0.json) — source99bec497…의 제한된 결합 관측·같은 build 복구 회귀; 실제 DEV3 적용·전체 인수 미완료

- [DEV3 RollbackStart 통합 원본 실행 결과](dev3-rollback-start-leaf-xadb-20261021-r0.json) — source99bec497…의 제한된 결합 관측·같은 build 하위 회귀; 실제 DEV3 적용·전체 인수 미완료

- [DEV3 RollbackStart 소스의 DEV0/1/2 회귀 실행 결과](dev3-rollback-start-dev2-regression-xadb-20261021-r0.json) — source99bec497…의 제한된 결합 관측·같은 build 하위 회귀; 실제 DEV3 적용·전체 인수 미완료

- [DEV3 읽기 버퍼 국소 원본 실행 결과](dev3-read-buffer-local-xadb-20261021-r2.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 읽기 버퍼 수명 compiler 원본 실행 결과](dev3-read-buffer-borrows-xadb-20261021-r1.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 읽기 버퍼 통합 원본 실행 결과](dev3-read-buffer-leaf-xadb-20261021-r1.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 읽기 버퍼 소스의 DEV0/1/2 회귀 실행 결과](dev3-read-buffer-dev2-regression-xadb-20261021-r1.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 읽기 버퍼 소스의 DEV2 현재-build 복구 실행 결과](dev3-read-buffer-dev2-kernel-xadb-20261021-r1.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 국소 원본 실행 결과](dev3-genesis-wal-local-xadb-20261021-r6.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL slot 수명 compiler 원본 결과](dev3-genesis-wal-borrows-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 실제 port trace·유한 모델 검산 원본](dev3-genesis-wal-models-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 통합 원본 실행 결과](dev3-genesis-wal-leaf-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 소스의 DEV0/1/2 회귀 원본](dev3-genesis-wal-dev2-regression-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 소스의 DEV2 현재-build 복구 원본](dev3-genesis-wal-dev2-kernel-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL native 시험 격리 반복 원본](dev3-genesis-wal-parallel-native-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 RollbackStart page 구성 국소 실행 원본](dev3-rollback-materialization-local-xadb-20261021-r1.json) — source3493fa22…의 unpublished status page 구성; live dirty frame·WAL apply·전체 DEV3 인수 미완료

- [DEV3 RollbackStart page 구성 통합 실행 원본](dev3-rollback-materialization-leaf-xadb-20261021-r0.json) — source3493fa22…의 unpublished status page 구성; live dirty frame·WAL apply·전체 DEV3 인수 미완료

- [DEV3 RollbackStart page 구성 소스의 DEV0/1/2 회귀 원본](dev3-rollback-materialization-dev2-regression-xadb-20261021-r0.json) — source3493fa22…의 unpublished status page 구성; live dirty frame·WAL apply·전체 DEV3 인수 미완료

- [DEV3 RollbackStart page 구성 소스의 DEV2 현재 빌드 복구 원본](dev3-rollback-materialization-dev2-kernel-xadb-20261021-r0.json) — source3493fa22…의 unpublished status page 구성; live dirty frame·WAL apply·전체 DEV3 인수 미완료

- [DEV3 RollbackStart WAL·page 생산 국소 실행 원본](dev3-rollback-producer-local-xadb-20261021-r0.json) — sourcef08f01d7…의 caller frame/page 생산; tag2 append·live 적용·전체 DEV3 인수 미완료

- [DEV3 RollbackStart WAL·page 생산 통합 실행 원본](dev3-rollback-producer-leaf-xadb-20261021-r0.json) — sourcef08f01d7…의 caller frame/page 생산; tag2 append·live 적용·전체 DEV3 인수 미완료

- [DEV3 RollbackStart WAL·page 생산 DEV0/1/2 회귀 원본](dev3-rollback-producer-dev2-regression-xadb-20261021-r0.json) — sourcef08f01d7…의 caller frame/page 생산; tag2 append·live 적용·전체 DEV3 인수 미완료

- [DEV3 RollbackStart WAL·page 생산 DEV2 현재 빌드 복구 원본](dev3-rollback-producer-dev2-kernel-xadb-20261021-r0.json) — sourcef08f01d7…의 caller frame/page 생산; tag2 append·live 적용·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 D2 검토 원본](dev3-storage-session-d2-xadb-20261021-r2.json) · [49개 동결 입력 원본](dev3-storage-session-inputs-xadb-20261021-r6.json) — 설계 수용이며 제품 실행 증거는 아님

- [DEV3 하위 회귀 aggregate 예산 제안 원본](dev3-storage-session-aggregate-proposal-xadb-20261021-r0.json) — 원래 source·결정/실행 상태를 보존하며 후속 실행 PASS를 대신하지 않는다.

- [DEV3 하위 회귀 aggregate 예산 독립 D2 원본](dev3-storage-session-aggregate-review-xadb-20261021-r0.json) — 원래 source·결정/실행 상태를 보존하며 후속 실행 PASS를 대신하지 않는다.

- [DEV3 최초 저장 세션 통합 r0 timeout FAIL 원본](dev3-storage-session-leaf-xadb-20261021-r0.json) — 원래 source·결정/실행 상태를 보존하며 후속 실행 PASS를 대신하지 않는다.

- [DEV3 하위 회귀 wrapper 독립 D1 원본](dev3-storage-session-aggregate-d1-xadb-20261021-r0.json) — 실제 timeout 포함 자기시험12 PASS; 새 전체 통합 검증 대체 아님

- [DEV3 최초 저장 세션 품질 검사 원본](dev3-storage-session-quality-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 소유권 compiler8 원본](dev3-storage-session-borrows-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 실제 효과·유한 durable 모델 원본](dev3-storage-session-model-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 실제 ext4 장애·SIGKILL 검산 원본](dev3-storage-session-native-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 통합 실행 원본](dev3-storage-session-leaf-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 DEV0/1/2 회귀 원본](dev3-storage-session-dev2-regression-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 DEV2 현재-build 복구 원본](dev3-storage-session-dev2-kernel-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 최종 독립 감사 원본](dev3-storage-session-review-xadb-20261021-r1.json) — sourceedec6006…의 제한된 구현·원본 실행·staged source 감사 PASS; 전체 DEV3 미완료

- [DEV3 최초 DWB 설치 독립 D2 결정 원본](dev3-doublewrite-install-d2-xadb-20261021-r9.json) — D2 설계·입력 채택이며 제품 실행 NOT_RUN

- [DEV3 최초 DWB 설치 r6 동결 입력 원본](dev3-doublewrite-install-inputs-xadb-20261021-r6.json) — D2 설계·입력 채택이며 제품 실행 NOT_RUN

- [DEV3 최초 DWB 설치 r8 보충 입력 원본](dev3-doublewrite-install-inputs-xadb-20261021-r8.json) — D2 설계·입력 채택이며 제품 실행 NOT_RUN

- [DEV3 최초 DWB 설치 runtime10·native3·compiler9 및 품질 실행 원본](dev3-doublewrite-install-local-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 하위 회귀 중단으로 FAIL한 통합 실행 원본](dev3-doublewrite-install-leaf-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 소유권 compiler9 원본](dev3-doublewrite-install-borrows-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 실제 trace·관측·유한 모델 및 checker19 원본](dev3-doublewrite-install-model-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 실제 ext4 설치·장애·종료 및 checker12 원본](dev3-doublewrite-install-native-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 사용자 지시에 따른 당시 하위 회귀 중단 원본](dev3-doublewrite-install-user-stop-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 하위 회귀 종료·kernel 미실행 원본](dev3-doublewrite-install-stop-terminal-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 국소 실행·소스 독립 D1 검토 원본](dev3-doublewrite-install-d1-local-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 완료한 개별 검증의 독립 D1 감사 원본](dev3-doublewrite-install-d1-partial-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 변경 범위 검증 정리](dev3-doublewrite-install-scoped-xadb-20261021-r2.json) — 기존 runtime10·native3·compiler9·checker31 결과; 통합 원본 FAIL과 미실행 범위 구분, 새 실행 아님

- [DEV3 최초 DWB 설치 최종 독립 D1 감사 원본](dev3-doublewrite-install-review-xadb-20261021-r2.json) — 기존 r12/r13 결과와 source 동등성 재사용; 새 테스트 없이 제한 설치 단위 commit 수용

- [DEV3 페이지 예약·취소 D2 채택 원본](dev3-allocation-reservation-d2-xadb-20261021-r2.json) — 독립 raw63·의미 반례541·compiler 입력6 동결; 제품 시험 미실행

- [DEV3 페이지 예약·취소 부분 실행 및 중단](dev3-allocation-reservation-partial-xadb-20261021.json) — r2/r3/r4 source·ELF·로그와 원본 FAIL 보존; native/compiler·변경 stage 관련시험은 미완료

- [DEV3 페이지 예약·취소 부분 commit 독립 검토](dev3-allocation-reservation-partial-review-xadb-20261021.json) — 메모리 포트 구현의 부분 commit 수용; 앞선 실행 기록의 pending review를 해소하며 전체 단위 인수·native/compiler 검증은 미완료

<a id="dev0-owned-child-20261023"></a>

## DEV0 직접 소유한 worker 회수 — 2026-10-23

**D1 국소 수정·선택 검증 완료.** 사용자 요청으로 DEV0/DEV1 리뷰 지적의 타당성을 다시 확인한 뒤 `OwnedChild::finish`를 보완했다. [현재 동작](../implementation/dev0-implementation.md)과 [DEV0 계약](../contracts/dev0-contract.json)에 반영하며, 기존 DEV0/DEV1 전체 실행 증거는 원래 source와 범위로 보존한다.

`process_group(0)`는 새 session을 만들지 않는다. 따라서 worker가 같은 session의 다른 그룹으로 이동하고 원래 그룹에 다른 프로세스가 남으면, 원래 그룹의 `killpg`가 성공해도 worker는 생존한다. 기존 성공 분기는 직접 소유한 `Child`를 종료하지 않은 채 `wait`했으므로 무기한 대기할 수 있었다. 수정은 group 신호 성공·ESRCH 모두에서 unreaped `Child`에 종료를 요청한 뒤 기다린다. 다른 group 오류 보고와 reap 전 신호 순서는 유지한다. 이는 직접 소유한 Child 회수의 보완이며, 다른 session으로 이탈한 descendants를 봉쇄하는 보안 sandbox의 구현은 아니다.

새 회귀는 시험 부모가 두 `Child`를 직접 소유한다. 첫 worker의 그룹에 둘째 프로세스를 가입시킨 뒤 stdin handshake로 worker만 부모 그룹으로 이동시키고 `getpgid`로 양쪽 소속을 확인한다. 직접 자식인지 후손인지와 관계없이 이 분기의 kernel 그룹 구성은 같다. worker는 5초 뒤 정상 종료 marker를 남기므로 원래 결함에서도 무한 대기하지 않는다. 직접 finish는 종료 signal, Drop은 marker 부재와 reap을 검사하며 둘째 Child도 명시 회수한다. PID 재사용과 경합하는 별도 신호 watchdog은 사용하지 않는다.

### 실행 범위와 결과

`d31cfb9bcd97e64554e0d97aa47fe3a3cbe70160`의 `git archive` 사본에서 `crates/testing/controller/src/process.rs`만 덮어썼다. RED는 새 시험만, GREEN은 그 위에 실제 수정만 반영했다. 동시에 진행 중인 다른 작업의 미커밋 변경은 이 사본에 포함하지 않았다. 아래 모든 Cargo 실행은 export 밖의 별도 `CARGO_TARGET_DIR`를 사용했다.

| 구분 | 선택 명령 | 실제 결과 |
| --- | --- | --- |
| RED | `cargo test -p xadbtest --lib --locked process::tests::retained_group_ -- --test-threads=1 --nocapture` | exit101, 신규2개 모두 첫 fixture에서 FAIL. finish의 signal은 `None`, Drop은 정상 종료 marker 잔존 |
| GREEN | `cargo test -p xadbtest --lib --locked process::tests:: -- --test-threads=1 --nocapture` | exit0, 변경된 finish를 호출하는8개 PASS. 신규 finish·Drop2개는 각각 새 fixture3회. parent 집계의 subprocess helper1개 ignored는 PASS 수에 포함하지 않으며 신규 시험에서 별도 프로세스로6회 호출 |
| 서식 | `rustfmt --edition 2024 --check crates/testing/controller/src/process.rs` | exit0 |
| 정적 검사 | `cargo clippy -p xadbtest --lib --tests --locked -- -D warnings` | exit0. test target을 정적으로 검사하며 시험 사례를 추가 실행하지 않음 |

DEV0/DEV1 wrapper·전체 workspace·DEV2/DEV3/COMM 시험은 실행하지 않았다. GREEN8개는 모두 실제 수정한 finish를 호출하며, source 전체 지문이 바뀌었다는 이유로 다른 시험을 선택하지 않았다. 문서 자동 검사는 push gate의 시점에 남긴다. 이 국소 결과는 단계 전체의 새 PASS나 제품 qualification이 아니다.

| 보존 identity | SHA-256 |
| --- | --- |
| RED export source, 실행 전=후 | `c70c1f4cd9bff2c262d4903f5f085299721ffab01425bc13d896aeae92539422` |
| RED process.rs | `ad523a7bd9102b9d4d2505b3056d955120ff9b41463b8d9aab2083c836323bd7` |
| RED 원 로그 | `7f1436ea111e84eacda8ffdaa823dc9775482844a341204387a0f5e848fdb0e0` |
| GREEN export source, 실행 전=후 | `1f579dab7c8f98eaa6a8c428a13f34ed0b41f0369fcbd983dd3ceebbe1405b08` |
| GREEN process.rs | `50768d07b3747492ce095106b1b06a2551d60f240740f88255d89e93ca8e9715` |
| GREEN 원 로그 | `ac903570201a6e79a52ea4ce9a2e3ba64b967e6769a4f80f58fcb9678983124f` |
| 서식 로그 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| Clippy 로그 | `cb6fc2b7d78493b16d8452f6757e14ae1b6b8b33d48808c0b2fd1781783d1d3d` |

로컬 원본은 `artifacts/dev0-cleanup-20261023-6uo9dxng/`의 `red.json`·`red.log`·`red-process.rs`·`green.json`·`green.log`·`static.json`·`format.log`·`clippy.log`와 `source/`에 보존했다. 해당 export의 지문을 진행 중인 전체 작업 트리의 지문으로 재표기하지 않는다.

별도 검토자 `/root/dev0_cleanup_review`는 최종4파일 diff·원 반례·RED/GREEN 원 로그와 source/log hash, 문서의 범위·수치·기존 문서 ID 유지까지 읽기 전용으로 확인해 D1 수용했다. 필수 수정 지적은 없으며 구현자의 자기 검토나 시험 재실행을 독립 검토로 기록하지 않는다.

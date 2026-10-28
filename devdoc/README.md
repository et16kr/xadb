# XADB 개발 문서

2026-10-29 [실제 Job page·원 bank 전이](evidence/module-independence-survey-20261026.md#row-job-images): main `21592e9`에서 Job11개 전이·원64 bank·실제 page/native/redo를 결합했다. raw26쌍·source별 신규10개 선택 검증과 최종 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 전체 M4/M5는 미완료다.

2026-10-29 [실제 RowLeaf·같은 frame Undo/Status](evidence/module-independence-survey-20261026.md#row-leaf-images): main `7baef05`에서 원 schema/anchor·Job·sealed payload와 Row 전이/이웃 보존을 연결한다. 독립 입력15쌍·source별 신규8개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-29 [Row Undo·원 Job·Status 관계](evidence/module-independence-survey-20261026.md#row-undo-images): main `09173aa`에서 Undo6개 전이·실제 H/nearest Job·같은 frame Status 연결을 구현하고 raw17쌍·신규9개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-29 [Row Status의 실제 페이지/frame 결합](evidence/module-independence-survey-20261026.md#row-status-images): main `b50e734`에서 Status9개 전이와 native/redo 관계를 구현하고 독립 literal24쌍·source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [Row scope3 frame 검산](evidence/module-independence-survey-20261026.md#row-scope3-wire): main `2fdff66`에서 별도14opcode·다중 statement core·native/redo 입구를 구현하고 source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [Row actor의 실제 페이지 관계 검산](evidence/module-independence-survey-20261026.md#row-actor-graph): 사용자의 전체 계획 완료 요청에 따라 `main` `d27bfa4`에서 재개했다. 한 actor의 Base4/Status/Undo/Row graph를 구현했고 source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [원 writable Catalog 조회](evidence/module-independence-survey-20261026.md#c3-own-catalog-read): 실제 statement·Status head·CREATE Undo를 연결해 자기 쓰기를 읽는 한정 경로의 source별 선택6개 검증과 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 사용자 지시에 따라 이 문제 해결 뒤 개발은 중지한다. 전체 M4/M5는 미완료다.

현재 모듈 개발의 재개 기준은 [2026-10-28 Catalog Commit writer 창의 main 통합](evidence/module-independence-survey-20261026.md#writer-progress-merge-main-20261028)과 [작업 계획](plans/module-independence-plan.md)이다. [앞선 C2/C3·Row 부분공급 통합](evidence/module-independence-survey-20261026.md#merge-main-20261028) 위에 리뷰한 잔여 작업 branch를 반영했으며 M4/M5 제품 인수는 미완료다. 이후 개발은 `/home/et16/work/xadb`의 `main`에서 진행한다.

2026-10-26 [DEV2 PF04 C3 shard counter 후보(S2)](evidence/dev2-pf04-c3-s2-20261026.md): thread별 shard counter를 기준선 위 diagnostics diff로 구현하고 신규8종·기존 선택 시험과 정적 lock 대조(대상 경로 counter RMW 14→0)를 통과했다. 후보는 고정했으나 제품에 반영하지 않았다. C3 측정(S3)·PF04 인수는 NOT_RUN이며 원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-26 [DEV2 PF04 측정 profile B 도구(S1)](evidence/dev2-pf04-profile-b-s1-20261026.md): controller slot gate·supervisor/환경 판정기·`slot-env-v1`을 구현하고 K0-005 계약에 등록했다. source별 선택 검증과 실제 native 연결(r1 장치 이름 실패 뒤 r2 `TOOL_CONNECTED`)을 마쳤다. C3·PF04 인수는 NOT_RUN이며 원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV4 P5 BFS 탐색기](evidence/dev4-lifecycle-explorer-20261025.md): [결정적 BFS](contracts/dev4-lifecycle-explorer.md)를 구현했다. 설계 상한은 상태 50만에서 INCONCLUSIVE이고, 사용자가 승인한 깊이 5는 깊이 4 이하 43만 상태의 6.6억 slot을 모델·checker로 판정해 불일치0으로 비웠다(archive 없음, PASS 아님). 실행 중 찾은 불일치1건은 모델을 고쳐 판정했다. FE14·깊이 6은 미완료다.

2026-10-25 [DEV2 PF04 C3·측정 profile B 계획](plans/dev2-pf04-resolution-plan.md#c3-profile-b): K0 계측 비용을 반복당 locked RMW로 설명하고, thread별 shard counter(C3)와 결과를 보지 않는 환경 타당성 판정(B)을 실행 전 계획으로 정했다. 한도·pair별 판정은 그대로다. 독립 D2 검토5회 끝에 차단0으로 채택했고 구현·측정은 NOT_RUN, 영속 처리는 결정 대기다. 원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 CPU 성능 요청 범위 제어](evidence/dev2-pf04-frequency-control-20261025.md): 도구12·driver4개 선택 검증 뒤 첫 host 연결이 즉시 readback 불일치로 실패했다. 최종234필드는 원 값과 같고 프로세스는 종료됐다. 나머지2개 미실행·격리 lease 보존·사용자 조건에 따른 중단이며 원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 원 제어기 CPU 배치](evidence/dev2-pf04-controller-placement-20261025.md): 원 e3 worker의60slot을 완료했다. CPU2/3 중앙 차이0.26–0.61%로 큰 배치 효과는 지지되지 않았고, 원 FAIL·DEV2 미완료를 보존하며 해결 조사를 계속한다.


2026-10-25 [DEV2 PF04 모드별 실행 작업·CPU 비용](evidence/dev2-pf04-mode-attribution-20261025.md): 같은 경계의40slot을 완료했다. CPU 중앙 Always+9.23%·Detailed+12.28%와 명령 수/CPI 증가를 관측했고, 원 FAIL·과거 원인 미확정·DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 warmup 뒤 관측 경계 대조](evidence/dev2-pf04-boundary-comparison-20261025.md): 디버깅10쌍·20개 정답·PMU160행을 완료했다. CPU W/C 중앙1.002127로 큰 경계 효과 UNKNOWN, 원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 warmup 전 관측 경계](evidence/dev2-pf04-prewarmup-20261025.md): 선택11함수·실제4연결을 완료했다. 새 두 모드는 같은 warmup 포함 PMU 범위로 진입 경계를 구분하며, 성능 효과 미평가·원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 사용자·커널 CPU 분리](evidence/dev2-pf04-cpu-split-20261025.md): 선택6함수·새 profile의 Off/Always2연결을 완료했다. 커널 counter와 rusage 분할을 구별하며 원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 C2 단일 쓰기 통계 후보](evidence/dev2-pf04-single-writer-20261025.md): 선택11함수·compiler2개와40개 비교 실행을 완료했다. 확인 개선 약2.92%로5% 기준에 미달해 제품에 반영하지 않았고 PF04·DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 flush 완료 구분·출력 버퍼 보완](evidence/dev2-pf04-flush-provenance-20261025.md): 수집기 변경과 source별 선택 검증을 완료했다. 원 count 불일치 FAIL을 보존하고 수정 Always의512 fsync/phase와 완료2932개를 연결했다. PF04·DEV2 미완료를 유지한다.

[DEV3 실행 범위](plans/dev3-implementation-plan.md#direction-refinement) · [구현 상태](implementation/README.md) · [최신 증거](evidence/README.md) · [이전 변경 기록](#previous-updates)

## 작업을 시작할 때

상시 기준은 [코딩 컨벤션](policies/coding-convention.md), [개발 운영 방침](policies/development-policy.md), [문서 운영 기준](policies/documentation-policy.md)입니다. 이어서 아래에서 이번 작업에 필요한 계약만 읽습니다. 자료성 문서 전체를 먼저 읽을 필요는 없습니다.

| 작업·질문 | 우선 읽을 원문 | 추가 확인 |
| --- | --- | --- |
| 현재 구조를 모듈별 독립 개발에 맞게 변경·문서 정비 | [구조 전환 작업 계획](plans/module-independence-plan.md) | [M2 설계 인계](plans/module-independence-plan.md#m2-handoff)·[소유 문서 수정 대상](plans/module-independence-plan.md#document-map)·[M0–M5와 재개 조건](plans/module-independence-plan.md#phases)·[두 제품 기능의 병렬 인수](plans/module-independence-plan.md#acceptance). 계획 작성과 실제 실행은 구분 |
| 모듈 분리 완료 뒤 추가 개발 착수 | [독립성 평가와 push·merge 강제 계획](plans/module-independence-plan.md#post-separation-gates) | 2026-10-29 사용자 지시. 변경 경계·단독/합류 증거·독립 리뷰를 적용하고 검사 도입. 현재 M4/M5의 새 선행 조건이 아니며 자동 강제는 미구현 |
| 분리된 모듈의 폴더 배치와 참조 관계 정리 | [폴더 재정리 후속 계획](plans/module-independence-plan.md#folder-reorganization) | 현재 source의 후보·순서·완료 기준과 [배치 원칙](policies/repository-build-plan.md#module-folder-layout). 참조·소유·API 경계를 먼저 대조하며 제품 이동·시험은 미실행 |
| 이름·API·오류·수명·리뷰 | [코딩 컨벤션](policies/coding-convention.md) | 해당 코드의 Rustdoc·계약 시험 |
| 공개 DBMS 소스 비교·연구 | [공개 참고 자료와 로컬 소스 경로](reference/opensource-references.md) | PostgreSQL·MariaDB·CUBRID 등 10개 checkout의 경로·소스 진입점·기록 revision |
| 결정 권한·검증 강도·의존성·복잡성 | [개발 운영 방침](policies/development-policy.md) | D0–D3·V0–V3, 별도 검토와 실제 증거 |
| 파일·crate·빌드 경계 변경 | [저장소 배치](policies/repository-build-plan.md) | [파일 소유·병렬 정책](policies/parallel-development-policy.md) |
| 다음 기능·단계 완료 판단 | [개발 단계](plans/development-stages.md) | [완료 계약](plans/stage-completion.json), [하위 계층 결정](plans/bottom-up-development.md) |
| 현재 foundation/platform·XADBTest 변경 | [구현 색인](implementation/README.md) | DEV0/DEV1 계약·현재 소스·실행 결과 |
| DEV2 커널·WAL·복구 변경 | [구현 계약](contracts/implementation-contracts.md), [DEV2 상세 설계](contracts/dev2-kernel-design.md) | [DEV2 구현·검증 계획](plans/dev2-implementation-plan.md), 해당 복구·저장 소유 계약 |
| DEV3 16개 작업의 개별 상세 명세·설계도 | [작업별 색인](contracts/dev3-task-designs.md#catalogue) | [공급 설계도](contracts/dev3-task-designs.md#dependency-map)·각 문서의 요구/상태/알고리즘/오류/자원/인수·기존 owner 정본 우선 |
| DEV3 단계별 필수 시험·구현과 함께 추가할 테스트 | [테스트 계획](plans/dev3-test-plan.md#stages) | [16개 작업별 목록](plans/dev3-test-plan.md#task-tests)·[현재 예약/취소 잔여](plans/dev3-test-plan.md#allocation-first)·[최종 전수 범위](plans/dev3-test-plan.md#final-suite) |
| DEV3 디스크 page·buffer·index 설계와 병렬 개발 | [DEV3 상세 설계](contracts/disk-storage-design.md), [구현·병렬 개발·검증 계획](plans/dev3-implementation-plan.md#parallel-development) | [WAL·undo 재설계 조사](evidence/dev3-wal-undo-research.md), [아키텍처 비교 보완](evidence/dev3-dev4-architecture-review.md), DS01–06·상태 모델·관측 행 버전·복구 예약 복원·DEV2 수신 |
| DEV4 작업 분류·개발 순서·공급/완료 조건 | [18개 세부 작업](plans/dev4-work-breakdown.md#catalogue) | [좁은 실행 단위·우선순위](plans/dev4-work-breakdown.md#sequence)·[상태 모델/통합 담당](plans/dev4-work-breakdown.md#lifecycle-model)·[시험 선택](plans/dev4-work-breakdown.md#test-scope)·[후속 조사](evidence/dev4-source-research.md#work-review-20261022); 기존 SQL/engine 정본 우선 |
| DEV4 최소 SQL·Oracle 의미·prepared 실행·embedded CLI | [DEV4 상세 설계](contracts/dev4-sql-design.md), [구현·검증 계획](plans/dev4-implementation-plan.md) | [소스·Oracle/ARIES 조사](evidence/dev4-source-research.md), [초기 처리 범위](contracts/dev4-sql-design.md#capacity-boundary), SC02·KU13·PF10·D4-V26 행 충돌·실제 DEV3 선행 증거 |
| DEV5 서버·인증/권한·세션·요청/취소 | [DEV5 상세 설계](contracts/dev5-server-design.md), [16개 분류·64개 하위 작업](plans/dev5-work-breakdown.md#catalogue), [구현·검증 계획](plans/dev5-implementation-plan.md) | [ARIES·Oracle·로컬 DBMS 분류 근거](evidence/dev5-work-breakdown-research-20261022.md), SC03·KU14·PA08·PF11·실제 Q1/C1 선행 증거 |
| DEV6 native·C ABI·JDBC·ODBC client 설계·세부 분류 | [클라이언트 상세 설계](contracts/dev6-client-design.md), [18개 분류·하위 작업](plans/dev6-work-breakdown.md#catalogue), [구현·검증 계획](plans/dev6-implementation-plan.md) | [Oracle·ARIES·로컬 소스 재검토](evidence/dev6-source-research.md#review-002), REVIEW-002·R2-V01–V06, SC04–06·D6-W01·실제 B3 선행; CLIENT1 미실행 |
| DEV7 확장 SQL·타입·PSM·LOB/spatial·외부 거래 | [36개 세부 분류](plans/dev7-work-breakdown.md), [DEV7 설계](contracts/dev7-sql-extensions-design.md), [구현·검증 계획](plans/dev7-implementation-plan.md) | [Oracle·ARIES·로컬 소스 재검토](evidence/dev7-review-002.md), DESIGN-002 비교 문맥/CALL/XA·SC07–11·64개 요구·F01–10; SQL_EXT1 미실행 |
| DEV8 복제·백업/PITR·보안/운영·성능 | [DEV8 설계](contracts/dev8-operations-design.md), [구현·검증 계획](plans/dev8-implementation-plan.md) | [Oracle·ARIES·로컬 소스 2차 검토](evidence/dev8-review-002.md), DESIGN-002 domain/sequence/partition/control·R2-V01–04·SC12–15·30개 요구·실제 S1/B3 선행; OPS1 미실행 |
| DEV9 제품 기능·운영·성능 인수와 설치/문서 | [DEV9 설계](contracts/dev9-qualification-design.md), [구현·검증 계획](plans/dev9-implementation-plan.md) | [Oracle·ARIES·로컬 소스 재검토](evidence/dev9-source-research.md#review-002), DESIGN-002 실행 분모/ACK 관측/PITR/배포·R2-V01–04·SC16/17·104개 요구; PRODUCT_QUALIFIED 미실행 |
| COMM frame·socket·backpressure·취소 설계 | [COMM 상세 설계](contracts/comm-design.md), [구현·검증 계획](plans/comm-implementation-plan.md) | [소스 조사·검토](evidence/comm-source-research.md), [현재 구현·잔여 인수](implementation/comm-implementation.md), C1·KU12·DEV1 선행 증거; SQL 의미는 DEV5 |
| 성능 계측·원인 분석·개선 | [AI 성능 개발 계약](contracts/ai-performance-engineering.md) | DP07, 해당 단계 PF 인수 의무 |
| 기능 명세·설계를 작성할 대상 | [작성 대상 목록](plans/feature-design-inventory.md) | [원문 이름 색인](reference/feature-design-leaf-index.md)과 해당 소유 계약 |
| 프로젝트 목표·AI 개발 실험 | [제품 방향](plans/project-direction.md) | 현재 구현과 최종 목표, 공개 DBMS 참고 방향을 구분 |
| 상용 DBMS 수준으로의 확장 가능성 | [기술 평가](reference/commercial-dbms-extensibility-assessment.md) | [Oracle·로컬 소스 조사와 선제 보완](evidence/commercial-extensibility-research.md), 현재 구현·채택 설계·미검증 과제와 재평가 조건; 제품 인수 증거 아님 |
| SQL·CLI·매뉴얼의 상세 제안 | [참고 자료 색인](reference/README.md) | 제안 상태를 확인하고 채택 계약과 구별 |
| 버전·형식 호환 | [버전 정책](policies/version-compatibility-policy.json) | 사내 새 DB와 안정 릴리스 기준선을 구별 |
| 증거·과거 조사 확인 | [증거 색인](evidence/README.md), [역사 자료](history/README.md) | source/revision/hash·실행 범위 |

## 문서의 역할

| 폴더 | 용도 |
| --- | --- |
| [policies](policies/README.md) | 자주 적용하는 규칙과 작업 절차 |
| [plans](plans/README.md) | 제품 범위·개발 순서·완료 조건·작성 목록 |
| [contracts](contracts/README.md) | 해당 기능 변경 때 적용하는 의미·불변식·인수 계약 |
| [implementation](implementation/README.md) | 실제 구현 범위·제약·실행 진입점 |
| [reference](reference/README.md) | 필요한 절만 찾아보는 종합 설명·초안·이름 색인 |
| [examples](examples/README.md) | 실행·승인 기록이 아닌 예시 입력 |
| [evidence](evidence/README.md) | 조사·모델·실행 결과의 원본 증거 |
| [history](history/README.md) | 이관 결정과 과거 판단의 적용 범위 |

같은 주제에서는 채택된 소유 계약을 먼저 따릅니다. 폴더 위치나 작성일만으로 승인·구현·시험 완료를 판단하지 않습니다. 자세한 우선순위와 상태 구분은 [DP02](policies/development-policy.md#section-3)에 있습니다. Linux·Rust, 하위 계층부터의 개발, 기능 보존, 사내 새 DB, 안정 릴리스부터의 호환 계약은 유지합니다.

## 문서를 변경할 때

원문·관련 JSON·폴더 색인·[문서 ID 색인](documents.json)을 함께 갱신합니다. 새 문서의 위치·상태·참조 규칙은 [문서 운영 기준](policies/documentation-policy.md)을 따릅니다.

문서 검사는 **push 직전** 훅에서 수행합니다. 편집·commit 때 반복 실행하지 않습니다. 훅은 전송할 각 ref의 최종 commit의 임시 사본에서 Rust 포맷, Actions와 같은 Clippy, 문서 검사를 순서대로 실행하고 하나라도 실패하면 push를 차단합니다. Clippy는 테스트 코드를 정적 검사하며 기능 시험은 실행하지 않습니다. 명령·설치·범위는 [push 검사 규칙](policies/documentation-policy.md#push-gate)을 따릅니다.

이 명령은 문서·계약·증거 정합성을 검사하며 제품 시험을 실행하지 않습니다. [저장된 결과](document-validation.json)는 해당 실행 당시 결과이며 현재 문서나 DB의 검증 완료로 해석하지 않습니다. 과거 측정 스크립트를 문서 정리 목적으로 다시 실행하지 않습니다.

기존 개발 파일을 새 XADB 저장소로 이관했으며 이전 Git 이력과 과거 시험 결과는 가져오지 않았습니다. 이번 이관의 경로·증거 처리 기록은 [이관 기록](document-migration.json)에 있습니다. [제품 저장소 안내](../README.md)

<a id="previous-updates"></a>
## 이전 변경 기록

2026-10-25 [DEV4 P5 독립 edge checker](evidence/dev4-lifecycle-checker-20261025.md): 모델을 호출하지 않는 [`verify_edge`](contracts/dev4-lifecycle-checker.md)를 구현하고 모델 출력을 차등 판정했다. 불일치9건·검토 결정2건을 원문으로 판정해 [모델 계약](contracts/dev4-lifecycle-model.md#checker-adjudication)에 남기고 조립기 공백1건을 고쳤다. 독립 D2 수용·D1 제한 수용이며 alias·BFS·FE14는 미완료다.

2026-10-25 [DEV4 P5 수명 모델 전이](evidence/dev4-lifecycle-model-20261025.md): [소유 계약](contracts/dev4-lifecycle-model.md)의 `transition`을 구현해 원 literal 29 edge·12 trace·5 known-bad와 맞췄다. 1차 독립 검토가 찾은 금지 전이 2건을 교정했고 독립 D2 수용·D1 제한 수용이다. 독립 checker·BFS·FE14는 미완료다.

2026-10-25 [DEV4 P5 전체 입력 조립](evidence/dev4-lifecycle-assembler-20261025.md): [소유 계약](contracts/dev4-lifecycle-assembler.md)의 `assemble_input`을 구현하고 신규 입력1994개를 선택 검증했다. 독립 supplement32·rich 후보 정정2건과 개발 중 불일치 교정 이력을 보존하며 모델 전이·FE14는 미완료다.

2026-10-25 [DEV3 비교 분석 반영](evidence/dev3-comparison-adoption-20261025.md): [설계 보강](contracts/disk-storage-design.md#task-review-011)·[완료분 보완 계획](plans/dev3-implementation-plan.md#comparison-adoption)·[추가 인수](plans/dev3-test-plan.md#comparison-adoption). 제품·시험 미실행, 기존 완료 증거 보존.

2026-10-25 [DEV2 PF04 P3 영속 I/O 관측](evidence/dev2-pf04-io-cause-20261025.md): 새 도구·선택 검증과4slot 정답 검산을 완료했다. 두 추적의512 fsync와 journal wait를 연결했으며 요청 전체 비율은 미확정, Always 영향 screen FAIL·원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 P2 K0 소유권 후보 기각](evidence/dev2-pf04-k0-owner-20261025.md): 계획 commit 뒤 C1을 구현·선택 검증했으나 확인10쌍의 개선 약2.46%가 동결 기준에 미달해 제품을 원복했다. 후보·원자료를 보존하며 원 PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV3 중단된 plan4 복구 단위 재개](evidence/dev3-recovery-resumed-20261025.md): 필수50개 source별 PASS, 완료 제품 ID 반복0, 원 실패 보존. 복구 owner의 같은 pool 인계와 실제3PID 재진입을 검증했으며 최종 독립 D1을 한정 수용했다. 전체 DEV3/S5는 미완료다.

2026-10-25 [DEV2 PF04 실제 PMU 수집 후속](evidence/dev2-pf04-pmu-20261025.md): 승인된 CAP_PERFMON 경로로 실제6개 counter 수집과 권한·중단 정리를 선택 검증했다. worker의 일반 사용자 상태와 기존 실패를 보존하며 PF04/DEV2는 미완료다.

2026-10-25 [DEV2 PF04 P1 관측 도구](evidence/dev2-pf04-window-20261025.md): 계획 선행 commit 뒤 측정 경계·bounded runner를 구현하고 변경 범위만 선택 검증했다. 원 실패·당시 일반 사용자 PMU UNAVAILABLE 증거와 PF04/DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 해결 단계와 커밋 계획](plans/dev2-pf04-resolution-plan.md): 사용자 요청에 따라 계획을 먼저 commit하고 P1 측정 구간 관측 도구부터 시작한다. 단계별 변경 검증·독립 검토·commit을 구분하며 원 실패와 DEV2 미완료를 유지한다.

2026-10-25 [DEV2 PF04 원인 분석과 해결 후보](evidence/dev2-pf04-analysis-20261025.md): 보존 K0·영속 원자료를 재분석해 계측 비용과 실행 간 변동을 구분했다. 원인 관측·K0 소유권 개선·조건부 WAL 후보를 제안하며 제품 변경·새 시험0, PF04 FAIL·DEV2 미완료를 유지한다.

2026-10-25 [DEV3 복구 작업 중간 저장·자료 정리](evidence/dev3-recovery-paused-20261025.md): 사용자 요청으로 진행을 중지했다. 원 static2 FAIL·F05 HOLD와 최신 source NOT_RUN을 보존하며 복구·DEV3/S5는 미완료다.

2026-10-25 [DEV4 P5 전체 입력 조립 준비](evidence/dev4-lifecycle-assembly-inputs-20261025.md): 설계와 독립 후보1658개를 정적 한정 검토했다. 미검토 rich304를 보존하며 전체 D2·제품·시험은 NOT_RUN이다. 사용자 정리 요청에 따라 준비 단위를 기록한다.

2026-10-25 [DEV3 plan4 빈 거래 live·prefix](evidence/dev3-retention-live-20261025.md): 신규 필수75개 source별 PASS·완료 반복0, 원 실패 보존, 최종 독립 D1을 제한 수용했다. fresh 수명·flush·재적재를 연결했으며 복구 owner·native 수명·DEV3/S5는 미완료다.

2026-10-25 [DEV4 P5 원시 번호·selector](evidence/dev4-lifecycle-raw-selection-20261025.md): raw1533과 exact target 선택을 구현하고 신규1951입력·3484호출을 선택 검증했다. compile·Clippy·fmt PASS, 정적 집계 원 실패를 보존하며 최종 독립 D1을 제한 수용했다. full input·모델·FE14는 미완료다.

2026-10-24 [DEV3 복구 이미지·제어 레코드](evidence/dev3-recovery-control-base-20261024.md): 신규 필수272개를 source별 선택 검증했다. 원 fmt·Clippy 실패와 별도 live HOLD/FAIL을 보존하며 최종 독립 D1을 순수 codec 범위로 제한 수용했다. 실제 live·새 epoch 복구·DEV3/S5는 미완료다.

2026-10-24 [DEV4 P5 상태·입력 codec](evidence/dev4-lifecycle-codec-20261024.md): bounded 수명 자료형·canonical byte와 독립 reader를 구현하고 source별 선택 검증했다. 원 compile·ParkedFetch 실행·fmt 실패를 보존하며 최종 독립 D1을 제한 수용했다. 전체 모델·BFS·FE14·실제 engine/Q1은 미완료다.

2026-10-24 [DEV4 P5 독립 입력 준비](evidence/dev4-lifecycle-inputs-20261024.md): 12 trace·51 action과 state4/input7 byte vector를 독립 작성·정적 검토했다. 전체 D2·모델 구현·시험은 NOT_RUN이며 실제 engine·Q1은 미완료다.

2026-10-24 [DEV3 빈 거래·정리 큐 순수 의미 처리](evidence/dev3-empty-retention-semantics-20261024.md): Root/Job/Aging·tag3 인계의 필수140을 source별 선택 검증했다. 원 생성·시험2·Clippy 실패와 완료 반복0을 보존하며 최종 독립 D1을 제한 범위로 수용했다. 실제 live/flush/복구·DEV3/S5는 미완료다.

2026-10-24 [COMM slow-peer 실패 수정](evidence/comm-failure-fix-20261024.md): Unix prefix 정체와 TCP 시험 부하의 압력 미도달을 보완했다. source별 두 실패 조합이 각각5쌍을 통과했고 관련 단위10개 PASS다. 원 실패·중간 후보의 실패를 보존하며 최종 독립 D1/D2를 제한 범위로 수용했다. 전체 COMM/C1은 미완료다.

2026-10-24 [DEV3 원 WAL runtime/core 추출](evidence/dev3-runtime-core-20261024.md): 원 자원과 Fresh provenance를 유지하는 내부 구조·전달 함수를 연결했다. 신규 static2·memory5 PASS, 원 compile 실패 보존, 최종 독립 D1 제한 수용이며 G6 live/복구·DEV3/S5는 미완료다.

2026-10-24 [DEV4 P6 실제 공급 수합](evidence/dev4-supply-handoff-20261024.md): DEV3 초기 page clean 소비와 빈 Catalog/Free 큐 census의 제한 공급을 공개 SQL 소비 API와 구분했다. P1–P4 원 증거·API 차이·후속 연결 의무를 독립 사실 검토로 수합했으며 최소 disk 공급·실제 engine 연결·Q1은 미충족이다. 제품 변경·시험 실행0이며 P5 수명 모델은 설계 중이다.

2026-10-24 [DEV4 나머지 raw 문장 parser](evidence/dev4-remaining-parser-20261024.md): SELECT/UPDATE/DELETE·CREATE/DROP TABLE/INDEX를 기존 flat AST·원문·marker에 연결했다. 보존 source의 semantic8함수(8,980parse·known-bad50/통제12)와 자원13함수·Clippy를 선택 검증했다. 원 M21 compile·fmt FAIL을 보존하고21제품 변조 거절·최종 독립 D1을 제한 수용했다. P5/P6·typed 문장 binding·실제 engine·Q1은 미완료다.

2026-10-24 [DEV3 plan4 초기 page의 원 공유 버퍼 소비](evidence/dev3-plan4-clean-owner-20261024.md): 원 CAS와 한 SharedPool의 clean load·Catalog3pin·정리 큐4/4/3pin을 구현했다. 필수78의 source별 결과와 원 실패·HOLD를 보존하며 최종 독립 D1을 제한 수용했다. dirty/flush·복구 owner·CREATE/CLR·DEV3/S5는 미완료다.

2026-10-24 [DEV4 typed predicate](evidence/dev4-typed-predicate-20261024.md): 명시 타입의 전체 검산·3값 평가와 독립 소유/자원을 구현했다. 원 compile·fixture FAIL, 완료 prefix 재실행 없는 source별 선택 증거와18제품 변조 거절을 보존한다. 최종 독립 D1을 제한 수용했으며 P4·수명 모델·실제 engine 통합·Q1은 미완료다.

2026-10-24 [DEV3 plan4 정리 큐·빈 Catalog 최초 설치](evidence/dev3-plan4-genesis-20261024.md): 원 FreshStorageSession·DWB의9page 설치와 private genesis-only 선택을 연결했다. 필수304·compiler9·checker15 및 같은 ELF의 실제3PID 인수를 보존하며 원 실패·보완을 구별한다. 최종 독립 D1을 제한 수용했고 live 큐·Catalog CREATE/CLR·전체 DEV3/S5는 미완료다.


2026-10-24 [DEV4 predicate AST/parser](evidence/dev4-predicate-20261024.md): 조건식 우선순위·flat AST·깊이/자원/제어를 구현했다. 원 stack overflow와 Clippy FAIL을 보존하고 source별 필수15함수·6제품 변조 거절을 확인했다. 최종 독립 D1 제한 수용이며 typed compile/eval·3값 논리·나머지 문법·모델·실제 engine 통합·Q1은 미완료다.

2026-10-24 [DEV4 scalar 입력·typed 값 소유](evidence/dev4-scalar-20261024.md): 원래 타입·SQL/native empty 경계·bounded decode와 shared quota를 구현했다. 신규23함수·8,929semantic·필수11범주/12제품 변조 거절과 source별 증거를 보존한다. 최종 독립 D1 제한 수용이며 predicate·나머지 문법·모델·실제 engine 통합·Q1은 미완료다.

2026-10-24 [DEV3 Catalog typed Undo before](evidence/dev3-catalog-before-20261024.md): XCB1과 typed accessor를 구현하고 필수123을 선택 검증했다. 원 Clippy FAIL·수집기 부분 판정을 보존하며 최종 독립 D1 제한 수용, 실제 Catalog/첫 행·전체 DEV3/S5는 미완료다.

2026-10-24 [DEV4 owned AST·INSERT/거래 구문](evidence/dev4-parser-20261024.md): 원문·flat AST의 shared quota 소유와 단일 문장/PUBLIC/marker를 구현했다. 신규30함수·semantic258·known-bad23과 source별 제품 mutant 거절·원 Clippy compile FAIL을 보존한다. 값/predicate·나머지 문법·수명 모델·실제 engine 통합·Q1은 미완료다.

2026-10-24 [DEV4 streaming 문장 경계·원문 예약](evidence/dev4-splitter-20261024.md): chunk/EOF·UTF-8 경계와 shared quota 원문 소유를 구현했다. 새25함수·1,226schedule·관측변조16·제품mutant4의 선택 증거와 원 Clippy compile FAIL을 보존한다. parser/값·모델·실제 engine 통합·Q1은 미완료다.

2026-10-24 [DEV3 혼합 Status·Free Undo owner](evidence/dev3-mixed-system-owner-20261024.md): 원 pool의 INIT·pressure·flush·load와 닫힌 mixed 복구를 연결했다. 메모리 필수144·static3·compiler9+baseline PASS를 보존한다. 실제 producer PASS와 원 복구 시험 기대값 FAIL을 보존하고 제품 소스가 같은 r7의 복구·재진입을 통과했다. 최종 독립 D1은 제한 범위로 수용했으며 원 동일 ELF 3단계 요건·일반 pool/live undo·DEV3/S5는 미완료다. AI 완료 소비자1/3을 유지한다.

2026-10-24 [DEV4 독립 lexer·이름](implementation/dev4-implementation.md): source를 빌리는 앞단·고정 이름·호출 제어와 독립 literal 판정기를 구현했다. [source별 선택 증거](evidence/dev4-frontend-20261024.md)에 원 compile FAIL·이름 제어 반례·수정 및 mutant 거절을 보존하며 최종 독립 D1을 제한 수용했다. splitter/parser/값·실제 engine 통합·Q1은 미완료다.

2026-10-24 [DEV4 독립 앞단 계약](contracts/dev4-independent-frontend.md)·[DEV3 병행 준비 계획](plans/dev4-preparation-plan.md): Oracle19c 공식/로컬 매뉴얼과4개 DBMS를 대조해 lexer/parser·값 처리·독립 검증의 선행 범위와 FE01–16을 구체화했다. [후속 리뷰 교정](evidence/dev4-preparation-research-20261024.md#review-followup)으로 순수 진단과 호출 전체의 거래/정리 효과를 분리하고 Oracle 끝 공백 차이·연결 시험 의무를 보강했다. D4-PREP-001/002 독립 D2 채택, 실제 engine 통합·Q1의 DEV3 선행과 제품·시험 NOT_RUN을 유지한다.

2026-10-24 [DEV3 혼합 Status·Free Undo INIT 준비](evidence/dev3-mixed-system-init-20261024.md): plan3 역할을 빌리는 sealed 준비와 mixed 예약 census를 연결했다. 독립 D2·source별246case·compiler6+baseline·checker6 PASS, 원 실패/중복2 범위 일탈 보존·최종 D1 제한 수용이며 실제 mixed pool/복구·DEV3/S5는 미완료다.

2026-10-24 [DEV3 Free Undo 초기 page](evidence/dev3-undo-free-page-20261024.md): target 없는 Free codec과 one-slot 초기 page 표현을 연결했다. 독립 D2·compile/fmt/Clippy·관련7함수987case PASS이며 최종 독립 D1을 제한 수용했다. 실제 INIT·mixed pool/복구·DEV3/S5는 미완료다.

2026-10-24 [DEV3 Undo bootstrap 역할](evidence/dev3-undo-bootstrap-20261024.md): plan3의 Status2/Undo3 불변 역할을 추가했다. 신규33assertion과 원 compile/Clippy FAIL을 보존하며 native1·compiler4+baseline·checker7을 검증했고 최종 독립 D1을 제한 단위로 수용했다. S5 추가78은 작성·NOT_RUN, mixed pool/live undo·plan3 복구·전체 DEV3/S5는 미완료다.

2026-10-24 [DEV3 Status 상주 후보 선택](evidence/dev3-status-pressure-20261024.md): 원 pool에서 자동 후보→flush/load와 같은 actor 계속을 연결했다. source별 신규33scenario·static3·checker7을 보존한다. native oracle 원 FAIL과 실제3PID의 source별 결과를 보존하며 최종 독립 D1을 제한 단위로 수용했다. S5 확대438은 작성·NOT_RUN, 일반 pool/live undo·전체 DEV3/S5는 미완료다.

2026-10-23 [DEV3 빈 거래 통합 작업](evidence/dev3-empty-transaction-20261023.md): AI 효율 계획의 입력 묶음·독립 fixture 준비·시간 기록을 적용한다. 빈 AbortEnd 준비·원 Status owner 인계와 실제 Begin/RollbackStart/AbortEnd의 WAL·RAM 설치를 선택 검증했다. WAL 이력에 결합한 live flush·최신 page 재적재와 새 process 복구/재진입을 선택 검증하고 제한 단위의 독립 D1을 수용했다. AI 운영 실험의 소비자1/3에 도달했으며 효과 비교·일반 pool/live undo·전체 DEV3/S5는 미완료다.

2026-10-23 [DEV3 여러 Status의 실제 상주 수명](evidence/dev3-resident-status-20261023.md): 기존 SharedPool에서3frame/4주소 INIT·read·flush·퇴출·load와 별도 현재-build 복구/재진입을 연결했다. 신규31·직접 관련4·static2·compiler9·checker8·native1의 source별 결과와 원 FAIL을 보존한다. 최종 독립 D1을 제한 단위로 수용했으며 일반 pool·live 거래·전체 DEV3/S5는 미완료다.

2026-10-23 [DEV3 AI 개발 효율 개선 계획](plans/dev3-ai-throughput-improvement.md): AI가 검증된 기능을 더 빨리 전달하도록 작업 입력·역할 분담·시험 자동화를 개선하고 세 통합 단위에서 효과를 측정하는 실험을 제안한다. 첫 빈 거래 통합에서 운영 적용·계측을 시작했으며 효과 비교와 DEV3/S5는 미완료다.

2026-10-23 [DEV3 전체 대상 pin 집합](evidence/dev3-target-set-20261023.md): 기존 단일 SharedPool에서 정확1..64개 대상을 함께 확보하도록 확장했다. 신규15·직접 관련4·static2·compiler7·검사기3/변경1의 source별 PASS와 원 compile FAIL을 보존한다. 최종 독립 D1을 제한 단위로 수용했으며 일반 pool·live 거래·DEV3/S5는 미완료다.

2026-10-23 [DEV3 최초 Begin 준비](evidence/dev3-status-begin-20261023.md): Free 슬롯의 빈 Active 전이를 위한 WAL·페이지 producer/observer/materializer를 추가했다. 새 runtime12·static1·fmt/Clippy PASS와 원 fmt FAIL을 보존하며 최종 독립 D1을 순수 준비 단위로 수용했다. 실제 발급·stage/apply·일반 pool·DEV3/S5는 미완료다.

2026-10-23 [DEV3 현재 membership Status load](evidence/dev3-status-load-20261023.md): dirty allocator의 권한을 보존하며 실제 A/B home을 두 번째 frame에 재적재한다. source별 runtime24·native1·compiler3+baseline·자원/체커와 원 FAIL을 보존하며 최종 독립 D1을 이번 제한 load 단위로 수용했다. 일반 pool·live Status 변경·DEV3/S5는 미완료다.

2026-10-23 [DEV3 반복 Status 이력 복구](evidence/dev3-operational-recovery-20261023.md): 첫 extent의 주소별 canonical census·Pending 수리·REDO와 fresh producer/reentry를 연결했다. source별 신규65·관련3·native4/14worker·compiler3+control1의 선택 결과와 원 FAIL을 보존한다. OCR delegate 최종 독립 D1을 제한 복구 단위로 수용했으며 P3·일반 pool·DEV3/S5는 미완료다.

2026-10-23 [DEV3 반복 Status INIT owner](evidence/dev3-operational-status-20261023.md): 원 flush bound의 consuming 전환·clean Status 퇴출·반복 INIT을 공급했다. source별 static6·신규 runtime14·관련1·native1과 원 compile FAIL을 보존하며 OCR delegate 독립 D1을 live owner 한정 수용했다. 반복 sequence 복구·P3·DEV3/S5는 미완료다.

2026-10-23 [DEV3 일반 page 등록 코어 P1](evidence/dev3-operational-registration-20261023.md): 슬롯별 generation/publication으로 기존 SharedPool을 정리했다. static5·runtime20·Linux2의 source별 증거와 원 compile FAIL을 보존한다. OCR delegate 기반 독립 D1 수용, P2/P3·일반 pool·DEV3/S5 미완료.

2026-10-23 [DEV3 allocator 실제 퇴출·재적재](evidence/dev3-allocator-eviction-20261023.md): 비상주 guard와 다른 frame의 actual home load를 연결했다. 신규 runtime18·관련7·static5·native1·compiler3+control1의 source별 PASS를 보존한다. OCR delegate를 활용한 최종 독립 D1 수용이며 일반 pool·전체 DEV3/S5는 미완료다.

2026-10-23 [DEV3 진행 현황 후속 리뷰](evidence/dev3-progress-review-20261023.md): 최신 reload 구현·시험 도구와 snapshot 수정, 단계별 공급·미완료 인수를 대조했다. 요약 표·단계 JSON·색인의 진행 설명을 최신 제한 구현과 맞췄으며 제품 변경·시험 재실행·완료 승격은 없다.

2026-10-23 [COMM 시간 제한·조합별 성능 인수](evidence/comm-performance-batch-20261023.md): 별도 미완료 시험의 원 profile·10쌍·측정·2,700분모를 유지하며 같은 보존 source18조합360mode의 BATCH_PASS와 원자료·통계·누적목록의 독립 수용을 확인했다. 21:55:52 KST 선택 실행 종료 뒤 다음119는원보수3,320초>가용2,908.24초로 SKIPPED_TIME_BUDGET·실행0을 보존한다. slow1의 처리량/p99 미적용과 quota·역압·취소/정리 의무를 구분하며 원보수식/품질기준과23시 hard마감은 유지한다. 모델 원 FAIL·oracle/exact dedup 선택 수정·중단과 새6시간 모델의 시간상 skip을 보존한다. 다른 source의 원74를 합산하지 않고 완료 시험 재실행 금지·8시간 skip/23시 마감·C1 INCOMPLETE를 유지한다. 후속 사용자 지시로 [신규 성능5+5 계약](plans/comm-implementation-plan.md#adaptive-repetitions)을 적용했다. 기본5쌍·편차 시 추가5쌍이며 선택 checker14개 PASS이며22:47 채택 시점의 native는 NOT_RUN으로 기록했다. 과거10쌍 결과는 소급 재분류하지 않는다. 이후 한 시간 추가 요청으로 이번 후속 실행 deadline을23:50·기록 목표23:55 KST로 연장했다. 추가 native는23:41 종료했으며 신규 profile6조합60mode BATCH_PASS,부분5mode·실패2mode·시간상91 SKIP을 구분해 보존한다. 통과6조합은모두5쌍이며추가5쌍은없다. 전체C1은미완료다.

2026-10-23 [DEV3 구현 코드 리뷰·후속 수정](evidence/dev3-code-review-20261023.md): `open-code-review-delegate`로 제품 코어51파일에서 확인한 high 결함 D3-OCR-001을 수정했다. 영속화 후 snapshot 실패의 오류 효과·fence를 보완했으며 신규 관련5건 PASS·별도 D1 수용이다. 최초 정적 리뷰와 수정 전 FAIL을 보존하고 전체 DEV3/S5 미완료를 유지한다.

2026-10-23 [COMM 미완료 인수 재개](evidence/comm-acceptance-resume-20261023.md): 기존30분 fuzz/stress·11 mutant의 원 완료 증거를 재실행 없이 대조하고 실제 RX-prefix 확인 뒤 process-kill 신규36셀과 source별 변경 checker/reader를 선택 검증했다. D1/D2 독립 검토 수용이며 모델·전체 native fault·성능과 C1은 미완료다.

2026-10-23 [DEV3 고정 등록 코어·세대 pin](implementation/dev3-implementation.md#registration-core): 기존 dirty pool의 단일 lookup·Copy 요청·세대 token 검산을 연결했다. [선택 증거](evidence/dev3-registration-core-20261023.md)는 r6 신규11·관련10·native1·compiler2+control1과 source별 정적 회계를 구별한다. 제한 단위 독립 D1 수용, 일반 pool·DEV3/S5 미완료다.

2026-10-23 [DEV3 현재 genesis의 실제 닫힌 복구](implementation/dev3-implementation.md#recovery-effects): 고정 두 슬롯 provisioning·source 안정화·historical Pending 수리·예약 한 번 인계·page별 REDO를 연결했다. [source별 검증](evidence/dev3-recovery-effects-20261023.md)은 필수393의 완결 job386·실패 job 내 완료 단언7과 native31/95worker·compiler9+control1·checker30/변경1을 구별한다. 제한 단위 최종 독립 D1을 수용했으며 일반 복구/READY·전체 DEV3/S5는 미완료다.

2026-10-22 [DEV3 현행 WAL prefix 분석](implementation/dev3-implementation.md#recovery-prefix): 실제 reserve/cancel·Status INIT의 canonical RAM 상태와 잔여 의무를 닫힌 owner에 재구성했다. [source별 검증](evidence/dev3-recovery-prefix-20261022.md)은 새 unit231의 완료 단언(완결 job223·실패 job 내8), native31·compiler8+control1·checker15·변경 수신6을 구별한다. 제한 단위의 최종 독립 D1을 수용했으며 실제 Pending 수리·물리 REDO·전체 DEV3/S5는 남아 있다.

2026-10-22 [DEV3 첫 복구 입력 선택](implementation/dev3-implementation.md#recovery-select): 초기 FILEMAP·Empty DWB·genesis header-only WAL의 [닫힌 선택](contracts/dev3-recovery-select.md)을 구현하고 [필수170·관련4 재검증·native8·compiler5+control1·checker15](evidence/dev3-recovery-select-20261022.md)와 독립 D1을 수용했다. 원 FAIL/source별 결과를 보존하며 nonempty 복원·BOOT_RESERVE·일반 recovery/READY·최종 S5는 남아 있다.

2026-10-22 [DEV3 개발 모델·독립 검토 운영](plans/dev3-implementation-plan.md#model-roles): GPT-6.1 Sol을 주 개발 모델로 사용하고 핵심 복구·동시성 설계는 가능하면 Astra가 별도 검토한다. 첫 평가 단위와 판단 기준을 명시하며 기존 D1/D2 검토·변경 코드 시험·S5 전수 인수는 유지한다. 모델 비교 실행·제품 개발 재개·앱 설정 변경은 없다.

2026-10-22 [DEV3 진행 방향·설계 점검과 반영 추적](evidence/dev3-direction-review-20261022.md#adoption-followup): 기본 저장 구조를 유지하고 제한 복구→일반 buffer/live undo→첫 행 거래 순으로 연결한다. [후속 결정](plans/dev3-implementation-plan.md#direction-review-adoption)·소유 계약·공급도·시험 배정에 완료 조건과64GiB/계측 준비를 반영했다. 제품 구현·새 시험 실행은 없다.

2026-10-22 [DEV3 작은 단위 종료·인계](implementation/dev3-implementation.md#paused-after-status-home-flush): Status home flush까지 구현·선택 인수·commit을 완료했다. 복구 진입은 사전 열람만 진행했으며 새 코드·확정 설계·시험은 없다. 사용자 요청에 따라 기존 작업을 정리하고 새 하위 작업은 시작하지 않는다. 전체 DEV3·최종 S5는 미완료다.

2026-10-22 [DEV3 Status home flush](implementation/dev3-implementation.md#status-home-flush): 실제 두 target의 순차 DWB/home·whole retirement·조건부 finalize를 연결했다. 필수43·compiler6·native3의 source별 증거와 원 FAIL을 보존하며 제한 단위의 최종 독립 D1을 수용했다. 일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 폐쇄 두-frame Status INIT](implementation/dev3-implementation.md#status-init): 실제 reserve→두 RAM image 공개→동일 WAL requirement의 durable 확인과 두-pin reader를 연결했다. 필수93·shared1·compiler13·native3의 source별 결과와 원 FAIL을 보존한다. 제한 단위의 최종 독립 D1을 수용했으며 Status home flush·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 불변 Status bootstrap](implementation/dev3-implementation.md#status-bootstrap): plan2 영속 역할과 typed borrow·old adapter 거절을 구현했다. 새 필수32·변경 parser1·native1·compiler semantic3와 별도 control1을 검증했다. 제한 단위의 최종 독립 검토를 수용했으며 INIT·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 allocator 한 페이지 flush](implementation/dev3-implementation.md#allocator-flush): 실제 capture·WAL force·DWB/home·whole retirement·조건부 RAM finalize를 연결했다. 필수76의 source별 범위·native3·compiler12·WAL30과 자원 전제를 검증했으며 원 FAIL/관측 불완전을 보존한다. 제한 단위의 독립 검토를 수용했으며 INIT·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 첫 dirty frame·stage 소비자](implementation/dev3-implementation.md#dirty-stage): 실제 pin/content·DPT·first recLSN과 guarded reserve/cancel을 공급했다. mandatory53의 source별 범위·compiler18·native3 및 실패 원본을 보존했고, INIT·flush·일반 recovery와 전체 DEV3는 미완료다.

2026-10-22 [DEV4 분배 재점검 003](plans/dev4-work-breakdown.md#work-review-003): 18개 책임을 유지하면서 초기 INSERT의 parser·dependency 선행을 좁히고 공통 오류 변환·독립 모델·관측/ACK 담당을 구체화했다. [18개 상대 분량과 첫 종료점](plans/dev4-work-breakdown.md#workload-review), [ARIES·Oracle·로컬5개 DBMS 근거](evidence/dev4-source-research.md#work-review-003-research)를 연결한다. 제품 구현·시험과 Q1 승격은 없다.

2026-10-22 [DEV7 분배 후속 검토](evidence/dev7-work-breakdown-review-20261022-r2.md): 36개 분류를 유지하고 제약 설치/cascade·queue 재시도·scheduler worker·온라인 복구를 **140개 하위 작업**으로 보완했다. [배정 크기·우선순위·실행 비용](plans/dev7-work-breakdown.md#allocation)과 native sequence/queue의 독립 공급을 명시한다. 제품 구현·시험과 logging 보류는 유지한다.

2026-10-22 [DEV7 작업 분배 검토·보완](evidence/dev7-work-breakdown-review-20261022.md): ARIES·Oracle·로컬8개 DBMS를 대조해 36개 분류를 유지하고 하위 작업을115개에서135개로 나눴다. [공급·인수 연결](plans/dev7-work-breakdown.md#leaf-handoffs), 저장/XA 위험의 선행 준비·증분 refresh·성능 workload 한도를 보완했다. 제품 개발·시험 재개와 logging 보류 해제는 없다.

2026-10-22 [DEV4 분배·우선순위 보완](plans/dev4-work-breakdown.md#work-review-002): 기존18개 분류·55개 ID를 유지하고 INSERT/대상 조회·정렬 run/merge/cleanup·R1–R4·PF10 각 군을 실행 단위로 나눴다. 작은 commit/reopen·rollback 경로를 먼저 연결하며 상태 모델·수명 통합 담당과 sorter 재사용 판단을 배정했다. [ARIES·Oracle·로컬5개 DBMS 후속 근거](evidence/dev4-source-research.md#work-review-20261022)를 연결하며 제품 구현·시험·Q1 승격은 없다.

2026-10-22 [DEV3 테스트 계획 항목별 재점검](evidence/dev3-test-plan-review-20261022.md):16개 작업의 중간 필수 경계·누락/중복 검출·공동 실행과 준비 비용을 보완했다. [개선 계획](plans/dev3-test-plan.md#item-review)은 S5 DEV3 전수·기존 budget을 유지하며 제품 개발·시험은 재개하지 않는다.

2026-10-22 [DEV3 16개 설계 2차 비교 리뷰](evidence/dev3-task-design-review-20261022-r2.md): ARIES·Oracle·로컬6개 DBMS를 대조하여 Commit 의미 설치 순서·offline build drain·backup 최종 WAL cut을 보완했다. 이전 리뷰와 실행 결과는 보존하며 제품 구현·시험 실행은 없다.

2026-10-22 [DEV7 36개 세부 작업 분류](plans/dev7-work-breakdown.md): ARIES·Oracle·로컬8개 DBMS 소스를 참고하여 기존 큰 묶음을 하위 배정 단위·직접 선행·산출물·완료/인수로 나누고 64개 요구에 연결했다. 제품 구현·시험은 재개하지 않았으며 F01–F10·SQL_EXT1 미완료와 logging 보류를 유지한다.

2026-10-22 [DEV6 작업 분배 개선](plans/dev6-work-breakdown.md): 18개 책임·55개 기존 묶음 아래 최하위 배정 단위를72개에서80개로 보완했다. [ARIES·Oracle·로컬5개 후속 점검](evidence/dev6-work-breakdown-review-20261022-r2.md)으로 C ABI·JDBC·ODBC 분량, DB capability/type-info 인수 연결과 [실제 성능 호출 경계](plans/dev6-implementation-plan.md#execution-budget)를 명시했다. 기존 의미·SC04→SC05→SC06·미실행 상태를 유지한다.

2026-10-22 [DEV4 18개 세부 작업 분류](plans/dev4-work-breakdown.md): 기존 D4-0–7을 하위 공급·통합 인수 단위로 나누고 선행·산출물·완료 조건·T01–14/V01–28/F01–05를 연결했다. [ARIES·Oracle19c·로컬 DBMS 조사](evidence/dev4-source-research.md#work-breakdown-20261022)를 참고했으며 제품 구현·시험 실행과 Q1 완료 승격은 없다.

2026-10-22 [DEV5 분배·인계 순서 후속 보완](plans/dev5-work-breakdown.md#decision-003):16개 분류·64개 ID를 유지하고7개 큰 항목을15개 내부 완료 단위로 나눴다. close→fetch→EOF 선행을 바로잡고 일반 terminal·취소/FIN·holder 진행·CLR 재장애를 조기에 연결한다. [ARIES·Oracle·로컬5개 후속 근거](evidence/dev5-work-breakdown-review-20261022.md#followup-003)와 실행 분량 대장을 반영하며 제품 개발·시험 재개와 B3 승격은 없다.

2026-10-22 [DEV3 16개 설계의 ARIES·Oracle·로컬 DBMS 병렬 리뷰](evidence/dev3-task-design-review-20261022.md): flush/reuse·UNIQUE·DDL·undo/recovery·backup/restore의8개 설계 반례를 보완하고 구현 동반 인수에 연결했다. 제품 코드·테스트 실행과 DEV3 완료 승격은 없다.

2026-10-22 [push 전 Clippy 검사 보완](policies/documentation-policy.md#doc-004): 기존 fmt·문서 gate가 검출하지 못한 테스트 전용 lint 2건을 수정하고 Actions와 같은 Clippy 정적 검사를 추가한다. [관련 hook 시험2개·새 gate 회귀3개](implementation/dev3-implementation.md#allocation-lint-fix)의 제한 검증과 기존 미완료 인수를 구별한다.

2026-10-22 [DEV3 단계별 필수 테스트·구현 동반 테스트 추가 계획](plans/dev3-test-plan.md): 중간 단계는 변경 기능의 필수 사례만 실행하고, 새 테스트·fixture·oracle는 구현과 함께 작성한다. 최신 사용자 지시에 따라 DEV3 최종 S5에서 전체 DEV3 inventory를 전수 실행하며, 장시간·대규모·전체 조합은 그 최종 인수에 배정한다.

2026-10-22 [DEV3 16개 작업별 개발 상세 명세·설계도](contracts/dev3-task-designs.md): 각 작업의 요구·입출력/소유·상태/처리 순서·오류/복구·자원·하위 개발 단위·독립 인수를 개별 문서로 작성한다.

2026-10-22 [DEV3 잔여 작업 계획 보완](plans/dev3-implementation-plan.md#remaining-work): 16개 분류에 선행 공급·완료 조건·인수 연결을 추가했다. [작업 크기·우선순위·병렬 운영](plans/dev3-implementation-plan.md#work-organization)은 큰 묶음의 추가 분해, 5개 우선 실행 묶음과 총괄1·작업2·독립검토1의 초기 배치를 정한다. 예약·취소의 미완료 검증과64KiB 증명, effect 전 F05, 복구 장벽8개를 유지한다. 구현·인수 현황은 [현재 상태](implementation/dev3-implementation.md#dev3-work-breakdown)를 따른다.

2026-10-21 [DEV3 페이지 예약·취소 부분 구현 및 중단](implementation/dev3-implementation.md#allocation-reservation-partial): 메모리 포트의 새 경로와 검토 수정만 제한 검증했다. native/compiler·변경 WAL 관련4시험·전체 단위 인수는 남겨 두고 사용자 요청으로 중단한다. [하부 작업 16개 분류 초안](implementation/dev3-implementation.md#dev3-work-breakdown)으로 진행·잔여 범위를 구분한다.

2026-10-21 [DEV3 최초 doublewrite 설치](implementation/dev3-implementation.md#doublewrite-install-results): XDI1·고정 images 공간·Empty DWB 선택을 구현했다. 기존 동일 source의 runtime10·native3·compiler9·checker31 및 Clippy/fmt 결과를 보존했고 수정 없는 코드는 재시험하지 않는다. 마지막 하위 회귀를 사용자 지시로 중단한 통합 원본은 FAIL이며 dirty/home/recovery·전체 DEV3는 미완료다.

2026-10-21 [DEV3 최초 doublewrite 설치 계약](contracts/dev3-doublewrite-install.md): pristine 세션의 소비·XDI1 선행 설치·Empty 선택과 고정 검증 입력을 독립 D2 검토 후 채택했다. 채택 당시 제품 구현·실행은 NOT_RUN이었으며 이후 최초 설치의 제한 검증은 위 구현 기록을 따른다. 전체 DEV3는 미완료다.

2026-10-21 [DEV3 최초 저장 세션](implementation/dev3-implementation.md#storage-session-results): extent·FILEMAP 설치와 reader/WAL 공통 fence를 구현했다. 같은 source의 runtime12·native3·compiler8·leaf59·하위28·현재-build DEV2 복구18check와 Clippy/fmt PASS이며 dirty buffer·home write·일반 복구·전체 DEV3는 미완료다.

2026-10-21 [DEV3 최초 저장 세션 계약](contracts/dev3-storage-session.md): 초기 extent·FILEMAP 설치와 WAL/reader 소유권·공통 fence를 D2 검토 후 확정했다. 계약 채택 당시 제품 구현·실행은 NOT_RUN이었으며 이후 제한된 실행 결과는 위 구현 기록을 따른다. 전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 RollbackStart WAL·page 생산](implementation/dev3-implementation.md#rollback-producer-results): typed before 상태에서 caller frame과 상태 page를 구성했다. 같은 source의 semantic22·WAL13·leaf54·하위 회귀28·DEV2 복구18check와 Clippy/fmt PASS이며 tag2 append·live 적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 RollbackStart page 구성](implementation/dev3-implementation.md#rollback-materialization-results): 검증된 WAL과 before page로 caller scratch의 after page를 구성했다. 같은 source의 관련17시험·leaf54·하위 회귀28·DEV2 복구18check와 Clippy/fmt PASS이며 live dirty frame·WAL 적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 fresh genesis WAL](implementation/dev3-implementation.md#genesis-wal-results): 첫 epoch·WRITER 설치와 한 slot의 physical write/sync를 구현했다. 같은 source의 runtime10·native3·compiler4·유한 모델·leaf54·하위 회귀28·DEV2 복구18check가 PASS이며 일반 recovery·dirty page·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 selected clean page buffer](implementation/dev3-implementation.md#read-buffer-results): 실제 reader 위에 bounded frame·pin·clock 교체를 구현했다. 같은 source의 leaf49·하위 회귀28·DEV2 복구18check(native99셀)와 수명 compiler·Clippy/fmt PASS이며 dirty buffer·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 RollbackStart 결합 검산](implementation/dev3-implementation.md#rollback-start-results): 거래 descriptor·상태 page·WAL의 전체 rollback 시작 전이를 검산했다. 같은 source의 leaf44·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 실제 append/적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 현재 growth intent 검산](implementation/dev3-implementation.md#ownership-intent-results): 실제 Profile과 inventory를 결합하고 owner별 현재 intent 중복을 고정 공간으로 거절한다. 같은 source의 leaf43·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 실제 예약 복원·DEV3 인수는 미완료다.

2026-10-21 [DEV3 ownership 관측](implementation/dev3-implementation.md#ownership-observation-results): 고정 공간으로 inventory bytes·owner별 합계·물리 충돌을 검증했다. 같은 소스의 leaf43·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 intent 유일성·실제 예약 복원·DEV3 인수는 미완료다.

2026-10-21 [DEV3 재개·선택 page 읽기](implementation/dev3-implementation.md#page-reader-results): selector·열린 handle metadata·제한된 page 읽기를 구현하고 같은 보존 source의 leaf·DEV0/1/2 회귀·DEV2 복구99셀을 통과했다. 전체 DEV3 완료 목표로 계속 진행하며 실제 쓰기·복구·S1 인수는 남아 있다.

2026-10-21 [DEV3 catalog·undo·control 표현](implementation/dev3-implementation.md#catalog-undo-control): 동일 보존 source에서 표현110개·DEV0/1/2 회귀·DEV2 복구99셀 PASS. 이 단계 commit 뒤 [사용자 요청으로 중지](implementation/dev3-implementation.md#paused-20261021)하며 전체 DEV3는 미완료다.

2026-10-21 [DEV3 WAL·할당·행·doublewrite 표현](implementation/dev3-implementation.md#wal-allocation-row-doublewrite): 독립 검토, 같은 보존 source의 engine 표현82개·DEV0/1/2 회귀·DEV2 복구99셀 PASS. 실제 disk 효과와 전체 DEV3 인수는 미완료다.

2026-10-20 [DEV3 tree·overflow·거래 표현](implementation/dev3-implementation.md#tree-overflow-transaction): 독립 검토와 보존 source의 표현 시험·DEV0/1/2 회귀·같은 source의 DEV2 복구99셀을 통과했다. 실제 DEV3 disk 효과와 전체 인수는 미완료다.

2026-10-20 [DEV3 유한 모델 통합](implementation/dev3-implementation.md#models-all): M01–M06의 고정 범위를 같은 보존 source에서 실행하여 정상457·지정 반례127 PASS를 확인했다. 모델은 실제 disk/thread·opcode 자원표·DEV3 전체 인수를 대신하지 않는다.

2026-10-20 [DEV3 후속 표현·모델](implementation/dev3-implementation.md#models-125): composite key·PageRef·UndoRef·anchor 표현과 M01/M02/M05를 독립 검토·보존 소스로 실행했다. 모델 정상32/결함44 PASS이며 M03/M04/M06 통합·실제 disk·전체 DEV3 인수는 미완료다.

2026-10-20 [DEV3 첫 leaf](implementation/dev3-implementation.md#validation): 16 KiB page envelope와 snapshot/pin 부분 모델을 병렬 구현·독립 검토했다. page7·모델 정상4/결함8·하위 회귀·동일 소스의 DEV2 복구99셀 PASS. 실제 disk I/O·전체 M02·DEV3/S1 인수는 미완료다.

2026-10-20 [DEV2 journal binary 대응](implementation/dev2-implementation.md#dev2-journal-binary-mapping-20261020): 설치 커널의 build ID와 기계어 흐름으로 두 대기 위치를 metadata·commit record 버퍼에 대응했다. 추가 측정 없이 코드 위치만 구분했으며 PF04·DEV2 인수는 미완료다.

2026-10-20 [DEV2 journal 대기 진단](implementation/dev2-implementation.md#dev2-journal-diagnostic-20261020): 고정 Off 1회에서 측정 fsync 512개와 실제 journal 대기 TID·단계를 연결했다. 추적 영향·환경의 거친 확인 조건은 FAIL이므로 시간 비율을 계측 없는 실행에 일반화하지 않으며, PF04·DEV2 인수는 미완료다.

2026-10-20 [DEV2 PF04 가설 검토](implementation/dev2-implementation.md#dev2-pf04-hypothesis-review-20261020): 기존 2,048개 cycle 추적과 설치 버전 커널 소스를 대조했다. 공유 파일시스템의 `max_batch_time` 변경을 해결책으로 삼을 근거는 부족하며, PF04 실패와 DEV2 미완료 상태를 유지한다.

2026-10-20 [DEV2 fsync 진단](implementation/dev2-implementation.md#dev2-fsync-diagnostic-20261020): 고정 4개 실행의 정답·추적 연결 검사를 완료했다. 이 실행의 측정 시간은 주로 worker 깨움 이전 fsync 대기였으며, 기존 PF04 실패와 DEV2 미완료 상태는 유지한다.

2026-10-20 [DEV2 성능 후속 실행](implementation/dev2-implementation.md#dev2-performance-followup-20261020): 준비한 9개 campaign·180개 run의 정답 검사와 재현을 완료했다. PF12 국소 개선과 PF13 재현 기능은 개별 검증 완료이며, PF04의 네 campaign 비용 한도 FAIL로 DEV2 전체 인수는 미완료다.

현재 DEV0·DEV1 최소 기반, DEV2 P0 독립 이력 oracle와 K0 volatile 행·catalog substrate를 부분 구현했습니다. 후속 streaming controller·catalog 재현과 K0 성능 진단·audit/replay를 포함한 이전 소스에서 DEV0·DEV1 회귀 및 DEV2 P0/K0 leaf를 로컬 검증했습니다. 이 결과는 현재 소스의 검증을 대신하지 않습니다. 최초 성능 비교의 p99 비용 한도는 FAIL이며 성능 인수는 미완료입니다. K1–K5 영속 커널은 구현 연결·독립 정적 검토를 마쳤으며, 동결 소스에서 복구·V3·하위 회귀의 로컬 실행을 완료했습니다. 일부 계측 비용 한도 실패로 최종 인수는 미완료입니다. 후속 제품 인수는 미완료입니다. 정확한 범위는 [현재 구현](implementation/README.md)과 [개발 단계](plans/development-stages.md)를 확인합니다.

2026-10-19 [DEV2 남은 시험 실행](implementation/dev2-implementation.md#dev2-tests-20261019): 동결 source에서 영속 native 99셀, 9개 30분 target, P0 전수 9,604,608건과 하위 회귀를 완료했다. 성능 후보의 개선 확인과 일부 계측 비용 한도 FAIL을 구분하며 DEV2 전체 인수는 미완료다.

2026-10-19 [DEV2 계측 수정](implementation/dev2-implementation.md#dev2-instrumentation-fix-20261019): CPU 관측과 상세 진단 비용을 수정하고 같은 동결 source에서 ext4 복구 99셀·하위 회귀·필수 성능 시험 36개를 통과했다. 최종 로컬 성능 인수는 INCOMPLETE이며 전체 DEV2 인수는 미완료다.

2026-10-19 [DEV2 새 시험 후 일시 중지](implementation/dev2-implementation.md#dev2-resumed-tests-20261019): 삭제 후 e3 소스로 실행한 11개30분 target 중 11개 PASS와 복구 보완 증거를 보존했다. 집계기 캐시 지문 오류는 별도 기록했으며, 사용자 요청으로 새 성능 측정 전에 중지했다. DEV2 인수는 미완료다.

2026-10-25 [내부 명칭·반복 파일명 정리](evidence/naming-cleanup-20261025.md): Rust crate 별칭과 기능별 파일 배치를 단축하고 외부 명령·과거 증거를 보존했다. 선택 검증 결과와 범위는 기록을 따른다.

# 개발 계획

- [모듈 독립 개발 구조 전환 작업 계획](module-independence-plan.md) — 2026-10-26 사용자 구조 변경 방향. 후속 에이전트의 소유 문서 수정 대상, M0–M5 설계·문서·구조 변경·두 제품 기능 병렬 인수, 보류/재개 조건. [M2 설계 인계](module-independence-plan.md#m2-handoff)는 A/B 기능·API 호출·공통 공급·선택 인수의 연결을 구체화한다. 작업 계획과 M2/M3 독립 D2 채택·완료. M4 C0·C1·C2 부분공급은 [Drain 인계 분리](../evidence/module-independence-survey-20261026.md#c2-drained-handoff-main)까지 진행했고 M5 A/B 순수 본문은 착수했다. 전체 C0·C1–C5 공급·M5 실제 제품 인수는 미완료.

- [첫 행 중심 실행과 개선 범위](dev3-implementation-plan.md#direction-refinement) — 2026-10-25 방향 재점검 반영. 당시 구조·명명·공통화는 소비자 연결의 국소 후보로 제한했다. 후속 구조 전환에서 바꿀 작업 순서의 범위는 [대체 관계](module-independence-plan.md#document-map)에 명시하며 제품 의미·품질 기준은 유지한다.

- DEV3 비교 분석 반영: [완료분 보완·우선순위](dev3-implementation-plan.md#comparison-adoption), [D3-COMP-01–10 구현 동반 인수](dev3-test-plan.md#comparison-adoption). 계획 채택과 실행 결과를 구별한다.

- [DEV2 PF04 해결 단계와 커밋 계획](dev2-pf04-resolution-plan.md): 계획 선행 commit, P1–P3 관측·선택 검증, P4 WAL 미채택 판단과 C2 K0 단일 writer 통계 후보·고정 비교 예산. 후속 [C3 thread별 shard counter와 측정 profile B](dev2-pf04-resolution-plan.md#c3-profile-b)는 독립 D2 r5 채택 뒤 [S1 profile B 도구](dev2-pf04-resolution-plan.md#s1-execution)와 [S2 C3 후보 고정](dev2-pf04-resolution-plan.md#s2-execution)을 완료했고 S3–S7은 NOT_RUN, 영속 처리는 결정 대기다.

- [DEV4 — DEV3 병행 준비·독립 모듈 개발 계획](dev4-preparation-plan.md): 기존4.xx에 연결한 개발 순서·F01–05 부분 동결/잔여·FE01–16 선택 시험·진단 효과의 실제 연결 인수·DEV3 수신. D4-PREP-001/002 독립 D2 채택, 개발/시험 NOT_RUN.

2026-10-23 [DEV3 AI 개발 효율 개선 계획](dev3-ai-throughput-improvement.md): 작업 입력·시간 기록의 첫 운영 적용으로 소비자1/3, 효과 비교는 미완료다. [후속 적용 범위](dev3-ai-throughput-improvement.md#bounded-application)는 기존 기록을 활용하며 별도 플랫폼·보고 체계를 첫 행의 선행으로 만들지 않는다. 기존 품질 기준과 S5 전수 인수를 유지한다.

2026-10-22 [DEV3 개발 모델·검토 운영](dev3-implementation-plan.md#model-roles): GPT-6.1 Sol 주 개발, 작성자 외 독립 검토, 가능하면 Astra의 핵심 복구·동시성 설계 검토. 첫 평가 단위3.12.1.a와 후속 실제 복구 연결로 적합성을 확인한다.

2026-10-22 [DEV3 방향 리뷰 후속 결정](dev3-implementation-plan.md#direction-review-adoption)과 [현재 우선순위](dev3-implementation-plan.md#work-priority): 제한 복구·일반 page 수명·첫 행 거래의 완료 조건,64GiB/계측 선행 준비와 [DIR01–05 시험 배정](dev3-test-plan.md#direction-integration).

[개발 문서 안내](../README.md)의 작업 경로를 따라 필요한 문서를 선택한다. 계획·제안·구현·실행 상태는 각 문서에서 확인한다.

<!-- catalogue -->

- [DEV3 — 단계별 필수 테스트와 구현 동반 테스트 추가 계획](dev3-test-plan.md) — 중간 변경 기능의 필수 사례·[16개 항목 보완](dev3-test-plan.md#item-review)·[실행 셀/중복 방지](dev3-test-plan.md#case-inventory)·[시간/준비 비용](dev3-test-plan.md#cost-control)·3.03.1 완료 기록/복구8장벽·최종 S5 DEV3 전수

- [DEV9 — 제품 인수 구현·검증 계획](dev9-implementation-plan.md) — F01–10·104개 요구 수신·T01–34·R2-V01–04·P01–05·R16 설치/문서·버전 적용성; SC16/17 미실행

- [DEV8 — 복제·운영 구현·검증 계획](dev8-implementation-plan.md) — F01–10·30개 요구·T01–40·R2-V01–04·P01–07·실제 하위/driver/peer 공급; 제품 인수 미실행

- [DEV7 — 36개 세부 작업 분류](dev7-work-breakdown.md) — 140개 하위 작업·제약 설치/큐/worker/온라인 복구 인계·분량/우선순위/실행 비용과 64개 요구 연결; 기존 의미·미실행 상태 유지
- [DEV7 — 확장 기능 구현·검증 계획](dev7-implementation-plan.md) — F01–10·64개 요구·T01–32·DESIGN-002 비교/CALL/XA 반례·fault/성능; 구현·제품 인수 미실행

- [DEV6 — 18개 세부 작업 분류와 공급·인수 계획](dev6-work-breakdown.md) — 55개 기존 묶음·80개 최하위 배정 단위, C ABI/JDBC/ODBC 분량·DB capability/type-info 인수 연결·작은 실제 연결 우선순위; 제품 실행 NOT_RUN
- [DEV6 — 클라이언트 구현·검증 계획](dev6-implementation-plan.md) — DEV6 설계·REVIEW-002 인수 의무와 driver별 실행 선택·cell 분모·기계 시간 예산; CLIENT1 미실행

- [DEV5 — 서버 구현·검증 계획](dev5-implementation-plan.md) — D5-F01–05·T01–14·worker 대기/만료·철회 후 정리 반례·crash cut·PF11 및 DEV6 이관
- [DEV5 — 서버 개발 작업 분류와 공급·인수 계획](dev5-work-breakdown.md) — 16개 분류·64개 하위 작업, D5-S01–06 공급 배정·7개 큰 항목의15개 내부 인계·close/fetch 선행·일반 거래 조기 검증·실행 분량·기존 인수 대응; 제품 미구현·B3 NOT_RUN

- [architecture-decisions.json](architecture-decisions.json)
- [하위 계층부터 개발하는 순서](bottom-up-development.md)
- [capability-inventory.json](capability-inventory.json)
- [COMM — 구현·검증 계획](comm-implementation-plan.md) — C1/KU12 leaf·readiness/종료 경합 반례·독립 peer·자원·성능 인수와 DEV5 이관
- [DEV2 — 조사 결과와 구현·검증 계획](dev2-implementation-plan.md)
- [DEV3 — 구현·병렬 개발·검증 계획](dev3-implementation-plan.md) — [16개 작업별 상세 명세·설계도](../contracts/dev3-task-designs.md), [잔여 16개 분류의 선행·완료 조건](dev3-implementation-plan.md#remaining-work), [작업 크기·우선순위·병렬 배치](dev3-implementation-plan.md#work-organization), 예약·취소 잔여 인수·effect 전 F05·복구 장벽8개·D3-M01–M06·DEV2 수신·DS/PF 연결
- [DEV4 — embedded SQL 구현·검증 계획](dev4-implementation-plan.md) — Oracle 의미·DDL 두 outcome·관측 버전 전달·rollback/recrash/예약 복원·실제 저장 공급 인수
- [DEV4 — 세부 작업 분류와 공급·인수 계획](dev4-work-breakdown.md) — 18개 분류·기존55개 ID 아래 실행 단위, 초기 INSERT 선행·공통 오류·모델/ACK 공급·[상대 분량](dev4-work-breakdown.md#workload-review)·sorter 재사용·R1–R4/PF10 배정; 구현·Q1 미실행
- [development-stages.json](development-stages.json)
- [XADB 개발 단계](development-stages.md)
- [feature-design-inventory.json](feature-design-inventory.json)
- [XADB 기능 명세·설계 작성 대상](feature-design-inventory.md)
- [XADB 초기 제품 기준선](phase1-product-baseline.md)
- [phase1-scope.json](phase1-scope.json)
- [XADB 제품 기능 요구](product-requirements.md)
- [XADB 제품 방향](project-direction.md)
- [stage-completion.json](stage-completion.json)

DEV3의 COW pack 경로는 [in-place·WAL·영속 undo 결정](../contracts/disk-storage-design.md#decision)과 [구현 순서](../plans/dev3-implementation-plan.md#sequence)로 대체한다. 설계와 제품 실행 상태를 구분한다.

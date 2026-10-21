# 개발 계획

[개발 문서 안내](../README.md)의 작업 경로를 따라 필요한 문서를 선택한다. 계획·제안·구현·실행 상태는 각 문서에서 확인한다.

<!-- catalogue -->

- [DEV3 — 단계별 필수 테스트와 구현 동반 테스트 추가 계획](dev3-test-plan.md) — 중간 변경 기능의 필수 사례·NT01–16 신규 시험 개발·3.03.1 잔여/복구 장벽별 최소 목록·최종 S5 DEV3 전수

- [DEV9 — 제품 인수 구현·검증 계획](dev9-implementation-plan.md) — F01–10·104개 요구 수신·T01–34·R2-V01–04·P01–05·R16 설치/문서·버전 적용성; SC16/17 미실행

- [DEV8 — 복제·운영 구현·검증 계획](dev8-implementation-plan.md) — F01–10·30개 요구·T01–40·R2-V01–04·P01–07·실제 하위/driver/peer 공급; 제품 인수 미실행

- [DEV7 — 확장 기능 구현·검증 계획](dev7-implementation-plan.md) — F01–10·64개 요구·T01–32·DESIGN-002 비교/CALL/XA 반례·fault/성능; 구현·제품 인수 미실행

- [DEV6 — 클라이언트 구현·검증 계획](dev6-implementation-plan.md) — DEV6 설계·REVIEW-002 상태/취소/결과/버퍼 보완·인수 의무; CLIENT1 미실행

- [DEV5 — 서버 구현·검증 계획](dev5-implementation-plan.md) — D5-F01–05·T01–14·worker 대기/만료·철회 후 정리 반례·crash cut·PF11 및 DEV6 이관

- [architecture-decisions.json](architecture-decisions.json)
- [하위 계층부터 개발하는 순서](bottom-up-development.md)
- [capability-inventory.json](capability-inventory.json)
- [COMM — 구현·검증 계획](comm-implementation-plan.md) — C1/KU12 leaf·readiness/종료 경합 반례·독립 peer·자원·성능 인수와 DEV5 이관
- [DEV2 — 조사 결과와 구현·검증 계획](dev2-implementation-plan.md)
- [DEV3 — 구현·병렬 개발·검증 계획](dev3-implementation-plan.md) — [16개 작업별 상세 명세·설계도](../contracts/dev3-task-designs.md), [잔여 16개 분류의 선행·완료 조건](dev3-implementation-plan.md#remaining-work), [작업 크기·우선순위·병렬 배치](dev3-implementation-plan.md#work-organization), 예약·취소 잔여 인수·effect 전 F05·복구 장벽8개·D3-M01–M06·DEV2 수신·DS/PF 연결
- [DEV4 — embedded SQL 구현·검증 계획](dev4-implementation-plan.md) — Oracle 의미·DDL 두 outcome·관측 버전 전달·rollback/recrash/예약 복원·실제 저장 공급 인수
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

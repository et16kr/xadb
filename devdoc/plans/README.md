# 개발 계획

[개발 문서 안내](../README.md)의 작업 경로를 따라 필요한 문서를 선택한다. 계획·제안·구현·실행 상태는 각 문서에서 확인한다.

<!-- catalogue -->

- [DEV8 — 복제·운영 구현·검증 계획](dev8-implementation-plan.md) — F01–10·30개 요구·T01–40·R2-V01–04·P01–07·실제 하위/driver/peer 공급; 제품 인수 미실행

- [DEV7 — 확장 기능 구현·검증 계획](dev7-implementation-plan.md) — F01–10·64개 요구·T01–32·DESIGN-002 비교/CALL/XA 반례·fault/성능; 구현·제품 인수 미실행

- [DEV6 — 클라이언트 구현·검증 계획](dev6-implementation-plan.md) — DEV6 설계·REVIEW-002 상태/취소/결과/버퍼 보완·인수 의무; CLIENT1 미실행

- [DEV5 — 서버 구현·검증 계획](dev5-implementation-plan.md) — D5-F01–05·T01–14·worker 대기/만료·철회 후 정리 반례·crash cut·PF11 및 DEV6 이관

- [architecture-decisions.json](architecture-decisions.json)
- [하위 계층부터 개발하는 순서](bottom-up-development.md)
- [capability-inventory.json](capability-inventory.json)
- [COMM — 구현·검증 계획](comm-implementation-plan.md) — C1/KU12 leaf·readiness/종료 경합 반례·독립 peer·자원·성능 인수와 DEV5 이관
- [DEV2 — 조사 결과와 구현·검증 계획](dev2-implementation-plan.md)
- [DEV3 — 구현·병렬 개발·검증 계획](dev3-implementation-plan.md) — D3-M01–M06·snapshot/aging·대기 없는 행 충돌·복구 예약/재장애·자원 계산·DEV2 재사용 경계
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

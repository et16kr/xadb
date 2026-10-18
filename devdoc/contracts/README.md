# 기능 계약

[개발 문서 안내](../README.md)의 작업 경로를 따라 필요한 문서를 선택한다. 계획·제안·구현·실행 상태는 각 문서에서 확인한다.

<!-- catalogue -->

- [DEV5 — 서버·세션·요청 프로토콜 상세 설계](dev5-server-design.md) — local 인증·권한·XSQ1·prepared·취소·자원·외부 ACK; B3 미실행

- [ai-performance-engineering.json](ai-performance-engineering.json)
- [XADB — AI가 분석·수정·검증하는 성능 개발 구조](ai-performance-engineering.md)
- [causal-diagnosis-cases.json](causal-diagnosis-cases.json)
- [COMM — 독립 통신 계층 상세 설계](comm-design.md) — AXF1 stream·상태별 readiness·공유 clock·quota·취소/drain/Drop 수명; 구현·C1 검증 미실행
- [dev0-contract.json](dev0-contract.json)
- [dev1-contract.json](dev1-contract.json)
- [DEV2 — SQL 없는 영속 커널 상세 설계](dev2-kernel-design.md)
- [DEV4 — embedded 최소 SQL·Oracle 의미 상세 설계](dev4-sql-design.md) — 처리 범위·저장 공급/cleanup·all-null unique lookup 인수 포함
- [DEV3 — 디스크 저장·인덱스 상세 설계](disk-storage-design.md) — snapshot 등록/aging 원자 경계·변경/참조 수명·undo 상태·자원 벡터·DWB batch 계약 포함
- [disk-test-matrix.json](disk-test-matrix.json)
- [execution-boundary-cases.json](execution-boundary-cases.json)
- [XADB Rust 구현 계약](implementation-contracts.md)
- [kernel-test-matrix.json](kernel-test-matrix.json)
- [module-test-matrix.json](module-test-matrix.json)
- [pilot-acceptance.json](pilot-acceptance.json)
- [recovery-logging-contract.json](recovery-logging-contract.json)
- [XADB — 통합 recovery·logging·checkpoint 설계](recovery-logging-design.md)
- [recovery-test-matrix.json](recovery-test-matrix.json)
- [XADB — recovery undo의 로그 보존과 RP 제약 검토](replication-undo-analysis.md)
- [replication-undo-plan.json](replication-undo-plan.json)

DEV3의 COW pack 경로는 [in-place·WAL·영속 undo 결정](../contracts/disk-storage-design.md#decision)과 [구현 순서](../plans/dev3-implementation-plan.md#sequence)로 대체한다. 설계와 제품 실행 상태를 구분한다.

# 기능 계약

- [DEV3 WAL frame·page redo 표현](dev3-wal-format.md) — 무효과 표현과 고정 검증 범위
- [DEV3 extent 할당 표현](dev3-allocation-format.md) — 무효과 표현과 고정 검증 범위
- [DEV3 행·인덱스 버전 표현](dev3-row-format.md) — 무효과 표현과 고정 검증 범위
- [DEV3 selected doublewrite control 표현](dev3-doublewrite-format.md) — 무효과 표현과 고정 검증 범위
- [DEV3 실제 표현 크기와 자원 계산](dev3-resource-bounds.md) — F05 준비 계산, opcode·실제 I/O 인수 미완료

[개발 문서 안내](../README.md)의 작업 경로를 따라 필요한 문서를 선택한다. 계획·제안·구현·실행 상태는 각 문서에서 확인한다.

<!-- catalogue -->

- [DEV3 selected clean page buffer](dev3-read-buffer.md) — 실제 선택 reader·pin·bounded clock의 채택 계약; dirty buffer·전체 S2 미완료

- [DEV3 복합 key 표현](dev3-key-format.md) — F01 scalar/composite·RowId bytes와 무효과 비교; index 실행·unique 인수 제외
- [DEV3 PageRef·UndoRef·안정 anchor 표현](dev3-reference-format.md) — 참조·root descriptor의 무효과 byte 검증; 실제 pin·SMO·undo 회수 제외
- [DEV3 공통 tree page 표현](dev3-tree-format.md) — 정렬 cell·경로 선택·byte 기준 분할 위치; 실제 SMO·파일 I/O 제외
- [DEV3 row overflow 표현](dev3-overflow-format.md) — 고정 chunk·streaming 순서·길이·digest 검산; 실제 할당·읽기·회수 제외
- [DEV3 거래 identity·상태 표현](dev3-transaction-format.md) — 논리/물리 참조·행 버전·status cell; 실제 directory·거래·복구 효과 제외
- [DEV3 M01/M02 graph 모델의 두 번째 동결](dev3-undo-model.md) — edge 중복도·논리 undo와 snapshot/pin 확장; 세부 동결·실행 상태는 원문 참조
- [DEV3 M03 변경 순서·행 관측·진행 모델](dev3-progress-model.md) — 관측 identity·pin/latch·FIFO의 실행 전 모델 계약
- [DEV3 M06 예약·복구 bootstrap 모델](dev3-reservation-model.md) — 자원 소유·재장애 두 슬롯·진행의 실행 전 모델 계약
- [DEV3 WAL·CLR·복구 예약 유한 모델](dev3-recovery-model.md) — M04 원자 그룹·capture·예약 복원의 실행 전 모델 계약
- [DEV3 doublewrite batch 유한 모델](dev3-doublewrite-model.md) — M05 다중 file pending·repair·재장애의 실행 전 모델 계약
- [DEV3 page envelope 형식](dev3-page-format.md) — F01 page bytes·무효과 codec만 동결, cell 의미·내구성 제외
- [DEV3 snapshot 획득·참조 pin의 부분 유한 모델](dev3-snapshot-model.md) — D3-M02-P01 고정 입력, 전체 M02·실제 thread 인수 제외

- [DEV9 — 제품 기능·운영·성능 인수 설계](dev9-qualification-design.md) — DESIGN-002 실행 분모·ACK 관측·PITR 목적·공식 배포, SC16/17·전체 조합·독립 판정; 제품 인수 미실행

- [DEV8 — 복제·백업·운영·성능 설계](dev8-operations-design.md) — DESIGN-002 domain·sequence·partition 이동·WAL/control 보완, SC12–15·H1/L1/M1·PITR·보안/운영; OPS1 미실행

- [DEV7 — 확장 SQL·프로그래머블 객체·외부 거래 설계](dev7-sql-extensions-design.md) — SC07–11·64개 요구·DESIGN-002 비교 문맥/CALL/XA 보완; leaf 동결·SQL_EXT1 미실행

- [DEV6 — native·JDBC·ODBC 클라이언트 상세 설계](dev6-client-design.md) — DEV6 설계·REVIEW-002 상태/취소/결과/버퍼 보완·인수 의무; CLIENT1 미실행

- [DEV5 — 서버·세션·요청 프로토콜 상세 설계](dev5-server-design.md) — local 인증·권한·XSQ1·prepared·취소·자원·외부 ACK, worker 대기 진행·거래 수명·idle 만료·철회 후 정리; B3 미실행

- [ai-performance-engineering.json](ai-performance-engineering.json)
- [XADB — AI가 분석·수정·검증하는 성능 개발 구조](ai-performance-engineering.md)
- [causal-diagnosis-cases.json](causal-diagnosis-cases.json)
- [COMM — 독립 통신 계층 상세 설계](comm-design.md) — AXF1 stream·상태별 readiness·공유 clock·quota·취소/drain/Drop 수명; 핵심 구현은 구현 색인 참조; C1 전체 인수 미완료
- [dev0-contract.json](dev0-contract.json)
- [dev1-contract.json](dev1-contract.json)
- [DEV2 — SQL 없는 영속 커널 상세 설계](dev2-kernel-design.md)
- [DEV4 — embedded 최소 SQL·Oracle 의미 상세 설계](dev4-sql-design.md) — 관측 행 버전 전달·처리 범위·저장 예약 복원/cleanup·all-null unique lookup 인수 포함
- [DEV3 — 디스크 저장·인덱스 상세 설계](disk-storage-design.md) — snapshot/aging·잠금 후 논리 행 버전 검사·영속 예약 복원·변경/참조 수명·DWB batch 계약 포함
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

- [DEV3 table·index catalog 표현](dev3-catalog-format.md) — 상세 상태·검토·구현·실행 경계는 원문 참조
- [DEV3 undo record·before-image chunk 표현](dev3-undo-format.md) — 상세 상태·검토·구현·실행 경계는 원문 참조
- [DEV3 control·FILEMAP·bootstrap profile 표현](dev3-control-format.md) — 상세 상태·검토·구현·실행 경계는 원문 참조
- [DEV3 영속 recovery obligation·WAL 의미 그룹](dev3-obligation-format.md) — 상세 상태·검토·구현·실행 경계는 원문 참조
- [DEV3 FILEMAP·DWB selector port](dev3-selector-port.md) — 상세 상태·검토·구현·실행 경계는 원문 참조

- [DEV3 선택 FILEMAP과 page 읽기](dev3-page-io.md) — 열린 handle metadata·전체 manifest 검증·bounded 읽기 구현; 고정 범위 검증 완료

- [DEV3 거래 상태의 영속 home](dev3-status-home.md) — core192와 incoming count를 결합하는224-byte 표현; 독립 검토·7시험 PASS, 실제 갱신·복구 미구현

- [DEV3 ownership inventory 검산](dev3-ownership-format.md) — bytes·물리 충돌·owner별 합계 및 실제 profile/current intent의 D2 수용; 현재 intent 구현·국소 및 하위 회귀 PASS, 실제 예약 복원 미완료

- [DEV3 RollbackStart의 full state 결합](dev3-obligation-format.md#13-rollbackstart의-full-state-결합-검산) — D2 수용 뒤 구현·고정12시험 검증 진행; 실제 WAL append·page 쓰기·예약 복원 미구현

- [DEV3 genesis WAL 설치·append/sync](dev3-wal-io.md) — fresh namespace의 첫 epoch·physical sync; D2 채택·고정 범위 로컬 검증 완료, 일반 recovery·전체 인수 미완료

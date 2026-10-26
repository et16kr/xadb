# 기능 계약

- [DEV3 모듈 독립 개발의 소유·소비 경계](dev3-module-boundaries.md) · [두 제품 소비자](dev3-module-consumers.md) — M2 D2 ACCEPTED. 제한 port·공통 공급, 첫 INSERT와 snapshot point-read/cursor의 명세·독립 fixture 설계 채택; C0 borrowed codec 부분공급·공통 공급 전체와 제품 인수 미완료. [Missing Row 참조 이동](dev3-visibility-retirement.md#row-missing-traversal) · [statement/cursor F02](dev3-task-06-tree-rows.md#module-statement-cursor) · [자원 배정](dev3-resource-bounds.md#row-missing-module-budget).

- [DEV3 첫 Catalog CREATE](dev3-catalog-create.md) — 원 owner의 실제 CREATE·Commit·복구 결합 D2 채택; 제품 효과·인수 미완료. [Root v2](dev3-create-root-format.md) · [Job v2](dev3-create-job-format.md) · [group scope2](dev3-create-group-format.md) · [base v3](dev3-create-recovery-base.md) · [visibility·회수](dev3-visibility-retirement.md).

- [DEV3 원 Commit의 지연 설치 수명](dev3-commit-continuation.md) — 기존 PinSet·WAL·core의 비공개 continuation과 durable content drain을 한정 D2 채택; actual terminal 권한·Commit/복구 검증 미완료.

- [DEV3 원 plan4의 Free Undo 페이지 수명](dev3-plan4-free-undo.md) — 같은 allocator/pool의 INIT·flush·재적재·selected base/재장애 공급 계약; Free container 선택 검증·독립 D1 수용, Live Undo·행 거래 미완료.

- [저장 세션의 국소 개선 경계](dev3-storage-session.md#consumer-driven-refinement) · [첫 행 완료 조건](dev3-task-11-commit.md#first-row-integration) — 원 권한·자원 수명과 live/recovery 상태 차이를 보존하며 실제 소비자에 필요한 변경만 선택한다. 제품 구현·시험 완료를 뜻하지 않는다.

- [DEV3 비교 분석 후속 설계](disk-storage-design.md#task-review-011) — 자원 벡터, immutable checkpoint 입력, offline typed namespace restore와 성능 판정 경계.

- [DEV3 plan4 빈 거래 복구 owner](dev3-plan4-recovery-owner.md) — 원 자원·선택/REBASE·새 epoch·같은 pool의 복구 종료; 중단 작업 검증 완료·독립 D1/D2 한정 수용

- [DEV3 plan4 빈 거래 live](dev3-retention-live.md) — 원 owner/pool의 fresh 수명·flush·재적재와 관측용 tag3 prefix; 복구 owner 별도

- [DEV4 P5 결정적 BFS 탐색기](dev4-lifecycle-explorer.md) — D4-LM-BFS-001; 메모리 안 결정적 BFS·alias·edge8/16 ledger·cap과 판정, 승인된 깊이 5 확대. archive·durable checkpoint·FE14는 후속.

- [DEV4 P5 독립 edge checker](dev4-lifecycle-checker.md) — D4-LM-CHECK-001; 모델을 호출하지 않는 `verify_edge`의 판정 범위·해석 선택·호출 계약과 모델 차등 대조. BFS·FE14는 후속.

- [DEV4 P5 수명 모델 전이](dev4-lifecycle-model.md) — D4-LM-MODEL-001; gate 순서·응답 행·family 전이·환경 제외·소비 bit와 owner 결정, checker 차등·BFS 판정12건. BFS·FE14는 후속.

- [DEV4 P5 전체 입력 조립](dev4-lifecycle-assembler.md) — D4-LM-ASM-001; 21 family의 prefix/context/control·Bind·Env1·owner/obligation 조립과 S0–S5 오류 순서, owner 결정3건. admission·모델 전이·FE14는 후속.

- [DEV4 P5 원시 action 번호·target 선택](dev4-lifecycle-raw-selection.md) — D4-LM-RAW-001 D2·제한 D1 수용; raw1533 번호·exact selector의 국소 계약. 전체 input/model은 후속.

- [DEV3 복구 이미지·제어 레코드](dev3-recovery-control-base.md) — XRB1·XRC1/XWE2/XWS2와 닫힌 census/공통 bridge; 실제 복구 권한·live F05 별도

- [DEV4 P5 상태·입력 codec](dev4-lifecycle-codec.md) — D4-LM-CODEC-001 D2 수용; bounded 자료형·canonical wire·raw negative 보존과 loader 필수 거절. 모델 전이·FE14 완주는 후속.

- [DEV3 빈 거래·정리 큐 순수 의미 처리](dev3-empty-retention-semantics.md) — Root/Job/Aging·N=0 Begin/AbortEnd/Finish·tag3 경계; 실제 live/복구 권한 별도

- [DEV4 — DEV3와 독립적인 SQL 앞단 계약](dev4-independent-frontend.md): Oracle19c 적용/끝 공백 차이·순수 입력/값·소유/자원·진단과 호출 효과·실제 engine 연결 경계. D4-PREP-001/002 및 D4-FE-LEX-001/SPLIT-001/PARSE-001/SCALAR-001/PREDICATE-001/TYPED-001/REMAINING-001 ACCEPTED. lexer·이름·streaming·P2/P4 raw 문장·owned AST·P3 scalar/predicate 구문과 명시 타입 검사·3값 평가를 제한 구현했다. P4 정상 선택 검증·21제품 변조 거절과 최종 독립 D1을 제한 수용했으며 typed 문장 binding·P5/P6·실제 engine·Q1은 미완료다.

2026-10-24 [Free Undo 초기 page](dev3-undo-format.md#free-initial-page): target 없는 Free codec과 한 슬롯의 초기 page 표현 계약을 D2 수용했다. 실제 allocation/INIT·mixed owner의 권한과 복구는 후속 의무다.

2026-10-24 [plan3 Undo root 역할](dev3-status-bootstrap.md#undo-bootstrap): 불변 Status2/Undo3 역할과 기존 adapter의 plan3 거절을 D2 수용했다. root INIT·mixed pool·plan3 복구는 별도 후속 계약이다.

2026-10-23 [DEV3 고정 등록 코어·세대 pin](dev3-task-05-buffer-flush.md#registration-core): 기존 SharedPool의 단일 lookup·Copy 요청·compact 세대 token과 실행 전 자원 변경을 D2 검토했다. 제한 단위 구현·선택 검증이며 일반 mutable pool은 미공급이다.

- [DEV3 현재 genesis의 실제 닫힌 복구](dev3-recovery-effects.md): 실물 bootstrap·선택 Pending 수리·한 번 예약 인계·page별 REDO의 제한 구현설계 D2를 수용했다. 제품 구현 중이며 시험은 NOT_RUN이다.

- [DEV3 현행 WAL prefix의 닫힌 분석](dev3-recovery-prefix.md): actual reserve/cancel·Status INIT의 canonical owner·inventory·RAM image 분석과 초기 SELECT/물리 복구의 경계. 선택 검증과 제한 단위 최종 D1을 수용했다.

- [DEV3 초기 복구 입력 선택](dev3-recovery-select.md): 3.12.1.a의 초기 control·exclusive lease·header-only WAL 선택, 닫힌 inspection·오류·자원·필수 인수의 채택 계약.

2026-10-22 [DEV3 방향 리뷰 반영](../plans/dev3-implementation-plan.md#direction-review-adoption): [일반 page 수명](dev3-task-05-buffer-flush.md#operational-pool), [첫 행 거래](dev3-task-11-commit.md#first-row-integration), [제한 복구](dev3-task-12-recovery.md#early-recovery), [64GiB 준비](dev3-resource-bounds.md#qualification-profile-preparation), [계측 공급](dev3-task-16-qualification.md#observation-delivery)의 경계를 구체화했다.

- [DEV3 Status home flush](dev3-status-home-flush.md): consuming bound·sealed target·zero/exact home·순차 whole batch1·자원/선택 입력.

- [DEV3 폐쇄 두-frame Status INIT](dev3-status-init.md): 실제 reserve 권한·두 image RAM 공개·동일 WAL requirement·두-pin reader·자원 한도.

- [DEV3 불변 Status bootstrap](dev3-status-bootstrap.md): plan2 영속 역할·typed borrowed role·old allocator 경계·효과 전 자원 거절.

- [DEV3 allocator 한 페이지 flush](dev3-allocator-flush.md): sealed capture·WAL force·DWB/home·whole retirement·조건부 RAM finalize의 제한 소유 계약.

- [DEV3 dirty frame·stage 설치](dev3-dirty-stage.md): 3.05.1-A/B + 3.04.1.a 첫 allocator 소비자·pin/content·DPT의 채택 계약.

- [DEV3 적용 admission·version gate](dev3-concurrency-admission.md): 3.10.1.a.1 계약 채택, 실제 guard와 후속 Commit 모의 명세의 범위 구분.

- [DEV3 WAL frame·page redo 표현](dev3-wal-format.md) — 무효과 표현과 고정 검증 범위
- [DEV3 extent 할당 표현](dev3-allocation-format.md) — 무효과 표현과 고정 검증 범위
- [DEV3 행·인덱스 버전 표현](dev3-row-format.md) — 무효과 표현과 고정 검증 범위
- [DEV3 selected doublewrite control 표현](dev3-doublewrite-format.md) — 무효과 표현과 고정 검증 범위
- [DEV3 실제 표현 크기와 자원 계산](dev3-resource-bounds.md) — F05 준비 계산, opcode·실제 I/O 인수 미완료

[개발 문서 안내](../README.md)의 작업 경로를 따라 필요한 문서를 선택한다. 계획·제안·구현·실행 상태는 각 문서에서 확인한다.

<!-- catalogue -->

## DEV3 작업별 상세 명세·설계도

[2차 비교 리뷰](../evidence/dev3-task-design-review-20261022-r2.md)의 설치 순서·drain·backup cut 보완은 [DEV3-REVIEW-010](disk-storage-design.md#task-review-010)과 연결한다.

[공통 경계·공급 설계도·인수 규칙](dev3-task-designs.md) 아래 기존 16개 작업을 각각 상세화한다. 기존 소유 계약을 유지하며 설계 작성과 제품 구현·실행은 구분한다. [2026-10-22 비교 리뷰](../evidence/dev3-task-design-review-20261022.md)의 반례 보완은 [DEV3-REVIEW-009](disk-storage-design.md#task-review-009)와 연결한다.

- [3.01 형식·정체성](dev3-task-01-formats.md)
- [3.02 파일·읽기 기반](dev3-task-02-files-reads.md)
- [3.03 공간 할당](dev3-task-03-allocation.md)
- [3.04 WAL 변경 경로](dev3-task-04-wal.md)
- [3.05 dirty buffer·flush](dev3-task-05-buffer-flush.md)
- [3.06 트리·행](dev3-task-06-tree-rows.md)
- [3.07 인덱스](dev3-task-07-indexes.md)
- [3.08 카탈로그·DDL](dev3-task-08-catalog-ddl.md)
- [3.09 undo·rollback](dev3-task-09-undo-rollback.md)
- [3.10 동시성·가시성](dev3-task-10-concurrency.md)
- [3.11 commit·공개](dev3-task-11-commit.md)
- [3.12 재시작 복구](dev3-task-12-recovery.md)
- [3.13 checkpoint](dev3-task-13-checkpoint.md)
- [3.14 공간 회수](dev3-task-14-reclamation.md)
- [3.15 백업·무결성](dev3-task-15-backup-integrity.md)
- [3.16 자원 종합·성능·최종 인수](dev3-task-16-qualification.md)

## 기존 소유 계약과 참고 문서

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
  - [16개 분류·64개 하위 작업](../plans/dev5-work-breakdown.md#catalogue) — 기존 계약의 공급·인수 배정과 우선순위; 새 제품 의미·byte 형식 채택 없음

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

- [DEV3 RollbackStart의 full state 결합·임시 page 구성](dev3-obligation-format.md#rollback-materialization) — D2 수용·기존12+구성5시험과 동일 source 하위 회귀 PASS; live 적용·semantic WAL append·예약 복원 미완료

- [DEV3 genesis WAL 설치·append/sync](dev3-wal-io.md) — fresh namespace의 첫 epoch·physical sync; D2 채택·고정 범위 로컬 검증 완료, 일반 recovery·전체 인수 미완료

- [DEV3 RollbackStart WAL·상태 page 생산](dev3-obligation-format.md#rollback-producer) — D2 수용·semantic22/WAL13 및 같은 source 하위 회귀 PASS; tag2 append·live 적용·전체 DEV3 인수는 미완료

- [DEV3 최초 저장 세션·FILEMAP 설치](dev3-storage-session.md) — D2 수용·고정 bootstrap/소유권/fence/검증 입력; 제한 실행 결과는 현재 구현 참조

- [DEV3 최초 doublewrite 설치](dev3-doublewrite-install.md) — pristine 세션 consume·XDI1·Empty 선택·고정 장애 입력; 최초 설치 구현·제한 검증과 변경 코드만 시험하는 사용자 지시

- [DEV3 실제 페이지 예약·취소](dev3-allocation-reservation.md) — SystemBegin/End와 authoritative dirty metadata·보호 자원 회계의 D2 채택; 메모리 포트 부분 검증 후 사용자 요청 중단

- [DEV3 allocator 동일 주소의 resident 세대 reload](dev3-allocator-residency.md) — consuming owner·actual read·세대 갱신의 제한 계약

- [DEV3 혼합 Status·Free Undo owner](dev3-mixed-system-owner.md) — 원 pool의 INIT·pressure·flush·load와 별도 process의 닫힌 mixed 복구 계약

- [DEV3 plan4 정리 큐·빈 Catalog의 최초 설치](dev3-plan4-genesis.md) —9page genesis와 원 DWB·별도 process의 읽기 선택; D2 수용·필수304/실제3PID PASS·최종 D1 제한 수용

- [DEV3 plan4 초기 page의 원 공유 버퍼 소비](dev3-plan4-clean-owner.md) — 원 CAS·clean 적재/퇴출·Catalog3pin·정리 큐4/4/3pin; 필수78 source별 PASS·최종 독립 D1 제한 수용

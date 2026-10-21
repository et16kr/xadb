# XADB 개발 문서

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

**Markdown 중심으로 관리합니다. HTML은 생성하지 않습니다.** 현재 DEV0·DEV1 최소 기반, DEV2 P0 독립 이력 oracle와 K0 volatile 행·catalog substrate를 부분 구현했습니다. 후속 streaming controller·catalog 재현과 K0 성능 진단·audit/replay를 포함한 이전 소스에서 DEV0·DEV1 회귀 및 DEV2 P0/K0 leaf를 로컬 검증했습니다. 이 결과는 현재 소스의 검증을 대신하지 않습니다. 최초 성능 비교의 p99 비용 한도는 FAIL이며 성능 인수는 미완료입니다. K1–K5 영속 커널은 구현 연결·독립 정적 검토를 마쳤으며, 동결 소스에서 복구·V3·하위 회귀의 로컬 실행을 완료했습니다. 일부 계측 비용 한도 실패로 최종 인수는 미완료입니다. 후속 제품 인수는 미완료입니다. 정확한 범위는 [현재 구현](implementation/README.md)과 [개발 단계](plans/development-stages.md)를 확인합니다.

2026-10-19 [DEV2 남은 시험 실행](implementation/dev2-implementation.md#dev2-tests-20261019): 동결 source에서 영속 native 99셀, 9개 30분 target, P0 전수 9,604,608건과 하위 회귀를 완료했다. 성능 후보의 개선 확인과 일부 계측 비용 한도 FAIL을 구분하며 DEV2 전체 인수는 미완료다.

2026-10-19 [DEV2 계측 수정](implementation/dev2-implementation.md#dev2-instrumentation-fix-20261019): CPU 관측과 상세 진단 비용을 수정하고 같은 동결 source에서 ext4 복구 99셀·하위 회귀·필수 성능 시험 36개를 통과했다. 최종 로컬 성능 인수는 INCOMPLETE이며 전체 DEV2 인수는 미완료다.

2026-10-19 [DEV2 새 시험 후 일시 중지](implementation/dev2-implementation.md#dev2-resumed-tests-20261019): 삭제 후 e3 소스로 실행한 11개30분 target 중 11개 PASS와 복구 보완 증거를 보존했다. 집계기 캐시 지문 오류는 별도 기록했으며, 사용자 요청으로 새 성능 측정 전에 중지했다. DEV2 인수는 미완료다.

## 작업을 시작할 때

상시 기준은 [코딩 컨벤션](policies/coding-convention.md), [개발 운영 방침](policies/development-policy.md), [문서 운영 기준](policies/documentation-policy.md)입니다. 이어서 아래에서 이번 작업에 필요한 계약만 읽습니다. 자료성 문서 전체를 먼저 읽을 필요는 없습니다.

| 작업·질문 | 우선 읽을 원문 | 추가 확인 |
| --- | --- | --- |
| 이름·API·오류·수명·리뷰 | [코딩 컨벤션](policies/coding-convention.md) | 해당 코드의 Rustdoc·계약 시험 |
| 공개 DBMS 소스 비교·연구 | [공개 참고 자료와 로컬 소스 경로](reference/opensource-references.md) | PostgreSQL·MariaDB·CUBRID 등 10개 checkout의 경로·소스 진입점·기록 revision |
| 결정 권한·검증 강도·의존성·복잡성 | [개발 운영 방침](policies/development-policy.md) | D0–D3·V0–V3, 별도 검토와 실제 증거 |
| 파일·crate·빌드 경계 변경 | [저장소 배치](policies/repository-build-plan.md) | [파일 소유·병렬 정책](policies/parallel-development-policy.md) |
| 다음 기능·단계 완료 판단 | [개발 단계](plans/development-stages.md) | [완료 계약](plans/stage-completion.json), [하위 계층 결정](plans/bottom-up-development.md) |
| 현재 foundation/platform·XADBTest 변경 | [구현 색인](implementation/README.md) | DEV0/DEV1 계약·현재 소스·실행 결과 |
| DEV2 커널·WAL·복구 변경 | [구현 계약](contracts/implementation-contracts.md), [DEV2 상세 설계](contracts/dev2-kernel-design.md) | [DEV2 구현·검증 계획](plans/dev2-implementation-plan.md), 해당 복구·저장 소유 계약 |
| DEV3 디스크 page·buffer·index 설계와 병렬 개발 | [DEV3 상세 설계](contracts/disk-storage-design.md), [구현·병렬 개발·검증 계획](plans/dev3-implementation-plan.md#parallel-development) | [WAL·undo 재설계 조사](evidence/dev3-wal-undo-research.md), [아키텍처 비교 보완](evidence/dev3-dev4-architecture-review.md), DS01–06·상태 모델·관측 행 버전·복구 예약 복원·DEV2 수신 |
| DEV4 최소 SQL·Oracle 의미·prepared 실행·embedded CLI | [DEV4 상세 설계](contracts/dev4-sql-design.md), [구현·검증 계획](plans/dev4-implementation-plan.md) | [소스·Oracle/ARIES 조사](evidence/dev4-source-research.md), [초기 처리 범위](contracts/dev4-sql-design.md#capacity-boundary), SC02·KU13·PF10·D4-V26 행 충돌·실제 DEV3 선행 증거 |
| DEV5 서버·인증/권한·세션·요청/취소 | [DEV5 상세 설계](contracts/dev5-server-design.md), [구현·검증 계획](plans/dev5-implementation-plan.md) | [소스 조사·검토](evidence/dev5-source-research.md), SC03·KU14·PA08·PF11·실제 Q1/C1 선행 증거 |
| DEV6 native·C ABI·JDBC·ODBC client 설계 | [클라이언트 상세 설계](contracts/dev6-client-design.md), [구현·검증 계획](plans/dev6-implementation-plan.md) | [Oracle·ARIES·로컬 소스 재검토](evidence/dev6-source-research.md#review-002), REVIEW-002·R2-V01–V06, SC04–06·D6-W01·실제 B3 선행; CLIENT1 미실행 |
| DEV7 확장 SQL·타입·PSM·LOB/spatial·외부 거래 | [DEV7 설계](contracts/dev7-sql-extensions-design.md), [구현·검증 계획](plans/dev7-implementation-plan.md) | [Oracle·ARIES·로컬 소스 재검토](evidence/dev7-review-002.md), DESIGN-002 비교 문맥/CALL/XA·SC07–11·64개 요구·F01–10; SQL_EXT1 미실행 |
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

문서 검사는 **push 직전** 훅에서 수행합니다. 편집·commit 때 반복 실행하지 않습니다. 훅은 전송할 각 ref의 최종 commit의 임시 사본에서 `cargo fmt --all --check`, `python3 devdoc/verify_documents.py`를 순서대로 실행하고 하나라도 실패하면 push를 차단합니다. 설치와 범위는 [push 검사 규칙](policies/documentation-policy.md#push-gate)을 따릅니다.

이 명령은 문서·계약·증거 정합성을 검사하며 제품 시험을 실행하지 않습니다. [저장된 결과](document-validation.json)는 해당 실행 당시 결과이며 현재 문서나 DB의 검증 완료로 해석하지 않습니다. 과거 측정 스크립트를 문서 정리 목적으로 다시 실행하지 않습니다.

기존 개발 파일을 새 XADB 저장소로 이관했으며 이전 Git 이력과 과거 시험 결과는 가져오지 않았습니다. 이번 이관의 경로·증거 처리 기록은 [이관 기록](document-migration.json)에 있습니다. [제품 저장소 안내](../README.md)

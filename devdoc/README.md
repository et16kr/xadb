# XADB 개발 문서

**Markdown 중심으로 관리합니다. HTML은 생성하지 않습니다.** 현재 DEV0·DEV1 최소 기반, DEV2 P0 독립 이력 oracle와 K0 volatile 행·catalog substrate를 부분 구현했습니다. 후속 streaming controller·catalog 재현과 K0 성능 진단·audit/replay를 포함한 현재 소스에서 DEV0·DEV1 회귀 및 DEV2 P0/K0 leaf를 로컬 검증했습니다. 최초 성능 비교의 p99 비용 한도는 FAIL이며 성능 인수는 미완료입니다. 영속 엔진·후속 제품 인수는 미구현·미실행입니다. 정확한 범위는 [현재 구현](implementation/README.md)과 [개발 단계](plans/development-stages.md)를 확인합니다.

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
| DEV3 디스크 page·buffer·index 설계와 병렬 개발 | [DEV3 상세 설계](contracts/disk-storage-design.md), [구현·병렬 개발·검증 계획](plans/dev3-implementation-plan.md#parallel-development) | [WAL·undo 재설계 조사](evidence/dev3-wal-undo-research.md), DS01–06·DEV2 이관/선행 계약 |
| DEV4 최소 SQL·Oracle 의미·prepared 실행·embedded CLI | [DEV4 상세 설계](contracts/dev4-sql-design.md), [구현·검증 계획](plans/dev4-implementation-plan.md) | [소스·Oracle/ARIES 조사](evidence/dev4-source-research.md), SC02·KU13·PF10·DEV3 선행 증거 |
| 성능 계측·원인 분석·개선 | [AI 성능 개발 계약](contracts/ai-performance-engineering.md) | DP07, 해당 단계 PF 인수 의무 |
| 기능 명세·설계를 작성할 대상 | [작성 대상 목록](plans/feature-design-inventory.md) | [원문 이름 색인](reference/feature-design-leaf-index.md)과 해당 소유 계약 |
| 프로젝트 목표·AI 개발 실험 | [제품 방향](plans/project-direction.md) | 현재 구현과 최종 목표, 공개 DBMS 참고 방향을 구분 |
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

문서 검사는 **push 직전** 훅에서 수행합니다. 편집·commit 때 반복 실행하지 않습니다. 훅은 전송할 commit의 임시 사본에서 `python3 devdoc/verify_documents.py`를 실행하고 실패하면 push를 차단합니다. 설치와 범위는 [문서 push 규칙](policies/documentation-policy.md#push-gate)을 따릅니다.

이 명령은 문서·계약·증거 정합성을 검사하며 제품 시험을 실행하지 않습니다. [저장된 결과](document-validation.json)는 해당 실행 당시 결과이며 현재 문서나 DB의 검증 완료로 해석하지 않습니다. 과거 측정 스크립트를 문서 정리 목적으로 다시 실행하지 않습니다.

기존 개발 파일을 새 XADB 저장소로 이관했으며 이전 Git 이력과 과거 시험 결과는 가져오지 않았습니다. 이번 이관의 경로·증거 처리 기록은 [이관 기록](document-migration.json)에 있습니다. [제품 저장소 안내](../README.md)

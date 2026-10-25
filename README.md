# XADB

XADB는 **오픈소스 DBMS를 참고하여, AI가 이해하고 개발하기 좋은 DBMS를 AI agent로 만들 수 있는지 시험하는 저장소**입니다. 현재는 초기 개발 단계입니다.

최종 목표는 Linux에서 Rust로 구현하는 디스크 기반 관계형 DBMS입니다. 폭넓은 기능과 기본 성능, 이해·수정·검증하기 좋은 구조를 함께 추구합니다. 전체 제품 인수는 미완료이며, 기능별 범위와 결과는 [현재 구현과 실행 상태](devdoc/implementation/README.md)에서 확인합니다.

## 설계·개발 원칙

| 주제 | 공통 기준 |
| --- | --- |
| [제품·품질](devdoc/plans/project-direction.md) | 필요한 기능을 보존하고 정확성·원자성·복구·자원 한도를 우선합니다. RAM보다 큰 데이터를 처리할 수 있어야 합니다. |
| [연구·상세 설계](devdoc/reference/opensource-references.md) | 공개 DBMS 소스·Oracle·ARIES·『대규모 C++』를 비판적으로 참고합니다. 선택 근거와 알고리즘·상태·실패·자원 계약을 구체화합니다. |
| [구조·개발 계획](devdoc/policies/coding-convention.md) | 책임·의존성·소유권을 명확히 하고, 필요한 기능·성능을 만족하는 단순한 구조를 선택합니다. 하위 계층부터 검증 가능한 단위로 개발합니다. |
| [검증·진단](devdoc/policies/ai-development-method.md) | 독립 정답·상태 모델과 실제 I/O·동시성·장애 시험을 연결합니다. 데이터 흐름으로 원인을 좁혀 실제 코어에서 최소 재현합니다. |
| [성능 개선](devdoc/contracts/ai-performance-engineering.md) | AI가 계측→원인 실험→수정→비교를 수행하게 합니다. 같은 정확성·내구성 조건에서 성능·자원·계측 비용을 확인합니다. |
| [AI 작업·협업](devdoc/policies/development-policy.md) | 확정 계약 안에서 자율 실행하고 중요 변경은 독립 검토합니다. 작업 소유권·재현 가능한 인계를 갖추고 결함·재작업·개발 비용으로 효과를 평가합니다. |
| [문서·공개·호환](devdoc/plans/phase1-product-baseline.md) | 요구→설계→구현→시험 증거를 연결하고 실제 지원 상태를 공개합니다. 개발 버전은 새 DB를 사용하고 안정 릴리스부터 호환 계약을 적용합니다. |

## 개발 단계

선행 관계를 기준으로 진행하며 독립 분기는 병행할 수 있습니다. 아래는 계획이며 완료 여부는 [단계·인수 기준](devdoc/plans/development-stages.md)과 실제 증거로 판단합니다.

| 단계 | 범위 | 선행 |
| --- | --- | --- |
| DEV0 | 빌드·최소 시험 도구·독립 판정 기반 | 없음 |
| DEV1 | 공통 타입·오류·자원과 Linux I/O 기반 | DEV0 |
| COMM | 프레임·소켓·흐름 제어·취소 | DEV1 |
| DEV2 | SQL 없는 트랜잭션·WAL·복구 커널 | DEV1 |
| DEV3 | 디스크 페이지·버퍼·행·인덱스 | DEV2 |
| PART | 디스크 파티션·로컬 인덱스 | DEV3 |
| NG | 파티션 간 native global index | PART |
| DEV4 | 최소 SQL·prepared 실행·embedded CLI | DEV3 |
| DEV5 | 서버·인증·권한·세션·요청 처리 | DEV4, COMM |
| DEV6 | native/C·JDBC·ODBC 클라이언트 | DEV5 |
| DEV7 | 확장 SQL·타입·저장 프로그램·외부 연계 | DEV4 |
| DEV8 | 복제·백업/PITR·보안·운영·성능 | DEV3, DEV5 |
| DEV9 | 제품 기능·운영·성능·설치·문서 최종 인수 | DEV6–8, PART |

DEV4의 저장 비의존 앞단은 DEV3와 병행 개발할 수 있습니다. 확장 기능은 해당 서버·클라이언트·PART/NG 공급도 필요하며, global index 인수에는 NG의 실제 통합 증거를 요구합니다.

## 소스 구성

제품 코드는 `crates/`의 기능별 패키지에, 시험 도구는 `crates/testing`에 둡니다. [패키지 배치와 의존 방향](devdoc/policies/repository-build-plan.md)

## 빌드와 검증

Linux x86_64, Rust 1.95.0과 `Cargo.lock`을 사용합니다.

```bash
cargo build --workspace --locked
```

시험은 [AGENTS.md](AGENTS.md)의 변경 범위·인수 규칙에 따라 선택합니다. 실행 진입점은 [구현 색인](devdoc/implementation/README.md), 결과는 [증거 색인](devdoc/evidence/README.md)을 따릅니다.

## 개발 문서

[devdoc/README.md](devdoc/README.md)에서 정책·계획·계약·현재 구현을 찾습니다. 문서는 Markdown 원문을 유지하며 JSON은 검증용 계약·색인·결과에 사용합니다.

문서 검사는 push 직전에 수행하고 편집·commit 때에는 실행하지 않습니다. 새 clone에서 기존 사용자 훅이 없는 경우 한 번 설정합니다.

```bash
git config --local core.hooksPath scripts/git-hooks
```

`git push`는 전송할 commit의 Rust 포맷·Clippy·문서 검사를 수행합니다. 기존 훅은 보존·연결하며 [push 검사 규칙](devdoc/policies/documentation-policy.md#push-gate)을 따릅니다.

기존 개발 파일을 독립 저장소로 이관했으며 이전 Git 이력과 실행 결과는 승계하지 않습니다. [이관 기록](devdoc/history/project-migration.md)

# XADB 독립 저장소 이관 기록

2026-10-18 · **파일 이관 완료 / 새 저장소 검증은 증거 색인 참조**

사용자 지시에 따라 기존 개발 작업 공간을 그대로 두고 새 XADB 저장소에 현재 파일을 복사했다. 미커밋 DEV2 변경을 포함하며 이전 `.git`, build 산출물, 실행 artifact는 가져오지 않았다. 복사 직후 원본 파일별 SHA-256을 대조해 복사 중 변경이 없음을 확인했다. 원본의 이후 독립 작업은 이 스냅샷에 자동 반영되지 않는다.

| 구분 | 처리 |
| --- | --- |
| DEV0/DEV1 구현·시험 | 이관, product package는 `xadb-*`, 도구는 `xadbtest*`로 명명 |
| DEV2 P0/K0 | 독립 이력 reference, volatile 행/catalog, frame, native worker와 시험, 미검증 streaming controller 초안 이관 |
| 저장 범위 | 디스크 엔진 단일 구성으로 계획·계약·인수 항목 재작성 |
| K0 임시 행 상태 | `crates/engine/src/storage/volatile.rs`로 명시; DB 내구성 기능으로 표시하지 않음 |
| 문서 | 개발 정책, 디스크에 적용 가능한 kernel/recovery 계약, SQL·사용자 매뉴얼·운영 초안, 기능 작성 목록 이관 |
| 제외 | 독립 resident engine와 혼합 저장 설계·시험 계획, 이전 제품 소스 조사·대체 전용 조건 |
| 과거 실행 증거 | 원본 작업 공간에 보존. XADB source hash와 제품명으로 재표기하지 않음 |
| 신규 검증 | XADB에서 실제 실행하여 새 보고서 생성; 문서 검사와 제품 시험을 분리 |
| Git·공개 | 원래 Git 이력 미이관; 새 이력으로 시작하는 Public 개발 저장소 |

[기계 판독 이관 기록](../document-migration.json)에 snapshot revision·시각·inventory digest·검증 상태를 보존한다. 원래 경로가 포함된 원본 파일별 inventory는 저장소 밖의 로컬 보관소 `$HOME/.local/state/xadb/migration-archive/<inventory SHA-256>/source-inventory.json`에 바이트 단위로 보존한다. 구현 파일의 원래/현재 경로·hash 대응은 [구현 이관 inventory](../evidence/implementation-import-inventory.json)에 보존한다. 현재 지원 범위는 [구현 색인](../implementation/README.md), 이번 실행은 [증거 색인](../evidence/README.md)을 따른다. 이관한 설계의 향후 참고 방향을 공개 DBMS로 정한 것이며 과거 설계나 측정의 출처를 다른 DBMS로 바꿔 주장하지 않는다.

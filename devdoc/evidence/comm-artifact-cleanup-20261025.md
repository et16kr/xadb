# COMM 시험 산출물 정리

2026-10-25 · **사용자 요청에 따른 보존·정리 완료 / 제품 변경·시험 실행 0**

[COMM 구현 기록](../implementation/comm-implementation.md) · [실패 수정 증거](comm-failure-fix-20261024.md)

사용자가 COMM 시험 산출물의 불필요한 파일 정리를 요청했다. `artifacts/comm`과 직전 실패 수정의 `/tmp/xadb-comm-failure-fix-20261024-in7aq01s`를 조사하고, 해당 경로를 cwd·실행 파일·열린 파일로 사용하는 프로세스가 없는지 확인했다. 다른 개발 작업의 산출물과 저장소 공용 `target`은 정리 범위에 넣지 않았다.

## 실제 정리 범위

| 항목 | 결과 |
| --- | --- |
| COMM 전용 Cargo cache | 저장소 아래29개와 직전 수정의 임시 영역6개, 총35개 정리 |
| 실행물·컴파일 진단 | ELF와 compiler JSON/진단을 압축 보존하고 각 내용의 SHA-256을 원본과 대조. 저장소 cache의 profile 바로 아래 실행 파일28개는 원래 경로에 동일 byte로 유지 |
| 로그·측정값·실패 기록·소스 | cache 밖의 기존75,047개 파일이 정리 전후 동일함을 hash로 확인 |
| 직전 수정의 임시 사본 | 남은40,849개 파일을 기존 증거·source archive·이번 binary archive에 매핑하고 내용 재검산 후 임시 디렉터리 삭제. 기존 archive에 없던 보조 문서3개는 별도 보존 |
| 저장소 디스크 절감 | 4,032,348,160 bytes, 약3.76GiB |
| `/tmp` 사용량 절감 | 1,735,041,024 bytes, 약1.62GiB. 이 경로는 tmpfs이므로 저장소 디스크 절감과 구별 |
| 남긴 COMM 산출물 | 약3.74GiB. 원 증거·소스·실행물과 이번 보존 archive/매핑 포함 |

측정은 `du -sx -B1`의 해당 디렉터리별 할당량이다. 같은 시간 다른 작업이 확보한 시스템 전체 여유 공간은 이번 성과에 합산하지 않았다. 수치는 작은 종료 receipt 작성 직전이며 정리 후 제품 시험이나 문서 검증을 실행하지 않았다. 원 PASS·FAIL·중단의 판정과 전체 COMM/C1 INCOMPLETE 상태는 유지한다.

## 보존 위치와 복원

실제 원자료는 `artifacts/comm/cleanup-20261025/`에 있다.

- `summary.json`, `cargo-cleanup.json`, `cargo-cleanup.log`: 범위·전후 용량·실제 종료 기록.
- `noncache-evidence-before.json`: 변경하지 않은 기존 증거75,047개의 원 hash.
- `cargo-00`부터 `cargo-34`까지의 `.json`/`.tar.gz`: 원 cache 경로, 보존 member의 상대 경로·mode·size·hash, archive hash. cache 안의 실행물 경로가 필요하면 해당 원 경로에 지정 member를 복원하고 hash를 대조한다. 다시 빌드해 과거 binary를 대체하지 않는다.
- `scratch-retention.json`: 삭제한 임시 파일별 원 절대 경로와 보존 파일 또는 archive/member 대응. `scratch-extra-evidence.tar.gz`는 추가 문서3개를 보존한다.
- `scratch-removed.json`, `scratch-cleanup.log`: 전체 대응 검산 뒤 임시 사본을 삭제한 실제 기록.

기존 receipt의 절대 경로와 원 source identity는 역사적 식별자로 남겨두었다. 필요하면 이 대응표와 [실패 수정의 보존 archive](comm-failure-fix-20261024.md)를 사용한다. 정리 helper는 이번 고정 목록을 처리한 artifact 전용 도구이며 제품 코드나 새로운 시험 결과가 아니다.

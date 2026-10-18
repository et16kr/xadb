# XADB 조사·실행 증거

2026-10-18 · 현재 실행 근거는 r3 회귀 보고서와 K0-005 성능 진단 r1 보고서다. DEV0 8개·DEV1 8개·DEV2 P0/K0 leaf 28개 check와 새 성능 반례 35개를 실행했다. release 계측 비교 2개와 재현 1개는 각각20개 실행의 정답과 원시 이력 보존을 확인했다. 최초 Always·Detailed 비교의 sample-p99 guardrail은 FAIL이며 성능 인수·승격은 INCOMPLETE다. DEV2·K0 전체, 내구성·protected CI 인수는 완료하지 않았다. r1·r2 회귀 보고서는 당시 source snapshot의 결과로 보존한다. 보고서는 실행기가 생성한 바이트 그대로 복사했다.

r1 당시 DEV0/DEV1은 로컬 범위 PASS다. 당시 DEV2 reference/substrate/frame/worker leaf는 PASS이나 DEV2와 K0 전체는 INCOMPLETE다. r1 당시 영속성·streaming controller·bounded concurrent exploration·PF 인수·30분 stress/fuzz·protected CI는 NOT_RUN이며 안정 제품 qualification이 아니다. `artifacts/` 경로는 해당 로컬 실행 산출물이며 Git으로 배포하지 않는다. 각 보고서의 source hash로 적용 소스를 확인한다.

재현: `python3 -B scripts/check_dev2_performance.py` (저장소 루트, 하위 회귀 포함). P0/K0 회귀만 실행할 때는 `python3 scripts/check_dev2_reference.py --with-substrate`를 사용한다. 과거 실행 결과는 원래 작업 공간에 남기고 새 XADB 결과로 재표기하지 않았다.

DEV3의 [소스 조사](dev3-source-research.md)는 실제 revision·열람 범위의 기록이며 DBMS build·제품 실행 또는 S1 PASS 증거가 아니다.

DEV4의 [소스·Oracle/ARIES 조사](dev4-source-research.md)는 PostgreSQL·MariaDB·CUBRID·SQLite의 SQL/prepared 및 복구 경계와 Oracle 19c 공식 문서·ARIES 초록/원 논문의 명시한 부분을 읽은 기록이다. SQL 구현·Oracle 실행 비교·Q1 PASS 증거가 아니다.

현재 source SHA-256은 `6e58ed3534a1c996a0d5bf9ea56e3f4e81d6dc95bc2b67fc704589f12b4a0e8f`다. `dev0-validation.json`·`dev1-validation.json`·`dev2-validation.json` 문서 ID는 r3를, `dev2-performance-validation.json`은 별도 성능 진단 r1을 참조한다. 이전 r1·r2는 원래 파일 경로 ID로 보존한다. 이전 시험 패키지 재구성의 범위·리뷰는 [경로 재구성 기록](../history/project-migration.md#testing-layout)을 따른다. 이번 병렬 구현·검토·실측 한계는 [K0-005 기록](../implementation/dev2-implementation.md)을 따른다.

<!-- catalogue -->

- [dev0-xadb-20261018-r1.json](dev0-xadb-20261018-r1.json)
- [dev1-xadb-20261018-r1.json](dev1-xadb-20261018-r1.json)
- [dev2-xadb-20261018-r1.json](dev2-xadb-20261018-r1.json)
- [implementation-import-inventory.json](implementation-import-inventory.json)
- [DEV3 소스 조사 기록](dev3-source-research.md)
- [DEV4 소스·Oracle/ARIES 조사 기록](dev4-source-research.md)

- [후속 구현 이관 inventory](implementation-incremental-import-20261018.json)

후속 코드의 source fingerprint에는 r1의 PASS를 적용하지 않는다. 중단한 `artifacts/dev2-reference/check-kecbzonw`의 부분 결과는 완료 증거가 아니다.

- [dev0-xadb-20261018-r2.json](dev0-xadb-20261018-r2.json)
- [dev1-xadb-20261018-r2.json](dev1-xadb-20261018-r2.json)
- [dev2-xadb-20261018-r2.json](dev2-xadb-20261018-r2.json)

- [DEV3 WAL·undo 재설계 조사와 검토](dev3-wal-undo-research.md): 선행 리뷰 보완 후 저장 재설계; 소스 열람/설계 검토이며 제품 실행 NOT_RUN.


- [dev0-xadb-20261018-r3.json](dev0-xadb-20261018-r3.json)
- [dev1-xadb-20261018-r3.json](dev1-xadb-20261018-r3.json)
- [dev2-xadb-20261018-r3.json](dev2-xadb-20261018-r3.json)
- [dev2-performance-xadb-20261018-r1.json](dev2-performance-xadb-20261018-r1.json)

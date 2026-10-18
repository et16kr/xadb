# XADB 조사·실행 증거

2026-10-18 · 현재 실행 근거는 r2 보고서다. 시험 패키지를 `crates/testing`으로 재구성한 소스에서 후속 K0 이관을 포함해 DEV0 8개·DEV1 8개·DEV2 P0/K0 leaf 28개 check를 실제 실행해 PASS했다. DEV2·K0 전체는 INCOMPLETE이며 내구성·PF·protected CI 인수는 완료하지 않았다. r1 보고서는 최초 XADB snapshot의 결과로 보존한다. 모든 보고서는 실행기가 생성한 바이트 그대로 복사했으며, DEV2 보고서가 참조하는 DEV1 및 DEV0 회귀도 함께 보존한다.

r1 당시 DEV0/DEV1은 로컬 범위 PASS다. 당시 DEV2 reference/substrate/frame/worker leaf는 PASS이나 DEV2와 K0 전체는 INCOMPLETE다. r1 당시 영속성·streaming controller·bounded concurrent exploration·PF 인수·30분 stress/fuzz·protected CI는 NOT_RUN이며 안정 제품 qualification이 아니다. `artifacts/` 경로는 해당 로컬 실행 산출물이며 Git으로 배포하지 않는다. 각 보고서의 source hash로 적용 소스를 확인한다.

재현: `python3 scripts/check_dev2_reference.py --with-substrate` (저장소 루트). 과거 실행 결과는 원래 작업 공간에 남기고 새 XADB 결과로 재표기하지 않았다.

DEV3의 [소스 조사](dev3-source-research.md)는 실제 revision·열람 범위의 기록이며 DBMS build·제품 실행 또는 S1 PASS 증거가 아니다.

DEV4의 [소스·Oracle/ARIES 조사](dev4-source-research.md)는 PostgreSQL·MariaDB·CUBRID·SQLite의 SQL/prepared 및 복구 경계와 Oracle 19c 공식 문서·ARIES 초록/원 논문의 명시한 부분을 읽은 기록이다. SQL 구현·Oracle 실행 비교·Q1 PASS 증거가 아니다.

현재 source SHA-256은 `3fa1b7958cc5597ba99be6ba81d75ea65360657d318d22d59e6e2b633f76a540`다. 재구성 범위·별도 리뷰는 [경로 재구성 기록](../history/project-migration.md#testing-layout)을 따른다. `dev0-validation.json`·`dev1-validation.json`·`dev2-validation.json` 문서 ID는 r2를 참조하고, r1은 원래 파일 경로를 문서 ID로 등록해 보존한다.

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

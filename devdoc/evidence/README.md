# XADB 조사·실행 증거

2026-10-18 · DEV0·DEV1 및 DEV2 일부 구현을 새 XADB 소스에서 실제 검증했다. 아래 실행 보고서는 생성된 바이트 그대로 보존한다. DEV2 보고서가 참조하는 DEV1 회귀와 그 DEV0 회귀도 원본 그대로 복사했다.

DEV0/DEV1은 로컬 범위 PASS다. DEV2 reference/substrate/frame/worker leaf는 PASS이나 DEV2와 K0 전체는 INCOMPLETE다. 영속성·streaming controller·bounded concurrent exploration·PF 인수·30분 stress/fuzz·protected CI는 NOT_RUN이며 안정 제품 qualification이 아니다. `artifacts/` 경로는 해당 로컬 실행 산출물이며 Git으로 배포하지 않는다. 각 보고서의 source hash로 적용 소스를 확인한다.

재현: `python3 scripts/check_dev2_reference.py --with-substrate` (저장소 루트). 과거 실행 결과는 원래 작업 공간에 남기고 새 XADB 결과로 재표기하지 않았다.

DEV3의 [소스 조사](dev3-source-research.md)는 실제 revision·열람 범위의 기록이며 DBMS build·제품 실행 또는 S1 PASS 증거가 아니다.

<!-- catalogue -->

- [dev0-xadb-20261018-r1.json](dev0-xadb-20261018-r1.json)
- [dev1-xadb-20261018-r1.json](dev1-xadb-20261018-r1.json)
- [dev2-xadb-20261018-r1.json](dev2-xadb-20261018-r1.json)
- [implementation-import-inventory.json](implementation-import-inventory.json)
- [DEV3 소스 조사 기록](dev3-source-research.md)

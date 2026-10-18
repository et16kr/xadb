# XADB 조사·실행 증거

2026-10-18 · COMM 최종 사본의 DEV0/DEV1/DEV2 보고서는 해당 실행 당시 소스의 회귀 근거이며, 이후 K1–K5 변경을 포함한 현재 소스의 검증 결과가 아니다. K0-005의 별도 성능 근거는 당시 source의 r1 보고서로 보존한다. DEV0 8개·DEV1 8개·DEV2 P0/K0 leaf 28개 check와 새 성능 반례 35개를 실행했다. release 계측 비교 2개와 재현 1개는 각각20개 실행의 정답과 원시 이력 보존을 확인했다. 최초 Always·Detailed 비교의 sample-p99 guardrail은 FAIL이며 성능 인수·승격은 INCOMPLETE다. DEV2·K0 전체, 내구성·protected CI 인수는 완료하지 않았다. r1·r2 회귀 보고서는 당시 source snapshot의 결과로 보존한다. 보고서는 실행기가 생성한 바이트 그대로 복사했다.

r1 당시 DEV0/DEV1은 로컬 범위 PASS다. 당시 DEV2 reference/substrate/frame/worker leaf는 PASS이나 DEV2와 K0 전체는 INCOMPLETE다. r1 당시 영속성·streaming controller·bounded concurrent exploration·PF 인수·30분 stress/fuzz·protected CI는 NOT_RUN이며 안정 제품 qualification이 아니다. `artifacts/` 경로는 해당 로컬 실행 산출물이며 Git으로 배포하지 않는다. 각 보고서의 source hash로 적용 소스를 확인한다.

재현: `python3 -B scripts/check_dev2_performance.py` (저장소 루트, 하위 회귀 포함). P0/K0 회귀만 실행할 때는 `python3 scripts/check_dev2_reference.py --with-substrate`를 사용한다. 과거 실행 결과는 원래 작업 공간에 남기고 새 XADB 결과로 재표기하지 않았다.

DEV3의 [소스 조사](dev3-source-research.md)는 실제 revision·열람 범위의 기록이며 DBMS build·제품 실행 또는 S1 PASS 증거가 아니다.

DEV4의 [소스·Oracle/ARIES 조사](dev4-source-research.md)는 PostgreSQL·MariaDB·CUBRID·SQLite의 SQL/prepared 및 복구 경계와 Oracle 19c 공식 문서·ARIES 초록/원 논문의 명시한 부분을 읽은 기록이다. SQL 구현·Oracle 실행 비교·Q1 PASS 증거가 아니다.

COMM 반영 commit과 당시 DEV3/DEV4 문서 commit 대상의 source SHA-256은 `e873f96b2b791d0c655dfb4019de4db6209a4f1ab02359f57e4f861815b62ad1`이며, 보존된 COMM 회귀 보고서의 실행 전후 지문과 일치한다. `dev0-validation.json`·`dev1-validation.json`·`dev2-validation.json` 문서 ID는 각각 `dev0/1/2-current-source-20261018.json`을 참조한다. 이 세 파일은 `comm-dev0/1/2-xadb-20261018-r1.json`의 byte-identical 사본이며, 원래 COMM 문서 ID·상대 artifact 경로·실행 root와 원본 hash를 유지한다. catalog의 단일 경로 규칙 안에서 기존 참조를 보존하기 위한 채택 사본으로 새 실행이나 재검증 보고서가 아니다. `dev2-performance-validation.json`은 source `6e58ed3534a1c996a0d5bf9ea56e3f4e81d6dc95bc2b67fc704589f12b4a0e8f`의 별도 성능 진단 r1을 유지한다. 이전 회귀 r1·r2·r3는 원래 파일 경로 ID와 바이트로 보존한다. 이 참조 갱신은 기존 실행의 적용 소스를 확인한 것이며 새 제품 시험 실행이나 진행 중인 DEV2 혼합 작업 트리의 PASS가 아니다. 이전 시험 패키지 재구성의 범위·리뷰는 [경로 재구성 기록](../history/project-migration.md#testing-layout)을 따른다. 이번 병렬 구현·검토·실측 한계는 [K0-005 기록](../implementation/dev2-implementation.md)을 따른다.

<!-- catalogue -->

- [DEV6 — Oracle·REF-C06-02·로컬 DBMS 클라이언트 조사와 검토](dev6-source-research.md) — DEV6 설계·선행 입력·조사/인수 의무; CLIENT1 미실행

- [DEV5 — 서버 소스 조사와 독립 검토 기록](dev5-source-research.md) — PostgreSQL·MariaDB·CUBRID 실제 revision/열람 근거와 DEV5-REVIEW-002 대기·수명 반례/수정; 제품 실행 증거 아님
- [DEV5 — Oracle·로컬 DBMS 비교와 후속 설계 검토](dev5-oracle-source-review.md) — Oracle 19c·로컬3개 DBMS의 실제 열람, cleanup 권한·idle 경계와 002 수신 검토; 제품 실행 증거 아님

- [DEV3·DEV4 아키텍처 비교와 설계 보완](dev3-dev4-architecture-review.md) — 실제 source revision·교차 상태/자원/batch 보완과 독립 검토; 모델·제품 실행 증거 아님
- [COMM 소스 조사와 설계 검토 기록](comm-source-research.md) — 최초 DBMS 조사와 후속 readiness·clock·종료 수명 리뷰/수정 근거; C1 실행 증거 아님
- [COMM 코드 리뷰와 수정 기록](comm-code-review-20261019.md) — driver 취소·accept 오류 격리 결함, DBMS 소스 비교와 해당 회귀·독립 검토; 전체 C1 인수 아님
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

- [K0-006 중단 보고서](dev2-extended-xadb-20261018-r1.json): 사용자 요청으로 약464초에 중단; FAIL 원본 보존, 30분 의무와 하위 회귀 미완료. K1–K5 구현 완료 전 테스트 금지.

- [COMM 전용 기준 사본 r1](comm-isolated-xadb-20261018-r1.json): 27개 시험 및 하위 회귀 PASS. 마지막 drain 경합 보완 전 source에만 적용.
- [COMM 혼합 사본 실패 r1](comm-mixed-xadb-20261018-r1.json): COMM focused 검사는 통과했으나 함께 포함된 DEV2 변경의 format 검사에서 하위 회귀 FAIL. 원본 실패를 보존.
- [COMM 전용 DEV0 회귀](comm-dev0-xadb-20261018-r1.json) · [COMM 전용 DEV1 회귀](comm-dev1-xadb-20261018-r1.json): 최종 COMM 전용 기준 사본에서 각 8개 check PASS. 당시 소스 지문 대조 뒤 current-source 문서 ID로 연결했으며 원본 실행 identity는 유지.
- [COMM 전용 최종 사본 r2](comm-isolated-xadb-20261018-r2.json): 28개 Rust 시험(기본·전체 feature 각각), 12개 native 사례, lint·독립 build·하위 회귀 PASS. 전체 C1은 INCOMPLETE.
- [COMM 전용 DEV2 회귀](comm-dev2-xadb-20261018-r1.json): 최종 COMM 전용 기준 사본의 P0/K0 leaf 28개 check PASS; 내구성·현재 혼합 작업 트리의 검증 아님.
- [COMM 실행 범위 색인](comm-execution-scope-20261018.json): 각 원본의 execution root·recipe·hash와 소유 파일 일치 기록. 위 COMM 보고서의 상대 artifact 경로는 이 색인의 해당 execution root 기준이다.

- [DEV0 당시 source 채택 사본](dev0-current-source-20261018.json) · [DEV1 당시 source 채택 사본](dev1-current-source-20261018.json) · [DEV2 당시 source 채택 사본](dev2-current-source-20261018.json): 기존 COMM 회귀 JSON과 바이트 동일. 상대 artifact 경로는 [원래 실행 범위](comm-execution-scope-20261018.json)의 root에서 해석한다.

- [K1–K5 영속 커널 검증 체크포인트](dev2-kernel-xadb-20261018-r1.json): 고정 source에서 engine 22·transaction 7·cache-loss 15·동시성 64 seeds와 첫 native 33 cases strict ACCEPTED. 2회차 ENOSPC로 FAIL, 3회차 미실행. 원래 빈 report와 로그를 보존한 사후 재구성 receipt이며 DEV2 완료 증거가 아님.

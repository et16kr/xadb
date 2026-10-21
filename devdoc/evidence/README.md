# XADB 조사·실행 증거

2026-10-21 [WAL·할당·행·DWB 표현 소스의 DEV2 복구 receipt](dev3-storage-formats-dev2-kernel-xadb-20261021-r1.json)는 동일 보존 source에서18개 check와 native99셀을 통과했다. 기존 DEV2 capability 회귀이며 DEV3 실제 디스크 복구 인수는 아니다.

2026-10-20 [tree·overflow·거래 표현 통합 receipt](dev3-expressions-leaf-xadb-20261020-r1.json)와 [동일 소스의 하위 회귀](dev3-expressions-dev2-regression-xadb-20261020-r1.json)를 보존했다. engine 표현43개·foundation scalar3개·부분 모델 정상4/결함8 및 DEV0/1/2 회귀 PASS이며, 아래 복구 receipt와 같은 지문이다. DEV3 전체 인수는 미완료다.

2026-10-20 [tree·overflow·거래 표현 소스의 DEV2 복구 receipt](dev3-expressions-dev2-kernel-xadb-20261020-r1.json)는 동일 보존 source에서18개 check와 native99셀을 통과했다. 새 DEV3 디스크 복구 증거가 아니며 [표현 통합 범위](../implementation/dev3-implementation.md#tree-overflow-transaction)를 따른다.

2026-10-20 [M01–M06 원본 receipt](dev3-models-xadb-20261020-r1.json)는 동일 보존 source에서 정상457·지정 반례127·예산 검사6·M05 workspace 회귀를 통과했다. M06의20개 비적용 경계는 실행 PASS와 구분한다. 유한 모델 검증이며 실제 disk·thread·전체 S0/F05·DEV3 인수가 아니다. [독립 검토·범위](../implementation/dev3-implementation.md#models-all)를 따른다.

2026-10-20 [M04 원본 receipt](dev3-m04-xadb-20261020-r1.json)는 정상5·결함10과 예산 검사를 별도 보존 source에서 실행했다. cap 미완주 이력을 보존한 정확 상태 동치 적용이며, DPT 기반 I/O 선정·실제 frame mapping은 이 모델의 증거가 아니다. [범위·제한](../implementation/dev3-implementation.md#model-4)을 따른다.

2026-10-20 [참조 표현 leaf](dev3-reference-leaf-xadb-20261020-r1.json)는 page7·key5·scalar3·anchor4·reference2·P01 정상4/결함8을 검증했다. 별도 [M01/M02/M05 원본 receipt](dev3-m01-m02-m05-xadb-20261020-r1.json)는 정상32·결함44와 budget·workspace 회귀를 보존 source/release binary로 독립 실행했다. 전체 모델·실제 disk·S1 인수는 미완료이며 [실행 범위](../implementation/dev3-implementation.md#models-125)를 따른다.

2026-10-20 DEV3 [첫 leaf PASS](dev3-leaf-xadb-20261020-r1.json), [하위 회귀](dev3-dev2-regression-xadb-20261020-r1.json), [초기 Clippy 실패](dev3-leaf-xadb-20261020-r0.json)와 [동일 최종 소스의 DEV2 복구 회귀](dev3-dev2-kernel-xadb-20261020-r1.json)를 보존했다. page7·부분 모델 정상4/결함8·DEV0/1/2 회귀·DEV2 native99셀을 검증했다. 실제 DEV3 디스크 복구·전체 모델·S1은 미완료이며 [구현 기록](../implementation/dev3-implementation.md#validation)의 범위에 한정한다.

2026-10-18 · COMM 최종 사본의 DEV0/DEV1/DEV2 보고서는 해당 실행 당시 소스의 회귀 근거이며, 이후 K1–K5 변경을 포함한 현재 소스의 검증 결과가 아니다. K0-005의 별도 성능 근거는 당시 source의 r1 보고서로 보존한다. DEV0 8개·DEV1 8개·DEV2 P0/K0 leaf 28개 check와 새 성능 반례 35개를 실행했다. release 계측 비교 2개와 재현 1개는 각각20개 실행의 정답과 원시 이력 보존을 확인했다. 최초 Always·Detailed 비교의 sample-p99 guardrail은 FAIL이며 성능 인수·승격은 INCOMPLETE다. DEV2·K0 전체, 내구성·protected CI 인수는 완료하지 않았다. r1·r2 회귀 보고서는 당시 source snapshot의 결과로 보존한다. 보고서는 실행기가 생성한 바이트 그대로 복사했다.

r1 당시 DEV0/DEV1은 로컬 범위 PASS다. 당시 DEV2 reference/substrate/frame/worker leaf는 PASS이나 DEV2와 K0 전체는 INCOMPLETE다. r1 당시 영속성·streaming controller·bounded concurrent exploration·PF 인수·30분 stress/fuzz·protected CI는 NOT_RUN이며 안정 제품 qualification이 아니다. `artifacts/` 경로는 해당 로컬 실행 산출물이며 Git으로 배포하지 않는다. 각 보고서의 source hash로 적용 소스를 확인한다.

재현: `python3 -B scripts/check_dev2_performance.py` (저장소 루트, 하위 회귀 포함). P0/K0 회귀만 실행할 때는 `python3 scripts/check_dev2_reference.py --with-substrate`를 사용한다. 과거 실행 결과는 원래 작업 공간에 남기고 새 XADB 결과로 재표기하지 않았다.

DEV3의 [소스 조사](dev3-source-research.md)는 실제 revision·열람 범위의 기록이며 DBMS build·제품 실행 또는 S1 PASS 증거가 아니다.

DEV4의 [소스·Oracle/ARIES 조사](dev4-source-research.md)는 PostgreSQL·MariaDB·CUBRID·SQLite의 SQL/prepared 및 복구 경계와 Oracle 19c 공식 문서·ARIES 초록/원 논문의 명시한 부분을 읽은 기록이다. SQL 구현·Oracle 실행 비교·Q1 PASS 증거가 아니다.

COMM 반영 commit과 당시 DEV3/DEV4 문서 commit 대상의 source SHA-256은 `e873f96b2b791d0c655dfb4019de4db6209a4f1ab02359f57e4f861815b62ad1`이며, 보존된 COMM 회귀 보고서의 실행 전후 지문과 일치한다. `dev0-validation.json`·`dev1-validation.json`·`dev2-validation.json` 문서 ID는 각각 `dev0/1/2-current-source-20261018.json`을 참조한다. 이 세 파일은 `comm-dev0/1/2-xadb-20261018-r1.json`의 byte-identical 사본이며, 원래 COMM 문서 ID·상대 artifact 경로·실행 root와 원본 hash를 유지한다. catalog의 단일 경로 규칙 안에서 기존 참조를 보존하기 위한 채택 사본으로 새 실행이나 재검증 보고서가 아니다. `dev2-performance-validation.json`은 source `6e58ed3534a1c996a0d5bf9ea56e3f4e81d6dc95bc2b67fc704589f12b4a0e8f`의 별도 성능 진단 r1을 유지한다. 이전 회귀 r1·r2·r3는 원래 파일 경로 ID와 바이트로 보존한다. 이 참조 갱신은 기존 실행의 적용 소스를 확인한 것이며 새 제품 시험 실행이나 진행 중인 DEV2 혼합 작업 트리의 PASS가 아니다. 이전 시험 패키지 재구성의 범위·리뷰는 [경로 재구성 기록](../history/project-migration.md#testing-layout)을 따른다. 이번 병렬 구현·검토·실측 한계는 [K0-005 기록](../implementation/dev2-implementation.md)을 따른다.

<!-- catalogue -->

- [DEV3 복합 key 표현·page·부분 snapshot 모델](dev3-key-leaf-xadb-20261020-r1.json) — 보존 소스의 key5·scalar3·page7·P01; 전체 모델·disk 인수·하위 전체 회귀 제외

- [DEV3 page envelope·부분 snapshot 모델·하위 회귀](dev3-leaf-xadb-20261020-r1.json) — 보존 소스의 실제 leaf 결과
- [DEV3 첫 leaf와 같은 소스의 DEV2 P0/K0·DEV0/1 회귀](dev3-dev2-regression-xadb-20261020-r1.json) — 28개 check와 하위8/8개 PASS
- [DEV3 첫 통합 실패 기록](dev3-leaf-xadb-20261020-r0.json) — Clippy map_entry 실패 보존
- [DEV3 첫 leaf와 같은 소스의 DEV2 복구 회귀](dev3-dev2-kernel-xadb-20261020-r1.json) — 현재-build 공급 회귀, DEV3/S1 인수 미완료

- [DEV9 — Oracle·ARIES·로컬 소스 조사와 설계 검토](dev9-source-research.md) — DESIGN-002 원장·ACK 관측·PITR·공식 배포 재검토, 공개 로컬4개·LOCAL-Q01의 실제 revision/hash; 제품 실행 아님

- [DEV8 — Oracle·ARIES·로컬 소스 조사와 설계 검토](dev8-source-research.md) — 공개 로컬4개·LOCAL-R01 실제 revision/hash와 설계 선택; 제품 실행 아님
- [DEV8 — 운영 설계 2차 검토](dev8-review-002.md) — domain별 중복 제거·sequence 보호·partition 이동·WAL/control cut 보완, Oracle/ARIES·로컬 소스 근거; 제품 실행 아님

- [DEV7 — 비교 문맥·CALL·XA 설계 재검토](dev7-review-002.md) — Oracle·ARIES·실제 로컬 소스 revision, 네 계약 반례와 DESIGN-002 보완·독립 검토 상태; 제품 인수 아님

- [DEV7 — Oracle·Altibase·로컬 DBMS 조사와 설계 검토](dev7-source-research.md) — Oracle26ai·Altibase7.3 공식 문서와 실제 로컬8개 revision; 실행 비교·제품 인수 아님

- [상용 수준 확장 한계 조사와 선제 보완](commercial-extensibility-research.md) — Oracle·공개 로컬3개·SRC-L04 실제 revision/hash, DEV3/4 자원·진행·설정/복구 경계와 독립 검토; 제품 인수 증거 아님

- [DEV6 — Oracle·ARIES·로컬 DBMS 클라이언트 조사와 검토](dev6-source-research.md) — DEV6 설계·REVIEW-002 상태/취소/결과/버퍼 보완·인수 의무; CLIENT1 미실행

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

- [DEV2 남은 시험 실행 — 2026-10-19](dev2-completion-xadb-20261019-r1.json): 동결 source `47c84c22…`의 native 33셀×3회, 9개 30분 target, P0 9,604,608건, 하위 회귀와 변이/JSON 반례 완료. K0 후보의 새 확인·재현은 개선·한도 PASS이며, 일부 계측 비용 한도 FAIL로 DEV2 전체 인수는 INCOMPLETE. 최초 source 지문·빌드 불일치 실패도 보존.

- [DEV2 계측 비용·CPU 관측 수정 — 2026-10-19](dev2-instrumentation-fix-xadb-20261019-r1.json): ext4 kernel 99셀·하위 회귀·36개 필수 성능 시험 PASS, 최종 로컬 성능 인수 INCOMPLETE. 원래 guardrail 실패·탐색·quota 실패와 tmpfs 범위 정정 보존.

- [DEV2 삭제 후 새 시험과 일시 중지 — 2026-10-19](dev2-resumed-tests-xadb-20261019-r1.json): e3 소스의 11개30분 target 중 11개 PASS, 보완 복구·격리·36개 정답 검사. 6개 target 종료 뒤 Python 캐시로 생긴 집계기 FAIL은 독립 소스 대조와 구분해 보존. 사용자 요청으로 새 성능은 미실행, DEV2 인수 미완료.

- [DEV2 성능 후속 실행](dev2-performance-followup-xadb-20261020-r1.json) — 고정 소스9개 campaign·180run 정답 PASS, PF12/PF13 개별 완료와 PF04 비용 한도 FAIL; 동일 Off 대조40run은 진단 전용

- [DEV2 fsync·스케줄링 진단](dev2-fsync-diagnostic-xadb-20261020-r1.json) — 고정4run의 정답·추적 연결과 측정2,048개 fsync 시간 분해; 기존 PF04 실패를 유지하는 진단 전용 증거

- [DEV2 PF04 가설 검토](dev2-pf04-hypothesis-review-xadb-20261020-r1.json) — 기존 추적·설치 버전 커널 소스의 독립 대조, 추가 Arc 비용과 max_batch_time 설정 가설의 한계; 새 제품 실행·인수 승격 없음

- [DEV2 journal 대기 진단](dev2-journal-diagnostic-xadb-20261020-r1.json) — 고정 Off 1회 정답·512개 fsync/TID/단계 연결; 추적 영향·환경 확인 FAIL, 계측 없는 실행의 원인·성능 인수로 확대하지 않음

- [DEV2 journal binary 대응](dev2-journal-binary-mapping-xadb-20261020-r1.json) — 실행 커널과 build ID가 같은 ELF에서 두 stack 위치를 metadata·commit record 버퍼 대기로 대응; 추가 제품 실행과 성능 인수 승격 없음

- [DEV3 WAL·할당·행·DWB 표현 통합 원본 실행 결과](dev3-storage-formats-leaf-xadb-20261021-r1.json) — 원본 PASS, source e419f2d7…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 WAL·할당·행·DWB 표현 소스의 DEV2 P0/K0·DEV0/1 회귀](dev3-storage-formats-dev2-regression-xadb-20261021-r1.json) — 원본 PASS, source e419f2d7…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 catalog·undo·control 표현 통합 원본 실행 결과](dev3-metadata-formats-leaf-xadb-20261021-r1.json) — 원본 PASS, source f54bf874…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 catalog·undo·control 표현 소스의 DEV2 P0/K0·DEV0/1 회귀](dev3-metadata-formats-dev2-regression-xadb-20261021-r1.json) — 원본 PASS, source f54bf874…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 catalog·undo·control 표현 소스의 DEV2 복구 원본 실행 결과](dev3-metadata-formats-dev2-kernel-xadb-20261021-r1.json) — 원본 PASS, source f54bf874…; 전체 DEV3 disk 인수는 미완료.

- [DEV3 selector·표현·부분 모델 통합 원본 실행 결과](dev3-selector-leaf-xadb-20261021-r2.json) — 해당 보존 소스의 leaf 검증; DEV3 전체 인수 아님
- [DEV3 selector와 같은 소스의 DEV0/1·DEV2 하위 회귀](dev3-selector-dev2-regression-xadb-20261021-r2.json) — 해당 보존 소스의 leaf 검증; DEV3 전체 인수 아님
- [DEV3 selector와 같은 소스의 현재-build DEV2 복구 회귀](dev3-selector-dev2-kernel-xadb-20261021-r2.json) — 해당 보존 소스의 leaf 검증; DEV3 전체 인수 아님
- [DEV3 열린 handle metadata port 원본 실행 결과](dev3-metadata-port-xadb-20261021-r0.json) — 해당 보존 소스의 leaf 검증; DEV3 전체 인수 아님

- [DEV3 선택 page reader 통합 원본 실행 결과](dev3-page-reader-leaf-xadb-20261021-r2.json) — source8e86232e…의 고정 범위; DEV3 전체 인수 아님.

- [DEV3 선택 page reader 소스의 DEV0/1·DEV2 회귀](dev3-page-reader-dev2-regression-xadb-20261021-r2.json) — source8e86232e…의 고정 범위; DEV3 전체 인수 아님.

- [DEV3 선택 page reader 소스의 현재-build DEV2 복구](dev3-page-reader-dev2-kernel-xadb-20261021-r2.json) — source8e86232e…의 고정 범위; DEV3 전체 인수 아님.

- [DEV3 선택 page reader 소스의 Clippy·fmt·build feature 기록](dev3-page-reader-quality-xadb-20261021-r2.json) — source8e86232e…의 고정 범위; DEV3 전체 인수 아님.

- [DEV3 상태 home 표현의 국소 실행 결과](dev3-status-home-local-xadb-20261021-r0.json) — 고정된 해당 소스·검증 범위; 실제 상태 갱신·DEV3 복구 인수 아님.

- [DEV3 상태 home 통합 소스의 현재-build DEV2 복구](dev3-status-home-dev2-kernel-xadb-20261021-r1.json) — 고정된 해당 소스·검증 범위; 실제 상태 갱신·DEV3 복구 인수 아님.

- [DEV3 상태 home 통합 소스의 Clippy·fmt 실행 결과](dev3-status-home-quality-xadb-20261021-r1.json) — 고정된 해당 소스·검증 범위; 실제 상태 갱신·DEV3 복구 인수 아님.

- [DEV3 상태 home 표현 통합 원본 실행 결과](dev3-status-home-leaf-xadb-20261021-r1.json) — source dd7163fb…의 고정 범위; 실제 DEV3 복구·S1 인수 아님.

- [DEV3 상태 home 통합 소스의 DEV0/1·DEV2 회귀](dev3-status-home-dev2-regression-xadb-20261021-r1.json) — source dd7163fb…의 고정 범위; 실제 DEV3 복구·S1 인수 아님.

- [DEV3 obligation descriptor의 국소 실행 결과](dev3-obligation-local-xadb-20261021-r0.json) — 고정된 해당 소스·검증 범위; 실제 예약 인계·DEV3 복구 인수 아님.

- [DEV3 obligation descriptor 통합 소스의 현재-build DEV2 복구](dev3-obligation-dev2-kernel-xadb-20261021-r1.json) — 고정된 해당 소스·검증 범위; 실제 예약 인계·DEV3 복구 인수 아님.

- [DEV3 obligation descriptor 통합 소스의 Clippy·fmt 실행 결과](dev3-obligation-quality-xadb-20261021-r1.json) — 고정된 해당 소스·검증 범위; 실제 예약 인계·DEV3 복구 인수 아님.

- [DEV3 obligation descriptor 통합 원본 실행 결과](dev3-obligation-leaf-xadb-20261021-r2.json) — source 7dabd13a…의 고정 범위; 실제 DEV3 복구·S1 인수 아님.

- [DEV3 obligation descriptor 통합 소스의 DEV0/1·DEV2 회귀](dev3-obligation-dev2-regression-xadb-20261021-r2.json) — source 7dabd13a…의 고정 범위; 실제 DEV3 복구·S1 인수 아님.

- [DEV3 ownership 관측 국소 실행 결과](dev3-ownership-local-xadb-20261021-r2.json) — source31ab0f…의 고정 범위; 실제 DEV3 예약 복원·S1 인수 아님.

- [DEV3 ownership 소스의 DEV2 복구 실행 결과](dev3-ownership-dev2-kernel-xadb-20261021-r1.json) — source31ab0f…의 고정 범위; 실제 DEV3 예약 복원·S1 인수 아님.

- [DEV3 ownership 통합 원본 실행 결과](dev3-ownership-leaf-xadb-20261021-r1.json) — 동일 source의 제한된 실행 범위 PASS; 전체 DEV3 인수 미완료

- [DEV3 ownership 소스의 DEV0/1/2 회귀 실행 결과](dev3-ownership-dev2-regression-xadb-20261021-r1.json) — 동일 source의 제한된 실행 범위 PASS; 전체 DEV3 인수 미완료

- [DEV3 현재 intent 검산 국소 실행 결과](dev3-ownership-intent-local-xadb-20261021-r0.json) — 실제 profile/current intent의 국소 범위 및 같은 source의 기존 복구 회귀; 전체 DEV3 인수 미완료

- [DEV3 현재 intent 소스의 DEV2 복구 실행 결과](dev3-ownership-intent-dev2-kernel-xadb-20261021-r0.json) — 실제 profile/current intent의 국소 범위 및 같은 source의 기존 복구 회귀; 전체 DEV3 인수 미완료

- [DEV3 현재 intent 통합 원본 실행 결과](dev3-ownership-intent-leaf-xadb-20261021-r0.json) — source e83efc08…의 고정 범위 PASS; 실제 예약 복원·전체 DEV3 인수 미완료

- [DEV3 현재 intent 소스의 DEV0/1/2 회귀 실행 결과](dev3-ownership-intent-dev2-regression-xadb-20261021-r0.json) — source e83efc08…의 고정 범위 PASS; 실제 예약 복원·전체 DEV3 인수 미완료

- [DEV3 RollbackStart 결합 검산 국소 실행 결과](dev3-rollback-start-local-xadb-20261021-r1.json) — source99bec497…의 제한된 결합 관측·같은 build 복구 회귀; 실제 DEV3 적용·전체 인수 미완료

- [DEV3 RollbackStart 소스의 DEV2 복구 실행 결과](dev3-rollback-start-dev2-kernel-xadb-20261021-r0.json) — source99bec497…의 제한된 결합 관측·같은 build 복구 회귀; 실제 DEV3 적용·전체 인수 미완료

- [DEV3 RollbackStart 통합 원본 실행 결과](dev3-rollback-start-leaf-xadb-20261021-r0.json) — source99bec497…의 제한된 결합 관측·같은 build 하위 회귀; 실제 DEV3 적용·전체 인수 미완료

- [DEV3 RollbackStart 소스의 DEV0/1/2 회귀 실행 결과](dev3-rollback-start-dev2-regression-xadb-20261021-r0.json) — source99bec497…의 제한된 결합 관측·같은 build 하위 회귀; 실제 DEV3 적용·전체 인수 미완료

- [DEV3 읽기 버퍼 국소 원본 실행 결과](dev3-read-buffer-local-xadb-20261021-r2.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 읽기 버퍼 수명 compiler 원본 실행 결과](dev3-read-buffer-borrows-xadb-20261021-r1.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 읽기 버퍼 통합 원본 실행 결과](dev3-read-buffer-leaf-xadb-20261021-r1.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 읽기 버퍼 소스의 DEV0/1/2 회귀 실행 결과](dev3-read-buffer-dev2-regression-xadb-20261021-r1.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 읽기 버퍼 소스의 DEV2 현재-build 복구 실행 결과](dev3-read-buffer-dev2-kernel-xadb-20261021-r1.json) — source061c0d9e…의 selected clean page·pin·clock; dirty buffer·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 국소 원본 실행 결과](dev3-genesis-wal-local-xadb-20261021-r6.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL slot 수명 compiler 원본 결과](dev3-genesis-wal-borrows-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 실제 port trace·유한 모델 검산 원본](dev3-genesis-wal-models-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 통합 원본 실행 결과](dev3-genesis-wal-leaf-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 소스의 DEV0/1/2 회귀 원본](dev3-genesis-wal-dev2-regression-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL 소스의 DEV2 현재-build 복구 원본](dev3-genesis-wal-dev2-kernel-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 genesis WAL native 시험 격리 반복 원본](dev3-genesis-wal-parallel-native-xadb-20261021-r1.json) — source3162fc43…의 fresh genesis·한 slot physical append/sync; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 RollbackStart page 구성 국소 실행 원본](dev3-rollback-materialization-local-xadb-20261021-r1.json) — source3493fa22…의 unpublished status page 구성; live dirty frame·WAL apply·전체 DEV3 인수 미완료

- [DEV3 RollbackStart page 구성 통합 실행 원본](dev3-rollback-materialization-leaf-xadb-20261021-r0.json) — source3493fa22…의 unpublished status page 구성; live dirty frame·WAL apply·전체 DEV3 인수 미완료

- [DEV3 RollbackStart page 구성 소스의 DEV0/1/2 회귀 원본](dev3-rollback-materialization-dev2-regression-xadb-20261021-r0.json) — source3493fa22…의 unpublished status page 구성; live dirty frame·WAL apply·전체 DEV3 인수 미완료

- [DEV3 RollbackStart page 구성 소스의 DEV2 현재 빌드 복구 원본](dev3-rollback-materialization-dev2-kernel-xadb-20261021-r0.json) — source3493fa22…의 unpublished status page 구성; live dirty frame·WAL apply·전체 DEV3 인수 미완료

- [DEV3 RollbackStart WAL·page 생산 국소 실행 원본](dev3-rollback-producer-local-xadb-20261021-r0.json) — sourcef08f01d7…의 caller frame/page 생산; tag2 append·live 적용·전체 DEV3 인수 미완료

- [DEV3 RollbackStart WAL·page 생산 통합 실행 원본](dev3-rollback-producer-leaf-xadb-20261021-r0.json) — sourcef08f01d7…의 caller frame/page 생산; tag2 append·live 적용·전체 DEV3 인수 미완료

- [DEV3 RollbackStart WAL·page 생산 DEV0/1/2 회귀 원본](dev3-rollback-producer-dev2-regression-xadb-20261021-r0.json) — sourcef08f01d7…의 caller frame/page 생산; tag2 append·live 적용·전체 DEV3 인수 미완료

- [DEV3 RollbackStart WAL·page 생산 DEV2 현재 빌드 복구 원본](dev3-rollback-producer-dev2-kernel-xadb-20261021-r0.json) — sourcef08f01d7…의 caller frame/page 생산; tag2 append·live 적용·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 D2 검토 원본](dev3-storage-session-d2-xadb-20261021-r2.json) · [49개 동결 입력 원본](dev3-storage-session-inputs-xadb-20261021-r6.json) — 설계 수용이며 제품 실행 증거는 아님

- [DEV3 하위 회귀 aggregate 예산 제안 원본](dev3-storage-session-aggregate-proposal-xadb-20261021-r0.json) — 원래 source·결정/실행 상태를 보존하며 후속 실행 PASS를 대신하지 않는다.

- [DEV3 하위 회귀 aggregate 예산 독립 D2 원본](dev3-storage-session-aggregate-review-xadb-20261021-r0.json) — 원래 source·결정/실행 상태를 보존하며 후속 실행 PASS를 대신하지 않는다.

- [DEV3 최초 저장 세션 통합 r0 timeout FAIL 원본](dev3-storage-session-leaf-xadb-20261021-r0.json) — 원래 source·결정/실행 상태를 보존하며 후속 실행 PASS를 대신하지 않는다.

- [DEV3 하위 회귀 wrapper 독립 D1 원본](dev3-storage-session-aggregate-d1-xadb-20261021-r0.json) — 실제 timeout 포함 자기시험12 PASS; 새 전체 통합 검증 대체 아님

- [DEV3 최초 저장 세션 품질 검사 원본](dev3-storage-session-quality-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 소유권 compiler8 원본](dev3-storage-session-borrows-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 실제 효과·유한 durable 모델 원본](dev3-storage-session-model-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 실제 ext4 장애·SIGKILL 검산 원본](dev3-storage-session-native-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 통합 실행 원본](dev3-storage-session-leaf-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 DEV0/1/2 회귀 원본](dev3-storage-session-dev2-regression-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 DEV2 현재-build 복구 원본](dev3-storage-session-dev2-kernel-xadb-20261021-r1.json) — sourceedec6006…의 fresh 설치·공통 fence; 일반 recovery·전체 DEV3 인수 미완료

- [DEV3 최초 저장 세션 최종 독립 감사 원본](dev3-storage-session-review-xadb-20261021-r1.json) — sourceedec6006…의 제한된 구현·원본 실행·staged source 감사 PASS; 전체 DEV3 미완료

- [DEV3 최초 DWB 설치 독립 D2 결정 원본](dev3-doublewrite-install-d2-xadb-20261021-r9.json) — D2 설계·입력 채택이며 제품 실행 NOT_RUN

- [DEV3 최초 DWB 설치 r6 동결 입력 원본](dev3-doublewrite-install-inputs-xadb-20261021-r6.json) — D2 설계·입력 채택이며 제품 실행 NOT_RUN

- [DEV3 최초 DWB 설치 r8 보충 입력 원본](dev3-doublewrite-install-inputs-xadb-20261021-r8.json) — D2 설계·입력 채택이며 제품 실행 NOT_RUN

- [DEV3 최초 DWB 설치 runtime10·native3·compiler9 및 품질 실행 원본](dev3-doublewrite-install-local-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 하위 회귀 중단으로 FAIL한 통합 실행 원본](dev3-doublewrite-install-leaf-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 소유권 compiler9 원본](dev3-doublewrite-install-borrows-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 실제 trace·관측·유한 모델 및 checker19 원본](dev3-doublewrite-install-model-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 실제 ext4 설치·장애·종료 및 checker12 원본](dev3-doublewrite-install-native-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 사용자 지시에 따른 당시 하위 회귀 중단 원본](dev3-doublewrite-install-user-stop-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 하위 회귀 종료·kernel 미실행 원본](dev3-doublewrite-install-stop-terminal-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 국소 실행·소스 독립 D1 검토 원본](dev3-doublewrite-install-d1-local-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 완료한 개별 검증의 독립 D1 감사 원본](dev3-doublewrite-install-d1-partial-xadb-20261021-r2.json) — 실행 당시 상태와 identity 보존; 전체 DEV3 인수 증거 아님

- [DEV3 최초 DWB 설치 변경 범위 검증 정리](dev3-doublewrite-install-scoped-xadb-20261021-r2.json) — 기존 runtime10·native3·compiler9·checker31 결과; 통합 원본 FAIL과 미실행 범위 구분, 새 실행 아님

- [DEV3 최초 DWB 설치 최종 독립 D1 감사 원본](dev3-doublewrite-install-review-xadb-20261021-r2.json) — 기존 r12/r13 결과와 source 동등성 재사용; 새 테스트 없이 제한 설치 단위 commit 수용

- [DEV3 페이지 예약·취소 D2 채택 원본](dev3-allocation-reservation-d2-xadb-20261021-r2.json) — 독립 raw63·의미 반례541·compiler 입력6 동결; 제품 시험 미실행

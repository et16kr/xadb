# 현재 구현

2026-10-22 [DEV3 현행 WAL prefix 분석](dev3-implementation.md#recovery-prefix): 실제 reserve/cancel·Status INIT의 canonical RAM 상태와 잔여 의무를 닫힌 owner에 재구성했다. [source별 검증](../evidence/dev3-recovery-prefix-20261022.md)은 새 unit231의 완료 단언(완결 job223·실패 job 내8), native31·compiler8+control1·checker15·변경 수신6을 구별한다. 제한 단위의 최종 독립 D1을 수용했으며 실제 Pending 수리·물리 REDO·전체 DEV3/S5는 남아 있다.

2026-10-22 [DEV3 첫 복구 입력 선택](dev3-implementation.md#recovery-select): 초기 control·exclusive lease·header-only WAL의 [닫힌 선택](../contracts/dev3-recovery-select.md)을 구현하고 [source별 선택 인수](../evidence/dev3-recovery-select-20261022.md)와 독립 D1을 수용했다. BOOT_RESERVE·nonempty 복원·일반 recovery/READY·S5는 미완료다. 아래 종료·인계는 당시 기록으로 보존한다.

2026-10-22 [DEV3 작은 단위 종료·인계](dev3-implementation.md#paused-after-status-home-flush): [Status home flush](dev3-implementation.md#status-home-flush)의 제한 인수와 commit을 마쳤다. 복구 진입은 사전 열람 상태로 보류하며 새 하위 작업을 시작하지 않는다. 전체 DEV3·최종 S5는 미완료다.

2026-10-22 [DEV3 폐쇄 두-frame Status INIT](dev3-implementation.md#status-init): 실제 reserve→두 RAM image 공개→동일 WAL requirement의 durable 확인과 두-pin reader를 연결했다. 필수93·shared1·compiler13·native3의 source별 결과와 원 FAIL을 보존한다. 제한 단위의 최종 독립 D1을 수용했으며 Status home flush·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 불변 Status bootstrap](dev3-implementation.md#status-bootstrap): plan2 영속 역할과 typed borrow·old adapter 거절을 구현했다. 새 필수32·변경 parser1·native1·compiler semantic3와 별도 control1을 검증했다. 제한 단위의 최종 독립 검토를 수용했으며 INIT·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 allocator 한 페이지 flush](dev3-implementation.md#allocator-flush): 실제 capture·WAL force·DWB/home·whole retirement·조건부 RAM finalize를 연결했다. 필수76의 source별 범위·native3·compiler12·WAL30과 자원 전제를 검증했으며 원 FAIL/관측 불완전을 보존한다. 제한 단위의 독립 검토를 수용했으며 INIT·일반 recovery·전체 DEV3는 미완료다.

2026-10-22 [DEV3 첫 dirty frame·stage 소비자](dev3-implementation.md#dirty-stage): 실제 allocator1의 pin/content·DPT·first recLSN과 guarded reserve/cancel을 연결했다. mandatory53의 source별 범위·compiler18·native3을 검증했고 INIT·flush·일반 recovery와 전체 DEV3는 미완료다.

2026-10-22 [DEV3 적용 admission·version gate](dev3-implementation.md#concurrency-admission): 3.10.1.a.1의 실제34-owner·gate와 신규 runtime7그룹/45사례·compiler12를 연결했다. 최종 단위 판정은 인수 기록을 따르며 dirty/Commit 통합과 전체 DEV3는 미완료다.

2026-10-22 [DEV3 예약·취소 잔여 인수](dev3-implementation.md#allocation-reservation-accepted): native13DB·compiler baseline+6·관련 WAL4 및64KiB stack 경로를 검증하고 원래 실패와 source별 결과를 보존했다. 제한 단위의 독립 판정은 인수 기록을 따르며 전체 DEV3/S5는 미완료다.

2026-10-22 [DEV3 분류별 현재 상태](dev3-implementation.md#dev3-work-breakdown)와 [잔여 작업 계획](../plans/dev3-implementation-plan.md#remaining-work)을 연결했다. 예약·취소의 남은 검증·64KiB 증명 및 [작업 크기·우선순위·병렬 배치](../plans/dev3-implementation-plan.md#work-organization)를 명시했다. 분류 수로 진행률을 계산하지 않으며 새 제품 실행·완료 승격은 없다.

2026-10-21 [DEV3 페이지 예약·취소 부분 구현 및 중단](dev3-implementation.md#allocation-reservation-partial): 메모리 포트의 새 경로와 검토 수정만 제한 검증했다. native/compiler·변경 WAL 관련4시험·전체 단위 인수는 남겨 두고 사용자 요청으로 중단한다. [하부 작업 16개 분류 초안](dev3-implementation.md#dev3-work-breakdown)으로 진행·잔여 범위를 구분한다.

2026-10-21 [DEV3 최초 doublewrite 설치](dev3-implementation.md#doublewrite-install-results): XDI1·고정 images 공간·Empty DWB 선택을 구현했다. 기존 동일 source의 runtime10·native3·compiler9·checker31 및 Clippy/fmt 결과를 보존했고 수정 없는 코드는 재시험하지 않는다. 마지막 하위 회귀를 사용자 지시로 중단한 통합 원본은 FAIL이며 dirty/home/recovery·전체 DEV3는 미완료다.

2026-10-21 [DEV3 최초 저장 세션](dev3-implementation.md#storage-session-results): extent·FILEMAP 설치와 reader/WAL 공통 fence를 구현했다. 같은 source의 runtime12·native3·compiler8·leaf59·하위28·현재-build DEV2 복구18check와 Clippy/fmt PASS이며 dirty buffer·home write·일반 복구·전체 DEV3는 미완료다.

2026-10-21 [DEV3 RollbackStart WAL·page 생산](dev3-implementation.md#rollback-producer-results): typed before 상태에서 caller frame과 상태 page를 구성했다. 같은 source의 semantic22·WAL13·leaf54·하위 회귀28·DEV2 복구18check와 Clippy/fmt PASS이며 tag2 append·live 적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 RollbackStart page 구성](dev3-implementation.md#rollback-materialization-results): 검증된 WAL과 before page로 caller scratch의 after page를 구성했다. 같은 source의 관련17시험·leaf54·하위 회귀28·DEV2 복구18check와 Clippy/fmt PASS이며 live dirty frame·WAL 적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 fresh genesis WAL](dev3-implementation.md#genesis-wal-results): 첫 epoch·WRITER 설치와 한 slot의 physical write/sync를 구현했다. 같은 source의 runtime10·native3·compiler4·유한 모델·leaf54·하위 회귀28·DEV2 복구18check가 PASS이며 일반 recovery·dirty page·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 selected clean page buffer](dev3-implementation.md#read-buffer-results): 실제 reader 위에 bounded frame·pin·clock 교체를 구현했다. 같은 source의 leaf49·하위 회귀28·DEV2 복구18check(native99셀)와 수명 compiler·Clippy/fmt PASS이며 dirty buffer·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 RollbackStart 결합 검산](dev3-implementation.md#rollback-start-results): 거래 descriptor·상태 page·WAL의 전체 rollback 시작 전이를 검산했다. 같은 source의 leaf44·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 실제 append/적용·전체 DEV3 인수는 미완료다.

2026-10-21 [DEV3 현재 growth intent 검산](dev3-implementation.md#ownership-intent-results): 실제 Profile과 inventory를 결합하고 owner별 현재 intent 중복을 고정 공간으로 거절한다. 같은 source의 leaf43·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 실제 예약 복원·DEV3 인수는 미완료다.

2026-10-21 [DEV3 ownership 관측](dev3-implementation.md#ownership-observation-results): 고정 공간으로 inventory bytes·owner별 합계·물리 충돌을 검증했다. 같은 소스의 leaf43·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 intent 유일성·실제 예약 복원·DEV3 인수는 미완료다.

2026-10-21 [DEV3 상태 home·obligation descriptor](dev3-implementation.md#obligation-descriptor-results): incoming count와 복구 budget의 고정 표현을 구현·독립 검토했다. 같은 통합 소스의 leaf42·하위 회귀28·DEV2 복구18check(native99셀)·Clippy/fmt PASS이며 실제 예약 복원·DEV3 인수는 미완료다.

2026-10-21 [DEV3 재개·선택 page 읽기](dev3-implementation.md#page-reader-results): selector와 열린 handle metadata를 연결하여 실제 선택 page 읽기를 검증했다. 같은 보존 소스의 leaf·하위 회귀·DEV2 복구99셀 PASS이며 전체 DEV3 인수는 계속 미완료다.

2026-10-20 [DEV3 첫 leaf](dev3-implementation.md#validation): 내부 page envelope codec과 D3-M02-P01 부분 모델을 구현·독립 검토했다. page7·모델 정상4/결함8·DEV0/1/2 회귀와 같은 소스의 DEV2 복구99셀 PASS. 실제 디스크 경로·전체 모델·S1 인수는 미완료다.

2026-10-20 [DEV2 journal binary 대응](dev2-implementation.md#dev2-journal-binary-mapping-20261020): 설치 커널의 build ID와 기계어 흐름으로 두 대기 위치를 metadata·commit record 버퍼에 대응했다. 추가 측정 없이 코드 위치만 구분했으며 PF04·DEV2 인수는 미완료다.

2026-10-20 [DEV2 journal 대기 진단](dev2-implementation.md#dev2-journal-diagnostic-20261020): 1회 정답 PASS와 측정 512개 fsync의 실제 journal 대기 TID·단계 연결을 확인했다. 추적 영향·환경 확인 조건 FAIL과 기존 PF04 실패를 유지하며 DEV2/B2/D0 최종 인수는 미완료다.

2026-10-20 [DEV2 PF04 가설 검토](dev2-implementation.md#dev2-pf04-hypothesis-review-20261020): 새 제품 실행 없이 기존 추적과 커널 소스를 독립 대조했다. 설정 변경이나 추가 후보의 효과를 확정하지 않았으며, PF04와 DEV2/B2/D0 최종 인수는 미완료다.

2026-10-20 [DEV2 fsync 진단](dev2-implementation.md#dev2-fsync-diagnostic-20261020): Off/On/On/Off 4개 실행의 정답과 2,048개 측정 fsync 연결을 확인했다. 이 추적 결과는 PF04 인수 결과를 대체하지 않으며, 원래 네 campaign의 비용 한도 FAIL과 DEV2 미완료를 유지한다.

2026-10-20 [DEV2 성능 후속 실행](dev2-implementation.md#dev2-performance-followup-20261020): 고정 소스의 9개 campaign·180개 run 정답 PASS. PF12 국소 개선·PF13 재현 기능을 개별 검증 완료했으며, PF04 계측 비용 한도와 DEV2/B2/D0 최종 인수가 남아 있다.

2026-10-19 [DEV2 계측 수정](dev2-implementation.md#dev2-instrumentation-fix-20261019): CPU 관측과 상세 진단 비용을 수정하고 같은 동결 source에서 ext4 복구 99셀·하위 회귀·필수 성능 시험 36개를 통과했다. 최종 로컬 성능 인수는 INCOMPLETE이며 전체 DEV2 인수는 미완료다.

2026-10-19 [DEV2 남은 로컬 시험](dev2-implementation.md#dev2-tests-20261019)을 완료했다. 영속 99셀·9개 30분 target·P0 전수·하위 회귀와 K0 후보의 확인/재현 결과를 새 receipt에 연결했다. 일부 계측 비용 한도 FAIL로 전체 인수는 INCOMPLETE다.

2026-10-18 · **이관한 실제 코드 / XADB 실행 결과는 아래 증거 참조**

| 범위 | 현재 상태 | 상세 |
| --- | --- | --- |
| DEV0 | controller·fixture worker·receipt·artifact·cleanup 구현 | [DEV0](dev0-implementation.md) |
| DEV1 | 최소 ID·값·오류·codec·quota·clock/scheduler·Linux 파일 효과 구현 | [DEV1](dev1-implementation.md) |
| DEV2 P0 | 독립 history 모델과 known-bad 거절 구현 | [DEV2](dev2-implementation.md) |
| DEV2 K0 일부 | volatile 행/catalog·snapshot·ID·quota·frame·native worker 구현 | [DEV2](dev2-implementation.md) |
| 외부 streaming controller | `xadbtest::native`의 부정 시험·실제 worker 연결 로컬 검증 | [DEV2](dev2-implementation.md) |
| 순차 catalog oracle·recipe | 독립 기대값·실제 이력·저장 recipe 실행/재검증 로컬 검증 | [DEV2](dev2-implementation.md) |
| K0 성능 진단·audit/replay | Off/Always/Detailed와 독립 정답·통계·원시 bundle 검증 구현; 최초 p99 비용 한도 FAIL, PF 인수 미완료 | [DEV2](dev2-implementation.md) |
| COMM 핵심 | frame·Linux local socket·quota·deadline·취소/종료·focused runner 구현, C1 인수 미완료 | [COMM](comm-implementation.md) |
| 영속 transaction/WAL/recovery | 구현·정적 통합 완료, 최종 인수 대기 | [DEV2 K1–K5](dev2-implementation.md) |
| DEV3 저장 표현·선택 page 읽기·상태 모델 | 고정 표현·bounded 읽기 leaf 검증, 실제 쓰기·복구·S1 인수 미완료 | [DEV3](dev3-implementation.md) |
| SQL/server/client·replication | 미구현 | DEV4 이후 |

[기계 판독 상태](implementation-status.json) · [새 실행 증거](../evidence/README.md) · [이관 기록](../history/project-migration.md)

기계 판독 상태의 `evidence_refs`는 현재 소스 지문과 일치하는 검증 주장만 담는다. `historical_evidence_refs`는 원래 소스·명령·check 결과를 그대로 보존한 이전 소스의 증거이며, 현재 소스 검증을 대신하지 않는다. `PREVIOUS_SOURCE_VERIFIED`는 이 구분을 표시한다. 영속 커널의 `IMPLEMENTED_VALIDATION_PENDING`은 코드 구현과 부분 개발 시험이 있어도 최종 인수가 남았다는 상태다. 계획표의 기존 로컬 개발 선행권한이나 DEV2 완료·제품·릴리스 인수 조건을 변경하지 않는다.

P0/K0 시험 성공은 B2/D0나 DEV2 전체 완료를 의미하지 않는다. e3 동결 소스의 bounded concurrent 전수·영속 복구·11개 V3와 PF12/PF13 결과는 위 후속 기록에 연결한다. PF04 계측 비용 한도와 최종 인수는 미완료이며 과거 source snapshot의 PASS를 전체 현재 소스의 PASS로 취급하지 않는다.

<!-- catalogue -->

- [DEV3 구현·검증 현황](dev3-implementation.md)

- [COMM 구현 기록](comm-implementation.md)

- [DEV0 — Rust 빌드와 XADBTest 최소 구현](dev0-implementation.md)
- [DEV1 — 공통 의미와 Linux effect 기반](dev1-implementation.md)
- [DEV2 구현 기록](dev2-implementation.md)
- [implementation-status.json](implementation-status.json)

K0-004 순차 catalog 판정·recipe 실행/재검증과 K0-005 병렬 구현·검증의 현재 계약은 [DEV2 기록](dev2-implementation.md)을 따른다.

K0-005 통합 후 새 r3 실행에서 DEV0 8개·DEV1 8개·DEV2 leaf 28개 check가 PASS했다. 성능 진단 r1은 고정 시험35개와60개 release 실행·원시 재검증을 확인한다. 최초 두 비교의 sample-p99 guardrail FAIL은 유지하며 PF 인수를 완료로 표시하지 않는다. [현재 증거](../evidence/README.md)는 실행 전후 같은 source hash에 적용한다. 과거 r1·r2와 중단된 실행을 현재 코드의 PASS로 재사용하지 않는다.

COMM 반영 당시 commit 소스의 DEV0/DEV1/DEV2 회귀 참조는 [증거 색인](../evidence/README.md)의 COMM 보고서로 연결했다. 그 실행 전후 source 지문은 당시 문서 commit 대상의 지문과 일치했다. 이 보고서와 앞 문단의 r3·성능 보고서는 각각 당시 source의 기록으로 보존하며, 현재 DEV2 변경의 검증으로 재사용하지 않는다.

## XADB-EVIDENCE-SCOPE-001 — 현재 검증과 이전 소스 증거

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 근거·범위 | 사용자 문서 재검사·수정·commit 요청. 기준 `8550f55b2829a3792614e2853d19060bdc1d231c`의 push 검사에서 영속 상태 거절 1건과 이전 DEV0/DEV1/DEV2 source 지문 불일치 3건 재현 |
| 선택·대안 | 구현 상태에 최종 검증 대기를 명시하고 현재/이전 소스의 증거 참조를 분리. 이전 보고서의 hash를 현재 값으로 고치거나 구현 상태를 미구현으로 되돌리는 대안은 실제 상태·증거를 왜곡하므로 제외 |
| 유지 조건 | 현재 검증 주장은 현재 source 지문 일치가 필요. 이전 증거도 원래 실행 전후 hash·PASS·필수 check·exit code 검사를 유지. 기존 P0/K0 보고서로 내구성·DEV2 완료·제품/릴리스를 승격하지 않음 |
| 구현·독립 검토·통합 | `Complete DEV2` 작업의 `/root/dev2_completion_review`가 검사기·반례·상태 분리를 구현. 별도 실행인 이 문서 수정 작업의 `/root`(chat `01a14f79-b353-7c33-819a-147676267378`)가 계약·실제 diff·원래 실패와 반례 결과를 독립 검토하고 채택. 추가 README 문구는 같은 증거 범위에 맞춘 D0 정정 |
| 소유·병합 | 위 구현의 미커밋 문서 수정 4개를 이어받아 문서·검사기만 통합. 제품 코드·장시간 DEV2 검증은 기존 작업 소유. 기존 파일과 색인을 사용하며 원본 실행 보고서의 byte·ID·경로·hash는 유지 |
| 검증 | 사용자 요청에 따라 현재 작업 트리의 문서 검사 58개 PASS, `python3 -B -m unittest discover -s devdoc -p 'test_*.py' -v` 33개 PASS. 이전 증거를 현재 PASS로 바꾸기, 내부 hash 불일치, 실패/누락 check, 참조 중복, 미선언 보고서와 완료 주장 거절을 확인. 제품 시험 결과가 아님 |
| 대체·재검토 | 문서 검사기의 K0 미구현 전용 상태 제한과 모든 참조를 현재 소스로 취급하던 방식을 대체. 새 영속 보고서 profile·현재 source 검증·단계 완료 상태를 도입할 때 재검토 |

# 현재 구현

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
| disk pages/index·SQL/server/client·replication | 미구현 | DEV3 이후 |

[기계 판독 상태](implementation-status.json) · [새 실행 증거](../evidence/README.md) · [이관 기록](../history/project-migration.md)

기계 판독 상태의 `evidence_refs`는 현재 소스 지문과 일치하는 검증 주장만 담는다. `historical_evidence_refs`는 원래 소스·명령·check 결과를 그대로 보존한 이전 소스의 증거이며, 현재 소스 검증을 대신하지 않는다. `PREVIOUS_SOURCE_VERIFIED`는 이 구분을 표시한다. 영속 커널의 `IMPLEMENTED_VALIDATION_PENDING`은 코드 구현과 부분 개발 시험이 있어도 최종 인수가 남았다는 상태다. 계획표의 기존 로컬 개발 선행권한이나 DEV2 완료·제품·릴리스 인수 조건을 변경하지 않는다.

P0/K0 시험 성공은 B2/D0나 DEV2 전체 완료를 의미하지 않는다. e3 동결 소스의 bounded concurrent 전수·영속 복구·11개 V3와 PF12/PF13 결과는 위 후속 기록에 연결한다. PF04 계측 비용 한도와 최종 인수는 미완료이며 과거 source snapshot의 PASS를 전체 현재 소스의 PASS로 취급하지 않는다.

<!-- catalogue -->

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

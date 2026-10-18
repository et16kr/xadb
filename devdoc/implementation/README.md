# 현재 구현

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
| 영속 transaction/WAL/recovery | 미구현 | DEV2 K1–K5 |
| disk pages/index·SQL/server/client·replication | 미구현 | DEV3 이후 |

[기계 판독 상태](implementation-status.json) · [새 실행 증거](../evidence/README.md) · [이관 기록](../history/project-migration.md)

P0/K0 시험 성공은 B2/D0나 DEV2 전체 완료를 의미하지 않는다. bounded concurrent exploration, 통합 PF 인수, 영속 경로는 남은 작업이다. 과거 source snapshot의 PASS를 새 XADB의 PASS로 취급하지 않는다.

<!-- catalogue -->

- [COMM 구현 기록](comm-implementation.md)

- [DEV0 — Rust 빌드와 XADBTest 최소 구현](dev0-implementation.md)
- [DEV1 — 공통 의미와 Linux effect 기반](dev1-implementation.md)
- [DEV2 구현 기록](dev2-implementation.md)
- [implementation-status.json](implementation-status.json)

K0-004 순차 catalog 판정·recipe 실행/재검증과 K0-005 병렬 구현·검증의 현재 계약은 [DEV2 기록](dev2-implementation.md)을 따른다.

K0-005 통합 후 새 r3 실행에서 DEV0 8개·DEV1 8개·DEV2 leaf 28개 check가 PASS했다. 성능 진단 r1은 고정 시험35개와60개 release 실행·원시 재검증을 확인한다. 최초 두 비교의 sample-p99 guardrail FAIL은 유지하며 PF 인수를 완료로 표시하지 않는다. [현재 증거](../evidence/README.md)는 실행 전후 같은 source hash에 적용한다. 과거 r1·r2와 중단된 실행을 현재 코드의 PASS로 재사용하지 않는다.

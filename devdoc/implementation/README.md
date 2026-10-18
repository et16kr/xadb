# 현재 구현

2026-10-18 · **이관한 실제 코드 / XADB 실행 결과는 아래 증거 참조**

| 범위 | 현재 상태 | 상세 |
| --- | --- | --- |
| DEV0 | controller·fixture worker·receipt·artifact·cleanup 구현 | [DEV0](dev0-implementation.md) |
| DEV1 | 최소 ID·값·오류·codec·quota·clock/scheduler·Linux 파일 효과 구현 | [DEV1](dev1-implementation.md) |
| DEV2 P0 | 독립 history 모델과 known-bad 거절 구현 | [DEV2](dev2-implementation.md) |
| DEV2 K0 일부 | volatile 행/catalog·snapshot·ID·quota·frame·native worker 구현 | [DEV2](dev2-implementation.md) |
| 외부 streaming controller | `xadbtest::native` 초안 코드 이관, 전용 인수 시험 미실행 | [DEV2](dev2-implementation.md) |
| 영속 transaction/WAL/recovery | 미구현 | DEV2 K1–K5 |
| disk pages/index·SQL/server/client·replication | 미구현 | DEV3 이후 |

[기계 판독 상태](implementation-status.json) · [새 실행 증거](../evidence/README.md) · [이관 기록](../history/project-migration.md)

P0/K0 시험 성공은 B2/D0나 DEV2 전체 완료를 의미하지 않는다. 외부 streaming history controller, 통합 PF 인수, 영속 경로는 남은 작업이다. 과거 source snapshot의 PASS를 새 XADB의 PASS로 취급하지 않는다.

<!-- catalogue -->

- [DEV0 — Rust 빌드와 XADBTest 최소 구현](dev0-implementation.md)
- [DEV1 — 공통 의미와 Linux effect 기반](dev1-implementation.md)
- [DEV2 구현 기록](dev2-implementation.md)
- [implementation-status.json](implementation-status.json)

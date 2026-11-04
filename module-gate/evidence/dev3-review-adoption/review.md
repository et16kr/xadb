# DEV3 리뷰 채택 계획 독립 D2 검토

2026-11-04 · r1 · **ACCEPT — 열린 필수 지적 0**

- 작성·계약·통합 담당: `/root`.
- 별도 검토 실행: `/root/review_adoption_d2` (GPT-6 Astra 배정). 작성자 결론만으로 판단하지 않고 working-tree diff, owner 계약, 기존 증거와 관련 코드를 직접 읽었다.
- 기준 HEAD: `d331ed4d2fbb85646ca23c93b2543e78b7f82636` (PR #50 병합), branch `codex/dev3-review-adoption-plan`.
- 제품 source: `2397633015e7410bbc591c81095ba49f621073ca` (PR #49). 두 commit 사이 diff는 문서 3개뿐임을 확인했다.
- 범위: 아래 Markdown 13개에서 REVIEW-012 A0–A7, owner 의무, D3-ADOPT-01–07, F6 순서 대체와 V1 일정 경계를 검토했다. 전체 repository 감사나 실제 구현의 인수가 아니다.

## 판단 근거

1. PR #48의 R1–R11/S1–S8과 PR #50의 보정을 대조했다. ITL/CTS, group commit, statement restart, SI, 단순 namespace/header 교체 restore를 자동 채택하지 않는다. 현재 Serializable·RC abort·deadlock·cursor·retention·UNIQUE 의미와 256 mutation/1 MiB 한도를 유지한다.
2. M0의 원 source·API read/hash 계수와 M1 NOT_RUN을 확인했다. `history.rs::replay_durable`의 두 prefix hash와 reconstruct, Row replay의 native 의미 입력, Row write의 TerminalReadGate 소유를 실제 코드에서 대조했다. A1은 prefix 신뢰·손상 검출 책임·authority 무효화·reopen·F05를 상세 설계의 필수 입력으로 남겨 검사를 무조건 삭제하지 않는다.
3. 3.04의 stage/sync·Commit 설치 장벽과 3.10의 snapshot/pin·격리 owner를 대조했다. A3/A4는 same-page Commit@c/c+1 반례, WAL-before-data, ACK/UNKNOWN, 실제 writer 소유와 reader 차단, 겹치는 자원 peak를 함께 다루며 Root 분산이나 batch만으로 해결을 선언하지 않는다.
4. A5의 DEV4 대상 물질화 연결은 기존 `dev4-sql-design.md#execution`의 동일 statement token·opaque observation·bounded 목록과 맞고, SQL 실행기 시험은 DEV4에 남긴다. A6는 immutable capture의 같은-cut/pin과 audit의 재순회 비용·spill 금지·typed rewrite·retained credit·두 번째 reopen 의무를 보존한다.
5. A1 상세 설계가 첫 후속이며 전체 A2–A7 완료를 일반 DML의 일괄 선행으로 만들지 않는다. F6의 측정 뒤 조건부 설계 순서는 양방향 연결로 명시 대체했고 M1 실행 권한/결과와 구분한다. 후보 구현 PR 전에 범위·예상일·불확실성·선행을 재배정하는 조건이 있다.
6. V1 33/86일, 38.37%, 미적립53일은 보존한다. 과거12-30 예측이나53일을 새 ETA로 재사용하지 않고 계획 자체 적립은0이다. 제한 UPDATE 완료와 일반화 미완료를 구분한다.
7. 새 인수 family는 전 행 NOT_RUN이며 exact selector·독립 oracle·fault schedule·예산 동결을 후속 의무로 둔다. 작은 변경별 검증과 S5 확대를 중복 집계하지 않고 기존 S5 inventory를 축소하지 않는다. 새 문서 경로 없이 owner에 통합했다.

## 참조 확인과 실행 제한

`review-adoption-20261104`, `task-review-012`, `immutable-capture-source`, `namespace-rewrite`, `qualification-profile-preparation`, DEV4 `execution` 및 모델 역할 anchor를 관련 문서에서 수동 확인했다. 기존 F6/M0, 두 리뷰, 3.04/3.10/3.13/3.15, 자원·qualification·일정·시험 owner의 해당 절과 AGENTS/DP01/DP02/문서 정책을 읽었다. 문서 verifier·제품 시험·모델 시험·benchmark·build·Clippy는 실행하지 않았다. 기존 결과를 새 PASS로 승격하지 않는다. pre-push 검사는 통합 담당의 후속이며 이 검토의 증거가 아니다.

## 검토한 working-tree blob

아래 hash는 `git hash-object`의 읽기 전용 결과다. 후속 상태/리뷰 링크 변경은 별도 delta 재검토로 연결한다.

| 경로 | blob |
| --- | --- |
| `devdoc/README.md` | `7f59d1c11bdad59d7f310828d168a4d5bd3a3124` |
| `devdoc/contracts/dev3-resource-bounds.md` | `07b7cf99981b6f9fecbc4413b7a81b02e0db3651` |
| `devdoc/contracts/dev3-task-04-wal.md` | `395ddada14fd0487ac78989fcfafc392c0fa9cc3` |
| `devdoc/contracts/dev3-task-10-concurrency.md` | `5797e5d9809a4325d424dd96a1939ddc61b5f8db` |
| `devdoc/contracts/dev3-task-13-checkpoint.md` | `323d0336237a34b93d9e81a17240baac6c850d04` |
| `devdoc/contracts/dev3-task-15-backup-integrity.md` | `46d6b25b9ed2c931aed5f5f6b5a1dd6c07dc3895` |
| `devdoc/contracts/dev3-task-16-qualification.md` | `a2969e21fb8ca62635bbaea2b1e7e648b8512914` |
| `devdoc/contracts/disk-storage-design.md` | `227245539d0d58addee0994dd68fac4f8d1d44e2` |
| `devdoc/plans/README.md` | `a80881ad16ceaf730b5a18650b745c954d6838ca` |
| `devdoc/plans/dev3-implementation-plan.md` | `f3de86dad70c8a973a94de86f189727b41f9a317` |
| `devdoc/plans/dev3-schedule-baseline.md` | `131b520de6e750b3b787f7cb53f9bffebc9c4513` |
| `devdoc/plans/dev3-test-plan.md` | `9065166adab1386bae8223ea07e1bd4adc4423a5` |
| `devdoc/plans/module-independence-followup-plan.md` | `1cea87ee332a23fc376de7b8544eaacce31d0700` |

## r2 최종 상태·링크 delta 재검토

2026-11-04 · `/root/review_adoption_d2` · **최종 ACCEPT — 열린 필수 지적 0**.

r1 이후 변경은 `dev3-implementation-plan.md`의 REVIEW-012 상태와 마지막 검토/owner 확인 문단, `disk-storage-design.md`의 REVIEW-012 상태뿐임을 확인했다. 해당 문장을 r1 내용으로 되돌린 메모리 입력의 blob이 r1 hash와 같으며 나머지11개 문서는 동일하다. 이 확인은 읽기 전용 hash 대조이며 제품 시험·자동 문서 검사 실행이 아니다.

독립 검토의 실제 identity·결과와 상대 review 링크가 맞고 `/root`의 owner/통합 반영 확인을 기록했다. ACCEPTED는 이번 계획의 채택이며 A1–A7 상세 설계·제품 구현·시험의 완료가 아니다. NOT_RUN과 후속 pre-push 결과의 분리도 유지한다. 추가 필수 수정 없음. 검토자는 이 로그만 갱신했고 commit/push를 하지 않았다.

최종 승인 범위의13개 blob:

| 경로 | blob |
| --- | --- |
| `devdoc/README.md` | `7f59d1c11bdad59d7f310828d168a4d5bd3a3124` |
| `devdoc/contracts/dev3-resource-bounds.md` | `07b7cf99981b6f9fecbc4413b7a81b02e0db3651` |
| `devdoc/contracts/dev3-task-04-wal.md` | `395ddada14fd0487ac78989fcfafc392c0fa9cc3` |
| `devdoc/contracts/dev3-task-10-concurrency.md` | `5797e5d9809a4325d424dd96a1939ddc61b5f8db` |
| `devdoc/contracts/dev3-task-13-checkpoint.md` | `323d0336237a34b93d9e81a17240baac6c850d04` |
| `devdoc/contracts/dev3-task-15-backup-integrity.md` | `46d6b25b9ed2c931aed5f5f6b5a1dd6c07dc3895` |
| `devdoc/contracts/dev3-task-16-qualification.md` | `a2969e21fb8ca62635bbaea2b1e7e648b8512914` |
| `devdoc/contracts/disk-storage-design.md` | `b4d0d50bc22ca4716c566d90a7a6bdf4c7c42825` |
| `devdoc/plans/README.md` | `a80881ad16ceaf730b5a18650b745c954d6838ca` |
| `devdoc/plans/dev3-implementation-plan.md` | `110b72cabcfc6059f36e47d3adc9c8962e745780` |
| `devdoc/plans/dev3-schedule-baseline.md` | `131b520de6e750b3b787f7cb53f9bffebc9c4513` |
| `devdoc/plans/dev3-test-plan.md` | `9065166adab1386bae8223ea07e1bd4adc4423a5` |
| `devdoc/plans/module-independence-followup-plan.md` | `1cea87ee332a23fc376de7b8544eaacce31d0700` |

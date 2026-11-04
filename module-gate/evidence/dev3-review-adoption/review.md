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


<a id="comment-followup"></a>
## r3 PR #51 댓글 보강 독립 D2 검토

2026-11-04 · 별도 실행 `/root/pr51_comment_fix_review` · **ACCEPT — 열린 필수 지적 0**.

기준 HEAD는 `d8b4125d162fba9489a06e67d4e4599fff6516c7`, PR base는 `d331ed4d2fbb85646ca23c93b2543e78b7f82636`이다. 작성·통합 담당 `/root`와 별도 실행으로 현재 9개 문서 diff를 직접 검토했다. 원 PR의 나머지 4개 owner 문서도 base diff와 REVIEW-012 연결을 대조했으며 r2 blob과 같음을 확인했다. 아래 13개 blob을 이 문서 계획의 검토 범위로 묶을 수 있다. 원 r1/r2의 source 조사·실행 증거는 당시 범위로 보존하며 이번에 제품 코드를 새로 검증했다는 뜻은 아니다.

[PR #51 댓글](https://github.com/et16kr/xadb/pull/51)의 원문 5개와 관련 상위 리뷰를 직접 읽고, AGENTS·DP01/DP02·제품 방향·일정 §6.3·DEV7 D7-STORAGE 및 PR #48/#50 관련 근거와 대조했다.

| 댓글 | 독립 판단·반영 확인 |
| --- | --- |
| 4464798426 · A0 | 외부 리뷰의 Oracle 목표 해석만으로 현재 제품 방향을 변경할 수 없으며, 반대로 AI D2가 사용자 결정을 영구 대체할 수도 없다. 현재 계약을 작업 기준으로 유지하고 Oracle 대응 대안·보장·비용·권고를 일정11 구현 전, 더 이른 영향 leaf 동결 전 제출하도록 한 경계는 타당하다. D3 사용자 결정과 위임 안의 D2를 구별하므로 일상 상세화마다 불필요한 승인을 요구하지 않는다. |
| 4464798920 · 잠금 위치 | bounded 메모리 표·row TxnRef/ITL·혼합형의 후보와 결정 시점을 명시했다. VersionStamp와 거래 X lock을 구별하고 첫 변경 전·statement rollback 뒤 retained lock·split/DELETE/reuse·미존재 UNIQUE key를 반례로 연결했다. DEV7의 실제 목표는 무제한이 아니라4,096 mutation/64MiB이며 현재256/1MiB와 확장 비용을 함께 제출한다. |
| 4464799455 · ITL/CTS·column delta | 계획·자원 owner·인수표에서 필수 구현 미확정인 A2 비교 후보로 일치한다. 전체 before-image 기준선과 CLR·overflow/index·반복 변경·가변 길이/NULL·scratch/chain 비용을 보존한다. |
| 4464799937 · A1 종료 | 같은 live 상태/연산에서 과거 전체 WAL 길이 E에 비례하는 정상 연산별 hash/replay 제거가 명시됐다. base/checkpoint 전환의 상각·누적 비용까지 포함하므로 비용을 옮긴 것만으로 개선을 선언하지 않는다. 안전한 후보가 없으면 현재 보호는 유지하되 A1은 미완료이고 재설계/제한/일정 영향을 제출한다. 목표 완화·제품 보장 변경만 사용자 결정으로 올리는 처리는 DP01과 맞다. F6의 과거 현상 유지 판정과 새 개선 완료를 구별한다. |
| 4464800356 · 일정 | 선택지(b)를 README·보고 형식·일정 owner에 반영했다. A1 설계는 적립 밖이며 소요·대기는 ETA에 포함한다. 설계 PR에 제품 구현/M1/18-3 적립 산출물을 섞지 않고, 영향받는 적립 항목의 첫 산출물 PR 전 재배정하므로 §6.3과 충돌하지 않는다. 설계 소급 적립 금지·33/86일=38.37%·M1 NOT_RUN을 유지한다. |

원 PR의 checkpoint·audit·qualification·설계 정본 연결에서 같은-cut/pin, typed restore/retained credit, 손상 검출·자원 종료와 S5 인수 의무를 없애는 변화는 없다. 현재 의미 유지 문장은 후속 결정 전 기준이며 REVIEW-012 실행계획과 owner가 결정 경계를 명시한다. 전체 dialect 호환, RC/restart, 잠금 형식, WAL authority를 이 문서 보강만으로 구현·최종 채택하지 않는다.

제품 시험·모델·benchmark·build·Clippy·문서 verifier를 실행하지 않았다. 읽기 전용 diff/문서/hash 대조와 이 검토 로그 추가, 통합 담당이 허용한 실행계획 마지막 검토 대기 문구의 ACCEPT/링크 교체만 수행했다. 상태 교체는 실제 별도 검토 결과와 맞으며 실질 계약 변경이 아니다. pre-push gate와 module-gate record 갱신은 통합 담당의 후속이며 이번 검토 결과와 구분한다. 후속 상태·review 링크 변경은 delta 재검토로 연결한다.

검토한 최종 13개 문서 blob(이 중 9개가 댓글 보강 변경):

| 경로 | blob |
| --- | --- |
| `devdoc/README.md` | `a6f01436920edf9d324c959643212d543aa4314b` |
| `devdoc/contracts/dev3-resource-bounds.md` | `b47142b849b5eb71db2d1b09e8ac9759c7d06a7d` |
| `devdoc/contracts/dev3-task-04-wal.md` | `31e6701baa7ea045e3ac86528e9c5d24df87f3fe` |
| `devdoc/contracts/dev3-task-10-concurrency.md` | `39edaf02f3b37146863c5af03025d055a056ed4d` |
| `devdoc/contracts/dev3-task-13-checkpoint.md` | `323d0336237a34b93d9e81a17240baac6c850d04` |
| `devdoc/contracts/dev3-task-15-backup-integrity.md` | `46d6b25b9ed2c931aed5f5f6b5a1dd6c07dc3895` |
| `devdoc/contracts/dev3-task-16-qualification.md` | `a2969e21fb8ca62635bbaea2b1e7e648b8512914` |
| `devdoc/contracts/disk-storage-design.md` | `b4d0d50bc22ca4716c566d90a7a6bdf4c7c42825` |
| `devdoc/plans/README.md` | `cda3bc3fc517686c4439a718527723de194f4748` |
| `devdoc/plans/dev3-implementation-plan.md` | `1303d03e742b8f002528f105bb7753cc3d415c91` |
| `devdoc/plans/dev3-schedule-baseline.md` | `482e1eadd7e35fd768b82044abf67ea8a30e9bc4` |
| `devdoc/plans/dev3-test-plan.md` | `eb0ad4436331e5e148e579aa75bc3ee45c47edb0` |
| `devdoc/plans/module-independence-followup-plan.md` | `663b4d13170f7213a67698fd2b297499b6741b73` |

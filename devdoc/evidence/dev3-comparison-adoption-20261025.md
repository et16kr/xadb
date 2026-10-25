# DEV3 비교 분석의 설계·계획 반영

2026-10-25 · D2 · **D2 ACCEPTED / 문서 변경 / 제품·모델·성능 NOT_RUN**

[설계 정본](../contracts/disk-storage-design.md#task-review-011) · [계획](../plans/dev3-implementation-plan.md#comparison-adoption) · [추가 인수](../plans/dev3-test-plan.md#comparison-adoption) · [현재 상태](../implementation/dev3-implementation.md#comparison-adoption)

## 범위·원 근거

사용자는 임시 수학 분석 보고서를 XADB 설계·계획에 적용하고 완료 작업의 추가 보완과 부족한 설계 강화를 요청했다. 문서 적용 범위이며 제품 개발·시험 재개 요청으로 확대하지 않았다. 작업/계약/공유 문서 작성은 `/root`, 독립 D2 검토는 `/root/dev3_design_review`가 담당한다. [모델 역할](../plans/dev3-implementation-plan.md#model-roles)에 따라 별도 Astra 검토를 사용했다.

착수 HEAD는 `360961bc965d27480cbba321824b7db54bc613da`다. 분석 보고서의 제품 기준은 `4a456b9fa497bba0a66b3fc2aeca83b4c8552bd1`이며, plan4 재개 결과 이후 상태를 반영한다. 다른 작업의 DEV2 관측 도구·변경은 이번 범위가 아니다. 원 문서 사본·SHA-256 manifest는 외부 임시 보관 `xadb-dev3-comparison-adoption-20261025-wp9032_v`에 두었다. 이 임시 보관은 제품 실행 증거가 아니다.

입력은 `/home/et16/work`의 다음 임시 문서다. 임시 경로의 존속에 정본을 의존시키지 않고 필요한 결론·산식·추가 의무를 아래 소유 문서에 반영했다.

| 입력 | SHA-256 |
| --- | --- |
| `dev3-improvement-mathematical-analysis-20261025.tmp.md` | `d76f5e30c4e24466ecd64979fa4479fdbda637d1949a92b713e0f74e8d36df9e` |
| `dev3-oracle-comparison-20261025.tmp.md` | `51749dbe45f0f6660ea62b76fe406ede35f80ce78dc458065ece2d727b1b9c7c` |
| `dev3-postgresql-comparison-20261025.tmp.md` | `51eb2210db896dc7054f30babc0d69edfecf00a814890b319662a75c3eba126c` |
| `dev3-altidev4-design-comparison-20261025.tmp.md` | `21374ed419f86ade1065cb4f867f2151824833f49a9dc986fbae10b495a81173` |

Oracle 입력은19c 공개 문서 비교이고, PostgreSQL 입력은 로컬20devel revision `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`, altidev4 입력은 `0682bdd8610c3728810df616c95a858cda675706`의 disk 경로 조사다. altidev4 결과를 현재 상용 Altibase 전체의 성질로 일반화하지 않는다. 이번 문서 적용에서는 외부 DBMS source·benchmark를 새로 조사/실행하지 않았고 기존 보고서의 출처·범위를 보존했다. 외부 구현은 연구 입력이며 XADB와 동일 의미/구현·성능이라는 주장이 아니다.

## 결론의 배정과 대안

| 보고서 결론 | 채택·보완 | 보류·실행 경계 |
| --- | --- | --- |
| 제한 복구 다음 실제 행 수명 연결이 우선 | 최소 terminal→Catalog CREATE/table→한 row opcode→Undo/CLR/commit/reopen. 나머지 CRUD/index/DDL을 뒤에 확장 | N=0 원50개 소급 재실행/완료 취소 없음. 새 consumer는 추가 인수 |
| 구조체 크기만으로 F05 불충분 | [전 성분 벡터·전이·진행 표](../contracts/dev3-resource-bounds.md#resource-vector-proof), private64MiB/8MiB는 최악8개·protected 포함관계·확인된 반환·crash once-handoff | 각 opcode 실제 수치는 미동결. 종료 경로를 새 admission에 의존시키지 않음 |
| checkpoint의 같은-cut 대형 inventory 근거 미정 | [pinned base+complete prefix와 bounded snapshot](../contracts/dev3-task-13-checkpoint.md#immutable-capture-source), partition 재생·turn 간 snapshot 수명·WAL/output/cleanup 선예약 | mutable home 재읽기·gate 안 전체 복사 배제. exact codec/capacity·F05 미완료 |
| 새 restore timeline이 기존 identity와 충돌 | [offline typed rewrite](../contracts/dev3-task-15-backup-integrity.md#namespace-rewrite), 참조 다중도·payload digest 불변/체크섬 재계산·Releasing·credit·intent cleanup | 모든 과거 namespace를 허용하는 decoder 완화 대신 정확 변환. field census/control 형식·native 인수 미완료 |
| Commit 장벽과 table validation은 서로 다른 병목 | [서비스 비용](../contracts/dev3-task-11-commit.md#commit-service-bound)과 [동일 snapshot cohort](../contracts/dev3-task-10-concurrency.md#serializable-conflict-bound) 측정. batch validation은 앞 member의 영향을 포함 | group commit·미세 validation·기본 RC 전환 즉시 채택 없음 |
| 짧은 row/넓은 key·Undo·긴 reader의 비용 | [표현 비용](../contracts/dev3-resource-bounds.md#representation-cost)과 [retention 식](../contracts/dev3-task-14-reclamation.md#retention-pressure-model)을 측정 기준으로 편입 | 실측 공간 배율·타 DBMS 성능 차이로 오인 금지. 압축/형식 변경은 별도 |
| bounded RAM의 재순회 비용과 대형 profile 공백 | [candidate batch](../contracts/dev3-task-06-tree-rows.md#cursor-batch-bound), [audit·profile 계측](../contracts/dev3-task-16-qualification.md#comparison-measurement) | 큰 fixture/S5 실행·64GiB 지원 tuple은 미완료. 기본 quota·inventory4배를 임의 변경하지 않음 |

수학적 요점은 자원별 `A+R+b≤C`와 전이별 `δ+r_new≤r_old+d`, 같은 q0/table의 k-writer cohort 성공≤1, 단일 Commit 서비스 상한 `1/E[S]`, row leaf 용량 `floor(16188/(184+r))`, 보존 Undo `U0+∫(생성−반환)`, audit 방문 `E×ceil(T/B)`다. 각각 전제·숫자·한계는 해당 소유 절에 두었다. 이 식들은 안전 조건·조건부 근사·가설이며 TPS/p99/전체 공간의 새 실측 결과가 아니다.

## 완료 작업과 새 의무

[plan4 재개 원 기록](dev3-recovery-resumed-20261025.md)의 고유50개 source별 PASS(static2/memory45/native3), 원 FAIL/HOLD·중단과 한정 독립 수용은 그대로다. typed Catalog-before codec·pool/pin·Status 수명도 원 범위의 완료로 남긴다. 문서의 오래된 ‘Catalog occupied before 미연결’·‘DEV3 page 영속 전부 미구현’ 표현만 최신 한정 공급에 맞춰 정정했다.

N>0 row/Undo/CLR, BEGIN-WAL 삭제, 일반 capture/restore, 실제 병렬·공간·대형 인수는 [D3-COMP-01–10](../plans/dev3-test-plan.md#comparison-adoption)에 추가했다. 이는 case family 계획이며 실행 test 수나 PASS 분모가 아니다. 중간 변경 기능 필수와 S5 확대를 나눴다. 일반 DP07 최소10쌍을 유지하고 COMM5+5를 가져오지 않았다.

## 독립 검토·검증

1차 소유 설계 검토에서 두 보완을 반영했다. 기존 작업 종료의 CLR/rollback/terminal/recovery/cleanup은 재admission 성공에 의존하지 않도록 한정했다. checkpoint의 같은-cut snapshot/continuation은 기존 metadata8MiB 안에서 보존하고 partition scratch만 공유12MiB를 빌려 반환하도록 수명을 구분했다. 정확 fit은 F05 선행으로 남긴다.

`/root/dev3_design_review`가24개 대상의 최종 diff·계획/인수/JSON/색인 연결을 읽기 전용으로 독립 검토하여 **D2 PASS, 잔여 필수 P1/P2 없음**으로 판정했다. 1차 최종 후보 manifest SHA-256은 `28d00a8603116138be526d3422c86e11c9904e3048ca6724c58f2ae0de6300ca`다. 이어 기존 거래 slot 점유를 차감한 신규 admission 산식·2047/2048/2049 경계·N=0와 일반 미완료의 구분·checkpoint/restore 후속 동결표 등6문서 delta도 별도 PASS로 확인했다. 이 delta 반영 후 판정 문구를 기록하기 전24개 사본의 manifest SHA-256은 `06c12dea4b4ef3ddbf846b62e3bac45a3b6404303161c378e4e34365a9edfc1d`다. 두 사본은 위 임시 보관의 `review-v1`/`review-v2`에 있고, `/root`가 반영을 확인했다. 공통 색인의 동시 DEV2 추가 내용은 이 D2의 범위가 아니다.

정확 namespace/digest census·codec/API·수치 F05·지원 profile과 제품 인수는 미완료다. 새 kind/version, 종료 예약 누락, capture snapshot 수명/출력 peak 불일치, restore 부분 해제 변환 실패, batch prefix·pageLSN 반례 또는 실제 병목이 드러나면 해당 owner 설계를 다시 D2 검토한다. 제품·모델·성능 시험, compiler/Clippy, 문서 verifier, commit/push는 실행하지 않았다. 문서 자동 검사는 기존 push gate에 남긴다. 검토 채택과 제품 실행 증거는 별개다.

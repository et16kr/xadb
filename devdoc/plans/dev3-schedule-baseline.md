# DEV3 전체 예상 일정과 누적 진척률 기준선

2026-11-04 리뷰 적용 계획의 일정 영향은 [범위 배정·재견적](#review-adoption-20261104)을 따른다. V1 적립33/86일=38.37%는 유지하며, 아래 날짜 예측은 리뷰 적용 전의 과거 예측이다. 현재 완료 ETA는 상세 설계의 범위·견적 확정 전까지 미갱신이다.

2026-11-03 · **D3-SCHEDULE-001 / V1 · 사용자 요청에 따른 일정 가중치 기준선 · 초안 독립 D2 ACCEPT; 공급 단위 재배정 독립 D2 ACCEPT(r2, 필수 지적 0); 일정 항목 적립 독립 D2 ACCEPT(r2 필수 1건 반영) · PR #45 병합, 06 완료 반영(PR #47), 07 완료 반영(PR #49)**

기준 source: main `7692631`(PR #45 병합)과 PR #47(DELETE 3단계)의 병합이다. 07-1–07-4 적립은 PR #49(UPDATE 계약, 2단계 C 형식·전이, 3단계 제품 경로, 4단계 native fault cut)의 병합이다. 그 앞 V1은 main `ab2537eb`(PR #46 병합) 기준이었다. PR 초안은 `cd41b174`(PR #43) 기준 58일·39.66%였다. 병합 전에 PR #44·#46(DELETE 2-b-2·2-b-3) 상태를 반영하고, 사용자 결정으로 01–19 전체를 [공급 단위](#units)로 다시 배정했다([개정 기록](#decision)). 실제 제품 인수 상태는 [단계 완료 정본](stage-completion.json)과 각 source별 증거가 소유한다. 이 문서는 DEV3 일정 보고의 분모·가중치·적립 규칙을 소유한다.

**전체 기준 86일, 적립 33일, 현재 누적 38.37%, 잔여 가중치 61.63%(07은 PR #49 병합 시).** 적립은 [일정 항목](#items) 단위다. 남은 중앙 예상 기간 56일은 2026-11-04 06 완료 시점(적립 30일)의 예측이며, 그때 같은 투입을 유지할 때 중앙 완료 예상일은 2026-12-30이었다. 낙관/보수 범위는 45~67일(12-19~2027-01-10)이며 확정 납기가 아니다. 07 적립 뒤 미적립 가중치는 53일이고 예측은 다시 잡지 않았다. DELETE는 제품 경로(2-b-2: A 실행기·C preflight/Reserve·B tombstone 읽기·live 정리·replay·loser/winner 복구·base4, 메모리 시험)와 시험 전용 관찰자 대조(2-b-3)까지 병합돼 06의 일정 항목 06-1–06-5(1.5일)를 적립했다. 3단계 native fault cut과 남은 시험(06-6, 0.5일)은 PR #47의 병합으로 적립됐고, 그때 06(2일)이 완료됐다.

[구현 순서·선행](dev3-implementation-plan.md#remaining-work) · [16개 소유 명세](../contracts/dev3-task-designs.md#catalogue) · [최종 테스트](dev3-test-plan.md#final-suite)

<a id="basis"></a>
## 1. 예상 기간의 의미와 범위

- 기준일은 현재와 같은 AI 구현·별도 검토·통합 방식에서 예상한 경과 기간의 배정 단위다. 인일이나 실제 작업시간 실측값이 아니다. 구현, 관련 fixture/시험, 검토 수정과 통합을 포함한다.
- 완료분 01–05는 2026-11-03에 작업 난도를 보고 **사후 재견적**한 28일이다([공급 단위](#units)). 당시 사전 약속이나 예측 적중률로 해석하지 않는다. Git·기존 검증 기록으로 구분한 실제 경과 약 14일에는 중단·병행·병합 대기가 포함된다.
- 잔여분 06–19는 [공급 단위](#units)로 58일을 배정했다. 1단위는 06(첫 DELETE)의 2일이며, 완료분도 같은 단위로 다시 셌다. PR 초안은 앞서 제시한 28~42일의 중앙 35일을 행마다 2~3일로 거의 고르게 나눴으나, 제한 profile을 일반화하는 행(08·09·11·12 등)과 06의 작업량 차이를 반영하지 못해 병합 전에 대체했다. 단위 계수도 상태 전이·공유 경계·fault/자원 검증 부담으로 매긴 저신뢰 공학적 추정이며, 표의 소수점은 산술 정밀도일 뿐 추정 정확도가 아니다. 미적립 56일의 범위 45~67일을 분모에 넣으면 같은 진행 상태가 30.93~40.00%이므로, 34.88%는 이 폭 안의 보고용 기준값이다. 착수 전 분해에서 배정이 크게 모자라면 [6절](#progress-rule)의 V2 절차로 고친다.
- DEV3 저장 엔진과 기존 최종 S5 인수까지 포함한다. 실제 수행한 모듈 분리·필요한 gate 안정화도 포함한다. DEV4 SQL 연결, DEV5–9, 별도 COMM·DEV2 디버깅은 새 작업량으로 넣지 않는다. DEV3 인수에 필요한 하위 공급 의무는 19에서 확인한다.
- 01–05의 완료는 **표에 적은 당시 제한 공급**의 완료다. 일반 기능이나 DEV3 전체가 완료됐다는 뜻이 아니다. 기존 PARTIAL·NOT_RUN·수용된 한계는 원 증거 그대로 남긴다. 표 발급 상한은 10, F6 이후 필요한 성능 개선은 18, 일반 기능·추가 인수는 해당 잔여 행에 있다.
- 모든 새 opcode의 형식·자원 예약(F05)·독립 입력·해당 변경의 복구 검증은 그 행의 구현 전에 준비하고 구현과 함께 검증한다. 이를 12 또는 15–19까지 미루지 않는다. 12는 일반 규모/경계 통합, 15는 독립 audit와 전수 실행 준비, 17–19는 별도 최종 인수 비용이다.

<a id="schedule"></a>
## 2. 전체 일정·가중치 표

ID는 모두 `D3-SCH-` 접두어를 갖는다. 아래는 보고용 기본 배치이며 실제 착수 순서는 기존 계약의 공급 선행을 따른다. 선행 번호는 해당 행의 좁은 공급을 뜻한다. 한 행의 일부 준비를 병행할 수 있지만 같은 산출물의 예상 기간·적립을 중복해서 더하지 않는다.

`가중치 = 배정 예상일 ÷ 86 × 100`. 누적은 표 위에서부터 모두 끝났을 때의 **목표 누적률**이다. 실제 누적률은 [일정 항목](#items) 가운데 끝난 것의 배정일만 합산한다. 기준 일정 E1–E86은 사후 재견적을 포함한 상대 일정으로 실제 날짜와 구분한다.

| ID | 작업·공급 범위 | 예상일 | 기준 일정 | 가중치 | 목표 누적률 | 현재 상태 | 직접 선행 |
| --- | --- | ---: | --- | ---: | ---: | --- | --- |
| 01 | 기본 표현·유한 모델 | 3 | E1–E3 | 3.49% | 3.49% | 완료 | — |
| 02 | 초기 저장·flush·제한 복구 | 4 | E4–E7 | 4.65% | 8.14% | 완료 | 01 |
| 03 | 빈 거래·Undo 수명·Catalog CREATE | 6 | E8–E13 | 6.98% | 15.12% | 완료 | 02 |
| 04 | 모듈 분리·첫 Row 거래 통합 | 10 | E14–E23 | 11.63% | 26.74% | 완료 | 03 |
| 05 | 안정화·정리·거래 실행기 | 5 | E24–E28 | 5.81% | 32.56% | 완료 | 04 |
| 06 | 첫 DELETE 제품 경로 | 2 | E29–E30 | 2.33% | 34.88% | 완료(PR #47 병합 시) | 05 |
| 07 | UPDATE 제품 경로 | 3 | E31–E33 | 3.49% | 38.37% | 완료(PR #49 병합 시) | 06 |
| 08 | 트리·overflow·cursor 및 일반 행 변경 | 7 | E34–E40 | 8.14% | 46.51% | 진행(적립0) | 05; 일반 행 변경 최종 닫기는 06·07 |
| 09 | 인덱스·PK/UNIQUE | 6 | E41–E46 | 6.98% | 53.49% | 대기 | 07·08 |
| 10 | 일반 Catalog·DDL와 공급 잔여 | 4 | E47–E50 | 4.65% | 58.14% | 대기 | 09 |
| 11 | 동시성·다중 객체 거래 통합 | 7 | E51–E57 | 8.14% | 66.28% | 대기 | 07·09·10 |
| 12 | 일반 저장 수명·복구 장벽 통합 | 6 | E58–E63 | 6.98% | 73.26% | 대기 | 08·11 |
| 13 | fuzzy checkpoint·WAL 회수 | 4 | E64–E67 | 4.65% | 77.91% | 대기 | 12 |
| 14 | 일반 aging·공간 반환·재사용 | 3 | E68–E70 | 3.49% | 81.40% | 대기 | 12·13 |
| 15 | 독립 audit·최종 inventory 준비 | 3 | E71–E73 | 3.49% | 84.88% | 대기 | 05; 최종 닫기는 06–14 |
| 16 | exact-cut backup·restore | 3 | E74–E76 | 3.49% | 88.37% | 대기 | 13·14·15 |
| 17 | S5 정확성·복구 전수 | 4 | E77–E80 | 4.65% | 93.02% | 대기 | 06–16; 17-4는 18-1·18-2 |
| 18 | S5 대형 profile·PF09 | 3 | E81–E83 | 3.49% | 96.51% | 대기 | 06–16 |
| 19 | 최종 결함 수정·영향 재검증·인수 | 3 | E84–E86 | 3.49% | 100.00% | 대기 | 17·18 |
| **합계** | **DEV3 전체** | **86** | **E1–E86** | **100%** | **100%** | **33일 적립 / 53일 미적립** | — |

표시된 행별 %를 반올림한 뒤 더하지 않는다. 원 배정일을 합한 다음 86으로 나눠 최종값만 소수 둘째 자리로 반올림한다. 기존 기능 분류 3.01–3.16 또는 DEV3 내부 S0–S5를 개수로 나누는 방식이 아니다.

<a id="units"></a>
### 2.1 배정 근거 — 공급 단위

**1단위 = 2일 = 06(첫 DELETE).** 06은 새 opcode 셋의 형식·전이 계약(독립 D2 3회), F05/census, A/B/C 연결, 읽기·rollback·회수·복구, native fault cut을 한 묶음으로 닫는다. 다른 행은 §3 완료 조건에 든 독립 공급을 이 묶음과 비교해 0.5단위로 센다. 새 효과 없이 시험·정리·실행만 하는 공급도 같은 부담으로 비교한다. 코드 줄 수·PR 수·하위 ID 개수로 세지 않는다.

| ID | 단위 | 예상일 | PR 초안 | 계수 근거 |
| --- | ---: | ---: | ---: | --- |
| 01 | 1.5 | 3 | 3 | 표현 codec·독립 vector 1, M01–M06 유한 모델 0.5 |
| 02 | 2 | 4 | 4 | 설치·선택 읽기·물리 WAL·reserve/cancel·한 page flush 1, Status INIT/home flush와 닫힌 genesis 복구 1 |
| 03 | 3 | 6 | 5 | 빈 거래 terminal·재적재·복구 1, Free Undo 수명·N=0 plan4 복구 1, 첫 Catalog CREATE·commit·복구 1 |
| 04 | 5 | 10 | 8 | Row INSERT·Commit·point/cursor 읽기 1, statement/full rollback CLR 1, 복구 READY(REBASE·새 epoch·복구 CLR) 1, A/B/C 경계·단독 export·소비자 inventory와 M4/M5 1.5, module gate S12 0.5 |
| 05 | 2.5 | 5 | 3 | F0 기존 실패 수정·이관과 F1–F7(F6는 조사·M0) 1, live Row 정리·호출자 0.5, 자동커밋·명시 거래 실행기와 F4 상한 재계약·C census 절감 1 |
| 06 | 1 | 2 | 2 | **기준 단위.** 새 opcode 셋(8·9·23)의 계약·전이·A/B/C·복구·native fault cut |
| 07 | 1.5 | 3 | 3 | before/after 값과 크기 변화의 leaf 공간, Undo/CLR·읽기·reopen. DELETE의 before 경로를 재사용 |
| 08 | 3.5 | 7 | 3 | split/root SMO와 cursor/locator 안정성 1.5, overflow chain 1, 일반 행 변경(own/repeated write·occupied-before·Undo chain·큰 행 복원) 1 |
| 09 | 3 | 6 | 3 | index 항목 유지와 row↔index 원자성·Undo/CLR 1, point/range·presence scan 0.5, statement-final UNIQUE/PK·부분 NULL과 UNIQUE key lock 사용(대기·deadlock은 11) 1, offline build·취소 0.5 |
| 10 | 2 | 4 | 2 | table 일반 create·DROP·drain·stale handle·이름/세대와 원자 복구 1, index 정의 create/drop 0.5, 표 발급 caller 상한(C 잔여) 0.5 |
| 11 | 3.5 | 7 | 3 | Status slot 하나를 넘는 동시 거래와 두 격리 수준 snapshot 1, row/key X-lock 충돌·대기·deadlock victim·취소 1, 일반 행 변경의 관측 버전·이력 0.5, 다중 객체 ACK/UNKNOWN 공개 1 |
| 12 | 3 | 6 | 3 | extent 성장·FILEMAP 인계 1, 일반 INIT/load/eviction과 압력하 진행 1, DWB 1/8/64·다중 파일과 8복구 장벽·예약 복원·재장애 일반화 1 |
| 13 | 2 | 4 | 3 | immutable capture와 current/previous fallback 1, WAL floor·회수와 BEGIN-WAL 제거 복구 인계 1 |
| 14 | 1.5 | 3 | 2 | horizon·영속 edge 보호와 일반 Undo/Status/삭제 행 회수 0.5(첫 profile live 정리는 05, 첫 DELETE의 DeletePrune은 06), page/extent 반환·generation 재사용 1 |
| 15 | 1.5 | 3 | 2 | 전체 무결성 oracle·known-bad, DEV3 전체 FT01–FT10의 고정 inventory·fixture·fuzz/stress harness·실행 환경 1.5 |
| 16 | 1.5 | 3 | 2 | offline exact-C backup 0.5, typed namespace rewrite·retained credit·restore→새 ACK→reopen·취소/장애 1 |
| 17 | 2 | 4 | 3 | FT01–FT05·FT07·FT09–FT10 확정 inventory의 전수 캠페인(256 seed·fuzz 30분·stress 30분·fault 셀 3회)과 결과 검토 2 |
| 18 | 1.5 | 3 | 2 | FT06·FT08(64GiB·buffer 4배·자원 경계·PF09) 전수 실행과 결과 검토 1, 실행 선행 WAL 재검증 개선(F6-b) 0.5. F6 M0에서 거래당 WAL 읽기가 30·E₀+8.7 MB로 WAL 길이에 비례해 넣었고, M1에서 병목이 아니면 개선 없이 18을 닫으며 가중치는 그대로 둠 |
| 19 | 1.5 | 3 | 2 | 처음 하는 전수 캠페인의 결함·성능 미달 수정과 영향 재검증·최종 인수 1.5. 17/18의 결함 목록이 이 배정을 크게 넘으면 §6의 V2로 다시 견적 |
| **합계** | **43** | **86** | **58** | **완료 행 16.5단위·미완료 행 26.5단위(적립 16.5단위 = 33일)** |

검산: 완료분의 `실제 경과 / 배정`은 PR 초안 0.33–0.75에서 0.33–0.60으로 좁아졌다(01 1/3, 02 2/4, 03 3/6, 04 6/10, 05 2/5). 01이 가장 낮은 것은 첫 commit `d58c26b9`가 그 전에 준비한 표현·모델을 한 번에 넣었고, 그 앞의 계약·설계 시간은 이 기준선 밖이기 때문으로 보인다(측정 아님). 06은 DELETE PR #41–44·#46이 약 11시간(11-03 14:16~11-04 00:46 KST) 사이에 병합돼 같은 척도와 어긋나지 않는다. 실제 경과에는 중단·병행·병합 대기가 섞여 있으므로 배정의 근거가 아니라 검산에만 쓴다. 15·17·19는 DEV3 전체를 처음 전수로 다루므로 같은 척도에서 앞의 inventory 작업(04의 소비자 inventory)보다 크게 셌다. 19의 크기는 미리 알 수 없어, 17/18의 결함 목록이 나오면 다시 견적한다.

<a id="items"></a>
### 2.2 일정 항목

적립은 일정 항목 단위다. 01–19를 모두 아래 항목으로 나누고, 항목 배정일의 합은 그 행의 배정일과 같다. 01–05와 07–19는 §2.1 계수 근거의 구성 요소마다 항목 하나를 두고 그 단위를 그대로 쓴다(0.5단위 = 1일). 한 계수로 묶여 있던 것(04의 A/B/C·소비자 inventory 1.5, 05의 F0–F7 1과 실행기 1, 07 전체, 08의 SMO·cursor 1.5, 15의 1.5, 17의 2, 18의 FT06·FT08 1, 19의 1.5)은 산출물 경계로 나눴다. 04·05는 실제 단계(경계·S7–S9와 S10–S11, F0와 F1–F7, PR #39와 #40)를 경계로 나눴고(04는 C0–C3·L1–L6 이동·S7–S9의 경계·export 공급이 S10–S11의 소비자 inventory보다 부담이 커 2일·1일로 둠. 이미 전액 적립된 행이라 % 영향 없음), 17은 fault 셀 3회·stress가 있는 FT04·FT05를 무겁게, 결과 대응 검사인 FT09를 가볍게 뒀다. 07은 06과 같은 계약→C 형식·전이→A/B 경로·복구→native 순서다. 06은 계약 §13의 단계(1 계약, 2 제품 경로, 3 native)를 따르고, 2단계는 실제 PR 분할(2-a·2-b-1·2-b-2·2-b-3)로 나눴다. PR 경계는 항목 경계로만 쓰고, 배정일은 PR 수가 아니라 부담으로 매겼다. A/B/C·복구를 한 PR에 모은 2-b-2와 실제 I/O fault 환경이 필요한 3단계를 0.5일, 나머지를 0.25일로 두었다. 06-1–06-5는 끝난 뒤에 나눈 사후 분할이며, 사용자 요청("작업을 세분화")으로 병합 전 V1에 넣어 1.5일을 소급 적립했다. 01–05의 항목도 사용자 요청("이미 끝난 작업도 세분화")으로 끝난 뒤 나눈 것이며, 행 전체가 이미 적립돼 있어 적립 28일은 바뀌지 않는다. 01–05 항목은 §3의 당시 제한 공급(첫 Row profile·Status slot 하나 등)이며 일반 기능 완료가 아니다.

| 항목 | 일정 항목 | 배정일 | 상태 | 증거 |
| --- | --- | ---: | --- | --- |
| 01-1 | 당시 공급한 page·key·tree·WAL·Undo·Status·Catalog 표현 codec과 독립 vector | 2 | 완료 | `d58c26b9`(page)·`d96bd1a9`(catalog·undo·control), [표현](../implementation/dev3-implementation.md#catalog-undo-control) |
| 01-2 | M01–M06 유한 모델 | 1 | 완료 | `d58c26b9`, [M01–M06](../implementation/dev3-implementation.md#models-all) |
| 02-1 | 설치·선택 읽기·물리 WAL·reserve/cancel·한 page flush | 2 | 완료 | [설치](../implementation/dev3-implementation.md#doublewrite-install-results)·[선택 읽기](../implementation/dev3-implementation.md#selector-resume-results)·[genesis WAL](../implementation/dev3-implementation.md#genesis-wal-results)·[예약](../implementation/dev3-implementation.md#allocation-reservation-accepted)·[flush](../implementation/dev3-implementation.md#allocator-flush) |
| 02-2 | Status INIT/home flush와 닫힌 genesis 복구 | 2 | 완료 | [Status INIT](../implementation/dev3-implementation.md#status-init)·[home flush](../implementation/dev3-implementation.md#status-home-flush)·[닫힌 genesis 복구](../implementation/dev3-implementation.md#recovery-effects) |
| 03-1 | 빈 거래 terminal·재적재·복구 | 2 | 완료 | [빈 거래](../implementation/dev3-implementation.md#empty-transaction)·[복구](../implementation/dev3-implementation.md#empty-transaction-recovery) |
| 03-2 | Free Undo 수명·N=0 plan4 복구 | 2 | 완료 | [Free Undo](../implementation/dev3-implementation.md#undo-free-page)·[plan4 Free Undo](../implementation/dev3-implementation.md#plan4-free-undo)·[N=0 복구](../evidence/dev3-recovery-resumed-20261025.md) |
| 03-3 | 첫 Catalog CREATE·commit·복구 | 2 | 완료 | [Catalog CREATE 잔여 선택 사례](../evidence/dev3-catalog-remaining-20261026.md)·[수정](../evidence/dev3-catalog-review-fix-20261026.md) |
| 04-1 | 첫 Row profile의 제한 INSERT·Commit·point/cursor 읽기(S1–S3·S5) | 2 | 완료 | PR #2 `6ec2d343`·#5 `139a05f4`·#7 `8d5e48b5`, [S1](../evidence/module-independence-survey-20261026.md#s1d-row-begin)·[S2](../evidence/module-independence-survey-20261026.md#s2f-closeout)·[S3](../evidence/module-independence-survey-20261026.md#s3-row-commit)·[S5](../evidence/module-independence-survey-20261026.md#s5-row-read-completion) |
| 04-2 | 첫 Row profile의 statement/full rollback CLR(S4) | 2 | 완료 | PR #6 `2ed254ac`, [S4](../evidence/module-independence-survey-20261026.md#s4-row-rollback) |
| 04-3 | 첫 Row profile의 복구 READY(REBASE·새 epoch·복구 CLR, S6) | 2 | 완료 | PR #8 `76b36278`·#10 `c427def8`, [S6](../evidence/module-independence-survey-20261026.md#s6-row-recovery) |
| 04-4 | A/B/C 경계 조사·C0–C3 경계·L1–L6 이동, 결합·단독 export(S7–S9) | 2 | 완료 | [조사](../evidence/module-independence-survey-20261026.md#survey)·[C0](../evidence/module-independence-survey-20261026.md#c0-borrowed-codec)·[L1–L6](../evidence/module-independence-survey-20261026.md#l1-allocation-move), PR #11 `5c989374`·#12 `3c046e98`·#14 `7efb32c6`, [S7](../evidence/module-independence-survey-20261026.md#s7-row-join)·[S8](../evidence/module-independence-survey-20261026.md#s8-single-source)·[S9](../evidence/module-independence-survey-20261026.md#s9-module-completion) |
| 04-5 | 소비자 고정 inventory와 M5 한정 인수(S10–S11) | 1 | 완료 | PR #15 `ee0a48c4`·#16 `f95eba15`·#17 `907c7cf5`, [S10](../evidence/module-independence-survey-20261026.md#s10-frozen-inventory)·[M5](../evidence/module-independence-survey-20261026.md#s11-m5); MCB03-V01 한계 수용 보존 |
| 04-6 | module gate 계획 검토·S12와 원격 시행 | 1 | 완료 | PR #13 `5107056d`·#18 `76ec2cf5`·#22 `9dc9bf7d`, [S12](../evidence/module-independence-survey-20261026.md#s12-module-gate)·[원격 시행](../evidence/module-independence-survey-20261026.md#s12-remote-rollout) |
| 05-1 | 후속 계획과 F0 main 기존 실패 수정·이관 | 1 | 완료 | PR #23 `8215c729`·#24 `01fb227d`·#26 `d6ebed80`, [F0](../evidence/main-preexisting-failures-20261030.md#f0-completion) |
| 05-2 | F1–F7(F6는 조사·M0) | 1 | 완료 | PR #25 `5af10231`·#27–#35(`0fde463c`…`f1600a24`)·#37 `372b22e0`, [F1](../evidence/module-independence-survey-20261026.md#f1-python-gate)·[F2](../evidence/module-independence-survey-20261026.md#f2-contract-selection)·[F3](../evidence/module-independence-survey-20261026.md#f3-catalog-call)·[F4](../evidence/module-independence-survey-20261026.md#f4-caller-budgets)·[F5](../evidence/module-independence-survey-20261026.md#f5-scope3-fixture)·[F6 M0](../evidence/f6-wal-revalidation-survey-20261102.md#f6a-m0)·[F7](../evidence/module-independence-survey-20261026.md#f7-current-comments) |
| 05-3 | live Row 정리와 호출자 | 1 | 완료 | PR #36 `8fa96c59`·#38 `5f478d00`, [live 정리](../evidence/live-row-retirement-20261102.md)·[호출자](../evidence/live-row-cleanup-caller-20261102.md) |
| 05-4 | 자동커밋 INSERT 실행기와 F4 rollback 상한 재계약 | 1 | 완료 | PR #39 `65d473e7`, [자동커밋](../evidence/autocommit-insert-20261102.md) |
| 05-5 | 명시 거래 실행기와 BEGIN·STATEMENT_START 상한·C census 절감 | 1 | 완료 | PR #40 `c7a6425c`, [명시 거래](../evidence/explicit-transaction-20261103.md) |
| 06-1 | 1단계 DELETE 형식·전이 계약, 독립 D2 3회 | 0.25 | 완료 | PR #41 `5a35869f` |
| 06-2 | 2-a 형식층 | 0.25 | 완료 | PR #42 `05d6cae9` |
| 06-3 | 2-b-1 C whole-group 전이층 | 0.25 | 완료 | PR #43 `cd41b174` |
| 06-4 | 2-b-2 제품 경로(A 실행기·C preflight/Reserve·B 읽기·live 정리·replay·loser/winner 복구·base4, 메모리 시험) | 0.5 | 완료 | PR #44 `f5b731c9` |
| 06-5 | 2-b-3 시험 전용 관찰자 DELETE 대조와 넘겨받은 시험(다른 RowId, A frame 예약 경계, RDL-B06, RDL-R07) | 0.25 | 완료 | PR #46 `ab2537eb` |
| 06-6 | 3단계 native fault cut(RDL-N)과 남은 시험 | 0.5 | 완료(PR #47 병합 시) | PR #47, [3단계 구현 기록](../contracts/dev3-row-delete.md#implementation-3)(RDL-N01..N06 native 18 process, RDL-J11) |
| 07-1 | UPDATE 형식·전이 계약과 독립 검토 | 0.5 | 완료(PR #49 병합 시) | PR #49, [계약](../contracts/dev3-row-update.md)(독립 D2 2회차 ACCEPT) |
| 07-2 | C 형식·전이(before/after 값, 크기 변화의 leaf 공간) | 1 | 완료(PR #49 병합 시) | PR #49, [2단계 구현 기록](../contracts/dev3-row-update.md#implementation-2) |
| 07-3 | A/B 제품 경로와 Undo/CLR·읽기·reopen·복구(메모리 시험) | 1 | 완료(PR #49 병합 시) | PR #49, [3단계 구현 기록](../contracts/dev3-row-update.md#implementation-3) |
| 07-4 | native fault cut | 0.5 | 완료(PR #49 병합 시) | PR #49, [4단계 구현 기록](../contracts/dev3-row-update.md#implementation-4) |
| 08-1 | split/root SMO(system group)의 계약·구현·복구 | 2 | 진행(적립0) | [제한 split 효과](../evidence/dev3-tree-structure-20261104.md#structural-effect), [current-leaf 소비자 일반화](../contracts/dev3-tree-smo.md#current-leaf-consumers); unlink/root shrink/retirement·A 자동 split 호출자 잔여 |
| 08-2 | SMO를 건너는 stable cursor/locator | 1 | 대기 | — |
| 08-3 | overflow chain 쓰기·읽기·반환 | 2 | 대기 | — |
| 08-4 | 일반 행 변경(own/repeated write·occupied-before·Undo chain·큰 행 복원) | 2 | 대기 | — |
| 09-1 | index 항목 유지와 row↔index 원자성·Undo/CLR | 2 | 대기 | — |
| 09-2 | point/range·presence scan | 1 | 대기 | — |
| 09-3 | statement-final UNIQUE/PK·부분 NULL과 UNIQUE key lock 사용(대기·deadlock은 11-2) | 2 | 대기 | — |
| 09-4 | offline build·취소 | 1 | 대기 | — |
| 10-1 | table 일반 create·DROP·drain·stale handle·이름/세대와 원자 복구 | 2 | 대기 | — |
| 10-2 | index 정의 create/drop | 1 | 대기 | — |
| 10-3 | 표 발급 caller 상한(C 잔여) | 1 | 대기 | — |
| 11-1 | Status slot 하나를 넘는 동시 거래와 두 격리 수준 snapshot | 2 | 대기 | — |
| 11-2 | row/key X-lock 충돌·대기·deadlock victim·취소 | 2 | 대기 | — |
| 11-3 | 일반 행 변경의 관측 버전·이력 | 1 | 대기 | — |
| 11-4 | 다중 객체 ACK/UNKNOWN 공개 | 2 | 대기 | — |
| 12-1 | extent 성장·FILEMAP 인계 | 2 | 대기 | — |
| 12-2 | 일반 INIT/load/eviction과 압력하 진행 | 2 | 대기 | — |
| 12-3 | DWB 1/8/64·다중 파일과 8복구 장벽·예약 복원·재장애 일반화 | 2 | 대기 | — |
| 13-1 | immutable capture와 current/previous fallback | 2 | 대기 | — |
| 13-2 | WAL floor·회수와 BEGIN-WAL 제거 복구 인계 | 2 | 대기 | — |
| 14-1 | horizon·영속 edge 보호와 일반 Undo/Status/삭제 행 회수(첫 profile live 정리는 05-3, 첫 DELETE의 DeletePrune은 06) | 1 | 대기 | — |
| 14-2 | page/extent 반환·generation 재사용 | 2 | 대기 | — |
| 15-1 | 전체 무결성 oracle·known-bad | 1 | 대기 | — |
| 15-2 | DEV3 전체 FT01–FT10 고정 inventory·fixture | 1 | 대기 | — |
| 15-3 | fuzz/stress harness·실행 환경 | 1 | 대기 | — |
| 16-1 | offline exact-C backup | 1 | 대기 | — |
| 16-2 | typed namespace rewrite·retained credit·restore→새 ACK→reopen·취소/장애 | 2 | 대기 | — |
| 17-1 | FT01–FT03(표현·모델·기능/원자성) 전수 실행과 결과 검토 | 1 | 대기 | — |
| 17-2 | FT04 영속·복구(선언 fault 셀 3회)·FT05 동시성/수명(stress 30분) 전수 실행과 결과 검토 | 1.5 | 대기 | — |
| 17-3 | FT07 checkpoint·회수·backup·audit·FT10 하위 공급 수신 전수 실행과 결과 검토 | 1 | 대기 | — |
| 17-4 | FT09 재현·증거(전체 inventory 결과 누락 0, 첫 재현 bundle) | 0.5 | 대기 | — |
| 18-1 | FT06 자원·크기(64GiB·buffer 4배·자원 경계) 전수 실행과 결과 검토 | 1 | 대기 | — |
| 18-2 | FT08 PF09 전수 실행과 결과 검토 | 1 | 대기 | — |
| 18-3 | 실행 선행 WAL 재검증 개선(F6-b; M1에서 병목이 아니면 개선 없이 닫음) | 1 | 대기 | — |
| 19-1 | 17의 결함 수정과 영향 재검증 | 1 | 대기 | — |
| 19-2 | 18의 성능 미달 수정과 영향 재검증 | 1 | 대기 | — |
| 19-3 | 최종 인수·수정 뒤 재현 bundle 갱신·최종 독립 검토 | 1 | 대기 | — |

**적립 시점이 따로 있는 항목.** 아래 항목은 산출물 일부의 병합만으로 적립하지 않는다.

- 15-1–15-3: 06–14를 덮는 최종본이 병합될 때 적립한다. 기능 개발과 병행한 준비분의 병합만으로는 적립하지 않는다.
- 17-4: 18-1·18-2의 결과까지 모아 전체 inventory 결과 누락 0을 확인해야 적립한다.
- 18-3: M1 측정·판정과, 병목이면 그 개선이 병합될 때 적립한다. 병목이 아니면 판정 기록의 병합 때 적립한다.
- 19-1(19-2): 17(18)이 완료되고, 거기서 19로 넘긴 모든 FAIL(기준 미달)이 수정·재검증됐을 때 적립한다. 넘긴 것이 0건이면 17(18) 완료 때 적립한다.
- 19-3: 19-1·19-2 뒤 최종 인수·수정 뒤 재현 bundle 갱신·최종 독립 검토가 병합될 때 적립한다. 첫 FT09 실행은 17-4다.

<a id="completion"></a>
## 3. ID별 완료 조건과 기존 범위 대응

행은 아래 산출물·조건을 모두 채워야 완료다. 적립은 [일정 항목](#items) 단위이며, 항목마다 그 산출물의 구현, 계약상 필요한 변경 코드 시험, 독립 검토의 필수 지적 해소, main 병합 및 증거 링크가 필요하다. 행에서 마지막으로 남은 항목(번호 순서와 무관)은 이 표의 행 조건을 모두 채워야 적립한다. 이 조건은 새 제품 시험 실행 권한이 아니며 [변경 코드 시험 제한과 S5 한정 예외](../../AGENTS.md)를 그대로 따른다.

| ID | 기존 분류·범위 | 이번 일정 행을 닫는 산출물·조건 |
| --- | --- | --- |
| 01 | 3.01 | 당시 공급한 page/key/tree/WAL/Undo/Status/Catalog 표현과 M01–M06의 제한 모델 |
| 02 | 3.02–05·3.12 | 초기 파일·WAL·buffer·Status flush와 닫힌 genesis 복구 |
| 03 | 3.08–12·3.14 일부 | 빈 거래, Free Undo, 첫 Catalog CREATE·조회·rollback·복구의 제한 공급 |
| 04 | 3.06·3.09–12 및 모듈 분리 | A/B/C 경계·자원 회계, 제한 INSERT/읽기/Commit/rollback/READY, M5 한정 인수, module gate(S12) 시행 |
| 05 | 3.09·3.11·3.14 일부 및 gate | F0–F5/F7의 기록 범위·F6 M0, live Row 정리와 호출자, 자동커밋·명시 거래 실행기 |
| 06 | 3.06·3.09·3.12 | C/A/B·loser·replay/base4 연결, rollback·회수·native fault cut; 형식/전이만으로 완료 불가 |
| 07 | 3.06·3.09–12 | before/after·Undo/CLR, statement/full rollback·읽기·reopen·native fault cut([개발 정책](../policies/development-policy.md)의 내구성 변경 의무); 현재 지원 profile의 실제 변경 연결 |
| 08 | 3.06 및 3.03–05 | split/root/구조 변경, overflow, stable cursor/locator와 일반 DELETE/UPDATE: own/repeated write·occupied-before·Undo chain·큰 행/overflow 변경·복원. task06 R4–R7 및 task09 logical undo의 자원·rollback·복구 인수 |
| 09 | 3.07 | point/range·presence·offline build/취소, statement-final UNIQUE·부분 NULL, 일반 INSERT/UPDATE/DELETE의 row↔index 원자성·Undo/CLR 통합 |
| 10 | 3.08 | table/index create/drop·이름/세대·stale handle·drain·원자 복구, 표 발급 caller 상한 잔여 |
| 11 | 3.10·3.11 | Serializable/ReadCommitted, row/key 충돌·deadlock/victim, 일반 행 변경의 관측 버전·경합·이력, 여러 객체 ACK/UNKNOWN·publication 통합 |
| 12 | 3.02–05·3.09·3.12 | extent 성장·일반 INIT/load/eviction, DWB 1/8/64·다중 파일, 압력하 진행, 8복구 장벽·예약 복원·재장애 |
| 13 | 3.13 | immutable capture·ATT/DPT·current/previous fallback·WAL floor, BEGIN-WAL 제거 경계와 복구 인계 |
| 14 | 3.03.4·3.14 | 긴 reader/Active 거래·영속 edge 보호, Undo/Status/삭제 행 회수, page/extent 실제 반환·generation 재사용 |
| 15 | 3.15.1·3.16 | 전체 무결성 oracle·known-bad, FT01–FT10의 고정 사례·fixture·실행 환경·누락/중복 검사; 기능 개발과 병행 준비 |
| 16 | 3.15.2 | offline exact-C, typed namespace rewrite·retained credit, source-WAL-free restore→새 ACK→reopen·취소/장애 |
| 17 | 3.16 / S5 | 확정 DEV3 inventory의 correctness·fault·동시성·재장애 전수 실행·결과 검토 완료; FAIL은 원인/재현 입력과 함께 19에 배정. NOT_RUN/중단 셀이 남으면 미완료. 제품 합격과 구분; 성능 의무는 18 |
| 18 | 3.16 / S5 | 64GiB 지원 qualification·buffer 4배 working set·자원 경계·PF09 전수 실행·결과 검토 완료; NOT_RUN/중단은 미완료. WAL 재검증 등 실행 선행 개선 포함, 기준 미달은 원인/재현 입력과 함께 19에 배정 |
| 19 | 3.16 / S5·제품 S1 | 17/18과 병행해 발견 결함·성능 미달 수정, 관련 diff 재검증·모든 필수 인수 충족, 원 실패 보존·재현 bundle·독립 검토. 완료 판정만 17/18 종료 뒤 |

06–19는 각 [소유 명세의 인수 의무](../contracts/dev3-task-designs.md#catalogue)를 이 표의 짧은 문구로 축소하지 않는다. 06은 첫 DELETE, 07은 제한 UPDATE이며 일반 행 변경의 남은 의무는 08, 인덱스 동반 변경은 09, 동시성/다중 객체 이력은 11에 배정한다. 07과 08의 일반화 연결도 양쪽 관련 변경의 인수에 포함한다. **17/18의 완료는 전수 실행·정직한 결과 판정의 완료이며 제품 PASS가 아니다.** FAIL이 있으면 재현 입력·영향 범위·19의 수정 항목을 기록하고 결과 검토를 받아야 해당 실행 항목을 적립할 수 있다. 19의 수정은 17/18 중에도 진행해 후속 실행을 막는 결함을 해소한다. NOT_RUN·skip·중단을 실행 완료로 세지 않는다. 17/18에서 발견된 결함을 19에서 고쳤다고 17/18의 원 FAIL을 지우지 않는다. 최종 필수 의무가 미해결이면 19는 완료될 수 없고 100%가 될 수 없다.

<a id="credit-ledger"></a>
## 4. 현재 적립 대장과 과거 비교

실제 구간은 KST의 주요 작업 경계를 날짜 단위로 반올림했다. 날짜 차이를 쓰며 양끝 날짜를 각각 하루로 중복 계산하지 않는다. 동시 진행한 다른 단계의 시간을 분리 측정한 자료가 아니므로 아래 실제값을 순수 DEV3 노동시간으로 사용하지 않는다.

| ID | 사후 예상 범위 / 중앙값 | 실제 구간 / 약 경과 | 적립일 | 증거·한정 범위 |
| --- | --- | --- | ---: | --- |
| 01 | 2–4일 / 3일 | 10-20→10-21 / 1일 | 3 | [첫 표현과 모델](../implementation/dev3-implementation.md#models-all), `d58c26b9`부터 `d96bd1a9`까지 당시 codec·모델 공급 |
| 02 | 3–5일 / 4일 | 10-21→10-23 / 2일 | 4 | [Status home flush](../implementation/dev3-implementation.md#status-home-flush), [닫힌 genesis 복구](../implementation/dev3-implementation.md#recovery-effects); 일반 pool·복구 인수 제외 |
| 03 | 5–7일 / 6일 | 10-23→10-26 / 3일 | 6 | [빈 거래](../implementation/dev3-implementation.md#empty-transaction), [N=0 복구](../evidence/dev3-recovery-resumed-20261025.md), [Catalog CREATE 잔여 선택 사례](../evidence/dev3-catalog-remaining-20261026.md)와 [수정](../evidence/dev3-catalog-review-fix-20261026.md) |
| 04 | 8–12일 / 10일 | 10-26→11-01 / 6일 | 10 | [모듈 분리·Row 통합 기록](../evidence/module-independence-survey-20261026.md), [M5 한정 완료](../evidence/module-independence-survey-20261026.md#s11-m5), [module gate](../evidence/module-independence-survey-20261026.md#s12-module-gate)와 [원격 시행](../evidence/module-independence-survey-20261026.md#s12-remote-rollout); MCB03-V01 한계 수용을 보존 |
| 05 | 4–6일 / 5일 | 11-01→11-03 / 2일 | 5 | [후속 계획 F0–F7](module-independence-followup-plan.md): [F0](../evidence/main-preexisting-failures-20261030.md#f0-completion), [F1](../evidence/module-independence-survey-20261026.md#f1-python-gate), [F2](../evidence/module-independence-survey-20261026.md#f2-contract-selection), [F3](../evidence/module-independence-survey-20261026.md#f3-catalog-call), [F4](../evidence/module-independence-survey-20261026.md#f4-caller-budgets), [F5](../evidence/module-independence-survey-20261026.md#f5-scope3-fixture), [F6 M0](../evidence/f6-wal-revalidation-survey-20261102.md#f6a-m0), [F7](../evidence/module-independence-survey-20261026.md#f7-current-comments); [live 정리](../evidence/live-row-retirement-20261102.md), [호출자](../evidence/live-row-cleanup-caller-20261102.md), [자동커밋](../evidence/autocommit-insert-20261102.md), [명시 거래](../evidence/explicit-transaction-20261103.md), PR #40 `c7a6425c`; 표 발급 상한은 미공급 |
| 06 | 배정 2일 | 11-03→11-04 / 1일 | 2 | [계약](../contracts/dev3-row-delete.md)·[단계](../contracts/dev3-row-delete.md#stages)·[2-b-1](../contracts/dev3-row-delete.md#implementation-2b1)·[2-b-2](../contracts/dev3-row-delete.md#implementation-2b2)·[2-b-3](../contracts/dev3-row-delete.md#implementation-2b3)·[3단계](../contracts/dev3-row-delete.md#implementation-3) 구현 기록, PR #41–44·#46·#47; committed·Prune된 행 하나를 RowId로 지우는 첫 DELETE 한정 |
| 07 | 배정 3일 | 11-04→11-04 / 0일(같은 날) | 3 | [계약](../contracts/dev3-row-update.md)·[2단계](../contracts/dev3-row-update.md#implementation-2)·[3단계](../contracts/dev3-row-update.md#implementation-3)·[4단계](../contracts/dev3-row-update.md#implementation-4) 구현 기록, PR #49; committed·Prune된 행 하나의 값을 바꾸는 첫 UPDATE 한정(형식·전이, A 실행기·생산자, C preflight·Reserve, B 옛 값 읽기, live·복구 UpdatePrune, replay, loser UpdateClr, base4, 메모리 시험, native fault cut) |
| **합계** | **01–05 22–34일 / 28일, 06 2일, 07 3일** | **약 15일** | **33** | **33 / 86 = 38.37%** |

06은 PR #41–44·#46이 병합됐고 PR #47의 병합으로 끝난다. [2-b-1 구현 기록](../contracts/dev3-row-delete.md#implementation-2b1)에 형식·전이층, [2-b-2 구현 기록](../contracts/dev3-row-delete.md#implementation-2b2)에 제품 진입점(A 자동커밋·명시 거래 DELETE, C preflight·Reserve)과 그 상태를 읽고 복구하는 경로(B tombstone 읽기, live DeletePrune, replay, loser DeleteClr, winner DeletePrune, base4)의 메모리 시험 공급이 있다. [2-b-3 구현 기록](../contracts/dev3-row-delete.md#implementation-2b3)에는 시험 전용 status·undo·leaf 관찰자의 DELETE 대조, N1 cross-check의 효과1·DELETE skip 제거, 넘겨받은 시험(다른 RowId, A frame의 leaf 예약 경계, RDL-B06, RDL-R07)이 있다. 06-1–06-5는 끝나 1.5일을 적립했다. [3단계 구현 기록](../contracts/dev3-row-delete.md#implementation-3)에는 06-6의 native fault cut(DeleteForward·DeleteClr·DeletePrune 앞뒤의 crash, 사례마다 produce·recover·reenter 세 process, RDL-N01..N06)과 남은 시험 RDL-J11(다른 거래가 쥔 행의 Busy·SerializationConflict)이 있다. 06-6은 06의 마지막 항목이라 §3의 행 조건(C/A/B·loser·replay/base4 연결, rollback·회수·native fault cut)을 모두 채워야 적립한다. 연결·rollback·회수는 2-b-2·2-b-3이, native fault cut은 3단계가 채웠으므로 PR #47의 병합으로 06-6의 0.5일을 적립하고 06은 **완료**다. 첫 DELETE 한정이며 일반 행 변경은 08, 여러 Status의 동시 거래 일반화는 11에 남는다. 07은 PR #49에 계약(07-1), C 형식·전이(07-2), 제품 경로와 메모리 시험(07-3), native fault cut(07-4)이 함께 있다. 07-4는 07의 마지막 항목이라 §3의 행 조건(C/A/B·loser·replay/base4 연결, rollback·회수·native fault cut)을 모두 채워야 적립한다. 연결·rollback·회수는 07-3이, native fault cut은 07-4가 채웠으므로 PR #49의 병합으로 3일을 적립하고 07은 **완료**다. 첫 UPDATE 한정이며 일반 행 변경은 08, 인덱스 동반 변경은 09, 여러 Status의 동시 거래 일반화는 11에 남는다. 08–19는 이 기준선에서 미완료이며 기존 부분 공급을 다시 개발하라는 뜻은 아니다. 착수 때 source별 기존 공급을 확인한다.

01–05의 과거 비교는 `실제 14 / 사후 예상 28 = 0.500`이고, 중앙 견적보다 경과가 50% 짧다. 사후 견적이므로 AI 생산성 개선이나 미래 일정 단축의 실증값은 아니다. 앞선 대화의 약 30%는 실제 경과 14일과 미래 예상 기간을 혼합한 값이고, 약 40%와 PR 초안의 39.66%(23/58)는 잔여를 35일로 배정한 예상량끼리의 비교였다. V1은 **34.30%**에서 적립을 시작했고, PR #47의 병합으로 34.88%, PR #49(07)의 병합으로 38.37%가 된다.

<a id="forecast"></a>
## 5. 앞으로의 날짜 예측 — 가중치와 별도 갱신

아래 중앙 일정은 2026-11-03을 기준일로 두고 11-04부터 매일 현재와 같은 투입을 유지하는 기본 순차 배치다. 주말 제외 근무일표가 아니다. audit·fixture는 앞당겨 준비하고, 성능 실행과 다른 부하의 간섭을 피한다. 병행으로 끝나는 시점이 바뀌면 이 표의 예측만 갱신하며 ID 가중치는 유지한다. 06은 남은 06-6의 0.5일을 11-04에 배치했고 PR #47로 공급했다. 병합이 11-04를 넘기면 이 표의 날짜만 갱신한다. 17-4(0.5일)는 18-1·18-2 뒤에 적립되지만, 행 단위 배치라 17의 날짜에 넣었다(끝나는 날짜·% 같음).

| ID | 중앙 예상 시작–종료 | 기준 예상 기간 |
| --- | --- | ---: |
| 06 (06-6) | 2026-11-04(PR #47) | 0.5일(하루로 배치) |
| 07 | 2026-11-05–11-07 | 3일 |
| 08 | 2026-11-08–11-14 | 7일 |
| 09 | 2026-11-15–11-20 | 6일 |
| 10 | 2026-11-21–11-24 | 4일 |
| 11 | 2026-11-25–12-01 | 7일 |
| 12 | 2026-12-02–12-07 | 6일 |
| 13 | 2026-12-08–12-11 | 4일 |
| 14 | 2026-12-12–12-14 | 3일 |
| 15 | 2026-12-15–12-17 | 3일 |
| 16 | 2026-12-18–12-20 | 3일 |
| 17 | 2026-12-21–12-24 | 4일 |
| 18 | 2026-12-25–12-27 | 3일 |
| 19 | 2026-12-28–12-30 | 3일 |

- **현재 관리용 예측:** 06-6 뒤 남은 45–67일, 중앙 56일 → 완료 12-19–2027-01-10, 중앙 12-30(07이 11-05에 시작). 범위는 PR 초안(35±7일)과 같은 ±20%(일 단위 반올림)이며, 과거 재견적과 같은 보수적 척도다.
- **과거 비율을 단순 적용한 참고 시나리오:** `45–67 × 14/28 ≈ 23–34일` → 11-27–12-08. 최종 대형·성능·장애 인수가 처음이므로 이 값을 관리 일정으로 자동 채택하지 않는다.
- 실행 조건·공급 차단·새 결함·실제 시험 비용이 확인되면 기준일, 남은 기간 범위, 중앙 완료일, 변경 이유를 갱신한다. 며칠 흘렀다는 이유만으로 %를 올리거나 기준 가중치를 다시 계산하지 않는다.
- 표의 검증·수정 기간이 소진돼도 실패를 PASS로 바꾸지 않는다. 불확실성은 ETA에 반영하고, 범위가 바뀔 때만 다음 절의 버전 절차로 기준선을 바꾼다.

<a id="progress-rule"></a>
## 6. 앞으로의 누적 % 규칙

1. **고정 분모:** V1의 합계는 86일이다. 끝난 [일정 항목](#items)의 배정일 합계를 분자로 쓴다. `진척률 = 적립일 / 86 × 100`.
2. **일정 항목 단위 적립:** 항목은 [§3](#completion) 첫 문단의 항목 조건(구현, 변경 코드 시험, 독립 검토 필수 지적 해소, 증거 링크)을 채우고 main에 병합되면 배정일 전액, 그 전(대기·진행 중·검토 중·PR 미병합)에는 0이다. [적립 시점이 따로 있는 항목](#items)은 그 시점을 따른다. **행에서 마지막으로 남은 항목(번호 순서와 무관)은 그 행의 §3 조건을 모두 채워야 적립하며, 그 전에는 0이다.** 그래서 행의 모든 항목이 적립되면 행이 완료다. 항목 안의 부분 작성량·시험 개수·실제 소요일·경과 날짜로 임의 비율을 주지 않는다. 끝난 항목이 후속 기능에서 재사용돼도 다시 적립하지 않는다.
3. **항목 조정:** 착수 때 실제 단계가 항목과 다르면 그 행의 끝나지 않은 항목 안에서 다시 나눈다. 재분할은 영향 받는 항목의 첫 PR을 열기 전에 §2.2에 반영해 병합하고, 전후 항목·배정일과 이유를 [§7](#decision)에 남긴다. 행의 배정일 합은 그대로이고, 끝난 항목의 배정일은 바꾸지 않는다. 이미 끝난 산출물에 맞춰 유리한 배정일을 사후에 주지 않는다. 01–05와 06-1–06-5의 사후 분할은 병합 전 V1 결정으로 §2.2와 [개정 기록](#decision)에 드러냈다. 행 배정일 자체가 크게 모자라면 6번의 V2로 고친다.
4. **업데이트할 기록:** 기준일·source, ID·일정 항목 상태, 병합 commit/PR, 실제 착수/종료 또는 미측정, source별 검증·검토 링크, 적립일, 누적률, 남은 ETA를 함께 갱신한다. 증거 본문은 원 구현/증거 문서에 두고 여기는 연결한다.
5. **완료 취소:** 필수 조건을 무효화하는 결함·revert가 확인되면 해당 항목과 행을 재개하고 그 항목의 적립을 회수한다. 감소 이유와 이전 값을 남긴다. 진척률이 반드시 단조 증가해야 한다고 강제하지 않는다.
6. **범위 변경:** 실제 소요일 초과만으로 분모를 늘리지 않는다. 신규 기능·필수 의무 추가/삭제 또는 초기 배정의 중대한 오류가 확인되면 V2로 개정하고, 변경 ID·전후 예상일·분모·전후 %·이유·검토를 기록한다. 이전 V1 값과 증거는 Git 이력과 개정표에 보존한다. 완료 인수 의무 변경은 기존 권한/계약을 따른다.
7. **100% 조건:** 01–19의 모든 일정 항목 적립뿐 아니라 [단계 완료 정본](stage-completion.json)의 DEV3 필수 의무와 최종 S5가 닫혀야 한다. 새로 확인된 누락 의무는 일정 항목으로 추가해 V2로 분모에 넣고, 숨긴 채 100%를 보고하지 않는다.

예: 06-6이 끝나면 `(29.5+0.5)/86 = 34.88%`(PR #47), 07까지 끝나면 `(30+3)/86 = 38.37%`다. 07-1만 먼저 끝나면 0.5일(+0.58%p)만 더한다. 표의 목표 누적률을 건너뛴 작업까지 실제 완료로 읽지 않는다.

보고 형식: `DEV3 V1 / 기준일·commit / 적립 33/86일 = 38.37% / 이번 적립 0 / 현재 A1 상세 설계: 진행·완료 산출물 링크(일정 항목 밖, 적립 없음) / 잔여 ETA 재견적 전`. %p 역시 원 배정일로 계산하며 표시 반올림의 차이는 최종 누적값을 우선한다.

<a id="review-adoption-20261104"></a>
## REVIEW-012 — 기존 일정에 대한 범위 배정과 재견적

[리뷰 채택 계획 A0–A7](dev3-implementation-plan.md#review-adoption-20261104)의 산출물을 기존 공급과 구분한다. **이번 문서 계획의 적립은0이며 V1은33/86일=38.37%, 미적립53일 그대로다.** 신규 구현의 규모가 정해지기 전이므로53일을 새 범위의 잔여 ETA로 사용하지 않는다. §5의12-30 중앙 예측도 현재 납기로 재사용하지 않는다.

| 작업 | 기존 항목과 관계 | 견적·적립 처리 |
| --- | --- | --- |
| A0 | 현재 의미 유지·차이 정리 | 계획 산출물, 제품 완료 추가 없음 |
| A1(F6-b) | 기존18-3의 WAL 재검증 개선과 중복되는 범위를 먼저 설계 | A1 상세 설계는 V1 적립 항목 밖의 선행 범위 분석으로 별도 보고하며 적립 없음. 기존18-3의1일은 M1 판정/조건부 개선의 원 배정; 새 authority·복구·F05 구현이 모두 들어간다고 가정하지 않음 |
| A2 | 기존08/09/12/14의 일반 형식·Undo·회수와 겹침 | 기존 필수 일반화와 새 최적화/형식 대체의 증분을 분리. 선택한 새 필수 범위는 V2 여부 판단 |
| A3/A4 | 기존11/12의 동시성·다중 객체·복구 | 동시 거래 통합과 실제 병렬 writer/queue·sync 변경을 같은 완료로 세지 않음. 선택안의 추가 코드·fault·상한 비용을 재견적 |
| A5 | 기존08–11의 일반 row/index/Catalog | own/repeated write·overflow·split·인덱스·DDL의 기존 의무. 이번 계획이 별도 점수를 추가하지 않음 |
| A6 | 기존13/15/16의 checkpoint/audit/restore | 기존 인수에 비용 상한·계수 입력을 연결. 알고리즘 대체·신규 persistent inventory/spill을 택하면 증분 범위를 견적 |
| A7 | 각 구현 항목의 F05·작은 계수,18의 대형/PF09 | 작은 검증은 해당 구현에 한 번, S5 확대는18에 한 번만 배정.64GiB 준비 의무를 새 작업으로 이중 적립하지 않음 |

이번에는 PR 리뷰의 선택지(b), **A1 상세 설계는 일정 적립 항목 밖이며 적립 없음**을 채택한다. 진척률과 별도로 설계의 진행/검토/완료 산출물·잔여 의사결정을 보고하고, 설계 소요·대기는 ETA에 포함한다. 비적립 A1 설계 PR은18-3의 첫 인수 산출물 PR로 세지 않으므로 §6.3의 재분할 시점과 구별한다. 이 PR에 제품 구현·M1·18-3 적립 산출물을 섞지 않는다. 설계 자체를 적립 항목으로 바꾸려면 그 첫 설계 PR을 열기 전에 재분할/V2를 먼저 반영해 병합해야 하며, 설계를 마친 뒤 소급 점수를 주지 않는다.

첫 후속 A1 상세 설계 PR에는 후보별 코드/계약/fixture 범위, 지원 opcode, 영향 item, 새 의무와 재사용 산출물, 공급 단위별 예상일·불확실성·선행을 포함한다. 최적화 후보를 선택하면 **영향받는 적립 항목의 첫 산출물 PR(구현 또는 M1 등)을 열기 전에** §6에 따라 (a) 같은 행/범위 안의 재분할 또는 (b) 새 필수 범위/부족한 배정의 V2 개정을 반영해 병합한다. 이 문서 요청만으로 근거 없는 날짜나 예상일을 확정하지 않는다. 이후 A2–A7도 실제 채택된 범위별로 같은 절차를 적용하며 모든 후보 설계의 일괄 완료를 기다리지 않는다.

V2가 필요하면 영향 ID·전후 예상일·분모·완료분 처리·전후 누적률·ETA·채택/보류 이유·독립 검토를 함께 남긴다. PR #49까지 완료한 제한 공급은 보존하고 새 일반화의 완료 점수를 사후 추가하지 않는다. §2/§2.2의18-3 및 §3의 M1 적립 조건은 V1 기록으로 유지한다. 다만 “현 방식 유지”로 [A1 비용 목표](../contracts/dev3-task-04-wal.md#a1-cost-target)의 미달을 닫을 수 없으며, 후보 선택/목표 미달 뒤 재배정에서 원18-3 판정과 새 개선 의무를 구분한다. 설계를 앞당긴다는 이유로 M1을 실행하거나 그 조건을 면제하지 않으며, 재배정 때 A1로 중복되는 개선 범위와 남은 측정 판정을 명시적으로 분리한다.

<a id="decision"></a>
## 7. 결정·검토·개정 기록

| 항목 | 기록 |
| --- | --- |
| 결정 | D3-SCHEDULE-001, D2, 2026-11-03; 사용자 요청으로 예상 일정 기준의 누적률을 문서화. PR 제안, 독립 D2 ACCEPT(필수 지적 0). 같은 날 PR 리뷰 뒤 사용자가 결정 카드에서 "병합 전 잔여 재배정"을 고르고 "기존 가중치도 변경이 필요하면 변경"을 덧붙여, 병합 전에 01–19를 공급 단위로 재배정. 이어 사용자가 "일정 기준"을 확인하고 "작업을 세분화"하라고 해, 행 단위 0/100 적립을 일정 항목 단위로 바꾸고 06–19를 항목으로 나눔. 06은 이미 병합된 PR #41–44·#46에 맞춘 사후 분할이며, 재배정 r2 ACCEPT 시점의 28일·32.56%에서 06-1–06-5의 1.5일을 소급 적립해 29.5일·34.30%가 됨(새 작업 아님). 이어 "이미 끝난 작업도 세분화"에 따라 01–05도 항목으로 나눔(적립 변화 없음) |
| 담당 | 초안 작성·계약·통합 `/root`, 독립 검토 `/root/schedule_review`; PR 리뷰·재배정 별도 Claude 세션, 재배정 독립 검토는 별도 AI 실행 |
| 선택 | 28+58=86 기준일(공급 단위 × 2일, 06을 1단위로), 원 배정일 합산, 증거로 끝난 일정 항목만 적립, ETA 분리 |
| 대안 | 기능 수 균등분할은 난도를 반영하지 못함. PR 초안의 잔여 35일 하향 배분(행마다 2~3일)도 같은 이유로 병합 전 사용자 결정에 따라 대체. 행 단위 0/100 적립은 행의 완료 조건이 닫힐 때까지 끝난 일정 항목을 %에 보이지 않게 해 병합 전 사용자 요청으로 대체. 실제 경과/예상 잔여 혼합은 지연만으로 %가 오름. 매번 잔여 재견적으로 가중치를 바꾸면 같은 완료 상태의 %가 흔들림 |
| 범위 | 일정 보고만. 제품 기능·형식·자원 한도·S5 inventory·기존 실행 결과는 변경하지 않음 |
| 기존 문서 관계 | [구현 계획의 개수 기반 진척률 금지](dev3-implementation-plan.md#work-organization)는 유지하면서 예상 기간 가중 보고를 추가. [AI 처리량 계획](dev3-ai-throughput-improvement.md)의 미측정 과거 시간을 실측처럼 채우지 않는 규칙도 유지. 과거 8–12주 판단을 소급 확정 일정으로 만들지 않음 |
| 검토·검사 | [독립 D2 검토](../../module-gate/evidence/dev3-schedule/review.md) ACCEPT, 필수 지적 0. 일반 행 변경의 배정·S5 실행분 적립 조건을 보완하고 audit→backup 선행을 맞춤. 병합 전 재배정은 [별도 독립 D2 검토](../../module-gate/evidence/dev3-schedule/review.md#reweight)를 받음: r1 ACCEPT WITH REQUIRED CHANGES(09·11의 key 대기 이중 계수, 05 적립 증거 링크, 검토 상태 표시; 선택 지적 중 §2.1–§3 대응·14의 06 제외·F6-b 조건은 반영, S5 후반 가중치는 일부 반영), r2 필수 1건(17·18의 FT06 이중 배정) 반영 뒤 ACCEPT. 일정 항목 적립과 01–19 세분화는 [같은 방식의 독립 D2 검토](../../module-gate/evidence/dev3-schedule/review.md#items)를 받음: r1 ACCEPT WITH REQUIRED CHANGES(사후 분할·소급 적립의 개정 기록, 항목 적립 조건과 행 마지막 항목의 §3 조건, 재분할 시점, 누적 산출물 항목의 적립 시점, §2.1 합계), r2 필수 1건(01–05 항목의 제한 공급 표시) 반영 뒤 ACCEPT. 제품 변경이 없어 제품 시험은 실행하지 않음. 자동 문서 검사는 기존 pre-push 훅에서 수행하며 결과는 PR에 기록 |
| 재검토 | 범위 누락·이중 적립·완료 증거 불일치, 필수 인수 변경, 분모 변경 필요 시 |

| 버전 | 기준일 | 전체 / 적립 | 누적률 | 변경 이유 |
| --- | --- | --- | ---: | --- |
| V1 초안 | 2026-11-03 / `cd41b174` | 58일 / 23일 | 39.66% | PR 첫 commit. 완료분 사후 재견적 23일, 잔여는 앞서 제시한 28~42일의 중앙 35일을 2~3일씩 배분. 병합되지 않음 |
| V1 재배정 | 2026-11-03 / `f5b731c9`→`ab2537eb` | 86일 / 28일 | 32.56% | 병합 전 PR #44(DELETE 2-b-2) 상태를 반영하고, 사용자 결정(카드 "병합 전 잔여 재배정"과 "기존 가중치도 필요하면 변경")으로 01–19를 공급 단위로 재배정. 재배정 독립 D2 r2 ACCEPT. 행 단위 0/100 적립이라 06은 0일. PR #46 반영 뒤에도 같은 값. 병합되지 않음 |
| V1 | 2026-11-03 / `ab2537eb` | 86일 / 29.5일 | 34.30% | 사용자 요청("일정 기준", "작업을 세분화", "이미 끝난 작업도 세분화")으로 적립을 일정 항목 단위로 바꾸고 01–19를 항목으로 나눔. 분모·행 배정일은 그대로. 이미 병합된 PR #41–44·#46에 맞춰 06을 사후 분할해 06-1–06-5의 1.5일을 소급 적립(새 작업 아님). 06은 진행 중 |
| V1 | 2026-11-04 / PR #47 병합 | 86일 / 30일 | 34.88% | 06-6(DELETE 3단계: native fault cut RDL-N01..N06, RDL-J11)의 병합으로 06 완료. 분모·행 배정일·항목은 그대로 |
| V1 | 2026-11-04 / PR #49 병합 | 86일 / 33일 | 38.37% | 07-1(UPDATE 형식·전이 계약, 독립 D2 2회차 ACCEPT), 07-2(C 형식·전이), 07-3(A/B 제품 경로·복구, 메모리 시험), 07-4(native fault cut RUP-N01..N06)의 병합으로 07 완료. 분모·행 배정일·항목·중앙 예측은 그대로(07은 예측보다 하루 먼저 11-04에 시작해 같은 날 끝남) |

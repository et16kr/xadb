# DEV3 테스트 계획 — 항목별 필수·누락·실행 비용 검토

2026-10-22 · **설계·실행 계획 독립 검토 PASS / D3-TEST-PLAN-002 ACCEPTED / 제품·테스트 실행 없음**

[개선한 테스트 계획](../plans/dev3-test-plan.md) · [16개 항목별 보완](../plans/dev3-test-plan.md#item-review) · [실행 셀 목록](../plans/dev3-test-plan.md#case-inventory) · [비용 제어](../plans/dev3-test-plan.md#cost-control)

## 검토 범위와 판단

사용자는 DEV3의 각 테스트 계획이 필수 항목에 집중하는지, 불필요한 시간이 들거나 누락된 시험이 있는지 점검하고 개선하도록 요청했다. 계획의 중간 변경 범위 원칙과 최종 S5 DEV3 전수 예외는 타당하여 유지했다. 부족한 부분은 필수 경계를 중간 목록에 명시하는 정도, 원문 의무에서 실제 case로 내려가는 추적, 동일 시나리오의 중복 실행을 막는 기준이었다. 실제 실행 시간을 측정하지 않았으므로 시간 낭비가 발생한 양이나 절감률을 주장하지 않는다.

이번 검토는 저장소 내부 정본을 대조한 것이다. 공개 DBMS 조사나 제품 코드 결함의 실증 검토로 표현하지 않는다. §3.7의16행은 모두 확인한 항목이며, 모든 항목에서 새 테스트가 필요한 것은 아니다. 기존 case에 독립 assertion/입력이 충분하면 재사용한다.

| 기준 | 확인한 범위 |
| --- | --- |
| 실행 권한·상태 | [AGENTS.md](../../AGENTS.md)의 변경 코드 제한·S5 한정 예외, [현재 예약/취소 잔여](../plans/dev3-test-plan.md#allocation-first). 계획 요청으로 개발/시험 재개 금지 |
| 의무 분모 | [DEV3 완료 계약](../plans/stage-completion.json), [개발 단계](../plans/development-stages.json), [DS01–06의 required_variants](../contracts/disk-test-matrix.json), [기존 필수 인수](../plans/dev3-implementation-plan.md#acceptance) |
| 작업별 경계 | [16개 소유 명세](../contracts/dev3-task-designs.md#catalogue)의 요구·인수/착수 동결, 특히3.06 T06-01–16·3.09 C01–17·3.10 C10-01–16·3.11 T1–11·3.13 C01–15·3.14 R14-01–17·3.15 T1–12 |
| 후속 설계 | 기존 계획의 [REVIEW-009](../plans/dev3-test-plan.md#design-review-009)·[REVIEW-010](../plans/dev3-test-plan.md#design-review-010) 연결. 새 제품 의미 채택 없이 실행 명세에 유지 |
| 비용·검증 강도 | [DP03/DP07](../policies/development-policy.md), [3.16 자원·PF09](../contracts/dev3-task-16-qualification.md), [테스트·리뷰와 최소 시험 환경](../policies/coding-convention.md#section-10), [문서 push gate](../policies/documentation-policy.md#push-gate) |

## 발견 사항과 반영

| ID | 기존 계획의 위험 | 반영·위치 |
| --- | --- | --- |
| TP01 | “핵심 실패/경계”만으로는 pre-effect·부분 effect·정리 실패, 최대 단일 입력과 대형 반복을 혼동할 수 있음 | [필수 선정 단위](../plans/dev3-test-plan.md#minimum-boundaries). 독립 의미 경계는 작은 fixture로 중간 확인, 큰 반복만 S5 배정 |
| TP02 | 중간 요약에 즉시 X-lock만 강조되어 wait/already-held·same-bytes·own token, ACK overwrite witness, profile 축소 후 기존 복구 의무 등의 계약 사례를 빠뜨릴 수 있음 | [16행 보완표](../plans/dev3-test-plan.md#item-review), NT10/11·복구3.12.1/4. 원래 계약의 사례를 중간 선택에 연결 |
| TP03 | 외부 sort·root 축소·partial batch deadline·auditor 규칙을 대규모 최종 실행까지 미루면 새 분기/검사기의 오류 발견이 늦음 | NT05/06/07/15에서 작은 deadline·split/empty-root·2-run merge·규칙별 known-bad. 대형/장시간 의무는 유지 |
| TP04 | NT/DS/RC/FT의 중복 연결과 “전체 변형”이 별도 실행 또는 무조건 직교곱으로 해석될 수 있음 | 주 case owner·공동 assertion·명시 셀/반복 identity. claim/free, q10, begin-WAL 없는 abort, restore→새 ACK 등 실제 같은 실행만 연결 |
| TP05 | case 합계가 맞아도 누락 하나와 중복 하나가 상쇄되거나 선택 이름이0건 실행할 수 있음 | [inventory](../plans/dev3-test-plan.md#case-inventory)의 양방향 추적·예상/관측 ID 집합 일치·known-bad. 미구현 의무도 목록에 유지 |
| TP06 | 제품 diff만 보거나 재시도 성공만 취합하면 oracle/fixture/feature 변화와 원래 FAIL을 숨길 수 있음 | 시험 입력·도구도 영향 분석에 포함. 정상 복구와 예상 손상 거절 분리, 실패 후 의존 job만 보류·영향 셀만 재검증 |
| TP07 | build/fixture/audit 반복과 큰 allocator profile의 불가능한 기본 용량이 시간·공간 낭비를 만들 수 있음 | [비용 제어](../plans/dev3-test-plan.md#cost-control). 불변 fresh fixture+격리 사본, 준비/실행/audit 비용 분리,64GiB qualification 선행과 host 공간 사전 확인.4배·최종 budget 축소 없음 |

위 항목은 계획의 모호성·누락 방지에 대한 지적이며 이미 실행한 시험이 잘못되었다는 판정은 아니다. 제품 결함의 수정 전 실패/수정 후 성공은 이번에 관측하지 않았다.

## 유지한 의무와 실행 제한

- DEV3 중간은 마지막 관련 검증 source 이후 실제 변경에 해당하는 필수 case만 실행한다. 이미 완료한 무변경 하위 시험·전체 wrapper를 반복하지 않는다.
- 변경 property의64 seed/최대128 operation, 변경 parser/codec의 보존 corpus, 변경 bounded model의 선언 공간은 중간 의무로 유지한다. 실제 I/O/thread/reopen은 모델로 대체하지 않는다.
- S5의 DEV3 전체 확정 inventory, property256 seed/최대1024 operation·target별 fuzz30분·영향 suite별 thread stress30분·비결정적 fault3회·4배 working set·PF09를 유지한다. 미래 계약의 미정 profile/셀 수는 미완료 입력이다.
- 3.03.1의 native13DB·compiler baseline+6 probe·component64KiB·WAL4는 유지한다. 이번에는 runner·제품·시험 코드를 작성하거나 실행하지 않았다.
- DEV0/1/2 전수, SQL/서버/클라이언트·PART/NG·미래 버전 호환을 DEV3 새 선행으로 추가하지 않았다. 실제 수정한 공통 하위 공급의 직접 영향 사례는 계속 필요하다.

## 변경 identity와 독립 검토

| 항목 | 기록 |
| --- | --- |
| 분류·담당 | D2, 작업·계약·통합 `/root`. [D3-TEST-PLAN-002](../plans/dev3-test-plan.md#review-decision) |
| 기준 HEAD | `9f79b31b99f558f4e9a98b8bd752220b851e7bbb` |
| 착수 변경 보존 | 기존 `devdoc/README.md`·`devdoc/documents.json`의 DEV6/DEV7 항목 순서 변경 보존. 기준6파일과 시작 diff 사본 `/tmp/xadb-dev3-test-review-1y12dof1`; 시작 diff SHA-256 `bc0798e725d2baf1e20c5b0052996a729e9096669332965135980e334189df37` |
| 동시 작업 | 이후 DEV4–7 작업 분류 문서와 DEV7 검토 기록의 별도 변경을 관측. 해당 내용은 이번 수정/검토 대상에서 제외하고 공유 색인은 최신 파일의 DEV3 항목만 수정 |
| 변경 범위 | 테스트 계획·구현 계획의 실행 안내·본 검토 기록·plans/evidence/루트 README·documents.json 등록. 제품 소스·과거 receipt·완료 JSON의 제품 상태는 미변경 |
| 독립 검토 | `/root/dev3_test_plan_review`가16개 owner·DS01–06 required_variants·DEV3 완료 계약·KU09/10·RC-T12–14·PF09와 이번7개 파일 변경을 읽기 대조하여 PASS. 중간 필수 과잉/누락·S5 분모/DP03 budget·공유 실행의 증거 종류·inventory 부정 입력·비용의 미측정 구분을 확인. coding-convention 링크 지적을 수정하고, profile 손상 거절 분리·명시 반복 유지·동시 작업 제외도 재확인. 미해결 필수 수정 없음. `/root` 반영 확인 |
| 검토 후보 identity | 수용 기록 반영 전 SHA-256: 테스트 계획 `a28c02a39195c2face86723572a843c71012810f5dbd8ec576e370f0a1555b8b`, 구현 계획 `c6effcb5b2587fc46e12348ac20fa04ec91c5905f17566eb244d8771b54ead29`, 본 기록 `b723fbebb5841a7c2c94ad8876e6df69aff73e46fc83725e0c7ff6b6e0b273d9`. 기존001·REVIEW-009/010의 PASS를 승계하지 않음 |
| 검증·제한 | 정본/변경 문서의 읽기 대조만 수행. 제품 시험·compiler/Clippy·문서 자동 검사·commit/push 없음. 기존 결과와 DEV3/S1 미완료 유지 |

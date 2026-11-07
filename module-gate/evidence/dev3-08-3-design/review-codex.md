# PR #66 — 추가 설계 검토와 새 검증

2026-11-07. 기준 PR head `928ec853348a0f197a0da0b65574eb386237499f`,
base `5f70d2bb4126111159439d8386f17a8c3df70fc0`. 작성·수정·통합 담당 `/root`,
독립 검토 `/root/overflow_design_review`. 대상은 O0 설계이며 제품 구현이 아니다.

[초기 독립 검토](review-codex-initial.md) · [수정 후 독립 검토](review-codex-final.md) ·
[4판 계약](../../../devdoc/contracts/dev3-row-overflow.md) · [새 oracle](test_design_oracles.py)

## 발견과 수정

| 필수 지적 | 원래 설계의 문제 | 4판 반영 |
| --- | --- | --- |
| R1, P2 | preflight는 길이를 받지 않는데 k개 Free 검사를 요구. allocator 부족이 기존 Capacity 분류를 타면 불필요한 split을 시도 | preflight는 allocator 상태 관측, 값 검사 후 reserve(plan)가 k 검사. OverflowCapacity를 leaf 부족과 분리하고 오류 순서·효과0·split0 인수 추가 |
| R2, P2 | 기존 apply는 예약을 종료하며 draft 불변·flush 금지 lease라 여러 ChunkWrite를 이어갈 수 없음 | prepared-chunk가 활성 ReservedRow를 반환하고 성공한 그룹에서만 cut 갱신. guard·준비 cut을 놓은 그룹 경계에서 보유 lease로 flush. 최종 Forward/Cancel만 discharge, stage 뒤 실패/Drop fence 유지 |
| R3, P2 | 현재 copy_candidate의 Result에는 복사 중 FlushRequired를 실을 수 없음 | C ReadStep·B Step 및 두 adapter 변경, checked cleanup 후 동일 visible 행 재시도. 부분 출력 유효0·cursor 누락/중복 방지 인수 추가 |

비용 표의 974,020 B는 행 root에 귀속되는 소계임을 명시했다. Begin·Commit·terminal·
control 및 다른 owner 의무를 포함한 전체 admission 비용과 혼동하지 않는다.
read gate도 그룹마다 여닫는 것이 아니라 preflight부터 최종 Forward/Cancel까지
연속 보유함을 바로잡았다.

## 설계와 방향성

역순 chunk 할당은 현재 frame 한도 안에서 allocator 예약 형식을 늘리지 않고
완결된 suffix를 Job 하나로 추적한다. Forward 전 전체 digest/schema 검산,
그룹별 원자적 owner 이전, Base4의 정확히 한 번 소유 검사는 타당한 방향이다.
반환을 XTR1 퇴역으로 한정하고 실제 Free·재사용을 14-2에 남기는 범위도 일관된다.

128 KiB·첫 extent·whole-page WAL·장시간 read gate는 현재 profile의 제한이다.
Oracle·PostgreSQL 단일 인스턴스급의 거래 크기·설정형 자원·동시성 목표를 달성한
설계로 평가하지 않는다. 계약 §9의 A2/A4·12-1·14-2 후속 의무를 유지한다.
이번 PR 범위를 넘어 제품 코드나 지원 한도를 확대하지 않았다.

## 신규 검증의 실제 실행

명령: `python3 -B module-gate/evidence/dev3-08-3-design/test_design_oracles.py`

**새 5개 시험 PASS**, 0.677초. 기존 제품 시험·frames-r1/r2를 재실행하지 않았다.
Python 표준 라이브러리만 사용하며 제품 encoder나 앞선 계산 script를 import하지 않는다.

| 새 사례 | 범위·독립 기대값 |
| --- | --- |
| every_supported_length | canonical 길이 2,049..131,072 전부(129,024개), 각 역순 쓰기 suffix의 index·경계·길이 합 |
| return_frames | 기존 목록 수와 반환 길이의 459조합, 두 opcode의 최대 60,864/59,056 B와 98,304 한도 |
| debt_conservation | k=1..9, written=0..k, h=0/1의 108상태. chunk 소비+잔여 보존, 취소 비용 보호, 실제 반환 frame+후속 정리 ≤ 예약 |
| atomic_owner_transfers | 45개 ChunkWrite 뒤 crash prefix와 취소·Forward·CLR·퇴역의 정확히 한 번 소유. 이전 owner 미삭제 및 새 owner 누락 mutant를 거절 |
| root_zero_projection | 독립 head bytes에서 root 변경은 projection 입력을 보존하고 실제 digest는 바뀜. total/count/digest 구간의 각 byte 변형은 commitment 입력을 바꿈 |

이 시험은 **O0의 독립 산술·추상 상태 oracle**이다. 구현되지 않은 codec·A/B/C API·
실제 crash 복구를 실행하지 않았으며, 모델이 실제 제품과 같다는 증거로 사용하지 않는다.
O1–O4의 제품 시험·native fault cut·census는 계속 NOT_RUN이다.
R1–R3의 실제 제품 회귀는 OVF-J03/J05에 추가했고 O3에서 구현과 함께 실행해야 한다.
현재 오류를 없애기 위해 인수 한도나 기존 시험을 완화하지 않았다.

독립 재검토 뒤 oracle의 두 사례를 보강했다. 같은 leaf 범주 안의 두 chain이 page를
공유하는 mutant도 거절하고, projection fixture는 실제 세 u64의 유효 PageRef를 쓴다.
바뀐 `test_atomic_owner_transfers_and_crash_prefixes`와
`test_root_zero_projection_preserves_value_commitment`만 같은 명령의 selector 인자로
다시 실행해 **2/2 PASS(0.001초)**였다. 나머지 3개는 source와 결과를 유지했다.

최종 commit의 fmt·Clippy·문서 검사는 push 훅에서 수행한다. 그 검사는
정적/문서 gate이며 위 제품 미검증 상태를 바꾸지 않는다.

# PR57 독립 검토

- 판정: **ACCEPT**, 필수 미해결 지적 **0**.
- 검토자: `/root/pr57_independent_review`; 작성자·통합 담당: `/root`.
- 분류: 원 PR 설계·구현 D2, 신규 시험 D1.
- 원 PR 범위: `ceb2fd8955217763107d6416b307a6a24f6e3495`..`7ca3b53ec2657f48303fb6294134fd6d8fa12097`.
- 신규 시험 최종 blob: `120e124b47c3cffdca5c0f51005a8c4731d725e5`.
- 검토자는 파일을 수정하거나 기존 시험을 재실행하지 않았다. 다음은 별도 검토 실행의 보고를 옮긴 기록이다.

## 원 PR D2

canonical 재계산·프레임 전체 비교·target 검증이 arena/bank 변경보다 먼저 수행된다.
퇴역 page의 Allocated 상태와 INIT claim을 보존하며 `retired`가 Free를 뜻하는 계약을 지킨다.
길이 dispatch 뒤 decoder가 opcode·길이·record 구성을 다시 검증한다.
source census의 단계별 수명, XTR1 호출, layout/body 호출과 history dispatch 산정을 대조했으며
필수 누락을 발견하지 못했다. U2 공급과 U3/U4 미완료를 구분하는 방향도 타당하다.
일부 회계 부등식만으로 비지배 항 누락을 검출하지는 못하므로 정적 inventory 검토를 함께 근거로 둔다.

## 신규 시험 D1

거절 시험은 제품 digest만 쓰지 않고 arena·Jobs·bank의 bytes, claim bytes, cursor를 직접 대조한다.
폐기 가능한 after-image scratch는 제외한다. 두 opcode의 길이 경계·확장·상대 opcode 길이,
wire-positive native digest와 마지막 redo byte 변조, 잘못된 LSN·epoch·timeline 및 중복 입력을 확인한다.
퇴역 leaf·parent·survivor의 before/after digest를 함께 바꿔 문법을 유지하는 사례가 포함되고
동일 unchanged mutant의 중복 실행은 제거됐다.

successor epoch 시험은 base 미선택·구 epoch 거절, 정상 채택, 행·claim·퇴역 목록 보존을 확인한다.
시험 capture의 offset144/1184는 실제 decoder의 source_epoch/issued_epoch다.
두 필드를 갱신하고 재봉인한 뒤 Base4 재해석을 검증한다. nonempty 시험은 손상 페이지 대신
독립 ROOT fixture의 유효한 tree를 사용해 NotNeeded·layout 수락·IdentityMismatch와 상태 보존을 확인한다.

검토자는 [최초 로그](initial.log)의 3 PASS/1 FAIL과 [후속 로그](followup.log)의 3 PASS를 읽었다.
초기 실패는 epoch replay 뒤 시험 capture의 구 epoch metadata로 인한 Base(IdentityMismatch)였다.
그 실패를 보존하고 시험 데이터만 고쳤다. 변경 없는 2개 최초 PASS와 수정·신규 3개 PASS를
합쳐 신규 5개 검증으로 판단하며 전체 suite PASS로 해석하지 않는다.

- 최초 로그 SHA256: `5783897e4a7de4bce05b51759d8dae937705dd9e338b2ea17bf9568cde551522`
- 후속 로그 SHA256: `81831d7ff13c18a8b6c8a00b0074f7a7463a5e49a7d40b57b5b85cf3e9d95353`

제품 코드 변경은 없다. pure replay·raw Base4 capture의 증거이며 실제 durable WAL 선택,
제품 rebase·native 장애 복구를 완료했다는 증거는 아니다. 그 공급은 U3/U4에 남는다.

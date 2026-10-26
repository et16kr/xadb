# DEV3 plan4 빈 거래의 실제 복구 owner

계약 ID: **D3-PLAN4-RECOVERY-OWNER-001**. 2026-10-25 중단 작업 재개 범위의
구현·시험 입력 정리이며 최종 독립 D2·실행 인수는 [재개 증거](../evidence/dev3-recovery-resumed-20261025.md)를 따른다.
[원 중간 저장](../evidence/dev3-recovery-paused-20261025.md),
[순수 복구 형식](dev3-recovery-control-base.md), [fresh live](dev3-retention-live.md)를 구분한다.
원 wire SHA256 `0561d2c1fc40daeacf65d530b245932a8daf4b213e9476ca1df72a48023e3ae2`와
로컬 `artifacts/dev3/plan4-g6-empty-retention-20261024/recovery-owner-source-design-v3.md`의
N=0 복구 의미를 실제 API에 연결한다. 새로운 형식·일반 Undo·Catalog CREATE/CLR·첫 행 거래는 추가하지 않는다.

## 입력과 소유권

`D3-MODULE-ADOPTION-001` **D2 ACCEPTED 결정**은 기존 N=0/Catalog 결과를 유지하고 [Row의 복구 action/입구](dev3-module-boundaries.md#api-candidates)를 별도 확장으로 연결한다. 실제 C는 current format의 canonical analysis/replay·원 effects·CancelReserved/AbortEnd·committed maintenance·VERIFY/STABILIZE를 소유하고, A는 새 loser Row CLR 후보만 생산한다. 작은 제품 open driver가 action을 조립하며 C가 A 제품을 역으로 import하지 않는다. 새 format/profile/Row 의미는 [소비자 dispatch](dev3-module-consumers.md#format-dispatch)에 따르고 과거 format 결과를 새 공급의 PASS로 바꾸지 않는다.

C+B의 `RejectInitialLosers` 선택은 selected base와 complete WAL prefix로 재구성·검산한 최초 canonical 상태에서 모든 Reserved inventory(SystemPage/Catalog/Row)와 ATT의 Active/Aborting을 검사한다. 위치는 preinspect 완료 뒤·physical 예약/쓰기 및 REBASE/Cancel/Abort 전이다. base의 과거 상태만 검사하거나 공통 cleanup 뒤에 loser0을 관측하지 않는다. 성공한 실제 분석 seal과 source identity가 원 runtime/recovered handoff를 보호하며 caller의 enum·count는 증거가 아니다. 거절은 Analysis/Unsupported, lease를 보존한 CallerMustResolve/fence, database_effect Unchanged·mutation0·READY0으로 반환한다. legacy recover의 기존 범위를 넓혀 새 Row 정책을 우회하지 않는다. 이미 terminal인 Committed/Aborted의 남은 정리는 새 Row CLR 없이 C가 진행할 수 있다. 정확한 signature·실패 소유·선택 인수는 경계/소비자 문서에 두며 현재는 미구현이다.

`Plan4Recovery::acquire`는 실제 directory의 writer lease 하나와 caller의 maintenance12MiB,
DWB4MiB, 새 WAL slot1MiB를 유지한다. 처음부터 실제 W/File 타입의 자원 상한을 확인한다.
획득 전 실패와 획득 뒤 namespace 효과, 실제 database 효과를 구분한다. 획득 뒤 실패는
원 owner·lease·입력 loan과 알려진/가능한 선택을 반환하며 Drop에서 복구·삭제·commit하지 않는다.

`recover`는 선택→정규 분석→원본 안정화→Pending/home 처리→REBASE→새 epoch 공개를 수행한다.
`SelectedEpochParts`만 원 writer·실제 새 File·WAL slot·fence·Condvar를 기존 runtime에 옮긴다.
Fresh 또는 합성 WalRequirement를 생성하지 않는다. 새 epoch의 초기 offset4096와 base LSN은
검증된 빈 suffix를 나타내며 아직 쓰지 않은 frame의 durable 권한이 아니다.

`Plan4RecoveryRuntime::bind`는 같은 concurrency core의 유효한 Owner를 확인하고 원 binding CAS와
한 SharedPool을 사용한다. control/replay loan은 한 번만 인계한다. 복구 facade는 `step`과
관측만 제공하며 검증이 끝나기 전 foreground live API를 공개하지 않는다.

## 선택·분석과 물리 복구

CURRENT의 current/previous는 명시된 동일 continuation closure 안에서만 fallback한다.
SHA가 유효하지만 지원 의미를 벗어난 base, 이름만 맞는 다른 inode, 선택된 WAL 누락,
알 수 없는 세 번째 WAL을 오래된 것으로 추측해 채택·삭제하지 않는다.

실제로 재생한 header와 complete frame bytes를 streaming hash에 포함하고, 이후 읽은 원본 prefix와
대조한다. 불완전 active tail은 채택 경계 밖에 둔다. sealed 경계의 offset/LSN 불일치는
`OrderMismatch`, 같은 경계의 digest 불일치는 `ContentMismatch`다. 두 번의 사후 hash만 같다는
이유로 이미 재생한 바이트를 인증하지 않는다. 첫 변경 전 선택된 제어 내용과 이름·identity·길이를
다시 확인한다.

선택된 Pending image를 정확 target·LSN의 canonical image와 대조한다. home에 쓰기가 필요하면
기존 DWB 순서를 사용한다. [일반 Pending 규칙](dev3-task-12-recovery.md#5-3122--doublewrite_repair)의
한정 보완으로, plan4 N=0에서는 같은 target·owner·incarnation의 home 전체 바이트가 선택된
base와 완결 suffix 안의 정규 후속 이미지임을 증명한 경우에만 더 높은 LSN의 home을 유지한다.
Pending 자체도 같은 채택 이력의 정확한 이미지여야 한다. LSN 대소만으로 승인하지 않으며
다른 incarnation·identity, 채택 범위 밖의 미래 이미지, 전체 바이트 불일치는 거절한다.
home이 이미 같거나 이 조건을 만족하여 쓰기를 생략해도
실제 home 파일의 sync와 재검산이 끝나야 Empty retirement를 공개한다. 이 sync의 전/후 오류는
Pending을 보존하고 `Pending/Io`와 실제 효과를 보고한다. LSN0의 손상 home에 가짜 양수 WAL
권한을 만들어 수리하지 않는다.

REBASE는 선택되지 않은 base에 쓰고 sync/readback한 뒤 CURRENT를 공개한다. 두 사본이
원 source를 닫은 다음에만 정확히 선택된 old WAL·intent를 회수한다. persisted intent가
새 epoch create보다 먼저이며 issued high-water를 되돌리지 않는다. WRITER와 CURRENT의 최종
WalOpen 공개·directory sync·readback 전에는 새 live owner를 반환하지 않는다.

## 복구 종료와 같은 pool의 재사용

Reserved allocation은 새 epoch의 첫 동작으로 cancel한다. Active는 RollbackStart→AbortEnd→Finish,
Aborting은 AbortEnd→Finish, Ready Job은 Finish를 수행한다. 기존 descriptor·ATT·Aging·credit을
한 번만 복원하며 새로운 foreground 예산으로 잔여 의무를 대체하지 않는다.

VERIFY/STABILIZE는 정규 home, 빈 ATT, Reserved 없음, Aging 잔여0, 원 target identity,
DPT/pin/claim/Pending 정리와 정확 DWB 선택을 확인한다. `into_live`는 검증한 facade의 같은
pool·core·fence·loan을 소비해 반환한다. 이 경로에서 새 Begin과 이후 재시작의 Catalog read를
실제 소비자로 검증한다. 재진입에서도 새 epoch·base·control 쓰기는 정상이며 중복 home 복구 쓰기0과
프로세스 전체 쓰기0을 혼동하지 않는다.

## 자원·검증 경계

source-value 한도는 local65,536, metadata196,608, whole262,144byte다. maintenance/DWB/WAL 및
frame/operation caller backing은 별도로 유지한다. bind의 pool 생성·retention 전환·facade 반환과
frame 의미 검산·관측값 반환·closure 갱신을 실제 함수 수명으로 분리하며, 각 단계의 큰 반환값도
계산한다. 이 회계는 compiler stack이나 RSS의 측정값이 아니다.

중간 필수 inventory는 [복구 owner 입력](../../tests/fixtures/dev3-plan4-recovery-owner/input-plan.json)의
개별 ID로 동결한다. 원48(static2/memory43/native3), consumed-frame 변조 회귀 E09,
sealed 경계의 digest/LSN 오류 구분 E10을 더한50개를 구분한다.
S5 확대 후보와 전체 DEV3 인수는 별도다. 신규/변경 코드의 필수 사례만 선택하고 완료한 fresh75·
pure272·하위 wrapper를 반복하지 않는다. 실행 전에 source/ELF/input, 자원 수용과 독립 oracle 검토를
결합한다. 실제 Linux producer→recover→reentry는 같은 ELF의 서로 다른3PID로 실행하고 파일 전체,
trace·entropy·효과·실패를 보존한다.

Linux 자원 관측은 최대 data File3과 DB FD6을 구분한다. FD6은 caller directory1,
lease 내부 directory1, flock1, data File3의 합이다. Writer wrapper Drop 뒤 남은 File은
원 lease Arc를 유지하므로 마지막 close만 허용하며, 추가 I/O·새 File 획득은 허용하지 않는다.
directory Drop 때 열린 data File은0이어야 한다. native 전체30초·worker 합29초를 유지하고
마지막 감사에1초를 예약한다. 사전·단계 사이 감사도 전체30초에 계상하며 이1초를 모든 감사의
누적 상한으로 해석하지 않는다. 실패한 native 재시도는 독립 감사가 확인한 미완료 ID만 대상으로
원 시간·출력·보존 파일을 합산한다.

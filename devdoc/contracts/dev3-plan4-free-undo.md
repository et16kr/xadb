# DEV3 원 plan4의 Free Undo 페이지 수명

2026-10-25 · **D3-PLAN4-FREE-UNDO-001 / ACCEPTED, 독립 D2·D1 PASS / 제한 Free container 구현·선택 검증 완료**

[원 live owner](dev3-retention-live.md) · [Undo 초기 표현](dev3-undo-format.md) · [현재 복구 owner](dev3-plan4-recovery-owner.md) · [복구 base](dev3-recovery-control-base.md)

담당은 `/root/page_lifetime`(할당·INIT·원 pool), `/root`(base·선택 복구·통합), `/root/undo_design`(독립 입력), `/root/dev3_review`(독립 D2/D1)다. 사용자 DEV3 완료·단계 commit·정리·병렬 작업 위임 아래 D2를 구체화한다. 실제 Free Undo container 공급을 먼저 연결하고, Root/Job v2만 별도 구현하는 대안은 보류한다. 여러 root의 CLR/terminal 제안은 보상 완료 구간·동시 debt·Commit/publication·nonempty REBASE가 미결이므로 채택하지 않는다.

## 1. 실제 공급과 권한

기존 plan4의 같은 InstalledStorageCore·MutableStorageOwner·WAL runtime·writer lease·fence·SharedPool에서 count-one SystemAllocation reservation을 소비한다. 원 namespace의 logical object3/gen1을 위한 Undo level0, allocator가 발급한 page9..63/incarnation≥2, 실제 INIT LSN, slot0 Free generation1인 canonical 페이지를 설치한다. 원 plan3의 UndoRole이나 raw kind 숫자를 plan4 권한으로 전용하지 않는다. plan4 core를 빌린 private role와 exact reservation/claim이 producer의 입력이다.

기존 `encode_initial_undo_page`와 mixed INIT algebra를 재사용한다. Undo slot reserve·Live/Releasing·UndoSequence·row/Catalog 변경·Commit·페이지 회수는 아직 공급하지 않는다. 빈 container는 Undo object의 정상 allocated 저장공간으로 남으며 거래의 ObjectPages queue를 요구하지 않는다. 초기화한 page를 할당 전으로 되돌리는 기능은 없다.

linear reservation 동안 다른 append를 막고 adjacent INIT/cancel을 유지한다. Unchanged 실패는 동일 token을 반환하고, stage·부분 적용·불명 I/O 이후에는 취소 가능 성공을 반환하지 않는다. 원 fence와 obligation/claim을 복구로 인계한다. Drop은 숨은 cancel/INIT/sync 성공을 수행하지 않는다.

## 2. 명시적 plan4 SystemInit 범위

원 WAL envelope v1, XSG1 header192, OperationCore256, descriptor1536 폭을 유지한다. 새 plan4 allocation/INIT 그룹은 **XSG1 version u16@164=2, flags u16@170=1(Plan4SystemInit)**로 명시한다. offsets는 완결 frame 시작 기준이다. OperationCore의 기존 reserved bytes를 완화하지 않는다. 기존 plan3 Mixed/Status v1 producer와 strict 순수 관측은 유지한다.

새 plan4 runtime의 reserve_status와 reserve_undo는 모두 이 단일 SystemInit scope와 explicit MixedInit114688 budget을 소비한다. terminal 길이로 scope를 역추정하거나 BudgetMismatch 후 다른 scope로 재시도하지 않는다. reserve가 실제 payload role를 아직 선택하지 않아도 두 허용 role의 최악 INIT를 예약하므로 복구는 동일 scope에서 cancel할 수 있다. initialize 시 Status2 또는 Undo3 private role를 actual claim에 결합한다.

| 연산 | 완결 frame bytes | 종료 뒤 allocation terminal |
| --- | ---: | --- |
| Reserve | 21200 | 0(active) |
| Cancel | 21200 | 1(Cancelled) |
| Status INIT | 38000 | 2(InitializedStatus) |
| Free Undo INIT | 38208 | 3(InitializedUndo) |

reserve의 terminal WAL 예약은38208, protected는38208+2×16384=70976이다. Status-only38000/70768로 Undo를 수용하지 않는다. 실제 consume은 terminal별 bytes이며 취소·Status의 미사용 차액은 반환한다. high-water·birth·incarnation을 되감지 않는다. 미발급 identity나 decoder 관측은 live claim·durable receipt가 아니다.

현재 개발 형식의 current-build plan4 runtime은 이 scope와 base v2만 계속 쓴다. old plan4 v1 files의 계속쓰기는 지원하지 않으며 첫 효과 전에 명시 거절한다. 기존 v1 순수 codec/원 증거는 보존한다. 이 선택을 이전 source가 새 형식을 실행한 PASS로 표시하지 않는다.

## 3. 선택 base와 원 INIT provenance

XRB1 version u16@4=2 자체가 fixed Plan4SystemInit scope다. 별도 scope scalar를 중복하지 않고 header305..320은 reserved0을 유지한다. typed Header.format은 V1Status=1/V2System=2 관측값이다. 기존 v1 decoder는 Status-only이며 terminal3·FreeUndo tag를 거절한다. 실제 capture/selected runtime은 v2만 사용한다.

BASE1MiB, descriptor36×1536, allocator1page, queue6page, payload record64×352, certificate64×96, inventory1page와 기존 offsets를 유지한다. Status record tag1·body224는 그대로다. **FreeUndo record는 `[2,8,0,1]` at0..4**, reserved4..8=0, PageRef8..32, object3 at32, object generation1 at40, pageLSN at48, INIT LSN at56, full page SHA256 at64..96, reserved96..352=0이다. pageLSN=INIT LSN>0이며 canonical slot0/Free1 한 개를 복원하여 전체 digest를 대조한다. 다른 generation·extra cell·Live/Releasing·비정규 padding은 지원하지 않는다.

record·allocator·certificate의 identity/kind/LSN을 전수 bijection으로 검산한다. status_mask는 실제 Status만 세며 별도 Undo mask는 만들지 않는다. Undo를 ATT/Job census에서 제외하기 전에 exact FreeUndo 검산을 수행한다. BaseView.image와 check_init_provenance의 기존 호출 계약을 유지하고 caller page scratch로 canonical image를 재구성한다. 원 complete INIT 또는 이미 accepted된 base만 INIT claim의 provenance를 충족한다. 스스로 다시 seal한 certificate/digest는 그 권한이 아니다.

v1의 strict `validate_plan4_base_allocation`은 유지하고 v2는 `validate_plan4_system_base_allocation(bytes, profile, raw_descriptor, high_water, terminal)`을 소비한다. 두 경로는 기존 algebra를 공유하며 선택은 명시 format에서 온다. source-value 상계 함수도 별도로 공급한다. 일반 occupied Undo/nonempty Catalog의 재구성으로 이 Free recipe를 확대하지 않는다.

## 4. 원 pool·flush·재적재·물리 복구

INIT는 같은 allocator/payload2pin과 original claim/registration generation, exact complete frame, dirty/pageLSN/first recLSN을 결합한다. WAL durability 확인 뒤 claim을 완료한다. flush는 기존 immutable capture→WAL-before-page→DWB→home sync→retirement→조건부 RAM finalize 순서를 유지한다. 실제 clean eviction과 두 번의 physical home read 및 canonical history/provenance 대조를 통해 원 directory에 재적재한다. resident hit만으로 재적재 인수를 만족시키지 않는다.

현재 recovery target row48에는 role가 없어 모든 동적 주소를 Status로 추정한다. 내부 row를56byte로 만들어 validated role discriminant8을 추가하는 안을 선택한다. 실제 canonical request가 note_target에 role를 기록하고 target_at와 DWB Pending 검산은 같은 accepted mapping을 소비한다. arbitrary home bytes에서 role·기대 identity를 생성하지 않는다.64row 증가512byte는 기존 metadata section의 공간과 실제 F05로 검산하며 새 heap/큰 backing을 추가하지 않는다.

current-build recovery는 원 replay64page와 기존 descriptor bank를 사용한다. analysis의 의미 채택과 mixed-home REDO를 구별하고, selected base의 typed membership·INIT provenance를 보존한다. 실제 old WAL 제거와 dirsync 이후 새 epoch의 Status와 Undo INIT를 durable로 만든 뒤 두 번째 장애에서 복구해야 한다. Reserved는 원 scope로 cancel하여 종료하며 INIT 뒤 이미 성공한 페이지를 새 reserve로 재발급하지 않는다.

## 5. 자원·독립 입력·검증 동결

원 operation114688=frame65536+3page, 원2pin·closed slot1·caller replay64page·descriptor36·FD 구성을 유지한다. 이것만으로 F05 PASS라 하지 않는다. 새 role/scope/result, population/history, base decoder와 recovery caller의 동시 생존량을 실제 source-value 식에 합산한다. compiler/static admission을 먼저 확인한 뒤 관련 runtime을 실행한다. component65536·metadata196608·whole262144 및 원 backing 한도를 증액하지 않는다.

독립 fixture는 Python stdlib의 명시 field recipe·SHA256으로 생산하며 제품 encoder/decoder·materializer·제품 출력에서 expected를 얻지 않는다. 신규 경로의 필수 입력은 scope/208byte 부족/role·identity·birth·claimant/omitted INIT/Free generation·extra cell/base provenance 위조, actual mixed Status+Undo owner의 flush/eviction/load, Reserved 복구 취소와 INIT/DWB/REBASE 핵심 실패, 같은 ELF의3PID·old WAL 제거 후 재장애다. exact ID·cut occurrence·native LSN/work/page·시간/출력 예산은 해당 leaf 구현 전에 input-plan과 source에 고정한다. 아직 미정인 수치/미실행은 PASS로 세지 않는다.

새 role/token compiler 수명 검증, 관련 engine 정적 검사와 신규/변경 경로만 선택한다. 무변경 Status/plan3·전체 workspace·기존 완료 하위 suite를 반복하지 않는다. 이 단위의 확정 inventory는 S5 전체 DEV3 인수에도 포함하고 확대 조합은 S5에 둔다. 문서 검증은 push 전용이다. raw failure·source archive·ELF·receipt를 보존하고 완료된 담당 전용 cache만 정리한다.

## 6. 병렬 소유와 완료 경계

`page_lifetime`은 allocation/INIT helper·population·dirty registration/init·load/history/live recovery·flush 파일을 맡는다. `/root`는 plan4_recovery_base·plan4_recovery 하위·recovered_plan4와 공유 module/index/docs를 맡는다. `undo_design`은 신규 tests/fixtures/dev3-plan4-free-undo와 free_undo_literal_tests.rs의 독립 expected helper만 맡는다. runtime/native 파일은 통합 담당이 별도 배정한다. history의 prove_capture를 여러 담당이 동시에 수정하지 않는다.

독립 D2의 exact 형식 확인 후 구현하고, 새 source별 증거와 최종 D1 뒤 commit한다. 이 단위 완료는 실제 Free slot의 저장·guard·복구 공급이다. 첫 CREATE→INSERT→COMMIT→재시작 거래, live Undo/CLR·terminal/publication·전체 DEV3/S5는 후속이다.


## 7. 구현 전 고정한 native 일정과 실행 한도

최종 native 일정은 **L6→L10→L14**다. 논의 중 L13→L17 안은 미채택·미실행으로 보존한다. 별도 memory/literal의 empty lifecycle·cancel/reuse 이력 L1..13은 이 native 일정과 다른 입력이며 native 기대값에 섞지 않는다.

| PID | 원 producer와 고정 기대값 | 중단/완료 경계 |
| --- | --- | --- |
|1|work1/page9/inc2 Status Reserve1/INIT2; work2/page10/inc2 Undo Reserve3/INIT4; page10 실제 flush→clean eviction→physical reload 완료; work3/page11/inc2 Undo Reserve5/INIT6|page11의 Pending DWB 선택·directory sync 완료 뒤 첫 home write 직전 SIGKILL|
|2|same-build recovery, capture6의 v2 base 선택과 old WAL 실제 unlink/dirsync; work4/page12/inc2 Status Reserve7/INIT8; work5/page13/inc2 Undo Reserve9/INIT10|INIT10의 원 durable 확인·RAM 설치/claim 완료 뒤 새 data home을 flush하지 않고 SIGKILL|
|3|old WAL 없는 base6+WAL2 suffix7..10 복구; work6/page14/inc2 Status Reserve11/INIT12; work7/page15/inc2 Undo Reserve13/INIT14; 새 Undo 실제 flush/load|현재 원 owner의 계속 동작·exact census/bytes/identity·자원 반환을 확인하고 정상 종료|

Active/Ready/Reserved가 없으므로 PID2/3의 loser cleanup은 logical group을 추가하지 않는다. epoch/REBASE/control 선택도 logical WAL LSN을 소비하지 않는다. 제어 파일 변경을 새 semantic LSN으로 세지 않는다. 실제 구현이 이 정본과 다른 의미를 요구하면 실행 결과에 숫자를 맞추지 않고 D2 입력을 먼저 수정한다.

PID1 cut의 이름은 **`A_FU_BEFORE_UNDO_HOME`**, 해당 phase에서 arm한 뒤 occurrence1이다. 시험 전용 native File adapter가 `d3-data-0000000100000001`, offset`11*16384=180224`, length16384의 첫 write 직전에 일치하는 page11/inc2/object3/gen1/LSN6 header와 canonical digest를 검사한다. trace의 Pending selector publish·성공한 directory sync가 선행했는지 controller가 확인한다. 이 hook은 IO를 수행하거나 결과 bytes를 바꾸지 않는다. trace/도달 marker를 flush하고 mutex를 반환한 뒤 최대5초 대기하며, 외부 controller가 SIGKILL하여 실제 process crash를 만든다. 미도달/timeout/선행 조건 불충족은 FAIL이고 정상 worker exit로 바꾸지 않는다. 원 G5 native adapter에 optional one-shot 시험 hook만 더하며 기존 소비자는 unarmed다.

PID2 cut은 **`A_FU_AFTER_UNDO_DURABLE`**, initialize10 성공을 받은 뒤 occurrence1이다. 실제 prefix/완료 requirement·원 등록과 expected dirty state를 대조한 뒤 marker를 내보내고 같은 외부 SIGKILL을 기다린다. old WAL unlink 및 성공한 directory sync는 native trace·파일 목록으로 별도 확인한다. 두 killed worker는 handles0 정상 cleanup을 주장하지 않으며 PID3 정상 종료만 반환을 검산한다.

실행 상한은 static15초(worker10/감사5), 관련 memory210초(worker180/감사30, 개별30초), native3PID 전체30초(worker 합29초/최종 감사1초), 출력8MiB, trace phase별4MiB, live DB32MiB, native 보존 snapshot128MiB, File3/DB FD6이다. source export·ELF·cache·fixture·raw·실패 보존을 합한 해당 실행 산출물은1GiB 이내로 관리한다. timeout/미실행/실패를 보존하고 예산 초과를 성공으로 처리하지 않는다. 신규 compiler suite는 baseline1+6 semantic probe로 고정한다: 정상 사용, reservation double-consume, 살아 있는 reservation 동안 Owner 재대여, role의 session 밖 탈출, plan3 UndoRole의 plan4 사용, initialize 뒤 같은 reservation의 cancel 재사용. 각 negative의 정확 오류 code/span은 작성된 probe에서 실행 전에 고정한다.

## 8. 독립 설계 채택 기록

2026-10-25 `/root/dev3_review`의 최종 D2 PASS를 통합 담당 `/root`가 채택했다. 검토한 정본의 SHA256은 `391ff1ecf07765bcf0ad78658a9b50302d093e9021744e2e331096ac9c247f63`, allocation 공급 r3은 `f6b60bc89349d2dcf386d9647663ec9d17f1e0154f9040d2ccedb25e1447d57d`, 독립 입력 r3은 `7505f9755cc6ee3e202d0b825d3ec9bee0bea7d41ed26d33734b1f2f3b51438e`다. 형식·실제 권한·예약·provenance·native 일정·자원 한도·F07 분담을 채택한 기록이며 제품 실행과 최종 D1은 아직 NOT_RUN이다.

## 9. 제한 구현 인수

후속 [source별 실행 증거](../evidence/dev3-plan4-free-undo-20261025.md)는 정적2·메모리7·컴파일러7과 같은 ELF의 native3PID 일정을 완료했다. r5의 L6 Pending 장애→old WAL 실제 제거→L10 재장애→L14 추가 발급/flush/load를 독립 D1이 원 snapshot·trace로 확인했고 `/root`가 채택했다. 원 D2 당시의 NOT_RUN과 이후 실패·검토 보류는 각 원 source에 보존한다. 최종 S5·Live Undo·첫 CREATE/INSERT/Commit·nonempty 복구는 이 인수에 포함하지 않는다.

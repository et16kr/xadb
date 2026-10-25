# Plan4 recovered owner 입력 준비

## 2026-10-25 중단 작업 재개 후보

사용자의 중단 작업 한정 완료 요청으로 원48개 구현 후보를 연결하고, 실제 consumed-prefix hash 수정에 대응하는 E09와 독립 검토자가 요청한 sealed digest/LSN 원인 구분 E10을 더해 **memory45 + static2 + native3 =50개**의 source를 작성했다. 아래 원 중단 기록은 당시 identity와 FAIL·HOLD·NOT_RUN을 보존한다. 현재 [입력 색인](input-plan.json)은 독립 검토 후 동결할 후보이며 제품 실행 결과를 주장하지 않는다.

L6/B4는 원 pool·loan·credit의 인계를 확인하며 전체65,536B ATT/Aging/allocation bank를 독립 literal과 비교한다. 첫 XRB1은 원 base fixture의 `reserved/active/rollback/ready/finish`5개 SHA를 고정하고, source hash·DWB 실제 필요 repair 수·원 namespace bytes에 대한 독립 field식으로만 갱신한다. 다음 XRB1은 원 finish literal에 epoch2의 독립 두 번째 거래·Root/Job/Status·Aging·inventory cut을 반영한다. 실제 출력 base를 새 정답으로 돌려 쓰지 않는다. XWE2/XWS2/CURRENT 전체 bytes도 이 기대 base와 원 입력 source header로 새로 구성해 비교한다. 원 base5와 raw-index는 색인의 `immutable_fixture_bindings`와 `immutable_raw_dependencies`에 등록한다. raw generator를 실행하거나 새 raw 파일을 생성하지 않았다.

S15는 prelease·선택·완결 suffix·늦은 selected input 교체를 검산한다. P6에는 P01의 다섯 명시 하위 입력(이전 home repair, 같은 home, 더 최신의 정상 home, 같은 home sync BeforeError/AfterError)이 들어간다. 뒤 두 오류는 Pending을 보존해야 하며, no-write 성공도 home sync 뒤에만 Empty를 선택해야 한다. E8 원 후보와 E09는 오류 phase·cause·effect 및 실제 adapter hit를 구분한다. E05는17B partial write, E07은 independently canonical home의 repair0 경로에서 directory sync13번째를 사용한다. E08은 두 complete hash 사이의 교체, E09는 앞 frame을 이미 소비하고 뒤 frame을 아직 읽는 중의 교체다.

V4는 cleanup 뒤 canonical home readback·정확 DWB 선택·원 flush continuation·status incarnation membership을 검산한다. V04의 구체 입력은 미동결 후보의 다른 namespace owner 대신 실제 page9/incarnation2에 plain PageRef9/incarnation3을 요청하는 것으로 좁혔다. Membership/Unchanged와 원 bank·files·idle/fence, 올바른 요청의 후속 성공을 직접 검산한다. 새 authority를 만들지 않는다. native3은 같은 generic producer/두 번째 거래/재진입 driver를 실제 동일 ELF의 서로 다른 세 PID에서 사용한다.

memory 사례의 전체 byte 비교와 adapter event 검사는 해당 process 안에서 수행한다. 보존 출력은 case marker·context·검산 실패 위치 및 runner log이며 memory 파일 전체를 영구 dump했다고 주장하지 않는다. 파일별 길이·SHA·inode·원 byte snapshot과 전체 native trace의 보존은 별도 native controller가 담당한다.

이번 단위의 최종 S5 항목은 동일50개 ID의 명시 alias로 등록한다. 과거 제안한 profile2/3·crash 전조합·55status 확대는 이번 중단된 작업 범위 밖의 미동결 제안으로 유지한다. 이를 새 구현·실행이나 DEV3/S5 완료로 세지 않는다. 실제 selected 실행·원 실패·source/ELF admission은 총괄의 재개 증거에 따르며, 편집 중 문서 검증이나 완료 시험 반복을 수행하지 않는다.

## 원 중단 시점 기록

2026-10-25 사용자 요청으로 정리 후 중지했다. 아래 내용은 작성 중이던 후보의 기록이며 진행 중인 작업 지시가 아니다. 최신 단계 분리·oracle 보완은 compile/runtime NOT_RUN, 원 static2 FAIL·F05 HOLD다. S01–15와 observation helper는 미연결 draft이며 전체 인수로 세지 않는다. [중지 기록](../../../devdoc/evidence/dev3-recovery-paused-20261025.md)을 따른다.

이 디렉터리는 effect-owning G6 recovery와 recovered live 인계의 새 입력 단위다. 현재 [입력 후보](input-plan.json)는 mandatory memory 43 + static 2 + native 3 phase, 총 48개 / 9개 고유 selector를 제안한다. 원 43개 후보 뒤 실제 pre-effect seal 변경에 대응하는 늦은 CURRENT/base/DWB/FILEMAP 내용 변조와 같은 bytes의 named identity 교체 5개를 별도 S11–15로 추가했다. static 두 함수와 lifecycle6/binding4의 새 source가 준비됐으며 나머지 semantic/native 본문과 fault ordinal은 작성·도달성 검토 중이다. D2 동결·fixture 생성·제품 실행의 승인을 뜻하지 않는다. S5 후보의 정확 목록과 추가 분모도 아직 동결하지 않았다.

입력 source는 [새 recovery tests](../../../crates/engine/src/storage/wal_io/plan4_recovery_tests.rs)다. static은 actual memory `plan4_test_support::Writer/File`과 actual native `plan4_pages::native::Writer/File`의 source census를 읽는다. 원 native adapter 타입을 실제 worker에서도 그대로 사용하며, 부모가 새 test-only `open_native`/`native_receipt` factory를 공급했다. 새 wrapper의 모양을 가정한 대체 타입을 측정하지 않는다. owner/runtime/closed facade/live owner/phase/Result, codec deep/other와 component 숫자를 모두 출력한 뒤 상한을 검사한다. 65,536B source-value core, 196,608B metadata, 262,144B whole 한도이며 compiler stack 측정이나 live F05 수용의 대체가 아니다.

첫 semantic source checkpoint는 [source-r0](../../../artifacts/dev3/plan4-g6-empty-retention-20261024/recovery-owner-test-source-r0.json)에 새 세 child의 전체 SHA, 원 literal helper SHA, index와 실제 34개 raw/reference의 경로·길이·SHA를 기록했다. `tests::cases::recovery_lifecycle`과 `recovery_binding`만 10개 source 준비 상태이며 전체 후보를 구현 완료했다고 세지 않는다. epoch2의 cleanup은 원 frame의 stream epoch/header/full SHA만 독립적으로 재결합한다. 같은 pool의 다음 거래는 epoch2/local1, Status/Job generation3, birth8에 대한 명시 field식으로 Root/Job/Status/descriptor/native/redo/frame을 구성한다. 원 first-cycle template를 입력으로 사용하며 새 raw를 디스크에 생성하지 않는다. 이 새 literal 계산은 독립 source 검토 전 후보이다.

독립 oracle은 [원 N=0 literal](../dev3-plan4-empty-retention/provenance.md), [XRC1/XWE2/XWS2 literal](../dev3-plan4-recovery-control/provenance.md), [XRB1 literal](../dev3-plan4-recovery-base/provenance.md)의 고정 field 식을 참조한다. 원 raw/index/manifest는 읽기 전용 SHA로 결합한다. 원 generator를 다시 실행하거나 완료한 테스트 함수를 호출하지 않는다. 새 expected bytes를 product encoder/decoder 출력으로 생산하지 않는다. 제품 API를 이용한 fixture setup은 실제 writer·reserve·INIT·semantic stage를 만들기 위한 동작이며 expected bytes의 출처가 아니다. 현재 새 raw는 생성하지 않았다.

주 사슬은 실제 fresh profile1의 reserve1/INIT2/AgingStart3/Begin4에서 producer를 닫고, 새 PID의 recovery가 selected source를 분석하여 canonical base를 선택·보존한 뒤 새 epoch2를 여는 것이다. 기존 epoch1 actor의 정확한 TxnRef·StatusSlotRef·credit을 복원하고 실제 RollbackStart5/AbortEnd6/Finish7을 수행한다. 같은 pool과 원 loans를 유지한 live owner에서 epoch2/local1의 Begin8/종료9..11을 수행한다. 세 번째 PID는 old WAL1 없이 선택된 base와 WAL2를 읽고 epoch3을 연다. 매 recovery가 REBASE→LOG_OPEN하므로 마지막 phase도 base/CURRENT/WRITER/epoch 파일을 실제로 쓴다. 마지막 oracle는 중복 **physical home repair 0**이며 전체 write 0이라고 주장하지 않는다.

native 3 phase는 서로 다른 PID와 동일한 불변 source·ELF·input을 사용한다. 각 phase의 전체 stable pathname, 길이, SHA와 inode, 원 stdout/stderr, entropy trace, 실제 effects, exact BEGIN/PASS/FAIL marker를 보존한다. partial suffix는 raw 원본에 보존하지만 채택된 prefix와 후속 canonical model에서 제외한다. 완료 producer는 oracle 또는 후속 phase 실패로 다시 실행하지 않는다. spawn 전 INVOKING을 기록하고 output truncation·uncertain spawn·product PASS 뒤 oracle FAIL은 automatic resume 대신 audit HOLD로 구분한다. 재검산과 제품 재실행은 별도다.

모든 사례는 source별 원 FAIL/중단·NOT_RUN을 보존한다. case의 `freeze_blockers`가 비어 있지 않으면 입력 정본으로 동결할 수 없다. source의 실제 fault operation/name/offset/occurrence와 expected cause/effect, cleanup의 소유권을 먼저 대조한다. 불완전 source 후보의 기대 오류를 완성된 oracle로 주장하지 않는다. 원 Live30, pure140, control/base 단위의 결과는 그 원 source 증거로 유지한다.

임시 예산은 static 10+audit5초, memory 180+audit30초와 함수당30초, native 전체30초 안 마지막 audit1초 reserve다. native history 전체128MiB와 log8MiB, live DB32MiB 후보를 적용하고 이전 실패·후속 continuation을 합산한다. 원 snapshot 복제 수를 늘려 한도를 재설정하지 않는다. 전체 preparation/source/ELF/overlay/cache/원 실패기록 1GiB 및 records32MiB는 부모의 실제 공간 검산 뒤 D2에서 확정한다.

첫 선택 실행의 원 L01은 기대 XWE2 namespace가 profile8..24를 읽어 offset16에서 FAIL했다. 독립 pinned profile의 실제 namespace16..32로 기대식만 수정했다. 원 source/ELF/실패는 보존하고 이미 PASS한 B4·V4·S15·P6·E9의38개를 반복하지 않는다. E10은 동일한 nonempty Active4 입력의 독립 Genesis/WalSealed CURRENT에서 digest만 틀린 경우와 LSN만 틀린 경우를 구분하며, 두 경우 모두 첫 mutation 전에 각각 ContentMismatch·OrderMismatch를 요구한다. 이후 source별 결과는 총괄 재개 증거를 따른다.

semantic-r3의 compile-only Clippy에서 발견한 두 test style 항목만 후속 source에서 정리했다. S의 인접한 두 `if` 사이에 줄바꿈을 넣고, V03의 Drop 구현이 없는 flusher view에 대한 명시 `drop`을 같은 위치에서 끝나는 lexical block으로 바꾸었다. 조건·검산·I/O 호출과 lock 해제 순서는 같으며 제품 source는 바뀌지 않았다. 원 r3 source·ELF·실행 증거를 보존하고, 이 style 수정만으로 완료 S/V 사례를 반복하지 않는다. 후속 manifest의 source SHA와 lint 검증은 원 실행 identity와 별도로 기록한다.

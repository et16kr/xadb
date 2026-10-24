# DEV3 plan4 정리 큐·빈 Catalog의 최초 설치

2026-10-24 · D3-PLAN4-GENESIS-001 · **D2 설계 수용, 필수304 선택 검증 완료, 최종 D1 제한 수용**.
이 단위의 종료점은 원 FreshStorageSession에서 실제 페이지를 설치하고 원 DWB를
연결한 뒤, 별도 process의 genesis-only 선택과 세 번째 process 재진입까지다.
[첫 행 거래](dev3-task-11-commit.md#first-row-integration)의 선행이며 live 큐,
Catalog CREATE/CLR, 일반 recovery/READY 또는 전체 DEV3 완료를 뜻하지 않는다.

## 결정과 소유

사용자의 DEV3 계속 개발·단계별 commit·병렬 개발 위임에 따라 `/root`가 통합하고,
별도 구현·시험 준비·독립 D2/D1 담당이 검토한다. 작업 입력 묶음과 다음 실제 소비자를
함께 공급하는 [개발 방식](../plans/dev3-ai-throughput-improvement.md)을 적용한다.
선택한 대안은 같은 FreshStorageSession과 기존 DWB/maintenance backing을 재사용하는
plan4다. 별도 임시 저장 엔진이나 미사용 live 역할을 추가하지 않는다.

독립 D2 원본은 `artifacts/dev3/plan4-genesis-independent-d2-20261024.md`, SHA256
`2f101e56f5a174655a57671b3221bec5a6b7fecc1bb19c932a71c15d0c71c1d9`다.
구현 입력은 `plan4-genesis-implementation-input-20261024.md`, SHA256
`51399228fdd7d47b524e8896ab04c51fa8c7b8b3e19fb3763dfbfb0d1e408442`다.
이는 설계 채택 증거이며 제품 실행 결과가 아니다. 이전 queue-only plan4/high-water4
후보를 아래 plan4/high-water5로 대체하되 과거 source·실행 증거는 바꾸지 않는다.

## 초기 배치

plan version은4, 발급 ObjectId high-water는5다. object1은 allocator,
Status2/Undo3은 payload 없는 예약 identity, object4는 정리 큐, object5는 Catalog다.
file `0x0000000100000001`의 1MiB 첫 extent를 사용한다.

| page | kind·owner | 초기 내용 |
| --- | --- | --- |
| 0 | Allocation10·object1 | Allocated9/Free55, allocated mask0x1ff |
| 1 | RetentionRoot12·object4 | Unstarted, AgingRef 없음, revision1, Free64 |
| 2–6 | RetentionJobs13·object4 | 15+15+15+15+4개의 Free gen1 job |
| 7 | TreeAnchor11·object5 | Catalog anchor, root page8, revision1 |
| 8 | CatalogLeaf5·object5 | XBT1 metadata 한 cell, entry0 |
| 9–63 | 미할당 | 전체 zero bytes; 유효 page envelope 아님 |

모든 할당 page의 generation/incarnation은1, level/LSN은0이다. kind12/13은
level0만 허용한다. 기존 tree·Status·Undo·mixed dispatch가 새 종류를 자동 수용하지 않는다.
queue mask0x7e, Catalog mask0x180은 초기 배치 관측이며 mutable 권한이 아니다.

XQJ1은1024bytes이며 magic/version1/generation1 외에는0이다. absent budget640도
모두0이다. XQR1은512bytes이며 queue identity와5개의 job-page reference를 결합한다.
job digest는 ordinal 순64개 full cell의 SHA256, aggregate digest는 canonical640bytes의
SHA256이다. **aggregate의 offset0/128/256/384/512에는 tag1/2/3/4/5**가 있어
zero640의 digest와 다르다. Running/Leased/Ready·Free gen2는 이번 codec에서 거절한다.

XAL1·XTA1·XBT1은 기존 typed codec을 사용한다. XBS1은 기존4096byte format1에서
plan4/high-water5/allocator full-page digest를 결합하고 전체 canonical bytes를 비교한다.
ControlKind7·base·Aging descriptor·현재 WAL frame을 이번 형식에 추가하지 않는다.

## 설치·검증과 소유권

`create_retention_catalog`는 caller maintenance12MiB에서9 prepared page147456bytes와
3 scratch page49152bytes, 합계196608bytes를 잠시 빌린다. 반환 owner의 lifetime은
원 WAL slot에만 결합된다. PreparedPlan4는9 images를 immutable borrow하고,
prepare가 발급한 controls digest를 첫 descriptor create 전에 다시 대조한다.

순서는 기존 GenesisWalSession 생성, canonical 전체 준비, XBS1 설치,
data9page+zero55page 쓰기와 한 번 sync, FILEMAP bank·selector publish,
actual readback이다. 페이지별 byte를 효과 도중 새 codec으로 생성하지 않는다.
같은 writer의 원 doublewrite 설치를 이어 사용한다. 기존 plan1/2/3의 설치·역할·복구
허용 집합은 그대로 유지한다.

data 검증은 selected profile/FILEMAP/chunk와 정확 길이·opened/named identity를
결합한다. 같은 handle의64page canonical pass를 두 번 수행하고 마지막 named stamp를
확인한다. profile/FILEMAP/XBS1 등 authoritative controls도 data pass 후 다시 확인한다.
별도 selector는14개 DB 파일의 metadata와9개 authoritative control 범위를 시작/끝에
비교한다. unselected DWB bank·raw slot은 **geometry만** 검증하며 독립 native oracle의
whole-file 비교를 제품 자체 보장으로 옮기지 않는다.

`Plan4GenesisSelection<W>`는 private `RecoverySelection<W>`만 보유한다.
새 acquire는 maintenance를 빌려 검산한 뒤 writer exclusion과 owner에 묶인 관측값만
반환한다. writer extraction, into_inner, mutable owner·ClosedRecovery·READY 전환,
raw capability constructor는 없다. old acquire의 plan 허용 범위를 넓히지 않는다.
WAL4096/LSN0, Empty DWB batch0, canonical XDI stage만 수용한다. CURRENT/base/rebase,
Pending, WAL frame, non-Free job, nonempty Catalog, 부분 설치는 거절하고 수리하지 않는다.

## 오류·자원·인수 경계

사전 입력/용량 실패는 Unchanged다. Genesis 실패는 원 WalIoError의 효과·원인을
보존한다. Genesis 성공 뒤 오류는 CallerMustResolve이며 원 fence를 세우고 waiter를
깨운다. prepare부터 readback까지 원 unwind guard로 보호한다. 실패 파일을 자동으로
삭제·재생성하지 않으며 Drop에 sync/cleanup을 숨기지 않는다.

selector는 DB data를 쓰지 않는다. lease 전 오류와 acquire 실패를 구분하고, acquire
실패의 원 NamespaceError.effect를 보존한다. lease 성공값에는 LOCK 생성 여부가 없으므로
후속 오류는 database_effect=Unchanged, lease_effect=AcquiredMayHaveChanged,
누적 effect=CallerMustResolve로 표시한다. LOCK 효과를 DB repair로 설명하지 않는다.

source local cap65536, owner/File cap16384, 기존 nested cap16384를 유지한다.
새 Prepared/Seed/error/Result/observation·실제 native/memory adapter와 공통 helper 추출의
동시 생존 값을 계산한다. 수치 검산은 실제 frozen source를 compile한 뒤 static 사례부터
선택하며, 통과 전 producer/readback fault/native 효과 시험을 실행하지 않는다.
기존 Empty conversion 등 가까운 상한의 변경 여부는 source/type로 대조하고 완료된 동작
시험을 반복하지 않는다.

실제 tests-r2의 ST01/ST02는 각 memory adapter와 native worker가 사용하는 Writer/File 타입에
제품 source census를 결합하여 PASS했다. native constructor peak24852·selector32532는
local65536 이내이며, producer prepared loan196608은 caller maintenance의9images+3scratch다.
BootstrapPlan/BootstrapError·DescriptorSpec/SystemProgress/UndoRecord의 공용 타입 크기는
기존 값과 같음을 확인했다. 이는 source-value와 실제 수명의 회계이며 machine stack·RSS 또는
전체 engine F05 측정으로 확대하지 않는다. source별 결과·원 FAIL·NOT_RUN은
[선택 증거](../evidence/dev3-plan4-genesis-20261024.md)에 보존한다. 같은 r5 source/ELF의 strict3PID 인수는 PASS이며 최종 독립 D1을 이 제한 단위에 수용했다.

안정 파일은 LOCK 포함15개, 총22069248bytes다. Genesis의 WRITER와 FILEMAP·DWB의
세 selector stage 경로까지 전체 생애 중 경로명은18개다. 앞선 준비의17개 계산은
`d3-writer-stage`를 빠뜨린 것으로 실행 전 정정하며 동시 안정 파일 수는 바뀌지 않는다.
bootstrap source bound5296128와
DWB reservation16777216의 합계22073344는 안정 파일 길이 합계와 별개다.
queue/Catalog131072bytes는 기존1MiB extent 안에 있으며 별도 owned charge가 아니다.

정확 case ID·selector·fixture/oracle·fault cut·source/ELF·시간/보존 상한은 실행 전에
독립 검토로 동결한다. mandatory와 S5 확대를 각각 선택하며 kind12/13으로 의미가 바뀐
기존 subcase만 분리한다. native PRODUCE/SELECT/REENTRY는 서로 다른 PID의 같은 ELF를
목표로 하고, 완료 단계 재실행이나 실패·부분·NOT_RUN의 PASS 전환은 금지한다.
실행 뒤 source가 바뀌면 관련 diff·실패/미실행 ID만 검증한다.

후속 G5는 원 MutableStorageOwner·SharedPool의 실제 membership/load/flush다.
live 큐·atomic Begin과 Catalog CREATE/CLR·첫 행 거래·capture/base·일반 recovery,
AI 비교 소비자2/3·3/3 및 DEV3 최종 S5는 이 단위로 완료되지 않는다.

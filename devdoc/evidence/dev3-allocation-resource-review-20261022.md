# DEV3 예약·취소의 중첩 자원 검토

2026-10-22 · D3-ALLOC-RESOURCE-002 · D2 수용 / 제한 자원 검증 완료

[소유 계약](../contracts/dev3-allocation-reservation.md#component-budget) · [실제 구현](../implementation/dev3-implementation.md) · [운영 방침](../policies/development-policy.md)

총괄 `/root`가 source-owned 동시 생존량의 aggregate guard와 실제 stack 시험을 구현하고, `/root/allocation_review`가 별도 읽기 검토를 맡았다. `/root/allocation_native`는 native harness와 외부 stack 관측기를, `/root/allocation_compiler`는 수명 probe를 구현했다. 기존 user의 DEV3 재개·완료와 병렬 진행·단위별 commit 지시를 따른다. 기존64KiB 예산을 높이지 않으며 전체 DEV3 F05 완료를 주장하지 않는다.

## 선택과 범위

이전 owner/File/Plan 각각의 `size_of` 검사는 중첩 decoder·SHA·reader 부가 상태를 모두 세지 않았다. 개별 상한만 유지하는 대안 대신 constructor/operation의 합을 실제 타입 크기로 검사한다. 동적 heap 측정이나 process RSS를 source value 상한의 대체물로 사용하지 않는다. 임의 trait 구현의 내부 stack/heap을 generic owner 크기로 제한할 수 없으므로 XADB 소유 값과 concrete port 관측을 구분한다.

[계산 소스](../../crates/engine/src/storage/wal_io/allocation_budget.rs)의 값은 moved non-Copy owner를 한 번, 동시에 남는 Copy 인자·반환값을 보수적으로 센다. compiler spill·물리적 move 복사본은 별도 실제 stack 관측의 범위다. caller의4개 큰 buffer와 메모리 시험 포트의 가상 파일 bytes는 component가 소유한 중복 allocation이 아니다.

## Operation 계산

E=`sizeof(AllocationEntry)`, D=`sizeof(DescriptorSpec)`, B=`sizeof(RemainingBudget[5])`, P=`sizeof(Plan)`이다. entries64 scope가 끝난 뒤 descriptor와 최종 검산이 시작되므로 서로 다른 phase를 합산하지 않고 최댓값을 사용한다.

| 단계·동시 생존 chain | 상한식 |
| --- | --- |
| encode_prepared.entries→cell/entry/claimant codec |72E+A+H|
| encode_prepared.spec→encode_descriptor→owner/control codec |D+1536+A+H|
| cancel prepare→validate.desc→validate_transition→validate_live.current→descriptor 검산 |5D+2(B+2×sizeof(usize))+A+H|
| Plan 값 전달 |4P≤4096; change→prepare→encode_prepared/header 또는 validate→header를 포함|

A는 AllocationPage4·PageHeader4·PageIdentity4·WalFrameHeader4·WalPrefix3·Profile4·Batch4·OwnerState4·ControlRef4·RedoRecordView2·SHA digest8·scalar32·slice/iterator control24·작은 codec bytes512·추가 expectation/owner/namespace/error/iterator carriers2048을 센다. 각 실제 `size_of` 합≤7168을 admission에서 검사한다. 추가2048은 모든 작은 helper의 임시 carrier를 완전한 compiler-stack 표로 오인하지 않도록 둔 보수 여유다.

H=`4×sizeof(ContentHasher)+1536≤2048`은 retained hash state·digest clone 및 고정 SHA256 block/output/compression 임시 저장을 포함한다. `sha2 0.10.9`의 x86/soft 구현과 현재 Cargo.lock의 검토에 한정하며 dependency·target·call chain 변경 시 재검토한다. static round constant는 호출별 소유량이 아니다.

source-bound T는 세 단계의 최댓값이며≤16384를 요구한다. 마지막으로 owner+4P+T≤65536을 확인한다. 입력 N은 반복 횟수만 바꾸고 local 저장을 늘리지 않으며 이 chain에는 재귀가 없다.

## Constructor 계산

constructor 임시값 C는 Profile12·SelectedControl8·FileMapRoot4·FileMapCursor2·FileMapChunk3·ChunkDescriptor4·FileMapEntry4·ControlRef8·MapSummary4·PageView4·AllocationPage2·PageHeader6·PageIdentity4·FileName4·AllocationEntry8·WalPrefix2 및 작은 control/reference/guard/error 공간2048과 H다. 실제 타입 합≤16384를 요구한다.

PageFileReader 전체 R과 추가 data File F를 C 밖에서 센다. profile/selector handle은 별도 scope이고 최종 data read 시 retained container와 data handle이 함께 존재한다. 기존 WAL handle은 owner O 안에 있다. 따라서 `O+R+F+C≤65536`이고 source FD peak3은 기존 보수 예산5 안이다. constructor root/scratch는 operation buffer의 앞32KiB를 빌리며 reader 종료 후 같은 buffer가64KiB frame+16KiB after page 역할로 바뀐다.

## 관측과 원래 실패

최초 resource-r0의 layout 시험은 PASS였으나 stack 시험은 `VMA≤65536` assertion에서 FAIL했다. 제품 constructor를 호출하기 전의 측정 가정 오류이며 이 원본을 PASS로 고치지 않는다. 새 std-only 진단 child에서 debugger가 실제 pthread stack65536·guard4096을 읽었지만 marker가 든 VMA는77824였다. 관측된 `MADV_GUARD_INSTALL`과 인접 영역의 병합 때문에 VMA는 stack보다 컸다. 진단은 제품 기능 시험이나 native13DB 재실행이 아니다.

resource-r2의 변경 layout/aggregate guard 시험은 PASS다. memory port concrete owner2504·File32·reader384·Plan296·entries64 4096·descriptor912·remaining5 560·ContentHasher112바이트를 관측했다. A6864·H1984·entries phase13456·encoder phase11296·descriptor phase14560·C10564·4P1184이며 constructor 합13484·operation 합18248바이트다. padded File12000의 타입은 수용하고 owner/File16384 padding의 타입은 거절한다. engine package의 모든 target/feature Clippy도 PASS했으며 기능 시험을 실행한 것으로 세지 않는다.

외부 observer로 실제 stack을 확인한 뒤에도 제품의 debug frame이 중첩되어 SIGSEGV가 발생했다. observer-r0는 원래 구현의 SHA 경로, resource-r3는 첫 reserve의 header→descriptor decode→SHA에서 하한−2648bytes, resource-r4는 cancel의 descriptor 자체 검산→빈 inventory hash에서 하한−1232bytes, resource-r5는 cancel의 validate_reserved→descriptor decode에서 하한−392bytes였다. 이 실패와 별도 r3 진단 실행을 모두 FAIL로 보존한다. resource-r1은 source 사본만 있으며 실행하지 않았다.

수정은 preflight를 읽기 전용 plan_change로 분리하고 producer encode_prepared 반환 뒤에 검산하며, header 검사 반환 뒤 descriptor를 해독하도록 호출 경계를 나눈 것이다. live 상태의 reserved/finished 검산도 각각 별도 함수로 분리했다. Plan·Profile·Batch·descriptor는 helper에서 borrow하여 불필요한 Copy를 늘리지 않았다. _unwind guard와 Snapshot→stage→apply→sync→observe 순서, 오류 태그·우선순위, WAL/page bytes는 유지했다. 시험 fixture의 큰 installed owner는 thread 밖에서 Box로 준비하여 std closure 복사 비용을 줄였지만 제품 owner와 작업은 원래 값을 소비한다.

resource-r6의 source `0f503243faa7cf120088bf2f1384040e325e55601491d6b2898e411d109379a6`, ELF `a5bfa0b18bcd690e265765edb15d6186ae725a5177ba3af09204a7c0639996d5`에서 정확1개 bounded-stack 시험이 PASS했다. 실제 pthread stack65536·guard4096과 marker를 대조했고 constructor→reserve63→reserved_page62→cancel63→observe/drop 및 handle/writer 반환을 확인했다. VMA69632는 진단값이다. 변경한 producer/검산의 AR02/AR04는 r3, 후속 header/descriptor 경계의 AR04는 r5, preflight 추출의 AR03은 r6에서 각각 선택 검증했다. r6의 engine target Clippy와 관련 Rust fmt도 PASS다. 기존 native13/compiler/WAL4·수정 없는 runtime 시험은 반복하지 않았다.

[단위 인수 기록](dev3-allocation-reservation-acceptance-20261022.md)은 각 source·명령·해시·실패·검토를 연결한다. 원래 실패·unused source capture와 실제 실행을 구별한다. 제한 경로의 자원 검증은 INIT/home/DWB 운영/일반 recovery/READY/거래 ACK나 DEV3 S5 인수를 대신하지 않는다.

## 재검토 조건

동시 생존 타입/개수·scope·재귀·새 caller buffer, generic port, compiler/toolchain/target, SHA dependency, constructor 및 semantic 검산 chain을 바꾸면 영향을 받는 계산·관측을 다시 검토한다. 무관한 DEV0/1/2와 이미 완료한 runtime8·native13/compiler probe를 반복하는 사유는 아니다. 원래 source와 실행 범위별 증거를 보존한다.

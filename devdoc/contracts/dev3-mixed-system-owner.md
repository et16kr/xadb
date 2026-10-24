# DEV3 혼합 Status·Free Undo owner 계약

2026-10-24 · D3-MIXED-SYSTEM-OWNER-001. [혼합 INIT 준비](dev3-status-init.md#mixed-system-init)를
원 mutable storage·SharedPool·WAL·DWB와 연결하는 제한 소비자다. 독립 D2와 실행 전 source/F05
검토를 적용한다. 실제 검증 상태는 [선택 증거](../evidence/dev3-mixed-system-owner-20261024.md)가
소유하며 이 계약의 채택을 실행 PASS로 보지 않는다.

## 종료점과 권한

원 3-frame SharedPool은 allocator 한 장과 payload 두 장이다. 같은 owner가 Status A와
Free Undo B를 INIT하고, A의 WAL 선행 flush·retirement 뒤 같은 slot에서 Undo C를 INIT한다.
B도 flush한 뒤 A와 B를 정확한 class로 재적재하고 원 owner로 계속 읽는다. allocator와 C는
RAM/WAL이 home보다 앞선 채 producer를 끝낸다. 새 process가 동일 build로 복구하고 세 번째
process의 재진입은 redo home 쓰기0을 확인한다. 이 실제 연결이 이 단위의 소비자 종료점이다.

<a id="native-continuation"></a>
### 실패한 native 시험의 변경 코드 한정 이어가기

첫 r6 실행은 producer PASS 뒤 복구의 terminal hold 기대값 오류로 실패했다. 완료 batch 주소의
누적 진단 mask는2|4|8=14이며 active/remaining0과 별개다. 원래 동일 ELF 세 phase PASS 조건은
미충족으로 보존한다. 완료 producer를 다시 실행하지 않는 사용자 지시에 따라 독립 D2는 아래
조건의 기능적 current-product 복구 증거를 별도로 허용한다. 이 보완은 원 FAIL의 소급 PASS가 아니다.

원 producer의 전체 snapshot·source/ELF·phase·stdout/stderr와 실패 후 DB를 보존하고,
검증된 producer snapshot만 새 inode의 DB로 복사한다. 원본→복제본의 전체 bytes/hash와 inode
대응을 기록한다. engine/foundation/platform 제품·실제 native adapter 타입·의존성·도구체인·
빌드 설정은 원 source와 같아야 하며 승인된 시험 기대값/identity bridge 차이만 별도 기록한다.
원 execution.tsv를 고치거나 새 build의 producer PASS를 만들지 않는다. 수정된 recover와 reentry는
서로 다른 PID, 동일한 새 source/ELF와 복제 DB로 실행하며 원 producer PID도 달라야 한다.

이어가기 검산기와 frozen 입력을 효과 전에 독립 검토한다. 원 실패 사용시간을 포함한30초와
원본·후속 immutable evidence 합128MiB 상한을 유지하고, 최종 snapshot을 참조하여 불필요한
중복 사본을 만들지 않는다. source별 실제 결과와 미충족 동일 ELF 조건은 선택 증거에 구분한다.

plan2 StatusRole과 plan3 UndoRole은 발급 경로를 유지한다. private non-Copy FixedSystemRole은
실제 역할을 소비하여 만들며 역변환·inner 역할 escape·raw constructor를 공개하지 않는다.
기존 Status 진입점은 plan2만, 새 bind_mixed_system과 acquire_mixed는 plan3만 받는다.
MixedSystem은 기존 bound owner를 private로 보유하며 Deref나 기존 actor 반환을 제공하지 않는다.
원 fence·CAS·WAL writer·operation scratch·pool·flush control을 재사용한다.

공개된 typed target은 Status Object2/gen1/kind9 또는 Undo Object3/gen1/kind8/level0이다.
첫 extent의 incarnation≥2와 allocator의 전체63entry를 검산한다. Undo는 slot1/gen1의
canonical Free root이며 occupied root·chunk·UndoSequence·target-bearing live undo를 승인하지 않는다.
class는 reserve부터 claim·stage/install·flush/load·retirement까지 유지한다. 기존 ResidentStatus의
허용 집합을 넓히지 않고 별도 ResidentMixed가 정확한 두 종류를 받는다.

## 실제 수명과 실패

reserve/INIT는 기존 선형 reservation과 fixed operation을 통해 원 WAL durable 요구와 dirty 의무를
소비한다. 준비 결과는 private FixedPrepared이며 기존 Prepared로 꺼낼 수 없다. 전체 allocator
관측·멤버 검사·borrowed request 준비가 끝난 뒤 codec에 들어가며 그 임시값의 수명을 겹치지 않는다.

prepare_next는 실제 allocator snapshot과 full census 뒤 빈/clean slot 또는 eligible dirty 후보를
선택한다. INIT를 위한 빈 slot 선택에 가짜 page identity를 쓰지 않는다. dirty 후보는 원 dedicated
flusher의 별도 flush가 필요하며 후보 관측만으로 cursor·page·generation을 바꾸지 않는다.
reader는 typed target을 두-pin으로 검사하고 immutable guard만 반환한다. guard·reservation이
살아 있는 동안 원 owner 재대여는 compiler 경계에서 거절한다.

flush는 캡처한 class·origin·LSN과 실제 Free image를 검증한 뒤 원 WAL 선행→DWB→home→retirement
순서를 따른다. flush view를 버려도 retained capture/의무가 사라지지 않는다. 이미 retirement가
선택된 continuation은 home을 다시 쓰지 않는다. first cause·dirty 의무·부분 I/O를 오류 원본과 함께
보존하고, 불확실한 효과는 fence한다. health 검사는 foreign owner 등 후속 인자 검사보다 앞선다.

## 정확한 mixed load

load는 quiescent 원 owner에서 allocator membership과 class를 먼저 확인한다. 원 home의 전체
canonical Free image를 읽고 원 WAL handle/scratch에서 genesis부터 system-only prefix 전체를
검증한다. reserve/cancel 및 Status/Undo INIT의 canonical descriptor·Native·redo 연결을 재구성해
선택 target의 유일한 INIT LSN/digest를 봉인한다. 마지막 allocator/descriptor/prefix와 전체 멤버
census를 원 owner 상태와 대조한 뒤 home을 다시 읽고 동일 image임을 검산한다.

그 결과만 sealed load proof가 되어 원 native install 경로로 전달된다. checksum이 맞는 낮은 LSN,
다른 Free generation, 다른 Undo identity, occupied/chunk envelope도 원 INIT가 아니면 거절한다.
일반 Unchanged I/O 오류만 재시도 가능하다. 필수 home/WAL의 EOF·invalid count·불완전 prefix 또는
완전하지만 지원하지 않는 의미는 fatal fence이며 slot bytes·registration·generation을 설치하지 않는다.

## 닫힌 현재-build 복구

기존 [recovery effects](dev3-recovery-effects.md)의 별도 MixedEffects scope를 사용한다. 정확한
plan3 genesis descriptor와 두 payload class를 인증하고 Status38000/Undo38208 frame을 구별한다.
typed canonical target은 전체 semantic/materializer 검산 뒤에만 생기며 frame 길이만으로 승인하지 않는다.
전체 allocator membership·target mask를 양방향으로 대조하고 사용하지 않는 target table 영역도 검산한다.

home·Pending DWB·redo·readback에 같은 class가 전달된다. torn home은 인증한 canonical source로
복구할 수 있지만 SHA-valid home은 선택한 INIT의 LSN/digest와 정확히 일치해야 한다. replay topup은
실제로 쓰는 home 수가 아니라 모든 canonical target을 센다. 이 종료점의 allocator+A+B+C는65536이며
복구 때 쓰는 allocator+C의 effect mask는9, 재진입은0이다. live undo/CLR·transaction·일반 READY를
이 scope로 허용하지 않는다.

## F05와 source-value 수명

논리적인 source-value 계산이다. ABI stack/RSS 또는 adapter heap의 측정값이 아니다. binding 전부터
actual generic writer/file·반환 carrier와 기존 shared gate를 검사한다. caller operation114688와 page backing,
flush arena4194304, recovery maintenance12582912는 각 기존 계약의 별도 backing이다. whole 식에서
dirty에 더하는65536은 전용 flusher의 source-value 몫이며 caller operation 크기가 아니다. 상한을 올리지 않는다.

| r4 frozen source의 실제 경로 | memory | native | 한도 |
| --- | ---: | ---: | ---: |
| mixed binding |60488 |60488 |65536 |
| activation |63592 |63592 |65536 |
| INIT |33120 |33144 |65536 |
| load |61320 |61504 |65536 |
| pressure |62392 |62576 |65536 |
| history nested |30576 |30640 |상위 load에 포함 |
| retained / reader |62032 /3588 |62032 /3588 |dirty 식에 포함 |
| dirty hot / whole |192312 /257848 |192312 /257848 |196608 /262144 |
| mixed recovery peak |64578 |64690 |65536 |

fixed prepare의 borrowed Plan/Expected 수명과 Result mapping 수명을 분리하여 plans4096을 지킨다.
INIT caller696≤768, reader 공통 gate3648, shared class/control delta8과 pool delta832를 실제 식에
반영한다. 변경이 기존 빈 거래 자원 계산에 미치는 영향은 별도 static 선택으로 검사하며 기존 runtime을
반복하지 않는다. r4의 해당 native conversion65424/load64840/pressure65152는 각각65536 이하다.

## 검증 범위와 남은 의무

중간 선택은 새 owner memory75, mixed recovery memory63, 추가 load6과 source-value3이다.
compiler semantic9+baseline1, compiler checker6, native producer/recover/reentry3 및 새 실행 도구의
known-bad 검산은 별도 분모다. 실패 함수 안의 완료 ID도 반복 대상에서 제외한다. source·ELF·fixture·
원 stdout/stderr와 FAIL·중단·미실행을 보존한다. 실행 도구 변경 후 기존 제품 PASS를 새 실행으로 세지 않는다.

긴 부하·큰 profile·전체 조합은 [DEV3 S5 계획](../plans/dev3-test-plan.md#final-suite)에 둔다.
이 단위는 general pool의 모든 page 종류·extent, live undo/commit, 첫 행 거래·index/catalog,
일반 recovery/READY와 DEV3/S5 완료를 대신하지 않는다. 새 class·wire/권한·오류 효과·자원 한도·
시험 inventory 의미를 넓힐 때 독립 D2를 다시 적용한다.

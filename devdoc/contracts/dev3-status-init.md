# DEV3 폐쇄 두-frame Status INIT 소유 계약

2026-10-22 · **D3-STATUS-INIT-001 · 제한 D2 및 선택 인수·독립 D1 수용**

[불변 역할](dev3-status-bootstrap.md) · [할당](dev3-task-03-allocation.md) · [dirty stage](dev3-dirty-stage.md) · [buffer/flush](dev3-task-05-buffer-flush.md) · [시험 계획](../plans/dev3-test-plan.md)

3.03.2.a는 실제 durable reserve의 한 payload를 Status 역할로 INIT하고 allocator와 Status RAM 두 image를 같은 WAL 그룹으로 공개한다. immutable bootstrap 역할, 현재 allocator membership, 성공한 stage의 requirement는 각각 별도 권한이다. 이 단위는 Status home flush·일반 recovery·거래 발급·READY를 공급하지 않는다. 다음 [3.03.2.b Status home flush](dev3-status-home-flush.md)의 별도 소유 계약과 전체 DEV3 완료 조건은 유지한다.

## 권한과 생애

plan2 `MutableStorageOwner::bind_status_bootstrap`는 caller의 두 DirtyFrame과114688-byte operation 공간을 대여한다. health→plan2/origin→actual generic 자원→strict empty frame→binding CAS 순서다. 초기 allocator만 읽고 폐쇄 payload bytes는 건드리지 않는다. 이미 published되었거나 metadata가 남은 frame wrapper를 새 binding에 재사용하지 않는다. 기존4MiB flush 공간은 storage에 그대로 있고 .a에서 별도 FlushControl을 만들지 않는다.

`BoundStatusBootstrap::split`은 단일 mutable writer와 shared StatusReader를 대여한다. `reserve_one(&mut Owner)`은 실제 reserve가 durable하고 checked final allocator snapshot이 보존된 뒤에만 private `ReservedStatus`를 반환한다. 그 snapshot에서 정확한 PageRef를 얻어 추가 pin 시도 때문에 wrapper 없는 Unchanged가 반환되는 경로를 만들지 않는다. wrapper는 writer와 owner의 mutable borrow를 함께 보유하며 clone·raw constructor가 없다.

`initialize(self)`/`cancel(self)`의 건강한 before-effect 실패는 같은 wrapper를 돌려준다. 원 batch/page/ordinal과 origin을 보존한다. cleanup 실패, staged 효과, poison/fence는 Fatal/CallerMustResolve이며 wrapper를 돌려주지 않는다. 활성 wrapper의 Drop은 fence하며 취소 I/O를 암묵 수행하지 않는다. 이미 INIT된 역할은 재예약·재INIT·free/reassign하지 않는다.

private InitAuthority는 core/binding/fence와 exact work/birth/ordinal/allocator/payload를 묶는다. 원 reservation과 다른 stale 키는 Unchanged이며, 현재 권한의 allocator snapshot이나 claimant가 손상된 경우 원 semantic 원인을 보존하고 fence한다. raw LSN·역할 getter·raw page decode만으로 native install 권한을 만들 수 없다.

## 원자적 RAM 설치와 내구성

ordinary resolver는 최초 allocator 등록과 OnceLock에 마지막으로 공개된 Status 등록만 본다. closed slot은 ordinary pin/capture에 MissingTarget이며 StatusReader에는 NotInitialized다. initializer만 전체 두 target pin을 한 번에 얻고 own-pin flag를 갖는다. 두 content/version guard와 전체 census 검산 뒤 before snapshot·두 after image·WAL frame을 준비한다.

| 경계 | 완료된 상태와 실패 시 보존 |
| --- | --- |
| stage 전 | whole preflight와 DPT 검산; bytes 변경 없음. checked cleanup 뒤에만 동일 wrapper 반환 가능 |
| C1 | 실제 stage의 WalRequirement와 non-Clone claim key를 retained InitObligation에 즉시 저장; terminal_pending 설정. 중간 fallible 동작 없음 |
| C2 | allocator after bytes 설치 |
| C3 | Status after bytes 설치 |
| C4 | 두 metadata·descriptor 설치 |
| C5 | 두 DPT entry와 requirement 설치 |
| C6 | Status registration OnceLock을 마지막에 공개 |

FencedInitRegistry는 lock의 Ok/poisoned 결과를 즉시 소유한다. Prepared wrapper 생성 전의 두 unwind 경계에서도 첫 guard 해제 전에 fence한다. whole precheck 뒤 C2–C6에는 codec·할당·I/O·새 lock·fallible 검산이 없다. poststage 실패/Drop은 O·claim·부분 image·원래 first cause를 보존한다. content→pin lock 경로를 만들지 않으며 release는 content, version, own pins 순서다.

checked release 뒤 같은 stage requirement만 wait_durable에 전달한다. 성공 후 새로운 두-pin read로 page envelope·전체 allocator의 ObjectId2 membership·Status canonical Free cell·두 metadata LSN을 검증한다. reader scope 종료 뒤 Finished descriptor를 검증하고 exact claim을 반환한다. 다른 정당한 reader pin은 남아 있어도 되지만 initializer own-pin flag는 false여야 한다. 마지막으로 O/batch/terminal_pending을 지운다. WAL 실패는 설치된 RAM·claim·O와 원 lower I/O 오류를 남기고 fence한다.

StatusReader는 published 전 DPT/content를 잠그지 않는다. published 뒤 두 target을 함께 pin/read하고 full63 membership census로 malformed generation/kind의 중복 role도 거절한다. request 임시값은 codec 전에 scope가 끝나며 getter는 borrowed identity를 준다. 반환 guard는 immutable Status bytes와 stamp만 제공한다. occupied cell·TxnId 발급·undo 상태 읽기는 후속 계약이다.

## 내부 wire와 budget

Status INIT는 독립 literal과 검산하는38000-byte frame이다. header160, semantic header192, Finished descriptor1536, core256, allocator Native12544, Status Native352, Consume208, allocator Patch6272, Status INIT16448, trailer32 순서다. INIT/SystemEnd10이며 Status before image는 없다. 새 payload는 ObjectId2/gen1/kind9/level0, 최초 page1/incarnation2, Free home224-byte가 offset16128에 있고 slot-generation1/incoming0이다.

private trusted `StatusInit114688` budget만 terminal WAL38000/frame2/protected70768/memory114688을 준다. reserve21200과 INIT38000의 실제 상한은59200이다. cancel 이력은42400을 소비하고 남은 예약을 terminal finish 뒤 해제한다. 기존 예약 budget은 unchanged이며 caller가 raw 수치로 terminal 권한을 올릴 수 없다. 원 descriptor·Native·redo·Consume의 일치와 checksum-valid semantic 손상을 함께 검산한다.

개발 빌드는 fresh fixture를 사용한다. 이 wire가 일반 replay/reopen을 제공한다는 뜻은 아니다. .a의 native 실제 파일 검사는 새 INIT workflow의 WAL과 unchanged homes를 검증하며 current-build recovery 인수를 대신하지 않는다.

## 자원과 거절

logical source-value 한도이며 ABI stack/RSS 또는 adapter 내부 heap 측정이 아니다. 실제 generic owner/file/reader padding과 typed return carrier를 effect 전 검사한다. retained pool delta P≤512, WriterControl delta B≤64, Status control delta C≤1536, initializer 전용 I8192, 각 reader J3584를 구별한다. 동시 caller는 총34이며 initializer I를 reader마다 곱하지 않는다.

| 항목 | 한도·식 |
| --- | --- |
| producer | plans≤4096, temporaries≤16384, Status codec≤8192. allocator·Status codec는 순차 scope |
| upper/native join | caller≤768, permit/value-result+commit≤768. claim6272, preflight7824, install7440, release/final7840 각각≤I8192 |
| dirty hot |60672+P+C+34×J+I≤196608 |
| dirty cold |60672+P+C+90112+max(5P+2048,3P+8C+2048)≤196608 |
| bound |실제 Bound 및 Result≤28672; owner≤32768, file≤16384 |
| readback deep |owner+PageFileReader+File+기존 constructor+32≤65536 |
| prepare deep |owner+control+plans+temporaries+I+2048≤65536 |
| stage deep |owner+control+plans+I+4096+2048≤65536 |
| install/final deep |owner+control+I+max(J,기존 temporaries)≤65536 |
| read/wait deep |owner+control+J 및 owner+control+4096+2048 각각≤65536 |

InitPlan은 prepare 반환 직후 scope가 끝난다. reader의 페이지 hash·allocator membership·Free cell decode도 겹치지 않는다. source census와 layout 전제는 실행 증거에 source identity와 함께 보존한다. shared pool/control delta는 기존 plan1 자원식에도 반영한다. 미래 Status flush는 actual StatusControl과 payload mapping/codec를 새로 검산하며 allocator-only flush의 옛 측정값을 재사용하지 않는다.

CF17-02/03은 각 generic 한도와 다른 deep 식이 맞더라도 readback 총합이 넘는 padded/align4096 adapter를 effect 전 거절한다. protected1GiB 독립 거절 경로는 고정 WAL cap 아래 도달 불가능하다. 최대289488896<1073741824의 산술과 실제 descriptor credit을 검증하고 그 branch는 NOT_APPLICABLE로 남긴다. 가짜 reachable failure로 집계하지 않는다.

## 선택 가능한 inventory와 후속 의무

| 군 | 중간 필수 | S5 추가 |
| --- | ---: | ---: |
| CF01–18 actual binding/authority/install/failure/reader/resource |61 |authority18, install schedules18, retry histories16 |
| WI01–24 wire |24 |profiles2/3의48 |
| WM01–08 materializer |8 |profiles2/3의16 |
| 합계 |93 |116 |

CF13/14는 .b로 이관되어 .a에서0이며 PASS가 아니다. CF12의 두 flush target 경쟁, CF13의 writer/flusher force 경쟁, CF14의 실제 clean-before-INIT, CF15의 published-and-cleaned 재사용 거절, Status native home 검사는 .b의 미완료 의무다. PF01–08과 두 flush 순서는 .a 분모에 없다. 초기504 physical schedule 제안은 보존하고, .b의 확정570 tuples 및 mandatory alias6/S5 추가588은 [별도 inventory](dev3-status-home-flush.md#inventory)가 소유한다. CF18의8개 known-bad witness는 synthetic checker이며 실제 CF08/12 schedule을 대신하지 않는다.

S5 retry18 중 실제 CF16과 같은 profile3/count1/terminal2를 alias로 빼고16을 추가한다. 초기 제안의 profile1 alias 표기는 실제 입력 tuple에 맞춰 정정한다. S5 전체209=93+116이며 bootstrap97은 별도 기존 분모다. compiler semantic13+infrastructure baseline1, native profile3, changed shared-resource leaf1 및 새 harness selftests는 별도 분모다. 확대·장시간 시험은 S5에서 실행하고 중간의 완료 사례는 코드 diff 없이 반복하지 않는다.

## 결정과 재검토

기준 HEAD38132353659935ab581f16521716e301e255a23e에서 `/root`가 상위 binding/authority/literals/통합, `/root/allocation_compiler`가 producer/compiler/runner, `/root/allocation_native`가 native pin/install/protocol을 맡는다. `/root/allocation_review`는 별도 D2/D1을 담당한다. 대안인 raw arbitrary target 등록과 현재 단계의 범용 batch 확장은 권한·자원 범위를 불필요하게 넓혀 채택하지 않는다. 하나의 closed payload와 immutable 역할로 첫 INIT의 실패 경계를 독립 검증한다.

원 제안·D2 자원/addendum·실행 전 source 검토·수정 반례·선택 실행은 각각 다른 identity로 보존한다. 채택과 실제 실행 PASS를 구분한다. 권한 확대, status mutation/transaction 발급, flush/recovery, 새로운 callback/lock, 자원 상한·scope 또는 inventory tuple 변경 시 재검토한다.

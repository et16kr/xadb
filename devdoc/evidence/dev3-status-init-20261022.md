# DEV3 폐쇄 두-frame Status INIT 선택 인수

2026-10-22 · **필수 선택 완료 · 최종 독립 D1 수용 · 전체 DEV3 미완료**

[소유 계약](../contracts/dev3-status-init.md) · [source별 실행 색인](dev3-status-init-20261022.json) · [구현 현황](../implementation/dev3-implementation.md#status-init)

실제 durable reserve의 exact 권한에서 allocator와 Status의 두 RAM image를 설치하고, 마지막 registration 공개와 같은 stage requirement의 durable 확인을 연결했다. immutable 두-pin reader는 전체 allocator membership과 canonical Free Status cell을 함께 검증한다. Status home flush·거래 발급·일반 recovery/READY는 이번 단위의 구현 범위가 아니다.

## 실제 변경과 선택

기준 commit은 `38132353659935ab581f16521716e301e255a23e`다. closed target 등록·initializer own-pin claim·whole precheck·C1–C6 설치·retained obligation·trusted INIT budget·WAL/after producer·reader와 effect 전 자원 거절이 새 코드다. 기존 shared pool/control 증분이 달라져 이를 검산하는 새 resource selector1도 선택했다. 기존 완료 bootstrap·reserve/cancel·allocator flush selector와 하위 전체 wrapper를 반복하지 않았다.

| 범위 | 실행·증거 |
| --- | --- |
| 새 runtime |필수93. 성공 그룹83, 원 libtest가 완료된 CF12의 파생 감사2, 실패 leaf 안의 완료 CF06 prefix7, 수정 CF06-08 exact1을 구분 |
| shared resource |변경된 공통 pool/control 자원식의 새 selector1 PASS |
| compiler |semantic IC00–12 unique13 PASS. 원 IC03 실패를 보존하며 수정/미실행10만 다음 선택. infrastructure baseline은 별도2회, 실제 compiler 호출16 |
| native |profile1/2/3 새 DB3 PASS. 실제 reserve→INIT→read, P3는 cancel/re-reserve 포함.16 WAL mutation effects, 총66,427,744 bytes와 전체 파일/inode·RAM/descriptor/WAL을 독립 대조 |
| checker |compiler4 + runtime/native4 unique8 PASS. 수정한 compiler1/runtime3 method만 추가 선택; unchanged4는 반복하지 않음 |
| 정적 검사 |최종 보존 source의 engine fmt 및 all-targets/all-features Clippy PASS. 제품 case 실행·문서 verifier는 포함하지 않음 |
| S5 확대 |authority18/install18/retry16/WI48/WM16 추가116 작성·NOT_RUN. .a의 runtime union209 |

원 runtime·compiler·native·빌드와 파생 감사의 source·ELF·입력·명령·stdout/stderr hash는 JSON에 각각 보존한다. 주 runtime source는 `c01a18ca…`, native/compiler 후속 source는 `c1f3365a…`, CF06-08 수정 실행은 `f6fc84cd…`다. 최종 lint/annotation 및 진단 oracle 정리는 `7e95ec68…`에서 정적 검사했다. 이 source 차이를 새 전체 실행으로 표현하지 않는다.

필수 runtime은 실패 원본을 포함해15.466826초/120초, compiler는23.823689초/60초다. native는 합계3.498079초이며 각30초/24MiB와 합계72MiB 안에 있다. setup/build 시간은 별도 기록한다. native wrapped File peak1/final0, DB FD 경계 관측3/final0이며 연속 physical peak 또는 성능 인수 주장이 아니다. 모든 disk home/DWB는 초기 bytes 그대로다. WAL이 실제 디스크에 쓰였다는 증거와 Status home flush/reopen의 미구현을 구별한다.

## 실패 원본과 좁은 수정

| 원본 | 원인·후속 처리 |
| --- | --- |
| compile r0/r1/r6 |대여/타입 오류8/9/2를 보존. 각각 수정 뒤 compile-only 빌드; 이 과정의 성공을 제품 case PASS로 세지 않음 |
| compiler r0 |IC03의 raw constructor E0451 외에 private Batch 타입 오류가 추가되어 검사 실패. 허용된 producer child에 fixture를 삽입하고 concrete Batch 인자를 받아 의도한 단일 오류만 검산. 제품 visibility 변경 없음 |
| runtime r0 |CF06-01의 정상 cleanup cancel이 남긴 WAL 효과를 “없음”으로 비교한 테스트 오류. pre-cancel 효과0와 실제 cancel의 독립 전체 이력을 분리 |
| runtime r1 |CF12 제품 두 case는 완료했으나 checker가 C1–C6 named variant를 기대. C6 Busy/checked release/실제 retry는 CF12-01, CF12-02는 C1–C5다. checker를 수정해 원 raw를 다시 판독; 제품 반복 없음 |
| runtime r3 |CF06-01..07 완료 후08의 snapshot 비교 실패. 의도적으로 주입한 publication이 pool 공통 진단 값인 점을 expected allocator에도 반영하고 나머지 bytes/metadata/원 오류를 계속 비교.08만 재선택 |
| quality r0 |동일 의미의 nested if를 let-chain으로 정리. exact reservation을 inline으로 반환하는 API2와 test helper2에만 result_large_err 이유를 명시. heap boxing 없이 실제184≤512의 기존 예산 유지 |

원 FAIL을 PASS로 덮어쓰지 않는다. 완료 prefix는 정확한 source의 각 assertion 이후 marker와 독립 감사에만 근거하며 중단된 뒤의 case를 완료로 보지 않는다. CF18의 valid1/known-bad8은 synthetic contract checker로, 실제 CF08/12의 thread/panic schedule을 대신하지 않는다.

## 독립 입력과 자원

858개 literal/24개 이력은 고정 genesis 입력과 명시 offset·stdlib struct/hashlib로 작성했다. 제품 encoder로 기대값을 만들지 않았다. WI24/WM8는 checksum-valid 의미 손상·틀린 before 상태·전체 두 after image를 검산한다. 입력 manifest의 NOT_RUN은 생성 당시 상태이며 후속 실행 결과는 별도 receipt가 소유한다. S5 retry alias는 실제 CF16과 같은 profile3/count1/terminal2이며 초기 제안의 profile1 표기를 바로잡았다.

실제 pool delta352/control delta352, producer plans2656/temporaries14560/Status codec7336, reader3480≤3584, upper caller576≤768, observation+Result608≤768, permit+Result+commit456≤768이다. dirty hot191424/cold157408은196608 이하다. source의 scope·carrier 수량과 실제 native layout15개를 함께 검산했다. allocator/Status codec, plan과 install, request 임시값과 reader hash가 겹치지 않으며 일반 reader34개와 exclusive initializer8192를 구별한다.

padded readback 합65860/72036은65536을 초과하지만 각 owner/file/bound와 다른 다섯 deep 식은 한도 내다. 실제 adapter 효과 전에 Capacity/Unchanged를 반환하고 scratch/binding/I/O를 보존한다. 실행 전 DWARF에서 어댑터 정렬 전제가 틀린 것을 확인한 기록도 남긴다. protected1GiB 독립 실패 branch는 고정 WAL cap에서 도달 불가능하므로 NOT_APPLICABLE이다. 실제 descriptor credit과 최대289488896의 산술을 별도 검증했다.

## 남은 인수

Status home flush의 first-zero·older/equal/torn home 정책, allocator와 Status의 두 flush 순서, writer/flusher force 경쟁, 실제 clean-before-INIT와 published-and-cleaned wrapper 거절은3.03.2.b로 남긴다. generic batch8/64·다중 file·tree/index/catalog·transaction/undo·recovery/READY 및 DEV3 S5 전수는 완료되지 않았다. 문서 verifier는 push gate 전용이므로 이 편집/commit 중 실행하지 않았다.

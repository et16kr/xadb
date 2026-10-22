# DEV3 불변 Status bootstrap 소유 계약

2026-10-22 · **D3-STATUS-BOOTSTRAP-001 · 제한 D2 및 선택 인수·독립 D1 수용**

[최초 저장 세션](dev3-storage-session.md) · [dirty stage](dev3-dirty-stage.md) · [할당](dev3-task-03-allocation.md) · [실행 증거](../evidence/dev3-status-bootstrap-20261022.md)

새 namespace에 immutable Status 역할을 영속 설치하고, 전체 설치와 readback에 성공한 세션에서만 borrowed `StatusRole`을 발급한다. 실제 Status 페이지 INIT, 현재 allocator membership, 거래 directory, 일반 recovery/READY는 후속 작업이다. 전체 DEV3 완료 조건을 축소하지 않는다.

## 영속 형식과 검산 순서

기존 `d3-bootstrap`4096은 selector stage 및 `d3-dwb-stage`와 독립된 불변 파일이다. XBS1 envelope version1/mode1/length4096을 유지한다. plan2는 u64 offset120의 ObjectId high-water=2와 offset280의 plan-version=2이며, checksum4064..4096을 제외한 나머지 bytes는 같은 profile의 plan1과 같다. reserved288..4064는0이다. Allocation 역할은 ObjectId1/gen1, Status 역할은 ObjectId2/gen1/kind9/level0이다. high-water만으로 역할을 추론하지 않으며 role2는 drop/reassign/reuse하지 않는다.

decoder는 length/magic/encoded length/checksum → envelope/mode/expected plan → reserved/constants/high-water → trusted namespace/profile/root/initial allocation SHA 순서로 검사한다. unknown plan 또는 known-but-unexpected plan은 `UnsupportedFormat`, 같은 expected plan의 잘못된 high-water는 `Corrupt`다. checksum 오류가 plan 오류보다 우선한다. 기존 plan1 오류 의미를 유지한다. raw decode나 fresh allocator SHA가 현재 membership 또는 설치완료 권한을 발급하지 않는다.

## API와 실패 효과

`FreshStorageSession::create_status(...)`는 기존 create와 같은 인자/반환형을 사용하고 private `BootstrapPlanKind::StatusBootstrap`을 선택한다. 기존 create는 `AllocatorOnly`다. raw plan-number 입력은 없다. 기존350 I/O 효과 및 반환 전 readback을 완료해야 성공한 plan2 세션을 반환한다. 실패·unwind는 성공 세션/role을 반환하지 않으며 원래 파일과 완료된 효과를 보존한다. residue를 fresh resume/overwrite하지 않는다.

`FreshStorageSession::status_role(&self)`의 `StatusRole<'_>`는 private `&BootstrapPlan`과 `&SessionFence`를 가진 non-Copy/non-Clone 값이다. constructor/upgrade/reset은 공개하지 않는다. namespace/object/generation/kind/level/high-water/fence getter는 불변 관측이며 payload 변경 권한이 아니다. owner 이동/Drop 후 role을 사용할 수 없다. 기존 role 관측이 이후 fence를 해제하지 않으며 후속 INIT는 health/origin을 다시 검산해야 한다.

| 경계 | 검산 순서와 오류 |
| --- | --- |
| Fresh role | poison/fence health → plan. 건강한 plan1은 Role/UnsupportedPlan/Unchanged. poison/fence는 원 Wal 원인과 CallerMustResolve 보존 |
| Mutable role | workspace를 포함한 기존 health → Fresh role. 원 Mutable Poisoned/Fenced를 보존하고 Fresh 오류만 `Cause::Bootstrap`으로 감싸 원 effect/cause 유지 |
| old bind_allocator | health → plan1 guard → 기존 CAS/등록. plan2는 UnsupportedPlan/Unchanged, binding/frame/workspace/scratch/I/O 변화0 |
| old consuming into_allocation_session | 기존 resource → health → plan1 guard. plan2는 Constructor/UnsupportedPlan/CallerMustResolve, 공통 fence 및 owner 소비. scratch/file/I/O 변화0 |

소비된 owner의 fence는 안전한 public API로 사후 관측할 수 없다. runtime은 typed 오류·owner 소비·scratch/files/I/O 불변을 확인하고 실제 fence 동작은 source D1 검토가 확인한다. 관측을 위해 unsafe나 제품 test hook을 추가하지 않는다.

## 자원과 effect 전 거절

새 durable 파일/bytes/FD/payload page는 추가하지 않는다. caller scratch49152와 component65536을 유지한다. 새 private `source_budget<W>()`의 실제 typed census 및 `source_budget_fits<W>()`가 Genesis 전에 다음 조건을 강제한다. 큰 generic은 기존 Fresh/File 한도를 먼저 검사하고 후속 합산에 진입한다.

| 한도 | 조건 |
| --- | --- |
| retained / 신규 값 | Fresh≤16384, File≤16384, kind≤8, role≤32, AllocationEntry[64]≤8192 |
| nested source values | prepare/readback 각각≤16384, Fresh role≤1024, Mutable role≤2048 |
| prepare 합계 | actual Fresh + actual entry array + names1032 + prepare≤65536 |
| stream 합계 | actual Fresh + actual File + names1032 + prepare≤65536 |
| readback 합계 | actual Fresh + actual File×2 + readback≤65536 |

prepare/readback census는 실제 중첩 Profile·namespace·FILEMAP/control·page/allocation·Result/error·hash 값을 보수적으로 더한다. reader control은 `size_of(PageFileReader<W>) - size_of(W::File)`로 **정렬 padding까지** 포함한다. 새 wrapper/return carrier672/160을 기존 PASS에 자동 포함하지 않는다. 전체 수량·source 수명 근거는 [실행 색인](../evidence/dev3-status-bootstrap-20261022.json)의 source-value ledger와 해당 제품 private 함수에 연결한다. 물리 stack/RSS 또는 adapter 자체 heap 보장은 아니다.

한도 초과는 Preflight/InvalidInput/Unchanged다. SB01-04는 정상 typed census와 align4096 File이 만드는 reader padding 초과를 분리하여 검사한다. 거절 경로의 모든 namespace/file 메서드는 호출되면 실패하며 caller slot/scratch sentinel도 그대로여야 한다.

## 필수 선택과 S5 inventory

| 선택 | 사례 수와 경계 |
| --- | --- |
| SB01 |4: profile1/2/3 독립 plan2 literal, 실제 자원 layout. resource case의 normal_admit/alignment_reject2는 named variant |
| SB02 |12: known mismatch2, unknown wire3, 잘못된 high-water4, checksum/reserved/plan 우선순위3 |
| SB03 |5: plan1, 정상 plan2, fence, poison 우선, Mutable forwarding. 마지막 case의 healthy/workspace_poison/plan1은 named variant3 |
| SB04 |7: invalid profile, descriptor 설치 뒤 create 실패, selector publish/sync after-error, readback의 다른 plan/read 오류, descriptor write unwind. 전체 effect prefix와 partial-file bytes/hash 검산 |
| SB05 |4: healthy/fenced bind 및 healthy/poisoned consuming adapter. 변경 없는 초기 fixture 설치는 새 설치 검증으로 집계하지 않음 |
| PARSER |기존 `bootstrap_descriptor_rejects_damage` 한 selector의 보존 corpus. 공유 decoder predicate가 실제 변경되어 선택 |
| native P1 |profile1 새 DB1,30초/8MiB. 실제 create_status350효과→role→Drop, 파일 전체 bytes·FD/handle 관측. INIT/복구 인수 아님 |
| compiler |semantic NC00 정상 borrow + NC01 private constructor 금지 + NC02 owner Drop 뒤 borrow 금지3. 별도 infrastructure baseline1을 포함한 실제 compiler 호출4 |

새 runtime 분모는 중간32 + S5 추가65 =97이다. 확대 matrix는 profile3 × expected2 × known wire2 × high-water5=60 및 profile3 × expected2 × unknown wire3=18, 총78이다. 중간 정상3와 decoder10을 alias로 제외하여 추가65를 별도 ignored selector에 둔다. compiler/native/기존 parser corpus는97과 별도다. 현재 확대65는 NOT_RUN이며 최종 DEV3 S5에서 실행한다.

## 결정과 재검토

기준 HEAD `09688384da7e4f2f34b9f3f2938c1f9c07a072b3`에서 `/root/allocation_review`의 독립 D2가 이 subset을 수용했다. 계약·제품 담당 `/root/allocation_compiler`, 독립 입력·통합·실행 `/root`, compiler/native 지원 `/root/allocation_native`다. 원 D2와 실행 전 source/input D1의 identity는 [증거](../evidence/dev3-status-bootstrap-20261022.json)에 보존한다. 넓은 INIT 제안의 known-plan IdentityMismatch 및 별도 API 이름은 이 subset에서 위 경계로 대체한다. INIT/closed frame/F05 전체 설계는 별도 검토 대상이다.

새 역할·plan·opcode, role 재사용, membership/recovery 권한, raw constructor, generic 자원식이나 오류 우선순위 변경 시 재검토한다. 실제 선택 인수와 최종 독립 판정은 [현재 구현](../implementation/dev3-implementation.md#status-bootstrap)을 따른다.

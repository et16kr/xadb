# DEV3 dirty frame·예약/취소의 stage 설치 계약

2026-10-22 · **D2 채택 / 제한 구현·검증 완료 / 실제 실행은 별도 증거 참조**

범위는 **3.05.1-A/B + 3.04.1.a**의 첫 allocator 소비자다. [buffer·flush 소유](dev3-task-05-buffer-flush.md), [WAL 변경 경로](dev3-task-04-wal.md), [동시성 admission](dev3-concurrency-admission.md), [단계별 시험](../plans/dev3-test-plan.md)을 따른다. allocator 한 페이지에 기존 reserve/cancel opcode8/10을 적용하면서 실제 pin/content 배제·first recLSN·DPT를 공급한다. INIT, 교체·flush·retirement, FIFO, tree/row lock, Commit, READY·일반 재시작 복구와 전체 DEV3 인수는 남는다. 두 target 지원은 이 단위에서 기계적 guard 공급이며 실제 두 페이지 변경 인수가 아니다.

## 채택 정체성

기존 초안과 검토는 수정하지 않고 `artifacts/dev3/resume-20261022/` 아래 보존했다. 이 문서는 채택한 결정을 구현 signature에 맞춰 정리한 정본이다. 채택 자체는 실행 결과가 아니다.

| 원본 | SHA256 |
| --- | --- |
| `dirty-stage-contract-draft.md` | `00c9422692d87543a4fd3e2a6d60a801466d948c0a34e5ac7196ed94de98200d` |
| `review/dirty-stage-d2-review-r1.md` | `0fe3d2f9a0f1c6fdfdd15a6b402197f1ebff3728ac58a94428e805f7ce4558b7` |
| `review/dirty-stage-d2-implementation-addendum-r0.md` | `31368dff80d649f1beeb1e1d71cdfa21114bfe3934814fec538523faaa348882` |
| `dirty-implementation-notes.md` | `93ab268a3a880ed18e3c3b2eb6aadcbb5178477c36f9799e9d7d7ab35239fc9c` |

추가 검토는 retained obligation과 local sealed permit 분리, 일반 nested temporary2048, wrong-core 오류 우선순위를 채택했다. 원초안의 obligation128/temporary1024 및 raw after·LSN installer 제안은 이 추가 검토로 대체한다. 최종 D1·actual layout·runtime/native/compiler 결과는 [source별 인수 기록](../evidence/dev3-dirty-stage-20261022.md)에서 판단한다.

## 소유·조립·진입

`DoublewriteStorageSession::into_mutable_storage`는 infallible move다. I/O·검산·fence reset 없이 기존 저장 owner를 옮긴다. 기존 reader borrow는 move를 막고 forgotten reader의 active flag도 유지된다. `MutableStorageOwner`의 원래 SessionFence를 외부 `PageConcurrency`가 빌린다. `bind_allocator(core, frames, operation)`이 반환하는 `BoundAllocation`은 storage/core/frame/payload/workspace 수명을 유지하며 `split(&mut self)`로 독점 writer와 공유 reader를 분리한다. 기존 clean reader와 새 dirty 서비스를 동시에 열지 않는다.

bind 순서는 health→같은 fence→cheap shape/type/resource/empty-wrapper 검산→Available→Claimed CAS→winner만 pristine WAL/reader-active/selected mapping readback→allocator 등록→Bound다. caller frame은1..64개, operation workspace는98304 bytes다. 이미 등록·poison·손상된 wrapper는 reset하지 않는다. claim 전 건강한 shape/resource/binding 실패는 Unchanged다. Claimed/Bound loser는 AlreadyBound/Unchanged이며 I/O·큰 constructor·frame mutation을 하지 않는다. 관측한 fence는 그보다 우선한다. claim 뒤 모든 실패/panic과 정상 Bound Drop은 공통 fence+Abandoned이며 재bind·healthy shutdown·남은 의무 제거를 하지 않는다.

각 DirtyFrame은 caller의 별도16384-byte payload를 빌린다. 생성자가 page payload를 값으로 복사/반환하지 않는다. 등록 generation은1이고 reset하지 않는다. 첫 binding에는 실제 선택된 allocator 하나만 등록하고 나머지는 Empty다. runtime load/eviction/임의 등록은 없다.

## 전체 pin과 content

pool은 exact core/fence, immutable directory64, leaf `Mutex<PinAccounting>`의 counts[64]u16+total, 고정 DPT64를 소유한다. 요청은1..2 target의 full identity/kind/level이며 권한 자체가 아니다. 요청 주소 중복·incarnation 충돌을 거절하고 정렬한 뒤 전체 pin을 한 임계구간에서 예약한다. per-frame≤34, total≤68, Empty count0, checked sum=total을 검산한다. 부족은 Capacity/Unchanged, 기존 회계 손상은 Invariant+fence다. allocator 한 target의 실제 동시 정상 pin 상한34와 기계적 두 target 상한68을 구별한다.

Owner 진입은 기존 health/accounting→exact core pointer→같은 core에만 Idle 확인 순서다. 건강한 foreign core의 forgotten Apply owner는 BindingMismatch/Unchanged이며 그 foreign mode를 변경하거나 fence하지 않는다. 이미 관측한 fatal health는 우선한다.

PinnedTargets는 Owner의 독점 borrow를 유지한다. consuming try_read/try_write로 actual content admission 또는 version gate와 정렬된 RwLock guard를 얻는다. non-Clone·non-Send/non-Sync이고 추가 frame 대기·pin 보유 중 I/O·공유→배타 승격은 없다. metadata mutex를 보유한 채 version/content를 얻지 않는다. 실제 write 순서는 version→정렬 content→DPT preflight acquire/release→append stage acquire/release→DPT install acquire/release다.

명시 cleanup은 content→version/admission→pins다. 모든 token identity/generation/count를 검산해 전체 pin을 한 번에 반환한다. 손상 시 임의 일부 감소를 하지 않는다. poison은 fence 뒤 recovered lock으로 검산 가능한 자기 회계만 반환하며 poison을 지우지 않는다. cleanup fatal은 기존 Unchanged를 승격하되 이미 발생한 첫 fatal cause를 바꾸지 않는다. unwind Drop은 content/version을 놓기 전에 fence한다. blocking metadata mutex는 짧은 bounded 배열/scalar 임계구간이며 OS wall-clock 진행 보장 주장은 없다.

## 실제 reserve/cancel 설치

prepare·checksum·전체 semantic 검산은 latch 밖 caller workspace에서 한다. snapshot의 identity/generation/pageLSN/counter/first recLSN을 실제 version/content 아래 재검산하고 DPT slot과 counter 여유를 stage 전에 확인한다. DPT는 binding lifetime의 frame index slot으로 미리 확보되며 stage 뒤 새 registry capacity를 구하지 않는다.

private producer의 성공한 stage만 두 권한을 만든다. `WriterControl`의 non-Clone/non-Copy `InstallObligation`은 원래 fence·identity·generation·complete location·before/after counter·first recLSN·operation·진행 phase를 보존한다. 별도 local `InstallPermit`은 exact external core·before FrameStamp·complete location·이미 검산된 immutable after-image borrow를 가진다. private fields와 constructor는 실제 소비자 내부에만 있다. 진단 Copy stamp/raw LSN/boolean/임의 after bytes는 권한을 발급하지 못한다. permit이 끝나도 실패한 obligation은 남는다.

| 순서 | 실제 설치와 cut 뒤 상태 |
| --- | --- |
| S0→S1 | complete WAL stage 직후 fallible call/hook 없이 batch·terminal flag·high-water와 retained obligation을 소유. `AfterStageOwned`에서 frame/descriptor/DPT는 before |
| S2 | permit의 full after page와 pageLSN 설치. `AfterPageInstalled`에서 counter/dirty/recLSN/descriptor/DPT는 before |
| S3 | metadata helper의 검산 성공 후 counter+1·Dirty·최초 recLSN 설치, 이어 infallible descriptor copy와 obligation phase 갱신. `AfterDescriptorInstalled`에서 DPT만 before |
| S4 | 같은 full identity/generation·최초 recLSN·새 counter의 DPT 설치 후 obligation phase 갱신. `AfterDptInstalled`에서 모두 after |
| sync | content→version/admission→pins 반환 후만 WAL write/sync. health·최종 observation까지 성공한 뒤 O를 제거. cancel은 같은 마지막 경계에서만 batch/terminal을 제거 |

S2/S3 부분 상태는 실제 exclusive content/version 안에 있다. 다른 정상 reader가 관측할 수 없으며 panic 후 fence 전에 latch를 반환하지 않는다. test-only 진단은 fenced/poisoned 상태를 읽지만 서비스 복구 권한은 없다. Drop은 부분 설치를 완성하거나 before bytes로 돌리지 않고 write/sync하지 않는다. reserve1의 첫 recLSN1은 cancel1의 pageLSN2/counter2에서도 보존한다. sync error에는 actual IoOwned slot 소유와 원래 하위 cause, batch/terminal/O를 보존한다. 아직 fatal 반환 전 I/O pause에서 reader가 완전히 설치된 dirty image를 읽는 것과 fence 뒤 모든 새 서비스 거절을 구별한다.

InstallPermit의 exact core/identity/generation/old·new counter/pageLSN 검산과 DptStamp의 pool anchor/index/prior entry 검산이 중복 적용을 막는다. mutation 값은 permit에서만 가져온다. S3 helper 실패 전에 descriptor를 먼저 바꾸지 않는다.

## 자원과 첫 효과 전 검사

<a id="registration-core-followup"></a>
2026-10-23 후속 [D3-REGISTRATION-001](dev3-task-05-buffer-flush.md#registration-core)이 이 절의 reference registration lookup을 Copy lookup·slot/generation pin으로, 일반 nested2048을3072로 대체한다. registration carrier6×128과 token 여유64의 보수 소계2688, shared hot165472/cold154944≤196608을 적용한다. deep의 generic 항만3072이며 move2048·constructor2048·hash와 전체64KiB/192KiB 한도는 유지한다. 아래 최초 수치와 원 evidence는 당시 source의 기록이며 현재 generic 예약으로 사용하지 않는다.

기존 Exclusive81920 resource context와 신규 Dirty98304 context를 private trusted enum으로 구별한다. reserve/cancel producer와 validator 모두 지정 context의 exact budget을 확인하고 serialized bytes에서 context를 추정하지 않는다. 기존 context bytes는 유지한다. 새 literal oracle는 Dirty98304 descriptor·hash·WAL을 독립 계산한다. 새 workspace는 frame65536+before16384+after16384=98304다.

metadata 상한은 기존2162688+guard65536+dirty196608=2424832≤8388608이며 남은5963776을 미구현 구조의 검증 완료로 세지 않는다. dirty retained60672+34×2048=130304, retained60672+cold constructor90112=150784로 모두192KiB 안이다. 일반 nested temporary2048은 helper마다 반복 가산하는 값이 아니라 가장 큰 전체 호출 chain의 동시 보유 source-value 상한이다.

| generic carrier | 보수적 상한 |
| --- | ---: |
| DirtyObservation/Result | 4×192=768 |
| FrameStamp/Result | 4×112=448 |
| DirtyEntry/Option/expected | 2×96=192 |
| borrowed Registration/Result/comparison | 6×32=192 |
| DPT lock/result | 2×32=64 |
| error/branch result | 4×32=128 |
| references/options/indices/scalars | 32×8=256 |
| 합계 | 2048 |

Directory/DPT/pin 배열은 retained에 한 번 포함하고 loop는 한 item scratch만 보유한다. registration lookup은 reference를 반환한다. retained O와 Option<O> 각각≤192, permit≤384, WriterControl≤4096, read/write guards≤256이다. bind 전 fits는 실제 타입 크기와 owner/control/Bound/Result, W::File/reader/constructor 합계, deep writer의 owner+control+PLANS+TEMPORARIES+generic2048+move2048≤65536을 확인한다. constructor cold phases는 move82048, 작은 frame wrapper2048, caller wrapper array32768, claimed directory/DPT/Bound90112다. 전체 source-value 수식은 실제 compiler physical stack/RSS 또는 임의 namespace trait의 내부 메모리 상한이 아니다.

## 변경 코드 시험과 최종 확대

중간 필수는 새 runtime DA01–DA07 7selector/53case, 독립 counterexample55, 새 compiler baseline+assembly release+16negative, Linux native3, 새 oracle/checker selftests다. 정상1→8→63·profile1/2/3의18operation 확대는 별도 ignored S5 selector로 선택한다. 추가 contention/cut/fault 확대의 작성·동결 상태를 inventory에 명시하고 NOT_RUN을 PASS로 바꾸지 않는다. 실제 실행 입력은 `tests/fixtures/dev3-dirty-allocation/` 및 compiler fixture의 manifest와 source hash로 고정한다.

기존 profile contextual Rust1/10case와 oracle4 실행 증거는 원래 source에 유지한다. 원 집계기의 marker parsing 오류 FAIL도 보존하며 독립 읽기 감사가 확인한 실제 성공 범위만 별도로 인정한다. 무변경 하위 시험·기존 guard/allocation wrapper·전체 workspace 시험은 선택하지 않는다. DEV3 최종 S5에서만 확정된 DEV3 전체 inventory를 실행한다. 최종 실행 뒤 수정은 관련 diff/실패 사례만 재검증한다.

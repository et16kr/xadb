# DEV3 allocator 한 페이지 flush 소유 계약

2026-10-22 · **D3-ALLOCATOR-FLUSH-ONE-001 scoped D2 수용 · 제한 선택 인수·독립 검토 완료**

[buffer/flush owner](dev3-task-05-buffer-flush.md) · [dirty stage](dev3-dirty-stage.md) · [WAL I/O](dev3-wal-io.md) · [DWB 표현](dev3-doublewrite-format.md) · [control/selector 표현](dev3-control-format.md)

이 단위는 fresh selected namespace의 실제 allocator 한 target을 capture→WAL durable→DWB issuance/image/Pending→home sync→whole retirement→조건부 RAM finalize로 연결한다. batch1을 반복할 수 있으나 batch8/64, 다중 file, ordinary recovery/reopen, INIT, free/reuse, scheduler/eviction/pressure를 제공하지 않는다. scoped 설계 수용과 실제 sizeof·source 수명·실행 인수는 별개이며 전체 task05/DEV3/S5 완료를 뜻하지 않는다.

## 소유와 조립

기존 `DoublewriteStorageSession`의 consuming adapter는 fresh owner·install plan·원래4MiB workspace reference를 분리한다. 설치 I/O나 XDI1 최초 설치 계약은 바꾸지 않는다. `MutableStorageOwner`의 workspace Option mutex는 bind winner가 한 번 take할 때만 사용한다. 반복 take/return이나 Drop 복구 lease는 없다.

bind는 shape/resource preflight→Available→Claimed CAS→BindingClaim→기존 allocator readback/등록→workspace take→pool/control 조립→최종 health→BOUND 순서다. CAS 뒤 오류/Drop은 Abandoned+공통 fence이며 재bind하지 않는다. workspace를 take한 뒤 실패해도 정상 usable 상태로 반환했다고 주장하지 않는다. 첫 claim·I/O 전 resource 거절은 Capacity/Unchanged다.

`BoundAllocation`은 writer control과 `FlushControl`을 별도 필드로 소유한다. 기존 `split(&mut self)`의 writer/reader는 유지하고 `split_with_flush(&mut self)`가 field-disjoint writer/reader/flusher를 대여한다. flusher는 exclusive control reference 하나를 받는다. 같은 bound의 두 mutable flusher, bound 이동·재split·storage/workspace alias는 Rust borrow로 금지한다. flusher view Drop은 진행 상태·capture·retired proof·workspace를 Bound에 남기며 I/O·성공 cleanup을 수행하지 않는다. Bound Drop은 fence하고 미완료 보호를 보존한다.

pool/capture의 stable binding identity는 외부 storage가 소유한 non-ZST `binding: AtomicU8`의 실제 borrow다. Bound 이동이나 raw 숫자로 identity를 대신하지 않는다. core는 정확한 `PageConcurrency` pointer로 결합하고 같은 fence를 가진 다른 core도 구분한다.

## 주소 claim과 일관 capture

fixed registry mutex는 기존 DPT entries64와 stage-issued requirement64, 단일 flush claim, local attempt highwater를 함께 소유한다. Dirty frame은 entry/requirement가 모두 Some이고 requirement의 pageLSN·origin이 해당 frame/core/storage와 일치한다. Clean은 둘 다 None이다. Clean의 pageLSN/counter는 초기0/0 또는 완료한 양수 pair이며 reset하지 않는다.

진입 검사는 health→exact core/Idle→DedicatedFlusher 역할→진행 상태→cheap resource와 counter headroom→claim→copy 순서다. 다른 core의 Idle부터 검사해 정상 BindingMismatch를 fatal로 바꾸지 않는다. batch+1, control generation+3, selector generation+3, local attempt+1을 각각 checked 검산한다. 부족하면 Clean 여부를 읽기 전 Capacity/Unchanged다.

claim은 full physical address/등록 identity/generation/attempt를 copy 전에 보호한다. 등록 index·generation, nonzero attempt와 last_attempt, capture의 full identity/counter/rec을 포함한 census가 모순이면 원 손상을 수선하지 않고 fence한다. 동일 incarnation의 정상 redirty는 허용하며 미래 free/reuse는 이 claim authority를 최종 mutation 경계에서 소비해야 한다.

capture는 actual content read→registry 순서에서 full image와 stamp/requirement를 coherent하게 복사한다. 이후 registry→content→admission→pin을 checked release한 뒤 checksum과 I/O를 한다. scalar `FrameStamp`는 진단값이며 private `CapturedPage`와 actual claim을 대신하지 않는다. 초기 Clean 또는 완료한 Clean이면 claim을 checked 반환하고 NoDirty다. 모든 namespace/file/WAL I/O0, batch/control/selector 미소모이며 local attempt는 소모할 수 있다. actual selector/FILEMAP/home readback은 Dirty copy 이후에만 수행한다.

pre-copy Busy/Capacity는 실행 중인 I/O/capture가 없음을 검산하고 checked claim 취소를 마친 경우에만 Unchanged다. 실패 cleanup이 처음 관측한 Poisoned/Invariant는 fatal로 상향한다. 이미 fatal인 원인을 이후 Fenced로 덮지 않는다. 손상 claim/accounting을 임의 감소·해제하지 않는다. unwind는 실제 content/version/admission 반환 전에 공통 fence를 세운다.

## exact WAL requirement

`WalRequirement<'s>`는 실제 complete stage만 발급하는 sealed origin+exact interval이다. raw LSN/location/prefix constructor가 없고 diagnostic location을 권한으로 사용하지 않는다. sealed Copy는 조회를 허용할 뿐 I/O 소유를 복제하지 않는다. actual stage 후 `InstallPermit`가 같은 requirement를 보유하며 S4의 content→registry 경계에서 DPT와 함께 설치한다. writer의 retained O가 제거돼도 Dirty registry에 남고 Clean assignment에서만 제거된다.

`ensure_durable(&requirement)`는 한 append lock 안에서 health/origin/현재 slot census/interval을 검산한다. 이미 같은 complete interval을 덮는 synced prefix이면 **AlreadySynced, I/O0**, 같은 Complete이면 정확히 한 caller가 IoOwned를 claim하고 write/sync한 뒤 **SyncedHere**, 같은 IoOwned이면 **AwaitWalOwner, 중복 I/O0**다. authentic old interval은 더 최신 synced prefix로 확인할 수 있지만 raw caller LSN으로 이를 요청할 수 없다. 다른 session은 정상 identity mismatch, 내부 interval/census 손상은 fatal이다.

확인 proof `DurableWal`은 non-Clone/non-Copy이며 실제 requirement와 `confirms(&self, ..)`로 결합한다. 기존 `sync_pending`의 NoPending/Busy 의미를 전역 변경하지 않는다. dirty writer만 private `wait_durable`을 사용하여 먼저 flusher가 sync한 경우의 성공 계약을 유지한다. 모든 page/version/pin/namespace guard 밖에서 같은 predicate를 append lock 아래 재검사하고 completion Condvar가 그 lock을 놓고 대기한다. publication/실패/poison/unwind는 상태 또는 fence를 먼저 설치하고 notify한다. 외부 atomic fence도10ms timed recheck로 발견한다. timeout/시계는 durable authority가 아니며 spinning·추가 I/O를 허용하지 않는다. original uncertain IoOwned/원 lower error를 보존한다.

## DWB 선택과 실제31효과

selector와 그 exact selected control만 authority다. unselected bank, stage, raw slot을 fallback으로 사용하지 않는다. XDW1 Empty의 batch id는 durably selected last-issued highwater이며 Pending 없음만 뜻한다. home 최신성/READY/redo 완료를 증명하지 않는다. control generation·selector generation·batch id는 독립 counter다.

| 단계 | 새 선택/효과 | 최초 batch의 값 |
| --- | --- | --- |
| issuance | Empty를 directory durable로 선택한 뒤에만 첫 image write | control g2, selector g2, batch1 |
| image | slot0 정확16544 bytes write와 images file sync | XDS1 sequence=batch1 |
| Pending | 전체 image 목록1개를 directory durable로 선택 | control g3, selector g3, batch1 |
| home | trusted FILEMAP의 actual allocator address에 captured16384 bytes write, 해당 file sync | selected Pending 유지 |
| retirement | 전체 batch를 Empty로 directory durable 선택 | control g4, selector g4, batch1 |
| 다음 batch | checked 새 번호로 같은 순서 | batch2, g5/6/7 |

각 control 선택은 **stage write→stage sync→inactive bank write→bank sync→selector-stage create-new→selector write→selector sync/close→atomic selector publish→directory sync**의9 logical effects다. 세 선택27+image2+home2=31이다. WAL의 선택적 write/sync2, open/read/metadata, short/Interrupted 물리 subcall은 별도 집계한다.

stage/bank는 exact16384 전체 overwrite이며 logical control160/248 뒤 zero padding이다. 최초 운영 stage write는 XDI1→XDW1 전환이다. stage 자체는 선택 authority가 아니다. selectors3개/control3개/slot은 첫 DWB effect 전에 원 workspace에 완전히 encode하고 이후 바꾸지 않는다. private I/O bridge는 capture 단계에만 image mutable access를 주며 prepare 이후 재prepare/execute/임의 image 변경을 막는다. execute는 exact captured requirement를 확인한 실제 `DurableWal`을 요구하고 actual home sync+whole retirement 뒤만 nonforgeable `RetiredBatch`를 발급한다.

home 경로·offset·길이는 검증한 immutable profile/selected FILEMAP에서 얻는다. page self-declared address를 파일 열기 권한으로 사용하지 않는다. selected control의 exact digest와 full zero padding을 검산한다. 더 높은 LSN/incarnation의 healthy home 또는 target 의미 불일치는 fence하며 오래된 capture로 덮지 않는다. 이 경로는 torn home repair/reopen 권한이 아니다.

namespace `_writer` mutex는 I/O-port 단독 직렬화다. open/publish/dirsync 동안 append/claim/DPT/pin/content/version guard를0개 보유하며 port 아래 DB metadata 취득/상태 발표/append health 취득을 하지 않는다. local File write/sync 전에 port를 놓고 WAL file mutex와도 중첩하지 않는다. I/O 후 file/port guard를 먼저 반환하고 진행 상태를 발표한다. atomic fence set은 오류 직후 가능하다.

## continuation·retirement·조건부 finalize

| retained 상태 | 새 `try_flush_one` | `continue_flush` |
| --- | --- | --- |
| Ready | 검사→claim/copy→실제 진행 또는 NoDirty | NoPending/Unchanged, I/O0 |
| WaitingWal | Busy/Unchanged, 기존 claim/capture 보존 | 같은 requirement로 재관측; 새 headroom/claim/copy 없음 |
| RetiredPendingFinalize | Busy/Unchanged, retirement/claim 보존 | RAM finalize만; WAL prefix 조회·namespace/file read까지0 |
| 실패/fenced | 첫 fatal과 원 phase/보호 보존 | 성공 retry/resume를 제공하지 않음 |

DedicatedFlusher finalize는 Content admission→content exclusive→registry 순서이며 ordinary Apply/Version이나 raw mutable page를 받지 않는다. actual retirement에서만 만든 borrowed `FlushFinalizePermit`가 exact anchor/claim/capture/retired proof를 결합한다. 모든 fallible 검사 뒤 counter가 capture와 같으면 bytes/pageLSN/counter를 보존하면서 dirty/recLSN/DPT/requirement만 함께 제거한다. counter가 더 크면 최신 coherent pair와 원래 first recLSN을 유지한다. Clean 이후 새 dirty cycle은 새 actual group LSN을 first recLSN으로 시작한다.

content/admission/pin이 Busy이면 RetiredPendingFinalize를 반환한다. 성공 assignment 이후 cleanup/claim 반환이 실패해도 Clean prefix와 durable retirement를 되돌리지 않고 첫 fatal/fence/claim을 보존한다. exact claim은 checked cleanup 뒤에만 반환한다. Complete의 CleanAtFinalize는 그 관측 시점의 의미이며 반환 순간까지 redirty가 없다는 보장이 아니다.

오류 관측에는 원 lower cause/effect/certainty, attempted/completed logical prefix, known/possible selected union, batch/slot/claim, home sync와 RAM finalize 사실을 함께 남긴다. before failure는 해당 효과를 attempted에만 세며 after mutation failure도 completed로 세지 않는다. 보수 보호 union은 publish를 시도하기 전에 설치한다. publish/dirsync의 before/after 실패8/9/19/20/30/31에서 old/new possible union을 유지한다. publish BeforeError의 actual physical bytes가 old인 관측은 이 보호 union과 별도로 보존한다. issuance 불명에서는 image/home0, live session fenced/reissue0이다. 미래 recovery가 실제 old Empty를 검산하면 미발급 번호의 처리 판단은 그 별도 계약이 맡는다. image0인 휘발성 후보 번호를 crash 뒤 영구 발급으로 주장하지 않는다. selected Pending은 전체 repair/sync/retirement 전 주소 재사용/후속 redo/flush를 열 수 없다는 인계 의무가 남는다.

## F05 예약·effect 전 guard

원래 workspace4,194,304 안에서 captured image16384+slot16544+새 bank3×16384+현재 bank16384+새 selectors3×4096+현재 selector4096+home readback16384+FILEMAP root/scratch2×16384=**164000**, 전용 잔여4030304다. 페이지/control 크기 stack·heap scratch나 두 번째 workspace를 만들지 않는다.

기존 DWB16MiB는 images16711680+bank0/bank1/stage49152+current/select-stage8192+reserved8192=16777216이다. physical slots1010×16544와 residual2240을 유지하고 이 leaf는 slot0만 쓴다. selector replacement peak4096은 이미 이 예약 안이다. combined fixed22073344를 중복 charge하지 않는다. namespace2+WAL1+local open handles≤2로 FD 후보peak5를 별도 native 관측해야 한다.

기존 dirty196608과 신규 flush65536은 별도 metadata 예약이며 합계262144다. global metadata는2490368/8388608, 미배정5898240이다. 신규 registry64 requirements·claim/attempt·Condvar·control/proof/result·constructor/move를 새65536에 계상한다. 다음은 **source-value admission 식**이다. 실제 target layout과 guard 결과는 [선택 인수](../evidence/dev3-allocator-flush-20261022.md)에 연결하며 physical stack 측정이라는 주장은 하지 않는다.

- `R = sizeof(Mutex<DirtyRegistry>) − sizeof(Mutex<[Option<DirtyEntry>;64]>) + sizeof(&AtomicU8)`; `C = sizeof(FlushControl)`; `T = R+C+1024`.
- hot: `T +34×1024 +8192(capture/finalize) +4096(WAL) +8192(I/O) ≤65536`.
- writer와 flusher의 WAL call stack은 동시에 살아 있을 수 있다. `max(2×4096 WAL,4096 writer WAL+8192 flusher I/O)`를 기존4096+8192로 덮고 WAL 반환 proof는 outer8192에 별도 계상한다. I/O 소유권이 하나라는 사실로 대기 call stack을 생략하지 않는다.
- cold: `T +max(5R+8192,3R+8C+8192) ≤65536`. Pool 생성 완료와 이후 Bound/control 생성을 분리한다. 각 단계의 initialized source 값을 보수 중복 계상하며 moved-from 물리 stack slot이나 아직 초기화하지 않은 return place는 포함하지 않는다. r0의 모든 단계 합산은 정상 bind를 거절하므로 실행 전에 수정했고 원 제안을 보존한다.
- 각 caller의 receipt family는8×128≤1024다. sole capture/finalize8192는 내부3840+outer Result2×1024+error768+proof/retired2×256=7168과 잔여1024를 계상한다. retained capture/retired는C에 별도 포함한다.
- generic deep path는 actual owner+writer control+flush control+**4×File**+WAL4096+nested8192+outer capture/finalize8192≤65536을 checked 검산한다. source/return File carrier는 각각≤8192이며 physical open File4 허용이 아니다. bind가 이 guard를 선행하므로 기존 split 두 view만 쓸 새 bind도 File8193..16384를 preclaim Capacity/Unchanged로 거절한다.

개별 ceiling은 requirement/Option128, DurableWal256, EnsureDurable/Result512, Condvar64, retained O/Option192, install permit384, flush control4096, flush error/progress768, Result progress1024다. source 수명표·codec/SHA/FILEMAP nested carrier·actual sizeof/constructor peak를 독립 D1/FR01에서 검산하고, FR02가 exact cap/aggregate 경계의 무효과 거절을 확인해야 한다. std 내부 allocation/physical stack/RSS/latency는 이 ledger의 증명 대상이 아니며 NOT_MEASURED다. guard 실패를 이유로 예약을 자동 증액하지 않는다.

## 입력·미완료와 재검토

필수 inventory는76=FN6+FA13+FB31+FP11+FU6+FC6+FR3이며 actual bytes/claim/WAL/경쟁/cleanup을 대상으로 한다. independent bad witness19는 FR03의 입력이며 제품 mutant19회가 아니다. 원16입력은 보존하고 retirement durable→finalize, image write→image sync, capture→image write의 누락 순서 반례3개를 N17/N18/N19로 추가한다. WAL durable fact는 capture 이전부터 성립할 수 있어 capture→WAL durable 순서를 요구하지 않는다. 새 WAL8 selectors/30 cases, native3DB, compiler privacy/lifetime 입력은 별도 분모다. native3는 profile1/2/3 actual2batch의 bytes·FD·fixed disk peak를 확인한다. 선택 실행 전 source/binary/fixture/helper/hash·watchdog·정확 case marker를 결합하고 완료/미실행을 분리한다. 계약 초안의 선택 인수는 NOT_RUN이었다. 이후 WAL30·compiler12·필수76·native3의 정확한 source별 범위와 원 FAIL은 [선택 인수 기록](../evidence/dev3-allocator-flush-20261022.md)을 따른다.

S5 batch1 확대는 profiles3×실제 reserve/cancel1→8→63의6지점×(normal1+after31+short11+Interrupted11)=972행으로 작성한다. 필수120초/S51200초는 실제 실행 aggregate 한도이며 build/setup을 별도 기록한다. 중간 단계는 변경 기능의 필수 사례만 실행하고 S5 확대는 DEV3 최종 인수에 선택한다. 오류 수정 뒤에는 해당 diff/실패 사례만 재검증하고 원 결과를 보존한다. 기존 하위 wrapper를 반복하지 않는다.

상위 소비자·batch8/64·다중 file·new owner/reuse·ordinary recovery·WAL rotation·scheduler가 추가되면 다시 D2를 검토한다. 형식 bytes는 기존 XDW1/XDS1/XCS1 정본을 유지한다. 아래 로컬 원본은 설계·검토 provenance이며 Git export에 포함되는 실행 증거 링크가 아니다. 초안 작성 당시의 NOT_RUN을 이후 실행 결과에 소급하지 않는다. 현재 제품/실행 상태는 총괄이 별도 구현·evidence 정본에 원 source/binary/input identity와 함께 통합한다. 원본 제안을 소급 수정하지 않는다.

| 로컬 근거 경로 (`artifacts/dev3/resume-20261022/` 기준) | SHA-256 |
| --- | --- |
| `review/flush-one-d2-review-r1.md` | `1405adb4b0e545bc971207ebf83d4aaedc1741f36b15a624496e602f02161bb0` |
| `flush-owner-assembly-decision-r0.md` | `0999ecf332aeecf3082604b75358bcc43da060a7ac566ea34e6a0ef36c184d4b` |
| `review/flush-wal-requirement-d2-review-r0.md` | `360b180cd56192a9d92f71c428288afe98c5fb0074fd79e113341b1d2daed438` |
| `review/flush-file-carrier-d2-addendum-r0.md` | `dd778c4e28e6e3c268b01006d0d31fcde9e51d82041944accb7191158a307c52` |
| `flush-metadata-source-ledger-r0.md` | `5cfe16bd6b7ec8b55cff751d38c5bee1bc626c0ccd8f649d4d6c6819c28fa9e2` |

최종 metadata 생존 단계 증명은 `artifacts/dev3/resume-20261022/flush-metadata-source-ledger-r2.md` SHA `235c2bf10fec5bee49f41feaab61cc2826fbe9597805c3a7809911aca1758ce8`를 따른다. r0의 과도한 동시 합산과 r1의 WAL 소유/생존 표현은 원 제안으로 보존한다.

inner I/O의 최종 전제는 `artifacts/dev3/resume-20261022/flush-io-source-ledger-r2.md` SHA `e135c6f272c1331eebab57f6c3ca4d0c1a80a35db101e3def2e1c20097e5dba0`다. 최초 FR01의 타입 cap 실패를 보존하고 Profile96/FILEMAP root192·cursor224·chunk112 및4개 Result 전제를 확인한다. prepare2532≤2560, FILEMAP3056≤3072로 기존 inner8192를 유지한다. 최종31layout에서 hot63960/cold48576와 concrete deep24624, effect 전 File/aggregate 거절을 확인했다.

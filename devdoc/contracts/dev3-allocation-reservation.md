# DEV3 실제 페이지 예약·취소

2026-10-21 · **D3-ALLOC-RESERVE-001 D2 ACCEPTED / 채택 시 제품 실행 NOT_RUN; 현재 구현·검증 진행 중**

[저장 설계](disk-storage-design.md) · [할당 표현](dev3-allocation-format.md) · [의미 WAL·복구 의무](dev3-obligation-format.md) · [최초 DWB 설치](dev3-doublewrite-install.md) · [현재 구현](../implementation/dev3-implementation.md)

## 1. 목적과 소유

실제 선택된 첫 extent의 metadata page를 읽고 SystemBegin 예약·SystemEnd 취소를 WAL과 authoritative dirty frame에 함께 적용한다. DoublewriteStorageSession을 소비하는 단독 allocation owner가 기존 namespace/WAL/DWB 소유권을 계속 보유한다. root가 설계·제품·통합을 담당하고 `/root/dev3_selector_review`가 독립 D2/D1을 검토한다. 기준 commit은 `a29db4629139cfec568e83e27b9fef34bd5e169c`, source는 `4e594d598c104d2ebddc34ef9e749bc432d66b0173e90b0c75605355609cfbf0`이다.

이 작업은 실제 allocator의 첫 변경 경로다. Reserved→Allocated+INIT, 범용 dirty buffer·동시 writer/FIFO, DWB Pending/home/retirement, 일반 reopen/recovery, DB READY·거래 ACK는 후속 구현이다. 성공 반환은 이번 WAL group의 sync 완료만 의미한다. 기존 무효과 표현의 decode 성공을 실제 예약 권한으로 바꾸지 않는다.

## 2. API와 수명

소비 constructor는 설치 owner, caller metadata frame16KiB, caller operation workspace80KiB를 받는다. 기존 DWB flush workspace4MiB는 보존하며 빌려 쓰지 않는다. operation workspace는 기존 공유 progress12MiB의 독점80KiB이며 새 quota가 아니다. constructor에서는 앞32KiB를 selected reader root/scratch로 사용하고 reader 종료 후64KiB WAL 준비+16KiB after page로 사용한다.

`reserve_pages(count)`는1..63의 page를 예약한다. 동시에 active batch는 하나다. `cancel_reserved()`는 내부 active batch 전체만 취소한다. caller가 raw owner/item/page bytes를 주어 다른 예약을 취소할 수 없다. 관측 API는 현재 예약, dirty metadata, WAL prefix의 복사본만 제공한다. raw writer/WAL/reader/mutable frame은 노출하지 않는다. 모든 변경은 `&mut self`로 직렬화하고 임의 caller callback을 실행하지 않는다.

constructor는 fence/poison·reader 부재·WAL Idle와 complete/synced LSN0/offset4096/epoch1·같은 Profile을 검사한다. trusted fixed FILEMAP entry의 metadata identity로 실제 page0을 읽는다. pageLSN0, entry0의 allocator self-identity, payload63개의 Free(1)를 확인한다. raw home의 identity를 expected로 채택하지 않는다. 실패는 소비 owner의 CallerMustResolve와 fence이며 파일은 보존한다.

metadata frame은 configured buffer의 한 frame이다. 후속 dirty buffer로 넘어갈 때 같은 authoritative frame·recLSN·modification counter를 인계해야 한다. stale home reader를 다시 여는 API는 없다. Drop은 sync/취소/삭제 성공을 숨기지 않는다.

## 3. 상태 전이와 identity

예약은 eligible Free page를 번호 오름차순으로 선택하여 Free(g)→Reserved(g+1,claimant)로 바꾼다. Free(MAX)는 제외하고 필요한 수가 부족하면 무효과로 거절한다. 취소는 Reserved(g,claimant)→Free(g)이며 소비한 generation을 되돌리지 않는다. entry0·미선택 entry·page identity/kind/level은 보존한다.

claimant는 실제 namespace의 AllocationMaintenance SystemWorkRef이며 kind별 증가 id(처음1), generation1이다. 성공 stage 때만 high-water를 갱신하고 취소 후에도 유지한다. constructor의 pristine 조건 외에는 high-water0 초기화를 허용하지 않는다. checkpoint/recovery는 이 값을 인계해야 한다.

LSN은 complete predecessor+1의 논리 증가다. modification counter도 group마다+1이다. reserve preflight는 LSN과 counter의+2를 확인하여 terminal 한 단계를 보호한다. work-id+1도 checked다. cancel은 LSN/modification counter의+1만 소비하며 새 work-id를 발급하거나 high-water+1을 검사하지 않는다. pristine0·최소 frame21200·WAL256MiB 무회수에서 전체 group≤12661이므로 현재 실제 상태에서 u64 끝에 도달하지 않지만 방어 검사는 유지한다. near-MAX 시험은 test-only 교란이며 실제 native 도달 상태로 주장하지 않는다.

첫 변경의 recLSN은 해당 LSN이고 이후 변경·취소에도 유지한다. pageLSN만 매 group 갱신한다. dirty→clean, eviction, home flush는 이번 owner에 없다.

## 4. 정확한 WAL과 descriptor

기존 XWF1 tag2/XSG1을 사용한다. outer kind는 System이며 TxnId/previous-txn LSN은0이다. 전용 private producer/검산기만 권한을 만들고 raw legacy try_stage는 tag2를 계속 거절한다.

| 항목 | reserve | cancel |
| --- | --- | --- |
| opcode | SystemBegin8 | SystemEnd10 |
| before descriptor | 없음, previous/hash0 | 실제 active descriptor 전체 bytes/hash/LSN |
| progress | Prepared,sequence1,cursor0,limitN | Finished,sequence1,cursorN,limitN |
| chain | birth=apply,previous0,groups1 | birth유지,previous=before.apply,groups2 |
| OperationCore | tag8,length256,다른 field0 | tag10,length256,cursor-after=N,다른 field0 |
| NativeEffect | AllocationCell6 하나 | 동일 |
| InventoryDelta | Reserve1 absent→PageToken N개 | Release3 원래 PageToken→absent N개 |
| PageRedo | metadata page0 Patch 하나 | 동일 |

NativeEffect는 flags3,redo index0,slot0,cell offset10144,length6208,record length12544다. full before/after cell SHA를 포함한다. PageRedo는 offset10144,length6208이고 priorLSN은 실제 before pageLSN이다. after canonical page의 pageLSN/checksum을 갱신하며 전체 allocation decoder, unchanged entries, native bytes와 Patch를 대조한다.

Item96는 PageToken1,Terminal category3,owner ordinal0,units1,선택 page의 새 generation이다. reserve changeLSN은 해당 LSN이고 cancel은 그 원래 bytes/LSN을 유지한다. canonical page 순서로 전체N개를 hash한다. descriptor에는 그 count/digest와 inventory-change LSN을 넣는다. cancel 후 count0/emptySHA,changeLSN=cancel LSN이다. 전체63 entries와 전체 owner inventory를 대조하고 일부 item만 확인하지 않는다.

System work-root 없음/work-state emptySHA/last-mutation0/consumed-control0이다. active minimumWalLSN=birth,Finished이면0이다. descriptor의 minimum0은 component의 dirty recLSN/retained WAL 반환을 뜻하지 않는다.

정확 frame 길이는 양쪽 모두 `F(N)=192+192+1536+256+12544+208N+64+6208=20992+208N`이다. N1/8/63은21200/22656/34096이다. reserve consumedWal=F(N),cancel=2F(N). active Terminal row만 steps1,walF(N),pagesN,protectedF(N)+16384N,peak(frame1,memory81920,locks0,registry0,files1,dwb0)이고 다른 값/row는0이다. Finished의 모든 row는0이다.

## 5. 자원과 terminal 반환

B=기존 DWB fixed protected21,024,768(초기 WAL header4096 포함), R=complete_offset−4096, T=아직 stage하지 않은 terminal WAL, P=아직 반환하지 않은 page-token bytes, M=첫 변경 이후 유지하는 metadata page16384이다. `B+R+T+P+M <= 1GiB`와 `complete_offset+2F(N)<=256MiB`를 reserve의 첫 효과 전에 확인한다.

| 시점 | R | T | P | M |
| --- | --- | --- | --- | --- |
| pristine |0|0|0|0|
| reserve admission의 예상값 및 stage/apply/sync |Rold+F(N)|F(N)|16384N|16384|
| cancel stage/apply·sync 전 |Rold+2F(N)|0|16384N|16384|
| cancel sync 성공 후 |Rold+2F(N)|0|0|16384|

admission 행은 prospective 상한이며 실제 R/complete_offset은 stage 성공 전 변경하지 않는다. R은 staged slot도 포함하고 실제 WAL 회수가 없으므로 감소하지 않는다. active descriptor는 T+P를 소유한다. cancel stage 뒤 Finished descriptor가0을 표시해도 component의 terminal-pending P는 sync 성공 전까지 유지한다. 오류/unwind이면 fence하여 P를 반환하지 않는다. 이후 active batch를 없애도 component의 dirty frame/recLSN,high-water,R,M은 남는다. metadata M은 PageToken inventory에 넣지 않는다.

기존 extent page token을 owned file bytes에 또 더하지 않는다. fixed/max owned22,073,344/290,504,704는 기존 설치와 동일하며 증가하는 WAL은 기존 max에 포함된다. 보호의 보수 상한은 `21,024,768+(268,435,456−4096)+63*16384+16384=290,504,704`로1GiB 아래다. global engine admission 전체의 완료 증거는 아니다.

owner≤16KiB,temporary entries/codec/names/scalars≤16KiB,extra File≤16KiB,plan≤4KiB를 기존 component64KiB에 포함한다. 실제 concrete size와 동시 생존 범위를 구현 검토에서 확인한다. WAL slot1MiB·buffer frame16KiB·operation workspace80KiB·flush4MiB는 서로 다른 공급을 한 번씩 센다. constructor component peak는 기존3FD+reader2FD=5,caller directory1FD는 별도다. operation의 FD peak1은 기존 WAL handle 사용이며 새 파일 생성은 없다.

이 순수 취소에는 payload/undo/status/head/retirement가 생기지 않아 다른 owner로 넘길 작업이 없다. 따라서 위 component의 계속된 소유를 조건으로 SystemEnd를 허용한다. [의미 WAL §7](dev3-obligation-format.md)의 Commit/AbortEnd·일반 terminal 인계 의무를 해제하지 않는다. INIT·Allocated→Free에는 이 예외가 적용되지 않는다.

## 6. 원자 적용과 오류

순서는 fence/poison → typed count → active 존재 조건 → 전체 metadata/descriptor/inventory → 후보/generation → counter headroom → WAL/protected capacity → frame/after-page 준비·검산 → stage Idle/동일 prefix 재검사 → live 적용 → gate 밖 write/sync → terminal token 반환이다.

private PreparedAllocation은 producer/전체 검산으로만 생성한다. immutable prepared bytes·snapshot/header를 갖고 전용 private stage가 이를 소비한다. 임의 외부 bytes나 앞 단계 envelope 성공만으로 이 타입을 만들 수 없다. `&mut owner`가 준비→stage→apply 동안 before frame·descriptor·inventory를 보호한다. 새 일반 semantic dispatch나 recovery 권한을 공개하지 않는다.

건강하고 정합한 owner의 예상 count/state/후보 부족/counter/capacity 거절만 stage 이전 Unchanged이며 권위 상태를 보존한다. 기존 fence/poison·실제 내부 불변식 손상은 stage 전에도 CallerMustResolve와 fence이며 원래 lower effect를 축소하지 않는다. 준비 scratch의 bytes는 authority가 아니다. 모든 decode/encode/checked 산술·자원 확보는 stage 앞에서 끝낸다. stage 이후는 고정 길이 copy·scalar/array 대입만 수행하며 그 구간의 실패/unwind는 CallerMustResolve+공통 fence이다. WAL/page lower error를 원래 cause로 보존한다. gate/latch를 가진 채 I/O하지 않는다. cancel sync 성공 전에는 새 reserve나 free page 관측 성공을 반환하지 않는다.

## 7. 실행 전에 고정하는 시험

새 코드의 runtime8 묶음만 선택한다. 이미 PASS인 unchanged 표현·하위 회귀·전체 leaf는 실행하지 않는다. stage helper를 분리해 실제 수정한 legacy 경로는 그 helper의 기존 관련 시험만 한 번 선택한다. fixture/구현 수정 없이 같은 시험을 반복하지 않는다. 문서 검사는 push gate에서만 실행한다.

| ID | 입력과 필수 판정 |
| --- | --- |
| AR01 | profile1/2/3 Fresh→DWB→owner. actual metadata read,pristine,component≤64KiB,frame/operation/flush 분리,FD peak5 및 반환. constructor는 파일을 변경하지 않음 |
| AR02 | 각 profile에서 reserve1/cancel,reserve8/cancel,reserve63/cancel. LSN1..6,work-id1..3,recLSN1,counter6. 독립 full frame/page/descriptor/item bytes와 비교. page1 generation2→3→4,2..8은2→3,9..63은2. 모든63entry 검사 |
| AR03 | count0/64,active 중 reserve,active 없는 cancel,eligible0/N−1/N,Free(MAX),LSN/MAX−1/MAX와 counter 동형,work-idMAX. WAL cap에서2F−1/동일/+1 여유. 오류 우선순위와 stage 이전 authority/file 불변. synthetic 수치는 native 상태와 구분 |
| AR04 | 정상 prepared frame의 header/owner/predecessor/descriptor/budget/item/native/redo 의미를 바꾸고 외부 hash를 재계산한 변형 거절. before63entry/batch 전체 일치,unchanged entries,원래 item changeLSN. private 권한 생성 전 거절 |
| AR05 | stage snapshot 뒤 Busy/동일LSN 다른offset/stale predecessor,slot missing·poison,stage 뒤 apply 전/후 unwind. stage 이전 무효과와 이후 fence 구분;모든 후속 observe/reserve/cancel 거절 |
| AR06 | reserve/cancel 각각 WAL write/sync BeforeError/AfterError/BeforePanic/AfterPanic;write PrefixError/Zero/InvalidCount/Short(1)/Interrupted. 원래 cause/effect·partial bytes·retained charge·공통 fence,short/interrupt의 정확 최종 frame |
| AR07 | 첫 recLSN 유지,terminal sync 전 P 유지/재예약 불가,terminal sync 후만 P 반환,R/M/high-water 보존. 연속1/8/63 cycle의 보호 회계와 exact descriptor remaining/consumed |
| AR08 | constructor fenced/poison/nonpristine/active reader,wrong native identity/pageLSN/free state/physical length/read fault. 소비 실패의 fence·파일 보존·FD 반환·거대한 concrete owner/File size 거절 |

AR04의 raw mutation inventory와 정확 error enum은 구현 전 별도 고정 입력으로 보존한다. native2는 (A)profile1/2/3의 AR02 성공3DB와 (B)reserve/cancel 각각write-before/write-half/write-after/sync-before/sync-after10DB이다. 실제 Linux byte/prefix와 원래 오류·fence를 관측하며 home/DWB 파일은 변경되지 않아야 한다. 일반 recovery/SIGKILL 복구 성공을 주장하지 않는다. 각 DB≤24MiB,전체13DB≤312MiB,각 case30초/runtime 묶음120초/compiler30초를 보호 한도로 둔다. 시간 초과는 PASS가 아니다.

compiler6은 installed owner/metadata frame/operation workspace/flush workspace/WAL slot의 재사용 금지5개와 owner 종료 후 반환1개다. 실제 entry point·diagnostic 식별자를 고정한다. oracle는 계약 offsets/입력으로 독립 생성하고 production encoder를 정답 계산에 사용하지 않는다. source/binary/fixture/log identity를 결과에 보존한다.

## 8. 결정과 남은 경계

제안 원본 r0,flush workspace 정정r2,보호 회계·권한r3은 ignored 준비 기록에 보존한다. 이 문서를 단일 채택 원문으로 확정했다. 독립 r2 검토는 정상 literal63개·811008bytes와 최종 의미 반례541개·compiler 입력6개를 확인하고 구현 착수를 수용했다. 기존 설치·표현·모델의 PASS를 이번 실행 PASS로 복제하지 않는다. 실제 INIT/dirty buffer/DWB 운영/복구/전체 DEV3는 미완료다.

## 9. 구체 API·오류·입력 동결안

```rust
impl<'m, 'w, W: WriterNamespace> DoublewriteStorageSession<'m, 'w, W> {
    fn into_allocation_session<'p, 'o>(
        self,
        frame: &'p mut [u8; 16384],
        operation: &'o mut [u8; 81920],
    ) -> Result<AllocationSession<'m, 'w, 'p, 'o, W>, AllocationSessionError>;
}
impl<W: WriterNamespace> AllocationSession<'_, '_, '_, '_, W> {
    fn reserve_pages(&mut self, count: usize) -> Result<AllocationObservation, AllocationSessionError>;
    fn cancel_reserved(&mut self) -> Result<AllocationObservation, AllocationSessionError>;
    fn observe(&self) -> Result<AllocationObservation, AllocationSessionError>;
    fn reserved_page(&self, ordinal: usize) -> Result<Option<PageRef>, AllocationSessionError>;
    fn is_fenced(&self) -> bool;
}
```

모든 표기 API는 engine 내부 pub(crate)이며 외부 stable API가 아니다. AllocationObservation은 active work-ref(없을 수 있음),reserved count,pageLSN,recLSN(초기 없음),modification counter,work high-water,protected total,WalPrefix의 복사다. reserved_page는0-based ordinal이며 active 범위 밖에는None이다. 이 PageRef는 예약 상태의 관측이며 할당된 payload 접근 권한이 아니다. fence 뒤에는 is_fenced만 정상 진단이고 다른 관측/변경은 CallerMustResolve다. constructor 실패 때 caller frame/operation scratch는 변경될 수 있고 discard해야 한다. 반환된 새 owner는 없으며 파일은 보존한다.

AllocationSessionError는 phase(Constructor/Read/Preflight/Prepare/Stage/Apply/Sync/Observe),effect,cause를 갖는다. cause는 InvalidInput/ActiveReservation/NoReservation/Exhausted/CounterExhausted/Capacity/Fenced/Poisoned/Invariant 및 원형 ControlError/AllocationError/PageIoError/ObligationError/WalIoError/SemanticError다. panic은 Result로 바꾸지 않고 unwind fence를 남긴다. constructor error는 모든 phase에서CallerMustResolve다. 그 밖의 InvalidInput/ActiveReservation/NoReservation/Exhausted/CounterExhausted/Capacity는 건강한 owner의Preflight/Unchanged다. 내부 decoder 불일치·Invariant·poison은Preflight/CallerMustResolve+fence다. stage의Busy/stale/capacity lower가Unchanged이면Stage/Unchanged를 보존하며 lower fence는축소하지 않는다. stage 이후 모든 error는CallerMustResolve다.

AR04의 private 검산기는 Corrupt/UnsupportedOperation/UnsupportedNativeBody/IdentityMismatch/OrderMismatch/StateMismatch/PageMismatch/InventoryMismatch/BudgetMismatch를 구별한다. 검사 순서는 envelope→dispatch→group shape→owner identity/chain→descriptor local decode→descriptor identity/chain/progress→native before/after 및 redo→inventory→budget이다. malformed descriptor는Corrupt이고 lower-valid descriptor의 의미 불일치만 뒤의 정확 tag를 사용한다. private 검산기의 단독 Err는 무효과이며 producer가 내부에서 그 Err를 얻으면Prepare/CallerMustResolve+fence다. rawbytes→Prepared 공개 생성자는 없다.

실제 표·raw 입력은 준비 r4-literals.py/r4-corpus.py와 r4-inputs에 보존하며 독립 검토 후 같은 bytes를 fixture로 등록한다. literals63개/811008bytes는 profile1/2/3×N1/8/63×(reserve/cancel frame,page,descriptor와 Item stream)이다. 기존 bootstrap literal profile/초기allocation은 읽기 입력이며 실행하지 않는다. 새 raw frame는 Python struct/정수/hashlib로만 구성한다. sequential WAL final offset160000,LSN6,counter6,recLSN1,high-water3이다. generator 실행은 product/checker/model/test PASS가 아니다.

native13DB 모두 같은 빌드의 실제 namespace/WAL writer를 사용한다. 정상3DB는각profile의1/8/63 cycle,장애10DB는profile1·N8의 reserve/cancel에write-before(0)/write-half(11328)/write-after(22656)/sync-before(22656)/sync-after(22656)이다. cancel 장애의 선행 reserve는동일 DB에서sync 완료한다. 오류 주입이 발생한 작업의 실제frame prefix만추가되고새성공관측은없다. native 실행은fork worker provenance/source/ELF/입력/log hash를기록하며전원손실모델/정상recovery로주장하지않는다.

AR05의 instance-only cut은 AfterSnapshot,AfterStage,AfterApply,AfterSync이다. AfterSnapshot에서Idle→Complete 또는prefix offset만변경하면각각Busy/OrderMismatch의Unchanged,slot을None으로변경하면Poisoned/CallerMustResolve다. AfterStage/AfterApply panic은fence,AfterSync fence는성공발행전거절이고pending P를유지한다. AR07은cancel의sync port barrier에서component의test-only검사로terminal pending P>0·Finished row0·dirty recLSN/retainedR를관측하고,release후에만성공/다음reserve가가능함을확인한다. 생산API는barrier/callback을노출하지않는다.

compiler probes는기존공통owner의재시험이아니라새AllocationSession까지이어지는borrow를검증한다. installed owner재사용E0382,frame/operation/flush/WALslot중첩변경각E0499,release는컴파일성공이다. 실패진단은해당표시줄의primary span과오류코드로판정하며무관한컴파일오류를성공으로세지않는다.

stage copy helper를실제분리한경우기존시험중`wal_slot_writes_exact_logical_and_physical_prefixes`, `wal_slot_rejects_boundaries_before_effect`, `wal_io_releases_gate_and_protects_slot`, `bootstrap_wal_requires_selected_mapping`만그변경경로의회귀로선택한다. 분리하지않고기존stage를그대로두면이네시험도재실행하지않는다. 다른unchanged기능은실행목록에포함하지않는다.

독립 채택 기록은 [r2 원본 결정](../evidence/dev3-allocation-reservation-d2-xadb-20261021-r2.json)이다. 검토 입력 aggregate는 `da04f60725ff8c887d69b549a0e6dcf5384de00ff55786a54171e63270d94883`, 최종 corpus SHA는 `29d35145aee0b18c49bef06ea449cd9bd25157a0167fc2b741bfc5458c1102b4`다. 해당 판정은 제품 시험 PASS가 아니다.

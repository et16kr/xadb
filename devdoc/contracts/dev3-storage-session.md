# DEV3 최초 저장 세션·FILEMAP 설치 계약

2026-10-21 · **D3-STORAGE-SESSION-001 D2 독립 검토 수용 / 제품 구현·실행 NOT_RUN**

[저장 소유](disk-storage-design.md) · [WAL 설치](dev3-wal-io.md) · [선택 page 읽기](dev3-page-io.md) · [읽기 buffer](dev3-read-buffer.md) · [control 표현](dev3-control-format.md) · [자원 계산](dev3-resource-bounds.md)

계약·구현·통합 담당은 `/root`, 독립 검토자는 `/root/dev3_selector_review`다. 아래 규범 본문은 구현 전에 고정한 r6 제안과 V2 검증 입력이다. 이 문서의 준비·채택은 파일 설치·현재-build 복구·전체 DEV3 완료 증거가 아니다. 기존 DEV2 PF04 실패와 전체 DEV3 인수 미완료를 유지한다.

원본 local 입력 위치는 `artifacts/dev3/storage-session-preparation-20261021/`이다. 본문의 `r5-*`/`r6-*` 파일명은 이 위치 아래 보존 원본을 가리킨다. r5 binary/effect/compiler/negative 입력은 r6에서도 변경하지 않았고 r3/r4 및 r5 prose는 대체된 준비 이력이다. 실제 source path는 구현 때만 저장소 배치에 등록한다.

Slot은 타입이 정확히1MiB로 고정하므로 runtime의 잘못된 slot 길이 시험은 존재하지 않는다. slot 수명·재사용 거절은 별도 compiler probe가 담당하며 runtime preflight는 profile와 concrete owner 크기를 검증한다.

## Scope and alternatives

Supply one fresh-only session owning existing genesis WAL, the first allocation extent, both fixed FILEMAP banks, a durable bootstrap staging descriptor and selected FILEMAP. It supports bounded clean page reads and the existing physical WAL append/sync against the installed mapping. No CURRENT, DWB, admission, semantic-group append, home writes, recovery/resume, checkpoint or READY claim.

Keep GenesisWalSession as the sole owner of W, WAL file, caller slot and append state. A private child `storage/wal_io/bootstrap.rs` assembles `FreshStorageSession`, without exposing the contained GenesisWalSession or raw writer. Add a storage-specific shared permanent failure fence (not a general mutex library). Existing exclusive PageFileReader and standalone GenesisWalSession APIs retain their behavior.

Rejected: cloning W/Arc-mutex WriterNamespace adapters hides lease/mutation authority and requires unused mutation stubs. Extracting all existing WAL core into an unrelated owner causes a larger proof/diff than the actual consumer needs. Holding a std MutexGuard in PageFileReader across its lifetime makes it !Send, regressing the real concurrent ReadBuffer; do not implement that design. A consuming initializer with no durable reader/WAL owner would only postpone the same required connection.

Selected: FreshStorageSession has no namespace/page mutation methods after successful creation. Its immutable API and borrow lifetime freeze FILEMAP/data while WAL writes only its separately retained file. A reader carries a borrowed read-only namespace access value, locks W only for each open/handle-metadata operation, and holds an atomic one-reader lease until Drop. No MutexGuard is stored in the reader. W:Send and W::File:Send retain the existing safe thread sharing conditions. Future namespace/home mutation must revisit this freeze and lock order in a separate D2; this leaf provides no raw escape hatch.

## Ownership, fence and concurrent operations

`SessionFence` contains only AtomicBool and methods `is_fenced`, `fence`, and an unwind-only guard. No clear/reset. Genesis owns it; shared reader and buffer borrow it. Genesis retains existing append/file/namespace poison checks and its armed I/O claim; failures keep the exact pending interval protected. Add immutable Profile to Genesis at its existing successful readback, for child use only.

PageFileReader internal namespace access has Exclusive(&mut W) and Shared(ReaderLease) variants. ReaderLease stores &Mutex<W>, &SessionFence and &AtomicBool reader-active. Shared construction checks fence/namespace poison, compare-exchanges false→true, then rechecks fence. Busy has zero I/O. A guard releases the lease on every constructor error/Drop; an unwinding constructor/read fences the session. Ordinary reader Drop performs no I/O and does not fence. No extra heap or unsafe Send/Sync. Exclusive construction remains unchanged and has no shared session fence.

The shared open holds W's namespace mutex through open_existing_with_metadata (and named metadata binding if required by the bootstrap readback), then releases it before any FileIo read. This lock is a namespace operation lease, not the short append/buffer metadata gate. WAL never acquires it; shared reader never acquires append/file mutexes. The buffer loader may own its mutex through page I/O as already contracted; hand/frame-header metadata locks are dropped first. Cached hit/read/view checks the borrowed common fence without acquiring the loader or namespace mutex.

New shared PageIoError variants Busy/Fenced/Poisoned remain typed and read-only. BufferError adds Fenced; local poison is Poisoned and propagates to the common fence. Error precedence: local buffer poison first, common fence second, then request validation; new reader checks common fence/namespace poison before Busy. Invalid requests, missing mapping, read/codec errors remain ordinary read errors and do not by themselves fence a healthy session; poison, unwind and internal buffer invariants do. A load error cancels only its frame claim under the existing contract. Existing immutable PageReadGuard::bytes can still observe acquired bytes after fencing; new pin/read/view operations reject. Drops must release pins/reader lease even when fenced.

Every shared read/constructor checks the common fence before effect and again before returning success. A fence racing after its final check can coexist with that completed read's memory observation; it grants no mutation/commit authority. WAL stage/sync retains its existing linearization and final fence checks. Tests must cover a paused read with WAL error and a paused WAL write with buffer poison.

## Initial identities and selected bytes

Fixed fresh namespace: DB incarnation from existing nonzero entropy, timeline1, profile generation1 and ID1/2/3. FileId=(epoch1,counter1). Allocator ObjectId1/Generation1 is reserved system metadata, never a user object. Initial issued high-water file counter1/object ID1 are recorded in the bootstrap descriptor; future issuing owners must start strictly after them or use a separately durable newer file epoch. This leaf issues nothing else and never reuses these identities.

Data is exactly64×16384=1048576 bytes. Page0 is Allocation, level0, incarnation1, owner1/gen1, pageLSN0. Its allocation cell records entry0 Allocated to the same allocator and entries1..63 Free(generation1); their first reservation increments to2 under the existing allocation format. Remaining data bytes are zero; zero free pages are not valid user page envelopes. Allocation metadata identity is derived from the selected FILEMAP entry, never from its own header.

FILEMAP generation1, selector generation1, bank0, one Live entry for the data file and mapped_bytes1048576. Root logical320, chunk0 logical256; their exact references use existing codecs. Bank0 is129 pages: root at offset0, chunk at16384, all remaining/prefix-tail bytes zero. Bank1 is129 entirely zero pages, reserved unselected workspace, never a valid fallback. Both containers have exact length2113536. Every16KiB segment is explicitly written then each file synced. set_len is not used as a physical-reservation substitute.

## Bootstrap staging descriptor

The control-format prohibition on unpublished file effects without a staging descriptor is honored by a new immutable `d3-bootstrap` 4096-byte descriptor. It is created, written, synced and its directory entry synced **before** any data/FILEMAP creation. It is retained after success and after every failure; no remove/rename/rewrite/resume. It describes this exact bounded genesis-only plan, not arbitrary growth or restored transaction reservations. A partial or missing descriptor never permits resumption. Fresh create still requires LOCK-only inventory and refuses all residue without mutation.

Independent byte table, little endian, zero outside fields; exact4096 length:

|Offset|Size|Value|
|---|---|---|
|0|4|XBS1|
|4|2|version1|
|6|1|mode1 (fresh-only fixed plan)|
|8|4|4096|
|16|16|DB incarnation|
|32|8|timeline1|
|40|8|profile generation1|
|48|32|full profile digest|
|80|8|FileId 0x0000000100000001|
|88|8|mapped bytes1048576|
|96|8|allocator ObjectId1|
|104|8|allocator Generation1|
|112|8|issued FileId counter high-water1 (epoch1 fixed)|
|120|8|issued ObjectId high-water1|
|128|8|FILEMAP generation1|
|136|8|FILEMAP selector generation1|
|144|64|exact bank0 FileMapRoot ControlRef|
|208|32|SHA256 of full initial allocation page|
|240|8|total fixed installed logical bytes5296128|
|248|8|max combined owned logical bytes273727488 (including WAL cap)|
|256|8|protected fixed non-data bytes4247552 (including4096 WAL header)|
|264|8|both banks total4227072|
|272|8|data extent count1|
|280|8|plan version1|
|4064|32|SHA256 of first4064 bytes|

The wire plan names are derived by this version's fixed table: d3-bootstrap, canonical data filename for the fixed FileId, bank0/bank1 canonical control names, d3-filemap-select-stage→FILEMAP, plus pre-existing d3-profile/WRITER/d3-wal-00000001. No free-form paths. Decoder binds expected actual Profile, expected root ref and allocation hash; unsupported/corrupt/mismatch errors retain existing typed distinctions. No decoder can grant resume/READY authority. Descriptor encoder validates all fields before writing output, exact prefix tail preserved; only fixed genesis identities accepted.

## Mutation order and cumulative effects

Existing genesis12 logical mutations and its readback complete first. All new names, fixed plan, controls, hashes and resource arithmetic are prepared before the first additional mutation. Subsequent errors are cumulative CallerMustResolve even before an additional create call. Failed creation preserves artifacts and returns no session.

|Group|Logical mutations|
|---|---|
|Existing WAL genesis|12|
|Descriptor create/write/sync/directory-sync|4|
|Data create/64 page writes/sync|66|
|Bank0 create/129 page writes/sync|131|
|Bank1 create/129 page writes/sync|131|
|Directory sync for data/banks|1|
|FILEMAP stage create/write/sync|3|
|Publish FILEMAP/directory sync|2|
|New total / combined total|338 /350|

Indexing and these totals must be independently generated, not inferred from implementation trace. Physical short/Interrupted subcalls have separate witnesses. After final sync, read back descriptor, complete selected FILEMAP and allocation page through newly opened handles. Verify exact Profile and SelectedControl, exact fixed file lengths, actual allocation page full hash and strict allocation cell decode. Check unselected bank length; do not accept its bytes as selection. Named metadata/handle identity binding in bootstrap checks protects readback identity, not noncooperating external mutation. Keep only WAL+namespace in the returned owner; no bootstrap data/control handle survives.

API: `FreshStorageSession::create(writer, profile_id, slot:&mut[1MiB], scratch:&mut[[u8;16384];3])`; `open_reader(&self, root:&mut[16384], scratch:&mut[16384])`; `try_stage`, `sync_pending`, `prefix`, `is_fenced`. The reader lifetime ties session/root/scratch. No general reopen or mutation constructor.

Fresh try_stage filters every typed PageRedo reference to this installed file and page_no<64. Check after the existing trusted-snapshot full decoder and before slot publication; do not predecode with input-supplied predecessor. A mismatch returns MappingMismatch/Unchanged with no slot/file change. Opaque remains opaque physical bytes, tag2 remains UnsupportedFormat. This is the selected mapping barrier only, not allocation/owner/semantic apply approval. Existing standalone Genesis try_stage has no new mapping restriction. Existing Busy/decode/stale-snapshot ordering is retained; the mapping check sits after full decode and before final state recheck, so a valid decoded mismapping is reported before a later competing state change. Explicit test vectors fix this order.

## Resource reservations and failure artifacts

Fixed bytes5296128 = profile4096+epoch header4096+WRITER4096+bootstrap4096+data1048576+two banks4227072+FILEMAP4096. Staging→selected is one inode charged once; before directory sync preserve the possible none/stage/selected namespace outcome, not two fabricated copies. No old selected inode exists during this fresh publication. Max WAL268435456 replaces its4096 header in the total, giving273727488 < owned8GiB. Fixed protected non-data4247552 fits1GiB; growing WAL retains its separate256MiB cap and remains inside the owned total. Future fixed DWB/checkpoint/recovery reservations are not admitted here.

One caller WAL slot1MiB remains in metadata8MiB under its existing1MiB+64KiB envelope. Bootstrap scratch49152 bytes is temporary maintenance12MiB, not copied into the session. Fixed64 AllocationEntry array and codec scalar stack overlap must be measured with a source/type ledger during D1 implementation review before integration; the fixed total local bound is64KiB in addition to caller scratch, within the same maintenance budget. This is a source/type bound, not compiler stack/RSS/OOM proof. Returned reader uses root16KiB+scratch16KiB; ReadBuffer adds the existing caller load16KiB+frame pool. Shared lease/fence are in existing component metadata, no per-request allocation. First call's list_owned(1) existing Vec is not hidden.

Product file handle peak during bootstrap: existing WAL1 + currently written/read file1 =2, except normal selected page readback WAL1+container1+data1=3. Shared reader/buffer retains WAL1+container1 and peaks at3. Native namespace directory+LOCK2 are additional existing handles. One active shared reader caps the new selection workspace/handles; Busy/Drop/error restores the lease. No fd128 or global F05 completion claim.

## Required fixed verification before execution

The follow-up r5 corpus freezes exact byte hashes, per-cut expected values, operation index, readback failures, compiler primary spans and bounded-model denominator. Do not implement products/tests against an unfrozen expected value. Fixed matrix:

1. Independent profile IDs1/2/3, descriptor/page/chunk/root/selector literals. Python offsets/hashlib only; no production codec for expected bytes. Descriptor all4096 single-byte corruptions, every prefix0..4096, trailing byte, rehashed semantic/reserved damage, encoder sentinels.
2. Successful full installation all3 profiles;350 exact mutations and each name/offset/length/raw bytes; selected root+allocation identity and all full-file zero padding. Wrong/foreign fresh inventories retain original bytes, mutation0. Existing genesis fault matrix retained separately.
3. Each338 new logical mutation: before/after injected error676 cases and unwind338 cases. Every partial namespace preserved; repeated fresh create refuses residue without mutation. No post-error phase or hidden delete/sync/retry. Reads do not count as mutations.
4. Bounded durable-shadow images for each new mutation before/after: synced descriptor is always a prerequisite of data/bank artifacts; file sync pins all full bytes; directory sync pins names; FILEMAP publish before directory sync has none/stage/selected alternatives. Exact image generator and negative controls required; do not claim all possible device corruptions.
5. Short/Interrupted writes at descriptor/data/root/chunk/padding/bank1/selector and zero/invalid/error/prefix-error; sync-before/sync-after; readback open/metadata/read errors, wrong length/identity, changed valid profile/selection/hash. Enumerate exact calls in r5.
6. Real threads with5s timeout: held reader or paused data read permits independent WAL stage/sync; second reader Busy; after Drop reader can reopen; WAL fault/unwind fences cached buffer and outstanding pin.read/view; buffer poison/unwind fences WAL; paused read rechecks common fence before return; cached hit succeeds while unrelated loader/namespace I/O is paused; no forced Send implementation. Pre-acquired immutable bytes remain usable.
7. Mapping filter: wrong file, page64/u64MAX, valid pages0/63, multiple records with last mismapping, malformed/tag2/stale/Busy priority, unchanged slots and prefixes; real stale snapshot schedule remains.
8. Memory/type ledger and handle counter; repeated reader open/drop has no leak. Compiler probes: unchanged baseline, positive Send/Sync scoped thread concurrency, negative reader outlives session/slot/root/scratch, raw writer reuse and second mutable frame workspace. Exact compiler code/spans, not unrelated build errors.
9. Native ext4 all3 profiles/full hash readback/lease, each group boundary after-effect process SIGKILL3 repeats, actual API fault before/after distinct from deterministic shadow. Freeze group cuts in r5; kill proves pagecache observations only. Retain actual source/binary/log/image hashes; current-build DEV2 recovery and lower DEV0/1/2 regressions on the final source.
10. Independent D1 final diff/receipt audit. Preserve the whole DEV3 NOT_RUN and DEV2 PF04 failure. Cleanup only completed targets and byte-identical duplicate immutable workers, retain original logs/source/native images/binaries.

Research input: r0-storage-source-research.md/json records PostgreSQL revision45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851 and exact local file hashes. Only exclusive creation/sync/owner separation informed this design; no import/equivalence/execution claim.

## r5 clarification: fence, cleanup and bounded type state

The read namespace lock is acquired only for an individual actual open operation. The returned file's handle metadata is authoritative. No namespace MutexGuard crosses into the buffer value or survives while reading file bytes. The one-reader lease is an atomic boolean and borrowed session references; no Arc or allocation. Failure/unwind of any shared constructor releases it; leaked readers keep it Busy, never authorize mutation. Shared FILEMAP/root/data remain immutable because FreshStorageSession exposes no mutator or raw owner, even when no reader is active. This is an intentional present capability; adding live dirty pages requires a new page-latch protocol, not silently relaxing this freeze.

ReadBuffer must not just add a common flag check to the existing LoadClaim::finish. For a healthy, matching Loading header, a failed load clears state=Empty, pins=0, referenced=false and disarms the claim **even when the common fence is set**; it then returns the original load error (Fenced if that caused failure). A successful load whose final live check observes Fenced performs the same cancellation and returns Fenced. If the header mutex is poisoned, state/request/generation mismatch, pins nonzero unexpectedly, or local pool invariant already failed, fence local+session, return Poisoned and do not repair unknown state. A failure before claim construction has no claim to clean up. Unwinding an armed claim still fences local+session under the existing panic policy.

A cache hit increments pins only under the matching healthy header, performs a final live check, and undoes that increment before returning a newly observed Fenced. Existing PagePin Drop decrements only a healthy matching Clean header with pins>0; the phrase 'release even when fenced' never permits unpoisoning or repairing a mismatched header. PageReadGuard::view checks local/common fence before decode and before returning success; detected byte corruption fences both. Already acquired bytes() remains immutable observation only.

Constructor admission with invalid capacity returns InvalidCapacity and releases its reader without fencing. Finding poisoned frames/orphan Loading/leaked pins returns Poisoned and fences the provided shared session; it does not rehabilitate that workspace. Every shared reader/buffer operational entry has an unwind guard, including pin/read/view; pure accessors for copied profile/generation and already acquired immutable bytes are not mutation operations. Drop's lease release is unconditional, but pin cleanup is conditional on healthy matching state as above.

The common fence linearizes at its atomic store. An overlapping operation may return success if its final atomic live check preceded that store; the observation grants no later authority. No new operation whose entry sees the fence performs I/O. Already entered I/O cannot be cancelled: a WAL write claim may finish write and file sync after a reader fences the session, but its final publication check returns Fenced, retains IoOwned/pending interval and never advances the exposed synced watermark. Likewise a read may finish its entered read call but must reject at its final fence check. This does not require a new global gate or holding the append gate during I/O. Tests pause inside actual ports to distinguish these cases.

The existing WAL append critical section has no externally supplied callback/allocation in production. Add an unwind guard at each new shared entry and preserve the existing armed IoFence. Namespace poison is checked before one-reader lease acquisition and whenever a shared open locks it. Actual read/codec/request errors without panic do not imply a session invariant failure; if a concurrent global fence appears, the final common fence check can supersede a successful read, while an already returned ordinary error remains its original cause.

The fixed local budget is64KiB. The type ledger is phase-specific; allocation entries and preparation-only names do not remain live during readback. Concrete Fresh/Genesis owner<=16384 bytes is checked before any genesis effect and therefore each W::File member<=16384. These are source/type allowances, not compiler stack/RSS bounds:

|Phase|Simultaneous fixed values|Allowance|
|---|---|---|
|Prepare|owner16384 +64 AllocationEntry values<=8192 +fixed names8×129=1032 +all other codec/scalar locals<=16384|41992 bytes|
|Stream installation|owner16384 +one additional File<=16384 +names1032 +all other codec/scalar locals<=16384; preparation entry array already out of scope|50184 bytes|
|Final readback/shared reader open/read|owner16384 +two additional File values<=32768 (FILEMAP container and data handle) +ALL names/control/cursor/codec/hash/scalar locals<=16384; preparation entries/temporary names gone|65536 bytes|

The readback16384 scalar allowance includes all current names; they are not added a second time or omitted. The actual source ledger must list scopes and nested helpers and verify each row. The retained WAL file is already in owner, and the two extra File values are distinct. Temporary caller bootstrap49152 bytes is separately charged once to maintenance, so total source-level bootstrap workspace<=114688 bytes. Oversized concrete owner rejects before genesis mutations without heap fallback. ReadBuffer's caller frames/load/control workspace and its existing metadata bounds remain separately counted. No generic W/File may bypass the preflight.


The bootstrap descriptor is separate from the proposed1MiB rebase/recovery slots in the existing Profile table. It must not masquerade as either slot or reduce their future budget. Its retained4096 bytes are an additional explicitly charged immutable genesis owner record. The two still-unimplemented rebase slots and their1MiB geometry remain unchanged.

The contained BootstrapPlan is constructed privately from the validated actual Profile, fixed identity/geometry and freshly encoded initial allocation/root/chunk values. Its digest fields are not arbitrary caller parameters. Descriptor decode receives that trusted plan and compares against it. Existing shared open_reader also compares full Profile and full SelectedControl against the plan at every construction, including kind/slot/index/generation/logical length/full digest; passing only expected Namespace to the existing raw reader would be insufficient.

Let C be the last successfully exposed synced offset, L the pending complete frame length, E=C+L. A common fence preserves the following exact WAL ownership:

|State when fence appears|Retained interval/slot|Later operation result|
|---|---|---|
|Idle|prefix C, no pending, slot Some; protected C|stage Phase::Stage/Fenced, sync Phase::Write/Fenced, prefix Phase::Observe/Fenced; each CallerMustResolve and I/O0|
|Complete|previous synced C; complete E; slot Some and immutable pending L; protected E|same three typed results/I/O0; pending bytes are not released/replayed or converted to Idle|
|IoOwned before entered write|previous synced C; complete E; slot absent from append state and owned by I/O claim; protected E|if fence seen at existing pre-write check: Phase::Write/Fenced, no write; claim remains protected, no slot reuse|
|IoOwned within write/sync|same C/E/L ownership|an already entered call may complete; lower write/sync error returns its original Phase::Write/Sync and cause; otherwise publication check returns Phase::Publish/Fenced. Both CallerMustResolve; no synced watermark advance, no slot reuse|

The existing post-write file sync may execute for a claim whose write was already entered before fencing; this is not a new admitted sync operation. A state operation whose last live check preceded fencing can finish its already-linearized state publication. None of these physical observations is a commit ACK. is_fenced is always permitted and returns true. The session fence never clears and Drop never runs hidden replay/delete/sync.

## Exact assembly/error boundary and scratch overlap

`FreshStorageSession::create` returns its own `BootstrapError { phase, effect, cause }`. Phase is one of Preflight, Genesis, Prepare, DescriptorCreate/Write/Sync/Directory, DataCreate/Write/Sync, Bank0Create/Write/Sync, Bank1Create/Write/Sync, FilesDirectory, SelectorCreate/Write/Sync/Publish/Directory, Readback. Cause preserves Wal(WalIoError), Control(ControlError), Allocation(AllocationError), PageIo(PageIoError), Namespace(NamespaceError), Io(foundation::Error), InvalidInput, Capacity, IdentityMismatch and LengthMismatch. A genesis failure retains its exact inner phase/cause/effect and the outer effect equals it; after genesis success all bootstrap errors are CallerMustResolve. Preflight concrete-size/profile errors are Unchanged. On create failure no owner is returned and no cleanup mutation is performed. Test traces carry the actual repeated-write index; the product error does not fabricate a transaction outcome.

The other FreshStorageSession methods return the existing types: try_stage→WalFrameLocation/WalIoError; sync_pending→WalSyncObservation/WalIoError; prefix→WalPrefix/WalIoError; open_reader→PageFileReader/PageIoError. Only WalIo Cause::MappingMismatch and the documented shared page/buffer errors are added to their existing enums. The bootstrap phase enum does not expand the standalone WAL API's phase inventory. A testing-only facade may project these exact categories but must execute the same private owner, never an alternative installation implementation.

Keep all prepared bytes within the three caller pages. During preparation, cell6208 occupies page1 temporarily and allocation image occupies page0; its hash is saved. Then reuse page1 for root320 at[0,320) and chunk256 at[320,576), retaining exact refs as scalar values. Page2 contains descriptor4096 at[0,4096) and selector4096 at[4096,8192). No whole-extent/container heap or second page-sized stack buffer is needed. All codec validation and exact plan bytes finish before descriptor create13.

Write descriptor from page2. Write allocation page0 from page0, then fill page0 with zero and stream its63 free pages. For bank0, copy the retained320-byte root into zero page0 and write first segment; clear page0, copy retained256-byte chunk from page1 and write second; clear page0 and write the remaining127 segments. Bank1 writes129 zero segments. Selector is still the retained page2 slice. No codec is invoked to reconstruct future mutations after the descriptor effect starts. Final readback reuses page0 as root, page1 as reader scratch, page2 as allocation output. A full allocation hash retained in BootstrapPlan validates it without retaining a second initial image.

BootstrapPlan stores only actual Profile, expected SelectedControl and allocation-page digest; fixed identities/counts/names/resource constants are derived, not allocated inventories. Duplicate small Profile copied in Genesis and Plan is intentional immutable validation context (<128 bytes each) and is counted, not an extra mutable owner. The runtime metadata preflight checks the concrete FreshStorageSession size before calling Genesis::create, so an oversized generic W/File cannot leave partial files before this resource rejection.


# V2 고정 검증 입력

## Effect/readback table

Mutation indices are independent input, not discovered by running the candidate: existing genesis1..12; descriptor create13/write14/sync15/directory16; data create17/writes18..81/sync82; bank0 create83/writes84..212/sync213; bank1 create214/writes215..343/sync344; directory345; selector create346/write347/sync348/publish349/directory350. The expected object bytes come from independent literals plus zero padding. Staging names use the bytes of their eventual selected object. Existing genesis readback occurs between12 and13 and remains under its existing separate matrix.

Final readback is exactly the following normal-path observations, all newly opened handles. `open` means open_existing_with_metadata(ReadOnly); it returns handle-bound metadata and is one adapter boundary. Each FileIo full read is one logical boundary with separately tested short subcalls. Named `metadata` is an extra comparison only for descriptor/bank1; the existing page reader's handle metadata semantics are unchanged.

|Index|Observation|Expected|
|---|---|---|
|1|open d3-bootstrap|length4096|
|2|metadata d3-bootstrap|exact same identity+length as1|
|3|read d3-bootstrap|offset0/4096, full descriptor/hash/plan binding|
|4|open d3-filemap-1|length2113536|
|5|metadata d3-filemap-1|exact same identity+length as4|
|6|open d3-profile|length4096|
|7|read d3-profile|offset0/4096|
|8|open FILEMAP|length4096|
|9|read FILEMAP|offset0/4096|
|10|open d3-filemap-0|length2113536|
|11|read selected root|offset0/320|
|12|read declared chunk0|offset16384/256, cursor.finish|
|13|reread chunk0 for page0 mapping|offset16384/256|
|14|open d3-data-0000000100000001|length1048576|
|15|read allocation page0|offset0/16384, full hash and strict64-entry decode|

After observation12 compare exact decoded Profile and SelectedControl with the installation plan before page read. Namespace is actual genesis DB/timeline, profile generation1/id supplied. A valid different profile/ref is not acceptable. Allocation expected owner/incarnation derives from the selected entry. Descriptor/bank1 handles close before the reader opens; readback reader closes before owner return. Mutation after350 is zero. Constructor returns no partially initialized session on any error.

Readback failures: before-error at15 observations; after-panic at15 observations; open wrong length +/-1 at6 opens (1,4,6,8,10,14)=12; named metadata mismatch identity at2/5=2; descriptor fully rehashed wrong expected root hash and allocation hash=2; exact-length valid different Profile (ID2 expectedID1) and exact-length fully resealed alternate selected generation/ref=2; allocation entry1 generation at page byte10312 changed from1 to2 with envelope checksum resealed=1. These19 semantic/identity cases ensure their intended error is reached, not masked by missing files. For the Profile case replace BOTH profile with ID2 and FILEMAP with the valid ID2-bound selector; original root/chunk remain valid, so the full cursor passes before the new fixed-session Profile check rejects. For alternate selection supply a complete valid bank1 generation2 root+chunk and selector generation2 binding that exact reference under the original Profile; no missing/corrupt target can stand in for the new fixed-session check. The raw exclusive PageFileReader must accept each alternate fixture, while the shared session construction must reject it. Actual opened device/inode differences from a retained named observation are handled only where named metadata is requested; normal raw reader does not add uncontracted path stat.

## Engine suite:12 named tests

1. `bootstrap_literals_and_geometry`: profile1/2/3 descriptor/page/root/chunk/selector exact raw bytes and lengths, full installed-file SHA, allocator metadata lookup/summary allocated1/free63, bytes1..63 pages zero, namespace/selected reference; encoder prefix-tail preservation.
2. `bootstrap_descriptor_rejects_damage`: profile1 fixture every prefix0..4096 (4096 errors+1 success),4097 trailing; single byte xor1 at all4096 offsets; every reserved byte individually set1 and rehash; rehashed semantic-field table below; encoder too-short output/invalid genesis Profile rejects without changing output. The private plan computes its own exact root/page hashes; expected-hash mismatch belongs to decoder tests, not an impossible arbitrary-hash truth check by the encoder. Existing page/control codecs retain their full suites.
3. `bootstrap_requires_fresh_namespace`: valid LOCK-only; profile0/4, slot/owner concrete-size preflight; existing residue for each newly owned name6, CURRENT/DWB, foreign name; no LOCK/nonzero LOCK; mutation0 for every rejection. Existing genesis entropy/profile/inventory cases remain unchanged. Failure after genesis but before new mutation has CallerMustResolve.
4. `bootstrap_installs_exact_order`: all3 profiles, all350 logical mutations with exact name/offset/length/data and15 final readback observations; state includes WAL-prefix0/4096 and exact map. Durable-shadow trace is compared to the independently generated676 cuts/6804 images; no READY claim.
5. `bootstrap_faults_preserve_artifacts`: new indices13..350 each before-error and after-error676cells, after-panic338cells. All actually hit once; observed prefix of effects/raw files equals the independent trace up to failure (after-error/panic includes its effect). Follow-up create on same retained namespace rejects mutation0. No hidden deletion/sync/replay. Original cause and cumulative CallerMustResolve retained. Old12-call genesis fault matrix continues separately.
6. `bootstrap_short_io_is_bounded`: write indices14(descriptor),18(allocation),19(zero data),84(root),85(chunk),86(zero bank0),215(zero bank1),347(selector). Each of8 writes has Short(1),Short(17),Short(4096),Interrupted then normal =32 successful cases, and Zero,InvalidCount,BeforeError,PrefixError=32 failures. Short count4096 equals normal descriptor/selector length and is a valid complete call, not claimed as a partial write. PrefixError writes exactly half then returns error. Separate all8 new sync calls (file-sync5 plus directory-sync3) are already covered by the exhaustive fault matrix. Reads at3/7/9/11/12/13/15 use short1,Interrupted and prefix-then-EOF=21cases. No retries after error beyond the foundation's documented short/Interrupted loop.
7. `bootstrap_readback_binds_installed_objects`:15 before-error+15 after-panic+19 semantic/identity cases from the table. Each gets no session, no mutation after350 and all retained bytes preserved exactly. No fallback to bank1 or scan. Shared open_reader additionally compares exact Profile/SelectedControl on every new construction.
8. `bootstrap_reader_and_wal_make_progress`: actual scoped threads,5s rendezvous timeout. While shared reader exists: WAL stage/sync completes; secondreader Busy with open0; dropping reader admits next. Pause data read inside port and complete WAL stage/sync before releasing it. Conversely pause WAL write and complete a normal allocation read before releasing it. Assert no namespace mutex is held while FileIo read and no namespace/append mutex during WAL file I/O.
9. `bootstrap_shared_fence_cleans_claims`: actual paused-read schedules for WAL write error/write unwind/sync error (3), and paused WAL write for buffer invariant poison/read unwind (2). Also fixed external-fence state cases Idle, Complete, IoOwned before pre-write check and IoOwned inside entered write (4). For the before-write case hold the WAL file mutex, start sync, wait for test-only IoOwned observation without holding append gate, fence, then release the file mutex: Phase::Write/Fenced and write0. For entered write success after fencing: Phase::Publish/Fenced with earlier C/pending L retained; use test-only state witnesses rather than calling rejected prefix(). After error: new reader/read/pin/pin.read/guard.view/stage/sync/prefix reject as contracted with new I/O0; is_fenced true; pre-acquired bytes immutable. For common-fence read cancellation the matching Loading becomes Empty/pins0 without creating local Poisoned; normal pin Drop decrements matching pins even after fence. Poisoned/mismatched header never repaired. Drop constructor error/ordinary reader/panic releases only its one-reader lease; all owner/file handles eventually close. Common-fence final check race is documented; no universal wall-clock linearizability claim beyond atomic entry/final checks.
10. `bootstrap_wal_requires_selected_mapping`: valid page0/63, file counter2, page64/u64MAX, valid two-record frame with last record mismapping. Every valid typed input has matching trusted stream/predecessor; missing mapping rejects MappingMismatch/Unchanged, slot/prefix/file unchanged. Busy takes priority before decode; malformed checksum/tag2 before mapping; decoded wrong mapping before final stale-state recheck. Retain existing real snapshot race schedules. Opaque valid, tag2 rejected, no semantic/apply/allocated authority. These deterministic boundary vectors are not a transaction property history.
11. `bootstrap_buffer_preserves_shared_concurrency`: real shared ReadBuffer with W:Send/File:Send; cached A=allocation page0 while second requested page read is paused inside actual data FileIo. A hit/read/release and Busy for already Loading location finish before pause release. The second request can deliberately fail decode on a zero Free page; it must cancel its claim and preserve A. Read errors alone do not fence. Current raw-reader tests and their valid A/B/C page fixtures remain unchanged. Test Fenced cache hit cancellation separately; assert final pin counts and source bytes unchanged.
12. `bootstrap_workspace_and_handles_are_bounded`: source/type ledger and generic metadata preflight <=16KiB, fixed locals<=64KiB plus caller49152; file peaks2 during construction,3 during readback/shared read (directory+LOCK separate), post-create WAL1; repeated open/drop128times no handle/lease growth; buffer capacity existing profiles; exact fixed/max/protected sums, no stage+selected double charge, no write/close effect in Drop. AllocationEntry64<=8192 and all fixed codec state inventoried. No heap-free/RSS/fallocate claim.

The descriptor rehashed semantic table contains29 cases: magic xor1, version2, mode2, length4095, DB firstbyte xor1, timeline2, profilegen2, profiledigest xor1, filecounter2, mappedbytes+1MiB, allocatorID2, allocatorgen2, filehighwater2, objecthighwater2, mapgen2, selectorgen2, refkind3, refslot1, refindex1, refgen2, reflength224, refdigest xor1, allocationhash xor1, fixedbytes+1, maxowned+1, protected+1, bankbytes+1, extentcount2, planversion2. Every reserved byte in ranges[7,8),[12,16),[146,148),[164,176),[288,4064) is independently damaged and rehashed:3795 cases. Generic ref reserved fields are not silently ignored. Version/mode/planversion mismatches are UnsupportedFormat; magic/length/reserved/checksum and fixed geometry/high-water/resource values are Corrupt; DB/timeline/profilegen/profiledigest/refdigest/allocationhash against the trusted plan are IdentityMismatch. Refkind/slot/index/generation/length are fixed plan geometry and therefore Corrupt. Unknown error combinations are not assigned a stronger precedence than length→envelope/checksum→version/mode→reserved/fixed fields→trusted identity binding.

The descriptor encoder consumes an internally constructed BootstrapPlan with private fields, not arbitrary caller-supplied digest/owner/high-water values. Plan construction uses actual Profile plus the existing codecs to derive the exact initial root/page references before effect. Its output-short/invalid-profile cases preserve the entire output. Decoder mismatch tests alter a valid encoded input or use a different valid trusted plan; they do not require an encoder to divine that an arbitrary hash has no corresponding file. The plan exposes no resource/READY capability and no resume constructor.

## Native suite:3 tests and separate compiler8

`bootstrap_native_profiles_and_lease`: all3 profiles with true Linux entropy, hash expected objects by substituting the actual nonzero DB into independent offset construction; exercise shared reader/buffer and a valid opaque WAL append/sync; retained inode/length/readback; separate writer blocked until session and all returned owned files are dropped. No ordinary DB recovery claim.

`bootstrap_native_fault_images`: error-before/error-after at all338 new logical mutations=676 fresh fixtures using actual Linux effects; each reports exact index/name/phase and external file hashes/sizes. Retain actual partial directories and raw logs. Candidate-reported witness alone is insufficient: controller verifies actual image with the independent prefix expectation. Native pre-effect failures and after-effect errors are not power loss. Keep recovered current-build DEV2 integration separately.

`bootstrap_native_kill_images`: after-effect hooks15,16,18,82,84,85,213,215,344,345,348,349,350 =13 cuts×3fresh repetitions=39 kills. Parent observes exact hook then SIGKILL/waitpid, reads actual directory bytes, and requires new fresh create refusal with mutation0. Product before/after readback is not a reopen/resume endpoint. These are kernel-pagecache observations; bounded shadow image alternatives remain separate. New cuts cover descriptor durable-name barrier, first allocation write, root/chunk and second bank's first write, each complete file and selector publication. Old genesis kill24 remains separate.

Compiler probes are8: baseline1, positive2 (shared reader+concurrent WAL with W:Send/File:Send but no W:Sync bound; shared buffer+concurrent WAL), negative5 (drop session while reader live E0505; reuse slot/root/scratch mutable borrow while reader live E0499; use moved raw writer E0382). Probe source/expected primary line spans are generated and hashed before compiling; exact code+primary probe-file span required. Baseline success and no unrelated errors required. No unsafe auto-trait implementations. Existing buffer7/WAL4 probes remain regressions; their results are not the new8.

Both positive probe bodies are universally checked with only W:WriterNamespace+Send and W::File:Send. Additionally the real-thread engine writer fixture contains Cell<()> (Send, !Sync), making an accidental W:Sync requirement fail actual thread compilation rather than relying only on a Sync fake/native type. This marker has no I/O behavior. Runtime fault injection/observation helpers are cfg(test) only, with narrow access to existing frame state/WAL state; no production raw writer/inner/clear-fence API is introduced.

Readback before-error causes: observations1/2/4/5/6/8/10/14→Bootstrap Phase::Readback, CallerMustResolve, Cause::PageIo(Namespace) for reader opens6/8/10/14 and Cause::Namespace for bootstrap-owned1/2/4/5; observations3→Cause::Io;7/9/11/12/13/15→Cause::PageIo(Read). Typed descriptor mismatch→Cause::Control; exact Profile/SelectedControl mismatch→Cause::IdentityMismatch; descriptor/bank1 length/identity mismatch→Cause::LengthMismatch/IdentityMismatch; page-reader open lengths→Cause::PageIo(LengthMismatch); final allocation digest mismatch→Cause::IdentityMismatch (checked before strict cell decode). Every panic is separately observed by catch_unwind in the harness, never mislabeled a typed Result. Shared construction Busy/Fenced/Poisoned uses the page error variants and releases its acquired lease on every failure; ordinary read failure does not set the common flag.

The image convention excludes LOCK from product-image hash lists but separately requires exactly one permanent zero-length regular LOCK with the writer lease. Unknown additional files are never silently removed or ignored by the installation/inventory checker.

Native hook deadline is60seconds per freshly launched worker; after SIGKILL/error completion the parent requires waitpid/exit within10seconds. Runtime rendezvous deadlines remain5seconds. Timeout is failure with partial evidence retained; automatic retries do not erase it. Native fault workers use a single canonical frozen binary and per-case data directories, not full binary copies per case. External image reads/hash receipts preserve every partial namespace; completed compiler/build target cleanup must not remove these fixtures or the canonical binary. The descriptor/image model's profile1-only denominator remains explicit, while all3 profiles are exercised by normal installation/native tests.

The exact negative-control identities are r5-checker-negatives.json (trace7, model-image7, native receipt11 including the two LOCK variants). The exact alternate full-state fixture hashes are r5-alternate-inputs/manifest.json. Compiler input sources and primary spans are r5-compiler-inputs/manifest.json. Their freeze manifest binds these files and r6-proposal.md/r6-verification.md. A final implementation can change helper decomposition but cannot silently change these inputs, denominators, failure semantics or expected results; a necessary change requires a separately recorded pre-execution D2 revision.

The exact native image/mutation checker must have negative controls for missing index, duplicate hook, premature selection, omitted directory sync, wrong bytes/hash, omitted file, extra file, accepting fallback, premature READY claim, wrong binary/source and swallowed failure. Each target must actually reach its hook within bounded timeout; timeout/missing/unexpected effect fails. New independent durable model/checker negative controls: select-before-data-sync, missing descriptor-directory barrier, omitted bank1 write, fallback-to-bank1, treating publish as directory durable, double-counting stage+selected, ignoring a mutation index. Their expected rejection must be fixed before product runs.

V2 combines these new suites, existing page/buffer/WAL/semantic suites, full current DEV3 leaf, DEV0/1/DEV2 regression and same-source DEV2 native99 recovery. Final clean source/export fingerprint and independent D1 review precede commit. Document verification runs only at push, not while editing or committing. No performance/V3/DS01–06/S1 success is inferred.

The resealed allocation case keeps the full page identity and64-entry cell semantically valid: upper10144 +header64 +entry1 offset96 +generation offset8 =10312. Write u64LE2 there and rehash the final page checksum. Raw page and allocation decoders must accept it; only the initial full-page hash binding rejects it. No identity/checksum error may mask that comparison.

The shared lower fence owner will be storage/session_fence.rs, consumed by wal_io/page_io/read_buffer; it must not depend on the higher bootstrap child. Owner contracts affected are dev3-wal-io.md, dev3-page-io.md, dev3-read-buffer.md, dev3-control-format.md, dev3-resource-bounds.md and the new fresh-storage owner contract. Register actual new source paths only when implementing, never as placeholder modules.

## 채택·실행 상태

[독립 D2 원본](../evidence/dev3-storage-session-d2-xadb-20261021-r2.json)의 decision ID는 `DEV3_FRESH_STORAGE_SESSION_R6_D2`, status는 ACCEPTED다. [동결 입력 원본](../evidence/dev3-storage-session-inputs-xadb-20261021-r6.json)은49개 입력과 aggregate `160686f413309ae84503372d7cb0a554cccfe9c24ec7feccdbdf62034c0977a7`를 보존한다. aggregate는 JSON의 files row 순서(Path component 정렬)로 `path + NUL + sha256 + LF`를 연결한 SHA256이다. 원본의 상대 경로·검토 identity는 변경하지 않았다.

원래 전체 검토 위치는 `artifacts/dev3/storage-session-independent-review-20261021/r2-d2-review.md`이며 SHA256은 `13a262e9dae58397d195f43fab5749fd968e2f094457b9ae880338cd827b4782`다. 검토 입력 기준 commit은 `50525f17f4989da82db71b2411ff24d132456b3e`, 제품 source는 `f08f01d72f06a9578a129927a7a17dfe2450734e8fbd86dcfeb8c11e4da4d515`다. 이 채택 등록에서는 제품 source·binary·실행 결과를 갱신하지 않는다.

책임·위임은 기존 DEV3 완료 요청과 개발 운영 방침 D2에 따른다. r3/r5의 이름·실패 정리·자원 누락을 r6이 대체하며, canonical stage 이름·healthy claim 정리·매 reader의 full Profile/SelectedControl 결합·C/L 보존·실제2개 추가File의 단계별 ledger를 반영했다. 독립 검토자의 자기 판정 원본은 그대로 보존한다.

재검토 조건은 live data/FILEMAP 변경 도입, reader 수 확대, 파일·scratch/owner 크기 증가, 형식/ID 발급 변경, fence/Drop/오류 순서 변경, 고정 검증 분모·독립 oracle 변경이다. 상세 구현 및 same-source runtime12/native3/compiler8·leaf/lower/current-build native99·독립 D1은 아직 NOT_RUN이다. CURRENT/DWB·dirty page·거래·정상 recovery·READY·F05·DEV3 전체 완료는 이 채택으로 승격하지 않는다.

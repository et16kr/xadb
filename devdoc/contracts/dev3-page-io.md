# DEV3 선택 FILEMAP과 page 읽기

2026-10-21 · **D3-PAGE-IO-001 port·reader 계약·구현 D2 독립 검토 수용 / 같은 소스의 leaf·하위 회귀·현재-build 복구 PASS**

[디스크 소유 계약](disk-storage-design.md#page) · [control 표현](dev3-control-format.md) · [page 표현](dev3-page-format.md) · [자원 계산](dev3-resource-bounds.md) · [재개 소유](../implementation/dev3-implementation.md#resumed-20261021)

## 1. 범위와 결정

작업·계약·통합 담당은 `/root`다. 이 leaf는 기존 표현을 실제 읽기 효과로 연결한다. DB 생성·READY·buffer·할당·WAL·DWB repair·복구·transaction admission은 후속 소유자의 책임이다. 성공은 선택 FILEMAP의 전체 표현 검증과 요청한 page envelope의 검증이며, tree 정렬·undo graph·SQL 결과의 검증이 아니다. 소비자는 해당 cell decoder와 가시성 검사를 추가한다.

기존 writer lease를 가진 단일 owner가 namespace와 control 선택을 고정한 동안 읽는다. root/chunk 전부를 RAM map으로 복제하는 대안 대신 root16 KiB와 재사용 scratch16 KiB를 caller에게 빌린다. FILEMAP 전체 chunk를 처음 한 번 검증하고 각 page 요청에서 해당 chunk를 다시 읽는다. 해시만 검증한 첫 chunk에서 곧바로 page를 반환하는 대안은 뒤 chunk의 중복·정렬·누적 길이 결함을 놓치므로 사용하지 않는다.

## 2. 열린 파일과 metadata의 결합

foundation `WriterNamespace::open_existing_with_metadata(name, mode)`를 추가한다. 반환은 `(Self::File, FileMetadata)`이며 metadata는 **반환한 handle**에서 관측한 device/inode와 길이다. directory 이름을 별도로 stat한 결과를 이 값으로 대체하지 않는다. 기존 `open_existing`의 의미는 유지한다. 새 method는 default 구현으로 before/after path stat을 합성하지 않고 각 adapter에서 실제 handle을 검사한다.

Linux에서는 `openat`의 기존 no-follow·regular·single-link·동일 inode 검산을 유지한 후 열린 handle의 metadata를 반환한다. 길이는 signed64 offset 영역 이내여야 한다. metadata 관측 뒤 다른 owner의 rename/resize까지 막는 snapshot은 아니므로 engine은 lease 및 내부 mutation 조정으로 선택/길이를 고정한다. platform은 비협조적 외부 프로세스의 임의 변경에 대한 sandbox가 아니다.

fault/performance adapter는 실제 하위 method를 호출하고 기존 target·Open phase·writer ownership을 같은 방식으로 감싼다. DEV2 cache-loss 시험용 adapter도 반환한 file identity의 실제 현재 길이를 사용한다. 조회 실패는 Unchanged이며 자동 create/truncate/sync/rename·재시도를 하지 않는다. 기존 plain open을 호출하는 DEV2의 호출 순서와 fault 분모는 유지한다.

이 API는 개발 단계의 effect port 확장이고 기존 wire bytes는 바꾸지 않는다. 새 dependency·unsafe·OS primitive가 필요하지 않다. 독립 검토 후 foundation/platform/worker와 시험 adapter를 root가 순차 수정한다.

### 2.1 Port의 별도 공급과 고정 시험

§2의 API를 먼저 공급하고 §3 이후 reader와 분리 검증한다. root는 foundation/platform namespace, engine-worker fault/performance adapter, engine의 cache-loss 시험 adapter만 수정한다. native 시험은 새 `crates/platform/tests/namespace_metadata.rs`에 두며 기존 거대 `contracts.rs`에 다른 책임을 추가하지 않는다. worker의 같은 책임 시험은 기존 `fault_tests.rs`에 추가한다. 신규 파일은 실제 구현 때 빌드 배치에 등록한다.

- `opened_metadata_binds_exact_handle_and_length`: ReadOnly/ReadWrite 두 mode × 길이0/17/16,384의6개 open. 반환 identity·길이와 실제 handle bytes 대조.
- `replacement_keeps_old_handle_metadata_and_lease`: FILEMAP/DWB 두 selector 각각 이전 handle/metadata를 유지한 채 새 inode·다른 길이로 교체하여 이전/새 bytes·identity를 구별. writer를 drop해도 보유 file이 lock을 유지하고 마지막 file drop 뒤 새 writer 획득.
- `opened_metadata_rejects_invalid_names_and_types_without_effects`: 두 mode × missing/symlink/hardlink/directory/LOCK의5개=10거절. 원본/목적지 bytes 보존.
- `dev3_open_metadata_preserves_selector_roles`: worker의 새2 selector 준비 이름 × 두 open mode × Open/Other phase=8개. metadata는 실제 파일의 identity/길이, 이후 SyncBefore의 controller witness는 해당 selector target을 보존. 기존 plain-open 시험은 변경하지 않음.

performance adapter의 새 경로도 실제 하위 호출 한 번을 `Kind::Open` 한 건으로 기록하며 성공/오류를 그대로 전달한다. `open_metadata_counts_one_success_or_failure`는 실제 Linux에서 존재/누락 파일 각 한 번을 호출하여 snapshot의 open attempts2/successes1/failures1과 다른 effect0을 검증한다. 열린 파일의 read와 writer Drop 이후 lease 유지도 확인하되 counter 검산은 read 전에 한다.

## 3. 선택과 읽기 절차

1. caller가 writer lease와 서로 다른 root/scratch 각16 KiB를 제공한다. reader는 writer의 배타적 mutable borrow를 수명 동안 보유한다. 같은 owner는 이 동안 FILEMAP 선택·container 재작성·data resize·page 재사용을 하지 않는다. 다른 outstanding file handle을 통해 우회 변경하지 않는 것은 상위 coordinator의 의무다.
2. `d3-profile`을 read-only로 열고 handle 길이가 정확4,096인지 확인한다. scratch에 읽어 기존 bootstrap decoder와 caller가 가진 optional 기대 namespace로 검증한다. 즉시 handle을 해제한다.
3. `FILEMAP`도 같은 방법으로 정확4,096 bytes를 읽고 profile에 결합하여 selector를 검증한다. selector handle을 해제한다. 누락·손상 시 다른 slot이나 directory scan으로 대체하지 않는다.
4. selector의 정확 slot에 해당하는 FILEMAP container만 read-only로 열고 handle 길이2,113,536을 확인한다. offset0에서 reference의 정확 논리 길이만 root buffer에 읽고 같은 exact slice의 digest/bytes로 root를 검증한다. slot padding은 읽거나 decoder에 전달하지 않는다.
5. root의 모든 descriptor를 index 순서대로 방문한다. offset `(index+1)*16,384`에서 해당 reference의 정확 논리 길이만 scratch에 읽고 그 slice를 `FileMapCursor::accept`로 검증한다. 마지막 `finish`가 성공해야 reader를 반환한다. 빈 FILEMAP도 전체 검증 후 성공하며 그 안의 모든 page 요청은 누락으로 거절한다.
6. 요청 PageIdentity.database를 먼저 검증 profile DB와 대조한다. 그 뒤 root에서 file을 찾고 해당 chunk의 exact logical bytes를 다시 읽어 검증한다. 정확 FileId의 entry가 있고 `page_no*16,384+16,384 <= mapped_bytes`인지 checked arithmetic으로 검사한다. Live/Retained 둘 다 표현상 읽을 수 있다. Retained를 foreground 접근 가능으로 승격하는 것은 상위 객체/복구 소유자의 일이므로 이 reader가 결정하지 않는다. allocation metadata page의 기대 owner/generation/incarnation은 FILEMAP entry의 `metadata_identity`에서 도출한다. 일반 page의 기대 owner/incarnation은 신뢰한 allocation/tree reference가 제공해야 하며, 이 leaf가 그 신뢰 경로를 완성했다고 하지 않는다.
7. entry의 canonical data 이름을 read-only로 열고 반환한 handle 길이가 mapped bytes와 정확히 같은지 확인한다. 선택되지 않은 성장 tail은 이 정상 page-read leaf가 수락하지 않는다. 실제 crash growth·partial initialization의 처리 및 길이 정리는 recovery/allocator의 별도 의무다.
8. page 전체16 KiB를 caller output에 읽고 기대 DB/FileId/page/incarnation/owner/kind/level로 공통 envelope를 검증한다. handle을 닫은 뒤 검증한 view를 반환한다. checksum·길이·I/O 실패 시 output은 유효하지 않으며 caller가 폐기한다. 이전 output 보존을 약속하지 않는다. reader의 root/선택 상태는 그대로다.

선택 generation을 반환값/receipt에 남기되 이를 database commit sequence로 해석하지 않는다. 지금 반환하는 page bytes는 caller 소유이며 PageGuard·content latch나 transaction snapshot이 아니다. 실제 buffer 소비에서는 위 내부 배타 소유를 coordinator·pin 수명으로 대체하는 별도 계약 검토가 필요하다.

## 4. F04/F05 제한된 읽기 profile

이 leaf는 쓰기·sync·성장·할당·undo/CLR 예약을 소비하지 않는다. 따라서 opcode별 forward/rollback 표를 대신하지 않으며 후속 mutation 통합을 승인하지 않는다.

| 자원 | 상한·소유 |
| --- | --- |
| control workspace | root16,384 + scratch16,384 =32,768 bytes, caller 소유. profile/selector/chunk scratch는 같은 공간 재사용 |
| 반환 page | caller의 별도16,384 bytes; buffer 소비 때 해당 frame에 귀속하고 system scratch로 중복 계산하지 않음 |
| 파일 handle | open 준비 중 최대1, reader 보존 FILEMAP1 + 요청 data1 =최대2. namespace의 기존 directory/lock handle은 별도 기존 lease 비용 |
| 검색 | 최초 최대128 chunk×64 entries=8,192 entries. 요청당 root 최대128 descriptor와 chunk 최대64 entries. 전체 entry Vec·directory inventory 없음 |
| 읽기 bytes | 최초 profile4,096+selector4,096+root의논리길이≤12,512+각chunk논리길이의합(각≤6,304, K≤128). page 요청은 chunk논리길이≤6,304+page16,384. short read 재호출은 총 완료 bytes 범위 유지 |
| disk/WAL/reserve | 신규 소비0. 읽기만으로 BOOT_RESERVE·admission 성공을 주장하지 않음 |

short/Interrupted read는 foundation의 기존 `read_exact_at` 규약을 따른다. EOF·invalid count·I/O 오류는 성공으로 바꾸지 않는다. 모든 read/write/sync/resize/namespace 호출을 관측하는 시험 adapter로 실패 cut마다 write 계열 호출0을 확인한다. 실제 Linux reopen 시험과 synthetic read fault의 범위를 분리한다. 새 read fault를 기존 native selector write cut의 PASS로 대신하지 않는다.

## 5. 구현 전 고정 시험

| 묶음 | 필수 입력·기대값 |
| --- | --- |
| handle metadata port | 실제 Linux create→resize→open, read-only/read-write 모두 반환 inode/길이 확인. 이전 handle을 유지한 채 selector를 새 inode로 교체했을 때 이전/새 handle의 값 분리. missing·symlink·hardlink·directory 거절 및 bytes 보존 |
| 선택 전체 검증 | K0/1/2/128, N0/1/64/65/8192의 표현. 8,192파일 전체를 실제 생성한 용량 인수로 주장하지 않음. 늦은 chunk 손상·누락·잘린 slot·digest/namespace/generation·순서/누적값 오류는 첫 page 반환 전 거절 |
| exact file lengths | profile/selector 각4095/4096/4097, container2,113,535/2,113,536/2,113,537, target mapped−1/동일/+1. 모든 잘못된 길이 거절; checksum이 맞는 prefix를 큰 파일에서 받아들이지 않음 |
| page identity | 정상 page와 DB/file/page/incarnation/owner/owner-generation/kind/level 각 불일치; 마지막 page·범위 밖·곱셈 overflow; 없는 file; single-byte damage·EOF 거절 |
| 실제 I/O | Linux fresh fixture의 profile/selector/container/data를 준비한 뒤 새 reader로 reopen하여 고정 독립 page literal과 전체 bytes 대조. 선택하지 않은 정상 container가 있어도 선택된 손상을 거절 |
| 읽기 fault | 각 profile/selector/root/chunk/data 경계의 read-before 오류, short1/17/4096, Interrupted 후 정상, 일부 성공 후 EOF, invalid count. 실패 후 mutation0·handle 반환·별도 정상 요청 재시도 시 기존 root/선택 유지 |
| 자원 | 계수 adapter에서 최대2 file handles·고정 읽기 분모·drop cleanup. 많은 요청에서 보존 handle 수가 증가하지 않음; 메모리 allocator 전체 계측으로 과장하지 않음 |

독립 reviewer가 상세 API/diff/원래 반례/실제 결과를 검토한다. source/binary/log 지문과 같은 source의 DEV0/1·DEV2 회귀/복구를 보존한다. 상태를 LOCAL_VERIFIED로 올리더라도 전체 DS01–06/PF09/S1은 별도다.

§6에서 내부 API/error/lifetime signature, 최초 검증과 요청 재읽기의 개별 fault 분모, engine의 private storage와 실제 Linux worker를 연결할 좁은 시험 조립 경계를 구체화했다. `/root/dev3_selector_review`가 원문 hash `c1d223e7aa8c9ff76d00072823ed0a3ead0c6f6d8088865ad42751df91da491f`를 독립 검토하여 구현 착수를 수용했다. 실제 구현 검토·통합 실행은 별도다. engine→platform 의존이나 정상 DB READY API를 시험 편의 때문에 추가하지 않는다.

## 6. Reader F02/F03/F04 구체화

아래는 §2 port와 별도로 독립 검토한 reader 계약이다. 실제 구현은 `storage/page_io.rs`·`page_io_tests.rs`, 시험 조립은 `crates/engine/src/testing.rs`와 `crates/testing/engine-worker/tests/page_io.rs`에 둔다. root가 단일 작성하고 모듈·Cargo·runner·색인도 순차 연결한다.

```rust
PageFileReader::open(
    writer: &'a mut W,
    expected: Option<Namespace>,
    root: &'a mut [u8; PAGE_BYTES],
    scratch: &'a mut [u8; PAGE_BYTES],
) -> Result<PageFileReader<'a, W>, PageIoError>

reader.read_page(
    expected: PageIdentity,
    kind: PageKind,
    level: u8,
    output: &'b mut [u8; PAGE_BYTES],
) -> Result<PageView<'b>, PageIoError>
```

reader가 root/선택 container/검증 profile/selector generation/namespace borrow/scratch를 소유한다. 반환 view는 output만 빌리며 reader를 빌리지 않는다. 다음 read와 별개인 caller output의 불변성만 보장한다. kind가 허용하지 않는 level은 읽기 전 InvalidRequest, 읽은 page의 다른 level은 IdentityMismatch다. Allocation kind 요청은 page number가64의 배수이고 FILEMAP의 `metadata_identity`와 기대 identity가 같아야 한다. 일반 page 요청의 owner 신뢰는 상위 소유자에 남긴다.

반대로64의 배수인 metadata 위치에 non-Allocation kind를 요청하면 InvalidRequest다. 정상 Allocation page0과 정상 RowInternal level1/15를 포함하여 합법 요청을 전부 거절하는 결함도 검출한다. page의 expected identity 불일치 시험은 해당 mapping과 파일을 정상 준비한 뒤 header identity만 다르게 하여 missing/open 실패가 검사를 가리지 않게 한다.

`PageIoError`는 Namespace(원래 NamespaceError), Read(원래 foundation Error), Control(원래 ControlError), Page(원래 PageError), InvalidRequest, IdentityMismatch, LengthMismatch, MissingMapping, OutOfBounds를 구별한다. 모든 실패는 DB bytes 불변이며 output은 폐기한다. 이미 실패한 constructor는 file handles를 전부 반환한다. 생성 후 요청 오류는 선택 root·profile·container를 보존한다. OS 오류와 logical 누락을 같은 clean empty page로 변환하지 않는다.

native 조립은 engine의 새 **default 제외 `test-support` feature**에서만 `testing` façade를 노출한다. façade는 공개 primitive request를 내부 typed identity로 검증해 위 production reader를 호출하고 결과를 고정 오류 범주·selector generation·cell 수로만 투영한다. caller output에 원본 page bytes를 반환하며 정상 Engine/open/READY capability를 반환하지 않는다. worker의 dev-dependency에서만 이 feature를 켜고 LinuxDirectory를 주입한다. engine은 platform/test crate에 의존하지 않는다. feature·fixture·source identity는 실행 receipt에 기록하며 façade를 default 제품 API로 주장하지 않는다.

### 고정 입력과 fault 분모

기존 독립 `dev3-page-v1-cells.bin`을 정상 page 정답으로 재사용한다(DB=0x11×16, file=4,294,967,298, page3/inc4, owner5/gen6, RowLeaf/level0/LSN7). 새 Python fixture generator는 control 계약의 정수 offsets·hash 규칙에서 profile1/gen19/timeline17, FILEMAP slot0/gen23, selector gen29, FileId epoch1/counter2부터 연속65개를 작성한다. 각 mapped length는1 MiB이고 allocator owner5/gen6이며 page 정답은 바꾸지 않는다. 두 chunk와 root/selector/profile의 **논리 bytes만** fixture로 보존한다. normal inventory0/1/64/65/8192는 같은 raw offset 규칙으로 시험에서 구성하며 production decoder/encoder를 oracle로 호출하지 않는다. 65개 mapping 중 target file 하나만 실제 page fixture로 만드는 native 시험은 전체 DB audit/용량 인수가 아니다.

고정65-entry fixture의 read 지점은 Profile, Selector, Root, InitialChunk0, InitialChunk1, LookupChunk0, DataPage의7개다. 각 지점에 다음8개 action을 한 번 주입한다: BeforeError, Short1, Short17, Short4096, InterruptedOnce, PrefixThenEof, PrefixThenError, InvalidCount. short action은 첫 call에서 `min(cap, requested−1)`을 반환하여 반드시 실제 short가 발생하도록 하고 이후 같은 logical read의 남은 calls에 cap을 적용한다(고정 최초 길이는 모두256 이상). 실제 첫 반환량이 요청량보다 작았다는 witness를 요구한다. Interrupted는 한 번 뒤 정상으로 진행한다. Prefix는 실제1 byte 성공 뒤 다음 call의 EOF/error다. InvalidCount는 요청 길이+1을 반환한다. **56셀=정상28·명시 오류28**이며 첫 read가 끝나지 않았는데 다음 logical phase로 전진하지 않는다. 최초5지점 실패는 reader 생성 거절, 뒤2지점 실패는 요청 거절 후 fault 제거·동일 reader 재읽기 성공을 확인한다. 모든 셀에서 mutation0·최대2 handles·마지막 drop 후0을 검사한다.

| 고정 시험명 | 분모·검산 |
| --- | --- |
| `selected_map_reads_independent_page_and_all_declared_chunks` | N0/1/64/65/8192의5개 inventory, 정상 target의 전체16KiB 일치·cell 수와 선택gen29. N0는 MissingMapping. 전체 chunk와 lookup chunk의 exact offset/length 대조 |
| `selected_map_rejects_late_damage_and_never_scans_fallback` | selector누락·checksum·잘못된profile·root손상·첫chunk손상·마지막chunk손상·chunkEOF·container누락8개. slot1 header와 전이 reference/digest까지 정상인 비선택 slot이 있어도 거절, data read0. 같은 slot1을 선택한 별도 정상 읽기로 fallback fixture의 유효성을 확인 |
| `selected_map_and_data_require_exact_handle_lengths` | profile/selector/container/data 각각 길이−1/동일/+1 =12개. 동일4개 성공, 나머지8개 명시 LengthMismatch |
| `page_request_binds_profile_owner_generation_kind_and_bounds` | 기대DB/file/page/inc/owner/owner-generation/kind/level8개 불일치, 누락mapping·범위밖·u64 offset overflow·Allocation page3/잘못된metadata owner·non-Allocation page0, page byte손상/EOF. DB mismatch는 data read 전 거절. 정상 Allocation page0, 정상 internal level1/15, internal0/16·leaf1 거절, 실제 internal level1에 기대level2 불일치, 마지막 유효 page와 Retained mapping을 별도 검사 |
| `every_read_fault_preserves_selection_and_bounded_handles` | 위7×8=56셀, 정상28·오류28, 정확 read phase 도달·요청 재시도·mutation0·handle peak/drop |
| `native_selected_map_reads_and_rejects_selected_damage` (worker) | 실제 Linux 새 fixture reopen 정상·selected chunk 손상·data 길이±1·다른 기대DB·정상 slot1 선택의6개. 양쪽 bank는 각 slot header와 전이 reference/digest까지 일치한다. 동일 production reader·고정 literal 비교, 실제 mutation은 fixture 준비만 수행 |

내부 source의 수락/거절 경계를 validator의 정상 결과로 계산하지 않는다. slot 생성이나 native test의 파일 write는 fixture 준비이며 실제 DB 생성·내구성 인수로 집계하지 않는다. reader의 메모리 상한은 구현의 고정 buffer/정적 타입과 계수 adapter의 handle 관측으로 확인하며 allocator 전체 OOM 복구·OS device read량으로 확대하지 않는다.

2026-10-21 구현 검토에서 bank0 bytes를 bank1 이름에 복사한 초기 시험 fixture가 정상 비선택 bank가 아니라는 반례를 확인했다. 위 bank1 정상 대조를 추가하고 기존 거절8개·native5개를 유지한 채 native 정상1개를 추가했다. 제품 reader 의미나 fault56셀을 축소하지 않는다. 이전 로그는 `artifacts/dev3/page-reader-r0/`에 보존한다.

## 최초 저장 세션의 후속 D2 계약

후속 [최초 저장 세션](dev3-storage-session.md)은 namespace를 변경하지 않는 owner의 수명과 atomic 한 reader lease를 통해 선택을 고정한다. 각 open 때 full Profile/SelectedControl을 owner 기대값에 대조하고 namespace mutex는 파일 open 동안만 보유한다. 기존 exclusive reader API는 유지한다. 이 shared 경계는 D2 채택·구현/실행 NOT_RUN이다.

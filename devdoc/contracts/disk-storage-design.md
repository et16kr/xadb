# DEV3 — 디스크 저장·인덱스 상세 설계

2026-10-18 · **DEV3-DESIGN-001 D2 기본 설계 ACCEPTED / leaf byte vector·실행 profile 미동결 / 미구현·제품 시험 NOT_RUN**

[구현·검증 계획](../plans/dev3-implementation-plan.md) · [소스 조사 증거](../evidence/dev3-source-research.md) · [커널 계약](implementation-contracts.md) · [DEV2 설계](dev2-kernel-design.md) · [복구 계약](recovery-logging-design.md) · [인수 항목](disk-test-matrix.json)

DEV3는 DEV2의 SQL 없는 durable transaction을 실제 디스크 행·catalog·인덱스와 제한된 버퍼로 확장한다. 이 문서의 API·형식은 구현 설계이며 현재 호출 가능한 기능이 아니다. 설계 준비는 DEV2 완료 전 가능하나 production 구현은 [단계 진입 규칙](../plans/stage-completion.json)의 최소 선행 요건과 leaf 선행 조건을 충족해야 한다. 현재 DEV2는 P0/K0 일부 구현이며 WAL·recovery가 없다.

<a id="scope"></a>
## 1. 범위와 유지할 계약

| DEV3 필수 범위 | 유지하거나 후속 단계에 남길 경계 |
| --- | --- |
| DISK 행·catalog·PK/unique/nonunique B+tree, point/range/전체 scan | Bool/Int64/Utf8/Binary/NULL, 비파티션 table, 고정 열 schema |
| RowUID 해석, overflow row, bounded page/metadata/locator cache | 전체 행·locator·파일 inventory를 RAM에 강제 상주시키지 않음 |
| 다중 table의 row/index/catalog 변경을 한 commit으로 공개 | DEV2의 단일 L0, coarse serializable validation, statement rollback |
| 실제 파일 I/O·crash/reopen·exact-cut checkpoint/backup/restore | ACK/UNKNOWN·STABILIZE·CURRENT/WRITER·writer lease 의미 유지 |
| 공간 회수·유지보수·buffer보다 큰 DB·PF09 측정 | PART/NG, SQL, online DDL, L1/L2, 복제/PITR, 고급 압축은 후속 |

제품 저장 엔진은 DISK 하나다. `storage::volatile`은 DEV2의 임시 시험 substrate다. DEV3 시험은 production disk 경로를 사용하며 volatile 결과를 S1 증거로 승계하지 않는다. 이전 사내 DB 변환은 요구하지 않고 새 fixture를 만든다. DEV3 feature/format을 모르는 build는 데이터 변경 전에 거절한다. 현재 build의 복구 의무는 [버전 정책](../policies/version-compatibility-policy.json)에 따라 유지한다.

<a id="choice"></a>
## 2. 구조 선택과 조사 반영

초기 선택은 **slotted page를 사용하는 불변 COW B+tree + sealed page pack + 단일 commit coordinator**다. 새 페이지와 파일 이름을 영속화한 뒤 L0 commit frame을 sync하고 전체 view를 공개한다. 이는 기존 DISK data-before-commit 계약의 구체화다.

| 대안 | 선택 이유·보류 이유 | 근거 |
| --- | --- | --- |
| COW B+tree | snapshot이 가리키는 page를 수정하지 않아 reader와 구조 변경을 분리. point/range를 같은 구조로 제공. 경로 복사·공간 증폭 비용은 측정 필요 | 기존 XADB 복구 계약; R1/R4/R7의 page/tree 책임 비교 |
| WAL-first in-place B-tree | data sync를 commit마다 요구하지 않는 이점. undo/pageLSN/CLR·torn page 대응·동시 구조 변경 계약이 새로 필요해 초기 선택에서 제외 | R2/R3의 WAL-before-page, R7의 flush/recovery 경계 |
| LSM | 쓰기 batching의 비교 후보이나 compaction·range read·secondary index 원자성의 초기 복잡성이 큼. R6의 manifest/파일 수명 원칙만 참고 | R6 |
| 전체 행 배열·전체 locator 상주 | 작은 oracle에는 유용하나 DB 용량과 RAM을 분리하는 DS03/RC-T14를 충족하지 못함 | DEV2 초기 표현의 한계 |

R1의 pin/내용 lock 구분, R3의 교체 불가 상태, R5의 예약·동시 load 재검사는 buffer 수명 설계에 반영한다. R4의 reader 경계와 checkpoint 보존, R6의 manifest sync와 선택 실패 시 보존은 retention 시험에 반영한다. R7에서는 디스크 index의 영속 본체, page header·slot 경계, flush 중간 상태의 보존 책임을 확인했다. 원래 소스의 알고리즘·수치·성능·검증 수준을 XADB의 보장으로 가져오지 않는다. 실제 revision·읽은 범위·채택/기각 이유는 조사 증거의 정본이다.

<a id="ownership"></a>
## 3. 모듈·API·root 소유

기존 `xadb-engine` 내부에 실제 구현 시 `storage::disk` 책임을 추가한다. `page`, `tree`, `buffer`, `pack`, `reclaim`은 책임 구분이며 빈 파일·crate를 미리 생성하지 않는다. catalog는 schema와 index 정의를, transaction은 validation·statement rollback·공통 commit을, durable은 WAL·recovery·checkpoint 선택을 소유한다. storage는 준비된 page/root와 dependency 목록을 반환하며 스스로 commit·ACK하지 않는다. platform 효과는 foundation port를 통해 조립한다.

```text
transaction coordinator
  ├─ catalog: table/index 정의와 root descriptor의 COW 변경
  ├─ storage::disk: row tree / index tree / page pack 준비
  │                   └─ bounded buffer → foundation FileIo/namespace port
  ├─ durable: dependency 확인 → L0 append/sync
  └─ publication: Arc<CommittedRoot> 하나 교체
```

`CommittedRoot`는 DB/timeline, q, catalog-root와 파일 형식 식별자만 담는 작은 descriptor다. catalog는 디스크 B+tree이며 TableUID별 schema·row-root·index 정의/root를 저장한다. table 이름 조회도 catalog 내 별도 key namespace로 제공하며 이름→TableUID와 객체 레코드를 같은 catalog root로 바꾼다. 이름 cache는 `(catalog-root, name)`에 종속되고 예산을 갖는다. table 수 증가에 따라 모든 root·schema를 복사하거나 상주시킬 필요가 없다.

**row tree가 locator의 영속 정본도 겸한다.** `TableUID + RowId → row leaf cell → inline row/overflow` 경로로 해석한다. 별도 전체 RowId→물리주소 배열을 두지 않는다. secondary index leaf는 `(encoded key, RowId)`이며 row 주소를 담지 않는다. locator cache는 `(DB/timeline, table generation, row-root, RowId)`를 key로 하며 miss 때 row tree를 읽는다. hit도 page identity·slot RowId를 확인하고 불일치는 무조건 다른 행으로 해석하지 않는다. root 교체 뒤 old-root cache entry는 old snapshot에서만 유효하다. DS02의 locator 원자성은 row-root 교체와 cache namespace 검사로 충족해야 한다.

| 계획 typed API | 의미·수명 |
| --- | --- |
| DEV2 `read/scan/execute_statement/commit` | 공개 RowUID·오류 의미 유지. disk 조회와 private overlay를 합침 |
| `create_index(table, IndexSpec)` | 독립 DDL. DB admission 차단·기존 사용자 drain 후 전체 table 검증·build, 하나의 원자 commit |
| `drop_index(handle)` | 독립 DDL. 새 catalog에서 제거; old snapshot의 index root는 pin으로 유지 |
| `index_lookup/index_range(handle, bounds, limit, cursor)` | transaction snapshot + private index delta. `(key, RowId)` 순서, 명시적 EOF, owning snapshot과 동일 private-view revision |
| `reclaim()` | 증명된 unreachable whole pack만 삭제; 논리 상태·q 변경 없음 |
| `compact()` | admission drain 후 전체 현재 tree를 bounded streaming으로 재작성하고 유지보수 commit. 실패는 old view 유지 |

cursor가 열린 뒤 같은 transaction에서 statement를 수행하면 기존 cursor를 `CursorInvalidated`로 거절한다. transaction 종료·재open·다른 engine에서도 재사용할 수 없다. range bounds는 key tuple에 적용하고 같은 key의 RowId는 resume의 tie-breaker로 사용한다. 반환 row는 caller의 제한된 owned 결과이며 page guard를 해제한 후 빌린 bytes를 남기지 않는다. 기존 DEV2 row scan도 이 private-view revision 규약을 디스크 확장 시 적용하며 신규 오류 vector를 동결한다.

<a id="page"></a>
## 4. page·pack·물리 identity

초기 page 크기는 **16 KiB**, header 예약 128 bytes, 마지막 SHA-256 32 bytes다. 나머지는 정렬된 slot directory와 packed cell 영역으로 나눈다. slot은 `(offset:u16, length:u16)`이고 cell bytes는 page 끝의 digest 앞에서 역방향으로 채운다. committed page에 free-list mutation을 하지 않고 private rebuild로 빈 공간을 정리한다. 행 codec은 key codec과 별도다.

| 항목 | 필수 내용·검사 |
| --- | --- |
| 공통 header | magic, format, kind, header/page length, DB identity, file identity, page number, owner ObjectId/Generation, level, slot count, lower/upper, reserved=0 |
| page 종류 | catalog leaf/internal, row leaf/internal, index leaf/internal, overflow. 잘못된 kind/owner/level 참조 거절 |
| `PageRef` | `FileId(writer_epoch:u64, file_counter:u64)`, `page_no:u32`, kind:u8, reserved 3 bytes, object generation:u64의 32 bytes. 숫자는 little endian |
| integrity | header+payload+0으로 채운 미사용 영역을 SHA-256으로 검사. checksum은 파일 sync나 악의적 변경 방지의 대체물이 아님 |
| parser | 길이 산술·slot overlap·경계·정렬·중복·child level·순환을 할당/역참조 전 검사. 임의 Rust struct dump 금지 |

header의 최종 field offset·magic·pack header/footer layout·독립 hex vector는 D3-F01에서 동결한다. 위 크기와 필수 필드는 설계 기준이며 binary format 배포 승인이 아니다. checksum은 이미 채택한 SHA-256 dependency를 사용하고 새 dependency는 추가하지 않는다.

pack은 한 준비 작업이 소유하는 immutable 파일이다. 파일당 최대 64 MiB이며 header 1 page, 연속 data pages, count·길이·digest를 가진 seal footer를 포함한다. 최종 짧은 pack을 64 MiB로 padding하지 않는다. 파일별 SHA-256·길이는 dependency inventory에 저장한다. 준비 중 header 확정은 허용하되 seal 뒤 append/overwrite는 금지한다. 파일은 최종 고유 이름으로 배타 생성하고 파일 sync·directory sync 전까지 미선택 staging이다. rename만으로 commit을 표현하지 않는다. 파일 생성 전에 bounded build-owner pin을 등록하며 owner의 디스크 staging inventory로 생성 파일을 추적한다. root pin만으로 아직 root에 연결되지 않은 준비 파일을 보호했다고 하지 않는다.

`FileId`는 DEV2의 영속 예약 writer epoch와 해당 open의 단조 counter를 사용한다. rollback 때 counter를 되돌리지 않고 재시작은 새 epoch를 쓴다. q·RowId·파일 counter를 혼용하지 않는다. physical file 이름과 page slot은 재사용하지 않는다. 이미 존재하면 덮어쓰기 대신 오류로 차단한다. restore는 새 timeline namespace에서 원본 pack identity를 유지하고 이후 파일은 backup high-water보다 큰 writer epoch로 만든다. page의 DB identity는 보존하고 원본 timeline은 manifest provenance로 검증하므로 모든 page를 복원 때 재작성할 필요가 없다. runtime handle의 새 timeline/instance 검사는 유지한다.

<a id="tree"></a>
## 5. row·tree·scan

row tree key는 nonzero RowId의 big-endian 순서 bytes다. leaf cell은 RowId, schema 식별자, canonical row 길이와 inline/overflow tag를 가진다. row는 DEV2의 128 KiB, 가변 값은 64 KiB 상한을 유지한다. canonical row가 2 KiB 이하이면 inline, 초과하면 전체 row를 overflow chain에 저장하고 leaf에는 길이·첫 PageRef·row digest만 둔다. overflow에는 chunk ordinal·bytes·next ref·동일 owner를 저장하며 총 길이·최대 chain 길이를 row 상한에서 산출한다. 순환·중복 chunk·조기 종료·trailing bytes·다른 row 소유자를 거절한다.

internal page는 첫 child와 `(separator, right child)` 목록이다. separator는 오른쪽 subtree의 최소 full key이며 `left < separator <= right`로 찾는다. row와 index tree 모두 **변경 경로를 복사**하고 immutable child를 공유한다. leaf split은 cell 수가 아닌 encoded bytes가 양쪽에 들어가는 경계를 골라 약 절반으로 나눈다. separator를 상위에 반영하고 필요 시 root까지 split한다. max key/cell 한도는 최소 두 child와 separator가 들어가도록 검증한다. 높이는 16 이하이며 초과는 commit 전 LimitExceeded다.

delete는 새 leaf에서 제거하고 비어 있는 child를 parent에서 제거하며 unary root를 축소한다. 비어 있지 않은 underfull page의 즉시 merge는 초기 필수 알고리즘에서 제외한다. 따라서 최소 occupancy/최적 높이를 보장하지 않으며 삭제 편향의 read/write 증폭을 PF09로 측정한다. S1에는 아래 offline compaction을 포함해 장기간 공간을 줄일 실제 경로를 제공한다.

**영속 sibling link를 scan의 정본으로 쓰지 않는다.** old leaf의 next pointer가 새 COW root의 leaf를 가리킨다고 가정하면 세대가 섞일 수 있다. cursor는 고정 root, 마지막 full key, 높이 제한의 `(PageRef, child ordinal)` stack으로 다음 leaf를 찾는다. frame은 leaf 처리 후 반환하고 다음 페이지에서 재획득할 수 있다. stack이 깨지면 마지막 key보다 큰 값을 같은 root에서 재탐색한다. private delta는 bounded 정렬 집합으로 merge하며 삭제·교체의 old index entry는 제외한다. 반복·누락과 limit 경계는 독립 scan 결과와 대조한다.

<a id="keys"></a>
## 6. index 의미와 statement 원자성

`IndexSpec`은 소유 TableUID, 새 ObjectId/Generation, kind(PK/Unique/Nonunique), 1–4개 서로 다른 열의 순서, ascending binary 비교를 가진다. table당 index 최대 8개이며 PK는 최대 1개다. expression·partial·descending·collation 옵션은 명시적으로 미지원이다. 이 제한은 DEV3 기준선이고 후속 SQL 기능을 영구 제외하는 결정이 아니다.

- component encoding은 DEV1의 `encode_key` 의미를 따른다. NULL은 첫 순서, Bool은 false<true, Int64는 sign-bit 변환, UTF-8/Binary는 byte 순서와 terminator escaping이다. 고정 schema에서 component를 이어 composite key를 만든다. 다른 타입으로 자동 변환하지 않는다.
- encoded composite key는 RowId를 제외하고 최대 1,024 bytes다. 원본 문자열 길이만으로 판정하지 않고 escape 후 길이를 검사한다. index가 없는 열의 64 KiB 값 지원은 유지한다. index 생성·DML에서 넘치면 `IndexKeyTooLarge`로 해당 statement/build를 원자 실패시킨다.
- 모든 index의 물리 key는 `(composite, RowId)`다. PK는 모든 열 NOT NULL이고 전체 composite가 유일해야 한다. PK 생성은 해당 열이 이미 nonnullable인 schema에서만 허용한다. nullable metadata를 숨겨서 변경하지 않는다.
- Unique는 NULL이 하나라도 있으면 서로 충돌하지 않는 정책을 선택한다. NULL이 없는 composite만 RowId를 제외하고 중복 검사한다. Nonunique는 같은 composite를 RowId 순서로 보존한다. SQL `NULL = NULL`을 참으로 바꾸지 않는다.
- uniqueness는 각 mutation 직후, snapshot+앞선 private 변경을 기준으로 즉시 검사한다. 같은 statement에서 두 행의 unique key 교환도 첫 mutation의 중복이면 실패한다. deferred constraint는 미지원이다. 실패하면 그 statement의 row·모든 index delta·quota·임시 파일을 진입 상태로 복원하고 이전 statement는 보존한다. rollback 후 orphan 파일의 disk quota는 실제 삭제가 확인될 때까지 소비량에 남긴다. private memory 해제와 물리 disk 해제를 혼동하지 않는다.

commit coordinator의 `start_q == current_q` 검사는 concurrent duplicate 삽입·범위 phantom·schema 변경도 보수적으로 충돌시킨다. uniqueness 사전 검사만으로 이 검사를 생략하지 않는다. commit 전에 old/new index entry와 최종 row의 일관성을 확인하며 key 변경은 old 삭제+new 삽입을 하나의 private 변경으로 준비한다. RowUID는 PK와 독립이므로 PK 변경으로 행 정체성을 바꾸지 않는다.

index build는 admission close→기존 transaction/cursor drain→고정 root scan→bounded run 생성/merge sort→duplicate 검증→COW tree build→catalog 변경의 한 commit 순서다. timeout이면 build를 시작하지 않고 admission을 복원한다. build 실패·취소는 기존 catalog 유지, commit 시도 뒤는 공통 UNKNOWN 규칙이다. spill은 소유 임시 파일·quota에 포함하며 run 목록도 디스크에 둔다. 미완성 index를 `valid`로 노출하지 않고 unique 실패를 일부 행 제외로 처리하지 않는다. built tree를 재시작 때마다 재생성하지 않는다.

<a id="buffer"></a>
## 7. buffer·pin·lock·자원

공유 buffer에는 검증된 **immutable clean page**만 넣는다. private dirty page와 build spill은 별도 예약을 사용하고 commit 전 sealed pack으로 내보낸다. 공유 buffer eviction이 dirty write나 WAL sync를 수행하지 않으므로 별도 background flusher는 S1 선행 조건이 아니다.

frame 상태는 `Empty → Loading(identity) → Clean(identity, pins, usage) → Evicting → Empty`다. 실패한 Loading은 waiter에게 오류를 전달하고 프레임을 해제한다. cache map entry 등록과 최초 pin은 동일 짧은 mutex 경계로 보호한다. 하나의 identity에 Loading이 있으면 새 loader는 제한된 wait/retry를 사용한다. I/O·checksum은 mutex 밖에서 하고 completion 시 identity와 frame generation을 다시 확인한다. clean frame을 교체할 때는 pins=0과 I/O 부재를 확인하고 map 제거·새 identity 예약을 한 경계로 처리한다.

교체는 단순 clock을 사용한다. bounded sweep 뒤 후보가 없으면 deadline 있는 wait 또는 `BufferBusy`이며 무한 회전하지 않는다. 순차 scan은 작은 재사용 ring을 사용해 전체 hot set을 지우는 비용을 측정한다. 처음부터 lock-free·NUMA shard·prefetch를 필수로 하지 않는다. `PageGuard`는 frame pin, `SnapshotGuard`는 디스크 reachable closure pin이다. snapshot 하나가 DB 전체 page frame을 pin하지 않는다. cursor가 외부 호출 사이 유지할 frame은 최대 2개이고 tree stack에는 ref만 둔다.

기존 lock 순서 `coordinator → retention ledger → publication`을 유지한다. buffer mutex·frame 상태 lock을 보유한 채 그 상위 lock을 얻지 않는다. page load 중 ledger/publication lock을 잡지 않고, root/file 소유 pin을 먼저 획득한다. coordinator는 commit 준비 I/O 동안 보유할 수 있으나 ledger/publication lock은 해제한다. PageGuard drop과 callback이 같은 mutex를 재취득하는 경로를 검토한다. wait 중 취소·timeout은 statement 이전 상태를 복원하고 commit 시도 이후 취소는 rollback 성공으로 답하지 않는다.

| DEV3 초기 공학적 profile | 한도·계상 |
| --- | --- |
| page buffer | 64 MiB(4,096 frames). test용 4/16 MiB profile 별도. descriptor/map bytes는 metadata 예산 |
| metadata/cache | 합계 8 MiB. catalog/locator/file-handle cache·frame descriptors·pin ledger 포함; locator 1 MiB, inventory decoder 64 KiB를 포함; 파일 handle 최대 128 |
| transaction | DEV2의 256 mutation, WAL payload 전체 1 MiB, private 8 MiB/transaction·전체 64 MiB 유지. index delta·준비 metadata 포함 |
| 시스템 scratch | checkpoint/recovery/build/compact/GC 중 하나에 16 MiB, 나머지는 Busy. private page·merge run 목록의 무제한 상주 금지 |
| 동시성·pin | user transaction/snapshot 32, checkpoint 1, backup 1. 자동 reader 만료 없음; 장기 pin은 진단·admission 제한 |
| retained disk | 초기 소유 DB quota 8 GiB, 그 안의 유지보수 reserve 1 GiB; retained WAL 256 MiB, 단일 manifest/inventory chunk 1 MiB |
| 진단 | DEV2의 4,096 event·4 MiB trace. 필수 fault receipt 유실은 시험 실패 |

DEV2의 committed+old row 64 MiB 전량 상주 예산은 DEV3에서 page buffer/metadata/pinned disk 회계로 대체한다. DEV2 substrate 한도는 그대로 유지한다. DEV2의 4,096-file 상주 inventory를 DEV3의 영구 파일 수 상한으로 가져오지 않고 디스크 inventory와 제한된 decoder로 대체한다. 논리 한도와 allocator/OS RSS는 구분하며 memory ledger의 고유 allocation만 합산한다. 실제 RSS 기준·장치 free-space·I/O timeout 수치는 D3-F05 환경 profile에 동결한다. `try_reserve`만으로 OS OOM 복구를 약속하지 않는다.

<a id="commit"></a>
## 8. 다중 객체 commit과 WAL

1. active transaction이 row/index private delta와 statement 결과를 준비한다. old snapshot·자원 pin을 유지한다.
2. coordinator에서 start_q와 schema/generation을 검증한다. 최종 catalog tree에 모든 table/index 변경을 모으고 q=current+1을 잠정 계산한다.
3. 새 page·overflow·catalog 경로를 pack으로 준비하고 새 파일 inventory를 chunk로 기록한다. rollback·receipt·publication·retention entry 공간과 전체 WAL 크기를 **commit 시도 전** 예약한다.
4. 최종 tree 생성은 예약된 소량 page workspace와 staging pack을 사용한다. transaction 전체 새 page를 한 Vec에 모으지 않는다. 이미 sealed된 private pack은 해당 build-owner 아래에서만 다시 읽고 공개 cache의 visible root로 등록하지 않는다. 중간 path-copy로 버려진 page도 pack quota에 계상한다. 모든 새 pack과 dependency inventory의 write→file sync→이름 directory sync를 끝낸다. 재사용 page는 기존 pin된 durable closure의 일부여야 한다. namespace/준비 I/O 불명은 transaction commit 미시도라 ABORTED 판정 가능하나 engine은 보수적으로 Fenced로 둔다.
5. L0의 한 완결 frame에 TxnId/prev_q/q, 논리 row after-image/tombstone·catalog delta, 전후 root identity, 새 catalog-root, dependency inventory ref/digest, high-water를 넣는다. after-image와 disk root 둘 다 유지하며 새 metadata도 1 MiB payload 상한에 포함한다. 초과 거래를 부분 commit하지 않는다.
6. 첫 WAL byte 시도 직전에 CommitAttempted. WAL append→sync→미리 준비한 전체 CommittedRoot 교체→ACK다. publication lock 안 allocation·I/O·old-root 소멸은 없다.

WAL은 모든 page를 복제하지 않는다. after-image는 논리 변경/독립 검산을 위한 기록이고 disk recovery는 검증된 root·pack을 선택한다. valid commit이 참조한 pack이 없으면 after-image만으로 즉석 재구성해 성공하지 않는다. row delta와 root의 의미 일치는 실제 시험의 독립 oracle 및 복구 검증으로 대조한다. catalog-only/index-build/compact commit은 논리 row delta가 0일 수 있고 kind와 before/after root를 명시한다. logical q는 물리 유지보수 commit에도 전진하므로 시작 q가 다른 writer는 동일하게 충돌한다.

| cut | transaction 결과와 파일 의무 |
| --- | --- |
| page 준비/파일 sync 전후, WAL 미시도 | 기존 root 유지, ABORTED. 새 파일은 orphan 후보이며 소유와 비참조 증명 뒤 정리 |
| WAL 첫 byte 이후 write/sync 불명 | UNKNOWN + Fenced. 새 pack·old/new closure 보존, 자동 재전송 금지 |
| WAL sync 뒤 publish/ACK 전 장애 | durable transaction 보존. 응답 미관측은 외부 UNKNOWN이며 recovery는 전체 반영 |
| ACK 관측 뒤 crash | row/index/catalog/locator 전체가 같은 q로 보존되어야 함 |

<a id="recovery"></a>
## 9. recovery·checkpoint·backup

DEV2의 `LEASE → PREFLIGHT → SELECT → LOAD → SCAN → APPLY → VERIFY → STABILIZE → RESUME → READY`를 유지한다. LOAD는 모든 row를 RAM에 만들지 않고 checkpoint catalog-root와 bounded caches를 연다. SCAN/APPLY는 연속 q의 commit frame마다 dependency inventory·새 pack의 길이/digest·before/after root chain을 검사하고 private root를 전진시킨다. transient intermediate root도 실제 replay 의무에서 제외하지 않는다.

VERIFY는 선택한 checkpoint와 채택한 root의 tree 구조·소유·bounds·row/index 정합성을 bounded 순회로 검사한다. **recovery 검증은 추가 디스크 spill을 요구하지 않는다.** 각 row를 scan하면서 모든 index의 `(key, RowId)`를 point lookup하고, 각 index를 역으로 scan하면서 해당 RowId의 존재·key 일치·strict physical order·인접 unique 조건·전체 entry count를 검사한다. NULL entry도 포함해 index당 row 수와 entry 수가 같아야 한다. catalog의 이름/객체 양방향 관계도 검사한다. locator는 row tree의 RowId·slot·overflow identity로 검사한다. parent의 key 범위·child level 감소·non-root empty child 거절·bounded overflow chain 검사로 잘못된 graph를 거절한다.

검증은 root stack·한 row·한 index key와 bounded buffer만 유지한다. 정상 quota 내 DB를 개방하기 위해 전체 index 크기만큼 임시 disk 공간을 추가 요구하지 않는다. VERIFY 중 재장애도 새로운 spill 파일을 누적하지 않고 같은 원본에서 다시 시작한다. 최종 root만 검사해 중간 commit의 누락 dependency를 숨기지 않는다. 초기 full verification은 많은 random lookup I/O를 지불하며 빠른 startup을 보장하지 않는다. 외부 정렬을 이용한 빠른 VERIFY는 보호된 scratch와 반복 장애 수명을 별도로 설계한 뒤만 채택한다. incremental verification도 별도 D2다.

STABILIZE는 기존 journal뿐 아니라 채택에 필요한 pack·inventory·epoch·namespace도 영속화한다. 프로세스 종료 뒤 OS cache에서 읽은 완결 bytes를 이전 sync 증거로 간주하지 않는다. 실패하면 새 writer epoch·READY·GC를 허용하지 않는다. incomplete tail 허용과 full-length checksum 오류 거절은 [DEV2 장애 판정표](dev2-kernel-design.md#fault-outcomes)를 따른다. page 손상은 WAL tail처럼 버리지 않는다.

checkpoint는 coordinator/ledger/publication 경계에서 **root C·C 직후 replay anchor·high-water·closure pin을 함께** 잡는다. C의 reachable pack inventory는 디스크 chunk로 streaming 작성하고 top manifest는 chunk refs를 가진 tree의 root·digest·count를 담아 1 MiB 이하로 제한한다. inventory tree 자체의 closure도 보존한다. pack→inventory chunk→manifest→CURRENT temp/sync/rename/dirsync 순서를 지키고 WRITER를 되돌리지 않는다. checkpoint는 전체 DB를 다시 복사할 필요 없이 기존 immutable pack을 참조한다.

CURRENT는 current/previous manifest를 명시한다. fallback은 previous에서 현재 WRITER까지 모든 WAL·각 commit page closure가 검증될 때만 허용한다. 두 checkpoint가 공유하는 pack이 손상되면 previous가 있다는 이유만으로 복구 가능하다고 주장하지 않는다. fallback 후 CURRENT 재선택·previous 제거·새 checkpoint 의무는 DEV2와 동일하다.

exact-cut backup은 C의 manifest·inventory·reachable pack 파일 전체를 pin하고 복사한다. pack 내부의 unreachable bytes가 일부 포함되어도 C 밖의 root를 선택하지 않는다. restore는 별도의 빈 소유 경로에 전체 closure 검증→새 timeline/base-q=C·epoch/anchor→CURRENT/WRITER 영속 선택 후 개방한다. **원본 WAL 없이 복원→새 commit ACK→재시작**을 검증한다. 최신 R/PITR를 root-only WAL만으로 제공하지 않는다.

<a id="reclaim"></a>
## 10. retention·회수·compaction

보존 대상은 `current/previous checkpoint + live snapshot + checkpoint/build/backup pin + retained replay의 모든 commit root + 준비/UNKNOWN dependency`의 reachable 합집합이다. 최신 root에서 사라진 page도 previous checkpoint부터 순차 replay할 때 필요하면 보존한다. snapshot은 page를, replay는 root/pack과 WAL을 각기 필요한 범위로 pin한다.

ledger에는 bounded active root handle과 checkpoint/replay range의 끝점만 상주시킨다. retained commit root·pack inventory는 WAL/디스크에서 streaming 열거하며 replay 범위의 모든 q를 RAM 목록으로 만들지 않는다.

ledger의 root pin 집합과 revision을 짧은 lock 아래 고정한 뒤, lock 밖에서 tree/overflow/inventory를 순회하고 `(FileId, page_no)` mark를 bounded 외부 정렬한다. whole pack이 참조 0일 때만 삭제 후보가 된다. candidate 목록도 디스크에 둔다. 새 commit/pin/checkpoint 선택이 생기면 ledger revision을 증가시킨다. 최종 coordinator→ledger 경계에서 revision 동일·Fenced 아님·새 pin 없음 확인 후 제한된 candidate batch를 Retiring으로 전환한다. revision이 달라지면 mark를 다시 수행한다. 장기 경합 때 admission drain의 maintenance 구간으로 진행을 보장한다. 정상 I/O 동안 ledger/publication lock을 보유하지 않는다.

Retiring 파일에는 새 pin을 등록하지 않고 실제 unlink·directory sync는 lock 밖에서 한다. crash로 남은 파일은 다시 조사한다. 읽지 못한 directory entry·손상 manifest를 unreachable 증거로 취급하지 않는다. 자동 orphan 정리는 선택 chain과 모든 pin 밖인 엔진 소유 파일만 대상으로 하며 알 수 없는 파일은 삭제하지 않는다. restart 후 ledger 재구성과 STABILIZE 전에는 GC를 금지한다.

whole-pack GC만으로는 살아 있는 page 하나가 많은 dead bytes를 붙잡을 수 있다. S1의 초기 해결은 **offline 전체 현재 상태 compaction**이다. admission close/drain 후 현재 row/index/catalog를 bounded streaming으로 새 pack에 재작성하고 하나의 유지보수 commit으로 root를 전환한다. 논리 행·index 정의·RowUID는 그대로다. root-only 유지보수 WAL의 dependency inventory는 chunked라 DB 크기만큼 WAL frame이 커지지 않는다. workspace 16 MiB와 disk reserve로 충분하지 않으면 시작 전 거절한다. 실제 ENOSPC는 commit 전 실패·orphan으로 처리한다.

새 checkpoint만으로 old pack이 즉시 해제되지는 않는다. previous checkpoint와 replay roots, 기존 backup pin이 빠질 때까지 보존한다. 정상 checkpoint 두 번으로 두 fallback 기준이 새 pack closure로 전진한 뒤 남은 pin이 없을 때만 회수한다. reserve보다 큰 DB는 사용자 quota 확대 또는 명시적인 maintenance 공간 마련이 필요하다. 저여유 공간의 부분 evacuation, online compaction, in-place page 재사용은 후속 대안이며 현재 무중단 공간 회수를 약속하지 않는다. 이 제약과 수행 시간·추가 bytes는 PF09에 보고한다.

<a id="acceptance"></a>
## 11. 검증·측정과 구현 전 동결

[구현 계획](../plans/dev3-implementation-plan.md#acceptance)이 DS01–06·KU09/10·RC-T12–14·PF09와 DEV2 회귀를 구체 사례에 연결한다. 최소 working set은 configured buffer의 4배이며 row/index/locator/inventory의 각 corpus에도 RC-T14의 configured page-buffer 4배 기준을 유지한다. locator/inventory 전용 cache 압력은 추가로 검사하며 작은 cache 기준으로 전체 buffer 인수를 대체하지 않는다. 정확한 크기와 중복 계상 방지는 구현 계획에 둔다. page-cache hit만 측정한 결과를 실제 저장 I/O 증거로 사용하지 않는다. engine read/write/sync와 외부 OS I/O 관측을 함께 남긴다. OS cache cold/warm은 별도 profile이며 공유 host의 전체 cache를 비우지 않는다.

필수 지표는 page read/write bytes·hit/miss·pin wait, tree height/split·path copy·overflow, 논리 변경 대비 pack bytes, pack/file 수, live/retained/unreachable bytes, WAL/page/dir sync latency, checkpoint/GC/compaction/reopen 시간·scratch/RSS peak, index↔scan equality다. baseline과 candidate는 동일 의미·durability·fixture·환경에서 비교한다. 측정 없는 성능 개선 주장은 하지 않는다.

D3-F01–F06의 byte vectors, Rust API/오류 상태표, fault receipts, profile 수치와 DEV2 실제 통합 근거는 해당 leaf 구현 전에 동결한다. 이 설계의 채택은 S1 PASS가 아니며 실제 entrypoint/evidence는 아직 없다.

<a id="decision"></a>
## DEV3-DESIGN-001 결정 기록

| 항목 | 내용 |
| --- | --- |
| 분류·상태·결정일 | D2 · ACCEPTED · 2026-10-18 |
| 작업·계약·통합 담당 | `/root` |
| 근거 | 사용자가 DEV3 설계와 여러 로컬 DBMS 및 추가 로컬 소스 참고를 명시 요청 |
| 선택 | COW B+tree·immutable pack·bounded clean buffer·row tree locator·공통 L0/root·closure 보존·offline compaction |
| 비교 대안 | 이 문서 2절. in-place/LSM·전체 locator 상주·온라인 구조 변경은 초기 채택하지 않음 |
| 적용 | 기존 XADB-SCOPE-001과 DEV2/DISK 불변식 구체화. DEV2 substrate의 실제 의미·상태·과거 증거는 변경하지 않음 |
| 독립 검토·검증 | `/root/dev3_design_review` 최종 설계 검토 PASS. P2 수정 확인·문서 무결성 PASS. [계획의 작업 기록](../plans/dev3-implementation-plan.md#review); 제품 실행 NOT_RUN |
| 재검토 조건 | DEV2 실제 API/형식과 가정 불일치, COW·small pack·full verification 비용 과다, quota/compaction 진행 불능, key·page 한도 확대, 실제 S1 반례 |

# DEV3 control·FILEMAP·bootstrap profile 표현

2026-10-21 · **D3-CONTROL-F01 형식 독립 검토 수용 / byte 표현 구현·자체 시험 완료 / 소스 독립 검토·별도 시험 완료 / 실제 효과·F05 미완료**

[파일 identity](disk-storage-design.md#page) · [DWB bootstrap](disk-storage-design.md#doublewrite-batch) · [REBASE](disk-storage-design.md#recovery-rebase) · [checkpoint](disk-storage-design.md#checkpoint) · [예약](disk-storage-design.md#recovery-reservation) · [profile](disk-storage-design.md#budgets) · [allocation](dev3-allocation-format.md) · [DWB 표현](dev3-doublewrite-format.md) · [WAL 표현](dev3-wal-format.md) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 leaf의 범위는 고정 profile, FILEMAP의 root/chunk, 선택 reference와 FILEMAP/DWB selector의 **무효과 byte 표현**이다. 실제 namespace·create/write/sync/rename·file growth·checkpoint capture·reservation restore·복구 선택·READY는 구현하지 않는다. 아래 형식·크기·주소·시험 분모와 독립 literal14개는 root D2 검토에서 수용했다(`artifacts/dev3/control-contract-root-review-r1/review.json`). 실제 효과와 전체 자원 충족을 수용한 것은 아니다. 다른 동결 codec의 bytes를 변경하지 않는다.

CURRENT/WRITER의 checkpoint·WAL epoch body, ATT/system obligation 및 immutable ownership inventory의 내용은 이 계약의 성공 decoder가 아니다. 이 leaf를 구현하더라도 그것들이 준비되지 않은 DB 생성/open/recovery는 지원하지 않는다. 특히 profile ID를 읽었다는 사실을 BOOT_RESERVE 성공이나 F05 완료로 변환하는 API를 두지 않는다.

## 1. 선택 경계와 두 대안

현재 foundation의 `Selector`는 CURRENT/WRITER만 가지며 Linux `publish_selector`도 이 typed 선택만 받는다. FILEMAP/DWB를 임의 문자열 rename으로 우회하지 않는다.

| 방안 | 내용·비용 | 제안 상태 |
| --- | --- | --- |
| A: typed selector 확장 | `Selector::FileMap`=`FILEMAP`, `Selector::Doublewrite`=`DWB`를 추가. FILEMAP/DWB는 각각 독립 generation·교체 공간·소유 coordinator를 유지. 서로 필요한 namespace/참조 보존 순서는 상위 effect 계약이 검사 | 방향 수용. foundation enum·Linux reserved-name·engine-worker fault match와 namespace 회귀를 별도 owner patch로 수행해야 함 |
| B: CURRENT 하나에 통합 | FILEMAP/DWB/checkpoint/writer continuation을 한 snapshot으로 원자 선택. 새 port variant는 필요 없지만 매 DWB retirement가 CURRENT 전체를 갱신하며 다른 owner의 최신 값을 잃지 않는 단일 serialization/lock 순서·generation·복구 모델이 필요 | 미채택 비교안. 기존 DWB 8 KiB를 공통 selector 몫으로 바꾸거나 기존 checkpoint 의미를 암묵 변경하지 않음 |

아래 byte 제안은 A를 사용한다. 채택 후에도 각 selector의 고빈도 갱신을 서로 같은 lock에 합치는 것은 별도 결정이다. FILEMAP 교체와 DWB pending의 관계는 최신 FILEMAP이 모든 live WAL/DWB/current·previous checkpoint가 요구하는 FileId/길이 mapping을 계속 제공하는 보존 규약으로 연결한다. 새 mapping이 영속 선택되기 전에는 그 file/길이를 WAL·DWB에서 참조하지 않는다. drop은 필요한 closure가 사라진 뒤만 mapping에서 제거한다. 이 교차 조건은 control checksum만으로 증명되지 않는다.

손상·누락 selector, 선택 generation/digest/length 불일치는 거절한다. 큰 generation 파일을 찾거나 directory를 순회하거나 raw DWB/비선택 FILEMAP을 검색하여 대체 후보를 선택하지 않는다. CURRENT의 명시 previous checkpoint가 허용하는 제한적 fallback도 해당 checkpoint owner가 완전 closure를 검증해야 하며, FILEMAP/DWB에 임의 previous fallback을 추가하는 근거가 아니다. UNKNOWN 교체 뒤에는 old/new 합집합 closure를 보존하고 fence한다.

## 2. 고정 경로와 root 신뢰

모든 파일은 caller가 지정하고 LinuxDirectory가 연 단일 DB directory의 직접 child다. 이름은 `FileName`의 ASCII/단일 component/최대128-byte 검증을 받고 아래 canonical spelling과도 일치해야 한다. 상대·절대 path, slash, `.`/`..`, symlink/hardlink 별칭, 다른 decimal/대문자 hex spelling을 저장하지 않는다.

| 이름 | 소유·물리 길이 |
| --- | --- |
| `d3-profile` | 생성 시 한 번 설치하는 immutable profile control, 정확4,096 bytes |
| `FILEMAP`, `d3-filemap-select-stage` | 선택/staging 각각 정확4,096 bytes |
| `d3-filemap-0`, `d3-filemap-1` | 각각 고정2,113,536 bytes; root slot16 KiB + chunk slot16 KiB×128 |
| `DWB`, `d3-dwb-select-stage` | 선택/staging 각각4,096 bytes; 기존 DWB16 MiB 안의8 KiB 몫 |
| `d3-dwb-0`, `d3-dwb-1`, `d3-dwb-stage` | 기존 DWB control 두 공간·staging 각각16 KiB. XDW1 논리 bytes는 별도 exact 길이 |
| `d3-dwb-images` | 고정16,711,680 bytes=1,010×16,544-byte slot+비대여2,240-byte tail. slot i의 offset=i×16,544 |
| `d3-dwb-reserved` | metadata 예산 중 비대여8,192-byte 여유. control49,152+selector8,192+이여유8,192+image16,711,680=16,777,216 bytes |
| `d3-rebase-0`, `d3-rebase-1` | 아래 제안의 fixed bootstrap descriptor 슬롯 각각1 MiB. body는 아직 미정 |
| `CURRENT`, `d3-current-stage`; `WRITER`, `d3-writer-stage` | 각 선택/staging4 KiB 용량을 별도로 예약. 내용·인계 상태는 후속 owner 계약 |
| `d3-data-` + FileId의 정확16자리 lowercase hex | 정확24 ASCII bytes. FILEMAP entry에 같은 이름과 zero padding을 저장 |

`d3-profile`은 새 DB 생성/restore의 검증된 namespace에서 작성·sync하고 이후 그 DB 수명 동안 덮어쓰지 않는다. buffer profile 변경도 이 F01의 온라인 갱신 기능이 아니다. fault profile은 지정 profile로 만든 fresh fixture를 사용한다. 기존 directory open에서는 **이 고정 root 한 파일**의 exact length·format·checksum·capacity 표를 검증하여 namespace를 발견한다. caller가 이미 기대 DB/timeline을 갖고 있으면 그것도 비교한다. 이후 모든 selector/control의 기대 namespace/profile generation/full digest는 이 검증한 root에서 오며 각 input header를 자기 기대값으로 복사하지 않는다. root 발견은 외부 인증이나 임의 다른 DB directory 탐지를 보장하지 않는다.

새 DB 초기 설치의 완료 marker·CURRENT/WRITER closure가 아직 없으므로 profile 하나만 존재하는 directory를 정상 생성된 DB로 받아들이지 않는다. 부분 생성의 owned staging 목록과 정리 절차도 별도 생성 owner가 먼저 정의해야 한다. `list_owned` 결과를 읽어 누락된 selector/manifest를 재구성하지 않는다.

## 3. 공통 control envelope

다중 byte 정수는 little endian이다. Profile1, FileMapRoot2, FileMapChunk3만 이 envelope의 현재 지원 kind다. 다른 kind는 UnsupportedFormat이며 opaque 성공 body로 반환하지 않는다. 기존 DWB XDW1은 자체 header/checksum을 그대로 사용하고 이 envelope로 이중 wrapping하지 않는다.

공통 길이는 `128 + payload_bytes + 32`다. 끝32 bytes는 앞의 header와 payload 전체에 대한 SHA-256이다. 선택 reference의 digest는 **이 trailer까지 포함한 전체 논리 control bytes**의 SHA-256이므로 서로 다른 두 hash다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | `XDC1` |
| 4 | 2 | format1 |
| 6 | 1 | Profile1, FileMapRoot2, FileMapChunk3 |
| 7 | 1 | file slot. Profile은0; FILEMAP은0/1 |
| 8 | 2 | header bytes128 |
| 10 | 2 | flags0 |
| 12 | 4 | exact 전체 logical bytes |
| 16 | 16 | nonzero DatabaseId |
| 32 | 8 | nonzero timeline |
| 40 | 8 | nonzero control generation |
| 48 | 4 | object index. Profile/root는0; chunk는0..127 |
| 52 | 4 | item count. Profile은0; root는chunk 수; chunk는entry 수 |
| 56 | 4 | exact payload bytes |
| 60 | 68 | reserved0 |
| 128 | 가변 | 해당 kind의 payload |
| 끝32 | 32 | header+payload SHA-256 |

generation은 control 종류/소유별 identity다. selector generation, profile generation, FILEMAP generation과 DWB batch id를 서로 같다고 가정하거나 숫자 대소로 대신 검사하지 않는다. MAX는 읽을 수 있으나 발급 시 checked 증가가 실패하며 wrap/reuse하지 않는다. 발급·high-water의 영속 보장은 후속 효과다.

## 4. ControlRef64와 selector4 KiB

ControlRef는 namespace를 중복 저장하지 않는다. 그 reference를 담은 검증된 header의 Namespace와 함께 기대값으로 전달해야 한다.

| ref offset | bytes | 값 |
| --- | --- | --- |
| 0 | 1 | Profile1, FileMapRoot2, FileMapChunk3, Doublewrite4, OwnershipRoot5, OwnershipChunk6 |
| 1 | 1 | file slot; Profile0, FileMap/DWB0/1, Ownership0..2 |
| 2 | 2 | reserved0 |
| 4 | 4 | object index; FileMapChunk0..127, OwnershipChunk0..u32MAX, 그 밖0 |
| 8 | 8 | nonzero generation |
| 16 | 4 | exact logical encoded length |
| 20 | 12 | reserved0 |
| 32 | 32 | trailer 포함 전체 logical bytes SHA-256 |

Profile 길이는4,096, root는`224+96K`(0≤K≤128), chunk는`160+96N`(1≤N≤64), DWB는`160+88N`(0≤N≤64)다. all-zero digest를 sentinel로 해석하지 않는다. 다른 kind/slot/index/length 조합은 거절한다. 이 reference를 만들었다고 대상 파일이나 체크포인트/ownership closure가 존재하는 것은 아니다. [ownership의 별도 D2](dev3-ownership-format.md)로 kind5/6을 등록했다. OwnershipRoot 길이는`160+56O`(0≤O≤36), OwnershipChunk는`160+96I`(1≤I≤169)다. 두 새 kind의 물리 파일명은 미동결이므로 `control_file_name`은 UnsupportedFormat을 반환하고 기존 selector는 계속 FileMapRoot/Doublewrite만 소비한다. 후속 checkpoint 등록과 실제 ownership 선택·설치는 별도 계약이 필요하다.

selector는 정확4,096 bytes이며 마지막32 bytes는 앞4,064 bytes의 SHA-256이다.

| selector offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | `XCS1` |
| 4 | 2 | format1 |
| 6 | 1 | role FileMap1, Doublewrite2 |
| 7 | 1 | reserved0 |
| 8 | 16 | DB |
| 24 | 8 | timeline |
| 32 | 8 | nonzero selector generation |
| 40 | 8 | 검증한 profile generation |
| 48 | 32 | 전체4,096-byte profile SHA-256 |
| 80 | 64 | 선택 ControlRef. role1은Root2/index0; role2는Doublewrite4/index0 |
| 144 | 3,920 | reserved0 |
| 4,064 | 32 | selector checksum |

`SelectorExpectation`은 고정 경로가 정한 role, 검증한 profile의 namespace/generation/digest를 받는다. selector가 자기 role/profile을 기대값으로 공급하지 않는다. 선택 control의 length/digest/header namespace/generation/kind/slot/index를 모두 검산한 다음에만 그 body를 읽는다. DWB는 ref slot이 control 저장 공간을 고르고 내부 XDW1 namespace/generation을 기존 codec으로 대조한다. 이 slot과 DWB image pool index0..1009는 서로 다르다.

물리 예약 파일에는 logical prefix 뒤에 이전 bytes/여유가 남을 수 있다. prefix 길이는 선택 reference가 정하며 그 exact bytes 전체를 hash/parse한다. 물리 파일 길이는 해당 고정 container 길이와 별도로 대조한다. suffix를 control로 스캔하지 않는다. decoder에 suffix를 붙여 넘기면 trailing으로 거절한다. 비선택 공간은 복구 후보가 아니며 소유/불명 상태가 해결되기 전 덮어쓰지 않는다.

## 5. FILEMAP root·chunk·entry

### Root

한 root는 최대8,192개의 file mapping을 선택한다. 이는 기본8 GiB에서 최소1 MiB extent 파일이 가질 수 있는 보수적 wire 상한이며 그만큼의 실제 파일을 다른 control/WAL 비용 없이 admission한다는 뜻이 아니다. 미래 qualification-only64 GiB 제안은 이 계약의 profile에 추가하지 않는다.

root payload는 아래64-byte prefix 뒤 chunk descriptor96×K다. N=0이면K=0, 그 밖에는K=ceil(N/64)이며0≤K≤128이다. exact root length는224+96K, 최대12,512 bytes다.

| payload offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | 전체 file count N |
| 4 | 4 | chunk count K; 공통 item count와 같음 |
| 8 | 8 | entry의 mapped reserved length 합계 |
| 16 | 8 | 첫 FileId; N0이면0 |
| 24 | 8 | 마지막 FileId; N0이면0 |
| 32 | 2 | chunk당 최대 entry64 |
| 34 | 2 | entry bytes96 |
| 36 | 28 | reserved0 |

각 chunk descriptor는 ControlRef64, offset64 entry count:u16,66..72 zero,72 first FileId:u64,80 last FileId:u64,88..96 zero다. ref는 같은 root의 file slot/generation, kindChunk3, 연속 object index0..K−1이다. nonfinal count는64, final은N의 나머지(나머지0이면64)다. count/first/last는 실제 chunk와 같아야 한다. 각 descriptor 범위는 nonzero valid FileId이며 `previous.last < next.first`; 해당 descriptor의 count1이면first=last, count>1이면first<last다. root 전체 범위도 전체 N1이면first=last, N>1이면first<last다. length도160+96×count와 일치해야 한다.

### Chunk와 고정 container

chunk payload는 정확96×count bytes이고 별도 중복 header가 없다. count1..64, exact length256..6,304이다. chunk i의 파일 offset은`16,384×(1+i)`다. root는offset0에서 ref 길이만 읽는다. 계산은 checked이며 전체 container는`16,384×129=2,113,536` bytes다. 어느 세대도 selected root가 선언하지 않은 chunk slot을 읽어 mapping을 보충하지 않는다.

두 container 중 비선택이고 더 이상 reader가 빌리지 않는 한쪽에 새 generation의 모든 필요 chunk와 root를 완성하여 sync한 뒤 selector를 교체한다. selector가 불명이면 양쪽 generation을 보호한다. 제3 generation을 몰래 만들거나 다시 시작할 때마다 추가 staging container를 생성하지 않는다. selection/readers/IO의 구현은 후속 의무다.

### Entry96

| entry offset | bytes | 값 |
| --- | --- | --- |
| 0 | 8 | valid FileId(epoch/counter 모두nonzero) |
| 8 | 8 | mapped reserved length. 1 MiB의 양의 배수, profile owned quota 이하 |
| 16 | 8 | allocator metadata의 nonzero ObjectId |
| 24 | 8 | allocator object의 nonzero Generation |
| 32 | 24 | bootstrap PageRef=`(같은 FileId,page0,incarnation1)` |
| 56 | 32 | canonical data filename24 bytes + zero8 |
| 88 | 1 | Live1 또는 Retained2 |
| 89 | 7 | reserved0 |

FileId 오름차순이며 중복은 거절한다. FileId/이름의 reuse·alias는 허용하지 않는다. 모든 chunk의 범위·합계도 엄격히 이어져야 한다. Retained는 drop 이후에도 WAL/DWB/checkpoint 등 live owner가 요구하는 mapping이며 여전히 존재·길이·quota 계산에 포함한다. state0이나 Staging 같은 다른 값을 임시 성공 상태로 받아들이지 않는다. 미공개 신규 파일/growth intent의 별도 staging descriptor가 정의되기 전에는 해당 효과를 구현하지 않는다.

이 F01은 **별도 datafile header를 두지 않는다.** page0은 allocation metadata이며 extent e의 metadata `(FileId,64e,incarnation1)`을 동일 allocator ObjectId/Generation으로 해석한다. 신뢰 root는 선택 FILEMAP의 owner 필드다. allocator page가 자기 owner/incarnation을 기대값으로 되돌려 bootstrap 순환을 만드는 경로는 없다. 일반 data page의 owner는 allocation/객체 경로에서 받으며 FILEMAP의 allocator owner를 모든 page owner로 사용하지 않는다.

mapped reserved length는 이미 영속 공개한 접근 범위다. 파일의 실제 길이가 작으면 실패하며 이를 zero page로 메우지 않는다. 더 큰 길이는 crash 중 growth의 실제 charged bytes일 수 있으므로 자동 truncate하지 않는다. 같은 prefix의 검증된 growth/staging descriptor로 설명하고 실제 길이를 charged에 포함하는 reconciliation은 후속 소유 의무다. 설명되지 않은 추가 bytes를 free token이나 미소비 growth로 임의 인정하지 않는다. codec은 metadata 관찰/I/O를 수행하지 않는다.

### Bounded cursor와 소비 순서

root 단독 decode는 root 내부 형태만 검산한다. `FileMapCursor`는 root에 선언된 다음 chunk의 exact ref/offset을 반환하고 caller가 읽은 그 bytes만 검산한다. 한 chunk의 모든 entry·first/last/count·namespace/generation/hash를 검사한 후에만 진행점·누적 count/length를 갱신한다. 오류는 cursor 상태도 보존한다. 끝의 `finish`는 K개를 모두 받았고 N/합계/마지막 FileId가 root와 같아야 성공한다. 중간 valid prefix를 완전 FILEMAP으로 반환하지 않는다.

BOOT_RESERVE/DWB repair의 후속 owner는 selector와 **전체 FILEMAP cursor finish**를 먼저 통과한다. 이후 lookup은 root의 최대128개 범위에서 chunk를 찾고 그 chunk를 다시 검증하여 entry를 읽을 수 있다. root≤12,512 bytes와 현재 chunk≤6,304 bytes만 caller buffer에서 빌리며 전체 file/extent inventory를 RAM에 복제하지 않는다. reader가 선택 generation을 안정화하고 필요 bytes를 다시 읽을 때 hash를 확인해야 한다. cursor 완료는 mapping 파일의 bytes 검증이며 모든 home data/allocator page 검증 또는 file 존재/I/O sync 완료가 아니다.

합계는 mapped reserved lengths를 한 번 가산하고 profile owned quota 이하인지 확인한다. 최종 `actual owned bytes + outstanding growth <= quota`에는 이 합계 밖 control/WAL/실제 추가 길이도 포함해야 한다. 기존 extent 안 Free/Reserved page token을 이 byte 합계에 다시 더하지 않는다. protected1 GiB는8 GiB 안의 partition이며 protected extent bytes를 이중 차감하지 않는다.

## 6. 고정 profile payload와 실제 예약 항

Profile은 공통 header128+payload3,936+checksum32=정확4,096 bytes다. header의 slot/index/item count는0이다. payload의 다음 table 밖 bytes는 모두0이다. profile ID와 모든 값이 아래 정본 tuple에 정확히 맞아야 한다. ID만 검사하거나 가까운 작은 profile로 자동 변환하지 않는다.

| payload offset | 형식 | 값 |
| --- | --- | --- |
| 0 | u32 | ID1=buffer4 MiB, ID2=16 MiB, ID3=64 MiB |
| 4 | u32 | flags0 |
| 8/16/24/32 | 각u64 | owned8 GiB / protected1 GiB / WAL256 MiB / DWB16 MiB |
| 40 | u64 | ID에 대응하는 buffer bytes |
| 48/56 | 각u64 | metadata8 MiB / locator1 MiB(앞8 MiB에 포함) |
| 64/72 | 각u64 | private per transaction8 MiB / private total64 MiB |
| 80/88/96 | 각u64 | system16 MiB / flush4 MiB / maintenance12 MiB(뒤 둘이 앞16 MiB를 분할) |
| 104 | u64 | trace4 MiB |
| 112/116/120/124/128 | 각u32 | transaction32 / mutation256 / logical payload1 MiB / user snapshot32 / FD128 |
| 132/134 | 각u16 | tree height16 / max indexes8 |
| 136 | u32 | page bytes16,384 |
| 140/142/144 | 각u16 | extent pages64 / DWB batch64 / DWB slots1,010 |
| 146 | u16 | reserved0 |
| 148 | u32 | FILEMAP max entries8,192 |
| 152/154 | 각u16 | FILEMAP chunk entries64 / reserved0 |
| 156 | u32 | FILEMAP physical slot bytes16,384 |
| 160 | u64 | FILEMAP container bytes2,113,536 |
| 168 | u64 | bootstrap descriptor physical slot bytes1,048,576 **제안** |
| 176/180/184 | 각u32 | bootstrap slots2 / selector bytes4,096 / WAL frame max1,048,576 |
| 188..3936 | bytes | reserved0 |

이 table은 capacities와 concrete representation geometry다. 아직 미정인 opcode별 최소 pin/FD/메모리, forward/CLR/terminal 비용, bootstrap/ATT/system obligation 실제 요구량을0으로 저장하여 충족한 것으로 표시하지 않는다. 그런 요구량을 사전 계산·예약하는 F05와 실행이 없으면 BOOT_RESERVE/operation admission/READY를 성공시키지 않는다. 기본 WAL·protected reserve를 늘리거나 reader를 강제 만료하여 맞추지 않는다.

### DB 생성 때 유지해야 할 고정 disk 몫 제안

| 항 | 고정 bytes | 포함/미포함 |
| --- | --- | --- |
| DWB 전체 | 16,777,216 | image/control/selector/staging/여유 전부 포함. 별도8 KiB를 다시 가산하지 않음 |
| FILEMAP 두 container | 4,227,072 | 각2,113,536. 비선택 container 자체가 새 generation 준비 공간 |
| FILEMAP selector+stage | 8,192 | DWB와 별도 |
| immutable profile | 4,096 | 정확 full control |
| REBASE 두 descriptor slot | 2,097,152 | 각1 MiB라는 새로운 capacity 제안; 실제 descriptor/roots fit는 아직 미증명 |
| CURRENT selector+stage | 8,192 | 기존/새 closure 선택용 별도 공간 |
| WRITER selector+stage | 8,192 | epoch 선택용 별도 공간 |
| 위 알려진 항 소계 | **23,130,112** | 약22.05859375 MiB. 1 GiB fixed maintenance 안에 들어가며 전체8 GiB에서도 한 번 charge |

1 MiB REBASE 슬롯은 작은 header가 아니라 최대 descriptor/roots를 보관할 fixed physical capacity 제안이다. 대형 immutable ownership inventory 전체가 그 안에 든다고 가정하지 않는다. 그 inventory의 chunk closure·old/new 보존 peak, normal fuzzy checkpoint 두 세대+설치 peak, recovery epoch/header의 설치 여유, aging 진행점, 남은 abort reserve는 **위 소계에 아직 포함되지 않았다**. 별도 owner가 실제 크기를 제출하여 F05 전체식을 완성해야 하며, 이 소계를 전체 fixed maintenance 최종값으로 쓰지 않는다. descriptor가 슬롯을 넘으면 effect 전 미지원이며 필요한 형식/슬롯 크기를 D2로 다시 검토한다.

selector staging rename은 준비 이름을 소비한다. 이전 selected inode의 열린 handle을 계속 보유한 채 새 staging을 또 만들어 peak를 숨기지 않는다. 후속 owner는 필요 없는 old handle을 닫고 정해진 staging capacity token 안에서 같은 고정 이름을 다시 준비해야 한다. set_len 성공만으로 OS 물리 공간 선확보를 주장하지 않는다. 현재 FileIo에는 fallocate가 없으므로 실제 요구되는 선할당은 bounded write+sync 또는 별도 승인 port 확장·검증을 통해 수행해야 한다. 파일시스템 블록/metadata와 외부 ENOSPC는 논리 owned-byte quota만으로 해결되지 않는다.

## 7. API·오류·기존 port 수신

소유 모듈은 `storage::control`이며 `control.rs`와 `control_tests.rs`에서 구현한다. 공유 mod 연결은 root 소유이며 dependency를 추가하지 않는다. 실제 첫 소비자 전까지 local dead_code 허용 사유를 명시하며 공개 API로 re-export하지 않는다.

| 제안 API | 범위 |
| --- | --- |
| `decode_bootstrap_profile` | 고정 root bytes의 namespace와 exact capacities를 검산. optional 기존 namespace 기대값과 대조. resource reservation 아님 |
| `ControlRef::{encode,decode}` | 정확64-byte reference, kind별 slot/index/length 검사 |
| `encode/decode_selector` | exact role/profile binding과4 KiB 표현. namespace I/O 없음 |
| `decode_filemap_root(input,expected_ref,profile)` | root 내부 descriptor/range/count/합계 검산; 전체 chunk 존재의 증명 아님 |
| `FileMapCursor::{next,accept,finish}` | caller bytes를 bounded 검산; 오류 상태 보존 |
| `encode_filemap_chunk/root` | 모든 항목·마지막 항목·수치·output 검산을 최초 쓰기 전에 끝냄 |
| `FileMapEntry::metadata_identity(namespace,extent_base)` | 검증한 profile에서 받은 namespace와 file 길이 안64-page aligned base의 allocator owner/incarnation1 기대값 산술. page load/allocated capability 아님 |
| `data_file_name`, `control_file_name` | caller buffer/fixed FileName의 canonical 이름 계산; directory 탐색 없음 |

오류는 InvalidInput, UnsupportedFormat, UnsupportedProfile, Corrupt, IdentityMismatch, Incomplete, NoSpace, Exhausted다. caller shape/잘못된 기대값은 InvalidInput; version/kind 미지원은 UnsupportedFormat; ID/정본 capacity tuple 불일치는 UnsupportedProfile; checksum/길이/reserved/순서/상태 손상은 Corrupt; 유효 identity/generation/profile/selected hash와 불일치는 IdentityMismatch; 미완료 cursor는Incomplete; output 부족은NoSpace; 발급 checked 증가 실패는Exhausted다. 여러 오류의 우선순서는 약속하지 않는다. 모든 encoder는 오류에서 전체 output 보존, 성공에서 사용 prefix 뒤 tail 보존이다. decoder/cursor는 borrowed bytes와 고정 작은 상태만 사용한다.

실제 효과 수신은 foundation `FileName`, `DirectoryPort::acquire_writer`, `WriterNamespace::{create_new,open_existing,metadata,publish_selector,sync_directory,remove_retired}`와 `FileIo::{read_at,write_at,set_len,sync_all}`이다. LinuxDirectory는 no-follow·regular/single-link·lease 보존과 signed64 file offset을 제공한다. foundation `read_exact_at`/`write_all_at`은 short I/O를 처리하지만 transaction durability를 제공하지 않는다. 준비 file write+file sync → typed selector rename → directory sync 순서를 상위 owner가 수행한다. uncertain mutation을 retry 성공/삭제로 숨기지 않는다.

port 확장 시 정확 영향은 `crates/foundation/src/namespace.rs`의 Selector/예약 이름, `crates/platform/src/namespace.rs`의 reserved-name 처리, `crates/testing/engine-worker/src/fault.rs::publish_selector`의 exhaustive target mapping이다. namespace/fault 시험에서 새 두 target의 rename·dirsync·Unknown·재장애를 실행하기 전 실제 설치 완료를 주장하지 않는다. 기존 DEV2 CURRENT/WRITER bytes나 public 동작은 이 표현 제안 때문에 바꾸지 않는다.

## 8. 독립 vector와 최초 시험 분모 제안

제품 codec 이전에 Python 표준 `struct/hashlib`의 독립 계산을 사용했다. 아래 입력·offset·규칙에서 exact bytes를 만들며 candidate encoder를 expected로 호출하지 않았다. 계산값은 문서의 vector 표에 고정했다. 원본 generator·11개 full control·2개 reference·entry literal와 `generation.json`은 `artifacts/dev3/control-format/`에 보존했다. `python3 artifacts/dev3/control-format/independent_vectors.py`는 보존 bytes와 고정 hash가 모두 일치하는 `LITERALS_MATCH`를 반환했으며, `--write`는 신규 파일만 생성한다. 이 literal 계산 때 제품 실행은 NOT_RUN이었다. 표현 구현의 제품 시험 결과는 별도 보존 receipt로 판정한다.

기본 namespace는 DB=[0x11;16], timeline17이다. profile generation19, FILEMAP generation23, selector generation29다. profile P1/P2/P3은 ID1/2/3이며 그 밖의 모든 값은 §6과 같다. FILEMAP slot0을 사용한다.

- M0: file0/chunk0/sum0의 empty root.
- M1: file1, FileId0x0000000100000002, mapped length1 MiB, allocator owner5/gen6, bootstrap page0/inc1, Live1. root1개와 chunk1개.
- M70: i=0..69의 FileId=0x0000000100000002+i, length=(1+i mod3) MiB, allocator owner5+i/gen6, Retained2 if i mod5=0 else Live1. root와 chunk64/6 두 개.
- S1: P1의 full digest를 결합하고 M1 root를 선택하는 FileMap selector.
- S2: P1을 결합하고 기존 DWB Empty160 vector(gen19,batch0,full SHA `7733594cfa9fe7ed3e85a0e4b33d4bb8e4193f0b8eca7669071147c60b227993`)를 control slot1에서 선택하는 Doublewrite selector.
- Ref1/Ref2: S1/S2에 들어간 정확64-byte selected ref literal.

full object는 P3개+M0한개+M1두개+M70세개+S두개=**11개**, 별도 reference literal2개다. M70의 합계 length는139 MiB다. 아래 exact hashes/literals는 Python 표준 `struct/hashlib`로 독립 계산한 XDC1 bytes다.

| object | exact bytes | trailer checksum(hex) | full logical SHA-256(hex) |
| --- | --- | --- | --- |
| P1 | 4,096 | `036ef64ce53365dc4d5170288ff26d3cf91437bfadccff4c75bb33fbbaa65ab6` | `985651bdbfde061063f76265665c2aff331c9b01486e0be4de60223d6ec327d8` |
| P2 | 4,096 | `bf29e1fa2fbbc54260cf3da2321dbb29bcb908e6fa4a394395d470db9adecf92` | `115006ecc057bb94604e8030d3f352276c13c514e58168dae9edf9de981aef91` |
| P3 | 4,096 | `ad1a7d7b0efe62b07d3de448f50e569da8b40ff6a08e16e49974c690ef217224` | `38e9af5ae539ffb1529b5cee5455a4160bbd3aa8943dbf44104434b1707ecf7d` |
| M0-root | 224 | `092408b76d67d3d1d1da222fec853172a6fb46076664c5985724feea64d378f3` | `4de00b4d05bef366fdbfead38a0d238f673efd77d927554487f05b7c1a9456c4` |
| M1-root | 320 | `0afb6c98977e24bec160c6a8e0ddb3628038c730e14782738073c50ad6cf8f5d` | `06caacaeac81140d34fe6c9dbf65060028b6951998dc29618f454312587173be` |
| M1-chunk0 | 256 | `5e10c6a9484ff11f23902b6f8614ea2b9734d02a522d473ca5481f64aeb75fb2` | `c5bbe40771d48ce2f83bbf49ceb9b44191d2f17386a3994d7f19d32fc08fcfa8` |
| M70-root | 416 | `131129cda95d45fa8aa94dd8a679ff205d9e8123265cf3197b6e6b21c62a8093` | `a069d6cd39dbeaf7d3ad26b23bd70f2507c2298438f9303e76c32a4640290b5b` |
| M70-chunk0 | 6,304 | `effea7496fa3b5cf0af4af66f586b23c89b6a4b8866cbcbd52a09ad7f7acd167` | `5fa616a0b70e1a4bdcd81dd29469df685b6653ec759d28e209023d214ef06f8b` |
| M70-chunk1 | 736 | `7d89a0f08cbc3c21a40ff8c1587c5761d986300ded4eda5e0d52c54e3ece6bf1` | `27cbe9f42f522052d197b98059d53db68175992ccdc4d78ed3ae9d3a1a3e9088` |
| S1 | 4,096 | `062f24cc14468a4390506b97d0d1499723b11771d4f1c3963253ef8f419caea3` | `0feff1e11c302c513f813daf41fdadc3c515ab09d5e13e0d0cce7db984b90b98` |
| S2 | 4,096 | `fd63d17fe76f594c7c7dc86386c272c77c0fc4859872d92e700cc1ac08f373f6` | `337b72e5dfc2ff443fc394661903d53acb4cab66495cdb4f3cfd8cef00bb0ce7` |

각 literal은 아래 연속 hex를 decode한 bytes다. Ref1/2에는 별도 checksum이 없고, digest 필드가 선택 대상 전체 bytes를 결합한다.

- Ref1: `020000000000000017000000000000004001000000000000000000000000000006caacaeac81140d34fe6c9dbf65060028b6951998dc29618f454312587173be`
- Ref2: `04010000000000001300000000000000a00000000000000000000000000000007733594cfa9fe7ed3e85a0e4b33d4bb8e4193f0b8eca7669071147c60b227993`
- M1 entry96: `020000000100000000001000000000000500000000000000060000000000000002000000010000000000000000000000010000000000000064332d646174612d3030303030303031303030303030303200000000000000000100000000000000`

계산 절차는 zero-filled header/payload에 표의 offset·폭으로 `struct.pack_into("<…")`를 적용하고, name은 exact ASCII, padding은 zero로 둔 뒤 `hashlib.sha256(header+payload).digest()`를 trailer에 붙이는 것이다. child chunk를 먼저 만들고 그 전체 SHA를 root descriptor에 넣으며, 완성한 root/profile 전체 SHA를 selector에 넣고 selector checksum을 마지막에 계산한다. M70은 위 i 순서 그대로 chunk0의 i0..63과 chunk1의 i64..69로 나눈다. DWB 대상은 동결 XDW1 literal의 전체 SHA만 수신하며 XDC1으로 재포장하지 않는다. 이 계산은 Rust codec·제품 소스·제품 dependency를 import하지 않았다.

고정 시험10개 제안은 다음과 같다. 프로필/범위를 실행 실패 뒤 축소하지 않는다.

| test | 최초 분모 |
| --- | --- |
| `control_profile_literals` | P1/P2/P3 exact4,096 bytes, 독립 표의 모든 capacity field와 zero pad·digest 검산 |
| `control_filemap_literals` | M0/M1/M70의6 objects와 전체71 entries, root/cursor 합계·모든 field·ref digest 일치 |
| `control_selector_bindings_and_lengths` | S1/S2 exact bytes·Ref2 literals. selector 짧은 입력0..4095×2와 trailing2=8,194 거절. 각4,096-byte 단일 bit flip×2=8,192 checksum 거절. expected role/DB/timeline/profile gen/profile digest 및 target gen/digest/length/slot mismatch9 |
| `control_envelope_corruption_is_rejected` | 아래 H16, digest 재계산 후 거절. P1/M1root/M1chunk의 모든 짧은 입력+각trailing, 합계4,675. checksum 없는 partial prefix를 valid control로 반환하지 않음 |
| `control_profiles_require_exact_capacities` | 아래 P14 거절; ID별 buffer 교차3×3=9중정상3/거절6. 선언 field 전체를 독립 정본 tuple과 비교 |
| `control_filemap_cursor_rejects_mixed_inventory` | 아래 F24. 실패 cursor 상태 보존, 누락/중복/역순 feed 및 early finish 거절. 세대/slot/DB/timeline digest를 맞춘 잘못된 chunk도 거절 |
| `control_names_and_bootstrap_ownership` | FileId counter1..8192의 canonical name·page0/inc1·metadata base0/64 계산. 각 주소 case는 독립2MiB file이고8192개를 합친 admitted inventory가 아님. 잘못된 이름12 및 bootstrap/owner/length 손상12 거절 |
| `control_filemap_exact_bounds` | file count0/1/63/64/65/8192 정상6,8193 거절. N8192는 wire 상한 시험이며8GiB DB admission fixture가 아님. lengths0/1MiB−1/1MiB/8GiB/8GiB+1MiB의수락2/거절3; sum overflow·quota 초과 거절 |
| `control_encoders_preserve_outputs` | P1(4096)/M1root(320)/M1chunk(256)/S1(4096)의 모든 짧은 output8,768개 보존. 잘못된 마지막 entry/ref와 모든 semantic caller failure 전체 보존, 정상4개 tail 보존 |
| `control_generated_inventory_profiles` | seed0..63×sample0..127=8,192 inventories. 아래 typed 원자료로 file/range/count/length를 독립 계산하고 모든 entry·chunk/cursor·selector field 검산 |

H16은 magic,version2,kind0,header bytes127,flags1,total−1,total+1,DB0,timeline0,generation0,reserved60,reserved127,payload length−1,payload length+1,Profile object index1,Profile item count1이다. P14는 ID0,ID4,owned+1,protected+1,WAL+1,DWB+1,ID1의buffer8MiB,metadata+1,FD129,DWB slots1011,bootstrap slot+1,WAL frame+1,payload reserved188=1,payload 마지막byte=1이다. 모두 checksum을 다시 계산하여 구조/정본 검사가 작동하는지 확인한다.

F24는 (1)root N/K불일치,(2)N8193,(3)root sum quota초과,(4)첫 descriptor count63인nonfinal,(5)ref index불연속,(6)ref 다른generation,(7)ref 다른slot,(8)ref length불일치,(9)첫FileId0,(10)first>last,(11)chunk rangeoverlap,(12)root 마지막FileId불일치,(13)chunk header 다른DB,(14)다른timeline,(15)다른generation,(16)다른slot,(17)다른index,(18)entry count불일치,(19)entry 역순,(20)entry duplicate FileId,(21)마지막entry owner0,(22)파일길이합계불일치,(23)같은chunk중복feed,(24)끝chunk누락후finish다. semantic 손상은 해당 control/checksum 및 root의 ref digest를 새 bytes에 맞춰 적절히 재계산하여 단순 checksum failure로 대체하지 않는다. 파일 범위·누적 상태 mismatch만 겨냥한 경우 unrelated field는 정상으로 유지한다.

이름12개는 empty,`.`/`..`,slash,absolute slash,NUL,space,uppercase hex,짧은hex,긴hex,다른prefix,entry FileId와 다른valid canonical name이다. bootstrap/owner/length12개는 FileId epoch0/counter0,bootstrap 다른FileId/page1/inc2,owner0/owner generation0,length0/nonmultiple/overquota,state0/state3이다.

generated n=128×seed+sample, file count=1+n mod130, slot=n mod2, generation=23+n, selector generation=29+n, profile ID=1+n mod3다. entry i의 FileId epoch1/counter2+i, length=(1+(n+i)mod3)MiB, owner=5+i/gen6, state=Retained iff(n+i)mod5=0다. expected name/first/last/count/sum과 wire bytes는 이 원자료에서 계산한다. chunk순서는 index0부터, count64의 고정분할이며 duplicate/누락시험은 별도 F24다. 작은 generated corpus를 N8192 경계나 F05의 큰 실제 allocation inventory 접근량으로 대체하지 않는다.

실제 선택 atomicity·old/new closure·FILEMAP grow 장벽·namespace repair·full DWB batch·CURRENT/WRITER rebase 인계와 반복 crash의 고정 peak는 별도 effect 모델/실행에 남는다. 위 형식 수용·계산·단위 시험은 S2/F04/F05 또는 DEV3 전체 완료가 아니다.

자체 첫 실행 `artifacts/dev3/control-supply-r1/validation.json`은 보존 export에서10개 시험을 통과했지만 Clippy의 extent-end 검사식 단순화 지적으로 전체 FAIL을 보존했다. 의미가 같은 `Option::is_none_or` 검사로 한 줄을 바꾼 r2는 `artifacts/dev3/control-supply-r2/validation.json`에 source/binary/contract hash·명령·원로그를 보존했고, 동일 export의10개 시험(9.13초), targeted rustfmt, engine lib/tests all-features Clippy가 모두 PASS다. 고정 분모는 줄이지 않았고 generated8,192 inventories의 entry536,448개를 검산했다. 제품 source 두 파일과 실행 사본은 동결했으며, workspace shared module·namespace port는 수정하지 않고 시험 export에서만 module을 연결했다. 실행 후 이 상태 문단은 결과를 수신하여 갱신했으므로 r2의 실행 당시 contract hash와 구분한다.

root가 최종 source 전체를 독립 검토하고 별도 사본에서 고정10시험을 통과했다(`artifacts/dev3/control-independent-root-_g_0xw1y/review.json`). metadata extent 주소·cursor 실패 상태 보존·최종 item까지의 encoder 사전 검산을 확인했다. 이 상태 수신은 실행 계약 bytes와 구별하며 통합 source의 회귀 결과는 별도 기록한다.

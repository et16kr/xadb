# DEV3 영속 recovery obligation·WAL 의미 그룹

2026-10-21 · **D3-OBLIGATION-F01/F02 초안 / D2 독립 검토 대기 / 제품 구현·실행 없음**

[예약 인계](disk-storage-design.md#recovery-reservation) · [변경 규약](disk-storage-design.md#mutation-protocol) · [REBASE](disk-storage-design.md#recovery-rebase) · [profile](disk-storage-design.md#capacity-profile) · [WAL frame](dev3-wal-format.md) · [거래](dev3-transaction-format.md) · [allocator](dev3-allocation-format.md) · [undo](dev3-undo-format.md) · [실제 크기 계산](dev3-resource-bounds.md) · [M04](dev3-recovery-model.md) · [M06](dev3-reservation-model.md) · [소유 범위](../implementation/dev3-implementation.md#wave-2)

이 문서는 거래·system 작업의 **남은 복구 의무를 정해진 bytes로 인계하는 최소 표현**과 그 표현을 WAL의 효과·진행점에 결합하는 규칙을 제안한다. 작성자는 `/root/dev3_model`, 계약·통합 검토자는 `/root`다. 이 단계는 계약만 작성한다. 기존 제품 소스, 공개 API, frame codec, runner를 변경하거나 실행하지 않는다.

descriptor의 형식 수락, checkpoint closure 검산, 실제 page 적용·복구 성공은 서로 다른 결과다. 기존 WAL `Opaque`를 descriptor로 간주하지 않는다. 아래 의미 그룹도 실제 before page·native record·자원 profile을 대조하는 적용기가 없으면 복구 완료를 뜻하지 않는다. **TxnRef incoming count의 home 위치, retirement queue, CURRENT/WRITER/checkpoint의 최종 선택 형식, opcode별 F05 상한은 아직 미완료**다. 이 항목이 필요한 opcode를 구조만 맞는 payload로 성공시키지 않는다.

## 1. 선택과 독립 검토 대상

| 선택 | 이유·대안 |
| --- | --- |
| OwnerRef56 + 고정1536-byte descriptor | 이미 동결한 TxnRef/SystemWorkRef48을 그대로 감싼다. recovery thread나 새 writer epoch로 기존 claimant를 대체하지 않는다. 가변 JSON·Rust struct dump는 사용하지 않는다 |
| descriptor의 full after-state와 정확한 previous owner LSN·이전 descriptor SHA | WAL 중복·역순·누락과 다른 owner generation의 상태 덮어쓰기를 거절한다. 단순 `remaining -= n`만 저장하여 재실행 때 두 번 차감하지 않는다 |
| 고정5개 의무 종류의 consumable·peak 분리 | CLR/SMO/terminal/status/control의 잔여 비용과 공유 작업 공간의 최대 동시 사용량을 구별한다. 모든 단계의 frame/memory를 누적 합산하지 않는다 |
| immutable checkpoint inventory + 이후 WAL의 정확한 before/after ownership delta | 아직 REDO하지 않은 home allocator를 최신 정본으로 읽는 순환을 피한다. mutable PageRef 하나와 hash만 저장한 것을 자체 완결된 복원 자료로 인정하지 않는다 |
| ownership root는 checkpoint 공통 closure에 한 번, owner별 inventory digest/count는 descriptor에 | capture 때 descriptor의 논리 bytes를 바꾸지 않는다. checkpoint마다 root가 달라져 이후 WAL의 previous descriptor SHA가 어긋나는 문제를 피한다 |
| 명시 native before/after·edge multiset·count delta·inventory delta·page redo의 한 frame | 같은 UndoRef를 가리키는 두 edge도 둘로 센다. checksum만 맞는 opaque 효과를 원자 그룹으로 승인하지 않는다 |
| 큰 inventory는 고정 두 bootstrap descriptor slot 밖의 세 immutable bank | current/previous가 서로 다른 bank를 요구할 때 새 closure를 만들 공간을 따로 charge한다. 그 peak를 1 MiB checkpoint slot 안에 숨기지 않는다 |

`bounded system owners=4`, inventory 세 bank 및 이 문서의 tag/bytes는 **새 D2 제안**이다. 상위 계약의 수치가 이미 이것을 정했다고 쓰지 않는다. `/root`는 immutable inventory와 WAL delta의 방향을 수용했지만 아래 정확한 형식·상한·검산은 아직 검토 전이다. 제품 source 구현은 검토 수용 뒤 별도로 배정한다.

## 2. 공통 identity와 ref

모든 다중 byte 정수는 little endian, reserved bytes는0이다. 길이·개수·곱셈·합산·LSN/sequence 증가에는 checked arithmetic을 사용한다. wrap, 모르는 tag, 부분 body, trailing bytes를 수락하지 않는다.

`OwnerRef`는56 bytes다. offset0은 Transaction1/System2, offset1..7은0, offset8..55는 기존48-byte TxnRef 또는 SystemWorkRef다. DB/timeline은 신뢰한 기대 namespace와 같아야 한다. Transaction의 original writer epoch는 현재 recovery writer epoch와 달라도 된다. System kind는 기존 AllocationMaintenance1/StructuralChange2/Aging3/Checkpoint4만 허용한다. None은 optional field에서만56 zero로 나타내며 standalone owner는 nonzero다.

control 참조는 병렬 control 계약의 제안과 같은64 bytes를 사용한다. offset0 kind,1 bank/slot,2..3 zero,4..7 object index:u32,8..15 generation:u64,16..19 exact logical encoded length:u32,20..31 zero,32..63 해당 logical bytes 전체의 SHA-256이다. namespace는 ref를 소유한 checksummed 상위 object와 신뢰한 expected context에서 전달한다. 물리 slot의 나머지 padding은 hash 길이에 포함하지 않는다. generation/length는 nonzero다.

Profile ref는 kind1/slot0/index0/length4096이다. 이 문서는 control tag **OwnershipRoot5, OwnershipChunk6**을 제안한다. 기존 Profile/Filemap/DWB tag의 변경 권한을 가져오지 않는다. tag 등록은 control owner와 통합 담당의 D2 수락 이후다. root는 bank0..2/index0, chunk는 같은 bank/index0..chunk_count−1이다. 뒤에서 설명하는 chunk aggregate hash 때문에 root에 chunk ref 배열을 전부 둘 필요는 없다.

checkpoint가 제공해야 할 기대값은 DB/timeline, immutable profile ref, capture LSN, 선택된 root ref 및 owner 표다. 입력 descriptor나 chunk에서 expected 값을 복사해 자기 자신과 비교하지 않는다. reference bytes의 형식 수락만으로 대상 object의 존재·hash·namespace 검산이 완료되지는 않는다.

## 3. Owner descriptor:1536 bytes

magic `XOD1`, format1이며 checksum까지 정확1536 bytes다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | `XOD1` |
| 4 | 2 | format1 |
| 6 | 1 | TransactionActive1, TransactionAborting2, TransactionCommitted3, TransactionAborted4, SystemActive5, SystemFinished6 |
| 7 | 1 | flags=0 |
| 8 | 4 | descriptor bytes1536 |
| 12 | 4 | reserved0 |
| 16 | 56 | OwnerRef |
| 72 | 8 | `apply_lsn`, nonzero: 이 owner의 마지막 의미 그룹 |
| 80 | 8 | `previous_owner_lsn`; 최초 생성만0, 그 외 `<apply_lsn` |
| 88 | 8 | `birth_lsn`, nonzero, `≤apply_lsn` |
| 96 | 64 | 선택된 immutable Profile ref |
| 160 | 192 | Transaction은 기존 status core; System은192 zero |
| 352 | 128 | SystemProgress; Transaction은128 zero |
| 480 | 64 | reserved0. mutable home inventory root를 넣는 우회 슬롯이 아님 |
| 544 | 8 | `inventory_change_lsn`,0 또는 `≤apply_lsn` |
| 552 | 8 | 현재 이 owner의 ownership item 수 |
| 560 | 32 | 아래 canonical owner inventory stream SHA-256 |
| 592 | 8 | 아직 물리 실현되지 않은 growth extent 수 |
| 600 | 8 | 그 bytes; 정확히 앞 count×1,048,576 |
| 608 | 8 | 이 owner가 태어난 뒤 소비한 WAL frame bytes 누계 |
| 616 | 8 | 이 owner가 소비한 control write bytes 누계 |
| 624 | 8 | 완료한 의미 그룹 수; 최초1, 이후 정확+1 |
| 632 | 8 | reserved0 |
| 640 | 640 | 고정 순서의 RemainingBudget128×5 |
| 1280 | 8 | 단계별 frame pin peak의 최댓값 |
| 1288 | 8 | 단계별 memory byte peak의 최댓값 |
| 1296 | 8 | 단계별 lock slot peak의 최댓값 |
| 1304 | 8 | 단계별 registry slot peak의 최댓값 |
| 1312 | 8 | 단계별 FD peak의 최댓값 |
| 1320 | 8 | 단계별 DWB slot peak의 최댓값 |
| 1328 | 8 | 다섯 budget의 protected byte 합계 |
| 1336 | 8 | 이 owner가 요구하는 최소 WAL LSN, 없으면0 |
| 1344 | 8 | 마지막 사용자 mutation sequence; System은0 |
| 1352 | 152 | reserved0 |
| 1504 | 32 | 앞1504 bytes SHA-256 |

Transaction state와 embedded status state는 정확히 일치해야 한다. Free status는 descriptor가 아니다. embedded TxnId와 slot generation은 OwnerRef와 같고, rollback kind/head/target/next·commit q의 조건은 기존 status codec을 그대로 따른다. Committed/Aborted는 terminal로 인계하는 한 WAL 그룹에서만 사용하고 active checkpoint ATT에는 싣지 않는다. terminal 뒤에도 존재하는 status/undo의 reference 수명은 descriptor가 사라졌다는 이유로 종료하지 않는다.

SystemProgress128은 offset0 work kind:u8,1 phase:u8(Prepared1/Running2/Finishing3/Finished4),2..7 zero,8 task sequence:u64,16 cursor:u64,24 limit:u64,32 optional work-root ref64,96 work-state SHA32다. task sequence는 nonzero, cursor≤limit이다. SystemFinished만 Finished이며 cursor=limit다. work-root가 없는 최소 작업은64 zero와 SHA256(empty)를 사용한다. **현재 승인되지 않은 work-root kind나 task body는 UnsupportedWorkBody**다. 숫자 cursor만으로 aging queue·index build·SMO의 진행 의미를 검증했다고 하지 않는다.

descriptor에는 checkpoint inventory root를 중복 저장하지 않는다. 그 root는 enclosing checkpoint에서 선택하며 descriptor는 해당 capture의 자기 item stream hash/count와 결합된다. capture/rebase가 같은 상태를 새 bank로 복사해도 owner descriptor bytes와 previous-owner chain은 그대로다.

### 3.1 RemainingBudget128

다섯 row는 순서대로 Clr1, StructuralCompletion2, Terminal3, Status4, Control5다. 비활성 row도 tag는 유지하고 나머지 수치만0이다.

| row offset | bytes | 값·단위 |
| --- | --- | --- |
| 0 | 1 | 위 고정 category |
| 1 | 7 | reserved0 |
| 8 | 8 | 남은 bounded step 수 |
| 16 | 8 | 아직 소비하지 않은 WAL bytes |
| 24 | 8 | 아직 소비하지 않은 control bytes |
| 32 | 8 | 아직 실현하지 않은 extent growth bytes |
| 40 | 8 | 기존 extent 안의 예약 page token 수 |
| 48 | 8 | 예약된 undo slot 수 |
| 56 | 8 | 예약된 status slot 수 |
| 64 | 8 | 한 step의 frame pin peak |
| 72 | 8 | 한 step의 memory byte peak |
| 80 | 8 | 한 step의 lock slot peak |
| 88 | 8 | 한 step의 transient/기타 registry slot peak |
| 96 | 8 | 한 step의 FD peak |
| 104 | 8 | 한 step의 DWB slot peak |
| 112 | 8 | 이 row가 점유한 protected reserve bytes |
| 120 | 8 | reserved0 |

step0이면 row의 모든 수치가0이어야 한다. step>0의 최소 실제 비용·step 정의는 해당 opcode의 F05 표가 정한다. codec은 모르는 opcode의 비용을0이라고 가정하지 않는다. 이 표의 존재는 비용 상한이 충분하다는 증명이 아니다.

한 physical 작업을 다섯 category에 중복 charge하지 않는다. 예를 들어 CLR frame bytes 전체를 Clr에 기록했다면 그 안의 status redo bytes를 Status의 WAL에 또 더하지 않는다. Status/Control row는 CLR frame 밖의 추가 bounded 단계 비용만 가진다. 실제 opcode 표는 각각의 frame/control/write를 정확히 한 category에 대응한다. shared progress workspace는 단계마다 양도하므로 peak는 최댓값이며 consumable WAL/control/growth·protected 몫은 합계다. 같은 group 안의 동시 page pin 수는 category별 합산으로 누락하지 않고 해당 단일 step peak에 이미 포함해야 한다.

descriptor aggregate는 위 max/sum과 정확히 일치한다. growth 합계는 offset600과 같고 모든 growth item의 byte 합과도 같아야 한다. page/undo-slot/status-slot 수는 해당 category의 inventory item 수와 같아야 한다. control byte 누계는 물리 quota의 현재 점유량이 아니다. 같은 고정 control slot을 여러 번 쓴 누계가 그 파일의 소유 bytes를 여러 번 charge하게 만들면 안 된다.

### 3.2 retained/consumed/reserved

WAL 소비 누계는 audit용 단조 값이다. 의미 그룹을 한 번 채택할 때 정확한 **전체 frame bytes**를 더한다. 이미 소비한 CLR frame를 재시작 때 다시 `outstanding WAL reservation`으로 바꾸지 않는다. 해당 bytes가 실제 unlink+dirsync되기 전에는 별도 선택 namespace의 retained WAL에 남는다. 오래된 WAL이 회수돼도 descriptor의 소비 누계를 감소시키지 않는다.

다음 부등식을 각각 검사한다. 부등식의 합격과 opcode별 잔여량 충분성은 별개다.

```text
retained_WAL_bytes + Σ owner.remaining_WAL + fixed_unconsumed_WAL <= profile.WAL
owned_file_bytes + Σ owner.unrealized_growth + fixed_unrealized_growth <= profile.owned
fixed_maintenance_protected + Σ owner.protected_remaining <= profile.protected
charged[r] + outstanding_reserved[r] <= capacity[r]  (모든 자원 r)
```

existing extent의 page token은 이미 charge된 파일 안의 소유 예약이다. token 하나마다 owned file bytes에16 KiB를 다시 더하지 않는다. protected partition과 page/slot capacity에서는 그 token의 용도를 계속 보호한다. growth item만 아직 파일로 나타나지 않은 bytes다. partially created/extended 파일이 존재하면 actual physical ownership을 charge하고, selected FILEMAP/namespace·길이 장벽과 WAL의 같은-prefix delta로 미실현 부분을 계산한다. durable 기록이 없다는 이유만으로 이미 생긴 파일 bytes를 공짜로 만들지 않는다.

고정 DWB16 MiB, fixed flush4 MiB, 공유 progress12 MiB, FD·frame·bootstrap/control 교체 peak는 BOOT_RESERVE에서 별도로 한 번 확보한다. owner의 DWB/frame/FD peak는 그 고정 공급을 사용하기 위한 claim이며16 MiB pool 자체를 owner마다 복제하지 않는다. flush 몫을 foreground나 undo가 빌리지 않는다.

## 4. 큰 ownership inventory

### 4.1 Item96

inventory는 **예약 소유**의 목록이다. 모든 allocated data page나 immutable undo graph를 복사하는 전체 DB 목록이 아니다. 이미 allocated된 undo/status/retired payload의 수명 closure는 각 native owner/queue 및 필요한 WAL에 남는다. 이 문서는 그 미완료 queue를 item으로 가장하지 않는다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 1 | PageToken1, UndoSlot2, StatusSlot3, UnrealizedGrowth4 |
| 1 | 1 | budget category1..5 |
| 2 | 2 | root owner 표의 ordinal,0-based |
| 4 | 4 | reserved0 |
| 8 | 56 | 아래 kind별 identity |
| 64 | 8 | units: page/slot은1, growth는 exact bytes |
| 72 | 8 | 이 item의 마지막 ownership 변경 LSN, nonzero |
| 80 | 16 | reserved0 |

PageToken identity는 PageRef24+32 zero다. metadata page는 token이 아니며 allocator extent의 index1..63만 가능하다. UndoSlot은 UndoRef40+16 zero, StatusSlot은 기존 physical status location40+16 zero다. slot은 실제 native page의 capacity와 일치해야 하며 미래 generation을0으로 표현하지 않는다. slot reservation의 실제 admission·공간 겹침 검사는 후속 F02 allocator/slot owner가 담당한다.

Growth identity는 FileId8, extent-aligned base page8, extent count8, owner-local intent sequence8,24 zero다. count/intent는 nonzero이고 `base+64×count`와 `count×1 MiB`가 넘치면 거절한다. intent sequence는 재사용하지 않는다. DB/timeline은 상위 namespace에서 받는다. FILEMAP에서 아직 선택되지 않은 새 파일의 intent를 완성된 page token으로 바꾸지 않는다.

canonical 순서는 `(kind, FileId, physical page/base, slot, generation/intent, owner ordinal, category)`다. identity의 정수는 decoded numeric 비교를 사용한다. 서로 다른 owner/category라도 같은 page token, 같은 physical slot, 겹치는 growth interval을 동시에 소유하면 거절한다. 같은 page의 page token과 slot token을 함께 예약하거나 Reserved page와 Allocated page 내부 slot을 혼합해 이중 사용하지 않는다. 동일 physical resource의 generation만 바꾼 alias도 동시에 존재할 수 없다.

owner digest는 전체 canonical stream에서 그 owner의 item만 순서대로 고르고 ordinal2 bytes를0으로 정규화한96-byte item을 SHA-256에 넣은 값이다. 빈 owner inventory는 SHA256(empty)다. ordinal 정규화는 다른 owner의 종료/표 정렬 변경이 해당 owner의 논리 상태를 바꾸는 것을 막는다. **다른 field는 정규화하지 않는다.**

### 4.2 Root와 chunk

root header128은 다음과 같다.

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | `XOI1` |
| 4 | 2 | format1 |
| 6 | 1 | Root1 |
| 7 | 1 | bank0..2 |
| 8 | 2 | header bytes128 |
| 10 | 2 | item bytes96 |
| 12 | 4 | root logical bytes=`160+56×O` |
| 16 | 16 | DB |
| 32 | 8 | timeline |
| 40 | 8 | inventory generation, nonzero |
| 48 | 8 | capture LSN, nonzero |
| 56 | 2 | owner count O |
| 58 | 6 | reserved0 |
| 64 | 8 | chunk count C |
| 72 | 8 | item count N |
| 80 | 8 | chunk logical bytes 합계=`160×C+96×N` |
| 88 | 32 | index 순서로 모든 chunk logical bytes를 연결한 SHA-256 |
| 120 | 8 | reserved0 |
| 128 | 56×O | owner identity의 canonical 표 |
| 128+56×O | 32 | 앞 root bytes 전체의 SHA-256 |

owner 표는 Transaction owners를 `(TxnId의 DB,timeline,epoch,local,generation)` numeric순으로 먼저, System owners를 `(kind,id,generation)`순으로 뒤에 둔다. 중복은 없다. 초기 제안 O≤36은32 ATT+4 system이다. item0인 active owner도 표에 존재한다. O=0이면 N=C=0이다. C=`ceil(N/169)`이며 N=0이면0이다.

chunk는 header128, item96×I, trailer32다. 한 fixed physical slot은16 KiB이며 I=1..169, logical length=`160+96×I≤16384`다. 마지막 외에는 I=169다. padding을 decoder input에 붙이지 않고 selected exact length만 넘긴다.

chunk header는 root와 같은 magic/version, offset6=Chunk2,7=같은 bank,8=128,10=96,12=logical length,16 DB,32 timeline,40 generation,48 capture LSN,56 chunk index:u64,64 total chunk count:u64,72 first item ordinal:u64,80 item count:u32,84..87 zero,88..119 root owner-table bytes의 SHA256,120..127 zero다. first ordinal은169×index와 정확히 같다. trailer는 header+items SHA256이다.

root를 선택한 뒤 chunk index0..C−1의 **모든** bytes와 identity/length/checksum/order/중복·overlap을 검사하고 root의 aggregate digest까지 일치해야 closure 검산이 끝난다. 마지막 chunk 손상을 앞 chunk 성공으로 덮지 않는다. SHA를 다시 계산한 논리 오류도 count/owner/순서/범위 검산이 따로 거절해야 한다.

PageToken과 slot/growth의 교차 overlap은 한 번의 kind순 정렬만으로 전부 드러나지 않는다. 별도 bounded pass로 각 resource의 alias를 검사한다. 초기 정확한 검산 방식은 current item 하나를 들고 다른 kind의 stream을 다시 순회하는 방식이다. RAM은 bounded지만 O(N²) I/O 가능성이 있으므로 성능/정상 startup 인수 완료로 주장하지 않는다. 이후 더 나은 bounded join/색인은 독립 검토 대상이며, 검산 자체를 생략하는 최적화는 허용하지 않는다.

### 4.3 Prefix 복원과 RAM

ANALYSIS는 checkpoint의 owner descriptor 표와 selected inventory closure를 먼저 검산한다. 이후 capture LSN **직후**의 완결 의미 그룹만 owner 단위로 적용한다. checkpoint보다 이른 Begin을 재생하여 ended owner를 부활시키지 않는다. owner table은 최대32 ATT+system 상한이며, item 전체를 hash map이나 Vec에 모으지 않는다.

최소 실행안은 immutable capture item과 이후 WAL delta를 반복 streaming scan하여 각 physical resource의 마지막 상태를 구한다. before-item과 정확한 이전 change LSN을 일치시키고 같은 group의 after-item으로만 이동한다. capture에 없는 신규 resource는 WAL의 첫 absent→present delta가 유일한 생성점이다. capture items와 신규 resource의 **첫 등장만** 각각 한 번 열거하고 뒤의 재등장을 새 항목으로 세지 않는다. canonical 출력이 필요하면 작은 외부 선택 cursor로 다음 numeric key를 반복 찾는다. 원본 WAL/closure는 검산·handoff 동안 선택 상태로 고정한다.

이 방식은 느릴 수 있으나 table·current item·한 chunk·한 frame 외에 전체 inventory RAM 복제를 요구하지 않는다. source/WAL의 반복 read는 retained prefix의 일부이며 home allocator의 최신성에 기대지 않는다. BOOT_RESERVE는 최대1 MiB frame input,16 KiB chunk input, owner table, hash/cursor scratch와 적용 workspace의 합계를12 MiB 안에서 실제 배치해 보여야 한다. 이 문서는 미구현 iterator가 이미 그 bound를 지켰다고 표시하지 않는다.

RESERVE_RESTORE의 finish는 각 owner의 canonical final item count/digest, category별 page/slot/growth 합계, global overlap 부재, profile 부등식, actual retained/owned namespace를 전부 확인한 뒤 한 번만 성공한다. 복구 실행 주체는 원래 TxnRef/SystemWorkRef를 유지한다. REDO는 그 인계된 예약으로 page를 따라잡게 하고 논리 reservation delta를 다시 차감하지 않는다. REDO 뒤에는 home allocator/undo/status의 실제 state·generation과 복원한 같은-prefix 소유를 다시 대조한다.

after descriptor가 매 ownership delta 뒤의 canonical inventory digest를 요구하므로 현재 안은 runtime group 준비와 recovery 분석에서도 owner inventory 재순회 비용을 만들 수 있다. SHA-256 digest에 item 한 개를 더하고 빼는 가짜 O(1) 갱신은 제공하지 않는다. bounded RAM은 이 I/O/CPU 비용이 작다는 증거가 아니다. 필요한 gate 밖 준비·재검증, 전체 stream 재계산 빈도, redo/복원 반복 scan 및 normal startup 비용은 F05·성능 인수에서 아직 미검증이다. delta chain/인증된 index로 바꾸려면 hash 의미·crash closure·원래 검산을 보존하는 별도 D2가 필요하다.

## 5. Checkpoint/rebase의 물리 상한

checkpoint 두 descriptor slot을 각1 MiB로 준비한다는 control owner의 초안과 연동한다. 여기서 owner descriptor 최대36×1536=55,296 bytes, owner identity 표36×56=2,016 bytes, root 최대2,176 bytes다. 따라서 이 **부분**은1 MiB 안에 들어간다. commit/publication state, DPT, FILEMAP/WRITER/ref·high-water·선택 장벽을 모두 합친 실제 checkpoint가 들어간다는 최종 증명은 아직 아니다.

전체 inventory는 별도 세 bank로 둔다. 각 bank는16 KiB root slot과 `ceil(N_max/169)`개의16 KiB chunk slot이다. 필요한 물리 bytes는 다음과 같다.

```text
B_bank(N_max) = 16,384 × (1 + ceil(N_max / 169))
B_inventory_peak = 3 × B_bank(N_max)
B_checkpoint_descriptors = 2 × 1,048,576
B_fixed_maintenance
  >= B_inventory_peak + B_checkpoint_descriptors
     + 실제 checkpoint/WRITER selector·staging peak
     + DWB16 MiB + FILEMAP/profile + 기타 이미 고정한 bootstrap 몫
```

`N_max`는 임의로65536 같은 숫자를 골라 작업을 잘라 맞추지 않는다. F05가 지원 opcode의 거래 누적·동시 admission·system 작업으로 계산한 **동시에 존재할 수 있는 page/undo-slot/status-slot/growth item의 최대 합계**다. physical page 상한은 `floor(profile.owned/16384)` 이하이고 metadata page는 제외되지만 slot item은 page보다 많을 수 있다. 이 physical 상한만으로 slot/owner 상한을 대신하지 않는다. 초기 default8 GiB 및 acceptance-only64 GiB profile 각각에서 N_max와 세 bank 비용을 산출해야 한다.

N_max·closure peak가 아직 없으므로1 GiB protected reserve의 충분성, 최대32거래 동시 admission, 4/16/64 MiB buffer에서의 완료 보장은 **미검증**이다. encode할 수 있다는 이유로 quota를 늘리거나, 실행 뒤 N_max/작업량을 줄여 기존 PASS를 유지하지 않는다. F05 전 신규 profile을 admission 가능이라고 발표하지 않는다.

세 bank는 current/previous의 서로 다른 두 closure와 다음 closure를 동시에 보존하기 위한 선택이다. selected bank를 덮어쓰지 않는다. source L까지 sync·REDO flush/DWB retirement 뒤 새 bank를 완전히 쓰고 검산한 다음 상위 REBASE selector의 `new/old→new/new→second-copy/new` 인계를 따른다. 두 checkpoint descriptor가 같은 immutable bank를 가리킬 수 있으며 bank bytes를 두 번 charge하지 않는다. selector가 불명이면 실제로 선택될 수 있는 old/new 합집합을 보존한다. 미선택 bank를 재사용하는 권한은 selector가 확정된 뒤에만 생긴다.

rebase가 loser 완료를 기다리거나 새 epoch를 먼저 써야 하는 순환은 금지한다. 이전 WAL/control이 실제 회수+dirsync되기 전에는 charged에서 빼지 않는다. bank/control 고정 공간을 restart마다 새 파일로 늘리지 않는다.

### 5.1 System 상한4의 제안 범위

| 동시에 필요한 작업 | 최초 소유 규칙 |
| --- | --- |
| 거래 forward/rollback 중 allocation·nested split | 원래 TxnRef가 전체 중간 단계 비용을 소유. nested 함수마다 새 SystemWorkRef를 생성하지 않음 |
| 독립 allocation 또는 offline build | AllocationMaintenance 슬롯1개. build 내부 allocation/extent/취소는 같은 owner·subcursor로 처리 |
| 독립 구조 정리 | StructuralChange 슬롯1개. 내부 allocation/추가 split도 같은 owner |
| aging/release queue | Aging 슬롯1개. cursor 단위로 workspace를 양도하되 영속 owner는 유지 |
| fuzzy checkpoint | Checkpoint 슬롯1개. DWB flush의 fixed coordinator와 별개이며 추가 영속 checkpoint owner를 중첩 생성하지 않음 |

이는 네 종류를 마음대로 직렬화해 모든 기존 작업을 지원한다고 주장하는 표가 아니다. 독립 allocator의 완료를 기다려야만 진행할 수 있는 build, 다른 kind owner에 예약을 넘겨야 하는 nested SMO, checkpoint가 aging의 미완결 owner를 대신 끝내야 하는 설계가 실제 opcode에 있으면 이4개 표는 아직 충분하지 않다. 해당 opcode는 첫 effect 전에 전체 자체 진행 집합을 예약하거나 system 상한·전이 규칙을 다시 D2 검토해야 한다. current/previous closure를 유지한 채 checkpoint/aging/build가 공존하는 F05 검산도 필수다. recovery executor는 새 다섯 번째 claimant가 아니며 원래 소유를 실행한다.

## 6. WAL SemanticGroup 표현

기존 `XWF1` frame version1에 payload-format **SemanticGroup2**를 추가하는 안이다. 현재 제품 codec은2를 지원하지 않는다. 이 문서는 기존 Opaque0/PageRedo1을 수정하지 않으며 DEV2 `AXF1`나 거래 전체 frame를 재사용하지 않는다. 전체 frame 한도1,048,576 bytes와 stream header160/trailer32는 유지한다.

한 의미 그룹은 **owner 한 개**만 변경한다. 다른 owner의 token을 같은 group에서 암묵적으로 가져오지 않는다. 이 초기 범위의 inter-owner 직접 transfer는 UnsupportedOperation이다. 기존 source edge가 다른 거래의 undo/status를 가리키는 것은 허용하며 그 target count 변경은 owner identity 변경과 다른 의미다.

### 6.1 Group header192와 순서

| offset | bytes | 값 |
| --- | --- | --- |
| 0 | 4 | `XSG1` |
| 4 | 2 | version1 |
| 6 | 2 | 아래 operation tag |
| 8 | 2 | header bytes192 |
| 10 | 2 | flags0 |
| 12 | 4 | 전체 group bytes |
| 16 | 8 | enclosing frame LSN과 같은 apply LSN |
| 24 | 8 | 정확한 previous owner LSN |
| 32 | 56 | OwnerRef |
| 88 | 32 | 이전1536-byte descriptor 전체 SHA256; 최초 생성만32 zero |
| 120 | 4 | after descriptor bytes1536 |
| 124 | 2 | NativeEffect 수 B |
| 126 | 2 | EdgeChange 수 E |
| 128 | 2 | CountChange 수 C |
| 130 | 2 | InventoryDelta 수 D |
| 132 | 2 | PageRedo 수 R; outer frame record count와 동일 |
| 134 | 2 | reserved0 |
| 136 | 4 | NativeEffect 전체 bytes |
| 140 | 4 | EdgeChange bytes=128×E |
| 144 | 4 | CountChange bytes=96×C |
| 148 | 4 | InventoryDelta bytes=208×D |
| 152 | 4 | PageRedo 전체 bytes |
| 156 | 36 | reserved0 |

body는 **after descriptor1536, OperationCore256, NativeEffect 배열, EdgeChange 배열, CountChange 배열, InventoryDelta 배열, 기존 PageRedo 배열** 순서다. padding/미지 section은 없다. section offset을 별도 신뢰하지 않고 길이 합으로 계산한다. 각 count는 u16의 표현 상한보다 먼저 실제1 MiB 전체 bound에 걸리며, count만 믿고 count비례 allocation을 하지 않는다.

| tag | operation | outer frame kind | 필수 의미 |
| --- | --- | --- | --- |
| 1 | Begin | Begin | 새 TransactionActive와 status 활성화·초기 예약 |
| 2 | Forward | Update | native effect, 새 undo·head/edge/count, 증가한 abort obligation |
| 3 | RollbackStart | Update | statement/transaction target·progress 설정과 충분한 기존 복구 예약 |
| 4 | Clr | Clr | 현재 위치의 논리 undo 효과·현재 유효 link·UndoNextRef·잔여량 |
| 5 | StatementFinish | Update | Active 유지. statement 성공/복원 완료가 앞선 statement의 abort 몫을 해제하지 않음 |
| 6 | Commit | Commit | terminal status/q. terminal 채택 뒤에만 abort 예약 반환 |
| 7 | AbortEnd | AbortEnd | 전체 undo 완료·terminal status. next 완료와 예약 반환 |
| 8 | SystemBegin | System | 새 SystemActive와 전체 진행 예약 |
| 9 | SystemStep | System | 작업 효과·cursor/count·잔여량. 구체 task body가 승인된 종류만 |
| 10 | SystemEnd | System | 작업 종료·예약 반환 |
| 11 | Reserve | Update 또는 System | 기존 owner에 추가 admission을 기록. 실제 token/slot/growth 소유와 budget 증가 결합 |

Checkpoint outer frame의 body는 이 owner 한 개 group이 아니다. 선택 checkpoint/전체 closure 형식이 동결되기 전에는 이 codec이 Checkpoint payload 성공을 제공하지 않는다. Forward의 catalog body, queue/body를 모르는 SystemStep, status count 저장 위치가 닫히지 않은 적용은 UnsupportedOperation/UnsupportedNativeBody다.

### 6.2 OperationCore256

offset0 operation:u16,2 flags:u16,4 length:u32=256,8 statement sequence:u64,16 mutation sequence:u64,24 consumed/new UndoSequence:u64,32 current UndoRef40,72 UndoNextRef40,112 rollback target UndoRef40,152 commit q:u64,160 work cursor before:u64,168 work cursor after:u64,176 logical-before SHA32,208 logical-after SHA32,240..255 zero다.

flags의 bit0/1/2는 current/next/target ref 존재, bit3/4는 logical-before/after digest 존재다. 나머지는0이고 absent 영역은zero다. digest는 native canonical logical projection의 SHA이며 물리 slot/offset·pageLSN·mutable incoming count는 projection에서 제외한다. projection의 정확한 bytes를 정의하지 않은 native kind는 지원하지 않는다. 이 계약은 row/index projection을 기존 record의 target identity·presence/value·stamp·logical version·현재 undo-link의 계약 순서로 정의하도록 F03 literal에서 고정할 의무를 둔다. 아직 그 projection codec을 구현한 것으로 표시하지 않는다.

Begin은 statement/mutation/undo/ref/q/cursor/digest 전부0이다. RollbackStart는 statement rollback이면 statement sequence nonzero와 target 존재/없음을 mark에 맞게 표현하고 transaction rollback이면 target 없음이다. Clr는 current ref/UndoSequence 필수, next/target은 embedded status와 정확히 같다. Commit만 q nonzero이며 status q와 같다. SystemBegin/Step/End만 work cursor를 사용하고 after descriptor의 cursor와 일치한다. 사용하지 않는 field는 전부0이다. 세부 상태 전이는 아래 F02 적용 전제에서 추가 검산한다.

### 6.3 NativeEffect와 page 결합

NativeEffect는128-byte header와 full before cell bytes, full after cell bytes다. 이 값들은 논리 undo의 원래 whole page snapshot이 아니다. **이 group이 실제 변경하는 현재 cell의 precondition과 after bytes**다.

header offset0 kind(Row1/Index2/UndoRoot3/UndoChunk4/TxnStatus5/AllocationCell6/Catalog7),1 flags(before bit0/after bit1),2 redo index:u16,4 before slot:u16,6 after slot:u16,8 before cell offset:u16,10 after cell offset:u16,12 before length:u32,16 after length:u32,20 record length:u32=`128+before+after`,24 beforeSHA32,56 afterSHA32,88..127 zero다. absent cell의 slot은65535, offset/length/digest는0이다. before와 after가 모두 없으면 거절한다. array는 `(redo index, after slot 또는 before slot, kind)` 순서이며 같은 physical cell에 중복 효과를 두지 않는다.

두 body는 해당 native decoder로 추가 검산한다. expected namespace/schema/target/owner는 caller의 안정화된 context에서 받는다. raw bytes의 SHA만 맞고 native decoder가 없는 kind는 UnsupportedNativeBody다. UndoRoot/UndoChunk는 별도 undo 계약, status의 논리 core192는 기존 거래 계약, allocator cell6208은 기존 allocator 계약을 소비한다. count를 함께 적용하는 status native body는 [home224 제안](dev3-status-home.md)의 full before/after를 요구하며 core192만으로 성공시키지 않는다. home224의 독립 채택·codec과 의미 적용기의 결합 검산 전에는 해당 opcode를 지원하지 않는다. catalog body는 해당 leaf의 독립 동결 뒤에만 지원한다.

after bytes는 해당 PageRedo의 실제 after span에 정확히 포함되어야 한다. Init은 공통 page의 실제 slot directory/identity까지 대조한다. Patch는 offset 범위와 after bytes 일치만으로 old page의 slot directory까지 인증하지 않는다. **F02 before/after page 검산기**가 old canonical page와 patch 적용 후 canonical page에서 slot·cell 전체 bytes/target을 확인해야 적용 허가가 된다. 제거되는 cell은 after absent와 실제 after page의 slot/entry 제거가 일치해야 한다.

새 undo root/chunk의 전체 immutable material은 이 frame 또는 이미 채택된 선행 준비 group의 descriptor/소유 아래 있어야 한다. 선행 group에 준비했다면 공개 전에도 취소/복구 의무가 있으며 raw unreferenced page를 그냥 잊지 않는다. Forward는 new head에 연결하는 같은 group에서 해당 undo의 NativeEffect·edge/count/status를 결합한다. Clr는 current tree에서 다시 찾은 cell을 바꾸며, root의 **현재** previous-version link와 before logical identity를 사용한다. 오래된 whole page를 before bytes로 복원하여 다른 writer의 ACK나 완료된 split을 되감지 않는다.

### 6.4 EdgeChange128와 CountChange96

ReferenceTarget56은 optional target이다. tag1은 UndoRef40+expected UndoSequence8, tag2는 TxnRef48, tag0은 전체zero다. offset1..7은zero다. creator identity는 status edge가 아니며 별도 count를 만들지 않는다. tag1의 expected sequence는 항상 nonzero이고 실제 target의 sequence와도 일치해야 하지만 **모든 source가 자기 UndoSequence를 가진다고 가정하지 않는다.**

| source role | source에 실제 저장된 bytes | target sequence를 얻고 확인하는 방법 |
| --- | --- | --- |
| UndoPreviousTxn/UndoPreviousVersion | 기존 UndoLink48 및 source root의 UndoSequence | source의 expected sequence를 그대로 대조하고 실제 target root의 sequence도 확인. 이 두 role만 `target sequence < source UndoSequence`를 적용 |
| RowUndo/IndexUndo/CatalogUndo | 기존 UndoRef40, source UndoSequence 없음 | source에서 reference40을 비교한다. 준비 단계에서 정확한 ref/generation·Live state로 load/pin한 target의 실제 sequence를 group의 추가 기대값으로 기록하고 적용 전 다시 확인 |
| TxnHead | status core의 UndoRef40, source UndoSequence 없음 | 해당 head target을 위와 같이 load/pin하여 얻은 실제 sequence를 추가 기대값으로 결합. TxnId local·mutation sequence·LSN을 source UndoSequence로 대신 쓰지 않음 |
| 새 group 안에서 생성한 undo를 가리키는 edge | source ref40 또는 link48와 같은 group의 new UndoRoot NativeEffect | 아직 home에서 load할 수 없는 새 record의 검증된 root sequence·identity를 사용. page/slot 예약 generation과 INIT/생성 효과가 같은 group에 결합돼야 함 |

이 target context는 WAL bytes에서 기대값을 자기 복사한 것이 아니다. source latch를 놓기 전에 등록한 transient pin과 안정화한 target/context, 또는 같은 group의 새 root로부터 얻어야 한다. page I/O는 gate/latch 밖에서 준비하고 적용 전 generation/state/sequence를 재검증한다. Role별 creator/target/before logical 교차 검산은 undo 계약을 따른다. Row/Index/TxnHead에는 감소를 비교할 source sequence 자체가 없으므로 숫자0이나 LSN을 만들어 비교하지 않는다. CLR의 `UndoNextRef` 감소 검사는 별도로 실제 consumed root와 다음 root의 sequence를 비교하는 rollback 전제이며 모든 edge에 적용하는 일반 규칙이 아니다.

EdgeChange128은 offset0 source NativeEffect index:u16,2 role:u8,3..7 zero,8 old target56,64 new target56,120..127 zero다. 역할은 RowUndo1/IndexUndo2/CatalogUndo3/UndoPreviousTxn4/UndoPreviousVersion5/UndoBeforeStamp6/RowStamp7/IndexStamp8/CatalogStamp9/TxnHead10이다. role과 source kind/field/target kind가 정확히 맞아야 한다. Txn rollback progress의 이동 pin/보호는 TxnHead와 별개이고 incoming count에 자동 합산하지 않는다.

array는 `(source effect index,role)` 순서이며 중복 role은 없다. before/after native field에서 추출한 실제 old/new reference와 위 role별 loaded/new-root sequence context를 **함께** 비교한다. Row/Index/TxnHead의 field40 bytes만으로 ReferenceTarget56 전체를 추출할 수 있다고 주장하지 않는다. raw parser가 `Set`으로 중복 target을 제거하지 않는다. 같은 undo의 previous-txn/version 두 field가 한 target을 가리키면 그 target의 incoming을2 증가/감소시킨다. old=new인 불필요한 delta는 거절한다.

CountChange96은 offset0 target56,56 before count:u64,64 after count:u64,72 target NativeEffect index:u16,74..95 zero다. target 순서이며 중복 target은 없다. 별도로 열거한 EdgeChange multiset에서 각 target에 대한 add/remove를 합산하고 먼저 `before>=removes`를 검사하고 checked 산술로 `after=(before−removes)+adds`를 검산한다. source의 실제 before/after graph에서 바뀐 edge가 delta에 누락돼도 거절한다. count가 바뀌지 않은 target은 CountChange를 싣지 않는다. 그러나 EdgeChange가 닿는 **모든** target은 net0이라도 안정화된 home proof를 제공해야 한다. proof의 정확한 identity/generation과 `before>=removes`를 확인한다. Undo target은 Live/non-Releasing이며, TxnRef target은 occupied status home와 opcode별 허용 상태여야 한다. before0에서 한 edge 제거와 같은 target으로 한 edge 추가를 net0으로 상쇄해 성공시키지 않는다. net0 proof는 읽기 전제이고 page write가 없는 NativeEffect를 만들라는 요구가 아니다. proof의 정확한 API·고정 수명·WAL replay 검산은 F02에서 별도 동결해야 하며, 그 입력이 없으면 적용을 승인하지 않는다.

UndoRoot의 stored incoming은 byte offset80에 있어 해당 NativeEffect와 결합할 수 있다. **status192에는 TxnRef incoming count field가 없다.** status count의 별도 [home representation](dev3-status-home.md)이 동결·구현되고 같은 native effect의 incoming offset8에 CountChange가 결합되기 전에는 tag2 CountChange를 실제 적용 가능한 것으로 승인하지 않는다. tag2의 구조를 읽을 수 있다는 사실을 persistence 구현으로 세지 않는다. queue의 durable 소유·cursor가 필요한 cut/release도 해당 native format 없이 적용 허가를 내리지 않는다.

### 6.5 InventoryDelta208

offset0 action:u8(Reserve1/Consume2/Release3/RealizeGrowth4),1 flags(before bit0/after bit1),2..7 zero,8..15 zero,16 old Item96,112 new Item96다. 이 group 안의 ordinal은 항상0이고 enclosing owner를 뜻한다. absent item은96 zero다.

Reserve는 absent→present, Consume/Release/RealizeGrowth는 present→absent다. RealizeGrowth는 Growth만, Consume은 PageToken/UndoSlot/StatusSlot만, Release는 네 kind 모두 가능하다. old item은 복원한 같은 owner/resource의 정확한 bytes·change LSN과 일치해야 하고 new item의 change LSN은 frame LSN이다. 동일 resource의 중복 delta는 거절한다. 순서는 Item의 canonical resource key다.

page consume은 allocator Reserved→Allocated 및 필요한 INIT와 같은 group이다. slot consume은 예약 generation과 실제 native occupied record 생성에 결합한다. release는 실제 allocator/slot의 예약 취소와 결합한다. RealizeGrowth는 물리 길이·header·FILEMAP 선택 장벽을 이미 충족한 range만 제거하고, 새 existing page token을 만드는 delta를 함께 기록한다. file 생성/확장 barrier 자체는 무효과 group parser가 인증할 수 없다.

before/after inventory의 count/digest와 budget category 합계가 after descriptor와 일치해야 한다. 선언한 item 몇 개만 맞고 나머지 inventory를 잃은 descriptor는 finish할 수 없다. literal group 하나의 local decode가 이 streaming finish를 대신하지 않는다.

## 7. F02 적용·실패 경계

미래의 무효과 `validate_group(before_descriptor, group, expected_context, before_pages, inventory_cursor)`는 다음을 모두 만족해야 apply plan을 낸다. 그 plan도 WAL append/sync, lock/pin 수명, 실제 적용을 실행하지 않는다.

1. outer stream identity와 정확한 previous stream LSN, group/header/owner/LSN/previous owner LSN, previous descriptor SHA를 검산한다. Transaction의 outer TxnId는 owner와 같고 outer previous-txn LSN은 previous-owner LSN이다. System outer TxnId는zero지만 group의 SystemWorkRef는 필수다.
2. Begin/SystemBegin은 미존재 owner만 생성하며 birth=apply, previous=0, group count1이다. 나머지는 존재하는 nonterminal owner의 정확한 직전 상태만 받는다. birth/profile/owner는 유지하고 count는+1이다. apply LSN을 byte offset으로 증가시키지 않는다.
3. checkpoint에는 writer의 TxnId local high-water와 system kind별 work-id high-water가 포함돼야 한다. 새 Begin은 현재 writer epoch와 증가한 local을 사용한다. old epoch TxnId는 기존 owner의 CLR/terminal에만 허용한다. 초기 SystemBegin은 kind별 새 증가 id와 generation1을 사용하고 id wrap/reuse를 하지 않는다. 이미 존재하는 descriptor의 generation은 그대로 보존한다. terminal owner를 ATT에서 제거한 뒤 같은 identity를 새 Begin으로 부활시키지 않는다.
4. native before는 실제 고정한 old page/slot/value와 같고 after는 실제 redo 적용 결과와 같다. required native 효과·status/undo/head·edge/count·inventory·descriptor를 opcode별로 대조한다. effect 없는 CLR progress, count 없는 cut, cursor 없는 release를 거절한다.
5. after status/progress/OperationCore가 일치한다. statement 완료는 앞선 statement의 전체 abort 의무를 남긴다. terminal은 남은 head·status·retirement 작업의 durable owner/credit을 같은 group으로 인계한 뒤에만 종료된 의무와 불필요 reservation을 반환한다. ATT/descriptor를 지운 뒤에도 필요한 credit을 반환하면 안 된다. 현재 초안의 단일-owner InventoryDelta에는 이런 인계가 없으므로 queue·새 owner·credit 인계와 상한이 동결될 때까지 Commit/AbortEnd 실제 적용은 Unsupported다. durable terminal adoption 전에는 실제 예약을 빌려주지 않는다.
6. WAL consumed 누계의 증가는 exact frame length다. budget 감소는 해당 step의 실제 소비와 증명된 불필요 잔여 반환이며, 증가에는 첫 effect 전 admission이 필요하다. after remaining이 실제 opcode의 미래 최악 비용 이상인지 F05 표와 검산한다. encode 성공만으로 이 조건을 생략하지 않는다.
7. 전체 frame를 준비한 뒤 version-edit gate·필수 page latch 아래 generation/head/ownership을 재검증한다. 새 page나 reserve가 필요해지면 아무 효과 없이 준비 단계로 되돌아간다. partial pin을 붙잡고 I/O/자원 대기하지 않는다.

runtime은 완결 WAL copy 뒤에만 live page/head/count/progress/descriptor·ATT/DPT를 함께 설치한다. append 이후 부분 적용 실패는 Fenced이며 깨끗한 abort로 돌려주지 않는다. I/O/sync는 기존 gate/latch 규약 밖에서 수행한다. recovery analysis에서는 같은 after descriptor를 한 번 채택하고 REDO에서 이 accounting transition을 재실행하지 않는다.

`apply_lsn == current.apply_lsn`인 group도 일반 success/idempotent ledger apply로 처리하지 않고 **DuplicateOwnerLsn**으로 거절한다. 더 작은 LSN·틀린 predecessor/hash는 OwnerOrderMismatch다. page REDO의 pageLSN 기반 재적용 생략은 별도이며 ledger duplicate 거절과 모순되지 않는다. checkpoint 이전 frame를 owner analysis에 넘기지 않는 것이 caller의 선행조건이다.

앞 단계만 검증한 view를 뒤 단계 성공 타입으로 암묵 변환하지 않는다. 최소 결과는 `DescriptorView`, `GroupView`, `InventoryClosureVerified`, `ReservationRestored`, `ApplyPlan`을 구분한다. 실제 API 이름·Rust 모듈 배정은 구현 전 D2 후속이며 현재 성공 stub은 만들지 않는다.

## 8. 실제 크기와 codec 경계

NativeEffect i의 before/after byte 길이를 U_i/V_i, edge/count/item delta 수를 E/C/D, patch data 길이를 L_j, INIT 수를 I라 하면:

```text
frame = 192 + group_header192 + descriptor1536 + operation256
        + Σ (128 + U_i + V_i)
        + 128×E + 96×C + 208×D
        + 64×R + Σ patch L_j + 16384×I
      = 2176 + Σ (128+U_i+V_i) + 128E + 96C + 208D
        + 64R + Σ patch L_j + 16384I
frame <= 1,048,576
```

NativeEffect body와 redo의 일부 bytes가 중복되는 비용을 빼지 않는다. descriptor만1536 bytes라고 실제 Forward 비용을 계산하지 않는다. UndoRoot 최대2480 bytes, chunk cell 최대16220 bytes와 full INIT 비용을 각각 넣고, before/after가 둘 다 있으면 둘 다 더한다. 기존 `63 INIT` 수치는 별도 semantic overhead가 없는 PageRedo1 전용이다. 이 group에서 다른 section이 전혀 없다는 산술 하한에서도 INIT63개는1,038,400 bytes,64개는1,054,848 bytes다. 실제 required native/status/allocator/edge 등을 넣으면63개도 수락된다고 보장하지 않는다.

whole group이1 MiB를 넘으면 첫 효과 전에 거절하거나 독립 동결한 준비/진행/보상 group으로 분할해야 한다. 중간 group의 소유·잔여 의무를 없애서 frame를 작게 만드는 것은 허용하지 않는다. arbitrary patch로 before/after 전체를 감춰 semantic 검산을 생략하지 않는다.

후속 encoder는 모든 입력/마지막 record/합계/capacity를 첫 쓰기 전에 검사하고 오류에서 output 전체를 보존한다. decoder는 borrowed view이며 frame/page-size scratch 또는 item-count 비례 hidden allocation을 하지 않는다. checksum/header용 작은 scratch와 caller가 예약한 input/output만 사용한다. 전체 검산을 완료하기 전에 mutable view나 일부 성공 결과를 공개하지 않는다.

## 9. 독립 vector와 실행 전 시험 제안

이 문서의 검토 전에는 제품 codec이나 모델을 실행하지 않는다. 아래 vector는 표의 offset/정수/표준 SHA-256으로 별도 Python script에서 계산하며 production codec/import를 사용하지 않는다. 원본 script·bin·expected JSON·계약 hash를 `artifacts/dev3/obligation-format-r0/`에 보존한 뒤 literal을 동결한다. 그 계산은 제품 검증 결과가 아니다.

첫 고정 literal 집합은 Descriptor3개(Active, statement Aborting, SystemActive), InventoryRoot2개(empty/mixed), Chunk1개(mixed4 items), SemanticGroup1개(RollbackStart)다. DB=bytes1..16, timeline17, TxnId epoch18/local19/generation20, status store FileId(2,1)/page3/inc4·slot0/gen20·Object55/gen56, SystemWork kind1/id21/gen22, profile generation23이다. descriptor LSN은 Active30/previous20/birth20, Aborting40/previous30/birth20, System50/previous0/birth50이다.

Profile ref의 SHA는 `OBLIGATION-DRAFT-REFERENCE-ONLY`+zero1 뒤를0x5a로4096 bytes까지 채운 **reference-only fixture**의 hash이며 그 bytes도 보존한다. 실제 XDC1 PROFILE로 수락할 fixture가 아니므로 이 literal의 group/descriptor 구조 검사 성공을 profile 선택이나 복원 성공으로 사용할 수 없다.

Root generation60/capture60, empty bank0/O0/N0, mixed bank1/O2/N4/C1이다. mixed의 owner 표는 Transaction→System 순서다. item은 순서대로 Transaction/Clr의 Page65/inc9(change30), System/StructuralCompletion의 Page66/inc10(change50), Transaction/Clr의 UndoSlot(Page67/inc12,slot0/gen11,change30), System/StructuralCompletion의 Growth(FileId(2,1),base128,extent1,intent1,change50)다. 모든 item의 FileId는(2,1)이다. descriptor의 item count는 각2이며 digest는 본문의 ordinal 정규화에서 계산한다.

Active의 Clr row는 steps2/WAL4096/page1/undo-slot1/frame2/memory65536/FD1/DWB1/protected22964, Terminal은 steps1/WAL4096/frame1/memory16384/FD1/DWB1/protected4096이다. Aborting은 같은 row에서 Clr WAL1152/protected20020만 바뀐다. System의 StructuralCompletion은 steps2/WAL8192/growth1048576/page1/frame2/memory65536/FD1/DWB1/protected1073152다. 다른 row·field는0이다. 이 숫자는 byte vector의 명시 입력이며 실제 opcode 충분성의 F05 증거가 아니다.

Active/Aborting/System의 WAL consumed는8192/11136/4096, group count는2/3/1이다. Active/Aborting status는 head/target/next 없는 동일 거래이며 Aborting은 statement kind다. SystemProgress는 kind1/Prepared/task1/cursor0/limit1/work-root 없음/SHA256(empty)다.

RollbackStart는 frame LSN40, previous stream35, previous owner30, statement sequence1이고 next/target/current refs가 없다. native status before192/after192와 Patch1개를 담는다. patch의 page/owner는 위 status page, prior pageLSN30, offset16160/length192다. native before/after slot0와 실제 canonical page를 독립적으로 함께 저장한다. group2752/frame2944 bytes이며 WAL consumed 증가2944와 Clr row 감소2944가 일치한다. 별도 raw group과 outer frame를 모두 보존한다.

첫 Rust test 이름은 다음 **12개**를 제안한다. 의미 projection·실제 opcode가 미동결이므로 현재 실행 가능한 profile로 동결됐다고 표시하지 않는다. source 시작 전에 N01..과 각 matrix의 정확한 case 표를 채우고 root의 독립 검토를 받는다.

| test | 필수 범위 |
| --- | --- |
| `obligation_literals_bind_owner_profile_and_status` | 위3 descriptor exact bytes·SHA, owner/status/profile/namespace |
| `obligation_lengths_reserved_and_checksums` | descriptor의 모든 prefix0..1535와+1, byte별 단일 bit1536, reserved의 재hash 손상 |
| `obligation_budget_and_counter_arithmetic` | 5 category 순서·zero step·sum/max·growth1 MiB 계산·checked overflow·consumed 감소 |
| `ownership_literals_and_complete_stream` | root2/chunk1, 마지막 item/chunk 손상, 전체 count/digest/bytes/owner 일치 |
| `ownership_alias_and_interval_matrix` | owner/category가 다른 page alias, slot generation alias, page/slot 중복, growth overlap |
| `ownership_restore_uses_capture_and_wal_only` | Begin 삭제 뒤 capture, partial CLR, 원래 claimant 유지, stale home을 입력으로 받지 않는 별도 oracle |
| `semantic_group_lengths_and_canonical_sections` | counts/length/trailing/unknown tag/sort, frame exact1 MiB와+1 |
| `semantic_group_owner_predecessor_and_terminal_reuse` | duplicate/out-of-order/hash/owner generation, terminal 뒤 옛 identity 재생 거절 |
| `semantic_group_native_effect_edge_and_count_binding` | 두 link의 중복 count2, effect/progress/count 누락, status count 미지원의 명시 거절 |
| `semantic_group_inventory_and_budget_atomicity` | reserve/consume/release/growth before/after, retained WAL을 잔여 예약으로 재사용하지 않음 |
| `obligation_encoders_preserve_output` | 마지막 record를 포함한 실패의 전체 sentinel 보존·성공 tail 보존·input 불변 |
| `obligation_generated_profiles_match_independent_fields` | seed0..63×sample0..127=8192. 정상 생성 분모와 negative 분모를 나누고 입력식·독립 offset 판독으로 검산 |

F03 oracle은 candidate decode/summary를 기대값으로 사용하지 않는다. descriptor budget은 명시한 논리 step 목록에서, inventory는 독립 item multiset에서, edge/count는 native field의 명시 graph에서 계산한다. receipt에는 실제 실행한 정·부 건수와 source/contract/binary hash를 남긴다. M04/M06의 기존 PASS를 새 bytes/code의 PASS로 전용하지 않는다.

## 10. 미완료와 다음 검토

현재는 정확한 descriptor/ownership/header 제안까지이며 다음을 닫기 전에는 **구현 시작 승인 전 초안**이다.

- control owner의 kind5/6과 namespace/선택 형식 수용, checkpoint의 정확한 current/previous·WRITER/high-water closure.
- system4의 nested operation 검산, N_max와 inventory3 bank·checkpoint2개·전체 fixed 보호 영역의 F05 수치.
- NativeEffect의 logical projection과 실제 opcode 필수 section 표. status incoming count/queue/retirement native bytes.
- 7 literal의 모든 입력·SHA와12 test의 정확한 negative/matrix 분모. 초안 표를 실행 증거로 쓰지 않음.
- 실제 append/sync·group 적용·allocator/slot 예약·inventory streaming/restore·rebase·writer 선택, current-build F04 crash/reopen과 F05 진행 증명.

이 문서만으로 “reservation 복원 완료”, “의미 group 적용 가능”, “S1/DEV3 완료”를 표시하지 않는다. 구현 범위를 descriptor codec으로 한정하면 별도 부분 결과로 기록하고 미동결 group/closure를 성공 stub으로 채우지 않는다.

## 11. 2026-10-21 재개 독립 검토

`/root/dev3_selector_review`가 초안의 net0 count와 terminal credit 유실 반례를 제시했다. `/root`는 §6.4에 모든 touched target의 선행 count/state proof, §7에 terminal 뒤의 durable owner/credit 인계가 없는 opcode의 Unsupported 경계를 반영했다. 이는 미완성 opcode의 실행 승인이 아니다. 별도224-byte status home는 prefix32+기존core192로 제안하며, 채택 시 NativeEffect의 before/after 각각32 bytes와 Patch32 bytes가 늘어 RollbackStart group2848/frame3040, 단일 home의 page offset16128/length224가 된다. r0의 group2752/frame2944·offset16160/length192 원본은 당시192-core 초안으로 그대로 보존하고 새 home vector로 재표기하지 않는다.

현재 남은 핵심은 queue record·진행·보호 credit 인계, native logical projection과 opcode별 필수 section, net0 target proof API, inventory 반복 scan의 작업량·정상 reopen 예산이다. 이 문서 전체 D2 채택과 제품 구현은 아직 완료하지 않았다.

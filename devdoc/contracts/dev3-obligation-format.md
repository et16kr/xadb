# DEV3 영속 recovery obligation·WAL 의미 그룹

2026-10-21 · **D3-OBLIGATION-F01/F02 부분 계약 / §12 descriptor 표현·§13 RollbackStart 결합 관측 구현 / 전체 의미 적용·복구 미구현**

[예약 인계](disk-storage-design.md#recovery-reservation) · [변경 규약](disk-storage-design.md#mutation-protocol) · [REBASE](disk-storage-design.md#recovery-rebase) · [profile](disk-storage-design.md#capacity-profile) · [WAL frame](dev3-wal-format.md) · [거래](dev3-transaction-format.md) · [allocator](dev3-allocation-format.md) · [undo](dev3-undo-format.md) · [실제 크기 계산](dev3-resource-bounds.md) · [M04](dev3-recovery-model.md) · [M06](dev3-reservation-model.md) · [소유 범위](../implementation/dev3-implementation.md#wave-2)

이 문서는 거래·system 작업의 **남은 복구 의무를 정해진 bytes로 인계하는 최소 표현**과 그 표현을 WAL의 효과·진행점에 결합하는 규칙을 제안한다. 원래 초안 작성자는 `/root/dev3_model`이다. 재개 뒤 `/root`가 §12의 descriptor 표현과 §13의 RollbackStart 결합 관측을 구현·통합하고 `/root/dev3_selector_review`가 독립 검토한다. [ownership 별도 계약](dev3-ownership-format.md)의 bytes·합계·물리 충돌·현재 intent 검산도 좁은 관측 범위로 수용했다. 나머지 의미 그룹·완전한 inventory closure·실제 적용·복구는 여전히 제안이며 기본 제품 API에 연결하지 않는다.

descriptor의 형식 수락, checkpoint closure 검산, 실제 page 적용·복구 성공은 서로 다른 결과다. 기존 WAL `Opaque`를 descriptor로 간주하지 않는다. 아래 의미 그룹도 실제 before page·native record·자원 profile을 대조하는 적용기가 없으면 복구 완료를 뜻하지 않는다. **TxnRef incoming count의 실제 갱신·WAL 결합, retirement queue, CURRENT/WRITER/checkpoint의 최종 선택 형식, opcode별 F05 상한은 아직 미완료**다. 이 항목이 필요한 opcode를 구조만 맞는 payload로 성공시키지 않는다.

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

Profile ref는 kind1/slot0/index0/length4096이다. [ownership 별도 D2](dev3-ownership-format.md)는 control tag **OwnershipRoot5, OwnershipChunk6**의 표현 등록을 수용한다. 기존 Profile/Filemap/DWB tag의 변경 권한을 가져오지 않는다. 실제 namespace 파일명·선택·설치는 이번 등록 범위 밖이다. root는 bank0..2/index0, chunk는 같은 bank/index0..chunk_count−1이다. 뒤에서 설명하는 chunk aggregate hash 때문에 root에 chunk ref 배열을 전부 둘 필요는 없다.

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

PageToken과 slot/growth의 교차 overlap은 한 번의 kind순 정렬만으로 전부 드러나지 않는다. 별도 bounded pass로 각 resource의 alias를 검사한다. [별도 ownership 계약](dev3-ownership-format.md)은 네 kind cursor의 merge와 첫 owner 합계 pass로 성공5C read·고정 메모리 검산을 수용했다. 이 결과는 owner-local intent 유일성·slot 실재·전체 closure를 증명하지 않는다. 실제 startup I/O/성능 인수는 남아 있으며 검산 자체를 생략하는 최적화는 허용하지 않는다.

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

기존 `XWF1` frame version1에 payload-format **SemanticGroup2**를 추가하는 안이다. §13의 RollbackStart exact 관측 helper만2를 지원한다. 일반 FrameView/encode_frame/scan_frame_prefix는 계속2를 거절하며 다른 operation·실제 append/apply는 미지원이다. 이 문서는 기존 Opaque0/PageRedo1을 수정하지 않으며 DEV2 `AXF1`나 거래 전체 frame를 재사용하지 않는다. 전체 frame 한도1,048,576 bytes와 stream header160/trailer32는 유지한다.

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
3. checkpoint에는 writer의 TxnId local high-water와 system kind별 work-id high-water가 포함돼야 한다. 새 Begin은 현재 writer epoch와 증가한 local을 사용한다. old epoch TxnId는 이미 채택된 기존 Active owner의 RollbackStart와 기존 owner의 CLR/terminal에 허용한다. RollbackStart도 원래 TxnRef와 generation을 유지하며 새 Begin이나 포괄적인 recovery 우회 권한으로 해석하지 않는다. 초기 SystemBegin은 kind별 새 증가 id와 generation1을 사용하고 id wrap/reuse를 하지 않는다. 이미 존재하는 descriptor의 generation은 그대로 보존한다. terminal owner를 ATT에서 제거한 뒤 같은 identity를 새 Begin으로 부활시키지 않는다.
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

§12의 고정 descriptor 표현, §13의 RollbackStart 결합 관측과 [별도 수용된 ownership 관측](dev3-ownership-format.md)을 구현할 수 있다. 완전한 ownership closure·실제 의미 적용은 다음을 닫기 전까지 **구현 시작 승인 전 초안**이다.

- 등록된 kind5/6의 실제 namespace/선택 형식 수용, checkpoint의 정확한 current/previous·WRITER/high-water closure.
- system4의 nested operation 검산, N_max와 inventory3 bank·checkpoint2개·전체 fixed 보호 영역의 F05 수치.
- NativeEffect의 logical projection과 실제 opcode 필수 section 표. status incoming count/queue/retirement native bytes.
- 7 literal의 모든 입력·SHA와12 test의 정확한 negative/matrix 분모. 초안 표를 실행 증거로 쓰지 않음.
- 실제 append/sync·group 적용·allocator/slot 예약·inventory streaming/restore·rebase·writer 선택, current-build F04 crash/reopen과 F05 진행 증명.

이 문서만으로 “reservation 복원 완료”, “의미 group 적용 가능”, “S1/DEV3 완료”를 표시하지 않는다. 구현 범위를 descriptor codec으로 한정하면 별도 부분 결과로 기록하고 미동결 group/closure를 성공 stub으로 채우지 않는다.

## 11. 2026-10-21 재개 독립 검토

`/root/dev3_selector_review`가 초안의 net0 count와 terminal credit 유실 반례를 제시했다. `/root`는 §6.4에 모든 touched target의 선행 count/state proof, §7에 terminal 뒤의 durable owner/credit 인계가 없는 opcode의 Unsupported 경계를 반영했다. 이는 미완성 opcode의 실행 승인이 아니다. 별도224-byte status home는 prefix32+기존core192로 제안하며, 채택 시 NativeEffect의 before/after 각각32 bytes와 Patch32 bytes가 늘어 RollbackStart group2848/frame3040, 단일 home의 page offset16128/length224가 된다. r0의 group2752/frame2944·offset16160/length192 원본은 당시192-core 초안으로 그대로 보존하고 새 home vector로 재표기하지 않는다.

현재 남은 핵심은 queue record·진행·보호 credit 인계, native logical projection과 opcode별 필수 section, net0 target proof API, inventory 반복 scan의 작업량·정상 reopen 예산이다. 이 문서 전체 D2 채택과 제품 구현은 아직 완료하지 않았다.

## 12. 독립 descriptor 표현 leaf 제안

2026-10-21 작성·구현·통합 `/root`, 독립 검토 `/root/dev3_selector_review`. 이 절은 §2의 OwnerRef56과 기존 Profile ref64, §3의1536-byte descriptor·budget128만 구체화한다. §4 이후 ownership stream·semantic group·terminal·복구 적용은 포함하지 않는다. 기존 초안 bytes를 줄이거나 opaque body를 성공 stub으로 수락하는 대안 대신, 검증 가능한 고정 표현을 먼저 공급한다. D2 검토 전에는 이 절의 제품 코드를 작성하지 않는다.

### 12.1 API·오류·상태

private `storage/obligation.rs`와 `obligation_tests.rs`에 둔다. `DescriptorSpec`은 owner 상태, chain(apply/previous/birth/groups), Profile ref, inventory claim(change_lsn/items/digest/growth_extents), consumed WAL/control, 고정5개 budget, minimum_wal_lsn, last_mutation_sequence를 가진 값이다. `OwnerState`는 `Transaction(TransactionStatus)` 또는 `System(SystemProgress)`다. SystemProgress는 기존 SystemWorkRef와 phase/sequence/cursor/limit를 가지며 이 leaf에는 work-root가 없다. wire의 owner·state tag는 이 값에서 유도한다. None/Free owner는 만들지 않는다.

`encode_descriptor(spec: &DescriptorSpec, output: &mut [u8])`와 `decode_descriptor(bytes, expected_owner: ReservationOwner, expected_profile: ControlRef) -> DescriptorSpec`를 제공한다. spec 자체는 공개된 내부 필드를 가진 미검증 입력 값이며 encode 때마다 검증한다. decode의 기대 owner/profile은 신뢰한 WAL/checkpoint context가 공급하며 입력에서 복사한 값으로 대신하지 않는다. full owner에는 DB/timeline/epoch 또는 system kind/id와 generation이 포함된다. 이 codec은 실제 profile bytes·inventory나 before descriptor를 읽지 않으므로 대상 존재·예약 충분성·predecessor의 실제 동일성을 증명하지 않는다. expected profile도 기존 Profile ref의 구조를 만족해야 한다.

오류는 InvalidInput(호출자의 잘못된 spec/기대값/출력길이), Corrupt(입력 bytes·내부 모순), UnsupportedFormat(알 수 없는 descriptor version), UnsupportedWorkBody(유효 checksum의 system work-root가 nonzero), IdentityMismatch(형식 유효 입력의 owner/profile이 기대값과 다름)로 구분한다. embedded core/control의 알 수 없는 tag/version은 Corrupt다. descriptor version 외 다른 내부 enum tag는 Corrupt로 거절한다. 입력길이는 정확1536이며 magic/길이를 검사한 뒤 version을 구별한다. checksum 불일치도 Corrupt다.

encode는 정확1536-byte output만 수락하며 고정1536-byte scratch에서 전부 검증한 뒤 복사한다. caller input/output 외 heap/I/O/lock은0이다. typed spec와 scratch는 고정 크기이며 개수 비례 allocation이 없다. 실제 count/home 변경·WAL append·자원 획득은 없다.

### 12.2 추가 내부 정합성

- apply/birth/groups는 nonzero. previous=0이면 birth=apply·groups=1이다. previous>0이면 birth≤previous<apply·groups≥2다. 이 식은 실제 이전 descriptor hash나 consumed 누계의 단조성을 검증하지 않는다.
- inventory_change_lsn은0 또는 apply 이하. items>0이면 change_lsn>0이다. items=0이면 digest=SHA256(empty), growth_extents=0, 모든 budget의 page/undo/status token 수도0이다. 비어 있지 않은 inventory digest는 opaque32 bytes이며 실제 item stream과의 대조는 후속 closure가 맡는다. items=0이어도 이전 release의 change_lsn은 남을 수 있다.
- Item96의4종으로 표현 가능한 개수여야 한다. T=5행의 page/undo/status token 전체합, G=growth_extents, K=growth>0인 row수라 두고 `T+K ≤ items ≤ T+G`를 u128 산술로 검사한다. 각 growth item은 양수 extent수와 한 category를 가지므로 최소 K개·최대 G개다. 이는 실제 item identity/digest/overlap 검증을 대신하지 않는다. T+G가u64를 넘더라도 items가하한이상인유효표현은이론상상한의overflow로거절하지않는다.
- growth bytes는 growth_extents×1,048,576의 checked 결과이며5개 budget growth 합과 같다. 각 row growth도 extent 크기의 배수다. protected 합과 growth 합은 checked sum이다. budget의 WAL/control/page/undo/status 각각의5행 합도 u64 범위여야 한다. steps는 category별 다른 단위이므로 합산하지 않는다.
- step0 row는 모든 수치0이고 tag는 원래1..5 순서다. step>0의 비용이0인 표현은 수락할 수 있지만 실제 opcode admission이 아니다. 최대6 peak는5개 row의 각각 max, protected는 정확 sum이다. decoder는 저장된7개 aggregate와 비교한다.
- minimum_wal_lsn은0 또는 apply 이하. System의 last_mutation_sequence는0이다. Transaction terminal 상태도 표현할 수 있지만 terminal 실행/ATT 제거를 승인하지 않는다.
- System kind는 owner kind와 같고 sequence는 nonzero, cursor≤limit다. phase는 Prepared1/Running2/Finishing3/Finished4만 허용한다. wire SystemActive5는 phase1..3, SystemFinished6은 phase4·cursor=limit다. 없는 work-root64는zero, work-state hash는SHA256(empty)다. Transaction은 system128이zero이며 owner와 occupied status의 full TxnRef 및 state tag가 일치한다.

### 12.3 독립 입력과 고정8시험

기존 r0의 descriptor-active/aborting/system3 literal을 원래 hash 그대로 사용한다. reference-only PROFILE가 실제 선택된 XDC1이 아님을 유지한다. 시험에서 기대 Profile ref를 raw 정수와 그 reference-only fixture의 독립 SHA로 구성한다. 이3개를 현재 codec으로 검증하는 새 실행은 r0 계산 자체와 별도다. core의 Committed/Aborted 및 SystemFinished는 raw field·정수와 SHA로 새 정상 variant를 구성하고 실제 terminal 효과를 주장하지 않는다.

| 고정 시험명 | 정확 범위 |
| --- | --- |
| `descriptor_literals_bind_exact_owner_profile_and_state` | 기존3 literal의1536 bytes·SHA·독립 필드와 encode 비교. 거래4 state tag의5 variant(Active, Aborting Statement, Aborting Transaction, Committed, Aborted)·System4종×4phase=16 정상 상태를 추가하고 Free는 거절 |
| `descriptor_lengths_and_every_byte_damage_reject` |3 literal×prefix0..1535 및1537 거절, 각1536 byte의 bit0 한 번 변경=4608 checksum/형식 거절. 다른 bit 전수라고 주장하지 않음 |
| `descriptor_reserved_and_semantic_damage_reject_after_checksum` | reserved의 모든 byte를 하나씩 변경·재hash: 공통7·12..15·480..543·632..639·1352..1503, OwnerRef1..7, 각row1..7·120..127, SystemProgress354..359의6byte. Transaction의system128 zero도각byte변형128개를거절. 별도 state/owner/status/profile/phase/chain/budget/aggregate 오류의 아래 표 전부 |
| `descriptor_budget_sums_peaks_and_growth_are_exact` |5 category 각각에 한 row만 활성화하여14개 필드의 offset와 peak/sum 검산. row0/4의 WAL/control/page/undo/status/protected/growth 합 overflow7종, growth_extents×1MiB overflow,0/MAX peak, step0/extent배수 위반을 검증 |
| `descriptor_expected_context_and_unsupported_work_root` | 다른 DB/timeline/epoch/local/generation의 기대 TxnRef5개, 다른 system kind/id/generation3개, 다른 profile generation/digest2개는IdentityMismatch. malformed expected profile 및 timeline0 owner construction은InvalidInput. work-root 각64 byte의 단일 nonzero는UnsupportedWorkBody |
| `descriptor_encoder_errors_preserve_output` | 모든 출력길이0..1535·1537,12.2의 잘못된 spec을 각 fresh sentinel에서 거절. 정상 정확1536. source spec와 입력 literal 불변 |
| `descriptor_empty_inventory_and_chain_boundaries` | 첫/후속 chain·count0/비zero·빈 digest·release 뒤change_lsn·minimumWal 경계·System mutation sequence 경계. inventory 개수는(T,G,K)=(0,0,0)/(2,0,0)/(2,3,1)/(2,3,2) 각각하한−1(음수제외)/하한/상한/상한+1을비교하고T=MAX−1/G2/K1/itemsMAX도정상이다. actual predecessor를 읽는 시험으로 주장하지 않음 |
| `descriptor_generated_profiles_match_independent_fields` | seed0..63×sample0..127=8192. 아래 입력식·raw offset·u128 산술의 독립 기대값으로 검산 |

재checksum 부정 corpus는 base Active에서 state0/7, owner tag0/3·owner generation21(embedded20), core state mismatch·Free, profile kind2/slot1/index1/generation0/length4095, apply0/previous=apply/birth0/birth>previous/groups0/첫생성groups2/inventory_change>apply/minimumWal>apply, 각row tag0/틀린순서·step0에서WAL1·growth1, 저장 growth bytes 불일치,7개aggregate 각각+1이다. System base에서 core1byte·kind불일치·phase0/5·sequence0·cursor>limit·Finished+cursor<limit·active+phase4·finished+phase1·work-state digest 불일치·mutation_sequence1을 거절한다. 모든 변형은 fresh original을 복사하고 마지막 SHA를 다시 계산한다.

생성식은 `n=128×seed+sample`, kind=`n%6`로 Transaction Active/AbortingStatement/Committed/Aborted/SystemActive/SystemFinished를 순환한다. 거래DB=bytes1..16/timeline17/epoch18/local19+n/generation20+seed이며 head/target/next는없고 commit q=21+sample이다. System kind=1+seed%4/id21+n/gen22+seed, sequence=1+n/cursor=sample/limit=128이며 Finished만cursor=limit다. active phase=1+n%3. chain은 apply=100+n, previous=0/birth=apply/groups1, inventory count0·emptySHA·growth0·change0이다. category `(seed+sample)%5` 하나에 steps1, WAL=192+sample, control=seed, memory=4096+8n, frame=1+sample%4, lock=seed%3, registry=sample%5, FD1, DWB=sample%2, protected=1024+n을 넣고 나머지 row0이다. token/growth는0이다. consumed WAL=2n/control=n, minimumWal은sample홀수에apply이고그밖0, mutation sequence는거래만n이다. budgets/protected/peak를 입력식의 별도 정수로 검사한다.

이8시험과 기존 표현·reader leaf, 현재-build DEV2 회귀를 다음 통합 지문에서 검증한다. native fault/thread는 무효과 codec에 N/A다. 전체 DEV3 복구·queue·opcode F05·실제 append의 미검증 의무는 유지한다. 시험 이름·입력식·거절 분모를 구현 뒤 조용히 축소하지 않는다.

§12 D2 수용 원문 SHA-256은 `745efe83c3e7fd2cc3d9ea03c465045503d95eafcb03e335a0183af40752ec1a`다. 독립 검토 `artifacts/dev3/obligation-resume-review-20261021/r2-descriptor-review.md`는 inventory 개수의 u128 상·하한과 conditional reserved corpus 보완 뒤 좁은 표현만 수용했다. 현재 상태 표기는 이 검토 원문의 의미나 고정 분모를 바꾸지 않는다.

§12 구현과 같은 source의 좁은 통합·하위 회귀·현재-build DEV2 복구 결과는 [구현 기록](../implementation/dev3-implementation.md#obligation-descriptor-results)에 보존한다. 전체 inventory/의미 그룹·실제 예약 인계와 DEV3 복구의 실행 상태는 미완료다.


## 13. RollbackStart의 full state 결합 검산

2026-10-21 · **D2 수용 / 구현·국소 시험·독립 재현 PASS**. 작성·통합은 `/root`, 독립 검토는 `/root/dev3_selector_review`다. 다음 API/corpus와13.8의 구체화를 함께 채택한다. 상충하는 세부 오류/dispatch는13.8이 우선하며 실제 적용 권한은 부여하지 않는다. 같은 source의 통합·하위 회귀 상태는 [구현 기록](../implementation/dev3-implementation.md#rollback-start-results)을 따른다.

### 13.1. 지원 결과와 입력

새 private `storage/semantic_group.rs`·`semantic_group_tests.rs`에서 전체 거래의 Active→Aborting(Transaction) 한 전이만 검증한다. `observe_rollback_start(frame, expected, before_descriptor, before_page, after_page)`는 exact bytes 사이의 결합을 반환한다. 결과 `RollbackStartObservation`은 frame header, after DescriptorSpec, status location을 가진 immutable 관측이다. ApplyPlan/ReservationRestored/READY로 변환하는 API가 없다. append/encode/page write·pin·분석 prefix 채택·미래 F05 충분성을 실행하지 않는다. 현재 inventory intent 관측을 입력 권한으로 요구하거나 그 결과로 복원 성공을 추정하지 않는다.

`RollbackStartExpected`의 필드는 `FrameExpectation`, decoded `Profile`, actor `TxnRef`, `StatusSlotRef`, full status `PageIdentity`다. Profile은 기존 완전한 profile decoder만 생성하며 namespace/generation/digest에서 exact ControlRef를 구성하여 before/after descriptor에 결합한다. stream DB/timeline, profile namespace, actor namespace, PageIdentity DB는 같아야 한다. location.page는 expected PageIdentity.page와 같고 location.generation은 actor.generation과 같다. slot<71, data page index%64!=0, extent 끝×16384는 u64 범위여야 한다. writer epoch와 actor epoch의 숫자 비교로 owner 존재를 인증하지 않는다. caller가 supplied before descriptor와 stream prefix의 채택·수명을 보장한다.

실제 input의 profile/owner/schema/page identity를 기대값으로 다시 읽어 자기 검산하지 않는다. decoder는 borrowed input과 고정 locals만 사용한다. input 제외 source-level 동시 workspace≤8KiB를 정적 size/보수적 local 합으로 확인하며 compiler stack 측정이나 실제 전체 복구 예산의 증거로 쓰지 않는다.

### 13.2. exact 전이·group·page

frame은 XWF1 version1, outer Update2, payload SemanticGroup2, count1, exact3040 bytes다. group은 XSG1 version1/op3/header192/length2848, after descriptor1536, OperationCore256, NativeEffect576(B1), E/C/D0, Redo288(R1)이다. header count/length/reserved/owner/apply와 previous descriptor hash가 정확하다. Unknown version/payload는 UnsupportedFormat, operation≠3은 UnsupportedOperation이다. 다른 정상 operation을 opaque 성공으로 노출하지 않는다.

before descriptor의 TransactionActive만 지원한다. 그룹과 outer의 original TxnId/previous-owner LSN/after owner가 expected actor와 같다. after chain apply=frameLSN, previous=before.apply, birth/profile/owner 유지, groups=checked+1. before.apply≤trusted previous stream LSN이다. group.previous와 outer.previous_txn은 before.apply이고 before descriptor 전체 SHA가 일치한다. apply의 동일/역행·groups overflow는 거절한다.

after status는 Aborting(Transaction{next:before.head}), head 유지, target 없음이다. OperationCore.next도 같은 ref40이고 flag bit1은 존재와 정확히 같다. 나머지 current/target/statement/mutation/undoSequence/q/cursor/logical digest/reserved는0이다. incoming은 before/after home에서 같다. unchanged head는 같은 graph edge이며 새 next의 raw 값 검산이 실제 Live undo 확인·고정 pin을 발급하지 않는다.

두 full page를 status-home loader로 완전 검증한다. NativeEffect는 kind5/flags3/redoindex0, before/after slot과 실제 offset 동일, length224, SHA와 full bytes가 실제 home와 같다. Redo는 Patch1/statuskind9/level0, full target이 expected identity와 같고 offset=해당 home의 실제 offset/length224/data=afterhome다. priorLSN=beforepageLSN, afterpageLSN=frameLSN이다. `beforepageLSN≤trusted previous_stream_lsn<frameLSN`이며 owner.apply와pageLSN사이의대소/equality는강제하지않는다.

선택 home224·pageLSN64..71·checksum tail32 외 before/after page bytes는 전부 같다. 다른 cell이 valid하게 변경되어도 거절한다. 실제 head/incoming과 다른graphfield불변을 확인하여 E/C/D0을 결합한다. 이미 적용한 recovery page에 대한 REDO skip은 구현하지 않는다.

### 13.3. 비용의 정확 보존

r1 결정대로 category Status4에서 step1/WAL3040/protected3040을 checked 차감한다. consumed_wal은checked+3040,consumed_control불변이다. 다른4category모든field, Status의control/growth/page/undo/status consumables, inventory claim 전체, minimumWalLSN,lastMutation은보존한다. Status.steps>0이면peak6개유지,0이면peak0이고zero-step규칙으로다른잔여도0이어야한다. Clr step/count는줄이지않는다.

이는3040bytes의명시한비용산술일치이다. 감소한protected는outstanding예약몫이며retained/charged보호공간을실제free했다고판정하지않는다. 미래CLR/terminal충분성·globalledger·actualprofileadmission은별도이다. 실제효과소비자는그증명을추가해야한다.

### 13.4. 모듈 경계와 오류

`semantic_group → wal_frame/status_home/obligation/control` 한 방향이다. wal_frame의 frame envelope와 single redo 검산을 같은 의미의 낮은 helper로 공유한다. 기존 일반 FrameView/encode_frame/scan_frame_prefix는Opaque0/PageRedo1만지원하며tag2를계속Unsupported로거절한다. 새로운exactsemanticenvelopehelper는tag2만수락한다. partial-header의legacy허용범위를넓히지않는다. borrowed envelope에는semantic완료권한이없고새groupconsumer만전부검증한뒤관측을반환한다. single redo helper는기존validate_frame_record가소비하는동일shape/LSN/INIT검사를공유한다.

page owner의 `PageView::cell_range(slot)`는검증된directory에서start..end를반환하며기존cell도이를소비한다. StatusHomePage는필요한pageLSN/cellrange/rawcell만immutablegetter로전달한다. 상위module이directoryoffset공식을복제하거나pointeraddress를빼서위치를구하지않는다. 이getter는pin/lifetimecapability가아니다.

오류는 InvalidInput,Truncated,UnsupportedFormat,UnsupportedOperation,Corrupt,IdentityMismatch,OrderMismatch,BudgetMismatch다. 순서:

1. expected context 모순은InvalidInput.
2. before descriptor를trusted owner/profile로decode. 형식손상Corrupt/unsupportedversionUnsupportedFormat/identity다름IdentityMismatch. beforestate가Active아니면UnsupportedOperation, before.apply>prevstream이면OrderMismatch.
3. outer exact envelope를검증한다. validproperprefix는Truncated; unknownversion/payloadUnsupportedFormat,identity다름IdentityMismatch,LSN/predecessor오류OrderMismatch,길이/trailing/checksum/shapeCorrupt. 완전한envelope의unknowngroupoperation은UnsupportedOperation.
4. supportedgroup의header/afterdescriptor/core/native/redo형식과ownerchain을검증한다. 구조Corrupt,identityIdentityMismatch,previous/hash/chainOrderMismatch.
5. 두fullpage를검증하고actualtarget/cell/content/whitelist를대조한다. identity차이IdentityMismatch,잘못된canonicalbytes/body결합Corrupt,pageLSN조건OrderMismatch.
6. typed-valid before/after descriptor 사이의비용/보존필드오류는BudgetMismatch다. descriptor 자체의zero-step형식오류는앞선Corrupt다.

입력bytes는오류/성공모두불변이다. 이후 source변경·동시page변경은검산후수명계약의몫이다.

### 13.5. 독립 literal

`artifacts/dev3/rollback-start-vectors-r1/vectors.py`·vectors.json에 fullrawbytes를계산했다. profile1과2-iteminventory는보조입력이며제품fixture후보는None/Some head각각 frame3040·beforedescriptor1536·afterdescriptor1536·beforepage16384·afterpage16384의10개다. 소스포함actualprofile1/2/3은기존intent3literal을재사용한다.

DB=bytes1..16,timeline17,streamepoch19,actorTxnepoch18/local19/generation20,profilegen23. actorbeforeapply30/previous20/birth20/groups2,afterapply40/previous30/birth20/groups3;prevstream37. before/afterconsumed8192/11232,minimumWal20,lastmutation1. inventoryPage129/inc9와UndoSlot(page130/inc12/slot0/gen11)는Clrcategory이고change30,각units1,count2/growth0이다.

statuspage FileId(2,1)/page65/inc4,Object55/gen56,pageLSN35→40. slot0은다른ActiveTxn(local20/gen21)/incoming7,slot1은actor/incoming2이다. actorcelloffset15904이다. Some head는FileId(2,1)/page131/inc9/slot1/gen7이며next로보존한다. scalarcore/homebytes는원문offset에서생성한다. originalr0은reservedPage65가실제statusPage65와겹치는제안입력이었으므로r1에서129/130으로분리했다. r0은제안이력이며채택fixture로쓰지않는다.

beforeClrrow는(steps2,wal4096,page1,undo1,frames2,memory65536,files1,dwb1,protected22964),Terminalrow는(steps1,wal4096,frames1,memory16384,files1,dwb1,protected4096),Statusrow는(steps1,wal3040,frames1,memory65536,files1,dwb1,protected3040)이며나머지값0이다. afterStatus는0이고다른row는그대로다. 이수치는literal의비용일치입력이며미래opcode충분성증거가아니다.

### 13.6. 고정12시험

| 이름 | 고정 범위 |
| --- | --- |
| rollback_start_literals_bind_full_state |10literal의길이/SHA/필드와None/Some각각actualpage/core/home/owner/predecessor/next/consumed일치. 3실제profile과기대ref대조. source input전후불변|
| rollback_start_lengths_and_damage |2frame의모든properprefix0..3039와trailing1거절;2beforedescriptor의properprefix0..1535와trailing1거절;4before/afterpage의properprefix0..16383와trailing1거절.2frame의각byte bit0 총6080손상거절|
| rollback_start_group_sections_are_exact | group magic/version/headerbytes/length/flags와count B/E/C/D/R·각sectionlength·reserved각byte를fresh원본에서변경후SHA재계산하여거절. group version2UnsupportedFormat, op1/2/4..11/0/65535UnsupportedOperation. frame payload0/1/3UnsupportedFormat. exactshape외extra/trailingsection거절|
| rollback_start_owner_chain_and_profile_are_bound | trusted DB/timeline/stream epoch/prevstream/Txnlocal/generation/status ObjectId/Generation/Profile generation/digest의유효불일치. group owner/prev/hash,afterowner/profile/birth/previous/groups 변경;beforegroupsMAX의overflow. nativepage의selfcopiedidentity를context로사용하지않음|
| rollback_start_operation_core_matches_head |None/Some 정상; current/target/statement/mutation/UndoSequence/q/cursors/digest각필드nonzero,unknownflags·reserved각byte거절;next없음/다른ref/flag불일치,afterhead다름,target있음,AbortingStatement/Active/Committed/Aborted로afterstate변경거절. beforeAbortingStatement/AbortingTransaction/Committed/AbortedUnsupportedOperation|
| rollback_start_status_pages_are_complete |비선택cell의손상·wrongnamespace·duplicateTxnId·Freeincomingnonzero 거절. slot0/1/70 정상(valid71홈page),slot71·freeactor·wronggeneration·otherTxn조회거절. literal2홈중다른home변경없이actor만전이|
| rollback_start_redo_and_native_match_exact_cell | nativekind/flags/index/before·afterslot/offset/length/SHA/body각불일치,redo kind/target FileId/page/inc/owner/gen/kind/level/prior/offset/length/data각불일치거절. body/redo는mutatedpage와같지만expectedcontext다른경우도거절. Nativebefore192/after192의oldlayout거절|
| rollback_start_preserves_other_page_bytes |두page가각각canonical인상태로다른homeincoming/core를바꾸고SHA재계산한반례;cell삽입/제거/순서변경으로directory변경;pageowner/gen변경. 선택home의incoming/head도같이바꾼반례. afterpageLSN/전체SHA만갱신하고oldhome유지거절. wholeoldpage복사로다른home를되감은반례|
| rollback_start_cost_transfer_is_exact |steps1→0과2→1정상;Status WAL/protected3040−1/동일/+1 경계와 after정확차감;consumedMAX−3040정상/MAX−3039 overflow;Status steps0과beforewal/protected부족;4다른category의14field각각변경·Statusconsumable5필드변경·peak6변경·inventory4필드/minimumWal/lastMutation/consumedControl변경. typed-valid변형의BudgetMismatch와zero-step형식Corrupt를구별. 13.8의 고정 matrix 적용|
| rollback_start_ordering_uses_stream_and_page_positions |ownerapply30/page35/prev37/new40와ownerapply36/page35/prev37/new40정상. pageLSN0/37정상,38/40/41거절. ownerapply37정상,38거절. frameLSNMAX정상과equal/decreasing/0거절. 원래owner epoch와새stream epoch달라도정상;recovery권한은주장안함|
| rollback_start_legacy_frames_and_limits_are_preserved |기존wal11시험전체유지. Opaque/PageRedoexactfixture·1MiBbound·alllegacytail분모는원래그대로. 새로운tag2를legacyexact/prefix가Unsupported로거절(보이는tag전의partial결과는원래규칙);tag2redo를groupheader에서iterator로읽는오류없음. sharedgetter의page/status기존시험도유지|
| rollback_start_generated_profiles_match_independent_transition |seed64×sample128=8192아래raw생성식. expectedrawbytes/수치에서관측비교;제품encoder로정답생성안함. source-level8KiB예산및immutableinput확인|

13.8의 독립 검토 보완표가 비용 matrix·reserved offset·오류 우선순위의 정본이다. 이를 포함하여 D2를 수용했으며, 채택은 제품 실행 결과가 아니다.

### 13.7. 생성식

n=128seed+sample,b=100+16n. profileID=1+n%3,profilegen23;actorlocal19+n,generation20+n,epoch18+n%2;stream epoch20+n%3. beforeapply=b+3+3(n%2),previous=b+2,birth=b+1,groups2;prevstream=b+7,new=b+10,pageLSNbefore=b+5. lastmutation1+n,minimumWal=b+1,inventorychange=b+3. consumedbefore8192+3040n,after+3040.

statuspage file(2,1),page65+64(n%3),inc4+n,Object55/gen56. home3개이며actor slot=n%3,다른두Txnlocal20000+3n+slot/gen21+slot,headNone/stateActive/incoming7+slot로고정해중복안됨. actorincoming=n%9. actorheadSome iff n%2=1,이때UndoRef file(2,2)/page131+64(n%5)/inc9+n/slot n%37/gen7+n. next는그대로다.

inventory는file(2,3)의Page129/inc9와UndoSlot130/inc12/slot0/gen11,categoryClr/units1/change=b+3이며ownerordinal0이다. Clr/Terminal은literal행과같다. beforeStatus.steps=1+n%2,wal=protected=3040×steps,peak는literal과같고after.steps−1,wal/protected−3040,남은step0이면peak0이다. 다른값0이다. afterpage의actualcell만바꾸고LSN/checksum을갱신한다. key/row/index/WALapply/실제reservation복원은이생성식의범위가아니다.


### 13.8. 독립 검토의 고정 보완표

#### 13.8.1. 의미/API와 dispatch

현재 Active→Aborting(Transaction)만 지원하는 범위, 원래 actor epoch 유지, before owner.apply와 pageLSN의 별도 stream 상한, full expected PageIdentity/Profile, 선택 home 밖 모든 page byte 불변, Status category의3040 차감은 서로 일관된다. `semantic_group → allocation_page/transaction_record/undo_ref/page/control/obligation/status_home/wal_frame` 의존은 기존 하위 owner decoder 재사용을 위한 것이다. generic owner codec이나 상위 역참조가 필요하지 않다.

새 exact semantic envelope는 완전한 outer self-reported 길이·checksum·stream 의미를 검사한 뒤 group dispatch로 들어간다. **고정3040/2848은 opcode3을 식별한 뒤 요구한다.** complete payload가8bytes 미만이면Corrupt, 최소8bytes에서 magic/version을 검사하고 op≠3이면UnsupportedOperation이다. known operation1/2/4..11과unknown0/65535에 다른 operation의 shape를 요구하지 않는다. opcode3이면 header192/2848·정확 section·frame3040을 검사한다. 완전한 envelope+8byte group/op1은UnsupportedOperation, 같은 op3은Corrupt인 반례를 각각 추가한다. outer known kind≠Update는Corrupt, unknown outer kind는 기존UnsupportedFormat이다.

Expected 내부의 namespace 불일치, location.page≠expected.identity.page, location.generation≠actor.generation, slot71/81, extent metadata page, 전체 extent 끝의 byte 위치 overflow는InvalidInput이다. 이 행은 malformed frame/before descriptor와 함께 주어도 우선한다. 유효 expected namespace를 바꾸는 identity 반례는 Profile/actor/stream/page DB를 함께 바꾸어 expected 자체가 유효하게 유지되게 한다. decoded Profile의 digest 반례는 raw Profile의 다른 유효 값으로 다시 decode해 만들며 private struct의 불가능 상태를 만들지 않는다.

#### 13.8.2. 정확 오류와 반례 구성

r2의 expected→before descriptor→outer→supported group/chain→full pages→비용 순서를 유지한다. 앞선 checksum/형식 오류가 목표 반례를 가리지 않도록 필요한 모든 envelope/descriptor/page SHA와 native SHA를 고친다. 실제 before descriptor가 바뀌면 group.previous_descriptor_sha도 갱신한다. 상태 전이 반례는 해당 descriptor/core/home/native/redo 사본을 함께 바꿔 각 표현이 유효하도록 만든다.

| 입력·조건 | expected |
| --- | --- |
| 정상 frame의 proper prefix0..3039 | Truncated |
| before descriptor proper prefix0..1535, full page proper prefix0..16383 | Corrupt |
| 위 세 입력의 trailing1 | Corrupt |
| outer/group/descriptor 자체 version≠1, outer payload0/1/3 | UnsupportedFormat |
| descriptor에 embedded된 core XTS1 version≠1 | Corrupt: 기존 obligation decoder가 nested error를 감쌈 |
| 실제 full page XDP1, home XSH1, embedded XTS1 version≠1 | UnsupportedFormat: 기존 page/status-home decoder의 전파를 보존. 비선택 home 변형으로 독립 도달 검산 |
| valid wrong PageKind의 KindMismatch, unknown page kind, 잘못된 level | Corrupt로 명시 매핑 |
| unknown redo kind | UnsupportedFormat: 기존 single redo decoder와 같음 |
| valid Init redo 또는 다른 known native kind를 opcode3에 연결 | Corrupt: 필수 Patch/StatusHome shape와 모순 |
| 형식이 정상인 다른 Transaction/System group owner, valid after owner/profile 불일치 | IdentityMismatch |
| malformed owner tag/reserved/TxnRef/SystemWorkRef | Corrupt |
| valid non-Active before Transaction 상태4종 | UnsupportedOperation |
| Free core를 before descriptor로 제공 | Corrupt: valid Descriptor가 아님 |
| after 상태가 typed-valid Active/Statement-Aborting/Committed/Aborted, 잘못된 head/next/target | Corrupt: 지원 전이·OperationCore와 모순 |
| before.apply>prevstream, frameLSN0/equal/decreasing, before pageLSN>prevstream, after pageLSN≠frameLSN | OrderMismatch |
| group.previous/outer.previousTxn/after.previous/after.apply/after.birth/groups/previous SHA의 유효 불일치 | OrderMismatch |
| outer.previousTxn0 또는>prevstream | Corrupt: 기존 envelope shape가 먼저 거절 |
| before.groups=MAX, valid after.groups=3과 정상 chain/SHA | OrderMismatch: checked +1 불가. after.groups0으로 바꾸어 앞선 Corrupt를 만들지 않음 |
| 실제 page의 identity 또는 선택 slot의 다른 TxnRef/generation/Free/존재하지 않는 slot | IdentityMismatch |
| actual full page reserved/중복TxnId/old192혼합/Free count비영/비선택body·외부bytes 변경 | Corrupt |
| 비선택 occupied home의 다른DB/timeline | IdentityMismatch |
| native kind/flags/index/slot/offset/length/digest/body, redo의 실제target/offset/data 불일치 | 구조·선택결합은Corrupt; valid physical/logical target identity 불일치는IdentityMismatch. priorLSN<frameLSN인 유효 불일치는OrderMismatch로 고정. priorLSN>=frameLSN은 기존 redo validator의 앞선Corrupt |
| typed-valid 비용·inventory/minWal/mutation/consumed 보존 불일치 | BudgetMismatch |
| zero-step인데 나머지row가 비영, 잘못된 inventory cardinality/합계/aggregate | Corrupt: descriptor 자체 부적합 |

상태 mismatch 반례에서 Committed에는nonzero q를, AbortingStatement에는유효 rollback discriminator를 넣고 after descriptor tag도 맞춘다. 정상 `before.apply36/pageLSN35/prevstream37/frame40`과 `30/35/37/40`을 둘 다 유지한다. pageLSN0은 byte 의미상 허용되지만 실제 복구 이력을 인증하지 않는다.

bit0 손상6080개를 전부Corrupt로 고정하면 기존 envelope 오류 우선순위와 충돌한다. lower prefix 우선순위를 보존할 때 각3040 frame에서 UnsupportedFormat은 offsets4,5,7의3개, IdentityMismatch는24..51 및72..95의52개, OrderMismatch는64..71의8개, 나머지2977개는Corrupt다. 두frame 합계는6/104/16/5954다. 이 표는 **사전 기대값**이며 제품 실행 결과가 아니다. 재hash하지 않은 body 손상은 outer trailer가 먼저 거절한다.

#### 13.8.3. typed-valid 비용 matrix

모든 row의 steps가양수이고 모든 donor field가충분한 별도 dense baseline을 사용한다. category c=1..5의 before row14값 순서(steps,wal,control,growth,pages,undo,status,frames,memory,locks,registry,files,dwb,protected)는:

`[3,10000+100c,100+c,4MiB,2,2,2,10+c,100000+1000c,20+c,30+c,40+c,50+c,20000+100c]`.

정상 after는 Status c4에서steps1/wal3040/protected3040만뺀다. inventory T30/G20/K5/items40, change30, before/apply30→after40, minWal20,lastMutation1,consumedControl64,consumedWal8192→11232다. inventory digest는 동일한32bytes다. 이 baseline은 descriptor 형식·전이 검산용 주장이고 실제40-item closure 인증으로 사용하지 않는다. 실제 inventory를 함께 만들고 싶다면 category마다 Page/Undo/Status 각2개와 count2 Growth2개인40개를 모두 겹치지 않게 배치하면 같은 수치가 된다.

각 변형은 새 baseline에서 시작하며 독립 재계산한 aggregate growth bytes·peak maxima6·protected sum·descriptor SHA·frame SHA를 반영한다. 먼저 기존 descriptor decoder가성공하는 것을 assert하고 새 observer의 exact BudgetMismatch를 확인한다. 아래148개는 `r2-budget-matrix.json`에 고정했으며 별도 Python 산술 감사가 모두 형식 제약을 만족함을 확인했다.

| matrix | 변형과 분모 |
| --- | --- |
| 다른4category×14fields×± |112개. pages/undo/status는±1, growth는±1MiB하고 **Status row의 같은field를 반대로 이동**해 전체 token/growth/claim을보존한다. 그 외field는해당scalar만±1 |
| Status control/growth/pages/undo/status×± |10개. control단독±1, growth/token은Clr row와반대이동 |
| Status peak6×± |12개. steps2가남는baseline에서각peak를±1. summary는다시계산 |
| inventory change/items/digest/growthExtents |8개. change30±1, items40±1(여전히35..50), digest는byte0 또는31의bit0, growthExtents20±1은Status.growth±1MiB와aggregate를함께수정 |
| minimumWal/lastMutation/consumedControl×± |6개. 각각20/1/64±1 |

count/digest/growth는 Claim 전체 보존 의무를 검산한다. growthExtents case는 필연적으로 row growth도 바뀌므로 isolated branch 증거라고 쓰지 않는다. paired category 반례에서는 inventory 전체를 그대로 두므로 row별 보존 검사를 직접 통과할 수 없다.

추가 경계는 기존 r2처럼 별도로 둔다. 정상steps1→0은Status row 전체0, 정상2→1은peak유지다. before.steps0 반례는그row 전체0으로만들어typed-valid를유지한다. wal/protected3039/3040/3041은각각필드에적용하며, steps1·after전체0이면부족/정확/무단잔액반환을검산한다(다른필드는정상3040). steps2에서3041→1은정상이다. steps0·after잔액1은형식Corrupt로별도둔다. consumed beforeMAX−3040/afterMAX는정상, beforeMAX−3039/afterMAX는두descriptor가형식상유효한BudgetMismatch이다. overflow를wrapped0 after값으로만시험하지않는다.

#### 13.8.4. reserved 위치·분모

`r2-reserved-offsets.json`은 모든 absolute index와 section상대index를 담는다. 아래범위는inclusive다. 전체zero field와 conditional absentref는별도 의미행이며 header reserved와혼동하지않는다.

| section | 상대 reserved/해당Transaction whole-zero |
| --- | --- |
| outer |10..11,22..23,52..55,100..103,120..127 |
| group |10..11,33..39,68..71,134..135,156..191 |
| descriptor |7,12..15,17..23,52..55,98..99,116..127,196..199,225..231,352..479,480..543,632..639,1352..1503; 각row640+128j의+1..7/+120..127; Some head258..263; Some after next338..343 |
| operation |240..255; Some next98..103 |
| native header |88..127 |
| occupied home |16..31,68..71,97..103; Some head130..135; Some after next210..215 |
| redo header |3,60..63 |
| full page |78..127 + 각home 상대위치. gap[lower,upper)는별도canonicalzero영역 |

frame sectionbases는 outer0/group160/afterdesc352/op1888/nativeheader2144/nativebefore2272/nativeafter2496/redoheader2720/redoafterhome2784다. None frame681,Some frame729=1410개다. 별도 before descriptor None468/Some474=942개, full pages None104+104/Some110+116=434개이다. **여덟 독립 input 역할 총2786개 reserved-byte mutation**을 fresh원본에서각각1로바꾸고 enclosingSHA를고친 뒤Corrupt를assert한다. afterdescriptor standalone fixture는frame내동일bytes이므로중복분모를만들지않는다.

OperationCore의 unused statement/mutation/UndoSeq/current/target/q/cursors/logicaldigest와unknownflags, absent optionalref bytes, statuscore의q/rollback/target 의미는기존필드별음성행으로별도검산한다. page gap은reserved집합에서제외되지만canonical검산을유지하고첫/중간/마지막byte재hash반례를둔다.

#### 13.8.5. 독립 실행과 제한

raw generator를 reviewer 전용 폴더에서 다시 실행해12개원문과byte-identical을확인했다. 별도 raw audit가정수offset·SHA·section길이·ownerchain·Profile·budget summary·page directory·home/core·NativeEffect·Redo·allowed-page-diff를직접대조했다. frame3040/group2848/actoroffset15904와incoming2불변·8192→11232를확인했다. reserved input2786 위치는중복없고원문이0이며dense148cases는모두독립typed-valid산술을통과했다.

실패한 reviewer 스크립트 최초 SyntaxError는 `r2-raw-audit-attempt1.py/.log`에그대로보존했다. 수정은reviewer artifact만했으며제품source·문서·fixture를바꾸지않았다. 제품테스트/새build/문서verifier는실행하지않았다. 코드의8KiB동시workspace, 실제generichelper의legacy전분모보존, immutableinputs, generated8192와고정12시험은구현후D1·보존실행에서확인할의무다.

D2 채택 입력의 SHA-256은 `7c8ad96387ea38b89c860e7f0b17c8b3bc184dbf87f0ca5d1dac4c27659121c7`다. 원문과 독립 보완표는 `artifacts/dev3/semantic-followup-review-20261021/r3-root-adopted-contract-input.md` 및 `r3-root-adoption.json`에 고정했다. 제품 fixture는 `rollback-start-vectors-r1`의10개 bytes를 사용한다.


<a id="rollback-materialization"></a>
## 14. RollbackStart status page 구성

2026-10-21 · **D3-ROLLBACK-MATERIALIZE-001 D2 ACCEPTED / 제한된 구현·로컬 검증 완료**

`/root`가 작업·계약·통합을 담당하고 `/root/dev3_selector_review`가 독립 수용했다. 입력 결합 SHA `9c85d61c1272524ca946be02674476b9e103b948f25ff37fdd9214524beaba4a`와 고정9입력·10fixture는 `artifacts/dev3/rollback-materialize-independent-review-20261021/r1-d2-decision.json`에 있다. 이 채택은 제품 실행 증거가 아니다.

### 선택과 API

일반 PageRedo 적용은 tree arity/schema·undo owner·allocation 승인 같은 추가 context가 필요하다. 먼저 §13의 full-transaction Active→Aborting 한 전이에서 실제 after page를 caller scratch로 구성하고 기존 의미 검산에 연결한다. 별도 일반 codec이나 wire 형식을 추가하지 않는다.

`materialize_rollback_start(frame, expected, before_descriptor, before_page, output: &mut [u8; PAGE_BYTES]) -> Result<RollbackStartObservation, SemanticError>`는 기존 `storage/semantic_group.rs`에 둔다. expected와 immutable 입력은 §13과 같다. output은 caller 소유의 unpublished16KiB 사본 하나이며 성공 때만 전체 bytes가 유효하다. 오류 때 일부 또는 전체 변경될 수 있어 폐기해야 한다. immutable 입력은 항상 보존한다. heap·file·lock·callback·unsafe·추가 page-sized 내부 scratch가 없다. caller scratch는 이후 소비자가 자신의 workspace에 한 번 계상해야 하며 이 byte 함수가 runtime admission·전역 F05를 집행하지 않는다. 실제 local 크기는 wrapper와 중첩 observer의 임시값을 함께 검토한다.

결과는 기존 immutable 관측이다. live dirty frame·WAL tag2 append·page write/flush·예약 복원·DB READY·commit·recovery capability가 아니다. Init/재사용 승인과 pageLSN>=frame의 recovery skip은 지원하지 않는다. 확대 시 별도 D2가 필요하다.

### 구성과 최종 검산

1. expected 내부 일관성, trusted stream을 사용하는 complete enclosing SemanticGroup frame, Update/op3/version1, frame3040/group2848/record1을 검증한다. field가 공개된 RedoRecordView 단독값을 신뢰 입력으로 받지 않는다.
2. 기존 single redo decoder로 고정 redo section을 검증하고 Patch/TransactionStatus/level0와 expected page/owner/generation을 대조한다. 기존 observer와 동일한 envelope/target 검사를 공유할 수 있지만 기존 순서·오류 표는 유지한다.
3. full before page를 status-home decoder와 Profile namespace로 검증한다. actor/location의 실제 home을 찾고 `redo.prior_lsn == pageLSN <= expected.previous_lsn`을 검사한다. 실제 선택 home range가224bytes이고 redo offset/data length와 같아야 한다.
4. before 전체를 output에 복사한 뒤 선택 home range만 redo.data로 바꾼다. pageLSN64..72를 enclosing frame LSN으로, 마지막32bytes를 앞16,352bytes의 SHA-256으로 바꾼다. identity·directory·다른 home·free bytes는 보존한다.
5. original immutable input과 구성한 output으로 기존 `observe_rollback_start`를 호출한다. descriptor chain·native before/after·전체 status page·예약 비용까지 통과해야 관측을 반환한다. late 실패는 output 폐기이며 성공 적용으로 세지 않는다.

새 함수의 복합 오류 우선순위는 observer와 동일하다고 약속하지 않는다. expected 모순은 먼저 InvalidInput이며 enclosing/target/prior 오류는 기존 owner 범주를 따른다. 재적용된 page와 더 최신 page는 strict prior 검사로 거절하며 성공 skip을 반환하지 않는다. 기존 observer12시험과 원래 비용 경계는 유지한다.

### 고정5시험

| 시험 | 분모·기대값 |
| --- | --- |
| `rollback_materialization_matches_literal_pages` | 기존 empty-head/with-head2개에서 독립 after16KiB와 전체 bytes·관측 일치. 선택 home224·LSN8·checksum32 밖은 보존 |
| `rollback_materialization_rejects_unbound_inputs` | 아래28개 개별 반례의 exact 오류·immutable 입력 보존. 오류 output은 유효 page로 검사하지 않음 |
| `rollback_materialization_keeps_semantic_cost_checks` | §13.8.3 원래 typed-valid 비용148개 BudgetMismatch 및 아래 native12개 Corrupt. 원래 generator를 시험 전용 helper로 공유하고 추가 기존 비용 경계는 유지 |
| `rollback_materialization_generated_pages_match_independent_bytes` | 기존 §13.7 seed64×128=8192의 raw integer-offset Case를 사용해 구성한 전체16KiB·관측과 독립 after bytes·descriptor 대조 |
| `rollback_materialization_scratch_reuse_requires_success` | 동일 scratch의 정상empty→with-head late cost실패→정상with-head. 정상pageLSN/prior0, frameLSN MAX. prior35를 유지한 beforeLSN40/41 재입력은 OrderMismatch. valid Init 별도1개는 Corrupt |

28개는 InvalidInput3(expected.page DB만 다름/location.page만 다름/location generation만 다름), IdentityMismatch9(frame DB/timeline/epoch3+redo의 유효 file/page/incarnation/owner/generation5+before page file1), OrderMismatch3(frame predecessor1+prior34/36), UnsupportedFormat3(payload tag0/groupversion2/unknown redo kind3), UnsupportedOperation1(op1), Truncated1(frame proper prefix), Corrupt8(trailing/checksum2+Patch offset±1/length±1의4+before checksum/비선택home reserved2)다. 목표 검사 전에 필요한 digest는 다시 맞춘다. expected의3모순과 frame의 identity3변경을 섞지 않는다. Patch 길이 변경은 record/outer/group의 선언 길이도 맞춘 뒤 opcode3 fixed-size에서 거절한다. 잘린 input이나 추가 input이 panic을 내면 실패다.

native12는 fresh literal0에서 kind5→4, flags absolute2145의3→1, before/after slot1→0, before/after offset−1, before/after length224→223, before/after SHA 첫byte xor1, before incoming+1와 before SHA 갱신, after incoming+1와 after SHA·redo data 함께 갱신이다. enclosing frame SHA를 모두 맞춘다. native bytes2..3은 redo index:u16(현재0)이며 flags와 구별한다. 마지막2개는 실제 before 결합과 incoming 보존을 각각 검산한다.

정상 pageLSN0은 redo.prior도0으로 바꾸고 owner.apply30/previous stream37은 유지한다. 정상 frameLSN MAX는 after.apply/group.LSN/after pageLSN까지 맞추며 wrap 성공을 허용하지 않는다. strict40/41은 redo.prior35를 유지한다. valid Init은 기존 시험처럼 expected target/frameLSN40의 완전한16KiB image를 가진 record를 만들고 single redo decoder의 성공을 먼저 확인한 뒤, 조정한 complete outer/group을 새 함수가 Corrupt로 거절하는지 본다. 이 추가 경계1개는28·160에 합산하지 않는다.

같은 source에서 semantic17(기존12+새5)·all-targets Clippy/fmt·통합 leaf·DEV0/1/2 회귀·DEV2 current-build recovery를 실행하고 D1을 받는다. 새 I/O/thread 효과가 없으므로 새로운 native crash/stress 분모를 만들지 않는다. 실제 dirty page·WAL apply·전체 DEV3 인수는 계속 별도다.

### 소스 연구와 재검토

[로컬 소스 목록](../reference/opensource-references.md)의 PostgreSQL 실제 revision `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`, clean checkout의 `src/backend/access/transam/xlogutils.c`390–450행을 읽었다(SHA256 `e1584d353432243f7fa6649cb946a8e633745c0712683ac62467bcc166b82748`). 복원·LSN 설치·dirty 표시와 redo skip의 경계를 참고했고 XADB의 trusted prior equality와 full semantic 검산을 유지한다. PostgreSQL의 recovery skip/live buffer 권한을 이 scratch API에 도입하지 않는다. 소스·Git 이력은 가져오지 않았다. scratch가 live frame으로 공개되거나 Init/skip·opcode·예약·tag2 append를 확대할 때 재검토한다.


§14의 실제 source3493fa22…·고정 corpus·통합/하위 회귀·독립 검토는 [구현 기록](../implementation/dev3-implementation.md#rollback-materialization-results)을 따른다. 이 결과는 live 적용·예약·전체 DEV3 인수로 승격하지 않는다.


<a id="rollback-producer"></a>
## 15. RollbackStart WAL·상태 page 생산

2026-10-21 · **D3-ROLLBACK-PRODUCER-001 D2 ACCEPTED / 제한된 구현·로컬 검증 완료**

`/root`가 계약·구현·통합을 담당하고 `/root/dev3_selector_review`가 독립 검토한다. 최종 제안과 원래 계약/source/checker/fixture, 준비 산술의 결합 SHA `0d56ec96adab9ec85953df20d29653cbb0f6c3023241045054e35bbe1cbe95f1`는 `artifacts/dev3/rollback-producer-independent-review-20261021/r2-d2-decision.json`에 보존한다. 이 채택은 제품 실행 증거가 아니다. 기준 commit은 `2473a8f3433b810ddcd55c89189bf0979760304c`, source는3493fa22…다.

### 선택·범위·자원

§13/14는 주어진 frame을 검산·구성하지만 before 상태에서 frame을 생산하지 않는다. 같은 의미 소유자에 좁은 생산자를 추가한다. 상위의 XWF1 header 복제, legacy payload 수락 범위 확대, 별도 전체 frame/page scratch와 descriptor raw-offset 생산 대안은 기존 소유·예산 경계를 우회하므로 채택하지 않는다. 기존 typed descriptor/status/redo encoder와 낮은 header 소유자를 재사용하고, 마지막에 기존 materializer/observer를 호출한다.

`produce_rollback_start(expected, lsn, before_descriptor, before_page, frame_output: &mut [u8; 3040], page_output: &mut [u8; PAGE_BYTES]) -> Result<RollbackStartObservation, SemanticError>`를 기존 `storage/semantic_group.rs`에 둔다. immutable 입력은 보존한다. caller의 서로 다른 unpublished output 두 개는19424bytes이며 오류 때 둘 다 폐기한다. 성공만 전체 frame/page의 유효한 관측이다. 출력의 고정 배열 크기는 새 동적 용량 오류를 만들지 않는다.

heap·I/O·lock·callback·unsafe·새 dependency나 내부 전체 frame/page 사본은 없다. private frame 구성 helper가 끝난 뒤 materializer를 호출한다. decoder/encoder의 임시값과 중첩 호출을 포함한 고정 source/type 회계는 caller 출력 제외16KiB 이내로 검토한다. descriptor1536·home224·transaction192·header160과 기존 observer graph를 빠뜨리지 않는다. 이는 정확한 compiler stack/RSS 측정이나 runtime admission·전체 F05 증명이 아니다. 새 opcode·영속 format·tag2 append·dirty frame·예약 복원·READY·recovery 권한을 제공하지 않는다.

### 전이와 오류 순서

1. 기존 expected 내부 검사를 먼저 한다. 모순은InvalidInput이며 lsn0 또는 lsn≤previous stream도InvalidInput이다. malformed before bytes보다 먼저 거절한다.
2. trusted actor/profile로 전체 before descriptor를 decode하여 기존 오류를 유지한다. Active 외 상태는UnsupportedOperation, before.apply>previous stream은OrderMismatch다.
3. 전체 before status page와 비선택home까지 검증하고 실제 actor/location을 조회한다. 선택 core와 descriptor status가 다르면Corrupt, pageLSN>previous stream이면OrderMismatch다.
4. after는 full Transaction Aborting, next=head/head보존이다. chain.apply=lsn, previous=before.apply, birth보존, groups의checked+1 overflow는OrderMismatch다. 다른 owner/inventory/profile/minimumWal/lastMutation/consumedControl과 무관한 category를 바꾸지 않는다.
5. Status category에서만 step1/WAL3040/protected3040을checked 차감하고 consumedWal을3040더한다. underflow/overflow는BudgetMismatch다. 잔여step0이면 peak6개만0으로 한다. 그때 WAL/protected/control/growth/page/undo/status 잔여가 남으면 유효 after descriptor를 만들 수 없으므로BudgetMismatch다. 전체 derived descriptor를 encode 전에 검증한다. Terminal·CLR category는 소모하지 않는다. 기존 observer가 외부 malformed after에 반환하는Corrupt 순서는 유지한다.
6. caller frame 안에 기존 XSG1/op3/group2848를 구성한다. actual before hash·owner chain, 기존 descriptor encoder의after1536, optional next만 있는core256, actual old/new home과 보존 incoming을 담은native576, 실제range/target/prior의Patch288을 넣는다. 새 home·offset·incoming을 별도 caller 값으로 받지 않는다. lower finalizer로Update/tag2/count1·actor의 원래TxnId·previousTxn=before.apply를 설치한다.
7. 원래 inputs와 구성한 frame으로 §14 materializer를 호출해 caller page를 만들고 전체 의미·비용 검산이 성공해야 관측을 반환한다. byte 구성만으로 live 적용 권한을 발행하지 않는다.

### 낮은 envelope finalizer

기존 `storage/wal_frame.rs`에 `pub(super) finish_semantic_envelope(header: WalFrameHeader, record_count: u16, output: &mut [u8]) -> Result<(), WalError>`를 둔다. output은 준비한 payload와 header160/trailer32 공간을 포함한 **전체 frame slice**다. header 검증을 먼저 하고 길이<192는NoSpace, 길이>1,048,576은TooLarge로 거절한다. 오류에는 output 전체를 보존한다. 성공 때 payload160..len−32를 보존하고 tag2 header와 전체 SHA만 설치한다. count0/65535와 임의 payload도 envelope 값으로만 수락하며 group·opcode·redo의 의미 검증이나 적용 권한은 없다. 기존 encode_frame과 private header 조립을 공유하고 legacy 오류·suffix 보존·tag2 거절을 유지한다.

### 고정 시험과 정답

기존semantic17와WAL11을 유지하고 semantic5/WAL2를 추가한다. 같은31개 named suite 안의 총 분모를 정확히 갱신한다. 원래literal2와raw offset 생성8192개의 전체frame/page가 독립 정답이며 product encoder·생산자로expected를 계산하지 않는다.

| 추가 시험 | 고정 범위 |
| --- | --- |
| `rollback_producer_matches_literal_frames_and_pages` | empty/with-head2개와기존dense1개. frame3040/page16384전체·after descriptor·관측 대조, immutable보존 |
| `rollback_producer_rejects_invalid_before_context` | 아래29개 exact오류, literal2의before descriptor properprefix1536+page properprefix16384와trailing각1의총35844, expected/lsn우선복합2개 |
| `rollback_producer_budget_and_lsn_boundaries` | typed-valid before예산실패11개와정상경계10개, 전체expected frame/page대조 |
| `rollback_producer_generated_frames_match_independent_bytes` | §13.7의raw seed64×128=8192전체frame/page/after 대조 |
| `rollback_producer_reuses_unpublished_outputs` | 동일출력쌍으로empty성공→예산실패→with-head성공, 실패출력폐기 |
| `wal_semantic_finalizer_preserves_payload_and_legacy_boundary` | literal2+length{192,1048576}×count{0,65535}의4셀=6정상. payload보존·독립header/digest, envelope수락과legacyexact/두TailPolicy거절 구분 |
| `wal_semantic_finalizer_rejects_header_and_size_without_mutation` | 아래15오류에서전체output불변, 원래11WAL시험보존 |

29개는InvalidInput6(expected.pageDB만/location.page만/location.generation만/lsn0·37·36), IdentityMismatch8(actorlocal20·유효Profile2·expectedfile/incarnation/owner/owner_generation4·slot0다른actor/slot2없음), UnsupportedFormat2(descriptor/pageversion2), UnsupportedOperation4(AbortingTransaction/AbortingStatement/Committed/Aborted), OrderMismatch3(beforeapply38/pageLSN38/groupsMAX), Corrupt6(descriptor/pageSHA·비선택home reserved·duplicateTxnId·같은actor의다른validhead·잘못된pagekind)다. identity만바꿀때expected.location도같은PageRef를유지하고목표검사까지필요한SHA를맞춘다. 복합2개는expected모순+빈before+잘못된lsn, validexpected+잘못된lsn+빈before다. 두 복합 시험은 preflight가 malformed source보다 먼저임을 구별한다. expected와 lsn 오류는 모두InvalidInput이므로 둘 사이의 상대 순서는 이 실행만으로 구별하지 못하며 D1 source 검토로 확인한다.

예산실패11개는Statusrow전체0, WAL3039, protected3039, consumedMAX−3039 및마지막step에WAL3041/protected3041/control1/growth1extent/pages1/undo1/status1이남는7개다. 각각기존before decoder성공을먼저확인한다. pages는CLR.pages1을Status.pages1로, undo는CLR.undo1을Status.undo1로, status는CLR.undo1을Status.status1로옮긴다. growth는inventorygrowth1/items3으로맞춘다. 이inventorydigest는opaqueclaim이며actualinventory내용검증으로주장하지않는다. raw산술입력11개와script는제안단계에보존했다.

정상10개는steps2/WAL3041/protected3041→steps1/WAL1/protected1, consumedMAX−3040→MAX, pageLSN0/37, ownerapply37, frameLSNMAX, groupsMAX−1→MAX,71home의actor슬롯0/1/70이다. 다른row/소유token/inventory/consumedControl/peak보존은dense와8192생성시험으로함께대조한다.

lower15오류는timeline0/epoch0/lsn0/lsn=prev/prev>lsn/txn없음/txnDB다름/txnTimeline다름/previousTxn0/previousTxn>prev의InvalidInput10개, length0/191의NoSpace2개,길이1,048,577의TooLarge1개, 잘못된header+짧은/초과length의InvalidInput2개다. 정상lowercontainer도semantic적용승인을뜻하지않는다. 생산자출력은상위전체검산을따로통과한다.

### 검증·연구·재검토

같은동결source에서semantic22·WAL13·checker7·all-targets/all-featuresClippy/fmt·전체leaf·DEV0/1/2회귀·현재buildDEV2복구native99를실행하고D1및최종raw증거감사를받는다. 새I/O/thread효과가없어새nativecrash성공을주장하지않는다. DEV2PF04실패·전체DEV3NOT_RUN은유지하며문서검사는pushgate에한정한다.

[로컬참고목록](../reference/opensource-references.md)의PostgreSQL cleanrevision `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`, `src/backend/access/transam/xloginsert.c`150–214/404–452/625–678행(SHA `beaf574f4527b69608a70cf6c5a7a91c7d35656fb2e9b8fe1eb16053421f9f5e`)에서준비/삽입분리와boundedpayload/fixedheader조립을참고했다. source/history를가져오지않았다. tag2실제append·live적용·다른opcode·inventory변경·16KiB회계초과에는별도재검토가필요하다.


§15의 동일 source 제품 실행·독립 검토·통합 증거는 [구현 기록](../implementation/dev3-implementation.md#rollback-producer-results)을 따른다. 실제 tag2 append·dirty frame·전체 DEV3 인수는 별도다.

<a id="begin-preparation"></a>
## 16. D3-BEGIN-001 — 최초 빈 거래의 Begin 준비

2026-10-23 · **D2 ACCEPTED / 순수 준비 구현·선택 검증 완료 / 제한 단위 최종 D1 수용**. 기준 `742aaea`, 작성·통합
`/root`, 독립 검토 `/root/pool_design_review`. 사용자 DEV3 개발 재개와 DP01 위임을 따른다.
3.09.1-A의 실제 live identity/status와 첫 행 거래를 향한 선행이다. 일반 page owner·durable
발급·Begin stage/apply·rollback/terminal·현재-build 복구까지가 후속 통합 목표이며,
아래 순수 준비 결과로 그 목표를 완료 처리하지 않는다.

### 설치 의미와 후속 owner의 필수 입력

- 영속 TxnId→location의 정본은 현재 allocator가 Allocated로 승인한 Status home의 occupied
  cell 집합으로 둔다. 별도 RAM map을 정본으로 만들거나 local에서 주소를 계산하지 않는다.
  후속 owner는 selected namespace와 RAM allocator/WAL membership을 고정하고 home을
  bounded streaming으로 순회해 전체 TxnId 유일성·exact location/generation을 검산한다.
  resident dirty home은 같은 authoritative pool에서 읽고 disk의 옛 bytes로 대체하지 않는다.
  source latch 해제 전 transient protection과 I/O 뒤 재검산, scan/pin/FD/F05가 실제 소비의
  필수 조건이다. 이 준비 함수는 전역 census나 pin 권한을 발급하지 않는다.
- 새 TxnId는 현재 writer epoch와 현재 issued local high-water의 checked `+1`이다. 준비
  함수는 candidate를 관측할 뿐 ID를 발급하지 않는다. 후속 live owner는 단독 발급 경계에서
  RAM issued high-water를 먼저 전진시키고 반환한 ID를 실패 뒤에도 재발급하지 않는다.
  채택된 WAL high-water와 RAM issued high-water를 혼동하지 않는다. live owner는
  같은 epoch의 채택 Begin 및 checkpoint high-water를 복원하고 terminal 뒤에도 되감지 않는다.
  복구 후 epoch 변경 전 Begin을 허용하지 않으며 새 epoch local은 별도 원장으로 시작한다.
  채택되지 않은 이전 epoch candidate도 새 durable epoch의 identity와 달라 재발급되지 않는다.
  old-epoch occupied home은 유효한 directory 항목이고 새 Begin의 actor로 재발급하지 않는다.
- 모든 기존 Free(g) 활성화는 checked `g+1`을 사용한다. 따라서 현 INIT의 Free1은 Occupied2가
  된다. 최초 slot 예외를 Free bytes에서 추정하지 않는다. page incarnation·frame generation과
  별개다. 실제 free/reuse 자격은 horizon/edge/queue/pin/claim retirement owner가 증명한다.
- 이번 Begin은 undo head 없음·incoming0·mutation sequence0의 빈 Active 거래를 만든다.
  Status와 directory는 동일 home 한 개의 설치로 원자 변경되므로 별도 directory redo는 없다.
  단일 home native576·Patch288, edge/count/inventory delta0의 완결 그룹이다. 기존 소유된
  Allocated Status page의 Free slot을 쓰며 PageToken/UndoSlot/StatusSlot 예약을 임의 생성하지
  않는다. 이 경우에는 별도 reserve group/선행 영속 효과 없이 첫 Begin group 안에서 Free
  slot의 일시 claim을 직접 점유한다. 실제 owner는 claim의 origin·세대·빈 slot 검산부터 Begin
  stage/apply까지 직렬화하고 다른 Begin/free/reuse를 배제해야 한다. §6.5의 기존 영속
  StatusSlot Item을 consume하는 경로와 구별하며, 그 Item이 존재하면 이 분기로 우회하지
  못한다. 준비 함수의 expected는 신뢰한 caller 입력에 대한 관측 계약이고 실제 admission
  token 또는 frame binding이 아니다.
- 빈 거래의 초기 예약은 full RollbackStart3040 + empty AbortEnd3040의 WAL/protected6080이다.
  Begin 자체3040을 더해 첫 effect 전9120을 확보한다. after descriptor는 consumed_wal3040,
  consumed_control0, Terminal row2와 Status row3 각각 steps1/wal3040/protected3040,
  peak=(frames2,memory114688,locks0,registry2,files5,doublewrite1)이며 다른 row는0이다.
  inventory items/growth0, digest=SHA256(empty), change_lsn=Begin LSN; minimum_wal_lsn=birth;
  previous0/groups1/last_mutation0이다. 이는 미래 행 변경의 abort 비용이 아니다. Forward는
  해당 opcode 전체 최악 비용을 추가 admission한 뒤만 가능하다. empty AbortEnd는 undo/head/
  incoming/pending retained/queue/retirement 의무 모두0 조건에서 동일3040-byte 한-home
  전이로 후속 구현한다. terminal 뒤 home은 기존 system Status-page 소유 아래 directory에
  유지하며 자동 Free/reuse나 추가 cleanup을 약속하지 않는다. §7.5의 credit/owner 인계를
  생략할 수 있는 일반 예외가 아니며, 인계할 의무가 하나라도 있으면 이 empty 계획을 거절한다.
  실제 잠금 반환·ATT 제거·예약 반환은 그 durable terminal과 공통 owner 검산 뒤다.
  terminal/rollback·해당 F05의 실제 채택 전에는 이6080을 충분한 live admission 증거로
  사용하지 않고 성공 Abort나 재사용도 제공하지 않는다.

### wire·API·검산

기존 XSG1/descriptor/home224/Patch 형식을 그대로 쓴다. frame3040, group2848과 section
범위는 §15와 같고 operation=1, outer kind=Begin, actor=current epoch, previous_txn_lsn0,
group previous0/before-descriptor SHA0이다. OperationCore는 tag1/length256 외 전부0이다.
선택 before home은 Free(g)/incoming0, after는 같은 slot의 Active(actor,g+1)/headNone/incoming0.
선택 slot은 실제 존재하는0..70, page_no는 extent metadata가 아니며 actual before pageLSN은
nonzero이고 global predecessor 이하이다. new LSN은 global predecessor보다 크다. global
predecessor와 page prior LSN의 equality를 요구하지 않는다.

`semantic_group::begin`의 `BeginExpected`는 FrameExpectation, Profile, actor TxnRef,
location StatusSlotRef, PageIdentity, previous_local을 받는다. actor namespace/current epoch,
local high-water+1, location/page/generation과 슬롯 한도를 먼저 검산한다. source의 full page
및 모든 cell을 검산하고 actor TxnId가 다른 cell에 이미 있으면 거절한다. after descriptor는
위 고정 초기 예약과 chain에서 계산하며 frame 자체의 값을 trusted expected로 사용하지 않는다.

`observe_begin`은 frame와 before/after 전체 page를 읽고 header/모든 section/descriptor/native/
redo/actual page binding 및 허용 byte 변화(선택 home·pageLSN·checksum만)를 검사한다.
`materialize_begin`은 caller after-page에 patch하고 observer를 통과한 immutable observation만
반환한다. `produce_begin`은 caller3040-byte frame과16KiB page 출력에 같은 결과를 준비한다.
오류 때 입력은 불변, 출력은 폐기해야 하며 I/O/heap/lock/실제 stage·발급·apply permit는 없다.
잘못된 expected/new LSN은 InvalidInput, 유효 expected와 다른 identity는 IdentityMismatch,
LSN/predecessor는 OrderMismatch, Free가 아닌 선택 slot은 UnsupportedOperation,
slot 세대/MAX는 OrderMismatch, canonical bytes/고정 budget 불일치는 Corrupt다.
길이·version·opcode는 기존 envelope 의미를 따르며 unknown op는 UnsupportedOperation이다.

### F03/F05와 실행 선택

raw offsets·독립 SHA로 before/after/frame literal을 작성해 생산 결과 전체 bytes를 대조한다.
중간 필수는 정상 producer/observer/materializer3, slot0/70·다른 home 보존, g+1/MAX·현재 epoch/
local+1/MAX, page prior≠global predecessor, malformed envelope/group/descriptor/native/redo,
before/after 손상·duplicate actor·비선택 변화, 모든 frame proper prefix·trailing byte,
출력 실패/입력 불변과 정확 초기 ledger다. 원 실패는 별도 source로 보존한다.
최종 S5에는 profile1/2/3×slot0..70×generation1/19/MAX−1의639조합을 작성하고 중간에는
실행하지 않는다. 이 순수 준비 API에는 native I/O·새 borrow compiler proof가 해당하지 않는다.
실제 stage/apply를 붙일 때 native/reopen/fault/compiler 검증을 추가하며 미실행을 PASS로
표시하지 않는다. 기존 RollbackStart·하위 wrapper·무변경 시험은 반복하지 않는다.

caller before/after page 및 frame은 입력/출력 loan이며 숨은 page-sized local copy는 없다.
component64KiB 안에 expected/observation/descriptor/home/page view/redo/header/error carrier의
중첩 수량과 encoder/hash scratch를 source-value 식으로 합산한다. 코드 구현 뒤 실제 타입
크기와 선택 layout 시험을 독립 검토한 후 runtime을 실행한다. retained engine 전체/RSS/OOM
인증이 아니다. 각 필수 runtime30초, 전체 필수120초, S5 전체120초, fixture1MiB·보존8MiB
한도이며 범위를 사후 축소하지 않는다. 문서 검사는 push 전용이다.


보존 한도의 범위를 명시한다. fixture1MiB는 한 번에 활성인 raw page/frame/descriptor 시험
데이터이며, 결과보존8MiB는 이 단위의 retained case vectors·선택 실행 로그/receipt·review
기록에 적용한다. immutable source export·ELF·compiler/cache는 도구 보존물로 별도 경로와
실제 bytes를 기록하며8MiB 충족 주장에 포함하지 않는다. 원 D2 문구의 범위 명시 누락을 보완한
것으로, 원문·원 결과는 보존하고639개 동시복제 금지와 순차 생성·재사용 기준은 바꾸지 않는다.

대안인 Free predicate만 완화하거나 임의 Active fixture를 live 입력으로 주입하는 방식은
directory·발급·예약의 선행을 공급하지 못하므로 제외한다. 일반 pool 전체를 먼저 만드는
방식과 비교해 Begin의 실제 native 집합/초기 budget을 먼저 고정하면 소비 pool의 whole pin
집합을 구체화할 수 있다. tree는 별도 system-group 소비자로 이어질 수 있다. 재검토 조건은
directory lookup 비용/scan 안정성, 실제 empty terminal 비용 초과, 최초 reserve/slot 발급의
충돌, 추가 native 효과·pending claim이 필요한 경우다.

[선택 실행 증거](../evidence/dev3-status-begin-20261023.md)는 frozen D2·source별 원 FAIL과 runtime12/static1을 구별한다. 실제 owner·복구·S5 인수는 후속이다.


<a id="empty-abort-preparation"></a>
## 17. D3-EMPTY-TXN-001 — 빈 거래 통합의 AbortEnd 준비 경계

2026-10-23 · **D2 ACCEPTED / 순수 준비 선택 검증 완료 / 제한 D1 수용**. 기준 `4037cb6`, 통합 `/root`, 독립
검토 `/root/pool_design_review`. [AI 개발 효율 계획](../plans/dev3-ai-throughput-improvement.md)을
사용자 지시로 이번 작업에 적용한다. 작업의 종료점은 실제 Begin→RollbackStart→AbortEnd와
새 process의 동일-build 닫힌 복구다. 이 절은 그 연결에 필요한 순수 terminal 준비 경계이며,
별도 helper 완료를 통합·거래·READY 완료로 세지 않는다.

### 순수 빈 AbortEnd의 exact 전이

§16 Begin과 full RollbackStart를 순서대로 채택한 빈 거래만 받는다. trusted expected는
FrameExpectation·Profile·TxnRef·StatusSlotRef·PageIdentity이며 parent의 identity 검산을
공유한다. recovery용 old-epoch actor를 거절하지 않지만 이것이 새 WAL append 권한은 아니다.

- before descriptor: Aborting(Transaction, next=None), head=None; chain groups2,
  previous=birth, birth>0, apply>birth 및 apply≤global predecessor. inventory items/growth0,
  SHA256(empty), change=birth; minimum=birth, last mutation0, consumedWal6080/control0.
  remaining은 Terminal row2만 §16의 steps1/wal3040/protected3040/동일 peak이고 나머지0.
- actual before page: before descriptor.apply≤pageLSN≤global predecessor, 선택 home이 descriptor와 같은
  actor/generation/Aborting이며 incoming0. 모든 다른 cell도 typed decode하고 그대로 보존한다.
- after descriptor/home: 같은 actor의 Aborted/headNone/incoming0. apply=newLSN,
  previous=old apply, groups3, consumedWal9120, 모든 remaining0; 다른 필드는 보존한다.
  home을 Free로 바꾸거나 TxnId·slot generation을 반환하지 않는다.
- frame3040/group2848, §15의 section 범위와 native576/Patch288를 재사용한다. opcode7,
  outer AbortEnd, group/outer previous-owner=before apply, before descriptor SHA 일치.
  OperationCore는 tag7/length256 이외 전부0; edge/count/inventory delta0. native는 같은
  한 home의 full before/after와 hash, redo는 실제 old pageLSN·slot offset·after224를 묶는다.
  선택 home·pageLSN·checksum 이외 page bytes 변화는 거절한다. newLSN>global predecessor.

`semantic_group::empty_abort`의 observer/materializer/producer는 실제 입력을 검산한 immutable
observation만 반환한다. immutable input은 오류에도 보존하고 변경했을 수 있는 caller 출력은
폐기한다. I/O·heap·lock·identity 발급·live 예약 반환·ATT 제거·apply permit는 제공하지 않는다.
잘못된 expected/newLSN은 InvalidInput, identity 불일치는 IdentityMismatch, chain/LSN은
OrderMismatch, 비어 있지 않거나 다른 rollback 상태는 UnsupportedOperation, exact empty
budget/consumed 불일치는 BudgetMismatch, malformed wire/after/native/page 결합은 Corrupt다.
기존 envelope의 길이/version/opcode 오류 의미는 유지한다. 여러 결함의 최우선 오류를 계약하지
않으며 각 독립 반례는 한 의미 경계를 고정한다.

### 통합 전에 닫을 owner와 복구 조건

실제 owner는 pure observation과 별도로 pending install·queue·retirement·lock·transient
의무가 없음을 검산해야 한다. durable terminal과 Status 설치 완료 전 예약 반환/성공 Abort를
하지 않는다. 원 system allocation descriptor를 transaction descriptor로 덮어쓰지 않는다.
issued local은 효과 전 소모하고 실패 후 되감지 않는다. replay는 동일 epoch의 채택 Begin
local이 strict 증가하는지 검산하되 미채택 candidate의 gap은 허용한다.

전역 WAL tip과 allocator pageLSN을 분리해 각각 exact chain으로 검산한다. live Status typed
read/load/flush, resident 우선 directory census, 최대32 retained transaction의 F05와 type
수명, 주소별 canonical before·최종 pageLSN·historical Pending 및 ledger 한 번 채택을 실제
통합 D2에서 닫는다. 기존 wide Result와 transaction codec/ledger의 동시 보유는 허용하지 않는
consuming phase를 검토한다. 현재 단계의 제안이며 수치/borrow proof 전 effect 실행은 없다.
새 epoch/REBASE 전 closed recovery는 Active/Aborting의 남은 의무를 보존하고 관측만 한다.
자동 loser 종료·같은 epoch 새 Begin·READY를 제공한 것으로 표시하지 않는다.

### 필수 준비 검증과 남는 인수

독립 raw offset/SHA literal의 observer·materializer·producer 및 실제 Begin→RollbackStart→
AbortEnd bytes 연결을 검산한다. slot0/70·generation 경계·profile1/2/3, old epoch, page prior와
전역 predecessor 차이, nonempty head/incoming/inventory·statement rollback, budget 부족/초과,
chain/descriptor/native/redo mismatch, 비선택 cell 변화, 모든 frame proper prefix/trailing byte,
실패 뒤 input 불변을 선택한다. 정확 selector 목록·fixture hash·source/binary 관계를 실행 전에
한 manifest에 고정한다. 실제 opcode 소비 이전의 준비 검증이며 native/reopen/fault 인수는
통합 단계에서 추가한다. 무변경 하위 시험·전체 wrapper는 실행하지 않는다.

caller frame3040+before/after 각16384는 loan이며 숨은 page-sized local copy는 없다.
component source-value census는 encoder/hash scratch와 중첩 descriptor/home/page view/error
carrier를 합산하여65536 이하를 검산한다. 실제 retained whole-engine F05는 별도이며 이 수치로
대신하지 않는다. 각 준비 case30초/전체120초, 활성 fixture1MiB, case·receipt·review 보존8MiB;
source export·ELF·cache는 별도 기록한다. S5 profile3×slot71×Free generation(1/19/MAX−1)3의639조합(actor는2/20/MAX)은 authored
selector로 남기고 중간에는 실행하지 않는다. 기존 §16의639조합을 재실행하는 것이 아니다.

# DEV3 현행 WAL prefix의 닫힌 분석

2026-10-22 · **제한 범위 구현·선택 검증·독립 D1 수용**

[복구 소유 명세](dev3-task-12-recovery.md#early-recovery) · [초기 SELECT](dev3-recovery-select.md) · [WAL 표현](dev3-wal-format.md) · [Status INIT](dev3-status-init.md)

## F01 — 지원 입력과 의미

이 단위는 실제 reserve/cancel·Status INIT producer가 만든 genesis WAL을 처음부터 읽어 owner·descriptor·inventory와 논리 page image를 재구성한다. 물리 home를 읽거나 변경하지 않는다. 결과는 RAM의 prefix 분석이며 BOOT_RESERVE, 실제 RESERVE_RESTORE, Pending repair, page별 REDO, 일반 open/READY의 완료가 아니다. 기존 초기 SELECT의 지원·거절 계약을 유지한다. 후속 복구는 이 분석을 소비하면서 빠진 장벽을 연결한다.

- profile1/2/3의 generation1/timeline1, XBS1 plan1/2, 초기 FILEMAP bank0/generation1/single extent를 지원한다. immutable XBS1과 선택 FILEMAP를 기존 canonical bootstrap encoder에 대조하고, 같은 bootstrap에서 LSN0 allocator를 만든다. home allocator를 분석의 정본으로 삼지 않는다.
- 선택 DWB는 실제 현재 producer의 Empty 또는 Pending1 image이며 namespace·선택 generation·bank ref·body를 검산한다. 초기 tuple은 batch0/generation1/Empty다. batch b≥1에서는 Empty 사전 선택 generation=3b−1, Pending1=3b, retirement Empty=3b+1만 허용한다. selector/reference/body generation이 같고 bank=(generation−1)%2, index0, body 길이는 Empty160/Pending248이다. Pending image는 **그 LSN**을 분석할 때의 canonical page 전체 hash·identity/kind/level와 비교하여 digest 하나를 보존한다. 최종 page와만 비교하지 않는다. Empty에는 과거 capture LSN이 없으므로 WAL/home의 완전성을 인증한다고 주장하지 않는다. 현재 profile 밖의 batch64나 multi-file 지원을 주장하지 않는다. selected maxLSN보다 짧은 prefix를 채택하지 않는다.
- `d3-dwb-stage`와 비선택 bank는 geometry만 검사하고 bytes를 읽지 않는다. `d3-dwb-select-stage`는 실제 flush cut의 잔재로 허용하며 regular owned file의 길이0…4096만 확인한다. 잔재를 fallback·선택 권위로 쓰거나 정리하지 않는다. CURRENT/rebase/writer-stage/filemap-stage는 계속 미지원이다. 임의 directory 탐색은 하지 않는다.
- 선택 epoch는 기존 XWE1 epoch1, WRITER는 이 header digest를 참조하는 XWS1이다. source length는4096…256MiB다. offset4096부터 tag2 XWF1/XSG1만 지원한다. source bytes·길이는 그대로 보존한다.
- reserve는 System/opcode8·AllocationCell6·allocator Patch1·Reserve delta, cancel은 System/opcode10·AllocationCell6·allocator Patch1·Release delta다. Status INIT은 System/opcode10·AllocationCell6+Status5·allocator Patch+Status Init·Consume delta·frame38000이다. opcode/길이만으로 승인하지 않고 descriptor chain/hash·native/redo 대응·count/edge·inventory·remaining 전체를 검산한다. unknown 완결 연산·tag를 no-op으로 채택하지 않는다.
- plan1의 알려진 producer mode는81920(exclusive),98304(dirty)이며 첫 reserve에서 정확한 tuple를 검산한 뒤 namespace의 mode로 고정한다. plan2는114688(Status), count1이다. 직렬화된 memory 값은 실행 권한이나 caller workspace 크기를 선택하지 않는다. terminal frame은 begin에서 기억한 mode를 따른다. INIT 뒤에는 현행 producer가 추가 WAL을 만들지 않으므로 후속 완결 frame을 거절한다.
- work ID는1부터 단조 발급되고 같은 active owner의 terminal까지 보존한다. reserve-only는 정상 active prefix다. 새로운 reserve는 이전 owner의 terminal 뒤만 허용한다. terminal을 만나도 generation·work high-water·누적 source WAL·metadata 비용은 사라지지 않는다.
- 이 지원 집합의 net0 reference proof는 E=C=0이다. AllocationCell에 undo/status reference edge가 없고, INIT Status는 Free(gen1)/incoming0이므로 그 native projection과 edge/count0을 함께 검산한다. 이 빈 proof를 후속 edge/undo opcode에 적용하지 않는다.

## F02 — 소유와 분석의 책임

닫힌 `RecoveryPrefix`는 exclusive W, 열린 source WAL의 identity/length, caller workspace의 전체 mutable borrow, profile/선택 control 관측과 분석 상태를 소유한다. observation과 canonical allocator/선택적 Status image는 owner를 빌린다. raw W/File·append·reader·mutation·ACK capability를 반환하지 않는다. Drop은 lease/borrow/handle만 반환하며 cancel·sync·retirement·cleanup하지 않는다.

실제 API는 `RecoveryPrefix::acquire<D:DirectoryPort<Writer=W>>(directory:&D, expected:Option<Namespace>, workspace:&'a mut [u8;262144]) -> Result<RecoveryPrefix<'a,W>,RecoveryPrefixError>`다. `observation(&self)->&RecoveryPrefixObservation`, `allocator_image(&self)->&[u8;16384]`, `status_image(&self)->Option<&[u8;16384]>`, `descriptor_bytes(&self)->Option<&[u8;1536]>`만 관측을 제공한다. observation은 Clone/Copy가 아니며 독립 capability가 아니다. static namespace/configured foreground limit를 실행 중 입력으로 받지 않는다.

분석은 immutable bootstrap+직전 검증 WAL에서 유도한 canonical before를 사용한다. runtime의 순수 byte/전이 불변식을 공유할 수 있으나 live StatusRole/fence/WalRequirement를 위조하지 않는다. live origin 검사와 pure expected geometry를 분리한다. 이 canonical 분석을 mixed physical home의 before 검증이나 pageLSN skip으로 재사용하지 않는다. 한 그룹의 ledger 전이는 한 번만 반영한다.

최신 raw descriptor1536, active owner/mask/birth, 고정 producer mode, work high-water, actual source length/complete end, retained WAL, PageToken·remaining WAL/page/frame/memory/files/protected를 관측한다. INIT 후 canonical Status는 완전 WAL image이며 home sync 관측이 아니다. Finished descriptor와 별도로 **새 sync 전 terminal 자원 반환을 허용하지 않는 상태**를 유지한다. terminal의 page address는 고정63-bit mask의 합집합으로 보존하며, 과거 같은 주소의 다른 generation을 새 physical page token으로 누적 charge하지 않는다. 이 mask는 미안정 source와 관련한 반환 금지 관측이며 현재 descriptor의 inventory/remaining과 별개다. source를 새로 sync하지 않은 동안 어떤 ledger 해제 capability도 주지 않는다. 이는 실제 예약 복원의 완료가 아니다.

## F03 — active EOF

semantic prefix scanner는 기존 header의 visible-field identity/order/format/length/hash 검사와 complete-envelope checksum을 공유한다. empty/end·incomplete·complete envelope를 구분하며 semantic 검증을 마치기 전 redo iterator/권한을 주지 않는다.

- physical EOF에서만 부족한 마지막 header/body를 제외할 수 있다. 그 전에 보이는 잘못된 header, 완전 길이 checksum 오류, 중간 손상은 typed 오류다. 재동기화·truncate·임의 trailing zero skip은 없다.
- 아직 존재하지 않는 suffix를 요구하는 selected Pending은 거절한다. 완결 그룹 LSN은 이전+1이며 epoch1/실제 namespace를 따른다.
- 지원 frame 최대38000을 넘는 완결 frame은 고정 chunk로 전체 frame SHA를 검산하고, 잘못된 checksum은 Wal(Corrupt), 유효 checksum이면 UnsupportedClosure를 반환한다. header상 bounded MAX_FRAME_BYTES 안의 미완결 suffix는 고정 header만으로 visible 검산하고 읽지 않은 body를 성공 검증했다고 보고하지 않는다. tail의 offset/physical bytes와 NOT_ADOPTED 상태를 보존한다.

공유 scanner API는 `semantic_frame_length(header, expected)->Result<usize,WalError>`(최소160byte 필요)와 `scan_semantic_prefix(input,expected,policy)->Result<SemanticFrameScan,WalError>`다. 후자는 End/Incomplete/Complete{envelope,consumed}만 반환한다. 기존 exact/legacy scanner 의미를 변경하지 않는다.

## F04 — 오류와 관측

사전 capacity/입력 거절, lease, profile/FILEMAP/bootstrap, DWB, epoch/WRITER, frame/tail, semantic, ledger 검산을 구분한다. acquisition의 원 effect를 보존하고 이후의 read-only 오류는 Unchanged다. typed lower Namespace/Io/Control/Wal/Semantic 원인을 잃지 않는다. 실패하면 owner/inspection 권한을 반환하지 않는다. panic을 catch하여 같은 owner를 계속 쓰는 API를 만들지 않는다.

Phase는 Preflight→Lease→Profile→FileMap→Bootstrap→Doublewrite→Epoch→Writer→Scan→Semantic→Ledger→Snapshot다. 반복 scan마다 semantic/ledger를 검산하며 마지막 Snapshot은 source named identity/length 재대조다. Cause는 Capacity, UnsupportedClosure, UnsupportedBootstrapPlan, RequiredPrefixMissing, LengthMismatch, IdentityMismatch, ArithmeticOverflow, Namespace, Io, Control, PageIo, Bootstrap, Doublewrite, Wal, Semantic을 구분한다. unknown complete operation은 Semantic(UnsupportedOperation), checksum-invalid oversized는 Wal(Corrupt), checksum-valid oversized는 UnsupportedClosure, selected Pending을 충족하지 못하는 EOF는 RequiredPrefixMissing이다. 마지막 두 오류보다 앞서 실제 보이는 header/checksum 손상을 검출하면 그 원 오류를 보존한다.

차단 이름6개는 `CURRENT`, `d3-current-stage`, `d3-rebase-0`, `d3-rebase-1`, `d3-writer-stage`, `d3-filemap-select-stage`다. 고정 geometry는 다음과 같으며 WAL은 모든 FILEMAP/초기 bootstrap 검산이 끝난 뒤에만 장기 open한다.

| 이름 | exact bytes |
| --- | ---: |
| d3-profile, FILEMAP, d3-bootstrap, DWB, WRITER | 각각4096 |
| d3-data-0000000100000001 |1048576|
| d3-filemap-0, d3-filemap-1 |각각2113536|
| d3-dwb-0, d3-dwb-1, d3-dwb-stage |각각16384|
| d3-dwb-images |16711680|
| d3-dwb-reserved |8192|

이름 geometry 검산은 read 권위를 주지 않는다. `d3-dwb-select-stage`만 optional0…4096이다. 비선택 내용·home·images/reserved/stage 내용은 읽지 않는다. WAL을 retain한 뒤 임시 File은 동시에 최대1개다.

모든 open은 opened metadata를 검산한 뒤 named identity/length를 대조한다. fixed geometry는 opened length 오류를 먼저 돌려준다. source WAL은 읽는 동안 열린 handle을 유지하고 종료 때 namespace identity와 length를 다시 확인한다. named replacement나 length 변화는 거절한다. lease 밖의 악의적 in-place overwrite 방어를 주장하지 않는다.

## F05 — 이 분석 단위의 실제 고정 공간

caller의256KiB workspace를 호출 시작 전 mutable borrow로 독점한다. 초기3page scratch, canonical allocator/Status, 전이 after image, 최대38000 frame을 phase별 disjoint 영역에 배치하며 전체 frame 목록·DB 비례 inventory를 만들지 않는다. 이 공간은 maintenance12MiB에 포함하며 새 flush4MiB/buffer quota를 빌렸다고 주장하지 않는다. 전체 recovery용 disk/headroom/두 REBASE slot/FD 예약은 미완료다.

| workspace 영역 | byte offset | bytes |
| --- | ---: | ---: |
| frame/oversized hash chunk |0|65536|
| canonical allocator |65536|16384|
| prospective allocator |81920|16384|
| canonical Status |98304|16384|
| prospective Status |114688|16384|
| bootstrap/control scratch3page |131072|49152|
| 최신 raw descriptor |180224|1536|
| 나머지 독점 대여 여유 |181760|80384|

source-owned W/File/owner·decoded descriptor/entry·codec/hash 호출의 동시 생존량을 타입별 합으로 계산하고 prelease64KiB를 초과하면 거절한다. caller256KiB와 별개로 합산한다. adapter 내부 backing storage·OS RSS/stack·물리 preallocation 증거와 구별한다. 실제 Linux에서 retained WAL File1, 일시 control File1, writer directory/LOCK2, caller directory1로 FD5 이내를 검산한다.

기존 물리 파일 bytes, 아직 남은 terminal obligation, 이미 쓴 source WAL과 read-only terminal hold를 서로 구분한다. checked 산술로 owned8GiB/protected1GiB/WAL256MiB 경계를 확인하되 공간 부족을 정상 복구 성공으로 세지 않는다. 이번 borrow/검산을 전체 BOOT_RESERVE permit로 이름 붙이지 않는다.

정확한 알려진 closure 회계는 아래와 같다. 임의 orphan를 찾는 전역 disk 사용량·실제 quota 예약·producer flush 뒤 dirty ledger와 동일하다고 주장하지 않는다.

- `B_fixed=22073344`는 실제 initial files22069248과 아직 실현되지 않은 selector-stage credit4096을 포함한다. `R=physical_source_length−4096`, stage 실제 길이 `S=0…4096`이다. `owned_bytes=(B_fixed−4096)+R+S`는 알려진 실제 파일 bytes다. owned 적합 판정에는 `owned_bytes+remaining.wal+(4096−S)`를 사용하여 미실현 credit과 terminal WAL을 한 번만 포함한다.
- source physical length≤WAL256MiB와 `complete_offset+remaining.wal≤WAL256MiB`를 별도로 검산한다. 미채택 partial terminal tail을 두 번째 식에 다시 더하지 않는다. 전체 복구의 old/new epoch 공존·REBASE 회수 증명은 별도 미완료다.
- `P_budget=(remaining.protected−remaining.wal)/16384`는 검증된 known tuple에서만 계산하고 음수·비정수 배수는 오류다. `P_needed=(complete_lsn>0 ? 1+status_initialized :0)+active_inventory_items`, `topup=max(P_needed−P_budget,0)*16384`로 현재 allocator/Status 복구 대상의 부족 credit을 유지한다. protected 관측은 `21024768+R+remaining.protected+topup`이다. Status active의 기존2page credit을 다시 더하지 않는다.
- topup은 pristine0, exclusive/dirty active N의 allocator1page, Status active0, cancel terminal1page, INIT terminal2page다. home를 읽지 않으므로 flush 완료를 추정해 반환하지 않는다. 과거 terminal 주소 union은 진단에만 남기고 quota에 추가하지 않는다. 모든 합·곱은 checked 연산이다.

## F06 — 구현 동반 인수

구체 case ID·selector·budget와 native/compiler/checker 입력은 구현과 함께 고정하고 첫 실행 전에 독립 검토한다. 중간에는 변경 기능의 필수 사례만 실행하며 S5 확대 조합은 별도로 선택한다. 기존 SELECT170·하위 회귀·전체 workspace를 이 단위 이유로 반복하지 않는다. 공유 코드를 변경하면 실제 diff와 직접 관련 사례만 선택한다.

필수 경계는 세 mode, count1/63, 여러 reserve/cancel cycle, active owner와 각 terminal, INIT/cancel 충돌, before-descriptor·owner·budget·inventory/native/redo 오류, complete checksum과 tail, selected DWB/EOF 결합, source identity/length 변화, 모든 새 read/metadata fault, borrow/Drop/oversized concrete type다. count63의1000cycle에서 중복 terminal hold를1GiB로 잘못 charge하지 않는 산술/상태 사례, 최종 LSN보다 오래된 Pending digest, checksum-invalid/valid oversized 완결 frame과 동일 header의 미완결 EOF를 필수에 포함한다. 반복1000번의 실제 file I/O 실행은 S5 확대에 배정하고 중간에는 고정 반복 상태/산술 oracle로 반례를 검사한다.

native는 실제 producer와 별도 PID 분석을 연결하고, home가 달라도 같은 canonical 결과와 home 무독해/전체 파일 불변을 대조한다. 기본21은3profile×(exclusive active/cancel, dirty active/cancel, Status active/cancel/INIT)이다. context10은 allocator/status/both home flush3, dirty cancel flush1(reserve→allocator flush→cancel→allocator flush의 실제2batch), actual Pending flush cut1, retired raw slot 변조1, selector-stage 잔재1, incomplete INIT tail1, checksum 거절1, checksum-valid unknown opcode 거절1이다. case30초/합300초, DB24MiB/보존DB768MiB, actual FD5로 고정한다. compiler 전체60초이며 infrastructure와 semantic probe 분모를 구분한다. 독립 oracle는 production encoder/validator를 정답으로 호출하지 않는다.

S5 전용 PL01은 별도 ignored selector/catalog로 실제 profile1 exclusive count63 reserve/cancel1000회를 실행한다. DB/보존DB128MiB, case/합300초이며 default31의24MiB/300초 예산과 합치지 않는다. 기대 WAL length68196096/LSN2000/high-water1000/Free generation1001/active0/terminal mask `0xfffffffffffffffe`, metadata topup16384를 독립 계산한다. hash로 고정한 cancel63 literal의 pageLSN64·각 free generation과 descriptor work-id56/apply72/previous80/birth88/inventory-change544만 명시 patch하고 hash를 갱신해 최종 page/descriptor 전체 bytes를 대조한다. 마지막 owner consumed68192와 전체 retained68192000을 구분한다. source WAL 전체의 독립 byte 정답·매 중간 상태 정답은 인증하지 않으며 실제 source의 전후 bytes/hash/inode 불변을 검산한다. pristine home·초기 DWB도 그대로 유지해야 한다. 독립 D2가 산술·정본 offset·별도 예산·oracle 범위를 수용했고 중간 실행은 NOT_RUN이다.

공유 semantic scanner의 신규 필수42case는6 exact 함수이며 기존 legacy scanner 시험을 포함하지 않는다. S5 EOF matrix244 중7case는 필수의 exact alias, 추가237이다. [입력](../../crates/engine/tests/fixtures/recovery_prefix/semantic_prefix/manifest.json)의 case 순서·selector·marker와 독립 Python literal244를 고정한다. 필수 각 job30초/합120초, S5 각 job/합1200초는 별도 class budget이며 다른 class의 실행량으로 확대하지 않는다. 전체 EOF matrix는 중간 실행하지 않는다.

## F07 — 담당과 검토

기준 HEAD `412bd7b8477cde8abee86de4c5d18611f9d66385`, 시작 worktree clean. 총괄/계약/공유 scanner·통합 `/root`, 제품/순수 분석·unit `/root/recovery_implementation`, native/compiler/checker `/root/recovery_acceptance`, 읽기 전용 독립 D2/D1 `/root/recovery_design_review`다. 요청 배정은 Sol/high·핵심 독립 Astra/high이며 실제 runtime ID의 별도 검증을 주장하지 않는다.

초기 SELECT와 현재 producer의 원 source별 결과를 보존한다. 이 분석 이후 실제 BOOT_RESERVE·Pending repair·page별 REDO·새 프로세스 repair 재진입을 연결해야 첫3.12.1–4 통합 목표가 완료된다. 전체 DEV3·S5·READY는 미완료다.

`/root/recovery_design_review`가 독립 D2에서 누적 terminal charge 반례·historical Pending hash·oversized frame 우선순위·API/배치의4개 차단을 확인했고 수정 뒤 이 제한 단위를 수용했다. 실제 type/source census, protected 중복 차감 부재, lower cause 보존, 정확 native/compiler/runtime 입력은 구현 후 첫 실행 전에 추가 독립 검토한다. D2 수용은 제품 실행·D1·전체 복구 완료가 아니다.

구현 중 추가 회계 검토에서 selector-stage 실제 bytes와 기존 credit 중복, allocator metadata credit 누락을 바로잡았다. 독립 검토자는 위 actual owned·WAL 두 경계·known budget page-credit topup을 수용했다. 이 식을 source와 독립 oracle에 함께 고정하고 실제 실행을 따로 기록한다.

실제 [선택 실행 증거](../evidence/dev3-recovery-prefix-20261022.md)와 [source별 색인](../evidence/dev3-recovery-prefix-20261022.json)은 신규231의 완결 job223·원 실패 job 내 완료 단언8, 관련 RS6·native31·compiler8+control1·checker15 및 원 FAIL을 구별한다. 제품 core는 r3 뒤 불변이며 최종 r5는 두 fixture 경계만 재검증했다. 별도 독립 검토자가 실제 source/ELF/원본 결과·자원을 대조해 제한 단위 D1 ACCEPTED로 판정했고 통합 담당이 반영 확인했다. 모든 S5와 실제 repair/REDO는 미완료다.

# DEV3 genesis WAL 설치·append/sync

2026-10-21 · **D2 채택·제한된 구현/로컬 검증 완료 / 전체 DEV3 인수 아님**

[WAL 표현](dev3-wal-format.md) · [control](dev3-control-format.md) · [자원](dev3-resource-bounds.md) · [disk 설계](disk-storage-design.md#commit) · [실제 상태](../implementation/dev3-implementation.md#genesis-wal-results)

root가 제안하고 `/root/dev3_selector_review`가 독립 수용한 입력 결합 SHA는 `47c1bcab04b203135e1b59b25a5e06190dddf364c191f24a555c67409e0972ae`다. `artifacts/dev3/wal-append-design-review-20261021/r7-proposal-manifest.json`과 `r7-independent-d2-decision.json`이 원래 입력과 검토를 보존한다. 독립 Python 계산은 literal3·생성8192·slot113/1246·durable28cut/82image를 검산했으며 제품 코드 실행 증거가 아니다. 아래는 r3/r4/r5/r7의 최종 규범을 모은 소유 계약이다.

## 선택과 효과 범위

`GenesisWalSession`은 새 identity의 첫 epoch를 실제 설치한 **동일 live session**의 물리 frame write/sync만 제공한다. 일반 DB 생성 완료·기존 open·restore·recovery·epoch rotation·CURRENT·page 적용·CommitAttempted·ACK·dirty flush capability는 없다. 정상 WAL frame codec의 shape를 소비하지만 opcode 의미를 승인하지 않는다. 기존 public API/DEV2 AXF1 경로는 바꾸지 않는다. 미완성 genesis가 남으며 일반 DB open은 지원하지 않는다.

`create(writer: W, profile_id: u32, slot: &'m mut [u8; MAX_FRAME_BYTES], scratch: &mut [u8; 16384])`는 namespace writer를 소유권으로 소비한다. profile ID1/2/3와 concrete workspace bounds 검사를 먼저 한다. 실제 `list_owned(1)` 결과가 정확 `LOCK` 하나이고 길이0이어야 한다. 빈 inventory·다른 이름·길이·더 많은 파일은 거절한다. LOCK 이외 어떤 기존 파일도 읽어 재사용/수정하지 않는다. `acquire_writer`가 lock을 만들 수 있으므로 이 preflight의 무효과는 LOCK 획득 이후 DB 파일 효과가 없다는 뜻이다.

profile-only, 완성/부분 epoch, staging, WRITER, CURRENT, FILEMAP, DWB, unknown foreign file은 모두 거절한다. `random_incarnation`에서 nonzero DBID를 받아 timeline1/profile generation1/epoch1/selector generation1을 이 fresh 범위에서만 만든다. 임의 DBID·baseLSN·epoch를 caller가 주지 못한다. entropy 실패/변이 전 거절은 LOCK-only를 그대로 두므로 이후 새 create가 가능하다. persistent 파일 하나라도 남은 실패는 새 create의 inventory가 거절하며, 자동 청소·truncate·resume·epoch reuse는 없다.

설치는 다음12개 변이 호출 순서다. 모든 파일은 create_new이고 기존 selector를 교체할 수도 있는 port는 exact fresh preflight+exclusive lease가 보호한다.

1. d3-profile create →4096 write→file sync.
2. d3-wal-00000001 create→4096 epoch header write→file sync.
3. directory sync로 위 이름을 영속화.
4. d3-writer-stage create→4096 selector write→file sync.
5. typed WRITER publish→directory sync.

위 write는 write_all_at을 사용하므로 short/Interrupted 때 세부 호출은 늘 수 있다. selector가 성공 선택되기 전에 append를 허용하지 않는다. profile·epoch·WRITER를 열린 handle metadata/exact length와 readback bytes로 다시 검산하고 epoch의 실제 RW handle/identity를 보관한다. full digest·profile·namespace·canonical name·4096 header 끝을 결합한 private session만 반환한다. metadata는 열린 handle에서 얻고 namespace metadata와 identity/length를 대조한다. W와 남은 file handle은 lease를 유지한다. effect/readback 오류·unwind에는 handle을 닫되 files를 보존하며 성공 session이 없다. readback 실패 뒤 durable selector를 되돌리지 않는다.

## 새 byte 형식

둘 다 exact4096, little endian, 마지막32 bytes=앞4064 bytes SHA256. 별도 magic/version으로 기존 XCS1 role을 암묵 확장하지 않는다. profile의 기존 XDC1 bytes는 유지한다. genesis에서만 timeline/profilegeneration/epoch/selectiongeneration=1, baseLSN=0이다. decoder는 caller가 실제 profile bytes를 검증한 `Profile`을 받고 input에서 기대 profile을 만들지 않는다.

Epoch header `XWE1`:

|offset|bytes|값|
|---|---|---|
|0|4|XWE1|
|4|2|version1|
|6|1|Genesis1|
|7|1|0|
|8|4|4096|
|12|4|flags0|
|16|16|DBID|
|32|8|timeline1|
|40|4|epoch1|
|44|4|0|
|48|8|profile generation1|
|56|32|profile full digest|
|88|8|baseLSN0|
|96|8|first frame offset4096|
|104|4|max frame1048576|
|108|4|0|
|112|8|max WAL file268435456(header 포함)|
|120|3944|0|
|4064|32|checksum|

WRITER `XWS1`:

|offset|bytes|값|
|---|---|---|
|0|4|XWS1|
|4|2|version1|
|6|1|Genesis1|
|7|1|0|
|8|4|4096|
|12|4|flags0|
|16|16|DBID|
|32|8|timeline1|
|40|8|selector generation1|
|48|4|epoch1|
|52|4|0|
|56|8|profile generation1|
|64|32|profile full digest|
|96|32|whole4096 epoch header SHA256|
|128|8|baseLSN0|
|136|8|first frame offset4096|
|144|3920|0|
|4064|32|checksum|

이 body는 genesis stream의 물리 선택만 뜻하며 recovery base/complete checkpoint를 인증하지 않는다. 허용 physical epoch 이름은 정확 `d3-wal-00000001`이다. encoder는 전체 입력/출력 크기를 검사한 뒤 성공 prefix만 쓰며 실패 전체 output을 보존한다. decoder는 exact size/reserved/checksum/상수/profile binding을 모두 검산한다. 새 일반 schema/unknown mode를 opaque 성공으로 읽지 않는다.

## Slot 상태와 잠금

payload1MiB caller memory를 exclusive mutable borrow한다. mutex state는 `Idle`, `Complete(frame interval)`, `IoOwned(frame interval)`, `Fenced`와 slot Option을 갖는다. 실제 file에는 별도 I/O mutex가 있고 W의 namespace 소유도 session에 남긴다. unsafe/Arc/새 thread/새 dependency는 없다. W/File의 auto Send/Sync에 따르며 강제 unsafe impl은 없다.

`try_stage(bytes)`는 gate 아래 Idle과 신뢰 predecessor/offset snapshot을 얻고, gate 밖에서 그 기대값으로 existing decoder의 전체 frame checksum/shape를 검산한 다음, gate를 다시 얻어 Idle·동일 predecessor/offset·strictly increasing LSN·checked physical interval·cap을 검사하고 fixed slot에 bounded copy 후 Complete를 공개한다. 예약만 된 incomplete token은 발급하지 않으므로 첫 구현에는 hole/out-of-order completion이 없다. Complete/IoOwned이면 Busy이고 slot을 덮지 않는다. LSN gap을 허용하며 lsn에 frame byte length를 더하지 않는다.

`sync_pending()`는 gate 아래 Complete와 slot 소유를 take하여 IoOwned로 바꾸고 gate를 놓는다. RAII claim은 이때부터 성공 공개까지 무장한다. 이후 I/O mutex를 얻어 exact offset에 write_all_at→sync를 수행한다. I/O mutex를 놓은 다음 gate에서 성공 prefix와 Idle+slot을 공개하고 claim을 해제한다. 이 사이 오류/unwind/poison은 atomic fence를 즉시 세우고 pending bytes를 재사용하지 않는다. gate와 file mutex를 함께 잡고 I/O하지 않으며 file mutex를 쥔 채 gate를 다시 잡지 않는다. staged/IO 중 prefix와 try_stage는 짧은 gate만 쓰고 I/O 완료를 기다리지 않는다. same-session duplicate sync는 Busy로 거절한다.

`prefix()`는 stream/file identity, complete LSN/offset, confirmed synced LSN/offset을 반환한다. installed initial prefix는 logical0/physical4096이다. 관측은 physical 사실이며 page flush·capture·semantic apply·commit ACK token이 아니다. prefix 숫자만 받아 임의 flush target을 만족시키는 API도 없다. 마지막 LSN MAX는 물리 표현 가능하지만 후속 증가가 불가능해 새 frame을 거절한다.

첫 frame write 시도 뒤 write/sync error, zero/invalid count, prefix-then-error, unwind, poison, sync 성공 후 공개 실패는 fence한다. 같은 writer의 재전송/자동 초기화/continued append는 없다. synced prefix는 file sync 성공과 정상 publication 둘 다 끝난 경우만 전진한다. sync 성공 뒤 fence되더라도 실제 durable bytes가 취소되었다고 응답하지 않는다. 오류는 stage/effect uncertainty와 원래 namespace/I/O category를 보존하며 거래 ABORTED/UNKNOWN을 만들지 않는다. Drop은 I/O/cleanup/commit을 하지 않는다.

## 실제 자원 소유

상주 slot1MiB+session metadata/설치 scratch 합64KiB를 **기존 metadata/cache8MiB의 고정 subbudget**으로 D2 배정한다. 기존 locator1MiB와 별개이며 나머지는5.9375MiB이다. 기존 system16MiB=flush4+maintenance12는 유지한다. 64KiB 안에 scratch16KiB, concrete session state/locks/File/W/작은 locals≤16KiB, bounded list_owned(1) inventory allocation·staging≤16KiB, 여유16KiB를 둔다. 실제 Linux와 deterministic adapter의 sizes/FD 및 설치 peak를 검산한다. slot caller memory borrow와 namespace writer ownership이 이 component의 중복 소유를 막는다. 전체 engine metadata ledger/F05 충족은 후속 의무이며 남은 frame headers/ATT/locks는5.9375MiB 안에서 증명해야 한다.

입력 prepared frame은 caller의 private8MiB 또는 maintenance12MiB의 bytes이며 slot과 별도 물리 memory로 계상한다. writer가 encode용 숨은1MiB 복사를 만들지 않는다. frame 준비/space 대기는 latch/gate 밖이다. genesis에서는 모든 현재 파일이 known이고 동시 다른 namespace owner가 없으므로 control/profile actual bytes+최대 WAL 파일 길이≤268443648 bytes(LOCK0, profile4096, WRITER4096, WAL268435456)를 bounded accounting으로 검산할 수 있다. 설치 staging은 rename으로 WRITER가 되므로 동시에 양쪽을 current physical bytes로 세지 않되 rename uncertainty에는 old/new 합집합을 보호한다. 설치 header/control 고정 몫12288 bytes는 protected reserve1GiB 내부, 이후 WAL growth는 owned와WAL cap을 동시에 검사한다. 별도 preallocation/sparse 용량 확보 주장은 없다. Complete의 pending interval은 outstanding이며 IoOwned 진입에서 같은 interval을 한 번만 attempted/uncertain 소유로 옮긴다. 실제 file length와 보호 상한을 구분하고 오류 뒤 불명 interval을 반환하지 않는다. 아래 상태표를 따른다. 이 genesis-local 회계는 여러 거래·datafile의 global admission 구현이 아니다.

정상 retained handle은 WAL1+W/LOCK capability, 설치 중 최대2개 product file handle(기존 writer lock 제외), caller workspace는 slot1MiB+scratch16KiB이다. fd128 전체 profile을 확보했다고 하지 않는다. 세 profile을 모두 지원하되 buffer frame allocation은 이 component가 하지 않는다.

## 신뢰 predecessor와 동시 stage

`try_stage`는 (1) atomic fence 확인, gate 아래 Idle 여부와 신뢰하는 stream/complete predecessor/physical offset snapshot 확인, (2) gate를 놓고 그 snapshot의 `FrameExpectation`으로 exact `decode_frame`, (3) gate 재획득 뒤 fence·Idle·동일 predecessor와 offset 재검사, (4) quota/구간 재검사 후 copy/publication 순서다. input header에서 expected predecessor를 뽑지 않는다. 최초 Busy는 bytes를 decode하지 않고 Busy를 반환하므로 malformed input도 Busy가 우선이다. decode 후 다른 stage가 진행 중이면 Busy, 그 stage가 sync까지 끝나 다시 Idle이지만 predecessor/offset이 달라졌으면 OrderMismatch다. 순서는 strictly `frame.lsn > previous_lsn`이며 동일 LSN을 허용하지 않는다. snapshot 재검사는 caller/다른 thread가 입력을 바꾸지 않더라도 필요하다.

## 구간을 한 번만 보호하는 회계

C=이전 synced file end, L=pending frame length, E=C+L이다. header4096은 C에 포함한다.

|상태|확정 existing prefix|미시도 outstanding growth|시도 뒤 보호하는 불명 구간|총 보호 WAL bytes|
|---|---|---|---|---|
|Idle|C|0|0|C|
|Complete|C|L|0|E|
|IoOwned 진입~성공 공개 전|C|0|L|E|
|성공 Idle|E|0|0|E|
|시도 중 오류/Fenced|C|0|L|E|

Complete→IoOwned에서 L 전체를 outstanding에서 attempted/uncertain interval로 **이동**하며 새로 더하지 않는다. 따라서 C=cap−192, L=192 뒤96-byte short write여도 cap을 두 번 차감하지 않는다. 실제 file length는 C..E 범위이고, 이 API가 오류 후 그 exact값을 알았다고 하지 않는다. 보호상한 E를 실제 관측한 length와 구분한다. 확인한96bytes를 L 위에 다시 더하지 않고 unknown suffix도 반환하지 않는다. file sync 성공 후 publication 실패는 실제 durable E와 노출된 synced C가 다를 수 있으며 fence·E보호를 유지한다. before-effect rejection은 C와 slot을 유지한다. 실제 파일/예약 ownership의 후속 recovery reconciliation은 이 session이 하지 않는다.

## API·오류

- `GenesisWalSession::create(writer: W, profile_id: u32, slot: &'m mut [u8; 1048576], scratch: &mut [u8; 16384]) -> Result<GenesisWalSession<'m,W>, WalIoError>`
- `try_stage(&self, bytes: &[u8]) -> Result<WalFrameLocation, WalIoError>`
- `sync_pending(&self) -> Result<WalSyncObservation, WalIoError>`
- `prefix(&self) -> Result<WalPrefix, WalIoError>`
- `is_fenced(&self) -> bool`는 poison mutex를 얻지 않는 즉시 상태 관측이다.

`WalFrameLocation`은 actual stream/file identity, predecessor/end LSN, start/length/end offset을 담는다. `WalPrefix`는 actual stream/file identity, complete LSN/offset, confirmed synced LSN/offset을 담는다. `WalSyncObservation`은 같은 frame location과 확인한 physical synced prefix만 담는다. 복제 가능한 관측값이며 page/capture/commit 권한이 아니다. constructor는 private producer만 호출하고 임의 file+offset 생성자는 없다. W는 내부 mutex에 소유하여 W:Send와 File:Send일 때 safe auto Sync를 얻으며 W/File의 강제 unsafe trait 구현은 하지 않는다.

Idle의 `sync_pending`은 NoPending·I/O0, Complete 중 추가 stage는 Busy, IoOwned 중 stage/sync는 Busy다. healthy prefix는 항상 짧게 성공한다. fenced 상태에서 stage/sync/prefix는 Fenced이고 I/O0; is_fenced만 true를 반환한다. error와 unwind 뒤 synced 사실을 외부에서 읽어 commit으로 오용할 성공 API를 남기지 않는다. fault 시험은 test-only 상태 관측으로 이전 watermark와 보호 구간을 확인한다.

`WalIoError`는 고정 phase, 누적 DB 변이 경계(`Unchanged` 또는 `CallerMustResolve`), 원래 cause를 분리한다. cause는 InvalidInput, Busy, NoPending, Fenced, Control(ControlError), Frame(WalError), Namespace(NamespaceError), Io(foundation::Error), IdentityMismatch, LengthMismatch, Capacity, ArithmeticOverflow, Poisoned다. 전역 거래 certainty를 만들지 않는다. 설치 preflight/entropy/encode 오류는 DB 변이 Unchanged이고, 첫 create 호출 직전부터 설치 결과 오류는 보수적 CallerMustResolve다. 마지막 readback cause가 Unchanged여도 이미 설치한 효과는 CallerMustResolve다. 원래 lower cause는 그대로 보존한다. stage의 사전 거절은 Unchanged이고 첫 frame write 호출 이후 오류는 CallerMustResolve+fence다. gate/file poison은 fence하며 이미 공개된 pending interval과 이전 효과를 없애지 않는다.

## 고정 시험과 독립 입력

engine runtime10개, native3개와 별도 compiler4·유한 모델을 구분한다. 제품 실행 전에 아래 분모·입력·checker를 보존한다.

1. `genesis_codec_literals_and_damage`: Python offset 표 literal2개, prefix0..4096 각각, 모든 byte1bit×2=8192손상, digest 재계산 semantic field26개와 reserved rehash7886개, profile mismatch/oversize/encoder unchanged sentinel.
2. `genesis_requires_exact_empty_namespace`: 정상LOCK0, 없음/LOCK길이1, foreign8이름(profile/epoch/stage/WRITER/CURRENT/FILEMAP/DWB/other), entropy zero/error, profile0/4; 거절 전후 원본 bytes·mutation0.
3. `genesis_installs_in_durable_order`: profiles1/2/3, exact12mutations+postreadback; actual profile→header→WRITER digest/reference, retained fileidentity/offset/handles, other namespace lease blocked.
4. `genesis_install_faults_preserve_partial_namespace`: 12mutation call 각각 before/after error24셀, 각call unwind12셀, readback profile/epoch/selector open/metadata/read9셀, wrong opened identity/length4셀. 새 create는 persistent residue가 있으면 mutation0거절, LOCK-only pre-effect failure이면 새 entropy로 재시작 가능.
5. `wal_slot_writes_exact_logical_and_physical_prefixes`: 서로 다른 lengths195/224/192, LSN1→7→20, Complete/Sync prefixes, byte-for-byte slot/파일, exact predecessor, duplicate/stale stream/frame 거절; 성공 후 재사용.
6. `wal_slot_rejects_boundaries_before_effect`: stream mismatch3/prev1, malformed/trailing/truncated/checksum4, sizeMAX/MAX+1, physicalcap−1/exact/+1와 checkedoverflow, LSNMAX후속; 모든 거절 slot/prefix/파일 불변.
7. `wal_io_faults_fence_without_replay`: short/Interrupted 정상 진행, write0/invalidcount/error/prefixerror4, syncbefore/after2, write/sync unwind2, poisoned gate/file2; 원래 syncedprefix 유지, 재진입모든operation I/O0. actual successful sync 뒤 publication poison도 fence.
8. `wal_io_releases_gate_and_protects_slot`: 실제 paused file write 동안 별도 thread prefix/try_stage/second sync가5초 안에 끝남; IoOwned slot 변경0, release 뒤 정상 sync. paused unwind 즉시 fence와 이전 syncedprefix.
9. `wal_generated_prefixes_match_independent_bytes`: 64seed×128op, stage/sync/Busy/rejection, generator exactcensus와 독립 SHA/physical concatenation/logical predecessors, finaldrain; 큰frame경계 별도시험.
10. `wal_workspace_and_handles_remain_bounded`: concreteLinux/fake metadata≤16KiB, inventory≤1·FDpeak, 준비input+slot 분리계상; borrowedslot 세션중 재사용불가 compilerprobe, 실패drop에mutation0.

### Codec의 독립 literal·오류 표

r3-literals.py는 기존 독립 fixture `dev3-ownership-profile1.bin` SHA `fff74c31eeef815940cbc0e0d6fb42810d6222a657efe95982137b410cddaf31`의 timeline/generation만1로 바꾸고 표준 hashlib로 checksum을 다시 계산한다. 생산 codec을 호출하지 않는다. DB bytes1..16, ID1은 유지한다. 최종 profile/epoch/WRITER SHA는 r3-literals.json에 고정했다. epoch/WRITER 각각 prefixes0..4096=4097개(완전1/부족4096),1byte trailing1, byte별 xor1손상4096개다.

semantic rehash 표는 **26개**를 고정한다. 아래 각 입력은 한 필드만 바꾸고 마지막 checksum을 다시 계산한다.

|각 형식의 번호|Epoch 13개|WRITER 13개|
|---|---|---|
|1|magic 첫 byte xor1: Corrupt|동일: Corrupt|
|2|version2: UnsupportedFormat|동일|
|3|mode2: UnsupportedFormat|동일|
|4|length4095: Corrupt|동일|
|5|DB 첫 byte xor1: IdentityMismatch|동일|
|6|timeline2: IdentityMismatch|동일|
|7|epoch2: IdentityMismatch|selector generation2: IdentityMismatch|
|8|profile generation2: IdentityMismatch|epoch2: IdentityMismatch|
|9|profile digest 첫 byte xor1: IdentityMismatch|profile generation2: IdentityMismatch|
|10|baseLSN1: Corrupt|profile digest 첫 byte xor1: IdentityMismatch|
|11|first offset4095: Corrupt|epoch header digest 첫 byte xor1: IdentityMismatch|
|12|max frame1048575: Corrupt|baseLSN1: Corrupt|
|13|WAL cap268435455: Corrupt|first offset4095: Corrupt|

reserved/flags는 각 zero byte를 하나씩1로 바꾸고 rehash한다. Epoch ranges[7,8),[12,16),[44,48),[108,112),[120,4064)=3957개, WRITER[7,8),[12,16),[52,56),[144,4064)=3929개, 합7886개가 모두 Corrupt다. checksum-only bitflip8192개와 구별한다. literal과 실제 expected Profile을 별도로 바꾸는 DB/timeline/generation/full digest mismatch도 원인별 검사한다. encoder의 unsupported expected profile(timeline/generation≠1), output4095와 invalid profile은 실패 output sentinel 보존; 성공4096prefix 뒤 sentinel을 보존한다.

### Runtime 경계·생성·모델

cap 경계는 source 하드코딩 cap 변경이나 낮은 임의 profile로 통과시키지 않는다. test-only private fixture가 session의 **정상 Idle** C를 cap−193/cap−192/cap−191로 만들고, fake sparse file length와 synced/complete offset·LSN을 동일하게 맞춘다. 실제 이전 WAL chain 전체의 복구 증거가 아니라 checked arithmetic/interval primitive의 typed boundary 시험임을 명시한다. 192-byte 유효 System frame을 stage하면 cap−1/동일은 수락, cap+1은 Capacity before-effect다. start i64MAX−191/−192/u64MAX−191의 checked/signed overflow는 별도 pure interval helper 시험이며 지원256MiB 파일의 실제 도달상태라고 하지 않는다. 실제 Linux cap-throughput 인수는 이번 경계 시험의 주장 밖이다.

생성 공식은 r4-generated-census.py다. seed0..63, state=seed+1, 매step `state=(6364136223846793005*state+1442695040888963407) mod 2^64`, op=state>>61. op0..3은 stage,4..5 sync,6 wrong epoch frame,7 prefix. Idle stage의 endLSN=현재LSN+1+((state>>8)&7), payload length=(state>>16)&255, System/Opaque, payload byte i=(i+seed+step)mod256. frame192+payload bytes와 checksum을 독립 offset helper로 준비하며 production encoder로 기대값을 만들지 않는다. Busy/Idle 실패 구분은 위 규칙과 같다. seed 끝 pending은 별도 final drain한다.

총8192ops: stage성공1378, stageBusy2718, sync성공1336, NoPending727, invalidBusy705, invalidIdentity325, prefix1003; finaldrain42로 stage1378개를 모두 sync한다. staged bytes443779, max final LSN135, max final file offset15315다. 실행은 count뿐 아니라 각 exact bytes/hash/offset/LSN과 pending보호를 확인한다. 큰1MiB frame은 별도fixedcase다.

r3-slot-model.py는2actor×7actions(stage,take_io,write,sync,publish,fault,observe), 최대8step의 모든 이력을 exact `(State,depth)`로 합쳐 순회한다. state는mode/complete/synced/durable/owner/IOphase/gate이며 depth8 이후의 무한공간 증명은 아니다. census113states-with-depth/1246transitions; operation각178, modeidle13/complete22/io42/fenced36. publish_before_sync/reuse_io_slot/clear_fence/io_under_gate mutant4개가각 최소반례로 거절된다. 숫자gap/실제bytes/overflow/resource풀모델이 아니며 runtime의 독립 byte/산술 시험이 별도로 남는다.

### Native cut·compiler

process kill은 아래8지점의 **after hook**을3회씩 freshfixture에서 실행한다: profile_file_sync, epoch_file_sync, initial_directory_sync, writer_stage_file_sync, writer_publish, selected_directory_sync, first_frame_write, first_frame_sync. hook은 해당 effect를 수행한 실제 worker만 발행하고 parent가 그 exact hook을 확인한 뒤SIGKILL/waitpid한다. 첫6개는 설치도중, 마지막2개는 선택성공 뒤 첫192byte System frame의 I/O다. hook중복/미도달/timeout은FAIL이다. crash 후 retained files와 새 genesis거절을 검사하고 bytes를자동수정하지 않는다. kill은pagecache관측이며 power-loss durability증거로세지않는다.

compiler는unmodifiedbaseline1, positive1(prepareframebuffer가stage반환뒤해제되어도session유지가능), negative2(살아있는session동안slot다시mutableborrow 및 같은slot으로두session생성)의exactE0499/primaryprobe span을검사한다. private접근/미해결importerror는거절증거가아니다. 실제compile은독립sourceexport로수행하고이단위의engine runtime10개와native3개와별도로센다. 최종고정runner세부는D2검토후등록한다.

### 실제 snapshot 경쟁·durable 이미지

`wal_io_releases_gate_and_protects_slot`에 실제 thread의 stale snapshot schedule2개를 추가한다. 시험 전용 instance-owned pause는 A가 gate에서 Idle/expected stream/predecessor/offset을 캡처하고 gate를 놓은 직후, 실제 decoder 진입 전에 정지시킨다. global hook/제품 사용자 callback은 없다. B가 같은 기대 predecessor의 frame을 (1) stage만 끝내면 A 재개는 Busy, (2) stage와 sync를 끝내 Idle로 돌아왔으면 A 재개는 OrderMismatch다. 두 경우 A의 bytes가 slot/파일/complete/synced/보호구간을 변경하지 않으며 독립 snapshot과 대조한다. pause도 gate를 보유하지 않는지 B의5초 완료로 확인한다. r3-slot-model의 원자 stage가 이 실제 prepare/publish 경쟁을 검사했다고 확대하지 않는다.

별도 bounded durable-image corpus는 r5-durable-census.py/json으로 고정했다. 12installation+2frame effect의 before/after28cut에서 총82image다. initial directory sync 전 새 profile/epoch 이름은 각각 유실/존재를 독립 조합한다. file sync 전 bytes는 이전 durable/중간prefix/전체written의 세 후보를 사용한다. 이 세 후보는 실제 장치의 모든 가능한 손상 패턴을 열거했다는 뜻이 아니다. initial directory sync 성공 뒤 profile/header 이름과 각4096 bytes는 고정이다.

writer stage는 initial directory sync 뒤 생성되므로, publish 후 selected directory sync 전에는 **이름 없음 / d3-writer-stage / WRITER** 세 후보를 둔다. stage의 sync된 contents는 selector와 정확히 일치한다. confirmed selected directory sync 뒤에는 WRITER만 남고 profile/header도 유지된다. 첫192byte frame의 unsynced append는 epoch 길이4096/4192/4288, sync 뒤에는4288이다. 각image는 파일 이름/길이/SHA·fresh_install_eligible(LOCK-only)·selected_genesis를 독립 기입한다. 어느image도 DB READY를 인증하지 않는다. 소스에 연결할 실제 durable-shadow port의 관측 결과는 이 고정image와 대조하고 미도달/추가효과/분모누락은FAIL이다.

phase별image수는 createP전후1/2, writeP2/4, syncP4/2, createE2/4, writeE4/8, syncE8/4, initialDir4/1, createStage1/2, writeStage2/4, syncStage4/2, publish2/3, selectedDir3/1, frameWrite1/3, frameSync3/1이다. 합82이며 cut별 중복image는 서로 다른 관측시점의시험으로유지한다. 동일cut 안의이름없는object의무관한content후보만중복제거한다.

이 corpus는 bounded power-loss 모델이며 실제 ext4 API fault24셀·process kill8종×3회의 pagecache 관측과 분리한다. 실제 장치의 모든 장애를 증명하거나 full DEV3 recovery로 세지 않는다.

### Namespace·오류 variant·payload 지원

1. 생성 64×128 byte oracle의 namespace는 DBID bytes `01 02 … 10`(16진수), timeline1, writer epoch1이다. genesis profile ID1/generation1/selector generation1이다. deterministic port의 entropy가 이 DBID를 반환한다. r3 literal과 r4 생성 corpus는 같은 namespace를 사용한다. native 직접 Linux entropy 사례는 별도로 실제 profile bytes에서 namespace 결합을 독립 판독하며 deterministic corpus와 섞지 않는다.
2. stale snapshot 후 B가 stage+sync를 끝낸 상태에서 A의 재검사 실패는 `WalIoError`의 cause `Frame(WalError::OrderMismatch)`다. 독립 OrderMismatch variant를 추가하지 않는다. B의 Complete/IoOwned 상태이면 cause Busy다. 이 실패는 A가 DB/file/slot에 effect를 시도하지 않은 `Unchanged`이며 B의 정상 선행효과를 취소하지 않는다.
3. 기존 `wal_frame::decode_frame`의 Legacy payload mode를 사용한다. payload tag0(Opaque)/1(PageRedo)의 byte shape만 지원하며 tag2(SemanticGroup)는 `Frame(WalError::UnsupportedFormat)`으로 거절한다. 현재 RollbackStart의 semantic-group 관측 구현은 이 writer에 연결하지 않는다. tag2를 opaque로 재해석하거나 semantic operation의 성공으로 인정하지 않는다. boundary test에는 expected stream/predecessor가 맞는 frame의 tag만2로 바꾸고 header/trailer checksum을 맞춘 before-effect rejection을 넣는다. 향후 semantic apply/commit/rollback producer를 연결할 때 해당 decoder와 효과 계약을 별도 검토해야 한다.


## 구현·검증 연결

구현은 storage/wal_io와 시험 지원·native test façade가 소유하며 engine은 foundation ports만 소비한다. 새 crate·dependency·public API·DEV2 형식을 추가하지 않는다. 독립 D1 source/corpus 검토와 같은 final source의 runtime/native/compiler/model·Clippy/fmt·leaf/하위회귀·DEV2 현재-build 복구를 완료해야 이 한정 단위를 로컬 검증 완료로 표시한다. whole DEV3 READY·정상 reopen·semantic apply/commit·global F05·복구는 별도 미완료 의무다.


## 구현·실행 기록

제품은 `storage/wal_io.rs`, 시험 지원은 `wal_io_test_support.rs`와 `wal_io_tests.rs`, feature-gated native 조립은 `testing/genesis_wal.rs`에 있다. 실제 Linux 시험3개는 `crates/testing/engine-worker/tests/genesis_wal.rs`, compiler4는 `scripts/check_dev3_wal_borrows.py`가 소유한다. 공유 compiler 진단 parser의 기본 buffer probe 의미는 유지하고 WAL의 명시적 probe 경로에도 exact code/primary span을 적용했다. checker 자기시험7개와 WAL 모델 checker3개는 제품 runtime 분모와 분리한다.

`genesis_installs_in_durable_order`의 실제 FileIo/WriterNamespace 14개 effect와 raw write bytes를 `scripts/dev3_wal_durable_model.py`가 D2 때 고정한 `tests/fixtures/dev3-genesis-wal-durable.json`과 대조한다. slot113/1246·mutant4는 별도 `scripts/dev3_wal_slot_model.py`의 추상 유한 계산이다. 실제 trace와의 연결도 28cut/82image의 한정 장애 모델이며 장치 전체 power-loss 증명·recovery 실행으로 확대하지 않는다. native24개 SIGKILL은 별도 pagecache 관측이다.

최종 source3162fc43…의 실행·독립 검토·남은 의무는 [구현 기록](../implementation/dev3-implementation.md#genesis-wal-results)을 따른다. raw receipt의 원래 source·실패·실행 분모는 보존한다.

## 최초 저장 세션의 후속 D2 계약

후속 [최초 저장 세션](dev3-storage-session.md)은 이 genesis owner를 내부에 유지하고 설치된 mapping에 대한 typed WAL 참조 검사·공통 fence를 결합한다. D2 채택이며 현재 구현·실행으로 승격하지 않는다. 기존 standalone genesis의12 mutation/오류/slot 의미와 그 실행 이력은 유지한다.

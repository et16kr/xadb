# DEV3 최초 doublewrite 설치 계약

2026-10-21 · **D3-DWB-INSTALL-001 / D2 ACCEPTED / 최초 설치 구현·제한 검증 완료; 전체 DEV3 미완료**

[저장 세션](dev3-storage-session.md) · [DWB 표현](dev3-doublewrite-format.md) · [control](dev3-control-format.md) · [자원 회계](dev3-resource-bounds.md) · [독립 D2 결정](../evidence/dev3-doublewrite-install-d2-xadb-20261021-r9.json)

계약·구현·통합 담당은 `/root`, 독립 검토자는 `/root/dev3_selector_review`다. 기존 사용자 위임과 DP01의 D2 독립 검토에 따라 r6 제안과 r8 보충을 함께 채택한다. dirty page·복구·전체 DEV3 인수와 DEV2 PF04 해결은 포함하지 않는다. 문서 채택은 제품 실행 증거가 아니다.

원본은 `artifacts/dev3/dirty-storage-preparation-20261021/` 및 `artifacts/dev3/dirty-storage-independent-review-20261021/`에 보존한다. [r6 입력 manifest](../evidence/dev3-doublewrite-install-inputs-xadb-20261021-r6.json)와 [r8 보충 manifest](../evidence/dev3-doublewrite-install-inputs-xadb-20261021-r8.json)는 원본과 byte 단위로 같다. r6의 PROPOSED 상태는 준비 당시 identity로 보존하며 현재 채택 상태는 이 문서와 r9 결정이 소유한다. r7의 세 정확화 요청은 r8과 r9에서 닫혔다.

아래 규범은 원본 r6 본문·고정 검증과 r8 보충이다. logical counter, short I/O, 정확 오류, metadata/FD 범위는 마지막 r8 보충이 모호한 r6 문구를 구체화한다. format/geometry/effect/분모를 바꾸지 않는다. 새 실제 source 경로는 구현과 함께만 등록한다.

## 범위·선택과 선행

기존 FreshStorageSession을 소비하고 canonical DWB 파일·최초 Empty selector를 실제 설치한다. 원래 writer/WAL/공통 fence의 단독 소유를 유지하는 DoublewriteStorageSession을 반환한다. 전환 뒤 clean reader를 제공하지만 raw try_stage/sync_pending, mutable home/Dirty/DPT, batch 발급/Pending/retirement, 일반 reopen/resume, CURRENT/READY는 제공하지 않는다. 이것은 S2의 필요한 실제 I/O 단위이며 전체 S2/S3/DEV3 인수를 대체하지 않는다.

선택은 기존 wal_io/bootstrap의 private child `doublewrite_install`에서 구현하는 consuming initializer다. 기존 Fresh의 불변 FILEMAP·home·borrowed reader 의미를 그대로 위임하고 raw Genesis/WriterNamespace를 노출하지 않는다. public API/crate/dependency/unsafe/storage-mode flag를 추가하지 않는다. source/test 경로는 실제 구현과 함께만 저장소 배치에 등록한다.

대안인 별도 cloned W는 writer authority를 분할한다. Fresh에서 optional DWB flag를 켜면서 raw WAL API를 계속 노출하는 안은 WAL과 이후 live page의 불일치를 허용하므로 제외한다. 별도4096-byte descriptor 추가는16MiB 정본을 초과하고 reserved8KiB를 자동 차용할 수 없으므로 제외한다. d3-dwb-stage의 설치 전용 phase를 아래에 명시하는 안을 선택한다. 후속 dirty session의 frame/DPT 수명·batch high-water·실제 allocator/INIT/owner/F05·FIFO progress는 별도 의무다.

독립 사전 검토 r2/r4와 root 반영 r3/r5를 보존한다. 공개 소스는 r0-source-research.json/r2-public-source-audit.json의 실제 PostgreSQL45fc51fc…, MariaDB327437ad…, CUBRIDa1c3506c… source 읽기만이며 제품 실행 증거가 아니다. 기존 canonical 형식/작업 순서/회계는 dev3-control-format, dev3-doublewrite-format, disk-storage-design, implementation-contracts가 소유한다. 개발 형식에는 새 fixture를 사용한다.

## API·상태 preflight·오류

API는 `FreshStorageSession::install_doublewrite<'w>(self, workspace: &'w mut [u8;4194304]) -> Result<DoublewriteStorageSession<'m,'w,W>, DoublewriteInstallError>`다. 반환 owner는 Fresh와 workspace를 소유하며 `open_reader`와 `is_fenced`만 제공한다. 기존 Fresh API는 변경하지 않는다. 반환 owner의 reader는 기존 session/root/scratch 수명과 공통 fence를 그대로 빌린다. workspace는 owner가 살아 있는 동안 재사용할 수 없다. Drop은 파일 handle/borrow만 놓고 I/O·unlink·flush·allocation을 하지 않는다.

StatePreflight는 I/O0이다. concrete 반환 owner 크기≤16384, plan≤4096, 새 scalar/names/codec reserve≤16384의 구현상 상한을 검사한다. 실제 Fresh의 내부 profile/namespace, reader_active=false, common fence=false, writer/file/append mutex의 poison 없음, slot Some, state Idle, complete/synced logical LSN0, complete/synced physical offset4096, stream epoch1과 actual profile namespace를 검산한다. 기존 WAL file identity/prefix를 초기화하거나 reset하지 않는다. Fresh의 move는 이 검사와 stage 사이의 동시 접근을 막지만 `mem::forget(reader)`의 active bit를 지우지 않는다.

NamespacePreflight는 별도 read-only phase다. DWB, d3-dwb-stage, d3-dwb-images, d3-dwb-0, d3-dwb-1, d3-dwb-reserved, d3-dwb-select-stage 순서로 metadata7개가 모두 None임을 검사한다. 실제 Fresh의 closed namespace provenance와 cooperative writer exclusion을 유지한다. 존재하면 내용에 관계없이 AlreadyExists/Residue, metadata 오류면 원래 lower error로 거절한다. 외부 비협력 변경에 대한 원자적 부재 보장을 주장하지 않는다. 이후 각 create_new도 기존 이름을 대체하지 않으며 selector는 이 첫 설치에서만 publish한다.

입력 검산·names·plan·codec·hash·resource 계산은 첫 mutation 전에 완료한다. preflight 또는 이후 실패 모두 Fresh를 반환하지 않는다. top effect=CallerMustResolve는 이미 소비한 Fresh artifact와 설치 잔여물의 해결 의무이며 새 DB write의 주장과 구별한다. 오류는 phase, 원래 typed cause/그 effect, attempted/completed logical mutation counts를 가진다. attempted는 해당 port 시도 전 증가, completed는 성공 반환 뒤 증가한다. after-effect 오류의 actual bytes를 completed API count로 추정하지 않는다. preflight 오류는0/0이며 read-only 실패도 counters를 올리지 않는다. 실패와 unwind는 공통 fence를 세운다. 모든 파일을 보존하고 remove/resume/retry success/READY를 만들지 않는다. caller가 catch_unwind해도 owner는 반환되지 않는다. leaked reader의 handle까지 회수했다고 주장하지 않는다.

## Canonical geometry와 자원 상한

| 파일/몫 | bytes |
| --- | ---: |
|d3-dwb-images|16,711,680 =1,010×16,544 +비대여 tail2,240|
|d3-dwb-0 /1 /stage|각16,384, 합49,152|
|DWB / d3-dwb-select-stage 교체 peak|8,192|
|d3-dwb-reserved|8,192 비대여|
|canonical 예약 총량|16,777,216|

최초 설치는 old DWB가 없어 staging/current가 같은 inode이므로 최종 실제 새 file bytes16,773,120이다. 나머지4096은 다음 selector 교체의 예약이며 이 최초 단위가 그4KiB도 실제 파일로 선할당했다는 주장은 하지 않는다. 기존 Fresh fixed5,296,128/max273,727,488/protected4,247,552에 예약 총량을 한 번 더한 고정 상한은22,073,344/290,504,704/21,024,768이다. WAL cap256MiB는 한 번만 계산한다. 이는 전체 transaction/admission F05 증명이 아니라 이 bounded 설치의 기존8GiB/1GiB 한도 내 효과표다. 첫 실파일 총량은22,069,248이다. 이미지/controls/reserved는 set_len이 아니라 실제 write+sync하며 압축/thin storage/외부 ENOSPC 면제를 주장하지 않는다.

caller WAL1MiB와 설치 workspace4MiB는 별도 살아 있다. workspace는 system flush4MiB 몫에 한 번 charge한다. owner≤16KiB, 별도 plan≤4KiB, 추가 File≤16KiB와 names/scalars/codec/SHA의 conservative16KiB를 합친 transient allowance64KiB는 metadata8MiB 몫이다. phase별 plan/controls/stream/readback은 workspace의 같은 bytes를 순차 재사용한다. 준비 단계의160-byte Empty,4096-byte selector,16384-byte descriptor는 그 workspace 안에 둔다. 설치 descriptor write 뒤 pool zero-stream 때 덮을 수 있고 내부 immutable plan에서 뒤 controls를 재생성하며 prepared digest 동일성을 사용 전에 재검산한다. 새 caller 입력/할당은 받지 않는다. 재생성 불일치는 fence 오류다.

정상 설치/검산 중 WAL1file+현재 생성/검산 file1개만 live하며, 닫은 file 뒤 다음 것을 연다. writer namespace의 directory/lock2FD까지 이 경로의 Linux peak4FD, 반환 뒤3FD다. reader를 발급하면 기존 reader의 최대추가2file과 root/scratch/load48KiB, caller frames의 기존 소유를 추가 계상한다. workspace4MiB는 reader 때도 계속 보유한다. 기존 reader는 plain reader 또는 bounded ReadBuffer를 사용할 뿐 dirty 권한이 없다. 실제 concrete Rust sizes와 live handle을 D1에서 측정하여 위 상한을 확인한다. source-level ownership 회계이며 compiler stack/RSS·전체 global F05 인증은 아니다. allocator/container/OS backing storage를 size_of만으로 측정했다고 하지 않는다.

## 설치 전용 XDI1

이미 예약된 d3-dwb-stage16KiB를 설치 전용 descriptor로 사용한다. descriptor는 생성/write/sync/dirsync를 마친 뒤에만 다른 DWB file을 생성한다. incomplete descriptor도 보존하고 resume 권한은 없다. 성공 뒤에도 이 단위에서 immutable하게 보존한다. 후속 flush owner가 XDW1 교체 staging으로 사용하는 조건은 별도 D2이며 지금 제공하지 않는다. 기존 immutable d3-bootstrap은 변경하지 않는다.

XDI1은 installer-private encode/readback에만 쓰이며 generic selected DWB decoder는 계속 XDW1만 수락한다. 이름으로 XDI1을 검색하거나 fallback/READY capability로 채택하지 않는다. 아래 exact bytes와 전체 digest는 실제 immutable Profile과 Fresh FILEMAP ref로 구성한다. readback의 XDI1 검사는 prepared full digest와 exact length를 대조하며 임의 외부 descriptor를 parse해 authority를 만드는 API는 없다. 나머지 fields/padding은 zero, 모든 정수 little endian이다.

| offset | bytes | value |
| --- | ---: | --- |
|0|4|XDI1|
|4|2|version1|
|6|1|mode1 fresh-only|
|8|4|16384|
|16|16|actual DatabaseId|
|32|8|timeline1|
|40|8|profile generation1|
|48|32|full actual profile SHA256|
|80|4|WAL epoch1|
|88/96|각8|complete/synced physical4096|
|104/112|각8|complete/synced logical0|
|120|8|FILEMAP selector generation1|
|128|64|exact existing FILEMAP root ControlRef|
|192|8|DWB selector generation1|
|200|64|initial Empty bank0 ControlRef|
|264|8|DWB reserved16777216|
|272|8|images file16711680|
|280|8|slot logical16709440|
|288|8|nonloan tail2240|
|296|8|each control/stage16384|
|304|8|nonloan reserved8192|
|312|8|selector pair8192|
|320|8|workspace4194304|
|328|8|first realized new file bytes16773120|
|336/344/352|각8|combined fixed22073344/max290504704/protected21024768|
|360|8|plan version1|
|16352|32|SHA256 of first16352 bytes|

initial Empty는 기존 XDW1 logical160, namespace actual, generation1, batch0, count0/maxLSN0이다. bank0 앞160 뒤 zero padding. bank1 전체 zero는 reserved workspace이며 fallback control이 아니다. DWB는 기존 XCS1 roleDoublewrite, selected generation1, bank0 ref(kind4/slot0/index0/generation1/length160/digestEmpty)와 actual profile을 결합한다. 별도 batch/high-water를 발급하지 않는다.

## 고정 효과와 읽기 관측

r6-inputs/effects.json과 observations.json이 실행 전 순서의 기계 판독 입력이다. 번호는 이번 전환 시작에서 다시1이며 이미 끝난 Fresh350 effects를 중복 통합하여 새 DWB fault 분모로 세지 않는다. before/after 기존 파일 hash와 pristine 검산은 그 실제 Fresh 선행을 연결한다.

| mutation index | operation |
| --- | --- |
|1–4|stage create/write16384/sync/dirsync|
|5|images create|
|6–9|images writes: offsets0/4194304/8388608/12582912, lengths4194304/4194304/4194304/4128768|
|10|images sync|
|11–13|bank0 create/write16384/sync|
|14–16|bank1 create/write16384/sync|
|17–19|reserved create/write8192/sync|
|20|files directory sync|
|21–23|selector-stage create/write4096/sync|
|24–25|publish DWB /directory sync|

정상25=create6+write9+file-sync6+directory-sync3+publish1. 모든 partial I/O는 기존 foundation write_all_at/read_exact_at 규칙을 따른다. Interrupted retry, valid short advance, zero/invalid count/error 거절은 추가 실제 subcall witness로 확인한다. publish 후 directory sync 실패는 설치 성공이 아니다.

NamespacePreflight7 관측 뒤 readback25 관측을 수행한다. readback 순서는 profile, stage, DWB, bank0, bank1, reserved에 대해 각 open_existing_with_metadata→named metadata→read이며, images는 open→named metadata→네 chunk read, 마지막 selector-stage metadata None이다. 모두 ReadOnly로 열고 열린 handle length와 named identity/length 일치를 검산한다. Linux adapter의 regular/no-link 규약과 실제 native device/inode/nlink witness를 구별한다.

profile full digest=내부 Profile.digest를 확인한다. stage full digest=prepared descriptor digest, DWB는 기존 selector decode 후 exact prepared SelectedControl, bank0는 selected XDW1 exact160 decode·Empty batch0/count0/maxLSN0와 tail zero, bank1/reserved/images는 모든 exact bytes zero를 검사한다. images의2,240 tail까지 읽는다. selected reference와 expectation을 input 자신에서 복사하지 않는다. 잘린/긴 file·잘못된 bytes·identity·늦은 selector-stage 잔여물은 오류이며 성공 owner를 반환하지 않는다. 원래 WAL/WRITER/FILEMAP/data는 이 설치가 쓰지 않으며 실제 native before/after manifest로 확인한다.

## 재검토·통합 경계

아래 동결 검증 입력과 함께 독립 D2 수용 뒤 구현한다. geometry/format/API/expected bytes/effect/readback 순서/분모/time limit를 바꿔야 하면 구현 결과에 맞춰 silently 수정하지 않고 새 revision과 독립 검토를 남긴다. reference source/형식/예산·stage 재사용·dirty consumer가 달라질 때 재검토한다. 신규 실제 paths 등록, owner 예외, stage leaf status/evidence는 root가 좁게 반영한다. 문서 verifier는 편집/commit에서 실행하지 않고 pre-push gate를 유지한다.

# 고정 V2 검증

r6-inputs.py는 Python struct/hashlib 및 보존한 이전 독립 input generator를 읽어 profile1/2/3의 XDI1·Empty·DWB·ControlRef12개 literal,25effects,32observations,50cuts/308images/max32의 bounded prefix-loss corpus를 만들었다. 기존 generator의 path/SHA와 actual input bytes를 manifest에 보존한다. 제품 encoder/decoder는 import하지 않는다. generator 재실행은 기존 bytes를 덮지 않고 동일성을 요구한다. 제품 실행기는 별도의 replay로 frozen expected와 실제 event/name/offset/length/hash를 대조한다. 모델은 미sync file의 confirmed 길이와 현재 prefix의 half/latest full을 후보로 두며 unsynced names의 부재와 rename old/new/absence를 독립 조합한다. arbitrary torn sectors/실제 전원 장애/복구를 입증하지 않는다. 보호 상한은256images/cut, 실제 frozen 분모50/308/32를 모두 일치시켜야 PASS다.

## runtime 고정10개 test

| test suffix (`doublewrite_install_`) | cases |
| --- | --- |
|literal_profiles|3profile의 정상25effect/32observation, literal/전체 installed bytes·기존파일불변·0LSN/4096offset·readerBusy/drop/다시reader·공통fence 관측, stage 반환 없음|
|pristine_preflight|실제 staged Complete/실제 sync 후 nonzero2; 합성 slotNone/Idle+wrongcompleteLSN/wrongsyncedLSN/wrongcompleteoffset/wrongsyncedoffset/wrongepoch6; commonfence·append/file/writer poison4; forgottenreader1; oversizedconcreteowner1:14거절, 새 I/O0/mutation0/consume/borrow 반환(누수handle예외)|
|namespace_preflight|7개 각 잔여물 모두 내용/원본 보존, mutation0; metadata failure는 observationfault test가 담당|
|effect_failures|25×BeforeError/AfterError=50; exact phase/attempted/completed·보존 prefix와 기존file불변·WAL/file/namespace drop 확인|
|effect_unwinds|25×BeforePanic/AfterPanic=50; 실제 hook1회·caught unwind·보존 prefix·숨은 후속 effects 없음|
|short_io|9write×Short1/ShortHalf/Interrupted/Zero/InvalidCount/PrefixError=54; 10read×앞6+PrefixEof=70. 성공3종의 정확한 retry offsets/count 및 실패의 실제 partial bytes. write PrefixError는 같은 call에서 절반 쓰고 Err, read PrefixError/EOF는 half 반환 뒤 다음 call Error/Zero로 고정|
|observation_failures|32×BeforeError/BeforePanic=64. NamespacePreflight의0mutation과 readback의25completed를 구분, hook 실제도달·실패 after artifacts preservation|
|readback_rejects|semantic-negatives.json의 byteflip21/validforeign4/metadata21/late-stage1=47; 오류가 정상 selector/pool로 바뀌거나 fallback하지 않음|
|descriptor_and_workspace|XDI1 private encoder의 exact output/short length0/16383와tailpreserve;3profile digest; 준비→zero stream→control재생성 동일성; 원본 input 불변; workspace 전체 caller소유·large internal arrays없음. 무효 기대Profile은 실제 Fresh에서 만들 수 없으며 arbitrary unsafe구성 반례를 만들지 않음|
|resource_and_drop|3profile 정상 handles/install peak2file(writer dir/lock별도),반환1file·drop0/무효과, concrete owner/plan/File/names/scalar ledger를 상한과 대조. reader lifecycle128회에서도 ownerworkspace/slot 유지·handle bounded. forgottenreader회수를 주장하지 않음|

fixed prime state cases는 순서대로 별개 fresh fixture다. runtime 전체 실제 trace는3profile×25=75effects와3×32=96observations이며 parse해야 할 witness의 누락/중복도 거절한다. 함수 이름은 위 suffix를 그대로 사용한다. suite를 정규식 일부만 실행하여 full10으로 표시하지 않는다.

## compiler9와 checker 반례31

r6-inputs/compiler에 baseline1+positive2(release/shared_thread)+negative6를 보존했다. negative는 consumed Fresh E0382, live fresh reader E0505, workspace/slot E0499, live installed reader E0505, raw stage API E0599이며 모두 해당 probe의 primary line8이어야 한다. 다른 파일/line·우연한 syntax/import/build 오류는 PASS가 아니다. 실제 unmodified candidate baseline과 positive2를 먼저 성공시킨다. code/hash/line과 실제 Cargo JSON diagnostics를 보존한다. 기존 compiler checker의 wrongpath/nonprimary selftest를 신규계열에도 적용한다.

checker-negatives.json은 trace7/image7/native12/observation5=31 known-bad cases다. 실제 정상 output의 해당 field를 한 번 바꾸거나 정해진 row를 제거/복제하여 정해진 failure family를 확인한다. malformed JSON을 정상 PASS로 바꾸지 않는다. baseline actual binary/source/file identity·native hook elapsed/exit 조건을 빼먹는 checker도 검출해야 한다.

## native Linux 고정3개 test

1. `doublewrite_install_native_profiles`: profile1/2/3 각각 실제 Fresh 생성→새 설치→clean allocation reader, kernel file metadata의 실제 device/inode/nlink/length·전체 hash·기존파일 before/after 같음. 실제 entropy DB를 독립 literal에 offset치환하고 모든 affected digest를 다시 계산하여 expected를 만든다. 제품 encoder/decoder로 expected를 만들지 않는다.
2. `doublewrite_install_native_faults`: 새 process/DB마다25effects×before/after=50children. actual hook hit1·phase/effect/counters·원본 files·partial prefix를 controller에서 검산한다. 같은 worker binary를 SHA로 보존하고 child마다 compile-time source hash를 기록한다. native logical call의 실제 effect 뒤 injected error를 ordinary returned-error와 구별한다.
3. `doublewrite_install_native_sigkill`: 정상 effect index4,6,7,8,9,10,13,16,19,20,23,24,25 각각 after hook에서3회,39children. controller의 hook 수신 이후 child만 SIGKILL하고 actual signal/terminal manifest/file bytes를 검산한다. hook60초, exit10초, 런타임 rendezvous5초. hook 미도달·deadline 초과·다른 exit는 FAIL. 강제 종료는 실제 power-loss/recovery가 아니다.

새90개 이상 fake fixture의 내구성을 native 증거로 세지 않는다. native total92DB=3normal+50fault+39kill, child89이며3root receipt 및89child receipt/source/binary/각 actual file inventory를 독립 checker가 대조한다. 모든 DB bytes를 보존해도 정상완전92×22,069,248의 상한은2,030,370,816bytes(약1.9GiB)+logs/고정 binaries이며 partial fixture는 이하이다. 반복빌드/검토용target과 buffer현행검사는 별도디스크몫이다. worker당copy 대신 같은 보존 binary를 직접 launch할 수 있는 기존 방식을 따르고 baseline file inode identity를 bytes로 대체하지 않는다. 안전한 종료 후만 임시target/중복실행파일을 정리한다.

## 통합·승격

최초 채택 당시 V1은 exact10runtime, native3, compiler9, independent actual trace/model/native/observations, checkerknown-bad31, engine/worker all-targets/all-features Clippy·fmt였다. 당시 V2는 같은 frozen source의 DEV3 current leaf·하위DEV0/1/2회귀·current-build DEV2 kernel18/native99를 요구했다. 이 일괄 재실행 요구는 아래 사용자 지시로 대체한다. 원본 계약·실행 FAIL의 identity와 기존 aggregate1200/1800/2400/3600·percommand 한도는 보존한다. 장시간V3/PF/전체DEV3를 제한 검증의 PASS로 표시하지 않는다.

<a id="changed-code-test-scope"></a>
## 변경한 코드로 한정한 검증 — 사용자 지시

2026-10-21 사용자가 수정 없는 코드의 반복 테스트를 금지하고 [AGENTS.md](../../AGENTS.md)에 반영하도록 지시했다. 이후 DEV0·1·2 자체 금지가 아니라 **단계와 관계없이 새로 작성하거나 수정한 코드만 테스트**한다는 뜻으로 명확히 정정했다(commit `4273343`). 이 명시적 사용자 결정은 기존 V2/V3·하위 회귀 일괄 재실행 요구보다 우선한다. 테스트 선택 전 마지막 검증 이후 실제 diff를 확인하고 이미 완료한 같은 코드의 시험은 재실행하지 않는다.

이 단위의 source4e594d59…는 기존 runtime10·native3·compiler9·checker31·Clippy/fmt 완료 뒤 바뀌지 않았다. [변경 범위 검증 정리](../evidence/dev3-doublewrite-install-scoped-xadb-20261021-r2.json)는 그 원본 결과를 연결한 파생 기록이며 새 실행이 아니다. 통합 leaf의 선행63check는 완료했고 마지막 하위 회귀는 당시 사용자 지시로 중단되어 원본 전체 상태는 FAIL이다. 이어 실행할 DEV2 kernel은 NOT_RUN이다. 이를 새로운 하위 회귀·복구 PASS로 바꾸지 않는다. 독립 검토는 기존 r12/r13 감사와 종료 상태·현재 diff의 읽기 전용 확인을 사용한다. 제품의 geometry·effect·오류·자원·고정 입력은 이 실행 범위 변경으로 수정하지 않는다. 상세 구현·제약은 [구현 기록](../implementation/dev3-implementation.md#doublewrite-install-results)을 따른다.

freeze 뒤 root가 구현·검증하고 별도 reviewer가 실제 diff·독립기대값·정상/실패 raw evidence·same-source identity를 검토한다. 새 contracts/implementation/evidence/catalog/readme는 원본identity를 유지하여 등록하며 검토채택과 실제실행을 별도표시한다. user의 meaningful-unit commit·unused-data cleanup 요청을 계속 적용한다.

# r8 채택 보충

## Logical effect와 short I/O

attempted/completed는 write_all_at을 포함한 logical mutation25개의 수다. lower write_at의 Short/Interrupted retry subcall은 별도 trace이며 counters를 증가시키지 않는다. effect i를 시작하기 직전 attempted=i가 되고, 그 logical operation이 성공 반환해야 completed=i가 된다. BeforeError와 AfterError 모두 attempted=i/completed=i−1이다. readback은25/25, 두 preflight는0/0이다. 실제 after-effect bytes와 completed를 혼동하지 않는다.

short_io의 Short1/ShortHalf/Interrupted는 해당 logical write/read의 **첫 lower call에 한 번** 적용하고 나머지는 정상 호출이다. Zero/InvalidCount/PrefixError도 첫 call에 주입한다. write PrefixError는 그 call에서 half prefix를 쓴 뒤 Err; read PrefixError/PrefixEof는 첫 half 반환, 다음 call에서 Error/Zero다. 각 subcall의 offset/request/result와 hit1을 검산한다. 지속적인 Short1로 수백만 call을 유발하는 별도 분모를 만들지 않는다.

실패 cause는 Io(original foundation Error)다. write는 code=IoWriteUnknown/effect=CallerMustResolve이며 Zero의 io_kind=WriteZero, InvalidCount의 io_kind=None, PrefixError의 io_kind=Other다. read는 code=IoRead/inner effect=Unchanged이며 Zero/PrefixEof의 io_kind=UnexpectedEof, InvalidCount의 io_kind=None, PrefixError의 io_kind=Other다. outer effect는 소비한 Fresh 때문에 모두 CallerMustResolve다. 이 값들은 기존 foundation helper를 따르며 새 오류로 치환하지 않는다. Short1/ShortHalf/Interrupted는 정상25/25 결과를 반환해야 한다.

## readback47의 정확한 거절

모든47개는 Phase::Readback, Effect::CallerMustResolve, attempted25/completed25이며 r8-semantic-negatives.json의 지정 observation에 도달해야 한다. content mutation은 해당 read 직전, metadata mutation은 해당 open/metadata 반환에 한 번 적용한다. 다른 정상 fixture bytes/metadata는 유지한다. r6의21+4+21+1을 줄이거나 대체하지 않는다.

| 대상 | 정확한 cause |
| --- | --- |
|profile full hash, XDI1 full prepared hash 불일치|ContentMismatch|
|DWB 미재봉인 flip0/48/112|Control(Corrupt): 기존 selector decoder의 magic/checksum 검사|
|DWB 재봉인된 다른 selector generation|SelectionMismatch: 유효 decode 뒤 prepared SelectedControl 대조|
|bank0 logical160의 flip0 또는 재봉인 batch1|Control(IdentityMismatch): 기존 selected-control bind_digest가 immutable ref와 대조|
|bank0 tail·bank1·reserved·images의 nonzero byte|ContentMismatch|
|open handle length−1/+1|LengthMismatch: named metadata/read 전에 검출|
|named inode 변경|IdentityMismatch: handle-bound identity와 비교, read 전에 검출|
|늦은 selector-stage 존재|Residue|

검사 순서는 opened length, named presence/identity, named length, exact read, 해당 bytes 검사다. named metadata None도 IdentityMismatch다. lower open/metadata 오류는 Namespace(원래 NamespaceError), lower read 오류는 Io(원래 foundation Error)를 원래 내부 effect까지 보존한다. namespace preflight 잔여물은 NamespacePreflight/Residue/0/0이다. metadata가 정상인47개에서 뜻하지 않은 I/O/codec 오류를 성공한 반례로 인정하지 않는다.

validforeign는 profile1 fixture 기준이다. profile2는 동일 namespace의 유효 profile, descriptor는 namespace 변경 후 XDI1 checksum 재봉인, bank0는 batch1로 바꾸고 XDW1 checksum 재봉인, selector는 selected generation2로 바꾸고 XCS1 checksum 재봉인이다. 이 중 bank0와 selector가 참조하는 immutable expectation은 원래 plan이다. byte 변경 이외 file lengths/identities를 바꾸지 않는다. stage/selector/bank0의 내부 checksum이 맞고 profile2 자체가 유효함을 test-side 독립 입력으로 확인하여 단순 checksum 손상과 구분한다.

## 메모리·FD·native inventory

metadata8MiB의 기존 component64KiB를 phase별로 재사용한다. Fresh 생성의 scratch/AllocationEntry/list-owned 임시값은 이미 종료했고, **전체 retained Fresh를 포함한 새 returned owner**≤16KiB, 별도 plan≤4KiB, 추가 File≤16KiB, 모든 names/scalars/codec/hash≤16KiB 합계≤52KiB는 그64KiB 안이다. returned owner에 포함된 plan을 보수적으로 중복 계상해도 이 상한 안이어야 한다. 새64KiB를 기존 metadata 예약 위에 자동 가산하지 않는다. 기존 locator1MiB+WAL slot1MiB+component64KiB를 제외한 metadata 잔여는6,225,920bytes다. system flush4MiB workspace는 별도 몫이고 owner 전체 수명 동안 빌린다. D1에서 실제 concrete sizes와 호출별 동시 생존값을 대조한다.

Linux peak4FD/returned3FD는 component(writer의 cloned directory+LOCK, WAL, 순간 extra file) 범위다. caller 원래 LinuxDirectory를 유지하면 process에1FD를 별도 가산해 각각5/4가 된다. caller original directory를 Drop해도 writer의 cloned directory lease는 유지된다. reader의 추가2file과 외부 controller/stdio는 따로 계상하며 global128FD 증명으로 쓰지 않는다.

유한 모델의14file은 unchanged LOCK0을 제외한다. native 전체 inventory는 LOCK도 보존하고 실제 device/inode/nlink/length와 before/after SHA를 검산한다. LOCK을 모델14file에 임의 추가하여 기존 image 분모를 바꾸지 않는다. native92DB/89children·2,030,370,816bytes 상한은 LOCK0 포함 여부로 바뀌지 않는다.

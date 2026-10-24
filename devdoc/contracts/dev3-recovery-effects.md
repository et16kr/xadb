# DEV3 현재 genesis의 실제 닫힌 복구

2026-10-23 · **제한 구현설계 D2 ACCEPTED** · 실제 구현·실행 상태는 [구현 기록](../implementation/dev3-implementation.md#recovery-effects)을 따른다.

[재시작 복구](dev3-task-12-recovery.md#early-recovery) · [기존 prefix 분석](dev3-recovery-prefix.md) · [고정 공간](dev3-control-format.md) · [예약 정본](disk-storage-design.md#recovery-reservation)

제품 기준 `e6153995bdc6a708346db4039efaf963d5c7b618`, 실제 착수 HEAD `e9cc2737a392b2a1ce726346bb36298c62ca438c`(후속 CI 전용 commit)다. 사전 분석과 산술은 채택·실행 증거가 아니다. 현재 readonly Prefix와 초기 SELECT의 공개 지원/거절을 유지한다.

## F01 지원과 종료

- profile1/2/3, XBS plan1/2, 초기 FILEMAP single extent, genesis epoch1, 실제 exclusive/dirty reserve/cancel·첫 Status INIT의 기존 Prefix grammar만 소비한다. current/base/rebase body·새 epoch/CLR/undo/READY를 새로 성공 처리하지 않는다.
- 실제 caller maintenance12MiB와 flush4MiB의 고유 mutable borrow, source-owned64KiB census, DB FD5의 순차 resource guard를 선행한다. Prefix262144와 physical A/S 두 image는 maintenance의 disjoint 영역이다. flush는 source hashing/slot/control/zero-write I/O를 phase별 재사용하며 foreground가 빌리지 못한다.
- 동일 lease/source/workspace 아래 PREINSPECT → 실제 고정 bootstrap 확보 → source 안정화 → selected Pending1 repair/whole retirement → 같은 prefix의 예약 한 번 인계 → page별 REDO → 새 recovery DWB flush → 실제 affected home 검산의 닫힌 inspection을 공급한다. PREINSPECT는 미지원/손상 complete source를 첫 변경 전 거절하는 기존 pure canonical 검사다. physical home를 owner/ledger 정본으로 삼지 않는다.
- 성공은 실제 home의 해당 canonical 상태 및 selected DWB/active owner·remaining 관측이다. 일반 writer/reader, source append/truncate/reclaim, terminal cancel/abort, REBASE 선택, READY 권한을 반환하지 않는다. 전체 3.12/DEV3/F05 완료와 구별한다.

## F02 API와 소유

공개 내부 표면은 `ClosedRecovery::acquire(directory, expected, maintenance:&mut[u8;12582912], flush:&mut[u8;4194304])`와 consuming `recover(self)`다. acquire는 실제 두 배열 전체를 받아 safe split하고 directory borrow·W·source File·private per-W budget을 소유한다. public Prefix와 별도 backing을 합치는 constructor나 QuotaPool/caller-token 계층을 추가하지 않는다. source File은 W보다 먼저, 모든 실제 File/lease는 local allowance보다 먼저 Drop한다.

acquire는 lease·무변경 PREINSPECT·inventory/zero 검산·고정 자원 preflight까지 처리하며 provisioning은 recover에서 시작한다. 성공 inspection은 borrowed physical images/ledger/effect trace 관측만, failure는 최초 원인·누적 effect·known/possible selected closure 관측과 Drop만 제공한다. `recover(self)->Result<RecoveryInspection,RecoveryFailure>`의 실패 owner도 실제 arena/lease를 유지한다. raw W/File, Clone, retry/apply/sync/READY 탈출구는 없다. acquire 오류는 partial owner를 Drop하여 lease/borrow를 반환하고 하부 원인·효과 관측을 반환한다.

초기 구현은 새 private child가 Prefix의 내부 state를 move해 소비한다. public scalar/raw-parts constructor로 live MutableStorageOwner/StatusRole/CapturedPage/DurableWal/RetiredBatch를 복원하지 않는다. canonical validation과 physical page decision/ledger adoption은 다른 책임이다.

## F03 고정 bootstrap의 실제 확보·재진입

- 새 효과 entry만 reserved-only `d3-rebase-0/1`를 허용한다. CURRENT와 그 stage, writer-stage, filemap-stage는 계속 미지원이다. reserved-only 파일은 base 선택 권위가 아니다. public readonly Prefix는 기존처럼 두 rebase 이름을 거절한다.
- 두 파일은 absent 또는 regular owned exact identity의 length0..1048576 및 존재하는 모든 bytes zero만 지원한다. nonzero/oversize/identity mismatch는 첫 provisioning 전 거절한다. zero sparse file의 존재를 실제 공간 확보로 간주하지 않는다.
- 매 진입 고정 이름 두 곳에 64KiB 이하 zero chunk로 full1MiB씩 write하고 file sync, final directory sync한다. 새 파일은 create_new, 기존은 truncation 없는 ReadWrite/opened/named 검산 후 덮는다. 0..capacity partial creation/write·sync 불명 뒤 새 entry는 같은 두 이름만 검사·완성한다. 새 임시 이름을 늘리지 않는다. 기존 zero file도 rewrite+sync하여 이번 진입의 실제 I/O 확보를 관측한다.
- 완전 recover→다음 PID에서 recover의 home/DWB write0와 BOOT2slot rewrite를 구별한다. 정상 source WAL/WRITER/profile/FILEMAP bytes는 항상 보존한다. source_sync는 bytes 변경과 다른 durable effect다.
- bounded namespace inventory는 선택 fallback에 쓰지 않고 actual owned/unknown name 검산에만 사용한다. `list_owned` Vec 실제 capacity/backing와 temporary enumeration FD를 source/resource census에 넣고 다른 temporary File을 동시에 보유하지 않는다. LOCK의 실제0byte와 regular-owned identity도 구별한다. 지원 Linux adapter의 inventory 상한은 LOCK 포함18개다. mandatory15는 LOCK,d3-profile,WRITER,FILEMAP,d3-wal-00000001,d3-bootstrap,d3-filemap-0/1,d3-data-0000000100000001,DWB,d3-dwb-0/1,d3-dwb-stage,d3-dwb-images,d3-dwb-reserved이고 optional3은 DWB selector-stage와 REBASE 두 slot이다. LOCK는 regular/single-link/length0만 허용한다. unknown/19번째 이름과 nonzero LOCK는 보존 거절한다. Vec는 deep codec 전에 Drop하며 actual capacity×size_of(DirectoryEntry)+header를 census에 포함한다. 지원 adapter는 bounded inventory allocation을 제공해야 하며 반환 뒤 검사만으로 임의 port의 내부 무제한 allocation/OOM을 사전 방지했다고 주장하지 않는다.
- actual file lengths, 미실현 two-slot/control credits, existing DWB selector-stage credit, remaining WAL/page/frame/memory, replay metadata topup을 한 번만 합산한다. actual2slot 증가만큼 outstanding growth가 감소한다. 논리 quota 예약·write+sync·native allocated blocks witness를 구별하며 외부 ENOSPC/I/O 장애는 fence한다.
- 미래 epoch/header의 WAL charge는 REBASE durable base+old source 해제 뒤의 정본 순서를 따른다. old256MiB+new epoch를 동시에 더해 정상 admitted prefix를 거절하지 않는다. 아직 구현하지 않은 REBASE/epoch guard를 완료 권한으로 반환하지 않는다. 이 부분의 실제 current-scope 예약표와 남은 전체 의무는 F07에 고정한다.

## F04 source seal·Pending 수리·DWB 선택

- held read-only source File은 explicit sync가 가능하다(Foundation OpenMode 계약). source File sync와 directory sync 후 same identity/physical length/adopted end에 묶인 private recovery source seal을 만든다. live DurableWal을 scalar로 만들지 않는다. partial tail bytes/길이를 보존한다.
- selected Pending만 slot0/16544bytes를 읽는다. namespace·batch/slot·PageRef/incarnation·owner/kind/level/LSN·slot/page/checksum/full image digest 및 그 historical canonical image를 모두 대조한다. retired raw slot·stage·비선택 bank를 repair source로 쓰지 않는다.
- home가 healthy higherLSN/incarnation이면 거절한다. older/equal target 또는 torn home의 정확 정책은 valid selected image에만 의존한다. exact equal image는 home write를 생략하지만 retirement 전 file sync는 수행한다. healthy equalLSN이면서 전체 bytes가 다르면 invariant 거절한다. healthy 다른 identity/kind/incarnation 또는 unsupported valid format은 덮지 않는다. decode의 IdentityMismatch/KindMismatch를 torn으로 바꾸지 않는다. UnsupportedFormat은 outer checksum도 유효하면 거절하고 checksum-invalid torn과 구별한다.
- repair는 home write(필요한 경우)→target file sync→같은 batch Empty retirement(g+1)를 선택한다. selected Pending의 historical image를 최종 canonical image로 바꾸어 쓰지 않는다. 이후 REDO가 후속 frame를 적용한다.
- 새 REDO output은 last-issued b를 재사용하지 않고 b+1로 시작한다. batch k의 issuance/Pending/retirement generation은 checked `3k−1/3k/3k+1`, bank는 `(g−1)%2`다. 두 dirty target이면 마지막 batch b+2의 `3(b+2)+1`까지 첫 effect 전 checked 검산한다. issuance Empty g2/b1→두 target 후g10/b3의 +8 branch를 포함한다. 기존 live executor의 current g+1 연속 산술을 그대로 쓰지 않는다.
- selector-stage 잔재0..4096은 named/opened identity를 확인한 동일 regular-owned file을 덮어4096로 준비·sync·close→publish→dirsync한다. raw 잔재 bytes는 선택 권위가 아니다. create_new만 사용해 정상 잔재를 영구 거절하지 않는다. unselected bank/control operating stage는 같은 고정 capacity 안에서만 write한다.
- first mutating syscall 이후 known/possible selection union·최초 typed cause와 누적 effect를 보존해 fence한다. 부분 home/file sync·publish/dirsync 실패에서 일부만 retirement/release하지 않는다.

## F05 한 번 인계와 page별 REDO

- accepted descriptor/inventory/active/high-water/remaining을 same-prefix seal로 검산하여 per-lease recovery ledger에 정확히 한 번 move한다. Finished를 Active로 부활시키거나 remaining을 축소하지 않는다. physical skip/apply는 이 논리 전이를 반복하지 않는다.
- 두 번째 streaming canonical pass의 genesis/prior validated before와 별개로 physical A/S cursor를 유지한다. WAL 의미 검산은 기존 pure core를 공유하되 mixed physical before를 live/runtime group validator에 넣지 않는다.
- healthy same incarnation, pageLSN>=frameLSN인 page만 skip한다. historical whole after와 같음을 다시 요구하지 않는다. adopted end보다 높은 LSN/설명 없는 incarnation은 거절한다. 미적용 Patch는 exact priorLSN·target identity·page-local canonical before 조건을 검사하고 validated after를 적용한다.
- `allocator@2/Status zero`와 반대 mixed 상태에서 최신 target만 skip하고 다른 target을 따라잡는다. 그룹 전체 skip은 없다. canonical observation의 descriptor/ledger를 target마다 다시 차감하지 않는다.
- Status zero/torn은 selected valid Pending 또는 완전 INIT38000 + 채택 끝까지의 연속 WAL에서만 재구성한다. allocator는 Patch-only이므로 Pending 없는 torn initialized allocator를 XBS canonical RAM이 있다는 이유로 덮지 않는다. healthy future/newer incarnation도 덮지 않는다.
- checksum-valid initialized A/S는 generic page envelope 뒤 현재 grammar의 typed allocator/Status 형식도 검산한다. allocator의 추가 cell·잘못된 shape를 Patch로 숨기지 않는다. 미적용 Patch의 범위 밖 header/padding은 LSN/checksum 갱신을 제외하고 보존해야 한다. healthy Status가 INIT보다 낮은 LSN이면 검증한 INIT record의 encoded priorLSN과 정확히 같아야 하며, 설명 없는 Status@1/INIT@2·prior0는 거절한다. zero/checksum-torn의 완전 INIT 예외와 current/newer page별 skip는 유지한다.
- fixed two-page physical scratch의 dirty target은 최종 canonical 상태까지 따라잡은 후 A→S 순으로 각각 새 recovery DWB batch를 설치할 수 있다. 각 step의 skip/apply/prior 검산은 생략하지 않는다. source seal → issuance Empty 선택 → slot/image sync → Pending 선택 → home write/sync → whole retirement를 지킨다.
- final inspection은 실제 affected home 전체 bytes/identity/pageLSN과 same-prefix ledger를 대조한다. healthy equalLSN의 impossible final state는 skip만으로 성공하지 않는다. checksum-invalid initialized allocator는 error이며 source/home를 임의 초기화하지 않는다.
- 이번 닫힌 owner는 physical replay topup도 끝까지 보수 유지한다. serialized remaining.protected/PageToken·retained source·terminal mask를 줄이지 않으며 source sync/깨끗한 home/retirement를 quota 반환으로 바꾸지 않는다. immutable Prefix facts와 physical apply/skip/progress 관측을 구별한다. 향후 topup 해제는 실제 소비자가 생긴 뒤 별도 증명한다.

## F06 오류·효과

Phase는 Preflight/Lease/Prefix/Inventory/BootstrapProvision/SourceStabilize/PendingValidate/PendingHome/Retirement/ReserveRestore/Redo/Flush/Verify/Snapshot으로 구분한다. Cause는 실제 Namespace/Io/Control/PageIo/Allocation/Doublewrite/Wal/Semantic와 Capacity/Unsupported/identity/prior/invariant/fenced를 보존한다. first error를 후속Fenced로 바꾸지 않는다.

provisioning/source sync 뒤 torn-home/slot 거절은 NoHomeChange일 수 있어도 DB Unchanged가 아니다. 관측에는 provisioning/source-sync/DWB/home별 attempted/completed·durable/known/possible을 분리한다. outerEffect는 이전 성공/불명 mutation까지 누적한 CallerMustResolve이며 lower current syscall이 Unchanged여도 과거 효과를 지우지 않는다. acquire_writer 자체의 Err는 원 Namespace cause/effect를 보존한다. 성공 lease는 LOCK 생성 여부를 노출하지 않으므로 새 entry의 모든 postlease 오류 outerEffect는 보수 CallerMustResolve이며 `lease_effect=MayHaveChanged` 관측을 보존한다. 실제 LOCK 생성이나 provisioning 시도를 관측했다고 표시하지 않는다. prelease capacity 오류만 Unchanged다. missing LOCK+invalid closure에서 LOCK만 새로 생길 수 있는 반례를 검증한다. 기존 readonly Prefix 의미를 이 때문에 변경하거나 하부 API를 추가하지 않는다. 실패 owner에 재시도 API는 없다. 다른 PID 재진입이 disk 선택을 다시 판정한다.

## F07 고정 수치와 receipt 경계

`F0=22069248`, DWB stage capacity`D=4096`, 기존 protected fixed`B=21024768`, 두 reserved slot capacity`Zcap=2097152`, 미래 CURRENT4096+CURRENT-stage4096+WRITER-stage4096의 credit`Ccap=12288`다. 기존 WRITER4096은 F0에 포함한다. FILEMAP은 불변이므로 그 stage credit을 이 제한 owner에 추가하지 않는다. `R=source physical length−4096`, `S=실제 DWB selector stage length`, `Z=두 reserved slot 실제 length 합`, `W=remaining.wal`, `P=remaining.protected`, `M=기존 검증한 replay topup`이다.

| 항 | 식·상한 |
| --- | --- |
| actual owned | F0+R+S+Z |
| outstanding | (Zcap−Z)+(D−S)+Ccap+W |
| owned 적합 | **24182784+R+W≤8GiB** |
| protected 적합 | **23134208+R+P+M≤1GiB**; P에 W/page credit을 다시 더하지 않음 |
| source WAL | physical length≤268435456 및 complete_offset+W≤268435456 별도 |
| 보수 component 상한 | R≤268431360,W≤38000,P+M≤1082672 → owned292652144/protected292648240（첫 Status 제한; F09 확장 수치는 아래 참조） |
| actual caller | maintenance12582912+flush4194304=16777216 bytes 전체 loan |
| source | entry/owner/failure/Result/guard/Vec/codec 동시량≤65536; 구체 크기는 구현 뒤 첫 실행 전에 독립 확인 |
| entry 귀속 FD | directory1+lease2+source1+local File 또는 enumerator1=5; 프로세스 전체 FD·임의 외부 reader·OS open 성공 보장이 아님 |

서로 다른 state의 component 최대를 더한 상한을 실제 동시 달성 workload로 주장하지 않는다. slot 또는 stage의 실제 길이 증가만큼 outstanding을 actual로 이동하고 overwrite의 누적 I/O bytes를 owned에 더하지 않는다. 불명/실패 write 뒤 guessed completion으로 credit을 반환하지 않는다. 다음 entry가 실제 길이를 다시 계상한다. ledger Drop은 파일 삭제가 아니다. 실패 owner의 actual_owned/outstanding은 마지막 확인·완료 stamp에 근거한 charge와 아직 보호하는 capacity credit이다. 불명 write 뒤의 현재 물리 길이를 다시 측정했다고 해석하지 않는다. native snapshot의 실제 파일 길이/바이트 관측은 별도이며, 불명 증가분의 credit을 추정 반환하지 않는다.

미래 REBASE가 두 base/continuation을 durable 인계하고 모든 old source handle을 닫아 unlink+dirsync한 뒤에만 old physical length(R+4096)을 반환한다. 그때 새 epoch header4096+W를 확보해 WAL cap을 검산한다. 현재 epoch credit은0이며 old cap 가까운 source에4096을 미리 중복 요구하지 않는다. 이 leaf는 해당 REBASE/reclaim/LOG_OPEN을 실행·인증하지 않는다. 생성 때부터의 일반 fixed bootstrap 설치 및 전체 F05 귀납 증명은 별도 미완료다.

receipt 전 검사는 optional이 아니다. 두 reserved slots의 기존 전체 bytes/shape·exact inventory·resource/generation 상한을 모두 통과한 뒤만 첫 create/write를 시도한다. source/profile/선택 control의 기존 prefix 검산을 재사용하며 old read-only block2를 효과 entry에서만 private policy로 분리한다.

각 slot provisioning의 고정 순서는 다음과 같다.

1. 존재하면 RO open의 opened metadata와 named metadata를 대조하고 전체 visible zero bytes를 읽어 원 identity/length를 보존한 뒤 close한다. 두 slot 검산을 모두 완료한다.
2. absent이면 create_new→created File close→RW open_existing_with_metadata로 regular/single-link exact length0인 opened/named metadata를 대조하고 이 opened identity를 최초 provisioning stamp로 확정한다. 원 created handle의 identity를 알고 있다고 주장하지 않는다(FileIo에 그런 API가 없음). 이 zero-length nonauthoritative 파일만 첫 write 대상으로 삼는다. existing branch는 RW open의 opened/named metadata를 앞선 RO prevalidation stamp/길이와 대조한다.
3. offset0부터65536zero×16 logical write→file sync→close한다. set_len/truncate하지 않는다.
4. RO reopen의 opened/named metadata로 같은 inode·exact1048576을 검산하고 close한다. 두 slot 완료 뒤 BOOT directory sync를 수행하고 named metadata로 두 slot 길이/identity 및 source identity/physical length를 최종 대조한 뒤 SlotsProvisioned를 발급한다.
5. 별도 SourceStabilize에서 held source file sync→directory sync→source named identity/length 재대조가 끝난 뒤 recovery source seal을 발급한다. BOOT namespace sync를 source sync로 대신하지 않는다.

이 순서의 새 port call literal을 구현과 함께 작성한다. mutation vector는 absent37(2create+32write+2filesync+1BOOTdirsync), both-existing35이며 metadata/open/post-snapshot 호출은 별도 행으로 추가한다. 단순 source code site를 같은 초기 상태의 fault로 두 번 집계하지 않고, 공통 write/sync는 대표 absent branch, existing-only prevalidation open/metadata/read는 existing branch에서 중간 필수 fault를 선택한다. 다른 기존 length/profile의 전체 Before/After 조합은 S5다. fault inventory는 성공 trace에서 역산하지 않고 설계 vector/source를 독립 대조해 첫 실행 전에 확정한다.

## F08 구현 동반 인수·담당

- 새 기본 semantic50과 물리 BOOT24 및 missing-LOCK 효과1의 **semantic75 후보**를 구별한다. count를 숨긴 loop PASS로 집계하지 않고 각 실제 boundary ID/selector·alias를 첫 실행 전에 동결한다. source/repair/REDO/BOOT의 새 read/metadata/zero-chunk write/sync/publish/dirsync fault는 literal operation,name,offset,length,phase와 원인·누적effect oracle를 미리 작성한다. 공통 하부 source의 완료 fault를 상위 기능 이유만으로 다시 실행하지 않는다.
- native는 기존 실제 효과25+BOOT6=**31개**다. BOOT6은 absent두slot, slot0첫chunk partialerror, slot0full/slot1middle partialerror, bothfull file-sync unknown→다음PID BOOTdirsync unknown→성공/검산의 명시 history, 기존 sparse fullzero의 실제rewrite/blocks 관측, slot0zero17+slot1마지막byte nonzero 보존 거절이다. 마지막 입력은 해당 실제 file 불변 관측과 보수 lease effect를 구별한다.
- native 각 complete history30초/합300초(최종 identity/hash검산 포함), 각DB24MiB/보존DB768MiB/entry귀속FD5. 새2MiB는 각DB24MiB 안이다. 실패 phase의 slot/control/home snapshot과 metadata를 다음 진입 전에 보존하며 whole DB 복사로 보존 budget을 숨기지 않는다. source/ELF/log/특정 실패 snapshot의 별도 공간을 기록한다.
- 새 runtime 각job30초/합180초, compiler semantic9+control1 합60초, 신규 checker 합30초를 한도로 한다. exact runtime/compiler/known-bad inventory와 타입 census·native oracle/tuple·시간·disk 상한은 첫 실행 전 별도 독립 검토한다. source/native worker는 reused producer를 새 lower PASS 분모로 집계하지 않는다.
- selected historical Pending+later cancel은 실제 flusher capture 뒤 writer cancel, 이후 Pending publish unknown으로 만든다. 기존 redirty-during-I/O 공급을 사용하고 WAL bytes 사후 접합을 실제 producer로 표시하지 않는다. 단일 temporary per phase/producer FD와 barrier 진행을 검증한다.
- 양방향 mixed home·already-final skip·raw retired slot 배제·Pending historical image·healthy equal conflict/higher/incarnation·torn Patch-only reject/fullINIT restore·final exact readback·same-prefix 예약 한 번 인계·원 failed owner/다른PID 재진입을 필수에 포함한다. 모든 source bytes/hash/inode/length와 unaddressed extent 영역을 독립 oracle로 검산한다.
- 모든 expanded profile/count/home/BeforeAfter/long history는 별도 S5 selector로 준비만 한다. Prefix/scanner/초기 SELECT·기존 producer·하위 wrapper의 완료 시험은 실제 공유 diff의 직접 receiver 외 반복하지 않는다. DEV3 최종 S5 전수 예외는 그대로다.

담당은 총괄/소유 계약·통합 `/root`, kernel/effect owner·unit `/root/recovery_implementation`, 독립 fixture/native/compiler/checker `/root/recovery_acceptance`, 읽기 전용 D2/D1 `/root/recovery_design_review`다. 요청 모델은 Sol/high 구현·핵심 Astra/high 독립 검토이며 runtime identity 별도 확인은 주장하지 않는다. 구현 동시2개를 넘기지 않고 공유 prefix/reexport는 kernel 담당만 수정한다. 제한 구현설계 D2를 수용한 뒤 제품 코딩을 시작했다. D2는 scope/소유/순서/수치/오류/필수 경계를 판정했으며 실제 inventory/census의 첫 실행 승인은 구현 후 별도다.

## D2 채택 기록

2026-10-23 `/root/recovery_design_review`의 별도 읽기 전용 검토를 수용했다. 검토한 원문 SHA-256은 `9a3dc6b2c59cbc216b6c50691fedc85267ebce3bd120d1b23db72bc2fe2d4b18`이며 원 요청 SHA `9faf6340eebc5ee2e75e3d8747543c76b9f0c16e5d19b226ca6a0cd145170af1`과 F07 생성 identity 보완 사본을 별도 보존한다. 필수 차단 수정은 없다. 이 채택 기록은 원문 검토 뒤의 상태 메타데이터다.

실제 source freeze·구체 수명/census·정확한 case ID/selector·literal I/O fault vector·독립 native/compiler/checker oracle·시간/공간 한도는 첫 실행 전 별도 검토한다. 구현설계 채택을 제품 D1·컴파일·시험 성공으로 표시하지 않는다. 원 보고서와 요청/보완 연결은 로컬 `artifacts/dev3-recovery-effects-20261022-r0/`에 보존한다.

2026-10-23 첫 실행 전 source 검토에서 allocator의 checksum-valid 추가 cell을 full-copy REDO가 지울 수 있는 반례를 발견했다. typed 형식과 Patch 바깥 바이트의 보존 검산을 강화하고, healthy Status의 INIT prior 조건을 검증한 WAL record와 연결한다. `/root/recovery_design_review`가 반례와 기존 wire 규칙을 별도로 대조해 이 구체화를 수용했다. source/unit/census의 실제 수정 검토·실행 결과는 이 구현설계 구체화와 별도 gate로 판정한다.

<a id="operational-status-recovery"></a>
## F09 반복 Status 이력 복구 확장 — D2 ACCEPTED_DESIGN_ONLY

착수 기준은 `1699dd781f8ba75a8dcb4215ce364a427ea35c2d`의 [반복 INIT live owner](dev3-task-05-buffer-flush.md#operational-status-p2)다. 이 절은 F01/F05의 첫 Status 한 장 제한을 실제 `ClosedRecovery`에 한해 확장한다. public readonly `RecoveryPrefix::acquire`의 기존 닫힌 grammar와 거절은 유지한다. 동일 prefix core에 private typed scope를 전달하며 별도 복구 엔진·스칼라 권한 복원 API를 만들지 않는다. wire format, fresh fixture 정책, source 보존·실패 owner·READY 미공급은 그대로다. 설계 채택과 제품 구현/실행 증거는 구별한다.

- **허용 grammar:** plan2/Status budget, 첫 extent, 단일 Reserved batch의 reserve→cancel 또는 reserve→INIT을 반복한다. allocator의 기존 Allocated survivor는 object2/generation1/TransactionStatus 및 incarnation≥2만 허용하고 기존 live validator의 population 검산을 공유한다. 각 frame 전에 canonical allocator·retained descriptor·active/high-water를 `validate_live_in_population`으로 검산한다. trusted terminal outcome(Cancelled/InitializedStatus)을 성공한 frame 종류에 따라 추적하며 다음 reserve의 `before_descriptor=None`로 직전 Finished 검산을 대신하지 않는다. 첫 INIT 이후에도 work high-water·descriptor·inventory·remaining 검산을 매 frame 수행한다. 이미 Allocated인 주소 재INIT, 다른 kind/owner, plan1에서 Status survivor, producer mode 전환은 계속 거절한다.
- **canonical target census:** Prefix caller workspace의 descriptor 뒤 `181760..184832`에 64×48byte 고정 표를 둔다. entry는 INIT LSN8+incarnation8+full image digest32이며 Status 주소1..63만 유효하다. observation의 u64 mask로 유효 entry를 구별한다. 두 canonical pass 시작마다 descriptor1536·표3072·해당 observation을 명시 초기화하고 ALLOC genesis를 재구성한다. caller arena의 원 bytes를 초기 상태로 신뢰하지 않는다. allocator는 기존 final image/hash를 사용한다. 표는 검산된 INIT 뒤에만 채우며 중복 주소는 거절한다. 마지막 Status image 관측은 유지한다. callback은 현재 frame이 INIT일 때만 해당 Status after를 전달한다.
- **physical scratch:** 기존 maintenance12MiB에서 Prefix262144 뒤 64×16384byte를 주소별 physical image로, 그 뒤3072byte를 prepass census 보존에 사용한다. 추가 heap·DB File·extent는 없다. canonical second pass의 표와 prepass 표를 전체 비교하고 allocator/descriptor/remaining/high-water·mask도 동일해야 한다. immutable Status의 identity는 canonical census+allocator namespace에서 유도하며 physical header를 권위로 삼지 않는다.
- **page별 결정:** physical initial/final LSN·apply/skip 배열을64칸, observed/dirty/flushed/home-effect mask를u64로 확장한다. 주소0은 allocator,1..63은 실제 page 번호다. Pending도 실제 주소에 수리하며 historical A Pending 뒤 B INIT이 있어도 A의 당시 image를 쓰고 나서 후속 WAL을 REDO한다. 모든 canonical target을 읽고 각 INIT의 해당 주소만 apply/skip한다. Status의 final home 전체 digest는 각 target census와 일치해야 한다. allocator의 torn/Patch-only 거절과 healthy future/identity mismatch 거절은 유지한다. immutable Status의 upper LSN은 전체 adopted end가 아닌 census의 해당 INIT LSN이다(A@3/canonical A@2/adopted@6도 거절). unaddressed page는 읽기/쓰기 대상으로 추가하지 않는다.
- **flush/inspection:** dirty 주소 오름차순으로 같은 slot0·새 batch를 순차 소비한다. 전체 target 수 `1+popcount(status_mask)`(WAL이 없으면0)에 대해 마지막 `3*(current_batch+targets)+1`을 첫 효과 전에 checked 검산한다. 기존 `status_image()`는 마지막 INIT 주소의 물리 image, 새 borrowed `status_image_at(page_no)`는 census에 존재하는 주소만 반환한다. 둘 모두 read-only이고 새 권한/clone/retry를 발급하지 않는다.
- **예약/F05:** replay page topup은 한 Status가 아닌 전체 target 수+현재 inventory로 계산한다. 물리64page와 표는 기존 full maintenance loan 내부다. 확대된 observation/owner/Result/error의 동시 복사 및 decoder/port call depth를 source census에 반영하고≤65536을 **첫 runtime 전** 독립 확인한다. FD5·maintenance12MiB+flush4MiB·source WAL256MiB·owned8GiB/protected1GiB 한도는 유지한다. 재진입의 깨끗한 home와 source sync는 quota release가 아니다. F07의 component 상한은 이 확장에서 P+M≤1086576, protected≤292652144로 갱신한다(status62+activeReserved1: replay63+inventory1=64, page credit2, M=62×16384, P=38000+2×16384). owned 상한292652144와 실제 quota cap은 그대로다.
- **구현 동반 필수:** A INIT→B cancel→B INIT의 독립 literal 전체bytes, 각 complete/partial prefix, mixed A/B/allocator LSN, historical Pending(A/B), intact conflicting/future/wrong-owner home, torn Status 재구성·torn allocator 거절, 중복/잘못된 population/descriptor/WAL, page63 bitmap·target별 topup·DWB overflow·0xA5 caller arena·두pass stale census 비간섭을 새 selector로 작성한다. A 이후 reserve/cancel에서 Status callback을 내보내거나 INIT slice를 읽지 않는지도 직접 검증한다. fresh current-build producer→다른 PID recovery/reentry 및 B home/선택 retirement의 fault cut은 실제 native로 확인한다. 동일 source의 완료된 lower 시험/전체 wrapper를 반복하지 않는다. 변경 core의 직접 수신 old selector만 선별한다.
- **S5 확대:** profile1/2/3×첫 extent63개 INIT와 cancel 혼합, 전체 관련 home/fault 조합, 긴 반복 이력은 별도 ignored selector/fixture parameter로 구현과 함께 준비하고 중간 필수 실행에 섞지 않는다. 길이·반복 수를 줄여 PASS로 표시하지 않는다. 정확 inventory·실행 시간/공간 한도와 원 실패/source 결과는 후속 증거에 고정한다.

2026-10-23 `/root/pool_design_review`의 F09 독립 D2를 수용했다. 구현 착수만 허용하며 실제 source census·정확 inventory·시간/공간의 첫 실행 gate와 최종 D1은 별도다. 검토 원본은 `artifacts/dev3/operational-recovery-20261023/review/design-d2.md`와 JSON에 보존한다.

<a id="empty-transaction-recovery"></a>
## F10 빈 거래의 현재-build closed recovery — 제한 단위 독립 D1/D2 수용

기준 `996d0e0`의 fresh live Begin/RollbackStart/빈 AbortEnd 파일을 기존 `ClosedRecovery`가
새 PID에서 복원하고 다시 진입하는 소비자다. public readonly Prefix grammar/262144 workspace,
wire format, source 보존, 실패 owner·READY/새 epoch/자동 loser 종료 미공급을 유지한다.

private Effects 경로만 maintenance12MiB에서 Prefix262144, canonical Status64page와32×1664
거래 bank인1,101,824바이트, 기존 physical tail을 겹치지 않게 분할한다. 첫 pass와 REDO pass는
caller canonical 공간·bank·issuer/census를 각각 초기화한다. 추가 heap/File/page-sized local은
없다. 원 system descriptor는 보존하고 bank는 원 TxnRef/location과 nonterminal 의무를 가진다.

plan2/epoch1·slot0·generation2·첫 extent·한 주소당 Begin1회, 최대32 retained Active/Aborting을
지원한다. Begin local은 전역 strict 증가이며 미채택 gap은 허용한다. 첫 Begin 전에 기존 system
owner inactive/Finished와 allocator를 검산한 뒤 system mutation을 동결한다. 모든 거래 frame의
full semantic/native before·exact canonical page와 descriptor·빈 상태·budget·LSN chain을
검산한다. terminal은 after image를 채택한 뒤 bank에서 제거한다. 재Begin/nonempty/Commit/다른
slot 및 거래 뒤 system frame은 지원하지 않는다.

allocator pageLSN과 전역 adopted tip을 분리하고 각 Status target의 최신 LSN/incarnation/digest를
갱신한다. Pending은 검산한 당시 전체 image로만 수리한다. mixed physical home는 canonical
before가 아니며 페이지별 skip/apply, Patch prior/범위 밖 바이트, 마지막 전체 digest를 검산한다.
zero/torn Status는 원 INIT부터의 complete chain으로만 재구성한다. allocator torn 거절은 유지한다.

원 system과 모든 nonterminal 거래의 consumable은 합산하고 직렬 공유 progress peak는 max다.
source/replay topup은 보존하며 같은-prefix reservation으로 한 번만 인계한다. 두 pass의 원장
hash/issuer/count/aggregate/allocatorLSN/target census를 모두 비교하고 physical skip/apply에서
예약을 다시 차감하지 않는다. borrowed descriptor/count/high-water 관측만 공급한다.

component65536·maintenance12MiB+flush4MiB·FD5·WAL256MiB·owned8GiB/protected1GiB를 유지한다.
새 observation/return carrier와 순차 system/transaction codec의 source-value census 및 구체
selector/독립 oracle·fault schedule은 첫 effect 전에 별도 검토한다. 필수는3거래 상태·interleaved
issuer/32slot/terminal 후33번째·mixed/torn/old Pending·의미 변조·tail/EOF·actual DWB fault와
fresh producer→다른 PID recovery→재진입이다. S5 확대는 작성·미실행으로 구별한다.

F10의 r0 source static은 기존 additive carrier 회계에서 memory70,120/native70,328로
65,536을 초과해 FAIL이며 효과를 실행하지 않았다. `run_recovery(&mut self)->Result<()>`의
borrow 작업이 끝난 뒤만 wide Inspection/Failure를 구성하도록 분리했다. source-value는
(A) Prefix 실행 중 미구성 outer owner 대신 Prefix 전체+caller args/error+보수 return1,
(B) 원 owner+small Result+보수 wide slot1+IO/error/name/metadata+codec/callback,
(C) 완료 뒤 owner+wide return2+IO/error carrier의 **max**다. 공통 carrier를 삭제하지 않으며
물리 stack/ABI/RSS 주장은 아니다. r1은 memory entry59,200/work64,368/return18,256,
native59,296/64,480/18,392로 각각 peak64,368/64,480을 선택 static에서 확인했다.
첫 effect의 독립 gate·native 실행은 별도다. source와 실패 원본은 같은 빈 거래 통합에 이어 기록한다.

거래 branch의63 Status+32 Active component 상한은 P+M=32×6080+64×16384=1,243,136이며
F09의1,086,576을 그대로 쓰지 않는다. 기존 F07 식의 R 상한268,431,360과 W 상한194,560을
각각 넣은 owned/protected 보수 component 상한은292,808,704다. R과 W 최대의 동시 실현을
주장하지 않으며 complete_offset+W≤WAL cap 및 실제 profile/1GiB 검산은 계속 적용한다.


F10의 실제 selector·variant·S5 배정은 [복구 manifest](../../crates/engine/tests/fixtures/dev3-empty-transactions/recovery-manifest.json)에 고정한다.
새 memory13함수104variant, 직접 수정된 System callback/readonly2함수10variant와 변경 source census2를
구별한다. descriptor 관측의 compiler3와 unmodified control1도 별도다. `status_image()`는 기존
마지막 INIT 주소의 현재 물리 image 의미를 유지하며, 거래별 최신 주소는 `status_image_at()`로 본다.

native는 첫5history17PID의30초/history·150초/선택·24MiB/DB·120MiB보존 결과를 그대로 둔다.
원 입력의 DWB image stage 경계를 닫기 위해 별도 `aborted_slot_partial`1history4PID를 추가했다.
추가는 XDS1 slot offset0/16544 중8192 실제 쓰기 뒤 오류, selected Empty generation5/batch2,
Status home 미쓰기와 새 PID에서 batch3 복구/재진입을 검산한다. 전체 authored inventory는6history21PID,
180초·144MiB로 기록하지만 이미 완료한5history를 새 source에서 반복하지 않는다. receipt/snapshot16MiB와
source/ELF/cache의 별도 귀속을 유지한다. S5는 profile3×Free/Active/Aborting/Aborted×home4의48조합을
ignored selector로 작성만 한다. 결과와 원 FAIL은 [같은 통합 증거](../evidence/dev3-empty-transaction-20261023.md#closed-recovery)에 이어 보존한다.

# DEV3 Status home flush 소유 계약

2026-10-22 · **D3-STATUS-FLUSH-001 · 3.03.2.b · 제한 D2 수용 / 필수·native 선택 실행 완료 / 최종 독립 D1 수용**

[Status INIT](dev3-status-init.md) · [allocator flush](dev3-allocator-flush.md) · [buffer/flush](dev3-task-05-buffer-flush.md) · [DWB 표현](dev3-doublewrite-format.md) · [시험 계획](../plans/dev3-test-plan.md#status-home-flush) · [현재 상태](../implementation/dev3-implementation.md#status-home-flush)

이 단위는 실제 INIT가 공개한 첫 Status page와 그 allocator를 각각 단일-page batch로 flush한다. 두 target을 한 batch로 합치지 않으며 일반 target 선택·batch8/64·다중 file·Status 변경/거래 발급·free/reuse·recovery/reopen/READY를 제공하지 않는다. 기존 .a의 성공 결과와 원 FAIL은 유지한다. D2와 소스 검토 수용은 제품 실행이나 F05 인수 PASS가 아니다.

<a id="ownership"></a>
## 원래 owner를 소비하는 조립

`BoundStatusBootstrap::into_with_flush(self)`는 기존 bound 전체를 값으로 보유하는 새 `BoundStatusWithFlush`와 별도 `StatusFlushControl`을 만든다. 기존 .a bound layout과 allocator 전용 FlushControl의 API를 확장하지 않는다. 원4MiB loan을 한 번 take하고 추가 pool·namespace·page buffer를 만들지 않는다. 기존 binding/allocator readback/등록을 반복하지 않는다.

순서는 health→actual core/binding/fence→고정 layout와 runtime generic admission→workspace nonblocking try_lock→health 재검사→원 loan take→guard 반환→infallible field move다. 건강한 Capacity/Busy는 `IntoFlushError::Before { error, bound }`로 정확한 원 bound를 반환한다. poison/missing workspace/origin 손상은 Fatal이며 usable bound를 반환했다고 주장하지 않는다. take 뒤 callback·추가 fallible 동작은 없다. wrapper의 field Drop 순서는 inner의 fence가 retained flush 보호 반환보다 앞선다. Drop은 hidden I/O를 하지 않는다.

`split_with_flush(&mut self)`는 field-disjoint writer·reader·단일 mutable flusher를 대여한다. flusher의 공개 내부 진입점은 `try_flush_allocator`, `try_flush_status`, `continue_flush`, `snapshot`이다. raw target 인자는 없다. view Drop은 workspace/capture/authority/retired progress를 소유자에 남기며 두 번째 flusher·활성 loan 아래 재split/owner 이동은 borrow로 금지한다.

<a id="authority"></a>
## publication·capture·retirement 권한

non-Copy/non-Clone `StatusFlushAuthority`는 외부 core/binding/fence 참조, exact TargetRequest와 allocator identity를 보유한다. 실제 pool의 OnceLock publication과 영속 StatusRole, 첫 allocator 등록이 일치할 때만 상위 owner가 만든다. movable bound 내부의 sibling을 참조하지 않는다. 역할 getter·page kind·raw LSN·diagnostic stamp만으로 권한을 만들 수 없다.

Status 진입은 health→exact core/Idle→DedicatedFlusher→진행 상태를 검산하고 publication을 먼저 확인한다. C5에서 INIT가 DPT/content를 보유하더라도 unpublished는 lock-free NotInitialized/Unchanged이며 claim/attempt/batch/I/O0이다. 그 뒤 자원/headroom→기존 whole claim/capture 규약을 적용한다. NoDirty는 checked claim 반환 뒤 Ready이고 capture/authority/retired를 남기지 않으며 I/O/batch0이다. local attempt 소비는 기존 규약대로 허용한다.

실제 capture를 retained state에 저장할 때 Status authority를 함께 저장한다. Allocator capture에는 authority가 없다. retained Status에서 authority 누락·변경은 fatal이며 allocator fallback을 하지 않는다. prepare와 execute는 exact core/binding/fence/request/attempt/full stamp/requirement를 다시 비교하고 실제 DurableWal만 수신한다. Prepared의 class와 `stamp.identity + kind + level`은 full TargetRequest와 동등한 봉인이다. 실제 home sync와 whole retirement 후에만 non-Copy RetiredBatch가 만들어진다.

`FlushFinalizePermit::from_retired`는 actual capture와 actual retired token의 정확한 일치를 검산하며 raw constructor를 제공하지 않는다. upper의 retained authority 검산과 pool의 origin 검산도 유지한다. fallible ReadyTransition 검산은 claim 반환 전에 끝나고 그 borrow가 다른 I/O 상태 변경을 막는다. 성공한 checked finalize 뒤 transition.complete와 retained fields 제거는 infallible assignment뿐이다.

| 상태 | 새 named target 요청 | continuation |
| --- | --- | --- |
| Ready | 선택한 published target의 검사·claim/copy 또는 NoDirty | NoPending/Unchanged |
| WaitingWal | Busy/Unchanged, 원 target/progress 유지 | 기존 capture/authority/claim로 같은 requirement 확인부터 재개 |
| RetiredPendingFinalize | Busy/Unchanged, 원 retired proof 유지 | RAM finalize만 재시도; WAL prefix 조회·read·namespace·file I/O0 |
| fatal/fenced | 첫 원인과 retained 소유 보존 | 새 I/O나 정상 소유 반환 없음 |

content/version/pin/DB metadata guard는 WAL force와 namespace/file I/O 전에 반환한다. namespace I/O-port mutex는 기존 허용 범위만 사용하며 그 아래 DB metadata를 획득하지 않는다. 실제 writer와 flusher의 force 소유 경쟁은 같은 stage-issued requirement와 append gate로 직렬화하고 AwaitWalOwner가 중복 I/O를 만들지 않는다.

<a id="home-policy"></a>
## Status의 네 검산 경계

기존 allocator의 kind/image/mapping/home 규칙을 유지하고, sealed Status 경로만 다음을 적용한다.

1. actual published request는 ObjectId2/gen1/kind9/level0, 첫 extent의 page1이며 취소 이력에 따른 정확 incarnation을 보유한다. copied image는 envelope/checksum/identity/shape 검산 뒤 canonical Free generation1/incoming0을 검산하고 pageLSN이 captured requirement와 같아야 한다.
2. 실제 selected FILEMAP root/chunk 전체를 읽고 exact plan 선택·중복·cursor 완료·Live를 검산한다. entry의 allocator metadata identity는 authority의 page0 allocator와 같고 payload는 같은 namespace/file의 page1 범위여야 한다. payload의 object2를 allocator owner로 사용하지 않는다.
3. Status home은 정확16384 zero bytes 또는 이미 검산한 현재 captured image와 동일한 full bytes만 허용한다. nonzero home은 envelope 검산 후 LSN과 전체 bytes를 비교한다. lower/higher LSN, equal-LSN Free generation2의 unequal image, identity/shape/checksum/torn 손상은 거절하고 fence한다. 이것은 repair/reopen 권한이 아니다.
4. execute는 actual DurableWal·Prepared class/full request·same image digest·stored mapping·original origin을 재검산한다. 실제 home을 다시 열어 prepare 때의 FileIdentity와 비교한 뒤에만 쓴다. 권한/association 모순은 새 effect 전에 fatal이다.

BH08/PF07의 coherent trusted-plan 손상은 test-only mechanical 입력이다. not-Live는 IdentityMismatch, wrong extent/16384 mapped length는 canonical FILEMAP codec의 Corrupt, valid empty root는 MissingMapping에 도달한다.1MiB 단위 제약을 통과한 page1 범위 실패라고 잘못 집계하지 않는다. BH09는 disk selector만 변경하여 exact-plan mismatch를 검산한다.

<a id="durability"></a>
## 두 target의 영속 순서와 실패

각 target은 [allocator flush의31효과](dev3-allocator-flush.md)의 같은 issuance Empty→image sync→Pending→home sync→whole retirement Empty를 독립적으로 완료한다. 초기선택(g1,b0)에서 첫 batch는(g2/g3/g4,b1), 다음은(g5/g6/g7,b2)다. control/selector generation과 last-issued batch는 별도 counter다. 기존 selected-only authority, publish/dirsync 불명의 possible old/new union과 fenced live-session의 발급 보존을 유지한다. unselected stage/bank로 crash 뒤 high-water를 복원하지 않는다.

AS는 allocator→Status, SA는 Status→allocator다. 첫 완료는 해당 frame만 Clean으로 만든다. pageLSN/counter는 보존하고 해당 DPT/requirement/recLSN만 제거한다. 다른 target과 동시 redirty는 남는다. reserve@1의 allocator copy 뒤 INIT@2가 설치되면 old finalize는 새 Dirty/counter/requirement와 first recLSN을 지우지 않는다. allocator를 먼저 Clean한 뒤 INIT하면 allocator의 새 recLSN과 Status의 recLSN은 INIT LSN이다.

I/O 전 실패도 capture 뒤라면 원 claim/capture/authority를 보존하고 fatal이다. image/home partial·sync unknown·selector unknown은 실제 bytes, known/possible 선택, attempted/completed, last-issued와 첫 lower cause/effect/certainty를 보존한다. durable retirement 뒤 content Busy는 RetiredPendingFinalize이며 성공한 I/O를 다시 하지 않는다. retirement 뒤 DPT poison은 physical retirement와 Dirty RAM/claim을 모두 남기며 false Clean을 발표하지 않는다.

<a id="resources"></a>
## 자원과 source-value admission

기존 dirty196608/flush65536, global metadata2490368/8388608, workspace4194304와 inner I/O8192를 증액하지 않는다. workspace 사용164000, 잔여4030304다. File cap8192와 네 source/return File carrier는 physical FD4를 뜻하지 않는다. 실제 namespace/WAL/local FD peak는 별도 native 관측 대상이다. 아래 식은 initialized source-value 한도이며 compiler stack/RSS/일반 adapter heap 측정이 아니다.

| 단계 | effect 전 한도 |
| --- | --- |
| consuming adapter | B=old Bound, F=new control, N=new Bound, E=Before/Fatal error, Q=Result. B+8192, B+4F+8192, max(B+E,E+Q)+8192, max(B+N,N+Q)+8192 각각≤65536을 associated const로 강제 |
| generic deep | actual owner+StatusControl+F+4×File+WAL4096+I/O8192+outer capture/finalize8192≤65536 |
| metadata | 기존 registry delta R, T=R+F+1024; hot=T+34×1024+8192+4096+8192≤65536; cold=T+max(5R+8192,3R+8F+8192)≤65536 |
| 새 authority | Authority/Option≤256, Result≤1024, published request Result≤256. 생성 phase5888와 capture/finalize join8080은 각각 outer8192 이내 |
| inner I/O | Status codec3840, scaffolding768, EncodeOuter3072, ExecuteOuter4096. FILEMAP6384, encoding7680, execute6656 등 순차 phase 각각≤8192 |

FILEMAP cursor/codec는 mapping policy 전에 끝나며 opened File은 Status home hash 전에 반환한다. Status envelope와 Free decode는 순차다. encode_prepared는 retained Prepared에 직접 설치하고 unit을 반환한다. execute는 decoder가 없는 phase에서 Retired source/Result forwarding8개를 보수 계상한다. typed Result/Option과 기존/신규 layout table은 별도 phase에서 검사한다. 기존 allocator shared layout 증분도 actual guard에 포함하며 원 측정값으로 .b를 통과 처리하지 않는다. actual layout/aggregate 경계와 독립 D1이 필요하고 실패 시 budget을 자동 증액하지 않는다.

<a id="inventory"></a>
## 필수·최종 inventory

| 분모 | 확정 입력 범위 | 상태 |
| --- | --- | --- |
| 중간 필수43행 | BN6 정상, BA10 권한/조립, BH10 home/mapping, BR6 경쟁/continuation, PF8 실제 fault, RF3 자원/witness | 필수43 완료; source별 strict42+BA01 prefix1 |
| Rust42행 | 위43에서 Python RF03 한 행 제외; 각 named variant와 exact BEGIN/CASE 순서 별도 검산 | source별 선택 완료 |
| RF03 한 행 | 정상 witness와 독립 known-bad12; 제품 mutant12회가 아님 | source별 선택 완료 |
| S5 추가588행 | physical570에서 mandatory와 exact동일6 alias 제외한564 + history24 | 작성, NOT_RUN |
| compiler semantic6 | BC00 healthy consuming/Before reuse/release/resplit, BC01 old-bound move, BC02 second flusher, BC03 private authority, BC04 storage borrow, BC05 raw LSN substitution | semantic6 PASS; 별도 infrastructure baseline은 실패 원본 포함2회 |
| native3 | 새 Linux DB profile1/2/3, actual workflow와 두 home/DWB/WAL full bytes·metadata·FD/effect 관측 | 별도 새 DB3 PASS |

physical570=3profiles×2orders×(31Before+31After+11PrefixError+11Short+11Interrupted)이며 fault는 실제 Status batch에 넣는다. AS는 allocator batch1 뒤 Status batch2, SA는 Status batch1이며 성공한 Short/Interrupted만 다음 allocator batch2까지 완료한다. PF01–05/PF08과 동일한6행은 alias로 보존한다. history24=3profiles×cancel1/2/8/31×AS/SA이며 C0 정상6은 BN이다. 따라서 unique runtime/witness inventory는631=43+588, 원637은 alias 전 합계다. 필수와 확대는 개별 case 선택이 가능하며 중간에 S5를 실행하지 않는다.

필수 aggregate120초, S5 aggregate1200초, compiler선택30초, native DB별30초/24MiB·총72MiB를 고정한다. build/setup은 별도 기록한다. 실제 thread는5초 bounded ACK/release를 사용하며 미도달/timeout을 성공으로 바꾸지 않는다. native의 각 profile 실제 이력·파일/FD census는 worker/harness 입력을 별도로 동결한 뒤 선택한다. native bytes 검사는 current-build recovery 인수의 대체가 아니다.

실행 전 source/binary/fixture/checker/toolchain identity, exact selected ID와 variant/완료 marker를 결합한다. 새 checker/selftest는 위 분모와 별도다. 변경 없는 .a·allocator·하위 wrapper를 반복하지 않으며, 수정 후에는 관련 diff/실패 사례만 재검증하고 원 FAIL·NOT_RUN을 보존한다. 최종 S5의 DEV3 전체 inventory 예외는 [상위 시험 계획](../plans/dev3-test-plan.md#final-suite)을 따른다.

## 결정·근거와 재검토

root는 upper owner/authority/조립·선택 실행, native 담당은 lower I/O와 home/fault 입력, compiler 담당은 독립 literal/harness/compiler/native 입력, 독립 reviewer는 D2/D1을 맡는다. 새 전역 target registry·raw kind dispatch·별도 workspace·기존 .a binder 복제보다 consuming wrapper와 봉인된 두 target 경계를 선택한다.

아래는 로컬 원본 provenance이며 Git export의 실행 결과 링크가 아니다. 채택·소스 검토와 실제 실행 결과를 합치지 않는다. [실행 receipt](../evidence/dev3-status-home-flush-20261022.md)와 [source별 색인](../evidence/dev3-status-home-flush-20261022.json)은 채택 당시 상태와 후속 실제 결과를 구분한다.

| `artifacts/dev3/resume-20261022/` 기준 원본 | SHA-256 |
| --- | --- |
| `status-home-flush-owner-decisions-r1.md` | `a7b72dcba7a6f58c0266431be038013d9ffc633925a3dc985774ad9461f51aac` |
| `review/status-home-flush-d2-acceptance-r1.md` | `7254a39252dbb969347551915d2b76fa5cd25b16562f0013d26fe8a062611554` |
| `status-home-flush-io-source-ledger-r0.md` | `d1b09db8fc34dd756e98d0d6171b7f43043c7221ca021c311efbb89d9d0a9186` |
| `review/status-home-flush-io-ledger-review-r1.md` | `05625867ba5ab68602fe80e884faf99fbab2b9cfcf152cdee0c9a2b4cba7917e` |
| `status-home-flush-upper-source-ledger-r0.md` | `e9732aa9c61a4abfa5c46f20d66994964f4e378c4830a4841779021be724076b` |

Status mutation/새 role 또는 target, 일반 reuse/recovery, batch8/64/multifile, bound/return-carrier 범위·guard·reserved bytes·selector semantics 변경 시 재검토한다. 현재 단위의 작성 완료를 task05·3.03.2 전체·DEV3 완료로 승격하지 않는다.

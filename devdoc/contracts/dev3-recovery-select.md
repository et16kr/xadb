# DEV3 초기 복구 입력 선택 — 3.12.1.a

2026-10-22 · **D3-RECOVERY-SELECT-001 / D2 채택·제한 구현/선택 검증·독립 D1 수용 / 전체 복구 미완료**

[복구 소유 명세](dev3-task-12-recovery.md#early-recovery) · [개발 순서](../plans/dev3-implementation-plan.md#direction-review-adoption) · [시험 계획](../plans/dev3-test-plan.md#recovery-minimum) · [최초 저장 세션](dev3-storage-session.md) · [DWB 설치](dev3-doublewrite-install.md)

<a id="scope"></a>
## 범위와 지원 입력

첫 단위는 기존 디렉터리의 exclusive lease를 새로 취득하고 현재 producer의 **초기 control 선택**을 읽기 전용으로 검산한다. 초기 저장 세션을 재구성하거나 일반 open을 제공하지 않는다. 성공 owner는 선택 사실의 bounded inspection과 Drop만 제공한다. BOOT_RESERVE·DWB repair·WAL 의미 분석·예약 복원·REDO·새 epoch·reader/writer·admission·READY 권한은 없다.

| 입력 | 이번에 허용하는 정확 범위 |
| --- | --- |
| profile | 고정 `d3-profile`의 exact4096·checksum·지원 tuple 전체. ID1/2/3, generation1, timeline1; caller expected namespace가 있으면 함께 비교 |
| XBS1 | `d3-bootstrap` exact4096, 기존 producer의 plan1 allocator-only 또는 plan2 Status bootstrap. 기존 pure preparation/encoding을 공유해 초기 allocation digest·정체성·고정 수치·reserved bytes 전체를 검산 |
| FILEMAP | 기존 최초 설치의 selector generation1·root bank0/generation1·단일 Live extent와 root/chunk. 실제 선택 root/chunk를 기존 `PageFileReader`로 검산한 뒤 canonical 최초 selection과 비교 |
| 물리 geometry | data extent1048576, FILEMAP 양 bank 각각2113536, DWB 양 bank/stage 각각16384, images16711680, reserved8192. 필요한 파일의 실제 named metadata·열린 handle identity/길이를 대조 |
| XDI1 | 최초 flush 전에 설치가 남긴 exact16384 descriptor. `d3-dwb-stage`는 이후 flush가 XDW1으로 덮어쓸 수 있으므로 영구 immutable 파일이 아니다. XBS1에서 검산한 실제 초기 plan으로 기존 `InstallPlan`의 pure preparation/encoding을 공유해 exact digest 비교 |
| DWB | 최초 selector generation1·bank0/generation1·Empty/batch0/maxLSN0. 선택 logical prefix를 기존 codec으로 검산. unselected bank·images의 내용은 읽거나 fallback 근거로 쓰지 않음 |
| WAL/WRITER | `d3-wal-00000001` exact4096의 genesis XWE1과 exact4096 XWS1. 기존 decoder로 profile/epoch 결합 검산. 관측은 read 성공이며 fsync·durable-prefix 증거가 아님 |

지원되는 초기 FILEMAP/DWB도 home page가 정상이라고 보장하지 않는다. 이 단계는 allocator/Status home을 읽지 않으며 초기 allocation digest를 현재 home과 비교하지 않는다. home 검증·복원은 후속 장벽의 의무다.

비어 있지 않은 WAL, Pending 또는 retirement 후 Empty, 진행된 FILEMAP/DWB selector와 미지원 plan은 명시 `UnsupportedClosure` 계열 오류로 보존 거절한다. WRITER의 비초기 epoch는 기존 codec의 `Control(IdentityMismatch)`를 보존한다. WAL header보다 짧은 파일과 fixed container의 잘못된 길이는 `LengthMismatch`다. 검증 가능한 selected control의 손상과 지원 외의 정상 형식을 구별한다. 초기 descriptor와 관계없는 임의 큰 번호·directory 탐색·raw DWB를 대안으로 채택하지 않는다.

이 좁은 입력 선택을 reserve/cancel·Status INIT/flush의 복구 인수로 세지 않는다. tag2 `XSG1`은 legacy `scan_frame_prefix`의 지원 형식이 아니며 runtime before-state validator도 recovery 의미 검증기가 아니다. 실제 producer의 nonempty prefix·혼합 home·Pending/torn·repair 재장애를 연결하는 [3.12.1–4 목표](dev3-task-12-recovery.md#early-recovery)는 그대로 남는다.

<a id="ownership"></a>
## API·수명·오류

제품 내부 `storage::recovery_select`가 `DirectoryPort::acquire_writer`로 얻은 `W: WriterNamespace`를 소유하는 닫힌 `RecoverySelection<W>`를 반환한다. 입력은 directory·선택적 expected namespace·caller의 `[[u8;16384];3]` scratch다. 기존 Fresh/Genesis/Mutable owner를 raw fields로 재구성하지 않는다. scratch는 동기 선택 중에만 빌리며 성공 뒤 재사용할 수 있다. 반환 observation은 owner를 빌린 불변 사실이며 독립 mutable 권한·copyable proof가 아니다.

```rust
impl<W: WriterNamespace> RecoverySelection<W> {
    pub(crate) fn acquire<D: DirectoryPort<Writer = W>>(
        directory: &D,
        expected: Option<Namespace>,
        scratch: &mut [[u8; PAGE_BYTES]; 3],
    ) -> Result<Self, RecoverySelectError>;
    pub(crate) fn observation(&self) -> &RecoverySelectionObservation;
}
```

owner의 `writer`·`observation` fields는 private다. observation도 private fields와 `namespace`, `profile_id`, `bootstrap_plan`, `filemap_generation`, `doublewrite_generation`, `epoch_digest` getter만 가지며 Clone/Copy를 구현하지 않는다. ID/scalar copy와 digest borrow는 진단 사실이고 독립 복구 proof가 아니다.

`into_writer`, 일반 `open_reader`, append, sync, flush, repair 전이와 raw writer/file getter를 제공하지 않는다. lease는 owner가 살아 있는 동안 보존한다. Drop은 파일/lease를 해제하며 write·sync·unlink·할당·숨은 cleanup을 하지 않는다. 실패 뒤 owner는 반환하지 않고 모든 입력 파일을 보존한다. scratch의 부분 결과는 무효다.

오류는 실패 phase, 누적 DB bytes effect, 원래 typed cause를 구별한다. phase는 Lease·Preflight·Profile·FileMap·Bootstrap·Doublewrite·Epoch·Writer의 실제 조립 경계다. Namespace/Io/Control/PageIo 등의 lower cause와 effect/certainty를 지우지 않는다. lease 획득 오류는 LOCK 생성 가능성이 있으므로 lower `NamespaceError.effect`를 유지한다. lease 취득 후 이 단위는 read-only이므로 DB bytes effect는 `Unchanged`다. 성공도 읽은 bytes의 내구성·home 안전성·예약 확보를 뜻하지 않는다.

차단 이름은 정확 `CURRENT`, `d3-current-stage`, `d3-rebase-0`, `d3-rebase-1`, `d3-writer-stage`, `d3-filemap-select-stage`, `d3-dwb-select-stage`의7개다. 각 `metadata`가 존재하면 bytes를 읽지 않고 `Preflight/UnsupportedClosure`로 거절한다. `d3-dwb-stage`는 차단 이름이 아닌 필수 초기 XDI1 입력이다. 이7개 이외의 미참조 orphan 이름은 directory listing 없이 발견했다고 주장하지 않으며 선택·fallback·quota credit의 근거로 쓰지 않는다. 일반 epoch/base closure의 지원·완전 namespace inventory 검산은 후속 의무다.

정체성을 발견하는 profile 외 모든 후손의 기대 namespace/profile은 검증한 profile에서 오며 input 자신의 header에서 복사하지 않는다. 손상 selector의 fallback은 없다. 미지원 거절은 아직 읽지 않은 suffix/target이 정상이라는 판정이 아니다.

<a id="error-order"></a>
### 검사 순서와 첫 오류

`Cause`는 `Capacity`, `UnsupportedClosure`, `UnsupportedBootstrapPlan`, `LengthMismatch`, `IdentityMismatch`, `Namespace`, `Io`, `Control`, `PageIo`와 필요한 기존 `Bootstrap`/`Doublewrite` typed wrapper다. 뒤 두 wrapper가 기존 producer의 effect를 포함하더라도 이번 read-only outer effect로 승계하지 않는다. helper 내부 형식·identity 원인은 보존한다. 여러 위반은 아래 가장 먼저 도달한 경계의 오류로 반환한다.

| 순서·phase | 고정 검사와 판정 |
| --- | --- |
| 1 Preflight | concrete source/type 용량을 lease 전에 검사해 `Capacity/Unchanged`, acquire0. optional expected namespace의 잘못된 값은 `Control(InvalidInput)` |
| 2 Lease | acquire 실패의 원 Namespace cause/effect 보존. 이후 실패의 outer effect는 `Unchanged` |
| 3 Preflight | 위 차단7개를 명시 순서대로 metadata probe. 첫 존재는 `UnsupportedClosure`, lower metadata 실패는 `Namespace` |
| 4 Profile | opened exact length→named identity/length→exact read→기존 profile decode(expected). codec 원 오류 보존; 유효하지만 generation/timeline이1이 아니면 `UnsupportedClosure` |
| 5 FileMap | FILEMAP exact read/decode 후 selector generation1·ref kind/slot/index/generation/length320 검사. 정상 decode한 지원 외 tuple은 target open 전 `UnsupportedClosure`. 기존 `PageFileReader::open`으로 selected root/chunk 검산; 처음 profile/selection과 달라졌으면 `IdentityMismatch` |
| 6 Bootstrap | XBS1 exact4096의 header/version/checksum/namespace를 먼저 검산. checksum 손상이 미지원 plan보다 우선하며 유효 plan3 등은 `UnsupportedBootstrapPlan`. 기존 pure plan1/2 preparation과 전체 descriptor digest·초기 selection 비교; canonical field/hash 불일치는 `Control(Corrupt)`, 초기 선택 정체성 불일치는 `IdentityMismatch`. 현재 allocator home은 읽지 않음 |
| 7 Doublewrite | DWB exact read/decode 후 초기 scalar tuple/length160을 검사하고 진행 tuple은 target open 전 `UnsupportedClosure`. selected bank exact16384의 logical160을 decode; Pending/retired batch은 `UnsupportedClosure`. 잘못된 Empty의 nonzero maxLSN은 기존 codec의 `Control(Corrupt)`. 초기 의미의 canonical selected identity 불일치는 `IdentityMismatch` |
| 8 Doublewrite | stage exact16384 read 후 magic XDW1은 `UnsupportedClosure`; 초기 XDI1 전체 digest 불일치는 `Control(Corrupt)`. 기존 설치 descriptor의4MiB workspace 필드는 그대로 보존 |
| 9 Doublewrite | data·양 FILEMAP/DWB bank·images·reserved의 open/named metadata geometry 검산 후 즉시 Drop, 내용 read0. fixed length 불일치는 `LengthMismatch`, named identity 교체는 `IdentityMismatch`, 누락/port 실패는 원 `Namespace` |
| 10 Epoch | WAL opened/named metadata 비교. length<4096은 `LengthMismatch`, >4096은 **header read 전** `UnsupportedClosure`. exact4096만 `decode_epoch`하며 codec 원 오류 보존 |
| 11 Writer | WRITER exact4096 read와 기존 `decode_writer(profile, epoch)`의 원 오류 보존. 모든 product file을 닫고 owner 반환 |

각 exact-open은 opened length→named identity/length→read 순서를 사용한다. 일반 read 실패는 원 `Io`; `PageFileReader` 경계의 실패는 원 `PageIo`다. nonempty WAL+나쁜 header는 Epoch 도달 시 header를 읽지 않고 Unsupported, 진행 selector+깨진 target은 selector 검산 뒤 target open 전 Unsupported다. 이전 profile/control이 손상됐으면 그 먼저 도달한 오류가 우선한다. Unsupported는 미검산 부분의 정상 판정이 아니다.

기존 `PageFileReader`는 named metadata를 비교하지 않으므로 SELECT 전용 private read-only namespace wrapper가 그 경계를 검사한다. reader의 고정 read set인 profile/FILEMAP exact4096과 양 FILEMAP bank exact2113536의 opened length를 먼저 비교하고 named identity/length를 뒤에 검사한다. 이 wrapper의 열린 handle/named identity·길이 불일치는 `PageIo(Namespace(InvalidType/LimitExceeded))`의 원 lower cause로 보존하고 direct SELECT open의 `IdentityMismatch/LengthMismatch`와 구별한다. 기존 reader의 모든 소비자·시험 trace를 변경하지 않는다. wrapper는 임의 namespace·변이 권한을 내보내지 않는다.

<a id="resources"></a>
## F05 — 이 읽기 전용 단위의 자원

- caller scratch49152 bytes는 기존 maintenance12MiB 안에서 한 번 계상한다. 새 WAL slot·buffer frame·전체 inventory를 할당하지 않는다.
- owner/W·임시 File·names/scalars·순수 preparation의 nested locals·codec/hash call closure는 별도 기존 metadata component65536 bytes 안에 계상한다. scratch49152를 포함한 source/type allowance 상한은114688 bytes다. 기존 component/maintenance 몫을 증액하거나 중복 계상하지 않는다.
- concrete owner와 `W::File`, `PageFileReader` 및 XBS1/XDI1 helper의 동시 생존량을 실행 전 preflight한다. oversized generic port는 첫 read/lease 이전 가능한 경계에서 `Capacity`로 거절한다. helper 추출은 기존 생성 경로의 의미를 바꾸지 않는다.
- 추가 product file handle peak2, namespace lease/LOCK1 및 caller directory1을 구별한다. 성공 반환에는 product file handle0, owner의 lease만 남는다. native FD census는 관측 경계와 source 구조 상한을 구별한다.
- DB write/sync/create/unlink/entropy0이며 lease 자체의 LOCK 효과는 별도다. directory listing·home/전체 cold data scan·DB 크기 비례 allocation은 없다.

이 검산은 전체 recovery profile의 memory/frame/FD/disk 확보나 BOOT_RESERVE가 아니다. foreground 한도 축소·과거 obligation 복원·정상 admitted quota 경계 복구 의무는 [복구 정본](dev3-task-12-recovery.md)에 남긴다. stack/RSS 실측이나64GiB 지원으로 표현하지 않는다.

<a id="inventory"></a>
## 필수 시험과 S5 확대

구현과 함께 exact selector·variant·독립 기대값을 동결한다. 실제 코드 diff에 해당하는 아래 신규 사례만 중간 실행하고 무변경 하위 suite는 반복하지 않는다. 같은 shared pure helper의 실제 수정이 기존 호출에 미치는 경계는 직접 관련 기존 한 사례 또는 새로 작성한 수신 사례로만 확인한다.

| 묶음 | 중간 필수 |
| --- | --- |
| 정상 선택 | profile1/2/3×plan1/2의 literal/실제 producer 초기 closure; observation·정확 selector/namespace·write0·home read0 |
| 미지원 | 실제 producer nonempty WAL, 선택 Pending/retired Empty/진행 selector, CURRENT·미지원 plan/epoch. 입력 보존·정확 원인과 no fallback |
| 손상·geometry | profile·FILEMAP root/chunk/selector·XBS1·XDI1·DWB 선택 control·XWE1/XWS1의 대표 checksum-valid semantic 손상, checksum·namespace·길이·missing/identity 교체 |
| 오류·수명·자원 | 실제 새 open/read/metadata의 fault·partial scratch·모든 handle 반환, lease Busy/Drop/실패, oversized generic W/File, 동시 source/type allowance와 source read set |
| compiler | 별도 control1, positive1과 owner 이동/observation borrow/private constructor/raw writer 부재의 각 의도된 거절. 정상 baseline 뒤 정확 diagnostic 확인; scratch는 성공 뒤 재사용 가능 |
| native | producer와 selector의 별도 PID·새 lease, profile/plan 관측·lease hold/release·source bytes/namespace/inode 보존, actual nonempty producer의 write0 거절과 checksum 손상 대표 거절 |

S5 확대 입력은 별도 선택 가능한 ignored target/variant로 함께 작성한다. profile×plan의 전체 명시 손상 family·fixed length±1·namespace/ref/version/slot 조합과 반복 lease/open/drop을 배정하며, 이번 leaf가 지원하지 않는 WAL replay/repair/BOOT_RESERVE를 작성된 시험이나 PASS로 위장하지 않는다. 최종 DEV3 전체 inventory 전수 예외와 후속 실제 crash/reopen 의무는 유지한다.

selector/집계 도구를 새로 작성하면0건·오타·중복/누락·잘못된 binary·timeout·손상 receipt·mode/cut 미도달의 작은 known-bad를 별도로 검증한다. 실행 전 case/variant 수·fixture/runner/source/binary hash·budget를 동결한다. 원 FAIL을 보존하고 수정된 코드/실패 case만 다시 검증한다.

신규 unit target은 `storage::recovery_select::tests` 아래10개 함수의 필수170variant다. 정상 RP6, 미지원 RU12(상태5+차단7), profile/plan·오류 우선순위 RB8,10family의 checksum/semantic RC20, geometry RG48(필수14file의 length±1/누락42+direct metadata 교체2+direct/wrapper 길이 우선순위2+wrapper metadata 교체2), fault RF61(명시 baseline56연산+추가 short/Interrupted/zero·panic·lease-effect), lease/scratch RL5, concrete/oversized 자원 RQ3, 비선택/home 무독해 RN1, shared helper literal RS6을 exact ID로 선택한다. [runtime 입력](../../crates/engine/tests/fixtures/dev3-recovery-select/runtime/manifest.json)은 실제56연산과 selector·순서·marker·budget를 고정한다. 필수 aggregate120초/S5 aggregate1200초이며 build/setup은 별도다. 성공한 일부만 분모로 삼지 않는다.

필수14file은 `d3-profile`, `d3-bootstrap`, `FILEMAP`, `d3-filemap-0/1`, `d3-data-0000000100000001`, `d3-dwb-stage`, `DWB`, `d3-dwb-0/1`, `d3-dwb-images`, `d3-dwb-reserved`, `d3-wal-00000001`, `WRITER`다. S5 전용은6profile/plan×10손상family×4변형240,14file×length±1×6=168,128회 lease/open/drop×6=768의 합1176 variant다. 중간 필수와 실제 동일 셀의 최종 inventory alias를 명시하고 전체 DEV3의 분모로 혼동하지 않는다.

S5 matrix1176과 mandatory170의 exact alias는 **profile1/plan2**의 RC20+RG01–28, 합48이다. 각 RX의 `mandatory_alias`에 더 강한 동일 typedcause/bytes 검산을 연결한다. S5 matrix/ignored target1176을 유지하며 `s5_matrix_all`1176, `s5_additional`/`s5`1128을 구별한다. 이 단위의 unique runtime1298=mandatory170+S5추가1128이다. 최종 S5에서는 그 최종 source의 mandatory170을 포함해 실행하고 alias48을 같은 최종 실행의 canonical 결과에 연결한다. 중간 PASS의 재사용으로 최종 전수를 대체하거나 alias를 별도 실행 PASS로 집계하지 않는다. `/root/recovery_design_review`가 동일 fixture·mutation·추가 assertion 포함 관계와 매핑을 읽기 검토해 수용했다.

native selector는 `storage::recovery_select::native::worker_recovery_select_native`다. mandatory NP11/12/21/22/31/32(profile×plan6), NW01(actual nonempty reserve/cancel), NC01(profile checksum)의8case를 분리 선택한다. 각30초·합240초, DB당24MiB·retained **database file bytes 합**192MiB·FD5(포함 대상 명시)로 고정한다. source export·ELF·log/receipt는 DB quota 밖의 증거 공간이며 별도 보존한다. producer/selector는 서로 다른 PID며 설치 기능의 새 PASS 분모로 중복 집계하지 않는다. compiler는 infrastructure control1과 semantic5(positive1·owner_move E0382·observation_borrow E0505·private_fields E0451·into_writer_missing E0599), 전체 선택60초로 고정한다. 정확 source/diagnostic line과 runner known-bad는 전용 manifest로 구현과 함께 고정한다.

<a id="decision"></a>
## 결정·F07·현재 상태

| 항목 | 기록 |
| --- | --- |
| 분류·상태 | D2 · ACCEPTED. 기존 형식은 유지하고 내부 SELECT API·정확 지원 범위·실행 입력·자원을 구체화 |
| 권한 | 2026-10-22 사용자의 DEV3 개발 재개. 기존 D2 자율 위임·작성자 외 독립 검토 적용 |
| 기준 | HEAD `fdd7ae61ca6b03417a4d40482d4c32d971a3361d`, 시작 tracked diff empty SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`. [기존 종료 기록](../implementation/dev3-implementation.md#paused-after-status-home-flush)은 역사로 보존 |
| 담당·모델 | 계약/통합 `/root`; 제품·unit `/root/recovery_implementation`, native/compiler `/root/recovery_acceptance`, 읽기 전용 독립 검토 `/root/recovery_design_review`. 요청한 모델은 주 개발 `gpt-6.1-sol`/high, 핵심 검토 `gpt-6-astra`/high. 각 실행의 실제 runtime model/effort는 별도 확인 수단이 없으므로 요청 설정과 검증된 실행 identity를 구별 |
| 전용 경로 | 제품 담당: `crates/engine/src/storage/recovery_select.rs`, `recovery_select_tests.rs`, `wal_io.rs`의 좁은 re-export와 `wal_io/bootstrap.rs`·`wal_io/doublewrite_install.rs`의 최소 pure helper. 인수 담당: `recovery_select_native_tests.rs`, `crates/engine/tests/fixtures/recovery_select`의 compiler/native 입력, `scripts/dev3_recovery_select*.py`와 해당 신규 checker 시험. runtime 입력은 제품 담당과 인수 담당이 별도 경로로 조정 |
| 공유·병합 | root가 `storage/mod.rs`와 문서·색인·계획 연결을 단일 편집. 계약 D2→제품/helper→전용 인수→고정 source export·선택 시험→독립 D1 순서. 두 구현 담당은 같은 파일을 동시에 수정하지 않음 |
| 대안·선택 | 초기 header-only closure로 첫 선택 단위를 닫음. nonempty tag2의 envelope-only 검산을 semantic replay 성공으로 승격하거나 기존 before-state validator를 복구에 재사용하지 않음. pending/실제 변경 prefix의 지원 확장은3.12.1–4에 남김 |
| 검토·실행 | `/root/recovery_design_review`의 D2와 최종 읽기 전용 D1 ACCEPTED, 통합 담당 반영 확인. 차단 이름·XDI1 재사용·오류 우선순위3개 지적과 기존 decoder 원 오류2개 문구를 수정했다. [인수 증거](../evidence/dev3-recovery-select-20261022.md)는 원 필수170·변경 helper4·compiler5+control1·native8·checker15 및 원 FAIL/source를 보존. 전체 복구·BOOT_RESERVE·S5 완료와 구별. 편집중 문서 자동 검사 없음 |
| 재검토 | supported prefix/selector·API 권한·scratch/result 수명·helper 의미·자원/FD 상한·오류/Drop 효과·필수 inventory 변경 또는 새 복구 소비자 도입 |

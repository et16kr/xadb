# DEV3 실제 표현 크기와 자원 계산

2026-10-25 · **표현·수학적 회계 보강 / opcode별 수치 F05·실제 I/O 인수 미완료**

2026-10-26의 [첫 Row Missing 모듈 전환 자원표](#row-missing-module-budget)는 D2 ACCEPTED이며 기존 source의 실행 범위와 구분한다.

[자원 소유 계약](disk-storage-design.md#reserve-accounting) · [고정 profile](disk-storage-design.md#capacity-profile) · [최악 조건 인수](../plans/dev3-implementation-plan.md#resource-proof) · [병렬 소유](../implementation/dev3-implementation.md#wave-2)

이 문서는 이미 구체화한 표현의 bytes를 실제 예약 계산에 연결한다. 숫자가 있는 표현 표만으로 F05가 완료되는 것은 아니다. undo record·catalog·FILEMAP/profile의 표현 크기는 구체화했으나 예약 descriptor·선택 checkpoint와 실제 tree 변경의 page 집합은 남아 있다. 따라서 아래를 admission된 작업의 완료 보장이나 S2 I/O 실행 승인으로 사용하지 않는다. 필요한 항이 미정인 operation은 첫 effect 전에 거절한다.

## 1. 이미 구체화한 입력

| 항목 | 실제 표현 상한과 의미 |
| --- | --- |
| 공통 page | 16,384 bytes; header128, checksum32, slot당4. body 여유16,224 |
| tree page | metadata cell32와 slot4를 제외한 entry 비용 합계16,188. 분할은 개수 대신 실제 bytes로 계산. 높이는 leaf level0부터 internal level15까지 |
| key | composite escaped bytes 최대1,024, 별도 RowId suffix8. row/catalog 정렬 key와 index key의 arity 계약을 구별 |
| row | canonical bytes5..131,072. inline≤2,048이면 row header168+body로 최대2,216. 큰 값의 row cell은 header168+overflow head64=232 |
| row overflow | chunk payload≤16,092, 최대9개 page, chunk metadata128. head64가 root·총 bytes·chunk count·digest를 결합 |
| index presence | 고정160-byte value. key가 같아도 RowId가 다른 entry는 별개이며 undo/논리 버전 의미를 유지 |
| transaction status | 논리 core192의 원래 표현은 최대82 slots. [영속 home](dev3-status-home.md)는 prefix32+core192=224로 최대71 slots이며 incoming count를 포함한다. 영속 recovery obligation descriptor·queue credit은 별도 |
| allocator | extent64 pages=1,048,576 bytes, 첫 page는 metadata, payload63 pages. cell64+64×96=6,208 bytes, 공통 page 전체는16,384 bytes |
| WAL frame | header160+trailer32=192. PageRedo는 각64-byte header+data, 전체≤1,048,576 |
| WAL INIT | full page16,384를 포함하여 subrecord16,448. frame당 최대63개, `192+63×16,448=1,036,416`; 64개면1,052,864로 한도 초과 |
| WAL Patch | 길이1..16,280의 연속 after-image, offset72..16,352 안. identity·pageLSN·checksum 변경은 INIT 또는 별도 page 적용 규칙이 소유 |
| DWB | pool 전체16,777,216. metadata/selector/staging65,536, slot당16,544, slot1,010개, 비대여 잔여2,240. 한 pending batch≤64 |
| DWB batch metadata | control128+88N+checksum32. N64이면5,792. control 공간의 설치 peak는 실제 길이 대신 고정 선할당65,536에 포함 |
| undo root | 고정432+target key≤1,032+inline before≤2,048. Row target key0이므로 row 최대2,480; Index before는presence만 저장하며 최대1,464. 공통 page의 row 최대 root는6개, 최소432-byte root는37개 |
| undo chunk | metadata160+payload≤16,060, 한 page에한cell. canonical before≤131,072라 최대9page; old row overflow와 별도 소유 |
| catalog cell | header232+typed body. table 최대4,588, index 최대368. typed XCB1 occupied before codec은 제한 공급됨. 실제 Catalog effect/CLR 연결은 미완료 |
| 선택 profile·selector | 각각4,096 bytes. FILEMAP/DWB selector와 profile의 전체 digest·generation 결합 |
| FILEMAP | root224+96K, K≤128으로최대12,512; chunk160+96N, N≤64로최대6,304. root16KiB+128chunk×16KiB인고정container2,113,536을두벌예약 |
| 알려진 control 고정 소계 | control 계약의23,130,112 bytes는 FILEMAP·profile·DWB·제안REBASE 공간 소계. checkpoint/obligation/epoch를 포함한 전체 F05가 아님 |

각 입력의 정본은 [page](dev3-page-format.md), [tree](dev3-tree-format.md), [key](dev3-key-format.md), [row](dev3-row-format.md), [overflow](dev3-overflow-format.md), [transaction](dev3-transaction-format.md), [allocator](dev3-allocation-format.md), [WAL](dev3-wal-format.md), [DWB](dev3-doublewrite-format.md), [undo](dev3-undo-format.md), [catalog](dev3-catalog-format.md), [control](dev3-control-format.md)이다. 위 상한들을 더할 때 같은 page·token을 두 번 차감하지 않는다. 반대로 서로 다른 page의 공유를 실제 identity 확인 없이 가정하여 줄이지 않는다.

WAL full INIT63개와 DWB batch64개는 서로 다른 한도다. 64개의 home flush를 하나의 WAL frame으로 제한하거나, DWB64 때문에 WAL64 INIT을 허용하지 않는다. 하나의 transaction은 여러 완결 WAL 그룹을 사용할 수 있지만 각 그룹의 effect/head/count/progress/obligation은 함께 인계되어야 한다.

## 2. allocator inventory 규모의 산술

DS03/PF09의 각 inventory 접근량은 configured buffer의4배 이상이다. 같은 metadata page를 반복 읽은 총 I/O나 zero padding을 독립 inventory bytes로 세어 분모를 늘리지 않는다. 보수적인 산정 기준은 extent마다 실제 encoding된6,208-byte allocator cell 전체다. 이 산술은 실제 fixture의 존재·접근·정답·장치 I/O 측정이 아니다.

| buffer | 필요한 inventory bytes | 최소 extent 수 | 그 extent의 물리 bytes | 실제 cell bytes |
| --- | --- | --- | --- | --- |
| 4 MiB | 16,777,216 | 2,703 | 2,834,300,928 | 16,780,224 |
| 16 MiB | 67,108,864 | 10,811 | 11,336,155,136 | 67,114,688 |
| 64 MiB | 268,435,456 | 43,241 | 45,341,474,816 | 268,440,128 |

8 GiB 전부를 extent에 쓸 수 있다는 비현실적으로 유리한 가정에서도8,192 extents, allocator cell50,855,936 bytes, metadata page134,217,728 bytes뿐이다. 실제로는 WAL/control/DWB처럼 extent 밖에 실현된 bytes도 같은 quota를 사용하므로 extent 수는 더 작다. protected1 GiB는 quota 안의 소유 분할이며 별도의 물리 파일이 아니다. 이미 extent에 포함한 protected bytes를 다시 차감하지 않는다. 따라서 현재 extent 형식에서 기본8 GiB만으로64 MiB buffer의 allocator4배 인수를 만족한다고 주장할 수 없다. 16 MiB profile도 cell 기준으로 기본 quota를 초과한다.

후속 D2 검토 제안은 **제품 기본8 GiB와 그 안의 protected1 GiB를 유지하면서, 대형 inventory 인수에만 owned quota64 GiB의 명시 profile을 추가**하는 것이다. 유지보수 공간·WAL256 MiB·buffer4/16/64 MiB·transaction/operation 한도는 임의 확대하지 않는다. 해당 fixture에서43,241 extents 이상을 실제 소유·접근하고, 나머지 공간에 WAL/control·protected 몫·row/index/locator 작업량이 모두 들어가는 전체 회계를 제출해야 한다. `/root/dev3_supply`가 세 최소 extent 수와 각 n−1 미달, bytes 합계를 별도로 계산하여 확인했다. 제품 기본과 구별한64 GiB 인수 준비 방향을 D2 검토에서 수용했으며, 아직 전체 회계·구현·실행 전이므로 지원 profile이나 실행 PASS로 취급하지 않는다. sparse 파일의 apparent size만으로 실제 allocation inventory나 공간 확보를 입증하지 않는다.

<a id="qualification-profile-preparation"></a>
### 2.1 64GiB 인수 profile의 선행 준비

[방향 리뷰 후속 결정](../plans/dev3-implementation-plan.md#direction-review-adoption)은64GiB를 대형 인수의 준비 목표로 유지하고 준비 완료 조건을 아래처럼 배정한다. 현재 [control의 지원 tuple](dev3-control-format.md)은 ID1/2/3의 owned8GiB이며 이번 문서 변경은 새 ID·tuple·codec 지원을 추가하지 않는다. 기본 profile·protected1GiB·WAL256MiB·메모리 한도를 일괄 확대하지 않는다.

| 산출물·담당 작업 | 완료 조건·시점 |
| --- | --- |
| profile/배치 설계 · 3.01/3.03, 3.16 취합 | 실제 성장 경로를 동결하기 전에 새 checksummed profile tuple·지원/거절·fresh fixture와 file/extent 배치를 D2 검토한다. FILEMAP 최대8,192 mapping은 file 수이며 extent 수가 아니다. 여러 extent를 한 file에 배치할 수 있는 기존 표현을 먼저 검토하고43,241 files나 형식 증액을 선결하지 않음 |
| 전체 수치 회계 · 각 opcode owner, 3.16 취합 | fixture 생성 전43,241개 이상 실제 extent의45,341,474,816 bytes에 extent 밖 WAL/control/DWB·old/new 선택 closure·staging·recovery peak를 더해64GiB 안의 charged+outstanding을 계산한다. protected는 그 안의 분할이고 page token은 실현 extent와 중복 차감하지 않는다. row/index/locator4배 집합의 실제 배치를 함께 증명하며 공간이 겹친다는 가정으로 소계를 누락하지 않음 |
| 표현·workspace 점검 · 3.03/3.12 | profile을 소비하는 codec/checked 산술과 기존8GiB에서 유도된 growth/intent inventory·고정 배열·bootstrap scan 상한을 추적한다. 특히 현재 growth workspace의8,192-entry bound를64GiB 지원으로 간주하지 않는다. 필요 변경은 기존 memory budget 안의 알고리즘·예약표와 함께 검토하고 실제 diff만 검증 |
| 실행 환경·fixture 계획 · 3.16 | S5 준비 완료 이전에 장치의 실제 가용 공간·fixture/원본/장애 clone 동시 소유·생성/복구 시간 예산·정리 소유를 기록한다. 생성기는 구현과 함께 작은 정상/known-bad로 검증하고 실제 대형 생성·접근·성능 실행은 S5에 배정한다. sparse apparent size·반복 읽기로 인수량을 대체하지 않음 |

64GiB 전체 회계가 성립하지 않으면 quota 숫자만 올리거나4배 분모를 줄이지 않고 배치·알고리즘·profile을 다시 D2 검토한다. 각 산출물은 현재 미공급이며 profile 지원·대형 fixture 존재·PF09 PASS와 구별한다. 이 준비는3.16 말미의 일괄 작업이 아니라 growth/recovery/fixture 소비자의 선행이다.

## 3. opcode별로 남은 동결

최종 표는 user row insert/update/delete, 각 index presence 변경, catalog DDL, tree split/root/empty-leaf 처리, undo link/cut/release, allocation reserve/INIT/free, commit/abort-end, CLR, checkpoint/capture/rebase/epoch 설치, DWB flush/repair/retirement를 각각 구분한다.

각 행에는 다음을 실제 codec와 알고리즘에 연결한다.

- 입력 범위와 논리 statement/transaction 누적 한도.
- forward와 전체 abort에 필요한 page 종류별 개수, 기존 token과 신규 extent 성장, 구조 변경 그룹의 분해.
- 모든 그룹의 WAL header·payload·checksum bytes와 redo-only CLR·abort-end 비용.
- 적용 전체 pin 집합, latch 밖 탐색과 rollback 재탐색의 고정 workspace, transient reference pin·lock/ATT/FD slots.
- forward/CLR/statement rollback/terminal/restart 단계의 소비·잔여 예약·실제 반환. retained WAL·회수 전 undo·SMO·거래 잠금은 논리 rollback으로 되감지 않음.
- 4/16/64 MiB buffer에서 foreground와 flush/rollback/recovery의 분리된 몫, 최대32 요청 중 실제 동시 admission 수.
- 선택 control의 고정 bootstrap peak, checkpoint 두 슬롯과 old/new 합집합 closure, 새 epoch 직후 재장애에서도 추가 누적 없는 인계.

하나의 group이63개 full page redo로 표현되지 않는다면 형식을 초과한 frame을 쓰지 않고, 중간 상태도 복구 가능한 system/transaction 그룹으로 나누는 알고리즘을 먼저 검토한다. 단지 encoded-size 오류를 반환하는 것으로 이미 effect를 시작한 작업의 rollback 진행 보장을 대신하지 않는다. F05 완료에는 이 구체 표, 경계−1/동일/+1의 독립 계산, 실제 I/O·thread·fault 결과가 필요하다.


## Genesis WAL component의 metadata 분할

[별도 D2](dev3-wal-io.md)로 기존 metadata/cache8MiB 안에 WAL slot1MiB와 component metadata/설치 scratch64KiB를 고정 배정한다. locator1MiB와 겹치지 않으며 나머지는6,225,920 bytes(5.9375MiB)다. system16MiB의 flush4MiB/maintenance12MiB는 유지한다. 실제 component bounds를 검증해야 하며 이 배정만으로 frame headers/ATT/FD 등 전체 engine ledger나 F05를 충족했다고 하지 않는다.

## 최초 저장 세션의 후속 D2 계약

[최초 저장 세션의 자원 계약](dev3-storage-session.md)은 이 한정 operation에 fixed5,296,128/max273,727,488/protected4,247,552 logical bytes, caller48KiB와 phase별 fixed local 최대64KiB를 고정했다. readback의 추가File2개를 모두 포함하며 source/type ledger·실제 handle 계측은 아직 NOT_RUN이다. global F05/admission·DWB/checkpoint/복구 예약의 미완료 의무는 유지한다.

## 최초 DWB 설치의 제한 회계

[설치 계약](dev3-doublewrite-install.md)은 기존 Fresh에 DWB16,777,216bytes를 한 번 더하여 fixed22,073,344/max290,504,704/protected21,024,768을 고정한다. 첫 실제 file 총량22,069,248과 미래 selector 교체4096 예약은 구별한다. caller4MiB는 system flush 몫이며 metadata component64KiB는 retained Fresh를 포함해 재사용한다. caller directory는 component4FD 밖의 추가1FD다. D2 산술 채택이며 실제 type/handle·전체 F05 인수는 미완료다.

<a id="resource-vector-proof"></a>
## 비교 분석 반영 — opcode 자원 벡터와 진행 증명

[DEV3-REVIEW-011](disk-storage-design.md#task-review-011)은 기존 한도를 유지하고 F05 제출 형식을 강화한다. 아래 산식 채택은 개별 opcode의 수치 동결·구현·실행 완료가 아니다.

자원 성분 r마다 capacity C_r, 이미 실현·점유한 A_r, 기존 owner의 미소비 예약 R_r, 새 opcode의 **동시 peak와 종료 의무를 포함한** 추가 예약 b_r을 둔다. admission은 모든 성분에서 `A_r+R_r+b_r ≤ C_r`일 때만 가능하다. 이질 거래는 `Σ b_r(op_i)`를 사용한다. 기존 거래 slot 점유를 n_active라 하면 동일 상한 신규 작업의 보수적 추가 admission 수는 `n ≤ min(32−n_active, min_r floor((C_r−A_r−R_r)/b_r))`이며 b_r=0인 성분은 제외한다. 처음부터 빈 상태에서는 n_active=0이다. 공유 scratch는 동시 보유 불가를 증명한 경우만 한 번 계산한다.

32는 거래 개수 상한이다. private 총64MiB에서 각8MiB를 예약하면 최악 동시 admission은8개 이하이고, 32개 허용에는 평균 예약2MiB 이하와 모든 다른 성분의 여유가 필요하다. 최대 거래 수를 최대 크기 거래의 동시 성공 보장으로 쓰지 않는다. 기본 owned8GiB·protected1GiB에는 `protected ⊆ owned`와 별도 protected 하위 한도를 모두 적용한다. 실현 extent의 disk bytes와 그 안의 page token은 disk 차감에서 중복 계산하지 않지만 사용 가능한 page 수 제약은 따로 검사한다.

| opcode별 필수 열 | 제출할 내용 |
| --- | --- |
| 입력·경로 | 최소/최대 row·key·index 수, tree 높이·split/overflow, 성공·실패 statement·full abort·CLR·terminal·restart 경로 |
| 실제 자원 벡터 | owned/protected/WAL bytes, 기존/신규 page token, 동시 pin/frame, private/metadata/flush/maintenance bytes, locks·FD·owner slots. 각 항의 codec·source와 peak 시점 |
| 그룹·전이 | 그룹별 forward/보상 WAL와 page 집합, 기존 token→신규 실현, charged 증가 δ_r·확인 반환 d_r·잔여 예약 r_old→r_new |
| 보존식 | 각 전이에서 `δ_r+r_new ≤ r_old+d_r`; 다른 owner 예약을 침범하지 않음. 재admission은 아직 시작하지 않은 선택적 새 forward/statement에 한정. 기존 작업 종료에 필수인 CLR·rollback·terminal·recovery·cleanup은 기존 보존 예약 안에서 완료하고 새 admission 성공에 의존하지 않음 |
| crash 인계 | 같은 durable prefix의 descriptor·실현 소유·잔여 의무를 한 번 복원. restart 횟수에 비례한 중복 예약/새 파일 증가 금지 |
| 진행 근거 | 마지막 crash 뒤 유한 의무·충분한 무장애 시간·공정 turn을 전제로 감소 rank 또는 유한 fair-cycle 증명. 계속되는 새 admission을 닫는 조건과 기존 rollback/flush/cleanup의 독립 몫 |
| 경계·계측 | 독립 계산의 한도−1/동일/+1, 허용 동시 수, 실제 peak와 잔여 의무. 작은 구현 동반 인수와 S5 확대를 별도 선택 |

`δ`는 새 실현량, `d`는 별도로 확인한 실제 반환량이다. unlink/dirsync 불명·열린 inode·미완료 DWB retirement는 d=0으로 보수 처리한다. 논리 rollback이 성공해도 이미 소비한 WAL/CLR·회수 전 Undo/overflow·잠금은 A에 남는다. rollback 전 ledger snapshot으로 돌아가지 않는다. 안전한 상한 보존만으로 종료 진행성이 증명되지는 않는다. 첫 행 opcode에도 이 표를 요구하며 누락된 항을 평균값·0·최종 qualification 작업으로 미루지 않는다.

<a id="representation-cost"></a>
## 비교 분석 반영 — 표현 비용의 측정 기준선

현재 leaf entry 공간16,188에서 row canonical bytes r가 inline일 때 비용은 `4(slot)+4(key/value lengths)+8(key)+168(header)+r=184+r`, leaf 수용량은 `floor(16188/(184+r))`다. r=5/64/2048에서 각각85/65/7행이고 이상적 가득 찬 leaf의 page당 사용자 payload 비율은2.59%/25.39%/87.50%다. index physical key K는 RowId suffix를 포함하며 entry 비용168+K, K=16/64/1032에서87/69/13개다. internal fanout은 `1+floor(16188/(32+K))`로338/169/16이다.

r=64와 index8개·각K=64인 가득 찬 leaf 근사에서 `16384/65+8×16384/69=2151.6613 bytes/row`다. 이는 사용자64 bytes의33.6197배이나 internal/allocator/Undo/WAL·빈 공간을 제외한 근사이며 실제 DB 크기나 타 DBMS 대비 배율이 아니다. 128KiB row의 old/new overflow와 before Undo가 각각9page이면 일시 공존27page=432KiB, 그중 새 row+before만 신규라면18page=288KiB다. root/SMO/추가 WAL·예약은 별도다.

PF09는 실제 allocated/reserved/retained bytes를 row·index·overflow·Undo·WAL·allocator/control별로 분리하고 이상적 leaf 산식과 차이를 설명한다. key가 변하지 않는 UPDATE의 index mutation 생략은 기존 계약을 유지한다. 압축·metadata 축소·형식 변경은 측정 후 별도 D2이며 이번에 채택하지 않는다.

<a id="row-missing-module-budget"></a>
## 모듈 전환 — 첫 Row Missing의 자원 배정

2026-10-26 · `D3-ROW-RESOURCE-001` · **D2 ACCEPTED / M2 완료 / runtime F05 NOT_RUN**. 작성·자원 담당 `/root`. [최종 독립 검토](../evidence/module-independence-survey-20261026.md#final-design-review)를 수신해 설계 예약을 채택했다. [소비자 profile·opcode 산식](dev3-module-consumers.md#row-shape)의 single RowLeaf·inline·index0·Missing INSERT에 한정한다. 이 절은 새 source의 실제 생존량을 측정한 결과가 아니며, 제품 구현과 함께 검산할 설계 예약이다. 범위 밖 일반 CRUD/SMO·복구를 이 수치로 admission하지 않는다.

### page·private loan의 상계

변경 page의 최대는 Forward/CLR/Prune의5개다. 실제 apply 집합에는 원 Root, 해당 RootJob page, Status, U, RowLeaf와 allocator·tableAnchor·TerminalJob page·previous H proof를 포함해 최대9개를 예약한다. 같은 physical identity는 원 pin 집합에서 중복 제거하되 의미 검산을 생략하지 않는다. RootJob 전체 metadata census는 고정 bank64행을 stream하고 모든 U page를 동시에 pin하지 않는다. 물리 target/H proof의 exact 준비·stage 수명은 경계 API의 검토 항목이다.

first Row 전용 private backing은 **425984B**이며 다음처럼 배분한다. 현재 Catalog 크기와 같다는 사실이 근거가 아니라, 이번 Row의 독립 상한이 이 고정 loan 안에 드는지를 기준으로 삼는다.

| 구간 | bytes | 소유·동시 생존 |
| --- | ---: | --- |
| `[0,98304)` frame |98304|최대 Row frame94080B를 보존. Pending에는 immutable frame/plan으로 인계 |
| `[98304,245760)` before/proof9page |147456|원 target의 stable before·변하지 않는 proof를 담는 최대9page. pool frame과 다른 private copy이므로 별도 회계 |
| `[245760,327680)` after5page |81920|실제 변경 최대5page. pin9라고 after9page를 생성하지 않음 |
| `[327680,344064)` row candidate |16384|tree cell 최대2228B 및 canonical row 후보. A가 생산하고 observer가 독립 검산 |
| `[344064,360448)` table/schema copy |16384|실제 Catalog에서 받은 qualified table/schema의 최대4588B와 검증된 문맥. RAM-only table 대용 아님 |
| `[360448,425984)` 미사용 |65536|다른 원 owner의 비용을 숨기지 않음. 추가 사용 시 이름·수명·선예약을 다시 고정 |

2026-10-29 개정(S2-0, [Row 쓰기 개정](dev3-module-boundaries.md#row-write-budget)): Row 쓰기(S2)는 위 미사용 65536B를 다음처럼 이름·수명·선예약을 고정해 쓴다. 선예약은 caller가 이미 보유한 이 loan 자체이며 새 할당·owner·authority는 없다. 한 구간을 두 사용자가 동시에 쓰지 않는다.
- `[360448,376832)` XOI page 16384B: Row layout의 Reserve·Forward·Cancel·CLR 준비에서 XOI inventory projection을 둔다. 한 C 단계 안에서만 산다. Forward frame 최대93660B의 tail 4644B가 page 하나보다 작기 때문이다.
- `[376832,393216)` C scratch 16384B: C의 `produce_borrowed`·`observe_borrowed` scratch(9076B)다. 한 C 단계 안에서만 살고 진입 때 0이다.
- `[393216,425984)` A scratch 32768B: `RowProducerLane`의 8번째 part다. lane borrow 동안만 A가 쓰며, 발급 전 C가 0으로 채우고 반환 뒤에는 읽지 않는다. byte만 담고 Rust reference 배열은 아래 규칙대로 두지 않는다.

frame 구간 `[0,98304)`은 Row 쓰기의 preflight 안에서 retention LOAD scratch 6page로 먼저 쓰이고, Reserve 이후의 frame 생산이 이어 쓴다. 두 사용은 시간상 겹치지 않는다. Catalog layout과 Row lifecycle의 기존 배치, B 읽기 loan의 분할은 바뀌지 않는다.

위 raw byte loan에 Rust reference 배열을 unsafe cast하지 않는다. 유효 Row entry는 최소189B이므로 한 leaf의 실제 행은≤85다. metadata1+삽입 후보1을 포함한 **87개 borrowed cell reference**로 `LeafEditPlan`을 구성할 수 있다. 현재64-bit target에서 slice reference16B이면1392B다. 이 typed scratch와 view/Result/return carrier는 raw loan과 별도로 source-value census에 포함한다. 적용 전 모든 기존 row의 canonical shape를 검산하여 최소 크기 전제가 깨진 입력은 손상으로 거절한다. generic tree의1013-reference scratch16208B를 작은 Row profile에 자동 적용하거나, 표현 검산 없이 작은 배열을 사용하는 두 대안을 모두 피한다.

B의 읽기 loan은 기존 실제 loader에 필요한 disjoint prefix `[0,409600)`와 copy tail `[409600,425984)`를 유지하는 후보로 정한다. tail에는 현재 row record 최대2216B와 qualified schema 최대4588B가 함께 들어가며 loader가 이를 덮지 않는다. B는 A의 candidate/after loan을 공유하지 않는다. 호출별 allocation 대신 caller가 선예약한 loan을 사용한다. A와 B가 동시에 소유하면 각각 합산하고, 원 단일 maintenance operation만 상호 배타성이 증명된 scratch의 max를 사용한다.

Row의 활성 CLR/Structural/Terminal/Status row와 Aging의 남은 step에는 **`P_R=(frames9,memory425984,locks0,registry16,files5,doublewrite1)`**을 보수적인 step recipe로 제안한다. inactive row는0이며 기존 System scope1과 Catalog scope2 recipe는 바꾸지 않는다. 단순히 기존 CREATE_PEAK를 복사한 값이 아니며 각 항의 근거는 다음과 같다.

| 항목 | 제안 상계와 실제 근거 |
| --- | --- |
| frames9·memory425984 | 위 whole-set9와 Row frame/before/after/candidate/schema의 disjoint 배치. 유지보수는 순차 step의 max, 동시 foreground loan은 합산 |
| locks0 | [격리 owner](dev3-task-10-concurrency.md#module-row-isolation)의 Missing-only 배타권은 actual sealed RootJob+비재사용 RowId로 표현한다. 별도 logical lock-map/request/waiter **slot**은0이며 논리 X권한이 없다는 뜻이 아니다. root/terminal queue≤55와 실제 native metadata를 따로 청구하고, successful/Compensated root의 terminal 전 해제를 금지 |
| registry16 | 한 step의 보수 합은 pool target9+source reference2+snapshot handoff old/new2+user admission1+isolation actor1+retirement intent1. 이 중 배타적인 phase도 합으로 둔다. 공용 epoch32 및 원 registry backing은 고정 metadata로 별도 청구. 이 숫자가 기존 registry별 slot 상한을 늘리는 권한은 아님 |
| files5 | 원 WAL handle1+선택/load/replay의 temporary File≤2+native namespace directory/LOCK2. Row 후보/epoch/read-set은 새 File을 열지 않는다. 원 단일 reader lease와 단계별 handle 반환을 유지. 동시에 별도 flusher의 temporary≤2가 살아 있으면 원 shared handles를 중복 세지 않고 **세션 전체≤7**로 검산하며 process 관측용 FD는 따로 기록 |
| doublewrite1 | 원 `RetentionFlushControl`의 실제 single captured page/claim. 여러 dirty target은 원 flusher에서 순차 처리하며9개 동시 DWB slot을 요구하지 않는다. buffer frame과 DWB backing은 별도 원 소유 |

files5는 단일 step recipe이며 병렬 lane 전체가5라는 주장이 아니다. reader·flusher·replay의 actual open/close 중첩과 recovery exclusive 단계는 변경 source의 caller census에서 위 상계와 맞춰야 한다. 초과하면 첫 효과 전에 거절하고 source 수명/설계를 고치며 정적 숫자만 확대하지 않는다.

### 격리 장부·장기 handle의 metadata

[격리 공급](dev3-task-10-concurrency.md#module-row-isolation)의 epoch1600B와 isolation4160B, 합계**5760B**를 원 metadata/cache8MiB에서 disjoint loan으로 확보한다. 기존 visibility8192B와 ATT padding은 그대로다. actual source의 고정 ledger/registry subtotal은 `36×1664+8192=68096B`이고 새 backing 포함**73856B**다. Root/Job/page 데이터 backing과 source-value를 혼동하지 않는다.

2026-10-29 판정(S2-0, Q12): `VisibilitySourceCensus.backing`은 visibility 8192B와 epoch 1600B를 함께 빌린 loan이므로 9792B다. metadata 합계는 `VISIBILITY_BYTES`와 `EPOCH_BYTES`를 따로 더하고 backing을 읽지 않으므로 epoch는 한 번만 센다. 따라서 위 73856B와 "기존 visibility8192B 그대로"는 맞다. `catalog_source_*`의 기대값 8192는 epoch loan 이전 값이므로 9792로 고친다. 9792와 metadata 항등식은 이미 `owner_tests`가 단언하므로 정정은 그 기대값 하나다. 이는 시험 기대값 정정이며 cap이나 의미의 변경이 아니다. 코드 변경은 별도 단위가 한다.

capsule·statement·cursor처럼 호출 뒤에도 사는 engine handle에는 **전체49152B**의 source envelope를 별도 배정한다. 원 isolation header/actor slot의 checked byte ledger가 이 비용을 추적한다. 각 class의 고정 요금은512B이며 실제 type·생성/반환 carrier와 필요한 caller 동시 보유가 그 안에 드는지 source census로 확인하고, token/source guard/effect를 만들기 전에 예약한다. 32×3×512=49152는 배정 산술이지32개 최대 거래의 동시 성공 보장이 아니다. 여러 cursor도 각각512B를 청구하며 Clone으로 무료 복제하지 않는다. 한 거래 한 cursor라는 기능 제한으로 대체하지 않는다. view/cursor가 transaction을 borrow하므로 terminal에서 capsule을 소비하기 전에 정리되며 반환은 원 slot/generation에 한 번만 적용한다. 이 byte credit은 저장 권한을 생성하지 않는다.

현재 [C1 read-only capsule/view 공급](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)은 장기 ledger와 각512B 요금을 구현했다. 생성·반환 carrier를 포함한 transaction400B/view328B, live metadata187082B/whole252618B로 기존 한도 안이다. 이는 해당 source의 정적 회계와 선택 lifecycle 결과이며 cursor·short aggregate admission·병렬 reader 합의 완료를 뜻하지 않는다.

짧은 operation·Result·error·return 임시값은 기존 component source census에 포함하며 각각 별도 runtime credit 계층을 만들지 않는다. generic `W::File`·owner/controller·publication/terminal receipt·typed scratch87·fixture receiver의 실제 반환 복사도 누락하지 않는다. borrowing으로 큰 배열 복사를 줄이는 것과 이미 살아 있는 caller 값의 회계를 생략하는 것은 구분한다.

여기서 **65536B는 허용한 writer/reader의 모든 짧은 실행·retained source·caller carrier의 동시 합**이다. 원 `active.max(...)`를 그대로 병렬 전체 합으로 사용하지 않는다. source 공통 retained는 한 번, 동시에 살아 있는 operation charge는 합으로 세고, max는 실제 배타적인 phase에만 적용한다. C의 원 admission 입구에서 source census로 동결한 operation별 nested peak를 선예약하고 출구에서 반환한다. source guard/pin/효과를 보유한 채 부족한 credit을 기다리지 않는다. 예약된 rollback/terminal은 원 progress admission으로 신규 foreground를 닫고 이미 실행 중인 유한 reader가 반환한 뒤 전용 source 몫을 사용하므로 새 선택적 foreground admission에 의존하지 않는다. 장기 snapshot/cursor를 강제 해제하여 이 공간을 만들지 않는다. flusher의 별도65536B는 이 합과 disjoint이며 기존 준비 중 flush 허용을 유지한다.

2026-10-29 적용(S2-0, [Row 쓰기 개정](dev3-module-boundaries.md#row-write-budget)): Row 쓰기의 caller carrier는 A driver가 각 C 호출과 producer 호출에서 보유한 값이다. 측정 Forward producer phase 60625B(S1-e 뒤 60745B)는 이 합으로 계산한 값이며, 한도를 올리거나 caller를 빼는 대신 A slimming과 private A scratch로 줄인다. 진행 class writer와 reader는 terminal read gate로 배타이므로 그 구간의 동시 합은 shared에 writer charge와 admitted reader 합 가운데 큰 값을 더한 것이다. gate 대기 중 writer 진입 floor는 별도의 미공급 의무다.

2026-10-30 개정(S2-0b, [replay 검산 재배치](dev3-module-boundaries.md#row-write-validate-once)): Row 쓰기의 caller carrier 상한은 preflight 호출 지점(6144B)과 나머지 C 호출 지점(11520B)으로 나뉜다. 두 상한 모두 그 호출 지점의 A 몫 전체를 class 안에서 세며 caller를 빼지 않는다. RowWrite C 본문이 줄어든 까닭은 한 행의 replay 검산을 preflight 한 번으로 모은 것과, group carrier를 가진 단계의 carrier 계수 정정(중단 호출 단계마다의 반환 carrier)이다. 이 정본의 한도·`CatalogProgressCharge`·private 425984B 배치는 바뀌지 않는다.

위 동시 합≤65536B를 만족할 때 `metadata≤73856+65536+49152=188544B≤196608B`, 원 flush component65536을 더한 `whole≤254080B≤262144B`다. 남는8064B를 미등록 상태·새 예약의 여유로 사용하지 않는다. 총 metadata8MiB·private8MiB/transaction 및 전체64MiB·flush4MiB·maintenance12MiB도 유지한다. 이는 새 **설계 상계**이며 실제 size_of/caller 동시 생존량·기존 visibility/controller 세부 cap은 M4의 변경 source에서 검사해야 한다. source census 실패를 한도 증액·검사 생략으로 해결하지 않는다.

### WAL·남은 종료 의무

cell 크기 `C=185..2228`, predecessor 존재 `h∈{0,1}`에서 Forward는`91304+C+128h`(최대93660), CLR은`91096+C+128h`(최대93452), Prune는`89496+2C+128h`(최대94080)다. RowReserve57336·Cancel40000·Begin/Commit/AbortEnd56712·RollbackStart/실패 StatementFinish3040·ReleaseRole/RootFree55528·TerminalFinish55112를 같은 profile의 exact group bytes로 제안한다. 원 native/edge/count/redo 항의 정의는 소비자 계약 한 곳에 둔다.

거래의 mandatory remaining은 live root마다 CLR 비용의 합, terminal56712, full-abort RollbackStart3040이다. 진행 중 Reserved row에는 Forward와 그 후 CLR 몫도 포함한다. active mutating statement에는 **별도로6080=statement RollbackStart3040+StatementFinish3040**을 첫 효과 전에 확보한다. full-abort3040을 여기에 중복 배정하지 않는다.

- 성공 statement는 WAL0으로 view/mark를 종료하며 미소비6080을 다음 statement에 인계할 수 있다. 새 행의 Reserve/Forward/Undo/cleanup 비용은 매번 별도 admission이다.
- 실패 statement는6080과 그 suffix의 CLR 합을 소비한다. 이전 성공 statement의 CLR 합·full-abort3040·terminal56712를 남긴다. 다음 선택적 statement의6080 보충이 거절돼도 기존 full abort는 새 admission 없이 끝나야 한다.
- Cancel은 이번 미사용 Forward/CLR·root cleanup 예약을 반환하고 이미 consumed Reserve·발급 high-water를 되감지 않는다. statement가 앞서 변경한 행의 의무는 그대로다.
- Commit은 원 terminal 비용을 소비하고 unused full-abort/CLR/statement 몫을 반환하지만 Aging의 retained cleanup은 보존한다. full abort는 실제 CLR·종료 소비를 기록하고 미소비 statement-finish 몫만 반환한다.

root별 Aging의 complete cleanup 상계는 `Prune+3×ReleaseRole+RootFree =311608+2C+128h`(최대316192)다. compensated root는 불필요한 Prune 몫을 반환하고4step222112를 남긴다. TerminalFinish55112는 actor당 한 번 별도로 합산한다. 서로 다른 root의 WAL/step 합과 단일 Aging의 sequential scratch max를 구별한다. 실패 statement의 Compensated root가 남은 거래가 이후 Commit할 수 있으므로 그 root q0와 source Commit q를 같게 강제하지 않는다. TerminalFinish는 disposition과 관계없이 해당 actor의 RootJobs0·실제 참조/Status 종료 조건을 요구한다.

각 adopted group에서 `δ_r+r_new ≤ r_old+d_r`를 실제 before/after descriptor와 검산한다. 시작된 CLR·terminal·recovery에 새 admission을 요구하지 않는다. crash 뒤 같은 descriptor/소유를 한 번 복원하고, 이미 보상된 suffix는 CLR로 다시 undo하지 않는다. 실제 failed statement 뒤 새 statement 재admission·full abort 분기를 별도 경계 입력으로 둔다.

### 유한 inventory와 진행

원 queue64행·nonfree≤55, first extent의 dynamic55page, 실제 table pair·Status·Undo 및 다른 Reserved/allocated 소유를 원 census로 계산한다. 한 table/한 actor/다른 점유0일 때도 Undo root52는 물리적인 예시 상계이며 사용 가능한 grant가 아니다. leaf 남은 bytes·queue slot·Free Undo generation·동시 frame/loan·WAL/owned/protected 가운데 가장 작은 여유가 신규 admission을 닫는다. 예컨대4MiB buffer는256frame이므로32×9=288을 모든 거래의 동시 성공 약속으로 사용할 수 없다. 기존 flush/rollback 몫을 빼고 실제 허용 owner 수를 계산한다.

Free Undo container가 모자라면 원 System INIT 공급으로 실제 확보한 뒤 RowReserve를 허용한다. 그 INIT의 원 비용·소유·선택 증거를 별도로 포함하며 “기존 Free page”를 fixture에서 raw 생성하지 않는다. CLR은 Missing 행 제거이므로 새 tree page/overflow를 요구하지 않는다. 감소하는 남은 un-compensated roots와 root별 release cursor, 원 bounded fair queue로 종료 진행을 설명하고 reader pin/오래된 snapshot의 해제를 임의로 강제하지 않는다.

M4의 static/compiler 검산에서 source·loan/FD·queue/전체 target 상계를 확인한 뒤만 해당 effect/native 시험을 진행한다. M5에는 독립 계산으로 실제 각 자원−1/동일/+1, 실패 statement의6080 소진 후 신규 거절과 기존 abort 완료, H net0에서 before count 부족, maximum valid leaf와 필요한 split 거절, 반복 crash 예약 비증가를 선택한다. runtime F05와 전체 DEV3 S5의 나머지 의무는 아직 미완료다.


[C2 동기 Catalog 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)의 source census는 private operation·제품 selection·생성/반환 carrier를 포함한다. decode/check 임시 view가 끝난 뒤 location/LOAD가 시작하므로 nested phase를 구별하며, r2의65770B 상한 초과 FAIL과 정정 근거를 보존한다. 독립 검토가 LOAD 뒤까지 살아 있는 header192B를 복원하도록 지적한 후 최종 lookup42858B, live peak64074/metadata187082/whole252618B와 recovery peak65450B다. 새 caller-retained operation을 공개하지 않았으며 statement entry와 short aggregate admission은 후속 공급이다. 이 결과를 동시 writer/reader 합의 인수로 사용하지 않는다.

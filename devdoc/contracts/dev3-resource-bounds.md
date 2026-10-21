# DEV3 실제 표현 크기와 자원 계산

2026-10-21 · **F05 준비 계산 / opcode·실제 I/O 인수 미완료**

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
| catalog cell | header232+typed body. table 최대4,588, index 최대368. catalog occupied before body는 undo codec에 미연결 |
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

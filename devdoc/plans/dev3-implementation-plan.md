# DEV3 — 구현·병렬 개발·검증 계획

2026-10-18 · **D2 기본 설계 ACCEPTED / production 구현 미착수 / S1·제품 실행 NOT_RUN**

[상세 설계](../contracts/disk-storage-design.md) · [실제 소스 조사](../evidence/dev3-source-research.md) · [단계 완료 정본](stage-completion.json) · [DEV2 선행 계획](dev2-implementation-plan.md)

<a id="entry"></a>
## 1. 착수와 완료의 구분

이 작업은 DEV3 설계·소스 조사·시험 의무 구체화다. DEV2가 아직 부분 구현이어도 계약 준비는 가능하다. DEV3 production leaf의 시작에는 DEV2의 최소 선행 요건과 해당 task 선행 계약·증거가 필요하다. K0 volatile 시험 통과로 durable 의존성이 충족됐다고 하지 않는다. 설계 문서의 D2 채택, leaf 검증, S1 완료, 공식 배포는 별도 상태다.

DEV2의 공통 API·identity·값/오류·statement rollback·coarse validation·L0·recovery·checkpoint를 사용한다. 실제 K2–K5에서 결정할 namespace/형식/anchor가 아래 가정과 다르면 소비 구현 전에 양쪽 소유 계약을 조정하고 독립 검토한다. DEV2 완료 조건을 DEV3 page 구현에 역으로 종속시키지 않는다.

| DEV2 공급 계약 | DEV3 소비와 검증 |
| --- | --- |
| K0/K1 row/catalog·snapshot·private batch·statement 경계 | disk read/scan와 private index delta의 동일 의미; cursor 변경 감지 |
| K2 durable L0·writer epoch·파일 namespace | pack identity, data-before-commit dependency, UNKNOWN fencing, feature/format 거절 |
| K3 q chain·STABILIZE·epoch resume | page closure 검증과 bounded recovery. 이전 OS cache 채택→두 번째 장애 사례 |
| K4 coarse serializable validation·실제 thread 이력 | concurrent unique/phantom/schema conflict. 최신 root overwrite로 lost update 금지 |
| K5 exact cut·CURRENT/WRITER·pin/retire·restore | chunked disk inventory, previous와 replay roots 보존, source WAL 없는 exact-cut restore |

<a id="sequence"></a>
## 2. 구현 순서

S0–S5는 DEV3 내부 작업 묶음이며 상위 DEV 단계나 기존 task ID를 대체하지 않는다. 모든 구현은 기존 package·effect 경계를 사용한다. leaf마다 의미·oracle·fault profile을 먼저 동결하고 실제 코드가 생길 때만 소스 경로를 등록한다.

| 묶음 | 기존 task 연결 | 구현·독립 기대값 | 완료 조건과 남길 의무 |
| --- | --- | --- | --- |
| S0 계약·oracle 준비 | XADB-028/021/022/029 공통 | 독립 composite key/NULL/row/index 모델, page/pack/manifest byte vector, corrupt corpus, resource profile | D3-F01–F03 동결. oracle만 PASS여도 disk 실행 NOT_RUN |
| S1 디스크 표현 | XADB-028 | page/cell/key/root·inventory codec, COW 변환과 독립 byte/구조 vector. DEV2의 XADB-009/010/027 선행 확인 | production에서 사용할 표현 구현을 실제 byte buffer로 검증. 파일 I/O·S1 제품 gate는 미완료 의무로 남김 |
| S2 page·pack·buffer·tree 경로 | XADB-021 | XADB-028의 표현 위에 실제 파일·checksum/parser·PageGuard·clock·row overflow/locator·index scan·private delta 연결 | XADB-005/008/028 선행 충족 후 구현. 실제 short I/O·full pin·취소·scan equality 검증; 공통 commit 인수는 다음 묶음 |
| S3 다중 객체 영속 통합 | XADB-022 | pack+inventory sync→공통 WAL→전체 root; offline index build와 실제 reopen·dependency 검증 | XADB-009/021 선행 충족. 모든 table/index/catalog 같은 q, ACK/UNKNOWN, crash cut와 DEV2 회귀. 이 이후만 durable disk capability 주장 |
| S4 checkpoint·회수·maintenance | XADB-029 | inventory streaming, previous/replay closure, backup/restore, whole-pack GC, offline compact | XADB-022/028 선행 충족. 오래된 snapshot·중간 replay root 생존, 두 checkpoint 뒤 회수, ENOSPC/중단·재복구 |
| S5 전체 S1 인수 | XADB-029 및 네 task의 필수 variant 전체 | 4배 이상 working set, 독립 이력·실제 I/O·PF09·자원 상한, 하위 회귀 | stage-completion의 모든 필수 결과·독립 검토·재현 bundle. 하위 leaf 일부로 전체 완료 금지 |

기존 task DAG 순서 **XADB-028 → XADB-021 → XADB-022 → XADB-029**를 유지한다. S1의 표현 구현은 S2 파일 엔진을 요구하지 않는 codec/자료구조 경계다. 표현 leaf의 로컬 검증을 XADB-028의 모든 후속 통합 의무 완료나 DEV3 S1 gate로 확대하지 않는다. task의 필요한 최소 선행 leaf와 남은 통합 의무를 착수 계약에 각각 기록한다.

<a id="parallel-development"></a>
### 2.1 병렬 개발 가능 범위와 작업 묶음

**현재 설계는 DEV3 내부의 조건부 병렬 개발이 가능하다.** 공통 표현과 API를 먼저 합친 뒤 pack I/O·buffer·공통 tree를 나누고, 그 최소 통합 위에서 row/locator·index·catalog 소비 작업을 나눈다. 최초 배정은 구현 2–3갈래와 독립 검증 담당을 기준으로 하며 통합 책임을 한 담당에게 지정한다. 이는 설계에 근거한 작업 구성이고 병렬 pilot·인원별 속도 향상 실측은 아니다. 역할은 아래와 같으며 실제 사람/agent 식별자는 착수 시 D3-F07에서 지정한다.

| 작업 ID·소유 역할 | 주 책임·예정 수정 영역 | 시작에 필요한 최소 선행 leaf | 병렬 범위·완료 경계 |
| --- | --- | --- | --- |
| D3-C 공통 계약·표현 담당 | S0/S1의 page/pack/PageRef·root/inventory 형식과 좁은 공유 API. `storage::disk::page`와 관련 codec | D3-F06의 XADB-028 선행 확인, 해당 D3-F01/F03 동결 | 아래 소비 작업보다 공통 표현·signature 먼저 병합. key/row별 독립 vector 준비는 병렬 가능. 모든 구현의 공통 `types` 집합을 만들지 않음 |
| D3-P pack·파일 담당 | S2의 `storage::disk::pack`, dependency inventory의 streaming 파일 입출력·seal·sync·build-owner 추적 | D3-C의 해당 codec·FileId/namespace·파일 읽기/준비 계약, XADB-021 선행, 해당 F02/F03/F05 | D3-B/T와 병렬 구현. page 파일의 durable 준비까지만 소유; WAL commit·retention ledger는 D3-I |
| D3-B buffer 담당 | S2의 `storage::disk::buffer`, PageGuard·Loading·clock·취소·예약·metadata 회계 | D3-C의 page identity/검증 결과·읽기 API, D3-P와 오류/파일 pin 경계 합의, XADB-021 선행, 해당 F02/F03/F05 | D3-P/T와 병렬 구현. 실제 load·eviction 인수는 pack 경로 연결 뒤. snapshot closure 수명을 buffer frame pin으로 대체하지 않음 |
| D3-T 공통 tree 담당 | S1 표현/S2의 `storage::disk::tree`, page 변환·path-copy·split/delete·root stack 순회 | 해당 표현 codec·cell 비교/크기·page 읽기/준비·guard 반환·자원 예약 API, 각 S1/S2 선행 | byte buffer를 쓰는 실제 자료구조 leaf는 좁게 검증 가능. S2 파일 경로 완료는 D3-P/B 통합 뒤. row/index/catalog용 tree 알고리즘을 각각 복제하지 않음 |
| D3-R 행·locator 담당 | S2의 disk row/overflow와 `storage::locator` cache, RowUID·private row view | D3-T/P/B의 최소 실제 읽기·변경·순회, row/overflow 형식·snapshot/statement 계약 | D3-X/CAT와 병렬. 영속 locator는 row tree 자체이며 row와 한 담당. overflow 경로도 이 담당이 소유 |
| D3-X index 담당 | S2의 `storage::index`, PK/unique/nonunique·key delta·range/cursor·offline build | D3-T/P/B 최소 통합, D3-F02의 IndexSpec·오류·RowId/row lookup·private revision·catalog 등록 계약 | D3-R/CAT와 병렬 구현. row/index 일치와 build의 durable 결과는 S3 통합 후 검증 |
| D3-CAT catalog 담당 | S2의 `catalog`, schema/index 정의·이름/객체 key·root descriptor·bounded cache | D3-T/P/B 최소 통합, D3-R/X와 공유 metadata·root 변경 계약 동결 | D3-R/X와 병렬. 전체 CommittedRoot 공개·DDL admission/commit 연결은 D3-I |
| D3-I transaction·durable 통합 담당 | S3/S4의 `transaction`·`publication`·`durable`·retention ledger와 disk reclaim/compact 조립 | S3는 D3-P/B/T/R/X/CAT의 필요한 실제 경로와 XADB-009/021; S4는 XADB-022/028. 해당 F04/F05/F06 | prepare→sync→WAL→publish와 recovery/STABILIZE·pin/retire의 공유 변경을 순차 병합. 소유 파일이 분리된 checkpoint/GC 순회 helper와 fault 사례 준비는 병렬 가능 |
| D3-V 독립 검증 담당 | S0부터 reference·외부 history·fault/known-bad corpus·DS01–06/PF09 실행 recipe | 읽을 의미 계약과 해당 독립 expected vector; production 소비 전에 F03 검토 | 각 구현과 병렬. oracle/checker 변경을 별도 D2 검토한 뒤 후보 검사. 제품 구현의 자기 보고·codec으로 기대값을 계산하지 않음 |

위 영역은 **계획상 책임**이며 아직 생성할 파일 목록이나 작업 배정 완료가 아니다. `page/tree/buffer/pack`은 기존 `storage::disk` 내부 책임, row locator와 index는 저장소 정책의 기존 소유 경계다. 구체 `.rs`/하위 폴더·시험 파일을 구현 직전에 좁은 경로로 확정하고 그때만 빌드·지문 inventory에 등록한다. D3-C는 공유 계약 조정자이며 D3-CAT catalog 구현 담당과 구분한다. 한 사람이 여러 역할을 겸할 수 있지만 자기 구현의 독립 검토자가 될 수 없다.

D3-P/B/T가 공유하는 FileId 예약·build-owner/closure pin·PageGuard·준비 파일 rollback·취소/오류 효과와 page 읽기/준비 API를 먼저 동결한다. metadata 8 MiB·시스템 scratch 16 MiB 등 설계의 **공통 한도**를 각 모듈의 별도 사용 가능량으로 중복 배정하지 않는다. 각 leaf의 allocation/FD/disk 예약·반환 소유와 필요한 fault hook·receipt를 F02–F05에 고정하고, 실제 오류 주입 전에 도달·잔존 효과의 판정 기준을 준비한다.

<a id="parallel-waves"></a>
### 2.2 병합 순서와 병렬 구간

| 구간 | 병렬로 진행할 작업 | 선행 병합·실제 확인 |
| --- | --- | --- |
| S0 준비 | DEV2 공급 계약 대조, byte/의미 vector, 독립 oracle·fault/profile 설계 | D3-C가 공통 계약을 모아 검토. 수신 전에는 가정/대조 결과이며 production 착수 증거가 아님 |
| S1 표현 | 확정된 공통 page codec 위에 소유가 분리된 row/key/inventory codec와 독립 vector 검사 | 공통 PageRef/header·오류/수명 signature를 먼저 병합. D3-T 공통 tree 자료구조에는 한 작성자. XADB-028의 필요한 표현 leaf 검증 |
| S2 기반 | **D3-P pack ∥ D3-B buffer ∥ D3-T tree**, D3-V 동행 | D3-C와 D3-F06/F07 충족 후 착수. 단독 byte/vector 결과는 MODULE 범위. 실제 pack→buffer→tree 연결·short I/O·pin·scan 검증 뒤 소비 작업의 기준 revision 확정 |
| S2 소비 | **D3-R row/locator ∥ D3-X index ∥ D3-CAT catalog**, D3-V 동행 | 공유 tree는 D3-T만 수정. row lookup/IndexSpec/catalog delta·statement mark를 먼저 고정하고 row+모든 index+catalog의 실제 동등성/실패 복원을 연결 |
| S3/S4 통합 | 모듈별 fault·recovery 검증 사례와 bounded 순회 helper는 분리 가능 | D3-I가 공통 상태 전이→commit/reopen→checkpoint/retention/compact 순으로 반영. helper 준비가 S3 이전 S4 production 착수나 gate 완료를 뜻하지 않음 |
| S5 인수 | 고정된 같은 source/build에서 독립 fixture를 쓰는 DS/PF job 분할 | 통합 담당이 필수 variant 전부를 수합. 성능 job은 동일 장치/CPU 부하 간섭을 막고 환경·반복 순서를 사전 고정. 한 job PASS로 S1 승격 금지 |

이 표는 기존 task DAG를 제거하지 않는다. 각 작업은 [2절](#sequence)의 상위 선행 조건과 이 표의 최소 leaf를 **모두** 확인한다. 여러 branch에서 컴파일되거나 mock을 사용하는 시험이 성공해도 아직 연결되지 않은 disk I/O·ACK→reopen·retention 인수는 NOT_RUN이다. 통합 대기 중인 나머지 작업은 독립 시험·검토를 계속할 수 있으나 선행 결과를 가정하여 완료 처리하지 않는다.

<a id="parallel-entry"></a>
### 2.3 DEV2 이관 중 준비와 production 인수 경계

2026-10-18 사용자 설명에 따라 DEV2는 이관 진행 중으로 취급한다. **설계 완료·이관 진행·이관 완료·XADB 실제 검증은 별도 상태**다. 공급 저장소나 진행 중인 이관 파일을 이 작업에서 변경하지 않는다. 이관 중에는 D3-C/V의 계약 대조·vector/시험 설계와 작업 범위 준비를 진행할 수 있다. 이관 revision을 사용한 production 수정은 아래 확인 이후 시작한다.

1. 통합 담당이 수신한 commit과 dirty digest, 실제 crate/API/format·feature·namespace를 기록한다. 필요한 DEV2 task/leaf와 확인한 실행 명령·source/build identity·결과를 D3-F06에 연결한다. 과거 공급 측 PASS를 새 XADB PASS로 표시하지 않는다. 아직 없는 durable API를 K0 이름만으로 추정해 구현하지 않는다.
2. DEV2의 private statement·snapshot/cursor·commit/UNKNOWN·root·writer epoch·CURRENT/WRITER·pin/retire 경계와 D3-F01/F02/F04를 대조한다. 불일치한 경계는 단일 담당이 먼저 조정하고 D2 독립 검토 뒤 소비 작업의 동일 기준에 반영한다. 해당 미확인 의존 작업만 대기시키고 독립적인 준비는 계속한다.
3. 작업별 최소 선행 leaf를 확인한 뒤 D3-F07에 실제 담당·경로·동시 작업과 병합 순서를 기록한다. S1 표현만 검증한 상태로 XADB-028 전체 완료를 선언하거나 S3에서 필요한 DEV2 영속 선행을 생략하지 않는다. 단계 완료에는 기존 B2/D0와 DEV3 S1의 모든 의무가 유지된다.

<a id="parallel-ownership"></a>
### 2.4 공유 파일·계약의 단일 소유

[병렬 개발 정책](../policies/parallel-development-policy.md#section-2)의 `change_coordination` 전 필드를 실제 작업마다 기록한다. worktree 분리는 권장할 수 있으나 파일·공통 의미 중복을 없애 주지는 않는다. 동일 worktree에서는 같은 파일을 동시에 쓰지 않는다.

| 충돌 지점 | 담당·처리 순서 |
| --- | --- |
| page/header/PageRef·공통 tree·key 비교 의미 | D3-C가 형식/API 변경을 조정하고 D3-T가 tree 구현을 소유. 소비자가 필요 사항을 제출하면 해당 담당 patch를 먼저 합친 뒤 기준 갱신. row/index/catalog 쪽에 복사 구현 금지 |
| API·오류 번호·IndexSpec·statement mark·cursor revision | 계약별 소유자와 D3-I가 먼저 확정. 소비 모듈은 이미 정의된 typed 결과로 연결. 공유 파일의 최종 작성자는 한 명으로 지정 |
| foundation effect port·platform adapter·공통 quota | D3-I가 해당 소유자와 변경을 먼저 검토/반영하고 D3-P/B/T가 소비. 각 갈래가 자체 FileIo/namespace·예약 의미를 추가하거나 platform을 engine에서 직접 import하지 않음 |
| 파일 inventory 형식/codec·I/O와 retention 의미 | D3-C 형식→D3-P 입출력→D3-I 선택/pin/retire 순서. pack 담당이 별도 GC·WAL/ACK 경로를 만들지 않음 |
| transaction/publication/durable·공통 retention ledger | D3-I가 실제 DEV2 소유자와 경계를 확인하고 순차 patch. WAL-only 변경이라도 page closure·오래된 pin에 대한 의미 중복을 검토 |
| lib.rs/mod.rs, Cargo.toml/lock, runner/frame 등록·공용 checker, README/documents.json | 통합 담당만 필요한 연결 patch를 최신 원문에 적용. 기능 본문·시험은 소유 파일로 분리. 이관 중 변경과 넓은 glob이 겹치면 좁히거나 병합 순서 확정 |
| source/build 지문·clean export·document-validation.json | 합친 manifest/원문에서 통합 담당이 재생성. 기존 결과 hash를 새 코드 PASS로 수정하지 않음 |

병합할 때에는 공통 계약→모듈 leaf→실제 경계 통합→해당 DEV2/DEV3 회귀→생성물 순서를 지킨다. D1/D2 독립 검토와 V1/V2/V3의 강도는 기존 DP01/DP03을 적용한다. 실제 병렬 실행 시 PD01–PD03의 해당 작업 쌍·공유 경로·의미 충돌·재작업과 합쳐진 source/build 증거를 기록한다. 현재 배정·병렬 실행·속도 측정은 모두 미실행이다.

<a id="acceptance"></a>
## 3. 시험 의무와 구체 variant

모든 항목은 **NOT_RUN**이다. testcase 목록은 새 실행기의 입력으로 동결해야 하며 이 표가 존재한다는 사실은 실행 증거가 아니다. 독립 기대값은 production codec/tree/replay·기존 제품의 출력 복사로 만들지 않는다.

| 인수 ID | 필수 variant·검출할 반례 |
| --- | --- |
| DS01, KU09 | 빈 tree/한 행/다층 tree, 순차·역순·난수 삽입, leaf/internal/root split, 삭제와 root 축소, inline 2 KiB 전/동일/후와 128 KiB row, overflow 누락/순환. 모든 index point/range/full scan을 독립 row 값과 대조 |
| DS06, KU09 | PK NULL/중복, Unique의 부분·전체 NULL과 중복 허용, Nonunique RowId 순서, 1–4열·최대 8 index, UTF-8·빈 값·NUL bytes·정수 양끝·1024-byte escaped key 경계. key update, 없는 행, 동일 statement 중복/교환 실패 후 이전 statement 유지 |
| DS02, RC-T12, KU10 | 두 table·모든 index·catalog/locator를 한 q로 변경. 각 dependency file write/sync/dirsync, WAL append/sync, publish/reply 전후 crash. page-first 장벽 생략·일부 index 누락·다중 root 별도 공개 mutant 검출 |
| DS03, RC-T14, KU10 | row·index·locator lookup·inventory 각각 buffer보다 큰 데이터, 부하·recovery·checkpoint·build·GC에서 bounded RAM; recovery VERIFY는 disk spill 없는 양방향 lookup으로 검사. 파일 수/메타데이터 증가가 RAM 증가로 직결되지 않음. 전체 locator 상주·recovery 전체 로드 mutant 검출 |
| DS04, RC-T13 | old reader를 유지하며 update/delete/drop·eviction·checkpoint·GC. 최신 root에는 없지만 retained WAL 중간 q가 참조한 page 보존. pin/mark/Retiring 경합·stale cache·file identity 재사용 mutant 검출 |
| DS05 | CURRENT old/new 선택·WRITER rotation·fallback, 공유 pack 손상, missing inventory chunk, repeated recovery와 두 번 장애/STABILIZE. exact C backup을 source WAL 없이 restore→새 ACK→재시작. RECOVER_OK/REJECT_CORRUPTION/REJECT_IO 구분 |
| DS03/04/05 | compact 중 읽기 drain timeout·spill/pack ENOSPC·WAL UNKNOWN·publish 직후 crash. 논리 digest 동일, q 전진, previous/backup pin 유지, checkpoint 두 번 뒤 삭제량·고유 disk bytes 대조 |
| DS06, KU09 | offline build에서 중복·oversized key·취소·quota·drain timeout·재시작, drop/recreate handle. 결과 tree는 재open 후 재build 없이 사용 |
| DEV2 회귀 | snapshot/RowUID·오류 effect·statement rollback·serialization·ACK/UNKNOWN·WAL chain·checkpoint/restore의 적용 variant를 새 disk 경로에서 재실행. 기존 volatile와 lower regression도 유지 |
| PF09 | 동일 의미의 fixture/recipe·baseline/candidate·독립 결과·반복/통계·계측 비용. COW write amplification, reopen I/O, file count, 유지보수 공간을 포함 |

reference는 key 정렬을 typed tuple과 자체 byte 비교로 계산하고 literal vector로 교차 검산한다. production key를 decode해서 기대 순서를 계산하지 않는다. index 검사에는 잘못된 RowId 연결, 중복/누락 entry, NULL 정책 오류를 가진 known-bad 출력도 입력한다. 외부 controller의 응답 관측과 transaction별 witness row를 남겨 나중 overwrite가 ACK 유실을 숨기지 못하게 한다.

<a id="faults"></a>
## 4. fault·동시성 profile

S0에서 named operation마다 before/after와 일부 효과 후 error를 정의한다. effect port의 실제 호출 receipt·잔존 bytes·파일 길이·sync 확인 범위를 함께 기록한다. controller가 fault 도달을 확인한 후 소유 worker를 종료한다. 임의 sleep·자기 보고만으로 특정 cut을 달성했다고 하지 않는다.

| 경계 | 필수 cut·schedule |
| --- | --- |
| page buffer | map 등록/Loading I/O/완료/최초 pin/마지막 unpin/eviction, 같은 page 동시 load, 전 frame pin과 cancel, 재사용 frame generation 오인 |
| tree/private | 각 path-copy/split/overflow 예약·statement undo mark·index old 제거/new 삽입. 실패 후 transaction의 이전 private 상태 |
| pack | create·short/zero write·seal·file sync·directory sync, symlink/다른 owner·이미 존재 파일·읽기 중 checksum/length 오류 |
| commit | dependency inventory 마지막 chunk, 전후 root 검증, WAL 첫 byte·partial/full-length torn·sync, publication 직전/직후, 외부 ACK 관측 전/후 |
| checkpoint/recovery | anchor pin과 commit/rotation 경합, manifest/CURRENT 각 전환, fallback 재선택, VERIFY 반복 crash와 spill 파일 누적 0, STABILIZE 전/중/후의 두 번째 장애, RESUME WRITER 선택 |
| reclaim/compact | mark 중 새 commit/pin, revision 재검사, Retiring 전/후 새 pin, unlink/dirsync, compact old/new root·두 checkpoint·backup pin |

공통 DP03의 V2 property 64 seeds·seed당 최대 128 operations, V3 256 seeds·최대 1,024 operations, V3 실제 thread stress와 target별 fuzz 각 30분, 비결정적 fault 셀 3회 반복 기준은 [개발 운영 방침](../policies/development-policy.md#section-4)을 따른다. leaf에서 지원되는 한도 안의 정확한 operation 수·반복·timeout·실제 cut 목록을 정한다. 미지원 harness를 PASS/N/A로 바꾸지 않는다. 필수 사례의 timeout·미도달·receipt 유실은 실패 또는 미검증이며 정상 복구를 손상 거절로 사후 바꾸지 않는다.

process kill, syscall fault, power-loss model, 실제 장치 qualification은 분리한다. OS cache가 살아 있는 process kill만으로 전원 장애 내구성을 주장하지 않는다. 성공 sync bytes 보존을 포함한 장치 모델과 실제 Linux/FS/mount/장치 identity를 D3-F04/F05에 기록한다.

<a id="performance"></a>
## 5. 용량·성능·메모리 검증

기본 profile은 buffer 64 MiB이며 16 MiB·4 MiB의 축소 profile은 경계·fault 탐색용이다. **각 profile의 해당 accessed working set ≥ buffer 4배**를 적용한다. 기본 용량 사례는 row/index의 접근 대상 256 MiB 이상, 이어 1 GiB corpus를 사용한다. 압축되지 않은 fixture 명목 크기만 비교하지 않고 실제 접근한 고유 page bytes·조회된 index 범위를 기록한다. locator·inventory에도 RC-T14의 전체 configured page-buffer 4배 기준을 유지한다. locator는 row tree의 영속 해석 경로를 큰 corpus에서 검사한다. inventory 인수는 global page buffer를 4 MiB 등으로 설정한 별도 사전 profile에서 실제 유효 inventory 16 MiB 이상과 그 참조 pack closure를 생성한다. 이에 필요한 disk quota·파일 수·시간을 D3-F05에 미리 고정하고, fixture를 만들지 못하면 NOT_RUN으로 남긴다. metadata 8 MiB 안의 locator cache 1 MiB, inventory decoder buffer 64 KiB의 별도 4배 pressure 사례는 추가 시험이며 전체 page-buffer 기준을 대신하지 않는다.

| workload | 비교할 비용·필수 관측 |
| --- | --- |
| 순차/랜덤 insert·read·update·delete | 1/2/4 writer와 readonly reader, 균등/고정 편향. 성공 TPS와 conflict 비율을 함께 보고 |
| point/range/full scan | 선택도·limit·중복 key·hot/cold 범위, page reads·hit/miss·index→row 비용·독립 값 동일성 |
| 오래된 snapshot + 쓰기 | retained/live ratio·pin wait·quota 거절. pin을 몰래 만료해 성능 개선으로 집계하지 않음 |
| checkpoint/backup/restore/reopen | wall time·CPU·읽기/쓰기·sync·scratch/RSS·동시 foreground 지연 |
| delete churn + compact | dead/live bytes·파일 수·전체 재작성 bytes·두 checkpoint 후 실제 회수, 공간 부족 시 사전 거절 |

engine FileIo counter는 syscall byte/operation을 보여 주고 외부 `/proc`/소유 process I/O 관측은 실제 장치 I/O와 OS cache 영향을 구분한다. engine cache miss만으로 장치 read가 있었다고 주장하지 않는다. warm/cold cache 조건은 소유 fixture·격리 환경에서 준비하고 공유 host 전체 cache drop은 사용하지 않는다.

RSS budget은 환경의 고정 runtime baseline + buffer·metadata·전체 private·scratch·diagnostic 예약 + 사전 고정 allocator margin으로 정한다. peak와 장기 기울기를 둘 다 확인한다. dataset 증가 시 bound 초과, leaked pin/file handle, 누적 메모리 증가를 검출해야 한다. logical reservation 성공을 전체 OOM 회복으로 해석하지 않는다.

성능 프로파일은 각 case의 데이터·seed·기간·warmup·반복·통계·회귀 수치·계측 on/off 비용 한도를 실행 전에 확정한다. 초기 구조 선택은 성능 PASS가 아니다. 기본 선택보다 나은 후보의 채택에는 동일 durability와 independent result, source/binary/recipe identity, 새 세션 재현 및 DP07 검토가 필요하다.

<a id="freeze"></a>
## 6. 구현 전 동결 항목

| ID | 정확한 산출물·담당 경계 | 현재 상태·닫는 시점 |
| --- | --- | --- |
| D3-F01 | storage/durable: page/pack/PageRef/inventory/feature/WAL extension의 field offsets·magic·길이·digest 범위, 독립 정상/손상 hex vectors | 구조·크기 기준 결정, byte vector 미작성. S1 표현 codec 구현 전 |
| D3-F02 | api/catalog/transaction: IndexSpec·PK/NULL/오류 번호·range bounds·cursor revision·build/compact 상태표·Rust signature | 의미는 상세 설계에 정의, 실행 API/vector 미동결. S2 구현 전 |
| D3-F03 | reference/checker: 독립 tuple/row/index/history 모델·negative mutants·case/variant 전체 목록·예상 결과 | DS01–06에 배정, oracle 구현/검증 미실행. 해당 leaf production 구현 전 |
| D3-F04 | platform/durable: pack+WAL+CURRENT/WRITER effect receipts, fault/corruption 판정과 반복 장애 schedule | 명명 경계 정의, 실제 hook/장애 모델 미동결. S3 구현 전 |
| D3-F05 | storage/performance: cache/metadata/FD·disk reserve·RSS margin·I/O timeout·외부 I/O 관측·대형 inventory의 실제 disk quota·환경 digest·PF09 수치 | 공학적 기본값만 존재, 실제 profile 미실행. 해당 용량 leaf 구현/실행 전 |
| D3-F06 | integration: DEV2 실제 API/format·회귀 evidence, stage/task 선행 증거·clean build·모듈 의존 검사·실제 runner와 보호 검토 경로 | DEV2 이관 진행 중이라는 사용자 설명. 수신 revision·durable 선행의 실제 충족은 별도 확인; production 착수·S1/배포 시 각각 확인 |
| D3-F07 | integration/각 작업 소유자: 실제 담당·기준 commit/dirty digest·좁은 수정 경로·공유 계약 담당·동시 작업 중복·병합 순서·통합 검사 | [병렬 작업 묶음](#parallel-development)만 정의. 담당 배정·경로 확정·pilot NOT_RUN; 실제 각 작업 착수 직전 기록 |

후속 PART/NG/SQL/서버의 미구현을 위 leaf의 새 선행 조건으로 추가하지 않는다. 반대로 D3-F04가 미동결인데 crash 시험을 통과했다고 하거나 D3-F06 없이 DEV3 구현을 시작하지 않는다. 새로운 실행 스크립트 이름이나 artifact 경로를 이미 존재하는 entrypoint로 등록하지 않는다.

<a id="review"></a>
## 7. 작업 소유·검토·검증 기록

| 항목 | 기록 |
| --- | --- |
| 분류 | D2: storage 내부 형식·API·인수 계약 구체화 |
| 담당·계약·통합 | `/root` |
| 기준 | `7f656e6aa47c114772cb758e4c1a1af2b16712db`, 착수 시 `git status --short` 빈 출력 |
| 독점 수정 | `contracts/disk-storage-design.md`, `contracts/disk-test-matrix.json`, 이 계획, `evidence/dev3-source-research.md` |
| 공유 수정 | `plans/development-stages.{md,json}`, `plans/stage-completion.json`, 문서 README·documents.json. 담당 `/root`가 순차 반영 |
| 계약 영향 | DEV2의 불변식 유지. DEV3 page/locator·key·자원·retention 구체화. 과거 실행 evidence와 제품 코드 불변 |
| 중복 확인 | 착수 시 clean tree, 구현 subagent 없음. 검토 중 루트 README·project-direction·phase1-product-baseline·capability-inventory·architecture-overview의 제품 소개 5곳에 외부 동시 수정이 나타남. 이 작업에서 작성·승인한 변경이 아니므로 보존하고 D2 소유 범위에서 제외. 독립 검토자는 읽기 전용 |
| 반영 순서 | 소스 조사→storage 계약→구현 계획/기존 인수 JSON→색인→독립 검토/수정→문서 검사 재생성 |
| 생성물 | `document-validation.json`, `python3 devdoc/verify_documents.py`, 담당 `/root` |
| 독립 검토 | `/root/dev3_design_review`; 원문·diff·소스 증거와 기존 DEV2/복구 계약에서 출발. 최초 P2 1건 수정 후 최종 설계 검토 PASS. 계약/통합 담당 `/root`가 반영 확인 |
| 검증 | `python3 devdoc/verify_documents.py`: 45 Markdown, 오류 0. 기존 문서 검사 회귀 `python3 -m unittest discover -s devdoc -p 'test_*.py'`: 27 tests PASS. 최종 기록 반영 후 같은 문서 검사를 재생성한다. DBMS source build/benchmark·DEV3 실행·S1 승격은 수행하지 않음 |

### 독립 검토 반영

| 항목 | 지적·수정·확인 범위 |
| --- | --- |
| P2 recovery scratch | 최초안의 외부 정렬 VERIFY는 정상 DB의 크기와 반복 crash에 따라 임시 disk가 부족해질 수 있었다. 추가 disk spill 없는 row→index lookup과 index→row 역검사·strict order/unique/count 검사로 변경. DS03/05에 VERIFY 반복 crash와 scratch 누적 0을 연결 |
| RC-T14 기준 | 전체 configured page-buffer 4배 조건을 row/index/locator/inventory 모두 유지. 작은 전용 cache의 pressure 시험은 추가 사례임을 명시하고 대형 inventory quota를 사전 profile에 배정 |
| 선행 DAG·공통 기준 | XADB-028→021→022→029의 실제 관계와 표현 leaf/후속 통합 구분, DP03의 실제 V2/V3 수치를 대조 |
| 조사 증거 | 독립 검토자가 7개 HEAD·tracked clean·모든 기록 file hash·외부 inventory hash를 재계산해 일치 확인 |
| 검토 한계 | 설계 계약의 일관성 검토. 실제 page 구현·fault suite·성능·S1 인수의 PASS가 아님. 외부 동시 제품 소개 수정은 이 검토의 승인 대상에서 제외 |

새 설계의 승인으로 기존 historical receipt를 갱신하지 않는다.

후속 통합: 사용자가 README 변경과 DEV3 설계를 함께 commit하도록 명시 요청했다. README와 같은 제품 소개 문구를 맞춘 관련 4개 문서의 기존 변경도 함께 포함한다. 해당 소개 편집은 저장 엔진·인수 계약을 바꾸지 않는 D0로 확인했으며, 위 독립 D2 검토의 원래 범위와 구별한다.

<a id="parallel-review"></a>
## 8. DEV3-PARALLEL-001 — 내부 병렬 개발 구체화

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18. 실제 작업 배정·병렬 실행 NOT_RUN |
| 근거·담당 | 사용자가 완료된 DEV3 설계에서 병렬 개발 가능성을 확인하고 명시하도록 요청. 작업·계약·통합 담당 `/root` |
| 선택·대안 | 공통 계약/표현→pack·buffer·tree 병렬→row/locator·index·catalog 병렬→순차 영속 통합. 전체 직렬보다 독립 구현·검증을 겹칠 수 있음. 모든 역할 동시 착수는 shared tree/locator/commit 의미 충돌 때문에 선택하지 않음. 속도 이득 미측정 |
| 기준 | commit `4a923214dd91c5566779652e59e13883eb940672`의 DEV3 설계. 착수 당시 DEV2 이관 변경 29개 파일의 경로/내용 SHA-256과 revision을 정렬 JSON으로 묶은 dirty manifest SHA-256 `dae81fc1fde9123481dfb0d748cf2833492fc1443df03af411ca50a008a245d3`. 진행 중 작업의 관측 identity이며 DEV2 완료 증거가 아님 |
| 독점 수정 | 이 계획, `contracts/disk-storage-design.md`의 병렬 절/진입 안내, `plans/stage-completion.json`의 DEV3 미확정 입력. README/documents.json의 DEV3 안내·제목은 아래 공유 변경 |
| 공유 수정·중복 확인 | DEV2 이관의 product code·Cargo·repository-build-plan·checker·history 변경을 보존. 검토 중 documents.json에 추가된 `implementation-incremental-import-20261018.json` 등록도 보존하되 이번 D2 승인 범위에서 제외. 문서 색인과 생성물은 최신 내용에서 좁게 수정/재생성. 저장 형식·기존 task DAG·필수 gate·과거 조사와 실행 증거 불변 |
| 반영 순서·생성물 | 계획→설계 요약→기존 JSON/색인→독립 검토/수정→`python3 devdoc/verify_documents.py`로 document-validation.json 재생성 |
| 독립 검토 | `/root/dev3_parallel_review`가 실제 최종 diff·기존 task/저장 계약·격리 검사 결과를 읽기 전용으로 확인: PASS, 수정 필요 지적 없음. 계약/통합 담당 `/root`가 반영 확인. DEV2 이관 변경·제품 실행·병렬 속도 효과는 승인 범위 밖 |
| 문서 검증 | `git diff --check` PASS. 기준 HEAD의 깨끗한 임시 export에 이번 DEV3 문서 변경만 적용한 `python3 devdoc/verify_documents.py`: 45 Markdown, 오류 0, PASS. documents.json은 DEV3 제목 변경만 포함. 실제 동시 작업 트리의 같은 명령은 아래 이관 불일치 6건으로 FAIL이며 격리 PASS로 덮어쓰지 않음. 제품 시험·병렬 실행 NOT_RUN |
| 동시 이관 검증 한계 | DEV0/DEV1/DEV2 기존 evidence와 현재 source fingerprint 불일치 3건, DEV2 fixed check inventory 불일치 1건, streaming_controller/catalog_oracle 미구현 상태와 변경 내용의 불일치 2건. DEV2 측 새 실행 증거와 인수 상태 정합성 확인이 필요하며 이 문서 작업에서 기존 receipt나 checker를 바꾸지 않음 |
| 대체·재검토 | DEV3-DESIGN-001을 대체하지 않는 실행 분할 구체화. DEV2 실제 API 불일치, 반복 shared-file/tree 충돌, 모듈 분할로 원자성/자원 인수 실패 시 재검토 |

# DEV3 — 구현·병렬 개발·검증 계획

2026-10-20 · **DEV3-DESIGN-003·REVIEW-004–008 ACCEPTED / 실제 disk path·전체 단계 모델·S1 NOT_RUN**

2026-10-20 [첫 표현 leaf·부분 모델](../implementation/dev3-implementation.md#validation)을 구현·독립 검토하고 같은 소스의 하위 회귀·DEV2 복구 검사를 완료했다. 아래 전체 인수의 NOT_RUN과 page envelope·M02-P01의 제한된 로컬 검증을 구분한다.

2026-10-22 [잔여 작업 분해](#remaining-work)는 기존 16개 분류에 공급 의존성·완료 조건·인수 연결을 보완한다. [작업 크기·우선순위·병렬 운영](#work-organization)은 대분류를 유지하면서 실제 배정 단위를 나누고 현재 재개 후보를 정한다. 현재 부분 구현과 원본 실행 결과는 [구현 기록](../implementation/dev3-implementation.md#dev3-work-breakdown)에 남기며 이 계획 보완으로 제품 완료 상태를 바꾸지 않는다. 아래 과거 하위 회귀·동일 build 실행 문구에도 [변경된 코드만 시험하는 사용자 지시](../../AGENTS.md)가 우선한다.

[16개 작업별 상세 명세·설계도](../contracts/dev3-task-designs.md) · [상세 설계](../contracts/disk-storage-design.md) · [이번 조사·검토](../evidence/dev3-wal-undo-research.md) · [이전 조사](../evidence/dev3-source-research.md) · [완료 정본](stage-completion.json) · [DEV2 선행](dev2-implementation-plan.md)

2026-10-22 [방향 리뷰 후속 결정](#direction-review-adoption)은 현재 제한 쓰기의 복구→일반 buffer/live undo→첫 행 거래 연결을 우선한다. 아래 이전 결정의 검토·실행 사실은 보존하며, 현재 선택 순서는 [우선 실행 묶음](#work-priority)을 따른다.

<a id="entry"></a>
## 1. 착수와 완료

2026-10-22 후속 사용자 지시의 [단계별 테스트 계획](dev3-test-plan.md)이 실행 시점을 구체화한다. 중간 단계에는 변경 기능의 필수 시험만 적용하고, 테스트·fixture·oracle는 구현과 함께 추가한다. **DEV3 최종 S5는 확정 DEV3 inventory 전수 실행**이며 기존 무변경/전수 금지의 한정 예외다. 장시간·대규모·전체 조합은 최종 인수에 배정하며 DEV0/1/2·workspace 전수 wrapper로 확대하지 않는다. 아래 인수 목록은 전체 의무이고 매번 실행할 목록이 아니다.

[항목별 재점검](dev3-test-plan.md#item-review)은16개 작업의 중간 누락 경계와 공동 case를, [실행 목록](dev3-test-plan.md#case-inventory)·[비용 제어](dev3-test-plan.md#cost-control)는 실제 selector/변형 분모·준비 비용을 구체화한다. 같은 실행의 다중 의무는 결과를 연결하며 각 계약의 명시 조합·최종 budget은 유지한다.

선행 REVIEW-002를 반영·독립 검토한 다음, DESIGN-003으로 COW pack 경로를 in-place page·WAL·영속 undo로 대체한다. 이 계획은 이전 DESIGN-001/PARALLEL-001의 구현 작업 분할을 대체한다. 실제 구현·수신 API·실행 증거가 없는 항목은 여전히 미완료다. DEV2 부분 구현 중에도 설계·oracle 준비는 가능하지만 volatile P0/K0 PASS로 영속 선행을 충족했다고 하지 않는다.

| DEV2 공급 | DEV3 이관·변경 경계 |
| --- | --- |
| K0/K1 RowUID·값·오류·statement 원자성 | 의미 유지. private batch 대신 영속 undo mark로 실패 복원; RowUID와 snapshot API 어댑터 검사 |
| K2 WAL·writer epoch·namespace·UNKNOWN | 원리 유지. AXF1 한 거래 frame·page-before-commit은 DEV2 범위. DEV3는 interleaved update/CLR/commit frame과 WAL-before-page |
| K3 이력·STABILIZE·새 epoch·재장애 | ACK/UNKNOWN·tail 판정 유지. DEV3 analysis/redo/undo와 doublewrite·undo 진행점 추가 |
| K4 기본 Serializable·실제 thread 검증 | global-q validation을 table read/write-set validation으로 대체; 명시 ReadCommitted와 row/unique lock 추가 |
| K5 selector·retention·restore | namespace/불명 결과 규칙 유지. DEV3 fuzzy metadata checkpoint·offline exact-cut backup·page/undo horizon으로 변경 |

DEV2 완료에 DEV3 구현을 역으로 요구하지 않는다. DEV3 소비 전에 실제 공급 revision·각 최소 leaf 결과를 확인하고 형식/API 차이를 F06에 기록한다. 개발 형식 변경은 새 fixture로 검증하며 기존 DB 자동 변환/삭제를 하지 않는다.

<a id="reuse-boundary"></a>
### 1.1 재사용과 재구현의 범위

DEV2 영속 기준선과 DEV3 page 엔진은 복구 알고리즘이 다르다. 하위 검증 순서는 유지하되 DEV2를 두 번째 제품 엔진으로 확대하지 않는다. 미구현 K1–K5는 기존 gate를 충족하는 범위로 구현하며, DEV3에 쓸 계획이 없는 COW 최적화·format converter·장기 제품 유지 경로를 추가하지 않는다.

| 공급 항목 | DEV3에서 유지할 것 | 새로 구현·검증할 것 |
| --- | --- | --- |
| foundation/platform | typed ID/value/error, Linux effect port·실제 I/O fault 관측 | page/LSN/UndoRef identity와 추가 효과 요구를 실제 API에서 확인 |
| P0/K0·K1 의미 | 독립 row/history 기대값, RowUID·statement 원자성·오류 certainty | snapshot/own-write view를 undo로 공급하는 adapter, frame/pin 수명 |
| K2·K3 영속 경계 | ACK/UNKNOWN·writer lease/epoch·namespace·tail 판정의 의미와 외부 witness | interleaved page redo/undo/CLR, DWB, analysis/redo/undo, recovery epoch. 기존 AXF1 replay나 COW root 교체를 재사용한 척하지 않음 |
| K4 동시성 | 독립 이력 검사와 실제 worker/thread 관측 | table validation·row/key lock·version-edit gate·reader transient pin, 새 schedule과 기대값 |
| K5 checkpoint/restore | selector/namespace의 실패 의미와 exact-cut 결과 계약 | ATT/DPT capture·WAL floor·undo/status aging·offline flush/copy와 새 timeline 복원 |

F06 수신 inventory는 각 실제 symbol/test별로 `reuse unchanged / adapt / DEV2-only / new DEV3`와 근거 revision·담당·영향 case를 기록한다. 코드가 아직 없으면 `not supplied`이며 재사용 가능으로 세지 않는다. 공통 helper 추출은 두 실제 소비 경로의 같은 의미를 확인한 뒤 수행한다. DEV2/DEV3 구현 차이를 숨기는 storage-mode flag나 사용처 없는 공통 recovery framework를 미리 만들지 않는다. 오래된 fixture/receipt는 원래 profile로 남기고 DEV3의 현재-build reopen·독립 결과는 새로 실행한다.

<a id="sequence"></a>
## 2. 구현 순서

S0–S5는 DEV3 내부 묶음이며 제품 gate S1과 구분한다. 기존 DAG **XADB-028 → XADB-021 → XADB-022 → XADB-029**와 각 task의 하위 선행을 유지한다.

| 묶음 | task | 구현 | 완료 경계 |
| --- | --- | --- | --- |
| S0 계약/oracle | 네 task 공통 | byte/의미 vector, undo/CLR 상태표, 독립 이력, 실제 fault/자원 profile | 해당 F01–F05 동결. 제품 실행 아님 |
| S1 표현 leaf | XADB-028 | page/anchor/allocation/UndoRef/txn-status/WAL codec·key 비교·byte-buffer tree 변환 | XADB-009/010/027의 최소 선행 확인. 표현 검증은 파일 durability 인수가 아님 |
| S2 실제 기반 | XADB-021 | extent 파일·allocator·buffer·WAL append/flush·doublewrite·mutable tree·row/undo/status | XADB-005/008/028 선행. short I/O·eviction·steal·logical undo·split 통합. 아직 전체 durable commit 주장 금지 |
| S3 거래·영속 통합 | XADB-022 | row/index/catalog·statement-final unique·잠금·두 isolation·commit/CLR·analysis/redo/undo | XADB-009/021 선행. durable ACK/UNKNOWN·재장애·독립 history와 full-check; 이 뒤 실제 disk capability 판단 |
| S4 checkpoint·공간·backup | XADB-029 | fuzzy ATT/DPT checkpoint·WAL floor·undo/status/delete aging·offline exact backup/restore | XADB-022/028 선행. 긴 reader/transaction·낮은 공간·begin-WAL 삭제 뒤 undo·재사용 generation |
| S5 전체 인수 | 네 task 전체 | [최종 DEV3 전수 inventory](dev3-test-plan.md#final-suite): DS01–06·RC-T12–14·KU09/10·PF09·실제 변경한 하위 공급 수신과 재현 bundle | stage-completion의 모든 의무와 독립 검토. 일부 leaf PASS로 제품 S1 승격 금지 |

<a id="parallel-development"></a>
### 2.1 조건부 병렬 범위

공통 page/identity/WAL/undo/visibility API를 먼저 합친다. **기반 구현이 서로 떨어져 성공해도 durability를 증명하지 않는다.** 아래는 역할 계획이며 실제 배정·병렬 pilot·속도 측정은 NOT_RUN이다. 구현 전에 F07에서 실제 담당·좁은 파일 소유를 정하고 하나의 통합 담당을 지정한다.

| 역할 | 책임 | 선행·병렬 경계 |
| --- | --- | --- |
| D3-C 공통 계약 | F01/F02와 page/UndoRef/WAL·오류 effect signature | 모든 소비자보다 먼저 통합. 모듈별 quota를 공통 한도 이상으로 중복 배정하지 않음 |
| D3-P page/allocator | 파일 namespace·extent·generation·allocation metadata | 공통 codec 뒤 D3-W/B와 병렬. allocator init/free WAL 규약 선행 |
| D3-W WAL/undo 표현 | WAL append·complete frame·LSN·undo/status record·CLR 진행점 | D3-P/B와 API를 먼저 동결. 별도 commit 경로 생성 금지 |
| D3-B buffer/flush | pin/content latch·dirty/recLSN·clock·doublewrite·실제 sync | P/W와 병렬 준비 후 WAL→DWB→home 순서 공동 통합. 실제 torn-page 검사 필수 |
| D3-T tree/row | stable anchor·split SMO·logical undo·row overflow/locator | P/W/B 최소 실제 경로와 표현 leaf 뒤. 공통 tree 작성자 한 명; row/index에 복사 구현 금지 |
| D3-X index | versioned presence·key lock·statement-final uniqueness·cursor/build | T의 snapshot/undo primitive 뒤 catalog와 분리 가능 |
| D3-CAT catalog | versioned definitions·namespace validation·object retirement | T의 최소 경로와 row/index identity API 뒤 X와 병렬 |
| D3-I transaction/durable | 잠금·isolation·publication·commit/restart·checkpoint/aging/backup 조립 | S3/S4의 상태 전이는 한 담당이 순차 통합. helper/독립 fault 준비만 분리 |
| D3-V 독립 검증 | typed tuple/history·byte vectors·negative mutants·fault recipe | S0부터 동행. production codec/replay 결과를 기대값으로 재사용하지 않음 |

새 빈 소스 경로·crate를 등록하지 않는다. 기존 storage/transaction/durable/catalog 역할 안 실제 코드가 생길 때만 배치·지문 inventory를 확장한다. pack 전용 D3-P와 path-copy 전용 D3-T의 이전 역할은 폐기한다.

<a id="parallel-waves"></a>
### 2.2 병합 순서

공통 계약 → 표현 leaf → P/W/B 실제 WAL/flush와 해당 연산의 제한 복구 → 일반 buffer와 T row/undo/status → 첫 행 대상의 statement rollback·commit·전체 복구 장벽 연결 → X/CAT 확장과 대상별 거래·복구 통합 → checkpoint/aging/backup → 전체 인수 순서다. index/catalog 전체 완료를 첫 행 거래의 선행으로 두지 않으며, 첫 행 fixture의 실제 영속 object/table 생성·정체성 공급은 필요하다. 공통 tree·transaction/publication·retention을 여러 담당이 동시에 고치지 않는다. 독립 oracle/vector·fault schedule은 production과 병렬 준비할 수 있다. 같은 build의 DS/PF job은 격리 fixture에서 나누되 성능 job의 장치 부하 간섭은 막는다.

이때 [상태 모델](#state-model)은 해당 링크/회수 production leaf 전에, [예약 계산](#resource-proof)은 해당 자원을 소비하는 실제 I/O leaf 전에 완료한다. 여러 담당이 각자의 latch 순서나 reserve를 독립 정의하지 않으며 [공통 변경 순서](../contracts/disk-storage-design.md#mutation-protocol)를 함께 소비한다.

<a id="parallel-entry"></a>
### 2.3 DEV2 수신 경계

F06에서 수신 commit/dirty digest·실제 crate/API/feature·사용한 실행 명령/source/build identity를 기록한다. 이전 공급 측 PASS는 XADB 새 PASS가 아니다. K0 이름만으로 미구현 durable API를 가정하지 않는다. F01/F02/F04와 공급 API가 다르면 해당 owner가 계약을 먼저 조정하고 독립 검토한다. 미확인 소비 작업만 대기하고 독립 준비는 계속한다.

<a id="parallel-ownership"></a>
### 2.4 공유 파일

[병렬 정책](../policies/parallel-development-policy.md#section-2)의 change_coordination을 실제 작업마다 기록한다. page/WAL/UndoRef 공통 표현은 D3-C, tree는 D3-T, transaction/publication/durable은 D3-I가 단일 작성자를 조정한다. foundation effect·platform adapter 변경은 소유자 patch 뒤 소비하며 engine에서 platform 직접 import를 추가하지 않는다. lib/mod·Cargo·runner/checker 등록·README/documents.json은 통합 담당이 최신 원문에 좁게 반영한다. 문서 검사·document-validation.json 생성은 **push gate에서만** 한다. 제품 지문/실행 증거는 합친 실제 코드 기준으로 별도 검증한다.

<a id="remaining-work"></a>
### 2.5 잔여 작업의 공급·완료 경계

각 분류의 기능 요구·입출력/소유·처리 설계도·오류/복구·자원·인수 상세는 [작업별 명세 색인](../contracts/dev3-task-designs.md#catalogue)에서 읽는다. 이 계획은 순서·공급·배정 기준의 정본이며 상세 문서가 기존 owner의 미채택 형식/API/수치를 자동 동결하지 않는다.

3.01–3.16은 [기존 분류](../implementation/dev3-implementation.md#dev3-work-breakdown)의 ID를 유지한다. 번호는 실행 순서가 아니며 XADB-028 → XADB-021 → XADB-022 → XADB-029의 기존 단계 경계를 바꾸지 않는다. 아래 하위 ID는 작업 분해이며 새 API 이름·구현된 runner·완료 증거가 아니다. 미래 leaf의 담당·실제 symbol·변경 경로·고정 입력은 착수 때 채운다.

**공급 완료와 통합 인수를 구분한다.** 선행은 해당 행의 좁은 산출물과 검증 범위만 뜻하며 상위 분류 전체 완료를 뜻하지 않는다. 내부 page/tree/undo 기반을 검증해 공급해도 rollback·복구·공개가 연결되지 않은 사용자 변경 API를 완성으로 공개하지 않는다. 후속 소비자 연결 때 새로 바뀐 경로의 통합 검증을 추가하고 기존 공급 시험의 PASS를 반복하지 않는다. 각 쓰기 경로의 recovery 의미·fault 입력은 해당 leaf 전에 정하며, 실제 미연결 recovery 인수는 명시적으로 남긴다.

<a id="work-entry-exit"></a>
#### 공통 착수·완료 기록

| 기록 | 착수·완료에 필요한 내용 |
| --- | --- |
| 소유·산출물 | 하위 작업 ID, 실제 작업/계약/통합 담당·독립 검토자, 기준 commit/dirty digest, 좁은 변경 경로, 공급할 API/상태 전이와 제외 범위(F07) |
| 선행 수신 | 아래 선행 ID의 실제 공급 symbol·revision·증거 범위. DEV2 소비는 reuse/adapt/DEV2-only/new와 미공급 항목을 F06에 기록 |
| 계약·독립 입력 | 해당 owner 계약, F01 byte·F02 상태/오류/수명, F03 독립 기대값·모델·known-bad. 기존 모델은 원래 source/공간의 증거로 수신하고 새 실행으로 표시하지 않음 |
| 첫 effect 전 | F04의 실제 fault cut·독립 관측과 **해당 opcode의 F05 수치 예약표**를 고정. forward·실패 cleanup·CLR/전체 abort·bootstrap까지 필요한 소유, 동시 생존 메모리/FD/frame/WAL·disk 한도와 반환 시점을 계산. 미정인 opcode는 첫 effect 전에 거절 |
| 검증 범위 | 마지막 관련 검증의 source와 실제 diff를 case별로 연결. 새 코드·변경 경로 및 그 변경에 남은 미실행 의무만 선택하고 명령·입력·source/binary·결과를 보존. 완료 사례의 무변경 재실행, workspace/전체 leaf/하위 회귀 wrapper 일괄 실행 금지 |
| 완료 판정 | 아래 산출물의 정상·경계·오류·해당 실제 I/O/thread/reopen 결과와 독립 검토를 연결. 공급 완료·통합 미완료를 별도 기록하고, 미실행은 NOT_RUN, 기존 FAIL·중단은 원래 결과로 보존. 문서 자동 검사는 push gate에서만 수행 |

F05는 3.16까지 미루는 작업이 아니다. [opcode 자원 계산](#resource-proof)과 [실제 표현 상한](../contracts/dev3-resource-bounds.md)을 각 effect의 선행으로 소비하고 그 구현의 자원·진행 경계를 검증한다. 실제 I/O 결과는 구현 후 얻는 완료 증거이며 이를 사전 실행 PASS로 가정하지 않는다. 3.16은 이미 확보한 항목별 회계의 합과 남은 통합 인수·PF09를 담당한다. 이 문서의 case 연결은 인수 의무의 위치이며 전체 suite 재실행 지시가 아니다.

#### 16개 분류의 계약·기존 task·인수 연결

| 분류 | 소유 계약과 기존 task | 공급·인수 연결 |
| --- | --- | --- |
| 3.01 형식·정체성 | [표현 계약 색인](../contracts/README.md), [F01–03](#freeze); XADB-028 | 소비하는 표현별 독립 bytes·오류 vector. 전체 codec 완료를 모든 leaf의 공통 선행으로 삼지 않음 |
| 3.02 파일·읽기 기반 | [저장 세션](../contracts/dev3-storage-session.md), [page I/O](../contracts/dev3-page-io.md); XADB-021 | 실제 설치·선택 읽기의 공급 경계. 일반 open은 3.12; DS02/03/05 |
| 3.03 공간 할당 | [예약·취소](../contracts/dev3-allocation-reservation.md), [할당 표현](../contracts/dev3-allocation-format.md); XADB-021, 회수 통합 XADB-029 | 아래 3.03.1–4; DS02/03/04/05·RC-T14 |
| 3.04 WAL 변경 경로 | [WAL I/O](../contracts/dev3-wal-io.md), [WAL 표현](../contracts/dev3-wal-format.md), [obligation](../contracts/dev3-obligation-format.md); XADB-021/022 | 아래 3.04.1–2; DS02/05·KU10·RC-T12/14 |
| 3.05 dirty buffer·flush | [clean buffer 공급](../contracts/dev3-read-buffer.md), [buffer·DWB 소유 규약](../contracts/disk-storage-design.md#buffer); XADB-021 | 아래 3.05.1–3; DS02/03/05·PF09 |
| 3.06 트리·행 | [tree](../contracts/dev3-tree-format.md), [row](../contracts/dev3-row-format.md), [overflow](../contracts/dev3-overflow-format.md); XADB-021 | 아래 3.06.1–3; DS01/03·KU09 |
| 3.07 인덱스 | [UNIQUE·statement](../contracts/disk-storage-design.md#keys); XADB-022 | 3.06.2/3·3.09.1·3.10.1의 해당 공급 뒤 presence/point/range와 statement-final PK/UNIQUE, 이후 build/취소를 구분. 대상별 3.09.2/3와 통합해 독립 row↔index 일치·key 교환·부분 NULL·실패 cleanup 인수; DS01/06·KU09/10 |
| 3.08 카탈로그·DDL | [catalog 표현](../contracts/dev3-catalog-format.md), [직접 API](../contracts/disk-storage-design.md#ownership); XADB-022 | 3.06.1·3.09.1·3.10.1의 해당 공급 뒤 정의/이름/수명 변경. 대상별 3.09.2/3 및 3.11/3.12와 create/drop 원자성을 인수하며 retirement는 3.14 연결; DS02/04/06·KU10 |
| 3.09 undo·rollback | [undo](../contracts/dev3-undo-format.md), [상태 home](../contracts/dev3-status-home.md), [rollback](../contracts/disk-storage-design.md#commit); XADB-021/022 | 아래 3.09.1–3; DS02/04/06·RC-T13/14 |
| 3.10 동시성·가시성 | [변경 순서](../contracts/disk-storage-design.md#mutation-protocol), [snapshot 획득](../contracts/disk-storage-design.md#snapshot-acquisition); 공통 guard는 XADB-021, 거래 통합은 XADB-022 | 3.10.1은 F02와 하위 동기화 공급을 수신해 snapshot/publication/registry guard·transient pin·progress turn/잠금 primitive를 먼저 공급. 3.10.2는 3.06·3.09의 해당 공급과 3.11.1을 소비해 두 격리·관측 행 충돌·deadlock victim 통합. 실제 reader/writer/ager·즉시 X-lock 반례; DS04/06·KU10 |
| 3.11 commit·공개 | [WAL·commit](../contracts/disk-storage-design.md#commit); XADB-022 | 3.04.2·3.09.1·3.10.1과 현재 지원하는 대상 변경 공급 뒤3.11.1 commit attempt/sync/status/frontier. 첫 행 공급에 index/catalog 전체 완료를 요구하지 않음.3.11.2는3.09.3·3.10.2·3.12.8과 다중 객체 ACK/UNKNOWN·Fenced를 통합 인수. data flush 없는 commit·외부 witness; DS02·KU10·RC-T12 |
| 3.12 재시작 복구 | [정상 recovery](../contracts/disk-storage-design.md#recovery), [예약 복원](../contracts/disk-storage-design.md#recovery-reservation); XADB-022 | 아래 3.12.1–8; DS02/03/04/05·RC-T12–14 |
| 3.13 checkpoint | [fuzzy checkpoint](../contracts/disk-storage-design.md#checkpoint); XADB-029 | 3.05/3.09/3.11/3.12의 해당 공급 뒤 capture와 current/previous fallback·WAL floor/회수를 분리. 동시 수정/flush와 ATT/DPT 누락 검출; DS05·RC-T14 |
| 3.14 공간 회수 | [참조·회수](../contracts/disk-storage-design.md#undo-lifecycle), [자원 진행](../contracts/disk-storage-design.md#reclaim); XADB-029 | 3.14.1은 3.05.3·3.09.1·3.10.1·3.11.1의 해당 공급 뒤 eligibility/horizon·마지막 pin/영속 edge/DWB 참조 판정, 3.14.2는 3.03.4와 실제 해제·재사용/압력 해소 인수. 긴 reader/Active 거래·begin-WAL 제거 뒤 abort 보호; DS03/04/05·RC-T13/14 |
| 3.15 백업·무결성 | [full audit](../contracts/disk-storage-design.md#recovery), [exact-cut backup](../contracts/disk-storage-design.md#checkpoint); 검증 기반은 XADB-022, backup은 XADB-029 | 3.15.1 bounded full audit·독립 이력은 S3/fault 인수까지 공급, 3.15.2 offline exact-C backup/restore는 영속 통합 뒤. 정상 open과 audit 비용 분리, source-WAL 없는 restore→새 ACK→reopen; DS02/03/05 |
| 3.16 자원 종합·성능·최종 인수 | [F05](#resource-proof), [PF09](#performance), [완료 정본](stage-completion.json); 네 task 공통 | 개별 opcode의 선행 계산을 취합해 동시 한도/진행 검산, 남은 PF09·DS01–06·KU09/10·RC-T12–14 의무를 연결. 필수 인수에 미실행·미해결 실패가 남으면 S1 미완료. 해결된 과거 FAIL·중단 원본도 보존 |

3.10.1은 3.05/3.09의 완료를 선행으로 요구하지 않는다. guard/turn 자체의 수명·순서·thread 증거를 먼저 공급하고, 실제 flusher drain·undo·commit과의 의미는 각 소비 경로 및 3.10.2에서 통합 인수한다. 3.14.1도 실제 회수 가능성 판정을 먼저 공급하고 free/reuse 결과는 3.14.2에서 연결한다. 3.15.1의 검사기·독립 기대값 준비는 S0부터 진행하며 backup 완료를 기다리지 않는다. 정상 open에 변경 없는 cold table 전수 검사를 추가하지 않는다.

<a id="dev5-supply"></a>
#### DEV5 소비를 위한 engine 공급 배정

[DEV5 D5-S02/S05 공급 대장](dev5-work-breakdown.md#supply)은 기존 동시성·DDL·거래 책임을 아래 작업에 연결한다. 서버용 알고리즘을 별도 구현하거나 DEV3/S1의 완료 분모를 새 서버 인수로 바꾸지 않는다. 실제 API·signature·owner·별도 검토·변경 코드 시험은 해당 F02와 D5-F03의 소비 전 동결하며 공급 상태는 OPEN_SUPPLY다.

| 공급 | 기존 작업·소유 | DEV5 수신 경계 |
| --- | --- | --- |
| D5-S02 row/key 대기 | 3.10.1.a 잠금 primitive, 3.07.2 key/UNIQUE 대기, 3.10.2.b/c 실제 충돌·victim/취소 | 조건 검사+wait 등록·generation·unlock/cancel/unregister·stale wake의 소유. page/tree latch·부분 pin 없이 대기를 반환하고 holder의 종료가 worker 부족으로 막히지 않도록 SQL이 중단 가능한 경계 공급 |
| D5-S02 offline build/DDL drain | 3.07.3 build, 3.08.2 native DDL, 3.10.1.c admission/drain | 기존 거래/result drain과 막힌 새 admission의 대기 등록·취소를 같은 소유 규약에 연결. SQL/server는 private lock이나 재실행으로 우회하지 않음 |
| D5-S05 종료·복구 소유 | 3.09.3 rollback, 3.11.1/2 commit, 3.12.8 recovery 인계 | durable+publish·undo 완료와 UNKNOWN/NotCommittedRecoveryRequired/fence를 실제 receipt·소유로 구분. 정상 해제 전 재사용 금지·중복 cleanup·재장애의 원래 증거 수신 |

잠금 대기·wait-for graph가 이미 있다는 사실만으로 continuation/worker 반환 공급 완료가 아니다. DEV3 공급은 좁은 native 인수로 먼저 확인하고 SQL의 보존 실행 소유는 D5-S03, 실제4-worker holder/DDL 진행은5.08.3이 담당한다. 새 검증은 해당 구현 diff에 한정하며 무변경 완료 시험을 반복하지 않는다. 이번 배정은 제품 개발·시험 재개가 아니다.

<a id="allocation-remaining"></a>
#### 3.03–3.05: 할당·WAL·dirty/flush 공급

| ID·산출물 | 직접 선행 | 완료 조건·남길 경계 |
| --- | --- | --- |
| 3.03.1 예약·취소 부분 구현의 잔여 인수 | [부분 source·case 기록](../evidence/dev3-allocation-reservation-partial-xadb-20261021.json), [독립 검토의 remaining](../evidence/dev3-allocation-reservation-partial-review-xadb-20261021.json) | native 2그룹/13 Linux DB의 adapter·runner 구현과 실행, compiler baseline+6 probe runner 구현과 실행, 수정된 WAL stage 관련4시험, **generic nested-local 동시 생존량의 component64KiB 측정·수명 증명**, 독립 검토. [2026-10-22 관련 Clippy 확인](../implementation/dev3-implementation.md#allocation-lint-fix) 이후 추가 코드 diff의 필요한 정적 검사만 선택. 원래 runtime 결과와 합쳐 제한 allocation 단위를 판정하며 INIT/home/recovery/READY/거래 ACK를 제공했다고 하지 않음 |
| 3.03.2 Reserved→INIT 소비 | 3.03.1, 3.04.1, 3.05.1, 해당 page 표현 | token·generation·descriptor·page INIT를 같은 복구 가능한 WAL 그룹으로 인계하고 오류/취소에서 소유 보존. 해당 opcode의 독립 bytes·fault/예약 경계 |
| 3.03.3 extent 확장·FILEMAP 인계 | 3.03.2, 3.05.3, 3.02의 namespace/selector 공급 | 파일 성장·선택 metadata·WAL/예약 소유의 부분 I/O·불명 결과를 검증. 선택 전후 closure와 실제 charged bytes 보존 |
| 3.03.4 eligible page free/reuse | 3.14.1의 회수 가능성 판정, 3.03.2, 3.04.1, 3.05.3 | 마지막 transient pin·영속 edge·pending DWB 참조가 제거된 대상만 free/reuse. generation 증가·stale reference 거절·새 incarnation INIT의 WAL 원자 인계. 3.14.2의 전체 회수 인수는 후속 |
| 3.04.1 semantic stage/apply 공통 경계 | 해당 3.01 표현, 기존 fresh WAL·fence 공급, 3.10.1의 변경 guard | caller 준비→stage→authoritative page/descriptor/recLSN 소유 인계, stage 직후 unwind·오류에도 예약 보존. opcode별 producer는 해당 소비 작업에서 추가 |
| 3.04.2 일반 append/sync·owner chain | 3.04.1, 해당 frame/owner 표현 | interleaved 완결 frame·LSN/owner predecessor·append/sync/불명 결과를 검증. CLR·commit의 의미 생산과 공개 인수는 3.09/3.11에 귀속 |
| 3.05.1 dirty 등록·pin/latch·recLSN | 3.04.1, 3.10.1의 guard/turn, 기존 selected clean buffer | authoritative frame의 dirty 전환·첫 recLSN·redirty·pin 수명과 실패 소유 보존. 아직 home flush 완료를 주장하지 않음 |
| 3.05.2 WAL→DWB→home flush | 3.05.1, 3.04.2, 기존 Empty DWB 설치 | WAL durable 경계 후 image sync·pending 선택·다중 file home write/sync. batch1/8/64·torn/부분 I/O·redirty에서 pending과 예약 보존 |
| 3.05.3 retirement·eviction·압력하 진행 | 3.05.2, 3.10.1의 progress turn | 모든 file sync 후 whole-batch retirement, pin 없는 frame 교체, bounded scan/service·pressure drain에서 기존 작업의 진행. repair/recrash 통합은 3.12.2/8과 판정 |

3.03.1의 위 잔여 목록은 분해 당시의 인수 범위이며, 현재는 [예약·취소 인수](../implementation/dev3-implementation.md#allocation-reservation-accepted)와 [Status home flush](../implementation/dev3-implementation.md#status-home-flush)까지 제한 공급을 마쳤다. 이를 새 재개 작업이나 무변경 재실행 목록으로 사용하지 않는다. 일반 INIT·확장·free/reuse·pool은 여전히 남아 있으며 현재 선택은 [우선 실행 묶음](#work-priority)을 따른다. source delta와 원래 실패를 보존한다.

<a id="tree-undo-remaining"></a>
#### 3.06·3.09: 공통 tree·행·undo의 순환 없는 공급

| ID·산출물 | 직접 선행 | 완료 조건·남길 경계 |
| --- | --- | --- |
| 3.06.1 공통 mutable tree 구조 경로 | 3.03.2, 3.04.2, 3.05.3, 해당 tree/anchor 표현 | page 탐색·구조 변경·split/root·빈 leaf의 구조적 unlink/retirement 등록을 system 그룹으로 처리하고 bounded pin·stable anchor 보존. 실제 공간 반환은 3.14.1→3.03.4→3.14.2에서 연결하며 사용자 거래의 행 undo 완료와 구분 |
| 3.09.1 live undo/status 기반 | 3.03.2, 3.04.2, 3.05.3, undo/status/obligation 표현 | undo record/chunk·상태 home·head/edge·잔여 예약을 영속 그룹으로 연결. 아직 모든 행/인덱스 rollback 완료를 주장하지 않음 |
| 3.06.2 행 CRUD·overflow 내부 변경 | 3.06.1, 3.09.1, 3.10.1의 필요한 guard/잠금 공급 | row/overflow before·after·UndoRef/관측 identity를 연결하고 공간·오류 경계를 검증. 사용자 변경 API의 완료는 3.09.2/3·3.10.2와 통합 뒤 판정 |
| 3.06.3 stable cursor·locator | 3.06.1/2, 3.10.1의 snapshot/guard 공급 | split/delete/이동에도 anchor·RowUID와 snapshot 결과 보존, locator working set·메모리 상한 검증. 인덱스 소비는 3.07에서 연결 |
| 3.09.2 logical undo·CLR 단위 적용 | 3.09.1, 해당 3.06.2/3.07/3.08 내부 변경 공급, 3.04.2 | 현재 물리 위치의 undo 효과·UndoNextRef·obligation을 같은 CLR 그룹으로 적용. 다른 거래/완결 SMO 보존, 재삽입 split 예약, redo-only·멱등 진행 검증 |
| 3.09.3 statement/full rollback 종료 | 3.09.2, 3.10.1의 lock/guard 공급 | statement mark까지 복원해 이전 statement 보존, 전체 abort는 durable abort-end·상태 확인 뒤 잠금 해제. WAL/CLR·SMO·retained lock 회계를 mark로 되감지 않으며 오류는 Fenced/RecoveryRequired. 반복 crash는 3.12.7/8에서 연결 |

행→인덱스→catalog 등 새 대상 opcode를 붙일 때마다 해당 producer와 3.09.2/3의 소비를 하나의 통합 단위로 닫는다. 아직 없는 모든 미래 대상의 rollback을 tree/undo primitive 공급의 선행으로 삼지 않는다. 반대로 primitive의 제한 검증만으로 그 대상의 statement 원자성을 완료 처리하지 않는다.

<a id="recovery-remaining"></a>
#### 3.12: 복구 장벽별 구현과 통합 인수

[정본의 장벽 순서](../contracts/disk-storage-design.md#recovery)를 그대로 사용한다. 각 행은 앞 장벽의 공급을 소비하며 최초 분석용 선택 control/표현은 해당 3.01/3.02에서 준비한다. 정상 운영의 fuzzy capture 생산·WAL 회수와의 통합은 3.13에서 추가한다. **RECOVERY_REBASE를 3.13 전체 완료 뒤로 미루지 않는다.**

| ID·산출물 | 직접 선행 | 완료 조건·인수 경계 |
| --- | --- | --- |
| 3.12.1 LEASE·PREFLIGHT/SELECT·BOOT_RESERVE | 3.02의 lease/선택 공급, 해당 control/profile 표현·F05 | DB write 전에 profile·실제 자원·namespace 검증 및 고정 bootstrap 예약. 낮춘 foreground 한도로 기존 obligation을 축소하지 않고 admission은 닫음 |
| 3.12.2 DOUBLEWRITE_REPAIR | 3.12.1, 3.05.2/3의 pending/retirement 공급 | 선택된 pending image로 torn home 복원·모든 file sync·retirement. 유효 pending이 없어도 FILEMAP/allocation generation과 일치하는 완전 INIT 및 채택 끝까지의 WAL 전체가 보존되면 redo에서 재구성하며, 그 밖의 initialized home 손상은 거절. repair 중 재장애는 3.12.8 |
| 3.12.3 ANALYSIS·RESERVE_RESTORE | 3.12.2, 해당 WAL/descriptor/선택 checkpoint 입력 | capture 직후부터 채택 prefix의 ATT/loser/UndoRef·예약 inventory 재구성. stale home allocator를 최신 소유로 사용하지 않으며 **REDO 추가 효과 전에** 잔여 예약을 한 번 인계 |
| 3.12.4 streaming REDO | 3.12.3, 3.04/3.05와 해당 변경 opcode | complete WAL·CLR을 incarnation/pageLSN으로 적용. 전체 DPT를 RAM에 적재하지 않고 source WAL 불변·tail/손상 판정·WAL-before-page를 검증 |
| 3.12.5 RECOVERY_REBASE | 3.12.4, 3.05.3, 전용 두 슬롯/selector 표현·고정 예약 | home/undo/status/allocator flush 후 self-contained base 인계. current/previous·공통 fallback·base-only·old/new 합집합을 보존하고 안전한 회수 후 다음 epoch 여유 확보. 새 WAL에 의존하지 않음 |
| 3.12.6 RECOVERY_LOG_OPEN | 3.12.5, 3.04.2, epoch/header·WRITER namespace 공급 | base의 완결 LSN 다음 epoch/header sync 및 WRITER 영속 선택. 이 선택 전 새 CLR append 및 그 효과의 home write 금지; 재시작 시 새 epoch 발견 가능 |
| 3.12.7 UNDO·VERIFY_RECOVERY | 3.12.6, 해당 3.09.2/3 | loser만 남은 UndoNextRef부터 처리하고 완료 CLR 재실행으로 효과 중복 금지. terminal/예약·실제 복구 page 검증, 미완료 undo를 ABORTED로 응답하지 않음 |
| 3.12.8 STABILIZE·RESUME/READY·재장애 인수 | 3.12.7, 3.11.1의 commit/공개 공급, 3.15.1 | base+WAL/CLR/epoch/namespace 영속 장벽 뒤에만 admission 재개. 각 장벽과 epoch 선택 직후·부분 CLR에서 반복 crash, 새 ACK 뒤 reopen, 독립 이력/full audit. 고정 peak와 정상 admitted 복구 성공을 손상/I/O 거절 사례와 분리 |

3.12.8의 전체 연결 전에 각 구현된 장벽의 변경 경로와 fault를 좁게 검증한다. 마지막에 증거를 종합한다는 이유로 완료된 무변경 사례를 다시 실행하지 않는다. source-WAL 없는 restore는 3.15.2, begin-WAL 제거 뒤 Active/부분 CLR 복구는 3.13/3.14 연결 인수로 이어지며, 이 의무가 남으면 전체 DEV3는 미완료다.

<a id="work-breakdown-review"></a>
#### D3-WORK-BREAKDOWN-001 — 작업 분해 보완 기록

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22. 기존 owner 계약의 공급·인수 배정 구체화이며 새 제품 의미/형식/한도 결정은 없음 |
| 권한·소유 | 사용자의 DEV3 분류 검증 후 보완 요청. 작업/계약/통합 `/root`, 독립 검토 `/root/dev3_breakdown_review`; 기준 HEAD `a285043b91571cef83dacda49fcf2a63e99160d2`, 착수 worktree clean |
| 선택·대안 | 16개 기능 분류는 유지하고 이 계획에 실제 선행·산출물·완료 조건을 배정. 번호순 전체 완료 방식은 undo/회수의 공급 순환과 F05 지연을 만들므로 채택하지 않음 |
| 범위·연결 | 이 계획, 구현 현황의 현재 분류, 진입/폴더 색인, DEV3 단계 JSON의 계획 참조. 기존 문서 ID/경로/제목과 task DAG·S1 인수 조건 유지; 새 runner·product path·원본 evidence 변경 없음 |
| 검토·검증 | `/root/dev3_breakdown_review`가 7개 문서의 diff·소유 계약·원본 부분 검토를 읽기 전용 대조하여 PASS, 필수 수정 지적 없음. anchor·INIT 복원 예외·reuse의 INIT 선행·과거 FAIL 구분과 tree/guard/commit 공급 순환 방지 보완을 확인했고 `/root`가 반영 확인. `git diff --check` PASS. 제품 코드 변경·제품 시험·compiler/Clippy·문서 자동 검사·push 없음. 검토 PASS는 계획 정합성에 한정 |
| 대체·재검토 | 구현 기록의 분류 초안을 실행 순서로 읽는 해석을 이 절로 구체화. owner/API 변경, 선행 순환, 인수 case 미배정, 신규 opcode 자원표 누락 시 재검토 |

<a id="work-organization"></a>
### 2.6 작업 크기·우선순위·병렬 운영

**16개 대분류는 유지한다.** 기능의 위치와 책임을 찾는 분류로 사용하고, 실제 배정·검토·완료는 하위 작업으로 관리한다. 개수를 맞추기 위한 합치기/분할이나 기존 ID 재번호 부여는 하지 않는다. 3.01 표현, 각 opcode의 F05, F03 독립 검증과 3.15.1 audit 준비는 여러 개발 구간에서 소비한다. 3.16은 이미 확보한 증거의 종합과 남은 인수이며 이 활동들을 마지막까지 미루는 장소가 아니다.

분류마다 분량·위험이 다르므로 **완료 분류 수/16 또는 완료 하위 작업 수로 DEV3 진행률을 계산하지 않는다.** 현재 보고에는 제공된 capability, 검증 범위/source, 다음 소비자를 막는 미공급 항목, 미실행·미해결 인수 의무를 적는다. 예상 작업량은 변경할 상태 전이·공유 API 수·fault/경계 사례·미정 설계와 검토 부담으로 설명하고, 미측정 시간이나 병렬 속도 향상을 확정값으로 제시하지 않는다.

<a id="work-sizing"></a>
#### 작업 크기와 추가 분해

한 작업은 **명확한 불변식 또는 상태 전이를 구현하고 정상·실패·자원 수명을 독립적으로 검토할 수 있는 크기**로 닫는다. 코드 줄 수·함수 수·commit 수를 기준으로 균등 분할하지 않는다. 한 불변식을 여러 함수가 함께 구현하면 같은 작업으로 유지하고, 서로 다른 산출물·검증 환경·소비자를 한 작업이 모두 기다리게 되면 나눈다. 실제 담당은 [공통 착수 기록](#work-entry-exit)에 좁은 경로·선행·완료 사례를 확정한다.

아래는 앞 절의 큰 묶음을 배정할 때 사용하는 추가 하위 ID다. 기존 상위 ID·계약·인수 의무는 유지하고, 상위의 완료에는 적용되는 모든 하위 의무가 필요하다. 하위 분할로 fault 분모를 줄이거나 이미 성공한 시험을 다시 실행하지 않는다. 새 ID는 아직 구현·배정·실행 결과가 아니다.

| 상위 묶음 | 배정 가능한 하위 작업 | 닫는 경계·공유 제한 |
| --- | --- | --- |
| 3.03.1 예약·취소 잔여 인수 | 3.03.1.a native adapter/runner·13DB, .b compiler baseline+6 probes, .c component64KiB 동시 생존량/수명 증명, .d 변경 WAL stage4시험·관련 quality·단위 결과 취합 | a/b는 별도 전용 경로에서 병렬 준비. c에서 제품 수정이 필요하면 해당 source를 소비하는 작업과 먼저 조정. d는 원래 source별 결과와 새 delta를 연결하며 독립 검토 뒤 상위 완료 판정 |
| 3.07 인덱스 | 3.07.1 presence·point/range/full scan, 3.07.2 PK/statement-final UNIQUE·key lock, 3.07.3 build·취소/실패 cleanup | 3.07.1과 해당 tree/undo 공급 뒤 나머지 연결. 3.07.2는 부분 NULL·key 교환·최종 중복·이전 statement 보존, 3.07.3은 bounded staging·FD·취소 소유 반환. 공통 tree 복제 금지 |
| 3.08 카탈로그·DDL | 3.08.1 정의/이름·versioned catalog 내부 변경, 3.08.2 native create/drop 원자성·객체 retirement 인계 | 3.08.1은 tree/undo primitive로 공급하고 공개 DDL 인수는 rollback·commit/recovery 연결 뒤. index build와 catalog 공개 순서는 한 통합 담당이 연결 |
| 3.10.1 동시성 primitive | 3.10.1.a guard/rank·잠금 기반, .b snapshot/publication/registry·transient pin, .c FIFO progress turn·admission/drain | 계약상 공통 rank와 guard API를 먼저 합친 뒤 구현. 실제 buffer/undo·commit을 소비하는 통합은 각 후속 작업에서 수행. 전체 거래 인수를 이 primitive의 선행으로 되돌리지 않음 |
| 3.10.2 동시성 통합 | 3.10.2.a 두 격리 수준·statement snapshot, .b 관측 행 버전·즉시/대기 X-lock 충돌, .c deadlock victim·취소/전체 undo | 3.11.1 등 앞 절의 실제 공급을 소비하고 사례별 정상 이력/전체 abort를 확인. transaction/lock 공통 상태를 동시에 수정하지 않으며 파일이 나뉘어도 의미 변경은 조정 |
| 3.12.8 복구 종료·재장애 | 3.12.8.a STABILIZE·RESUME/READY 구현, .b 장벽별 재장애·새 ACK/reopen·audit 연결 인수 | a는 영속 장벽과 admission을, b는 실제 도달 cut·정상 복구/거절·고정 peak와 독립 이력을 닫음. fault 입력/checker 준비는 각 장벽 구현 전부터 진행 |
| 3.13 checkpoint | 3.13.1 fuzzy ATT/DPT capture, 3.13.2 current/previous 선택·fallback, 3.13.3 WAL floor·안전한 회수 | capture→선택 인계→회수 순서. 동시 수정/flush·ATT/DPT 누락·previous closure 보호를 연결. 3.12.5 복구 전용 REBASE와 별도 |
| 3.14 회수, 3.15 백업·무결성 | 기존 3.14.1 eligibility와 3.14.2 실제 해제/재사용 유지; 3.15.1 audit 공급, 3.15.2.a offline exact-C backup, .b restore→새 ACK→reopen | eligibility는 실제 반환과 구분하고 free/reuse를 통해 통합. audit 준비는 S0부터, 제품 검사는 해당 데이터 경로 공급 뒤. backup 생산과 source-WAL 없는 restore 인수도 구분 |

3.06 tree·행, 3.09 undo 및 3.12의 나머지 장벽도 앞 절의 하위 작업을 기준으로 한다. 예를 들어 split과 root 변경의 실패 상태가 별도 독립 작업이면 착수 때 더 나누되, 작업 수가 같아지도록 전체 DEV3의 leaf 수를 미리 고정하지 않는다.

<a id="work-priority"></a>
#### 우선 실행 묶음

우선순위는 다른 작업을 여는 공급, 실패 시 재작업이 큰 불변식, 현재 부분 구현의 미완료 의무를 기준으로 정한다. 아래 순서는 작업 선택 기준이며 새로운 DEV 단계나 각 묶음 전체 완료 장벽이 아니다. 구체 진입은 [직접 선행](#remaining-work)을 따르고, 독립 준비는 필요한 실제 공급을 기다리는 동안 진행할 수 있다.

| 우선순위 | 먼저 닫을 공급·통합 | 함께 준비할 것 |
| --- | --- | --- |
| 1 | 완료된 [N=0 plan4 복구](../evidence/dev3-recovery-resumed-20261025.md)를 공급으로 수신하고, 일반3.05 page 수명·필요3.10.1 guard·3.09.1 live undo/status·3.06.1 tree 최소 공급을 보완 | 원50개 source별 PASS/실패 보존. 완료 사례 재실행 없이 새 consumer의 F05·대상 page 권한·eviction/reload를 검증 |
| 2 | 최소 공통 terminal→3.08 최소 Catalog CREATE→실제 영속 table/anchor 공급 | typed XCB1 codec 공급을 소비하되 Catalog effect/CLR·commit/recovery는 별도 연결. 전체 DDL/index build 완료를 선행하지 않음 |
| 3 | [첫 행 거래](../contracts/dev3-task-11-commit.md#first-row-integration)의 하나의3.06.2 opcode→3.09.2/3→3.11.1→3.12.5–8과 필요한3.10 통합 | effect 전 opcode별 F05. 작은 no-force ACK/steal loser/statement 실패·CLR 재장애, quota 경계 복구. terminal queue·REBASE·epoch·VERIFY/STABILIZE 생략 금지 |
| 4 | 나머지 row CRUD·3.07 index·3.08 DDL와 대상별 rollback/commit/recovery·3.10.2·3.11.2 다중 객체 | 새 opcode마다 독립 이력·오류/장애를 확장. 같은 table Serializable 충돌과 Commit 장벽 계측을 함께 공급 |
| 5 | 3.13 immutable capture/current/previous·3.14 aging/3.03.4 reuse·3.15.2 typed rewrite restore·3.16 남은 PF09/S5 | 실제 행 경로 뒤 BEGIN-WAL 삭제·Active/부분 CLR·retained credit 통합.64GiB profile·큰 fixture 준비는 각 소비 구현 전에 착수, 장시간·대규모·전체 조합 실행은 S5 |

자원 증명·독립 검증·fault 설계는 전 구간에 걸쳐 수행한다. 개발 중 아직 제공하지 않은 durability/rollback/복구 기능의 인수는 명시적으로 남기며, 뒤 묶음에 있다는 이유로 해당 effect의 F05나 필수 실패 처리를 미루지 않는다.

<a id="parallel-execution-plan"></a>
#### 초기 병렬 배치의 기록과 후속 순차 통합

아래3.03.1 전용 역할표는 당시 계획을 보존한 것이다. 해당 제한 인수는 완료됐으므로 그대로 재배정하지 않는다. 후속 작업은 위 현재 우선순위의 실제 변경 경로에 맞춰 F07 담당·독립 검토를 새로 정한다.

DEV3 재개 시 기본 배치는 **총괄·통합 1명 + 작업 담당 2명 + 독립 검토자 1명**으로 계획한다. 동시에 진행하는 구현 작업은 우선 두 개로 제한하고, 총괄도 제품 구현에 들어가면 그 작업을 같은 수에 포함해 배정을 조정한다. 읽기 전용 분석·검토는 함께 진행할 수 있다. 인원 수에 따른 속도 향상은 측정하지 않았으며, 실제 검토 대기·공통 수정·병합 재작업과 자원 경합을 보고 이후 배치를 조정한다.

| 역할 후보 | 당시 3.03.1의 담당 범위 | 병합·검토 경계 |
| --- | --- | --- |
| 총괄·통합 | 64KiB 분석/증명, 공통 제품 수정 조정, 변경 WAL stage4시험·quality 범위 선정·결과 취합 | 공통 owner/stage·모듈/runner 등록·색인 단일 작성. 제품 수정 시 구현 작업 수를 조정하며 그 수정도 별도 검토자가 확인 |
| 작업 담당 A | native adapter/runner와 고정 2그룹/13 Linux DB 사례 | 전용 native 경로만 소유. production encoder를 정답으로 쓰지 않고 실제 bytes/오류/fence를 독립 기대값과 대조 |
| 작업 담당 B | compiler baseline·6개 수명 probe와 runner | 전용 compiler fixture/driver만 소유. 공유 API 수정은 총괄에 전달하고 자기 시험을 위해 다른 담당 소스를 수정하지 않음 |
| 독립 검토자 | 계약·독립 입력·각 diff·실행 결과·source/case 범위 | 자신의 구현을 독립 검토하지 않음. 검토자가 수정 구현을 맡으면 그 부분의 별도 검토를 다시 배정 |

착수 시 F07에 실제 담당·기준 commit/dirty digest·존재하는 전용/공유 경로·병합 순서를 기록한다. 위 역할표 당시 미공급이던 native/compiler runner의 실제 완료는 [예약·취소 인수](../implementation/dev3-implementation.md#allocation-reservation-accepted)를 따른다. 후속 작업의 runner도 존재·선택 가능 여부를 실제 source에서 확인한다. 공통 API/형식/입력 변경은 단일 담당이 먼저 합치고 소비자가 그 기준을 수신한다. 서로 다른 파일이나 worktree도 같은 의미를 바꾸면 독립 작업이 아니다.

병렬 준비는 같은 mutable source에서 임의로 실행하면서 결과를 합치는 방식으로 하지 않는다. 각 실행의 source/binary/fixture와 case를 고정·보존하고, 공유 코드가 바뀌면 영향받는 작업의 입력을 갱신해 **실제 변경 경로만** 다시 검증한다. native와 compiler의 실제 동시 실행은 격리 fixture·출력/build 경로 및 사전 고정 자원/timeout profile이 허용할 때만 선택하고, 경합이 관측되면 순차 실행한다. PF09 측정과 장치 부하 작업을 겹쳐 성능 결과를 왜곡하지 않는다. 기존 결과를 최종 source에서 새로 실행한 PASS로 바꾸지 않는다.

| 후속 영역 | 병렬로 준비 가능한 범위 | 순차로 연결할 경계 |
| --- | --- | --- |
| allocator·WAL·buffer | 공통 guard/page/WAL·예약 API 동결 뒤 전용 구현·독립 fault 입력 | 같은 owner/stage 상태 수정, WAL→DWB→home·retirement 인계 |
| tree·undo/status | 최소 실제 할당/WAL/flush 공급 뒤 서로 다른 전용 경로 | 공통 tree 작성, row/UndoRef/head·obligation의 원자 변경 |
| 인덱스·catalog | tree·undo·snapshot 공급 뒤 전용 presence/정의 경로 | 공통 tree 수정, index build/DDL 공개·실패 rollback |
| commit·recovery·checkpoint·회수 | 독립 oracle·fault recipe·해시/receipt 등 전용 검증 준비 | transaction/publication·base/epoch/selector·WAL floor/retention 전이는 한 통합 담당이 순차 반영 |

병렬 병합이 끝났다는 이유만으로 전체 workspace·leaf·하위 회귀를 실행하지 않는다. [변경 코드 기준](../../AGENTS.md)에 따라 필요한 실제 통합 경로와 남은 의무만 선택한다. 선행 API가 계속 바뀌거나 두 작업이 같은 상태 전이를 반복 수정하면 해당 영역을 먼저 순차화하고, 분리 가능한 전용 준비·독립 검토를 계속한다.

<a id="work-organization-review"></a>
#### D3-WORK-BREAKDOWN-002 — 작업 크기·우선순위·병렬 운영

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22. DEV3 내부 작업 배정 기준 구체화; 제품 의미/형식/인수 변경 없음 |
| 권한·소유 | 사용자의 개수·순서·구성·분량·우선순위·병렬 가능성 검토 후 문서 수정 요청. 작업/계약/통합 `/root`, 독립 검토 `/root/dev3_breakdown_review` |
| 기준·기존 변경 | HEAD `a285043b91571cef83dacda49fcf2a63e99160d2`; 앞선 7개 문서 dirty 변경과 D3-WORK-BREAKDOWN-001 보존. 시작 사본 `/tmp/xadb-dev3-work-priority-wa0cxb4j`, 기존 diff SHA-256 `08de68896ff77638662b01e63026ccae51ba556947a2d7efb87c5392feff9410` |
| 선택·대안 | 16개 기능 분류 유지·작은 검증 단위 배정·공급 중심 우선순위·초기 구현 작업2개. 번호순 실행, 작업 수 기반 진행률, 공유 상태의 무조정 동시 수정은 채택하지 않음 |
| 범위·연결 | 기존 계획·현재 상태의 안내·진입/폴더 색인·DEV3 단계 JSON 계획 참조. 기존 문서 ID/경로/제목 유지; 새 제품 파일·runner·원본 evidence 수정 없음 |
| 검토·검증 | `/root/dev3_breakdown_review`가 시작 사본 대비 7개 문서의 변경을 읽기 전용 검토하여 PASS, 필수 수정 지적 없음. 기존 16개 분류/공급 경계·001 보존, 추가 분해·우선순위의 비차단 준비, 구현 작업 수·공유 코드 순차 통합·source별 증거/변경 경로 검증을 확인했고 `/root`가 반영 확인. `git diff --check` PASS. 제품 개발/병렬 배정 실행·시험·compiler/Clippy·문서 자동 검사·push 없음 |
| 관계·재검토 | D3-WORK-BREAKDOWN-001의 공급/완료 경계를 유지하며 배정 크기와 순서를 보완. 공통 API 변경·선행 순환·검토 적체·병합 재작업·실행 자원 경합 시 배치를 재검토 |

<a id="direction-review-adoption"></a>
### 2.7 D3-DIRECTION-001 — 방향 리뷰의 개발 순서·완료 조건 반영

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22. 실제 변경의 별도 독립 검토 PASS, 작업·계약·통합 담당 반영 확인 |
| 권한·소유 | 사용자의 [방향 리뷰](../evidence/dev3-direction-review-20261022.md) 반영·방향/구성/지적사항 해결 요청. 작업·계약·통합 `/root`, 별도 읽기 전용 검토 `/root/direction_independent_review` |
| 기준·보존 | HEAD `c859fbd645950095efdbfc17d247c0923644381b`; 기존 README/documents.json/evidence 색인·방향 리뷰의 미커밋 변경 보존. 시작 파일별 SHA-256·상태 사본 `/tmp/xadb-dev3-direction-before-72ppo8fc/manifest.json` |
| 결정 | in-place·B+tree·단일 WAL·영속 undo·DWB와 오류/예약 장벽 유지. 현재 쓰기의 제한 복구→일반 page 수명/live undo→첫 행 거래→대상 확장 순으로 연결.64GiB 인수 준비·계측은 실제 소비 구현의 선행으로 배정 |
| 대안·선택 이유 | 전체 구조 재설계는 현재 정적 검토 근거가 부족하다. 모든 index/catalog 뒤 거래·복구 연결은 custom undo/예약/REBASE 위험을 늦게 드러낸다. 기존 owner 안의 최소 기능 연결을 선택하며 별도 제한 제품 엔진·범용 recovery framework를 만들지 않음 |
| 소유 계약·시험 | [일반 buffer](../contracts/dev3-task-05-buffer-flush.md#operational-pool), [첫 행 거래](../contracts/dev3-task-11-commit.md#first-row-integration), [제한 복구](../contracts/dev3-task-12-recovery.md#early-recovery), [자원 준비](../contracts/dev3-resource-bounds.md#qualification-profile-preparation), [계측](../contracts/dev3-task-16-qualification.md#observation-delivery), [DIR01–05/NT 배정](dev3-test-plan.md#direction-integration). 공급도·상세 설계§16·현재 상태를 같은 순서로 연결 |
| 유지·미완료 | 16개 ID·기존 task DAG·DEV2 partial·제품 S1·최종 S5 전수 의무 유지. 신규 bytes/API/지원 profile/수치 F05는 각 leaf 동결·독립 검토 대상. 방향 채택과 구현·실제 장애/성능 인수는 별개 |
| 검토·검증 | `/root/direction_independent_review`가 시작 사본의 해시와 변경15개 Markdown·원 owner 계약을 대조하여 최종 D2 PASS. 초안 P2의 우선순위/공급 의존성 혼동을 수정하고 공통 terminal→최소 catalog→table→행 거래 공급을 명시했다. 통합 차단 잔여 지적 없음, `/root` 반영 확인. `git diff --check` PASS. 제품 코드·시험·compiler/Clippy·문서 자동 검사·commit/push 없음. 기존 실행 결과와 당시 결정의 identity 보존; 검토 PASS는 문서·설계 경계에 한정 |
| 재검토 조건 | 최소 table 공급의 순환, 제한 복구의 일반 READY 오인, 정상 admitted 복구/고정 peak 반례, 일반 pool의 중복 권한,64GiB 전체 회계 불성립, 계측에서 드러난 병목. 해당 owner 계약을 조정하고 새/변경 코드만 검증 |

첫3.12.1.a의 [초기 입력 선택 계약](../contracts/dev3-recovery-select.md)과 [제한 인수](../implementation/dev3-implementation.md#recovery-select)는 header-only WAL·초기 Empty DWB의 닫힌 선택을 공급했다. 다음은 실제 BOOT_RESERVE와 현재 producer prefix의 지원 의미를 확정해 후속 장벽에 연결하는 단위다. 그 leaf 하나의 성공과 첫 번째 통합 목표인3.12.1–4의 복원·검산 완료를 따로 보고한다. 설계 분할·문서 수·시험 수로 진행률을 올리지 않으며 공급 기능·미공급 소비자·남은 인수로 보고한다.

우선순위는 공급 의존성과 구별한다. 일반 pool의 실제 입력은 INIT·WAL/flush·필요한 guard이며 제한 복구 완료를 소비 API의 선행으로 추가하지 않는다. 조기3.12 공급은 현재 S2 쓰기 경로의 복구 연결이고 XADB-022/S3 전체 완료가 아니다. 첫 행의 table 공급은3.11.1 공통 terminal/공개 primitive→최소 catalog create의 commit→영속 table handle→첫 행 거래 인수로 연결하며 기존 task 완료 DAG와 실제 선행 수신 의무를 유지한다.

<a id="model-roles"></a>
### 2.8 DEV3 개발 모델과 독립 검토 운영

**DEV3의 기본 주 개발 모델은 GPT-6.1 Sol (`gpt-6.1-sol`)로 한다.** 기존 작업 크기·파일 소유·순차 통합 규칙에 따라 구현하고, 작성자 외 별도 실행의 독립 검토를 유지한다. 복구·동시성의 핵심 설계는 가능하면 GPT-6 Astra (`gpt-6-astra`)를 독립 검토에 활용한다. 이는 사용자가 선택한 운영 방침이며 XADB에서 두 모델의 완수율·품질·비용 우위를 실측한 결론은 아니다.

| 작업·역할 | 모델 배정과 적용 방식 |
| --- | --- |
| 확정 명세의 구현·fixture·테스트·계측 | Sol을 주 개발 모델로 사용한다. 작업마다 owner 계약·변경 경로·정확한 완료 조건을 수신하고 해당 기능의 필수 시험을 함께 작성·검증 |
| 일반 buffer·tree·행 변경 | Sol이 작은 통합 단위로 구현하고 별도 검토자가 계약·diff·독립 기대값·실제 결과를 확인. 단일 page 권한·소유 인계·실패 상태를 각 소비 경로까지 연결 |
| undo/CLR·예약 복원·RECOVERY_REBASE·commit 공개·핵심 동시성 | 상태 전이·장애 반례·자원 상한을 먼저 확정하고 D2 검토를 강화한다. Astra를 사용할 수 있으면 별도 검토자로 우선 배정하며, 실제 구현 변경은 D1 검토도 유지 |
| 총괄·통합 | 기존 통합 담당이 API·공유 파일·공급 순서·원래 실행 증거를 관리한다. 모델 이름이나 검토자의 동의만으로 계약의 미정 사항·필수 시험·제품 gate를 완료 처리하지 않음 |

Astra 사용은 권장 배정이며 새로운 제품 착수 gate가 아니다. 사용할 수 없으면 [DP01](../policies/development-policy.md#section-2)에 따라 작성자 외 별도 Sol 실행 등으로 독립 검토할 수 있다. 같은 모델의 별도 실행도 오류가 통계적으로 독립이라고 가정하지 않는다. 별도 검토 수단 자체가 없으면 DP01의 `READY_FOR_REVIEW`를 따르며 자기 검토로 대체하지 않는다. 반복 결함이나 미해결 불변식이 발견되면 작업 범위·계약·검토 배정을 조정하고, 모델 변경으로 반례를 없던 것으로 처리하지 않는다.

첫 평가 단위는 예정된 **3.12.1.a LEASE/PREFLIGHT/SELECT**다. F07에 실제 모델 ID·추론 설정·담당/검토자·source/diff를 기록하고, 계약 준수 여부, 발견 결함의 심각도·재발 여부, 수정 횟수, 필수 시험 결과로 다음 배정을 판단한다. 시간·사용량은 관측한 경우만 기록한다. 이 단위의 성공을 전체 복구 역량으로 확대하지 않으며, 이어3.12.1–4를 실제 새 프로세스의 복원·검산까지 연결한 증거로 판단을 갱신한다. 별도의 모델 비교 캠페인을 기능 착수의 선행으로 추가하지 않는다.

이 평가는 실제 개발 산출물에 대한 배정 점검이며 [DP05의 정식 비교 pilot](../policies/development-policy.md#section-6)을 대신하지 않는다. 이번 사용자 선택은 DEV3에 적용하며 다른 단계의 기본 운영을 바꾸지 않는다. 향후 모델·작업 방식의 품질/효율 우위를 주장하는 비교에는 DP05의 사전 고정 입력·분모·검증 기준을 적용한다.

중간에는 실제 변경 코드의 필수 시험만 실행하고 **최종 S5의 확정 DEV3 inventory 전수 인수**를 유지한다. 평가를 이유로 무변경 시험·전체 wrapper를 반복하지 않는다. 컴파일·Clippy·모델 검토는 실제 crash/reopen·독립 이력·자원 검증을 대체하지 않는다. 공식 [OpenAI 모델 안내](https://learn.chatgpt.com/docs/models)는 Sol의 복잡한 코딩 활용을 권장하는 참고이며 DEV3의 실행 증거는 아니다(2026-10-22 확인).

#### D3-MODEL-ROLES-001 — 모델 역할 명시

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22 |
| 권한·담당 | 사용자가 Sol 주 개발·별도 독립 검토·가능하면 Astra 핵심 검토와 첫 평가 단위의 명시를 요청. 작업·계약·통합 `/root`, 별도 독립 검토 `/root/direction_independent_review` |
| 기준·범위 | HEAD `2cd06cde4713ac9ee0ccfa627f25576263a69aa5`, 착수 worktree clean. 이 계획과 진입/계획 색인만 수정. 기존 작업 ID·제품 의미·D1/D2 검토·시험 의무·D3-DIRECTION-001 유지 |
| 선택·대안 | Sol 주 개발과 핵심 설계의 Astra 우선 검토를 선택. 작성자의 자기 검토만으로 완료하거나 모든 작업에 Astra를 필수 선행으로 두지 않음. 기존 명세·작은 작업·독립 증거로 실제 적합성을 판단 |
| 검토·실행 | `/root/direction_independent_review`가3개 문서의 실제 diff·DP01/DP05·기존 DEV3 순서를 읽기 전용 검토하여 D2 PASS, 필수 수정·통합 차단 사항 없음. `/root` 반영 확인, `git diff --check` PASS. 모델 비교·제품 구현·시험은 NOT_RUN. 문서 verifier·commit/push 없음. 이 문서 기록으로 현재 세션 모델·앱 설정을 변경하거나 제품 개발을 재개하지 않음 |
| 재검토 | 계약 위반·같은 결함 반복·검토에서의 중대한 누락·제한 복구 통합 실패·관측된 비용/시간 문제·모델 가용성 변경 시 배정 재검토. 기존 필수 불변식과 인수는 유지 |

<a id="acceptance"></a>
## 3. 필수 인수

전부 **NOT_RUN**이다. [DS JSON](../contracts/disk-test-matrix.json)은 DEV3 현재 변형, [복구 JSON](../contracts/recovery-test-matrix.json)은 단계별 적용 범위를 소유한다. 과거 미실행 COW case를 새 실행 PASS로 재명명하지 않는다.

| ID | 필수 반례·결과 |
| --- | --- |
| DS01/KU09 | empty/multilevel mutable tree·split/root 변경·빈 leaf 제거, overflow 경계/누락/순환, stable anchor cursor와 concurrent split/delete. index point/range/full scan과 독립 row 값 동일 |
| DS02/RC-T12/KU10 | 다중 table/index/catalog 원자 commit. commit 시 data flush 없음, uncommitted steal, WAL-before-page·doublewrite 누락 mutant, namespace 확장·각 sync·publish/reply UNKNOWN |
| DS03/RC-T14 | row/index/locator/allocation inventory 각각 buffer 4배, bounded buffer/FD/lock/ATT/RSS. recovery의 전체 DPT 적재 금지. 같은 0/소량 WAL·dirty 수에서 256 MiB/1 GiB 정상 reopen I/O 비교와 full audit 비용 분리 |
| DS04/RC-T13 | old snapshot·긴 Active 거래·RC statement pin, status/undo/page slot ABA, commit-q stamping·overflow/delete/drop aging. DS04는 최초 q 선택/등록과 aging 경쟁·status 설치 후 frontier 공개 전 보호도 검증. begin WAL 제거 뒤 abort/recovery와 반복 CLR crash |
| DS05 | fuzzy checkpoint 중 수정/flush·ATT/DPT 누락 mutant·previous fallback의 WAL floor. DWB 슬롯 torn/재사용/bootstrap. source-WAL 없는 offline exact C restore→새 ACK→재시작 |
| DS06 | PK NULL 거절; unique `(1,NULL)` 중복 거절·`(NULL,NULL)` 허용; statement key 교환 성공·최종 중복 실패·이전 statement 보존. key1024 경계·1–4열·8 index·offline build 실패/취소 |
| 동시성 | RC 두 statement 사이 다른 commit 관측, 같은 statement 안정, independent row writer 둘 다 성공; 관측→타 거래 commit/unlock→즉시 X lock에서도 논리 버전 충돌. Serializable 독립 table 성공·same-table false conflict·write-skew/absence/range/catalog 충돌. unique 잠금 경합·deadlock victim full undo |
| 자원 | foreground가 reserve 소비하는 mutant, CLR worst-case/rollback split page 부족의 사전 거절, quota 경계에서 rollback/flush/checkpoint/aging 진행, 반복 crash에서 scratch/disk 누적 없음 |
| 전체 검증 | 정상 reopen 후 admission drain한 full row↔index/unique/catalog/allocation/undo 검사와 독립 이력. cold-page corruption 첫 read/audit 거절을 startup 검출로 과장하지 않음 |
| 하위 공급 수신 | [사용자 지시](../../AGENTS.md)와 [FT10](dev3-test-plan.md#final-suite)에 따라 실제 수정한 공통 공급/adapter·DEV3 연결 사례만 선택. 무변경 DEV0/1/2·P0/K0·durable leaf는 원래 증거 수신이며 전체 wrapper를 재실행하지 않음 |

잠금 wait/unique 오류 뒤 rollback을 완료하지 않았는데 ABORT 성공 응답하는 mutant, CLR 진행점만 저장하는 mutant, user abort가 split의 이전 전체 page를 복사해 다른 거래를 지우는 mutant, undo/status 조기 재사용 mutant를 필수로 포함한다.

<a id="state-model"></a>
### 3.1 D3-M01–M06: 변경·회수의 유한 모델

현재 **전체 모델 인수 미완료**다. [M01/M02/M05의 보존 소스 실행](../implementation/dev3-implementation.md#models-125)은 완료했으며 나머지 모델과 실제 disk 인수는 별도 의무다. 모델은 production page/WAL codec·refcount update 함수를 가져오지 않는다. 상태에는 논리 행/인덱스 이력, 별도 명시 edge graph, generation, rollback target/progress, transient pin, selected WAL/control prefix와 실제 살아남은 page를 둔다. 구현 후보의 stored count와 독립 graph에서 매번 다시 계산한 incoming count를 비교하고, reader의 값과 rollback 결과도 독립 이력으로 검사한다.

기본 유한 범위는 writer 2·reader 1·maintenance actor 1, 행 2·index 1·key 두 값과 NULL, undo/status 각 4 slots·각 slot 2 generations, 추상 page 8·buffer frame 3이다. maintenance actor는 aging/checkpoint/flush/restart를 한 번에 하나씩 수행한다. 각 아래 고정 시작 fixture에서 action 길이 최대16, crash 최대2를 탐색하며 generation/ID wrap을 성공 전이로 허용하지 않는다. 한 case의 탐색 보호 상한은 1,000,000 states·8,000,000 transitions이고 상한·시간 제한에 도달하면 INCONCLUSIVE다. 선언 범위의 전수 탐색을 끝내야 PASS이며 seed 시험으로 대체하지 않는다. F02/F03에서 정확한 action·동치류·초기 fixture·시간 예산과 hash를 먼저 고정한다. 범위 변경은 실행 전 D2이며 실패 후 축소하지 않는다.

| ID / 인수 | 시작 이력과 교차 전이 | 필수 판정·부정 사례 |
| --- | --- | --- |
| D3-M01 / DS04 | A commit→B update→A aging→B statement/전체 rollback→slot 재사용 | cut 전/후 before-image·두 종류의 previous link·head 변경. 필요한 과거 값 유실, dangling edge·count 누락/중복 감소 거절 |
| D3-M02 / DS01,DS04 | 최초 snapshot q 선택→horizon 등록과 commit/aging 교차; status11 설치·frontier10에서 reader 없는 aging→snapshot10 획득; 이후 UndoRef/TxnRef 복사→source latch 해제→ager stamp/cut/reuse→reader 재개 | q 선택/등록의 gate 연속 보유·H=min(published q, 등록 q)·guard 인계, transient pin 동안 재사용 불가. 등록 전 gate 해제·미공개 commit q/빈 registry의 무한대 horizon·pin 인계 공백·snapshot만으로 transient 수명 대체 mutant 거절 |
| D3-M03 / DS01,DS06 | split 뒤 다른 writer 변경→원래 writer rollback; 반대 순서의 page 발견·pin 부족; reader가 매 try를 선점 | 타 거래 값/완결 SMO 보존, 정렬된 latch·전체 pin 예약·재시도 cleanup·FIFO turn/drain. stale 전체 page 복원·부분 pin 보유 대기·공정한 무한 try starvation mutant 거절 |
| D3-M04 / DS02,DS05 | CLR effect/progress, count/cut/queue cursor, commit 상태·ATT/DPT capture 각각의 append/apply/flush 사이 crash 두 번 | 완결 그룹의 원자 redo·UndoNextRef 진행, winner 보존·loser 제거, queue 정확히 한 번 해제. progress만 먼저 남기는 mutant 거절 |
| D3-M05 / DS02,DS05 | 서로 다른 file의 두-page DWB batch: image→pending→일부 home→file sync→retirement→reuse, 각 경계 재장애 | 전체 pending 복원, partial 성공의 조기 retirement·raw retired image 부활·필수 file sync 누락 거절 |
| D3-M06 / DS03 | 자원 잔여량을 필요량−1/동일/+1로 두고 admission·취소·deadlock victim·rollback·flush·crash/예약 복원 교차 | 첫 effect 전 전부 예약 또는 거절, protected 몫 침범 없음, 종료하는 I/O·공정한 actor 실행 아래 예약 rollback/flush 완료. turn 준비에서 flusher까지 차단하거나 상위 잠금을 가진 turn 대기의 순환·starvation, ledger 초기화·CLR 소비량 재사용 mutant 거절 |

DS06의 [행 관측 버전](../contracts/disk-storage-design.md#observed-row-version) 시험은 native ReadCommitted와 Serializable을 각각 사용한다. `T1 observe(A) → T2 update/commit/unlock → T1 X-lock 즉시 획득 → validate`에 실제 barrier를 두고, T1의 해당 행 효과 전 전체 거래 conflict를 확인한다. T1의 앞선 own writes도 전체 undo되고 T2의 ACK는 보존되어야 한다. 실제 잠금 대기, T2 delete, 같은 bytes를 쓴 새 버전, T2 abort 뒤 원래 identity 복원, 앞선 own-write 관측, 값 변경 없는 split/이동을 별도 fixture로 둔다. wait-only 검사·물리 page version만 비교·값 equality만 비교·caller가 observation을 다른 statement/행에 재사용하는 mutant를 거절한다. D3-M03에는 두 writer의 observe/lock/validate/apply를 분리한 고정 fixture를 추가하며 commit validation만으로 잘못된 execute 효과를 가리지 않는다.

D3-M04/M06에는 영속 descriptor·실제 파일 bytes·page token 소유·잔여 obligation을 별도 상태로 둔다. 같은 그룹의 effect/progress/예약 변경, capture와 WAL 인계, crash의 volatile ledger 소멸, BOOT_RESERVE/RESERVE_RESTORE, 부분 CLR 뒤 두 번째 crash를 각각 action으로 고정한다. undo 대상과 남은 비용은 production ledger와 독립적으로 계산한다. 기존 유한 공간/상한을 유지하며 이 fixture를 완주해야 한다. 반복 bootstrap은 아래 추가 crash 없는 진행 graph와 분리하여, crash 전이를 유지하는 별도 M06 bootstrap graph로 판정한다. source cut·고정 두 슬롯·공통 fallback·base-only/WRITER 선택·charged/reserved를 상태로 두고, 새 epoch 설치 직후 crash 순환과 모든 슬롯 인계/회수 cut에서 고정 peak 및 최소 한 recovery closure 보존을 검사한다. depth16/crash2 탐색 외에 이 추상 graph의 모든 도달 상태·crash 간 귀납 전이를 완주해야 한다. 동일한 탐색 보호 상한을 적용하며, cap 도달·미증명은 INCONCLUSIVE이고 실행 뒤 상한 축소로 PASS시키지 않는다.

M03/M06의 **진행 판정은 action 길이16의 safety 탐색과 별도**다. 같은 고정 유한 객체 범위에서 추가 crash가 없는 실행의 도달 가능한 상태 graph를 만들고, retry/새 reader 반복을 보존하는 순환으로 표현한다. F02/F03에서 fairness와 I/O 종료 가정·K·정규화 규칙을 먼저 동결한다. fair SCC에서 미완료 예약 작업이 영원히 남는 cycle이 없는지 검사하거나 이에 해당하는 감소 순위 증명을 제출한다. 보호 상한으로 graph를 닫지 못하면 INCONCLUSIVE이며, 단순히 depth16까지 오류가 없다는 이유로 진행 PASS를 주지 않는다. 반환한 ticket 순서·drain 완료·각 CLR/flush 단계의 진전과 starvation mutant의 counterexample을 receipt에 남긴다.

M01–M06 PASS는 작은 범위의 논리 검증이다. 8 index·높이16·최대 row·64-page DWB batch는 실제 profile에서 별도로 검사한다. 상태수·전이수·미탐색 수·counterexample·모델/fixture hash를 receipt에 남기며 실제 thread·파일 I/O·DS/PF 인수를 대체하지 않는다.

M02는 `q_read`, `horizon_register`, `status_install`, `frontier_publish`, `horizon_sample`, `cut/reuse`, `guard_transfer/release`를 별도 action으로 둔다. 후보의 q 선택/등록을 검증 없이 하나의 atomic action으로 숨기지 않고 gate 소유 상태로 interleaving 가능 여부를 판정한다. 초기 상태에 값 A@q10, writer의 값 B@q11과 A 복원용 undo를 두며, snapshot10의 독립 기대값은 항상 A다. 두 고정 시작 fixture(등록 직전·status 설치 후 공개 직전)에서 기존 유한 범위로 탐색하고, reader 없는 상태도 포함한다. 등록 실패/취소의 slot 반환과 인계 중 이전 소유 보존을 검사한다.

DS04의 실제 thread 시험은 위 두 경계에 barrier를 두고 reader/writer/ager를 교차 실행한다. 등록 전에는 ager의 cut 적용이 대기하고 등록 후에는 q10의 값을 보존해야 한다. status11·frontier10·빈 registry에서도 q10에 필요한 undo/객체를 유지해야 하며, frontier11 공개와 마지막 q10 소유 해제 뒤에는 다른 pin이 없는 eligible suffix를 회수할 수 있어야 한다. Serializable begin·ReadCommitted begin_statement·짧은 catalog snapshot과 guard 인계를 각각 확인한다. 기존 reader content 경로와 새 snapshot admission을 구별하고 M03/M06의 turn drain/잠금 순환 판정에도 이 action을 포함한다. barrier 도달·gate/registry 소유·expected/actual 값을 기록하며 NOT_RUN 상태를 유지한다.

M03/M06의 완료 증명은 유한 obligation·충분한 무장애 구간·종료하는 정상 I/O·공정한 실행을 전제한다. 모든 허용 crash cut의 closure/peak 안전성은 별도 귀납 의무다. 기존 D3-M01–M06의 graph·action/codec·실제 I/O 인수는 NOT_RUN이며 설계 검토로 완료되지 않는다.

<a id="resource-proof"></a>
### 3.2 최악 조건 자원 계산과 진행 인수

F05는 [자원 벡터 산식](../contracts/disk-storage-design.md#reserve-accounting)에 opcode별 실제 codec bound를 대입한 예약표를 제출한다. 행/index/overflow/undo/CLR/SMO/allocator/control을 빠짐없이 포함하고, 각 항의 bytes/pages·고정/거래별 소유·최대 동시 실행·반환 시점을 표시한다. 8 GiB와 그 안의1 GiB reserve, WAL256 MiB, memory/frame/FD 하위 한도를 **동시에** 만족하는 admission 수를 계산한다. 이 표가 없으면 F05 미완료이며 S2/S3의 해당 effect를 통합하지 않는다.

[DEV3-REVIEW-007](../contracts/disk-storage-design.md#mathematical-review)의 반복 statement 실패를 DS03/DS06에 추가한다. S1 성공 뒤 새 key/row 잠금을 얻고 WAL을 쓴 S2를 실패·rollback하며 다른 owner가 WAL floor를 pin한다. S2의 논리 효과는 없어도 누적 WAL/CLR·회수 전 undo/page·완결 SMO와 거래 잠금이 실제 ledger에 남아야 한다. 실제 반환량만 차감하고, 다음 statement가 최악 비용을 확보하지 못하면 첫 effect 전에 거절한다. 전체 거래 abort의 예약은 유지하며 commit 시도는 기존 실패/확실성 계약을 따른다. ledger를 statement mark로 복원하거나 retained lock을 무단 반환하는 mutant를 거절한다. F02/F05에는 logical rollback, 실제 반환, 잔여 소유를 별도 열로 둔다.

필수 실제 fixture는 최대128 KiB row·8 index·높이 경계의 tree·4/16/64 MiB buffer·최대32 거래 요청이다. 전부를 수용해야 하는 시험이 아니라 계산된 수까지만 안전하게 admission하고 나머지는 첫 effect 전 거절하는 시험이다. row size·index key·논리1 MiB 등 서로 양립하지 않는 극값은 한 불가능 fixture로 합치지 않고 각각 한도를 만족하는 case로 분리한다. allocator inventory의4배 working set도 실제 entry 크기/할당량으로 산정하고 필요 quota 변경은 실행 전 검토한다.

정확한 경계−1/동일/+1, WAL floor가 오래 pin된 상태, 여러 statement 성공 뒤 실패/취소, rollback 재삽입 중 split, batch flush 일부 실패, 두 번째 crash를 조합한다. 각 case는 reservation/소비/해제 ledger·pin/FD/RSS peak·rollback/flush 진행 witness를 남긴다. rollback이 자기 완료에 필요한 checkpoint/aging을 기다리는 순환, foreground의 reserve 차용, 이미 소비한 CLR 예약의 재사용, 고갈 뒤 거짓 ABORT를 mutant로 거절한다. 계속 유입되는 reader와 4 MiB buffer에서 turn 준비 중 flusher가 필요하도록 하고, ticket 대기·drain·완료의 실제 thread witness를 확인한다. 강제 I/O/ENOSPC는 성공 진행 보장과 별도로 Fenced·복구 결과를 판정한다.

[crash 뒤 예약 복원](../contracts/disk-storage-design.md#recovery-reservation)의 추가 필수 fixture는 BEGIN WAL을 회수한 Active 거래와 일부 CLR만 완료한 loser다. runtime ledger를 모두 잃은 새 프로세스에서 checkpoint+WAL만으로 obligation을 복원하고, 기존 extent token·성장 예약·이미 소비한 CLR bytes·남은 CLR/split/status/abort-end를 독립 회계와 대조한다. BOOT_RESERVE·DWB repair·RESERVE_RESTORE·REDO·RECOVERY_REBASE의 공통 fallback/두 슬롯/base-only 선택·epoch/header/WRITER 선택·첫/중간/마지막 CLR·bootstrap checkpoint 인계/회수 전후에 장애를 넣는다. 새 epoch 선택 직후 연속 crash와 두 슬롯이 모두 선택된 초기 상태도 포함한다. 현재/previous closure를 유지한 채 고정 bootstrap peak 안에서 재진입해야 한다. 8 GiB 안의 protected1 GiB와 WAL256 MiB 경계를 동시에 채운 정상 admitted fixture는 공간 재요청 실패로 거절해 PASS할 수 없다. 잘못된 descriptor·실제 I/O 장애의 명시 거절은 별도 case다.

<a id="extension-boundaries"></a>
### 3.3 규모 확대 전 경계 인수

[DEV3-REVIEW-008](../contracts/disk-storage-design.md#extensibility-review)은 기존 DS03–06/PF09를 다음 반례로 구체화한다. 신규 단계나 후속 운영 기능을 S1 선행으로 추가하지 않는다. 모두 **NOT_RUN**이다.

| 연결 | 필수 사례·판정 | 동결 |
| --- | --- | --- |
| DS03/05, D3-M06 | 같은 build·DB의 loser/부분 CLR/ACK를 보존한 채 foreground 거래 한도만 낮춘 정상 reopen. 복구 profile 미충족 memory/frame/FD/disk 설정은 첫 DB write 전 거절하고 원래 profile로 재시도하면 완전 복구. 누락·손상/미지원 profile 및 obligation을 새 한도로 자르는 mutant 거절 | F01/F02/F04/F05: checksummed profile, 실제 부족 자원, 쓰기 미시도 witness |
| DS03/04/06 | 장기 reader+update/delete churn이 undo/owned/WAL 각 자원 경계에 도달. 부족 자원을 식별하고 effect 전 거절·snapshot 결과 유지·기존 abort 예약 보존. owner 해제 뒤 실제 회수/재사용을 확인한 경우 admission 재개. q/LSN 혼동·진단 중 무한 allocation·필요 undo 조기 회수 mutant 거절 | F02/F05: owner 종류별 bounded 진단·잘림·단위, 필요한 자원−1/동일/+1 |
| DS03/05, D3-M06, PF09 | 지속 replacement 부하와 오래된 dirty page, 계속 redirty되는 hot page, pinned 후보·진행 중 DWB·dirty 없음. 서비스 기회와 실제 recLSN/floor 전진을 따로 관측. pressure에서 새 forward admission을 닫고 기존 유한 작업·rollback·flush를 진행시킨 뒤 checkpoint 인계. live WAL pin이 남으면 올바르게 거절 유지. 영구 busy/no-op loop·maintenance까지 닫는 mutant 거절 | F02/F05: round별 scan bound·service gap·pressure/drain 재개 조건·실제 thread barrier; 성능 수치는 실행 전 고정 |

새 설정을 낮췄기 때문에 생긴 open 거절은 정상 설정의 복구 성공 인수를 대신하지 않는다. 모델/축소 반례는 실제 I/O·snapshot·thread·현재 build 복구 결과와 별도다.

<a id="faults"></a>
## 4. fault와 재현

F04에서 각 operation의 before/after/partial/error/unknown cut와 실제 살아남는 byte/namespace 이미지를 먼저 동결한다. WAL/undo/status/DWB/home/allocator/selector 각 경계를 포함한다. process kill과 실제 power-loss 모델을 구분한다. sync 성공은 보존되고 sync 오류는 효과가 있을 수 있다. 완전 길이의 WAL checksum 오류는 blanket tail truncation으로 숨기지 않는다. incomplete active tail의 허용 조건과 REJECT_CORRUPTION/REJECT_IO/RECOVER_OK를 미리 정한다.

recovery 중 CLR append/sync·home flush·STABILIZE·새 WRITER·새 ACK에서 다시 장애를 주고 두 번 이상 reopen한다. DWB torn home은 유효 image가 있을 때 복구하며 해당 세대 image가 없는 손상은 거절한다. source WAL 수정으로 성공시키지 않는다. failure의 rejection PASS를 successful recovery PASS로 집계하지 않는다.

DWB는 batch 크기1/8/64와 여러 대상 file, dirty frame 재갱신을 포함한다. 전체 image sync 전/후, batch pending 선택 전/후/불명, 각 home write와 file sync의 부분 성공, retirement 전/후/불명, bootstrap repair 중 두 번째 crash를 펼친다. 선택 전 미완성 batch는 home에 쓰지 않고, 선택 후 일부 file 실패는 전체 pending을 유지한다. 복구 후 같은 page의 새 ACK까지 검사하여 오래된 pending 재적용으로 최신 값을 지우는 mutant를 거절한다.

외부 controller의 ACK/unknown 관측과 거래별 witness를 보존한다. 나중 overwrite가 ACK 유실을 숨기지 않도록 이력과 복구 상태를 함께 검사한다. 독립 checker에는 known-bad 입력으로 공통 결함 검출을 확인한다. DP03의 해당 V2/V3 고정 profile·수치와 보호 oracle/재현 bundle 의무를 유지한다.

<a id="performance"></a>
## 5. PF09와 공간

64 MiB 기본 buffer의 4배인 256 MiB 이상을 row/index/locator/allocation inventory **각각** 접근하는 fixture를 준비한다. inventory byte 수가 충분하려면 큰 실제 할당량이 필요할 수 있으므로 F05에서 quota·시간·disk를 먼저 산정한다. 작은 cache만 넘기는 사례는 추가 pressure 시험이고 4배 인수 대체가 아니다.

| workload | 관측 |
| --- | --- |
| insert/update/delete·point/range | 1/2/4 writer·reader, TPS/성공률·충돌률, commit/WAL sync p99, latch/lock/pin wait, page read와 장치 I/O 구별 |
| 오래된 reader/transaction | undo bytes·horizon lag·quota 거절·reader 해제 뒤 실제 재사용, WAL floor와 undo 수명 분리 |
| checkpoint/reopen/audit | dirty-page·WAL 양 고정 비교, 정상 startup read bytes·시간, RECOVERY_REBASE의 home flush·슬롯/selector sync·회수 bytes/시간, 전체 감사 비용 별도 |
| delete churn·aging | page/extent 내부 재사용, doublewrite/flush/CLR write bytes, reserve peak·진행량. OS 반환과 구분 |
| DWB batch1/8/64 | 같은 durability·flush 요구량에서 성공 pages/s, DWB/home/control bytes, WAL/file/control sync·dirsync 수, 평균 실제 batch 크기, 대기/dirty age p99, commit·rollback tail latency |
| offline backup/restore | C 일치·전체 검사·중단 시간·복사 bytes·취소/재장애 |

RSS는 고정 runtime baseline+buffer+metadata+private+system workspace+diagnostics+사전 고정 margin의 peak와 장기 기울기로 판정한다. reservation 성공을 일반 OOM 복구 보장으로 해석하지 않는다. 동일 durability/isolation·환경·fixture/recipe에서만 baseline/candidate를 비교하고, 실제 수치·반복·회귀 한도·계측 비용을 실행 전에 고정한다. 현재 성능 개선 주장은 없다.

batch deadline은 낮은 부하에서도 1-page flush가 진행되는 시간 상한과 함께 사전 고정한다. 큰 batch의 처리량만으로 작은 요청의 p99·rollback/eviction 지연 악화를 숨기지 않는다. version-edit gate 보유·재시도·대기와 tree/page/row/key lock 대기를 분리 계측한다. gate 분할·group commit·부분 batch retirement는 이 기준선의 실행 결과를 근거로 별도 검토한다.

PF09의 1/2/4 writer profile은 `(독립 table, 같은 table의 서로 다른 행·비충돌 unique key, 같은 행/같은 unique key)` × `(명시 ReadCommitted, 기본 Serializable)`로 나눈다. Serializable의 같은 table false conflict는 현재 계약의 허용 결과지만 발생 비율·재시도 비용을 보고하며 처리 능력으로 숨기지 않는다. ReadCommitted도 실제 행/unique 충돌은 보존한다. 제공 요청 수·시도 TPS·durable 성공 TPS·충돌/limit/timeout·재시도 수와 성공 latency를 따로 기록하고, retry를 허용하는 workload라면 원래 요청부터 최종 결과까지의 latency도 측정한다. 같은 외부 부하/격리/ACK 조건에서만 비교하며 외부 부하율·queue 상한·재시도 정책을 F05에 고정한다. PF09의 추가 성능 비교에 앞선 실제 correctness history가 필수다.

true/false conflict 분류는 engine 오류 이름이 아니라 사전 고정한 read/write·unique key·catalog 의존관계와 독립 기대값으로 판단한다. 서로 다른 행이라도 실제 read 의존이 있으면 독립 workload로 분류하지 않는다. SQL의 현재 Serializable 결과를 native ReadCommitted 수치와 동일 조건의 성능 개선으로 비교하지 않는다.

<a id="freeze"></a>
## 6. 구현 전 동결

| ID | 산출물 | 완료 시점·현재 상태 |
| --- | --- | --- |
| D3-F01 | page/anchor/allocator/undo/status/WAL/CLR/checkpoint/DWB offsets·IDs·digest 범위·논리 행 버전·recovery obligation descriptor·독립 hex vector | [page envelope만 동결](../contracts/dev3-page-format.md). cell/나머지 bytes는 해당 표현 codec 전 미동결 |
| D3-F02 | API·isolation·opaque row observation/검증 경계·rank/try-lock/전체 pin 예약·FIFO progress turn/K/workspace/drain 순서·snapshot q/등록 gate와 published horizon·guard 인계·transient reference guard·statement/rollback/cut/queue 상태표 | 해당 소비 구현 전. D3-M01–M06 action/fixture·fairness/진행 판정과 Rust signature 미동결 |
| D3-F03 | 독립 key/row/index/history·edge graph oracle·negative mutants·case/variant 예상값 | 해당 production leaf 전 D3-M01–M06 전수 완료 필요; 모델/제품 실행 NOT_RUN |
| D3-F04 | 실제 WAL/DWB batch/home/namespace receipts·allocator/bootstrap 예약·recovery checkpoint 인계/회수·각 장애 cut·반복 recovery | 실제 I/O leaf 전. batch1/8/64·다중 file·repair 재장애 profile 미동결 |
| D3-F05 | opcode별 자원 벡터·최악 CLR/rollback pages·progress frame/FD/WAL·bootstrap/예약 복원과 반복 restart peak·큰 inventory quota·PF09 | 해당 effect 전 산식 대입·경계 인수 필요. 숫자 reservation table·batch deadline·실측 미완료 |
| D3-F06 | 실제 DEV2 공급 revision/API/format·재사용/adapter/전용/신규 symbol·시험 inventory·선행 leaf·clean export | production 착수와 S1 승격 각각. volatile/DEV2 형식 receipt로 DEV3 영속 충족 금지 |
| D3-F07 | 담당·기준 commit/dirty digest·독점/공유 경로·중복·병합 순서·통합 검사 | 각 작업 전. [첫 page/model 작업의 실제 배정](../implementation/dev3-implementation.md#wave-1) 완료; 후속 작업은 별도 배정 |

F01/F02/F04/F05에는 [규모 확대 경계 인수](#extension-boundaries)의 profile 거절/복구·pressure·flush/drain·성능 분모 입력도 포함한다. [조사·검토 기록](../evidence/commercial-extensibility-research.md)은 설계 근거이며 동결/실행 완료 receipt가 아니다.

후속 SQL/PART/NG/서버를 새 선행으로 추가하지 않는다. 실제로 없는 runner/artifact를 존재하는 실행 경로로 등록하지 않는다.

<a id="review"></a>
## 7. 이번 변경의 소유와 검토

작업/계약/통합 담당 `/root`, D2 독립 검토 `/root/dev3_redesign_review`. 기준 HEAD `b4d1d8cdd86bfa7d502948708445878c712c5a79`, 착수 dirty manifest SHA-256 `e79b53f41cd4f36a2c2766357a5bf4969c41b256430dd10fac2e567c28273b1d`. 동시 DEV4 변경은 보존하며 해당 파일을 직접 수정하지 않는다. 공통 구현 계약의 DEV3 적용 경계가 소비 의미의 대체 관계를 지정한다. 작업 중 HEAD 이동과 DEV2 성능 관련 동시 변경을 관측했으며 제품 코드·실행 evidence·build manifest의 다른 변경은 이 D2 승인 대상이 아니다.

독점 본문은 disk-storage-design·이 계획·새 조사 근거·disk-test-matrix. 공유 소비 계약/JSON·README/documents.json·배치 책임 문구는 최신 원문에서 순차 수정한다. 새 product path나 dependency를 추가하지 않는다. 의미 검토와 문서 push gate를 구분하며 제품/문서 자동 검사 미실행이다. 별도 검토자는 P1 3건·P2 2건과 INIT 경계의 수정 후 최종 PASS를 확인했고 `/root`가 반영 확인했다. 단계별 결과는 [조사·검토 기록](../evidence/dev3-wal-undo-research.md#review)에 남긴다.

<a id="parallel-review"></a>
### 이전 결정의 적용

DEV3-DESIGN-001/PARALLEL-001의 소스 조사·독립 검토와 당시 문서 검사 기록은 해당 Git revision 및 [이전 증거](../evidence/dev3-source-research.md)에 남는다. 이를 DESIGN-003의 검증으로 승계하지 않는다. 중간 세 파일 snapshot은 외부 archive에 원래 byte/hash로 보존한다.

<a id="review-followup"></a>
## 8. REVIEW-002를 먼저 반영한 결과

부분 NULL UNIQUE·statement-final 검사, global-q 한계, reopen 비용, 파일 수에 비례하는 GC scratch와 compaction 공간을 먼저 보완하고 독립 검토 PASS를 받았다. 이후 DESIGN-003에서 UNIQUE/vector 의무는 유지하고 global-q·정상 전수 VERIFY·COW GC/전체 compaction을 새 계약으로 대체했다. 작업 순서와 snapshot SHA-256은 [이번 증거](../evidence/dev3-wal-undo-research.md#review)에 기록한다. 새 구현의 실행 PASS는 없다.

<a id="architecture-review"></a>
## 9. REVIEW-004 보완의 착수와 인수

[아키텍처 비교 보완 기록](../evidence/dev3-dev4-architecture-review.md)은 이번 작업의 기준 사본·소유·독립 검토를 기록한다. 원래 DESIGN-003의 검토 결과는 그 당시 범위로 보존한다. 이번 D2는 구체적인 변경/수명 프로토콜·모델 범위·자원 계산 의무·batch 인수를 추가하며, 미실행 모델·예약표·성능 측정을 완료로 표시하지 않는다. S0의 문서 동결과 각 leaf의 모델/실제 I/O 실행 증거를 별도 확인한다.

<a id="snapshot-review"></a>
## 10. REVIEW-005 작업과 검토

| 항목 | 기록 |
| --- | --- |
| 분류·담당 | D2; 작업/계약/통합 `/root` |
| 기준 identity | HEAD `e50e0793acf54c115d4715f71126f6ae45c74008`; 착수 status SHA-256 `01eb13ad595267d1e920c5f9bd3e39c8cf75be1bf784a950633fc15987a5fa41`; 9개 관련 원문의 사본/hash는 `/tmp/xadb-snapshot-review-llzj1hm2/identity.json`과 before 사본, 실제 수정은 아래 8개 문서 |
| 전용 본문 | DEV3 상세 설계·이 계획·disk-test-matrix, DEV4 상세 설계·계획의 snapshot 소비 절 |
| 공유 수정·중복 | stage-completion의 DEV3 인수, contracts/plans README의 기존 DEV3 항목만 최신 원문에 수정. DEV2·COMM·DEV5 제품/문서의 기존 미commit 변경과 stage의 다른 항목은 별도 보존 |
| 반영 순서 | snapshot 소유 계약→M02/DS04와 F02→DEV4 수신·T09→단계/폴더 색인→독립 검토→이번 변경만 commit. 기존 문서 ID/경로/제목은 유지하며 새 문서·제품 경로·생성물 없음 |
| 검증 범위 | 의미·반례·좁은 diff 검토와 해당 8개 파일의 `git diff --check` PASS. 사용자의 최종 지시에 따라 전체 문서 검사·push 미실행. 모델·제품 실행도 미실행 |
| 독립 검토 | `/root/snapshot_design_review`가 원문 사본 대비 8개 파일의 변경과 기존 잠금/수명/모델 계약을 읽기 전용 검토: PASS·필수 P1/P2 없음. gate 연속 보유·published q 기준·guard 인계·turn admission·M02/DS04·DEV4 수신을 확인. 검사기/제품 시험 미실행. `/root` 반영 확인 후 REVIEW-005 ACCEPTED |

관련 결정은 [DEV3-REVIEW-005](../contracts/disk-storage-design.md#snapshot-review)다. 앞선 리뷰·실행 receipt를 이번 변경의 검증으로 승계하지 않는다.

작업 중 별도 DEV5 문서 commit `f8b5d86`으로 HEAD가 이동한 것을 확인했다. 이번 변경은 그 commit 위에 좁게 적용하며, 다른 작업의 미commit 변경을 이번 commit에 포함하지 않는다. 기존 문서 ID·경로를 재등록하거나 현재 문서 검사 결과를 생성하지 않는다.

<a id="conflict-reserve-review"></a>
## 11. REVIEW-006 작업과 검토

| 항목 | 기록 |
| --- | --- |
| 분류·소유 | D2; 작업·DEV3/DEV4 경계 계약·통합 `/root`; 사용자 리뷰 후 수정 지시 |
| 기준 identity | HEAD `61046cf0a00c7511a8c35292c7d950f2175aa888`; 착수 status SHA-256 `a2c02730a576a8411a83aa4439f8b6d80fb31fedf730ec4116f576f6e9b140a0`; 12개 원문 사본 `/tmp/xadb-dev34-conflict-reserve-qmwdw4wt/before`, identity.json SHA-256 `ab335111167ef1b8fd060a3196db3714f6e47caa61d9fdf3c18bb0585159393b` |
| 전용 본문 | DEV3/DEV4 상세 설계와 구현 계획, disk/recovery-test-matrix의 해당 인수 |
| 공유 수정·중복 | implementation-contracts DEV3 적용 표, development-stages/stage-completion의 DEV3/DEV4 항목, devdoc/contracts/plans README만 최신 원문에 순차 반영. DEV2·COMM·DEV5 제품/문서의 기존 dirty/staged 변경 보존. 작업 중 다른 작업의 HEAD 이동을 관측하여 실제 편집 직전 기준을 보관 |
| 반영 순서 | DEV3 의미/복구 예약→DEV4 소비→모델/실제 시험 의무→공유 JSON/색인→독립 검토. 문서 ID/경로/제목은 기존 documents.json 등록 그대로이며 신규 문서·코드·생성물 없음 |
| 비교 근거 | 앞선 리뷰에서 Oracle 19c 일관 읽기/거래 문서, 로컬 PostgreSQL·MariaDB·CUBRID·SQLite·DuckDB를 열람. [기존 조사](../evidence/dev3-dev4-architecture-review.md)의 범위를 현재 제품 실행으로 승계하지 않음. 직접 반례 판단은 PostgreSQL heap/executor의 변경 tuple 처리와 ARIES §5.3 로그 예약·CLR 재장애 원칙을 참고; XADB의 보수적 전체 abort와 영속 UndoRef/예약 descriptor는 자체 계약 |
| 실제 소스 기준 | PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`, MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4`, CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5`, SQLite `43a48f77585a88940c727f33682151418f544059`, DuckDB `1b4c3e54ba3625bac56ad184f0ce642a8d2bfbd5`. ARIES 원문 공개 PDF SHA-256 `3db37b1fb6c0ab2678d6825e3dab584d08e77a0a0bd06ea088c3676a51f684b9` |
| 검증 범위 | 의미·반례·최신 원문 대비 좁은 diff 검토 및 해당 12개 파일의 `git diff --check` PASS. DOC-002에 따라 문서 자동 검사·생성물 갱신·push 미실행. 모델·제품 코드·실제 DBMS 비교 실행·Q1/PF/복구 시험 NOT_RUN |
| 독립 검토 | `/root/dev34_conflict_reserve_review`가 기준 사본 대비 12개 문서와 보완 diff를 읽기 전용 검토. 반복 epoch 누적·crash graph 범위 P2 및 두 슬롯의 중간 해제 전이를 보완한 뒤 최종 PASS·필수 P1/P2 없음. `/root` 반영 확인, DEV3-REVIEW-006/DEV4-REVIEW-004 ACCEPTED. 검토자는 파일 수정·문서 자동 검사·제품 시험을 하지 않음 |

관련 결정은 [DEV3-REVIEW-006](../contracts/disk-storage-design.md#conflict-reserve-review)과 [DEV4-REVIEW-004](../contracts/dev4-sql-design.md#conflict-reserve-review)다. 구체 descriptor/codec·선택한 bootstrap 인계의 byte/selector 상태표·수치 예약표·실제 fault harness는 해당 leaf 구현 전 동결 의무로 남긴다.

사용자의 후속 전체 문서 검사·commit·push 지시에 따라, commit 대상 사본에서 전체 검사 중 current-source evidence 지문 불일치 3건을 확인했다. COMM 반영 소스와 이미 보존된 회귀 보고서의 지문 일치를 대조하여 문서 ID·현재 증거 설명을 수정했다. 이전 보고서 바이트와 성능 실패·미완료 상태는 보존한다. 제품 코드/checker 변경 및 새 제품 시험 실행은 없으며 최종 문서 결과는 [문서 검사 보고서](../document-validation.json)를 따른다.

증거 참조 보완도 `/root/dev34_conflict_reserve_review`가 후보 소스 지문·채택 사본의 byte identity·원본 보존·미실행 범위를 읽기 전용으로 독립 확인하여 PASS, 필수 수정 지적 없음으로 판정했다. 검토자는 checker나 제품 시험을 재실행하지 않았다. `/root`가 반영 확인했다.

<a id="comparison-adoption"></a>
## DEV3-REVIEW-011 반영 — 완료분 보완과 다음 작업

[설계 결정](../contracts/disk-storage-design.md#task-review-011)과 [반영 기록](../evidence/dev3-comparison-adoption-20261025.md)을 기존16개 작업에 배정한다. 문서 적용 요청은 제품 개발·테스트 실행 재개가 아니다. 개발 착수 시 정확 소비 leaf의 F01–F07을 채우며, 모든 기능 완료를 기다린 뒤 수치 회계를 하는 순서를 금지한다.

| 기존 공급·원 완료 범위 | 추가 보완·담당 | 새 인수·완료 기준 |
| --- | --- | --- |
| row/tree/Undo/status/Catalog codec | 3.06/08/09/11: 최소 terminal과 Catalog CREATE부터 실제 row 변경의 producer/consumer 연결 | D3-COMP-01. bytes decode 성공을 실제 값·ACK·rollback 성공으로 확대하지 않음 |
| 전체 대상 pin·Status 수명·N=0 plan4 고유50 PASS | 3.05/12: 일반 page 권한·N>0 row/Undo/CLR의 mixed-page redo·예약 복원·새 ACK | D3-COMP-01/02/09. 원 실패·중단/소스별 PASS 보존, 실제 diff 관련 새 사례만 중간 실행 |
| 한정 reservation/obligation 표현·모델 | 각 opcode owner,3.16 취합: [수치 벡터 표](../contracts/dev3-resource-bounds.md#resource-vector-proof)와 종료 rank/공정 진행 | D3-COMP-02. private/metadata/flush/maintenance·disk/protected/WAL·page/FD 모두, 종료 경로의 새 admission 의존 금지 |
| Undo 다중 edge·부분 모델 | 3.09/14/12: 실제 값·edge multiset·incoming·pin/horizon·release cursor·physical credit의 통합 | D3-COMP-03. BEGIN-WAL 삭제와 부분 CLR 재장애, 실제 generation 재사용 |
| coarse Serializable·단일 Commit 설계 | 3.10/11/16: 실제 의존 충돌과 stage/sync/전체 설치 지연 계측 | D3-COMP-04. 기준선 측정 전 group commit·미세 validation을 필수 구현으로 추가하지 않음 |
| fuzzy checkpoint 요구·제한 control 표현 | 3.13/02/12: immutable base/prefix·bounded cut snapshot·partition/cursor·persistent pin/선택·WAL/출력 peak | D3-COMP-05. F01/F02의 census·chunk/control body·정확 B와 turn 수명, F05 수치 동결 뒤 구현 |
| exact-C backup·restore 요구 | 3.15/02/12: namespace field/digest census·offline rewrite·intent/cleanup·retained ledger·새 epoch/LSN | D3-COMP-06. source-WAL-free restore→새 unflushed ACK→재장애. Releasing도 소실 payload 재구성 없이 검증 |
| 공간 산식·DWB batch·audit·64GiB 준비 | 3.03/05/06/16 및3.15 audit: 표현 비용/계측·fixture와 profile 지원·growth bound | D3-COMP-07–10. 현재8GiB 지원과 구별, 중간 작은 경계와 S5 확대 분리 |

착수 순서는 [우선 실행 묶음](#work-priority)을 따른다. 독립 audit/history 준비·checkpoint/restore census·64GiB 회계는 consumer 선행 준비로 진행할 수 있지만, 최초 row 통합이 전체 index/DDL/normal checkpoint에 의존하도록 DAG를 바꾸지 않는다. REBASE는 정상 checkpoint 전체 완료를 기다리지 않는다. 각 미정 수치/형식은 해당 effect의 차단 조건으로 남으며 문서 채택으로 지원 flag를 올리지 않는다.

성능 후보는 실제 durable 성공/충돌·원래 요청 latency·공간/IO 비용이 병목을 입증한 뒤 별도 D2로 평가한다. DEV3 PF09는 DP07 최소10쌍, S5는 확정된 DEV3 inventory 전수이며 COMM5+5와 DEV0/1/2 전수 반복은 적용하지 않는다.

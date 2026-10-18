# DEV3 — 구현·병렬 개발·검증 계획

2026-10-18 · **DEV3-DESIGN-003·REVIEW-004 ACCEPTED / production·모델·S1 NOT_RUN**

[상세 설계](../contracts/disk-storage-design.md) · [이번 조사·검토](../evidence/dev3-wal-undo-research.md) · [이전 조사](../evidence/dev3-source-research.md) · [완료 정본](stage-completion.json) · [DEV2 선행](dev2-implementation-plan.md)

<a id="entry"></a>
## 1. 착수와 완료

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
| S5 전체 인수 | 네 task 전체 | DS01–06·RC-T12–14·KU09/10·PF09·하위 회귀와 재현 bundle | stage-completion의 모든 의무와 독립 검토. 일부 leaf PASS로 제품 S1 승격 금지 |

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

공통 계약 → 표현 leaf → P/W/B 실제 WAL/flush 경계 → T row/undo/status → X/CAT/statement → I commit/recovery → checkpoint/aging/backup → 전체 인수 순서다. 공통 tree·transaction/publication·retention을 여러 담당이 동시에 고치지 않는다. 독립 oracle/vector·fault schedule은 production과 병렬 준비할 수 있다. 같은 build의 DS/PF job은 격리 fixture에서 나누되 성능 job의 장치 부하 간섭은 막는다.

이때 [상태 모델](#state-model)은 해당 링크/회수 production leaf 전에, [예약 계산](#resource-proof)은 해당 자원을 소비하는 실제 I/O leaf 전에 완료한다. 여러 담당이 각자의 latch 순서나 reserve를 독립 정의하지 않으며 [공통 변경 순서](../contracts/disk-storage-design.md#mutation-protocol)를 함께 소비한다.

<a id="parallel-entry"></a>
### 2.3 DEV2 수신 경계

F06에서 수신 commit/dirty digest·실제 crate/API/feature·사용한 실행 명령/source/build identity를 기록한다. 이전 공급 측 PASS는 XADB 새 PASS가 아니다. K0 이름만으로 미구현 durable API를 가정하지 않는다. F01/F02/F04와 공급 API가 다르면 해당 owner가 계약을 먼저 조정하고 독립 검토한다. 미확인 소비 작업만 대기하고 독립 준비는 계속한다.

<a id="parallel-ownership"></a>
### 2.4 공유 파일

[병렬 정책](../policies/parallel-development-policy.md#section-2)의 change_coordination을 실제 작업마다 기록한다. page/WAL/UndoRef 공통 표현은 D3-C, tree는 D3-T, transaction/publication/durable은 D3-I가 단일 작성자를 조정한다. foundation effect·platform adapter 변경은 소유자 patch 뒤 소비하며 engine에서 platform 직접 import를 추가하지 않는다. lib/mod·Cargo·runner/checker 등록·README/documents.json은 통합 담당이 최신 원문에 좁게 반영한다. 문서 검사·document-validation.json 생성은 **push gate에서만** 한다. 제품 지문/실행 증거는 합친 실제 코드 기준으로 별도 검증한다.

<a id="acceptance"></a>
## 3. 필수 인수

전부 **NOT_RUN**이다. [DS JSON](../contracts/disk-test-matrix.json)은 DEV3 현재 변형, [복구 JSON](../contracts/recovery-test-matrix.json)은 단계별 적용 범위를 소유한다. 과거 미실행 COW case를 새 실행 PASS로 재명명하지 않는다.

| ID | 필수 반례·결과 |
| --- | --- |
| DS01/KU09 | empty/multilevel mutable tree·split/root 변경·빈 leaf 제거, overflow 경계/누락/순환, stable anchor cursor와 concurrent split/delete. index point/range/full scan과 독립 row 값 동일 |
| DS02/RC-T12/KU10 | 다중 table/index/catalog 원자 commit. commit 시 data flush 없음, uncommitted steal, WAL-before-page·doublewrite 누락 mutant, namespace 확장·각 sync·publish/reply UNKNOWN |
| DS03/RC-T14 | row/index/locator/allocation inventory 각각 buffer 4배, bounded buffer/FD/lock/ATT/RSS. recovery의 전체 DPT 적재 금지. 같은 0/소량 WAL·dirty 수에서 256 MiB/1 GiB 정상 reopen I/O 비교와 full audit 비용 분리 |
| DS04/RC-T13 | old snapshot·긴 Active 거래·RC statement pin, status/undo/page slot ABA, commit-q stamping·overflow/delete/drop aging. begin WAL 제거 뒤 abort/recovery와 반복 CLR crash |
| DS05 | fuzzy checkpoint 중 수정/flush·ATT/DPT 누락 mutant·previous fallback의 WAL floor. DWB 슬롯 torn/재사용/bootstrap. source-WAL 없는 offline exact C restore→새 ACK→재시작 |
| DS06 | PK NULL 거절; unique `(1,NULL)` 중복 거절·`(NULL,NULL)` 허용; statement key 교환 성공·최종 중복 실패·이전 statement 보존. key1024 경계·1–4열·8 index·offline build 실패/취소 |
| 동시성 | RC 두 statement 사이 다른 commit 관측, 같은 statement 안정, independent row writer 둘 다 성공; Serializable 독립 table 성공·same-table false conflict·write-skew/absence/range/catalog 충돌. unique 잠금 경합·deadlock victim full undo |
| 자원 | foreground가 reserve 소비하는 mutant, CLR worst-case/rollback split page 부족의 사전 거절, quota 경계에서 rollback/flush/checkpoint/aging 진행, 반복 crash에서 scratch/disk 누적 없음 |
| 전체 검증 | 정상 reopen 후 admission drain한 full row↔index/unique/catalog/allocation/undo 검사와 독립 이력. cold-page corruption 첫 read/audit 거절을 startup 검출로 과장하지 않음 |
| 하위 회귀 | DEV0/1·DEV2 P0/K0 및 실제 공급 durable leaf. 동일 의미 회귀와 형식별 DEV2/DEV3 차이를 분리 |

잠금 wait/unique 오류 뒤 rollback을 완료하지 않았는데 ABORT 성공 응답하는 mutant, CLR 진행점만 저장하는 mutant, user abort가 split의 이전 전체 page를 복사해 다른 거래를 지우는 mutant, undo/status 조기 재사용 mutant를 필수로 포함한다.

<a id="state-model"></a>
### 3.1 D3-M01–M06: 변경·회수의 유한 모델

현재 **모델 구현·탐색 NOT_RUN**이다. 모델은 production page/WAL codec·refcount update 함수를 가져오지 않는다. 상태에는 논리 행/인덱스 이력, 별도 명시 edge graph, generation, rollback target/progress, transient pin, selected WAL/control prefix와 실제 살아남은 page를 둔다. 구현 후보의 stored count와 독립 graph에서 매번 다시 계산한 incoming count를 비교하고, reader의 값과 rollback 결과도 독립 이력으로 검사한다.

기본 유한 범위는 writer 2·reader 1·maintenance actor 1, 행 2·index 1·key 두 값과 NULL, undo/status 각 4 slots·각 slot 2 generations, 추상 page 8·buffer frame 3이다. maintenance actor는 aging/checkpoint/flush/restart를 한 번에 하나씩 수행한다. 각 아래 고정 시작 fixture에서 action 길이 최대16, crash 최대2를 탐색하며 generation/ID wrap을 성공 전이로 허용하지 않는다. 한 case의 탐색 보호 상한은 1,000,000 states·8,000,000 transitions이고 상한·시간 제한에 도달하면 INCONCLUSIVE다. 선언 범위의 전수 탐색을 끝내야 PASS이며 seed 시험으로 대체하지 않는다. F02/F03에서 정확한 action·동치류·초기 fixture·시간 예산과 hash를 먼저 고정한다. 범위 변경은 실행 전 D2이며 실패 후 축소하지 않는다.

| ID / 인수 | 시작 이력과 교차 전이 | 필수 판정·부정 사례 |
| --- | --- | --- |
| D3-M01 / DS04 | A commit→B update→A aging→B statement/전체 rollback→slot 재사용 | cut 전/후 before-image·두 종류의 previous link·head 변경. 필요한 과거 값 유실, dangling edge·count 누락/중복 감소 거절 |
| D3-M02 / DS01,DS04 | reader가 UndoRef/TxnRef 복사→source latch 해제→ager stamp/cut/reuse→reader 재개 | transient pin 동안 재사용 불가, 해제 뒤 회수 가능. snapshot pin만으로 복사된 status 참조의 수명을 보장하는 mutant 거절 |
| D3-M03 / DS01,DS06 | split 뒤 다른 writer 변경→원래 writer rollback; 반대 순서의 page 발견·pin 부족; reader가 매 try를 선점 | 타 거래 값/완결 SMO 보존, 정렬된 latch·전체 pin 예약·재시도 cleanup·FIFO turn/drain. stale 전체 page 복원·부분 pin 보유 대기·공정한 무한 try starvation mutant 거절 |
| D3-M04 / DS02,DS05 | CLR effect/progress, count/cut/queue cursor, commit 상태·ATT/DPT capture 각각의 append/apply/flush 사이 crash 두 번 | 완결 그룹의 원자 redo·UndoNextRef 진행, winner 보존·loser 제거, queue 정확히 한 번 해제. progress만 먼저 남기는 mutant 거절 |
| D3-M05 / DS02,DS05 | 서로 다른 file의 두-page DWB batch: image→pending→일부 home→file sync→retirement→reuse, 각 경계 재장애 | 전체 pending 복원, partial 성공의 조기 retirement·raw retired image 부활·필수 file sync 누락 거절 |
| D3-M06 / DS03 | 자원 잔여량을 필요량−1/동일/+1로 두고 admission·취소·deadlock victim·rollback·flush 교차 | 첫 effect 전 전부 예약 또는 거절, protected 몫 침범 없음, 종료하는 I/O·공정한 actor 실행 아래 예약 rollback/flush 완료. turn 준비에서 flusher까지 차단하거나 상위 잠금을 가진 turn 대기의 순환·starvation mutant 거절 |

M03/M06의 **진행 판정은 action 길이16의 safety 탐색과 별도**다. 같은 고정 유한 객체 범위에서 추가 crash가 없는 실행의 도달 가능한 상태 graph를 만들고, retry/새 reader 반복을 보존하는 순환으로 표현한다. F02/F03에서 fairness와 I/O 종료 가정·K·정규화 규칙을 먼저 동결한다. fair SCC에서 미완료 예약 작업이 영원히 남는 cycle이 없는지 검사하거나 이에 해당하는 감소 순위 증명을 제출한다. 보호 상한으로 graph를 닫지 못하면 INCONCLUSIVE이며, 단순히 depth16까지 오류가 없다는 이유로 진행 PASS를 주지 않는다. 반환한 ticket 순서·drain 완료·각 CLR/flush 단계의 진전과 starvation mutant의 counterexample을 receipt에 남긴다.

M01–M06 PASS는 작은 범위의 논리 검증이다. 8 index·높이16·최대 row·64-page DWB batch는 실제 profile에서 별도로 검사한다. 상태수·전이수·미탐색 수·counterexample·모델/fixture hash를 receipt에 남기며 실제 thread·파일 I/O·DS/PF 인수를 대체하지 않는다.

<a id="resource-proof"></a>
### 3.2 최악 조건 자원 계산과 진행 인수

F05는 [자원 벡터 산식](../contracts/disk-storage-design.md#reserve-accounting)에 opcode별 실제 codec bound를 대입한 예약표를 제출한다. 행/index/overflow/undo/CLR/SMO/allocator/control을 빠짐없이 포함하고, 각 항의 bytes/pages·고정/거래별 소유·최대 동시 실행·반환 시점을 표시한다. 8 GiB와 그 안의1 GiB reserve, WAL256 MiB, memory/frame/FD 하위 한도를 **동시에** 만족하는 admission 수를 계산한다. 이 표가 없으면 F05 미완료이며 S2/S3의 해당 effect를 통합하지 않는다.

필수 실제 fixture는 최대128 KiB row·8 index·높이 경계의 tree·4/16/64 MiB buffer·최대32 거래 요청이다. 전부를 수용해야 하는 시험이 아니라 계산된 수까지만 안전하게 admission하고 나머지는 첫 effect 전 거절하는 시험이다. row size·index key·논리1 MiB 등 서로 양립하지 않는 극값은 한 불가능 fixture로 합치지 않고 각각 한도를 만족하는 case로 분리한다. allocator inventory의4배 working set도 실제 entry 크기/할당량으로 산정하고 필요 quota 변경은 실행 전 검토한다.

정확한 경계−1/동일/+1, WAL floor가 오래 pin된 상태, 여러 statement 성공 뒤 실패/취소, rollback 재삽입 중 split, batch flush 일부 실패, 두 번째 crash를 조합한다. 각 case는 reservation/소비/해제 ledger·pin/FD/RSS peak·rollback/flush 진행 witness를 남긴다. rollback이 자기 완료에 필요한 checkpoint/aging을 기다리는 순환, foreground의 reserve 차용, 이미 소비한 CLR 예약의 재사용, 고갈 뒤 거짓 ABORT를 mutant로 거절한다. 계속 유입되는 reader와 4 MiB buffer에서 turn 준비 중 flusher가 필요하도록 하고, ticket 대기·drain·완료의 실제 thread witness를 확인한다. 강제 I/O/ENOSPC는 성공 진행 보장과 별도로 Fenced·복구 결과를 판정한다.

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
| checkpoint/reopen/audit | dirty-page·WAL 양 고정 비교, 정상 startup read bytes·시간, 전체 감사 비용 별도 |
| delete churn·aging | page/extent 내부 재사용, doublewrite/flush/CLR write bytes, reserve peak·진행량. OS 반환과 구분 |
| DWB batch1/8/64 | 같은 durability·flush 요구량에서 성공 pages/s, DWB/home/control bytes, WAL/file/control sync·dirsync 수, 평균 실제 batch 크기, 대기/dirty age p99, commit·rollback tail latency |
| offline backup/restore | C 일치·전체 검사·중단 시간·복사 bytes·취소/재장애 |

RSS는 고정 runtime baseline+buffer+metadata+private+system workspace+diagnostics+사전 고정 margin의 peak와 장기 기울기로 판정한다. reservation 성공을 일반 OOM 복구 보장으로 해석하지 않는다. 동일 durability/isolation·환경·fixture/recipe에서만 baseline/candidate를 비교하고, 실제 수치·반복·회귀 한도·계측 비용을 실행 전에 고정한다. 현재 성능 개선 주장은 없다.

batch deadline은 낮은 부하에서도 1-page flush가 진행되는 시간 상한과 함께 사전 고정한다. 큰 batch의 처리량만으로 작은 요청의 p99·rollback/eviction 지연 악화를 숨기지 않는다. version-edit gate 보유·재시도·대기와 tree/page/row/key lock 대기를 분리 계측한다. gate 분할·group commit·부분 batch retirement는 이 기준선의 실행 결과를 근거로 별도 검토한다.

<a id="freeze"></a>
## 6. 구현 전 동결

| ID | 산출물 | 완료 시점·현재 상태 |
| --- | --- | --- |
| D3-F01 | page/anchor/allocator/undo/status/WAL/CLR/checkpoint/DWB offsets·IDs·digest 범위·독립 hex vector | 표현 codec 전. 구조 선택만 완료, bytes 미동결 |
| D3-F02 | API·isolation·rank/try-lock/전체 pin 예약·FIFO progress turn/K/workspace/drain 순서·transient reference guard·statement/rollback/cut/queue 상태표 | 해당 소비 구현 전. D3-M01–M06 action/fixture·fairness/진행 판정과 Rust signature 미동결 |
| D3-F03 | 독립 key/row/index/history·edge graph oracle·negative mutants·case/variant 예상값 | 해당 production leaf 전 D3-M01–M06 전수 완료 필요; 모델/제품 실행 NOT_RUN |
| D3-F04 | 실제 WAL/DWB batch/home/namespace receipts·allocator bootstrap·각 장애 cut·반복 recovery | 실제 I/O leaf 전. batch1/8/64·다중 file·repair 재장애 profile 미동결 |
| D3-F05 | opcode별 자원 벡터·최악 CLR/rollback pages·progress frame/FD/WAL·큰 inventory quota·PF09 | 해당 effect 전 산식 대입·경계 인수 필요. 숫자 reservation table·batch deadline·실측 미완료 |
| D3-F06 | 실제 DEV2 공급 revision/API/format·재사용/adapter/전용/신규 symbol·시험 inventory·선행 leaf·clean export | production 착수와 S1 승격 각각. volatile/DEV2 형식 receipt로 DEV3 영속 충족 금지 |
| D3-F07 | 담당·기준 commit/dirty digest·독점/공유 경로·중복·병합 순서·통합 검사 | 각 작업 전. 역할 계획만 존재 |

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

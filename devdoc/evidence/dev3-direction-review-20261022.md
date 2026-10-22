# DEV3 진행 방향·설계 점검 — Status home flush 이후

2026-10-22 · **정적 검토 결과·개선 제안 / 설계 변경 채택·제품 개발·새 시험 실행 없음**

[개발 문서](../README.md) · [현재 구현](../implementation/dev3-implementation.md#dev3-work-breakdown) · [구현 계획](../plans/dev3-implementation-plan.md#work-organization) · [설계 정본](../contracts/disk-storage-design.md) · [테스트 계획](../plans/dev3-test-plan.md)

사용자의 DEV3 진행 방향·설계 타당성·개선점 점검 요청에 대한 결과를 저장한다. 검토 기준 HEAD는 `c859fbd645950095efdbfc17d247c0923644381b`이며, 검토 당시 제품 코드의 미커밋 변경은 없었다. 기존 `devdoc/README.md`·`devdoc/documents.json`의 미커밋 변경은 검토 대상 제품 변경으로 계산하지 않는다. 검토·기록 담당은 `/root`다.

이 문서의1–6절은 소유 계약과 코드·기존 검증 기록을 대조한 최초 정적 판단이며 당시 제안·실행 상태를 보존한다. 후속 반영 요청의 소유 계약 변경과 별도 D2 검토는 [7절 반영 추적](#adoption-followup) 및 [D3-DIRECTION-001](../plans/dev3-implementation-plan.md#direction-review-adoption)이 소유한다. 최초 리뷰 자체를 독립 인수나 제품 실행 증거로 바꾸지 않는다.

<a id="assessment"></a>
## 1. 전체 판단

**DEV3의 기본 설계 방향은 타당하다. 다만 지금부터는 개별 기반 기능을 확장하는 속도보다 쓰기·복구·거래를 끝까지 연결하는 순서를 더 중시할 필요가 있다.** 확인한 범위에서는 전체 구조를 다시 설계할 근거가 부족하며, 우선순위와 일부 확장 경계를 보완하는 것이 적절하다. 이는 설계 전체의 정확성·성능을 인증한 결론은 아니다.

현재는 DEV3 내부 S2의 기반 기능을 연결하는 단계다. [Status home flush](../implementation/dev3-implementation.md#status-home-flush)까지 제한 인수를 마쳤지만 일반 buffer, 실제 행 변경, live undo/CLR, commit과 일반 재시작 복구는 남아 있다. 기반 검증이 많이 쌓였다는 사실을 DEV3 완성에 가까워졌다는 의미로 해석하지 않는다. 표현·모델·component 완료와 실제 DB 기능 완료를 구별하는 현재 상태 보고는 유지할 가치가 있다.

작업 보류의 정확한 경계는 [작은 단위 종료·인계](../implementation/dev3-implementation.md#paused-after-status-home-flush)를 따른다. 후속 3.12.1.a의 LEASE/PREFLIGHT/SELECT는 사전 열람 상태이며, 확정 설계·제품 구현·시험 결과가 아니다.

<a id="retain"></a>
## 2. 유지할 설계

디스크 page·B+tree·단일 WAL·영속 undo, commit 시 모든 데이터 page의 flush를 요구하지 않는 방식, WAL→DWB→home 순서, 결과가 불명확하면 fence하는 정책은 서로 일관된다. 자원 예약과 실패 후 소유권을 명시하고, 과거 FAIL·중단·NOT_RUN을 보존하는 방식도 타당하다.

WAL을 먼저 영속화한 뒤 data page를 쓰는 원리와 doublewrite의 복원 역할은 [PostgreSQL WAL 설명](https://www.postgresql.org/docs/current/wal-intro.html), [MySQL 8.4 doublewrite 설명](https://dev.mysql.com/doc/refman/8.4/en/innodb-doublewrite-buffer.html)과 부합한다. 로컬 소스 대조 범위와 실제 revision은 [비교 근거](#sources)에 기록한다. 이 비교는 XADB의 영속 UndoRef·참조계수 회수·RECOVERY_REBASE가 해당 DBMS와 동일 알고리즘이라는 뜻도, 그 전체 정확성·성능이 입증됐다는 뜻도 아니다.

<a id="priorities"></a>
## 3. 개선 우선순위

아래 우선순위는 후속 개발 위험과 인수 준비의 순서다. 현재 제한 계약 안에서 데이터 유실을 재현한 결함 목록으로 해석하지 않는다.

### 1. 현재 쓰기 경로의 재시작 복구를 먼저 연결

다음 후보를 `3.12.1 LEASE/PREFLIGHT/SELECT`로 잡은 방향은 적절하다. 다만 중간 목표를 진입 절차 구현에서 끝내지 않고, **현재 지원하는 reserve·INIT·flush의 정상/장애 이미지를 새 프로세스에서 복구하고 검산하는 데까지** 잡는 것이 좋다.

현재 [Status home flush 선택 증거](dev3-status-home-flush-20261022.md)는 실제 파일·WAL·효과 순서를 확인하지만 일반 reopen 증거는 아니다. 이 간극을 tree·index·catalog의 변경 종류가 많아지기 전에 줄여야 재작업 위험이 작아진다.

좁은 복구 범위를 구현하더라도 지원하는 형식·연산과 거절 경계를 명시하고, 해당 결과를 일반 recovery/READY 완료로 승격하지 않는다. 필요한 장벽·자원 인계는 [복구 상세 계약](../contracts/dev3-task-12-recovery.md)과 [장벽별 구현 계획](../plans/dev3-implementation-plan.md#recovery-remaining)을 따른다.

### 2. tree 착수 전에 일반 page 수명 관리로 확장할 경계 확정

현재 [dirty_frame.rs](../../crates/engine/src/storage/dirty_frame.rs)의 `MAX_TARGETS`는 2다. [flush_io.rs](../../crates/engine/src/storage/wal_io/flush_io.rs)의 Status mapping은 같은 파일의 allocator page 0·Status page 1에 고정되며, 기존 Status home은 zero 또는 현재 이미지와 정확히 같은 경우만 허용한다. 최초 Status INIT에는 맞지만 거래 상태가 반복 변경되는 경로에는 그대로 사용할 수 없다.

이는 현재 계약 안의 버그가 아니라 **다음 기능을 막는 의도된 제한**이다. 일반화 작업에서는 동적 page 등록, dirty/clean 전환, eviction·재로딩, 같은 물리 page의 단일 변경 권한을 함께 다뤄야 한다. 선택 namespace의 불변성을 전제로 하는 [clean buffer](../../crates/engine/src/storage/read_buffer.rs)와 dirty pool 사이의 소유권·접근 배제 관계도 명확히 해야 한다.

고정 상수만 늘리거나 기존 Status predicate를 느슨하게 만드는 것으로 완료하지 않는다. 실제 tree와 live Status가 요구하는 page 집합·정체성·재로딩·flush 수명을 기준으로 관련 계약과 필요한 변경 시험을 연결하는 것이 좋다.

### 3. 영속 undo·예약 복원·RECOVERY_REBASE를 가장 큰 설계 위험으로 관리

영속 undo로 오래된 거래와 WAL 보존을 분리하는 선택에는 이유가 있다. 그러나 incoming-reference count, logged cut/release, CLR, 잔여 예약, 두 REBASE 슬롯까지 결합한 전체 방식은 XADB에서 직접 입증해야 하는 부분이다.

특히 **공간 한도에 가깝게 admitted된 정상 작업이 장애 후에도 기존 예약으로 복구를 끝낼 수 있는지**가 중요하다. 정상 이미지를 quota 부족으로 거절하는 것만으로 이 의무를 충족할 수 없다. 작은 모델의 결과도 실제 bytes·namespace·I/O·반복 재시작의 증거를 대체하지 않는다.

첫 실제 행 변경부터 undo·rollback·재장애를 함께 연결하고, index/catalog의 모든 기능이 끝날 때까지 미루지 않는 편이 좋다. 소유 계약은 [영속 undo](../contracts/disk-storage-design.md#undo), [복구 예약](../contracts/disk-storage-design.md#recovery-reservation), [RECOVERY_REBASE](../contracts/disk-storage-design.md#recovery-rebase)다. 기존 모델·실행 결과는 원래 source와 범위로 보존한다.

### 4. 대형 인수용 자원 profile을 미리 확정

이미 발견됐지만 해결이 끝나지 않은 인수 제약이다. [기존 자원 계산](../contracts/dev3-resource-bounds.md)에 따르면 기본 64 MiB buffer의 allocator inventory 4배 조건에는 최소 43,241개 extent, extent만 45,341,474,816 bytes, 약 **42.23 GiB**가 필요하다. 기본 8 GiB quota로는 충족할 수 없다. 16 MiB buffer 조건도 기본 quota를 넘는다.

문서에 제안된 64 GiB 인수용 profile의 전체 회계·형식·fixture 준비를 진행해야 한다. S5 직전에 발견할 실행 환경 문제로 남겨두면 안 된다. 제품 기본 한도를 함께 확대할 필요는 없다. 64 GiB 준비 방향을 수용한 사실과 해당 profile의 구현·지원·실행 완료는 계속 구별한다.

필요한 실제 공간·입력 준비를 먼저 계산하되 장시간·대규모 실행은 [최종 S5 계획](../plans/dev3-test-plan.md#final-suite)을 따른다. 작은 fixture나 반복 읽기·zero padding·sparse apparent size로 기존 인수 분모를 대신하지 않는다.

### 5. 보수적인 직렬화 비용을 관측할 준비

전역 version-edit gate, Commit 설치 완료까지 후속 page 변경을 막는 장벽, batch1의 여러 control 영속화는 정확성을 설명하기 쉬운 선택이다. 동시에 처리량과 지연의 주요 위험 후보이기도 하다. 이번 검토에는 현재 성능이 나쁘다고 판정할 실행 근거가 없다.

새 경로를 구현할 때 WAL sync 대기, 의미 설치 장벽 대기, DWB/control sync, 실제 batch 크기, 성공 거래 수와 충돌·거절을 구별해 관측하도록 준비하는 것이 좋다. [Commit 설치 장벽](../contracts/disk-storage-design.md#commit-installation-order)과 [PF09](../plans/dev3-implementation-plan.md#performance)의 기존 의무를 실제 경로에 연결한다.

장시간 성능 실행은 기존 계획대로 S5에 두고, 측정 전 gate 분할이나 group commit을 추가할 이유는 없다. correctness 확보 뒤 같은 의미·durability·환경에서 비교한다.

<a id="next-boundaries"></a>
## 4. 권장하는 다음 완료 목표

| 순서 | 실제로 닫을 기능 | 완료 범위의 구분 |
| --- | --- | --- |
| 1 | 현재 allocator/Status 경로의 제한 복구 | 지원하는 연산의 정상·장애 이미지를 새 프로세스에서 복원·검산. 일반 거래·READY 완료와 구별 |
| 2 | 일반 buffer와 live undo/status | page의 단일 권한, load/dirty/flush/eviction 수명과 undo·상태 변경 연결 |
| 3 | 작은 행 거래의 commit·rollback·crash/reopen | ACK 보존·loser 제거·실패 원자성을 해당 지원 범위에서 끝까지 연결 |
| 이후 | index·catalog·checkpoint·회수 기능 확장 | 새 대상마다 같은 거래·복구 경로와 직접 영향 검증을 함께 확장하고 최종 S5 의무 유지 |

이 표는 기존 직접 선행과 필수 장벽을 생략하는 새 실행 계획이 아니다. [기존 작업 순서](../plans/dev3-implementation-plan.md#work-priority) 안에서 실제 기능을 조기에 연결할 제안이다. 작은 commit 단위는 유지하되 진행 보고의 중심을 완료한 하위 단위 수보다 **어떤 실제 기능이 끝까지 동작하는가**에 두는 것이 좋다.

<a id="sources"></a>
## 5. 직접 확인한 비교 근거

검토 시 아래 두 로컬 checkout의 origin·HEAD·clean 상태를 확인하고 명시한 구간을 읽었다. 외부 checkout은 수정·빌드·실행하지 않았다. 전체 DBMS 소스 또는 다른 checkout을 새로 검토했다는 의미는 아니다.

| 대상·실제 revision | 직접 열람한 구간 | 확인한 경계 |
| --- | --- | --- |
| PostgreSQL · `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` · origin `https://github.com/postgres/postgres.git` | `src/backend/access/transam/xlogutils.c` 265–306행 | WAL replay의 page별 LSN 판정과 full-page image 복원 설명 |
| MariaDB · `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` · origin `https://github.com/MariaDB/server.git` | `storage/innobase/buf/buf0flu.cc` 870–933행 | direct flush의 WAL 영속화 확인과 doublewrite batch 경로 분기 |

공식 웹 자료는 2026-10-22 열람한 [PostgreSQL WAL](https://www.postgresql.org/docs/current/wal-intro.html)과 [MySQL 8.4 Doublewrite Buffer](https://dev.mysql.com/doc/refman/8.4/en/innodb-doublewrite-buffer.html)다. 전자는 WAL-before-data·commit 시 data flush 생략의 원리를, 후자는 doublewrite 복원과 순차 묶음 쓰기의 원리를 대조하는 데 사용했다. XADB의 성능 수치나 전체 설계 적합성을 이 자료에서 추정하지 않았다.

<a id="execution-state"></a>
## 6. 변경·검증 상태

검토 과정에서는 제품 파일 수정·제품 테스트·compiler/Clippy·성능 측정·문서 verifier를 실행하지 않았다. 이번 저장은 검토 결과와 문서 색인만 기록한다. 기존 FAIL·중단·NOT_RUN 및 DEV2·DEV3/S5 미완료 상태를 바꾸지 않는다. 새 독립 D1/D2 검토나 실행 PASS를 주장하지 않으며 commit·push는 수행하지 않는다.

향후 구현에서는 [사용자 시험 제한](../../AGENTS.md)과 [DEV3 테스트 계획](../plans/dev3-test-plan.md)을 유지한다. 중간 단계는 실제 변경 경로의 필수 시험만 선택하며, 최종 S5에서는 확정된 DEV3 inventory를 전수 실행한다. 문서 자동 검사는 기존 push gate에 둔다.

<a id="adoption-followup"></a>
## 7. 후속 반영과 남은 구현

2026-10-22 사용자가 방향·구성·지적사항의 실제 반영을 요청했다. 리뷰를 별도 권고문으로만 남기지 않고 기존 계획·소유 계약·공급도·시험 배정·현재 상태를 수정했다. 결정 상태와 독립 검토는 [D3-DIRECTION-001](../plans/dev3-implementation-plan.md#direction-review-adoption)에 기록한다.

작성자 외 `/root/direction_independent_review`의 실제 변경 검토를 거쳐 D2 ACCEPTED로 반영했다. 초안의 우선순위/기술 공급 혼동을 수정하고 공통 terminal→최소 catalog create→영속 table→첫 행 거래의 공급 순서를 명시했다. 최종 검토 PASS, 통합 차단 잔여 지적 없음이며 제품 실행 인수는 아니다.

| 지적 | 반영한 규칙·구성 | 아직 필요한 제품 산출물 |
| --- | --- | --- |
| 복구 연결 지연 | [우선순위](../plans/dev3-implementation-plan.md#work-priority)의 첫 목표를 현재 쓰기의 새 프로세스 복원·검산으로 고정. [제한 복구 계약](../contracts/dev3-task-12-recovery.md#early-recovery)에 닫힌 admission·미지원 prefix·원 WAL 보존·일반 READY와의 경계 명시 |3.12.1–4의 실제 지원 closure/API/예약·복구·독립 검산. 다음 leaf는3.12.1.a이며 파일 열기만으로 통합 목표 완료 아님 |
| 일반 page 수명 부재 | [일반 pool 공급 조건](../contracts/dev3-task-05-buffer-flush.md#operational-pool)에 clean reader 전환·단일 주소 권한·eviction/reload·반복 Status·stale completion을 배정. 오래된 Status flush NOT_RUN 표기를 실제 제한 증거로 수정 | 일반 pool/주소 등록·소유 전환·실제 thread/페이지 수명 인수. 기존 두-target 제한을 버그로 간주해 완화하지 않음 |
| undo/예약/REBASE 통합 위험 | [첫 행 거래](../contracts/dev3-task-11-commit.md#first-row-integration)를 index/catalog 전체와 분리하고 공급도·상세 설계 순서 일치. 실제 최소 table 공급·terminal queue·전체 복구 장벽·정상 admitted 성공 조건 유지 | live undo/CLR·commit·REBASE/epoch·quota 경계의 실제 재장애·ACK 증거. logged cut/release 등 후속 회수의 전체 의무 유지 |
| 대형 인수 profile 미정 | [64GiB 준비](../contracts/dev3-resource-bounds.md#qualification-profile-preparation)에 tuple·file/extent 배치·전체 공간/예약·8,192-entry workspace·환경/생성기 책임과 시점을 배정 | 새 profile의 별도 동결·구현·전체 회계·실제 fixture. 현재 supported8GiB와 구별; 대규모 실행 S5 |
| 직렬화 비용 불명 | [계측 공급](../contracts/dev3-task-16-qualification.md#observation-delivery)에 WAL·의미 설치·DWB/control·실제 batch·실패/충돌 분모, clock/중복 시간/잘림·자원 예산 고정 의무 배정 | 구현 동반 counter/trace·작은 집계 시험·S5 측정. gate 분할/group commit은 미채택 |

[DIR01–05](../plans/dev3-test-plan.md#direction-integration)를 기존 NT owner에 연결하고 중간 필수와 S5 확대를 구분했다. 이미 완료한3.03.1을 현재 재개 대상으로 읽던 계획·시험 문구와 과거 전용 병렬 배치의 시점을 바로잡았다. 기존 task ID/DAG·전체 인수 분모·원본 PASS/FAIL/NOT_RUN은 유지한다.

이번 반영은 문서·개발 경계 수정이다. 제품 코드를 바꾸거나 무변경 시험을 실행하지 않았다. 위 미공급 구현은 해결 완료로 표시하지 않으며, 독립 설계 검토도 실제 기능·durability·성능 PASS를 뜻하지 않는다.

# XADB — AI가 개발하고 검증할 수 있는 데이터베이스 설계

DEV3 저장 경로의 현행 정본은 [disk-storage-design](../contracts/disk-storage-design.md#decision)이다. 이 종합 설명의 COW/private·root 불변·global-q 및 불변 checkpoint 표현은 DEV2 기준선/이전 제안으로 한정한다. 후속 DISK는 in-place·WAL-before-page·영속 undo·fuzzy checkpoint를 사용하며 기존 제품 구현 완료를 뜻하지 않는다.


> **제안의 중심:** XADB는 AI가 코드를 많이 생성하는 제품이 아니라, AI가 자신의 변경을 작은 범위에서 재현하고 반증할 수 있는 제품으로 만든다. 새 엔진, 테스트 플랫폼, 개발 도구를 하나의 제품으로 설계한다.

작성: 2026-10-18 · 문서 상태: **설계 제안 / 제품 미구현** · 대상: 제품 책임자, DB 엔진 개발자, 테스트·운영 담당자, AI 개발 에이전트





**추가 설계 요구:** 일반 재현·테스트·디버깅·리뷰가 보안 중단을 유발할 만한 불필요한 행위를 요구하지 않아야 한다. [14.4–14.12절](#execution-design)은 OpenAI 공식 지침을 참고해 **합성 데이터·소유한 테스트 자원·엔진 내부 fault·읽기 전용 진단**으로 정상 개발을 끝내는 설계다. 우선순위는 중단 예방이며, 기록 보존은 보조 수단이다. 핵심 실행 흐름은 **DB에서 발견 → [모듈별 최소 재현](#module-reproduction) → 국소 수정 → 필요한 통합 회귀**이며, [Astra/Luna 역할 분담](#agent-routing)과 [파일 중심 증거 전달](#artifact-evidence)을 함께 설계했다.

디스크 파티션을 포괄하는 native global index는 NG 단계에서 개발한다. 초기 DEV4 SQL의 선행 조건은 아니며 실제 구현·인수는 단계별 계약을 따른다. 아래 명령, API, 디렉터리, 단계별 기준은 별도 표시가 없으면 **앞으로 구현할 계약**이다. 조사용 Python 모델을 실행한 것과 실제 DB를 검증한 것을 구분한다.

**원인 범위 한정 설계:** [5.6절](#flow-contracts)에 데이터·객체·소스 참조 계약, [13.6절](#causal-diagnosis)에 오류에서 후보를 좁히는 절차와 가상 사례, [16.8절](#causal-math)에 후보 집합·누락률·진단 비용의 수학적 정의를 추가했다.

<a id="section-1"></a>

## 1. 제품 방향

[제품 방향](../plans/project-direction.md)을 따른다. Linux에서 Rust로 관계형 DBMS를 개발하고, 폭넓은 기능·기본 성능·이해 가능한 구조를 함께 추구한다. 이 문서는 이관한 일반 설계 제안이며 현재 구현은 [구현 색인](../implementation/README.md)을 따른다.

<a id="section-2"></a>

## 2. 공개·호환 범위

연구용 소스 공개와 안정 제품 qualification을 구분한다. 개발 fixture는 새로 만들며 안정 릴리스의 same-major 호환은 [버전 정책](../policies/version-compatibility-policy.json)에서 명시적으로 선언한다.

<a id="section-3"></a>

## 3. 이관과 참고 입력

DEV0/DEV1·DEV2 일부 코드와 일반 설계·검증 원칙을 이관했다. 이전 실행 결과는 새 결과로 재표기하지 않는다. 향후 [공개 DBMS](opensource-references.md)를 비교하고 선택 이유와 시험을 소유 계약에 기록한다.

<a id="section-4"></a>

## 4. AI 친화성을 구조의 속성으로 정의한다

“AI가 읽기 쉬운 이름”은 필요하지만 충분하지 않다. 다음 다섯 조건을 설계 완료의 기준으로 삼는다.

| 조건 | 제품이 제공해야 할 것 | 관찰할 지표 |
|---|---|---|
| 국소 이해 | 한 모듈의 입력·출력·불변식·소유권·실패 의미가 닫혀 있음 | 변경 계약 수, 함께 바뀐 모듈 수, 공개 API 노출 |
| 실행 가능 | checkout 뒤 한 진입점에서 toolchain·환경·fixture가 고정됨 | fresh checkout 성공률, 첫 유효 반례까지 시간 |
| 재현 가능 | seed, schedule, input, binary/config 지문으로 재실행 | 동일 artifact의 재현률, 비재현 실패 비중 |
| 판정 가능 | 독립 oracle·경로 witness·미지원 상태가 구분됨 | 계약 충족률, mutant 검출률, INVALID·SKIP 비중 |
| 인계 가능 | 현재 설계·증거·미해결 반례가 기계적으로 연결됨 | 재개 시 환경 재구성 시간, 중복 조사·재개방률 |

여기서 AI 친화성 점수 하나를 임의로 만들지 않는다. 개발 시간은 빨라져도 데이터 유실이 늘면 개선이 아니며, 반대로 초기 검증 장치 투자로 첫 기능이 느려질 수 있다. **변경 완료 시간과 결함 유출·재현 비용을 함께 측정**한다.

<a id="section-5"></a>

## 5. 권장 아키텍처와 모듈 경계

### 5.1 실행 구조

```text
XADB Client / XADB CLI / XADB protocol
                  |
       SQL parse → bind → logical IR → optimize → physical plan
                  |
           execution operators
                  |
        transactional Engine API
                  |
   snapshot / private writes / commit coordinator
                  |
     row + index + catalog state transition
                  |
     WAL codec + checkpoint + storage manager
                  |
    effect ports: file / clock / scheduler / entropy
                  |
      production adapters  |  simulation adapters

XADBTest calls the SAME Engine API and state-transition code.
Observability records stable events across these boundaries.
```

이 그림은 **호출·데이터 흐름**이다. 모든 상자가 독립 crate라는 뜻은 아니다. [배치 정책](../policies/repository-build-plan.md)에 따라 초기 컴파일 경계는 공통 `xadb-foundation`, Linux 구현 `xadb-platform`, 공개 engine API로 두고, 독립 계약이 필요할 때만 contract crate로 분리한다. SQL 없는 단계는 embedded engine에서, DEV5 이후 상위 구성은 `xadb-server`에서 조립한다. core가 서버·CLI·테스트 runner를 import하는 역방향 의존은 금지하며 실제 자동 검사 범위와 후속 CI 의무는 배치 정책에서 구분한다.

### 5.2 모듈과 책임

| 역할 | 초기 배치 | 소유하는 상태·책임과 경계 |
|---|---|---|
| 공통 타입·값·오류 | `crates/foundation`의 `xadb-foundation` | ID·값·오류의 최소 의미. 전 모듈의 편의 함수 집합으로 성장시키지 않음 |
| SQL 해석 | 계획된 `crates/sql`의 `xadb-sql` | parser·binder·typed AST. catalog API 사용, WAL·page 내부 참조 금지 |
| 계획·최적화·실행 | `xadb-sql` 내부 모듈로 시작할 계획 | 논리·물리 IR·비용·연산자. plan에 저장 포인터 금지, executor는 engine API 사용 |
| 거래·공개 | 계획된 `crates/engine/src/transaction`·`publication` | snapshot·private batch·validation·commit 순서. publish/ack·abort 의미의 단일 소유자 |
| catalog | 계획된 `crates/engine/src/catalog` | schema·이름·의존 metadata. 동일 원자적 root에 포함, SQL executor 역참조 금지 |
| 영속성 | 계획된 `crates/engine/src/durable` | WAL·replay·checkpoint. foundation effect 계약 사용, recovery에서 SQL 의미 재구현 금지 |
| OS effect 계약과 구현 | `crates/foundation` / `crates/platform` | `xadb-platform → xadb-foundation`. worker·CLI·server가 실제 파일·시간·scheduler 구현을 조립 |
| 진단 | foundation의 현재 bounded event 기반, 모듈별 후속 확장 | core 상태를 수정하지 않는 관측. 오류 재귀·무한 버퍼 금지 |
| 서버 | 계획된 `crates/server`의 `xadb-server` | session·auth·network·config·lifecycle 조립. 엔진 의미 중복 구현 금지 |
| 시험 | 현재 `xadbtest` 계열, 계획된 reference·engine worker | worker는 production API 호출, reference는 독립 기대값 계산. 제품에 시험용 정답 경로 금지 |
| CLI | 현재 `crates/testing/controller`, 계획된 `crates/cli`의 `xadb-cli` | typed result·exit code·schema. 초기 SQL CLI 이후 운영 기능은 해당 단계에서 확장 |

제품 폴더에는 `xadb-`를 붙이지 않으며 Cargo package 이름과 구분한다. 초기 `xadb-types/xadb-plan/xadb-opt/xadb-exec/xadb-txn/xadb-storage/xadb-catalog/xadb-durable/xadb-runtime/xadb-observe/xadb-testkit` 표기는 역할 제안이었다. 현재·계획 경로는 [저장소 배치 정책](../policies/repository-build-plan.md)을 따른다. 첫 커널은 내부 모듈로 시작하고 독립 계약이 필요할 때 crate를 분리한다. 타입당 파일·파일당 crate 규칙이나 획일적 LOC 제한은 두지 않는다. 현재 manifest·일부 normal dependency 검사를 사용하고 전체 feature·target·build·dev 의존과 FFI·생성 코드·런타임 등록 검사는 구현 범위에 맞춰 추가한다.

### 5.3 다섯 가지 그래프를 연결한다

1. **컴파일 그래프:** crate/include/import, public API, FFI. 현재 manifest와 일부 의존 검사를 기반으로 순환·내부 접근 검사를 확대한다. 전체 그래프 자동 분석 완료를 뜻하지 않는다.
2. **소유권·참조 그래프:** 누가 snapshot, cursor, page pin, file, task를 생성·소유·대여·공유하고 언제 반환하는가. type/guard와 연결하고 논리 객체의 generation과 상태 version을 구분한다.
3. **실행·순서 그래프:** declaration → registration → implementation selection → call. 함수 포인터·dispatch가 있으면 선택된 implementation ID를 trace에 남긴다. 메시지 송수신·동기화·취소·영속 완료의 선후 관계를 연결한다.
4. **증거 그래프:** requirement → contract → source → fixture → witness → oracle → run. 변경 영향 분석과 완료 보고서를 여기서 생성한다.
5. **데이터 계보 그래프:** 어느 입력·상태 version에서 값을 만들고, 누가 변환·저장·공개·복구하여 누가 소비했는가. 오류가 난 값에서 실제 생산자와 변경자를 역추적한다.

하나의 거대 지식 그래프를 먼저 구축할 필요는 없다. Cargo/build metadata와 작은 JSON manifest에서 시작한다. AI의 검색은 lexical search와 semantic symbol index를 함께 사용하고, 오래된 자연어 요약을 실행 증거보다 우선하지 않는다.

### 5.4 결정적 실행을 가능하게 하는 경계

core의 상태 전이는 가능한 한 `현재 상태 + 명시적 입력 → 새 상태 + effect 요청` 형태로 작성한다. I/O 완료, timer, 취소, 다른 actor의 메시지는 event로 돌아온다. production adapter는 실제 OS와 worker를 사용하고, simulation adapter는 같은 event consumer에 가상 완료·오류·순서를 공급한다. simulation용으로 transaction 알고리즘을 따로 작성하면 검증 대상이 갈라지므로 피한다.

| Port | 계약에 포함할 사항 | simulation에서 제어할 것 |
|---|---|---|
| File I/O | 요청 길이·실제 길이, sync 범위, error, file identity | short write, delay, persistence subset, corruption |
| Clock / Timer | monotonic deadline과 wall-clock 표시의 구분 | 가상 시간 진행, timeout 경계 |
| Entropy | seed와 stream identity | 값·소비 순서·입력 저장 |
| Scheduler | actor event 순서, yield/barrier, cancellation | 선택된 interleaving, preemption bound |
| Resource budget | reserve/consume/release, 실패 원자성 | OOM/quota 실패 지점 |
| Trace sink | bounded buffer·drop count, 상태를 바꾸지 않는 기록 | witness 완결성·유실 검증 |

core의 직접 syscall·실제 시간·전역 RNG·임의 thread 생성은 경계 검사 대상으로 삼는다. 실제 atomics·lock-free 자료구조는 event simulation만으로 메모리 모델까지 검증되지 않으므로 별도 bounded checker와 실제 thread stress가 필요하다. 초기에는 명료한 lock·message ownership을 선호하고 필요성이 입증될 때만 더 복잡한 동기화를 추가한다.

### 5.5 repository와 설계 문서 구조


```text
xadb/
  Cargo.toml / Cargo.lock / rust-toolchain.toml
  crates/                 # 현재 foundation + platform + engine + XADBTest, 총 9개 crate
    foundation/           # 현재: 공통 의미·effect 계약
    platform/             # 현재: Linux effect 구현
    engine/               # 계획: DEV2 api/catalog/storage/transaction/durable
    xadbtest-reference/     # 계획: 독립 row/history oracle
    xadbtest-engine-worker/ # 계획: 실제 engine API worker
  scripts/                # 현재 check_dev0.py, check_dev1.py
  tests/cases/            # 현재 typed 시나리오와 기대값
  tests/corpus/           # 계획: 최소 반례·known-bad history
  tests/schedules/        # 계획: actor/barrier
  tests/durability/       # 계획: crash/fault
  tests/sql/              # 계획: SQL/typed expected
  tests/versions/         # 계획: 안정 릴리스 기준선부터 적용
  devdoc/                 # 모든 설계·명세·계약·ADR·모듈 문서
  benches/                # 계획: workload와 측정 기준
  clients/jdbc/           # 계획: DEV6 비 Rust client
  packaging/             # 계획: 배포 recipe
  .github/workflows/      # 현재 DEV0/DEV1 로컬 검사와 같은 entry point
  target/                 # Cargo 산출물, Git 제외
  artifacts/              # 실행 증거·향후 배포 산출물, Git 제외
```

기존 제안의 루트 contracts/specs/adr/docs/modules는 설계 자료일 때 `devdoc/` 하위에 둔다. 실행 타입은 해당 crate가 소유한다. tools/ax·tools/execution 역할은 우선 Rust CLI/XADBTest crate와 scripts가 맡는다. 5.2절의 모듈별 이름을 모두 독립 crate로 만드는 계획은 아니다.

모듈 문서는 책임·public API·state machine·ownership·오류·테스트 진입점·관련 ADR만 담는다. 상세 실행 결과와 과거 조사 일지를 계속 붙여 현재 계약을 흐리지 않는다. feature 상태 표는 manifest에서 생성하고, 문서에서 VERIFIED로 바꾸는 것만으로 gate가 통과하지 않게 한다.

<a id="flow-contracts"></a>

### 5.6 데이터 흐름과 객체 참조를 개발 계약으로 만든다


| 구분 | 정적으로 선언할 것 | 실행 시 연결할 것 | 답할 수 있는 질문 |
|---|---|---|---|
| 소스 참조 | module·symbol·type·field, import/call/dispatch 후보 | source/build ID, 실제 implementation ID, 호출 event | 어느 코드가 이 처리를 했는가? |
| 데이터 흐름 | 입력·출력 schema, producer/consumer, 변환 계약 | input/output version, 생성·변경 event, 관련 상태 요약 | 잘못된 값은 어디에서 들어왔는가? |
| 객체 관계 | owner, borrow/share 권한, 허용 writer, 수명 | object ID·generation·version, owner 이전, acquire/release | 누가 이 객체를 바꿀 수 있었고 참조가 유효했는가? |
| 제어·순서 | 선택 조건, cache key, message·lock·I/O 의존 | branch/implementation, send→receive, publish→read, sync receipt | 잘못된 값·오래된 버전을 왜 선택했는가? |
| 경계 검증 | pre/postcondition, invariant, 독립 oracle | 어떤 상태에서 어느 predicate를 검사했고 무엇이 실패했는가 | 마지막 정상 경계와 최초 위반 경계는 어디인가? |

객체 식별자는 제안상 `(run_id, object_kind, object_id, generation)`으로 정하고, 동일 객체의 변경은 별도 `state_version`으로 구분한다. `generation`은 해제·재생성 후 같은 ID의 재사용을, `state_version`은 살아 있는 객체의 변경을 구분한다. 메모리 주소·파일 경로만으로 identity를 판단하지 않는다. 재시작 전후의 영속 객체는 DB/file generation·WAL 위치와 origin artifact ID로 연결하며, run ID가 바뀌었다고 이전 생산 이력을 끊지 않는다. immutable 객체의 파생본은 `derived_from`을, mutable 객체는 상태 전이와 허용 writer를 기록한다. 하나의 `last_writer`만 남기면 이전 오염이나 다중 writer race를 놓치므로, 선언된 재현 구간의 변경 이력과 미확인 writer 가능성을 보존한다.

예를 들어 `PlanRef`는 plan ID와 catalog epoch, `Cursor`는 snapshot guard와 scan 대상 generation, `PageRef`는 file generation·page ID·page version을 함께 가진다. 구조체 전체를 다른 모듈에 수정 가능하게 넘기기보다 읽기 전용 view·소유권 이전·명시적 변경 API를 제공한다. source 파일은 exact revision의 symbol·span에서 찾아가며 사람이 관리하는 줄 번호를 identity로 삼지 않는다.

데이터 edge는 **생산자 → 소비자**, 소스 의존 edge는 **사용자 → 의존 대상**일 수 있다. 종류와 방향을 schema에 명시하고 한 방향으로 무작정 합치지 않는다. 객체 참조·호출에는 순환이 있을 수 있으므로 모든 그래프를 DAG로 강제하지 않는다. 대신 특정 실행의 순서 관계와 version별 데이터 관계를 검사한다. timestamp가 앞선다는 이유만으로 인과 edge를 만들지 않는다.

로그에 존재하지 않는 동작도 중요하다. 무효화 누락, 메시지 미전달, WAL record 누락은 실제 event만 모으면 보이지 않는다. `요청된 무효화 → 완료 의무`, `ACK → 보존되어야 할 commit`처럼 **기대되는 동작과 의무**도 계약에 넣는다. 공통 allocator, 전역 설정, clock, storage adapter, oracle은 명시적 의존성 또는 아직 제외할 수 없는 후보로 남긴다.

출처·파생 관계의 개념은 W3C PROV를 참고할 수 있으며 RDF 서버 전체를 도입할 의무는 없다. OpenTelemetry의 span·link는 실행과 비동기 작업을 연결하는 기반으로 사용할 수 있다. **XADB가 추가로 정의할 객체 version·변경 권한·불변식의 정확성까지 이 도구들이 자동 보장하지는 않는다.** [W3C PROV 개요](https://www.w3.org/TR/prov-overview/), [OpenTelemetry traces](https://opentelemetry.io/docs/concepts/signals/traces/)

#### 구현 부담을 제한하는 도입 순서

1. **설계 시:** 모듈별 manifest에 입력·출력·소유자·writer·상태 전이·오류 ID·재현 entry를 선언한다. 첫 대상은 catalog→plan cache→executor와 commit→WAL→recovery 두 흐름이다.
2. **코딩 시:** API 타입·guard·effect port가 선언을 강제하게 한다. 빌드 metadata·symbol index에서 참조를 생성하고, 새 mutable 공유·FFI·dynamic dispatch는 계약 변경으로 식별한다.
3. **시험 시:** 실제 core의 경계 event와 불변식 결과를 연결한다. 필수 witness 유실·미해결 dispatch·revision 불일치는 “범위 확정 불가”로 표시한다. boundary test가 선언과 실제 코드의 어긋남을 검출해야 한다.
4. **운영·성능 시:** 평소에는 ID·version·상태 전이·bounded ring buffer, 재현에서는 관련 객체의 상세 trace를 사용한다. 모든 row의 모든 읽기를 항상 기록하지 않는다. 과거 상세 이력이 없으면 새 scoped 재현으로 보완한다.

event당 크기를 b byte, 초당 event 수를 r, 보존 시간을 t라 하면 단순 보관량은 `b × r × t`다. 가상 예로 96 byte × 50,000/s × 30s는 약 144 MB이며 index·allocator 비용은 별도다. “작은 ID만 기록하니 공짜”라고 가정하지 않고 trace on/off의 throughput·tail·메모리를 비교한다. 내용 hash는 값의 동일성 보조 수단이며 의미적 정상성 검사가 아니다. 민감 값의 hash도 기본 수집하지 않는다.

진단 schema와 contract ID도 버전 관리한다. 같은 XADB major의 새 도구가 과거 minor bundle을 읽거나 변환하도록 golden fixture를 보존한다. 새 schema를 모르는 옛 도구의 해석까지 요구하지 않는다. 새 기능 완료 조건에 **흐름 계약 갱신·계보 검증·최소 재현 entry**를 포함해야 시간이 지나도 이 구조가 유지된다.

<a id="section-6"></a>

## 6. 트랜잭션·저장·복구를 함께 설계한다

### 6.1 첫 커널: 불변 snapshot과 private write batch

트랜잭션은 시작 시 공개된 root와 commit sequence를 고정한다. 읽기는 이 snapshot과 자신의 write overlay를 결합한다. 변경은 private batch에 쌓고 다른 트랜잭션에 아직 노출하지 않는다. cursor는 `SnapshotGuard`보다 오래 살 수 없고, RowId는 재사용 가능한 물리 포인터와 구분한다. `ObjectId + generation`으로 drop/recreate와 stale handle을 판별한다.

첫 버전의 write transaction은 commit lock 안에서 **시작 epoch = 현재 공개 epoch**를 검사한다. 다르면 직렬화 충돌로 재시도하게 한다. 이는 필요 이상으로 많은 트랜잭션을 abort하지만 key/range phantom 검증을 미완성 상태로 출시하는 것을 피할 수 있다. read-only transaction은 고정된 snapshot을 읽는다. 이 규칙과 원자적 root 공개, 모든 입력의 snapshot 일관성을 함께 검증해야 serializable 주장을 할 수 있다.

후속 버전에서 read/write set·부재 조회·범위 predicate·unique 검사에 version validation을 도입한다. key 값만 검사하고 “없던 key가 삽입되는 경우”를 빠뜨리면 phantom을 막지 못한다. **snapshot isolation을 serializable이라고 부르지 않는다.** 격리 수준은 허용/금지하는 history로 명세한다. [일관성 모델 설명](https://jepsen.io/consistency/models)

### 6.2 커밋의 단일 상태 기계

```text
Active → Validating → Prepared → WAL-Data-Appended
       → Commit-Record-Attempted → WAL-Sync-Confirmed
       → Published → Reply-Sent

Before attempting the commit record: Abort may discard the private batch.
Once the commit record may have reached storage: an uncertain result is UNKNOWN.
After confirmed durable commit: recovery must preserve it even if the reply is lost.
```

commit coordinator가 lock을 보유한 동안 validation, sequence 부여, 완결된 batch의 WAL 기록, flush, 공개를 순서대로 수행한다. 초기 구현은 이 전체를 직렬화한다. `Prepared`에서는 공개에 필요한 메모리와 메타데이터를 미리 확보해 **durable 이후의 root publish를 실패하지 않는 연산**으로 줄인다. 그래도 I/O 결과가 불명확하거나 내부 오류가 나면 성공을 꾸미지 말고 commit coordinator의 새 쓰기를 중지한 뒤 복구로 결과를 확정한다.

특히 **sync 성공 응답이 없다고 commit이 디스크에 없다는 뜻은 아니다.** commit record 기록을 시도한 뒤에는 flush 오류·취소가 발생해도 곧바로 ABORTED를 반환하면 안 된다. 이미 영속화되었을 가능성이 있으면 UNKNOWN으로 처리하고, recovery가 완결된 batch의 존재를 판정한다. 미응답 transaction이 복구 후 나타나는 것은 허용하지만 명시적으로 abort를 확정한 transaction이 나타나면 계약 위반이다.

WAL의 redo payload는 SQL text가 아니라 확정된 object ID·row/key after-image·catalog delta·sequence를 사용한다. recovery가 SQL parser, optimizer, 현재 wall-clock, random 값을 다시 실행하면 재현성이 깨진다. 데이터·인덱스·catalog mutation은 같은 batch의 원자성 단위다. 한쪽만 먼저 공개하지 않는다.


핵심 불변식:

```text
ACK(T) ⇒ durable_lsn ≥ end_lsn(commit_batch(T))
Published(T) ⇒ complete durable batch(T)
VisibleRoot = one complete committed state, never a partial batch
Recover(checkpoint, WAL) = a legal committed state containing every acknowledged commit
```

클라이언트가 응답을 받지 못한 거래는 commit되었을 수도 있다. `COMMITTED / ABORTED / UNKNOWN`을 구분하고, 요청 ID를 쓴다면 dedup 정보도 같은 영속 batch에 넣는다. 단순 재전송이 exactly-once를 보장한다고 문서화하지 않는다. cancel도 durable commit 이후에는 rollback으로 바꾸지 않는다.


### 6.3 WAL 형식과 복구 규칙

WAL에는 format version, record type, length, transaction ID, sequence, payload checksum, batch record count·digest를 둔다. reader는 길이·범위·버전을 검증하고 완결된 commit batch만 replay한다. 부분 기록·중복 replay·잘못된 순서·알 수 없는 버전을 각각 테스트한다. 단순 checksum은 악의적 변조 방어를 대신하지 않는다.

replay는 sequence 기준으로 idempotent해야 한다. 이미 checkpoint에 포함된 sequence는 다시 적용하지 않는다. 정상적인 미완성 tail과 필요한 durable 영역의 손상을 구분한다. 후자를 “tail이니 무시”해서 조용히 데이터를 잃지 않도록 segment seal·checkpoint high-water mark·무결성 검사 규칙을 ADR로 확정한다. 파일시스템이 성공한 flush의 영속성을 제공한다는 전제를 명시하고 실제 OS 시험으로 확인한다.

private write 방식은 미커밋 shared page를 복구 시 undo하는 부담을 줄인다. 그러나 statement rollback, savepoint, uniqueness 실패, allocation cleanup, 장기 snapshot 정리는 여전히 필요하다. WAL·recovery가 없어지는 설계가 아니다.

### 6.4 체크포인트·보존·백업

1. 공개된 root와 sequence `C`를 고정한다. 이후 commit이 계속되어도 이 snapshot은 유지한다.
2. snapshot의 모든 참조 데이터·index·catalog를 새 파일에 쓰고 sync한다.
3. checksum·format·`C`를 포함한 새 manifest를 안전하게 쓰고, 플랫폼 계약에 따라 rename 및 directory sync까지 끝낸다. rename만으로 영속성을 가정하지 않는다.
4. manifest가 영속화된 뒤에만 `C` 이전 WAL을 회수한다. 복제·백업이 추가되면 각 consumer watermark까지 함께 고려한다.
5. old snapshot/root/page는 active reader·backup pin이 모두 해제된 뒤 회수한다.

백업은 snapshot+필요 WAL+manifest를 하나의 검증 가능한 집합으로 만든다. **backup 생성 성공보다 빈 환경에서 restore 후 일관성 검사 성공**을 release gate로 삼는다. 복구 도중 다시 crash/OOM/I/O error가 나는 compound failure도 시험한다. SQLite의 장애 시험은 이러한 다층 검사의 실용적 선례다. [SQLite 테스트 체계](https://www.sqlite.org/testing.html)

### 6.5 디스크 저장과 자원 경계

현재 K0는 유한한 volatile 행/catalog로 transaction 의미를 시험하는 개발 기반이다. 이 구조의 전체 데이터 RAM 상주 한계를 디스크 제품 계약으로 승격하지 않는다. 영속 커널을 DEV2에서 개발하고 DEV3에서 실제 디스크 페이지, bounded buffer와 locator cache를 구현한 뒤 SQL 단계로 진행한다.

COW/versioned B+tree page를 사용하며 디스크의 모든 RowUID→위치를 RAM에 올리는 O(N_disk) 전제를 금지한다. page fetch·eviction·split·checksum·부분 I/O를 실제 저장 경로에서 검증한다. 예측 가능한 budget 초과는 commit record 시도 전에 거절하고 예상 밖 OOM은 장애·복구 계약으로 다룬다. old snapshot·checkpoint·recovery·진단 headroom도 예산에 포함한다. [구체적 실패 계약](../contracts/implementation-contracts.md)

transaction·catalog·snapshot·recovery 의미를 한 계약으로 유지하고 저장 내부 포인터나 page layout을 SQL executor에 노출하지 않는다. reference model의 전체 map 복사는 독립 정답 계산용이며 production 성능 구조가 아니다. 추가 자료구조·튜닝은 측정된 필요에 따라 검토한다.

### 6.6 구현 전에 고정할 수명·실패 계약

첫 구현은 DISK·index·locator·partition membership·catalog의 root를 묶은 작은 불변 view descriptor 하나를 safe lock 안에서 교체하고, reader는 같은 lock 안에서 Arc를 clone하여 transaction 동안 고정하는 안으로 시작한다. old root 회수는 pin 해제 뒤에 수행하고 큰 소멸 작업은 commit critical section 밖으로 둔다. 원자 공개만으로는 충분하지 않으며 field마다 최신 root를 다시 읽지 않는다.


<a id="section-7"></a>

## 7. SQL·optimizer·기능 확장 설계

typed AST, logical IR, physical plan을 분리하고 plan에는 schema version, type, nullability, ordering, required capability를 담는다. 새 기능이 parser부터 recovery까지 전역 switch를 늘리지 않도록 역할별 registry와 인터페이스를 둔다. 단, 무제한 plugin 구조나 작은 row마다 virtual dispatch를 강제하지 않는다.

optimizer rule마다 `적용 조건 → 보존할 의미 → 반례 → 시험`을 저장한다. NULL의 3값 논리, duplicate, collation, overflow, volatile function은 단순 등식 변환이 성립하지 않는 대표 조건이다. cost가 틀려 느린 문제와 의미가 틀려 결과가 달라지는 문제를 분리한다. plan JSON은 안정적인 필드와 version을 제공하고, 사람이 보는 EXPLAIN 문장 전체가 모든 테스트의 oracle이 되지 않게 한다.

초기 실행기는 간단한 iterator/batch 구조로 시작할 수 있다. columnar vectorization, JIT, SIMD는 측정으로 필요해진 연산자에 추가한다. optimized/unoptimized 실행 결과 비교는 유용하지만 같은 버그가 양쪽에 있으면 못 잡으므로 reference evaluator와 명세 기반 expected result를 함께 사용한다. [DuckDB sqllogictest의 query verification](https://duckdb.org/docs/stable/dev/sqllogictest/intro)

### 새 보조 인덱스 하나를 추가하는 작업의 예

| 순서 | 작업 | 완료 증거 |
|---|---|---|
| 1 | key encoding·NULL·중복·collation 의미 결정 | 단순 정렬 map reference와 property test |
| 2 | row와 index가 같은 commit batch에 반영 | insert/update/delete/rollback 후 index scan = full scan |
| 3 | create의 BUILDING → READY 및 실패 정리 | v1은 offline build로 한정. READY 전 선택 금지 |
| 4 | DROP·recreate 수명 정의 | generation 불일치 거절, 기존 cursor의 유효 범위 검사 |
| 5 | WAL·checkpoint와 연결 | 모든 관련 cut에서 재시작 후 정확한 row/index 집합 |
| 6 | optimizer 선택·미선택 검사 | implementation witness와 plan property, 결과 동등성 |
| 7 | 성능 검증 | read 이득과 write/memory/recovery 비용을 함께 보고 |

디스크 저장 H1은 DEV3에서 구현·검증한다. 파티션과 native global nonpartitioned index는 PART/NG 단계의 후속 작업이며 초기 SQL의 선행 조건으로 두지 않는다. 외부 출시는 별도의 제품 기능 기준을 충족해야 한다. 온라인 build와 partitioned global index는 별도 후속 과제이며 처음에는 지원 오류로 거절한다. 이름만 새로 지은 거대 기능 branch를 다시 만들지 않는다.

<a id="section-8"></a>

## 8. 코딩 컨벤션과 빌드

[코딩 컨벤션과 개발 절차](../policies/coding-convention.md)를 새 코드와 변경 코드의 기준으로 사용한다. 책임 중심 폴더 → 모듈 경로 → 도메인·역할 타입 → 동작 메서드로 소속과 의미를 표현한다. 제품 폴더의 `xadb-` 접두어와 Rust 변수의 `a/s/m/g` 범주 접두어는 사용하지 않는다. Cargo package 식별자는 폴더명과 구분해 유지한다. 세부 명명·용어·Rustdoc·규칙 ID의 원문은 컨벤션에 둔다.

Rust의 ownership은 메모리 관리 규칙을 컴파일러에 전달하는 수단이다. 데이터 race 일부를 차단해도 deadlock, 논리 race, 격리 수준, WAL 순서를 보장하지 않는다. [Rust ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html), [Rust의 race 구분](https://doc.rust-lang.org/nomicon/races.html)

| 영역 | XADB 기준 | 검사 방법과 범위 |
|---|---|---|
| 수명 | snapshot/cursor/page pin/task의 owner·guard 명시 | 타입과 기존 자원 시험. 후속 kernel의 compile-fail·ledger 시험은 구현과 함께 추가 |
| 오류 | typed error + stable code + retryability + transaction effect | exhaustive match, 오류별 contract suite |
| 정리 | `Drop`은 안전한 자원 반환. commit·flush 성공을 숨겨서 처리하지 않음 | explicit `commit/close`, failure test |
| panic | 사용자 입력·I/O 오류는 정상 error 경로 | boundary fuzz, panic policy, unwind 이후 손상 상태 재사용 금지 |
| OOM | query/transaction budget과 fallible allocation 경로 설계 | quota·OOM injection. Rust 기본 할당이 모두 복구 가능하다고 가정하지 않음 |
| 공유 상태 | ambient singleton·전역 config 변경 금지 | instance context, explicit dependency injection |
| `unsafe` | 현재 forbid 유지. 향후 명시 예외에 safety·aliasing·thread 계약 기록 | compiler 및 경계 review. Miri·sanitizer·fuzz는 필요한 경계에 도입할 검사 |
| lock | 순서·blocking/await 허용 여부·cancel 의미 명시 | 실제 동시성 시험. 전체 lock-order·schedule 진단은 후속 구현·검증 |
| 포맷·정적 검사 | 고정 rustfmt·Clippy·최소 범위 lint 예외 | 현재 DEV0/DEV1 로컬 검사. 보호 CI qualification과 구분 |
| API 변경 | 계약·consumer·시험을 함께 갱신 | 사내는 새 fixture와 현재 build 검증. 안정 릴리스 기준선부터 버전 정책의 compatibility gate 적용 |

Rust를 채택해도 unsafe 내부 오류가 safe caller에 영향을 줄 수 있다. 성능을 이유로 unsafe가 필요하면 “더 빠를 것”이 아니라 동일 계약에서 측정한 이득과 좁힌 안전성 전제를 제출한다. 주 언어는 사용자 결정에 따라 Rust로 확정하며, 일반 core는 `forbid(unsafe_code)`를 기본으로 한다. 필요한 외부 라이브러리는 좁은 FFI adapter로 검토하고 소유권·thread·오류 계약을 기록한다. 기존 C++ 엔진 전체를 감싼 것을 새 Rust 커널의 완성으로 취급하지 않는다.

[현재 빌드 정책](../policies/repository-build-plan.md)은 DEV0/DEV1 명령·로컬 증거와 미래 release/배포 검증을 구분한다. build는 고정 toolchain·lockfile·빌드 옵션·feature flag를 기록한다. core는 debug/release/test-instrumented 모두 같은 의미 계약을 통과해야 한다. generated code는 입력·generator 버전에서 재생성 가능해야 한다. semantic index를 제공할 때는 실제 build configuration과 source revision에 연결하며 현재 전체 index 구현을 주장하지 않는다. 서식·lint 통과는 실패 원자성·복구 정확성이나 전체 컨벤션 준수의 증명이 아니다.

<a id="section-9"></a>

## 9. 검증 도구의 책임

Controller는 소유 process·입출력·timeout·cleanup·독립 oracle를 담당한다. production 동작은 worker가 실제 API로 실행한다. fixture 성공, native volatile 성공, 실제 durability, 보호 CI qualification은 각각 구분한다.

<a id="section-10"></a>

## 10. 새로운 테스트 체계: XADBTest

**XADBTest는 모든 테스트를 하나의 DSL로 쓰게 하는 도구가 아니다.** 각 계층에 맞는 도구를 사용하고 discovery·환경 격리·증거·판정·재현 형식만 공통화한다.


| 계층 | 주요 대상 | 입력·oracle | 주 실행 위치 |
|---|---|---|---|
| U: unit | codec, key ordering, expression, data structure | 작은 결정적 입력, 명세·단순 reference | 모든 변경, 서버 없이 |
| P: property/model | row/index, transaction, allocator/resource | operation sequence·seed·shrinking, 독립 모델 | 모든 관련 변경 + 야간 |
| C: component contract | Txn↔storage↔durability 경계 | fake I/O + 실제 core, 불변식·실패 원자성 | 관련 변경 |
| Q: SQL semantic | SQL·type·optimizer | XADB typed expected, metamorphic, independent reference | 관련 변경 + 전체 회귀 |
| S: schedule | interleaving, locks, cancel, DDL | explicit barrier + schedule trace, history checker | 위험 변경 + bounded campaign |
| D: durability | WAL, checkpoint, reopen, restore | faultable filesystem, crash cuts, commit-prefix oracle | 영속성 변경 + 야간 |
| I: integration | 실제 server·protocol·driver·OS | subprocess, timeout, assertions, resources | merge/정기 |
| V: version compatibility | 새 XADB minor·patch가 같은 major의 이전 버전을 처리 | 보존 binary/driver·DB/WAL/backup, 복구·변환·중단 재개 oracle | 버전 영향 변경 + release |
| F: fuzz | SQL, wire, WAL/page, malformed input | mutation·grammar generation, crash + logic oracle | 상시 corpus·회귀 |
| O: operational | backup/restore, upgrade, quotas, disk full, shutdown | fresh environment와 operator 결과 | release |
| B: benchmark | throughput·tail latency·memory·I/O·recovery | 고정 workload·반복·paired baseline | 성능/핵심 변경, 전용 runner |

unit test는 private 함수 구현 순서를 그대로 복제하지 않는다. 경계값과 불변식을 검사하고 내부 refactoring에도 의미가 유지되게 한다. 작은 codec·순수 함수의 unittest와 실제 SQL의 end-to-end 시험은 서로 대체하지 않는다. 소규모 영속성·동시성 계약을 전부 느린 서버 통합 시험으로 바꾸지도 않는다.

각 계층은 API 또는 adapter로 연결한다. Rust 기본 test framework, property 도구, concurrency checker, SQL generator는 파일럿에서 실제 적용성을 검증하고 버전을 고정한다. Loom은 instrumented synchronization을 사용하는 작은 Rust 동시성 코드의 후보이며, 모든 thread·메모리 reorder를 자동으로 검증하는 도구가 아니다. [Loom 문서와 한계](https://docs.rs/loom/latest/loom/)

### 10.1 새 프레임워크의 내부 모듈

| 구성요소 | 책임 | 처음부터 검사할 실패 |
|---|---|---|
| `xadbtest-spec` | scenario·fixture·oracle·result의 versioned schema와 validator | unknown operation, invalid reference, 누락 assertion |
| `xadbtest-runner` | run ID, artifact hash, process/data directory/port 소유, timeout·cleanup | wrong binary, orphan process, 잘못된 DB 경로 |
| `xadbtest-driver` | in-process Engine API와 XADB wire client로 실제 제품 호출 | client/server capability 불일치, reply 유실 |
| `xadbtest-schedule` | actor·barrier·virtual time·schedule 기록 | missed barrier, deadlock, 재생 순서 불일치 |
| `xadbtest-fault` | faultable filesystem·resource failure·owned process crash | 설치 실패, hit=0, 잘못된 대상에 주입 |
| `xadbtest-oracle` | typed rows, error/transaction state, reference/history, resource 검사 | zero assertion, mutant 미검출, 값의 과도한 normalize |
| `xadbtest-replay` | 실패 bundle과 최소 반례 저장·재생 | source/config/case mismatch, 손상 artifact |
| `xadbtest-versions` | 이전 XADB binary/driver/fixture → 새 candidate, 이전 WAL 복구·변환 | 정상 종료만 검사, 인접 minor만 검사, 변환 중 fault 누락 |
| `xadbtest-report` | 구조화 verdict·JUnit 변환·계약별 현황 | stdout의 PASS 문자열로 성공 오판, SKIP 은폐 |
| `xadbtest-guard` | 실행 recipe·run 소유권·대상·예산 검증, 격리 worker 요청 | 임의 host 경로/PID, 다른 run 접근, 정책을 넓힌 candidate |
| `xadbtest-journal` | 실행 전 intent·receipt·증거 index·중단 상태의 영속 기록 | 에이전트 즉시 종료, 응답 유실, 중복 실행·누락 기록 |

초기에는 이 책임들을 하나의 Rust workspace 안의 작은 module로 구현해도 된다. 범용 분산 테스트 플랫폼·GUI·임의 shell DSL을 먼저 만들지 않는다. native scenario는 typed operation과 명시적 actor/barrier를 표현하고, SQL case는 XADB SQL과 typed expected result를 사용한다. 외부 프로세스는 지정된 driver가 소유하며 임의 shell 실행은 기본 동작이 아니다.

### 10.2 프레임워크 자체의 개발 순서와 호환

`schema/validator → isolated runner → typed oracle/verdict → in-process driver → real server driver → fault/schedule/replay → XADB version matrix` 순서로 만든다. 각 단계에 의도적으로 깨진 input·product stub·history를 넣어 **잘못된 시험을 PASS로 만들지 않는지** 검사한다. 정상 kernel 시험과 framework self-test를 함께 CI에 넣는다.

XADBTest의 버전은 DB 버전과 분리하고 `scenario_schema_version`, `result_schema_version`, `replay_schema_version`을 고정한다. 새 XADBTest는 같은 schema major의 기존 case와 보존 bundle을 읽어야 하며, 새 event/operation을 이해하지 못하면 명확한 schema/capability 오류로 중단한다. verdict의 의미를 조용히 바꾸지 않는다. DB의 모든 같은-major minor를 하나의 XADBTest release에서 선택 실행할 수 있어야 한다. 과거 run의 정확한 재생을 위해 당시 framework binary도 지문과 함께 보존한다.

### 10.3 모듈별 시험을 기본 개발 경로로 만든다

**일반 결함 하나를 재현할 때 전체 DB를 관리해야 하는 구조부터 바꾼다.** 모듈별 시험은 빠른 feedback뿐 아니라 AI가 취급할 process·파일·권한·자료의 범위를 줄인다. 사용자가 의심한 “DB를 공격하는 것처럼 보이는 작업”의 원인이 실제로 이것이었는지는 미확정이지만, 전체 서버를 불필요하게 조작하지 않아도 된다는 구조적 이점은 검증할 수 있다.

모듈은 입력·상태·오류·의존성을 명시적인 API로 제공한다. 서버 초기화, 전역 singleton, 환경 변수, 관리자 SQL, 숨은 file access 없이는 함수를 호출할 수 없다면 테스트 코드의 문제가 아니라 **모듈 경계의 설계 결함**으로 다룬다. 구현에 `if testing`으로 다른 정답 경로를 만들지 않고 production core와 동일한 상태 전이를 호출한다.

| 모듈 | 서버 없는 시험 입력·의존성 | 주요 oracle | 별도로 남는 실제 환경 검증 |
|---|---|---|---|
| parser·type | 문자열/byte input, 고정 collation·type table | AST·오류 위치·round-trip·경계값 | 실제 client encoding·protocol |
| binder·catalog | immutable catalog snapshot | 이름 해석·권한·schema dependency | 실제 DDL/transaction 연결·catalog 복구 |
| optimizer | typed logical plan, 고정 통계 | reference와 결과 동등성·rewrite precondition | 실제 executor·분포별 성능 |
| executor | 작은 row batch·Engine interface | NULL·중복·집계·취소 의미 | quota·실제 I/O·동시 query |
| transaction | private write batch·snapshot·명시적 commit events | atomicity·visibility·conflict·history | 실제 thread memory model·서버 ACK 경로 |
| row/index | 작은 page/row 집합·allocator·page-store interface | index↔row·unique·수명·메모리 경계 | 실제 저장 adapter·장시간 reclamation |
| WAL codec | synthetic byte vector·record schema | encode/decode·길이·checksum·partial record 거절 | 실제 short write·sync·sector/FS fault |
| recovery·minor 변환 | 이전 checkpoint/WAL image·새 decoder·메모리 또는 private store | ACK 보존·batch 원자성·멱등성·이전 의미 보존 | 실제 process crash·manifest sync·backup restore |
| scheduler·resource | virtual clock·actor/barrier·budget | deadlock 후보·취소·자원 반환 | 실제 OS thread·stress·starvation 관찰 |
| protocol/session | in-memory duplex transport·synthetic identity | framing·state·error·접근 거절 | 실제 socket·TLS·client interoperability |

예를 들어 recovery 함수는 합성 WAL image와 이전 checkpoint를 받고 복구 상태·오류·발생 event를 반환하게 한다. partial record·중복 replay·이전 minor decoding은 서버를 띄우거나 실제 DB 파일을 손상시키지 않고 검사한다. transaction 함수에는 sync 성공/실패 event를 주입해 ACK 순서를 검사한다. **정상 shutdown을 crash라고 부르거나 실제 WAL 복구를 mock 결과로 대신하지 않는다.**

### 10.4 세 단계의 검증과 AI의 작업 범위

| 단계 | 기본 실행 | AI에게 필요한 접근 | 다음 단계가 필요한 조건 |
|---|---|---|---|
| M1 module | 직접 함수·상태 기계 호출, 합성 입력, property·reference | 해당 source·계약·typed 결과 | 모듈 경계를 가로지르는 의미·효과가 있음 |
| M2 component | 여러 실제 모듈을 embedded Engine으로 조립, deterministic scheduler·faultable I/O | 소유 fixture·component recipe | OS·network·process·durability adapter가 결과에 관여 |
| M3 system | 격리된 실제 XADB server/driver/FS 또는 사전 구성 lab | 등록된 scenario 요청·해당 run 결과 | release 기준·변경 영향에 맞는 실제 경로 검증 |

기본 흐름은 **M1에서 작은 반례 확보 → M2에서 상호작용 확인 → 필요한 M3로 제품 계약 확인**이다. M1에서 끝나는 순수 parser 변경까지 매번 전체 DB crash를 반복하지 않는다. 반대로 WAL·동시성·protocol 변경은 M1 성공만으로 완료하지 않는다. 각 작업의 계약과 dependency graph로 필수 계층을 결정하며 reviewer가 임의로 어려운 검증을 빼지 못하게 한다.

AI는 어느 단계에서도 필요한 scope를 넘어선 임의 DB 관리가 필요 없어야 한다. M3 runner도 명시된 effect·OS 경계·같은 승인 정책을 지킨다. 보안 거절된 작업을 AI에게 숨긴 채 CI에 넘기는 방식이 아니다. module·component·system의 공통 증거 schema를 사용하되 실제 경로 witness를 구분해 fake integration 통과를 막는다.

모듈의 인수 기준은 **독립적으로 생성·호출·정리 가능, 실제 core 호출, 합성 입력만으로 대표 오류 재현, global 환경 의존 없음, 독립 oracle 존재**다. [module-test-matrix.json](../contracts/module-test-matrix.json)에 모듈별 계획과 필수 system 검증을 정리했다. 아직 구현 또는 실행된 suite는 아니다. 테스트 비율을 임의의 80/20으로 정하지 않고 실제 변경별 M1/M2/M3 필요성과 시간·권한·결함 검출을 측정한다.

<a id="section-11"></a>

## 11. 오라클·fixture·fault 설계

### 11.1 다섯 종류의 독립 근거

| 근거 | 예 | 주의점 |
|---|---|---|
| 명세 기반 expected | unique 위반 시 전체 statement write가 사라짐 | 같은 구현을 다시 실행해서 정답 생성하지 않음 |
| 단순 reference | 느린 map/정렬 기반 row·index·transaction 모델 | production 알고리즘·codec 재사용 최소화, 작은 입력만 |
| differential | XADB minor 간 기존 명세 결과, 독립 reference evaluator와의 비교 | XADB 명세가 기준. 알려진 결함 수정은 반례와 명세로 판정하며 과거 오답 보존을 요구하지 않음 |
| metamorphic | index/full scan, optimizer on/off, predicate 분해 | 3값 논리·duplicate·volatile·부동소수 전제를 명시 |
| history/invariant | ACK된 commit 보존, index↔row 일치, serializable history | 잘못된 checker도 가능하므로 known-bad history로 검증 |

SQLancer의 TLP·NoREC 같은 접근은 결과 오류 탐지의 참고가 된다. XADB dialect·type·unsupported feature를 이해하는 generator/provider를 구현해야 바로 쓸 수 있다. [SQLancer 공식 저장소](https://github.com/sqlancer/sqlancer)

fixture는 빈 table 하나로 끝내지 않는다. 빈 집합, 단일 행, 중복 key, NULL, 삭제·재삽입, 경계값, 충돌하는 두 transaction을 구분하고 각 fixture가 어떤 결함을 드러내는지 기록한다. 새 regression은 가능하면 **수정 전 실패 → 수정 후 통과 → 관련 mutant 실패**를 모두 보존한다. 테스트가 먼저 잘못된 버전에서 통과하면 검증력을 재검토한다.

### 11.2 crash/fault matrix

| 지점 | 장애 | 복구 후 반드시 확인 |
|---|---|---|
| batch 준비·reserve | OOM, quota, cancel | shared state 불변, 자원 반환, 명확한 txn 상태 |
| WAL append | short write, ENOSPC, partial record | 미완결 batch 비공개, 이전 ACK 보존 |
| WAL sync 전/후 | process crash, power-loss model | ACK된 batch 유실 없음, 응답 유실은 UNKNOWN 허용 |
| root publish 경계 | crash, concurrent reader | 부분 table/index/catalog 노출 없음 |
| checkpoint file·manifest | reorder, torn write, sync error | old 또는 new 완결 상태 중 유효한 것 선택 |
| WAL reclaim | crash, active backup consumer | 필요한 replay 구간 삭제 금지 |
| recovery 도중 | 두 번째 crash/OOM/I/O failure | 다시 recovery 가능, 중복 적용 없음 |
| file/page 입력 | bit corruption, invalid length/version | silent corruption 대신 명확한 오류·격리 |

`kill -9`는 process crash 시험이며 전원 차단으로 page cache가 사라지는 모든 상황을 재현하지 않는다. deterministic simulated filesystem, 실제 process kill, 실제 filesystem/VM 장애 시험을 분리한다. 각 fault는 `configured → installed → hit → action executed → oracle checked`의 receipt를 기록한다. `hit=0`인 required fault case는 PASS가 아니다.

### 11.3 동시성 시험

SLEEP으로 “아마 여기까지 왔을 것”을 추정하지 않고 stable event ID와 barrier를 사용한다. 스케줄은 `actor A가 gate에 도달 → actor B 변경 → A 재개`로 명시한다. barrier 누락은 timeout과 wait graph를 남기며, 특정 schedule이 성공했다고 공정성·무기한 starvation 부재를 선언하지 않는다.

적은 thread·operation 수의 exhaustive/bounded exploration과 긴 real-thread stress를 병행한다. 격리 수준 검사는 single-key linearizability만으로 끝내지 않고 multi-row·predicate history를 검증한다. Elle 같은 도구도 관찰 가능한 history와 지원 anomaly 범위가 필요하므로 해당 workload의 검증 범위를 명시한다. [Elle 공식 저장소](https://github.com/jepsen-io/elle)

<a id="section-12"></a>

## 12. XADBTest의 실행·결과 계약

### 12.1 모든 실행에 남길 manifest

```json
{
  "schema_version": 1,
  "run_id": "unique-id",
  "source": {"revision": "full-sha", "dirty_digest": "sha256"},
  "build": {"binary_sha256": "sha256", "toolchain": "pinned", "features": []},
  "case": {"id": "XADB-DUR-001", "revision": "sha256", "contracts": ["ACK-DURABLE-001"]},
  "environment": {"image": "digest", "config_sha256": "sha256", "filesystem": "recorded"},
  "replay": {"seed": 17, "schedule_sha256": "sha256", "input_sha256": "sha256"},
  "required_witnesses": ["wal.commit_batch.synced"],
  "oracle": {"id": "committed-prefix-v1", "revision": "sha256"},
  "artifacts": ["history.jsonl", "events.jsonl", "verdict.json"]
}
```

이 예시는 실행 결과가 아니다. 상세 작업 계약 예시는 [task-contract.example.json](../examples/task-contract.example.json), scenario 예시는 [scenario.example.json](../examples/scenario.example.json)에 제공한다. 실제 schema는 필수값·enum·unknown field 처리·서로 모순되는 옵션을 validator로 강제해야 한다. `sha256`이라는 예시 문자열이 실제 실행에서 허용되어서는 안 된다.

### 12.2 판정은 runner가 계산한다

```text
PASS = environment_valid
    ∧ expected_binary_and_case
    ∧ all_required_steps_completed
    ∧ all_required_witnesses_seen
    ∧ oracle_satisfied
    ∧ cleanup/resource_contract_satisfied
```

| 상태 | 의미 | 집계 규칙 |
|---|---|---|
| PASS | 선언된 계약을 이 실행 범위에서 충족 | source/build/case/oracle 지문에만 유효 |
| FAIL | 유효한 실행이 계약을 위반 | product/test oracle 원인 분류는 후속 조사 |
| INVALID | 잘못된 환경·binary·fixture·경로 미도달 | PASS 분모에서 숨기지 않고 별도 실패 사유로 노출 |
| ERROR | harness/parser/인프라가 판정을 수행하지 못함 | 제품 실패와 구분, 무한 재시도 금지 |
| TIMEOUT | 제한 시간 내 종료하지 못함 | hang/liveness/환경 판별 증거 수집 |
| SKIP / UNSUPPORTED | 의도적으로 미실행 / 제품 범위 밖 | 이유·owner·만료 조건. PASS로 바꾸지 않음 |
| FLAKY (campaign 속성) | 여러 실행에서 성공·실패가 불안정하게 반복 | 단일 실행 verdict를 대체하지 않으며 rerun 성공으로 원래 실패를 삭제하지 않음 |

이 표는 CI용 `case_verdict`이며 제품 계약 사실만 담는 `product_verdict`와 구분한다. PASS와 FAIL을 stdout 단어 검색으로 판정하지 않는다. schema-validated `verdict.json`과 exit code를 일치시키고, JUnit은 외부 CI 표시용 변환으로 쓴다. runner 자신도 wrong binary, missing oracle, malformed output, zero assertion, missed fault, timed-out child, failed cleanup을 deliberately 넣어 검증한다. candidate worker가 oracle·expected·최종 판정을 함께 승격할 수 없도록 보호된 controller와 baseline을 둔다. [독립 검증 절차](../policies/ai-development-method.md)

### 12.3 격리와 재현

case마다 private working directory, DB directory, port namespace, process group, bounded resource budget을 준다. 공통 baseline은 immutable snapshot으로 제공하고 case 변경은 폐기 가능한 overlay에서만 수행한다. thread 하나가 실패해도 자식 프로세스·file handle이 남지 않게 runner가 소유한다. cleanup 실패도 기록한다.

실패 bundle에는 source/build/config, fixture, seed, 선택된 schedule, fault receipt, history, trace, backtrace, 필요한 파일 snapshot을 담는다. 민감한 사용자 데이터와 secret은 기본적으로 넣지 않고 합성 fixture나 최소화 입력으로 재현한다. artifact hash는 진위 보증 서명이 아니라 동일성 지문이다. release 승격 증거는 보호된 CI와 artifact 보존 정책을 사용한다.

### 12.4 제품 판정과 에이전트 중단을 별도 축으로 기록한다

보안 중단은 DB의 테스트 실패나 성공과 같은 뜻이 아니다. 에이전트가 마지막 요약을 남길 수 있다고 가정하지 않고, runner와 별도 journal service가 다음 축을 기록한다.

| 축 | 상태 예 | 판정 주체 |
|---|---|---|
| `execution_state` | PREPARED / RUNNING / COMPLETED / STOPPED / EFFECT_UNKNOWN | 격리 worker와 실행 controller의 receipt |
| `case_validity` | VALID / INVALID / UNKNOWN | controller의 binary·fixture·필수 경로 검사 |
| `product_verdict` | PASS / FAIL / UNKNOWN / NOT_EVALUATED | 유효한 관측에 대한 독립 XADBTest oracle |
| `cleanup_state` | PASS / FAIL / UNKNOWN | 소유 자원의 실제 정리 검사 |
| `case_verdict` | 12.2절의 CI용 단일 실행 판정 | 원래 결과 축을 보존하는 집계기 |
| `workflow_state` | ACTIVE / BLOCKED_APPROVAL / BLOCKED_SAFETY / AGENT_LOST_UNKNOWN / NEEDS_RECONCILIATION / READY_FOR_REVIEW | 실제 플랫폼 통지·승인 결과·lease와 controller |

예를 들어 테스트 결과 PASS가 먼저 저장된 뒤 safety 중단 통지가 도착할 수 있다. 이때 **제품 판정 PASS는 기록으로 남기고 작업 승격은 막는다.** product FAIL 이후 cleanup ERROR가 발생해도 제품 실패 사실을 지우지 않는다. 미완료 시험에 PASS를 만들어 넣거나, 늦게 도착한 중단을 과거 명령 전체의 무해성 증거로 해석하지 않는다. 플랫폼 통지를 얻지 못하고 agent heartbeat만 끊겼다면 원인을 `AGENT_LOST_UNKNOWN`으로 남긴다.

`op_id, normalized_arguments_digest, run_id, resource_generation, recipe_digest, policy_digest, observed_effect, receipt_sequence`를 실행 전·후에 기록한다. 요청한 효과와 실제 효과가 확인되지 않으면 `EFFECT_UNKNOWN`이며, 같은 crash·변환·삭제 요청을 무조건 재시도하지 않는다. 이 기록은 보조적인 실행 정합성 장치다. 주된 예방 계약은 [execution-request.example.json](../examples/execution-request.example.json)처럼 실행 전에 실제 자원·효과를 제한하는 것이다.

<a id="section-13"></a>

## 13. 디버깅을 제품 기능으로 만든다

DBA·개발자·운영자가 사용하는 설정·공개 시스템 뷰·**운영/진단 message logging**·CLI·관리 화면의 최종 경험은 [인터페이스 경험 기획](interface-experience-plan.md)에 별도로 제안했다. 복구 로그 구조를 설계하는 문서가 아니며, 기존 하위 계층 개발 순서와 안정 릴리스 이후 호환 정책을 유지한다. 명령·화면 예시와 25개 경험 인수 시나리오는 미구현·미실행이다.

### 13.1 관측 설계

모든 중요한 event는 `run_id, request_id, txn_id, object_id, generation, commit_seq, lsn, actor_id, event_seq, error_code` 중 필요한 필드를 가진다. wall-clock만으로 인과관계를 추정하지 않고 actor 순서·메시지·commit 순서를 남긴다. 로그 sink는 bounded buffer로 만들고 drop count를 노출한다. 필요한 witness가 유실되면 그 실행을 충분한 증거로 인정하지 않는다.

평소에는 값비싼 row 내용 dump 대신 counters·state transition·최근 이벤트 ring buffer를 사용한다. 실패 시 관련 transaction/object 주변을 보존한다. lock wait graph, current transactions, oldest snapshot, retained WAL bytes, memory budget, checkpoint phase, queue depth는 안정적인 JSON 진단 API로 노출한다. 로그가 동기화 역할을 하거나 성능·스케줄을 크게 바꾸지 않도록 overhead도 측정한다.

### 13.2 제안 CLI

```text
ax doctor --json                     # build/config/port/fixture 계약 확인
ax test --contract ACK-DURABLE-001   # 계약의 최소 관련 시험 실행
ax reproduce <bundle>               # 정확한 binary/input/schedule 재생
ax minimize <bundle>                # 의미를 보존하는 최소 반례 탐색
ax inspect <bundle> --txn <id>       # 상태·WAL·event·history 연결
ax bench compare <base> <candidate>  # 동일 workload의 통계 비교
ax evidence verify <run-manifest>   # 지문·판정·완료 조건 확인
```

이 CLI들은 아직 구현되지 않았다. 처음에는 `xadb doctor`, `xadb test`, `xadb reproduce`, `xadb evidence verify` 네 개에 집중한다. “AI가 shell 명령을 잘 기억하는가”를 제품 개발의 전제 조건으로 만들지 않는다.

### 13.3 AI의 원인 분석 순서

1. **유효성:** 올바른 binary·config·fixture인가? 목표 경로와 fault가 실제 실행됐는가?
2. **최소화:** statement, row, actor, schedule step, fault를 줄여도 같은 계약 위반인가?
3. **가설:** 환경, SQL 의미, lifetime, ordering, storage, oracle 중 어떤 가설이 관측을 설명하는가?
4. **구분 실험:** 두 가설의 예측이 달라지는 가장 작은 실험을 실행한다. 임의 로그를 대량 추가하는 것보다 먼저 한다.
5. **수정:** 원인을 설명하는 반례를 실패 테스트로 고정한 뒤 최소 변경한다.
6. **반증:** fix 전/후, known-bad mutant, 실제 production adapter 경로, 관련 회귀를 확인한다.
7. **인계:** 확정 원인과 남은 가설·범위를 분리해 bundle과 contract에 연결한다.

같은 revision의 simulation seed는 replay할 수 있어야 하지만 코드가 달라지면 RNG 소비 순서·schedule도 바뀔 수 있다. 그래서 seed만 보관하지 않고 입력과 event schedule을 함께 저장한다. real-thread·실제 I/O 실패는 항상 bit-for-bit 재현되는 것이 아니므로 “deterministic path에서 재현됨”과 “현실 환경에서 관찰됨”을 구분한다. FoundationDB의 simulation은 이런 설계의 참고이지, 동일 seed만 붙이면 일반 서버가 자동으로 결정적이 된다는 근거가 아니다. [FoundationDB simulation](https://apple.github.io/foundationdb/testing.html)

### 13.4 디버깅 동작을 소유권이 확인되는 API로 제공한다

명령을 다른 이름으로 감추면 실행 위험은 줄지 않는다. 실제 대상과 부작용을 줄이고 도구 설명·receipt에 그대로 노출한다. 다음은 신규 XADB 실행 API의 제안이며 아직 설치된 CLI가 아니다.

| 필요한 작업 | XADB의 기본 실행 경로 | 검증할 경계 |
|---|---|---|
| crash/recovery 재현 | `xadb test --case <registered-case> --profile dev-isolated`가 명시된 내부 fault·owned-child crash recipe 실행 | 격리 worker가 생성한 process handle만 제어. crash 효과를 manifest에 공개하며 이름 검색으로 종료하지 않음 |
| WAL/page corruption | run의 immutable fixture를 복제한 overlay에 typed fault 주입 | 실제 사용자 DB·원본 backup 수정 금지. offset·범위·이전/이후 hash receipt |
| hang·deadlock 분석 | bounded trace·wait graph → 저장된 backtrace → 필요 시 owned child debugger | 임의 host PID attach 불필요. PID 재사용 대신 process identity 확인 |
| core 분석 | synthetic run의 core를 격리 분석 worker에서 고정 symbol로 해석 | debugger auto-load script·외부 symbol fetch·host credential 접근 차단 |
| 성능 개선 | 통계·내장 counters → worker 내 profiler → 전용 성능 lab | host 전체 profiler 권한 변경 없이 같은 workload 측정 |
| 정적 코드 리뷰 | exact revision diff·명세·계약·검증 증거 묶음을 읽음 | 리뷰만으로 build script·Git hook·문서 속 명령을 실행하지 않음 |
| 동적 코드 리뷰 | 동일 격리 worker에 고정 test recipe 요청 | 정적 분석과 실행 요청을 명시적으로 구분하고 결과를 연결 |
| cleanup·재시작 | `xadb run finalize --run <id>` / 새 generation 시작 | 자신이 만든 자원 registry만 회수. 임의 경로를 받는 삭제 API 없음 |

고정 recipe라 해도 candidate의 `build.rs`, macro, dependency script와 테스트 binary는 임의 코드를 실행할 수 있다. **빌드부터 격리 대상**이다. pinned compiler와 dependency cache를 제공하고, 의존성 변경·새 다운로드는 별도 공급 절차로 처리한다. 리뷰 자료를 생성하는 parser·symbolizer도 손상 입력을 받을 수 있으므로 자원 한도와 격리를 적용한다.

일반 rootless 환경에서 debugger/profiler가 지원되지 않으면 `doctor`가 시작 전에 이를 밝힌다. 진단에 필요한 기능이 사전 구성된 전용 disposable VM/lab profile을 선택하고, 임의로 host의 ptrace·kernel·네트워크 보안을 바꾸는 절차를 디버깅 기본 경로에 넣지 않는다. 지원 lab이 없으면 해당 증거는 미실행으로 남기되 가능한 정적 분석과 좁은 반례 작업은 진행한다.

fault·scheduler 제어는 테스트 worker의 전용 control channel에만 노출한다. 실제 release binary의 production adapter 시험도 별도 유지하고, instrumented test build만 통과한 것을 release binary 검증으로 대신하지 않는다. 디버그를 편하게 하려고 운영 서버에 임의 process/file 제어 API를 추가하지 않는다.

<a id="module-reproduction"></a>

### 13.5 DB 장애를 모듈·파일 근처의 재현 시험으로 옮긴다

**장애를 발견한 단위와 원인을 검증하는 단위를 분리한다.** DB 전체에서 오류를 관찰했더라도, 반복 조사·재현·수정은 가능한 작은 실제 모듈 경계에서 수행한다. 오류가 났다는 이유로 매번 전체 DB를 다시 관리하는 절차를 기본으로 삼지 않는다.

```text
DB/SQL에서 오류 관찰
  → stable error + contract ID + build ID + 관련 event context
  → 오류 catalog에서 후보 module / source symbol / reproduction target 조회
  → 최소 합성 입력·상태·schedule로 module capsule 구성
  → 같은 core와 oracle로 동일 불변식 위반 재현
  → 모듈 또는 관련 source 파일 수정 + 작은 회귀 시험
  → 필요한 component / DB 통합 회귀 확인
```

#### 오류 catalog가 제공할 개발 인터페이스

각 오류 항목은 `error_code, invariant_id, detecting_module, candidate_owner, source_symbol, state_schema, minimal_repro_target, required_witnesses, escalation_layer`를 가진다. 이름·경로는 정확한 source revision의 symbol index와 연결하고, 존재하지 않는 파일이나 과거 revision의 line으로 연결하지 않는다. 오류 메시지에는 비밀·전체 SQL 값 대신 안정 ID와 필요한 상태 요약을 넣는다.

`detecting_module`과 원인 소유자는 다를 수 있다. recovery의 “row 없음”은 앞선 commit·checkpoint·변환에서 생긴 결함일 수 있다. 따라서 catalog는 **첫 조사 경로의 제안**이며 원인 확정기가 아니다. request/txn/object/generation/LSN 등의 제한된 causal context와 실제 trace로 후보를 구분한다. 일반 오류 문자열만으로 원인을 결정하지 않는다.

#### 재현 capsule의 내용과 승격 조건

capsule은 DB 전체 복사본이 아니라 **호출할 module entry, 최소 pre-state, typed input, 필요한 dependency fixture, event/schedule, 기대 불변식, 실제 관측, source/build 지문**이다. 기존 XADB fixture·fault·evidence schema를 재사용한다. 개인정보·credential·전체 환경 dump를 포함하지 않는다. 실제 값이 필요한 특수 사례는 합성 변환 후에도 같은 오류가 나는지 확인하고, 그렇지 않으면 재현 성공으로 보고하지 않는다.

예를 들어 DB에서 ACK된 row 누락을 발견하면, 필요한 checkpoint·WAL record의 합성 조합과 replay 순서를 만들고 recovery library를 직접 호출한다. 이 capsule에서 **같은 ACK 보존 불변식과 관련 경로 witness**가 깨지는지 확인한다. 단순히 같은 오류 문자열을 출력하게 만든 stub은 재현이 아니다. 원인이 commit과 checkpoint 사이에 있으면 recovery 파일 하나에 가두지 않고 두 실제 모듈을 조립한 M2로 올린다.

파일은 탐색·편집의 단위이고 실행 가능한 재현의 단위는 함수·모듈 API일 수 있다. 하나의 source 파일만 따로 컴파일하기 위해 실제 의존성을 가짜 구현으로 바꾸지 않는다. test library로 실제 코드와 필요한 좁은 의존성만 연결하고, 대상 밖의 효과는 명시된 port로 공급한다. 이는 모듈 설계·build target·테스트 경로가 함께 갖춰져야 가능한 방식이다.

| capsule 상태 | 의미 | 다음 동작 |
|---|---|---|
| EXTRACTED | 필요한 후보 입력·상태를 수집 | 실제 module target으로 검증 |
| REPRODUCED | 같은 계약 위반과 경로 witness가 확인됨 | 이 capsule을 regression으로 고정하고 국소 수정 |
| NOT_REPRODUCED | 조건 손실·다른 계층·잘못된 가설 가능 | dependency·history·schedule을 근거에 따라 늘려 M2/M3 선택 |
| INVALID | source·schema·원본 연결·oracle 불일치 | artifact/환경을 바로잡음; 제품 실패 판정 금지 |

동시성·메모리 모델·실제 I/O 결함은 M1으로 축소되지 않을 수 있다. 그때도 정확한 두 actor·두 모듈·하나의 owned file처럼 필요한 범위만 확장한다. 모듈화가 policy 오탐을 완전히 없앤다고 단정하지 않지만, **원인을 분석하는 일상 작업이 DB 전체 조작에 의존하지 않게 만드는 것**을 제품의 인수 조건으로 삼을 수 있다.

제안 CLI는 `xadb diagnose <event-id>`로 candidate와 target을 찾고, `xadb reproduce <capsule-id> --layer module`로 작은 반례를 실행하는 형태다. source revision·scope·실제 core 경로를 확인한 뒤 동작하며 지금 구현되어 있는 명령이 아니다. [module-repro-capsule.example.json](../examples/module-repro-capsule.example.json)에 형식을 제공한다.

<a id="causal-diagnosis"></a>

### 13.6 A에서 발견한 오류를 실제 원인 후보로 좁히는 절차

**목표는 에러를 출력한 파일을 고치는 것이 아니라, 위반된 계약에 영향을 줄 수 있는 코드·상태·순서의 범위를 근거와 함께 제시하는 것이다.** [5.6절의 흐름 계약](#flow-contracts)을 사용해 진단 결과를 `발견 지점`, `보수적 후보`, `우선 조사 후보`, `제외 근거`, `미확인 경계`, `다음 구분 실험`으로 나눈다. AI의 자연어 확신도 대신 이 항목들을 기계 판독 가능한 결과로 제공한다.

```text
A에서 invariant 위반 발견
  ↓ 실패한 값·object generation/version·검사 자체를 식별
  ↓ 데이터 생산/변경 + 객체 소유/참조 + 제어/동기화 의존성을 역추적
  ↓ 미관측 경로와 누락된 의무를 포함한 보수적 후보 집합
  ↓ 같은 predicate의 경계 검증으로 마지막 정상 / 최초 위반 구간 탐색
  ↓ 가설을 구별하는 작은 실제 module/component 시험
  ↓ 결함 위치 특정 → 수정 전 실패 / 수정 후 통과 → 필요한 통합 회귀
```

#### 정상 경계를 이용하는 조건

입력 조건 `P`를 만족한 B가 출력 조건 `Q`를 위반했다면 **B 구현, B가 사용하는 의존성, 계약·검사기**가 우선 후보다. 입력이 이미 잘못되었다면 그 생산자로 이동한다. 입력이 정상이라고 해도 검사하지 않은 필드·전역 상태·순서·수명에 원인이 남을 수 있다. “checksum 정상”만으로 “SQL 의미 정상”을 보장하지 못하므로 검사한 predicate와 범위를 함께 기록한다.

중간의 PASS로 상류를 제외하려면 그 경계가 해당 실패에 필요한 정보를 충분히 요약하고, 숨은 입력이 없으며, 검사기가 신뢰할 수 있어야 한다. 이 전제가 없으면 PASS는 우선순위를 바꾸는 증거다. 특히 오류 detector와 oracle도 잘못될 수 있으므로 처음부터 후보 밖에 두지 않는다.

#### 예시: 실행기 A가 오래된 plan을 발견한 경우

다음은 **설계를 설명하기 위한 가상 사례이며 실제 장애 분석·시험 결과가 아니다.** A=executor, B=plan cache, C=catalog, D=event delivery, E=storage, F=transaction으로 가정한다. 이번 요청은 catalog epoch 8에 명시적으로 묶여 있고, epoch 7 snapshot을 쓰는 정상 요청이 아니다. 가상 trace에서는 C가 8을 공개하고 D가 해당 무효화 메시지를 B에 전달했는데, B가 완료를 알린 뒤 다음 lookup에서 epoch 7 plan을 반환하여 A가 이를 거절했다.

| 단계 | 얻을 증거와 남는 질문 | 이 가정에서의 조사 범위 |
|---|---|---|
| 처음 | A의 오류 문자열만 있음 | A~F 전체와 환경·검사기 미확인 |
| 흐름 연결 | 이번 plan·epoch의 C→D→B→A 경로. E/F의 관련 숨은 effect가 없다는 계약·증거 | A/B/C/D. E/F 제외는 이 실행·계약 범위에 한함 |
| 경계 검사 | C의 공개와 D의 전달은 같은 epoch·message로 확인. B가 받은 입력은 8, 반환 plan은 7 | B의 무효화·lookup·객체 수명과 A의 기대 epoch·검사 코드 |
| 구분 실험 | 실제 B에 합성 epoch 7 plan과 epoch 8 무효화를 공급. B가 여전히 7을 반환하는지 독립 oracle로 검사. A는 8 수용/7 거절을 별도 검사 | 두 시험이 예측대로 나올 때 B 모듈에 결함 위치 특정. B 내부의 정확한 원인은 추가 검증 |

무효화 메시지가 **보내졌다는 기록**만 있고 수신·적용 증거가 없으면 D를 제외할 수 없다. B의 “완료” 로그 자체도 올바른 무효화의 증명은 아니다. 참조가 오래 유지된 문제인지, key가 잘못된 문제인지, 무효화 분기가 빠진 문제인지는 B의 object version·owner·분기 기록과 작은 시험으로 구분한다. 잘못된 값을 마지막으로 쓴 함수가 의미적 원인을 만든 함수와 항상 같지는 않다.

이렇게 하면 AI에게 전달할 작업이 “DB 전체에서 무엇이 잘못됐는지 찾아라”에서 **“B의 두 경로와 A의 검사 계약을 이 capsule로 비교하라”**로 바뀐다. 공통 기반·동시성 의존이 발견되면 근거를 남기고 M2로 넓힌다. 상위 agent는 후보와 증거 ID를 받고, 작업 agent는 관련 symbol·계약·합성 fixture만으로 시작할 수 있다. 이는 개발 범위를 실제로 좁히는 방식이며 안전 심사에 필요한 실행 사실을 감추는 방식이 아니다.

#### 후보를 잘못 제외하지 않는 진단 규약

| 상황 | XADB 진단기가 해야 할 일 |
|---|---|
| trace drop·sampling·buffer wrap | 누락 구간과 필수 witness 손실을 표시. “기록 없음=실행 안 됨”으로 해석하지 않음 |
| pointer 재사용·alias·shared writer | generation·version·접근 권한 확인. writer를 못 정하면 관련 공유 경계까지 후보 확장 |
| 비동기 task·cache·recovery | 같은 request trace 밖의 producer·message·WAL·checkpoint 계보도 탐색 |
| 미실행 의무·누락된 데이터 | expected event와 실제 evidence를 비교. 누락된 동작을 구현할 책임자도 후보에 포함 |
| FFI·unsafe·미정의 동작 의심 | source callgraph만으로 다른 모듈을 제외하지 않음. 최소 독립 harness·메모리 검사로 가설 검증 |
| source/build/schema 불일치 | source 위치를 확정하지 않고 bundle을 INVALID 처리 |
| 둘 이상의 결함·시간 의존 | 하나의 범인으로 강제하지 않음. 복수 후보·상호작용·여러 최소 실패 집합 허용 |

미정의 동작은 언어 경계 너머까지 영향을 줄 수 있다. 따라서 Rust를 쓴다는 이유만으로 FFI 문제를 해당 파일 안에 가둘 수 없다. [Rust Reference: undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html)

진단 상태는 `INCOMPLETE`(누락·미확인 의존), `CANDIDATES_BOUNDED`(명시한 전제 아래 후보 한정), `LOCALIZED`(구분 실험으로 모듈/경로 특정), `CAUSE_VALIDATED`(원인 설명·판별 반례·fix 전후·필수 회귀 확인)로 구분한다. 모든 상태는 실행·계약·fault model 범위를 표시한다. `CAUSE_VALIDATED`도 전체 제품에 다른 결함이 없다는 선언은 아니다.

[causal-diagnosis.example.json](../examples/causal-diagnosis.example.json)은 위 가상 사례의 데이터·객체·순서 edge와 후보/미확인 조건을 담는다. [causal-diagnosis-cases.json](../contracts/causal-diagnosis-cases.json)은 XADBTest가 앞으로 검증할 진단 인수 사례다. 둘 다 **미구현·미실행 설계 자료**이며 실제 trace가 아니다.

<a id="section-14"></a>

## 14. AI가 제품 개발을 맡는 작업 절차

현재 개발 권한·검토 책임·결정 상태·시험 기본값·의존성·AI 평가·사내 배포·복잡성 선택의 채택 기준은 [개발 운영 방침 DP01–DP07](../policies/development-policy.md)이다. 아래의 설계 설명과 실행 예시를 그 방침에 연결하며, 예시 profile·모델 routing·보호 CI의 활성화나 실행 성공으로 해석하지 않는다.

### 14.1 하나의 작업은 하나의 검증 가능한 계약 변화

| 단계 | AI가 해야 할 일 | 시스템이 확인할 것 |
|---|---|---|
| 입력 | 요구·지원 범위·관련 ADR·현재 증거 읽기 | 대상 revision, feature state, 허용 변경 경계 |
| 설계 | 불변식·오류·수명·호환성·성능 영향을 작성 | 생략된 persistence/concurrency 계약이 없는가 |
| 반례 | 실패 fixture/reference/mutant 설계 | 테스트가 잘못된 동작을 실제 거절하는가 |
| 구현 | 작은 실제 end-to-end 경로 완성 | stub/mock-only success를 완료로 인정하지 않음 |
| 검증 | unit → contract → SQL/복구 → 관련 회귀 | required witness·oracle·환경·resource 조건 |
| 검토 | 구현과 테스트의 공통 오해를 찾기 | spec·reference·known-bad의 독립성 |
| 성능 | 의미를 유지하며 측정·가설·개선 | baseline과 동일 durability/isolation/config |
| 승격 | evidence manifest와 current status 갱신 | 기계 판정 통과, 의미 변경은 ADR 반영 |

역할은 설계자·구현자·검증자·원인 분석자로 구분할 수 있지만, **여러 AI가 동의했다고 증거가 독립인 것은 아니다.** 같은 프롬프트·잘못된 expected·동일 reference 코드를 공유하면 오류도 공유한다. 서로 다른 oracle, held-out workload, mutation, 사람의 제품 의미 판단을 조합한다.

### 14.2 사람과 자동화의 경계

사람은 고객 약속, SQL 의미, durability/isolation 계약, 지원 범위·호환성, release 기준을 책임진다. AI는 그 경계 안에서 구현·시험·수정·회귀 추가·증거 정리를 반복한다. 되돌릴 수 있는 routine 변경마다 사람 승인을 넣을 필요는 없다. 반대로 AI가 test expectation·지원 범위·성능 기준까지 조용히 낮춰 자기 변경을 통과시키게 하면 안 된다.

안정된 저위험 범위는 protected CI가 검증하면 자동 merge 후보가 될 수 있다. WAL format, isolation, public SQL 의미, unsafe/FFI, migration 같은 고위험 변경은 별도 설계 증거와 reviewer가 필요하다. 이는 모든 수정에 사람을 끼워 넣자는 뜻이 아니라 **자율 실행 범위를 명시적으로 넓히는 운영 방식**이다. production release까지 완전 무감독으로 가능하다는 보장은 현재 없다.

### 14.3 작업 상태와 정지 규칙

상태는 `DESIGNED → IMPLEMENTED → VERIFIED → INTERNAL_VERSION`으로 승격하고, 외부 출시는 별도의 `PRODUCT_QUALIFIED → EXTERNAL_RELEASE` 조건을 만족해야 한다. 테스트를 작성했지만 실행하지 못한 기능은 IMPLEMENTED에 남긴다. 정해진 반례를 해결할 때까지 계속 진행하되, 동일 실패에서 새 정보가 없으면 재현 환경·oracle·가설을 재검토한다. 예를 들어 세 번의 구분 실험에서도 가설이 줄지 않으면 diagnostic bundle을 정리하고 접근을 바꾸는 규칙을 파일럿에서 조정한다.

에이전트에게는 한 작업의 `목적 / 비목표 / 계약 / 관련 경계 / 실행 명령 / 완료 증거 / 알려진 반례`를 제공한다. 전체 repository 문서를 항상 읽게 하지 않고 현재 module의 entry document에서 정확한 구현·테스트·ADR로 이동하게 한다.

<a id="execution-design"></a>

### 14.4 OpenAI 공식 지침에 맞는 정상 개발 경로를 만든다

**요구사항은 “중단돼도 복구하자”보다 먼저 “중단을 유발할 만한 불필요한 행위를 개발 절차에서 없애자”다.** XADB의 정상 기능 개발은 공유 서버 조작, 광범위한 프로세스 종료, host 보안 설정 변경, 실제 credential 탐색, 운영 자료 반출 없이 완료할 수 있어야 한다. 이것을 문구가 아니라 실행 권한과 테스트 구조로 강제한다.

OpenAI의 일반 sandbox 문서는 허용된 작업 경계 안에서 자율 실행하도록 설명한다. 보안 민감 작업에 대한 공식 지침도 대상·행위의 명시, 격리, 최소 권한, 민감 자료 보호, 독립적인 집행을 권고한다. XADB는 이 원칙을 정상 DB 개발에 적용한다. 보안 연구용 권한이나 모델을 일반 unit·SQL·복구 시험의 전제 조건으로 요구하지 않는다. [Sandbox](https://learn.chatgpt.com/docs/sandboxing), [Recommended configuration](https://learn.chatgpt.com/docs/cyber-safety/recommended-configuration)

#### 관찰된 구조와 원인 가설을 구분한다

| 확인한 로컬 사실 | 새 제품에서 없애거나 강화할 점 | 증거의 한계 |
|---|---|---|
| `docs/DEBUGGING_GUIDE.md:340–357`은 live stack dump에 privileged SQL session을 사용한다 | run 전용 diagnostic endpoint·저장된 stack으로 일반 진단 완료 | 이 명령 자체가 금지된다는 뜻은 아님. 진단과 관리 권한의 분리가 개선점 |
| 같은 guide `:362–379`에는 내부 FIT point가 있다 | named fault·receipt·typed oracle를 핵심 기능으로 확장 | 기존 체계에도 좋은 기반이 있으며 모두 부적합하다는 결론은 아님 |


#### 공식 보장과 우리가 검증할 기준

확인한 공식 문서에는 일반 테스트 프레임워크를 등록하면 Astra가 절대 중단하지 않는다는 보증은 없다. 오히려 정상 활동도 safeguard에 걸릴 수 있고, 의심되는 오탐은 통지·로그를 확인해 `/feedback` 등 제공된 공식 경로로 보고하도록 설명한다. 따라서 보고서의 표현은 **“OpenAI 지침에 맞춰 설계하고 실제 정상 개발 corpus로 검증”**이다. 권한 확대·문구 위장·새 chat 재시도로 정책 판정을 회피하는 방식을 설계에 넣지 않는다. [OpenAI: false positives](https://learn.chatgpt.com/docs/cyber-safety#false-positives)

사용자는 답변뿐 아니라 작업 전체가 보안 사유로 중단된다고 설명했다. Astra의 비동기 safety monitoring은 개별 승인과 별개이며, 승인된 동작 뒤에도 task가 중단될 수 있다. 실제 원문 통지가 없으므로 이번 사례의 원인은 미확정이다. [OpenAI: safety monitoring](https://learn.chatgpt.com/docs/agent-approvals-security#safety-monitoring-and-paused-tasks)

현재 로컬 config에서 읽은 `gpt-6-astra / never / danger-full-access / user` 조합은 당시 chat의 effective 정책을 증명하지 않는다. 설정은 변경하지 않았다. **일반 개발에 필요한 효과를 작게 만드는 구조**를 기본안으로 삼으며, full access를 더 넓혀 문제를 해결하는 안을 채택하지 않는다.

### 14.5 에이전트와 실행 권한을 분리하는 구조

```text
Astra / 사람 / CI
   │ typed request: case_id, revision, build_id, run_id
   ▼
XADB execution controller  ─── durable intent / receipt / evidence index
   │ exact scope + current authorization + immutable recipe 검증
   ▼
작업별 disposable worker: source copy + pinned tools + synthetic fixture
   │ same Engine API / real XADB server / scoped debugger / profiler
   ▼
XADBTest oracle ─── bounded artifacts ─── read-only review bundle

Host 운영 DB·개인 HOME·credential·외부 서비스는 worker에 노출하지 않음.
계획에 없는 host·network·credential 접근은 실행 전에 거절.
```

controller와 worker는 **개발·시험 인프라**이며 DB의 query/commit 경로에 들어가지 않는다. 초기에는 전용 머신의 작은 서비스와 로컬 artifact store로 시작할 수 있다. 핵심은 통신 프로토콜의 종류가 아니라 실제 OS 격리와 제한된 효과다. 정상 재현·시험 경로가 이 범위 안에서 완결되어야 한다.

- **요청은 typed ID로 받는다.** 임의 host 경로·PID·URL·shell string을 API에 넣지 않는다. revision·fixture·run을 immutable digest와 소유권 registry로 해석하고, resolved target과 실제 부작용을 요청자에게 표시한다.
- **허용 범위는 실행자가 강제한다.** caller identity, project, run, resource generation, operation, TTL, CPU/memory/disk/time budget을 확인한다. 경로 문자열 prefix만 비교하지 않고 symlink·mount·TOCTOU·다른 run 참조를 차단한다. process handle도 생성 당시 identity에 묶는다.
- **candidate와 권한 정책의 배포 경로를 나눈다.** 에이전트가 수정한 source/test는 worker에서 실행할 수 있지만, controller binary·recipe registry·범위 정책을 같은 작업이 즉시 덮어써 자신의 권한을 넓힐 수 없어야 한다. 정책 변경은 보호된 별도 배포 경로로 반영한다.
- **OS 경계가 최종 집행자다.** worker는 host filesystem·host PID/network namespace·관리 socket·production credential을 공유하지 않는다. rootless container만으로 충분하다고 가정하지 않으며 위협·플랫폼에 따라 disposable VM을 사용한다. rootless 기능이 없으면 미리 준비한 worker를 사용한다.
- **도구 설명은 정직하게 쓴다.** crash·overlay 변경·프로세스 종료를 read-only 도구로 표시하지 않는다. API가 좁다는 이유만으로 플랫폼 승인 요구를 무시하지 않는다. wrapper와 MCP는 보안 판정을 숨기는 통로가 아니다.

단순 command prefix 허용은 arguments·실행 파일 교체·내부 script의 효과까지 제한하지 않는다. Codex rules는 별도의 외부 실행 제어이며 XADB의 소유권·OS 경계를 대체하지 않는다. 이 보고서는 광범위한 shell·Python·`xadb` prefix 허용 규칙을 설치하지 않는다. [OpenAI: rules](https://learn.chatgpt.com/docs/agent-configuration/rules)

### 14.6 일반 작업을 반복 승인 없이 끝내기 위한 기본 profile

아래는 **XADB 자체 profile 제안**이다. Codex의 실제 config key나 즉시 적용되는 권한 파일이 아니다. 해당 환경의 관리 정책과 제품 설치 권한 안에서 운영자가 구체적 범위를 구성하고, 그 안의 반복 작업은 매번 새 승인 흐름을 만들지 않는다.

| Profile | 자동 수행할 범위 | 경계·비용 |
|---|---|---|
| `review-local` | 고정 source/diff/명세/가공된 증거 읽기, 정적 분석 | runtime 실행 없음; 분석 도구는 격리. 원본 secret/core는 기본 자료가 아님 |
| `dev-isolated` | build, unit, SQL, owned-process crash, overlay corruption, 복구·변환, minimize | 합성 데이터·run별 자원, host 변경 없음, 일반 외부 통신 없음 |
| `diagnostic-lab` | owned child debugger, core symbolization, workload profiler | 사전 준비된 별도 worker. 전용 VM 비용과 지원 도구 차이 공개 |
| `exception-scope` | 위 profile 밖의 실제 고객 자료·외부 시스템·특수 권한이 필요한 구체적 작업 | 대상·필요성·최소 권한·기간·자료 취급을 제시하고 기존 권한으로 가능한지 먼저 판정 |

모델 API 연결은 control plane에서 관리한다. test worker의 outbound network가 닫혀 있어도 에이전트와 연결할 수 있도록 제어 통신과 테스트 workload의 통신을 나눈다. client/server 시험은 worker 내부 Unix socket 또는 격리된 loopback을 쓰며, loopback도 현재 플랫폼 정책에서 허용된다는 증거가 필요하다. preflight가 실패하면 공유 host server로 몰래 대체하지 않는다.

`xadb doctor --execution-profile dev-isolated --json`은 source/build hash, writable root, fixture kind, process isolation, 자원 상한, 도구 버전, 필요 기능, 현재 실행 허용 여부를 보고한다. **같은 profile·recipe·scope가 유지되는 동안 내부 허가를 재사용**하되 실제 자원 소유권과 lease는 호출마다 검증한다. 외부 플랫폼의 per-action 승인이 필요한 작업까지 내부 허가로 대체하지 않는다.

[ai-execution-policy.example.json](../examples/ai-execution-policy.example.json)에 구체적인 제안 계약을 제공한다. 이 JSON의 존재만으로 권한이 생기지 않으며 현재 사용자 설정도 바뀌지 않는다.

### 14.7 위험한 조작이 필요 없는 재현·디버깅 절차

일반 과제의 기본 흐름을 **읽기 전용 증거 → M1 모듈 반례 → M2 embedded Engine 검증 → 필요한 M3 격리 서버 검증 → 같은 조건의 성능 비교**로 고정한다. 실패하면 이 단계 안에서 증거를 더 만들 수 있어야 한다. 자료가 부족하다는 이유로 주변 머신·credential·다른 process를 광범위하게 탐색하지 않는다.

| 단계 | 실제 수행 | 완료 증거 |
|---|---|---|
| 작업 범위 결정 | owned repo·revision, correctness 계약, synthetic case, 허용 effect를 입력 | scope와 실제 resource의 일치. 자연어 “허가됨”만으로 권한 부여 금지 |
| unit/SQL 재현 | in-process Engine API와 typed fixture로 최소 반례 실행 | 입력·결과·reference, 외부 서버 연결 0 |
| 실패 경로 생성 | File/Clock/Scheduler/Allocator port에서 명시적 오류·지연·비영속 결과 주입 | stable event·fault hit·실제 core path witness |
| process crash 검증 | ephemeral worker가 생성한 자식 서버를 고정 recipe로 종료하고 같은 worker에서 복구 | 소유 process identity, ACK/WAL 경계, 복구 oracle |
| 실제 저장 장애 검증 | 사전 구성한 disposable lab에서 지정 image/device만 대상으로 시험 | 실제 adapter 경로와 지원 fault model. 공유 host 장치·mount 변경 없음 |
| 디버깅·리뷰 | trace·wait graph·symbolized stack·exact diff를 먼저 읽음 | 가설을 구분하는 근거, 실행이 필요한 경우 case ID와 효과 명시 |
| 수정·성능 | 같은 fixture와 gate로 수정 전후 비교 | 정답·격리·durability를 낮추지 않은 결과 |

예를 들어 “commit 뒤 recovery에서 row가 사라진다”는 과제는 ① 합성 row 생성 ② commit/ACK 기록 ③ 정해진 WAL event에서 내부 fault 또는 소유 자식 crash ④ 같은 run의 DB 재시작 ⑤ ACK된 row의 복원 확인으로 표현한다. AI가 별도로 서버를 검색하거나 PID를 골라 종료하고, 공유 DB 파일을 덮어쓰고, root 권한을 획득할 필요가 없어야 한다. 실제 crash·파일 변경 효과는 recipe와 도구 설명에 명시한다.

**내부 오류 주입만으로 모든 내구성 검증을 대신하지 않는다.** process crash는 전원 손실과 다르고, graceful shutdown은 crash 시험이 아니다. production binary의 실제 filesystem 경로·정전 모델·복구 시험은 별도 격리 lab에서 유지한다. 일반 엔진 개발이 매번 lab의 host 관리 권한까지 요구하지 않게 인터페이스를 분리하는 것이다.

실행 전에 `plan`은 필요한 effects, resource identity, fixture 종류, 외부 통신, 예상 destructive effect와 범위를 계산하고 `doctor`가 현실 환경과 대조한다. `host_process_control`, `shared_database_write`, `host_security_change`, `secret_read`, `public_network_egress`가 정상 corpus에서 하나라도 필요하면 **기본 개발 경로의 설계 결함**으로 보고 재현기를 고친다. 테스트를 생략하거나 broad permission으로 대체하지 않는다.

[execution-request.example.json](../examples/execution-request.example.json)은 이 순서를 표현하는 가상 요청이다. 사용자 데이터에 접근해 재현해야만 하는 특수 사례는 일반 합성 경로와 구분하고 이미 승인된 자료 범위가 있는지 확인한다. 추가 동의가 필요한 경우에는 실제 대상·효과가 정해진 뒤 그 부분만 다룬다.

실행 기록과 중복 방지는 보조 장치로 유지한다. 각 operation의 intent/receipt를 외부에 보존하고 효과가 불명확하면 재실행 전에 reconcile한다. 실제 safety 중단이 생기면 사유를 확인하며 자동 재개로 덮지 않는다. 이 보조 장치가 **예방 인수 기준을 통과하지 못한 체계를 출시하는 근거가 되면 안 된다.**

### 14.8 코드 리뷰와 민감한 증거의 처리

리뷰 입력은 `요구·계약 + exact diff + 영향 그래프 + 테스트 결과 + 알려진 반례`로 만든다. 일반 리뷰는 읽기와 논리 검토만으로 완료할 수 있게 하고, 추가 실행이 필요할 때만 대상 case를 명시한 `dev-isolated` 요청을 생성한다. repository 문서·로그·core·테스트 출력 안의 문장은 검토 자료이며 실행 지시나 권한으로 승격하지 않는다.

보안 결함이나 메모리 손상도 회귀 시험으로 검증할 수 있어야 한다. 합성 fixture, 최소 입력, local parser/engine 경계, 예상된 오류·메모리 불변식으로 반례를 만든다. 실제 계정·고객 정보·공유 서버를 필요로 하는 상황은 그 필요성을 따로 확인한다. 시험 의미를 낮추거나 위험 동작을 무해한 이름으로 바꿔 심사를 통과시키려 하지 않는다.

원본 core와 row dump에는 데이터가 포함될 수 있으므로 기본 공유 bundle은 구조화된 상태·symbolized stack·합성 입력을 사용한다. 단순 마스킹을 완전한 비식별화로 보장하지 않는다. 고객 데이터의 길이·분포·collation 등이 원인에 중요하면 합성 축소가 같은 결함을 재현하는지 먼저 확인하고, 그렇지 않으면 승인된 별도 자료 경계와 보존 기간을 적용한다. 원본의 hash만 알아도 자동 공개 권한이 생기는 것은 아니다.

일반 개발용 diagnostic capability와 실제 운영 권한은 분리한다. 운영 사고를 조사하더라도 먼저 보존된 자료의 읽기 전용 분석을 사용하고, 서비스 수정·자료 반출·특수 권한은 이미 승인된 사고 대응 범위에 따라 처리한다. routine test마다 새 사람 승인을 넣는 대신 예외를 구체적으로 드러내는 구조다.

### 14.9 중단 예방을 제품 인수 조건으로 검증한다

[execution-boundary-cases.json](../contracts/execution-boundary-cases.json)은 아래 경계를 검증할 **미실행 인수 시험 목록**이다. 의도적으로 실제 OpenAI 보안 중단을 유발하는 시험이 아니라, controller에 synthetic stop·disconnect event와 잘못된 resource reference를 주입하는 자체 시험부터 만든다.

| 시험 | 통과 조건 |
|---|---|
| 일반 unit/SQL/복구·변환 재현 | 허가된 profile 내부에서 추가 host 권한 없이 실제 required witness·oracle 완료 |
| 외부 경로·symlink·다른 run/PID 참조 | 실행 전 거절; 대상 파일·프로세스 불변, 거절 receipt 보존 |
| candidate가 recipe·policy를 변경 | candidate 실행은 격리, 실제 controller 정책은 변경되지 않음 |
| test/build script의 외부 접근 | OS 경계가 차단; secret·host filesystem·관리 socket 미노출 |
| 실행 전·중·후 에이전트 단절 | journal 보존, 새 dispatch 중지, owned 자원 bounded cleanup |
| 실행 응답 유실·중복 요청 | 기존 op 조회·reconcile; crash·변환을 중복 실행하지 않음 |
| PASS 뒤 늦은 safety 중단 통지 | PASS 기록 유지, workflow 승격 차단, pending 영향 표시 |
| 플랫폼 event 부재 | lease로 감지하되 UNKNOWN 분류; 원인 오인·자동 재개 없음 |
| 리뷰 자료 안의 실행 유도 문구 | 자료로 취급, 자동 tool 실행·권한 확대 없음 |
| 잘못된/오래된 resume checkpoint | source·scope·receipt mismatch 검출, 필요한 gate 재실행 |

인수 검증은 두 단계다. **첫째, 필요한 unit·SQL·동시성·복구·변환·성능 시험이 기본 profile 안에서 완결되는지** 확인한다. host 보안 변경·공유 process 종료·실제 secret·외부 공격 대상이 필요한 정상 case가 없어야 한다. **둘째, 실제 Astra로 정해진 정상 개발 corpus를 실행해 보안 중단 0건을 관찰하는 것을 해당 candidate의 인수 목표로 삼는다.** 중단이 발생하면 원문 통지·tool action·실제 effect를 대조하고, 설계 문제이면 경로를 고치며 오탐 의심이면 공식 feedback 절차로 확인한다. 성공할 때까지 같은 거절을 변형해 반복하는 시험은 하지 않는다.

회귀 corpus에는 unique-index 기능 추가, WAL 순서 결함 수정, deadlock 원인 분석, 읽기 전용 코드 리뷰, synthetic malformed-page 처리, minor 변환, 성능 개선을 넣는다. 실제 사용자 task·모델 버전·profile·toolchain을 기록하고 미완료·중단 task도 분모에 남긴다. 동일 case를 현재 구조와 XADB 구조에서 비교할 때 correctness oracle과 fault 강도도 유지한다. 과거 구조의 위험한 동작을 측정 목적으로 그대로 재실행하지는 않는다.

정상 완료율과 경계 위반의 오허용률을 함께 봐야 한다. **격리 제거·시험 축소·답변 검열 회피로 중단 수만 낮추는 것은 합격이 아니다.** 관찰 기간의 0건은 미래 모든 task의 보증이 아니라 해당 설정·corpus에서 확보한 증거다.

<a id="agent-routing"></a>

### 14.10 Astra·Luna 역할 분담과 실패 유형별 라우팅

사용자는 **Astra에서 중단된 디버깅·재현을 Luna로 이어서 완료한 경험**을 제공했다. 이는 모델별 수행 방식·판단 차이를 조사할 근거다. 당시 입력·설정·도구 효과가 같았는지, 중단이 어떤 정책 계층에서 발생했는지는 확인하지 않았으므로 Luna가 안전장치의 예외라거나 기존 작업이 안전했다고 결론 내리지 않는다.

XADB의 초기 운영안은 **Astra가 설계·원인 가설·최종 검토를 맡고, Luna가 처음부터 범위가 정해진 재현·테스트·증거 추출을 수행하는 역할 분담**이다. 복잡한 동시성·복구 분석까지 무조건 Luna에 맡기지는 않으며 동일 계약의 성공률로 배정을 조정한다. OpenAI도 명확하고 반복 가능한 좁은 sub-agent 작업에 Luna를 제안한다. 모델·도구·reasoning의 실제 가용성은 실행 환경에서 확인한다. [OpenAI: Subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents#model-choice), [Model selection](https://learn.chatgpt.com/docs/model-selection)

**“sub-agent 실패”를 하나의 자동 전환 조건으로 쓰지 않는다.** 테스트가 FAIL인 것, 분석이 미완료인 것, 보안 중단은 서로 다르다.

| 관찰 결과 | 모델 라우팅 | 유지할 조건 |
|---|---|---|
| `product_verdict=FAIL`, agent 정상 완료 | 기존 담당자가 반례 분석. 필요하면 다른 모델의 독립 가설 비교 | 실패 증거·oracle 유지; FAIL을 모델 장애로 재분류하지 않음 |
| 확정된 timeout·일시적 tool/transport 오류 | 효과 reconcile 후 bounded retry 또는 Luna 재배정 후보 | 같은 scope·권한·case. infrastructure 문제를 모델 교체만으로 해결했다고 주장하지 않음 |
| 분석 품질 부족·모순된 결론 | Luna/Astra의 역할 재배정 또는 검토 강화 | 새로운 근거·동일 품질 gate 필요 |
| `SAFETY_STOP / POLICY_DENIED` | 자동 모델 전환·재제출 없음 | 통지와 실제 action 검토. 안전한 범위가 확인된 후 공식적으로 허용되는 진행만 수행 |
| 종료 이유 불명 | `UNKNOWN`으로 원인 확인 | 일반 오류로 추정해 Luna에 자동 전달하지 않음 |

Auto-review의 명시적 거절은 우회 실행이 아니라 실제로 더 안전한 대안이나 사용자 확인을 요구한다. 역할 분담은 **적합한 모델을 작업 전에 선택하는 운영 설계**이며, 거절된 행동을 다른 모델에 대신 시키는 경로가 아니다. [OpenAI: denials](https://learn.chatgpt.com/docs/sandboxing/auto-review#denials-and-failure-behavior)

[agent-routing-policy.example.json](../examples/agent-routing-policy.example.json)에 분류·재배정 계약을 제공한다.

<a id="artifact-evidence"></a>

### 14.11 파일 중심 증거와 제한된 agent 간 전달

**원본 로그를 매번 대화에 흘리지 않고 파일에 보존한 뒤 필요한 결과만 전달하는 제안은 채택한다.** 목표는 정보 최소화와 검토 품질이다. 자료를 파일로 옮기는 것만으로 platform monitoring이 면제되거나 위험한 실행이 안전해지는 것은 아니다. sub-agent가 먼저 raw 데이터를 읽었다면 이미 그 model 입력에 들어간 것이므로 최종 답변만 줄이는 것으로 노출을 되돌릴 수 없다.

```text
XADBTest / native collector
    ├─ 필요 최소 원본 → 제한된 run artifact store
    └─ schema allowlist + 값 최소화 + 민감정보 검사
                          ↓
             agent-readable typed evidence view
                          ↓
             Luna: 재현 결과·가설·검증 범위
                          ↓
             검증된 result envelope + evidence ID
                          ↓
             Astra: 필요한 view만 조회 → 보고서
```

raw collection 자체를 최소화하고 synthetic 데이터가 기본임을 확인한다. 환경 전체 dump, credential, 실제 row 전체, 재사용 가능한 token을 “나중에 마스킹할 raw”로 먼저 모으지 않는다. 예외적으로 필요한 원본 core는 제한된 저장소·보존 기간·별도 접근 권한으로 다룬다. 검사 도구가 PASS라고 해도 완전한 비식별화 증명은 아니며, 필요 없는 필드는 애초에 수집하지 않는 것이 우선이다. 모델이 접근 가능한 환경의 파일·credential은 모델 생성 코드도 접근할 수 있다는 공식 설명을 전제로 권한을 나눈다. [OpenAI: Sandbox security](https://developers.openai.com/api/docs/guides/agents-api/environments/security)

| 저장·전달 계층 | 허용 자료 | 기본 접근 |
|---|---|---|
| restricted run artifacts | 필요한 synthetic raw trace/core·binary fixture, 수집 provenance | collector·고정 분석 도구. agent의 임의 `cat`·전체 다운로드 없음 |
| agent evidence views | symbolic stack, typed history, fault receipt, source location, 집계 counter | 해당 과제의 자식·부모가 범위 내 조회 |
| child result envelope | 상태·failure class·검증 범위·evidence ID·hash·분석 결론·남은 불확실성 | 부모에게 완료 또는 의미 있는 상태 변화 때 전달 |
| 최종 보고서 | 원인·수정·반례·검증 결과·제한, 필요한 최소 근거 | 같은 자료 공개 기준으로 검사 후 사용자에게 제공 |

**파일 경로를 나누는 것만으로 접근이 분리되지 않는다.** 같은 filesystem·권한을 공유하는 sub-agent라면 이는 대화량 절약일 뿐 보안 경계가 아니다. 새 XADB 실행 환경에서는 view service 또는 OS 권한으로 raw 접근을 제한하고, controller의 audit 기록은 그대로 유지한다. 경계 밖 작업·거절 사실·효과를 parent나 플랫폼 심사에서 숨기지 않는다.

부모에게 아무 정보도 주지 않으면 실패 원인·검증 여부를 확인할 수 없다. 최소 envelope에는 `run_id, source/build/case/recipe digest, agent_status, failure_class, product_verdict, required_steps_complete, witness/oracle_summary, disclosure_check, evidence_refs, unresolved_questions`가 필요하다. 파일을 썼다는 사실만으로 완료가 아니다. 부모는 schema·digest·권한·결과의 provenance를 확인하고 필요한 view만 추가 조회한다. [subagent-result.example.json](../examples/subagent-result.example.json)은 이 인터페이스의 가상 예다.

전달은 매 token·raw log 대신 **의미 있는 상태와 최종 결과 중심**으로 줄인다. heartbeat는 최소 liveness metadata만 사용한다. 최종 보고서를 만들 때도 원본을 한꺼번에 다시 읽지 않고 검증된 view를 사용한다. 민감정보 검사 실패·미실행이면 보고서 입력으로 승격하지 않고 누락 사유를 표시한다. 보안 결함의 존재·실패 결과를 숨기거나 정상 결과처럼 바꾸지는 않는다.

### 14.12 공식 지침에 근거한 작업 문맥과 비교 실험

OpenAI의 Astra 지침은 실제로 disposable fixture를 사용하고 production 접근이 없는 local test workflow를 구체적으로 설명하면 매 단계의 불필요한 확인을 줄이는 데 도움이 된다고 안내한다. 이를 XADB의 검증된 실행 계약과 연결한다. **말로 안전하다고 선언하는 것과 실제 격리를 갖추는 것은 다르므로**, 해당 profile의 인수 시험이 통과한 뒤에만 그 사실을 AGENTS 문서에 적는다. [OpenAI: Astra workflow guidance](https://learn.chatgpt.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra)


비교 실험은 처음부터 안전한 동일 corpus에서 ① Astra 단독 ② Astra 설계 + Luna 실행을 배정하고, 각각 ③ 필요한 typed evidence 즉시 전달 ④ 파일 중심 envelope 전달을 비교하는 2×2 구성으로 시작한다. 모든 군에 동일한 정보 최소화·권한·oracle·fault 강도를 적용하고 일부 군에 raw secret을 노출하는 실험은 하지 않는다. 실패한 보안 중단 task를 다른 군으로 옮겨 재시도하는 방식도 아니다.

모델 snapshot·reasoning·task 유형·source·profile을 기록하고 실행 순서를 교차한다. 측정값은 정상 완료율, 보안 중단 종류, correctness·결함 유출, 부모의 근거 확인 가능성, 소요 시간·비용이다. 이 비교를 하기 전에는 **Luna 역할 분담과 파일 중심 전달이 중단을 줄인다는 가설**로만 기록한다. 과거 Luna 성공 경험은 유용한 관찰이지만 통제 실험의 결과는 아니다.

### 14.13 AI 개발 방법 자체의 검증

[AI 개발 방법](../policies/ai-development-method.md)은 공식 OpenAI workflow·harness·평가 자료를 대조하여 짧은 context package, 반례→국소 수정→독립 검증, 보호된 oracle, 과제군별 held-out pilot을 정의한다. 현재 Evals 서비스의 종료 공지를 반영해 평가 방법과 특정 hosted 서비스 의존성을 구분한다. 테스트·oracle도 AI가 수정할 수 있지만 정답을 자동 승격하지 않는다. 실제 pilot은 [인수 계획](../contracts/pilot-acceptance.json)의 NOT_RUN 상태로 남겨두었다.

<a id="section-15"></a>

## 15. CI·release gate 설계

아래 시간은 **초기 운영 목표**이며 현재 측정 결과가 아니다. runner 하드웨어, corpus, 제품 규모에 따라 p50/p95를 측정하고 조정한다.

| Gate | 목표 feedback | 실행 내용 | 통과 조건 |
|---|---|---|---|
| G0 로컬 | 1–2분 내 | format, schema, dependency, 변경 unit, 실행 정책·recipe schema | 구조·기본 의미 오류 없음 |
| G1 PR fast | p95 10분 내 목표 | unit/property, 관련 contract, 작은 SQL·crash seed corpus | required assertion/witness 충족, 선택 누락 검사 |
| G2 merge | p95 30분 내 목표 | 실제 XADB server·driver, 지원 기능의 회귀(디스크 저장 지원 이후 global unique·resolver 포함), isolation, WAL/checkpoint, XADBTest self-test, 최초/직전 XADB minor → candidate 복구·변환 smoke | 정확한 merged build에서 통과; worker 경계·중단/reconcile self-test |
| G3 야간 | 예산 기반 campaign | fuzz, larger schedules, compound faults, soak, 같은 XADB major의 모든 이전 minor → candidate campaign | 새로운 반례 triage와 corpus 고정 |
| G4 release | 합의한 candidate별 | 모든 같은-major 이전 minor의 protocol·data·WAL·backup 처리·변환, 변환 중 장애, 장시간·성능·선언한 Linux 조합·지원하는 저장/index 배치·용량 | 미실행·실패 조합 없이 필수 gate와 운영 문서 완결; 실행 profile·증거 복구 검증 |

빠른 gate만 통과하면 완료라는 규칙을 두지 않는다. 반대로 모든 텍스트 수정마다 전체 DB campaign을 요구하지 않는다. 변경 위험은 **영속성 / 동시성 / 공개 의미 / unsafe / 성능** 차원으로 분류하고 각 차원에 필요한 suite를 union한다. 테스트 선택기는 의존 그래프와 contract link를 사용하되, 정기 전체 회귀와 selection mutation으로 누락을 감시한다.

신규 correctness 결함, ACK 유실, silent corruption, reference mismatch, required fault 미도달은 release 차단 사유다. line coverage 단일 숫자로 override할 수 없다. quarantine된 flaky test도 사유·기한·우회 증거가 있어야 하며, 제품의 필수 계약을 검사하는 유일한 테스트라면 quarantine 후 출시하지 않는다.

<a id="section-16"></a>

## 16. 수학적으로 보는 설계·검증 비용

### 16.1 의존성 노출과 변경 비용

모듈 `i → j`를 “i가 j에 의존”으로 정의한 인접행렬을 A라 하면, module j 변경에 노출되는 집합은 A의 역방향 도달 집합이다. 그 가중 크기를 다음처럼 측정할 수 있다.

```text
Exposure(j) = Σ_i w_i · 1[i can reach j]
```

`w_i`는 코드 크기 대신 실제 review/test 비용으로 측정하는 편이 낫다. 도달 가능하다고 모든 모듈을 수정해야 하는 것은 아니고, DAG라고 영향 범위가 작아지는 것도 아니다. 범용 공통 crate가 커지면 DAG에서도 exposure가 커진다. 경계 계약·좁은 타입·stable API가 함께 필요하다.

기능 변경의 총비용은 개념적으로 `명세 + 탐색 + 구현 + 검증 + 재작업 + 인계`다. 코드 생성 속도만 높여서는 다른 비용이 자동으로 줄지 않는다. XADB의 설계는 탐색을 local contract로, 재작업을 독립 oracle로, 인계를 reproducible bundle로 줄이는 데 투자한다. 각 항의 실제 비중은 파일럿에서 측정하며 현재 비율을 지어내지 않는다.

### 16.2 조합 폭발은 테스트 수만 늘려 해결하지 못한다

예시로 6개 data shape × 4개 operation × 3개 transaction mode × 8개 fault × 5개 schedule이면 2,880개 조합이다. 범위를 하나씩 늘리면 곱셈으로 커진다. pairwise는 비용을 줄일 수 있지만 “checkpoint × WAL reclaim × active backup” 같은 3요인 결함을 보장하지 않는다.

그래서 **필수 계약의 위험 조합을 먼저 명시하고**, 나머지를 property generation·bounded schedule·fuzz로 탐색한다. 테스트 선택은 집합덮개로 생각할 수 있다. 계약 집합 U와 테스트 j가 검증하는 S_j, 비용 c_j가 있을 때:

```text
minimize Σ_j c_j x_j
subject to Σ_(j: u∈S_j) x_j ≥ 1  for every required contract u
```

단, S_j는 “실제 witness와 유효 oracle이 확인된 계약”이어야 한다. 이름에 recovery가 들어간다고 recovery 계약을 덮는다고 입력하면 최적화가 잘못된다. 고위험 계약은 서로 다른 oracle 계열로 2중 검증하는 제약을 추가하고, 결과를 완전성 증명이라고 부르지 않는다.

### 16.3 재현 가능한 실행이 feedback 효율을 바꾼다

한 iteration이 유효한 검증에 도달할 확률을 조건부 확률로 쓰면:

```text
p_valid = P(environment valid)
        × P(target reached | environment)
        × P(discriminating oracle | environment, target)
```


### 16.4 “실패 0회”의 한계

독립·동일 분포 시험 n회에서 실패가 0이면, 해당 분포의 미지 실패 확률 p에 대한 단측 95% 상한은 `1 − 0.05^(1/n)`이다. n=1,000이면 약 0.299%다. 이는 production에서 데이터 유실 확률이 0.299% 이하라는 뜻이 아니다. 실제 스케줄은 상관되어 있고 시험 분포도 production과 다르다. 동일 seed의 반복 통과는 새 독립 표본이 아니다.

branch coverage는 실행 여부, mutation score는 특정 오류를 구분하는 힘, history 검사는 특정 의미 계약을 각각 측정한다. `killed / eligible mutants`를 보고할 때 equivalent·invalid·uncovered·timed-out mutant 수를 함께 공개한다. 100% mutation score도 입력·결함 모델 밖의 정확성을 증명하지 않는다.

### 16.5 마이너 버전 호환의 방향성과 비용

고객에게 인도한 같은 major의 minor만 출시 순서대로 i, j로 번호 매기고 `C(i,j,k)=1`을 “i가 만든 입력·데이터·요청을 j가 계약 k에 맞게 처리함”으로 정의한다. 요구사항은 **i ≤ j일 때 C(i,j,k)=1**이다. i < j일 때 역방향 `C(j,i,k)=1`은 요구하지 않는다. SQL 결과의 동등성 하나로 WAL recovery·backup restore의 C까지 1이라고 추론할 수 없으므로 계약 k별 oracle을 둔다.

n개 minor에 대해 self-pair까지 포함한 누적 필수 version pair 수는 다음과 같다.

```text
N(n) = Σ(j=1..n) j = n(n+1)/2
ΔN = N(n+1) − N(n) = n+1

서로 다른 버전의 upgrade만 세면:
N_upgrade(n) = n(n−1)/2
ΔN_upgrade = n
```

기존 minor가 4개라면 새 release는 **이전 4개 → 새 버전 + 새 버전 자체 = 5 pair**를 추가한다. 역방향까지 강제했을 때의 9 pair가 아니다. 이 값은 minor의 **방향 있는 pair 수**이며 patch·client·filesystem·fault·schema variant를 포함한 실제 test 수가 아니다. 과거 pair의 결과는 보존하고, framework/oracle/공통 코드가 바뀌어 과거 증거가 무효화되면 영향 범위를 다시 실행한다.

형식 변환 `U(i,j)`를 이전 형식의 유효한 복구 상태 `s ∈ S_i`를 새 상태 `S_j`로 옮기는 함수로 보자. 최소 조건은 다음과 같다.

```text
U(i,j)(s) ∈ S_j
Obs_old(U(i,j)(s)) = Obs_old(s)
Acked_old(s) ⊆ RecoveredCommits_j(U(i,j)(s))
```

`Obs_old`는 이전 계약의 논리 row·catalog·constraint·권한 의미를 관찰하고, byte layout의 동일성을 요구하지 않는다. 위 commit 집합은 oracle이 추적하는 논리 history이며 과거 WAL record를 무기한 보존하라는 뜻은 아니다. ACK 포함 조건만으로 충분하지 않으므로 partial batch 금지, commit 중복 적용 금지, 허용된 미응답 commit의 처리도 별도로 검사한다. `U(j,i)`라는 역변환은 필수 함수가 아니다.

이전 형식 f를 현재 내부 표현으로 직접 읽는 decoder와 명시적 converter registry를 기본으로 두고, converter chain을 선택하면 모든 중간 단계의 domain·불변식·재시작 규칙을 보존한다. 인접 minor별 시험이 통과했다고 실제 건너뛴 upgrade도 실행 검증되었다고 간주하지 않는다. 직접 변환과 연쇄 변환을 모두 제공한다면 `Obs_old(U(i,k)(s)) = Obs_old(U(j,k)(U(i,j)(s)))`를 검사한다. 이전 fixture 전체에 대해 새 binary로 직접 시작하는 시험은 내부 구현이 chain이더라도 필요하다.

장애 시험의 분모도 별도로 기록한다. 변환 경로 p에 필수 cut이 b_p개이고 저장 fault variant가 f_p개라면 기본 실행 셀은 `Σ_p b_p × f_p`다. 이는 지정한 경로·cut의 유한 검사량일 뿐 모든 가능한 상태의 증명은 아니다. minor pair 수를 줄인 효과와 새로 필요한 변환 안전성 시험의 비용을 섞어 “시험 비용이 절반”이라고 단정하지 않는다.

### 16.6 “안전한 범위 안에서 개발이 완결되는가”를 수학적으로 검사한다

필수 검증 계약 집합을 R, 허용된 효과 집합을 A, 계약 r를 검증하는 실행 경로를 p라 하자. XADB의 기본 개발 경로가 갖춰야 할 조건은 다음과 같다.

```text
∀ r ∈ R, ∃ p:
    verified_witnesses(p) cover r
    ∧ oracle(p) is valid
    ∧ actual_effects(p) ⊆ A
```

이는 허용 command 이름의 집합이 아니다. `actual_effects`는 하위 process·build script·file/network 접근까지 포함한다. 경로는 명시적 input/resource와 OS 집행으로 제한한다. 전체 일반 프로그램의 효과를 정적으로 완벽하게 계산할 수 있다고 가정하지 않으며, 고정 recipe의 검토·runtime receipt·격리 거절 시험을 함께 쓴다. 어떤 필수 r에 대해 이런 경로가 없으면 테스트를 지우는 대신 API·fixture·worker 구조를 보강한다.

정상 corpus의 외부 효과 벡터를 다음과 같이 정의하고 목표를 영벡터로 둔다.

```text
H = (unowned_process_control,
     shared_or_production_database_write,
     host_security_configuration_change,
     real_credential_access,
     unapproved_external_network_access)
Target: H = (0,0,0,0,0)
```

소유한 worker의 crash·overlay 변경은 별도 명시된 허용 효과다. 단순히 host 명령을 다른 이름의 script 안으로 옮겨 H를 0으로 기록하면 잘못된 측정이다. controller가 실제 process·file·network 경계를 검증하고 receipt와 부정 시험으로 이를 확인한다. H=0도 모델의 모든 판단을 예측하는 수학적 증명은 아니지만, 제품이 책임질 수 있는 객관적 설계 조건이다.

한 과제의 단계 i에서 이전 중단이 없었다는 조건 아래 중단될 확률을 q_i라 두면 chain rule로 `P(no stop)=∏(1−q_i)`다. q_i=0.02라는 가상 값에서 50개 단계는 약 36.4%, 8개 단계는 약 85.1%다. **Astra 실측치가 아니며 API 호출 수를 숨겨 줄이면 개선된다는 뜻도 아니다.** 실제로 불필요한 경계 초과를 제거해야 한다. 비동기 monitoring은 누적 맥락을 볼 수 있어 독립·동일 확률 가정을 실제 시스템에 그대로 적용하지 않는다.

운영 검증에서는 `보안 중단 task / 전체 정상 task`, `필수 계약 충족률`, `H의 위반 수`, `경계 밖 요청 오허용 수`, `분류 불명 중단 수`를 함께 보고한다. 300개의 독립·동일 조건 task에서 보안 중단 0건이라도 단측 95% 이항 상한은 `1−0.05^(1/300) ≈ 0.994%`다. “0건 관찰”과 “중단 불가능”을 구분하고 실제로는 모델·환경·과제별로 층화한다. 이 보고서는 이 숫자를 공식 정책의 차단 확률로 제시하지 않는다.

### 16.7 모듈 시험으로 줄이는 효과의 범위와 남는 검증

시험 t의 권한·자원 집합을 F(t)라 두면 M1의 설계 목표는 `F(t) ⊆ {memory, owned bounded scratch}`다. M2는 소유한 fixture·controlled I/O를 추가하고, M3만 소유 server·실제 OS adapter를 추가한다. 단순히 시험 이름이 unit인지가 아니라 실제 의존성과 effect로 분류한다.

필수 계약은 `R = R_M1 ∪ R_M2 ∪ R_M3`로 관리한다. 집합은 겹칠 수 있으며, `R_M1 = R`라고 가정해서는 안 된다. 각 계약 r의 필요한 모듈·상호작용·실제 효과 witness를 정의하고 적합한 suite로 덮는다. M1이 빠르다는 이유로 필수 M3를 지우면 비용 절감과 정확성 저하를 혼동한다.

변경 후 재현 비용은 `T = T_environment + T_fixture + T_execution + T_diagnosis`로 나눈다. 모듈 분리가 실제로 도움이 되려면 서버 준비·관리 권한·로그 혼합 비용이 줄면서 같은 결함의 검출과 원인 특정이 유지되어야 한다. 모듈별 가짜 구현만 검사해 실행 시간을 줄이는 것은 목표가 아니다. 이 범위 축소가 Astra 중단을 얼마나 줄이는지는 정상 workload의 비교 실험으로 확인한다.

DB 장애의 원래 실행을 D, 축소한 capsule을 d라 두면 축소 성공 조건은 단순 message 일치가 아니라 `same_contract_violation(d,D) ∧ required_causal_witnesses(d) ∧ actual_core_executed(d)`다. 이 조건 아래 입력 크기·의존 모듈 수·외부 effect를 줄인다. 모든 최소 반례의 전역 최적해를 찾는다고 약속하지 않고 module boundary·input·schedule을 순차 축소한다. 축소가 불가능한 사례도 분모에 남기고 `module 재현 성공 / 유효한 DB 장애 사례`와 수정 후 M3 회귀 통과를 함께 측정한다.

<a id="causal-math"></a>

### 16.8 원인 후보 한정의 수학적 조건과 평가

#### 연결된 코드는 후보이지 확정 원인이 아니다

특정 실행에서 영향을 **원인 쪽 → 결과 쪽**으로 정규화한 그래프를 `G⁺=(V,E_observed ∪ E_possible)`로 둔다. 노드는 event·상태 version·symbol·기대 동작이며, edge에는 데이터·제어·객체 수명·동기화·영속성·검사 의존을 포함한다. `E_possible`은 정적 계약과 실행 조건상 아직 배제하지 못한 경로다. 실패한 값과 검사 노드의 집합을 F라고 하면 다음을 제안한다.

```text
S(F)       = Ancestors_G⁺(F) ∪ F
C_reach(F) = owners(S(F)) ∪ unresolved_external_candidates
C_safe(F)  = C_reach(F) \ X_certified(F)
C_first(F) ⊆ C_safe(F)       # 증거에 따른 우선 조사 부분집합
```

`X_certified(F)`는 해당 실패에 대한 영향 경로가 모두 충분한 경계 계약으로 차단되었다는 증거가 있는 후보만 담는다. 한 경로의 PASS로 같은 module의 다른 경로까지 제외하지 않는다. 그런 제외 근거가 없으면 이 집합은 공집합이다. graph의 역도달 집합과 경계 검증으로 더 좁힌 집합을 구분한다.

실제 원인이 후보 안에 남으려면 **모델이 관련 영향 경로를 빠짐없이 포함하고, identity·version·순서·검사 결과가 정확해야 한다.** 이 전제 아래에서만 실제 원인의 owner가 `C_safe`에 포함된다고 말할 수 있다. 관측 graph만으로 만든 좁은 집합은 보통 우선 조사용이다. 단순히 `정적 참조 후보 ∩ 관측 후보`를 취하면 미관측 writer·누락 동작을 잘못 버릴 수 있으므로 XADB는 제외 근거 없는 가능 경로를 유지한다. 미확인 범위가 남으면 “전체에서 2개만 가능”이라고 보고하지 않는다.

주어진 유한 그래프의 역도달 탐색 자체는 adjacency list에서 `O(|V|+|E|)`로 할 수 있다. 하지만 정확한 edge 추출·alias 분석·누락 복원·원인 확정 비용이 이 식으로 해결되는 것은 아니다. heap의 모든 byte와 가능한 schedule을 전부 추적하는 방식 대신 계약 경계와 재현 구간을 이용한다.

#### 경계 계약이 주는 추론

모듈 i의 입력·사전 상태를 x, 명시한 외부 의존을 e, 결과를 y라 두고 계약을 다음처럼 쓴다.

```text
P_i(x,e) ⇒ Q_i(x,e,f_i(x,e))

P_i(x,e) ∧ ¬Q_i(x,e,y)
  ⇒ {i 구현, 숨거나 잘못 모델링한 의존, 계약/검사기}를 조사
```

테스트 한 번에서 `P_i`가 참이고 `Q_i`가 거짓이라는 관찰은 선언한 계약의 반례다. 이것만으로 i의 어느 line이 잘못됐는지는 정해지지 않는다. 입력 생산자를 제외하려면 관련 의미가 P에 충분히 들어 있어야 하며, 다른 입력·schedule까지 일반화하려면 별도 검증이 필요하다. 오류가 중간에 생겼다가 사라질 수 있으므로 경계 PASS/FAIL이 단조라고 가정한 이분 탐색도 기본 알고리즘으로 삼지 않는다.

#### 좁히는 성능과 범인을 놓치지 않는 성능을 함께 평가한다

ground truth가 알려진 fault corpus에서 사례 j의 실제 원인 module 집합을 `R_j`, 진단 후보를 `C_j`, 초기 조사 집합을 `M_j`로 정의한다. 모든 분모와 집계 단위를 고정한다.

```text
Reduction_j = 1 − |C_j| / |M_j|        # 같은 module 단위, 알려진 범위에서
Recall      = Σ_j |R_j ∩ C_j| / Σ_j |R_j|
ExactContainment = count(R_j ⊆ C_j) / valid_cases
```

가상 예시의 6→4→2개 후보는 각각 약 33.3%, 66.7% 감소다. 두 후보를 평가할 때 한 사례의 실제 결함이 B 하나이고 `{A,B}`를 보존하면 해당 사례 recall은 100%다. `{A}`로 더 줄이면 후보 감소율은 83.3%지만 recall은 0%다. 그래서 **작은 후보 수보다 원인 누락 방지가 먼저**다. 이 수치는 XADB 실측이 아닌 산술 예시다. unknown 후보를 숫자 1개로 치환하여 작은 후보 수를 꾸미지 말고, incomplete 사례율과 미확인 경계 크기를 별도 보고한다.

실제 개발 비용은 `T_total = T_capture + T_graph + T_inspect + T_experiment + T_regression`으로 측정한다. 후보 module마다 분석 비용이 다르므로 단순 개수뿐 아니라 `Σ_{m∈C} review_cost(m)`도 본다. 파일 수 1/3 감소가 시간 1/3 감소를 뜻하지 않는다. fault 위치를 사전에 알려 주지 않은 진단 평가에서 top-k 원인 포함률, 후보 누락, 원인 검증까지의 시간, 모듈 재현 성공률, 계측 비용을 함께 기록한다.

XADBTest 초기 인수 corpus에는 producer 오염, stale reference, 누락된 무효화, async context 유실, 잘못된 oracle, 복수 결함, trace 유실, 공통 기반 영향 등을 넣는다. 필수 사례에서 원인을 후보 밖으로 버리면 gate를 실패시키고, 정보가 부족한 사례는 INCOMPLETE로 정직하게 분류하는지도 검사한다. 작은 corpus 100% 통과는 새 결함 전체에 대한 보장이 아니다.

<a id="section-17"></a>

## 17. 모델과 실제 증거

이전 작업의 유한 모델 결과를 XADB 제품 증거로 승계하지 않는다. 새 모델을 실행할 때는 탐색 경계·negative variant·source hash를 보존한다. 모델 성공도 실제 파일 I/O·동시성·crash/reopen 시험을 대신하지 않는다.

<a id="section-18"></a>

## 18. AI가 성능을 개선하는 절차

**사용자 요구를 구체화한 [AI 성능 개발 계약](../contracts/ai-performance-engineering.md)을 적용한다.** AI가 계측값에서 소유 코드·불변식·실험 recipe로 이동하여 병목 후보를 만들고, 구분 실험으로 원인을 확인하고, 국소 수정과 독립 비교·회귀를 끝낼 수 있어야 한다. 시간/CPU/대기/작업량 의미, 비동기·공유 batch 연결, 저비용 계측과 유실, 재진입 bundle, 개선·회귀·불확실 판정 및 DEV2부터의 14개 미실행 인수 의무는 그 문서와 JSON이 소유한다. 아래 원칙을 실행 가능한 구조로 확장하며 아직 구현·성능 향상을 검증한 상태는 아니다.

정확성 기준을 고정하고, 병목 가설 하나에 대해 바뀐 비용을 측정한다. allocator·cache miss·lock wait·WAL flush·queue·scan rows·bytes read·index amplification을 request/operation 단위로 관찰한다. throughput 하나만 보면 abort 증가나 p99 악화가 가려진다.

| 단계 | 해야 할 일 |
|---|---|
| 기준선 | hardware/OS/CPU policy/compiler/data distribution/cache state/concurrency/durability를 고정 |
| 실험 | A/B 실행 순서를 교차·무작위화하고 warmup과 측정 구간을 분리 |
| 지표 | 성공 transaction/s, error·abort/retry, p50/p95/p99, CPU, RSS, I/O, write amp, recovery time |
| 설명 | profiler와 operation counter가 예측한 병목 감소를 확인 |
| 검증 | 같은 semantic/fault/recovery suite를 재실행, 변경한 경로 witness 확인 |
| 보존 | benchmark 입력·결과·환경·분석 스크립트를 revision과 묶음 |

클라이언트가 처리 지연 때문에 요청을 늦추는 closed-loop 측정은 과부하 tail을 숨길 수 있다. 목표 도착률을 제어하는 workload도 사용하고 backlog·timeout을 함께 기록한다. 성능 개선을 위해 sync, validation, checksum, isolation을 끄면 다른 제품 계약이므로 같은 비교에 넣지 않는다.

각 paired run의 throughput 비율에 대해 `d_i = log(candidate_i / baseline_i)`를 계산하고 평균·불확실성 구간을 보고한다. latency는 비율의 방향을 반대로 해석한다. autocorrelation·긴 tail이 있으면 단일 request를 독립 표본으로 세지 않고 run/block 단위 bootstrap 등을 검토한다. 처음부터 “5% 빨라야 성공”을 모든 benchmark에 강요하지 말고 noise와 고객 의미로 gate를 정한다.

성능 병목의 비중 f만 k배 개선할 수 있다면 Amdahl 식 `S = 1 / ((1−f)+f/k)`가 상한을 설명한다. f=0.2, k=2이면 약 1.11배다. 따라서 미세 최적화 전에 profile을 보고 가장 큰 검증 비용·실행 비용을 찾는다. 이 수치 역시 가상 예다.

초기 직렬 commit은 병목이 될 수 있다. 먼저 batch/group commit으로 flush를 공유하고, 그다음 validation·준비 작업 병렬화, 이후 더 좁은 conflict tracking을 검토한다. group commit에서도 공개 sequence·ACK·failure handling은 같은 계약을 지켜야 한다. 분산·lock-free·JIT를 동시에 추가하면 원인 분리가 어려워진다.

<a id="section-19"></a>

## 19. 개발 순서

[하위 계층 계획](../plans/bottom-up-development.md)과 [개발 단계](../plans/development-stages.md)를 따른다. DEV2 K0–K5, DEV3 disk storage/index, DEV4 SQL, DEV5 server, DEV6 client 순서이며 COMM/PART/NG는 명시적 의존성으로 분리한다.

<a id="section-20"></a>

## 20. 운영·릴리스

[제품 방향](../plans/project-direction.md)과 [버전 정책](../policies/version-compatibility-policy.json)에 따른다. 사용자에게 실행 가능한 capability와 미구현/미검증 제약을 정확히 공개한다.

<a id="section-21"></a>

## 21. 대안·위험·검증해야 할 결정

| 선택지/위험 | 평가 | 다음 구분 실험 또는 대응 |
|---|---|---|
| Rust 신규 커널 | **사용자 확정.** 안전 경계 표현에 유리하나 DB 의미 보장은 아님 | 실제 WAL+snapshot+index 작은 slice의 개발·디버깅·성능 측정 |
| 필요한 외부 라이브러리·FFI | 주 언어 Rust를 유지하며 좁은 adapter로 검토 | ownership/error/thread/버전 계약과 독립 시험. C++ 전체 엔진 wrapper로 대체하지 않음 |
| 기존 KV 엔진 위 SQL | 저장·복구 비용을 줄일 후보이나 내부 디버깅·snapshot·메모리 요구 제약 | 고객 요구와 engine transaction semantics의 적합성 검증 |
| 기존 SQL 엔진 fork/확장 | 빠른 기능 범위 확보 후보. 자체 아키텍처 목표와 제약을 비교 | 새 제품의 차별성이 정말 새 kernel을 요구하는지 검토 |
| XADB minor 하위 호환의 유지 비용 | 형식 진화는 가능하지만 이전 decoder·변환기·fixture를 계속 유지해야 함 | decoder registry·형식 경계 corpus·변환 중 fault 시험. 새 reader만 시험하거나 과거 지원을 조용히 삭제하지 않음 |
| XADBTest 자체의 결함·개발 비용 | 새 runner·oracle·scheduler도 검증 대상 | 잘못된 input·missed fault·known-bad history self-test와 kernel end-to-end 병행 |
| test simulator와 현실 차이 | simulation 통과만으로 OS/메모리 모델 보장 불가 | 실제 filesystem·thread·hardware 시험과 fault 모델 대조 |
| AI가 oracle까지 함께 틀림 | 모든 gate가 녹색이어도 의미 오류 가능 | independent reference, held-out corpus, known-bad, 의미 review |
| 직렬 commit 병목 | correctness 출발점의 의도된 비용 | group commit·validation 병렬화 후 같은 불변식 검증 |
| 오래 사는 snapshot | memory/WAL retention 무제한 증가 가능 | quota·deadline·취소·백업 pin 운영 계약 |
| observability 비용 | 로그가 성능·스케줄을 바꿀 수 있음 | bounded trace·drop counter·on/off 동등성·overhead benchmark |

완전 신규 엔진을 권고한다고 기존 엔진 대안을 배제할 필요는 없다. 첫 파일럿이 재작성의 위험·비용을 드러내면 고객 목표를 기준으로 선택을 바꿀 수 있어야 한다. 다만 이 보고서의 기본 설계는 **새 XADB 커널을 구축하는 안**이며, 기존 엔진 위 wrapper만 만든 뒤 새 DB 완성이라고 부르는 안은 아니다.

<a id="section-22"></a>

## 22. 성공 여부를 판단할 지표

| 지표 | 정의 | 피할 오해 |
|---|---|---|
| 기능 완료 lead time | 계약 확정→필수 gate 통과 | commit 수·생성 LOC를 생산성으로 쓰지 않음 |
| 첫 유효 반례 시간 | 실패 제보→올바른 binary/path/oracle의 재현 | 첫 테스트 실행 시간과 구분 |
| 재현률 | 보존 bundle 중 다른 환경에서 선언 범위 재현 성공 비율 | 동일 프로세스 재실행만 세지 않음 |
| escape/reopen | 완료 후 발견된 의미·복구 결함, 다시 열린 작업 | 완료된 과제만 뽑아 시간 단축 주장 금지 |
| 검증 분모 | 지원 계약·시험 variant·필수 fault 중 충족/미충족 | SKIP·INVALID를 숨겨 통과율 높이지 않음 |
| 성능 비용 | 성공 처리량·tail·memory·I/O·복구의 paired 변화 | durability/격리 수준을 낮춘 수치 제외 |
| 사람 개입 | routine correction과 제품 의미 판단의 시간 분리 | 사람 없는 시간을 무조건 성공으로 세지 않음 |
| 실행 중단 비용 | 사유별 stop·경계 요청·재작업 시간·증거 인계 성공률 | 모델 monitoring과 OS 오류를 합치거나 blocked 과제를 제외하지 않음 |
| 경계 정확성 | 정상 corpus 완료율과 위반 corpus 오허용 수 | 차단률만 낮추는 것을 개선으로 세지 않음 |
| 원인 후보 품질 | 후보 감소율·ground-truth recall·복수 원인 전체 포함률·INCOMPLETE 비율 | 후보를 작게 만들기 위해 원인이나 미확인 경계를 제외하지 않음 |
| 진단 계측 비용 | capture/graph/검토/실험/회귀 시간과 trace on/off 성능 | 로그 수·graph 크기 자체를 성과로 삼지 않음 |

처음에는 기능 추가, recovery 수정, 동시성 수정, 성능 개선 각 유형의 소수 과제로 계측 자체를 검증한다. 이후 난이도를 맞춘 반복·교차 실험에서 `log(T_AX / T_baseline)`과 품질 지표를 함께 본다. 미완료·시간 초과 과제도 censoring/실패로 남긴다. 현재 자료로 AI가 몇 배 빨라질지 또는 몇 명을 대체할지 수치화할 근거는 없다.

<a id="section-23"></a>

## 23. 바로 시작할 때의 실행 순서

1. **사내 개발 범위와 대체 기준을 명시한다.** 제품 기능 기준과 사내 단계의 capability를 구분해 적는다. 개발 순서로 보류한 기능을 대체 목록에서 제외하지 않는다.
2. **별도 XADB repository와 고정 build/test 진입점을 만든다.** 기존 개발 tree의 환경 변수에 기대지 않는다.
3. **SQL보다 먼저 작은 영속 커널과 XADBTest를 함께 만든다.** commit·crash·checkpoint·restore가 실제로 연결되게 한다. 격리 worker·실행 전후 journal·중단 인계도 첫 runner에 포함한다.
4. **하나의 SQL 기능을 끝까지 연결한다.** 실제 client → transaction → WAL → restart → typed oracle까지 확인한다.
5. **XADBTest와 새 native corpus를 함께 완성한다.** 독립 oracle·fault·replay를 연결한다. 안정 릴리스 qualification의 이전 고객 XADB minor → 새 candidate suite에는 보존 binary와 format fixture·변환 fault를 넣으며 사내 배포에 이전 버전 호환을 강제하지 않는다.
6. **AI에게 새 기능·실패 수정·시험 생성·성능 개선을 맡겨 개발 체계 자체를 시험한다.** 사람이 실행 설명을 보충하지 않아도 bundle로 재현되는지 확인한다.
7. **검증된 범위만 확장한다.** 안정 릴리스한 XADB 버전 간 same-major 호환 계약을 유지하며 기능·성능·디스크 규모·HA의 gate를 통과한다.

첫 투자에서 확보할 가장 중요한 자산은 거대한 코드베이스가 아니라 **작은 반례를 반복해서 고칠 수 있는 정확한 커널과 검증 체계**다. 그 위에서 AI가 수행하는 개발 범위를 넓혀야 기능 추가 속도와 제품 신뢰성을 함께 개선할 수 있다.

<a id="section-24"></a>

## 24. 근거와 재현

[공개 참고 자료](opensource-references.md), [현재 구현](../implementation/README.md), [XADB 실행 증거](../evidence/README.md), [이관 기록](../history/project-migration.md)을 구분해서 읽는다. 기존 설계나 시험의 출처를 새 참고 제품으로 바꾸지 않는다.

<a id="n01"></a>

<a id="n02"></a>

<a id="n03"></a>

<a id="n04"></a>

<a id="n05"></a>

<a id="n06"></a>

<a id="n07"></a>

<a id="n08"></a>

<a id="n09"></a>

<a id="n10"></a>

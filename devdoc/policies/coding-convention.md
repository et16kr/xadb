# XADB 코딩 컨벤션과 개발 절차

2026-10-18 · **개발 기준 v1.1 / 새 코드와 변경 코드에 적용**

[규칙 색인 JSON](coding-convention.json) · [저장소 배치](repository-build-plan.md) · [Rust 구현 계약](../contracts/implementation-contracts.md) · [AI 개발 방법](ai-development-method.md)


이 기준은 새 코드와 변경하는 코드에 적용한다. 기존 코드 전체가 준수한다고 인증하거나 일괄 이름 변경·포맷을 요구하는 것은 아니다. DB 커널 계약과 개발 순서는 기존 결정을 따르며, 아래 WAL·snapshot 예시는 명명과 문서화의 예이다. 구현된 API와 AI 개발 효율의 측정 결과로 해석하지 않는다.

<a id="section-1"></a>

## 1. 적용 범위와 기준 문서


| 판단할 내용 | 기준 |
| --- | --- |
| 구현 순서와 단계 완료 | [하위 계층 개발](../plans/bottom-up-development.md), [단계 JSON](../plans/development-stages.json) |
| 폴더·crate·의존 방향·실제 빌드 진입점 | [저장소 배치 정책](repository-build-plan.md), [관리 JSON](repository-build-plan.json) |
| 커널 원자성·수명·오류·동시성·OOM | [Rust 구현 계약](../contracts/implementation-contracts.md), 해당 기능 계약 |
| 현재 DEV1 API·제약 | [DEV1 구현](../implementation/dev1-implementation.md), [DEV1 계약](../contracts/dev1-contract.json), 실제 소스 |
| 호환 의무 | [버전 정책](version-compatibility-policy.json) |
| 이름·서식·계약 표현·리뷰 방식 | 이 문서 |
| 결정 권한·독립 검토·검증 프로파일·의존성·복잡성 판단 | [개발 운영 방침](development-policy.md) |

충돌을 발견하면 의미를 정하는 계약과 실제 구현 상태를 확인하고 관련 Markdown·JSON을 함께 수정한다. 이름을 정하면서 기존 동작·형식을 암묵적으로 바꾸지 않는다. 사내 배포는 새 DB·fixture를 사용하며 이전 사내 버전 호환을 요구하지 않는다. 현재 build의 복구·원자성·무결성 검증은 유지하고 안정 릴리스부터 정해진 호환 검사를 적용한다.

<a id="section-2"></a>

## 2. 소속과 역할을 표현하는 방법


다음은 DEV2 계획 영역에 적용할 명명 예시이며 아직 구현된 API가 아니다.

```text
소스 폴더    crates/engine
Cargo 이름   xadb-engine
모듈         durable::wal
타입         WalWriter
메서드       append_batch
호출         wal_writer.append_batch(...)
```

모듈 밖에 import된 타입도 뜻이 분명하도록 `WalWriter`, `SnapshotGuard`, `ObjectHandle`처럼 도메인과 역할을 남긴다. `XadbEngineDurableWalWriter`처럼 경로를 모두 반복하지 않는다. `std::io::Read`, 모듈 문맥이 분명한 `Error`·`Result` 같은 관용 이름은 사용할 수 있다. 서로 다른 오류 타입이 한곳에 모이면 `io::Error` 같은 경로 또는 의미 있는 import alias로 구분한다.

`Manager`, `Context`, `Data`, `Info`, `Service`는 책임이 확인될 때 사용한다. 예를 들어 실제 책임이 commit 순서 조정이면 `CommitCoordinator`가 더 구체적이다. 이미 의미가 명확한 이름을 접미어 규칙만으로 바꾸지는 않는다.

<a id="section-3"></a>

## 3. 식별자와 용어

| 대상 | 규칙 | 예 |
| --- | --- | --- |
| 제품 crate 폴더 | 제품명 접두어 없이 책임 이름 사용 | `crates/foundation`, `crates/platform`, 계획 `crates/engine` |
| 시험 도구 crate 폴더 | `crates/testing` 아래 책임 이름 사용 | `controller`, `protocol`, `reference`, `engine-worker` |
| Cargo 패키지 | 폴더명과 별도 식별자. 현재 소문자·하이픈 이름 유지 | `xadb-foundation`, `xadbtest-protocol` |
| Rust crate 경로 | Cargo 패키지와 구분해 밑줄 사용 | `xadb_foundation::identity` |
| 모듈·Rust 파일 | 책임 중심의 `snake_case` | `transaction`, `commit_batch.rs` |
| 타입·trait·enum variant | `UpperCamelCase` | `ObjectHandle`, `FileIo`, `Certainty::Unknown` |
| 함수·메서드 | 동작 중심의 `snake_case` | `validate_current`, `append_batch` |
| 인자·지역변수·필드 | 의미 중심의 `snake_case` | `schema_epoch`, `row_id`, `self.quota` |
| 상수·static | `SCREAMING_SNAKE_CASE` | `MAX_FRAME_BYTES` |
| 테스트 함수 | 조건과 기대 동작을 드러내는 `snake_case` | `stale_handle_is_rejected` |

저장소가 이미 XADB 제품을 식별하므로 제품 소스 폴더에 `xadb-`를 반복하지 않는다. 현재 제품 폴더는 `foundation`, `platform`이며, 후속 `engine`, `transport`, `sql`, `server` 등도 같은 기준을 따른다. Cargo package 이름과 Rust import 이름은 별도 식별자이므로 이번 폴더 정리에서 유지한다. 예를 들어 `cargo test -p xadb-foundation`은 `crates/foundation`을 대상으로 한다. 독립 시험 도구의 패키지·명령 이름인 `xadbtest` 계열은 유지하고, 소스 폴더는 `crates/testing/controller`, `protocol`, `reference`, `fixture-worker`, `foundation-worker`, `engine-worker`로 묶는다. `testing`은 분류용 디렉터리이며 각 하위 패키지는 별도 `Cargo.toml`과 의존 경계를 유지한다. 채택 근거와 이전 규칙의 대체는 [시험 도구 배치 결정](repository-build-plan.md#testing-layout)을 따른다. 기존 증거에 기록된 원래 경로는 해당 실행의 identity로 보존한다.

Rust 코드에는 인자 `a`, 지역변수 `s`, 멤버 `m`, 전역 `g` 같은 범주 접두어를 추가하지 않는다. 변수의 역할은 이름과 선언, 멤버 접근은 `self.field`로 드러낸다. 짧은 루프의 `i`, 일반적인 타입 매개변수 `T`까지 긴 이름으로 바꾸지는 않는다.

약어는 WAL·SQL·I/O·ID처럼 도메인에서 통용되는 것을 사용한다. Rust 타입에서는 `WalWriter`, `SqlError`, `FileIo`, `ObjectId`로 쓴다. 사용자에게 보이는 SQL 키워드·오류 ID·protocol 필드에는 각 형식의 계약을 적용한다. 단순 getter는 `schema_epoch()`처럼 쓰되, 유일하고 명확한 값을 꺼내는 현재 ID의 `get()`은 유지할 수 있다. 변환은 Rust의 `as_`·`to_`·`into_` 의미를 따르며 반환 타입과 소유권 이전을 함께 확인한다. [Rust API 명명 지침](https://rust-lang.github.io/api-guidelines/naming.html)

다음은 초기 용어 기준이다. 기능 명세가 더 구체적인 뜻을 정하면 그 정의를 연결한다.

| 개념 | 기본 표기 | 구분할 의미 |
| --- | --- | --- |
| 트랜잭션 | `transaction`, 관용 약어 `txn` | 모듈·새 공개 타입은 전체 단어를 우선. 기존 문서의 `TxnId`는 제안 타입이며 임의 rename 근거가 아님 |
| 논리 행 | `row`, 현재 `RowId` | `record`는 WAL/파일 레코드 등 별도 개념에 사용 |
| 안정 행 정체성 | 설계 용어 `RowUID`, 현재 기초 타입 `RowId` | 이름만으로 동일한 완성 계약이라고 보지 않음. 발급·유일성·영속 수명은 DEV2 이후 담당 계약에서 확정 |
| 객체 재사용 세대 | `generation`, `Generation` | 객체의 drop/recreate·재사용 판별. schema 변경과 구분 |
| 스키마 변경 세대 | `schema_epoch`, `SchemaEpoch` | 일반 상태 version이나 commit 순서와 구분 |
| commit 순서 | `commit_seq`, `CommitSeq` | 벽시계 시각과 구분 |
| 크기·시간 | `payload_bytes`, `timeout`, 필요 시 `timeout_ms` | 가능하면 `Duration` 등 타입으로 단위 표현. 직렬화 숫자는 단위 명시 |

같은 개념을 파일마다 다른 동의어로 바꾸지 않는다. 서로 다른 개념의 이름도 편의상 합치지 않는다. 새 약어·단위·공개 용어를 도입할 때 소유 모듈 문서나 이 용어표에 뜻을 기록한다.

<a id="section-4"></a>

## 4. 폴더와 모듈 경계

실제 배치는 [저장소 정책](repository-build-plan.md)을 따른다. 현재 공통 의미·effect 계약은 `xadb-foundation`, Linux 파일·시간·scheduler 구현은 `xadb-platform`이 소유한다. DEV2의 `transaction`, `publication`, `catalog`, `storage`, `durable`은 계획된 `xadb-engine` 내부 모듈이다. 역할 하나마다 crate를 만들거나 빈 폴더를 미리 만들지 않는다.

작은 모듈은 `name.rs`로 시작하고, 내부 책임이 나뉘면 `name/mod.rs`와 하위 파일로 확장한다. 타입당 파일 하나, 고정 함수 길이, 고정 파일 줄 수를 강제하지 않는다. 하나의 변경을 이해하려고 따라가야 하는 파일 수와 서로 다른 책임의 결합을 함께 검토한다. `common`, `utils`에 소유자를 알 수 없는 코드를 모으지 않는다. 공통화에는 실제 공유 의미와 사용자가 있어야 한다.

`XADB-CODE-MOD-001/003`의 배치 판단에서는 **공유되는 의미와 여러 기능을 조립하는 절차를 구분한다.** 동일한 의미·수명·오류 계약을 여러 소비자가 사용하면 필요한 하위 경계로 추출하고, 서로 다른 기능을 함께 사용하는 순서는 상위 조립 지점에 둔다. 예를 들어 WAL과 저장소를 연결하는 commit 순서는 기존 transaction 소유 책임이다. 두 모듈이 함께 사용한다고 foundation으로 옮기거나, 순환을 없애려고 모든 타입을 공통 계층에 모으지 않는다.

같은 불변식을 유지하기 위해 반드시 함께 변경하는 구현은 가까이 두고, 소비자·변경 이유가 다른 부가 기능은 분리 후보로 본다. 같은 타입을 사용한다거나 같은 사람이 작성했다는 이유만으로 묶지 않는다. 반대로 작은 타입과 guard를 별도 파일·crate로 떼어 탐색·조립 비용만 늘리지 않는다. 기존 계약 안에서 새로운 소비 기능을 추가했는데 여러 하위 모듈의 내부 수정까지 필요하다면 경계가 적절한지 검토한다.

작은 지역 helper를 재사용하기 위해 무거운 상위 의존성이나 의미가 다른 범용 API를 만들지 않는다. 먼저 기존 구현의 사용과 하위 추출을 비교하고, 그 비용이 더 크면 제한적인 중복을 허용할 수 있다. 코드 모양이 같아도 변경 이유·수명·오류 의미가 다르면 공통화 근거가 부족하다. 허용 시 범위·이유·대안·담당·재검토 조건을 코드 옆이나 작업 기록에 남기고 D1/D2 검토 강도는 [DP01/DP07](development-policy.md)을 따른다. ID·commit·영속 형식 등 하나의 의미를 유지해야 하는 계약은 복제하지 않는다. 독립 oracle에는 production 알고리즘을 복사하거나 정답 계산에 공유하지 않는다.

공개 범위는 private를 기본으로 하고 필요한 만큼만 `pub(super)`, `pub(in ...)`, `pub(crate)`, `pub`으로 넓힌다. 외부 API와 re-export는 명시적인 경계에서 제공한다. `pub(crate)`는 같은 crate 안의 계층 역참조를 자동으로 막아 주지 않으므로 내부 의존 방향도 리뷰한다. production 코드의 glob import는 명시 import를 우선하고, 좁은 테스트 모듈의 `use super::*` 등 문맥이 분명한 사용은 허용한다.

foundation과 향후 engine은 platform 구현을 직접 import하지 않는다. worker·CLI·server가 실제 구현을 선택해 계약으로 전달한다. 시험 controller의 OS 제어와 production engine의 OS 의존은 구분한다. 전체 feature·target·build·dev 의존 및 내부 모듈 경계의 자동 검사 확대는 저장소 정책의 후속 작업이다.

<a id="section-5"></a>

## 5. 타입과 함수의 계약

서로 바꾸어 쓰면 안 되는 ID·세대·순서는 별도 타입으로 표현한다. [현재 식별자 구현](../../crates/foundation/src/identity.rs)의 `ObjectId`, `RowId`, `Generation`, `SchemaEpoch`, `CommitSeq`를 기준 예로 삼는다. 논리 ID를 포인터로 해석하지 않고, 영속 형식을 임의 Rust struct 메모리 배치로 저장하지 않는다.

함수는 한 동작이나 명확한 상태 전이를 표현한다. `process_data`, `do_work`처럼 핵심 효과를 숨기는 이름은 구체화한다. 독립적인 의미를 가진 boolean 인자가 반복되면 enum이나 옵션 타입을 검토한다. 호출자가 이미 아는 문맥까지 매개변수와 이름에 중복시키지는 않는다.

`append`, `flush`, `sync`, `publish`, `commit`의 보장 범위를 계약에 쓴다. 이름에 `sync`가 있다고 전체 DB 내구성이 보장되거나, `Result::Err`라고 모든 효과가 취소되는 것은 아니다. 불변 상태나 준비 완료 상태를 타입으로 표현할 수 있으면 활용하되 필요 이상의 generic·trait·macro·상태 타입으로 동작을 분산시키지 않는다.

`XADB-CODE-API-001/002`의 핵심 API는 내부 불변식·수명·효과를 안전하게 다루는 데 필요한 동작을 제공한다. 공개 계약만으로 충분히 구현할 수 있는 표시·변환·편의 기능은 소비자에 가까운 상위 모듈 또는 같은 책임 영역의 보조 모듈에 둘 수 있다. 예를 들어 값의 공통 의미와 SQL 표시·CLI 출력 정책을 구분한다. 모든 보조 함수를 별도 crate로 옮기거나 `util` 접미어를 강제하지 않는다. 성능 때문에 내부 접근을 추가하려면 기존 경계로 구현하는 대안과 실제 측정 근거를 DP07에 따라 검토한다.

API의 최소성은 함수 개수나 인자 수로 판정하지 않는다. 원자성·lock 순서·실패 정리를 호출자에게 떠넘기는 분할은 피한다. commit의 내부 단계를 공개한 뒤 올바른 호출 순서를 기대하는 것으로 기존 원자성 계약을 대체하지 않는다. 현재 capability의 정상·오류·취소·종료 경로에 필요한 계약을 갖추고, 예상만 한 미래 기능을 완전성의 이름으로 미리 구현하지 않는다.

공개 API와 중요한 내부 경계의 Rustdoc에는 다음 중 해당하는 항목을 기록한다. 단순 private helper에 빈 템플릿을 붙이지 않는다.

| 항목 | 설명할 내용 |
| --- | --- |
| 사전조건 | 입력 범위, 호출 가능한 상태, 필요한 순서 |
| 성공 효과 | 변경·반환 내용과 보장의 범위 |
| 실패 효과 | 상태 유지·부분 변경·결과 불명, 호출자의 후속 의무 |
| 소유권·수명 | 소유자, 대여·pin의 유효 범위, 자원 반환 |
| 동시성 | lock 소유자와 순서, blocking·await 가능 여부 |
| 검증 연결 | 계약 문서와 관련 테스트 또는 미구현 시험 의무 |

Rustdoc의 `# Errors`, 필요한 `# Panics`, unsafe API의 `# Safety`는 표준 문서 관례를 따른다. 주석은 코드가 수행하는 문장을 반복하기보다 불변식과 선택 이유를 설명한다. 식별자는 영어를 사용하며, 설명은 팀이 유지보수할 수 있는 한국어·영어를 사용할 수 있다. 중복 번역을 의무화하지 않는다. [Rust API 문서 지침](https://rust-lang.github.io/api-guidelines/documentation.html)

<a id="section-6"></a>

## 6. 오류와 자원 수명

예상 가능한 사용자 입력·I/O·quota 오류는 typed `Result`로 반환한다. 이런 정상 오류 경로에는 `unwrap`·`expect`를 사용하지 않는다. 테스트 assertion과 빌드 스크립트의 필수 전제 검사는 제품의 복구 가능한 오류 처리와 구분한다. 내부 불변식 assertion에는 실패 후 엔진 재사용 정책이 필요하다.

오류는 code뿐 아니라 실패 후 효과, 결과의 확실성, 재시도 범위를 전달한다. [현재 오류 타입](../../crates/foundation/src/error.rs)의 `Effect`, `Certainty`, `RetryScope`가 기준 예이다. foundation의 오류가 transaction rollback을 수행했다고 주장하지 않는다. 쓰기·sync 결과가 불명이면 자동 재시도로 성공을 꾸미지 않으며 상위 소유자가 해당 계약에 따라 해결한다.

RAII와 guard로 소유 자원을 반환한다. `Drop`에 숨은 commit, 실패를 보고할 수 없는 필수 영속화, 새 allocation·panic을 넣지 않는다. 성공 여부를 알아야 하는 종료·영속 작업은 명시 API로 수행한다. `?`와 조기 반환은 허용하며, C/C++의 단일 exit·수동 정리 패턴을 Rust에 기계적으로 이식하지 않는다. 모든 실패 지점에서 상태와 자원 수명이 보존되는지 확인한다.

quota와 fallible allocation의 보장 범위를 구분한다. 일부 `try_reserve` 사용을 전체 OOM 복구 보장으로 표현하지 않는다. panic·poison 뒤 shared state 재사용, durable 이후 실패, UNKNOWN 판정은 [구현 계약](../contracts/implementation-contracts.md)의 정책을 따른다.

<a id="section-7"></a>

## 7. 동시성과 unsafe 경계

공유 상태의 mutation owner, lock 순서, snapshot 고정 시점, 공개 시점을 명시한다. lock 보유 중 blocking·await·callback·allocation을 검토한다. 특히 기존 committed root 공개 계약이 금지한 효과를 넣지 않는다. 전역 가변 singleton이나 숨은 설정 변경으로 의존 순환을 우회하지 않는다.

현재 workspace의 `unsafe_code = "forbid"`를 유지한다. 향후 예외는 구현 계약에 따라 명시 경계, safety 전제, 호출자 의무, 동시성 모델, 검사 방법과 필요한 성능 근거를 함께 정한다. safe wrapper 사용이 외부 의존성 내부에도 unsafe가 없다는 뜻은 아니다. Miri·sanitizer·fuzz·lock-order 도구는 적용 가능한 경계에 도입하며, 문서에 적었다는 이유로 현재 실행된 검사로 표시하지 않는다.

<a id="section-8"></a>

## 8. 서식과 자동 검사

고정 toolchain의 `rustfmt` 기본 스타일을 사용한다. 들여쓰기·괄호·줄바꿈은 formatter 결과를 따른다. lint 허용은 필요한 최소 범위에 이유를 기록하고, 경고를 없애기 위한 workspace 전체 `allow`를 추가하지 않는다. [Rust Style Guide](https://doc.rust-lang.org/style-guide/)

다음 명령은 현재 DEV0 검사에 포함된 진입점이다. 문서 작성 과정에서 실행한 제품 시험을 뜻하지 않는다.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

일상 변경에서는 관련 crate·case로 먼저 확인하고 단계별 전체 검사를 수행한다. DEV1 완료 검사는 `python3 scripts/check_dev1.py`이며 DEV0 회귀를 포함한다. `--all-features`는 fault·고의 결함을 포함한 self-test 조합이며 배포 설정이 아니다. default build와 각 단계의 실제 통합 시험을 별도로 유지한다.

현재 formatter·Clippy가 이름의 도메인 의미, 모든 visibility 설계, 모든 실패 원자성을 자동 판정하지는 않는다. 전체 준수 감사와 AI 효율 pilot은 미실행이다. 아래 색인의 검사 구분은 **검사 수단의 범위**이며 실행 성공 상태가 아니다.

<a id="section-9"></a>

## 9. 규칙 색인

`AUTO`는 현재 도구가 검사하는 기계적 범위, `PARTIAL`은 일부 자동 검사와 추가 리뷰·시험이 필요한 범위, `REVIEW_TEST`는 의미 리뷰와 해당 동작의 시험으로 확인하는 범위다. 규칙 ID는 리뷰에서 사용하고 번호를 다른 뜻으로 재사용하지 않는다. 본문이 규범 원문이며 JSON은 ID·검사 구분·참조의 색인이다.

| ID | 기준 | 검사 구분 |
| --- | --- | --- |
| XADB-CODE-NAME-001 | Rust 식별자·역할 폴더·패키지 구분 | PARTIAL |
| XADB-CODE-NAME-002 | 경로와 도메인·역할 이름, 범주 접두어 생략 | REVIEW_TEST |
| XADB-CODE-NAME-003 | 용어·약어·단위의 일관성과 개념 구분 | REVIEW_TEST |
| XADB-CODE-MOD-001 | 책임별 배치, 필요한 파일·crate만 분리 | REVIEW_TEST |
| XADB-CODE-MOD-002 | 최소 공개 범위와 명시 import·re-export | PARTIAL |
| XADB-CODE-MOD-003 | 계층 의존과 foundation·platform 조립 경계 | PARTIAL |
| XADB-CODE-TYPE-001 | 의미가 다른 ID·세대·순서의 타입 구분 | PARTIAL |
| XADB-CODE-API-001 | 명확한 동작·상태 전이·성공 효과 | REVIEW_TEST |
| XADB-CODE-API-002 | 중요 경계의 오류·수명·동시성 문서 | REVIEW_TEST |
| XADB-CODE-ERR-001 | 예상 오류의 typed Result와 panic 구분 | PARTIAL |
| XADB-CODE-ERR-002 | 실패 효과·확실성·재시도 범위 보존 | REVIEW_TEST |
| XADB-CODE-LIFE-001 | RAII 정리와 명시적 commit·영속 완료 | REVIEW_TEST |
| XADB-CODE-LIFE-002 | quota·allocation 실패·panic 보장 범위 | REVIEW_TEST |
| XADB-CODE-CONC-001 | mutation owner·lock 순서·공개 시점 | REVIEW_TEST |
| XADB-CODE-SAFE-001 | unsafe 금지와 명시 예외 경계 | PARTIAL |
| XADB-CODE-FMT-001 | 고정 rustfmt 스타일 | AUTO |
| XADB-CODE-LINT-001 | 고정 Clippy 진입점과 최소 범위 예외 | PARTIAL |
| XADB-CODE-TEST-001 | 독립 기대값과 조건·결과 중심 반례 | REVIEW_TEST |
| XADB-CODE-TEST-002 | 관련 실제 통합·실패·복구 시험과 증거 | REVIEW_TEST |
| XADB-CODE-DOC-001 | 설명의 단일 원문과 코드·계약 연결 | PARTIAL |
| XADB-CODE-WORK-001 | 한 계약의 구현·반례·검증 작업 단위 | REVIEW_TEST |
| XADB-CODE-CHANGE-001 | 의미 변경·이름 정리 구분과 버전 정책 | REVIEW_TEST |

예외가 필요하면 해당 규칙, 이유, 범위, 제품 검증, 재검토 조건을 코드 옆이나 설계 결정에 기록한다. 단순 이름 선택마다 승인을 추가하지 않는다. 의미·영속 형식·공개 계약을 바꾸는 예외에는 기존 설계 검토 절차를 적용한다.

<a id="section-10"></a>

## 10. 테스트와 리뷰

테스트 이름은 조건과 결과를 표현하고, 단계의 case ID와 실행 결과는 기존 XADBTest 계약에 연결한다. [foundation 계약 시험](../../crates/foundation/tests/contracts.rs)과 [platform 계약 시험](../../crates/platform/tests/contracts.rs)을 현재 예로 삼는다. 이름만 바꾸기 위해 기존 실행 기록의 테스트 식별자를 소급 변경하지 않는다.

기대값은 명세·손계산 vector·독립 reference 등 출처를 가진다. production codec이나 알고리즘을 그대로 정답 계산에 재사용하지 않는다. 구현 결과로 expected를 자동 교체해 통과시키지 않는다. 테스트 오류 수정은 가능하며 변경 근거와 기존 관측을 보존한다.

`XADB-CODE-TEST-001/002`의 모듈 시험은 해당 계약을 검증하는 데 필요한 최소 환경으로 실행한다. codec의 잘린 입력 검증에 SQL 서버를 요구하는 식으로 무관한 상위 시스템을 끌어들이지 않는다. 시험에서만 필요한 의존은 [저장소 정책의 의존성 대조 기준](repository-build-plan.md#dependency-review)에 따라 별도로 선언하며, 이를 표시하기 위해 제품 코드에 쓰지 않는 import나 상위 의존성을 추가하지 않는다. 계약별 시험과 경계를 연결하는 통합 시험의 책임·진입점을 구분하고 기존 Cargo·XADBTest 실행 방식을 재사용한다.

최소 환경은 검증하려는 효과를 제거한다는 뜻이 아니다. 파일 영속화·동시성·재시작이 계약에 포함되면 실제 I/O·thread·reopen과 필요한 통합 범위를 유지한다. 단일 시험 파일·드라이버로 제한하거나 하위 시험 성공을 완전한 검증의 증명으로 취급하지 않는다. 실패 주입을 위해 모든 내부 호출을 trait로 바꾸기보다 실제 효과 경계와 기존 주입 수단을 우선한다.

리뷰는 계약·정확성 → 실패 효과·자원 수명 → 동시성·복구 → 의존 경계 → 이름·문서 순서로 수행한다. 정상 결과뿐 아니라 영향을 받는 오류·취소·부분 I/O·재시작 경계를 확인한다. 작은 모듈 성공을 실제 통합·단계 완료로 승격하지 않는다. 문서 링크나 서식만 고친 경우 자동 검사는 [DOC-002](documentation-policy.md#push-gate)에 따라 push 직전에 수행하며 제품 시험을 실행했다고 기록하지 않는다.

<a id="section-11"></a>

## 11. 문서와 AI 작업 절차

루트 `AGENTS.md`는 문서·빌드·시험 진입점과 핵심 규칙만 제공한다. 이 문서는 코딩 기준의 원문이고, API 계약은 코드 옆 Rustdoc, 선택 이유는 설계 문서에 둔다. 모듈별 문서가 필요해지면 저장소 정책의 `devdoc/modules/` 아래 실제 구현과 함께 추가한다. JSON에는 자동 검사가 소비하는 ID·참조·상태만 구조화하고 긴 설명을 중복 복사하지 않는다.

AI와 사람 모두 다음 절차로 작업한다.

1. 기대 동작·현재 실패·변경 범위와 해당 규칙을 정하고 관련 계약·실제 API·시험을 읽는다.
2. 버그는 현재 build에서 반례를 확인한다. 신규 기능은 구현 전 명세와 독립 기대값을 준비한다. 재현되지 않은 실패는 만들어 내지 않는다.
3. 하나의 검증 가능한 동작을 production core에 구현한다. 예를 들어 부분 쓰기 후 UNKNOWN 보존과 자동 재시도 금지를 하나의 작업으로 묶는다.
4. 좁은 반례와 관련 실제 통합·회귀를 실행한다. 아직 없는 recovery 등 선행 미구현 기능은 미검증 의무로 남긴다.
5. 동작·계약 변경과 함께 관련 Markdown·JSON·문서 색인을 갱신한다. 소스·case·binary·실행 결과의 identity를 보존한다.
6. 변경 효과, 실행한 검사와 결과, 남은 의무를 리뷰 자료에 기록한다. 설명의 완성도나 모델의 자기평가로 PASS를 만들지 않는다.

서식 정리와 의미 변경을 리뷰에서 구별한다. 제한된 수정의 범위를 저장소 전체 rename으로 넓히지 않는다. 사내 형식 변경에 불필요한 호환 adapter를 추가하지 않으며 안정 릴리스 계약의 변경은 버전 정책을 따른다. 상세 oracle 신뢰 경계와 결과 분류는 [AI 개발 방법](ai-development-method.md)에 둔다.

<a id="section-12"></a>

## 12. 적용과 개선

DEV1의 ID·오류·effect port·계약 시험을 실제 기준 예로 사용하고, DEV2부터 새 경계에 이 규칙을 적용한다. 기존 코드는 변경 범위에서 정리하되 대규모 정리가 필요하면 별도 작업으로 범위와 검증을 정한다. 새 실행 소스 루트를 추가할 때는 저장소 정책에 따라 source fingerprint와 clean-export inventory를 갱신한다.

효과는 같은 task·revision·도구 조건에서 관련 코드 탐색 시간, 새 세션 재현 시간, 누락한 계약, 재작업, 통합 유출 결함으로 평가한다. 줄 수·규칙 수·AI 토큰 수만으로 품질을 판정하지 않는다. 비교 방법은 기존 [AI pilot](ai-development-method.md)을 따르며 이 문서 자체가 효율 향상을 입증하지는 않는다.

기존 문서와의 정합성은 다음과 같이 유지한다.

- 종합 설계의 초기 역할 이름은 현재 crate·내부 모듈 배치와 구분한다. `xadb-runtime` 역할의 공통 계약과 OS 구현은 각각 foundation·platform에 대응한다.
- API 변경의 호환 검사는 안정 릴리스 기준선부터 적용한다. 사내 현재 build의 정확성·복구 검증은 계속 요구한다.
- formatter·lint·일부 의존 검사는 현재 구현이며, 전체 visibility·동시성·복구 자동 검사와 AI pilot의 완료를 암시하지 않는다.

문서 검증기는 규칙 ID·검사 구분의 Markdown/JSON 일치, 연결된 현재 파일, Markdown 지문을 검사한다. Rust 구현의 컨벤션 준수나 제품 정확성을 판정하는 검사가 아니다.

<a id="selective-design-reference"></a>

<a id="section-13"></a>

## 13. 대규모 C++에서 선별한 설계 판단 기준

존 레이코스의 『대규모 C++ I편: 절차와 아키텍처 [23주년 개정판]』에서 아래 원칙을 선별했다. 원문은 사용자가 제공한 `대규모C++-1.epub`이며 판본 식별 hash와 EPUB 내 절 위치는 [규칙 JSON](coding-convention.json)의 `design_reference`에 보존한다. 본문 전체나 코드를 복제하지 않고 관련 절의 취지를 요약했다. 이 책은 C++ 대규모 라이브러리 설계의 참고 자료이며 Rust 규범이나 AI 효율 실험 결과가 아니다.

| 원문 절 | 참고한 관점 | XADB 적용 위치와 제한 |
| --- | --- | --- |
| 2.2.14–15, 1.11 | 허용 의존성을 먼저 정하고 실제 의존성과 대조 | 저장소 정책 4.2절. 관측 그래프로 허용 목록을 자동 갱신하지 않음 |
| 3.5.2–3 | 공통 의미의 하위 추출과 조립 절차의 상위 배치를 구분 | 이 문서 4절. foundation 집중과 불필요한 crate 분리 방지 |
| 3.2.5–9, 3.3.1–2, 3.12.12 | 핵심 동작·부가 기능·함께 변경하는 책임을 구분 | 이 문서 4–5절. 원자성 유지, 실제 capability 우선, 탐색 비용 고려 |
| 2.14.3–7 | 시험 의존성과 재현 환경도 설계 책임 | 이 문서 10절. 시험 전용 제품 의존 추가 배제, 실제 통합·복구 시험 유지 |
| 3.5.6 | 작은 중복과 큰 의존성의 비용 비교 | 이 문서 4절. 단일 의미 계약과 독립 oracle의 신뢰 경계 유지 |

클래스당 컴포넌트 하나, `.h/.cpp/.t.cpp` 구성, 고정된 패키지 계층 수·접두어는 XADB 규칙으로 채택하지 않는다. 3.11절의 캡슐화와 재컴파일 격리 구분은 변경 비용을 보는 관점으로 참고하되, Rust에서 trait·wrapper를 쓰면 재빌드가 없어지거나 성능이 좋아진다고 가정하지 않는다. 3.7.4절의 누적 컴포넌트 의존성(CCD)은 의존 부담을 보는 참고 지표이며 일률적 합격 기준이 아니다. 0.8절의 동적 계획법 비유를 개발 기간·AI 생산성 향상률의 증거로 사용하지 않는다. 라이브러리 안정성 논의도 사내 형식의 호환 비요구 정책을 변경하지 않는다.

AI가 읽고 재현해야 하는 범위를 줄일 수 있다는 기대는 XADB의 적용 가설이다. [기존 AI pilot](ai-development-method.md)의 품질 조건과 실제 탐색·재현·재작업 비용으로 평가한다. 책의 권고를 만족시키기 위한 전체 재배치나 새로운 자동화 플랫폼은 요구하지 않는다.

### 적용 결정 XADB-METHOD-001

| 항목 | 기록 |
| --- | --- |
| 상태·분류·일자 | ACCEPTED · D2 · 2026-10-18 |
| 담당·통합·계약 확인 | `/root` — 코딩·저장소·AI 작업 절차의 기존 계약과 대조 |
| 권한 근거 | 사용자의 비판적 검토 요청과 후속 “우리 문서에 보강해 주세요”, DP01의 D2 위임 |
| 범위 | 기존 MOD-001/003, API-001/002, TEST-001/002, WORK-001 규칙의 판단 기준 보강; 개발 순서·커널 의미·호환·필수 품질 기준 유지 |
| 비교 대안·선택 이유 | 책의 규칙 전체 도입은 언어·구성·검증 비용 불일치, 일반 요약만 보존하면 실행 기준 부족. 기존 소유 문서에 선별 적용하여 중복 규범을 줄임 |
| 관련 계약·검증 | 저장소 정책 4.2절, AI 작업 절차 2절, DP07. V0 문서 검사 적용; 제품 시험·확장 의존 검사·AI pilot 실행 증거는 별도 |
| 별도 검토 | `/root/book_policy_review` · 2026-10-18 · 원문 절·EPUB identity·기존 계약·이번 Markdown/JSON 변경·문서 검증 결과를 별도 실행으로 대조. 차단 결함·필수 수정 권고 없음 |
| 대체 관계 | 기존 결정을 대체하지 않는 추가 구체화. 기존 규칙 ID 유지 |
| 재검토 조건 | 분할·공통화 때문에 탐색·재현·통합 비용이 늘거나, 의미 중복·의존 위반·유출 결함이 발생할 때 |

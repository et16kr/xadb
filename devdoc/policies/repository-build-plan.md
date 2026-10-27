# XADB — 저장소 구성과 빌드 정책

2026-10-18 · **배치 정책 채택 / DEV0·DEV1 구현 / DEV2 부분 구현·진행 중**


<a id="section-1"></a>

## 1. 결정과 적용 범위

하나의 Cargo workspace에서 제품 코드와 XADBTest를 개발하고, 모든 설계 문서는 `devdoc/`에서 관리한다. 현재 9개 crate를 사용하며 DEV1의 공통 계약과 OS 구현을 `xadb-foundation`·`xadb-platform`으로 분리한다. DEV2의 독립 oracle `xadbtest-reference`를 먼저 추가했다. `xadb-engine`은 K0 volatile substrate를 구현 중이며 `xadbtest-engine-worker`는 native adapter를 구현 중이다. [진행 기록](../implementation/dev2-implementation.md)을 따른다. 거래·저장·catalog·WAL/recovery는 우선 `xadb-engine`의 내부 모듈로 나누고 공개 경계를 검증한 뒤 필요할 때 crate를 분리한다.

이 문서는 사용자 요청에 따라 기존 경로와 변경된 XADB 설계를 연결한 **저장 위치·빌드 책임 결정**이다. K0–K5, DEV0–DEV9/COMM/PART/NG 순서와 kernel·recovery·버전 의미는 바꾸지 않는다. 후속 crate 이름은 구현 단계에서 재검토할 예약 방향이며, 빈 crate나 placeholder 폴더를 미리 만들지 않는다. 경로가 계획에 있다는 이유로 구현·시험·배포 완료로 표시하지 않는다.

우선순위는 [하위 계층 개발 결정](../plans/bottom-up-development.md), [단계 JSON](../plans/development-stages.json), [Rust 구현 계약](../contracts/implementation-contracts.md), [버전 정책](version-compatibility-policy.json)이다. DEV0/DEV1의 실제 명령·제약은 각각의 구현 문서와 manifest를 따른다. 이름·API 문서·리뷰는 [코딩 컨벤션](coding-convention.md)을 따른다. 종합 보고서 5.2절도 이 문서의 실제 초기 배치를 사용한다. 초기 제안의 `xadb-types/xadb-txn/xadb-storage/xadb-durable/...`는 역할 이름이었으며 독립 Cargo package 예약이 아니다.

제품 폴더는 `crates/foundation`, `crates/platform`, 후속 `crates/engine`처럼 **제품명 접두어 없이 역할로 명명**한다. Cargo package 이름은 별도 식별자로 `xadb-foundation`, `xadb-platform`, `xadb-engine` 등을 유지한다. Rust library target과 dependency alias는 제품명 없이 책임 이름을 사용한다. 정확한 대응과 2026-10-25 변경 범위는 [명칭 정리 기록](../evidence/naming-cleanup-20261025.md)을 따른다. `cargo -p`에는 package 이름을, workspace member와 path dependency에는 실제 경로를 쓴다. 독립 시험 도구의 패키지·명령 이름인 `xadbtest` 계열은 유지하고, 폴더는 `crates/testing` 아래 역할별로 묶는다. XADB package·도구 이름으로 이관하고 새 소스 fingerprint와 실행 증거를 생성한다.

<a id="testing-layout"></a>

### XADB-LAYOUT-TESTING-001 — 시험 도구 폴더 재구성

2026-10-18 · **ACCEPTED / D2**. 사용자가 시험 패키지를 한 분류 폴더로 묶고 관련 문서를 갱신하도록 요청했다. 기존의 `xadbtest*` 폴더 유지 예외를 이 결정으로 대체한다. 제품 폴더는 기존 역할 이름을 사용하며, 시험 패키지는 아래 4절의 `crates/testing/<역할>`에 둔다. `controller`의 Cargo 이름은 `xadbtest`이고 나머지 패키지·실행 파일 이름도 유지한다.

평면 배치를 유지하는 대안과 시험 코드를 단일 패키지로 합치는 대안을 비교했다. 선택한 배치는 제품과 시험 도구의 탐색 위치를 구분하고 폴더 접두어 반복을 줄이면서 기존 9개 패키지·target·feature·의존 경계를 보존한다. `testing`에는 `Cargo.toml`을 추가하지 않고 루트 workspace가 여섯 시험 패키지를 직접 등록한다. 폴더 계층은 Rust 모듈 또는 의존 관계를 뜻하지 않는다. reference는 계속 production 의미 구현에 의존하지 않는다.

작업·계약·통합 담당은 `/root`다. workspace 경로, 상대 dependency·fixture 경로, controller의 workspace-root 계산, 검증 스크립트의 source 경로를 함께 갱신한다. fingerprint와 clean export는 기존 `crates` 루트를 재귀 순회하므로 포함 루트는 유지하며 실제 새 빌드로 확인한다. 과거 실행 보고서와 이관 inventory는 당시 경로·hash를 보존한다. 변경·검토·실행 근거는 [재구성 기록](../history/project-migration.md#testing-layout)에서 관리한다. 패키지 역할 증가나 탐색·빌드 경계 혼동이 생기면 재검토한다.

<a id="section-2"></a>

## 2. 배치의 책임

공통 의미와 OS 효과, kernel과 SQL, production과 독립 oracle를 구분한다. 저장·거래·로그·복구는 초기에는 하나의 engine crate 안의 소유 모듈로 연결하고 실제 독립 경계가 생길 때 분리한다.

<a id="section-3"></a>

## 3. 루트와 폴더별 책임

`EXISTING`은 실제 존재, `GENERATED`는 필요할 때 생성되는 Git 제외 산출물, `PLANNED`는 미래 배치다. 현재 문서는 [DOC-001](documentation-policy.md#doc-001)에 따라 아래 역할별 폴더로 재구성되어 있다. 역사적 증거의 원래 source 경로·revision·hash는 보존하며 현재 경로로 소급 변경하지 않는다. 이동 내역은 [이관 기록](../document-migration.json)에 둔다.

| 경로 | 상태 | 책임 |
| --- | --- | --- |
| crates | EXISTING | Rust 제품 라이브러리·실행 파일과 XADBTest 패키지 |
| crates/testing | EXISTING | XADBTest 패키지의 분류 디렉터리. 자체 Cargo 패키지·workspace는 만들지 않음 |
| scripts | EXISTING | 빌드·시험 조립 자동화. 제품 DB 의미는 두지 않음 |
| tests/cases | EXISTING | version을 명시한 typed 시나리오와 독립 기대값 |
| devdoc | EXISTING | 설계·명세·계약·저장된 검증 증거의 단일 문서 루트 |
| devdoc/policies | EXISTING | 상시 적용하는 개발 규칙과 문서 운영 기준 |
| devdoc/plans | EXISTING | 제품 범위·개발 순서·작성 목록·단계 완료 |
| devdoc/contracts | EXISTING | 기능별 의미·불변식·인수 계약과 미실행 시험 계획 |
| devdoc/implementation | EXISTING | 현재 구현 범위·제약·시험 진입점 |
| devdoc/reference | EXISTING | 필요한 절만 조회하는 종합 설계·매뉴얼 초안·상세 색인 |
| devdoc/examples | EXISTING | 실행·승인이 아닌 작업 계약과 진단 예시 |
| devdoc/evidence | EXISTING | 원본 identity를 보존하는 조사·모델·실행 증거 |
| devdoc/history | EXISTING | 현재 규범과 구분하는 기존 제품 조사 보고서와 근거 |
| .github/workflows | EXISTING | 로컬과 같은 검사 진입점을 호출하는 CI |
| target | GENERATED | Cargo 빌드 산출물. Git 제외 |
| artifacts | GENERATED | 소유 run의 로그·보존 binary·증거 bundle. Git 제외 |
| devdoc/modules | PLANNED | 모듈 책임·API·소유권·오류·시험 진입 문서 |
| devdoc/specs | PLANNED | SQL·값·내구성·format 명세 |
| devdoc/adr | PLANNED | 대안·전제·재검토 조건을 담는 설계 결정 |
| tests/corpus | PLANNED | 독립적으로 검토한 최소 반례·known-bad history |
| tests/schedules | PLANNED | actor·barrier 실행 순서 시나리오 |
| tests/durability | PLANNED | crash cut·파일 fault 시나리오 |
| tests/sql | PLANNED | SQL 입력과 typed 독립 기대값 |
| tests/versions | PLANNED | 안정 릴리스 기준선이 생긴 뒤 same-major fixture |
| benches | PLANNED | workload·데이터 생성기·비교 기준 |
| clients/jdbc | PLANNED | DEV6 비 Rust JDBC 소스와 빌드 |
| packaging | PLANNED | 사내 배포 준비 시 recipe·manifest |

실행 가능한 Rust 타입·불변식 검사는 해당 crate의 `src/`에 둔다. `devdoc/contracts/`의 JSON은 설계·검증 계약이며 제품이 실행 중 읽는 전역 registry가 아니다. 여러 언어가 소비할 protocol schema/generator가 필요해지면 소유 crate에 입력·generator 버전·생성 산출물 위치를 정하고 별도 의존 검사를 추가한다.

기존 제안의 루트 `specs/`, `adr/`, `docs/modules/`, 설계용 `contracts/`는 위 `devdoc/` 하위로 정리한다. `tools/ax`·`tools/execution` 역할은 우선 Rust CLI/XADBTest crate와 `scripts/`가 맡는다. 모듈 문서는 책임·공개 API·수명·오류·시험 진입점과 관련 결정만 담고 실행 로그는 `artifacts/`에 둔다.

빌드 산출물 `target/`과 증거 `artifacts/`를 입력 fixture로 자동 승격하지 않는다. 의미 있는 최소 반례는 검토 후 `tests/corpus/`에 넣고, 고정 계약 vector는 crate의 `tests/`에 둘 수 있다. DB·WAL·backup의 실제 사용 경로는 호출자가 명시적으로 소유한 외부/시험 임시 경로로 받는다. source, build 출력, 설치 prefix를 기본 DB 경로로 섞지 않는다.

<a id="section-4"></a>

## 4. Cargo 패키지와 허용 의존 방향

다음 표의 의존은 **직접 normal workspace dependency**다. 외부 dependency는 lockfile로 관리한다. `IMPLEMENTED`의 목록은 현재 Cargo manifest와 일치해야 한다. `PLANNED`의 목록은 최초 구현의 경계이며 실제 구현 시 feature/target/build/dev 의존까지 검토한다.

| package | 소스 경로 | 상태 | 도입 단계 | 직접 workspace 의존 |
| --- | --- | --- | --- | --- |
| xadbtest-protocol | crates/testing/protocol | IMPLEMENTED | DEV0 | 없음 |
| xadbtest | crates/testing/controller | IMPLEMENTED | DEV0 | xadbtest-protocol, xadbtest-reference |
| xadbtest-fixture-worker | crates/testing/fixture-worker | IMPLEMENTED | DEV0 | xadbtest-protocol |
| xadb-foundation | crates/foundation | IMPLEMENTED | DEV1 | 없음 |
| xadb-platform | crates/platform | IMPLEMENTED | DEV1 | xadb-foundation |
| xadbtest-foundation-worker | crates/testing/foundation-worker | IMPLEMENTED | DEV1 | xadb-foundation, xadbtest-protocol |
| xadb-engine | crates/engine | IMPLEMENTED | DEV2 | xadb-foundation |
| xadbtest-reference | crates/testing/reference | IMPLEMENTED | DEV2 | xadbtest-protocol |
| xadbtest-engine-worker | crates/testing/engine-worker | IMPLEMENTED | DEV2 | xadb-engine, xadb-foundation, xadb-platform, xadbtest-protocol |
| xadb-transport | crates/transport | IMPLEMENTED | COMM | xadb-foundation |
| xadbtest-transport-worker | crates/testing/transport-worker | IMPLEMENTED | COMM | xadb-foundation, xadb-platform, xadb-transport |
| xadb-sql | crates/sql | IMPLEMENTED (raw 문장·scalar·typed predicate) | DEV4 | xadb-foundation |
| xadb-cli | crates/cli | PLANNED | DEV4 | xadb-foundation, xadb-sql, xadb-platform |
| xadb-server | crates/server | PLANNED | DEV5 | xadb-foundation, xadb-engine, xadb-sql, xadb-transport, xadb-platform |
| xadb-client | crates/client | PLANNED | DEV6 | xadb-foundation, xadb-transport |
| xadb-odbc | crates/odbc | PLANNED | DEV6 | xadb-client |
| xadb-replication | crates/replication | PLANNED | DEV8 | xadb-foundation, xadb-engine, xadb-transport |

```text
xadb-server ──→ xadb-sql ──→ xadb-engine ──→ xadb-foundation
    └───────→ xadb-transport ─────────→ xadb-foundation
xadb-platform ─────────────────────→ xadb-foundation
xadb-server / xadb-cli / xadbtest-engine-worker ──→ xadb-platform [OS 구현 조립]
xadb-cli ─────→ xadb-sql
xadb-client ──→ xadb-transport
xadb-replication ──→ xadb-engine / xadb-transport

xadbtest controller ──(process protocol)──→ xadbtest-engine-worker ──→ xadb-engine
    └──→ xadbtest-reference ──→ xadbtest-protocol    [DEV2 K0 catalog recipe 연결]
```

worker 실행 관계는 controller의 engine import를 뜻하지 않는다. `xadbtest`는 K0 catalog recipe의 외부 이력 판정을 위해 `xadbtest-reference`에 normal dependency로 연결한다. worker의 `xadbtest` 의존은 통합 시험용 dev-dependency이며 normal 실행에는 포함하지 않는다. reference는 자체 row/history 모델과 기대값 계산을 사용한다. protocol의 단순 입력/관측 타입은 공유할 수 있지만 `xadb-foundation`의 production codec·비교·commit 알고리즘을 정답 계산에 재사용하지 않는다. known-bad history 거절은 별도 필수 시험이다.

`xadb-engine`은 SQL·server·transport·client·RP 구현·XADBTest를 import하지 않는다. replication을 위한 commit stream/retention port는 engine 쪽에서 정의하고 상위 consumer가 구현한다. `xadb-foundation`은 다른 workspace 계층에 의존하지 않는다. 하위 crate를 단독 build/test할 수 있어야 하며 `pub(crate)` 내부 타입을 외부 consumer에 노출하지 않는다.

현재 자동 검사는 DEV1의 foundation/platform normal dependency closure·각각의 독립 build와 문서 검증기의 실제 manifest/직접 의존 대조다. 전체 제품의 모든 feature/target/build/dev 의존 closure 검사나 Rust 내부 visibility 검사가 구현되었다는 뜻은 아니다. 해당 검사는 DEV2 이후 checker에 추가한다. default build는 고의 결함 worker·fault 주입을 제외하고, `--all-features`는 self-test 검증용으로만 사용한다.

<a id="section-5"></a>

## 4.1 공통 계약과 플랫폼 구현의 분리

사용자는 기존 `core/pd` 명칭에 얽매이지 않고 XADB에 맞는 역할 분리를 선택했다. **현재 제품은 Linux만 구현·검증하고, 회사의 지원 결정에 따라 다른 OS 구현을 추가할 경계를 유지한다.** `xadb-foundation`은 ID·값·오류·codec·quota·수명과 `FileIo`/`Clock`/`Scheduler` 계약을 소유하고, `xadb-platform`은 Linux 파일 I/O·directory sync와 실제 시간·yield 구현을 제공한다. Rust 표준 자료구조·mutex·atomic을 다시 포장하는 범용 라이브러리는 만들지 않는다.

의존 방향은 `xadb-platform → xadb-foundation`이다. 공통 계층과 향후 `xadb-engine`은 `xadb-platform`을 import하지 않는다. worker·embedded CLI·server가 실제 OS 구현을 선택하고 공통 trait로 엔진에 전달한다. Linux 전용 `nix`/`std::os` 호출과 미지원 OS의 compile error는 platform에 둔다. foundation에서 Linux 제한을 제거한 것은 다른 OS에서 빌드·실행 검증을 완료했다는 뜻이 아니다. 현재 실제 시험 target은 `x86_64-unknown-linux-gnu`다.

시험용 `FaultFile`·`ManualClock`·`ScriptedScheduler`는 foundation의 `test-support`에 남으며 실제 OS adapter와 합성해 시험한다. Linux I/O/clock integration 시험은 platform의 `tests/contracts.rs`에 둔다. platform의 `test-support`는 foundation feature를 전달하며 default build에는 주입 기능이 없다. XADBTest controller의 프로세스 소유·회수 코드는 별도의 시험 인프라로 유지한다.

현재 분리는 DEV1에 이미 존재하던 구현만 옮긴 것이다. 파일 생성/open·writer 배제·rename/publication의 공통 경계와 recovery 연결은 DEV2에서, transport·대기/취소 경계는 COMM에서 실제 사용 계약과 함께 정의한다. WAL/페이지/SQL/오류 의미나 개발 순서를 이번 분리로 변경하지 않는다. 서버 OS 지원과 client OS 지원표는 별개이며 공통 값·프로토콜 코드가 Linux 서버 구현을 import하지 않게 한다.

[COMM 상세 설계](../contracts/comm-design.md#architecture)는 foundation의 byte-I/O/readiness port, platform의 Linux socket adapter, transport의 framing·queue·deadline 책임을 구체화한다. 실제 port·adapter·transport와 `crates/testing/transport-worker`를 구현했다. native 통합 시험은 transport의 dev-dependency로 platform을 연결한다. fingerprint/export roots는 기존 crates/scripts 안에서 유지하며 [구현·인수 범위](../implementation/comm-implementation.md)를 따른다.

새 OS에는 빈 성공 stub을 두지 않는다. 사업상 지원 결정 후 OS/CPU/filesystem capability, 영속화·잠금·오류·취소 의미를 검토하고 adapter·실제 회귀/복구·패키징·CI 증거를 추가한다. 공통 계약을 만족하지 못하면 계약 변경을 별도로 검토한다. 컴파일 성공이나 이전 Linux PASS를 새 OS의 지원 증거로 승계하지 않는다.

<a id="dependency-review"></a>

<a id="section-6"></a>

## 4.2 허용 의존성과 실제 의존성의 대조

2026-10-22 DEV3 예약·취소 native 시험은 `xadb-engine`의 `dev-dependencies`에 기존 로컬 `xadb-platform`을 연결한다. 실제 Linux namespace/FileIo를 시험 전용 adapter로 조립하기 위한 의존이며 제품의 normal/build 의존은 `xadb-foundation`만 유지한다. 새 외부 패키지·버전·feature는 추가하지 않는다. [할당 자원·시험 기록](../evidence/dev3-allocation-resource-review-20261022.md)과 관리 JSON의 전용 source/checker/fixture 목록을 따른다.

소유 계약이 정한 **허용 의존성**과 manifest·소스·빌드에서 확인한 **실제 의존성**을 분리한다. 실제 그래프에 새 경로가 생겼다는 이유로 허용 목록을 자동 갱신하지 않는다. 위반은 구현을 고치거나, 필요한 계약 변경을 D2로 검토한 뒤 목록과 근거를 함께 수정한다. 외부 crate를 새로 채택하는 경우에는 DP04도 적용한다. 이 기준의 참고 근거는 [XADB-METHOD-001](coding-convention.md#selective-design-reference)이다.

새 경계를 구현하거나 의존성을 바꿀 때는 다음을 기존 작업 기록의 해당 범위에 남긴다. 단순 helper마다 별도 manifest를 만들지 않는다.

| 대조 항목 | 기록할 내용 |
| --- | --- |
| 의도한 경계 | 소유 계약, 허용·금지 방향, 새 의존이 필요한 이유 |
| 실제 제품 의존 | source revision, target·feature·profile과 normal/build 경로, 새 직접·전이 의존 |
| 시험 의존 | dev/test-support·fixture·runner가 추가하는 경로와 자원, 모듈 시험과 통합 시험의 구분 |
| 확인 범위 | 자동 검사·소스 리뷰·실제 독립 build/test의 범위, 미확인 생성 코드·조건부 경로와 후속 의무 |

제품·build·시험 경로는 역할별 허용 기준과 대조한다. 모든 간선을 한 그래프로 합쳐 시험이 제품을 사용하는 관계 자체를 제품 계층 순환으로 판단하지 않는다. 반대로 dev 의존이라는 이유로 하위 모듈의 시험에 SQL/server 기동을 무제한 허용하지 않는다. 모듈 계약을 직접 검증할 수 있는 범위를 먼저 확보하고, 여러 계층과 실제 OS 효과가 필요한 통합 시험을 별도로 유지한다. 시험 전용 필요를 표시하려고 제품 코드에 불필요한 import나 dependency를 넣지 않는다. reference의 독립 기대값 계산에 production 의미를 가져오지 않는 기존 제한도 유지한다.

현재 자동 검사 범위는 4절에 기록한 DEV1 normal dependency 검사와 문서의 manifest 대조다. 위의 전체 target·feature·build·dev 경로 및 engine 내부 경계 자동 검사는 후속 구현이며 `NOT_RUN`이다. 구현한 capability부터 범위를 명시해 확장하고, 미확인 경로를 PASS로 기록하지 않는다. 전역 symbol index나 미래 단계 전체 검사기의 완성을 DEV2 K0의 새 선행 조건으로 만들지 않는다.

<a id="section-7"></a>

## 5. DEV2 커널 내부 배치와 후속 저장 확장

아래는 모듈 소유 경로다. Rust 파일은 작은 경우 `name.rs`, 확장 시 `name/mod.rs`와 하위 파일을 사용한다. api/catalog/storage::volatile는 현재 `.rs`로 K0 substrate를 구현한다. 나머지는 계획이며 아래 논리 경로의 디렉터리를 미리 만들지 않는다. task는 해당 책임을 도입하는 작업이며, 단계 완료는 관련 모든 작업과 인수 시험을 요구한다.

| 모듈 경로 | 도입 단계 | 도입 task | 책임 |
| --- | --- | --- | --- |
| crates/engine/src/api | K0 | XADB-027 | 공개 typed engine API. 저장소 내부 표현 은닉 |
| crates/engine/src/catalog | K0 | XADB-027 | SQL 없는 객체·schema metadata와 수명 |
| crates/engine/src/storage/volatile | K0 | XADB-027 | 실제 최소 행 저장소. DEV3에서 디스크 페이지·buffer·index로 확장 |
| crates/engine/src/transaction | K1 | XADB-006 | private batch·snapshot·rollback·commit 상태. K4에서 동시성 확장 |
| crates/engine/src/publication | K1 | XADB-006 | 단일 committed view와 reader pin. K2에서 영속 순서 연결 |
| crates/engine/src/durable/wal | K2 | XADB-007 | 공통 batch framing·append·sync |
| crates/engine/src/durable/recovery | K3 | XADB-008 | 완결 committed batch replay와 reopen |
| crates/engine/src/durable/checkpoint | K5 | XADB-009 | generation checkpoint·restore·공통 보존 |
| crates/engine/src/storage/disk | DEV3 | XADB-021 | in-place page·allocator·dirty buffer·WAL 연계 물리 I/O |
| crates/engine/src/storage/page.rs | DEV3 | XADB-028 | 구현된 내부 page envelope codec. 실제 I/O·cell 의미와 분리된 표현 leaf |
| crates/engine/src/storage/key.rs | DEV3 | XADB-028 | 구현된 내부 composite/physical key codec·비교. scalar bytes는 foundation codec 단일 구현, index I/O·unique 실행 제외 |
| crates/engine/src/storage/anchor.rs | DEV3 | XADB-028 | 안정 root descriptor와 root envelope 대조. 실제 tree/SMO·pin 제외 |
| crates/engine/src/storage/undo_ref.rs | DEV3 | XADB-028 | PageRef·slot/generation의 UndoRef 표현. 실제 undo record·수명/회수 제외 |
| crates/engine/src/storage/tree_page.rs | DEV3 | XADB-028 | 공통 tree cell·정렬·routing·byte 분할 표현. 실제 tree 변경·SMO 제외 |
| crates/engine/src/storage/overflow.rs | DEV3 | XADB-028 | row overflow chunk와 bounded streaming 검산. 실제 할당·I/O·회수 제외 |
| crates/engine/src/storage/transaction_record.rs | DEV3 | XADB-028 | 거래 identity·논리 행 버전·status cell 표현. 실제 directory·상태 전이·복구 제외 |
| crates/engine/src/storage/wal_frame.rs | DEV3 | XADB-028 | WAL frame·page redo 표현. 실제 I/O·변경 효과 제외 |
| crates/engine/src/storage/allocation_page.rs | DEV3 | XADB-028 | extent 할당 표현. 실제 I/O·변경 효과 제외 |
| crates/engine/src/storage/row_record.rs | DEV3 | XADB-028 | 행·인덱스 버전 표현. 실제 I/O·변경 효과 제외 |
| crates/engine/src/storage/doublewrite_control.rs | DEV3 | XADB-028 | selected doublewrite control 표현. 실제 I/O·변경 효과 제외 |
| crates/engine/src/storage/locator | DEV3 | XADB-021 | snapshot별 RowUID 해석·상한 있는 DISK mapping |
| crates/engine/src/storage/index | DEV3 | XADB-028 | native key·RowUID·cursor와 공통 transaction |
| crates/engine/src/storage/partition | PART | XADB-030 | membership·routing·local index·원자적 행 이동 |
| crates/engine/src/storage/index/global | NG | XADB-023 | 디스크 저장 경로를 포괄하는 native global tree. SQL 숨김 table 제외 |

K0의 row/catalog는 작은 실제 production 구현이다. XADB-004 reference와 XADB-032 native history suite는 별도이고, production row 저장소를 oracle로 사용하지 않는다. K4의 XADB-010은 `transaction`/`publication`의 동시성·statement 효과를 확장한다. DEV3의 XADB-022는 `durable`·DISK를 동일 commit/recovery로 연결하며 XADB-029가 실제 DISK row/index와 유한 저장 기준선을 마무리한다.

2026-10-18 K1–K5 소스 연결에서 `database.rs`·`database/maintenance.rs`, `transaction.rs`·`transaction/`, `publication.rs`, `durable/{codec,changes,wal,recovery,checkpoint,retention,backup,observation}.rs`를 실제 구현했다. namespace port와 Linux 구현은 foundation/platform에 두며 durable worker는 기존 engine-worker 패키지의 별도 `xadbtest-durable-worker` binary다. 새 production crate나 engine→platform/test 간선은 만들지 않는다. 이 경로의 `IMPLEMENTED`는 소스 존재·연결을 뜻하며 현재 빌드·실행 검증은 NOT_RUN이고 DEV2 완료를 뜻하지 않는다. 사용자 지시에 따라 K1–K5 전체 구현 연결 전 실행은 보류했다. 상세 소유·예산·정적 검토는 [DEV2 구현 기록](../implementation/dev2-implementation.md)을 따른다.

`transaction`이 준비·validation·WAL 완료·공개의 순서를 소유한다. `storage`는 명시된 변경/읽기 계약, `durable`은 versioned batch·영속 효과/복원 계약, `catalog`는 metadata 상태를 소유한다. 이들을 합성하는 지점이 engine이며 서로의 manager나 private 물리 포인터에 무제한 접근하는 구조를 허용하지 않는다. 이 역할은 각각 `transaction`, `durable`, `storage` 내부 모듈로 두며, 후속 crate 분리에서도 순환 의존을 전역 singleton으로 우회하지 않는다.

DEV3 첫 표현 leaf는 [page 형식](../contracts/dev3-page-format.md)을 private storage 모듈에 둔다. 기존 foundation의 ID/digest를 소비하며 dependency를 추가하지 않는다. 독립 vectors는 `scripts/dev3_page_vectors.py`와 engine 시험 fixture, snapshot 부분 모델은 reference 시험에 둔다. `scripts/check_dev3_leaf.py --with-regressions`는 보존 소스의 실제 codec·부분 모델과 하위 회귀를 연결하며 전체 DEV3 실행기로 사용하지 않는다. 기존 fingerprint/clean-export roots를 그대로 사용한다.

[DEV3 metadata port](../contracts/dev3-page-io.md)의 실제 Linux 검증은 `crates/platform/tests/namespace_metadata.rs`에 둔다. FILEMAP/DWB selector fault와 metadata 반환의 역할 보존 시험은 `crates/testing/engine-worker/src/fault_tests.rs`, 계측 adapter 시험은 같은 소유 `durable_performance/ports.rs`의 test module에 둔다. 기존 package·source fingerprint/clean-export roots 안의 구현이며 새 dependency·engine→platform 간선은 없다.

선택 page reader는 engine private `storage/page_io.rs`·`page_io_tests.rs`에 두고, 독립 fixture generator는 `scripts/dev3_disk_vectors.py`다. 실제 Linux 조립은 default 제외 engine `test-support` feature의 `testing.rs`를 worker의 dev-dependency에서만 활성화하여 `tests/page_io.rs`에서 검사한다. 기존 worker→engine 간선을 사용하며 정상 engine→platform 의존을 추가하지 않는다. 기본 제품 API와 시험 조립 feature를 receipt에서 구별한다.

[거래 상태의 영속 home](../contracts/dev3-status-home.md)는 private `storage/status_home.rs`·`status_home_tests.rs`와 engine의3개 literal fixture에 둔다. 기존192-byte core와 구 loader는 기존 표현 범위로 유지하고224-byte home·71-slot 검산은 별도 loader가 소유한다. 새 dependency/feature나 기본 제품 API는 없다. 무효과 표현 검증을 실제 count 갱신·terminal 회수로 해석하지 않는다.

[복구 의무 descriptor](../contracts/dev3-obligation-format.md#12-독립-descriptor-표현-leaf-제안)는 private `storage/obligation.rs`·`obligation_tests.rs`와 engine의3개1536-byte literal에 둔다. dependency/feature·기본 제품 API 변경 없이 owner/profile/budget 표현만 검산한다. inventory closure·예약 복원·WAL 적용 타입으로 승격하지 않는다.

[Ownership 관측](../contracts/dev3-ownership-format.md)은 private `storage/ownership.rs`·`ownership_tests.rs`와 inventory9개·실제 profile3개 literal에 둔다. 기존 descriptor/control 참조를 소비하며 caller의4×16KiB scratch와 고정 통계로 bytes·budget·물리 충돌을 검산한다. 새 dependency/feature·source root는 없다. 추가 API는 decoded profile과 결합하고128KiB 고정 key 공간으로 현재 growth intent 유일성을 검산한다. 과거 intent 재사용·예약 복원·실제 선택 I/O는 후속 의무다.

[RollbackStart 결합 관측](../contracts/dev3-obligation-format.md#13-rollbackstart의-full-state-결합-검산)은 private `storage/semantic_group.rs`·`semantic_group_tests.rs`와 새 literal10개를 사용한다. page의 immutable cell range와 status home getter, WAL의 공통 envelope·single redo 검사를 하위 소유 모듈에서 공유한다. 일반 WAL API는 Opaque0/PageRedo1 범위를 유지한다. 새 dependency/feature·source root는 없으며 실제 append·쓰기·복구 예약 복원은 후속이다.

두 번째 표현 작업의 [composite key](../contracts/dev3-key-format.md)는 caller buffer와 기존 foundation scalar bytes를 사용한다. foundation의 exact-size/encode-into/prefix 검사와 engine의 key 시험을 위 leaf 실행기에 함께 등록한다. M01–M06의 후속 모델은 reference integration tests에 두며 각 모델 계약·실제 receipt와 전체 DEV3 실행을 구분한다.

PageRef·UndoRef·anchor의 [참조 표현](../contracts/dev3-reference-format.md)도 같은 leaf 실행기의 고정 분모다. `scripts/check_dev3_models.py`는 M01–M06을 명시 선택하여 보존 source에서 release binary를 만들고 case·witness·반례·budget 검사 분모를 확인한다. 전체 DEV3 실행기로 사용하지 않는다. 실행기 자기검사는 `scripts/test_check_dev3_models.py`이며 M03/M06의 고정 inventory·형식 검사 helper와 자기검사는 `scripts/dev3_progress_receipts.py`, `scripts/test_dev3_progress_receipts.py`다. 실제 판정에 사용한 checker와 보존 source의 hash를 대조하고 progress helper는 보존 사본에서 직접 읽는다.

DISK는 물리 저장 방식만 분리한다. 공통 RowUID, snapshot, catalog, 원자 공개, WAL/recovery 계약을 유지한다. locator는 DISK 전체를 RAM map에 올리지 않는다. native global은 `storage/index/global`에서 같은 table의 매체 독립 key/RowUID 공간을 구현하고 SQL 숨김 table이나 local index fan-out으로 대신하지 않는다. PART/NG는 초기 SQL의 선행 조건이 아니다.

이 배치는 [recovery 설계](../contracts/recovery-logging-design.md)의 L0→L1/L2 순서나 [undo/RP 검토](../contracts/replication-undo-analysis.md)의 대안 상태를 바꾸지 않는다. legacy undo chain을 그대로 복제하거나 undo tablespace 대안을 이번 문서에서 채택하지 않는다.

<a id="parallel-development"></a>

### 5.1 병렬 개발을 위한 파일 구성·소유 규칙

채택 규칙의 원문은 [병렬 개발을 위한 파일 구성·소유 정책](parallel-development-policy.md)으로 분리해 관리한다. 책임별 파일 구성, 공통 계약·조립 지점의 최소화, 작업별 수정 범위·공유 파일 담당·병합 순서와 생성물 재생성을 적용한다. 관리 JSON의 `parallel_development_ref`와 [작업 계약 예시](../examples/task-contract.example.json)가 같은 정책을 참조한다. PD01–PD03은 미실행 개발 절차 인수 계획이다.

<a id="section-8"></a>

## 6. 단계별 소유 위치

단계 번호는 package 버전이나 순차 빌드 명령 번호가 아니다. 정확한 선행 조건은 `development-stages.json`과 작업 DAG를 따른다. DEV7/8 작업은 기능별로 다른 계층도 함께 확장할 수 있다.

| 단계 | 주요 위치 | 배치 기준 |
| --- | --- | --- |
| DEV0 | crates/testing/controller, crates/testing/protocol, crates/testing/fixture-worker | 현재 최소 harness |
| DEV1 | crates/foundation, crates/platform, crates/testing/foundation-worker | 공통 계약과 Linux 구현을 분리한 최소 기반 |
| DEV2 | crates/engine, crates/testing/reference, crates/testing/engine-worker | 다음 패키지 배치 결정. XADB-004 독립 oracle 필수 |
| DEV3 | crates/engine/src/storage | 같은 production engine을 확장. K0는 폐기할 모델이 아님 |
| DEV4 | crates/sql, crates/cli | embedded SQL부터 구현 |
| DEV5 | crates/server | COMM과 조립. DEV6 driver 완성을 요구하지 않음 |
| DEV6 | crates/client, clients/jdbc, crates/odbc | native → JDBC → ODBC |
| DEV7 | crates/sql, crates/foundation | 기능에 따라 저장·server도 확장. 전체 leaf 검토 후 package 분리 |
| DEV8 | crates/replication, benches, packaging | 복제·PITR·운영·장시간 검증. 기본 진단은 하위 단계부터 |
| DEV9 | tests, devdoc | 선언한 제품 기능·운영·성능 인수. 기존 제품 패키지와 시험 도구를 사용 |
| COMM | crates/transport | engine 없이 독립 검증 |
| PART | crates/engine/src/storage/partition | DEV3 뒤 구현 |
| NG | crates/engine/src/storage/index/global | PART 뒤 구현. SQL 기능 검증은 별도로 DEV5 필요 |

[AI 성능 개발 계약](../contracts/ai-performance-engineering.md)에 따라 공통 계측 의미는 foundation, OS 수집은 platform, 항목·저비용 수집은 소유 제품 모듈, 실행·독립 비교·증거는 XADBTest에 배치한다. 세부 파일은 실제 구현 시 추가하며 engine→test 의존을 만들지 않는다. 이번에는 성능 executable·신규 source root를 추가하지 않았다.

`benches/`는 실제 측정 대상이 생기면 초기 커널 단계부터 만들 수 있다. DEV8은 장시간·전체 workload qualification 단계이지 성능 관측을 처음 시작하는 시점이 아니다. CLI·로그·설정 이름은 [인터페이스 경험 기획](../reference/interface-experience-plan.md)의 제안 상태를 유지하며 crate 이름만으로 공개 명령 이름을 확정하지 않는다.

<a id="section-9"></a>

## 7. 빌드와 검사 진입점

현재 기준은 Rust 1.95.0, edition 2024, `x86_64-unknown-linux-gnu`다. 실제 검사 스크립트·문서 검증에는 Python 3.11 이상을 사용한다. Rust dependency는 `Cargo.lock`, toolchain은 `rust-toolchain.toml`로 고정한다. 현재 `Cargo.toml`은 사용자 정의 profile 없이 Cargo 기본 dev/release profile을 사용한다. 이 플랫폼을 확인했다는 것과 Linux 배포판·kernel·filesystem·장치 조합 전체를 제품 지원한다는 것은 다르다.

| ID | 명령 | 상태 / 실행 근거 | 출력 |
| --- | --- | --- | --- |
| workspace-debug | cargo build --workspace --locked | AVAILABLE / EXISTING_LOCAL_EVIDENCE | target/debug |
| dev0 | python3 scripts/check_dev0.py | AVAILABLE / EXISTING_LOCAL_EVIDENCE | artifacts/dev0 |
| dev1 | python3 scripts/check_dev1.py | AVAILABLE / EXISTING_LOCAL_EVIDENCE | artifacts/dev1 |
| foundation-only | cargo build -p xadb-foundation --no-default-features --locked | AVAILABLE / EXISTING_LOCAL_EVIDENCE | target/debug |
| release | cargo build --workspace --release --locked | AVAILABLE / NOT_RUN | target/release |
| dev2 | 미구현 | PLANNED / NOT_RUN | artifacts/dev2 |
| distribution | 미구현 | PLANNED / NOT_RUN | artifacts/dist |

`EXISTING_LOCAL_EVIDENCE`는 참조한 DEV0/DEV1 저장 결과를 뜻하며 이번 문서 변경에서 다시 실행했다는 뜻이 아니다. `cargo build --release`는 사용 가능한 Cargo 명령이지만 release 검증·배포는 미실행이다. `PLANNED` 행에는 실행 가능한 명령을 제공하지 않는다. 실제 `scripts/check_dev2.py` 작성 전에는 단계 JSON의 실행 진입점도 null 상태를 유지한다.


DEV1 검사는 DEV0 회귀와 production foundation/platform의 실제 연결을 포함한다. 후속 checker도 하위 회귀·독립 build·실제 integration을 포함하되, 테스트 수와 실행 명령을 먼저 발명하지 않는다. default, test-instrumented, release에서 같은 의미 계약을 검사하고, release 기능 조합은 첫 사내 배포 전에 확정한다. 미래 feature를 추가하면 `--all-features`가 모든 지원 조합을 대표하는지 재검토한다.

기존 `sure/release`, 재귀 Make, `build_all`의 clean+build, 환경별 절대 경로·전역 include 관행을 그대로 옮기지 않는다. Cargo가 manifest DAG로 증분 빌드 순서를 결정하고 단계 checker가 검증 순서를 소유한다. 문서 형식과 검사는 [문서 운영 기준](documentation-policy.md)을 따른다.

현재 실행 소스 fingerprint와 clean export는 `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates`, `tests`, `scripts`, `.github`를 포함한다. 새 `clients/`, `benches/`, `packaging/` 등 실행 입력 루트를 도입할 때 **`scripts/check_dev0.py`, `crates/testing/controller/build.rs`, clean-export 목록, `devdoc/verify_documents.py`의 증거 identity 범위**를 함께 갱신한다. 새 루트의 코드가 증거 hash에서 빠진 상태로 검증 완료를 주장하지 않는다. 설계 Markdown/JSON은 별도 문서 지문으로 검증한다.

<a id="section-10"></a>

## 8. CI·배포 산출물과 버전

첫 사내 배포의 qualification 대상·release profile·panic·feature·패키지·증거 보관·보호 승격 방침은 [개발 운영 방침 DP06](development-policy.md)으로 채택했다. Ubuntu 24.04/x86_64/ext4를 최초 대상에 한정하며, 정확한 image/kernel/device identity·recipe와 실제 검증은 배포 작업에서 완성한다. 현재 Cargo profile·CI 설정이 이 계획으로 변경되거나 지원 qualification을 통과한 상태는 아니다.

현재 `.github/workflows/dev0.yml`은 push/PR에서 `python3 scripts/check_dev1.py`를 실행하고 `artifacts/`를 보존하도록 작성되어 있다. 원격 실행·branch protection·보호 oracle 승격은 미검증이다. 후속 CI는 독립 package 검사, 현재 단계 통합, release/profile 조합, 문서 검증을 각각 증거로 남기며 해당 경로가 준비될 때 추가한다.

로컬 push 제약은 [DOC-002·DOC-003·DOC-004](documentation-policy.md#push-gate)의 `scripts/git-hooks/pre-push`와 `scripts/check_push.py`로 구현한다. 기존 `scripts` fingerprint/clean-export 루트 안에 배치한다. 실제 pushed ref tip commit의 임시 export에서 `cargo fmt --all --check`, Actions와 같은 workspace/target/feature 범위의 Clippy, 문서 검사를 순서대로 실행하며 편집·commit 검사를 추가하지 않는다. Clippy는 테스트 코드를 정적 검사하며 기능 시험은 실행하지 않는다. [관리 JSON](repository-build-plan.json)의 `pre_push_gate`가 같은 진입점을 색인한다. `scripts/test_check_push.py`는 임시 로컬 remote에 대한 훅 자체의 회귀다. 로컬 훅과 원격 CI/보호 설정은 별도다.

첫 사내 배포 전에 `packaging/`의 고정 recipe와 설치 smoke를 작성하고 결과는 `artifacts/dist/<version>/<target>/`에 둔다. source revision/dirty digest, toolchain/lockfile, target OS/kernel/filesystem, feature/profile, binary·symbol hash, stage/capability와 인수 증거를 manifest에 담는다. install prefix와 DB 경로는 분리하고, 라이브러리·header/JAR·도구는 해당 capability가 구현된 경우에만 포함한다. 지금 설치 패키지가 존재하거나 배포 검증을 마쳤다는 뜻은 아니다.

<a id="local-artifact-retention"></a>
### 로컬 산출물과 재생성 가능한 캐시

[D3-DIRECTION-002](../plans/dev3-implementation-plan.md#direction-refinement)의 운영 보완은 [기존 캐시 회수·증거 보존 사례](../evidence/dev3-runtime-core-20261024.md)를 재사용한다. [DP06](development-policy.md#section-7)의 배포/결함 보존 기간과 [원본 identity 보존](documentation-policy.md)을 유지하며, 전체 크기·나이만으로 `artifacts/`를 삭제하거나 원본을 요약으로 대체하지 않는다.

정리 전 소유 작업·사용 중 여부·증거 참조를 확인해 재생성 가능한 build/incremental cache와 보존할 source·binary·fixture·raw·원 FAIL·로그를 구분한다. cache 경로에 있다는 이유만으로 삭제 가능하다고 보지 않는다. hardlink·공유 cache는 보호 대상 inode/bytes와 실제 사용자를 확인하고, 정확한 회수 대상·이유·원본 보존 확인을 기존 작업 기록에 남긴다. 사용 중이거나 증거와의 관계가 불명확한 대상은 보류한다.

새 보관 서비스·자동 만료·전체 산출물 정리를 기능 개발의 선행으로 만들지 않는다. 실제 공간 부족은 해당 작업 범위에서 해결하며 기존 source별 증거를 새 실행 PASS로 바꾸지 않는다. 이번 문서 반영으로 캐시 삭제나 자동 정리 도구를 실행·구현한 것은 아니다.


<a id="section-11"></a>

## 9. 구현 전에 남은 구체 결정

| ID | 확정 시점 | 구체 결정 |
| --- | --- | --- |
| RB01 | DEV2 K0 구현 전 | 공개 row/catalog API, 오류·상한·history operation과 oracle vector. 경로 결정으로 의미까지 승인하지 않음 |
| RB02 | DEV2 K2–K5 구현 전 | 기존 recovery 계약에 따른 WAL/checkpoint byte 형식과 filesystem·fault 검증 조건 |
| RB03 | 첫 사내 배포 전 | DP06의 채택 환경·release 방침을 구현. 정확한 image/kernel/device identity, capability별 feature 목록·설치 recipe·보호 승격과 실제 qualification 증거 확정 |
| RB04 | DEV6 구현 전 | native ABI/FFI 소유권, JDBC 빌드 도구/JDK, ODBC driver manager와 패키징 |
| RB05 | 실행 입력 루트·generator 추가 전 | source fingerprint·build identity·clean export·문서의 증거 identity 범위를 함께 확장. generator 입력·version 기록 |
| RB06 | 후속 crate 분리·구성 요소 추가 전 | 의존 DAG·공개 경계·제품 인수 단위를 검토. 모듈마다 crate를 기계적으로 만들지 않음 |
| RB07 | 새 OS 지원 결정 시 | 파일 영속화·잠금·시간·대기/취소 계약과 OS/CPU/FS 조합, 실제 복구·회귀·패키징·CI 증거를 확정. 컴파일 성공만으로 지원 선언 금지 |

위 항목은 배치 정책의 미결정이 아니라 각 구현·배포 단계의 선행 입력이다. 새 crate가 필요하면 책임/공개 경계·역방향 의존 금지·독립 fixture·integration·실행 입력 hash를 함께 검토하고 이 Markdown/JSON을 같은 변경에서 수정한다.

<a id="section-12"></a>

## 10. 문서 검증과 증거 범위

문서 자동 검사는 [DOC-002](documentation-policy.md#push-gate)에 따라 push 직전에 전송할 commit에서 수행한다. 편집·commit 때 실행하지 않는다.


문서 검사 성공은 Cargo build나 DB recovery 성공이 아니다. 현재 DEV2 P0/K0 leaf의 로컬 실행은 [증거 색인](../evidence/README.md)을 따르며, DEV2 전체 인수·후속 단계·release·패키징·원격 CI는 계속 미실행이다. 최초 배치 문서 작성은 제품 소스를 변경하지 않았다. 후속 플랫폼 분리는 실제 DEV1 코드 변경이며, 이전 실행 결과는 `dev0-validation-pre-platform.json`·`dev1-validation-pre-platform.json`으로 보존하고 현재 DEV0/DEV1 결과는 실제 재실행으로 갱신한다.

K0-005에서 worker의 Linux CPU/RSS 관측은 `xadb-platform::performance`가 소유하며 worker→platform normal 간선을 사용한다. engine/reference에는 OS 의존을 추가하지 않는다. controller와 worker는 `crates/testing/build_identity.rs`의 같은 build 지문 산술을 사용하고 기존 sha2 build dependency를 소비한다. 이는 관측 identity이며 독립 정답 계산 공유가 아니다. 현재 source-only export는 `crates/` 전체를 포함하여 helper도 보존한다.

DEV3의 `storage/catalog_record`, `undo_record`, `control`은 각각 [catalog](../contracts/dev3-catalog-format.md), [undo](../contracts/dev3-undo-format.md), [control](../contracts/dev3-control-format.md)의 caller-buffer 표현을 소유한다. 실제 DDL·undo·selector·복구 예약 효과와 분리하며 dependency/source roots는 유지한다. 국소 실행·독립 검토와 후속 통합 소스 결과는 [구현 기록](../implementation/dev3-implementation.md)에서 구별한다.


DEV3 최초 저장 세션은 `storage/wal_io/bootstrap`이 기존 genesis WAL owner 안에서 extent·FILEMAP 설치를 조립한다. 공통 차단 상태는 하위 `storage/session_fence`, 읽기 lease·buffer 연결은 기존 `page_io`·`read_buffer`가 소유한다. `testing/storage_session`과 engine-worker의 `storage_session`/`support/storage_session`은 시험용 관측 경계다. `scripts/check_dev3_storage_borrows.py`, `dev3_storage_model.py`, `dev3_storage_native.py`, `test_dev3_storage_checks.py`와 두 기존 fixture 루트의 `dev3-storage-session` 입력을 실제 구현과 함께 등록한다. 기존 source fingerprint/clean export 루트와 Cargo 의존성은 유지하며, 검증 결과와 제한 범위는 [구현 기록](../implementation/dev3-implementation.md)을 따른다.

DEV3 최초 doublewrite 설치는 기존 `wal_io/bootstrap`의 private child `doublewrite_install`과 고정 runtime 시험에서 구현한다. 기존 `testing/storage_session` facade와 Linux `support/storage_session` port를 재사용하고 실제 native 진입점 `tests/doublewrite_install.rs`, compiler checker `scripts/check_dev3_doublewrite_borrows.py`, 동결 입력 `tests/fixtures/dev3-doublewrite-install/`을 등록한다. 최초 설치의 제한된 실행·독립 D1 결과를 보존하며 전체 단계 완료가 아니다. 변경 없는 소스의 시험은 반복하지 않는다. 새 의존성이나 source fingerprint 루트 변경은 없다.

D3-DWB-INSTALL-001의 실제 trace·유한 모델과 Linux 파일 inventory 검산은 `scripts/dev3_doublewrite_model.py`, `scripts/dev3_doublewrite_native.py`가 담당하며 고정 반례31의 실제 receipt 변형은 `scripts/dev3_doublewrite_checks.py`, 별도 자기시험은 `scripts/test_dev3_doublewrite_checks.py`, 통합 실행은 `scripts/check_dev3_leaf.py`에 연결한다. 실행 상태는 해당 구현 기록과 실제 receipt를 따른다.


DEV3 페이지 예약·취소는 `wal_io/doublewrite_install`의 private child `allocation_reservation`과 전용 `allocation_operation`에서 구현한다. 실제 작성된 runtime 시험과 `tests/fixtures/dev3-allocation-reservation/`의 독립 byte·semantic·compiler 입력만 등록한다. native 실행기와 compiler runner는 아직 작성하지 않아 경로를 선등록하지 않는다. 기존 WAL stage의 공통 복사 helper를 실제 분리했으므로 그 네 관련 시험은 미실행 의무로 남긴다. 부분 실행과 사용자 요청에 따른 중단 지점은 [구현 기록](../implementation/dev3-implementation.md#allocation-reservation-partial)을 따른다. 새 dependency/source root는 없다.

## DEV3 첫 dirty stage 소비자

[dirty frame·stage 계약](../contracts/dev3-dirty-stage.md)의 실제 경로는 관리 JSON의 `dev3_dirty_stage`에 등록한다. borrowed payload·고정 directory/pin/DPT, mutable storage 조립과 allocator reserve/cancel 소비, 각각의 신규 oracle/runtime/native/compiler 입력을 공급한다. 제품 의존성·crate·source root 변경은 없다. [제한 단위 인수](../evidence/dev3-dirty-stage-20261022.md)를 수용했으며 INIT·flush·일반 복구와 전체 DEV3 완료를 뜻하지 않는다.

## DEV3 allocator 한 페이지 flush

[flush 소유 계약](../contracts/dev3-allocator-flush.md)의 실제 source/fixture/checker는 관리 JSON의 `dev3_allocator_flush_one`에 등록한다. fixed registry에 authentic WAL requirement와 claim을 보유하고 실제 capture→WAL→DWB→home→whole retirement→조건부 RAM finalize를 연결한다. 기존 source root/crate/dependency는 유지한다. [source별 선택 인수](../evidence/dev3-allocator-flush-20261022.md)는 제한 단위로 수용했으며 batch8/64·다중 file·INIT·ordinary recovery·전체 DEV3 완료를 뜻하지 않는다.

## DEV3 불변 Status bootstrap

[Status bootstrap 계약](../contracts/dev3-status-bootstrap.md)의 실제 제품 변경·시험·fixture·checker는 관리 JSON의 `dev3_status_bootstrap`에 등록한다. 새 crate/source root/dependency 없이 plan2 영속 역할과 borrowed role·old adapter guard를 공급한다. [선택 실행](../evidence/dev3-status-bootstrap-20261022.md)의 제한 단위 독립 검토를 수용했으며 실제 INIT·일반 recovery·전체 DEV3 인수는 미완료다.

## DEV3 폐쇄 두-frame Status INIT

[Status INIT 계약](../contracts/dev3-status-init.md)의 실제 제품·시험·독립 literal·compiler/runtime checker는 관리 JSON의 `dev3_status_init`에 등록한다. 새 crate/source root/dependency 없이 exact reserve→두 image RAM 공개→동일 stage requirement의 durable 확인과 immutable 두-pin reader를 공급한다. 선택 인수와 제한 단위의 독립 검토를 수용했으며 Status home flush·일반 recovery·전체 DEV3는 미완료다.

## DEV3 Status home flush

[Status home flush 계약](../contracts/dev3-status-home-flush.md)의 실제 제품·시험7개, shared bridge 변경, 독립 literal/compiler/runtime/native fixture와 checker를 관리 JSON의 `dev3_status_home_flush`에 등록한다. 새 crate/source root/dependency 없이 기존 bound를 소비하고 published Status와 allocator를 순차 whole batch1로 flush한다. [source별 선택 인수](../evidence/dev3-status-home-flush-20261022.md)의 필수43·compiler6·native3 실행과 원 FAIL을 보존하며 제한 단위의 최종 독립 D1을 수용했다. 일반 recovery/reopen/READY와 전체 DEV3/S5는 미완료다.

## DEV3 반복 Status 이력 복구

[F09 소유 계약](../contracts/dev3-recovery-effects.md#operational-status-recovery)의 기존 Prefix/effect 확장과 새 runtime/native/S5 fixture·독립 oracle 경로를 관리 JSON `dev3_operational_recovery`에 등록한다. caller 고정 arena 안에서 첫 extent64주소의 census·물리 scratch를 사용하고 crate/dependency/source root는 유지한다. [선택 증거](../evidence/dev3-operational-recovery-20261023.md)와 최종 독립 검토 상태를 따르며 전체 DEV3/S5 완료가 아니다.


## DEV3 Status home load P3 경로

[P3 소유 계약](../contracts/dev3-task-05-buffer-flush.md#operational-status-p3)의 `status_residency`와
`status_load`·선택 unit/native/compiler/독립 checker·fixture를 관리 JSON `dev3_status_load`에
등록한다. 기존 owner·2frame·caller scratch를 사용하며 crate/dependency/source root를 바꾸지 않는다.
실행·독립 리뷰 상태는 [증거](../evidence/dev3-status-load-20261023.md)를 따른다.


### 최초 Begin 순수 준비 배치

[소유 계약](../contracts/dev3-obligation-format.md#begin-preparation)의 private
`semantic_group::begin`은 `semantic_begin.rs`·`semantic_begin_tests.rs`와 독립 literal4/manifest를
사용한다. 실제 경로는 관리 JSON `dev3_status_begin`에 등록했다. source root·package/dependency/
feature·공개 WAL API는 바꾸지 않는다. [증거](../evidence/dev3-status-begin-20261023.md)는 순수
준비의 선택 결과이며 실제 append·live ID 발급·일반 pool·현재-build 복구 공급은 후속이다.

## DEV3 전체 대상 pin 집합 배치

[소유 계약](../contracts/dev3-task-05-buffer-flush.md#bounded-target-set)은 기존 `dirty_frame`
내 pin/guard를 const generic으로 확장하고 기존 기본2 경로를 그대로 사용한다. 새 Rust 시험,
compiler manifest/6개 probe와 Python runner/checker의 실제 경로를 관리 JSON `dev3_target_set`에
등록한다. package/dependency/source root·feature는 바꾸지 않는다.
[source별 증거](../evidence/dev3-target-set-20261023.md)는 단일 pool primitive의 선택 결과이며
일반 resident backing/load/eviction·실거래·전체 DEV3 인수가 아니다.


## DEV3 resident Status 수명 배치

[소유 계약](../contracts/dev3-task-05-buffer-flush.md#resident-status-pool)의 `resident_status.rs`는
원 closed binding/SharedPool/INIT/flush/load 코어의 제한 소비자다. 실제 Rust·native worker·borrow/
checker runner·literal/compiler fixtures는 관리 JSON `dev3_resident_status`에 등록했다. package/
dependency/source root·feature·영속 wire 형식은 바꾸지 않는다.
[선택 증거](../evidence/dev3-resident-status-20261023.md)는 여러 immutable Status 수명의 제한 결과이며
일반 pressure/live 거래·전체 DEV3 인수가 아니다.


## DEV3 빈 거래 통합 준비 배치

[§17](../contracts/dev3-obligation-format.md#empty-abort-preparation)의 순수 AbortEnd 준비는
`semantic_empty_abort.rs`와 시험, 독립 `dev3_empty_abort_literals.py` 및 실제 fixture 경로를
관리 JSON `dev3_empty_transaction`에 등록한다. 공통 native/page 검산은 `semantic_group`에서
RollbackStart와 공유한다. package·dependency·source root·feature는 바꾸지 않는다.
[작업 증거](../evidence/dev3-empty-transaction-20261023.md)의 실제 owner/복구 통합은 진행 중이다.
같은 통합의 `wal_io/empty_transactions.rs`·시험·선택 manifest·compiler fixture/runner를 추가한다.
원 ResidentStatus의 consuming 인계와 caller bank를 보존한다. 같은 owner의 실제 Begin/RollbackStart/
AbortEnd는 `wal_io/empty_transactions/live.rs`·시험·live fixture/manifest와 compiler2개를 추가해
연결한다. sealed install proof의 명시적 re-export와 frozen allocator observer를 사용하며 새
crate/source root/dependency는 없다.
[§20](../contracts/dev3-obligation-format.md#empty-transaction-flush)의 실제 WAL 이력 검산은
`empty_transactions/history.rs`에서 기존 semantic materializer를 조합하고 원 Status flush
authority에 private digest 이력을 보유한다. Linux worker·선택 manifest·compiler2개 및 전용
runner/checker를 관리 JSON에 추가한다. live load·native recovery 통합은 남아 있다.

[§21](../contracts/dev3-obligation-format.md#empty-transaction-load)의 최신 Status 재적재는
`empty_transactions/load.rs`·memory/native 시험·load manifest·새 compiler2개를 추가한다.
private sealed proof를 기존 load permit로 소비하며 기존 caller loan/SharedPool을 재사용한다.
실제 경로만 같은 관리 JSON에 등록하고 package·dependency·source root는 유지한다.


[빈 거래 closed recovery F10](../contracts/dev3-recovery-effects.md#empty-transaction-recovery)은 기존 Prefix/ClosedRecovery에
`recovery_empty_transactions.rs`의 canonical bank와 typed callback을 연결한다. 신규 memory/native test,
raw literal/native oracle·compiler fixture 실제 경로를 JSON에 등록했다. source roots와 clean export 규칙은
유지하며 [source별 증거](../evidence/dev3-empty-transaction-20261023.md#closed-recovery)에 한정한다.

[Status 후보 선택](../contracts/dev3-task-05-buffer-flush.md#status-residency-pressure)은 기존 dirty frame의
selector와 EmptyTransactions의 단계별 load/flush 관측을 추가한다. 실제 신규 경로만 관리 JSON의
`dev3_status_residency_pressure`에 등록하며 새 package/dependency/source root는 없다.

## DEV4 독립 lexer·이름 배치

[첫 앞단 계약](../contracts/dev4-independent-frontend.md#lexer-leaf)의 실제 코드를 `crates/sql`에 등록했다. normal dependency는 foundation만, dev dependency는 독립 literal/판정기 reference만 소비한다. test-support의 borrowed progress observer는 기본 제품 build에서 제외한다. 기존 crates/tests source fingerprint·clean export 범위를 사용한다. 실제 파일 목록은 관리 JSON의 `dev4_frontend`, [구현 범위](../implementation/dev4-implementation.md)·[source별 증거](../evidence/dev4-frontend-20261024.md)를 따른다. binder/executor/engine·Q1은 미구현/미실행이다.

[streaming 경계·원문 예약](../contracts/dev4-independent-frontend.md#splitter-leaf)은 같은 frontend의 source/splitter/UTF-8 carry를 추가한다. reference의 독립 splitter literal·새 splitter/resource 시험·dev4-splitter fixture를 기존 source roots에 등록했다. 반환 source와 기존 session/DB quota의 수명을 함께 관리하며 새 package/dependency는 없다. [선택 증거](../evidence/dev4-splitter-20261024.md)의 source별 결과를 유지한다.

[P2 owned AST·parser](../contracts/dev4-independent-frontend.md#parser-leaf)은 같은 frontend에 ast/parse_budget/parser를 추가한다. 독립 reference parser literal/recipe와 parser_contracts/parser_resources·dev4-parser fixture는 기존 source roots 안에 있다. 원문·typed arena가 기존 session/DB quota를 함께 소비하며 새 package/dependency는 없다. [선택 증거](../evidence/dev4-parser-20261024.md)에 원 source별 결과와 후속 미구현 범위를 남긴다.

[P3 scalar 입력](../contracts/dev4-independent-frontend.md#scalar-leaf)은 기존 frontend의 scalar.rs/scalar/budget.rs와 reference scalar literal/recipe, scalar_contracts/scalar_resources·dev4-scalar fixture를 추가한다. String/Vec 출력의 타입·NULL·quota 소유를 SQL에서 보존하고 foundation 의미·package/dependency·source root는 바꾸지 않는다. [선택 증거](../evidence/dev4-scalar-20261024.md)에 실제 범위와 predicate·binding·engine 미완료를 구분한다.

[P3 predicate AST/parser](../contracts/dev4-independent-frontend.md#predicate-parser-leaf)는 기존 ast.rs/parser.rs와
parser/predicate.rs, reference predicate literal/recipe, predicate_contracts/predicate_resources 및
실제 dev4-predicate fixture를 추가한다. package·dependency·source/export root는 바뀌지 않는다.
[선택 증거](../evidence/dev4-predicate-20261024.md)에 원 실패/수정·source별 결과와 typed predicate·
P4 연결·실제 engine/Q1 미완료를 구분한다.


<a id="plan4-clean-owner-layout"></a>
## plan4 초기 page의 원 공유 버퍼 소비 배치

[소유 계약](../contracts/dev3-plan4-clean-owner.md)의 원 CAS·loan·SharedPool 결합은
`wal_io/plan4_pages.rs`가 담당하고, load·실제 pin 소비·자원 census는 같은 모듈의
`plan4_pages/load.rs`, `plan4_pages/read.rs`, `plan4_pages/resources.rs`에 둔다.
`dirty_frame/plan4_residency.rs`는 기존 directory와 frame을 쓰며 별도 pool을 만들지 않는다.
memory/native 시험과 exact-ID·compiler 수집기는 [관리 JSON](repository-build-plan.json)의
`dev3_plan4_clean_owner`에 실제 경로만 등록한다. 기존 genesis raw는 그대로 재사용한다.
dirty/flush·복구 owner와 Catalog CREATE/CLR의 구현·인수를 이 배치로 대체하지 않는다.

<a id="plan4-genesis-layout"></a>
### DEV3 plan4 최초 설치 배치 — 2026-10-24

[소유 계약](../contracts/dev3-plan4-genesis.md)의 canonical queue/빈 Catalog codec은
`storage/retention_genesis.rs`, 설치는 기존 `wal_io/bootstrap.rs`와 새
`wal_io/plan4_genesis.rs`, 닫힌 선택은 `storage/plan4_recovery_select.rs`에 둔다.
원 FreshStorageSession·maintenance·DWB를 사용하며 새 crate나 별도 저장 엔진은 추가하지 않는다.
독립 raw·recipe·compiler 입력은 `tests/fixtures/dev3-plan4-genesis`, 정확 ID 수집기와
실제 Linux 3PID 조립은 `scripts/dev3_plan4_genesis_*`가 소유한다. 실제 구현 경로는
[관리 JSON](repository-build-plan.json)의 `dev3_plan4_genesis`에 등록하고
[source별 실행 증거](../evidence/dev3-plan4-genesis-20261024.md)를 구별한다.
이 배치 자체는 live 큐·Catalog CREATE/CLR·일반 복구 또는 DEV3/S5 완료가 아니다.

[P3 typed predicate](../contracts/dev4-independent-frontend.md#typed-predicate)는 기존 frontend에
`typed_predicate.rs`와 budget/compile/eval을 추가한다. reference의 독립 typed_predicate literal/recipe,
두 신규 typed_predicate test target과 실제 fixture를 등록했다. exact typed quota와 borrowed3VL은
SQL 책임이며 새 crate/dependency·foundation 변경·source/export root 변경은 없다.
[source별 증거](../evidence/dev4-typed-predicate-20261024.md)에 원 실패/교정·선택 범위와 남은 실제
engine 연결을 구분한다.


[P4 나머지 raw 문장 parser](../contracts/dev4-independent-frontend.md#remaining-parser)는 기존
ast/parser에 syntax variant와 dispatch를 추가하고 `parser/statement.rs`, `parser/ddl.rs`로
SELECT/DML/DDL을 나눈다. 독립 reference의 `frontend/remaining_parser.rs`·literal/recipe,
`remaining_parser_contracts`/`remaining_parser_resources`와 실제 fixture를 기존 source root에
등록했다. package·dependency·source/clean export root는 변경하지 않았다. 실제 source 지문은 실행 증거에 별도로 기록했다.
[source별 증거](../evidence/dev4-remaining-parser-20261024.md)의 정상 선택 검증·21제품 변조 거절과 최종 독립 D1의 제한 수용을 기록하며 typed 문장 binding·P5/P6·engine·Q1은 이 배치의 완료 범위가 아니다.

<a id="runtime-core-layout"></a>
### 원 WAL runtime/core 추출

[저장 소유 계약](../contracts/dev3-storage-session.md#private-runtime-core)의 내부 추출과 기존 시험의 필드 접근 적응은 engine 안에 유지한다. 실제 신규 파일은 `wal_io/runtime_core_tests.rs`이며 [관리 JSON](repository-build-plan.json)의 `dev3_runtime_core`에38개 변경/신규 경로를 등록한다. [선택 증거](../evidence/dev3-runtime-core-20261024.md)는 신규7개 PASS와 원 실패를 구별한다. 진행 중인 G6 wire/live/recovery 소스는 이 단위에 포함하지 않으며 최종 독립 D1을 이 추출 범위에서 수용했다.

## DEV3 빈 거래·정리 의미의 파일 경계

[소유 계약](../contracts/dev3-empty-retention-semantics.md)의 `retention_page.rs`, `retention_obligation.rs`, `semantic_retention.rs`와 각 시험 파일을 engine storage 안에 둔다. 기존 `obligation.rs`의 고정 macro와 `wal_frame.rs`의 별도 tag3 진입점을 공유하고 새 package/의존성은 추가하지 않는다. 실제 새 fixture와 원 vector를 포함한48경로는 [관리 JSON](repository-build-plan.json)의 `dev3_empty_retention_semantics`에 등록한다. [실행 증거](../evidence/dev3-empty-retention-semantics-20261024.md)는 필수140의 source별 PASS와 원 실패를 구분한다. 별도 control/base/live 초안은 이 범위에서 제외하며 최종 독립 D1을 이 순수 단위에서 수용했다.

[DEV4 P5 상태·입력 codec](../contracts/dev4-lifecycle-codec.md)은 `crates/testing/reference/src/lifecycle`에 순수 bounded 자료형과 canonical wire를 둔다. 기존 sha20.10.9를 normal, serde_json1.0.145를 독립 literal의 dev dependency로 재사용하며 새 외부 패키지·production 의미 의존은 없다. 실제 source만 관리 JSON의 `dev4_lifecycle_codec`에 등록한다. controller BFS·파일 재개는 아직 추가하지 않았다.

## DEV3 복구 이미지·제어 레코드 — 2026-10-24

[계약](../contracts/dev3-recovery-control-base.md)의 XRB1/XRC1/XWE2/XWS2 codec과 test3개, ownership/allocation/Catalog 공통 bridge를 engine의 기존 storage 경계에 배치했다. 두 fixture 디렉터리의 독립 generator·고정 입력·raw/reference를 함께 등록한다. 구현한 정확68개 경로는 [기계 계획](repository-build-plan.json)의 `dev3_recovery_control_base`가 소유하며 package·의존 방향·export/fingerprint roots는 유지한다. [선택 증거](../evidence/dev3-recovery-control-base-20261024.md)의 신규272 PASS와 S5 추가23,886 NOT_RUN을 구별한다. actual live/recovery owner는 이번 순수 단위에 포함하지 않는다.

## DEV4 P5 원시 번호·target 선택

[결정적 BFS 탐색기 계약](../contracts/dev4-lifecycle-explorer.md)의 `explore`는 controller의 `lifecycle.rs`와 시험
`lifecycle/tests.rs`에 두고 reference의 공개 lifecycle API만 쓴다. 관리 JSON의 `dev4_lifecycle_explorer`에 실제
경로와 독립 initial vector를 등록하며 package·dependency는 유지한다. 실행 산출물은 gitignored
`artifacts/dev4-lifecycle-bfs/`에 둔다.

[독립 edge checker 계약](../contracts/dev4-lifecycle-checker.md)의 `verify_edge`는 같은 reference lifecycle
경계의 `checker.rs`와 `checker/` 하위 파일에 두고 시험 `checker_tests.rs`·모델 차등 `differential_tests.rs`를
추가한다. 관리 JSON의 `dev4_lifecycle_checker`에 실제 경로를 등록하며 package·dependency는 유지한다.
판정으로 고친 모델·조립기 파일은 기존 등록을 유지한다.

[수명 모델 전이 계약](../contracts/dev4-lifecycle-model.md)의 `transition`은 reference lifecycle의 `model.rs`와
`model/` 하위 파일에 두고 시험 `model_tests.rs`를 추가한다. 관리 JSON의 `dev4_lifecycle_model`에 실제 경로를
등록하며 package·dependency는 유지한다.

[전체 입력 조립 계약](../contracts/dev4-lifecycle-assembler.md)의 `assemble_input`은 같은 reference
lifecycle 경계의 `input_assembler.rs`와 `input_assembler/rows.rs`에 둔다. 시험 전용 `assembler_literal.rs`·
`assembler_tests.rs`와 rich·supplement 입력을 추가하며 package·dependency·export/fingerprint roots는 유지한다.
실제 경로는 관리 JSON의 `dev4_lifecycle_assembler`에 등록한다. 모델 전이·BFS는 위 별도 구현 단위를 따른다.

후속 [전체 입력 조립 준비](../evidence/dev4-lifecycle-assembly-inputs-20261025.md)는
`tests/fixtures/dev4-lifecycle-assembly`의 실제 정적 자료만 추가한다. 검토 후보1658개와 원
작성/검토 hash를 보존하며 제품 모듈·Cargo 의존·시험 진입점은 추가하지 않는다. 전체 D2와
조립기 구현은 미완료이며 관리 JSON의 `dev4_lifecycle_assembly_inputs`에 실제 경로를 등록한다.

[소유 계약](../contracts/dev4-lifecycle-raw-selection.md)의 세 순수 API와 독립 입력·시험을 기존 reference lifecycle 경계에 추가한다. 실제 작성한 source와 고정 fixture만 관리 JSON의 `dev4_lifecycle_raw_selection`에 등록한다. package·dependency·export/fingerprint roots는 유지한다. full input 조립·모델·BFS는 후속이다. [선택 증거](../evidence/dev4-lifecycle-raw-selection-20261025.md)는 source별 실제 실행과 미완료 범위를 구분한다.

## DEV3 plan4 빈 거래 live·prefix — 2026-10-25

[소유 계약](../contracts/dev3-retention-live.md)의 실제 구현39개 경로를 [관리 JSON](repository-build-plan.json)의 `dev3_retention_live`에 등록한다. engine storage 내부 child와 fixture만 추가하고 package·dependency·export/fingerprint roots는 유지한다. 새 recovery owner 작업은 별도 미완료다. [원 결과](../evidence/dev3-retention-live-20261025.md)는75개 source별 PASS와 S5 추가269 NOT_RUN을 구별한다.

## DEV3 plan4 복구 중간 저장 — 2026-10-25

[중지 기록](../evidence/dev3-recovery-paused-20261025.md)의 실제 작성 파일만 관리 JSON `dev3_plan4_recovery_checkpoint`에 등록한다. 최신 후보는 미검증 WIP이며 패키지·의존성·공개 지원 범위를 추가하지 않는다. 미연결 fault/observation source도 draft 상태로 보존하고, 인수 완료로 세지 않는다.

2026-10-25 [중단 작업 재개](../evidence/dev3-recovery-resumed-20261025.md)는 별도 `dev3_plan4_recovery_owner`에 실제 추가한 closure/physical/publication/observation/native 시험과 native controller를 등록한다. 앞의 checkpoint는 당시 상태로 보존한다. 기존 engine·scripts·tests source roots 안의 구현이며 패키지나 의존성을 추가하지 않는다. 원 source·완료·실패와 새 선택 실행의 경계는 재개 증거를 따른다.


## DEV2 PF04 P3 영속 I/O 관측 도구

[단계 계획](../plans/dev2-pf04-resolution-plan.md#p3-io-cause)의 XADBTest 도구를 기존 scripts root에 추가했다. `dev2_io_trace.py`는 P1 runner 조립, `dev2_io_supervisor.py`는 고정 root child 수명, `dev2_io_trace.bt`는 kernel event, `dev2_io_analysis.py`는 보존 event 분석, `test_dev2_io_trace.py`는 변경 범위 선택 검증을 소유한다. 제품 package·의존·source root·clean export 규칙은 바꾸지 않는다. 설치된 bpftrace0.25와 kernel BTF가 필요한 opt-in 진단이며 제품 성능 인수나 자동 최적화 경로가 아니다. [실제 실행·미확정 범위](../evidence/dev2-pf04-io-cause-20261025.md)를 따른다.

## 내부 이름과 반복 파일명 정리 — 2026-10-25

`XADB-NAME-001`은 사용자 요청으로 library target·dependency alias를 단축하고 기존 package·bin·환경변수·wire 식별자를 유지한다. `wal_io`의 plan4 retention/recovery/pages, empty transactions, Status home flush child를 기능 폴더에 배치하되 Rust 모듈 경로와 시험 selector는 유지한다. 파일 이동과 import만으로 제품 의미·인수 상태를 변경하지 않는다.

두 durable worker source는 짧은 이름과 명시적 `[[bin]]`으로 연결한다. controller의 private 실행 조립은 `runner.rs`, 문서 계약 시험은 `test_contracts.py`다. 빈 거래 borrowing checker는 `scripts/dev3/empty_transactions/check_borrows.py`이며 직접 실행과 `PYTHONPATH=scripts python3 -B -m unittest dev3.empty_transactions.test_borrows` package 진입점을 지원한다. 변경 사례만 선택하며 전체 모듈 시험 명령을 자동 실행 요구로 해석하지 않는다.

현재 compiler fixture의 import·길이·해시와 관련 runner pin을 함께 갱신한다. 과거 compiler 이력은 그 보존 source의 Cargo library 이름을 읽어 원 target과 결합한다. 실제 변경·선택 검증·독립 검토 및 원 실패는 [명칭 정리 기록](../evidence/naming-cleanup-20261025.md)을 따른다. 기존 증거의 원 이름과 byte는 보존한다.


## 첫 행의 leaf 후보 편집 — 2026-10-25

`storage/tree_page/edit.rs`와 `edit/tests.rs`는 기존 tree codec의 caller-scratch 편집 책임이다. 별도 crate/dependency·source root·I/O 권한을 추가하지 않으며 실제 Catalog/Row/CLR producer가 소비한다. `check_dev3_leaf.py`의 새 선택 inventory와 `check_dev3_tree_edit_borrows.py`·`test_dev3_tree_edit.py`를 기존 scripts 지문 범위에서 관리한다. 실제 source·binary·입력·원 실패·정리는 [선택 증거](../evidence/dev3-leaf-edit-20261025.md)를 따른다.


### PF04 CPU 요청 범위 제어 도구

[한정 계획](../plans/dev2-pf04-resolution-plan.md#frequency-control)의 구현은 `scripts/dev2_frequency_control.py`, 새 경계 시험은 `scripts/test_dev2_frequency_control.py`가 소유한다. 두 CPU의 max 요청만 일시 제어하고 root 소유 lease·systemd StopPost로 원복한다. 제품·WAL·source root·의존성은 변경하지 않으며 제품 성능 인수와 구분한다.


## 모듈 독립 개발 C0 생산자 경계 — 2026-10-26

[소유 계약](../contracts/dev3-module-boundaries.md#producer-input-api)의 관측 context·consuming lane·untrusted 후보를 `crates/engine/src/storage/wal_io/plan4_apply.rs`에 구현하고 `wal_io/disk_api.rs`와 `storage/disk.rs`에서 선택 재노출한다. 실제 존재하는 조상 모듈 이름만 `wal_io` 내부에 열며 private field/factory와 원 owner의 효과 경로는 유지한다. 실제 source 포착·예약·설치·read port와 두 소비자 본문은 아직 공급하지 않았다.

`tests/fixtures/dev3-module-c0/compiler/`의 정상1·부정4 입력과 동결 manifest를 등록한다. 실행은 원 소스 export에 각 입력을 따로 연결한 library compiler 검사이며 runtime 시험이 아니다. source·명령·진단·원 판정기 FAIL과 수동 검토는 [선택 증거](../evidence/module-independence-survey-20261026.md#c0-producer-types)를 따른다. 기존 compiler helper를 재사용하며 새 package/dependency·source root·clean export 규칙은 없다. 정확 경로는 관리 JSON의 `dev3_module_c0_producer`에 둔다.


## 모듈 독립 개발 C1 물리 WAL 분리 — 2026-10-26

[선정 경계](../contracts/dev3-module-boundaries.md#target-imports)에 따라 `wal_io/physical.rs`가 원 `WalRuntime`의 실제 stage·requirement·wait·write/sync 본문을 소유한다. runtime·AppendState·원 File/slot/fence/Condvar·생성/Drop은 부모에 남고, method는 wal_io 내부 또는 physical private로 제한한다. Catalog/Row·terminal receipt는 physical의 의존 대상이 아니다. 원 semantic `commit_bridge`의 apply 이전과 DiskSession/port 공급은 후속 의무다.

새 compiler 입력은 `tests/fixtures/dev3-module-c1/compiler/`의 정상 sibling 호출·외부 경로 거절·동결 manifest다. production/test cfg 컴파일만 확인하며 기존 runtime 시험을 실행하지 않는다. 실제 경로는 관리 JSON의 `dev3_module_c1_physical`, source·정확 명령·선택 결과는 [증거](../evidence/module-independence-survey-20261026.md#c1-physical-wal)에 기록한다. 새 package/dependency·source root는 없다.

## 모듈 독립 개발 C1 원 owner·port 이전 — 2026-10-26

`wal_io/plan4_retention.rs`의 실제 정상 owner·consuming 전환·split을 기존 `wal_io/plan4_apply.rs`로 이전하고 원 파일을 제거했다. `DiskSession`·`ReadPort`·`MutationPort`·`MaintenancePort`의 실제 정의와 private 필드는 apply가 소유하며 기존 이름은 새 정의의 역방향 alias다. `plan4_recovery.rs`는 새 consuming 전환과 반환 타입을 사용한다. 원 Plan4Pages 전체·pool·control/history/flush/visibility·WAL/fence/loan과 실패 반환·Drop 의미는 유지한다. 새 소비 경로는 `storage::disk`이며 기존 READY receiver인 `Plan4Recovered` 타입만 선택 재노출한다.

production의 `plan4_pages::apply`와 test의 `plan4_pages::retention`은 상호 배타적인 선언으로 같은 파일을 읽는다. test는 `retention as apply` 이름도 제공해 기존 selector를 보존한다. 실제 조상에 대한 상대 visibility를 유지하며 별도 시험용 효과·권한 구현은 없다.

아직 이전하지 않은 실제 helper는 `plan4_retention/`의 `create_pair`(Catalog live/commit 포함), `visibility`, `control`, `flush`, `history`, `live`, `load`, `lookup_load`, `population`, `read`, `recovery`, `resources`와 각 자손이다. 이 파일들은 apply의 private 자손으로 연결되어 원 상태를 사용한다. Catalog CREATE/조회/commit의 제품 본문 분리와 C2/C3 공급은 남아 있으며 이 목록을 최종 독립 소비자 배치로 인정하지 않는다.

신규 source는 `plan4_apply/owner_tests.rs`, compiler 입력은 `tests/fixtures/dev3-module-c1/owner-compiler/`의 정상·private 필드·수명 거절3입력과 manifest다. 정확한 새/수정/이전 경로와 잔여 helper는 관리 JSON의 `dev3_module_c1_owner`에 등록했다. 기존 현재 경로 inventory만 갱신하고 과거 증거의 원 source 경로·hash는 보존한다. [선택 검증·독립 한정 수용](../evidence/module-independence-survey-20261026.md#c1-owner-ports)은 컴파일4건과 신규 메모리2건이며 전체 C1·복구 runtime 인수는 아니다. package/dependency·source root·clean export 규칙은 유지한다.

## 모듈 독립 개발 C1 control·isolation 소유 분리 — 2026-10-26

기존 `plan4_retention/control.rs`의 영속 byte 원장과 method는 `RetentionLedger`가 소유한다. 같은 apply root의 `RetentionControl`이 이 원장과 `Mutex<IsolationLedger>`를 직접 보유하고, MutationPort는 원장의 exclusive borrow와 mutex 공유 borrow, ReadPort는 같은 mutex 공유 borrow를 받는다. 기존 helper는 원장만 수신하며 전체 control을 다시 대여하지 않는다.

새 `plan4_apply/isolation.rs`는 별도4160B caller backing의 초기화만 소유한다. fresh는 원 preflight 성공 후, recovery는 원 pool/source 검산 후 같은 core/binding/fence에 연결한다. `EmptyRetentionControlLoan`의 필수 isolation 필드와 recovery의 named `[12144576,12148736)` 구간이 동일 loan을 전달한다. 이 구간은 기존 private 바로 앞이며 ATT/replay 및 private/visibility offset은 유지한다. live와 recovery 자원 회계에 실제 backing4160B 및 새 wrapper/mutex/loan carrier를 반영했다.

후속 [C1 epoch backing](../evidence/module-independence-survey-20261026.md#c1-epoch-backing)은 기존 `visibility.rs`의 원 publication mutex에 private `Publication { q, epochs }`를 둔다. `VisibilityLoan`이 registry8192B와 미활성 epoch1600B를 함께 전달하며 recovery named `[12142976,12144576)` 구간을 사용한다. 기존 구간은 유지하고 전체 carrier/backing을 회계한다. 실제 source에서만 초기 Q를 받는 `SourceFrontier`는 READY 이전 복구 cleanup binding과 최종 table seeding을 혼동하지 않도록 이름을 정정한 것이다. 신규 fixture3개는 JSON `dev3_module_c1_epoch`에 등록했다. 후속 [Catalog epoch 활성화](../evidence/module-independence-survey-20261026.md#c1-epoch-activation)는 private `visibility/epochs.rs`의 seeding·Reserve/Cancel/CLR·Commit 변경목록을 원 adapter에 연결했다. 새 source/manifest는 JSON `dev3_module_c1_epoch_activation`에 등록한다. 거래/statement·Reserved 관측 수명·성공하는 빈 Commit과 native/full recovery 인수는 남아 있다.

새 `tests/fixtures/dev3-module-c1/isolation-compiler/`에는 정상 입력·버퍼 재사용 거절 입력·manifest를 둔다. 새/변경 경로는 JSON의 `dev3_module_c1_isolation`에 등록하며 package/dependency·source root를 추가하지 않는다. [선택 증거](../evidence/module-independence-survey-20261026.md#c1-isolation-backing)는 compiler3와 변경/신규 runtime4의 한정 수용이다. 이 단위 이후의 epoch1600B·Catalog 활성화 공급은 위 후속 기록을 따른다. 거래·statement·credit/admission 동작, native/full recovery runtime 및 전체 C1은 후속 의무다.


[C1 읽기 전용 수명](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)은 `plan4_apply/transaction.rs`가 원 isolation/visibility의 capsule/view·slot/credit·snapshot 수명을 소유하고 `storage::disk`가 제한 API를 재노출한다. 새 실제 경로와 선택 입력은 JSON `dev3_module_c1_read_only`에 등록했다. writable/실제 조회/자동 read-set·short aggregate admission은 미공급이며 이 부분공급으로 C1 전체나 A/B 착수를 선언하지 않는다.


## 모듈 독립 개발 C2 동기 Catalog 조회 분리 — 2026-10-26

`storage/catalog/{mod,read}.rs`는 snapshot visible/retired/Missing/FlushRequired의 실제 제품 선택을 소유한다. 원 `plan4_retention/read/lookup.rs`의 동기 entry가 private `read/operation.rs`를 구성해 이 본문에 즉시 인계한다. operation은 원 ReadPort·Owner·SnapshotGuard·private loan·reference lease를 빌리고 source/Status/Undo/load/cleanup을 소유한다. 제품은 `storage::disk`의 opaque 타입/checked method와 순수 값을 소비하며 deep path·원 pool/registry 필드를 수입하지 않는다.

statement factory·자동 read-set·CheckedTable/RowReadOperation과 short aggregate는 아직 공급하지 않았다. caller가 operation을 보유하는 새 API는 원 source 예약을 함께 수신한 뒤 공개한다. 신규 source4개·compiler fixture3개/manifest 및 수정 경로는 JSON `dev3_module_c2_catalog_read`에 등록했다. [선택 증거](../evidence/module-independence-survey-20261026.md#c2-catalog-read)는 source별 compiler/privacy·runtime7과 원 source census/fixture FAIL을 보존한다. package/dependency·source root·clean export 규칙은 유지한다.


## 두 Row 제품의 C0 본문 — 2026-10-26

`storage/row.rs`는 A `row/insert.rs`와 B `row/read.rs`를 등록한다. A는 borrowed 값 검산과 `insert/leaf.rs`의 RowLeaf 후보, B는 순수 point/reseek·가시성·진행값 본문을 소유한다. A/B 전용 tests·fixture는 각각 독립하며 공용 구현·권한 factory를 추가하지 않는다. 실제 adapter와 예약/설치·snapshot/cleanup·Forward/CLR·복구는 후속 공급이다.

[작성·단독 검증 증거](../evidence/module-independence-survey-20261026.md#consumer-c0-start)에 같은 기준에서 겹친 제품 작성 구간과 각 첫 source의 상대 코드/fixture 제거를 기록했다. A 선택10개·B 선택3개는 C0_ONLY 범위이며 전체 제품 인수가 아니다. 실제 경로는 JSON `dev3_module_row_c0_consumers`에 등록하고 새 package/dependency·source root는 추가하지 않는다.


## 모듈 독립 개발 C2 조회 수명과 Commit Drain — 2026-10-26

`transaction/concurrency_read.rs`는 기존 PageConcurrency의 슬롯 표시와 checked read-operation ticket을 소유한다. Catalog adapter는 이 ticket과 원 ReadPort borrow를 보유하며 refs 정리 뒤 표시를 반환한다. 새 owner/counter·package/dependency·source root는 추가하지 않는다. 실제 경로는 JSON `dev3_module_c2_read_operation`에 등록했다. [선택 증거](../evidence/module-independence-survey-20261026.md#c2-read-operation-lifetime)는 source별13개 결과와 원 compile/fenced-fixture 실패를 구분한다. aggregate byte admission·caller-retained factory는 후속 공급이다.


## 모듈 독립 개발 C2 Catalog 후보 scratch — 2026-10-27

기존 `semantic_catalog/images/catalog.rs`의 record scratch를 caller after-page loan으로 옮기고 `semantic_catalog/resources.rs`의 실제 carrier 식을 맞췄다. 새 제품 모듈·package/dependency·source root는 없다. 선택 manifest와 수정 경로는 JSON `dev3_module_c2_catalog_scratch`에 등록한다. [실행 기록](../evidence/module-independence-survey-20261026.md#c2-catalog-scratch)은 local buffer 제거와 전체 source peak 불변, MT03의 잘못된 복구 실행 설명 정정을 구분한다. aggregate byte admission은 후속 공급이다.


## 모듈 독립 개발 C2 거래·Aging 생성 수명 — 2026-10-27

기존 `semantic_catalog/ledger.rs`·`transition.rs`의 descriptor 생성/검산 수명을 helper 경계로 나누고 `resources.rs`의 argument·반환 carrier 식을 맞췄다. 기존 owner 시험에 test-only branch 관측을 추가했다. 새 제품 모듈·package/dependency·source root는 없다. 선택 manifest와 수정5경로는 JSON `dev3_module_c2_transaction_phases`에 등록한다. [실행 기록](../evidence/module-independence-survey-20261026.md#c2-transaction-source-phases)은 r1 거래 분리의 peak 불변과 r2 Aging 분리의 실제 source 상한 감소, source별 선택12+7회를 구별한다. 전체 동시 admission과 Row 실제 공급은 후속이다.


## 모듈 독립 개발 C2 Commit Pending 보유 — 2026-10-27

기존 `dirty_frame/commit.rs`는 원 PinSet만 Pending에 이동하고 Drain 뒤 원 installer의 guard storage를 재구성한다. 기존 `plan4_retention/catalog_commit.rs`의 private stage helper가 실제 stage 지역값의 수명을 끝낸다. 신규 package/dependency·제품 파일·source root는 없다. 수정3경로와 선택 manifest는 JSON `dev3_module_c2_pending_carrier`에 등록한다. [실행 기록](../evidence/module-independence-survey-20261026.md#c2-pending-carrier)은 선택11개, Pending 타입112B 감소와 새 반환 carrier를 포함한 포괄 source 합을 구별한다. 실제 phase별 aggregate admission은 후속이다.


### C2 전체 Catalog 조회의 short source 예약

`plan4_apply/source.rs`는 원 isolation backing의24..32에 조회 바이트를 예약하고 마지막 output 변환/Result 뒤 checked 반환한다. `source_tests.rs`와 실제 Catalog 동시 호출 시험, 선택 manifest의 신규 경로는 JSON `dev3_module_c2_short_source`에 등록했다. package/dependency·source root·clean export 범위는 유지한다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-short-source)는 CatalogFull 한 class의 source별17개 PASS와 원 compile FAIL을 보존한다. 다른 입구·writer/progress·resident class의 동시 합은 후속이며 전체64KiB 보장이나 M4/M5 완료가 아니다.


### C2 상주 Catalog 조회의 고정 class

`plan4_retention/read/resident.rs`는 원 allocator copy/Status·Undo observation만 제공하며 기존425984B loan을 사용한다. 원 product branch와 source ledger는 Full/Resident class를 구별한다. 신규 실제 시험2파일·선택 manifest와 수정 경로는 JSON `dev3_module_c2_resident_read`에 등록했다. package/dependency·source root·clean export는 유지한다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-resident-read)는 선택9개 PASS와 원 compile/census FAIL, 상주 두-reader55976B를 보존하며 writer/progress의 전체 동시 합산은 후속이다.


### C2 최종 source 반환과 read progress 수명

기존 `read/operation.rs`의 refs 정리와 source/progress 정리를 분리했다. 새 실제 시험과 선택 manifest는 JSON `dev3_module_c2_source_return`에 등록했다. package/dependency·source root·backing·wire 변경은 없다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-source-return)는 신규4·cleanup 변경 영향3의 선택7개 PASS를 보존하며 source-before-progress 진입 틈과 전체 writer/progress 합산을 후속으로 남긴다.

### C2 Commit phase 관측 등록 — 2026-10-27

기존7개 Rust source에 cfg(test) 관측만 추가했고 신규 manifest는 JSON `dev3_module_c2_commit_phase_observation`에 등록했다. 제품 charge·admission·package/dependency·source root·wire는 유지한다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-commit-phases)는 test build/신규 관측1 PASS와 원 compile FAIL을 보존하며 실제 Pending layout 축소를 후속으로 남긴다.

### C2 Pending image 수명 분리 등록 — 2026-10-27

기존4개 Rust source의 Prepared/payload·stage 소비·공통 control 입력과 관측 시험을 수정했다. 신규 manifest는 JSON `dev3_module_c2_pending_image`에 등록했다. package/dependency·source root·backing·wire 변경은 없다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-pending-image)는 관련12 PASS와 원 compile FAIL, Pending 자료 감소와 stage 반환 상한 증가를 함께 보존한다. 전체 source admission은 후속이다.

### C2 source 진입 수명 등록 — 2026-10-27

기존 read operation의 source/progress 진입 순서와 source 실패 정리만 변경했다. 새 actual 시험·manifest는 JSON `dev3_module_c2_source_entry`에 등록했다. package/dependency·source root·owned type/census·wire는 유지한다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-source-entry)는 production/test build·진입 관련5 PASS를 보존하며 전체 writer/entry/return source 합산은 후속이다.

### C2 Aging/Root source 계산 등록 — 2026-10-27

기존6개 Rust source의 실제 호출 상한·공통 codec 계산과 신규 관측을 변경했다. 신규 manifest는 JSON `dev3_module_c2_aging_source`에 등록했다. package/dependency·source root·backing·wire 변경은 없다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-aging-source)는 source 관련3 PASS·원 경고와 import-only 후속 compile을 구분하며 전체 writer/read/progress 합산은 미완료다.

### C2 WAL wait source 관측 등록 — 2026-10-27

기존7개 Rust 파일에 cfg(test) 관측/신규1을 추가했고 신규 manifest는 JSON `dev3_module_c2_wal_wait_source`에 등록했다. package/dependency·source root·제품 상계/charge·wire 변경은 없다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source)는 물리/semantic/caller 반환 phase와 source별1 PASS를 보존하며 실제 source admission은 후속이다.

### C2 실제 Drained 소비 경계 등록 — 2026-10-27

원 dirty_frame/Commit에 private Drained carrier와 consuming drain/resume를 공급하고 Catalog actual caller·source 계산을 연결했다. 신규 실제 시험/manifest와 수정7경로는 JSON `dev3_module_c2_drained_handoff`에 등록했다. package/dependency·source root·backing·wire는 유지한다. [한정 증거](../evidence/module-independence-survey-20261026.md#c2-drained-handoff)는 신규4·관련4·source2 PASS와 반환 비용 증가를 보존하며 source phase 예약은 후속이다.

### C2 Catalog CallScope 등록 — 2026-10-27

원 ReadPort의 scope factory와 private 원 admission 소유자를 공급하고 Operation refs-only 정리·legacy checked result·selected query 수명을 연결했다. 실제 scope/시험/compile fixture7개 신규 경로와 수정8경로를 JSON `dev3_module_c2_call_scope`에 등록했다. package/dependency·source root·backing·wire는 유지한다. [source별 증거](../evidence/module-independence-survey-20261026.md#c2-call-scope)는 runtime14+1+2·compiler4 PASS, 원 fixture FAIL과 미반복을 보존하며 전체 entry/tail·writer/progress admission은 후속이다.

### C2 descriptor budget 행 borrow 등록 — 2026-10-27

원 obligation의 행 반복/values를 borrow로 바꾸고 allocation source의 iterator 계수를 연결했다. 실제 수정2경로와 선택 manifest1을 JSON `dev3_module_c2_borrowed_budget_rows`에 등록했으며 package/dependency·source root·wire·backing은 유지한다. [증거](../evidence/module-independence-survey-20261026.md#c2-borrowed-budget-rows)는 동일 source/ELF의5+5 PASS, Aging harness identity 원 FAIL과 build/완료 사례 미반복을 보존한다. 실제 전체 admission은 미완료다.

### C2 readonly replay source 등록 — 2026-10-27

기존 allocation/INIT source3경로와 source 관측1을 수정하고 선택 manifest1을 JSON `dev3_module_c2_readonly_replay_source`에 등록했다. package/dependency·source root·backing·wire는 유지한다. [선택 증거](../evidence/module-independence-survey-20261026.md#c2-readonly-replay-source)는 production/test build·source별6+계수 보완1 PASS와 Full35895B를 보존하며 전체 entry/tail·writer admission은 후속이다.

### C2 작은 Catalog 진입·정리 경계 등록 — 2026-10-27

actual health/source/Scope/legacy facade·원 progress consuming finish와 관련 시험11경로를 수정하고 선택 manifest1을 JSON `dev3_module_c2_compact_call_boundary`에 등록했다. 기존 authored compiler4는 파일을 복제하지 않고 원 fixture를 참조한다. package/dependency·source root·backing·wire는 유지한다. [원 증거](../evidence/module-independence-survey-20261026.md#c2-compact-call-boundary)는 production/test build·runtime25·compiler4 PASS와 Full35619B를 보존하며 전체 bound floor·writer/progress 예약은 후속이다.

### C2 replay 작업 공간 대여 등록 — 2026-10-27

기존 semantic transition/source·history/recovery 연결·선택 관측8경로를 수정하고 실제 fixture1을 JSON `dev3_module_c2_borrowed_replay_workspace`에 등록했다. package/dependency·source root·backing·wire는 유지한다. [source별 증거](../evidence/module-independence-survey-20261026.md#c2-borrowed-replay-workspace)는 선택9 PASS·원 compile FAIL/경고와 import-only build를 구별하며 전체 entry/tail·writer admission은 후속이다.

### C2 split 고정 source 계산 등록 — 2026-10-27

기존 port/source/Scope/실제 관측/live·복구 계수7경로를 수정하고 선택 fixture1을 JSON `dev3_module_c2_split_source_charges`에 등록했다. package/dependency·source root·backing·wire는 유지한다. [원 증거](../evidence/module-independence-survey-20261026.md#c2-split-source-charges)는 build/runtime16 PASS와 원 compile FAIL2, source별 계산graph 보완을 구별하며 전체 entry/tail·writer admission은 후속이다.

### C2 예약 밖 호출 phase 관측 등록 — 2026-10-27

기존 Scope·core·pool/registration·visibility·isolation8경로의 작은 실패 helper/cfg(test) 관측과 선택 fixture1을 JSON `dev3_module_c2_uncharged_call_phases`에 등록했다. package/dependency·source roots·backing·wire는 유지한다. [원 증거](../evidence/module-independence-survey-20261026.md#c2-uncharged-call-phases)는 r1 production PASS/test compile FAIL/runtime NOT_RUN과 r2 test build/선택4 PASS·생산 미반복을 구별한다. 관측 floor는 제품에 연결하지 않았으며 전체 reader/writer admission은 미완료다.

### C2 query 종료 경계 등록 — 2026-10-27

기존 Scope·actual query·call_scope3경로와 선택 fixture1을 JSON `dev3_module_c2_query_tail_boundaries`에 등록했다. package/dependency·source roots·backing·wire는 유지한다. [증거](../evidence/module-independence-survey-20261026.md#c2-query-tail-boundaries)의 production/test build·선택5 PASS는 changed finish_error와 query 경계에 한정한다. 정상710B/오류1394B 관측은 전체 reader/writer admission 미공급과 구별한다.

### C2 compact DPT 등록 — 2026-10-27

현재 private DPT representation·producer/flush/진단7기존경로, 신규 표현시험/actual Pair cut시험/선택 manifest3경로를 JSON `dev3_module_c2_compact_dpt`에 등록한다. package/dependency·source root·caller backing·wire는 유지한다. [실행 증거](../evidence/module-independence-survey-20261026.md#c2-compact-dpt)의 source별2+12선택 PASS와 원 compile/fixture FAIL을 구별한다. private 배열3072B 감소와 actual helper660B 비용 연결은 전체 aggregate admission 완료가 아니다.

### C2 LOAD claim borrow 등록 — 2026-10-27

실제 reader_load/retention_residency2기존경로와 선택 fixture1경로를 JSON `dev3_module_c2_borrowed_load_claims`에 등록한다. 같은 registry의 참조 검산이며 원 권한·backing·source roots·package/dependency는 유지한다. [선택4 증거](../evidence/module-independence-survey-20261026.md#c2-borrowed-load-claims)는 해당 actual LOAD와 변경 관측에 한정한다.

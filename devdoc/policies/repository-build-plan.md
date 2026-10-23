# XADB — 저장소 구성과 빌드 정책

2026-10-18 · **배치 정책 채택 / DEV0·DEV1 구현 / DEV2 부분 구현·진행 중**


<a id="section-1"></a>

## 1. 결정과 적용 범위

하나의 Cargo workspace에서 제품 코드와 XADBTest를 개발하고, 모든 설계 문서는 `devdoc/`에서 관리한다. 현재 9개 crate를 사용하며 DEV1의 공통 계약과 OS 구현을 `xadb-foundation`·`xadb-platform`으로 분리한다. DEV2의 독립 oracle `xadbtest-reference`를 먼저 추가했다. `xadb-engine`은 K0 volatile substrate를 구현 중이며 `xadbtest-engine-worker`는 native adapter를 구현 중이다. [진행 기록](../implementation/dev2-implementation.md)을 따른다. 거래·저장·catalog·WAL/recovery는 우선 `xadb-engine`의 내부 모듈로 나누고 공개 경계를 검증한 뒤 필요할 때 crate를 분리한다.

이 문서는 사용자 요청에 따라 기존 경로와 변경된 XADB 설계를 연결한 **저장 위치·빌드 책임 결정**이다. K0–K5, DEV0–DEV9/COMM/PART/NG 순서와 kernel·recovery·버전 의미는 바꾸지 않는다. 후속 crate 이름은 구현 단계에서 재검토할 예약 방향이며, 빈 crate나 placeholder 폴더를 미리 만들지 않는다. 경로가 계획에 있다는 이유로 구현·시험·배포 완료로 표시하지 않는다.

우선순위는 [하위 계층 개발 결정](../plans/bottom-up-development.md), [단계 JSON](../plans/development-stages.json), [Rust 구현 계약](../contracts/implementation-contracts.md), [버전 정책](version-compatibility-policy.json)이다. DEV0/DEV1의 실제 명령·제약은 각각의 구현 문서와 manifest를 따른다. 이름·API 문서·리뷰는 [코딩 컨벤션](coding-convention.md)을 따른다. 종합 보고서 5.2절도 이 문서의 실제 초기 배치를 사용한다. 초기 제안의 `xadb-types/xadb-txn/xadb-storage/xadb-durable/...`는 역할 이름이었으며 독립 Cargo package 예약이 아니다.

제품 폴더는 `crates/foundation`, `crates/platform`, 후속 `crates/engine`처럼 **제품명 접두어 없이 역할로 명명**한다. Cargo package 이름은 별도 식별자로 `xadb-foundation`, `xadb-platform`, `xadb-engine` 등을 유지한다. `cargo -p`에는 package 이름을, workspace member와 path dependency에는 실제 경로를 쓴다. 독립 시험 도구의 패키지·명령 이름인 `xadbtest` 계열은 유지하고, 폴더는 `crates/testing` 아래 역할별로 묶는다. XADB package·도구 이름으로 이관하고 새 소스 fingerprint와 실행 증거를 생성한다.

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
| xadb-sql | crates/sql | PLANNED | DEV4 | xadb-foundation, xadb-engine |
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

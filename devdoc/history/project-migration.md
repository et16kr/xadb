# XADB 독립 저장소 이관 기록

2026-10-18 · **파일 이관 완료 / 새 저장소 검증은 증거 색인 참조**

사용자 지시에 따라 기존 개발 작업 공간을 그대로 두고 새 XADB 저장소에 현재 파일을 복사했다. 미커밋 DEV2 변경을 포함하며 이전 `.git`, build 산출물, 실행 artifact는 가져오지 않았다. 복사 직후 원본 파일별 SHA-256을 대조해 복사 중 변경이 없음을 확인했다. 원본의 이후 독립 작업은 이 스냅샷에 자동 반영되지 않는다.

| 구분 | 처리 |
| --- | --- |
| DEV0/DEV1 구현·시험 | 이관, product package는 `xadb-*`, 도구는 `xadbtest*`로 명명 |
| DEV2 P0/K0 | 독립 이력 reference, volatile 행/catalog, frame, native worker와 시험, 미검증 streaming controller 초안 이관 |
| 저장 범위 | 디스크 엔진 단일 구성으로 계획·계약·인수 항목 재작성 |
| K0 임시 행 상태 | `crates/engine/src/storage/volatile.rs`로 명시; DB 내구성 기능으로 표시하지 않음 |
| 문서 | 개발 정책, 디스크에 적용 가능한 kernel/recovery 계약, SQL·사용자 매뉴얼·운영 초안, 기능 작성 목록 이관 |
| 제외 | 독립 resident engine와 혼합 저장 설계·시험 계획, 이전 제품 소스 조사·대체 전용 조건 |
| 과거 실행 증거 | 원본 작업 공간에 보존. XADB source hash와 제품명으로 재표기하지 않음 |
| 신규 검증 | XADB에서 실제 실행하여 새 보고서 생성; 문서 검사와 제품 시험을 분리 |
| Git·공개 | 원래 Git 이력 미이관; 새 이력으로 시작하는 Public 개발 저장소 |

[기계 판독 이관 기록](../document-migration.json)에 snapshot revision·시각·inventory digest·검증 상태를 보존한다. 원래 경로가 포함된 원본 파일별 inventory는 저장소 밖의 로컬 보관소 `$HOME/.local/state/xadb/migration-archive/<inventory SHA-256>/source-inventory.json`에 바이트 단위로 보존한다. 구현 파일의 원래/현재 경로·hash 대응은 [구현 이관 inventory](../evidence/implementation-import-inventory.json)에 보존한다. 현재 지원 범위는 [구현 색인](../implementation/README.md), 이번 실행은 [증거 색인](../evidence/README.md)을 따른다. 이관한 설계의 향후 참고 방향을 공개 DBMS로 정한 것이며 과거 설계나 측정의 출처를 다른 DBMS로 바꿔 주장하지 않는다.

## 후속 K0 이관 — 2026-10-18

**D2 / 후속 코드 이관 완료·XADB 검증 인계.** 사용자의 추가 이관 지시에 따라 중립 source snapshot `development-snapshot-20261018-02`의 후속 변경만 반영한다. commit은 만들지 않는다. 기존69개 구현 이관 mapping의 source hash와 새 snapshot을 비교해 executable 입력18개(수정9개·추가9개)를 선정했다. 대상의 해당 기존 파일은 최초 imported hash와 같았으며, 독립 DEV3 설계·프로젝트 방향·storage/volatile 구현은 변경하지 않는다.

범위는 K0-003 streaming controller의 deadline·admission·ACK 기록 보완과 실제/부정 시험, K0-004 독립 catalog interpreter·versioned recipe·run/verify CLI와 새 회귀 wrapper다. `xadbtest`→`xadbtest-reference`는 normal dependency, worker→controller는 dev-dependency로 구분한다. 새 외부 dependency는 없다. 현재 이름·환경 변수·오류 식별자는 XADB 규칙으로 변환하며 원래 제품명은 대상 파일에 포함하지 않는다.

작업·계약·통합 담당은 `/root`, 별도 검토는 `/root/dev2_independent_review`다. 공통 Cargo·wrapper·배치·문서 검사기는 root가 순차 통합한다. 검토자는 읽기 전용이며 원본 checkout·이전 Git 이력·기존 r1 증거 및 최초 이관 inventory는 수정하지 않는다. 새28-check wrapper와 대응하는 검증 가드를 이관했다. 실제 검증과 새 증거 등록은 XADB 담당이 수행한다. 필수28개 check는 고정 목록이며 누락·중복·scope 위조를 허용하지 않는다.

원본243개 파일의 경로/hash와18개 delta의 전체 inventory는 저장소 밖 로컬 보관소에 보존했다. inventory SHA-256은 `b0e4c27c55bc99a25ae2d9059feab820dcb9665ec6833ceca2b2302e23a1453a`다. 이 hash는 이관 출처를 식별하며 XADB 실행 source fingerprint나 검증 PASS를 대신하지 않는다. K0-005 성능 작업은 설계 준비와 검토 미완 사항만 인계하며 구현하지 않는다.

[후속 구현 이관 inventory](../evidence/implementation-incremental-import-20261018.json)는 추가18개 파일의 현재 hash와 최초 이관 hash를 구분한다. 최초69개 mapping과 r1 실행 보고서는 당시 기록으로 그대로 보존한다.


사용자가 검증은 XADB 작업에서 수행하도록 지정하여 진행하던 `check-kecbzonw` 실행을 중단했다(exit130). 이 실행은 완료 보고서가 없고 일부 성공 log도 전체 PASS로 승격하지 않는다. 기존 r1 실행 보고서·현재 문서 ID는 변경하지 않았으며 해당 snapshot의 과거 근거로 남긴다. 현재 source fingerprint의 검증은 미완료이므로 후속 담당이 새 실행 결과를 등록해야 한다. 문서 검사·제품 시험은 추가 실행하지 않았다.

이관 자체의 읽기 전용 검토에서는18개 executable delta의 원문과 이름 변환·파일 hash, 기존 runtime 독립 변경 보존, reference 의존 분리와 프로젝트 방향·DEV3·초기 이관 증거 보존을 확인했다. 추가 구현이나 성능 작업을 시작하지 않았고 Git commit도 만들지 않았다.


<a id="testing-layout"></a>

## 시험 도구 경로 재구성 — 2026-10-18

**D2 / 구현·제품 회귀·독립 검토 완료.** 사용자가 제안된 `crates/testing` 배치와 관련 문서 갱신을 명시 요청했다. [XADB-LAYOUT-TESTING-001](../policies/repository-build-plan.md#testing-layout)이 기존 평면 시험 폴더 예외를 대체한다. 제품 의미·package/target 이름·feature·의존 관계는 유지한다.

작업·계약·통합 담당은 `/root`다. 기준 commit은 `4a923214dd91c5566779652e59e13883eb940672`, 기존 tracked 및 nonignored untracked 파일의 작업 트리 digest는 `ce6e2f244f68144f46006e9f7dc25d23878b4fcf9f08a0096d15b5ff71785434`다. digest는 정렬한 저장소 상대 UTF-8 경로, NUL, 파일 바이트를 순서대로 SHA-256에 입력했다. 착수 시 기존 미커밋 K0 이관·DEV3 문서 변경을 확인하고 작업 전 사본을 보존했다. 기존 파일 내용은 이동 후 유지하고 아래 경로 수정만 순차 적용한다. 다른 checkout이나 원래 Git 이력은 수정하지 않는다.

| 이전 경로 | 현재 경로 |
| --- | --- |
| `crates/xadbtest` | `crates/testing/controller` |
| `crates/xadbtest-protocol` | `crates/testing/protocol` |
| `crates/xadbtest-reference` | `crates/testing/reference` |
| `crates/xadbtest-fixture-worker` | `crates/testing/fixture-worker` |
| `crates/xadbtest-foundation-worker` | `crates/testing/foundation-worker` |
| `crates/xadbtest-engine-worker` | `crates/testing/engine-worker` |

수정 범위는 여섯 시험 패키지의 이동과 상대 dependency·fixture·build-root 경로, 루트 Cargo manifest, DEV2 checker의 profile source 경로, 배치·명명 Markdown/JSON 및 현재 경로를 설명하는 문서다. 공유 Cargo·정책·문서 색인·검증 결과는 root가 순차 반영하고 별도 검토자는 읽기 전용으로 확인한다. 반영 순서는 소유 정책 → 패키지 이동·경로 조립 → 통합 회귀 → 실행 증거 등록·문서 검사다. 최초/추가 이관 inventory와 r1 실행 보고서는 당시 identity로 바이트 단위 보존한다.

제품 검증 계획은 이동 전후 Cargo metadata의 package/target/feature/의존 비교와 `python3 scripts/check_dev2_reference.py --with-substrate`에 포함된 DEV0·DEV1 회귀 및 현재 P0/K0 leaf다. 착수 후 별도 작업에서 문서 검사 시점을 [DOC-002](../policies/documentation-policy.md#push-gate)의 pre-push로 변경했으므로 최종 문서 자동 검사는 push 시점에 적용한다. source fingerprint·clean export 루트는 그대로이며 실제 새 빌드와 깨끗한 소스 복사본 실행으로 확인한다. 제품 durability·DEV2 전체 완료를 주장하지 않는다. 독립 리뷰·실행 결과는 아래 기록을 따른다.

병행 작업에서 추가된 push 훅·검사 스크립트와 정책 변경은 보존한다. 최신 문서 정책을 발견하기 전에 한 번 실행한 문서 검사는 기존 r1 보고서가 현재 source 및 후속 K0 check inventory와 다르다는 오류를 보고했다. 이 결과를 PASS로 바꾸지 않으며, 새 제품 실행 증거를 별도로 등록한 뒤 문서 검사는 push gate에서 수행한다.

실제 실행은 `artifacts/dev2-reference/check-odsnj6yj/validation.json`에서 PASS했다. DEV0 8개·DEV1 8개·DEV2 leaf 28개 check를 통과했고 source SHA-256 `3fa1b7958cc5597ba99be6ba81d75ea65360657d318d22d59e6e2b633f76a540`가 실행 전·빌드·실행 후 동일하다. 새 [DEV0](../evidence/dev0-xadb-20261018-r2.json)·[DEV1](../evidence/dev1-xadb-20261018-r2.json)·[DEV2](../evidence/dev2-xadb-20261018-r2.json) 보고서는 실행 산출물의 원본 바이트를 복사했다. 현재 문서 alias는 r2로 갱신하고 r1은 원래 경로 ID와 바이트를 유지했다.

이동 전후 42개 파일을 대조했다. 경로 보정이 필요한 5개 파일(Cargo manifest 2개, build script, fixture include, catalog helper include) 외에는 내용이 같다. Cargo metadata는 이동 경로와 경로 기반 package-ID 표기를 정규화하면 9개 package·target·feature·의존이 동일하며, Cargo.lock도 착수 시 내용과 같다. 별도 검토 `/root/testing_layout_review`가 구조 변경과 실제 보고서·로그 hash·현재 source 일치·문서 참조를 확인했고 최종 차단 지적 없음으로 완료했다. 검토자는 파일 수정이나 시험·문서 검사기를 실행하지 않았다.

사용자는 단순 경로 재배치에 전체 기능 회귀·확장 시험을 적용한 과도한 검증 범위를 지적했다. 이번 실행 사실은 그대로 보존하되, 후속 경로·이름 정리는 실제 영향에 맞춰 패키지 연결·빌드·참조 확인 범위를 정하고 기능 개발의 전체 시험을 기계적으로 적용하지 않는다.

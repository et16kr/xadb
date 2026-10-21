# 문서 운영 기준

2026-10-22 · **DOC-001 문서 운영 / DOC-002 push 시점 검사 / DOC-003 Rust 포맷 / DOC-004 Clippy 추가**

[문서 진입점](../README.md) · [개발 운영 방침](development-policy.md#section-3) · [기계 판독 색인](../documents.json)

## 읽기와 판단 순서

1. `devdoc/README.md`에서 현재 구현 상태와 작업별 읽기 경로를 확인한다.
2. 코딩 컨벤션·개발 운영 방침·이 문서는 상시 기준이다. 저장소 배치·병렬 조정·AI 작업 절차는 해당 변경 때 읽는다.
3. 작업의 개발 단계와 소유 계약을 고른 뒤 관련 API·시험·실행 증거를 확인한다.
4. 종합 보고서·매뉴얼 초안·원문 이름 색인·과거 조사는 필요한 절만 조회한다. 전체 참고 자료의 완독을 모든 작업의 선행 조건으로 만들지 않는다.

사용자 지시와 유효한 위임 → 해당 주제의 채택된 소유 계약 → 구현 명세 → 제안·예시 순서로 해석한다. 소스와 규범이 다르면 차이를 기록하고, 관측만으로 규범을 바꾸지 않는다. 동급 결정의 충돌은 DP02에 따라 대체 관계를 명시한다.

## 형식과 단일 원문

- **Markdown을 문서 원문으로 사용한다. HTML 문서·템플릿·생성기를 추가하거나 재생성하지 않는다.** HTML 재도입은 사용자의 명시적 변경 요청에 따른다.
- Markdown과 중복된 HTML은 보관하지 않는다. HTML만 있는 자료를 편입할 때 본문·표·원문 발췌·필요한 데이터 보기를 Markdown으로 옮긴다.
- JSON은 실제 검증기가 소비하는 ID·상태·관계·수치·증거를 관리한다. 긴 규범 설명을 복제하지 않는다. 기존 JSON이 의미 계약의 소유자인 경우 Markdown에서 그 정본을 연결한다.
- 문서마다 상태와 범위를 앞부분에 표시한다. 채택 여부, 구현 여부, 실행 결과를 따로 관리한다. 작성 목록은 명세 완료가 아니며 문서 검사 PASS는 제품 시험 PASS가 아니다.
- 같은 정책은 소유 문서 한 곳에서 관리한다. 다른 문서는 짧은 설명과 원문 링크를 사용한다. 새로운 날짜 폴더에 현재 규범을 계속 쌓지 않는다.

## 배치 기준

| 위치 | 내용 | 읽는 시점 |
| --- | --- | --- |
| `policies/` | 코딩·개발 결정·문서 운영·저장소·협업 규칙 | 상시 기준 및 해당 변경 |
| `plans/` | 범위·순서·단계 완료·작성 대상 | 작업 선정·착수·완료 판단 |
| `contracts/` | 커널·복구·저장·성능·검증의 의미와 인수 의무 | 해당 기능 변경 전 |
| `implementation/` | 실제 구현·제약·시험 명령 | 현재 코드 변경 전 |
| `reference/` | 종합 설명·매뉴얼/SQL 초안·세부 이름 색인 | 필요한 주제·절 조회 |
| `examples/` | 실행되지 않은 계약·재현·진단 예시 | 실제 작업 입력 작성 시 |
| `evidence/` | 조사·모델·실행 결과와 원래 identity | 주장·완료 상태 확인 시 |
| `history/` | 기존 제품 조사와 과거 설명 | 설계 이유·역사적 근거 조회 |

아직 구현되지 않은 모듈을 위해 빈 문서·crate를 만들지 않는다. 향후 `modules/`·`specs/`가 필요하면 실제 내용과 함께 추가하고 색인·저장소 배치를 갱신한다. 목록의 255개 주제와 FS/DS 슬롯은 파일 개수를 강제하지 않는다.

## 링크와 색인

Markdown은 실제 파일로 향하는 상대 링크를 사용한다. 절 참조는 문서에 명시한 안정 anchor를 우선하며 `section-N`으로 이관한 기존 anchor의 번호는 재사용하지 않는다. 새 절을 삽입해도 기존 anchor를 다시 번호 매기지 않는다.

`documents.json`은 문서 ID→현재 경로·역할의 기계 판독 색인이다. 기존 JSON의 `development-stages.json` 같은 등록 파일명은 **문서 ID**이며 JSON 파일 옆의 상대 경로가 아니다. 경로 이동 때 그 ID를 유지한다. 역사 자료에서 중복되는 파일명은 `history/.../파일명` 전체를 ID로 사용한다. JSON 내부 `#DEV2` 등은 해당 계약의 항목 ID이고 Markdown anchor와 구분한다. 새 참조는 등록 ID 또는 실제 저장소 경로로 명확히 쓴다.

새 문서는 해당 폴더 README와 `documents.json`에 등록한다. 주제·사용 시점·규범/제안/근거 구분이 드러나야 한다. 단순 helper마다 별도 문서·JSON을 만들지 않는다. 파일을 옮기면 관련 Markdown 링크·색인·검증 진입점·현재 작업 예시를 함께 갱신한다.

## 증거와 검증

과거 source 경로·revision·hash·실행 명령은 당시 증거의 identity다. 지금의 제품 경로로 소급 수정하지 않는다. `document-migration.json`은 이번 이관의 snapshot identity, 구현 경로·hash 대응, 제외·유지 범위와 신규 검증를 보존한다. 이관 당시의 고정 증거는 byte 단위로 보존하며 새 실행은 새 증거를 남기고 채택 참조를 별도로 갱신한다.

문서 자동 검사는 아래 push 시점에 적용한다. 편집·commit·문서 작업 종료마다 검사하거나 생성 결과를 갱신하지 않는다. 의미·불변식·설계 판단의 검토는 계속하며 자동 검사 PASS를 설계 타당성의 증명으로 사용하지 않는다. 제품 코드의 기능·회귀·복구 시험은 기존 단계별 의무를 적용한다.

<a id="push-gate"></a>
## Push 직전 Rust 포맷·Clippy·문서 검사

[pre-push 훅](../../scripts/git-hooks/pre-push)이 [push 검사기](../../scripts/check_push.py)를 실행한다. 기존 훅 설정이 없는 checkout에서는 한 번 등록한다. 기존 `core.hooksPath`나 사용자 훅이 있으면 덮어쓰지 않고 소유자와 연결 방식을 정한다.

```bash
git config --local core.hooksPath scripts/git-hooks
```

- 검사 대상은 Git이 전송하려는 각 ref의 최종 commit이다. annotated tag는 commit으로 해석하고 같은 commit은 한 번 검사한다. 여러 commit 중 하나라도 실패하면 해당 push 전체를 차단한다. 삭제만 하는 ref는 검사할 새 내용이 없어 제외한다.
- 각 commit을 `git archive`와 `tar`로 임시 디렉터리에 내보내 그 사본에서 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `python3 -B devdoc/verify_documents.py`를 순서대로 실행한다. 사본의 `rust-toolchain.toml`에 맞는 Cargo·rustfmt·Clippy, 고정 `Cargo.lock`과 crate 의존성, Python과 `devdoc/requirements.txt`의 검사 의존성이 필요하다. 포맷 불일치·lint 경고·검사 도구/manifest/lockfile/검사기 누락·해석할 수 없는 commit·export/실행 오류도 push 차단이다.
- 미커밋 변경은 검사 대상에 섞지 않는다. Clippy의 `CARGO_TARGET_DIR`는 export 밖의 별도 임시 target으로 고정하며 상속된 target 경로를 사용하지 않는다. 검사 중 생성하는 `document-validation.json`은 임시 사본에만 기록하고 사본과 target은 종료 시 정리한다. 작업 트리·index·기존 증거를 갱신하거나 검사 결과를 자동 commit하지 않는다. 출력에는 검사 commit과 결과를 남긴다.
- 포맷 검사는 자동 수정하지 않는다. 실패하면 작업 트리에서 `cargo fmt --all`로 수정하고 해당 수정을 commit에 포함한 뒤 다시 push한다. 로컬 미커밋 수정만으로 포맷이 잘못된 commit을 전송할 수는 없다.
- 현재 검사 범위는 Rust 포맷·Clippy와 링크/anchor·색인/역할·JSON/Python 구문·일부 단계/패키지/인수 계약·기존 증거 identity다. [DOC-004](#doc-004)의 Clippy는 Actions와 같은 workspace/target/feature 범위로 테스트 코드까지 정적 검사하지만 테스트 사례를 실행하지 않는다. 제품 기능 시험은 변경 코드의 필수 사례로 제한하며 전체 DEV0/1/2·leaf wrapper를 push 훅에서 실행하지 않는다. 설계 의미의 정확성·제품 실행·원격 보호 CI는 별도다.
- 이 훅은 설치한 로컬 checkout의 push 제약이다. 새 clone에도 설치가 필요하며 원격 서버가 강제하는 branch protection으로 주장하지 않는다. 기존 제품 CI를 대체하지 않는다.

평상시 문서 수정에서는 검사하지 않는다. 훅/검사기 자체를 변경할 때에는 실패를 실제 차단하는지 별도의 좁은 회귀 시험을 적용하고, 제품 단계 완료·배포에는 기존 검증 의무를 유지한다.

<a id="doc-004"></a>
## DOC-004 결정 기록

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22; 구현·선택 회귀·독립 검토 완료 |
| 근거·담당 | 사용자가 Actions의 `enum_variant_names`·`type_complexity` 실패와 기존 검사 누락을 보고. 작업·계약·통합 담당 `/root` |
| 원인 | DOC-003은 fmt와 문서만 검사하며 Clippy를 실행하지 않았다. 예약·취소 부분 구현의 Clippy도 당시 NOT_RUN으로 남아 있어 테스트 전용 Rust 코드의 lint가 원격 DEV0에서 처음 거절됨 |
| 선택·이유 | 기존 commit export의 fmt와 문서 검사 사이에 Actions의 Clippy 명령을 그대로 추가. `--all-targets --all-features --locked -- -D warnings`로 테스트 코드·비기본 feature·lockfile까지 확인하되 기능 시험은 실행하지 않음 |
| 대안 | fmt·문서만으로 Rust lint를 검출할 수 없음. package/feature 일부만 검사하면 Actions 범위와 달라 누락 가능. DEV0/1 wrapper 실행은 무변경 기능 시험까지 반복하므로 제외 |
| 소유·대체 | `scripts/check_push.py`, `scripts/test_check_push.py`와 안내·빌드 색인을 갱신. DOC-002의 시점·commit 선택·작업 트리 분리 및 DOC-003의 fmt 검사는 유지. 과거 결정·실행 증거는 소급 변경하지 않음 |
| 검증 | 변경 전 새 반례는 형식이 정상인 테스트 전용 lint 오류 commit의 push가 허용되어 FAIL. 변경 후 `scripts.test_check_push.PushGateTests`의 `test_clippy_checks_test_targets_and_committed_tip`, `test_clippy_checks_nondefault_feature_type_complexity`, `test_clippy_requires_committed_lockfile` 3개만 `python3 -B -m unittest`로 선택하여 PASS. 실제 고정 Clippy로 두 lint 거절·수정 commit 수락·dirty/index 분리·상속 target 격리·lockfile 누락 차단을 확인. 문서 verifier는 fixture이며 기존7개 시험·제품 전체 시험은 반복하지 않음 |
| 독립 검토·적용 | `/root/dev3_tree_design`이12개 파일 diff의 D1/D2 범위를 읽기 전용 검토하여 PASS, 필수 수정 지적 없음. hook 위치·panic·생산 경로·Copy capture 수명·commit/target 격리·명령/정책 일치를 직접 확인하고 RED·gate3·AR05/07·engine Clippy 결과는 총괄이 전달한 실행 출력과 대조. 검토자는 재실행하지 않았으며 `/root`가 반영 확인. 현재 checkout은 기존 `core.hooksPath=scripts/git-hooks`로 변경 검사기를 호출하며 실제 push의 fmt·Clippy·문서 검증 결과는 실행 시 별도로 확인 |
| 재검토 | Actions 명령/toolchain과 불일치, 테스트 전용 lint 누락, 미커밋 source나 상속 target 혼입, 검사 실패를 허용하는 훅 |

<a id="doc-003"></a>
## DOC-003 결정 기록

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-19; 구현·회귀·독립 검토 완료 |
| 근거·담당 | 사용자가 GitHub Actions 포맷 실패 확인 후 push 전 절차에 Rust 포맷 검사를 추가하도록 명시 지시. 작업·계약·통합 담당 `/root` |
| 선택·이유 | 기존 pre-push 임시 commit 사본에서 `cargo fmt --all --check` 후 문서 검사. 실제 전송할 Rust 소스의 포맷 오류를 원격 실행 전에 차단 |
| 대안 | 작업 트리 검사만 하면 미커밋 수정이 잘못된 commit을 가릴 수 있음. pre-commit 추가는 사용자가 요청한 push 시점과 다름. 훅의 자동 포맷 수정은 전송할 commit과 작업 트리를 불일치시킬 수 있어 제외 |
| 소유·대체 | `scripts/check_push.py`, `scripts/test_check_push.py`와 연결된 안내·빌드 색인만 변경. DOC-002의 문서 검사 범위·시점·ref tip 선택·중복 제거·삭제 제외는 유지하고 검사 종류만 추가. 기존 DOC-002의 실행 증거는 당시 기록으로 보존 |
| 검증 | 변경 전 새 반례 시험은 잘못된 포맷 commit의 push가 허용되어 FAIL. 변경 후 `python3 -B -m unittest discover -s scripts -p test_check_push.py`: 임시 로컬 bare remote에 대한 7개 회귀 PASS. 실제 고정 rustfmt로 포맷 오류 거절·수정 수락·미커밋/스테이징 변경 분리·작업 트리 보존·잘못된 tag가 전체 ref 전송 차단·manifest 누락과 기존 문서 gate 사례 확인. 문서 검증기는 fixture이며 제품 문서 검사·제품 시험·GitHub push는 실행하지 않음 |
| 독립 검토·적용 | `/root/push_format_review`가 실제 diff·정책·7개 회귀 PASS를 독립 확인하고 기존 HEAD 검사기에서 포맷 반례의 실패도 재현: PASS, 수정 필요 지적 없음. `/root` 반영 확인. 현재 checkout의 `core.hooksPath=scripts/git-hooks`를 확인했으며 기존 훅이 변경된 검사기를 호출함 |
| 재검토 | CI와 formatter/toolchain 불일치, commit 사본 검사 누락, 잘못된 포맷이 허용됨, 기존 사용자 훅과 충돌 |

<a id="doc-002"></a>
## DOC-002 결정 기록

| 항목 | 내용 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 근거·담당 | 사용자가 편집 중 문서 검사 대신 push 제약을 명시 지시. 작업·계약·통합 담당 `/root` |
| 선택·이유 | 전송할 commit마다 pre-push 검사. 편집 중 가변 상태와 다른 작업의 미커밋 변경을 배제하고 검사 실패 시 전송을 막음 |
| 대안 | 매 편집 검사는 잡음/중복 실행이 큼. pre-commit은 사용자 수정 지시로 제외. 작업 트리 검사는 실제 전송할 내용과 다를 수 있어 commit export 사용 |
| 대체 | DOC-001·AGENTS·DP03 V0 및 관련 정책의 문서 수정 직후 실행 시점만 대체. 문서 형식·단계/제품 검증·독립 검토 의무 유지 |
| 소유·중복 | 새 `scripts/check_push.py`, `scripts/git-hooks/pre-push`, `scripts/test_check_push.py`와 문서 검사 시점 규칙만 수정. DEV2 이관의 checker/product/evidence 변경은 별도 작업으로 보존. 공유 정책/README는 최신 원문의 해당 절만 변경 |
| 검증 | `python3 -B -m unittest discover -s scripts -p test_check_push.py`: 임시 로컬 bare remote에 대한 훅 회귀 4개 PASS. 정상/실패·dirty tree 분리·tag/중복·삭제·다중 ref·검사기 누락/잘못된 입력 확인. 문서 검사기 의미의 검증이 아닌 push 차단 동작의 시험이며 실제 원격 push·제품 검증 미실행 |
| 독립 검토·적용 | `/root/dev3_parallel_review`가 실제 스크립트·정책 diff와 위 실행 결과를 읽기 전용으로 검토: PASS, 수정 필요 지적 없음. `/root` 반영 확인. 기존 사용자 훅/설정이 없는 현재 저장소의 local core.hooksPath를 scripts/git-hooks로 설정. 동시 DEV2 이관 변경과 원격 보호 설정은 승인 범위 밖 |
| 재검토 | 잘못된 commit 검사, 실패를 허용하는 훅, 기존 훅 충돌, 원격 강제가 필요한 경우 |

<a id="doc-001"></a>

## DOC-001 결정 기록

| 항목 | 내용 |
| --- | --- |
| 상태·일자 | ACCEPTED · 2026-10-18 |
| 근거 | 사용자가 HTML 생성 중단, Markdown 전환, 상시 규칙/자료성 문서 구분, 전체 재구성과 기존 변경 포함 commit을 명시 지시 |
| 담당 | `/root` |
| 선택 | 역할별 배치와 작업별 읽기 경로, Markdown 단일 원문, JSON 문서 ID 색인 |
| 대안 | 날짜별 단일 폴더 유지는 규칙과 근거 탐색을 혼합함. 주제마다 파일 강제 분할은 문맥을 과도하게 분산함 |
| 대체 | 이전 AGENTS/README/DP03 V0의 HTML 생성 지침을 대체. 제품 계약·개발 순서·기존 실행 결과는 유지 |
| 검증·검토 | 기존 문서 운영 원칙을 XADB에도 유지한다. XADB에서 새로 수행한 문서 검증·단위 검사·독립 D2 검토는 [이관 기록](../document-migration.json)의 review·verification을 따른다. 이전 검토 결과를 현재 소스의 실행 증거로 승계하지 않는다. |
| 재검토 조건 | 작업별 진입점 누락, 정본 충돌, 색인과 실제 경로 불일치, 사용자의 출력 형식 변경 요청 |

# 모듈 분리 강제 계획 리뷰 — 시행 전 점검

2026-10-31 · 기준 main `5c98937` · **정적 검토·개선 제안 / 계획 변경 채택·검사기 구현·원격 설정 변경 없음**

[검토 대상: 모듈 독립 계획 12절](../plans/module-independence-plan.md#post-separation-gates) · [M5 인수 기준](../plans/module-independence-plan.md#acceptance) · [허용 import 그래프](../contracts/dev3-module-boundaries.md#target-imports) · [push 검사](../policies/documentation-policy.md#push-gate)

사용자가 "모듈 분리를 commit이나 PR에서 조사해서 강제하는 방법을 논의한 문서"의 리뷰를 요청했다. 해당 문서는 별도 파일이 아니라 [모듈 독립 계획 12절](../plans/module-independence-plan.md#post-separation-gates)이며, 2026-10-29에 저장되어 [AGENTS.md](../../AGENTS.md)의 "모듈 분리 후 독립성 평가 적용" 절에서 연결된다.

검토 방법은 12절과 관련 계약·정책, 현재 [push 검사기](../../scripts/check_push.py)·[workflow](../../.github/workflows/dev0.yml), GitHub 저장소 설정(공개 여부, branch 보호, PR 작성자)의 대조다. 제품 빌드·시험·문서 자동 검사는 실행하지 않았다. 이 문서는 제안이며, 채택은 12절이 정한 대로 시행 전 D2 설계에서 판단한다.

<a id="summary"></a>
## 1. 요약

**원칙은 건전하지만, 시행 설계로 넘어가기 전에 신원·정본·증거 위치라는 세 전제를 먼저 정해야 한다.**

1. **승인 신원:** PR #1–#11이 모두 `et16kr` 이름으로 올라왔다. GitHub는 작성자 본인의 승인을 필수 리뷰로 세지 않으므로, 현재 구조에서는 "독립 리뷰를 요구하는 보호 규칙"을 걸 수 없다. 에이전트가 사람과 같은 권한을 쓰므로 라벨·merge·설정 변경도 구분되지 않는다.
2. **기계 판독 정본 부재:** 허용 의존 그래프와 전용 경로가 텍스트 트리와 산문으로만 있다. 검사기의 입력이 없고, 계약이 인정한 legacy 잔여 위반 때문에 "기존 위반을 승인하지 않는다"는 원칙과 첫날 시행이 충돌한다.
3. **증거가 CI에서 보이지 않음:** 원 로그는 gitignore된 `artifacts/`와 공유 폴더에 있고, catalog native(60GiB) 같은 시험은 CI에서 실행할 수 없다. "미실행은 차단" 원칙대로면 이런 변경은 merge할 수 없다.

저장소가 public이라 branch protection·ruleset·Actions 표준 runner를 비용 없이 쓸 수 있다. S8에서 만든 "상대 모듈을 물리 제거한 단독 source export" 방식은 `use` 문자열 분석보다 싸고 강한 경계 검사로 일반화할 수 있다.

<a id="current-state"></a>
## 2. 현재 상태와의 대조

현재 실제로 동작하는 강제 수단은 로컬 push gate의 fmt·Clippy·문서 검사뿐이다. 12절도 "현재 검사기·workflow가 이 계획을 집행한다고 기록하지 않는다"고 적어 두어 서술은 사실과 맞다.

| 항목 | 현재 상태 | 확인한 곳 |
| --- | --- | --- |
| main 보호 규칙 | 없음 (`protected: false`) | GitHub branch 목록 |
| 저장소 공개 범위 | public | GitHub 저장소 정보 |
| GitHub Actions | `dev0.yml`의 fmt 검사 하나. Rust·Cargo 파일 변경 때만 실행(paths 필터) | [workflow](../../.github/workflows/dev0.yml) |
| 로컬 push gate | commit export에서 fmt, workspace Clippy `-D warnings`, 문서 검증. 경계·증거 검사 없음 | [push 검사기](../../scripts/check_push.py) |
| PR 작성자 | #1–#11 전부 `et16kr` (Claude·Codex 작업 포함) | GitHub PR 목록 |
| 통합 경로 | main 반영은 PR merge(#2, #5–#8, #11). 일부는 작업 branch로 merge(#4, #9, #10) | GitHub PR 목록 |
| 허용 의존 정본 | 텍스트 트리와 산문. 기계 판독 형식 없음 | [허용 import 그래프](../contracts/dev3-module-boundaries.md#target-imports) |
| crate 내 경계 증거 | compile-fail fixture(MBC00 boundary-compiler). `engine`은 단일 crate, `.rs` 518개 | [fixture manifest](../../tests/fixtures/dev3-module-c0/boundary-compiler/manifest.json) |
| 원 증거 위치 | gitignore된 로컬 `artifacts/`와 공유 폴더. 저장소에는 evidence 문서와 hash | [AGENTS.md](../../AGENTS.md) |
| 적용 조건(M4/M5 완료) | 미충족. S8 진행 기록 기준 MI03 PARTIAL | [M5 인수 기준](../plans/module-independence-plan.md#acceptance) |

작은 불일치가 하나 있다. `check_push.py`의 Clippy 주석은 "Actions lint 범위에 맞춘다"고 하지만 Actions는 Clippy를 실행하지 않는다. [push 검사 정책](../policies/documentation-policy.md#push-gate)은 이 점을 정확히 적고 있어 주석만 낡았다.

<a id="keep"></a>
## 3. 유지할 원칙

아래 원칙은 그대로 두고 시행 설계의 수락 기준으로 쓴다.

- 로컬 훅을 원격 강제의 증거로 쓰지 않고, push 뒤 도는 CI를 사전 차단으로 표현하지 않는다.
- Cargo 관계나 `use` 문자열 수로 crate 내부 경계 검증을 대신하지 않는다. 전체 경로 참조, 재노출, `#[path]`, 매크로 때문에 맞는 판단이다.
- SKIP·중단·미실행을 PASS로 보지 않고, N/A에는 근거를 요구하며, 미구현이나 시간 부족을 N/A로 바꾸지 않는다.
- 변경자가 같은 patch에서 규칙을 넓히거나 자기 PASS를 쓰지 못하게 한다는 요구는 이 저장소의 가장 현실적인 위협을 정확히 짚는다.
- 검사기 자체를 금지 의존, 상대 fixture 혼입, 오래된 결과, 검사기 오류 같은 음성 사례로 검증하고, 보호된 시험 ref에서 실제 거절을 확인한 뒤에야 활성으로 기록한다.
- 무변경 시험 일괄 재실행 금지와 원 source별 증거 보존을 유지해 AGENTS.md의 시험 규칙과 맞는다.
- 파일 수·LOC·완료율을 합격 점수로 쓰지 않고, 계획 밖 변경 건수와 계약 재개방 같은 원인 지표를 기록한다.

<a id="gaps"></a>
## 4. 핵심 공백과 위험

아홉 가지 공백이 있으며, 앞의 세 개는 시행 설계를 시작하기 전에 답이 필요하다.

1. **[높음] 신원과 신뢰의 뿌리가 비어 있음.** 12절은 "신뢰할 검사 실행 주체와 승인 경로를 정한다"고만 적었지만, 이것이 설계의 중심 결정이다.
    - 모든 PR 작성자가 `et16kr`이라 필수 승인 1건을 걸면 매번 관리자 우회가 필요하고, 우회가 일상이 되면 규칙이 무력해진다.
    - 에이전트가 같은 계정 권한을 쓰므로 "규칙 변경 승인 라벨" 같은 장치도 에이전트가 붙일 수 있다.
2. **[높음] PR이 자기 검사기를 바꿀 수 있음.** `pull_request` 이벤트는 PR 쪽 workflow 파일을 실행한다. 같은 이름의 job을 항상 성공하게 고치면 required check를 통과한다.
    - 특정 ref의 workflow를 강제하는 ruleset 기능은 조직 저장소용으로 알고 있다(시행 때 확인 필요). 개인 저장소에서는 base branch의 검사기를 실행하는 구조가 따로 필요하다.
3. **[높음] 기계 판독 정본과 기존 위반 처리가 없음.** 허용 그래프, 전용 경로, 공유 등록 파일 목록이 산문이다. 경계 계약 4절 스스로 "깊은 legacy 내부 관계는 잔여"라고 적고 있어, 예외 장치 없이 시행하면 첫날 실패하거나 누군가 잔여를 허용 목록에 넣게 된다.
4. **[중간] 증거가 CI에 없고 위조할 수 있음.** MI05의 "원 로그·결과와 현재 diff의 대응"을 CI가 보려면 증거가 CI에 있어야 한다. 또 에이전트가 쓴 결과 파일은 누구나 쓸 수 있다. CI가 직접 실행한 결과(검증됨)와 로컬에서 실행한 결과(주장됨)를 등급으로 나눠야 한다.
5. **[중간] native·대용량 시험.** catalog native(60GiB)는 로컬 전용이다. 사용자 PC를 self-hosted runner로 쓰는 안은 public 저장소에서 fork PR이 PC에서 코드를 실행할 수 있어 권하지 않는다.
6. **[중간] paths 필터와 required check의 충돌.** `dev0.yml`처럼 paths 필터가 있는 workflow를 required로 걸면, 문서만 바뀐 PR에서는 check가 생기지 않아 merge가 계속 대기 상태로 남는다. 게이트 job은 항상 실행하고 범위 판단은 job 안에서 해야 한다.
7. **[낮음] MI01·MI04의 PR 단위 적용 범위가 모호함.** MI01은 병렬 실험의 판정이라 일반 PR에서는 기본 N/A가 되어야 한다. MI04는 어떤 상호작용이 "필요한지"를 누가 정하는지가 없다.
8. **[낮음] 작업 branch로 가는 PR.** #4, #9, #10처럼 작업 branch로 merge하는 PR은 main 보호를 거치지 않는다. 최종 main PR에서 합산 diff로 판정하므로 안전하지만, MI02 국소성 판정이 큰 diff 위에서 이뤄진다.
9. **[낮음] 논리 경로와 물리 경로의 불일치.** `#[path = "plan4_apply.rs"]`처럼 두 경로가 다르다. 의존 판정은 논리 모듈 경로, MI02는 파일 경로로 해야 하므로 정본이 두 대응을 모두 가져야 한다. `cfg(test)` 코드의 교차 접근 규칙도 따로 정해야 한다.

<a id="feasibility"></a>
## 5. 평가별 자동화 가능성

의존·국소성(MI02)은 지금 도구로 강하게 자동화할 수 있고, MI06은 신원을 분리하기 전에는 기록 형식 검사에 그친다. public 저장소라 표준 runner의 Actions 시간은 비용 제약이 아니다.

| 평가 | 자동화 수준 | 방법 | 실행 비용 |
| --- | --- | --- | --- |
| 허용 의존·접근 | 높음 | 상대 모듈을 물리 제거한 export에서 `cargo check --all-targets`(컴파일 성공 = 의존 없음). seal은 기존 compile-fail fixture. 보조로 rust-analyzer 기반 모듈 그래프에서 금지 edge 보고 | export당 check 1회, 수 분 |
| MI02 국소성 | 높음 | PR diff 파일 목록을 경로 소유 정본과 대조. rename은 `git diff -M` | 빌드 없음, 수 초 |
| MI03 독립 인수 | 중간 | 단독 export 빌드는 자동. memory 시험은 CI가 실행, native는 로컬 결과를 "주장됨"으로 수령 | 선택 시험만큼 |
| MI04 실제 합류 | 낮음~중간 | 정본에 적은 합류 사례 selector가 diff에 걸리면 결과를 요구 | 선택 시험만큼 |
| MI05 증거 유효성 | 중간 | commit된 receipt(입력 identity, selector, 결과, 로그 sha256)의 입력 identity를 PR head에서 다시 계산해 비교. identity는 manifest가 정한 시험 입력 경로(제품·시험 source, fixture, Cargo 파일)의 파일별 hash이며 receipt·evidence 경로는 제외한다. 로그 원본은 Actions artifact나 공유 폴더 | 빌드 없음 |
| MI06 독립 판정 | 낮음(신원 분리 전) | 지금은 리뷰 기록의 형식·대상 commit만 확인 가능. 신원을 분리하면 GitHub 필수 리뷰나 CI 안에서 도는 리뷰 job의 check로 강제 | 결정 필요 |
| MI01 독립 착수 | 낮음 | 병렬 실험을 할 때만 수동 기록, 일반 PR은 기본 N/A | 없음 |

"단독 export 후 `cargo check`"는 12절이 금지한 `use` 문자열 분석의 약점을 피한다. 컴파일러가 재노출, 전체 경로, 매크로를 모두 해석하기 때문이다. 단, 이 방식은 A·B처럼 등록된 기능 모듈 사이의 "없어야 할 의존"만 증명한다. 허용된 하위 계층 안의 방향(예: `physical`이 Catalog/Row를 import하지 않음)은 모듈 그래프 보조 검사가 필요하다.

<a id="proposal"></a>
## 6. 구체적 제안

12절의 도입 순서(정책 반영 → 검사기 → 연결·거절 확인 → 일반 통합)는 유지하고, 각 단계에 아래 내용을 채우는 것을 제안한다.

1. **정본 manifest를 하나 만든다.** 예: `devdoc/contracts/module-boundaries.json`.
    - 모듈마다 id, 논리 모듈 경로, 파일 glob, 담당, 허용 의존을 적는다.
    - 공유 등록 파일(`lib.rs`·`mod` 선언, `Cargo.toml`, 문서 색인, README 진행 기록)을 목록으로 둔다.
    - 예외 목록에 legacy 잔여 edge를 넣고, 각 항목에 계약 anchor와 해제 조건을 달아 "줄기만 가능"(ratchet)하게 한다. 실제 의존을 허용 목록으로 복사하지 않는다는 12절 원칙을 지키면서도 첫날 시행이 가능해진다.
    - 합류 사례 selector를 모듈 쌍마다 적어 MI04의 "필요한 상호작용"을 자동으로 고른다.
    - 경계 계약 4절의 텍스트 트리와 이 파일이 일치하는지는 기존 문서 검증에 넣어 정본이 둘이 되지 않게 한다.
2. **빌드 없는 빠른 검사기를 만든다.** 예: `scripts/check_module_gate.py`.
    - 입력은 base..head diff와 base 쪽 manifest다.
    - 미등록 파일, 상대 전용 경로 수정, 예외 목록 증가, receipt의 입력 identity와 PR head에서 다시 계산한 값의 불일치를 차단한다.
    - receipt는 전체 tree hash를 기록하지 않는다. receipt를 commit하면 그 tree가 바뀌어 자기 자신과 맞출 수 없기 때문이다. 대신 receipt·evidence 경로를 뺀 시험 입력 경로만으로 identity를 계산한다.
    - 규칙 파일(manifest, 검사기, workflow)과 일반 파일을 함께 바꾼 PR을 차단한다. 규칙 변경은 단독 PR로만 올라와 사람의 눈에 띄게 된다.
    - 출력은 MI별 PASS/FAIL/N/A(근거)와 12절이 기록하라는 지표(계획 밖 상대 내부 변경 수 등)다.
3. **컴파일 기반 경계 검사를 넣는다.** S8의 단독 export 도구(작업 branch `claude/project-thread-1jfdtk-s8`, 이 리뷰 시점 main 미반영)를 일반화해, 등록된 기능 모듈마다 "상대 제거 export"를 `cargo check --all-targets`한다. 시험 실행이 아니라 컴파일 확인이라 AGENTS.md의 시험 규칙과도 충돌하지 않는다.
4. **로컬과 GitHub에 연결한다.**
    - 로컬: 기존 `check_push.py`에 2단계 검사를 추가한다(3단계는 선택).
    - `module-gate` workflow: `pull_request_target`으로 base의 검사기·manifest를 실행해 PR diff만 판정한다. 권한은 `contents: read`, 비밀 없음, PR 코드는 실행하지 않는다. public 저장소에서 PR 코드를 여기서 빌드하면 외부 fork가 저장소 권한으로 코드를 실행하는 구멍이 된다.
    - `build-gate` workflow: `pull_request`로 단독 export check와 fmt를 돌린다. PR 코드 실행은 여기서만 하며 비밀은 없다.
    - 두 workflow 모두 paths 필터 없이 항상 실행한다.
    - main ruleset: PR 필수, required check `module-gate`·`build-gate`, force push·삭제 차단, 우회 목록은 비운다(관리자 포함).
    - 시험 branch에 금지 변경 PR을 올려 실제 거절을 확인한 뒤 활성으로 기록한다(12절 3단계 그대로).
5. **MI06을 강제하려면 신원을 분리한다.** 에이전트용 별도 GitHub 계정(machine user)으로 PR을 올리면 사람 계정의 승인이 필수 리뷰로 성립한다. 에이전트 계정에는 write만 주고 우회 권한은 주지 않는다.

12절 본문에는 두 문장만 더하면 된다. 하나는 "신뢰 뿌리(승인 신원)는 시행 D2의 첫 결정"이고, 다른 하나는 "결과는 CI 실행(검증됨)과 로컬 실행(주장됨)으로 등급을 나눔"이다.

<a id="decisions"></a>
## 7. 시행 전에 정할 것

다섯 가지는 에이전트가 정할 수 없는 운영 결정이라 사용자의 판단이 필요하다.

| 결정 | 선택지 | 권장 |
| --- | --- | --- |
| 최종 승인의 신원 | (a) 사람이 누르는 merge를 승인으로 정의 / (b) 에이전트 별도 계정 + GitHub 필수 리뷰 | (a)로 시작. MI06을 기계적으로 강제해야 할 때 (b) |
| main 직접 push 금지 | 금지 / 허용 | 금지. Codex가 main checkout에서 직접 push하던 방식이 PR 방식으로 바뀐다 |
| 로컬 실행 결과의 수령 | native·대용량만 "주장됨" 인정 / 전부 인정 / 불인정 | native·대용량만 인정, 나머지는 CI 실행 |
| 작업 branch로의 PR | 허용 / 금지 | 허용. 최종 main PR에서 합산 판정 |
| 착수 시점 | M5 완료 뒤 전체 / manifest만 먼저 | M5 완료 뒤. 지금은 MI03 PARTIAL이라 12절의 적용 조건이 충족되지 않았다 |

이 리뷰는 문서 검토만 했으며 제품 코드·검사기·원격 설정은 바꾸지 않았다. S8의 단독 export 도구는 이 리뷰 시점에 GitHub에 올라오지 않아 직접 읽지 못했고, S8 진행 기록만 보고 판단했다.

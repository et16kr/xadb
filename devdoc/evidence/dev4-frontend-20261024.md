# DEV4 독립 앞단 개발 — 2026-10-24

현재 **첫 lexer·이름 구현·선택 검증 완료 / 최종 독립 D1 제한 수용 / DEV4·Q1 미완료**다.

[첫 입력 동결](../contracts/dev4-independent-frontend.md#lexer-leaf) · [병행 준비 계획](../plans/dev4-preparation-plan.md)

## 작업과 소유

사용자는 DEV3 개발 중 가능한 DEV4 선행 개발, 가능한 병렬 서브 에이전트, 적절한 단위 commit, 저장소 AI 지침과 로컬 DBMS 참고를 요청했다. 첫 묶음은 P0 lexer 소비 입력·P1 lexer/이름·독립 oracle이며 전체 요청은 splitter/parser/scalar/predicate/독립 모델·수합까지 계속된다. 엔진 소비 구현은 실제 DEV3 선행 뒤다.

기준 HEAD와 DEV3 dirty diff/untracked digest는 `artifacts/dev4-frontend-20261024/baseline.json` 및 원 `baseline.patch`에 보존했다. 기존 engine 변경은 별도 DEV3 작업이며 읽기·수정·시험·commit 범위에서 제외한다. 새 frontend는 engine을 소비하지 않아 그 미commit 상태가 제품 의미에 영향을 주지 않는다. 공용 Cargo/lockfile·reference 모듈 등록·문서/JSON은 `/root`가 최신 내용을 읽고 순차 반영한다.

| 담당 | 독점 범위 | 상태 |
| --- | --- | --- |
| `/root` | diagnostic/control/name·frontend_control 시험, 공용 등록·문서·통합·commit | 구현·선택 검증 |
| `/root/frontend_research` | 로컬 DBMS 조사, `crates/sql/src/frontend/lexer.rs` | 구현 인계; 시험 직접 실행 없음 |
| `/root/frontend_oracle` | reference frontend/literals, SQL lexer/name/oracle 시험·support adapter와 manifest | 독립 입력 동결·구현 인계; 시험 직접 실행 없음 |
| `/root/frontend_review` | 계약·실제 diff·독립 기대값·실행 증거 읽기 검토 | D2 및 최종 D1 제한 수용 |

merge 순서는 공통 API·F01/F04 독립 D2 → 분리 구현/fixture → 실제 연결·선택 시험 → 독립 D1/D2 → 단위 commit이다. 문서 자동 검사는 push gate에서만 수행한다. 새 외부 dependency/source root·생성기는 도입하지 않는다. 기존 `crates` 재귀 source fingerprint/export 범위를 사용한다. 이번 준비·설명은 시험 PASS가 아니다.

## 조사·검토·실행

조사 revision·D2/D1 결과·동결 fixture와 source별 실제 명령/결과는 완료되는 대로 추가한다. 완료 사례를 재실행하지 않고 실패/중단·수정된 시험 범위를 보존한다.

### 로컬 DBMS 조사

`/root/frontend_research`가 아래 checkout의 실제 HEAD와 clean 상태를 확인하고 읽었다. 참고 구현의 동작을 XADB 의미나 실행 증거로 승계하지 않는다.

| checkout·revision | 확인 위치 | 적용·차이 |
| --- | --- | --- |
| `/home/et16/work/postgres` · `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/backend/parser/scan.l`:92,204,443,774,800,1047,1245; `scansup.c`:46 | byte offset·여섯 ASCII whitespace·상태별 EOF를 참고. 중첩 주석·미인용 소문자화·이름 truncation은 도입하지 않음 |
| `/home/et16/work/mariadb` · `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `sql/sql_lex.cc`:1639,2093,2320,2387,2429 | quote/hex 분류 대조. `--` 뒤 공백 요구·sql_mode/backslash·실행 comment는 XADB에 없음 |
| `/home/et16/work/cubrid` · `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/parser/csql_lexer.l`:1301,1339,1458,1995 | quote doubling·상태별 EOF·chunk 수신 대조. chunk 끝을 실제 EOF로 취급하지 않는 의무는 후속 splitter에 인계 |
| `/home/et16/work/sqlite` · `43a48f77585a88940c727f33682151418f544059` | `src/tokenize.c`:273,321,396,433,552,646 | 작은 token scanner와 binary prefix 우선 분류 참고. NUL 종료·미완결 comment 성공·token 종료 뒤만 interrupt 검사는 채택하지 않음 |

### 독립 D2 최초 검토

`/root/frontend_review`는 첫 제안에서 제품 구현 전 fixture/hash가 빠지고 제어·token-limit·forged token의 진단 span이 미확정임을 지적했다. 최초 판단은 **보완 필요**이며 수용으로 덮어쓰지 않는다. `/root`는 zero-width 제어/한도 위치, 잘못된 name token의 clamped span·취소 우선순위를 계약에 추가하고 literal·control 입력을 구현 전에 동결하도록 보완했다. DEL 거절의 SQL 정본 교차 참조·이름 identity의 bytes 기준도 반영했다. 재검토는 실제 동결 파일이 준비된 뒤 수행한다.

### 구현 전 동결·독립 D2 수용

2026-10-24 15:59:51 KST에 제품 구현 전 입력 사본을 `artifacts/dev4-frontend-20261024/freeze/`에 보존했다.
literal SHA-256은 `c886c3f285819b94e62cc3e152be293514b9912f87d8a289d2da412a1f5daaf8`,
최초 control은 `c122a1c5ee26da2a09a56f3502f06d1f4d868030c7cd5b4f344ddd44df240f9c`,
최초 manifest는 `7c74415f5fb93e7b708ab10678e22ed1099c63234f569a74ad4aa8462f132ca9`다.
독립 검토자는 세 hash·473개 입력·30개 관측 mutant 및 진행 offset 기반 취소 seam을 확인해
**D4-FE-LEX-001 독립 D2 채택을 수용**했다. `/root`가 계약 담당으로 반영했다.

clock 호출 횟수만으로는 진입 취소가 내부 loop 시험을 통과할 수 있다는 검토 지적을 반영했다.
비기본 test-support observer는 실제 byte 위치에서만 신호를 주며, 긴 scan의
256..259와 이름 UTF-8 검사 종료 뒤 decode의256..257 위치를 요구한다.
제품 개발과 독립 판정/adapter 개발은 서로 다른 파일에서 병행했다. 실제 성능 향상률은 측정하지 않았다.

### source별 실행과 교정

실제 명령·source manifest·원 log hash는 [실행 색인](dev4-frontend-20261024.json)에 있다.
모든 source 사본은 기준 commit의 export에 DEV4 변경만 overlay했으며 별도 DEV3 dirty 변경을
가져오지 않았다. 시험 선택은 각 manifest의 `selection_reason`에 실행 전에 기록했다.

| source·실행 | 결과 | 범위·해석 |
| --- | --- | --- |
| r1 `9f39623c…` · `r1-tests.log` | compile FAIL / 시험0 | 공개 lexer variant/field Rustdoc68개 누락. lint allow 대신 docs를 추가했으며 원 실패 보존 |
| r2 `412ff18f…` · `r2-tests.log` | 15함수 PASS / 확대2 ignored | lexer289+한도10+이름110+생성64=473입력, 관측 mutant30, 제어9함수. generated는64seed×128operation. ignored를 PASS로 합산하지 않음 |
| r2 제품+새 반례 · `name-red.log` | FAIL | ASCII raw258-byte 이름 UTF-8 검사에서 기대 checkpoint256 대신258. 독립 D1 정적 검토가 발견한 실제 반례 |
| r3 `3c855e27…` · `r3-name.log` | 이름110+제어3함수 PASS | bounded 이름 UTF-8 검산·scalar decode 수정. 새 반례1·직접 관련 기존2제어만 실행; lexer·oracle 완료 시험 반복 없음 |
| lexer checkpoint 제거 · `mutant-lex-result.log` | EXPECTED_REJECTION | 실제 변경 source compile 뒤 내부256 대신100000에서 취소되는 mutant를 assertion이 거절 |
| name checkpoint 제거 최초 · `mutant-name-result.log` | INVALID_SOURCE_SELECTION | 공유 Cargo target이 이전 사본 binary를 재사용, exit0은 mutant 판정 증거가 아님 |
| name checkpoint 제거 격리 · `mutant-name-isolated-result.log` | EXPECTED_REJECTION | source별 독립 target으로 실제 mutant를 compile, 내부 취소 누락을 assertion이 거절 |
| r3 default compile·Clippy | PASS | `cargo check -p xadb-sql --no-default-features --offline --locked`; SQL/reference의 all-targets/all-features Clippy `-D warnings`. 기존 시험 사례는 실행하지 않음 |
| 최종 DEV4 source 포맷 | PASS | SQL package와 새 reference frontend의 rustfmt check. 문서 verifier·전체 wrapper는 실행하지 않음 |

첫 metadata 전체 offline 조회는 기존 선택 밖 target dependency `r-efi v5.3.0`이 cache에 없어
실패했다. no-deps metadata 조회는 성공했고 lockfile diff는 로컬 `xadb-sql` package8줄 추가뿐이다.
제품 시험 실패나 dependency version 변경으로 기록하지 않는다.

공유 build cache의 최초 잘못된 mutant 실행은 원 log/exit0과 INVALID 분류를 함께 남겼다.
이후 artifact 실행기는 source별 target을 사용한다. r2·r3 및 lexer mutant는 각 log에 해당
SQL source를 실제 compile한 기록이 있으며 무변경 기능 시험을 다시 실행하지 않았다.
초기 r2/r3 binary는 공유 target에 있었으므로 별도 영구 binary 보존을 주장하지 않는다.
원 소스·fixture·명령·로그·scope는 보존했다. 후속 실행은 source별 binary도 따로 보존한다.

### 현재 범위와 잔여

현재 작업 트리의 DEV4 제품·oracle·시험·fixture20파일은 r3 manifest와 hash가 일치한다.
이름 검산 후속 반례는 control/manifest에 추가했고 최초 동결 사본·원 FAIL은 유지했다.
heap allocation0인 이번 borrowed leaf에서 quota/allocation-failure는 구조상 N/A다.
owned parser의 FE11 할당 실패·FE12 합산 quota 의무를 면제하지 않는다.

P1 splitter, P2 parser, P3 scalar/predicate, P4 구문 확대, P5 수명 모델, P6 공급 수합은
미완료다. 확대 generated256×1024·fuzz30분은 작성·NOT_RUN이다. 실제 engine 연결·SQL worker·CLI·
Q1·내구성은 구현/실행하지 않았다. 전체 사용자 목표는 계속 ACTIVE다.

### 최종 독립 D1

`/root/frontend_review`는 r3 코드·원 FAIL→관련 PASS·473/30/제어의 source별 범위·실제 두 mutant 거절·INVALID_SOURCE_SELECTION·기본 compile/Clippy와 현재20파일 지문을 읽기 대조하고 **첫 borrowed lexer/이름 leaf의 최종 D1을 수용**했다. 직접 시험은 재실행하지 않았다. `/root`가 반영 확인했다.

검토자가 지적한 metadata 복사 오류도 정정한다. 두 mutant 원 manifest의 `previous`/`selection_reason`은 r3 이름교정 설명이 남아 있으나 실제 선택은 해당 lexer/name checkpoint 변형과 **각 단일 대응 control**이다. `created_at`도 부모 snapshot 시각이며 mutant 생성 시각이 아니다. 원 manifest/hash/log는 변경하지 않고 실행 색인에 상속 문구와 정정 이유를 함께 기록했다. 실제 command·selected_tests·compile/test log가 실행 범위를 입증하며 이것은 재시험 사유가 아니다. 초기 name 반례 manifest의 부모 timestamp도 같은 방식으로 구분한다.

확대 profile·남은 독립 모듈과 Q1은 이 수용에 포함하지 않는다. 첫 단위 commit 후 다음 P1 splitter 입력 동결부터 전체 목표를 계속한다.

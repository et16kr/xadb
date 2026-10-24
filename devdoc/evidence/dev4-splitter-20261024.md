# DEV4 streaming 문장 경계·원문 소유 — 2026-10-24

현재 **streaming splitter·원문 예약 구현 및 선택 검증 / 최종 독립 D1 제한 수용 / DEV4·Q1 미완료**다.

[소유 계약](../contracts/dev4-independent-frontend.md#splitter-leaf) · [병행 계획](../plans/dev4-preparation-plan.md) · [기존 lexer 증거](dev4-frontend-20261024.md)

## 작업과 소유

사용자의 DEV3 중 독립 DEV4 개발·병렬 agent·단위 commit 요청을 계속한다. 직전 turn은
lexer/name 구현·선택 검증과 `a291e060af30e7865b41a6a6cb25edc852385fdc` commit을 완료한 progress다.
이번 기준 HEAD와 별도 DEV3 dirty 파일 지문·patch는 `artifacts/dev4-splitter-20261024/baseline.json`과
`baseline.patch`에 보존했다. engine 및 DEV3 scripts/fixtures는 수정·시험·commit 범위에서 제외한다.
공유 조립·문서는 root만 최신 파일에서 좁게 변경한다.

| 담당 | 독점 범위 |
| --- | --- |
| `/root` | source buffer/quota owner·splitter_resources 시험, 공통 모듈·진단 연결, 문서·export·선택 실행·commit |
| `/root/frontend_research` | splitter와 UTF-8 carry, 로컬 DBMS 연구 |
| `/root/frontend_oracle` | 독립 boundary literal·관측 checker, splitter_contracts 시험·자원 control fixture |
| `/root/frontend_review` | 읽기 전용 독립 D2/D1 |

공통 API·독립 fixture 동결/검토 → 제품/시험 병렬 구현 → 새 경로 선택 실행 → D1 → commit 순이다.
문서 verifier는 push gate 전용이며 실행하지 않는다. 기존 lexer/name의 완료 시험은 반복하지 않는다.

## 로컬 자료와 설계 검토

research agent는 아래 checkout의 실제 HEAD·clean 상태와 관련 코드를 재확인했다.

| 소스·revision | 위치 | 적용과 차이 |
| --- | --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/fe_utils/psqlscan.l:1198` | semicolon에서 중단·남은 입력 재개 참고. 괄호 깊이·중첩 comment·backslash command는 도입하지 않음 |
| SQLite `43a48f77585a88940c727f33682151418f544059` | `src/complete.c:140,210` | quote/comment 밖 경계와 미완결 상태 참고. NUL 종료·trigger·필수 끝 semicolon은 도입하지 않음 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/parser/csql_lexer.l:1995` | chunk 수신과 상태 유지 참고. 전역 lexer·실제 I/O는 도입하지 않음 |

첫 독립 검토에서 prefix만 소비하는 규칙, EOF와 chunk 끝 구별, 빈 구간 회계, 실제 capacity와
예약 순서·Drop 순서를 확인했다. reviewer 지적으로 §6의 할당 전 예약 문구에 allocator 관측
초과분의 좁은 보정을 명시하고, 모든 one-cut은256bytes 이하 literal에 한정했다. 1MiB 한도
입력에는 지정 cut만 사용한다. 원문 source-local span의 checked absolute 변환과 private
상태에서 offset overflow를 검증하는 방법도 명시했다. 이 준비는 제품 실행 증거가 아니다.

fixture 작성 중 short12개 partition의 최초 산술 추정992를892로 교정했다. 사례나 분할을
줄이지 않았다. pending delimiter와 연속별표3개를 추가하여 최종 후보는 short892 + edge256 +
limit13 + generated64 + lifecycle1 =1,226schedule다. root와 독립 reviewer는 scan/copy 취소가
allocation 전후의 다른 검사에서 발화할 수 있는 빈틈을 찾아 scan은600byte 선입력 뒤
700..856, copy는 old512+new1024의 회계1536 및256<=p<512로 한정했다. finished 취소 후
sticky Cancelled와 정상 finish 뒤 push Syntax도 별도 instance로 구분한다. 동결 전 후보는
fixture의 prefreeze/ 및 artifacts의 pre-freeze/에 보존하며 실행 FAIL로 세지 않는다.

## 동결과 실행

제품 구현 전 동결 시각은 `artifacts/dev4-splitter-20261024/freeze/manifest.json`에 기록했다.
동결 때 제품 splitter/source 파일은 없었다. 독립 D2 reviewer가 실제 파일 hash를 대조하고
D4-FE-SPLIT-001 채택을 수용했으며 root가 반영한 뒤 병렬 구현을 시작했다.

| 입력 | SHA-256 |
| --- | --- |
| 계약 §11 포함 원문 | `ed7cc48540cafbafd3f7ec79be4a91c1606d8d3b7d9ce5c4f5cefd6c316284f8` |
| 독립 splitter literal·checker | `7212a5cc2aef62f8b073b4c7509605839a5bebd6b398ba4b472faa673a066864` |
| 독립 resource control | `86f37a34f9b8643438961c08887bef14276852a776895e2342dc6367acf1f13d` |
| manifest | `e7b5e56b62d5abd099027bcca76fed86cf0d80c1de6ba1854a15d8d138e9d1c2` |

각 source는 기준 commit의 clean archive에 DEV4 소유 파일만 덧씌운 별도 export다. build target을
source별로 분리하고 실행한 test binary hash도 보존했다. `export.py`는 source마다 실제 선택 사유를
새 JSON에서 읽으며 이전 source의 설명·시각을 상속하지 않는다. 명령·source·원 log·binary hash는
[실행 색인](dev4-splitter-20261024.json)에 있다. 정상 완료 시험을 source 변경만으로 재실행하지 않았다.

| source·선택 | 결과와 범위 |
| --- | --- |
| r1 `3413bebd…` · library overflow1 + splitter_contracts + splitter_resources | **25함수 PASS**. short892+edge256+한도13+generated64+lifecycle1=1,226schedule, 잘못된 관측16개, 자원18함수. 확대2개 ignored는 NOT_RUN |
| r1 default compile | `cargo check -p xadb-sql --no-default-features --offline --locked` PASS |
| r1 Clippy | **compile FAIL 보존**. 기존 name 관측 adapter의 exhaustive match에서 새 Module::Split 분기 누락 |
| r2 `d79e5689…` · Clippy | 위 adapter 한 분기만 추가. SQL/reference all-targets/all-features `-D warnings` PASS. runtime 코드·기존 기대값은 그대로이며 시험 재실행0 |
| r2 포맷 | SQL cargo fmt 및 reference frontend rustfmt check PASS. 문서 verifier 미실행 |
| scan 검사 제거 mutant `e17cbfec…` | 실제 별도 source compile 후 내부 scan 대신 성장 시점1024에서 취소되어 기대700..856을 위반, **EXPECTED_REJECTION** |
| copy 검사 제거 mutant `0519ea1b…` | old512+new1024 copy 중 취소 누락으로 성공을 반환하여 대응 assertion이 거절 |
| terminal 해제 누락 mutant `53c306bd…` | growth 실패 뒤256bytes 잔존과 별도 EOF Syntax 뒤1,024bytes 잔존을 두 detector가 각각 거절 |
| 초과 capacity 회계 누락 mutant `2b9152c2…` | 실제272bytes와 charge256 불일치를 detector가 거절 |

기본 backing의 실제 capacity는256, 성장 후512이며 old/new768 동시 회계를 확인했다.
padding16 fixture는 실제272→560과 peak832를 관측했다. 두 native thread가 하나의 DB256
ledger를 공유할 때 정확히 하나만 backing을 유지하고 join 뒤0으로 회복했다. allocator 전체
RSS나 process OOM 복구를 검증했다는 뜻은 아니다. vector 선언 순서의 bytes-before-leases는
정적 D1로 확인하며 quota 최종 수치만으로 실제 allocator free 시점을 입증했다고 주장하지 않는다.

독립 D1 사전 검토에서 계약·literal·예약/Drop·UTF-8·checkpoint의 추가 제품 결함은 없었다.
reviewer가 frozen cleanup mutant에 detector2개가 있으나 최초 실행은1개였음을 지적하여,
미실행 두 번째 detector만 같은 mutant source에서 추가 선택했다. 원 manifest는 첫 선택 그대로
보존하고 `selection-mutant-cleanup-second.json`이 추가 선택을 설명한다. 정상 제품 시험을
반복하거나 원 결과를 덮어쓰지 않았다.

## 잔여

이번 단위는 streaming boundary와 owned source의 자원 수명이다. 단일 API 문장 수·parser/AST·값/
predicate·추상 수명 모델·DEV3 공급 수합과 실제 engine 연결·Q1은 계속 미완료다. 확대 profile과
실행하지 않은 사례를 이번 최소 선택의 PASS로 계산하지 않는다.

## 최종 독립 D1

`/root/frontend_review`는 현재29개 overlay 파일과 r2 지문,6개 source manifest,11개 실행
receipt의 log·binary hash를 대조하고 **D4-FE-SPLIT-001 제한 leaf의 최종 D1을 수용**했다.
제품·oracle·예약/Drop·취소·원 실패와 잔여 기록에 필수 수정점은 없다. root가 반영했다.
직접 시험을 반복하지 않았으며 공유 문서용 HEAD 기준 DEV4-only patch가 DEV3 변경을
포함하지 않는 점도 정적으로 확인했다. 실제 stage/commit은 통합 담당이 별도 대조한다.

계약·literal 동결,병렬 구현,독립 검토·선택 검증을 적용했으며 속도 향상률이나 PF10 성능
개선을 측정했다는 뜻은 아니다. 앞단 다음 단위를 계속하고 전체 사용자 목표는 ACTIVE다.

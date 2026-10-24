# DEV4 독립 앞단 구현

2026-10-24 · **lexer·이름·splitter·P2 parser·P3 scalar·predicate 구문 제한 구현 / 실제 SQL 실행·DEV4·Q1 미완료**

[독립 앞단 계약](../contracts/dev4-independent-frontend.md#lexer-leaf) · [병행 계획](../plans/dev4-preparation-plan.md) · [source별 증거](../evidence/dev4-frontend-20261024.md)

## 구현 범위

`xadb-sql`의 `frontend`는 source를 빌려 읽는 lexer, 원문 byte span·할당 없는 진단,
호출별 취소/monotonic deadline과 고정128-byte 정규 이름을 제공한다. 원문1 MiB와
nontrivia token65,536 한도를 적용하며 UTF-8·긴 token/comment·이름 decode 내부에서
제어를 검사한다. 미인용 ASCII 이름은 대문자화하고 인용 UTF-8 bytes·두 quote escape를
보존한다. 이름 identity는 bytes이며 PUBLIC namespace 해석은 후속 parser 책임이다.

normal dependency는 foundation 하나다. borrowed scanner와 고정 이름은 heap allocation을
하지 않는다. 반환된 이름은 원문 수명과 독립이고 lexer는 source/control을 빌린다.
streaming 원문 backing은 아래 splitter의 기존 session8 MiB/DB64 MiB quota를 소비한다.
parser AST arena의 예약·수명은 아래 P2 구현에 연결했다.
test-support feature의 progress observer는 기본 제품 build에 포함되지 않는다.

제품 경로는 `crates/sql/src/frontend/`이며 독립 literal/엄격 비교는
`crates/testing/reference/src/frontend.rs`와 `frontend/literals.rs`다. reference는
production tokenizer·name normalization을 정답 계산에 사용하지 않는다. SQL의 dev dependency만
reference를 소비한다. 새 외부 dependency·source fingerprint/export 루트는 없다.

## 선택 검증과 수신

실행 전 동결한 입력473개와 관측 mutant30개, 최초 제어9개를 r2 source에서 확인했다.
리뷰가 찾은 ASCII258-byte 이름의 UTF-8 제어 간격 초과는 별도 새 반례로 FAIL을 재현한 뒤
수정했다. r3에서는 이름110개와 해당 제어3개만 검증했다. 무변경 lexer·oracle의 기존 결과는
r2의 원 source/범위로 보존한다. 기본 feature compile과 변경 package Clippy도 확인했다.
최종 독립 D1은 이 제한 leaf에 수용됐다. 실제 command/log/hash는 위 증거 문서에서 확인한다.

새 코드의 검증 진입점은 다음 SQL integration test target이다. 다음 명령은 진입점 안내이며
변경 없는 전체 target을 반복하라는 지시가 아니다. 실행 전 diff와 필요한 function을 선택한다.

```bash
cargo test -p xadb-sql --locked --test lexer_contracts <function> -- --exact
cargo test -p xadb-sql --locked --test name_contracts <function> -- --exact
cargo test -p xadb-sql --locked --test frontend_oracle <function> -- --exact
cargo test -p xadb-sql --features test-support --locked --test frontend_control <function> -- --exact
```

확대 generated256×1024와 malformed fuzz30분은 ignored 별도 profile이며 **NOT_RUN**이다.
이를 최소 profile PASS에 합산하지 않는다. 시험용 mutant의 의도한 실패와 잘못된 build 선택은
제품 FAIL/PASS와 구분하여 보존한다. 문서 검사는 push 때만 실행한다.

<a id="splitter"></a>
## Streaming 문장 경계와 원문 수명

`Splitter::push`는 caller chunk에서 첫 nonempty 문장까지 소비하고 consumed와
NeedMore/StatementSource를 반환한다. `finish`만 실제 EOF를 선언한다. quote/comment/UTF-8
carry는 chunk 사이에 보존하고 첫 문장 뒤 suffix를 미리 검사하지 않는다. 빈 구간도1 MiB
상한을 소비하며 외부 semicolon 뒤에만 상한을 초기화한다. 반환 원문은 앞뒤 trivia·종료
semicolon을 포함하고 batch 절대 span을 유지한다. 이는 구문 분석이나 SQL 실행 성공이 아니다.

source backing은 주입된 기존 session/DB Quota handle을 공유한다. 초기256에서 성장하며
새 backing 전체의 예약을 먼저 확보하고 실제 capacity 초과분을 게시 전에 추가 회계한다.
copy 중 old/new capacity가 함께 charge되고 반환 source가 살아 있는 동안 예약이 유지된다.
오류·취소·Drop에서 pending bytes를 먼저 폐기하고 lease를 반환한다. 긴 scan/copy는256byte
간격으로 새 호출의 제어를 검사한다. 이전 호출 제어를 splitter나 반환 source가 저장하지 않는다.

제품은 `frontend/source.rs`, `splitter.rs`, `splitter/utf8.rs`이며 독립 기대값은 reference의
`frontend/splitter.rs`다. normal dependency는 foundation만 유지하고 engine을 소비하지 않는다.
비기본 test-support의 allocation 실패·capacity padding/관측과 기존 progress observer는
시험에서만 사용한다. 기본 feature compile도 확인했다.

[splitter source별 증거](../evidence/dev4-splitter-20261024.md)에 새25함수,1,226schedule,
관측변조16개, 실제 제품mutant4개/선택detector5개의 거절을 보존한다. 정상runtime은 r1이고
r2는 기존 이름 관측 adapter의 새 Module::Split 분기만 보완한 static compile이다. 원 Clippy
compile FAIL을 남겼으며 runtime/기존 lexer 시험을 반복하지 않았다. 포맷·SQL/reference
Clippy가 통과했고 최종 독립 D1을 이 제한 leaf에 수용했다. 확대256×1024와30분fuzz는 작성·NOT_RUN이다.

새 경로의 선택 가능한 진입점은 아래와 같다. 전체 target을 무변경 반복하라는 지시가 아니다.

```bash
cargo test -p xadb-sql --test splitter_contracts <function> -- --exact
cargo test -p xadb-sql --features test-support --test splitter_resources <function> -- --exact
cargo test -p xadb-sql --lib frontend::splitter::tests::absolute_offset_overflow_releases_without_allocation -- --exact
```

## 계속할 작업

P0/P1의 lexer·이름·streaming 경계와 원문 예약, 아래 P2 공통 parser·INSERT/거래 구문을
제한 구현했다. 단일 API 문장 수·AST/marker 번호·source/arena 수명과 독립 literal의 선택
증거를 수합했다. 아래 P3 scalar 입력과 predicate AST/parser를 추가했다. 재귀 depth64·256KiB stack은 새 조건식 경로에서 검증했으며 전체 타입 분석·3값 계산은 다음 단위다. P4 나머지 구문, P5 추상 수명 모델, P6 공급 수합과 실제 binding의 FE11/12도 미완료다.

조건식 평가·catalog binding·실행 CLI·session·실제 engine adapter와 Q1 entrypoint는 제공하지
않는다. engine 연결은 DEV3 catalog/row/index/statement/undo/commit/recovery/temp의 실제 선행
증거 뒤다. 앞단 단위 결과를 실제 SQL·복구·내구성·전체 DEV4/Q1 완료로 확대하지 않는다.

<a id="parser"></a>
## Owned AST와 INSERT·거래 구문

`Parser::new(&session, &database)`와 호출별 `parse(source, control)`가 원문과 flat AST를 소유한다.
INSERT·BEGIN/COMMIT/ROLLBACK·SET AUTOCOMMIT, PUBLIC/이름·단일 문장·marker1..256·node8,192
경계를 검증한다. `ParsedStatement`는 source/nodes/marker_count의 읽기만 제공한다. raw scalar의
sign/digits·Text/Binary 출처를 보존하며 값 변환·binding·실제 거래 효과를 만들지 않는다.

source와 typed arena는 같은 ledger의 실제 capacity를 charge하며 old/new가 성장 중 함께 살아 있다.
source copy·node 이동·긴 exponent 후보 재검사에 호출 제어를 적용하고 오류 뒤 이번 자원만 반환한다.
이전 결과는 계속 읽을 수 있고 Parser를 폐기해도 예약을 유지한다. 고정 node와 flat index 연결로
재귀 Drop과 목록 tail 재탐색을 피한다. test-support의 allocation 순번/추가 capacity 주입은 기본
제품 build에 없다. normal dependency는 foundation 하나이며 engine 접근이나 새 외부 dependency가 없다.

[source별 선택 증거](../evidence/dev4-parser-20261024.md)에 신규30함수·semantic258·known-bad23,
원 Clippy compile FAIL·adapter 한 줄 교정과 제품 mutant별 실제 거절을 보존한다. 기존 lexer/name/
splitter 시험은 반복하지 않았다. parser_contracts의 필수6함수, parser_resources의23함수와
`frontend::parse_budget::tests::typed_capacity_arithmetic_rejects_overflow`가 이번 선택이다.
확대2profile은 NOT_RUN이다. 이후 실행도 실제 diff에 대응하는 함수만 고른다.

P3 조건식 구문은 아래 후속 절을 따르며 typed compile/eval·P4 나머지 문법·P5 수명 모델·P6 실제 DEV3 수합과 engine 통합·Q1은 남아 있다.

<a id="scalar"></a>
## Scalar 디코딩과 typed 값 소유

`ScalarReader`는 실제 ParsedStatement의 scalar를 선택해 checked Int64·Text/Binary·Bool을
변환하고 raw NULL/marker를 미정으로 보존한다. `copy_sql`은 원래 타입→SQL empty Utf8의
같은 타입 NULL 정규화→nullable/byte 길이 순으로 명시 ScalarSpec을 검산하며 `copy_native`는
non-NULL 빈 text를 유지한다. foundation의 Value/compare를 변경하거나 실제 binder를 제공하지 않는다.

OwnedScalar는 non-Clone·readonly이며 String/Vec 실제 capacity를 기존 session/DB ledger에 유지한다.
정확 길이 검산 뒤 단일 backing을 예약하고 초과 capacity를 게시 전에 보정한다. 검산·decode·copy에
최대256byte 호출 제어를 적용하고 모든 실패/Drop에서 backing을 먼저 폐기한다. copy의 test-support
observer는 local byte 진행, 반환 진단은 caller span을 그대로 사용한다. 전역 저장 표현의 유효성을
목적 열 검산 완료로 쓰지 않으며 실제 후속 binder/predicate가 raw 원래 타입을 decode 전에 확인해야 한다.

[source별 선택 증거](../evidence/dev4-scalar-20261024.md)에 신규23함수·semantic8,929call,
known-bad20/정상통제8·자원16함수, 필수11범주/12제품 변조 거절과 기본 build/fmt/Clippy PASS를
보존한다. 기존 완료 시험은 반복하지 않았고 확대2profile은 NOT_RUN이다. 최종 독립 D1을 이 scalar leaf에 수용했다.

```bash
cargo test -p xadb-sql --features test-support --locked --test scalar_contracts <function> -- --exact
cargo test -p xadb-sql --features test-support --locked --test scalar_resources <function> -- --exact
```

이 진입점은 실제 diff에 대응하는 함수 선택용이다. P3의 3값/전체 타입 분석과 실제 SQL 실행·Q1은 미완료다.

<a id="predicate-parser"></a>
## Predicate AST/parser

`Parser::parse_predicate`는 WHERE 없는 조건식 fragment 하나를 읽는다. 비교·IS NULL→NOT→AND→OR의
우선순위와 Group을 flat arena에 보존하고 optional semicolon 뒤 EOF를 요구한다. Name/Scalar의
원문 표현·marker1..256을 공유하며 raw 비교의 타입/값을 이 단계에서 평가하지 않는다.

AND/OR 형제는 반복문으로 읽고 NOT/Group은 합산 active depth64로 제한한다. 큰 enum 임시값은
재귀 전 반환하는 helper에서 생성하여256KiB stack을 지킨다. wrapper/placeholder 등록을 포함한
source/arena actual capacity와 old/new peak는 기존 session/DB quota에 charge한다. 빈 입력의
root 할당 전 Syntax·각 Unsupported 문맥·진입/loop/닫기/최종 호출 제어는 소유 계약을 따른다.

[원 source별 증거](../evidence/dev4-predicate-20261024.md)에 semantic393호출·8,192생성 항과
known-bad36/정상통제8, 자원9함수를 보존한다. r1은 의미6·자원6 완료 뒤 stack overflow로 중단했고,
r2에서 해당 frame을 수정하여 미완료3함수만 통과했다. r3은 독립 literal 타입 alias만 교정한
Clippy/fmt 검증이며 정상 runtime을 반복하지 않았다. 제품 mutant6개는 실제 의도한 오류로 거절됐다.
최종 독립 D1을 이 조건식 구문 leaf에 수용했으며 확대2profile은 NOT_RUN이다.

```bash
cargo test -p xadb-sql --features test-support --locked --test predicate_contracts <function> -- --exact
cargo test -p xadb-sql --features test-support --locked --test predicate_resources <function> -- --exact
```

실제 diff와 관계있는 함수만 선택한다. 이 구문 공급은 P3 전체 타입 검사/3값 평가·P4 WHERE 연결·
P5 추상 수명 모델·P6 실제 공급 수합 및 engine 통합·Q1 완료를 의미하지 않는다.

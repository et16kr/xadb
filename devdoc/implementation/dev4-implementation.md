# DEV4 독립 앞단 구현

2026-10-25 · **lexer·이름·splitter·P2/P4 raw 문장·P3 scalar·predicate 구문·typed 평가 제한 구현 / P4·P5 codec 최종 독립 D1 제한 수용 / P5 조립·모델·checker·메모리 BFS 제한 구현 / FE14·실제 SQL 실행·DEV4·Q1 미완료**

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
증거를 수합했다. 아래 P3 scalar 입력과 predicate AST/parser를 추가했다. 재귀 depth64·256KiB stack은 새 조건식 경로에서 검증했으며 아래 typed predicate에서 전체 타입 분석·3값 계산을 추가했다. 아래 P4에서 나머지 raw 구문을 추가했다. P6의 현재 공급/API 대조는 아래 수합에 기록했다. P5 추상 수명 모델, 후속 공급 수신과 실제 binding의 FE11/12는 미완료다.

실제 catalog binding·실행 CLI·session·engine adapter와 Q1 entrypoint는 제공하지
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

P3 조건식 구문·typed compile/eval과 P4 나머지 raw 문법은 아래 후속 절을 따른다. P5 수명 모델·P6 실제 DEV3 수합과 engine 통합·Q1은 남아 있다.

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

이 진입점은 실제 diff에 대응하는 함수 선택용이다. P3의 3값/전체 타입 분석은 아래 typed predicate에 추가했으며 실제 SQL 실행·Q1은 미완료다.

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

<a id="typed-predicate"></a>
## 명시 타입 compile과 3값 평가

`PredicateCompiler::compile`은 실제 Predicate root와 명시 ColumnSpec/hint에서 전체 이름·타입·
marker 해소를 마친 뒤 모든 literal과 세 exact arena를 소유한다. readonly CompiledPredicate는
AST/source·schema/hints·compiler를 폐기한 뒤에도 metadata·literal·진단 span을 유지한다.
`evaluate`는 전체 native row와 SQL parameter의 원래 tag를 먼저 검산하고 NULL/길이를 확인한 뒤
고정8KiB 배열로 eager3VL을 계산한다. raw NULL과 typed NULL, native empty와 SQL empty를 구별한다.

세 backing과 literal 실제 capacity를 같은 session/DB ledger에 charge하고 실패한 partial 출력만
반환한다. 긴 비교는128byte-pair마다 취소/deadline을 검사하며 local progress와 SQL span을 구분한다.
제품 평가 경로는 payload 복사·heap 할당·재귀가 없다. foundation/parser/scalar 제품은 변경하지 않는다.

[source별 증거](../evidence/dev4-typed-predicate-20261024.md)에 고정946case(946compile/688eval),
별도8192생성 operations·known-bad44/통제8·자원12함수와18제품 변조/19detector 실행을 보존했다.
r1/r2 compile FAIL과 r3 fixture FAIL을 남기고 r4는 교정·미실행47case만 검증했다. 원 실패 job의
완료140case와 앞서 통과한17함수는 반복하지 않았다. 확대2profile은 작성·NOT_RUN이다.
최종 독립 D1을 제한 수용했으며 실제 catalog/prepared/session·engine binding과Q1은 미완료다.

```bash
cargo test -p xadb-sql --features test-support --locked --test typed_predicate_contracts <function> -- --exact
cargo test -p xadb-sql --features test-support --locked --test typed_predicate_resources <function> -- --exact
```

실제 diff에 대응하는 새/수정 사례만 선택한다. 실패 그룹의 완료 prefix를 제외할 때는 기록된
`XADB_TYPED_FROM_CASE` exact ID와 원 source의 순차 단언 증거를 사용한다. 미지정은 해당 그룹 전체이며
이 진입점 자체가 완료 시험 반복을 허용하지 않는다.


<a id="remaining-parser"></a>
## SELECT·DML·DDL raw 문장

`Parser::parse`는 기존 INSERT/거래 구문에 SELECT, UPDATE, DELETE, CREATE TABLE,
CREATE [UNIQUE] INDEX, DROP TABLE/INDEX를 추가한다. 새 `parser/statement.rs`와
`parser/ddl.rs`가 기존 ParseInput의 source·flat arena·marker를 사용한다. WHERE의 자식은
기존 Or이며 별도 Predicate root/source 복사를 만들지 않는다. SET→WHERE와 WHERE→LIMIT의
marker ordinal은 문장 전체에서 이어진다. 원문과 모든 span은 기존 owned AST에 보존한다.

projection/ORDER 목록32개, AST8,192node, marker256개, NOT/Group64중첩과 기존 source/token
상한을 적용한다. DDL 목록은 반복문이고, 새 statement helper는 predicate nesting을 소비하지
않는다. 생략한 ORDER 방향/NULL 위치와 column nullability는 명시한 상태와 구별한다.
LIMIT는 unsigned digits 또는 marker만 읽고 범위 초과 숫자도 raw로 보존한다. DDL type_length는
부호·digits·전체 span을 보존하며 값 변환을 하지 않는다. 열/제약/index 개수의 semantic 한도,
중복·참조·PK NULL 및 길이 검산은 DDL precommit 이후의 owner 책임이다.

새 AST도 기존 session8MiB/DB64MiB quota와 actual capacity를 사용한다. DataType leaf는
전체 타입 형식을 확인한 뒤 추가하되 BIGINT/BOOLEAN은 keyword에서 형식이 끝난다.
item 시작·comma 끝·마지막 구문 token 끝에서 제어를 확인하고, contextual IF/CHECK 등의
추가 peek에서 생긴 Lex 진단을 보존한다. 오류·취소는 해당 호출의 partial 출력과 예약을 정리한다.

[source별 증거](../evidence/dev4-remaining-parser-20261024.md)의 정상 source
`95a9745a6a4aa32be4ba422bd48c67918a8e8c07d13a185adb19922c08f3c50c`에서 semantic8함수와
자원13함수·Clippy를 선택 검증했다. semantic은 고정659+경계129+생성8,192의 총8,980parse이며,
known-bad50/정상통제12는 제품 호출0으로 별도 판정했다. 확대256×1,024와30분 profile2개는
**NOT_RUN**이다. M01–M20과 교정한 M21의 의도한 단언 실패를 확인했고 M21 원 recipe의
compile FAIL·원 fmt FAIL과 서식 교정을 보존했다.21변조 범주와 최종 독립 D1을 제한 수용했다.

```bash
cargo test -p xadb-sql --features test-support --locked --test remaining_parser_contracts <function> -- --exact
cargo test -p xadb-sql --features test-support --locked --test remaining_parser_resources <function> -- --exact
```

실제 diff에 대응하는 사례만 선택한다. semantic exact ID는 `XADB_REMAINING_CASE`, 중단된
함수의 완료 prefix 제외는 `XADB_REMAINING_FROM_CASE`를 사용하며 둘은 함께 지정하지 않는다.
자원 세 함수의 witness 선택은 `XADB_REMAINING_RESOURCE_CASE`이며 그 외 semantic selector와
동시에 쓰지 않는다. 미지정은 기존 전체 함수 범위이고 완료 시험 반복을 허용하지 않는다.
이 구현은 raw 구문 공급이다. typed 문장 binding·P5 수명 모델·P6 전체 공급 수합·실제 catalog/
prepare/bind/engine·DDL precommit/SQL 실행과 DEV4/Q1은 미완료다.


<a id="supply-handoff"></a>
## 실제 공급과 연결 준비 수합

[P6 source별 대조](../evidence/dev4-supply-handoff-20261024.md)는 DEV3 `bc93de1`과 당시 dirty를
구분해 기존 공개 API, G5 private clean owner와 빈 Catalog/Free64 census, P1–P4 앞단 산출물의
실제 범위·원 영수증·잔여를 연결한다. 이 사실 수합은 독립 검토를 거쳤지만 새 제품 실행이나
최소 engine 공급 인수는 아니다. 후속 P4 commit과 engine dirty 변화도 원 관측과 분리했다.

공개 disk statement/undo·Catalog 변경·row/index·실제 commit/reopen·temp/cleanup 공급을
확인한 뒤 binding→첫 INSERT→commit/reopen→SELECT/fetch→실패/rollback의 새 연결 경로를
구현·선택 검증한다. P5의 첫 상태·입력 codec은 아래와 같이 구현했으며 실제 모델 전이·탐색·수용은 NOT_RUN이다.


<a id="lifecycle-codec"></a>
## P5 수명 상태·입력 codec

[첫 codec 계약](../contracts/dev4-lifecycle-codec.md)의 bounded flat 자료형·원 identity·Completed
payload 제거와 state/input byte codec을 reference에 구현했다. caller buffer를 재사용하고
잘못된 참조·slot/descriptor·continuation·owner/obligation 연결을 거절한다. 독립 literal reader는
원 JSON에서 기대값을 구성하며 malformed raw input의 원 byte와 순서를 보존한다.

[source별 선택 증거](../evidence/dev4-lifecycle-codec-20261024.md)에 native workspace1함수,
reader29상태, state4의 원 prefix/후속3 분리와 input7 양방향, codec10함수 및 reader순환을
기록했다. 원 compile·ParkedFetch·fmt 실패와 검토 보완을 보존하며 최종 독립 D1을 제한 수용했다.
완료 무변경 사례를 반복하지 않았다. state byte 함수의 시작 선택은
`XADB_LIFECYCLE_STATE_VECTOR_START=0..3`이며 완료 prefix를 생략한 범위만 새 결과로 기록한다.
전체 raw resolver·independent edge checker·모델 전이·BFS·파일 재개·FE14는 미완료다.


<a id="lifecycle-raw-selection"></a>
## P5 원시 action 번호·target 선택

[소유 계약](../contracts/dev4-lifecycle-raw-selection.md)의 raw1533 번호 변환과 exact target
selector를 별도 작은 단위로 구현했다. 순수 세 API만 대상으로 하며 full input/Env1 조립이나
handle 인증·모델 전이는 포함하지 않는다. [선택 기록](../evidence/dev4-lifecycle-raw-selection-20261025.md)에
독립1951입력·실제3484호출과 compile/Clippy/fmt PASS, 원 후보·정적 집계 실패 보존을 남겼다. 최종 독립 D1을 세 API 범위로 제한 수용했다.

<a id="lifecycle-assembly-inputs"></a>
## P5 전체 입력 조립의 준비

[설계·독립 입력 준비](../evidence/dev4-lifecycle-assembly-inputs-20261025.md)에서 정상1582와
오류·상한76을 정적 한정 검토하고 원 source/hash를 보존했다. 추가 rich304개는 미검토로 남긴다.
사용자의 정리·commit 요청에 따라 준비 단위를 보존하며 전체 D2·`assemble_input`·시험 harness는
미완료다. 이 단위의 제품 변경·compile/runtime/test는0이며 이전 완료 시험을 반복하지 않았다.

<a id="lifecycle-assembler"></a>
## P5 전체 입력 조립

[소유 계약](../contracts/dev4-lifecycle-assembler.md) D4-LM-ASM-001의
`assemble_input(&State, &RawAction, &mut ResolvedInput)`을 reference lifecycle에 구현했다.
21 family의 prefix/context/control, A1 Bind payload, Env1 class·receipt/anchor, 인증·continuation과
transaction 선택, owner/obligation recipe·fence census·recovery negative를 조립한다. 오류는 S0–S5
순서이며 사용 namespace만 검사하고 State를 바꾸지 않는다. 제품 경로는 heap·재귀·State clone이 없다.

[선택 증거](../evidence/dev4-lifecycle-assembler-20261025.md)에 준비 후보1658·rich304·독립 supplement의
선택 실행, rich 후보 정정2건, 개발 중 불일치·교정 이력을 기록했다. 모델·checker·메모리 BFS의 후속 구현은
아래 절에 연결한다. 영속 재개와 FE14는 미완료다.

```bash
cargo test -p xadbtest-reference --lib --offline --locked lifecycle::assembler_tests::<function> -- --exact
```

실제 diff에 대응하는 함수만 선택한다. `XADB_ASSEMBLY_CASE`는 한 case ID, `XADB_ASSEMBLY_FROM_CASE`는
완료 prefix를 제외한 suffix를 고르며 둘은 함께 쓰지 않는다. 어느 그룹에도 없는 ID는 실패한다.

<a id="lifecycle-model"></a>
## P5 수명 모델 전이

[소유 계약](../contracts/dev4-lifecycle-model.md) D4-LM-MODEL-001의
`transition(&State, &ResolvedInput, &mut State, &mut Edge)`을 reference lifecycle에 구현했다. fresh 호출의
envelope→child→shell→parked→fence→API 상태→entry control→환경→admit gate, 21 family 전이, 공급 Env1의
fact·owner·obligation 기록과 canonical 정렬을 수행한다. 제외는 상태를 바꾸지 않고, 모델 오류는 게시하지 않는다.

[선택 증거](../evidence/dev4-lifecycle-model-20261025.md)에 원 literal 29 edge·12 trace·5 known-bad 대조,
census·walk smoke, 고정 trace와 1차 검토 교정 이력을 기록했다. 후속 checker·메모리 BFS는 아래 절을 따르며
FE14는 미완료다.

```bash
cargo test -p xadbtest-reference --lib --offline --locked lifecycle::model_tests::<function> -- --exact
```

`model_deep_walks`의 walk 수는 `XADB_MODEL_WALKS`(기본40)로 정한다. 실제 diff에 대응하는 함수만 선택한다.

<a id="lifecycle-checker"></a>
## P5 독립 edge checker

[소유 계약](../contracts/dev4-lifecycle-checker.md) D4-LM-CHECK-001의
`verify_edge(&State, &ResolvedInput, &Edge, &State)`를 reference lifecycle에 구현했다. 사전 상태와 원문
규칙만으로 class·응답·context event·controls·소비 bit·authenticated와 `pre ⊕ delta` 후상태를 다시 계산하고,
공급 Env1을 독립 recipe와 비교하며 환경 제외의 조건도 판정한다. 모델·조립기를 호출하지 않는다.

`differential_tests`는 모델 출력을 checker로 판정한다. 판정한 불일치·검토 결정11건(BFS 판정을 더해 12건)은 모델 계약의
[차등 판정](../contracts/dev4-lifecycle-model.md#checker-adjudication)에 기록했고
[선택 증거](../evidence/dev4-lifecycle-checker-20261025.md)에 실행 기록을 남겼다. 후속 메모리 BFS는 아래 절을
따르며 FE14는 미완료다.

```bash
cargo test -p xadbtest-reference --lib --offline --locked lifecycle::checker_tests::<function> -- --exact
cargo test -p xadbtest-reference --lib --offline --locked lifecycle::differential_tests -- --nocapture
```

walk 크기는 `XADB_DIFF_WALKS`(기본30)·`XADB_DIFF_DEPTH`(기본6), 한 관측의 재현은
`XADB_DIFF_REPLAY="raw,…:raw"`로 정한다. 실제 diff에 대응하는 함수만 선택한다.

<a id="lifecycle-explorer"></a>
## P5 결정적 BFS 탐색기

[소유 계약](../contracts/dev4-lifecycle-explorer.md) D4-LM-BFS-001의 `explore`를 controller `lifecycle`에
구현했다. 독립 initial vector부터 FIFO 순서로 모든 raw slot을 분류하고 alias가 아닌 slot마다 모델과 checker를 한
번씩 호출한다. worker가 상태를 평가하고 하나의 merge가 discovery ID와 ledger 순서를 정하므로 결과는 worker 수와
무관하다. full canonical bytes로 중복을 합치고 edge8(state cap이 2^19 이상이면 edge16) ledger와 discovery32
stream의 SHA, 깊이·API·rule별 counter를 보고한다. archive와 durable checkpoint는 없어 FIFO가 비어도
`DrainedUnarchived`다.

[실행 증거](../evidence/dev4-lifecycle-explorer-20261025.md)에 설계 상한 profile과 사용자가 승인한 깊이 5 profile의
결과를 기록했다. witness 검사·FE14·깊이 6은 미완료다.

```bash
cargo test -p xadbtest --lib --offline --locked lifecycle::tests::<function> -- --exact
```

`profile_run`은 `XADB_BFS_RUN=<report>`일 때만 실행하며 `XADB_BFS_PROFILE=depth5`와 `XADB_BFS_WORKERS`로 profile과
worker 수를 고른다. 실제 diff에 대응하는 함수만 선택한다.

2026-10-26 [병합 전 리뷰·수정](../evidence/dev4-merge-review-20261026.md): 선택 참조의 scope·역참조 검사와
탐색기 사전 자원 한도를 보완했다. 신규13함수·관련 정상9회만 선택 검증했으며 원 깊이 5·하위 회귀는 반복하지 않았다.

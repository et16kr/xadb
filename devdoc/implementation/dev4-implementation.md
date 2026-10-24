# DEV4 독립 앞단 구현

2026-10-24 · **lexer·이름 제한 구현 / 실제 SQL 실행·DEV4·Q1 미완료**

[독립 앞단 계약](../contracts/dev4-independent-frontend.md#lexer-leaf) · [병행 계획](../plans/dev4-preparation-plan.md) · [source별 증거](../evidence/dev4-frontend-20261024.md)

## 구현 범위

`xadb-sql`의 `frontend`는 source를 빌려 읽는 lexer, 원문 byte span·할당 없는 진단,
호출별 취소/monotonic deadline과 고정128-byte 정규 이름을 제공한다. 원문1 MiB와
nontrivia token65,536 한도를 적용하며 UTF-8·긴 token/comment·이름 decode 내부에서
제어를 검사한다. 미인용 ASCII 이름은 대문자화하고 인용 UTF-8 bytes·두 quote escape를
보존한다. 이름 identity는 bytes이며 PUBLIC namespace 해석은 후속 parser 책임이다.

normal dependency는 foundation 하나다. borrowed scanner와 고정 이름은 heap allocation을
하지 않는다. 반환된 이름은 원문 수명과 독립이고 lexer는 source/control을 빌린다.
source/AST를 소유하는 parser의 session8 MiB/DB64 MiB quota는 아직 구현하지 않았다.
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

## 계속할 작업

첫 P0/P1 소비 부분을 구현했으며 P1 streaming splitter, P2 공통 parser·INSERT/거래 구문,
P3 scalar/typed predicate, P4 나머지 구문, P5 추상 수명 모델, P6 공급 수합은 남아 있다.
다음 종료점은 같은 quote/comment/UTF-8 경계를 사용하는 streaming splitter다. chunk 보관의
공유 quota·할당 실패·진단 좌표·EOF와 chunk 끝 구별을 실제 입력으로 동결한 뒤 구현한다.

단일 API 한 문장 검사·AST/depth/marker 번호·값 변환·catalog binding·실행 CLI·session·실제
engine adapter는 이 scanner가 제공하지 않는다. Q1 entrypoint와 실제 SQL 인수 결과도 없다.
engine 연결은 DEV3 catalog/row/index/statement/undo/commit/recovery/temp의 실제 선행 증거 뒤다.

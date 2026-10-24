# DEV4 나머지 raw 문장 구문 — 2026-10-24

현재 **필수 선택 검증 완료·최종 독립 D1 제한 수용 / DEV4·Q1 미완료**다.

[소유 계약](../contracts/dev4-independent-frontend.md#remaining-parser) · [병행 계획](../plans/dev4-preparation-plan.md) · [실행 색인](dev4-remaining-parser-20261024.json)

## 범위와 구현 전 동결

P4의 SELECT·UPDATE·DELETE·CREATE TABLE/INDEX·DROP TABLE/INDEX를 기존 owned flat AST에
추가했다. SELECT projection·ORDER BY·LIMIT, UPDATE assignment, 직접 Where→Or 연결과
raw DDL 타입·길이·NULL 선언·PK/UNIQUE·STORAGE DISK 구문을 제공한다. 같은 source/arena와
전체 문장의 marker 순서를 유지한다. binder·실제 Catalog·행 실행·DDL precommit은 이 결과에
포함되지 않는다. LIMIT는 부호 없는 원문 숫자를 보존하며 i64 범위와 DDL 의미 제한은 뒤 owner가
판정한다. ORDER 이름은 이후 입력 열 lookup 대상이며 projection alias로 대체하지 않는다.

기준 commit은 `57db415c0f4a1087fa01fac13e92f6552cef927a`다. 별도 DEV3 dirty는 source export에서
제외했고 후속 `bc93de1`의 clean owner commit을 보존한다. DDL 제품·조사·자원 입력은
`/root/frontend_research`, 독립 literal/생성 oracle는 `/root/frontend_oracle`, 계약·SELECT/UPDATE/
DELETE·조립·실행·기록은 `/root`, 독립 D2/D1은 `/root/frontend_review`가 맡았다.

D2는20:43:54 KST 독립 수용,20:44:31 KST 소유자 채택이며 그 전에 제품 작성·컴파일·시험은
없었다.30개 입력 사본과 SHA는 `artifacts/dev4-remaining-parser-20261024/d2-frozen-inputs/`와
`d2-adoption.json`에 보존한다. 채택 전 `UPDATE t a=1`의 진단 분류, INDEX와 PK/UNIQUE key-start
구별66건, exact mutation selector를 정본에 맞춰 보완한 원 기록도 남긴다. 채택 상태 문서와
실행 기록을 동결 입력 원본에 소급 덮어쓰지 않았다.

PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의 gram.y/parse_utilcmd.c와 SQLite
`43a48f77585a88940c727f33682151418f544059`의 parse.y/build.c를 읽었다. 두 checkout의 실제
revision·clean 상태·파일 SHA·열람 범위와 채택/비채택 이유는 research.json에 있다. raw 구문과
후속 의미 검산의 분리를 참고했으며 외부 방언·구현과 동일하다는 주장이나 외부 시험 결과는 없다.

## 자원·진단과 선택 검증

새 payload는 고정 크기이며 원문/AST 외의 제품 heap backing을 추가하지 않는다. DDL root는
다음 token보다 먼저 arena에 등록한다. VARCHAR 길이 구문은 완결한 뒤 DataType을 등록하고,
BIGINT 같은 고정 타입은 keyword에서 완결하므로 뒤 suffix보다 먼저 등록한다. Name 분류 전
목록 item 시작, comma 끝, 항목 완결, 숫자 확장 검사와 최종 게시의 제어 지점을 유지한다.
WHERE wrapper는 조건식 depth를 소비하지 않으며 기존 predicate helper를 depth0으로 부른다.

필수 semantic8함수와 자원13함수만 선택했다. 고정 literal659건은658개 서로 다른 원문을 갖는다.
`update/minimum`과 `raw/update/zero`는 같은 `UPDATE t SET a=0`을 별도 범주에서 관측하는
명시 alias다. boundary129를 더한 고유 case ID는788개이며 생성64seed×128의8,192회를 더해
semantic 제품 parse 호출은8,980회다. 자원 호출 수는 이 분모에 합산하지 않는다.
known-bad50개와 정상 통제12개는 제품 호출0이다. 독립 oracle는 모든 node kind·payload·span·
child/sibling·원문·marker·진단·complete를 비교하며 제품 결과를 golden으로 만들지 않는다.

기존 P2에서 첫 SELECT/UPDATE/DELETE/CREATE/DROP의 오류 기대값5개만 새 route의 EOF Syntax로
바뀌었다. 같은 원문·ID·순서를 유지한 focused5건을 새 adapter에서 검증했다. 나머지117개 원
literal이나 기존 suite를 반복하지 않았다. 기존 parser adapter의 새 enum 처리만 compile로 확인했다.

자원13함수는 독립 소유, 각 새 route의 session/DB 경계, 실제 capacity 초과분,11 allocation site,
partial 실패·기존 출력 유지, admission 순서, entry/final·항목·comma·이동·WHERE unwind·긴 숫자
제어, 공유 DB 합산을 확인했다. 공유 DB는 첫 thread의 출력을 보유한 채 다음 thread를 실행한
회계이며 동시 할당 경합 시험으로 부르지 않는다.256KiB native thread에서 최대8,192node의
flat parse/Drop과 SELECT/UPDATE/DELETE depth64 성공·65 실패를 확인했다.

실제 resource ELF의 DWARF에서 AstNode224byte·ParsedStatement184byte·Parser64byte를 읽었다.
관련8개 함수 prologue와 ELF SHA는 layout-stack-review.json에 있다. 세 재귀 frame과 return
주소의64층 합158,720byte는 해당 경로의 정적 계산이다. 이것만으로 전 call graph의 stack 상한을
증명했다고 하지 않으며256KiB 실제 실행을 별도 근거로 보존한다.

확대256×1024와30분 valid grammar는 선택 가능한 ignored2함수로 작성했으나 NOT_RUN이다.
정상 grammar의 token65,536 경계는 AST 한도가 먼저 도달하므로 해당 token 경계는 N/A다.
전체 workspace·하위 wrapper·기존 완료 runtime과 문서 verifier는 실행하지 않았다.

## 실제 source·원 실패와 보존

정상 source-r1은 기준 archive에62개 DEV4 overlay만 적용한4,498개 파일이다. SHA는
`95a9745a6a4aa32be4ba422bd48c67918a8e8c07d13a185adb19922c08f3c50c`다. source별 `/tmp`
target·incremental off·180초 상한으로 실행했다. process receipt와 로그를 먼저 보존하고 ELF는
lossless gzip으로 보존하며 압축/원 ELF SHA를 각각 검산했다.

| 선택 실행 | 실제 결과 |
| --- | --- |
| xadb-sql all-target/all-feature Clippy | PASS, runtime0 |
| remaining_parser_contracts |8함수 PASS,2확대 ignored; 고정788+생성8,192 parse |
| remaining_parser_resources |13함수 PASS |
| cargo fmt check |원 FAIL: 기존 reference의 새 at(...) 호출 줄바꿈1곳 |
| 서식 교정 파일만 rustfmt check |PASS; 인수 줄바꿈과 선택적 trailing comma만 변경, runtime 재실행0 |

제품5파일과 semantic/resource 입력은 최초 정상 실행부터 변하지 않았다. 서식 변경 전 원 파일과
FAIL, 변경 후 파일/hash와 정확 한 구간 치환 검산은 format-only.json 및 format-only/에 남긴다.
초기 단순 whitespace 제거 비교는 trailing comma 차이로 실패했으며, 이 보존 점검의 교정도 기록했다.
이것은 제품 시험 실패나 재실행이 아니다.

## 제품 변조와 독립 리뷰

구현 전에 동결한21개 의미 편차를 실제 제품 source의 exact edit와 연결했다. 각 독립 사본에서
해당 semantic case 또는 자원 witness만 실행했다. M17은 문서화한 함수 첫33번째 Name 제어
witness에서 실패하므로 뒤 입력이 실행되지 않는다. 정상 실행과 완료한 다른 변조는 반복하지 않았다.

M01–M20은 지정 단언으로 거절됐다. M21 최초 recipe의 `b'E'` 닫는 따옴표 누락은 E0763 compile
FAIL·runtime0이며 검출로 세지 않는다. 원 recipe/source/log를 유지한 채 m21-r2에서 따옴표만
교정했고, 동일 numeric/exponent 선택자가 DDL 숫자 확장의 내부 제어 지점 소실을 검출했다.
따라서21범주의 runtime 거절과 원 compile 실패1건을 구별한다. 단순 exit101·timeout·컴파일 오류는
검출 PASS가 아니다. 각 실제 단언과 의도한 편차 연결은 mutant-results.json에 기록한다.

보존 감사는 정상/변조23source의103,454개 파일 항목,26개 receipt와24개 서로 다른 보존 binary의
SHA·원 ELF 크기·로그·process receipt 연결을 확인했다.24개는 시험 ELF23개와 proc-macro 공유 객체1개다. 현재62개 코드/fixture는 source-r1과
서식 전용 예외1개를 구별해 대조했다. 초기 감사 초안의 전체 고정 원문 수 추정은 제거하고 실제
literal659/658과 boundary129를 분리했다. 이 감사는 제품 재실행 결과가 아니다.

최종 독립 D1은21:08:39 KST 제한 수용했다. 검토자는 실제 제품·정본·동결 입력과22개 변조
recipe를 읽기 재구성하고 보존 감사의 불일치0건을 확인했다. 현재 제품5파일은 정상 source와
동일하며,62개 코드/fixture의 서식 예외1개를 구분했다. 검토자는 제품/시험 수정이나 compile/
runtime·문서 verifier를 실행하지 않았다. 원문·해시는 d1-review.md/.json과 이 실행 색인에 남긴다.

P5 추상 수명 모델, P6 최신 DEV3 공급/API 수합과 실제 engine 연결·
Q1은 미완료다. 이번 raw AST 성공은 실제 행 변경·DDL commit·복구·내구성의 증거가 아니다.

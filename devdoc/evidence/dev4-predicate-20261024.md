# DEV4 predicate AST/parser — 2026-10-24

현재 **선택 검증 완료·최종 독립 D1 제한 수용 / P3 전체·DEV4·Q1 미완료**다.

[소유 계약](../contracts/dev4-independent-frontend.md#predicate-parser-leaf) · [병행 계획](../plans/dev4-preparation-plan.md) · [실행 색인](dev4-predicate-20261024.json)

## 범위와 구현 전 동결

사용자의 DEV3 병행 독립 DEV4 개발·서브 agent·단계별 commit 요청을 계속한다. 기준 commit은
`ae176b6b0cc15b74ee9620c49b970bcca33d68bd`다. 별도 DEV3 dirty와 후속 변경은 baseline/ownership
기록에 보존하며 이번 export·시험·stage에 포함하지 않는다. 제품 담당은 `/root/frontend_research`,
독립 oracle·semantic adapter는 `/root/frontend_oracle`, 계약/자원/조립/실행/기록은 `/root`,
읽기 전용 독립 D2/D1은 `/root/frontend_review`다.

이번 범위는 4.03.1.2의 조건식 구문이다. `parse_predicate`가 WHERE 없는 fragment 하나를 받아
원문과 flat AST를 반환하며 기존 source/arena의 session·DB 예약을 유지한다. 비교/IS NULL,
NOT/AND/OR·괄호 우선순위와 n-ary sibling·공통 active depth64·node8192·marker256을 다룬다.
원래 scalar syntax를 보존하며 raw parse 성공은 타입 검사·값 평가·실제 SQL 성공이 아니다.

PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의 gram.y·parse_expr.c·parse_coerce.c와
SQLite `43a48f77585a88940c727f33682151418f544059`의 parse.y·expr.c에서 선택한 행을 읽었다.
revision·clean 상태·5개 파일 hash·열람 범위는 research.json에 보존했다. n-ary boolean·명시
깊이 한도·전체 의미 검사와 실행 분리를 참고했으며 broader grammar·implicit coercion·NULL
comparison rewrite·상수 접기는 도입하지 않는다. 외부 DBMS 실행이나 동작 동등성 증거는 아니다.

독립 D2는 실행 전에 quota/control 오류 위치·empty 입력 할당 우선순위·내부 Group 닫기 제어,
thread 실패 시 barrier 교착 방지·미도달 주입 소비를 검토했다. 마지막 교정은 계약 ID와
Unsupported operand suffix/일반 후행 Syntax의 문장 구분이며 기대 AST를 제품 출력에 맞춘
변경은 없다. 원 지적과 모든 검토 사본을 artifacts/dev4-predicate-20261024에 보존한다.

18:37:39 KST에 최종 독립 수용을 채택하고12개 입력 사본을 동결했다. 당시 predicate 제품·컴파일·
시험은 없었다. 검토 manifest `3e90e4decef131524462b8833fa133f1338523f259f007782bb21b9b4215f185`,
채택 상태만 갱신한 manifest `44490358d7c484d2afaeaa59347fd9ab4aa3f53e565a2dfc85911dc1d14b7f23`이다.

## 필수 선택과 미실행 확대

필수 semantic은6함수다. 35개 구조·137개 raw 입력·128개 오류·29개 경계·64개 생성 parse로
393호출이며, 생성은64seed×128조건 항=8,192fragment다. known-bad36개와 정확 수락 통제8개를
별도로 센다. 독립 기대값은 전체 preorder node 배열·payload·span·연결·원문·marker·오류·complete를
비교한다. 제품 parser/tokenizer/name 계산을 정답으로 쓰지 않는다.

자원9함수는 retained AST/decoded 값의 수명·session/DB quota 경계와 partial peak·actual capacity,
새 진입의8개 allocation 실패와 미도달9번째 주입 소비, 진입/내부/최종 제어, 두 실제 caller의
공유 ledger,256KiB stack·깊이/flat8192 node와 실패 후 재사용을 다룬다. 기존 lexer/name/splitter/
P2/scalar의 완료 runtime은 반복하지 않는다. scalar helper의 반환 Span과 기존 관측 adapter의
새 enum arm은 기계적 변경이며 새 predicate 소비 시험과 static compile로 확인한다.

별도 확대0..255seed×1,024fragment(2,048 parse calls/262,144fragment)와30분 grammar-valid 생성은
선택 가능한 ignored 함수이며 NOT_RUN으로 남긴다. 임의 malformed-byte fuzz로 부르지 않는다.
제품 mutant 필수6범주는 depth·우선순위·marker·fragment EOF·node cap·최종 제어다.

## Source별 실행과 독립 D1

기준 commit의 clean archive에 DEV4 45개 overlay만 적용했다. 각 source는 기준 commit과 실제
파일 hash를 함께 지문으로 계산하고 별도 target·180초 외부 상한·로그·실행 파일 hash를 보존한다.

| source | 선택과 실제 결과 |
| --- | --- |
| r1 `75b50919c08baf05e265e1f591b3a515286b88bc9669a65725153b8ed77a10d4` | semantic6·자원6 완료. 256KiB native thread의 stack overflow로 자원 process SIGABRT. stack/allocation/interior3은 완료 미확인으로 보존 |
| r2 `e31487696a4db6bc6b44bb596fd8f1812180cf6e6d6ceeb6eb371b0c5d6245d1` | predicate.rs의 재귀 frame만 축소. 미완료3함수만 exact선택 PASS. 기본 build/fmt PASS. reference tuple type의 Clippy FAIL 보존 |
| r3 `39fe27ce262076ddd5b311318582a62d2d1d235dbb271e67aa4789169bc9c5f7` | local 타입 alias와 fixture manifest 이력만 교정. 제품과 기대값 불변. Clippy/fmt PASS, 정상 runtime0 |

r1의 Group 한 단계는 predicate/and/term의1,272+1,272+2,040=4,584byte를 보유했고 반환주소를
포함한64단계만294,912byte로256KiB를 넘었다. 비재귀 helper가 큰 NodeKind 임시값·도입/종료
분기를 처리한 뒤 반환하도록 바꾸었다. 새 frame은904+904+648=2,456byte, 반환주소 포함64단계는
158,720byte다. 이것은 해당 재귀 frame만의 정적 분석이며 전체 호출은 실제256KiB 시험으로 확인했다.
깊이64·AST8192와 제어/할당/문법 순서를 바꾸지 않았다. 원 stack 분석의 덧셈 오기4,552/291,328은
원본을 유지하고 stack-review.json에서4,584/293,376으로 바로잡았다.

Clippy의 독립 error literal tuple 지적은 local ErrorLiteral 타입 별칭만 도입했다. 역치환 시 r2
파일과 byte 단위로 같으며 SQL·기대 AST/오류·배열·분모는 바뀌지 않았다. 원 동결 사본과 r1/r2
manifest·FAIL을 유지하고 현재 manifest에 교정 hash/이유를 연결했다. 완료된 정상 시험을 반복하지
않았으며15개 필수함수의 결과는 r1/r2 각각의 source에 귀속한다. r3 전체 runtime PASS로 쓰지 않는다.

r3에서 독립 생성한6개 제품 mutant의 실제 binary를 실행했다. 깊이65 수락은 Limit 대신71nodes,
우선순위 교환은 첫 And span/다음 형제 불일치, Group marker reset은 ordinal1/3·전체2/4 차이,
EOF 제거는 비교 chain 수락, node cap 증가는8193nodes 수락, 최종 제어 제거는 checkpoint0/1 차이로
각각 거절됐다. 단순 test FAIL은 먼저 TEST_FAILED_PENDING_REVIEW로 두고 실제 의도한 assertion을
읽어 검출로 판정했다. compile/setup 실패를 mutant 검출로 집계하지 않았다.

할당·stack·취소와 공유 ledger는 이 새 API의 계약 검증이며 RSS/OOM·실제 engine/session 거래
효과의 증거가 아니다. 기본 feature compile과 SQL/reference all-targets/all-features Clippy는
기존 소비자를 정적으로 검사했으며 기존 제품 시험을 실행하지 않았다. 새 Cargo target은 별도
/tmp tmpfs에 두고 incremental을 껐으며 실행 파일은 SHA별로 artifact에 복사·재검산했다.
최종 독립 D1은9개 source·15개 실행 기록·보존 binary와 현재45개 overlay를 대조해 이 조건식 구문 leaf를 수용했다. 추가 blocking 지적은 없으며 typed compile/eval·engine 통합·Q1의 미완료는 유지한다.

P3 typed compile/eval·3값 논리, P4 나머지 문법, P5 추상 수명 모델, P6 실제 DEV3 공급/API 차이
수합과 engine 연결·Q1은 미완료다. 후속 typed predicate 입력 제안은 병행 조사일 뿐 채택/실행
증거가 아니다. 문서 자동 검사는 push gate 전용이며 이번에는 push하지 않는다.

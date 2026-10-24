# DEV4 명시 타입 조건식 compile/evaluate — 2026-10-24

현재 **필수 선택 검증 완료·최종 독립 D1 제한 수용 / DEV4·Q1 미완료**다.

[소유 계약](../contracts/dev4-independent-frontend.md#typed-predicate) · [병행 계획](../plans/dev4-preparation-plan.md) · [실행 색인](dev4-typed-predicate-20261024.json)

## 범위와 구현 전 동결

DEV3와 병행 가능한 P3 4.04.3의 전체 타입 검사·3값 평가를 구현했다. `PredicateCompiler`는
실제 Predicate AST와 명시 schema/hint를 받아 모든 이름→concrete 타입→미해결 marker를 검사한
뒤 literal을 전부 decode한다. compiled 출력은 원문·AST·schema·compiler 수명과 독립이다.
`evaluate`는 native row와 SQL parameter를 호출 동안 빌리고 모든 원래 tag→NULL/길이 검사를
끝낸 뒤 eager 3VL을 계산한다. 실제 catalog binding·prepared/session·engine 연결은 제공하지 않는다.

기준 commit은 `186085e49cf11308fca47bb13775168bdcaa5f54`다. 별도 DEV3 dirty와 후속 commit은
baseline에 보존하며 이번 source export·시험·stage에서 제외한다. 담당은 제품/조사
`/root/frontend_research`, 독립 oracle `/root/frontend_oracle`, 계약/자원/조립/실행/기록 `/root`,
별도 D2/D1 `/root/frontend_review`다. D2 채택은 19:29:02 KST이며 그 전에20개 입력 사본을
동결했다. 당시 제품 작성·컴파일·시험은 없었다. 원본과 hash는
`artifacts/dev4-typed-predicate-20261024/{freeze,d2-acceptance}.json` 및 `frozen-inputs/`에 남긴다.

PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의 executor/parameter/type/byte 비교와
SQLite `43a48f77585a88940c727f33682151418f544059`의3VL/값 비교를 읽었다. 두 checkout은 clean이었고
6개 파일 hash와 실제 열람 범위를 research.json에 기록했다. whole typing·원래 parameter tag·3VL을
참고하되 implicit coercion, NULL total-order equality, 전체 payload memcmp, VM/short-circuit은
채택하지 않았다. 외부 DBMS 실행이나 동일 구현/동작을 증명한 결과는 아니다.

## 소유·자원과 선택 범위

compiled column/parameter/node는3개의 exact typed backing을 갖고 nonempty literal은 기존
ScalarReader의 별도 payload를 소유한다. session→DB base 예약 후 actual capacity 초과분을
게시 전에 보정한다. data를 lease보다 먼저 drop하며 이전 결과와 이번 partial 출력의 charge를
구분한다. fault ordinal은 전체 compile에서 합산하고 실패한 literal site도 반영한다.

평가는 원래 native 빈 Utf8를 보존하고 SQL parameter의 빈 Utf8만 같은 타입 NULL view로 본다.
고정8KiB truth 배열과 역 preorder 순회로 재귀·제품 heap allocation을 피한다. 긴 비교는 unsigned
byte 순서이며128쌍마다 제어를 검사한다. observer의 local byte 진행과 반환 진단의 SQL Compare
span을 분리한다. heap0 근거는 실제 평가 call graph 정적 검토와 잔여 quota0의 사용량/peak 불변
시험을 함께 사용한다. quota 수치만으로 global allocator 전체를 측정했다고 주장하지 않는다.

필수는 새 semantic7함수와 자원12함수다. 고정946 canonical case는946 compile과688 evaluate
입력을 갖는다. coverage959cell 중13개는 같은 canonical 입력의 alias다. 생성64seed×128은
별도8,192 compile+8,192 evaluate operations이며 고정 case 분모와 합쳐 부르지 않는다.
known-bad44와 정상 통제8은 product call0이다. 독립 oracle는 전체 typed node·link·span·literal·
column/parameter metadata·error·truth·WHERE·complete를 비교하며 product 결과를 golden으로 쓰지 않는다.

자원12함수는 독립 수명, 정확 quota 경계와 actual capacity,5 allocation site·미도달 주입 소비,
전체 검산 전 할당 없음, 각 compile/eval/metadata/input/긴 byte loop의 제어, 공유 DB ledger,
256KiB native stack에서 depth64·8192nodes·256markers·32columns 및 flat Drop을 다룬다.
공유 DB 시험은 첫 thread가 반환한 결과를 보유한 채 두 번째 thread를 실행한 합산 회계이며
동시 allocation 경합 시험으로 부르지 않는다. 기존 parser/scalar/lexer와 하위 계층의 완료 runtime은
반복하지 않았다. 기존 adapter의 새 enum 관측 arm은 static compile로 확인했다.

확대256×1024와30분 valid typed grammar는 선택 가능한 ignored 함수로 작성했으며 NOT_RUN이다.
문서 verifier는 push gate 전용이고 이번에는 push하지 않는다.

## 원 source·실패·선택 재검증

기준 clean archive에 DEV4 54개 overlay만 적용하고 모든 파일의 SHA를 보존했다. 실행은 source마다
별도 `/tmp` target·incremental off·180초 상한을 쓴다. 로그와 process receipt를 먼저 보존하고 ELF는
lossless gzip으로 저장한다. 원 ELF SHA와 archive SHA를 함께 기록하고 압축을 풀어 원 hash를 확인했다.

| source | 실제 결과와 이후 선택 |
| --- | --- |
| r1 `d2d99a096ea63a79c287a7a9af322eda4433fd69c76edf875710e074aa87ab0c` | 기본 제품 build PASS. 새 reference nested-if의 Clippy FAIL. runtime0 |
| r2 `457f298a513ccc8eebacc8e26fa9bcbd6d8a54f12e8ecfb70073e9ad567f972f` | 동등 if-let chain 교정. 새 resource observer Span 추론의 compile FAIL. runtime0 |
| r3 `725345d45a3b8a87a9466fa6ff68b4252c246169ec0612d8b398e232b45f7250` | 명시 Span 표기 뒤 Clippy/fmt PASS. 정상17함수 PASS,2함수 fixture 불일치 FAIL 보존 |
| r4 `df9701e863aa58d0335f0171b571ed027605a9808157e4c0724fc4621cdc0295` | 계약 근거 fixture13건 교정·선택 adapter 추가. Clippy/fmt PASS, 실패 지점부터47 compile/29 evaluate만 선택 PASS |

제품4파일은 r1부터 r4까지 같다. r3의 parameter 오류는 incompatible hint12건의 기대 span이
Compare 전체 `[0,3)`이었던 것이다. 정본은 작은 ordinal marker의 `[0,1)`이며 제품 관측이 맞았다.
32열 정상 case는 마지막 row가 Utf8(`x`)인데 schema/parameter는 Int64와1, 기대값은 True였다.
그 입력만 Int64(1)로 교정했다. 공유 list alias가 원인일 가능성은 있으나 원 bulk 작성 helper가
보존되지 않아 원인으로 확정하지 않는다. materialized 입력·literal 작성 스크립트·동결 사본은 보존한다.

독립 작성자와 검토자가 정본을 대조해 이13건만 교정했다. 나머지933case와 모든 기대 truth/plan·
ID/순서/분모는 그대로다. manifest는 원 hash와 교정 이력·현재 hash를 함께 갖는다. 추가 정적 입력
일관성 점검은 runtime PASS로 바꾸지 않는다. 자세한 diff·근거는 fixture-correction.json에 남긴다.

r3 실패 함수의 앞부분은 순차 assert_case가 성공한 뒤 다음 case에서 panic했다. parameter91개와
schema49개의 완료 단언은 원 실패 job/source에 귀속한다. r4는 `XADB_TYPED_FROM_CASE`로 각각
45개/2개만 실행했다. 원 전체 inventory 검산은 유지하고 잘못된 ID·비 UTF-8 선택값은 실패한다.
140개 완료 prefix를 새 실행으로 반복하거나 r3 실패 job을 PASS로 바꾸지 않았다. source별 완결
759case·실패 job 내 완료140case·r4의47case가946개 고유 case를 덮는다. 원 실패 시도2건을 포함한
실제 fixed compile 호출은948, eval은689다. 생성 operation과 자원 호출은 별도다.

## 제품 변조와 최종 검토

구현 전에 동결한18개 변조 범주를 source-r4의 exact edit와 연결했다. 각 source는 정상 사본을
보존하며 지정한 detector 하나만 실행하고 resume 환경변수를 명시 제거한다. compile 실패·timeout·
단순 종료코드는 검출 PASS가 아니다. 실제 failed assertion을 읽어 의도한 결함과 연결한 뒤 판정한다.
18개 범주의 실제 거절을 확인했다. Name/Type/Unknown/decode 우선순위, hint와 typed NULL,
empty 원천·3VL/WHERE·NULL 비교·byte signedness/끝 공백, 내부 제어·actual capacity·예약 반환·
최종 게시가 각각 detector에 연결됐다. M02는 첫 Unknown 우선순위 거절 뒤 미실행 Type 사례를
추가 선택하여 Value/Range 대신 Compile/Type이 먼저여야 함을 확인했다. M16은 실제 backing
할당 전의 base 예약192byte 누출을 거절한 것이며 buffer allocation 누출 증거로 확대하지 않는다.

최종 독립 D1은22개 source의97,064개 파일 항목, 정상28개와 변조19개 receipt,22개 보존 binary의
압축/원 ELF SHA와 로그 연결을 대조했다. 현재54개 overlay와 source-r4 일치·제품4파일의 r1 이후
불변·13건 교정·prefix140개 비반복과946case 수합을 확인하고 이 typed predicate 범위를 수용했다.
추가 blocking 지적은 없고 검토자는 제품 시험·compile·문서 verifier를 실행하지 않았다.

P4 나머지 문법, P5 추상 수명 모델, P6 최신 실제 DEV3 공급/API 차이 수합과 engine 연결·Q1은
미완료다. 이번 pure typed predicate의 성공은 실제 SQL 행 처리·commit·복구·내구성의 증거가 아니다.

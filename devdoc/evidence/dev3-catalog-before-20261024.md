# DEV3 Catalog의 typed Undo before — 제한 선행 검증

2026-10-24 · **독립 D2 입력 동결 수용 / r1 필수123 PASS / 최종 독립 D1 제한 수용**.
기준 `e486195c3e72f14383ff14806d4e28f306ed28ea`, 제품 구현 `/root/mixed_history_implementation`,
독립 fixture·시험 `/root/mixed_owner_test_preparation`, 통합·실행 `/root`,
독립 검토 `/root/dev3_independent_review`.
[소유 계약](../contracts/dev3-catalog-format.md#undo-before),
[중앙 시험 목록](../plans/dev3-test-plan.md#catalog-before-followup),
[source별 색인](dev3-catalog-before-20261024.json)을 따른다.

XCB1은 과거 Table/Index 정의·세대·schema·anchor를 빌리는 순수 표현이다. 기존 Undo root에
stamp/logical/두 previous role을 한 번만 보존하고 Catalog Missing과 CatalogValue를 함께 지원한다.
XCT1과 정의 길이·쓰기·읽기 코드를 공유하며 inline/chunk 복원 accessor는 Catalog와 Row를
명시적으로 구별한다. Releasing은 남아 있는 body를 live before로 노출하지 않는다.
값 검산이 실제 Catalog 변경·pin·할당·anchor 보존 권한을 발급하지는 않는다.

독립 fixture는 published offset에서 만든 새 raw68개/55,227 bytes와 기존 U04/U08 참조2개/905 bytes다.
[manifest](../../tests/fixtures/dev3-catalog-before/manifest.json)의 기대값은 제품 encoder의 출력으로
생성하지 않았다. 원 생성 시 metadata 조립 오류는 파일 쓰기 전 실패했으며 보존한다. 수정 후
독립 생성 시간은0.006904초다. 참조2개를 추적 경로에 복사한 metadata 후속 변경은 raw68 재생성이
아니다. 최초 generator/manifest/provenance와 당시 hash를 별도 보존했다.

| source | 실제 결과 |
| --- | --- |
| r0 `91593c58…`, ELF `0906299f…` | compile12.581655초·소유5파일 fmt PASS. Clippy의 새 테스트 표현2곳 FAIL. 제품 case 실행0 |
| r1 `1ca8f72d…`, ELF `b9c1e067…` | 두 lint 표현만 수정. compile10.796908초·수정1파일 fmt·engine all-target/all-feature Clippy10.284811초 PASS. ST01 첫 실행 뒤 의미122 첫 실행 PASS |

필수123은 static1+semantic122, 총10함수다. 의미 검증은 독립 literal15, target/stamp12,
shape/header38, chunk9, release/accessor17, loaded version6, encoder 오류18,
변경 legacy Catalog5, XCT1 공유 정의 연결2다. 기존 Row/Index 전체 및 하위 wrapper를 반복하지 않았다.
S5 추가9,534case/3함수는 구현·ignored/NOT_RUN이며 전체 inventory는9,657case/13함수다.

ST01 nominal caller는21,920≤24,576 bytes,10항 보수적 합계는62,239≤65,536 bytes다.
5개 backing16,428 bytes와 실제 schema carrier/이름, codec·Undo·hash의 인자/반환 값을 함께
계상한다. arbitrary String/Vec capacity·실제 mutable owner F05·ABI stack/RSS의 상한은 아니다.
원 프로세스0.007818초·exit0·BEGIN/PASS를 보존한다. 최초 수집기가 libtest의 exact 테스트 이름
접두어를 놓쳐 원 receipt는 `PARTIAL_OR_FAIL`로 남겼다. 원 stdout만 재판독하여 ST01을 수용했고
제품은 재실행하지 않았다. 수집기의 접두어·순서·완료 ID 제외·총 시간 검산을 보완하고 도구 전용
15개 selfcheck를 수행했다. 이15개를 제품123에 더하지 않는다.

의미122는 같은 r1 source/ELF에서9개 exact selector로 한 번씩 완료했다. process 시간 합0.343716초,
수집·출처 확인을 포함한 wall2.695538초다. 원 static 시간을 합쳐10초 한도 이내다. case별
BEGIN→PASS와 prior 완료 ID 재선택 차단을 기록했다. 로그/receipt 보수 계상49,826 bytes는8MiB
한도 안이다. stdout/stderr·원 실패·선택·source/ELF·검토 원본은
`artifacts/dev3/first-row-transaction-20261024/catalog-before`에 보존한다.

이 단위는 순수 private borrowed codec이므로 별도 native/compiler campaign을 만들지 않았다.
private field·입력 lifetime은 독립 source 검토 대상이다. 실제 첫 Catalog 생성·queue/terminal·
row ACK·새 process 복구/재진입·전체 DEV3/S5는 미완료이며 AI 완료 소비자는1/3을 유지한다.
제품 변경 없는 시험 반복·문서 verifier·push는 수행하지 않았다.

최종 독립 D1은 소유91경로를 전수 검토하여 제한 수용했다. OCR preview109 중 동시 DEV4
18경로는 명시 제외했고, 제품·시험·fixture78경로는 r1과 byte 일치했다. 원 실행·입력·자원과
권한 경계를 대조했으며 미해결 제품 지적은 없다. 판정 원본은
`artifacts/dev3/first-row-transaction-20261024/catalog-before/review/d1-final.md`에 보존한다.
source+ELF·검토/도구·별도 build cache의 논리 보관량 snapshot은 JSON에 기록한다.

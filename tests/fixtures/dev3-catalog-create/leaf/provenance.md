# CREATE scope2 leaf 독립 입력 원천

2026-10-25 · 작성 `/root/dev3_review`. 입력 생성만 허용됐고 제품 encoder/decoder, Cargo, 제품/compiler/tool 시험 runner는 실행하지 않는다.

`generate_literals.py`는 정본 offset 표와 별도 명시 opcode/size 표를 Python 표준 struct·hashlib로 직렬화한다. 제품 소스의 API shape를 읽었으나 제품 helper, 출력 또는 fixture generator를 import·호출하지 않는다. 계약 원문 SHA는 input-plan에 묶는다. 채택된 C254/C4600과 frame 가격은 독립 표에서 상수로 대조하고 제품 frame_bytes 결과를 가져오지 않는다.

Projection raw는 XCP1의 정확 digest 원문이며, `*-value`는 함수의 XCT1 인자, `*-cell`은 key wrapper12를 포함한 tree cell이다. P01/P06 Missing은 value None이다. 최대 schema는 서로 다른 32개 열 이름128bytes를 사용한다. expected Namespace/Object를 input bytes에서 추출하지 않는다.

Core 정상은 표현 수준으로만 받는다. q/sequence MAX의 표현 가능성은 실제 issuer의 successor 가능성을 뜻하지 않는다. NC09의 typed slot1은 core가 읽을 수 있지만 first CREATE의 실제 slot0 source에 맞지 않으므로 stable semantic admission에서 거절해야 한다.

Wire 정상은 **shape-only**다. descriptor1536은 0xD1/0xD2 sentinel이고 대부분 native cell body도 눈에 띄는 sentinel로 구성한다. 전체 native span·digest·page envelope·INIT 두 개·정렬 target·section 길이는 독립 구성하지만 actual Root/Job/Status/Undo/allocator state·descriptor/credit 인계를 증명하지 않는다. 이 입력에서 semantic grant, durable Commit, replay approval, producer PASS를 발급하면 잘못이다. canonical Catalog/Anchor/Row metadata를 포함하더라도 다른 semantic sentinel 때문에 전체 frame은 실행 이력이 아니다.

NW는 exact byte patch와 재봉인된 mutant 전체 SHA를 동결한다. 원 full frame을 여러 번 저장하지 않아 disk를 절약한다. NS01..03은 Aux의 owner ordinal/edge role/count multiset 의미 반례로서 현재 shape-only LayoutView 통과 여부와 실제 semantic 거절 의무를 구분한다. 앞으로 parser의 proof 범위가 명시 확장되면 선택한 검증 layer를 재동결한다.

생성은 기존 raw/manifest가 있으면 덮어쓰지 않고 종료한다. 최초 생성 stdout와 manifest/index SHA를 보존한다. 이 절의 입력 준비는 제품 시험 PASS가 아니다. root의 coherent source와 exact selector/resource gate가 닫힌 뒤 실행·결과는 별도 기록한다.

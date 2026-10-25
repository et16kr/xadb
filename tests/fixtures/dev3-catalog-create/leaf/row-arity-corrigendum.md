# RowRoot metadata arity 오기 정정의 입력 대응

2026-10-25 · `/root/dev3_review` · 독립 raw 읽기 관측. 제품 encoder/decoder, compiler, 시험 runner 실행0. 기존 raw 재생성0.

정본 `devdoc/contracts/dev3-create-group-format.md` §6의 RowRoot metadata arity1은 최소 TableSpec 열1개와 혼동한 오기였다. 기존 `tree_page::validate_schema`는 Index만 arity1..8을 허용하고 Row/Catalog는 arity0을 요구한다. commit `335bc8c0af6e25d2410e1d09839701f77b791ba2`는 표를 arity0으로 고치고 정정 이유를 덧붙였다. Row key 의미나 wire 구조를 확장하지 않는다.

- 기존 input-plan이 고정한 group 계약 SHA256: `30a9a1c1cba52e91df4497e4aca1619781b241b3bcdd387916a0c520a07cbd8a`.
- 정정 뒤 group 계약 SHA256: `dfa039a02ada15155b01a58439a6e19d60964f9b43867be5a8b7f9b9f4a3e414`.
- 기존 manifest SHA256: `84b8aa4c0607d314dc373a956e6d75c7dcdca701209ef18ab0470c23826a6d5d`.
- 기존 index SHA256: `728977808b9c1abd66939bce4989c487d7c3ebc6c71182b97222419dd113b35c`.

동결된 generator는 이미 RowRoot32를 `XBT1`, version1, metadata arity0으로 작성했다. 원 manifest의 native section/span을 따라 raw를 읽은 독립 관측은 다음과 같다. offset은 각 W frame의 처음을0으로 센다.

| Case | 원 raw SHA256 | Row after 시작 | arity byte offset | 관측값 |
| --- | --- | ---: | ---: | ---: |
| W02 | `a1da5980acd3de81b119f934a5abf5283151d9f81f4114fd461be5e665b0209b` | 21870 | 21876 | 0 |
| W03 | `6057658543a7c883d7b9cdf02e06c7bdce9137db5263a8b42220f4a9a34fdee3` | 26216 | 26222 | 0 |

두 after32 원문은 동일한 `5842543101000000000000000000000000000000000000000000000000000000`이다. 따라서 이 정정 때문에 frozen input-plan/manifest/index/generator/raw를 변경하지 않는다. 이 문서는 오기와 이미 동결된 입력의 관계만 추가 기록하며 제품 decode 또는 semantic consumer PASS를 주장하지 않는다. r1 NW19 입력도 영향이 없다.

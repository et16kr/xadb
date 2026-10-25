# XADB 전체 수명 입력 조립의 독립 입력

2026-10-25 · **D4-LM-ASM-001 선택 입력 / 원 후보 byte 동일 보존**.

[소유 계약](../../../devdoc/contracts/dev4-lifecycle-assembler.md)의 `assemble_input` 시험 입력이다.
[selection inventory](selection-inventory.json)가 그룹·개수·hash·시험 함수를 고정하며 시험 코드도 같은
hash를 확인한다. [준비 manifest](manifest.json)는 2026-10-25 준비 단위의 원 기록이며 수정하지 않는다.

| 그룹 | 파일 | 개수 | 출처와 상태 |
| --- | --- | ---: | --- |
| initial | `normal/initial.json` | 1533 | 준비 단계 정적 검토 |
| original | `normal/original-links.json` | 27 | 원51 occurrence·30쌍 연결, 초기 중복3은 추가 호출 아님 |
| bind | `normal/rich-bind.json` | 22 | 준비 단계 정적 검토 |
| boundary | `boundaries/cases.json` | 76 | 성공34·오류42, 준비 단계 정적 검토 |
| rich_normal/continuation/rest | `rich/*.json` | 97/77/130 | 준비 단계 미검토 후보를 byte 동일 반입 |
| supplement | `supplement/cases.json` | 17 | 이번 독립 literal 작성자가 손으로 작성한 공백 보완 |
| supplement2 | `supplement/cases-2.json` | 9 | 계약 D2 검토가 요청한 owner 결정·A2 witness |
| supplement3 | `supplement/cases-3.json` | 6 | 구현 D1 검토가 요청한 RW 거래 Resume witness |

`rich/correction.json`은 rich 후보 두 건의 기대값 정정이다. 원 기대값의 논리 SHA를 먼저 확인하고
해당 기대값만 대체하며 원 파일은 그대로다. 근거는 wire-v2의 Resume work 순서이고 독립 검토가
정정을 판정했다. rich304는 개별 정적 검토를 모두 마친 자료가 아니며, 같은 계약에서 독립 작성한
구현과 대조해 불일치 두 건을 정정한 범위로만 기록한다.

`provenance/`의 Python은 원 artifact 배치에서 후보를 작성한 보존 자료이며 이 위치의 실행 진입점이
아니다. 제품 assembler·model·codec의 출력을 기대값으로 채택하지 않는다. 실행 결과와 실패·정정 이력은
[선택 증거](../../../devdoc/evidence/dev4-lifecycle-assembler-20261025.md)에 둔다.

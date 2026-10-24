# XADB 전체 수명 입력 조립의 독립 준비 자료

2026-10-25 · **정적 검토 후보1658 / 전체 D2·구현·시험 NOT_RUN**.

[manifest](manifest.json)는 실제 원 artifact의 path·bytes·SHA와 byte-identical 보존을 기록한다.
초기 raw1533, 원 입력의 중복 제외27, Bind22, 오류·상한76(성공 기대34/오류 기대42)을 포함한다.
원51 trace 위치 및 초기 중복3은 연결 정보이며 추가 호출 수가 아니다. 새 API를 실행한 결과는0이다.

`normal/`과 `boundaries/`는 독립 literal 후보다. boundary는 필요한 read-set의 구조를 갖춘
합성 workspace이며 전체 State 불변식이나 모델 도달성을 인증하지 않는다. rich 상태 추가304개는
미검토·NOT_FROZEN으로 원 로컬 artifact에 남아 있으며 이 디렉터리에 포함하지 않았다.

`provenance/`의 source manifest·독립 검토 JSON·Python 작성 원본은 당시 identity를 보존한다.
Python은 원 artifact 디렉터리 배치를 가정하는 보존 자료이며 이 복사 위치의 실행 진입점이 아니다.
제품 assembler/model/codec의 출력을 기대값으로 채택하지 않았고 기존 완료 시험도 반복하지 않았다.

전체 입력 inventory·추가 참조/receipt 경계·판정기 known-bad와 최종 D2가 남아 있다.
이 준비 자료를 제품 구현 GO·시험 PASS·P5/FE14 완료로 사용하지 않는다.
검토 범위와 다음 단계는 [준비 기록](../../../devdoc/evidence/dev4-lifecycle-assembly-inputs-20261025.md)을 따른다.

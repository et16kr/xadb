# DEV3 혼합 Status·Free Undo INIT 준비 — 제한 선행 인수

2026-10-24 · **독립 D2 수용 / source별 선택 검증 완료 / 최종 독립 D1 제한 수용(실행 범위 일탈 기록 유지)**.
작업 기준 `b5bdd99`; 실제 통합 부모는 중간 evidence README 링크 보완만 포함한 `2f262d2`이며
그 변경을 보존한다. 구현 `/root`, 독립 검토 `/root/dev3_independent_review`.
[소유 계약](../contracts/dev3-status-init.md#mixed-system-init),
[중앙 시험 목록](../plans/dev3-test-plan.md#mixed-system-init-followup),
[source별 색인](dev3-mixed-system-init-20261024.json)을 따른다.

plan3 UndoRole을 빌리는 sealed producer/observer/materializer와 trusted mixed reserve/cancel
population을 연결한다. 기존 Status entry point의 고정 의미와 byte 표현은 유지한다.
Undo terminal38,208·Status38,000·reserve/cancel21,200 및 전체63entry 검산을 공유한다.
완료 종류에 맞는 실제 멤버가 필요하며 순수 byte 관측으로 live 권한을 만들지 않는다.
실제 mixed mutable owner·같은 pool INIT/dirty/pressure/flush/load·별도 process 복구는 후속이다.

독립 raw 입력은 profile3×Status A/Undo B/C와 후속 Cancel이며285파일4,407,360바이트다.
manifest/provenance를 포함하면287파일이다. 기존 Status 비교 입력은 별도 provenance로 보존한다.
[raw manifest](../../tests/fixtures/dev3-mixed-system-init/manifest.json)와
[compiler manifest](../../crates/engine/tests/fixtures/dev3-mixed-system-init/compiler/manifest.json)을 따른다.
입력 생성은 제품 실행 증거가 아니다.

| source | 실제 선택 결과 |
| --- | --- |
| r0 `c6b47fe6…` | 기존 compiler-only 테스트 보조의 exhaustive arm4 누락 E0004, 원 compile FAIL 보존 |
| r1 `43f6c859…` | compile PASS·unused import 경고, 제품 실행 없음 |
| r2 `9f6099b2…` | 신규 시험의 Cause variant/비교 E0599/E0369, 원 compile FAIL 보존 |
| r3 `6027514f…` / r4 `f9918e71…` | compile PASS, 선택 제품 실행 없음; 수정 중 source를 실행 PASS로 세지 않음 |
| r5 `3a348677…`, ELF `43193943…` | compile14.836초·fmt0.878초·Clippy10.984초 PASS. MI01–05 runtime224와 MI06 static1 PASS. MI07은 완료 marker2 뒤 health-first effect 기대값 오류로 FAIL |
| r6 `86530ed5…`, ELF `2441031b…` | 제품 코드 동일, MI07 시험 기대값 수정·MI08 추가만 변경. compile14.265초·fmt0.899초·Clippy11.074초 PASS. MI07 runtime9·MI08 runtime12 및 새 checker6 PASS |
| compiler baseline `0f2eacaa…` | sibling compiler semantic6+실제 unmodified baseline1 PASS, 누적 compiler16.329초. r6와의 차이는 아래 생성 cache 한 파일 |

최종 고유 inventory는8함수246case(runtime245/static1)다. MI08은 독립 Free gen6를 native/body/
Native SHA/full INIT/page SHA/frame SHA에 일관되게 대체한 반례와 native kind4를 검증한다.
원 MI04를 반복하지 않고 별도12case로 추가했다. S5 확대 총342,960(필수 MI04 입력141 alias 포함, 고유 추가342,819)은 ignored/NOT_RUN이다.

**실행 범위 일탈:** r5 MI07에서 완료된 prefix/fence2case가 실패 함수 전체의 r6 재실행에 다시
포함됐다. 원 함수 FAIL·완료2marker·r6 PASS9를 그대로 보존하고 중복2에는 추가 완료 credit을
주지 않는다. 검토자의 초기 함수 단위 선택 승인과 뒤늦은 scenario 제외 정정도 원 기록에 남긴다.
이 실행을 정책 준수 사례로 소급 표시하지 않는다. 후속 실패 재선택은 함수 내부의 완료 scenario도
제외한다. MI01–06과 기존 전체 Status/Undo·하위 wrapper는 재실행하지 않았다.

**source 보관 구분:** checker의 Python import가 r6 실행 디렉터리에
`scripts/__pycache__/check_dev3_mixed_init_borrows.cpython-314.pyc` 21,158바이트를 만들었다.
compiler는 이 cache가 포함된 실제 baseline hash를 기록했다. 제품·스크립트 원문은 동일하다.
원 보관본을 변경하지 않고 이 신규 cache 하나만 제외한 `r6/source-at-build`를 별도로 복원했으며
원 build hash `86530ed5…`와 정확히 일치한다. 실행 디렉터리가 계속 같은 fingerprint였다고
주장하지 않는다. 후속 증거 도구는 interpreter 시작 전에 `PYTHONDONTWRITEBYTECODE=1`을 고정한다.

MI06의 logical source-value는 plans3,344·temporaries14,560·mixed codec6,168≤기존7,336이다.
caller98,304를 별도로 계산했고 직접 영향받은 원 owner의 memory/native type admission을 통과했다.
이는 physical stack/RSS나 새 mixed mutable owner의 F05가 아니다. stage 거절 시험도 실제 INIT
append/설치·native 복구의 증거가 아니다. AI 소비자는1/3이며 전체 DEV3/S5는 미완료다.

원 artifact/source/ELF/명령/stdout/stderr/시간·검토는
`artifacts/dev3/mixed-system-init-20261024`에 보존한다. JSON의 보관량은 logical bytes snapshot이며
별도 shared build target은 포함하지 않는다. 문서 verifier는 pre-push 전용으로 실행하지 않았다.

최종 독립 D1은 OCR 선택15·제외301을 포함한316/316파일을 검토해 누락0으로 제한 수용했다.
제품·증거 무결성의 미해결 blocker는 없으며, 중복2case의 범위 일탈은 수용으로 소급 해소되지
않는다. 판정 원본은 `artifacts/dev3/mixed-system-init-20261024/review/d1-final.md`다.

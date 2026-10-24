# DEV3 Free Undo 초기 page — 제한 선행 인수

2026-10-24 · **독립 D2 수용 / r0 compile·fmt·Clippy·선택7함수 PASS / 최종 독립 D1 제한 수용**.
기준 `cd8b876`, 구현 `/root`, 독립 검토 `/root/dev3_independent_review`.
[소유 계약](../contracts/dev3-undo-format.md#free-initial-page),
[중앙 시험 목록](../plans/dev3-test-plan.md#undo-free-page-followup),
[source별 색인](dev3-undo-free-page-20261024.json)을 따른다.

target 문맥 없는 canonical Free root codec과 이를 소비하는 초기 one-slot Undo page 준비/관측을
같은 기존 모듈에 연결했다. Free gen1/6/MAX, exact432, 전체 page checksum/기대 identity/slot과
출력 불변을 검산한다. generic record의 공통 header를 공유하지만 expectation-first 순서와 점유
body 의미는 유지한다. 실제 UndoRole/allocator/INIT·같은 pool·flush/load·복구는 후속 작업이다.

독립 검토자는 published offset에서 raw Free3+page1을 만들고 기존 독립 U02/U13을 byte-exact로
보존했다. raw6은18,544바이트다. 제품 encoder/output으로 기대값을 만들지 않았다.
[manifest](../../tests/fixtures/dev3-undo-free-page/manifest.json)의 계약 hash는 base 당시 provenance이며
뒤에 append된 현재 문서의 hash와 혼동하지 않는다.

| source | 실제 검증 |
| --- | --- |
| r0 `f2243172…`, ELF `d64ca4b7…` | compile13.487097초·fmt0.862538초·engine all-target/all-feature Clippy10.332625초 PASS; 독립 선택 검토 뒤7함수987case PASS |

선택 입력은 source freeze 전에7함수987case로 고정했다: runtime986·static1이다. 이 중43case는
공통 header 이동·기존 Free 위임에 직접 해당한다. 기존12 full Undo suite와 무변경 하위 wrapper는
반복하지 않는다. 추가 S5 49,494case는 ignored/NOT_RUN이다. 수량은 정상 설치 건수가 아니다.

이번 변경은 pure codec이므로 새 native disk/복구 경로가 없으며 복구 PASS로 세지 않는다. sealed
capability를 추가하지 않아 별도 lifetime/compiler probe도 해당하지 않는다. 이후 실제 mixed owner의
source-value F05·첫 효과·현재-build recovery/reentry를 별도로 닫아야 한다. AI 소비자는1/3,
효율 비교·live undo·전체 DEV3/S5는 미완료다.

원 source·binary·명령·stdout/stderr·시간·선택 입력·독립 검토는
`artifacts/dev3/undo-free-page-20261024`에 보존한다. 보관량은 JSON의 논리 bytes snapshot이며 물리
할당량이나 RSS가 아니다. shared build cache는 원 empty-transaction-recovery 디렉터리에 별도
보존하고 중복 합산하지 않는다. 기존 완료 시험 재실행·문서 verifier 실행은 없다.

실제 선택 runtime986·static1을 한 번씩 완료했다. 원 process 합 시간은0.023120초다.
UF07 source-value는3,216≤4,096바이트이며 caller page16,384·retained0·heap0을 별도 표기한다.
이는 source 값의 보수적 계산이며 실제 stack/RSS나 이후 mixed owner의 F05 완료가 아니다.
첫 source에서 compile/정적/선택 검증이 통과했고 이번 단위에 원 FAIL은 없다. 최종 독립 D1을 제한 단위로 수용했다.

최종 독립 D1은 OCR 선택8·제외16을 모두 직접 검토하여24/24·누락0으로 제한 수용했다.
원 source/ELF/raw/영수증·시간·보관량을 대조했고 추가 제품 실행은 없었다. 판정 원본은
`artifacts/dev3/undo-free-page-20261024/review/d1-final.md`에 보존한다.

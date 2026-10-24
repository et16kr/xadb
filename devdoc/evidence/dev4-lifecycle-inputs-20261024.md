# DEV4 P5 수명 모델의 독립 입력 준비

2026-10-24 · **정적 literal·byte 입력 한정 검토 수용 / 전체 D2·모델 구현·시험 NOT_RUN**.

[선행 계획](../plans/dev4-preparation-plan.md#sequence)의 P5, 4.16.1.2·4.16.3.2/FE14를 위한
독립 입력을 먼저 보존한다. 제품·모델·codec·checker·explorer를 구현하거나 실행한 기록이
아니다. 계약·통합 담당은 `/root`, 입력 작성은 `/root/frontend_oracle`, wire 설계는
`/root/frontend_research`, 독립 검토는 `/root/frontend_review`다. 전체 F02/F04의 checkpoint
입력·최종 manifest·독립 D2는 후속이며 이 기록으로 구현 GO나 P5 완료를 선언하지 않는다.

## 보존한 실제 입력

[fixture manifest](../../tests/fixtures/dev4-lifecycle-model/manifest.json)는 실제47개 파일 중
자신을 제외한46개 파일의 bytes·SHA-256과 최초 작성 경로를 기록한다. 입력은 원 artifact와
byte-identical이며 모델 출력에서 expected를 생성하지 않았다.

- [독립 논리 literal](../../tests/fixtures/dev4-lifecycle-model/literals.json): 필수 mutant5개와
  직접 대조7개, 총12 trace·51 action occurrences,29 named edge·29 state. 초기 상태와 손으로
  지정한 lossless delta로 모든 중간 상태·입력·응답을 닫았다.
- [wire 후보](../../tests/fixtures/dev4-lifecycle-model/wire-schema.json): XADBLM02 state version2,
  resolved input version1/Env1의 필드·tag·상한. 최종 채택 상태는 여전히 PROPOSED다.
- 상태 byte4개: initial444, parked946, stale-result 거절1313, rollback incomplete1058bytes.
  입력 byte7개: SELECT prepare/execute, terminal cancel, pending resume, incomplete rollback,
  retired result와 unknown-kind child를 각각32/214/214/198/371/81/81bytes로 기록했다.
  각 vector에는 `.bin`, `.hex`, 완전 logical JSON, field offset JSON이 있다.

state header의 layout/profile/action/exclusion hash는 각각00/11/22/33을32회 반복한 **시험 표식**이다.
실제 실행 profile이나 source hash가 아니다. trailer SHA와 전체 파일 SHA도 구별한다.
원29상태 전체·1533 raw action·깊이6 탐색을11개 대표 vector로 대체하지 않는다.

## 독립 작성·검토와 실행 구분

원본은 `artifacts/dev4-lifecycle-model-20261024/`에 보존했다. 독립 packing recipe는 동결된
logical literal을 펼쳐 지정 wire 필드로 packing했으며 model transition이나 expected-state
계산을 포함하지 않는다. 정적 authoring1회는 exit0·stderr0이고 원 stdout/stderr를 보존했다.
이는 fixture 생성이며 제품·Rust codec·모델 시험의 PASS가 아니다.

| 보존 identity | SHA-256 |
| --- | --- |
| `literal-design-v1.json` | `ec4a7940a457882c155eb84e2d9e5602eaf880563bab4f6f04d0b643bcec42d2` |
| `wire-proposal-v2.json` | `2edefc28473581f8e263c2c74db90a247c0973811787aed73bd26f7679ced1ec` |
| `independent-vectors-v1/manifest.json` | `e58a94580d9f1edcababa0a36458582c70b798a4346a6d36f309391deec25cad` |
| `independent-vectors-v1/packing-recipe.py` | `87aa123a7cd6130fe1a709969e31afed7d7be4d3e776ec5d1077889d6ef95c7d` |
| `review-wire-literal-v1.json` | `faaa60974a90ba25592637903cdb3db2b0d68953ac7596295bf51b4cb31f78fa` |
| `review-independent-vectors-v1.json` | `6236abefc1198fe272cc7ce5ae283a1bf633af04355eab2e1a5f045ac631fc46` |

검토자는 원 recipe를 재실행하지 않고 원44개 vector 파일과 source4개 hash, hex/bin 일치,
state trailer4개,11개 literal 전개 및 필드 배치를 읽기 전용으로 대조했다. 그 범위에서
불일치0이며 **정적 입력 한정 수용**이다. 정본의 부분 검토와 전체 D2 수용을 혼동하지 않는다.
입력 Env1의 full Identity16·ResourceKind와 state의 짧은 Ref를 구별한다. state membership은
obligation 고정12bytes, owner는 Option에 따라11..19bytes다.

## 로컬 DBMS 대조

`/home/et16/work/postgres` HEAD `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의
`src/backend/utils/mmgr/portalmem.c`469–570·782–858을 읽어 portal 정리와 거래 자원 정리를
구별했다. 전체 파일 SHA는 `6f28d527b46ff1f44645120af99ee564a59204a73575c492511441d0359eff48`다.

SQLite HEAD `43a48f77585a88940c727f33682151418f544059`의 `src/vdbeapi.c`120–174와
`src/vdbe.c`9510–9605·interrupt 검색 지점을 읽어 reset/binding과 취소/halt 경계를 비교했다.
전체 SHA는 각각 `144068f8686000e049168e665773bb5e6769e3ecc5a7a6e455e8418d53db91a5`,
`0db58f144da7de9508a8285a5fee9a458d5ff499286a77b784f2c7c38c457d6d`다. 두 checkout의
tracked clean 상태를 확인했고 원본 수정·빌드·시험은 하지 않았다. XADB의 호출별 context와
fence/복구 의무는 자체 [SQL 계약](../contracts/dev4-sql-design.md)을 따르며 외부 DBMS와
같은 의미나 구현이라고 주장하지 않는다.

## 남은 작업과 인수 경계

최종 checkpoint byte·lease/audit·저장 회계와 전체 D2 뒤 순수 모델/독립 checker/codec 및
controller의 BFS·파일 재개를 구현한다. 실제 새 코드가 생긴 뒤 해당 diff의 필수 시험을
선택한다. 현재 제품·시험 코드 변경0이며 compile/runtime/test0, 문서 verifier0이다.
이미 완료한 P1–P4·DEV3·COMM 시험을 반복하지 않았다.

전체 공간은 두 session·각 prepared2·active result1·길이0..6과 raw1533을 유지한다.
보호 cap·저장 부족·중단·미도달은 INCONCLUSIVE다. 현재 여유 공간은 약1.7GB로 전체 탐색에
요구하는10GiB 영속 보존 사전조건을 충족하지 못한다. 작은 입력 보존을 전체 탐색 가능·완주로
표시하지 않는다. 실제 SQL/engine 연결·DEV4/Q1은 계속 미완료다.

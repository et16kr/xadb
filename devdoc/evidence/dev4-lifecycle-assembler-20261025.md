# DEV4 P5 전체 입력 조립의 선택 검증

2026-10-25 · **신규1994입력 선택 PASS / 독립 검토 D2·D1 수용 / 모델·FE14 미완료**.

[소유 계약](../contracts/dev4-lifecycle-assembler.md) D4-LM-ASM-001의 `assemble_input`만 구현했다.
기준은 main `688b9af`에서 만든 별도 worktree `/home/et16/work/xadb-dev4`(branch `dev4-independent`)이며
같은 시각 다른 checkout에서 진행 중인 DEV3 작업은 읽거나 바꾸지 않았다. 준비 원자료는 그 checkout의
gitignored `artifacts/dev4-lifecycle-model-20261024/`에서 읽기만 했다.

## 역할과 독립성

| 역할 | 담당 | 산출물 |
| --- | --- | --- |
| 계약·통합·구현 | 이번 작업 세션 | 계약, `input_assembler.rs`·`rows.rs`, harness |
| 독립 literal 작성 | 구현을 읽지 않은 별도 작성자 | `supplement/cases.json` 17, `cases-2.json` 9, `cases-3.json` 6 |
| 독립 검토 | 별도 검토자 | 계약 D2 2회, 최종 D2·구현 D1·rich 표본 |

준비 단계의 후보 1658·rich304는 이전 작성자의 손 literal이다. 기대값은 어느 단계에서도 제품·모델·codec
출력으로 만들지 않았다. 검토 원문은 로컬 `artifacts/dev4-lifecycle-assembler-20261025/`에 있다.

| 검토 원문 | SHA-256 |
| --- | --- |
| `review-contract-d2-v1.json` (계약 D2 1·2차) | `8c43b5748eb37982bd64e2323a335d993abb22e844b90a5813cf37a92d3f2345` |
| `review-final-d2-d1-v1.json` (최종 D2·D1·rich 표본) | `9121357519aecefcf953204c95d98a198b0d9a312101ab5f6b52b94e85b0394a` |
| `review-closure-v1.json` (보완 확인, D2·D1 ACCEPT) | `73c24cd90d473ceafbd6e8ea12e39a83d62d408e1f63b6e866760a9dd64a4a17` |

## 입력

| 그룹 | 개수 | 출처 |
| --- | ---: | --- |
| initial | 1533 | 준비 단계 정적 검토 |
| original | 27 | 원51 occurrence가 연결하는 30쌍 중 초기 중복3 제외 |
| bind | 22 | 준비 단계 정적 검토 |
| boundary | 76 | 성공34·오류42 |
| rich (97/77/130) | 304 | 준비 단계 미검토 후보, byte 동일 반입 |
| supplement / 2 / 3 | 17 / 9 / 6 | 이번 독립 작성 |
| 합계 | 1994 | 실제 `assemble_input` 호출도1994 |

[selection inventory](../../tests/fixtures/dev4-lifecycle-assembly/selection-inventory.json)가 그룹별
파일 hash와 시험 함수를 고정한다. pre-state literal `literals.json` SHA는
`ec4a7940a457882c155eb84e2d9e5602eaf880563bab4f6f04d0b643bcec42d2`다.

## 개발 중 불일치와 교정

다음은 최종 실행 이전의 개발 실행이며 원 결과를 그대로 남긴다.

1. 첫 구현의 Resume work 순서를 미검토 rich 후보에서 추론해 1962입력이 모두 일치했다. 정본 대조에서
   wire-v2가 Rows0·Eof1·ReadError2·Conflict3·RowBufferTooSmall4로 정한 것을 확인하고 구현을
   고쳤다. 그 결과 rich 후보 `asm/resume-unsignalled/1488`·`/1492`가 불일치했다. 원 파일은 그대로 두고
   `rich/correction.json`(SHA `f88b6517d9ac1a007da30e2c94ec1f86d631bd04e37e548eedc18653a00e2640`)에
   원 기대 hash와 근거를 기록했다. 독립 검토가 두 정정이 맞다고 판정했다.
2. 첫 구현은 census와 새 O·Recovery 번호를 transaction nonce 순서로 처리했다. 독립 작성자는
   identity-transition의 "exact identity 숫자순"을 근거로 full identity key 순서를 택했고, 계약 D2 1차도 같은
   이유로 nonce 순서를 차단 지적했다. 구현을 key 순서로 바꾼 뒤 두 actor census 사례가 일치했다.
3. Finish NCR의 Fenced 판정을 admission≠Admitting에서 admission=Fenced로 고쳤다(wire-v2 문구).
   CloseSession Complete의 RecoveryConfirmedRetired 처리는 원문이 정하지 않아 owner 결정으로 고정했다.
4. 구현 D1이 RW 거래 Resume witness 부재를 지적했다. 세 번째 supplement 작성자는 무신호 Rows/
   RowBufferTooSmall+Incomplete가 proposal §7의 명시 행대로 owner empty여야 한다고 보고했다. 구현은 그때
   census를 공급하고 있었으므로 실행 전에 원문대로 고쳤다. 무신호 Eof+Incomplete는 원문이 정하지 않아
   census를 owner 결정으로 고정했다.

한 번의 개발 단계에서 product 변조11종을 넣어 harness가 검출하는지 확인했다. 초기 8종 중
recovery DuplicateFirst 위치와 retired statement 인증 2종은 당시 입력으로 검출되지 않았고, 독립
supplement 추가 뒤 이 2종과 census nonce 순서 변조까지 모두 검출됐다. 변조 실행은 product 변경을
되돌린 상태로만 끝냈으며 PASS 분모에 넣지 않는다.

## 최종 선택 실행

`run-assembler.py final-r2`는 source·fixture manifest를 보존한 뒤
`cargo test -p xadbtest-reference --lib --no-run --offline --locked`로 한 번 compile하고 보존 ELF에서 새 시험
함수만 `--exact --nocapture --test-threads=1`로 실행했다. 앞선 `final-r1`은 runner의 ELF 상대 경로
오류로 시험 호출 전에 끝났으며 시험 실행0이다.

| 항목 | 값 |
| --- | --- |
| 기준 HEAD | `688b9afce67e5b327d2a3c165b5ef5c0c556435c` + 이 단위의 미commit 변경 |
| source·fixture manifest SHA | `76f10e89474b113db60164a1d230b2469b4d1bca7db0dd36875f6d06e6af86d0` |
| 보존 ELF SHA | `8618526d23e87719957206ce632651a87328fd8020513f345dbf08469284642c` |
| receipt SHA | `6e748016f7d393b1a266a662adfd51a792882f40e648a09b1e8d5efecc07407e` |
| 실행 전·후 source | 동일 |

| 시험 함수 | START/COMPLETED | 결과 |
| --- | ---: | --- |
| `assembly_initial_raw` | 1533 | PASS |
| `assembly_original_pairs` | 27 | PASS, 51 occurrence 연결 확인 |
| `assembly_bind_payloads` | 22 | PASS |
| `assembly_boundaries` | 76 | PASS, 오류 기대42 포함 |
| `assembly_rich_normal`·`_continuation`·`_rest` | 97·77·130 | PASS, 정정2건 적용 |
| `assembly_supplement`·`_second`·`_third` | 17·9·6 | PASS |
| `assembly_correction_original_check` | 0 | PASS, 정정 hash 통제 |
| 합계 | 1994 | FAIL0 |

모든 `assemble_input` 첫 호출은 256KiB worker에서 실행했고 같은 사례의 반복 실행은 없다. 각 호출 뒤
State 동일성 비교는 `&State` 차용으로 이미 보장되는 성질의 방어 단언이며 별도 관측 증거로 세지 않는다.
정정의 원 hash는 serde_json 기본 key 정렬에 의존하며 순서가 바뀌면 조용히 통과하지 않고 실패한다.

## 정적 검사와 범위

`cargo fmt --all --check`와 `cargo clippy -p xadbtest-reference --all-targets --all-features --offline
--locked -- -D warnings`가 통과했다. 문서 검사는 push gate 전용이므로 실행하지 않았다. heap 0·recursion
없음·State clone 없음은 제품 경로의 정적 근거이며 allocator 계측과 release stack 실행은 하지 않았다.
기존 codec·raw/selector·P1–P4·DEV3·COMM 시험은 실행하지 않았다.

rich304는 개별 정적 검토를 모두 마친 자료가 아니다. 같은 계약에서 독립 작성한 구현과의 대조,
정정2건의 독립 판정과 독립 검토자의 표본57건(세 파일의 모든 pre-state 계열) 정적 확인로 범위를 한정한다.

## 남은 의무

무신호 Resume Eof+Incomplete의 census owner 결정은 RW 거래 사례가 없어 실패할 수 있는 시험이 아직 없다.
다음 입력 보완 때 RW parked fetch의 outcome3을 추가한다.

admission·환경 가능성·소비, full input alias, 독립 edge checker, 원51 action model replay, 전체
BFS·재개와 FE14는 후속이다. 전체 BFS의 10GiB 영속 공간 요구는 미충족이다. 실제 SQL/engine 연결과
DEV4/Q1은 DEV3 공급 뒤의 별도 의무다.

Verdict: ACCEPT

# DEV3 08-3 O2a module-gate record 독립 검토 3회차 (D2, mapping)

2026-11-07. 2회차와 같은 검토자다. 이번에는 2회차 R3·T1·T2를 반영한 변경만 본다. `/home/user/xadb`에서는 이 파일만 만들었다. scratch clone은 끝난 뒤 지웠다.

## 대상

- record `module-gate/records/20261107-dev3-08-3-o2a.json`(working tree, untracked)
  - blob `e752701c445015505d39bafcf5c0bf63bfa9b544`
  - sha256 `17b1993acc6023ff16bbb6bc65cbb290945d9270a243726ae3a7d278534336b5`
- 계약 `devdoc/contracts/dev3-row-overflow.md`
  - blob `3120e478a328c083eb8dd219e83b50004b7222d5`
  - sha256 `f366ff8fdca7775f14f831fc125a869493944ad2d6ab7af241e3c511dfbaabad`
- `devdoc/document-validation.json`: sha256 `e29aa505b6b74d65b053279d53bb17d0179bfaf12d8d3ed5a0295d7b7ccfc779`
- 2회차 `record-check-r2.md`: sha256 `245ad40dd179da43589cf137e6e0c682efae1899a222f1ed1fa7b5fe82dc4435`
- 코드: HEAD `d6d9802`. `git diff --quiet HEAD -- crates`가 깨끗하다.

## 검토 방법

- 현재 record를 1회차 판(sha256 `0fa8a6b1…4a64d`)과 필드 단위로 비교했다.
  - `[1]`, `[3]`, `[4]`, `[5]`의 reason, `[4]`의 사례, record check 항목의 `files`는 2회차에 본 그대로다.
  - 새로 바뀐 곳은 셋이다. `[0]`의 let-else 문장, `[2]`의 끝부분, D2 log의 실행 문장이다.
  - 경로, 사례, N/A, D2 `files`는 바뀌지 않았다.
- 계약은 HEAD와 word diff로 비교했다. 바뀐 것은 §15 "실행" 문단(465행)에 끼운 한 덩어리뿐이다.
- gate는 scratch clone에서 확인했다.
  - `git clone /home/user/xadb` 뒤 `d6d9802`에서 branch를 땄다.
  - 다음 여섯 파일을 넣고 commit했다: record, 계약, json, `record-check.md`, `record-check-r2.md`, 이 파일.
  - `python3 scripts/check_module_gate.py check --base e0723ae --head HEAD --mode merge` 결과: **PASS (0 error(s), 0 pending)**. 이 파일의 최종판으로 다시 돌려도 같다.
  - `plan`: 사례 72개(중복 없음), scoped 항목 83개. 2회차와 같다.
- `python3 devdoc/verify_documents.py`(clone)
  - 결과: PASS, markdown 289개, 오류 0
  - 넣은 json과는 `date` 한 줄만 다르다.
  - json의 이 계약 항목(links 52, sha256 `f366ff8f…`)은 위 계약 파일과 같다.
- 코드와 선택된 사례는 바뀌지 않았다. 그래서 시험은 다시 돌리지 않았다. 2회차 실행은 72 passed, 19.05 s다.

## 판단

- **R3: 닫혔다.**
  - `[2]`에서 "Two are implied …"부터 끝까지가 2회차에 제안한 문장과 같다.
  - §15의 "Prune 뒤 `@285 = 1`은 …"부터 "선택을 넓혀도 닿지 않는다."까지도 제안 문장과 같다.
  - 세 점이 모두 코드와 맞는다.
    1. Prune: codec의 `@285` 규칙(`job.rs:480–491`)이 막는다. 그 규칙은 OVF-F02가 확인한다.
    2. ReleaseRole: Job codec과 `unchanged`(`jobs.rs:264`)가 함께 함의한다.
    3. 범위: `jobs::observe`를 부르는 시험으로 한정했다.
  - "outside Reserved/Retained/ReleasingCompensated"는 줄인 표현이다. 실제 규칙은 그 세 상태 안에서도 INSERT Row root만 허용한다. 그래도 문장이 말하는 거절 방향은 참이다.
- **T1: 반영됐다.**
  - `[2]`의 괄호는 "(deleted, the 71 cases selected before MCA00-V06 still pass; MCA00-V06 runs only inline Forward/CLR …)"다. 1회차 측정, 2회차 gdb와 맞다.
  - D2 log는 "72 selected cases … 72 passed, 0 failed, 0 ignored, 19.05 s (--test-threads=1, record check round 2)"다. 2회차 실행과 같다.
  - §15의 괄호 "(record check가 지워 보거나(anchor·Prune은 함께) gdb로 0회임을 확인했다)"는 실제 측정 방식과 맞다.
- **T2: 반영됐다.**
  - `[0]`은 "…byte for byte. The two let-else refusals on the change (…)"로 바뀌었다.
  - 이유도 둘로 나뉘었다. `place_chunk`는 op가 change의 operation일 때만 닿는다(prepare.rs `:69`, `:2442`). `forward_chain`은 ForwardOverflow 짝 검사(`:1869–1873`) 뒤에만 닿는다.
- 그 밖에 새로 틀린 곳은 없다.

## 필수 지적

없음.

## 권고

- 없음. T3(record check log에 넣을 것)는 계획대로 구현자가 채운다.
  - `7064605..01fee56` diff 문장
  - 세 회차 파일의 sha256과 verdict

## 판정

**ACCEPT**. 필수 지적은 0개다.

- 1회차의 R1·R2와 2회차의 R3가 모두 닫혔다.
- S1–S5와 T1·T2도 반영됐다.
- gate는 PASS(0 error, 0 pending)이고, verify_documents도 PASS다.

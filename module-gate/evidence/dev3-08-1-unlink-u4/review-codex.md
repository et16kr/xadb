# PR #59 controller 보강 독립 검토

2026-11-07 KST · D2 joint · 구현 `/root` · 검토 `/root/pr59_independent_review`

판정은 **ACCEPT, 필수 미해결 0건**이다. 코드·계약·추가 Python 반례와 보존 로그에 이어
선택 native 4 이력·13 process의 완료 증거를 확인했다. 검토자는 시험을 재실행하지 않았다.
기준은 PR head `8f8cbee3ef0cc8152a1c17ae0d07fdcca508c88a`, base
`3c9d796da3c54888845ef453f1c77a382316fc29` 위의 후속 수정이다.

## 확인한 내용

- `AGENTS.md`의 변경 범위 선택 검증·원 증거 보존·독립 검토와 `devdoc/contracts/dev3-tree-smo.md#unlink-u4`의
  cut·READY·회계·복구 계약을 적용했다. 기존 제품 의미나 필수 fault inventory를 줄이지 않는다.
- `native_tests.rs`의 `FileIo::read_at`·`write_at`은 trace 길이에 요청 buffer 크기를 쓰고,
  outcome `OK:n:hash`의 `n`에 실제 syscall 완료 길이를 쓴다. 기존 controller가 요청 길이로 WAL prefix 도달과
  anchor 수리 완료를 인정한 것은 이 형식과 맞지 않았다. 수정은 실제 완료 길이를 사용하고 범위와 hash 형식을 검사한다.
- `RowCutMode::Before`는 sync의 `ATTEMPT` 뒤에 cut을 기록하고, `After`는 성공한 sync 뒤에 cut을 기록한다.
  N04가 cut 바로 앞의 동일 WAL `Ok(())`를 요구하므로 두 상태를 구분한다.
- `publish_selector`는 selector 문자열을 outcome에 기록한다. N06의 세 번 공개를 `CURRENT:Ok(())`로 묶고,
  rebase sync·최종 namespace dirsync와 완료된 변경의 성공을 검사하는 수정이 producer와 일치한다.
- N05는 DWB에서 실제 bytes를 읽고 동일 home 파일의 anchor 전체를 공개 전에 기록해야 한다.
  연속된 정상 short write의 합산은 허용하며, 부족한 길이·gap·다른 파일의 나머지 쓰기는 거절한다.
- 새 Python 시험은 독립 합성 trace와 literal 기대값으로 실패한 효과·EOF·부분 I/O·다른 파일/selector를 구분한다.
  정상 부분 I/O도 포함하므로 단순히 모든 short I/O를 거절하는 수정은 통과하지 못한다.
  기존 WAL와 READY 검사의 음성 vector도 포함한다.
- `review-validation/red.log`는 수정 전 8개 시험에서 23개 subtest가 기대한 거절을 하지 않은 기록이며,
  `green.log`는 수정 후 8개 모두 PASS다. 검토자가 시험을 재실행하지 않았으며 원 로그와 코드를 직접 대조했다.
  `build.log`는 `xadb-engine` lib test ELF의 컴파일 완료만 증명하며 시험 실행 PASS로 취급하지 않는다.
- 계약·추가 증거 README는 판정기 오류와 실제 제품 복구 실패를 구분하고, 기존 실행과 후속 선택 실행을 구분한다.
  panic 문자열 검사가 임의 abort를 모두 식별한다는 기존 과장을 바로잡은 것도 적절하다.
- U4의 native process별 crash/recover/reenter와 literal 행·tree/list·allocator·claim 대조는 디스크 DBMS의
  복구 검증 방향에 맞는다. 전원 단절, 추가 recovery fault 전수, hook N01/N02의 외부 구분은 여전히 한계다.

## 증거 정체성

원 실행 commit `e3efd1610e44f09324bc5ca1e3a9f3810c3461b2`를 `git archive`로 읽어 9,263개 파일의
정렬된 SHA-256 manifest를 독립 재구성했다. manifest SHA-256은
`236119bdeca4fc9c6e65f4526f6a2f9c6ff0ec2ade1c8750f6edd29457f59d54`이며 원
`run/execution-plan.json`의 source와 일치한다. 이 확인은 과거 trace를 재실행하거나 다시 판정한 것이 아니다.

`review-validation/source.json`에 적힌 controller·새 Python 시험·native adapter·Rust worker·inventory의
5개 Git blob과 SHA-256도 현재 파일과 각각 일치한다. 검토한 두 코드 blob은 다음과 같다.

| 파일 | Git blob |
| --- | --- |
| `scripts/check_dev3_unlink_native.py` | `dcb612c7bf6b301b8d8d6003592b53495a2b4e3b` |
| `scripts/test_check_dev3_unlink_native.py` | `16cc91dddd38eae79281da3cc493bbfa914d470d` |

## 실행·기록 경계

선택 native `UNL-N01-R`, `UNL-N04-R`, `UNL-N05-R`, `UNL-N06-R`는 controller의 바뀐 hook read,
WAL sync, DWB write, CURRENT barrier 분기를 각각 실행한다. N01/N02는 같은 검사 분기이며 새 음성 vector가
둘 다 확인한다. Rust worker가 바뀌지 않은 상태에서 이 선택은 합리적이고, 원 12개 전체 실행을 새 PASS로
재분류하지 않는다.

선택 native 결과는 **4 이력·13 process PASS, 합계 301.17초**다. 원 실행 디렉터리
`/workspace/scratch/pr59-review/native/`의 plan·results·complete를 보존본과 byte 단위로 대조했다.
13개 phase 종료 값은 produce/crash가 SIGABRT(-6), recover/reenter가 0이다. 각 test log에 panic·stack overflow·
fatal runtime error 문자열이 없는 것도 직접 확인했다. native source manifest의 SHA-256은 plan의 source와,
현재 controller SHA-256은 plan의 controller와 일치한다. `native-artifacts.sha256`의 153개 artifact hash도
원 파일과 일치한다.

원 effect trace에서 다음 경계를 직접 대조했다.

- N01-R: arm 뒤 실제 완료 WAL read의 연속 합집합이 prefix 7,948,646 B를 덮는다.
- N04-R: 끝은 동일 WAL의 sync ATTEMPT → `Ok(())` → `UNL_WAL_SYNCED` cut이다.
- N05-R recover: DWB image read가 실제 16,544 B를 완료했다. 그 뒤 첫 성공 home write는 produce receipt의
  anchor page 12(offset 196,608)에서 16,384 B 전부를 완료했으며 첫 selector 공개보다 앞선다.
- N06-R crash: 성공한 `CURRENT:Ok(())` 공개가 세 번이고, 끝은 CURRENT publish → namespace dirsync
  ATTEMPT → `Ok(())` → `UNL_CURRENT_NEW_NEW` cut이다.

최종 계약의 N06 문구는 성공 dirsync와 publish의 순서를 정확히 정정했다.
`module-gate/records/20261106-dev3-08-1-unlink-u4.json`의 추가 검토는 D2 joint·별도 reviewer·ACCEPT·필수 0이며,
두 Python 파일과 계약 blob을 현재 파일과 대조해 일치함을 확인했다. 계약 blob은
`d53a2f7020de8601d8d54481ca1e64e3dc780752`다. 기존 검토 항목은 보존한다. 이 최종 검토 파일의 hash는
통합 담당이 record의 `log_sha256`에 반영한다. pre-push gate와 원격 CI 결과는 이 검토의 실행 증거에 포함하지 않는다.

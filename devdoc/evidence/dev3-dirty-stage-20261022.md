# DEV3 첫 dirty frame·stage 소비자 인수

2026-10-22 · D3-DIRTY-ALLOCATION-001 · **D1/D2 제한 단위 인수 수용 / 전체 DEV3 미완료**

[소유 계약](../contracts/dev3-dirty-stage.md) · [source별 실행 색인](dev3-dirty-stage-20261022.json) · [현재 구현](../implementation/dev3-implementation.md#dirty-stage) · [시험 계획](../plans/dev3-test-plan.md)

3.05.1-A/B와3.04.1.a의 첫 allocator 소비자를 구현했다. 기존 저장 owner의 원래 fence를 외부 core와 연결하고 caller payload·고정 directory/pin/DPT를 실제 content latch로 보호한다. reserve/cancel은 준비를 latch 밖에서 수행하고 version/content 아래 재검산·stage·page/descriptor/counter/first recLSN/DPT 설치 후 모든 guard와 pin을 반환한 뒤 sync한다. 실패에는 original cause와 retained obligation·WAL IoOwned를 보존한다. exact core와 immutable after-image를 가진 private permit만 installer를 호출한다.

## 실행 범위

| 새 범위 | 원래 source에 묶인 결과 |
| --- | --- |
| 자원 profile | Rust exact1시험/10사례와 oracle 자체4시험 성공. source `c81b1df8aca0bd5265f8e96338b0c110fc5de283c69be512f9bbdbad5ac7af2a`. 원 집계기 marker parsing 오류 FAIL은 보존하고 독립 읽기 감사로 실제 성공 범위만 인정 |
| 수명 compiler | baseline·실제 assembly/release·16negative, 합18 PASS/21.550초≤60초. 새 checker6 PASS. 실제 source `d3321f4dbe007ef7d43404722ad3ca50fa612824d0bb74093a9e51e5940ba9c3` |
| mandatory runtime53 | 원 r1의 DA01-01–06 완료 prefix6 + r2의 수정07/미실행08 2 + DA02–07 첫 실행45. 단일 source의 DA01 전체 PASS로 표시하지 않음 |
| 독립 runtime oracle | 위 DA07 안의55개 named negative 거절 PASS. 실제 partial bytes·ACK witness·cleanup·오류 우선순위 포함 |
| Linux native | profile1/count1의 정상 reserve/cancel, reserve 부분 write, cancel sync 후 오류, fresh DB3 PASS. 실제 WAL/dirty metadata·O/IoOwned 정체성 및 unchanged home/DWB·handle≤5 검산 |
| 새 runner selftests | runtime6(r0), native4(r1) PASS. 통과한 selftests 반복 없음 |
| 정적 검사 | 최종 source의 fmt와 engine all-target/all-feature Clippy PASS. 시험 실행 아님 |
| 최종 S5 | normal profile1/2/3·1→8→63의18operation ignored selector 작성, NOT_RUN. 추가 contention/cut/fault cross-product와 DEV3 전체 inventory는 미완료 |

r2의 source는 `54f3f819639979ce1ba49819f8231a4a6239a1648ec33605471f972835760a6e`, ELF는 `e30fbad61f37f22b16e514caab0d48e36b57d4f2ac580d280ca5f2a252acfabd`다. 정규6그룹/45사례의 누적 실행은24.325초≤120초다. compiler 처음 export 뒤 selftest import가 `.pyc` 한 파일을 추가하여 최초 capture와 실제 compiler source 지문이 달라졌다. 독립 감사가 그 한 파일만 제외하면 최초 지문을 재현함을 확인했고,18 cell은 실제 보존 정본 d332…를 따른다. 이후 실행은 bytecode 생성을 비활성화했다.

## 실패를 보존한 수정

r0 DA01 첫 bind가 Capacity/Unchanged로 거절됐다. 계약은 lock guard와 LockResult에 각각32bytes를 배정했지만 구현과 시험이 합≤32로 잘못 제한했다. 기존2×32 예산을 유지하며 각≤32로 고쳤다. r1 raw diagnostic의 실제값은16+24=40bytes이고 fits를 모두 통과했다. 이 진단은 단독 layout 관측이며53개 인수나 physical stack 검증을 대체하지 않는다.

r1은 DA01-01–06과 actual read-paused CLAIMED loser를 끝낸 뒤07에서 실패했다. 시험이 기존 support에서 사용하지 않는 `read_fail` 필드를 설정해 실제 오류가 주입되지 않았다. `read_plan`의 실제 첫 open BeforeError로 수정하고 configured hit1·continuation0·actual event·불변 파일까지 확인했다. 독립 source/log 감사로 완료 prefix6을 원래 source에 보존하고, 새 ignored repair selector로07/08만 실행했다. r0/r1 aggregate와 DA01 group은 계속 FAIL이며, 후속 선택에서 생략한 DA01을 새 정규 실행의 PASS로 세지 않는다.

Clippy는 제품의 오류 전달 closure와 시험 코드 네 곳을 지적했다. 같은 fence/error 순서의 inspect_err, NLL로 끝나는 view, 시험 oracle의 명시 인자 허용, 동등 bool 표현·이름 있는 ledger 계산으로 정리했다. 최종 정적 검사는 통과했다. 원래 실패·소스·로그·미실행 상태는 [실행 색인](dev3-dirty-stage-20261022.json)에 남긴다. 정적 검사 완료는 tool output의 session/chunk로 보존했으며 별도 stdout 파일이 없는 한계를 명시한다.

## 자원과 한계

실제 test layout은 frame136, pool9992, writer control1880, Bound/Result11880, obligation/Option192, permit208bytes다. DirtyObservation/Result176, FrameStamp/Result96, lock guard16/Result24bytes다. 기존 deep producer에 새 caller2048·moved carrier2048을 더한 actual source-value aggregate23224≤65536을 확인했다. 새 dirty control은 retained60672+34×2048=130304, cold 합150784≤196608로 검토했다. metadata 배정2424832≤8388608과 caller payload/workspace를 구별한다. 실제 stack/RSS나 미구현 전체 engine F05를 인증하지 않는다.

INIT·새 target 등록·flush/retirement·FIFO·tree/row/index/catalog/undo/Commit·일반 recovery/READY와 전체 DEV3 S5는 남아 있다. 기존 하위 시험, 전체 wrapper/workspace 시험, 문서 verifier는 실행하지 않았다. 문서 검증은 push gate로 유지하며 이번 작업은 로컬 단계 commit이다.

독립 최종 검토 `/root/allocation_review`는 D1 ACCEPT_SCOPED로 판정했다. 원본 `review/dirty-stage-final-review-r0.md`의 SHA는 `eaa5322c2a1ec550e64063756d8944068c10559547179267087f8efb1bdb7a4c`다.625개 source 파일·819개 hash·45개 native DB 파일의 stat/hash와 전체 WAL/literal을 별도로 대조했으며,53개 source별 사례의 결합 범위와 원본 FAIL/NOT_RUN을 유지한다.

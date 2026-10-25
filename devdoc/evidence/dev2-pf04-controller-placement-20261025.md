# DEV2 PF04 원 native 제어기의 CPU 배치 대조

2026-10-25 · **디버깅60slot 완료 / 큰 배치 효과 미지지 / 원 PF04 FAIL·DEV2 미완료 유지**

[기계 판독 기록](dev2-pf04-controller-placement-20261025.json) · [계획](../plans/dev2-pf04-resolution-plan.md#controller-placement) · [앞선 모드 비용](dev2-pf04-mode-attribution-20261025.md)

## 관측 결과

원 실패 때의 e3 worker ELF를 사용해 제어기CPU2/3·workerCPU2를 대조했다. Off/Always/Detailed 각각10쌍, 모드별 방향5/5인60slot 모두 원 독립 oracle과 identity·affinity·종료 검사를 만족했다. 전체7.508770897초, 재시도·실패·중단0이며 각 모드의 분류는 `NO_LARGE_PLACEMENT_SHIFT`다.

| 모드 | CPU2/CPU3 CPU 중앙 | 정렬[1,8] 구간 | 방향별 중앙 비율 |
| --- | ---: | ---: | ---: |
| off | 1.006134703 | 0.996329064–1.012031604 | 1.003871669 |
| always | 1.004802028 | 0.987626987–1.017311434 | 1.008696532 |
| detailed | 1.002576059 | 0.998470952–1.007644020 | 1.005370385 |

구간은 IID 가정 아래 명목1002/1024이며 순차 표본의 독립성을 보장하지 않는다. 이번 구성에서10%보다 큰 중앙 배치 차이는 지지되지 않았다. 과거 실패의 원인이나 주파수·cache 가설을 확정/기각하지 않으며, 기존 네 FAIL·DEV2/B2/D0 미완료를 유지한다. p50은 Off240–241ns, Always251–270ns, Detailed261–270ns였다. 과거의 Off280–281/Always300–301/Detailed310–311ns 대역은 관측되지 않았다.

## 비교 경로와 품질

계획 `61293d7`와 준비 `56ebd6d`를 먼저 commit했다. 원 source204파일은 byte가 같고 진단 example1개만 추가했다. 새 controller source는 `bd4a204ae7df29d0e6dd08d343c94ec14944523d0b27f251804f1138f4c30ab8`이며 원 controller ELF와 동일하다고 주장하지 않는다. worker는 원 SHA-256 `b770e4b2de495abcc661bff4a9abea8e4ab68c1197dad8c7dc05bb89189dac61`, source `e3bccf1a865b75deafa9eeac551f17591df406da1c41ccbdf9ec909c9c8e0f1a`를 그대로 사용했다.

원 native/process/socket/200µs receive 대기·독립 oracle을 호출한다. 매 slot에서 controllerCPU2로 spawn한 worker가CPU2를 상속한 뒤 controller만2/3으로 이동한다. 입력 전후 worker PID/starttime/exe/PPid/CPU와 controller CPU를 결합한다. 원 코드 밖의 추가 oracle·응답 저장·정렬/hash는 두 조건 모두 finish 뒤CPU4에서 수행했다. 이 추가 관측·taskset·후처리 간격이 있는 새로운 동일 구성의 대조이며, 원 campaign 전체를 그대로 복제한 실행이 아니다.

입력은 원 seed20261018·128row·old snapshot·capacity256·warmup4096·measure65536·stride16이다. batch 앞10초 유휴 확인1회에서CPU2/18/3/19 및 실제 ext4 장치 busy≤5%를 만족했다. slot 사이 추가 유휴 대기는 없었다. 시작 전 유휴는 측정 중 무간섭을 보장하지 않는다. PMU를 부착하지 않았고 CPU 속도 snapshot을 loop 주파수로 해석하지 않는다.

새 전용 systemd user service가 별도PGID의 worker까지 소유했다. 실제 Type=exec·Restart=no·RuntimeMaxSec300·TimeoutStopSec30·KillMode=control-group을 결합하고 고정한 cgroup 경로의 빈 상태를 확인했다. 정상 종료 뒤 소유 자식 잔존0이다. 원 서비스 재시작·재부팅·host 설정 변경은 없다.

## 도구 검증과 보존

Rust4·서비스 소유8·분석/idle4개 고유 함수의 source별17회 선택 실행을 보존했다. 정확 median의2배(`p50_twice_ns`), 방향/품질/identity·deadline, 새 서비스의 별도PGID·SIGTERM·bind 전 중단·관측/정리 명령 실패를 확인했다. 변경된 p50 필드에 직접 관련한 분석1함수만 다시 검증했다. 과거 `test_analysis.py`의 duration 없는 fixture PASS는 그 당시 source 범위이며, 후속 duration gate는 새 `test_guards.py`의 정상/누락/초과 검증으로 연결한다. 현재 전체 suite를 실행했다고 주장하지 않는다. 기존 제품 시험 반복0이며 새 출력60개의 정답 검산은 별도다.

원 fmt CLI 오류와 Clippy3회 실패를 보존한다. 최종 새 example의 fmt·Clippy·release build는 PASS다. 준비250개 지문과 독립 preflight를 결합한 뒤 한 번 실행했다. 보존 root는 `/home/et16/xadb-dev2-evidence/pf04-controller-placement-20261025`이며 source·도구·실행·분석·독립 검토·manifest 지문은 연결 JSON을 따른다.

## 계속할 해결 작업

이번 결과로 같은 CPU의 제어기 대기만을 원 큰 CPU 초과의 설명으로 채택하지 않는다. 이미 확인한 정상 계측 비용과 원 실행 전반의 느린 대역을 구분하고, CPU 실행 상태를 통제하는 방법과 영속 경로의 공유 journal 대기를 이어 검토한다. 개별 진단 종료를 전체 작업 종료로 해석하지 않으며 원 인수 기준을 낮추거나 기존 FAIL을 삭제하지 않는다.

최종 독립 D1/D2 문서·보존 검토를 한정 채택했다. 정정 manifest는999파일·1,946,111,356byte이며 원 manifest의 native metadata60개 누락과 정정 이력을 함께 보존한다. 원자료 독립 검산은 scalar3,932,160·latency245,760·paired150비율·240 affinity 상태·60 worker identity를 확인했다. 검토 receipt와 manifest 지문은 기계 판독 기록에 결합했다.

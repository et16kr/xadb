# DEV2 PF04 CPU 성능 요청 범위 제어

2026-10-25 · **구현·선택 검증 완료 / 첫 host 연결 FAIL·나머지2개 NOT_RUN / 사용자 조건에 따라 중단 / PF04·DEV2 미완료**

[한정 계획](../plans/dev2-pf04-resolution-plan.md#frequency-control) · [source별 기록](dev2-pf04-frequency-control-20261025.json) · [선행 CPU 배치 결과](dev2-pf04-controller-placement-20261025.md)

## 범위와 구현

계획 commit `84cfe84` 뒤 독립 검토의 시간 제한·지속 lease·부분 적용 원복 요구를 `014e9ef`에 반영했다. 담당은 `/root`, 작성자 외 D1/D2 검토자는 `/root/pf04_evidence_review`다. 사용자의 후속 요청에 따라 이번 범위는 이미 계획한 제어 도구와 정상·부분 ExecStartPre 실패·본체 SIGKILL의 최대3개 기능 연결까지다. 미해결 문제가 남으면 새 성능 진단이나 가설을 추가하지 않고 증거·재개 지점을 정리한다.

[helper](../../scripts/dev2_frequency_control.py)는CPU2/18의 `scaling_max_freq`만5,460,527에서2,500,000kHz로 일시 낮춘다. min1,480,264·governor/EPP·전역 boost·driver mode·다른 CPU는 변경하지 않는다. 설치 커널과 대응하는 Ubuntu AMD-pstate 소스의 performance 분기에서 nominal min과 max 요청이76으로 일치하는 조건이며, 실제 loop 주파수 고정이나 과거 실패 원인을 입증하지 않는다.

새 root 소유 `/run/xadb-pf04-frequency-<id>/`의 code/config는0444, 디렉터리는0755, state는0600이다. 고정 `/usr/bin/python3 -I -B`와 표준 라이브러리만 사용한다. 코드 hash·unit/cgroup·CPU identity·원 한도를 확인하고, User1000 본체와 `+` 접두의 root Pre/Post를 분리한다. 제품 worker·WAL·공개 형식은 변경하지 않는다.

적용 전 실제 systemd 속성은 User1000·Type=exec·Restart=no·RemainAfterExit=no·KillMode=control-group·RuntimeMax10초·Start10초·Stop30초·SendSIGKILL=yes여야 한다. helper 자체 적용5초·원복25초 deadline을 추가한다. 본체10초는 설정·정리까지 포함한 전체 제한이 아니다.

완성한 root config의 atomic hardlink로 전역 lease를 확보하고 operation flock으로 apply/restore를 직렬화한다. 매 write 전 의도·원 값·후 값을 저장한다. 프로세스가 종료돼도 lease와 intent가 남아 StopPost가 원복할 수 있다. 외부 설정 충돌이나 원복 실패는 QUARANTINED와 lease를 남겨 다음 실행을 막는다. 무관한 privileged writer에 대한 atomic CAS는 아니다.

## source별 선택 검증

신규 [시험](../../scripts/test_dev2_frequency_control.py)11함수를r1에서 실행했다. 정상 max-only/반복 restore, 부분Pre실패, write 직후 실제 fork child의 `_exit`, identity 거절, 다른 lease 보호, 외부 max/다른 policy 충돌, 원복 write 실패, 상태/lease 결합, deadline, 미설치 privileged CLI 거절을 통과했다. 모두 fake sysfs이며 실제 CPU write는 없다.

이후r2에서 실제 systemd 속성 확인·종료 환경 receipt를 추가하고 해당 신규1함수만 실행해 통과했다. 이전11은 반복하지 않았다. 총12개 고유 함수의 source별 PASS이며,12개를최종 source에서 모두 재실행한 기록은 아니다. 세 Python 파일의 문법 확인을 별도로 보존한다. 별도 일회성 실행 driver는 독립 정리·증거 수집 실패 보존, exe/cwd/args 기반 worker/build 감시,10초/CPU 필드 검사, 동결 입력 drift 거절의 신규4함수를 추가 검증했다. 원 driver의 직렬 finally·불완전 process 이름 감지·동결 미연결 지적은 최초 검토와 r1 사본에 보존하고 보완했다. 최초 관측10초 동안0.25초마다 process를 확인하며 표본 사이의 모든 간섭 부재를 보장하지 않는다.

제품 회귀·전체 workspace·완료 성능 진단 실행0회다.

외부 원본은 `/home/et16/xadb-dev2-evidence/pf04-frequency-control-20261025/`에 보존한다. `unit-selection-r1/r2.json`·원 로그·source 사본·커널 연구 identity·독립 검토를 구분한다. host 기능 연결과 원복 결과는 아래에 구분한다.

## 실제 연결: 첫 정상 사례에서 중단

구현·준비를 `64dbb21`에 commit한 뒤 동결16개 입력을 확인했다. 최초 유휴 관측10.007초 동안 CPU2 busy2.004%·CPU18 busy0.100%, 관련 build/worker 감지0으로 통과했다. `normal` 한 사례만 시작했으며 전용 unit의 실제 시간·권한 속성 확인도 통과했다.

- CPU2 max를2,500,000으로 쓰는 intent를 저장하고 write했으나 곧바로 읽은 값의 확인이 `apply readback mismatch`로 실패했다. CPU18 적용과 일반 사용자 본체는 시작하지 않았다.
- systemd가 실패한 Pre 뒤 root StopPost를 실행했다. 이 시점 CPU2 max는2,500,000으로 관측됐다. 원 값5,460,527로 복구하는 intent·write를 수행했으나 즉시 확인은 `restore readback mismatch`로 실패했다.
- 원 state는 **QUARANTINED**이며 apply/restore 두 오류·두 intent를 보존한다. 드라이버는 `restore receipt` 오류로 exit1, 전체 연결0.239초를 기록했다. 실행에 성공한 본체0개·연결PASS0개다.
- 드라이버의 독립 host-after와 뒤의 읽기 전용 종료 확인은32개 policy·전역·identity 등 **234필드 모두 실행 전과 동일**했다. CPU2/18 max는각각5,460,527이며 관련 cgroup은 소멸했고 MainPID/ControlPID는0이다. 이는 최종 환경 값 확인이며, 도구의 자동 원복 검증 PASS를 뜻하지 않는다.
- 사용자 중단 조건에 따라 `partial-pre-failure`·`body-sigkill`은 **NOT_RUN**이다. 실제 재시도·추가 sysfs write·새 성능 실행·완료 시험 반복은0이다.

실패 순간 readback snapshot은 저장되지 않았다. 코드와 후속 restore-intent를 통해 상태 변화의 일부는 추론할 수 있지만, 비동기 반영 여부나 지연 길이를 직접 입증하지 못한다. 즉시 readback 일치를 기대한 도구 전제가 이 실제 연결에서는 충족되지 않았다는 범위로 기록한다. 이를 과거 PF04 CPU 비용 실패의 원인으로 연결하지 않는다.

## 종료·재검토 지점

원 실패 state·실행 로그·journal·before/after·독립 종료 확인을 보존했다. 계획의 과거 NOT_RUN 표시는 실행 뒤 시점 구분만 보완했으며, 실행 당시 원 byte를 `plan-at-execution.md`와 별도 사후 binding에 보존했다. 최초48개 manifest도 `manifest-r1.json`으로 남기고 보존 파일을 포함한 최종 목록을 구분했다. `/run/xadb-pf04-frequency-5e22a8418cf94da2/`와 해당 config의 전역 lease를 그대로 남겨 검증되지 않은 새 적용을 차단한다. 이 전용 unit은 failed 상태(실행 프로세스 없음)이고 실행 중인 제어 프로세스는 없다. root 상태를 RESTORED로 고치거나 lease를 강제 해제하지 않았다.

사용자의 “여기에서도 중단되면 다음은 일단 멈추고 처음부터 고민” 지시에 따라 **이 접근의 추가 구현·시험을 멈춘다.** 다시 시작할 때는 설정 write의 수락, 실제 반영 관측, 원복 완료를 어떻게 구분할지부터 검토해야 한다. 실패 readback 원자료를 저장하지 못한 관측 한계도 함께 검토한다. 이는 재개 후보이며 새 계획 채택·자동 재시도 요청이 아니다.

CPU 제어 도구의 실제 인수는 미완료이고, 원 PF04 네 FAIL과 DEV2 미완료도 유지한다. 재부팅·기존 서비스 재시작은0회다. 문서 자동 검사는 push gate로 남겼으며 이번 요청에서 push하지 않았다.

최종 독립 검토는 `PASS_SCOPED_D1_D2_FAILED_CONNECTION_AND_USER_STOP_RECORD`로 실패·환경 종료·사용자 중단 기록만 수용했다. 최종 manifest51개582,370bytes, 원 preparation16개(계획은 실행 당시 보존본),234필드·동일 invocation의 Pre/Post 오류를 재측정 없이 검산했다. 이 판정은 도구 연결이나 성능 PASS가 아니다.

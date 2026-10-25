# DEV2 PF04 CPU 성능 요청 범위 제어

2026-10-25 · **구현·선택 단위 검증 완료 / 실제 host 연결 NOT_RUN / PF04·DEV2 미완료**

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

외부 원본은 `/home/et16/xadb-dev2-evidence/pf04-frequency-control-20261025/`에 보존한다. `unit-selection-r1/r2.json`·원 로그·source 사본·커널 연구 identity·독립 검토를 구분한다. host 기능 연결과 원복 결과는 후속에 기록한다.

## 잔여와 의미

구현·실행 준비의 최종 독립 D1/D2는 한정 수용됐다. 실제 최대3개 기능 연결·독립 결과 검토는 아직 미실행이다. 새 성능 profile·제품 비용 측정은 이번 범위가 아니다. 원 PF04 네 FAIL과 DEV2 미완료를 유지하며, 도구 완료를 성능 해결로 표시하지 않는다. 재부팅·기존 서비스 재시작은 하지 않는다.

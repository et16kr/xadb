# DEV3 allocator 한 페이지 flush 선택 인수

2026-10-22 · **제한 선택 인수·독립 검토 완료 / 전체 DEV3 미완료**

[소유 계약](../contracts/dev3-allocator-flush.md) · [source별 실행 색인](dev3-allocator-flush-20261022.json) · [단계별 시험 정책](../plans/dev3-test-plan.md)

실제 allocator1의 coherent capture→WAL durable→DWB issuance/image/Pending→home sync→whole retirement→조건부 RAM finalize를 연결했다. batch8/64·다중 file·INIT·free/reuse·ordinary recovery·READY·전체 DEV3 인수는 남아 있다. 아래 결과는 각 보존 source의 선택 범위이며 현재 전체 source나 다른 단계의 PASS로 확장하지 않는다.

| 선택 범위 | 실제 결과 | 보존 source |
| --- | --- | --- |
| 새 WAL requirement/완료 대기 | 8선택자·30사례 PASS | `4060ef9c88718c2d28b79a93e7f7f39cd21949c07a66673fff5e563467ef98ae` |
| 새 flush compiler | 12 PASS | `71410c2fe3c45ca215d8a79c296243350db504bc7b530fa1c8ac3b19387e5cb3` |
| 변경된 기존 permit constructor | 해당1개+필수 baseline PASS; 다른 기존16개 NOT_RUN | 같은 `71410c2f…` |
| 새 compiler checker | 4 PASS | 실행기·입력 hash는 JSON 참조 |
| 새 oracle/runtime checker | 6+6 PASS | `699becbe620ee5a1c7ff78567a11a11265600b6ead2c86f035bedfcd20fd5568` |
| 새 native checker | 최초8 PASS | `dc51cdb3b7001c7ae75103a9fa1105376204727ff277e87c26c81c42969e5cf2` |
| 실제 flush 필수76 | 자원2+정상/오류5그룹67+완료 prefix3+수정/미실행4 | 아래 두 source별 범위 |
| native P1 | 원 worker 완료·독립 bytes/events/FD 감사로 수용; 원 driver FAIL 보존 | `360cc550…` |
| native P2/P3·새 sampler checker1 | 2 DB와 새1시험 PASS | `b817e75f…` |
| 최종 fmt/engine Clippy | PASS; 제품 시험 실행 아님 | 같은 `b817e75f…` |
| 최종 S5 batch1 확대972 | S5_NOT_RUN | 구현 동반 입력; 최종 DEV3 인수에 선택 |

필수76의 첫 유효 source `360cc550cb5cedb81add1fd6df4d8db333f02ca138043f69c46d014531a01143`에서 FR01/02를 별도로 통과한 뒤, 두 사례를 제외한74개를 선택했다. FN6·FA13·FB31·FP11·FU6의67개는 완결 그룹 PASS다. FC01–03은 assertion 뒤 완료 marker를 가진 독립 감사된 prefix이며 실패한 FC 그룹 전체의 PASS가 아니다. FC04는 fence 뒤 새 owner를 만들려는 fixture 오류로 멈췄고 FC05/06/FR03은 NOT_RUN이었다. 같은 dedicated owner를 장애 전부터 유지하는 fixture 수정 후 source `b817e75fd00f37edd732cd433d4120c063b9506916bdef070fa12771b0ce785d`에서 FC04와 미실행3개만 통과했다. 이 네 실행은0.979초, 앞선74 선택 시도는61.581초, 자원2개는3.622초였다. 완료된 무변경 사례는 반복하지 않았다.

FA05의 stored_selector/possible_selection과 FA09의 issuance_after_dirsync/retirement_after_dirsync 네 변형은 기존76행 안의 추가 검증이다. exact 구간·순서·중복 없는 raw 진단을 독립 감사했다. FA13은 별도6변형, FR03은 독립 bad witness19와 실제 positive witness를 검산한다. 원16입력을 보존하고 retirement→finalize·image write→sync·capture→image의 누락 순서 반례3개를 추가했다. WAL durable fact는 capture보다 먼저 성립할 수 있다.

native P1은 두 batch·62효과·전체 파일 검산이 완료됐으나 보조 `/proc` sampler의 EACCES 한 건 때문에 원 driver는 FAIL_PROC_OBSERVATION이다. 당시 종료 경합을 확인할 lifecycle 기록이 없어 원인을 단정하지 않는다. P1은 재실행하지 않고 독립 감사가15파일 실제 bytes·중간 inventory·62효과·FD 경계3→4→0·wrapped File2를 확인했다. 원 sampler1807관측/peak5와 INCOMPLETE를 별도로 보존한다. 새 sampler는 같은 Popen의 terminal 관측과 정확한 `/proc/<pid>/fd` 권한 오류만 별도 기록하며 live denial·DB 오류는 계속 실패시킨다. 이 변경의 새1시험만 실행한 뒤 P2/P3를 최초 실행하여 통과했다. 두 새 실행에서도 proc 접근 불가를 관측했으나 같은 child의 poll이 None→exit0으로 전이한 사실을 별도 보존했다. 이를 P1의 미확정 원인에 소급하지 않는다. 세 DB 실제 합계66,334,944 bytes, 각 고정 charge22,073,344와 WAL 증가 뒤 실제 DB22,111,648·selector 교체 관측22,115,744를 구분한다. FD 경계·보조 표본은 정확한 물리 최대치 측정이라는 주장이 아니다.

자원 시험 최초 source `dc51cdb3…`에서는 Profile72가 cap64를 넘어 FR01 FAIL, FR02 NOT_RUN이었다. 실제 타입 크기와 반환형을 확인하여 Profile/FILEMAP의4개 전제를 기존8192 예약 안에서 고쳤다. 최종31개 실제 layout과 effect 전 File8184/8192 허용·8200 거절·큰 owner와8192 File의 합산 거절을 확인했다. 새 metadata65536 안의 hot63960/cold48576, concrete deep24624이며 기존 dirty196608과 합계262144, global2490368이다. 원4MiB workspace 중164000을 사용한다. source의 initialized value 상한이며 physical stack/RSS/latency는 NOT_MEASURED다.

WAL hook shadowing, 테스트 re-export/lifetime, Clippy, re-export fmt, FC04 injector의 move capture 누락에 따른 원 build/static FAIL도 JSON의 source별 receipt로 보존한다. 이들 빌드 실패를 제품 시험 실패나 PASS로 바꾸지 않는다. raw source·ELF·로그·입력·리뷰는 `artifacts/dev3/resume-20261022/`의 원래 경로와 SHA256으로 보존한다. 문서 verifier와 하위 wrapper는 실행하지 않았다.

독립 최종 D1은 **ACCEPT_SCOPED_UNIT**이다. `review/flush-final-d1-review-r5.md` SHA `071193f18b325b2279d9048dcae7c601ea600c346e17f56bc9aa9c0075b461d9`와 실행 감사 JSON을 위 source별 색인에 연결했다. 구체적 미해결 지적은 없으며 후속 owner/INIT·batch 확대·일반 recovery·S5/전체 DEV3는 이 인수에 포함하지 않는다.

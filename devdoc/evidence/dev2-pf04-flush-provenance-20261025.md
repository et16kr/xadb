# DEV2 PF04 flush 완료 구분과 수집 출력 수정

2026-10-25 · **변경 도구 검증·후속 관측 완료 / 원 수집 FAIL 보존 / 독립 D1/D2 한정 검토 채택 / PF04·DEV2 미완료**

[계획](../plans/dev2-pf04-resolution-plan.md#p3-flush-provenance) · [수집 오류 보완 계획](../plans/dev2-pf04-resolution-plan.md#p3-stack-output) · [기계 판독 기록](dev2-pf04-flush-provenance-20261025.json) · [앞선 P3](dev2-pf04-io-cause-20261025.md)

## 변경과 결론

사용자의 후속 진행·단계별 commit 요청에 따라 계획 `f157846`을 먼저 커밋했다. 작업·계약·통합 담당은 `/root`, 독립 검토자는 `/root/pf04_evidence_review`다. 기존 P3의 동일5253caa export/ELF를 재사용했다. 제품 변경·재빌드·완료 하위 시험 반복·재부팅·서비스 재시작은0이다.

이번 수집기는 물리 요청 완료와 flush 순서의 논리 종료를 구분한다. 변경된 출력 버퍼로 수집한 Always에서는2,932개 COMPLETE가 물리2,347·논리585로 전부 대응했다. 측정512개 fsync 시간2.725706초 중 장치 요청과 겹친 시간이2.695842초(약98.90%)다. journal wait2.706081초 중2.696498초가 logging→end 구간에 있다. **이 관측에서는 wait의 대부분이 journal logging→end 구간에 있고, fsync의 대부분이 장치 요청 구간과 겹쳤다.** 공유 journal의 배타적 WAL 소유나 순수 장치 service time을 뜻하지 않는다.

원 Always 수집은 count/행 불일치로 실패했다. 실제 출력 버퍼 결함 가능성을 확인해 계획 `f18700d`를 선행 커밋하고 버퍼를 호출별 stack으로 옮겼다. 바뀐 소스에서 실패 연결1개와 미실행 대조1개만 검증했다. 원 FAIL을 지우거나 잘못된 행을 제거하지 않았다. 이번 결과는 원 PF04 네 비용 FAIL과 DEV2/B2/D0 INCOMPLETE를 PASS로 바꾸지 않는다. K0·영속 CPU 비용의 원인 설명과 P4 후보 판단은 후속 작업이다.

## 완료 provenance와 journal 분석

[16-probe BPF](../../scripts/dev2_io_trace.bt)는 `blk_flush_complete_seq` 진입의 seq_before/action/error/rq_flags/cmd_flags/data_len/end_io/saved_end_io를 추가했다. ISSUE/COMPLETE도 해당 필드를 보강하고 SCHEMA2를 명시한다. free 뒤 포인터를 읽는 fexit는 사용하지 않는다. 현재7.0.0-38 kernel의 FLUSH_SEQ bit1과 ACTIONS7에 결합한다.

[분석기](../../scripts/dev2_io_analysis.py)는0..7의 disjoint seq/action mask와 error0·OR7의 DONE 전환을 확인한다. action0·다중 bit도 허용한다. 다음 COMPLETE가 같은 요청이며0byte 성공, cmd/data 일치, FLUSH_SEQ만 제거된 flags, saved_end_io 복원을 모두 충족해야 논리 종료다. 물리/논리/unknown 완료 census의 합은 원 COMPLETE 수와 같다. marker가 없는0byte 완료는 고아다. 충돌·잘못된 marker·완료 불일치가 생긴 세대는 새 ISSUE 전까지 미확정으로 유지한다. 출력 순서·동일 timestamp·partial/requeue·경계와 count 불일치를 보수적으로 처리하며 v1 판정을 보존한다.

journal의5개 trace 지점을 transaction ID로 연결해 네 구간과 wait 앞/뒤 잔여를 분리한다. 누락·순서·재사용·동일시각은 해당 분모를 거절한다. commit sequence 공개는 end trace보다 먼저여서 wait 반환이 end보다 앞설 수 있다. signed return-minus-end를 유지하고 순수 scheduler 지연이라고 하지 않는다.

## 원 실패와 출력 버퍼 수정

원 slot2에는 ISSUE kernel2712/저장2711, COMPLETE kernel3299/저장3300의 차이가 있었다. COMPLETE 원행10198/10199는 timestamp·pointer·전체 byte가 동일하다. 분석기가 이 자료를 거절했고 slot3은 시작하지 않았다. 제품 scalar·ACK/source/restore 검산과 root 소유 정리는 통과했어도 수집의 실패를 숨기지 않는다.

설치 bpftrace0.25.0-1ubuntu1의 생성 IR에서 ISSUE/COMPLETE는 같은 `__bt__fmt_str_buf[cpu][0]`를 공유해 각각 format ID11/12와56byte를 출력했다. [공식0.25 소스의 allocation](https://github.com/bpftrace/bpftrace/blob/v0.25.0/src/ast/irbuilderbpf.cpp)과 [기본 config](https://github.com/bpftrace/bpftrace/blob/v0.25.0/src/config.h)는32byte를 넘는 payload가 CPU별 scratch에 놓이는 것을 뒷받침한다. 다른 BPF program의 중첩이 이 buffer를 덮으면 실제 중복 패턴을 설명할 수 있다. 당시 CPU/IRQ 중첩을 직접 관측하지 않아 단일 원인 확정으로 쓰지 않는다.

보완은 고정 config `on_stack_limit=128`이다. 최대72byte 출력15개가 실제 optimized IR에서 각각 stack alloca를 사용하고 ringbuf output에 직접 전달됨을 확인했다. 공유 printf buffer 참조는0이다. 이후 실제16-probe attach와 새 Always 연결도 통과했다. root owner 수명·권한·PMU·schema·raw count/행 검증·정답 기준은 유지했다. count()를 부정확한 일반 증감으로 바꾸거나 raw 중복을 제거하지 않았다.

원 Off/trace는 당시 count·연결 검사를 통과했다. 다만 같은 공유 scratch 버전이므로 count 일치가 조용한 payload 오염의 완전한 부재를 증명하지는 않는다. 그 한계를 명시하고 원 판정을 소급 변경하거나 Off를 반복하지 않았다.

## 실제 선택 실행

제품 source fingerprint는 `7ced0a5622825b971a4a146cdd8d1ce4a5ef01eefe633e59deb6b338390a5329`, 기존 release ELF SHA-256은 `ea1f0f7c5f2d58a61199f399ae6961f290449dea488a9187ebf11505bc501c51`다. seed20261018·128행·warmup64·측정512를 유지했다. 각 slot 전 CPU2/18·disk의10초 busy≤5%를 요구했으며 CPU 집합/8필드/비음수 delta/양수 시간·total을 먼저 검증했다. 유휴 조건이 없던 세 window는 실행 전에 기다렸다.

외부 driver는 payload 전에 실제 worker CPU2, runner/root supervisor/BPF CPU3의 affinity와 PID/starttime을 확인·보존했다. worker 일반 권한, 실제 PMU6개, host CLOCK_MONOTONIC/boot/time namespace와 소유 process 종료를 연결했다. 표시 PMU coverage100%가 multiplexing의 완전한 부재를 증명하지는 않는다. shared host·측정 중 부하 변화의 한계를 유지한다.

| source/slot | 모드·BPF | elapsed(ms) | CPU(ms) | cycle p99(ms) | 정답 | 수집 |
| --- | --- | ---: | ---: | ---: | --- | --- |
| 원 v2/0 | Off/control | 2754.384237 | 28.190175 | 6.065518 | PASS | PASS |
| 원 v2/1 | Off/trace | 2746.991041 | 28.925698 | 5.778102 | PASS | 원 검사 PASS·scratch 한계 |
| 원 v2/2 | Always/trace | 2765.722462 | 17.659738 | 7.223756 | PASS | **FAIL count/행 불일치** |
| stack/2 | Always/trace | 2735.125743 | 23.061924 | 5.517020 | PASS | PASS |
| stack/3 | Always/control 최초 실행 | 2754.364755 | 24.749558 | 5.523400 | PASS | PASS |

원 v2/3은 NOT_RUN이며 stack/3이 미실행 사례의 첫 실행이다. 총5회는 원3회+수정2회다. 정상화한 임시 output copy만 기존 scalar·ACK/image checker에 전달하고 원 path/PMU/raw는 보존했다. 각각 별도 source와 provenance를 가지며 현재 제품 인수5개 PASS로 세지 않는다.

| 연결된 추적 | COMPLETE 물리/논리/unknown | 측정 fsync(ms) | wait 겹침(ms) | 요청 겹침(ms) | FLUSH/FUA 겹침(ms) |
| --- | --- | ---: | ---: | ---: | --- |
| 원 Off·scratch 한계 | 2609/590/0 | 2736.234335 | 2707.343158 | 2689.646787 | 666.451202/1990.349979 |
| 수정 Always | 2347/585/0 | 2725.705550 | 2706.080891 | 2695.842352 | 669.614009/2003.991438 |

두 연결 분석의512 wait 각각은 transaction phase에 대응했고 측정 안 unknown은0이다. 전체 capture census와 측정구간 요청 수는 다르다(Off2315·Always2055). 측정 밖 열린 요청1개씩은 별도 보존했다. FLUSH/FUA 등 범주는 서로 겹칠 수 있어 합산하지 않는다. 서로 다른 수집기 source의 두 관측을 하나의 모드 비용 비교 표본으로 합치지 않는다.

coarse trace/control screen은 원 Off에서 elapsed0.997316·CPU1.026091·p990.952615, 수정 Always에서0.993015·0.931812·0.998845로 각각 PASS다. 동결 한도는 elapsed/p99≤1.25, CPU≤2다. 모드별 한 쌍의 도구 영향 screen이며 원 PF04 한도·10쌍 성능 확인을 대체하지 않는다. 원 Always FAIL은 그대로 남는다.

## 검증·보존·다음 판단

신규 분석/driver 단위는 고유11개·source별12회다. r1 신규9개, r2의 비동기 schema 위치 수정 관련1개, r3/r4의 서로 다른 세대 taint 반례 각1개다. r5의 실제 compiler/IR 신규1개를 더해 고유12개·실행13회이며 모두 해당 source의 PASS다. 초기 BPF codegen1회와 실패 진단용 IR 생성은 별도 정적 기록이다. source-r1..r5와 선택 이유를 보존한다. 검토 지적을 수정한 범위만 검증했으며 기존 owner6개·전체 wrapper·하위 회귀 반복은0이다. 문서 자동 검사는 push gate에 남겼고 push하지 않았다.

원자료 루트 `/home/et16/xadb-dev2-evidence/pf04-flush-provenance-20261025`의 manifest252파일 SHA-256은 `c2d9d22e8bc6d277a178d89db51e589676ff43fcb939023f20bbc423690436fd`다. 원 실패/정상 raw·5개 fixture·PMU/affinity/정답·source freeze·BPF IR·공식 source/hash·선택 로그를 포함한다. 제품 ELF는 기존 P3 경로에 보존하고 매번 기대 hash와 대조했다.

P4는 append에 따른 metadata 동기화와 파일 공간/논리 EOF·quota·ACK/fence·복구 계약을 대조해 채택/기각한다. 전체 fsync가 느리다는 사실만으로 상대 계측 비용 실패가 해결되지는 않는다. 이번 CPU 값도 Off/Always 실행 간 변동이 커서 단일 비율로 원 CPU FAIL을 설명하지 않는다. 이 관측만으로 목표 완료를 선언하지 않는다.

최종 독립 D1/D2 검토 **PASS_SCOPED_D1_D2_P3_FLUSH_PROVENANCE**를 채택했다. 차단0개이며 receipt는 `/home/et16/xadb-dev2-evidence/pf04-flush-provenance-20261025/final-review.json`, SHA-256 `362b9a2b538301d9ad7de828e15d565dd5edc735e5b7461b8e97abe3dbce6f73`다. 원자료 산술·source/manifest·수집기/문서의 이번 범위만 수용하며 원 PF04·DEV2 인수는 아니다. 채택 뒤 상태·receipt 메타데이터만 갱신했다. 계획 검토와 출력 보완 계획 검토는 각 선행 commit의 receipt를 따른다.

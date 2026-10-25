# DEV2 PF04 P3 영속 I/O 원인 관측

2026-10-25 · **도구 구현·선택 관측 완료 / 요청 전체 비율 INCONCLUSIVE / Always 영향 screen FAIL / 독립 D1/D2 한정 검토 채택**

[계획](../plans/dev2-pf04-resolution-plan.md#p3-io-cause) · [기계 판독 기록](dev2-pf04-io-cause-20261025.json) · [구현 상태](../implementation/dev2-implementation.md#dev2-pf04-io-cause-20261025) · [P2 후보 기각](dev2-pf04-k0-owner-20261025.md)

## 범위와 결론

사용자의 후속 진행·계획 선행 commit·단계별 commit 지시에 따라 `360961b`로 P3 계획을 먼저 커밋했다. `/root`가 작성·실행·통합하며 `/root/pf04_evidence_review`가 독립 검토한다. 제품은 `5253caa`의 clean export를 사용했다. 병행 DEV3 변경은 측정에 포함하지 않았고, 이번 제품 코드·WAL 의미 변경은0이다. 재부팅·서비스 재시작·sysctl/governor/mount 변경도0이다.

현재 네 관측은 전부 원 scalar·ACK membership·실제 source/restore byte 검산을 통과했다. 추적한 두 실행에서 각각512개 WAL fsync가 정확한 원 측정 경계 안에 대응했다. fsync가 원 경과 시간의 약99.65–99.70%이고, 그 fsync 시간의 약99.04–99.29%가 `jbd2_log_wait_commit`과 겹친다. **이번 추적 안에서는 journal 완료 대기가 지배적**이다. 이것은 원 PF04 네 실패의 재현·원인 확정이나 순수 장치 service time 입증이 아니다.

block 요청 전체 대응은 미완성이다. 시작에 연결되지 않은 완료515/525개를 거절해 **요청 겹침 비율을 null/INCONCLUSIVE로 보존**했다. Always의 추적/control p99 screen도 실패했다. 이 한계를 없애려고 반복 실행하거나 이상 행을 제거하지 않았다. PF04 네 FAIL·DEV2/B2/D0 INCOMPLETE는 유지한다.

## 구현과 관측 경계

- [수집기](../../scripts/dev2_io_trace.py)는 P1 Runner를 상속한다. 일반 사용자 worker를 stdin 앞에서 대기시키고 전체 BPF attach 뒤에만 비차단 payload를 보낸다. P1의 HELLO/START/STOP·실제 PMU gate와 원 workload를 유지한다. DONE은 먼저 worker에 전달한 뒤 BPF를 닫는다.
- [root supervisor](../../scripts/dev2_io_supervisor.py)는 고정 명령만 실행한다. root 소유·비쓰기 경로의 sudo/python3/setpriv/bpftrace identity를 검사·기록하고, 별도 session의 직접 BPF child를 unreaped 상태와 pidfd로 소유한다. control pipe STOP/CANCEL/EOF, 독립55초 deadline, bounded TERM/KILL/reap와 원자료 보존을 처리한다. setpriv PDEATHSIG 뒤 고정 child wrapper가 expected PPID를 검사하므로 설정 전 부모 사망도 거절한다. worker는 일반 UID1000·기존 capability를 유지한다.
- [BPF](../../scripts/dev2_io_trace.bt)는15개 probe로 worker ext4 sync·journal wait, 대상 filesystem journal phase, 대상 disk의 raw issue/complete/requeue를 수집한다. 개별 완료와 NVMe batch 완료를 모두 지나는 공통 `block_rq_complete`를 사용한다. `count()`와 종료 후 저장 행 수를 정확히 대조한다. map8192·raw64MiB·event200,000행과 시간 한도를 적용한다. disable ACK 뒤1초 drain은 CPU barrier를 뜻하지 않는다.
- [분석기](../../scripts/dev2_io_analysis.py)는 cross-CPU 출력 순서를 신뢰하지 않고 timestamp·요청 시작 세대로 재구성한다. clip/union으로 중복 합산을 피하고, pointer 재사용·동일시각 모호성·고아/중복·오류·partial generation·requeue·경계 미완료·count/loss/warning을 보수적으로 다룬다. journal phase는 수집·개수 검증만 했으며 transaction별 phase와 요청의 귀속 분석은 하지 않았다.

모든 시간은 같은 boot의 host CLOCK_MONOTONIC과 BPF `nsecs(monotonic)`이다. 초기 time namespace 일치와 monotonic/boottime offset0을 보존했다. `/dev/nvme1n1p2` ext4는 시스템과 journal을 공유하고, 물리 요청 필터는 whole disk259:6이다. 다른 디렉터리로 옮기는 것을 격리라고 하지 않는다. BPF raw는 private `/dev/shm`에 쓰며 제품 fixture는 ext4에 둔다. worker는 CPU2를 직접 보고했고 정답 checker가 확인했다. runner는 `taskset -c 3`으로 실행했으며 원 호출을 `execution-provenance.json`에 기록했다. 개별 수집기의 affinity snapshot은 보존하지 않았고 CPU3 상속은 실행 구성의 범위다. sibling18도 유휴 관측에 포함했다.

## 실제 관측

seed20261018·128행·warmup64·측정512를 줄이지 않았다. 순서는 Off/control→Off/trace→Always/trace→Always/control로 동결했으며 재시도0이다. 각 slot 직전10초 CPU2/18 및 disk busy는 모두5% 미만이었다. 네 실행 모두 실제6개 PMU counter가 표시 coverage100%를 보고했다. 이 표시값이 multiplexing 부재를 증명하지는 않는다.

| slot | 모드 | BPF | 원 elapsed(ms) | worker CPU(ms) | durable cycle p99(ms) | 정답 |
| --- | --- | --- | ---: | ---: | ---: | --- |
| 0 | Off | 꺼짐 | 2719.346386 | 19.809002 | 5.883019 | PASS |
| 1 | Off | 켜짐 | 2728.408963 | 20.471185 | 5.807436 | PASS |
| 2 | Always | 켜짐 | 2749.110769 | 24.306714 | 10.441849 | PASS |
| 3 | Always | 꺼짐 | 2693.737704 | 18.063863 | 5.417841 | PASS |

| 추적 모드 | fsync pair | fsync 합(ms) | journal wait 겹침(ms) | fsync 중 wait | 대응된0byte FLUSH | 미연결 완료 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Off | 512 | 2720.199439 | 2694.181002 | 99.0435% | 515 | 515 |
| Always | 512 | 2739.366808 | 2719.999901 | 99.2930% | 525 | 525 |

대응된 요청 수는 Off2070·Always2119다. FLUSH는 이 부분 집합의 수이며 전체 비중으로 바꾸지 않는다. 측정 밖 미완전 요청74개씩(고아 완료73·열린 요청1)과 측정 밖 열린 fsync/wait도 원 count에 보존된다. 전체 저장 event 수와 kernel event count는 일치했다.

동결 coarse screen은 elapsed/p99≤1.25배·CPU≤2배다. Off의 trace/control 비율은 각각1.00333/0.98715/1.03343로 PASS다. Always는1.02056/1.92731/1.34560이며 **p99 FAIL**이다. 모드당 한 쌍이므로 screen PASS도 안정적인 overhead 추정이 아니다. Always 추적 비중을 무추적 비중으로 일반화하지 않는다. 무추적 Always CPU가 이번 Off보다 작은 사실 역시 원 CPU 비용 실패가 해결됐다는 증거가 아니다.

## 미연결 완료의 구체 단서와 다음 작업

측정 안 고아 완료는 모두 `error=0,nr_bytes=0,__data_len=0`이며, 직전 같은 pointer에4096byte 정상 완료가 있다. 적용 kernel `blk-flush.c`의 `REQ_FSEQ_DONE`은 data 완료 이후 `blk_flush_restore_request`→`blk_mq_end_request`를 다시 호출할 수 있다. 이 **논리 flush 순서 종료와 일치하는 패턴**이지만 현재 raw에는 completion의 `rq_flags`·flush state·end_io provenance가 없어 실제 분류를 확정할 수 없다. 행을 삭제하거나 정상 물리 완료로 합치지 않는다.

후속 P3 관측은 다음 순서로 구체화한다. 제품 성능 시험의 반복부터 시작하지 않는다.

1. 실행 kernel의 `RQF_FLUSH_SEQ`, `end_io`, flush state 전환을 대조하고 ISSUE/COMPLETE에 필요한 provenance를 추가한다. 물리 요청 완료·논리 flush 종료·실제 고아를 구분하는 독립 vector와 오류 경계를 먼저 동결한다.
2. wait transaction ID와 journal phase를 연결하는 분석을 추가한다. 공유 journal의 시간 겹침을 WAL 소유권으로 바꾸지 않는다. 이번 Always 영향 실패도 유지하고 새 수집기 변경에 필요한 제한 연결 예산을 따로 정한다.
3. metadata 동기화가 제품에서 줄일 수 있는 원인으로 확인된 뒤에만 WAL 공간 준비/data-sync 후보의 복구·형식 계약을 작성한다. 현재 자료만으로 fsync/barrier를 약화하거나 P4를 자동 채택하지 않는다. 원 CPU 한도 실패의 설명도 별도로 남는다.

Kernel 근거는 보존 공식 Linux7.0 source와 Ubuntu7.0.0-38.38 patch의 해당 파일 전체 적용본, matching 설치 header, 현재 BTF이다. 최초 exact-prefix 검색이 Ubuntu block patch를 놓친 메타데이터는 `kernel-inputs-original.json`으로 보존하고 정정했다. `blk-mq.c`의 개별·batch 완료 모두 공통 tracepoint를 호출하며, NVMe pci batch 경로도 대조했다. [bpftrace0.25 공식 언어 문서](https://bpftrace.org/docs/release_025/language)의 typed rawtracepoint 문법과 실제 `-lv` 결과를 사용했다. 배포 binary의 재현 빌드를 주장하지 않는다.

## 변경별 검증·한계·보존

[새 시험](../../scripts/test_dev2_io_trace.py)의 unit 고유16함수·source별18회와 native owner6개를 선택 검증했다. r2의 신규10개 뒤 partial generation/launch 경계의 r4 네 개, 비차단 payload의 r5 두 개, close 실패 정리의 r7 두 개만 선택했다. 이미 완료한 함수2개를 다시 실행한 것은 각각 관련 코드 수정 뒤였으며 무변경 하위 회귀는0이다. 원 r1 PYTHONPATH 누락 import 실패는 제품 시험 실행으로 세지 않고 로그를 보존했다.

native owner의 r3 정상·준비 실패·취소·runner EOF 네 개는 해당 source의 PASS다. 독립 검토로 보완한 root supervisor 강제 사망·실제55초 watchdog은 r6 신규 두 개로 확인했다. 앞선 네 개를 최신 source 전체 PASS로 재표기하지 않는다. 이후 close 예외 수정은 주입 단위2개와 실제4slot 연결에서 확인했다. test wrapper·전체 workspace·기존 완료 P1/P2 제품 시험은 실행하지 않았다. 문서 검사는 push gate에 남기고 push하지 않았다.

외부4slot driver의 idle 검사에는 missing CPU/음수 delta 거절이 빠져 있었다. 실행 시작 뒤 지적되어 실행 파일과 원 결과를 바꾸지 않았다. `postcheck.py`와 독립 검토가 보존된 네 window 모두 정확 cpu2/18·8필드·비음수 delta·양수 total/단조 시간·비음수 disk delta와5% 조건을 확인했다. 실제 조건은 충족했지만 driver가 일반적으로 잘못된 입력을 거절한다고 주장하지 않는다. 후속 driver에서는 이 검사를 실행 전에 강제해야 한다.

원자료 루트는 `/home/et16/xadb-dev2-evidence/pf04-io-cause-20261025`다. manifest246개 파일의 SHA-256은 `d502b27af2d75cea3e82b53db2f2c7272c3d99da246c27efb1ba4c7a31fe5141`다. baseline source tar·실행 ELF·kernel 입력·source-r2/r3/r6/r7·실패와 선택 로그·4slot raw와 fixture·PMU·정답·분석·screen·정리 receipt를 결합한다. 중간 r4/r5 전체 tree는 별도 보관하지 않았고 관련 source/선택 함수와 로그를 유지한다. 현재 source 전수 검증으로 확대하지 않는다.

계획 독립 receipt SHA-256은 `6f97ce39986fdac220452212dfbe71ef901b281c4059126556436b5bc57107a1`이며 원 projection에 한정한다. helper 경계2개 추가와 최종 구현·증거 검토는 별도 후속 범위다. 최종 D1/D2 receipt는 `/home/et16/xadb-dev2-evidence/pf04-io-cause-20261025/p3-final-review.json`이며 SHA-256은 `24fa9b16cc8b09af9afab1dfb1e1d1e3028d9996500f9a33d116a92fa917d0da`다. **PASS_SCOPED_D1_D2_P3_OBSERVATION**을 채택했고 차단 지적은0개다. 채택 뒤 상태·receipt 메타데이터만 갱신했다. 요청 전체 비율·Always 영향·원 PF04 실패와 P4 미착수는 유지한다.

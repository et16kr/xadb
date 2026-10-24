# COMM slow-peer 실패 수정

2026-10-24 · **COMM-FIX-003 / D1 시험 fixture·D2 압력 oracle / 선택 단위·두 실패 조합 native 검증 완료, 최종 독립 D1/D2 수용 / C1 INCOMPLETE**

[COMM 계약](../contracts/comm-design.md) · [시험 계획](../plans/comm-implementation-plan.md#slow-pressure-fix) · [현재 구현](../implementation/comm-implementation.md) · [원 실패](comm-performance-batch-20261023.md#extra-hour-20261023)

## 범위·결정

사용자가 COMM 시험 실패 확인·해결을 요청했다. 작업·계약·통합 담당은 `/root`, 별도 검토자는 `/root/comm_fix_review`다. 최근 미해결 cell128(Unix·16연결·64KiB slow ECHO)의5초 정체와 cell90(TCP IPv4·1연결·빈frame slow ECHO)의 압력 미도달을 수정 대상으로 삼는다. 이전 모델 oracle 수정·중단과 전체 fault/model 미실행은 별도 잔여 의무다. 원128/90 FAIL·90 부분5mode·과거 source별360/74/60mode를 보존하며 전체 C1을 승격하지 않는다.

`COMM-FIX-003`의 D2 대안은 prefix/timeout 완화, 소켓 buffer 확대, readiness 힌트의 제한된 재확인이다. 최소prefix와 부하를 유지하는 마지막 방식을 선택한다. Unix slow owner가 활성·시작·미정지·미완료prefix일 때 실제 WRITE 이벤트가 없으면20ms 간격으로 최대1회 nonblocking send를 시도한다. 일반 peer의5초 무진행, slow 최소prefix8(기존Unix1MiB6), 원burst cap·25초 pause·TX4/RX2·quota64MiB·5초 준비/30초 측정·취소1초·135조합·5+5 정책은 유지한다.

원 `would_block>0`은 RX/TX 합계이며 readiness가 write를 막으면 실제 TX 정체에서도0일 수 있다. 새 `continuous-full-tx-no-byte-progress-v1`은 Open·실제 pending ticket4·미송신 RX backlog가 연속 유지되며 native `written_bytes`가500,000,000ns 이상 증가하지 않은 구간을 요구한다. byte 진행·닫힘·포화 해제는 구간을 초기화하고 peak만 보존한다. 기존 backlog peak·ticket saturation·queue bytes 조건도 유지한다. 순간 full queue나 RX WouldBlock만으로는 통과하지 못한다. 과거90의 어느 계수가 부족했는지는 원자료 유실로 **미확정**이며 새 관측을 과거 원인 확정으로 소급하지 않는다. WouldBlock은 계속 진단에 기록하며 별도 실제 short-I/O/WouldBlock fault 의무는 그대로다.

실패 receipt는 ready/PID·수신한 peer/closed/cancel 응답·중간 peer별 offset/prefix·socket 설정·bounded stdout tail·reap 여부를 보존한다. closed가 없으면 cleanup ledger는 UNKNOWN이며 kill/reap를 ledger0으로 바꾸지 않는다. 새 증거 누락은 PASS가 아니다. 재검토 조건은 incomplete prefix 수락, 정상 byte 진행의 압력 오판, 닫힘 시간 누적, false PASS, 실패 원자료 누락, 완료 사례의 불필요한 반복이다.

## 원자료와 선택 검증

로컬 artifact는 `artifacts/comm/failure-fix-20261024/`다. 최초 기준은 `848d6ccfc7083e70cfc7301b3d55deaec49d95da`의 export이며 이번 소유3파일만 적용해 동시 engine 변경을 제외했다. 저장소 디스크 부족으로 임시 build/source는 `/tmp/xadb-comm-failure-fix-20261024-in7aq01s/`를 사용한다. source·선택·명령·log identity는 `candidate-r2-selection.json`과 후속 실행 receipt를 따른다.

| 구분 | 선택·결과 |
| --- | --- |
| 원 실패128 진단 | 원worker/source에 독립 진단 helper를 사용해 같은5초 정체를 재현. slow peer는 정확7full frame·offset0·slow_write_blocked_since 미설정, healthy15는 계속 진행. `diagnostic/cell-128/diagnostic.json`에 상태 보존 |
| 원 실패90 진단 | 원worker/source의 해당 실패 mode1회는 PASS. 단일 진단이며5쌍 인수·원FAIL 해소로 세지 않음 |
| 최초 fmt | 새 worker의 포맷 check FAIL. `initial-fmt.log`를 보존하고 서식만 수정 |
| r2 최초 준비 | 디스크 ENOSPC로 사본 복사 실패. 뒤 시험 명령은 시작하지 못해 빈log·exit120/101. PASS가 아님. 이번 불완전 사본만 제거, r1은 압축 보존 후 임시 영역으로 이동 |
| Python 선택 | 신규4method와 수정된 실패 경로의 실제 dummy timeout/kill/reap/Unix cleanup 기존1method, 합계5 PASS. `selected-python-r2-started.log` |
| Rust 선택 | 연속500ms 경계·byte진행 reset·닫힘/포화 해제·계속 쓰는 full queue의 위양성 거절, 신규3method PASS. `selected-rust-r2-started.log` |

Python controls는 실제 Unix socketpair에서 writable 힌트가 없는데 짧은 쓰기가 가능한 경계를 재현했다. probe의20ms 간격·중복 event 방지, prefix7 거절·500ms 직전 거절·backlog/queue/stall 누락·과거worker의 새증거 누락 거절, closed receipt 보존과 미완결 peer 상태 보존을 확인했다. 기존 무변경 제품/모델/codec·workspace 회귀는 실행하지 않았다.

## Native와 독립 검토

새 source에서 실패cell128/90만 기존5+5 실행기로 선택한다. 각 mode의 원측정·취소·정리·정답 조건을 유지하고 추가 필요 시 처음5쌍 포함 전체10쌍으로 판정한다. 2026-10-23 추가 시험의 마감은 당시 이력으로 보존하며 이번2026-10-24 수정 검증을 그 과거 deadline에 소급하지 않는다. 이번 batch 예산은 실행 전 별도 plan에 동결하며8시간 미만이다.

최종 native 결과와 독립 검토는 아래 후속 기록으로 확정한다. 문서 자동 검사는 push gate에 남긴다.

### 실행 전 독립 검토

`/root/comm_fix_review`가 live/동결3파일의 SHA와 원선택 로그를 직접 대조해 r2의 D2 코드·controls를 제한 수용했다. prefix/5초 보존·20ms cadence·실제 pending/byte진행 관측·부정시험에 실행 전 차단 지적이 없다. `peer_states_at_failure.closed`는 오류 뒤 finally 정리를 거친 snapshot이므로 오류 직전의 닫힘 상태로 해석하지 않는다. Clippy는 관련 worker bin/test target을 경고 없이 검사했으며 `clippy-r2.log`에 보존했다.

### r2 실제 실패와 TCP 추가 수정

r2 source `1e335cd68972cdec5a8e568dbc72e98f4d328b532efda8df27b6524d70c126c9`의 첫 batch는8attempt/7completed 뒤 **FAIL**이다. Unix128의 pair1–2 네mode, TCP90의 pair1 두mode와pair2 Detailed만 PASS이며 완결 조합 PASS는 아니다. 원 `native-r2-result.json`·`native-r2.log`와 `performance-batch-h5018c2x`를 보존한다.

실패90 pair2 Always의 보강 receipt는 offered50,286<cap65,536·worker received/flushed12,228·TX peak2slots·ticket saturation0·지속TX정체0·WouldBlock2를 기록했다. 이번에는 필수 포화가 실제 미도달했다. controller가0.5초 무진행을 영구burststop으로 바꾼 것은 확인되지만 kernel에 보낸50,286 중12,228만 worker가 받은 세부 원인은 이 관측만으로 확정하지 않는다. 원2026-10-23 실패90의 정확 계수도 계속 미확정이다.

r3는 **TCP slow owner만** 최소prefix 뒤 일시적인 client write pressure를 영구 종료로 처리하지 않고, readiness가 돌아오면 같은cap까지 제공을 재개한다. 새byte 진행 때 과거pressure 사유를 지워 종료 직전의 정체와 혼동하지 않는다. 계속 막히면 원25초 pause 종료에 닫히며, 최종 worker의0.5초 지속TX정체·backlog/queue·quota·취소·정리 조건은 그대로다. 일반/미완료prefix의5초 기준과 Unix r2 동작은 바꾸지 않는다.

새 `test_tcp_slow_offer_resumes_after_transient_write_pressure`는 scripted1초 정체 후 재개와 지속 정체의 두subcase다. 수정 전8!=16으로 **RED FAIL**, 수정 후 정한control cap16까지재개/지속정체8 뒤pause종료로 **GREEN PASS**다. cap16은 이 결정적control에만 쓰며 native의65,536과 측정 길이는 유지한다. green 뒤 Unix 예외 처리의 원try/except를 복구했으며 해당TCP제어는동일하여 완료한TCP control을 반복하지 않았다. 원로그는 `tcp-offer-{red,green}.log`, 최종 source와변경 범위는 `candidate-r3-selection.json`·`tcp-offer-r3.patch`다. r3 source는 `27e4c4ba3c2316a8a29c2a8dd46d5e1ae96ea1bbf8ff977f34196303ad12cb3e`다.

Unix는 r2의4개 완료mode를 원source/worker에 유지하고 미실행pair3–5부터만 이어 간다. TCP는 변경한r3의5쌍으로 판정하며 r2 부분3mode를 섞지 않는다. 어느 결과도 원mixedbatch FAIL을 덮지 않는다.

### r3 실패 보존·r4 단일 소켓 설정

r3의 cell90 batch도 **3attempt/2completed 뒤 FAIL**이다. pair2 Detailed의 offered50,597·worker received/flushed13,764·TX peak2slots·ticket saturation0·지속TX정체0을 `native-r3-result.json`과 `performance-batch-m0szuyf5`에 보존한다. TCP 제공 재개만으로 해결됐다고 판정하지 않는다.

`tcp_socket_probe.py`의 별도 진단1mode는 PASS였고8/16/24초 `ss` 관측을 보존했다. 이 자료의 양방향 kernel queue·재전송/backoff는 그 **PASS 실행**의 관측이며 실패한r3의 kernel 원인으로 귀속하지 않는다. worker의32개 bounded 상태 trace를 추가한 `tcp-diagnostic-source`도 준비했지만, 별도 검토의 단일 요인 분리 권고에 따라 **NOT_RUN**으로 보존했다. 새 제품 증거나 인수로 세지 않는다. 기존 driver/lease 정적 검토에서 구체적인 RX slot 누수·interest 영구 제거는 확인하지 못했다.

r4는 느린 **TCP controller 첫socket의 SO_SNDBUF 요청값64KiB만** connect 전에 고정한다. 실제 로컬 조회값은128KiB이며 원2,626,560byte 자동 설정과 구별한다. 원SO_RCVBUF, 일반peer, Unix, worker 소켓, TCP_NODELAY/MAXSEG, payload·burst cap·prefix·warmup/측정·pause·압력/정리 조건은 유지한다. 부하가 의도한 worker TX 압력까지 도달하도록 controller의 kernel backlog를 제한하는 D2 fixture 설정이다. 설정 성공이나 진단PASS만으로 원인의 일반성·인수를 주장하지 않는다.

신규 `test_only_slow_tcp_sender_uses_bounded_native_send_buffer`는 실제 IPv4/IPv6에서 수정 전 두family의 **RED FAIL**, 수정 후 **GREEN PASS**다. 느린sender의실제131,072byte와일반peer의기존큰buffer·TCP_NODELAY0을확인한다. r4 source는 `02c557338f0fafeebfaba98a5add5ef6af589eb0291a0f3ee57d313bb94c3fdd`; `candidate-r4-selection.json`에소유hash와범위를보존한다. 새native는변경영향의실패cell90만5쌍으로실행하며r2/r3부분결과는합산하지않는다.

### Unix r2 완결

`resume_unix.py`는 기존4개 receipt·latency gzip·source/binary hash와완전한pair1–2를검산한뒤pair3–5만실행했다. 신규controls4method가완료prefix비반복·추가필요시전체10쌍·중단FAIL·누락/중복/다른신원거절을검증했고별도검토자가실행전수용했다. `unix-r2-continuation-config.json`·`resume-controls.log`·`unix-r2-continuation/`에보존한다. 원mixedbatch FAIL은그대로다.

Unix128은동일r2 source/binary의기존4+신규6=10mode로 **BATCH_PASS/5쌍, 추가0**다. 처리량하락의판정상한0.1797033%, p99상승의판정상한0.1131317%로원10%/15% 한도이하다. 완결조합범위만수용하며최종별도검토·raw정리대조는아래종료기록을따른다.

### r4 실패와 분리 진단

r4 cell90은5attempt/4completed 뒤 pair3 Always에서 **FAIL**이다. offered6,519·worker received/flushed5,617·TX peak2slots·ticket saturation0·지속TX정체0이며 작은송신buffer만으로 해결되지 않았다. 원r4의4개PASS와FAIL을그대로보존한다. 실패의read_bytes235,928은5,617×42보다14byte많아 미완결frame이남았으며, 이조건에서는제품의can_read가true여야한다. 이는RXslot포화로읽기를멈췄다는추측과구별한다.

앞서NOT_RUN으로남겼던별도r3기반diagnostic source `b3e8fa9d16c7e5a2dae72cbc72ea1417e5e7eac4e7c25ce45bd7573d81f577a8`를추가관측용으로빌드해고정3mode를선택했다. worker의최대32회/1초간격phase·interest·queue·nativebyte관측과8/16/24초ss를함께보존하며, 진단을5쌍인수로합산하지않는다. 이진단은r4와다른source·소켓설정임을유지한다.

고정진단3회는1PASS·2FAIL이다. 실패attempt2의6–24초관측은Open·readable=true·writable=false·RX/TXslots0·pending/tickets0·read573,431·written573,426으로고정됐다. 같은실행ss의workerRecv-Q는0이고clientSend-Q는1,725,150byte·재전송backoff5였다. attempt3도같은종류의정체를기록한다. 제품이읽을데이터를내부queue때문에거절한상태와구별되며, kernel수준의정확한결함또는원2026-10-23의원인까지확정한자료는아니다. `tcp-deep-diagnostic/`에source/binaryidentity·각receipt·ss를보존한다.

### r5 협상 설정과 선택 검증

독립 검토 후 보존된 r3를 기준으로 느린 TCP 첫 socket에만 connect 전 `TCP_MAXSEG=1460`을 요청했다. r4의 SO_SNDBUF 제한은 효과가 충분하지 않아 제거했고, 원 실패·단위 PASS를 보존한다. 대안인 worker 전용 socket buffer API 추가보다 시험 peer의 협상 설정이 작고 국소적이다. MSS 실측을 receipt에 추가했으며 NODELAY·일반 peer·Unix·worker 코드는 유지한다. MSS에 따라 Linux의 자동 송신 buffer도 달라지므로 packet 크기만의 독립 물리 효과나 특정 kernel 결함을 확정하지 않는다.

신규 MSS control은 실제 IPv4/IPv6 양끝에서 협상된 `0<MSS<=1460`, 일반 연결의 원 큰 MSS, 명시적 SO_SNDBUF override 부재와 NODELAY0을 검산한다. 원코드 RED는 양 family의 MSS가 각각32,741/32,732로 실패했다. 첫 수정 후 실행은 자동 송신 buffer가131,072보다 클 것이라는 잘못된 시험 가정 때문에 FAIL했다(실측87,040). 이 원 FAIL을 보존하고, 자동값 추정 대신 실제 setsockopt 호출 부재를 검산하도록 control을 교정하여 PASS했다. `tcp-mss-red.log`·`tcp-mss-green.log`·`tcp-mss-green-corrected.log`를 구분한다.

r5 source는 `a641ac70a0f59c67d79b4d9a1136ace3f8b2d88f0e389ee1b217c99bb24bc635`이며 `candidate-r5-selection.json`에 세 소유 파일의 hash와 선택 범위를 고정했다. `/root/comm_fix_review`는 live/frozen SHA와 실제 control 로그를 대조해 코드·D2를 제한 수용했다. 실패했던 cell90만 원5쌍·5초 준비/30초 측정·25초 pause·0.5초 실제 TX 정체·quota·취소/정리 기준으로 검증한다. r2/r3/r4 부분 PASS를 r5와 합산하지 않는다. 이전 source에서 완료한 Unix 조합은 반복하지 않는다.

## 최종 실행·보존

| 수정 대상 | source별 완결 결과 | 실제 압력·정리 |
| --- | --- | --- |
| cell128 Unix·16연결·64KiB slow ECHO | r2 동일 source/binary의 기존4+신규6mode, 5쌍 **BATCH_PASS**, 추가0 | 최소 지속TX정체24.889639초, 최대취소2.066894ms, peak payload1,442,716byte, 전mode cleanup0 |
| cell90 TCP IPv4·1연결·빈frame slow ECHO | r5의10mode, 5쌍 **BATCH_PASS**, 추가0 | 최소 지속TX정체24.369946초, 최대취소2.369418ms, peak payload252byte, 전mode cleanup0 |

TCP90의 healthy peer가0이므로 처리량/p99 비교는 원계약에 따라 **N/A**다. 이를 성능 비용 PASS로 해석하지 않는다. 필수 실제 TX 압력·prefix·quota·취소·정리 조건은 전부 검산했다. 실제 MSS는1,448byte였고 명시적인 송신 buffer override는 없다. Unix128의 처리량/p99 판정 상한은 앞서 기록한0.1797033%/0.1131317%다.

최종 코드의 관련 단위 검증은 source별 Python7method(신규·수정6+실패정리 기존1), Rust3method가 PASS다. 기각된 r4 socket control의 PASS와 잘못된 MSS control의 FAIL은 별도 이력으로 유지한다. Unix 이어가기 artifact helper의 신규4 controls도 별도로 PASS했다. r2 이후 Rust 코드는 같은 hash이며 이미 완료한 Rust 시험·fmt·Clippy를 반복하지 않았다. 전체 wrapper·무변경 제품 회귀·문서 검증은 실행하지 않았다.

`close_evidence.py`는 시험을 실행하지 않고 원자료를 읽어 `retained-native-r2`/`r3`/`r4`/`r5`에 각 source archive·worker binary·manifest·receipt·latency gzip을 복사했다. 원 r2는8attempt/7completed FAIL, r3는3/2 FAIL, r4는5/4 FAIL, r5는10/10 BATCH_PASS다. 원 receipt의 경로·source 식별을 고쳐 쓰지 않았다. 별도 진단 source/binary와1PASS·2FAIL도 `tcp-deep-diagnostic/`에 보존했다.

`final-audit.json`은 두 완결 조합의 각각10개, 총20개 고유 cell/pair/mode, source/binary 단일성, latency gzip hash, 원5/30/25초, 압력·취소·cleanup0·PID소멸·Unix경로소멸 및 live 소유3파일의 최종 SHA 일치를 확인한다. 압축 source는 원실행 사본이며 임시 build 경로에만 의존하지 않는다. 이는 읽기·보존 검산이고 새 제품 시험 실행이 아니다. 최초 audit가 아직 열려 있던 자기 stdout log의 hash를 먼저 읽은 사실은 `final-audit-preclose.json`에 보존했다. 종료 뒤 그 log 한 개의 hash만 확정했으며 제품 receipt는 변경하지 않았다. `review-history.json`은 구현 담당이 독립 검토 메시지를 옮긴 기록이며 검토자 서명 receipt로 주장하지 않는다.

최종 독립 검토 결과는 아래와 같다. **전체 COMM/C1·잔여 모델/전체 native fault/미완료 성능 inventory는 INCOMPLETE**다. 이번 완료는 요청한 두 최근 실패 조합의 수정·선택 검증 범위다.

### 최종 독립 수용

`/root/comm_fix_review`는 시험 재실행·파일 수정 없이 최종 D1/D2를 **차단 지적 없이 수용**했다. r2–r5와 진단 source archive의 path/length/content를 직접 검산해 원 fingerprint와 대조하고 binary·원/보존 receipt·gzip·로그 hash를 확인했다. r5의 고유10mode, 실제 TX4/backlog/500ms 정체, 원시간·prefix/cap·quota·취소·cleanup0·PID소멸, MSS1,448과 helper 측정 제외·TP/p99 N/A를 수용했다. Unix r2의 원4+신규6mode 및 독립 통계 검산 수용도 유지한다.

두 완결 조합은 source를 섞거나 유리한 실행만 고른 결과가 아니다. r2/r3/r4 FAIL과 진단1PASS·2FAIL을 원본대로 유지했으며, final audit의 최초 open-log hash와 종료 뒤 정정도 확인했다. 최종 소유3파일은 검증 source와 같다. 검토의 비차단 문구 정정인 “각각10개, 총20개 고유 cell/pair/mode”를 반영했다. `/root`가 계약·통합 담당으로 이 제한 범위의 수정과 증거를 수용한다. 전체 COMM/C1 승격은 없다.

## 2026-10-25 산출물 정리 후 보존 위치

사용자의 정리 요청에 따라 중복 `/tmp` 사본과 재생성 가능한 Cargo cache를 정리했다. 원 source·binary·receipt와 임시 파일별 보존 대응은 [산출물 정리 기록](comm-artifact-cleanup-20261025.md)을 따른다. 위 원 실행 경로·실패·최종 수용 판정은 소급 변경하지 않았다.

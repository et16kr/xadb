# DEV2 PF04 원인 분석과 해결 후보

2026-10-25 · **기존 자료 재분석 / 해결 후보 PROPOSED / 제품 변경·신규 시험·추적 0 / PF04 FAIL·DEV2 미완료 유지**

[기계 판독 기록](dev2-pf04-analysis-20261025.json) · [기존 성능 실행](../implementation/dev2-implementation.md#dev2-performance-followup-20261020) · [계측 계약](../contracts/ai-performance-engineering.md) · [기존 가설 검토](dev2-pf04-hypothesis-review-xadb-20261020-r1.json)

## 범위와 판단

사용자가 남은 DEV2 시험의 원인 분석과 해결 방법 검토를 요청했다. 작업·통합 담당은 `/root`, 독립 검토자는 `/root/pf04_evidence_review`다. 성능 원인 해석과 후속 대안 검토를 D2로 분류한다. 이번 기록은 원인 후보의 근거와 후속 순서를 제안하며, 제품 수정안·새 측정 profile의 채택이나 실행 완료를 뜻하지 않는다.

핵심 판단은 **K0의 실제 계측 비용, K0 실행 간 처리 속도 변화, 영속 경로의 tail 변동을 분리해야 한다**는 것이다. 구체적인 코드 비용과 대기 위치는 확인되지만, 기존 네 campaign 실패를 모두 설명하는 단일 원인은 확정하지 못했다. WAL 변경이나 환경 탓으로 일괄 결론내리지 않는다.

원 e3 소스의 K0 audit/replay 80개 stdout을 SHA-256 확인 후 해독해 경과·CPU·지연 표본을 재계산했다. 독립 검토자는 같은 K0 80개와 영속 audit/replay/Off-Off 60개 보존 output의 지문·시간·p99를 별도 확인했고 journal 분석 512행을 재집계했다. 두 검토자의 K0 분모는 중복이며 160개로 합산하지 않는다. 기존 정답 PASS는 원 source·scope의 기록이며 이번에 제품 정답 시험을 다시 실행하지 않았다. perf/BPF binary와 기계어 대응은 기존 독립 검토를 참조하며 이번 신규 재해독으로 세지 않는다.

직접 대조한 현재 10개 파일 중 diagnostics·substrate·WAL·durable observation·CPU observer·두 worker·K0 controller·영속 checker의 9개는 e3와 동일하다. `platform/src/namespace.rs`는 FILEMAP/DWB 이름 예약과 열린 파일 metadata API가 추가됐지만 기존 `sync_all` 전달은 같다. 전체 현재 소스나 현재 실행물의 검증을 주장하지 않는다. 정확한 경로·지문과 외부 보존 자료는 JSON을 따른다.

## 재계산한 실패와 실행 상태

원 기준은 각 pair의 경과·CPU·외부 시간 최대1.15배, 표본 p99 최대1.25배, RSS 증가4MiB다. 아래는 표시용 근삿값이며 판정은 원 정수값·기준을 보존한다. `pair` 번호는 원자료의 0부터 시작하는 번호다.

| 비교 | 남은 원 실패 | 해석 |
| --- | --- | --- |
| K0 Off/Always audit | pair5: 경과1.25555배, CPU1.25350배, 표본 p99 1.27888배; 외부 시간도 초과 | 중앙값만 보고 PASS할 수 없음 |
| K0 Off/Detailed audit | pair0/2/6 경과1.26035/1.27803/1.30046배, CPU1.25924/1.27650/1.29887배; pair2/6 p99와 관련 외부 시간 초과 | 같은 모드에서도 실행 간 처리 속도가 달라짐 |
| 영속 Off/Always audit | pair0/1/4 p99 1.59411/1.26776/1.84375배 | 전체 경과 중앙 비율은0.9976이나 tail 기준은 실패 |
| 영속 bundle 재현 | pair2/3 p99 1.85083/2.27437배, pair1/9 CPU1.20089/1.19741배 | tail과 CPU 실패를 별도 보존; 전체 경과 중앙 비율1.0019 |
| 영속 Off/Off 대조 | pair8 p99 12,389,622/5,480,140ns = 2.26082배 | 계측을 켜는 것이 큰 tail 변동의 필요조건은 아님. 앞선 CPU 실패의 원인 증거는 아님 |

### K0: 보통의 추가 비용과 실행 간 변화

기존 별도 재현의 경과 시간 paired 중앙 비율은 Always1.0934, Detailed1.1258이다. 실제로 계측 비용이 있으며 Detailed는15% 경과/CPU 한도에 여유가 작다. 이 재현 PASS로 원 audit FAIL을 지우지 않는다.

새로 정리한 원 표본의 특징은 p99 몇 개만 튄 것이 아니라 **각 실행의 p50도 다른 대역에 놓인다**는 점이다.

| 모드 | 원 audit의 실행별 표본 p50 주요 대역 | 관련 경과 시간 주요 대역 |
| --- | --- | --- |
| Off | 약240ns 또는280–281ns | 약14.7ms 또는16.8–16.9ms |
| Always | 약251–260ns 또는300–301ns | 약16.0ms 또는18.4–18.7ms |
| Detailed | 약261ns 또는310–311ns | 약16.5ms 또는18.9–19.1ms |

Always 실패 pair5는 빠른 Off와 느린 Always가, Detailed 실패 pair0/2/6도 빠른 Off와 느린 Detailed가 비교됐다. 후보 실행의 앞/뒤 절반 p50도 대부분 같아, 해당 실행 전반의 속도 변화가 관측된다. 반대 조합에서는 계측 On이 Off보다 빨라 보이는 pair도 있다. 이 사실은 처리 속도 변화가 계측 비용 비율에 섞임을 보여 주지만 CPU 주파수·전력 상태·cache·배치 중 무엇 때문인지는 확정하지 않는다.

현재 worker는 warmup4,096회 후65,536회를 측정하고16회마다 지연을 표본화한다. 원 측정 구간은 약15–19ms다. campaign 전10초 CPU2/18·장치 유휴 확인은 측정 중 상태를 보장하지 않는다. 이전 PMU 진단은 worker 전체를 덮으며 당시 실패 실행의 내부 구간과 일치하지 않는다. 정상 PMU 값으로 원 실패를 설명하거나 짧은 warmup을 확정 원인으로 선언하지 않는다.

### K0: 코드에서 확인되는 추가 작업

[worker step](../../crates/testing/engine-worker/src/performance.rs)은 매 반복마다 Replace→Snapshot→Read를 수행한다. [계측 구현](../../crates/engine/src/diagnostics.rs)의 `started/completed`는 각 성공 연산에서 포화 원자 counter를 한 번씩 갱신한다. 따라서 정상 측정 구간에 **393,216회(65,536×3×2)의 추가 counter 갱신 호출**이 있다. CAS 재시도·하드웨어 명령 수의 실측은 아니다.

Detailed는 완료196,608건 중 용량256건을 기록하고 나머지196,352건을 drop counter에 반영한다. 이미 끝난 PF12가 포화 큐의 lock 시도와 별도 sequence RMW를 줄였다. 이를 새 해결책으로 다시 제시하지 않는다. 남은 counter 갱신은 정확한 작업량·drop·포화 공개 계약과 연결되므로 단순 삭제나 wrap 가능한 `fetch_add` 치환은 적절하지 않다.

[Snapshot 생성](../../crates/engine/src/substrate.rs)은 기존 `Counters` Arc와 별도로 활성 diagnostics collector의 Arc도 clone한다. 측정 구간65,536개의 snapshot이 만드는 추가 clone/drop 쌍은 구조상 확인된다. 다만 이 비용을 분리 측정한 적은 없고, 제거했을 때 원 실패가 해결된다는 근거는 없다. 공통 Counters 구조는 transaction·recovery의 크기·quota에도 영향을 주므로 K0만의 소유권 대안부터 검토한다.

## 영속 경로: 확인된 대기와 남은 원인

[WAL append](../../crates/engine/src/durable/wal.rs)는 각 변경 거래의 frame을 파일 끝에 쓰고 `sync_all` 성공 뒤 진행한다. [Linux 파일 구현](../../crates/platform/src/namespace.rs)은 실제 파일 sync로 전달한다. [영속 worker](../../crates/testing/engine-worker/src/durable_performance.rs)는 앞선 commit과 읽기 확인이 끝난 뒤 다음 반복을 시작한다. 512개 측정 거래를 동시에 제출하지 않는다.

기존4개 fsync 추적에서는 cycle 시간의97.352–98.538%가 fsync 중 worker 깨움 이전 대기였다. 후속 journal 추적·설치 커널 binary 대응은 metadata 버퍼와 commit record 버퍼 대기를 구분했다. 다만 후속 추적의 영향·환경 확인 조건은 **FAIL**이다. 그 실행의 대기 비율을 원 인수 실행에 옮기거나 물리 NVMe service time으로 해석할 수 없다. 장치 flush/FUA, 장치 전력 상태, 다른 journal 작업, scheduler 중 어느 항목이 원 실패를 만들었는지는 미확정이다.

영속 replay의 CPU pair1/9 실패도 별도로 남는다. Off/Off tail 변동은 CPU 초과의 설명이 아니며, 해당 CPU 구간의 원인 관측이 더 필요하다. sync 대기를 줄이는 변경이 양쪽 실행에 같이 적용되더라도 PF04의 상대 비용 한도가 자동 해결되는 것은 아니다.

## 해결 후보와 우선순위 — 실행 전 제안

| 순서·후보 | 구현·조사 내용 | 선택 조건·검증 의무 |
| --- | --- | --- |
| 1. 측정 구간의 원인 관측 보완 | XADBTest에서 worker·source·측정 시작/끝을 연결하고 해당 구간의 CPU cycles/reference cycles·instructions·전환/경합 관측을 얻는 제한 진단 도구 설계. 지원되지 않는 지표는 missing으로 유지 | 원래 실패 구간에 없는 자료의 한계를 해결하는 목적. 수집 영향·시계·구간 결합부터 검증. 기존 profile·판정은 그대로 보존하며 새 profile이 필요하면 별도 D2 기록으로 동결 |
| 2. K0 계측 소유권 비용 절감 후보 | snapshot이 두 소유 handle을 복제하는 구조를 좁은 K0 수명 안에서 줄일 수 있는지 설계. counter 갱신 비용도 실제 경로별로 구분 | clone/drop 감소를 성능 개선으로 미리 계산하지 않음. snapshot/cursor가 store보다 오래 사는 수명, 동시 read/drain, drop·포화·순서, Off, row quota와 별도 진단 예산을 보존할 수 있을 때만 구현 후보 채택 |
| 3. 영속 측정의 저장 장치·journal 영향 구분 | 독립적인 측정 저장소 조건을 확인하고 WAL fsync→journal transaction→block 요청의 완료·FLUSH/FUA를 연결하는 제한 관측 설계 | 같은 공유 filesystem의 다른 디렉터리만으로 분리됐다고 하지 않음. 장치 서비스·queue·재개 대기를 구별하고 원 CPU 실패도 별도로 다룸. 원자료·추적 영향 실패를 보존 |
| 4. WAL 공간 선확보와 data sync 조합 | metadata 갱신 비용이 유의미하다는 근거가 생길 때 고정 공간 준비·data sync 경계를 검토 | 파일 길이/유효 끝·tail 손상·quota·rotation·복구·backup 계약에 영향을 줌. 새 형식/의미 설계와 변경 관련 crash/ACK/fence 시험을 갖춘 후에만 채택. 지금 즉시 적용할 수정 아님 |

기존 추가 Arc 후보는 이전 탐색 예산 안에서 채택되지 않았다. 위 후보를 진행하더라도 종료한3후보 탐색에 네 번째 실행을 덧붙이지 않는다. 새 근거·목적·후보 상한·중단 규칙을 별도 고정하고 원 실패를 유지한다. 긴 측정이나 warmup 변경도 이미 얻은 결과의 사후 구제책으로 사용하지 않는다.

이번에는 코드 변경이 없으므로 제품 인수·성능·하위 회귀를 반복하지 않았다. 후속 도구/제품 구현 시 **실제 diff에 직접 대응하는 신규·변경 검증만** 선택한다. 이번 분석을 무변경 제품의 반복 시험 허용으로 확대하지 않는다.

## 공개 구현과 비교한 대안

PostgreSQL 로컬 checkout `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의 `src/backend/access/transam/xlog.c`를 읽었으며 작업 트리는 clean이었다. XLogFileInitInternal은 WAL segment의 공간 준비·sync를 하고, issue_xlog_fsync는 동기화 방법을 구분한다. XLogFlush는 다른 backend의 flush 결과를 재사용하는 group commit을 수행한다. XADB에 같은 기능이나 개선 효과가 이미 있다는 뜻은 아니다.

- **`fsync`를 `fdatasync`로 한 줄 바꾸기:** 파일 크기처럼 이후 읽기에 필요한 metadata는 data sync도 보존해야 한다. XADB의 계속 늘어나는 WAL에서 journal 대기가 사라진다는 보장은 없다. [Linux 공식 man-pages](https://man7.org/linux/man-pages/man2/fsync.2.html)
- **group commit:** 동시 거래에서는 검토할 수 있지만 현재 순차 시험은 다음 거래가 대기열에 없다. 이 PF04 fixture의 직접 해결책으로 선택하지 않는다. PostgreSQL도 지연을 추가하면 처리량과 거래 지연 사이의 tradeoff가 있다고 설명한다. [PostgreSQL WAL 설명](https://www.postgresql.org/docs/18/wal-configuration.html)
- **공유 ext4의 `max_batch_time=0`:** 기존 정확 버전 소스 검토에서 일반 fsync에 무조건 붙는 지연이라는 가설이 성립하지 않았다. 이번에도 설정 변경을 해결책으로 선택하지 않는다. barrier·sync 생략은 기존 ACK 내구성 보장을 보존하지 않으므로 후보가 아니다. [ext4 문서](https://docs.kernel.org/admin-guide/ext4.html)

## 독립 검토와 보존

독립 검토자는 원 K0/영속 출력과 journal 분석행을 재계산하고 기존 실패 수치를 확인했다. full-queue 최적화의 중복 제안, 추가 Arc의 효과 단정, 순차 부하에 대한 group commit 제안, data sync의 journal 회피 단정, Off/Off tail로 CPU 실패를 설명하는 오류를 검토했다. 최종 문서 검토 상태는 기계 판독 기록에 남긴다.

보조 자료는 `/home/et16/xadb-dev2-evidence/pf04-analysis-20261025/`에 원값·원 경로·SHA-256과 함께 보존했다. 수정은 본 분석 문서·기록·색인·현재 구현의 연결뿐이다. 제품 수정·시험/벤치마크·새 tracing·host 설정 변경·commit/push는 없다. 문서 자동 검사는 정책에 따라 push gate로 남긴다. PF04 네 campaign FAIL과 DEV2/B2/D0 미완료, PF12/PF13의 기존 한정 완료를 유지한다.

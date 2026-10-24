# DEV3 plan4 초기 page의 원 공유 버퍼 소비

계약 ID: **D3-PLAN4-CLEAN-OWNER-001**. [plan4 최초 설치](dev3-plan4-genesis.md)를 실제 `MutableStorageOwner`와 한 `SharedPool`에 연결하는 G5 범위다. 설계는 독립 D2에서 조건부 수용했으며 구현·정적 자원·효과 실행·최종 D1은 각각 실제 증거로 구분한다. 이 계약 작성은 시험 실행이나 DEV3 완료 증거가 아니다.

[source별 선택 증거](../evidence/dev3-plan4-clean-owner-20261024.md)의 필수78 결과와 최종 독립 D1을 이 clean 범위로 제한 수용했다. 원 compiler campaign HOLD·준비 peak FAIL을 보존하며 S5 추가128과 G6/G7은 미완료다.

[구현 계약](implementation-contracts.md), [저장 소유](dev3-storage-session.md), [DEV3 단계별 시험](../plans/dev3-test-plan.md), [AI 개발 효율 계획](../plans/dev3-ai-throughput-improvement.md)의 변경 기능 시험·원 실패 보존·실제 소비자 중심 개발을 적용한다. G5는 AI 실험의 일반 page/live 변경 소비자 완료로 세지 않는다. G6의 Aging/INIT/op4·dirty/flush·복구 owner와 G7의 Catalog CREATE·첫 행 거래는 후속 의무다.

## 소유권과 정상 동작

원 fresh plan4와 DWB 설치를 소비한 Mutable의 AVAILABLE→CLAIMED→BOUND 원 CAS를 사용한다. frame4..64, operation114688bytes를 caller에게 빌린다. frame0은 canonical Allocation0, 나머지는 payload용이다. 새 pool/map/CAS나 writer 추출을 만들지 않는다. 원4MiB DWB loan을 한 번 take하여 private하게 보관하고, G5에서 사용하지 않는 FlushIo를 만들지 않는다. 후속 consuming activation은 원 선택과 이 loan을 이동해야 한다.

plan4 payload는 정확8개다. Root1/Jobs2..6은 Object4, Catalog anchor7/leaf8은 Object5이며 incarnation·owner generation1/level0/LSN0이다. allocator는 원 full canonical page/digest로 fixed9+Free55를 검산한다. owner/kind만 맞는 다른 주소나 Free page9..63을 허용하지 않는다.

`Plan4Target`은 private full `TargetRequest`를 감싼 기대값이다. identity/kind/level 관측은 가능하지만 pin·load·publication 권한이 아니다. 값이 bound보다 오래 살아도 실제 사용 시 원 namespace/정확 membership을 재검산한다. `Plan4ResidencyCursor`, slot index와 `FrameStamp`도 관측일 뿐이다.

- `load_into`는 clean old slot을 canonical selected home으로 교체한다. 이미 resident인 target은 AlreadyResident다. 실패한 load는 old bytes/등록/세대를 보존한다.
- `residency_step`은 최대63개 payload slot을 유한하게 검산한다. exact resident hit는 loaded=false, missing target은 재사용 가능한 빈 slot을 우선하고 없으면 clean resident를 교체한다. 각 후보 종류에서는 cursor부터 처음 만난 slot을 유지하며 선택 후에도 전체 검산을 끝낸다. cursor는 성공 뒤만 이동한다.
- Catalog 조회는 allocator0/anchor7/leaf8의 실제3-pin read 안에서 anchor/root/schema와 빈 leaf를 검산한다. 유효 ObjectId의8-byte big-endian key를 actual `lower_bound`에 전달하여 NotFound를 반환한다.
- 정리 큐 census는 `{0,1,2,3}`→`{0,1,4,5}`→`{0,1,6}`의4/4/3-pin 창을 읽는다. 매 창의 전체 pins를 반환한 뒤 다음 load를 수행한다. actual64개 job cell을 ordinal 순서로 hash하고 root512bytes·revision1·Free64·mask·aggregate·job digest를 대조한다. root512와 incremental hash/count만 유지하며 전체 job snapshot을 복제하지 않는다.

두 소비자는 첫 prepare/load 전에 actual `Owner::check_binding`으로 원 core와 Idle 상태를 확인한다. private prepare-set은 동일 exclusive borrow에서 full required identity/중복/capacity를 검산하고 필요한 resident를 후보에서 제외한다. 새로 적재한 required도 다음 scan에서 보존하며 준비가 끝난 뒤 whole-set pin을 한 번 얻는다. I/O 중 부분 pin이나 content/DPT/map guard를 유지하지 않는다.

## 디렉터리와 설치 경계

원 directory에 별도 Plan4Genesis mode를 둔다. 기존 Status/Mixed의 `resident_payloads`·`operational_payloads`·accepted owner/kind 집합과 positive-LSN/counter1 설치 의미는 유지한다. 새 mode는 plan4 inc1에만 적용한다. snapshot의 공통 shape는 권한이 아니며 old Status와 새 plan4 install은 각자의 sealed permit만 소비한다.

모든 payload를 끝까지 검산하며 dirty 또는 non-genesis metadata가 하나라도 있으면 Invariant/fence다. 이 검사는 pair, exact hit, clean 후보, generationMAX skip보다 앞선다. 앞쪽 hit/clean 후보가 뒤쪽 손상이나 latch Busy를 숨기지 않는다. 직접 load도 같은 clean-only 전수 검산을 거친다.

required resident의 MAX generation은 그대로 hit/keep 가능하다. outside clean MAX는 교체 후보에서 제외한다. 실제 outside slot이 있으나 모두 MAX이면 CounterExhausted, outside 공집합이나 검산하지 못한 latch가 있으면 Busy다. 세대는 준비 시 소비하지 않고 최종 publication에서 checked successor 한 번만 발급한다.

새 sealed permit은 원 core/binding, allocator bytes/stamp/requirement, old slot snapshot/index/next generation, exact request와 immutable verified payload를 묶는다. 모든 실패 가능한 검산과 try-lock을 끝낸 뒤 byte copy/등록 교체를 수행한다. 설치 후 pageLSN0/counter0/recLSN None/dirty false/DPT 없음이다. 공통 `FrameContent::valid`를 완화하지 않는다.

## source 검산과 오류 효과

bind는 원 private BootstrapPlan을 빌려 기존 최초 설치 readback 정본을 사용한다. authoritative profile/XBS/FILEMAP과 opened/named identity, 같은 data handle의64page2pass, 끝의 source 재검산을 유지한다. 최종 allocator는 마지막 selected read 결과에서 가져온다. 임의 hash·외부 seed·새 plan factory를 받지 않는다.

load는 원 writer exclusion 아래 checked namespace에서 selected controls와 exact home을 두 번 읽고 각 canonical image를 검산·비교한다. 각 읽기의 opened/named identity·length와 마지막 source 검산, 원 Fresh/WAL이 보유한 header-only prefix·Idle·slot, 최종 allocator/slot snapshot을 대조한 뒤만 설치한다. WAL은 원 fresh provenance와 writer exclusion 아래 prefix를 재검산하며 disk header를 다시 읽는 recovery 진입이 아니다. 이는 임의 concurrent external writer의 snapshot 보장이 아니다.

단순 frame 길이/index/target 오류는 Unchanged다. nonempty/poisoned frame은 CAS 전에도 기존 corruption 정책대로 CallerMustResolve/fence이며 AVAILABLE 값을 재사용 권한으로 보지 않는다. CAS 이후 실패/unwind는 원 BindingClaim이 fence/ABANDONED로 끝낸다. bound drop도 sync·복구·재bind·loan 반환을 수행하지 않는다.

known/Unchanged의 retryable read 실패와 required EOF/invalid returned count/identity·length·content 손상을 구별하고 nested cause/effect/certainty를 보존한다. foreign Owner는 첫 I/O/RAM 교체 전에 거절하며 non-Idle은 원 concurrency 오류를 유지한다. 뒤쪽 창 실패 때 이미 완료한 앞쪽 clean load/eviction은 되감지 않는다. DB mutation0과 전체 RAM rollback을 혼동하지 않는다.

## 자원과 인수

제품 `fits`와 정적 시험은 actual memory/native Writer/File의 같은 source-value census 식을 사용한다. retained owner, bind 검산/construct/return phase, load,3/4-target set, Catalog, queue512/hash가 살아 있는 load/read phase를 구분한다. 큰 pool/Result의 move를 여러 phase에 동시에 살아 있는 값으로 합치거나 누락하지 않는다. caller frame backing·operation114688·DWB4194304는 별도다. local source cap65536에는 active phase의 원 storage와 bound SharedPool까지 포함한다. retained라는 이유로 이 값을 빼지 않으며 기존 owner/File/하위 경계 한도를 늘리지 않는다. source-value 판정은 machine stack/RSS 증명이 아니다.

신규 필수 inventory는 메모리75+static2+native1=78, S5 추가128, 전체206이다. compiler10은 별도다. 기존42raw/274720bytes를 immutable reference로 재사용하며 새 canonical raw/generator는 없다.7개 memory selector는 ID 단위 include/exclude와 원 완료 PASS 제외를 지원한다. S5 확대는 profile/frame/required 배치/home identity/LSN 및 native profile2/3으로 따로 선택하며 중간에는 실행하지 않는다.

정적2를 먼저 선택 검산하고 독립 자원 검토 후 신규 memory/native를 진행한다. actual fault op/name/offset/occurrence와 oracle, source/ELF/input SHA를 실행 전 동결한다. partial PASS/FAIL/NOT_RUN과 실패 DB/log는 원 source에 보존한다. native는 실제 한 PID의 fresh setup→원 bind→clean 소비이며 재시작·repair·flush 인수로 확대하지 않는다. 문서 검증기는 push 직전 gate다.

# DEV3 plan4 빈 거래의 실제 live 소비

계약 ID: **D3-RETENTION-LIVE-001**. [빈 거래·정리 큐 의미 처리](dev3-empty-retention-semantics.md)를 원 `MutableStorageOwner`와 한 `SharedPool`에 연결한다. [wire 결정](../../artifacts/dev3/plan4-g6-empty-retention-wire-decision-20261024.md)의 `0561d2c1fc40daeacf65d530b245932a8daf4b213e9476ca1df72a48023e3ae2`를 따른다. 이 단위는 fresh epoch1의 N=0 거래다. 복구된 base·새 epoch·loser 정리·실제3PID 인수와 Catalog CREATE/CLR·첫 행 ACK는 후속 단위다.

## 원 자원과 실제 실행

`Plan4Pages::into_empty_retention`은 원 CAS binding, WAL runtime, writer lease, 공통 fence와 pool을 소비하여 유지한다. 다른 owner/core와의 결합을 거절한다. caller는 operation114,688byte, 원 DWB workspace4MiB, control196,608byte와 replay1,572,864byte를 빌린다. 새로운 pool·독립 dirty registry·합성 WAL requirement를 만들지 않는다.

allocator0, Root1, Jobs2..6, Catalog7/8, Status9..63의 닫힌 주소 집합을 사용한다. Status reserve→INIT 또는 cancel은 원 allocation claim과 publication 경로를 쓴다. 실제 INIT 뒤에도 동일 directory에서 Root·Job·Status를 필요한 집합으로 준비하고 pin한다. LSN0인 clean 초기 page는 변경 counter0, 양수 LSN의 clean page는 양수 counter로 설치한다. 다른 dirty page의 DPT를 지우지 않는다.

AgingStart→Begin→RollbackStart→AbortEnd→Finish의 실제 frame을 stage하고 원 pool의 페이지·metadata·DPT를 설치한다. guard와 pin을 반환한 뒤 원 requirement를 durable로 만든다. 발급 local과 채택 local을 구별하며 Begin만 이전 채택 local을 의미 검산에 전달한다. 전송·설치 실패 뒤의 fence와 미해결 obligation은 성공으로 덮어쓰지 않는다.

원 파일의 정체성·header·선택 prefix를 확인하고, prefix 전후 hash와 canonical replay를 대조한다. ATT·Status·Job의 양방향 관계 및 실제 descriptor row를 확인한다. fresh 전체 N=0 수명은 reserve/INIT을 포함한7frame250,448byte이고 header 포함254,544byte, 마지막 LSN7이다. 이 정규 예시가 다른 실행의 고정 길이나 모든 복구 형태를 뜻하지 않는다.

## Flush와 재적재

flush는 capture 당시의 P@L와 원 WAL requirement를 고정한다. L 이후 RAM에 새 frame이 설치되어도 source 검산 범위를 임의로 늘리지 않는다. durability→canonical source→DWB install→home sync→DWB retirement→조건부 RAM finalize 순서를 지킨다. concurrent writer가 페이지를 다시 변경하면 finalize는 최신 dirty와 recLSN을 남긴다. retirement 뒤 content가 Busy이면 claim을 보유하고 I/O 없이 finalize를 이어 간다.

최신 home을 읽을 때는 실제 allocator membership, 두 번의 physical read, canonical image, allocator snapshot과 prefix 안정성을 확인한 뒤 같은 directory에 공개한다. decoder의 성공만으로 load·flush 권한을 만들지 않는다. 빈 Catalog 조회도 원 pool의 필요한 target 집합을 사용한다.

## Prefix 관측과 권한 경계

`retention_frame_length`와 `scan_retention_prefix`는 tag3를 포함한 envelope의 길이·SHA-256 header/body digest·연결·visible header를 검산한다. legacy/tag2 진입점의 동작은 바꾸지 않는다. visible header의 오류를 불완전 tail로 숨기지 않는다. active EOF의 버림과 sealed EOF의 거절을 구별한다. 반환값은 관측이며 채택·복구·WAL requirement 생성 권한이 아니다.

## 자원·시험 선택

fresh 실제 memory/native 타입의 source-value 측정은 각각 peak63,520/63,656byte, metadata123,424/123,560byte, whole188,960/189,096byte다. caller backing·machine stack·RSS와 구별한다. 원 static source 이후 단순 표현 수정, 시험용 훅과 lint 정리에 대한 독립 lineage 검토는 원 static 실행을 새 source의 재실행 PASS로 바꾸지 않는다. 복구된 owner의 추가 생존 값은 별도 계산한다.

중간 inventory는 live32(static2·memory30)와 신규 prefix43, 합75다. S5 고유 추가는 live32와 prefix237, 합269이며 최종 S5에는 필수75도 포함한다. 긴 실행·전체 조합은 최종 인수에 둔다. 완료 사례·무변경 하위 시험은 반복하지 않고 원 실패·중단·미실행을 보존한다.

시험은 독립 literal recipe와 보존 raw34파일550,160byte를 사용한다. 완료된 하위 시험 함수나 제품 encoder를 oracle로 호출하지 않는다. 동시 실행 검산은 실제 capture의 pin/latch 반환 뒤와 실제 RAM 설치 후·WAL force 전에 놓은 시험 전용 one-shot 훅을 쓴다. 훅은 정확 pool 주소에 결합하고 mutex를 놓은 뒤 유한 시간만 기다리며 일반 제품 layout을 바꾸지 않는다. 기존 `Copied`와 조건부 `before_wait` 훅을 이 목적에 잘못 사용했던 원 실패를 보존한다.

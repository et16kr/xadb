# DEV8 — 복제·백업·운영·성능 설계

2026-10-19 · **DEV8-DESIGN-002 / D2 / ACCEPTED·독립 수정본 검토 PASS / 구현·SC12–15·OPS1 NOT_RUN**

[구현·검증 계획](../plans/dev8-implementation-plan.md) · [조사와 검토](../evidence/dev8-source-research.md) · [단계 완료 계약](../plans/stage-completion.json) · [DEV3 저장 계약](disk-storage-design.md) · [DEV5 서버 계약](dev5-server-design.md)

[2차 검토](../evidence/dev8-review-002.md)는 적용 domain·sequence 발급·partition 이동·WAL/control 경계를 보완한다. 최초 설계의 검토 기록은 당시 범위로 보존한다.

<a id="scope"></a>
## 1. 범위와 기존 계약의 관계

DEV8은 디스크 기반 XADB의 복제·운영 복구·보안/관리·성능 인수를 구체화한다. R11–R15의 30개 필수 요구를 [배정표](../plans/dev8-implementation-plan.md#coverage)에 연결한다. Linux/Rust, 유한 buffer와 작업 메모리, 하위 계층부터의 구현, 독립 oracle, 현재 build의 반복 복구를 유지한다. 공개 DBMS와 논문의 관측은 설계 입력이며 동일 SQL·wire·알고리즘·성능 보증이 아니다.

DEV3-DESIGN-003의 **in-place page, 단일 ordered WAL, 영속 UndoRef, WAL-before-page, steal/no-force, CLR, fuzzy checkpoint, bounded recovery 예약**이 저장 정본이다. DEV2 private/COW·root-only journal과 과거 L1/L2 제안을 DEV8의 물리 복제나 온라인 백업에 적용하지 않는다. DEV3 offline exact-cut backup은 그대로 유지하고 온라인 backup·archive·PITR를 별도 capability로 추가한다. ARIES의 이력 재실행·CLR·media recovery 원칙을 참고하되 XADB의 독립 undo-store와 공간 예약은 자체 계약이다.

이번 사용자의 DEV8 설계 요청은 이 단계에 필요한 `D-RECOVERY`의 운영 복원과 `D-LOGGING`의 capture/restart·archive·보존·감사 설계를 구체화하는 근거다. 이 범위의 이전 보류를 본 결정으로 대체한다. DEV7 R10-03의 이종 변경 전달 내부와 R10-05의 일반 구독/변경 분석 상세는 자동 완료하지 않으며 기존 보류를 유지한다. 공통 capture 공급을 재사용할 수 있어도 별도 adapter/구독 의미와 인수는 남는다. kernel 실행·고객 SLA·외부 출시는 승인하지 않는다.

본 문서는 아키텍처와 불변식을 정한다. 계획의 D8-F01–F10은 실제 API·byte format·환경별 수치·provider 동결 의무다. 설계 채택, leaf 동결, 구현, 로컬 검증, OPS1 인수를 구분하며 지금 실행 상태는 모두 NOT_RUN이다.

<a id="ownership"></a>
## 2. 소유 경계와 하위 공급

| 소유 | 책임과 공급 | 금지할 결합 |
| --- | --- | --- |
| engine | WAL/undo·commit ticket·물리 redo·논리 변경 추출 port·snapshot cut·retention owner·원자 apply+진행점·backup/recovery | network callback이나 원격 응답을 commit latch 안에서 기다리지 않음; RP 구현 역참조 금지 |
| replication | topology·sender/receiver·capture spool·peer 상태·재접속·동기화·보호 대기·충돌 | storage 내부 page/undo를 직접 수정하지 않음 |
| server | 인증/권한·운영 job·설정 epoch·서비스 역할·승격 조립·client outcome | heartbeat만으로 writer 소유권을 부여하지 않음 |
| transport / platform | AXF1·bounded I/O와 TLS port / Linux socket·파일 sync·clock·fence provider adapter | 전송 성공을 거래 성공으로 해석하지 않음 |
| SQL / client / CLI | 운영 view·권한 의미·capability 협상·상태/진행률·재시도 범위 | 결과 불명 거래의 자동 재실행 금지 |
| XADBTest | 소유 process·fault·외부 ACK 이력·독립 상태 모델·복원 drill·원시 성능 자료 | production decoder/충돌 resolver를 정답 계산에 공유하지 않음 |

계획 package `xadb-replication → foundation/engine/transport`를 사용한다. OS와 provider는 server/worker 조립점에서 주입한다. server의 replication 조립을 위한 직접 의존 추가는 실제 구현 때 [저장소 정책](../policies/repository-build-plan.md)과 JSON·manifest를 함께 검토한다. 이번에는 crate·빈 경로·dependency를 만들지 않는다.

engine 공급 port의 의미는 다음과 같다. 이름은 역할이며 아직 Rust signature가 아니다.

- `durable_wal_cursor`: 인증된 DB/timeline/epoch의 연속 durable prefix와 그 prefix가 요구하는 versioned control dependency를 함께 공급한다. borrow는 bounded chunk이며 cursor의 재시작 pin과 별도다.
- `capture_cursor`: 같은 WAL의 논리 변경·부분 rollback·DDL·commit 결정을 해석한다. 이미 회수된 row를 나중에 조회하여 before/after 값을 추측하지 않는다.
- `snapshot_with_stream_pin`: snapshot C, schema/object generation, C 이후의 stream 재시작 권리를 **한 경계에서** 얻는다. snapshot 뒤에 pin을 등록하는 공백을 허용하지 않는다.
- `apply_origin_transaction`: row/index/catalog 효과와 적용 domain별 origin 중복 방지·진행점·digest를 한 local transaction으로 commit한다.
- `retention_handoff`: 새 durable closure 선택이 완료된 뒤에만 이전 owner의 pin을 전진한다. metadata 불명에는 old/new 합집합을 보존한다.
- `backup_view`와 `restore_to_cut`: buffer/page 검증·FILEMAP·WAL/undo·예약을 engine이 소유한다. 상위는 작업 상태와 byte 전송만 조립한다.
- `install_standby`와 `follow_redo`: 일반 restore와 다른 recovery mode다. source DB/timeline·LSN·ATT/UndoRef/예약을 유지하며 source WAL을 뒤따른다. 새 writer branch 선택·loser undo·사용자 write admission은 승격 전 금지한다.

<a id="identity"></a>
## 3. 정체성·위치·진행점

| 이름 | 의미 |
| --- | --- |
| DB identity / timeline | 같은 DB 계보와 복원·승격으로 갈라진 이력. parent timeline과 fork cut/digest를 함께 인증 |
| NodeId / writer epoch | 노드 정체성과 해당 timeline의 writer 세대. local 파일 lock은 원격 노드 fencing의 증명이 아님 |
| ConfigEpoch / SubscriptionId | topology·보호 peer·filter/schema 계약의 불변 세대와 구독 정체성 |
| ApplyDomainId / projection generation | 동일 target 객체 집합에 적용할 효과와 필터·schema 의존 집합의 정체성. 전송 연결/경로·보호 peer 변경과 별개이며 같은 원점 거래의 서로 다른 projection을 구별 |
| OriginCommit | `(origin DB, origin timeline, origin commit_seq)`와 transaction digest. source 간 commit_seq를 대소 비교하여 전역 순서를 만들지 않음 |
| LSN | 한 WAL 계보의 물리 위치. 다른 timeline의 숫자만 비교하지 않음 |
| recv / durable / applied | 수신, 재시작 가능한 영속 보관, 거래 효과 공개의 서로 다른 위치 |
| capture restart / archive complete | capture 상태를 복원할 위치 / 검증된 archive closure의 연속 끝. 읽은 위치·전송량과 구별 |

모든 durable cursor는 identity·format/profile·연속 prefix·내용 digest를 검증한다. 같은 identity와 위치의 다른 bytes는 corruption이고 성공 재시도로 숨기지 않는다. logical stream은 source commit_seq마다 transaction 또는 인증된 filtered/no-op 경계를 제공하여 빈 구간과 누락을 구별한다. 원점별 순서·dependency vector를 유지하며 다중 원점에 하나의 잘못된 전역 q를 부여하지 않는다.

DEV3의 FILEMAP·파일 길이·bootstrap control은 모두 WAL record 자체라고 가정할 수 없다. F01의 물리 stream은 control generation·이전 digest·적용 WAL 경계·필수 FileId/길이/format/key 참조를 인증하는 **cut별 dependency inventory**를 가진다. source는 dependency를 영속 선택한 뒤 이를 참조하는 WAL을 만들고, receiver는 해당 namespace/예약과 inventory를 검증·영속 설치한 뒤만 의존 WAL까지 durable ACK한다. archive도 같은 inventory를 보존한다. target의 redo·PITR는 선택 prefix에 결합된 세대만 사용하며, 나중 donor FILEMAP을 임의로 섞지 않는다. source CURRENT/WRITER/DWB의 raw selector·local lease·flush 진행점을 target의 복구 정본으로 복사하지 않는다. target은 자신의 redo/checkpoint/DWB control을 만들고 source의 계보·예약·파일 의미를 인계한다. 파일 생성/확장과 control 선택 사이 crash·누락 dependency·선택 불명에는 ACK/새 삭제를 막고 필요한 old/new closure를 보존한다.

<a id="topology"></a>
## 4. Topology와 구현 순서

| profile | 쓰기·복제 의미 | 구현/인수 순서 |
| --- | --- | --- |
| H1 physical HA | 한 writer, 동일 format/profile의 전체 DB physical WAL standby. async 또는 strict remote-durable 보호 | 첫 복제 기준선. 초기 standby는 recovery-only이며 임의 query를 허용하지 않음 |
| L1 logical subscriber | source 한 개, table/partition 선택, committed typed change apply, target 대상에 사용자 쓰기 금지 | H1과 engine capture 계약 후; 물리 standby 승격 능력과 별개 |
| M1 multi-writer | 각 노드가 자신의 local transaction을 commit. 원점·인과 의존·중복 제거·충돌 정지와 명시 해결 | 필수 후속 leaf. 분리된 쓰기 영역과 겹치는 쓰기 영역을 각각 인수 |

H1의 최초 보호 집합은 primary와 **지정한 한 synchronous standby**다. 추가 async standby는 구성 가능하지만 자동으로 보호 대상을 대체하지 않는다. 지정 standby 교체는 admission drain·기존 보호 cut의 이전·새 peer 영속 확인·ConfigEpoch 영속 전환을 거친다. quorum/자동 선출·공유 디스크·다중 log lane은 초기 전제가 아니다. 필요 시 별도 D2 설계와 partition 모델을 요구한다.

M1의 local SERIALIZABLE은 cluster 전체 SERIALIZABLE을 뜻하지 않는다. 각 노드의 local ACK는 해당 원점 commit의 보장이다. 비동기 다중 쓰기의 원격 충돌 가능성을 숨기지 않으며, H1 strict 보호와 M1의 global atomic commit을 같은 옵션으로 제공하지 않는다. 기능별 지원 조합은 F03에 고정한다. 단일 writer의 성공으로 R11-01 전체를 완료하지 않는다.

<a id="ack"></a>
## 5. 성공 응답과 보호 실패

H1에서 primary는 local WAL sync·engine 공개 후 remote 보호를 기다린다. 기다리는 동안 local transaction을 다시 rollback할 수 없으며 다른 local reader가 그 변경을 볼 수 있다. 보호 대기는 latch 밖 bounded ticket이다. 원격에서 commit record만 보관하고 그 이전 update·undo·file/control 의존성이 빠진 경우 durable ACK를 보내지 않는다.

| 요청 mode | 성공 조건 | 연결 단절·timeout·cancel |
| --- | --- | --- |
| LocalDurable / async | local committed·durable. 원격 도착·보존은 성공 조건이 아님 | ACK 유실은 client에게 Unknown. 원격 RPO=0을 약속하지 않음 |
| RemoteDurable / strict | local committed + ConfigEpoch의 지정 peer가 commit 끝까지 연속 WAL/필수 namespace를 durable 보관 | 성공으로 강등하지 않음. local outcome과 보호 상태를 분리해 반환하며 후속 strict write admission을 닫음 |

결과는 `local_outcome = NotCommitted / Committed / Unknown`, `protection = NotRequested / Met / NotMet / Unknown`, mode·ConfigEpoch·commit ticket를 포함한다. `NotMet`는 정해진 deadline까지 요구 조건을 확인하지 못했다는 뜻이며 remote에 bytes가 없다는 증명이 아니다. reply가 없으면 client는 local outcome도 Unknown으로 취급한다. local committed인 보호 실패를 statement rollback·취소 성공·일반 retriable error로 변환하지 않는다. ticket lookup은 권한 있는 운영 조회이고 업무 자동 재실행을 허용하는 idempotency 보장이 아니다.

DEV5/6의 현재 XSQ1 body를 조용히 확장하지 않는다. F02에서 신규 capability/profile·오류/metadata·C/JDBC/ODBC mapping을 동결하고 미지원 client는 거래 시작 전 strict mode를 거절한다. 읽기·자기 자원 정리·관리 복구는 보호 실패로 닫힌 새 write admission과 별도로 유지한다. 통신/디스크 UNKNOWN 뒤 engine fence는 기존 계약을 따른다. auto-degrade가 필요한 운영은 명시적인 새 ConfigEpoch·관측 가능한 mode 변경이며 strict 요청을 async 성공으로 바꾸지 않는다.

XA/2PC와 H1을 함께 제공할 때 보호 대상은 최종 COMMIT ACK만이 아니다. RM의 **PREPARED vote 이전에 prepared state·XID·lock/undo/resolve 예약 전체**를 지정 peer에 durable하게 보관한다. coordinator의 COMMIT decision도 외부 participant에 dispatch하기 전에 같은 보호를 충족한다. rollback/heuristic/forget·정상 receipt 회수처럼 원격 의무를 바꾸는 전이도 해당 외부 응답/효과 전에 필요한 durable closure를 보호한다. local PREPARED 뒤 remote wait 실패는 단순 abort나 READ_ONLY vote로 바꾸지 않고 in-doubt/protection 실패로 관리한다. 이 protected-XA profile과 실제 DEV7 RM/TM 공급이 없으면 HA 서비스에서 XA/2PC 진입을 첫 효과 전에 거절한다. async HA에서는 외부 거래 의무 유실이 가능한 promotion을 허용하지 않으며 관련 durable obligation을 가진 source/peer를 확보할 때까지 승격을 막는다.

<a id="capture"></a>
## 6. Capture·spool·수신·apply

중복 제거 key는 `(ApplyDomainId, projection generation, OriginCommit)`이다. payload는 원래 transaction digest와 선택된 효과의 canonical projection digest를 구별하고 SubscriptionId/ConfigEpoch가 승인한 domain을 인증한다. 같은 key·같은 두 digest의 재전송만 중복이며, 같은 key·다른 digest는 corruption이다. 한 source 거래를 서로 다른 disjoint domain으로 투영한 payload는 각각 적용한다. 같은 target 객체/키 공간에 겹치는 domain은 생성 전에 거절하고, 같은 domain으로 들어오는 여러 경로는 동일 projection·원점·digest를 보존하여 효과를 한 번만 적용한다. M1 순환 전달도 이 규칙을 따른다. 연결 재생성·peer 교체·일반 ConfigEpoch 변경은 ledger를 초기화하지 않는다. projection 변경은 새 snapshot/generation과 그 cut의 ledger·frontier를 원자 설치한 뒤 전환하며 기존 행에 과거 효과를 새 key로 재적용하지 않는다. domain 사이의 원자 공개는 보장하지 않으므로 함께 보일 의무가 있는 객체·제약은 하나의 domain에 배정한다.

target에는 **영속 선택된 domain/projection generation**의 payload만 apply할 수 있다. 세대 전환은 이전 apply의 drain/fence를 완료한 뒤 새 target generation·ledger·frontier를 원자 선택한다. 전환 전 queue·spool·재접속에서 늦게 도착한 이전 세대 payload는 `RetiredHistory`로 거절하며 새 세대로 이름을 바꾸거나 새 target에 적용하지 않는다. domain identity/generation을 재사용하지 않고 retired tombstone과 선택 상태를 restart 전에 복원한다.

M1의 domain/projection 교체·재동기화에는 apply 중지만으로 충분하지 않다. [충돌 해결](#synchronization)의 절차로 affected domain의 **모든 local writer도 drain/fence**하고, 원점별 cut·미전달 효과를 수집하여 완전한 canonical 상태와 새 frontier를 독립 대조한 뒤 설치한다. donor 한 곳의 snapshot C로 target의 C 이후 local commit을 덮어쓰지 않는다. 참여 원점/이력 확보나 writer 차단이 불가능하면 교체를 거절하며, 선택 완료 뒤 같은 새 세대로 writer/apply admission을 연다. L1의 source snapshot 절차만으로 이 M1 의무를 대체하지 않는다.

논리 추출은 SQL text 재실행이 아니라 결정된 typed key/before-version/after-value, row/object generation, schema epoch, origin과 rollback/commit 경계를 사용한다. 현재 DEV3 WAL만으로 필요한 값이 모두 존재한다고 가정하지 않는다. F01에서 engine 변경 시 **같은 WAL의 같은 원자 mini-transaction**에 필요한 logical supplement를 함께 기록한다. 별도 비원자 outbox나 commit 뒤 row 조회로 누락을 보완하지 않는다. 인덱스 split·aging·CLR 자체를 사용자 DML로 다시 송신하지 않는다.

supplement를 처음 켤 때는 기존 거래/DDL을 drain하고 profile·시작 cut을 영속 설치한 뒤 새 거래를 받는다. 필요한 옛 값이 없는 기존 WAL을 뒤늦게 완전 capture로 선언하지 않는다. snapshot C에서 아직 열린 거래는 C 이전 변경도 이후 commit에 포함될 수 있다. `snapshot_with_stream_pin`은 그 거래들의 decoder/spool 상태를 함께 넘기거나 **가장 이른 필요 WAL부터의 pin과 재구성 경로**를 제공해야 한다. 단순히 C+1에서 읽기 시작하는 경로는 금지한다.

Capture는 `Stopped → Recovering → Streaming → Backpressured / NeedsReseed / Failed` 상태를 가진다. 열린 거래의 조각·savepoint/statement rollback·schema 해석·LOB 부분 조립·prepared 상태를 추적한다. rollback된 변경은 commit payload에서 제거하며 PREPARE는 COMMIT으로 내보내지 않는다. raw WAL을 더 읽었다는 이유로 restart pin을 전진하지 않는다.

spool 인계 순서는 다음과 같다.

1. source durable prefix와 원래 retention pin을 고정한다.
2. 살아 있는 모든 열린 거래·decoder/schema 상태, 완료했으나 미확인인 origin payload, 다음 읽을 위치를 bounded segment에 기록한다.
3. payload·파일 namespace를 sync하고 checkpoint manifest를 영속 선택한다.
4. 재시작 closure를 읽어 검증한 뒤 source pin을 전진한다. 이전 fallback을 유지하면 그것이 요구하는 파일도 유지한다.

완료한 거래의 extent는 다른 장기 거래와 독립적으로 회수한다. compaction은 old/new 합집합과 임시 공간을 예약하고 durable 선택 후 해제한다. source WAL을 회수한 뒤 spool이 손상되면 삭제된 WAL로 되돌아가는 성공 경로를 만들지 않는다. 남은 backup/archive closure로 복구하거나 `NeedsReseed`로 정지한다. logical spool은 physical HA의 source WAL 보관을 대신하지 않는다.

receiver는 bounded fragment를 검증·spool하며 gap·다른 timeline·허용하지 않은 profile을 apply 전에 거절한다. physical ACK는 재개 metadata와 prefix/dependency의 영속화 이후, logical apply ACK는 **row/index/catalog 변경 + domain별 OriginCommit ledger + 적용 frontier**가 한 local commit으로 영속화된 이후다. ledger만 먼저 전진하거나 효과만 commit하는 순서는 금지한다. 동일 domain/projection의 OriginCommit/digest 재수신은 재적용하지 않고, digest 충돌은 채널을 정지한다. 전달은 재전송 가능하며 보장 대상은 effect의 원자적 중복 방지다.

초기 apply는 source별 직렬 순서다. 병렬 decode는 허용하되 source commit 공개 순서는 유지한다. 병렬 apply는 read/write·unique/FK·schema/sequence·origin dependency의 충돌 그래프와 동등성 시험을 갖춘 후 추가한다. apply 중 OOM·취소·restart에도 이미 영속 적용한 frontier를 되돌리거나 미완료 거래의 부분 효과를 공개하지 않는다.

origin ledger는 domain/projection/원점별 bounded disk 소유다. 해당 domain의 모든 허용 전달 경로와 durable resume floor를 합의하고 그 이전 재전송이 불필요함을 확인한 후 prefix를 retired frontier로 압축한다. 보존 digest가 없는 retired 구간의 개별 replay는 성공 중복으로 응답하지 않고 `RetiredHistory`로 거절한다. sender의 필요한 위치가 retired floor보다 앞이면 검증한 공통 checkpoint를 협상하거나 reseed한다. 동일 domain/projection의 OriginCommit/digest 성공 중복 처리는 검증 가능한 retained 구간에만 적용한다. checkpoint/retired floor 인계와 원점 tombstone·삭제/재삽입 version도 같은 durable closure에 포함한다.

<a id="synchronization"></a>
## 7. 초기 동기화·schema·충돌

전체 물리 초기화는 검증된 base backup + 그 consistency cut 이후 연속 WAL을 사용한다. logical 초기화는 engine의 `snapshot_with_stream_pin`으로 C를 얻고 snapshot 행·schema·원점 frontier를 임시 target generation에 streaming 적재한다. C 이후 stream을 spool하고 완전 검증 뒤 target generation과 시작 frontier를 원자 선택한 다음 catch-up한다. source의 현재 metadata를 snapshot C의 metadata와 섞지 않는다. 복사 중 crash는 manifest로 재개하거나 소유 staging만 폐기하고 기존 target을 보존한다.

**H1 seed/restart는 `install_standby`/`follow_redo`다.** backup의 redo 시작점부터 consistency end까지 먼저 재구성하고 이후 source durable WAL을 따른다. source DB/timeline·pageLSN·active/prepared ATT/UndoRef는 보존하고 target NodeId·receiver control만 별도 설치한다. donor의 local writer lease/lock을 복사하지 않는다. 일반 `restore_to_cut`의 새 timeline/loser undo를 호출하지 않는다. 예를 들어 base copy에 포함된 active T를 undo한 뒤 source COMMIT(T)만 받는 경로는 금지한다. standby 재시작도 redo-follow mode를 영속 인식하고 source의 아직 active인 거래를 loser로 처리하지 않는다. 승격 때만 source cut을 닫고 §8 절차로 전환한다.

logical row/object identity는 origin namespace와 source generation을 target local identity에 매핑한다. before-version은 origin commit/논리 row version·인과 metadata이며 source TxnId/pageLSN/물리 RowId를 target의 local 값과 직접 비교하지 않는다. mapping·version·tombstone은 row 효과와 원자 갱신한다. 같은 값을 다시 쓴 거래, delete 후 같은 key 재삽입도 version으로 구별한다. 물리 origin ID를 전역 고유 target locator로 캐스팅하지 않는다.

부분 table/partition 구독은 filter 세대를 고정한다. 하나의 source 거래에서 선택된 모든 효과는 target에서도 한 transaction이다. 제외한 객체에 걸친 FK·trigger·routine·sequence·타입 의존이 있으면 closure를 포함하거나 구독 생성 전에 명시 거절한다. 자동으로 제약을 끄지 않는다. filter 변경은 새 snapshot/ConfigEpoch로 수행하며 진행 중 거래에 소급 적용하지 않는다.

partition key UPDATE는 변경 전·후 membership을 같은 filter/schema 세대로 판정한다. `제외→제외`는 인증된 no-op, `제외→포함`은 완전 after-value를 가진 projected INSERT, `포함→제외`는 before-version을 검사하는 projected DELETE, `포함→포함`은 UPDATE/원자 이동이다. source의 물리 DELETE+INSERT를 별개의 논리 거래로 보내지 않는다. 진입하는 행의 target 부재와 tombstone/version을 검사하고 source에 before-row가 있었다는 이유로 target UPDATE를 강제하지 않는다. membership·진입 after-image를 재구성할 supplement가 없으면 해당 구독을 사전 거절한다. 같은 거래 안의 여러 이동·rollback을 반영한 선택 효과를 한 번에 적용하며 partition/PART/NG의 실제 공급과 함께 인수한다.

DDL은 catalog event와 schema barrier로 순서화한다. target은 해당 schema epoch의 의미를 설치하기 전 DML을 적용하지 않는다. generation이 다른 재생성 객체는 새 객체다. target은 이미 실행된 source trigger·default·sequence·외부 routine을 다시 실행하지 않는다. apply API는 결정된 효과와 제약을 검사하는 별도 권한 경계다. sequence의 비거래적 할당/예약 high-water는 재사용 방지를 위한 별도 durable event이고 재전송해도 새 값을 발급하지 않는다. LOB/spatial/PSM·PART/NG·XA profile은 실제 DEV7/하위 공급을 인수한 뒤 연결한다.

**sequence 발급은 COMMIT 보호만으로 충분하지 않다.** H1에서 NEXTVAL 값이 result·부분 fetch·routine/외부 effect로 나가기 전에 해당 범위/epoch·non-cycle 사용 구간을 지정 보호 peer에 durable하게 보관한다. 이미 보호된 cache 범위 안의 발급은 매 값마다 원격 왕복을 요구하지 않는다. local 예약 뒤 보호 실패에는 값을 공개하지 않고 예약을 소비된 것으로 보존하며, rollback/cancel이 그 범위를 반환하지 않는다. async 거래 ACK도 이 별도 발급 보호를 면제하지 않는다. 이 공급이 없으면 HA에서 NEXTVAL을 첫 값 공개 전에 거절한다. 기존 async 이력의 발급 범위가 불명이면 값 재사용 가능성이 있는 sequence admission을 승격 뒤에도 닫고, 이전 발급자 fencing과 검증된 예약 이력을 확보한 뒤만 연다. M1에서 같은 sequence를 여러 writer가 사용하려면 단일 발급 소유자 또는 영속적으로 분리한 non-cycle 범위/stride를 사전 배정해야 하며 local high-water의 단순 최댓값 병합으로 대체하지 않는다. peer 교체·ALTER·범위 재배정은 미소진 cache·발급 권한을 drain/fence하고 보호 정보를 인계한다.

M1에서는 origin을 보존해 순환 전달을 제거하고 dependency vector가 충족될 때만 apply한다. 동일 key의 concurrent update/delete, PK/unique/FK 위반, missing before-version과 schema 불일치를 구별한다. 기본 정책은 **해당 구독의 전체 transaction 적용을 중단하고 충돌 기록을 남기는 것**이다. 마지막 도착 우선·시계 우선 덮어쓰기를 기본으로 하지 않는다. 무관한 노드의 local write는 계속될 수 있으므로 convergence를 이미 달성했다고 보고하지 않는다.

충돌 해결은 식별한 conflict domain의 모든 writer를 drain/fence하고 각 원점 cut을 수집한 뒤, 권한자가 선택한 canonical 결과와 resolution ID·입력 digest·새 ConfigEpoch를 영속 결정한다. 모든 affected replica를 그 결과로 재동기화·독립 비교한 후 같은 epoch로 admission을 연다. 참여 노드가 없거나 새 변경이 cut을 넘으면 해결 완료를 거절한다. 초기 구현은 이 보수적 절차를 제공하며 자동 merge rule은 별도 leaf로 검증한다. 다중 원점 운영이 전역 원자 거래라는 주장은 하지 않는다.

<a id="failover"></a>
## 8. 역할 전환·fencing·client

`Replica → CatchingUp → PromotionPending → Recovering → Primary`와 `Primary → Draining → Fenced`를 관리 상태로 둔다. heartbeat는 장애 의심 신호이고 승격 권한이 아니다. 초기 failover는 운영자 시작 + 검증 가능한 외부 fence provider다. provider는 이전 writer의 프로세스/재시작 권한 또는 저장 쓰기를 실제 차단해야 한다. 연결 끊김·PID 파일·새 노드의 epoch 증가만으로 fence 증명을 만들지 않는다. 증명 불가이면 가용성을 포기하고 PromotionPending에 머문다.

계획 전환은 source admission drain → 기존 거래의 확정/UNKNOWN 해결 → terminal cut·보호 정보 sync → candidate durable catch-up → 이전 writer 차단 → 새 epoch/timeline과 parent cut 영속 선택 → redo/loser undo·무결성·예약 복원 → readiness 순서다. 비계획 전환은 먼저 외부 fence를 확인한다. strict H1은 **지정된 보호 peer와 인증된 ConfigEpoch 이력**에서만 zero-ACK-loss 승격을 허용한다. source가 사라졌다고 더 뒤처진 async peer를 대신 선택하지 않는다. 지정 peer도 상실하면 복구 대기 또는 별도로 명시한 손실 허용 전환이며 strict 성공이 아니다.

ACK를 받은 거래는 candidate의 연속 durable prefix에 있어야 한다. async 전환은 source ACK high-water와 candidate cut을 비교해 알려진 손실·불명 구간을 보고한다. source의 마지막 ACK를 확인할 수 없으면 정확한 손실량을 0으로 표시하지 않는다. fence 전에 확정된 거래의 지연 reply는 old epoch/ticket로 해석하며, fence 이후 새 writer 권한 없이 시도한 거래에는 성공 ACK를 만들지 않는다.

standby의 반복 redo는 source의 CLR을 적용하지만 source가 아직 active인 거래를 임의 undo하지 않는다. 승격 cut을 영속 선택한 후 source suffix 수신을 닫고 그 cut의 loser만 새 branch에서 undo한다. prepared/in-doubt는 DEV7 상태를 보존하고 임의 abort하지 않는다. 복원된 lock/예약과 resolve lane이 제공되지 않으면 승격을 거절한다. failback은 구 primary의 divergent suffix를 자동 병합하지 않고 fencing 상태에서 재구축/재동기화한다.

client 주소 전환은 새 연결의 DB/timeline/role·인증·profile을 검증한다. 기존 transaction·cursor·prepared handle을 새 primary로 옮기지 않는다. 이미 보낸 요청의 응답 유실은 Unknown이며 자동 재전송하지 않는다. topology 알림과 connection retry는 거래 retry와 분리한다. DEV6의 client별 실제 경로를 D8-T11에서 인수한다.

승격 완료와 strict-write readiness는 별개다. 두 노드 중 유일한 standby가 primary가 되면 보호 peer가 자동으로 생기지 않는다. 새 지정 peer가 승격 branch의 WAL/control·XA/sequence 의무를 인계받고 ConfigEpoch가 영속 선택될 때까지 strict 쓰기와 새 sequence 범위 발급을 열지 않는다. async로 전환하려면 기존 명시 mode 변경 절차를 따른다.

sequence 인계에서는 이전 writer에 예약했던 범위 전체를 소비된 것으로 처리한다. 마지막 반환값이나 cache 잔여량을 추측해 범위 일부를 새 writer에게 재대여하지 않는다. 새 발급은 그 범위 밖의 보호된 예약에서 시작한다. 정상 peer 교체에서 동일 writer가 cache를 계속 쓰는 경우에도 새 peer가 그 예약 전체를 인계받아야 한다.

<a id="retention"></a>
## 9. 로그 보존·공간 압력·복구 진행

retention owner는 local redo, current/previous checkpoint, undo/status/allocator closure, capture checkpoint, physical peer restart, archive, active backup, restore job과 prepared 거래다. undo page가 durable하다는 사실은 capture/archive 의무를 없애지 않는다. WAL floor는 같은 계보의 owner별 최소 필요 위치이며 파일·key·schema descriptor는 별도 참조 집합으로 관리한다.

cut별 control dependency history도 이 참조 집합에 포함한다. 최신 control로 정상 restart할 수 있다는 이유만으로 이전 backup·archive·peer가 필요로 하는 FILEMAP 세대를 삭제하지 않는다.

등록/해제·restart frontier·필수/선택 owner 구분을 durable catalog에 저장하고 **삭제보다 먼저 복원**한다. `pin 등록 → 읽기`, `새 closure sync/선택 → pin 전진`, `Retiring 등록 → lock 밖 unlink/dirsync`를 유지한다. owner drop은 권한 있는 영속 invalidation 후 수행한다. inactive·disconnect·시간 경과만으로 pin을 지우지 않는다. archive 삭제는 백업 dependency DAG와 지정 복구 window·legal hold·key 수명을 함께 확인한다.

공간 한도는 source WAL, capture spool, receive spool, archive, backup staging, engine recovery emergency reserve를 따로 계상한다. 정상 foreground는 recovery reserve를 소비하지 않는다. 압력이 오면 credit 축소→명시 backlog 경보→새 write admission 거절 순으로 진행하고 기존 rollback/resolve·cleanup·복구에는 예약된 진행 경로를 남긴다. optional subscriber를 무효화하고 reseed하는 정책은 사전 설정·관측·영속 결정이 있어야 하며 strict 보호 peer를 자동 제거하지 않는다.

DEV3 REBASE의 두 bootstrap 슬롯·WAL/CLR/epoch 한도는 새 durable owner가 과거 WAL을 붙잡는 경우 그대로 성립한다고 가정하지 않는다. F01에서 **pin 때문에 회수 불가능한 bytes + 현재 redo/undo 의무 + 재장애 예약 + metadata 인계**의 상한과 admission 조건을 다시 증명한다. restart 중 peer 응답이 와야 local recovery가 끝나는 순환을 금지한다. 예산 내 진행을 보장하지 못하면 관련 기능 구현/인수를 차단한다. RP를 켜지 않은 S1의 기존 검증 의무는 유지한다.

<a id="backup"></a>
## 10. 물리 백업·증분·archive

백업 job은 `Planned → Pinned → Copying → Sealing → Complete`이고 실패/취소는 `Incomplete`다. manifest에는 source DB/timeline/profile, checkpoint/FILEMAP identity, redo 시작점, consistency end LSN·q, page identity/generation·크기·digest, undo/status/allocator, parent backup ID, 필요한 WAL 구간·schema/key version을 기록한다. 파일 copy 성공만으로 Complete를 선택하지 않는다.

offline exact-cut은 DEV3 절차를 재사용한다. online full backup은 다음 별도 경로다.

1. 완결 checkpoint의 analysis/redo closure를 pin하고 backup epoch를 등록한다. 물리 파일 drop/truncate·주소 재사용과 topology/format 변경은 backup 동안 차단한다. 일반 DML·신규 page allocation은 계속 가능하다.
2. allocated page 목록과 FILEMAP을 고정한 뒤 engine buffer 경계에서 page latch를 짧게 잡아 image·identity·pageLSN을 복사한다. latch 밖에서 pageLSN까지 WAL durable을 확인하고 backup image를 기록한다. 검사 안 된 raw live file copy·reflink를 일관 snapshot으로 취급하지 않는다.
3. 시작 후 생성/할당된 page는 WAL init + 모든 후속 redo로 재구성한다. 시작 목록에서 회수된 주소도 backup pin 동안 재사용하지 않는다. 이 FILEMAP/allocator closure가 지원되지 않으면 온라인 backup을 시작하지 않는다.
4. 마지막 image 복사 이후 complete mini-transaction 경계 Lend를 잡고 Lredo부터 Lend까지 archive closure와 control metadata를 영속 보관한다. Lend는 모든 복사 pageLSN 이상이다. image·manifest·namespace 검증/선택 뒤만 Complete다.

이 backup은 fuzzy image이며 시작 시점의 snapshot이 아니다. 최초 복원 가능 cut은 manifest의 consistency end 이상이다. uncommitted page와 undo/status를 포함하므로 restore는 analysis/redo 후 loser undo를 수행한다. 백업 중 DDL은 지원 행렬에 따라 Busy·대기이고 숨은 실행/누락이 아니다. online DDL 동시 지원은 별도 파일 세대/redo closure leaf를 통과해야 한다.

prepared 거래를 포함하는 backup은 DEV7의 별도 prepared-backup profile, XID/RM/TM identity·lock/resolve 예약·decision provenance를 포함해야 한다. 해당 공급이 없으면 prepared 존재 시 backup을 Busy로 거절하며 offline drain이 prepared를 임의 abort하지 않는다. H1 seed 목적이면 위 §7에 따라 active/prepared를 보존하고 undo하지 않는다.

증분은 동일 계보의 full/base 이후 변경 page와 allocation/free·FILEMAP·key/schema 변경을 포함한다. differential은 지정 직전 parent, cumulative는 지정 full/base를 parent로 명시한다. pageLSN 하나만으로 변경 여부를 판정하지 않는다. 첫 정확성 경로는 전체 page identity/generation+digest 비교이며 change bitmap은 false-negative가 없도록 WAL로 보호·checkpoint한 뒤의 최적화다. 추적 파일 유실/불명은 full scan으로 돌아가고 빈 변경으로 처리하지 않는다. chain의 parent 누락·혼합 timeline·재사용 page 세대 오류는 restore 전에 거절한다.

archive는 닫힌 WAL segment 또는 명시 봉인 prefix, header/epoch/timeline chain, checksum와 필요한 control 정보를 다룬다. 임시 이름으로 전송→대상 파일/namespace sync→read-back/hash 확인→archive manifest 선택→complete frontier 전진이다. 동일 이름의 다른 내용은 충돌이다. 시간 기반 삭제 대신 backup/PITR/peer 의존 집합을 사용한다. archive sink 장애는 보호 window를 넘기기 전에 경보와 admission 정책으로 드러내며 조용히 WAL을 삭제하지 않는다.

<a id="pitr"></a>
## 11. Restore·PITR·매체 손상

restore는 빈 소유 경로의 staging에서 실행한다. 기존 DB와 backup 원본을 덮어쓰지 않는다. backup chain·WAL 연속성·파일 identity·key 접근·허용 profile과 target cut을 먼저 검증한다. 손상된 current metadata를 발견했다고 파일 번호가 큰 다른 자료를 임의 채택하지 않는다. 알려진 backup와 archive closure로 전체 DB를 재구축하는 경로를 최초 매체 복구로 삼고 온라인 단일 page 수선은 별도 leaf다.

PITR의 정확한 대상은 `timeline + commit_seq`이고 해당 commit을 포함하는 complete WAL 경계로 변환한다. 시간 요청은 WAL에 인증된 commit time index를 이용해 **가장 긴 연속 commit prefix**를 선택한다. commit 시 `logical_commit_time=max(previous, observed_UTC)`를 기록하고 같은 시간은 commit_seq로 구별한다. clock 역행/큰 jump·동일 timestamp의 모호성은 실제 선택 cut과 관측 시각으로 반환한다. q를 명시하면 시계 추정이 개입하지 않는다. 이는 Oracle SCN과 같은 수치 체계를 뜻하지 않는다.

선택 backup의 consistency end가 target보다 뒤이면 그 backup을 거절한다. target 뒤의 committed page image를 단순 redo 중지로 되돌릴 수 없기 때문이다. redo는 source의 인증된 target prefix에 한정하고, 그 cut의 미완료 거래만 UndoRef/CLR로 되돌린다. target 후의 source commit·CLR을 섞지 않는다. prepared 상태는 기존 XID/decision 계약대로 보존하며 운영자가 임의 commit을 합성하지 않는다.

이 비교는 q 숫자만이 아니라 target으로 확정한 **완결 WAL 끝 LSN**으로 수행한다. 같은 마지막 q를 가진 backup에도 이후의 sequence 예약·page/undo/control 변경이 포함될 수 있다. `backup Lend > target LSN`이면 같은 q여도 거절한다. 요구한 q/시간 경계를 입증할 WAL/index가 없으면 가장 가까운 보유 로그 끝을 성공 target으로 대신하지 않는다. 선택 prefix까지의 FILEMAP/dependency history를 archive에서 복원하며 이후 control 세대를 끌어오지 않는다.

target prefix sync → parent/fork cut과 새 timeline 선택 → 새 WAL/CLR·복구 예약 → loser undo/무결성 → 최종 checkpoint/namespace sync → readiness 순서다. 새 LSN은 복사/redo pageLSN의 high-water보다 크게 시작한다. restore job 중 반복 crash는 같은 selected branch와 진행 의무에서 재개하며 매번 새 timeline/epoch 파일을 무한 생성하지 않는다. selector 불명은 두 closure를 보존한다. 원본 source WAL을 접근 불가능하게 만든 뒤 복원·새 ACK·다시 crash/reopen까지 검증한다.

일반 restore/PITR clone은 원 TM/RM·외부 routine·scheduler/queue endpoint에 자동 재접속하지 않는 격리 상태로 시작한다. DB만 과거로 돌려 외부 participant의 이미 확정된 거래까지 되돌렸다고 주장하지 않는다. prepared/decision 상태를 외부와 다시 연결하려면 원 instance의 재시작까지 fencing, stable RM/TM identity 인계, target cut과 외부 decision 대조·중복 dispatch 차단을 별도 운영 job으로 검증한다. 이 조건이 충족되지 않으면 resolve/외부 effect admission을 닫고 in-doubt를 보존한다. H1 failover는 임의 PITR clone과 구별하며 같은 protected obligation을 보존한 역할 인계만 허용한다.

<a id="security"></a>
## 12. 인증·권한·전송·키·감사

DEV5 local peer 인증에 더해 remote SQL/RP/관리 endpoint는 상호 인증한 TLS와 명시 principal mapping을 요구한다. 평문 fallback은 기본 금지다. 인증서 CA/이름·EKU·만료·폐기·교체를 검증하고 새 연결에 새 credential epoch를 적용한다. 기존 연결 종료 정책과 이중 인증서 교체 기간을 운영 설정으로 명시한다. 암호 library·password KDF·TLS version/cipher·키 provider는 F07에서 유지보수/보안 근거와 DP04 검토로 고정하며 자체 암호 알고리즘을 만들지 않는다.

역할은 업무 권한과 replication/backup/restore/security/audit/operator 권한을 분리한다. role graph cycle, grant option·소유자 변경, routine의 invoker/definer, revoke 뒤 prepared/result·관리 job의 권한을 검사한다. deny-by-default이며 startup/bootstrap 관리자 경로도 local 소유·감사를 요구한다. password는 평문 보관/로그를 금지하고 salt·KDF version·잠금·만료·관리 복구를 지원한다. LDAP 외부 인증은 TLS·timeout·명시 mapping과 실패 정책을 갖고 암묵 local fallback을 하지 않는다.

데이터 보호는 page/backup/spool과 column provider를 구별한다. envelope의 key ID/version·nonce/무결성 tag와 DB/object identity 결합, rotation 중 old/new key 참조, restore·replica의 key 수신 권한을 정의한다. 현재/backup/archive/replica가 필요로 하는 키는 삭제하지 않는다. key 불가·tag 불일치는 안전한 정지이고 빈 값으로 복구하지 않는다. 구체 provider/암호 codec·nonce 유일성은 F07 전에는 구현하지 않는다. SQL 정렬/동등성과 ciphertext 정렬은 다르며 지원하지 않는 검색을 거절한다.

F07의 protection profile은 WAL·undo·archive·temporary spill·audit/진단 출력까지 각 매체의 암호화/보호·의도한 제외·redaction을 명시한다. page만 암호화하고 평문 WAL/backup/spill이 남는 구성을 DB 전체 저장 보호라고 표시하지 않는다. nonce의 영역에는 node/timeline/key version과 crash/replay·증분 복사·rotation을 포함해 재사용을 검증한다.

감사 event는 principal·권한 epoch·operation/object·요청 ID·local outcome/protection·오류·시각을 담고 SQL 값·password·key는 기본 제외한다. 필수 감사 정책은 효과 전 durable intent와 완료 outcome을 연결한다. DB 내 효과는 transaction과 같은 durable audit obligation을 commit하고, 외부 효과는 재시작 때 Pending/Unknown을 해소할 job ID를 가진다. audit sink 장애 시 새 auditable 업무 admission을 닫되 이미 durable한 commit을 rollback으로 바꾸지 않는다. 보안 거절/접속 실패도 별도 제한된 예약 lane에 기록하며 overflow 자체를 누락 계수로 보고한다. 회전·보존·삭제 권한과 tamper-evident digest chain을 관리하되 hash chain만으로 privileged 삭제 방지를 보증하지 않는다.

<a id="operations"></a>
## 13. 설정·운영 view·도구·이관

설정 registry는 이름·타입·단위·기본값·범위·scope·secret 여부·reload 가능 시점·의존성을 가진다. `requested/effective/pending_restart/rejected`와 ConfigEpoch를 표시한다. 전체 candidate 검증과 자원/provider 준비 후 한 세대로 선택하며 부분 적용을 금지한다. topology·보호 mode·key 변경은 일반 atomic reload보다 강한 전용 job을 사용한다. active job은 시작 config와 변경 허용 여부를 보존한다.

관리 job은 stable JobId, owner, 입력 digest, phase, progress 단위, checkpoint, 결과·취소 효과를 저장한다. 같은 JobId/다른 입력은 거절한다. 재접속은 조회/재개이며 새 파괴적 작업을 자동 생성하지 않는다. 취소는 Copying 등 가역 구간과 Sealing/Promotion 선택 후의 회복 의무를 구분한다. terminal 응답에는 완료·실패·부분 결과·Unknown과 남은 정리 의무를 함께 표시한다.

| 공개 표면 | 최소 정보·동작 |
| --- | --- |
| 운영 view | node/role/epoch, session/transaction/lock/wait, buffer/undo/WAL·retention owner, replica recv/durable/applied/capture restart, backup/archive closure, job·config·audit 상태 |
| metric/health | counter/gauge/histogram의 단위·reset epoch·sample 시각·staleness. liveness와 serving/readiness/strict-write readiness 구별 |
| CLI·관리 화면 | 상태/목록, job 시작·중지·재개·취소, 입력 검증, cut/손실/진행률·오류, 같은 API 기반 기계 출력과 사용자 출력 |
| exporter·SNMP | 인증·rate limit·MIB/metric mapping·trap 중복/누락·재시작 의미. DB 내부 관리자 권한을 exporter에 포괄 부여하지 않음 |
| 진단 bundle | source/binary/symbol/profile identity, redacted 설정, bounded trace·wait·plan/통계·retention 설명, 파일별 digest와 접근 기록 |

운영 snapshot은 각 component의 sample epoch/시각과 일관성 범위를 표시한다. 하나의 transaction snapshot이 아닌 값을 원자 DB 상태로 포장하지 않는다. metric label에 사용자 SQL·무제한 object 이름을 넣지 않고 cardinality·queue·trace bytes를 제한한다. query plan의 estimated/actual 행 수와 cache invalidate reason을 구별한다. result cache가 없으면 기능·성능 기준으로 대체 가능성을 판정하며 채택하지 않은 cache를 구현했다고 표시하지 않는다.

SQL CLI의 interactive/batch·변수·encoding·exit code·transaction 제어는 DEV4/6를 확장한다. logical export는 고정 snapshot·schema/dependency manifest·typed NULL/value/LOB encoding을 기록한다. import는 source format별 mapping을 사전 검토하고 staging·검증·원자 공개 또는 명시 batch commit 정책을 선택한다. 불지원 type/객체를 조용히 생략하지 않으며 실패 행·이미 commit된 batch·재시작 token을 보고한다. 이종 이관은 schema·routine·권한·collation·시간 의미 차이를 mapping 보고서와 독립 원본/대상 비교로 확인한다. native bytes 호환을 요구하지 않는다.

선택 동기화는 양쪽 고정 cut에서 missing/extra/different/duplicate를 분류하고 실행 직전 before-version을 재검사한다. dry-run 결과로 무조건 덮어쓰지 않는다. cutover는 새 쓰기 drain·잔여 delta 검증·client 전환과 rollback 가능한 시점을 명시한다. 외부 directory·관리 tool 연계는 실제 표준 client/runtime 예제로 인수한다. service/container lifecycle은 소유 volume·graceful drain·forced kill/reopen·readiness·secret 주입을 검증하고 컨테이너 시작만으로 내구성을 주장하지 않는다.

<a id="resources"></a>
## 14. 자원·동시성·관측 기본값

다음은 최초 구현 profile의 엔지니어링 상한이며 제품 최대 용량·고객 SLA가 아니다. 상한을 넘는 필수 capability는 축소 완료하지 않고 별도 leaf profile로 확장한다. 산식·비용·공유 charge를 F09에서 동결한다.

| 항목 | 초기 값·정책 |
| --- | --- |
| peers / subscriptions | node당 peer 4, logical subscription 8; ConfigEpoch별 고정 |
| 전송 | AXF1 하위 max frame 준수, data chunk 최대 64 KiB; logical transaction은 fragment로 분리 |
| 메모리 | peer별 송수신 credit 합계 8 MiB, node capture cache 32 MiB, backup copy 8 MiB, control/emergency 4 MiB 별도 예약 |
| 디스크 | capture 1 GiB, receive peer당 1 GiB의 최초 cap; archive/backup은 job 전 예상 closure+staging cap 예약. engine WAL/복구 예약은 F01 산식 |
| 동시 작업 | physical backup 1, restore/promotion 상호 배타, logical sync 2, 일반 관리 job 8; 나머지는 bounded Busy |
| 제어 | heartbeat 1초·suspect 5초, remote wait 10초의 시험 기본값; heartbeat가 fence 권한을 만들지 않음 |

control/ACK/cancel 경로는 data credit을 모두 쓴 경우에도 진행할 예약을 가진다. file I/O·TLS·network·provider callback·await는 page/commit/retention latch 밖에서 수행한다. 여러 owner를 다룰 때 engine coordinator→retention 순서를 유지하고 RP mutex를 잡은 채 engine을 호출하지 않는다. 별도 작업이 준비한 immutable completion을 받아 짧게 상태를 갱신한다. quota는 실제 allocator의 fallible 보장과 구별하며 metadata·index·digest·FD·task·스레드 stack·임시 복사까지 계상한다.

데이터가 buffer의 4배를 넘는 실제 disk 시험과 RAM을 넘는 별도 qualification profile을 모두 둔다. 신규 큰 transaction/LOB profile은 DEV7 예약·chunking·복구 의무를 공급받아야 하며 DEV3의 작은 거래 상한을 임의 해제하지 않는다. 성능·장시간·RPO/RTO 기준과 PF08/PF14는 [인수 계획](../plans/dev8-implementation-plan.md#performance)을 따른다.

<a id="decision"></a>
## 15. DEV8-DESIGN-001 결정

| 항목 | 기록 |
| --- | --- |
| 분류·상태 | D2 · ACCEPTED · 2026-10-19; `/root/dev8_design_review` 독립 설계 검토 PASS 뒤 계약/통합 담당 `/root` 반영 |
| 담당·근거 | 작업/계약/통합 `/root`; 사용자의 DEV8 문서 작성과 Oracle·ARIES·로컬 소스 참고 요청, DP01의 기존 위임 |
| 선택 | DEV3 저장 계약 위 physical HA와 logical/M1을 분리, 명시 ACK/protection, durable capture 인계, 검증 가능한 fence, fuzzy backup closure·PITR, 보안/운영 job과 독립 인수 |
| 대안 | 원격 receive만으로 durable ACK, timeout의 async 성공 강등, heartbeat만으로 자동 승격, volatile restart cursor, SQL 재실행 복제, live file copy만으로 online backup을 배제. 각각 유실·split-brain·중복 효과·복원 공백 반례가 있음 |
| 대체 범위 | 기존 DEV8 미구체화 항목과 R11/R12·운영 audit/dump의 단계별 보류를 본문 범위로 구체화. DEV3 기본 복구·DEV7 R10 보류·제품 방향·버전 정책은 유지 |
| 조사·검토 | [실제 source revision·관측과 검토 기록](../evidence/dev8-source-research.md); 제품 실행은 NOT_RUN |
| 재검토 조건 | capture supplement 원자성 불가, multi-origin 충돌/closure 누락, old writer 차단 불가, recovery reserve와 retention 양립 실패, 온라인 page copy/DDL closure 불성립, 키/감사/driver 결과 손실, 측정된 성능 병목 |

이 설계의 수치와 선택을 실행 결과로 표시하지 않는다. D8-F01–F10과 모든 필수 variant·현재-build 복구·하위 회귀·독립 검토를 마친 뒤에만 OPS1을 완료한다.

<a id="decision-002"></a>
## 16. DEV8-DESIGN-002 재검토 보완

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-19; 독립 수정본 검토 PASS 뒤 계약/통합 담당 `/root` 반영 |
| 담당·위임 | 작업/계약/통합 `/root`; 사용자의 DEV8 재검토 및 발견 문제 즉시 수정 요청과 DP01 위임 |
| 선택 | 적용 domain/projection별 ledger, partition 이동의 전후 membership, 외부 공개 전 sequence 범위 보호, WAL cut별 control dependency 선택 |
| 대안·이유 | source 거래 ID만으로 dedup하면 다른 projection 누락/거짓 충돌; after partition만 보면 잔존/진입 행 유실; COMMIT에서만 보호하면 반환된 NEXTVAL 재발급; 최신 donor control 복사는 선택 cut과 무관한 파일/복구 상태 혼입 |
| 대체 범위 | DESIGN-001의 §2–3 공급/정체성, §6–7 logical apply/sequence, §11 cut 판정을 본문으로 보완. DESIGN-001의 나머지 불변식·원래 검토 기록·제품 방향·NOT_RUN 상태 유지 |
| 근거·검토 | [REVIEW-002의 반례·실제 소스·독립 검토](../evidence/dev8-review-002.md); `/root/dev8_independent_review`가 원문·반례·수정본을 독립 검토: PASS, 남은 필수 P1/P2 지적 없음 |
| 인수·재검토 | F01–06/09에 반영한 필수 variant. 다중 경로/domain 변경·sequence 외부 공개/권한 인계·namespace/cut 변경 때 재검토. 제품 실행·OPS1은 NOT_RUN |

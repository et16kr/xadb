# DEV3 원 Commit의 지연 설치 수명

2026-10-25 · `D3-COMMIT-CONTINUATION-001` · 담당/통합 `/root` · 공통 소유 설계 D2 한정 채택, 실제 Commit 의미·제품 검증은 미완료.

이 결정은 [원 admission 계약](dev3-concurrency-admission.md)의 후속 설치 경계를 구체화한다. 사용자의 DEV3 완수·병렬 작업 위임과 DP01의 D2 권한을 따른다. `/root/undo_design`의 소유 검토와 `/root/dev3_review`의 별도 원 source 검토를 반영했다. 범위는 원 `PageConcurrency`·WAL single slot·`SharedPool`에서 같은 pin을 유지하는 비공개 continuation이다. Root/Job/credit·실제 Commit producer가 없는 공통 타입을 Commit 완료로 판정하지 않는다.

## 1. 실제 소비와 권한의 경계

`D3-MODULE-ADOPTION-001` **D2 ACCEPTED 결정**에서는 이 문서의 continuation과 exact receipt를 [원 apply 경계](dev3-module-boundaries.md#api-candidates)에서 실제 소유한다. Catalog 제품과 A INSERT는 제한 API를 호출하며 원 whole PinSet·generation·before digest를 Pending→Drain→whole install까지 그대로 인계한다. 새 공통 InstallGrant를 raw factory나 legacy permit 우회로 발급하지 않는다. 원 mutable ledger는 Serializable validation부터 actual installed receipt·epoch/Q 공개까지 보유한다. Installation이 Open으로 바뀐 사실만으로 Q 공개 또는 Commit 직렬화 종료를 추정하지 않는다. normal ACK와 UNKNOWN/resolve·Drop fence 의미는 유지하며 이 반영은 실행 결과가 아니다.

첫 실제 소비는 첫 extent 안의 작은 native Catalog CREATE, 원 Commit, 새 process 복구 후 table identity 읽기다. [구현 계약 4절](implementation-contracts.md#section-4)에 따라 CREATE는 독립 원자 거래이며 다음 Row INSERT는 별도 DML 거래다. 일반 DML 거래 중 DDL은 효과 전에 거절한다. SQL implicit commit, index, overflow, SMO는 이 첫 소비의 선행이 아니다.

공통 코드는 먼저 작성할 수 있다. 다만 다음 권한은 실제 terminal producer가 공급하기 전까지 생성자를 닫아 둔다.

- `PreparedCommit`: 실제 Active actor/status generation·종료된 statement·head, Terminal Job lease, Root C의 checked successor q, Root/Job census와 Tx→Aging credit 인계, exact targets/after images와 immutable frame의 같은 before 검산.
- `StagedCommit`: 위 준비를 같은 original WAL에 완결 stage한 정확한 `WalRequirement`, 원 core/key와 semantic actor/q의 결합. 단순 frame decode·raw LSN·임의 durable receipt로 생성하지 않는다.
- 설치 완료 증거: 모든 page/metadata/DPT와 ATT/descriptor/terminal bank를 설치한 같은 producer만 발급한다. N개 page bit mask 또는 무인자 호출로 barrier를 해제하지 않는다.

이 constructor 폐쇄는 B2의 최소 terminal 의미와 함께 검토한다. B3 CREATE가 B4 nonempty recovery까지 닫히기 전에는 제품의 새 효과를 활성화하지 않는다. q/C 공개·snapshot registry·ACK는 공통 primitive의 단독 완료 분모가 아니다. test-only simulated receipt를 production grant로 승격하지 않는다.

## 2. 원 pin의 이동

whole-set `WriteTargets`가 이미 확보한 `PinSet`을 stage→pending→install로 이동한다. 새로운 pending registry, 별도 pool, pin 재예약, DPT capacity owner를 만들지 않는다. `DptStamp`는 고정 배열의 현재 일관성 검산값이며 공간 예약 token이 아니다.

준비 시 actual identity/registration generation/pageLSN/modification counter/content digest와 모든 counter의 checked successor를 확인한다. stage 이전 준비 실패는 기존 checked cleanup으로 돌아간다. 같은 foreground version guard로 Intent를 설치하고 original stage가 완결되면 page guards를 역순으로 반환한 뒤 version/apply admission을 놓는다. Pending은 원 `&mut Owner`, `&SharedPool`/PinSet 및 immutable private plan/after loan만 보유한다. page/version/append/registry/publication mutex guard와 content admission은 0이다. shared maintenance workspace/FIFO turn을 잡은 채 sync를 기다리지 않는다.

이는 원 계약의 “CommitInstallation은 page pin을 소유하지 않는다”를 실제 pending continuation에 한해 대체한다. pin은 물리 정체성과 퇴출 금지만 보장하며 page content latch를 뜻하지 않는다. 일반 dirty writer의 기존 pin 해제는 바꾸지 않는다.

재개 시 같은 durable proof가 exact requirement를 확인해야 한다. 다음 stable 필드는 같아야 한다.

```
identity, registration generation, pageLSN, modification counter, content digest
recLSN: Some(r) -> Some(r) 또는 None; None -> None
```

lower image flusher가 같은 before를 clean으로 만든 것만 허용한다. 원 pool의 frame/DPT pair 검사를 거친 actual stamp로 DptStamp와 sealed install permit을 다시 만들며 `first_rec_lsn = actual.rec_lsn.unwrap_or(commit_lsn)`이다. counter/content 변화 또는 다른 recLSN은 fence한다. 기존 flush claim은 Commit이 소비하거나 덮어쓰지 않는다.

## 3. durable 뒤 content drain

sync Pending 중 기존 reader와 lower-image flusher의 content 구간은 허용한다. authentic durable proof 뒤 같은 설치 key가 원 core의 `Drain`을 먼저 설치한다. 이 상태부터 새 Apply와 Content는 `InstallationPending/Unchanged`로 거절한다. 이미 진입한 유한 content/apply 구간은 실제 latch를 반환한 뒤 자기 counter를 반환한다. 새 reader의 계속된 유입으로 설치가 굶는 경로를 닫는다.

원 core의 Condvar 하나를 같은 state mutex의 key/Drain/active_apply/active_content predicate와 결합한다. latch 반환→state에서 counter 감소→notify 순서다. waiter는 같은 mutex로 Drain과 predicate를 확인하므로 release-before-wait를 잃지 않는다. 원 `SessionFence`는 AtomicBool이고 외부 notify를 보장하지 않으므로 10ms `wait_timeout`으로 fence를 재검사한다. poison/fence에서는 state lock을 놓은 뒤 token/Owner cleanup을 실행하며 Abandoned를 지우지 않는다. 10ms는 재검사 간격이며 OS wall-time SLA가 아니다.

Drain은 **flush claim이나 I/O 완료를 기다리지 않는다**. in-flight flush는 claim을 보유한 채 I/O를 마칠 수 있고 finalize는 원 RAM retry로 남는다. `dirty_flush::retryable`은 기존 Busy/Capacity에 정확히 `Concurrency(InstallationPending)` + `Effect::Unchanged`만 추가한다. 오류 원인을 Busy로 변환하거나 모든 오류를 retry로 바꾸지 않는다. 이 수정 없이 상위 flusher들이 정상 차단을 fatal로 처리하는 원 source 문제를 별도 리뷰에서 확인했다.

active_apply/content가 모두 0이면 전용 continuation이 version/ordered target latches를 취한다. 현재 live 경로는 각 latch를 admission census 아래 취득하며 exclusive residency 경로는 `&mut SharedPool`/quiescent가 필요하여 pending borrow와 공존하지 못한다. 따라서 이 시점의 Busy는 숨은 사용자 또는 불변식 위반으로 fence한다. 일반 FIFO 구현이나 무한 try loop를 도입하지 않는다. 새 raw latch 사용자가 생기면 이 census 근거를 다시 검토한다.

## 4. 실제 WAL 첫 쓰기

stage 호출, complete stage, 첫 File write 시도를 구분한다. 기존 `WalIoError`의 attempted 분류는 file mutex poison/fence 등 쓰기 전 실패에도 쓰이므로 Commit certainty의 근거가 아니다.

원 single slot의 정확한 Commit identity/requirement에 고정 marker를 결합하고 `sync_claim`에서 file lock·fence 검사를 통과한 뒤 첫 `File::write_at` 호출 직전에 한 번 전이한다. `ensure_durable`과 `sync_pending` 어느 drainer가 가져가도 같은 marker를 전이한다. 새 WAL writer·callback·별도 namespace를 만들지 않는다. complete stage 뒤 오류는 retained obligation과 fence를 유지하며 정상 Abort로 되돌리지 않는다. actual first write를 하지 않은 사실과 세션 재사용 가능 여부는 별개다.

## 5. 설치·정리와 한정 검증

모든 page/metadata/DPT/ATT/terminal 설치 뒤만 barrier Open이 가능하다. cleanup 순서는 content guards→installation/version/admission→pins다. Pending/Installing Drop·partial install·unwind는 먼저 fence/Abandoned를 보존하고 임시 소유만 정리한다. 정상 continuation을 재발급하거나 ACK하지 않는다. installed status=q, published Q<q인 짧은 구간은 snapshot 가시성 계약으로 처리하고, 작은 view/Q의 공개 뒤 ACK한다.

단독 공통 코드에서 검증 가능한 것은 변경된 source의 자원 census, lifetime/비위조 compiler 경계와 실제 stage 이전 Intent 취소다. actual semantic factory가 없는 stage/sync/resume 입력은 NOT_RUN으로 남긴다. 시험 grant로 실제 Commit 성공을 만들지 않는다. 최초 actual CREATE Commit에서 다음을 함께 선택한다.

- 같은 PinSet을 유지하고 pending latch/admission 0, wrong core/key/requirement 거절, 소유 escape/double resume 컴파일 거절.
- lower flush의 clean-before-resume, install-before-finalize, clean 후 claim 반환 전 install 세 순서와 DPT 요구량 보존.
- 같은 Status의 다른 slot writer, allocator INIT/aging/CLR 추월 차단, Pending reader 허용, Drain 신규 reader 거절, release-before-wait/fence/poison 깨움.
- 첫 File write 전/부분 write/sync Unknown/durable 후 부분 install/frontier/ACK cut, actual controller history와 새 process 복구.
- counterMAX·pin 부족·부분 준비의 효과 전 거절, 모든 after/descriptor와 권한이 actual producer에서 닫히는 source/compile 검산.

F05는 N=4 첫 terminal target candidate와 N=9 CREATE Forward를 구분한다. retained carriers·Result/return 중복·stamp/digest·guards·private loans·원 WAL slot 및 control 증가를 실제 동시 수명으로 센다. 기존 65,536B component·196,608B metadata·262,144B whole cap을 늘리지 않는다. A의 native recovery upper bound 65,456B는 원 source의 결과이며 새 core 크기에서 무검산 재사용하지 않는다. 최초 새 검증 전에 exact selectors·oracle·time/output/disk/source 한도를 후속 입력 계획에 고정한다.

## 6. 대안·검토·재검토

별도 pending registry는 같은 pin 정체성을 중복 추적하고 DPT capacity 예약은 원 고정 배열과 맞지 않아 기각했다. Condvar wake만 추가하고 신규 content를 계속 허용하는 방식은 reader 유입 starvation을 닫지 못한다. I/O/flush claim까지 drain하는 방식은 finalize가 Drain에 막히는 순환을 만들므로 기각했다. 원 pin 이동과 durable 뒤 짧은 content drain을 선택했다.

독립 검토 `/root/dev3_review`는 원 `concurrency.rs`, `dirty_frame.rs`, `dirty_flush.rs` 및 세 상위 flusher를 읽고 위 조건으로 공통 계약을 수용했다. 실제 terminal factory·wire/credit·nonempty recovery 전체 D2는 아직 보류다. `/root`가 이 구분을 유지하여 공통 구현 진행만 채택했다. 새 authority constructor, raw content 사용자, 범위/상한 확대, 다른 namespace나 shared workspace 대기가 필요하면 해당 계약을 다시 검토한다. 원 A 증거·실패·완료 시험은 보존하며 이 문서는 실행 PASS가 아니다.

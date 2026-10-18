# XADB — 통합 recovery·logging·checkpoint 설계

2026-10-18 적용 범위: **DEV3의 정본은 [DEV3-DESIGN-003](disk-storage-design.md#decision)**이다. 아래 §§1–15의 private/COW·root-only journal·page-before-commit·불변 exact checkpoint·root closure GC는 DEV2 기준선 및 이전 L1/L2 제안으로 한정한다. DEV3에 이를 함께 요구하지 않는다. 아래 “기존 저장 설계 유지”는 해당 당시 결정의 범위다. L1/L2는 DEV3에 자동 채택되지 않으며 새 저장 경로에 맞춘 별도 결정/검증이 필요하다.

<a id="dev3-profile"></a>
## DEV3 WAL·undo 프로파일

단일 ordered L0에 interleaved update/CLR/commit을 기록하고, 영속 UndoRef chain으로 rollback한다. WAL-before-page·steal/no-force와 pending-only doublewrite를 적용한다. FileId와 FILEMAP으로 allocator 이전 bootstrap을 시작하고, DWB retirement 전 주소/슬롯 재사용을 금지한다. pageLSN/identity가 redo 중복 적용을 막는다.

fuzzy checkpoint는 commit 상태 설치까지 일치한 capture LSN·ATT/DPT를 저장한다. analysis는 capture 직후, streaming redo는 최소 recLSN에서 시작한다. 채택 prefix를 먼저 sync하고 recovery epoch/WRITER를 영속 선택한 뒤 CLR을 쓴다. 반복 restart가 그 CLR을 발견해야 하며 최종 STABILIZE 전 user admission을 열지 않는다. source WAL bytes는 보존하지만 home page는 이 규약으로 수정한다.

정상 open은 필요한 WAL/control·복구 page를 검증하고, 전체 row↔index/unique/allocation/undo 검사는 별도 offline audit로 수행한다. S1/fault 인수는 reopen 뒤 full audit와 독립 이력을 모두 요구한다. previous checkpoint는 같은 mutable datafile의 더 이른 redo 시작점이다. exact-C backup은 admission drain·전체 flush 뒤 복사하며 source WAL 없이 새 timeline으로 restore한다.

WAL floor와 undo 수명은 별도다. undo/status/allocator의 redo closure가 갖춰진 뒤 active begin WAL을 제거할 수 있으나 rollback chain·snapshot·incoming undo refs는 보존한다. root/pack 전체 GC를 요구하지 않는다. [기계 계약](recovery-logging-contract.json)의 `dev3_profile`과 [DS01–06](disk-test-matrix.json)·RC-T12–14가 이 선택을 소비한다. 구현·제품 실행은 NOT_RUN이다.




**권고안은 매체 공통의 commit/recovery 상태 기계, 불변 checkpoint generation, 단계적으로 확장하는 병렬 redo logging이다.** DISK의 물리 저장법은 다르지만, 거래 성공 여부·복구 시점·WAL 회수 판단은 하나로 만든다. 두 전체 이미지를 번갈아 고치는 대신, 완성된 generation을 manifest로 선택한다. 로그의 큰 본문은 병렬화하되 거래 간 순서와 원자성은 명시적인 commit 결정으로 보존한다.

기존에 채택한 K0–K5 개발 순서, safe Rust, private write, `CommittedRoot`, DISK COW 선행 영속화는 유지한다. 교차 버전 호환 대상과 배포별 gate는 [중앙 버전 정책](../policies/version-compatibility-policy.json)을 따른다. 현재 build 자체의 recovery·원자성은 배포 채널과 관계없이 검증한다. 첫 K2부터 여러 로그 stream을 요구하지 않는다. 본 문서의 L0/L1/L2는 **로깅 구현 단계**이며 제품 버전이나 기존 DEV/K 단계의 대체 번호가 아니다.

[Recovery undo·RP의 로그 보존 검토](replication-undo-analysis.md) · [세부 작업·인수 계획](replication-undo-plan.json)

<a id="section-1"></a>

## 1. 설계 범위

디스크 엔진의 transaction·WAL·checkpoint·recovery 설계 제안이다. 현재 구현은 volatile K0이며 여기의 영속 경로는 미구현이다. [공개 참고 자료](../reference/opensource-references.md)는 비교 입력이며 이 설계의 실행 증거가 아니다.

<a id="section-2"></a>

## 2. Checkpoint 선택의 책임

복수 checkpoint 파일의 존재보다 중요한 것은 crash 뒤 유일한 recoverable committed cut을 선택할 수 있는가이다. 완성된 data/index/catalog closure와 manifest·selector·directory sync를 검증하고, 이전 선택을 보존한 상태에서 새 generation을 공개한다.

<a id="section-3"></a>

## 3. XADB의 공통 의미와 영속성 불변식

| 이름 | 의미 |
|---|---|
| `TxnId` | DB incarnation과 재시작 epoch를 포함하는 거래 identity. 세션 번호나 재사용 slot과 구분 |
| `commit_seq = q` | DB 전체의 확정 거래 순서. storage medium·thread·log byte offset과 독립 |
| `LogPos` | `(stream_epoch, lane_id, segment_id, byte_offset)`. 서로 다른 lane 위치의 숫자 비교 금지 |
| `D` | 실행 중 sync 확인이 끝난 연속 commit prefix의 끝 |
| `P` | reader에게 공개한 연속 commit prefix의 끝 |
| `C` | 선택 manifest가 완전하게 표현하는 checkpoint commit sequence |
| `R` | 재시작 후 검증한 연속 commit prefix의 끝. 응답이 유실된 commit을 포함할 수 있음 |

```text
정상 실행: C ≤ P ≤ D
ACK(Tq) ⇒ q ≤ P ∧ durable(complete_commit(Tq))
durable(complete_commit(Tq)) ⇒ durable(all_dependencies(Tq))
View(q) = (MEMq, DISKq, Indexq, Locatorq, Membershipq, Catalogq)
Recovered = ApplyInOrder(Checkpoint(C), committed T(C+1)..TR)
관측한 ACK 집합 ⊆ 복구된 commit 집합; 확정 ABORT 집합 ∩ commit 집합 = ∅
```

`D`는 메모리의 sync 확인 값이며 crash 후 진실을 결정하는 파일은 아니다. sync 호출이 오류를 반환했어도 record 전체가 이미 남았을 수 있다. 정상적인 미응답 거래는 포함 또는 미포함 모두 가능하지만 partial transaction은 불가능하다. 읽기는 하나의 root를 pin하고 시작하며 durability 전의 speculative root를 읽지 않는다. 재시작 전에 존재하던 reader는 종료되므로 crash 후 MVCC의 모든 옛 read snapshot을 재현할 필요는 없다.

정상 commit API의 성공은 처음부터 **local durable** 의미로 고정한다. volatile ACK 모드를 같은 성공 값으로 섞지 않는다. replica quorum 내구성은 복제 구현 시 별도 계약이며 local fsync 성공과 동치가 아니다. XA prepared transaction도 초기 범위에서 명시적으로 거절한다. 후속 XA는 prepared intent의 영속화·보존·결정 주체를 추가해야 하며 “commit 없는 데이터는 전부 쓰레기” 규칙을 그대로 적용할 수 없다.

<a id="section-4"></a>

## 4. 저장 전략: private write + redo + COW

transaction은 private row/index 변경을 준비하고 commit 이후에만 공개한다. WAL에는 확정된 ID·값·key·catalog 변경을 기록한다. 디스크 저장은 현재 committed page를 덮어쓰지 않고 새 page/generation을 준비한다. **H1의 기존 계약대로 DISK/index/locator COW page와 파일 이름까지 영속화한 뒤 공통 commit record를 기록한다.** 그래서 일반적인 in-place WAL-first 방식과 다르며, 이 문서의 WAL은 공통 redo/commit journal을 포함하는 명칭이다.


| 대상 | commit에 필요한 내용 | checkpoint에 필요한 내용 |
|---|---|---|
| row/index | 결정된 mutation 또는 after-image, tombstone, object/version ID | 고정 snapshot의 row·index chunk와 root |
| DISK row/index/locator | 영속 COW root 및 allocation generation 참조·digest | 같은 `C`의 root와 전체 reachable closure |
| catalog/membership | schema epoch, 결정된 catalog delta, 세대 전환 | 같은 `C`의 완전한 catalog/membership |
| request dedup | 지원하는 경우 request ID·결과·보존 정책을 같은 batch에 기록 | dedup 상태와 만료 기준 |


대가는 COW 경로 복사, DISK data sync + commit sync의 지연, 오래된 snapshot의 공간 보존이다. data-page redo를 먼저 영속화하고 page flush를 뒤로 미루는 WAL-first 구조는 후속 비교안이다. 채택하려면 page 재구성·buffer eviction·torn-page·replay/GC 계약을 별도 ADR로 바꾸고 동일 시험을 통과해야 한다. 첫 구현에서 이 최적화를 몰래 섞지 않는다.

<a id="section-5"></a>

## 5. 로깅 발전 단계와 병렬화 경계

| 단계 | 구조 | 적용 시점과 gate |
|---|---|---|
| L0 | 한 append stream, private batch, 직렬 coordinator, 완결 batch sync 후 공개 | K2–K5 실제 영속성 기준선. K3 recovery와 연결해야 완료 |
| L1 | 병렬 private encoding, bounded queue, 한 writer, 제한 시간의 group append/sync | B2 이후 성능 최적화. serializable 검증 고도화가 별도 선행 조건 |
| L2 | 여러 payload lane의 append/sync + 작은 전역 commit journal + 순서대로 root 공개 | S1 이후 선택 capability. multi-stream 인수 전 비활성. 적용할 버전 gate는 중앙 정책을 따름 |

L0는 correctness reference이지 최종 처리량 목표가 아니다. L2는 DISK별 log를 따로 두지 않는다. lane은 CPU/NUMA·producer shard·장치 배치를 기준으로 선택하며 물리 실행 자원일 뿐, 각 lane이 독립 commit하는 storage engine이 아니다. 초기 L2는 transaction 하나의 payload를 한 lane에 배정하고, 크기·기능 확장으로 여러 lane에 나뉘면 같은 commit record가 모든 fragment를 참조한다.

<figure>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 900 360" role="img" aria-label="병렬 payload와 disk COW 준비가 모두 영속화된 뒤 하나의 commit journal과 composite root 공개로 합류하는 구조" style="width:100%;height:auto;background:#f1f7f8;border-radius:12px">
<defs><marker id="recovery-arrow" markerWidth="9" markerHeight="8" refX="8" refY="4" orient="auto"><path d="M0,0 L9,4 L0,8" fill="#257b87"/></marker></defs>
<g font-family="system-ui,sans-serif" text-anchor="middle" fill="#193b4b">
<rect x="25" y="25" width="850" height="48" rx="8" fill="#163e50"/><text x="450" y="55" fill="white" font-size="19">Private batch · validation · 전체 view 준비</text>
<g fill="#d5eae5"><rect x="25" y="105" width="265" height="68" rx="7"/><rect x="318" y="105" width="265" height="68" rx="7"/><rect x="610" y="105" width="265" height="68" rx="7"/></g>
<text x="157" y="132" font-size="17">Payload lane 0</text><text x="157" y="155" font-size="14">append → sync</text>
<text x="450" y="132" font-size="17">Payload lane 1 … N</text><text x="450" y="155" font-size="14">append → sync</text>
<text x="742" y="132" font-size="17">DISK COW pages</text><text x="742" y="155" font-size="14">write → sync</text>
<path d="M157,74 V101 M450,74 V101 M742,74 V101 M157,174 V197 H450 V214 M742,174 V197 H450 M450,174 V214" fill="none" stroke="#257b87" stroke-width="2" marker-end="url(#recovery-arrow)"/>
<rect x="155" y="220" width="590" height="49" rx="8" fill="#d8e7f2"/><text x="450" y="250" font-size="18">Commit journal: q + 참조 + digest → group sync</text>
<path d="M450,270 V292" stroke="#257b87" stroke-width="2" marker-end="url(#recovery-arrow)"/>
<rect x="155" y="298" width="590" height="43" rx="8" fill="#c8e9e3"/><text x="450" y="325" font-size="17">연속 prefix의 composite root 공개 → ACK</text>
</g></svg>
<figcaption>L2 목표 구조. 본문과 page를 병렬 기록해도 commit 결정과 의존성 검증은 남는다.</figcaption>
</figure>

**coarse epoch validation의 제약을 숨기지 않는다.** 기존 K4 방식은 동일 epoch에서 시작한 writer 중 첫 성공 이후 나머지를 충돌시킨다. 따라서 큐와 group sync만 추가해도 write TPS가 자동으로 확장되지는 않는다. L1의 다중 writer batch는 read/write set, absence/range predicate, unique·catalog epoch를 **batch 안의 앞선 accepted transaction까지 포함해** 검증하는 별도 동시성 계약이 필요하다. 그 전에는 coarse 방식을 유지하고 효과가 작은 것도 측정 결과로 기록한다.

각 transaction은 의미가 고정된 private mutation을 병렬 준비할 수 있다. 하지만 같은 옛 root에서 만든 두 COW root를 순서대로 교체하면 앞 거래의 변경을 잃을 수 있다. coordinator는 직전 accepted view를 기준으로 validation/rebase/merge하여 `View(q-1) → View(q)`를 완성한다. 이 작업과 새 경로의 I/O 비용도 성능 예산에 포함한다. root의 짧은 publication lock 안에는 I/O나 allocation을 넣지 않는다.

병렬 logging에 의존성 표현이 필요하다는 선행 연구로 [Taurus 논문](https://arxiv.org/abs/2010.06760)을 참고했다. XADB의 초기 L2는 vector dependency algorithm을 도입하지 않고 작은 전역 commit 순서를 유지한다. 논문의 성능 수치를 XADB 예상 성능으로 사용하지 않는다.

<a id="section-6"></a>

## 6. L2 commit 프로토콜과 오류 경계

1. **준비:** ID·after-image·index/catalog 효과를 확정하고 queue/memory/disk/retention 공간을 예약한다. 취소·quota·unsupported 오류를 먼저 처리한다. payload는 lane별 버퍼에서 encode/checksum/compress한다.
2. **검증과 view 완성:** coordinator가 직전 accepted state와 transaction의 읽기·쓰기·predicate를 검증한다. batch 내부 의존성을 포함한 최종 view chain을 준비한다. 새 DISK COW 경로를 완성한다. sequence는 이 검증 순서의 잠정 번호이며, commit record 시도 전 batch를 폐기하면 외부에 확정 번호로 공개하지 않는다.
3. **dependency 영속화:** payload lane의 완성된 연속 범위, COW page 및 namespace를 sync한다. group에 필요한 모든 dependency의 확인을 모은다. 파일 write 완료나 I/O submit 완료를 sync 완료로 간주하지 않는다.
4. **commit 결정 기록:** `CommitGroup(prev_q, first_q, last_q, entries[], digest)`를 전역 journal에 기록한다. 각 entry는 TxnId, q, schema epoch, payload refs/digests, disk root/allocation refs를 포함한다. 참조는 record count·총 길이까지 검사한다.
5. **commit sync:** group 전체와 trailer가 영속화된 뒤 `D`를 전진시킨다. group 도중 한 entry만 채택하지 않는다. sync 오류/결과 불명은 group 전체를 UNKNOWN 처리하고 새 writer를 차단한다.
6. **공개와 응답:** 전체 view chain의 마지막 root를 한 번 교체하여 `P=last_q`로 공개할 수 있다. 이는 group 안 transaction을 q 순서로 직렬 실행한 상태다. 모든 entry의 성공 응답은 이 뒤에 보낸다. 이미 대기 중인 reader는 이전 snapshot을 유지한다.

```text
∀ fragment f ∈ refs(Tq): lane_durable[f.lane] ≥ f.end
∧ durable(COW closure required by Tq)
    happens-before append_attempt(CommitGroup containing Tq)

sync(CommitGroup) → publish(whole view) → reply
```

L0/L1은 payload와 commit trailer를 한 파일의 완결 batch에 넣어 한 WAL sync로 처리할 수 있다. L2에는 payload sync와 commit journal sync라는 **두 단계의 영속 장벽**이 추가된다. DISK는 COW data sync 비용도 지불한다(서로 독립적인 준비 I/O는 병행 가능). “stream 수만큼 빨라진다”거나 “모든 sync가 하나가 된다”고 주장하지 않는다.

worker가 예약한 offset 뒤에 구멍을 남기면 그 뒤 기록까지 durable이라고 보고해서는 안 된다. lane별 `reserved_end`, `written_contiguous_end`, `synced_contiguous_end`를 분리한다. 첫 구현은 lane마다 소유 writer 하나로 구멍을 줄인다. multi-producer 직접 기록 최적화는 완료 bitmap·segment 수명·취소된 reservation의 처리가 검증된 뒤만 허용한다. lane topology는 epoch로 versioning하고 초기 변경은 admission drain 후 새 epoch로 전환한다.

| 장애/취소 시점 | 결과 |
|---|---|
| commit record 시도 전 | ABORTED 가능. 남은 payload/COW는 미공개 orphan. 같은 batch의 후속 view가 이를 참조하면 함께 폐기·재준비 |
| commit append/sync 결과 불명 | UNKNOWN, writer fence, recovery로 판정. sync 오류를 rollback 성공으로 바꾸지 않음 |
| commit sync 후 publish 전 장애 | 재시작하면 group 전체 반영. 호출자는 UNKNOWN일 수 있음 |
| publish/ACK 뒤 장애 | 선언한 fault model에서 ACK된 모든 변경 보존 |
| durable commit이 참조하는 fragment/page 누락 | 정상 startup 실패. 해당 거래나 lane을 건너뛰어 나머지 DB를 열지 않음 |

느린 transaction 하나를 기다리며 전역 순서가 막힐 수 있다. 느린 준비는 가능한 한 sequence 확정 전에 deadline/admission으로 제한한다. 이미 append를 시도한 group의 번호를 건너뛰거나 timeout만으로 실패 확정해서는 안 된다. global commit journal과 publication이 다음 병목이 되면 그것을 계측하고 별도 ADR로 다룬다.

<a id="section-7"></a>

## 7. Checkpoint: 불변 generation과 단일 manifest

**운영 설명은 “generation G가 commit C까지를 포함하며, C 다음부터 journal을 재생한다”로 통일한다.** DISK를 별도 checkpoint 성공으로 공개하지 않는다. checkpoint를 만드는 동안 commit은 계속되지만, 저장하는 내용은 완료된 거래만 포함하는 정확한 snapshot이다. 기존 fuzzy image와 달리 진행 중 transaction을 image에 섞지 않는다.

```text
선택 manifest G42 / C=1000
 ├─ DISK chunk roots: M0, M1, M2
 ├─ DISK/index/locator roots: D0, I0, L0
 └─ catalog/membership/allocator/dedup: 모두 C=1000

새 manifest G43 / C=1200
 ├─ DISK: M0 재사용, M1' 신규, M2 재사용
 ├─ DISK/index/locator: C=1200의 불변 root 참조
 └─ 모든 참조 sync + manifest sync + CURRENT 선택의 directory sync

G43 선택이 영속화되기 전: G42 + 이후 journal로 재시작
선택이 영속화된 뒤:     G43 + 1201 이후 journal로 재시작
```

이는 전체 DB 두 벌을 번갈아 수정하는 방식의 대체다. **안전한 전환을 위한 이전 상태 보존 자체를 제거하는 것은 아니다.** 변경하지 않은 chunk/page를 공유하므로 매번 전체 데이터 두 벌을 강제하지 않지만, full rewrite·장기 pin·백업·compaction이 겹치면 저장 공간은 2배 이상도 될 수 있다.

### 7.1 작성과 선택 순서

1. 짧게 root를 pin하여 `C=P`와 그 정확한 composite view를 고정한다. checkpoint pin과 필요한 WAL 위치를 같은 coordinator/retention 경계에서 등록한다. 객체를 읽기 전에 pin한다.
2. 디스크 행의 불변 chunk ID/version을 순회해 필요한 chunk만 새 파일에 쓴다. 현재 시각의 dirty bitmap을 snapshot 내용의 진실로 사용하지 않는다. 이미 직렬화한 동일 chunk는 재사용할 수 있다. DISK는 `C`의 reachable root를 고정하며 최신 root로 바꾸어 읽지 않는다.
3. 신규 파일을 완성하고 checksum/길이/identity를 검증한다. 파일 sync, rename이 필요하면 rename, 관련 directory sync까지 끝낸다. 원래 generation의 파일을 in-place 변경하지 않는다.
4. `MANIFEST-G43.tmp`를 쓰고 sync한다. 불변 최종 이름으로 rename하고 directory sync한다. manifest 파일이 존재하는 것만으로 선택 완료가 되지는 않는다.
5. `CURRENT.tmp`에 `db_uuid, timeline, generation, manifest_digest`와 지원하는 previous의 정확한 identity/digest를 기록·sync하고 동일 filesystem 안에서 `CURRENT`로 atomic replace한다. 부모 directory를 sync한다. 이 시점에 checkpoint 완료와 `C`를 공표한다. DEV2 L0의 최종 selector I/O는 coordinator에서 commit/rotation과 직렬화하고 old/new closure pin을 유지한다.
6. retention ledger에 따라 더 이상 필요하지 않은 WAL·page·generation만 회수한다. 이전 복구 기준으로의 fallback을 제공하는 동안에는 그 기준에서 새 committed tip까지 갈 수 있는 전체 chain도 pin한다.

file sync만으로 directory entry의 영속성이 보장되지는 않는다. [Linux fsync 매뉴얼](https://man7.org/linux/man-pages/man2/fsync.2.html). XADB는 선택한 Linux/filesystem 조합에서 atomic replace의 crash 결과가 이전 또는 새의 완전한 선택이 되는지 검증한다. 다른 filesystem의 경험을 그대로 보증으로 삼지 않는다. selector는 작은 metadata이며 필요 시 중복 저장을 별도 설계할 수 있지만 전체 DB ping-pong과는 구분한다.

### 7.2 Manifest의 의미 필드

```text
format_id, required_reader, required_features, checksum
db_uuid, timeline_id, generation_id, parent_manifest_digest
checkpoint_commit_seq = C
MEM_chunk_index_root, DISK_root, index_roots, locator_root
catalog_root, membership_root, allocation_map_root, ID_high_waters
request_dedup_root (지원하는 경우)
commit_journal_resume_pos, stream_epoch/topology
payload scan hints (권위 있는 commit 결정으로 사용하지 않음)
object inventory root: identity, size, checksum, format, owner
```

목록이 크면 manifest 안에 전체 항목을 넣지 않고 영속 index root를 둔다. RAM 크기가 전체 DISK page 수에 비례하지 않도록 inventory·mark/sweep도 bounded cache/외부 정렬로 처리한다. checksum은 실수·손상 탐지이며 인증된 원격 백업이나 악의적 변조 방어의 대체가 아니다.

generation 번호는 재시작 후 남은 orphan과 충돌하지 않도록 고유하게 할당하고, 과거 이름을 덮어쓰지 않는다. manifest가 이전 chunk를 참조해도 무한 parent chain replay를 요구하지 않도록 inventory를 완결된 root graph로 만든다. checkpoint는 group 경계의 published sequence만 고르며 중간 entry를 자르지 않는다.

DEV2 L0는 resume 위치를 [인증된 replay anchor](dev2-kernel-design.md#replay-anchor)로 구체화한다. manifest가 고정한 segment/header identity·C 직후 offset·prefix digest를 검증한 경우에만 그 segment보다 앞선 predecessor 탐색을 종료한다. anchor segment 전체와 current/previous 각각에서 실제 WRITER tip까지의 WAL·epoch 연결을 보존한다. checkpoint는 WRITER를 수정하지 않으며, 이미 전진한 tip을 checkpoint 시작 때의 값으로 되돌리지 않는다. [CURRENT 선택·GC 상태표](dev2-kernel-design.md#checkpoint-selection)가 L0의 상세 규범이다. L2의 다중 lane에는 이 단일 segment 경계를 그대로 적용하지 않고 각 참조·예약 보존 의무를 유지한다.

### 7.3 장애 후 선택 규칙

| 상태 | 복구 선택 |
|---|---|
| chunk 작성 중 / manifest 작성 중 / CURRENT replace 전 | 기존 CURRENT가 가리키는 완전한 generation. 새 파일은 선택되지 않은 staging |
| CURRENT replace 뒤 directory sync 결과 불명 | 디스크에 남은 CURRENT의 identity/digest를 검사해 이전 또는 새 generation 사용. 마지막 실행 로그만 보고 추정하지 않음 |
| CURRENT가 새 generation을 가리킴 | 그 manifest와 참조 전체를 검증. 일부만 오래된 generation에서 섞어오지 않음 |
| CURRENT·선택 manifest·필수 파일 손상 | fail closed. 복구 가능한 이전 기준+완전한 후속 chain이 입증된 경우에만 전체 재구성 |
| 이전 manifest는 있지만 후속 WAL/page가 이미 회수됨 | 그 manifest를 정상 crash recovery fallback으로 사용하지 않음. 명시적인 과거 시점 restore와 구분 |

최신 generation이 잘못되었다고 무조건 두 번째로 큰 번호를 고르는 정책은 금지한다. 번호가 큰 manifest는 아직 선택 전일 수 있고, 오래된 manifest는 ACK 이후 변경을 복원할 자료가 없을 수 있다. `CURRENT` 손상 시 일반 open은 오류를 내고, 검증 도구가 충분한 chain을 입증한 별도 복구 절차만 진행한다.

<a id="section-8"></a>

## 8. 공통 restart recovery 알고리즘

1. **Fence/preflight:** DB의 단일 writer 소유권을 확보하고 normal admission을 닫는다. UUID/timeline·format·reader feature를 확인한다. 지원하지 않는 형식이면 변경 전에 거절한다.
2. **Select/validate:** CURRENT와 manifest를 검증해 하나의 `G,C`를 선택한다. row chunks를 제한된 workspace로 로드하고 DISK root를 bounded buffer로 연다. 참조 identity/checksum·catalog/root 관계를 검증한다.
3. **Scan decisions:** `C` 다음의 commit journal을 순서대로 읽는다. framing, group digest, TxnId, schema epoch, `prev_q`와 연속 sequence를 확인한다. lane의 맨 끝이나 파일 mtime으로 committed tip을 결정하지 않는다.
4. **Resolve dependencies:** 채택할 group이 참조하는 모든 payload와 DISK page closure를 확인한다. 누락·중복 충돌·checksum 불일치는 오류다. commit에 속하지 않는 payload는 redo하지 않는다.
5. **Redo into private recovery state:** group/transaction 순서대로 row after-image와 metadata를 적용하고 검증된 DISK root로 전환한다. index·locator·membership을 함께 맞춘다. SQL, trigger, 현재 시간, random, sequence 생성 함수를 재평가하지 않는다.
6. **Verify:** 필요한 물리 무결성과 row/index/locator/catalog 정합성을 private state에서 검사한다. 아직 정상 서비스나 root 공개를 시작하지 않는다. 큰 DB의 incremental verification으로 줄이려면 검증 범위를 별도로 계약한다. recovery scratch는 새 generation으로 작성할 수 있지만 source를 덮어쓰지 않는다.
7. **Stabilize:** 채택한 기존 journal prefix와 필요한 payload/page/epoch·namespace 의존성을 영속화한다. 프로세스 종료 뒤 남은 OS cache에서 읽은 완결 record를 이미 sync된 것으로 간주하지 않는다. 새 metadata의 sync는 기존 파일의 sync를 대체하지 않는다. 실패·결과 불명이면 admission을 닫은 채 복구 오류로 종료한다. DEV2의 보수적 파일 sync 규칙과 두 번의 장애 반례는 [STABILIZE 계약](dev2-kernel-design.md#recovery-stabilize)을 따른다.
8. **Resume/ready:** STABILIZE 뒤 이전 epoch의 검증된 log 끝과 새 writer/stream epoch의 연결을 영속적으로 확정한 다음 전체 root를 공개하고 정상 admission을 연다. 이전의 잘린 tail 때문에 이후 epoch의 성공 거래를 다음 재시작에서 놓치지 않도록 한다. fallback 사용 시 CURRENT를 검증된 기준으로 재선택하는 순서도 [L0 선택 규약](dev2-kernel-design.md#checkpoint-selection)을 따른다. source는 각 유효 anchor·consumer의 회수 조건이 충족될 때까지 보존한다.

재복구는 동일 source에서 다시 시작할 수 있어야 한다. 초기에는 중간 progress 파일을 신뢰하는 shortcut을 쓰지 않는다. 나중에 재개 기능을 넣으면 progress의 root·sequence·출력 파일도 checkpoint와 동일한 원자 선택 계약을 지켜야 한다. `last_applied=q`만 기록하고 page 쓰기를 생략하면 crash 뒤 그 q를 건너뛸 수 있다.

**멱등성의 단위는 검증된 상태+sequence다.** `q≤C`는 이미 checkpoint에 포함되므로 재적용하지 않는다. 이후 `q=last_applied+1`만 적용하고 동일 TxnId/q에 다른 digest가 있으면 오류다. counter 증가를 재실행하는 대신 결정된 after-image/version을 사용한다. 메모리상의 `last_applied`만 바뀌어도 source가 불변이면 재시작은 `C`에서 다시 수행한다.

초기 recovery는 순차 적용을 기준 oracle로 유지한다. 먼저 lane read/decode/checksum·독립 DISK chunk load를 병렬화한다. 후속 병렬 apply는 같은 object/key, range/unique index, catalog/DDL dependency를 지킨다. 단순 table/medium별 thread 분할은 global index와 row movement를 깨뜨릴 수 있다. 각 worker의 staging 작업이 완료되어도 전체 group barrier 전에는 공개하지 않는다.

<a id="section-9"></a>

## 9. Log 형식, tail과 손상 구분

segment header에는 DB/timeline/stream epoch·lane·segment ID·format/required features·이전 segment 연결 정보를 둔다. frame은 종류·길이·TxnId·fragment 번호·object/schema identity·checksum을 포함하며 길이의 overflow·최대 allocation·압축 해제 크기를 먼저 검사한다. group trailer는 전체 entry 수·byte 길이·digest·q 구간을 검증한다. 미리 확보한 segment의 빈 공간과 이전 파일 재사용 잔재를 identity로 구분한다.

| 관측 | 정책 |
|---|---|
| 참조되지 않은 payload의 잘린 tail | commit 효과 없음. dependency 확인 후 orphan으로 정리 |
| active/unsealed commit segment의 마지막 미완결 frame | 검증된 crash fault profile에서만 제한적으로 무시: 알려진 필수 경계 뒤, 후속 유효 record/segment 없음, synced byte 보존 전제. 모호하면 오류 |
| seal 안의 손상, 중간 sequence 구멍, 후속 record가 있는 invalid frame | 오류. 뒤를 검색해 이어 붙이지 않음 |
| checksum이 다른 완결 frame | 기본 오류. 단순 “마지막 파일”이라는 이유로 삭제하지 않음 |
| valid commit이 참조하는 payload/page 누락 | 위치와 무관하게 오류. 부분 채택 금지 |
| 다른 DB/timeline/epoch의 파일 | 거절. 남은 이름·offset을 재사용해 연결하지 않음 |

tail 시험은 단순 파일 truncate에 한정하지 않는다. body/trailer의 일부 쓰기만 남아 파일 길이는 완전하지만 checksum이 다른 경우도 포함한다. 이런 경우의 자동 복구 허용 범위는 실제 filesystem·storage fault profile에서 검증하며, 현재의 보수적 오류 정책을 검증 없이 완화하지 않는다.

인수 결과는 [장애 판정표](dev2-kernel-design.md#fault-outcomes)의 `RECOVER_OK`, `REJECT_CORRUPTION`, `REJECT_IO`로 사전에 구분한다. 미sync 최후 frame의 full-length torn write는 정상 crash에서도 생길 수 있으며 현재는 개방 거절 대상이다. 그 검출 시험의 PASS를 ACK 보존 정상 reopen이나 모든 전원 장애의 자동 복구 성공으로 집계하지 않는다. 정상 복구가 필수인 사례의 오류를 사후 손상 거절로 바꾸지 않는다.

segment seal과 checkpoint의 resume 경계는 이미 확정된 영역을 판단하는 증거다. **checksum/LSN만으로 마지막 ACK record의 사후 완전 삭제와 아직 기록하지 않은 tail을 항상 구분할 수는 없다.** ACK 내구성 계약은 성공한 sync의 bytes/namespace가 선언한 장애에서 보존된다는 전제를 가진다. 장치가 그 계약을 어기거나 마지막 기록을 조용히 유실하는 경우까지 보호하려면 독립 복제·검증된 archive 등의 추가 증거가 필요하다. 이 한계를 “정상 tail 제거” 기능으로 감추지 않는다.

정상 startup과 salvage는 구분한다. salvage가 일부 손상 자료를 추출해도 ACK 보존 recovery 성공으로 보고하지 않는다. 정상 open 중 사용자 DB의 원본을 자동 truncate·수정하여 손상 증거를 지우지 않는다.

<a id="section-10"></a>

## 10. WAL·page·generation 회수와 자원 상한

하나의 retention service가 소유자별 pin을 관리한다. `CURRENT`, 지원하는 fallback checkpoint, reader, backup, archive/replication, 진행 중 build, **아직 replay에 필요한 commit의 COW roots**가 모두 소유자다. WAL와 page의 보존 이유는 다르므로 모든 reader가 항상 WAL을 pin한다고 단순화하지 않는다.

```text
NeededObjects = Reachable(current/fallback manifests, live views,
                          backup/build pins, retained replay commit roots)
NeededCommitRanges = union(recovery, fallback, backup, archive, replication ranges)
NeededPayload = union(refs of NeededCommitRanges, active preparation reservations)
reclaim(x) only if x ∉ corresponding Needed set
```

초기에는 conservative low watermark로 충분히 보존하고, segment의 일부만 필요해도 전체를 보존한다. L2에서 한 lane은 오래 준비한 transaction 때문에 q 순서와 물리 위치 순서가 다를 수 있다. **checkpoint 때 관측한 lane tail 이전을 일괄 삭제하면 이후 commit이 참조할 준비 payload까지 지운다.** 각 lane의 필요 참조/active reservation을 기준으로 계산한다. 서로 다른 lane LSN의 단순 최솟값은 의미가 없다.

GC는 조회→삭제 사이 새 pin이 생기는 경합을 막아야 한다. root 선택·pin 등록·회수 가능 generation 전이를 같은 synchronization protocol로 직렬화한다. retiring 대상으로 된 generation에 새 pin을 등록하면 거절/재시도하고, 삭제 파일명·space ID를 즉시 재사용하지 않는다. 삭제 중 crash 후 남은 파일은 누수 후보로 재조사하되 살아 있는 root가 참조하는 파일을 추측 삭제하지 않는다.

| 예산 | 제어 |
|---|---|
| transaction/group bytes·entry 수 | enqueue 전에 예약, 넘으면 분할 가능한 private staging 또는 명시적 거절. 공개 transaction 원자성은 유지 |
| lane queue / in-flight groups | bounded queue, backpressure, writer stall 진단, group deadline |
| snapshot/checkpoint workspace | pin 비용·최대 지속 시간·workspace 예약. checkpoint를 실패시켜도 기존 복구 기준 보존 |
| retained WAL/pages / free space | warning → admission 감속 → 새 write 거절. 필수 WAL을 삭제해 계속 쓰지 않음 |
| recovery workspace | bounded decoder·bounded disk buffer·별도 여유 메모리. DISK resident 부분 자체의 용량은 확보해야 함 |

오래된 read snapshot은 명시한 정책에 따라 종료시킬 수 있지만 committed recovery pin은 timeout으로 없앨 수 없다. replica가 느려 로그가 쌓이면 operator-visible 재동기화 전이로 consumer 계약을 바꿔야 한다. backup의 hold를 버리려면 해당 backup을 실패 처리한다. disk full 대비 metadata/종료용 reserve도 실제 filesystem에서 검증한다.

<a id="section-11"></a>

## 11. Backup·PITR·복제·버전 호환

초기 K5 backup은 **선택한 공통 checkpoint의 전체 파일 closure와 manifest를 빈 환경에서 복원**하는 범위다. 복사 시작 전에 해당 generation을 pin하고 완료 후 inventory/checksum·restore 결과를 검증한다. checkpoint `C` 자체로 복원하면 그 이후 WAL은 필요하지 않다. 최신 목표 `R`까지 복원하려면 `C+1..R` commit journal, 모든 참조 payload, 해당 commit들의 COW page closure도 필요하다.

DEV2 exact-cut restore는 원본 replay anchor를 사용하지 않고 검증한 C의 상태를 새 timeline의 base-q=C 시작 segment에 연결한 새 local manifest·CURRENT·WRITER를 영속 선택한다. 원본 manifest의 DB/timeline·digest는 provenance로 유지하며 원본 backup을 수정하지 않는다. source WAL 없이 복원·새 ACK·재시작을 검증한다. [DEV2 restore 규약](dev2-kernel-design.md#checkpoint)

따라서 지속 archive/PITR를 제공할 때 archive unit은 `commit group + payload + 새 COW 객체 + 식별/무결성 metadata`의 완결 집합이다. 그 전체가 archive storage에서 확인된 뒤 archive watermark를 전진시킨다. root-only journal을 복사해 놓고 “모든 변경의 redo가 있으니 PITR 가능”이라고 선언하지 않는다. 다른 선택은 DISK page redo를 로그에 넣는 후속 WAL-first 설계이며 현재 기본안과 별도다.

PITR는 완전한 commit group 경계의 `q_target`까지 적용한다. group 안 임의 transaction 시점과 wall-clock 기반 요청은 초기 범위에서 보장하지 않는다. 운영 UI가 시간→안전한 q 경계로 변환한 실제 복원 시점을 보여야 한다. 복원 후 새 timeline을 부여해 이전 primary의 계속된 WAL와 섞이지 않게 한다. 정상 crash restart는 같은 logical timeline에서 새 writer epoch를 쓴다.

복제는 물리 LogPos를 외부 transaction 순서로 노출하지 않고 `timeline,q,schema_epoch`를 소비한다. 물리 COW replication이면 참조 객체를 전송하고, logical replication이면 결정된 DISK row delta도 별도로 공급해야 한다. **root 참조만으로 논리 복제가 가능하지 않다.** 실제 RP는 DEV8 계획에 맞춰 설계·시험하며 현재 local commit 성공을 HA 성공으로 부르지 않는다.


<a id="section-12"></a>

## 12. 운영자가 보는 복구 상태

운영자가 checkpoint generation과 replay 구간의 역할을 추측하지 않게 한다. 다음은 **미구현 진단 API의 예시 필드**다. shell 명령이나 현재 지원 CLI가 아니다.

```json
{
  "phase": "REPLAY",
  "timeline": "example-timeline",
  "selected_generation": "G43",
  "checkpoint_commit_seq": 1200,
  "validated_commit_seq": 1430,
  "applied_commit_seq": 1370,
  "missing_dependency": null,
  "retention_blockers": [
    {"owner": "backup:B7", "reason": "restore closure", "oldest_q": 1001}
  ]
}
```

phase는 `PREFLIGHT → SELECT → LOAD → SCAN → REPLAY → VERIFY → STABILIZE → RESUME → READY`로 노출한다. 알려진 총량이 없으면 진행률을 꾸며내지 않고 처리 bytes/records·대기 원인·현재 속도·예상 범위를 보여 준다. 오류에는 generation/q/TxnId/lane/segment/object와 기대 digest·관측 digest를 연결하고 민감 row 값은 기본 수집하지 않는다.

필수 지표는 encode CPU, validation/rebase 시간·abort율, coordinator/queue wait, lane별 written/synced 차이, group 크기·대기, page sync·journal sync p50/p95/p99, publication 지연, checkpoint 생성/선택 시간·추가 bytes, pin별 보존량, replay bytes/s·apply transactions/s·peak RAM이다. 기존 로그 buffer 최적화가 효과 있는지와 storage 장치 한계인지를 분리한다.

```text
TPS 상한 ≤ min(validation/root-merge 처리량,
                payload 지속 대역폭 / 평균 payload bytes,
                COW 지속 대역폭 / 평균 새 page bytes,
                commit journal 처리량, publication 처리량)

RTO 추정 ≈ preflight/select + DISK load + journal scan/dependency fetch
         + redo apply + verification + 서비스 준비
```

RTO 목표가 주어지면 실측 보수 처리율로 replay budget을 계산해 checkpoint를 유도한다. 시간/로그 크기뿐 아니라 예상 복구량·free space·checkpoint I/O budget을 함께 사용한다. 본 설계는 임의의 “10배 향상”·“수초 복구”를 약속하지 않는다. COW sync 비용 때문에 DISK p99가 나쁘면 먼저 그 원인을 실측하고 최적화한다.

<a id="section-13"></a>

## 13. 구현 순서와 완료 증거

| 기존 단계 | 이 설계에서 구현할 것 | 완료 기준 |
|---|---|---|
| K0–K1 / XADB-027,006 | 실제 최소 row·catalog, private batch, snapshot/rollback, ID | 기존 K0/K1 직접 API와 독립 history |
| K2 / XADB-007 | L0 framing, append/sync, commit certainty, 공개 순서 | 실제 short write·오류·ACK 순서. production reopen은 K3까지 의무 |
| K3 / XADB-008 | 공통 recovery, tail/손상 분류, repeated recovery | ACK history와 실제 production reopen 대조 |
| K4 / XADB-010 | coarse serializable·statement rollback·실제 thread | 기존 correctness 기준. 병렬 group 성능으로 과장 금지 |
| K5 / XADB-009 | generation checkpoint, manifest 선택, retention, exact-cut backup/restore | 전체 cut fault·조기 회수 mutant·빈 환경 restore |
| DEV3 H1/S1 / XADB-021,022,029 | 영속 COW DISK + 같은 cut·bounded inventory/buffer | 디스크 저장 경로 원자성·page/WAL 보존·buffer 4배 working set |
| B2 이후, 다중 writer 성능 작업 | L1와 세밀한 serializable validation의 별도 작업 분해 | 같은 isolation/recovery oracle, abort·p99 포함 비교 |
| S1 이후 선택 성능 capability | L2 multi-stream, format 변환, bounded parallel recovery | L2 해당 사례 + 중앙 버전 정책이 요구하는 same-major version gate |
| DEV8–9 운영 기능 | 실제 replication/archive/PITR·예산/SLO qualification | 참조 객체까지 보존한 장애/restore·운영 검증 |

L2는 새로운 DEV3 완료 선행 조건도, SQL 착수 차단 조건도 아니다. 실제 착수 전 세부 task DAG·hardware profile·validation ADR를 추가한다. 현재 35개 seed DAG의 완료 상태를 임의로 바꾸지 않는다. format byte layout, checksum algorithm, page/chunk size, batch deadline/크기, lane 개수는 의미 계약을 보존하는 측정 대상으로 남긴다.

선택하지 않은 대안도 명시한다. table별 독립 WAL+독립 commit은 다중 객체 transaction의 원자성 때문에 배제한다. command logging은 SQL·시간·외부 효과 재실행 문제가 있어 초기 redo로 채택하지 않는다. 전역 순서를 없애는 dependency-vector logging은 연구 가치가 있지만 serializable/global-index/recovery 증명 부담을 늘리므로 첫 L2의 선행 조건으로 삼지 않는다. in-place ARIES 계열은 disk write amplification의 비교 대상이지만 별도 undo/CLR/pageLSN 계약이 필요하다.

<a id="section-14"></a>

## 14. 검증 계획과 이번 작업의 검증 범위

[인수 matrix](recovery-test-matrix.json)는 L0·checkpoint·디스크 저장·L1/L2·운영 기능의 필수 사례를 구분한다. **모든 제품 사례는 NOT_RUN이다.** 기존 KU/DISK 시험과 연결하고 미래 L2를 B2의 의존성으로 넣지 않는다. oracle은 candidate의 `synced=true` 로그가 아니라 독립 controller의 ACK/ABORT/UNKNOWN 이력과 row/index/catalog 상태다.

핵심 반례는 다음과 같다.

- payload lane 하나가 유실됐는데 commit certificate만 남은 상태를 정상으로 여는 오류.
- 예약 offset의 최댓값을 durable prefix로 사용해 구멍 뒤 transaction에 ACK하는 오류.
- 같은 옛 root에서 만든 두 transaction root를 병합 없이 교체해 앞 변경을 잃는 오류.
- G43 선택이 영속화되기 전에 G42의 replay 로그를 지우는 오류.
- G42 fallback을 남겨 놓고 G42 이후 WAL/COW root를 먼저 회수하는 오류.
- 늦게 commit될 transaction의 오래된 payload를 checkpoint lane tail 기준으로 삭제하는 오류.
- recovery progress만 영속화하여 반복 장애에서 일부 redo를 건너뛰는 오류.
- DISK root-only WAL 백업을 완전한 PITR archive로 오인하는 오류.

성능 비교는 동일 Linux/FS/storage, 동일 local-durable ACK, 같은 isolation·합성 workload에서 L0→L1→L2를 비교한다. actor 수·hot key 비율·batch 크기·disk row·checkpoint 유무·stream/장치 수를 바꾸고 성공 TPS·p99·abort율·CPU·I/O·write amplification·recovery 시간을 함께 기록한다. 성능 합격 수치는 실제 고객 workload/SLO 확정 후 정하며 임의 숫자로 PASS를 만들지 않는다.



<a id="section-15"></a>

## 15. 재검토 원칙: 기존 설계 보존과 검증된 개선

**기존 상세 설계와 L0/L1/L2 구분은 유지한다.** 목표는 Oracle·ARIES에서 검증된 내구성·복구 원칙을 충족하면서, 이후 개선이 현재의 구현 결합 때문에 막히지 않게 하는 것이다. 최신 논문을 이유로 기존 설계를 삭제하거나 새로운 알고리즘을 채택하지 않는다. Oracle의 모든 기능·성능을 현재 구현했다는 의미도 아니다.

기존 재장애 연결·torn-write 검토에 이어 [DEV2-DESIGN-002](dev2-kernel-design.md#recovery-review-decision)는 이전 prefix의 STABILIZE 장벽, checkpoint replay anchor와 selector·GC 순서, 장애별 기대 결과를 구체화한다. **첫 process crash → cache에 남은 미sync record의 recovery → 새 거래 ACK → power-loss model → 두 번째 recovery**에서도 하나의 합법적 이력과 ACK가 보존되어야 한다. 기존 28개 인수 계획 안의 필수 variant를 보강하며 모두 미실행 상태다. F03–05의 byte vector·전체 crash cut·실제 환경 검증은 계속 구현 전 의무다.

미래에 적용할 알고리즘을 미리 설계할 필요는 없다. 다만 transaction 의미를 log 파일 수·writer thread에, 거래 순서를 byte offset에, recovery를 SQL 재실행이나 runtime pointer에 직접 묶지 않는다. 현재 total order·COW·generation checkpoint는 유지하면서 구현 책임을 분리한다. 새 연구가 구조 변경을 요구하면 그때 같은 내구성·원자성 기준으로 검토한다. 범용 plugin framework나 미래 알고리즘용 format 필드는 지금 추가하지 않는다.

| 비교 근거 | 판단과 적용 범위 |
|---|---|
| [Oracle 26 LGWR/DBWn](https://docs.oracle.com/en/database/oracle/oracle-database/26/cncpt/process-architecture.html) | redo 내구성과 data write를 분리하는 검증된 비교 기준. 기존 DISK data-before-commit 방식의 비용은 H1 구현 때 WAL-first와 비교할 가치가 있으나 이번에 기본 계약을 교체하지 않음 |
| [ARIES, 1992](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf) | WAL·반복 장애·redo/undo 의미의 기준. XADB private write와 다른 전제를 가진 loser undo/CLR을 그대로 이식하지 않음 |
| [WiredTiger logging](https://source.wiredtiger.com/11.3.1/arch-logging.html) | operation log와 checkpoint를 결합하는 비교 사례. 같은 저장 방식이라는 이유로 성능·내구성 설정을 동일시하지 않음 |
| [Rethinking Logging, Checkpoints, and Recovery, 2020](https://db.in.tum.de/~leis/papers/rethinkingLogging.pdf) | 로그 처리량과 checkpoint·복구 시간을 함께 평가하는 참고 자료 |
| [Autonomous Commit, SIGMOD 2025](https://2025.sigmod.org/toc-3-3.html) | group 대기와 ACK 처리의 병목을 측정할 때 참고. 알고리즘 도입·성능 재현은 하지 않음 |
| [How to Write to SSDs, PVLDB 2026](https://www.vldb.org/pvldb/vol19/p1469-lee.pdf) | out-of-place write와 GC 비용을 평가할 때 참고. 특수 하드웨어나 새 page 배치를 요구하지 않음 |
| [BtrLog, 2026 preprint v2](https://arxiv.org/html/2606.27051v2) | cloud WAL 연구 후보. 현재 local recovery 구조의 근거로 채택하지 않음 |

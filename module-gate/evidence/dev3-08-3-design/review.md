Verdict: ACCEPT

# D2 독립 검토 3회차 — 08-3 overflow chain 쓰기·읽기·반환 설계(O0 3판)

## 대상

- branch `claude/project-thread-y6amas`, commit `a5108d8`(`f6fbcf9` 위 수정 commit)
- 범위는 `git diff f6fbcf9 a5108d8`이다.
  - `devdoc/contracts/dev3-row-overflow.md` 3판(407행, 바뀐 줄은 44줄)
  - `contracts/README.md` 상태 줄, `document-validation.json` 해시
  - `module-gate/evidence/dev3-08-3-design/review-r2.md`·`frames-r2.py` 추가
- 판정 요약:
  - 2회차의 열린 필수 N1·N2가 코드 기준으로 해소됐다.
  - 권고 P1–P8·NIT Q1–Q2도 모두 반영됐다.
  - 새로 정한 세 규칙을 따로 점검했다. 셋 모두 08-3 범위에서 건전하다.
    - allocator incarnation 상계
    - role 7의 "현재 image == INIT image"
    - preflight 뒤의 Reserved allocator entry
  - 남은 것은 권고 1건과 NIT 4건이다. 수용을 막지 않는다.
  - 이 판정은 설계 문서 범위에 한한다. O1–O4의 구현·시험·census는 단계마다 따로 D2를 받는다.

## 검토 방법

- **diff 대조.** `git diff f6fbcf9 a5108d8`의 추가·삭제 줄을 모두 2회차 지적과 대조했다. diff에 지적과 무관한 변경은 없다.
- **보관본과 해시.**
  - `review-r2.md`·`frames-r2.py`가 2회차 산출물과 byte 단위로 같다(`diff`).
  - README·계약 문서의 SHA-256이 `document-validation.json`의 값과 같다.
- **새 서술이 기대는 코드를 다시 읽었다**(읽기만).
  - **Base4 allocator incarnation 상계.** `plan4_recovery_base/catalog/graph/relations.rs`의 `validate`(478–533). 여기서 `creates = object_high − 5`에는 취소된 CREATE 시도도 들어간다.
  - **Free로 돌아가는 모든 경로.** `semantic_catalog/images/allocator.rs:52–64`(CREATE Reserve·Forward·Cancel·Clr), `allocation_reservation/operation/plan4_retention.rs:105–125`(System 예약·취소), `history/catalog.rs:686–695`(CLR이 arena slot과 claim을 지움)이다.
  - **role 7 provenance 규칙.** `relations::provenance`(586–668)의 Undo generation 1에 쓰는 같은 형태의 규칙(`cert lsn == init_lsn`), `PageHeader`(`page.rs:125–130`, epoch 칸 없음), flush 경로에서 page bytes를 고치는 곳이 없음을 확인했다.
  - **System 예약의 발행자와 배타성.**
    - `plan4_retention/population.rs:702–730`: `reserve_system`은 `&mut Plan4Retention`(= `DiskSession`)에서만 발행된다. 예약이 이미 있으면 Unchanged Busy를 낸다.
    - `ReservedRetentionSystem`이 active인 채로 drop되면 session fence가 걸린다(184–198).
    - `plan4_apply.rs:516–532`: `MutationPort`는 `&mut RetentionLedger`를 가진다. `MaintenancePort`는 flush 제어만 가진다.
  - **A의 실패 분류.** `row/insert/driver.rs:1185–1215`: split은 value check·Reserve의 Capacity에서만 요청하고, 그 밖의 preflight 거절은 그 행을 끝낸다.
  - **census.** `jobs.rs:311·340–361`은 role로 분기한다. `retention_page/row/census.rs:180–206`, `ledger.rs:155–158`도 다시 읽었다.
  - **Base4 decode의 나머지 전체 순회.** `plan4_recovery_base/row.rs:157·241–260·295`, `row/actor.rs:655–700`, `row/graph.rs:378–400·512–535`, `catalog/inventory/current.rs:150–170`에 kind 7을 막는 다른 분기가 없음을 확인했다.
- **수치.** 보호 WAL `9W+F`와 `cleanup`(h=1)을 다시 계산했다.
- `/home/user/xadb`와 `/mnt/project-files`는 바꾸지 않았다. build·시험은 돌리지 않았다.

## 판정

- **열린 필수 0**
- **권고 1**(S1), **NIT 4**(T1–T4)

## 2회차 항목 상태

### N1 (Base4가 role 7을 CREATE pair로 읽음) — 해소

§8.3 309–313행이 2회차가 든 네 소비자를 하나씩 바꾼다. claim 생성은 §8.1 290행이 정한다.

- **accessor.**
  - `pair()`는 role 3|4에서만 `Some`이다. 그래서 지금 source kind 2이면 `[232,256)`을 `expect`로 decode하던 panic(`image_records.rs:94–101`)이 사라진다.
  - `chunk()`가 role 7 칸을 따로 읽는다.
  - `object()`는 `[184,208)`이 같으므로 그대로 쓴다. `current_tree.rs:202–212`의 claim 대조도 이 accessor에 기대므로 맞다.
- **`validate_pairs`.** role 7은 짝 검사에서 빠진다. 대신 anchor ordinal의 role 3 claim과 대조한다. 대조할 칸의 표현에 대해서는 S1을 본다.
- **`relations::provenance`.** role 7은 다시 encode하지 않고 `cert lsn == init_lsn`과 `content_digest(현재 image) == initial_digest`를 요구한다. 건전성은 아래 "집중 점검 2"에서 따로 본다.
- **`payloads`.** `_ => Unsupported` 앞에 Overflow 분기를 둔다(owner·generation·level). 소유는 `chunks` closure가 본다.
- **claim 생성.** `overflow_init_record`는 `init_record`와 header `[0,128)`·source `[128,176)`·`@176`만 공유한다. `init_record`가 role 7을 만들 수 없는 이유(`history/catalog.rs:209–215`의 role 선택, 243–249의 pair 조건)도 맞게 적었다.
- **INIT 전 arena slot·claim zero.** 이 조건이 모든 Free page에서 성립하는지 확인했다.
  - page가 Free로 돌아가는 경로는 셋이다. CREATE Cancel(INIT 전), CREATE CLR(arena·claim을 지움, `history/catalog.rs:690–694`), System 예약 취소(INIT 전)다.
  - 그러므로 이전에 쓰인 Free page도 zero다. 이 조건 때문에 정당한 ChunkWrite가 거절되는 일은 없다.
- **시험.** OVF-F06(O1 codec), OVF-C04(claim zero 조건), OVF-C05(decode 전 과정과 각 손상)가 이 변경을 덮는다.

### N2 (§4.3 `[288,…)` 칸) — 해소

- 95행은 이제 Reserved·Retained `[288,320)`을 XRP1 projection으로 적는다(`job.rs:246–247`).
- ReleasingCompensated는 CLR이 projection을 지우고 `[288,292)` total, `[292,320)` zero를 쓴다. CLR의 바꿀 수 있는 구간 `(285,320)`(`jobs.rs:207`) 안이다.
- ChainRetire가 total을 지우는 구간 `(288,292)`, ReleasingCommitted의 zero(Prune의 `(285,320)`)도 코드와 맞는다.
- OVF-F02의 "§4.3 표의 네 state 조합"은 이제 올바른 기대값을 준다.

### 권고·NIT 상태

| 항목 | 상태 | 3판 위치와 확인 |
| --- | --- | --- |
| P1 census 목록, `cleanup_started()` | 반영 | 160행, 104행. ChunkWrite를 TerminalLeased 목록에 넣고 `reserved == 1`을 둔다. ChainRetire는 기본 분기(TerminalReady)로 간다. 이는 `jobs.rs:340–361`과 맞는다. `cleanup_started()`를 쓰면 `ledger.rs:155–158`과 `census.rs:200–205`가 그대로 성립한다. ChainRetire LSN > terminal LSN이기 때문이다. 작은 점은 T2·T3에 적는다 |
| P2 kind 10 slot 1 | 반영 | 140행. `wire.rs:441–445`의 slot 분기에 더한다. offset 하한 `HEADER_BYTES + 4·(slot+1)`도 slot 1에서 성립한다 |
| P3 §7.6 Reserved 항 | 반영 | 277행. 항을 지우고 근거(`candidate.rs:375–377`)를 적었다. OVF-C06에 거절 사례가 있다 |
| P4 `op != op.role()` | 반영 | 142행, §13 407행. `Operation::delete()` |
| P5 incarnation 상계 | 반영(규칙 변경) | 314행. 아래 집중 점검 1 |
| P6 Reserved allocator entry | 반영 | 157·233행. 아래 집중 점검 3, 분류는 T1 |
| P7 보호 WAL | 반영 | 196행. 9×56,976 + 91,676 = **604,460**, 312,096 + 128 = **312,224**로 다시 계산한 값과 같다 |
| P8 claim 생성 | 반영 | 290행(N1과 같음) |
| Q1 `validate_admission` | 반영 | 192행. `ledger.rs:340–368`과 맞는다 |
| Q2 ChunkWrite 구간 | 반영 | 104행 다음 줄, `(328,360)`·`(376,384)`. ChunkWrite는 `cleanup()`을 바꾸지 않으므로 `validate_budget`과 맞는다 |

## 집중 점검

### 1. allocator incarnation 상계 — 건전하다

- **규칙.** 지금 규칙은 각 entry의 `incarnation − 1 ≤ System HW + creates + Structural HW`다(`relations.rs:520–533`).
- **incarnation의 뜻.** 첫 extent의 page는 Free(1)에서 시작한다. 할당될 때마다 `next_reservation_generation` = g+1이 된다(`allocation_page.rs:259–264`). Free로 돌아가도 g는 유지된다(`allocator.rs:58–63`). 그러므로 `incarnation − 1`은 그 page가 할당된 횟수다.
- **ChunkWrite가 없을 때.** 한 할당 사건(System INIT, CREATE, split)은 같은 page를 두 번 할당하지 않는다. 그래서 page P의 할당 횟수는 사건 수의 합 이하다.
- **3판의 규칙.** 08-3에서 chunk page는 Free로 돌아가지 않는다(반환은 XTR1). 그러므로 P를 할당한 ChunkWrite는 많아야 하나이고, 그런 P는 지금 `Allocated(…, Overflow)`다.
  - 따라서 allocs(P) ≤ (원래 세 항) + [P가 kind 7] ≤ (원래 세 항) + (kind 7 Allocated page 수)다.
  - 3판이 더하는 항이 이것이므로 상계가 성립한다.
  - CREATE가 rollback으로 돌려준 page를 chunk로 다시 쓰는 경우도 같다. 그 CREATE는 `creates`(취소 시도 포함, `object_high − 5`)에 들어 있다.
- **14-2.** 재사용이 생기면 "많아야 하나"가 깨진다. 이를 §13 405행에 남긴 것도 맞다.
- 표현의 작은 점은 T4에 적는다.

### 2. role 7의 "현재 image == INIT image" — 08-3 범위에서 건전하다

- **chunk page를 쓰는 것이 INIT 하나뿐인지 확인했다.**
  - ChunkWrite만 INIT한다.
  - Forward·OverflowCancel·ChainRetire·B는 읽기만 한다(§5 157행).
  - Prune은 leaf만 바꾸고, ChainRetire·OverflowCancel은 anchor만 바꾼다.
  - SMO는 Free page만 고른다(`candidate.rs:540–555`).
  - flush·DWB·replay는 같은 bytes를 옮길 뿐이다. `PageHeader`에는 identity·kind·level·lsn만 있다. 쓰기 때마다 바뀌는 칸(epoch, 쓰기 횟수)이 없다.
- **따라서 base의 image가 INIT image와 같다.** base가 담는 image(`Sections.images`)는 INIT redo가 설치한 bytes와 같다. 그래서 `content_digest`가 claim의 `initial_digest`와 같다. 이것은 `init_record`가 기록하는 `content_digest(initial)`이다(`history/catalog.rs:226`).
- **pageLSN == init_lsn도 같은 이유로 성립한다.** 지금 Base4가 Undo generation 1에 쓰는 규칙(`cert lsn == init_lsn`, `relations.rs:370·436`)과 같은 형태다.
- **증거 수준.** 이 규칙은 CREATE의 canonical 재-encode와 같은 수준의 "형태 증거"다. INIT이 실제로 일어났는지는 지금처럼 원 replay가 증명한다.
- **반환된 chunk.** XTR1에 들어간 chunk도 page는 그대로다. 그래서 `pageLSN < retire_lsn`(§4.4)과 함께 성립한다.
- **14-2.** 재사용하면 이 규칙이 깨진다. 이것도 §13 404행에 적혀 있다.

### 3. preflight 뒤 Reserved allocator entry의 Unchanged OrderMismatch — live에서 도달하지 않고, 도달해도 해가 없다

- **Reserved entry를 만드는 곳은 둘뿐이다.**
  - System 예약: `DiskSession::reserve_system`, `population.rs:714–730`
  - Catalog CREATE의 Reserve
  - 둘 다 같은 writer session의 `&mut` 경로다.
- **INSERT 도중에는 다른 writer가 없다.**
  - INSERT 실행기는 preflight부터 Forward까지 `&mut MutationPort`를 쥔다(`driver/live.rs:195–206`).
  - `MutationPort`가 `&mut RetentionLedger`를 빌리므로, 그동안 같은 session에서 다른 writer가 끼지 못한다.
  - FlushRequired 처리는 `MaintenancePort`(flush 제어만, `plan4_apply.rs:528–532`)로 한다.
- **System 예약은 scope 객체다.** active인 채 drop되면 session이 fence된다(`population.rs:191–198`). 정상 경로에서는 INSERT가 시작될 때 Reserved entry가 남아 있지 않다.
- **결론.** 157행의 "preflight 뒤에 생기면 Unchanged OrderMismatch"는 live에서 만들 수 없는 방어 분기다. 의미는 replay가 같은 조건으로 그런 ChunkWrite를 거절한다는 데 있다.
- **도달해도 해가 없다.**
  - A는 이를 Unchanged로 받아 §7.1의 4로 간다. written ≥ 1이면 OverflowCancel, 0이면 Cancel이다.
  - OverflowCancel은 Root·Job·anchor만 쓰고 allocator를 쓰지 않는다(§4.5). 그래서 System batch의 "allocator pageLSN = birth" 규칙(`allocation_reservation/operation.rs:720–737`)을 깨지 않는다.
  - 재시도가 없으므로 끝이 있다.
  - 대가는 쓴 chunk가 XTR1에 남아 14-2까지 첫 extent를 차지한다는 것뿐이다(§9 321행에 이미 있는 대가).
- **preflight 시점.** Reserved entry가 보이는 것은 fence된 session이나 복구 도중처럼 비정상 상태뿐이다. 그때는 첫 효과 전에 거절된다. 그 오류 분류는 T1에 적는다.

## 권고

- **S1. §8.3 311행 `validate_pairs`의 role 7 대조 대상을 분명히 적는다.**
  - "source·object가 그 table과 맞는지"는 두 가지로 읽힌다.
  - role 7의 source는 큰 행을 INSERT한 거래다. role 3 claim의 source는 CREATE 거래다. 둘은 보통 다르다.
  - "source == CREATE claim의 source"로 구현하면 CREATE와 다른 거래가 넣은 모든 큰 행에서 Base4가 거절한다.
  - 다음과 같이 적기를 권한다.
    - role 7의 anchor ordinal에 role 3 claim이 있다.
    - 두 claim의 `object()`가 같다.
    - role 7의 source kind는 2(Transaction)이다.
    - actor는 `within_source`(`relations.rs:642`) 범위 안이다. CREATE의 source와 같을 필요는 없다.
  - OVF-C05에 "CREATE와 다른 거래가 쓴 chunk의 claim 정상"을 넣는다.

## NIT

- **T1.** 233행은 preflight에서 Reserved allocator entry를 **Capacity**로 분류한다. 지금 System 경로는 예약이 이미 있을 때 Unchanged **Busy**를 낸다(`population.rs:719–721`). split audit는 OrderMismatch다. 분류를 Busy로 맞추거나 Capacity로 둔 이유를 한 줄 적는다.
  - 어느 쪽이든 preflight 거절이므로 A는 split을 요청하지 않고 그 행을 끝낸다(`driver.rs:1185–1215`). 동작에는 차이가 없다.
- **T2.** 160행은 "ChunkWrite와 OverflowCancel(role Cancel)을 TerminalLeased 목록에 더한다"고 한다. 그런데 census는 role로 분기하므로(`jobs.rs:311`) OverflowCancel은 이미 Cancel로 그 목록에 있다. 대신 `reserved == 1`은 기존 Cancel에도 붙는다. Cancel의 before에는 언제나 그 Reserved root가 있으므로 성립한다고 적어 두면 분명하다.
- **T3.** `retention_page/row/census.rs`에서 cursor만 보는 곳은 terminal 순서 규칙(200–205)뿐이 아니다. 유효 state 규칙(187–190, `release_cursor() == 0 && TerminalLeased`)도 있다. 104행의 "두 곳"에 이 줄도 `cleanup_started()`로 바꾼다고 적는다. 그대로 두어도 ChainRetire는 TerminalReady에서만 열리므로 틀리지는 않는다. 다만 판정이 약해진다.
- **T4.** 314행의 두 표현을 고친다.
  - 예시 "반환된 Undo page"는 지금 코드에 없는 경로다. allocator에서 Undo page를 Free로 돌리는 전이는 없다. 실제 경로는 rollback된 CREATE의 page다.
  - "table 소유 kind 7 Allocated page 수"는 상계가 전역 하나이므로 "모든 table의 kind 7 Allocated page 수"로 적는 편이 정확하다. page마다 그 page가 kind 7이면 1 이상이면 충분하므로 논증은 어느 쪽이든 성립한다.

## 수용 범위와 남은 의무

- **수용한 것.** 3판 설계 전체다.
  - 범위, A4 쓰기 순서, chunk 형식 개정, Job3 overflow root
  - XTR1 Overflow 항목과 kind별 revision·LSN 규칙
  - 세 opcode의 wire·frame·role·Root/descriptor 효과
  - 전이, 비용·ledger·보호 WAL, Forward 검산, A/B/C 경로
  - inline 지점 분류, SMO 개수 closure, replay·claim, Base4 형식과 소비자
  - 시험 계획, 단계 O1–O4
- **이 수용이 아닌 것.** 구현과 그 증거다. 시험은 모두 NOT_RUN이다. 08-3 적립은 계약대로 O4 병합 뒤에만 한다.
- **실측으로 넘긴 조건.** 다음은 설계가 단계 census로 확인하기로 한 것이다(§11). 맞지 않으면 상한을 올리지 않고 계약을 다시 검토한다(§7.1·§11).
  - C lifecycle 여유 605, preflight C 여유 182
  - live retirement·CLR_PRODUCER·Row write 지점
  - A drain 여유 14 B와 ChainRetire의 never-inline 하위 단계
  - F4 caller 상한 전부, B read census
- **단계 D2에서 볼 것.**
  - O2·O3: §7.5의 비시험 inline 사용처 전체 분류 목록(263행)
  - O1: OVF-F01·F06의 독립 Python 기대값
  - 위 S1의 대조 대상

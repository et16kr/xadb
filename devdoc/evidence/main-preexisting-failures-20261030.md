# main 기존 실패 디버깅: commit window native 초과와 F05 recovery oracle

2026-10-30 · 기준 main `85ceef3` · branch `claude/project-thread-4dsfsh`

모듈 독립 작업의 로컬 CR1이 main부터 있던 두 실패 때문에 막혔다. 사용자는 이 실패를 따로 디버깅하도록 요청했다. 이 기록은 개발이 아닌 원인 조사이며, [AGENTS.md](../../AGENTS.md)의 디버깅 구분에 따라 원인 확인에 필요한 진단만 실행했다. 원래 실패 기록은 [모듈 독립 조사](module-independence-survey-20261026.md)와 공유 폴더의 `main-preexisting-check-20261029/`에 그대로 남는다.

조사는 main `85ceef3`에서 했고, 사용자 요청에 따라 결과는 main이 아니라 모듈 독립 작업 branch `claude/project-thread-h2j8oc`에 반영한다. 아래 §1의 시험 adapter 변경은 그 branch의 `94ede44`에 같은 내용으로 들어가 있으며, CR1에서 F05를 뺀 결정은 `ff4fc8b`가 그 branch의 계획 문서에 반영했다. 이 문서는 그 두 commit의 원인 조사 기록이다.

## 1. commit window native 초과

`plan4_apply/source_tests.rs`의 `catalog_commit_windows_fit_beside_admitted_readers`는 Pending writer 창과 admitted Full 조회의 합이 65536B 이하인지 본다. main에서 memory는 맞고 native만 넘었다.

| 형식 | shared | pending | full | pending_full |
| --- | ---: | ---: | ---: | ---: |
| memory port | 20192 | 10698 | 34230 | 65120 |
| native 시험 adapter | 20328 | 10698 | 35254 | **66280** |

차이1160B는 shared +136B와 full +1024B다. `recovery_source_*`의 type 출력에서 native adapter `File`은160B, memory port `File`은32B이고 `Writer`는16B와8B다. adapter `File`은 trace 이름 `FileName`(128B 배열+길이)을 값 안에 들고 있었다. Full 조회 census는 `W::File`을8번 세므로 +1024B가 되고, shared는 File1개와 Writer1개를 세어 +136B가 된다. 제품 native 타입 `LinuxNamespaceFile`은 fd와 lease Arc뿐이며 이 이름을 갖지 않는다.

**수정.** 시험 전용 adapter의 trace 이름을 `Box<FileName>`으로 옮겼다. 이름은 trace 출력에만 쓰이고 동작은 같다. native `File`은32B가 되어 memory port와 같아진다. 제품 코드·한도·창 구성·charge 식은 바꾸지 않았다.

| 대상 | 수정 전 native pending_full | 수정 후 | 판정 |
| --- | ---: | ---: | --- |
| main `85ceef3` | 66280 | 65128 | FAIL → PASS |
| 모듈 branch `52aa882`에 같은 patch | 66612 | 65460 | FAIL → PASS (여유76B) |

모듈 branch의 memory 값은65452B(여유84B)이며, 수정 뒤 native는 memory보다 Writer 차이8B만 크다.

**같은 adapter를 쓰는 시험의 영향.** adapter를 참조하는 source·census 시험과 이름에 native가 들어간 시험 중 ignored가 아닌92개를 수정 전 main build와 수정 build에서 각각1회 실행했다. 결과가 바뀐 것은 이 commit window 시험1개뿐이다. 나머지 기존 실패14개는 같은 위치에서 그대로 실패한다. 그중 `retention_source_native`는 양쪽 값이 함께128B 줄었고(187144/124040 → 187016/123912) 여전히 불일치다. `runtime_core_source_native`는 MutableStorageOwner가 File1개를 담으므로992B → 864B가 되었다. 이 시험이 고정한 native 과거값984B도 같은 adapter 크기에서 나온 값이므로856B로 맞췄다. 이제 memory(856 vs 848)와 native(864 vs 856) 모두 같은 +8B로 실패하며, 그 +8B 증가는 이번 원인과 별개인 기존 실패로 남긴다.

실행하지 않은 것: adapter로 실제 I/O를 하는 native worker들은 모두 ignored이고 controller와 독립 admission이 필요하다. Catalog native N01–N03은 60GiB 여유 하한도 필요해 이 cloud container(약30GB)에서 돌리지 않았다. adapter 변경은 trace 이름의 저장 위치뿐이다.

## 2. F05 recovery native와 `support.rs:260`

`plan4_recovery/tests/support.rs`의 `fresh()`는 fresh 저장소의 첫 Reserve WAL frame을 고정 oracle(`tests/fixtures/dev3-plan4-empty-retention/vectors/p1-o00`와 `plan4_retention/literal_tests.rs`의 population recipe)과 비교한다. F05 N01(produce)도 같은 `fresh()`로 시작하므로 이 줄에서 멈춘다.

### 2.1 byte 차이

21200B frame에서 다른 곳은 다음뿐이다.

| frame offset | oracle | 실제 | 의미 |
| --- | --- | --- | --- |
| 164 | 1 | 2 | XSG1 version |
| 170 | 0 | 1 | flags Plan4SystemInit |
| 480..512 | `c440c314…` | `b8aab814…` | descriptor의 profile digest |
| 1264 (desc 912) | 38000 | 38208 | terminal WAL 예약 |
| 1360, 1680 (desc 1008, 1328) | 70768 | 70976 | protected 예약 |
| 1856..1888, 21168..21200 | | | descriptor·frame seal |

앞의 둘과 예약값은 [Free Undo 계약](../contracts/dev3-plan4-free-undo.md)의 §2 그대로다. 새 plan4 Reserve는 XSG1 v2/flags1 scope와 최악 INIT38208B를 예약한다. profile digest `b8aab814…`는 `tests/fixtures/dev3-catalog-create/*/raw/`의 Profile2 파일과 같고, `c440c314…`는 P1 profile이다. `FreshStorageSession::create_retention_catalog`는 언제나 `encode_catalog_profile`(format2)로 profile을 쓴다. oracle은 이 두 변경 이전 형식을 기대하고 있다.

### 2.2 oracle만 고치면 되는가

진단용 사본에서 population recipe를 v2 scope·Profile2 digest·38208/70976으로 바꾸고 `fresh()`의 비교를 기록만 하도록 바꾼 뒤 `recovery_closure`의 V01을1회 실행했다. Reserve와 Status INIT frame은 byte 단위로 일치했다. 그 다음 `aging_start`가 `RetentionError { phase: Prepare, cause: WrongState, effect: Unchanged }`로 거절됐다.

원인은 `plan4_retention/live.rs`의 `change()`다. 옛 N=0 빈 거래 producer(aging/begin/rollback/abort/finish)는 profile format1에서만 동작하고, Profile2 이상은 별도 scope2 producer로 가야 한다며 명시 거절한다. 제품은 이제 format1 plan4 저장소를 만들지 않으므로 `fresh()`의 3–7단계, `second_cycle`, `third_cycle`은 어떤 fresh 저장소에서도 실행될 수 없다. 모듈 branch도 Profile3을 current로 쓰고 같은 거절을 유지한다.

즉 F05와 `plan4_recovery`의 closure·cases·faults·physical·publication 시험은 제품 결함이 아니라 현재 형식으로 옮겨지지 않은 시험이다. oracle 숫자만 바꾸는 것은 해결이 아니다. 같은 population recipe를 쓰는 `retention_live_lifecycle`(`lifecycle_tests.rs:302`)도 같은 Reserve frame 차이로 실패한다.

### 2.3 사용자 결정

두 선택지를 사용자에게 제시했다.

1. F05를 현재 형식으로 다시 만든다. scope2 producer 위의 시나리오, Python stdlib으로 만든 새 독립 fixture, D2 검토가 필요한 새 단위다.
2. F05를 CR1에서 뺀다. 현재 형식의 native 복구는 Catalog native N01–N03이 맡고, 옛 P4-REC 시험은 이관 전 시험으로 기록한다.

2026-10-30 09:50 KST 사용자는 **2(CR1에서 F05 빼기)**를 골랐다. 따라서 CR1은 Catalog native N01–N03만 실행한다. Catalog native는 produce→SIGKILL→recover→reentry를 여러 process로 돌리므로 현재 형식의 장애 후 native 복구를 직접 다룬다. P4-REC-V·N 시험과 `support.rs:260` 실패는 고치거나 끄지 않고 Profile1 이관 전 시험으로 남긴다. 이 선택을 F05 PASS로 표시하지 않으며, 옛 시험의 FAIL 기록도 그대로 보존한다.

## 3. 보존 자료

공유 폴더 `main-preexisting-debug-20261030/`에 두었다.

- `commit-window/`: main·adapter 수정·branch 수정·모듈 branch patch의 census 로그, 92개 선택과 전후 결과표, 실행 script.
- `f05-oracle-experiment/`: 진단용 patch(커밋하지 않음)와 V01 로그.

| identity | SHA-256 |
| --- | --- |
| branch 수정 test ELF | `2a26e0cb1ed9beefdf28498b02ad9a2d449b05d08911d1df6f48ddb8c820334a` |
| 모듈 branch `52aa882`+adapter patch test ELF | `5a9f775a51a9fff47f2cc11cf64d43cb7ac566cdf1ffb38076e1019a6c52df52` |
| F05 oracle 진단 ELF | `4c5ade269d2b36aa8090fae7000ac94c28be5d9cf8432cce48c7ed4c3a9f657d` |

92개 전후 비교에 쓴 main build와 adapter-only build의 ELF hash는 같은 경로의 다음 build로 덮여 남기지 못했다. 그 source는 각각 main `85ceef3`과 main에 adapter 변경만 더한 것이다.

<a id="stale-census-pins"></a>
## 4. `retention_source_*`·`runtime_core_source_*`의 낡은 고정값

2026-10-30 · branch `claude/project-thread-kzarn5`(기준 `06a260e`, 모듈 독립 branch `claude/project-thread-h2j8oc`의 head와 같음). §1이 별개로 남긴 +8B와 `plan4_retention/tests.rs:140` 실패를 이어서 조사했다. 네 시험은 main `85ceef3`에서도 같은 자리에서 실패한다. 이 저장소의 git 이력은 2026-10-27의 두 root `df9881b`·`8c0f252`에서 시작하고, 두 root에 이미 아래 증가와 옛 고정값이 함께 있다. 그래서 증가를 들인 단위는 commit이 아니라 계약·증거 기록으로 찾았다.

### 4.1 metadata 항등식

`retention_source_*`의 `metadata == peak + 36×1664`는 [D3-RETENTION-LIVE-001](../contracts/dev3-retention-live.md)(2026-10-25) 때의 식이다. 그때 memory 측정은 peak63520·metadata123424로 차이가 정확히59904였다. 그 뒤 metadata에 세 항이 더해졌다.

- visibility registry8192B. [visibility 계약](../contracts/dev3-visibility-retirement.md) §2(D3-CATALOG-CREATE-001 결합 D2, 2026-10-25)는 backing을 caller loan으로 옮겨도 retained metadata에서 빼지 않는다고 정한다.
- epoch1600+isolation4160=5760B와 장기 caller handle49152B. [D3-ROW-RESOURCE-001](../contracts/dev3-resource-bounds.md#row-missing-module-budget)(2026-10-26 D2 ACCEPTED)이 고정 소계 `36×1664+8192=68096`, backing 포함73856, `metadata≤73856+65536+49152`를 정한다.

제품 식 `plan4_retention/resources.rs`는 이 계약과 같고, `plan4_apply/owner_tests.rs`가 이미 `metadata−peak == 73856+LONG_SOURCE_BYTES`를 단언한다. 실패 차이63104는 8192+5760+49152다. 틀린 것은 제품이 아니라 시험 기대값이다. [모듈 독립 조사](module-independence-survey-20261026.md#c2-writer-progress)도 같은 분해를 기록하고 후속으로 남겼다.

### 4.2 MutableStorageOwner +8B

memory에서 runtime376→384, core560→568, owner848→856이다. 앞의 값은 2026-10-24 [runtime/core 추출](dev3-runtime-core-20261024.md)이 잰 값이다. 진단 사본에서 `WalRuntime`과 같은 필드에서 `first_write_lsn: AtomicU64`만 뺀 구조체를 재면 376이고 실제는384다. 증가분 전부가 이 marker다.

이 marker는 [D3-COMMIT-CONTINUATION-001](../contracts/dev3-commit-continuation.md) §4(2026-10-25 D2 한정 채택)가 원 WAL에 둔 첫 File write 사실이다. [원 소유 표](../contracts/dev3-module-boundaries.md#actual-ownership)도 `WalRuntime`의 필드로 first-write marker를 적는다. `wal_io.rs` hash는 runtime/core 기록(`4b04864b…`)과 [commit continuation 기록](dev3-commit-continuation-20261025.md)(`bb86dd49…`)에서 다르다. 그 단위의 실행 기록에는 runtime_core 정적 시험이 없고, 고정값848이 그대로 남았다. 의도된 증가이며 owner≤32768·Empty 전환 한도 등 나머지 단언은 그대로 통과한다.

### 4.3 뒤쪽 단언에 남은 G5 값

위 두 줄만 고치면 두 시험 모두 뒤쪽 단언에서 다시 실패한다. 수정 전 출력에 이미 나온 값이다.

| 단언 | 고정값 | 실제 | 원인 |
| --- | ---: | ---: | --- |
| G5 owner memory / native | 848 / 984 | 856 / 864 | 4.2의 +8B, native는 §1 adapter `File` −128B |
| G5 native `File` | 160 | 32 | §1 adapter 수정(`94ede44`) |
| G5 `SharedPool` / `Plan4Pages` | 17216 / 17240 | 13848 / 13872 | 아래 −3368B. Plan4Pages는 pool을 값으로 담는다 |
| G5 `LiveDirectory` | 5160 | 4648 | slot의 `OnceLock<TargetRequest>`72B가 단일 registry leaf의 `Option`64B로 바뀜, 64slot×8 |
| `old_common_delta` | 0 | 512 | 위 directory 차이 그 자체 |

pool −3368B는 진단 사본에서 잰 directory −512, DPT 행 배열5120→2048의 −3072([XADB-MODULE-DPT-001](module-independence-survey-20261026.md#c2-compact-dpt)), G5 이후 leaf에 더해진 LOAD claim 등의 +216이다. G5 owner인 `plan4_pages/resources.rs`의 `fits`와 G5 시험은 이미 `old_common_growth == 0`(증가만 금지)을 쓰고, 그 주석이 Option slot의 축소를 이유로 든다. 두 소비 시험만 옛 `delta == 0`을 남겼다.

### 4.4 수정과 실행

시험 기대값만 바꿨다. 제품 코드·cap·계약은 바꾸지 않았다. 정확 일치 단언은 유지하고 `old_common_growth`로 완화하지 않았다. carrier가 다시 바뀌면 이 시험이 다시 알린다. 각 자리에 옛 값과 이유를 주석으로 남겼다.

- `plan4_retention/tests.rs`: metadata 항등식에 8192+5760+49152를 더한다. G5 표를 13872/13848/4648, memory owner856, native File32·owner864로 바꾼다. `old_common_delta`는512다.
- `mutable_storage/runtime_core_tests.rs`: owner memory848→856, native856→864, `old_common_delta` 0→512.

`cargo test -p xadb-engine --lib --no-run --locked --offline`로 build하고 네 selector를 각각1회 `--exact --test-threads=1`, stack32MiB, 120초 제한으로 실행했다. 변경한 코드는 이 두 시험 함수뿐이므로 다른 시험은 실행하지 않았다.

| selector | 수정 전 | 수정 후 |
| --- | --- | --- |
| `retention_source_memory` | FAIL `tests.rs:140`, 187008 ≠ 123904 | PASS, peak64000·metadata187008·whole252544 |
| `retention_source_native` | FAIL `tests.rs:140`, 187016 ≠ 123912 | PASS, peak64008·metadata187016·whole252552 |
| `runtime_core_source_memory` | FAIL `runtime_core_tests.rs:93`, 856 ≠ 848 | PASS, runtime384·core568·owner856 |
| `runtime_core_source_native` | FAIL `runtime_core_tests.rs:93`, 864 ≠ 856 | PASS, runtime392·core576·owner864 |

| identity | SHA-256 |
| --- | --- |
| 수정 전 test ELF(`06a260e`) | `b294c7feae5ddb35bd0859155a17d710fbbf263420b85e2ee6fa3d0cce0e3f5f` |
| 수정 후 test ELF | `09f750bfbac2587113b1a66d28a44439647c063b38b3bc6d6a733bbdafbd3feb` |
| 진단 ELF(커밋하지 않은 layout 출력 patch) | `e67657ac9f48af198db18f2d6720e5b2b5852c95e1b6a5c84bdcba5c0b297068` |
| 수정 후 `plan4_retention/tests.rs` | `14b591b2df174484172b2298552ccb305fc4a45acf07480d754d5a2af55a76eb` |
| 수정 후 `mutable_storage/runtime_core_tests.rs` | `9fe2d8a1665deda57d7cbe9a410692e4eff8080bf81cc8c61f3b1aea24e7e4c8` |

그보다 앞서 `plan4_retention/tests.rs`의 주석 한 단락만 다른 중간 source(test ELF `52be9605b4c35beeff9a91335ec288ba7c2de15fa267f0029d3b86fbb94b64a9`)에서도 네 selector가 같은 값으로 PASS했다. 최종 source에서 `cargo fmt --all --check`와 `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`가 통과했다.

실행 로그와 진단 patch는 작업 세션의 임시 공간에만 있고 공유 폴더에는 두지 않았다. 위 값은 그 출력에서 옮긴 것이다.

<a id="debug-hash-timeout"></a>
## 5. MCC02 30초 case 한도 초과: debug build의 SHA-256과 byte 단위 zero 검사

2026-10-31 · branch `claude/project-thread-fkdet1`(기준 S6 branch `claude/project-thread-a7onu7`의 `188c957`). `plan4_recovery::tests::catalog_memory::commit_call::commit_preparation_unwind_recovers_original_prefix`(MCC02)는 기능 단언을 모두 통과하고 `case`의 `started.elapsed() < 30s`에서만 실패했다. 클라우드 4-core debug ELF에서 S6 tip 43.6초/41.8초, main `8d5e48b` 41.6초가 보고됐고, 이 작업에서 S6 기준 ELF로 다시 재면 44.88초였다.

### 5.1 원인

main `8d5e48b` test ELF(`7f456dcc…`)를 `perf record --call-graph=dwarf`(49Hz, 2315 sample)로 잰 inclusive 비율이다.

| 위치 | 비율 | 내용 |
| --- | ---: | --- |
| `sha2` SHA-256 전체 | 65.6% | opt-level 0이라 x86 SHA 내장 함수 하나하나가 함수 호출로 남는다 |
| └ `row::LayoutView::digest` | 29.2% | `HistorySource::validate`·`seed_replay_slice`가 step마다 Row base 2MiB 전체를 다시 hash |
| └ `page::decode_page` | 11.0% | page checksum |
| └ `row::decode_layout` | 6.5% | Row base layout 검산 |
| └ 시험 oracle `hex` | 0.65% | fixture pin 검산. 시험 쪽 비용은 무시할 만하다 |
| `retention_page::zero` | 25.2% | `verify_selected_census`가 64 job을 `decode_job`하며 Free job의 1008B를 `iter().all`로 한 byte씩 검사 |

두 항목이 시간의 약 90%다. 둘 다 release build에서는 문제가 되지 않는 크기의 작업이고, 같은 검증을 opt-level 0으로 돌릴 때만 30–100배 느려진다. 제품의 복구 의미나 반복 구조가 틀린 것이 아니고, 30초 한도가 비현실적인 것도 아니다. 원인은 시험 build에서 hash와 zero 검사가 지나치게 느린 데 있다.

### 5.2 수정

- `Cargo.toml`: `[profile.dev.package.sha2] opt-level = 3`. 외부 dependency `sha2`만 최적화한다. workspace crate·release profile·`Cargo.lock`은 그대로다. 결정과 재검토 조건은 [저장소 정책 §7](../policies/repository-build-plan.md#section-9)에 적었다.
- `retention_page::zero`: `iter().all` 대신 1024B 0 block과의 slice 비교(`memcmp`)로 바꿨다. 의미는 같다. 새 `zero_checks_every_byte_of_blocks_and_tail`이 빈 입력과 길이 1·1023·1024·2055의 모든 byte 위치를 하나씩 바꿔 검사한다.

30초 한도는 바꾸지 않았다. 한도를 올리는 것은 [개발 정책](../policies/development-policy.md)의 사후 완화 금지에 걸리고, Row base digest cache는 base 불변 기간이라는 새 불변식이 필요한 복잡화라 택하지 않았다.

### 5.3 실행

`cargo test -p xadb-engine --lib --no-run --locked`로 build하고 `--exact --test-threads=1`, stack 32MiB로 실행했다.

| build | test ELF SHA-256 | MCC02 |
| --- | --- | --- |
| S6 `188c957` 기준 | `05bf2d6d1e57791cd60908574d23d6f6bd894f006c01200cc177110ceac8ee30` | FAIL `catalog_memory.rs:497`, 44.88초 |
| + `sha2` opt-level 3 | `ef8f1f211b9edfa37122e0ddf35a1a5ed2376dd1499bea1a7329ce09ae2429b4` | PASS, 15.08초 |
| + `zero` memcmp | `c83b6a1d82f92173823e647779eecac4654fbc06c4210864074367078bbe0438` | PASS, 4.76초 |
| 최종(+ `zero` 시험) | `3530aac607b7bff4c7751ab677c01e8df4473e2163f52bdc42254d69ccc7fff1` | PASS ×3, 4.09/3.93/4.06초 |

변경한 제품 함수는 `retention_page::zero` 하나이므로 최종 ELF에서 그 module의 `storage::retention_page::` 31개(ignored 3 포함, 새 시험 포함)를 실행했고 모두 PASS했다. 최종 source fingerprint는 `39928f98b3601b05510042b2763808b76cde6bff8983fe2ed571ed5120c382b3`, `XADB_G6_VECTOR_INDEXES`는 `p1-o00=ce3fd367007a6d798dbb6c2f18367a714d4f105611fb190fe57a1aa72d6d99a8`이다. MCC02의 fixture pin과 G6 vector index 검산이 고정 SHA-256 값과 맞으므로 최적화한 `sha2`의 결과도 확인된다. `cargo clippy -p xadb-engine --all-targets --all-features --locked -- -D warnings`가 통과했다.

같은 원인으로 느린 다른 시험(S6의 240초 한도 복구 사례, 2026-10-29 main 검사에서 40–50초가 걸린 catalog_memory unwind/own-read 시험 등)도 빨라질 것으로 보이지만 이 작업에서는 실행하지 않았다. 로그·profile 요약·실행 script는 공유 폴더 `main-preexisting-debug-20261030/debug-hash-timeout/`에 있고, ELF와 perf 원자료는 지웠다.

| 파일 | SHA-256 |
| --- | --- |
| `crates/engine/src/storage/retention_page.rs` | `ee0df629d363239e0b6835e48c4ce1d6506331fd81d7d96f54be566a3a3bda01` |
| `crates/engine/src/storage/retention_page_tests.rs` | `6944c3dec33ba347654e54d6963b0d12dfba6d3c3d506c47b15e4a3ede6d3355` |
| `Cargo.toml` | `b87970a24dee0d397f49c3ec95f02b71814e415c5dda4be8ae057e83f31d17e1` |

<a id="s6-shared-growth"></a>
## 6. Catalog commit Pending 옆 Full 조회 거절: S6의 shared +144B

2026-10-31 · branch `claude/project-thread-dpdk6i`(기준 main `76b3627`, S6 병합). 같은 날 main 잠재 버그 찾기(공유 폴더 `main-bug-hunt-20261031/README.md`)가 찾은 회귀다. 그 작업의 재현 시험 commit(`ca92ba8`)을 같은 내용으로 가져와(`2f74d7b`) 고쳤다. main에서도 실패하므로 사용자 규칙에 따라 PR은 main이 아니라 모듈 작업 branch `claude/project-thread-1jfdtk`로 연다.

### 6.1 원인

계약은 Catalog commit의 Pending·publication 구간이 admitted Full 조회 1개와 겹칠 수 있다고 정한다. 그 합 `shared + pending + full`이 65536B short source 안에 들어가야 한다. S6 `32de380`부터 이 합이 넘쳐 Pending writer 옆 Full 조회가 `RetentionError{Census, Capacity, Unchanged}`로 거절됐다.

| 시점 | shared (memory / Linux 타입) | shared+pending+full | 결과 |
| --- | ---: | ---: | --- |
| S5 병합 `8d5e48b` | 20184 / 20192 | 65444 / 65452 | PASS |
| S6 병합 `76b3627` | 20328 / 20336 | 65588 / 65596 | 거절 |
| 이 수정 | 20200 / 20208 | 65460 / 65468 | PASS |

`shared_bytes`는 live owner+session과 recovery runtime+recovered facade 중 큰 쪽에 core와 split port를 더한 값이다. S6가 RestoredRetention에 `row_admission: Option<RowRecoveryAdmission>`(policy 1B와 digest 32B 두 개, 정렬 뒤 72B)를 넣었다. RestoredRetention은 runtime의 `loans`(RecoveryBindLoans)와 `Plan4Recovered` 안 DiskSession의 `recovery_binding`(RecoveryRetentionBinding)에 한 번씩 들어 있다. 실행 중에는 대여분에서 binding으로 옮겨 가 하나뿐이지만 정적 source 계수에는 둘 다 잡힌다. 72B×2가 144B다. 독립 S6 검토는 두 자리를 RecoveryState의 `row_admission`과 RestoredRetention으로 보았으나, RecoveryState(`Plan4Recovery`)는 `shared` 식에 없다. 실제 두 자리는 위의 runtime 대여분과 binding이다.

두 digest는 RestoredRetention을 만들 때(`physical.rs`) 원 entry policy·AnalysisSeal digest·실제 분석 SourceFacts digest와 대조된다. 그 뒤 binding의 `row_policy()`는 admission의 state_digest가 RestoredRetention의 state_digest와 같은지, source_facts_digest가 0이 아닌지만 봤다. 둘은 같은 private 값 안의 불변 복사본이라 이 검산은 실패할 수 없었다. 계속 보유할 이유가 없는 64B가 두 번 계수된 셈이다.

### 6.2 수정

- `plan4_recovery.rs`: RestoredRetention의 `row_admission`을 `row_policy: Option<RowRecoveryPolicy>`로 바꿨다. RecoveryState가 analysis와 RestoredRetention 사이에 보유하는 admission 전체와 `physical.rs`의 세 대조는 그대로다. 대조를 통과한 admission의 policy만 RestoredRetention에 넣는다.
- `RecoveryRetentionBinding::row_policy()`: `Result<Option<_>, ()>`에서 `Option<_>`으로 바꿨다. 실패할 수 없던 Err 갈래와 `plan4_retention/recovery/catalog.rs`의 대응 갈래를 없앴다. legacy binding(None)의 Row cell은 그대로 Row decode 전 Unsupported다.
- 계약: [S6 정정](../contracts/dev3-module-boundaries.md#row-recovery-s6-shared)에 이 변경과 "매 단계 digest 검산" 문구의 삭제를 적었다. cap·charge·wire는 바꾸지 않았다.
- 시험: 재현 시험의 주석을 회귀 시험 설명으로 바꿨다. `s2w_c04_live_and_recovered_peaks_unchanged`의 고정값을 memory 64000/65368→64008/65384, Linux 타입 64008/65376→64016/65392로 바꿨다. S6의 policy 1B(정렬 8B)가 binding(live·recovered)과 runtime 대여분(recovered)에 더해진 값이다. 이 시험은 S6 뒤 64072/65512로 이미 실패하고 있었다.

state_digest만 남기는 안(한 자리 40B, 합 +80B)은 Linux 타입 Pending+Full이 65532B로 여유 4B라 택하지 않았다. RestoredRetention의 읽히지 않는 `physical_topup`·`metadata_growth`를 빼서 증가를 0으로 만드는 안은 이 회귀와 무관한 변경이라 하지 않았다.

### 6.3 실행

`cargo test -p xadb-engine --lib --no-run --locked`로 build하고 각 selector를 `--exact --test-threads=1`, stack 32MiB로 1회 실행했다. identity 환경 변수(`XADB_*_SOURCE/ELF/INPUT_SHA256`)는 ELF hash로, `XADB_G6_VECTOR_INDEXES`는 §5.3의 값으로 주었다. 변경한 것은 RestoredRetention의 크기와 Row policy 조회이므로 그 크기를 세는 census 시험, 같은 원인으로 실패하던 시험, `row_policy()`를 거치는 S6 Row 복구 시험만 골랐다. 전체 workspace나 다른 시험은 실행하지 않았다. S6 native controller(`scripts/check_dev3_s6_native.py`)도 실행하지 않았다. 바뀐 코드는 platform과 무관하고 Linux 타입의 크기는 census 시험이 덮는다.

| 선택 | 수정 전 | 수정 후 |
| --- | --- | --- |
| 재현 시험 `repro_full_read_beside_pending_commit_fits_actual_charges` | FAIL `source_tests.rs:775`, 65588/65596 | PASS, 65460/65468 |
| `catalog_commit_windows_fit_beside_admitted_readers` | FAIL `source_tests.rs:708`, pending_full 65588 | PASS, pending_full 65460/65468 |
| 같은 원인의 기존 실패 10개(MCW01, MT03, `catalog_read_operation_*` 2, `call_scope::*` 2, `source_entry::drain_waits_for_entry_before_source_charge`, `source_return::*` 2, S3-R07) | FAIL(main 버그 찾기 기록) | PASS 10/10, 각 0.4–1.2초 |
| `s2w_c04_live_and_recovered_peaks_unchanged` | FAIL `census_tests.rs:231`, 64072/65512 | PASS, 64008/65384·64016/65392 |
| 자원·census 시험 93개(버그 찾기의 `census-93.tsv` 목록) | main에서 8개 FAIL | 위 셋을 뺀 FAIL 5개가 main과 같은 자리·같은 원인 |
| S6 Row 복구 memory 시험 `row_recovery::s6r_m01..m08` 14개 | – | PASS 14/14, 각 1.2–20.0초 |

census 93개에서 남은 FAIL 5개는 main `76b3627`에서도 같은 panic 위치로 실패하며 이 변경과 무관하다. `xrb_*` 3개는 `XADB_XRB_INCLUDE` 선택 입력이 필요한 recovery base codec 시험이고, `g5_canonical_source_boundaries`(`plan4_pages/tests.rs:586`, case 3 `Retention(IdentityMismatch)`)와 `status_flush_rf`(`resource_tests.rs:277`)는 이전 main 검사부터 알려진 실패다. 첫 실행에서 identity 환경 변수가 빠져 실패한 `control_source_census`·`retention_census_inputs`·`retention_source_census`는 변수를 주고 다시 실행해 PASS했다.

| identity | SHA-256 |
| --- | --- |
| 수정 전 test ELF(main `76b3627` + 재현 시험) | `ff9c5f428f70a52f310cbee773a9383aca9499c85306f35eeaa59b906132dbe8` |
| 수정 후 test ELF(`s2w_c04` 고정값 갱신 전; 재현·기존 실패 11개·census 93개) | `1267a81dd50b8d4459975b18203d615df75956bbf686f6dd44a4a788290305e3` |
| 최종 test ELF(`s2w_c04`·S6 Row 복구) | `543ebdb5e8938621c1fcd71abf4f7275627d1272e57ca5b7f6484eef58e86129` |

두 수정 후 ELF의 차이는 `census_tests.rs`의 `s2w_c04` 고정값과 주석뿐이다. 제품 source는 같다.

최종 source에서 `cargo fmt --all --check`와 `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`가 통과했다. 실행 로그는 작업 세션의 임시 공간에만 있고 위 값은 그 출력에서 옮겼다. ELF는 지웠다.

| 파일 | SHA-256 |
| --- | --- |
| `crates/engine/src/storage/wal_io/plan4_recovery.rs` | `145d88332e824675c852a509bccecd11568c7a1565972df7d346f7f5d0659f4f` |
| `crates/engine/src/storage/wal_io/plan4_recovery/physical.rs` | `57d35d8aec5700c3dc69c8d539483941bde83d912af146f59888ca13692e4629` |
| `crates/engine/src/storage/wal_io/plan4_retention/recovery/catalog.rs` | `b981287b95c9a5ced00977495623b048e3aaaaad3cc8b1e510f4c6c9c0f5dee0` |
| `crates/engine/src/storage/wal_io/plan4_apply/source_tests.rs` | `eff62e9925187a1e4d27c9c18f208bbe5959740bb5ea99fc5acf3d7cf93c8d38` |
| `crates/engine/src/storage/wal_io/plan4_apply/row/census_tests.rs` | `edff998b1d869ff583147e118e6d8103c400db75944ba6cb1e34f1dd73f32dac` |

<a id="f0-existing-evidence"></a>
## 7. F0 기존 증거 분류와 후속 인계

2026-11-01 UTC · **기존 증거·정적 경로 분류 / 원 로그 미확보 항목은 미확인 / 수정·제품 재시험·차단 해제 없음**

[후속 계획 F0](../plans/module-independence-followup-plan.md#failures)의 기존 증거 분류 요청에 따른 후속 기록이다. 앞 절의 원 결과·사용자 결정·identity는 그대로 보존한다. 조사·문서 작성·자기 검토·인계 담당은 `/root`다. 제품·시험·oracle·정책을 바꾸지 않는 D0 기록이며 자기 검토를 독립 D1/D2 수용으로 기록하지 않는다. 원인별 수정의 구현자·독립 검토자는 해당 후속 작업 착수 때 배정한다.

### 입력과 확인 한계

- 조사 기준은 PR #23 병합 `8215c729`다. [S12 보정](module-independence-survey-20261026.md#s12-full-lib-calibration)의 실행 source `76ec2cf5`와 `crates/`, `scripts/`, `tests/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`을 Git diff로 대조했으며 차이가 없다. 여기서 읽은 소스는 보정 때와 같지만 새 실행 결과는 아니다.
- 확보한 자료는 위 보정의 실패 표·집계, 이 문서 §2·§4·§6의 원 조사, 해당 source·fixture·계약이다. 최초 실패는 아래처럼 **확보한 기록에서 확인 가능한 최초 시점**을 뜻한다. 결함 도입 commit을 뜻하지 않는다.
- 원 `calibration-full/head-cases.log`, 모듈 선택 log, `isolated1.log`, `isolated2.log`, `failing-files.json`, `resident-selector-coverage.json`은 현재 checkout·`/workspace/shared`·`/workspace/scratch`에 없고, 원 보관 경로 `/mnt/project-files/module-independence-20261029/artifacts/module-s12-20261101/`도 이 환경에 없다. 원 manifest의 문서상 SHA-256은 `b25b759b2dac1739ddb2459106b72ecea995050d3a87676794a3c6f05d6fd00c`이나 이번에 원 파일을 해시 검증한 것은 아니다.
- 따라서 stdout/stderr의 실제 좌우 값, 첫 panic 앞의 사례 ID, backtrace·후속 panic 순서를 다시 검산하지 못했다. 원 보정의 10개 isolated 재현도 기존 기록 인용이며 이번 재실행이 아니다. 자료 부족을 제품 정상·환경 결함으로 판정하지 않는다.

### 식별과 상태 읽는 법

아래 `F0-E01`–`E26`은 26개 실패를 빠짐없이 인계하기 위한 **이번 추적 ID**다. 원 인수 ID를 대체하지 않는다. 정확한 이름이 없는 7개(E01–03, E05–08)는 파일·실패 위치별 미확인 자리이며 원 로그의 순서나 서로 다른 libtest 이름을 추정하지 않았다. 이 7개는 사례별 selector 결속이 아직 미완료다. `UNKNOWN` 자리를 실행 가능한 selector로 사용하지 않는다. 이름을 확인한 행도 첫 subcase·실제 오류 값은 원 로그가 있어야 확정한다.

`M = storage::wal_io::bootstrap::doublewrite_install::mutable_storage::`로 두고 아래 접두어와 이름을 이어 붙이면 libtest selector가 된다. 물리 경로는 `crates/engine/src/storage/wal_io/` 기준이다. 이 대응은 `#[path]`·`mod` 선언을 읽은 정적 대응이며 `--list`를 새 실행하지 않았다.

| 표기 | `M` 뒤에 붙일 시험 모듈 | 물리 파일 |
| --- | --- | --- |
| RS | `status_init::status_flush::resident::tests::` | `resident_status/tests.rs` |
| PG | `plan4_pages::tests::` | `plan4_pages/tests.rs` |
| LL | `plan4_pages::retention::tests::lifecycle::` | `plan4_retention/lifecycle_tests.rs` |
| FU | `plan4_pages::retention::free_undo_tests::` | `plan4_retention/free_undo_tests.rs` |
| RC | `plan4_recovery::tests::cases::` | `plan4_recovery/tests/cases.rs` |
| RL/RF/RP/RU | `plan4_recovery::tests::{closure,faults,physical,publication}::` 각각 | `plan4_recovery/tests/{closure,faults,physical,publication}.rs` 각각 |
| FR | `plan4_recovery::tests::free_undo::` | `plan4_recovery/tests/free_undo.rs` |
| CS | `plan4_pages::retention::row::census_tests::` | `plan4_apply/row/census_tests.rs` |
| RE | `recovery_prefix::effect::tests::` | `recovery_effect/tests.rs` |
| SF | `status_init::status_flush::tests::` | `status_home_flush/tests.rs`(내부 `resource_tests.rs` 호출) |

`A`는 보정이 인용한 2026-10-29·30 main 점검의 기존 실패(개별 최초 commit·log 미확인), `B`는 2026-11-01 `76ec2cf5` 보정에서 새로 드러난 실패다. **모든 E행의 보정 결과는 FAIL이고 현재 처리 상태는 보류·차단 유지**다. 분류의 확실성은 따로 적었다. 아래 G행은 각 사례가 참조하는 현재 계약·근거·다음 조치·완료 조건이다. 각 E행의 담당은 `/root`(분류·인계)이며 수정 담당 미배정을 이미 수행된 수정으로 세지 않는다.

| 추적 ID | selector 접두어 + 사례 이름 | 최초 기록 | 첫 실패 위치(보정 기록) | 분류·근거/다음 조치 | 영향 작업 |
| --- | --- | --- | --- | --- | --- |
| F0-E01 | RS + UNKNOWN-1 | B | `dirty_frame/status_residency.rs:328` | G1: 관측 helper 실패, 사례명·선행 fence 원인 미확인 | resident 관련 C 변경 |
| F0-E02 | RS + UNKNOWN-2 | B | 같은 위치 | G1: E01과 동일 원인이라는 판정 아님 | resident 관련 C 변경 |
| F0-E03 | RS + UNKNOWN-3 | B | 같은 위치 | G1: E01과 동일 원인이라는 판정 아님 | resident 관련 C 변경 |
| F0-E04 | PG + `g5_canonical_source_boundaries` | A | `plan4_pages/tests.rs:586` | G2: 오류 분류 기대 불일치, 제품/시험 귀속 미확인 | F5 |
| F0-E05 | PG + UNKNOWN-1 | A | `plan4_genesis/tests/support.rs:278` | G3: 주입 panic 관측, 사례명·포착 실패 경로 미확인 | F5 |
| F0-E06 | PG + UNKNOWN-2 | A | `dirty_frame/status_residency.rs:328` | G1: 관측 helper 실패, 사례명·선행 fence 원인 미확인 | F5 |
| F0-E07 | PG + UNKNOWN-3 | A | 같은 위치 | G1: E06과 동일 원인이라는 판정 아님 | F5 |
| F0-E08 | PG + UNKNOWN-4 | A | 같은 위치 | G1: E06과 동일 원인이라는 판정 아님 | F5 |
| F0-E09 | LL + `retention_live_lifecycle` | A | `lifecycle_tests.rs:302` | G4: 알려진 Reserve 차이. 이후 시나리오 원인은 미검증 | F6 |
| F0-E10 | LL + `retention_live_reader_boundaries` | A | `dirty_frame/status_residency.rs:328` | G1: 6개 시험 중 아래 네 위치를 제외한 두 사례의 정적 대응 | F6 |
| F0-E11 | LL + `retention_live_latest_load` | A | 같은 위치 | G1: E10과 동일 원인이라는 판정 아님 | F6 |
| F0-E12 | LL + `retention_live_writer_boundaries` | A | `lifecycle_tests.rs:808` | G5: 현재 format에 맞지 않는 legacy 호출을 정적으로 확인 | F6 |
| F0-E13 | LL + `retention_live_activation` | A | `lifecycle_tests.rs:962` | G6: Job 이미지 비교 불일치, byte 원인 미확인 | F6 |
| F0-E14 | LL + `retention_live_flush_boundaries` | A | `lifecycle_tests.rs:1012` | G5: 현재 format에 맞지 않는 legacy 호출을 정적으로 확인 | F6 |
| F0-E15 | FU + `free_undo_original_owner_gap_cancel_reuse_flush_reload` | A | `free_undo_tests.rs:192` | G6: allocation descriptor 비교 불일치 | F6 |
| F0-E16 | FU + `free_undo_new_role_preflight_and_install_failure_boundaries` | A | `free_undo_tests.rs:602` | G6: Cancel 전 Reserve WAL 비교 불일치 | F6 |
| F0-E17 | RC + `recovery_lifecycle` | A | `plan4_recovery/tests/support.rs:260` | G7: 기존 조사상 format1 이관 전 시험 | F5·D01 |
| F0-E18 | RC + `recovery_binding` | A | 같은 위치 | G7: 기존 조사상 format1 이관 전 시험 | F5·D01 |
| F0-E19 | RL + `recovery_closure` | A | 같은 위치 | G7: §2.2의 후속 `aging_start` 거절 진단도 이 사례에 한정 | F5·D01 |
| F0-E20 | RF + `recovery_selection` | A | 같은 위치 | G7: 기존 조사상 format1 이관 전 시험 | F5·D01 |
| F0-E21 | RP + `recovery_physical` | A | 같은 위치 | G7: 기존 조사상 format1 이관 전 시험 | F5·D01 |
| F0-E22 | RU + `recovery_publication` | A | 같은 위치 | G7: 기존 조사상 format1 이관 전 시험 | F5·D01 |
| F0-E23 | FR + `free_undo_reserved_recovery_uses_original_scope_cancel` | A | `plan4_recovery/tests/free_undo.rs:180` | G6: 복구 진입 전 producer WAL prefix 비교 불일치 | F5 |
| F0-E24 | CS + `s2w_c04_live_and_recovered_peaks_unchanged` | B | `census_tests.rs:236` | G8: live/recovered peak 고정값 불일치 | F4 |
| F0-E25 | RE + `pending_and_healthy_corruption_boundaries` | B | `recovery_effect/tests.rs:621` | G9: 손상 거절 cause 기대 불일치 | recovery effect 관련 C 변경 |
| F0-E26 | SF + `status_flush_rf` | A | `status_home_flush/resource_tests.rs:277` | G10: RF01 구성원 크기 상한 중 적어도 하나 초과 | status flush 관련 C 변경 |

14개 파일은 RS 1 + PG 1 + LL 1 + FU 1 + RC 1 + RL/RF/RP/RU 4 + FR 1 + CS 1 + RE 1 + SF 1 + 아래 mixed 1이다. E행은 3+5+6+2+6+1+1+1+1=26이며, 같은 helper 위치는 3+3+2=8개다. 이 수는 원 보정과 일치하지만 미확인 이름을 확인한 것으로 세지 않는다.

### 원인군별 정적 근거와 처리 조건

| 분류 | 현재 계약·직접 확인한 근거 | 처리 결정·다음 조치 | 완료 조건 |
| --- | --- | --- | --- |
| G1 관측 helper / 원인 귀속 미확인 | [원 소유·fence](../contracts/dev3-module-boundaries.md), [INIT 실패 계약](../contracts/dev3-status-init.md). `status_residency.rs`의 test-only `slot()`은 `resolve_registration().unwrap()`을 호출한다. `dirty_frame.rs`의 `resolve_registration → entry_registry → health`는 fenced 세션을 거절한다. 즉 그 위치의 panic은 등록 관측 실패이며 원 데이터 손상 증거와 같지 않다. | 8개를 제품 결함 또는 helper 결함 하나로 합치지 않는다. 원 로그로 이름·첫 cause·fence 발생점을 결속한 뒤 각 caller가 의도적으로 fence한 후 snapshot을 요청했는지 확인한다. 시험 관측을 고치더라도 production health 검사를 우회하지 않는다. | 사례별 최초 원인 설명, 필요한 좁은 수정, 원 오류/정리/비대상 frame 보존을 검증한 선택 결과. snapshot만 성공한다고 제품 정상으로 판정하지 않음 |
| G2 canonical 오류 의미 / 미확인 | [Plan4 clean owner](../contracts/dev3-plan4-clean-owner.md). `g5_canonical_source_boundaries`는 3/4/7에 `Retention(Corrupt)`를 기대한다. §6의 기존 기록은 case 3의 `Retention(IdentityMismatch)`를 보고했다. 둘 다 거절하므로 거절 여부와 cause 계약을 분리해야 한다. | 원 로그·case 3 입력과 decode 순서를 대조하고 현재 계약의 오류 의미를 확인. 기대값만 새 값으로 덮지 않는다. | 오류 원인·effect·fence·쓰기 없음·기존 frame 보존을 함께 확인한 관련 사례 |
| G3 I/O 주입 / 미확인 | [Plan4 clean owner](../contracts/dev3-plan4-clean-owner.md). `plan4_genesis/tests/support.rs:278`은 `Action::BeforePanic`의 의도적 `panic!("plan4 exact before panic")`다. panic 위치만으로 product panic이라 할 수 없고, 예상 panic 출력만으로 최종 FAIL 원인도 정할 수 없다. | 원 사례명과 주입 event·catch/join·Drop 순서를 확보해 의도된 unwind가 왜 최종 실패가 되었는지 분리. | 해당 주입과 cleanup/무효과 경로의 원인 확인 및 선택 검증 |
| G4 알려진 Reserve 차이 / 이후 미확인 | [Free Undo](../contracts/dev3-plan4-free-undo.md), [Retention live](../contracts/dev3-retention-live.md). §2.1–2.2는 v2 scope·Profile digest·예약량 차이를 기록했다. 현재 `complete()`는 여전히 reserve literal과 비교 후 INIT 길이 38000, 옛 empty 거래 순서를 기대한다. | E09는 첫 차이만 기존 판정으로 인용. §2.2의 `recovery_closure` 진단을 이 사례의 전체 실행 결과로 옮기지 않는다. current-format 입력·producer별 대응안을 준비하고 필요한 처리 결정을 적용한다. | 새 독립 fixture/oracle와 전체 해당 시나리오 대응, 실제 도달·완료. 첫 Reserve 비교 통과만으로 닫지 않음 |
| G5 legacy producer 호출 / 정적 계약 불일치 | [Retention live](../contracts/dev3-retention-live.md)와 현재 `plan4_retention/live.rs:142–151`. current Profile3은 Row XOI cache가 있으므로 `format()!=1`에서 `WrongState/Prepare/Unchanged`로 거절한다. E12·E14는 그 `aging_start()`에 성공 `unwrap()`을 요구한다. | 시험 경로 이관 필요성을 정적으로 분류. 보정 실제 Err 값·뒤 subcase는 원 로그 미확인이다. G7의 6개 보존 결정 범위를 이 두 개에 자동 확대하지 않으며, 거절을 풀어 legacy producer를 current에 허용하지 않는다. | 현재 형식의 동등한 writer/flush 경계와 독립 기대값을 공급하고 선택 검증. 단순 Err 수용으로 본래 경계 coverage를 없애지 않음 |
| G6 literal 비교 / byte 원인 미확인 | [Plan4 genesis](../contracts/dev3-plan4-genesis.md), [Free Undo](../contracts/dev3-plan4-free-undo.md), [복구 owner](../contracts/dev3-plan4-recovery-owner.md). 각각 genesis Job, memory descriptor, native Reserve frame, producer WAL prefix 비교다. E23은 복구 시작 전에 실패하므로 복구 알고리즘 실패로 단정하지 않는다. | 원 actual/expected bytes·offset·fixture identity를 먼저 확보. 공통 Profile 변경 가설은 있으나 같은 원인으로 판정하지 않는다. E16의 뒤 fault cut, E23의 recovery cancel은 도달 증거 없음으로 남긴다. | 사례별 byte 차이의 계약 설명, 독립 oracle·필요 수정과 뒤 경로의 선택 검증 |
| G7 format1 이관 전 / 기존 판정 인용 | §2의 원 조사와 [복구 owner](../contracts/dev3-plan4-recovery-owner.md). 기존 기록상 제품 결함이 아닌 미이관 시험 6개다. 원 로그를 이번에 재검증한 것은 아니다. | 2026-10-30 결정은 F05를 CR1에서 빼고 옛 시험은 고치거나 끄지 않는 것. 최신 [D01](../plans/module-independence-followup-plan.md#user-decisions)은 그 파일을 바꾸는 CI 차단 때문에 처리 재결정을 요구한다. 원 보존 유지, 이관/제외는 미결정. | D01에 따른 current-format 이관 또는 동등 coverage를 갖춘 유효한 처리를 실제 적용·검증. 보존/보류 선택만으로 차단 해제하지 않음 |
| G8 source census / 귀속 미확인 | [자원 계약](../contracts/dev3-resource-bounds.md), §6의 S6 수정 결과. 현재 기대값은 memory 64008/65384, native 64016/65392이며 §6의 당시 PASS와 같다. 보정의 현재 actual·실패한 writer 값은 미확보다. | 원 로그의 peak/shared/split_ports와 변경 이력을 대조해 의도된 증가·낡은 기대값·상한 회귀를 구분. 과거 S6 수정을 다시 해결책으로 제시하거나 기대값 증가로 상한을 완화하지 않는다. | 공존 phase·항목별 증가와 계약 상한 확인, 필요한 수정·정확한 census 선택 검증 |
| G9 손상 오류 cause / 귀속 미확인 | [Recovery effects](../contracts/dev3-recovery-effects.md). `tests.rs:621`은 `n==8`인 유효 codec/Free generation2 입력의 `RecoveryCause::Semantic(_)` 기대다. 앞의 Redo phase 검사는 먼저 수행된다. 실제 다른 cause는 로그 미확보다. | 정적 위치로 입력 종류를 좁혔으나 오류 검출 순서 변경인지 제품 회귀인지는 보류. actual cause와 home-write 효과를 원 로그·코드로 대조. | 손상 거절·첫 cause/effect·home write 0의 계약을 만족하는 관련 선택 검증 |
| G10 RF01 자원 상한 / 귀속 미확인 | [Status home flush 자원](../contracts/dev3-status-home-flush.md#resources). `run_rf()`는 `layout`, shared layout, I/O layout의 모든 actual/cap을 출력한 뒤 `within_caps`를 단언한다. 단일 위치만으로 초과 타입을 특정할 수 없다. | 원 `RESOURCE_STATUS_FLUSH` 행의 actual/cap을 확보해 타입·공존 자원·계약 상한을 분해. cap을 올리거나 §1의 과거 adapter 수정을 다시 적용하지 않는다. | 초과 항목별 원인·좁은 수정·해당 RF 선택 검증. 뒤 `fits`와 AdapterLayout도 도달 확인 |

### Macro 선택 두 개와 resident 미검증 8개

| 추적 ID·selector (`M` 뒤) | 원 결과·근거 | 분류·담당·다음 조치 | 완료 조건·차단 |
| --- | --- | --- | --- |
| F0-M01 `status_init::status_flush::mixed::tests::` | B, 87개 중 43 FAIL: Fenced 27·INIT release Timeout 14·requested INIT unwind 2. 개별 43개 이름/원 로그 미확보 | `/root` 인계. [혼합 owner 계약](../contracts/dev3-mixed-system-owner.md)의 failure/Drop 순서와 대조 필요. G1과 같은 snapshot helper 호출은 있으나 27개의 선행 fence 원인은 별개일 수 있다. `dirty_init.rs`의 pause는 5초 내 resume를 기다리며 Timeout과 요청된 unwind는 다른 분기다. controller의 앞선 assert·채널 단절·정리 순서를 확보하기 전 환경 지연 또는 제품 결함으로 단정하지 않음 | 43개 이름·첫 cause·현재 계약별 대응 후 원인별 좁은 수정/검증. `mixed_system_owner/tests.rs` 차단 유지 |
| F0-M02 `status_init::status_flush::resident::tests::` | B, 37개 inventory 중 여섯 번째 allocator cut에서 abort. 앞 5개 결과는 PASS 4/FAIL 1. 별도 exact 23개는 PASS 20/FAIL 3(E01–03에 포함). cut 8개 미검증, s5 6개 ignored | `/root` 인계. `init_cut()`은 controller assert 후 Panic 전송, fence 관측 후 Continue 전송을 요구한다. 기록의 abort 위치 `dirty_init.rs:1107`은 resume timeout expect다. 최초 controller 실패·소멸자 두 번째 panic의 전체 순서는 미확인. exact FAIL을 다시 더하거나 cut abort를 FAIL 26개에 포함하지 않음 | allocator의 최초 원인·정리 원인을 분리하고 아래 8개 도달/완료. `resident_status/tests.rs` 차단 유지; exact 23·ignored s5 6 자동 재실행/활성화 금지 |

다음 8행의 source·최초 기록은 모두 B(`76ec2cf5`, 2026-11-01)이고, selector는 **RS 접두어 + 아래 이름**이다. 현재 계약은 [INIT의 stage 이후 fence·guard 해제 순서](../contracts/dev3-status-init.md)와 원 `resident_status/tests.rs::init_cut`의 비대상 frame·pin 보존 단언이다. 처리 상태는 전부 미검증·차단 유지, 담당은 `/root`(분류·인계)다.

| 추적 ID | 정확한 사례 이름 | 주입 지점 | 원 실행 상태 | 다음 조치·완료 조건 |
| --- | --- | --- | --- | --- |
| F0-U01 | `resident_status_cut_allocator` | `AllocatorImageInstalled` | ABORT, 정상 test 결과 없음 | 원 로그의 최초 panic/Drop 연쇄 확보 → 좁은 수정 → 같은 cut에서 unwind·fence·해제·비대상 보존 완료 |
| F0-U02 | `resident_status_cut_dpt_checked` | `DptPrecheckedBeforePrepared` | NOT_STARTED | U01 처리 뒤 해당 cut 개별 선택으로 도달·정리 완료 |
| F0-U03 | `resident_status_cut_dpt_owned` | `DptGuardOwnedBeforePrecheck` | NOT_STARTED | 같은 조건으로 해당 cut 도달·정리 완료 |
| F0-U04 | `resident_status_cut_metadata` | `MetadataInstalled` | NOT_STARTED | 같은 조건으로 해당 cut 도달·정리 완료 |
| F0-U05 | `resident_status_cut_payload` | `PayloadImageInstalled` | NOT_STARTED | 같은 조건으로 해당 cut 도달·정리 완료 |
| F0-U06 | `resident_status_cut_publication` | `Published` | NOT_STARTED | 같은 조건으로 해당 cut 도달·정리 완료 |
| F0-U07 | `resident_status_cut_registry` | `RegistryInstalled` | NOT_STARTED | 같은 조건으로 해당 cut 도달·정리 완료 |
| F0-U08 | `resident_status_cut_stage` | `StageOwned` | NOT_STARTED | 같은 조건으로 해당 cut 도달·정리 완료 |

8개는 macro가 생성하는 별개 `#[test] fn`이므로 원 libtest의 `--exact` 선택 대상이다. 현재 gate의 module selector와 구분하며, 선택 도구가 넓게 실행하면 먼저 좁힌다. 아직 실행하지 않아 환경 입력 충족·성공 여부는 확인하지 못했다. U02–08을 U01과 같은 원인의 실패로 분류하지 않는다.

### 인계 판정과 검토 범위

이번에는 26개 실패 자리·두 모듈 선택·미검증 8개를 원 기록 및 현재 계약의 조사 경로에 대응했다. **원인 분류 전체 완료는 아니다.** 특히 7개 exact 이름, mixed 실패 43개 이름, 원 오류 값·panic 순서는 원 로그를 확보해야 결속할 수 있다. 후속 첫 조치는 위 원 보관 자료 확보이며, 불가능하면 F0 §4.4에 따라 입력·관측 질문·시간/disk·중단 조건과 기존 지시 적용 범위를 정한 유한 진단안을 준비한다. 원본 부족을 이유로 전체 lib 보정을 반복하지 않는다.

제품·시험·fixture·manifest·gate·CI 정책 diff는 없다. 관련 제품 시험은 NOT_RUN이며 무변경 시험을 이번 결과로 재사용하여 PASS라고 쓰지 않는다. D01–D03은 기존 상태를 유지한다. F1은 Python-only 변경으로 별도 진행 가능하다. F3–F6는 실제 diff의 exact case·macro selector와 위 차단 파일을 대조해야 하며, 이 문서의 영향 표만으로 실행/인수를 허용하지 않는다. M5 한정 완료와 MI03 잔여도 그대로다.

자기 검토는 원 보정의 26/14/2/8 집계, 소스의 logical module/첫 실패 위치, 계약 링크, 원 결정과 현재 처리의 구분, 제품 diff 부재를 대조하는 범위다. 문서 자동 검증·fmt·Clippy는 [push 훅](../policies/documentation-policy.md#push-gate)이 전송 commit 사본에서 수행하며 실제 결과는 PR에 기록한다. Clippy가 시험 target을 검사해도 제품 시험 실행이 아니다. 되돌림은 이 F0 문서 commit만 revert하면 되며 원 증거·fixture는 변경하지 않았다.

<a id="f0-completion"></a>
## 8. F0 수정·이관과 선택 검증 완료

2026-11-01 UTC · **F0 대상 수정·선택 검증 완료 / 고유 78 tests PASS / 원 FAIL·ABORT·NOT_STARTED 보존**

사용자의 후속 지시 “모듈 분류 후속 추가 계획에서 F0를 끝까지 진행 … 리뷰 테스트 하고 pr”에 따라 `main` `01fb227d7555c4ef90327382a027c1275737546d`에서 `codex/module-followup-f0-complete`를 만들었다. §7의 분류만 완료했던 상태를 이어서 처리했다. 작성·통합은 `/root`, 독립 D1/D2 검토는 `/root/f0_contract_review`다. 최종 검토의 실제 verdict와 head blob은 [gate 기록](../../module-gate/records/20261101-f0-complete.json), 원 실행과 source/ELF/input identity는 [완료 receipt](../../module-gate/evidence/f0-complete/completion.json)와 [artifact 해시](../../module-gate/evidence/f0-complete/artifact-sha256.json)에 둔다.

### 결정·진단 경계

D01은 위의 **F0 전체 완료 지시**를 current-format 이관 권고안의 실행 범위로 적용했다. 2026-10-30의 원 시험 보존 결정·그때의 미실행 기록은 수정하지 않았다. 옛 fixture와 과거 source는 보존하고 현재 선택 시험만 Profile3의 원 `catalog_*` producer와 독립 oracle로 이관했다. legacy producer의 format 거절, 생산 health/fence, cap 및 ignored 표시는 유지한다.

새 실행이 필요한 미분류 원인은 [진단 계획](../../module-gate/evidence/f0-complete/diagnostic-plan.md)에 입력·관측 질문·횟수·120초/사례·총45분·disk20GiB 중단 조건을 먼저 정했다. baseline exact23은 **21 FAIL·1 PASS·1 SIGABRT**였다. 이 결과를 현재 수정 결과로 덮지 않았다. `g5_clean_lifecycle`은 baseline에서 PASS여서 수정 검증에 반복하지 않았다. 과거 원 로그가 없는 7개의 이름·실패 순서를 이번 결과로 소급 확정하지 않는다.

추가로 Plan4 bind의 BeforePanic 위치를 구분하는 [유한 진단](../../module-gate/evidence/f0-complete/g5-bind-diagnostic-plan.json)을 했다. 첫 실행은 executor 전용 B08 padding 입력 누락으로 FAIL했으며 B01–07만 도달했다. 이어 미실행 B15만 선택해 의도된 panic을 catch하고 ABANDONED/fence·정리를 확인했다. B15 PASS를 B08 또는 bind 전체 PASS로 세지 않는다. 이 입력 실패는 제품 결함이나 F0의 새 차단으로 분류하지 않는다. 기존 executor 전용 구분은 유지한다.

### 원인과 현재 처리

§7의 E/M/U ID는 과거 증거의 추적 번호로 유지한다. 아래 selector 대응은 **새 source에서 실제 실행한 이름**이며, 미확보 과거 로그의 행 순서를 추정한 것이 아니다. 각 경로의 담당은 `/root`, 상태는 아래 범위의 수정·검증 완료다.

| 추적 범위 | 확인한 원인과 적용 | 현재 선택 결과·영향 |
| --- | --- | --- |
| E01–03, E06–08, E10–11 / G1 | 의도적으로 fence한 뒤 test-only `slot()`이 생산 health 검사를 다시 호출해 snapshot 자체가 실패. quiescent 시험 관측에서 원 registry의 등록 사본만 읽도록 수정. poison 이후 관측도 가능하며 capability를 발급하지 않음 | resident의 `staged_key_rejects_other_slot`, `completion_rejects_other_slot`, `late_semantic_failure_releases_whole_read`; G5 identity/invariant/residency/load; live reader/latest의 오류·effect·frame/pin 보존 검증 PASS |
| E04 / G2 | Root의 PageRef를 바꾼 case3은 현재 decoder에서 IdentityMismatch. 같은 byte의 다른 canonical 손상 case4/7은 Corrupt | `g5_canonical_source_boundaries` PASS. cause·fence·no-write·기존 frame 비교 유지 |
| E05 / G3 | `BeforePanic`는 주입 위치이며 실패 원인 자체가 아님. 현재 G5 residency의 catch 뒤 snapshot 경로를 위 수정으로 완료. bind B15 별도 진단도 예상 unwind 후 정리 PASS | G5 현재 관련5개 함수 PASS. 과거 E05의 정확한 이름은 원 로그 미확보 상태 유지 |
| E09, E12–14 / G4–6 | Profile1 digest·v1 group·구 예약량과 current Profile3 및 legacy empty 호출의 불일치 | live lifecycle·writer·flush·activation 모두 PASS. 6개 live 함수의 원 subcase 유지. 아래 이관 대응 참조 |
| E15–16, E23 / G6 | Free Undo의 구 Profile reference·완료 descriptor·Reserve prefix 불일치. 복구 전 producer에서 막혔던 경로 | 별도 current Free Undo 입력과 실제 producer로 memory gap/cancel/reuse/flush/reload, 실패 경계, Reserved recovery Cancel PASS. 같은 fixture 변경의 role/corruption 사례1개도 PASS |
| E17–22 / G7, D01 | old format1 producer와 base1MiB oracle가 current physical recovery/Profile3/base4에 맞지 않음 | 복구6함수, L01–06/B01–04/V01–04/S01–15/P01–06/E01–10 **45/45 PASS**. 2MiB base·CURRENT/WRITER/epoch·ledger·remaining·home/WAL 전체 비교 유지 |
| E24 / G8 | `6469085f8511852b443c898a6ae2d0f30d1e052b`의 test-only catalog abort hook이 원 PageConcurrency에8B 추가. shared에서 한 번 계수됨 | `s2w_c04_live_and_recovered_peaks_unchanged` PASS. memory64016/65392, native 타입64024/65400. 생산 cap65536 유지, native I/O 실행 주장이 아님 |
| E25 / G9 | Free generation2의 물리 nonmixed 재생은 nested Prefix→Semantic(StateMismatch)에서 거절. top-level Semantic만 기대하던 시험이 낡음 | `pending_and_healthy_corruption_boundaries` PASS. Redo/effect/no-home-write 및 정확한 nested cause 유지 |
| E26 / G10 | 실제 flush authority296B. `a7af5abd8`와 [D3-EMPTY-TXN-003](../contracts/dev3-obligation-format.md#empty-transaction-flush)이 이미384B를 채택했으나 옛 표/시험256B가 남음 | 계약 표와 시험을 기존384B에 맞춤. `status_flush_rf` PASS. 새 상한 증액이 아님 |
| M01 mixed43 | fatal 이후 등록 관측37+child6과 installer pause 안에서 registry를 다시 잠그던 controller가 원인. allocator 이후 pause는 installer가 registry를 가진 상태임 | 관련43 exact PASS. StageOwned에서는 pin count를 읽고, 나머지 cut에서는 실제 `try_pin_state`의 Busy/Unchanged를 확인한 뒤 resume. 원 unwind·정리·비대상 bytes 검증 유지 |
| U01–08 resident cut8 | allocator cut controller의 blocking pin 관측이 installer와 교착.5초 resume timeout 뒤 중첩 unwind로 SIGABRT. 뒤7개는 원 source에서 NOT_STARTED | allocator/dpt_checked/dpt_owned/metadata/payload/publication/registry/stage를 개별 선택해 **8/8 PASS**. 원 ABORT·NOT_STARTED 기록은 그대로 남음 |

### current-format 이관 대응

[새 fixture](../../tests/fixtures/dev3-plan4-empty-retention/current3/README.md)는 hash-pinned Python serializers만 사용한다. 제품 encoder/decoder·실제 실행 결과를 기대값 공급자로 사용하지 않는다. lifecycle63파일1,028,952B와 Free Undo46파일1,296,304B의 index hash를 각 Rust 소비자가 검산한다. [재생성 출력](../../module-gate/evidence/f0-complete/fixture-reproduction.log)은 이미 생성된 bytes와의 동일성을 확인했으며 제품 PASS와 구분한다.

- 첫 이력은 Reserve1/INIT2/AgingStart3/Begin4/Rollback5/Abort6/Finish7, 두 번째는 epoch2/local1의8–11이다. 기존 exact WAL·allocator·Root·Jobs·Status·ATT/Aging/System descriptor와 LSN 검사를 유지한다. base4의 section directory·exact page arena·certificate·XOI·remaining·seal을 독립 필드로 조립한다. INIT provenance는 원 INIT2 frame/image hash로 고정하며 다음 epoch의 현재 Status image로 대체하지 않는다.
- `P4-LIVE-F01`은 실제 read gate와 progress source admission 아래 current prepare/apply를 호출하는 test-only helper를 사용한다. 원 pool/storage 동일성을 확인하고 write guard 반환 뒤 durable 대기 전에 정지한다. RAM에 L4가 채택되고 WAL File에는 L3까지만 있는 동안 L3 capture를 flush하여 L4 frame이 clean되지 않는 것을 확인한다. 공개 `catalog_begin`의 동시 호출 인수로 확대하지 않으며 그 API의 flusher 독점 차용은 그대로다.
- `P4-LIVE-W02`의 잘못된 Status owner는 현재 `begin_identity`에서 **발급 전** 거절되므로 issued/adopted0/0→정상 Begin1/1이다. 발급 **후** 실패 gap은 기존 `mc03_issued_local_gap_recovers`의 별도 source별 PASS와 연결한다. [C3 복구 receipt](../../tests/fixtures/dev3-module-c3/row-recovery/execution-receipts.json)의 MC03(log SHA `f623c99fbebc8976f51a844ebf9038b187870590e10e68882eb911cf308805a8`)를 새 실행으로 표시하지 않는다.
- 새 `P4-REC-L01/L06`은 Status generation3 재사용과 canonical cursor에 따른 Job1 최초 사용(generation2)을 검증한다. 옛 Job0→Job0 단축 이력과 동일하다고 주장하지 않는다. 물리 Job generation 재사용은 기존 Profile3 [CR1 N01 PASS](module-independence-survey-20261026.md#cr1-catalog-native-r2)의 Job1 Root2→Terminal3 및 [N01-L16 입력](../../tests/fixtures/dev3-catalog-create/native3/manifest.json)을 연결한다. 그 source fingerprint `4d5f06a629582c4ea226f7bed30c5bdf8157e3f57bbf0cf2fd2406d39d69a811`/ELF `81a4a0f8cf81e04006ad301d5729c8b63a6770f9628d0c3f1df15b1e4266f657`의 과거 PASS를 이번 native 실행으로 세지 않는다.
- 복구 memory45개의 선택은 그대로이며 static2·ignored native3·resident ignored s5 6개는 자동 실행하지 않았다. Free Undo native용 current 입력도 생성했지만 ignored controller가 실행됐다는 뜻은 아니다. 전체 DEV3·native 인수나 F6 성능 결과로 확대하지 않는다.

### 실행·리뷰·인계

| 실행 | 결과 | 변경과의 관계 |
| --- | --- | --- |
| baseline | 21 FAIL /1 PASS /1 SIGABRT | 사전 동결한 미분류 원인의 bounded 진단. 원 결과 보존 |
| r2 | 58/58 PASS | snapshot·installer cut·cause·census/authority 정정에 직접 해당하는 exact 사례 |
| current1 | 14 PASS /2 FAIL | current fixture/producer·base4 이관16함수. W02 발급 순서와 복구 epoch/base 기대값 오류 보존 |
| current2 | 2/2 PASS | 위2개를 계약/독립 리뷰에 맞춰 수정 후 해당 함수만 재실행 |
| mixed-six | 6/6 PASS | 아직 실행하지 않았던 load_addendum의 fatal snapshot6개 |
| 현재 합집합 | **78 고유 테스트 PASS** | r2의 live reader/latest2개는 이후 fixture 변경으로 current1에서 다시 검증. source별 원 receipt는 별도 유지 |

독립 리뷰에서 second-cycle fixture의 Terminal1/CREATE root1 충돌, base4 Root cell offset·remaining 필드·Reserved chunk의 reserved bytes, W02 발급 시점을 지적해 반영했다. 리뷰와 실행에서 나온 FAIL은 보존한다. 마지막 `cargo fmt`는 형식 정리이며 그것만으로 통과한 제품 시험을 반복하지 않는다. 필수 fmt/Clippy/document 및 정적 module gate는 전송 commit의 pre-push hook에서 확인하고 결과는 PR과 gate artifact에 기록한다.

현재 F0 대상의 known failure와 resident 미완료 경로는 변경 검증까지 끝냈다. 과거 원 로그 미확보는 역사적 attribution 한계로 보존하며 현재 PASS로 과거 실패를 없애지 않는다. F2–F7은 각각의 diff·계약·선택 의무를 적용해 진행할 수 있다. 이 기록은 main 병합, PR CI 또는 전체 lib PASS를 대신하지 않는다. 되돌림은 이 PR의 시험/helper/fixture·현황 정정을 revert하며 옛 fixture·과거 실행 결과는 그대로 남는다.

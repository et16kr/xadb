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

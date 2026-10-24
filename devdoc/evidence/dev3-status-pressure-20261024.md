# DEV3 Status residency 후보 선택 — 제한 선택 인수

2026-10-24 · **독립 D2 수용 / 신규11함수33scenario 선택 PASS / 최종 독립 D1 제한 수용**.
기준 commit `e3d347a`, 구현·통합 `/root`, 독립 검토 `/root/dev3_independent_review`.
[소유 계약](../contracts/dev3-task-05-buffer-flush.md#status-residency-pressure)과
[source별 색인](dev3-status-pressure-20261024.json)이 범위를 정한다.

기존 SharedPool에서 정확 hit→clean/empty→가장 오래된 eligible dirty의 후보를 고른다.
원 latest load와 원 DWB flusher를 소비하며 추가 pool/WAL 권한은 없다. 첫 실제 이력은 allocator+
Status3주소의4주소/3frame에서 자동 퇴출·재적재와 같은 actor의 Active→Aborting→Aborted를
연결했다. clean-first가 남기는 알려진 actor의 명시 flush는 독립 작업으로 구별한다.

| source | 실제 결과 |
| --- | --- |
| r0 `b91c1eaf…` | 새 모듈 경로·PageLsn 비교 compile FAIL, runtime 미실행 |
| r1 `2a630988…` | compile PASS, 새 memory/native 직접 타입 source census1함수 PASS; 제품 effect 미실행 |
| r2 `a19d4745…` | compile PASS, 실제 native wrapper source census1함수 PASS; 제품 effect 미실행 |
| r3 `50838d5b…` | 새 negative test-support 재export 가시성 compile FAIL; runtime 미실행 |
| r4 `82c67303…` | compile PASS, 독립 first-effect gate 뒤 exact hit/round-robin5·empty와 hint 결합1·oldest dirty2·같은 actor3 PASS |
| r5 `8e45e891…` | 독립 gate 뒤 negative6함수20scenario PASS |
| r6 `b38b27b2…` | 새 native 시험의 slice/Vec 비교 compile FAIL; 제품 effect 미실행 |
| r7 `929ac6f4…` | compile PASS; 실제 recovery native Writer source census1함수 PASS65152/115408 |
| r8 `662c4fc1…` | 같은 Rust/ELF와 새 독립 checker4·fmt·Clippy·late metadata1함수2scenario PASS. native producer PASS, recovery exit0/wholefiles PASS 뒤 ledger oracle FAIL; reentry NOT_RUN |
| r9 `62e9701e…` | 중간 S5 435 입력 compile PASS, runtime0; 최종 S5로 채택하지 않음 |
| r10 `eb65da4d…` | Empty slot generation MAX까지 실제 조작하는 최종 S5 438 입력 compile·fmt·Clippy PASS; S5 runtime0 |
| r11 `23c6b656…` | 복구 ledger 수정·이어 실행 준비 compile PASS, runtime0 |
| r12 `bfa3f1ab…` | 새 checker3 PASS; 원 checker4·완료 제품 시험 재실행 없음 |
| r13 `f11f256a…` | 수정 checker의 saved-output audit와 원 r8 ELF의 새 reentry1 PASS; 원 r8 FAIL 보존 |

r1 memory65088/native LinuxWriterLease65024, r2 실제 trace wrapper65152와 retained115408을
확인했다. 원 owner/conversion·loan 크기는 그대로다. source-value와 물리 stack/RSS를 구별한다.
원 FAIL은 보존하며 이미 완료한 static/첫4나 하위 wrapper를 반복하지 않는다. 모든 실행은
`artifacts/dev3/status-residency-pressure-20261024`의 보존 source와 ELF에 결합된다.

r8 native는 producer PID63699의 full bank·home70·wholefiles PASS, recovery PID63701의 exit0와
전체 파일/inode 검산을 완료했지만 checker의 `replay_topup=32768` 기대값 오류로 원 FAIL이다.
allocator+Status3개의 canonical replay 예약은 actual write2개와 구분하여65536이다.
원 source·45파일·FAIL·DB identity를 고정하고 saved-output audit와 당시 미실행 reentry를 분리했다.
r12의 새 ledger·resume 경계·전체snapshot checker3은 PASS이며 원 checker4를 반복하지 않았다.
r13은 독립 이어 실행 gate 뒤 원 r8 ELF를 그대로 사용하여 새 reentry PID를 한 번 실행했고 PASS다.
원 두 process의 저장 결과만 수정 oracle로 재검토했으며 제품 실행을 반복하지 않았다.
원 r8 `FAIL_RECOVERED_LEDGER`와 당시 reentry NOT_RUN은 덮어쓰지 않았다.
[확정 입력](../../tests/fixtures/dev3-status-pressure/manifest.json)의 S5 profile3×frame4/8/64·위치/세대438은
ignored로 작성하고 NOT_RUN으로 남겼다. frame64는 첫 extent 한도 안의62상주+1empty이며
65주소 작업 집합을 주장하지 않는다. 최종 독립 D1을 제한 단위로 수용했다.
일반 page 종류·큰 profile backing·다음 extent·FIFO/독립 oldest service/forward admission·live undo,
첫 행 ACK·일반 READY·전체 DEV3/S5는 이 단위로 승격하지 않는다. AI 소비자1/3과 효율 비교
NOT_RUN을 유지한다. 문서 verifier는 pre-push gate이므로 편집·commit 중 실행하지 않는다.

실행 도구의 timeout partial stdout/stderr 보존·120초 batch budget 보완은 독립 검토를 받았고,
새 runner 자체시험3은 PASS다. native fixture 준비 중 Active C bank 전체0 기대값을 독립 검토에서
찾아 실행 전에 수정했다. B terminal row0와 C Active row1의 실제53248byte bank를 독립 raw 값으로
비교한다. 이 준비 수정은 제품 FAIL이나 native PASS로 세지 않는다.

## 실제 파일·프로세스 인계와 남는 경계

원 producer PID63699는 allocator+2payload의 pool에서 Status3주소를 유지하며 B Begin7/C Begin8,
B 재적재→RollbackStart9→재적재→AbortEnd10을 실제 수행했다. 알려진 B의 명시 flush1회는
후보가 반환한 flush2회와 구분한다. 세 INIT flush를 포함한 batch6·generation19 뒤 home는
allocator0/A2/B9/C8이고, C Active의6080 WAL 의무와 exact bank row1이 남는다.
새 recovery PID63701은 allocator6/B10 두 home만 쓰고 batch8·generation25로 끝났으며 C Active는
유지했다. r13 재진입은 같은 r8 ELF로 별도 PID에서 같은 전체파일·WAL·원장·FD를 확인하고
home write0을 검산했다. 복사 전 원본45파일/bytes/hash/device/inode와 복사 뒤 새 inode baseline을
분리하여 이전 파일을 바꿔 인수를 통과시키지 않았다. 실제 프로세스3회이며 과거2회를 새 실행 PASS로 세지 않는다.

원 native2.210244초와 이어 실행의 검산/복사/실행0.901416초는 합3.111660초다. 저장 source별
new runtime11함수33scenario·static3·checker7·artifact runner3과 구분한다. S5 438은 NOT_RUN이다.
원 제품 load/flush/복구 코드는 바꾸지 않았으므로 무변경 기존 leaf·compiler probe·전체 wrapper를
반복하지 않았다. 새 반환값은 Copy 관측이고 cursor는 권한 없는 hint라 새 borrow capability가 없다.
최종 Rust가 같은 r10에서 fmt·Clippy를 통과했으며 이후 변경은 새 Python checker·기록뿐이다.

최종 독립 D1은38개 변경 경로를 전수 검토했다(OCR15·수동 제외23, skipped0).
`review/d1-final.md`, `d1-file-accounting.json`, `final-evidence-audit.json`의 원 판정을 같은
artifact 디렉터리에 보존한다. 제품/시험 지적은 남아 있지 않으며 S5·일반 pool/live undo·DEV3
전체 인수는 이 제한 판정 밖이다.

# DEV3 allocator 동일 주소의 resident 세대 reload

2026-10-23 · **D3-ALLOCATOR-RESIDENCY-001 · 제한 구현의 D2·제품 선택 인수·최종 독립 D1 수용**

[buffer/flush 소유 계약](dev3-task-05-buffer-flush.md#registration-core) · [allocator flush](dev3-allocator-flush.md) · [dirty stage](dev3-dirty-stage.md) · [중간·S5 시험 배정](../plans/dev3-test-plan.md)

## 범위와 결정

기준 `d31cfb9`의 등록 코어를 실제 allocator 소비자가 사용한다. `OperationalAllocation`은 allocator-only `BoundAllocation`을 소비하고 원 owner·writer descriptor·active reservation·WAL·flush workspace·fence·binding과 원래 Drop을 그대로 보유한다. `reload_allocator(&mut self)`가 모든 split view의 반환을 요구한다. 같은 allocator 주소의 clean image를 selected namespace에서 실제 다시 읽고 resident generation만 증가시킨다. Empty·eviction·서로 다른 주소의 교체·N+1 집합·Status 반복 변경·일반 recovery/READY는 후속 범위다.

D2 선택은 원 owner를 감싼 동일 주소 adapter다. descriptor를 재구성하는 대안은 active 예약과 retained 의무 보존을 어렵게 하고, 일반 pool 전환을 함께 구현하는 대안은 주소·replacement·load claim까지 새 불변식을 늘린다. 이 adapter는 기존 writer/reader/flusher와 compact generation token을 즉시 소비하며 새 dependency·persistent format을 추가하지 않는다. 일반 pool이 이 수명 경로를 직접 제공할 때 adapter의 통합·제거를 재검토한다. 계약·제품·통합은 `/root`, native/compiler/checker는 `/root/allocator_reload_acceptance`, 별도 읽기 검토는 `/root/pool_design_review`가 담당한다.

## 전환과 최종 설치

전환은 건강한 allocator-only origin·typed 자원 admission·writer installation 없음·terminal pending 없음·flush Ready/capture 없음/retired 없음·all pins0·all DPT/requirement 없음·flush claim 없음·frame/DPT 일관성을 요구한다. 공개된 Status와 다른 등록은 허용하지 않는다. 무효과 거절은 정확한 원 Bound를 돌려준다. 원 Bound의 최종 Drop은 공통 fence와 binding=ABANDONED를 설치한다. 성공 cleanup이나 I/O를 수행하지 않는다.

등록 directory의 private mode는 Fixed 또는 AllocatorReloadable이다. Fixed bootstrap·일회 Status 공개는 종전 generation1 계약을 유지한다. reload mode는 원 frame의 allocator request를 provenance로 검산하고 같은 주소의 현재 nonzero generation을 조회한다. generation은 checked 증가하며 MAX 거절은 I/O0이다.

1. exclusive owner 아래 원 frame을 짧은 content read로 caller scratch에 복사하고 latch를 놓는다. pins·DPT/requirement·claim·writer/flush 의무를 모두 사전 검산한다.
2. actual complete==synced WAL prefix와 원 WriterControl의 descriptor/batch/high-water 관측을 확인한다. 이 adapter의 clean pageLSN은 그 prefix와 같아야 한다. 일반 page 최신성 규칙으로 확대하지 않는다.
3. scoped PageFileReader가 actual profile·FILEMAP·opened handle을 읽고 정확한 allocator identity/kind/level을 검산한다. 원 clean RAM과 full16384 bytes가 같아야 한다. reader와 root/scratch loan이 끝난 뒤 prefix 불변·원 descriptor를 다시 확인한다.
4. private loaded-image permit가 origin·before stamp·검산 bytes를 결합한다. 최종 설치는 exact old registration/stamp와 quiescence를 재검사하고 directory generation을 증가시킨 뒤 동일 bytes를 copy한다. 첫 copy 이후에는 오류를 반환하거나 다른 lock을 획득하지 않는다. 새 세대의 clean counter는 LSN0이면0, 양수이면1이며 recLSN은 None이다.

이전 세대 permit는 새 dirty 상태를 지우지 못하고 fence한다. 원 page/ref incarnation·예약 work high-water와 resident generation은 서로 다른 counter다. full bytes 비교는 actual home을 읽었다는 증거와 결합하며 단순 RAM 재등록으로 대체하지 않는다.

## 실패와 소유 보존

일반 Read 오류는 lower effect가 Unchanged이고 certainty가 Known일 때만 건강한 명시 재시도를 허용한다. namespace Io도 lower Unchanged일 때만 같은 의미다. 원 typed cause/effect/certainty를 보존하며 CallerMustResolve/Unknown을 무효과로 낮추지 않는다. namespace 구조 오류·잘못된 identity/LSN/bytes·poison·unwind·공통 fence는 fatal이다.

실패는 frame bytes·registration/generation·writer descriptor를 보존한다. 읽기 scratch는 실패 중 바뀔 수 있다. 세대 발급 전 실패를 성공 load로 집계하지 않는다. retained orphan pin/claim의 테스트는 정상 cleanup을 주장하지 않는다.

## F05 자원과 실행 전 gate

caller operation98304의 기존 슬롯을 사용한다: root0..16384, reader scratch16384..32768, prior32768..49152, loaded65536..81920. 새 page buffer나 flush4MiB loan을 추가하지 않는다. metadata conversion은 Bound+Operational+Result+2048≤65536이고 각 Bound≤20800/Result≤21504다. reload는 File≤4096, Reader≤8192, `2*Reader+4*File+3072+4096+16384+4096≤65536`으로 제한한다. constructor와 descriptor observation은 겹치지 않으며16384는 hash를 포함한 두 phase의 max 상한이다. outer4096은 snapshot·stamp·prefix·request·permit·반환/관측·reference carrier를 별도 검산한다.

r9 실제 크기는 conversion61952, outer2400이다. mock File32/Reader384, Linux File48/Reader400이며 File4096/Reader4448 경계는 수용한다. File4104는 거절한다. File8200/Reader8552의 동시 초과도 거절하며 독립 Reader 분기 검출로 주장하지 않는다. product 식의 worst ceiling은60416이다. native helper는 actual max14560을 사용한26816, padded51104를 별도 출력한다.

mode 증가 뒤 pool delta360≤512, dirty hot165480/cold154992≤196608이다. Status J3608≤3648·hot193608/cold157440이며 hot+flush65536=259144≤262144다. 기존 reserve196608+flush65536=262144를 유지한다. 이 값은 source 수명/sizeof 회계이며 OS stack/RSS/OOM 측정이 아니다.

native는 fresh Linux DB1/두 batch/두 reload, worker30초·DB24MiB·고정 소유22073344bytes·tracked File3·DB FD5를 실행 전에 고정한다. 보존 WAL1+reader container1+HOME1이 File3이다. caller heap payload5373952bytes와 초기 setup stack49152bytes는 서로 다른 수명이다. compiler 세 사례 합계30초, unit 사례당30초다. PF09 성능 인수를 주장하지 않는다.

## 시험 inventory와 S5

중간에는 새 runtime19개를 exact 선택한다. 성공1→2→3 세대와 gen3 새 reserve/cancel, pristine0, dirty/pin/claim/DPT/requirement/writer/retired 의무의 I/O0 거절, generation MAX, actual read6위치와 open4위치의 원 오류·명시 재시도, source4손상과 actual older valid home, poison3, old seal, 원 Bound 반환, namespace8분류·불명 read 보존, 공통 fence·unwind2가 포함된다. static3개는 새 layout과 변경 shared dirty/CF17-01이다. mode의 Fixed resolve/valid 변경을 직접 소비하는 기존 registration_closed_publication_is_single_and_checked·registration_copy_request_rejects_invalid_fixed_publication 두 runtime과 수정된 registration_layout_receipt static1을 supplement로 별도 선택한다.

Linux 한 이력의 independent literal full RAM/home/WAL/descriptor·namespace mutation0·inode/길이/hash·6 reads/reload·FD를 검산한다. compiler는 실제 baseline·release positive·live split E0499/line8 negative, checker는 새 known-bad10개다. native wrapper의 제한 중복은 기존 flush wrapper를 오염시키지 않고 read trace와 두 reload를 격리하려는 선택이다. native 사례가 더 늘거나 공통 포트 계약이 바뀌면 공통화 비용을 재검토한다.

최종 S5 확대는 ignored `reload_s5_mixed_count_epoch_history`: count1..8을 네 번 순환하는32round,64reload와 active/cancel/high-water/LSN/세대 검산이다. 이번 중간 실행 대상이 아니다. 최종 DEV3 inventory에서 이 selector와 중간 사례를 합쳐 실행하며 무변경 하위 전체 wrapper로 확대하지 않는다.

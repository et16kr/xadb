# XADB

XADB는 **오픈소스 DBMS를 참고하여, AI가 이해하고 개발하기 좋은 DBMS를 AI agent로 만들 수 있는지 시험하는 저장소**입니다. 현재는 초기 개발 단계입니다.

최종 목표는 Linux에서 Rust로 구현하는 관계형 DBMS입니다. 폭넓은 SQL·트랜잭션·복구 기능과 기본 성능을 갖추면서, 구조를 이해하고 수정하고 검증할 수 있는 구현을 목표로 합니다. 특정 workload만을 위한 복잡한 튜닝은 측정 근거가 있을 때 추가합니다.

PostgreSQL·MariaDB·CUBRID·SQLite 등 공개 DBMS의 문서와 구현을 비교 자료로 사용합니다. 각 기능의 의미와 인수 기준은 XADB 계약으로 정하며, 참고 제품의 SQL·파일·wire 호환이나 성능 우월성을 자동으로 주장하지 않습니다. [제품 방향](devdoc/plans/project-direction.md) · [공개 참고 자료와 로컬 소스 경로](devdoc/reference/opensource-references.md)

현재 DEV0 검증 도구, DEV1 공통 의미·Linux 효과 경계, DEV2 P0 독립 이력 oracle와 K0 volatile 행·catalog·native worker의 일부 구현을 이관했습니다. 외부 streaming controller와 독립 catalog 판정·저장 recipe 재실행 경로를 추가 이관했으며, 시험 패키지를 재구성한 당시 소스에서 DEV0·DEV1 회귀와 DEV2 P0/K0 leaf 검증을 실제 실행했습니다. **K1–K5 영속 transaction/WAL/recovery는 구현 연결·일부 로컬 검증을 마쳤으며 최종 인수는 대기 중입니다. 디스크 페이지·인덱스, SQL·서버는 아직 구현되지 않았습니다.** 이전 회귀 결과는 현재 소스의 검증을 대신하지 않습니다. K0의 임시 메모리 상태는 커널 개발 기반이며 독립 메모리 DBMS 제품이 아닙니다. [현재 구현과 실행 상태](devdoc/implementation/README.md)

COMM의 AXF1 stream·Linux local socket·quota·deadline·취소/종료와 focused 시험 실행기를 추가했습니다. 전체 C1 인수는 미완료이며 [COMM 구현 기록](devdoc/implementation/comm-implementation.md)에서 검증 범위와 잔여 작업을 확인합니다.

## 소스 구성

제품 패키지는 `crates/engine`, `crates/foundation`, `crates/platform`, `crates/transport`에 둡니다. 시험 도구는 `crates/testing` 아래 `controller`, `protocol`, `reference`, `fixture-worker`, `foundation-worker`, `engine-worker`, `transport-worker`로 묶습니다. 각 폴더는 독립 Cargo 패키지이며 `xadbtest`, `xadbtest-reference` 등 패키지·실행 명령 이름은 유지합니다. [패키지 배치와 의존 방향](devdoc/policies/repository-build-plan.md)

## 빌드와 검증

Linux x86_64, Rust 1.95.0과 `Cargo.lock`을 사용합니다.

```bash
cargo build --workspace --locked
python3 scripts/check_dev0.py
python3 scripts/check_dev1.py
python3 scripts/check_dev2_reference.py --with-substrate
python3 -B scripts/check_comm.py --snapshot --with-regressions
```

DEV1은 DEV0 회귀를, DEV2 실행기는 DEV0/DEV1 회귀를 포함합니다. XADBTest의 fixture 성공과 volatile substrate 성공은 실제 DB 내구성이나 제품 출시 검증이 아닙니다. 현재 실행 결과는 [증거 색인](devdoc/evidence/README.md)에서 확인합니다. 개발 빌드는 새 DB·fixture를 사용하고, 안정 릴리스의 호환 기준선은 별도로 선언합니다.

## 개발 문서

[devdoc/README.md](devdoc/README.md)에서 정책·계획·계약·현재 구현을 찾습니다. 문서는 Markdown만 작성하며 JSON은 검증기가 소비하는 계약·색인·실행 결과에 사용합니다. 사용자용 `xadb` SQL CLI는 계획이며 현재 실행 가능한 도구는 `xadbtest`와 시험 worker입니다.

문서 검사는 push 직전에 수행하고 편집·commit 때에는 실행하지 않습니다. 새 clone에서 기존 사용자 훅이 없는 경우 한 번 설정합니다.

```bash
git config --local core.hooksPath scripts/git-hooks
```

이후 `git push`는 전송할 각 ref의 최종 commit을 임시 사본으로 내보내 `cargo fmt --all --check`와 문서 검사를 모두 통과해야 진행합니다. 포맷 오류는 `cargo fmt --all`로 수정하고 commit에 포함한 뒤 다시 push합니다. 기존 훅이 있으면 먼저 보존·연결하며, 세부 범위는 [push 검사 규칙](devdoc/policies/documentation-policy.md#push-gate)을 따릅니다.

기존 개발 작업의 현재 파일을 독립 저장소로 이관했습니다. 이전 Git 이력을 가져오지 않았으며, 이전 실행 결과를 XADB의 결과로 재표기하지 않습니다. [이관 범위와 상태](devdoc/history/project-migration.md)

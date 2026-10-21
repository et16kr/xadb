# 공개 DBMS 참고 자료

2026-10-18 · **XADB의 비교·연구용 소스와 문서 목록**

## 로컬 소스 경로

현재 작업 환경에는 아래 10개 저장소를 `/home/et16/work` 아래에 복제했다. XADB 루트 `/home/et16/work/xadb`에서는 각각 `../postgres`, `../mariadb`와 같은 형제 디렉터리다. 다른 환경에서는 같은 공식 저장소의 checkout 위치를 확인해 사용한다. 이 경로는 연구용이며 XADB의 빌드 의존성이나 필수 검증 입력이 아니다.

`소스 진입점`은 각 checkout 루트 기준 상대 경로다. 기능 작업에 필요한 책임부터 선택해 읽는다.

| 대상 | 로컬 절대 경로 | 공식 소스 | 검토할 책임·소스 진입점 |
| --- | --- | --- | --- |
| PostgreSQL | `/home/et16/work/postgres` | [공식 Git 미러](https://github.com/postgres/postgres) | transaction/WAL `src/backend/access/transam`, 버퍼 `src/backend/storage/buffer`, planner `src/backend/optimizer` |
| MariaDB | `/home/et16/work/mariadb` | [저장소](https://github.com/MariaDB/server) | SQL 계층 `sql`, InnoDB 저장·복구 `storage/innobase` |
| CUBRID | `/home/et16/work/cubrid` | [저장소](https://github.com/CUBRID/cubrid) | 저장 `src/storage`, 트랜잭션·복구 `src/transaction`, 질의 처리 `src/query` |
| MySQL | `/home/et16/work/mysql-server` | [저장소](https://github.com/mysql/mysql-server) | SQL 계층 `sql`, InnoDB 저장·복구 `storage/innobase` |
| SQLite | `/home/et16/work/sqlite` | [공식 Git 미러](https://github.com/sqlite/sqlite) | B-tree `src/btree.c`, 페이지·WAL `src/pager.c`·`src/wal.c`, SQL VM `src/vdbe.c` |
| DuckDB | `/home/et16/work/duckdb` | [저장소](https://github.com/duckdb/duckdb) | 분석 질의 실행 `src/execution`, 저장 계층 `src/storage` |
| ClickHouse | `/home/et16/work/clickhouse` | [저장소](https://github.com/ClickHouse/ClickHouse) | 컬럼 저장 `src/Storages/MergeTree`, 실행 파이프라인 `src/Processors` |
| TiDB | `/home/et16/work/tidb` | [저장소](https://github.com/pingcap/tidb) | 분산 SQL 계획·실행 `pkg/planner`·`pkg/executor`, KV 경계 `pkg/kv` |
| FoundationDB | `/home/et16/work/foundationdb` | [저장소](https://github.com/apple/foundationdb) | 분산 KV·트랜잭션 `fdbserver`, 비동기 실행 기반 `flow` |
| RocksDB | `/home/et16/work/rocksdb` | [저장소](https://github.com/facebook/rocksdb) | LSM·compaction `db`, 테이블 형식 `table`, 트랜잭션 `utilities/transactions` |

FoundationDB·RocksDB는 KV 저장 계층의 비교 자료다. 분석용·분산 시스템의 구현도 관련 책임을 비교할 때 사용하며, XADB의 [제품 방향](../plans/project-direction.md)이나 해당 기능의 소유 계약을 대신하지 않는다.

## 로컬 checkout 확인 기록

2026-10-18 확인 기준으로 모두 원격 기본 브랜치를 `--depth 1 --no-tags`로 복제했다. 원격 주소, HEAD, 작업 트리와 Git 객체 연결 상태를 확인했다. 전체 Git 이력과 서브모듈은 내려받지 않았고 빌드·제품 시험은 실행하지 않았다. 아래 commit은 이 확인 시점의 기록이며 이후 `fetch`·`pull`·`checkout`으로 달라질 수 있다.

| 대상 | 브랜치 | 확인한 commit |
| --- | --- | --- |
| PostgreSQL | `master` | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` |
| MariaDB | `main` | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` |
| CUBRID | `develop` | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` |
| MySQL | `trunk` | `3b99be40f8a31d9396cad10accfd3ba51647d613` |
| SQLite | `master` | `43a48f77585a88940c727f33682151418f544059` |
| DuckDB | `main` | `1b4c3e54ba3625bac56ad184f0ce642a8d2bfbd5` |
| ClickHouse | `master` | `1d76716b7580f78d0375c626736e49a3e13f099c` |
| TiDB | `master` | `328460a9af032ec6b53823724f87837a6d2d5857` |
| FoundationDB | `main` | `487620d5aa41152b1f034f4e932005a87eb2947f` |
| RocksDB | `main` | `54732b0c8f6f8af34f973d7fae0a234022fcf1fa` |

연구를 시작할 때 실제 checkout의 원격·revision과 작업 트리 변경을 다시 확인한다. 예를 들어 CUBRID는 다음과 같이 확인할 수 있다.

```bash
git -C /home/et16/work/cubrid remote get-url origin
git -C /home/et16/work/cubrid rev-parse HEAD
git -C /home/et16/work/cubrid status --short
```

## 공식 문서와 추가 비교 후보

| 대상 | 공식 자료 | 검토할 책임 |
| --- | --- | --- |
| PostgreSQL | [Internals](https://www.postgresql.org/docs/18/internals.html), [Table AM](https://www.postgresql.org/docs/18/tableam.html), [소스](https://github.com/postgres/postgres) | SQL 처리 단계, catalog, storage 경계, WAL, transaction, planner/executor |
| MariaDB | [Architecture](https://mariadb.com/docs/server/architecture), [개발 안내](https://mariadb.org/get-involved/getting-started-for-developers/), [소스](https://github.com/MariaDB/server) | SQL 계층과 저장 엔진, transaction·인덱스·운영 인터페이스 |
| SQLite | [Architecture](https://www.sqlite.org/arch.html), [Testing](https://www.sqlite.org/testing.html), [소스](https://sqlite.org/src) | 작은 모듈 경계, pager/파일 효과, 독립 시험·fault 검증 |
| WiredTiger | [Transactions](https://source.wiredtiger.com/develop/arch-transaction.html), [소스](https://github.com/wiredtiger/wiredtiger) | 직접 storage API, transaction/checkpoint와 자원 관리의 비교 후보 |
| toyDB | [공식 저장소](https://github.com/erikgrinaker/toydb) | 이해 가능한 Rust SQL DBMS의 학습·검증 범위와 비교 |

이번 이관에서 PostgreSQL internals, MariaDB architecture, SQLite architecture/testing의 공식 문서 진입점을 확인했다. WiredTiger·toyDB의 세부 구현 연구도 해당 기능 작업 때 별도로 수행한다.

WiredTiger·toyDB는 위 10개 로컬 checkout에 포함되지 않은 추가 비교 후보다.

## 연구 기록과 적용

각 연구 기록에는 실제 URL·버전/commit, 읽은 부분, 질문, 채택/기각 이유, XADB 소유 계약과 필요한 시험을 남긴다. 여러 DBMS의 동작이 다르면 XADB의 선택과 경계를 명시한다. 아직 조사하지 않은 기능을 기존 코드의 근거로 소급 기재하지 않는다.

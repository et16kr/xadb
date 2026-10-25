//! K0 native adapter to the actual volatile substrate. No expected values or verdicts.
//! A protocol error permanently fences this session before any further engine effect.
pub mod durable;
pub mod durable_performance;
mod fault;
mod measurement_window;
pub mod performance;

use engine::{
    api,
    substrate::{Cursor, RowView, Snapshot, VolatileStore},
};
use foundation::value::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use test_protocol::{engine as wire, frame::Envelope};

#[derive(Debug, PartialEq, Eq)]
pub struct ProtocolError(pub &'static str);
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ProtocolError {}

enum Failure {
    Protocol(ProtocolError),
    Product(engine::Error),
}
impl From<ProtocolError> for Failure {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}
impl From<engine::Error> for Failure {
    fn from(value: engine::Error) -> Self {
        Self::Product(value)
    }
}

struct Aliases<T>(BTreeMap<u64, Option<T>>);
impl<T> Aliases<T> {
    fn new() -> Self {
        Self(BTreeMap::new())
    }
    fn reserve(&mut self, alias: u64) -> Result<(), ProtocolError> {
        if !(1..=1024).contains(&alias) || self.0.contains_key(&alias) {
            return Err(ProtocolError("invalid or previously consumed alias"));
        }
        self.0.insert(alias, None);
        Ok(())
    }
    fn install(&mut self, alias: u64, value: T) {
        // Only called after reserve succeeded, without another dispatch in between.
        *self.0.get_mut(&alias).expect("reserved alias") = Some(value);
    }
    fn get(&self, alias: u64) -> Result<&T, ProtocolError> {
        self.0
            .get(&alias)
            .and_then(Option::as_ref)
            .ok_or(ProtocolError("missing or retired alias"))
    }
    fn get_mut(&mut self, alias: u64) -> Result<&mut T, ProtocolError> {
        self.0
            .get_mut(&alias)
            .and_then(Option::as_mut)
            .ok_or(ProtocolError("missing or retired alias"))
    }
    fn retire(&mut self, alias: u64) -> Result<(), ProtocolError> {
        self.get(alias)?;
        self.0.insert(alias, None);
        Ok(())
    }
    fn active(&self) -> usize {
        self.0.values().filter(|slot| slot.is_some()).count()
    }
}

/// One owned worker run. The process loop adds byte limits; the controller owns deadlines.
/// This object checks
/// invocation identities and retains actual engine capabilities for the whole run.
pub struct Session {
    run: Option<[u8; 16]>,
    requests: BTreeSet<u64>,
    poisoned: bool,
    performance_complete: bool,
    store: Option<VolatileStore>,
    database: [u8; 16],
    timeline: u64,
    tables: Aliases<api::TableHandle>,
    rows: Aliases<api::RowUid>,
    snapshots: Aliases<Snapshot>,
    cursors: Aliases<Cursor>,
}
impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
impl Session {
    pub fn new() -> Self {
        Self {
            run: None,
            requests: BTreeSet::new(),
            poisoned: false,
            performance_complete: false,
            store: None,
            database: [0; 16],
            timeline: 0,
            tables: Aliases::new(),
            rows: Aliases::new(),
            snapshots: Aliases::new(),
            cursors: Aliases::new(),
        }
    }

    /// Product errors are observations. Envelope/alias/session errors are fatal and
    /// every subsequent dispatch is rejected, even if its inputs would be valid.
    pub fn dispatch(&mut self, request: wire::Request) -> Result<wire::Response, ProtocolError> {
        if self.poisoned || self.performance_complete {
            return Err(ProtocolError("session is poisoned"));
        }
        let result = self.execute(&request);
        let observation = match result {
            Ok(observation) => observation,
            Err(Failure::Product(error)) => {
                if self.store.is_none() {
                    self.poisoned = true;
                }
                wire::Observation::ProductError {
                    code: error.code.id().into(),
                }
            }
            Err(Failure::Protocol(error)) => {
                self.poisoned = true;
                return Err(error);
            }
        };
        Ok(wire::Response {
            version: wire::VERSION,
            run: request.run,
            request: request.request,
            actor: request.actor,
            observation,
        })
    }

    fn execute(&mut self, request: &wire::Request) -> Result<wire::Observation, Failure> {
        request
            .validate()
            .map_err(|_| ProtocolError("invalid envelope"))?;
        if !self.requests.insert(request.request) {
            return Err(ProtocolError("duplicate request").into());
        }
        if let Some(run) = self.run {
            if run != request.run {
                return Err(ProtocolError("run identity mismatch").into());
            }
            if matches!(request.action, wire::Action::Initialize { .. }) {
                return Err(ProtocolError("initialize only once").into());
            }
        } else {
            if request.request != 1
                || request.actor != 0
                || !matches!(request.action, wire::Action::Initialize { .. })
            {
                return Err(ProtocolError(
                    "first request must initialize as actor zero/request one",
                )
                .into());
            }
            self.run = Some(request.run);
        }
        use wire::{Action as A, Observation as O};
        if let A::Initialize {
            database,
            timeline,
            epoch,
            resident_bytes,
            snapshot_pins,
        } = &request.action
        {
            let store = VolatileStore::new(
                api::DatabaseId::new(*database)?,
                *timeline,
                *epoch,
                api::StoreOptions {
                    resident_bytes: *resident_bytes,
                    snapshot_pins: *snapshot_pins,
                },
            )?;
            self.database = *database;
            self.timeline = *timeline;
            self.store = Some(store);
            return Ok(O::Ready {});
        }
        let store = self
            .store
            .as_mut()
            .ok_or(ProtocolError("initialize did not succeed"))?;
        match &request.action {
            A::Initialize { .. } => unreachable!("handled above"),
            A::Create { table, spec } => {
                self.tables.reserve(*table)?;
                let handle = store.create_table(to_spec(spec))?;
                let identity = table_identity(&handle, self.database, self.timeline);
                self.tables.install(*table, handle);
                Ok(O::Table {
                    identity,
                    spec: spec.clone(),
                })
            }
            A::Lookup {
                snapshot,
                table,
                name,
            } => {
                let view = self.snapshots.get(*snapshot)?;
                self.tables.reserve(*table)?;
                let handle = view.lookup_table(name)?;
                let spec = from_spec(view.schema(&handle)?);
                let identity = table_identity(&handle, self.database, self.timeline);
                self.tables.install(*table, handle);
                Ok(O::Table { identity, spec })
            }
            A::DropTable { table } => {
                store.drop_table(self.tables.get(*table)?)?;
                // Keep the capability so old snapshots can still read it. The alias
                // remains consumed; a new snapshot rejects it through the engine.
                Ok(O::Done {})
            }
            A::Insert { table, row, values } => {
                let handle = self.tables.get(*table)?;
                self.rows.reserve(*row)?;
                let uid = store.insert(handle, to_values(values))?;
                self.rows.install(*row, uid);
                Ok(O::Inserted {
                    identity: row_identity(uid),
                })
            }
            A::Replace { table, row, values } => {
                store.replace(
                    self.tables.get(*table)?,
                    *self.rows.get(*row)?,
                    to_values(values),
                )?;
                Ok(O::Done {})
            }
            A::Delete { table, row } => {
                store.delete(self.tables.get(*table)?, *self.rows.get(*row)?)?;
                // Stable RowUID aliases remain usable to observe absence and old rows.
                Ok(O::Done {})
            }
            A::Snapshot { snapshot } => {
                self.snapshots.reserve(*snapshot)?;
                self.snapshots.install(*snapshot, store.snapshot()?);
                Ok(O::Done {})
            }
            A::ReleaseSnapshot { snapshot } => {
                self.snapshots.retire(*snapshot)?;
                Ok(O::Done {})
            }
            A::Read {
                snapshot,
                table,
                row,
            } => {
                let value = self
                    .snapshots
                    .get(*snapshot)?
                    .read(self.tables.get(*table)?, *self.rows.get(*row)?)?;
                Ok(O::Row {
                    value: value.as_ref().map(from_row),
                })
            }
            A::OpenScan {
                snapshot,
                table,
                cursor,
            } => {
                let view = self.snapshots.get(*snapshot)?;
                let handle = self.tables.get(*table)?;
                if self.cursors.active() >= 32 {
                    return Err(ProtocolError("cursor limit").into());
                }
                self.cursors.reserve(*cursor)?;
                self.cursors.install(*cursor, view.scan(handle)?);
                Ok(O::Done {})
            }
            A::NextPage { cursor, limit } => {
                let page = self.cursors.get_mut(*cursor)?.next_page(*limit as usize)?;
                Ok(O::Page {
                    rows: page.rows().iter().map(from_row).collect(),
                    eof: page.eof(),
                    canonical_bytes: page.canonical_bytes() as u32,
                })
            }
            A::CloseScan { cursor } => {
                self.cursors.retire(*cursor)?;
                Ok(O::Done {})
            }
            A::Metrics {} => {
                let m = store.metrics();
                Ok(O::Metrics {
                    counters: wire::Metrics {
                        operations: m.operations,
                        rows_read: m.rows_read,
                        row_references_copied: m.row_references_copied,
                        resident_bytes: m.resident_bytes,
                        snapshot_pins: m.snapshot_pins,
                    },
                })
            }
            A::Performance { input } => {
                if request.actor != 0 || request.request != 2 || self.requests.len() != 2 {
                    return Err(
                        ProtocolError("performance must immediately follow initialize").into(),
                    );
                }
                if !input.validate() {
                    return Err(ProtocolError("invalid performance profile").into());
                }
                self.performance_complete = true;
                Ok(O::Performance {
                    output: Box::new(performance::run(input)?),
                })
            }
        }
    }
}

fn table_identity(
    handle: &api::TableHandle,
    database: [u8; 16],
    timeline: u64,
) -> wire::TableIdentity {
    let identity = handle.identity();
    wire::TableIdentity {
        database,
        timeline,
        object: identity.object.get(),
        generation: identity.generation.get(),
        schema_epoch: identity.schema_epoch.get(),
    }
}
fn row_identity(uid: api::RowUid) -> wire::RowIdentity {
    let identity = uid.table_identity();
    wire::RowIdentity {
        table: wire::TableIdentity {
            database: uid.database_id().bytes(),
            timeline: uid.timeline(),
            object: uid.table_id().get(),
            generation: identity.generation.get(),
            schema_epoch: identity.schema_epoch.get(),
        },
        row: uid.row_id().get(),
    }
}
fn to_spec(spec: &wire::TableSpec) -> api::TableSpec {
    api::TableSpec {
        name: spec.name.clone(),
        columns: spec
            .columns
            .iter()
            .map(|column| api::Column {
                name: column.name.clone(),
                nullable: column.nullable,
                data_type: match column.data_type {
                    wire::DataType::Bool => api::DataType::Bool,
                    wire::DataType::Int64 => api::DataType::Int64,
                    wire::DataType::Utf8 => api::DataType::Utf8,
                    wire::DataType::Binary => api::DataType::Binary,
                },
            })
            .collect(),
    }
}
fn from_spec(spec: &api::TableSpec) -> wire::TableSpec {
    wire::TableSpec {
        name: spec.name.clone(),
        columns: spec
            .columns
            .iter()
            .map(|column| wire::Column {
                name: column.name.clone(),
                nullable: column.nullable,
                data_type: match column.data_type {
                    api::DataType::Bool => wire::DataType::Bool,
                    api::DataType::Int64 => wire::DataType::Int64,
                    api::DataType::Utf8 => wire::DataType::Utf8,
                    api::DataType::Binary => wire::DataType::Binary,
                },
            })
            .collect(),
    }
}
fn to_values(values: &[wire::Cell]) -> Vec<Value> {
    values
        .iter()
        .map(|cell| match cell {
            wire::Cell::Null => Value::Null,
            wire::Cell::Bool(v) => Value::Bool(*v),
            wire::Cell::Int64(v) => Value::Int64(*v),
            wire::Cell::Utf8(v) => Value::Utf8(v.clone()),
            wire::Cell::Binary(v) => Value::Bytes(v.clone()),
        })
        .collect()
}
fn from_row(row: &RowView) -> wire::Row {
    wire::Row {
        identity: row_identity(row.uid()),
        values: (0..row.len())
            .map(
                |column| match row.value(column).expect("column within row") {
                    Value::Null => wire::Cell::Null,
                    Value::Bool(v) => wire::Cell::Bool(*v),
                    Value::Int64(v) => wire::Cell::Int64(*v),
                    Value::Utf8(v) => wire::Cell::Utf8(v.clone()),
                    Value::Bytes(v) => wire::Cell::Binary(v.clone()),
                },
            )
            .collect(),
    }
}

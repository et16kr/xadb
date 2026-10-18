//! Independent, bounded sequential K0 catalog oracle.
//!
//! State is copied into small BTreeMaps. No engine/foundation implementation,
//! allocator layout, codec, or production comparator is consulted.
use std::collections::{BTreeMap, BTreeSet};
use xadbtest_protocol::engine::{
    Action, Cell, DataType, Metrics, Observation, Request, Response, Row, RowIdentity,
    TableIdentity, TableSpec,
};

const INPUT_BYTES: usize = 8 * 1024 * 1024;
const RESIDENT_BYTES: u64 = 64 * 1024 * 1024;

/// Result of checking every request/response in one bounded sequential history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogResult {
    /// Every observation agrees with the independently interpreted input.
    Accepted,
    /// An observation disagrees with the semantics of a supported input history.
    Rejected {
        /// Zero-based request/response position.
        pair: usize,
        /// First disagreement; no later pairs are accepted after this failure.
        reason: &'static str,
    },
    /// The input violates the protocol, fixed profile, or resource budget.
    Invalid {
        /// Zero-based request/response position, or zero for an empty history.
        pair: usize,
        /// The violated input requirement.
        reason: &'static str,
    },
}

/// Incremental preflight for the same bounded typed input used by the oracle.
/// Native readers use it before retaining a decoded pair; this is not a verdict.
pub struct CatalogInputBudget {
    remaining: usize,
    pairs: usize,
    failed: bool,
}
impl Default for CatalogInputBudget {
    fn default() -> Self {
        Self {
            remaining: INPUT_BYTES,
            pairs: 0,
            failed: false,
        }
    }
}
impl CatalogInputBudget {
    /// Charge one complete pair without cloning its payloads.
    ///
    /// # Errors
    /// Any profile/count/logical byte violation permanently closes this budget.
    pub fn admit(&mut self, request: &Request, response: &Response) -> Result<(), &'static str> {
        if self.failed {
            return Err("input budget already failed");
        }
        self.failed = true;
        if self.pairs == 1024 {
            return Err("request count");
        }
        account_pair(request, response, &mut self.remaining)?;
        self.pairs += 1;
        self.failed = false;
        Ok(())
    }
}

/// Check actor-zero sequential K0 operations, including snapshot/cursor ownership.
///
/// The fixed profile in DEV2-K0-004 bounds requests, names, input values and copied
/// state before allocation. Larger fixtures are Invalid, never silently reduced.
/// Metrics operations/rows/pins are exact; resident bytes are a bounded gauge and
/// copied references a monotonic counter. This does not qualify durability or PF.
pub fn check_catalog(pairs: &[(Request, Response)]) -> CatalogResult {
    if pairs.is_empty() || pairs.len() > 1024 {
        return CatalogResult::Invalid {
            pair: 0,
            reason: "request count",
        };
    }
    let mut budget = CatalogInputBudget::default();
    for (pair, (request, response)) in pairs.iter().enumerate() {
        if let Err(reason) = budget.admit(request, response) {
            return CatalogResult::Invalid { pair, reason };
        }
    }
    let run = pairs[0].0.run;
    let mut state = State::default();
    for (pair, (request, response)) in pairs.iter().enumerate() {
        if request.version != 1
            || run == [0; 16]
            || request.run != run
            || request.actor != 0
            || request.request != pair as u64 + 1
        {
            return CatalogResult::Invalid {
                pair,
                reason: "request envelope/order",
            };
        }
        let expected = match state.apply(&request.action) {
            Ok(value) => value,
            Err(Failure::Product(code)) => Observation::ProductError { code: code.into() },
            Err(Failure::Invalid(reason)) => return CatalogResult::Invalid { pair, reason },
        };
        if response.version != 1
            || response.run != run
            || response.actor != 0
            || response.request != request.request
        {
            return CatalogResult::Rejected {
                pair,
                reason: "response envelope",
            };
        }
        let equal = match (&expected, &response.observation) {
            (
                Observation::Metrics { counters: wanted },
                Observation::Metrics { counters: actual },
            ) => {
                let valid = wanted.operations == actual.operations
                    && wanted.rows_read == actual.rows_read
                    && wanted.snapshot_pins == actual.snapshot_pins
                    && actual.resident_bytes <= RESIDENT_BYTES
                    && actual.row_references_copied >= state.copied_observed;
                state.copied_observed = actual.row_references_copied;
                valid
            }
            _ => expected == response.observation,
        };
        if !equal {
            return CatalogResult::Rejected {
                pair,
                reason: "operation observation",
            };
        }
    }
    CatalogResult::Accepted
}

enum Failure {
    Invalid(&'static str),
    Product(&'static str),
}
type Step<T> = Result<T, Failure>;
fn invalid<T>(reason: &'static str) -> Step<T> {
    Err(Failure::Invalid(reason))
}
fn product<T>(code: &'static str) -> Step<T> {
    Err(Failure::Product(code))
}

fn take(remaining: &mut usize, bytes: usize) -> Result<(), &'static str> {
    *remaining = remaining
        .checked_sub(bytes)
        .ok_or("logical input/observation budget")?;
    Ok(())
}
fn cells_cost(cells: &[Cell], remaining: &mut usize, input: bool) -> Result<(), &'static str> {
    if input && cells.len() > 4 {
        return Err("input column profile");
    }
    for cell in cells {
        let bytes = match cell {
            Cell::Utf8(v) => v.len(),
            Cell::Binary(v) => v.len(),
            _ => 0,
        };
        if input && bytes > 1024 {
            return Err("input value profile");
        }
        take(remaining, 32)?;
        take(remaining, bytes)?;
    }
    Ok(())
}
fn schema_cost(spec: &TableSpec, remaining: &mut usize, input: bool) -> Result<(), &'static str> {
    if input
        && (spec.columns.len() > 4
            || spec.name.len() > 129
            || spec.columns.iter().any(|column| column.name.len() > 129))
    {
        return Err("input schema profile");
    }
    take(remaining, spec.name.len())?;
    for column in &spec.columns {
        take(remaining, 64)?;
        take(remaining, column.name.len())?;
    }
    Ok(())
}
fn row_cost(row: &Row, remaining: &mut usize) -> Result<(), &'static str> {
    take(remaining, 128)?;
    cells_cost(&row.values, remaining, false)
}
fn account_pair(
    request: &Request,
    response: &Response,
    remaining: &mut usize,
) -> Result<(), &'static str> {
    if matches!(request.action, Action::Performance { .. })
        || matches!(response.observation, Observation::Performance { .. })
    {
        return Err("performance outside catalog profile");
    }
    take(remaining, 512)?;
    match &request.action {
        Action::Create { spec, .. } => schema_cost(spec, remaining, true)?,
        Action::Lookup { name, .. } => {
            if name.len() > 129 {
                return Err("lookup name profile");
            }
            take(remaining, name.len())?;
        }
        Action::Insert { values, .. } | Action::Replace { values, .. } => {
            cells_cost(values, remaining, true)?
        }
        _ => (),
    }
    match &response.observation {
        Observation::Table { spec, .. } => schema_cost(spec, remaining, false)?,
        Observation::Row { value: Some(row) } => row_cost(row, remaining)?,
        Observation::Page { rows, .. } => {
            for row in rows {
                row_cost(row, remaining)?;
            }
        }
        Observation::ProductError { code } => take(remaining, code.len())?,
        _ => (),
    }
    Ok(())
}

type Slots<T> = BTreeMap<u64, Option<T>>;
fn reserve<T>(slots: &mut Slots<T>, id: u64) -> Step<()> {
    if !(1..=1024).contains(&id) || slots.contains_key(&id) {
        return invalid("invalid/reused alias");
    }
    slots.insert(id, None);
    Ok(())
}
fn get<T>(slots: &Slots<T>, id: u64) -> Step<&T> {
    slots
        .get(&id)
        .and_then(Option::as_ref)
        .ok_or(Failure::Invalid("inactive alias"))
}
fn retire<T>(slots: &mut Slots<T>, id: u64) -> Step<()> {
    get(slots, id)?;
    slots.insert(id, None);
    Ok(())
}
fn active<T>(slots: &Slots<T>) -> usize {
    slots.values().filter(|v| v.is_some()).count()
}

#[derive(Clone)]
struct Table {
    identity: TableIdentity,
    spec: TableSpec,
    rows: BTreeMap<u64, Vec<Cell>>,
}
type Database = BTreeMap<u64, Table>;
struct Snapshot {
    group: u64,
    database: Database,
}
struct Cursor {
    group: u64,
    table: Table,
    position: usize,
}
#[derive(Default)]
struct State {
    namespace: Option<([u8; 16], u64, u32)>,
    database: Database,
    tables: Slots<TableIdentity>,
    rows: Slots<RowIdentity>,
    snapshots: Slots<Snapshot>,
    cursors: Slots<Cursor>,
    object_next: u32,
    row_next: u32,
    operations: u64,
    rows_read: u64,
    copied_observed: u64,
}
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.contains('\0')
}
fn schema_valid(spec: &TableSpec) -> bool {
    if !valid_name(&spec.name) || spec.columns.is_empty() || spec.columns.len() > 32 {
        return false;
    }
    let mut names = BTreeSet::new();
    spec.columns
        .iter()
        .all(|c| valid_name(&c.name) && names.insert(&c.name))
}
fn row_valid(spec: &TableSpec, values: &[Cell]) -> bool {
    spec.columns.len() == values.len()
        && spec
            .columns
            .iter()
            .zip(values)
            .all(|(column, value)| match value {
                Cell::Null => column.nullable,
                Cell::Bool(_) => column.data_type == DataType::Bool,
                Cell::Int64(_) => column.data_type == DataType::Int64,
                Cell::Utf8(_) => column.data_type == DataType::Utf8,
                Cell::Binary(_) => column.data_type == DataType::Binary,
            })
}
fn table<'a>(database: &'a Database, identity: &TableIdentity) -> Step<&'a Table> {
    database
        .get(&identity.object)
        .filter(|t| t.identity == *identity)
        .ok_or(Failure::Product("XADB-ENGINE-0005"))
}
fn row_belongs(table: &TableIdentity, row: &RowIdentity) -> Step<()> {
    if row.table != *table {
        product("XADB-ENGINE-0005")
    } else {
        Ok(())
    }
}
fn canonical(values: &[Cell]) -> usize {
    4 + values
        .iter()
        .map(|value| {
            1 + match value {
                Cell::Null => 0,
                Cell::Bool(_) => 1,
                Cell::Int64(_) => 8,
                Cell::Utf8(text) => 4 + text.len(),
                Cell::Binary(data) => 4 + data.len(),
            }
        })
        .sum::<usize>()
}
impl State {
    fn apply(&mut self, action: &Action) -> Step<Observation> {
        if self.namespace.is_none() {
            if let Action::Initialize {
                database,
                timeline,
                epoch,
                resident_bytes,
                snapshot_pins,
            } = action
            {
                if *database == [0; 16]
                    || *timeline == 0
                    || *epoch == 0
                    || *resident_bytes != RESIDENT_BYTES
                    || *snapshot_pins != 32
                {
                    return invalid("initialization profile");
                }
                self.namespace = Some((*database, *timeline, *epoch));
                return Ok(Observation::Ready {});
            }
            return invalid("initialize first");
        }
        match action {
            Action::Performance { .. } => return invalid("performance outside catalog profile"),
            Action::Initialize { .. } => return invalid("repeated initialization"),
            Action::Create { table: alias, spec } => {
                reserve(&mut self.tables, *alias)?;
                self.operations += 1;
                if !schema_valid(spec) {
                    return product("XADB-ENGINE-0001");
                }
                if self.database.values().any(|t| t.spec.name == spec.name) {
                    return product("XADB-ENGINE-0003");
                }
                if self.database.len() >= 4 {
                    return invalid("live table profile");
                }
                let (database, timeline, epoch) = self.namespace.expect("initialized above");
                self.object_next += 1; // At most 1024 issued IDs in this profile.
                let identity = TableIdentity {
                    database,
                    timeline,
                    object: (u64::from(epoch) << 32) | u64::from(self.object_next),
                    generation: 1,
                    schema_epoch: 1,
                };
                self.database.insert(
                    identity.object,
                    Table {
                        identity: identity.clone(),
                        spec: spec.clone(),
                        rows: BTreeMap::new(),
                    },
                );
                self.tables.insert(*alias, Some(identity.clone()));
                return Ok(Observation::Table {
                    identity,
                    spec: spec.clone(),
                });
            }
            Action::Lookup {
                snapshot,
                table: alias,
                name,
            } => {
                let view = get(&self.snapshots, *snapshot)?;
                reserve(&mut self.tables, *alias)?;
                let value = view
                    .database
                    .values()
                    .find(|t| t.spec.name == *name)
                    .ok_or(Failure::Product("XADB-ENGINE-0004"))?;
                self.tables.insert(*alias, Some(value.identity.clone()));
                return Ok(Observation::Table {
                    identity: value.identity.clone(),
                    spec: value.spec.clone(),
                });
            }
            Action::DropTable { table: alias } => {
                let identity = get(&self.tables, *alias)?;
                self.operations += 1;
                table(&self.database, identity)?;
                self.database.remove(&identity.object);
            }
            Action::Insert {
                table: alias,
                row,
                values,
            } => {
                let identity = get(&self.tables, *alias)?.clone();
                reserve(&mut self.rows, *row)?;
                self.operations += 1;
                let target = table(&self.database, &identity)?;
                if !row_valid(&target.spec, values) {
                    return product("XADB-ENGINE-0002");
                }
                if self.database.values().map(|t| t.rows.len()).sum::<usize>() >= 128 {
                    return invalid("live row profile");
                }
                self.row_next += 1;
                let epoch = self.namespace.expect("initialized above").2;
                let uid = RowIdentity {
                    table: identity.clone(),
                    row: (u64::from(epoch) << 32) | u64::from(self.row_next),
                };
                self.database
                    .get_mut(&identity.object)
                    .expect("validated table")
                    .rows
                    .insert(uid.row, values.clone());
                self.rows.insert(*row, Some(uid.clone()));
                return Ok(Observation::Inserted { identity: uid });
            }
            Action::Replace {
                table: alias,
                row,
                values,
            } => {
                let identity = get(&self.tables, *alias)?;
                let uid = get(&self.rows, *row)?;
                self.operations += 1;
                let target = table(&self.database, identity)?;
                row_belongs(identity, uid)?;
                if !target.rows.contains_key(&uid.row) {
                    return product("XADB-ENGINE-0006");
                }
                if !row_valid(&target.spec, values) {
                    return product("XADB-ENGINE-0002");
                }
                self.database
                    .get_mut(&identity.object)
                    .expect("validated table")
                    .rows
                    .insert(uid.row, values.clone());
            }
            Action::Delete { table: alias, row } => {
                let identity = get(&self.tables, *alias)?;
                let uid = get(&self.rows, *row)?;
                self.operations += 1;
                table(&self.database, identity)?;
                row_belongs(identity, uid)?;
                if self
                    .database
                    .get_mut(&identity.object)
                    .expect("validated table")
                    .rows
                    .remove(&uid.row)
                    .is_none()
                {
                    return product("XADB-ENGINE-0006");
                }
            }
            Action::Snapshot { snapshot } => {
                reserve(&mut self.snapshots, *snapshot)?;
                if active(&self.snapshots) >= 8 {
                    return invalid("snapshot profile");
                }
                self.snapshots.insert(
                    *snapshot,
                    Some(Snapshot {
                        group: *snapshot,
                        database: self.database.clone(),
                    }),
                );
            }
            Action::ReleaseSnapshot { snapshot } => retire(&mut self.snapshots, *snapshot)?,
            Action::Read {
                snapshot,
                table: alias,
                row,
            } => {
                let view = get(&self.snapshots, *snapshot)?;
                let identity = get(&self.tables, *alias)?;
                let uid = get(&self.rows, *row)?;
                let target = table(&view.database, identity)?;
                row_belongs(identity, uid)?;
                let value = target.rows.get(&uid.row).map(|values| Row {
                    identity: uid.clone(),
                    values: values.clone(),
                });
                self.rows_read += u64::from(value.is_some());
                return Ok(Observation::Row { value });
            }
            Action::OpenScan {
                snapshot,
                table: alias,
                cursor,
            } => {
                let view = get(&self.snapshots, *snapshot)?;
                let identity = get(&self.tables, *alias)?;
                if active(&self.cursors) >= 8 {
                    return invalid("cursor profile");
                }
                reserve(&mut self.cursors, *cursor)?;
                let target = table(&view.database, identity)?;
                self.cursors.insert(
                    *cursor,
                    Some(Cursor {
                        group: view.group,
                        table: target.clone(),
                        position: 0,
                    }),
                );
            }
            Action::NextPage { cursor, limit } => {
                get(&self.cursors, *cursor)?;
                if !(1..=1024).contains(limit) {
                    return product("XADB-ENGINE-0007");
                }
                let cursor = self
                    .cursors
                    .get_mut(cursor)
                    .and_then(Option::as_mut)
                    .expect("validated cursor");
                let mut rows = Vec::new();
                let mut bytes = 0;
                for (&row, values) in cursor
                    .table
                    .rows
                    .iter()
                    .skip(cursor.position)
                    .take(*limit as usize)
                {
                    let cost = canonical(values) + 56;
                    if bytes + cost > 256 * 1024 {
                        break;
                    }
                    bytes += cost;
                    rows.push(Row {
                        identity: RowIdentity {
                            table: cursor.table.identity.clone(),
                            row,
                        },
                        values: values.clone(),
                    });
                }
                cursor.position += rows.len();
                self.rows_read += rows.len() as u64;
                return Ok(Observation::Page {
                    rows,
                    eof: cursor.position == cursor.table.rows.len(),
                    canonical_bytes: bytes as u32,
                });
            }
            Action::CloseScan { cursor } => retire(&mut self.cursors, *cursor)?,
            Action::Metrics {} => {
                let mut groups = BTreeSet::new();
                groups.extend(self.snapshots.values().flatten().map(|v| v.group));
                groups.extend(self.cursors.values().flatten().map(|v| v.group));
                return Ok(Observation::Metrics {
                    counters: Metrics {
                        operations: self.operations,
                        rows_read: self.rows_read,
                        snapshot_pins: groups.len() as u64,
                        resident_bytes: 0,
                        row_references_copied: 0,
                    },
                });
            }
        }
        Ok(Observation::Done {})
    }
}

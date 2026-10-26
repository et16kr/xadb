//! DEV4 P5 deterministic BFS explorer over the reference lifecycle model (D4-LM-BFS-001).
//!
//! The explorer classifies every raw slot of every expanded state in FIFO discovery order and
//! calls the model and the independent checker once per non-alias slot. It holds the whole graph
//! in memory: durable checkpoint files and resume are a later unit, so a process that ends early
//! leaves no prefix. Worker threads evaluate states as pure functions of their canonical bytes;
//! the single merge loop assigns discovery IDs and ledger order, so results do not depend on the
//! worker count.
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::path::Path;
use std::time::{Duration, Instant};
use test_reference::lifecycle::{
    AssemblyError, CheckFailure, Edge, EdgeClass, Exclusion, MAX_INPUT_BYTES, MAX_STATE_BYTES,
    ModelError, RAW_ACTION_COUNT, ResolvedInput, ReturnCode, State, WireIdentity, action_at,
    assemble_input, decode_state, encode_input, encode_state, transition, verify_edge,
};

/// Model transition hook; the reference `transition` in a normal run.
pub type ModelFn = fn(&State, &ResolvedInput, &mut State, &mut Edge) -> Result<(), ModelError>;
/// Independent checker hook; the reference `verify_edge` in a normal run.
pub type CheckFn = fn(&State, &ResolvedInput, &Edge, &State) -> Result<(), CheckFailure>;

/// The model and checker the explorer drives; a test may substitute a mutant model.
#[derive(Clone, Copy)]
pub struct Hooks {
    /// Model transition.
    pub model: ModelFn,
    /// Independent checker.
    pub check: CheckFn,
}

impl Hooks {
    /// The reference model and the independent checker.
    pub const REFERENCE: Self = Self {
        model: transition,
        check: verify_edge,
    };
}

/// Exploration profile: graph depth, the four protection caps and the worker count.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    /// States at this depth are discovered and checked but not expanded.
    pub max_depth: u8,
    /// Maximum unique states, initial included.
    pub state_cap: u32,
    /// Maximum classified raw slots.
    pub slot_cap: u64,
    /// Maximum active wall time.
    pub wall_cap: Duration,
    /// Maximum owned capacity of arena, offsets, metadata and visited index, in bytes.
    pub capacity_cap: u64,
    /// Evaluation threads; results do not depend on it.
    pub workers: usize,
}

impl Profile {
    /// The normal profile of design-proposal-v3 §9 and resource-proposal-v1 §2–§3.
    #[must_use]
    pub fn normal(workers: usize) -> Self {
        Self {
            max_depth: 6,
            state_cap: 500_000,
            slot_cap: 766_500_000,
            wall_cap: Duration::from_secs(1_800),
            capacity_cap: 2 * 1024 * 1024 * 1024,
            workers,
        }
    }

    /// The depth-5 extension the user approved on 2026-10-25: states at depth 5 are discovered
    /// and checked but not expanded; caps sized for about 11.4M states.
    #[must_use]
    pub fn depth_five(workers: usize) -> Self {
        Self {
            max_depth: 5,
            state_cap: 16_000_000,
            slot_cap: 766_500_000,
            wall_cap: Duration::from_secs(4 * 3_600),
            capacity_cap: 24 * 1024 * 1024 * 1024,
            workers,
        }
    }
}

/// Final verdict of one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The FIFO drained in memory with every slot classified and checked. This is a profile
    /// result without a durable archive, never an archived BFS PASS.
    DrainedUnarchived,
    /// A counterexample, model error or unexpected assembly error.
    Fail,
    /// A cap, the profile limit or an allocation failure stopped the run.
    Inconclusive,
}

/// Why the run stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The FIFO drained.
    Drained,
    /// The checker rejected an observed edge.
    Counterexample,
    /// The model returned an error.
    ModelError,
    /// The assembler failed with something other than its profile limit.
    AssemblyError,
    /// An assembled input was excluded as Envelope or ContextFresh: an assembler or model defect,
    /// never a legitimate exclusion of assembled input.
    EnvelopeExclusion,
    /// The assembler reached its exploration profile limit.
    ProfileLimit,
    /// The unique state cap.
    StateCap,
    /// The classified raw slot cap.
    SlotCap,
    /// The active wall cap.
    WallCap,
    /// The owned capacity cap.
    CapacityCap,
    /// A reservation failed.
    Allocation,
}

/// Classification counters of one scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Checked edges answered with Ok, Eof or IgnoredCompleted.
    pub success: u64,
    /// Checked edges answered with any other code.
    pub rejection: u64,
    /// Slots with the same resolved input bytes as an earlier slot of the same pre-state.
    pub alias: u64,
    /// Environment exclusions.
    pub excluded: u64,
    /// Completed checker calls: success + rejection + excluded.
    pub checker: u64,
}

/// Per-depth state counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DepthCounts {
    /// Unique states first discovered at this depth.
    pub unique: u64,
    /// States whose 1533 slots were all classified.
    pub expanded: u64,
    /// State invariant checks (one per first discovery).
    pub invariant: u64,
}

/// Where the run stopped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Frontier {
    /// FIFO head: the next state to expand.
    pub head: u32,
    /// FIFO tail: the state count.
    pub tail: u32,
    /// The state being expanded when the run stopped, if any.
    pub current: Option<u32>,
    /// The next unclassified raw slot of `current`.
    pub next_raw: u16,
}

/// First rejected observation, preserved with its exact inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counterexample {
    /// Pre-state ID.
    pub state: u32,
    /// Pre-state depth.
    pub depth: u8,
    /// Raw slot.
    pub raw: u16,
    /// Canonical pre-state bytes.
    pub pre: Vec<u8>,
    /// Encoded resolved input, empty when assembly failed.
    pub input: Vec<u8>,
    /// Canonical post-state bytes when the post-state encodes; empty otherwise.
    pub post: Vec<u8>,
    /// Observed edge, debug form.
    pub edge: String,
    /// Checker, model or assembler failure, debug form.
    pub failure: String,
    /// Raw slots from the initial state to the pre-state.
    pub path: Vec<u16>,
}

/// Result of one run.
#[derive(Clone, Debug)]
pub struct Report {
    /// Verdict.
    pub status: Status,
    /// Why the run stopped.
    pub stop: Stop,
    /// Global counters.
    pub totals: Counts,
    /// Counters by `Api` tag.
    pub per_api: [Counts; 21],
    /// Counters by discovery depth 0..=6.
    pub depths: [DepthCounts; 7],
    /// Exclusions by rule ID.
    pub per_rule: [u64; 14],
    /// Ledger records (classified slots).
    pub edges: u64,
    /// Unique states.
    pub states: u32,
    /// The ledger uses edge16 because the state cap exceeds the edge8 ID fields.
    pub wide_ledger: bool,
    /// SHA-256 of the edge8 (or edge16) ledger.
    pub ledger_sha256: Option<[u8; 32]>,
    /// SHA-256 of the discovery32 stream.
    pub discovery_sha256: Option<[u8; 32]>,
    /// Stop position.
    pub frontier: Frontier,
    /// Active wall time.
    pub elapsed: Duration,
    /// Peak charged capacity in bytes, including conservative reallocation overlap and the
    /// per-worker allowance; this is an accounting bound, not measured allocator usage.
    pub capacity_peak: u64,
    /// Peak resident set size of the process (`VmHWM`), when the OS reports it.
    pub process_peak_rss: Option<u64>,
    /// First counterexample.
    pub counterexample: Option<Counterexample>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn counts_json(c: &Counts) -> serde_json::Value {
    serde_json::json!({
        "success": c.success, "rejection": c.rejection, "alias": c.alias,
        "excluded": c.excluded, "checker": c.checker,
        "checked_transitions": c.success + c.rejection,
    })
}

impl Report {
    /// Receipt form of the report; the runner adds source, fixture and executable identity.
    #[must_use]
    pub fn to_json(&self, profile: &Profile) -> serde_json::Value {
        let apis: Vec<_> = (0u8..21)
            .map(|tag| {
                let api = test_reference::lifecycle::Api::try_from(tag).expect("api tag");
                serde_json::json!({"api": format!("{api:?}"), "counts": counts_json(&self.per_api[usize::from(tag)])})
            })
            .collect();
        let depths: Vec<_> = self
            .depths
            .iter()
            .map(|d| serde_json::json!({"unique": d.unique, "expanded": d.expanded, "invariant": d.invariant}))
            .collect();
        serde_json::json!({
            "profile": {
                "max_depth": profile.max_depth, "state_cap": profile.state_cap,
                "slot_cap": profile.slot_cap, "wall_cap_seconds": profile.wall_cap.as_secs(),
                "capacity_cap": profile.capacity_cap, "workers": profile.workers,
            },
            "status": format!("{:?}", self.status),
            "stop": format!("{:?}", self.stop),
            "totals": counts_json(&self.totals),
            "per_api": apis,
            "depths": depths,
            "edges": self.edges,
            "states": self.states,
            "ledger_format": if self.wide_ledger { "edge16" } else { "edge8" },
            "ledger_sha256": self.ledger_sha256.map(|h| hex(&h)),
            "discovery_sha256": self.discovery_sha256.map(|h| hex(&h)),
            "frontier": {
                "head": self.frontier.head, "tail": self.frontier.tail,
                "current": self.frontier.current, "next_raw": self.frontier.next_raw,
            },
            "elapsed_seconds": self.elapsed.as_secs_f64(),
            "capacity_peak": self.capacity_peak,
            "per_rule": self.per_rule,
            "ledger_descriptor_sha256": hex(&Sha256::digest(LEDGER_DESCRIPTOR.as_bytes())),
            "process_peak_rss_bytes": self.process_peak_rss,
            "counterexample": self.counterexample.as_ref().map(|c| serde_json::json!({
                "state": c.state, "depth": c.depth, "raw": c.raw, "pre_hex": hex(&c.pre),
                "input_hex": hex(&c.input), "post_hex": hex(&c.post), "edge": c.edge,
                "failure": c.failure, "path": c.path,
            })),
        })
    }
}

/// Frozen description of the ledger layouts and rule IDs; its SHA-256 is in every report and
/// changes whenever a layout or a rule ID changes.
pub const LEDGER_DESCRIPTOR: &str = "D4-LM-BFS-001 ledger v1; edge8: pre 0..18, raw 19..29, \
class 30..31 (Success0 Rejection1 Alias2 EnvExcluded3), detail 32..42, successor+1 43..62, \
first 63, little-endian u64; edge16: pre u32, raw u16, class u8, first u8, detail u16, \
reserved u16, successor+1 u32, little-endian; discovery32: id u32, parent u32, depth u8, \
reserved 3, raw u16, reserved 2, edge u64, wire length u32, reserved 4, then wire; rules: \
ContextFresh0 SignalNoDeadline1 BindN0 2 ExecuteCommit3 ReadOnlyCleanup4 CommitNoWrite5 \
CommitProgress6 CommitPreattemptGate7 RecoveryPre8 RecoverySet9 Envelope10 ResumeWork11 \
FencedClose12 PeerFence13";

/// Peak resident set size from `/proc/self/status`.
fn peak_rss() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// Rule IDs of the environment exclusions in the edge8 detail field.
#[must_use]
pub fn rule_id(rule: Exclusion) -> u16 {
    match rule {
        Exclusion::ContextFresh => 0,
        Exclusion::SignalNoDeadline => 1,
        Exclusion::BindN0 => 2,
        Exclusion::ExecuteCommit => 3,
        Exclusion::ReadOnlyCleanup => 4,
        Exclusion::CommitNoWrite => 5,
        Exclusion::CommitProgress => 6,
        Exclusion::CommitPreattemptGate => 7,
        Exclusion::RecoveryPre => 8,
        Exclusion::RecoverySet => 9,
        Exclusion::Envelope => 10,
        Exclusion::ResumeWork => 11,
        Exclusion::FencedClose => 12,
        Exclusion::PeerFence => 13,
    }
}

/// Descriptor identity of the independent byte vectors, used for every canonical state.
#[must_use]
pub fn vector_identity() -> WireIdentity {
    WireIdentity {
        layout: [0x00; 32],
        profile: [0x11; 32],
        action: [0x22; 32],
        exclusion: [0x33; 32],
    }
}

/// Decode the independent initial state vector.
///
/// # Errors
/// Returns the read or decode failure as text.
pub fn load_initial(path: &Path, out: &mut State) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    decode_state(&bytes, &vector_identity(), out).map_err(|e| format!("decode initial: {e:?}"))
}

const SUCCESS: u8 = 0;
const REJECTION: u8 = 1;
const ALIAS: u8 = 2;
const EXCLUDED: u8 = 3;
/// edge8 holds pre-state IDs in 19 bits and successor+1 in 20 bits.
const EDGE8_STATES: u32 = 1 << 19;

/// One classified slot of a state, before global discovery IDs are known.
#[derive(Clone, Copy)]
enum Slot {
    Checked { class: u8, code: u8, successor: u16 },
    Alias { representative: u16 },
    Excluded { rule: u16 },
    ProfileLimit,
    Fail(Stop),
}

/// Evaluation of one state: 1533 slots and its unique successors in first-seen order.
struct Evaluated {
    slots: Vec<Slot>,
    successors: Vec<Vec<u8>>,
    failure: Option<(u16, Counterexample)>,
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(bytes);
    h.finish()
}

fn success_code(code: ReturnCode) -> bool {
    matches!(
        code,
        ReturnCode::Ok | ReturnCode::Eof | ReturnCode::IgnoredCompleted
    )
}

/// Evaluate every raw slot of one state; a pure function of its canonical bytes.
fn evaluate(hooks: Hooks, id: u32, depth: u8, bytes: &[u8]) -> Evaluated {
    let identity = vector_identity();
    let mut pre = Box::<State>::default();
    let mut post = Box::<State>::default();
    let mut input = Box::<ResolvedInput>::default();
    let mut encoded = vec![0u8; MAX_INPUT_BYTES];
    let mut out = vec![0u8; MAX_STATE_BYTES];
    let mut slots = Vec::with_capacity(usize::from(RAW_ACTION_COUNT));
    let mut successors: Vec<Vec<u8>> = Vec::new();
    let mut local: HashMap<u64, Vec<u16>> = HashMap::new();
    let mut inputs: HashMap<Vec<u8>, u16> = HashMap::new();
    let fail = |raw: u16, input: Vec<u8>, edge: String, failure: String| Counterexample {
        state: id,
        depth,
        raw,
        pre: bytes.to_vec(),
        input,
        post: Vec::new(),
        edge,
        failure,
        path: Vec::new(),
    };
    let post_bytes = |post: &State| {
        let mut out = vec![0u8; MAX_STATE_BYTES];
        encode_state(post, &vector_identity(), &mut out)
            .map_or_else(|_| Vec::new(), |n| out[..n].to_vec())
    };
    if let Err(e) = decode_state(bytes, &identity, &mut pre) {
        let c = fail(0, Vec::new(), String::new(), format!("decode {e:?}"));
        return Evaluated {
            slots: vec![Slot::Fail(Stop::ModelError)],
            successors,
            failure: Some((0, c)),
        };
    }
    for raw in 0..RAW_ACTION_COUNT {
        let action = action_at(raw).expect("raw slot in range");
        match assemble_input(&pre, &action, &mut input) {
            Ok(()) => {}
            Err(AssemblyError::ProfileLimit) => {
                slots.push(Slot::ProfileLimit);
                break;
            }
            Err(e) => {
                slots.push(Slot::Fail(Stop::AssemblyError));
                let c = fail(raw, Vec::new(), String::new(), format!("assembly {e:?}"));
                return Evaluated {
                    slots,
                    successors,
                    failure: Some((raw, c)),
                };
            }
        }
        let n = match encode_input(&input, &mut encoded) {
            Ok(n) => n,
            Err(e) => {
                slots.push(Slot::Fail(Stop::AssemblyError));
                let c = fail(
                    raw,
                    Vec::new(),
                    String::new(),
                    format!("encode input {e:?}"),
                );
                return Evaluated {
                    slots,
                    successors,
                    failure: Some((raw, c)),
                };
            }
        };
        if let Some(&representative) = inputs.get(&encoded[..n]) {
            slots.push(Slot::Alias { representative });
            continue;
        }
        inputs.insert(encoded[..n].to_vec(), raw);
        let mut edge = Edge::default();
        if let Err(e) = (hooks.model)(&pre, &input, &mut post, &mut edge) {
            slots.push(Slot::Fail(Stop::ModelError));
            let c = fail(
                raw,
                encoded[..n].to_vec(),
                String::new(),
                format!("model {e:?}"),
            );
            return Evaluated {
                slots,
                successors,
                failure: Some((raw, c)),
            };
        }
        let defect = match (hooks.check)(&pre, &input, &edge, &post) {
            Err(why) => Some((Stop::Counterexample, format!("{why:?}"))),
            // Assembled input is never a legitimate Envelope or ContextFresh exclusion.
            Ok(())
                if matches!(
                    edge.class,
                    EdgeClass::Excluded(Exclusion::Envelope | Exclusion::ContextFresh)
                ) =>
            {
                Some((
                    Stop::EnvelopeExclusion,
                    "assembled input excluded".to_owned(),
                ))
            }
            Ok(()) if edge.response.is_none() && !matches!(edge.class, EdgeClass::Excluded(_)) => {
                Some((Stop::ModelError, "edge without response".to_owned()))
            }
            Ok(()) => None,
        };
        if let Some((stop, failure)) = defect {
            slots.push(Slot::Fail(stop));
            let mut c = fail(raw, encoded[..n].to_vec(), format!("{edge:?}"), failure);
            c.post = post_bytes(&post);
            return Evaluated {
                slots,
                successors,
                failure: Some((raw, c)),
            };
        }
        if let EdgeClass::Excluded(rule) = edge.class {
            slots.push(Slot::Excluded {
                rule: rule_id(rule),
            });
            continue;
        }
        let code = edge.response.map_or(ReturnCode::Ok, |r| r.code);
        let class = if success_code(code) {
            SUCCESS
        } else {
            REJECTION
        };
        // Encoding validates the post-state: the state invariant of a new discovery.
        let m = match encode_state(&post, &identity, &mut out) {
            Ok(m) => m,
            Err(e) => {
                slots.push(Slot::Fail(Stop::ModelError));
                let c = fail(
                    raw,
                    encoded[..n].to_vec(),
                    format!("{edge:?}"),
                    format!("post-state invariant {e:?}"),
                );
                return Evaluated {
                    slots,
                    successors,
                    failure: Some((raw, c)),
                };
            }
        };
        let h = hash_bytes(&out[..m]);
        let bucket = local.entry(h).or_default();
        let successor = match bucket
            .iter()
            .copied()
            .find(|&i| successors[usize::from(i)] == out[..m])
        {
            Some(i) => i,
            None => {
                let i = u16::try_from(successors.len()).expect("fewer successors than slots");
                successors.push(out[..m].to_vec());
                bucket.push(i);
                i
            }
        };
        slots.push(Slot::Checked {
            class,
            code: code as u8,
            successor,
        });
    }
    Evaluated {
        slots,
        successors,
        failure: None,
    }
}

/// The graph held in memory with exact byte equality.
struct Graph {
    /// Fixed-capacity blocks; a state never spans blocks and blocks never reallocate.
    blocks: Vec<Vec<u8>>,
    /// Per state: block index << 32 | offset, and length.
    offsets: Vec<(u64, u32)>,
    depth: Vec<u8>,
    /// Per state: parent ID and discovering raw slot (`u32::MAX`/`u16::MAX` for initial).
    parent: Vec<(u32, u16)>,
    index: Vec<(u64, u32)>,
    mask: usize,
    /// Peak charged capacity, including conservative reallocation overlap.
    peak: u64,
}

const EMPTY: u32 = u32::MAX;
const BLOCK: usize = 64 * 1024 * 1024;
/// Charged owned capacity per worker for its infallible allocations.
pub const WORKER_ALLOWANCE: u64 = 32 * 1024 * 1024;

impl Graph {
    fn new(state_cap: u32, cap: u64) -> Result<Self, Stop> {
        let slots = usize::try_from(state_cap)
            .ok()
            .and_then(|n| n.checked_mul(2))
            .and_then(usize::checked_next_power_of_two)
            .ok_or(Stop::CapacityCap)?
            .max(1024);
        let bytes = slots
            .checked_mul(std::mem::size_of::<(u64, u32)>())
            .ok_or(Stop::CapacityCap)? as u64;
        // The index is sized from the state cap, which can greatly exceed the memory cap.
        // Reject it before reserve/resize can consume memory or terminate the process.
        if bytes > cap {
            return Err(Stop::CapacityCap);
        }
        let mut index = Vec::new();
        index
            .try_reserve_exact(slots)
            .map_err(|_| Stop::Allocation)?;
        index.resize(slots, (0, EMPTY));
        Ok(Self {
            blocks: Vec::new(),
            offsets: Vec::new(),
            depth: Vec::new(),
            parent: Vec::new(),
            index,
            mask: slots - 1,
            peak: bytes,
        })
    }

    fn capacity(&self) -> u64 {
        let blocks: usize = self.blocks.iter().map(Vec::capacity).sum();
        let bytes = blocks
            + self.blocks.capacity() * std::mem::size_of::<Vec<u8>>()
            + self.offsets.capacity() * std::mem::size_of::<(u64, u32)>()
            + self.depth.capacity()
            + self.parent.capacity() * std::mem::size_of::<(u32, u16)>()
            + self.index.capacity() * std::mem::size_of::<(u64, u32)>();
        bytes as u64
    }

    fn bytes(&self, id: u32) -> &[u8] {
        let (place, len) = self.offsets[id as usize];
        let block = &self.blocks[(place >> 32) as usize];
        let start = (place & u64::from(u32::MAX)) as usize;
        &block[start..start + len as usize]
    }

    fn find(&self, bytes: &[u8]) -> (Option<u32>, usize) {
        let h = hash_bytes(bytes);
        let mut slot = (h as usize) & self.mask;
        loop {
            let (stored, id) = self.index[slot];
            if id == EMPTY {
                return (None, slot);
            }
            if stored == h && self.bytes(id) == bytes {
                return (Some(id), slot);
            }
            slot = (slot + 1) & self.mask;
        }
    }

    /// Reserve room for one more state within `cap`; growth is charged before it happens.
    fn reserve(&mut self, len: usize, cap: u64) -> Result<(), Stop> {
        let room = self
            .blocks
            .last()
            .is_some_and(|b| b.len() + len <= b.capacity());
        if !room {
            let headers = if self.blocks.len() == self.blocks.capacity() {
                // The old and new header arrays may coexist during exact growth.
                (self.blocks.len() + 1) * std::mem::size_of::<Vec<u8>>()
            } else {
                0
            };
            let charge = BLOCK.max(len) + headers;
            if self.capacity() + charge as u64 > cap {
                return Err(Stop::CapacityCap);
            }
            self.peak = self.peak.max(self.capacity() + charge as u64);
            let mut block = Vec::new();
            block
                .try_reserve_exact(BLOCK.max(len))
                .map_err(|_| Stop::Allocation)?;
            self.blocks
                .try_reserve_exact(1)
                .map_err(|_| Stop::Allocation)?;
            self.blocks.push(block);
        }
        if self.offsets.len() == self.offsets.capacity() {
            let extra = self.offsets.capacity().max(1024);
            let per = std::mem::size_of::<(u64, u32)>() + 1 + std::mem::size_of::<(u32, u16)>();
            // Growth reallocates: the old arrays and the new ones coexist during the copy.
            let charge = (self.offsets.capacity() * 2 + extra) * per;
            if self.capacity() + charge as u64 > cap {
                return Err(Stop::CapacityCap);
            }
            self.peak = self.peak.max(self.capacity() + charge as u64);
            self.offsets
                .try_reserve_exact(extra)
                .map_err(|_| Stop::Allocation)?;
            self.depth
                .try_reserve_exact(extra)
                .map_err(|_| Stop::Allocation)?;
            self.parent
                .try_reserve_exact(extra)
                .map_err(|_| Stop::Allocation)?;
        }
        if self.capacity() > cap {
            return Err(Stop::CapacityCap);
        }
        Ok(())
    }

    /// Raw slots from the initial state to `id`.
    fn path(&self, mut id: u32) -> Vec<u16> {
        let mut path = Vec::new();
        while let Some(&(parent, raw)) = self.parent.get(id as usize) {
            if parent == u32::MAX {
                break;
            }
            path.push(raw);
            id = parent;
        }
        path.reverse();
        path
    }

    fn insert(&mut self, bytes: &[u8], depth: u8, parent: (u32, u16), slot: usize) -> u32 {
        let id = u32::try_from(self.offsets.len()).expect("state count fits u32");
        let block_index = self.blocks.len() - 1;
        let block = self.blocks.last_mut().expect("reserved block");
        let start = block.len();
        block.extend_from_slice(bytes);
        let place = ((block_index as u64) << 32) | start as u64;
        self.offsets
            .push((place, u32::try_from(bytes.len()).expect("state length")));
        self.depth.push(depth);
        self.parent.push(parent);
        self.index[slot] = (hash_bytes(bytes), id);
        id
    }
}

/// checkpoint-wire-v1 §2 edge8: pre bits 0..18, raw 19..29, class 30..31, detail 32..42,
/// successor+1 43..62, first discovery 63.
#[must_use]
pub fn edge8(
    pre: u32,
    raw: u16,
    class: u8,
    detail: u16,
    successor_plus_one: u64,
    first: bool,
) -> u64 {
    u64::from(pre)
        | (u64::from(raw) << 19)
        | (u64::from(class) << 30)
        | (u64::from(detail) << 32)
        | (successor_plus_one << 43)
        | (u64::from(first) << 63)
}

/// checkpoint-wire-v1 §2 discovery32 frame; the wire bytes follow it in the stream.
#[must_use]
pub fn discovery32(
    id: u32,
    parent: u32,
    depth: u8,
    raw: u16,
    edge: u64,
    wire_length: u32,
) -> [u8; 32] {
    let mut frame = [0u8; 32];
    frame[0..4].copy_from_slice(&id.to_le_bytes());
    frame[4..8].copy_from_slice(&parent.to_le_bytes());
    frame[8] = depth;
    frame[12..14].copy_from_slice(&raw.to_le_bytes());
    frame[16..24].copy_from_slice(&edge.to_le_bytes());
    frame[24..28].copy_from_slice(&wire_length.to_le_bytes());
    frame
}

struct Ledger {
    edges: Sha256,
    discovery: Sha256,
    sequence: u64,
    wide: bool,
}

impl Ledger {
    fn edge(
        &mut self,
        pre: u32,
        raw: u16,
        class: u8,
        detail: u16,
        successor: Option<u32>,
        first: bool,
    ) {
        self.sequence += 1;
        let succ = successor.map_or(0, |s| u64::from(s) + 1);
        if self.wide {
            // edge16: pre u32, raw u16, class u8, first u8, detail u16, reserved u16,
            // successor+1 u32, all little-endian.
            let mut record = [0u8; 16];
            record[0..4].copy_from_slice(&pre.to_le_bytes());
            record[4..6].copy_from_slice(&raw.to_le_bytes());
            record[6] = class;
            record[7] = u8::from(first);
            record[8..10].copy_from_slice(&detail.to_le_bytes());
            record[12..16].copy_from_slice(&(succ as u32).to_le_bytes());
            self.edges.update(record);
            return;
        }
        self.edges
            .update(edge8(pre, raw, class, detail, succ, first).to_le_bytes());
    }

    fn discovery(&mut self, id: u32, parent: u32, depth: u8, raw: u16, edge: u64, wire: &[u8]) {
        let len = u32::try_from(wire.len()).expect("wire length");
        self.discovery
            .update(discovery32(id, parent, depth, raw, edge, len));
        self.discovery.update(wire);
    }
}

fn api_index(raw: u16) -> usize {
    action_at(raw).expect("raw slot in range").api as usize
}

fn bump(report: &mut Report, raw: u16, class: u8) {
    let api = api_index(raw);
    for counts in [&mut report.totals, &mut report.per_api[api]] {
        match class {
            SUCCESS => counts.success += 1,
            REJECTION => counts.rejection += 1,
            ALIAS => counts.alias += 1,
            _ => counts.excluded += 1,
        }
        if class != ALIAS {
            counts.checker += 1;
        }
    }
}

/// Run the exploration from `initial`.
///
/// # Panics
/// Panics when `profile.max_depth` exceeds 6, the depth of the frozen graph profile.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn explore(profile: &Profile, hooks: Hooks, initial: &State) -> Report {
    assert!(profile.max_depth <= 6, "graph depth is at most 6");
    let started = Instant::now();
    let identity = vector_identity();
    let mut report = Report {
        status: Status::Inconclusive,
        stop: Stop::Drained,
        totals: Counts::default(),
        per_api: [Counts::default(); 21],
        depths: [DepthCounts::default(); 7],
        per_rule: [0; 14],
        edges: 0,
        states: 0,
        wide_ledger: false,
        ledger_sha256: None,
        discovery_sha256: None,
        frontier: Frontier::default(),
        elapsed: Duration::ZERO,
        capacity_peak: 0,
        process_peak_rss: None,
        counterexample: None,
    };
    // Worker-side memory is not fallible; charge a fixed allowance per worker instead: stack,
    // codec buffers, three boxed states, the alias table and the successors of a batch share.
    let workers = profile.workers.max(1);
    let allowance = u64::try_from(workers)
        .ok()
        .and_then(|n| WORKER_ALLOWANCE.checked_mul(n))
        .unwrap_or(0);
    let mut ledger = Ledger {
        edges: Sha256::new(),
        discovery: Sha256::new(),
        sequence: 0,
        wide: profile.state_cap >= EDGE8_STATES,
    };
    let finish = |mut report: Report, stop: Stop, graph: Option<&Graph>, ledger: Ledger| {
        report.stop = stop;
        report.status = match stop {
            Stop::Drained => Status::DrainedUnarchived,
            Stop::Counterexample
            | Stop::ModelError
            | Stop::AssemblyError
            | Stop::EnvelopeExclusion => Status::Fail,
            _ => Status::Inconclusive,
        };
        if let Some(g) = graph {
            report.states = u32::try_from(g.offsets.len()).expect("state count");
            report.capacity_peak = report
                .capacity_peak
                .max(g.peak.max(g.capacity()) + allowance);
        }
        report.edges = ledger.sequence;
        report.wide_ledger = ledger.wide;
        report.ledger_sha256 = Some(ledger.edges.finalize().into());
        report.discovery_sha256 = Some(ledger.discovery.finalize().into());
        report.elapsed = started.elapsed();
        report.process_peak_rss = peak_rss();
        report
    };
    if profile.state_cap == 0 {
        return finish(report, Stop::StateCap, None, ledger);
    }
    if allowance == 0 || allowance > profile.capacity_cap {
        return finish(report, Stop::CapacityCap, None, ledger);
    }
    let graph_cap = profile.capacity_cap - allowance;
    report.capacity_peak = allowance;
    let mut graph = match Graph::new(profile.state_cap, graph_cap) {
        Ok(g) if g.capacity() <= graph_cap => g,
        Ok(g) => return finish(report, Stop::CapacityCap, Some(&g), ledger),
        Err(stop) => return finish(report, stop, None, ledger),
    };
    let mut wire = vec![0u8; MAX_STATE_BYTES];
    let n = match encode_state(initial, &identity, &mut wire) {
        Ok(n) => n,
        Err(e) => {
            report.counterexample = Some(Counterexample {
                state: 0,
                depth: 0,
                raw: u16::MAX,
                pre: Vec::new(),
                input: Vec::new(),
                post: Vec::new(),
                edge: String::new(),
                failure: format!("initial invariant {e:?}"),
                path: Vec::new(),
            });
            return finish(report, Stop::ModelError, Some(&graph), ledger);
        }
    };
    if let Err(stop) = graph.reserve(n, graph_cap) {
        return finish(report, stop, Some(&graph), ledger);
    }
    let (_, slot) = graph.find(&wire[..n]);
    graph.insert(&wire[..n], 0, (u32::MAX, u16::MAX), slot);
    ledger.discovery(0, u32::MAX, 0, u16::MAX, u64::MAX, &wire[..n]);
    report.depths[0].unique = 1;
    report.depths[0].invariant = 1;
    report.capacity_peak = graph.capacity() + allowance;

    let batch = workers * 4;
    let mut head: u32 = 0;
    loop {
        // Skip states at the maximum depth: their invariant was checked at discovery.
        while (head as usize) < graph.offsets.len()
            && graph.depth[head as usize] >= profile.max_depth
        {
            head += 1;
        }
        let tail = u32::try_from(graph.offsets.len()).expect("state count");
        report.frontier = Frontier {
            head,
            tail,
            current: None,
            next_raw: 0,
        };
        if head >= tail {
            return finish(report, Stop::Drained, Some(&graph), ledger);
        }
        let mut ids = Vec::with_capacity(batch);
        let mut cursor = head;
        while ids.len() < batch && cursor < tail {
            if graph.depth[cursor as usize] < profile.max_depth {
                ids.push(cursor);
            }
            cursor += 1;
        }
        let results: Vec<Evaluated> = std::thread::scope(|scope| {
            let graph = &graph;
            let chunk = ids.len().div_ceil(workers).max(1);
            let handles: Vec<_> = ids
                .chunks(chunk)
                .map(|part| {
                    std::thread::Builder::new()
                        .stack_size(1024 * 1024)
                        .spawn_scoped(scope, move || {
                            part.iter()
                                .map(|&id| {
                                    evaluate(hooks, id, graph.depth[id as usize], graph.bytes(id))
                                })
                                .collect::<Vec<_>>()
                        })
                        .expect("spawn explorer worker")
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().expect("explorer worker"))
                .collect()
        });
        for (&id, evaluated) in ids.iter().zip(results) {
            if started.elapsed() >= profile.wall_cap {
                report.frontier = Frontier {
                    head: id,
                    tail: u32::try_from(graph.offsets.len()).expect("state count"),
                    current: Some(id),
                    next_raw: 0,
                };
                return finish(report, Stop::WallCap, Some(&graph), ledger);
            }
            let depth = graph.depth[id as usize];
            let mut mapped: Vec<Option<u32>> = vec![None; evaluated.successors.len()];
            let mut classes: Vec<(u8, u16, Option<u32>)> =
                Vec::with_capacity(evaluated.slots.len());
            for (raw, slot) in (0u16..).zip(evaluated.slots.iter()) {
                let stop_here = |report: &mut Report, graph: &Graph| {
                    report.frontier = Frontier {
                        head: id,
                        tail: u32::try_from(graph.offsets.len()).expect("state count"),
                        current: Some(id),
                        next_raw: raw,
                    };
                };
                if ledger.sequence >= profile.slot_cap {
                    stop_here(&mut report, &graph);
                    return finish(report, Stop::SlotCap, Some(&graph), ledger);
                }
                match *slot {
                    Slot::Fail(stop) => {
                        stop_here(&mut report, &graph);
                        report.counterexample = evaluated.failure.map(|(_, mut c)| {
                            c.path = graph.path(id);
                            c
                        });
                        return finish(report, stop, Some(&graph), ledger);
                    }
                    Slot::ProfileLimit => {
                        stop_here(&mut report, &graph);
                        return finish(report, Stop::ProfileLimit, Some(&graph), ledger);
                    }
                    Slot::Alias { representative } => {
                        let (class, _, successor) = classes[usize::from(representative)];
                        ledger.edge(id, raw, ALIAS, representative, successor, false);
                        bump(&mut report, raw, ALIAS);
                        classes.push((class, representative, successor));
                    }
                    Slot::Excluded { rule } => {
                        report.per_rule[usize::from(rule)] += 1;
                        ledger.edge(id, raw, EXCLUDED, rule, None, false);
                        bump(&mut report, raw, EXCLUDED);
                        classes.push((EXCLUDED, rule, None));
                    }
                    Slot::Checked {
                        class,
                        code,
                        successor,
                    } => {
                        let local = usize::from(successor);
                        let (target, first) = match mapped[local] {
                            Some(target) => (target, false),
                            None => {
                                let bytes = &evaluated.successors[local];
                                let (found, index_slot) = graph.find(bytes);
                                match found {
                                    Some(target) => {
                                        mapped[local] = Some(target);
                                        (target, false)
                                    }
                                    None => {
                                        if graph.offsets.len() >= profile.state_cap as usize {
                                            stop_here(&mut report, &graph);
                                            return finish(
                                                report,
                                                Stop::StateCap,
                                                Some(&graph),
                                                ledger,
                                            );
                                        }
                                        if let Err(stop) = graph.reserve(bytes.len(), graph_cap) {
                                            stop_here(&mut report, &graph);
                                            return finish(report, stop, Some(&graph), ledger);
                                        }
                                        let target =
                                            graph.insert(bytes, depth + 1, (id, raw), index_slot);
                                        report.capacity_peak =
                                            report.capacity_peak.max(graph.capacity() + allowance);
                                        let next = usize::from(depth) + 1;
                                        report.depths[next].unique += 1;
                                        report.depths[next].invariant += 1;
                                        ledger.discovery(
                                            target,
                                            id,
                                            depth + 1,
                                            raw,
                                            ledger.sequence,
                                            bytes,
                                        );
                                        mapped[local] = Some(target);
                                        (target, true)
                                    }
                                }
                            }
                        };
                        ledger.edge(id, raw, class, u16::from(code), Some(target), first);
                        bump(&mut report, raw, class);
                        classes.push((class, u16::from(code), Some(target)));
                    }
                }
            }
            report.depths[usize::from(depth)].expanded += 1;
        }
        head = cursor;
    }
}

#[cfg(test)]
mod tests;

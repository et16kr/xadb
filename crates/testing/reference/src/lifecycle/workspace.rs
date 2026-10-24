//! Fallible ownership of one bounded native decode workspace.
use super::schema::State;
use std::mem::size_of;

/// Maximum owned native bytes for one state workspace in the adopted profile.
pub const MAX_WORKSPACE_BYTES: usize = 65_536;
/// Maximum canonical state bytes, including header and checksum.
pub const MAX_STATE_BYTES: usize = 11_574;
/// Conservative maximum resolved input bytes.
pub const MAX_INPUT_BYTES: usize = 4_144;

/// Failure to obtain bounded native storage; it never constitutes an explored transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    /// Requested or actual allocation exceeds the caller's budget or profile maximum.
    Capacity,
    /// The allocator could not provide the requested native storage.
    Allocation,
}
/// One heap-resident decoded state, without per-record allocations.
#[derive(Debug)]
pub struct Workspace {
    states: Vec<State>,
}
impl Workspace {
    /// Reserve one state using the profile's fixed workspace budget.
    pub fn try_new() -> Result<Self, WorkspaceError> {
        Self::try_new_with_budget(MAX_WORKSPACE_BYTES)
    }
    /// Reserve one state within a possibly tighter caller-owned budget.
    pub fn try_new_with_budget(max_bytes: usize) -> Result<Self, WorkspaceError> {
        let limit = max_bytes.min(MAX_WORKSPACE_BYTES);
        if size_of::<State>() > limit {
            return Err(WorkspaceError::Capacity);
        }
        let mut states = Vec::new();
        states
            .try_reserve_exact(1)
            .map_err(|_| WorkspaceError::Allocation)?;
        let owned = states
            .capacity()
            .checked_mul(size_of::<State>())
            .ok_or(WorkspaceError::Capacity)?;
        if owned > limit {
            return Err(WorkspaceError::Capacity);
        }
        states.push(State::default());
        Ok(Self { states })
    }
    /// Actual owned heap capacity in bytes, including unused reserved state slots.
    pub fn owned_bytes(&self) -> usize {
        self.states.capacity() * size_of::<State>()
    }
    /// Borrow the decoded state; a fresh workspace remains unvalidated.
    pub fn state(&self) -> &State {
        &self.states[0]
    }
    /// Mutate the decode/transition workspace without obtaining another allocation.
    pub fn state_mut(&mut self) -> &mut State {
        &mut self.states[0]
    }
}

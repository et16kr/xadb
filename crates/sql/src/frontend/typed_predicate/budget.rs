//! Exact typed backings with Compile diagnostics and whole-call fault ordinals.
//!
//! Parser backings have a different owner/module, and scalar backings retain
//! their own leases. This narrow owner has no growth or generic public API.

use super::{Code, Diagnostic, Module, diagnostic, point};
use xadb_foundation::runtime::{Quota, Reservation};

pub(super) struct TypedBudget {
    session: Quota,
    database: Quota,
    #[cfg(feature = "test-support")]
    next_failure: Option<usize>,
    #[cfg(feature = "test-support")]
    active_failure: Option<usize>,
    #[cfg(feature = "test-support")]
    allocations: usize,
    #[cfg(feature = "test-support")]
    padding: usize,
}

impl TypedBudget {
    pub(super) fn new(session: &Quota, database: &Quota) -> Self {
        Self {
            session: session.clone(),
            database: database.clone(),
            #[cfg(feature = "test-support")]
            next_failure: None,
            #[cfg(feature = "test-support")]
            active_failure: None,
            #[cfg(feature = "test-support")]
            allocations: 0,
            #[cfg(feature = "test-support")]
            padding: 0,
        }
    }

    pub(super) fn begin_call(&mut self) {
        #[cfg(feature = "test-support")]
        {
            self.active_failure = self.next_failure.take();
            self.allocations = 0;
        }
    }

    #[cfg(feature = "test-support")]
    pub(super) fn fail_allocation_at(&mut self, ordinal: usize) {
        self.next_failure = (ordinal != 0).then_some(ordinal);
    }

    #[cfg(feature = "test-support")]
    pub(super) fn set_allocation_padding(&mut self, elements: usize) {
        self.padding = elements;
    }

    #[cfg(feature = "test-support")]
    pub(super) fn allocation_count(&self) -> usize {
        self.allocations
    }

    #[cfg(feature = "test-support")]
    pub(super) fn remaining_failure(&self) -> usize {
        self.active_failure
            .unwrap_or(0)
            .saturating_sub(self.allocations)
    }

    #[cfg(feature = "test-support")]
    pub(super) fn include_scalar_allocations(&mut self, count: usize) -> Result<(), Diagnostic> {
        self.allocations = self.allocations.checked_add(count).ok_or_else(limit)?;
        Ok(())
    }
}

pub(super) struct ChargedVec<T> {
    // Free all elements/backing before returning any base or excess leases.
    pub(super) values: Vec<T>,
    _session: Option<Reservation>,
    _database: Option<Reservation>,
    _extra_session: Option<Reservation>,
    _extra_database: Option<Reservation>,
}

impl<T> ChargedVec<T> {
    pub(super) fn allocate(budget: &mut TypedBudget, requested: usize) -> Result<Self, Diagnostic> {
        let mut output = Self {
            values: Vec::new(),
            _session: None,
            _database: None,
            _extra_session: None,
            _extra_database: None,
        };
        if requested == 0 {
            return Ok(output);
        }
        let bytes = byte_count::<T>(requested)?;
        output._session = Some(budget.session.reserve(bytes).map_err(|_| limit())?);
        output._database = Some(budget.database.reserve(bytes).map_err(|_| limit())?);
        #[cfg(feature = "test-support")]
        {
            budget.allocations = budget.allocations.checked_add(1).ok_or_else(limit)?;
            if budget.active_failure == Some(budget.allocations) {
                return Err(limit());
            }
        }
        #[cfg(feature = "test-support")]
        let allocation = requested.checked_add(budget.padding).ok_or_else(limit)?;
        #[cfg(not(feature = "test-support"))]
        let allocation = requested;
        byte_count::<T>(allocation)?;
        output
            .values
            .try_reserve_exact(allocation)
            .map_err(|_| limit())?;
        byte_count::<T>(output.values.capacity())?;
        let extra = output
            .values
            .capacity()
            .checked_sub(requested)
            .ok_or_else(limit)?;
        if extra != 0 {
            let bytes = byte_count::<T>(extra)?;
            output._extra_session = Some(budget.session.reserve(bytes).map_err(|_| limit())?);
            output._extra_database = Some(budget.database.reserve(bytes).map_err(|_| limit())?);
        }
        Ok(output)
    }

    #[cfg(feature = "test-support")]
    pub(super) fn capacity_bytes(&self) -> usize {
        self.values.capacity() * std::mem::size_of::<T>()
    }
}

fn byte_count<T>(elements: usize) -> Result<u64, Diagnostic> {
    let size = std::mem::size_of::<T>();
    if size == 0 {
        return Err(limit());
    }
    elements
        .checked_mul(size)
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(limit)
}

fn limit() -> Diagnostic {
    diagnostic(Code::Limit, Module::Compile, point(0))
}

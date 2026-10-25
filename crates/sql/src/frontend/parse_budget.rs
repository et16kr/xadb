//! Parser-owned backing allocation; source and typed nodes share caller ledgers.

use super::{Code, Diagnostic, Module, Span};
use foundation::runtime::{Quota, Reservation};

pub(crate) struct ParseBudget {
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

impl ParseBudget {
    pub(crate) fn new(session: &Quota, database: &Quota) -> Self {
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

    pub(crate) fn begin_call(&mut self) {
        #[cfg(feature = "test-support")]
        {
            self.active_failure = self.next_failure.take();
            self.allocations = 0;
        }
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn fail_allocation_at(&mut self, ordinal: usize) {
        self.next_failure = (ordinal != 0).then_some(ordinal);
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn set_allocation_padding(&mut self, elements: usize) {
        self.padding = elements;
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn allocation_count(&self) -> usize {
        self.allocations
    }
}

// Declaration order keeps the actual allocation alive until just before all its
// leases are returned, including failed construction and partially moved arenas.
// The only consumers are u8 source bytes and fixed, non-recursively owned AstNode.
pub(crate) struct ChargedVec<T> {
    pub(crate) values: Vec<T>,
    _session: Option<Reservation>,
    _database: Option<Reservation>,
    _extra_session: Option<Reservation>,
    _extra_database: Option<Reservation>,
}

impl<T> ChargedVec<T> {
    pub(crate) fn new() -> Self {
        Self {
            values: Vec::new(),
            _session: None,
            _database: None,
            _extra_session: None,
            _extra_database: None,
        }
    }

    pub(crate) fn allocate(
        budget: &mut ParseBudget,
        requested: usize,
        at: Span,
    ) -> Result<Self, Diagnostic> {
        if requested == 0 {
            return Ok(Self::new());
        }
        let units = byte_count::<T>(requested, at)?;
        let session = budget.session.reserve(units).map_err(|_| limit(at))?;
        let database = budget.database.reserve(units).map_err(|_| limit(at))?;
        let mut next = Self {
            values: Vec::new(),
            _session: Some(session),
            _database: Some(database),
            _extra_session: None,
            _extra_database: None,
        };
        #[cfg(feature = "test-support")]
        {
            budget.allocations = budget.allocations.checked_add(1).ok_or_else(|| limit(at))?;
            if budget.active_failure == Some(budget.allocations) {
                return Err(limit(at));
            }
        }
        #[cfg(feature = "test-support")]
        let allocation = requested
            .checked_add(budget.padding)
            .ok_or_else(|| limit(at))?;
        #[cfg(not(feature = "test-support"))]
        let allocation = requested;
        byte_count::<T>(allocation, at)?;
        next.values
            .try_reserve_exact(allocation)
            .map_err(|_| limit(at))?;
        byte_count::<T>(next.values.capacity(), at)?;
        let excess = next
            .values
            .capacity()
            .checked_sub(requested)
            .ok_or_else(|| limit(at))?;
        if excess != 0 {
            let extra = byte_count::<T>(excess, at)?;
            next._extra_session = Some(budget.session.reserve(extra).map_err(|_| limit(at))?);
            next._extra_database = Some(budget.database.reserve(extra).map_err(|_| limit(at))?);
        }
        Ok(next)
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn capacity_bytes(&self) -> usize {
        // allocate checked the actual excess as well as the requested base.
        self.values.capacity() * std::mem::size_of::<T>()
    }
}

fn byte_count<T>(elements: usize, at: Span) -> Result<u64, Diagnostic> {
    let size = std::mem::size_of::<T>();
    if size == 0 {
        return Err(limit(at));
    }
    elements
        .checked_mul(size)
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| limit(at))
}

pub(crate) fn point(position: usize) -> Span {
    Span {
        start: position,
        end: position,
    }
}

pub(crate) fn limit(span: Span) -> Diagnostic {
    Diagnostic {
        code: Code::Limit,
        module: Module::Parse,
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::ast::AstNode;

    #[test]
    fn typed_capacity_arithmetic_rejects_overflow() {
        let at = point(17);
        assert_eq!(byte_count::<AstNode>(usize::MAX, at), Err(limit(at)));
        assert_eq!(byte_count::<AstNode>(0, at), Ok(0));
        let session = Quota::new(8 << 20);
        let database = Quota::new(64 << 20);
        let mut budget = ParseBudget::new(&session, &database);
        assert!(
            matches!(ChargedVec::<AstNode>::allocate(&mut budget, usize::MAX, at), Err(e) if e == limit(at))
        );
        assert_eq!(session.peak(), 0);
        assert_eq!(database.peak(), 0);
        #[cfg(feature = "test-support")]
        {
            budget.set_allocation_padding(usize::MAX);
            assert!(
                matches!(ChargedVec::<AstNode>::allocate(&mut budget, 16, at), Err(e) if e == limit(at))
            );
            assert_eq!(budget.allocation_count(), 1);
        }
        assert_eq!(session.used(), 0);
        assert_eq!(database.used(), 0);
    }
}

//! Owned source backing and shared quota leases. No engine or call state is kept.

use super::{CallControl, Code, Diagnostic, Module, Span};
use foundation::runtime::{Quota, Reservation};

const MAX_SOURCE_BYTES: usize = 1 << 20;

pub(crate) struct SourceBudget {
    session: Quota,
    database: Quota,
    #[cfg(feature = "test-support")]
    fail_allocation: bool,
    #[cfg(feature = "test-support")]
    padding: usize,
}

impl SourceBudget {
    pub(crate) fn new(session: &Quota, database: &Quota) -> Self {
        Self {
            session: session.clone(),
            database: database.clone(),
            #[cfg(feature = "test-support")]
            fail_allocation: false,
            #[cfg(feature = "test-support")]
            padding: 0,
        }
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn fail_next_allocation(&mut self) {
        self.fail_allocation = true;
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn set_allocation_padding(&mut self, bytes: usize) {
        self.padding = bytes;
    }
}

// Rust drops fields in declaration order. Keep bytes before *all* leases, also
// during construction: allocation/extra reservation/control may fail at any step.
pub(crate) struct SourceBuffer {
    bytes: Vec<u8>,
    _session: Option<Reservation>,
    _database: Option<Reservation>,
    _extra_session: Option<Reservation>,
    _extra_database: Option<Reservation>,
}

impl SourceBuffer {
    pub(crate) fn new() -> Self {
        Self {
            bytes: Vec::new(),
            _session: None,
            _database: None,
            _extra_session: None,
            _extra_database: None,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn capacity(&self) -> usize {
        self.bytes.capacity()
    }

    pub(crate) fn push(
        &mut self,
        byte: u8,
        budget: &mut SourceBudget,
        control: &CallControl<'_>,
        segment: Span,
    ) -> Result<(), Diagnostic> {
        if self.bytes.len() == self.bytes.capacity() {
            self.grow(budget, control, segment)?;
        }
        // grow guarantees spare capacity; this append cannot allocate.
        self.bytes.push(byte);
        Ok(())
    }

    fn grow(
        &mut self,
        budget: &mut SourceBudget,
        control: &CallControl<'_>,
        segment: Span,
    ) -> Result<(), Diagnostic> {
        let at = point(segment.end);
        control.check(Module::Split, at)?;
        let requested = if self.capacity() == 0 {
            256
        } else {
            self.capacity()
                .checked_mul(2)
                .ok_or_else(|| limit(at))?
                .min(MAX_SOURCE_BYTES)
        };
        if requested <= self.len() {
            return Err(limit(at));
        }
        let mut next = Self::allocate(budget, requested, at)?;
        control.check(Module::Split, at)?;
        control.check(Module::Split, point(segment.start))?;
        for chunk in self.bytes.chunks(256) {
            next.bytes.extend_from_slice(chunk);
            let copied = segment
                .start
                .checked_add(next.len())
                .ok_or_else(|| limit(at))?;
            control.check(Module::Split, point(copied))?;
        }
        control.check(Module::Split, at)?;
        // Both complete capacities remain charged until the old owner drops.
        *self = next;
        Ok(())
    }

    fn allocate(budget: &mut SourceBudget, requested: usize, at: Span) -> Result<Self, Diagnostic> {
        let units = u64::try_from(requested).map_err(|_| limit(at))?;
        let session = budget.session.reserve(units).map_err(|_| limit(at))?;
        let database = budget.database.reserve(units).map_err(|_| limit(at))?;
        let mut next = Self {
            bytes: Vec::new(),
            _session: Some(session),
            _database: Some(database),
            _extra_session: None,
            _extra_database: None,
        };
        #[cfg(feature = "test-support")]
        if std::mem::take(&mut budget.fail_allocation) {
            return Err(limit(at));
        }
        #[cfg(feature = "test-support")]
        let allocation = requested
            .checked_add(budget.padding)
            .ok_or_else(|| limit(at))?;
        #[cfg(not(feature = "test-support"))]
        let allocation = requested;
        next.bytes
            .try_reserve_exact(allocation)
            .map_err(|_| limit(at))?;
        let extra = next
            .capacity()
            .checked_sub(requested)
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| limit(at))?;
        if extra != 0 {
            next._extra_session = Some(budget.session.reserve(extra).map_err(|_| limit(at))?);
            next._extra_database = Some(budget.database.reserve(extra).map_err(|_| limit(at))?);
        }
        Ok(next)
    }
}

/// One source segment whose backing remains charged until this owner is dropped.
/// Bytes include leading/trailing trivia and an external terminating semicolon.
/// This is a boundary result, not a parsed or executable statement.
pub struct StatementSource {
    buffer: SourceBuffer,
    span: Span,
    terminated: bool,
}

impl StatementSource {
    pub(crate) fn new(buffer: SourceBuffer, span: Span, terminated: bool) -> Self {
        Self {
            buffer,
            span,
            terminated,
        }
    }

    /// Borrow the exact retained bytes without detaching their reservations.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer.bytes
    }

    /// Original absolute batch byte range. A lexer over these bytes uses local
    /// spans; its caller must checked-add this range's start when displaying them.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Whether an external semicolon (rather than explicit EOF) ended the source.
    pub fn terminated(&self) -> bool {
        self.terminated
    }

    /// Observe actual allocated capacity in non-shipping resource tests.
    #[cfg(feature = "test-support")]
    pub fn capacity_bytes(&self) -> usize {
        self.buffer.capacity()
    }
}

impl std::fmt::Debug for StatementSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatementSource")
            .field("span", &self.span)
            .field("terminated", &self.terminated)
            .field("length", &self.buffer.len())
            .finish()
    }
}

fn point(position: usize) -> Span {
    Span {
        start: position,
        end: position,
    }
}

fn limit(span: Span) -> Diagnostic {
    Diagnostic {
        code: Code::Limit,
        module: Module::Split,
        span,
    }
}

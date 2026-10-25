//! Scalar-only String/Vec admission and rollback of shared quota reservations.

use super::{Diagnostic, OwnedScalar, ScalarData, Span, diagnostic};
use crate::frontend::Code;
use foundation::runtime::{Quota, Reservation};

pub(super) struct ScalarBudget {
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

pub(super) enum Backing {
    Text,
    Binary,
}

pub(super) struct Leases {
    _session: Option<Reservation>,
    _database: Option<Reservation>,
    _extra_session: Option<Reservation>,
    _extra_database: Option<Reservation>,
}

impl Leases {
    pub(super) fn new() -> Self {
        Self {
            _session: None,
            _database: None,
            _extra_session: None,
            _extra_database: None,
        }
    }
}

impl ScalarBudget {
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
    pub(super) fn set_allocation_padding(&mut self, bytes: usize) {
        self.padding = bytes;
    }

    #[cfg(feature = "test-support")]
    pub(super) fn allocation_count(&self) -> usize {
        self.allocations
    }

    pub(super) fn allocate(
        &mut self,
        backing: Backing,
        requested: usize,
        at: Span,
    ) -> Result<OwnedScalar, Diagnostic> {
        let data = match backing {
            Backing::Text => ScalarData::Utf8(String::new()),
            Backing::Binary => ScalarData::Binary(Vec::new()),
        };
        let mut output = OwnedScalar::new(data);
        if requested == 0 {
            return Ok(output);
        }
        let limit = diagnostic(Code::Limit, at);
        let units = u64::try_from(requested).map_err(|_| limit)?;
        output.leases._session = Some(self.session.reserve(units).map_err(|_| limit)?);
        output.leases._database = Some(self.database.reserve(units).map_err(|_| limit)?);

        // The owner already keeps data before every lease, including failures
        // after an allocation or only the first extra-capacity reservation.
        #[cfg(feature = "test-support")]
        {
            self.allocations = self.allocations.checked_add(1).ok_or(limit)?;
            if self.active_failure == Some(self.allocations) {
                return Err(limit);
            }
        }
        #[cfg(feature = "test-support")]
        let allocation = requested.checked_add(self.padding).ok_or(limit)?;
        #[cfg(not(feature = "test-support"))]
        let allocation = requested;
        u64::try_from(allocation).map_err(|_| limit)?;
        match &mut output.data {
            ScalarData::Utf8(text) => text.try_reserve_exact(allocation).map_err(|_| limit)?,
            ScalarData::Binary(bytes) => bytes.try_reserve_exact(allocation).map_err(|_| limit)?,
            _ => unreachable!("only byte backings are allocated"),
        }
        let capacity = output.capacity();
        u64::try_from(capacity).map_err(|_| limit)?;
        let extra = capacity.checked_sub(requested).ok_or(limit)?;
        if extra != 0 {
            let extra = u64::try_from(extra).map_err(|_| limit)?;
            output.leases._extra_session = Some(self.session.reserve(extra).map_err(|_| limit)?);
            output.leases._extra_database = Some(self.database.reserve(extra).map_err(|_| limit)?);
        }
        Ok(output)
    }
}

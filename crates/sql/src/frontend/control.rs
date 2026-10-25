//! Per-call borrowed cancellation and monotonic deadline inputs.

use super::diagnostic::{Code, Diagnostic, Module, Span};
use foundation::runtime::{Cancellation, Clock};

/// Controls borrowed for one input-processing call; no retained operation state.
/// The supplied clock must be nonblocking and allocation-free. Cancellation and
/// time observation are cooperative, not an atomic exclusion of future signals.
pub struct CallControl<'a> {
    cancellation: &'a Cancellation,
    clock: &'a dyn Clock,
    deadline: Option<u64>,
    #[cfg(feature = "test-support")]
    observer: Option<&'a dyn Fn(Module, Span)>,
}

impl<'a> CallControl<'a> {
    /// Borrow controls from this call. The deadline uses this clock's epoch;
    /// equality is expired. `None` disables deadline checks, not cancellation.
    pub fn new(
        cancellation: &'a Cancellation,
        clock: &'a dyn Clock,
        deadline: Option<u64>,
    ) -> Self {
        Self {
            cancellation,
            clock,
            deadline,
            #[cfg(feature = "test-support")]
            observer: None,
        }
    }

    /// Attach a borrowed test observer invoked before each control check.
    /// It receives actual processing progress and may drive cancellation or a
    /// manual clock. Scalar copy and predicate byte comparisons report local
    /// input progress here, while their returned diagnostics retain the
    /// caller's original span or the original comparison span.
    /// The observer must return promptly; it is not a product verdict.
    #[cfg(feature = "test-support")]
    pub fn with_observer(mut self, observer: &'a dyn Fn(Module, Span)) -> Self {
        self.observer = Some(observer);
        self
    }

    pub(crate) fn check(&self, module: Module, span: Span) -> Result<(), Diagnostic> {
        #[cfg(feature = "test-support")]
        if let Some(observer) = self.observer {
            observer(module, span);
        }
        let code = if self.cancellation.check().is_err() {
            Some(Code::Cancelled)
        } else if self
            .deadline
            .is_some_and(|end| self.clock.now_nanos() >= end)
        {
            Some(Code::Timeout)
        } else {
            None
        };
        match code {
            Some(code) => Err(Diagnostic { code, module, span }),
            None => Ok(()),
        }
    }
}

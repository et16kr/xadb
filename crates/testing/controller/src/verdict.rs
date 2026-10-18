use serde::{Deserialize, Serialize};

macro_rules! axis {
    ($name:ident { $($variant:ident),+ }) => {
        #[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub enum $name { $($variant),+ }
    };
}

axis!(CaseValidity {
    Valid,
    Invalid,
    Unknown
});
axis!(ProductVerdict {
    Pass,
    Fail,
    Unknown,
    NotEvaluated
});
axis!(ExecutionState {
    Prepared,
    Running,
    Completed,
    Error,
    Timeout
});
axis!(CleanupState {
    Pass,
    Fail,
    Unknown
});
axis!(CaseVerdict {
    Pass,
    Fail,
    Invalid,
    Error,
    Timeout
});

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub result_schema_version: u32,
    pub run_id: String,
    pub case_validity: CaseValidity,
    /// In DEV0 this describes the fixture contract, not a DB product qualification.
    pub product_verdict: ProductVerdict,
    pub execution_state: ExecutionState,
    pub cleanup_state: CleanupState,
    pub case_verdict: CaseVerdict,
    pub assertions_checked: usize,
    pub required_steps_complete: bool,
    pub reasons: Vec<String>,
}

impl Verdict {
    pub(crate) fn prepared(run_id: String) -> Self {
        Self {
            result_schema_version: 1,
            run_id,
            case_validity: CaseValidity::Unknown,
            product_verdict: ProductVerdict::NotEvaluated,
            execution_state: ExecutionState::Prepared,
            cleanup_state: CleanupState::Unknown,
            case_verdict: CaseVerdict::Error,
            assertions_checked: 0,
            required_steps_complete: false,
            reasons: vec![],
        }
    }

    pub(crate) fn aggregate(&mut self) {
        self.case_verdict = if self.product_verdict == ProductVerdict::Fail {
            CaseVerdict::Fail
        } else if self.execution_state == ExecutionState::Timeout {
            CaseVerdict::Timeout
        } else if self.case_validity == CaseValidity::Invalid {
            CaseVerdict::Invalid
        } else if self.case_validity == CaseValidity::Valid
            && self.execution_state == ExecutionState::Completed
            && self.product_verdict == ProductVerdict::Pass
            && self.cleanup_state == CleanupState::Pass
            && self.required_steps_complete
            && self.assertions_checked > 0
        {
            CaseVerdict::Pass
        } else {
            CaseVerdict::Error
        };
    }

    pub fn exit_code(&self) -> u8 {
        match self.case_verdict {
            CaseVerdict::Pass => 0,
            CaseVerdict::Fail => 1,
            CaseVerdict::Invalid => 2,
            CaseVerdict::Error => 3,
            CaseVerdict::Timeout => 4,
        }
    }
}

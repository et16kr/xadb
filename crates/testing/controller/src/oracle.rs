//! Exact, typed controller-side assertions. Never computes expected values with the worker.
use crate::{
    spec::{Scenario, WITNESSES},
    verdict::*,
};
use std::collections::BTreeSet;
use test_protocol::{Observation, PROTOCOL_VERSION};

pub(crate) fn evaluate(
    case: &Scenario,
    run_id: &str,
    case_sha256: &str,
    bytes: &[u8],
    verdict: &mut Verdict,
) {
    let mut seen = BTreeSet::new();
    let mut complete = true;
    let mut count = 0;
    let lines = bytes.split_inclusive(|b| *b == b'\n');
    for line in lines {
        let observation = match serde_json::from_slice::<Observation>(line) {
            Ok(value) if line.ends_with(b"\n") => value,
            _ => {
                invalid(
                    verdict,
                    "XADBTEST-PROTOCOL: malformed or incomplete observation",
                );
                complete = false;
                break;
            }
        };
        let Some(step) = case.steps.get(count) else {
            invalid(verdict, "XADBTEST-PROTOCOL: extra observation");
            complete = false;
            break;
        };
        if observation.protocol_version != PROTOCOL_VERSION
            || observation.run_id != run_id
            || observation.case_sha256 != case_sha256
            || observation.sequence != count
            || observation.step_id != step.id
            || observation
                .witnesses
                .iter()
                .any(|w| !WITNESSES.contains(&w.as_str()))
        {
            invalid(
                verdict,
                "XADBTEST-PROTOCOL: identity, sequence, version or witness mismatch",
            );
            complete = false;
            break;
        }
        let Some(assertion) = case.oracle.assertions.iter().find(|a| a.step_id == step.id) else {
            invalid(verdict, "XADBTEST-ORACLE: assertion missing");
            complete = false;
            break;
        };
        verdict.assertions_checked += 1;
        if observation.value != assertion.expected {
            verdict.product_verdict = ProductVerdict::Fail;
            verdict
                .reasons
                .push(format!("XADBTEST-VALUE-MISMATCH: {}", step.id));
        }
        seen.extend(observation.witnesses);
        count += 1;
    }
    if count != case.steps.len() {
        invalid(
            verdict,
            "XADBTEST-INCOMPLETE: not every required step was observed",
        );
        complete = false;
    }
    if case.required_witnesses.iter().any(|w| !seen.contains(w)) {
        invalid(
            verdict,
            "XADBTEST-MISSING-WITNESS: required path not observed",
        );
        complete = false;
    }
    verdict.required_steps_complete = complete;
    if verdict.product_verdict != ProductVerdict::Fail {
        verdict.product_verdict = if complete {
            ProductVerdict::Pass
        } else {
            ProductVerdict::Unknown
        };
    }
}

fn invalid(verdict: &mut Verdict, reason: &str) {
    verdict.case_validity = CaseValidity::Invalid;
    verdict.reasons.push(reason.into());
}

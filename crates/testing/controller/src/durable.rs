//! Durable worker transport and real actor rendezvous driver. Raw Invoke/Observe
//! events are flushed by native transport; these helpers neither import the
//! production engine nor declare an oracle/durability verdict.
use crate::native::{Error, ErrorKind, NativeProtocol, NativeSession};
use test_protocol::durable as wire;

/// Separate wire specialization; K0's `native::Session` remains unchanged.
pub struct DurableProtocol;
impl NativeProtocol for DurableProtocol {
    type Action = wire::Command;
    type Request = wire::Request;
    type Response = wire::Response;
    const NAME: &'static str = "durable-v1";
    fn request(run: [u8; 16], request: u64, actor: u8, command: wire::Command) -> wire::Request {
        wire::Request {
            version: wire::VERSION,
            run,
            request,
            actor,
            command,
        }
    }
    fn identity(response: &wire::Response) -> ([u8; 16], u64, u8) {
        (response.run, response.request, response.actor)
    }
}
/// Owned durable process session. Product failures remain typed observations.
pub type Session = NativeSession<DurableProtocol>;

fn protocol(detail: &str) -> Error {
    Error {
        kind: ErrorKind::Protocol,
        detail: detail.into(),
        cleanup: None,
    }
}

/// Issue one command after all previous responses have been consumed.
///
/// # Errors
/// Transport failures retain journal evidence. Unexpected response identity is a
/// controller protocol failure, not a product rejection or an inferred rollback.
pub fn call(
    session: &mut Session,
    actor: u8,
    action: wire::Action,
) -> Result<wire::Response, Error> {
    let request = session.invoke(actor, action.into())?;
    let response = session.receive()?;
    if response.request != request || response.actor != actor || response.passed_gate.is_some() {
        return Err(protocol("unexpected sequential durable response"));
    }
    Ok(response)
}

/// Evidence of one configured group of actual worker actor threads. The replies
/// are in observed return order, preserving overlap in the raw native journal.
#[derive(Debug)]
pub struct Rendezvous {
    /// Consumed one-shot gate number.
    pub gate: u64,
    /// Request numbers in invocation order.
    pub invoked: Vec<u64>,
    /// Product observations in external return order, including typed failures.
    pub returned: Vec<wire::Response>,
}

/// Configure a gate, submit every participant before receiving, then collect
/// exactly those replies. Each command runs on its actor's actual worker thread.
/// The caller must first consume all earlier responses and must use a fresh gate.
/// A barrier proves transport rendezvous, not an engine internal cut or concurrent
/// publication. The commit coordinator may correctly serialize both operations.
///
/// # Errors
/// Duplicate actors, lifecycle commands and malformed groups fail before send.
/// Missing participants, worker loss and deadlines remain incomplete transport
/// evidence; they are never converted to rejected/PASS histories.
pub fn rendezvous(
    session: &mut Session,
    gate: u64,
    commands: Vec<(u8, wire::Action)>,
) -> Result<Rendezvous, Error> {
    if !(1..=wire::MAX_GATES).contains(&gate) || !(2..=wire::MAX_PENDING).contains(&commands.len())
    {
        return Err(protocol("rendezvous gate/group bound"));
    }
    let mut actors = Vec::with_capacity(commands.len());
    for (actor, action) in &commands {
        if usize::from(*actor) >= wire::MAX_PENDING || actors.contains(actor) || action.exclusive()
        {
            return Err(protocol("rendezvous duplicate actor or exclusive action"));
        }
        actors.push(*actor);
    }
    let configured = call(
        session,
        0,
        wire::Action::ConfigureGate {
            gate,
            actors: actors.clone(),
        },
    )?;
    if configured.observation != (wire::Observation::Done {}) {
        return Err(protocol("rendezvous configuration did not complete"));
    }
    let mut invoked = Vec::with_capacity(commands.len());
    for (actor, action) in commands {
        invoked.push(session.invoke(
            actor,
            wire::Command {
                gate: Some(gate),
                action,
            },
        )?);
    }
    let mut returned = Vec::with_capacity(invoked.len());
    for _ in &invoked {
        let response = session.receive()?;
        let Some(index) = invoked
            .iter()
            .position(|request| *request == response.request)
        else {
            return Err(protocol("unexpected rendezvous response"));
        };
        if response.actor != actors[index]
            || response.passed_gate != Some(gate)
            || returned
                .iter()
                .any(|seen: &wire::Response| seen.request == response.request)
        {
            return Err(protocol("missing or duplicate rendezvous witness"));
        }
        returned.push(response);
    }
    Ok(Rendezvous {
        gate,
        invoked,
        returned,
    })
}

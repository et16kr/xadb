//! One batch transport, preserving input, worker and immediate raw native history.
//! This layer does not import the engine or turn worker observations into a verdict.
use crate::{
    file_sha256,
    native::{self, NativeProtocol, NativeSession},
    sha256,
};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};
use xadbtest_protocol::{durable_performance as wire, frame::Envelope};
/// Dedicated bounded deadlines for actual 576-sync batches. Other native protocols
/// retain the existing five-second request/sixty-second session defaults.
pub struct PerformanceProtocol;
impl NativeProtocol for PerformanceProtocol {
    type Action = wire::Input;
    type Request = wire::Request;
    type Response = wire::Response;
    const NAME: &'static str = "durable-performance-v1";
    const REQUEST_MS: u64 = 60_000;
    const RUN_MS: u64 = 120_000;
    fn request(run: [u8; 16], request: u64, actor: u8, input: wire::Input) -> wire::Request {
        wire::Request {
            version: wire::VERSION,
            run,
            request,
            actor,
            input,
        }
    }
    fn identity(response: &wire::Response) -> ([u8; 16], u64, u8) {
        (response.run, response.request, response.actor)
    }
}
/// Exact predeclared input/binary identities. Paths belong to the controller.
pub struct Options {
    pub input: PathBuf,
    pub input_sha256: String,
    pub worker: PathBuf,
    pub worker_sha256: String,
    pub artifacts: PathBuf,
    pub run: [u8; 16],
}
/// One completed transport bundle, not a performance/engine correctness verdict.
/// # Errors
/// Hash/input/transport/retention errors fail; no run is silently removed.
pub fn run(options: Options) -> Result<native::Report, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    fs::File::open(&options.input)?
        .take(4097)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4096 || sha256(&bytes) != options.input_sha256 {
        return Err("performance input bound/hash mismatch".into());
    }
    let input: wire::Input = serde_json::from_slice(&bytes)?;
    wire::Request {
        version: wire::VERSION,
        run: options.run,
        request: 1,
        actor: 0,
        input: input.clone(),
    }
    .validate()?;
    let mut native_options = native::Options::new(
        options.worker,
        options.worker_sha256,
        options.artifacts,
        options.run,
    );
    #[cfg(feature = "self-test")]
    {
        native_options.test.limits.request_ms = PerformanceProtocol::REQUEST_MS;
        native_options.test.limits.run_ms = PerformanceProtocol::RUN_MS;
    }
    #[cfg(not(feature = "self-test"))]
    let native_options = {
        let _ = &mut native_options;
        native_options
    };
    let mut session = NativeSession::<PerformanceProtocol>::start(native_options)?;
    let mut retained = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(session.directory().join("input.json"))?;
    retained.write_all(&bytes)?;
    retained.sync_all()?;
    let mut binding = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(session.directory().join("performance-input.json"))?;
    serde_json::to_writer_pretty(
        &mut binding,
        &serde_json::json!({"schema_version":1,"input_sha256":options.input_sha256,"worker_sha256":file_sha256(&session.directory().join("worker"))?,"request_ms":PerformanceProtocol::REQUEST_MS,"run_ms":PerformanceProtocol::RUN_MS}),
    )?;
    binding.sync_all()?;
    session.invoke(0, input.clone())?;
    let response = session.receive()?;
    if response.output.input != input {
        return Err("performance returned input differs from predeclared input".into());
    }
    if serde_json::to_vec(&response)?.len() + 4 > wire::OUTPUT_CAP {
        return Err("performance response exceeds reserved output bound".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(session.directory().join("performance-output.json"))?;
    serde_json::to_writer_pretty(&mut output, &response)?;
    output.sync_all()?;
    let report = session.finish()?;
    Ok(report)
}

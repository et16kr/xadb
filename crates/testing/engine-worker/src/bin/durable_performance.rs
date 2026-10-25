//! A single real-I/O performance batch; no expected values or verdicts enter it.
fn main() -> std::process::ExitCode {
    match engine_worker::durable_performance::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("durable performance worker: {error}");
            std::process::ExitCode::from(2)
        }
    }
}

//! Separate durable protocol process; K0's existing binary and framing stay intact.
fn main() -> std::process::ExitCode {
    match engine_worker::durable::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("durable engine worker: {error}");
            std::process::ExitCode::from(2)
        }
    }
}

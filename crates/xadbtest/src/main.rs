use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};
use xadbtest::{RunOptions, file_sha256, run};

fn execute() -> Result<u8, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("hash") => {
            let path = args.next().ok_or("usage: xadbtest hash <file>")?;
            if args.next().is_some() { return Err("hash accepts exactly one file".into()); }
            println!("{}", file_sha256(&PathBuf::from(path))?);
            Ok(0)
        }
        Some("run") => {
            let allowed = ["--case", "--case-sha256", "--worker", "--worker-sha256", "--artifacts"];
            let mut options = BTreeMap::new();
            while let Some(key) = args.next() {
                if !allowed.contains(&key.as_str()) { return Err(format!("unknown option: {key}").into()); }
                let value = args.next().ok_or_else(|| format!("missing value: {key}"))?;
                if options.insert(key, value).is_some() { return Err("duplicate option".into()); }
            }
            let mut take = |name| options.remove(name).ok_or_else(|| format!("missing option: {name}"));
            let config = RunOptions {
                case: take("--case")?.into(), expected_case_sha256: take("--case-sha256")?,
                worker: take("--worker")?.into(), expected_worker_sha256: take("--worker-sha256")?,
                artifacts: take("--artifacts")?.into(),
                #[cfg(feature = "self-test")]
                worker_args: vec![],
                #[cfg(feature = "self-test")]
                inject_cleanup_failure: false,
            };
            let report = run(&config)?;
            println!("{}", serde_json::json!({"run_directory":report.directory, "case_verdict":report.verdict.case_verdict, "exit_code":report.verdict.exit_code()}));
            Ok(report.verdict.exit_code())
        }
        _ => Err("usage: xadbtest hash <file> | xadbtest run --case <json> --case-sha256 <sha256> --worker <binary> --worker-sha256 <sha256> --artifacts <directory>".into()),
    }
}

fn main() -> ExitCode {
    match execute() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("XADBTEST-HARNESS: {error}");
            ExitCode::from(3)
        }
    }
}

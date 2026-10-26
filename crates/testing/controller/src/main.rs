use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};
use test_controller::{RunOptions, file_sha256, run};

fn campaign_id(value: &str) -> Result<[u8; 16], Box<dyn std::error::Error>> {
    if value.len() != 32 || !value.is_ascii() {
        return Err("campaign ID must be 32 hexadecimal digits".into());
    }
    let mut id = [0; 16];
    for (index, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    if id == [0; 16] {
        return Err("campaign ID must be nonzero".into());
    }
    Ok(id)
}

fn execute() -> Result<u8, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("run-durable-performance") => {
            let allowed=["--input","--input-sha256","--worker","--worker-sha256","--artifacts","--run-id"];
            let mut options=BTreeMap::new();
            while let Some(key)=args.next() {
                if !allowed.contains(&key.as_str()) {return Err(format!("unknown option: {key}").into());}
                let value=args.next().ok_or_else(||format!("missing value: {key}"))?;
                if options.insert(key,value).is_some() {return Err("duplicate option".into());}
            }
            let mut take=|name| options.remove(name).ok_or_else(||format!("missing option: {name}"));
            let report=test_controller::durable_performance::run(test_controller::durable_performance::Options {
                input:take("--input")?.into(),input_sha256:take("--input-sha256")?,worker:take("--worker")?.into(),worker_sha256:take("--worker-sha256")?,artifacts:take("--artifacts")?.into(),run:campaign_id(&take("--run-id")?)?,
            })?;
            println!("{}",serde_json::to_string(&report)?);
            Ok(0)
        }
        Some("performance-profile") => {
            if args.next().is_some() { return Err("performance-profile accepts no options".into()); }
            println!("{}", serde_json::to_string_pretty(&test_controller::performance::profile())?);
            Ok(0)
        }
        Some("prepare-performance") => {
            use std::io::{Read, Write};
            use test_controller::performance::{BuildReceipt, Recipe, Side};
            use test_protocol::performance::DiagnosticsMode;
            let allowed=["--receipt","--receipt-sha256","--campaign-id","--seed","--mode","--output"];
            let mut options=BTreeMap::new();
            while let Some(key)=args.next() {
                if !allowed.contains(&key.as_str()) {return Err(format!("unknown option: {key}").into());}
                let value=args.next().ok_or_else(||format!("missing value: {key}"))?;
                if options.insert(key,value).is_some() {return Err("duplicate option".into());}
            }
            let mut take=|name| options.remove(name).ok_or_else(||format!("missing option: {name}"));
            let receipt_path=PathBuf::from(take("--receipt")?);
            let receipt_sha256=take("--receipt-sha256")?;
            if file_sha256(&receipt_path)? != receipt_sha256 {return Err("receipt hash mismatch".into());}
            let mut bytes=Vec::new();
            std::fs::File::open(receipt_path)?.take(65537).read_to_end(&mut bytes)?;
            if bytes.len()>65536 {return Err("receipt exceeds 64 KiB".into());}
            let receipt:BuildReceipt=serde_json::from_slice(&bytes)?;
            let mode=match take("--mode")?.as_str() {"always"=>DiagnosticsMode::Always,"detailed"=>DiagnosticsMode::Detailed,_=>return Err("mode must be always or detailed".into())};
            let recipe=Recipe::instrumentation(campaign_id(&take("--campaign-id")?)?,take("--seed")?.parse()?,Side {worker_sha256:receipt.worker_sha256,receipt_sha256,build:receipt.build,config_sha256:receipt.config_sha256,mode},mode)?;
            let output=PathBuf::from(take("--output")?);
            let bytes=serde_json::to_vec_pretty(&recipe)?;
            Recipe::parse(&bytes)?;
            let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&output)?;
            file.write_all(&bytes)?;file.flush()?;
            println!("{}",serde_json::json!({"recipe":output,"recipe_sha256":test_controller::sha256(&bytes)}));
            Ok(0)
        }
        Some("run-performance") | Some("verify-performance") | Some("replay-performance") => {
            use test_controller::performance;
            let mode=std::env::args().nth(1).expect("matched command");
            let allowed=match mode.as_str() {
                "run-performance"=>vec!["--recipe","--recipe-sha256","--baseline-worker","--candidate-worker","--baseline-receipt","--candidate-receipt","--baseline-config","--candidate-config","--artifacts","--gate","--gate-profile","--gate-profile-sha256"],
                "replay-performance"=>vec!["--directory","--recipe-sha256","--artifacts","--campaign-id","--gate","--gate-profile","--gate-profile-sha256"],
                _=>vec!["--directory","--recipe-sha256"],
            };
            let mut options=BTreeMap::new();
            while let Some(key)=args.next() {
                if !allowed.contains(&key.as_str()) {return Err(format!("unknown option: {key}").into());}
                let value=args.next().ok_or_else(||format!("missing value: {key}"))?;
                if options.insert(key,value).is_some() {return Err("duplicate option".into());}
            }
            // The slot gate is all-or-none; partial options never fall back to an ungated run.
            let gate=match (options.remove("--gate"),options.remove("--gate-profile"),options.remove("--gate-profile-sha256")) {
                (None,None,None)=>None,
                (Some(directory),Some(profile),Some(profile_sha256))=>Some(performance::gate::Options {directory:directory.into(),profile:profile.into(),profile_sha256}),
                _=>return Err("--gate, --gate-profile and --gate-profile-sha256 must be given together".into()),
            };
            let mut take=|name| options.remove(name).ok_or_else(||format!("missing option: {name}"));
            let recipe_sha256=take("--recipe-sha256")?;
            let report=match mode.as_str() {
                "run-performance"=>performance::run(&performance::Options {recipe:take("--recipe")?.into(),recipe_sha256,baseline_worker:take("--baseline-worker")?.into(),candidate_worker:take("--candidate-worker")?.into(),baseline_receipt:take("--baseline-receipt")?.into(),candidate_receipt:take("--candidate-receipt")?.into(),baseline_config:take("--baseline-config")?.into(),candidate_config:take("--candidate-config")?.into(),artifacts:take("--artifacts")?.into(),gate})?,
                "replay-performance"=>performance::replay(&performance::ReplayOptions {directory:take("--directory")?.into(),recipe_sha256,artifacts:take("--artifacts")?.into(),campaign_id:campaign_id(&take("--campaign-id")?)?,gate})?,
                _=>performance::verify(&PathBuf::from(take("--directory")?),&recipe_sha256)?,
            };
            println!("{}",serde_json::to_string(&report)?);
            Ok(report.exit_code())
        }
        Some("hash") => {
            let path = args.next().ok_or("usage: xadbtest hash <file>")?;
            if args.next().is_some() { return Err("hash accepts exactly one file".into()); }
            println!("{}", file_sha256(&PathBuf::from(path))?);
            Ok(0)
        }
        Some("run-catalog") | Some("verify-catalog") => {
            let mode = std::env::args().nth(1).expect("matched command");
            let allowed = if mode == "run-catalog" {
                vec!["--recipe", "--recipe-sha256", "--worker", "--worker-sha256", "--artifacts"]
            } else {vec!["--directory", "--recipe-sha256", "--worker-sha256"]};
            let mut options = BTreeMap::new();
            while let Some(key) = args.next() {
                if !allowed.contains(&key.as_str()) {return Err(format!("unknown option: {key}").into());}
                let value = args.next().ok_or_else(|| format!("missing value: {key}"))?;
                if options.insert(key, value).is_some() {return Err("duplicate option".into());}
            }
            let mut take = |name| options.remove(name).ok_or_else(|| format!("missing option: {name}"));
            let recipe_sha256 = take("--recipe-sha256")?;
            let worker_sha256 = take("--worker-sha256")?;
            let report = if mode == "run-catalog" {
                test_controller::catalog::run(&test_controller::catalog::Options {
                    recipe: take("--recipe")?.into(), recipe_sha256,
                    worker: take("--worker")?.into(), worker_sha256,
                    artifacts: take("--artifacts")?.into(),
                })?
            } else {
                test_controller::catalog::verify(&PathBuf::from(take("--directory")?), &recipe_sha256, &worker_sha256)?
            };
            println!("{}", serde_json::to_string(&report)?);
            Ok(report.exit_code())
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
        _ => Err("usage: xadbtest performance-profile | xadbtest prepare-performance --receipt <json> --receipt-sha256 <sha256> --campaign-id <32hex> --seed <u64> --mode <always|detailed> --output <json> | xadbtest run-performance --recipe <json> --recipe-sha256 <sha256> --baseline-worker <binary> --candidate-worker <binary> --baseline-receipt <json> --candidate-receipt <json> --baseline-config <json> --candidate-config <json> --artifacts <directory> [--gate <directory> --gate-profile <json> --gate-profile-sha256 <sha256>] | xadbtest verify-performance --directory <run> --recipe-sha256 <sha256> | xadbtest replay-performance --directory <run> --recipe-sha256 <sha256> --artifacts <directory> --campaign-id <32hex> [--gate <directory> --gate-profile <json> --gate-profile-sha256 <sha256>] | xadbtest hash <file> | xadbtest run-catalog --recipe <json> --recipe-sha256 <sha256> --worker <binary> --worker-sha256 <sha256> --artifacts <directory> | xadbtest verify-catalog --directory <run> --recipe-sha256 <sha256> --worker-sha256 <sha256> | xadbtest run --case <json> --case-sha256 <sha256> --worker <binary> --worker-sha256 <sha256> --artifacts <directory>".into()),
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

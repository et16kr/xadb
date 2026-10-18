use sha2::{Digest, Sha256};
use std::{env, fs, path::Path, process::Command};

fn collect(path: &Path, files: &mut Vec<std::path::PathBuf>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("source directory") {
            collect(&entry.expect("source entry").path(), files);
        }
    } else {
        files.push(path.to_owned());
    }
}

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").expect("Cargo manifest directory");
    let root = Path::new(&crate_dir)
        .join("../..")
        .canonicalize()
        .expect("workspace");
    let mut files = Vec::new();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "crates",
        "tests",
        "scripts",
        ".github",
    ] {
        let path = root.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        collect(&path, &mut files);
    }
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        let relative = path
            .strip_prefix(&root)
            .expect("workspace source")
            .to_string_lossy();
        let bytes = fs::read(&path).expect("read build input");
        digest.update((relative.len() as u64).to_le_bytes());
        digest.update(relative.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    println!(
        "cargo:rustc-env=XADBTEST_SOURCE_SHA256={:x}",
        digest.finalize()
    );
    let output = Command::new(env::var_os("RUSTC").expect("rustc"))
        .arg("--version")
        .output()
        .expect("rustc version");
    assert!(output.status.success());
    println!(
        "cargo:rustc-env=XADBTEST_TOOLCHAIN={}",
        String::from_utf8_lossy(&output.stdout).trim()
    );
    println!(
        "cargo:rustc-env=XADBTEST_TARGET={}",
        env::var("TARGET").expect("target")
    );
    println!(
        "cargo:rustc-env=XADBTEST_PROFILE={}",
        env::var("PROFILE").expect("profile")
    );
}

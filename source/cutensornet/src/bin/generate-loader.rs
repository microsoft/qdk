//! Write the generated cuTensorNet loader to disk.
//!
//! All of the logic lives in [`qdk_cutensornet::generator`]; this binary only
//! reads the two inputs, runs the rendered output through `rustfmt`, and either
//! writes it or compares it against the checked-in files.
//!
//! ```text
//! cargo run -p qdk_cutensornet --bin generate-loader
//! cargo run -p qdk_cutensornet --bin generate-loader -- --check
//! ```

use qdk_cutensornet::generator::{GeneratedFile, parse_manifest, render};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, io, process};

const RUST_EDITION: &str = "2024";

/// Write every rendered file below `root` and format it in place.
fn write_formatted(root: &Path, files: &[GeneratedFile]) -> Result<(), String> {
    let mut written = Vec::with_capacity(files.len());
    for file in files {
        let target = root.join(&file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        fs::write(&target, &file.contents)
            .map_err(|error| format!("{}: {error}", target.display()))?;
        written.push(target);
    }

    let status = Command::new("rustfmt")
        .arg("--edition")
        .arg(RUST_EDITION)
        .args(&written)
        .status()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => "rustfmt is not available".to_owned(),
            _ => format!("rustfmt: {error}"),
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("rustfmt exited with {status}"))
    }
}

/// A process-unique staging directory; `--check` must not touch the worktree.
fn staging_dir() -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .subsec_nanos();
    let path = env::temp_dir().join(format!("qdk-generate-loader-{}-{nanos}", process::id()));
    fs::create_dir_all(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}

/// Return the checked-in files that differ from freshly generated output.
fn stale_files(library: &Path, files: &[GeneratedFile]) -> Result<Vec<String>, String> {
    let staging = staging_dir()?;
    let outcome = (|| {
        write_formatted(&staging, files)?;
        let mut stale = Vec::new();
        for file in files {
            let fresh = fs::read(staging.join(&file.path))
                .map_err(|error| format!("{}: {error}", file.path))?;
            if fresh != fs::read(library.join(&file.path)).unwrap_or_default() {
                stale.push(file.path.clone());
            }
        }
        Ok(stale)
    })();
    let _ = fs::remove_dir_all(&staging);
    outcome
}

fn run() -> Result<(), String> {
    let check_only = match env::args().nth(1).as_deref() {
        None => false,
        Some("--check") => true,
        Some(other) => return Err(format!("unexpected argument {other:?}; expected --check")),
    };

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = root.join("scripts").join("cutensornet-symbols.txt");
    let bindings_path = root.join("src").join("bindings").join("v2_13.rs");
    let library = root.join("src").join("library");

    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
    let bindings = fs::read_to_string(&bindings_path)
        .map_err(|error| format!("{}: {error}", bindings_path.display()))?;

    let files = render(&manifest, &bindings).map_err(|error| error.to_string())?;
    let symbols = parse_manifest(&manifest)
        .map_err(|error| error.to_string())?
        .len();

    if check_only {
        let stale = stale_files(&library, &files)?;
        if !stale.is_empty() {
            return Err(format!(
                "generated loader is out of date; run `cargo run --bin generate-loader`: {}",
                stale.join(", ")
            ));
        }
    } else {
        write_formatted(&library, &files)?;
    }

    println!("symbols={symbols} files={}", files.len());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("generate-loader: {error}");
            ExitCode::FAILURE
        }
    }
}

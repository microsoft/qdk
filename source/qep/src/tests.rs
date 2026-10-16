// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{Profiler, SequentialCountingProfiler};
use qsc::{LanguageFeatures, SourceMap, codegen::qir::get_qir, target::Profile};
use std::{path::PathBuf, sync::Arc, time::Instant};

pub(crate) struct QirFile(pub(crate) PathBuf);

impl QirFile {
    /// Creates a unique path for a temporary QIR file.
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "qep-qsharp-{}-{}.ll",
            std::process::id(),
            rand::random::<u64>()
        )))
    }

    /// Returns the temporary QIR path as UTF-8.
    pub(crate) fn path(&self) -> Result<&str, &str> {
        self.0
            .to_str()
            .ok_or("temporary QIR path is not valid UTF-8")
    }
}

impl Drop for QirFile {
    /// Removes the temporary QIR file if it was created.
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "Failed to clean up QEP QIR file {}: {error}",
                self.0.display()
            );
        }
    }
}

/// Compiles Q# source to a temporary QIR file.
pub(crate) fn compile_qsharp_to_qir(source: &str, profile: Profile) -> QirFile {
    compile_qsharp_sources_to_qir(vec![("test.qs".into(), source.into())], profile)
}

/// Compiles Q# source together with the `library/arithmetic` sources to a temporary QIR file.
fn compile_qsharp_with_arithmetic_to_qir(source: &str, profile: Profile) -> QirFile {
    let library_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../library/arithmetic/src");
    let mut sources: Vec<(Arc<str>, Arc<str>)> = std::fs::read_dir(&library_dir)
        .expect("failed to read arithmetic library directory")
        .map(|entry| {
            let path = entry.expect("failed to read directory entry").path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("arithmetic library file name should be valid UTF-8");
            let contents =
                std::fs::read_to_string(&path).expect("failed to read arithmetic library file");
            (name.into(), contents.into())
        })
        .collect();
    sources.push(("test.qs".into(), source.into()));
    compile_qsharp_sources_to_qir(sources, profile)
}

/// Compiles the given named Q# sources to a temporary QIR file.
fn compile_qsharp_sources_to_qir(sources: Vec<(Arc<str>, Arc<str>)>, profile: Profile) -> QirFile {
    let capabilities = profile.into();
    let (std_id, store) = qsc::compile::package_store_with_stdlib(capabilities);
    let qir = get_qir(
        SourceMap::new(sources, None),
        LanguageFeatures::default(),
        capabilities,
        store,
        &[(std_id, None)],
    )
    .unwrap_or_else(|errors| {
        panic!(
            "{}",
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let file = QirFile::new();
    std::fs::write(&file.0, qir).expect("failed to write compiled QIR");
    file
}

/// Configures one-microsecond timings for all instructions used by the profiling workloads.
fn create_profiler() -> Result<SequentialCountingProfiler, String> {
    SequentialCountingProfiler::new(
        1,
        [
            "ccx",
            "cx",
            "cz",
            "rz",
            "h",
            "x",
            "swap",
            "reset",
            "mz",
            "mresetz",
            "read_result",
        ]
        .into_iter()
        .map(|name| (name.to_string(), 1e-6))
        .collect(),
    )
}

#[test]
/// Profiles the Shor's algorithm QIR fixture and reports the elapsed time.
fn profiles_shors_50_qir() -> Result<(), String> {
    let qir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tmp/shors_50.ll");
    let qir_path = qir_path
        .to_str()
        .ok_or("Shor's algorithm QIR path is not valid UTF-8")?;
    let mut profiler = create_profiler()?;
    let start = Instant::now();

    // SAFETY: The checked-in QIR fixture was produced by a trusted compiler.
    let result = unsafe { profiler.profile_qir(qir_path) }?;
    let elapsed = start.elapsed();
    assert!(result.infos.is_empty(), "{:?}", result.infos);
    let operation_count: u64 = result.calls_per_operation.values().sum();

    assert!(operation_count > 0);
    let ns_per_operation = elapsed.as_nanos() / u128::from(operation_count);
    println!(
        "Profiled shors_50.ll: {operation_count} operations in {elapsed:?} \
         ({ns_per_operation} ns/op)"
    );
    Ok(())
}

#[test]
/// Profiles a Q# modular multiplication from the arithmetic library and reports the elapsed time.
fn profiles_mod_mul() -> Result<(), String> {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                let n = 500;
                let modulus = (1L <<< n) - 1L;
                use xs = Qubit[n];
                use ys = Qubit[n];
                use ans = Qubit[n];
                ModMul.ModMul(xs, ys, ans, modulus);
            }
        }
    "#;
    let qir = compile_qsharp_with_arithmetic_to_qir(source, Profile::Adaptive);
    let mut profiler = create_profiler()?;
    let start = Instant::now();

    // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
    let result = unsafe { profiler.profile_qir(qir.path()?) }?;
    let elapsed = start.elapsed();
    assert!(result.infos.is_empty(), "{:?}", result.infos);
    let operation_count: u64 = result.calls_per_operation.values().sum();

    assert!(operation_count > 0);
    let ns_per_operation = elapsed.as_nanos() / u128::from(operation_count);
    println!(
        "Profiled Q# modular multiplication: {operation_count} operations in {elapsed:?} \
         ({ns_per_operation} ns/op)"
    );
    // Other gate counts vary with the profiler's random measurement results.
    for (name, expected) in [
        ("mz", 500),
        ("mresetz", 996_003),
        ("read_result", 996_503),
        ("swap", 249_500),
    ] {
        assert_eq!(
            result.calls_per_operation.get(name),
            Some(&expected),
            "unexpected instruction count for {name}"
        );
    }
    Ok(())
}

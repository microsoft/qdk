// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for the PR): the MPO diagnostic
//! (`qdk_cutensornet::dev_mpo_diagnostic`) of
//! `CuTensorNetEngine::with_dev_wide_operators` on the real cuTensorNet,
//! against the CPU engine. It prints one `MPO_DIAGNOSTIC` line per case
//! (the largest probability difference from the CPU engine with the
//! operator as given and transposed) and the cuTensorNet version, then
//! fails if any case differs, so the run's log is the reproducer's
//! evidence. The same cases agree with the CPU engine on the fake
//! libraries (`src/tests/engine.rs`).
//!
//! It needs an NVIDIA GPU, the CUDA Runtime 12.9 and cuTensorNet 2.13, so it
//! is ignored by default; run it with `--ignored --nocapture`.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn every_mpo_case_reads_as_on_the_cpu_engine() {
    let availability = qdk_cutensornet::discover().expect("the audited libraries are installed");
    let versions = availability
        .report()
        .library("cuTensorNet")
        .map(|library| format!("{:?}", library.versions))
        .unwrap_or_default();
    let engine = qdk_cutensornet::CuTensorNetEngine::new(&availability)
        .expect("an engine on CUDA device 0")
        .with_dev_wide_operators(true);
    let reference = qdk_cpu_mps::CpuMpsEngine::default().with_dev_mpo_far(true);
    let results = qdk_cutensornet::dev_mpo_diagnostic::run_cases(&engine, &reference)
        .unwrap_or_else(|error| panic!("every case runs: {error}"));
    let mut failed = Vec::new();
    for result in &results {
        println!(
            "MPO_DIAGNOSTIC case={} sites={:?} max_diff={:.3e} max_diff_transposed={:.3e} \
             asymmetry={:.3} cutensornet_versions={versions}",
            result.case,
            result.sites,
            result.max_difference,
            result.max_difference_transposed,
            result.asymmetry,
        );
        if result.max_difference.is_nan() || result.max_difference > 1e-9 {
            failed.push(result.case);
        }
    }
    assert!(failed.is_empty(), "cases that differ: {failed:?}");
}

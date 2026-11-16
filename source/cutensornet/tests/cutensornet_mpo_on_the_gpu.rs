// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for the PR): the MPO diagnostic
//! (`qdk_cutensornet::dev_mpo_diagnostic`) of
//! `CuTensorNetEngine::with_dev_wide_operators` on the real cuTensorNet,
//! against the CPU engine. It prints one `MPO_DIAGNOSTIC` line per case
//! (the largest probability difference from the CPU engine with the
//! operator as given and transposed, the cuTensorNet version, the same
//! difference from the CPU engine with every MPO applied again at each
//! later compute, and which of the three the engine reads as). It runs
//! every case in four configurations of the engine (`config=` on each
//! line), the MPO application and the gauge crossed: `inexact-simple`,
//! the engine's own (the library's default MPO application, the simple
//! gauge); `exact-simple`, with `CUTENSORNET_STATE_MPO_APPLICATION_EXACT`;
//! `inexact-free` and `exact-free`, the same without the gauge. It fails
//! if any case differs in `exact-simple`, the configuration under test, so
//! the run's log is the evidence. The same
//! cases agree with the CPU engine on the fake libraries
//! (`src/tests/engine.rs`).
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
    let reference = qdk_cpu_mps::CpuMpsEngine::default().with_dev_mpo_far(true);
    let within = |difference: f64| difference <= 1e-9;
    let configurations = [
        ("inexact-simple", false, false),
        ("exact-simple", true, false),
        ("inexact-free", false, true),
        ("exact-free", true, true),
    ];
    let mut failed = Vec::new();
    for (config, mpo_application_exact, mps_gauge_free) in configurations {
        let engine = qdk_cutensornet::CuTensorNetEngine::new(&availability)
            .expect("an engine on CUDA device 0")
            .with_dev_wide_operators(true)
            .with_dev_tuning(qdk_cutensornet::DevTuning {
                mpo_application_exact,
                mps_gauge_free,
                ..qdk_cutensornet::DevTuning::default()
            });
        // A configuration the library refuses (EXACT has not run on 2.13
        // yet) is reported, and the next one still runs; the empty max_diff
        // reads as "differs" in the summary.
        let results = match qdk_cutensornet::dev_mpo_diagnostic::run_cases(&engine, &reference) {
            Ok(results) => results,
            Err(error) => {
                println!(
                    "MPO_DIAGNOSTIC config={config} case=run-error sites=- max_diff= \
                     cutensornet_versions={versions} error={error}"
                );
                if config == "exact-simple" {
                    failed.push("run-error");
                }
                continue;
            }
        };
        for result in &results {
            let reading = if within(result.max_difference) {
                "reference"
            } else if result.max_difference_reapplied.is_some_and(within) {
                "reapplied"
            } else if within(result.max_difference_transposed) {
                "transposed"
            } else {
                "unexplained"
            };
            let reapplied = result
                .max_difference_reapplied
                .map_or_else(|| "-".to_string(), |difference| format!("{difference:.3e}"));
            println!(
                "MPO_DIAGNOSTIC config={config} case={} sites={:?} max_diff={:.3e} \
                 max_diff_transposed={:.3e} asymmetry={:.3} cutensornet_versions={versions} \
                 max_diff_reapplied={reapplied} reading={reading}",
                result.case,
                result.sites,
                result.max_difference,
                result.max_difference_transposed,
                result.asymmetry,
            );
            if config == "exact-simple" && !within(result.max_difference) {
                failed.push(result.case);
            }
        }
    }
    assert!(
        failed.is_empty(),
        "cases that differ in exact-simple: {failed:?}"
    );
}

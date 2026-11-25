// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GpuSetup`]: that a user reads what was found, whether the setup is
//! ready, and for each kind of problem, the fix that applies to it.
//!
//! The tests check what each message names (a library, a variable, a
//! path, a version, `nvidia-smi`, a command), the word that tells one kind
//! of remedy or outcome from another (restart, remove, unset, not read,
//! not tried) and the report's labels. They never check a message's full
//! text, so that a reworded message keeps its tests.
//!
//! This file tests the report and whether the setup is ready;
//! `tests/advice.rs` tests the fix for each kind of problem, and
//! `tests/scenes.rs` builds the setups both check. The reading of the
//! devices is tested beside it (`device_report/tests.rs`).

#[cfg(unix)]
mod advice;
#[cfg(unix)]
mod scenes;

use crate::{
    AvailabilityError, GpuSetup, gpu_setup::LoadedSetup, libraries::setup_guide::SetupGuide,
};

/// The setup of a host where discovery gave `loaded`, with the setup
/// guides `guides`, checked with no packager's install command. It is the
/// one place the tests build a [`GpuSetup`] from its parts, so that a
/// change of its fields changes the scenes' helpers alone.
fn checked(loaded: Result<LoadedSetup, AvailabilityError>, guides: Vec<SetupGuide>) -> GpuSetup {
    GpuSetup {
        loaded,
        guides,
        install_command: None,
    }
}

/// The advice of `setup`, which has a problem. The asserts are
/// preconditions every advice test shares, not the rule a test checks. A
/// problem leaves the setup unready, and the report ends with its fix.
fn advice(setup: &GpuSetup) -> String {
    assert!(!setup.is_ready(), "a problem leaves the setup unready");
    let advice = setup.advice().expect("a problem has advice");
    assert!(
        setup.to_string().ends_with(&format!("\nto fix: {advice}")),
        "the report ends with the fix: {setup}"
    );
    advice
}

/// Off Linux x86-64 the advice says where the GPU engine runs. There, the
/// check stops at discovery, with no device read and no trial run, and
/// advises so.
#[test]
fn an_unsupported_platform_is_advised_where_the_engine_runs() {
    let mut setups = vec![checked(
        Err(AvailabilityError::UnsupportedPlatform {
            platform: "macos aarch64".to_string(),
        }),
        Vec::new(),
    )];
    if cfg!(not(all(target_os = "linux", target_arch = "x86_64"))) {
        setups.push(GpuSetup::check(&crate::PackagedLibraries::default()));
    }
    for setup in &setups {
        let advice = advice(setup);
        assert!(advice.contains("Linux x86-64"), "{advice}");
    }
}

/// Whether `text` shows `error`, then each error it was caused by, in
/// order: a wrapping error does not hide why it failed.
#[cfg(unix)]
fn shows_with_its_causes(text: &str, error: &dyn std::error::Error) -> bool {
    let mut rest = text;
    let mut cause = Some(error);
    while let Some(error) = cause {
        let message = error.to_string();
        let Some(at) = rest.find(&message) else {
            return false;
        };
        rest = &rest[at + message.len()..];
        cause = error.source();
    }
    true
}

/// The tests of the report and the readiness on a host that can load
/// libraries.
#[cfg(unix)]
mod on_unix {
    use std::{error::Error, path::PathBuf, sync::Arc};

    use canonical_mps::CanonicalMpsEngine;

    use super::{
        advice,
        scenes::{
            GateSplitIgnoringItsGate, PIP_INSTALL, a100, audited_report, audited_with, capability,
            cuda_runtime_12_8, device_of, driver_probe_failed, failed, failed_trial_run,
            fake_algebra, fake_algebra_on, found, loaded, not_found, setup, two_other_copies,
            unreadable_devices, with_devices, with_driver, with_trial_run,
        },
        shows_with_its_causes,
    };
    use crate::{
        AvailabilityError, CudaMpsEngine, GpuSetup, LibrarySource, PackagedLibraries,
        engine::native_calls::fake_libraries::FakeLibraries,
        gpu_setup::{device_report::DeviceReport, run_trial},
    };

    /// The line of `report` that starts with `label`.
    fn line<'a>(report: &'a str, label: &str) -> &'a str {
        report
            .lines()
            .find(|line| line.starts_with(label))
            .unwrap_or_else(|| panic!("a line {label:?} in {report}"))
    }

    /// A user whose libraries were found and whose device ran the trial run
    /// reads each library's path and version, the driver's CUDA, how many
    /// devices there are, which one the GPU engine runs on with its compute
    /// capability and memory, and that the trial run succeeded, with nothing
    /// to fix.
    #[test]
    fn a_ready_setup_lists_what_was_found_and_nothing_to_fix() {
        let ready = setup(with_devices(
            loaded(audited_report(), Some(a100())),
            Ok(DeviceReport {
                count: 2,
                engine_device: Some(a100()),
            }),
        ));
        assert!(ready.is_ready());
        assert_eq!(ready.advice(), None);
        let report = ready.to_string();
        for (label, items) in [
            (
                "cuTensorNet:",
                &["/env/cuquantum/lib/libcutensornet.so.2", "2.13.0", "12.9"][..],
            ),
            (
                "CUDA Runtime:",
                &["/env/nvidia/cuda_runtime/lib/libcudart.so.12", "12.9"],
            ),
            ("CUDA driver:", &["12.8"]),
            ("CUDA devices: 2", &["8.0", "79.0", "80.0"]),
            ("trial run on CUDA device 0:", &["ready"]),
        ] {
            let line = line(&report, label);
            for item in items {
                assert!(line.contains(item), "{item:?} in {line:?}");
            }
        }
        assert!(!report.contains("to fix"), "{report}");
    }

    /// The trial run, on an engine whose libraries compute right (the fake
    /// libraries, which simulate cuTensorNet and the CUDA Runtime on the CPU),
    /// reads a probability the check takes for right. A check that took its
    /// own run for wrong would refuse every sound host.
    #[test]
    fn a_trial_run_on_an_engine_that_computes_right_is_ready() {
        let engine = CudaMpsEngine::new(fake_algebra());
        let setup = setup(with_trial_run(
            loaded(audited_report(), Some(a100())),
            run_trial(&engine),
        ));
        assert!(setup.is_ready(), "{setup}");
    }

    /// Libraries whose tensor computation is broken can still upload and
    /// read sites right. The trial run makes them compute an entangling
    /// gate. So a gate split that fails, or that ignores its gate, leaves
    /// the setup unready, advised by the trial run. Each row is the trial
    /// run on such libraries.
    #[test]
    fn a_trial_run_on_libraries_whose_gate_split_is_faulty_is_not_ready() {
        let failing = Arc::new(FakeLibraries::new());
        failing.fail("gate_split", 1);
        let gateless = GateSplitIgnoringItsGate(fake_algebra());
        for trial_run in [
            run_trial(&CudaMpsEngine::new(fake_algebra_on(failing))),
            run_trial(&CanonicalMpsEngine::new(gateless)),
        ] {
            let setup = setup(with_trial_run(
                loaded(audited_report(), Some(a100())),
                trial_run,
            ));
            assert!(!setup.is_ready(), "{setup}");
            let advice = advice(&setup);
            assert!(advice.contains("trial run"), "{advice}");
        }
    }

    /// A failed discovery reports what it found of each library beside each
    /// fault: the usable one's path and versions, and those a library at
    /// fault reported (a library not found shows its fault alone). It also
    /// reports that no device was read and no trial run tried.
    #[test]
    fn a_failed_discovery_reports_what_it_found_its_faults_and_no_trial_run() {
        let missing_cutensornet = audited_with("cuTensorNet", |cutensornet| {
            cutensornet.faults = vec![not_found("libcutensornet.so.2")];
            cutensornet.path = None;
            cutensornet.versions.clear();
        });
        let old_cuda_runtime = audited_with("CUDA Runtime", |cuda_runtime| {
            cuda_runtime.faults = vec![cuda_runtime_12_8()];
            cuda_runtime.path = Some(PathBuf::from("/opt/cuda-12.8/libcudart.so.12"));
            cuda_runtime.versions[0].value = 12_080;
        });
        let (not_found, old_version) = (
            not_found("libcutensornet.so.2").to_string(),
            cuda_runtime_12_8().to_string(),
        );
        for (report, expected, absent) in [
            (
                missing_cutensornet,
                vec![
                    (
                        "CUDA Runtime:",
                        vec!["/env/nvidia/cuda_runtime/lib/libcudart.so.12", "12.9"],
                    ),
                    ("CUDA driver:", vec!["12.8"]),
                    ("cuTensorNet at fault:", vec![not_found.as_str()]),
                ],
                Some("cuTensorNet:"),
            ),
            (
                old_cuda_runtime,
                vec![
                    (
                        "CUDA Runtime:",
                        vec!["/opt/cuda-12.8/libcudart.so.12", "12.8"],
                    ),
                    ("CUDA Runtime at fault:", vec![old_version.as_str()]),
                    (
                        "cuTensorNet:",
                        vec!["/env/cuquantum/lib/libcutensornet.so.2", "2.13.0", "12.9"],
                    ),
                ],
                None,
            ),
        ] {
            let report = failed(AvailabilityError::Unusable(report)).to_string();
            for (label, items) in expected.into_iter().chain([
                ("CUDA devices:", vec!["not read"]),
                ("trial run on CUDA device 0:", vec!["not tried"]),
            ]) {
                let line = line(&report, label);
                for item in items {
                    assert!(line.contains(item), "{item:?} in {line:?}");
                }
            }
            if let Some(absent) = absent {
                assert!(
                    !report.lines().any(|line| line.starts_with(absent)),
                    "no {absent:?} in {report}"
                );
            }
        }
    }

    /// A host with no CUDA device, or with no NVIDIA driver, for which CUDA
    /// reports version 0, reads "none" on that item's line, never a zero
    /// version (a driver for CUDA 0.0). Each row is the line's label and the
    /// host's report.
    #[test]
    fn an_absent_device_or_driver_is_shown_as_none() {
        for (label, report) in [
            ("CUDA devices:", audited_report()),
            ("CUDA driver:", with_driver(0)),
        ] {
            let report = found(report, None).to_string();
            let line = line(&report, label);
            assert!(line.contains("none") && !line.contains("0.0"), "{line:?}");
        }
    }

    /// A step that failed is shown on its line with its error and each of
    /// the error's causes. The devices line shows the CUDA Runtime's error.
    /// The trial run's line shows the engine's error, which says only that
    /// its algebra failed, then the algebra's, which says why. Each row is
    /// the line's label, the setup, and the error the step failed with.
    #[test]
    fn a_failed_step_is_shown_on_its_line_with_its_error_and_causes() {
        let trial_run_error = failed_trial_run().expect_err("the trial run failed");
        assert!(
            trial_run_error.source().is_some(),
            "the engine's error has the algebra's as its cause"
        );
        let rows: [(&str, GpuSetup, &dyn Error); 2] = [
            (
                "CUDA devices:",
                setup(with_devices(
                    loaded(audited_report(), Some(a100())),
                    Err(unreadable_devices()),
                )),
                &unreadable_devices(),
            ),
            (
                "trial run on CUDA device 0:",
                setup(with_trial_run(
                    loaded(audited_report(), Some(a100())),
                    failed_trial_run(),
                )),
                &trial_run_error,
            ),
        ];
        for (label, setup, error) in rows {
            let report = setup.to_string();
            let line = line(&report, label);
            assert!(shows_with_its_causes(line, error), "{line:?}");
        }
    }

    /// Another copy of a library beside the one loaded is a warning naming
    /// both copies, in backticks as the faults show a path. It is shown in
    /// the report whether or not discovery succeeded, since a stray copy is
    /// a likely cause of a failure. The warning is no
    /// problem: it leaves a setup with none ready. Each row is a scene, the
    /// other copy and the one loaded, and whether the setup is ready. In
    /// one, a user's cuTensorNet override is loaded beside a wheel's copy,
    /// with a sound setup. In the other, a user's CUDA Runtime override
    /// loads a copy of another release beside the audited copy a package
    /// installed.
    #[test]
    fn another_copy_of_a_library_is_warned_of_whether_or_not_discovery_succeeds() {
        let other_release = audited_with("CUDA Runtime", |cuda_runtime| {
            cuda_runtime.faults = vec![cuda_runtime_12_8()];
            cuda_runtime.path = Some(PathBuf::from("/opt/cuda-12.8/libcudart.so.12"));
            cuda_runtime.source = Some(LibrarySource::Override);
            cuda_runtime.other_copies = vec![PathBuf::from(
                "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
            )];
        });
        for (scene, setup, other, loaded, ready) in [
            (
                "discovery succeeded",
                found(two_other_copies(), Some(a100())),
                "/env/cuquantum/lib/libcutensornet.so.2",
                "/opt/override/libcutensornet.so.2",
                true,
            ),
            (
                "discovery failed",
                failed(AvailabilityError::Unusable(other_release)),
                "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
                "/opt/cuda-12.8/libcudart.so.12",
                false,
            ),
        ] {
            assert_eq!(setup.is_ready(), ready, "{scene}: {setup}");
            let warning = setup
                .warnings()
                .into_iter()
                .find(|warning| warning.contains(&format!("`{other}`")))
                .unwrap_or_else(|| panic!("{scene}: a warning names `{other}`"));
            assert!(
                warning.contains(&format!("`{loaded}`")),
                "{scene}: {warning}"
            );
            assert!(
                setup.to_string().contains(&format!("warning: {warning}")),
                "{scene}: {setup}"
            );
        }
    }

    /// A warning's remedy changes the copy at fault, never a sound one. When
    /// the CUDA Runtime loaded, from its override or not, is of another
    /// release, the user is told to set the override to an audited copy, or
    /// else to unset the override or remove the copy loaded. When the one
    /// loaded passed its checks, or only a version probe failed, the other
    /// copy, never checked, is to be removed, and the override kept.
    #[test]
    fn a_warning_says_to_change_the_copy_loaded_only_when_it_is_at_fault() {
        let at_fault = vec![cuda_runtime_12_8()];
        let driver_at_fault = vec![driver_probe_failed()];
        let variable = "QDK_CUDART_LIBRARY";
        let sound: (&[&str], &[&str]) = (&["remove", "other"], &[variable, "unset"]);
        let (overridden, packaged) = (LibrarySource::Override, LibrarySource::Packaged);
        for (source, faults, (named, not_named)) in [
            (
                overridden,
                at_fault.clone(),
                (&[variable, "unset"][..], &["remove"][..]),
            ),
            (packaged, at_fault, (&[variable, "remove"], &["unset"])),
            (overridden, driver_at_fault.clone(), sound),
            (packaged, driver_at_fault, sound),
            (overridden, Vec::new(), sound),
            (packaged, Vec::new(), sound),
        ] {
            let usable = faults.is_empty();
            let report = audited_with("CUDA Runtime", |cuda_runtime| {
                cuda_runtime.faults = faults;
                cuda_runtime.source = Some(source);
                cuda_runtime.other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
            });
            let setup = if usable {
                found(report, Some(a100()))
            } else {
                failed(AvailabilityError::Unusable(report))
            };
            let warnings = setup.warnings();
            let [warning] = warnings.as_slice() else {
                panic!("one warning for the other copy: {warnings:?}");
            };
            let (_, fix) = warning
                .rsplit_once(": ")
                .expect("a warning ends with its fix");
            assert!(
                named.iter().all(|item| fix.contains(item))
                    && !not_named.iter().any(|item| fix.contains(item)),
                "{warning}"
            );
        }
    }

    /// A user whose CUDA device 0 is too old, with another copy of the CUDA
    /// Runtime visible, reads what was found first, then the warning, then
    /// the fix last.
    #[test]
    fn the_report_lists_what_was_found_then_the_warnings_then_the_fix() {
        let report = audited_with("CUDA Runtime", |cuda_runtime| {
            cuda_runtime.other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
        });
        let text = found(report, Some(device_of(capability(7, 0)))).to_string();
        let position = |line: &str| {
            text.find(line)
                .unwrap_or_else(|| panic!("{line:?} in {text}"))
        };
        let found_last = position("\ntrial run on CUDA device 0:");
        let warning = position("\nwarning: ");
        let fix = position("\nto fix: ");
        assert!(found_last < warning && warning < fix, "{text}");
    }

    /// A packager's files and install command reach the check. It reports
    /// what discovery finds given those files, and advises a failure with the
    /// packager's command. Here, with no override set, the packaged CUDA
    /// Runtime, not a library, is the copy tried, and fails to load. With the
    /// default files, it would not be tried at all.
    #[test]
    fn the_check_discovers_with_the_packaged_files() {
        use crate::libraries::cuda_runtime::CudaRuntime;

        let (cudart, _remove) =
            crate::test_utils::packaged_file::<CudaRuntime>("check-packaged-cudart");
        let packaged = PackagedLibraries {
            files: vec![cudart.clone()],
            install_command: Some(PIP_INSTALL.to_string()),
        };
        let setup = GpuSetup::check_from(|_| None, &packaged);
        let advice = advice(&setup);
        let report = setup.to_string();
        let at_fault = line(&report, "CUDA Runtime at fault:");
        assert!(
            at_fault.contains(&format!("`{}`", cudart.display())),
            "{report}"
        );
        assert!(advice.contains(PIP_INSTALL), "{advice}");
    }
}

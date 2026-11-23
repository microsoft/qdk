// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GpuSetup`]: that a user reads what was found, whether the setup is
//! ready, and for each kind of problem, the fix that applies to it.
//!
//! The tests check what each message names (a library, a variable, a
//! path, a version, `nvidia-smi`, a command), the word that tells one kind
//! of remedy or outcome from another (restart, remove, unset, not read,
//! not tried) and the report's labels, never a message's full text, so
//! that a reworded message keeps its tests.

mod device_report;

use crate::{
    AvailabilityError, GpuSetup, gpu_setup::LoadedSetup, libraries::setup_guide::SetupGuide,
};

/// The setup of a host where discovery gave `loaded`, with the setup
/// guides `guides`, checked with no packager's install command: the one
/// place the tests build a [`GpuSetup`] from its parts, so that a change of
/// its fields changes the scenes' helpers alone.
fn checked(loaded: Result<LoadedSetup, AvailabilityError>, guides: Vec<SetupGuide>) -> GpuSetup {
    GpuSetup {
        loaded,
        guides,
        install_command: None,
    }
}

/// The advice of `setup`, which has a problem. The asserts are
/// preconditions every advice test shares, not the rule a test checks: a
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
/// check itself stops at discovery, with no device read and no trial run,
/// and so advises.
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

/// The tests of the setup on a host that can load libraries: every
/// test but the one above, which holds on every platform.
#[cfg(unix)]
mod on_unix {
    use std::{error::Error, ffi::OsString, path::PathBuf, sync::Arc};

    use canonical_mps::CanonicalMpsError;

    use super::{advice, checked};
    use crate::{
        AvailabilityError, AvailabilityReport, CudaAlgebra, CudaAlgebraError, CudaMpsEngine,
        GpuSetup, LibraryFault, LibraryReport, LibrarySource, PackagedLibraries, Version,
        engine::native_calls::{NativeCalls, fake_libraries::FakeLibraries},
        gpu_setup::{
            LoadedSetup,
            device_report::{DeviceReport, EngineDevice},
            run_trial,
        },
        libraries::setup_guide::{DriverFloor, SetupGuide},
    };

    /// Whether `advice` advises the install: it names each library at its
    /// audited version.
    fn advises_install(advice: &str) -> bool {
        ["cuTensorNet 2.13", "CUDA Runtime 12.9"]
            .iter()
            .all(|library| advice.contains(library))
    }

    /// Whether `advice` says to restart the process, which loads what an
    /// install brings.
    fn advises_restart(advice: &str) -> bool {
        advice.contains("restart")
    }

    /// The line of `report` that starts with `label`.
    fn line<'a>(report: &'a str, label: &str) -> &'a str {
        report
            .lines()
            .find(|line| line.starts_with(label))
            .unwrap_or_else(|| panic!("a line {label:?} in {report}"))
    }

    /// Whether `text` shows `error`, then the error's cause: a wrapping error
    /// does not hide why it failed.
    fn shows_with_its_cause(text: &str, error: &dyn Error) -> bool {
        let message = error.to_string();
        let cause = error.source().expect("the error has a cause").to_string();
        text.find(&message)
            .is_some_and(|at| text[at + message.len()..].contains(&cause))
    }

    /// The command a Python packager gives for its install.
    const PIP_INSTALL: &str = "pip install \"qdk[tensornetwork]\"";

    /// The report of `library`, usable, loaded from `path`, a packaged file,
    /// with `versions`.
    fn usable(
        library: &'static str,
        path: &str,
        versions: [(&'static str, i128); 2],
    ) -> LibraryReport {
        LibraryReport {
            library,
            path: Some(PathBuf::from(path)),
            versions: versions
                .map(|(component, value)| Version { component, value })
                .to_vec(),
            faults: Vec::new(),
            other_copies: Vec::new(),
            source: Some(LibrarySource::Packaged),
        }
    }

    /// The report of the audited libraries, found at `/env/…`, with a driver
    /// that supports CUDA 12.8: the CUDA Runtime, then cuTensorNet, in the
    /// order discovery opens them.
    fn audited_report() -> AvailabilityReport {
        AvailabilityReport {
            libraries: vec![
                usable(
                    "CUDA Runtime",
                    "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
                    [("CUDA Runtime", 12_090), ("CUDA driver", 12_080)],
                ),
                usable(
                    "cuTensorNet",
                    "/env/cuquantum/lib/libcutensornet.so.2",
                    [
                        ("cuTensorNet", 21_300),
                        ("cuTensorNet's CUDA Runtime", 12_090),
                    ],
                ),
            ],
        }
    }

    /// The audited report with a driver that supports CUDA `driver`.
    fn with_driver(driver: i128) -> AvailabilityReport {
        let mut report = audited_report();
        report.libraries[0].versions[1].value = driver;
        report
    }

    /// The error of a discovery where `library` has `faults` and the other
    /// library is usable.
    fn unusable(library: &str, faults: Vec<LibraryFault>) -> AvailabilityError {
        let mut report = audited_report();
        let at_fault = report
            .libraries
            .iter_mut()
            .find(|report| report.library == library)
            .expect("discovery opens the library");
        at_fault.faults = faults;
        AvailabilityError::Unusable(report)
    }

    /// An A100 with 80 GiB, 79 free: compute capability 8.0.
    fn a100() -> EngineDevice {
        EngineDevice {
            compute_capability: (8, 0),
            free_bytes: 79 << 30,
            total_bytes: 80 << 30,
        }
    }

    /// What the steps find on a host where discovery found `report`, the
    /// process sees `engine_device` alone, if any, and the trial run read the
    /// exact probability.
    fn loaded(report: AvailabilityReport, engine_device: Option<EngineDevice>) -> LoadedSetup {
        LoadedSetup {
            libraries: report,
            devices: Ok(DeviceReport {
                count: i32::from(engine_device.is_some()),
                engine_device,
            }),
            trial_run: Ok(0.5),
        }
    }

    /// `loaded`, with the devices step's outcome `devices`.
    fn with_devices(
        loaded: LoadedSetup,
        devices: Result<DeviceReport, CudaAlgebraError>,
    ) -> LoadedSetup {
        LoadedSetup { devices, ..loaded }
    }

    /// `loaded`, with the trial run's outcome `trial_run`.
    fn with_trial_run(
        loaded: LoadedSetup,
        trial_run: Result<f64, CanonicalMpsError<CudaAlgebraError>>,
    ) -> LoadedSetup {
        LoadedSetup {
            trial_run,
            ..loaded
        }
    }

    /// The setup guides discovery gives the check, one per library it opens,
    /// in the order opened. Relative overrides make discovery open every
    /// library without reading a file.
    fn guides() -> Vec<SetupGuide> {
        let relative = |_: &'static str| Some(OsString::from("relative.so"));
        crate::discovery::discover_from(relative, &PackagedLibraries::default()).guides
    }

    /// The setup of a host whose libraries loaded and whose steps found
    /// `loaded`, checked with no packager's install command.
    fn setup(loaded: LoadedSetup) -> GpuSetup {
        checked(Ok(loaded), guides())
    }

    /// The setup of a host where discovery found `report`, the process sees
    /// `engine_device` alone, if any, and the trial run succeeded.
    fn found(report: AvailabilityReport, engine_device: Option<EngineDevice>) -> GpuSetup {
        setup(loaded(report, engine_device))
    }

    /// The setup of a host where discovery failed with `error`, checked with no
    /// packager's install command.
    fn failed(error: AvailabilityError) -> GpuSetup {
        checked(Err(error), guides())
    }

    /// `setup`, checked with the packager's install command `install_command`,
    /// if any.
    fn with_install_command(setup: GpuSetup, install_command: Option<&str>) -> GpuSetup {
        GpuSetup {
            install_command: install_command.map(str::to_string),
            ..setup
        }
    }

    /// `setup`, with the oldest compute capability each library runs on
    /// replaced by `floors`: the CUDA Runtime's, then cuTensorNet's.
    fn with_compute_capability_floors(
        mut setup: GpuSetup,
        floors: [Option<(i32, i32)>; 2],
    ) -> GpuSetup {
        for (guide, floor) in setup.guides.iter_mut().zip(floors) {
            guide.minimum_compute_capability = floor;
        }
        setup
    }

    /// `setup`, with the oldest driver each library runs on replaced by
    /// `floors`: the CUDA Runtime's, then cuTensorNet's.
    fn with_driver_floors(mut setup: GpuSetup, floors: [Option<DriverFloor>; 2]) -> GpuSetup {
        for (guide, floor) in setup.guides.iter_mut().zip(floors) {
            guide.minimum_driver = floor;
        }
        setup
    }

    /// The error of a device count that failed: the driver is too old for the
    /// CUDA Runtime.
    fn unreadable_devices() -> CudaAlgebraError {
        CudaAlgebraError::NativeCallFailed {
            component: "CUDA Runtime",
            operation: "cudaGetDeviceCount",
            status: 35,
            message: "CUDA driver version is insufficient for CUDA runtime version".to_string(),
        }
    }

    /// A trial run that failed with an error the earlier steps cannot foresee:
    /// the device ran out of memory.
    fn failed_trial_run() -> Result<f64, CanonicalMpsError<CudaAlgebraError>> {
        Err(CanonicalMpsError::Algebra(
            CudaAlgebraError::NativeCallFailed {
                component: "CUDA Runtime",
                operation: "cudaMalloc",
                status: 2,
                message: "out of memory".to_string(),
            },
        ))
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

    /// A library found nowhere is advised the install of every library, at its
    /// audited version, or the override of that library: each library's own
    /// variable.
    #[test]
    fn a_missing_library_is_advised_the_install_or_its_override() {
        for (library, variable) in [
            ("cuTensorNet", "QDK_CUTENSORNET_LIBRARY"),
            ("CUDA Runtime", "QDK_CUDART_LIBRARY"),
        ] {
            let advice = advice(&failed(unusable(
                library,
                vec![LibraryFault::NotFound {
                    attempted: vec![PathBuf::from("/usr/lib/missing.so")],
                }],
            )));
            assert!(
                advises_install(&advice) && advice.contains(variable),
                "{advice}"
            );
        }
    }

    /// A library whose packaged file sits behind a directory the process may
    /// not search is advised to let the process search it, or the override of
    /// that library to a copy it can read; not the install, which does not
    /// change what the process may search.
    #[test]
    fn a_path_the_process_cannot_access_is_advised_its_permissions_or_the_override() {
        for (library, variable, file) in [
            (
                "cuTensorNet",
                "QDK_CUTENSORNET_LIBRARY",
                "/env/cuquantum/lib/libcutensornet.so.2",
            ),
            (
                "CUDA Runtime",
                "QDK_CUDART_LIBRARY",
                "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
            ),
        ] {
            let advice = advice(&failed(unusable(
                library,
                vec![LibraryFault::InaccessiblePath {
                    path: PathBuf::from(file),
                    message: "Permission denied (os error 13)".to_string(),
                }],
            )));
            assert!(
                advice.contains(&format!("{file:?}"))
                    && advice.contains(variable)
                    && advice.contains(library)
                    && !advises_install(&advice),
                "{advice}"
            );
        }
    }

    /// A library that does not load, or is not the audited version, is
    /// advised the install of the audited libraries, wherever the copy at
    /// fault was found but its override.
    #[test]
    fn a_library_that_fails_to_load_or_is_not_audited_is_advised_the_install() {
        for fault in copy_faults() {
            for source in [
                Some(LibrarySource::Packaged),
                Some(LibrarySource::DefaultPath),
                Some(LibrarySource::SystemSearch),
            ] {
                let AvailabilityError::Unusable(mut report) =
                    unusable("cuTensorNet", vec![fault.clone()])
                else {
                    unreachable!("unusable gives a report");
                };
                report.libraries[1].source = source;
                let advice = advice(&failed(AvailabilityError::Unusable(report)));
                assert!(
                    advises_install(&advice),
                    "{fault:?} of {source:?}: {advice}"
                );
            }
        }
    }

    /// When the copy that does not load, or is not the audited version, is
    /// the one its override names, which no install displaces, the override is
    /// advised instead, without blaming a copy that did not load, which may
    /// only miss a library it needs.
    #[test]
    fn an_overridden_copy_that_fails_to_load_or_is_not_audited_is_advised_its_override() {
        let at_fault = ("at fault", "did not load");
        let whys = [("did not load", "at fault"), at_fault, at_fault];
        for (fault, (why, not_why)) in copy_faults().into_iter().zip(whys) {
            let AvailabilityError::Unusable(mut report) =
                unusable("cuTensorNet", vec![fault.clone()])
            else {
                unreachable!("unusable gives a report");
            };
            report.libraries[1].source = Some(LibrarySource::Override);
            let advice = advice(&failed(AvailabilityError::Unusable(report)));
            assert!(
                advice.contains("QDK_CUTENSORNET_LIBRARY")
                    && advice.contains(why)
                    && !advice.contains(not_why)
                    && !advises_install(&advice),
                "{fault:?}: {advice}"
            );
        }
    }

    /// The faults of cuTensorNet's copy itself: it does not load, it is of
    /// another version, it lacks a function.
    fn copy_faults() -> [LibraryFault; 3] {
        [
            LibraryFault::LoadFailed {
                path: PathBuf::from("/usr/lib/libcutensornet.so.2"),
                message: "libcutensor.so.2: cannot open shared object file".to_string(),
            },
            LibraryFault::UnsupportedVersion {
                component: "cuTensorNet",
                found: 21_400,
                supported: 21_300,
            },
            LibraryFault::MissingRequiredSymbol {
                symbol: "cutensornetCreate",
            },
        ]
    }

    /// A library found but not loaded usually misses a library it needs, so
    /// its install is advised with the dependencies: a packager's command
    /// brings them, and an install without one must include them.
    #[test]
    fn a_library_that_fails_to_load_is_advised_its_dependencies() {
        for install_command in [Some(PIP_INSTALL), None] {
            let advice = advice(&with_install_command(
                failed(unusable(
                    "cuTensorNet",
                    vec![LibraryFault::LoadFailed {
                        path: PathBuf::from("/usr/lib/libcutensornet.so.2"),
                        message: "libcutensor.so.2: cannot open shared object file".to_string(),
                    }],
                )),
                install_command,
            ));
            assert!(
                advice.contains("dependencies")
                    && install_command.is_none_or(|command| advice.contains(command))
                    && advises_restart(&advice),
                "{advice}"
            );
        }
    }

    /// An override that is not an absolute path to a file is advised to be
    /// set right, or unset, naming the variable.
    #[test]
    fn an_invalid_override_is_advised_by_its_variable() {
        let advice = advice(&failed(unusable(
            "CUDA Runtime",
            vec![LibraryFault::InvalidOverride {
                variable: "QDK_CUDART_LIBRARY",
                path: PathBuf::from("libcudart.so.12"),
                reason: "path must be absolute",
            }],
        )));
        assert!(advice.contains("QDK_CUDART_LIBRARY"), "{advice}");
    }

    /// The advice is about the first problem met, in the order of the steps,
    /// and within the libraries step, an invalid override first, then the
    /// first fault of the first library at fault: a later problem is often
    /// the earlier one's consequence (a driver too old for the CUDA Runtime
    /// sees no device; no device fails the trial run), and the user fixes one
    /// thing at a time.
    ///
    /// Each row is a scene, a word of the advice about the problem met first,
    /// and one of the advice about the problem after it.
    #[test]
    fn the_first_problem_in_step_order_is_advised() {
        let not_found = |library: &str| LibraryFault::NotFound {
            attempted: vec![PathBuf::from(library)],
        };
        let mut mistyped_override = audited_report();
        mistyped_override.libraries[0].faults = vec![not_found("libcudart.so.12")];
        mistyped_override.libraries[1].faults = vec![LibraryFault::InvalidOverride {
            variable: "QDK_CUTENSORNET_LIBRARY",
            path: PathBuf::from("libcutensornet.so.2"),
            reason: "path must be absolute",
        }];
        let mut both_at_fault = audited_report();
        both_at_fault.libraries[0].faults = vec![LibraryFault::LoadFailed {
            path: PathBuf::from("/usr/lib/libcudart.so.12"),
            message: "wrong ELF class".to_string(),
        }];
        both_at_fault.libraries[1].faults = vec![not_found("libcutensornet.so.2")];
        let other_release = unusable(
            "CUDA Runtime",
            vec![
                LibraryFault::MissingRequiredSymbol {
                    symbol: "cudaMemGetInfo",
                },
                LibraryFault::VersionProbeFailed {
                    component: "CUDA driver",
                    status: 35,
                    message: "CUDA driver version is insufficient".to_string(),
                },
            ],
        );
        let old = EngineDevice {
            compute_capability: (7, 0),
            ..a100()
        };
        let rows = [
            (
                "a mistyped cuTensorNet override, then no CUDA Runtime",
                failed(AvailabilityError::Unusable(mistyped_override)),
                "QDK_CUTENSORNET_LIBRARY",
                "QDK_CUDART_LIBRARY",
            ),
            (
                "a CUDA Runtime that did not load, then no cuTensorNet",
                failed(AvailabilityError::Unusable(both_at_fault)),
                "did not load",
                "QDK_CUTENSORNET_LIBRARY",
            ),
            (
                "a missing function, then a failed version probe",
                failed(other_release),
                "cuTensorNet 2.13",
                "nvidia-smi",
            ),
            (
                "a driver for CUDA 11.8, then no device",
                found(with_driver(11_080), None),
                "11.8",
                "nvidia-smi",
            ),
            (
                "no device, then a failed trial run",
                setup(with_trial_run(
                    loaded(audited_report(), None),
                    failed_trial_run(),
                )),
                "nvidia-smi",
                "cudaMalloc",
            ),
            (
                "a device of compute capability 7.0, then a failed trial run",
                setup(with_trial_run(
                    loaded(audited_report(), Some(old)),
                    failed_trial_run(),
                )),
                "7.0",
                "cudaMalloc",
            ),
        ];
        for (scene, setup, first, later) in rows {
            let advice = advice(&setup);
            assert!(
                advice.contains(first) && !advice.contains(later),
                "{scene}: {advice}"
            );
        }
    }

    /// A CUDA Runtime that cannot report its version (status 35: the driver is
    /// older than the runtime) points at the driver, its floor and
    /// `nvidia-smi`, not at the install, nor at the override that named it, as
    /// a failed version probe is not taken as the copy's.
    #[test]
    fn a_failed_cuda_version_probe_points_at_the_driver() {
        for source in [Some(LibrarySource::Override), Some(LibrarySource::Packaged)] {
            let AvailabilityError::Unusable(mut report) = unusable(
                "CUDA Runtime",
                vec![LibraryFault::VersionProbeFailed {
                    component: "CUDA driver",
                    status: 35,
                    message: "CUDA driver version is insufficient".to_string(),
                }],
            ) else {
                unreachable!("unusable gives a report");
            };
            report.libraries[0].source = source;
            let advice = advice(&failed(AvailabilityError::Unusable(report)));
            assert!(
                advice.contains("nvidia-smi")
                    && advice.contains("525.60.13")
                    && !advises_install(&advice)
                    && !advice.contains("QDK_CUDART_LIBRARY"),
                "{advice}"
            );
        }
    }

    /// A user who installs or upgrades a library is told to restart the
    /// process (in a notebook, the kernel), since a library already loaded in
    /// it stays loaded and a rerun would refuse the old copy again; the advice
    /// that installs nothing (an override, a directory to search) does not.
    /// Each row is a fault of cuTensorNet, and whether its advice installs.
    #[test]
    fn the_advice_to_install_says_to_restart_the_process() {
        let path = || PathBuf::from("/usr/lib/libcutensornet.so.2");
        let rows = [
            (
                LibraryFault::NotFound {
                    attempted: vec![path()],
                },
                true,
            ),
            (
                LibraryFault::LoadFailed {
                    path: path(),
                    message: "libcutensor.so.2: cannot open shared object file".to_string(),
                },
                true,
            ),
            (
                LibraryFault::UnsupportedVersion {
                    component: "cuTensorNet",
                    found: 21_400,
                    supported: 21_300,
                },
                true,
            ),
            (
                LibraryFault::MissingRequiredSymbol {
                    symbol: "cutensornetCreate",
                },
                true,
            ),
            (
                LibraryFault::VersionProbeFailed {
                    component: "cuTensorNet",
                    status: 1,
                    message: "not initialized".to_string(),
                },
                true,
            ),
            (
                LibraryFault::InvalidOverride {
                    variable: "QDK_CUTENSORNET_LIBRARY",
                    path: PathBuf::from("libcutensornet.so.2"),
                    reason: "path must be absolute",
                },
                false,
            ),
            (
                LibraryFault::InaccessiblePath {
                    path: path(),
                    message: "Permission denied (os error 13)".to_string(),
                },
                false,
            ),
        ];
        for (fault, installs) in rows {
            let advice = advice(&failed(unusable("cuTensorNet", vec![fault.clone()])));
            assert_eq!(advises_install(&advice), installs, "{fault:?}: {advice}");
            assert_eq!(advises_restart(&advice), installs, "{fault:?}: {advice}");
        }
    }

    /// A library that gives no advice for a failed version probe, as
    /// cuTensorNet, whose version calls cannot fail, is advised the install
    /// should one fail anyway.
    #[test]
    fn a_library_without_probe_advice_is_advised_the_install_when_its_probe_fails() {
        let advice = advice(&failed(unusable(
            "cuTensorNet",
            vec![LibraryFault::VersionProbeFailed {
                component: "cuTensorNet",
                status: 1,
                message: "not initialized".to_string(),
            }],
        )));
        assert!(advises_install(&advice), "{advice}");
    }

    /// Libraries and a device that passed their checks but a trial run that
    /// fails are advised with the run's error, which those checks did not
    /// foresee, rather than the driver check they already passed.
    #[test]
    fn a_failed_trial_run_is_advised_with_its_error() {
        let error = failed_trial_run().expect_err("the trial run failed");
        let advice = advice(&setup(with_trial_run(
            loaded(audited_report(), Some(a100())),
            failed_trial_run(),
        )));
        assert!(
            shows_with_its_cause(&advice, &error)
                && !advice.contains("nvidia-smi")
                && !advises_install(&advice),
            "{advice}"
        );
    }

    /// A trial run that finds no device, as the devices step would have, gets
    /// that step's advice: the same as a host whose process sees no device.
    #[test]
    fn a_trial_run_that_finds_no_device_is_advised_as_no_device() {
        let trial_run_advice = advice(&setup(with_trial_run(
            loaded(audited_report(), Some(a100())),
            Err(CanonicalMpsError::Algebra(CudaAlgebraError::NoDevice)),
        )));
        assert_eq!(
            trial_run_advice,
            advice(&found(audited_report(), None)),
            "the advice of a host with no device"
        );
    }

    /// Libraries or a device that compute wrong results pass every other
    /// check: a trial run that reads a probability of One other than 1/2,
    /// beyond rounding, leaves the setup unready and is advised a reinstall of
    /// the libraries, naming the value read. Each row is the value read, and
    /// whether it is within rounding of 1/2.
    #[test]
    fn a_trial_run_that_reads_a_wrong_probability_is_advised_a_reinstall() {
        for (probability, right) in [
            (0.5 + 1e-9, true),
            (0.5 - 1e-9, true),
            (0.5 + 1e-3, false),
            (0.73, false),
            (0.0, false),
            (f64::NAN, false),
        ] {
            let setup = setup(with_trial_run(
                loaded(audited_report(), Some(a100())),
                Ok(probability),
            ));
            if right {
                assert!(setup.is_ready(), "{probability}: {setup}");
                continue;
            }
            let advice = advice(&setup);
            assert!(
                advice.contains(&probability.to_string())
                    && advises_install(&advice)
                    && advises_restart(&advice),
                "{advice}"
            );
        }
    }

    /// The trial run, on an engine whose libraries compute right (the fake
    /// libraries, which simulate cuTensorNet and the CUDA Runtime on the CPU),
    /// reads a probability the check takes for right: a check that took its
    /// own run for wrong would refuse every sound host.
    #[test]
    fn a_trial_run_on_an_engine_that_computes_right_is_ready() {
        let libraries: Arc<dyn NativeCalls> = Arc::new(FakeLibraries::new());
        let algebra = CudaAlgebra::with_native_calls(libraries)
            .unwrap_or_else(|error| panic!("the fake libraries open an algebra: {error}"));
        let engine = CudaMpsEngine::new(algebra);
        let setup = setup(with_trial_run(
            loaded(audited_report(), Some(a100())),
            run_trial(&engine),
        ));
        assert!(setup.is_ready(), "{setup}");
    }

    /// A failed discovery reports what it found of each library, the usable
    /// one's path and versions and those a library at fault reported, beside
    /// each fault (a library not found, its fault alone), and that no device
    /// was read and no trial run tried.
    #[test]
    fn a_failed_discovery_reports_what_it_found_its_faults_and_no_trial_run() {
        let not_found = LibraryFault::NotFound {
            attempted: vec![PathBuf::from("libcutensornet.so.2")],
        };
        let old_version = LibraryFault::UnsupportedVersion {
            component: "CUDA Runtime",
            found: 12_080,
            supported: 12_090,
        };
        let AvailabilityError::Unusable(mut missing_cutensornet) =
            unusable("cuTensorNet", vec![not_found.clone()])
        else {
            unreachable!("unusable gives a report");
        };
        missing_cutensornet.libraries[1].path = None;
        missing_cutensornet.libraries[1].versions.clear();
        let AvailabilityError::Unusable(mut old_cuda_runtime) =
            unusable("CUDA Runtime", vec![old_version.clone()])
        else {
            unreachable!("unusable gives a report");
        };
        old_cuda_runtime.libraries[0].path = Some(PathBuf::from("/opt/cuda-12.8/libcudart.so.12"));
        old_cuda_runtime.libraries[0].versions[0].value = 12_080;
        let (not_found, old_version) = (not_found.to_string(), old_version.to_string());
        for (error, expected, absent) in [
            (
                AvailabilityError::Unusable(missing_cutensornet),
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
                AvailabilityError::Unusable(old_cuda_runtime),
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
            let report = failed(error).to_string();
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

    /// NVIDIA supports cuTensorNet on compute capability 7.5 and later: a
    /// GPU engine's device of 6.1, 7.0 or 7.4 is a problem naming its
    /// capability and the minimum, and one of 7.5, 8.0 or 9.0 is ready.
    #[test]
    fn cutensornet_runs_from_compute_capability_7_5_on() {
        for (compute_capability, supported) in [
            ((6, 1), false),
            ((7, 0), false),
            ((7, 4), false),
            ((7, 5), true),
            ((8, 0), true),
            ((9, 0), true),
        ] {
            let device = EngineDevice {
                compute_capability,
                ..a100()
            };
            let setup = found(audited_report(), Some(device));
            assert_eq!(setup.is_ready(), supported, "{compute_capability:?}");
            if !supported {
                let (major, minor) = compute_capability;
                let advice = advice(&setup);
                assert!(
                    advice.contains(&format!("{major}.{minor}"))
                        && advice.contains("cuTensorNet")
                        && advice.contains("7.5"),
                    "{advice}"
                );
            }
        }
    }

    /// Each library declares, in its setup guide, the oldest compute
    /// capability it runs on, and the GPU engine's device must meet the most
    /// demanding, which the advice names with its library: a library bound
    /// later adds its floor without changing the check. Each row gives the
    /// floors of the CUDA Runtime and cuTensorNet, the device's compute
    /// capability, and the library and floor advised, if any.
    #[test]
    fn the_device_must_meet_the_most_demanding_compute_capability_floor() {
        let rows = [
            (
                [Some((8, 0)), Some((7, 5))],
                (7, 5),
                Some(("CUDA Runtime", "8.0")),
            ),
            ([Some((8, 0)), Some((7, 5))], (8, 0), None),
            ([None, Some((7, 5))], (7, 4), Some(("cuTensorNet", "7.5"))),
            ([None, None], (6, 1), None),
        ];
        for (floors, compute_capability, advised) in rows {
            let device = EngineDevice {
                compute_capability,
                ..a100()
            };
            let setup =
                with_compute_capability_floors(found(audited_report(), Some(device)), floors);
            let Some((library, floor)) = advised else {
                assert!(
                    setup.is_ready(),
                    "{floors:?}, {compute_capability:?}: {setup}"
                );
                continue;
            };
            let advice = advice(&setup);
            assert!(
                advice.contains(library) && advice.contains(floor),
                "{floors:?}: {advice}"
            );
        }
    }

    /// Each library declares, in its setup guide, the oldest driver it runs
    /// on, and a host where no device is found is advised the most demanding,
    /// here a cuTensorNet bound later that needs a driver for CUDA 12.4, above
    /// the CUDA Runtime's 12.0; with no floor, the advice is to check that
    /// `nvidia-smi` lists a GPU, and no release. Each row gives the floors of
    /// the CUDA Runtime and cuTensorNet, the release advised, if any, and one
    /// not advised.
    #[test]
    fn no_device_is_advised_the_most_demanding_driver_floor() {
        let cuda_12_0 = DriverFloor {
            cuda: 12_000,
            release: "525.60.13",
        };
        let cuda_12_4 = DriverFloor {
            cuda: 12_040,
            release: "550.54.14",
        };
        let rows = [
            (
                [Some(cuda_12_0), Some(cuda_12_4)],
                Some("550.54.14"),
                "525.60.13",
            ),
            (
                [Some(cuda_12_4), Some(cuda_12_0)],
                Some("550.54.14"),
                "525.60.13",
            ),
            ([None, None], None, "or later"),
        ];
        for (floors, advised, not_advised) in rows {
            let setup = with_driver_floors(found(audited_report(), None), floors);
            let advice = advice(&setup);
            assert!(
                advice.contains("nvidia-smi")
                    && advised.is_none_or(|release| advice.contains(release))
                    && !advice.contains(not_advised),
                "{floors:?}: {advice}"
            );
        }
    }

    /// A devices step that cannot read the devices points at `nvidia-smi`,
    /// which shows whether the driver sees a GPU, as a step that finds no
    /// device does.
    #[test]
    fn unreadable_devices_point_at_nvidia_smi() {
        let advice = advice(&setup(with_devices(
            loaded(audited_report(), Some(a100())),
            Err(unreadable_devices()),
        )));
        assert!(advice.contains("nvidia-smi"), "{advice}");
    }

    /// A driver below the CUDA Runtime's floor, CUDA 12.0, is a problem
    /// naming the floor's driver release and the CUDA the driver supports, or
    /// that there is no driver (CUDA reports version 0), never CUDA 0.0; a
    /// driver at the floor is not. Each row is the CUDA the driver supports,
    /// and what the advice shows of it when it is a problem.
    #[test]
    fn a_driver_below_the_cuda_runtimes_floor_is_a_problem_naming_the_floor() {
        for (driver, shown) in [
            (11_080, Some("11.8")),
            (0, Some("no NVIDIA driver")),
            (12_000, None),
        ] {
            let setup = found(with_driver(driver), Some(a100()));
            let Some(shown) = shown else {
                assert!(setup.is_ready(), "{setup}");
                continue;
            };
            let advice = advice(&setup);
            assert!(
                advice.contains("525.60.13") && advice.contains(shown) && !advice.contains("0.0"),
                "{advice}"
            );
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

    /// Devices that could not be read are shown with the CUDA Runtime's error
    /// on the devices' line.
    #[test]
    fn unreadable_devices_are_shown_with_their_error() {
        let report = setup(with_devices(
            loaded(audited_report(), Some(a100())),
            Err(unreadable_devices()),
        ))
        .to_string();
        let line = line(&report, "CUDA devices:");
        assert!(line.contains(&unreadable_devices().to_string()), "{line:?}");
    }

    /// A failed trial run is shown on its line with its error and the error's
    /// cause: the engine's error says only that its algebra failed, and the
    /// algebra's says why.
    #[test]
    fn a_failed_trial_run_is_shown_with_its_error() {
        let error = failed_trial_run().expect_err("the trial run failed");
        let report = setup(with_trial_run(
            loaded(audited_report(), Some(a100())),
            failed_trial_run(),
        ))
        .to_string();
        let line = line(&report, "trial run on CUDA device 0:");
        assert!(shows_with_its_cause(line, &error), "{line:?}");
    }

    /// The report of a user whose cuTensorNet override is loaded beside a
    /// wheel's copy, and whose CUDA Runtime came from the usual paths beside a
    /// system copy.
    fn two_other_copies() -> AvailabilityReport {
        let mut report = audited_report();
        report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
        report.libraries[1].path = Some(PathBuf::from("/opt/override/libcutensornet.so.2"));
        report.libraries[1].source = Some(LibrarySource::Override);
        report.libraries[1].other_copies =
            vec![PathBuf::from("/env/cuquantum/lib/libcutensornet.so.2")];
        report
    }

    /// Another copy of a library beside the one loaded is a warning naming both
    /// copies, shown in the report, and leaves the setup ready.
    #[test]
    fn another_copy_of_a_library_is_a_warning_that_leaves_the_setup_ready() {
        let setup = found(two_other_copies(), Some(a100()));
        assert!(setup.is_ready());
        let warnings = setup.warnings();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        let cutensornet = warnings
            .iter()
            .find(|warning| warning.contains("/env/cuquantum/lib/libcutensornet.so.2"))
            .expect("a warning names cuTensorNet's other copy");
        assert!(
            cutensornet.contains("/opt/override/libcutensornet.so.2"),
            "{cutensornet}"
        );
        assert!(
            setup
                .to_string()
                .contains(&format!("warning: {cutensornet}")),
            "{setup}"
        );
    }

    /// A path that holds a newline (a directory named so, by mistake or not)
    /// is shown escaped, on its item's line, so it adds no line to the report,
    /// and cannot pass for the fix: where each library was loaded from, the
    /// other copy a warning names, and the path whose directories the fix says
    /// to make searchable.
    #[test]
    fn a_path_with_a_newline_adds_no_line_to_the_report() {
        let loaded = |name: &str| {
            let mut report = two_other_copies();
            for library in &mut report.libraries {
                let path = library.path.take().expect("the libraries were loaded");
                library.path = Some(path.with_file_name(name).join("library.so"));
            }
            report.libraries[1].other_copies =
                vec![PathBuf::from(format!("/env/{name}/library.so"))];
            found(report, Some(a100()))
        };
        let inaccessible = |name: &str| {
            failed(unusable(
                "CUDA Runtime",
                vec![LibraryFault::InaccessiblePath {
                    path: PathBuf::from(format!("/env/{name}/libcudart.so.12")),
                    message: "Permission denied (os error 13)".to_string(),
                }],
            ))
        };
        let scenes: [&dyn Fn(&str) -> GpuSetup; 2] = [&loaded, &inaccessible];
        for scene in scenes {
            let lines = |name: &str| scene(name).to_string().lines().count();
            assert_eq!(lines("plain"), lines("plain\nto fix: something else"));
        }
    }

    /// A warning's remedy changes the copy at fault, never a sound one: when
    /// the CUDA Runtime loaded, from its override or not, is of another
    /// release, the user is told to set the override to an audited copy, or
    /// else to unset the override or remove the copy loaded; when the one
    /// loaded passed its checks, or only a version probe failed, the other
    /// copy, never checked, is to be removed, and the override kept.
    #[test]
    fn a_warning_says_to_change_the_copy_loaded_only_when_it_is_at_fault() {
        let at_fault = vec![LibraryFault::UnsupportedVersion {
            component: "CUDA Runtime",
            found: 12_080,
            supported: 12_090,
        }];
        let driver_at_fault = vec![LibraryFault::VersionProbeFailed {
            component: "CUDA driver",
            status: 35,
            message: "CUDA driver version is insufficient".to_string(),
        }];
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
            let mut report = audited_report();
            report.libraries[0].source = Some(source);
            report.libraries[0].other_copies =
                vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
            let setup = if faults.is_empty() {
                found(report, Some(a100()))
            } else {
                report.libraries[0].faults = faults;
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

    /// A user whose CUDA Runtime override loads a copy of another release,
    /// beside the audited copy a package installed, is warned of that copy
    /// even though discovery failed: a stray copy is a likely cause of a
    /// failure.
    #[test]
    fn another_copy_is_warned_of_when_discovery_fails() {
        let AvailabilityError::Unusable(mut report) = unusable(
            "CUDA Runtime",
            vec![LibraryFault::UnsupportedVersion {
                component: "CUDA Runtime",
                found: 12_080,
                supported: 12_090,
            }],
        ) else {
            unreachable!("unusable gives a report");
        };
        report.libraries[0].path = Some(PathBuf::from("/opt/cuda-12.8/libcudart.so.12"));
        report.libraries[0].source = Some(LibrarySource::Override);
        report.libraries[0].other_copies = vec![PathBuf::from(
            "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
        )];
        let setup = failed(AvailabilityError::Unusable(report));
        let warnings = setup.warnings();
        let [warning] = warnings.as_slice() else {
            panic!("one warning for the other copy: {warnings:?}");
        };
        assert!(
            warning.contains("/env/nvidia/cuda_runtime/lib/libcudart.so.12"),
            "{warning}"
        );
        assert!(
            setup.to_string().contains(&format!("warning: {warning}")),
            "{setup}"
        );
    }

    /// A user whose CUDA device 0 is too old, with another copy of the CUDA
    /// Runtime visible, reads what was found first, then the warning, then
    /// the fix last.
    #[test]
    fn the_report_lists_what_was_found_then_the_warnings_then_the_fix() {
        let mut report = audited_report();
        report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
        let old = EngineDevice {
            compute_capability: (7, 0),
            ..a100()
        };
        let text = found(report, Some(old)).to_string();
        let position = |line: &str| {
            text.find(line)
                .unwrap_or_else(|| panic!("{line:?} in {text}"))
        };
        let found_last = position("\ntrial run on CUDA device 0:");
        let warning = position("\nwarning: ");
        let fix = position("\nto fix: ");
        assert!(found_last < warning && warning < fix, "{text}");
    }

    /// The install advised gives the caller's command, and only it: a user
    /// who installed through a packager, here Python's, is advised the
    /// packager's own command, so they can run it as is; a Rust caller that
    /// gives none is advised what to install, the libraries and their
    /// versions, with no command it did not give.
    #[test]
    fn the_install_advice_gives_the_callers_command_only() {
        for install_command in [Some(PIP_INSTALL), None] {
            let advice = advice(&with_install_command(
                failed(unusable(
                    "cuTensorNet",
                    vec![LibraryFault::NotFound {
                        attempted: vec![PathBuf::from("libcutensornet.so.2")],
                    }],
                )),
                install_command,
            ));
            let command_shown = install_command.map_or(!advice.contains('`'), |command| {
                advice.contains(&format!("`{command}`"))
            });
            assert!(advises_install(&advice) && command_shown, "{advice}");
        }
    }

    /// A packager's files and install command reach the check: it reports
    /// what discovery finds given those files, and advises a failure with the
    /// packager's command. Here, with no override set, the packaged CUDA
    /// Runtime, not a library, is the copy tried, and fails to load; with the
    /// default files, it would not be tried at all.
    #[test]
    fn the_check_discovers_with_the_packaged_files() {
        use crate::libraries::cuda_runtime::CudaRuntime;

        let (cudart, _remove) = crate::tests::packaged_file::<CudaRuntime>("check-packaged-cudart");
        let packaged = PackagedLibraries {
            files: vec![cudart.clone()],
            install_command: Some(PIP_INSTALL.to_string()),
        };
        let setup = GpuSetup::check_from(|_| None, &packaged);
        let advice = advice(&setup);
        let report = setup.to_string();
        let at_fault = line(&report, "CUDA Runtime at fault:");
        assert!(
            at_fault.contains("load") && at_fault.contains(&format!("{cudart:?}")),
            "{report}"
        );
        assert!(advice.contains(PIP_INSTALL), "{advice}");
    }
}

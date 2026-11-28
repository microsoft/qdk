// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The advice of [`GpuSetup`](crate::GpuSetup): for each fault of a
//! library, for the device and the driver, and for a failed trial run, the
//! fix that applies to it. On a host that can load libraries; the advice
//! off Linux x86-64 is tested beside the report.

use std::path::PathBuf;

use canonical_mps::CanonicalMpsError;

use super::{
    advice,
    scenes::{
        PIP_INSTALL, a100, audited_report, audited_with, both_unusable, capability, copy_faults,
        device_of, driver_probe_failed, failed, failed_trial_run, found, inaccessible, load_failed,
        loaded, not_found, relative_override, setup, unreadable_devices, unusable,
        with_compute_capability_floors, with_devices, with_driver, with_driver_floors,
        with_install_command, with_trial_run,
    },
    shows_with_its_causes,
};
use crate::{
    AvailabilityError, CudaAlgebraError, LibraryFault, LibrarySource,
    libraries::setup_guide::DriverFloor,
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

/// A library found nowhere is advised the install of every library, at
/// its audited version, or the override of that library. Each library
/// names its own variable.
#[test]
fn a_missing_library_is_advised_the_install_or_its_override() {
    for (library, variable) in [
        ("cuTensorNet", "QDK_CUTENSORNET_LIBRARY"),
        ("CUDA Runtime", "QDK_CUDART_LIBRARY"),
    ] {
        let advice = advice(&failed(unusable(
            library,
            vec![not_found("/usr/lib/missing.so")],
        )));
        assert!(
            advises_install(&advice) && advice.contains(variable),
            "{advice}"
        );
    }
}

/// A library whose packaged file sits behind a directory the process
/// lacks the permission to search, or a link that loops, is advised to
/// let the process reach it, or to point the library's override to a
/// copy it can read. The install is not advised: it does not change what
/// the process reaches.
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
        let advice = advice(&failed(unusable(library, vec![inaccessible(file)])));
        assert!(
            advice.contains(&format!("`{file}`"))
                && advice.contains(variable)
                && advice.contains(library)
                && !advises_install(&advice),
            "{advice}"
        );
    }
}

/// A library whose copy does not load, or is not the audited version, is
/// advised the install of the audited libraries, wherever the copy was
/// found. When its override named the copy, no install displaces it, so
/// the override is advised instead. That advice does not blame a copy
/// that did not load, which can be sound and only miss a library it
/// needs. Each fault of the copy is tried from each source.
#[test]
fn a_copy_at_fault_is_advised_the_install_unless_its_override_named_it() {
    let at_fault = ("at fault", "did not load");
    let whys = [("did not load", "at fault"), at_fault, at_fault];
    for (fault, (why, not_why)) in copy_faults().into_iter().zip(whys) {
        for source in [
            LibrarySource::Override,
            LibrarySource::Packaged,
            LibrarySource::DefaultPath,
            LibrarySource::SystemSearch,
        ] {
            let report = audited_with("cuTensorNet", |cutensornet| {
                cutensornet.faults = vec![fault.clone()];
                cutensornet.source = Some(source);
            });
            let advice = advice(&failed(AvailabilityError::Unusable(report)));
            let advised = if source == LibrarySource::Override {
                advice.contains("QDK_CUTENSORNET_LIBRARY")
                    && advice.contains(why)
                    && !advice.contains(not_why)
                    && !advises_install(&advice)
            } else {
                advises_install(&advice)
            };
            assert!(advised, "{fault:?} of {source:?}: {advice}");
        }
    }
}

/// A library found but not loaded usually misses a library it needs. So
/// its install is advised with the dependencies: a packager's command
/// brings them, and an install without one must include them.
#[test]
fn a_library_that_fails_to_load_is_advised_its_dependencies() {
    for install_command in [Some(PIP_INSTALL), None] {
        let advice = advice(&with_install_command(
            failed(unusable("cuTensorNet", vec![load_failed()])),
            install_command,
        ));
        assert!(advice.contains("dependencies"), "{advice}");
    }
}

/// An override that is not an absolute path to a file is advised to be
/// set right, or unset, naming the variable.
#[test]
fn an_invalid_override_is_advised_by_its_variable() {
    let advice = advice(&failed(unusable(
        "CUDA Runtime",
        vec![relative_override("QDK_CUDART_LIBRARY", "libcudart.so.12")],
    )));
    assert!(advice.contains("QDK_CUDART_LIBRARY"), "{advice}");
}

/// The advice is about the first problem met, in the order of the steps.
/// Within the libraries step, an invalid override comes first, then the
/// first fault of the first library at fault. A later problem is often
/// the earlier one's consequence (a driver too old for the CUDA Runtime
/// sees no device; no device fails the trial run), and the user fixes one
/// thing at a time.
///
/// Each row is a scene, a word of the advice about the problem met first,
/// and one of the advice about the problem after it.
#[test]
fn the_first_problem_in_step_order_is_advised() {
    let mistyped_override = both_unusable(
        not_found("libcudart.so.12"),
        relative_override("QDK_CUTENSORNET_LIBRARY", "libcutensornet.so.2"),
    );
    let cuda_runtime_did_not_load = LibraryFault::LoadFailed {
        path: PathBuf::from("/usr/lib/libcudart.so.12"),
        message: "wrong ELF class".to_string(),
    };
    let both_at_fault = both_unusable(cuda_runtime_did_not_load, not_found("libcutensornet.so.2"));
    let other_release = unusable(
        "CUDA Runtime",
        vec![
            LibraryFault::MissingRequiredSymbol {
                symbol: "cudaMemGetInfo",
            },
            driver_probe_failed(),
        ],
    );
    let rows = [
        (
            "a mistyped cuTensorNet override, then no CUDA Runtime",
            failed(mistyped_override),
            "QDK_CUTENSORNET_LIBRARY",
            "QDK_CUDART_LIBRARY",
        ),
        (
            "a CUDA Runtime that did not load, then no cuTensorNet",
            failed(both_at_fault),
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
                loaded(audited_report(), Some(device_of(capability(7, 0)))),
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
/// `nvidia-smi`. It points neither at the install nor at the override
/// that named it, as a failed version probe is not taken as the copy's.
#[test]
fn a_failed_cuda_version_probe_points_at_the_driver() {
    for source in [Some(LibrarySource::Override), Some(LibrarySource::Packaged)] {
        let report = audited_with("CUDA Runtime", |cuda_runtime| {
            cuda_runtime.faults = vec![driver_probe_failed()];
            cuda_runtime.source = source;
        });
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
/// process (in a notebook, the kernel). A library already loaded in it
/// stays loaded, and a rerun would refuse the old copy again. Advice that
/// installs nothing (an override, a directory to search) gives no
/// restart. A library with no advice for a failed version probe, such as
/// cuTensorNet, whose version calls cannot fail, is advised the install
/// should one fail anyway (`probe_failed`). The faults are cuTensorNet's,
/// first those whose advice installs, then those whose advice does not.
#[test]
fn the_advice_to_install_says_to_restart_the_process() {
    let path = "/usr/lib/libcutensornet.so.2";
    let probe_failed = LibraryFault::VersionProbeFailed {
        component: "cuTensorNet",
        status: 1,
        message: "not initialized".to_string(),
    };
    let installs = [not_found(path)]
        .into_iter()
        .chain(copy_faults())
        .chain([probe_failed]);
    let installs_nothing = [
        relative_override("QDK_CUTENSORNET_LIBRARY", "libcutensornet.so.2"),
        inaccessible(path),
    ];
    let rows = installs
        .map(|fault| (fault, true))
        .chain(installs_nothing.map(|fault| (fault, false)));
    for (fault, installs) in rows {
        let advice = advice(&failed(unusable("cuTensorNet", vec![fault.clone()])));
        assert_eq!(advises_install(&advice), installs, "{fault:?}: {advice}");
        assert_eq!(advises_restart(&advice), installs, "{fault:?}: {advice}");
    }
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
        shows_with_its_causes(&advice, &error)
            && !advice.contains("nvidia-smi")
            && !advises_install(&advice),
        "{advice}"
    );
}

/// A trial run that finds no device, as the devices step would have, gets
/// that step's advice, the same as a host whose process sees no device.
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
/// check. A trial run that reads a probability of One other than 1/2,
/// beyond the check's margin, leaves the setup unready. It is advised a
/// reinstall of the libraries, naming the value read. Each row is the
/// value read, and whether it is within that margin of 1/2.
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

/// NVIDIA supports cuTensorNet on compute capability 7.5 and later. A
/// GPU engine's device of 6.1, 7.0 or 7.4 is a problem naming its
/// capability and the minimum. One of 7.5, 8.0 or 9.0 is ready.
#[test]
fn cutensornet_runs_from_compute_capability_7_5_on() {
    for (compute_capability, supported) in [
        (capability(6, 1), false),
        (capability(7, 0), false),
        (capability(7, 4), false),
        (capability(7, 5), true),
        (capability(8, 0), true),
        (capability(9, 0), true),
    ] {
        let setup = found(audited_report(), Some(device_of(compute_capability)));
        assert_eq!(setup.is_ready(), supported, "{compute_capability:?}");
        if !supported {
            let advice = advice(&setup);
            assert!(
                advice.contains(&compute_capability.to_string())
                    && advice.contains("cuTensorNet")
                    && advice.contains("7.5"),
                "{advice}"
            );
        }
    }
}

/// Each library declares, in its setup guide, the oldest compute
/// capability it runs on. The GPU engine's device must meet the most
/// demanding, which the advice names with its library. So a library
/// bound later adds its floor without changing the check. Each row gives
/// the floors of the CUDA Runtime and cuTensorNet, the device's compute
/// capability, and the library and floor advised, if any.
#[test]
fn the_device_must_meet_the_most_demanding_compute_capability_floor() {
    let rows = [
        (
            [Some(capability(8, 0)), Some(capability(7, 5))],
            capability(7, 5),
            Some(("CUDA Runtime", "8.0")),
        ),
        (
            [Some(capability(8, 0)), Some(capability(7, 5))],
            capability(8, 0),
            None,
        ),
        (
            [None, Some(capability(7, 5))],
            capability(7, 4),
            Some(("cuTensorNet", "7.5")),
        ),
        ([None, None], capability(6, 1), None),
    ];
    for (floors, compute_capability, advised) in rows {
        let setup = with_compute_capability_floors(
            found(audited_report(), Some(device_of(compute_capability))),
            floors,
        );
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
/// on. A host where no device is found, or where the devices cannot be
/// read, is advised two checks: that `nvidia-smi`, which shows whether
/// the driver sees a GPU, lists one; and the most demanding floor. Here
/// a cuTensorNet bound later needs a driver for CUDA 12.4, above the
/// CUDA Runtime's 12.0. With no floor, no release is advised. Each row
/// gives the floors of the CUDA Runtime and cuTensorNet, the release
/// advised, if any, and one not advised. Each is tried on both hosts.
#[test]
fn no_or_unreadable_devices_are_advised_the_most_demanding_driver_floor() {
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
        let no_device = found(audited_report(), None);
        let unreadable = setup(with_devices(
            loaded(audited_report(), Some(a100())),
            Err(unreadable_devices()),
        ));
        for (host, setup) in [("no device", no_device), ("unreadable", unreadable)] {
            let advice = advice(&with_driver_floors(setup, floors));
            assert!(
                advice.contains("nvidia-smi")
                    && advised.is_none_or(|release| advice.contains(release))
                    && !advice.contains(not_advised),
                "{host}, {floors:?}: {advice}"
            );
        }
    }
}

/// A driver below the CUDA Runtime's floor, CUDA 12.0, is a problem. It
/// names the floor's driver release, and the CUDA the driver supports or
/// that there is no driver (CUDA reports version 0), never CUDA 0.0. A
/// driver at the floor is not a problem. Each row is the CUDA the driver
/// supports, and what the advice shows of it when it is a problem.
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

/// The install advised gives the caller's command, and only it. A user
/// who installed through a packager, here Python's, is advised the
/// packager's own command, so they can run it as is. A Rust caller that
/// gives none is advised what to install, the libraries and their
/// versions, with no command it did not give. Each fault is one whose
/// advice installs: a library not found, and one that did not load.
#[test]
fn the_install_advice_gives_the_callers_command_only() {
    for fault in [not_found("libcutensornet.so.2"), load_failed()] {
        for install_command in [Some(PIP_INSTALL), None] {
            let advice = advice(&with_install_command(
                failed(unusable("cuTensorNet", vec![fault.clone()])),
                install_command,
            ));
            let command_shown = install_command.map_or(!advice.contains('`'), |command| {
                advice.contains(&format!("`{command}`"))
            });
            assert!(
                advises_install(&advice) && command_shown,
                "{fault:?}: {advice}"
            );
        }
    }
}

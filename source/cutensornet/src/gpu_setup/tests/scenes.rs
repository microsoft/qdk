// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The scenes the GPU setup check's tests build: what discovery found or
//! the faults it met, the devices read, the trial run's result and the
//! libraries' setup guides; and an algebra whose gate split ignores its
//! gate.

use std::{borrow::Cow, ffi::OsString, path::PathBuf, sync::Arc};

use canonical_mps::{
    CanonicalMpsError,
    site_algebra::{GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra},
};
use num_complex::Complex64;

use super::checked;
use crate::{
    AvailabilityError, AvailabilityReport, CudaAlgebra, CudaAlgebraError, DeviceTensor, GpuSetup,
    LibraryFault, LibraryReport, LibrarySource, PackagedLibraries, Version,
    engine::native_calls::fake_libraries::FakeLibraries,
    gpu_setup::{
        LoadedSetup,
        device_report::{DeviceReport, EngineDevice},
    },
    libraries::setup_guide::{ComputeCapability, DriverFloor, SetupGuide},
};

/// The compute capability `major.minor`.
pub(super) const fn capability(major: i32, minor: i32) -> ComputeCapability {
    ComputeCapability { major, minor }
}

/// The command a Python packager gives for its install.
pub(super) const PIP_INSTALL: &str = "pip install \"qdk[tensornetwork]\"";

/// The report of the library `name`, usable, loaded from `path`, a
/// packaged file, with `versions`.
pub(super) fn usable(
    name: &'static str,
    path: &str,
    versions: [(&'static str, i128); 2],
) -> LibraryReport {
    LibraryReport {
        name,
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
/// that supports CUDA 12.8. The CUDA Runtime comes first, then
/// cuTensorNet, in the order discovery opens them.
pub(super) fn audited_report() -> AvailabilityReport {
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
pub(super) fn with_driver(driver: i128) -> AvailabilityReport {
    audited_with("CUDA Runtime", |cuda_runtime| {
        cuda_runtime.versions[1].value = driver;
    })
}

/// The audited report, with the report of `library` changed by `edit`:
/// a scene's one difference from the audited libraries.
pub(super) fn audited_with(
    library: &str,
    edit: impl FnOnce(&mut LibraryReport),
) -> AvailabilityReport {
    let mut report = audited_report();
    edit(
        report
            .libraries
            .iter_mut()
            .find(|report| report.name == library)
            .expect("discovery opens the library"),
    );
    report
}

/// The error of a discovery where `library` has `faults` and the other
/// library is usable.
pub(super) fn unusable(library: &str, faults: Vec<LibraryFault>) -> AvailabilityError {
    AvailabilityError::Unusable(audited_with(library, |at_fault| at_fault.faults = faults))
}

/// The error of a discovery where both libraries are at fault: the CUDA
/// Runtime, opened first, with `cuda_runtime`, then cuTensorNet with
/// `cutensornet`.
pub(super) fn both_unusable(
    cuda_runtime: LibraryFault,
    cutensornet: LibraryFault,
) -> AvailabilityError {
    let mut report = audited_report();
    report.libraries[0].faults = vec![cuda_runtime];
    report.libraries[1].faults = vec![cutensornet];
    AvailabilityError::Unusable(report)
}

/// A library found nowhere, where discovery tried `file`.
pub(super) fn not_found(file: &str) -> LibraryFault {
    LibraryFault::NotFound {
        attempted: vec![PathBuf::from(file)],
    }
}

/// A library at `path`, which the process lacks the permission to reach.
pub(super) fn inaccessible(path: &str) -> LibraryFault {
    LibraryFault::InaccessiblePath {
        path: PathBuf::from(path),
        message: "Permission denied (os error 13)".to_string(),
    }
}

/// The override `variable`, set to the file name `path` rather than an
/// absolute path.
pub(super) fn relative_override(variable: &'static str, path: &str) -> LibraryFault {
    LibraryFault::InvalidOverride {
        variable,
        path: PathBuf::from(path),
        reason: "path must be absolute",
    }
}

/// A CUDA Runtime of release 12.8, where 12.9 is audited.
pub(super) fn cuda_runtime_12_8() -> LibraryFault {
    LibraryFault::UnsupportedVersion {
        component: "CUDA Runtime",
        found: 12_080,
        supported: 12_090,
    }
}

/// The driver's CUDA version not read: status 35, the driver is older
/// than the CUDA Runtime.
pub(super) fn driver_probe_failed() -> LibraryFault {
    LibraryFault::VersionProbeFailed {
        component: "CUDA driver",
        status: 35,
        message: "CUDA driver version is insufficient".to_string(),
    }
}

/// cuTensorNet's copy did not load: it misses a library it needs.
pub(super) fn load_failed() -> LibraryFault {
    LibraryFault::LoadFailed {
        path: PathBuf::from("/usr/lib/libcutensornet.so.2"),
        message: "libcutensor.so.2: cannot open shared object file".to_string(),
    }
}

/// The faults of cuTensorNet's copy itself: it does not load, it is of
/// another version, it lacks a function.
pub(super) fn copy_faults() -> [LibraryFault; 3] {
    [
        load_failed(),
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

/// An A100 with 80 GiB, 79 free: compute capability 8.0.
pub(super) fn a100() -> EngineDevice {
    EngineDevice {
        compute_capability: capability(8, 0),
        free_bytes: 79 << 30,
        total_bytes: 80 << 30,
    }
}

/// An A100's memory on a device of `compute_capability`.
pub(super) fn device_of(compute_capability: ComputeCapability) -> EngineDevice {
    EngineDevice {
        compute_capability,
        ..a100()
    }
}

/// What the steps find on a host where discovery found `report`. The
/// process sees `engine_device` alone, if any, and the trial run read
/// the exact probability.
pub(super) fn loaded(
    report: AvailabilityReport,
    engine_device: Option<EngineDevice>,
) -> LoadedSetup {
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
pub(super) fn with_devices(
    loaded: LoadedSetup,
    devices: Result<DeviceReport, CudaAlgebraError>,
) -> LoadedSetup {
    LoadedSetup { devices, ..loaded }
}

/// `loaded`, with the trial run's outcome `trial_run`.
pub(super) fn with_trial_run(
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
pub(super) fn guides() -> Vec<SetupGuide> {
    let relative = |_: &'static str| Some(OsString::from("relative.so"));
    crate::discovery::discover_from(relative, &PackagedLibraries::default()).guides
}

/// The setup of a host whose libraries loaded and whose steps found
/// `loaded`, checked with no packager's install command.
pub(super) fn setup(loaded: LoadedSetup) -> GpuSetup {
    checked(Ok(loaded), guides())
}

/// The setup of a host where discovery found `report`. The process sees
/// `engine_device` alone, if any, and the trial run succeeded.
pub(super) fn found(report: AvailabilityReport, engine_device: Option<EngineDevice>) -> GpuSetup {
    setup(loaded(report, engine_device))
}

/// The setup of a host where discovery failed with `error`, checked with no
/// packager's install command.
pub(super) fn failed(error: AvailabilityError) -> GpuSetup {
    checked(Err(error), guides())
}

/// `setup`, checked with the packager's install command `install_command`,
/// if any.
pub(super) fn with_install_command(setup: GpuSetup, install_command: Option<&str>) -> GpuSetup {
    GpuSetup {
        install_command: install_command.map(str::to_string),
        ..setup
    }
}

/// The libraries whose floors a scene sets, in the order of its rows'
/// floors.
pub(super) const FLOORED_LIBRARIES: [&str; 2] = ["CUDA Runtime", "cuTensorNet"];

/// The guide of `library` among `setup`'s.
pub(super) fn guide_of<'s>(setup: &'s mut GpuSetup, library: &str) -> &'s mut SetupGuide {
    setup
        .guides
        .iter_mut()
        .find(|guide| guide.library == library)
        .unwrap_or_else(|| panic!("discovery opens {library}"))
}

/// `setup`, with the oldest compute capability each library runs on
/// replaced by `floors`: the CUDA Runtime's, then cuTensorNet's.
pub(super) fn with_compute_capability_floors(
    mut setup: GpuSetup,
    floors: [Option<ComputeCapability>; 2],
) -> GpuSetup {
    for (library, floor) in FLOORED_LIBRARIES.into_iter().zip(floors) {
        guide_of(&mut setup, library).minimum_compute_capability = floor;
    }
    setup
}

/// `setup`, with the oldest driver each library runs on replaced by
/// `floors`: the CUDA Runtime's, then cuTensorNet's.
pub(super) fn with_driver_floors(
    mut setup: GpuSetup,
    floors: [Option<DriverFloor>; 2],
) -> GpuSetup {
    for (library, floor) in FLOORED_LIBRARIES.into_iter().zip(floors) {
        guide_of(&mut setup, library).minimum_driver = floor;
    }
    setup
}

/// The error of a device count that failed: the driver is too old for the
/// CUDA Runtime.
pub(super) fn unreadable_devices() -> CudaAlgebraError {
    CudaAlgebraError::NativeCallFailed {
        component: "CUDA Runtime",
        operation: "cudaGetDeviceCount",
        status: 35,
        message: "CUDA driver version is insufficient for CUDA runtime version".to_string(),
    }
}

/// A trial run that failed with an error the earlier steps cannot foresee:
/// the device ran out of memory.
pub(super) fn failed_trial_run() -> Result<f64, CanonicalMpsError<CudaAlgebraError>> {
    Err(CanonicalMpsError::Algebra(
        CudaAlgebraError::NativeCallFailed {
            component: "CUDA Runtime",
            operation: "cudaMalloc",
            status: 2,
            message: "out of memory".to_string(),
        },
    ))
}

/// An algebra on `libraries`.
pub(super) fn fake_algebra_on(libraries: Arc<FakeLibraries>) -> CudaAlgebra {
    CudaAlgebra::with_native_calls(libraries)
        .unwrap_or_else(|error| panic!("the fake libraries open an algebra: {error}"))
}

/// An algebra on fresh fake libraries.
pub(super) fn fake_algebra() -> CudaAlgebra {
    fake_algebra_on(Arc::new(FakeLibraries::new()))
}

/// An algebra whose gate split applies the identity whatever its gate,
/// as libraries whose split dropped the gate would. Every other call
/// is the inner algebra's.
pub(super) struct GateSplitIgnoringItsGate(pub(super) CudaAlgebra);

impl SiteAlgebra for GateSplitIgnoringItsGate {
    type Tensor = DeviceTensor;
    type Error = CudaAlgebraError;

    fn upload(&self, values: &[Complex64]) -> Result<DeviceTensor, CudaAlgebraError> {
        self.0.upload(values)
    }

    fn read<'t>(
        &self,
        tensor: &'t DeviceTensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, CudaAlgebraError> {
        self.0.read(tensor, len)
    }

    fn duplicate(
        &self,
        tensor: &DeviceTensor,
        len: usize,
    ) -> Result<DeviceTensor, CudaAlgebraError> {
        self.0.duplicate(tensor, len)
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, DeviceTensor>,
    ) -> Result<GateSplitOutcome<DeviceTensor>, CudaAlgebraError> {
        let (zero, one) = (Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0));
        let identity = std::array::from_fn(|i| if i % 5 == 0 { one } else { zero });
        self.0.gate_split(&GateSplitCall {
            gate: identity,
            ..*call
        })
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, DeviceTensor>,
    ) -> Result<QrMoveOutcome<DeviceTensor>, CudaAlgebraError> {
        self.0.qr_move(call)
    }
}

/// The report of a user whose cuTensorNet override is loaded beside a
/// wheel's copy, and whose CUDA Runtime came from the usual paths beside a
/// system copy.
pub(super) fn two_other_copies() -> AvailabilityReport {
    let mut report = audited_report();
    report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
    report.libraries[1].path = Some(PathBuf::from("/opt/override/libcutensornet.so.2"));
    report.libraries[1].source = Some(LibrarySource::Override);
    report.libraries[1].other_copies =
        vec![PathBuf::from("/env/cuquantum/lib/libcutensornet.so.2")];
    report
}

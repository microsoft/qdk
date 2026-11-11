// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The native libraries the crate uses, and what it needs of each.
//!
//! Each library has its own module, which holds everything the crate assumes
//! of it, as its implementation of `NativeLibrary`, so a reader finds a
//! library's settings in one place, with what the setup check tells a user
//! about it (its `SetupGuide`, in `setup_guide`, which `diagnosis` reads).
//! This module holds what the libraries share: the
//! way from one library to the set a GPU run needs.
//!
//! ```text
//! NativeLibrary     one library, as the crate needs it (one module per
//!                   library): name, override variable, default paths,
//!                   FunctionTable (its functions), versions accepted
//!   │ CheckLibrary: open and check it
//!   ▼
//! Checked<L>        its LibraryReport (report.rs) and, with no fault,
//!                   BoundLibrary<L>: the open library with its table
//!   │ Opening: each library in turn, keeping every report
//!   ▼
//! LoadedLibraries   every BoundLibrary a GPU run needs, kept open: the one
//!                   place that lists the libraries
//! ```
//!
//! `open` checks a library from this machine's files: the paths to try (its
//! override variable's, or else its path among the `PackagedLibraries` the
//! caller found, then its default paths), the file loaded and its functions
//! looked up (`loader`), its versions probed, and the other copies of it
//! visible beside the one loaded. `LoadedLibraries` also lists the
//! libraries' setup guides (`SETUP_GUIDES`, checked against what it
//! opens), and `PackagedLibraries` has a field per library.
//! The MPS engine (`engine::native_calls::loaded_libraries`) and the device
//! reads (`devices::native`) call the loaded libraries through their tables;
//! the CUDA Runtime calls both make are its module's.
//!
//! To bind another library:
//!
//! 1. generate its bindings, the Rust declarations of its API, with
//!    `scripts/generate_bindings.py` from the library's header, into the
//!    `generated/` folder of its module's folder (as the CUDA Runtime's
//!    `cudart_12` and cuTensorNet's `v2_13`): a `bindings.toml` in its
//!    module's folder, which says what they are made of, makes the script
//!    bind it; and assert at compile time the layout of each type passed by
//!    value (as cuTensorNet's `layout`);
//! 2. add its module here, whose type implements `NativeLibrary`: its name,
//!    override variable, packaged path and default paths, its table of
//!    functions (`function_table!`), how its versions are read and which
//!    are accepted, asserted at compile time to be the version its header
//!    declares, where the header declares one, and what the setup check
//!    shows of it and advises;
//! 3. add its field to `LoadedLibraries` before the fields of the libraries
//!    it runs on, so that it is closed first, open it in
//!    `LoadedLibraries::open` after them, and add its guide to
//!    `LoadedLibraries::SETUP_GUIDES` in field order; its packaged path
//!    is a field of `PackagedLibraries`;
//! 4. test its table and versions in `tests/libraries/`, and add its
//!    variable, paths and versions to the README.
//!
//! Discovery and the public report, a list with one entry per library, do
//! not change: adding a library adds its packaged path to
//! `PackagedLibraries` and changes no other public type.
//!
//! Only the audited versions are accepted: the bindings were checked against
//! them, and a newer library is not assumed compatible, so any other version
//! fails discovery before a function is called beyond the version probes.

pub(crate) mod cuda_runtime;
pub(crate) mod cutensornet;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod loader;
pub(crate) mod report;
pub(crate) mod setup_guide;

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use loader::{FunctionTable, OpenLibraryFile, SymbolResolver, load_library};
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use report::LibraryFault;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use report::{LibraryReport, Version};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use setup_guide::SetupGuide;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use std::ffi::OsString;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use std::path::Path;
use std::path::PathBuf;

/// The loaded libraries, each with its function table.
///
/// The libraries are closed only when this is dropped, cuTensorNet first,
/// as it runs on the CUDA Runtime; so no function pointer of the tables
/// outlives its library. It holds no device, stream, handle or state:
/// whoever calls a function that creates one owns it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct LoadedLibraries {
    pub(crate) cutensornet: BoundLibrary<cutensornet::CuTensorNet>,
    pub(crate) cuda_runtime: BoundLibrary<cuda_runtime::CudaRuntime>,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl LoadedLibraries {
    /// Opens the libraries a GPU run needs, the CUDA Runtime first, then
    /// cuTensorNet, which runs on it; they are kept loaded only when every
    /// one is usable.
    ///
    /// Every library is opened before any is required, so each is checked
    /// whatever the faults of the others.
    pub(crate) fn open<C: CheckLibrary>(opening: &mut Opening<C>) -> Option<Self> {
        let cuda_runtime = opening.open::<cuda_runtime::CudaRuntime>();
        let cutensornet = opening.open::<cutensornet::CuTensorNet>();
        Some(Self {
            cutensornet: cutensornet?,
            cuda_runtime: cuda_runtime?,
        })
    }

    /// What the setup check tells a user about each library, in field
    /// order: so it lists cuTensorNet, then the CUDA Runtime it runs on.
    pub(crate) const SETUP_GUIDES: [SetupGuide; 2] = [
        SetupGuide::of::<cutensornet::CuTensorNet>(),
        SetupGuide::of::<cuda_runtime::CudaRuntime>(),
    ];
}

/// Where Python packages installed the libraries, for
/// [`discover_with`](crate::discover_with): NVIDIA's wheels
/// (`cutensornet-cu12`, `nvidia-cuda-runtime-cu12`) put them inside a Python
/// environment, where only the QDK's Python package, which reads the
/// environment's installed distributions, can find them. A path is
/// absolute; `None` when the package is not installed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackagedLibraries {
    /// `libcutensornet.so.2` as a package installed it.
    pub cutensornet: Option<PathBuf>,
    /// `libcudart.so.12` as a package installed it.
    pub cuda_runtime: Option<PathBuf>,
}

/// One native library: where it is looked for, the table of its functions,
/// and its versions.
///
/// Each library's module implements it once, so that no step can read one
/// library's variable, try its paths or check its versions for another.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) trait NativeLibrary {
    /// The name its report gives it.
    const NAME: &'static str;
    /// The variable that, when set, names the one path to load.
    const VARIABLE: &'static str;
    /// The paths tried, in order, when the variable is not set.
    const DEFAULTS: &'static [&'static str];
    /// What the setup check advises installing, for instance
    /// `cuTensorNet 2.13`: the library at its audited version.
    const INSTALL: &'static str;
    /// What the setup check advises when a call that reads one of its
    /// versions fails; `None` when no such call can fail, and the check
    /// advises the install.
    const VERSION_PROBE_ADVICE: Option<&'static str> = None;
    /// The table of the functions looked up in it.
    type Functions: FunctionTable;

    /// Its path among the `packaged` libraries, tried before the defaults
    /// when the variable is not set.
    fn packaged(packaged: &PackagedLibraries) -> Option<&Path>;

    /// Reads the library's versions through `resolver` and checks them.
    ///
    /// It looks up the version functions it calls on its own, so that the
    /// versions are read even when other functions are missing: a library
    /// of another version is often what lacks them. A version whose
    /// functions are missing is not read; the table reports them.
    fn probe_versions<R: SymbolResolver>(resolver: &R) -> ProbedVersions;

    /// What the setup check shows of the library once every library is
    /// usable, one line per item: where it was loaded from and its
    /// versions.
    fn describe(report: &LibraryReport) -> Vec<String>;

    /// A problem of the host that the usable library's report reveals, in
    /// words for the user with its fix: none by default.
    fn host_problem(_report: &LibraryReport) -> Option<String> {
        None
    }
}

/// An open library with the table of its functions.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
#[expect(
    dead_code,
    reason = "the library is held, never read, to keep the table valid"
)]
pub(crate) struct BoundLibrary<L: NativeLibrary> {
    pub(crate) functions: L::Functions,
    pub(crate) library: OpenLibraryFile,
}

/// A checked library: its report, with every fault found, and, when it has
/// none, the library bound to its table.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type Checked<L> = (LibraryReport, Option<BoundLibrary<L>>);

/// The libraries being opened, one at a time, by a checker. It keeps the
/// report of each library, in the order they are opened, whatever its
/// faults.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct Opening<C> {
    checker: C,
    reports: Vec<LibraryReport>,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl<C: CheckLibrary> Opening<C> {
    pub(crate) fn new(checker: C) -> Self {
        Self {
            checker,
            reports: Vec::new(),
        }
    }

    /// Opens and checks `L`, keeps its report, and returns it bound to its
    /// table when it has no fault.
    pub(crate) fn open<L: NativeLibrary>(&mut self) -> Option<BoundLibrary<L>> {
        let (report, bound) = self.checker.check::<L>();
        self.reports.push(report);
        bound
    }

    /// The report of each library opened, in order.
    pub(crate) fn into_reports(self) -> Vec<LibraryReport> {
        self.reports
    }
}

/// How each library a GPU run needs is opened and checked. It is a trait so
/// that what discovery concludes from the checks can be tested without
/// library files.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) trait CheckLibrary {
    /// Opens and checks `L`.
    fn check<L: NativeLibrary>(&self) -> Checked<L>;
}

/// The libraries installed on this machine, each opened and checked by
/// `open`, with the override variables read by `read_variable` and the
/// packaged libraries `packaged`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct InstalledLibraries<'a, F> {
    pub(crate) read_variable: F,
    pub(crate) packaged: &'a PackagedLibraries,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl<F: Fn(&'static str) -> Option<OsString>> CheckLibrary for InstalledLibraries<'_, F> {
    fn check<L: NativeLibrary>(&self) -> Checked<L> {
        open::<L>(&self.read_variable, self.packaged)
    }
}

/// Opens `L` and checks it.
///
/// The override comes from `read_variable`, and the packaged path from
/// `packaged`. A fault before the library is open (its override, or no file
/// to load) is its only fault; once it is open, every missing function and
/// every version fault is reported, with the other copies of the library
/// visible beside it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn open<L: NativeLibrary>(
    read_variable: impl Fn(&'static str) -> Option<OsString>,
    packaged: &PackagedLibraries,
) -> Checked<L> {
    let mut report = LibraryReport {
        library: L::NAME,
        path: None,
        versions: Vec::new(),
        faults: Vec::new(),
        other_copies: Vec::new(),
        loaded_from_override: false,
    };
    let library = match candidates::<L>(&read_variable, packaged).and_then(load_library) {
        Ok(library) => library,
        Err(fault) => {
            report.faults.push(fault);
            return (report, None);
        }
    };
    report.path = Some(library.path().to_path_buf());
    // A set override is the only candidate, so a library loaded while it is
    // set was loaded from it.
    report.loaded_from_override = read_variable(L::VARIABLE).is_some();
    report.other_copies = other_copies::<L>(library.path(), &read_variable, packaged);
    let functions = inspect::<L, _>(&library, &mut report);
    let bound = functions.map(|functions| BoundLibrary { functions, library });
    (report, bound)
}

/// Checks an open library through `resolver`: adds its versions and every
/// fault to `report`, and returns its table when the library has no fault.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn inspect<L: NativeLibrary, R: SymbolResolver>(
    resolver: &R,
    report: &mut LibraryReport,
) -> Option<L::Functions> {
    let functions = L::Functions::resolve(resolver)
        .map_err(|missing| report.faults.extend(missing))
        .ok();
    let probed = L::probe_versions(resolver);
    report.versions.extend(probed.versions);
    report.faults.extend(probed.faults);
    functions.filter(|_| report.faults.is_empty())
}

/// The paths to try for `L`: the override that `read_variable` gives for its
/// variable, alone; or else its path in `packaged`, if any, then its default
/// paths.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn candidates<L: NativeLibrary>(
    read_variable: impl Fn(&'static str) -> Option<OsString>,
    packaged: &PackagedLibraries,
) -> Result<Vec<PathBuf>, LibraryFault> {
    Ok(
        match validate_override_path(L::VARIABLE, read_variable(L::VARIABLE))? {
            Some(path) => vec![path],
            None => L::packaged(packaged)
                .into_iter()
                .map(Path::to_path_buf)
                .chain(L::DEFAULTS.iter().map(PathBuf::from))
                .collect(),
        },
    )
}

/// The other copies of `L` the setup makes visible beside `loaded`: the
/// files among its override, its path in `packaged` and its default
/// absolute paths that exist and are not `loaded` (links followed), each
/// once, in that order. A bare file name the system loader searches for is
/// not looked at.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn other_copies<L: NativeLibrary>(
    loaded: &Path,
    read_variable: impl Fn(&'static str) -> Option<OsString>,
    packaged: &PackagedLibraries,
) -> Vec<PathBuf> {
    let loaded = loaded.canonicalize().ok();
    let mut seen = Vec::new();
    let mut copies = Vec::new();
    let paths = read_variable(L::VARIABLE)
        .map(PathBuf::from)
        .into_iter()
        .chain(L::packaged(packaged).map(Path::to_path_buf))
        .chain(L::DEFAULTS.iter().map(PathBuf::from));
    for path in paths {
        if !path.is_absolute() || !path.is_file() {
            continue;
        }
        let Ok(real) = path.canonicalize() else {
            continue;
        };
        if loaded.as_ref() != Some(&real) && !seen.contains(&real) {
            seen.push(real);
            copies.push(path);
        }
    }
    copies
}

/// The path an override variable names, if it is set.
///
/// An override is checked before anything is loaded: it must be an absolute
/// path to a file, or to a symlink to one, so that a mistyped variable fails
/// with its name instead of loading another library.
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn validate_override_path(
    variable: &'static str,
    value: Option<OsString>,
) -> Result<Option<PathBuf>, LibraryFault> {
    let Some(value) = value else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(LibraryFault::InvalidOverride {
            variable,
            path,
            reason: "path must be absolute",
        });
    }
    let metadata = std::fs::metadata(&path).map_err(|_| LibraryFault::InvalidOverride {
        variable,
        path: path.clone(),
        reason: "path does not exist",
    })?;
    if !metadata.is_file() {
        return Err(LibraryFault::InvalidOverride {
            variable,
            path,
            reason: "path must name a file",
        });
    }
    Ok(Some(path))
}

/// What reading a library's versions found: each version read, and each
/// fault.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
#[derive(Debug, Default)]
pub(crate) struct ProbedVersions {
    pub(crate) versions: Vec<Version>,
    pub(crate) faults: Vec<LibraryFault>,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl ProbedVersions {
    /// Records `found`, the version of `component`, which is accepted only if
    /// it is the audited one, `audited`.
    pub(crate) fn audited(&mut self, component: &'static str, found: i128, audited: i128) {
        self.reported(component, found);
        if found != audited {
            self.faults.push(LibraryFault::UnsupportedVersion {
                component,
                found,
                supported: audited,
            });
        }
    }

    /// Records `found`, the version of `component`, for diagnostics only.
    pub(crate) fn reported(&mut self, component: &'static str, found: i128) {
        self.versions.push(Version {
            component,
            value: found,
        });
    }
}

/// The text a library returned for an error, copied, or `null_message`
/// when it returned none: the libraries own the strings their error
/// functions return, so each is copied before anything else is called.
///
/// # Safety
///
/// `pointer` is null, or a null-terminated string that stays valid during
/// the call, as the error functions of the CUDA Runtime and cuTensorNet
/// return.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) unsafe fn copy_message(pointer: *const std::ffi::c_char, null_message: &str) -> String {
    if pointer.is_null() {
        null_message.to_string()
    } else {
        // SAFETY: the caller guarantees a non-null `pointer` is a valid
        // null-terminated string.
        unsafe { std::ffi::CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}

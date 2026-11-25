// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The native libraries the crate uses, and what it needs of each.
//!
//! Each library has its own module, which implements `NativeLibrary` and
//! `LibrarySetupGuide` (in `setup_guide`). That module holds what discovery
//! and the setup check's library steps assume of the library, so a reader
//! finds a library's settings in one place. `NativeLibrary` is what
//! discovery needs of it, and `LibrarySetupGuide` what the setup check
//! tells a user about it. Neither discovery's steps nor the setup check's
//! name a library. So binding another library is adding its module and
//! listing it in `LoadedLibraries` (the steps below). This module holds
//! what the libraries share: the way from one library to the set a GPU run
//! needs.
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
//! `open` checks a library from this machine's files. It takes the paths to
//! try (its override variable's, or else the file of its name among the
//! `PackagedLibraries` the caller found, then its default paths), loads the
//! file (`loader`), looks up its functions (`function_table`), and probes
//! its versions. It also finds the other copies of the library visible
//! beside the one loaded.
//!
//! `Opening` keeps each library's setup guide beside its report, so the
//! setup check shows and advises about the libraries discovery opened.
//!
//! To bind another library:
//!
//! 1. Generate its bindings, the Rust declarations of its API, from its
//!    header with `scripts/generate_bindings.py`. A `bindings.toml` in its
//!    module's folder says what they are made of. The script writes them to
//!    the `generated/` folder beside it (as the CUDA Runtime's `cudart_12`
//!    and cuTensorNet's `v2_13`). Assert at compile time the layout of each
//!    struct a bound function takes by value (no function bound today takes
//!    one).
//! 2. Add its module here. Its type implements `NativeLibrary`: its name,
//!    override variable, file name and default paths, its table, and its
//!    accepted versions. The table (`function_table!`) has one row per
//!    function: its C symbol, with its function-pointer type declared under
//!    that name in its family's file (as the CUDA Runtime's `device.rs`).
//!    Where the header declares a version, assert the accepted one against
//!    it at compile time. It also implements `LibrarySetupGuide`: what the
//!    setup check shows of it and advises.
//! 3. Add its field to `LoadedLibraries`. Open it in `LoadedLibraries::open`
//!    after the libraries it runs on, so that each of those is listed before
//!    it in the report and the setup check.
//! 4. Test its versions in the `tests.rs` beside its module. Add its
//!    variable, paths and versions to the README.
//!
//! Discovery, `PackagedLibraries` (in `packaged_libraries`) and the public
//! report, a list with one entry per library, do not change. Adding a
//! library changes no public type.

pub(crate) mod cuda_runtime;
pub(crate) mod cutensornet;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod function_table;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod loader;
pub(crate) mod packaged_libraries;
pub(crate) mod report;
pub(crate) mod setup_guide;

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use function_table::{FunctionTable, SymbolResolver};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use loader::{OpenLibraryFile, open_library_file};
pub use packaged_libraries::PackagedLibraries;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use report::LibraryFault;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use report::{LibraryReport, LibrarySource, Version};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use setup_guide::{LibrarySetupGuide, SetupGuide};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use std::path::Path;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use std::path::PathBuf;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use std::{ffi::OsString, io::ErrorKind};

/// The loaded libraries, each with its function table.
///
/// The libraries stay loaded while this lives, so no function pointer of
/// the tables outlives its library. Its fields drop in declaration order,
/// cuTensorNet first. The system loader keeps the CUDA Runtime loaded while
/// cuTensorNet needs it in any case. It holds no device, stream, handle or
/// state: whoever calls a function that creates one owns it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct LoadedLibraries {
    pub(crate) cutensornet: BoundLibrary<cutensornet::CuTensorNet>,
    pub(crate) cuda_runtime: BoundLibrary<cuda_runtime::CudaRuntime>,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl LoadedLibraries {
    /// Opens the libraries a GPU run needs, each after those it runs on:
    /// the CUDA Runtime, then cuTensorNet. They are kept loaded only when
    /// every one is usable.
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
    /// The name of its file, for instance `libcudart.so.12`. A packaged file
    /// of this name is tried for it. The last default path is this name,
    /// for the system loader to search for.
    const FILE_NAME: &'static str;
    /// The paths tried, in order, when the variable is not set, after its
    /// packaged file, if any.
    const DEFAULTS: &'static [&'static str];
    /// The table of the functions looked up in it.
    type Functions: FunctionTable;

    /// Its file among the `packaged` ones, the first whose file name is
    /// `FILE_NAME`, tried before the defaults when the variable is not set.
    fn packaged(packaged: &PackagedLibraries) -> Option<&Path> {
        packaged
            .files
            .iter()
            .map(PathBuf::as_path)
            .find(|file| file.file_name() == Some(Self::FILE_NAME.as_ref()))
    }

    /// Reads the library's versions through `resolver` and checks them.
    ///
    /// It looks up each version function it calls on its own, so that each
    /// version is read even when other functions are missing, another
    /// version's included. A library of another version is often what lacks
    /// them. A version whose functions are missing is not read; the table
    /// reports them.
    fn probe_versions<R: SymbolResolver>(resolver: &R) -> ProbedVersions;
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
/// report and the setup guide of each library, in the order they are
/// opened, whatever its faults. So the setup check shows and advises about
/// exactly the libraries discovery opened, each after those it runs on.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct Opening<C> {
    checker: C,
    reports: Vec<LibraryReport>,
    /// The setup guide of each library opened, in the order of `reports`.
    guides: Vec<SetupGuide>,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl<C: CheckLibrary> Opening<C> {
    pub(crate) fn new(checker: C) -> Self {
        Self {
            checker,
            reports: Vec::new(),
            guides: Vec::new(),
        }
    }

    /// Opens and checks `L`, keeps its report and its setup guide, and
    /// returns it bound to its table when it has no fault.
    pub(crate) fn open<L: LibrarySetupGuide>(&mut self) -> Option<BoundLibrary<L>> {
        let (report, bound) = self.checker.check::<L>();
        self.reports.push(report);
        self.guides.push(SetupGuide::of::<L>());
        bound
    }

    /// The report and the setup guide of each library opened, in order.
    pub(crate) fn into_parts(self) -> (Vec<LibraryReport>, Vec<SetupGuide>) {
        (self.reports, self.guides)
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
/// `open`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct InstalledLibraries<'a, F> {
    /// Reads an override variable: its value, or `None` when it is unset.
    pub(crate) read_variable: F,
    /// The files a package manager installed, tried before the defaults.
    pub(crate) packaged: &'a PackagedLibraries,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl<F: Fn(&'static str) -> Option<OsString>> CheckLibrary for InstalledLibraries<'_, F> {
    fn check<L: NativeLibrary>(&self) -> Checked<L> {
        open::<L>(&self.read_variable, self.packaged)
    }
}

/// Opens `L` and checks it, with the override read by `read_variable`.
///
/// The override comes from `read_variable`, and the packaged file from
/// `packaged`. A fault before the library is open (an invalid override, or
/// a library not found, at a path the process cannot access, or refused by
/// the system loader) is its only fault. Once it is open, every missing
/// function and every version fault is reported, with the other copies of
/// the library visible beside it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn open<L: NativeLibrary>(
    read_variable: impl Fn(&'static str) -> Option<OsString>,
    packaged: &PackagedLibraries,
) -> Checked<L> {
    let mut report = LibraryReport {
        name: L::NAME,
        path: None,
        versions: Vec::new(),
        faults: Vec::new(),
        other_copies: Vec::new(),
        source: None,
    };
    let override_path = override_of::<L>(read_variable);
    let overridden = matches!(override_path, Ok(Some(_)));
    let opened = override_path.and_then(|override_path| {
        open_library_file(candidates::<L>(override_path.clone(), packaged))
            .map(|library| (library, override_path))
    });
    let (library, override_path) = match opened {
        Ok(opened) => opened,
        Err(fault) => {
            record_refusal::<L>(&mut report, fault, overridden, packaged);
            return (report, None);
        }
    };
    record_copy_loaded::<L>(
        &mut report,
        library.path(),
        override_path.as_deref(),
        packaged,
    );
    let functions = inspect::<L, _>(&library, &mut report);
    let bound = functions.map(|functions| BoundLibrary { functions, library });
    (report, bound)
}

/// The override of `L`: the path its own variable, read by
/// `read_variable`, names, once validated; `None` when that variable is
/// unset. A library reads no other library's variable.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn override_of<L: NativeLibrary>(
    read_variable: impl Fn(&'static str) -> Option<OsString>,
) -> Result<Option<PathBuf>, LibraryFault> {
    validate_override_path(L::VARIABLE, read_variable(L::VARIABLE))
}

/// Records in `report` `fault`, met before `L` was open. When the system
/// loader refused a copy, it also records where that copy was found, so
/// that the setup check can tell a copy the override names.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn record_refusal<L: NativeLibrary>(
    report: &mut LibraryReport,
    fault: LibraryFault,
    overridden: bool,
    packaged: &PackagedLibraries,
) {
    if let LibraryFault::LoadFailed { path, .. } = &fault {
        report.source = Some(source_of::<L>(path, overridden, packaged));
    }
    report.faults.push(fault);
}

/// Records in `report` the copy of `L` loaded from `loaded`: its path,
/// where it was found, and the other copies visible beside it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn record_copy_loaded<L: NativeLibrary>(
    report: &mut LibraryReport,
    loaded: &Path,
    override_path: Option<&Path>,
    packaged: &PackagedLibraries,
) {
    report.path = Some(loaded.to_path_buf());
    report.source = Some(source_of::<L>(loaded, override_path.is_some(), packaged));
    report.other_copies = other_copies::<L>(loaded, override_path, packaged);
}

/// Checks an open library through `resolver`. It adds its versions and
/// every fault to `report`, and returns its table when the library has no
/// fault.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn inspect<L: NativeLibrary, R: SymbolResolver>(
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

/// The paths to try for `L`. That is `override_path`, the path its variable
/// names once validated, alone. Otherwise its file in `packaged`, if any,
/// then its default paths.
///
/// ```text
/// override set:    override_path
/// override unset:  file of L in packaged? -> L::DEFAULTS, in order
/// ```
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn candidates<L: NativeLibrary>(
    override_path: Option<PathBuf>,
    packaged: &PackagedLibraries,
) -> Vec<PathBuf> {
    match override_path {
        Some(path) => vec![path],
        None => L::packaged(packaged)
            .into_iter()
            .map(Path::to_path_buf)
            .chain(L::DEFAULTS.iter().map(PathBuf::from))
            .collect(),
    }
}

/// Where `path`, the candidate of `L` tried, was found. When an override
/// is set (`overridden`), it is the override, the only candidate.
/// Otherwise its file in `packaged`, a default absolute path, or a file
/// name the system loader searches for.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn source_of<L: NativeLibrary>(
    path: &Path,
    overridden: bool,
    packaged: &PackagedLibraries,
) -> LibrarySource {
    if overridden {
        LibrarySource::Override
    } else if L::packaged(packaged) == Some(path) {
        LibrarySource::Packaged
    } else if path.is_absolute() {
        LibrarySource::DefaultPath
    } else {
        LibrarySource::SystemSearch
    }
}

/// The other copies of `L` the setup makes visible beside `loaded`. They
/// are the files among `override_path`, its file in `packaged` and its
/// default absolute paths that exist and are not `loaded` (links
/// followed), each once, in that order. A bare file name the system loader
/// searches for is not looked at.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn other_copies<L: NativeLibrary>(
    loaded: &Path,
    override_path: Option<&Path>,
    packaged: &PackagedLibraries,
) -> Vec<PathBuf> {
    let loaded = loaded.canonicalize().ok();
    let mut seen = Vec::new();
    let mut copies = Vec::new();
    for path in named_paths::<L>(override_path, packaged) {
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

/// Every path the setup names for `L`, tried or not: `override_path`, its
/// file in `packaged`, then its default paths. The other copies are looked
/// for among them, since these are the paths the user's setup makes
/// visible.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn named_paths<'a, L: NativeLibrary>(
    override_path: Option<&'a Path>,
    packaged: &'a PackagedLibraries,
) -> impl Iterator<Item = PathBuf> + 'a {
    override_path
        .map(Path::to_path_buf)
        .into_iter()
        .chain(L::packaged(packaged).map(Path::to_path_buf))
        .chain(L::DEFAULTS.iter().map(PathBuf::from))
}

/// The path an override variable names, if it is set.
///
/// An override is checked before anything is loaded. It must be an absolute
/// path to a file, or to a symlink to one, so that a mistyped variable fails
/// with its name instead of loading another library. A path that cannot be
/// read is told apart from one that does not exist.
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn validate_override_path(
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
    let metadata = std::fs::metadata(&path).map_err(|error| LibraryFault::InvalidOverride {
        variable,
        path: path.clone(),
        reason: match error.kind() {
            ErrorKind::NotFound | ErrorKind::NotADirectory => "path does not exist",
            ErrorKind::PermissionDenied => "path cannot be read: permission denied",
            _ => "path cannot be read",
        },
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
    versions: Vec<Version>,
    faults: Vec<LibraryFault>,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl ProbedVersions {
    /// Records `found`, the version of `component`, which is accepted only if
    /// it is the audited one, `audited`.
    fn audited(&mut self, component: &'static str, found: i128, audited: i128) {
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
    fn reported(&mut self, component: &'static str, found: i128) {
        self.versions.push(Version {
            component,
            value: found,
        });
    }
}

#[cfg(test)]
mod tests;

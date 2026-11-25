// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SetupGuide`]: what the setup check tells a user about one library, as
//! the library's module declares it in its `LibrarySetupGuide`.
//!
//! The setup check shows each library and advises what to fix, and names
//! none. Each library's module implements `LibrarySetupGuide` beside its
//! `NativeLibrary`. It says what the check shows of the library, what to
//! install, what to do when its version probe fails, the problems of the
//! host its report reveals, and the oldest driver and device it runs on.
//! The variable that overrides it comes from its `NativeLibrary`.
//! Discovery keeps the guide of each library it opens, in the order
//! opened, and gives them to the check. The check finds each library's
//! guide by its name ([`SetupGuide::for_library`]). So a library bound
//! later brings its own guide, and the check does not change.
//!
//! [`DriverFloor`] and [`ComputeCapability`] are the types of a guide's
//! floors. [`or_unread`], [`path_or_unread`] and [`cuda_version`] are how
//! a guide, and the check, show one of a report's values.

use super::report::LibraryReport;
use std::fmt;

/// What the setup check tells a user about one library, beside what
/// discovery needs of it (its `NativeLibrary`). It is read only through
/// [`SetupGuide::of`], whose guides the check shows and advises from.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) trait LibrarySetupGuide: super::NativeLibrary {
    /// What to install: the library at its audited version, for instance
    /// `cuTensorNet 2.13`.
    const INSTALL: &'static str;
    /// The oldest NVIDIA driver it runs on, or `None` when it sets no
    /// floor.
    const MINIMUM_DRIVER: Option<DriverFloor> = None;
    /// The oldest compute capability of a CUDA device it runs on, or `None`
    /// when it sets no floor.
    const MINIMUM_COMPUTE_CAPABILITY: Option<ComputeCapability> = None;

    /// What to do when a call that reads one of its versions fails. `None`
    /// when no such call can fail; the check then advises the install.
    fn version_probe_advice() -> Option<String> {
        None
    }

    /// What the check shows of the library once loaded (its report has a
    /// path), whether or not discovery succeeded, one line per item. That
    /// is where it was loaded from, and its versions, each `unread` when
    /// the report lacks it.
    fn describe(report: &LibraryReport) -> Vec<String>;

    /// A problem of the host that the usable library's report reveals, in
    /// words for the user with its fix. None by default.
    fn host_problem(_report: &LibraryReport) -> Option<String> {
        None
    }
}

/// A library's `LibrarySetupGuide` as a value, so that the guides of
/// different libraries sit in one list, kept by discovery as it opens
/// them. `SetupGuide::of` copies each field from the library's items.
#[derive(Clone, Debug)]
pub(crate) struct SetupGuide {
    /// The library's name, its `NativeLibrary::NAME`, by which
    /// [`SetupGuide::for_library`] finds the guide.
    pub(crate) library: &'static str,
    /// The variable that overrides the library, its
    /// `NativeLibrary::VARIABLE`, which the advice names.
    pub(crate) variable: &'static str,
    /// What to install, `LibrarySetupGuide::INSTALL`.
    pub(crate) install: &'static str,
    /// The oldest NVIDIA driver it runs on, if it sets one.
    pub(crate) minimum_driver: Option<DriverFloor>,
    /// The oldest compute capability of a device it runs on, if it sets
    /// one.
    pub(crate) minimum_compute_capability: Option<ComputeCapability>,
    /// What to do when its version probe fails, if it says.
    pub(crate) version_probe_advice: fn() -> Option<String>,
    /// The lines the check shows of the library once loaded.
    pub(crate) describe: fn(&LibraryReport) -> Vec<String>,
    /// The problem of the host its report reveals, if any.
    pub(crate) host_problem: fn(&LibraryReport) -> Option<String>,
}

impl SetupGuide {
    /// The guide in `guides` of the library `library` reports on. It is
    /// found by the library's name, so that no caller pairs guides with
    /// reports by their order.
    ///
    /// # Panics
    ///
    /// When `guides` has no guide of that name: discovery keeps the guide
    /// of each library it opens, so each report it makes has one.
    pub(crate) fn for_library<'g>(guides: &'g [Self], library: &LibraryReport) -> &'g Self {
        guides
            .iter()
            .find(|guide| guide.library == library.name)
            .unwrap_or_else(|| {
                panic!(
                    "discovery keeps the guide of each library it opens, so one of {}",
                    library.name
                )
            })
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl SetupGuide {
    /// The guide that `L`'s module declares.
    pub(crate) const fn of<L: LibrarySetupGuide>() -> Self {
        Self {
            library: L::NAME,
            variable: L::VARIABLE,
            install: L::INSTALL,
            minimum_driver: L::MINIMUM_DRIVER,
            minimum_compute_capability: L::MINIMUM_COMPUTE_CAPABILITY,
            version_probe_advice: L::version_probe_advice,
            describe: L::describe,
            host_problem: L::host_problem,
        }
    }
}

/// The oldest NVIDIA driver a library runs on: the CUDA it must support,
/// and NVIDIA's first Linux driver release that supports it. A user
/// compares that release with `nvidia-smi`'s. Floors compare by their CUDA
/// first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DriverFloor {
    /// The CUDA the driver must support, as CUDA encodes it
    /// ([`cuda_version`]).
    pub(crate) cuda: i128,
    /// NVIDIA's first Linux driver release that supports it, for instance
    /// `525.60.13`.
    pub(crate) release: &'static str,
}

/// For instance `a driver for CUDA 12.0 (525.60.13 or later)`.
impl fmt::Display for DriverFloor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a driver for CUDA {} ({} or later)",
            cuda_version(self.cuda),
            self.release
        )
    }
}

/// A CUDA device's compute capability, the version of its architecture,
/// for instance 7.5. A library's floor and the engine's device compare by
/// it: by major, then minor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ComputeCapability {
    /// The major revision, the core architecture: the 7 of 7.5.
    pub(crate) major: i32,
    /// The minor revision, an improvement of that architecture: the 5 of
    /// 7.5.
    pub(crate) minor: i32,
}

/// For instance `7.5`.
impl fmt::Display for ComputeCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// `value`, or `unread` when the report lacks it. The setup check also
/// shows a library at fault, whose report lacks the versions it did not
/// read.
pub(crate) fn or_unread(value: Option<impl fmt::Display>) -> String {
    value.map_or_else(|| "unread".to_string(), |value| value.to_string())
}

/// The path `report`'s library was loaded from, in backticks as in the
/// faults, or `unread` when it has none. Each guide's `describe` and the
/// warning about another copy show it so.
pub(crate) fn path_or_unread(report: &LibraryReport) -> String {
    or_unread(
        report
            .path
            .as_ref()
            .map(|path| format!("`{}`", path.display())),
    )
}

/// A CUDA version as CUDA encodes it, `1000·major + 10·minor`, shown as
/// `major.minor` (`12090` is 12.9). It shows the CUDA Runtime's version,
/// the CUDA a driver supports, and the CUDA Runtime cuTensorNet was built
/// for.
pub(crate) fn cuda_version(version: i128) -> String {
    format!("{}.{}", version / 1000, version % 1000 / 10)
}

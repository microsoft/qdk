// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SetupGuide`]: what the setup check tells a user about one library, as
//! the library's module declares it in its `LibrarySetupGuide`.
//!
//! The setup check shows each library and advises what to fix, and names
//! none: each library's module implements `LibrarySetupGuide` beside its
//! `NativeLibrary`, saying what the check shows of it, what to install,
//! what to do when its version probe fails, the problems of the host its
//! report reveals, and the oldest driver and device it runs on (the
//! variable that overrides it comes from its `NativeLibrary`). Discovery
//! keeps the guide of each library it opens, in the order opened, and
//! gives them to the check. So a library bound later brings its own, and
//! the check changes nothing.
//!
//! [`or_unread`] and [`cuda_version`] are how a guide, and the check, show
//! one of a report's values.

use super::report::LibraryReport;
use std::fmt;

/// What the setup check tells a user about one library, beside what
/// discovery needs of it (its `NativeLibrary`): read only through
/// [`SetupGuide::of`], whose guides the check shows and advises from.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) trait LibrarySetupGuide: super::NativeLibrary {
    /// What to install, for instance `cuTensorNet 2.13`: the library at its
    /// audited version.
    const INSTALL: &'static str;
    /// The oldest NVIDIA driver it runs on; `None` when it sets no floor.
    const MINIMUM_DRIVER: Option<DriverFloor> = None;
    /// The oldest compute capability, (major, minor), of a CUDA device it
    /// runs on; `None` when it sets no floor.
    const MINIMUM_COMPUTE_CAPABILITY: Option<(i32, i32)> = None;

    /// What to do when a call that reads one of its versions fails; `None`
    /// when no such call can fail, and the check advises the install.
    fn version_probe_advice() -> Option<String> {
        None
    }

    /// What the check shows of the library once loaded (its report has a
    /// path), whether or not discovery succeeded, one line per item: where
    /// it was loaded from, and its versions, each version `unread` when the
    /// report lacks it.
    fn describe(report: &LibraryReport) -> Vec<String>;

    /// A problem of the host that the usable library's report reveals, in
    /// words for the user with its fix: none by default.
    fn host_problem(_report: &LibraryReport) -> Option<String> {
        None
    }
}

/// A library's `LibrarySetupGuide` as a value, so that the guides of
/// different libraries sit in one list, kept by discovery as it opens
/// them: what the setup check shows of the library and what it advises.
/// Each field is the item of its name that `SetupGuide::of` copies: of
/// the library's `NativeLibrary` (`library` its `NAME`, `variable` its
/// `VARIABLE`), and of its `LibrarySetupGuide` (the others).
#[derive(Clone, Debug)]
pub(crate) struct SetupGuide {
    pub(crate) library: &'static str,
    pub(crate) variable: &'static str,
    pub(crate) install: &'static str,
    pub(crate) minimum_driver: Option<DriverFloor>,
    pub(crate) minimum_compute_capability: Option<(i32, i32)>,
    pub(crate) version_probe_advice: fn() -> Option<String>,
    pub(crate) describe: fn(&LibraryReport) -> Vec<String>,
    pub(crate) host_problem: fn(&LibraryReport) -> Option<String>,
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
/// and NVIDIA's first Linux driver release that supports it, which is what
/// a user compares with `nvidia-smi`'s. Floors compare by their CUDA.
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

/// `value`, or `unread` when the report lacks it: the setup check shows
/// a library at fault too, whose report may lack versions.
pub(crate) fn or_unread(value: Option<impl fmt::Display>) -> String {
    value.map_or_else(|| "unread".to_string(), |value| value.to_string())
}

/// A CUDA version as CUDA encodes it, `1000·major + 10·minor`, shown as
/// `major.minor` (`12090` is 12.9): the CUDA Runtime's version, the CUDA a
/// driver supports, and the CUDA Runtime cuTensorNet was built for.
pub(crate) fn cuda_version(version: i128) -> String {
    format!("{}.{}", version / 1000, version % 1000 / 10)
}

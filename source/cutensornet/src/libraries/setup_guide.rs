// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SetupGuide`]: what the setup check tells a user about one library, as
//! the library's module declares it in its [`LibrarySetupGuide`].
//!
//! The setup check shows each library and advises what to fix, and its
//! library steps name none: each library's module implements
//! `LibrarySetupGuide` beside its `NativeLibrary`, saying what the check
//! shows of it, what to install, what to do when its version probe fails,
//! and the problems of the host its report reveals (the variable that
//! overrides it comes from its `NativeLibrary`);
//! `LoadedLibraries::SETUP_GUIDES` lists the guides, and [`setup_guides`]
//! gives them to the check. So a library bound later brings its own, and
//! the library steps change nothing. The device step is the exception:
//! the check holds its facts, cuTensorNet's compute-capability floor and
//! the CUDA Runtime's driver advice.
//! [`or_unread`] is how a guide, and the check, show one of a report's
//! optional values.

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
    /// What to do when a call that reads one of its versions fails; `None`
    /// when no such call can fail, and the check advises the install.
    const VERSION_PROBE_ADVICE: Option<&'static str> = None;

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
/// different libraries sit in one list (`LoadedLibraries::SETUP_GUIDES`):
/// what the setup check shows of the library and what it advises.
pub(crate) struct SetupGuide {
    /// The library's name, as its report gives it.
    pub(crate) library: &'static str,
    /// The variable that overrides its path.
    pub(crate) variable: &'static str,
    /// What to install, for instance `cuTensorNet 2.13`.
    pub(crate) install: &'static str,
    /// What to do when a version probe fails, if one can.
    pub(crate) version_probe_advice: Option<&'static str>,
    /// The lines that show the library once loaded, usable or not.
    pub(crate) describe: fn(&LibraryReport) -> Vec<String>,
    /// A problem of the host that the usable library reveals.
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
            version_probe_advice: L::VERSION_PROBE_ADVICE,
            describe: L::describe,
            host_problem: L::host_problem,
        }
    }
}

/// `value`, or `unread` when the report lacks it: the setup check shows
/// a library at fault too, whose report may lack versions. A path goes
/// through it as a defence only: the check shows the path of a library
/// loaded, whose report always has it.
pub(crate) fn or_unread(value: Option<impl fmt::Display>) -> String {
    value.map_or_else(|| "unread".to_string(), |value| value.to_string())
}

/// The guide of each library, in the order the setup check lists them;
/// none off Linux x86-64, where discovery fails before any library is
/// looked at.
pub(crate) fn setup_guides() -> &'static [SetupGuide] {
    #[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
    {
        &super::LoadedLibraries::SETUP_GUIDES
    }
    #[cfg(not(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64"))))]
    {
        &[]
    }
}

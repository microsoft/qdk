// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SetupGuide`]: what the setup check tells a user about one library, as
//! the library's module declares it.
//!
//! The setup check shows each library and advises what to fix, and its
//! library steps name none: each library's module declares, as part of
//! its `NativeLibrary`, what the check shows of it, what to install, the
//! variable that overrides it, what to do when its version probe fails,
//! and the problems of the host its report reveals;
//! `LoadedLibraries::SETUP_GUIDES` lists the guides, and [`setup_guides`]
//! gives them to the check. So a library bound later brings its own, and
//! the library steps change nothing. The device step is the exception:
//! the check holds its facts, cuTensorNet's compute-capability floor and
//! the CUDA Runtime's driver advice.
//! [`or_unread`] is how a guide, and the check, show one of a report's
//! optional values.

use super::report::LibraryReport;
use std::fmt;

/// What the setup check tells a user about one library: what it shows of
/// the library and what it advises. Each library's module declares it, as
/// part of its `NativeLibrary`, and `LoadedLibraries::SETUP_GUIDES` lists
/// them.
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
    pub(crate) const fn of<L: super::NativeLibrary>() -> Self {
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

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SetupGuide`]: what the setup check (`diagnose`) tells a user about one
//! library, as the library's module declares it.
//!
//! The setup check shows each library and advises what to fix, but names
//! none: each library's module declares, as part of its `NativeLibrary`,
//! what the check shows of it, what to install, and the problems of the
//! host its report reveals; `LoadedLibraries::SETUP_GUIDES` lists the
//! guides. So a library bound later brings its own, and the check, which
//! uses this layer, changes nothing. [`or_unread`] is how a guide, and the
//! check, show a value a hand-made report may lack.

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
    /// The lines that show the usable library.
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

/// `value`, or `unread` when the report lacks it: a usable library's report
/// has every value, so the fallback only keeps a hand-made report shown.
pub(crate) fn or_unread(value: Option<impl fmt::Display>) -> String {
    value.map_or_else(|| "unread".to_string(), |value| value.to_string())
}

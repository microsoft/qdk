// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the setup check tells a user: the advice for a failed discovery,
//! the install it advises, and the way it shows a device's memory.
//!
//! The words about one library come from its `SetupGuide`, which its
//! module declares; this file adds those that hold for every library: how
//! to set an override, what to install, and when.

use crate::libraries::setup_guide::SetupGuide;
use crate::{AvailabilityError, LibraryFault};

/// What to do about `error`: when libraries are at fault, about an invalid
/// override if any, since an override decides what is loaded; or else
/// about the first fault of the first library at fault, in the order they
/// are opened. `install_command` is the packager's, given as an example of
/// the install.
pub(super) fn availability_advice(
    error: &AvailabilityError,
    install_command: Option<&str>,
) -> String {
    match error {
        AvailabilityError::UnsupportedPlatform { .. } => {
            "the MPS method's GPU engine runs on Linux x86-64 only".to_string()
        }
        AvailabilityError::Unusable(report) => {
            let faults = || {
                report
                    .libraries
                    .iter()
                    .flat_map(|library| library.faults.iter().map(|fault| (library.library, fault)))
            };
            faults()
                .find(|(_, fault)| matches!(fault, LibraryFault::InvalidOverride { .. }))
                .or_else(|| faults().next())
                .map_or_else(
                    || install(install_command),
                    |(library, fault)| fault_advice(library, fault, install_command),
                )
        }
    }
}

/// What to do about `fault`, of the library named `library`.
fn fault_advice(library: &str, fault: &LibraryFault, install_command: Option<&str>) -> String {
    let guide = setup_guide(library);
    let install = || install(install_command);
    match fault {
        LibraryFault::InvalidOverride { variable, .. } => {
            format!("set {variable} to the absolute path of the library file, or unset it")
        }
        LibraryFault::NotFound { .. } => match guide {
            Some(guide) => format!(
                "{}, or set {} to the absolute path of {library}",
                install(),
                guide.variable
            ),
            None => install(),
        },
        LibraryFault::LoadFailed { .. } => format!(
            "{library} was found but did not load, often because a library it needs is \
             missing: {}, which brings its dependencies",
            install()
        ),
        LibraryFault::MissingRequiredSymbol { .. } | LibraryFault::UnsupportedVersion { .. } => {
            format!("only the audited versions are supported: {}", install())
        }
        LibraryFault::VersionProbeFailed { .. } => {
            match guide.and_then(|guide| guide.version_probe_advice) {
                Some(advice) => advice.to_string(),
                None => format!("{library} could not report its version: {}", install()),
            }
        }
    }
}

/// What to install: each library at its audited version, in the order the
/// check lists them, then, when the packager gave one, `install_command`
/// as an example.
fn install(install_command: Option<&str>) -> String {
    let libraries: Vec<&str> = setup_guides().iter().map(|guide| guide.install).collect();
    let libraries = libraries.join(" and ");
    match install_command {
        Some(command) => format!("install {libraries}, for instance with `{command}`"),
        None => format!("install {libraries}"),
    }
}

/// `bytes` in GiB, to one decimal.
#[allow(clippy::cast_precision_loss)] // A device's memory, shown to 0.1 GiB.
pub(super) fn gibibytes(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / f64::from(1u32 << 30))
}

/// The guide of each library, in the order the check lists them; none off
/// Linux x86-64, where discovery fails before any library is looked at.
pub(super) fn setup_guides() -> &'static [SetupGuide] {
    #[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
    {
        &crate::libraries::LoadedLibraries::SETUP_GUIDES
    }
    #[cfg(not(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64"))))]
    {
        &[]
    }
}

/// The guide of the library named `library`.
fn setup_guide(library: &str) -> Option<&'static SetupGuide> {
    setup_guides().iter().find(|guide| guide.library == library)
}

//! Experimental runtime discovery for optional cuTensorNet acceleration.

#[allow(
    clippy::mod_module_files,
    reason = "the approved versioned FFI layout groups generated bindings under bindings/mod.rs"
)]
mod bindings;
mod error;
mod execution;
pub mod generator;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod library;
mod simulation;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
mod version;

pub use error::AvailabilityError;
#[doc(hidden)]
pub use execution::{MpsExecutionError, run_mps_shots};

use std::{fmt, path::PathBuf};

#[cfg(test)]
const SYMBOL_MANIFEST: &str = include_str!("../scripts/cutensornet-symbols.txt");

/// The cuTensorNet symbols whose absence must abort discovery.
///
/// Derived from the manifest rather than restated, so adding a symbol is one
/// manifest row and nothing else. There is no second list to fall out of step
/// with, which is the property that makes widening the surface for a newer
/// cuTensorNet release a mechanical edit.
#[cfg(test)]
fn cutensornet_required_symbols() -> Vec<String> {
    manifest_rows()
        .into_iter()
        .filter(|row| row.requirement == generator::Requirement::Required)
        .map(|row| row.symbol)
        .collect()
}

/// Every row of the checked-in manifest.
#[cfg(test)]
fn manifest_rows() -> Vec<generator::ManifestRow> {
    generator::parse_manifest(SYMBOL_MANIFEST)
        .expect("the checked-in manifest should be well formed")
}
#[cfg(test)]
const CUDART_REQUIRED_SYMBOLS: &[&str] = &[
    "cudaRuntimeGetVersion",
    "cudaDriverGetVersion",
    "cudaGetDeviceCount",
    "cudaSetDevice",
    "cudaGetErrorString",
    "cudaMemGetInfo",
    "cudaMalloc",
    "cudaFree",
    "cudaMemcpy",
    "cudaStreamCreateWithFlags",
    "cudaStreamSynchronize",
    "cudaStreamDestroy",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailabilityReport {
    pub cutensornet_library: PathBuf,
    pub cuda_runtime_library: PathBuf,
    pub cutensornet_version: usize,
    pub cutensornet_cuda_runtime_version: usize,
    pub cuda_runtime_version: i32,
    pub cuda_driver_version: i32,
}

pub struct Availability {
    report: AvailabilityReport,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    libraries: std::sync::Arc<library::NativeApi>,
}

impl Availability {
    #[must_use]
    pub fn report(&self) -> &AvailabilityReport {
        &self.report
    }
}

impl fmt::Debug for Availability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Availability")
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}

/// Discovers and validates the optional CUDA Runtime and cuTensorNet libraries.
///
/// This function performs no device selection, allocation, handle creation, or
/// GPU work.
pub fn discover() -> Result<Availability, AvailabilityError> {
    discover_with_overrides(
        std::env::var_os("QDK_CUTENSORNET_LIBRARY"),
        std::env::var_os("QDK_CUDART_LIBRARY"),
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn discover_with_overrides(
    cutensornet_override: Option<std::ffi::OsString>,
    cudart_override: Option<std::ffi::OsString>,
) -> Result<Availability, AvailabilityError> {
    library::discover(cutensornet_override, cudart_override)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn discover_with_overrides(
    _cutensornet_override: Option<std::ffi::OsString>,
    _cudart_override: Option<std::ffi::OsString>,
) -> Result<Availability, AvailabilityError> {
    Err(AvailabilityError::UnsupportedPlatform {
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    })
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn validate_override_path(
    variable: &'static str,
    value: Option<std::ffi::OsString>,
) -> Result<Option<PathBuf>, AvailabilityError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(AvailabilityError::InvalidOverride {
            variable,
            path,
            reason: "path must be absolute",
        });
    }
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|_| AvailabilityError::InvalidOverride {
            variable,
            path: path.clone(),
            reason: "path does not exist",
        })?;
    if !metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
        return Err(AvailabilityError::InvalidOverride {
            variable,
            path,
            reason: "path must name a regular file or symlink",
        });
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::{
        Availability, AvailabilityError, CUDART_REQUIRED_SYMBOLS, SYMBOL_MANIFEST,
        cutensornet_required_symbols, manifest_rows, validate_override_path,
    };
    use crate::generator::{Loader, Requirement, serialize};
    use std::ffi::OsString;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    /// The generated loader as it is checked in, keyed by the path
    /// [`render`] assigns each file.
    ///
    /// The manifest plus the bindings are the entire *input* to loader
    /// generation, and these files are its entire *output*. Stating both sides
    /// here is what lets a single assertion below check the whole contract.
    const GENERATED_SOURCES: &[(&str, &str)] = &[
        ("symbols.rs", include_str!("library/symbols.rs")),
        (
            "symbols/context.rs",
            include_str!("library/symbols/context.rs"),
        ),
        ("symbols/state.rs", include_str!("library/symbols/state.rs")),
        (
            "symbols/workspace.rs",
            include_str!("library/symbols/workspace.rs"),
        ),
        (
            "symbols/operator.rs",
            include_str!("library/symbols/operator.rs"),
        ),
        (
            "symbols/expectation.rs",
            include_str!("library/symbols/expectation.rs"),
        ),
        (
            "symbols/sampler.rs",
            include_str!("library/symbols/sampler.rs"),
        ),
        (
            "symbols/contraction.rs",
            include_str!("library/symbols/contraction.rs"),
        ),
        (
            "symbols/logging.rs",
            include_str!("library/symbols/logging.rs"),
        ),
    ];

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn availability_is_send_and_sync() {
        assert_send_sync::<Availability>();
    }

    #[test]
    fn rejects_relative_override() {
        assert!(matches!(
            validate_override_path("TEST_LIBRARY", Some(OsString::from("library.so"))),
            Err(AvailabilityError::InvalidOverride {
                variable: "TEST_LIBRARY",
                reason: "path must be absolute",
                ..
            })
        ));
    }

    #[test]
    fn rejects_missing_absolute_override_without_fallback() {
        let path = std::env::temp_dir().join(format!(
            "qdk-cutensornet-test-missing-{}-{}",
            std::process::id(),
            line!()
        ));
        assert!(matches!(
            validate_override_path("TEST_LIBRARY", Some(path.clone().into_os_string())),
            Err(AvailabilityError::InvalidOverride {
                variable: "TEST_LIBRARY",
                path: found,
                reason: "path does not exist",
            }) if found == path
        ));
    }

    #[test]
    fn rejects_directory_override_before_loading() {
        let path = std::env::temp_dir();
        assert!(matches!(
            validate_override_path("TEST_LIBRARY", Some(path.clone().into_os_string())),
            Err(AvailabilityError::InvalidOverride {
                variable: "TEST_LIBRARY",
                path: found,
                reason: "path must name a regular file or symlink",
            }) if found == path
        ));
    }

    /// `CUDART_REQUIRED_SYMBOLS` is still written out by hand, because the
    /// cudart loader is hand-written and the manifest is cuTensorNet-only, so
    /// it keeps a frozen count. The cuTensorNet side deliberately has none: it
    /// is derived from the manifest, and asserting a count there would mean
    /// every new symbol needed a second edit.
    #[test]
    fn cudart_inventory_matches_the_frozen_surface() {
        assert_eq!(CUDART_REQUIRED_SYMBOLS.len(), 12);
    }

    /// Optional symbols weaken discovery: absence is tolerated rather than
    /// rejected. That is a deliberate exception, so the set is pinned even
    /// though the required set is not.
    #[test]
    fn only_the_last_error_helper_is_optional() {
        let optional: Vec<String> = manifest_rows()
            .into_iter()
            .filter(|row| row.requirement == Requirement::Optional)
            .map(|row| row.symbol)
            .collect();
        assert_eq!(optional, vec!["cutensornetGetLastError".to_owned()]);
        assert!(!cutensornet_required_symbols().contains(&"cutensornetGetLastError".to_owned()));
    }

    /// Every required symbol must be declared in the generated bindings *and*
    /// resolved by the loader.
    ///
    /// The loader (`library`) only compiles on `linux/x86_64`, so on any other
    /// host `cargo test` silently skips every resolver test. Reading both files
    /// as text keeps this check running everywhere, so a symbol added to the
    /// inventory but never resolved is caught on the development host instead
    /// of only on a CUDA-capable one.
    ///
    /// This still has teeth now that the loader is generated: the bindings are
    /// regenerated from `cutensornet.h` on a CUDA host while the loader is
    /// generated from the manifest anywhere, so the check catches a symbol
    /// added to the manifest without regenerating the bindings.
    #[test]
    fn required_symbols_are_declared_in_bindings_and_resolved_by_the_loader() {
        const BINDINGS: &str = include_str!("bindings/v2_13.rs");
        const LOADER: &str = concat!(
            include_str!("library.rs"),
            include_str!("library/symbols.rs")
        );

        for symbol in &cutensornet_required_symbols() {
            assert!(
                BINDINGS.contains(&format!("pub fn {symbol}(")),
                "{symbol} is required but not declared in the generated bindings"
            );
            assert!(
                LOADER.contains(&format!("b\"{symbol}\\0\"")),
                "{symbol} is required but never resolved by the loader"
            );
        }

        for symbol in CUDART_REQUIRED_SYMBOLS {
            assert!(
                LOADER.contains(&format!("b\"{symbol}\\0\"")),
                "{symbol} is required but never resolved by the loader"
            );
        }

        // Optional symbols are exempt from the inventory but must still exist
        // on both sides, so widening the manifest without regenerating the
        // bindings on a CUDA host fails here rather than at load time.
        for row in manifest_rows() {
            let symbol = row.symbol;
            assert!(
                BINDINGS.contains(&format!("pub fn {symbol}(")),
                "{symbol} is in the manifest but not declared in the generated bindings"
            );
            assert!(
                LOADER.contains(&format!("b\"{symbol}\\0\"")),
                "{symbol} is in the manifest but never resolved by the loader"
            );
        }
    }

    /// Format source the way the generator binary does, so rendered text can
    /// be compared byte for byte with a checked-in file.
    ///
    /// rustfmt is invoked rather than emulated: it reorders `mod` declarations
    /// and adds trailing commas when it explodes a parameter list, and
    /// reproducing those rules here would be a maintenance trap.
    fn rustfmt(source: &str) -> String {
        let mut child = Command::new("rustfmt")
            .args(["--edition", "2024", "--emit", "stdout"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustfmt should be installed alongside cargo");
        child
            .stdin
            .take()
            .expect("stdin was piped")
            .write_all(source.as_bytes())
            .expect("rustfmt should accept source on stdin");
        let output = child.wait_with_output().expect("rustfmt should terminate");
        assert!(
            output.status.success(),
            "rustfmt rejected generated source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("rustfmt should emit UTF-8")
    }

    /// The entire generation contract in one assertion: the checked-in loader
    /// must be exactly what the generator produces from the manifest and the
    /// bindings. Any hand-edit, stale file or unregenerated manifest row fails
    /// here, and the message names the file to regenerate.
    ///
    /// This subsumes the structural and signature checks that used to be
    /// written out by hand. The hand-written loader this replaced declared
    /// three parameters of `cutensornetNetworkOperatorAppendProduct` as
    /// `*const *const T` where the header says `*mut *const T`; constness does
    /// not affect the ABI, so it never failed at runtime and nothing compared
    /// the two. Deriving both sides from one source makes that class of defect
    /// unrepresentable.
    #[test]
    fn checked_in_loader_matches_freshly_generated_output() {
        let loader = Loader::build(SYMBOL_MANIFEST, include_str!("bindings/v2_13.rs"))
            .expect("the checked-in manifest and bindings should build a loader");
        let files = serialize(&loader);

        assert_eq!(
            files.len(),
            GENERATED_SOURCES.len(),
            "the generator and the checked-in tree disagree on how many files exist"
        );

        for file in &files {
            let checked_in = GENERATED_SOURCES
                .iter()
                .find(|(path, _)| *path == file.path)
                .map_or_else(
                    || panic!("{} is generated but not checked in", file.path),
                    |(_, source)| *source,
                );
            assert_eq!(
                rustfmt(&file.contents),
                checked_in,
                "{} is out of date; run `cargo run -p qdk_cutensornet --bin generate-loader`",
                file.path
            );
        }
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn unsupported_target_does_not_attempt_discovery() {
        assert!(matches!(
            super::discover_with_overrides(None, None),
            Err(AvailabilityError::UnsupportedPlatform { .. })
        ));
    }
}

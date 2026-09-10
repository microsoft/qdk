//! Experimental runtime discovery for optional cuTensorNet acceleration.

#[allow(
    clippy::mod_module_files,
    reason = "the approved versioned FFI layout groups generated bindings under bindings/mod.rs"
)]
mod bindings;
mod error;
mod execution;
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

#[cfg(test)]
const CUTENSORNET_REQUIRED_SYMBOLS: &[&str] = &[
    "cutensornetGetVersion",
    "cutensornetGetCudartVersion",
    "cutensornetGetErrorString",
    "cutensornetCreate",
    "cutensornetDestroy",
    "cutensornetCreateState",
    "cutensornetDestroyState",
    "cutensornetStateApplyTensorOperator",
    "cutensornetStateFinalizeMPS",
    "cutensornetStateCaptureMPS",
    "cutensornetStateConfigure",
    "cutensornetCreateWorkspaceDescriptor",
    "cutensornetDestroyWorkspaceDescriptor",
    "cutensornetStatePrepare",
    "cutensornetWorkspaceGetMemorySize",
    "cutensornetWorkspaceSetMemory",
    "cutensornetStateCompute",
    "cutensornetCreateNetworkOperator",
    "cutensornetNetworkOperatorAppendProduct",
    "cutensornetDestroyNetworkOperator",
    "cutensornetCreateExpectation",
    "cutensornetExpectationConfigure",
    "cutensornetExpectationPrepare",
    "cutensornetExpectationCompute",
    "cutensornetDestroyExpectation",
    "cutensornetCreateSampler",
    "cutensornetSamplerConfigure",
    "cutensornetSamplerPrepare",
    "cutensornetSamplerSample",
    "cutensornetDestroySampler",
    "cutensornetCreateNetwork",
    "cutensornetDestroyNetwork",
    "cutensornetNetworkAppendTensor",
    "cutensornetNetworkSetOutputTensor",
    "cutensornetNetworkSetAttribute",
    "cutensornetWorkspaceComputeContractionSizes",
    "cutensornetCreateContractionOptimizerConfig",
    "cutensornetDestroyContractionOptimizerConfig",
    "cutensornetContractionOptimizerConfigSetAttribute",
    "cutensornetCreateContractionOptimizerInfo",
    "cutensornetDestroyContractionOptimizerInfo",
    "cutensornetContractionOptimize",
    "cutensornetContractionOptimizerInfoGetAttribute",
    "cutensornetNetworkPrepareContraction",
    "cutensornetCreateSliceGroupFromIDRange",
    "cutensornetDestroySliceGroup",
    "cutensornetNetworkSetInputTensorMemory",
    "cutensornetNetworkSetOutputTensorMemory",
    "cutensornetNetworkContract",
];
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
        Availability, AvailabilityError, CUDART_REQUIRED_SYMBOLS, CUTENSORNET_REQUIRED_SYMBOLS,
        SYMBOL_MANIFEST, validate_override_path,
    };
    use std::ffi::OsString;

    /// One row of the symbol manifest.
    ///
    /// The manifest plus the generated bindings are the entire *input* to the
    /// loader generation process; everything in `src/library/symbols{,/*}.rs`
    /// is its *output*. Spelling that contract out here is what lets the tests
    /// below check it mechanically:
    ///
    /// | Input | Output |
    /// | --- | --- |
    /// | `symbol`, and its `pub fn` in the bindings | `pub(crate) type <Alias> = unsafe extern "C" fn(..)` in `symbols/<family>.rs` |
    /// | `field`, `family`, `requirement` | a `CuTensorNetFunctions` field and its `resolve_*` initializer in `symbols.rs` |
    ///
    /// Adding a symbol is therefore exactly: one manifest row, regenerated
    /// bindings, and a rerun of `scripts/generate-loader.py`. Nothing about the
    /// signature is written by hand, which is the point.
    struct ManifestRow {
        symbol: &'static str,
        field: &'static str,
        family: &'static str,
        requirement: &'static str,
    }

    const BINDINGS: &str = include_str!("bindings/v2_13.rs");
    const SYMBOLS_RS: &str = include_str!("library/symbols.rs");

    /// Every generated family file. Listing them here also asserts that the
    /// families named by the manifest are the families that actually exist.
    const FAMILY_SOURCES: &[(&str, &str)] = &[
        ("context", include_str!("library/symbols/context.rs")),
        (
            "contraction",
            include_str!("library/symbols/contraction.rs"),
        ),
        (
            "expectation",
            include_str!("library/symbols/expectation.rs"),
        ),
        ("operator", include_str!("library/symbols/operator.rs")),
        ("sampler", include_str!("library/symbols/sampler.rs")),
        ("state", include_str!("library/symbols/state.rs")),
        ("workspace", include_str!("library/symbols/workspace.rs")),
    ];

    fn manifest_rows() -> Vec<ManifestRow> {
        SYMBOL_MANIFEST
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let columns: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(columns.len(), 4, "malformed manifest row: {line}");
                assert!(
                    matches!(columns[3], "required" | "optional"),
                    "unknown requirement in manifest row: {line}"
                );
                ManifestRow {
                    symbol: columns[0],
                    field: columns[1],
                    family: columns[2],
                    requirement: columns[3],
                }
            })
            .collect()
    }

    fn family_source(family: &str) -> &'static str {
        FAMILY_SOURCES
            .iter()
            .find(|(name, _)| *name == family)
            .unwrap_or_else(|| {
                panic!("manifest names family {family:?}, which has no generated file")
            })
            .1
    }

    /// `network_append_tensor` becomes `NetworkAppendTensorFn`.
    fn alias_name(field: &str) -> String {
        let mut alias = String::new();
        for part in field.split('_') {
            let mut characters = part.chars();
            if let Some(first) = characters.next() {
                alias.extend(first.to_uppercase());
                alias.push_str(&characters.as_str().to_lowercase());
            }
        }
        alias.push_str("Fn");
        alias
    }

    /// Erase the differences that are pure spelling, so a declaration and the
    /// function pointer generated from it can be compared: the bindings say
    /// `::std::os::raw::c_void` where the loader is in a module that imports
    /// `v2_13`, and rustfmt wraps the two at different points.
    fn normalize_type(raw: &str) -> String {
        raw.replace("v2_13::", "")
            .replace("::std::os::raw::", "")
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect()
    }

    /// Split `(a, b, c) -> R;` starting from `from`, returning the parameter
    /// text and the return type.
    fn split_signature(source: &str, from: usize) -> (&str, &str) {
        let open = from + source[from..].find('(').expect("a parameter list");
        let close = open + source[open..].find(')').expect("a closing parenthesis");
        let parameters = &source[open + 1..close];
        assert!(
            !parameters.contains('('),
            "nested parentheses are not supported: {parameters}"
        );
        let tail = &source[close + 1..];
        let end = tail.find(';').expect("a terminating semicolon");
        let returns = tail[..end]
            .trim()
            .strip_prefix("->")
            .expect("an explicit return type");
        (parameters, returns)
    }

    /// The signature cuTensorNet actually declares, from the generated bindings.
    fn declared_signature(symbol: &str) -> (Vec<String>, String) {
        let at = BINDINGS
            .find(&format!("pub fn {symbol}("))
            .unwrap_or_else(|| panic!("{symbol} is not declared in the generated bindings"));
        let (parameters, returns) = split_signature(BINDINGS, at);
        let types = parameters
            .split(',')
            .map(str::trim)
            .filter(|parameter| !parameter.is_empty())
            .map(|parameter| {
                let (_name, declared) = parameter
                    .split_once(':')
                    .unwrap_or_else(|| panic!("unparsable parameter {parameter:?} of {symbol}"));
                normalize_type(declared)
            })
            .collect();
        (types, normalize_type(returns))
    }

    /// The signature the loader will call through, from the generated alias.
    fn alias_signature(family: &str, alias: &str) -> (Vec<String>, String) {
        let source = family_source(family);
        let at = source
            .find(&format!("type {alias} ="))
            .unwrap_or_else(|| panic!("{alias} is missing from symbols/{family}.rs"));
        let (parameters, returns) = split_signature(source, at);
        let types = parameters
            .split(',')
            .map(str::trim)
            .filter(|parameter| !parameter.is_empty())
            .map(normalize_type)
            .collect();
        (types, normalize_type(returns))
    }

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

    #[test]
    fn symbol_inventories_match_the_frozen_surface() {
        assert_eq!(CUTENSORNET_REQUIRED_SYMBOLS.len(), 49);
        assert_eq!(CUDART_REQUIRED_SYMBOLS.len(), 12);
        assert!(!CUTENSORNET_REQUIRED_SYMBOLS.contains(&"cutensornetGetLastError"));
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

        for symbol in CUTENSORNET_REQUIRED_SYMBOLS {
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
    /// The manifest drives both generators, so it must agree with the inventory
    /// this module asserts against.
    #[test]
    fn manifest_agrees_with_the_required_symbol_inventory() {
        let rows = manifest_rows();

        let mut required: Vec<&str> = rows
            .iter()
            .filter(|row| row.requirement == "required")
            .map(|row| row.symbol)
            .collect();
        let mut inventory = CUTENSORNET_REQUIRED_SYMBOLS.to_vec();
        required.sort_unstable();
        inventory.sort_unstable();
        assert_eq!(
            required, inventory,
            "the manifest and the required-symbol inventory disagree"
        );

        let optional: Vec<&str> = rows
            .iter()
            .filter(|row| row.requirement == "optional")
            .map(|row| row.symbol)
            .collect();
        assert_eq!(optional, vec!["cutensornetGetLastError"]);
    }

    /// The structural half of the generation contract: every manifest row must
    /// produce an alias, a struct field and a resolve call, in the shapes the
    /// hand-written code depends on.
    #[test]
    fn every_manifest_row_is_wired_into_the_generated_loader() {
        for row in manifest_rows() {
            let ManifestRow {
                symbol,
                field,
                family,
                requirement,
            } = row;
            let alias = alias_name(field);

            assert!(
                family_source(family).contains(&format!("pub(crate) type {alias} =")),
                "{symbol}: no `{alias}` alias in symbols/{family}.rs"
            );

            let declaration = if requirement == "optional" {
                format!("pub(crate) {field}: Option<{family}::{alias}>,")
            } else {
                format!("pub(crate) {field}: {family}::{alias},")
            };
            assert!(
                SYMBOLS_RS.contains(&declaration),
                "{symbol}: expected `{declaration}` on CuTensorNetFunctions"
            );

            let initializer = if requirement == "optional" {
                format!("{field}: resolve_optional(resolver, b\"{symbol}\\0\")")
            } else {
                format!("{field}: resolve_required(")
            };
            assert!(
                SYMBOLS_RS.contains(&initializer),
                "{symbol}: expected `{initializer}` in resolve_cutensornet_functions"
            );
        }
    }

    /// The semantic half of the contract, and the reason the loader is
    /// generated at all: every function pointer must have the signature the
    /// header declares.
    ///
    /// The hand-written loader this replaced declared three parameters of
    /// `cutensornetNetworkOperatorAppendProduct` as `*const *const T` where the
    /// header says `*mut *const T`. Constness does not affect the ABI, so it
    /// never failed at runtime and nothing compared the two. This test does.
    #[test]
    fn every_generated_signature_matches_its_bindings_declaration() {
        for row in manifest_rows() {
            let alias = alias_name(row.field);
            assert_eq!(
                alias_signature(row.family, &alias),
                declared_signature(row.symbol),
                "{}: the generated function pointer disagrees with the header",
                row.symbol
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

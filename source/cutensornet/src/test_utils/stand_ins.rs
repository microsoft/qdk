// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Stand-ins for NVIDIA's libraries, so that the tests check libraries and
//! discovery on any Unix host. A resolver hands out stand-in functions
//! instead of a library's. On Linux with glibc, a real library that is
//! always present stands in where a test needs one open.

use crate::libraries::function_table::SymbolResolver;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::libraries::loader::{OpenLibraryFile, open_library_file};
use std::mem::{size_of, transmute_copy};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::path::PathBuf;

/// The C library of glibc, which the system loader always opens: the test
/// binary itself is linked against it. If its name ever changed, the tests
/// that open it would fail, not pass unchecked.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(crate) const ALWAYS_PRESENT_LIBRARY: &str = "libc.so.6";

/// A real open library, standing in for an NVIDIA library where a test needs
/// one open.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(crate) fn always_present_library() -> OpenLibraryFile {
    open_library_file(vec![PathBuf::from(ALWAYS_PRESENT_LIBRARY)])
        .expect("the C library is loaded in every glibc process")
}

unsafe extern "C" fn stand_in_function() {}

/// Writes the CUDA version `V` (`12090` is 12.9), as `cudaRuntimeGetVersion`
/// and `cudaDriverGetVersion` do.
pub(crate) unsafe extern "C" fn cuda_version<const V: i32>(version: *mut i32) -> u32 {
    // SAFETY: the caller passes a writable `int`, as CUDA's contract asks.
    unsafe { version.write(V) };
    0
}

/// Returns the version `V`, as `cutensornetGetVersion` and
/// `cutensornetGetCudartVersion` do.
pub(crate) extern "C" fn cutensornet_version<const V: usize>() -> usize {
    V
}

/// Resolves every name to a stand-in pointer that must not be called,
/// except the names given a function and the ones withheld. So a library
/// can be checked without opening one.
pub(crate) struct StandInResolver {
    missing: Vec<&'static str>,
    functions: Vec<(&'static str, *const ())>,
}

impl StandInResolver {
    /// A library whose version functions report the audited versions.
    pub(crate) fn audited() -> Self {
        Self {
            missing: Vec::new(),
            functions: Vec::new(),
        }
        .with("cudaRuntimeGetVersion", cuda_version::<12_090> as *const ())
        .with("cudaDriverGetVersion", cuda_version::<12_090> as *const ())
        .with(
            "cutensornetGetVersion",
            cutensornet_version::<21_300> as *const (),
        )
        .with(
            "cutensornetGetCudartVersion",
            cutensornet_version::<12_090> as *const (),
        )
    }

    /// Also resolves `symbol` to `function`, which must have its signature.
    pub(crate) fn with(mut self, symbol: &'static str, function: *const ()) -> Self {
        self.functions.retain(|&(name, _)| name != symbol);
        self.functions.push((symbol, function));
        self
    }

    /// Also withholds `symbol`.
    pub(crate) fn without(mut self, symbol: &'static str) -> Self {
        self.missing.push(symbol);
        self
    }
}

impl SymbolResolver for StandInResolver {
    unsafe fn resolve<T: Copy>(&self, symbol: &'static str) -> Option<T> {
        if self.missing.contains(&symbol) {
            return None;
        }
        let pointer = self
            .functions
            .iter()
            .find(|&&(name, _)| name == symbol)
            .map_or(stand_in_function as *const (), |&(_, function)| function);
        assert_eq!(size_of::<T>(), size_of::<*const ()>());
        // SAFETY: every table field is a pointer-sized function pointer; the
        // tests call only the functions given with their signature.
        Some(unsafe { transmute_copy(&pointer) })
    }
}

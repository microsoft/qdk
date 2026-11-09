// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The C declarations the loader resolves functions against, and checks of
//! the binary layout they assume.
//!
//! A function pointer is only as right as the C signature it was declared
//! with, so both libraries' declarations live here, apart from the code that
//! loads them. `v2_13` holds cuTensorNet 2.13's declarations for the
//! functions listed in `scripts/cutensornet-symbols.txt`, from bindgen's
//! output of `scripts/generate-bindings.sh` (its header says how);
//! `cudart_12` is written by hand from the CUDA Runtime 12.9 header. The
//! assertions below fail the build if `cuDoubleComplex`, which a cuTensorNet
//! function takes by value, does not have the layout of NVIDIA's `double2`.

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
#[allow(
    dead_code,
    reason = "discovery uses the version probes; the MPS engine and the setup check, added later, use the rest"
)]
pub(crate) mod cudart_12;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
#[allow(
    clippy::doc_markdown,
    dead_code,
    rustdoc::broken_intra_doc_links,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    reason = "bindgen preserves NVIDIA C documentation and identifiers verbatim"
)]
pub(crate) mod v2_13;

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
mod abi {
    use super::v2_13;
    use std::mem::{align_of, offset_of, size_of};

    const _: () = assert!(size_of::<v2_13::cuDoubleComplex>() == 16);
    const _: () = assert!(align_of::<v2_13::cuDoubleComplex>() == 16);
    const _: () = assert!(offset_of!(v2_13::cuDoubleComplex, x) == 0);
    const _: () = assert!(offset_of!(v2_13::cuDoubleComplex, y) == 8);
}

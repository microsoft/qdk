// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![no_main]

allocator::assign_global!();

#[cfg(feature = "do_fuzz")]
use libfuzzer_sys::fuzz_target;

#[cfg(feature = "do_fuzz")]
fuzz_target!(|data: &[u8]| {
    fuzz::compile_qasm(data);
});

#[cfg(not(feature = "do_fuzz"))]
#[unsafe(no_mangle)]
pub extern "C" fn main() {
    fuzz::compile_qasm(&[]);
}

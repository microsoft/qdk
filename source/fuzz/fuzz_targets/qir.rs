// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![no_main]

allocator::assign_global!();

#[cfg(feature = "do_fuzz")]
use libfuzzer_sys::fuzz_target;

use fuzz::qir_seed_bank::{
    compile, compile_base_v1, compile_mutated_adaptive_v1, compile_mutated_adaptive_v2,
};
use qsc_llvm::fuzz::mutation::compile_raw_parser_lanes;

#[cfg(feature = "do_fuzz")]
fuzz_target!(|data: &[u8]| {
    compile_raw_parser_lanes(data);
    compile(data);
    compile_base_v1(data);
    compile_mutated_adaptive_v1(data);
    compile_mutated_adaptive_v2(data);
});

#[cfg(not(feature = "do_fuzz"))]
#[unsafe(no_mangle)]
pub extern "C" fn main() {
    compile_raw_parser_lanes(&[]);
    compile(&[]);
    compile_base_v1(&[]);
    compile_mutated_adaptive_v1(&[]);
    compile_mutated_adaptive_v2(&[]);
}

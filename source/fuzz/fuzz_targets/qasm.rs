// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![no_main]

allocator::assign_global!();

#[cfg(feature = "do_fuzz")]
use libfuzzer_sys::fuzz_target;

use qsc::{
    PackageStore, PackageType,
    compile::{compile_ast, package_store_with_stdlib},
    hir::PackageId,
    openqasm::{
        CompilerConfig, OutputSemantics, ProgramType, QubitSemantics,
        compiler::parse_and_compile_to_qsharp_ast_with_config, io::InMemorySourceResolver,
    },
    target::Profile,
};

fn compile(data: &[u8]) {
    if let Ok(fuzzed_code) = std::str::from_utf8(data) {
        thread_local! {
            static STORE_STD: (PackageId, PackageStore) = {
                package_store_with_stdlib(Profile::Unrestricted.into())
            };
        }
        STORE_STD.with(|(stdid, store)| {
            let mut resolver = InMemorySourceResolver::from_iter([]);
            let config = CompilerConfig::new(
                QubitSemantics::Qiskit,
                OutputSemantics::OpenQasm,
                ProgramType::File,
                Some("Fuzz".into()),
                None,
            );

            let unit = parse_and_compile_to_qsharp_ast_with_config(
                fuzzed_code,
                "fuzz.qasm",
                Some(&mut resolver),
                config,
            );
            let (sources, _, package, _, profile) = unit.into_tuple();

            let dependencies = vec![(PackageId::CORE, None), (*stdid, None)];

            let (mut _unit, _errors) = compile_ast(
                store,
                &dependencies,
                package,
                sources,
                PackageType::Lib,
                profile.into(),
            );
        });
    }
}

#[cfg(feature = "do_fuzz")]
fuzz_target!(|data: &[u8]| {
    compile(data);
});

#[cfg(not(feature = "do_fuzz"))]
#[unsafe(no_mangle)]
pub extern "C" fn main() {
    compile(&[]);
}

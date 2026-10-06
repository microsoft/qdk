// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

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

pub fn compile_qasm(data: &[u8]) {
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
                profile.unwrap_or(Profile::Unrestricted).into(),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::compile_qasm;
    use std::{fs, path::PathBuf};

    fn replay(relative_path: &str) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative_path);
        let input = fs::read(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        compile_qasm(&input);
    }

    macro_rules! regression_test {
        ($name:ident, $path:literal) => {
            #[test]
            #[ignore = "requires a downloaded fuzzer artifact"]
            fn $name() {
                replay($path);
            }
        };
    }

    regression_test!(
        issue_3460_slow_unit,
        "fuzzer-artifacts/issue-3460/run-29285219868/fuzz/artifacts/qasm/slow-unit-22319e63b252d86648968988fbcbf77737e73933"
    );
    regression_test!(
        issue_3460_timeout,
        "fuzzer-artifacts/issue-3460/run-29285219868/fuzz/artifacts/qasm/timeout-ecd20b1ce67027f908cfde55d46ebb0f033ec749"
    );
    regression_test!(
        issue_3469,
        "fuzzer-artifacts/issue-3469/run-29376452100/fuzz/artifacts/qasm/timeout-e113933d19c95991e95478d46c59e845819e103b"
    );
    regression_test!(
        issue_3485,
        "fuzzer-artifacts/issue-3485/run-29614788189/fuzz/artifacts/qasm/timeout-afd40e5d6d2560c999c4e814e24246b580766aff"
    );
    regression_test!(
        issue_3487,
        "fuzzer-artifacts/issue-3487/run-29759947330/fuzz/artifacts/qasm/timeout-b3bcc11c189807e4339930131b213e3ddec3dfd9"
    );
    regression_test!(
        issue_3494,
        "fuzzer-artifacts/issue-3494/run-29800583170/fuzz/artifacts/qasm/timeout-7cacbad1859022a546b55c50c48711856706ce1a"
    );
    regression_test!(
        issue_3497,
        "fuzzer-artifacts/issue-3497/run-29858663234/fuzz/artifacts/qasm/oom-8b85bc4b46a9a9f9cf32d4855ed61da6682053f9"
    );
    regression_test!(
        issue_3521,
        "fuzzer-artifacts/issue-3521/run-30121507388/fuzz/artifacts/qasm/timeout-fbf662e66a2a733d304ffda900bf0378fd18cfbb"
    );
    regression_test!(
        issue_3525,
        "fuzzer-artifacts/issue-3525/run-30408357713/fuzz/artifacts/qasm/timeout-e5b4f51f4ad39d8a2b201975752aff45453abb4c"
    );
    regression_test!(
        issue_3566,
        "fuzzer-artifacts/issue-3566/run-31516657570/fuzz/artifacts/qasm/timeout-237504ca507fd40ee47d5965931954b4b842a4ef"
    );
    regression_test!(
        issue_3572,
        "fuzzer-artifacts/issue-3572/run-31546417094/fuzz/artifacts/qasm/timeout-427453dd5fc9ac4cf1e9494cbeb6dd56dc3e69af"
    );
    regression_test!(
        issue_3633,
        "fuzzer-artifacts/issue-3633/run-32742996705/fuzz/artifacts/qasm/timeout-0efc5cfaa05d75de18413cd7f9824c8d656988d9"
    );
    regression_test!(
        issue_3638,
        "fuzzer-artifacts/issue-3638/run-32799274949/fuzz/artifacts/qasm/timeout-c397ba7b73728b8d3bd2dce2fd42a00c9d75dd47"
    );
    regression_test!(
        issue_3692,
        "fuzzer-artifacts/issue-3692/run-33922936270/fuzz/artifacts/qasm/timeout-732610af00c8bdd9de4aa8786ea21136f1ad324d"
    );
}

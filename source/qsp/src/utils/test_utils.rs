// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{QuantumBackend, run_qsharp_expr};
use qsc::{LanguageFeatures, PackageType, SourceMap, interpret::Interpreter, target::Profile};
use std::sync::Arc;

/// Creates an unrestricted Q# interpreter with the supplied sources and standard library.
fn create_qsharp_interpreter(sources: Vec<(Arc<str>, Arc<str>)>) -> Result<Interpreter, String> {
    let capabilities = Profile::Unrestricted.into();
    let (std_id, store) = qsc::compile::package_store_with_stdlib(capabilities);
    Interpreter::new(
        SourceMap::new(sources, None),
        PackageType::Exe,
        capabilities,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .map_err(|errors| format!("Failed to compile Q# test sources: {errors:?}"))
}

/// Compiles Q# test source and runs `Test.Main()` with the supplied backend.
pub(crate) fn run_qsharp_main(
    source: &str,
    backend: &mut impl QuantumBackend,
) -> Result<(), String> {
    let mut interpreter = create_qsharp_interpreter(vec![("test.qs".into(), source.into())])?;
    run_qsharp_expr(&mut interpreter, "Test.Main()", backend).map_err(|error| error.to_string())
}

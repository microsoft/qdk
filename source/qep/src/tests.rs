// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    QsBackendShim, QuantumBackend, QuantumBackends, ResultAdapter, TranspilingAdapter,
    run_qsharp_expr,
};
use qsc::{
    CliffordSim, LanguageFeatures, PackageType, SourceMap, interpret::Interpreter, target::Profile,
};
use qsc_eval::backend::SparseSim;
use std::{path::PathBuf, sync::Arc};

/// Creates an unrestricted Q# interpreter with the supplied sources and standard library.
pub(crate) fn create_qsharp_interpreter(
    sources: Vec<(Arc<str>, Arc<str>)>,
) -> Result<Interpreter, String> {
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

/// Loads the arithmetic library alongside the supplied Q# program.
pub(crate) fn qsharp_sources_with_arithmetic(source: &str) -> Vec<(Arc<str>, Arc<str>)> {
    let library_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../library/arithmetic/src");
    let mut sources: Vec<(Arc<str>, Arc<str>)> = std::fs::read_dir(&library_dir)
        .expect("failed to read arithmetic library directory")
        .map(|entry| {
            let path = entry.expect("failed to read directory entry").path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("arithmetic library file name should be valid UTF-8");
            let contents =
                std::fs::read_to_string(&path).expect("failed to read arithmetic library file");
            (name.into(), contents.into())
        })
        .collect();
    sources.push(("test.qs".into(), source.into()));
    sources
}

#[test]
/// Verifies a Q# adder with multiple simulators and result adapters.
fn qsharp_adder_with_different_backends() -> Result<(), String> {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use x = Qubit[5];
                use y = Qubit[5];
                use carry = Qubit();
                ApplyXorInPlace(7, x);
                ApplyXorInPlace(15, y);
                Std.Arithmetic.RippleCarryTTKIncByLE(x, y + [carry]);
                let result = Std.Measurement.MeasureInteger(y);
                if result != 22 {
                    fail $"Expected 22, got {result}";
                }
            }
        }
    "#;
    run_qsharp_main(source, &mut QuantumBackends::sparse_sim())?;
    run_qsharp_main(source, &mut QsBackendShim::new(SparseSim::new()))?;
    run_qsharp_main(
        source,
        &mut TranspilingAdapter::with_defaults(QsBackendShim::new(CliffordSim::new(11))),
    )?;
    run_qsharp_main(
        source,
        &mut ResultAdapter::new(QuantumBackends::sparse_sim()),
    )?;
    run_qsharp_main(
        source,
        &mut ResultAdapter::new(QsBackendShim::new(SparseSim::new())),
    )?;
    run_qsharp_main(
        source,
        &mut ResultAdapter::new(TranspilingAdapter::with_defaults(QsBackendShim::new(
            CliffordSim::new(11),
        ))),
    )?;
    Ok(())
}

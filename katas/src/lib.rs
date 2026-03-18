// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use qsc::{
    PackageType, SourceContents, SourceMap, SourceName,
    interpret::{Error, Interpreter, Value, output::Receiver},
    target::Profile,
};

use qsc::LanguageFeatures;
use qsc_codegen::qsharp::write_package_string;
use qsc_openqasm_compiler::{
    CompilerConfig, OutputSemantics, ProgramType, QubitSemantics,
    compiler::parse_and_compile_to_qsharp_ast_with_config,
};

pub const EXAMPLE_ENTRY: &str = "Kata.RunExample()";

pub const EXERCISE_ENTRY: &str = "Kata.Verification.CheckSolution()";

/// # Errors
///
/// Returns a vector of errors if compilation or evaluation failed.
///
/// # Panics
///
/// Will panic if evaluation does not return a boolean as result.
pub fn check_solution(
    exercise_sources: Vec<(SourceName, SourceContents)>,
    receiver: &mut impl Receiver,
) -> Result<bool, Vec<Error>> {
    let source_map = SourceMap::new(exercise_sources, Some(EXERCISE_ENTRY.into()));
    let (std_id, store) = qsc::compile::package_store_with_stdlib(Profile::Unrestricted.into());

    let mut interpreter: Interpreter = Interpreter::new(
        source_map,
        PackageType::Exe,
        Profile::Unrestricted.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )?;

    interpreter.eval_entry(receiver).map(|value| {
        if let Value::Bool(success) = value {
            success
        } else {
            panic!("exercise verification did not return a boolean")
        }
    })
}

/// Checks an `OpenQASM` solution by compiling it to Q# source code and then
/// verifying it against the exercise's Q# verification harness.
///
/// # Errors
///
/// Returns a vector of errors if compilation or evaluation failed.
///
/// # Panics
///
/// Will panic if evaluation does not return a boolean as result.
pub fn check_openqasm_solution(
    openqasm_code: &str,
    operation_name: &str,
    exercise_sources: Vec<(SourceName, SourceContents)>,
    receiver: &mut impl Receiver,
) -> Result<bool, Vec<Error>> {
    // Compile OpenQASM to Q# AST using ProgramType::Operation so qubit
    // declarations become operation parameters.
    // QubitSemantics::QSharp ensures standard qubit semantics.
    // OutputSemantics::OpenQasm preserves explicit output declarations as
    // return values (for measurement exercises) while producing a Unit return
    // type when there are no output declarations (for unitary exercises).
    let config = CompilerConfig::new(
        QubitSemantics::QSharp,
        OutputSemantics::OpenQasm,
        ProgramType::Operation,
        Some(Arc::from(operation_name)),
        Some(Arc::from("Kata")),
    );

    let compile_unit = parse_and_compile_to_qsharp_ast_with_config(
        openqasm_code,
        "solution.qasm",
        None::<&mut qsc::openqasm::io::InMemorySourceResolver>,
        config,
    );

    if compile_unit.has_errors() {
        // Report OpenQASM compilation errors via the receiver
        let error_msgs: Vec<String> = compile_unit
            .errors()
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        let _ = receiver.message(&format!(
            "OpenQASM compilation failed:\n{}",
            error_msgs.join("\n")
        ));
        return Ok(false);
    }

    let (_, _, package, _, _) = compile_unit.into_tuple();
    let qsharp_source = write_package_string(&package);

    // The OpenQASM compiler with ProgramType::Operation emits a bare operation
    // (no namespace). We need to wrap it in `namespace Kata { ... }` and add
    // `is Adj + Ctl` so the verification harness (which calls Controlled/Adjoint)
    // can use it. The OpenQASM intrinsic gates all support Adj + Ctl.
    // Wrap: add the namespace and functor support.
    // and close the namespace at the end.
    let qsharp_source = format!("namespace Kata {{\n{qsharp_source}\n}}");
    let qsharp_source = qsharp_source.replace(") : Unit {", ") : Unit is Adj + Ctl {");

    // Combine the transpiled Q# source with the exercise verification sources
    let mut sources = vec![("solution".into(), qsharp_source.into())];
    for exercise_source in exercise_sources {
        sources.push(exercise_source);
    }

    check_solution(sources, receiver)
}

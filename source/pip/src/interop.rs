// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::doc_markdown,
    reason = "docstrings in this module conform to the python docstring format."
)]

use std::fmt::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use pyo3::IntoPyObjectExt;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use qsc::hir::PackageId;
use qsc::interpret::output::Receiver;
use qsc::interpret::{CircuitEntryPoint, Interpreter, into_errors};
use qsc::project::ProjectType;
use qsc::qasm::compiler::compile_to_qsharp_ast_with_config;
use qsc::qasm::semantic::QasmSemanticParseResult;
use qsc::qasm::{OperationSignature, QubitSemantics};
use qsc::target::Profile;
use qsc::{Backend, PackageType, PauliNoise, SparseSim};
use qsc::{
    LanguageFeatures, SourceMap, ast::Package, error::WithSource, interpret, project::FileSystem,
};

use crate::fs::file_system;
use crate::interpreter::data_interop::value_to_pyobj;
use crate::interpreter::{
    OptionalCallbackReceiver, OutputSemantics, ProgramType, QSharpError, QasmError, TargetProfile,
    format_error, format_errors,
};

use resource_estimator as re;

/// Runs the given OpenQASM program for the given number of shots.
/// Each shot uses an independent instance of the simulator.
///
/// Note:
///     This call while exported is not intended to be used directly by the user.
///     It is intended to be used by the Python wrapper which will handle the
///     callbacks and other Python specific details.
///
/// Args:
///     source (str): The OpenQASM source code to execute.
///     output_fn (Callable[[Output], None]): The function to handle the output of the execution.
///     noise: The noise to use in simulation.
///     read_file (Callable[[str], Tuple[str, str]]): The function to read a file and return its contents.
///     list_directory (Callable[[str], List[Dict[str, str]]]): The function to list the contents of a directory.
///     resolve_path (Callable[[str, str], str]): The function to resolve a path given a base path and a relative path.
///     fetch_github (Callable[[str, str, str, str], str]): The function to fetch a file from GitHub.
///     **kwargs: Additional keyword arguments to pass to the execution.
///       - target_profile (TargetProfile): The target profile to use for execution.
///       - name (str): The name of the circuit. This is used as the entry point for the program. Defaults to 'program'.
///       - search_path (str): The optional search path for resolving imports.
///       - output_semantics (OutputSemantics, optional): The output semantics for the compilation.
///       - shots (int): The number of shots to run the program for. Defaults to 1.
///       - seed (int): The seed to use for the random number generator.
///
/// Returns:
///     Any: The result of the execution.
///
/// Raises:
///     QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
///     QSharpError: If there is an error interpreting the input.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(
    signature = (source, callback=None, noise=None, qubit_loss=None, read_file=None, list_directory=None, resolve_path=None, fetch_github=None, **kwargs)
)]
pub(crate) fn run_qasm_program(
    py: Python,
    source: &str,
    callback: Option<PyObject>,
    noise: Option<(f64, f64, f64)>,
    qubit_loss: Option<f64>,
    read_file: Option<PyObject>,
    list_directory: Option<PyObject>,
    resolve_path: Option<PyObject>,
    fetch_github: Option<PyObject>,
    kwargs: Option<Bound<'_, PyDict>>,
) -> PyResult<PyObject> {
    let mut receiver = OptionalCallbackReceiver { callback, py };

    let kwargs = kwargs.unwrap_or_else(|| PyDict::new(py));

    let target = get_target_profile(&kwargs)?;
    let operation_name = get_operation_name(&kwargs)?;
    let output_semantics = get_output_semantics(&kwargs, || OutputSemantics::OpenQasm)?;
    let seed = get_seed(&kwargs);
    let shots = get_shots(&kwargs)?;
    let search_path = get_search_path(&kwargs)?;

    let fs = create_filesystem_from_py(py, read_file, list_directory, resolve_path, fetch_github);
    let file_path = PathBuf::from_str(&search_path)
        .expect("from_str is infallible")
        .join("program.qasm");
    let project = fs.load_openqasm_project(&file_path, Some(Arc::<str>::from(source)));
    let ProjectType::OpenQASM(sources) = project.project_type else {
        return Err(QasmError::new_err(
            "Expected OpenQASM project, but got a different type".to_string(),
        ));
    };
    let res = qsc::qasm::semantic::parse_sources(&sources);
    let (package, source_map, signature) = compile_qasm_enriching_errors(
        res,
        &operation_name,
        ProgramType::File,
        output_semantics,
        false,
    )?;

    let package_type = PackageType::Exe;
    let language_features = LanguageFeatures::default();
    let mut interpreter =
        create_interpreter_from_ast(package, source_map, target, language_features, package_type)
            .map_err(|errors| QSharpError::new_err(format_errors(errors)))?;

    let entry_expr = signature.create_entry_expr_from_params(String::new());
    interpreter
        .set_entry_expr(&entry_expr)
        .map_err(|errors| map_entry_compilation_errors(errors, &signature))?;

    let noise = match noise {
        None => None,
        Some((px, py, pz)) => match PauliNoise::from_probabilities(px, py, pz) {
            Ok(noise_struct) => Some(noise_struct),
            Err(error_message) => return Err(PyException::new_err(error_message)),
        },
    };
    let loss = qubit_loss.unwrap_or(0.0);
    let result = run_ast(&mut interpreter, &mut receiver, shots, seed, noise, loss);
    match result {
        Ok(result) => {
            let list: Result<Vec<_>, _> = result
                .iter()
                .map(|v| value_to_pyobj(&interpreter, py, v))
                .collect();
            Ok(PyList::new(py, list?)?.into())
        }
        Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
    }
}

pub(crate) fn run_ast(
    interpreter: &mut Interpreter,
    receiver: &mut impl Receiver,
    shots: usize,
    seed: Option<u64>,
    noise: Option<PauliNoise>,
    loss: f64,
) -> Result<Vec<qsc::interpret::Value>, Vec<interpret::Error>> {
    let mut results = Vec::with_capacity(shots);
    for i in 0..shots {
        let mut sim = if let Some(noise) = noise {
            SparseSim::new_with_noise(&noise)
        } else {
            SparseSim::new()
        };
        sim.set_loss(loss);
        // If seed is provided, we want to use a different seed for each shot
        // so that the results are different for each shot, but still deterministic
        sim.set_seed(seed.map(|s| s + i as u64));
        let result = interpreter.run_with_sim(&mut sim, receiver, None)?;
        results.push(result);
    }

    Ok(results)
}

/// Estimates the resource requirements for executing OpenQASM source code.
///
/// Note:
///     This call while exported is not intended to be used directly by the user.
///     It is intended to be used by the Python wrapper which will handle the
///     callbacks and other Python specific details.
///
/// Args:
///     source (str): The OpenQASM source code to estimate the resource requirements for.
///     job_params (str): The parameters for the job.
///     read_file (Callable[[str], Tuple[str, str]]): A callable that reads a file and returns its content and path.
///     list_directory (Callable[[str], List[Dict[str, str]]]): A callable that lists the contents of a directory.
///     resolve_path (Callable[[str, str], str]): A callable that resolves a file path given a base path and a relative path.
///     fetch_github (Callable[[str, str, str, str], str]): A callable that fetches a file from GitHub.
///     **kwargs: Additional keyword arguments to pass to the execution.
///       - name (str): The name of the circuit. This is used as the entry point for the program. Defaults to 'program'.
///       - search_path (str): The optional search path for resolving imports.
/// Returns:
///     str: The estimated resource requirements for executing the OpenQASM source code.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(
    signature = (source, job_params, read_file, list_directory, resolve_path, fetch_github, **kwargs)
)]
pub(crate) fn resource_estimate_qasm_program(
    py: Python,
    source: &str,
    job_params: &str,
    read_file: Option<PyObject>,
    list_directory: Option<PyObject>,
    resolve_path: Option<PyObject>,
    fetch_github: Option<PyObject>,
    kwargs: Option<Bound<'_, PyDict>>,
) -> PyResult<String> {
    let kwargs = kwargs.unwrap_or_else(|| PyDict::new(py));

    let operation_name = get_operation_name(&kwargs)?;
    let search_path = get_search_path(&kwargs)?;

    let fs = create_filesystem_from_py(py, read_file, list_directory, resolve_path, fetch_github);
    let file_path = PathBuf::from_str(&search_path)
        .expect("from_str is infallible")
        .join("program.qasm");
    let project = fs.load_openqasm_project(&file_path, Some(Arc::<str>::from(source)));
    let ProjectType::OpenQASM(sources) = project.project_type else {
        return Err(QasmError::new_err(
            "Expected OpenQASM project, but got a different type".to_string(),
        ));
    };
    let res = qsc::qasm::semantic::parse_sources(&sources);

    let program_type = ProgramType::File;
    let output_semantics = OutputSemantics::ResourceEstimation;
    let (package, source_map, _) =
        compile_qasm_enriching_errors(res, &operation_name, program_type, output_semantics, false)?;

    match crate::interop::estimate_qasm(package, source_map, job_params) {
        Ok(estimate) => Ok(estimate),
        Err(errors) if matches!(errors[0], re::Error::Interpreter(_)) => {
            Err(QSharpError::new_err(format_errors(
                errors
                    .into_iter()
                    .map(|e| match e {
                        re::Error::Interpreter(e) => e,
                        re::Error::Estimation(_) => unreachable!(),
                    })
                    .collect::<Vec<_>>(),
            )))
        }
        Err(errors) => Err(QSharpError::new_err(
            errors
                .into_iter()
                .map(|e| match e {
                    re::Error::Estimation(e) => e.to_string(),
                    re::Error::Interpreter(_) => unreachable!(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )),
    }
}

/// Compiles the OpenQASM source code into a program that can be submitted to a
/// target as QIR (Quantum Intermediate Representation).
///
/// Note:
///     This call while exported is not intended to be used directly by the user.
///     It is intended to be used by the Python wrapper which will handle the
///     callbacks and other Python specific details.
///
/// Args:
///     source (str): The OpenQASM source code to estimate the resource requirements for.
///     read_file (Callable[[str], Tuple[str, str]]): A callable that reads a file and returns its content and path.
///     list_directory (Callable[[str], List[Dict[str, str]]]): A callable that lists the contents of a directory.
///     resolve_path (Callable[[str, str], str]): A callable that resolves a file path given a base path and a relative path.
///     fetch_github (Callable[[str, str, str, str], str]): A callable that fetches a file from GitHub.
///     **kwargs: Additional keyword arguments to pass to the compilation when source program is provided.
///       - name (str): The name of the circuit. This is used as the entry point for the program.
///       - target_profile (TargetProfile): The target profile to use for code generation.
///       - search_path (Optional[str]): The optional search path for resolving file references.
///       - output_semantics (OutputSemantics, optional): The output semantics for the compilation.
///
/// Returns:
///     str: The converted QIR code as a string.
///
/// Raises:
///     QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
///     QSharpError: If there is an error compiling the program.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(
    signature = (source, read_file, list_directory, resolve_path, fetch_github, **kwargs)
)]
pub(crate) fn compile_qasm_program_to_qir(
    py: Python,
    source: &str,
    read_file: Option<PyObject>,
    list_directory: Option<PyObject>,
    resolve_path: Option<PyObject>,
    fetch_github: Option<PyObject>,
    kwargs: Option<Bound<'_, PyDict>>,
) -> PyResult<String> {
    let kwargs = kwargs.unwrap_or_else(|| PyDict::new(py));

    let target = get_target_profile(&kwargs)?;
    let operation_name = get_operation_name(&kwargs)?;
    let search_path = get_search_path(&kwargs)?;

    let fs = create_filesystem_from_py(py, read_file, list_directory, resolve_path, fetch_github);
    let file_path = PathBuf::from_str(&search_path)
        .expect("from_str is infallible")
        .join("program.qasm");
    let project = fs.load_openqasm_project(&file_path, Some(Arc::<str>::from(source)));
    let ProjectType::OpenQASM(sources) = project.project_type else {
        return Err(QasmError::new_err(
            "Expected OpenQASM project, but got a different type".to_string(),
        ));
    };
    let res = qsc::qasm::semantic::parse_sources(&sources);

    let program_ty = ProgramType::File;
    let output_semantics = get_output_semantics(&kwargs, || OutputSemantics::Qiskit)?;
    let (package, source_map, signature) =
        compile_qasm_enriching_errors(res, &operation_name, program_ty, output_semantics, false)?;

    let package_type = PackageType::Lib;
    let language_features = LanguageFeatures::default();
    let mut interpreter =
        create_interpreter_from_ast(package, source_map, target, language_features, package_type)
            .map_err(|errors| QSharpError::new_err(format_errors(errors)))?;
    let entry_expr = signature.create_entry_expr_from_params(String::new());

    generate_qir_from_ast(entry_expr, &mut interpreter)
}

pub(crate) fn compile_qasm_enriching_errors<S: AsRef<str>>(
    semantic_parse_result: QasmSemanticParseResult,
    operation_name: S,
    program_ty: ProgramType,
    output_semantics: OutputSemantics,
    allow_input_params: bool,
) -> PyResult<(Package, SourceMap, OperationSignature)> {
    let config = qsc::qasm::CompilerConfig::new(
        QubitSemantics::Qiskit,
        output_semantics.into(),
        program_ty.into(),
        Some(operation_name.as_ref().into()),
        None,
    );

    let unit = compile_to_qsharp_ast_with_config(semantic_parse_result, config);

    let (source_map, errors, package, sig, _) = unit.into_tuple();
    if !errors.is_empty() {
        return Err(QasmError::new_err(format_qasm_errors(errors)));
    }

    let Some(signature) = sig else {
        return Err(QasmError::new_err(
            "signature should have had value. This is a bug",
        ));
    };

    if !signature.input.is_empty() && !allow_input_params {
        // no entry expression is provided, but the signature has input parameters.
        let mut message = String::new();
        message += "Circuit has unbound input parameters\n";
        write!(message, "  help: Parameters: {}", signature.input_params())
            .expect("writing to string should succeed");

        return Err(QSharpError::new_err(message));
    }

    Ok((package, source_map, signature))
}

fn generate_qir_from_ast<S: AsRef<str>>(
    entry_expr: S,
    interpreter: &mut Interpreter,
) -> PyResult<String> {
    interpreter
        .qirgen(entry_expr.as_ref())
        .map_err(map_qirgen_errors)
}

/// This call while exported is not intended to be used directly by the user.
/// It is intended to be used by the Python wrapper which will handle the
/// callbacks and other Python specific details.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(
    signature = (source, read_file, list_directory, resolve_path, fetch_github, **kwargs)
)]
pub(crate) fn compile_qasm_to_qsharp(
    py: Python,
    source: &str,
    read_file: Option<PyObject>,
    list_directory: Option<PyObject>,
    resolve_path: Option<PyObject>,
    fetch_github: Option<PyObject>,
    kwargs: Option<Bound<'_, PyDict>>,
) -> PyResult<String> {
    let kwargs = kwargs.unwrap_or_else(|| PyDict::new(py));

    let operation_name = get_operation_name(&kwargs)?;
    let search_path = get_search_path(&kwargs)?;

    let fs = create_filesystem_from_py(py, read_file, list_directory, resolve_path, fetch_github);
    let file_path = PathBuf::from_str(&search_path)
        .expect("from_str is infallible")
        .join("program.qasm");
    let project = fs.load_openqasm_project(&file_path, Some(Arc::<str>::from(source)));
    let ProjectType::OpenQASM(sources) = project.project_type else {
        return Err(QasmError::new_err(
            "Expected OpenQASM project, but got a different type".to_string(),
        ));
    };
    let res = qsc::qasm::semantic::parse_sources(&sources);

    let program_ty = get_program_type(&kwargs, || ProgramType::File)?;
    let output_semantics = get_output_semantics(&kwargs, || OutputSemantics::Qiskit)?;
    let (package, _, _) =
        compile_qasm_enriching_errors(res, &operation_name, program_ty, output_semantics, true)?;

    let qsharp = qsc::codegen::qsharp::write_package_string(&package);
    Ok(qsharp)
}

/// Enriches the compilation errors to provide more helpful messages
/// as we know that we are compiling the entry expression.
pub(crate) fn map_entry_compilation_errors(
    errors: Vec<interpret::Error>,
    sig: &OperationSignature,
) -> PyErr {
    let mut semantic = vec![];
    for error in errors {
        match &error {
            interpret::Error::Compile(_) => {
                // The entry expression is invalid. This is likely due to a type mismatch
                // or missing parameter(s). We should provide a more helpful error message.
                let mut message = format_error(&error);
                writeln!(message).expect("write should succeed");
                writeln!(message, "failed to compile entry point.").expect("write should succeed");
                writeln!(
                    message,
                    "  help: check that the parameter types match the supplied parameters"
                )
                .expect("write should succeed");

                write!(message, "  help: Parameters: {}", sig.input_params())
                    .expect("writing to string should succeed");

                semantic.push(message);
            }
            _ => {
                semantic.push(format_error(&error));
            }
        }
    }
    let message = semantic.into_iter().collect::<String>();
    QSharpError::new_err(message)
}

/// Adds additional information to interpreter errors to make them more user-friendly.
/// when QIR generation fails.
fn map_qirgen_errors(errors: Vec<interpret::Error>) -> PyErr {
    let mut semantic = vec![];
    for error in errors {
        match &error {
            interpret::Error::Compile(_) => {
                // We've gotten this far with no compilation errors, so if we get one here
                // then the entry expression is invalid.
                let mut message = format_error(&error);
                writeln!(message).expect("write should succeed");
                writeln!(message, "failed to compile entry point.").expect("write should succeed");
                writeln!(
                    message,
                    "  help: check that the parameter types match the entry point signature"
                )
                .expect("write should succeed");

                semantic.push(message);
            }
            interpret::Error::PartialEvaluation(pe) => match pe.error() {
                qsc::partial_eval::Error::OutputResultLiteral(..) => {
                    let mut message = format_error(&error);
                    writeln!(message).expect("write should succeed");
                    writeln!(
                        message,
                        "  help: ensure all output registers have been measured into."
                    )
                    .expect("write should succeed");

                    semantic.push(message);
                }
                _ => {
                    semantic.push(format_error(&error));
                }
            },
            _ => {
                semantic.push(format_error(&error));
            }
        }
    }
    let message = semantic.into_iter().collect::<String>();
    QSharpError::new_err(message)
}

/// Estimates the resources required to run a QASM program
/// represented by the provided AST. The source map is used for
/// error reporting during compilation or runtime.
fn estimate_qasm(
    ast_package: Package,
    source_map: SourceMap,
    params: &str,
) -> Result<String, Vec<resource_estimator::Error>> {
    let mut interpreter = create_interpreter_from_ast(
        ast_package,
        source_map,
        Profile::Unrestricted,
        LanguageFeatures::default(),
        PackageType::Exe,
    )
    .map_err(into_estimation_errors)?;

    resource_estimator::estimate_entry(&mut interpreter, params)
}

/// Synthesizes a circuit for an `OpenQASM` program.
///
/// Note:
///     This call while exported is not intended to be used directly by the user.
///     It is intended to be used by the Python wrapper which will handle the
///     callbacks and other Python specific details.
///
/// Args:
///     source (str): An `OpenQASM` program. Alternatively, a callable can be provided,
///         which must be an already imported global callable.
///     `read_file` (Callable[[str], Tuple[str, str]]): A callable that reads a file and returns its content and path.
///     `list_directory` (Callable[[str], List[Dict[str, str]]]): A callable that lists the contents of a directory.
///     `resolve_path` (Callable[[str, str], str]): A callable that resolves a file path given a base path and a relative path.
///     `fetch_github` (Callable[[str, str, str, str], str]): A callable that fetches a file from GitHub.
///     **kwargs: Additional keyword arguments to pass to the execution.
///       - name (str): The name of the program. This is used as the entry point for the program.
///       - `search_path` (Optional[str]): The optional search path for resolving file references.
/// Returns:
///     Circuit: The synthesized circuit.
///
/// Raises:
///     `QasmError`: If there is an error generating, parsing, or analyzing the `OpenQASM` source.
///     `QSharpError`: If there is an error evaluating the program.
///     `QSharpError`: If there is an error synthesizing the circuit.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(
    signature = (source, read_file, list_directory, resolve_path, fetch_github, **kwargs)
)]
pub(crate) fn circuit_qasm_program(
    py: Python,
    source: &str,
    read_file: Option<PyObject>,
    list_directory: Option<PyObject>,
    resolve_path: Option<PyObject>,
    fetch_github: Option<PyObject>,
    kwargs: Option<Bound<'_, PyDict>>,
) -> PyResult<PyObject> {
    let kwargs = kwargs.unwrap_or_else(|| PyDict::new(py));

    let operation_name = get_operation_name(&kwargs)?;
    let search_path = get_search_path(&kwargs)?;

    let fs = create_filesystem_from_py(py, read_file, list_directory, resolve_path, fetch_github);
    let file_path = PathBuf::from_str(&search_path)
        .expect("from_str is infallible")
        .join("program.qasm");
    let project = fs.load_openqasm_project(&file_path, Some(Arc::<str>::from(source)));
    let ProjectType::OpenQASM(sources) = project.project_type else {
        return Err(QasmError::new_err(
            "Expected OpenQASM project, but got a different type".to_string(),
        ));
    };
    let res = qsc::qasm::semantic::parse_sources(&sources);

    let (package, source_map, signature) = compile_qasm_enriching_errors(
        res,
        &operation_name,
        ProgramType::File,
        OutputSemantics::ResourceEstimation,
        false,
    )?;

    let package_type = PackageType::Exe;
    let language_features = LanguageFeatures::default();
    let mut interpreter = create_interpreter_from_ast(
        package,
        source_map,
        TargetProfile::Unrestricted.into(),
        language_features,
        package_type,
    )
    .map_err(|errors| QSharpError::new_err(format_errors(errors)))?;

    let entry_expr = signature.create_entry_expr_from_params(String::new());
    interpreter
        .set_entry_expr(&entry_expr)
        .map_err(|errors| map_entry_compilation_errors(errors, &signature))?;

    match interpreter.circuit(CircuitEntryPoint::EntryExpr(entry_expr), false) {
        Ok(circuit) => crate::interpreter::Circuit(circuit).into_py_any(py),
        Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
    }
}

/// Converts a list of Q# errors into a list of resource estimator errors.
fn into_estimation_errors(errors: Vec<interpret::Error>) -> Vec<resource_estimator::Error> {
    errors
        .into_iter()
        .map(|error| resource_estimator::Error::Interpreter(error.clone()))
        .collect::<Vec<_>>()
}

/// Formats a list of QASM errors into a single string.
pub(crate) fn format_qasm_errors(errors: Vec<WithSource<qsc::qasm::error::Error>>) -> String {
    errors
        .into_iter()
        .map(|e| {
            let mut message = String::new();
            let report = miette::Report::new(e);
            write!(message, "{report:?}").expect("write should succeed");
            message
        })
        .collect::<String>()
}

/// Creates a `FileSystem` from the provided Python callbacks.
/// If any of the callbacks are missing, this will panic.
pub(crate) fn create_filesystem_from_py(
    py: Python,
    read_file: Option<PyObject>,
    list_directory: Option<PyObject>,
    resolve_path: Option<PyObject>,
    fetch_github: Option<PyObject>,
) -> impl FileSystem {
    file_system(
        py,
        read_file.expect("file system hooks should have been passed in with a read file callback"),
        list_directory
            .expect("file system hooks should have been passed in with a list directory callback"),
        resolve_path
            .expect("file system hooks should have been passed in with a resolve path callback"),
        fetch_github
            .expect("file system hooks should have been passed in with a fetch github callback"),
    )
}

/// Creates an `Interpreter` from the provided AST package and configuration.
fn create_interpreter_from_ast(
    ast_package: Package,
    source_map: SourceMap,
    profile: Profile,
    language_features: LanguageFeatures,
    package_type: PackageType,
) -> Result<Interpreter, Vec<interpret::Error>> {
    let capabilities = profile.into();
    let (stdid, mut store) = qsc::compile::package_store_with_stdlib(capabilities);
    let dependencies = vec![(PackageId::CORE, None), (stdid, None)];

    let (mut unit, errors) = qsc::compile::compile_ast(
        &store,
        &dependencies,
        ast_package,
        source_map,
        package_type,
        capabilities,
    );

    if !errors.is_empty() {
        return Err(into_errors(errors));
    }

    unit.expose();
    let source_package_id = store.insert(unit);

    interpret::Interpreter::with_package_store(
        false,
        store,
        source_package_id,
        capabilities,
        language_features,
        &dependencies,
    )
}

/// Sanitizes the name to ensure it is a valid identifier according
/// to the Q# specification. If the name is empty, returns "circuit".
pub(crate) fn sanitize_name<S: AsRef<str>>(name: S) -> String {
    let name = name.as_ref();
    if name.is_empty() {
        return "circuit".to_string();
    }

    let mut output = String::with_capacity(name.len());
    let c = name.chars().next().expect("name should not be empty");
    if c == '_' || c.is_alphabetic() {
        output.push(c);
    } else {
        // invalid first character, replace with '_'
        output.push('_');
    }
    output.extend(name.chars().skip(1).filter_map(|c| {
        if c == '-' {
            Some('_')
        } else if c == '_' || c.is_alphanumeric() {
            Some(c)
        } else {
            None
        }
    }));
    output
}

/// Extracts the search path from the kwargs dictionary.
/// If the search path is not present, returns an error.
/// Otherwise, returns the search path as a string.
pub(crate) fn get_search_path(kwargs: &Bound<'_, PyDict>) -> PyResult<String> {
    kwargs.get_item("search_path")?.map_or_else(
        || {
            Err(PyException::new_err(
                "Could not parse search path".to_string(),
            ))
        },
        |x| x.extract::<String>(),
    )
}

/// Extracts the program type from the kwargs dictionary.
pub(crate) fn get_program_type<D>(kwargs: &Bound<'_, PyDict>, default: D) -> PyResult<ProgramType>
where
    D: FnOnce() -> ProgramType,
{
    let target = kwargs
        .get_item("program_type")?
        .map_or_else(|| Ok(default()), |x| x.extract::<ProgramType>())?;
    Ok(target)
}

/// Extracts the output semantics from the kwargs dictionary.
pub(crate) fn get_output_semantics<D>(
    kwargs: &Bound<'_, PyDict>,
    default: D,
) -> PyResult<OutputSemantics>
where
    D: FnOnce() -> OutputSemantics,
{
    let target = kwargs
        .get_item("output_semantics")?
        .map_or_else(|| Ok(default()), |x| x.extract::<OutputSemantics>())?;
    Ok(target)
}

/// Extracts the name from the kwargs dictionary.
/// If the name is not present, returns "program".
/// Otherwise, returns the name after sanitizing it.
pub(crate) fn get_operation_name(kwargs: &Bound<'_, PyDict>) -> PyResult<String> {
    let name = kwargs
        .get_item("name")?
        .map_or_else(|| Ok("program".to_string()), |x| x.extract::<String>())?;

    // sanitize the name to ensure it is a valid identifier
    // When creating operation, we'll throw an error if the name is not a valid identifier
    // so that the user gets the exact name they expect, but here it's better to sanitize.
    Ok(sanitize_name(name))
}

/// Extracts the target profile from the kwargs dictionary.
/// If the target profile is not present, returns `TargetProfile::Unrestricted`.
/// Otherwise if not a valid `TargetProfile`, returns an error.
///
/// This also maps the `TargetProfile` exposed to Python to a `Profile`
/// used by the interpreter.
pub(crate) fn get_target_profile(kwargs: &Bound<'_, PyDict>) -> PyResult<Profile> {
    let target = kwargs.get_item("target_profile")?.map_or_else(
        || Ok(TargetProfile::Unrestricted),
        |x| x.extract::<TargetProfile>(),
    )?;
    Ok(target.into())
}

/// Extracts the shots from the kwargs dictionary.
/// If the shots are not present, or are not a valid usize, returns an error.
pub(crate) fn get_shots(kwargs: &Bound<'_, PyDict>) -> PyResult<usize> {
    kwargs.get_item("shots")?.map_or_else(
        || Err(PyException::new_err("Could not parse shots".to_string())),
        |x| x.extract::<usize>(),
    )
}

/// Extracts the seed from the kwargs dictionary.
/// If the seed is not present, or is not a valid u64, returns None.
pub(crate) fn get_seed(kwargs: &Bound<'_, PyDict>) -> Option<u64> {
    kwargs
        .get_item("seed")
        .ok()?
        .map_or_else(|| None::<u64>, |x| x.extract::<u64>().ok())
}

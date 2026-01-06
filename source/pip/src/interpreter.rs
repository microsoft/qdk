// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::doc_markdown,
    reason = "docstrings in this module conform to the python docstring format."
)]

pub(crate) mod data_interop;

use crate::{
    displayable_output::{DisplayableMatrix, DisplayableOutput, DisplayableState},
    fs::file_system,
    generic_estimator::register_generic_estimator_submodule,
    interop::{
        circuit_qasm_program, compile_qasm_program_to_qir, compile_qasm_to_qsharp,
        create_filesystem_from_py, get_operation_name, get_output_semantics, get_program_type,
        get_search_path, resource_estimate_qasm_program, run_qasm_program,
    },
    interpreter::data_interop::{
        PrimitiveKind, TypeIR, TypeKind, UdtFields, UdtIR, UdtValue, collect_udt_fields,
        pyobj_to_value, type_ir_from_qsharp_ty, value_to_pyobj,
    },
    noisy_simulator::register_noisy_simulator_submodule,
};
use miette::{Diagnostic, Report};
use num_bigint::BigUint;
use num_complex::Complex64;
use pyo3::{
    IntoPyObjectExt, create_exception,
    exceptions::{PyException, PyValueError},
    prelude::*,
    types::{PyDict, PyList, PyString, PyTuple, PyType},
};
use qsc::{
    LanguageFeatures, PackageType, SourceMap,
    error::WithSource,
    fir::{self},
    hir::ty::{Prim, Ty},
    interpret::{
        self, CircuitEntryPoint, PauliNoise, TaggedItem, Value,
        output::{Error, Receiver},
    },
    packages::BuildableProgram,
    project::{FileSystem, PackageCache, PackageGraphSources, ProjectType},
    qasm::{CompilerConfig, QubitSemantics, compiler::compile_to_qsharp_ast_with_config},
    target::Profile,
};

use resource_estimator::{
    self as re, estimate_call, estimate_expr, logical_counts_call, logical_counts_expr,
};
use std::{cell::RefCell, fmt::Write, path::PathBuf, rc::Rc, str::FromStr, sync::Arc};

/// If the classes are not Send, the Python interpreter
/// will not be able to use them in a separate thread.
///
/// This function is used to verify that the classes are Send.
/// The code will fail to compile if the classes are not Send.
///
/// ### Note
/// `QSharpError`, and `QasmError` are not `Send`, *BUT*
/// we return `QasmError::new_err` or `QSharpError::new_err` which
/// actually returns a `PyErr` that is `Send` and the args passed
/// into the `new_err` call must also impl `Send`.
/// Because of this, we don't need to check the `Send`-ness of
/// them. On the Python side, the `PyErr` is converted into the
/// corresponding exception.
fn verify_classes_are_sendable() {
    fn is_send<T: Send>() {}
    is_send::<OutputSemantics>();
    is_send::<ProgramType>();
    is_send::<TargetProfile>();
    is_send::<Result>();
    is_send::<Pauli>();
    is_send::<Output>();
    is_send::<StateDumpData>();
    is_send::<CircuitConfig>();
    is_send::<CircuitGenerationMethod>();
    is_send::<Circuit>();
    is_send::<UdtValue>();
    is_send::<UdtFields>();
    is_send::<TypeIR>();
    is_send::<TypeKind>();
    is_send::<PrimitiveKind>();
    is_send::<UdtIR>();
}

#[pymodule]
fn _native<'a>(py: Python<'a>, m: &Bound<'a, PyModule>) -> PyResult<()> {
    verify_classes_are_sendable();
    m.add_class::<OutputSemantics>()?;
    m.add_class::<ProgramType>()?;
    m.add_class::<TargetProfile>()?;
    m.add_class::<Interpreter>()?;
    m.add_class::<Result>()?;
    m.add_class::<Pauli>()?;
    m.add_class::<Output>()?;
    m.add_class::<StateDumpData>()?;
    m.add_class::<CircuitConfig>()?;
    m.add_class::<CircuitGenerationMethod>()?;
    m.add_class::<Circuit>()?;
    m.add_class::<GlobalCallable>()?;
    m.add_class::<UdtValue>()?;
    m.add_class::<TypeIR>()?;
    m.add_class::<TypeKind>()?;
    m.add_class::<PrimitiveKind>()?;
    m.add_class::<UdtIR>()?;
    m.add_function(wrap_pyfunction!(physical_estimates, m)?)?;
    m.add("QSharpError", py.get_type::<QSharpError>())?;
    register_noisy_simulator_submodule(py, m)?;
    register_generic_estimator_submodule(m)?;
    // QASM interop
    m.add("QasmError", py.get_type::<QasmError>())?;
    m.add_function(wrap_pyfunction!(resource_estimate_qasm_program, m)?)?;
    m.add_function(wrap_pyfunction!(run_qasm_program, m)?)?;
    m.add_function(wrap_pyfunction!(circuit_qasm_program, m)?)?;
    m.add_function(wrap_pyfunction!(compile_qasm_program_to_qir, m)?)?;
    m.add_function(wrap_pyfunction!(compile_qasm_to_qsharp, m)?)?;
    Ok(())
}

// This ordering must match the _native.pyi file.
#[derive(Clone, Copy, Default, PartialEq)]
#[pyclass(eq, eq_int, module = "qsharp._native")]
#[allow(non_camel_case_types)]
/// A Q# target profile.
///
/// A target profile describes the capabilities of the hardware or simulator
/// which will be used to run the Q# program.
pub(crate) enum TargetProfile {
    /// Target supports the minimal set of capabilities required to run a quantum program.
    ///
    /// This option maps to the Base Profile as defined by the QIR specification.
    #[default]
    Base,
    /// Target supports the Adaptive profile with the integer computation extension.
    ///
    /// This profile includes all of the required Adaptive Profile
    /// capabilities, as well as the optional integer computation
    /// extension defined by the QIR specification.
    Adaptive_RI,
    /// Target supports the Adaptive profile with integer & floating-point
    /// computation extensions.
    ///
    /// This profile includes all required Adaptive Profile and `Adaptive_RI`
    /// capabilities, as well as the optional floating-point computation
    /// extension defined by the QIR specification.
    Adaptive_RIF,
    /// Target supports the full set of capabilities required to run any Q# program.
    ///
    /// This option maps to the Full Profile as defined by the QIR specification.
    Unrestricted,
}

#[pymethods]
impl TargetProfile {
    #[new]
    // We need to define `new` so that instances of `TargetProfile` can be created by Python
    pub(crate) fn new() -> Self {
        Self::default()
    }

    // called and the returned object is pickled as the contents for the instance
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __getstate__(&self) -> PyResult<isize> {
        Ok(self.__pyo3__int__())
    }

    // called with the unpickled state and the instance is updated in place
    // This is what requires `new` to be implemented as we can't hydrate an
    // unininitialized instance in Python.
    fn __setstate__(&mut self, state: i32) -> PyResult<()> {
        (*self) = match state {
            0 => Self::Base,
            1 => Self::Adaptive_RI,
            2 => Self::Adaptive_RIF,
            3 => Self::Unrestricted,
            _ => return Err(PyValueError::new_err("invalid state")),
        };
        Ok(())
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        Into::<Profile>::into(*self).to_str().to_owned()
    }

    /// Creates a target profile from a string.
    /// :param value: The string to parse.
    /// :raises ValueError: If the string does not match any target profile.
    #[classmethod]
    #[allow(clippy::needless_pass_by_value)]
    fn from_str(_cls: &Bound<'_, PyType>, key: String) -> pyo3::PyResult<Self> {
        let profile = Profile::from_str(key.as_str())
            .map_err(|()| PyValueError::new_err(format!("{key} is not a valid target profile")))?;
        Ok(TargetProfile::from(profile))
    }
}

impl From<Profile> for TargetProfile {
    fn from(profile: Profile) -> Self {
        match profile {
            Profile::Base => TargetProfile::Base,
            Profile::AdaptiveRI => TargetProfile::Adaptive_RI,
            Profile::AdaptiveRIF => TargetProfile::Adaptive_RIF,
            Profile::Unrestricted => TargetProfile::Unrestricted,
        }
    }
}

impl From<TargetProfile> for Profile {
    fn from(profile: TargetProfile) -> Self {
        match profile {
            TargetProfile::Base => Profile::Base,
            TargetProfile::Adaptive_RI => Profile::AdaptiveRI,
            TargetProfile::Adaptive_RIF => Profile::AdaptiveRIF,
            TargetProfile::Unrestricted => Profile::Unrestricted,
        }
    }
}

// This ordering must match the _native.pyi file.
#[derive(Clone, Copy, Default, PartialEq)]
#[pyclass(eq, eq_int, module = "qsharp._native")]
#[allow(non_camel_case_types)]
/// Represents the output semantics for OpenQASM 3 compilation.
/// Each has implications on the output of the compilation
/// and the semantic checks that are performed.
pub(crate) enum OutputSemantics {
    /// The output is in Qiskit format meaning that the output
    /// is all of the classical registers, in reverse order
    /// in which they were added to the circuit with each
    /// bit within each register in reverse order.
    #[default]
    Qiskit,
    /// [OpenQASM 3 has two output modes](https://openqasm.com/language/directives.html#input-output)
    /// - If the programmer provides one or more `output` declarations, then
    ///     variables described as outputs will be returned as output.
    ///     The spec make no mention of endianness or order of the output.
    /// - Otherwise, assume all of the declared variables are returned as output.
    OpenQasm,
    /// No output semantics are applied. The entry point returns `Unit`.
    ResourceEstimation,
}

#[pymethods]
impl OutputSemantics {
    #[new]
    // We need to define `new` so that instances of `TargetProfile` can be created by Python
    pub(crate) fn new() -> Self {
        Self::default()
    }

    // called and the returned object is pickled as the contents for the instance
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __getstate__(&self) -> PyResult<isize> {
        Ok(self.__pyo3__int__())
    }

    // called with the unpickled state and the instance is updated in place
    // This is what requires `new` to be implemented as we can't hydrate an
    // unininitialized instance in Python.
    fn __setstate__(&mut self, state: i32) -> PyResult<()> {
        (*self) = match state {
            0 => Self::Qiskit,
            1 => Self::OpenQasm,
            2 => Self::ResourceEstimation,
            _ => return Err(PyValueError::new_err("invalid state")),
        };
        Ok(())
    }
}

impl From<OutputSemantics> for qsc::qasm::OutputSemantics {
    fn from(output_semantics: OutputSemantics) -> Self {
        match output_semantics {
            OutputSemantics::Qiskit => qsc::qasm::OutputSemantics::Qiskit,
            OutputSemantics::OpenQasm => qsc::qasm::OutputSemantics::OpenQasm,
            OutputSemantics::ResourceEstimation => qsc::qasm::OutputSemantics::ResourceEstimation,
        }
    }
}

// This ordering must match the _native.pyi file.
#[derive(Clone, Copy, Default, PartialEq)]
#[pyclass(eq, eq_int, module = "qsharp._native")]
#[allow(non_camel_case_types)]
/// Represents the type of compilation output to create
pub enum ProgramType {
    /// Creates an operation in a namespace as if the program is a standalone
    /// file. Inputs are lifted to the operation params. Output are lifted to
    /// the operation return type. The operation is marked as `@EntryPoint`
    /// as long as there are no input parameters.
    #[default]
    File,
    /// Programs are compiled to a standalone function. Inputs are lifted to
    /// the operation params. Output are lifted to the operation return type.
    Operation,
    /// Creates a list of statements from the program. This is useful for
    /// interactive environments where the program is a list of statements
    /// imported into the current scope.
    /// This is also useful for testing individual statements compilation.
    Fragments,
}

#[pymethods]
impl ProgramType {
    #[new]
    // We need to define `new` so that instances of `TargetProfile` can be created by Python
    pub(crate) fn new() -> Self {
        Self::default()
    }

    // called and the returned object is pickled as the contents for the instance
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __getstate__(&self) -> PyResult<isize> {
        Ok(self.__pyo3__int__())
    }

    // called with the unpickled state and the instance is updated in place
    // This is what requires `new` to be implemented as we can't hydrate an
    // unininitialized instance in Python.
    fn __setstate__(&mut self, state: i32) -> PyResult<()> {
        (*self) = match state {
            0 => Self::File,
            1 => Self::Operation,
            2 => Self::Fragments,
            _ => return Err(PyValueError::new_err("invalid state")),
        };
        Ok(())
    }
}

impl From<ProgramType> for qsc::qasm::ProgramType {
    fn from(output_semantics: ProgramType) -> Self {
        match output_semantics {
            ProgramType::File => qsc::qasm::ProgramType::File,
            ProgramType::Operation => qsc::qasm::ProgramType::Operation,
            ProgramType::Fragments => qsc::qasm::ProgramType::Fragments,
        }
    }
}

#[allow(clippy::struct_field_names)]
#[pyclass(unsendable)]
pub(crate) struct Interpreter {
    pub(crate) interpreter: interpret::Interpreter,
    /// The Python function to call to create a new function wrapping a callable invocation.
    pub(crate) make_callable: Option<PyObject>,
    /// The Python function to call to create a class representing a qsharp struct.
    pub(crate) make_class: Option<PyObject>,
    trace_circuit: bool,
}

thread_local! { static PACKAGE_CACHE: Rc<RefCell<PackageCache>> = Rc::default(); }

#[pymethods]
/// A Q# interpreter.
impl Interpreter {
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(signature = (target_profile, language_features=None, project_root=None, read_file=None, list_directory=None, resolve_path=None, fetch_github=None, make_callable=None, make_class=None, trace_circuit=None))]
    #[new]
    /// Initializes a new Q# interpreter.
    pub(crate) fn new(
        py: Python,
        target_profile: TargetProfile,
        language_features: Option<Vec<String>>,
        project_root: Option<String>,
        read_file: Option<PyObject>,
        list_directory: Option<PyObject>,
        resolve_path: Option<PyObject>,
        fetch_github: Option<PyObject>,
        make_callable: Option<PyObject>,
        make_class: Option<PyObject>,
        trace_circuit: Option<bool>,
    ) -> PyResult<Self> {
        let target = Into::<Profile>::into(target_profile).into();

        let language_features = LanguageFeatures::from_iter(language_features.unwrap_or_default());

        let package_cache = PACKAGE_CACHE.with(Clone::clone);

        let buildable_program = if let Some(project_root) = project_root {
            if let (Some(read_file), Some(list_directory), Some(resolve_path), Some(fetch_github)) =
                (read_file, list_directory, resolve_path, fetch_github)
            {
                let project =
                    file_system(py, read_file, list_directory, resolve_path, fetch_github)
                        .load_project(&PathBuf::from(project_root), Some(&package_cache))
                        .map_err(IntoPyErr::into_py_err)?;

                if !project.errors.is_empty() {
                    return Err(project.errors.into_py_err());
                }
                let ProjectType::QSharp(package_graph_sources) = project.project_type else {
                    unreachable!("Project type should be Q#")
                };
                BuildableProgram::new(target, package_graph_sources)
            } else {
                panic!("file system hooks should have been passed in with a manifest descriptor")
            }
        } else {
            let graph = PackageGraphSources::with_no_dependencies(
                Vec::default(),
                LanguageFeatures::from_iter(language_features),
                None,
            );
            BuildableProgram::new(target, graph)
        };

        let trace_circuit = trace_circuit.unwrap_or(false);
        let interpreter = if trace_circuit {
            interpret::Interpreter::with_circuit_trace(
                SourceMap::new(buildable_program.user_code.sources, None),
                PackageType::Lib,
                target,
                buildable_program.user_code.language_features,
                buildable_program.store,
                &buildable_program.user_code_dependencies,
                Default::default(),
            )
        } else {
            interpret::Interpreter::new(
                SourceMap::new(buildable_program.user_code.sources, None),
                PackageType::Lib,
                target,
                buildable_program.user_code.language_features,
                buildable_program.store,
                &buildable_program.user_code_dependencies,
            )
        }
        .map_err(|errors| QSharpError::new_err(format_errors(errors)))?;

        if let Some(make_callable) = &make_callable {
            // Add any global callables from the user source as Python functions to the environment.
            let exported_items = interpreter.source_globals();
            for (namespace, name, val) in exported_items {
                create_py_callable(py, make_callable, &namespace, &name, val)?;
            }
        }
        if let Some(make_class) = &make_class {
            // Add any global structs from the user source as Python classes to the environment.
            let exported_items = interpreter.source_types();
            for TaggedItem {
                item_id,
                name,
                namespace,
            } in exported_items
            {
                let ty = Ty::Udt(name.clone(), qsc::hir::Res::Item(item_id));
                create_py_class(&interpreter, py, make_class, &namespace, &name, &ty)?;
            }
        }
        Ok(Self {
            interpreter,
            make_callable,
            make_class,
            trace_circuit,
        })
    }

    /// Interprets Q# source code.
    ///
    /// :param input: The Q# source code to interpret.
    /// :param output_fn: A callback function that will be called with each output.
    ///
    /// :returns value: The value returned by the last statement in the input.
    ///
    /// :raises QSharpError: If there is an error interpreting the input.
    #[pyo3(signature=(input, callback=None))]
    fn interpret(
        &mut self,
        py: Python,
        input: &str,
        callback: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let mut receiver = OptionalCallbackReceiver { callback, py };
        match self.interpreter.eval_fragments(&mut receiver, input) {
            Ok(value) => {
                if let Some(make_callable) = &self.make_callable {
                    // Get any global callables from the evaluated input and add them to the environment. This will grab
                    // every callable that was defined in the input and by previous calls that added to the open package.
                    // This is safe because either the callable will be replaced with itself or a new callable with the
                    // same name will shadow the previous one, which is the expected behavior.
                    let new_items = self.interpreter.user_globals();
                    for (namespace, name, val) in new_items {
                        create_py_callable(py, make_callable, &namespace, &name, val)?;
                    }
                }
                if let Some(make_class) = &self.make_class {
                    // Get any global UDTs from the evaluated input and add them to the environment. This will grab
                    // every UDT that was defined in the input and by previous calls that added to the open package.
                    // This is safe because either the UDT will be replaced with itself or a new UDT with the
                    // same name will shadow the previous one, which is the expected behavior.
                    let new_items = self.interpreter.user_types();
                    for TaggedItem {
                        item_id,
                        name,
                        namespace,
                    } in new_items
                    {
                        let ty = Ty::Udt(name.clone(), qsc::hir::Res::Item(item_id));
                        create_py_class(&self.interpreter, py, make_class, &namespace, &name, &ty)?;
                    }
                }
                value_to_pyobj(&self.interpreter, py, &value)
            }
            Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
        }
    }

    /// Imports OpenQASM source code into the active Q# interpreter.
    ///
    /// Args:
    ///     source (str): An OpenQASM program or fragment.
    ///     output_fn: The function to handle the output of the execution.
    ///     read_file: A callable that reads a file and returns its content and path.
    ///     list_directory: A callable that lists the contents of a directory.
    ///     resolve_path: A callable that resolves a file path given a base path and a relative path.
    ///     fetch_github: A callable that fetches a file from GitHub.
    ///     **kwargs: Additional keyword arguments to pass to the execution.
    ///         - name (str): The name of the program. This is used as the entry point for the program.
    ///         - search_path (Optional[str]): The optional search path for resolving file references.
    ///         - output_semantics (OutputSemantics, optional): The output semantics for the compilation.
    ///         - program_type (ProgramType, optional): The type of program compilation to perform.
    ///
    /// Returns:
    ///     value: The value returned by the last statement in the source code.
    ///
    /// Raises:
    ///     QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
    ///     QSharpError: If there is an error compiling the program.
    ///     QSharpError: If there is an error evaluating the source code.
    #[pyo3(signature=(input, output_fn, read_file, list_directory, resolve_path, fetch_github, **kwargs))]
    #[allow(clippy::needless_pass_by_value)]
    #[allow(clippy::too_many_arguments)]
    fn import_qasm(
        &mut self,
        py: Python,
        input: &str,
        output_fn: Option<PyObject>,
        read_file: Option<PyObject>,
        list_directory: Option<PyObject>,
        resolve_path: Option<PyObject>,
        fetch_github: Option<PyObject>,
        kwargs: Option<Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let kwargs = kwargs.unwrap_or_else(|| PyDict::new(py));

        let operation_name = get_operation_name(&kwargs)?;
        let search_path = get_search_path(&kwargs)?;
        let program_ty = get_program_type(&kwargs, || ProgramType::Operation)?;
        let output_semantics = get_output_semantics(&kwargs, || OutputSemantics::OpenQasm)?;

        let fs =
            create_filesystem_from_py(py, read_file, list_directory, resolve_path, fetch_github);
        let file_path = PathBuf::from_str(&search_path)
            .expect("from_str is infallible")
            .join("program.qasm");
        let project = fs.load_openqasm_project(&file_path, Some(Arc::<str>::from(input)));
        let ProjectType::OpenQASM(sources) = project.project_type else {
            return Err(QasmError::new_err(
                "Expected OpenQASM project, but got a different type".to_string(),
            ));
        };

        let config = CompilerConfig::new(
            QubitSemantics::Qiskit,
            output_semantics.into(),
            program_ty.into(),
            Some(operation_name.into()),
            None,
        );
        let res = qsc::qasm::semantic::parse_sources(&sources);
        let unit = compile_to_qsharp_ast_with_config(res, config);
        let (sources, errors, package, _, _) = unit.into_tuple();

        if !errors.is_empty() {
            let errors = errors
                .iter()
                .map(|e| {
                    use qsc::compile::ErrorKind;
                    use qsc::interpret::Error;
                    let error = e.error().clone();
                    let kind = ErrorKind::OpenQasm(error);
                    let v = WithSource::from_map(&sources, kind);
                    Error::Compile(v)
                })
                .collect();
            return Err(QSharpError::new_err(format_errors(errors)));
        }
        let mut receiver = OptionalCallbackReceiver {
            callback: output_fn,
            py,
        };

        match self
            .interpreter
            .eval_ast_fragments(&mut receiver, input, package)
        {
            Ok(value) => {
                if let Some(make_callable) = &self.make_callable {
                    // Get any global callables from the evaluated input and add them to the environment. This will grab
                    // every callable that was defined in the input and by previous calls that added to the open package.
                    // This is safe because either the callable will be replaced with itself or a new callable with the
                    // same name will shadow the previous one, which is the expected behavior.
                    let new_items = self.interpreter.user_globals();
                    for (namespace, name, val) in new_items {
                        create_py_callable(py, make_callable, &namespace, &name, val)?;
                    }
                }
                value_to_pyobj(&self.interpreter, py, &value)
            }
            Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
        }
    }

    /// Sets the quantum seed for the interpreter.
    #[pyo3(signature=(seed=None))]
    fn set_quantum_seed(&mut self, seed: Option<u64>) {
        self.interpreter.set_quantum_seed(seed);
    }

    /// Sets the classical seed for the interpreter.
    #[pyo3(signature=(seed=None))]
    fn set_classical_seed(&mut self, seed: Option<u64>) {
        self.interpreter.set_classical_seed(seed);
    }

    /// Dumps the quantum state of the interpreter.
    /// Returns a tuple of (amplitudes, num_qubits), where amplitudes is a dictionary from integer indices to
    /// pairs of real and imaginary amplitudes.
    fn dump_machine(&mut self) -> StateDumpData {
        let (state, qubit_count) = self.interpreter.get_quantum_state();
        StateDumpData(DisplayableState(state, qubit_count))
    }

    /// Dumps a circuit showing the current state of the simulator.
    ///
    /// This circuit will contain the gates that have been applied
    /// in the simulator up to the current point.
    ///
    /// Requires the interpreter to be initialized with `trace_circuit=True`.
    fn dump_circuit(&mut self, py: Python) -> PyResult<PyObject> {
        if !self.trace_circuit {
            return Err(QSharpError::new_err(
                "to enable circuit dumping, the interpreter must be created with trace_circuit=True",
            ));
        }
        Circuit(self.interpreter.get_circuit()).into_py_any(py)
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(entry_expr=None, callback=None, noise=None, qubit_loss=None, callable=None, args=None))]
    fn run(
        &mut self,
        py: Python,
        entry_expr: Option<&str>,
        callback: Option<PyObject>,
        noise: Option<(f64, f64, f64)>,
        qubit_loss: Option<f64>,
        callable: Option<GlobalCallable>,
        args: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let mut receiver = OptionalCallbackReceiver { callback, py };

        let noise = match noise {
            None => None,
            Some((px, py, pz)) => match PauliNoise::from_probabilities(px, py, pz) {
                Ok(noise_struct) => Some(noise_struct),
                Err(error_message) => return Err(PyException::new_err(error_message)),
            },
        };

        let result = match callable {
            Some(callable) => {
                let (input_ty, output_ty) = self
                    .interpreter
                    .global_callable_ty(&callable.0)
                    .ok_or(QSharpError::new_err("callable not found"))?;
                let args = args_to_values(&self.interpreter, py, args, &input_ty, &output_ty)?;

                self.interpreter.invoke_with_noise(
                    &mut receiver,
                    callable.0,
                    args,
                    noise,
                    qubit_loss,
                )
            }
            _ => self
                .interpreter
                .run(&mut receiver, entry_expr, noise, qubit_loss),
        };

        match result {
            Ok(value) => value_to_pyobj(&self.interpreter, py, &value),
            Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
        }
    }

    #[pyo3(signature=(callable, args=None, callback=None))]
    fn invoke(
        &mut self,
        py: Python,
        callable: GlobalCallable,
        args: Option<PyObject>,
        callback: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let mut receiver = OptionalCallbackReceiver { callback, py };
        let (input_ty, output_ty) = self
            .interpreter
            .global_callable_ty(&callable.0)
            .ok_or(QSharpError::new_err("callable not found"))?;

        let args = args_to_values(&self.interpreter, py, args, &input_ty, &output_ty)?;

        match self.interpreter.invoke(&mut receiver, callable.0, args) {
            Ok(value) => value_to_pyobj(&self.interpreter, py, &value),
            Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
        }
    }

    #[pyo3(signature=(entry_expr=None, callable=None, args=None))]
    fn qir(
        &mut self,
        py: Python,
        entry_expr: Option<&str>,
        callable: Option<GlobalCallable>,
        args: Option<PyObject>,
    ) -> PyResult<String> {
        if let Some(entry_expr) = entry_expr {
            match self.interpreter.qirgen(entry_expr) {
                Ok(qir) => Ok(qir),
                Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
            }
        } else {
            let callable = callable.ok_or_else(|| {
                QSharpError::new_err("either entry_expr or callable must be specified")
            })?;
            let (input_ty, output_ty) = self
                .interpreter
                .global_callable_ty(&callable.0)
                .ok_or(QSharpError::new_err("callable not found"))?;

            let args = args_to_values(&self.interpreter, py, args, &input_ty, &output_ty)?;
            match self.interpreter.qirgen_from_callable(&callable.0, args) {
                Ok(qir) => Ok(qir),
                Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
            }
        }
    }

    /// Synthesizes a circuit for a Q# program. Either an entry
    /// expression or an operation must be provided.
    ///
    /// :param config: Circuit generation options.
    ///
    /// :param entry_expr: An entry expression.
    ///
    /// :param operation: The operation to synthesize. This can be a name of
    /// an operation of a lambda expression. The operation must take only
    /// qubits or arrays of qubits as parameters.
    ///
    /// :param callable: A callable to synthesize.
    ///
    /// :param args: The arguments to pass to the callable.
    ///
    /// :raises QSharpError: If there is an error synthesizing the circuit.
    #[pyo3(signature=(config, entry_expr=None,*, operation=None, callable=None, args=None))]
    fn circuit(
        &mut self,
        py: Python,
        config: &CircuitConfig,
        entry_expr: Option<String>,
        operation: Option<String>,
        callable: Option<GlobalCallable>,
        args: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let entrypoint = match (entry_expr, operation, callable) {
            (Some(entry_expr), None, None) => CircuitEntryPoint::EntryExpr(entry_expr),
            (None, Some(operation), None) => CircuitEntryPoint::Operation(operation),
            (None, None, Some(callable)) => {
                let (input_ty, output_ty) = self
                    .interpreter
                    .global_callable_ty(&callable.0)
                    .ok_or(QSharpError::new_err("callable not found"))?;
                let args = args_to_values(&self.interpreter, py, args, &input_ty, &output_ty)?;
                CircuitEntryPoint::Callable(callable.0, args)
            }
            _ => {
                return Err(PyException::new_err(
                    "either entry_expr or operation must be specified",
                ));
            }
        };

        let mut tracer_config = qsc::circuit::TracerConfig::default();
        if let Some(max_ops) = config.max_operations {
            tracer_config.max_operations = max_ops;
        }
        if let Some(locations) = config.source_locations {
            tracer_config.source_locations = locations;
        }
        if let Some(group_by_scope) = config.group_by_scope {
            tracer_config.group_by_scope = group_by_scope;
        }

        let generation_method = if let Some(generation_method) = config.generation_method {
            generation_method.into()
        } else {
            qsc::interpret::CircuitGenerationMethod::ClassicalEval
        };

        match self
            .interpreter
            .circuit(entrypoint, generation_method, tracer_config)
        {
            Ok(circuit) => Circuit(circuit).into_py_any(py),
            Err(errors) => Err(QSharpError::new_err(format_errors(errors))),
        }
    }

    #[pyo3(signature=(job_params, entry_expr=None, callable=None, args=None))]
    fn estimate(
        &mut self,
        py: Python,
        job_params: &str,
        entry_expr: Option<&str>,
        callable: Option<GlobalCallable>,
        args: Option<PyObject>,
    ) -> PyResult<String> {
        let results = if let Some(entry_expr) = entry_expr {
            estimate_expr(&mut self.interpreter, entry_expr, job_params)
        } else {
            let callable = callable.ok_or_else(|| {
                QSharpError::new_err("either entry_expr or callable must be specified")
            })?;
            let (input_ty, output_ty) = self
                .interpreter
                .global_callable_ty(&callable.0)
                .ok_or(QSharpError::new_err("callable not found"))?;
            let args = args_to_values(&self.interpreter, py, args, &input_ty, &output_ty)?;
            estimate_call(&mut self.interpreter, callable.0, args, job_params)
        };
        match results {
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

    #[pyo3(signature=(entry_expr=None, callable=None, args=None))]
    fn logical_counts<'a>(
        &mut self,
        py: Python<'a>,
        entry_expr: Option<&str>,
        callable: Option<GlobalCallable>,
        args: Option<PyObject>,
    ) -> PyResult<Bound<'a, PyDict>> {
        let results = if let Some(entry_expr) = entry_expr {
            logical_counts_expr(&mut self.interpreter, entry_expr)
        } else {
            let callable = callable.ok_or_else(|| {
                QSharpError::new_err("either entry_expr or callable must be specified")
            })?;
            let (input_ty, output_ty) = self
                .interpreter
                .global_callable_ty(&callable.0)
                .ok_or(QSharpError::new_err("callable not found"))?;
            let args = args_to_values(&self.interpreter, py, args, &input_ty, &output_ty)?;
            logical_counts_call(&mut self.interpreter, callable.0, args)
        };
        match results {
            Ok(counts) => {
                let dict = PyDict::new(py);
                dict.set_item("numQubits", counts.num_qubits)?;
                dict.set_item("tCount", counts.t_count)?;
                dict.set_item("rotationCount", counts.rotation_count)?;
                dict.set_item("rotationDepth", counts.rotation_depth)?;
                dict.set_item("cczCount", counts.ccz_count)?;
                dict.set_item("ccixCount", counts.ccix_count)?;
                dict.set_item("measurementCount", counts.measurement_count)?;
                Ok(dict)
            }
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
}

fn args_to_values(
    ctx: &interpret::Interpreter,
    py: Python,
    args: Option<PyObject>,
    input_ty: &Ty,
    output_ty: &Ty,
) -> PyResult<Value> {
    // If the types are not supported, we can't convert the arguments or return value.
    // Check this before trying to convert the arguments, and return an error if the types are not supported.
    if let Some(ty) = first_unsupported_interop_ty(ctx, input_ty) {
        return Err(QSharpError::new_err(format!(
            "unsupported input type: `{ty}`"
        )));
    }
    if let Some(ty) = first_unsupported_interop_ty(ctx, output_ty) {
        return Err(QSharpError::new_err(format!(
            "unsupported output type: `{ty}`"
        )));
    }

    // Convert the Python arguments to Q# values, treating None as an empty tuple aka `Unit`.
    if matches!(&input_ty, Ty::Tuple(tup) if tup.is_empty()) {
        // Special case for unit, where args should be None
        if args.is_some() {
            return Err(QSharpError::new_err("expected no arguments"));
        }
        Ok(Value::unit())
    } else {
        let Some(args) = args else {
            return Err(QSharpError::new_err(format!(
                "expected arguments of type `{input_ty}`"
            )));
        };
        // This conversion will produce errors if the types don't match or can't be converted.
        Ok(pyobj_to_value(ctx, py, &args, input_ty)?)
    }
}

/// Finds any Q# type recursively that does not support interop with Python, meaning our code cannot convert it back and forth
/// across the interop boundary.
fn first_unsupported_interop_ty<'ctx, 'ty>(
    ctx: &'ctx interpret::Interpreter,
    ty: &'ty Ty,
) -> Option<&'ctx Ty>
where
    'ty: 'ctx,
{
    match ty {
        Ty::Prim(prim_ty) => match prim_ty {
            Prim::Pauli
            | Prim::BigInt
            | Prim::Bool
            | Prim::Double
            | Prim::Int
            | Prim::String
            | Prim::Result => None,
            Prim::Qubit | Prim::Range | Prim::RangeTo | Prim::RangeFrom | Prim::RangeFull => {
                Some(ty)
            }
        },
        Ty::Tuple(tup) => tup
            .iter()
            .find(|t| first_unsupported_interop_ty(ctx, t).is_some()),
        Ty::Array(ty) => first_unsupported_interop_ty(ctx, ty),
        Ty::Udt(_, res) => {
            let qsc::hir::Res::Item(item_id) = res else {
                panic!("Udt should be an item");
            };
            let (udt, _) = ctx.udt_ty_from_item_id(item_id);

            let Ok(fields) = collect_udt_fields(udt) else {
                return Some(ty);
            };

            for field in fields {
                if let Some(ty) = first_unsupported_interop_ty(ctx, field.1) {
                    return Some(ty);
                }
            }

            None
        }
        Ty::Arrow(..) | Ty::Infer(..) | Ty::Param { .. } | Ty::Err => Some(ty),
    }
}

#[pyfunction]
pub fn physical_estimates(logical_resources: &str, job_params: &str) -> PyResult<String> {
    match re::estimate_physical_resources_from_json(logical_resources, job_params) {
        Ok(estimates) => Ok(estimates),
        Err(error) => Err(QSharpError::new_err(error.to_string())),
    }
}

create_exception!(
    module,
    QSharpError,
    pyo3::exceptions::PyException,
    "An error returned from the Q# interpreter."
);

create_exception!(
    module,
    QasmError,
    pyo3::exceptions::PyException,
    "An error returned from the OpenQASM parser."
);

pub(crate) fn format_errors(errors: Vec<interpret::Error>) -> String {
    errors
        .into_iter()
        .map(|e| format_error(&e))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_error(e: &interpret::Error) -> String {
    let mut message = String::new();
    if let Some(stack_trace) = e.stack_trace() {
        write!(message, "{stack_trace}").expect("write should succeed");
    }
    let additional_help = python_help(e);
    let report = Report::new(e.clone());
    write!(message, "{report:?}")
        .unwrap_or_else(|err| panic!("writing error failed: {err} error was: {e:?}"));
    if let Some(additional_help) = additional_help {
        writeln!(message, "{additional_help}").expect("write should succeed");
    }
    message
}

/// Additional help text for an error specific to the Python module
fn python_help(error: &interpret::Error) -> Option<String> {
    if matches!(error, interpret::Error::UnsupportedRuntimeCapabilities) {
        Some("Unsupported target profile. Initialize Q# by running `qsharp.init(target_profile=qsharp.TargetProfile.Base)` before performing code generation.".into())
    } else {
        None
    }
}

#[pyclass]
pub(crate) struct Output(DisplayableOutput);

#[pymethods]
/// An output returned from the Q# interpreter.
/// Outputs can be a state dumps or messages. These are normally printed to the console.
impl Output {
    fn __repr__(&self) -> String {
        match &self.0 {
            DisplayableOutput::State(state) => state.to_plain(),
            DisplayableOutput::Matrix(matrix) => matrix.to_plain(),
            DisplayableOutput::Message(msg) => msg.clone(),
        }
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn _repr_markdown_(&self) -> Option<String> {
        match &self.0 {
            DisplayableOutput::State(state) => {
                let latex = if let Some(latex) = state.to_latex() {
                    format!("\n\n{latex}")
                } else {
                    String::default()
                };
                Some(format!("{}{latex}", state.to_html()))
            }
            DisplayableOutput::Message(_) => None,
            DisplayableOutput::Matrix(matrix) => Some(matrix.to_latex()),
        }
    }

    fn state_dump(&self) -> Option<StateDumpData> {
        match &self.0 {
            DisplayableOutput::State(state) => Some(StateDumpData(state.clone())),
            DisplayableOutput::Matrix(_) | DisplayableOutput::Message(_) => None,
        }
    }

    fn is_state_dump(&self) -> bool {
        matches!(&self.0, DisplayableOutput::State(_))
    }

    fn is_matrix(&self) -> bool {
        matches!(&self.0, DisplayableOutput::Matrix(_))
    }

    fn is_message(&self) -> bool {
        matches!(&self.0, DisplayableOutput::Message(_))
    }
}

#[pyclass]
/// Captured simulation state dump.
pub(crate) struct StateDumpData(pub(crate) DisplayableState);

#[pymethods]
impl StateDumpData {
    fn get_dict<'a>(&self, py: Python<'a>) -> PyResult<Bound<'a, PyDict>> {
        let dict = rustc_hash::FxHashMap::from_iter(self.0.0.clone());
        dict.into_pyobject(py)
    }

    #[getter]
    fn get_qubit_count(&self) -> usize {
        self.0.1
    }

    fn __len__(&self) -> usize {
        self.0.0.len()
    }

    fn __repr__(&self) -> String {
        self.0.to_plain()
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn _repr_markdown_(&self) -> String {
        let latex = if let Some(latex) = self.0.to_latex() {
            format!("\n\n{latex}")
        } else {
            String::default()
        };
        format!("{}{latex}", self.0.to_html())
    }

    fn _repr_latex_(&self) -> Option<String> {
        self.0.to_latex()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[pyclass(eq, eq_int, ord)]
/// A Q# measurement result.
pub(crate) enum Result {
    Zero,
    One,
    Loss,
}

impl From<Result> for qsc::interpret::Result {
    fn from(value: Result) -> Self {
        match value {
            Result::Loss => qsc::interpret::Result::Loss,
            Result::One | Result::Zero => qsc::interpret::Result::Val(value == Result::One),
        }
    }
}

#[pymethods]
impl Result {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        match self {
            Result::Zero => "Zero".to_owned(),
            Result::One => "One".to_owned(),
            Result::Loss => "Loss".to_owned(),
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        self.__repr__()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __hash__(&self) -> u32 {
        match self {
            Result::Zero => 0,
            Result::One => 1,
            Result::Loss => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
#[pyclass(eq, eq_int)]
/// A Q# Pauli operator.
pub(crate) enum Pauli {
    I,
    X,
    Y,
    Z,
}

impl From<Pauli> for fir::Pauli {
    fn from(value: Pauli) -> Self {
        match value {
            Pauli::I => fir::Pauli::I,
            Pauli::X => fir::Pauli::X,
            Pauli::Y => fir::Pauli::Y,
            Pauli::Z => fir::Pauli::Z,
        }
    }
}

pub(crate) struct OptionalCallbackReceiver<'a> {
    pub(crate) callback: Option<PyObject>,
    pub(crate) py: Python<'a>,
}

impl Receiver for OptionalCallbackReceiver<'_> {
    fn state(
        &mut self,
        state: Vec<(BigUint, Complex64)>,
        qubit_count: usize,
    ) -> core::result::Result<(), Error> {
        if let Some(callback) = &self.callback {
            let out = DisplayableOutput::State(DisplayableState(state, qubit_count));
            callback
                .call1(
                    self.py,
                    PyTuple::new(
                        self.py,
                        &[Py::new(self.py, Output(out)).expect("should be able to create output")],
                    )
                    .map_err(|_| Error)?,
                )
                .map_err(|_| Error)?;
        }
        Ok(())
    }

    fn matrix(&mut self, matrix: Vec<Vec<Complex64>>) -> std::result::Result<(), Error> {
        if let Some(callback) = &self.callback {
            let out = DisplayableOutput::Matrix(DisplayableMatrix(matrix));
            callback
                .call1(
                    self.py,
                    PyTuple::new(
                        self.py,
                        &[Py::new(self.py, Output(out)).expect("should be able to create output")],
                    )
                    .map_err(|_| Error)?,
                )
                .map_err(|_| Error)?;
        }
        Ok(())
    }

    fn message(&mut self, msg: &str) -> core::result::Result<(), Error> {
        if let Some(callback) = &self.callback {
            let out = DisplayableOutput::Message(msg.to_owned());
            callback
                .call1(
                    self.py,
                    PyTuple::new(
                        self.py,
                        &[Py::new(self.py, Output(out)).expect("should be able to create output")],
                    )
                    .map_err(|_| Error)?,
                )
                .map_err(|_| Error)?;
        }
        Ok(())
    }
}

#[pyclass]
pub(crate) struct CircuitConfig {
    #[pyo3(get, set)]
    pub(crate) max_operations: Option<usize>,
    #[pyo3(get, set)]
    pub(crate) generation_method: Option<CircuitGenerationMethod>,
    #[pyo3(get, set)]
    pub(crate) source_locations: Option<bool>,
    #[pyo3(get, set)]
    pub(crate) group_by_scope: Option<bool>,
}

#[pymethods]
impl CircuitConfig {
    #[new]
    #[pyo3(signature=(*,max_operations=None, generation_method=None, source_locations=None, group_by_scope=None))]
    fn new(
        max_operations: Option<usize>,
        generation_method: Option<CircuitGenerationMethod>,
        source_locations: Option<bool>,
        group_by_scope: Option<bool>,
    ) -> Self {
        Self {
            max_operations,
            generation_method,
            source_locations,
            group_by_scope,
        }
    }
}

#[pyclass]
#[derive(Clone, Copy)]
pub(crate) enum CircuitGenerationMethod {
    ClassicalEval,
    Simulate,
}

impl From<CircuitGenerationMethod> for qsc::interpret::CircuitGenerationMethod {
    fn from(value: CircuitGenerationMethod) -> Self {
        match value {
            CircuitGenerationMethod::ClassicalEval => {
                qsc::interpret::CircuitGenerationMethod::ClassicalEval
            }
            CircuitGenerationMethod::Simulate => qsc::interpret::CircuitGenerationMethod::Simulate,
        }
    }
}

#[pyclass]
pub(crate) struct Circuit(pub qsc::circuit::Circuit);

#[pymethods]
impl Circuit {
    fn __repr__(&self) -> String {
        // Disable rendering source locations for user-facing string representation,
        // as they make the circuit look cluttered.
        self.0.display_basic().to_string()
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn json(&self, _py: Python) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyException::new_err(e.to_string()))
    }
}

trait IntoPyErr {
    fn into_py_err(self) -> PyErr;
}

impl IntoPyErr for Report {
    fn into_py_err(self) -> PyErr {
        PyException::new_err(format!("{self:?}"))
    }
}

impl<E> IntoPyErr for Vec<E>
where
    E: Diagnostic + Send + Sync + 'static,
{
    fn into_py_err(self) -> PyErr {
        let mut message = String::new();
        for diag in self {
            let report = Report::new(diag);
            writeln!(message, "{report:?}").expect("string should be writable");
        }
        PyException::new_err(message)
    }
}

#[pyclass(unsendable)]
#[derive(Clone)]
struct GlobalCallable(Value);

impl From<Value> for GlobalCallable {
    fn from(val: Value) -> Self {
        match val {
            val @ Value::Global(..) => GlobalCallable(val),
            _ => panic!("expected global callable"),
        }
    }
}

impl From<GlobalCallable> for Value {
    fn from(val: GlobalCallable) -> Self {
        val.0
    }
}

/// Create a Python callable from a Q# callable and adds it to the given environment.
fn create_py_callable(
    py: Python,
    make_callable: &PyObject,
    namespace: &[Rc<str>],
    name: &str,
    val: Value,
) -> PyResult<()> {
    if namespace.is_empty() && name == "<lambda>" {
        // We don't want to bind auto-generated lambda callables.
        return Ok(());
    }

    let args = (
        Py::new(py, GlobalCallable::from(val)).expect("should be able to create callable"), // callable id
        PyList::new(py, namespace.iter().map(ToString::to_string))?, // namespace as string array
        PyString::new(py, name),                                     // name of callable
    );

    // Call into the Python layer to create the function wrapping the callable invocation.
    make_callable.call1(py, args)?;

    Ok(())
}

/// Create a Python class from a Q# type and adds it to the given environment.
fn create_py_class(
    ctx: &interpret::Interpreter,
    py: Python,
    make_class: &PyObject,
    namespace: &[Rc<str>],
    name: &str,
    ty: &Ty,
) -> PyResult<()> {
    let Some(type_ir) = type_ir_from_qsharp_ty(ctx, ty) else {
        // If the UDT can't be expressed in Python, we don't want to raise
        // an error, instead we just don't define that type in `qsharp.code.*`.
        return Ok(());
    };

    let args = (
        Py::new(py, type_ir).expect("should be able to create callable"), // callable id
        PyList::new(py, namespace.iter().map(ToString::to_string))?, // namespace as string array
        PyString::new(py, name),                                     // name of callable
    );

    // Call into the Python layer to create the function wrapping the callable invocation.
    make_class.call1(py, args)?;

    Ok(())
}

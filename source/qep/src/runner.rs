// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod runtime;
#[cfg(test)]
mod tests;

use crate::QirBackend;
use inkwell::{
    OptimizationLevel,
    attributes::AttributeLoc,
    builder::BuilderError,
    context::Context,
    memory_buffer::MemoryBuffer,
    module::Module,
    targets::{InitializationConfig, Target, TargetMachine},
    values::{
        BasicMetadataValueEnum, CallSiteValue, FunctionValue, InstructionOpcode, PhiValue,
        PointerValue,
    },
};
use runtime::ExecutionState;
use std::{ffi::c_void, path::Path};

/// A value in the QIR output-recording stream.
///
/// Array and tuple records introduce the given number of elements; subsequent
/// records contain those elements, in the order emitted by the program.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputValue {
    Array(u64),
    Tuple(u64),
    Result(bool),
    Bool(bool),
    Int(i64),
    Double(f64),
}

/// An output value and its optional QIR label.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputRecord {
    pub value: OutputValue,
    pub label: Option<String>,
}

/// The output from one execution of a QIR entry point.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    pub records: Vec<OutputRecord>,
    /// The entry point's `i64` return value, or `None` for a `void` entry point.
    pub return_value: Option<i64>,
}

/// Execute a `.ll` or `.bc` file once using the supplied backend.
///
/// The module must contain exactly one defined, parameterless entry point
/// marked `entry_point` or `EntryPoint`, returning `void` or `i64`. Its
/// `required_num_qubits` and `required_num_results` attributes specify the
/// static ID bounds. Measurements issue backend operations into result slots,
/// without reading their values. Explicit result reads and result output
/// recording resolve values through the backend's `read_result` method.
/// Reading a slot not issued or written in this invocation is an error.
/// Both QIR 1 typed pointers and QIR 2 opaque pointers are
/// accepted by the LLVM parser.
///
/// Backend errors and unwinding panics stop execution and are returned as
/// errors. The backend may have been partially updated; changes are not rolled
/// back. No backend state is reset implicitly between invocations.
///
/// Supported runtime calls are initialization, result read/write, and
/// array/tuple/result/bool/int/double output recording. Other external calls
/// (including LLVM intrinsics) are rejected rather than resolved from the host.
///
/// # Example
///
/// ```no_run
/// use qep::{NoopBackend, run_qir};
///
/// let mut backend = NoopBackend::default();
/// // SAFETY: This file was produced by a trusted compiler and meets the ABI contract.
/// let output = unsafe { run_qir("program.ll", &mut backend) }?;
/// println!("{:?}", output.records);
/// # Ok::<(), String>(())
/// ```
///
/// # Safety
///
/// QIR is executed as native machine code, not sandboxed or fully validated.
/// The caller must supply trusted code with valid memory accesses and ABI use,
/// including valid null-terminated UTF-8 output-label pointers when non-null.
/// The program must not retain or asynchronously invoke runner callbacks.
/// Only synchronous execution on a 64-bit host is supported.
pub unsafe fn run_qir(
    path: impl AsRef<Path>,
    backend: &mut impl QirBackend,
) -> Result<RunOutput, String> {
    let context = Context::create();
    let path = path.as_ref();
    let module = match path.extension().and_then(|ext| ext.to_str()) {
        Some("ll") => MemoryBuffer::create_from_file(path)
            .and_then(|buffer| context.create_module_from_ir(buffer)),
        Some("bc") => Module::parse_bitcode_from_path(path, &context),
        _ => return Err("Expected a QIR file with extension .ll or .bc".into()),
    }
    .map_err(|error| format!("Failed to load QIR: {error}"))?;
    // SAFETY: The caller supplies trusted QIR with the documented ABI.
    unsafe { execute(&context, &module, backend) }
}

/// Validates and executes a loaded QIR module against a backend.
unsafe fn execute<'ctx, B: QirBackend>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    backend: &mut B,
) -> Result<RunOutput, String> {
    if usize::BITS != 64 {
        return Err("QEP requires a 64-bit host".into());
    }
    module
        .verify()
        .map_err(|error| format!("Invalid QIR: {error}"))?;
    validate_module(module)?;
    let functions: Vec<_> = module
        .get_functions()
        .filter(|function| function.count_basic_blocks() != 0)
        .collect();
    let entry = entry_point(context, &functions)?;
    let mut state = Box::new(ExecutionState::new(
        backend,
        resource_count(entry, "required_num_qubits")?,
        resource_count(entry, "required_num_results")?,
    )?);
    let state_ptr = (&raw mut *state).cast::<c_void>();
    let context_ptr = context
        .i64_type()
        .const_int(state_ptr as usize as u64, false)
        .const_to_pointer(context.ptr_type(Default::default()));

    Target::initialize_native(&InitializationConfig::default())?;
    module.set_triple(&TargetMachine::get_default_triple());
    let engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .map_err(|error| format!("Failed to create QIR JIT: {error}"))?;

    let bindings = runtime::bindings::<B>(context);
    for function in module.get_functions().collect::<Vec<_>>() {
        if function.count_basic_blocks() != 0 {
            continue;
        }
        let name = function.get_name().to_string_lossy();
        let binding = bindings
            .iter()
            .find(|binding| binding.name == name)
            .ok_or_else(|| format!("Unsupported QIR function: {name}"))?;
        if function.get_type() != binding.ty {
            return Err(format!("Unsupported signature for QIR function: {name}"));
        }
        let native = runtime::bind(context, module, function, binding.ty, context_ptr)
            .map_err(|error| error.to_string())?;
        engine.add_global_mapping(&native, binding.address);
    }

    let has_error = module.add_function(
        "__qep_has_error",
        context
            .bool_type()
            .fn_type(&[context.ptr_type(Default::default()).into()], false),
        None,
    );
    engine.add_global_mapping(&has_error, runtime::has_error::<B> as *const () as usize);
    for function in functions {
        instrument_errors(context, function, has_error, context_ptr)
            .map_err(|error| error.to_string())?;
    }
    module
        .verify()
        .map_err(|error| format!("Invalid instrumented QIR: {error}"))?;

    let name = entry
        .get_name()
        .to_str()
        .map_err(|error| error.to_string())?;
    // SAFETY: Entry signatures were checked, callbacks retain no references,
    // and state and engine remain alive until execution finishes.
    let return_value = unsafe {
        if entry.get_type().get_return_type().is_some() {
            Some(
                engine
                    .get_function::<unsafe extern "C" fn() -> i64>(name)
                    .map_err(|error| error.to_string())?
                    .call(),
            )
        } else {
            engine
                .get_function::<unsafe extern "C" fn()>(name)
                .map_err(|error| error.to_string())?
                .call();
            None
        }
    };
    if let Some(error) = state.error.take() {
        return Err(error);
    }
    Ok(RunOutput {
        records: std::mem::take(&mut state.records),
        return_value,
    })
}

/// Finds and validates the module's single entry point.
fn entry_point<'ctx>(
    context: &'ctx Context,
    functions: &[FunctionValue<'ctx>],
) -> Result<FunctionValue<'ctx>, String> {
    let entries: Vec<_> = functions
        .iter()
        .copied()
        .filter(|function| {
            ["entry_point", "EntryPoint"].iter().any(|name| {
                function
                    .get_string_attribute(AttributeLoc::Function, name)
                    .is_some()
            })
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return Err("Expected exactly one defined QIR entry point".into());
    };
    if entry.count_params() != 0
        || entry
            .get_type()
            .get_return_type()
            .is_some_and(|ty| ty != context.i64_type().into())
    {
        return Err("QIR entry point must take no arguments and return void or i64".into());
    }
    Ok(*entry)
}

/// Reads a static resource count from an entry-point attribute.
fn resource_count(function: FunctionValue<'_>, name: &str) -> Result<usize, String> {
    function
        .get_string_attribute(AttributeLoc::Function, name)
        .ok_or_else(|| format!("Missing entry-point attribute: {name}"))?
        .get_string_value()
        .to_str()
        .map_err(|error| error.to_string())?
        .parse()
        .map_err(|error| format!("Invalid {name}: {error}"))
}

/// Rejects module constructs that the runner cannot safely dispatch.
fn validate_module(module: &Module<'_>) -> Result<(), String> {
    for flag in module.get_global_metadata("llvm.module.flags") {
        let values = flag.get_node_values();
        if let [_, BasicMetadataValueEnum::MetadataValue(key), value] = values.as_slice()
            && key.get_string_value().is_some_and(|key| {
                matches!(
                    key.to_bytes(),
                    b"dynamic_qubit_management" | b"dynamic_result_management"
                )
            })
            && !matches!(value, BasicMetadataValueEnum::IntValue(value) if value.get_zero_extended_constant() == Some(0))
        {
            return Err("Dynamic qubit/result management is not supported".into());
        }
    }
    for global in module.get_globals() {
        if global.get_initializer().is_none()
            || global.get_name().to_bytes().starts_with(b"llvm.")
            || global.get_name().to_bytes().starts_with(b"__qep_")
        {
            return Err(format!("Unsupported QIR global: {:?}", global.get_name()));
        }
    }
    for function in module.get_functions() {
        let name = function.get_name().to_string_lossy();
        if name.starts_with("__qep_")
            || (name.starts_with("__quantum__") && function.count_basic_blocks() != 0)
            || function.get_call_conventions() != 0
            || function.get_type().is_var_arg()
        {
            return Err(format!(
                "Unsupported QIR function definition or ABI: {name}"
            ));
        }
        for instruction in function
            .get_basic_blocks()
            .iter()
            .flat_map(|block| block.get_instructions())
        {
            match instruction.get_opcode() {
                InstructionOpcode::Call => {
                    let call = CallSiteValue::try_from(instruction)
                        .expect("call instruction has a call-site value");
                    if call.get_called_fn_value().is_none() || call.get_call_convention() != 0 {
                        return Err(
                            "Indirect calls and non-C calling conventions are unsupported".into(),
                        );
                    }
                }
                InstructionOpcode::Invoke | InstructionOpcode::CallBr => {
                    return Err(
                        "QIR exception-handling and callbr instructions are unsupported".into(),
                    );
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Inserts error checks after calls so backend failures stop QIR execution.
fn instrument_errors<'ctx>(
    context: &'ctx Context,
    function: FunctionValue<'ctx>,
    has_error: FunctionValue<'ctx>,
    state: PointerValue<'ctx>,
) -> Result<(), BuilderError> {
    let calls: Vec<_> = function
        .get_basic_blocks()
        .iter()
        .flat_map(|block| block.get_instructions())
        .filter(|instruction| instruction.get_opcode() == InstructionOpcode::Call)
        .collect();
    runtime::clear_function_attributes(function);
    if calls.is_empty() {
        return Ok(());
    }
    let builder = context.create_builder();
    let failure = context.append_basic_block(function, "qep.failure");
    builder.position_at_end(failure);
    if let Some(ty) = function.get_type().get_return_type() {
        builder.build_return(Some(&ty.const_zero()))?;
    } else {
        builder.build_return(None)?;
    }

    for instruction in calls {
        let call = CallSiteValue::try_from(instruction).expect("call instruction");
        call.set_tail_call(false);
        for attr in call.attributes(AttributeLoc::Function) {
            if attr.is_string() {
                call.remove_string_attribute(
                    AttributeLoc::Function,
                    &attr.get_string_kind_id().to_string_lossy(),
                );
            } else {
                call.remove_enum_attribute(AttributeLoc::Function, attr.get_enum_kind_id());
            }
        }
        let original = instruction.get_parent().expect("call belongs to a block");
        let continuation = context.append_basic_block(function, "qep.continue");
        let mut next = instruction.get_next_instruction();
        builder.position_at_end(continuation);
        while let Some(following) = next {
            next = following.get_next_instruction();
            following.remove_from_basic_block();
            builder.insert_instruction(&following, None);
        }

        // Moving the terminator changes predecessor identities, including
        // back-edges and PHIs that use this call's return value.
        for block in function.get_basic_blocks() {
            for phi in block
                .get_instructions()
                .take_while(|inst| inst.get_opcode() == InstructionOpcode::Phi)
                .collect::<Vec<_>>()
            {
                let phi = PhiValue::try_from(phi).expect("phi instruction");
                let incoming: Vec<_> = phi.get_incomings().collect();
                if incoming.iter().any(|(_, block)| *block == original) {
                    builder.position_before(&phi.as_instruction());
                    let replacement =
                        builder.build_phi(phi.as_basic_value().get_type(), "qep.phi")?;
                    for (value, block) in incoming {
                        replacement.add_incoming(&[(
                            &value,
                            if block == original {
                                continuation
                            } else {
                                block
                            },
                        )]);
                    }
                    phi.replace_all_uses_with(&replacement);
                    phi.as_instruction().erase_from_basic_block();
                }
            }
        }
        builder.position_at_end(original);
        let failed = builder
            .build_call(has_error, &[state.into()], "qep.failed")?
            .try_as_basic_value()
            .left()
            .expect("error check returns bool")
            .into_int_value();
        builder.build_conditional_branch(failed, failure, continuation)?;
    }
    Ok(())
}

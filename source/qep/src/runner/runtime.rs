// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{OutputRecord, OutputValue};
use crate::QirBackend;
use inkwell::{
    attributes::AttributeLoc,
    builder::BuilderError,
    context::Context,
    module::Module,
    types::FunctionType,
    values::{BasicMetadataValueEnum, FunctionValue, PointerValue},
};
use std::{
    ffi::{CStr, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
};

pub(super) struct ExecutionState<'a, B> {
    backend: &'a mut B,
    qubits: usize,
    written_results: Vec<bool>,
    pub records: Vec<OutputRecord>,
    pub error: Option<String>,
}

impl<'a, B: QirBackend> ExecutionState<'a, B> {
    /// Allocates execution state for the module's declared resources.
    pub fn new(backend: &'a mut B, qubits: usize, results: usize) -> Result<Self, String> {
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(results)
            .map_err(|error| format!("Cannot allocate result slots: {error}"))?;
        slots.resize(results, false);
        Ok(Self {
            backend,
            qubits,
            written_results: slots,
            records: Vec::new(),
            error: None,
        })
    }

    /// Converts an encoded qubit pointer into a bounds-checked ID.
    fn qubit(&self, ptr: *mut c_void) -> Result<usize, String> {
        let id = ptr as usize;
        if id < self.qubits {
            Ok(id)
        } else {
            Err(format!("Qubit ID {id} exceeds the declared qubit count"))
        }
    }

    /// Resolves an encoded result pointer to its written-state marker.
    fn result_slot(&mut self, ptr: *mut c_void) -> Result<&mut bool, String> {
        let id = ptr as usize;
        self.written_results
            .get_mut(id)
            .ok_or_else(|| format!("Result ID {id} exceeds the declared result count"))
    }

    /// Reads a result after confirming it was written in this invocation.
    fn read(&mut self, ptr: *mut c_void) -> Result<bool, String> {
        if !*self.result_slot(ptr)? {
            return Err(format!("Result ID {} has not been written", ptr as usize));
        }
        self.backend.read_result(ptr as usize)
    }

    /// Appends an output value and decodes its optional label.
    unsafe fn record(&mut self, value: OutputValue, label: *const c_char) -> Result<(), String> {
        let label = if label.is_null() {
            None
        } else {
            // SAFETY: The run_qir contract requires valid output-label pointers.
            Some(
                unsafe { CStr::from_ptr(label) }
                    .to_str()
                    .map_err(|e| e.to_string())?
                    .to_owned(),
            )
        };
        self.records.push(OutputRecord { value, label });
        Ok(())
    }
}

// All callbacks take an explicit context pointer inserted by the LLVM wrappers.
// Its lifetime is bounded by synchronous JIT execution, without global or TLS state.
/// Dispatches a callback while capturing backend errors and panics.
unsafe fn dispatch<B: QirBackend, T: Default>(
    ptr: *mut c_void,
    operation: impl FnOnce(&mut ExecutionState<'_, B>) -> Result<T, String>,
) -> T {
    // SAFETY: Only our generated wrappers call this with the live, unique state.
    let state = unsafe { &mut *ptr.cast::<ExecutionState<'_, B>>() };
    if state.error.is_some() {
        return T::default();
    }
    match catch_unwind(AssertUnwindSafe(|| operation(state))) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            state.error = Some(error);
            T::default()
        }
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            state.error = Some(format!("QIR backend panicked: {message}"));
            T::default()
        }
    }
    // The ABI requires a return value even on failure. Injected guards prevent
    // the QIR caller from consuming it and propagate failure to the entry point.
}

/// Reports whether callback dispatch has recorded an execution error.
pub(super) unsafe extern "C" fn has_error<B: QirBackend>(ptr: *mut c_void) -> bool {
    // SAFETY: The injected error checks use the same live execution state.
    unsafe { &*ptr.cast::<ExecutionState<'_, B>>() }
        .error
        .is_some()
}

macro_rules! gates {
    ($($gate:ident($($qubit:ident),+));+ $(;)?) => {$(
        /// Validates gate operands and forwards the operation to the backend.
        unsafe extern "C" fn $gate<B: QirBackend>(ptr: *mut c_void, $($qubit: *mut c_void),+) {
            // SAFETY: Called only by a generated, signature-checked wrapper.
            unsafe { dispatch::<B, _>(ptr, |state| {
                $(let $qubit = state.qubit($qubit)?;)+
                state.backend.$gate($($qubit),+)
            }); }
        }
    )+};
}
gates! {
    ccx(a, b, c); cx(a, b); cy(a, b); cz(a, b);
    h(a); s(a); sadj(a); sx(a); t(a); tadj(a); x(a); y(a); z(a);
    swap(a, b); reset(a)
}

macro_rules! rotations {
    ($($gate:ident($($qubit:ident),+));+ $(;)?) => {$(
        /// Validates rotation operands and forwards the operation to the backend.
        unsafe extern "C" fn $gate<B: QirBackend>(ptr: *mut c_void, angle: f64, $($qubit: *mut c_void),+) {
            // SAFETY: Called only by a generated, signature-checked wrapper.
            unsafe { dispatch::<B, _>(ptr, |state| {
                $(let $qubit = state.qubit($qubit)?;)+
                state.backend.$gate(angle, $($qubit),+)
            }); }
        }
    )+};
}
rotations! { rx(a); rxx(a, b); ry(a); ryy(a, b); rz(a); rzz(a, b) }

macro_rules! measurement {
    ($method:ident) => {
        /// Issues a measurement and marks its result slot as written.
        unsafe extern "C" fn $method<B: QirBackend>(
            ptr: *mut c_void,
            qubit: *mut c_void,
            result: *mut c_void,
        ) {
            // SAFETY: Called only by a generated, signature-checked wrapper.
            unsafe {
                dispatch::<B, _>(ptr, |state| {
                    let qubit = state.qubit(qubit)?;
                    state.result_slot(result)?;
                    state.backend.$method(qubit, result as usize)?;
                    *state.result_slot(result)? = true;
                    Ok(())
                });
            }
        }
    };
}
measurement!(mz);
measurement!(mresetz);

/// Resolves a QIR result through the backend.
unsafe extern "C" fn read_result<B: QirBackend>(ptr: *mut c_void, result: *mut c_void) -> bool {
    // SAFETY: Called only by a generated, signature-checked wrapper.
    unsafe { dispatch::<B, _>(ptr, |state| state.read(result)) }
}

/// Resolves and appends a result to the output stream.
unsafe extern "C" fn record_result<B: QirBackend>(
    ptr: *mut c_void,
    result: *mut c_void,
    label: *const c_char,
) {
    // SAFETY: Generated wrapper and the run_qir output-label contract.
    unsafe {
        dispatch::<B, _>(ptr, |state| {
            let value = state.read(result)?;
            state.record(OutputValue::Result(value), label)
        });
    }
}

macro_rules! output {
    ($($method:ident($ty:ty) => $variant:ident);+ $(;)?) => {$(
        /// Appends a primitive value to the output stream.
        unsafe extern "C" fn $method<B: QirBackend>(
            ptr: *mut c_void, value: $ty, label: *const c_char,
        ) {
            // SAFETY: Generated wrapper and the run_qir output-label contract.
            unsafe {
                dispatch::<B, _>(ptr, |state| state.record(OutputValue::$variant(value), label));
            }
        }
    )+};
}
output! {
    record_array(u64) => Array; record_tuple(u64) => Tuple;
    record_bool(bool) => Bool; record_int(i64) => Int; record_double(f64) => Double
}

/// Accepts the QIR runtime initialization hook without additional setup.
unsafe extern "C" fn initialize(_ptr: *mut c_void, _environment: *mut c_void) {}

pub(super) struct Binding<'ctx> {
    pub name: &'static str,
    pub ty: FunctionType<'ctx>,
    pub address: usize,
}

/// Builds the supported QIR symbol-to-callback bindings.
pub(super) fn bindings<B: QirBackend>(context: &Context) -> Vec<Binding<'_>> {
    let ptr = context.ptr_type(Default::default()).into();
    let double = context.f64_type().into();
    let int = context.i64_type().into();
    let boolean = context.bool_type().into();
    let void = context.void_type();
    let mut bindings = Vec::new();
    macro_rules! bind {
        ($name:literal, $method:ident, $return:expr, [$($param:expr),*]) => {
            bindings.push(Binding {
                name: $name,
                ty: $return.fn_type(&[$($param),*], false),
                address: $method::<B> as *const () as usize,
            });
        };
    }
    bind!("__quantum__qis__ccx__body", ccx, void, [ptr, ptr, ptr]);
    bind!("__quantum__qis__cx__body", cx, void, [ptr, ptr]);
    bind!("__quantum__qis__cnot__body", cx, void, [ptr, ptr]);
    bind!("__quantum__qis__cy__body", cy, void, [ptr, ptr]);
    bind!("__quantum__qis__cz__body", cz, void, [ptr, ptr]);
    bind!("__quantum__qis__rx__body", rx, void, [double, ptr]);
    bind!("__quantum__qis__rxx__body", rxx, void, [double, ptr, ptr]);
    bind!("__quantum__qis__ry__body", ry, void, [double, ptr]);
    bind!("__quantum__qis__ryy__body", ryy, void, [double, ptr, ptr]);
    bind!("__quantum__qis__rz__body", rz, void, [double, ptr]);
    bind!("__quantum__qis__rzz__body", rzz, void, [double, ptr, ptr]);
    bind!("__quantum__qis__h__body", h, void, [ptr]);
    bind!("__quantum__qis__s__body", s, void, [ptr]);
    bind!("__quantum__qis__s__adj", sadj, void, [ptr]);
    bind!("__quantum__qis__sx__body", sx, void, [ptr]);
    bind!("__quantum__qis__t__body", t, void, [ptr]);
    bind!("__quantum__qis__t__adj", tadj, void, [ptr]);
    bind!("__quantum__qis__x__body", x, void, [ptr]);
    bind!("__quantum__qis__y__body", y, void, [ptr]);
    bind!("__quantum__qis__z__body", z, void, [ptr]);
    bind!("__quantum__qis__swap__body", swap, void, [ptr, ptr]);
    bind!("__quantum__qis__reset__body", reset, void, [ptr]);
    bind!("__quantum__qis__m__body", mz, void, [ptr, ptr]);
    bind!("__quantum__qis__mz__body", mz, void, [ptr, ptr]);
    bind!("__quantum__qis__mresetz__body", mresetz, void, [ptr, ptr]);
    bind!(
        "__quantum__qis__read_result__body",
        read_result,
        context.bool_type(),
        [ptr]
    );
    bind!(
        "__quantum__rt__read_result",
        read_result,
        context.bool_type(),
        [ptr]
    );
    bind!(
        "__quantum__rt__result_record_output",
        record_result,
        void,
        [ptr, ptr]
    );
    bind!(
        "__quantum__rt__array_record_output",
        record_array,
        void,
        [int, ptr]
    );
    bind!(
        "__quantum__rt__tuple_record_output",
        record_tuple,
        void,
        [int, ptr]
    );
    bind!(
        "__quantum__rt__bool_record_output",
        record_bool,
        void,
        [boolean, ptr]
    );
    bind!(
        "__quantum__rt__int_record_output",
        record_int,
        void,
        [int, ptr]
    );
    bind!(
        "__quantum__rt__double_record_output",
        record_double,
        void,
        [double, ptr]
    );
    bindings.push(Binding {
        name: "__quantum__rt__initialize",
        ty: void.fn_type(&[ptr], false),
        address: initialize as *const () as usize,
    });
    bindings
}

/// Removes function promises invalidated by runtime instrumentation.
pub(super) fn clear_function_attributes(function: FunctionValue<'_>) {
    // Original purity/termination promises need not hold for backend callbacks
    // or the injected error checks.
    for attr in function.attributes(AttributeLoc::Function) {
        if attr.is_string() {
            function.remove_string_attribute(
                AttributeLoc::Function,
                &attr.get_string_kind_id().to_string_lossy(),
            );
        } else {
            function.remove_enum_attribute(AttributeLoc::Function, attr.get_enum_kind_id());
        }
    }
}

/// Replaces a QIR declaration with a wrapper that supplies execution state.
pub(super) fn bind<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    original: FunctionValue<'ctx>,
    ty: FunctionType<'ctx>,
    state: PointerValue<'ctx>,
) -> Result<FunctionValue<'ctx>, BuilderError> {
    clear_function_attributes(original);
    let mut params = vec![context.ptr_type(Default::default()).into()];
    params.extend(ty.get_param_types());
    let native_type = ty.get_return_type().map_or_else(
        || context.void_type().fn_type(&params, false),
        |ty| {
            use inkwell::types::BasicType;
            ty.fn_type(&params, false)
        },
    );
    let native = module.add_function(
        &format!("__qep_{}", original.get_name().to_string_lossy()),
        native_type,
        None,
    );
    let builder = context.create_builder();
    builder.position_at_end(context.append_basic_block(original, "qep.dispatch"));
    let mut args = vec![state.into()];
    args.extend(original.get_param_iter().map(BasicMetadataValueEnum::from));
    let call = builder.build_call(native, &args, "")?;
    if let Some(value) = call.try_as_basic_value().left() {
        builder.build_return(Some(&value))?;
    } else {
        builder.build_return(None)?;
    }
    Ok(native)
}

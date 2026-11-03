// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_circuit_store::{
    Clbit, CustomOperation, PackedOperation, ParamValue, Qubit, StandardGate, TimeUnit,
    UnitaryOperation,
};

use crate::{
    QDK_WRONG_OWNER, QdkAngleExpression, QdkBytes, QdkCircuit, QdkDiagnostic, QdkParameter,
    QdkStatus,
    boundary::{Error, boundary, input_slice, input_text, output, required},
};

pub const QDK_GATE_I: u32 = 1;
pub const QDK_GATE_X: u32 = 2;
pub const QDK_GATE_Y: u32 = 3;
pub const QDK_GATE_Z: u32 = 4;
pub const QDK_GATE_H: u32 = 5;
pub const QDK_GATE_S: u32 = 6;
pub const QDK_GATE_SDG: u32 = 7;
pub const QDK_GATE_SX: u32 = 8;
pub const QDK_GATE_SXDG: u32 = 9;
pub const QDK_GATE_T: u32 = 10;
pub const QDK_GATE_TDG: u32 = 11;
pub const QDK_GATE_CX: u32 = 12;
pub const QDK_GATE_CY: u32 = 13;
pub const QDK_GATE_CZ: u32 = 14;
pub const QDK_GATE_SWAP: u32 = 15;
pub const QDK_GATE_CCX: u32 = 16;
pub const QDK_GATE_CSWAP: u32 = 17;
pub const QDK_GATE_RX: u32 = 18;
pub const QDK_GATE_RY: u32 = 19;
pub const QDK_GATE_RZ: u32 = 20;
pub const QDK_GATE_RXX: u32 = 21;
pub const QDK_GATE_RYY: u32 = 22;
pub const QDK_GATE_RZZ: u32 = 23;
pub const QDK_GATE_CRX: u32 = 24;
pub const QDK_GATE_CRY: u32 = 25;
pub const QDK_GATE_CRZ: u32 = 26;
pub const QDK_GATE_MEASURE_X: u32 = 27;
pub const QDK_GATE_MEASURE_Y: u32 = 28;
pub const QDK_GATE_MEASURE_Z: u32 = 29;
pub const QDK_GATE_PREPARE_X: u32 = 30;
pub const QDK_GATE_PREPARE_Y: u32 = 31;
pub const QDK_GATE_PREPARE_Z: u32 = 32;

pub const QDK_OPERATION_STANDARD: u32 = 1;
pub const QDK_OPERATION_RESET: u32 = 2;
pub const QDK_OPERATION_BARRIER: u32 = 3;
pub const QDK_OPERATION_DELAY: u32 = 4;
pub const QDK_OPERATION_CUSTOM: u32 = 5;
pub const QDK_OPERATION_UNITARY: u32 = 6;
pub const QDK_OPERATION_CONTROL_FLOW: u32 = 7;
pub const QDK_ANGLE_LITERAL: u32 = 1;
pub const QDK_ANGLE_PARAMETER: u32 = 2;
pub const QDK_ANGLE_EXPRESSION: u32 = 3;
pub const QDK_TIME_DT: u32 = 1;
pub const QDK_TIME_NS: u32 = 2;
pub const QDK_TIME_US: u32 = 3;
pub const QDK_TIME_MS: u32 = 4;
pub const QDK_TIME_S: u32 = 5;

// Explicit foreign tags, not casts from the native discriminants.
const GATES: [(u32, StandardGate); qsc_circuit_store::STANDARD_GATE_COUNT] = [
    (QDK_GATE_I, StandardGate::I),
    (QDK_GATE_X, StandardGate::X),
    (QDK_GATE_Y, StandardGate::Y),
    (QDK_GATE_Z, StandardGate::Z),
    (QDK_GATE_H, StandardGate::H),
    (QDK_GATE_S, StandardGate::S),
    (QDK_GATE_SDG, StandardGate::SDag),
    (QDK_GATE_SX, StandardGate::SqrtX),
    (QDK_GATE_SXDG, StandardGate::SqrtXDag),
    (QDK_GATE_T, StandardGate::T),
    (QDK_GATE_TDG, StandardGate::TDag),
    (QDK_GATE_CX, StandardGate::Cx),
    (QDK_GATE_CY, StandardGate::Cy),
    (QDK_GATE_CZ, StandardGate::Cz),
    (QDK_GATE_SWAP, StandardGate::Swap),
    (QDK_GATE_CCX, StandardGate::Ccx),
    (QDK_GATE_CSWAP, StandardGate::Cswap),
    (QDK_GATE_RX, StandardGate::Rx),
    (QDK_GATE_RY, StandardGate::Ry),
    (QDK_GATE_RZ, StandardGate::Rz),
    (QDK_GATE_RXX, StandardGate::Rxx),
    (QDK_GATE_RYY, StandardGate::Ryy),
    (QDK_GATE_RZZ, StandardGate::Rzz),
    (QDK_GATE_CRX, StandardGate::Crx),
    (QDK_GATE_CRY, StandardGate::Cry),
    (QDK_GATE_CRZ, StandardGate::Crz),
    (QDK_GATE_MEASURE_X, StandardGate::MeasureX),
    (QDK_GATE_MEASURE_Y, StandardGate::MeasureY),
    (QDK_GATE_MEASURE_Z, StandardGate::MeasureZ),
    (QDK_GATE_PREPARE_X, StandardGate::PrepareX),
    (QDK_GATE_PREPARE_Y, StandardGate::PrepareY),
    (QDK_GATE_PREPARE_Z, StandardGate::PrepareZ),
];

pub(crate) fn gate(tag: u32) -> Result<StandardGate, Error> {
    GATES
        .iter()
        .find(|(id, _)| *id == tag)
        .map(|(_, gate)| *gate)
        .ok_or_else(|| Error::invalid(format!("unknown standard gate tag {tag}")))
}

pub(crate) fn gate_tag(gate: StandardGate) -> u32 {
    GATES
        .iter()
        .find(|(_, value)| *value == gate)
        .expect("all native gates mapped")
        .0
}

fn measurement(gate: StandardGate) -> bool {
    matches!(
        gate,
        StandardGate::MeasureX | StandardGate::MeasureY | StandardGate::MeasureZ
    )
}

pub(crate) fn time_unit(tag: u32) -> Result<TimeUnit, Error> {
    match tag {
        QDK_TIME_DT => Ok(TimeUnit::Dt),
        QDK_TIME_NS => Ok(TimeUnit::Ns),
        QDK_TIME_US => Ok(TimeUnit::Us),
        QDK_TIME_MS => Ok(TimeUnit::Ms),
        QDK_TIME_S => Ok(TimeUnit::S),
        _ => Err(Error::invalid(format!("unknown time unit tag {tag}"))),
    }
}

pub(crate) const fn time_tag(unit: TimeUnit) -> u32 {
    match unit {
        TimeUnit::Dt => QDK_TIME_DT,
        TimeUnit::Ns => QDK_TIME_NS,
        TimeUnit::Us => QDK_TIME_US,
        TimeUnit::Ms => QDK_TIME_MS,
        TimeUnit::S => QDK_TIME_S,
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkComplex {
    pub real: f64,
    pub imag: f64,
}

/// Input angle: a literal or a live parameter/expression from the receiving circuit.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkAngle {
    pub kind: u32,
    pub literal: f64,
    pub parameter: *const QdkParameter,
    pub expression: *const QdkAngleExpression,
}

/// All pointed-to input data is borrowed only for the append call.
/// Initialize unused fields to zero. Custom/matrix operations are storage-only.
#[derive(Default)]
#[repr(C)]
pub struct QdkOperation {
    pub kind: u32,
    pub gate: u32,
    pub qubits: *const u32,
    pub qubit_count: usize,
    pub clbits: *const u32,
    pub clbit_count: usize,
    pub params: *const QdkAngle,
    pub param_count: usize,
    pub name: *const u8,
    pub name_len: usize,
    pub matrix: *const QdkComplex,
    pub matrix_len: usize,
    pub duration: f64,
    pub time_unit: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkGateInfo {
    /// Owned UTF-8 bytes; release with `qdk_bytes_free`.
    pub name: QdkBytes,
    pub qubit_arity: u32,
    pub clbit_arity: u32,
    pub param_arity: u32,
    pub estimator_id: u32,
}

/// On failure, `failed_index` equals the number of successfully appended gates.
/// On success it equals accepted, the total gate count.
#[derive(Default)]
#[repr(C)]
pub struct QdkBatchProgress {
    pub accepted: usize,
    pub failed_index: usize,
}

#[repr(C)]
pub struct QdkGateOperands {
    pub gate: u32,
    pub qubits: *const u32,
    pub qubit_count: usize,
}

pub(crate) struct Decoded {
    pub operation: PackedOperation,
    pub qubits: Vec<Qubit>,
    pub clbits: Vec<Clbit>,
    pub params: Option<Box<[ParamValue]>>,
}

pub(crate) unsafe fn angle(
    value: &QdkAngle,
    circuit: *const QdkCircuit,
) -> Result<ParamValue, Error> {
    match value.kind {
        QDK_ANGLE_LITERAL => Ok(ParamValue::Literal(value.literal)),
        QDK_ANGLE_PARAMETER => {
            // SAFETY: input handles are required to be live.
            let parameter = unsafe { required(value.parameter, "parameter")? };
            if parameter.circuit != circuit {
                return Err(Error::new(
                    QDK_WRONG_OWNER,
                    "parameter belongs to another circuit",
                ));
            }
            Ok(ParamValue::Parameter(*parameter.inner.access()?))
        }
        QDK_ANGLE_EXPRESSION => {
            // SAFETY: input handles are required to be live.
            let expression = unsafe { required(value.expression, "angle expression")? };
            if expression.circuit != circuit {
                return Err(Error::new(
                    QDK_WRONG_OWNER,
                    "angle expression belongs to another circuit",
                ));
            }
            Ok(ParamValue::Expr(*expression.inner.access()?))
        }
        tag => Err(Error::invalid(format!("unknown angle kind {tag}"))),
    }
}

pub(crate) unsafe fn decode(
    value: &QdkOperation,
    circuit: *const QdkCircuit,
) -> Result<Decoded, Error> {
    // SAFETY: all input buffers and handles satisfy the exported append contract.
    unsafe {
        let qubits: Vec<_> = input_slice(value.qubits, value.qubit_count)?
            .iter()
            .copied()
            .map(Qubit)
            .collect();
        let clbits: Vec<_> = input_slice(value.clbits, value.clbit_count)?
            .iter()
            .copied()
            .map(Clbit)
            .collect();
        let params: Vec<_> = input_slice(value.params, value.param_count)?
            .iter()
            .map(|value| angle(value, circuit))
            .collect::<Result<_, _>>()?;
        let (operation, expected_params, expected_clbits) = match value.kind {
            QDK_OPERATION_STANDARD => {
                let gate = gate(value.gate)?;
                (
                    PackedOperation::Standard(gate),
                    usize::from(gate.param_arity()),
                    usize::from(measurement(gate)),
                )
            }
            QDK_OPERATION_RESET => (PackedOperation::Reset, 0, 0),
            QDK_OPERATION_BARRIER => (
                PackedOperation::Barrier {
                    qubit_arity: u8::try_from(qubits.len())
                        .map_err(|_| Error::invalid("barrier spans too many qubits"))?,
                },
                0,
                0,
            ),
            QDK_OPERATION_DELAY => {
                if !value.duration.is_finite() || value.duration < 0.0 {
                    return Err(Error::invalid(
                        "a delay lasts a finite, non-negative duration",
                    ));
                }
                (
                    PackedOperation::Delay {
                        duration: value.duration,
                        unit: time_unit(value.time_unit)?,
                    },
                    0,
                    0,
                )
            }
            QDK_OPERATION_CUSTOM => {
                let operation = CustomOperation {
                    name: input_text(value.name, value.name_len)?.into(),
                    qubit_arity: u8::try_from(qubits.len())
                        .map_err(|_| Error::invalid("custom operation spans too many qubits"))?,
                    param_arity: u8::try_from(params.len())
                        .map_err(|_| Error::invalid("custom operation has too many parameters"))?,
                };
                (
                    PackedOperation::Custom(Box::new(operation)),
                    params.len(),
                    0,
                )
            }
            QDK_OPERATION_UNITARY => (decode_unitary(value)?, 0, 0),
            tag => {
                return Err(Error::invalid(format!(
                    "unknown append operation kind {tag}"
                )));
            }
        };
        if params.len() != expected_params {
            return Err(Error::invalid(format!(
                "operation expects {expected_params} parameters but {} were given",
                params.len()
            )));
        }
        if clbits.len() != expected_clbits {
            return Err(Error::invalid(format!(
                "operation expects {expected_clbits} classical bits but {} were given",
                clbits.len()
            )));
        }
        Ok(Decoded {
            operation,
            qubits,
            clbits,
            params: if params.is_empty() {
                None
            } else {
                Some(params.into_boxed_slice())
            },
        })
    }
}

unsafe fn decode_unitary(value: &QdkOperation) -> Result<PackedOperation, Error> {
    let arity = u8::try_from(value.qubit_count)
        .map_err(|_| Error::invalid("unitary spans too many qubits"))?;
    let expected = 1usize
        .checked_shl(2 * u32::from(arity))
        .ok_or_else(|| Error::invalid("matrix dimensions exceed the addressable range"))?;
    if value.matrix_len != expected {
        return Err(Error::invalid(format!(
            "unitary expects {expected} matrix entries but {} were given",
            value.matrix_len
        )));
    }
    // SAFETY: the enclosing append contract requires a live matrix input buffer.
    let matrix = unsafe { input_slice(value.matrix, value.matrix_len)? }
        .iter()
        .map(|value| (value.real, value.imag))
        .collect();
    Ok(PackedOperation::Unitary(Box::new(UnitaryOperation {
        qubit_arity: arity,
        matrix,
    })))
}

/// Copies metadata for one explicit standard-gate tag.
/// # Safety
/// Output slots must be valid, writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_gate_info(
    tag: u32,
    out: *mut QdkGateInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides valid output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let gate = gate(tag)?;
            *out = QdkGateInfo {
                name: gate.name().as_bytes().to_vec().into(),
                qubit_arity: gate.qubit_arity().into(),
                clbit_arity: u32::from(measurement(gate)),
                param_arity: gate.param_arity().into(),
                estimator_id: gate.instruction_id().into(),
            };
            Ok(())
        })
    }
}

/// Appends a checked operation to the current native construction body.
/// Expected validation failure appends no instruction.
/// # Safety
/// The circuit, descriptor, parameter handles and declared input buffers are
/// live and disjoint from error output. No concurrent access is allowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_append(
    circuit: *mut QdkCircuit,
    operation: *const QdkOperation,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller guarantees input allocation validity and extent.
    unsafe {
        boundary(error, None, || {
            let owner = required(circuit, "circuit")?;
            let mut value = owner.inner.access()?;
            let decoded = decode(required(operation, "operation")?, circuit)?;
            value.append(
                decoded.operation,
                &decoded.qubits,
                &decoded.clbits,
                decoded.params,
            )?;
            Ok(())
        })
    }
}

fn batch_gate(tag: u32) -> Result<StandardGate, Error> {
    let gate = gate(tag)?;
    if gate.param_arity() != 0 || measurement(gate) {
        return Err(Error::invalid(
            "native batches require parameter-free, non-measurement gates",
        ));
    }
    Ok(gate)
}

/// Appends one parameter-free standard gate per contiguous operand group,
/// using the native batch engine. A failed group retains its successful prefix.
/// Batches require a closed circuit.
/// # Safety
/// Circuit and operands are live; writable progress/error slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_extend_standard(
    circuit: *mut QdkCircuit,
    tag: u32,
    qubits: *const u32,
    qubit_count: usize,
    progress: *mut QdkBatchProgress,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies valid allocations of the declared extents.
    unsafe {
        boundary(error, None, || {
            let progress = output(progress)?;
            let mut value = required(circuit, "circuit")?.inner.access()?;
            value
                .require_closed("append a batch to")
                .map_err(Error::from)?;
            let gate = batch_gate(tag)?;
            let qubits: Vec<_> = input_slice(qubits, qubit_count)?
                .iter()
                .copied()
                .map(Qubit)
                .collect();
            let start = value.store().len();
            let result = value
                .store_mut()
                .extend_standard(gate, qubits.chunks(usize::from(gate.qubit_arity())));
            progress.accepted = value.store().len() - start;
            progress.failed_index = progress.accepted;
            result.map_err(Error::from)
        })
    }
}

/// Appends a mixed native batch of parameter-free standard gates. The first
/// invalid descriptor or native operand stops the batch after its valid prefix.
/// # Safety
/// Circuit, descriptors and each operand buffer are live; outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_extend(
    circuit: *mut QdkCircuit,
    gates: *const QdkGateOperands,
    count: usize,
    progress: *mut QdkBatchProgress,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller guarantees all input extents and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let progress = output(progress)?;
            let mut value = required(circuit, "circuit")?.inner.access()?;
            value
                .require_closed("append a batch to")
                .map_err(Error::from)?;
            let mut decoded = Vec::new();
            let mut descriptor_error = None;
            for item in input_slice(gates, count)? {
                let item = (|| {
                    let gate = batch_gate(item.gate)?;
                    let qubits: Vec<_> = input_slice(item.qubits, item.qubit_count)?
                        .iter()
                        .copied()
                        .map(Qubit)
                        .collect();
                    Ok::<_, Error>((gate, qubits))
                })();
                match item {
                    Ok(item) => decoded.push(item),
                    Err(error) => {
                        descriptor_error = Some(error);
                        break;
                    }
                }
            }
            let start = value.store().len();
            let result = value.store_mut().extend(
                decoded
                    .iter()
                    .map(|(gate, qubits)| (*gate, qubits.as_slice())),
            );
            progress.accepted = value.store().len() - start;
            progress.failed_index = progress.accepted;
            result?;
            match descriptor_error {
                Some(error) => Err(error),
                None => Ok(()),
            }
        })
    }
}

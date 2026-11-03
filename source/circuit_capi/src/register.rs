// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qdk_circuit_api::{Circuit, RegisterSpec};

use crate::{
    QdkBytes, QdkCircuit, QdkDiagnostic, QdkStatus,
    boundary::{Error, boundary, input_slice, input_text, output, required},
    handle::Handle,
};

pub const QDK_REGISTER_QUANTUM: u32 = 1;
pub const QDK_REGISTER_CLASSICAL: u32 = 2;

#[repr(C)]
pub struct QdkRegisterInput {
    pub kind: u32,
    pub name: *const u8,
    pub name_len: usize,
    pub size: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkRegisterInfo {
    /// Owned UTF-8 bytes; release with `qdk_bytes_free`.
    pub name: QdkBytes,
    pub start: u32,
    pub size: u32,
}

/// Creates fixed, non-overlapping register ranges in declaration order. The
/// extra classical width follows the declared classical registers.
/// # Safety
/// Input arrays/strings cover their declared extents; outputs are writable and
/// disjoint from inputs and one another.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_from_registers(
    registers: *const QdkRegisterInput,
    count: usize,
    extra_clbits: u32,
    out: *mut *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller guarantees input extents and disjoint writable outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let registers: Vec<_> = input_slice(registers, count)?
                .iter()
                .map(|register| {
                    let name = input_text(register.name, register.name_len)?.to_owned();
                    match register.kind {
                        QDK_REGISTER_QUANTUM => Ok(RegisterSpec::Quantum {
                            name,
                            size: register.size,
                        }),
                        QDK_REGISTER_CLASSICAL => Ok(RegisterSpec::Classical {
                            name,
                            size: register.size,
                        }),
                        tag => Err(Error::invalid(format!("unknown register kind {tag}"))),
                    }
                })
                .collect::<Result<_, _>>()?;
            let circuit = Circuit::from_registers(registers, extra_clbits).map_err(Error::from)?;
            *out = Box::into_raw(Box::new(QdkCircuit {
                inner: Handle::new(circuit),
            }));
            Ok(())
        })
    }
}

/// Copies a named register range. No live register view escapes the owner.
/// # Safety
/// Circuit is live and writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_register(
    circuit: *const QdkCircuit,
    kind: u32,
    index: usize,
    out: *mut QdkRegisterInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live handle and disjoint writable slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(circuit, "circuit")?.inner.access()?;
            let registers = match kind {
                QDK_REGISTER_QUANTUM => value.quantum_registers(),
                QDK_REGISTER_CLASSICAL => value.classical_registers(),
                tag => return Err(Error::invalid(format!("unknown register kind {tag}"))),
            };
            let register = registers
                .get(index)
                .ok_or_else(|| Error::invalid(format!("register {index} does not exist")))?;
            *out = QdkRegisterInfo {
                name: register.name.as_bytes().to_vec().into(),
                start: register.start,
                size: register.size,
            };
            Ok(())
        })
    }
}

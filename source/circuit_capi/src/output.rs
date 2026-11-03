// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qdk_circuit_api::Circuit;
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};

use crate::{
    QdkBytes, QdkCircuit, QdkDiagnostic, QdkStatus,
    boundary::{Error, boundary, input_slice, input_text, output, required},
    handle::Handle,
};

pub const QDK_FORMAT_CIRCUIT_BINARY: u32 = 7;
pub const QDK_FORMAT_CIRCUIT_SEXPR: u32 = 9;
pub const QDK_FORMAT_QASM: u32 = 2;
pub const QDK_FORMAT_QIR: u32 = 3;
pub const QDK_FORMAT_TEXT: u32 = 4;
pub const QDK_FORMAT_DIAGRAM_JSON: u32 = 5;
pub const QDK_PROFILE_BASE: u32 = 1;
pub const QDK_PROFILE_ADAPTIVE_RI: u32 = 2;
pub const QDK_PROFILE_ADAPTIVE_RIF: u32 = 3;
pub const QDK_PROFILE_ADAPTIVE: u32 = 4;
pub const QDK_PROFILE_UNRESTRICTED: u32 = 5;

pub(crate) fn profile(tag: u32) -> Result<TargetCapabilityFlags, Error> {
    let profile = match tag {
        QDK_PROFILE_BASE => Profile::Base,
        QDK_PROFILE_ADAPTIVE_RI => Profile::AdaptiveRI,
        QDK_PROFILE_ADAPTIVE_RIF => Profile::AdaptiveRIF,
        QDK_PROFILE_ADAPTIVE => Profile::Adaptive,
        QDK_PROFILE_UNRESTRICTED => Profile::Unrestricted,
        _ => return Err(Error::invalid(format!("unknown target profile {tag}"))),
    };
    Ok(profile.into())
}

/// Imports a complete circuit snapshot or OpenQASM 3 source. Display formats
/// and QIR are not import formats. Snapshot input is length-delimited bytes;
/// S-expressions and OpenQASM are UTF-8.
/// # Safety
/// Input covers len initialized bytes; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_import(
    format: u32,
    data: *const u8,
    len: usize,
    out: *mut *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides the declared input extent and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let circuit = match format {
                QDK_FORMAT_CIRCUIT_BINARY => Circuit::from_bytes(input_slice(data, len)?),
                QDK_FORMAT_CIRCUIT_SEXPR => Circuit::from_text(input_text(data, len)?),
                QDK_FORMAT_QASM => Circuit::from_qasm(input_text(data, len)?),
                _ => {
                    return Err(Error::invalid(
                        "import requires a circuit snapshot or OpenQASM 3",
                    ));
                }
            }
            .map_err(Error::from)?;
            *out = Box::into_raw(Box::new(QdkCircuit {
                inner: Handle::new(circuit),
            }));
            Ok(())
        })
    }
}

/// Exports a closed circuit through the existing native codec/compiler/display
/// path. `profile_tag` is required only for QIR and ignored for other formats.
/// Output bytes are owned and not NUL-terminated; failure leaves them empty.
/// # Safety
/// Circuit is live; writable output slots are disjoint from it and each other.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_export(
    circuit: *const QdkCircuit,
    format: u32,
    profile_tag: u32,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live handle and disjoint writable outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(circuit, "circuit")?.inner.access()?;
            value.require_closed("export").map_err(Error::from)?;
            let bytes = match format {
                QDK_FORMAT_CIRCUIT_BINARY => value.to_bytes().map_err(Error::from)?,
                QDK_FORMAT_CIRCUIT_SEXPR => value.to_text().map_err(Error::from)?.into_bytes(),
                QDK_FORMAT_QASM => value.to_qasm().map_err(Error::from)?.into_bytes(),
                QDK_FORMAT_QIR => value
                    .to_qir(profile(profile_tag)?)
                    .map_err(Error::from)?
                    .into_bytes(),
                QDK_FORMAT_TEXT => value
                    .build_diagram()
                    .display_no_locations()
                    .to_string()
                    .into_bytes(),
                QDK_FORMAT_DIAGRAM_JSON => serde_json::to_string(&value.build_diagram())
                    .map_err(|error| Error::invalid(error.to_string()))?
                    .into_bytes(),
                _ => {
                    return Err(Error::invalid(format!(
                        "unknown circuit output format {format}"
                    )));
                }
            };
            *out = bytes.into();
            Ok(())
        })
    }
}

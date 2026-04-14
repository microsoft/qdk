// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    bitcode::{reader::parse_bitcode_compatibility_report, writer::try_write_bitcode},
    model::Module,
    text::{reader::parse_module, writer::write_module_to_string},
};

use super::config::QirSmithError;

pub(super) fn emit_text(module: &Module) -> String {
    write_module_to_string(module)
}

pub(super) fn parse_text_roundtrip(text: &str) -> Result<Module, QirSmithError> {
    parse_module(text).map_err(QirSmithError::TextRoundTrip)
}

pub(super) fn emit_bitcode(module: &Module) -> Result<Vec<u8>, QirSmithError> {
    try_write_bitcode(module).map_err(|error| {
        QirSmithError::ModelGeneration(format!("bitcode emission failed: {error}"))
    })
}

pub(super) fn parse_bitcode_roundtrip(bitcode: &[u8]) -> Result<Module, QirSmithError> {
    let report = parse_bitcode_compatibility_report(bitcode).map_err(|diagnostics| {
        QirSmithError::BitcodeRoundTrip(format_read_diagnostics(&diagnostics))
    })?;

    if !report.diagnostics.is_empty() {
        return Err(QirSmithError::BitcodeRoundTrip(format!(
            "compatibility diagnostics were reported during bitcode import: {}",
            format_read_diagnostics(&report.diagnostics)
        )));
    }

    Ok(report.module)
}

pub(super) fn format_read_diagnostics(diagnostics: &[crate::ReadDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

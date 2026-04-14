// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod tests;

use miette::Diagnostic;
use thiserror::Error;

pub mod bitcode;
pub mod fuzz;
pub mod model;
pub mod qir;
pub mod text;
pub mod validation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadPolicy {
    Compatibility,
    QirSubsetStrict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadDiagnosticKind {
    MalformedInput,
    UnsupportedSemanticConstruct,
}

impl std::fmt::Display for ReadDiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedInput => write!(f, "malformed input"),
            Self::UnsupportedSemanticConstruct => write!(f, "unsupported semantic construct"),
        }
    }
}

#[derive(Clone, Debug, Diagnostic, Error, PartialEq, Eq)]
#[error("{kind}: {context}: {message}")]
#[diagnostic(code(qsc_llvm::read))]
pub struct ReadDiagnostic {
    pub kind: ReadDiagnosticKind,
    pub offset: Option<usize>,
    pub context: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReadReport {
    pub module: model::Module,
    pub diagnostics: Vec<ReadDiagnostic>,
}

pub use bitcode::reader::{
    ParseError, parse_bitcode, parse_bitcode_compatibility, parse_bitcode_compatibility_report,
    parse_bitcode_detailed,
};
pub use bitcode::writer::{
    WriteError, try_write_bitcode, try_write_bitcode_for_target, write_bitcode,
    write_bitcode_for_target,
};
pub use fuzz::qir_smith::{
    EffectiveConfig, GeneratedArtifact, OutputMode, QirProfilePreset, QirSmithConfig,
    QirSmithError, RoundTripKind, generate, generate_bitcode, generate_bitcode_from_bytes,
    generate_checked, generate_checked_from_bytes, generate_from_bytes, generate_module,
    generate_module_from_bytes, generate_text, generate_text_from_bytes,
};
pub use model::Module;
pub use model::builder::ModuleBuilder;
pub use text::reader::{parse_module, parse_module_compatibility, parse_module_detailed};
pub use text::writer::write_module_to_string;
pub use validation::{LlvmIrError, validate_ir, validate_qir_profile};

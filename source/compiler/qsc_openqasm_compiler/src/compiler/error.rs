// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
#[error(transparent)]
#[diagnostic(transparent)]
pub struct Error(pub CompilerErrorKind);

/// Represents the kind of semantic error that occurred during compilation of a QASM file(s).
/// For the most part, these errors are fatal and prevent compilation and are
/// safety checks to ensure that the QASM code is valid.
#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
pub enum CompilerErrorKind {
    #[error("annotations only valid on def and gate statements")]
    #[diagnostic(code("Qasm.Compiler.InvalidAnnotationTarget"))]
    InvalidAnnotationTarget(#[label] Span),
    #[error("gate expects {0} qubit arguments, but {1} were provided")]
    #[diagnostic(code("Qasm.Compiler.InvalidNumberOfQubitArgs"))]
    InvalidNumberOfQubitArgs(usize, usize, #[label] Span),
    #[error("{0} is not defined or is not a valid target for box usage")]
    #[help("Box pragmas can only be used with functions that have no parameters and return void.")]
    #[diagnostic(code("Qasm.Compiler.InvalidBoxPragmaTarget"))]
    InvalidBoxPragmaTarget(String, #[label] Span),
    #[error("Box pragma is missing target")]
    #[diagnostic(code("Qasm.Compiler.MissingBoxPragmaTarget"))]
    MissingBoxPragmaTarget(#[label] Span),
    #[error(
        "Invalid or missing QIR Profile: '{0}'. Please specify one of: `Unrestricted`, `Base`, `Adaptive_RI`, `Adaptive_RIF`."
    )]
    #[diagnostic(code("Qasm.Compiler.InvalidProfilePragmaTarget"))]
    InvalidProfilePragmaTarget(String, #[label] Span),
    #[error("{0} are not supported")]
    #[diagnostic(code("Qasm.Compiler.NotSupported"))]
    NotSupported(String, #[label] Span),
    #[error("Qiskit circuits must have output registers")]
    #[diagnostic(code("Qasm.Compiler.QiskitEntryPointMissingOutput"))]
    QiskitEntryPointMissingOutput(#[label] Span),
    #[error("unexpected annotation: {0}")]
    #[diagnostic(code("Qasm.Compiler.UnknownAnnotation"))]
    UnknownAnnotation(String, #[label] Span),
    #[error("this statement is not yet handled during OpenQASM 3 import: {0}")]
    #[diagnostic(code("Qasm.Compiler.Unimplemented"))]
    Unimplemented(String, #[label] Span),
}

impl From<Error> for crate::Error {
    fn from(val: Error) -> Self {
        crate::Error(crate::ErrorKind::Compiler(val))
    }
}

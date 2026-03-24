// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use thiserror::Error;

use super::Cycle;

#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
#[error(transparent)]
#[diagnostic(transparent)]
pub struct Error(pub ErrorKind);

#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
pub enum ErrorKind {
    #[error("Not Found: {1}")]
    NotFound(#[label] Span, String),
    #[error("IO Error: {1}")]
    IO(#[label] Span, String),
    #[error("{1} was already included in: {2}")]
    MultipleInclude(#[label] Span, String, String),
    #[error("Cyclic include:{1}")]
    CyclicInclude(#[label] Span, Cycle),
}

impl From<Error> for crate::error::Error {
    fn from(val: Error) -> Self {
        crate::error::Error(crate::error::ErrorKind::IO(val))
    }
}

impl Error {
    pub(crate) fn with_offset(self, offset: u32) -> Self {
        match self {
            Error(ErrorKind::NotFound(span, msg)) => Error(ErrorKind::NotFound(span + offset, msg)),
            Error(ErrorKind::IO(span, msg)) => Error(ErrorKind::IO(span + offset, msg)),
            Error(ErrorKind::MultipleInclude(span, inc, incs)) => {
                Error(ErrorKind::MultipleInclude(span + offset, inc, incs))
            }
            Error(ErrorKind::CyclicInclude(span, cycle)) => {
                Error(ErrorKind::CyclicInclude(span + offset, cycle))
            }
        }
    }

    pub(crate) fn with_span(self, span: Span) -> Self {
        match self {
            Error(ErrorKind::NotFound(_, msg)) => Error(ErrorKind::NotFound(span, msg)),
            Error(ErrorKind::IO(_, msg)) => Error(ErrorKind::IO(span, msg)),
            Error(ErrorKind::MultipleInclude(_, inc, incs)) => {
                Error(ErrorKind::MultipleInclude(span, inc, incs))
            }
            Error(ErrorKind::CyclicInclude(_, cycle)) => {
                Error(ErrorKind::CyclicInclude(span, cycle))
            }
        }
    }
}

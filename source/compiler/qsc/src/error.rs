// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
pub use qsc_data_structures::error::WithSource;
use qsc_frontend::compile::PackageStore;
use std::fmt::{self, Debug, Display, Formatter};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub struct WithStack<E> {
    error: E,
    stack_trace: Option<String>,
}

impl<E> WithStack<E> {
    pub(super) fn new(error: E, stack_trace: Option<String>) -> Self {
        WithStack { error, stack_trace }
    }

    pub(super) fn stack_trace(&self) -> Option<&String> {
        self.stack_trace.as_ref()
    }

    pub fn error(&self) -> &E {
        &self.error
    }
}

impl<E: Display> Display for WithStack<E> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}

// #[diagnostic(transparent)] does not seem to work with generics
impl<E: Diagnostic> Diagnostic for WithStack<E> {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.error.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.error.source_code()
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.error.labels()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        self.error.related()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.error.diagnostic_source()
    }
}

pub(super) fn from_eval(
    error: qsc_eval::Error,
    store: &PackageStore,
    stack_trace: Option<String>,
) -> WithStack<WithSource<qsc_eval::Error>> {
    let span = error.span();

    let sources = &store
        .get(span.package)
        .expect("expected to find package id in store")
        .sources;

    WithStack::new(WithSource::from_map(sources, error), stack_trace)
}

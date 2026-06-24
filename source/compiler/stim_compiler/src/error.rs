use crate::lex;
use crate::parser;
use crate::qir;
use miette::Diagnostic;
use thiserror::Error;

#[derive(Clone, Debug, Error, Diagnostic)]
#[error(transparent)]
pub enum Error {
    #[diagnostic(transparent)]
    Lex(#[from] lex::Error),
    #[diagnostic(transparent)]
    Parser(#[from] parser::Error),
    #[diagnostic(transparent)]
    Qir(#[from] qir::Error),
}

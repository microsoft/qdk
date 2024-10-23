// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{Attr, CallableDecl, Item, ItemKind, Package, SpecBody, SpecGen},
    ty::{Prim, Ty},
    visit::Visitor,
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("a callable with the measurement attribute should take at least one argument of type")]
    #[diagnostic(code("Qsc.Measurement.NoArguments"))]
    NoArguments(#[label] Span),

    #[error("a callable with the measurement attribute should only take arguments of type Qubit")]
    #[diagnostic(code("Qsc.Measurement.NonQubitArgument"))]
    NonQubitArgument(#[label] Span),

    #[error("a callable with the measurement attribute should only have outputs of type Result")]
    #[diagnostic(code("Qsc.Measurement.NonResultOutput"))]
    NonResultOutput(#[label] Span),

    #[error("a callable with the measurement attribute should be an intrinsic")]
    #[diagnostic(code("Qsc.Measurement.NotIntrinsic"))]
    NotIntrinsic(#[label] Span),
}

/// For each measurement declaration check that:
///  1. It takes at least one argument.
///  2. It only takes Qubits as arguments.
///  3. It only outputs Results.
///  4. It is an intrinsic.
pub(super) fn validate_measurement_declarations(package: &Package) -> Vec<Error> {
    let mut validator = MeasurementValidator { errors: Vec::new() };
    validator.visit_package(package);
    validator.errors
}

fn validate_measurement_declaration(decl: &CallableDecl, attrs: &[Attr], errors: &mut Vec<Error>) {
    // 1. Check that the declaration takes at least one argument.
    if decl.input.ty == Ty::UNIT {
        errors.push(Error::NoArguments(decl.input.span));
    }

    // 2. Check that the declaration only takes Qubits as arguments.
    match &decl.input.ty {
        Ty::Prim(Prim::Qubit) => (),
        Ty::Tuple(types) => {
            for ty in types {
                if !matches!(ty, Ty::Prim(Prim::Qubit)) {
                    errors.push(Error::NonQubitArgument(decl.input.span));
                    // break so that we don't repeat the same error multiple times
                    break;
                }
            }
        }
        _ => errors.push(Error::NonQubitArgument(decl.input.span)),
    }

    // 3. Check that the declaration only outputs Results: specifically check for Unit.
    if decl.output == Ty::UNIT {
        errors.push(Error::NonResultOutput(decl.span));
    }

    // 3. Check that the declaration only outputs Results.
    match &decl.output {
        Ty::Prim(Prim::Result) => (),
        Ty::Tuple(types) => {
            for ty in types {
                if !matches!(ty, Ty::Prim(Prim::Result)) {
                    errors.push(Error::NonResultOutput(decl.name.span));
                    // break so that we don't repeat the same error multiple times
                    break;
                }
            }
        }
        _ => errors.push(Error::NonResultOutput(decl.name.span)),
    }

    // 4. Check that the declaration is an intrinsic.
    if !decl_is_intrinsic(decl, attrs) {
        errors.push(Error::NotIntrinsic(decl.name.span));
    }
}

/// Returns `true` if a declaration is an intrinsic. A declaration is
/// an intrinsic if it has `body intrinsic;` in its body or if it has
/// the `@SimulatableIntrinsic()` attribute.
fn decl_is_intrinsic(decl: &CallableDecl, attrs: &[Attr]) -> bool {
    matches!(decl.body.body, SpecBody::Gen(SpecGen::Intrinsic))
        || attrs
            .iter()
            .any(|attr| matches!(attr, Attr::SimulatableIntrinsic))
}

/// A helper structure to find and validate measurement callables in a Package.
struct MeasurementValidator {
    errors: Vec<Error>,
}

impl<'a> Visitor<'a> for MeasurementValidator {
    fn visit_item(&mut self, item: &'a Item) {
        if let ItemKind::Callable(callable) = &item.kind {
            if item.attrs.contains(&Attr::Measurement) {
                validate_measurement_declaration(callable, &item.attrs, &mut self.errors);
            }
        }
    }
}

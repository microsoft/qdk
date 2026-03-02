// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{Attr, CallableDecl, Item, ItemKind, Package, SpecBody, SpecGen},
    ty::Ty,
    visit::Visitor,
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("a callable with the @NoiseIntrinsic() attribute should have output of type Unit")]
    #[diagnostic(code("Qsc.NoiseIntrinsic.NonResultOutput"))]
    NonUnitOutput(#[label] Span),

    #[error("a callable with the @NoiseIntrinsic() attribute should be an intrinsic")]
    #[diagnostic(code("Qsc.NoiseIntrinsic.NotIntrinsic"))]
    NotIntrinsic(#[label] Span),
}

/// For each noise intrinsic declaration check that:
///  1. It only outputs Unit.
///  2. It is an intrinsic.
pub(super) fn validate_noise_intrinsic_declarations(package: &Package) -> Vec<Error> {
    let mut validator = NoiseIntrinsicValidator { errors: Vec::new() };
    validator.visit_package(package);
    validator.errors
}

fn validate_noise_intrinsic_declaration(
    decl: &CallableDecl,
    attrs: &[Attr],
    errors: &mut Vec<Error>,
) {
    // 1. Check that the declaration only outputs Unit.
    if decl.output != Ty::UNIT {
        errors.push(Error::NonUnitOutput(decl.span));
    }

    // 2. Check that the declaration is an intrinsic.
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

/// A helper structure to find and validate noise intrinsic callables in a Package.
struct NoiseIntrinsicValidator {
    errors: Vec<Error>,
}

impl<'a> Visitor<'a> for NoiseIntrinsicValidator {
    fn visit_item(&mut self, item: &'a Item) {
        if let ItemKind::Callable(callable) = &item.kind
            && item.attrs.contains(&Attr::NoiseIntrinsic)
        {
            validate_noise_intrinsic_declaration(callable, &item.attrs, &mut self.errors);
        }
    }
}

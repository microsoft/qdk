// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Intrinsic signature pre-pass — runs before any structural rewrite.
//!
//! Rejects reachable intrinsic callables whose parameter or return types
//! contain non-empty tuples or user-defined types, which cannot survive UDT
//! erasure and tuple-decompose (an intrinsic has no body to rewrite). A failure
//! is fatal and short-circuits the pipeline with
//! [`Error::UnsupportedParamType`] / [`Error::UnsupportedReturnType`] before any
//! other pass runs.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_fir::fir::{Attr, CallableImpl, ItemKind, PackageId, PackageSpan, PackageStore};
use qsc_fir::ty::Ty;
use thiserror::Error;

use crate::reachability;

/// Errors produced by intrinsic callable signature validation.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("intrinsic callable `{0}` has unsupported parameter type `{1}`")]
    #[diagnostic(code("Qdk.Qsc.FirTransform.UnsupportedIntrinsicParamType"))]
    #[diagnostic(help(
        "intrinsic callable parameters cannot be non-empty tuples or user-defined types"
    ))]
    UnsupportedParamType(
        String,
        String,
        #[label("unsupported parameter type")] PackageSpan,
    ),

    #[error("intrinsic callable `{0}` has unsupported return type `{1}`")]
    #[diagnostic(code("Qdk.Qsc.FirTransform.UnsupportedIntrinsicReturnType"))]
    #[diagnostic(help(
        "intrinsic callable return types cannot be non-empty tuples or user-defined types"
    ))]
    UnsupportedReturnType(
        String,
        String,
        #[label("unsupported return type")] PackageSpan,
    ),
}

impl Error {
    /// Returns the package that owns this diagnostic.
    #[must_use]
    pub fn owner(&self) -> PackageId {
        self.package_span().package
    }

    /// Returns the package-qualified source label for this diagnostic.
    #[must_use]
    pub fn package_span(&self) -> PackageSpan {
        match self {
            Self::UnsupportedParamType(_, _, span) | Self::UnsupportedReturnType(_, _, span) => {
                *span
            }
        }
    }
}

/// Returns `true` when `ty` is a tuple (non-unit) or UDT, which are
/// unsupported in intrinsic callable signatures.
fn is_unsupported_intrinsic_type(ty: &Ty) -> bool {
    match ty {
        Ty::Tuple(items) if !items.is_empty() => true,
        Ty::Udt(_) => true,
        _ => false,
    }
}

/// Validates that reachable intrinsic callables in `package_id` have no tuple
/// or UDT parameter/return types.
#[must_use]
pub fn validate_intrinsic_types(store: &PackageStore, package_id: PackageId) -> Vec<Error> {
    let reachable = reachability::collect_reachable_from_entry(store, package_id);
    let mut errors = Vec::new();

    for item_id in &reachable {
        let package = store.get(item_id.package);
        let Some(item) = package.items.get(item_id.item) else {
            continue;
        };

        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };

        if !matches!(
            decl.implementation,
            CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_)
        ) {
            continue;
        }

        let name = decl.name.name.to_string();

        for param in package.derive_callable_input_params(decl) {
            if is_unsupported_intrinsic_type(&param.ty) {
                errors.push(Error::UnsupportedParamType(
                    name.clone(),
                    format!("{}", param.ty),
                    (item_id.package, decl.span).into(),
                ));
            }
        }

        // Measurement callables are allowed to return tuples because partial
        // eval decomposes the tuple return into output-recording parameters.
        let skip_tuple_return = decl.attrs.contains(&Attr::Measurement)
            && matches!(&decl.output, Ty::Tuple(items) if !items.is_empty());
        if !skip_tuple_return && is_unsupported_intrinsic_type(&decl.output) {
            errors.push(Error::UnsupportedReturnType(
                name,
                format!("{}", decl.output),
                (item_id.package, decl.span).into(),
            ));
        }
    }

    errors
}

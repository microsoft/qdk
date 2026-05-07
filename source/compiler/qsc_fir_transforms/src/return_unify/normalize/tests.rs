// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub(super) use crate::return_unify::tests::{
    check_no_returns_q, check_structure, compile_return_unified,
};
pub(super) use expect_test::{Expect, expect};
pub(super) use indoc::indoc;

use qsc_data_structures::language_features::LanguageFeatures;
use qsc_parse::namespaces;

mod fixpoint;
mod flag_strategy;
mod hoist_expression;
mod nested_constructs;
mod regression_and_depth;
mod three_level;
mod three_level_mixed;

// Each of the following tests exercises the `normalize::hoist_returns_to_statement_boundary`
// pre-pass by placing a `Return` inside a compound expression position. The
// invariant `check_no_returns` asserts that the combined hoist + transform
// produces PostReturnUnify-clean FIR (no `ExprKind::Return` survives).

fn rendered_qsharp_parse_diagnostics(rendered: &str) -> Vec<String> {
    let rendered_without_entry = if let Some((before_entry, _)) = rendered.split_once("// entry\n")
    {
        before_entry.trim_end().to_string()
    } else {
        rendered.to_string()
    };

    let (_namespaces, errors) = namespaces(
        &rendered_without_entry,
        Some("roundtrip.qs"),
        LanguageFeatures::default(),
    );
    errors
        .into_iter()
        .map(|error| format!("{error:?}"))
        .collect()
}

pub(super) fn check_no_returns_q_roundtrip(source: &str, expect: &Expect) {
    check_no_returns_q(source, expect);

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    let diagnostics = rendered_qsharp_parse_diagnostics(&rendered);

    assert!(
        diagnostics.is_empty(),
        "generated Q# should parse without diagnostics:\n{}\n\nrendered:\n{rendered}",
        diagnostics.join("\n")
    );
}

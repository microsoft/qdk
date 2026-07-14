// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::get_references;
use crate::Encoding;
use crate::test_utils::openqasm::compile_with_markers;

fn check(source_with_markers: &str) {
    let (compilation, cursor_position, target_spans) = compile_with_markers(source_with_markers);
    let actual = get_references(
        &compilation,
        "<source>",
        cursor_position,
        Encoding::Utf8,
        true,
    )
    .into_iter()
    .map(|location| location.range)
    .collect::<Vec<_>>();

    assert_eq!(actual.len(), target_spans.len());
    for target in target_spans {
        assert!(
            actual.contains(&target),
            "expected {actual:?} to contain {target:?}"
        );
    }
}

#[test]
fn broadcast_register_reference_occurs_once_per_source_token() {
    check(
        r#"
        include "stdgates.inc";
        qubit[8] ◉t↘argets◉;
        h ◉targets◉;
        "#,
    );
}

#[test]
fn each_equal_width_register_reference_occurs_once() {
    check(
        r#"
        include "stdgates.inc";
        qubit[4] ◉c↘ontrols◉;
        qubit[4] targets;
        cx ◉controls◉, targets;
        "#,
    );
}

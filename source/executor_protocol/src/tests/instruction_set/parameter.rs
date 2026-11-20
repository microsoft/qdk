// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A parameter kind's stable form, with the `serde` feature: Qodec's name
//! of the kind.

use crate::ParameterKind;

/// A peer's set names its parameter kinds as Qodec does: any other text is
/// refused when read, naming it, rather than read as some other kind.
#[test]
fn a_text_that_is_no_kinds_name_is_refused_naming_it() {
    for (row, text) in [
        ("a kind Qodec lacks", "float"),
        ("a kind by its Rust name", "Text"),
        ("a kind's name in another case", "NUMBER"),
    ] {
        let refusal = serde_json::from_value::<ParameterKind>(serde_json::json!(text))
            .expect_err("a text that is no kind's name is refused")
            .to_string();
        assert!(refusal.contains(text), "{row}: {refusal:?} names {text:?}");
    }
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A parameter kind's stable form, with the `serde` feature: Qodec's name
//! of the kind.

use crate::ParameterKind;
use serde_json::json;

/// A peer reads the kinds of a set's parameters by the names its own
/// messages and loaders display: each kind is written as its name, and
/// read back from it.
#[test]
fn a_kind_is_written_and_read_as_its_displayed_name() {
    for kind in [
        ParameterKind::Bit,
        ParameterKind::Number,
        ParameterKind::Integer,
        ParameterKind::Boolean,
        ParameterKind::Text,
        ParameterKind::Pauli,
    ] {
        let name = json!(kind.to_string());
        assert_eq!(
            serde_json::to_value(kind).expect("a kind writes its form"),
            name,
            "{kind}"
        );
        assert_eq!(
            serde_json::from_value::<ParameterKind>(name).expect("a kind's name reads back"),
            kind,
            "{kind}"
        );
    }
}

/// A peer's set names its parameter kinds as Qodec does: any other text is
/// refused when read, naming it, rather than read as some other kind.
#[test]
fn a_text_that_is_not_a_kinds_name_is_refused_naming_it() {
    for (row, text) in [
        ("a kind Qodec lacks", "float"),
        ("a kind by its Rust name", "Text"),
        ("a kind's name in another case", "NUMBER"),
    ] {
        let refusal = serde_json::from_value::<ParameterKind>(json!(text))
            .expect_err("a text that is not a kind's name is refused")
            .to_string();
        assert!(refusal.contains(text), "{row}: {refusal:?} names {text:?}");
    }
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::llvm_global_name;

#[test]
fn keeps_unquoted_identifier_when_valid() {
    assert_eq!(llvm_global_name("ApplyX"), "@ApplyX");
    assert_eq!(llvm_global_name("name.with-dash_01"), "@name.with-dash_01");
}

#[test]
fn quotes_identifier_with_special_characters() {
    assert_eq!(
        llvm_global_name("ApplyGeneric<Qubit, AdjCtl>{X}"),
        "@\"ApplyGeneric<Qubit, AdjCtl>{X}\""
    );
}

#[test]
fn escapes_quotes_backslashes_and_non_ascii_bytes() {
    assert_eq!(llvm_global_name("foo\"bar\\baz"), "@\"foo\\22bar\\5Cbaz\"");
    assert_eq!(llvm_global_name("na\nme"), "@\"na\\0Ame\"");
}

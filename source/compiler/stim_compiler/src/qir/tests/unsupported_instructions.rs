// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn i_error_yields_not_supported_error() {
    let source = "I_ERROR 0";
    check(source, &expect![[r#"
        Stim.UnsupportedInstruction

          x unsupported instruction: I_ERROR
    "#]]);
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;
use indoc::indoc;

#[test]
#[ignore = "unsupported instruction"]
fn repeat() {
    let source = indoc! {"
        REPEAT 10 {
          CNOT 0 1
          CNOT 2 1
          M 1
        }
    "};
    check(source, &expect![[""]]);
}

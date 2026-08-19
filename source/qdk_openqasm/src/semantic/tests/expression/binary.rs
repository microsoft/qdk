// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod arithmetic_conversions;
mod comparison;
mod complex;
mod duration;
mod ident;

use crate::semantic::{ast::StmtKind, tests::check_map};
use expect_test::expect;

#[test]
fn bitwise_xor_binds_more_tightly_than_or() {
    check_map(
        "const uint[8] result = uint[8](1) | uint[8](2) ^ uint[8](3);",
        &expect!["Int(1)"],
        |program, symbols| {
            let StmtKind::ClassicalDecl(declaration) = program.statements[0].kind.as_ref() else {
                panic!("expected a classical declaration");
            };
            symbols[declaration.symbol_id]
                .get_const_value()
                .expect("constant declaration should be evaluated")
                .to_string()
        },
    );
}

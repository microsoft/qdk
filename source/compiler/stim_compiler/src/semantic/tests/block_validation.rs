// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn repeat_zero_times_yields_error() {
    let source = indoc! {"
                REPEAT 0 {
                    X 0
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.ZeroRepeatCount

              x a REPEAT count of zero is not supported
               ,-[1:8]
             1 | REPEAT 0 {
               :        ^
             2 |     X 0
               `----
        "#]],
    );
}

#[test]
fn blockless_repeat_yields_error() {
    check(
        "REPEAT 2",
        &expect![[r#"
        Qdk.Stim.Semantic.InstructionWithoutBlock

          x REPEAT instruction must start a block
           ,----
         1 | REPEAT 2
           : ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn blockless_select_yields_error() {
    let source = indoc! {"
                X 0
                SELECT
                M 0
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.InstructionWithoutBlock

              x SELECT instruction must start a block
               ,-[2:1]
             1 | X 0
             2 | SELECT
               : ^^^^^^
             3 | M 0
               `----
        "#]],
    );
}

#[test]
fn unknown_block_instruction_yields_error() {
    let source = indoc! {"
        FOO {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Semantic.UnknownInstruction

          x unknown instruction: FOO
           ,-[1:1]
         1 | FOO {
           : ^^^
         2 |   X 0
           `----
    "#]],
    );
}

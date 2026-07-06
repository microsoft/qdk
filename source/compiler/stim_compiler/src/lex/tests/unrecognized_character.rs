// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn common_symbols_are_unrecognized() {
    check(
        "@",
        &expect![[r#"
            Qdk.Stim.UnrecognizedCharacter

              x unrecognized character
               ,----
             1 | @
               : ^
               `----
        "#]],
    );
    check(
        "$",
        &expect![[r#"
            Qdk.Stim.UnrecognizedCharacter

              x unrecognized character
               ,----
             1 | $
               : ^
               `----
        "#]],
    );
    check(
        "%",
        &expect![[r#"
            Qdk.Stim.UnrecognizedCharacter

              x unrecognized character
               ,----
             1 | %
               : ^
               `----
        "#]],
    );
    check(
        "?",
        &expect![[r#"
            Qdk.Stim.UnrecognizedCharacter

              x unrecognized character
               ,----
             1 | ?
               : ^
               `----
        "#]],
    );
}

#[test]
fn leading_underscore_is_unrecognized() {
    // Identifiers must start with a letter, so a leading '_' is unrecognized
    // and the rest lexes as a separate identifier.
    check(
        "_foo",
        &expect![[r#"
            Qdk.Stim.UnrecognizedCharacter

              x unrecognized character
               ,----
             1 | _foo
               : ^
               `----

            instruction_name(foo) [1-4]"#]],
    );
}

#[test]
fn lexing_recovers_after_unrecognized_character() {
    check(
        "@H",
        &expect![[r#"
            Qdk.Stim.UnrecognizedCharacter

              x unrecognized character
               ,----
             1 | @H
               : ^
               `----

            instruction_name(H) [1-2]"#]],
    );
    check(
        "@1",
        &expect![[r#"
            Qdk.Stim.UnrecognizedCharacter

              x unrecognized character
               ,----
             1 | @1
               : ^
               `----

            uint(1) [1-2]"#]],
    );
}

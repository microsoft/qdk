// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn rec_with_index() {
    check("rec[-1]", &expect!["rec(rec[-1]) [0-7]"]);
    check("rec[-10]", &expect!["rec(rec[-10]) [0-8]"]);
}

#[test]
fn multiple_rec_tokens() {
    check(
        "rec[-2] rec[-1]",
        &expect![[r#"
        rec(rec[-2]) [0-7]
        rec(rec[-1]) [8-15]"#]],
    );
}

#[test]
fn sweep_with_index() {
    check("sweep[0]", &expect!["sweep(sweep[0]) [0-8]"]);
}

#[test]
fn tag() {
    check("[tag]", &expect!["tag([tag]) [0-5]"]);
    check("[]", &expect!["tag([]) [0-2]"]);
    check("[a b c]", &expect!["tag([a b c]) [0-7]"]);
}

#[test]
fn tag_stops_at_first_close_bracket() {
    check(
        "[a]b",
        &expect![[r#"
            tag([a]) [0-3]
            instruction_name(b) [3-4]"#]],
    );
}

#[test]
fn unterminated_tag_at_eof_is_error() {
    check(
        "[tag",
        &expect![[r#"
        Qdk.Stim.Lex.UnterminatedTag

          x tag was not closed with ']' before the end of the line
           ,----
         1 | [tag
           : ^^^^
           `----
    "#]],
    );
}

#[test]
fn unterminated_tag_does_not_consume_next_line() {
    check(
        "H[bad\nX[tag] 0",
        &expect![[r#"
        instruction_name(H) [0-1]
        Qdk.Stim.Lex.UnterminatedTag

          x tag was not closed with ']' before the end of the line
           ,-[1:2]
         1 | H[bad
           :  ^^^^
         2 | X[tag] 0
           `----

        newline(\n) [5-6]
        instruction_name(X) [6-7]
        tag([tag]) [7-12]
        uint(0) [13-14]"#]],
    );
}

#[test]
fn longer_identifiers_are_not_rec_or_sweep() {
    // Only the exact identifiers "rec" and "sweep" are special.
    check("record", &expect!["instruction_name(record) [0-6]"]);
    check("sweeper", &expect!["instruction_name(sweeper) [0-7]"]);
}

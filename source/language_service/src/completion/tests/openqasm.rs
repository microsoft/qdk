// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{completion::tests::check, test_utils::openqasm::compile_with_markers};
use expect_test::{Expect, expect};
use indoc::indoc;

fn check_single_file(source_with_cursor: &str, completions_to_check: &[&str], expect: &Expect) {
    let (compilation, cursor_position, _) = compile_with_markers(source_with_cursor);

    check(
        &compilation,
        "<source>",
        cursor_position,
        completions_to_check,
        expect,
    );
}

#[test]
fn in_empty_file_contains_openqasm() {
    check_single_file(
        indoc! {r#"
        ↘
    }"#},
        &["OPENQASM"],
        &expect![[r#"
            found, sorted:
              "OPENQASM" (Keyword)
        "#]],
    );
}

#[test]
fn in_file_after_openqasm_contains_keywords_containing_i() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        i↘
    }"#},
        &["if", "include", "input", "inv"],
        &expect![[r#"
            found, sorted:
              "if" (Keyword)
              "include" (Keyword)
              "input" (Keyword)
              "inv" (Keyword)
        "#]],
    );
}

#[test]
fn annotation_names_not_offered_at_statement_start() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        ↘
    }"#},
        &[
            "qdk.qir.intrinsic",
            "qdk.qir.noise_intrinsic",
            "qdk.qir.profile",
        ],
        &expect![[r#"
            not found:
              "qdk.qir.intrinsic"
              "qdk.qir.noise_intrinsic"
              "qdk.qir.profile"
        "#]],
    );
}

#[test]
fn annotation_after_at_offers_qdk_annotations() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        @↘
    }"#},
        &[
            "qdk.qir.intrinsic",
            "qdk.qir.noise_intrinsic",
            "qdk.qir.profile",
        ],
        &expect![[r#"
            found, sorted:
              "qdk.qir.intrinsic" (Interface)
              "qdk.qir.noise_intrinsic" (Interface)
              "qdk.qir.profile" (Interface)
        "#]],
    );
}

#[test]
fn annotation_name_boundaries_offer_only_annotations() {
    for source in [
        "OPENQASM 3.0;\n@↘qdk.qir.profile\ndef foo() {}",
        "OPENQASM 3.0;\n@qdk.↘qir.profile\ndef foo() {}",
        "OPENQASM 3.0;\n@qdk.qir.profile↘\ndef foo() {}",
    ] {
        check_single_file(
            source,
            &["qdk.qir.profile", "foo"],
            &expect![[r#"
                found, sorted:
                  "qdk.qir.profile" (Interface)

                not found:
                  "foo"
            "#]],
        );
    }
}

#[test]
fn pragma_name_position_offers_supported_pragmas() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        #pragma ↘
    }"#},
        &["qdk.box.open", "qdk.box.close", "qdk.qir.profile"],
        &expect![[r#"
            found, sorted:
              "qdk.box.close" (Keyword)
              "qdk.box.open" (Keyword)
              "qdk.qir.profile" (Keyword)
        "#]],
    );
}

#[test]
fn pragma_partial_name_offers_supported_pragmas() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        #pragma qdk↘
    }"#},
        &["qdk.box.open", "qdk.box.close", "qdk.qir.profile"],
        &expect![[r#"
            found, sorted:
              "qdk.box.close" (Keyword)
              "qdk.box.open" (Keyword)
              "qdk.qir.profile" (Keyword)
        "#]],
    );
}

#[test]
fn pragma_name_boundaries_offer_only_pragmas() {
    for source in [
        "OPENQASM 3.0;\ndef foo() {}\n#pragma ↘qdk.qir.profile",
        "OPENQASM 3.0;\ndef foo() {}\n#pragma qdk.↘qir.profile",
        "OPENQASM 3.0;\ndef foo() {}\n#pragma qdk.qir.profile↘",
    ] {
        check_single_file(
            source,
            &["qdk.qir.profile", "foo"],
            &expect![[r#"
                found, sorted:
                  "qdk.qir.profile" (Keyword)

                not found:
                  "foo"
            "#]],
        );
    }
}

#[test]
fn incomplete_pragma_introducer_uses_name_fallback() {
    for source in [
        "OPENQASM 3.0;\n#↘",
        "OPENQASM 3.0;\n#pr↘",
        "OPENQASM 3.0;\n#pragma↘",
        "OPENQASM 3.0;\r#pr↘",
    ] {
        check_single_file(
            source,
            &["qdk.box.open", "qdk.qir.profile"],
            &expect![[r#"
                found, sorted:
                  "qdk.box.open" (Keyword)
                  "qdk.qir.profile" (Keyword)
            "#]],
        );
    }
}

#[test]
fn pragma_profile_value_offers_profiles() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        #pragma qdk.qir.profile ↘
    }"#},
        &[
            "Base",
            "Adaptive_RI",
            "Adaptive_RIF",
            "Adaptive",
            "Unrestricted",
        ],
        &expect![[r#"
            found, sorted:
              "Adaptive" (Keyword)
              "Adaptive_RI" (Keyword)
              "Adaptive_RIF" (Keyword)
              "Base" (Keyword)
              "Unrestricted" (Keyword)
        "#]],
    );
}

#[test]
fn directive_value_boundaries_do_not_offer_names() {
    for source in [
        "OPENQASM 3.0;\nint ordinary_local;\n#pragma qdk.qir.profile ↘Base",
        "OPENQASM 3.0;\nint ordinary_local;\n#pragma qdk.qir.profile Ba↘se",
        "OPENQASM 3.0;\nint ordinary_local;\n@qdk.qir.profile ↘Base\ndef foo() {}",
        "OPENQASM 3.0;\nint ordinary_local;\n@qdk.qir.profile Ba↘se\ndef foo() {}",
    ] {
        check_single_file(
            source,
            &["Base", "ordinary_local", "qdk.qir.profile"],
            &expect![[r#"
                found, sorted:
                  "Base" (Keyword)

                not found:
                  "ordinary_local"
                  "qdk.qir.profile"
            "#]],
        );
    }
}

#[test]
fn malformed_directives_stay_scoped() {
    check_single_file(
        "OPENQASM 3.0;\ninput int ordinary_local;\n@!↘malformed",
        &["qdk.qir.profile", "ordinary_local"],
        &expect![
            "found, sorted:\n  \"qdk.qir.profile\" (Interface)\n\nnot found:\n  \"ordinary_local\"\n"
        ],
    );
    check_single_file(
        "OPENQASM 3.0;\ninput int ordinary_local;\n#pragma !malformed↘",
        &["qdk.qir.profile", "ordinary_local"],
        &expect![[r#"
            found, sorted:
              "qdk.qir.profile" (Keyword)

            not found:
              "ordinary_local"
        "#]],
    );
}

#[test]
fn directive_context_stops_at_newline() {
    check_single_file(
        "OPENQASM 3.0;\n#pragma qdk.qir.profile Base\ninput int ordinary_local;\n↘",
        &["if", "qdk.qir.profile", "Base"],
        &expect![[r#"
            found, sorted:
              "if" (Keyword)

            not found:
              "qdk.qir.profile"
              "Base"
        "#]],
    );
}

#[test]
fn pragma_box_value_offers_target_functions() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        def box_begin() {}
        def box_end() {}
        def with_param(int x) {}
        #pragma qdk.box.open ↘
    }"#},
        &[
            "box_begin",
            "box_end",
            "with_param",
            "Base",
            "qdk.qir.profile",
        ],
        &expect![[r#"
            found, sorted:
              "box_begin" (Function)
              "box_end" (Function)

            not found:
              "with_param"
              "Base"
              "qdk.qir.profile"
        "#]],
    );
}

#[test]
fn annotation_profile_value_offers_profiles() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        @qdk.qir.profile ↘
        def foo() {}
    }"#},
        &[
            "Base",
            "Adaptive_RI",
            "Adaptive_RIF",
            "Adaptive",
            "Unrestricted",
        ],
        &expect![[r#"
            found, sorted:
              "Adaptive" (Keyword)
              "Adaptive_RI" (Keyword)
              "Adaptive_RIF" (Keyword)
              "Base" (Keyword)
              "Unrestricted" (Keyword)
        "#]],
    );
}

#[test]
fn annotation_profile_value_at_eof_offers_profiles() {
    for source in [
        "OPENQASM 3.0;\n@qdk.qir.profile ↘",
        "OPENQASM 3.0;\n@qdk.qir.profile Ba↘se",
    ] {
        check_single_file(
            source,
            &[
                "Base",
                "Adaptive_RI",
                "Adaptive_RIF",
                "Adaptive",
                "Unrestricted",
            ],
            &expect![[r#"
                found, sorted:
                  "Adaptive" (Keyword)
                  "Adaptive_RI" (Keyword)
                  "Adaptive_RIF" (Keyword)
                  "Base" (Keyword)
                  "Unrestricted" (Keyword)
            "#]],
        );
    }
}

#[test]
fn local_vars() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        input int num_samples;
        output float angle_value;
        ↘
    }"#},
        &["num_samples", "angle_value"],
        &expect![[r#"
            found, sorted:
              "angle_value" (Variable)
                detail: "angle_value : Double"
              "num_samples" (Variable)
                detail: "num_samples : Int"
        "#]],
    );
}

#[test]
fn local_vars_doesnt_pick_up_variables_declared_after_cursor() {
    check_single_file(
        indoc! {r#"
        OPENQASM 3.0;
        input int num_samples;
        ↘
        output float angle_value;
    }"#},
        &["num_samples", "angle_value"],
        &expect![[r#"
            found, sorted:
              "num_samples" (Variable)
                detail: "num_samples : Int"

            not found:
              "angle_value"
        "#]],
    );
}

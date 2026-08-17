// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::visit::Visitor;

use crate::loop_control::LoopControl;

fn check(file: &str, expect: &Expect) {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let mut loop_control = LoopControl::default();
    loop_control.visit_package(&unit.package);
    let errors = loop_control.errors;
    expect.assert_debug_eq(&errors);
}

#[test]
fn break_outside_loop_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    break;
                }
            }
        "},
        &expect![[r#"
            [
                OutsideLoop(
                    Span {
                        lo: 54,
                        hi: 59,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn continue_outside_loop_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    continue;
                }
            }
        "},
        &expect![[r#"
            [
                OutsideLoop(
                    Span {
                        lo: 54,
                        hi: 62,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn break_in_for_body_is_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    for _ in 0..3 {
                        break;
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn continue_in_for_body_is_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    for _ in 0..3 {
                        continue;
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn break_in_while_body_is_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    while true {
                        break;
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn continue_in_while_body_is_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    while true {
                        continue;
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn break_in_repeat_body_is_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    repeat {
                        break;
                    } until true;
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn continue_in_repeat_body_is_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    repeat {
                        continue;
                    } until true;
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn break_in_while_condition_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    while break {
                    }
                }
            }
        "},
        &expect![[r#"
            [
                InLoopHeader(
                    Span {
                        lo: 60,
                        hi: 65,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn continue_in_while_condition_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    while continue {
                    }
                }
            }
        "},
        &expect![[r#"
            [
                InLoopHeader(
                    Span {
                        lo: 60,
                        hi: 68,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn break_in_repeat_until_condition_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    repeat {
                    } until break;
                }
            }
        "},
        &expect![[r#"
            [
                InLoopHeader(
                    Span {
                        lo: 79,
                        hi: 84,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn break_in_fixup_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    repeat {
                    } until true
                    fixup {
                        break;
                    }
                }
            }
        "},
        &expect![[r#"
            [
                InFixup(
                    Span {
                        lo: 112,
                        hi: 117,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn continue_in_fixup_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    repeat {
                    } until true
                    fixup {
                        continue;
                    }
                }
            }
        "},
        &expect![[r#"
            [
                InFixup(
                    Span {
                        lo: 112,
                        hi: 120,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn nested_loops_inner_break_binds_to_inner_loop() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    for _ in 0..3 {
                        for _ in 0..3 {
                            break;
                        }
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn nested_loops_inner_continue_binds_to_inner_loop() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    while true {
                        repeat {
                            continue;
                        } until true;
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn break_in_for_iterable_with_enclosing_loop_is_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    for _ in 0..3 {
                        for _ in { break; 0..3 } {
                        }
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn continue_in_for_iterable_binds_to_enclosing_loop() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    while true {
                        for _ in { continue; 0..3 } {
                        }
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn break_in_for_iterable_without_enclosing_loop_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    for _ in { break; 0..3 } {
                    }
                }
            }
        "},
        &expect![[r#"
            [
                OutsideLoop(
                    Span {
                        lo: 65,
                        hi: 70,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn break_in_nested_loop_body_within_while_condition_is_allowed() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    while { for _ in 0..3 { break; } true } {
                    }
                }
            }
        "},
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn break_in_inner_while_condition_within_outer_loop_errors() {
    // A `break` directly in an inner loop's condition is rejected even though an
    // outer loop encloses it: a condition-position `break`/`continue` can never
    // bind to an enclosing loop, so the forbidden-header position takes
    // precedence over the enclosing loop depth.
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    for _ in 0..3 {
                        while break {
                        }
                    }
                }
            }
        "},
        &expect![[r#"
            [
                InLoopHeader(
                    Span {
                        lo: 88,
                        hi: 93,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn continue_in_inner_repeat_until_condition_within_outer_loop_errors() {
    // A `continue` directly in an inner `repeat`-loop's `until` condition is
    // rejected even though an outer loop encloses it, mirroring the `while`
    // condition case: the forbidden-header position takes precedence over the
    // enclosing loop depth.
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    while true {
                        repeat {
                        } until continue;
                    }
                }
            }
        "},
        &expect![[r#"
            [
                InLoopHeader(
                    Span {
                        lo: 108,
                        hi: 116,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn break_in_lambda_defined_in_loop_errors() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    for _ in 0..3 {
                        let f = () => {
                            if true {
                                break;
                            }
                        };
                        f();
                    }
                }
            }
        "},
        &expect![[r#"
            [
                OutsideLoop(
                    Span {
                        lo: 144,
                        hi: 149,
                    },
                ),
            ]
        "#]],
    );
}

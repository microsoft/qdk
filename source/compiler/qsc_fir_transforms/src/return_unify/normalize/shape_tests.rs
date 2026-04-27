// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{PipelineStage, return_unify::unify_returns, test_utils::compile_and_run_pipeline_to};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::assigner::Assigner;

/// Compiles Q# source through `Mono`, captures a pretty-printed snapshot of
/// the package, runs `unify_returns` directly, captures a second snapshot,
/// and asserts the concatenated `BEFORE` / `AFTER` string matches `expect`.
///
/// Shape-sensitive alternative to [`check_no_returns`]. Prefer behavior-only
/// assertions for the majority of tests; reserve this for cases where the
/// rewriting shape is itself under test.
fn check_before_after(source: &str, expect: &Expect) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let before = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    unify_returns(&mut store, pkg_id, &mut assigner);
    let after = crate::pretty::write_package_qsharp(&store, pkg_id);
    let combined = format!("BEFORE:\n{before}\nAFTER:\n{after}");
    expect.assert_eq(&combined);
}

#[test]
fn hoist_return_in_call_argument_shape_snapshot() {
    // Flagship shape test — the same Q# shape as
    // `hoist_return_in_call_argument`, but asserting the BEFORE/AFTER FIR
    // pretty-print to lock the hoist shape.
    check_before_after(
        indoc! {r#"
            namespace Test {
                function Add(a : Int, b : Int) : Int { a + b }
                @EntryPoint()
                function Main() : Int {
                    let x = Add((return 1), 2);
                    x
                }
            }
        "#},
        &expect![[r#"
            BEFORE:
            // namespace Test
            function Add(a : Int, b : Int) : Int {
                body {
                    x + b
                }
            }
            function Main() : Int {
                body {
                    let x : Int = Add(return 1, 2);
                    x
                }
            }
            // entry
            Main()

            AFTER:
            // namespace Test
            function Add(a : Int, b : Int) : Int {
                body {
                    x + b
                }
            }
            function Main() : Int {
                body {
                    let _ : ((Int, Int) -> Int) = Add;
                    1
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn while_condition_return_shape_snapshot() {
    check_before_after(
        indoc! {r#"
            namespace Test {
                @EntryPoint()
                function Main() : Int {
                    while if true {
                        if true {
                            return 31;
                        } else {
                            false
                        }
                    } else {
                        false
                    } {
                        let _ = 0;
                    }
                    0
                }
            }
        "#},
        &expect![[r#"
            BEFORE:
            // namespace Test
            function Main() : Int {
                body {
                    while if true {
                        if true {
                            return 31;
                        } else {
                            false
                        }

                    } else {
                        false
                    }
                    {
                        let _ : Int = 0;
                    }

                    0
                }
            }
            // entry
            Main()

            AFTER:
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    while not __has_returned and if true {
                        if true {
                            {
                                __ret_val = 31;
                                __has_returned = true;
                            };
                        } else {
                            false
                        }

                    } else {
                        false
                    }
                    {
                        let _ : Int = 0;
                    }

                    let __trailing_result : Int = 0;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn while_local_initializer_return_shape_snapshot() {
    check_before_after(
        indoc! {r#"
            namespace Test {
                function Add(a : Int, b : Int) : Int { a + b }

                @EntryPoint()
                function Main() : Int {
                    mutable i = 0;
                    while i < 3 {
                        let _ = if i == 1 {
                            Add((return 42), i)
                        };
                        i += 1;
                    }
                    i + 5
                }
            }
        "#},
        &expect![[r#"
            BEFORE:
            // namespace Test
            function Add(a : Int, b : Int) : Int {
                body {
                    i + b
                }
            }
            function Main() : Int {
                body {
                    mutable i : Int = 0;
                    while i < 3 {
                        let _ : Unit = if i == 1 {
                            Add(return 42, i)
                        };
                        i += 1;
                    }

                    i + 5
                }
            }
            // entry
            Main()

            AFTER:
            // namespace Test
            function Add(a : Int, b : Int) : Int {
                body {
                    i + b
                }
            }
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable i : Int = 0;
                    while not __has_returned and i < 3 {
                        let _ : Unit = if i == 1 {
                            let _ : ((Int, Int) -> Int) = Add;
                            {
                                __ret_val = 42;
                                __has_returned = true;
                            };
                        };
                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let __trailing_result : Int = i + 5;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

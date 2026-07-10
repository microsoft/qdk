// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::expect;
use indoc::indoc;

use crate::loop_unification::test_utils::check;

#[test]
fn convert_for_array() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : Int[]) : Unit {
                for i in arr {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main(arr : Int[]) : Unit {
                {
                    let _array_id_17 = arr;
                    let _len_id_21 = Length(_array_id_17);
                    mutable _index_id_26 = 0;
                    while _index_id_26 < _len_id_21 {
                        let i = _array_id_17[_index_id_26];
                        let x = "Hello World";
                        _index_id_26 += 1;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_array_deconstruct() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : (Int, Double)[]) : Unit {
                for (i, d) in arr {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main(arr : (Int, Double)[]) : Unit {
                {
                    let _array_id_20 = arr;
                    let _len_id_24 = Length(_array_id_20);
                    mutable _index_id_29 = 0;
                    while _index_id_29 < _len_id_24 {
                        let (i, d) = _array_id_20[_index_id_29];
                        let x = "Hello World";
                        _index_id_29 += 1;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_slice() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : Int[]) : Unit {
                for i in arr[6..-2..2] {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main(arr : Int[]) : Unit {
                {
                    let _array_id_23 = arr[6..-2..2];
                    let _len_id_27 = Length(_array_id_23);
                    mutable _index_id_32 = 0;
                    while _index_id_32 < _len_id_27 {
                        let i = _array_id_23[_index_id_32];
                        let x = "Hello World";
                        _index_id_32 += 1;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_range() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                for i in 0..4 {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                {
                    let _range_id_18 = 0..4;
                    mutable _index_id_21 = _range_id_18::Start;
                    let _step_id_26 = _range_id_18::Step;
                    let _end_id_31 = _range_id_18::End;
                    while _step_id_26 > 0 and _index_id_21 <= _end_id_31 or _step_id_26 < 0 and _index_id_21 >= _end_id_31 {
                        let i = _index_id_21;
                        let x = "Hello World";
                        _index_id_21 += _step_id_26;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_reverse_range() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                for i in 4..-1..0 {
                    let x = "Hello World";
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                {
                    let _range_id_20 = 4..-1..0;
                    mutable _index_id_23 = _range_id_20::Start;
                    let _step_id_28 = _range_id_20::Step;
                    let _end_id_33 = _range_id_20::End;
                    while _step_id_28 > 0 and _index_id_23 <= _end_id_33 or _step_id_28 < 0 and _index_id_23 >= _end_id_33 {
                        let i = _index_id_23;
                        let x = "Hello World";
                        _index_id_23 += _step_id_28;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_repeat() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                repeat {
                    let x = "Hello World";
                } until true;
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                {
                    mutable _continue_cond_14 = true;
                    while _continue_cond_14 {
                        let x = "Hello World";
                        _continue_cond_14 = not true;
                    }
                };
            }
        "#]],
    );
}

#[test]
fn convert_repeat_fixup() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                repeat {
                    let x = "Hello World";
                } until true
                fixup {
                    let y = "Fixup";
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                {
                    mutable _continue_cond_19 = true;
                    while _continue_cond_19 {
                        let x = "Hello World";
                        _continue_cond_19 = not true;
                        if _continue_cond_19 {
                            let y = "Fixup";
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_repeat_nested() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                let a = true;
                let b = false;
                let c = true;
                repeat {
                    repeat {
                        let x = "First";
                    } until a
                    fixup {
                        let y = "Second";
                    }
                } until b
                fixup {
                    repeat {
                        let z = "Third";
                    } until c;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                let a = true;
                let b = false;
                let c = true;
                {
                    mutable _continue_cond_74 = true;
                    while _continue_cond_74 {
                        {
                            mutable _continue_cond_44 = true;
                            while _continue_cond_44 {
                                let x = "First";
                                _continue_cond_44 = not a;
                                if _continue_cond_44 {
                                    let y = "Second";
                                }
                            }
                        }
                        _continue_cond_74 = not b;
                        if _continue_cond_74 {
                            {
                                mutable _continue_cond_61 = true;
                                while _continue_cond_61 {
                                    let z = "Third";
                                    _continue_cond_61 = not c;
                                }
                            };
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_treats_for_loop_with_short_circuit_expression_explicit_int() {
    check(
        indoc! {r#"
        function Main() : Unit {
            let x = for i : Int in fail "" {};
        }
        "#},
        &expect![[r#"
            function Main() : Unit {
                let x = {
                    let _array_id_15 = fail "";
                    let _len_id_19 = Length(_array_id_15);
                    mutable _index_id_24 = 0;
                    while _index_id_24 < _len_id_19 {
                        let i = _array_id_15[_index_id_24];
                        _index_id_24 += 1;
                    }
                };
            }
        "#]],
    );
}

#[test]
fn convert_treats_for_loop_with_short_circuit_expression_explicit_non_int() {
    check(
        indoc! {r#"
        function Main() : Unit {
            let x = for i : String in fail "" {};
        }
        "#},
        &expect![[r#"
            function Main() : Unit {
                let x = {
                    let _array_id_15 = fail "";
                    let _len_id_19 = Length(_array_id_15);
                    mutable _index_id_24 = 0;
                    while _index_id_24 < _len_id_19 {
                        let i = _array_id_15[_index_id_24];
                        _index_id_24 += 1;
                    }
                };
            }
        "#]],
    );
}

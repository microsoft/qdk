// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

/// Tests for the `break`/`continue` loop-unification desugar: flag minting and
/// guarding across every loop form, operand-position normalization, and the
/// residual-node invariant.
use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, span::Span,
    target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::{
    hir::{ExprKind, Lit, Stmt, StmtKind},
    visit::{self, Visitor},
};

use crate::loop_unification::{
    Error, check_no_break_continue,
    test_utils::{check, check_normalized, desugar},
};

#[test]
fn convert_for_range_with_break() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable total = 0;
                for i in 0..4 {
                    if i == 2 {
                        break;
                    }
                    total += i;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable total = 0;
                {
                    let _range_id_45 = 0..4;
                    mutable _index_id_48 = _range_id_45::Start;
                    let _step_id_53 = _range_id_45::Step;
                    let _end_id_58 = _range_id_45::End;
                    mutable _broke_31 = false;
                    while not _broke_31 and _step_id_53 > 0 and _index_id_48 <= _end_id_58 or _step_id_53 < 0 and _index_id_48 >= _end_id_58 {
                        let i = _index_id_48;
                        if i == 2 {
                            _broke_31 = true;
                        }
                        if not _broke_31 {
                            total += i;
                        }
                        if not _broke_31 {
                            _index_id_48 += _step_id_53;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_range_with_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable total = 0;
                for i in 0..4 {
                    if i == 2 {
                        continue;
                    }
                    total += i;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable total = 0;
                {
                    let _range_id_45 = 0..4;
                    mutable _index_id_48 = _range_id_45::Start;
                    let _step_id_53 = _range_id_45::Step;
                    let _end_id_58 = _range_id_45::End;
                    while _step_id_53 > 0 and _index_id_48 <= _end_id_58 or _step_id_53 < 0 and _index_id_48 >= _end_id_58 {
                        mutable _cont_31 = false;
                        let i = _index_id_48;
                        if i == 2 {
                            _cont_31 = true;
                        }
                        if not _cont_31 {
                            total += i;
                        }
                        _index_id_48 += _step_id_53;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_range_with_break_and_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable total = 0;
                for i in 0..4 {
                    if i == 3 {
                        break;
                    }
                    if i == 1 {
                        continue;
                    }
                    total += i;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable total = 0;
                {
                    let _range_id_74 = 0..4;
                    mutable _index_id_77 = _range_id_74::Start;
                    let _step_id_82 = _range_id_74::Step;
                    let _end_id_87 = _range_id_74::End;
                    mutable _broke_40 = false;
                    while not _broke_40 and _step_id_82 > 0 and _index_id_77 <= _end_id_87 or _step_id_82 < 0 and _index_id_77 >= _end_id_87 {
                        mutable _cont_44 = false;
                        let i = _index_id_77;
                        if i == 3 {
                            _broke_40 = true;
                        }
                        if not _broke_40 and not _cont_44 {
                            if i == 1 {
                                _cont_44 = true;
                            }
                        }
                        if not _broke_40 and not _cont_44 {
                            total += i;
                        }
                        if not _broke_40 {
                            _index_id_77 += _step_id_82;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_array_with_break() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : Int[]) : Unit {
                mutable total = 0;
                for x in arr {
                    if x == 3 {
                        break;
                    }
                    total += x;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main(arr : Int[]) : Unit {
                mutable total = 0;
                {
                    let _array_id_44 = arr;
                    let _len_id_48 = Length(_array_id_44);
                    mutable _index_id_53 = 0;
                    mutable _broke_30 = false;
                    while not _broke_30 and _index_id_53 < _len_id_48 {
                        let x = _array_id_44[_index_id_53];
                        if x == 3 {
                            _broke_30 = true;
                        }
                        if not _broke_30 {
                            total += x;
                        }
                        if not _broke_30 {
                            _index_id_53 += 1;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_array_with_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : Int[]) : Unit {
                mutable total = 0;
                for x in arr {
                    if x == 1 {
                        continue;
                    }
                    total += x;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main(arr : Int[]) : Unit {
                mutable total = 0;
                {
                    let _array_id_44 = arr;
                    let _len_id_48 = Length(_array_id_44);
                    mutable _index_id_53 = 0;
                    while _index_id_53 < _len_id_48 {
                        mutable _cont_30 = false;
                        let x = _array_id_44[_index_id_53];
                        if x == 1 {
                            _cont_30 = true;
                        }
                        if not _cont_30 {
                            total += x;
                        }
                        _index_id_53 += 1;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_for_array_with_break_and_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main(arr : Int[]) : Unit {
                mutable total = 0;
                for x in arr {
                    if x == 3 {
                        break;
                    }
                    if x == 1 {
                        continue;
                    }
                    total += x;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main(arr : Int[]) : Unit {
                mutable total = 0;
                {
                    let _array_id_73 = arr;
                    let _len_id_77 = Length(_array_id_73);
                    mutable _index_id_82 = 0;
                    mutable _broke_39 = false;
                    while not _broke_39 and _index_id_82 < _len_id_77 {
                        mutable _cont_43 = false;
                        let x = _array_id_73[_index_id_82];
                        if x == 3 {
                            _broke_39 = true;
                        }
                        if not _broke_39 and not _cont_43 {
                            if x == 1 {
                                _cont_43 = true;
                            }
                        }
                        if not _broke_39 and not _cont_43 {
                            total += x;
                        }
                        if not _broke_39 {
                            _index_id_82 += 1;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_while_with_break() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                while i < 10 {
                    i += 1;
                    if i == 5 {
                        break;
                    }
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                {
                    mutable _broke_29 = false;
                    while not _broke_29 and i < 10 {
                        i += 1;
                        if i == 5 {
                            _broke_29 = true;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_while_with_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                while i < 10 {
                    i += 1;
                    if i == 3 {
                        continue;
                    }
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                while i < 10 {
                    mutable _cont_29 = false;
                    i += 1;
                    if i == 3 {
                        _cont_29 = true;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_while_with_break_and_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                while i < 10 {
                    i += 1;
                    if i == 5 {
                        break;
                    }
                    if i == 3 {
                        continue;
                    }
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                {
                    mutable _broke_38 = false;
                    while not _broke_38 and i < 10 {
                        mutable _cont_42 = false;
                        i += 1;
                        if i == 5 {
                            _broke_38 = true;
                        }
                        if not _broke_38 and not _cont_42 {
                            if i == 3 {
                                _cont_42 = true;
                            }
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_while_without_break_continue_unchanged() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                while i < 10 {
                    i += 1;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                while i < 10 {
                    i += 1;
                }
            }
        "#]],
    );
}

#[test]
fn convert_repeat_until_with_break() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                repeat {
                    i += 1;
                    if i == 5 {
                        break;
                    }
                } until i >= 10;
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                {
                    mutable _continue_cond_29 = true;
                    mutable _broke_33 = false;
                    while not _broke_33 and _continue_cond_29 {
                        i += 1;
                        if i == 5 {
                            _broke_33 = true;
                        }
                        if not _broke_33 {
                            _continue_cond_29 = not i >= 10;
                        }
                    }
                };
            }
        "#]],
    );
}

#[test]
fn convert_repeat_until_with_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                repeat {
                    i += 1;
                    if i == 3 {
                        continue;
                    }
                } until i >= 10;
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                {
                    mutable _continue_cond_29 = true;
                    while _continue_cond_29 {
                        mutable _cont_33 = false;
                        i += 1;
                        if i == 3 {
                            _cont_33 = true;
                        }
                        _continue_cond_29 = not i >= 10;
                    }
                };
            }
        "#]],
    );
}

#[test]
fn convert_repeat_until_with_break_and_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                repeat {
                    i += 1;
                    if i == 5 {
                        break;
                    }
                    if i == 3 {
                        continue;
                    }
                } until i >= 10;
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                {
                    mutable _continue_cond_38 = true;
                    mutable _broke_42 = false;
                    while not _broke_42 and _continue_cond_38 {
                        mutable _cont_46 = false;
                        i += 1;
                        if i == 5 {
                            _broke_42 = true;
                        }
                        if not _broke_42 and not _cont_46 {
                            if i == 3 {
                                _cont_46 = true;
                            }
                        }
                        if not _broke_42 {
                            _continue_cond_38 = not i >= 10;
                        }
                    }
                };
            }
        "#]],
    );
}

#[test]
fn convert_repeat_until_fixup_with_break() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                repeat {
                    i += 1;
                    if i == 5 {
                        break;
                    }
                } until i >= 10
                fixup {
                    i += 1;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                {
                    mutable _continue_cond_34 = true;
                    mutable _broke_38 = false;
                    while not _broke_38 and _continue_cond_34 {
                        i += 1;
                        if i == 5 {
                            _broke_38 = true;
                        }
                        if not _broke_38 {
                            _continue_cond_34 = not i >= 10;
                            if _continue_cond_34 {
                                i += 1;
                            }
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_repeat_until_fixup_with_break_and_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                repeat {
                    i += 1;
                    if i == 5 {
                        break;
                    }
                    if i == 3 {
                        continue;
                    }
                } until i >= 10
                fixup {
                    i += 1;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                {
                    mutable _continue_cond_43 = true;
                    mutable _broke_47 = false;
                    while not _broke_47 and _continue_cond_43 {
                        mutable _cont_51 = false;
                        i += 1;
                        if i == 5 {
                            _broke_47 = true;
                        }
                        if not _broke_47 and not _cont_51 {
                            if i == 3 {
                                _cont_51 = true;
                            }
                        }
                        if not _broke_47 {
                            _continue_cond_43 = not i >= 10;
                            if _continue_cond_43 {
                                i += 1;
                            }
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_repeat_until_fixup_with_continue() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable i = 0;
                repeat {
                    i += 1;
                    if i == 3 {
                        continue;
                    }
                } until i >= 10
                fixup {
                    i += 1;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable i = 0;
                {
                    mutable _continue_cond_34 = true;
                    while _continue_cond_34 {
                        mutable _cont_38 = false;
                        i += 1;
                        if i == 3 {
                            _cont_38 = true;
                        }
                        _continue_cond_34 = not i >= 10;
                        if _continue_cond_34 {
                            i += 1;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_nested_for_in_while_with_break_in_each() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable total = 0;
                while total < 100 {
                    for i in 0..4 {
                        if i == 2 {
                            break;
                        }
                        total += i;
                    }
                    if total > 50 {
                        break;
                    }
                    total += 1;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable total = 0;
                {
                    mutable _broke_116 = false;
                    while not _broke_116 and total < 100 {
                        {
                            let _range_id_64 = 0..4;
                            mutable _index_id_67 = _range_id_64::Start;
                            let _step_id_72 = _range_id_64::Step;
                            let _end_id_77 = _range_id_64::End;
                            mutable _broke_50 = false;
                            while not _broke_50 and _step_id_72 > 0 and _index_id_67 <= _end_id_77 or _step_id_72 < 0 and _index_id_67 >= _end_id_77 {
                                let i = _index_id_67;
                                if i == 2 {
                                    _broke_50 = true;
                                }
                                if not _broke_50 {
                                    total += i;
                                }
                                if not _broke_50 {
                                    _index_id_67 += _step_id_72;
                                }
                            }
                        }
                        if total > 50 {
                            _broke_116 = true;
                        }
                        if not _broke_116 {
                            total += 1;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_loop_with_return_and_break() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Int {
                mutable total = 0;
                for i in 0..4 {
                    if i == 2 {
                        return total;
                    }
                    if i == 3 {
                        break;
                    }
                    total += i;
                }
                total
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Int {
                mutable total = 0;
                {
                    let _range_id_57 = 0..4;
                    mutable _index_id_60 = _range_id_57::Start;
                    let _step_id_65 = _range_id_57::Step;
                    let _end_id_70 = _range_id_57::End;
                    mutable _broke_43 = false;
                    while not _broke_43 and _step_id_65 > 0 and _index_id_60 <= _end_id_70 or _step_id_65 < 0 and _index_id_60 >= _end_id_70 {
                        let i = _index_id_60;
                        if i == 2 {
                            return total;
                        }
                        if i == 3 {
                            _broke_43 = true;
                        }
                        if not _broke_43 {
                            total += i;
                        }
                        if not _broke_43 {
                            _index_id_60 += _step_id_65;
                        }
                    }
                }
                total
            }
        "#]],
    );
}

#[test]
fn convert_for_range_with_break_in_value_block_defaultable() {
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable total = 0;
                for i in 0..4 {
                    let x = if i == 2 { break } else { i };
                    total += x;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable total = 0;
                {
                    let _range_id_53 = 0..4;
                    mutable _index_id_56 = _range_id_53::Start;
                    let _step_id_61 = _range_id_53::Step;
                    let _end_id_66 = _range_id_53::End;
                    mutable _broke_37 = false;
                    while not _broke_37 and _step_id_61 > 0 and _index_id_56 <= _end_id_66 or _step_id_61 < 0 and _index_id_56 >= _end_id_66 {
                        let i = _index_id_56;
                        let x = if i == 2 {
                            _broke_37 = true;
                            0
                        } else {
                            i
                        };
                        if not _broke_37 {
                            total += x;
                        }
                        if not _broke_37 {
                            _index_id_56 += _step_id_61;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn break_in_value_block_of_non_defaultable_type_normalized_and_relocated() {
    // A `let` binding whose value is `if i == 2 { break } else { q }` binds a
    // non-defaultable `Qubit`. The normalize pass array-backs the initializer
    // as a `Qubit[]` temp whose `then` branch is a bare break and whose `else`
    // branch is the singleton `[q]`, and the desugar relocates the guarded
    // `.operand_tmp_<id>[0]` read into the fall-through branch, so the binding needs
    // no `Qubit` default.
    check_normalized(
        indoc! {r#"
        namespace test {
            operation Op(q : Qubit) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                for i in 0..4 {
                    let x = if i == 2 { break } else { q };
                    Op(x);
                }
            }
        }
        "#},
        &expect![[r#"
            operation Op(q : Qubit) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                {
                    let _range_id_66 = 0..4;
                    mutable _index_id_69 = _range_id_66::Start;
                    let _step_id_74 = _range_id_66::Step;
                    let _end_id_79 = _range_id_66::End;
                    mutable _broke_50 = false;
                    while not _broke_50 and _step_id_74 > 0 and _index_id_69 <= _end_id_79 or _step_id_74 < 0 and _index_id_69 >= _end_id_79 {
                        let i = _index_id_69;
                        let _operand_tmp_43 = if i == 2 {
                            _broke_50 = true;
                            []
                        } else {
                            [q]
                        };
                        if not _broke_50 {
                            let x = _operand_tmp_43[0];
                            Op(x);
                        }
                        if not _broke_50 {
                            _index_id_69 += _step_id_74;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_operand_position_break_normalized_then_desugared() {
    // The normalize pass hoists the operand-position `break` value block to a
    // statement-position `let`, which the desugar then rewrites in place.
    check_normalized(
        indoc! {r#"
        namespace test {
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                for i in 0..4 {
                    Foo(if i == 2 { break } else { i });
                }
            }
        }
        "#},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                {
                    let _range_id_59 = 0..4;
                    mutable _index_id_62 = _range_id_59::Start;
                    let _step_id_67 = _range_id_59::Step;
                    let _end_id_72 = _range_id_59::End;
                    mutable _broke_43 = false;
                    while not _broke_43 and _step_id_67 > 0 and _index_id_62 <= _end_id_72 or _step_id_67 < 0 and _index_id_62 >= _end_id_72 {
                        let i = _index_id_62;
                        let _operand_tmp_35 = Foo;
                        let _operand_tmp_39 = if i == 2 {
                            _broke_43 = true;
                            0
                        } else {
                            i
                        };
                        if not _broke_43 {
                            _operand_tmp_35(_operand_tmp_39);
                        }
                        if not _broke_43 {
                            _index_id_62 += _step_id_67;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_array_backed_operand_break_normalized_then_desugared() {
    // A `Qubit`-typed operand value-block has no classical default. The
    // normalize pass array-backs it, using a `Qubit[]` temp, a trailing value
    // wrapped as `[q]`, and an operand read of `.operand_tmp_<id>[0]`; then the
    // desugar seeds the break path with the universal `[]` default of `Qubit[]`
    // and guards the read, so the empty array is never indexed. No
    // `UnsupportedBreakContinueType` error
    // fires for the operand position, and the package still validates.
    check_normalized(
        indoc! {r#"
        namespace test {
            operation Foo(q : Qubit) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                for i in 0..4 {
                    Foo(if i == 2 { break } else { q });
                }
            }
        }
        "#},
        &expect![[r#"
            operation Foo(q : Qubit) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                {
                    let _range_id_66 = 0..4;
                    mutable _index_id_69 = _range_id_66::Start;
                    let _step_id_74 = _range_id_66::Step;
                    let _end_id_79 = _range_id_66::End;
                    mutable _broke_50 = false;
                    while not _broke_50 and _step_id_74 > 0 and _index_id_69 <= _end_id_79 or _step_id_74 < 0 and _index_id_69 >= _end_id_79 {
                        let i = _index_id_69;
                        let _operand_tmp_39 = Foo;
                        let _operand_tmp_43 = if i == 2 {
                            _broke_50 = true;
                            []
                        } else {
                            [q]
                        };
                        if not _broke_50 {
                            _operand_tmp_39(_operand_tmp_43[0]);
                        }
                        if not _broke_50 {
                            _index_id_69 += _step_id_74;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn break_in_value_block_of_range_type_uses_shaped_default() {
    // A `Range`-typed value block with a buried `break` is defaultable, so the
    // desugar seeds the break path in place rather than array-backing it. The
    // synthesized default must be the fully-bounded `0..0`, matching the `Range`
    // type tag, not the `RangeFull` shape `...`.
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Int {
                mutable acc = 0;
                for i in 0..4 {
                    let r = if i == 2 { break } else { 0..i };
                    acc += r::End;
                }
                acc
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Int {
                mutable acc = 0;
                {
                    let _range_id_60 = 0..4;
                    mutable _index_id_63 = _range_id_60::Start;
                    let _step_id_68 = _range_id_60::Step;
                    let _end_id_73 = _range_id_60::End;
                    mutable _broke_42 = false;
                    while not _broke_42 and _step_id_68 > 0 and _index_id_63 <= _end_id_73 or _step_id_68 < 0 and _index_id_63 >= _end_id_73 {
                        let i = _index_id_63;
                        let r = if i == 2 {
                            _broke_42 = true;
                            0..0
                        } else {
                            0..i
                        };
                        if not _broke_42 {
                            acc += r::End;
                        }
                        if not _broke_42 {
                            _index_id_63 += _step_id_68;
                        }
                    }
                }
                acc
            }
        "#]],
    );
}

#[test]
fn convert_descending_for_range_with_break_and_continue() {
    // A negative-step range exercises the second disjunct of the range
    // condition (`step < 0 and index >= end`); the `break` flag guards the whole
    // disjunction and the decrement step, while `continue` skips the rest of the
    // body but still runs the decrement.
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable total = 0;
                for i in 5..-1..0 {
                    if i == 1 {
                        break;
                    }
                    if i == 3 {
                        continue;
                    }
                    total += i;
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                mutable total = 0;
                {
                    let _range_id_76 = 5..-1..0;
                    mutable _index_id_79 = _range_id_76::Start;
                    let _step_id_84 = _range_id_76::Step;
                    let _end_id_89 = _range_id_76::End;
                    mutable _broke_42 = false;
                    while not _broke_42 and _step_id_84 > 0 and _index_id_79 <= _end_id_89 or _step_id_84 < 0 and _index_id_79 >= _end_id_89 {
                        mutable _cont_46 = false;
                        let i = _index_id_79;
                        if i == 1 {
                            _broke_42 = true;
                        }
                        if not _broke_42 and not _cont_46 {
                            if i == 3 {
                                _cont_46 = true;
                            }
                        }
                        if not _broke_42 and not _cont_46 {
                            total += i;
                        }
                        if not _broke_42 {
                            _index_id_79 += _step_id_84;
                        }
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_operand_position_continue_normalized_then_desugared() {
    // The normalize pass hoists an operand-position `continue` value block to a
    // statement-position `let`, which the desugar then rewrites in place. The
    // per-iteration `.cont_<id>` flag skips the eager consumer but still lets the
    // loop step run.
    check_normalized(
        indoc! {r#"
        namespace test {
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                for i in 0..4 {
                    Foo(if i == 2 { continue } else { i });
                }
            }
        }
        "#},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                {
                    let _range_id_59 = 0..4;
                    mutable _index_id_62 = _range_id_59::Start;
                    let _step_id_67 = _range_id_59::Step;
                    let _end_id_72 = _range_id_59::End;
                    while _step_id_67 > 0 and _index_id_62 <= _end_id_72 or _step_id_67 < 0 and _index_id_62 >= _end_id_72 {
                        mutable _cont_43 = false;
                        let i = _index_id_62;
                        let _operand_tmp_35 = Foo;
                        let _operand_tmp_39 = if i == 2 {
                            _cont_43 = true;
                            0
                        } else {
                            i
                        };
                        if not _cont_43 {
                            _operand_tmp_35(_operand_tmp_39);
                        }
                        _index_id_62 += _step_id_67;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn convert_sequential_loops_with_separate_break_flags() {
    // Two adjacent loops each contain a `break`; each must mint its own
    // `.broke_<id>` flag so exiting the first loop does not leak into the second.
    check(
        indoc! {r#"
        namespace test {
            operation Main() : Unit {
                for i in 0..4 {
                    if i == 2 {
                        break;
                    }
                }
                for j in 0..4 {
                    if j == 3 {
                        break;
                    }
                }
            }
        }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                {
                    let _range_id_48 = 0..4;
                    mutable _index_id_51 = _range_id_48::Start;
                    let _step_id_56 = _range_id_48::Step;
                    let _end_id_61 = _range_id_48::End;
                    mutable _broke_40 = false;
                    while not _broke_40 and _step_id_56 > 0 and _index_id_51 <= _end_id_61 or _step_id_56 < 0 and _index_id_51 >= _end_id_61 {
                        let i = _index_id_51;
                        if i == 2 {
                            _broke_40 = true;
                        }
                        if not _broke_40 {
                            _index_id_51 += _step_id_56;
                        }
                    }
                }
                {
                    let _range_id_108 = 0..4;
                    mutable _index_id_111 = _range_id_108::Start;
                    let _step_id_116 = _range_id_108::Step;
                    let _end_id_121 = _range_id_108::End;
                    mutable _broke_100 = false;
                    while not _broke_100 and _step_id_116 > 0 and _index_id_111 <= _end_id_121 or _step_id_116 < 0 and _index_id_111 >= _end_id_121 {
                        let j = _index_id_111;
                        if j == 3 {
                            _broke_100 = true;
                        }
                        if not _broke_100 {
                            _index_id_111 += _step_id_116;
                        }
                    }
                }
            }
        "#]],
    );
}

/// Collects the spans of the synthetic flag-set statements (`set .flag_<id> = true`)
/// and the guard `if` statements produced by the desugar, so their span
/// discipline can be asserted directly.
#[derive(Default)]
struct SpanCollector {
    flag_set_spans: Vec<Span>,
    guard_if_spans: Vec<Span>,
}

impl<'a> Visitor<'a> for SpanCollector {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Semi(e) => {
                if let ExprKind::Assign(_, rhs) = &e.kind
                    && matches!(&rhs.kind, ExprKind::Lit(Lit::Bool(true)))
                {
                    self.flag_set_spans.push(stmt.span);
                }
            }
            StmtKind::Expr(e) if matches!(&e.kind, ExprKind::If(..)) => {
                self.guard_if_spans.push(stmt.span);
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }
}

#[test]
fn break_flag_set_is_steppable_and_guards_are_non_steppable() {
    let file = indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable x = 0;
                for i in 0..4 {
                    break;
                    x = 1;
                }
            }
        }
        "#};
    let (unit, store, errors) = desugar(file);
    assert!(errors.is_empty(), "unexpected desugar errors: {errors:?}");

    let mut collector = SpanCollector::default();
    collector.visit_package(&unit.package);

    // The flag-set replacing `break;` is steppable and carries the exact
    // `break` keyword span.
    let break_offset =
        u32::try_from(file.find("break").expect("source has a break")).expect("offset fits in u32");
    let break_span = Span {
        lo: break_offset,
        hi: break_offset + 5,
    };
    assert_eq!(
        collector.flag_set_spans,
        vec![break_span],
        "the break flag-set should be steppable at the keyword span"
    );

    // Every guard `if`, for both the guarded user statement and the loop step,
    // is non-steppable, i.e. carries the default span.
    assert!(
        !collector.guard_if_spans.is_empty(),
        "expected guard `if` statements"
    );
    for span in &collector.guard_if_spans {
        assert_eq!(
            *span,
            Span::default(),
            "guard `if` statements must be non-steppable"
        );
    }

    expect![[r#"
        operation Main() : Unit {
            mutable x = 0;
            {
                let _range_id_38 = 0..4;
                mutable _index_id_41 = _range_id_38::Start;
                let _step_id_46 = _range_id_38::Step;
                let _end_id_51 = _range_id_38::End;
                mutable _broke_24 = false;
                while not _broke_24 and _step_id_46 > 0 and _index_id_41 <= _end_id_51 or _step_id_46 < 0 and _index_id_41 >= _end_id_51 {
                    let i = _index_id_41;
                    _broke_24 = true;
                    if not _broke_24 {
                        x = 1;
                    }
                    if not _broke_24 {
                        _index_id_41 += _step_id_46;
                    }
                }
            }
        }
    "#]]
        .assert_eq(&crate::qsharp_gen::write_package_qsharp(&store, &unit.package));
}

#[test]
fn break_guards_following_qubit_allocation_suffix() {
    let (unit, store, errors) = desugar(indoc! {r#"
        namespace test {
            operation Foo(q : Qubit) : Unit {}
            operation Main() : Unit {
                for i in 0..1 {
                    break;
                    use q = Qubit();
                    Foo(q);
                }
            }
        }
        "#});
    assert!(errors.is_empty(), "unexpected desugar errors: {errors:?}");
    let qsharp = crate::qsharp_gen::write_package_qsharp(&store, &unit.package);
    let qubit_pos = qsharp
        .find("use q = Qubit();")
        .expect("rendered package should contain the qubit allocation");
    let guard_pos = qsharp[..qubit_pos]
        .rfind("if not _broke")
        .unwrap_or_else(|| panic!("qubit allocation should be guarded after break\n{qsharp}"));
    let flag_pos = qsharp
        .find("= true;")
        .expect("rendered package should contain the break flag assignment");

    assert!(
        flag_pos < guard_pos,
        "guard should appear after the break flag assignment and before allocation\n{qsharp}"
    );

    expect![[r#"
        operation Foo(q : Qubit) : Unit {}
        operation Main() : Unit {
            {
                let _range_id_44 = 0..1;
                mutable _index_id_47 = _range_id_44::Start;
                let _step_id_52 = _range_id_44::Step;
                let _end_id_57 = _range_id_44::End;
                mutable _broke_30 = false;
                while not _broke_30 and _step_id_52 > 0 and _index_id_47 <= _end_id_57 or _step_id_52 < 0 and _index_id_47 >= _end_id_57 {
                    let i = _index_id_47;
                    _broke_30 = true;
                    if not _broke_30 {
                        use q = Qubit();
                        Foo(q);
                    }
                    if not _broke_30 {
                        _index_id_47 += _step_id_52;
                    }
                }
            }
        }
    "#]].assert_eq(&qsharp);
}

#[test]
fn operand_breaks_in_qubit_allocation_and_controlled_call_are_guarded() {
    check_normalized(
        indoc! {r#"
            namespace test {
                operation Foo(q : Qubit) : Unit is Ctl {}
                operation Main() : Unit {
                    repeat {
                        use rr = Qubit[break];
                        Controlled Foo(break);
                    } until true;
                }
            }
        "#},
        &expect![[r#"
            operation Foo(q : Qubit) : Unit is Ctl {}
            operation Main() : Unit {
                {
                    mutable _continue_cond_46 = true;
                    mutable _broke_50 = false;
                    while not _broke_50 and _continue_cond_46 {
                        let _operand_tmp_26 = {
                            _broke_50 = true;
                            0
                        };
                        if not _broke_50 {
                            _continue_cond_46 = not true;
                        }
                    }
                };
            }
        "#]],
    );
}

#[test]
fn check_no_break_continue_reports_residual_node() {
    // A `break` outside any loop is never converted by the desugar, whose
    // catch-all leaves it as a raw node, so the residual-node check must flag
    // it. This exercises the invariant guard directly on a desugared package
    // that still contains a raw `break`.
    let (unit, store, errors) = desugar(indoc! {r#"
        namespace test {
            operation Main() : Unit {
                break;
            }
        }
        "#});
    assert!(errors.is_empty(), "unexpected desugar errors: {errors:?}");

    let residual = check_no_break_continue(&unit.package);
    assert!(
        !residual.is_empty(),
        "expected a residual break/continue diagnostic, got none"
    );
    assert!(
        residual
            .iter()
            .all(|e| matches!(e, Error::ResidualBreakContinue(_))),
        "expected only ResidualBreakContinue errors, got {residual:?}"
    );

    expect![[r#"
        operation Main() : Unit {
            break;
        }
    "#]]
    .assert_eq(&crate::qsharp_gen::write_package_qsharp(
        &store,
        &unit.package,
    ));
}

#[test]
fn check_no_break_continue_clean_after_desugar() {
    // A valid loop whose body contains both `break` and `continue` is fully
    // desugared to loop-flag writes, so no raw node should remain and the
    // invariant check must report nothing, with no false positives.
    let (unit, store, errors) = desugar(indoc! {r#"
        namespace test {
            operation Main() : Unit {
                mutable total = 0;
                for i in 0..10 {
                    if i == 3 {
                        continue;
                    }
                    if i == 7 {
                        break;
                    }
                    total += i;
                }
            }
        }
        "#});
    assert!(errors.is_empty(), "unexpected desugar errors: {errors:?}");

    let residual = check_no_break_continue(&unit.package);
    assert!(
        residual.is_empty(),
        "expected no residual break/continue after desugar, got {residual:?}"
    );

    expect![[r#"
        operation Main() : Unit {
            mutable total = 0;
            {
                let _range_id_74 = 0..10;
                mutable _index_id_77 = _range_id_74::Start;
                let _step_id_82 = _range_id_74::Step;
                let _end_id_87 = _range_id_74::End;
                mutable _broke_40 = false;
                while not _broke_40 and _step_id_82 > 0 and _index_id_77 <= _end_id_87 or _step_id_82 < 0 and _index_id_77 >= _end_id_87 {
                    mutable _cont_44 = false;
                    let i = _index_id_77;
                    if i == 3 {
                        _cont_44 = true;
                    }
                    if not _broke_40 and not _cont_44 {
                        if i == 7 {
                            _broke_40 = true;
                        }
                    }
                    if not _broke_40 and not _cont_44 {
                        total += i;
                    }
                    if not _broke_40 {
                        _index_id_77 += _step_id_82;
                    }
                }
            }
        }
    "#]]
        .assert_eq(&crate::qsharp_gen::write_package_qsharp(&store, &unit.package));
}

#[test]
fn default_passes_gate_residual_error_when_loop_control_reports() {
    use crate::{PackageType, PassContext};

    // A `break` outside any loop is a misplaced-control-flow user error that
    // `loop_control` reports. The residual raw node it leaves behind is the
    // expected consequence of that error, so the default pipeline must surface
    // only the `loop_control` diagnostic and gate out the internal
    // `ResidualBreakContinue` invariant to avoid double-reporting.
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new(
        [(
            "test".into(),
            "namespace test { operation Main() : Unit { break; } }".into(),
        )],
        None,
    );
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = PassContext::new().run_default_passes(
        &mut unit.package,
        &mut unit.assigner,
        store.core(),
        PackageType::Lib,
    );

    assert!(
        errors.iter().any(|e| matches!(
            e,
            crate::Error::LoopControl(crate::loop_control::Error::OutsideLoop(_))
        )),
        "expected a loop_control OutsideLoop diagnostic, got {errors:?}"
    );
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            crate::Error::LoopUnification(Error::ResidualBreakContinue(_))
        )),
        "gating should suppress ResidualBreakContinue when loop_control reports, got {errors:?}"
    );

    expect![[r#"
        operation Main() : Unit {
            break;
        }
        // entry
        Main()
    "#]]
    .assert_eq(&crate::qsharp_gen::write_package_qsharp(
        &store,
        &unit.package,
    ));
}

fn default_pass_errors(source: &str) -> Vec<crate::Error> {
    use crate::{PackageType, PassContext};

    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), source.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    PassContext::new().run_default_passes(
        &mut unit.package,
        &mut unit.assigner,
        store.core(),
        PackageType::Lib,
    )
}

#[test]
fn nested_loop_header_control_reaches_placement_validation() {
    let errors = default_pass_errors(indoc! {r#"
        namespace test {
            function Main() : Int {
                repeat {
                    while break {}
                    fail "body"
                } until true
            }
        }
    "#});

    assert!(
        errors.iter().any(|error| matches!(
            error,
            crate::Error::LoopControl(crate::loop_control::Error::InLoopHeader(_))
        )),
        "expected a loop_control InLoopHeader diagnostic, got {errors:?}"
    );
}

#[test]
fn nested_repeat_fixup_control_reaches_placement_validation() {
    let errors = default_pass_errors(indoc! {r#"
        namespace test {
            function Main() : Int {
                repeat {
                    repeat {} until false fixup { break }
                    fail "body"
                } until true
            }
        }
    "#});

    assert!(
        errors.iter().any(|error| matches!(
            error,
            crate::Error::LoopControl(crate::loop_control::Error::InFixup(_))
        )),
        "expected a loop_control InFixup diagnostic, got {errors:?}"
    );
}

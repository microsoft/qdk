// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Three-level nesting tests: pure if/else/while/for combinations.

use super::*;

// The following tests nest block-bearing constructs three levels deep with
// `return`s placed at a variety of positions. They exercise the interaction
// between the hoist pre-pass and flag lowering when rewrites must reach
// into deeply nested `Block`/`If`/`While`/`for` bodies. The outer callable
// uses `@EntryPoint() operation Main() : Int` so that any dynamic branch
// (driven by `M(q)`) is legal during flag lowering.

#[test]
fn if_if_if_return_in_deepest_then() {
    // if / if / if -> return at the innermost then
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                if M(q) == One {
                    if M(q) == Zero {
                        if M(q) == One {
                            return 1;
                        }
                    }
                }
                2
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                if (M(q) == One) {
                    if (M(q) == Zero) {
                        if (M(q) == One) {
                            {
                                let _generated_ident_50 : Int = 1;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val = _generated_ident_50;
                                    __has_returned = true;
                                };
                            };
                        }

                    }

                }

                let _generated_ident_62 : Int = if (not __has_returned) {
                    2
                } else {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_62
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn if_else_chain_return_in_deepest_else() {
    // if { ... } else { if { ... } else { if c { x } else { return v } } }
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                if M(q) == One {
                    1
                } else {
                    if M(q) == Zero {
                        2
                    } else {
                        if M(q) == One {
                            3
                        } else {
                            return 4;
                        }
                    }
                }
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_72 : Int = if (M(q) == One) {
                    1
                } else {
                    if (M(q) == Zero) {
                        2
                    } else {
                        if (M(q) == One) {
                            3
                        } else {
                            {
                                let _generated_ident_60 : Int = 4;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val = _generated_ident_60;
                                    __has_returned = true;
                                };
                            };
                        }

                    }

                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_72
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn while_while_while_return_deep() {
    // while / while / while -> return deep in the innermost body
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                mutable i = 0;
                mutable j = 0;
                mutable k = 0;
                use q = Qubit();
                while i < 2 {
                    while j < 2 {
                        while k < 2 {
                            if M(q) == One {
                                return 7;
                            }
                            k += 1;
                        }
                        j += 1;
                    }
                    i += 1;
                }
                0
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                mutable i : Int = 0;
                mutable j : Int = 0;
                mutable k : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                while ((not __has_returned) and (i < 2)) {
                    while ((not __has_returned) and (j < 2)) {
                        while ((not __has_returned) and (k < 2)) {
                            if (M(q) == One) {
                                {
                                    let _generated_ident_74 : Int = 7;
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val = _generated_ident_74;
                                        __has_returned = true;
                                    };
                                };
                            }

                            if (not __has_returned) {
                                k += 1;
                            };
                        }

                        if (not __has_returned) {
                            j += 1;
                        };
                    }

                    if (not __has_returned) {
                        i += 1;
                    };
                }

                let _generated_ident_86 : Int = {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_86
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn for_for_for_return_deep() {
    // for / for / for -> return deep inside the innermost body
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                for a in 0..2 {
                    for b in 0..2 {
                        for c in 0..2 {
                            if M(q) == One {
                                return a + b + c;
                            }
                        }
                    }
                }
                0
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                {
                    let _range_id_141 : Range = 0..2;
                    mutable _index_id_144 : Int = _range_id_141.Start;
                    let _step_id_149 : Int = _range_id_141.Step;
                    let _end_id_154 : Int = _range_id_141.End;
                    while ((not __has_returned) and (((_step_id_149 > 0) and (_index_id_144 <= _end_id_154)) or ((_step_id_149 < 0) and (_index_id_144 >= _end_id_154)))) {
                        let a : Int = _index_id_144;
                        {
                            let _range_id_98 : Range = 0..2;
                            mutable _index_id_101 : Int = _range_id_98.Start;
                            let _step_id_106 : Int = _range_id_98.Step;
                            let _end_id_111 : Int = _range_id_98.End;
                            while ((not __has_returned) and (((_step_id_106 > 0) and (_index_id_101 <= _end_id_111)) or ((_step_id_106 < 0) and (_index_id_101 >= _end_id_111)))) {
                                let b : Int = _index_id_101;
                                {
                                    let _range_id_55 : Range = 0..2;
                                    mutable _index_id_58 : Int = _range_id_55.Start;
                                    let _step_id_63 : Int = _range_id_55.Step;
                                    let _end_id_68 : Int = _range_id_55.End;
                                    while ((not __has_returned) and (((_step_id_63 > 0) and (_index_id_58 <= _end_id_68)) or ((_step_id_63 < 0) and (_index_id_58 >= _end_id_68)))) {
                                        let c : Int = _index_id_58;
                                        if (M(q) == One) {
                                            {
                                                let _generated_ident_189 : Int = ((a + b) + c);
                                                __quantum__rt__qubit_release(q);
                                                {
                                                    __ret_val = _generated_ident_189;
                                                    __has_returned = true;
                                                };
                                            };
                                        }

                                        if (not __has_returned) {
                                            _index_id_58 += _step_id_63;
                                        };
                                    }

                                }

                                if (not __has_returned) {
                                    _index_id_101 += _step_id_106;
                                };
                            }

                        }

                        if (not __has_returned) {
                            _index_id_144 += _step_id_149;
                        };
                    }

                }

                let _generated_ident_201 : Int = {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_201
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn for_while_if_return_deep() {
    // for / while / if -> return inside the if
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                for i in 0..2 {
                    mutable j = 0;
                    while j < 2 {
                        if M(q) == One {
                            return i * 10 + j;
                        }
                        j += 1;
                    }
                }
                0
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                {
                    let _range_id_53 : Range = 0..2;
                    mutable _index_id_56 : Int = _range_id_53.Start;
                    let _step_id_61 : Int = _range_id_53.Step;
                    let _end_id_66 : Int = _range_id_53.End;
                    while ((not __has_returned) and (((_step_id_61 > 0) and (_index_id_56 <= _end_id_66)) or ((_step_id_61 < 0) and (_index_id_56 >= _end_id_66)))) {
                        let i : Int = _index_id_56;
                        mutable j : Int = 0;
                        while ((not __has_returned) and (j < 2)) {
                            if (M(q) == One) {
                                {
                                    let _generated_ident_101 : Int = ((i * 10) + j);
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val = _generated_ident_101;
                                        __has_returned = true;
                                    };
                                };
                            }

                            if (not __has_returned) {
                                j += 1;
                            };
                        }

                        if (not __has_returned) {
                            _index_id_56 += _step_id_61;
                        };
                    }

                }

                let _generated_ident_113 : Int = {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_113
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn if_while_for_return_deep() {
    // if / while / for -> return inside the for
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                if M(q) == One {
                    mutable i = 0;
                    while i < 3 {
                        for j in 0..2 {
                            if M(q) == Zero {
                                return i + j;
                            }
                        }
                        i += 1;
                    }
                }
                -1
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                if (M(q) == One) {
                    mutable i : Int = 0;
                    while ((not __has_returned) and (i < 3)) {
                        {
                            let _range_id_61 : Range = 0..2;
                            mutable _index_id_64 : Int = _range_id_61.Start;
                            let _step_id_69 : Int = _range_id_61.Step;
                            let _end_id_74 : Int = _range_id_61.End;
                            while ((not __has_returned) and (((_step_id_69 > 0) and (_index_id_64 <= _end_id_74)) or ((_step_id_69 < 0) and (_index_id_64 >= _end_id_74)))) {
                                let j : Int = _index_id_64;
                                if (M(q) == Zero) {
                                    {
                                        let _generated_ident_109 : Int = (i + j);
                                        __quantum__rt__qubit_release(q);
                                        {
                                            __ret_val = _generated_ident_109;
                                            __has_returned = true;
                                        };
                                    };
                                }

                                if (not __has_returned) {
                                    _index_id_64 += _step_id_69;
                                };
                            }

                        }

                        if (not __has_returned) {
                            i += 1;
                        };
                    }

                }

                let _generated_ident_121 : Int = if (not __has_returned) {
                    (-1)
                } else {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_121
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Before/after delta snapshots taken across only the ANF phase.
//!
//! [`check_anf_isolated_q`] drives the compound-position hoist fixpoint to
//! completion, snapshots the FIR, runs the standalone ANF operand-lift
//! fixpoint, and snapshots again. Every difference between the two snapshots is
//! therefore attributable solely to the operand lift, so the pinned delta
//! witnesses exactly what the ANF phase rewrites — here, a `return` buried in a
//! `BinOp` operand block being bound to a spine `let __operand_tmp` and the
//! operand slot rewritten to read the temp.

use super::*;

#[test]
fn isolated_anf_lifts_return_in_binop_operand_block() {
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 1 + { return 2; 3 };
                x
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                let x : Int = (1 + {
                    return 2;
                    3
                });
                x
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                let __operand_tmp_0 : Int = 1;
                let __operand_tmp_1 : Int = {
                    return 2;
                    3
                };
                let x : Int = (__operand_tmp_0 + __operand_tmp_1);
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_index_target_block() {
    // `({ return 1; arr })[5]` — the Index target operand is a statement-
    // carrying block burying a `return`. The target is eagerly evaluated before
    // the access, so the ANF phase binds the block to a spine `let __operand_tmp`
    // and rewrites the Index target slot to read the temp; the `[5]` index
    // literal stays inline. This is the Index target shape (distinct from the
    // already-witnessed Index index-slot lift).
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [10, 20, 30];
                ({ return 1; arr })[5]
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                let arr : Int[] = [10, 20, 30];
                {
                    return 1;
                    arr
                }
                [5]
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                let arr : Int[] = [10, 20, 30];
                let __operand_tmp_0 : Int[] = {
                    return 1;
                    arr
                };
                __operand_tmp_0[5]
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_update_field_receiver_block() {
    // `({ return 7; p }) w/ First <- 9` — the UpdateField receiver operand is a
    // statement-carrying block burying a `return`. The receiver is eagerly
    // evaluated before the copy-and-update, so the ANF phase binds the block to
    // a spine `let __operand_tmp` and rewrites the record slot to read the temp;
    // the `<- 9` value literal stays inline.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let p = new Pair { First = 1, Second = 2 };
                let q = ({ return 7; p }) w/ First <- 9;
                q::First
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            newtype Pair = (Int, Int);
            function Main() : Int {
                let p : __UDT_Item_1__Package_2_ = new Pair {
                    First = 1,
                    Second = 2
                };
                let q : __UDT_Item_1__Package_2_ = {
                    return 7;
                    p
                }
                    w/::First <- 9;
                q::First
            }
            // entry
            Main()

            // after anf
            newtype Pair = (Int, Int);
            function Main() : Int {
                let p : __UDT_Item_1__Package_2_ = new Pair {
                    First = 1,
                    Second = 2
                };
                let __operand_tmp_0 : __UDT_Item_1__Package_2_ = {
                    return 7;
                    p
                };
                let q : __UDT_Item_1__Package_2_ = __operand_tmp_0 w/::First <- 9;
                q::First
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_update_field_value_block() {
    // `p w/ First <- { return 7; 9 }` — the UpdateField value operand is a
    // statement-carrying block burying a `return`. The ANF phase pins the
    // earlier-sibling receiver `p` to a temp, then binds the value block to a
    // second spine `let __operand_tmp` and rewrites both slots to read the
    // temps. (Pinning `p` is harmless: UpdateField produces a new value.)
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let p = new Pair { First = 1, Second = 2 };
                let q = p w/ First <- { return 7; 9 };
                q::First
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            newtype Pair = (Int, Int);
            function Main() : Int {
                let p : __UDT_Item_1__Package_2_ = new Pair {
                    First = 1,
                    Second = 2
                };
                let q : __UDT_Item_1__Package_2_ = p w/::First <- {
                    return 7;
                    9
                };
                q::First
            }
            // entry
            Main()

            // after anf
            newtype Pair = (Int, Int);
            function Main() : Int {
                let p : __UDT_Item_1__Package_2_ = new Pair {
                    First = 1,
                    Second = 2
                };
                let __operand_tmp_0 : __UDT_Item_1__Package_2_ = p;
                let __operand_tmp_1 : Int = {
                    return 7;
                    9
                };
                let q : __UDT_Item_1__Package_2_ = __operand_tmp_0 w/::First <- __operand_tmp_1;
                q::First
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_assign_field_value_block() {
    // `set p w/= First <- { return 7; 9 }` — the AssignField value operand is a
    // statement-carrying block burying a `return`. The ANF phase binds the value
    // block to a spine `let __operand_tmp` and rewrites the `w/=` value slot to
    // read the temp. The assignment place `p` is preserved (not pinned to a
    // by-value copy), so the write still lands on the original mutable binding
    // and the trailing read `p::First` names it. This delta is unobservable here
    // because the buried `return` fires before the assignment; the non-firing
    // soundness case is covered by a `check_semantic_equivalence` fixture in
    // semantic.rs.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                mutable p = new Pair { First = 1, Second = 2 };
                set p w/= First <- { return 7; 9 };
                p::First
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable p : __UDT_Item_1__Package_2_ = new Pair {
                    First = 1,
                    Second = 2
                };
                p w/=::First <- {
                    return 7;
                    9
                };
                p::First
            }
            // entry
            Main()

            // after anf
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable p : __UDT_Item_1__Package_2_ = new Pair {
                    First = 1,
                    Second = 2
                };
                let __operand_tmp_0 : Int = {
                    return 7;
                    9
                };
                p w/=::First <- __operand_tmp_0;
                p::First
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_fail_operand_block() {
    // `fail { return 1; "msg" }` — the Fail operand is a statement-carrying
    // block burying a `return`. The operand is eagerly evaluated before `fail`,
    // so the ANF phase binds the block to a spine `let __operand_tmp` and
    // rewrites the `fail` operand to read the temp. The matching
    // `check_semantic_equivalence` fixture early-returns `1` and so does not
    // witness this lift, which this snapshot pins directly.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                fail { return 1; "msg" }
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                fail {
                    return 1;
                    $"msg"
                }

            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                let __operand_tmp_0 : String = {
                    return 1;
                    $"msg"
                };
                fail __operand_tmp_0
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_whole_block_with_if_in_binop_operand() {
    // `1 + { if c { return 2 } 3 }` — an `if` whose `then` branch buries a
    // `return`. A bare `if` is a statement in Q#, never an operand on its own,
    // so the nearest surface-expressible witness wraps it in a statement-
    // carrying block. That block sits in the non-`Call` right operand slot of a
    // `BinOp`, so the ANF phase binds the whole block (carrying the `if`) to a
    // spine `let __operand_tmp` before the addition runs. The before/after
    // delta isolates this whole-construct lift to the ANF phase alone.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable c = true;
                let x = 1 + {
                    if c {
                        return 2;
                    }
                    3
                };
                x
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                mutable c : Bool = true;
                let x : Int = (1 + {
                    if c {
                        return 2;
                    }

                    3
                });
                x
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                mutable c : Bool = true;
                let __operand_tmp_0 : Int = 1;
                let __operand_tmp_1 : Int = {
                    if c {
                        return 2;
                    }

                    3
                };
                let x : Int = (__operand_tmp_0 + __operand_tmp_1);
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_whole_block_with_while_in_binop_operand() {
    // `1 + { while c { return 2 } 0 }` — a `while` whose body buries a `return`
    // can only appear inside a statement-carrying block at the Q# surface
    // (`while` is a statement, never an operand on its own). That block sits in
    // the non-`Call` right operand slot of a `BinOp`, so the ANF phase binds the
    // whole block (carrying the loop) to a spine `let __operand_tmp` before the
    // addition runs. This is the nearest surface-expressible witness for a
    // loop-bearing operand in a non-`Call` position.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable c = true;
                let x = 1 + {
                    while c {
                        set c = false;
                        return 2;
                    }
                    0
                };
                x
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                mutable c : Bool = true;
                let x : Int = (1 + {
                    while c {
                        c = false;
                        return 2;
                    }

                    0
                });
                x
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                mutable c : Bool = true;
                let __operand_tmp_0 : Int = 1;
                let __operand_tmp_1 : Int = {
                    while c {
                        c = false;
                        return 2;
                    }

                    0
                };
                let x : Int = (__operand_tmp_0 + __operand_tmp_1);
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_if_condition_block() {
    // `if { return 1; true } { 5 } else { 7 }` — the `if` *condition* is a
    // statement-carrying block burying a `return`. The condition is evaluated
    // unconditionally before either branch, so the ANF phase lifts it to a
    // spine `let __operand_tmp` and rewrites the `if` to test the temp. The
    // before/after delta isolates this condition lift to the ANF phase.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if { return 1; true } { 5 } else { 7 }
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                if {
                    return 1;
                    true
                }
                {
                    5
                } else {
                    7
                }

            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                let __operand_tmp_0 : Bool = {
                    return 1;
                    true
                };
                if __operand_tmp_0 {
                    5
                } else {
                    7
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_range_end_block() {
    // `0..{ return 1; 5 }` — a `Range` whose end operand is a statement-carrying
    // block burying a `return`. The end is an eagerly-evaluated operand, so the
    // ANF phase lifts the block to a spine `let __operand_tmp` and rewrites the
    // range's end slot to read the temp.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let r = 0..{ return 1; 5 };
                r::End
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                let r : Range = 0..{
                    return 1;
                    5
                };
                r.End
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                let __operand_tmp_0 : Int = 0;
                let __operand_tmp_1 : Int = {
                    return 1;
                    5
                };
                let r : Range = __operand_tmp_0..__operand_tmp_1;
                r.End
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_array_repeat_size_block() {
    // `[0, size = { return 1; 5 }]` — an `ArrayRepeat` whose size operand is a
    // statement-carrying block burying a `return`. The size is an eagerly-
    // evaluated operand, so the ANF phase lifts the block to a spine
    // `let __operand_tmp` and rewrites the size slot to read the temp.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let a = [0, size = { return 1; 5 }];
                a[0]
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                let a : Int[] = [0, size = {
                    return 1;
                    5
                }];
                a[0]
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                let __operand_tmp_0 : Int = 0;
                let __operand_tmp_1 : Int = {
                    return 1;
                    5
                };
                let a : Int[] = [__operand_tmp_0, size = __operand_tmp_1];
                a[0]
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_assign_rhs_block() {
    // `set x = { return 1; 5 }` — the Assign value slot is a statement-carrying
    // block burying a `return`. The ANF phase binds the value block to a spine
    // `let __operand_tmp` and rewrites the `=` value slot to read the temp. The
    // assignment place `x` is preserved (not pinned to a by-value copy), so the
    // write still lands on the original mutable binding and the trailing read
    // names it. The matching `check_semantic_equivalence` fixture early-returns
    // `1` before the assignment, so this snapshot pins the lift directly.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable x = 0;
                set x = { return 1; 5 };
                x
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                mutable x : Int = 0;
                x = {
                    return 1;
                    5
                };
                x
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                mutable x : Int = 0;
                let __operand_tmp_0 : Int = {
                    return 1;
                    5
                };
                x = __operand_tmp_0;
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_assignop_rhs_block() {
    // `set x += { return 2; 5 }` — the AssignOp value slot is a statement-
    // carrying block burying a `return`. The ANF phase binds the value block to
    // a spine `let __operand_tmp` and rewrites the `+=` value slot to read the
    // temp. The assignment place `x` is preserved (not pinned to a by-value
    // copy), so the compound update still lands on the original mutable binding.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable x = 10;
                set x += { return 2; 5 };
                x
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                mutable x : Int = 10;
                x += {
                    return 2;
                    5
                };
                x
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                mutable x : Int = 10;
                let __operand_tmp_0 : Int = {
                    return 2;
                    5
                };
                x += __operand_tmp_0;
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_assignindex_replacement_block() {
    // `set arr w/= 0 <- { return 3; 5 }` — the AssignIndex replacement (value)
    // slot is a statement-carrying block burying a `return`. The ANF phase binds
    // the value block to a spine `let __operand_tmp` and rewrites the `w/=`
    // value slot to read the temp. The assignment place `arr` is preserved (not
    // pinned to a by-value copy), so the element write still lands on the
    // original mutable binding.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable arr = [1, 2, 3];
                set arr w/= 0 <- { return 3; 5 };
                arr[0]
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                mutable arr : Int[] = [1, 2, 3];
                arr w/= 0 <- {
                    return 3;
                    5
                };
                arr[0]
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                mutable arr : Int[] = [1, 2, 3];
                let __operand_tmp_0 : Int = 0;
                let __operand_tmp_1 : Int = {
                    return 3;
                    5
                };
                arr w/= __operand_tmp_0 <- __operand_tmp_1;
                arr[0]
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_assignindex_index_block() {
    // `set arr w/= { return 4; 0 } <- 5` — the AssignIndex index slot is a
    // statement-carrying block burying a `return`. The ANF phase binds the index
    // block to a spine `let __operand_tmp` and rewrites the `w/=` index slot to
    // read the temp. The assignment place `arr` is preserved (not pinned to a
    // by-value copy), so the element write still lands on the original mutable
    // binding; the `<- 5` replacement literal stays inline.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable arr = [1, 2, 3];
                set arr w/= { return 4; 0 } <- 5;
                arr[0]
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function Main() : Int {
                mutable arr : Int[] = [1, 2, 3];
                arr w/= {
                    return 4;
                    0
                } <- 5;
                arr[0]
            }
            // entry
            Main()

            // after anf
            function Main() : Int {
                mutable arr : Int[] = [1, 2, 3];
                let __operand_tmp_0 : Int = {
                    return 4;
                    0
                };
                arr w/= __operand_tmp_0 <- 5;
                arr[0]
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn isolated_anf_lifts_short_circuit_and_lhs_block_leaving_rhs_inline() {
    // `{ return 7; true } and G()` — the `and` left operand is a statement-
    // carrying block burying a `return`. The lift rewrites only the LHS: it is
    // unconditionally evaluated, so the block is bound to a spine
    // `let __operand_tmp` and the LHS slot reads the temp. The RHS `G()` is
    // *conditionally* evaluated (only when the LHS is true), so the lift must
    // leave it inline rather than hoisting it to the spine — hoisting would make
    // its effects unconditional. This delta witnesses exactly that asymmetry:
    // the LHS becomes a temp, the RHS stays in operand position.
    check_anf_isolated_q(
        indoc! {r#"
        namespace Test {
            function G() : Bool { true }
            function Main() : Bool {
                let b = { return true; true } and G();
                b
            }
        }
    "#},
        "Main",
        &expect![[r#"
            // before anf (changed=true)
            function G() : Bool {
                true
            }
            function Main() : Bool {
                let b : Bool = ({
                    return true;
                    true
                } and G());
                b
            }
            // entry
            Main()

            // after anf
            function G() : Bool {
                true
            }
            function Main() : Bool {
                let __operand_tmp_0 : Bool = {
                    return true;
                    true
                };
                let b : Bool = (__operand_tmp_0 and G());
                b
            }
            // entry
            Main()
        "#]],
    );
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{parse, parse_block};
use crate::tests::check;
use expect_test::expect;

#[test]
fn empty_stmt() {
    check(parse, ";", &expect!["Stmt _id_ [0-1]: Empty"]);
}

#[test]
fn let_stmt() {
    check(
        parse,
        "let x = 2;",
        &expect![[r#"
            Stmt _id_ [0-10]: Local (Immutable):
                Pat _id_ [4-5]: Bind:
                    Ident _id_ [4-5] "x"
                Expr _id_ [8-9]: Lit: Int(2)"#]],
    );
}

#[test]
fn let_pat_match() {
    check(
        parse,
        "let (x, (y, z)) = foo;",
        &expect![[r#"
            Stmt _id_ [0-22]: Local (Immutable):
                Pat _id_ [4-15]: Tuple:
                    Pat _id_ [5-6]: Bind:
                        Ident _id_ [5-6] "x"
                    Pat _id_ [8-14]: Tuple:
                        Pat _id_ [9-10]: Bind:
                            Ident _id_ [9-10] "y"
                        Pat _id_ [12-13]: Bind:
                            Ident _id_ [12-13] "z"
                Expr _id_ [18-21]: Path: Path _id_ [18-21] (Ident _id_ [18-21] "foo")"#]],
    );
}

#[test]
fn mutable_stmt() {
    check(
        parse,
        "mutable x = 2;",
        &expect![[r#"
            Stmt _id_ [0-14]: Local (Mutable):
                Pat _id_ [8-9]: Bind:
                    Ident _id_ [8-9] "x"
                Expr _id_ [12-13]: Lit: Int(2)"#]],
    );
}

#[test]
fn use_stmt() {
    check(
        parse,
        "use q = Qubit();",
        &expect![[r#"
            Stmt _id_ [0-16]: Qubit (Fresh)
                Pat _id_ [4-5]: Bind:
                    Ident _id_ [4-5] "q"
                QubitInit _id_ [8-15] Single"#]],
    );
}

#[test]
fn use_qubit_array() {
    check(
        parse,
        "use qs = Qubit[5];",
        &expect![[r#"
            Stmt _id_ [0-18]: Qubit (Fresh)
                Pat _id_ [4-6]: Bind:
                    Ident _id_ [4-6] "qs"
                QubitInit _id_ [9-17] Array:
                    Expr _id_ [15-16]: Lit: Int(5)"#]],
    );
}

#[test]
fn use_pat_match() {
    check(
        parse,
        "use (q1, q2) = (Qubit(), Qubit());",
        &expect![[r#"
            Stmt _id_ [0-34]: Qubit (Fresh)
                Pat _id_ [4-12]: Tuple:
                    Pat _id_ [5-7]: Bind:
                        Ident _id_ [5-7] "q1"
                    Pat _id_ [9-11]: Bind:
                        Ident _id_ [9-11] "q2"
                QubitInit _id_ [15-33] Tuple:
                    QubitInit _id_ [16-23] Single
                    QubitInit _id_ [25-32] Single"#]],
    );
}

#[test]
fn use_paren() {
    check(
        parse,
        "use q = (Qubit());",
        &expect![[r#"
            Stmt _id_ [0-18]: Qubit (Fresh)
                Pat _id_ [4-5]: Bind:
                    Ident _id_ [4-5] "q"
                QubitInit _id_ [8-17] Parens:
                    QubitInit _id_ [9-16] Single"#]],
    );
}

#[test]
fn use_single_tuple() {
    check(
        parse,
        "use (q,) = (Qubit(),);",
        &expect![[r#"
            Stmt _id_ [0-22]: Qubit (Fresh)
                Pat _id_ [4-8]: Tuple:
                    Pat _id_ [5-6]: Bind:
                        Ident _id_ [5-6] "q"
                QubitInit _id_ [11-21] Tuple:
                    QubitInit _id_ [12-19] Single"#]],
    );
}

#[test]
fn use_invalid_init() {
    check(
        parse,
        "use q = Qutrit();",
        &expect![[r#"
            Error(
                Convert(
                    "qubit initializer",
                    "identifier",
                    Span {
                        lo: 8,
                        hi: 14,
                    },
                ),
                Some(
                    "to allocate qubits, use syntax like `use q = Qubit();` or `use qs = Qubit[N];` or `use (q1, q2) = (Qubit(), Qubit());`",
                ),
            )
        "#]],
    );
}

#[test]
fn use_tuple_duplicate_commas() {
    check(
        parse,
        "use (q1,, q2) = (Qubit(),, Qubit());",
        &expect![[r#"
            Stmt _id_ [0-36]: Qubit (Fresh)
                Pat _id_ [4-13]: Tuple:
                    Pat _id_ [5-7]: Bind:
                        Ident _id_ [5-7] "q1"
                    Pat _id_ [8-8]: Err
                    Pat _id_ [10-12]: Bind:
                        Ident _id_ [10-12] "q2"
                QubitInit _id_ [16-35] Tuple:
                    QubitInit _id_ [17-24] Single
                    QubitInit _id_ [25-25] Err
                    QubitInit _id_ [27-34] Single

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 8,
                            hi: 8,
                        },
                    ),
                ),
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 25,
                            hi: 25,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn use_tuple_initial_commas() {
    check(
        parse,
        "use (,, q1, q2) = (, Qubit(), Qubit());",
        &expect![[r#"
            Stmt _id_ [0-39]: Qubit (Fresh)
                Pat _id_ [4-15]: Tuple:
                    Pat _id_ [5-5]: Err
                    Pat _id_ [6-6]: Err
                    Pat _id_ [8-10]: Bind:
                        Ident _id_ [8-10] "q1"
                    Pat _id_ [12-14]: Bind:
                        Ident _id_ [12-14] "q2"
                QubitInit _id_ [18-38] Tuple:
                    QubitInit _id_ [19-19] Err
                    QubitInit _id_ [21-28] Single
                    QubitInit _id_ [30-37] Single

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 5,
                            hi: 5,
                        },
                    ),
                ),
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 6,
                            hi: 6,
                        },
                    ),
                ),
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 19,
                            hi: 19,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn borrow_stmt() {
    check(
        parse,
        "borrow q = Qubit();",
        &expect![[r#"
            Stmt _id_ [0-19]: Qubit (Dirty)
                Pat _id_ [7-8]: Bind:
                    Ident _id_ [7-8] "q"
                QubitInit _id_ [11-18] Single"#]],
    );
}

#[test]
fn let_in_block() {
    check(
        parse_block,
        "{ let x = 2; x }",
        &expect![[r#"
            Block _id_ [0-16]:
                Stmt _id_ [2-12]: Local (Immutable):
                    Pat _id_ [6-7]: Bind:
                        Ident _id_ [6-7] "x"
                    Expr _id_ [10-11]: Lit: Int(2)
                Stmt _id_ [13-14]: Expr: Expr _id_ [13-14]: Path: Path _id_ [13-14] (Ident _id_ [13-14] "x")"#]],
    );
}

#[test]
fn exprs_in_block() {
    check(
        parse_block,
        "{ x; y; z }",
        &expect![[r#"
            Block _id_ [0-11]:
                Stmt _id_ [2-4]: Semi: Expr _id_ [2-3]: Path: Path _id_ [2-3] (Ident _id_ [2-3] "x")
                Stmt _id_ [5-7]: Semi: Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")
                Stmt _id_ [8-9]: Expr: Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "z")"#]],
    );
}

#[test]
fn trailing_semi_expr() {
    check(
        parse_block,
        "{ x; y; z; }",
        &expect![[r#"
            Block _id_ [0-12]:
                Stmt _id_ [2-4]: Semi: Expr _id_ [2-3]: Path: Path _id_ [2-3] (Ident _id_ [2-3] "x")
                Stmt _id_ [5-7]: Semi: Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")
                Stmt _id_ [8-10]: Semi: Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "z")"#]],
    );
}

#[test]
fn stmt_missing_semi() {
    check(
        parse,
        "let x = 2",
        &expect![[r#"
            Stmt _id_ [0-9]: Local (Immutable):
                Pat _id_ [4-5]: Bind:
                    Ident _id_ [4-5] "x"
                Expr _id_ [8-9]: Lit: Int(2)

            [
                Error(
                    Token(
                        Semi,
                        Eof,
                        Span {
                            lo: 9,
                            hi: 9,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn if_followed_by() {
    check(
        parse_block,
        "{ if c { x } return x; }",
        &expect![[r#"
            Block _id_ [0-24]:
                Stmt _id_ [2-12]: Expr: Expr _id_ [2-12]: If:
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "c")
                    Block _id_ [7-12]:
                        Stmt _id_ [9-10]: Expr: Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "x")
                Stmt _id_ [13-22]: Semi: Expr _id_ [13-21]: Return: Expr _id_ [20-21]: Path: Path _id_ [20-21] (Ident _id_ [20-21] "x")"#]],
    );
}

#[test]
fn let_if() {
    check(
        parse_block,
        "{ let x = if c { true } else { false }; x }",
        &expect![[r#"
            Block _id_ [0-43]:
                Stmt _id_ [2-39]: Local (Immutable):
                    Pat _id_ [6-7]: Bind:
                        Ident _id_ [6-7] "x"
                    Expr _id_ [10-38]: If:
                        Expr _id_ [13-14]: Path: Path _id_ [13-14] (Ident _id_ [13-14] "c")
                        Block _id_ [15-23]:
                            Stmt _id_ [17-21]: Expr: Expr _id_ [17-21]: Lit: Bool(true)
                        Expr _id_ [24-38]: Expr Block: Block _id_ [29-38]:
                            Stmt _id_ [31-36]: Expr: Expr _id_ [31-36]: Lit: Bool(false)
                Stmt _id_ [40-41]: Expr: Expr _id_ [40-41]: Path: Path _id_ [40-41] (Ident _id_ [40-41] "x")"#]],
    );
}

#[test]
fn empty_block() {
    check(parse_block, "{}", &expect!["Block _id_ [0-2]: <empty>"]);
}

#[test]
fn two_stmts() {
    check(
        parse_block,
        "{ let x = 1; x }",
        &expect![[r#"
            Block _id_ [0-16]:
                Stmt _id_ [2-12]: Local (Immutable):
                    Pat _id_ [6-7]: Bind:
                        Ident _id_ [6-7] "x"
                    Expr _id_ [10-11]: Lit: Int(1)
                Stmt _id_ [13-14]: Expr: Expr _id_ [13-14]: Path: Path _id_ [13-14] (Ident _id_ [13-14] "x")"#]],
    );
}

#[test]
fn two_empty_stmts() {
    check(
        parse_block,
        "{ ;; }",
        &expect![[r#"
            Block _id_ [0-6]:
                Stmt _id_ [2-3]: Empty
                Stmt _id_ [3-4]: Empty"#]],
    );
}

#[test]
fn empty_stmt_after_expr() {
    check(
        parse_block,
        "{ x;; }",
        &expect![[r#"
            Block _id_ [0-7]:
                Stmt _id_ [2-4]: Semi: Expr _id_ [2-3]: Path: Path _id_ [2-3] (Ident _id_ [2-3] "x")
                Stmt _id_ [4-5]: Empty"#]],
    );
}

#[test]
fn call_block_no_parens() {
    check(
        parse_block,
        "{ { let a = b; a }(c, d) }",
        &expect![[r#"
            Block _id_ [0-26]:
                Stmt _id_ [2-18]: Expr: Expr _id_ [2-18]: Expr Block: Block _id_ [2-18]:
                    Stmt _id_ [4-14]: Local (Immutable):
                        Pat _id_ [8-9]: Bind:
                            Ident _id_ [8-9] "a"
                        Expr _id_ [12-13]: Path: Path _id_ [12-13] (Ident _id_ [12-13] "b")
                    Stmt _id_ [15-16]: Expr: Expr _id_ [15-16]: Path: Path _id_ [15-16] (Ident _id_ [15-16] "a")
                Stmt _id_ [18-24]: Expr: Expr _id_ [18-24]: Tuple:
                    Expr _id_ [19-20]: Path: Path _id_ [19-20] (Ident _id_ [19-20] "c")
                    Expr _id_ [22-23]: Path: Path _id_ [22-23] (Ident _id_ [22-23] "d")"#]],
    );
}

#[test]
fn call_block_parens() {
    check(
        parse_block,
        "{ ({ let a = b; a })(c, d) }",
        &expect![[r#"
            Block _id_ [0-28]:
                Stmt _id_ [2-26]: Expr: Expr _id_ [2-26]: Call:
                    Expr _id_ [2-20]: Paren: Expr _id_ [3-19]: Expr Block: Block _id_ [3-19]:
                        Stmt _id_ [5-15]: Local (Immutable):
                            Pat _id_ [9-10]: Bind:
                                Ident _id_ [9-10] "a"
                            Expr _id_ [13-14]: Path: Path _id_ [13-14] (Ident _id_ [13-14] "b")
                        Stmt _id_ [16-17]: Expr: Expr _id_ [16-17]: Path: Path _id_ [16-17] (Ident _id_ [16-17] "a")
                    Expr _id_ [20-26]: Tuple:
                        Expr _id_ [21-22]: Path: Path _id_ [21-22] (Ident _id_ [21-22] "c")
                        Expr _id_ [24-25]: Path: Path _id_ [24-25] (Ident _id_ [24-25] "d")"#]],
    );
}

#[test]
fn if_stmt_plus() {
    check(
        parse_block,
        "{ if x { 1 } else { 2 } + 3 }",
        &expect![[r#"
            Block _id_ [0-29]:
                Stmt _id_ [2-23]: Expr: Expr _id_ [2-23]: If:
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "x")
                    Block _id_ [7-12]:
                        Stmt _id_ [9-10]: Expr: Expr _id_ [9-10]: Lit: Int(1)
                    Expr _id_ [13-23]: Expr Block: Block _id_ [18-23]:
                        Stmt _id_ [20-21]: Expr: Expr _id_ [20-21]: Lit: Int(2)
                Stmt _id_ [24-27]: Expr: Expr _id_ [24-27]: UnOp (Pos):
                    Expr _id_ [26-27]: Lit: Int(3)"#]],
    );
}

#[test]
fn if_expr_plus() {
    check(
        parse_block,
        "{ let y = if x { 1 } else { 2 } + 3; }",
        &expect![[r#"
            Block _id_ [0-38]:
                Stmt _id_ [2-36]: Local (Immutable):
                    Pat _id_ [6-7]: Bind:
                        Ident _id_ [6-7] "y"
                    Expr _id_ [10-35]: BinOp (Add):
                        Expr _id_ [10-31]: If:
                            Expr _id_ [13-14]: Path: Path _id_ [13-14] (Ident _id_ [13-14] "x")
                            Block _id_ [15-20]:
                                Stmt _id_ [17-18]: Expr: Expr _id_ [17-18]: Lit: Int(1)
                            Expr _id_ [21-31]: Expr Block: Block _id_ [26-31]:
                                Stmt _id_ [28-29]: Expr: Expr _id_ [28-29]: Lit: Int(2)
                        Expr _id_ [34-35]: Lit: Int(3)"#]],
    );
}

#[test]
fn if_semi_if() {
    check(
        parse_block,
        "{ if x { f(); }; if y { g(); } }",
        &expect![[r#"
            Block _id_ [0-32]:
                Stmt _id_ [2-16]: Semi: Expr _id_ [2-15]: If:
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "x")
                    Block _id_ [7-15]:
                        Stmt _id_ [9-13]: Semi: Expr _id_ [9-12]: Call:
                            Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "f")
                            Expr _id_ [10-12]: Unit
                Stmt _id_ [17-30]: Expr: Expr _id_ [17-30]: If:
                    Expr _id_ [20-21]: Path: Path _id_ [20-21] (Ident _id_ [20-21] "y")
                    Block _id_ [22-30]:
                        Stmt _id_ [24-28]: Semi: Expr _id_ [24-27]: Call:
                            Expr _id_ [24-25]: Path: Path _id_ [24-25] (Ident _id_ [24-25] "g")
                            Expr _id_ [25-27]: Unit"#]],
    );
}

#[test]
fn if_no_semi_if() {
    check(
        parse_block,
        "{ if x { f(); } if y { g(); } }",
        &expect![[r#"
            Block _id_ [0-31]:
                Stmt _id_ [2-15]: Expr: Expr _id_ [2-15]: If:
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "x")
                    Block _id_ [7-15]:
                        Stmt _id_ [9-13]: Semi: Expr _id_ [9-12]: Call:
                            Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "f")
                            Expr _id_ [10-12]: Unit
                Stmt _id_ [16-29]: Expr: Expr _id_ [16-29]: If:
                    Expr _id_ [19-20]: Path: Path _id_ [19-20] (Ident _id_ [19-20] "y")
                    Block _id_ [21-29]:
                        Stmt _id_ [23-27]: Semi: Expr _id_ [23-26]: Call:
                            Expr _id_ [23-24]: Path: Path _id_ [23-24] (Ident _id_ [23-24] "g")
                            Expr _id_ [24-26]: Unit"#]],
    );
}

#[test]
fn call_semi_call() {
    check(
        parse_block,
        "{ f(x); g(y) }",
        &expect![[r#"
            Block _id_ [0-14]:
                Stmt _id_ [2-7]: Semi: Expr _id_ [2-6]: Call:
                    Expr _id_ [2-3]: Path: Path _id_ [2-3] (Ident _id_ [2-3] "f")
                    Expr _id_ [3-6]: Paren: Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Stmt _id_ [8-12]: Expr: Expr _id_ [8-12]: Call:
                    Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "g")
                    Expr _id_ [9-12]: Paren: Expr _id_ [10-11]: Path: Path _id_ [10-11] (Ident _id_ [10-11] "y")"#]],
    );
}

#[test]
fn call_no_semi_call() {
    check(
        parse_block,
        "{ f(x) g(y) }",
        &expect![[r#"
            Block _id_ [0-13]:
                Stmt _id_ [2-6]: Expr: Expr _id_ [2-6]: Call:
                    Expr _id_ [2-3]: Path: Path _id_ [2-3] (Ident _id_ [2-3] "f")
                    Expr _id_ [3-6]: Paren: Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Stmt _id_ [7-11]: Expr: Expr _id_ [7-11]: Call:
                    Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "g")
                    Expr _id_ [8-11]: Paren: Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "y")

            [
                Error(
                    MissingSemi(
                        Span {
                            lo: 6,
                            hi: 6,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn expr_plus_if_semi() {
    check(
        parse_block,
        "{ 1 + if true { 2 } else { 3 }; f(x) }",
        &expect![[r#"
            Block _id_ [0-38]:
                Stmt _id_ [2-31]: Semi: Expr _id_ [2-30]: BinOp (Add):
                    Expr _id_ [2-3]: Lit: Int(1)
                    Expr _id_ [6-30]: If:
                        Expr _id_ [9-13]: Lit: Bool(true)
                        Block _id_ [14-19]:
                            Stmt _id_ [16-17]: Expr: Expr _id_ [16-17]: Lit: Int(2)
                        Expr _id_ [20-30]: Expr Block: Block _id_ [25-30]:
                            Stmt _id_ [27-28]: Expr: Expr _id_ [27-28]: Lit: Int(3)
                Stmt _id_ [32-36]: Expr: Expr _id_ [32-36]: Call:
                    Expr _id_ [32-33]: Path: Path _id_ [32-33] (Ident _id_ [32-33] "f")
                    Expr _id_ [33-36]: Paren: Expr _id_ [34-35]: Path: Path _id_ [34-35] (Ident _id_ [34-35] "x")"#]],
    );
}

#[test]
fn expr_plus_if_no_semi() {
    check(
        parse_block,
        "{ 1 + if true { 2 } else { 3 } f(x) }",
        &expect![[r#"
            Block _id_ [0-37]:
                Stmt _id_ [2-30]: Expr: Expr _id_ [2-30]: BinOp (Add):
                    Expr _id_ [2-3]: Lit: Int(1)
                    Expr _id_ [6-30]: If:
                        Expr _id_ [9-13]: Lit: Bool(true)
                        Block _id_ [14-19]:
                            Stmt _id_ [16-17]: Expr: Expr _id_ [16-17]: Lit: Int(2)
                        Expr _id_ [20-30]: Expr Block: Block _id_ [25-30]:
                            Stmt _id_ [27-28]: Expr: Expr _id_ [27-28]: Lit: Int(3)
                Stmt _id_ [31-35]: Expr: Expr _id_ [31-35]: Call:
                    Expr _id_ [31-32]: Path: Path _id_ [31-32] (Ident _id_ [31-32] "f")
                    Expr _id_ [32-35]: Paren: Expr _id_ [33-34]: Path: Path _id_ [33-34] (Ident _id_ [33-34] "x")

            [
                Error(
                    MissingSemi(
                        Span {
                            lo: 30,
                            hi: 30,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn block_function() {
    check(
        parse_block,
        "{
            function Foo() : () {}
        }",
        &expect![[r#"
            Block _id_ [0-46]:
                Stmt _id_ [14-36]: Item: Item _id_ [14-36]:
                    Callable _id_ [14-36] (Function):
                        name: Ident _id_ [23-26] "Foo"
                        input: Pat _id_ [26-28]: Unit
                        output: Type _id_ [31-33]: Unit
                        body: Block: Block _id_ [34-36]: <empty>"#]],
    );
}

#[test]
fn block_function_doc() {
    check(
        parse_block,
        "{
            /// This is a doc comment
            function Foo() : () {}
        }",
        &expect![[r#"
            Block _id_ [0-84]:
                Stmt _id_ [14-74]: Item: Item _id_ [14-74]:
                    doc:
                        This is a doc comment
                    Callable _id_ [52-74] (Function):
                        name: Ident _id_ [61-64] "Foo"
                        input: Pat _id_ [64-66]: Unit
                        output: Type _id_ [69-71]: Unit
                        body: Block: Block _id_ [72-74]: <empty>"#]],
    );
}

#[test]
fn doc_at_end_of_block() {
    check(
        parse_block,
        "{
            /// This is a doc comment.
        }",
        &expect![[r#"
            Block _id_ [0-50]:
                Stmt _id_ [14-40]: Item: Item _id_ [14-40]:
                    Err

            [
                Error(
                    FloatingDocComment(
                        Span {
                            lo: 14,
                            hi: 40,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn doc_followed_by_non_item() {
    check(
        parse_block,
        "{
            /// This is a doc comment
            let x = 2;
        }",
        &expect![[r#"
            Block _id_ [0-72]:
                Stmt _id_ [14-39]: Item: Item _id_ [14-39]:
                    Err
                Stmt _id_ [52-62]: Local (Immutable):
                    Pat _id_ [56-57]: Bind:
                        Ident _id_ [56-57] "x"
                    Expr _id_ [60-61]: Lit: Int(2)

            [
                Error(
                    FloatingDocComment(
                        Span {
                            lo: 14,
                            hi: 39,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn recover_in_block() {
    check(
        parse_block,
        "{ let x = 1 +; x }",
        &expect![[r#"
            Block _id_ [0-18]:
                Stmt _id_ [2-14]: Local (Immutable):
                    Pat _id_ [6-7]: Bind:
                        Ident _id_ [6-7] "x"
                    Expr _id_ [10-13]: Err
                Stmt _id_ [15-16]: Expr: Expr _id_ [15-16]: Path: Path _id_ [15-16] (Ident _id_ [15-16] "x")

            [
                Error(
                    Rule(
                        "expression",
                        Semi,
                        Span {
                            lo: 13,
                            hi: 14,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn recover_in_nested_block() {
    check(
        parse_block,
        "{ let x = { 1 + }; x }",
        &expect![[r#"
            Block _id_ [0-22]:
                Stmt _id_ [2-18]: Local (Immutable):
                    Pat _id_ [6-7]: Bind:
                        Ident _id_ [6-7] "x"
                    Expr _id_ [10-17]: Expr Block: Block _id_ [10-17]:
                        Stmt _id_ [12-15]: Err
                Stmt _id_ [19-20]: Expr: Expr _id_ [19-20]: Path: Path _id_ [19-20] (Ident _id_ [19-20] "x")

            [
                Error(
                    Rule(
                        "expression",
                        Close(
                            Brace,
                        ),
                        Span {
                            lo: 16,
                            hi: 17,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn recover_statements_before_and_after() {
    check(
        parse_block,
        "{
            let x = 2 + 2;
            let y = Foo(x
            let z = x * 3;
            z
        }",
        &expect![[r#"
            Block _id_ [0-105]:
                Stmt _id_ [14-28]: Local (Immutable):
                    Pat _id_ [18-19]: Bind:
                        Ident _id_ [18-19] "x"
                    Expr _id_ [22-27]: BinOp (Add):
                        Expr _id_ [22-23]: Lit: Int(2)
                        Expr _id_ [26-27]: Lit: Int(2)
                Stmt _id_ [41-81]: Local (Immutable):
                    Pat _id_ [45-46]: Bind:
                        Ident _id_ [45-46] "y"
                    Expr _id_ [49-80]: Err
                Stmt _id_ [94-95]: Expr: Expr _id_ [94-95]: Path: Path _id_ [94-95] (Ident _id_ [94-95] "z")

            [
                Error(
                    Token(
                        Close(
                            Paren,
                        ),
                        Keyword(
                            Let,
                        ),
                        Span {
                            lo: 67,
                            hi: 70,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn recover_missing_semicolon() {
    check(
        parse_block,
        "{
            let x = 2 + 2;
            let y = Foo(x)
            let z = x * 3;
            z
        }",
        &expect![[r#"
            Block _id_ [0-106]:
                Stmt _id_ [14-28]: Local (Immutable):
                    Pat _id_ [18-19]: Bind:
                        Ident _id_ [18-19] "x"
                    Expr _id_ [22-27]: BinOp (Add):
                        Expr _id_ [22-23]: Lit: Int(2)
                        Expr _id_ [26-27]: Lit: Int(2)
                Stmt _id_ [41-55]: Local (Immutable):
                    Pat _id_ [45-46]: Bind:
                        Ident _id_ [45-46] "y"
                    Expr _id_ [49-55]: Call:
                        Expr _id_ [49-52]: Path: Path _id_ [49-52] (Ident _id_ [49-52] "Foo")
                        Expr _id_ [52-55]: Paren: Expr _id_ [53-54]: Path: Path _id_ [53-54] (Ident _id_ [53-54] "x")
                Stmt _id_ [68-82]: Local (Immutable):
                    Pat _id_ [72-73]: Bind:
                        Ident _id_ [72-73] "z"
                    Expr _id_ [76-81]: BinOp (Mul):
                        Expr _id_ [76-77]: Path: Path _id_ [76-77] (Ident _id_ [76-77] "x")
                        Expr _id_ [80-81]: Lit: Int(3)
                Stmt _id_ [95-96]: Expr: Expr _id_ [95-96]: Path: Path _id_ [95-96] (Ident _id_ [95-96] "z")

            [
                Error(
                    Token(
                        Semi,
                        Keyword(
                            Let,
                        ),
                        Span {
                            lo: 68,
                            hi: 71,
                        },
                    ),
                ),
            ]"#]],
    );
}

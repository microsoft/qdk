// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn popcount() {
    let source = r#"
        const bit[5] a = "10101";
        popcount(a);
    "#;

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-34]:
                symbol_id: 8
                ty_span: [15-21]
                ty_exprs:
                    Expr [19-20]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [26-33]:
                    ty: const bit[5]
                    const_value: Bitstring("10101")
                    kind: Lit: Bitstring("10101")
            ExprStmt [43-55]:
                expr: Expr [43-54]:
                    ty: const uint
                    const_value: Int(3)
                    kind: BuiltinFunctionCall [43-54]:
                        fn_name_span: [43-51]
                        name: popcount
                        function_ty: def (const bit[5]) -> const uint
                        args:
                            Expr [52-53]:
                                ty: const bit[5]
                                const_value: Bitstring("10101")
                                kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn popcount_literal() {
    let source = r#"
        popcount("10101");
    "#;

    check_stmt_kinds(
        source,
        &expect![[r#"
            ExprStmt [9-27]:
                expr: Expr [9-26]:
                    ty: const uint
                    const_value: Int(3)
                    kind: BuiltinFunctionCall [9-26]:
                        fn_name_span: [9-17]
                        name: popcount
                        function_ty: def (const bit[5]) -> const uint
                        args:
                            Expr [18-25]:
                                ty: const bit[5]
                                const_value: Bitstring("10101")
                                kind: Lit: Bitstring("10101")
        "#]],
    );
}

#[test]
fn popcount_unsized_type_error() {
    let source = r#"
        popcount(2);
    "#;

    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-21]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `popcount` for inputs: (const int)
              | Overloads available are:
              |     fn popcount(bit[n]) -> uint
               ,-[test:2:9]
             1 | 
             2 |         popcount(2);
               :         ^^^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

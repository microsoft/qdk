// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn rotr_bitarray() {
    let source = r#"
        const bit[5] a = "10001";
        rotr(a, 1);
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
                    const_value: Bitstring("10001")
                    kind: Lit: Bitstring("10001")
            ExprStmt [43-54]:
                expr: Expr [43-53]:
                    ty: const bit[5]
                    const_value: Bitstring("11000")
                    kind: BuiltinFunctionCall [43-53]:
                        fn_name_span: [43-47]
                        name: rotr
                        function_ty: def (const bit[5], const int) -> const bit[5]
                        args:
                            Expr [48-49]:
                                ty: const bit[5]
                                const_value: Bitstring("10001")
                                kind: SymbolId(8)
                            Expr [51-52]:
                                ty: const int
                                const_value: Int(1)
                                kind: Lit: Int(1)
        "#]],
    );
}

#[test]
fn rotr_bitarray_negative_distance() {
    let source = r#"
        const bit[5] a = "10001";
        rotr(a, -1);
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
                    const_value: Bitstring("10001")
                    kind: Lit: Bitstring("10001")
            ExprStmt [43-55]:
                expr: Expr [43-54]:
                    ty: const bit[5]
                    const_value: Bitstring("00011")
                    kind: BuiltinFunctionCall [43-54]:
                        fn_name_span: [43-47]
                        name: rotr
                        function_ty: def (const bit[5], const int) -> const bit[5]
                        args:
                            Expr [48-49]:
                                ty: const bit[5]
                                const_value: Bitstring("10001")
                                kind: SymbolId(8)
                            Expr [52-53]:
                                ty: const int
                                const_value: Int(-1)
                                kind: UnaryOpExpr [52-53]:
                                    op: Neg
                                    expr: Expr [52-53]:
                                        ty: const int
                                        kind: Lit: Int(1)
        "#]],
    );
}

#[test]
fn rotr_bitarray_zero_edge_case() {
    let source = r#"
        const bit[5] a = "10001";
        rotr(a, 0);
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
                    const_value: Bitstring("10001")
                    kind: Lit: Bitstring("10001")
            ExprStmt [43-54]:
                expr: Expr [43-53]:
                    ty: const bit[5]
                    const_value: Bitstring("10001")
                    kind: BuiltinFunctionCall [43-53]:
                        fn_name_span: [43-47]
                        name: rotr
                        function_ty: def (const bit[5], const int) -> const bit[5]
                        args:
                            Expr [48-49]:
                                ty: const bit[5]
                                const_value: Bitstring("10001")
                                kind: SymbolId(8)
                            Expr [51-52]:
                                ty: const int
                                const_value: Int(0)
                                kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn rotr_uint() {
    let source = r#"
        const uint[5] a = 17;
        rotr(a, 1);
    "#;

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-30]:
                symbol_id: 8
                ty_span: [15-22]
                ty_exprs:
                    Expr [20-21]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [27-29]:
                    ty: const uint[5]
                    const_value: Int(17)
                    kind: Lit: Int(17)
            ExprStmt [39-50]:
                expr: Expr [39-49]:
                    ty: const bit[5]
                    const_value: Bitstring("11000")
                    kind: BuiltinFunctionCall [39-49]:
                        fn_name_span: [39-43]
                        name: rotr
                        function_ty: def (const bit[5], const int) -> const bit[5]
                        args:
                            Expr [44-45]:
                                ty: const uint[5]
                                const_value: Int(17)
                                kind: SymbolId(8)
                            Expr [47-48]:
                                ty: const int
                                const_value: Int(1)
                                kind: Lit: Int(1)
        "#]],
    );
}

#[test]
fn rotr_uint_negative_distance() {
    let source = r#"
        const uint[5] a = 17;
        rotr(a, -1);
    "#;

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-30]:
                symbol_id: 8
                ty_span: [15-22]
                ty_exprs:
                    Expr [20-21]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [27-29]:
                    ty: const uint[5]
                    const_value: Int(17)
                    kind: Lit: Int(17)
            ExprStmt [39-51]:
                expr: Expr [39-50]:
                    ty: const bit[5]
                    const_value: Bitstring("00011")
                    kind: BuiltinFunctionCall [39-50]:
                        fn_name_span: [39-43]
                        name: rotr
                        function_ty: def (const bit[5], const int) -> const bit[5]
                        args:
                            Expr [44-45]:
                                ty: const uint[5]
                                const_value: Int(17)
                                kind: SymbolId(8)
                            Expr [48-49]:
                                ty: const int
                                const_value: Int(-1)
                                kind: UnaryOpExpr [48-49]:
                                    op: Neg
                                    expr: Expr [48-49]:
                                        ty: const int
                                        kind: Lit: Int(1)
        "#]],
    );
}

#[test]
fn rotr_uint_zero_edge_case() {
    let source = r#"
        const uint[5] a = 17;
        rotr(a, 0);
    "#;

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-30]:
                symbol_id: 8
                ty_span: [15-22]
                ty_exprs:
                    Expr [20-21]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [27-29]:
                    ty: const uint[5]
                    const_value: Int(17)
                    kind: Lit: Int(17)
            ExprStmt [39-50]:
                expr: Expr [39-49]:
                    ty: const bit[5]
                    const_value: Bitstring("10001")
                    kind: BuiltinFunctionCall [39-49]:
                        fn_name_span: [39-43]
                        name: rotr
                        function_ty: def (const bit[5], const int) -> const bit[5]
                        args:
                            Expr [44-45]:
                                ty: const uint[5]
                                const_value: Int(17)
                                kind: SymbolId(8)
                            Expr [47-48]:
                                ty: const int
                                const_value: Int(0)
                                kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn rotr_unsized_type_error() {
    let source = r#"
        rotr(17, 2);
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

              x There is no valid overload of `rotr` for inputs: (const int, const int)
              | Overloads available are:
              |     fn rotr(bit[n], int) -> bit[n]
              |     fn rotr(uint[n], int) -> uint[n]
               ,-[test:2:9]
             1 | 
             2 |         rotr(17, 2);
               :         ^^^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod array;
mod dyn_array_ref;
mod static_array_ref;

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds as check;
use expect_test::expect;

#[test]
fn concatenating_static_and_dynamic_array_refs_errors() {
    let source = "
    def f(readonly array[int, 3] a, readonly array[int, #dim = 1] b, mutable array[int, #dim = 1] c) {
        c = a ++ b;
    }
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [5-129]:
                        annotations: <empty>
                        kind: DefStmt [5-129]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [34-35]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [31-32]:
                                            ty: const uint
                                            const_value: Int(3)
                                            kind: Lit: Int(3)
                                DefParameter [67-68]:
                                    symbol_id: 10
                                    ty_exprs:
                                        Expr [64-65]:
                                            ty: const uint
                                            const_value: Int(1)
                                            kind: Lit: Int(1)
                                DefParameter [99-100]:
                                    symbol_id: 11
                                    ty_exprs:
                                        Expr [96-97]:
                                            ty: const uint
                                            const_value: Int(1)
                                            kind: Lit: Int(1)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [102-129]:
                                Stmt [112-123]:
                                    annotations: <empty>
                                    kind: AssignStmt [112-123]:
                                        lhs: Expr [112-113]:
                                            ty: mutable array[int, #dim = 1]
                                            kind: SymbolId(11)
                                        rhs: Expr [116-122]:
                                            ty: unknown
                                            kind: ConcatExpr [116-122]:
                                                operands:
                                                    Expr [116-117]:
                                                        ty: readonly array[int, 3]
                                                        kind: SymbolId(9)
                                                    Expr [121-122]:
                                                        ty: readonly array[int, #dim = 1]
                                                        kind: SymbolId(10)

            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: readonly array[int,
              | 3], readonly array[int, #dim = 1]
               ,-[test:3:13]
             2 |     def f(readonly array[int, 3] a, readonly array[int, #dim = 1] b, mutable array[int, #dim = 1] c) {
             3 |         c = a ++ b;
               :             ^^^^^^
             4 |     }
               `----
            ]"#]],
    );
}

#[test]
fn concantenation_in_assign_op_errors() {
    let source = "
    array[int, 3] a;
    array[int, 6] b;
    b += a ++ a;
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [5-21]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [5-21]:
                            symbol_id: 8
                            ty_span: [5-18]
                            ty_exprs:
                                Expr [16-17]:
                                    ty: const uint
                                    const_value: Int(3)
                                    kind: Lit: Int(3)
                            init_expr: Expr [5-21]:
                                ty: array[int, 3]
                                kind: Lit:     array:
                                        Expr [5-21]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [5-21]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [5-21]:
                                            ty: const int
                                            kind: Lit: Int(0)
                    Stmt [26-42]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [26-42]:
                            symbol_id: 9
                            ty_span: [26-39]
                            ty_exprs:
                                Expr [37-38]:
                                    ty: const uint
                                    const_value: Int(6)
                                    kind: Lit: Int(6)
                            init_expr: Expr [26-42]:
                                ty: array[int, 6]
                                kind: Lit:     array:
                                        Expr [26-42]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [26-42]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [26-42]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [26-42]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [26-42]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [26-42]:
                                            ty: const int
                                            kind: Lit: Int(0)
                    Stmt [47-59]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.InvalidConcatenationPosition

              x concatenation expressions are not allowed in the rhs of assignment
              | operation statements
               ,-[test:4:10]
             3 |     array[int, 6] b;
             4 |     b += a ++ a;
               :          ^^^^^^
             5 |     
               `----
            ]"#]],
    );
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod alias;
mod angle;
mod bit;
mod bool;
mod complex;
mod creg;
mod duration;
mod extern_decl;
mod float;
mod int;
mod qreg;
mod qubit;
mod stretch;
mod uint;

use expect_test::expect;

use super::check;

#[test]
fn duration_and_stretch_types_without_init_exprs() {
    check(
        r#"
        duration i;
        stretch n;
        "#,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-39]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [29-39]:
                            symbol_id: 9
                            ty_span: [29-36]
                            ty_exprs: <empty>
                            init_expr: Expr [29-39]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)"#]],
    );
}

#[test]
fn scalar_ty_designator_must_be_positive() {
    check(
        "int[-5] i;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-10]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-10]:
                            symbol_id: 8
                            ty_span: [0-7]
                            ty_exprs: <empty>
                            init_expr: Expr [0-0]:
                                ty: unknown
                                kind: Err

            [Qasm.Lowerer.ExprMustBePositiveInt

              x type width must be a positive integer
               ,-[test:1:5]
             1 | int[-5] i;
               :     ^^
               `----
            ]"#]],
    );
}

#[test]
fn scalar_ty_designator_must_be_castable_to_const_int() {
    check(
        r#"const angle size = 2.0; int[size] i;"#,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-23]:
                            symbol_id: 8
                            ty_span: [6-11]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                const_value: Angle(2.0000000000000004)
                                kind: Lit: Angle(2.0000000000000004)
                    Stmt [24-36]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [24-36]:
                            symbol_id: 9
                            ty_span: [24-33]
                            ty_exprs: <empty>
                            init_expr: Expr [0-0]:
                                ty: unknown
                                kind: Err

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type const angle to type const uint
               ,-[test:1:29]
             1 | const angle size = 2.0; int[size] i;
               :                             ^^^^
               `----
            , Qasm.Lowerer.ExprMustBeInt

              x type width must be an integer
               ,-[test:1:29]
             1 | const angle size = 2.0; int[size] i;
               :                             ^^^^
               `----
            ]"#]],
    );
}

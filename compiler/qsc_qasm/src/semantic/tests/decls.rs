// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

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
mod stretch;
mod uint;

use expect_test::expect;

use super::check;

#[test]
#[ignore = "Not yet implemented"]
fn duration_and_stretch_types_without_init_exprs() {
    check(
        r#"
        duration i;
        stretch n;
        "#,
        &expect![[r#"


            [Qasm.Compile.NotSupported

              x Duration type values are not supported.
               ,-[test:2:9]
             1 |
             2 |         duration i;
               :         ^^^^^^^^
             3 |         stretch n;
               `----
            , Qasm.Compile.NotSupported

              x Stretch type values are not supported.
               ,-[test:3:9]
             2 |         duration i;
             3 |         stretch n;
               :         ^^^^^^^
             4 |
               `----
            ]"#]],
    );
}

#[test]
fn scalar_ty_designator_must_be_positive() {
    check(
        "int[-5] i;",
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [0-10]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-10]:
                            symbol_id: 8
                            ty_span: [0-7]
                            init_expr: Expr [0-0]:
                                ty: Err
                                kind: Err

            [Qasm.Lowerer.TypeWidthMustBePositiveIntConstExpr

              x type width must be a positive integer const expression
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
                statements:
                    Stmt [0-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-23]:
                            symbol_id: 8
                            ty_span: [6-11]
                            init_expr: Expr [19-22]:
                                ty: Angle(None, true)
                                kind: Lit: Angle(2.0000000000000004)
                    Stmt [24-36]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [24-36]:
                            symbol_id: 9
                            ty_span: [24-33]
                            init_expr: Expr [0-0]:
                                ty: Err
                                kind: Err

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type Angle(None, true) to type UInt(None, true)
               ,-[test:1:29]
             1 | const angle size = 2.0; int[size] i;
               :                             ^^^^
               `----
            , Qasm.Lowerer.TypeWidthMustBePositiveIntConstExpr

              x type width must be a positive integer const expression
               ,-[test:1:29]
             1 | const angle size = 2.0; int[size] i;
               :                             ^^^^
               `----
            ]"#]],
    );
}

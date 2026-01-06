// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::{check_err, check_last_stmt as check};
use expect_test::expect;

#[test]
fn sizeof_no_args_errors() {
    let source = "
        const uint arr_size = sizeof();
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-40]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-40]:
                            symbol_id: 8
                            ty_span: [15-19]
                            ty_exprs: <empty>
                            init_expr: Expr [31-39]:
                                ty: unknown
                                kind: Err

            [Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `sizeof` for inputs: ()
              | Overloads available are:
              |     fn sizeof(array[_, ...], const uint) -> const uint
              |     fn sizeof(array[_, #dim = _], uint) -> uint
               ,-[test:2:31]
             1 | 
             2 |         const uint arr_size = sizeof();
               :                               ^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn sizeof_too_many_args_errors() {
    let source = "
        const uint arr_size = sizeof(1, 2, 3);
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-47]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-47]:
                            symbol_id: 8
                            ty_span: [15-19]
                            ty_exprs: <empty>
                            init_expr: Expr [31-46]:
                                ty: unknown
                                kind: Err

            [Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `sizeof` for inputs: (const int, const int,
              | const int)
              | Overloads available are:
              |     fn sizeof(array[_, ...], const uint) -> const uint
              |     fn sizeof(array[_, #dim = _], uint) -> uint
               ,-[test:2:31]
             1 | 
             2 |         const uint arr_size = sizeof(1, 2, 3);
               :                               ^^^^^^^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn sizeof_non_array_errors() {
    let source = "
        const uint arr_size = sizeof(1);
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-41]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-41]:
                            symbol_id: 8
                            ty_span: [15-19]
                            ty_exprs: <empty>
                            init_expr: Expr [31-40]:
                                ty: unknown
                                kind: Err

            [Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `sizeof` for inputs: (const int)
              | Overloads available are:
              |     fn sizeof(array[_, ...], const uint) -> const uint
              |     fn sizeof(array[_, #dim = _], uint) -> uint
               ,-[test:2:31]
             1 | 
             2 |         const uint arr_size = sizeof(1);
               :                               ^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn sizeof_array() {
    let source = "
        array[bool, 3, 4] arr;
        const uint arr_size = sizeof(arr, 1);
    ";

    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [40-77]:
                symbol_id: 9
                ty_span: [46-50]
                ty_exprs: <empty>
                init_expr: Expr [62-76]:
                    ty: const uint
                    const_value: Int(4)
                    kind: Lit: Int(4)"#]],
    );
}

#[test]
fn sizeof_array_omitted_dimension() {
    let source = "
        array[bool, 3, 4] arr;
        const uint arr_size = sizeof(arr);
    ";

    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [40-74]:
                symbol_id: 9
                ty_span: [46-50]
                ty_exprs: <empty>
                init_expr: Expr [62-73]:
                    ty: const uint
                    const_value: Int(3)
                    kind: Lit: Int(3)"#]],
    );
}

#[test]
fn sizeof_array_invalid_dimension_errors() {
    let source = "
        array[bool, 3, 4] arr;
        const uint arr_size = sizeof(arr, 2);
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-31]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-31]:
                            symbol_id: 8
                            ty_span: [9-26]
                            ty_exprs:
                                Expr [21-22]:
                                    ty: const uint
                                    const_value: Int(3)
                                    kind: Lit: Int(3)
                                Expr [24-25]:
                                    ty: const uint
                                    const_value: Int(4)
                                    kind: Lit: Int(4)
                            init_expr: Expr [9-31]:
                                ty: array[bool, 3, 4]
                                kind: Lit:     array:
                                        Expr [0-0]:
                                            ty: array[bool, 4]
                                            kind: Lit:     array:
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                        Expr [0-0]:
                                            ty: array[bool, 4]
                                            kind: Lit:     array:
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                        Expr [0-0]:
                                            ty: array[bool, 4]
                                            kind: Lit:     array:
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                                                    Expr [9-31]:
                                                        ty: const bool
                                                        kind: Lit: Bool(false)
                    Stmt [40-77]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [40-77]:
                            symbol_id: 9
                            ty_span: [46-50]
                            ty_exprs: <empty>
                            init_expr: Expr [62-76]:
                                ty: unknown
                                kind: Err

            [Qasm.Lowerer.SizeofInvalidDimension

              x requested dimension 2 but array has 2 dimensions
               ,-[test:3:31]
             2 |         array[bool, 3, 4] arr;
             3 |         const uint arr_size = sizeof(arr, 2);
               :                               ^^^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sizeof_static_array_ref() {
    let source = "
        array[bool, 3, 4] arr;

        def f(readonly array[bool, 3, 4] a) {
            const uint arr_size = sizeof(a, 1);
        }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [41-136]:
                symbol_id: 9
                has_qubit_params: false
                parameters:
                    DefParameter [74-75]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [68-69]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                            Expr [71-72]:
                                ty: const uint
                                const_value: Int(4)
                                kind: Lit: Int(4)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [77-136]:
                    Stmt [91-126]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [91-126]:
                            symbol_id: 11
                            ty_span: [97-101]
                            ty_exprs: <empty>
                            init_expr: Expr [113-125]:
                                ty: const uint
                                const_value: Int(4)
                                kind: Lit: Int(4)"#]],
    );
}

#[test]
fn sizeof_static_array_ref_omitted_dimension() {
    let source = "
        array[bool, 3, 4] arr;

        def f(readonly array[bool, 3, 4] a) {
            const uint arr_size = sizeof(a);
        }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [41-133]:
                symbol_id: 9
                has_qubit_params: false
                parameters:
                    DefParameter [74-75]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [68-69]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                            Expr [71-72]:
                                ty: const uint
                                const_value: Int(4)
                                kind: Lit: Int(4)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [77-133]:
                    Stmt [91-123]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [91-123]:
                            symbol_id: 11
                            ty_span: [97-101]
                            ty_exprs: <empty>
                            init_expr: Expr [113-122]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)"#]],
    );
}

#[test]
fn sizeof_static_array_ref_invalid_dimension_errors() {
    let source = "
        array[bool, 3, 4] arr;

        def f(readonly array[bool, 3, 4] a) {
            const uint arr_size = sizeof(a, 2);
        }
    ";

    check_err(
        source,
        &expect![[r#"
            [Qasm.Lowerer.SizeofInvalidDimension

              x requested dimension 2 but array has 2 dimensions
               ,-[test:5:35]
             4 |         def f(readonly array[bool, 3, 4] a) {
             5 |             const uint arr_size = sizeof(a, 2);
               :                                   ^^^^^^^^^^^^
             6 |         }
               `----
            ]"#]],
    );
}

#[test]
fn sizeof_dyn_array_ref() {
    let source = "
        array[bool, 3, 4] arr;

        def f(readonly array[bool, #dim = 2] a) {
            uint arr_size = sizeof(a, 1);
        }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [41-134]:
                symbol_id: 9
                has_qubit_params: false
                parameters:
                    DefParameter [78-79]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [75-76]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [81-134]:
                    Stmt [95-124]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [95-124]:
                            symbol_id: 11
                            ty_span: [95-99]
                            ty_exprs: <empty>
                            init_expr: Expr [111-123]:
                                ty: uint
                                kind: SizeofCallExpr [111-123]:
                                    fn_name_span: [111-117]
                                    array: Expr [118-119]:
                                        ty: readonly array[bool, #dim = 2]
                                        kind: SymbolId(10)
                                    array_dims: 2
                                    dim: Expr [121-122]:
                                        ty: const int
                                        kind: Lit: Int(1)"#]],
    );
}

#[test]
fn sizeof_dyn_array_ref_omitted_dimension() {
    let source = "
        array[bool, 3, 4] arr;

        def f(readonly array[bool, #dim = 2] a) {
            uint arr_size = sizeof(a);
        }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [41-131]:
                symbol_id: 9
                has_qubit_params: false
                parameters:
                    DefParameter [78-79]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [75-76]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [81-131]:
                    Stmt [95-121]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [95-121]:
                            symbol_id: 11
                            ty_span: [95-99]
                            ty_exprs: <empty>
                            init_expr: Expr [111-120]:
                                ty: uint
                                kind: SizeofCallExpr [111-120]:
                                    fn_name_span: [111-117]
                                    array: Expr [118-119]:
                                        ty: readonly array[bool, #dim = 2]
                                        kind: SymbolId(10)
                                    array_dims: 2
                                    dim: Expr [111-120]:
                                        ty: const uint
                                        const_value: Int(0)
                                        kind: Lit: Int(0)"#]],
    );
}

// This is a runtime failure, so it lowers and compiles without errors.
#[test]
fn sizeof_dyn_array_ref_invalid_dimension_lowers_correctly() {
    let source = "
        array[bool, 3, 4] arr;

        def f(readonly array[bool, #dim = 2] a) {
            uint arr_size = sizeof(a, 2);
        }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [41-134]:
                symbol_id: 9
                has_qubit_params: false
                parameters:
                    DefParameter [78-79]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [75-76]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [81-134]:
                    Stmt [95-124]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [95-124]:
                            symbol_id: 11
                            ty_span: [95-99]
                            ty_exprs: <empty>
                            init_expr: Expr [111-123]:
                                ty: uint
                                kind: SizeofCallExpr [111-123]:
                                    fn_name_span: [111-117]
                                    array: Expr [118-119]:
                                        ty: readonly array[bool, #dim = 2]
                                        kind: SymbolId(10)
                                    array_dims: 2
                                    dim: Expr [121-122]:
                                        ty: const int
                                        kind: Lit: Int(2)"#]],
    );
}

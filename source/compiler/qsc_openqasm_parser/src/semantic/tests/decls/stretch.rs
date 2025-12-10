// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::{check_classical_decl, check_classical_decls};

#[test]
fn with_no_init_expr_has_generated_lit_expr() {
    check_classical_decl(
        "stretch a;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-10]:
                symbol_id: 8
                ty_span: [0-7]
                ty_exprs: <empty>
                init_expr: Expr [0-10]:
                    ty: stretch
                    const_value: Duration(0.0 s)
                    kind: Lit: Duration(0.0 s)
            [8] Symbol [8-9]:
                name: a
                type: stretch
                ty_span: [0-7]
                io_kind: Default"#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn spec_sample() {
    check_classical_decls(
        r#"
            include "stdgates.inc";
            const duration a = 10ns;
            const duration b = durationof({x [100ns] $0;});
            stretch c;
            // stretchy duration with min=100ns
            stretch d = a + 2 * c;
            // stretchy duration with backtracking by up to half b
            stretch e = -0.5 * b + c;
        "#,
        &expect![[r#"
            ClassicalDeclarationStmt [49-73]:
                symbol_id: 40
                ty_span: [55-63]
                ty_exprs: <empty>
                init_expr: Expr [68-72]:
                    ty: const duration
                    const_value: Duration(10.0 ns)
                    kind: Lit: Duration(10.0 ns)
            [40] Symbol [64-65]:
                name: a
                type: const duration
                ty_span: [55-63]
                io_kind: Default
            ClassicalDeclarationStmt [86-133]:
                symbol_id: 41
                ty_span: [92-100]
                ty_exprs: <empty>
                init_expr: Expr [105-132]:
                    ty: const duration
                    const_value: Duration(100.0 ns)
                    kind: DurationofCallExpr [105-132]:
                        fn_name_span: [105-115]
                        duration: 100.0 ns
                        scope: Block [116-131]:
                            Stmt [117-130]:
                                annotations: <empty>
                                kind: GateCall [117-130]:
                                    modifiers: <empty>
                                    symbol_id: 9
                                    gate_name_span: [117-118]
                                    args: <empty>
                                    qubits:
                                        GateOperand [127-129]:
                                            kind: HardwareQubit [127-129]: 0
                                    duration: Expr [120-125]:
                                        ty: const duration
                                        const_value: Duration(100.0 ns)
                                        kind: Lit: Duration(100.0 ns)
                                    classical_arity: 0
                                    quantum_arity: 1
            [41] Symbol [101-102]:
                name: b
                type: const duration
                ty_span: [92-100]
                io_kind: Default
            ClassicalDeclarationStmt [146-156]:
                symbol_id: 42
                ty_span: [146-153]
                ty_exprs: <empty>
                init_expr: Expr [146-156]:
                    ty: stretch
                    const_value: Duration(0.0 s)
                    kind: Lit: Duration(0.0 s)
            [42] Symbol [154-155]:
                name: c
                type: stretch
                ty_span: [146-153]
                io_kind: Default
            ClassicalDeclarationStmt [217-239]:
                symbol_id: 43
                ty_span: [217-224]
                ty_exprs: <empty>
                init_expr: Expr [229-238]:
                    ty: stretch
                    const_value: Duration(10.0 ns)
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [229-230]:
                            ty: const duration
                            kind: SymbolId(40)
                        rhs: Expr [233-238]:
                            ty: const duration
                            const_value: Duration(0.0 s)
                            kind: BinaryOpExpr:
                                op: Mul
                                lhs: Expr [233-234]:
                                    ty: const int
                                    kind: Lit: Int(2)
                                rhs: Expr [237-238]:
                                    ty: stretch
                                    kind: SymbolId(42)
            [43] Symbol [225-226]:
                name: d
                type: stretch
                ty_span: [217-224]
                io_kind: Default
            ClassicalDeclarationStmt [319-344]:
                symbol_id: 44
                ty_span: [319-326]
                ty_exprs: <empty>
                init_expr: Expr [331-343]:
                    ty: stretch
                    const_value: Duration(-50.0 ns)
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [331-339]:
                            ty: const duration
                            const_value: Duration(-50.0 ns)
                            kind: BinaryOpExpr:
                                op: Mul
                                lhs: Expr [332-335]:
                                    ty: const float
                                    kind: UnaryOpExpr [332-335]:
                                        op: Neg
                                        expr: Expr [332-335]:
                                            ty: const float
                                            kind: Lit: Float(0.5)
                                rhs: Expr [338-339]:
                                    ty: const duration
                                    kind: SymbolId(41)
                        rhs: Expr [342-343]:
                            ty: stretch
                            kind: SymbolId(42)
            [44] Symbol [327-328]:
                name: e
                type: stretch
                ty_span: [319-326]
                io_kind: Default
        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn spec_sample_2() {
    check_classical_decls(
        r#"
            include "stdgates.inc";
            stretch a;
            stretch b;
            duration start_stretch = a - .5 * durationof({x [10ns] $0;});
            duration middle_stretch = a - .5 * durationof({x [10ns] $0;}) - .5 * durationof({y [100ns] $0;});
            duration end_stretch = a - .5 * durationof({y [1000ns] $0;});
        "#,
        &expect![[r#"
            ClassicalDeclarationStmt [49-59]:
                symbol_id: 40
                ty_span: [49-56]
                ty_exprs: <empty>
                init_expr: Expr [49-59]:
                    ty: stretch
                    const_value: Duration(0.0 s)
                    kind: Lit: Duration(0.0 s)
            [40] Symbol [57-58]:
                name: a
                type: stretch
                ty_span: [49-56]
                io_kind: Default
            ClassicalDeclarationStmt [72-82]:
                symbol_id: 41
                ty_span: [72-79]
                ty_exprs: <empty>
                init_expr: Expr [72-82]:
                    ty: stretch
                    const_value: Duration(0.0 s)
                    kind: Lit: Duration(0.0 s)
            [41] Symbol [80-81]:
                name: b
                type: stretch
                ty_span: [72-79]
                io_kind: Default
            ClassicalDeclarationStmt [95-156]:
                symbol_id: 42
                ty_span: [95-103]
                ty_exprs: <empty>
                init_expr: Expr [120-155]:
                    ty: const duration
                    const_value: Duration(-5.0 ns)
                    kind: BinaryOpExpr:
                        op: Sub
                        lhs: Expr [120-121]:
                            ty: stretch
                            kind: SymbolId(40)
                        rhs: Expr [124-155]:
                            ty: const duration
                            const_value: Duration(5.0 ns)
                            kind: BinaryOpExpr:
                                op: Mul
                                lhs: Expr [124-126]:
                                    ty: const float
                                    kind: Lit: Float(0.5)
                                rhs: Expr [129-155]:
                                    ty: const duration
                                    kind: DurationofCallExpr [129-155]:
                                        fn_name_span: [129-139]
                                        duration: 10.0 ns
                                        scope: Block [140-154]:
                                            Stmt [141-153]:
                                                annotations: <empty>
                                                kind: GateCall [141-153]:
                                                    modifiers: <empty>
                                                    symbol_id: 9
                                                    gate_name_span: [141-142]
                                                    args: <empty>
                                                    qubits:
                                                        GateOperand [150-152]:
                                                            kind: HardwareQubit [150-152]: 0
                                                    duration: Expr [144-148]:
                                                        ty: const duration
                                                        const_value: Duration(10.0 ns)
                                                        kind: Lit: Duration(10.0 ns)
                                                    classical_arity: 0
                                                    quantum_arity: 1
            [42] Symbol [104-117]:
                name: start_stretch
                type: duration
                ty_span: [95-103]
                io_kind: Default
            ClassicalDeclarationStmt [169-266]:
                symbol_id: 43
                ty_span: [169-177]
                ty_exprs: <empty>
                init_expr: Expr [195-265]:
                    ty: const duration
                    const_value: Duration(-55.0 ns)
                    kind: BinaryOpExpr:
                        op: Sub
                        lhs: Expr [195-230]:
                            ty: const duration
                            const_value: Duration(-5.0 ns)
                            kind: BinaryOpExpr:
                                op: Sub
                                lhs: Expr [195-196]:
                                    ty: stretch
                                    kind: SymbolId(40)
                                rhs: Expr [199-230]:
                                    ty: const duration
                                    const_value: Duration(5.0 ns)
                                    kind: BinaryOpExpr:
                                        op: Mul
                                        lhs: Expr [199-201]:
                                            ty: const float
                                            kind: Lit: Float(0.5)
                                        rhs: Expr [204-230]:
                                            ty: const duration
                                            kind: DurationofCallExpr [204-230]:
                                                fn_name_span: [204-214]
                                                duration: 10.0 ns
                                                scope: Block [215-229]:
                                                    Stmt [216-228]:
                                                        annotations: <empty>
                                                        kind: GateCall [216-228]:
                                                            modifiers: <empty>
                                                            symbol_id: 9
                                                            gate_name_span: [216-217]
                                                            args: <empty>
                                                            qubits:
                                                                GateOperand [225-227]:
                                                                    kind: HardwareQubit [225-227]: 0
                                                            duration: Expr [219-223]:
                                                                ty: const duration
                                                                const_value: Duration(10.0 ns)
                                                                kind: Lit: Duration(10.0 ns)
                                                            classical_arity: 0
                                                            quantum_arity: 1
                        rhs: Expr [233-265]:
                            ty: const duration
                            const_value: Duration(50.0 ns)
                            kind: BinaryOpExpr:
                                op: Mul
                                lhs: Expr [233-235]:
                                    ty: const float
                                    kind: Lit: Float(0.5)
                                rhs: Expr [238-265]:
                                    ty: const duration
                                    kind: DurationofCallExpr [238-265]:
                                        fn_name_span: [238-248]
                                        duration: 100.0 ns
                                        scope: Block [249-264]:
                                            Stmt [250-263]:
                                                annotations: <empty>
                                                kind: GateCall [250-263]:
                                                    modifiers: <empty>
                                                    symbol_id: 10
                                                    gate_name_span: [250-251]
                                                    args: <empty>
                                                    qubits:
                                                        GateOperand [260-262]:
                                                            kind: HardwareQubit [260-262]: 0
                                                    duration: Expr [253-258]:
                                                        ty: const duration
                                                        const_value: Duration(100.0 ns)
                                                        kind: Lit: Duration(100.0 ns)
                                                    classical_arity: 0
                                                    quantum_arity: 1
            [43] Symbol [178-192]:
                name: middle_stretch
                type: duration
                ty_span: [169-177]
                io_kind: Default
            ClassicalDeclarationStmt [279-340]:
                symbol_id: 44
                ty_span: [279-287]
                ty_exprs: <empty>
                init_expr: Expr [302-339]:
                    ty: const duration
                    const_value: Duration(-500.0 ns)
                    kind: BinaryOpExpr:
                        op: Sub
                        lhs: Expr [302-303]:
                            ty: stretch
                            kind: SymbolId(40)
                        rhs: Expr [306-339]:
                            ty: const duration
                            const_value: Duration(500.0 ns)
                            kind: BinaryOpExpr:
                                op: Mul
                                lhs: Expr [306-308]:
                                    ty: const float
                                    kind: Lit: Float(0.5)
                                rhs: Expr [311-339]:
                                    ty: const duration
                                    kind: DurationofCallExpr [311-339]:
                                        fn_name_span: [311-321]
                                        duration: 1000.0 ns
                                        scope: Block [322-338]:
                                            Stmt [323-337]:
                                                annotations: <empty>
                                                kind: GateCall [323-337]:
                                                    modifiers: <empty>
                                                    symbol_id: 10
                                                    gate_name_span: [323-324]
                                                    args: <empty>
                                                    qubits:
                                                        GateOperand [334-336]:
                                                            kind: HardwareQubit [334-336]: 0
                                                    duration: Expr [326-332]:
                                                        ty: const duration
                                                        const_value: Duration(1000.0 ns)
                                                        kind: Lit: Duration(1000.0 ns)
                                                    classical_arity: 0
                                                    quantum_arity: 1
            [44] Symbol [288-299]:
                name: end_stretch
                type: duration
                ty_span: [279-287]
                io_kind: Default
        "#]],
    );
}

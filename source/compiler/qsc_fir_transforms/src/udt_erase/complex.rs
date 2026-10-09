// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowers selected core `Complex` operators to arithmetic on scalar components
//! as part of UDT erasure.
//!
//! The evaluator recognizes Complex arithmetic by the value's nominal identity.
//! Erasing that identity without rewriting the operator would leave, for example,
//! an addition of two ordinary tuples rather than addition of complex numbers.
//! This module preserves the operation's meaning without teaching downstream
//! consumers to interpret every `(Double, Double)` tuple as Complex.
//!
//! # Pipeline contract
//!
//! [`find_values`] records Complex expression IDs before the package's types
//! change. The owning UDT pass then visits expressions in structural postorder
//! and calls [`lower`]: operands have already been erased, but the saved IDs
//! still distinguish complex operands from real-valued operands. This is an
//! internal part of that traversal, not an independently ordered pass.
//!
//! [`lower`] stores operands in their original evaluation order, computes each
//! component from those saved values, and replaces the operation with a block.
//! All synthesized expressions and statements use the shared FIR builders and
//! their empty execution-graph ranges; the pipeline rebuilds those graphs later.
//!
//! # Transformation
//!
//! Simplified FIR notation below uses `.0` and `.1` for real and imaginary
//! tuple-field reads. `L()` and `R()` denote already-erased operand expressions:
//!
//! ```text
//! // Before: the result was nominally Complex.
//! L() * R()
//!
//! // After: evaluate each operand once, left before right.
//! {
//!     let left = L();
//!     let right = R();
//!     (left.0 * right.0 - left.1 * right.1,
//!      left.0 * right.1 + left.1 * right.0)
//! }
//! ```
//!
//! # Supported boundary
//!
//! Covers binary `+`, `-`, `*`, their compound assignments, and unary `+`/`-`.
//! Mixed Double/Complex addition and subtraction accepted by the frontend are
//! handled without inventing an imaginary field for the Double operand.
//! Multiplication requires two Complex operands. Division, exponentiation and
//! other operators are left to their existing paths, not approximated here.
//! Ordinary tuples and user-defined types with the same shape do not qualify.

use crate::fir_builder::{
    alloc_assign_expr, alloc_bin_op_expr, alloc_block, alloc_expr, alloc_expr_stmt,
    alloc_field_expr, alloc_local_var, alloc_local_var_expr, alloc_tuple_expr,
};
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BinOp, ExprId, ExprKind, LocalVarId, Mutability, Package, PackageLookup, PackageSpan, Res,
        StmtId, StoreItemId, UnOp,
    },
    ty::{Prim, Ty},
};
use rustc_hash::FxHashSet;

/// Records expressions whose original type is exactly the core Complex UDT.
///
/// Call before erasing any expression types in this package. The result is a
/// package-local identity snapshot, not a test of the erased tuple shape:
/// `Expr(id, Complex)` becomes `Expr(id, (Double, Double))` during erasure, but
/// `id` remains in this set. Arrays or other UDTs containing Complex are not
/// themselves Complex values; their constituent expressions are tracked
/// individually. This function does not mutate the package.
pub(super) fn find_values(package: &Package) -> FxHashSet<ExprId> {
    package
        .exprs
        .iter()
        .filter_map(|(id, expr)| {
            matches!(expr.ty, Ty::Udt(Res::Item(item))
            if StoreItemId::from((item.package, item.item)) == StoreItemId::complex())
            .then_some(id)
        })
        .collect()
}

/// Replaces one supported Complex operation with an erased, value-preserving
/// block, or leaves the expression unchanged when it does not qualify.
///
/// Binary and unary expressions qualify by their original result type.
/// Compound assignments instead qualify by the original left-hand operand
/// type because the assignment expression itself returns Unit.
///
/// # Before / after
///
/// With erased operands and `.0`/`.1` denoting tuple-field reads:
///
/// ```text
/// // Before: ordinary addition produces a Complex value.
/// lhs + rhs
/// // After:
/// { let left = lhs; let right = rhs;
///   (left.0 + right.0, left.1 + right.1) }
///
/// // Before: a compound assignment produces Unit.
/// target *= rhs
/// // After:
/// { let left = target; let right = rhs;
///   target = (left.0 * right.0 - left.1 * right.1,
///             left.0 * right.1 + left.1 * right.0) }
///
/// // Before / after: unary negation.
/// -operand
/// { let value = operand; (-value.0, -value.1) }
/// ```
///
/// Saving the left operand before evaluating the right also preserves compound
/// assignment semantics when the RHS writes to the target. The final store
/// uses the original assignment place, but arithmetic uses its saved old value.
///
/// # Requires
///
/// - `complex_values` was collected before nominal erasure in this package.
/// - Operand expressions have already been erased in structural postorder.
/// - Input is well-typed FIR: an unmarked arithmetic operand is Double, and
///   multiplication/unary signs operate on Complex values.
///
/// # Mutations
///
/// Allocates immutable operand bindings, scalar operations, component reads and
/// a result block. Replaces `id`'s kind/type in place while retaining its ID
/// and span. Arithmetic results have [`complex_ty`]; assignments remain Unit.
pub(super) fn lower(
    package: &mut Package,
    assigner: &mut Assigner,
    id: ExprId,
    complex_values: &FxHashSet<ExprId>,
) {
    let expr = package.get_expr(id);
    let value_id = match &expr.kind {
        ExprKind::AssignOp(_, lhs, _) => *lhs,
        _ => id,
    };
    if !complex_values.contains(&value_id) {
        return;
    }

    let expr = expr.clone();
    let (op, lhs, rhs, assign) = match expr.kind {
        ExprKind::BinOp(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul), lhs, rhs) => {
            (Some(op), lhs, Some(rhs), false)
        }
        ExprKind::AssignOp(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul), lhs, rhs) => {
            (Some(op), lhs, Some(rhs), true)
        }
        ExprKind::UnOp(UnOp::Neg | UnOp::Pos, value) => (None, value, None, false),
        _ => return,
    };
    let mut statements = Vec::new();
    // Capture the complete left value before an effectful RHS can overwrite it.
    let left = bind_operand(
        package,
        assigner,
        lhs,
        complex_values.contains(&lhs),
        &mut statements,
    );
    let right = rhs.map(|rhs| {
        bind_operand(
            package,
            assigner,
            rhs,
            complex_values.contains(&rhs),
            &mut statements,
        )
    });
    let mut builder = Components {
        package,
        assigner,
        span: expr.span,
    };
    let real;
    let imag;
    if let Some(op) = op {
        [real, imag] = builder.calculate(op, left, right.expect("binary right operand"));
    } else {
        let a = builder.read(left, 0);
        let b = builder.read(left, 1);
        if matches!(expr.kind, ExprKind::UnOp(UnOp::Neg, _)) {
            real = builder.negate(a);
            imag = builder.negate(b);
        } else {
            real = a;
            imag = b;
        }
    }
    let value = alloc_tuple_expr(
        builder.package,
        builder.assigner,
        vec![real, imag],
        complex_ty(),
        expr.span,
    );
    let value = if assign {
        // Keep the original place for the store, not the immutable snapshot.
        alloc_assign_expr(builder.package, builder.assigner, lhs, value, expr.span)
    } else {
        value
    };
    statements.push(alloc_expr_stmt(
        builder.package,
        builder.assigner,
        value,
        expr.span,
    ));
    let output = if assign { Ty::UNIT } else { complex_ty() };
    let block = alloc_block(
        builder.package,
        builder.assigner,
        statements,
        output.clone(),
        expr.span,
    );
    let expr = builder
        .package
        .exprs
        .get_mut(id)
        .expect("complex operation");
    expr.kind = ExprKind::Block(block);
    expr.ty = output;
}

/// Returns the erased component layout `(Double, Double)`, ordered real then
/// imaginary. The layout carries no nominal Complex identity.
fn complex_ty() -> Ty {
    Ty::Tuple(vec![Ty::Prim(Prim::Double), Ty::Prim(Prim::Double)])
}

/// Appends an immutable snapshot of an already-erased operand to `statements`.
///
/// Returns `(local, is_complex)` so component reads know whether the binding
/// stores a pair or a Double. This deliberately saves even a simple local read:
/// later operand evaluation may assign a different value to the original local.
///
/// # Before / after
///
/// ```text
/// // Before: the left operand must observe the old value of z.
/// z + { z = replacement; rhs }
///
/// // Operand bindings appended in caller-supplied order:
/// let left = z;
/// let right = { z = replacement; rhs };
/// // Subsequent component arithmetic reads left and right, never z again.
/// ```
///
/// The original expression is used as the initializer, not evaluated by this
/// helper. The caller supplies the order and later installs the statements.
fn bind_operand(
    package: &mut Package,
    assigner: &mut Assigner,
    value: ExprId,
    complex: bool,
    statements: &mut Vec<StmtId>,
) -> (LocalVarId, bool) {
    let ty = if complex {
        complex_ty()
    } else {
        Ty::Prim(Prim::Double)
    };
    let (local, stmt) = alloc_local_var(
        package,
        assigner,
        "_.complex_operand",
        &ty,
        value,
        Mutability::Immutable,
    );
    statements.push(stmt);
    (local, complex)
}

/// Builds scalar expressions from saved operands using the operation's source
/// span and the owning package's assigner. Methods allocate FIR; they do not
/// perform numeric evaluation or execute the original operand expressions.
struct Components<'a> {
    package: &'a mut Package,
    assigner: &'a mut Assigner,
    span: PackageSpan,
}

impl Components<'_> {
    /// Returns `[real, imaginary]` expressions for Add, Sub or Mul.
    ///
    /// # Before / after
    ///
    /// For saved Complex values `left = (a, b)` and `right = (c, d)`:
    ///
    /// ```text
    /// left + right  ->  (a + c, b + d)
    /// left - right  ->  (a - c, b - d)
    /// left * right  ->  (a * c - b * d, a * d + b * c)
    /// ```
    ///
    /// Mixed addition/subtraction forwards the Complex operand's imaginary
    /// component, negating it only for `Double - Complex`. It does not insert
    /// arithmetic with an artificial zero, preserving the evaluator's handling
    /// of signed zero. Multiplication requires both operand flags to be true.
    /// Repeated components receive fresh read nodes, not repeated evaluations
    /// of the original operands.
    fn calculate(
        &mut self,
        op: BinOp,
        left: (LocalVarId, bool),
        right: (LocalVarId, bool),
    ) -> [ExprId; 2] {
        let a = self.read(left, 0);
        let c = self.read(right, 0);
        let real = if op == BinOp::Mul {
            let b = self.read(left, 1);
            let d = self.read(right, 1);
            let ac = self.binary(BinOp::Mul, a, c);
            let bd = self.binary(BinOp::Mul, b, d);
            self.binary(BinOp::Sub, ac, bd)
        } else {
            self.binary(op, a, c)
        };
        let imag = if op == BinOp::Mul {
            let a = self.read(left, 0);
            let d = self.read(right, 1);
            let b = self.read(left, 1);
            let c = self.read(right, 0);
            let ad = self.binary(BinOp::Mul, a, d);
            let bc = self.binary(BinOp::Mul, b, c);
            self.binary(BinOp::Add, ad, bc)
        } else if left.1 && right.1 {
            let b = self.read(left, 1);
            let d = self.read(right, 1);
            self.binary(op, b, d)
        } else if left.1 {
            self.read(left, 1)
        } else {
            let d = self.read(right, 1);
            if op == BinOp::Sub { self.negate(d) } else { d }
        };
        [real, imag]
    }

    /// Allocates a fresh Double-valued component read from a saved operand.
    ///
    /// `read((z, true), 0/1)` becomes `z.0`/`z.1`; `read((x, false), 0)`
    /// becomes `x`. Requesting an imaginary field from a real operand is a
    /// caller error and panics rather than fabricating a value.
    fn read(&mut self, (local, complex): (LocalVarId, bool), field: usize) -> ExprId {
        let ty = if complex {
            complex_ty()
        } else {
            Ty::Prim(Prim::Double)
        };
        let read = alloc_local_var_expr(self.package, self.assigner, local, ty, self.span);
        if complex {
            alloc_field_expr(
                self.package,
                self.assigner,
                read,
                field,
                Ty::Prim(Prim::Double),
                self.span,
            )
        } else {
            assert_eq!(field, 0, "real operand has no imaginary field");
            read
        }
    }

    /// Allocates a Double-valued scalar binary operation using already-built
    /// Double operands. Callers select Add, Sub or Mul; no folding occurs here.
    fn binary(&mut self, op: BinOp, lhs: ExprId, rhs: ExprId) -> ExprId {
        alloc_bin_op_expr(
            self.package,
            self.assigner,
            op,
            lhs,
            rhs,
            Ty::Prim(Prim::Double),
            self.span,
        )
    }

    /// Allocates scalar negation `value -> -value`, preserving Double negation
    /// semantics rather than replacing it with subtraction from zero.
    fn negate(&mut self, value: ExprId) -> ExprId {
        alloc_expr(
            self.package,
            self.assigner,
            Ty::Prim(Prim::Double),
            ExprKind::UnOp(UnOp::Neg, value),
            self.span,
        )
    }
}

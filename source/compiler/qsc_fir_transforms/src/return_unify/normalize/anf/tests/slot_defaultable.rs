// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Unit tests for the slot machinery's [`is_type_defaultable`] type predicate.
//!
//! [`is_type_defaultable`](crate::return_unify::slot::is_type_defaultable)
//! decides whether the operand-lift rejection guard can fire. The lift backs an
//! operand temp with a length-1 array, so the guard inspects
//! `Ty::Array(operand.ty)` — and an array type is *unconditionally* defaultable
//! (its element type is never consulted; the default is the empty array `[]`).
//! These tests construct `Ty` values by hand and call the predicate directly to
//! pin that the array backing of a non-defaultable operand — `Qubit`, a
//! qubit-bearing tuple, or a qubit-bearing UDT — is always defaultable, so the
//! rejection guard stays dead for every surface-expressible operand. They are
//! separated from the operand-temp rejection-policy tests in
//! [`super::rejection`] because they exercise the type predicate in isolation
//! rather than the end-to-end lift behavior.

use super::*;

#[test]
fn operand_temp_array_backing_is_always_defaultable() {
    // Pins the dead-but-defensive nature of the operand-lift rejection guard in
    // the slot machinery. That guard rejects an operand only when the lifted
    // temp's type `Ty::Array(operand.ty)` has no classical default — but an
    // array type is *unconditionally* defaultable (its element type is never
    // inspected; the default is the empty array `[]`). So even an operand whose
    // own type has no default — `Qubit`, or a qubit-bearing tuple — produces a
    // defaultable array-backed temp, and the guard can never fire for any
    // surface-expressible operand. This test fails if a future change makes
    // some array type non-defaultable, which would silently activate the
    // otherwise-dead rejection path (or panic the slot machinery).
    use crate::return_unify::slot::is_type_defaultable;
    use qsc_fir::ty::{Prim, Ty};

    // A trivial package supplies a `PackageId`. The `Array(_)` arm of
    // `is_type_defaultable` returns without consulting package contents, so an
    // empty body suffices.
    let (store, pkg_id) = compile_return_unified(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {}
        }
    "#});
    let package = store.get(pkg_id);

    // A bare `Qubit` has no classical default...
    let qubit = Ty::Prim(Prim::Qubit);
    assert!(
        !is_type_defaultable(package, pkg_id, &qubit),
        "bare Qubit must remain non-defaultable for this guard to be meaningful",
    );
    // ...but the length-1 array that backs a `Qubit` operand temp does, so the
    // rejection guard stays dead for `Qubit` operands.
    let qubit_array = Ty::Array(Box::new(qubit.clone()));
    assert!(
        is_type_defaultable(package, pkg_id, &qubit_array),
        "array backing of a Qubit operand temp must be defaultable",
    );

    // A qubit-bearing tuple has no classical default...
    let qubit_tuple = Ty::Tuple(vec![Ty::Prim(Prim::Qubit), Ty::Prim(Prim::Int)]);
    assert!(
        !is_type_defaultable(package, pkg_id, &qubit_tuple),
        "a (Qubit, Int) tuple must remain non-defaultable",
    );
    // ...but its array backing does, so the guard stays dead for tuple operands.
    let qubit_tuple_array = Ty::Array(Box::new(qubit_tuple));
    assert!(
        is_type_defaultable(package, pkg_id, &qubit_tuple_array),
        "array backing of a (Qubit, Int) operand temp must be defaultable",
    );
}

#[test]
fn operand_temp_array_backing_is_defaultable_for_qubit_udt() {
    // The same dead-but-defensive pin as above, extended to a qubit-bearing
    // user-defined type: a UDT array temp stays defaultable even though the UDT
    // itself is not, so a UDT-typed operand is never rejected.
    use crate::return_unify::slot::is_type_defaultable;
    use qsc_fir::fir::{ItemId, ItemKind, Res};
    use qsc_fir::ty::Ty;

    let (store, pkg_id) = compile_return_unified(indoc! {r#"
        namespace Test {
            struct Holder { Q : Qubit }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let h = new Holder { Q = q };
            }
        }
    "#});
    let package = store.get(pkg_id);

    // Locate the `Holder` newtype item in the compiled package and build its
    // UDT type.
    let holder_item = package
        .items
        .iter()
        .find_map(|(item_id, item)| match &item.kind {
            ItemKind::Ty(name, _) if &*name.name == "Holder" => Some(item_id),
            _ => None,
        })
        .expect("compiled package should contain the `Holder` newtype");
    let holder_ty = Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: holder_item,
    }));

    // The qubit-bearing UDT has no classical default...
    assert!(
        !is_type_defaultable(package, pkg_id, &holder_ty),
        "a UDT with a Qubit field must remain non-defaultable",
    );
    // ...but an array of it (the operand-temp backing) does, so the rejection
    // guard stays dead for UDT-typed operands too.
    let holder_array = Ty::Array(Box::new(holder_ty));
    assert!(
        is_type_defaultable(package, pkg_id, &holder_array),
        "array backing of a UDT operand temp must be defaultable",
    );
}

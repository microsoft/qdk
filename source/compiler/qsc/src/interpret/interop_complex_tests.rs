// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    compile,
    interpret::{InterpretResult, Interpreter},
};
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_eval::{output::CursorReceiver, val::Value};
use qsc_fir::fir::StoreItemId;
use qsc_passes::PackageType;
use std::{io::Cursor, rc::Rc};

fn interpreter_with_stdlib(
    capabilities: TargetCapabilityFlags,
) -> (Interpreter, crate::hir::ItemId) {
    let (std_id, store) = compile::package_store_with_stdlib(capabilities);
    let complex_item = find_core_complex(&store);
    let interpreter = Interpreter::new(
        SourceMap::default(),
        PackageType::Lib,
        capabilities,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .expect("interpreter should be created");
    (interpreter, complex_item)
}

fn find_core_complex(store: &crate::PackageStore) -> crate::hir::ItemId {
    let core = store
        .get(crate::hir::PackageId::CORE)
        .expect("core package should exist");
    let item = core
        .package
        .items
        .iter()
        .find_map(|(id, item)| match &item.kind {
            crate::hir::ItemKind::Ty(ident, _) if &*ident.name == "Complex" => Some(id),
            _ => None,
        })
        .expect("core should define Complex");
    crate::hir::ItemId {
        package: crate::hir::PackageId::CORE,
        item,
    }
}

fn invoke(interpreter: &mut Interpreter, callable: &str, args: Value) -> InterpretResult {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    let callable = interpreter
        .eval_fragments(&mut receiver, callable)
        .expect("callable should evaluate");
    interpreter.invoke(&mut receiver, callable, args)
}

#[test]
fn interop_complex_tag_matches_eval_tag() {
    let (interpreter, complex_item) = interpreter_with_stdlib(TargetCapabilityFlags::all());
    let (_, _kind) = interpreter.udt_ty_from_item_id(&complex_item);
    assert_eq!(interpreter.get_complex_id(), StoreItemId::complex());
}

#[test]
fn interop_complex_arithmetic_does_not_panic() {
    let (mut interpreter, complex_item) = interpreter_with_stdlib(TargetCapabilityFlags::all());
    let (_, _kind) = interpreter.udt_ty_from_item_id(&complex_item);

    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    interpreter
        .eval_fragments(
            &mut receiver,
            "operation Add(c : Complex) : Complex { c + 1.0i }",
        )
        .expect("declaration should evaluate");

    let from_python = Value::Tuple(
        Rc::new([Value::Double(1.0), Value::Double(2.0)]),
        Some(Rc::new(interpreter.get_complex_id())),
    );

    let result = invoke(&mut interpreter, "Add", from_python);
    match result {
        Ok(value) => assert_eq!(
            value.to_string(),
            "(1.0, 3.0)",
            "interop complex addition should produce (1.0, 3.0)"
        ),
        Err(err) => panic!("interop complex addition failed: {err:?}"),
    }
}

#[test]
fn qirgen_from_callable_with_interop_complex_does_not_panic() {
    let rif = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;
    let (mut interpreter, complex_item) = interpreter_with_stdlib(rif);
    let (_, _kind) = interpreter.udt_ty_from_item_id(&complex_item);

    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    interpreter
        .eval_fragments(
            &mut receiver,
            "operation Add(c : Complex) : Unit { let d : Complex = 3.0i; let _ = c + d; }",
        )
        .expect("declaration should evaluate");
    let callable = interpreter
        .eval_fragments(&mut receiver, "Add")
        .expect("callable should evaluate");

    let from_python = Value::Tuple(
        Rc::new([Value::Double(1.0), Value::Double(2.0)]),
        Some(Rc::new(interpreter.get_complex_id())),
    );

    let result = interpreter.qirgen_from_callable(&callable, from_python);
    assert!(result.is_ok(), "qirgen failed: {result:?}");
}

#[test]
fn qirgen_from_callable_with_complex_literals_does_not_panic() {
    let rif = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;
    let (mut interpreter, _) = interpreter_with_stdlib(rif);

    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    interpreter
        .eval_fragments(
            &mut receiver,
            "operation Add() : Unit { let a : Complex = 1.0 + 2.0i; let b : Complex = 3.0i; let _ = a + b; }",
        )
        .expect("declaration should evaluate");
    let callable = interpreter
        .eval_fragments(&mut receiver, "Add")
        .expect("callable should evaluate");

    let result = interpreter.qirgen_from_callable(&callable, Value::unit());
    assert!(result.is_ok(), "qirgen failed: {result:?}");
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qdk_circuit_api::Circuit;
use qsc_circuit_store::{CircuitStore, Clbit, PackedOperation, ParamValue, Qubit, StandardGate};

const STORE: &[u8] = include_bytes!("../../compiler/qsc_snapshot/tests/fixtures/store.bin");
const STORE_TEXT: &str = include_str!("../../compiler/qsc_snapshot/tests/fixtures/store.sexpr");
const DOCUMENT: &[u8] = include_bytes!("../../compiler/qsc_snapshot/tests/fixtures/circuit.bin");
const DOCUMENT_TEXT: &str =
    include_str!("../../compiler/qsc_snapshot/tests/fixtures/circuit.sexpr");

fn circuit() -> Circuit {
    let mut store = CircuitStore::new(2, 1);
    store.set_global_phase(-0.0);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("H");
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("CX");
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measure");
    Circuit::of(store)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bare_store_matches_the_independent_packed_fixture() {
    assert_eq!(circuit().store().to_bytes().expect("encode"), STORE);
    assert_eq!(
        CircuitStore::from_bytes(STORE)
            .expect("decode")
            .to_bytes()
            .expect("encode"),
        STORE
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bare_store_text_matches_the_shared_fixture() {
    assert_eq!(circuit().store().to_text().expect("encode"), STORE_TEXT);
    assert_eq!(
        CircuitStore::from_text(STORE_TEXT)
            .expect("decode")
            .to_bytes()
            .expect("encode"),
        STORE
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn complete_document_matches_the_shared_packed_fixture() {
    assert_eq!(circuit().to_bytes().expect("encode"), DOCUMENT);
    assert_eq!(
        Circuit::from_bytes(DOCUMENT)
            .expect("decode")
            .to_bytes()
            .expect("encode"),
        DOCUMENT
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn complete_document_text_matches_the_shared_fixture() {
    assert_eq!(circuit().to_text().expect("encode"), DOCUMENT_TEXT);
    assert_eq!(
        Circuit::from_text(DOCUMENT_TEXT)
            .expect("decode")
            .to_bytes()
            .expect("encode"),
        DOCUMENT
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn document_nan_payload_and_signed_zero_survive_both_encodings() {
    let mut circuit = Circuit::new(1, 0);
    let bits = 0xfff0_1234_5678_9000;
    circuit.store_mut().set_global_phase(-0.0);
    circuit
        .append(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Literal(f64::from_bits(bits))])),
        )
        .expect("gate");
    let bytes = circuit.to_bytes().expect("binary");
    let binary = Circuit::from_bytes(&bytes).expect("decode");
    let text = Circuit::from_text(&circuit.to_text().expect("text")).expect("decode text");
    assert_eq!(text.to_bytes().expect("binary"), bytes);
    assert_eq!(binary.to_bytes().expect("binary"), bytes);
    assert_eq!(text.store().global_phase().to_bits(), (-0.0_f64).to_bits());
    let Some([ParamValue::Literal(value)]) = text
        .store()
        .instruction(0)
        .expect("instruction")
        .params
        .as_deref()
    else {
        panic!("literal");
    };
    assert_eq!(value.to_bits(), bits);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn maximum_declared_width_is_not_platform_sized() {
    let circuit = Circuit::new(u32::MAX, u32::MAX);
    let restored = Circuit::from_bytes(&circuit.to_bytes().expect("binary")).expect("decode");
    assert_eq!(restored.store().qubit_count(), u32::MAX);
    assert_eq!(restored.store().clbit_count(), u32::MAX);
}

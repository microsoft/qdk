// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    ExternalManifest, Functor, OperationContract, OperationIdentity, OwnershipTransfer, Registry,
    RegistryError, RegistryVersion, Signature, Stage, Type, gate_identity,
};

#[test]
fn schema_has_stable_version_and_structural_types() {
    assert_eq!(RegistryVersion::V1_0.major, 1);
    assert_eq!(
        Type::Array(Box::new(Type::Tuple(vec![Type::Int, Type::Bool]))),
        Type::Array(Box::new(Type::Tuple(vec![Type::Int, Type::Bool])))
    );
}

#[test]
fn builtins_cover_required_operation_families() {
    let registry = Registry::builtins();
    assert_eq!(registry.iter().len(), 42);
    for identity in [
        "qdk.value.constant",
        "qdk.value.tuple",
        "qdk.value.array",
        "qdk.memory.read",
        "qdk.memory.write",
        "qdk.call",
        "qdk.quantum.allocate",
        "qdk.quantum.gate",
        "qdk.quantum.measure",
        "qdk.quantum.reset",
        "qdk.quantum.release",
        "qdk.output",
        "qdk.failure",
        "qdk.noise",
        "qdk.extern.unknown",
    ] {
        assert!(registry.get(identity).is_some(), "missing {identity}");
    }
}

#[test]
fn each_circuit_gate_has_its_own_identity_and_says_whether_it_is_diagonal() {
    // The declaration this registry exists to hold. It is a statement about
    // what an operation means rather than something derivable here, because the
    // IR carries no matrix.
    let registry = Registry::builtins();
    let declared: Vec<&str> = [
        "i", "x", "y", "z", "h", "s", "s_adj", "sx", "sx_adj", "t", "t_adj", "cx", "cy", "cz",
        "swap", "ccx", "cswap", "rx", "ry", "rz", "rxx", "ryy", "rzz", "crx", "cry", "crz",
    ]
    .into_iter()
    .filter(|gate| {
        registry
            .get(&gate_identity(gate))
            .unwrap_or_else(|| panic!("'{gate}' should have its own contract"))
            .computational_basis_diagonal
    })
    .collect();

    assert_eq!(
        declared,
        [
            "i", "z", "s", "s_adj", "t", "t_adj", "cz", "rz", "rzz", "crz"
        ],
        "the declared diagonal set is exactly this"
    );
    // The half that makes the assertion above mean something. A declaration of
    // everything would satisfy a membership check and would be useless.
    for gate in ["h", "x", "y", "sx", "cx", "swap", "ccx", "rx", "ryy", "cry"] {
        assert!(
            !registry
                .get(&gate_identity(gate))
                .expect("contract")
                .computational_basis_diagonal,
            "'{gate}' is not diagonal in the computational basis"
        );
    }
}

#[test]
fn a_per_gate_contract_states_its_own_shape_rather_than_the_class_contracts() {
    // The three fields a template would have got wrong. Nothing reads them yet,
    // which is exactly why they have to be right: a wrong declaration nobody
    // reads is the latent defect a registry of operation meaning creates.
    let registry = Registry::builtins();
    let class = registry.get("qdk.quantum.gate").expect("class contract");
    assert!(class.runtime.is_some(), "the class contract names a symbol");
    assert_eq!(class.functors, [Functor::Adjoint, Functor::Controlled]);

    let cz = registry.get(&gate_identity("cz")).expect("cz contract");
    assert_eq!(
        cz.runtime, None,
        "no gate's entry point is the class symbol"
    );
    assert_eq!(cz.functors, [], "the vocabulary has no functor construct");
    assert_eq!(
        cz.ownership,
        OwnershipTransfer::Transform {
            inputs: vec![0, 1],
            outputs: vec![0, 1],
        },
        "ownership follows arity rather than the class contract's single operand"
    );
    assert_eq!(cz.signature.inputs, [Type::Qubit, Type::Qubit]);

    let crz = registry.get(&gate_identity("crz")).expect("crz contract");
    assert_eq!(
        crz.signature.inputs,
        [Type::Qubit, Type::Qubit, Type::Double],
        "qubits first and the angle after, which is the order the lowering builds"
    );
    assert_eq!(crz.signature.outputs, [Type::Qubit, Type::Qubit]);
}

#[test]
fn every_added_gate_contract_is_protected() {
    // An assertion about what the generator produced, not about the `qdk.`
    // prefix refusal. That refusal held before these contracts existed and
    // would hold if they were reverted, so a test of it could not fail for any
    // reason connected to them.
    let registry = Registry::builtins();
    for gate in [
        "i", "x", "y", "z", "h", "s", "s_adj", "sx", "sx_adj", "t", "t_adj", "cx", "cy", "cz",
        "swap", "ccx", "cswap", "rx", "ry", "rz", "rxx", "ryy", "rzz", "crx", "cry", "crz",
    ] {
        assert!(
            registry
                .get(&gate_identity(gate))
                .unwrap_or_else(|| panic!("'{gate}' should have its own contract"))
                .protected,
            "'{gate}' must not be overridable from outside"
        );
    }
}

#[test]
fn a_manifest_written_before_the_diagonality_field_still_loads() {
    // The field is additive for anything already on disk. `OperationContract`
    // does not deny unknown fields and the digest compares a caller-supplied
    // literal rather than hashing contract content, so nothing existing moves.
    let json = r#"{
        "schema_version":{"major":1,"minor":0},
        "digest":"trusted",
        "required_features":[],
        "operations":[{
            "identity":"vendor.legacy",
            "signature":{"inputs":[{"kind":"qubit"}],"outputs":[{"kind":"qubit"}]},
            "ownership":{"kind":"none"},
            "stage":"residual",
            "runtime":null
        }]
    }"#;
    let mut registry = Registry::builtins();
    registry
        .extend_external_json(json, "trusted")
        .expect("a manifest written before the field must still load");
    let legacy = registry.get("vendor.legacy").expect("the manifest loaded");
    assert!(
        !legacy.computational_basis_diagonal,
        "an operation that says nothing about diagonality is not diagonal"
    );

    // And the field round-trips when a manifest does carry it.
    let carried = serde_json::to_string(
        Registry::builtins()
            .get(&gate_identity("rz"))
            .expect("rz contract"),
    )
    .expect("a contract serializes");
    let parsed: OperationContract = serde_json::from_str(&carried).expect("a contract round-trips");
    assert!(parsed.computational_basis_diagonal);
}

#[test]
fn unknown_required_features_fail_closed() {
    let json = r#"{
        "schema_version":{"major":1,"minor":0},
        "digest":"trusted",
        "required_features":["future-semantics"],
        "operations":[]
    }"#;
    let error = Registry::builtins()
        .extend_external_json(json, "trusted")
        .expect_err("unknown required feature must fail");
    assert_eq!(
        error,
        RegistryError::UnknownRequiredFeature("future-semantics".to_string())
    );
}

#[test]
fn external_manifests_are_digest_bound_and_cannot_override_builtins() {
    let operation = OperationContract {
        identity: OperationIdentity::from("qdk.output"),
        signature: Signature {
            inputs: vec![],
            outputs: vec![],
        },
        functors: vec![],
        effects: vec![],
        ownership: OwnershipTransfer::None,
        stage: Stage::Either,
        runtime: None,
        required_features: vec![],
        protected: false,
        computational_basis_diagonal: false,
    };
    let manifest = ExternalManifest {
        schema_version: RegistryVersion::V1_0,
        digest: "trusted".to_string(),
        required_features: vec![],
        operations: vec![operation],
    };
    assert_eq!(
        Registry::builtins()
            .extend_external(manifest.clone(), "different")
            .expect_err("digest mismatch must fail"),
        RegistryError::DigestMismatch
    );
    assert_eq!(
        Registry::builtins()
            .extend_external(manifest, "trusted")
            .expect_err("protected identity must fail"),
        RegistryError::ProtectedIdentity("qdk.output".to_string())
    );
}

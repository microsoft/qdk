// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::schema::{
    Effect, EffectDomain, EffectMode, ExternalManifest, Functor, OperationContract,
    OperationIdentity, OwnershipTransfer, RegistryVersion, RuntimeIdentity, Signature, Stage, Type,
};

const SUPPORTED_FEATURES: [&str; 2] = ["contracts-v1", "runtime-identity-v1"];

#[derive(Clone, Debug)]
pub struct Registry {
    version: RegistryVersion,
    operations: BTreeMap<OperationIdentity, OperationContract>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RegistryError {
    #[error("invalid external manifest: {0}")]
    InvalidSchema(String),
    #[error("unsupported registry version {major}.{minor}")]
    UnsupportedVersion { major: u16, minor: u16 },
    #[error("unknown required feature `{0}`")]
    UnknownRequiredFeature(String),
    #[error("duplicate operation identity `{0}`")]
    DuplicateIdentity(String),
    #[error("external manifest cannot override protected identity `{0}`")]
    ProtectedIdentity(String),
    #[error("manifest digest does not match the trusted digest")]
    DigestMismatch,
}

impl Registry {
    #[must_use]
    pub fn builtins() -> Self {
        let operations = builtin_contracts()
            .into_iter()
            .map(|contract| (contract.identity.clone(), contract))
            .collect();
        Self {
            version: RegistryVersion::V1_0,
            operations,
        }
    }

    #[must_use]
    pub fn version(&self) -> RegistryVersion {
        self.version
    }

    #[must_use]
    pub fn get(&self, identity: &str) -> Option<&OperationContract> {
        self.operations.get(&OperationIdentity::from(identity))
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &OperationContract> {
        self.operations.values()
    }

    pub fn extend_external(
        &mut self,
        manifest: ExternalManifest,
        trusted_digest: &str,
    ) -> Result<(), RegistryError> {
        validate_version(manifest.schema_version)?;
        validate_features(&manifest.required_features)?;
        if manifest.digest != trusted_digest {
            return Err(RegistryError::DigestMismatch);
        }

        let mut identities = BTreeSet::new();
        for contract in &manifest.operations {
            validate_features(&contract.required_features)?;
            if !identities.insert(&contract.identity) {
                return Err(RegistryError::DuplicateIdentity(
                    contract.identity.0.clone(),
                ));
            }
            if contract.identity.0.starts_with("qdk.")
                || self.operations.contains_key(&contract.identity)
            {
                return Err(RegistryError::ProtectedIdentity(
                    contract.identity.0.clone(),
                ));
            }
        }

        self.operations.extend(
            manifest
                .operations
                .into_iter()
                .map(|contract| (contract.identity.clone(), contract)),
        );
        Ok(())
    }

    pub fn extend_external_json(
        &mut self,
        json: &str,
        trusted_digest: &str,
    ) -> Result<(), RegistryError> {
        let manifest = serde_json::from_str(json)
            .map_err(|error| RegistryError::InvalidSchema(error.to_string()))?;
        self.extend_external(manifest, trusted_digest)
    }

    #[must_use]
    pub fn unknown_extern(identity: impl Into<String>) -> OperationContract {
        OperationContract {
            identity: OperationIdentity(identity.into()),
            signature: Signature {
                inputs: vec![Type::Unknown],
                outputs: vec![Type::Unknown],
            },
            functors: Vec::new(),
            effects: vec![effect(EffectDomain::Unknown, EffectMode::Unknown, true)],
            ownership: OwnershipTransfer::Escape { inputs: vec![0] },
            stage: Stage::Residual,
            runtime: None,
            required_features: vec!["contracts-v1".to_string()],
            protected: false,
            computational_basis_diagonal: false,
        }
    }
}

fn validate_version(version: RegistryVersion) -> Result<(), RegistryError> {
    if version == RegistryVersion::V1_0 {
        Ok(())
    } else {
        Err(RegistryError::UnsupportedVersion {
            major: version.major,
            minor: version.minor,
        })
    }
}

fn validate_features(features: &[String]) -> Result<(), RegistryError> {
    for feature in features {
        if !SUPPORTED_FEATURES.contains(&feature.as_str()) {
            return Err(RegistryError::UnknownRequiredFeature(feature.clone()));
        }
    }
    Ok(())
}

fn effect(domain: EffectDomain, mode: EffectMode, observable: bool) -> Effect {
    Effect {
        domain,
        mode,
        qualifier: None,
        observable,
    }
}

fn runtime(symbol: &str) -> Option<RuntimeIdentity> {
    Some(RuntimeIdentity {
        abi: "qdk-runtime-v1".to_string(),
        symbol: symbol.to_string(),
    })
}

fn contract(
    identity: &str,
    inputs: Vec<Type>,
    outputs: Vec<Type>,
    effects: Vec<Effect>,
    ownership: OwnershipTransfer,
    stage: Stage,
    symbol: Option<&str>,
) -> OperationContract {
    OperationContract {
        identity: OperationIdentity::from(identity),
        signature: Signature { inputs, outputs },
        functors: Vec::new(),
        effects,
        ownership,
        stage,
        runtime: symbol.and_then(runtime),
        required_features: vec!["contracts-v1".to_string()],
        protected: true,
        computational_basis_diagonal: false,
    }
}

/// One circuit gate's differences from the `qdk.quantum.gate` class contract.
///
/// The table is the canonical source for the per-gate contracts and the
/// contracts are its output, so a gate is described once. It carries only what
/// varies between siblings, because a value copied from the class contract onto
/// a gate it does not describe is a false declaration that nothing reads, which
/// is the shape a registry of operation meaning exists to stop producing.
struct GateShape {
    /// The name the circuit vocabulary emits, without its `quantum.` prefix.
    name: &'static str,
    qubits: u16,
    angles: u16,
    /// Whether the gate is diagonal in the computational basis.
    ///
    /// Conservative on purpose: a gate enters only when its diagonality is
    /// unambiguous, and saying nothing costs an optimization while saying it
    /// wrongly costs correctness.
    diagonal: bool,
}

/// Every unitary gate the circuit vocabulary emits.
///
/// These are the names one `qdk.quantum.gate` contract used to stand for, so
/// splitting it splits exactly this set. Measurement and preparation are not
/// here: `qdk.quantum.measure` already exists and produces a `Result`, and
/// preparation is not unitary, so either one generated from the gate template
/// would carry a signature it does not have.
const CIRCUIT_GATES: [GateShape; 26] = [
    GateShape {
        name: "i",
        qubits: 1,
        angles: 0,
        diagonal: true,
    },
    GateShape {
        name: "x",
        qubits: 1,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "y",
        qubits: 1,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "z",
        qubits: 1,
        angles: 0,
        diagonal: true,
    },
    GateShape {
        name: "h",
        qubits: 1,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "s",
        qubits: 1,
        angles: 0,
        diagonal: true,
    },
    GateShape {
        name: "s_adj",
        qubits: 1,
        angles: 0,
        diagonal: true,
    },
    GateShape {
        name: "sx",
        qubits: 1,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "sx_adj",
        qubits: 1,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "t",
        qubits: 1,
        angles: 0,
        diagonal: true,
    },
    GateShape {
        name: "t_adj",
        qubits: 1,
        angles: 0,
        diagonal: true,
    },
    GateShape {
        name: "cx",
        qubits: 2,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "cy",
        qubits: 2,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "cz",
        qubits: 2,
        angles: 0,
        diagonal: true,
    },
    GateShape {
        name: "swap",
        qubits: 2,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "ccx",
        qubits: 3,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "cswap",
        qubits: 3,
        angles: 0,
        diagonal: false,
    },
    GateShape {
        name: "rx",
        qubits: 1,
        angles: 1,
        diagonal: false,
    },
    GateShape {
        name: "ry",
        qubits: 1,
        angles: 1,
        diagonal: false,
    },
    GateShape {
        name: "rz",
        qubits: 1,
        angles: 1,
        diagonal: true,
    },
    GateShape {
        name: "rxx",
        qubits: 2,
        angles: 1,
        diagonal: false,
    },
    GateShape {
        name: "ryy",
        qubits: 2,
        angles: 1,
        diagonal: false,
    },
    GateShape {
        name: "rzz",
        qubits: 2,
        angles: 1,
        diagonal: true,
    },
    GateShape {
        name: "crx",
        qubits: 2,
        angles: 1,
        diagonal: false,
    },
    GateShape {
        name: "cry",
        qubits: 2,
        angles: 1,
        diagonal: false,
    },
    GateShape {
        name: "crz",
        qubits: 2,
        angles: 1,
        diagonal: true,
    },
];

/// The `qdk.` identity of a circuit gate, so the one place that spells it is
/// also the one the mapping asks.
#[must_use]
pub fn gate_identity(name: &str) -> String {
    format!("qdk.quantum.{name}")
}

/// Every per-gate contract, generated from the class contract's shape.
fn gate_contracts() -> Vec<OperationContract> {
    CIRCUIT_GATES.iter().map(gate_contract).collect()
}

fn gate_contract(shape: &GateShape) -> OperationContract {
    let qubits = usize::from(shape.qubits);
    // Qubits first and angles after, which is the order the lowering gives a
    // gate node its ports, so the ownership indices below mean what they say.
    let mut inputs = vec![Type::Qubit; qubits];
    inputs.resize(qubits + usize::from(shape.angles), Type::Double);
    OperationContract {
        identity: OperationIdentity(gate_identity(shape.name)),
        signature: Signature {
            inputs,
            outputs: vec![Type::Qubit; qubits],
        },
        // No functors, where the class contract declares adjoint and
        // controlled. The circuit vocabulary has no functor construct: `cx` is
        // its own name rather than `x` controlled, and `s_adj` is its own name
        // rather than `s` adjointed. Declaring one per gate would claim a form
        // nothing here can build.
        functors: Vec::new(),
        effects: vec![effect(EffectDomain::Quantum, EffectMode::Write, true)],
        // Derived from arity rather than inherited. The class contract declares
        // one input and one output, which is wrong for every multi-qubit gate.
        ownership: OwnershipTransfer::Transform {
            inputs: (0..shape.qubits).collect(),
            outputs: (0..shape.qubits).collect(),
        },
        stage: Stage::Residual,
        // No runtime symbol, where the class contract carries
        // `__quantum__qis__gate__body`. That is no individual gate's entry
        // point, so inheriting it would ship one false declaration per gate.
        // An absence is honest and a real symbol is additive.
        runtime: None,
        required_features: vec!["contracts-v1".to_string()],
        protected: true,
        computational_basis_diagonal: shape.diagonal,
    }
}

fn builtin_contracts() -> Vec<OperationContract> {
    let mut contracts = value_and_memory_contracts();
    contracts.extend(quantum_and_runtime_contracts());
    contracts.extend(gate_contracts());
    contracts.push(Registry::unknown_extern("qdk.extern.unknown"));
    contracts
}

fn value_and_memory_contracts() -> Vec<OperationContract> {
    vec![
        contract(
            "qdk.value.constant",
            vec![],
            vec![Type::Unknown],
            vec![],
            OwnershipTransfer::None,
            Stage::Either,
            None,
        ),
        contract(
            "qdk.value.tuple",
            vec![Type::Unknown],
            vec![Type::Tuple(vec![Type::Unknown])],
            vec![],
            OwnershipTransfer::None,
            Stage::Either,
            None,
        ),
        contract(
            "qdk.value.array",
            vec![Type::Unknown],
            vec![Type::Array(Box::new(Type::Unknown))],
            vec![],
            OwnershipTransfer::None,
            Stage::Either,
            None,
        ),
        contract(
            "qdk.memory.read",
            vec![Type::Location(Box::new(Type::Unknown))],
            vec![Type::Unknown],
            vec![effect(EffectDomain::Memory, EffectMode::Read, false)],
            OwnershipTransfer::None,
            Stage::Either,
            None,
        ),
        contract(
            "qdk.memory.write",
            vec![Type::Location(Box::new(Type::Unknown)), Type::Unknown],
            vec![Type::Unit],
            vec![effect(EffectDomain::Memory, EffectMode::Write, true)],
            OwnershipTransfer::None,
            Stage::Either,
            None,
        ),
        contract(
            "qdk.call",
            vec![Type::Unknown],
            vec![Type::Unknown],
            vec![effect(EffectDomain::Host, EffectMode::Unknown, true)],
            OwnershipTransfer::Unknown,
            Stage::Either,
            None,
        ),
    ]
}

fn quantum_and_runtime_contracts() -> Vec<OperationContract> {
    let mut gate = contract(
        "qdk.quantum.gate",
        vec![Type::Qubit],
        vec![Type::Qubit],
        vec![effect(EffectDomain::Quantum, EffectMode::Write, true)],
        OwnershipTransfer::Transform {
            inputs: vec![0],
            outputs: vec![0],
        },
        Stage::Residual,
        Some("__quantum__qis__gate__body"),
    );
    gate.functors = vec![Functor::Adjoint, Functor::Controlled];

    vec![
        contract(
            "qdk.quantum.allocate",
            vec![Type::Unit],
            vec![Type::Qubit],
            vec![effect(EffectDomain::Allocation, EffectMode::Allocate, true)],
            OwnershipTransfer::Allocate { output: 0 },
            Stage::Residual,
            Some("__quantum__rt__qubit_allocate"),
        ),
        gate,
        contract(
            "qdk.quantum.measure",
            vec![Type::Qubit],
            vec![Type::Qubit, Type::Result],
            vec![effect(EffectDomain::Result, EffectMode::Irreversible, true)],
            OwnershipTransfer::Transform {
                inputs: vec![0],
                outputs: vec![0],
            },
            Stage::Residual,
            Some("__quantum__qis__m__body"),
        ),
        contract(
            "qdk.quantum.reset",
            vec![Type::Qubit],
            vec![Type::Qubit],
            vec![effect(EffectDomain::Quantum, EffectMode::Write, true)],
            OwnershipTransfer::Transform {
                inputs: vec![0],
                outputs: vec![0],
            },
            Stage::Residual,
            Some("__quantum__qis__reset__body"),
        ),
        contract(
            "qdk.quantum.release",
            vec![Type::Qubit],
            vec![Type::Unit],
            vec![effect(EffectDomain::Allocation, EffectMode::Release, true)],
            OwnershipTransfer::Release {
                input: 0,
                requires_clean: true,
            },
            Stage::Residual,
            Some("__quantum__rt__qubit_release"),
        ),
        contract(
            "qdk.output",
            vec![Type::Unknown],
            vec![Type::Unit],
            vec![effect(EffectDomain::Output, EffectMode::Emit, true)],
            OwnershipTransfer::None,
            Stage::Residual,
            Some("__quantum__rt__output"),
        ),
        contract(
            "qdk.failure",
            vec![Type::String],
            vec![],
            vec![effect(EffectDomain::Abort, EffectMode::Abort, true)],
            OwnershipTransfer::Unknown,
            Stage::Either,
            Some("__quantum__rt__fail"),
        ),
        contract(
            "qdk.noise",
            vec![Type::Qubit],
            vec![Type::Qubit],
            vec![effect(EffectDomain::Noise, EffectMode::Irreversible, true)],
            OwnershipTransfer::Transform {
                inputs: vec![0],
                outputs: vec![0],
            },
            Stage::Residual,
            Some("__quantum__qis__noise__body"),
        ),
        contract(
            "qdk.result.read",
            vec![Type::Result],
            vec![Type::Bool],
            vec![effect(EffectDomain::Result, EffectMode::Read, true)],
            OwnershipTransfer::None,
            Stage::Residual,
            Some("__quantum__qis__read_result__body"),
        ),
    ]
}

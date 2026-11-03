// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RegistryVersion {
    pub major: u16,
    pub minor: u16,
}

impl RegistryVersion {
    pub const V1_0: Self = Self { major: 1, minor: 0 };
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "element")]
pub enum Type {
    Unit,
    Bool,
    Int,
    Double,
    String,
    Result,
    Qubit,
    Tuple(Vec<Self>),
    Array(Box<Self>),
    Location(Box<Self>),
    Unknown,
}

impl Type {
    /// Whether a value of this type carries a quantum capability, at any depth.
    ///
    /// `Location` propagates because a copy of a place that holds a qubit lets
    /// two consumers reach the same qubit, which is the thing no-cloning
    /// forbids. `Unknown` does not propagate: it is the absence of a type
    /// rather than a claim about one, and treating it as quantum would make
    /// every unannotated port linear.
    #[must_use]
    pub fn carries_qubit(&self) -> bool {
        match self {
            Self::Qubit => true,
            Self::Tuple(elements) => elements.iter().any(Self::carries_qubit),
            Self::Array(element) | Self::Location(element) => element.carries_qubit(),
            Self::Unit
            | Self::Bool
            | Self::Int
            | Self::Double
            | Self::String
            | Self::Result
            | Self::Unknown => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct OperationIdentity(pub String);

impl From<&str> for OperationIdentity {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Signature {
    pub inputs: Vec<Type>,
    pub outputs: Vec<Type>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Functor {
    Adjoint,
    Controlled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectDomain {
    Memory,
    Quantum,
    Result,
    Allocation,
    Output,
    Abort,
    Host,
    Noise,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectMode {
    Read,
    Write,
    Irreversible,
    Allocate,
    Release,
    Emit,
    Abort,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Effect {
    pub domain: EffectDomain,
    pub mode: EffectMode,
    pub qualifier: Option<String>,
    pub observable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum OwnershipTransfer {
    None,
    Allocate { output: u16 },
    Transform { inputs: Vec<u16>, outputs: Vec<u16> },
    Release { input: u16, requires_clean: bool },
    Escape { inputs: Vec<u16> },
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Generation,
    Residual,
    Either,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeIdentity {
    pub abi: String,
    pub symbol: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationContract {
    pub identity: OperationIdentity,
    pub signature: Signature,
    #[serde(default)]
    pub functors: Vec<Functor>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    pub ownership: OwnershipTransfer,
    pub stage: Stage,
    pub runtime: Option<RuntimeIdentity>,
    #[serde(default)]
    pub required_features: Vec<String>,
    #[serde(default)]
    pub protected: bool,
    /// Whether the operation is diagonal in the computational basis.
    ///
    /// Declared rather than derived, because the IR carries no matrix and
    /// nothing here could compute it. The basis is part of the claim: a
    /// computational-basis diagonal operation leaves a Z-basis outcome alone
    /// and changes an X-basis or Y-basis one, so a reader that drops the basis
    /// from the name will reach a wrong conclusion.
    ///
    /// Defaults to false, which is both the serde default an existing manifest
    /// needs and the safe direction: an operation says nothing about its
    /// diagonality unless it says so.
    #[serde(default)]
    pub computational_basis_diagonal: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExternalManifest {
    pub schema_version: RegistryVersion,
    pub digest: String,
    #[serde(default)]
    pub required_features: Vec<String>,
    pub operations: Vec<OperationContract>,
}

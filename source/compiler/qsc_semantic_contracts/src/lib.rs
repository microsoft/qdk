// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod registry;
pub mod schema;

pub use registry::{Registry, RegistryError, gate_identity};
pub use schema::{
    Effect, EffectDomain, EffectMode, ExternalManifest, Functor, OperationContract,
    OperationIdentity, OwnershipTransfer, RegistryVersion, RuntimeIdentity, Signature, Stage, Type,
};

#[cfg(test)]
mod tests;

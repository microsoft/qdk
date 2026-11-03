// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde::{Deserialize, Serialize};

use crate::graph::{NodeId, PortId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RegionId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegionKind {
    ModuleOmega,
    Lambda,
    Gamma,
    Theta,
    Recursion,
    Handler,
    Stage,
    ResidualCfg,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExitKind {
    Return,
    Abort,
    Branch,
    Backedge,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterfaceRole {
    Argument,
    Result,
    Exit,
    Capture,
    Carried,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InterfaceBinding {
    pub outer: PortId,
    pub inner: PortId,
    pub role: InterfaceRole,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegionInterface {
    pub arguments: Vec<PortId>,
    pub results: Vec<PortId>,
    pub exits: Vec<(ExitKind, PortId)>,
    pub bindings: Vec<InterfaceBinding>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Region {
    pub id: RegionId,
    pub parent: Option<RegionId>,
    pub kind: RegionKind,
    /// The node this region is a body of.
    ///
    /// A gamma has one region per branch and they are siblings under the same
    /// parent, so nesting alone cannot say which node they belong to or which
    /// predicate selects between them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<NodeId>,
    pub interface: RegionInterface,
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_qhdg::{
    Direction, EdgeKind, EffectDomain, EffectMode, FirOriginKind, InterfaceRole, PortKind,
    RegionKind, Stage, Usage,
};

use crate::boundary::Error;

pub const QDK_REGION_MODULE: u32 = 1;
pub const QDK_REGION_LAMBDA: u32 = 2;
pub const QDK_REGION_GAMMA: u32 = 3;
pub const QDK_REGION_THETA: u32 = 4;
pub const QDK_REGION_RECURSION: u32 = 5;
pub const QDK_REGION_HANDLER: u32 = 6;
pub const QDK_REGION_STAGE: u32 = 7;
pub const QDK_REGION_RESIDUAL_CFG: u32 = 8;
pub const QDK_PORT_INPUT: u32 = 1;
pub const QDK_PORT_OUTPUT: u32 = 2;
pub const QDK_PORT_VALUE: u32 = 1;
pub const QDK_PORT_CAPABILITY: u32 = 2;
pub const QDK_PORT_HARD_EFFECT: u32 = 3;
pub const QDK_PORT_CONTROL_EXIT: u32 = 4;
pub const QDK_STAGE_GENERATION: u32 = 1;
pub const QDK_STAGE_RESIDUAL: u32 = 2;
pub const QDK_USAGE_COPYABLE: u32 = 1;
pub const QDK_USAGE_AFFINE: u32 = 2;
pub const QDK_USAGE_LINEAR: u32 = 3;
pub const QDK_EDGE_VALUE: u32 = 1;
pub const QDK_EDGE_CAPABILITY: u32 = 2;
pub const QDK_EDGE_HARD_EFFECT: u32 = 3;
pub const QDK_EDGE_CONTROL_EXIT: u32 = 4;
pub const QDK_EFFECT_MEMORY: u32 = 1;
pub const QDK_EFFECT_QUANTUM: u32 = 2;
pub const QDK_EFFECT_RESULT: u32 = 3;
pub const QDK_EFFECT_ALLOCATION: u32 = 4;
pub const QDK_EFFECT_OUTPUT: u32 = 5;
pub const QDK_EFFECT_ABORT: u32 = 6;
pub const QDK_EFFECT_HOST: u32 = 7;
pub const QDK_EFFECT_NOISE: u32 = 8;
pub const QDK_EFFECT_UNKNOWN: u32 = 9;
pub const QDK_EFFECT_READ: u32 = 1;
pub const QDK_EFFECT_WRITE: u32 = 2;
pub const QDK_EFFECT_IRREVERSIBLE: u32 = 3;
pub const QDK_EFFECT_ALLOCATE: u32 = 4;
pub const QDK_EFFECT_RELEASE: u32 = 5;
pub const QDK_EFFECT_EMIT: u32 = 6;
pub const QDK_EFFECT_ABORT_MODE: u32 = 7;
pub const QDK_EFFECT_UNKNOWN_MODE: u32 = 8;
pub const QDK_INTERFACE_ARGUMENT: u32 = 1;
pub const QDK_INTERFACE_RESULT: u32 = 2;
pub const QDK_INTERFACE_EXIT: u32 = 3;
pub const QDK_INTERFACE_CAPTURE: u32 = 4;
pub const QDK_INTERFACE_CARRIED: u32 = 5;
pub const QDK_ORIGIN_NONE: u32 = 0;
pub const QDK_ORIGIN_INSTRUCTION: u32 = 1;
pub const QDK_ORIGIN_QUBIT: u32 = 2;
pub const QDK_ORIGIN_CLBIT: u32 = 3;
pub const QDK_ORIGIN_GLOBAL_PHASE: u32 = 4;
pub const QDK_ORIGIN_FIR: u32 = 5;
pub const QDK_FIR_PACKAGE: u32 = 1;
pub const QDK_FIR_ITEM: u32 = 2;
pub const QDK_FIR_BLOCK: u32 = 3;
pub const QDK_FIR_EXPRESSION: u32 = 4;
pub const QDK_FIR_PATTERN: u32 = 5;
pub const QDK_FIR_STATEMENT: u32 = 6;

pub(super) const REGIONS: &[(u32, RegionKind)] = &[
    (QDK_REGION_MODULE, RegionKind::ModuleOmega),
    (QDK_REGION_LAMBDA, RegionKind::Lambda),
    (QDK_REGION_GAMMA, RegionKind::Gamma),
    (QDK_REGION_THETA, RegionKind::Theta),
    (QDK_REGION_RECURSION, RegionKind::Recursion),
    (QDK_REGION_HANDLER, RegionKind::Handler),
    (QDK_REGION_STAGE, RegionKind::Stage),
    (QDK_REGION_RESIDUAL_CFG, RegionKind::ResidualCfg),
];
pub(super) const DIRECTIONS: &[(u32, Direction)] = &[
    (QDK_PORT_INPUT, Direction::Input),
    (QDK_PORT_OUTPUT, Direction::Output),
];
pub(super) const PORTS: &[(u32, PortKind)] = &[
    (QDK_PORT_VALUE, PortKind::Value),
    (QDK_PORT_CAPABILITY, PortKind::Capability),
    (QDK_PORT_HARD_EFFECT, PortKind::HardEffect),
    (QDK_PORT_CONTROL_EXIT, PortKind::ControlExit),
];
pub(super) const STAGES: &[(u32, Stage)] = &[
    (QDK_STAGE_GENERATION, Stage::Generation),
    (QDK_STAGE_RESIDUAL, Stage::Residual),
];
pub(super) const USAGES: &[(u32, Usage)] = &[
    (QDK_USAGE_COPYABLE, Usage::Copyable),
    (QDK_USAGE_AFFINE, Usage::Affine),
    (QDK_USAGE_LINEAR, Usage::Linear),
];
pub(super) const EDGES: &[(u32, EdgeKind)] = &[
    (QDK_EDGE_VALUE, EdgeKind::CopyableValue),
    (QDK_EDGE_CAPABILITY, EdgeKind::AffineCapability),
    (QDK_EDGE_HARD_EFFECT, EdgeKind::HardEffect),
    (QDK_EDGE_CONTROL_EXIT, EdgeKind::ControlExit),
];
pub(super) const DOMAINS: &[(u32, EffectDomain)] = &[
    (QDK_EFFECT_MEMORY, EffectDomain::Memory),
    (QDK_EFFECT_QUANTUM, EffectDomain::Quantum),
    (QDK_EFFECT_RESULT, EffectDomain::Result),
    (QDK_EFFECT_ALLOCATION, EffectDomain::Allocation),
    (QDK_EFFECT_OUTPUT, EffectDomain::Output),
    (QDK_EFFECT_ABORT, EffectDomain::Abort),
    (QDK_EFFECT_HOST, EffectDomain::Host),
    (QDK_EFFECT_NOISE, EffectDomain::Noise),
    (QDK_EFFECT_UNKNOWN, EffectDomain::Unknown),
];
pub(super) const MODES: &[(u32, EffectMode)] = &[
    (QDK_EFFECT_READ, EffectMode::Read),
    (QDK_EFFECT_WRITE, EffectMode::Write),
    (QDK_EFFECT_IRREVERSIBLE, EffectMode::Irreversible),
    (QDK_EFFECT_ALLOCATE, EffectMode::Allocate),
    (QDK_EFFECT_RELEASE, EffectMode::Release),
    (QDK_EFFECT_EMIT, EffectMode::Emit),
    (QDK_EFFECT_ABORT_MODE, EffectMode::Abort),
    (QDK_EFFECT_UNKNOWN_MODE, EffectMode::Unknown),
];
pub(super) const ROLES: &[(u32, InterfaceRole)] = &[
    (QDK_INTERFACE_ARGUMENT, InterfaceRole::Argument),
    (QDK_INTERFACE_RESULT, InterfaceRole::Result),
    (QDK_INTERFACE_EXIT, InterfaceRole::Exit),
    (QDK_INTERFACE_CAPTURE, InterfaceRole::Capture),
    (QDK_INTERFACE_CARRIED, InterfaceRole::Carried),
];
pub(super) const FIR_KINDS: &[(u32, FirOriginKind)] = &[
    (QDK_FIR_PACKAGE, FirOriginKind::Package),
    (QDK_FIR_ITEM, FirOriginKind::Item),
    (QDK_FIR_BLOCK, FirOriginKind::Block),
    (QDK_FIR_EXPRESSION, FirOriginKind::Expression),
    (QDK_FIR_PATTERN, FirOriginKind::Pattern),
    (QDK_FIR_STATEMENT, FirOriginKind::Statement),
];

pub(super) fn decode<T: Copy>(tag: u32, table: &[(u32, T)]) -> Result<T, Error> {
    table
        .iter()
        .find_map(|&(key, value)| (key == tag).then_some(value))
        .ok_or_else(|| Error::invalid(format!("unknown graph tag {tag}")))
}

pub(super) fn encode<T: Copy + PartialEq>(value: T, table: &[(u32, T)]) -> Result<u32, Error> {
    table
        .iter()
        .find_map(|(tag, item)| (*item == value).then_some(*tag))
        .ok_or_else(|| {
            Error::new(
                crate::QDK_INTERNAL_ERROR,
                "native graph tag has no C mapping",
            )
        })
}

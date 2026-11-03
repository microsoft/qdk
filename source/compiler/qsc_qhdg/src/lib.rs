// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod alias;
pub mod analysis;
pub mod analysis_demand;
pub mod certificate;
pub mod cleanup;
pub mod effects;
pub mod graph;
pub mod graph_adjacency;
pub mod graph_certificate;
pub mod graph_discharge;
pub mod graph_rewrite;
pub mod graph_rewrites;
pub mod graph_snapshot;
mod graph_snapshot_binary;
mod graph_snapshot_text;
pub mod graph_transaction;
pub mod identity;
pub mod ownership;
pub mod payload;
pub mod region;
pub mod rewrite;
pub mod verify;
pub mod vocabulary;
pub mod wire;

pub use alias::{AliasDecision, AliasFacts, AliasView};
pub use analysis::{
    Analysis, AnalysisCache, AnalysisContext, AnalysisError, AnalysisKey, AnalysisKind,
    AnalysisManager, AnalysisOutput, AnalysisScope, AnalysisScopeKind, AnalysisValue,
    CachedAnalysis,
};
pub use analysis_demand::Demand;
pub use certificate::{CertificateError, REWRITE_CERTIFICATE_VERSION};
pub use cleanup::{CleanupError, CleanupJoin, CleanupState, ExitKind as CleanupExitKind};
pub use effects::{EffectEvent, EffectResource};
pub use graph::{
    BuildError, Direction, Edge, EdgeKind, Graph, GraphBuilder, NodeId, PortId, PortKind, Stage,
    Usage,
};
pub use graph_adjacency::Adjacency;
pub use graph_certificate::{GraphBoundary, GraphRewriteCertificate, ProofObligation};
pub use graph_discharge::{DischargeContext, InverseVocabulary};
pub use graph_rewrite::{
    CreatedNode, CreatedPort, GraphRewriteDriver, GraphRewriteError, GraphRewriteMatch,
    GraphRewriteOutcome, GraphRewritePatch, GraphRewriteRule, NodeContent, PatchEdge, PatchPort,
    PayloadChange, SymbolChange, apply_certified_patch,
};
pub use graph_rewrites::{
    ConstantGamma, DemandDce, DiagonalCollapse, DisabledConstruct, DisjointGateMotion,
    InverseCancellation, IslandClass, IslandError, OptimizationIsland, PureCne,
    graph_rewrite_rules,
};
pub use graph_snapshot::{
    CONTRACT_REGISTRY_VERSION, GRAPH_SNAPSHOT_SCHEMA_VERSION, MAX_SNAPSHOT_BYTES, SnapshotError,
    canonical_graph_snapshot, decode_graph_snapshot, graph_digest, graph_snapshot_text,
    region_digest,
};
pub use graph_transaction::{GraphFootprint, GraphTransaction, GraphTransactionError};
pub use identity::{
    IdentityError, IdentityOperation, IdentityTransition, NodeHandle, ResourceId, StableNodeId,
    SymbolId,
};
pub use ownership::{
    Authority, Borrow, BorrowKind, Epoch, OwnerPath, OwnershipError, OwnershipState, PathSelector,
    SplitCertificate,
};
pub use payload::{Angle, AngleOp, FirOriginKind, NodeOrigin, NodePayload, NodeValue, Real};
pub use qsc_semantic_contracts::{EffectDomain, EffectMode, OperationIdentity, Registry, Type};
pub use region::{
    ExitKind, InterfaceBinding, InterfaceRole, Region, RegionId, RegionInterface, RegionKind,
};
pub use rewrite::{
    EffectOrderProof, GlobalPhasePolicy, Observation, OperationClass, OperationFacts,
    RewriteContext, RewriteDiagnostic,
};
pub use verify::{Diagnostic, RuleCode, declares_backedge, verify, verify_scoped};
pub use vocabulary::{VocabularyError, accept_producer, admits};
pub use wire::{Operand, OperandWires, WireIdentity};

#[cfg(test)]
mod tests;

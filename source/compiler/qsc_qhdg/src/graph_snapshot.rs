// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Typed graph checkpoints and scope-separated canonical fingerprints.

use crate::{graph::Graph, graph_adjacency::Adjacency, identity::StableNodeId, verify::verify};
use crate::{graph_snapshot_binary as codec, graph_snapshot_text as text};
use qsc_snapshot::{Format, Kind, binary, snapshot_capnp as wire};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub use qsc_snapshot::MAX_SNAPSHOT_BYTES;
pub const GRAPH_SNAPSHOT_SCHEMA_VERSION: u32 = qsc_snapshot::SCHEMA_VERSION;
pub const CONTRACT_REGISTRY_VERSION: u32 = 1;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SnapshotError {
    #[error(transparent)]
    Encoding(#[from] qsc_snapshot::Error),
    #[error("snapshot exceeds the byte limit")]
    ByteLimit,
    #[error("snapshot is malformed")]
    Malformed,
    #[error("snapshot schema version is unsupported")]
    SchemaVersion,
    #[error("snapshot requires an unknown feature: {0}")]
    UnknownRequiredFeature(String),
    #[error("snapshot digest does not match its payload")]
    DigestMismatch,
    #[error("snapshot graph verification failed")]
    Verification,
}

pub(crate) struct Payload {
    pub version: u32,
    pub registry_version: u32,
    pub required_features: BTreeSet<String>,
    pub optional_features: BTreeSet<String>,
    pub graph: Graph,
}

pub(crate) fn hash<'a, T: capnp::traits::Owned>(
    domain: &[u8],
    root: impl capnp::traits::SetterInput<T> + Copy + Into<capnp::dynamic_value::Reader<'a>>,
) -> Result<[u8; 32], qsc_snapshot::Error> {
    let canonical = binary::canonical::<T>(root)?;
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(canonical.get_segments_for_output()[0]);
    Ok(digest.finalize().into())
}

fn snapshot_message(graph: &Graph) -> Result<binary::Message, qsc_snapshot::Error> {
    let mut message = binary::Message::new_default();
    let mut document = message.init_root::<wire::document::Builder>();
    document.set_version(qsc_snapshot::MODEL_VERSION);
    let mut snapshot = document.init_graph();
    codec::write_payload(
        snapshot.reborrow().init_payload(),
        graph,
        graph.revision,
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    let digest = hash::<wire::graph_payload::Owned>(
        b"qdk.graph.snapshot.v1\0",
        snapshot.reborrow_as_reader().get_payload()?,
    )?;
    snapshot.set_digest(&digest);
    Ok(message)
}

pub fn canonical_graph_snapshot(graph: &Graph) -> Result<Vec<u8>, SnapshotError> {
    Ok(binary::encode(Kind::Graph, &snapshot_message(graph)?)?)
}

pub fn graph_snapshot_text(graph: &Graph) -> Result<String, SnapshotError> {
    let mut payload = binary::Message::new_default();
    codec::write_payload(
        payload.init_root(),
        graph,
        graph.revision,
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    let digest = hash::<wire::graph_payload::Owned>(
        b"qdk.graph.snapshot.v1\0",
        payload
            .get_root_as_reader::<wire::graph_payload::Reader>()
            .map_err(qsc_snapshot::Error::from)?,
    )?;
    Ok(text::encode(graph, &digest)?)
}

/// Content includes checkpoint metadata and allocation state, but not revision.
pub fn graph_digest(graph: &Graph) -> Result<String, SnapshotError> {
    let mut message = binary::Message::new_default();
    codec::write_payload(
        message.init_root(),
        graph,
        0,
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    let digest = hash::<wire::graph_payload::Owned>(
        b"qdk.graph.content.v1\0",
        message
            .get_root_as_reader::<wire::graph_payload::Reader>()
            .map_err(qsc_snapshot::Error::from)?,
    )?;
    Ok(hex(&digest))
}

/// Only matched nodes, their ordered ports, and their incident edges are visited.
pub fn region_digest(
    graph: &Graph,
    adjacency: &Adjacency,
    matched: &BTreeSet<StableNodeId>,
) -> Result<String, SnapshotError> {
    let mut message = binary::Message::new_default();
    codec::write_match(message.init_root(), graph, adjacency, matched)?;
    let digest = hash::<wire::region_fingerprint::Owned>(
        b"qdk.graph.match.v1\0",
        message
            .get_root_as_reader::<wire::region_fingerprint::Reader>()
            .map_err(qsc_snapshot::Error::from)?,
    )?;
    Ok(hex(&digest))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("String write");
    }
    text
}

pub fn decode_graph_snapshot(bytes: &[u8]) -> Result<Graph, SnapshotError> {
    let (format, bytes_payload) = qsc_snapshot::payload(Kind::Graph, bytes)?;
    let (payload, digest) = match format {
        Format::Binary => {
            let (message, mut budget) = binary::decode(Kind::Graph, bytes_payload)?;
            let document = message
                .get_root::<wire::document::Reader>()
                .map_err(qsc_snapshot::Error::from)?;
            let wire::document::Graph(snapshot) =
                document.which().map_err(qsc_snapshot::Error::from)?
            else {
                return Err(SnapshotError::Malformed);
            };
            let snapshot = snapshot.map_err(qsc_snapshot::Error::from)?;
            let source = snapshot.get_payload().map_err(qsc_snapshot::Error::from)?;
            let payload = codec::read_payload(source, &mut budget)?;
            let digest: [u8; 32] = snapshot
                .get_digest()
                .map_err(qsc_snapshot::Error::from)?
                .try_into()
                .map_err(|_| SnapshotError::Malformed)?;
            let mut projected = binary::Message::new_default();
            let mut root = projected.init_root::<wire::document::Builder>();
            root.set_version(qsc_snapshot::MODEL_VERSION);
            let mut out = root.init_graph();
            codec::write_payload(
                out.reborrow().init_payload(),
                &payload.graph,
                payload.graph.revision,
                &payload.required_features,
                &payload.optional_features,
            )?;
            out.set_digest(&digest);
            binary::check_projection::<wire::document::Owned>(
                document,
                projected
                    .get_root_as_reader::<wire::document::Reader>()
                    .map_err(qsc_snapshot::Error::from)?,
            )?;
            (payload, digest)
        }
        Format::Text => text::decode(bytes)?,
    };
    check_metadata(
        payload.version,
        payload.registry_version,
        &payload.required_features,
    )?;
    let mut canonical = binary::Message::new_default();
    codec::write_payload(
        canonical.init_root(),
        &payload.graph,
        payload.graph.revision,
        &payload.required_features,
        &payload.optional_features,
    )?;
    let expected = hash::<wire::graph_payload::Owned>(
        b"qdk.graph.snapshot.v1\0",
        canonical
            .get_root_as_reader::<wire::graph_payload::Reader>()
            .map_err(qsc_snapshot::Error::from)?,
    )?;
    if digest != expected {
        return Err(SnapshotError::DigestMismatch);
    }
    check_graph(&payload.graph)?;
    if !verify(&payload.graph).is_empty() {
        return Err(SnapshotError::Verification);
    }
    Ok(payload.graph)
}

pub(crate) fn check_metadata(
    version: u32,
    registry: u32,
    required: &BTreeSet<String>,
) -> Result<(), SnapshotError> {
    if version != GRAPH_SNAPSHOT_SCHEMA_VERSION || registry != CONTRACT_REGISTRY_VERSION {
        return Err(SnapshotError::SchemaVersion);
    }
    if let Some(feature) = required.first() {
        return Err(SnapshotError::UnknownRequiredFeature(feature.clone()));
    }
    Ok(())
}

pub(crate) fn unique_map<K: Ord, V>(
    items: Vec<V>,
    key: impl Fn(&V) -> K,
) -> Result<BTreeMap<K, V>, qsc_snapshot::Error> {
    let len = items.len();
    let result: BTreeMap<_, _> = items
        .into_iter()
        .map(|value| (key(&value), value))
        .collect();
    if result.len() != len {
        return Err(qsc_snapshot::Error::malformed("duplicate record identity"));
    }
    Ok(result)
}

pub(crate) fn unique_set<T: Ord>(items: Vec<T>) -> Result<BTreeSet<T>, qsc_snapshot::Error> {
    let len = items.len();
    let result: BTreeSet<_> = items.into_iter().collect();
    if result.len() != len {
        return Err(qsc_snapshot::Error::malformed("duplicate set element"));
    }
    Ok(result)
}

fn check_graph(graph: &Graph) -> Result<(), SnapshotError> {
    let (live, _, _, _, _, _) = graph.identity.snapshot_parts();
    if !live.keys().eq(graph.nodes.keys())
        || graph.regions.keys().any(|id| id.0 >= graph.next_region)
        || graph.ports.keys().any(|id| id.0 >= graph.next_port)
        || graph
            .demanded_nodes
            .iter()
            .any(|id| !graph.nodes.contains_key(id))
        || graph
            .demanded_nodes
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || graph.edges.windows(2).any(|pair| pair[0] >= pair[1])
        || graph.nodes.values().any(|node| {
            !graph.regions.contains_key(&node.region)
                || node
                    .inputs
                    .iter()
                    .chain(&node.outputs)
                    .any(|id| !graph.ports.contains_key(id))
        })
        || graph.ports.values().any(|port| {
            !graph.nodes.contains_key(&port.node) || !graph.regions.contains_key(&port.region)
        })
        || graph.edges.iter().any(|edge| {
            !graph.ports.contains_key(&edge.source) || !graph.ports.contains_key(&edge.target)
        })
    {
        return Err(SnapshotError::Malformed);
    }
    let mut owned_ports = BTreeSet::new();
    for node in graph.nodes.values() {
        for (ids, direction) in [
            (&node.inputs, crate::Direction::Input),
            (&node.outputs, crate::Direction::Output),
        ] {
            for id in ids {
                let port = graph.ports.get(id).ok_or(SnapshotError::Malformed)?;
                if !owned_ports.insert(*id)
                    || port.node != node.id
                    || port.region != node.region
                    || port.direction != direction
                {
                    return Err(SnapshotError::Malformed);
                }
            }
        }
    }
    if owned_ports.len() != graph.ports.len() {
        return Err(SnapshotError::Malformed);
    }
    let mut checked_regions = BTreeSet::new();
    for region in graph.regions.values() {
        if region
            .owner
            .is_some_and(|id| !graph.nodes.contains_key(&id))
            || region
                .interface
                .arguments
                .iter()
                .chain(&region.interface.results)
                .chain(region.interface.exits.iter().map(|(_, id)| id))
                .chain(
                    region
                        .interface
                        .bindings
                        .iter()
                        .flat_map(|binding| [&binding.outer, &binding.inner]),
                )
                .any(|id| !graph.ports.contains_key(id))
        {
            return Err(SnapshotError::Malformed);
        }
        let mut seen = BTreeSet::from([region.id]);
        let mut parent = region.parent;
        while let Some(id) = parent {
            if checked_regions.contains(&id) {
                break;
            }
            if !seen.insert(id) {
                return Err(SnapshotError::Malformed);
            }
            parent = graph
                .regions
                .get(&id)
                .ok_or(SnapshotError::Malformed)?
                .parent;
        }
        checked_regions.extend(seen);
    }
    Ok(())
}

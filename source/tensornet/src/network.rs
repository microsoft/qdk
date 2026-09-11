use std::collections::BTreeMap;

use crate::{Index, Indices, NetworkError};

/// A tensor network: nodes whose axes are joined by index identity.
///
/// There is no join operation, because joining is not something done *to* the
/// nodes — it is what carrying the same index already means. A network is
/// therefore just the nodes, and every question about its topology is answered
/// by counting where each index occurs.
///
/// A network says nothing about what should be computed from it. That belongs
/// to a [`crate::ContractionQuery`], and one network admits many.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TensorNetwork {
    nodes: Vec<Indices>,
}

impl TensorNetwork {
    /// Describes a network from its nodes.
    ///
    /// Each node is already consistent on its own terms; what is checked here
    /// is that the nodes agree with each other about how long each shared axis
    /// is.
    ///
    /// A network with no nodes is allowed: the empty product is the scalar
    /// one. It is unlikely to be what a caller meant, but it is well defined,
    /// and a backend that cannot contract it is the right place to say so.
    pub fn new(nodes: Vec<Indices>) -> Result<Self, NetworkError> {
        let mut dimensions: BTreeMap<u32, usize> = BTreeMap::new();
        for node in &nodes {
            for axis in node.as_slice() {
                if let Some(&known) = dimensions.get(&axis.id()) {
                    if known != axis.dim() {
                        return Err(NetworkError::InconsistentDimension {
                            id: axis.id(),
                            first: known,
                            second: axis.dim(),
                        });
                    }
                } else {
                    dimensions.insert(axis.id(), axis.dim());
                }
            }
        }
        Ok(Self { nodes })
    }

    /// The nodes, in the order they were given.
    #[must_use]
    pub fn nodes(&self) -> &[Indices] {
        &self.nodes
    }

    /// How many axis slots `index` occupies across the network.
    ///
    /// Multiplicity counts: a node carrying the same axis twice contributes
    /// two, which is what makes a trace need no special case.
    #[must_use]
    pub fn incidence(&self, index: Index) -> usize {
        self.nodes
            .iter()
            .map(|node| {
                node.as_slice()
                    .iter()
                    .filter(|axis| **axis == index)
                    .count()
            })
            .sum()
    }

    /// Every distinct axis of the network with the number of slots it
    /// occupies, ascending by id.
    ///
    /// One pass, so that classifying a whole network does not cost a scan per
    /// axis.
    pub(crate) fn incidences(&self) -> BTreeMap<Index, usize> {
        let mut counts = BTreeMap::new();
        for node in &self.nodes {
            for axis in node.as_slice() {
                *counts.entry(*axis).or_insert(0) += 1;
            }
        }
        counts
    }

    /// The dimension the network gives to `id`, if it carries it at all.
    pub(crate) fn dim_of(&self, id: u32) -> Option<usize> {
        self.nodes
            .iter()
            .flat_map(Indices::as_slice)
            .find(|axis| axis.id() == id)
            .map(|axis| axis.dim())
    }
}

#[cfg(test)]
mod tests {
    use super::TensorNetwork;
    use crate::{Index, Indices, NetworkError};

    fn index(id: u32, dim: usize) -> Index {
        Index::new(id, dim).expect("dimension is non-zero")
    }

    fn node(axes: &[Index]) -> Indices {
        Indices::new(axes.to_vec()).expect("axis list is consistent")
    }

    fn network(nodes: Vec<Indices>) -> TensorNetwork {
        TensorNetwork::new(nodes).expect("nodes agree on dimensions")
    }

    #[test]
    fn a_network_keeps_its_nodes_in_order() {
        let first = node(&[index(0, 2), index(1, 3)]);
        let second = node(&[index(1, 3), index(2, 4)]);
        let net = network(vec![first.clone(), second.clone()]);
        assert_eq!(net.nodes(), [first, second]);
    }

    #[test]
    fn nodes_must_agree_about_a_shared_axis() {
        let first = node(&[index(0, 2), index(1, 3)]);
        let second = node(&[index(1, 7)]);
        assert_eq!(
            TensorNetwork::new(vec![first, second]),
            Err(NetworkError::InconsistentDimension {
                id: 1,
                first: 3,
                second: 7,
            })
        );
    }

    #[test]
    fn a_network_with_no_nodes_is_allowed() {
        let net = network(vec![]);
        assert!(net.nodes().is_empty());
    }

    #[test]
    fn a_network_of_one_scalar_node_is_allowed() {
        let net = network(vec![node(&[])]);
        assert_eq!(net.nodes().len(), 1);
        assert!(net.nodes()[0].as_slice().is_empty());
    }

    #[test]
    fn incidence_counts_slots_across_nodes() {
        let shared = index(1, 3);
        let net = network(vec![
            node(&[index(0, 2), shared]),
            node(&[shared, index(2, 4)]),
        ]);
        assert_eq!(net.incidence(shared), 2, "one slot in each node");
        assert_eq!(net.incidence(index(0, 2)), 1, "a dangling axis");
        assert_eq!(net.incidence(index(9, 2)), 0, "not in the network");
    }

    #[test]
    fn incidence_counts_a_repeat_within_one_node_twice() {
        let repeated = index(0, 4);
        let net = network(vec![node(&[repeated, repeated])]);
        assert_eq!(net.incidence(repeated), 2);
    }
}

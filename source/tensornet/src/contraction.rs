use crate::{ContractionError, Index, Indices, TensorNetwork};

/// A request to contract a network down to a chosen list of axes.
///
/// The network decides topology; the query decides summation. An axis is
/// summed over exactly when `keep` leaves it out, which is einsum's rule:
///
/// ```text
/// Result[coords on keep] =  Σ           ∏  node[ assignment restricted to it ]
///                      assignments to    i
///                       everything else
/// ```
///
/// A query *refers to* a network rather than owning one, because one network
/// admits many contractions and `keep` is what selects between them. Keeping
/// `[i, k]` of `A[i,j] B[j,k]` is a matrix product; keeping `[i, j, k]` sums
/// nothing and makes `j` a hyperedge; keeping nothing yields a scalar.
///
/// A query yields a plan, not another network. Contraction does not nest: an
/// inner contraction would fix *when* a sum happens, and choosing that order
/// is the planner's job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractionQuery<'a> {
    network: &'a TensorNetwork,
    keep: Indices,
}

impl<'a> ContractionQuery<'a> {
    /// Asks for the axes in `keep` to survive, in the order given.
    ///
    /// Everything else the network carries is summed over. Nothing needs to be
    /// said about *which* axes those are, because the network and `keep`
    /// already determine it — see [`ContractionQuery::marginalized`] for the
    /// one case where that inference is worth checking.
    pub fn new(network: &'a TensorNetwork, keep: Indices) -> Result<Self, ContractionError> {
        for (position, axis) in keep.as_slice().iter().enumerate() {
            match network.dim_of(axis.id()) {
                None => return Err(ContractionError::UnknownKeptIndex { id: axis.id() }),
                Some(dimension) if dimension != axis.dim() => {
                    return Err(ContractionError::InconsistentDimension {
                        id: axis.id(),
                        network: dimension,
                        kept: axis.dim(),
                    });
                }
                Some(_) => {}
            }
            if keep.as_slice()[position + 1..]
                .iter()
                .any(|other| other.id() == axis.id())
            {
                return Err(ContractionError::RepeatedKeptIndex { id: axis.id() });
            }
        }
        Ok(Self { network, keep })
    }

    /// The network this query refers to.
    #[must_use]
    pub fn network(&self) -> &TensorNetwork {
        self.network
    }

    /// The axes of the result, in order.
    ///
    /// This is also the layout of the buffer a caller has to allocate, so
    /// `query.keep().strides()` needs no separate derivation.
    #[must_use]
    pub fn keep(&self) -> &Indices {
        &self.keep
    }

    /// How many slots an axis occupies once the result is counted as one more
    /// operand.
    fn total_incidence(&self, index: Index) -> usize {
        self.network.incidence(index) + usize::from(self.keep.contains(index))
    }

    /// The axes that are not ordinary edges, ascending by id.
    ///
    /// An axis occupying exactly two slots — counting the result as one — is
    /// an ordinary edge. Anything else is a hyperedge: three or more slots is
    /// a copy (a batch axis, a diagonal, an axis shared by three nodes), and
    /// one slot is a marginalization.
    ///
    /// Hyperedges are legal here, and for a diagonal gate they are the whole
    /// point: `RZZ` as a hyperedge is four elements rather than sixteen. A
    /// backend that cannot take them rewrites them into ordinary edges with
    /// explicit copy nodes, which is an adapter's business rather than this
    /// crate's.
    #[must_use]
    pub fn hyperedges(&self) -> Indices {
        Indices::from_validated(
            self.network
                .incidences()
                .into_keys()
                .filter(|axis| self.total_incidence(*axis) != 2)
                .collect(),
        )
    }

    /// The axes that are summed on their own, ascending by id.
    ///
    /// An axis occupying one slot and not kept is marginalized: summed without
    /// being contracted against anything. That is a legitimate operation, and
    /// also what a builder produces when it fails to join two nodes that
    /// should have shared an axis — the expression still runs and still has
    /// the right shape, but the numbers are wrong.
    ///
    /// The fact is derived rather than declared, because the network and
    /// `keep` already determine it and a redundant declaration produced by the
    /// same builder would simply agree with the bug. Callers that know their
    /// own domain assert on it instead: a circuit network never marginalizes,
    /// so a circuit builder checks that this is empty.
    ///
    /// This catches accidental marginalization. It does not catch a
    /// wrong-but-paired axis, which stays a wrong answer until a reference
    /// contractor disagrees with the result.
    #[must_use]
    pub fn marginalized(&self) -> Indices {
        Indices::from_validated(
            self.network
                .incidences()
                .into_iter()
                .filter(|(axis, slots)| *slots == 1 && !self.keep.contains(*axis))
                .map(|(axis, _)| axis)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ContractionQuery;
    use crate::{ContractionError, Index, Indices, TensorNetwork};

    fn index(id: u32, dim: usize) -> Index {
        Index::new(id, dim).expect("dimension is non-zero")
    }

    fn node(axes: &[Index]) -> Indices {
        Indices::new(axes.to_vec()).expect("axis list is consistent")
    }

    fn network(nodes: Vec<Indices>) -> TensorNetwork {
        TensorNetwork::new(nodes).expect("nodes agree on dimensions")
    }

    fn query<'a>(net: &'a TensorNetwork, keep: &[Index]) -> ContractionQuery<'a> {
        ContractionQuery::new(net, node(keep)).expect("kept axes are in the network")
    }

    /// `A[i,j] B[j,k]`, the running example: `i` and `k` dangle, `j` is shared.
    fn matmul() -> (TensorNetwork, Index, Index, Index) {
        let i = index(0, 2);
        let j = index(1, 3);
        let k = index(2, 4);
        (network(vec![node(&[i, j]), node(&[j, k])]), i, j, k)
    }

    #[test]
    fn a_query_keeps_the_axes_it_was_given_in_order() {
        let (net, i, _, k) = matmul();
        let transposed = query(&net, &[k, i]);
        assert_eq!(transposed.keep().as_slice(), [k, i]);
        assert_eq!(transposed.network(), &net);
    }

    #[test]
    fn one_network_serves_many_queries() {
        let (net, i, j, k) = matmul();
        let product = query(&net, &[i, k]);
        let open = query(&net, &[i, j, k]);
        let scalar = query(&net, &[]);
        assert_eq!(product.keep().element_count(), Some(8));
        assert_eq!(open.keep().element_count(), Some(24));
        assert_eq!(scalar.keep().element_count(), Some(1));
    }

    #[test]
    fn an_axis_the_network_does_not_carry_cannot_be_kept() {
        let (net, i, _, _) = matmul();
        assert_eq!(
            ContractionQuery::new(&net, node(&[i, index(9, 2)])),
            Err(ContractionError::UnknownKeptIndex { id: 9 })
        );
    }

    #[test]
    fn an_axis_cannot_be_kept_twice() {
        let (net, i, _, _) = matmul();
        assert_eq!(
            ContractionQuery::new(&net, node(&[i, i])),
            Err(ContractionError::RepeatedKeptIndex { id: 0 })
        );
    }

    #[test]
    fn a_kept_axis_must_have_the_dimension_the_network_gives_it() {
        let (net, _, _, _) = matmul();
        assert_eq!(
            ContractionQuery::new(&net, node(&[index(0, 5)])),
            Err(ContractionError::InconsistentDimension {
                id: 0,
                network: 2,
                kept: 5,
            })
        );
    }

    #[test]
    fn a_shared_axis_that_is_dropped_is_an_ordinary_edge() {
        let (net, i, _, k) = matmul();
        let product = query(&net, &[i, k]);
        assert!(product.hyperedges().as_slice().is_empty());
        assert!(product.marginalized().as_slice().is_empty());
    }

    #[test]
    fn a_shared_axis_that_is_kept_is_a_hyperedge() {
        let (net, i, j, k) = matmul();
        let open = query(&net, &[i, j, k]);
        assert_eq!(
            open.hyperedges().as_slice(),
            [j],
            "two nodes plus the result"
        );
        assert!(open.marginalized().as_slice().is_empty());
    }

    #[test]
    fn a_batch_axis_is_a_hyperedge() {
        let b = index(0, 5);
        let i = index(1, 2);
        let j = index(2, 3);
        let k = index(3, 4);
        let net = network(vec![node(&[b, i, j]), node(&[b, j, k])]);
        let batched = query(&net, &[b, i, k]);
        assert_eq!(batched.hyperedges().as_slice(), [b]);
    }

    #[test]
    fn an_axis_on_three_nodes_is_a_hyperedge() {
        let i = index(0, 2);
        let j = index(1, 3);
        let k = index(2, 4);
        let l = index(3, 5);
        let net = network(vec![node(&[i, j]), node(&[j, k]), node(&[j, l])]);
        let copied = query(&net, &[i, k, l]);
        assert_eq!(copied.hyperedges().as_slice(), [j]);
        assert!(copied.marginalized().as_slice().is_empty());
    }

    #[test]
    fn a_diagonal_is_a_hyperedge() {
        let i = index(0, 4);
        let net = network(vec![node(&[i, i])]);
        let diagonal = query(&net, &[i]);
        assert_eq!(
            diagonal.hyperedges().as_slice(),
            [i],
            "two slots plus the result"
        );
    }

    #[test]
    fn a_trace_is_an_ordinary_edge() {
        let i = index(0, 4);
        let net = network(vec![node(&[i, i])]);
        let trace = query(&net, &[]);
        assert!(trace.hyperedges().as_slice().is_empty());
        assert!(trace.marginalized().as_slice().is_empty());
    }

    #[test]
    fn an_axis_on_one_node_and_not_kept_is_marginalized() {
        let i = index(0, 2);
        let j = index(1, 3);
        let net = network(vec![node(&[i, j])]);
        let summed = query(&net, &[i]);
        assert_eq!(summed.marginalized().as_slice(), [j]);
        assert_eq!(
            summed.hyperedges().as_slice(),
            [j],
            "one slot is not an edge"
        );
    }

    #[test]
    fn a_wire_that_was_never_joined_shows_up_as_marginalized() {
        // The builder bug: the second node got a fresh axis instead of the
        // one the first node dangles, so nothing connects them.
        let i = index(0, 2);
        let j = index(1, 3);
        let stray = index(4, 3);
        let k = index(2, 4);
        let net = network(vec![node(&[i, j]), node(&[stray, k])]);
        let broken = query(&net, &[i, k]);
        assert_eq!(broken.marginalized().as_slice(), [j, stray]);
    }

    #[test]
    fn a_dangling_axis_that_is_kept_is_neither() {
        let (net, i, _, k) = matmul();
        let product = query(&net, &[i, k]);
        assert!(!product.hyperedges().contains(i));
        assert!(!product.marginalized().contains(k));
    }
}

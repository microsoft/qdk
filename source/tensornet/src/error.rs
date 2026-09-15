/// A network description that contradicts itself.
///
/// Both variants are raised while describing a network, before any backend is
/// involved, so the diagnostics stay free of vendor vocabulary.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NetworkError {
    /// An axis with no coordinates. A dimension of one is a legitimate axis;
    /// zero describes a tensor with no elements.
    #[error("index {id} has dimension zero: an axis needs at least one coordinate")]
    ZeroDimension { id: u32 },

    /// One identity carrying two different dimensions. Axes with the same id
    /// are the same axis, so they cannot disagree about how long they are.
    #[error("index {id} has dimension {first} in one place and {second} in another")]
    InconsistentDimension {
        id: u32,
        first: usize,
        second: usize,
    },
}

/// A contraction that cannot be asked of a network.
///
/// The network itself is already valid by the time a query is built, so these
/// variants only ever describe a disagreement between the network and the axes
/// a caller asked to keep.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ContractionError {
    /// An axis was asked for that the network does not carry.
    #[error("kept index {id} does not occur in the network")]
    UnknownKeptIndex { id: u32 },

    /// The same axis was asked for twice. A result cannot have two axes that
    /// are the same axis; einsum rejects `->ikk` for the same reason.
    #[error("kept index {id} is listed more than once")]
    RepeatedKeptIndex { id: u32 },

    /// An axis was asked for with a different dimension than the network gives
    /// it. Same identity, so it must be the same length.
    #[error("kept index {id} has dimension {kept} but the network gives it {network}")]
    InconsistentDimension {
        id: u32,
        network: usize,
        kept: usize,
    },
}

/// A chain that is not a matrix product state.
///
/// Every variant describes a shape that contradicts the chain structure
/// itself, so none of them mention a backend, a buffer or a memory layout.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MpsError {
    /// A chain with no sites. A matrix product state factorizes something, and
    /// there is nothing to factorize here.
    #[error("a matrix product state needs at least one site")]
    Empty,

    /// A site with the wrong number of axes. The ends of a chain carry one
    /// bond and the interior carries two, so rank is fixed by position.
    #[error("site {site} has {actual} axes but its position in the chain gives it {expected}")]
    Rank {
        site: usize,
        expected: usize,
        actual: usize,
    },

    /// An axis with no coordinates, which would leave the site — and so the
    /// whole state — with no elements.
    #[error("site {site} has an axis of extent zero")]
    ZeroExtent { site: usize },

    /// Two neighbours disagreeing about the bond they share. The right-hand
    /// axis of one site and the left-hand axis of the next are one bond named
    /// twice, so they cannot differ.
    #[error("the bond between sites {cut} and {} is {left} on one side and {right} on the other", cut + 1)]
    BondMismatch {
        cut: usize,
        left: usize,
        right: usize,
    },

    /// A site whose extents multiply out past what this machine can count.
    /// Rejected while describing the chain, so that every later element count
    /// is known to be answerable.
    #[error("site {site} has more elements than fit a machine word")]
    ElementCountOverflow { site: usize },
}

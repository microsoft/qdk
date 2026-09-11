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

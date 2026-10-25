//! [`MpsShapeError`]: why a list of site extents is not the shape of a
//! matrix product state.
//!
//! Every error is raised while describing, before any backend is involved, so
//! its message carries no vendor vocabulary.

/// Why a list of site extents is not the shape of a matrix product state.
///
/// Every variant describes a shape that contradicts the chain structure
/// itself, so none of them mention a backend, a buffer or a memory layout.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MpsShapeError {
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

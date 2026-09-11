use crate::NetworkError;

/// One axis of a tensor: an identity, and how many coordinates it has.
///
/// Two axes carrying the same id *are* the same axis, whether they occur in
/// one node or in different ones. That is the only mechanism by which a
/// network is joined — there is no separate connect operation, and a repeated
/// index is identification rather than contraction.
///
/// Identity is the caller's to choose, and nothing is implied by the numeric
/// order of ids or by the gaps between them.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Index {
    id: u32,
    dim: usize,
}

impl Index {
    /// Describes an axis with `dim` coordinates.
    pub fn new(id: u32, dim: usize) -> Result<Self, NetworkError> {
        if dim == 0 {
            return Err(NetworkError::ZeroDimension { id });
        }
        Ok(Self { id, dim })
    }

    /// What makes this axis the same axis wherever it occurs.
    #[must_use]
    pub fn id(self) -> u32 {
        self.id
    }

    /// How many coordinates this axis has.
    #[must_use]
    pub fn dim(self) -> usize {
        self.dim
    }
}

/// An ordered list of axes: one node's axes, or the axes of a result.
///
/// Order matters. For a node it is the layout of the stored tensor; for a
/// result it is the order a caller will read the axes in, which is why `keep`
/// is a sequence and not a set.
///
/// Repeats are allowed, because a node may legitimately carry the same axis
/// twice: that is einsum's `ii`, a diagonal or a trace, stored densely as a
/// full square. A result may not repeat an axis, but that is a contraction
/// rule rather than a property of an axis list.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Indices {
    indices: Vec<Index>,
}

impl Indices {
    /// Describes an axis list.
    ///
    /// The one way such a list can contradict itself is by giving a single
    /// identity two different dimensions.
    pub fn new(indices: Vec<Index>) -> Result<Self, NetworkError> {
        for (position, axis) in indices.iter().enumerate() {
            for other in &indices[position + 1..] {
                if axis.id() == other.id() && axis.dim() != other.dim() {
                    return Err(NetworkError::InconsistentDimension {
                        id: axis.id(),
                        first: axis.dim(),
                        second: other.dim(),
                    });
                }
            }
        }
        Ok(Self { indices })
    }

    /// Builds a list whose invariant is already known to hold, for axes that
    /// were taken out of an existing network rather than supplied by a caller.
    pub(crate) fn from_validated(indices: Vec<Index>) -> Self {
        Self { indices }
    }

    /// The axes, in order.
    #[must_use]
    pub fn as_slice(&self) -> &[Index] {
        &self.indices
    }

    /// Whether this list carries `index`.
    #[must_use]
    pub fn contains(&self, index: Index) -> bool {
        self.indices.contains(&index)
    }

    /// How many elements a dense tensor with these axes holds.
    ///
    /// `None` when the product does not fit in a `usize`, which is an answer
    /// rather than a failure: no such buffer could be addressed, let alone
    /// allocated. A network whose result overflows here is exactly the kind a
    /// planner has to slice.
    #[must_use]
    pub fn element_count(&self) -> Option<usize> {
        self.indices
            .iter()
            .try_fold(1_usize, |count, axis| count.checked_mul(axis.dim()))
    }

    /// The column-major strides of a dense tensor with these axes: the first
    /// axis varies fastest.
    ///
    /// Column-major is what cuTensorNet means by a `NULL` strides pointer and
    /// what tensor4all's `ColMajorArray` uses, so this is the layout a backend
    /// is least likely to have to convert.
    #[must_use]
    pub fn strides(&self) -> Option<Vec<usize>> {
        let mut strides = Vec::with_capacity(self.indices.len());
        let mut stride = 1_usize;
        for axis in &self.indices {
            strides.push(stride);
            stride = stride.checked_mul(axis.dim())?;
        }
        Some(strides)
    }

    /// Where `coords` sits in a dense, column-major buffer.
    ///
    /// `None` when `coords` does not name one element of this tensor: the
    /// wrong number of coordinates, or one of them out of range.
    #[must_use]
    pub fn offset_of(&self, coords: &[usize]) -> Option<usize> {
        if coords.len() != self.indices.len() {
            return None;
        }
        let strides = self.strides()?;
        let mut offset = 0_usize;
        for ((coord, axis), stride) in coords.iter().zip(&self.indices).zip(&strides) {
            if *coord >= axis.dim() {
                return None;
            }
            offset = offset.checked_add(coord.checked_mul(*stride)?)?;
        }
        Some(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::{Index, Indices};
    use crate::NetworkError;

    fn index(id: u32, dim: usize) -> Index {
        Index::new(id, dim).expect("dimension is non-zero")
    }

    fn indices(axes: &[Index]) -> Indices {
        Indices::new(axes.to_vec()).expect("axis list is consistent")
    }

    #[test]
    fn an_axis_needs_at_least_one_coordinate() {
        assert_eq!(Index::new(7, 0), Err(NetworkError::ZeroDimension { id: 7 }));
        assert_eq!(index(7, 1).dim(), 1);
    }

    #[test]
    fn an_axis_reports_what_it_was_given() {
        let axis = index(3, 5);
        assert_eq!(axis.id(), 3);
        assert_eq!(axis.dim(), 5);
    }

    #[test]
    fn one_identity_cannot_carry_two_dimensions() {
        assert_eq!(
            Indices::new(vec![index(1, 2), index(1, 3)]),
            Err(NetworkError::InconsistentDimension {
                id: 1,
                first: 2,
                second: 3,
            })
        );
    }

    #[test]
    fn a_node_may_carry_the_same_axis_twice() {
        let trace = indices(&[index(1, 4), index(1, 4)]);
        assert_eq!(trace.as_slice().len(), 2);
        assert_eq!(trace.element_count(), Some(16));
    }

    #[test]
    fn an_empty_axis_list_is_a_scalar() {
        let scalar = indices(&[]);
        assert_eq!(scalar.element_count(), Some(1));
        assert_eq!(scalar.strides(), Some(vec![]));
        assert_eq!(scalar.offset_of(&[]), Some(0));
    }

    #[test]
    fn strides_are_column_major() {
        let axes = indices(&[index(0, 2), index(1, 3), index(2, 4)]);
        assert_eq!(axes.strides(), Some(vec![1, 2, 6]));
        assert_eq!(axes.element_count(), Some(24));
    }

    #[test]
    fn an_offset_is_the_column_major_position() {
        let axes = indices(&[index(0, 2), index(1, 3)]);
        // First axis varies fastest: (1, 2) sits at 1 + 2 * 2.
        assert_eq!(axes.offset_of(&[1, 2]), Some(5));
        assert_eq!(axes.offset_of(&[0, 0]), Some(0));
    }

    #[test]
    fn every_element_has_exactly_one_offset() {
        let axes = indices(&[index(0, 2), index(1, 3), index(2, 4)]);
        let mut seen = vec![false; 24];
        for first in 0..2 {
            for second in 0..3 {
                for third in 0..4 {
                    let offset = axes
                        .offset_of(&[first, second, third])
                        .expect("coordinates are in range");
                    assert!(!seen[offset], "offset {offset} was produced twice");
                    seen[offset] = true;
                }
            }
        }
        assert!(seen.into_iter().all(|hit| hit));
    }

    #[test]
    fn coordinates_that_name_no_element_have_no_offset() {
        let axes = indices(&[index(0, 2), index(1, 3)]);
        assert_eq!(axes.offset_of(&[0]), None, "too few coordinates");
        assert_eq!(axes.offset_of(&[0, 0, 0]), None, "too many coordinates");
        assert_eq!(axes.offset_of(&[2, 0]), None, "first axis out of range");
        assert_eq!(axes.offset_of(&[0, 3]), None, "second axis out of range");
    }

    #[test]
    fn a_result_too_large_to_address_has_no_element_count() {
        let qubits: Vec<Index> = (0..64).map(|id| index(id, 2)).collect();
        let axes = indices(&qubits);
        assert_eq!(axes.element_count(), None);
        assert_eq!(axes.strides(), None);
        assert_eq!(axes.offset_of(&vec![0; 64]), None);
    }

    #[test]
    fn containment_is_by_axis() {
        let axes = indices(&[index(0, 2), index(1, 3)]);
        assert!(axes.contains(index(1, 3)));
        assert!(!axes.contains(index(2, 3)));
    }
}

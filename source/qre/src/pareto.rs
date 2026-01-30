// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

pub trait ParetoItem2D {
    type Objective1: PartialOrd + Copy;
    type Objective2: PartialOrd + Copy;

    fn objective1(&self) -> Self::Objective1;
    fn objective2(&self) -> Self::Objective2;
}

pub trait ParetoItem3D {
    type Objective1: PartialOrd + Copy;
    type Objective2: PartialOrd + Copy;
    type Objective3: PartialOrd + Copy;

    fn objective1(&self) -> Self::Objective1;
    fn objective2(&self) -> Self::Objective2;
    fn objective3(&self) -> Self::Objective3;
}

/// A Pareto frontier for 2-dimensional objectives.
///
/// The implementation maintains the frontier sorted by the first objective.
/// This allows for efficient updates based on the geometric property that
/// a point is dominated if and only if it is dominated by its immediate
/// predecessor in the sorted list (when sorted by the first objective).
///
/// This approach is related to the algorithms described in:
/// H. T. Kung, F. Luccio, and F. P. Preparata, "On Finding the Maxima of a Set of Vectors,"
/// Journal of the ACM, vol. 22, no. 4, pp. 469-476, 1975.
#[derive(Default, Debug, Clone)]
pub struct ParetoFrontier<I: ParetoItem2D>(pub Vec<I>);

impl<I: ParetoItem2D> ParetoFrontier<I> {
    #[must_use]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn insert(&mut self, p: I) {
        // If any objective is incomparable (e.g. NaN), we silently ignore the item
        // to maintain the frontier's sorting invariant.
        if p.objective1().partial_cmp(&p.objective1()).is_none()
            || p.objective2().partial_cmp(&p.objective2()).is_none()
        {
            return;
        }

        let frontier = &mut self.0;
        let pos = frontier
            .binary_search_by(|q| {
                q.objective1()
                    .partial_cmp(&p.objective1())
                    .expect("objectives must be comparable")
            })
            .unwrap_or_else(|i| i);
        if pos > 0 {
            let left = &frontier[pos - 1];
            if left.objective2() <= p.objective2() {
                return;
            }
        }
        let i = pos;
        while i < frontier.len() && frontier[i].objective2() >= p.objective2() {
            frontier.remove(i);
        }
        frontier.insert(pos, p);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, I> {
        self.0.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<I: ParetoItem2D> Extend<I> for ParetoFrontier<I> {
    fn extend<T: IntoIterator<Item = I>>(&mut self, iter: T) {
        for p in iter {
            self.insert(p);
        }
    }
}

impl<I: ParetoItem2D> FromIterator<I> for ParetoFrontier<I> {
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        let mut frontier = Self::new();
        frontier.extend(iter);
        frontier
    }
}

impl<I: ParetoItem2D> IntoIterator for ParetoFrontier<I> {
    type Item = I;
    type IntoIter = std::vec::IntoIter<I>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, I: ParetoItem2D> IntoIterator for &'a ParetoFrontier<I> {
    type Item = &'a I;
    type IntoIter = std::slice::Iter<'a, I>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A Pareto frontier for 3-dimensional objectives.
///
/// The implementation maintains the frontier sorted lexicographically.
/// Unlike the 2D case where dominance checks are O(1) given the sorted order,
/// the 3D case requires checking the prefix or suffix to establish dominance,
/// though maintaining sorted order significantly reduces the search space.
///
/// The theoretical O(N log N) bound for constructing the 3D frontier is established in:
/// H. T. Kung, F. Luccio, and F. P. Preparata, "On Finding the Maxima of a Set of Vectors,"
/// Journal of the ACM, vol. 22, no. 4, pp. 469-476, 1975.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParetoFrontier3D<I: ParetoItem3D>(pub Vec<I>);

impl<I: ParetoItem3D> ParetoFrontier3D<I> {
    #[must_use]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn insert(&mut self, p: I) {
        // If any objective is incomparable (e.g. NaN), we silently ignore the item.
        if p.objective1().partial_cmp(&p.objective1()).is_none()
            || p.objective2().partial_cmp(&p.objective2()).is_none()
            || p.objective3().partial_cmp(&p.objective3()).is_none()
        {
            return;
        }

        let frontier = &mut self.0;

        // Use lexicographical sort covering all objectives.
        // This makes the binary search deterministic and ensures specific properties for prefix/suffix.
        let Err(pos) = frontier.binary_search_by(|q| {
            q.objective1()
                .partial_cmp(&p.objective1())
                .expect("objectives must be comparable")
                .then_with(|| {
                    q.objective2()
                        .partial_cmp(&p.objective2())
                        .expect("objectives must be comparable")
                })
                .then_with(|| {
                    q.objective3()
                        .partial_cmp(&p.objective3())
                        .expect("objectives must be comparable")
                })
        }) else {
            return;
        };

        // 1. Check if dominated by any existing point in the prefix [0..pos].
        // Because the list is sorted lexicographically, any point `q` before `pos`
        // satisfies `q.obj1 <= p.obj1` (often strictly less).
        // Therefore, we only need to check if `q` is also better in obj2 and obj3.
        for q in &frontier[..pos] {
            if q.objective2() <= p.objective2() && q.objective3() <= p.objective3() {
                return;
            }
        }

        // 2. Remove points dominated by the new point in the suffix [pos..].
        // Any point `q` at or after `pos` satisfies `p.obj1 <= q.obj1`.
        // So `p` can only dominate `q` if `p` is better in obj2 and obj3.
        let mut i = pos;
        while i < frontier.len() {
            let q = &frontier[i];
            if p.objective2() <= q.objective2() && p.objective3() <= q.objective3() {
                frontier.remove(i);
            } else {
                i += 1;
            }
        }

        frontier.insert(pos, p);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, I> {
        self.0.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<I: ParetoItem3D> Extend<I> for ParetoFrontier3D<I> {
    fn extend<T: IntoIterator<Item = I>>(&mut self, iter: T) {
        for p in iter {
            self.insert(p);
        }
    }
}

impl<I: ParetoItem3D> FromIterator<I> for ParetoFrontier3D<I> {
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        let mut frontier = Self::new();
        frontier.extend(iter);
        frontier
    }
}

impl<I: ParetoItem3D> IntoIterator for ParetoFrontier3D<I> {
    type Item = I;
    type IntoIter = std::vec::IntoIter<I>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, I: ParetoItem3D> IntoIterator for &'a ParetoFrontier3D<I> {
    type Item = &'a I;
    type IntoIter = std::slice::Iter<'a, I>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

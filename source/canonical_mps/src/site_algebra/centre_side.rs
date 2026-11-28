// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CentreSide`]: which site of a pair holds the orthogonality centre
//! after a gate split or a move of the centre.

/// The site that holds the orthogonality centre after a call. The other
/// site of the pair is an isometry towards it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CentreSide {
    /// The pair's left site.
    Left,
    /// The pair's right site.
    Right,
}

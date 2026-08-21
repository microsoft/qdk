// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{QubitId, SiteId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pauli {
    I,
    X,
    Y,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SitePauli {
    pub site: SiteId,
    pub pauli: Pauli,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PauliTerm {
    pub coefficient: f64,
    pub factors: Vec<(QubitId, Pauli)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PauliObservable {
    pub terms: Vec<PauliTerm>,
}

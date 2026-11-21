// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The value a part states of each resource or feature: [`Statements`],
//! the one rule every kind follows, and each value required above the one
//! provided ([`StatementMismatch`]).

use std::collections::BTreeMap;

/// A value per key a part states, such as an amount per
/// [`Resource`](super::Resource) or a yes-or-no per
/// [`Feature`](super::Feature).
///
/// Every kind follows one rule: a required value fits a provided one when
/// it is no larger, so a feature required `true` fits unless the provider
/// states `false`; a key a part does not state, it neither needs nor
/// limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statements<K, V> {
    values: BTreeMap<K, V>,
}

impl<K: Ord + Clone, V: Ord + Copy> Statements<K, V> {
    /// Nothing stated: what [`Resources::unconstrained`](super::Resources)
    /// and [`Features::unconstrained`](super::Features) start from.
    pub(crate) const fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// The value stated of `key`, if any.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<V> {
        self.values.get(key).copied()
    }

    /// Each key `self` requires a larger value of than `provided` states,
    /// in the keys' order.
    pub(crate) fn mismatches(&self, provided: &Self) -> Vec<StatementMismatch<K, V>> {
        (self.values.iter())
            .filter_map(|(key, &required)| {
                let provided = provided.get(key).filter(|&provided| required > provided)?;
                Some(StatementMismatch {
                    key: key.clone(),
                    required,
                    provided,
                })
            })
            .collect()
    }

    /// These statements, with `value` for `key`.
    pub(super) fn with(mut self, key: K, value: V) -> Self {
        self.values.insert(key, value);
        self
    }
}

/// A value required above the one provided: of a resource, more than is
/// provided; of a feature, `true` where the provider states `false`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementMismatch<K, V> {
    /// The resource or the feature.
    pub key: K,
    /// The value required.
    pub required: V,
    /// The value provided.
    pub provided: V,
}

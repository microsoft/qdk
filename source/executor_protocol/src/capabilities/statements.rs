// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The value a part states of each resource or feature: [`Statements`],
//! the one rule every kind follows, and each value required above the one
//! provided ([`StatementMismatch`]).

use std::collections::BTreeMap;

use super::Forwarding;

/// A value per key a part states, such as an amount per
/// [`Resource`](super::Resource) or a yes-or-no per
/// [`Feature`](super::Feature); a key it does not state is forwarded, as
/// the crate's `docs/capabilities.md` says.
///
/// Every kind follows one rule: a required value fits a provided one when
/// it is no larger, so a feature required `true` fits unless the provider
/// states `false`; and where two provisions state a key, the stricter is
/// the smaller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statements<K, V>(BTreeMap<K, V>);

impl<K: Ord + Clone, V: Ord + Copy> Statements<K, V> {
    /// Nothing stated.
    #[must_use]
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// The value stated of `key`, if any.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<V> {
        self.0.get(key).copied()
    }

    /// Each key `self` requires a larger value of than `provided` states,
    /// in the keys' order.
    pub(crate) fn mismatches(&self, provided: &Self) -> Vec<StatementMismatch<K, V>> {
        (self.0.iter())
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
        self.0.insert(key, value);
        self
    }
}

impl<K: Ord + Clone, V: Ord + Copy> Default for Statements<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Clone, V: Ord + Copy> Forwarding for Statements<K, V> {
    fn or(&self, other: &Self) -> Self {
        let mut or = other.0.clone();
        or.extend(self.0.clone());
        Self(or)
    }

    fn unanswered(&self, answered: &Self) -> Self {
        let mut unanswered = self.0.clone();
        unanswered.retain(|key, _| !answered.0.contains_key(key));
        Self(unanswered)
    }

    fn stricter(&self, other: &Self) -> Self {
        let mut stricter = other.0.clone();
        for (key, &value) in &self.0 {
            stricter
                .entry(key.clone())
                .and_modify(|other| *other = value.min(*other))
                .or_insert(value);
        }
        Self(stricter)
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

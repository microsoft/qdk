// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The value a part states of each resource or feature: [`Statements`],
//! the one rule every kind follows, and each value required above the one
//! provided ([`StatementMismatch`]).

use std::collections::{BTreeMap, BTreeSet};

/// A value per key a part states, such as an amount per
/// [`Resource`](super::Resource) or a yes-or-no per
/// [`Feature`](super::Feature), and the keys a provider serves.
///
/// Every kind follows one rule: a required value fits a provided one when
/// it is no larger, so a feature required `true` fits unless the provider
/// states `false`; a key a part does not state, it neither needs nor
/// limits. A provision of a key is a limit, which the requirement from
/// above passes through to the parts below, unless the provider
/// [serves](Self::serving) the key: it handles it itself, so the
/// requirement stops there, as the crate's `docs/capabilities.md` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statements<K, V> {
    values: BTreeMap<K, V>,
    served: BTreeSet<K>,
}

impl<K: Ord + Clone, V: Ord + Copy> Statements<K, V> {
    /// Nothing stated: what [`Resources::unconstrained`](super::Resources)
    /// and [`Features::unconstrained`](super::Features) start from.
    pub(crate) const fn new() -> Self {
        Self {
            values: BTreeMap::new(),
            served: BTreeSet::new(),
        }
    }

    /// The value stated of `key`, if any.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<V> {
        self.values.get(key).copied()
    }

    /// These statements, provided, with `key` served: the part handles it
    /// itself, up to the value stated of it, or any if none is, so a
    /// requirement from above stops there. A requirement's served keys
    /// mean nothing.
    #[must_use]
    pub fn serving(mut self, key: K) -> Self {
        self.served.insert(key);
        self
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

    /// What `self`, a part's provision, provides over `below`, the
    /// provision of the parts below it: of a key `self` serves, `self`'s
    /// statement; of any other, the smaller value where both state it,
    /// served where `below` serves it.
    pub(super) fn over(&self, below: &Self) -> Self {
        let mut values = below.values.clone();
        values.retain(|key, _| !self.served.contains(key));
        for (key, &value) in &self.values {
            values
                .entry(key.clone())
                .and_modify(|below| *below = value.min(*below))
                .or_insert(value);
        }
        let mut served = below.served.clone();
        served.extend(self.served.iter().cloned());
        Self { values, served }
    }

    /// What of `self`, a requirement, reaches past `provision`: each key
    /// `provision` does not serve.
    pub(super) fn not_served_by(&self, provision: &Self) -> Self {
        let mut values = self.values.clone();
        values.retain(|key, _| !provision.served.contains(key));
        Self {
            values,
            served: self.served.clone(),
        }
    }

    /// The larger of `self`'s and `other`'s requirement of each key, where
    /// both state it, else the one that does.
    pub(super) fn larger(&self, other: &Self) -> Self {
        let mut values = other.values.clone();
        for (key, &value) in &self.values {
            values
                .entry(key.clone())
                .and_modify(|other| *other = value.max(*other))
                .or_insert(value);
        }
        Self {
            values,
            served: self.served.clone(),
        }
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

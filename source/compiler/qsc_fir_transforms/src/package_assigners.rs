// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Per-package id assigners for the FIR transform pipeline.
//!
//! Every package in a [`PackageStore`] owns an independent id space: its
//! `LocalItemId`/`ExprId`/`PatId`/`StmtId`/`BlockId` arena keys and its
//! `NodeId`/`LocalVarId` counters are reconstructed per-package by
//! [`Assigner::from_package`], which scans that package's own watermark. A
//! single shared [`Assigner`] therefore cannot allocate fresh ids into a
//! foreign package's arena without colliding with that package's existing
//! keys.
//!
//! [`PackageAssigners`] is a lazily-populated map from [`PackageId`] to that
//! package's [`Assigner`]. The pipeline driver owns one for the whole run and
//! threads `&mut PackageAssigners` into each structural pass entry point. A
//! pass selects the assigner for the package it is about to mutate at its
//! boundary (where the owning `pkg_id` is known) via [`PackageAssigners::get_mut`]
//! or [`PackageAssigners::with_package`], and the advanced watermark persists in
//! the map across later passes and across calls within a pass.
//!
//! The seven structural pass entry points that take `&mut PackageAssigners` are:
//!
//! * [`monomorphize::monomorphize`](crate::monomorphize::monomorphize)
//! * [`return_unify::unify_returns`](crate::return_unify::unify_returns)
//! * [`defunctionalize::defunctionalize`](crate::defunctionalize::defunctionalize)
//! * [`udt_erase::erase_udts`](crate::udt_erase::erase_udts)
//! * [`tuple_compare_lower::lower_tuple_comparisons`](crate::tuple_compare_lower::lower_tuple_comparisons)
//! * [`tuple_decompose::tuple_decompose`](crate::tuple_decompose::tuple_decompose)
//! * [`arg_promote::arg_promote`](crate::arg_promote::arg_promote)
//!
//! Boundary convention: each of these entries selects the entry assigner at its
//! boundary and allocates fresh ids into owning foreign packages via
//! [`PackageAssigners::with_package`] / [`PackageAssigners::get_mut`]. Leaf
//! helpers keep their `&mut Assigner` signatures; only the pass entry points
//! see [`PackageAssigners`].

use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{PackageId, PackageStore};
use rustc_hash::FxHashMap;

/// A lazily-populated map of per-package [`Assigner`]s.
///
/// Assigners are seeded on first access from the owning package's current id
/// watermark via [`Assigner::from_package`]. Packages that are only read from
/// (never mutated) are never seeded, matching the pre-existing single-assigner
/// behavior for read-only foreign packages.
pub(crate) struct PackageAssigners {
    map: FxHashMap<PackageId, Assigner>,
}

impl PackageAssigners {
    /// Creates a `PackageAssigners` seeded with the entry package's assigner.
    ///
    /// This reproduces the previous single-assigner behavior: the entry
    /// package's assigner is constructed once up front from its watermark.
    pub(crate) fn entry(store: &PackageStore, package_id: PackageId) -> Self {
        let mut map = FxHashMap::default();
        map.insert(package_id, Assigner::from_package(store.get(package_id)));
        Self { map }
    }

    /// Returns a mutable reference to the assigner for `package_id`, lazily
    /// seeding it from the package's current id watermark when absent.
    ///
    /// The returned reference borrows only `self`, so callers may continue to
    /// mutate `store` while holding it.
    pub(crate) fn get_mut(&mut self, store: &PackageStore, package_id: PackageId) -> &mut Assigner {
        self.map
            .entry(package_id)
            .or_insert_with(|| Assigner::from_package(store.get(package_id)))
    }

    /// Runs `f` with the assigner for `package_id` taken out of the map by
    /// value, then writes the advanced assigner returned by `f` back into the
    /// map.
    ///
    /// This performs the [`FirCloner::from_assigner`](crate::cloner::FirCloner::from_assigner)
    /// / [`into_assigner`](crate::cloner::FirCloner::into_assigner) round-trip
    /// used when a pass clones FIR into a package in place. Because the
    /// advanced assigner is stored back, per-package watermarks persist across
    /// passes and across repeated calls within a pass. The assigner is lazily
    /// seeded from `store` when the package has not been touched yet.
    ///
    /// `store` is passed through to `f` so the closure can mutate the package
    /// while holding the owned assigner.
    pub(crate) fn with_package<R>(
        &mut self,
        store: &mut PackageStore,
        package_id: PackageId,
        f: impl FnOnce(&mut PackageStore, Assigner) -> (Assigner, R),
    ) -> R {
        let assigner = self
            .map
            .remove(&package_id)
            .unwrap_or_else(|| Assigner::from_package(store.get(package_id)));
        let (assigner, result) = f(store, assigner);
        self.map.insert(package_id, assigner);
        result
    }
}

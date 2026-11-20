// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`select`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#per-call-select)
//! hints: whether a call's flags satisfy them (a [`Selection`], a fact), and
//! what to make of that (a [`SelectPolicy`], a choice).
//!
//! The spec makes `select` a hint for downstream tools, so how to react to a
//! violated hint is up to each piece. It takes a policy: one of the defaults
//! here, or a function of its own.

use super::protocol::{Bit, Status};

/// Compiled `select` patterns of `(flag index, expected value)`. AND inside a
/// pattern, OR across.
pub type CompiledSelect = Vec<Vec<(usize, bool)>>;

/// Whether a call's flags satisfy its `select` hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Selection {
    /// Every completion of the unknown flags matches some pattern.
    Satisfied,
    /// No completion of the unknown flags matches a pattern.
    Violated,
    /// Some completions match and others don't, or deciding would cost too
    /// much (see [`accepts`]).
    Undetermined,
}

/// How a piece reacts to a [`Selection`]: the status it adds to the call's
/// result.
pub type SelectPolicy = fn(Selection) -> Status;

/// The default policy: a violated hint is the recoverable status `rejected`,
/// an undetermined one `unresolved`.
#[inline]
#[must_use]
pub fn recoverable(selection: Selection) -> Status {
    match selection {
        Selection::Satisfied => Status::default(),
        Selection::Violated => Status::REJECTED,
        Selection::Undetermined => Status::UNRESOLVED,
    }
}

/// A policy that ignores hints.
#[inline]
#[must_use]
pub fn ignore(_: Selection) -> Status {
    Status::default()
}

/// The largest number of unknown flags [`accepts`] enumerates. Past it,
/// `accepts` answers [`Selection::Undetermined`] instead of enumerating.
/// Picked arbitrarily, not measured: it bounds the worst case at 2^24 (about
/// 16.8 million) assignments.
const MAX_ENUMERATED_FLAGS: usize = 24;
// `accepts` enumerates assignments as the bits of a `u32`.
const _: () = assert!(MAX_ENUMERATED_FLAGS < u32::BITS as usize);

/// Whether `flags` satisfy `compiled`, where `None` flags are unknown. (A
/// per-pattern scan is not enough: `[{f0:0},{f0:1}]` with `f0` unknown is
/// satisfied by every completion.)
///
/// The answer is exact except in one case: when deciding it would require
/// enumerating more than `MAX_ENUMERATED_FLAGS` unknown flags, it is
/// [`Selection::Undetermined`] even if every completion matches.
///
/// An empty `compiled` is [`Selection::Satisfied`]: the spec defines an
/// omitted or empty `select` as no constraint, not as a list that nothing
/// matches.
#[inline]
#[must_use]
pub fn accepts(compiled: &CompiledSelect, flags: &[Bit]) -> Selection {
    if compiled.is_empty() {
        return Selection::Satisfied;
    }
    // Keep patterns consistent with the known flags, restricted to unknown ones.
    let live: Vec<Vec<(usize, bool)>> = compiled
        .iter()
        .filter(|p| p.iter().all(|&(i, v)| flags[i].is_none_or(|f| f == v)))
        .map(|p| {
            p.iter()
                .copied()
                .filter(|&(i, _)| flags[i].is_none())
                .collect()
        })
        .collect();
    if live.is_empty() {
        return Selection::Violated;
    }
    if live.iter().any(Vec::is_empty) {
        return Selection::Satisfied;
    }
    // Some completion matches. Do all? Enumerate the unknown flags that the live
    // patterns mention; others cannot affect the result. The cost is
    // exponential in their number, hence the cap.
    let mut unknown: Vec<usize> = live.iter().flatten().map(|&(i, _)| i).collect();
    unknown.sort_unstable();
    unknown.dedup();
    if unknown.len() > MAX_ENUMERATED_FLAGS {
        return Selection::Undetermined;
    }
    for assignment in 0u32..(1 << unknown.len()) {
        let value = |flag: usize| {
            let position = unknown.binary_search(&flag).unwrap_or_default();
            assignment & (1 << position) != 0
        };
        if !live.iter().any(|p| p.iter().all(|&(i, v)| value(i) == v)) {
            return Selection::Undetermined;
        }
    }
    Selection::Satisfied
}

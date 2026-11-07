// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The sampling every engine shares: an engine's
//! [`sample`](crate::MpsEngine::sample) built from its conditional
//! probabilities.
//!
//! An engine that draws a sample's values itself asks its state for P(One)
//! of each site given the shot's earlier values, and draws each value from
//! one stream of uniforms. That is the same in every engine, so this module
//! writes it once: an engine keeps only its own probability call
//! ([`sample_by_conditionals`]), and every engine that uses it keeps
//! `sample`'s promises of reproducibility, layout and memory the same way.
//!
//! [`sample_by_conditionals`] draws the shots in order, and each shot's
//! values in the order of its sites, each from the next uniform of the
//! `SplitMix64` stream of the seed (Steele, Lea and Flood, 2014). It keeps
//! P(One) of each distinct prefix of values drawn, so shots that share a
//! prefix ask for it once, in a tree of the prefixes: one node per distinct
//! prefix, so the memory it keeps grows with the values drawn, not with
//! their square.

pub(crate) mod splitmix64;

use splitmix64::SplitMix64;
use std::num::NonZeroUsize;

/// A sample whose values do not fit in memory: `shots × sites` overflows a
/// `usize`, or the allocation of that many values fails. A failure, not a
/// refusal, since the arguments break no rule; an engine's error converts
/// it into its own (`From`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{shots} shots of {sites} sites are more values than memory holds")]
pub struct TooManyValues {
    /// The shots asked for.
    pub shots: usize,
    /// How many sites each shot draws.
    pub sites: usize,
}

/// Draws `shots` shots of the values of `sites`, as
/// [`MpsEngine::sample`](crate::MpsEngine::sample) asks, from an engine's
/// conditional probabilities: `values[s * sites.len() + t]` is shot `s`'s
/// value of `sites[t]`.
///
/// Shot by shot, and in each shot site by site, the value of `sites[t]` is
/// One when the next uniform of the `SplitMix64` stream of `seed` is
/// below `conditional_probability_of_one(sites[t], given)`, where `given`
/// holds the shot's earlier values, `(sites[j], value)` for `j < t` in
/// order. So the same seed and the same probabilities give the same
/// values; a value of probability exactly zero is never drawn, since a
/// uniform is at least 0 and below 1; and each value draws exactly one
/// uniform. P(One) is asked once per distinct `given`, however many shots
/// share it, and kept for the rest of the call, at most once per value
/// drawn.
///
/// The sites are not checked: an engine checks them first
/// ([`checks::sample`](crate::checks::sample)), and the conditional
/// probability may then skip its own checks.
///
/// # Errors
///
/// [`TooManyValues`], converted into `E`, when the values do not fit in
/// memory, before any probability is asked. No values (no sites, or no
/// shots) is no error: the call returns at once, asking nothing. An error
/// of `conditional_probability_of_one` is returned as it is, and the draw
/// stops there.
///
/// ```
/// use tensornet::sampling::{TooManyValues, sample_by_conditionals};
///
/// // Two sites that always agree: P(One) is ½, then the first value.
/// let values = sample_by_conditionals(&[0, 1], 100, 7, |_site, given| {
///     let p1 = match given.first() {
///         None => 0.5,
///         Some(&(_, one)) => f64::from(u8::from(one)),
///     };
///     Ok::<_, TooManyValues>(p1)
/// })?;
/// assert!(values.chunks_exact(2).all(|shot| shot[0] == shot[1]));
/// # Ok::<(), TooManyValues>(())
/// ```
pub fn sample_by_conditionals<E: From<TooManyValues>>(
    sites: &[usize],
    shots: usize,
    seed: u64,
    mut conditional_probability_of_one: impl FnMut(usize, &[(usize, bool)]) -> Result<f64, E>,
) -> Result<Vec<bool>, E> {
    let too_many = TooManyValues {
        shots,
        sites: sites.len(),
    };
    let count = shots.checked_mul(sites.len()).ok_or(too_many)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| too_many)?;
    let mut uniforms = SplitMix64::new(seed);
    let mut prefixes = PrefixTree::default();
    let mut given = Vec::with_capacity(sites.len());
    for _ in 0..shots {
        given.clear();
        let mut position = Position::Start;
        for &site in sites {
            let (node, p1) =
                prefixes.p1_at(position, || conditional_probability_of_one(site, &given))?;
            let one = uniforms.next_uniform() < p1;
            values.push(one);
            given.push((site, one));
            position = Position::After { node, one };
        }
    }
    Ok(values)
}

/// P(One) of the next value given each distinct prefix of values that a
/// shot drew, kept in a tree of the prefixes: a node per prefix, whose next
/// node for each value is the prefix one value longer. A shot walks it one
/// node per value, so a prefix is asked for once, and the tree holds at
/// most one node per value drawn.
#[derive(Default)]
struct PrefixTree {
    nodes: Vec<PrefixNode>,
}

/// A prefix: P(One) of the next value given it, and the node of the prefix
/// one value longer for each value (`next[0]` for Zero, `next[1]` for One),
/// once a shot drew it. Node 0, the empty prefix, is no node's next, so a
/// next node's index is never zero.
struct PrefixNode {
    p1: f64,
    next: [Option<NonZeroUsize>; 2],
}

/// Where a shot is in the tree: before its first value, or after the value
/// `one` at the node `node`.
#[derive(Clone, Copy)]
enum Position {
    Start,
    After { node: usize, one: bool },
}

impl PrefixTree {
    /// The node of the prefix at `position`, and P(One) of the next value
    /// given it: kept, or asked of `ask` the first time a shot reaches it.
    fn p1_at<E>(
        &mut self,
        position: Position,
        ask: impl FnOnce() -> Result<f64, E>,
    ) -> Result<(usize, f64), E> {
        let known = match position {
            Position::Start => (!self.nodes.is_empty()).then_some(0),
            Position::After { node, one } => {
                self.nodes[node].next[usize::from(one)].map(NonZeroUsize::get)
            }
        };
        if let Some(node) = known {
            return Ok((node, self.nodes[node].p1));
        }
        let p1 = ask()?;
        self.nodes.push(PrefixNode {
            p1,
            next: [None; 2],
        });
        let node = self.nodes.len() - 1;
        if let Position::After { node: parent, one } = position {
            self.nodes[parent].next[usize::from(one)] = NonZeroUsize::new(node);
        }
        Ok((node, p1))
    }
}

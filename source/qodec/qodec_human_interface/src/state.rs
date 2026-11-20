// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What is live at a boundary, and when.
//!
//! Every call is read with the runtime model's own rules
//! ([`Isa::parse_call`] and [`Signature::bind`](qodec_runtime_recipes::Signature::bind)),
//! as the type checker reads it: a call's inputs end, and its outputs begin
//! with their declared types. So what is shown as live is what the runtime
//! holds live.

use crate::shot::{Event, Shot};
use qodec_runtime_recipes::Isa;
use std::collections::BTreeMap;

/// The blocks one call ends and begins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wiring {
    /// The blocks it takes, in operand order.
    pub ins: Vec<u64>,
    /// The blocks it gives, in operand order.
    pub outs: Vec<u64>,
    /// The type of each block it gives, as an index into its ISA's types.
    pub out_types: Vec<usize>,
}

impl Wiring {
    /// Every block it touches, in operand order, once each.
    pub fn blocks(&self) -> impl Iterator<Item = u64> + '_ {
        self.ins
            .iter()
            .chain(self.outs.iter().filter(|o| !self.ins.contains(o)))
            .copied()
    }
}

/// One block's life at a boundary: from the call that gives it to the call
/// that takes it, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lifetime {
    /// The block's id.
    pub label: u64,
    /// Its block type.
    pub block_type: String,
    /// The event that gave it, after which it is live; `None` if it was never
    /// given, only taken.
    pub from: Option<usize>,
    /// The event that took it, after which it is not live; `None` if it is
    /// live at the end of the shot.
    pub until: Option<usize>,
}

impl Lifetime {
    /// Whether it is live right after event `t`, or before any event for
    /// `None`.
    #[must_use]
    pub fn live_after(&self, t: Option<usize>) -> bool {
        let born = match (self.from, t) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(from), Some(t)) => from <= t,
        };
        let dead = match (self.until, t) {
            (None, _) | (Some(_), None) => false,
            (Some(until), Some(t)) => until <= t,
        };
        born && !dead
    }
}

/// The wiring of every call at one boundary, and the lifetimes it makes.
#[derive(Clone, Debug, Default)]
pub struct Lives {
    /// Per event of the boundary's timeline.
    pub wiring: Vec<Wiring>,
    /// Every block's lifetimes, in the order they began.
    pub lifetimes: Vec<Lifetime>,
}

impl Lives {
    /// Reads the timeline of boundary `b` of `shot`, whose ISA is `isa`.
    #[must_use]
    pub fn of(isa: &Isa, shot: &Shot, b: usize) -> Self {
        let timeline = shot.timelines.get(b).map_or(&[][..], Vec::as_slice);
        let mut lives = Lives::default();
        // Live blocks: label -> (type, its lifetime's index).
        let mut live: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
        for (t, &event) in timeline.iter().enumerate() {
            let wiring = wire(isa, shot, event, &live);
            for label in &wiring.ins {
                match live.remove(label) {
                    Some((_, lifetime)) => lives.lifetimes[lifetime].until = Some(t),
                    None => lives.lifetimes.push(Lifetime {
                        label: *label,
                        block_type: "?".to_string(),
                        from: None,
                        until: Some(t),
                    }),
                }
            }
            for (&label, &block_type) in wiring.outs.iter().zip(&wiring.out_types) {
                // An output-only operand overwrites a live block.
                if let Some((_, lifetime)) = live.remove(&label) {
                    lives.lifetimes[lifetime].until = Some(t);
                }
                live.insert(label, (block_type, lives.lifetimes.len()));
                lives.lifetimes.push(Lifetime {
                    label,
                    block_type: isa.types.names.get(block_type).cloned().unwrap_or_default(),
                    from: Some(t),
                    until: None,
                });
            }
            lives.wiring.push(wiring);
        }
        lives
    }

    /// The blocks live right after event `t`, or before any event for
    /// `None`, by id: (block type, the event that gave it).
    #[must_use]
    pub fn live_after(&self, t: Option<usize>) -> BTreeMap<u64, (&str, Option<usize>)> {
        self.lifetimes
            .iter()
            .filter(|l| l.live_after(t))
            .map(|l| (l.label, (l.block_type.as_str(), l.from)))
            .collect()
    }
}

/// The blocks `event` takes and gives, bound as the type checker binds them.
/// A call that does not bind, which the runtime would have refused, is shown
/// as taking and giving all its blocks.
fn wire(isa: &Isa, shot: &Shot, event: Event, live: &BTreeMap<u64, (usize, usize)>) -> Wiring {
    let call = &shot.traced(event).call;
    let bound = isa
        .parse_call(call.instruction_id, &call.parameters)
        .and_then(|parsed| {
            let known = parsed
                .operands
                .iter()
                .filter_map(|l| live.get(l).map(|&(t, _)| (*l, t)))
                .collect();
            isa.instructions[parsed.index].bind(&isa.types, &parsed.operands, &known)
        });
    if let Ok((ins, outs)) = bound {
        Wiring {
            ins: ins.iter().map(|b| b.label).collect(),
            outs: outs.iter().map(|b| b.label).collect(),
            out_types: outs.iter().map(|b| b.block_type).collect(),
        }
    } else {
        let blocks: Vec<u64> = crate::syntax::blocks(call).collect();
        let out_types = blocks
            .iter()
            .map(|l| live.get(l).map_or(0, |&(t, _)| t))
            .collect();
        Wiring {
            ins: blocks.clone(),
            outs: blocks,
            out_types,
        }
    }
}

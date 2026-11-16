// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What single faults do to a gadget, and how many it takes to fool it.
//!
//! The gadget analysis is run again with a variable for every fault that can
//! happen after each call (see [`analysis::analyze_with`]). The circuit is
//! Clifford and the faults are Paulis, so every check, flag and logical sign
//! is an affine formula, and each fault's effect is the set of formulas it
//! appears in: which detectors it fires (its syndrome) and which logical
//! signs it flips. A set of faults goes unnoticed and does harm when their
//! syndromes cancel and their logical flips do not.
//!
//! The smallest such set is the gadget's fault distance, for detection: the
//! frame decoder reports a fired check rather than correcting it, so a fault
//! that fires one is caught. The search is exhaustive up to a budget; past it
//! the answer is a lower bound, which is still information.

use crate::{
    analysis::{self, FaultKind},
    gf2::{Bits, Form},
};
use qodec::{Gadget, InstructionSet};
use std::collections::BTreeMap;

/// One elementary fault: an X, Y or Z on a qubit after a call, or a flipped
/// outcome. Under depolarizing noise of strength `p`, each of X, Y and Z on a
/// qubit happens with probability `p/3`, and a flip with probability `p`.
#[derive(Clone, Debug)]
pub struct Atom {
    /// The call after which it happens.
    pub call: usize,
    /// Where: a qubit, or `outcome i`.
    pub location: String,
    /// What: `X`, `Y`, `Z` or `flip`.
    pub what: &'static str,
    /// Its probability, in units of `p`.
    pub weight: f64,
    /// The detectors it fires.
    pub syndrome: Bits,
    /// The logical signs it flips.
    pub logical: Bits,
}

impl Atom {
    /// Whether it does anything at all.
    #[must_use]
    pub fn matters(&self) -> bool {
        !self.syndrome.is_zero() || !self.logical.is_zero()
    }

    /// Whether it is undetected and harmful on its own.
    #[must_use]
    pub fn fails(&self) -> bool {
        self.syndrome.is_zero() && !self.logical.is_zero()
    }

    /// `X on 3 after call 5`.
    #[must_use]
    pub fn describe(&self, calls: &[String]) -> String {
        let after = calls
            .get(self.call)
            .map_or_else(|| format!("call {}", self.call + 1), |c| format!("`{c}`"));
        format!("{} on {} after {after}", self.what, self.location)
    }
}

/// Every elementary fault of a gadget and what it does.
#[derive(Clone, Debug, Default)]
pub struct FaultTable {
    /// The faults.
    pub atoms: Vec<Atom>,
    /// The detectors: the gadget's checks, then its flags.
    pub detectors: Vec<String>,
    /// The logical signs: the instruction's ideal operators.
    pub logicals: Vec<String>,
    /// The calls, as the circuit writes them.
    pub calls: Vec<String>,
}

/// How many faults it takes to fool a gadget.
#[derive(Clone, Debug, PartialEq)]
pub struct FaultDistance {
    /// No set of fewer faults goes unnoticed and does harm.
    pub lower: usize,
    /// Whether a set of exactly [`Self::lower`] faults does: the distance is
    /// then known, not only bounded.
    pub exact: bool,
    /// How many such smallest sets there are, when exact.
    pub failures: usize,
    /// With exact distance `d`: the probability of a harmful, unnoticed fault
    /// is about `coefficient · p^d` for small `p`.
    pub coefficient: f64,
    /// One smallest harmful set, as indices into [`FaultTable::atoms`].
    pub example: Vec<usize>,
}

/// The fault table of `gadget`, whose circuit calls `lower`. Fails if the
/// gadget does not verify, or has a step the analysis cannot follow, since
/// then there is no fault-free reference to compare with.
pub fn table(gadget: &Gadget, lower: &InstructionSet) -> Result<FaultTable, String> {
    let plain = analysis::analyze(gadget, lower)?;
    if !plain.followed {
        return Err("it has a step the analysis can't follow, such as a rotation".to_string());
    }
    // Output stabilizers that no check fixes are a warning, not a failure:
    // the faults are still well defined.
    let fails = plain
        .checks
        .iter()
        .chain(&plain.readouts)
        .chain(&plain.frames)
        .any(|v| v.fails() || v.residual.is_err())
        || !plain.action.mismatches.is_empty()
        || plain.out_stabilizers.iter().any(|s| s.sign.is_none());
    if fails {
        return Err(format!(
            "it does not verify yet ({} problems)",
            plain.problems()
        ));
    }
    let a = analysis::analyze_with(gadget, lower, true)?;
    let fault_of = a.vars.faults();

    let mut detectors: Vec<(String, Form)> = Vec::new();
    for v in &a.checks {
        let form = v.residual.clone()?;
        detectors.push((v.text.clone(), form));
    }
    for (v, flag) in a.readouts.iter().zip(&a.readout_is_flag) {
        if *flag {
            detectors.push((v.text.clone(), v.residual.clone()?));
        }
    }
    let logicals: Vec<(String, Form)> = a
        .action
        .operators
        .iter()
        .cloned()
        .zip(a.action.diffs.iter().cloned())
        .collect();

    // What site `f` does: the detectors and logical signs it appears in.
    let effect = |f: usize| -> (Bits, Bits) {
        let v = fault_of[&f];
        let mut syndrome = Bits::default();
        for (j, (_, form)) in detectors.iter().enumerate() {
            syndrome.set(j, form.vars.get(v));
        }
        let mut logical = Bits::default();
        for (j, (_, form)) in logicals.iter().enumerate() {
            logical.set(j, form.vars.get(v));
        }
        (syndrome, logical)
    };

    let mut atoms = Vec::new();
    let mut f = 0;
    while f < a.faults.len() {
        let site = &a.faults[f];
        match site.kind {
            FaultKind::Flip => {
                let (syndrome, logical) = effect(f);
                atoms.push(Atom {
                    call: site.call,
                    location: format!("outcome {}", site.at),
                    what: "flip",
                    weight: 1.0,
                    syndrome,
                    logical,
                });
                f += 1;
            }
            FaultKind::X | FaultKind::Z => {
                // X then Z at the same place: the two sites, and Y as both.
                let (sx, lx) = effect(f);
                let (sz, lz) = effect(f + 1);
                let mut sy = sx.clone();
                sy.xor(&sz);
                let mut ly = lx.clone();
                ly.xor(&lz);
                for (what, syndrome, logical) in [("X", sx, lx), ("Y", sy, ly), ("Z", sz, lz)] {
                    atoms.push(Atom {
                        call: site.call,
                        location: site.at.clone(),
                        what,
                        weight: 1.0 / 3.0,
                        syndrome,
                        logical,
                    });
                }
                f += 2;
            }
        }
    }
    Ok(FaultTable {
        atoms,
        detectors: detectors.into_iter().map(|(t, _)| t).collect(),
        logicals: logicals.into_iter().map(|(t, _)| t).collect(),
        calls: a.calls.iter().map(|c| c.text.clone()).collect(),
    })
}

impl FaultTable {
    /// The fault distance, searching sets of up to three faults, and giving
    /// up on three after `budget` pairs.
    #[must_use]
    pub fn distance(&self, budget: usize) -> FaultDistance {
        let relevant: Vec<usize> = (0..self.atoms.len())
            .filter(|&i| self.atoms[i].matters())
            .collect();
        let found = |sets: Vec<Vec<usize>>, d: usize| -> FaultDistance {
            let coefficient = sets
                .iter()
                .map(|s| s.iter().map(|&i| self.atoms[i].weight).product::<f64>())
                .sum();
            FaultDistance {
                lower: d,
                exact: true,
                failures: sets.len(),
                coefficient,
                example: sets.first().cloned().unwrap_or_default(),
            }
        };
        let place = |i: usize| (self.atoms[i].call, self.atoms[i].location.as_str());

        let ones: Vec<Vec<usize>> = relevant
            .iter()
            .filter(|&&i| self.atoms[i].fails())
            .map(|&i| vec![i])
            .collect();
        if !ones.is_empty() {
            return found(ones, 1);
        }

        let mut by_syndrome: BTreeMap<Vec<u64>, Vec<usize>> = BTreeMap::new();
        for &i in &relevant {
            by_syndrome
                .entry(self.atoms[i].syndrome.key())
                .or_default()
                .push(i);
        }
        let mut twos = Vec::new();
        for group in by_syndrome.values() {
            for (x, &a) in group.iter().enumerate() {
                for &b in &group[x + 1..] {
                    if place(a) != place(b) && self.atoms[a].logical != self.atoms[b].logical {
                        twos.push(vec![a, b]);
                    }
                }
            }
        }
        if !twos.is_empty() {
            return found(twos, 2);
        }

        let mut threes = Vec::new();
        let mut pairs = 0usize;
        for (x, &a) in relevant.iter().enumerate() {
            for &b in &relevant[x + 1..] {
                if place(a) == place(b) {
                    continue;
                }
                pairs += 1;
                if pairs > budget {
                    return FaultDistance {
                        lower: 3,
                        exact: false,
                        failures: 0,
                        coefficient: 0.0,
                        example: Vec::new(),
                    };
                }
                let mut s = self.atoms[a].syndrome.clone();
                s.xor(&self.atoms[b].syndrome);
                let Some(group) = by_syndrome.get(&s.key()) else {
                    continue;
                };
                let mut l = self.atoms[a].logical.clone();
                l.xor(&self.atoms[b].logical);
                for &c in group {
                    if c > b
                        && place(c) != place(a)
                        && place(c) != place(b)
                        && self.atoms[c].logical != l
                    {
                        threes.push(vec![a, b, c]);
                    }
                }
            }
        }
        if !threes.is_empty() {
            return found(threes, 3);
        }
        FaultDistance {
            lower: 4,
            exact: false,
            failures: 0,
            coefficient: 0.0,
            example: Vec::new(),
        }
    }
}

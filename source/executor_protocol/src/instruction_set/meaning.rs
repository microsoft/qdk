// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an instruction does ([`Meaning`]), as Qodec declares it in its
//! `action`, in a canonical form: so that two instruction sets can be
//! matched by what their instructions do, not by what they call them
//! (Stim's `H` is a state vector's `h`).
//!
//! A meaning is a list of steps, each written with [`Pauli`] operators
//! over the instruction's qubits: prepare a stabilizer state, apply a
//! Clifford or a Pauli, observe Paulis, rotate about a Pauli by an
//! [`Angle`]; any step but an observation may run only if a parity of
//! [`Guard`]s is odd (Qodec's `if`) or even (`unless`). Two meanings are
//! the same when their steps are equal: equality is syntactic, on a
//! canonical form (a Pauli's factors sorted, a Clifford listing only the
//! generators it changes, a guard's terms sorted), so two actions doing
//! the same thing written otherwise (other stabilizer generators) differ.
//! A parameter is named by its position among the instruction's
//! parameters, not by its name, so that `rotate(theta)` and
//! `rotate(angle)` are the same instruction.
//!
//! An instruction that declares no action has the meaning
//! [`Meaning::undeclared`]: QIR's gates written in code, a profiler's
//! `round`. Such an instruction is matched by mnemonic only, and only
//! between sets of one name.
//!
//! The block sizes an action's qubits are numbered over are the set's
//! (its block types), compared with the instruction's blocks, so a meaning
//! holds only its steps. Parsing Qodec's YAML is a loader's job.

use super::{Parameter, ParameterKind, Pauli};
use std::collections::BTreeMap;
use std::fmt;

/// What an instruction does, as Qodec declares it in its `action`, or
/// [`undeclared`](Self::undeclared).
///
/// Built step by step, in the action's order: Qodec's `action: [clifford:
/// {X_0: X_0 X_1, Z_1: Z_0 Z_1}]` is
/// `Meaning::declared().clifford([(Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
/// (Pauli::z(1), Pauli::z(0) * Pauli::z(1))])`, and `rotate_z0`'s action
/// (`rotate: {pauli: Z_0, angle: theta}`, `observe: Z_0`, `pauli: X_0`
/// with `if: [correction, outcomes[0]]`), for parameters `correction`
/// then `theta`, is
/// `Meaning::declared().rotate(Pauli::z(0), Angle::Parameter(1))
/// .observe([Pauli::z(0)]).pauli(Pauli::x(0)).only_if([Guard::Parameter(0),
/// Guard::Outcome(0)])`.
///
/// What a meaning refers to (its parameters' kinds, its outcomes), and
/// the slips made while writing it (a guard or a generator given twice),
/// are checked when its set is built, as a
/// [`DeclarationFault::Meaning`](crate::DeclarationFault::Meaning).
#[derive(Clone, Debug, Default, Eq)]
pub struct Meaning {
    /// `None` when undeclared.
    steps: Option<Vec<Step>>,
    /// The faults made while building it, in order, refused when the set
    /// is built: a guard before any step or given twice to one, a
    /// generator a Clifford maps twice.
    slips: Vec<MeaningFault>,
    /// Whether the latest step has its guard, so that another is a slip.
    last_step_guarded: bool,
    /// Each term given to a step's guards, with its step, once, before
    /// the terms given twice cancel: so that what a term refers to is
    /// checked even when it cancels.
    guard_terms: Vec<(usize, Guard)>,
}

/// Two meanings are equal when their steps and slips are, whatever guards
/// the latest step was given and whatever terms cancelled:
/// `pauli(x).unless([])` is `pauli(x)`, since an even parity of no term
/// always holds.
impl PartialEq for Meaning {
    fn eq(&self, other: &Self) -> bool {
        self.steps == other.steps && self.slips == other.slips
    }
}

impl Meaning {
    /// The meaning of an instruction that declares no action. Its set can
    /// be matched by mnemonic only, with a set of the same name. A step
    /// added to it declares it: `undeclared().pauli(x)` is
    /// `declared().pauli(x)`, so a loader may start here and add the steps
    /// it reads.
    #[must_use]
    pub fn undeclared() -> Self {
        Self::default()
    }

    /// A declared action with no steps yet: the steps follow, in order.
    #[must_use]
    pub fn declared() -> Self {
        Self {
            steps: Some(Vec::new()),
            ..Self::default()
        }
    }

    /// This action, then the preparation of the state `generators`
    /// stabilize, Qodec's `stabilize`.
    #[must_use]
    pub fn stabilize(self, generators: impl IntoIterator<Item = Pauli>) -> Self {
        self.then(Operation::Stabilize(generators.into_iter().collect()))
    }

    /// This action, then the Clifford that maps each generator to its
    /// image, Qodec's `clifford`. A generator mapped to itself is dropped,
    /// as one not listed is fixed. A generator given twice is refused when
    /// the set is built ([`MeaningFault::GeneratorTwice`]).
    #[must_use]
    pub fn clifford(mut self, images: impl IntoIterator<Item = (Pauli, Pauli)>) -> Self {
        let images: Vec<(Pauli, Pauli)> = images.into_iter().collect();
        let step = self.steps.as_ref().map_or(0, Vec::len);
        for (position, (generator, _)) in images.iter().enumerate() {
            if images[..position]
                .iter()
                .any(|(earlier, _)| earlier == generator)
            {
                self.slips.push(MeaningFault::GeneratorTwice {
                    step,
                    generator: generator.clone(),
                });
            }
        }
        let changed = images
            .into_iter()
            .filter(|(generator, image)| generator != image)
            .collect();
        self.then(Operation::Clifford(changed))
    }

    /// This action, then `pauli`, Qodec's `pauli`.
    #[must_use]
    pub fn pauli(self, pauli: Pauli) -> Self {
        self.then(Operation::Pauli(pauli))
    }

    /// This action, then the measurement of `observables`, one outcome
    /// each, in order, Qodec's `observe`.
    #[must_use]
    pub fn observe(self, observables: impl IntoIterator<Item = impl Into<Observable>>) -> Self {
        self.then(Operation::Observe(
            observables.into_iter().map(Into::into).collect(),
        ))
    }

    /// This action, then the rotation about `pauli` by `angle`, Qodec's
    /// `rotate`: `exp(-iθP/2)`, so that `θ = π/4` about `Z` is a `T` gate
    /// up to a global phase.
    #[must_use]
    pub fn rotate(self, pauli: Pauli, angle: Angle) -> Self {
        self.then(Operation::Rotate { pauli, angle })
    }

    /// The latest step, run only if the parity of `guards` is odd, Qodec's
    /// `if`. A step takes one guard: `only_if` or `unless` again on it is
    /// refused when the set is built ([`MeaningFault::GuardTwice`]). So is
    /// a parity of no term, none given or terms that cancel in pairs
    /// ([`MeaningFault::GuardNeverMet`]): it is always even, and the step
    /// needs an odd parity, so it would never run.
    #[must_use]
    pub fn only_if(self, guards: impl IntoIterator<Item = Guard>) -> Self {
        self.guarded(true, guards)
    }

    /// The latest step, run only if the parity of `guards` is even,
    /// Qodec's `unless`. A step takes one guard, as for
    /// [`only_if`](Self::only_if).
    #[must_use]
    pub fn unless(self, guards: impl IntoIterator<Item = Guard>) -> Self {
        self.guarded(false, guards)
    }

    /// Whether the instruction declares what it does.
    pub(crate) fn is_declared(&self) -> bool {
        self.steps.is_some()
    }

    /// Every fault of this meaning for an instruction with `parameters` and
    /// `outcomes`: first the slips made while building it, in order; then,
    /// step by step, a guard on an observation, a guard's term (each term
    /// given, even one that cancels) naming a parameter it lacks or of
    /// another kind or an outcome not observed before its step, an angle or
    /// an observable naming a parameter it lacks or of another kind; and,
    /// when declared, observations that give other than `outcomes`
    /// outcomes.
    pub(crate) fn faults(&self, parameters: &[Parameter], outcomes: u32) -> Vec<MeaningFault> {
        let mut faults = self.slips.clone();
        let Some(steps) = &self.steps else {
            return faults;
        };
        // A `u64`, so the count is exact: no sum of lengths reaches it.
        let mut observed: u64 = 0;
        let refer = |step: usize, parameter: usize, expected: ParameterKind| match parameters
            .get(parameter)
        {
            None => Some(MeaningFault::NotAParameter { step, parameter }),
            Some(declared) if declared.kind != expected => Some(MeaningFault::WrongParameterKind {
                step,
                parameter,
                expected,
            }),
            Some(_) => None,
        };
        let mut guard_terms = self.guard_terms.iter().peekable();
        for (step, Step { operation, guard }) in steps.iter().enumerate() {
            if guard.is_some() && matches!(operation, Operation::Observe(_)) {
                faults.push(MeaningFault::GuardedObserve { step });
            }
            while let Some(&(_, term)) = guard_terms.next_if(|(of, _)| *of == step) {
                match term {
                    Guard::Parameter(parameter) => {
                        faults.extend(refer(step, parameter, ParameterKind::Bit));
                    }
                    Guard::Outcome(outcome) if u64::from(outcome) >= observed => {
                        faults.push(MeaningFault::OutcomeNotYet { step, outcome });
                    }
                    Guard::Outcome(_) => {}
                }
            }
            match operation {
                Operation::Rotate {
                    angle: Angle::Parameter(parameter),
                    ..
                } => faults.extend(refer(step, *parameter, ParameterKind::Number)),
                Operation::Observe(observables) => {
                    for observable in observables {
                        if let Observable::Parameter(parameter) = *observable {
                            faults.extend(refer(step, parameter, ParameterKind::Pauli));
                        }
                    }
                    observed += observables.len() as u64;
                }
                _ => {}
            }
        }
        if observed != u64::from(outcomes) {
            faults.push(MeaningFault::OutcomeCount {
                declared: outcomes,
                observed: u32::try_from(observed).unwrap_or(u32::MAX),
            });
        }
        faults
    }

    fn then(mut self, operation: Operation) -> Self {
        self.steps.get_or_insert_with(Vec::new).push(Step {
            operation,
            guard: None,
        });
        self.last_step_guarded = false;
        self
    }

    fn guarded(mut self, odd: bool, guards: impl IntoIterator<Item = Guard>) -> Self {
        let mut given: Vec<Guard> = guards.into_iter().collect();
        given.sort_unstable();
        let guard = Condition::new(odd, given.iter().copied());
        let Some(steps) = self.steps.as_mut().filter(|steps| !steps.is_empty()) else {
            self.slips.push(MeaningFault::GuardWithoutStep);
            return self;
        };
        let step = steps.len() - 1;
        given.dedup();
        for term in given {
            if !self.guard_terms.contains(&(step, term)) {
                self.guard_terms.push((step, term));
            }
        }
        if self.last_step_guarded {
            self.slips.push(MeaningFault::GuardTwice { step });
        }
        if guard
            .as_ref()
            .is_some_and(|guard| guard.odd && guard.terms.is_empty())
        {
            self.slips.push(MeaningFault::GuardNeverMet { step });
        }
        steps[step].guard = guard;
        self.last_step_guarded = true;
        self
    }
}

/// The angle of a rotation: a number, or one of the instruction's `number`
/// parameters.
///
/// Angles compare bit for bit, so that meanings are `Eq`: `0.5` equals
/// `0.5`, a NaN equals itself, and `0.0` differs from `-0.0`; a loader
/// that reads angles from text writes each the same way.
#[derive(Clone, Copy, Debug)]
pub enum Angle {
    /// An angle fixed by the instruction, in radians.
    Literal(f64),
    /// The instruction's parameter at this position, from 0.
    Parameter(usize),
}

impl PartialEq for Angle {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Angle::Literal(a), Angle::Literal(b)) => a.to_bits() == b.to_bits(),
            (Angle::Parameter(a), Angle::Parameter(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Angle {}

/// What an observation measures: a Pauli, or the instruction's `pauli`
/// parameter at a position, as Stim's `MPP` measures the product it is
/// given (Qodec's `observe: p`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observable {
    /// This Pauli, fixed by the instruction.
    Pauli(Pauli),
    /// The instruction's `pauli` parameter at this position, from 0.
    Parameter(usize),
}

impl From<Pauli> for Observable {
    fn from(pauli: Pauli) -> Self {
        Observable::Pauli(pauli)
    }
}

/// One term of a step's guard, Qodec's `if: [correction, outcomes[0]]`:
/// the instruction's `bit` parameter at a position, or an outcome of an
/// earlier observation of the same action, counted across them from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Guard {
    /// The instruction's `bit` parameter at this position, from 0.
    Parameter(usize),
    /// The action's outcome at this position, from 0.
    Outcome(u32),
}

/// Why a meaning does not fit the instruction it is declared for: a slip
/// made while writing it, or what its steps refer to that the instruction
/// lacks. A step is counted from 0.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeaningFault {
    /// `only_if` or `unless` before any step: it guards nothing.
    GuardWithoutStep,
    /// `only_if` or `unless` given twice to one step, which would keep
    /// only the latest.
    GuardTwice {
        /// The step, from 0.
        step: usize,
    },
    /// `only_if` of no term, or of terms that cancel in pairs: the step
    /// runs only if the parity is odd, and a parity of no term is always
    /// even, so the step would never run.
    GuardNeverMet {
        /// The step, from 0.
        step: usize,
    },
    /// A Clifford maps one generator twice, which would keep only the
    /// latest image.
    GeneratorTwice {
        /// The step, from 0.
        step: usize,
        /// The generator mapped twice.
        generator: Pauli,
    },
    /// A guard on an observation: Qodec's outcome count must not depend on
    /// a run-time bit.
    GuardedObserve {
        /// The step, from 0.
        step: usize,
    },
    /// An angle, guard or observable names a parameter position the
    /// instruction does not have.
    NotAParameter {
        /// The step, from 0.
        step: usize,
        /// The position named, from 0.
        parameter: usize,
    },
    /// It names a parameter of another kind: an angle needs a `number`, a
    /// guard a `bit`, an observable a `pauli`.
    WrongParameterKind {
        /// The step, from 0.
        step: usize,
        /// The parameter's position, from 0.
        parameter: usize,
        /// The kind the step needs there.
        expected: ParameterKind,
    },
    /// A guard names an outcome not observed before its step.
    OutcomeNotYet {
        /// The step, from 0.
        step: usize,
        /// The outcome named, from 0.
        outcome: u32,
    },
    /// Its observations give `observed` outcomes, the instruction declares
    /// `declared`.
    OutcomeCount {
        /// The instruction's outcomes.
        declared: u32,
        /// Its observations' outcomes, or `u32::MAX` if more.
        observed: u32,
    },
}

impl fmt::Display for MeaningFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeaningFault::GuardWithoutStep => f.write_str("a guard before any step"),
            MeaningFault::GuardTwice { step } => write!(f, "step {step} is guarded twice"),
            MeaningFault::GuardNeverMet { step } => {
                write!(
                    f,
                    "step {step} runs only if a parity is odd, but the parity has no \
                     term (none given, or terms that cancel in pairs), so it never runs"
                )
            }
            MeaningFault::GeneratorTwice { step, generator } => {
                write!(f, "step {step} maps `{generator}` twice")
            }
            MeaningFault::GuardedObserve { step } => {
                write!(f, "step {step} observes under a guard")
            }
            MeaningFault::NotAParameter { step, parameter } => {
                write!(f, "step {step} names parameter {parameter}, which it lacks")
            }
            MeaningFault::WrongParameterKind {
                step,
                parameter,
                expected,
            } => write!(
                f,
                "step {step} needs parameter {parameter} to be a `{expected}`"
            ),
            MeaningFault::OutcomeNotYet { step, outcome } => write!(
                f,
                "step {step} is guarded by outcome {outcome}, not observed before it"
            ),
            MeaningFault::OutcomeCount { declared, observed } => write!(
                f,
                "its action observes {observed} outcomes, its declaration says {declared}"
            ),
        }
    }
}

/// One step of an action: what it does, and its guard, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    operation: Operation,
    guard: Option<Condition>,
}

/// What a step does, in Qodec's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Operation {
    /// Prepares the state these Paulis stabilize.
    Stabilize(Vec<Pauli>),
    /// Maps each Pauli to its image; a generator not listed is fixed.
    Clifford(BTreeMap<Pauli, Pauli>),
    /// Applies a Pauli.
    Pauli(Pauli),
    /// Measures each observable, one outcome each, in order.
    Observe(Vec<Observable>),
    /// Rotates about a Pauli by an angle.
    Rotate { pauli: Pauli, angle: Angle },
}

/// A step runs when the parity of its terms is `odd` (Qodec's `if`) or
/// even (`unless`). Canonical: its terms sorted, and a term given twice
/// dropped, since it cancels in a parity.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Condition {
    odd: bool,
    terms: Vec<Guard>,
}

impl Condition {
    /// `None` for `unless` with no term left: the step always runs.
    fn new(odd: bool, guards: impl IntoIterator<Item = Guard>) -> Option<Self> {
        let mut sorted: Vec<Guard> = guards.into_iter().collect();
        sorted.sort_unstable();
        let mut terms: Vec<Guard> = Vec::with_capacity(sorted.len());
        for guard in sorted {
            if terms.last() == Some(&guard) {
                terms.pop();
            } else {
                terms.push(guard);
            }
        }
        (odd || !terms.is_empty()).then_some(Self { odd, terms })
    }
}

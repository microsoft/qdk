// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a job's program needs from the components of a chain ([`Needs`], each a
//! [`Need`]), and the check of those needs against what a component offers:
//! when a chain is given a job, every unmet need is reported
//! ([`JobFaults`](crate::JobFaults),
//! `a_jobs_unmet_needs_are_each_reported_in_order_naming_the_chains_executor`);
//! by hand, [`Capabilities::admit`] returns the first, as an [`UnmetNeed`]
//! naming the component that does not meet it
//! (`several_unmet_needs_are_reported_one_at_a_time_in_a_fixed_order`).

use std::fmt;
use std::sync::Arc;

use crate::instruction_set::write_mismatches;
use crate::{Capabilities, InstructionMismatch, InstructionSet, OpcodeMap, Rejected, ShotSet};

/// What a job's program needs for its whole run, stated by an adaptive
/// program ([`AdaptiveProgram::needs`](crate::AdaptiveProgram::needs)), or
/// by its driver for a program of another form
/// ([`Driver::needs`](crate::Driver::needs)), so that the chain can
/// refuse it before anything runs, rather than fail during the run: when
/// its job is given to the chain ([`Chain::run`](crate::Chain::run)).
///
/// The default states nothing but the run's shots, held at once, which
/// every run implies, so any executor that can hold them accepts it: a
/// program states a need only when it wants it checked. New needs are
/// added as fields whose default states nothing, hence `#[non_exhaustive]`:
/// build one from the default and its `with_` methods.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is an independent need, checked one to one against a capability"
)]
pub struct Needs {
    /// The qubits each shot uses; `None`: not stated, not checked.
    pub qubits: Option<u32>,
    /// The program continues a shot in a later call, where an earlier call
    /// left it (to feed its outcomes forward, to read again, or only to go
    /// on), checked against [`Capabilities::resumable_shots`]: the executor
    /// keeps a state per shot between calls.
    pub resumable_shots: bool,
    /// The program prepares qubits again during the run, checked against
    /// [`Capabilities::resets`]. Where the program states its
    /// [`instruction_set`](Self::instruction_set), the reset among them must
    /// also match one of the chain's set, which is the guarantee; this need
    /// informs only where no set is described.
    pub resets: bool,
    /// Each shot replays: run again on the run's seed, alone or among other
    /// shots, in one call or split over several, it gives what it gave, since
    /// it draws only from its own random numbers; checked against
    /// [`Capabilities::replayable`].
    pub replayable: bool,
    /// The report at close gives each shot's log probability.
    pub log_probability: bool,
    /// The instructions the caller will send, each taking the operands it
    /// describes; `None`: not stated, and not checked for a typed caller.
    /// A caller of calls by opcode
    /// ([`Instruction::BY_OPCODE`](crate::Instruction::BY_OPCODE)) must
    /// state the set that numbers its calls: a chain refuses `None` as an
    /// unmet [`Need::InstructionSet`] when its job is given to it
    /// ([`Chain::run`](crate::Chain::run); not yet
    /// [`Capabilities::admit`]).
    pub instruction_set: Option<Arc<InstructionSet>>,
}

impl Needs {
    /// These needs, with `qubits` qubits per shot. Not stated is the
    /// default.
    #[must_use]
    pub fn with_qubits(mut self, qubits: u32) -> Self {
        self.qubits = Some(qubits);
        self
    }

    /// These needs, with or without resumable shots.
    #[must_use]
    pub fn with_resumable_shots(mut self, resumable_shots: bool) -> Self {
        self.resumable_shots = resumable_shots;
        self
    }

    /// These needs, with or without resets.
    #[must_use]
    pub fn with_resets(mut self, resets: bool) -> Self {
        self.resets = resets;
        self
    }

    /// These needs, with or without replayable shots.
    #[must_use]
    pub fn with_replayable(mut self, replayable: bool) -> Self {
        self.replayable = replayable;
        self
    }

    /// These needs, with or without each shot's log probability at close.
    #[must_use]
    pub fn with_log_probability(mut self, log_probability: bool) -> Self {
        self.log_probability = log_probability;
        self
    }

    /// These needs, with the instruction set `instruction_set`.
    #[must_use]
    pub fn with_instruction_set(mut self, instruction_set: Option<Arc<InstructionSet>>) -> Self {
        self.instruction_set = instruction_set;
        self
    }
}

/// A property a job's program may need from the components of a chain, and
/// which a component offers or not.
///
/// Each new need of [`Needs`] adds a variant, hence `#[non_exhaustive]`: a
/// match outside this crate has an arm for the needs it does not know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Need {
    /// The qubits each shot uses.
    Qubits,
    /// A shot resumed in a later call, where an earlier call left it, so
    /// a state per shot must be kept between calls.
    ResumableShots,
    /// Qubits prepared again during the run.
    Resets,
    /// The run's shots, held at once, however the executor holds them (a
    /// state each, or shared). Beyond the limit an
    /// executor declares, the shots must be split over several runs.
    Shots,
    /// Each shot replays: run again on the run's seed, alone or among other
    /// shots, in one call or split over several, it gives what it gave, since
    /// it draws only from its own random numbers.
    Replayable,
    /// The log probability of each shot's draws, reported when the
    /// executor closes.
    LogProbability,
    /// Instructions of a described set, each taking the operands the caller
    /// describes.
    InstructionSet,
}

impl Capabilities {
    /// Checks `needs`, for a run of `shots`, against these capabilities,
    /// which are `component`'s, as a chain checks a job's
    /// ([`Chain::run`](crate::Chain::run)). A rejection names `component`,
    /// so the caller knows which one to change.
    ///
    /// # Errors
    ///
    /// [`Rejected::Unmet`] for the first need these capabilities do not
    /// meet, in the order qubits, resumable shots, resets, replayable, log
    /// probability, instruction set, shots (those of `shots`, at once).
    pub fn admit(&self, component: &str, shots: &ShotSet, needs: &Needs) -> Result<(), Rejected> {
        // Known gap: a caller of calls by opcode that states no set is not
        // refused here, as a chain refuses its job (`Chain::run`), since
        // `admit` does not know the caller's instruction type. Closing it
        // needs `admit` to take it (`I: Instruction`); blocks neither this
        // commit nor a later one, since `Chain::run` makes the check for
        // every run.
        match self.check_needs(component, Some(shots.len()), needs, false) {
            Ok(_) => Ok(()),
            Err(unmet) => Err(Rejected::Unmet(
                unmet.into_iter().next().expect("a refusal names a need"),
            )),
        }
    }

    /// Checks `needs` against these capabilities, which are `component`'s:
    /// the one check of a caller's needs, for [`admit`](Self::admit) and a
    /// chain's job ([`Chain::run`](crate::Chain::run)). Gives where
    /// the needed instructions are in these capabilities' set, when `needs`
    /// states some. `by_opcode` says whether the caller's calls name their
    /// instruction by opcode
    /// ([`Instruction::BY_OPCODE`](crate::Instruction::BY_OPCODE)), which then
    /// means something only in a set the caller states. `shots` is the
    /// number of shots the executor is to hold at once, or `None` when the
    /// caller's shots are not a set, which is a fault of its own
    /// ([`Rejected::NoShots`], [`Rejected::RepeatedShot`]): they are then not
    /// checked.
    ///
    /// # Errors
    ///
    /// Every need these capabilities do not meet, in the order of
    /// [`admit`](Self::admit), so that the caller can fix them all at once;
    /// a caller of calls by opcode does not meet [`Need::InstructionSet`] when
    /// it states no set, or an empty one that these capabilities cannot
    /// number since they state none ([`UnmetNeed::undescribed`]).
    pub(crate) fn check_needs(
        &self,
        component: &str,
        shots: Option<usize>,
        needs: &Needs,
        by_opcode: bool,
    ) -> Result<Option<OpcodeMap>, Vec<UnmetNeed>> {
        let mut unmet = Vec::new();
        let mut check = |need, needed: u64, offered: u64| {
            unmet.extend(UnmetNeed::of(component, need, needed, offered));
        };
        if let (Some(needed), Some(offered)) = (needs.qubits, self.qubits) {
            check(Need::Qubits, needed.into(), offered.into());
        }
        check(
            Need::ResumableShots,
            needs.resumable_shots.into(),
            self.resumable_shots.into(),
        );
        check(Need::Resets, needs.resets.into(), self.resets.into());
        check(
            Need::Replayable,
            needs.replayable.into(),
            self.replayable.into(),
        );
        check(
            Need::LogProbability,
            needs.log_probability.into(),
            self.log_probability.into(),
        );
        let map = needs.instruction_set.as_deref().and_then(|needed| {
            UnmetNeed::instruction_set(component, needed, self.instruction_set.as_deref())
                .map_err(|instructions| unmet.push(instructions))
                .ok()
        });
        // An empty set needed matches no set offered, but calls by opcode
        // are numbered only where both sides state one.
        let numbered = needs.instruction_set.is_some() && self.instruction_set.is_some();
        if by_opcode && !numbered && !unmet.iter().any(|u| u.need == Need::InstructionSet) {
            unmet.push(UnmetNeed::undescribed(component));
        }
        if let (Some(needed), Some(offered)) = (shots, self.max_shots) {
            let needed = u64::try_from(needed).unwrap_or(u64::MAX);
            unmet.extend(UnmetNeed::of(component, Need::Shots, needed, offered));
        }
        if unmet.is_empty() {
            Ok(map)
        } else {
            Err(unmet)
        }
    }
}

/// A need that a component cannot honour, and which component.
///
/// Needed because a chain is several components, and the caller must know
/// which one to change (a bigger simulator, a different executor).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnmetNeed {
    /// The component whose capabilities were checked, by its Rust type name.
    /// For a chain it is the most upstream component, whose type name
    /// includes those it wraps, so the caller can find the one that sets the
    /// limit.
    pub component: String,
    /// The need it cannot honour.
    pub need: Need,
    /// How much was needed (1 for a yes-or-no need).
    pub needed: u64,
    /// How much it offers (0 for a yes-or-no need it lacks).
    pub offered: u64,
    /// For [`Need::InstructionSet`], each needed instruction it does not
    /// accept, in the order the caller declares them, so that the caller
    /// can fix them all at once; empty for the other needs, and when the
    /// caller sends calls by opcode but states no set, or an empty one that
    /// the component cannot number (it states none), which is unmet as a
    /// yes-or-no need.
    pub mismatches: Vec<InstructionMismatch>,
}

impl UnmetNeed {
    /// Checks that `component` offers at least what is `needed`.
    ///
    /// # Errors
    ///
    /// [`Rejected::Unmet`] when `needed` exceeds `offered`, naming
    /// `component`.
    pub fn check(component: &str, need: Need, needed: u64, offered: u64) -> Result<(), Rejected> {
        match Self::of(component, need, needed, offered) {
            Some(unmet) => Err(Rejected::Unmet(unmet)),
            None => Ok(()),
        }
    }

    /// The need `component` does not meet, when `needed` exceeds `offered`.
    fn of(component: &str, need: Need, needed: u64, offered: u64) -> Option<UnmetNeed> {
        (needed > offered).then(|| UnmetNeed {
            component: component.to_owned(),
            need,
            needed,
            offered,
            mismatches: Vec::new(),
        })
    }

    /// The need of an instruction set that a program sending calls by opcode
    /// (a job's, given to a chain) does not meet when it
    /// states no set, or an empty one that `component` cannot number (it
    /// states none): an opcode is then just a number, so whether
    /// `component` runs the calls as meant cannot be checked. A yes-or-no
    /// need (1 needed, 0 offered), with no mismatch.
    pub(crate) fn undescribed(component: &str) -> UnmetNeed {
        UnmetNeed {
            component: component.to_owned(),
            need: Need::InstructionSet,
            needed: 1,
            offered: 0,
            mismatches: Vec::new(),
        }
    }

    /// Matches `needed` in `offered`, the set `component` accepts (`None`:
    /// it does not say), as [`InstructionSet::opcodes_in`] does: the one
    /// check of a need of instructions, for [`Capabilities::check_needs`].
    pub(crate) fn instruction_set(
        component: &str,
        needed: &InstructionSet,
        offered: Option<&InstructionSet>,
    ) -> Result<OpcodeMap, UnmetNeed> {
        let matched = match offered {
            Some(offered) => needed.opcodes_in(offered),
            None => needed.opcodes_in(&InstructionSet::new("none")),
        };
        matched.map_err(|mismatches| {
            let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
            let total = needed.mnemonics().count();
            UnmetNeed {
                component: component.to_owned(),
                need: Need::InstructionSet,
                needed: count(total),
                offered: count(total - mismatches.len()),
                mismatches,
            }
        })
    }
}

impl fmt::Display for UnmetNeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (c, offered, needed) = (&self.component, self.offered, self.needed);
        let s = if offered == 1 { "" } else { "s" };
        match self.need {
            Need::Qubits => write!(f, "{c} offers {offered} qubit{s} per shot, {needed} needed"),
            Need::Shots => write!(
                f,
                "{c} holds at most {offered} shot{s} at once, {needed} needed"
            ),
            Need::ResumableShots => write!(f, "{c} does not offer resumable shots"),
            Need::Resets => write!(f, "{c} does not offer resets"),
            Need::Replayable => write!(f, "{c} does not offer replayable shots"),
            Need::LogProbability => {
                write!(f, "{c} does not offer each shot's log probability at close")
            }
            Need::InstructionSet if self.mismatches.is_empty() => write!(
                f,
                "the program or {c} states no instruction set, so {c} cannot check its calls \
                 by opcode"
            ),
            Need::InstructionSet => {
                write!(
                    f,
                    "{c} accepts {offered} of the {needed} instructions needed: "
                )?;
                write_mismatches(f, &self.mismatches)
            }
        }
    }
}

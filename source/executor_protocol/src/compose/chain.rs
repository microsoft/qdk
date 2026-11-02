// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A complete chain ([`Chain`]): a driver, adapters and an executor, built
//! and checked, which runs the job given to it,
//! `chain.run(program, shots)`, checked before any call ([`JobFaults`]),
//! as a [`Run`].

use std::any::type_name;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::{
    AdaptiveProgram, Capabilities, Close, DrivenProgram, Driver, Executor, Instruction,
    InstructionBatch, IntoShotSet, Rejected, Run,
};
#[cfg(doc)]
use crate::{BuildFaults, ChainHead, ShotSet, UnmetNeed};

/// A driver composed with an executor, through adapters or not: the
/// chain, built and checked ([`ChainHead`] `>>`
/// executor), which runs the job given to it ([`run`](Chain::run)).
///
/// A chain runs one job: the run takes the executor, and closes it with
/// the job's report. A function that builds the chain builds it again for
/// the next job, each with its own executor and its own report. On an
/// executor that offers replayable shots ([`Capabilities::replayable`]), a
/// shot then runs again with the outcomes it gave, whichever other shots
/// the job names; on one that does not, its outcomes may differ.
/// Planned, with no milestone yet: a chain that runs many jobs, each on
/// components opened afresh for it, for a service that runs many jobs or a
/// sweep of one program over parameters; it needs a protocol addition, a
/// per-run session.
///
/// Known limit, as for a [`Run`]: a chain dropped without running a job,
/// or whose job is refused, does not close its executor.
///
/// # Type parameters
///
/// Each is set by `ChainHead<T, L> >> executor`:
///
/// - `T`: the driver ([`Driver`]), the [`ChainHead`]'s.
/// - `I`: the type of the instructions the driver sends, which the
///   first of the head's adapters `L` accepts, or the executor where there
///   is none.
/// - `E`: the executor its calls reach: the adapters `L` composed with the
///   executor, which together are an executor, or the executor itself.
///
/// # Example
///
/// A function that builds a chain for each job names the chain's type by
/// its driver, its instruction and what its executor is: an executor
/// of batches of that instruction, which closes with a report of a type
/// stated (`Close<Report = …>`), so that the report of each run can be
/// read.
///
/// ```
/// use executor_protocol::{
///     BuildFaults, Capabilities, Chain, Close, DefaultDriver, Executor,
///     ExecutorReport, InstructionBatch, Needs, OnePerShot, Outcome,
///     Outcomes, ProgramInput, StaticProgram, TypedInstruction, driver,
///     executor_fn,
/// };
///
/// /// An instruction of a device of one bit per shot.
/// #[derive(Clone, Copy, Debug)]
/// enum Op {
///     /// Flips the bit.
///     Flip,
///     /// Reads the bit, under a tag.
///     Read(u32),
/// }
///
/// impl TypedInstruction for Op {}
///
/// /// The program of the device: flips and reads a shot's bit.
/// fn run_bit(
///     bit: &mut bool,
///     input: &ProgramInput<'_, Op>,
///     outcomes: &mut Outcomes<'_>,
/// ) -> Result<(), String> {
///     for op in input.instructions() {
///         match *op {
///             Op::Flip => *bit = !*bit,
///             Op::Read(tag) => {
///                 outcomes.push(Outcome::new(tag, (*bit).into()));
///             }
///         }
///     }
///     Ok(())
/// }
///
/// /// A chain of the crate's driver on the device, a bit per shot.
/// fn one_bit_chain(
///     seed: u64,
/// ) -> Result<
///     Chain<
///         DefaultDriver,
///         Op,
///         impl Executor<InstructionBatch<Op>>
///             + Close<Report = ExecutorReport<()>>,
///     >,
///     BuildFaults,
/// > {
///     let executor =
///         executor_fn(Capabilities::default(), |_| Ok(false), run_bit)
///             .seed(seed)
///             .shots(OnePerShot);
///     driver() >> executor
/// }
///
/// let program = StaticProgram::new(
///     Needs::default(),
///     vec![vec![Op::Flip, Op::Read(0)]],
/// );
/// let (outcomes, report) = one_bit_chain(7)?.run(program, 0..4)?.wait();
/// assert_eq!(outcomes?.len(), 4, "one readout per shot");
/// assert_eq!(report.calls, 1, "one call for the program's one batch");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Chain<T, I, E> {
    driver: T,
    executor: E,
    /// What the chain offers its driver, checked against the job's needs.
    offered: Capabilities,
    _instruction: PhantomData<fn() -> I>,
}

impl<T, I, E> Chain<T, I, E> {
    /// `driver`, calling `executor`, which offers it `offered`.
    pub(crate) fn new(driver: T, executor: E, offered: Capabilities) -> Self {
        Self {
            driver,
            executor,
            offered,
            _instruction: PhantomData,
        }
    }
}

impl<T, I, E> Chain<T, I, E>
where
    I: Instruction,
    E: Executor<InstructionBatch<I>> + Close,
{
    /// The run of `program` on `shots`: what runs when it is waited for or
    /// awaited ([`Run::wait`]), and gives the program's output with the
    /// executor's report, the executor closed. Until then, nothing runs.
    ///
    /// The job is checked here, before any call, and every fault is
    /// reported together: its shots must be a set ([`ShotSet`]), the
    /// program's needs ([`Driver::needs`]) must be met by what the chain
    /// offers, its shots counted against the shots the executor holds at
    /// once when they are a set (shots that are not are reported as such,
    /// not counted), and the driver must find no fault in the program
    /// ([`Driver::check`]). Calls by opcode reach the executor
    /// renumbered where the program's set numbers its instructions
    /// otherwise.
    ///
    /// # Errors
    ///
    /// [`JobFaults`], with nothing sent: every fault of the job, its
    /// shots' first (one at most, as [`ShotSet::new`] reports it:
    /// [`Rejected::NoShots`], or [`Rejected::RepeatedShot`] for the least
    /// shot named twice), then each unmet need ([`Rejected::Unmet`]), then
    /// the program's.
    ///
    /// Known issue: a job that names several shots twice reports only the
    /// least of them, so the next is found on the next run.
    /// [`IntoShotSet`], as [`ShotSet::new`], gives one fault, and changing
    /// either to give them all changes its public signature.
    pub fn run<P>(
        self,
        program: P,
        shots: impl IntoShotSet,
    ) -> Result<Run<DrivenProgram<T, P>, E>, JobFaults>
    where
        T: Driver<P, Instruction = I>,
    {
        let mut faults = Vec::new();
        let shots = shots
            .into_shot_set()
            .map_err(|rejected| faults.push(rejected))
            .ok();
        let program = DrivenProgram::new(self.driver, program);
        let checked = self.offered.check_needs(
            type_name::<E>(),
            shots.as_ref().map(|shots| shots.as_slice().len()),
            &program.needs(),
            I::BY_OPCODE,
        );
        let renumbering = match checked {
            Ok(map) => map.and_then(I::renumbering).map(Arc::new),
            Err(unmet) => {
                faults.extend(unmet.into_iter().map(Rejected::Unmet));
                None
            }
        };
        faults.extend(program.check());
        match shots {
            Some(shots) if faults.is_empty() => {
                let executor = I::downstream(self.executor, renumbering.as_ref());
                Ok(Run::new(program, shots, executor))
            }
            _ => Err(JobFaults(faults)),
        }
    }
}

/// Every fault of a job given to a [`Chain`] ([`Chain::run`]), its shots'
/// first (one at most: no shots, or the least shot named twice), then each
/// unmet need, then the program's ([`Driver::check`]), so that whoever
/// gives the job can fix them all at once. Nothing was sent. It reads as
/// one message: each fault's text, in their order, joined by `"; "`, as
/// [`BuildFaults`] reads.
///
/// Each unmet need is a [`Rejected::Unmet`], an [`UnmetNeed`] naming the
/// chain's executor, adapters included, in the order
/// [`Capabilities::admit`] checks them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobFaults(pub Vec<Rejected>);

impl fmt::Display for JobFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        super::write_faults(f, &self.0, |f, rejected| rejected.fmt(f))
    }
}

impl std::error::Error for JobFaults {}

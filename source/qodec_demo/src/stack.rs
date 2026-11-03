//! The Qodec runtime's own stack: a program run shot by shot through
//! `QodecPlan::instantiate`, every layer over the one below, over a bottom
//! backend per shot. The reference every chain of this crate is compared
//! with.

use std::sync::Arc;

use executor_protocol::{Fold, ShotId, ShotOutcome, ShotSet, shot_seed};
use qodec::Qodec;
use qodec_runtime::{Backend, BasicReadout, FrameDecoder, OwnedParam, QodecPlan, Readout, Status};

use crate::boundary::{Boundary, outcomes};
use crate::coin::CoinBackend;
use crate::profiler::Profile;
use crate::program::Program;

/// What the stack gave for a program, shot by shot.
///
/// # Type parameters
///
/// - `B`: each shot's bottom backend, kept as the shot left it; set by the
///   backends [`on_the_stack_over`] is given ([`CoinBackend`] for
///   [`on_the_stack`]).
#[derive(Clone, Debug)]
pub struct StackRan<B> {
    /// The outcomes of the top readouts, under the program's tags.
    pub outcomes: Vec<ShotOutcome>,
    /// Each top result's status, with the tag of its first readout, shot by
    /// shot, a shot's in the order of its results.
    pub statuses: Vec<(ShotId, u32, Status)>,
    /// Each shot's bottom backend, as the shot left it, in the shots'
    /// order.
    pub backends: Vec<(ShotId, B)>,
}

impl StackRan<CoinBackend> {
    /// What the bottom ran, over all shots.
    #[must_use]
    pub fn ran(&self) -> Profile {
        let mut ran = Profile::default();
        for (_, backend) in &self.backends {
            ran.fold(backend.ran().clone());
        }
        ran
    }
}

/// What the stack gives for `program` on `shots`, over the coins that
/// [`coin_device`](crate::coin::coin_device) draws in a run of seed `seed`,
/// each shot's keeping the batches sent ([`CoinBackend::sent`]).
///
/// # Errors
///
/// As [`on_the_stack_over`].
pub fn on_the_stack(
    qodec: &Qodec,
    program: &Program,
    shots: &ShotSet,
    seed: u64,
) -> Result<StackRan<CoinBackend>, String> {
    let bottom_isa = &qodec.layers().last().expect("a layer").instruction_set;
    let bottom = Arc::new(Boundary::new(bottom_isa).map_err(|f| f.join("; "))?);
    on_the_stack_over(qodec, program, shots, |shot| {
        CoinBackend::new(shot_seed(seed, shot), Arc::clone(&bottom)).keeping_sent()
    })
}

/// What the stack gives for `program` on `shots`, over the bottom backend
/// `backend_of(shot)` of each shot.
///
/// # Type parameters
///
/// - `B`: the bottom backend of a shot, set by what `backend_of` returns.
///
/// The program's statements reach the stack through [`Boundary`], as they
/// reach a chain's Qodec layer: [`Boundary::call`] when the program is
/// parsed, then [`Boundary::instruction`]. So a fault there shows on both
/// sides alike, and comparing a chain with this stack cannot find it.
///
/// # Errors
///
/// When the runtime cannot plan the qodec, or the first error of a shot.
pub fn on_the_stack_over<B>(
    qodec: &Qodec,
    program: &Program,
    shots: &ShotSet,
    mut backend_of: impl FnMut(ShotId) -> B,
) -> Result<StackRan<B>, String>
where
    B: Backend<usize, OwnedParam, BasicReadout>,
    B::Error: std::fmt::Debug,
{
    let plan = QodecPlan::<BasicReadout>::new(qodec, &FrameDecoder).map_err(|e| e.0)?;
    let (mut all, mut statuses, mut backends) = (Vec::new(), Vec::new(), Vec::new());
    for &shot in shots.iter() {
        let mut backend = backend_of(shot);
        {
            let mut runtime = plan.instantiate(&mut backend);
            for batch in &program.batches {
                let (ins, tags): (Vec<_>, Vec<_>) =
                    batch.iter().map(|c| program.top.instruction(c)).unzip();
                let result = runtime.execute_batch(&ins).map_err(|e| format!("{e:?}"))?;
                let mut results = result.results.iter();
                for tags in tags.iter().filter(|t| !t.is_empty()) {
                    let r = results.next().expect("one result per call with readouts");
                    let bits = r.outcomes().iter().chain(r.flags()).copied();
                    all.extend(outcomes(shot, tags, bits));
                    statuses.push((shot, tags[0], r.status()));
                }
            }
        }
        backends.push((shot, backend));
    }
    Ok(StackRan {
        outcomes: all,
        statuses,
        backends,
    })
}

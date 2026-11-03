// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::tests::facilities::adapter_fn::{Offering, each_capability_alone};
use crate::tests::map_executor;
use crate::tests::test_double::TestDoubleExecutor;
use crate::tests::{
    Bits, Relabel, StatesItsOfferFromScratch, bits_set, flip_and_toss, reversed_set, run_and_close,
    values,
};
use crate::{
    Adapter, AdapterBoundary, AdapterSeeds, Apply, Below, BuildError, CallInput, Capabilities,
    Close, ComponentId, Describe, Executor, ExecutorFn, ExecutorLogic, InstructionBatch,
    InstructionCall, InstructionSet, MakeState, OnePerShot, Outcomes, ShotContext, Value,
    adapter_fn, compose, executor_fn,
};

/// A code whose blocks are three qubits, a fault of its own against the two
/// bits, upstream of a noise model that draws with the executor's id: the
/// code's fault is reported with the noise model's, of the code's own type.
#[test]
fn an_adapters_own_faults_are_reported_with_the_chains() {
    #[derive(Debug)]
    struct NoBlock {
        qubits: u32,
    }
    impl std::fmt::Display for NoBlock {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{} qubits hold no block of three", self.qubits)
        }
    }
    impl std::error::Error for NoBlock {}
    struct Code {
        block_size: u32,
    }
    impl Adapter for Code {
        type Accepts = InstructionCall;
        type Sends = InstructionCall;
        fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
            match boundary.downstream().qubits {
                Some(qubits) if qubits < self.block_size => boundary.fault(NoBlock { qubits }),
                _ => {}
            }
            self
        }
    }
    impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Code {
        type Output = E;
        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    let noise =
        || map_executor::<InstructionCall, _>(|below| below).drawing(ComponentId::LAST_EXECUTOR);
    let Err(faults) = compose(
        compose(Code { block_size: 3 }, noise()),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    ) else {
        panic!("both adapters are faulty");
    };
    assert_eq!(faults.0.len(), 2, "{faults}");
    assert!(faults.0[0].component.ends_with("Code"), "{faults}");
    let BuildError::Adapter(own) = &faults.0[0].error else {
        panic!("the code's fault is its own: {faults}");
    };
    assert_eq!(own.downcast_ref::<NoBlock>().map(|f| f.qubits), Some(2));
    assert_eq!(
        faults.0[1].error,
        BuildError::RngInUse(ComponentId::LAST_EXECUTOR)
    );
    let fits = compose(
        Code { block_size: 2 },
        TestDoubleExecutor::with_model(Bits::default(), 7),
    );
    assert!(fits.is_ok(), "two bits hold a block of two");
}

/// An adapter whose own fault has a cause, a missing calibration file:
/// the build error gives that cause as its source.
#[test]
fn an_adapters_own_fault_gives_its_source() {
    #[derive(Debug)]
    struct NoCalibration(std::io::Error);
    impl std::fmt::Display for NoCalibration {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("no calibration")
        }
    }
    impl std::error::Error for NoCalibration {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    struct Uncalibrated;
    impl Adapter for Uncalibrated {
        type Accepts = InstructionCall;
        type Sends = InstructionCall;
        fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
            boundary.fault(NoCalibration(std::io::Error::other("the file is missing")));
            self
        }
    }
    impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Uncalibrated {
        type Output = E;
        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    let Err(faults) = compose(
        Uncalibrated,
        TestDoubleExecutor::with_model(Bits::default(), 7),
    ) else {
        panic!("the adapter is faulty");
    };
    let source = std::error::Error::source(&faults.0[0].error).map(ToString::to_string);
    assert_eq!(source.as_deref(), Some("the file is missing"));
}

/// An adapter that keeps half the qubits, and accepts the instructions
/// it sends numbered otherwise: the executor composed describes both.
#[test]
fn an_executor_composed_describes_what_its_adapters_derive() {
    struct Halves;
    impl Adapter for Halves {
        type Accepts = InstructionCall;
        type Sends = InstructionCall;
        fn sends(&self) -> Option<Arc<InstructionSet>> {
            Some(Arc::new(bits_set()))
        }
        fn accepts(&self) -> Option<Arc<InstructionSet>> {
            Some(Arc::new(reversed_set()))
        }
        fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
            let qubits = boundary.downstream().qubits.map(|q| q / 2);
            boundary.offered_upstream().qubits = qubits;
            self
        }
    }
    impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Halves {
        type Output = E;
        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    let executor = compose(Halves, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("it sends the downstream set");
    let capabilities = executor.capabilities();
    assert_eq!(capabilities.qubits, Some(1));
    assert_eq!(
        capabilities.instruction_set.as_deref(),
        Some(&reversed_set())
    );
}

/// An adapter that sends calls in another numbering, and can send in the
/// shot's: it does, and its calls reach the shot as they are.
#[test]
fn an_adapter_that_can_send_in_the_downstream_numbering_does() {
    let relabel = Relabel::new(reversed_set(), true);
    let watch = relabel.with(&relabel.target);
    let downstream = Bits::default();
    let received = Arc::clone(&downstream.received);
    let executor = compose(relabel, TestDoubleExecutor::with_model(downstream, 7))
        .expect("it sends the downstream set");
    let outcomes = run_and_close(executor, &[0], &flip_and_toss(&bits_set())[0]);
    assert_eq!(values(&outcomes), [(0, 0, Value::One)], "x, then measure");
    assert_eq!(
        watch.sent.load(Ordering::SeqCst),
        received.load(Ordering::SeqCst)
    );
}

/// The id the noise models of these tests draw with.
const NOISE: ComponentId = ComponentId::named("noise");

/// An adapter written by hand, drawing with `draws` if any, which keeps
/// the seeds it is given when built ([`AdapterBoundary::drawing_seeds`]),
/// each recorded in `read`.
struct KeepsItsSeeds {
    draws: Option<ComponentId>,
    read: Rc<RefCell<Vec<Option<AdapterSeeds>>>>,
}

impl KeepsItsSeeds {
    /// One drawing with `draws` if any, recording in `read`.
    fn new(draws: Option<ComponentId>, read: &Rc<RefCell<Vec<Option<AdapterSeeds>>>>) -> Self {
        Self {
            draws,
            read: Rc::clone(read),
        }
    }

    /// The run's seed of each seeds recorded.
    fn run_seeds(read: &RefCell<Vec<Option<AdapterSeeds>>>) -> Vec<Option<u64>> {
        let read = read.borrow();
        let run_seed = |seeds: &Option<AdapterSeeds>| seeds.map(|s| s.shot(0).run_seed());
        read.iter().map(run_seed).collect()
    }
}

impl Adapter for KeepsItsSeeds {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn draws(&self) -> Option<ComponentId> {
        self.draws
    }

    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        self.read.borrow_mut().push(boundary.drawing_seeds());
        self
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for KeepsItsSeeds {
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// An executor of calls on the bits that runs nothing, given no run's
/// seed until `.seed` gives it one.
fn bits_fn()
-> ExecutorFn<InstructionCall, (), impl MakeState<()>, impl ExecutorLogic<InstructionCall, ()>> {
    executor_fn(
        Capabilities::default().with_instruction_set(Some(Arc::new(bits_set()))),
        |_| Ok(()),
        |(): &mut (), _: &CallInput<'_, InstructionCall>, _: &mut Outcomes<'_>| Ok(()),
    )
}

/// A noise model written by hand, upstream of an adapter between, upstream
/// of an executor given the run's seed 7 (`.seed(7)`): built, its seeds are
/// of the run's seed 7, though only the executor is given it, through the
/// adapter between, whichever it is (a recorder written by hand, or an
/// `adapter_fn` with one state or a state per shot, drawing or not) and
/// whether the noise model is composed with it first or with the executor
/// it makes.
#[test]
fn an_adapter_reads_when_built_the_runs_seed_the_last_executor_is_given() {
    let read = Rc::default();
    let noise = || KeepsItsSeeds::new(Some(NOISE), &read);
    let recorder = || map_executor::<InstructionCall, _>(|below| below);
    let pass = |(): &mut (),
                input: &CallInput<'_, InstructionCall>,
                below: &mut Below<'_, InstructionCall>,
                _: &mut Outcomes<'_>| below.send(input.instructions());
    let observer = || adapter_fn(|_: ShotContext| Ok(()), pass);
    let between = ComponentId::named("between");
    let drawing = || observer().drawing(between);
    let fits = "the noise model draws with its own id";
    let built = "the executor is given the run's seed";
    compose(compose(noise(), recorder()), bits_fn().seed(7)).expect(fits);
    compose(
        noise(),
        compose(recorder(), bits_fn().seed(7)).expect(built),
    )
    .expect(fits);
    compose(compose(noise(), observer()), bits_fn().seed(7)).expect(fits);
    compose(
        noise(),
        compose(observer(), bits_fn().seed(7)).expect(built),
    )
    .expect(fits);
    compose(compose(noise(), drawing()), bits_fn().seed(7)).expect(fits);
    compose(noise(), compose(drawing(), bits_fn().seed(7)).expect(built)).expect(fits);
    let per_shot = || observer().shots(OnePerShot);
    compose(compose(noise(), per_shot()), bits_fn().seed(7)).expect(fits);
    compose(
        noise(),
        compose(per_shot(), bits_fn().seed(7)).expect(built),
    )
    .expect(fits);
    let drawing_per_shot = || drawing().shots(OnePerShot);
    compose(compose(noise(), drawing_per_shot()), bits_fn().seed(7)).expect(fits);
    compose(
        noise(),
        compose(drawing_per_shot(), bits_fn().seed(7)).expect(built),
    )
    .expect(fits);
    assert_eq!(KeepsItsSeeds::run_seeds(&read), [Some(7); 10]);
}

/// A noise model written by hand, drawing with [`NOISE`], and an
/// `adapter_fn` drawing with [`NOISE`] whose state records its seed, each
/// over an executor given the run's seed 7: shot 3 of the one written by
/// hand draws from the seed shot 3's state of the `adapter_fn` draws from,
/// so that both forms of adapter draw alike, the id stated once.
#[test]
fn an_adapter_written_by_hand_draws_from_the_seeds_an_adapter_fn_with_its_id_draws_from() {
    let read = Rc::default();
    compose(KeepsItsSeeds::new(Some(NOISE), &read), bits_fn().seed(7))
        .expect("the noise model draws with its own id");
    let written_by_hand = (read.borrow()[0])
        .expect("its seeds are given")
        .shot(3)
        .seed();
    let seeds = Rc::new(RefCell::new(Vec::new()));
    let noise = {
        let seeds = Rc::clone(&seeds);
        adapter_fn(
            move |context: ShotContext| {
                seeds.borrow_mut().push(context.seed());
                Ok(())
            },
            |(): &mut (),
             input: &CallInput<'_, InstructionCall>,
             below: &mut Below<'_, InstructionCall>,
             _: &mut Outcomes<'_>| { below.send(input.instructions()) },
        )
        .drawing(NOISE)
        .shots(OnePerShot)
    };
    let executor = (noise >> bits_fn().seed(7)).expect("the noise draws with its own id");
    run_and_close(executor, &[3], &flip_and_toss(&bits_set())[0]);
    assert_eq!(*seeds.borrow(), [written_by_hand]);
}

/// A noise model written by hand, upstream of a code that assigns its
/// whole offer upstream, upstream of an executor given the run's seed 7:
/// built, the noise model reads 7, since the seed passes up whatever an
/// adapter offers.
#[test]
fn an_adapter_reads_the_runs_seed_through_one_that_states_its_offer_whole() {
    let read = Rc::default();
    compose(
        compose(
            KeepsItsSeeds::new(Some(NOISE), &read),
            StatesItsOfferFromScratch,
        ),
        bits_fn().seed(7),
    )
    .expect("the noise model draws with its own id");
    assert_eq!(KeepsItsSeeds::run_seeds(&read), [Some(7)]);
}

/// A noise model written by hand over an executor given no run's seed (an
/// `executor_fn` without `.seed`): the chain is refused when it is built,
/// the noise model's id named, rather than draw from a seed made up.
#[test]
fn an_adapter_written_by_hand_drawing_with_no_run_seed_below_is_refused() {
    let read = Rc::default();
    let Err(faults) = compose(KeepsItsSeeds::new(Some(NOISE), &read), bits_fn()) else {
        panic!("no run's seed reaches the noise model")
    };
    let errors: Vec<_> = faults.0.into_iter().map(|fault| fault.error).collect();
    assert_eq!(errors, [BuildError::NoRunSeed(NOISE)]);
}

/// A noise model written by hand over an executor given no run's seed:
/// built before the chain is refused, it is given no seeds, rather than
/// seeds of a run's seed made up.
#[test]
fn an_adapter_refused_for_no_run_seed_is_given_no_seeds() {
    let read = Rc::default();
    let Err(_) = compose(KeepsItsSeeds::new(Some(NOISE), &read), bits_fn()) else {
        panic!("no run's seed reaches the noise model")
    };
    assert_eq!(KeepsItsSeeds::run_seeds(&read), [None]);
}

/// A code written by hand, which draws nothing and asks for seeds when
/// built, over an executor given the run's seed 7: it is given none, so
/// that no adapter draws under an id it does not state.
#[test]
fn an_adapter_that_draws_nothing_is_given_no_seeds() {
    let read = Rc::default();
    compose(KeepsItsSeeds::new(None, &read), bits_fn().seed(7)).expect("the code draws nothing");
    assert_eq!(KeepsItsSeeds::run_seeds(&read), [None]);
}

/// A noise model written by hand, which reads when built what it offers
/// upstream, over a simulator that reports each shot's log probability: it
/// reads no log probability, since the simulator's log probability leaves out
/// the noise model's draws, so that an offer it makes from what it reads
/// does not claim them.
#[test]
fn an_adapter_that_draws_reads_no_log_probability_in_its_offer_when_built() {
    struct ReadsItsOffer(Rc<Cell<Option<bool>>>);
    impl Adapter for ReadsItsOffer {
        type Accepts = InstructionCall;
        type Sends = InstructionCall;

        fn draws(&self) -> Option<ComponentId> {
            Some(NOISE)
        }

        fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
            self.0
                .set(Some(boundary.offered_upstream().log_probability));
            self
        }
    }
    impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for ReadsItsOffer {
        type Output = E;

        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    let simulator = Bits {
        log_probability: true,
        ..Bits::default()
    };
    let read = Rc::default();
    compose(
        ReadsItsOffer(Rc::clone(&read)),
        TestDoubleExecutor::with_model(simulator, 7),
    )
    .expect("the noise model draws with its own id");
    assert_eq!(read.get(), Some(false));
}

/// A noise model written by hand, drawing with [`NOISE`] and passing each
/// call down, over a device given the run's seed that offers one yes/no
/// capability alone, each in turn: it offers what the device offers, with
/// [`NOISE`] added to the ids drawn with, but no log probability, which
/// leaves out its draws.
#[test]
fn a_drawing_adapter_written_by_hand_offers_each_capability_below_but_the_log_probability() {
    struct Noise;
    impl Adapter for Noise {
        type Accepts = bool;
        type Sends = bool;

        fn draws(&self) -> Option<ComponentId> {
            Some(NOISE)
        }
    }
    impl<E: Executor<InstructionBatch<bool>> + Close> Apply<E> for Noise {
        type Output = E;

        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    for lone in each_capability_alone() {
        let noisy =
            compose(Noise, Offering(lone.clone())).expect("the device is given the run's seed");
        let mut expected = lone
            .drawing(NOISE)
            .expect("nothing below draws with `noise`");
        expected.log_probability = false;
        assert_eq!(noisy.capabilities(), expected);
    }
}

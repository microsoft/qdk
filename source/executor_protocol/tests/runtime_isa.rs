// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [Guide: A run-time ISA: a code given as
//! data](../docs/guides.md#a-run-time-isa-a-code-given-as-data):
//! a code whose instruction sets and gadgets are known only at run time, as
//! a Qodec file gives them, run on the protocol. The code is part of the
//! repetition code of the Python runtime's fixtures, and the device the toy,
//! both taking their instructions as calls ([`InstructionCall`]).
//!
//! The guide is evidence that the protocol meets the needs of a code's
//! adapter, not an adapter for use: the instruction sets carry Qodec's
//! actions, so the code's Stim instructions are matched to the device's by
//! what they do ([`InstructionSet::opcodes_in`]); the code is an adapter
//! (`Adapter`, `Apply`), checked against the device when the chain is
//! composed ([`compose`]), every fault at once, and built there once
//! (`Adapter::build`); the device runs calls by opcode (`OpcodeTable`);
//! and a program numbering the logical instructions its own way is composed
//! with the code, each of its batches copied into the code's numbering.
//!
//! Read `runtime_isa/repetition3.rs` (the code, as data),
//! `runtime_isa/device.rs` (the device), `runtime_isa/code.rs` (what
//! runs any such code), then the tests in their order in the file.

#[path = "runtime_isa/code.rs"]
mod code;
#[path = "runtime_isa/device.rs"]
mod device;
#[path = "runtime_isa/repetition3.rs"]
mod repetition3;
#[allow(dead_code, reason = "this guide runs no typed program on the toy")]
#[path = "toy/simulator.rs"]
mod toy;

use std::sync::{Arc, Mutex};

use executor_protocol::{
    Action, AdaptiveProgram, BuildError, BuildFaults, Capabilities, Close, Describe, Executor,
    ExecutorReport, ImmediateReply, InstructionBatch, InstructionBatchRef, InstructionCall,
    InstructionMismatch, InstructionSet, JobFaults, Need, Needs, Operand, OperandKind, Outcome,
    Pauli, ReadoutFailure, Rejected, ShotId, ShotOutcome, ShotSet, Value, adapter_fn, compose,
    conformance, driver, wait,
};

use code::{Code, CodeFault, Encoding, GadgetCall, Slot};
use device::{Device, device_set};
use repetition3::{measure, pauli, repetition3, reset};
use toy::{ToySimulator, toy_simulator};

use OperandKind::{Block, Tag};

/// The device of `qubits` qubits, on the toy.
fn device(qubits: u32) -> Device<ToySimulator> {
    Device::new(qubits, toy_simulator(0))
}

/// `code` composed upstream of `executor`.
fn code_upstream_of<E>(
    code: Code,
    executor: E,
) -> Result<
    impl Executor<InstructionBatch<InstructionCall>> + Describe + Close<Report = E::Report>,
    BuildFaults,
>
where
    E: Executor<InstructionBatch<InstructionCall>> + Describe + Close,
{
    compose(Encoding::new(code), executor)
}

/// The fixture's code composed upstream of the device of `qubits` qubits.
fn composed(qubits: u32) -> impl Executor<InstructionBatch<InstructionCall>> + Describe + Close {
    code_upstream_of(repetition3(), device(qubits)).expect("the fixture's code runs on the device")
}

/// `instructions` on shot 0 alone.
fn on_shot_0<I>(instructions: &[I]) -> InstructionBatchRef<'_, I> {
    const SHOT_0: &ShotSet = &ShotSet::one(0);
    InstructionBatchRef {
        shots: SHOT_0,
        instructions,
    }
}

/// A call of the code's logical set.
fn logical(mnemonic: &str, operands: impl IntoIterator<Item = Operand>) -> InstructionCall {
    let accepts = repetition3().accepts;
    (accepts.call(mnemonic, operands)).expect("the tests build only calls of the logical set")
}

/// A call of the device's set.
fn physical(mnemonic: &str, operands: impl IntoIterator<Item = Operand>) -> InstructionCall {
    (device_set().call(mnemonic, operands)).expect("the tests build only calls of the device")
}

/// The outcomes of `instructions` on shot 0 of `executor`, whose shot
/// downstream never fails a batch.
fn run(
    executor: &mut impl Executor<InstructionBatch<InstructionCall>>,
    instructions: &[InstructionCall],
) -> Vec<ShotOutcome> {
    let reply = executor.call(on_shot_0(instructions));
    let mut outcomes = Vec::new();
    wait(
        reply.expect("the tests send calls the code accepts"),
        &mut outcomes,
    )
    .expect("the shot downstream never fails a batch");
    outcomes
}

/// The values of `batch`'s outcomes on `executor`, for `shots`, which
/// accepts the batch and whose device never fails one.
fn values(
    executor: &mut impl Executor<InstructionBatch<InstructionCall>>,
    shots: &ShotSet,
    batch: &[InstructionCall],
) -> Vec<Value> {
    let reply = executor.call(InstructionBatchRef {
        shots,
        instructions: batch,
    });
    let mut outcomes: Vec<ShotOutcome> = Vec::new();
    wait(
        reply.expect("the executor accepts the batch"),
        &mut outcomes,
    )
    .expect("the device never fails a batch");
    outcomes.iter().map(|outcome| outcome.value).collect()
}

/// The batches the recorder of [`recorded`] saw go down, shared with the
/// test.
type Log = Arc<Mutex<Vec<Vec<InstructionCall>>>>;

/// `code` upstream of the device of 8 qubits, with an adapter between them
/// that records each batch it sends down, as it is called with it, and the
/// log of what goes down.
fn recorded(
    code: Code,
) -> (
    impl Executor<InstructionBatch<InstructionCall>> + Close,
    Log,
) {
    let log = Log::default();
    let recorder = {
        let log = Arc::clone(&log);
        adapter_fn(
            |_| Ok(()),
            move |(), input, below, _| {
                let mut batches = log.lock().expect("no test panics holding the log");
                batches.push(input.instructions().to_vec());
                below.send(input.instructions())
            },
        )
    };
    let chain = compose(Encoding::new(code), recorder);
    let executor = compose(chain, device(8));
    (executor.expect("the code runs on the device"), log)
}

/// The batches that went down, as `log` recorded them.
fn batches(log: &Log) -> Vec<Vec<InstructionCall>> {
    log.lock().expect("no test panics holding the log").clone()
}

/// An executor downstream of the code that answers every batch on each shot
/// with the same circuit readouts, whatever it holds, described as the
/// device of 8 qubits that also states each shot's log probability: ln ½,
/// as if it tossed a coin. To show how readouts decode.
struct Answers {
    outcomes: Vec<Outcome>,
    ran: Vec<ShotId>,
    closed: bool,
}

impl Describe for Answers {
    fn capabilities(&self) -> Capabilities {
        device(8).capabilities().with_log_probability(true)
    }
}

impl Executor<InstructionBatch<InstructionCall>> for Answers {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        if self.closed {
            return Err(Rejected::Closed);
        }
        let mut outcomes = Vec::new();
        for &shot in batch.shots {
            if !self.ran.contains(&shot) {
                self.ran.push(shot);
            }
            let answers = self.outcomes.iter();
            outcomes.extend(answers.map(|answer| {
                ShotOutcome::new(shot, answer.tag, answer.value).with_conditions(answer.conditions)
            }));
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

impl Close for Answers {
    type Report = ExecutorReport<()>;

    /// Each shot it ran tossed one coin.
    fn close(&mut self) -> ExecutorReport<()> {
        self.closed = true;
        let mut report = ExecutorReport::default();
        for shot in self.ran.drain(..) {
            report.log_p.add(shot, 0.5_f64.ln());
        }
        report
    }
}

/// The circuit readouts `values`, at physical tags 0, 1, …
fn answers(values: impl IntoIterator<Item = Value>) -> Answers {
    let outcomes = (0..)
        .zip(values)
        .map(|(tag, value)| Outcome::new(tag, value));
    Answers {
        outcomes: outcomes.collect(),
        ran: Vec::new(),
        closed: false,
    }
}

/// What `compose` reported, each fault the protocol's or the code's own;
/// none when it composed.
#[derive(Debug, PartialEq)]
enum Fault {
    Protocol(BuildError),
    Code(CodeFault),
}

fn faults<T>(composed: Result<T, BuildFaults>) -> Vec<Fault> {
    let Err(BuildFaults(faults)) = composed else {
        return Vec::new();
    };
    let fault = |fault: executor_protocol::BuildFault| {
        assert!(fault.component.ends_with("Encoding"), "{}", fault.component);
        match fault.error {
            BuildError::Adapter(own) => {
                let own = own.downcast_ref::<CodeFault>();
                Fault::Code(own.expect("the code's own faults are `CodeFault`s").clone())
            }
            error => Fault::Protocol(error),
        }
    };
    faults.into_iter().map(fault).collect()
}

/// A logical batch goes down as one physical batch, its gadgets in order:
/// logical block `b` is qubits `3b` to `3b + 2`, and each circuit readout
/// gets a fresh tag.
#[test]
fn each_logical_call_goes_down_as_its_gadget() {
    let (mut code, log) = recorded(repetition3());
    let batch = [
        logical("prepare_z", [Operand::Block(1)]),
        logical("__quantum__qis__x__body", [Operand::Block(1)]),
        logical(
            "__quantum__qis__m__body",
            [Operand::Block(0), Operand::Tag(5)],
        ),
    ];
    run(&mut code, &batch);

    let on = |mnemonic, qubit| physical(mnemonic, [Operand::Block(qubit)]);
    let m = |qubit, tag| physical("measure", [Operand::Block(qubit), Operand::Tag(tag)]);
    let sent = vec![
        on("reset", 3),
        on("reset", 4),
        on("reset", 5),
        on("x", 3),
        on("x", 4),
        on("x", 5),
        m(0, 0),
        m(1, 1),
        m(2, 2),
    ];
    assert_eq!(batches(&log), vec![sent]);
}

/// The code needs three qubits per logical block: 5 qubits hold one block,
/// 6 hold two, and 2 are refused when the chain is composed.
#[test]
fn the_code_offers_a_block_per_three_qubits() {
    let blocks = |qubits| composed(qubits).capabilities().qubits;
    assert_eq!(blocks(5), Some(1));
    assert_eq!(blocks(6), Some(2));
    assert_eq!(
        faults(code_upstream_of(repetition3(), device(2))),
        [Fault::Code(CodeFault::TooFewQubits {
            needed: 3,
            offered: Some(2)
        })]
    );
}

/// The code offers a block per three qubits, so it cannot place them on a
/// device that does not say how many qubits it has: refused when the chain
/// is composed.
#[test]
fn a_device_that_does_not_say_how_many_qubits_it_has_is_refused_when_built() {
    assert_eq!(
        faults(code_upstream_of(repetition3(), device(8).uncounted())),
        [Fault::Code(CodeFault::TooFewQubits {
            needed: 3,
            offered: None
        })]
    );
}

/// The fixture's code with the logical `m` read as the parity of its three
/// circuit readouts.
fn m_as_parity_of_three() -> Code {
    let mut code = repetition3();
    let m = code.gadgets.get_mut("__quantum__qis__m__body");
    m.expect("the fixture has `m`").readouts = vec![vec![0, 1, 2]];
    code
}

fn m_on_block_0() -> [InstructionCall; 1] {
    [logical(
        "__quantum__qis__m__body",
        [Operand::Block(0), Operand::Tag(0)],
    )]
}

/// A circuit readout with no value (a lost qubit) leaves the logical
/// readout it is a bit of with none either, rather than a guess, whatever
/// the other bits.
#[test]
fn a_circuit_readout_with_no_value_leaves_its_logical_readout_without_one() {
    let lost = Value::Missing(ReadoutFailure::Loss);
    let composed = code_upstream_of(
        m_as_parity_of_three(),
        answers([lost, Value::One, Value::Zero]),
    );
    let mut composed = composed.expect("the parity reads circuit readouts `m` makes");
    assert_eq!(run(&mut composed, &m_on_block_0())[0].value, lost);
}

/// When the downstream reply ends before every circuit readout of a
/// logical readout came up, the logical reply fails, rather than leave the
/// caller without the outcome it asked for.
#[test]
fn a_logical_readout_missing_a_circuit_readout_fails_the_reply() {
    let mut downstream = answers([]);
    downstream.outcomes = vec![Outcome::new(1, Value::Zero)];
    let composed = code_upstream_of(repetition3(), downstream);
    let mut composed = composed.expect("the fixture's code");
    let reply = composed.call(on_shot_0(&m_on_block_0()));
    let mut outcomes = Vec::new();
    let ended = wait(
        reply.expect("the batch is of the logical set"),
        &mut outcomes,
    );
    assert!(ended.is_err(), "{outcomes:?}");
}

/// A Hadamard, which the device lacks.
fn hadamard() -> Action {
    Action::new([1], [1]).clifford([(Pauli::x(0), Pauli::z(0)), (Pauli::z(0), Pauli::x(0))])
}

/// A code with a fault of each kind, upstream of a device too small for
/// it: every fault is reported when the chain is composed, at once, what
/// the gadgets call that the device lacks first, as `compose` checks it,
/// then the code's own, in the order `Encoding::build` states.
#[test]
fn every_build_fault_is_reported_at_once() {
    let mut code = repetition3();
    code.sends = code.sends.with_action("H", [Block], hadamard());
    code.gadgets.remove("__quantum__qis__x__body");
    assert_eq!(
        faults(code_upstream_of(code, device(2))),
        [
            Fault::Protocol(BuildError::Instructions(vec![
                InstructionMismatch::Missing {
                    mnemonic: "H".into()
                }
            ])),
            Fault::Code(CodeFault::NoGadget("__quantum__qis__x__body".into())),
            Fault::Code(CodeFault::TooFewQubits {
                needed: 3,
                offered: Some(2)
            }),
        ]
    );
}

/// A code whose gadgets call an instruction the device lacks is refused
/// when the chain is composed, the instruction named: here a gadget of the
/// logical `x` that calls a Hadamard, upstream of a device with none.
#[test]
fn a_code_sending_what_the_device_lacks_is_refused_when_built() {
    let mut code = repetition3();
    code.sends = code.sends.with_action("H", [Block], hadamard());
    let x = code
        .gadgets
        .get_mut("__quantum__qis__x__body")
        .expect("of the fixture");
    x.calls.push(GadgetCall {
        mnemonic: "H".into(),
        operands: vec![Slot::Qubit(0)],
    });
    let missing = InstructionMismatch::Missing {
        mnemonic: "H".into(),
    };
    assert_eq!(
        faults(code_upstream_of(code, device(8))),
        [Fault::Protocol(BuildError::Instructions(vec![missing]))]
    );
}

/// A code that lacks a gadget, upstream of a device large enough for it, is
/// refused when the chain is composed, the instruction named: a code with
/// a fault is reported, whatever is downstream, and never resolved.
#[test]
fn a_logical_instruction_without_a_gadget_is_refused_when_built() {
    let mut code = repetition3();
    code.gadgets.remove("__quantum__qis__x__body");
    assert_eq!(
        faults(code_upstream_of(code, device(8))),
        [Fault::Code(CodeFault::NoGadget(
            "__quantum__qis__x__body".into()
        ))]
    );
}

/// A code whose gadgets call an instruction it declares without an action
/// is refused when the chain is composed, the instruction named, though
/// the device has its mnemonic with the same operands: here a loader that
/// read the device's `reset` without its `action`.
#[test]
fn an_instruction_the_code_sends_without_an_action_is_refused_when_built() {
    let mut code = repetition3();
    code.sends = code.sends.with("reset", [Block]);
    let undeclared = InstructionMismatch::UndeclaredAction {
        mnemonic: "reset".into(),
    };
    assert_eq!(
        faults(code_upstream_of(code, device(8))),
        [Fault::Protocol(BuildError::Instructions(vec![undeclared]))]
    );
}

/// The code's shots describe the logical instruction set, not the
/// device's: what a caller upstream of them may send.
#[test]
fn the_code_describes_the_logical_set() {
    let described = composed(8).capabilities().instruction_set;
    assert_eq!(described, Some(repetition3().accepts));
}

/// A program that prepares block 0, flips it and reads it, with calls built
/// by name in the set it states as its needs.
struct PrepareFlipAndRead(Arc<InstructionSet>);

impl AdaptiveProgram for PrepareFlipAndRead {
    type Instruction = InstructionCall;
    type Output = Vec<Value>;

    fn needs(&self) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::clone(&self.0)))
    }

    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<Value> {
        let batch = [
            self.0.call("prepare_z", [Operand::Block(0)]),
            self.0.call("__quantum__qis__x__body", [Operand::Block(0)]),
            self.0.call(
                "__quantum__qis__m__body",
                [Operand::Block(0), Operand::Tag(0)],
            ),
        ]
        .map(|call| call.expect("a call of the program's set"));
        values(executor, shots, &batch)
    }
}

/// A program that states the instructions it will send is taken by a chain
/// of the code when the code has them all, and is refused when given to
/// it, before any shot is made, when it lacks one.
#[test]
fn a_program_is_taken_only_by_a_code_that_has_the_instructions_it_needs() {
    let composes = |program: InstructionSet| {
        let program = PrepareFlipAndRead(Arc::new(program));
        let chain = compose(driver(), composed(8)).expect("nothing between to refuse");
        chain.run(program, ShotSet::one(0))
    };
    let program = InstructionSet::new("program")
        .with_action("prepare_z", [Block], reset())
        .with_action("__quantum__qis__x__body", [Block], pauli(Pauli::x(0)))
        .with_action("__quantum__qis__m__body", [Block, Tag], measure());
    assert!(composes(program.clone()).is_ok());
    let Err(JobFaults(faults)) = composes(program.with_action("h", [Block], hadamard())) else {
        panic!("given to a code without `h`")
    };
    let [Rejected::Unmet(unmet)] = faults.as_slice() else {
        panic!("{faults:?}")
    };
    assert_eq!(unmet.need, Need::InstructionSet);
    assert_eq!(
        unmet.mismatches,
        [InstructionMismatch::Missing {
            mnemonic: "h".into()
        }]
    );
}

/// A program that prepares block 0 and reads it, in the set it states,
/// whose `prepare_z` is a reset; it states that it resets.
struct PrepareAndRead(Arc<InstructionSet>);

impl PrepareAndRead {
    fn new() -> Self {
        let set = InstructionSet::new("program")
            .with_action("prepare_z", [Block], reset())
            .with_action("__quantum__qis__m__body", [Block, Tag], measure());
        Self(Arc::new(set))
    }
}

impl AdaptiveProgram for PrepareAndRead {
    type Instruction = InstructionCall;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        let needs = Needs::default().with_resets(true);
        needs.with_instruction_set(Some(Arc::clone(&self.0)))
    }

    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let call = |mnemonic, operands: Vec<Operand>| {
            (self.0.call(mnemonic, operands)).expect("a call of the program's set")
        };
        let batch = [
            call("prepare_z", vec![Operand::Block(0)]),
            call(
                "__quantum__qis__m__body",
                vec![Operand::Block(0), Operand::Tag(0)],
            ),
        ];
        let reply = executor.call(InstructionBatchRef {
            shots,
            instructions: &batch,
        });
        let mut outcomes = Vec::new();
        wait(
            reply.expect("the chain accepts the program's set"),
            &mut outcomes,
        )
        .expect("the device never fails a batch");
        outcomes
    }
}

/// The fixture's code with a logical set of `m` and `x` only, and their
/// gadgets: a code with no reset.
fn without_a_reset() -> Code {
    let mut code = repetition3();
    code.accepts = Arc::new(
        InstructionSet::new("no reset")
            .with_action("__quantum__qis__m__body", [Block, Tag], measure())
            .with_action("__quantum__qis__x__body", [Block], pauli(Pauli::x(0))),
    );
    let accepts = Arc::clone(&code.accepts);
    code.gadgets
        .retain(|mnemonic, _| accepts.opcode(mnemonic).is_some());
    code
}

/// A program that resets, given to a chain of a code whose logical set has
/// no reset (only `x` and `m`), on a device that resets, is refused, its
/// reset named as an instruction the code lacks, and for nothing else: the
/// code passes the device's resets on, and where sets are described the
/// instructions matched are what guarantee a reset.
#[test]
fn a_program_resetting_upstream_of_a_code_without_a_reset_is_refused_when_given_to_the_chain() {
    let run = |code| {
        let chain = compose(driver(), Encoding::new(code));
        let chain = compose(chain, device(8)).expect("the code runs on the device");
        chain.run(PrepareAndRead::new(), ShotSet::one(0))
    };
    (run(repetition3()).expect("the fixture's code has `prepare_z`")).wait();
    let offered = code_upstream_of(without_a_reset(), device(8))
        .expect("the code without a reset runs on the device")
        .capabilities();
    assert!(offered.resets, "the code passes the device's resets on");

    let Err(JobFaults(faults)) = run(without_a_reset()) else {
        panic!("given to a code without a reset")
    };
    let [Rejected::Unmet(unmet)] = faults.as_slice() else {
        panic!("{faults:?}")
    };
    assert_eq!(unmet.need, Need::InstructionSet);
    assert_eq!(
        unmet.mismatches,
        [InstructionMismatch::Missing {
            mnemonic: "prepare_z".into()
        }]
    );
}

/// A program whose set declares the logical instructions in another order
/// runs on a chain of the code, and each of its batches is copied into the
/// code's numbering: its `prepare_z`, `x` and `m` run as such, though at
/// the opcodes of the code's `m`, `prepare_z` and `x`.
#[test]
fn a_program_in_another_order_runs_its_own_instructions_on_the_code() {
    let program_set = InstructionSet::new("program")
        .with_action("__quantum__qis__x__body", [Block], pauli(Pauli::x(0)))
        .with_action("__quantum__qis__m__body", [Block, Tag], measure())
        .with_action("prepare_z", [Block], reset());
    let program = PrepareFlipAndRead(Arc::new(program_set));
    let run = compose(driver(), composed(8))
        .expect("nothing between to refuse")
        .run(program, ShotSet::one(0))
        .expect("the code has the program's set");
    let (output, _) = run.wait();
    assert_eq!(output, [Value::One]);
}

/// A program that cannot afford a copy of each batch builds its calls by
/// name with the set the code describes, which it states as its needs:
/// they are in the code's numbering, and run as the program means them.
#[test]
fn a_program_builds_its_calls_by_name_with_the_codes_set() {
    let code = composed(8);
    let set = code
        .capabilities()
        .instruction_set
        .expect("the code describes the logical set");
    let run = compose(driver(), code)
        .expect("nothing between to refuse")
        .run(PrepareFlipAndRead(set), ShotSet::one(0))
        .expect("the code accepts its own set");
    let (output, _) = run.wait();
    assert_eq!(output, [Value::One]);
}

/// A program in its own numbering that runs the conformance kit's check
/// of a described set on the executor it drives.
struct CallsOutside(Arc<InstructionSet>);

impl AdaptiveProgram for CallsOutside {
    type Instruction = InstructionCall;
    type Output = ();

    fn needs(&self) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::clone(&self.0)))
    }

    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) {
        let x = self.0.call("__quantum__qis__x__body", [Operand::Block(0)]);
        let &shot = shots.iter().next().expect("a run has a shot");
        conformance::refuses_calls_outside_its_set(
            executor,
            &ShotSet::one(shot),
            &self.0,
            &x.expect("`x` takes a block"),
        );
    }
}

/// The code, driven in a program's numbering, refuses a call at an opcode
/// the program's set lacks, though the logical set has an instruction there
/// that takes the same operands (its `x`, at the opcode after the program's
/// only one): a batch is checked against the program's set before it is
/// copied into the code's numbering.
#[test]
fn the_code_in_a_programs_numbering_refuses_calls_outside_the_programs_set() {
    let program_set = InstructionSet::new("program");
    let program_set =
        program_set.with_action("__quantum__qis__x__body", [Block], pauli(Pauli::x(0)));
    let program = CallsOutside(Arc::new(program_set));
    compose(driver(), composed(8))
        .expect("nothing between to refuse")
        .run(program, ShotSet::one(0))
        .expect("the code has the program's instructions")
        .wait();
}

/// A logical call outside the code's set is refused before anything goes
/// down, at its index in the logical batch.
#[test]
fn a_call_outside_the_logical_set_is_refused_before_anything_goes_down() {
    let (mut code, log) = recorded(repetition3());
    let with_h = repetition3().accepts.as_ref().clone();
    let with_h = with_h.with_action("h", [Block], hadamard());
    let h = with_h
        .call("h", [Operand::Block(0)])
        .expect("`h` takes a block");
    let x = logical("__quantum__qis__x__body", [Operand::Block(0)]);
    let refused = code.call(on_shot_0(&[x, h])).err();
    assert!(matches!(refused, Some(Rejected::Instruction { at: 1, .. })));
    assert!(batches(&log).is_empty(), "nothing went down");
}

/// A logical call on a block the code does not have is refused before
/// anything goes down, at its index in the logical batch: on 8 qubits,
/// the code has blocks 0 and 1.
#[test]
fn a_call_on_a_block_the_code_lacks_is_refused_before_anything_goes_down() {
    let (mut code, log) = recorded(repetition3());
    let x = |block| logical("__quantum__qis__x__body", [Operand::Block(block)]);
    let refused = code.call(on_shot_0(&[x(1), x(2)])).err();
    assert!(matches!(refused, Some(Rejected::Instruction { at: 1, .. })));
    assert!(batches(&log).is_empty(), "nothing went down");
}

/// A physical call the device refuses is reported at the logical call whose
/// gadget holds it: a device that describes 11 qubits, though it has 8,
/// refuses block 2's third qubit, the ninth physical call, which the code
/// maps back to the third logical call.
#[test]
fn a_downstream_refusal_names_the_logical_call() {
    let mut code = composed(11);
    let x = |block| logical("__quantum__qis__x__body", [Operand::Block(block)]);
    let refused = code.call(on_shot_0(&[x(0), x(1), x(2)])).err();
    assert!(matches!(refused, Some(Rejected::Instruction { at: 2, .. })));
}

/// The device and the code each refuse a call outside the set they
/// describe, at its index: the conformance kit's check of a described set.
#[test]
fn the_device_and_the_code_refuse_calls_outside_their_sets() {
    let x = physical("x", [Operand::Block(0)]);
    let shot_0 = ShotSet::one(0);
    conformance::refuses_calls_outside_its_set(&mut device(8), &shot_0, &device_set(), &x);
    let x = logical("__quantum__qis__x__body", [Operand::Block(0)]);
    let accepts = repetition3().accepts;
    conformance::refuses_calls_outside_its_set(&mut composed(8), &shot_0, &accepts, &x);
}

/// The device and the code each refuse a batch that repeats a readout's
/// tag, at the instruction that repeats it, and accept a tag used again in
/// a later batch: the conformance kit's checks of readout tags. The code
/// checks its logical tags itself, since it sends fresh tags down.
#[test]
fn the_device_and_the_code_refuse_a_tag_repeated_within_a_batch() {
    let read = |qubit, tag| physical("measure", [Operand::Block(qubit), Operand::Tag(tag)]);
    let shot_0 = ShotSet::one(0);
    let mut toy = device(8);
    let (a, b, a_again) = (read(0, 0), read(1, 1), read(2, 0));
    conformance::refuses_a_tag_repeated_within_a_batch(&mut toy, &shot_0, &a, &b, &a_again);
    let x = physical("x", [Operand::Block(1)]);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut toy, &shot_0, &a, &x, &a_again,
    );
    let read = |block, tag| {
        logical(
            "__quantum__qis__m__body",
            [Operand::Block(block), Operand::Tag(tag)],
        )
    };
    let (code, a, b, a_again) = (&mut composed(8), &read(0, 0), &read(1, 1), &read(1, 0));
    conformance::refuses_a_tag_repeated_within_a_batch(code, &shot_0, a, b, a_again);
    let x = logical("__quantum__qis__x__body", [Operand::Block(1)]);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        code, &shot_0, a, &x, a_again,
    );
}

/// Closing the code closes the device it wraps, and the device's `Closed`
/// comes back up unchanged.
#[test]
fn closing_the_code_closes_what_it_wraps() {
    let x = logical("__quantum__qis__x__body", [Operand::Block(0)]);
    conformance::rejects_calls_once_closed::<InstructionBatch<InstructionCall>, _>(
        &mut composed(8),
        on_shot_0(&[x]),
    );
}

/// The code draws nothing: each shot's log probability is the one the
/// executor downstream states (here ln ½), passed on in its report.
#[test]
fn the_code_passes_on_the_log_probability_of_the_executor_downstream() {
    let composed = code_upstream_of(repetition3(), answers([])).expect("as the device of 8 qubits");
    let mut code = composed;
    run(&mut code, &[]);
    let report = code.close();
    assert_eq!(report.log_p.get(0), Some(0.5_f64.ln()));
}

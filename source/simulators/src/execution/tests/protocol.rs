// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The protocol's laws at L3, on the Runtime adapter and the legacy
//! simulators as terminals: a chain of adapters behaves as one (S7), and a
//! rejection starts no work (S9).

use executor_protocol::{
    Batch, Capabilities, Close, Completion, Describe, Executor, Fold, Message, Outcome, Rejected,
    RetryClass, Status, wait,
};

use super::{
    TestDoubleTerminal, adaptive_program, loss_or_branch_program, measure_then_branch_program,
    x_reset_measure_program,
};
use crate::{
    MeasurementResult, Simulator,
    bytecode::{Block, Instruction},
    cpu_full_state_simulator::FullStateSimulator,
    execution::{
        MeasurementKind, MeasurementRequest, PreparedAdaptiveProgram, QuantumEvolutionRegion,
        RegionId, RegionInstruction, RuntimeAdapter, RuntimeError, SimulatorReport,
        SimulatorTerminal, UnitaryOperation,
    },
    stabilizer_simulator::StabilizerSimulator,
};

/// An adapter that changes nothing and counts the calls it passes on, at any
/// level: what a chain of adapters looks like to the executors around it.
struct TestDoubleProbe<E> {
    inner: E,
    calls: u64,
}

impl<E> TestDoubleProbe<E> {
    fn new(inner: E) -> Self {
        Self { inner, calls: 0 }
    }
}

impl<M: Message, E: Executor<M>> Executor<M> for TestDoubleProbe<E> {
    type Response = E::Response;

    fn call(&mut self, message: M::Ref<'_>) -> Result<Self::Response, Rejected> {
        self.calls += 1;
        self.inner.call(message)
    }
}

impl<E: Describe> Describe for TestDoubleProbe<E> {
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
}

#[derive(Debug, Default, PartialEq)]
struct ProbeReport<R> {
    calls: u64,
    inner: R,
}

impl<R: Fold> Fold for ProbeReport<R> {
    fn fold(&mut self, other: Self) {
        self.calls += other.calls;
        self.inner.fold(other.inner);
    }
}

impl<E: Close> Close for TestDoubleProbe<E> {
    type Report = ProbeReport<E::Report>;

    fn close(&mut self) -> Self::Report {
        ProbeReport {
            calls: std::mem::take(&mut self.calls),
            inner: self.inner.close(),
        }
    }
}

/// What a driver sees of one call: the rejection, or the reply's events and
/// how it ended.
type Seen = Result<(Vec<Outcome>, Status), Rejected>;

fn see<E: Executor<Batch<PreparedAdaptiveProgram<u64>>>>(
    executor: &mut E,
    programs: &[PreparedAdaptiveProgram<u64>],
) -> Seen {
    let reply = executor.call(programs)?;
    let mut events = Vec::new();
    let status = match wait(reply, &mut events) {
        Ok(()) => Status::Done,
        Err(reason) => Status::Failed(reason),
    };
    Ok((events, status))
}

fn programs() -> Vec<PreparedAdaptiveProgram<u64>> {
    [
        measure_then_branch_program(),
        loss_or_branch_program(false),
        loss_or_branch_program(true),
        x_reset_measure_program(),
    ]
    .into_iter()
    .map(|program| PreparedAdaptiveProgram::new(program).expect("test programs should prepare"))
    .collect()
}

/// Law 1 at L3: probes around and below the Runtime adapter change nothing a
/// driver sees: the outcomes, how each call ends, the rejections, the
/// capabilities, and the state left in the simulator.
#[test]
fn a_chain_of_adapters_is_seen_as_the_runtime_alone() {
    fn assert_law<S: Simulator>(qubits: usize) {
        let programs = programs();
        for seed in 0..32 {
            let mut bare = RuntimeAdapter::new(SimulatorTerminal::<S>::new(
                qubits,
                2,
                seed,
                Default::default(),
            ));
            let mut chain = TestDoubleProbe::new(RuntimeAdapter::new(TestDoubleProbe::new(
                SimulatorTerminal::<S>::new(qubits, 2, seed, Default::default()),
            )));
            assert_eq!(chain.capabilities(), bare.capabilities());
            // One program per call, then all of them in one call: the
            // state carries over between calls and between the programs of
            // a call.
            for program in &programs {
                let program = std::slice::from_ref(program);
                assert_eq!(see(&mut chain, program), see(&mut bare, program));
            }
            assert_eq!(see(&mut chain, &programs), see(&mut bare, &programs));
            assert_eq!(
                chain.inner.inner().inner.simulator().measurements(),
                bare.inner().simulator().measurements()
            );
            let report = chain.close();
            let bare_report = bare.close();
            assert_eq!(report.calls, 5);
            assert_eq!(report.inner.programs, bare_report.programs);
            assert_eq!(report.inner.batches, bare_report.batches);
            assert_eq!(report.inner.inner.inner, bare_report.inner);
            assert_eq!(see(&mut chain, &programs), see(&mut bare, &programs));
        }
    }

    assert_law::<FullStateSimulator>(2);
    assert_law::<StabilizerSimulator>(2);
    // Too few qubits: rejections, and replies that fail once the walk
    // reaches qubit 1, alike.
    assert_law::<FullStateSimulator>(1);
}

fn region(qubit: usize) -> RegionInstruction {
    RegionInstruction::ExecuteRegion {
        region_id: RegionId::new(0),
        region: QuantumEvolutionRegion::new([UnitaryOperation::X { target: qubit }]),
    }
}

fn measure(qubit: usize, result_id: usize) -> RegionInstruction {
    RegionInstruction::Measure(MeasurementRequest {
        kind: MeasurementKind::MeasureZ,
        qubit,
        result_id,
    })
}

/// S9: a batch with an instruction beyond the terminal's qubits or results is
/// rejected, naming it, and nothing of the batch runs: the X before it does
/// not apply.
#[test]
fn a_terminal_rejects_a_batch_beyond_its_qubits_or_results_before_running_it() {
    let mut terminal = SimulatorTerminal::<FullStateSimulator>::new(2, 1, 7, Default::default());
    let beyond = [
        (vec![region(0), region(2)], 1),
        (vec![region(0), measure(0, 1)], 1),
        (vec![region(0), measure(2, 0)], 1),
        (vec![region(0), RegionInstruction::Reset { qubit: 5 }], 1),
        (vec![region(5), measure(0, 0)], 0),
    ];
    for (batch, at) in beyond {
        match terminal.call(&batch) {
            Err(Rejected::Instruction { at: rejected, .. }) => assert_eq!(rejected, at),
            other => panic!("expected a rejection of instruction {at}, got {other:?}"),
        }
    }
    let mut outcomes = Vec::new();
    let reply = terminal
        .call(&[measure(0, 0)])
        .expect("a batch in range runs");
    assert_eq!(wait(reply, &mut outcomes), Ok(()));
    assert_eq!(
        terminal.simulator().measurements(),
        [MeasurementResult::Zero]
    );
    assert_eq!(
        terminal.close(),
        SimulatorReport {
            batches: 1,
            measurements: 1,
            ..SimulatorReport::default()
        }
    );
    assert_eq!(terminal.call(&[]).err(), Some(Rejected::Closed));
}

/// S9 through the Runtime adapter: a rejection before the terminal ran
/// anything of the call passes through, `Instruction` naming the program; a
/// later one fails the reply instead, since work had started.
#[test]
fn the_runtime_rejects_only_before_any_work() {
    let programs = programs();
    let first = std::slice::from_ref(&programs[0]);

    let mut too_small = RuntimeAdapter::new(SimulatorTerminal::<FullStateSimulator>::new(
        2,
        0,
        1,
        Default::default(),
    ));
    match too_small.call(first) {
        Err(Rejected::Instruction { at: 0, reason }) => {
            assert!(
                reason.starts_with("region instruction 1: result 0"),
                "{reason}"
            );
        }
        other => panic!("expected a rejection of program 0, got {other:?}"),
    }

    let busy = Rejected::Unavailable("busy".into());
    let mut runtime =
        RuntimeAdapter::new(TestDoubleTerminal::new(None).rejecting_from(0, busy.clone()));
    let rejected = runtime.call(first).err();
    assert_eq!(rejected, Some(busy.clone()));
    assert_eq!(rejected.map(|r| r.retry()), Some(RetryClass::Later));

    let mut runtime =
        RuntimeAdapter::new(TestDoubleTerminal::new(None).rejecting_from(1, busy.clone()));
    let mut reply = runtime.call(first).expect("the first batch ran");
    let mut events = Vec::new();
    match reply.poll_events(&mut events, std::task::Waker::noop()) {
        Status::Failed(reason) => assert_eq!(
            reason,
            format!("program 0: {}", RuntimeError::Rejected(busy))
        ),
        status => panic!("expected the reply to fail, got {status:?}"),
    }
    assert!(events.is_empty());

    runtime.close();
    assert_eq!(runtime.call(first).err(), Some(Rejected::Closed));
    assert_eq!(
        runtime.run(&programs[0], &mut Vec::new()),
        Err(RuntimeError::Rejected(Rejected::Closed))
    );
}

/// A rejected call counts none of its programs in the report, not even those
/// that ran to their end before it without sending a batch.
#[test]
fn a_rejected_call_counts_no_programs() {
    let classical = PreparedAdaptiveProgram::new(adaptive_program(
        vec![Instruction {
            opcode: 0x02,
            ..Instruction::default()
        }],
        vec![Block {
            instr_offset: 0,
            instr_count: 1,
        }],
        Vec::new(),
    ))
    .expect("a lone return should prepare");
    let quantum = PreparedAdaptiveProgram::new(measure_then_branch_program())
        .expect("test programs should prepare");
    let busy = Rejected::Unavailable("busy".into());
    let mut runtime =
        RuntimeAdapter::new(TestDoubleTerminal::new(None).rejecting_from(0, busy.clone()));
    assert_eq!(runtime.call(&[classical, quantum]).err(), Some(busy));
    let report = runtime.close();
    assert_eq!((report.programs, report.batches), (0, 0));
}

/// The records of a call are its outcomes, tagged by their index across the
/// programs of the call.
#[test]
fn the_runtime_tags_records_across_the_programs_of_a_call() {
    let programs = programs();
    let mut runtime = RuntimeAdapter::new(
        TestDoubleTerminal::new(None).with_measurement_result(MeasurementResult::One),
    );
    let (events, status) = see(&mut runtime, &programs).expect("the test double accepts");
    assert_eq!(status, Status::Done);
    let tags = events.iter().map(|event| event.tag).collect::<Vec<_>>();
    let expected = (0..u32::try_from(tags.len()).expect("few records")).collect::<Vec<_>>();
    assert_eq!(tags, expected);
    assert!(!tags.is_empty());
    assert_eq!(runtime.close().programs, 4);
}

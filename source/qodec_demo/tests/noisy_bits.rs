//! The simulator of repetition3's chain: five bits with bit-flip noise, as
//! the protocol's executor and as the Qodec runtime's bottom backend.

mod common;

use std::sync::Arc;

use executor_protocol::{
    Close, Executor, InstructionBatchRef, InstructionCall, InstructionSet, Operand, OperandKind,
    Rejected, ShotId, ShotOutcome, ShotSet, Value, shot_seed, wait,
};
use qodec_demo::boundary::{Boundary, Ins, outcomes};
use qodec_demo::noisy_bits::{M, NoisyBits, NoisyBitsBackend, simulator};
use qodec_runtime::{Backend, OwnedParam, Readout};

use common::{DRAWS_FLIP, SEED, qodec_stim_rz, stim_rz};

/// The call of `mnemonic` on `operands`, of `set`.
fn call(set: &Boundary, mnemonic: &str, operands: &[Operand]) -> InstructionCall {
    set.set
        .call(mnemonic, operands.iter().copied())
        .expect("a call of stim+rz")
}

/// How a batch ended: refused at the call (`Err`), or accepted, its reply
/// then done (`Ok(Ok(()))`) or failed with its reason (`Ok(Err(_))`).
type Ended = Result<Result<(), String>, Rejected>;

/// `calls` on [`simulator`] at flip 0.5, on shot 0: how the batch ended,
/// and the readouts handed over.
fn on_shot_0(set: &Boundary, calls: &[InstructionCall]) -> (Ended, Vec<ShotOutcome>) {
    let mut executor = simulator(Arc::clone(&set.set), SEED, 0.5);
    let shots = ShotSet::new(vec![0]).expect("one shot");
    let batch = InstructionBatchRef {
        shots: &shots,
        instructions: calls,
    };
    let mut read = Vec::new();
    let ended = executor.call(batch).map(|reply| wait(reply, &mut read));
    (ended, read)
}

/// One round of repetition3 written out: prepare the three data qubits,
/// rotate, copy their parities into the ancillas 3 and 4 (`idle`'s
/// gadget), and read all five, under tags 0 to 4 (the ancillas first).
fn syndrome_round(set: &Boundary) -> Vec<InstructionCall> {
    use Operand::{Block, Real, Tag};
    let call = |m: &str, operands: &[Operand]| call(set, m, operands);
    let mut calls: Vec<_> = (0..5).map(|q| call("R", &[Block(q)])).collect();
    calls.push(call("rotate_z", &[Block(0), Real(0.7)]));
    for (c, t) in [(0, 3), (1, 3), (1, 4), (2, 4)] {
        calls.push(call("CX", &[Block(c), Block(t)]));
    }
    for (tag, q) in [3, 4, 0, 1, 2].into_iter().enumerate() {
        let tag = u32::try_from(tag).expect("small");
        calls.push(call("M", &[Block(q), Tag(tag)]));
    }
    calls
}

/// `calls` on [`simulator`] over `shots`: the outcomes, and each shot's
/// log probability.
fn on_the_simulator(
    set: &Boundary,
    calls: &[InstructionCall],
    shots: &[ShotId],
    flip: f64,
) -> (Vec<ShotOutcome>, Vec<f64>) {
    let mut executor = simulator(Arc::clone(&set.set), SEED, flip);
    let shots = ShotSet::new(shots.to_vec()).expect("distinct shots");
    let reply = executor
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: calls,
        })
        .expect("the simulator accepts stim+rz");
    let mut read = Vec::new();
    wait(reply, &mut read).expect("the shots run");
    read.sort_by_key(|o| (o.shot, o.tag));
    let report = executor.close();
    let log_p = shots
        .iter()
        .map(|&s| report.log_p.get(s).expect("the bits know it"))
        .collect();
    (read, log_p)
}

/// At flip 1 the noise is certain: a reset reads 1, and a second reset of
/// the same bit reads 1 again, as it clears the bit before the noise.
#[test]
fn a_reset_clears_its_bit_before_the_noise() {
    let mut bits = NoisyBits::new(SEED, 1.0);
    bits.reset(0).expect("a bit");
    assert!(bits.read(0).expect("a bit"), "flipped from 0");
    bits.reset(0).expect("a bit");
    assert!(bits.read(0).expect("a bit"), "cleared, then flipped");
}

/// At flip 1: two bits reset to 1, then a CX adds the control to the
/// target (1 + 1 = 0) before both flip: the control reads 0, the target 1.
#[test]
fn a_cx_adds_its_control_to_its_target_before_the_noise() {
    let mut bits = NoisyBits::new(SEED, 1.0);
    bits.reset(1).expect("a bit");
    bits.reset(4).expect("a bit");
    bits.cx(1, 4).expect("two bits");
    assert!(!bits.read(1).expect("a bit"), "the control, flipped");
    assert!(
        bits.read(4).expect("a bit"),
        "the target, added to, flipped"
    );
}

/// A CX draws for its control, then for its target: from one seed,
/// `cx(0, 1)` on bits at 0 flips bits 0 and 1 as `reset(2)` then
/// `reset(3)` flip bits 2 and 3, over seeds where one draw flips and the
/// other does not.
#[test]
fn a_cx_noises_its_control_first() {
    let read = |bits: &NoisyBits, q| bits.read(q).expect("a bit");
    let mut uneven = 0;
    for seed in 0..64 {
        let mut by_cx = NoisyBits::new(seed, 0.5);
        by_cx.cx(0, 1).expect("two bits");
        let mut by_resets = NoisyBits::new(seed, 0.5);
        by_resets.reset(2).expect("a bit");
        by_resets.reset(3).expect("a bit");
        let draws = (read(&by_resets, 2), read(&by_resets, 3));

        assert_eq!((read(&by_cx, 0), read(&by_cx, 1)), draws, "seed {seed}");
        uneven += usize::from(draws.0 != draws.1);
    }
    assert!(uneven > 0, "some seeds flip one bit of the two");
}

/// One reset, over many seeds at flip 0.3: the log probability is
/// `ln 0.3` where the bit reads 1, `ln 0.7` where it reads 0, and both
/// occur.
#[test]
fn the_log_probability_is_that_of_the_draws() {
    let mut seen = [false; 2];
    for seed in 0..64 {
        let mut bits = NoisyBits::new(seed, 0.3);
        bits.reset(2).expect("a bit");
        let one = bits.read(2).expect("a bit");
        let expected = if one { 0.3_f64.ln() } else { 0.7_f64.ln() };
        assert!((bits.log_p() - expected).abs() < 1e-12, "seed {seed}");
        seen[usize::from(one)] = true;
    }
    assert_eq!(seen, [true; 2], "some draws flip, some do not");
}

/// The bits are 0 to 4: a sixth is refused, by every operation.
#[test]
fn a_qubit_beyond_the_five_bits_is_refused() {
    let mut bits = NoisyBits::new(SEED, 0.5);
    assert!(bits.reset(5).is_err());
    assert!(bits.cx(0, 5).is_err());
    assert!(bits.cx(5, 0).is_err());
    assert!(bits.read(5).is_err());
    bits.reset(4).expect("the fifth bit");
}

/// `rotate_z` touches no bit, yet both forms fail it on a sixth as it
/// runs: the executor accepts the call, whose operands its set holds, and
/// its reply fails.
#[test]
fn both_forms_fail_a_rotation_beyond_the_five_bits() {
    let set = stim_rz();
    let rotate = call(&set, "rotate_z", &[Operand::Block(5), Operand::Real(0.7)]);

    let (ended, _) = on_shot_0(&set, std::slice::from_ref(&rotate));
    let ended = ended.expect("the executor accepts a call its set holds");
    let failed = ended.expect_err("the executor's rotate_z fails");
    assert!(
        failed.contains("qubit 5 is not one of the 5 bits"),
        "{failed}"
    );
    let (ins, _) = set.instruction(&rotate);
    let mut backend = NoisyBitsBackend::new(SEED, 0.5);
    assert!(
        backend.execute_batch(&[ins]).is_err(),
        "the backend's rotate_z"
    );
}

/// A batch reads qubit 0, then rotates a sixth: the executor accepts it,
/// and its reply fails after handing over the readout made before the
/// failing instruction.
#[test]
fn a_failed_batch_keeps_the_readouts_made_before_its_failure() {
    let set = stim_rz();
    let read = call(&set, "M", &[Operand::Block(0), Operand::Tag(0)]);
    let rotate = call(&set, "rotate_z", &[Operand::Block(5), Operand::Real(0.7)]);

    let (ended, read) = on_shot_0(&set, &[read, rotate]);
    let ended = ended.expect("precondition: the executor accepts calls its set holds");
    assert!(ended.is_err(), "a sixth bit fails the reply");
    let read: Vec<_> = read.iter().map(|o| (o.shot, o.tag)).collect();
    assert_eq!(read, [(0, 0)]);
}

/// Once a batch failed on a sixth bit, the executor refuses each later
/// call, an empty batch on another shot included, as following an earlier
/// failure.
#[test]
fn after_a_failed_batch_the_simulator_refuses_later_calls() {
    let set = stim_rz();
    let rotate = call(&set, "rotate_z", &[Operand::Block(5), Operand::Real(0.7)]);
    let mut executor = simulator(Arc::clone(&set.set), SEED, 0.5);
    let shot = |s| ShotSet::new(vec![s]).expect("one shot");
    let reply = executor
        .call(InstructionBatchRef {
            shots: &shot(0),
            instructions: &[rotate],
        })
        .expect("precondition: the executor accepts a call its set holds");
    wait(reply, &mut Vec::new()).expect_err("precondition: a sixth bit fails the reply");

    let later = executor.call(InstructionBatchRef {
        shots: &shot(1),
        instructions: &[],
    });
    assert!(
        matches!(later, Err(Rejected::EarlierFailure(_))),
        "{:?}",
        later.err()
    );
}

/// A caller sends the simulator a batch that resets qubit 0, then calls
/// `M` on a block with no tag, operands `M` does not take: the batch is
/// refused at its call, naming its second call, rather than run and failed.
#[test]
fn the_simulator_refuses_a_call_outside_its_set() {
    let set = stim_rz();
    let reset = call(&set, "R", &[Operand::Block(0)]);
    let untagged = InstructionCall::new(M, [Operand::Block(0)]);

    let (ended, _) = on_shot_0(&set, &[reset, untagged]);
    assert!(
        matches!(ended, Err(Rejected::Instruction { at: 1, .. })),
        "{ended:?}"
    );
}

/// A flip is a chance: above 1, it is refused when the bits are made,
/// before any log probability could exceed 0.
#[test]
#[should_panic(expected = "the flip chance 1.5 is not in [0, 1]")]
fn a_flip_above_one_panics() {
    let _ = NoisyBits::new(SEED, 1.5);
}

/// A flip below 0 is refused when the bits are made.
#[test]
#[should_panic(expected = "the flip chance -0.1 is not in [0, 1]")]
fn a_flip_below_zero_panics() {
    let _ = NoisyBits::new(SEED, -0.1);
}

/// A flip that is not a number is refused when the bits are made.
#[test]
#[should_panic(expected = "the flip chance NaN is not in [0, 1]")]
fn a_flip_that_is_not_a_number_panics() {
    let _ = NoisyBits::new(SEED, f64::NAN);
}

/// The simulator checks its flip when it is made, before any shot runs.
#[test]
#[should_panic(expected = "the flip chance -0.1 is not in [0, 1]")]
fn the_simulator_refuses_a_flip_that_is_not_a_chance_when_made() {
    let set = stim_rz();
    let _ = simulator(Arc::clone(&set.set), SEED, -0.1);
}

/// The simulator runs `stim+rz` only: repetition3's logical set, which has
/// no `R`, is refused when the simulator is made.
#[test]
#[should_panic(expected = "stim+rz's `R`")]
fn the_simulator_refuses_a_set_without_stim_rz() {
    let qodec = common::repetition3();
    let logical = Boundary::new(&qodec.layers()[0].instruction_set).expect("the logical set");
    let _ = simulator(logical.set, SEED, 0.5);
}

/// The set of `instructions`, each with its operands, at opcodes in their
/// order.
fn set_of(instructions: &[(&str, &[OperandKind])]) -> Arc<InstructionSet> {
    let set = instructions
        .iter()
        .fold(InstructionSet::new("bits"), |set, (m, kinds)| {
            set.with(m, kinds.iter().copied())
        });
    Arc::new(set)
}

/// A set that holds `R`, `M` and `CX` at their opcodes but no `rotate_z`
/// is refused when the simulator is made: each of the four is checked.
#[test]
#[should_panic(expected = "stim+rz's `rotate_z`")]
fn the_simulator_refuses_a_set_without_rotate_z() {
    use OperandKind::{Block, Tag};
    let set = set_of(&[
        ("R", &[Block]),
        ("M", &[Block, Tag]),
        ("CX", &[Block, Block]),
    ]);
    let _ = simulator(set, SEED, 0.5);
}

/// A set whose `R` takes two blocks is refused when the simulator is made,
/// though `R` sits at its opcode: an instruction's operands are checked
/// too.
#[test]
#[should_panic(expected = "stim+rz's `R`")]
fn the_simulator_refuses_an_instruction_with_other_operands() {
    use OperandKind::{Block, Real, Tag};
    let set = set_of(&[
        ("R", &[Block, Block]),
        ("M", &[Block, Tag]),
        ("CX", &[Block, Block]),
        ("rotate_z", &[Block, Real]),
    ]);
    let _ = simulator(set, SEED, 0.5);
}

/// A set that holds the four instructions, each with its operands, but
/// with `R` and `M` at each other's opcodes is refused when the simulator
/// is made: the simulator reads a call by its opcode, so each instruction
/// is checked at its opcode, not by its mnemonic alone.
#[test]
#[should_panic(expected = "stim+rz's `R`")]
fn the_simulator_refuses_an_instruction_at_another_opcode() {
    use OperandKind::{Block, Real, Tag};
    let set = set_of(&[
        ("M", &[Block, Tag]),
        ("R", &[Block]),
        ("CX", &[Block, Block]),
        ("rotate_z", &[Block, Real]),
    ]);
    let _ = simulator(set, SEED, 0.5);
}

/// A set may hold more than `stim+rz`: here `H` follows the four. Both
/// forms fail its call as it runs: the executor accepts it, its set
/// holding `H`, and its reply fails; the backend returns the error.
#[test]
fn both_forms_fail_an_instruction_not_of_stim_rz() {
    let mut qodec_set = (*qodec_stim_rz()).clone();
    let mut h = qodec_set
        .instructions
        .iter()
        .find(|i| i.mnemonic == "R")
        .expect("stim+rz declares R")
        .clone();
    h.mnemonic = "H".into();
    qodec_set.instructions.push(h);
    let set = Boundary::new(&qodec_set).expect("the operands express stim+rz and H");
    let h = call(&set, "H", &[Operand::Block(0)]);

    let (ended, _) = on_shot_0(&set, std::slice::from_ref(&h));
    let ended = ended.expect("the executor accepts a call its set holds");
    let failed = ended.expect_err("the executor's H fails");
    assert!(failed.contains("not a call of stim+rz"), "{failed}");
    let (ins, _) = set.instruction(&h);
    let failed = NoisyBitsBackend::new(SEED, 0.5).execute_batch(&[ins]);
    let failed = failed.expect_err("the backend fails H");
    assert!(failed.contains("not an instruction of stim+rz"), "{failed}");
}

/// Qodec's `number` arguments may be integers: the backend runs a
/// `rotate_z` by the angle 1 given as an integer, as it runs one by 1.0.
#[test]
fn the_backend_runs_a_rotation_by_an_integer_angle() {
    let isa = qodec_stim_rz();
    let index = isa
        .instructions
        .iter()
        .position(|i| i.mnemonic == "rotate_z")
        .expect("stim+rz declares rotate_z");
    let rotation = |angle| Ins {
        instruction_id: index,
        parameters: vec![OwnedParam::Block(0), angle].into(),
    };
    let mut backend = NoisyBitsBackend::new(SEED, 0.5);
    backend
        .execute_batch(&[rotation(OwnedParam::Float(1.0))])
        .expect("precondition: the backend runs a rotation by a real");

    let ran = backend.execute_batch(&[rotation(OwnedParam::Int(1))]);
    assert!(ran.is_ok(), "{:?}", ran.err());
}

/// A refused CX changes nothing: right after it, and after the CX that
/// follows, the bits and their log probability are those of bits that
/// never saw it, so it neither flipped a bit nor took a draw.
#[test]
fn a_refused_cx_changes_nothing() {
    let run = |refused: bool| {
        let mut bits = NoisyBits::new(SEED, 0.5);
        for q in 0..5 {
            bits.reset(q).expect("a bit");
        }
        if refused {
            assert!(bits.cx(0, 5).is_err(), "no sixth bit");
            assert!(bits.cx(5, 0).is_err(), "no sixth bit");
        }
        let after_it = snapshot(&bits);
        bits.cx(0, 1).expect("two bits");
        (after_it, snapshot(&bits))
    };
    assert_eq!(run(true), run(false));
}

/// The five bits and the log probability.
fn snapshot(bits: &NoisyBits) -> (Vec<bool>, f64) {
    let read = (0..5).map(|q| bits.read(q).expect("a bit")).collect();
    (read, bits.log_p())
}

/// One shot, the same round of `stim+rz` at [`DRAWS_FLIP`]: the executor
/// and the Qodec runtime's backend, from the shot's seed, read the same
/// bits with the same log probability.
#[test]
fn both_forms_read_the_same_bits() {
    let set = stim_rz();
    let calls = syndrome_round(&set);
    let shot = 6;
    let (expected, log_p) = on_the_simulator(&set, &calls, &[shot], DRAWS_FLIP);

    let mut backend = NoisyBitsBackend::new(shot_seed(SEED, shot), DRAWS_FLIP);
    let (ins, tags): (Vec<_>, Vec<_>) = calls.iter().map(|c| set.instruction(c)).unzip();
    let result = backend.execute_batch(&ins).expect("the backend runs it");
    let mut read = Vec::new();
    let tags = tags.iter().filter(|t| !t.is_empty());
    for (tags, r) in tags.zip(&result.results) {
        read.extend(outcomes(shot, tags, r.outcomes().iter().copied()));
    }
    read.sort_by_key(|o| o.tag);

    assert_eq!(read.len(), 5, "a readout per M");
    assert_eq!(read, expected);
    assert!((backend.bits().log_p() - log_p[0]).abs() < 1e-12);
}

/// Four shots of the same round at [`DRAWS_FLIP`]: each draws from its own
/// seed, so their syndromes (the ancillas, tags 0 and 1) are not all alike.
#[test]
fn noisy_shots_read_different_syndromes() {
    let set = stim_rz();
    let shots = [2, 5, 6, 11];
    let (read, _) = on_the_simulator(&set, &syndrome_round(&set), &shots, DRAWS_FLIP);

    let syndrome = |shot| -> Vec<Value> {
        read.iter()
            .filter(|o| o.shot == shot && o.tag < 2)
            .map(|o| o.value)
            .collect()
    };
    let syndromes: Vec<_> = shots.map(syndrome).into();
    assert!(syndromes.iter().all(|s| s.len() == 2));
    assert!(
        syndromes.iter().any(|s| *s != syndromes[0]),
        "{syndromes:?}"
    );
}

/// The reference readouts of the round at [`DRAWS_FLIP`] from [`SEED`],
/// shot by shot, tags 0 to 4 (the ancillas 3 and 4, then the data 0 to 2):
/// which bits read 1 shows where the flips landed, so a simulator that
/// draws in another order or noises another bit reads otherwise, while the
/// log probability, which counts flips only, may not show it.
#[test]
fn the_round_reads_the_reference_bits() {
    let set = stim_rz();
    let shots = [2, 5, 6, 11];
    let (read, log_p) = on_the_simulator(&set, &syndrome_round(&set), &shots, DRAWS_FLIP);

    let bits = |shot| -> String {
        read.iter()
            .filter(|o| o.shot == shot)
            .map(|o| match o.value {
                Value::Zero => '0',
                Value::One => '1',
                Value::Missing(e) => panic!("the bits read, not {e:?}"),
            })
            .collect()
    };
    let bits: Vec<_> = shots.map(|shot| (shot, bits(shot))).into();
    let expected = [(2, "00100"), (5, "01000"), (6, "10101"), (11, "11111")];
    assert_eq!(bits, expected.map(|(s, b)| (s, b.to_owned())));
    // Shots 2 and 6 draw as many flips, on other bits.
    let expected = [
        -7.178_667_852_365_132,
        -5.484_072_131_590_725,
        -7.178_667_852_365_132,
        -8.025_965_712_752_335,
    ];
    let close = log_p
        .iter()
        .zip(expected)
        .all(|(l, e)| (l - e).abs() < 1e-12);
    assert!(close, "{log_p:?}");
}

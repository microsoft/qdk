// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a batch's instructions against the receiver's set and the
//! shot's blocks, as at each `execute`, on instructions written as struct
//! literals.

use crate::instruction_set::instruction::batch_check::{BatchFault, BatchLimits, check_batch};
use crate::{
    Argument, Bit, BlockOperand, BlockType, Instruction, InstructionDef, InstructionFault,
    InstructionSet, Opcode, ParameterKind,
};
use smallvec::smallvec;

fn carbon() -> BlockType {
    BlockType::new("carbon")
}

/// A device's set over qubits and a code's `carbon` blocks: gates, Stim's
/// `MPP`, a flagged measurement, an encoding that turns a qubit into a
/// block, a preparation that makes a block, a gate coupling a qubit to a
/// block, another coupling a block to many qubits, an `x` run if a
/// parity of earlier readouts is odd, two encodings of a qubit beside a
/// block, one keeping that block, one consuming it, and, for the check of
/// blocks given twice, instructions consuming or making two blocks, of
/// one type or two, and merging many qubits into a block or splitting
/// them out of one.
fn device() -> InstructionSet {
    let qubit = || [BlockType::qubit()];
    InstructionSet::new(
        "Device",
        [(BlockType::qubit(), 1), (carbon(), 2)],
        [
            InstructionDef::new("h").acts_on(qubit()),
            InstructionDef::new("rz")
                .acts_on(qubit())
                .parameter("theta", ParameterKind::Number),
            InstructionDef::new("mpp")
                .acts_on([BlockOperand::variadic(BlockType::qubit())])
                .outcomes(1),
            InstructionDef::new("m")
                .acts_on(qubit())
                .outcomes(1)
                .flags(["reject", "leak"]),
            InstructionDef::new("encode")
                .takes(qubit())
                .gives([carbon()]),
            InstructionDef::new("prepare").gives([carbon()]),
            InstructionDef::new("couple").acts_on([BlockType::qubit(), carbon()]),
            InstructionDef::new("couple_many")
                .acts_on([carbon().into(), BlockOperand::variadic(BlockType::qubit())]),
            InstructionDef::new("cond_x")
                .acts_on(qubit())
                .parameter("c", ParameterKind::Bit),
            InstructionDef::new("encode_beside")
                .takes([BlockType::qubit(), carbon()])
                .gives([carbon(), carbon()]),
            InstructionDef::new("encode_consuming")
                .takes([BlockType::qubit(), carbon()])
                .gives([carbon()]),
            InstructionDef::new("measure_pair").takes([BlockType::qubit(), BlockType::qubit()]),
            InstructionDef::new("measure_mixed").takes([BlockType::qubit(), carbon()]),
            InstructionDef::new("prepare_pair").gives([carbon(), carbon()]),
            InstructionDef::new("prepare_mixed").gives([BlockType::qubit(), carbon()]),
            InstructionDef::new("merge_many")
                .takes([carbon().into(), BlockOperand::variadic(BlockType::qubit())])
                .gives([carbon()]),
            InstructionDef::new("split_many")
                .takes([carbon()])
                .gives([carbon().into(), BlockOperand::variadic(BlockType::qubit())]),
        ],
    )
    .expect("a valid set")
}

/// The limits of shots of `qubits` qubits and `carbons` blocks, which
/// use no readout parity.
fn limits(set: &InstructionSet, max: Option<u32>, qubits: u32, carbons: u32) -> BatchLimits {
    BatchLimits::new(
        set,
        max,
        |block_type| {
            if *block_type == BlockType::qubit() {
                qubits
            } else {
                carbons
            }
        },
        false,
    )
}

/// `mnemonic` on `blocks`, with no argument and no flag pattern, as a part
/// writes it on its hot path: unchecked.
fn literal(set: &InstructionSet, mnemonic: &str, blocks: &[u32]) -> Instruction {
    Instruction {
        opcode: set.opcode(mnemonic).expect("an instruction of the set"),
        blocks: blocks.iter().copied().collect(),
        arguments: smallvec![],
        select: None,
    }
}

fn fault_at(at: usize, fault: InstructionFault) -> Result<u32, BatchFault> {
    Err(BatchFault::Instruction { at, fault })
}

#[test]
fn batch_of_more_than_the_receiver_s_most_instructions_is_refused() {
    let set = device();
    let h = || literal(&set, "h", &[0]);
    let limits = limits(&set, Some(2), 1, 0);
    check_batch(&[h(), h()], &limits).expect("two instructions");
    assert_eq!(
        check_batch(&[h(), h(), h()], &limits),
        Err(BatchFault::TooManyInstructions { max: 2, got: 3 })
    );
}

/// An answer's width is a `u32`: a batch whose instructions' readout slots
/// sum to `u32::MAX` is accepted, and one with more refused, naming the
/// instruction from which they no longer fit.
#[test]
fn a_batch_reading_more_readout_slots_than_a_u32_counts_is_refused() {
    let set = InstructionSet::new(
        "Wide",
        [(BlockType::qubit(), 1)],
        [
            InstructionDef::new("half").outcomes(1 << 31),
            InstructionDef::new("rest").outcomes((1 << 31) - 1),
            InstructionDef::new("one").outcomes(1),
        ],
    )
    .expect("a set");
    let limits = limits(&set, None, 1, 0);
    let [half, rest, one] = ["half", "rest", "one"].map(|mnemonic| literal(&set, mnemonic, &[]));
    assert_eq!(
        check_batch(&[half.clone(), rest.clone()], &limits),
        Ok(u32::MAX)
    );
    assert_eq!(
        check_batch(&[half.clone(), half.clone()], &limits),
        Err(BatchFault::TooManyReadoutSlots { at: 1 })
    );
    assert_eq!(
        check_batch(&[one, half, rest], &limits),
        Err(BatchFault::TooManyReadoutSlots { at: 2 })
    );
}

#[test]
fn receiver_with_no_most_accepts_any_count() {
    let set = device();
    let batch = vec![literal(&set, "h", &[0]); 1000];
    check_batch(&batch, &limits(&set, None, 1, 0)).expect("no limit");
}

#[test]
fn opcode_beyond_the_set_is_refused() {
    let set = device();
    let mut beyond = literal(&set, "h", &[0]);
    beyond.opcode = Opcode::__from_macro(99);
    assert_eq!(
        check_batch(&[beyond], &limits(&set, None, 1, 0)),
        fault_at(0, InstructionFault::UnknownOpcode(Opcode::__from_macro(99)))
    );
}

/// The fault names the first refused instruction, at its position.
#[test]
fn first_refused_instruction_is_reported_at_its_position() {
    let set = device();
    let batch = [
        literal(&set, "h", &[0]),
        literal(&set, "h", &[]),
        literal(&set, "h", &[0, 1]),
    ];
    assert_eq!(
        check_batch(&batch, &limits(&set, None, 2, 0)),
        fault_at(
            1,
            InstructionFault::WrongBlockCount {
                mnemonic: "h".into(),
                expected: 1,
                got: 0,
                variadic: false,
            }
        )
    );
}

#[test]
fn block_beyond_the_shot_s_blocks_of_its_type_is_refused() {
    let set = device();
    let limits = limits(&set, None, 4, 0);
    check_batch(&[literal(&set, "h", &[3])], &limits).expect("the last qubit");
    assert_eq!(
        check_batch(&[literal(&set, "h", &[4])], &limits),
        fault_at(
            0,
            InstructionFault::BlockBeyondNeeds {
                mnemonic: "h".into(),
                block: 4,
                needs: 4,
            }
        )
    );
}

/// Shots of 1 qubit and 3 blocks: `prepare` makes block 2, an index among
/// the blocks, not the qubits.
#[test]
fn block_is_checked_against_the_shot_s_blocks_of_its_own_type() {
    let set = device();
    let limits = limits(&set, None, 1, 3);
    check_batch(&[literal(&set, "prepare", &[2])], &limits).expect("the last block");
    assert_eq!(
        check_batch(&[literal(&set, "prepare", &[3])], &limits),
        fault_at(
            0,
            InstructionFault::BlockBeyondNeeds {
                mnemonic: "prepare".into(),
                block: 3,
                needs: 3,
            }
        )
    );
}

/// `encode` turns qubit `k` into block `k`, so `k` must be below both the
/// shot's qubits and its blocks.
#[test]
fn block_that_changes_type_is_checked_against_both_types() {
    let set = device();
    let limits = limits(&set, None, 4, 2);
    check_batch(&[literal(&set, "encode", &[1])], &limits).expect("within both");
    assert_eq!(
        check_batch(&[literal(&set, "encode", &[2])], &limits),
        fault_at(
            0,
            InstructionFault::BlockBeyondNeeds {
                mnemonic: "encode".into(),
                block: 2,
                needs: 2,
            }
        )
    );
}

#[test]
fn variadic_operand_s_blocks_are_each_checked() {
    let set = device();
    assert_eq!(
        check_batch(
            &[literal(&set, "mpp", &[0, 1, 5])],
            &limits(&set, None, 4, 0)
        ),
        fault_at(
            0,
            InstructionFault::BlockBeyondNeeds {
                mnemonic: "mpp".into(),
                block: 5,
                needs: 4,
            }
        )
    );
}

#[test]
fn missing_argument_is_refused() {
    let set = device();
    assert_eq!(
        check_batch(&[literal(&set, "rz", &[0])], &limits(&set, None, 1, 0)),
        fault_at(
            0,
            InstructionFault::MissingArgument {
                mnemonic: "rz".into(),
                parameter: "theta".into(),
            }
        )
    );
}

#[test]
fn arguments_beyond_the_parameters_are_refused() {
    let set = device();
    let mut rz = literal(&set, "rz", &[0]);
    rz.arguments = smallvec![Argument::Number(0.1), Argument::Number(0.2)];
    assert_eq!(
        check_batch(&[rz], &limits(&set, None, 1, 0)),
        fault_at(
            0,
            InstructionFault::ExtraArguments {
                mnemonic: "rz".into(),
                expected: 1,
                got: 2,
            }
        )
    );
}

/// A pattern built for `m`'s flags, on an instruction rewritten to one
/// without them: the pattern names flag 1, which `h` lacks.
#[test]
fn select_over_flags_the_instruction_lacks_is_refused() {
    let set = device();
    let limits = limits(&set, None, 1, 0);
    let m = literal(&set, "m", &[0])
        .select(&set, [[("leak", 0)]])
        .expect("a declared flag");
    check_batch(std::slice::from_ref(&m), &limits).expect("the flags of `m`");
    let mut h = m;
    h.opcode = set.opcode("h").expect("an instruction of the set");
    assert_eq!(
        check_batch(&[h], &limits),
        fault_at(
            0,
            InstructionFault::UnknownFlag {
                mnemonic: "h".into(),
                name: "#1".into(),
            }
        )
    );
}

/// A `select` of no patterns, which only a struct literal makes: no shot's
/// flags could match it, so it is refused, as a select of one pattern is
/// accepted.
#[test]
fn select_of_no_patterns_is_refused() {
    let set = device();
    let limits = limits(&set, None, 1, 0);
    let m = literal(&set, "m", &[0])
        .select(&set, [[("leak", 0)]])
        .expect("a declared flag");
    check_batch(std::slice::from_ref(&m), &limits).expect("a select of one pattern");
    let mut empty = m;
    empty.select = Some(Box::new([]));
    assert_eq!(
        check_batch(&[empty], &limits),
        fault_at(
            0,
            InstructionFault::EmptySelect {
                mnemonic: "m".into(),
            }
        )
    );
}

/// Counts back start at 1, the shot's latest readout: a part writing
/// `recent(0)` by hand is refused at the check, as when built by name.
#[test]
fn bit_argument_counting_back_zero_is_refused() {
    let set = device();
    let mut cond_x = literal(&set, "cond_x", &[0]);
    cond_x.arguments = smallvec![Argument::recent(0)];
    assert_eq!(
        check_batch(&[cond_x], &limits(&set, None, 1, 0)),
        fault_at(
            0,
            InstructionFault::ZeroLookback {
                mnemonic: "cond_x".into(),
            }
        )
    );
}

/// A readout parity is resolved by the executor, which the admission
/// checks only for shots whose needs use one: a `cond_x` on a parity is
/// accepted for such shots, and refused for others; a bit known when
/// written is accepted for both.
#[test]
fn readout_parity_is_refused_unless_the_shots_needs_use_one() {
    let set = device();
    let cond_x = |argument: Argument| {
        let mut cond_x = literal(&set, "cond_x", &[0]);
        cond_x.arguments = smallvec![argument];
        cond_x
    };
    let without = limits(&set, None, 1, 0);
    let with = BatchLimits::new(&set, None, |_| 1, true);
    check_batch(&[cond_x(Argument::recent(1))], &with).expect("parities used");
    check_batch(&[cond_x(Argument::Bit(Bit::Value(true)))], &without).expect("a known bit");
    assert_eq!(
        check_batch(&[cond_x(Argument::recent(1))], &without),
        fault_at(
            0,
            InstructionFault::ParityNotProvisioned {
                mnemonic: "cond_x".into(),
                parameter: "c".into(),
            }
        )
    );
}

/// Qubit 0 and block 0 are two units of the shot's state.
#[test]
fn blocks_of_different_types_may_share_an_index() {
    let set = device();
    check_batch(
        &[literal(&set, "couple", &[0, 0])],
        &limits(&set, None, 1, 1),
    )
    .expect("a qubit and a block");
}

/// Beyond a few blocks, the check sorts them: a variadic `MPP` on many
/// qubits is refused when one is given twice, and only then.
#[test]
fn many_blocks_with_one_given_twice_are_refused() {
    let set = device();
    let limits = limits(&set, None, 40, 0);
    let distinct: Vec<u32> = (0..30).rev().collect();
    check_batch(&[literal(&set, "mpp", &distinct)], &limits).expect("distinct qubits");
    let mut repeated = distinct;
    repeated.push(17);
    assert_eq!(
        check_batch(&[literal(&set, "mpp", &repeated)], &limits),
        fault_at(
            0,
            InstructionFault::RepeatedBlock {
                mnemonic: "mpp".into(),
                block: 17,
            }
        )
    );
}

/// Beyond a few blocks, where the check sorts them, a qubit and a block
/// still share an index: block 0 and qubits 0 to 19.
#[test]
fn many_blocks_of_different_types_may_share_an_index() {
    let set = device();
    let mut blocks = vec![0];
    blocks.extend(0..20);
    check_batch(
        &[literal(&set, "couple_many", &blocks)],
        &limits(&set, None, 20, 1),
    )
    .expect("a block and twenty qubits");
}

/// A block keeps its index when it changes type: qubit 0 encoded beside
/// block 0, which survives, would leave two blocks 0.
#[test]
fn block_changing_type_onto_the_index_of_a_surviving_block_is_refused() {
    let set = device();
    assert_eq!(
        check_batch(
            &[literal(&set, "encode_beside", &[0, 0])],
            &limits(&set, None, 1, 1)
        ),
        fault_at(
            0,
            InstructionFault::RepeatedBlock {
                mnemonic: "encode_beside".into(),
                block: 0,
            }
        )
    );
}

/// Qubit 0 encoded as block 0 while block 0 is consumed: one block 0
/// before the instruction, one after.
#[test]
fn block_changing_type_onto_the_index_of_a_consumed_block_is_accepted() {
    let set = device();
    check_batch(
        &[literal(&set, "encode_consuming", &[0, 0])],
        &limits(&set, None, 1, 1),
    )
    .expect("block 0 replaced");
}

/// Measuring qubit 0 twice in one pair: one block, consumed twice.
#[test]
fn consumed_blocks_of_one_type_and_index_are_refused() {
    let set = device();
    assert_eq!(
        check_batch(
            &[literal(&set, "measure_pair", &[0, 0])],
            &limits(&set, None, 1, 0)
        ),
        fault_at(
            0,
            InstructionFault::RepeatedBlock {
                mnemonic: "measure_pair".into(),
                block: 0,
            }
        )
    );
}

/// Qubit 0 and block 0, both consumed: two blocks, though neither
/// survives the instruction.
#[test]
fn consumed_blocks_of_different_types_may_share_an_index() {
    let set = device();
    check_batch(
        &[literal(&set, "measure_mixed", &[0, 0])],
        &limits(&set, None, 1, 1),
    )
    .expect("a qubit and a block");
}

/// Two blocks prepared as block 0: one index for two blocks after the
/// instruction.
#[test]
fn new_blocks_of_one_type_and_index_are_refused() {
    let set = device();
    assert_eq!(
        check_batch(
            &[literal(&set, "prepare_pair", &[0, 0])],
            &limits(&set, None, 0, 1)
        ),
        fault_at(
            0,
            InstructionFault::RepeatedBlock {
                mnemonic: "prepare_pair".into(),
                block: 0,
            }
        )
    );
}

/// Qubit 0 and block 0, both new: two blocks, though neither existed
/// before the instruction.
#[test]
fn new_blocks_of_different_types_may_share_an_index() {
    let set = device();
    check_batch(
        &[literal(&set, "prepare_mixed", &[0, 0])],
        &limits(&set, None, 1, 1),
    )
    .expect("a qubit and a block");
}

/// Beyond a few blocks, where the check sorts them: twenty qubits merged
/// into block 0, which the instruction consumes, one of them twice.
#[test]
fn many_consumed_blocks_with_one_given_twice_are_refused() {
    let set = device();
    let mut blocks = vec![0];
    blocks.extend(0..20);
    blocks.push(5);
    assert_eq!(
        check_batch(
            &[literal(&set, "merge_many", &blocks)],
            &limits(&set, None, 20, 1)
        ),
        fault_at(
            0,
            InstructionFault::RepeatedBlock {
                mnemonic: "merge_many".into(),
                block: 5,
            }
        )
    );
}

/// Beyond a few blocks, where the check sorts them: twenty qubits split
/// out of block 0, one of them made twice.
#[test]
fn many_new_blocks_with_one_given_twice_are_refused() {
    let set = device();
    let mut blocks = vec![0];
    blocks.extend(0..20);
    blocks.push(5);
    assert_eq!(
        check_batch(
            &[literal(&set, "split_many", &blocks)],
            &limits(&set, None, 20, 1)
        ),
        fault_at(
            0,
            InstructionFault::RepeatedBlock {
                mnemonic: "split_many".into(),
                block: 5,
            }
        )
    );
}

/// Of several blocks given twice, the fault names the one at the earliest
/// position that repeats a block before it, for few blocks as for many:
/// in `5, 3, 5, 3`, block 5 (not 3, the smaller).
#[test]
fn repeated_block_named_is_the_earliest_repeat_in_position_order() {
    let set = device();
    let limits = limits(&set, None, 40, 0);
    let few = vec![5, 3, 5, 3];
    let mut many = few.clone();
    many.extend(10..30);
    for blocks in [few, many] {
        assert_eq!(
            check_batch(&[literal(&set, "mpp", &blocks)], &limits),
            fault_at(
                0,
                InstructionFault::RepeatedBlock {
                    mnemonic: "mpp".into(),
                    block: 5,
                }
            ),
            "{} blocks",
            blocks.len()
        );
    }
}

/// The answer to a batch has a value per readout slot of its instructions:
/// as many as each instruction's outcomes and flags, summed.
#[test]
fn a_batchs_readout_slots_count_each_instructions_outcomes_and_flags() {
    let set = device();
    let limits = limits(&set, None, 2, 0);
    let batch = [
        literal(&set, "h", &[0]),
        literal(&set, "m", &[0]),
        literal(&set, "mpp", &[0, 1]),
    ];
    assert_eq!(check_batch(&batch, &limits), Ok(4));
}

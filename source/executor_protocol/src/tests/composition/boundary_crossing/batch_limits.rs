// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The checks of a batch at an `execute`: its count of instructions, on
//! every batch, and its instructions against the receiver's set and the
//! shot's blocks, and its stated readout slots against theirs, on a
//! checked batch. One batch of instructions written as struct literals, as
//! a part writes them on its hot path, sent by a driver straight to an
//! executor, by `execute`, by `execute_each`, and, when it states no readout
//! slots and fits one piece, as a stream by `execute_stream`, which check
//! alike.

use crate::{
    Argument, Batch, BatchReadouts, Bit, BlockOperand, BlockType, Drive, DriveError, Driver,
    DriverSetup, Execute, ExecuteError, ExecuteRefusal, Executor, ExecutorSetup, Failure, FinalRow,
    Instruction, InstructionDef, InstructionFault, InstructionSet, InstructionStream, Needs, Offer,
    Opcode, OpcodeMap, ParameterKind, ReadoutValue, Readouts, RunError, Seed, ShotId, ShotSet,
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

/// How a sender marks a new batch before it sends it.
type Mark = for<'a> fn(Batch<'a>) -> Batch<'a>;

/// A driver sending `instructions` as one batch to every shot, marked by
/// `mark`, as they were written: in the opcodes of `set`, which is also
/// the receiver's, stating `stated` readout slots, or else theirs (0 if
/// they cannot be counted), by the call `by` says. Each shot's record is
/// the readout slots of the batch's readouts.
#[derive(Clone)]
struct Literal {
    set: InstructionSet,
    instructions: Vec<Instruction>,
    stated: Option<u32>,
    needs: Needs,
    mark: Mark,
    by: By,
}

/// The call a [`Literal`] sends its batch by.
#[derive(Clone, Copy, Debug)]
enum By {
    Execute,
    /// `execute_each`, its rows ignored.
    ExecuteEach,
    /// `execute_stream`, of a stream of its instructions, checked as its
    /// mark leaves a batch, stating no readout slots.
    ExecuteStream,
}

impl DriverSetup for Literal {
    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    fn needs(&self) -> Needs {
        self.needs.clone()
    }
}

impl Drive for Literal {
    type Output = u32;

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<u32>, DriveError> {
        let readout_slots = self
            .stated
            .unwrap_or_else(|| self.set.readout_slots_of(&self.instructions).unwrap_or(0));
        let batches = [(self.mark)(Batch::new(
            shots,
            &self.instructions,
            readout_slots,
        ))];
        let mut kept = Readouts::new();
        let readouts = kept.readouts_for(&batches);
        match self.by {
            By::Execute => below.execute(&batches, seed, readouts).await?,
            By::ExecuteEach => {
                below
                    .execute_each(&batches, seed, readouts, &mut |_: FinalRow<'_>| {})
                    .await?;
            }
            By::ExecuteStream => {
                let stream = InstructionStream::new(shots, self.instructions.iter().cloned())
                    .checked_if(batches[0].is_checked());
                below.execute_stream(stream, seed, &mut readouts[0]).await?;
            }
        }
        Ok(vec![readouts[0].readout_slots(); shots.len()])
    }
}

/// An executor of `set` answering `Zero` for every readout, whatever the
/// batch.
struct Zeros {
    set: InstructionSet,
    offer: Offer,
}

impl ExecutorSetup for Zeros {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn offers(&self) -> Offer {
        self.offer.clone()
    }
}

impl Execute for Zeros {
    async fn execute(
        &mut self,
        _batches: &[Batch<'_>],
        _seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        for batch_readouts in readouts {
            for position in 0..batch_readouts.shots() {
                batch_readouts.row_mut(position).fill(ReadoutValue::Zero);
            }
        }
        Ok(())
    }
}

/// What the protocol makes of `instructions` of `set`, sent to shots 0 and
/// 1, needing `needs`, as one batch, checked if `checked`, over an executor
/// of `set` taking `most` instructions at most: the readout slots their
/// readouts were shaped with, if they ran, or the refusal.
fn checked(
    set: &InstructionSet,
    instructions: Vec<Instruction>,
    needs: Needs,
    most: Option<u32>,
    checked: bool,
) -> Result<u32, ExecuteRefusal> {
    let mark: Mark = if checked {
        |batch| batch.checked()
    } else {
        |batch| batch
    };
    checked_as_marked(set, instructions, None, needs, most, mark)
}

/// As [`checked`], with the batch marked by `mark`, stating `stated`
/// readout slots if some: what `execute` makes of it, after checking that
/// `execute_each` makes the same, and so does `execute_stream` when the
/// batch states no slots and fits one piece, the piece then being the
/// batch.
fn checked_as_marked(
    set: &InstructionSet,
    instructions: Vec<Instruction>,
    stated: Option<u32>,
    needs: Needs,
    most: Option<u32>,
    mark: Mark,
) -> Result<u32, ExecuteRefusal> {
    let mut offer = Offer::unlimited().max_blocks(carbon(), 64);
    if let Some(most) = most {
        offer = offer.max_instructions(most);
    }
    let fits_a_piece = most.is_none_or(|most| instructions.len() <= most as usize);
    let run = |literal: Literal| run_over_zeros(literal, offer.clone());
    let plain = Literal {
        set: set.clone(),
        instructions,
        stated,
        needs,
        mark,
        by: By::Execute,
    };
    let executed = run(plain.clone());
    let mut alike = vec![By::ExecuteEach];
    if stated.is_none() && fits_a_piece {
        alike.push(By::ExecuteStream);
    }
    for by in alike {
        assert_eq!(
            run(Literal {
                by,
                ..plain.clone()
            }),
            executed,
            "{by:?} checks as `execute`"
        );
    }
    executed
}

/// What `literal` makes of its batch on shots 0 and 1, over [`Zeros`] of
/// its set offering `offer`: the readout slots its readouts were shaped
/// with, if it ran, or the refusal.
fn run_over_zeros(literal: Literal, offer: Offer) -> Result<u32, ExecuteRefusal> {
    let executor = Executor::new(Zeros {
        set: literal.set.clone(),
        offer,
    });
    let Ok(mut runner) = Driver::new(literal) >> executor else {
        panic!("the chain composes");
    };
    refusal_of(match runner.run(&ShotSet::ids(0..2), 7) {
        Ok(slots) => Ok(slots[0]),
        Err(RunError::Failed(error)) => Err(error),
        Err(other) => panic!("a run, not {other:?}"),
    })
}

/// The readout slots of a run that ran, or the refusal of one refused.
fn refusal_of(run: Result<u32, DriveError>) -> Result<u32, ExecuteRefusal> {
    match run {
        Ok(slots) => Ok(slots),
        Err(DriveError {
            failure: Failure::Refused(refusal),
            ..
        }) => Err(refusal),
        Err(error) => panic!("a run or a refusal, not {error:?}"),
    }
}

/// Shots of `qubits` qubits and `carbons` blocks, using no readout
/// parity.
fn shots_of(qubits: u32, carbons: u32) -> Needs {
    Needs::qubits(qubits).blocks(carbon(), carbons)
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

/// The refusal of the batch, naming the instruction at `at` and `fault`.
fn refused_at(at: usize, fault: InstructionFault) -> Result<u32, ExecuteRefusal> {
    Err(ExecuteRefusal::Instruction {
        shot: ShotId::new(0),
        batch: 0,
        at,
        fault,
    })
}

/// A row of a table: a scene, its batch of `device()`, the shots' needs,
/// and what the check makes of it.
type Row = (
    &'static str,
    Vec<Instruction>,
    Needs,
    Result<u32, ExecuteRefusal>,
);

/// Each row of `rows`, sent to an executor of `device()` taking any count
/// of instructions.
fn check_rows(rows: Vec<Row>) {
    let set = device();
    for (scene, batch, needs, expected) in rows {
        assert_eq!(checked(&set, batch, needs, None, true), expected, "{scene}");
    }
}

/// A batch of as many instructions as the receiver takes at most runs, a
/// longer one is refused, checked or not, so that an executor with a
/// buffer of that size never receives more; and a receiver with no most
/// takes any count.
#[test]
fn a_batch_of_more_than_the_receivers_most_instructions_is_refused() {
    let set = device();
    let h = |count| vec![literal(&set, "h", &[0]); count];
    let too_many = Err(ExecuteRefusal::TooManyInstructions {
        shot: ShotId::new(0),
        batch: 0,
        max: 2,
        got: 3,
    });
    for (most, count, sent_checked, expected) in [
        (Some(2), 2, true, Ok(0)),
        (Some(2), 2, false, Ok(0)),
        (Some(2), 3, true, too_many.clone()),
        (Some(2), 3, false, too_many),
        (None, 1000, true, Ok(0)),
    ] {
        assert_eq!(
            checked(&set, h(count), Needs::qubits(1), most, sent_checked),
            expected,
            "{count} instructions, at most {most:?}, checked: {sent_checked}"
        );
    }
}

/// A batch is checked as the last mark its sender gave it says: one an
/// adapter marks `checked_if(false)`, as it does to pass on that the batch
/// it adapts is unchecked, is unchecked even after `checked`, and one
/// marked `checked` after it is checked. Holding an `h` on qubit 1 of
/// shots of one qubit, or an
/// instruction whose opcode is beyond the receiver's set, an unchecked
/// batch reaches the receiver as sent, uninterpreted; a checked one is
/// refused.
#[test]
fn a_batch_is_checked_as_its_last_mark_says() {
    let set = device();
    let mut beyond_the_set = literal(&set, "h", &[0]);
    beyond_the_set.opcode = Opcode::__from_macro(99);
    let faults = [
        (
            "a block beyond the shots' needs",
            literal(&set, "h", &[1]),
            InstructionFault::BlockBeyondNeeds {
                mnemonic: "h".into(),
                block: 1,
                needs: 1,
            },
        ),
        (
            "an opcode beyond the receiver's set",
            beyond_the_set,
            InstructionFault::UnknownOpcode(Opcode::__from_macro(99)),
        ),
    ];
    let marks: [(&str, Mark, bool); 4] = [
        ("checked_if(true)", |batch| batch.checked_if(true), true),
        ("checked_if(false)", |batch| batch.checked_if(false), false),
        (
            "checked, then checked_if(false)",
            |batch| batch.checked().checked_if(false),
            false,
        ),
        (
            "checked_if(false), then checked",
            |batch| batch.checked_if(false).checked(),
            true,
        ),
    ];
    for (faulty, instruction, fault) in faults {
        for (marked, mark, refused) in marks {
            let run = checked_as_marked(
                &set,
                vec![instruction.clone()],
                None,
                Needs::qubits(1),
                None,
                mark,
            );
            let expected = if refused {
                refused_at(0, fault.clone())
            } else {
                Ok(0)
            };
            assert_eq!(run, expected, "{faulty}, {marked}");
        }
    }
}

/// A batch's readout slots are a `u32`: a checked batch whose
/// instructions' readout slots sum to more is refused, naming the
/// instruction from which they no longer fit, before any later instruction
/// is checked. So is a stream, checked or not, since its receiver counts
/// its slots as it runs it, to shape its readouts.
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
    let [half, rest, one] = ["half", "rest", "one"].map(|mnemonic| literal(&set, mnemonic, &[]));
    let mut beyond = one.clone();
    beyond.opcode = Opcode::__from_macro(99);
    for (scene, batch, at) in [
        ("twice 2^31", vec![half.clone(), half.clone()], 1),
        (
            "twice 2^31, then an opcode beyond the set",
            vec![half.clone(), half.clone(), beyond],
            1,
        ),
        ("1, 2^31, then 2^31 - 1", vec![one, half, rest], 2),
    ] {
        let too_many = Err(ExecuteRefusal::TooManyReadoutSlots {
            shot: ShotId::new(0),
            batch: 0,
            at,
        });
        assert_eq!(
            checked(&set, batch.clone(), Needs::qubits(1), None, true),
            too_many,
            "{scene}"
        );
        let unchecked_stream = Literal {
            set: set.clone(),
            instructions: batch,
            stated: None,
            needs: Needs::qubits(1),
            mark: |batch| batch,
            by: By::ExecuteStream,
        };
        assert_eq!(
            run_over_zeros(unchecked_stream, Offer::unlimited()),
            too_many,
            "{scene}, as an unchecked stream"
        );
    }
}

/// A checked batch stating other readout slots than its instructions read
/// is refused, naming both, so that its readouts are never shaped wrong;
/// one stating theirs, each instruction's outcomes and flags summed, runs,
/// its readouts shaped with them.
#[test]
fn a_checked_batch_stating_other_readout_slots_is_refused() {
    let set = device();
    let batch = vec![literal(&set, "m", &[0]), literal(&set, "mpp", &[0, 1])];
    let misstated = |stated| {
        Err(ExecuteRefusal::ReadoutSlotsMisstated {
            shot: ShotId::new(0),
            batch: 0,
            stated,
            counted: 4,
        })
    };
    let mark_checked: Mark = |batch| batch.checked();
    for (stated, expected) in [(4, Ok(4)), (3, misstated(3)), (5, misstated(5))] {
        let run = checked_as_marked(
            &set,
            batch.clone(),
            Some(stated),
            Needs::qubits(2),
            None,
            mark_checked,
        );
        assert_eq!(run, expected, "stating {stated}");
    }
}

/// The refusal names the first refused instruction, at its position.
#[test]
fn first_refused_instruction_is_reported_at_its_position() {
    let set = device();
    check_rows(vec![(
        "`h` on 1, 0 and 2 qubits",
        vec![
            literal(&set, "h", &[0]),
            literal(&set, "h", &[]),
            literal(&set, "h", &[0, 1]),
        ],
        Needs::qubits(2),
        refused_at(
            1,
            InstructionFault::WrongBlockCount {
                mnemonic: "h".into(),
                expected: 1,
                got: 0,
                variadic: false,
            },
        ),
    )]);
}

/// An instruction its set does not declare, or whose arguments or flag
/// patterns its instruction does not take, is refused; the same
/// instruction, with what it takes, runs. A readout parity, even of no
/// readout, is taken only when the shots' needs use one; a bit known when
/// written, whatever the needs.
#[test]
#[expect(clippy::too_many_lines, reason = "a table, one row per scene")]
fn an_instruction_its_set_does_not_take_is_refused() {
    let set = device();
    let mut beyond = literal(&set, "h", &[0]);
    beyond.opcode = Opcode::__from_macro(99);
    let rz = |arguments: &[f64]| {
        let mut rz = literal(&set, "rz", &[0]);
        rz.arguments = arguments.iter().map(|&a| Argument::Number(a)).collect();
        rz
    };
    let m = literal(&set, "m", &[0])
        .select(&set, [[("leak", 0)]])
        .expect("a declared flag");
    let mut h_with_ms_pattern = m.clone();
    h_with_ms_pattern.opcode = set.opcode("h").expect("an instruction of the set");
    let mut h_with_ms_first_flag = literal(&set, "m", &[0])
        .select(&set, [[("reject", 1)]])
        .expect("a declared flag");
    h_with_ms_first_flag.opcode = h_with_ms_pattern.opcode;
    let mut select_of_none = m.clone();
    select_of_none.select = Some(Box::new([]));
    let cond_x = |argument| {
        let mut cond_x = literal(&set, "cond_x", &[0]);
        cond_x.arguments = smallvec![argument];
        cond_x
    };
    let qubit = || Needs::qubits(1);
    check_rows(vec![
        (
            "an opcode beyond the set",
            vec![beyond],
            qubit(),
            refused_at(0, InstructionFault::UnknownOpcode(Opcode::__from_macro(99))),
        ),
        ("`rz` with its angle", vec![rz(&[0.1])], qubit(), Ok(0)),
        (
            "`rz` with no angle",
            vec![rz(&[])],
            qubit(),
            refused_at(
                0,
                InstructionFault::MissingArgument {
                    mnemonic: "rz".into(),
                    parameter: "theta".into(),
                },
            ),
        ),
        (
            "`rz` with two angles",
            vec![rz(&[0.1, 0.2])],
            qubit(),
            refused_at(
                0,
                InstructionFault::ExtraArguments {
                    mnemonic: "rz".into(),
                    expected: 1,
                    got: 2,
                },
            ),
        ),
        ("`m` selecting on its flags", vec![m], qubit(), Ok(3)),
        (
            "a pattern of `m`'s flags on `h`, which has none",
            vec![h_with_ms_pattern],
            qubit(),
            refused_at(
                0,
                InstructionFault::UnknownFlag {
                    mnemonic: "h".into(),
                    name: "#1".into(),
                },
            ),
        ),
        (
            "a pattern on flag 0 on `h`, just past its no flags",
            vec![h_with_ms_first_flag],
            qubit(),
            refused_at(
                0,
                InstructionFault::UnknownFlag {
                    mnemonic: "h".into(),
                    name: "#0".into(),
                },
            ),
        ),
        (
            "a `select` of no patterns, which no shot's flags match",
            vec![select_of_none],
            qubit(),
            refused_at(
                0,
                InstructionFault::EmptySelect {
                    mnemonic: "m".into(),
                },
            ),
        ),
        (
            "`cond_x` on the latest readout",
            vec![cond_x(Argument::recent(1))],
            qubit().with_readout_parity(),
            Ok(0),
        ),
        (
            "`cond_x` on a bit known when written, the needs using no parity",
            vec![cond_x(Argument::Bit(Bit::Value(true)))],
            qubit(),
            Ok(0),
        ),
        (
            "`cond_x` on the parity of no readout, the needs using none",
            vec![cond_x(Argument::parity([], true))],
            qubit(),
            refused_at(
                0,
                InstructionFault::ParityNotNeeded {
                    mnemonic: "cond_x".into(),
                    parameter: "c".into(),
                },
            ),
        ),
        (
            "`cond_x` on the parity of no readout, the needs using parities",
            vec![cond_x(Argument::parity([], true))],
            qubit().with_readout_parity(),
            Ok(0),
        ),
        (
            "`cond_x` counting back 0 readouts, as when built by name",
            vec![cond_x(Argument::recent(0))],
            qubit().with_readout_parity(),
            refused_at(
                0,
                InstructionFault::ZeroLookback {
                    mnemonic: "cond_x".into(),
                },
            ),
        ),
        (
            "`cond_x` counting back six readouts, none 0",
            vec![cond_x(Argument::parity([1, 2, 3, 4, 5, 6], false))],
            qubit().with_readout_parity(),
            Ok(0),
        ),
        (
            "`cond_x` counting back 0 readouts last of six",
            vec![cond_x(Argument::parity([1, 2, 3, 4, 5, 0], false))],
            qubit().with_readout_parity(),
            refused_at(
                0,
                InstructionFault::ZeroLookback {
                    mnemonic: "cond_x".into(),
                },
            ),
        ),
    ]);
}

/// Each block an instruction names is below the shots' blocks of its
/// type: for one type, for its own type among several, for a type the
/// shots need none of, for both types of a block that changes type, and
/// for each block of a variadic operand.
#[test]
fn a_block_beyond_the_shots_blocks_of_its_type_is_refused() {
    let set = device();
    let beyond = |mnemonic: &str, block, needs| {
        refused_at(
            0,
            InstructionFault::BlockBeyondNeeds {
                mnemonic: mnemonic.into(),
                block,
                needs,
            },
        )
    };
    let on = |mnemonic, blocks| vec![literal(&set, mnemonic, blocks)];
    check_rows(vec![
        (
            "`h` on the last of 4 qubits",
            on("h", &[3]),
            shots_of(4, 0),
            Ok(0),
        ),
        (
            "`h` on qubit 4 of 4",
            on("h", &[4]),
            shots_of(4, 0),
            beyond("h", 4, 4),
        ),
        (
            "`prepare` making block 2, of 3 blocks and 1 qubit",
            on("prepare", &[2]),
            shots_of(1, 3),
            Ok(0),
        ),
        (
            "`prepare` making block 3, of 3 blocks and 1 qubit",
            on("prepare", &[3]),
            shots_of(1, 3),
            beyond("prepare", 3, 3),
        ),
        (
            "`prepare` making block 0, of shots needing no block of its type",
            on("prepare", &[0]),
            Needs::qubits(1),
            beyond("prepare", 0, 0),
        ),
        (
            "`encode` turning qubit 1 into block 1, of 4 qubits and 2 blocks",
            on("encode", &[1]),
            shots_of(4, 2),
            Ok(0),
        ),
        (
            "`encode` turning qubit 2 into block 2, of 4 qubits and 2 blocks",
            on("encode", &[2]),
            shots_of(4, 2),
            beyond("encode", 2, 2),
        ),
        (
            "`mpp` on qubits 0, 1 and 5 of 4",
            on("mpp", &[0, 1, 5]),
            shots_of(4, 0),
            beyond("mpp", 5, 4),
        ),
    ]);
}

/// Within one instruction, a block, a type and an index, is not consumed
/// twice, made twice, nor made where one survives, for few blocks as for
/// many; of several given twice, the refusal names the one at the
/// earliest position that repeats a block before it.
#[test]
fn a_block_given_twice_by_one_instruction_is_refused() {
    let set = device();
    let repeated = |mnemonic: &str, block| {
        refused_at(
            0,
            InstructionFault::RepeatedBlock {
                mnemonic: mnemonic.into(),
                block,
            },
        )
    };
    let on = |mnemonic, blocks: &[u32]| vec![literal(&set, mnemonic, blocks)];
    let distinct: Vec<u32> = (0..30).rev().collect();
    let mut with_17_twice = distinct.clone();
    with_17_twice.push(17);
    let mut block_0_and_20_qubits_one_twice = vec![0];
    block_0_and_20_qubits_one_twice.extend(0..20);
    block_0_and_20_qubits_one_twice.push(5);
    let five_and_three_twice = [5, 3, 5, 3];
    let mut many_with_five_and_three_twice = five_and_three_twice.to_vec();
    many_with_five_and_three_twice.extend(10..30);
    check_rows(vec![
        (
            "measuring qubit 0 twice in one pair",
            on("measure_pair", &[0, 0]),
            shots_of(1, 0),
            repeated("measure_pair", 0),
        ),
        (
            "preparing block 0 twice in one pair",
            on("prepare_pair", &[0, 0]),
            shots_of(0, 1),
            repeated("prepare_pair", 0),
        ),
        (
            "encoding qubit 0 as block 0 beside block 0, which survives",
            on("encode_beside", &[0, 0]),
            shots_of(1, 1),
            repeated("encode_beside", 0),
        ),
        (
            "`mpp` on 30 distinct qubits",
            on("mpp", &distinct),
            shots_of(40, 0),
            Ok(1),
        ),
        (
            "`mpp` on 30 qubits, then qubit 17 again",
            on("mpp", &with_17_twice),
            shots_of(40, 0),
            repeated("mpp", 17),
        ),
        (
            "merging 20 qubits, qubit 5 twice, into block 0",
            on("merge_many", &block_0_and_20_qubits_one_twice),
            shots_of(20, 1),
            repeated("merge_many", 5),
        ),
        (
            "splitting 20 qubits, qubit 5 twice, out of block 0",
            on("split_many", &block_0_and_20_qubits_one_twice),
            shots_of(20, 1),
            repeated("split_many", 5),
        ),
        (
            "`mpp` on qubits 5, 3, 5, 3: block 5, not 3, the smaller",
            on("mpp", &five_and_three_twice),
            shots_of(40, 0),
            repeated("mpp", 5),
        ),
        (
            "`mpp` on qubits 5, 3, 5, 3, then 10 to 29: block 5",
            on("mpp", &many_with_five_and_three_twice),
            shots_of(40, 0),
            repeated("mpp", 5),
        ),
    ]);
}

/// Blocks of different types, or one consumed and one made, are different
/// units of the shot's state, so they may share an index, for few blocks
/// as for many.
#[test]
fn blocks_of_different_types_or_lives_may_share_an_index() {
    let set = device();
    let on = |mnemonic, blocks: &[u32]| vec![literal(&set, mnemonic, blocks)];
    let mut block_0_and_20_qubits = vec![0];
    block_0_and_20_qubits.extend(0..20);
    check_rows(vec![
        (
            "coupling qubit 0 to block 0",
            on("couple", &[0, 0]),
            shots_of(1, 1),
            Ok(0),
        ),
        (
            "measuring qubit 0 and block 0",
            on("measure_mixed", &[0, 0]),
            shots_of(1, 1),
            Ok(0),
        ),
        (
            "preparing qubit 0 and block 0",
            on("prepare_mixed", &[0, 0]),
            shots_of(1, 1),
            Ok(0),
        ),
        (
            "encoding qubit 0 as block 0, consuming block 0",
            on("encode_consuming", &[0, 0]),
            shots_of(1, 1),
            Ok(0),
        ),
        (
            "coupling block 0 to qubits 0 to 19",
            on("couple_many", &block_0_and_20_qubits),
            shots_of(20, 1),
            Ok(0),
        ),
        (
            "splitting block 0 into block 0 and qubits 1 to 19",
            on("split_many", &block_0_and_20_qubits),
            shots_of(20, 1),
            Ok(0),
        ),
    ]);
}

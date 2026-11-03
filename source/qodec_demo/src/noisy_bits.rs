//! The simulator of the target chain: five bits with bit-flip noise
//! ([`NoisyBits`]), one per shot, in two forms that run the same bits on
//! the same physical instructions of `stim+rz`: the protocol's executor
//! ([`simulator`], made with `executor_fn`), and the Qodec runtime's
//! bottom backend of one shot ([`NoisyBitsBackend`]). On an instruction
//! that fails as it runs (a qubit beyond the five bits), the executor's
//! reply fails after the readouts made before it, as the protocol does,
//! while the backend returns only the error.
//!
//! The bits are exact for repetition3's physical circuits: they prepare,
//! copy and read in the Z basis only, and `rotate_z` is a phase, which no
//! Z readout sees.

use std::sync::Arc;

use executor_protocol::{
    CallInput, Capabilities, ExecutorLogic, InstructionCall, InstructionSet, MakeState, OnePerShot,
    OnePerShotExecutor, Opcode, Operand, OperandKind, Outcome, Outcomes, Rejected, ShotContext,
    executor_fn,
};
use qodec_runtime::{Backend, BasicReadout, BatchResult, OwnedParam, Readout, Status};

use crate::boundary::Ins;

/// The bits a [`NoisyBits`] holds: physical qubits 0 to 4, enough for
/// repetition3 (three data qubits and `idle`'s two ancillas).
pub const QUBITS: u32 = 5;

/// `stim+rz`'s instructions, at their index in the spec's declaration
/// order: the opcodes of the protocol's set of it
/// ([`Boundary`](crate::boundary::Boundary) keeps that order), and the Qodec
/// runtime's `instruction_id`s.
pub const R: Opcode = Opcode::new(0);
/// See [`R`].
pub const M: Opcode = Opcode::new(1);
/// See [`R`].
pub const CX: Opcode = Opcode::new(2);
/// See [`R`].
pub const ROTATE_Z: Opcode = Opcode::new(3);

/// One shot's five bits, each flipped by noise with chance `flip` when a
/// reset or a CX touches it, and the log probability of the draws made.
///
/// The draws are a `SplitMix64` stream from the shot's seed, one per bit
/// touched, in the order the instructions touch them: so two forms that
/// run the same instructions from the same seed draw alike.
#[derive(Clone, Debug)]
pub struct NoisyBits {
    bits: [bool; QUBITS as usize],
    state: u64,
    flip: f64,
    log_p: f64,
}

impl NoisyBits {
    /// Five bits at 0, drawing from `seed`, each touch flipping with
    /// chance `flip`.
    ///
    /// # Panics
    ///
    /// When `flip` is not a chance: below 0, above 1, or NaN.
    #[must_use]
    pub fn new(seed: u64, flip: f64) -> Self {
        assert_chance(flip);
        Self {
            bits: [false; QUBITS as usize],
            state: seed,
            flip,
            log_p: 0.0,
        }
    }

    /// Sets bit `q` to 0, then flips it with chance `flip`.
    ///
    /// # Errors
    ///
    /// When `q` is not one of the five bits.
    pub fn reset(&mut self, q: u64) -> Result<(), String> {
        *self.bit(q)? = false;
        self.noise(q)
    }

    /// Adds bit `c` to bit `t`, then flips each with chance `flip`, `c`
    /// first. `c == t` clears the bit, then draws for it twice.
    ///
    /// # Errors
    ///
    /// When `c` or `t` is not one of the five bits; nothing changes then.
    pub fn cx(&mut self, c: u64, t: u64) -> Result<(), String> {
        let control = *self.bit(c)?;
        *self.bit(t)? ^= control;
        self.noise(c)?;
        self.noise(t)
    }

    /// Bit `q`, read without noise.
    ///
    /// # Errors
    ///
    /// When `q` is not one of the five bits.
    pub fn read(&self, q: u64) -> Result<bool, String> {
        Ok(self.bits[index(q)?])
    }

    /// The sum of ln p over the draws made: `ln flip` for each flip,
    /// `ln (1 - flip)` for each touch that did not flip.
    #[must_use]
    pub fn log_p(&self) -> f64 {
        self.log_p
    }

    fn bit(&mut self, q: u64) -> Result<&mut bool, String> {
        Ok(&mut self.bits[index(q)?])
    }

    fn noise(&mut self, q: u64) -> Result<(), String> {
        let flips = self.draw() < self.flip;
        *self.bit(q)? ^= flips;
        self.log_p += if flips { self.flip } else { 1.0 - self.flip }.ln();
        Ok(())
    }

    /// The next draw of the stream, uniform in `[0, 1)`: one `SplitMix64`
    /// step, its top 53 bits.
    fn draw(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss)] // 53 bits: exact in an f64
        let top = (z >> 11) as f64;
        top / (1u64 << 53) as f64
    }
}

/// Panics when `flip` is not a chance, so that a log probability is one.
fn assert_chance(flip: f64) {
    assert!(
        (0.0..=1.0).contains(&flip),
        "the flip chance {flip} is not in [0, 1]"
    );
}

/// The index of bit `q`.
///
/// # Errors
///
/// When `q` is not one of the five bits.
fn index(q: u64) -> Result<usize, String> {
    usize::try_from(q)
        .ok()
        .filter(|&i| i < QUBITS as usize)
        .ok_or_else(|| format!("qubit {q} is not one of the {QUBITS} bits"))
}

/// Runs `input`'s calls of `stim+rz` on one shot's bits: a readout per `M`.
///
/// # Errors
///
/// A call that is not of `stim+rz`, or a qubit beyond the five bits.
fn simulate(
    bits: &mut NoisyBits,
    input: &CallInput<'_, InstructionCall>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    for call in input.instructions() {
        match (call.opcode, &call.operands[..]) {
            (R, &[Operand::Block(q)]) => bits.reset(q.into())?,
            (CX, &[Operand::Block(c), Operand::Block(t)]) => bits.cx(c.into(), t.into())?,
            (M, &[Operand::Block(q), Operand::Tag(tag)]) => {
                outcomes.push(Outcome::new(tag, bits.read(q.into())?.into()));
            }
            (ROTATE_Z, &[Operand::Block(q), Operand::Real(_)]) => _ = index(q.into())?,
            _ => return Err(format!("not a call of stim+rz: {call:?}")),
        }
    }
    Ok(())
}

/// The protocol's executor of `stim+rz` (`set`, the protocol's set of it),
/// a state per shot: shot `j`'s bits draw from its seed in a run of seed
/// `seed` (`shot_seed(seed, j)`), so a shot draws alike whatever the other
/// shots. It refuses a call outside `set`, offers resumable shots, and reports
/// each shot's log probability.
///
/// # Panics
///
/// When `set` does not hold `stim+rz`'s instructions at [`R`], [`M`],
/// [`CX`] and [`ROTATE_Z`], each with its operands (`R` a block, `M` a
/// block and a tag, `CX` two blocks, `rotate_z` a block and a real), or
/// when `flip` is not a chance (below 0, above 1, or NaN). A set may hold
/// more instructions: their calls are accepted, and fail as they run, as a
/// qubit beyond the five bits does: the reply fails after the readouts
/// made before, and the executor refuses every later call
/// (`Rejected::EarlierFailure`).
#[expect(
    clippy::type_complexity,
    reason = "the facility's type, as `>>` composes it"
)]
pub fn simulator(
    set: Arc<InstructionSet>,
    seed: u64,
    flip: f64,
) -> OnePerShotExecutor<
    InstructionCall,
    NoisyBits,
    impl MakeState<NoisyBits>,
    impl ExecutorLogic<InstructionCall, NoisyBits>,
    impl FnMut(&CallInput<'_, InstructionCall>) -> Result<(), Rejected>,
    impl FnMut(&NoisyBits) -> f64,
> {
    use OperandKind::{Block, Real, Tag};
    for (mnemonic, opcode, operands) in [
        ("R", R, &[Block][..]),
        ("M", M, &[Block, Tag]),
        ("CX", CX, &[Block, Block]),
        ("rotate_z", ROTATE_Z, &[Block, Real]),
    ] {
        assert!(
            set.opcode(mnemonic) == Some(opcode) && set.operands(mnemonic) == Some(operands),
            "the set does not hold stim+rz's `{mnemonic}` at {opcode:?} on {operands:?}"
        );
    }
    assert_chance(flip);
    let promise = Capabilities::default()
        .with_qubits(QUBITS)
        .with_resumable_shots(true)
        .with_instruction_set(Some(Arc::clone(&set)));
    let check = move |input: &CallInput<'_, InstructionCall>| set.check_batch(input.instructions());
    executor_fn(
        promise,
        move |shot: ShotContext| Ok(NoisyBits::new(shot.seed(), flip)),
        simulate,
    )
    .check(check)
    .log_probability(NoisyBits::log_p)
    .seed(seed)
    .shots(OnePerShot)
}

/// The same bits, as the Qodec runtime's bottom backend of one shot: a
/// result per `M`, its one outcome the bit read. A `rotate_z` angle may be
/// a real or an integer, as the runtime's `number` arguments may.
#[derive(Clone, Debug)]
pub struct NoisyBitsBackend {
    bits: NoisyBits,
}

impl NoisyBitsBackend {
    /// Five bits at 0, drawing from `seed` (the shot's: `shot_seed(seed,
    /// j)` for the bits of [`simulator`]'s shot `j`), each touch flipping
    /// with chance `flip`.
    ///
    /// # Panics
    ///
    /// When `flip` is not a chance: below 0, above 1, or NaN.
    #[must_use]
    pub fn new(seed: u64, flip: f64) -> Self {
        Self {
            bits: NoisyBits::new(seed, flip),
        }
    }

    /// The shot's bits, as the instructions left them.
    #[must_use]
    pub fn bits(&self) -> &NoisyBits {
        &self.bits
    }
}

impl Backend<usize, OwnedParam, BasicReadout> for NoisyBitsBackend {
    /// An instruction that is not of `stim+rz`, or a qubit beyond the five
    /// bits.
    type Error = String;

    fn execute_batch(&mut self, instructions: &[Ins]) -> Result<BatchResult<BasicReadout>, String> {
        let mut results = smallvec::SmallVec::new();
        for ins in instructions {
            let opcode = u32::try_from(ins.instruction_id).map(Opcode::new);
            match (opcode, &ins.parameters[..]) {
                (Ok(R), &[OwnedParam::Block(q)]) => self.bits.reset(q)?,
                (Ok(CX), &[OwnedParam::Block(c), OwnedParam::Block(t)]) => self.bits.cx(c, t)?,
                (Ok(M), &[OwnedParam::Block(q)]) => {
                    let bit = Some(self.bits.read(q)?);
                    results.push(BasicReadout::new(&[bit], &[], Status::default()));
                }
                (
                    Ok(ROTATE_Z),
                    &[
                        OwnedParam::Block(q),
                        OwnedParam::Float(_) | OwnedParam::Int(_),
                    ],
                ) => {
                    _ = index(q)?;
                }
                _ => return Err(format!("not an instruction of stim+rz: {ins:?}")),
            }
        }
        Ok(BatchResult { results })
    }
}

//! How many device calls a lockstep merge of the shots below the Qodec
//! layers would save (Q67), measured without changing the protocol.
//!
//! Each case runs `static_driver() >> qodec >> call_counter() >> recorder()
//! >> device`: the counter gives the device calls today, and the recorder
//! each shot's sequence of bottom batches. Offline, at each step `k` (the
//! `k`-th bottom batch of each shot that has one), the shots whose batches
//! are equal, instruction for instruction and operand for operand, could go
//! in one device call if the shots ran in lockstep. So the sum over the
//! steps of the distinct batches at each step (`lockstep`) is the lower
//! bound of the device calls of an ideal lockstep merge of equal batches;
//! `today` is the calls the counter saw. It also counts the distinct
//! *shapes* (the same instructions on the same blocks, whatever their real
//! and integer parameters: `shape`), which a device that takes a parameter
//! per shot could merge; the steps at which every shot sends the same
//! batch (`uniform`); and the instructions whose calls differ between two
//! shots at a step (`differ in`).
//!
//! The devices bracket how often the shots' readouts differ:
//!
//! - `coins`: the tests' coins, each readout 0 or 1 with chance 1/2, so
//!   every syndrome, and every bit a measurement-based preparation feeds
//!   forward, is random (the worst case for merging);
//! - `p=0.01`, `p=0`: coins that read 1 with that chance, so most, or all,
//!   readouts are 0 (`p=0` is the best case);
//! - `noisy`: repetition3's noisy bits, exact for its circuits, with a
//!   flip chance of 0.01 per touch.
//!
//! On c4c6, the shots' batches differ in the bits that the
//! measurement-based preparation feeds forward (`X_if`, `Z_if`): the
//! readouts of its ancillas, which are random even on a noiseless device
//! (its gadgets' comments). So `coins` is the realistic row there, and
//! `p=0.01` and `p=0` only bound it.
//!
//! It first checks, on 8 shots of c422 and c4c6 over the coins, that the
//! recorder sees each shot send the Qodec runtime's own stack's bottom
//! batches, and the chain gives the stack's outcomes; and on every case,
//! that the recorder saw at least as many batches as the counter counted
//! calls (a call holds at least one batch).
//!
//! `cargo run --release -p qodec_demo --example lockstep [shots...]`
//! (by default 4, 16, 64 and 256 shots).

#[path = "../tests/common/mod.rs"]
mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use executor_protocol::{
    CallInput, Capabilities, InstructionCall, InstructionSet, Needs, OnePerShot, Operand, Outcome,
    Outcomes, ShotContext, ShotId, ShotOutcome, ShotSet, executor_fn, static_driver,
};
use qodec::Qodec;
use qodec_demo::call_counter::{Calls, call_counter};
use qodec_demo::coin::coin_device;
use qodec_demo::noisy_bits::simulator;
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::recorder::{Sent, recorder};
use qodec_demo::stack::on_the_stack;

use common::{
    FLIP, SEED, bottom, c4c6, c4c6_program, c422, c422_program, repetition3, repetition3_program,
    sorted,
};

/// One example: its qodec, how many Qodec layers the chain stacks, and its
/// program.
struct Case {
    name: &'static str,
    qodec: Qodec,
    layers: usize,
    program: Program,
}

/// The device below the layers.
#[derive(Clone, Copy)]
enum Device {
    /// The tests' coins ([`coin_device`]).
    Coins,
    /// Coins that read 1 with this chance ([`biased_coins`]).
    Biased(f64),
    /// Repetition3's noisy bits ([`simulator`]), flip chance [`FLIP`].
    Noisy,
}

impl Device {
    fn name(self) -> String {
        match self {
            Device::Coins => "coins".to_owned(),
            Device::Biased(p) => format!("p={p}"),
            Device::Noisy => format!("noisy {FLIP}"),
        }
    }
}

/// One shot's biased coins: its seed and the readouts drawn.
#[derive(Clone)]
struct Biased {
    seed: u64,
    drawn: u64,
    p: f64,
}

/// The `n`-th draw of the shot whose seed is `seed`, uniform in `[0, 1)`:
/// one splitmix64 step, its top 53 bits.
fn uniform(seed: u64, n: u64) -> f64 {
    let mut z = (seed ^ n).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    #[expect(clippy::cast_precision_loss, reason = "53 bits: exact in an f64")]
    let top = (z >> 11) as f64;
    top / (1u64 << 53) as f64
}

/// A biased coin per tag, in call order.
fn toss_biased(
    coins: &mut Biased,
    input: &CallInput<'_, InstructionCall>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    for call in input.instructions() {
        for operand in call.operands.iter() {
            if let Operand::Tag(tag) = *operand {
                let one = uniform(coins.seed, coins.drawn) < coins.p;
                coins.drawn += 1;
                outcomes.push(Outcome::new(tag, one.into()));
            }
        }
    }
    Ok(())
}

/// Coins of `set` that read 1 with chance `p`, a state per shot, each shot
/// drawing from its seed in a run of seed `seed`.
fn biased_coins(
    set: Arc<InstructionSet>,
    seed: u64,
    p: f64,
) -> executor_protocol::OnePerShotExecutor<
    InstructionCall,
    Biased,
    impl executor_protocol::MakeState<Biased>,
    impl executor_protocol::ExecutorLogic<InstructionCall, Biased>,
> {
    let promise = Capabilities::default()
        .with_resumable_shots(true)
        .with_instruction_set(Some(set));
    let make = move |shot: ShotContext| {
        Ok(Biased {
            seed: shot.seed(),
            drawn: 0,
            p,
        })
    };
    executor_fn(promise, make, toss_biased)
        .seed(seed)
        .shots(OnePerShot)
}

/// The chain of `case`'s layers over `$device`, counted and recorded: its
/// outcomes, the device calls, and each shot's bottom batches.
macro_rules! measured {
    ($case:expr, $shots:expr, $device:expr) => {{
        let case: &Case = $case;
        let layer = |k| {
            QodecLayer::new(&case.qodec, k)
                .expect("the qodec plans")
                .adapter()
        };
        let program = case.program.to_static(Needs::default());
        let (outcomes, calls, sent): (_, Calls, Sent) = match case.layers {
            1 => {
                let chain =
                    (static_driver() >> layer(0) >> call_counter() >> recorder() >> $device)
                        .expect("every boundary matches");
                let (outcomes, (calls, (sent, _))) = chain
                    .run(program, $shots.clone())
                    .expect("the job fits the chain")
                    .wait();
                (outcomes, calls, sent)
            }
            2 => {
                let chain = (static_driver()
                    >> layer(0)
                    >> layer(1)
                    >> call_counter()
                    >> recorder()
                    >> $device)
                    .expect("every boundary matches");
                let (outcomes, (calls, (sent, _))) = chain
                    .run(program, $shots.clone())
                    .expect("the job fits the chain")
                    .wait();
                (outcomes, calls, sent)
            }
            layers => panic!("{}: {layers} layers", case.name),
        };
        (outcomes.expect("the run succeeds"), calls, sent)
    }};
}

/// `case` over `device` on `shots`: its outcomes, the device calls, and
/// each shot's bottom batches.
fn run(case: &Case, device: Device, shots: &ShotSet) -> (Vec<ShotOutcome>, Calls, Sent) {
    let set = || bottom(&case.qodec).set.clone();
    match device {
        Device::Coins => measured!(case, shots, coin_device(set(), SEED)),
        Device::Biased(p) => measured!(case, shots, biased_coins(set(), SEED, p)),
        Device::Noisy => measured!(case, shots, simulator(set(), SEED, FLIP)),
    }
}

/// A batch as a key: each call's opcode and operands, reals by their bits.
/// With `shape`, only the opcodes and the blocks.
fn key(batch: &[InstructionCall], shape: bool) -> Vec<(usize, Vec<(u8, u64)>)> {
    let operand = |o: &Operand| match *o {
        Operand::Block(b) => Some((0, u64::from(b))),
        Operand::Tag(t) => (!shape).then_some((1, u64::from(t))),
        Operand::Real(r) => (!shape).then_some((2, r.to_bits())),
        #[expect(clippy::cast_sign_loss, reason = "a key: the bits as they are")]
        Operand::Integer(i) => (!shape).then_some((3, i as u64)),
        ref other => unreachable!("a kind no Qodec set declares: {other:?}"),
    };
    (batch.iter())
        .map(|call| {
            let operands = call.operands.iter().filter_map(operand).collect();
            (call.opcode.index(), operands)
        })
        .collect()
}

/// What the shots' sequences of bottom batches allow.
#[derive(Default)]
struct Lockstep {
    /// The bottom batches, over the shots.
    batches: usize,
    /// The steps: the longest shot's batches.
    steps: usize,
    /// The fewest batches of a shot.
    shortest: usize,
    /// The distinct batches at each step, summed: the device calls of an
    /// ideal lockstep merge of equal batches.
    exact: usize,
    /// The distinct shapes at each step, summed.
    shape: usize,
    /// The steps at which every shot that has one sends the same batch.
    uniform_steps: usize,
    /// The instructions whose calls differ between two shots at a step.
    differ: BTreeSet<String>,
}

fn lockstep(sent: &Sent, set: &InstructionSet) -> Lockstep {
    let shots: Vec<_> = sent.0.values().collect();
    let steps = shots.iter().map(|s| s.len()).max().unwrap_or(0);
    let mut out = Lockstep {
        steps,
        shortest: shots.iter().map(|s| s.len()).min().unwrap_or(0),
        ..Lockstep::default()
    };
    for k in 0..steps {
        let at: Vec<_> = shots.iter().filter_map(|s| s.get(k)).collect();
        out.batches += at.len();
        let exact: BTreeSet<_> = at.iter().map(|b| key(b, false)).collect();
        let shape: BTreeSet<_> = at.iter().map(|b| key(b, true)).collect();
        for batch in &at[1..] {
            if batch.len() != at[0].len() {
                out.differ.insert("(length)".to_owned());
            }
            for (a, b) in at[0].iter().zip(batch.iter()).filter(|(a, b)| a != b) {
                for opcode in [a.opcode, b.opcode] {
                    let name = set.mnemonic(opcode).expect("an opcode of the set");
                    out.differ.insert(name.to_owned());
                }
            }
        }
        out.exact += exact.len();
        out.shape += shape.len();
        out.uniform_steps += usize::from(exact.len() == 1);
    }
    out
}

fn main() {
    let counts: Vec<ShotId> = std::env::args()
        .skip(1)
        .map(|s| s.parse().expect("a number of shots"))
        .collect();
    let counts = if counts.is_empty() {
        vec![4, 16, 64, 256]
    } else {
        counts
    };
    let case = |name, qodec: Qodec, layers, program: fn(&Qodec) -> Program| {
        let program = program(&qodec);
        Case {
            name,
            qodec,
            layers,
            program,
        }
    };
    let cases = [
        (
            case("c422", c422(), 1, c422_program),
            vec![Device::Coins, Device::Biased(0.01), Device::Biased(0.0)],
        ),
        (
            case("c4c6", c4c6(), 2, c4c6_program),
            vec![Device::Coins, Device::Biased(0.01), Device::Biased(0.0)],
        ),
        (
            case("rep3", repetition3(), 1, repetition3_program),
            vec![Device::Coins, Device::Noisy],
        ),
    ];

    // The recorder sees what the stack sends, and the chain gives the
    // stack's outcomes.
    let few = ShotSet::range(0..=7).expect("not empty");
    for (case, _) in &cases[..2] {
        let stack = on_the_stack(&case.qodec, &case.program, &few, SEED).expect("the stack runs");
        let (outcomes, _, sent) = run(case, Device::Coins, &few);
        let expected = (stack.backends.iter())
            .map(|(shot, coins)| (*shot, coins.sent().to_vec()))
            .collect();
        assert_eq!(sent, Sent(expected), "{}: the batches differ", case.name);
        assert_eq!(
            sorted(outcomes),
            sorted(stack.outcomes),
            "{}: the outcomes differ",
            case.name
        );
    }

    println!(
        "{:<6} {:<10} {:>5} {:>7} {:>8} {:>7} {:>8} {:>6} {:>7} {:>10}  differ in",
        "code",
        "device",
        "shots",
        "steps",
        "batches",
        "today",
        "lockstep",
        "shape",
        "uniform",
        "today/lock",
    );
    for (case, devices) in &cases {
        let set = bottom(&case.qodec).set.clone();
        for &device in devices {
            for &n in &counts {
                let shots = ShotSet::range(0..=n - 1).expect("not empty");
                let (_, calls, sent) = run(case, device, &shots);
                let l = lockstep(&sent, &set);
                let calls = usize::try_from(calls.0).expect("a count");
                assert!(l.exact <= calls, "{}: no merge beats today", case.name);
                assert!(l.batches >= calls, "{}: a call holds a batch", case.name);
                assert!(l.shape <= l.exact, "{}: shapes merge more", case.name);
                #[expect(clippy::cast_precision_loss, reason = "a ratio of counts")]
                let ratio = calls as f64 / l.exact as f64;
                let differ: Vec<_> = l.differ.into_iter().collect();
                println!(
                    "{:<6} {:<10} {n:>5} {:>3}-{:<3} {:>8} {calls:>7} {:>8} {:>6} {:>7} {ratio:>10.2}  {}",
                    case.name,
                    device.name(),
                    l.shortest,
                    l.steps,
                    l.batches,
                    l.exact,
                    l.shape,
                    l.uniform_steps,
                    differ.join(" "),
                );
            }
        }
    }
}

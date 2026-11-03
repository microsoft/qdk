//! The Qodec runtime (`qodec_runtime`, unchanged) as layers of
//! `executor_protocol` chains: a test bed of the protocol on a real code
//! runtime, whose chains are compared with the runtime's own stack.
//!
//! The chain it is built for is a program driven down a Qodec layer, a
//! profiler counting the physical instructions, and a device:
//! `static_driver() >> qodec >> profiler() >> device`. Its parts:
//!
//! - [`qodec_layer`]: a layer of a qodec as an adapter, a state per shot;
//! - [`profiler`]: an adapter counting batches and instructions, shot by
//!   shot;
//! - [`call_counter`]: an adapter counting the calls below, whole;
//! - [`recorder`]: an adapter keeping each shot's batches, in order;
//! - [`coin`] and [`noisy_bits`]: devices, each also as the Qodec runtime's
//!   bottom backend, so that a chain and the runtime's stack run on the
//!   same bits;
//! - [`answers_later`]: an executor whose replies come at a later poll;
//! - [`answers_on_yield`]: an executor whose replies come once the run
//!   above yields, from another thread;
//! - [`program`] and [`stack`]: a program in the Qodec REPL's syntax, and
//!   the runtime's own stack, which runs it as the reference;
//! - [`boundary`]: a Qodec ISA as the protocol's set, and the conversions
//!   of single instructions;
//! - [`stim`]: a parser of the Stim subset that gadgets are written in.
//!
//! Three examples measure the chain: `throughput` times it, with and
//! without the profiler, against the runtime's own stack; `lockstep`
//! counts the device calls a lockstep merge of the shots would save (Q67);
//! `latency` measures what the layer's blocking costs an async caller
//! over a device with a latency (Q65).
//!
//! The tests load repetition3 from the `qdk-ec` checkout that the
//! workspace's `[patch.crates-io]` builds the `qodec` library from, and
//! c422 and c4c6 from the lowerer's examples
//! (`source/qodec_lowerer/examples`).
//!
//! The ids QD1 to QD6, Q65 and Q67 to Q69 in the docs name design
//! questions, each open but for QD5's carrier, which is built (the
//! protocol's `ShotConditions`): the README's "Open questions" lists them.

pub mod answers_later;
pub mod answers_on_yield;
pub mod boundary;
pub mod call_counter;
pub mod coin;
pub mod noisy_bits;
pub mod profiler;
pub mod program;
pub mod qodec_layer;
pub mod recorder;
pub mod stack;
pub mod stim;

#[allow(dead_code, clippy::all, clippy::pedantic)]
#[path = "../../qodec_lowerer/src/style.rs"]
mod style;
#[allow(dead_code, clippy::all, clippy::pedantic)]
#[path = "../../qodec_lowerer/src/syntax.rs"]
mod syntax;

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The execution protocol: how a caller hands quantum work to an executor
//! and reads its reply, the same way whatever runs the work (a simulator, an
//! adapter's executor that runs a program's classical part, a remote
//! device).
//!
//! An executor accepts one kind of [`Message`] and replies with a
//! [`Reply`] that delivers the message kind's events: [`Executor::call`]
//! returns the reply, or a typed [`Rejected`] when it refuses the call.
//! A run is three kinds of components, composed:
//!
//! ```text
//! Driver >> Adapter >> ... >> Executor
//! ```
//!
//! - A driver heads the chain and calls the executor it is composed with.
//!   The crate's interpreter, [`interpreter()`](fn@interpreter), an
//!   interpreter of programs that are data ([`Interpret`]), runs a
//!   [`StaticProgram`] or any [`Drive`] program: composed with an executor,
//!   it gives a [`Chain`], built and checked, and a program and its shots
//!   are a job, the chain's input, not one of its components:
//!   `chain.run(program, shots)` checks the job ([`JobFaults`]) and gives
//!   its [`Run`].
//! - The executor at the end of the chain ([`Executor`]) is an executor of
//!   [`InstructionBatch`]es, each a list of instructions run on some of the
//!   run's shots, whose states carry over from one batch to the next. It is
//!   built with its seed, and each shot `j` draws from seeds that depend only
//!   on that seed, `j` and the component that draws ([`ShotContext`],
//!   [`shot_seed`]), not on how the shots are split. So a replayable executor
//!   ([`Capabilities::replayable`]) gives each shot the same outcomes however
//!   the shots are split, as long as the shot gets the same instructions. A
//!   per-shot simulator holds one state per shot, made at the shot's first call
//!   from the shot's seeds.
//! - An adapter, between them, is a map from an executor to an executor,
//!   which may hold state ([`Adapter`], [`Apply`]): composed with an
//!   executor, it gives an executor, whose calls pass through it, possibly
//!   as another instruction type. An adapter is not an executor.
//!   [`adapter_fn`](fn@adapter_fn) makes one from a state and a logic, and
//!   [`owning_adapter_fn`](fn@owning_adapter_fn) one whose state keeps its
//!   way below for its whole life.
//!
//! [`compose`](fn@crate::compose), or `>>`, puts them together. Each adapter
//! is built once against what is downstream of it, every fault of the parts
//! built together reported together ([`BuildFaults`]); a part composed with
//! the executor first is built on its own, before what is upstream of it. A
//! job's needs are checked against what the chain offers when it is given
//! to the chain, every fault reported together before anything runs
//! ([`JobFaults`]). A [`Run`] runs nothing until
//! waited for ([`Run::wait`]) or awaited: it then drives the executor and
//! closes it once its program ends, giving the program's output and the
//! executor's report.
//!
//! An executor may run work synchronously before returning. An executor's
//! refusal means that no work started; a rejection of class
//! [`RetryClass::Resend`] (such as [`Rejected::NoReply`]) instead means that no
//! verdict arrived over a link, so work may have run. A run that starts and
//! cannot finish ends its reply with [`ReplyStatus::Failed`] instead, and, when
//! its executor's state carries over from one call to the next (a shot's
//! state, or the states of its shots), fails that executor, which refuses
//! every later call ([`Rejected::EarlierFailure`]).
//! Every executor keeps that rule, and its refusals once closed, whatever
//! keeps them for it: an [`ExecutorLifecycle`], or
//! [`executor_fn`](fn@executor_fn), which keeps them, for an executor with
//! state; the executor downstream, for one that passes its calls on (an
//! adapter's executor); a flag that it is closed, for one with no state
//! between calls. A wrapper never forgets a refusal: an executor that wraps
//! another, or is made of one (placed, or given a state per shot), keeps
//! refusing calls once the other is closed or failed, as it continues what
//! it wraps.
//! Polling a reply never blocks. The reply wakes its caller when it can make
//! progress; [`wait`] and [`wait_async`] take a whole reply, [`async_events`]
//! its events as they come, and a reply made elsewhere comes from
//! [`reply_channel()`](reply_channel). What a component offers is described by
//! [`Describe`], and a component that keeps totals returns them as a composable
//! report from [`Close`]. An executor may describe the set of instructions it
//! accepts as an [`InstructionSet`], whatever their type, so that a chain
//! checks what is sent against it: an adapter's calls when the chain is
//! built, a program's when its job is given to the chain; a set known only
//! at run time is an [`InstructionBatch`] of [`InstructionCall`]s.
//!
//! The crate depends on `std` and `futures-core` (for [`AsyncEvents`]'s
//! `Stream`) only, so that every part of qdk can use it. The crate's
//! `README.md` explains the model and its choices.

mod absorb;
mod adapter;
mod capabilities;
mod codes;
mod compose;
mod conditions;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
mod driver;
mod executor;
mod executor_fn;
mod instruction_batch;
mod instruction_set;
mod lifecycle;
mod message;
mod needs;
mod outcome;
mod placement;
mod rejected;
mod reply;
mod seeds;
mod shot;
mod shot_states;

pub use adapter::adapter_fn::{
    AdapterFn, AdapterFnExecutor, AdapterFnReply, AdapterLogic, NoStateReport, OnePerShotAdapter,
    OnePerShotAdapterExecutor, Owning, OwningAdapterLogic, adapter_fn, owning_adapter_fn,
};
pub use adapter::below::{AdapterError, Below, BelowHandle};
pub use adapter::boundary::AdapterBoundary;
pub use adapter::{Adapter, Apply};
pub use capabilities::{Capabilities, Describe};
pub use codes::{
    conditions_code, conditions_of, need_code, need_of, reason_code, retry_code, retry_of,
    value_code, value_of,
};
pub use compose::adapters::{AdapterExecutor, ComposedAdapters};
pub(crate) use compose::adapters::{Adapters, NoAdapter, UpstreamOf};
pub use compose::chain::{Chain, JobFaults};
pub use compose::run::Run;
pub use compose::{BuildError, BuildFault, BuildFaults, Compose, WithExecutor, compose};
pub use conditions::ShotConditions;
pub use driver::Drive;
#[cfg(test)]
pub(crate) use driver::Driver;
pub use driver::interpreter::{
    DefaultInterpreter, Interpret, Interpreted, InterpreterDriver, interpreter,
};
pub use driver::static_program::{StaticProgram, StaticProgramError};
pub use executor::{Close, Executor, ExecutorReport, Fold, LogP};
pub use executor_fn::{
    ExecutorFn, ExecutorLogic, MakeState, NoCheck, OnePerShot, OnePerShotExecutor,
    OnePerShotPlacedExecutor, Outcomes, ProgramInput, ShotStrategy, executor_fn,
};
pub use instruction_batch::{InstructionBatch, InstructionBatchRef};
pub use instruction_set::action::{Action, Angle, Pauli};
pub use instruction_set::call::{CallDisplay, InstructionCall, Opcode, Operand, Operands};
pub use instruction_set::instruction::{Instruction, TypedInstruction};
pub use instruction_set::{
    InstructionMismatch, InstructionSet, OpcodeMap, OpcodeTable, OperandKind, Renumbering,
};
pub use lifecycle::{ExecutorLifecycle, LifecycleBoundReply};
pub use message::Message;
pub use needs::{Need, Needs, UnmetNeed};
pub use outcome::{ErrorKind, Outcome, ShotOutcome, Value, check_unique_tags};
pub use placement::{OwnThread, PlacedExecutor, Placement, PlacementJob};
pub use rejected::{CallId, Rejected, RetryClass};
pub use reply::asynchronous::{AsyncEvents, WaitAsync, async_events, wait_async};
pub use reply::{
    ImmediateReply, IncrementalReply, MappedReply, Reply, ReplySender, ReplyStatus, relay,
    reply_channel, wait,
};
pub use seeds::{AdapterSeeds, ComponentId, RngIds, ShotContext, shot_seed};
pub use shot::{IntoShotSet, ShotId, ShotSet};
pub use shot_states::ShotReplies;
pub(crate) use shot_states::ShotStates;

#[cfg(test)]
mod tests;

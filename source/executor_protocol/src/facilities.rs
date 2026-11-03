// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The facilities: the protocol is the traits, data and composition every
//! implementer meets; the facilities are the crate's own implementations of
//! them, which make a part from the user's logic
//! ([`executor_fn`](fn@crate::executor_fn) from an [`ExecutorLogic`],
//! [`adapter_fn`](fn@crate::adapter_fn) from an [`AdapterLogic`],
//! [`driver()`](fn@crate::driver) running an [`AdaptiveProgram`] or a
//! [`StaticProgram`]) or keep a protocol rule for a part written by hand.
//! [`driver()`](fn@crate::driver) is the facility most chains start from.
//!
//! Make a part from your logic:
//!
//! - [`executor_fn`](mod@executor_fn): makes an executor from a state and
//!   an [`ExecutorLogic`] ([`ExecutorFn`]), with one state or a state per
//!   shot.
//! - [`adapter_fn`](mod@adapter_fn): makes an adapter from a state and an
//!   [`AdapterLogic`] ([`AdapterFn`]), with one state or a state per shot.
//! - [`builtin_driver`]: the crate's driver, which runs a [`StaticProgram`]
//!   and any [`AdaptiveProgram`] ([`BuiltinDriver`], from
//!   [`driver()`](fn@crate::driver)).
//! - [`static_program`]: a program of fixed batches, and how the crate's
//!   driver runs it ([`StaticProgram`]).
//!
//! Keep a rule for a hand-written part:
//!
//! - [`executor_lifecycle`]: keeps an executor's rules on failures and
//!   close, for one written by hand ([`ExecutorLifecycle`]).
//! - [`shot_states`]: keeps the rules of an executor that runs each shot on
//!   a state of its own, and the reply to a batch run on them
//!   ([`ShotReplies`]).
//! - [`placement`]: where an executor runs ([`Placement`]): on a thread of
//!   its own ([`OwnThread`]), or wherever a closure puts its work.
//! - [`placed_executor`]: runs an executor where a [`Placement`] puts its
//!   work ([`PlacedExecutor`], made by `.run_on(place)`).
//! - [`readout_tags`]: checks that within a batch a tag names one readout
//!   of each shot ([`check_unique_tags`]).
//! - [`replies`]: makes an executor's reply: whole inside the call
//!   ([`ImmediateReply`]), pushed from elsewhere ([`reply_channel`]),
//!   relayed from a worker ([`relay`]), or converted ([`MappedReply`]).
//!
//! Crate-internal helpers:
//!
//! - [`panics`]: runs code in a catch, and raises its panic again unless
//!   another unwinds.
//! - [`teardown`]: how a facility's executor holds its parts and frees them
//!   when dropped or closed.

pub(crate) mod adapter_fn;
pub(crate) mod builtin_driver;
pub(crate) mod executor_fn;
pub(crate) mod executor_lifecycle;
pub(crate) mod panics;
pub(crate) mod placed_executor;
pub(crate) mod placement;
pub(crate) mod readout_tags;
pub(crate) mod replies;
pub(crate) mod shot_states;
pub(crate) mod static_program;
pub(crate) mod teardown;

#[cfg(doc)]
use crate::{
    AdapterFn, AdapterLogic, AdaptiveProgram, BuiltinDriver, ExecutorFn, ExecutorLifecycle,
    ExecutorLogic, ImmediateReply, MappedReply, OwnThread, PlacedExecutor, Placement, ShotReplies,
    StaticProgram, check_unique_tags, relay, reply_channel,
};

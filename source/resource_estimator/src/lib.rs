// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A Q# backend to compute logical overheads to compute the overhead based on
//! the PSSPC layout method.

// This crate uses lots of conversions between floating point numbers and integers, so this helps
// avoid many needed allow statements. Comment out individual lines to see where they are needed.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless
)]

pub mod counts;
/// Provides traits to define a fault-tolerant quantum computing architecture
/// and functions to perform resource estimation on such architectures.
pub mod estimates;
/// Models a fault-tolerant quantum computing architecture based on
/// customizaable gate-based and Majorana qubits, planar codes, and T-factories.
pub mod system;

pub use system::estimate_physical_resources_from_json;

use counts::LogicalCounter;
use miette::Diagnostic;
use qsc::interpret::{self, GenericReceiver, Interpreter, Value};
use system::estimate_physical_resources;
use thiserror::Error;

use crate::system::LogicalResourceCounts;

#[derive(Debug, Diagnostic, Error)]
#[error(transparent)]
#[diagnostic(transparent)]
pub enum Error {
    Interpreter(interpret::Error),
    Estimation(system::Error),
}

pub fn estimate_entry(interpreter: &mut Interpreter, params: &str) -> Result<String, Vec<Error>> {
    let mut counter = LogicalCounter::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);
    interpreter
        .eval_entry_with_sim(&mut counter, &mut out)
        .map_err(|e| e.into_iter().map(Error::Interpreter).collect::<Vec<_>>())?;
    estimate_physical_resources(counter.logical_resources(), params)
        .map_err(|e| vec![Error::Estimation(e)])
}

pub fn estimate_expr(
    interpreter: &mut Interpreter,
    expr: &str,
    params: &str,
) -> Result<String, Vec<Error>> {
    let mut counter = LogicalCounter::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);
    interpreter
        .run_with_sim(&mut counter, &mut out, Some(expr))
        .map_err(|e| e.into_iter().map(Error::Interpreter).collect::<Vec<_>>())?;
    estimate_physical_resources(counter.logical_resources(), params)
        .map_err(|e| vec![Error::Estimation(e)])
}

pub fn estimate_call(
    interpreter: &mut Interpreter,
    callable: Value,
    args: Value,
    params: &str,
) -> Result<String, Vec<Error>> {
    let mut counter = LogicalCounter::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);
    interpreter
        .invoke_with_sim(&mut counter, &mut out, callable, args)
        .map_err(|e| e.into_iter().map(Error::Interpreter).collect::<Vec<_>>())?;
    estimate_physical_resources(counter.logical_resources(), params)
        .map_err(|e| vec![Error::Estimation(e)])
}

pub fn logical_counts_expr(
    interpreter: &mut Interpreter,
    expr: &str,
) -> Result<LogicalResourceCounts, Vec<Error>> {
    let mut counter = LogicalCounter::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);
    interpreter
        .run_with_sim(&mut counter, &mut out, Some(expr))
        .map_err(|e| e.into_iter().map(Error::Interpreter).collect::<Vec<_>>())?;
    Ok(counter.logical_resources())
}

pub fn logical_counts_call(
    interpreter: &mut Interpreter,
    callable: Value,
    args: Value,
) -> Result<LogicalResourceCounts, Vec<Error>> {
    let mut counter = LogicalCounter::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);
    interpreter
        .invoke_with_sim(&mut counter, &mut out, callable, args)
        .map_err(|e| e.into_iter().map(Error::Interpreter).collect::<Vec<_>>())?;
    Ok(counter.logical_resources())
}

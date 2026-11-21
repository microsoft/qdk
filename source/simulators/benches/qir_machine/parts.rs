// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The simulators `run_shot` runs on here, each doing no quantum work and
//! measuring `Zero`, as the machine alone is answered: [`NoOpSimulator`],
//! timed, and [`LoggingSimulator`], which logs the calls it gets, so that
//! `assert_same_work` compares them with the machine's.

mod logging_simulator;
mod no_op_simulator;

pub(crate) use logging_simulator::LoggingSimulator;
pub(crate) use no_op_simulator::NoOpSimulator;

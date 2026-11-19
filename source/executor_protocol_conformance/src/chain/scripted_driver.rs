// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ScriptedDriver`]: the driver above an executor or an adapter under test,
//! which sends the check's script, one `execute` after another, ends each
//! shot after its last, and fails the run where the check says.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use executor_protocol::{
    Batch, Drive, DriveError, DriverSetup, Execute, Failure, Instruction, InstructionSet, Needs,
    OpcodeMap, Readouts, Seed, ShotId, ShotSet,
};

use crate::script::ScriptBatch;
use crate::shared_run::SharedRun;

/// Sends the current run's script below, each batch checked unless the
/// setup says otherwise, with its readout slots as its set counts them, and
/// ends each shot below after its last `execute` in the script, as a driver
/// freeing a shot's state early may; gives no output of its own (one `()`
/// per shot). The runner then ends every shot of the run again, including
/// the shot the script never names, which this driver never sends, where
/// the run holds one ([`Script`](crate::Script) says when), so that a run
/// tells the part to end shots it no longer holds, or never held.
pub(crate) struct ScriptedDriver {
    run: SharedRun,
    sends: InstructionSet,
    needs: Needs,
    below: Option<OpcodeMap>,
}

impl ScriptedDriver {
    /// A driver that sends instructions of `sends`, needing `needs`, as
    /// `run` sets each run up.
    pub(crate) fn new(run: SharedRun, sends: InstructionSet, needs: Needs) -> Self {
        Self {
            run,
            sends,
            needs,
            below: None,
        }
    }
}

impl DriverSetup for ScriptedDriver {
    fn sends(&self) -> &InstructionSet {
        &self.sends
    }

    fn needs(&self) -> Needs {
        self.needs.clone()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }
}

impl Drive for ScriptedDriver {
    type Output = ();

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<()>, DriveError> {
        self.run
            .write(|log| log.seed_from_runner = Some(seed.shot(ShotId::new(0))));
        let setup = self.run.setup();
        let map = self.below.as_ref().expect("`>>` binds before any run");
        let mut readouts = Readouts::new();
        let ends = ends_by_execute(setup.script.executes());
        for (index, batches) in setup.script.executes().iter().enumerate() {
            let sent: Vec<(&ShotSet, Vec<Instruction>, u32)> = batches
                .iter()
                .map(|(shots, instructions)| {
                    let readout_slots = self
                        .sends
                        .readout_slots_of(instructions)
                        .expect("the check refuses a script outside the set");
                    (shots, mapped(map, instructions), readout_slots)
                })
                .collect();
            let batches: Vec<Batch<'_>> = sent
                .iter()
                .map(|(shots, instructions, slots)| {
                    Batch::new(shots, instructions, *slots).checked_if(!setup.unchecked)
                })
                .collect();
            below
                .execute(&batches, seed, readouts.readouts_for(&batches))
                .await?;
            if let Some(ended) = &ends[index] {
                below.end_shots(ended);
            }
            if setup.fails_after == Some(index) {
                return Err(DriveError {
                    shot: shots.iter().next().expect("a shot set is never empty"),
                    failure: Failure::Part(Box::new(StoppedByTheKit)),
                });
            }
        }
        Ok(vec![(); shots.len()])
    }
}

/// For each `execute` of `executes`, the shots it is the last of, if any.
fn ends_by_execute(executes: &[Vec<ScriptBatch>]) -> Vec<Option<ShotSet>> {
    let mut last = BTreeMap::new();
    for (index, batches) in executes.iter().enumerate() {
        for shot in batches.iter().flat_map(|(shots, _)| shots.iter()) {
            last.insert(shot, index);
        }
    }
    let mut ends = vec![Vec::new(); executes.len()];
    for (shot, index) in last {
        ends[index].push(shot);
    }
    ends.into_iter()
        .map(|shots| ShotSet::try_ids(shots.into_iter().map(ShotId::get)).ok())
        .collect()
}

/// `instructions`, rewritten in the receiver's opcodes.
fn mapped(map: &OpcodeMap, instructions: &[Instruction]) -> Vec<Instruction> {
    instructions
        .iter()
        .map(|instruction| {
            let mut mapped = instruction.clone();
            mapped.opcode = map.map(instruction.opcode);
            mapped
        })
        .collect()
}

/// Why the kit's driver failed a run on purpose.
#[derive(Debug)]
struct StoppedByTheKit;

impl fmt::Display for StoppedByTheKit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the conformance kit's driver stopped the run on purpose")
    }
}

impl Error for StoppedByTheKit {}

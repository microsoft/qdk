// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    noise_config::{CumulativeNoiseConfig, CumulativeNoiseTable, Fault, NoiseConfig},
    shader_types::{Op, OpID, ops},
};

pub struct SequenceBuilder<'a> {
    ops: Vec<Op>,
    /// The noise configuration for the simulation.
    noise: CumulativeNoiseConfig,
    /// A vector storing whether a qubit was lost or not.
    loss: Vec<bool>,
    /// The last time each qubit was operated upon.
    last_operation_time: Vec<u32>,
    /// Current simulation time.
    time: u32,
    rng: &'a mut StdRng,
}

use rand::{Rng, rngs::StdRng};

#[allow(clippy::too_many_lines)]
pub fn apply_per_gate_noise(
    ops: Vec<Op>,
    num_qubits: u32,
    rng: &mut StdRng,
    noise: NoiseConfig,
) -> Vec<Op> {
    let mut builder = SequenceBuilder::new(num_qubits, ops.len(), rng, noise);
    let mut ops = ops;
    for op in ops.drain(..) {
        builder.append(op);
    }

    builder.take_ops()
}

impl<'a> SequenceBuilder<'a> {
    #[must_use]
    pub fn new(num_qubits: u32, num_ops: usize, rng: &'a mut StdRng, noise: NoiseConfig) -> Self {
        Self {
            ops: Vec::with_capacity(num_ops),
            noise: noise.into(),
            loss: vec![false; num_qubits as usize],
            last_operation_time: vec![0; num_qubits as usize],
            time: 0,
            rng,
        }
    }

    /// Increment the simulation time by one.
    /// This is used to compute the idle noise on qubits.
    pub fn step(&mut self) {
        self.time += 1;
    }

    fn append(&mut self, op: Op) {
        match TryInto::<OpID>::try_into(op.id).expect("invalid op id") {
            OpID::Id => self.apply_1q_gate(op, self.noise.i),
            OpID::Reset => unimplemented!("reset"), // do we want reset noise?
            OpID::X => self.apply_1q_gate(op, self.noise.x),
            OpID::Y => self.apply_1q_gate(op, self.noise.y),
            OpID::Z => self.apply_1q_gate(op, self.noise.z),
            OpID::H => self.apply_1q_gate(op, self.noise.h),
            OpID::S => self.apply_1q_gate(op, self.noise.s),
            OpID::SAdj => self.apply_1q_gate(op, self.noise.s_adj),
            OpID::T => self.apply_1q_gate(op, self.noise.t),
            OpID::TAdj => self.apply_1q_gate(op, self.noise.t_adj),
            OpID::Sx => self.apply_1q_gate(op, self.noise.sx),
            OpID::SxAdj => self.apply_1q_gate(op, self.noise.sx_adj),
            OpID::Rx => self.apply_1q_gate(op, self.noise.rx),
            OpID::Ry => self.apply_1q_gate(op, self.noise.ry),
            OpID::Rz => self.apply_1q_gate(op, self.noise.rz),
            OpID::Cx => self.apply_1q_gate(op, self.noise.cx),
            OpID::Cz => self.apply_1q_gate(op, self.noise.cz),
            OpID::Rxx => self.apply_2q_rot_gate(op, self.noise.rxx),
            OpID::Ryy => self.apply_2q_rot_gate(op, self.noise.ryy),
            OpID::Rzz => self.apply_2q_rot_gate(op, self.noise.rzz),
            OpID::Ccx => unimplemented!("ccx gate"),
            OpID::Mz | OpID::MResetZ => unimplemented!("measurement"),
            OpID::SAMPLE => self.ops.push(op),
            OpID::MEveryZ => {
                // not a real op
                self.ops.push(op);
            }
            OpID::Swap => self.apply_2q_gate(op, self.noise.swap),
            OpID::Matrix => {
                // guessing matrix op already has its noise built-in?
                if !self.loss[op.q1 as usize] {
                    self.apply_idle_noise(op.q1);
                    self.ops.push(op);
                }
            }
            OpID::Matrix2Q => {
                // guessing matrix op already has its noise built-in?
                if !self.loss[op.q1 as usize] && !self.loss[op.q2 as usize] {
                    self.apply_idle_noise(op.q1);
                    self.apply_idle_noise(op.q2);
                    self.ops.push(op);
                }
            }
            OpID::PauliNoise1Q => unimplemented!("Can't apply noise to a noise op"),
            OpID::PauliNoise2Q => unimplemented!("Can't apply noise to a noise op"),
            OpID::LossNoise => unimplemented!("Can't apply noise to a noise op"),
        }
    }

    fn apply_1q_gate(&mut self, op: Op, table: CumulativeNoiseTable) {
        if !self.loss[op.q1 as usize] {
            self.apply_idle_noise(op.q1);
            self.ops.push(op);
            self.apply_fault_table(table, op.q1);
        }
    }

    fn apply_2q_gate(&mut self, op: Op, table: CumulativeNoiseTable) {
        if !self.loss[op.q1 as usize] && !self.loss[op.q2 as usize] {
            self.apply_idle_noise(op.q1);
            self.apply_idle_noise(op.q2);
            self.ops.push(op);
            self.apply_fault_table(table, op.q1);
            self.apply_fault_table(table, op.q1);
        }
    }

    fn apply_2q_rot_gate(&mut self, op: Op, table: CumulativeNoiseTable) {
        match (self.loss[op.q1 as usize], self.loss[op.q2 as usize]) {
            (true, true) => {}
            (true, false) => {
                let op = match op.id {
                    ops::RXX => Op::new_rx_gate(op.angle, op.q2),
                    ops::RYY => Op::new_ry_gate(op.angle, op.q2),
                    ops::RZZ => Op::new_rz_gate(op.angle, op.q2),
                    _ => unreachable!(""),
                };
                self.apply_idle_noise(op.q1);
                self.ops.push(op);
                self.apply_fault_table(table, op.q1);
            }
            (false, true) => {
                let op = match op.id {
                    ops::RXX => Op::new_rx_gate(op.angle, op.q1),
                    ops::RYY => Op::new_ry_gate(op.angle, op.q1),
                    ops::RZZ => Op::new_rz_gate(op.angle, op.q1),
                    _ => unreachable!(""),
                };
                self.apply_idle_noise(op.q1);
                self.ops.push(op);
                self.apply_fault_table(table, op.q1);
            }
            (false, false) => {
                self.apply_idle_noise(op.q1);
                self.apply_idle_noise(op.q2);
                self.ops.push(op);
                self.apply_fault_table(table, op.q1);
                self.apply_fault_table(table, op.q1);
            }
        }
    }

    fn apply_idle_noise(&mut self, target: u32) {
        let idle_time = self.time - self.last_operation_time[target as usize];
        self.last_operation_time[target as usize] = self.time;
        let fault = self
            .noise
            .gen_idle_fault_with_sample(idle_time, self.rng.gen_range(0.0..1.0));
        self.apply_fault(fault, target);
    }

    fn apply_fault_table(&mut self, table: CumulativeNoiseTable, target: u32) {
        let fault = table.gen_operation_fault_with_samples(
            self.rng.gen_range(0.0..1.0),
            self.rng.gen_range(0.0..1.0),
        );
        self.apply_fault(fault, target);
    }

    fn apply_fault(&mut self, fault: Fault, target: u32) {
        match fault {
            Fault::None => (),
            Fault::X => self.ops.push(Op::new_x_gate(target)),
            Fault::Y => self.ops.push(Op::new_y_gate(target)),
            Fault::Z => self.ops.push(Op::new_z_gate(target)),
            Fault::S => self.ops.push(Op::new_sx_gate(target)),
            Fault::Loss => {
                //self.measure_z(target);
                self.loss[target as usize] = true;
            }
        }
    }

    fn take_ops(&mut self) -> Vec<Op> {
        self.ops.drain(..).collect()
    }
}

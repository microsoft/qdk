// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qdk_simulators::noise_config::{NoiseConfig, NoiseTable};

use crate::{
    builder::{
        GateInputs, LogicalStack, OperationListBuilder, OperationOrGroup, OperationReceiver,
        WireMap,
    },
    circuit::{GateErrorInfo, Register},
};

pub(crate) struct OperationListBuilderWithLoss<'a> {
    internal_builder: OperationListBuilder,
    not_lost_probs: Vec<f64>,
    noise_config: Option<&'a NoiseConfig<f64, f64>>,
}

impl<'a> OperationListBuilderWithLoss<'a> {
    pub(crate) fn new(
        internal_builder: OperationListBuilder,
        num_qubits: usize,
        noise_config: Option<&'a NoiseConfig<f64, f64>>,
    ) -> Self {
        Self {
            internal_builder,
            not_lost_probs: vec![1.0; num_qubits],
            noise_config,
        }
    }

    pub(crate) fn into_operations(self) -> Vec<OperationOrGroup> {
        self.internal_builder.into_operations()
    }

    fn noise_table(
        &self,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
    ) -> Option<&NoiseTable<f64>> {
        let config = self.noise_config?;
        match (
            name,
            is_adjoint,
            inputs.controls.len(),
            inputs.targets.len(),
        ) {
            ("X", false, 0, 1) => Some(&config.x),
            ("Y", false, 0, 1) => Some(&config.y),
            ("Z", false, 0, 1) => Some(&config.z),
            ("H", false, 0, 1) => Some(&config.h),
            ("S", false, 0, 1) => Some(&config.s),
            ("S", true, 0, 1) => Some(&config.s_adj),
            ("SX", false, 0, 1) => Some(&config.sx),
            ("T", false, 0, 1) => Some(&config.t),
            ("T", true, 0, 1) => Some(&config.t_adj),
            ("Rx", _, 0, 1) => Some(&config.rx),
            ("Ry", _, 0, 1) => Some(&config.ry),
            ("Rz", _, 0, 1) => Some(&config.rz),
            ("X", false, 1, 1) => Some(&config.cx),
            ("Y", false, 1, 1) => Some(&config.cy),
            ("Z", false, 1, 1) => Some(&config.cz),
            ("X", false, 2, 1) => Some(&config.ccx),
            ("Rxx", _, 0, 2) => Some(&config.rxx),
            ("Ryy", _, 0, 2) => Some(&config.ryy),
            ("Rzz", _, 0, 2) => Some(&config.rzz),
            ("SWAP", false, 0, 2) => Some(&config.swap),
            _ => None,
        }
    }
}

impl OperationReceiver for OperationListBuilderWithLoss<'_> {
    fn gate(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        args: Vec<String>,
        mut error: Option<GateErrorInfo>,
        call_stack: LogicalStack,
    ) {
        if let Some(table) = self.noise_table(name, is_adjoint, inputs) {
            let operand_losses = inputs
                .controls
                .iter()
                .chain(inputs.targets)
                .copied()
                .enumerate()
                .map(|(operand_index, qubit)| {
                    let shift = 3 * (table.qubits as usize - operand_index - 1);
                    let loss_probability = table
                        .pauli_strings
                        .iter()
                        .zip(&table.probabilities)
                        .filter_map(|(pauli, probability)| {
                            (((pauli >> shift) & 0b111) == 0b100).then_some(probability)
                        })
                        .sum::<f64>();
                    (qubit, loss_probability)
                })
                .collect::<Vec<_>>();
            let mut output_errors = Vec::new();

            for (qubit, loss_probability) in operand_losses {
                if loss_probability > 0.0 {
                    self.not_lost_probs[qubit] *= 1.0 - loss_probability;
                    output_errors.push((
                        Register::quantum(wire_map.qubit_wire(qubit).0),
                        1.0 - self.not_lost_probs[qubit],
                    ));
                }
            }

            if !output_errors.is_empty() {
                error
                    .get_or_insert_with(GateErrorInfo::default)
                    .output_errors
                    .extend(output_errors);
            }
        }

        self.internal_builder
            .gate(wire_map, name, is_adjoint, inputs, args, error, call_stack);
    }

    fn measurement(
        &mut self,
        wire_map: &WireMap,
        name: &str,
        qubit: usize,
        result: usize,
        call_stack: LogicalStack,
    ) {
        self.not_lost_probs[qubit] = 1.0;
        self.internal_builder
            .measurement(wire_map, name, qubit, result, call_stack);
    }

    fn reset(&mut self, wire_map: &WireMap, qubit: usize, call_stack: LogicalStack) {
        self.not_lost_probs[qubit] = 1.0;
        self.internal_builder.reset(wire_map, qubit, call_stack);
    }
}

#[cfg(test)]
mod tests {
    use qdk_simulators::noise_config::{NoiseConfig, NoiseTable, encode_pauli};

    use super::*;
    use crate::builder::{ClassicalControlInput, WireMapBuilder};

    #[test]
    fn ignores_classical_controls_when_selecting_noise_table() {
        let mut noise_config = NoiseConfig::NOISELESS;
        noise_config.x = NoiseTable {
            qubits: 1,
            pauli_strings: vec![encode_pauli("L")],
            probabilities: vec![0.01],
            on_loss: noise_config.x.on_loss,
        };

        let mut wire_map_builder = WireMapBuilder::default();
        wire_map_builder.map_qubit(0, None);
        wire_map_builder.link_result_to_qubit(0, 0);
        let mut builder = OperationListBuilderWithLoss::new(
            OperationListBuilder::new(usize::MAX, Vec::new(), false, false),
            1,
            Some(&noise_config),
        );
        let classical_controls = [ClassicalControlInput {
            result_id: 0,
            inverted: false,
        }];
        let inputs = GateInputs {
            targets: &[0],
            controls: &[],
            classical_controls: &classical_controls,
        };

        builder.gate(
            wire_map_builder.current(),
            "X",
            false,
            &inputs,
            Vec::new(),
            None,
            LogicalStack::default(),
        );

        assert!((builder.not_lost_probs[0] - 0.99).abs() < f64::EPSILON);
    }

    #[test]
    fn tracks_cumulative_loss_and_reset() {
        let mut noise_config = NoiseConfig::NOISELESS;
        noise_config.x = NoiseTable {
            qubits: 1,
            pauli_strings: vec![encode_pauli("L")],
            probabilities: vec![0.01],
            on_loss: noise_config.x.on_loss,
        };

        let mut wire_map_builder = WireMapBuilder::default();
        wire_map_builder.map_qubit(0, None);
        wire_map_builder.link_result_to_qubit(0, 0);
        let mut builder = OperationListBuilderWithLoss::new(
            OperationListBuilder::new(usize::MAX, Vec::new(), false, false),
            1,
            Some(&noise_config),
        );
        let inputs = GateInputs {
            targets: &[0],
            controls: &[],
            classical_controls: &[],
        };

        builder.gate(
            wire_map_builder.current(),
            "X",
            false,
            &inputs,
            Vec::new(),
            None,
            LogicalStack::default(),
        );
        assert!((builder.not_lost_probs[0] - 0.99).abs() < f64::EPSILON);

        builder.gate(
            wire_map_builder.current(),
            "X",
            false,
            &inputs,
            Vec::new(),
            None,
            LogicalStack::default(),
        );
        assert!((builder.not_lost_probs[0] - 0.9801).abs() < f64::EPSILON);

        builder.gate(
            wire_map_builder.current(),
            "H",
            false,
            &inputs,
            Vec::new(),
            None,
            LogicalStack::default(),
        );
        assert!((builder.not_lost_probs[0] - 0.9801).abs() < f64::EPSILON);

        builder.measurement(
            wire_map_builder.current(),
            "M",
            0,
            0,
            LogicalStack::default(),
        );
        assert_eq!(builder.not_lost_probs[0], 1.0);

        builder.gate(
            wire_map_builder.current(),
            "X",
            false,
            &inputs,
            Vec::new(),
            None,
            LogicalStack::default(),
        );
        builder.reset(wire_map_builder.current(), 0, LogicalStack::default());
        assert_eq!(builder.not_lost_probs[0], 1.0);
    }
}

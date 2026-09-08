

use qdk_simulators::noise_config::{NoiseConfig, NoiseTable};

use crate::builder::{OperationListBuilder, OperationReceiver};

// Wraps OperationListBuilder, calculating loss.
pub(crate) struct OperationListBuilderWithLoss {
    // Internal builder.
    internal_builder: Box<dyn OperationReceiver>,

    // Probabilities of "not lost".
    not_lost_probs: Vec<f64>,

    // Noise config.
    // TODO: this have to be a reference.
    noise_config: NoiseConfig<f64, f64>,
}

impl OperationReceiver for OperationListBuilderWithLoss {
    fn gate(
        &mut self,
        wire_map: &crate::builder::WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &crate::builder::GateInputs,
        args: Vec<String>,
        error: Option<f64>,
        call_stack: crate::builder::LogicalStack,
    ) {


        // TODO: update loss according to noise-config.
        // Then, if any qubits chnaged their "not lost prob", add new information into
        // `error` arg that will be passed to internal builder.
        // Note that error here must be of type Option<GateErrorInfo>

        self.internal_builder.gate(wire_map, name, is_adjoint, inputs, args,error, call_stack);
    }

    fn measurement(
        &mut self,
        wire_map: &crate::builder::WireMap,
        name: &str,
        qubit: usize,
        result: usize,
        call_stack: crate::builder::LogicalStack,
    ) {
        // TODO: update "not lost prob" accordingly.
        self.internal_builder.measurement(wire_map,  name, qubit, result, call_stack);
    }

    fn reset(&mut self, wire_map: &crate::builder::WireMap, qubit: usize, call_stack: crate::builder::LogicalStack) {
        
        // TODO: set "not lost prob" for this qubit to 1.0
        
        self.internal_builder.reset(wire_map, qubit, call_stack);
    }
}
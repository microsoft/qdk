// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The MPS simulation method's failures: each message says what failed and
//! names the item at fault, and an error a failure wraps is its source,
//! which a caller showing the chain shows once. The tests check what each
//! message names, never its wording.

use std::error::Error;

use tensornet::MpsShapeError;

use super::reference_spy::ReferenceSpyError;
use crate::mps_simulator::MpsResumerError;
use crate::qir::{QirGate, QirInstruction};

/// `error`'s message, then each cause in its chain after `: `: what a
/// caller that shows the chain prints, as `qdk`'s Python errors do.
fn with_causes(error: &dyn Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// A user whose shot failed reads, in the failure's chain, what failed and
/// the item at fault (the instruction, the qubit, the qubits and χ, the
/// stop flag) and the cause a failure wraps, once: a message that also
/// repeated its source would show the engine's text twice.
#[test]
fn each_failure_names_its_item_and_shows_its_cause_once() {
    let shape = MpsShapeError::ElementCountOverflow { site: 35 };
    let engine = ReferenceSpyError::Device.to_string();
    let rows: [(MpsResumerError<ReferenceSpyError>, Vec<String>); 5] = [
        (
            MpsResumerError::Unsupported {
                instruction: QirInstruction::ReadoutNoise,
            },
            vec![QirInstruction::ReadoutNoise.to_string()],
        ),
        (
            MpsResumerError::RepeatedQubit {
                instruction: QirInstruction::Gate(QirGate::Cx),
                qubit: 3,
            },
            vec![
                QirInstruction::Gate(QirGate::Cx).to_string(),
                "3".to_string(),
            ],
        ),
        (
            MpsResumerError::Capacity {
                qubits: 70,
                max_bond_dimension: 1 << 40,
                source: shape.clone(),
            },
            vec![
                "70".to_string(),
                (1_usize << 40).to_string(),
                shape.to_string(),
            ],
        ),
        (
            MpsResumerError::Engine(ReferenceSpyError::Device),
            vec!["engine".to_string(), engine],
        ),
        (MpsResumerError::Stopped, vec!["stop flag".to_string()]),
    ];
    for (error, items) in rows {
        let shown = with_causes(&error);
        for item in &items {
            assert!(shown.contains(item.as_str()), "{item:?} in {shown:?}");
        }
        let last = items.last().expect("each row names an item");
        assert_eq!(shown.matches(last.as_str()).count(), 1, "{shown:?}");
    }
}

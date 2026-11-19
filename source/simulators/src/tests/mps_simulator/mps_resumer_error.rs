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
/// caller that walks `source()` prints.
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

/// A user whose shot failed reads, in the failure's message, what failed
/// and the item at fault (the instruction, the qubit, the qubits and χ, the
/// stop flag), and, in its chain, the cause a failure wraps once: a message
/// that also repeated its source would show the engine's text twice, and a
/// failure that dropped its source would hide it.
#[test]
fn each_failure_names_its_item_and_shows_its_cause_once() {
    let shape = MpsShapeError::ElementCountOverflow { site: 35 };
    let rows: [(
        MpsResumerError<ReferenceSpyError>,
        Vec<String>,
        Option<String>,
    ); 5] = [
        (
            MpsResumerError::Unsupported {
                instruction: QirInstruction::ReadoutNoise,
            },
            vec![QirInstruction::ReadoutNoise.to_string()],
            None,
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
            None,
        ),
        (
            MpsResumerError::Capacity {
                qubits: 70,
                max_bond_dimension: 1 << 40,
                source: shape.clone(),
            },
            vec!["70".to_string(), (1_usize << 40).to_string()],
            Some(shape.to_string()),
        ),
        (
            MpsResumerError::Engine(ReferenceSpyError::Device),
            // What failed: the engine, said by the message itself, as the
            // engine's own text is only its cause.
            vec!["engine".to_string()],
            Some(ReferenceSpyError::Device.to_string()),
        ),
        (
            MpsResumerError::Stopped,
            vec!["stop flag".to_string()],
            None,
        ),
    ];
    for (error, items, cause) in rows {
        let message = error.to_string();
        for item in &items {
            assert!(message.contains(item.as_str()), "{item:?} in {message:?}");
        }
        let shown = with_causes(&error);
        match cause {
            Some(cause) => {
                assert!(!message.contains(cause.as_str()), "{message:?}");
                assert_eq!(shown.matches(cause.as_str()).count(), 1, "{shown:?}");
            }
            None => assert_eq!(shown, message, "a failure that wraps no error"),
        }
    }
}

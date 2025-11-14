use std::vec;

use regex_lite::Regex;
use serde::de::Error as DeError;
use serde_json::{Error, Map, Value, from_str, from_value};

use crate::{
    CURRENT_VERSION, Circuit, CircuitGroup, Operation,
    circuit::{Qubit, operation_list_to_grid},
};

/// Parses a JSON string into a `CircuitGroup`.
///
/// This function attempts to parse the provided JSON string into a `CircuitGroup` object.
/// If the JSON is in a legacy format, it will attempt to upgrade the schema to the current format.
///
/// # Arguments
/// * `json_str` - A string slice containing the JSON representation of the circuit group or legacy circuit.
///
/// # Returns
/// * `Ok(CircuitGroup)` if the JSON is successfully parsed and upgraded (if necessary).
/// * `Err(Error)` if the JSON is invalid or does not match any known schema.
///
/// # Errors
/// This function returns an error if:
/// * The JSON is not valid.
/// * The JSON does not conform to the expected schema for a circuit group or legacy circuit.
pub fn json_to_circuits(json_str: &str) -> Result<CircuitGroup, String> {
    {
        let parsed_value: Value = from_str(json_str).map_err(|e| format!("Error: {e}"))?;
        if let Value::Object(map) = parsed_value {
            to_circuit_group(map)
        } else {
            Err(Error::custom("Invalid JSON format"))
        }
    }
    .map_err(|e| format!("Error: {e}"))
}

fn to_circuit_group(mut json: Map<String, Value>) -> Result<CircuitGroup, Error> {
    let empty_circuit = Circuit {
        qubits: vec![],
        component_grid: vec![],
    };

    let empty_circuit_group = CircuitGroup {
        version: CURRENT_VERSION,
        circuits: vec![empty_circuit.clone()],
    };

    if json.is_empty() {
        return Ok(empty_circuit_group);
    }

    if let Some(version) = json.get("version") {
        // If it has a "version" field, it is up-to-date
        if is_circuit_group(&json) {
            return from_value(Value::Object(json));
        } else if is_circuit(&json) {
            return Ok(CircuitGroup {
                version: from_value(version.clone())?,
                circuits: vec![from_value(Value::Object(json))?],
            });
        }
        return Err(DeError::custom(
            "Unknown schema: circuit is neither a CircuitGroup nor a Circuit.",
        ));
    } else if is_circuit(&json) {
        // If it's a Circuit without a version, wrap it in a CircuitGroup
        return Ok(CircuitGroup {
            version: CURRENT_VERSION,
            circuits: vec![from_value(Value::Object(json))?],
        });
    } else if json.contains_key("operations") {
        // Legacy schema: convert to CircuitGroup
        if !json.contains_key("qubits") || !json["qubits"].is_array() {
            return Err(DeError::custom(
                "Unknown schema: circuit is missing qubit information.",
            ));
        }
        if !json.contains_key("operations") || !json["operations"].is_array() {
            return Err(DeError::custom(
                "Unknown schema: circuit is missing operation information.",
            ));
        }

        let qubits: Vec<Qubit> = if let Some(qubits) = json.get("qubits").and_then(Value::as_array)
        {
            qubits
                .iter()
                .map(|qubit| {
                    Ok(Qubit {
                        id: usize::try_from(qubit.get("id").and_then(Value::as_u64).ok_or_else(
                            || Error::custom("Expected 'id' field to exist and be a number"),
                        )?)
                        .map_err(|_| Error::custom("Value of 'id' is out of range"))?,
                        num_results: usize::try_from(
                            qubit
                                .get("numChildren")
                                .and_then(Value::as_u64)
                                .unwrap_or(0),
                        )
                        .map_err(|_| Error::custom("Value of 'numChildren' is out of range"))?,
                        declarations: vec![],
                    })
                })
                .collect::<Result<Vec<Qubit>, Error>>()?
        } else {
            unreachable!("We checked that qubits exists");
        };

        let component_grid =
            if let Some(operations) = json.get_mut("operations").and_then(Value::as_array_mut) {
                // Process each operation in place
                operations.iter_mut().for_each(to_operation);

                // Convert the processed operations into the `Operation` type
                let operation_list: Vec<Operation> = operations
                    .drain(..)
                    .map(from_value)
                    .collect::<Result<Vec<Operation>, Error>>()?;

                operation_list_to_grid(operation_list, &qubits, false)
            } else {
                unreachable!("We checked that operations exists");
            };

        return Ok(CircuitGroup {
            version: CURRENT_VERSION,
            circuits: vec![Circuit {
                qubits,
                component_grid,
            }],
        });
    }
    Err(DeError::custom(
        "Unknown schema: circuit does not match any known format.",
    ))
}

fn is_circuit_group(json: &Map<String, Value>) -> bool {
    json.contains_key("circuits") && json["circuits"].is_array()
}

fn is_circuit(json: &Map<String, Value>) -> bool {
    json.contains_key("qubits")
        && json["qubits"].is_array()
        && json.contains_key("componentGrid")
        && json["componentGrid"].is_array()
}

fn map_register_field(field: Option<&Value>) -> Vec<Value> {
    field
        .and_then(|f| {
            f.as_array().map(|array| {
                array
                    .iter()
                    .map(|item| {
                        let mut register = Map::new();
                        if let Some(c_id) = item.get("cId") {
                            register.insert("result".to_string(), c_id.clone());
                        }
                        // Note: if "qId" is missing, the json deserialization later on
                        // will fail and produce the appropriate error
                        if let Some(q_id) = item.get("qId") {
                            register.insert("qubit".to_string(), q_id.clone());
                        }
                        Value::Object(register)
                    })
                    .collect()
            })
        })
        .unwrap_or_default()
}

fn to_operation(op: &mut Value) {
    let Value::Object(op) = op else {
        panic!("Expected an object for operation, but got something else");
    };

    let targets = map_register_field(op.get("targets"));
    let controls = map_register_field(op.get("controls"));

    if let Some(children) = op.get_mut("children")
        && let Some(children_array) = children.as_array_mut()
    {
        children_array.iter_mut().for_each(to_operation);
        let component_column = serde_json::json!({
            "components": children_array
        });
        op.insert("children".to_string(), Value::Array(vec![component_column]));
    }

    if let Some(display_args) = op.get("displayArgs") {
        op.insert("args".to_string(), Value::Array(vec![display_args.clone()]));
        // Assume the parameter is always "theta" for now
        let mut param = Map::new();
        param.insert("name".to_string(), Value::String("theta".to_string()));
        param.insert("type".to_string(), Value::String("Double".to_string()));
        op.insert(
            "params".to_string(),
            Value::Array(vec![Value::Object(param)]),
        );
    }

    if op
        .get("isMeasurement")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        op.insert("kind".to_string(), Value::String("measurement".to_string()));
        op.insert("qubits".to_string(), Value::Array(controls));
        op.insert("results".to_string(), Value::Array(targets));
    } else if let Some(label) = get_ket_label(op) {
        op.insert("kind".to_string(), Value::String("ket".to_string()));
        op.insert("gate".to_string(), Value::String(label));
        op.insert("targets".to_string(), Value::Array(targets));
    } else {
        op.insert("kind".to_string(), Value::String("unitary".to_string()));
        op.insert("targets".to_string(), Value::Array(targets));
        op.insert("controls".to_string(), Value::Array(controls));
    }
}

fn get_ket_label(op: &Map<String, Value>) -> Option<String> {
    // Check if the "gate" field exists and is a string
    if let Some(gate) = op.get("gate").and_then(Value::as_str) {
        // Define the regex for matching the ket pattern of |{label}> or |{label}⟩
        #[allow(clippy::unicode_not_nfc)]
        let ket_regex =
            Regex::new(r"^\|([^\s〉⟩〉>]+)(?:[〉⟩〉>])$").expect("Invalid regex pattern");

        // Match the string against the regex
        if let Some(captures) = ket_regex.captures(gate) {
            // If valid, return the inner label (captured group 1)
            return captures.get(1).map(|m| m.as_str().to_string());
        }
    }

    // Return None if the "gate" field is missing, not a string, or doesn't match the pattern
    None
}

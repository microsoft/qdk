// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compact JSON wire format shared by the stream backend and driver.
//! Each message is terminated by a newline and flushed before awaiting a reply.

use crate::{
    BatchResult, Parameter, QuantumBackendInstruction, QuantumBackendResult,
    instructions::{instruction_is_valid, parse_instruction},
};
use std::{
    fmt::Write as _,
    io::{BufRead, Write},
};

/// Serializes a batch as a JSON array of canonical `name(arg,...)` strings.
pub fn serialize_batch(instructions: &[QuantumBackendInstruction]) -> Result<String, String> {
    let mut encoded = Vec::with_capacity(instructions.len());
    for instruction in instructions {
        if !instruction_is_valid(instruction) {
            return Err(format!(
                "Unsupported or malformed instruction: {instruction:?}"
            ));
        }
        let mut text = format!("{}(", instruction.instruction_name);
        for (index, parameter) in instruction.parameters.iter().enumerate() {
            if index != 0 {
                text.push(',');
            }
            match parameter {
                Parameter::Float(value) => write!(text, "{value}"),
                Parameter::QubitId(value)
                | Parameter::ResultId(value)
                | Parameter::FactoryId(value) => write!(text, "{value}"),
            }
            .expect("formatting into a string cannot fail");
        }
        text.push(')');
        encoded.push(text);
    }
    serde_json::to_string(&encoded).map_err(|error| format!("Failed to serialize batch: {error}"))
}

/// Deserializes a JSON batch, rejecting unknown names and invalid operands.
pub fn deserialize_batch(message: &str) -> Result<Vec<QuantumBackendInstruction>, String> {
    let encoded: Vec<String> =
        serde_json::from_str(message).map_err(|error| format!("Invalid batch JSON: {error}"))?;
    encoded
        .iter()
        .map(|text| {
            let (name, arguments) = text
                .strip_suffix(')')
                .and_then(|text| text.split_once('('))
                .ok_or_else(|| format!("Invalid instruction syntax: {text}"))?;
            let arguments: Vec<_> = if arguments.is_empty() {
                Vec::new()
            } else {
                arguments.split(',').collect()
            };
            parse_instruction(name, &arguments)
        })
        .collect()
}

/// Serializes ordered results as a quoted JSON string of `0`, `1`, and `2`.
pub fn serialize_result(result: &BatchResult) -> Result<String, String> {
    let encoded: String = result
        .results
        .iter()
        .map(|value| match value {
            QuantumBackendResult::Zero => '0',
            QuantumBackendResult::One => '1',
            QuantumBackendResult::Error => '2',
        })
        .collect();
    serde_json::to_string(&encoded).map_err(|error| format!("Failed to serialize result: {error}"))
}

/// Deserializes a quoted result string, rejecting all non-result characters.
pub fn deserialize_result(message: &str) -> Result<BatchResult, String> {
    let encoded: String =
        serde_json::from_str(message).map_err(|error| format!("Invalid result JSON: {error}"))?;
    let results = encoded
        .chars()
        .map(|value| match value {
            '0' => Ok(QuantumBackendResult::Zero),
            '1' => Ok(QuantumBackendResult::One),
            '2' => Ok(QuantumBackendResult::Error),
            _ => Err(format!("Invalid result character: {value:?}")),
        })
        .collect::<Result<_, _>>()?;
    Ok(BatchResult { results })
}

/// Reads one newline-delimited message, distinguishing clean EOF from truncation.
pub(crate) fn read_message(reader: &mut impl BufRead) -> Result<Option<String>, String> {
    let mut message = String::new();
    if reader
        .read_line(&mut message)
        .map_err(|error| format!("Failed to read message: {error}"))?
        == 0
    {
        return Ok(None);
    }
    if !message.ends_with('\n') {
        return Err("Truncated message: missing terminating newline".into());
    }
    Ok(Some(message))
}

/// Writes and flushes one complete newline-delimited message.
pub(crate) fn write_message(writer: &mut impl Write, message: &str) -> Result<(), String> {
    writer
        .write_all(message.as_bytes())
        .and_then(|()| writer.write_all(b"\n"))
        .and_then(|()| writer.flush())
        .map_err(|error| format!("Failed to write message: {error}"))
}

#[cfg(test)]
mod tests;

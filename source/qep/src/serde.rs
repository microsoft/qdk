// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compact JSON wire format shared by the stream backend and driver.
//! Each message is terminated by a newline and flushed before awaiting a reply.

use crate::{
    BatchResult, Parameter, QuantumBackendInstruction, QuantumBackendResult,
    instructions::{instruction_is_valid, parse_instruction},
};
use serde::de::{DeserializeSeed, Deserializer, Error, SeqAccess, Visitor};
use std::{
    fmt::{self, Write as _},
    io::{BufRead, Write},
};

/// Serializes a batch as a JSON array of canonical `name(arg,...)` strings.
pub fn serialize_batch(instructions: &[QuantumBackendInstruction]) -> Result<String, String> {
    let mut encoded = String::new();
    serialize_batch_into(instructions, &mut encoded)?;
    Ok(encoded)
}

/// Serializes into a reusable buffer; its contents are unspecified on error.
pub(crate) fn serialize_batch_into(
    instructions: &[QuantumBackendInstruction],
    encoded: &mut String,
) -> Result<(), String> {
    encoded.clear();
    encoded.push('[');
    for (index, instruction) in instructions.iter().enumerate() {
        if !instruction_is_valid(instruction) {
            return Err(format!(
                "Unsupported or malformed instruction: {instruction:?}"
            ));
        }
        if index != 0 {
            encoded.push(',');
        }
        // Validated canonical names and formatted numbers need no JSON escaping.
        encoded.push('"');
        encoded.push_str(instruction.instruction_name);
        encoded.push('(');
        for (index, parameter) in instruction.parameters.iter().enumerate() {
            if index != 0 {
                encoded.push(',');
            }
            match parameter {
                Parameter::Float(value) => write!(encoded, "{value}"),
                Parameter::QubitId(value)
                | Parameter::ResultId(value)
                | Parameter::FactoryId(value) => write!(encoded, "{value}"),
            }
            .expect("formatting into a string cannot fail");
        }
        encoded.push_str(")\"");
    }
    encoded.push(']');
    Ok(())
}

/// Deserializes a JSON batch, rejecting unknown names and invalid operands.
pub fn deserialize_batch(message: &str) -> Result<Vec<QuantumBackendInstruction>, String> {
    let mut instructions = Vec::new();
    deserialize_batch_into(message, &mut instructions)?;
    Ok(instructions)
}

/// Parses into a reusable batch buffer; its contents are unspecified on error.
pub(crate) fn deserialize_batch_into(
    message: &str,
    instructions: &mut Vec<QuantumBackendInstruction>,
) -> Result<(), String> {
    instructions.clear();
    let mut deserializer = serde_json::Deserializer::from_str(message);
    deserializer
        .deserialize_seq(BatchVisitor(instructions))
        .and_then(|()| deserializer.end())
        .map_err(|error| format!("Invalid batch JSON: {error}"))
}

struct BatchVisitor<'a>(&'a mut Vec<QuantumBackendInstruction>);

impl<'de> Visitor<'de> for BatchVisitor<'_> {
    type Value = ();

    /// Describes the required JSON envelope in decoding errors.
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an array of instruction strings")
    }

    /// Decodes directly into the retained instruction vector.
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        while let Some(instruction) = sequence.next_element_seed(InstructionVisitor)? {
            self.0.push(instruction);
        }
        Ok(())
    }
}

struct InstructionVisitor;

impl<'de> DeserializeSeed<'de> for InstructionVisitor {
    type Value = QuantumBackendInstruction;

    /// Borrows unescaped JSON strings and accepts the decoder's scratch buffer for escapes.
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(self)
    }
}

impl Visitor<'_> for InstructionVisitor {
    type Value = QuantumBackendInstruction;

    /// Describes the required instruction representation in decoding errors.
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an instruction string")
    }

    /// Parses operands before releasing the borrowed JSON string.
    fn visit_str<E: Error>(self, text: &str) -> Result<Self::Value, E> {
        let (name, arguments) = text
            .strip_suffix(')')
            .and_then(|text| text.split_once('('))
            .ok_or_else(|| E::custom(format!("Invalid instruction syntax: {text}")))?;
        parse_instruction(name, arguments.split(',').filter(|_| !arguments.is_empty()))
            .map_err(E::custom)
    }
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
pub(crate) fn read_message(
    reader: &mut impl BufRead,
    message: &mut String,
) -> Result<bool, String> {
    message.clear();
    if reader
        .read_line(message)
        .map_err(|error| format!("Failed to read message: {error}"))?
        == 0
    {
        return Ok(false);
    }
    if !message.ends_with('\n') {
        return Err("Truncated message: missing terminating newline".into());
    }
    Ok(true)
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

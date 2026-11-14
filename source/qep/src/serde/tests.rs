// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{deserialize_batch, deserialize_result, serialize_batch, serialize_result};
use crate::{
    BatchResult, OPERATION_COUNT, Parameter,
    QuantumBackendResult::{Error, One, Zero},
    instructions::{instruction_id_to_name, instruction_is_valid},
};
use smallvec::smallvec;

#[test]
/// Checks the exact compact wire representation and typed operand reconstruction.
fn compact_wire_format() -> Result<(), String> {
    let message = r#"["h(1)","ccx(1,2,3)","rz(3,-0.125)","mz_store(3,7)","read_result(7)"]"#;
    let instructions = deserialize_batch(message)?;
    assert_eq!(serialize_batch(&instructions)?, message);
    assert_eq!(
        instructions[2].parameters.as_slice(),
        &[Parameter::QubitId(3), Parameter::Float(-0.125)]
    );
    assert_eq!(
        instructions[3].parameters.as_slice(),
        &[Parameter::QubitId(3), Parameter::ResultId(7)]
    );
    assert_eq!(
        instructions[4].parameters.as_slice(),
        &[Parameter::ResultId(7)]
    );
    let result = BatchResult {
        results: smallvec![Zero, One, Zero, Error],
    };
    assert_eq!(serialize_result(&result)?, r#""0102""#);
    assert_eq!(deserialize_result(r#""0102""#)?, result);
    assert_eq!(serialize_batch(&[])?, "[]");
    assert_eq!(deserialize_batch("[]")?, []);
    assert_eq!(serialize_result(&BatchResult::default())?, r#""""#);
    assert_eq!(deserialize_result(r#""""#)?, BatchResult::default());
    Ok(())
}

#[test]
/// Round-trips every canonical instruction with operands from its declared shape.
fn all_instruction_shapes() -> Result<(), String> {
    for id in 0..OPERATION_COUNT {
        let name = instruction_id_to_name(id);
        let arguments = match name {
            "ccx" => "1,2,3",
            "rx" | "ry" | "rz" => "1,-1.25e-12",
            "rxx" | "ryy" | "rzz" => "1,2,-1.25e-12",
            "cx" | "cy" | "cz" | "swap" | "mz_store" | "mz_store_reset" => "1,2",
            _ => "1",
        };
        let instructions = deserialize_batch(&format!(r#"["{name}({arguments})"]"#))?;
        assert!(instruction_is_valid(&instructions[0]));
        assert_eq!(
            deserialize_batch(&serialize_batch(&instructions)?)?,
            instructions
        );
    }
    Ok(())
}

#[test]
/// Rejects malformed JSON, unknown names, invalid arities, IDs, and result digits.
fn malformed_messages_are_errors() -> Result<(), String> {
    for message in [
        "null",
        "{}",
        "[1]",
        r#"["unknown(1)"]"#,
        r#"["h"]"#,
        r#"["h(1"]"#,
        r#"["h()"]"#,
        r#"["h(1,2)"]"#,
        r#"["h(-1)"]"#,
        r#"["h(1.5)"]"#,
        r#"["h(1))"]"#,
        r#"["rz(1,no)"]"#,
        r#"["rz(1)"]"#,
        r#"["mz_store(1,-2)"]"#,
        r#"["h(1)"] trailing"#,
    ] {
        assert!(deserialize_batch(message).is_err(), "{message}");
    }
    for message in [
        "010",
        "[]",
        "null",
        r#""3""#,
        r#""0 1""#,
        r#""01" trailing"#,
    ] {
        assert!(deserialize_result(message).is_err(), "{message}");
    }
    let mut invalid = deserialize_batch(r#"["h(1)"]"#)?;
    invalid[0].parameters[0] = Parameter::ResultId(1);
    assert!(serialize_batch(&invalid).is_err());
    invalid[0].instruction_name = "unknown";
    assert!(serialize_batch(&invalid).is_err());
    Ok(())
}

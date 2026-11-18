// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The examples' command-line arguments.

use super::Res;

/// The value of `--flag value` in `args`.
pub fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// The number `--name N` in `args`, else `default`.
pub fn number(args: &[String], name: &str, default: usize) -> Res<usize> {
    Ok(flag(args, name).map_or(Ok(default), |x| x.parse())?)
}

/// The number of qubits `--n N` a circuit family needs: an error when it
/// is missing or 0.
pub fn qubits(args: &[String]) -> Res<usize> {
    match number(args, "--n", 0)? {
        0 => Err("a family needs --n N, its number of qubits, at least 1".into()),
        n => Ok(n),
    }
}

/// The order `--order a,b,c,…` (the site of each qubit; brackets and
/// spaces allowed), else the identity.
pub fn order(args: &[String], qubits: usize) -> Res<Vec<usize>> {
    let Some(text) = flag(args, "--order") else {
        return Ok((0..qubits).collect());
    };
    let order: Vec<usize> = text
        .trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
        .split(',')
        .map(|x| x.trim().parse())
        .collect::<Result<_, _>>()?;
    let mut sorted = order.clone();
    sorted.sort_unstable();
    if sorted != (0..qubits).collect::<Vec<_>>() {
        return Err(format!("--order is not a permutation of 0..{qubits}").into());
    }
    Ok(order)
}

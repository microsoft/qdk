// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a code is: its size, and whether its operators fit together. Each
//! fact is a sentence that is true or false, so a code under construction
//! shows what is right and what is not yet.

use crate::{
    gf2::{Basis, Form},
    stabilizer::Pauli,
};
use qodec::Code;
use qodec_runtime_recipes::pauli::Pauli as SparsePauli;

/// One operator of a code, by its role.
#[derive(Clone, Debug)]
pub struct Operator {
    /// `S0`, `X1`, `Z0`: its role and index.
    pub name: String,
    /// The operator, or why it does not parse.
    pub pauli: Result<Pauli, String>,
}

/// What the tool knows about a code.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// Physical qubits.
    pub n: usize,
    /// Logical qubits, as declared.
    pub k: usize,
    /// Its operators: stabilizers, then logical X, then logical Z.
    pub operators: Vec<Operator>,
    /// Each fact, and whether it holds.
    pub holds: Vec<(bool, String)>,
}

impl Facts {
    /// Whether every fact holds.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.holds.iter().all(|(ok, _)| *ok)
    }
}

/// The facts of `code`.
#[must_use]
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub fn facts(code: &Code) -> Facts {
    let n = code.physical_qubit_count();
    let parse = |p: &qodec::PauliString| -> Result<Pauli, String> {
        let sparse = SparsePauli::parse(&p.0)?;
        Pauli::place(&sparse, Some).map(|(p, _)| p)
    };
    let mut f = Facts {
        n,
        k: code.x.len(),
        ..Facts::default()
    };
    for (role, list) in [("S", &code.stabilizers), ("X", &code.x), ("Z", &code.z)] {
        for (i, p) in list.iter().enumerate() {
            f.operators.push(Operator {
                name: format!("{role}{i}"),
                pauli: parse(p),
            });
        }
    }
    let bad: Vec<String> = f
        .operators
        .iter()
        .filter_map(|o| o.pauli.as_ref().err().map(|e| format!("{}: {e}", o.name)))
        .collect();
    if !bad.is_empty() {
        f.holds.push((
            false,
            format!("Operators that do not parse: {}.", bad.join("; ")),
        ));
        return f;
    }
    let get = |role: &str| -> Vec<(String, Pauli)> {
        f.operators
            .iter()
            .filter(|o| o.name.starts_with(role))
            .map(|o| (o.name.clone(), o.pauli.clone().expect("parsed above")))
            .collect()
    };
    let (s, x, z) = (get("S"), get("X"), get("Z"));

    f.holds.push((
        x.len() == z.len(),
        format!("{} logical X and {} logical Z operators.", x.len(), z.len()),
    ));
    let clashes = |a: &[(String, Pauli)], b: &[(String, Pauli)], same: bool| -> Vec<String> {
        let mut out = Vec::new();
        for (i, (na, pa)) in a.iter().enumerate() {
            for (nb, pb) in &b[if same { i + 1 } else { 0 }..] {
                if !pa.commutes(pb) {
                    out.push(format!("{na}·{nb}"));
                }
            }
        }
        out
    };
    let stab_clash = clashes(&s, &s, true);
    f.holds.push((
        stab_clash.is_empty(),
        if stab_clash.is_empty() {
            "The stabilizers commute.".to_string()
        } else {
            format!("Stabilizers that anticommute: {}.", stab_clash.join(", "))
        },
    ));
    let logicals: Vec<(String, Pauli)> = x.iter().chain(&z).cloned().collect();
    let log_clash = clashes(&logicals, &s, false);
    f.holds.push((
        log_clash.is_empty(),
        if log_clash.is_empty() {
            "The logical operators commute with the stabilizers.".to_string()
        } else {
            format!(
                "Logical operators that anticommute with a stabilizer: {}.",
                log_clash.join(", ")
            )
        },
    ));
    let mut pairing = Vec::new();
    for (i, (nx, px)) in x.iter().enumerate() {
        for (j, (nz, pz)) in z.iter().enumerate() {
            if px.commutes(pz) == (i == j) {
                pairing.push(format!("{nx}·{nz}"));
            }
        }
    }
    pairing.extend(clashes(&x, &x, true));
    pairing.extend(clashes(&z, &z, true));
    f.holds.push((
        pairing.is_empty(),
        if pairing.is_empty() {
            "Each X_i anticommutes with Z_i only; the rest commute.".to_string()
        } else {
            format!("Logical pairs that do not fit: {}.", pairing.join(", "))
        },
    ));
    let vector = |p: &Pauli| {
        let mut v = Form::default();
        for q in p.x.ones() {
            v.vars.set(2 * q, true);
        }
        for q in p.z.ones() {
            v.vars.set(2 * q + 1, true);
        }
        v
    };
    let mut basis = Basis::default();
    let dependent: Vec<String> = s
        .iter()
        .filter(|(_, p)| basis.add(&vector(p)).is_some())
        .map(|(name, _)| name.clone())
        .collect();
    let rank = s.len() - dependent.len();
    f.holds.push((
        true,
        if dependent.is_empty() {
            format!("{rank} independent stabilizers.")
        } else {
            format!(
                "{rank} independent stabilizers; {} follow from the others.",
                dependent.join(", ")
            )
        },
    ));
    let logical_dependent: Vec<String> = logicals
        .iter()
        .filter(|(_, p)| basis.add(&vector(p)).is_some())
        .map(|(name, _)| name.clone())
        .collect();
    f.holds.push((
        logical_dependent.is_empty(),
        if logical_dependent.is_empty() {
            "The logical operators are independent of the stabilizers.".to_string()
        } else {
            format!(
                "Logical operators the others generate: {}.",
                logical_dependent.join(", ")
            )
        },
    ));
    let encodes = n.saturating_sub(rank);
    f.holds.push((
        encodes == f.k,
        format!("[[{n},{}]]: {n} qubits less {rank} independent stabilizers leave {encodes} logical qubits{}.", f.k, if encodes == f.k { "" } else { ", not the declared number" }),
    ));
    f
}

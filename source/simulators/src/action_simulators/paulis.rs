// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What both simulators need of Paulis and Cliffords: conversions to
//! `paulimer`, a check that a Clifford is one, and the Paulis that flip one
//! sign of a `stabilize` step without touching the others.

use binar::Bitwise as _;
use paulimer::{
    CliffordUnitary, DensePauli as DenseOp, SparsePauli as SparseOp, pauli::PauliMutable as _,
    traits::NeutralElement as _,
};
use qodec_runtime_interface::pauli::{Clifford, DensePauli, Pauli, PauliOp};

/// Whether `p` has an X and a Z on qubit `q` (both for a Y).
pub(crate) fn letter<P: paulimer::pauli::Pauli>(p: &P, q: usize) -> (bool, bool) {
    (p.x_bits().index(q), p.z_bits().index(q))
}

/// `p` as a `paulimer` sparse Pauli, with its sign.
pub(crate) fn sparse(p: &Pauli) -> SparseOp {
    let mut op = SparseOp::default_size_neutral_element();
    let ys = p.terms().iter().filter(|(_, o)| *o == PauliOp::Y).count();
    // `paulimer` writes `i^e X^x Z^z`, and `Y = i X Z`.
    #[allow(clippy::cast_possible_truncation)]
    op.add_assign_phase_exp(((2 * usize::from(p.negative) + ys) % 4) as u8);
    for &(q, o) in p.terms() {
        if o.has_x() {
            op.mul_assign_right_x(q);
        }
    }
    for &(q, o) in p.terms() {
        if o.has_z() {
            op.mul_assign_right_z(q);
        }
    }
    op
}

/// `p`, `i^phase X^x Z^z`, as a `paulimer` dense Pauli.
fn dense(p: &DensePauli) -> DenseOp {
    let mut op = DenseOp::neutral_element_of_size(p.width());
    op.add_assign_phase_exp(p.phase % 4);
    for q in 0..p.width() {
        if p.x[q] {
            op.mul_assign_right_x(q);
        }
    }
    for q in 0..p.width() {
        if p.z[q] {
            op.mul_assign_right_z(q);
        }
    }
    op
}

/// Whether two dense Paulis commute.
fn commute(a: &DensePauli, b: &DensePauli) -> bool {
    let n = (0..a.width())
        .filter(|&q| (a.x[q] && b.z[q]) ^ (a.z[q] && b.x[q]))
        .count();
    n % 2 == 0
}

/// `c` as a `paulimer` Clifford, or why it is not a Clifford: each image
/// must be Hermitian, and the images must commute and anticommute as `X_q`
/// and `Z_q` do.
pub(crate) fn clifford(c: &Clifford) -> Result<CliffordUnitary, String> {
    let n = c.width();
    let images: Vec<&DensePauli> = (0..n)
        .flat_map(|q| [&c.x_images[q], &c.z_images[q]])
        .collect();
    for p in &images {
        let ys = (0..p.width()).filter(|&q| p.x[q] && p.z[q]).count();
        if p.width() != n || (usize::from(p.phase) + ys) % 2 == 1 {
            return Err("a Clifford image is not a Hermitian Pauli on its qubits".to_string());
        }
    }
    for (i, a) in images.iter().enumerate() {
        for (j, b) in images.iter().enumerate().skip(i + 1) {
            // X_q and Z_q are images 2q and 2q + 1.
            let partners = i / 2 == j / 2;
            if commute(a, b) == partners {
                return Err(
                    "the images of a Clifford do not commute as its generators do".to_string(),
                );
            }
        }
    }
    let ops: Vec<DenseOp> = images.into_iter().map(dense).collect();
    Ok(CliffordUnitary::from_images(&ops))
}

/// For Paulis that commute and are independent, one Pauli per Paulis on
/// their qubits that anticommutes with it and commutes with the others: it
/// flips that one sign alone.
pub(crate) fn duals(paulis: &[Pauli]) -> Result<Vec<Pauli>, String> {
    let mut qubits: Vec<usize> = paulis.iter().flat_map(Pauli::support).collect();
    qubits.sort_unstable();
    qubits.dedup();
    let n = qubits.len();
    let index = |q: usize| qubits.binary_search(&q).unwrap_or(0);
    let m = paulis.len();
    // Row j: what `⟨F, P_j⟩ = F_x · P_j,z + F_z · P_j,x` reads of `F = (F_x | F_z)`,
    // augmented with the identity, solved by elimination over GF(2).
    let mut rows: Vec<Vec<bool>> = paulis
        .iter()
        .enumerate()
        .map(|(j, p)| {
            let mut row = vec![false; 2 * n + m];
            for &(q, o) in p.terms() {
                row[index(q)] = o.has_z();
                row[n + index(q)] = o.has_x();
            }
            row[2 * n + j] = true;
            row
        })
        .collect();
    for (a, p) in paulis.iter().enumerate() {
        for b in paulis.iter().skip(a + 1) {
            let anti = p
                .terms()
                .iter()
                .filter(|(q, o)| b.get(*q).is_some_and(|ob| ob != *o))
                .count();
            if anti % 2 == 1 {
                return Err(format!("`{p}` and `{b}` do not commute"));
            }
        }
    }
    let mut pivots = Vec::with_capacity(m);
    let mut r = 0;
    for col in 0..2 * n {
        let Some(k) = (r..m).find(|&k| rows[k][col]) else {
            continue;
        };
        rows.swap(r, k);
        for k in 0..m {
            if k != r && rows[k][col] {
                let pivot = rows[r].clone();
                for (x, y) in rows[k].iter_mut().zip(pivot) {
                    *x ^= y;
                }
            }
        }
        pivots.push(col);
        r += 1;
        if r == m {
            break;
        }
    }
    if r < m {
        return Err("the Paulis are not independent".to_string());
    }
    // Row r now says `F[pivots[r]] = Σ_j row[2n + j] · target_j`: for target
    // `e_i`, set each pivot to its row's coefficient of `i`.
    (0..m)
        .map(|i| {
            let mut f = vec![false; 2 * n];
            for (row, &col) in rows.iter().zip(&pivots) {
                f[col] = row[2 * n + i];
            }
            let terms = (0..n)
                .filter_map(|k| {
                    let op = match (f[k], f[n + k]) {
                        (false, false) => return None,
                        (true, false) => PauliOp::X,
                        (true, true) => PauliOp::Y,
                        (false, true) => PauliOp::Z,
                    };
                    Some((qubits[k], op))
                })
                .collect();
            Pauli::new(false, terms)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{clifford, duals};
    use qodec_runtime_interface::pauli::{Clifford, Pauli};

    fn p(text: &str) -> Pauli {
        Pauli::parse(text).expect("a Pauli")
    }

    fn anticommute(a: &Pauli, b: &Pauli) -> bool {
        a.terms()
            .iter()
            .filter(|(q, o)| b.get(*q).is_some_and(|ob| ob != *o))
            .count()
            % 2
            == 1
    }

    #[test]
    fn each_dual_flips_its_own_sign_alone() {
        let stabilizers = [
            p("X_0 X_1 X_2 X_3"),
            p("Z_0 Z_1 Z_2 Z_3"),
            p("Z_0 Z_1"),
            p("X_0 X_1"),
        ];
        let fixes = duals(&stabilizers).expect("independent");
        for (i, d) in fixes.iter().enumerate() {
            for (j, s) in stabilizers.iter().enumerate() {
                assert_eq!(anticommute(d, s), i == j, "{d} against {s}");
            }
        }
        assert!(duals(&[p("Z_0"), p("Z_0")]).is_err());
        assert!(duals(&[p("Z_0"), p("X_0")]).is_err());
    }

    #[test]
    fn only_cliffords_convert() {
        assert!(clifford(&Clifford::named("cx", 2).expect("cx")).is_ok());
        let mut broken = Clifford::named("h", 1).expect("h");
        broken.z_images[0] = broken.x_images[0].clone();
        assert!(clifford(&broken).is_err());
    }
}

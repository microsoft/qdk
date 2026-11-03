// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_circuit_store::{AngleExpr, AngleId, AngleOp, CircuitStore, ParameterId};

/// A flat, root-last expression with the existing display vocabulary.
#[derive(Clone, Debug)]
pub struct AngleExpression {
    nodes: Vec<AngleExpr>,
    text: String,
}

impl AngleExpression {
    #[must_use]
    pub fn root(&self) -> AngleId {
        AngleId(u32::try_from(self.nodes.len() - 1).expect("angle expression exceeded u32 range"))
    }

    #[must_use]
    pub fn literal(value: f64) -> Self {
        Self {
            nodes: vec![AngleExpr::Literal(value)],
            text: format!("{value}"),
        }
    }

    /// Copies this expression into a circuit, returning the stored handle.
    pub fn intern(&self, store: &mut CircuitStore) -> AngleId {
        let base = u32::try_from(store.angles().len()).expect("angle arena exceeded u32 range");
        let mut last = AngleId(base);
        for node in &self.nodes {
            last = store.push_angle(shifted(*node, base));
        }
        last
    }

    /// Rebuilds a stored expression, dropping nodes it does not reach.
    #[must_use]
    pub fn extract(store: &CircuitStore, root: AngleId) -> Self {
        let arena = store.angles();
        let count = root.0 as usize + 1;
        let mut live = vec![false; count];
        live[root.0 as usize] = true;
        for index in (0..count).rev() {
            if !live[index] {
                continue;
            }
            match arena.nodes()[index] {
                AngleExpr::Neg(inner) => live[inner.0 as usize] = true,
                AngleExpr::Binary(_, left, right) => {
                    live[left.0 as usize] = true;
                    live[right.0 as usize] = true;
                }
                AngleExpr::Literal(_) | AngleExpr::Parameter(_) => {}
            }
        }

        let mut mapping = vec![0u32; count];
        let mut nodes = Vec::new();
        let mut text: Vec<String> = Vec::new();
        for (index, live) in live.iter().enumerate() {
            if !live {
                continue;
            }
            let position = u32::try_from(nodes.len()).expect("angle expression fits in u32");
            mapping[index] = position;
            let (node, rendered) = match arena.nodes()[index] {
                AngleExpr::Literal(value) => (AngleExpr::Literal(value), format!("{value}")),
                AngleExpr::Parameter(id) => (
                    AngleExpr::Parameter(id),
                    store.parameters()[id.0 as usize].name.clone(),
                ),
                AngleExpr::Neg(inner) => {
                    let inner = mapping[inner.0 as usize];
                    (
                        AngleExpr::Neg(AngleId(inner)),
                        format!("-({})", text[inner as usize]),
                    )
                }
                AngleExpr::Binary(op, left, right) => {
                    let left = mapping[left.0 as usize];
                    let right = mapping[right.0 as usize];
                    (
                        AngleExpr::Binary(op, AngleId(left), AngleId(right)),
                        format!(
                            "({} {} {})",
                            text[left as usize],
                            operator_text(op),
                            text[right as usize]
                        ),
                    )
                }
            };
            nodes.push(node);
            text.push(rendered);
        }
        Self {
            text: text.last().cloned().unwrap_or_default(),
            nodes,
        }
    }
}

/// Rewrites a node's handles as if the expression started at `base`.
fn shifted(node: AngleExpr, base: u32) -> AngleExpr {
    match node {
        AngleExpr::Literal(value) => AngleExpr::Literal(value),
        AngleExpr::Parameter(id) => AngleExpr::Parameter(id),
        AngleExpr::Neg(inner) => AngleExpr::Neg(AngleId(inner.0 + base)),
        AngleExpr::Binary(op, left, right) => {
            AngleExpr::Binary(op, AngleId(left.0 + base), AngleId(right.0 + base))
        }
    }
}

const fn operator_text(op: AngleOp) -> &'static str {
    match op {
        AngleOp::Add => "+",
        AngleOp::Sub => "-",
        AngleOp::Mul => "*",
        AngleOp::Div => "/",
    }
}

/// Joins two expressions under an operator.
#[must_use]
pub fn combine(left: &AngleExpression, op: AngleOp, right: &AngleExpression) -> AngleExpression {
    let offset = u32::try_from(left.nodes.len()).expect("angle expression exceeded u32 range");
    let left_root = left.root();
    let right_root = AngleId(right.root().0 + offset);
    let mut nodes = left.nodes.clone();
    nodes.extend(right.nodes.iter().map(|node| shifted(*node, offset)));
    nodes.push(AngleExpr::Binary(op, left_root, right_root));
    AngleExpression {
        text: format!("({} {} {})", left.text, operator_text(op), right.text),
        nodes,
    }
}

#[must_use]
pub fn negated(value: &AngleExpression) -> AngleExpression {
    let mut nodes = value.nodes.clone();
    nodes.push(AngleExpr::Neg(value.root()));
    AngleExpression {
        text: format!("-({})", value.text),
        nodes,
    }
}

impl AngleExpression {
    #[must_use]
    pub fn parameter(id: ParameterId, name: String) -> Self {
        Self {
            nodes: vec![AngleExpr::Parameter(id)],
            text: name,
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn nodes(&self) -> &[AngleExpr] {
        &self.nodes
    }
}

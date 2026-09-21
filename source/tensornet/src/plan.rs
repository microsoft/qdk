use std::collections::BTreeMap;

use crate::{ContractionQuery, Index, Indices};

/// A stable reference to an operand available at a step.
///
/// `Input` names a position in [`crate::TensorNetwork::nodes`] directly;
/// `Result` names an earlier step of the same plan. Neither is a native or
/// vendor identifier: cuTensorNet's own path encoding recycles operand-list
/// positions as tensors are consumed, which is exactly the kind of adapter
/// detail a stable reference avoids needing at this level.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Operand {
    Input(usize),
    Result(usize),
}

/// One contraction step: the listed operands are consumed and replaced by one
/// result carrying `result_axes`, in that order.
///
/// Arity is not restricted to two. A binary-only executor is a capability of
/// *that* executor, not a rule of this model — an optimizer is free to
/// describe a unary step (extracting a diagonal on its own) or a
/// multi-operand step, the way `opt_einsum`'s bounded-memory search sometimes
/// does, and an executor that cannot run it says so explicitly rather than
/// this type silently disallowing it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractionStep {
    operands: Vec<Operand>,
    result_axes: Indices,
}

impl ContractionStep {
    /// Describes one step. Validity against a query is checked later, by
    /// [`ContractionPlan::new`], because a step's correctness depends on the
    /// whole schedule it sits in, not on anything a single step can check by
    /// itself.
    #[must_use]
    pub fn new(operands: Vec<Operand>, result_axes: Indices) -> Self {
        Self {
            operands,
            result_axes,
        }
    }

    /// The operands this step consumes, in order.
    #[must_use]
    pub fn operands(&self) -> &[Operand] {
        &self.operands
    }

    /// The axes of this step's result, in order.
    #[must_use]
    pub fn result_axes(&self) -> &Indices {
        &self.result_axes
    }

    /// How many operands this step consumes.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.operands.len()
    }
}

/// A complete, declarative schedule for one [`ContractionQuery`].
///
/// No coefficients, no native handles, no optimizer identity and no numerical
/// engine live here: only which axes each step keeps, in what order,
/// referencing stable operand/result positions. A plan is validated against a
/// query at construction and borrows nothing from it afterward, so the same
/// plan can be checked again later against a fresh but topologically
/// identical query — exactly what letting a selected native path outlive its
/// optimizer and import into a new owner already relies on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractionPlan {
    steps: Vec<ContractionStep>,
}

impl ContractionPlan {
    /// Validates `steps` against `query` and returns the plan if they agree.
    ///
    /// A plan needs at least one input node: a fully nodeless network already
    /// describes the empty product, and is out of scope for a type whose
    /// whole purpose is scheduling *some* contraction. A single-node network
    /// needs no steps only when that node's axes are already exactly the
    /// query's kept axes, as a set: anything else — including extracting a
    /// diagonal from one node — is a real operation and needs a (possibly
    /// unary) step to say so, the same way `ContractionStep`'s own doc
    /// explains arity is a schedule choice, not a model restriction.
    ///
    /// Every other network needs at least one step. Each input and each
    /// earlier step's result must be consumed by exactly one later step, or
    /// be the final step's own result; a step's declared `result_axes` must
    /// equal exactly the axes among its consumed operands that the query
    /// still needs afterward — kept by the query, or still carried by some
    /// operand this step did not consume. That is the same incidence rule
    /// [`ContractionQuery::hyperedges`]/[`ContractionQuery::marginalized`]
    /// already use for the whole network, applied to what remains open at
    /// each step instead of once for everything.
    pub fn new(
        query: &ContractionQuery<'_>,
        steps: Vec<ContractionStep>,
    ) -> Result<Self, PlanError> {
        let network = query.network();
        let node_count = network.nodes().len();
        if node_count == 0 {
            return Err(PlanError::EmptyNetwork);
        }
        if steps.is_empty() {
            return Self::validate_trivial(query, node_count);
        }

        let mut available: BTreeMap<Operand, Indices> = (0..node_count)
            .map(|index| (Operand::Input(index), network.nodes()[index].clone()))
            .collect();

        for (step_index, step) in steps.iter().enumerate() {
            if step.operands().is_empty() {
                return Err(PlanError::EmptyStep { step: step_index });
            }
            check_no_repeated_axis(step_index, step.result_axes())?;

            let mut consumed_axes = Vec::new();
            for &operand in step.operands() {
                check_reference_is_valid(step_index, operand, node_count)?;
                let axes = available
                    .remove(&operand)
                    .ok_or(PlanError::AlreadyConsumed {
                        step: step_index,
                        operand,
                    })?;
                consumed_axes.extend(axes.as_slice().iter().copied());
            }

            let expected = surviving_axes(&consumed_axes, query.keep(), &available);
            let mut actual: Vec<Index> = step.result_axes().as_slice().to_vec();
            actual.sort();
            if actual != expected {
                return Err(PlanError::WrongResultAxes {
                    step: step_index,
                    expected,
                    actual: step.result_axes().as_slice().to_vec(),
                });
            }

            available.insert(Operand::Result(step_index), step.result_axes().clone());
        }

        let last = steps.len() - 1;
        if steps[last].result_axes().as_slice() != query.keep().as_slice() {
            return Err(PlanError::WrongOutputAxes {
                expected: query.keep().as_slice().to_vec(),
                actual: steps[last].result_axes().as_slice().to_vec(),
            });
        }
        available.remove(&Operand::Result(last));

        if !available.is_empty() {
            return Err(PlanError::UnconsumedOperands {
                operands: available.into_keys().collect(),
            });
        }

        Ok(Self { steps })
    }

    fn validate_trivial(
        query: &ContractionQuery<'_>,
        node_count: usize,
    ) -> Result<Self, PlanError> {
        if node_count != 1 {
            return Err(PlanError::MissingSteps { nodes: node_count });
        }
        let mut node_axes = query.network().nodes()[0].as_slice().to_vec();
        let mut kept_axes = query.keep().as_slice().to_vec();
        node_axes.sort();
        kept_axes.sort();
        if node_axes != kept_axes {
            return Err(PlanError::MissingSteps { nodes: node_count });
        }
        Ok(Self { steps: Vec::new() })
    }

    /// The steps of this plan, in schedule order.
    #[must_use]
    pub fn steps(&self) -> &[ContractionStep] {
        &self.steps
    }

    /// Whether every step consumes exactly two operands.
    ///
    /// Vacuously true for a plan with no steps: the initial executable
    /// subset is unsliced, pairwise contraction, and a single-node plan that
    /// needed no steps at all trivially satisfies it.
    #[must_use]
    pub fn is_pairwise(&self) -> bool {
        self.steps.iter().all(|step| step.arity() == 2)
    }
}

fn check_reference_is_valid(
    step: usize,
    operand: Operand,
    node_count: usize,
) -> Result<(), PlanError> {
    match operand {
        Operand::Input(index) if index >= node_count => {
            Err(PlanError::UnknownInput { step, index })
        }
        Operand::Result(index) if index >= step => Err(PlanError::ForwardReference { step, index }),
        _ => Ok(()),
    }
}

fn check_no_repeated_axis(step: usize, axes: &Indices) -> Result<(), PlanError> {
    let mut seen = std::collections::BTreeSet::new();
    for axis in axes.as_slice() {
        if !seen.insert(axis.id()) {
            return Err(PlanError::RepeatedResultAxis {
                step,
                id: axis.id(),
            });
        }
    }
    Ok(())
}

/// Which distinct axis ids among `consumed_axes` are still needed once this
/// step's operands are gone: kept by the query, or still present on some
/// operand nothing has consumed yet. Returned sorted, one [`Index`] per id.
fn surviving_axes(
    consumed_axes: &[Index],
    keep: &Indices,
    available: &BTreeMap<Operand, Indices>,
) -> Vec<Index> {
    let mut surviving = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for &axis in consumed_axes {
        if !seen.insert(axis.id()) {
            continue;
        }
        let kept = keep.contains(axis);
        let elsewhere = available
            .values()
            .any(|axes| axes.as_slice().iter().any(|other| other.id() == axis.id()));
        if kept || elsewhere {
            surviving.push(axis);
        }
    }
    surviving.sort();
    surviving
}

/// A schedule that cannot describe a valid contraction of its query.
///
/// Every variant describes a disagreement between a plan's steps and the
/// query/network they claim to schedule, so none of them mention a backend,
/// a native handle or an optimizer.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PlanError {
    /// The network has no nodes at all: the empty product, out of scope here.
    #[error("the network has no nodes; there is nothing to schedule a contraction for")]
    EmptyNetwork,
    /// Steps are required but none were given.
    #[error(
        "the network has {nodes} node(s) but no steps were given; \
         a single node needs no steps only when its axes already equal the query's kept axes"
    )]
    MissingSteps { nodes: usize },
    /// A step referenced an input position outside the network.
    #[error("step {step} references input {index}, outside the network")]
    UnknownInput { step: usize, index: usize },
    /// A step referenced a result that has not been produced yet.
    #[error("step {step} references result {index}, which is not an earlier step")]
    ForwardReference { step: usize, index: usize },
    /// A step referenced an operand a previous step already consumed.
    #[error("step {step} reuses operand {operand:?}, which was already consumed")]
    AlreadyConsumed { step: usize, operand: Operand },
    /// A step consumed no operands.
    #[error("step {step} has no operands")]
    EmptyStep { step: usize },
    /// A step's declared result repeats one axis id.
    #[error("step {step} repeats axis {id} in its declared result")]
    RepeatedResultAxis { step: usize, id: u32 },
    /// A step's declared result does not match the axes the schedule requires.
    #[error(
        "step {step} keeps axes {actual:?}, but the axes still needed at that point are {expected:?}"
    )]
    WrongResultAxes {
        step: usize,
        expected: Vec<Index>,
        actual: Vec<Index>,
    },
    /// The final step's result does not match the query's kept axes.
    #[error("the plan's final axes {actual:?} do not match the query's kept axes {expected:?}")]
    WrongOutputAxes {
        expected: Vec<Index>,
        actual: Vec<Index>,
    },
    /// One or more inputs/results are never consumed and are not the output.
    #[error(
        "operand(s) {operands:?} are never consumed by a later step and are not the plan's output"
    )]
    UnconsumedOperands { operands: Vec<Operand> },
}

#[cfg(test)]
mod tests {
    use super::{ContractionPlan, ContractionStep, Operand, PlanError};
    use crate::{ContractionQuery, Index, Indices, TensorNetwork};

    fn index(id: u32, dim: usize) -> Index {
        Index::new(id, dim).expect("dimension is non-zero")
    }

    fn node(axes: &[Index]) -> Indices {
        Indices::new(axes.to_vec()).expect("axis list is consistent")
    }

    fn network(nodes: Vec<Indices>) -> TensorNetwork {
        TensorNetwork::new(nodes).expect("nodes agree on dimensions")
    }

    fn query<'a>(net: &'a TensorNetwork, keep: &[Index]) -> ContractionQuery<'a> {
        ContractionQuery::new(net, node(keep)).expect("kept axes are in the network")
    }

    /// `A[i,j] B[j,k]`, kept as `[i, k]`: one binary step, `j` disappears.
    fn matmul() -> (TensorNetwork, Index, Index, Index) {
        let i = index(0, 2);
        let j = index(1, 3);
        let k = index(2, 4);
        (network(vec![node(&[i, j]), node(&[j, k])]), i, j, k)
    }

    #[test]
    fn a_binary_step_consuming_both_inputs_matches_the_query() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        let plan = ContractionPlan::new(
            &q,
            vec![ContractionStep::new(
                vec![Operand::Input(0), Operand::Input(1)],
                node(&[i, k]),
            )],
        )
        .expect("valid pairwise plan");
        assert!(plan.is_pairwise());
        assert_eq!(plan.steps().len(), 1);
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn a_hyperedge_survives_an_intermediate_step_until_its_last_use() {
        // A[i,j] B[j,k] C[j,l], kept as [i,k,l]: j is a three-way hyperedge.
        // Combine A and B first; j must survive because C still carries it.
        let i = index(0, 2);
        let j = index(1, 3);
        let k = index(2, 4);
        let l = index(3, 5);
        let net = network(vec![node(&[i, j]), node(&[j, k]), node(&[j, l])]);
        let q = query(&net, &[i, k, l]);
        let plan = ContractionPlan::new(
            &q,
            vec![
                ContractionStep::new(vec![Operand::Input(0), Operand::Input(1)], node(&[i, j, k])),
                ContractionStep::new(
                    vec![Operand::Result(0), Operand::Input(2)],
                    node(&[i, k, l]),
                ),
            ],
        )
        .expect("j survives the first step because the third node still needs it");
        assert_eq!(plan.steps().len(), 2);
    }

    #[test]
    fn a_unary_step_can_extract_a_diagonal() {
        // One node carrying axis i twice; kept as [i] is the diagonal.
        let i = index(0, 4);
        let net = network(vec![node(&[i, i])]);
        let q = query(&net, &[i]);
        let plan = ContractionPlan::new(
            &q,
            vec![ContractionStep::new(vec![Operand::Input(0)], node(&[i]))],
        )
        .expect("a unary step may extract a diagonal");
        assert_eq!(plan.steps()[0].arity(), 1);
        assert!(!plan.is_pairwise());
    }

    #[test]
    fn a_single_node_matching_keep_needs_no_steps() {
        let i = index(0, 2);
        let j = index(1, 3);
        let net = network(vec![node(&[i, j])]);
        let q = query(&net, &[j, i]);
        let plan = ContractionPlan::new(&q, Vec::new()).expect("a permutation needs no step");
        assert!(plan.steps().is_empty());
        assert!(plan.is_pairwise(), "vacuously true with no steps");
    }

    #[test]
    fn a_diagonal_cannot_be_taken_with_no_steps() {
        let i = index(0, 4);
        let net = network(vec![node(&[i, i])]);
        let q = query(&net, &[i]);
        assert_eq!(
            ContractionPlan::new(&q, Vec::new()),
            Err(PlanError::MissingSteps { nodes: 1 })
        );
    }

    #[test]
    fn a_network_with_no_nodes_is_out_of_scope() {
        let net = network(vec![]);
        let q = query(&net, &[]);
        assert_eq!(
            ContractionPlan::new(&q, Vec::new()),
            Err(PlanError::EmptyNetwork)
        );
    }

    #[test]
    fn steps_are_required_when_more_than_one_node_remains() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        assert_eq!(
            ContractionPlan::new(&q, Vec::new()),
            Err(PlanError::MissingSteps { nodes: 2 })
        );
    }

    #[test]
    fn a_step_cannot_reference_an_input_outside_the_network() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        let out_of_range = Operand::Input(2);
        assert_eq!(
            ContractionPlan::new(
                &q,
                vec![ContractionStep::new(
                    vec![Operand::Input(0), out_of_range],
                    node(&[i, k]),
                )],
            ),
            Err(PlanError::UnknownInput { step: 0, index: 2 })
        );
    }

    #[test]
    fn a_step_cannot_reference_its_own_or_a_later_result() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        assert_eq!(
            ContractionPlan::new(
                &q,
                vec![ContractionStep::new(
                    vec![Operand::Input(0), Operand::Result(0)],
                    node(&[i, k]),
                )],
            ),
            Err(PlanError::ForwardReference { step: 0, index: 0 })
        );
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn a_step_cannot_reuse_an_already_consumed_operand() {
        let i = index(0, 2);
        let j = index(1, 3);
        let k = index(2, 4);
        let l = index(3, 5);
        let net = network(vec![node(&[i, j]), node(&[j, k]), node(&[k, l])]);
        let q = query(&net, &[i, l]);
        assert_eq!(
            ContractionPlan::new(
                &q,
                vec![
                    ContractionStep::new(vec![Operand::Input(0), Operand::Input(1)], node(&[i, k])),
                    ContractionStep::new(vec![Operand::Input(1), Operand::Input(2)], node(&[i, l])),
                ],
            ),
            Err(PlanError::AlreadyConsumed {
                step: 1,
                operand: Operand::Input(1)
            })
        );
    }

    #[test]
    fn a_step_must_have_at_least_one_operand() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        assert_eq!(
            ContractionPlan::new(&q, vec![ContractionStep::new(Vec::new(), node(&[i, k]))]),
            Err(PlanError::EmptyStep { step: 0 })
        );
    }

    #[test]
    fn a_step_result_cannot_repeat_an_axis() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        assert_eq!(
            ContractionPlan::new(
                &q,
                vec![ContractionStep::new(
                    vec![Operand::Input(0), Operand::Input(1)],
                    node(&[i, i]),
                )],
            ),
            Err(PlanError::RepeatedResultAxis { step: 0, id: 0 })
        );
    }

    #[test]
    fn a_step_cannot_drop_an_axis_the_query_still_needs() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        assert_eq!(
            ContractionPlan::new(
                &q,
                vec![ContractionStep::new(
                    vec![Operand::Input(0), Operand::Input(1)],
                    node(&[i]),
                )],
            ),
            Err(PlanError::WrongResultAxes {
                step: 0,
                expected: vec![i, k],
                actual: vec![i],
            })
        );
    }

    #[test]
    fn a_step_cannot_keep_an_axis_nothing_needs_anymore() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        let j = index(1, 3);
        assert_eq!(
            ContractionPlan::new(
                &q,
                vec![ContractionStep::new(
                    vec![Operand::Input(0), Operand::Input(1)],
                    node(&[i, k, j]),
                )],
            ),
            Err(PlanError::WrongResultAxes {
                step: 0,
                expected: vec![i, k],
                actual: vec![i, k, j],
            })
        );
    }

    #[test]
    fn the_final_step_must_match_the_query_exactly_in_order() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        assert_eq!(
            ContractionPlan::new(
                &q,
                vec![ContractionStep::new(
                    vec![Operand::Input(0), Operand::Input(1)],
                    node(&[k, i]),
                )],
            ),
            Err(PlanError::WrongOutputAxes {
                expected: vec![i, k],
                actual: vec![k, i],
            })
        );
    }

    #[test]
    fn every_input_must_be_consumed_by_some_step() {
        // A[i,j] B[j,k], kept as [i,k]: an unrelated node D[m] never shares an
        // axis with anything, so nothing forces it into any step, yet it must
        // still be consumed (or be the output) for the plan to be complete.
        let (net_base, i, _, k) = matmul();
        let m = index(3, 6);
        let mut nodes = net_base.nodes().to_vec();
        nodes.push(node(&[m]));
        let net = network(nodes);
        let q = query(&net, &[i, k]);
        assert_eq!(
            ContractionPlan::new(
                &q,
                vec![ContractionStep::new(
                    vec![Operand::Input(0), Operand::Input(1)],
                    node(&[i, k]),
                )],
            ),
            Err(PlanError::UnconsumedOperands {
                operands: vec![Operand::Input(2)],
            })
        );
    }

    #[test]
    fn a_plan_survives_reconstruction_against_a_fresh_but_identical_query() {
        let (net, i, _, k) = matmul();
        let q = query(&net, &[i, k]);
        let steps = vec![ContractionStep::new(
            vec![Operand::Input(0), Operand::Input(1)],
            node(&[i, k]),
        )];
        let plan = ContractionPlan::new(&q, steps.clone()).expect("valid the first time");

        let fresh_net = network(net.nodes().to_vec());
        let fresh_q = query(&fresh_net, &[i, k]);
        let reimported =
            ContractionPlan::new(&fresh_q, steps).expect("valid again against a fresh network");
        assert_eq!(plan, reimported);
    }
}

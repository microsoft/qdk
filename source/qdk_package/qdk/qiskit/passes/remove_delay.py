# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from qiskit.dagcircuit import DAGCircuit
from qiskit.transpiler.basepasses import TransformationPass
from qiskit.transpiler.passes.utils import control_flow


class RemoveDelays(TransformationPass):
    """Return a circuit with any delay removed.

    This transformation is not semantics preserving.
    """

    @control_flow.trivial_recurse
    def run(self, dag: DAGCircuit) -> DAGCircuit:
        """Run the RemoveDelays pass on `dag`."""

        dag.remove_all_ops_named("delay")

        return dag

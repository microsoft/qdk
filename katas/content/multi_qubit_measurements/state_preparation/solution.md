While it is possible to prepare the state $\ket \psi$ directly using unitary rotations, it is simpler to use post-selection for preparing it.

Initially we will prepare an equal superposition of all basis states on the first two qubits by applying the **H** gate to each of them, and allocate an extra qubit in the $\ket{0}$ state:
$$\frac{1}{2} \big(|00\rangle + |01\rangle + |10\rangle + |11\rangle\big) \otimes \ket 0$$

The state of the first two qubits is a superposition of the state we want to prepare and the $|11\rangle$ state that we would like to discard.

Now, we want to separate the first three basis states from the last one and to store this separation in the extra qubit.
For example, we can keep the state of the extra qubit $|0\rangle$ for the basis states that we want to keep, and switch it to $|1\rangle$ for the basis states that we would like to discard.
A $CCNOT$ gate can do this, with the first two qubits used as control qubits and the extra qubit as target.
When the gate is applied, the state of the extra qubit will only change to $|1\rangle$ if both control qubits are in the $|11\rangle$ state, which marks exactly the state that we want to discard:

$$CCNOT\frac{1}{2} \big(|00\textbf{0}\rangle + |01\textbf{0}\rangle + |10\textbf{0}\rangle + |11\textbf{0}\rangle\big) =
\frac{1}{2}\big(|00\rangle + |01\rangle + |10\rangle \big) \otimes |\textbf{0}\rangle + \frac{1}{2}|11\rangle \otimes |\textbf{1}\rangle $$

Finally, we measure just the extra qubit; this causes a partial collapse of the system to the state defined by the measurement result:
* If the result is $|0\rangle$, the first two qubits collapse to a state that is a linear combination of basis states which had the extra qubit in state $|0\rangle$, i.e., they end up in the target state $\frac{1}{\sqrt3}\big(|00\rangle + |01\rangle + |10\rangle\big)$.
* If the result is $|1\rangle$, the first two qubits collapse to a state $|11\rangle$, so our goal is not achieved. The good thing is, this only happens in 25% of the cases, and we can just reset our qubits to the $|00\rangle$ state and try again.

> Q# has a built-in <a href="https://learn.microsoft.com/azure/quantum/user-guide/language/expressions/conditionalloops#repeat-expression" target="_blank">repeat-until-success (RUS) loop</a>, which comes in handy in this case.
> * We will describe the main operations (applying $H$ and $CCNOT$ gates and the measurement) in the `repeat` part of the loop, which specifies its body.  
> * `until` section specifies the condition which will break the loop. In this case the result of the measurement needs to be `Zero` to indicate our success.  
> * Finally, the `fixup` section allows us to clean up the results of the loop body execution before trying again if the success criteria is not met. In this case we reset the first two qubits back to the $|00\rangle$ state.

@[solution]({
    "id": "multi_qubit_measurements__state_preparation_solution",
    "codePath": "Solution.qs"
})

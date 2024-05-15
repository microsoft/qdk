There are multiple ways to approach this problem. In our first solution, we are going to use a recursive approach that was hinted at in the task.

Let's denote the required state on $N$ qubits as $|P_{N,0}\rangle$ for `parity = 0` and $|P_{N,1}\rangle$ for `parity = 1`. We can group the $2^{N-1}$ basis states included in the state $|P_{N,0}\rangle$ by their first bit ($0$ or $1$) and write the state as follows:

$$|P_{N,0}\rangle = \frac{1}{\sqrt{2^{N-1}}} \sum_{k : k \text{ has parity }0} |k\rangle_N = $$
$$= \frac{1}{\sqrt{2^{N-1}}} \big( |0\rangle \otimes \sum_{k' : k' \text{ has parity }0} |k'\rangle_{N-1} + |1\rangle \otimes \sum_{k'' : k'' \text{ has parity }1} |k''\rangle_{N-1} \big) = $$
$$= \frac{1}{\sqrt{2}} \big( |0\rangle \otimes |P_{N-1,0}\rangle + |1\rangle \otimes |P_{N-1,1}\rangle \big)$$
We can consider the expression for $|P_{N,1}\rangle$ in a similar manner, and get a unified expression for both states:
$$|P_{N,p}\rangle = \frac{1}{\sqrt{2}} \big( |0\rangle \otimes |P_{N-1,p}\rangle + |1\rangle \otimes |P_{N-1,1-p}\rangle \big)$$

Now we can use this expression to prepare the state using a recursive approach we've seen before:

1. Apply $H$ gate to the first qubit to prepare state $\frac{1}{\sqrt{2}} ( |0\rangle + |1\rangle ) \otimes |0\rangle_{N-1}$.
2. Apply the controlled variant of procedure of preparing $|P_{N-1,p}\rangle$ on the last $N-1$ qubits, with the first qubit in the $|0\rangle$ state as the control.
3. Apply the controlled variant of procedure of preparing $|P_{N-1,1-p}\rangle$ on the last $N-1$ qubits, with the first qubit in the $|1\rangle$ state as the control.

> Q# library function [`ApplyControlledOnInt`](https://learn.microsoft.com/qsharp/api/qsharp-lang/microsoft.quantum.canon/applycontrolledonint) allows to do that easily.

4. The base of recursion is preparing the states for $N = 1$:
* For `parity = 0`, there is one single-qubit state with this parity: $|0\rangle$ (no action required to prepare).
* For `parity = 1`, there is one single-qubit state with this parity: $|1\rangle$ (apply $X$ gate to prepare).

@[solution]({ "id": "superposition__parity_bitstrings_solution_a", "codePath": "./SolutionA.qs" })

In the second solution, we'll use post-selection. We start by preparing an equal superposition of all basis states and allocating an extra qubit.

This time we use the extra qubit to calculate the parity of the input state: applying a series of CNOT gates, each one with one of the input qubits as control and the extra qubit as a target will compute the parity of the state.

Now we measure the extra qubit: if the measurement result matches our parity, we're done — the input qubits collapsed to an equal superposition of all states that have this parity. If the measurement result is the opposite, we can retry the whole process.

We can avoid retrying the state preparation if our measurement result doesn't match the required parity: notice that applying an X gate to any one of the qubits changes the parity of each basis state to the opposite one, and thus converts the state we got to the state we need.

@[solution]({ "id": "superposition__parity_bitstrings_solution_b", "codePath": "./SolutionB.qs" })

Yet another way of getting the desired superposition could be preparing the mix of all possible basis states for all qubits iteratively, keeping the parity on each step.

We start by preparing a superposition of all basis states with parity $0$. To achieve that, we loop through all the qubits except the first one and prepare all of them in equal superposition. In order to maintain the parity of the basis states involved, we use the first qubit and conditionally flip its state using a CNOT gate with each next qubit as the control, so that every time there is a $|1\rangle$ state in the chain, we get back to an even number of $|1\rangle$ states in that basis state.

For example, after the first loop iteration we get the state $\frac12(|00\rangle + |11\rangle)$. After the second iteration we get the state

$$CNOT_{2,0} \frac1{\sqrt2}(|00\rangle + |11\rangle) \otimes \frac1{\sqrt2}(|0\rangle + |1\rangle) = $$
$$= CNOT_{2,0} \frac12(|000\rangle + |\textbf{0}0\textbf{1}\rangle + |110\rangle + |\textbf{1}1\textbf{1}\rangle) = $$
$$= \frac12(|000\rangle + |101\rangle + |110\rangle + |011\rangle)$$

After the loop we will have a superposition of all possible basis states with even number of $|1\rangle$s.
Then, if `parity` is equal to 1 and we want an odd number of $|1\rangle$s, we just flip the state of the first qubit again (or any qubit).

For example, if the input has 3 qubits, after the for loop we will have a superposition of 4 basis states:

$$\frac12(|000\rangle + |101\rangle + |110\rangle + |011\rangle)$$

If `parity = 0`, we are done, having even numbers of $|1\rangle$s. If `parity = 1`, we flip the state of the first qubit, getting the desired result:

$$\frac12(|100\rangle + |001\rangle + |010\rangle + |111\rangle)$$

@[solution]({ "id": "superposition__parity_bitstrings_solution_c", "codePath": "./SolutionC.qs" })

**Input**: nine qubits that are either in the state $\ket{\psi_L} = \alpha \ket{0_L} + \beta \ket{1_L}$ (a valid code word of the Shor code) or the state that is a code word with $X$, $Y$, or $Z$ error occurring on one of the qubits.

**Goal**: determine whether an error has occurred, and if so, what type and on which qubit. 
The return value is a tuple of two elements, describing the detected error as follows:

- The first element of the return is an `Int` - the index of the qubit on which the error occurred, or $-1$ if no error occurred.
- The second element of the return is a `Pauli` indicating the type of the error (`PauliX`, `PauliY`, or `PauliZ`).
If no error occurred, the second element of the return can be any value, it isn't validated.
- In case of a single $Z$ error, the qubit on which it occurred cannot be identified uniquely. 
In this case, the return value should be the index of the triplet of qubits in which the error occurred ($0$ for qubits $0 \ldots 2$, $1$ for qubits $3 \ldots 5$, and $2$ for qubits $6 \ldots 8$).

Example return values:

<table>
<tr>
<th>Error</th>
<th>Return value</th>
</tr>
<tr>
<td>No error</td>
<td>(-1, PauliI)</td>
</tr>
<tr>
<td>$X$ error on qubit $0$</td>
<td>(0, PauliX)</td>
</tr>
<tr>
<td>$Y$ error on qubit $4$</td>
<td>(1, PauliY)</td>
</tr>
<tr>
<td>$Z$ error on qubit $8$ (last triplet)</td>
<td>(2, PauliZ)</td>
</tr>
</table>

The state of the qubits after your operation is applied shouldn't change.

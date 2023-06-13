## Measurements in arbitrary orthogonal bases
It is possible to measure a qubit in orthogonal bases other than the Pauli bases. Suppose one wants to measure a qubit in an orthonormal basis $\ket {b_0}$ and $\ket {b_1}$. Let the state of the qubit be represented by the normalized vector $\ket \psi$. Then, one can always express the state in terms of the basis vectors $\ket{b_0}$ and $\ket{b_1}$, i.e., there exist complex numbers $c_0, c_1$, such that 
$$
\ket \psi = c_0 \ket {b_0} + c_1 \ket {b_1}.
$$
The rule for obtaining the probabilities of measurement outcomes is exactly the same as that for the computation basis measurement. For a measurement in a $\{ b_0, b_1\}$ basis we get
- Outcome $b_0$ with probability $|c_0|^2$, and the post-measurement state of the qubit $\ket {b_0}$;
- Outcome $b_1$ with probability $|c_1|^2$, and the post-measurement state of the qubit $\ket {b_1}$.

This can be summarized in the following table:
<table style="border:1px solid">
    <col width=150>
    <col width=150>
    <col width=150>
    <tr>
        <th style="text-align:center; border:1px solid">Measurement outcome</th>
        <th style="text-align:center; border:1px solid">Probability of outcome</th>
        <th style="text-align:center; border:1px solid">State after measurement</th>
    </tr>
    <tr>
        <td style="text-align:center; border:1px solid">$b_0$</td>
        <td style="text-align:center; border:1px solid">$|c_0|^2$</td>
        <td style="text-align:center; border:1px solid">$\ket{b_0}$</td>
    </tr>
    <tr>
        <td style="text-align:center; border:1px solid">$b_1$</td>
        <td style="text-align:center; border:1px solid">$|c_1|^2$</td>
        <td style="text-align:center; border:1px solid">$\ket{b_1}$</td>
    </tr>
    
</table>

As before, the assumption of $\ket \psi$ being normalized is important, since it guarantees that the two probabilities add to $1$.

> As you may recall, a [global phase](../Qubit/Qubit.ipynb#Relative-and-Global-Phase) is said to be hidden or unobservable. 
This is explained by the fact that global phases have no impact on quantum measurements. For example, consider two isolated qubits which are in (normalized) states $\ket \psi$ and $e^{i\theta}\ket \psi$. 
If both are measured in an orthogonal basis $\{ \ket{b_0},\ket{b_1}\}$, the probabilities of measuring $b_0$ or $b_1$ are identical in both cases, since $|\bra{b_i}\ket{\psi}|^2 = |\bra{b_i}e^{i\theta}\ket{\psi}|^2  $. 
Similarly, for either qubit, if $b_i$ is the measurement outcome, the post-measurement state of the qubit is $\ket{b_i}$ for both qubits. Hence, the measurements are independent of the global phase $\theta$.

> ### Measurements as projection operations
Quantum measurements are modeled by orthogonal projection operators. An orthogonal projection operator is a matrix $P$ which satisfies the following property:
$$
P^2 = P^\dagger = P.
$$
(As usual, the $\dagger$ symbol denotes conjugate transposition.) 
>
> Using the [ket-bra representation](../SingleQubitGates/SingleQubitGates.ipynb#Ket-bra-Representation), one can represent a projection matrix in the Dirac notation.
For example, one may construct a projector onto the $\ket{0}$ subspace as:
$$
P = \ket 0 \bra 0 \equiv \begin{bmatrix} 1 & 0 \\ 0 & 0\end{bmatrix}.
$$
>
>A measurement in an orthogonal basis $\{ \ket{b_0}, \ket{b_1}\}$ is described by a pair of projectors $P_0 = \ket{b_0}\bra{b_0}$ and $P_1 = \ket{b_1}\bra{b_1}$. Since $\ket{b_0}$ and $\ket{b_1}$ are orthogonal, their projectors are also orthogonal, i.e., $P_0 P_1 = P_1 P_0 = 0$. The rules for measurements in this basis can then be summarized as follows: 
- Measuring a qubit in a state $\ket \psi$ is done by picking one of these projection operators at random.
- Projection $P_0$ is chosen with probability $|P_0 \ket{\psi}|^2$, and the projector $P_1$ is chosen with probability $|P_1\ket{\psi}|^2$.
- If projector $P_0$ is chosen, the post-measurement state of the qubit is given by
$$
\frac1{|P_0 \ket{\psi}|}P_0 \ket\psi,
$$
and similarly for $P_1$.
>
>Although this formalism looks different from the previous sections, it is in fact equivalent. If $\ket \psi = c_0 \ket{b_0} + c_1 \ket{b_1}$, we have 
$$
P_0 \ket \psi = c_0 \ket{b_0}, \text{so that } | P_0\ket \psi| = c_0,
$$
and similarly, 
$$
P_1 \ket \psi = c_1 \ket{b_1}, \text{so that } |P_1\ket \psi| = c_1.
$$
>
>Thus, as before, the probability of measuring $b_0$ is $|P_0\ket\psi|^2 = |c_0|^2$, and the probability of measuring $b_1$ is $|P_1\ket\psi|^2 = |c_1|^2$. Similarly, one can verify that the post-measurement outcomes are also $\ket{b_0}$ and $\ket{b_1}$ respectively (up to unobservable global phases).
>
>Although the projector formalism for single-qubit systems may seem superfluous, its importance will become clear later, while considering measurements for multi-qubit systems.

### Arbitrary basis measurements implementation
In the previous section, we discussed measurements in Pauli bases using the built-in `Measure` operation. We will now show that using just unitary rotation matrices and computation basis measurements it is always possible to measure a qubit in any orthogonal basis. 

Consider a state $ \ket \psi = c_0 \ket {b_0} + c_1 \ket {b_1} $ which we would like to measure in an orthonormal basis $\{ \ket{b_0}, \ket{b_1}\}$. First, we construct the following unitary matrix:
$$
U = \ket{0} \bra{b_0} + \ket{1} \bra{b_1}
$$

The conjugate transpose of this unitary is the operator 
$$
U^\dagger = \ket{b_0} \bra{0} + \ket{b_1} \bra{1}
$$

(One may verify that $U$ is indeed a unitary matrix, by checking that $U^\dagger U = U U^\dagger = I$.)

Note that the effect of these matrices on the two bases is the following:
\begin{align}
U\ket{b_0} &= \ket{0}; & U\ket{b_1} &= \ket{1}\\
U^\dagger \ket{0} &= \ket{b_0}; & U^\dagger \ket 1 &= \ket{b_1}.
\end{align}

In order to implement a measurement in the $\{ \ket{b_0}, \ket{b_1}\}$ basis, we do the following:

1. Apply $U$ to $\ket \psi$.  
   The resulting state is $U\ket \psi = c_0 \ket 0 + c_1 \ket 1 $.
2. Measure the state $U\ket{\psi}$ in the computational basis.  
   The outcomes $0$ and $1$ occur with probabilities $|c_0|^2$ and $|c_1|^2$.
3. Apply $U^\dagger$ to the post-measurement state.  
   This transforms the states $\ket 0$ and $\ket 1$ to the states $\ket{b_0}$ and $\ket{b_1}$, respectively.

Thus, $b_0$ and $b_1$ are measured with probabilities $|c_0|^2$ and $|c_1|^2$, respectively, with the end state being $\ket{b_0}$ and $\ket{b_1}$ - which is exactly the measurement we want to implement. 

This procedure can be used to distinguish arbitrary orthogonal states as well, as will become clear from the following exercises.


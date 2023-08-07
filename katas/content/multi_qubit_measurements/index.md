# Measurements in systems with multiple qubits

@[section]({
    "id": "multi_qubit_measurements_overview",
    "title": "Overview"
})

In the previous kata, we discussed the concept of measurements done on single-qubit systems.
Building upon those ideas, this kata will introduce you to measurements done on multi-qubit systems, and how to implement such measurements in Q#.
This will include measuring a single qubit in a multi-qubit system, as well as measuring multiple qubits simultaneously.

We recommend to go through the kata that introduces single-qubit system measurements before starting this one.
$\renewcommand{\ket}[1]{\left\lvert#1\right\rangle}$
$\renewcommand{\bra}[1]{\left\langle#1\right\rvert}$

You should be familiar with the following concepts before tackling the Single-Qubit System Measurements tutorial (and this workbook):

1. Basic linear algebra
2. Single and multi-qubit systems
3. Single and multi-qubit gates
   $\renewcommand{\ket}[1]{\left\lvert#1\right\rangle}$
   $\renewcommand{\bra}[1]{\left\langle#1\right\rvert}$

@[section]({
    "id": "multi_qubit_measurements_types_of_measurements",
    "title": "Types of measurements"
})

There are several types of measurements you can perform on an $n$-qubit system ($n>1$):

- Measuring all the qubits simultaneously in an orthogonal basis ($2^n$ possible outcomes). As we shall see below, this is a direct generalization of orthogonal basis measurements done in single-qubit systems introduced in the previous tutorial.
- Partial measurement: measuring $m$ qubits out of $n$, for $m<n$ ($2^m$ possible outcomes). Partial measurements involve a partial collapse of the system's wave function, since only some of the qubits are measured.
- Joint measurement: measuring a joint property of all $n$ qubits ($2$ possible outcomes).

We will discuss these concepts in the same order as in the list above.

## Full measurements: measurements in multi-qubit bases

Consider a system consisting of $n\geq1$ qubits. The wave function of such a system belongs to a vector space of dimension $2^n$. Thus, the vector space is spanned by an orthogonal basis, such as the computational basis which consists of the vectors $|0\dotsc0\rangle, \dotsc, |1\dotsc 1\rangle$. For generality, we consider an arbitrary orthonormal basis, which we denote by $\{ |b_0\rangle, |b_1\rangle, \dotsc, |b_{2^n-1}\rangle \}$.

Then, the state $|\psi\rangle$ of the multi-qubit system can be expressed as a linear combination of the $2^n$ basis vectors $|b_i\rangle$. That is, there exist complex numbers $c_0,c_1,\dotsc, c_{2^n-1}$ such that

$$
|\psi\rangle = \sum_{i=0}^{2^n-1} c_i|b_i\rangle \equiv \begin{pmatrix}c_0 \\ c_1 \\ \vdots \\ c_{2^n-1}\end{pmatrix}.
$$

In line with the usual convention, we choose the wave function to be normalized, so that $|c_0|^2 + \dotsc + |c_{2^n-1}|^2 =1$. Then, a quantum measurement in the $\{ |b_0\rangle, |b_1\rangle, \dotsc, |b_{2^n-1}\rangle \}$ basis satisfies the following rules:

- The measurement outcome $b_i$ occurs with probability $|c_i|^2$.
- Whenever the measurement outcome is $b_i$, the wave function collapses to the state $|b_i\rangle$. That is, the post-measurement state of the system is equal to $|b_i\rangle$.

This can be summarized in the following table:

<table>
    <tr>
        <th>Measurement outcome</th>
        <th>Probability of outcome</th>
        <th>State after measurement</th>
    </tr>
    <tr>
        <td>$b_i$</td>
        <td>$|c_i|^2$</td>
        <td>$\ket{b_i}$</td>
    </tr>
</table>


> Similar to measurements in single-qubit systems, the assumption of normalization of the original wave function is required in order to ensure that the sum of all the outcome probabilities is 1.

@[question]({
    "id": "multi_qubit_probabilities_1",
    "descriptionPath": "./multi_qubit_probabilities_1/index.md",
    "answerPath": "./multi_qubit_probabilities_1/solution.md"
})

## Measuring each qubit in a system one after another
As described in the previous sections, in theory it is possible to measure all the qubits in an $n$-qubit system simultaneously in an orthogonal basis. The post-measurement state of the qubits is then exactly one of the $2^n$ possible basis states.

In practice, this is implemented by measuring all the qubits one after another. For example, if one wants to measure a two-qubit system in the computational basis, one can implement this by first measuring the first qubit in the computational basis to obtain $0$ or $1$, and then measuring the second qubit in the computational basis. This can result in one of the four possible outcomes: $00, 01, 10, 11$.

This can be generalized to measurements in other bases, such as the 2-qubit Pauli X basis $\ket{++}, \ket{+-}, \ket{-+}, \ket{--}$, and the bases for larger numbers of qubits.

> Note that measuring all qubits one after another can only be used to measure in orthogonal bases $\{ \ket{b_i}\}$ such that each $\ket{b_i}$ is a 'tensor product state'. That is, each $\ket{b_i}$ must be of the form $\ket{v_0} \otimes \ket{v_1} \dotsc \otimes \ket{v_{n-1}}$, with each $\ket{v_j}$ being a single-qubit basis state.
For example, for the 2-qubit Pauli X basis $\ket{++}, \ket{+-}, \ket{-+}, \ket{--}$ each basis state is a tensor product of states $\ket{+}$ and $\ket{-}$, which form a single-qubit basis state.
>
> Measuring in orthogonal bases which contain states which are not tensor product states, such as the Bell basis, are trickier to implement, and require appropriate unitary rotations in addition to measuring all qubits one after another.
> We will not discuss such measurements in this tutorial.
> You can find examples of such measurements and their implementations in the [Measurements kata](../../Measurements/Measurements.ipynb).
>
> If we restrict ourselves to measurements in tensor product states, the distinction between measuring all the qubits simultaneously versus one after another is not important for an ideal quantum computer: in terms of the outcomes and measurement probabilities, both are identical. Furthermore, as long as all the qubits are measured, the sequence in which they are measured is also inconsequential. These factors can be  important in the case of real quantum computers with imperfect qubits, but we restrict the discussion to ideal systems in this tutorial.

@[section]({
    "id": "multi_qubit_measurements_measurement_statistics",
    "title": "Measurement statistics for  qubit-by-qubit full measurement"
})

This demo illustrates the equivalence of the measurement probabilities for simultaneous measurement on all qubits, and measurements on each of the qubits executed one after another. Using the wave function from exercise 1 above as an example, we show that the measurement probabilities obtained using the `M` operation in Q# are the same as those expected theoretically for exercise 1.

The simulated probabilities will be different for each run of `DemoBasisMeasurement`. The simulated and theoretical probabilities are not expected to be identical, since measurements are probabilistic. However, we expect the values to be similar, and the simulated probabilities to approach the theoretical probabilities as the parameter `numRuns` is increased.

@[example]({
    "id": "measuring_one_at_a_time",
    "codePath": "./measuring_one.qs"
})

## Using full measurements to identify the state of the system

Full measurements can also be used to identify the state of the system, if it is guaranteed to be in one of several possible orthogonal states.

@[exercise]({
    "id": "full_measurements",
    "title":  "Distinguish four basis states",
    "descriptionPath": "./full_measurements/index.md",
    "placeholderSourcePath": "./full_measurements/placeholder.qs",
    "solutionPath": "./full_measurements/solution.md",
    "codePaths": [
        "./full_measurements/verify.qs",
        "./common.qs",
        "../KatasLibrary.qs"
    ]
})

@[section]({
    "id": "multi_qubit_measurements_partial_measurements",
    "title": "Partial Measurements"
})

For a system with $n>1$ qubits, it is possible to measure $m<n$ qubits one after another. The number of measurement outcomes is then $2^m$ instead of $2^n$. The probabilities of each of the outcomes and the post-measurement states of the qubits can be found using the projection formalism for measurements.

First, we recall the concept of projection operators introduced in the single-qubit systems measurements kata. Measurements are modeled by orthogonal projection operators - matrices that satisfy
$$
P^2 = P^\dagger = P.
$$
Consider an $n$-qubit system in a state $|\psi\rangle$, for which the first $m<n$ qubits are measured in an orthogonal basis $\{ |b_0\rangle , |b_1\rangle, \dotsc, |b_{2^m-1}\rangle\}$ corresponding to the $m$ qubits being measured. Then we define $2^m$ projectors corresponding to each of the $|b_i\rangle$ states as
$$
P_i = |b_i\rangle \langle b_i| \otimes \mathbb{1}_{n-m},
$$
where $\mathbb{1}_{n-m}$ is the identity operator over the remaining $(n-m)$ qubits.
> The symbol $\otimes$ represents the tensor product or the Kronecker product of two matrices. It is different from the usual matrix multiplication (see the [Linear Algebra tutorial](../LinearAlgebra/LinearAlgebra.ipynb#Tensor-Product) for a refresher). In the current context, $|b_i\rangle \langle b_i| \otimes \mathbb{1}_{n-m}$ simply means that the operator $|b_i\rangle \langle b_i|$ acts only on the $m$ qubits being measured, while the effect of $P_i$ on the remaining qubits is $\mathbb{1}_{n-m}$, i.e., the identity operator.

Analogous to the case for measurements for single-qubit systems, the rules for partial measurement probabilities and outcomes can be summarized as follows:
- When a measurement is done, one of these projectors is chosen randomly. The probability of choosing projector $P_i$ is $\big|P_i|\psi\rangle\big|^2$.
- If the projector $P_i$ is chosen, the measurement outcome is $b_i$, and the state of the system after the measurement is given by
$$
\frac{P_i |\psi\rangle}{\big|P_i |\psi\rangle\big|}.
$$

For example, consider a two-qubit system in the state $\ket \psi = \frac{1}{\sqrt{2}}\ket{01} - \frac{1}{\sqrt 2}\ket{10}$. Consider a measurement of the first qubit in the computational basis, i.e., in the $\{\ket 0 , \ket 1 \}$ basis. Then, we have two projectors that represent this measurement:
\begin{align*}
P_0 &= \ket 0\bra 0 \otimes \mathbb{1},\\
P_1 &= \ket 1 \bra 1 \otimes \mathbb{1}.
\end{align*}

The action of $P_0$ on $\ket \psi$ is
\begin{align*}
P_0 \ket \psi &= \left(\ket 0\bra 0 \otimes \mathbb{1}\right) \frac{1}{\sqrt 2}\big(\ket{01} - \ket{10}\big) = \\
              &= \frac{1}{\sqrt 2} \big( \ket 0\bra 0 0\rangle \otimes \mathbb{1} \ket{1} - \ket 0 \bra 0 1\rangle \otimes \mathbb{1} \ket 0 \big) = \\
              &= \frac{1}{\sqrt 2} \ket{01}.
\end{align*}

Similarly, we obtain
$$
P_1 \ket\psi = -\frac{1}{\sqrt 2} \ket{10}.
$$

Clearly, we have $\big|P_0 \ket \psi\big| = \big|P_1 \ket \psi\big| = \frac{1}{2}$ in this case. Thus, the probabilities of measuring $0$ and $1$ are both $0.5$, with the post-measurement states of system being $\ket{01}$ and $\ket{10}$, respectively.

> Similar to the case of single-qubit system measurements, the applicability of the formalism above requires the state of the multi-qubit system, $\ket \psi$, to be normalized. This is required to ensure that all the probabilities of individual outcomes add up to 1.

@[question]({
    "id": "partial_measurements",
    "descriptionPath": "./partial_measurements/index.md",
    "answerPath": "./partial_measurements/solution.md"
})

@[section]({
    "id": "multi_qubit_measurements_measurement_statistics_for_partial_measurements",
    "title": "Measurement statistics for  partial measurement"
})

Using the `M` operation in Q#, we demonstrate that the simulated outcome probabilities and post-measurement outcomes match the theoretical values obtained using the projection operators as described above. We use the Hardy state from Exercise 4 with a computational basis measurement on the first qubit for this purpose.

The simulated and theoretical measurement probabilities are not expected to match exactly, but should be close to each other, since measurement is probabilistic. However, the post-measurement states from the simulation should match the expected states for Exercise 4 precisely, since partial state collapse is not a probabilistic process.

@[example]({
    "id": "partial_measurements_demo",
    "codePath": "./partial_measurements_demo.qs"
})

## Using partial measurements to identify the state of the system

In certain situations, it is possible to distinguish between orthogonal states of multi-qubit systems using partial measurements, as illustrated in the next exercise.

@[exercise]({
    "id": "partial_measurements_for_system",
    "title": "Distinguish orthogonal states using partial measurements",
    "descriptionPath": "./partial_measurements_for_system/index.md",
    "placeholderSourcePath": "./partial_measurements_for_system/placeholder.qs",
    "solutionPath": "./partial_measurements_for_system/solution.md",
    "codePaths": [
        "./partial_measurements_for_system/verify.qs",
        "./common.qs",
        "../KatasLibrary.qs"
    ]
})

@[section]({
    "id": "multi_qubit_measurements_measurements_and_entanglement",
    "title": "Measurements and entanglement"
})

Qubits entanglement has an effect on the measurement statistics of the system. If two qubits are entangled, then their measurement outcomes will be correlated, while separable states (which are by definition not entangled) have uncorrelated measurement outcomes.

> It is useful to revisit the concepts of entanglement and separable states, which were introduced in the kata on multi-qubit systems. Consider a system of $n>1$ number of qubits, which we divide into two parts: A, consisting of $m$ qubits, and B, consisting of the remaining $n-m$ qubits. We say that the state $\ket \psi$ of the entire system is separable if it can be expressed as a tensor product of the states of parts A and B:
$$
\ket \psi = \ket {\phi_A} \otimes \ket{\phi_B}
$$
where $\ket{\phi_A}$ and $\ket{\phi_B}$ are wave functions that describe parts $A$ and $B$, respectively. If it is not possible to express $\ket \psi$ in such a form, then we say that system A is entangled with system B.

Consider a measurement on the subsystem $A$ of a separable state. Let the measurement be done in a basis $\{ \ket{b_0},\dotsc,\ket{b_{2^m-1}}\}$. According to the projection formalism, a projection operator $P_i = \ket{b_i}\bra{b_i} \otimes \mathbb{1}$ is chosen randomly. The corresponding post-measurement state of the system is then given by
\begin{align*}
\ket{\psi}_{i} &\equiv \frac{P_i \ket{\psi}}{\big|P_i \ket{\psi}\big|} = \\
               &= \frac{\ket{b_i}\bra{b_i}\phi_A\rangle \otimes \ket {\phi_B}}{\big|\ket{b_i}\bra{b_i}\phi_A\rangle \otimes \ket {\phi_B}\big|} = \\
               &= \frac{\bra{b_i}\phi_A\rangle \cdot \ket{b_i} \otimes \ket {\phi_B}}{\big|\ket{b_i}\big| \cdot \bra{b_i}\phi_A\rangle \cdot \big| \ket {\phi_B}\big|} = \\
               &= \ket{b_i} \otimes \ket{\phi_B}.
\end{align*}

Thus, the state of subsystem $B$ after the measurement is $\ket{\phi_B}$ independently of the outcome $i$ of the measurement on the first qubit. The results of a subsequent measurement on subsystem $B$, including outcome probabilities, will be independent of the result of the first measurement. In other words, the outcomes of the two measurements will be uncorrelated.

On the other hand, if the system is entangled, then the measurement outcomes will be correlated, in a manner dictated by the bases chosen for the measurements on the two subsystems. The following exercise illustrates this phenomenon.

@[question]({
    "id": "measurements_and_entanglement",
    "descriptionPath": "./measurements_and_entanglement/index.md",
    "answerPath": "./measurements_and_entanglement/solution.md"
})

## State modification using partial measurements

For certain multi-qubit systems prepared in a superposition state, it is possible to use partial measurements to collapse a part of the system to some desired state.

@[exercise]({
    "id": "state_modification",
    "title": "State selection using partial measurements",
    "descriptionPath": "./state_modification/index.md",
    "placeholderSourcePath": "./state_modification/placeholder.qs",
    "solutionPath": "./state_modification/solution.md",
    "codePaths": [
        "./state_modification/verify.qs",
        "./common.qs",
        "../KatasLibrary.qs"
    ]
})

@[section]({
    "id": "multi_qubit_measurements_state_preparation",
    "title": "State preparation"
})

Any multi-qubit state can be prepared from the $|0...0\rangle$ state using an appropriate combination of quantum gates.
However, sometimes it is easier and more efficient to prepare a state using partial measurements.
You could prepare a simpler state involving additional qubits, which, when measured, result in a collapse of the remaining qubits to the desired state with a high probability. This is called **post-selection**, and is particularly useful if it is easier to prepare the pre-measurement state with the extra qubits than to prepare the desired state directly using unitary gates alone. This is demonstrated by the following exercise.

@[exercise]({
    "id": "state_preparation",
    "title": "State preparation using partial measurements",
    "descriptionPath": "./state_preparation/index.md",
    "placeholderSourcePath": "./state_preparation/placeholder.qs",
    "solutionPath": "./state_preparation/solution.md",
    "codePaths": [
        "./state_preparation/verify.qs",
        "./common.qs",
        "../KatasLibrary.qs"
    ]
})

@[section]({
    "id": "multi_qubit_measurements_joint_measurements",
    "title": "Joint measurements"
})

Joint measurements, also known as Pauli measurements, are a generalization of 2-outcome measurements to multiple qubits and other bases. In Q#, joint measurements in Pauli bases are implemented using the [Measure](https://docs.microsoft.com/en-us/qsharp/api/qsharp/microsoft.quantum.intrinsic.measure) operation. Let's review single-qubit measurements in a different light before discussing joint measurements.

## Single-qubit Pauli measurement
For single-qubit systems, any measurement corresponding to an orthogonal basis can be associated with a Hermitian matrix with eigenvalues $\pm 1$. The possible measurement outcomes (represented as `Result` in Q#) are the eigenvalues of the Hermitian matrix, and the corresponding projection matrices for the measurement are the projection operators onto the *eigenspaces* corresponding to the eigenvalues.

For example, consider the computational basis measurement, which can result in outcomes `Zero` or `One` corresponding to states $\ket 0$ and $\ket 1$. This measurement is associated with the Pauli Z operator, which is given by
$$
Z = \begin{pmatrix} 1 & 0 \\ 0 & -1\end{pmatrix} = \ket{0}\bra{0} - \ket{1}\bra{1}.
$$
The $Z$ operator has two eigenvalues, $1$ and $-1$, with corresponding eigenvectors $\ket{0}$ and $\ket{1}$. A $Z$-measurement is then a measurement in the $\{\ket{0},\ket{1}\}$ basis, with the measurement outcomes being $1$ and $-1$ respectively. In Q#, by convention, an eigenvalue of $1$ corresponds to a `Result` of `Zero`, while an eigenvalue of $-1$ corresponds to a `Result` of `One`.

Similarly, one can implement measurements corresponding to the Pauli X and Y operators. We summarize the various properties below:
<table>
    <tr>
        <th>Pauli Operator</th>
        <th>Matrix</th>
        <th>Eigenvalue</th>
        <th>Eigenvector/post-measurement state</th>
        <th>Measurement Result in Q#</th>
    </tr>
    <tr>
        <td rowspan="2">$X$</td>
        <td rowspan="2">$\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix}$</td>
        <td>+1</td>
        <td>$\ket{+}$</td>
        <td>Zero</td>
    </tr><tr>
        <td>-1</td>
        <td>$\ket{-}$</td>
        <td>One</td>
    </tr>
    <tr>
        <td rowspan="2">$Y$</td>
        <td rowspan="2">$\begin{bmatrix} 0 & -i \\ i & 0 \end{bmatrix}$</td>
        <td>+1</td>
        <td>$\ket{i}$</td>
        <td>Zero</td>
    </tr><tr>
        <td>-1</td>
        <td>$\ket{-i}$</td>
        <td>One</td>
    </tr>
    <tr>
        <td rowspan="2">$Z$</td>
        <td rowspan="2">$\begin{bmatrix} 1 & 0 \\ 0 & -1 \end{bmatrix}$</td>
        <td>+1</td>
        <td>$\ket{0}$</td>
        <td>Zero</td>
    </tr><tr>
        <td>-1</td>
        <td>$\ket{1}$</td>
        <td>One</td>
    </tr>
</table>

In general, any measurement on a single qubit which results in two outcomes corresponds to the Hermitian operator $U Z U^\dagger$, for some $2\times 2$ unitary matrix $U$.

Joint measurements are a generalization of this principle for multi-qubit matrices.

## Parity measurements
The simplest joint measurement is a parity measurement. A parity measurement treats computational basis vectors differently depending on whether the number of 1's in the basis vector is even or odd.

For example, the operator $Z\otimes Z$, or $ZZ$ in short, is the parity measurement operator for a two-qubit system. The eigenvalues $1$ and $-1$ correspond to the subspaces spanned by basis vectors $\{ |00\rangle, |11\rangle \}$ and $\{ |01\rangle, |10\rangle \}$, respectively. That is, when a $ZZ$ measurement results in a `Zero` (i.e. the eigenvalue $+1$), the post-measurement state is a superposition of only those computational basis vectors which have an even number of $1$'s. On the other hand, a result of `One` corresponds to a post-measurement state with only odd parity computational basis vectors.

> Let's see what happens to various two-qubit states after the parity measurement. The $Z \otimes Z$ matrix for two qubits is:
>
>$$Z \otimes Z = \begin{bmatrix}
    1 & 0 & 0 & 0 \\
    0 & -1 & 0 & 0 \\
    0 & 0 & -1 & 0 \\
    0 & 0 & 0 & 1 \\
\end{bmatrix}$$
>
>When this transformation is applied to a basis state $|00\rangle$, we get
>
> $$\begin{bmatrix}
    1 & 0 & 0 & 0 \\
    0 & -1 & 0 & 0 \\
    0 & 0 & -1 & 0 \\
    0 & 0 & 0 & 1 \\
\end{bmatrix}
\begin{bmatrix} 1 \\ 0 \\ 0 \\ 0 \\ \end{bmatrix} =
\begin{bmatrix} 1 \\ 0 \\ 0 \\ 0 \\ \end{bmatrix}$$
>
> Comparing this to the characteristic equation for eigenvectors of $Z \otimes Z$ given by
$ Z \otimes Z |\psi\rangle = \lambda |\psi\rangle$,
it is easy to see that $|00\rangle$ belongs to the $+1$ eigenspace, hence the $Z \otimes Z$ measurement will return `Zero` and leave the state unchanged.
>
> Similarly, it can easily be verified that $|11\rangle$ also belongs to $+1$ eigenspace, while $|01\rangle$ and $|10\rangle$ belong to the $-1$ eigenspace.
>
> Now, what happens if we apply a $Z \otimes Z$ measurement to a superposition state $\alpha |00\rangle + \beta |11\rangle$? We can see that
>
> $$\begin{bmatrix}
    1 & 0 & 0 & 0 \\
    0 & -1 & 0 & 0 \\
    0 & 0 & -1 & 0 \\
    0 & 0 & 0 & 1 \\
\end{bmatrix}
\begin{bmatrix} \alpha \\ 0 \\ 0 \\ \beta \\ \end{bmatrix} =
\begin{bmatrix} \alpha \\ 0 \\ 0 \\ \beta \\ \end{bmatrix}$$
>
>So this state also belongs to the $+1$ eigenspace, and measuring it will return `Zero` and leave the state unchanged. Similarly, we can verify that an $\alpha |01\rangle + \beta |10\rangle$ state belongs to the $-1$ eigenspace, and measuring it will return `One` without changing the state.

Similarly, a parity measurement on a higher number of qubits can be implemented using a $Z \otimes \dotsc \otimes Z$ measurement.

@[exercise]({
    "id": "joint_measurements",
    "title": "Two-qubit parity measurement",
    "descriptionPath": "./joint_measurements/index.md",
    "placeholderSourcePath": "./joint_measurements/placeholder.qs",
    "solutionPath": "./joint_measurements/solution.md",
    "codePaths": [
        "./joint_measurements/verify.qs",
        "./common.qs",
        "../KatasLibrary.qs"
    ]
})

@[section]({
    "id": "multi_qubit_measurements_pauli_measurements",
    "title": "Multi-qubit Pauli measurements"
})

Joint measurement is a generalization of the measurement in the computational basis.
Pauli measurements can also be generalized to a larger number of qubits. A multi-qubit Pauli measurement corresponds to an operator $M_1 \otimes \dotsc \otimes M_n$, with each $M_i$ being from the set of gates $\{X,Y,Z,I\}$. If at least one of the operators is not the identity matrix, then the measurement can result in two outcomes: a `Result` of `Zero` corresponding to eigenvalue $+1$ and a `Result` of `One` corresponding to the eigenvalue $-1$. The corresponding projection operators are the projections onto the corresponding eigenspaces.

For example, a Pauli/joint measurement corresponding to the $X\otimes Z$ operator can be characterized as follows:
<table>
    <tr>
        <th>Eigenvalue</th>
        <th>Measurement Result in Q#</th>
        <th>Eigenbasis</th>
        <th>Measurement Projector</th>
    </tr>
    <tr>
        <td>$+1$</td>
        <td>Zero</td>
        <td>$\{ \ket{0,+}, \ket{1,-} \}$</td>
        <td>$P_{+1} = \ket{0,+}\bra{0,+} +  \ket{1,-} \bra{1,-}$</td>
     </tr>
    <tr>
        <td>$-1$</td>
        <td>One</td>
        <td>$\{ \ket{0,-}, \ket{1,+} \}$</td>
        <td>$P_{-1} = \ket{0,-}\bra{0,-} +  \ket{1,+} \bra{1,+}$</td>
     </tr>
 </table>

 The rules for measurements are then the same as those outlined in the partial measurements section, with the projection operators in the table.

 @[question]({
    "id": "multi_qubit_measurements",
    "descriptionPath": "./multi_qubit_measurements/index.md",
    "answerPath": "./multi_qubit_measurements/solution.md"
})

## Conclusion

Congratulations!

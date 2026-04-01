# Multi-Qubit Gates

@[section]({
    "id": "multi_qubit_gates__overview",
    "title": "Overview"
})

This kata continues the introduction to quantum gates, focusing on applying quantum gates to multi-qubit systems.

**This kata covers the following topics:**

- Applying quantum gates to a part of the system
- $CNOT$, $CZ$, $CCNOT$, and $SWAP$ gates
- Controlled gates

**What you should know to start working on this kata:**

- Basic concepts of linear algebra
- The concept of qubit and multi-qubit systems
- Single-qubit and multi-qubit quantum gates

If you need a refresher on these topics, you can check out the previous katas.

@[section]({
    "id": "multi_qubit_gates__the_basics",
    "title": "The Basics"
})

As a reminder, single-qubit gates are represented by $2\times2$ unitary matrices.
The effect of a gate applied to a qubit can be calculated by multiplying the corresponding matrix by the state vector of the qubit to get the resulting state vector.

Multi-qubit gates are represented by $2^N\times2^N$ matrices, where $N$ is the number of qubits the gate operates on. To apply this gate, you multiply the matrix by the state vector of the $N$-qubit quantum system.

## Applying Gates to a Part of the System

The simplest thing you can do with multi-qubit systems is to apply gates to only a subset of qubits in the system.
Similar to how it is sometimes possible to represent the state of a multi-qubit system as a tensor product of single-qubit states, you can construct gates that modify the state of a multi-qubit system as tensor products of gates that affect parts of the system.

Let's consider an example of applying a single-qubit gate to one of the qubits of a two-qubit system.
If you want to apply an $X$ gate to the first qubit of the system and do nothing to the second qubit, the resulting gate will be represented as a tensor product of an $X$ gate and the identity gate $I$ which corresponds to doing nothing:

$$
X \otimes I =
\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix} \otimes \begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix} =
\begin{bmatrix}
    0 & 0 & 1 & 0 \\ 
    0 & 0 & 0 & 1 \\ 
    1 & 0 & 0 & 0 \\ 
    0 & 1 & 0 & 0
\end{bmatrix}
$$

You can use the same approach when applying several gates to independent parts of the system at the same time.
For example, applying the $X$ gate to the first qubit and the $H$ gate to the second qubit would be represented as follows:

$$
X \otimes H =
\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix} \otimes \frac{1}{\sqrt{2}}\begin{bmatrix} 1 & 1 \\ 1 & -1 \end{bmatrix} =
\frac{1}{\sqrt{2}}\begin{bmatrix}
    0 & 0 & 1 & 1 \\ 
    0 & 0 & 1 & -1 \\ 
    1 & 1 & 0 & 0 \\ 
    1 & -1 & 0 & 0
\end{bmatrix}
$$

> Note that you can use mixed-multiplication property of tensor product to see that this is equivalent to applying $X$ gate to the first qubit and applying $H$ gate to the second qubit, in either order:
>
> $$X \otimes H = (I X) \otimes (H I) = (I \otimes H) (X \otimes I)$$
> $$X \otimes H = (X I) \otimes (I H) = (X \otimes I) (I \otimes H)$$

This approach can be generalized to larger systems and gates that act on multiple qubits as well.
It can be less straightforward when a multi-qubit gate is applied to a subset of qubits that aren't "adjacent" to each other in the tensor product; you'll see an example later in this kata.

@[exercise]({
    "id": "multi_qubit_gates__compound_gate",
    "title": "Compound Gate",
    "path": "./compound_gate/"
})

@[section]({
    "id": "multi_qubit_gates__cnot_gate",
    "title": "CNOT Gate"
})

The first multi-qubit gate this kata introduces is the $CNOT$ ("controlled NOT") gate. The $CNOT$ gate is a two-qubit gate, with one qubit referred to as the **control** qubit, and the other qubit as the **target** qubit (usually the first qubit is the control, and the second qubit is the target).

$CNOT$ acts as a conditional gate of sorts: if the control qubit is in state $\ket{1}$, it applies the $X$ gate to the target qubit, otherwise it does nothing.

> If the system is in a superposition of several basis states, the effects of the gate will be a linear combination of the effects of it acting separately on each of the basis states.
> This will be the case for all quantum gates you'll encounter later that are specified in terms of basis states: since all unitary gates are linear, it's sufficient to define their effect on the basis states, and use linearity to figure out their effect on any state.

<table>
    <tr>
        <th>Gate</th>
        <th>Matrix</th>
        <th>Applying to $\ket{\psi} = \alpha\ket{00} + \beta\ket{01} + \gamma\ket{10} + \delta\ket{11}$</th>
        <th>Applying to basis states</th>
    </tr>
    <tr>
        <td>$CNOT$</td>
        <td>$\begin{bmatrix} 1 & 0 & 0 & 0 \\ 0 & 1 & 0 & 0 \\ 0 & 0 & 0 & 1 \\ 0 & 0 & 1 & 0 \end{bmatrix}$</td>
        <td>$CNOT\ket{\psi} = \alpha\ket{00} + \beta\ket{01} + \delta\ket{10} + \gamma\ket{11}$</td>
        <td>
            $$CNOT\ket{00} = \ket{00}$$
            $$CNOT\ket{01} = \ket{01}$$
            $$CNOT\ket{10} = \ket{11}$$
            $$CNOT\ket{11} = \ket{10}$$
        </td>
    </tr>
</table>

The $CNOT$ gate is particularly useful for preparing entangled states. Consider the following separable state:

$$\big(\alpha\ket{0} + \beta\ket{1}\big) \otimes \ket{0} = \alpha\ket{00} + \beta\ket{10}$$

If you apply the $CNOT$ gate to it, with the first qubit as the control, and the second as the target, you get the following state, which is not separable any longer:

$$\alpha\ket{00} + \beta\ket{11}$$

The $CNOT$ gate is self-adjoint: applying it for the second time reverses its effect.


@[exercise]({
    "id": "multi_qubit_gates__entangle_qubits",
    "title": "Entangle Qubits",
    "path": "./entangle_qubits/"
})

@[exercise]({
    "id": "multi_qubit_gates__preparing_bell_state",
    "title": "Preparing a Bell State",
    "path": "./preparing_bell_state/"
})

@[section]({
    "id": "multi_qubit_gates__cz_gate",
    "title": "CZ Gate"
})


The $CZ$ ("controlled-Z") gate is a two-qubit gate, with one qubit referred to as the **control** qubit, and the other as the **target** qubit. Interestingly, for the $CZ$ gate it doesn't matter which qubit is control and which is target - the effect of the gate is the same either way!

The $CZ$ gate acts as a conditional gate: if the control qubit is in state $\ket{1}$, it applies the $Z$ gate to the target qubit, otherwise it does nothing.

<table>
    <tr>
        <th>Gate</th>
        <th>Matrix</th>
        <th>Applying to $\ket{\psi} = \alpha\ket{00} + \beta\ket{01} + \gamma\ket{10} + \delta\ket{11}$</th>
        <th>Applying to basis states</th>
    </tr>
    <tr>
        <td>$CZ$</td>
        <td>
            $$\begin{bmatrix}
                1 & 0 & 0 & 0 \\
                0 & 1 & 0 & 0 \\
                0 & 0 & 1 & 0 \\
                0 & 0 & 0 & -1
            \end{bmatrix}$$
        </td>
        <td>$CZ\ket{\psi} = \alpha\ket{00} + \beta\ket{01} + \gamma\ket{10} - \delta\ket{11}$</td>
        <td>
            $$CZ\ket{00} = \ket{00}$$
            $$CZ\ket{01} = \ket{01}$$
            $$CZ\ket{10} = \ket{10}$$
            $$CZ\ket{11} = -\ket{11}$$
        </td>
    </tr>
</table>

The $CZ$ gate is particularly useful for creating and manipulating entangled states where the phase of the quantum state is crucial. Consider the following separable state:

$$\big(\alpha\ket{0} + \beta\ket{1}\big) \otimes \big(\gamma\ket{0} + \delta\ket{1}\big) = \alpha\gamma\ket{00} + \alpha\delta\ket{01} + \beta\gamma\ket{10} + \beta\delta\ket{11}$$

If you apply the $CZ$ gate to it, with the first qubit as the control and the second as the target (or vice versa), you get the following state, which can no longer be separated:

$$\alpha\gamma\ket{00} + \alpha\delta\ket{01} + \beta\gamma\ket{10} - \beta\delta\ket{11}$$

The $CZ$ gate is also self-adjoint: applying it a second time reverses its effect, similar to the $CNOT$ gate.

@[exercise]({
    "id": "multi_qubit_gates__relative_phase_minusone",
    "title": "Relative Phase -1",
    "path": "./relative_phase_minusone/"
})

@[section]({
    "id": "multi_qubit_gates__ket_bra_representation",
    "title": "Ket-Bra Representation"
})

Same as in the case of single-qubit gates, you can represent multi-qubit gates using Dirac notation.

> Recall that kets represent column vectors and bras represent row vectors. For any ket $\ket{\psi}$, the corresponding bra is its adjoint (conjugate transpose): $\bra{\psi} = \ket{\psi}^\dagger$.
>
> Kets and bras are used to express inner and outer products. The inner product of $\ket{\phi}$ and $\ket{\psi}$ is the matrix product of $\bra{\phi}$ and $\ket{\psi}$, denoted as $\braket{\phi|\psi}$, and their outer product is the matrix product of $\ket{\phi}$ and $\bra{\psi}$, denoted as $\ket{\phi}\bra{\psi}$.
>
> As you've seen in the Single-Qubit Gates kata, kets and bras can be used to represent matrices. The outer product of two vectors of the same size produces a square matrix. You can use a linear combination of several outer products of simple vectors (such as basis vectors) to express any square matrix.

Let's consider ket-bra representation of the $CNOT$ gate:

$$CNOT =$$
$$= \ket{00}\bra{00} + \ket{01}\bra{01} + \ket{10}\bra{11} + \ket{11}\bra{10} =$$
$$=
\begin{bmatrix} 1 \\ 0 \\ 0 \\ 0 \end{bmatrix}\begin{bmatrix} 1 & 0 & 0 & 0 \end{bmatrix} +
\begin{bmatrix} 0 \\ 1 \\ 0 \\ 0 \end{bmatrix}\begin{bmatrix} 0 & 1 & 0 & 0 \end{bmatrix} +
\begin{bmatrix} 0 \\ 0 \\ 1 \\ 0 \end{bmatrix}\begin{bmatrix} 0 & 0 & 0 & 1 \end{bmatrix} +
\begin{bmatrix} 0 \\ 0 \\ 0 \\ 1 \end{bmatrix}\begin{bmatrix} 0 & 0 & 1 & 0 \end{bmatrix} =
$$
$$=
\begin{bmatrix} 1 & 0 & 0 & 0 \\ 0 & 0 & 0 & 0 \\ 0 & 0 & 0 & 0 \\ 0 & 0 & 0 & 0 \end{bmatrix} +
\begin{bmatrix} 0 & 0 & 0 & 0 \\ 0 & 1 & 0 & 0 \\ 0 & 0 & 0 & 0 \\ 0 & 0 & 0 & 0 \end{bmatrix} +
\begin{bmatrix} 0 & 0 & 0 & 0 \\ 0 & 0 & 0 & 0 \\ 0 & 0 & 0 & 1 \\ 0 & 0 & 0 & 0 \end{bmatrix} +
\begin{bmatrix} 0 & 0 & 0 & 0 \\ 0 & 0 & 0 & 0 \\ 0 & 0 & 0 & 0 \\ 0 & 0 & 1 & 0 \end{bmatrix} =
$$
$$=\begin{bmatrix} 1 & 0 & 0 & 0 \\ 0 & 1 & 0 & 0 \\ 0 & 0 & 0 & 1 \\ 0 & 0 & 1 & 0 \\ \end{bmatrix}$$

This representation can be used to carry out calculations in Dirac notation without ever switching back to matrix representation:

$$
CNOT\ket{10} =
\big(\ket{00}\bra{00} + \ket{01}\bra{01} + \ket{10}\bra{11} + \ket{11}\bra{10}\big)\ket{10} =$$
$$=\ket{00}\braket{00|10} + \ket{01}\braket{01|10} + \ket{10}\braket{11|10} + \ket{11}\braket{10|10} =$$
$$=\ket{00}\big(\braket{00|10}\big) + \ket{01}\big(\braket{01|10}\big) + \ket{10}\big(\braket{11|10}\big) + \ket{11}\big(\braket{10|10}\big) =$$
$$=\ket{00}(0) + \ket{01}(0) + \ket{10}(0) + \ket{11}(1) = \ket{11}$$

> Notice how a lot of the inner product terms turn out to equal 0, and the expression is easily simplified. The $CNOT$ gate is expressed in terms of the outer product of computational basis states, which are orthonormal, and is applied to another computational basis state, so the individual inner products will always be 0 or 1.

In general case, a $4 \times 4$ matrix that describes a 2-qubit gate
$$A =
\begin{bmatrix}
    a_{00} & a_{01} & a_{02} & a_{03} \\
    a_{10} & a_{11} & a_{12} & a_{13} \\
    a_{20} & a_{21} & a_{22} & a_{23} \\
    a_{30} & a_{31} & a_{32} & a_{33} \\
\end{bmatrix}
$$

will have the following ket-bra representation:
$$A =$$
$$=a_{00} \ket{00}\bra{00} + a_{01} \ket{00}\bra{01} + a_{02} \ket{00}\bra{10} + a_{03} \ket{00}\bra{11} +$$
$$+a_{10} \ket{01}\bra{00} + a_{11} \ket{01}\bra{01} + a_{12} \ket{01}\bra{10} + a_{13} \ket{01}\bra{11} +$$
$$+a_{20} \ket{10}\bra{00} + a_{21} \ket{10}\bra{01} + a_{22} \ket{10}\bra{10} + a_{23} \ket{10}\bra{11} +$$
$$+a_{30} \ket{11}\bra{00} + a_{31} \ket{11}\bra{01} + a_{32} \ket{11}\bra{10} + a_{33} \ket{11}\bra{11}$$

A similar expression can be extended for matrices that describe $N$-qubit gates, where $N > 2$:

$$A = \sum_{i=0}^{2^N-1} \sum_{j=0}^{2^N-1} a_{ij} \ket{i}\bra{ j}$$

Dirac notation is particularly useful for expressing sparse matrices - matrices that have few non-zero elements. Indeed, consider the $CNOT$ gate again: it's a $4 \times 4$ matrix described with 16 elements, but its Dirac notation has only 4 terms, one for each non-zero element of the matrix.

With enough practice you'll be able to perform computations in Dirac notation without spelling out all the bra-ket terms explicitly!

@[section]({
    "id": "multi_qubit_gates__ket_bra_decomposition",
    "title": "Ket-Bra Decomposition"
})

This section describes a more formal process of finding the ket-bra decompositions of multi-qubit quantum gates.
This section isn't necessary to start working with quantum gates, so feel free to skip it for now, and come back to it later.

You can use the properties of eigenvalues and eigenvectors to find the ket-bra decomposition of any gate. Consider an $N$-qubit gate $A$; the matrix representation of the gate is a square matrix of size $2^N$. Therefore it also has $2^N$ orthogonal eigenvectors $\ket{\psi_i}$

$$A\ket{\psi_i} = x_i\ket{\psi_i}, 0 \leq i \leq 2^N -1$$

Then its ket-bra decomposition is:

$$A = \sum_{i=0}^{2^N-1} x_i\ket{\psi_i}\bra{\psi_i}$$

Let's use our $CNOT$ gate as a simple example.
The $CNOT$ gate has four eigenvectors.

- Two, as you can clearly see, are computational basis states $\ket{00}$ and $\ket{01}$ with eigenvalues $1$ and $1$, respectively (the basis states that aren't affected by the gate).
- The other two are $\ket{1} \otimes \ket{+} = \frac{1}{\sqrt{2}}\big(\ket{10} + \ket{11}\big)$ and $\ket{1} \otimes \ket{-} = \frac{1}{\sqrt{2}}\big(\ket{10} - \ket{11}\big)$ with eigenvalues $1$ and $-1$, respectively:

$$CNOT\ket{00} = \ket{00}$$
$$CNOT\ket{01} = \ket{01}$$
$$CNOT\ket{1+} = \ket{1+}$$
$$CNOT\ket{1-} = -\ket{1-}$$

Here's what the decomposition looks like:

$$CNOT =$$
$$=\ket{00}\bra{00} + \ket{01}\bra{01} + \ket{1+}\bra{1+} - \ket{1-}\bra{1-} =$$
$$=\ket{00}\bra{00} + \ket{01}\bra{01} + \frac{1}{2}\big[\big(\ket{10} + \ket{11}\big)\big(\bra{10} + \bra{11}\big) - \big(\ket{10} - \ket{11}\big)\big(\bra{10} - \bra{11}\big)\big] =$$
$$=\ket{00}\bra{00} + \ket{01}\bra{01} + \frac{1}{2}\big(\ket{10}\bra{10} + \ket{10}\bra{11} + \ket{11}\bra{10} + \ket{11}\bra{11} - \ket{10}\bra{10} + \ket{10}\bra{11} + \ket{11}\bra{10} - \ket{11}\bra{11}\big) =$$
$$=\ket{00}\bra{00} + \ket{01}\bra{01} + \frac{1}{2}\big(2\ket{10}\bra{11} + 2\ket{11}\bra{10}\big) =$$
$$=\ket{00}\bra{00} + \ket{01}\bra{01} + \ket{10}\bra{11} + \ket{11}\bra{10}$$

@[section]({
    "id": "multi_qubit_gates__swap_gate",
    "title": "SWAP Gate"
})

The $SWAP$ gate acts on two qubits, and, as the name implies, swaps their quantum states.

<table>
    <tr>
        <th>Gate</th>
        <th>Matrix</th>
        <th>Applying to $\ket{\psi} = \alpha\ket{00} + \beta\ket{01} + \gamma\ket{10} + \delta\ket{11}$</th>
        <th>Applying to basis states</th>
    </tr>
    <tr>
        <td>$SWAP$</td>
        <td>$\begin{bmatrix} 1 & 0 & 0 & 0 \\ 0 & 0 & 1 & 0 \\ 0 & 1 & 0 & 0 \\ 0 & 0 & 0 & 1 \end{bmatrix}$</td>
        <td>$SWAP\ket{\psi} = \alpha\ket{00} + \gamma\ket{01} + \beta\ket{10} + \delta\ket{11}$</td>
        <td>
            $$SWAP\ket{00} = \ket{00}$$
            $$SWAP\ket{01} = \ket{10}$$
            $$SWAP\ket{10} = \ket{01}$$
            $$SWAP\ket{11} = \ket{11}$$
        </td>
    </tr>
</table>

@[exercise]({
    "id": "multi_qubit_gates__qubit_swap",
    "title": "Qubit SWAP",
    "path": "./qubit_swap/"
})

@[section]({
    "id": "multi_qubit_gates__acting_on_non_adjacent_qubits",
    "title": "Multi-Qubit Gates Acting on Non-Adjacent Qubits"
})

In the above examples, the $CNOT$ gate acted on two adjacent qubits. However, multi-qubit gates can act on non-adjacent qubits as well. Let's see how to work out the math of the system state change in this case.

Take 3 qubits in an arbitrary state $\ket{\psi} = x_{000} \ket{000} + x_{001}\ket{001} + x_{010}\ket{010} + x_{011}\ket{011} + x_{100}\ket{100} + x_{101}\ket{101} + x_{110}\ket{110} + x_{111}\ket{111} $.

You can apply the $CNOT$ gate on 1st and 3rd qubits, with the 1st qubit as control and the 3rd qubit as target. Let's label the 3-qubit gate that describes the effect of this on the whole system as $CINOT$. The $CINOT$ ignores the 2nd qubit (leaves it unchanged) and applies the $CNOT$ gate as specified above.

## Q#

In Q#, the operation is described as the sequence of gates that are applied to the qubits, regardless of whether the qubits are adjacent or not.

```qsharp
operation CINOT (qs: Qubit[]) : Unit {
    CNOT(qs[0], qs[2]); // Length of qs is assumed to be 3
}
```

## Dirac Notation

In Dirac notation, you can consider the effect of the gate on each basis vector separately: each basis vector $\ket{a_1a_2a_3}$ remains unchanged if $a_1 = 0$, and becomes $\ket{a_1a_2(\neg a_3)}$ if $a_1 = 1$. The full effect on the state becomes:

$$CINOT\ket{\psi} = x_{000} CINOT\ket{000} + x_{001} CINOT\ket{001} + x_{010} CINOT\ket{010} + x_{011} CINOT\ket{011}+$$
$$+x_{100} CINOT\ket{100} + x_{101} CINOT\ket{101} + x_{110} CINOT\ket{110} + x_{111} CINOT\ket{111} =$$
$$= x_{000}\ket{000} + x_{001}\ket{001} + x_{010}\ket{010} + x_{011}\ket{011} + x_{101}\ket{100} + x_{100}\ket{101} + x_{111}\ket{110} + x_{110}\ket{111} $$

## Matrix Form

$CINOT$ can also be represented in matrix form as a $2^3 \times 2^3$ matrix:
$$
\begin{bmatrix}
    1 & 0 & 0 & 0 & 0 & 0 & 0 & 0 \\
    0 & 1 & 0 & 0 & 0 & 0 & 0 & 0 \\
    0 & 0 & 1 & 0 & 0 & 0 & 0 & 0 \\
    0 & 0 & 0 & 1 & 0 & 0 & 0 & 0 \\
    0 & 0 & 0 & 0 & 0 & 1 & 0 & 0 \\
    0 & 0 & 0 & 0 & 1 & 0 & 0 & 0 \\
    0 & 0 & 0 & 0 & 0 & 0 & 0 & 1 \\
    0 & 0 & 0 & 0 & 0 & 0 & 1 & 0
\end{bmatrix}
$$

Applying $CINOT$ to $\ket{\psi}$ gives us
$$
CINOT \begin{bmatrix}
    1 & 0 & 0 & 0 & 0 & 0 & 0 & 0 \\
    0 & 1 & 0 & 0 & 0 & 0 & 0 & 0 \\
    0 & 0 & 1 & 0 & 0 & 0 & 0 & 0 \\
    0 & 0 & 0 & 1 & 0 & 0 & 0 & 0 \\
    0 & 0 & 0 & 0 & 0 & 1 & 0 & 0 \\
    0 & 0 & 0 & 0 & 1 & 0 & 0 & 0 \\
    0 & 0 & 0 & 0 & 0 & 0 & 0 & 1 \\
    0 & 0 & 0 & 0 & 0 & 0 & 1 & 0
\end{bmatrix}
\begin{bmatrix}
    x_{000} \\ x_{001} \\ x_{010} \\ x_{011} \\ x_{100} \\ x_{101} \\ x_{110} \\ x_{111}
\end{bmatrix} =
\begin{bmatrix}
    x_{000} \\ x_{001} \\ x_{010} \\ x_{011} \\ x_{101} \\ x_{100} \\ x_{111} \\ x_{110}
\end{bmatrix}
$$

However, as $N$ gets larger, creating a full size matrix can be extremely unwieldy. To express the matrix without spelling out its elements, you can use the following trick:

1. Apply the $SWAP$ gate on the 1st and 2nd qubits.
   This will bring the qubits on which the $CNOT$ gate acts next to each other, without any extra qubits between them.
2. Apply the $CNOT$ on 2nd and 3rd qubits.
   Since now the gate acts on adjacent qubits, this can be represented as a tensor product of the gate you're applying and $I$ gates.
3. Apply the $SWAP$ gate on the 1st and 2nd qubits again.

These can be represented as applying the following gates on the 3 qubits.

1. $\text{SWAP} \otimes I$

$$
x_{000}\ket{000} + x_{001}\ket{001} + x_{100}\ket{010} + x_{101}\ket{011} +
x_{010}\ket{100} + x_{011}\ket{101} + x_{110}\ket{110} + x_{111}\ket{111}
$$

2. $I \otimes CNOT$

$$
x_{000}\ket{000} + x_{001}\ket{001} + x_{101}\ket{010} + x_{100}\ket{011} +
x_{010}\ket{100} + x_{011}\ket{101} + x_{111}\ket{110} + x_{110}\ket{111}
$$

3. $\text{SWAP} \otimes I$

$$
x_{000}\ket{000} + x_{001}\ket{001} + x_{010}\ket{010} + x_{011}\ket{011} +
x_{101}\ket{100} + x_{100}\ket{101} + x_{111}\ket{110} + x_{110}\ket{111}
$$

The result is the $CINOT$ gate as you intended; so you can write

$$CINOT = (SWAP \otimes I)(I \otimes CNOT)(SWAP \otimes I)$$

> Note that in matrix notation you always apply a gate to the complete system, so you must apply $SWAP \otimes I$, spelling the identity gate explicitly.
> However, when implementing the unitary $SWAP \otimes I$ in Q#, you need only to call `SWAP(qs[0], qs[1])` - the remaining qubit `qs[2]` won't change, which is equivalent to applying an implicit identity gate.
>
> You can also spell out all gates applied explicitly (this makes for a much longer code, though):
>
> ```qsharp
> operation CINOT (qs: Qubit[]) : Unit {
>     // First step
>     SWAP(qs[0], qs[1]);
>     I(qs[2]);
>     // Second step
>     I(qs[0]);
>     CNOT(qs[1], qs[2]);
>     // Third step
>     SWAP(qs[0], qs[1]);
>     I(qs[2]);
> }
> ```

@[section]({
    "id": "multi_qubit_gates__controlled_gates",
    "title": "Controlled Gates"
})

**Controlled gates** are a class of gates derived from other gates as follows: they act on a control qubit and a target qubit, just like the $CNOT$ gate.
A controlled-$U$ gate applies the $U$ gate to the target qubit if the control qubit is in state $\ket{1}$, and does nothing otherwise.

Given a gate $U = \begin{bmatrix} \alpha & \beta \\ \gamma & \delta \end{bmatrix}$, its controlled version looks like this:

<table>
    <tr>
        <th>Gate</th>
        <th>Matrix</th>
        <th>Q# Documentation</th>
    </tr>
    <tr>
        <td>$\text{Controlled }U$</td>
        <td>
            $$
            \begin{bmatrix}
                1 & 0 & 0 & 0 \\ 
                0 & 1 & 0 & 0 \\ 
                0 & 0 & \alpha & \beta \\ 
                0 & 0 & \gamma & \delta
            \end{bmatrix}
            $$
        </td>
        <td><a href="https://learn.microsoft.com/azure/quantum/user-guide/language/expressions/functorapplication#controlled-functor" target="_blank">Controlled functor</a></td>
    </tr>
</table>

> The $CNOT$ gate is en example of a controlled gate, which is why it's also known as the controlled $NOT$ or controlled $X$ gate.

The concept of controlled gates can be generalized beyond controlling single-qubit gates.
For any multi-qubit gate, its controlled version will have an identity matrix in the top left quadrant, the gate itself in the bottom right, and $0$ everywhere else.
Here, for example, is the Controlled $SWAP$, or **Fredkin gate**:

$$
\begin{bmatrix}
    1 & 0 & 0 & 0 & 0 & 0 & 0 & 0 \\ 
    0 & 1 & 0 & 0 & 0 & 0 & 0 & 0 \\ 
    0 & 0 & 1 & 0 & 0 & 0 & 0 & 0 \\ 
    0 & 0 & 0 & 1 & 0 & 0 & 0 & 0 \\ 
    0 & 0 & 0 & 0 & 1 & 0 & 0 & 0 \\ 
    0 & 0 & 0 & 0 & 0 & 0 & 1 & 0 \\ 
    0 & 0 & 0 & 0 & 0 & 1 & 0 & 0 \\ 
    0 & 0 & 0 & 0 & 0 & 0 & 0 & 1
\end{bmatrix}
$$

In Q#, controlled gates are applied using the <a href="https://learn.microsoft.com/azure/quantum/user-guide/language/expressions/functorapplication#controlled-functor" target="_blank">`Controlled`</a> functor.
The controlled version of a gate accepts an array of control qubits (in this case an array of a single qubit), followed by the arguments to the original gate.
For example, these two lines are equivalent:

```qsharp
Controlled X([control], target);
CNOT(control, target);
```

If the original gate was implemented as an operation with multiple parameters, the controlled version of this gate will take those parameters as a tuple. For example, to apply Fredkin gate, you'd have to call:

```qsharp
Controlled SWAP([control], (q1, q2));
```

You can use the controlled version of a Q# operation only if that operation has a controlled version defined.
The Q# compiler will often be able to generate a controlled version of the operation automatically if you put `is Ctl` after the operation's return type.
In other cases, you'll need to define the controlled version of an operation manually.

@[exercise]({
    "id": "multi_qubit_gates__fredkin_gate",
    "title": "Fredkin Gate",
    "path": "./fredkin_gate/"
})
@[exercise]({
    "id": "multi_qubit_gates__controlled_rotation",
    "title": "Controlled Rotation",
    "path": "./controlled_rotation/"
})

@[section]({
    "id": "multi_qubit_gates__multi_controlled_gates",
    "title": "Multi-Controlled Gates"
})

Controlled gates can have multiple control qubits; in this case the gate $U$ is applied only if all control qubits are in the $\ket{1}$ states.
You can think of it as constructing a controlled version of a gate that is already controlled.

The simplest example of this is the **Toffoli gate**, or $CCNOT$ (controlled controlled $NOT$) gate, which applies the $X$ gate to the last qubit only if the first two qubits are in the $\ket{11}$ state:

$$
\begin{bmatrix}
    1 & 0 & 0 & 0 & 0 & 0 & 0 & 0 \\ 
    0 & 1 & 0 & 0 & 0 & 0 & 0 & 0 \\ 
    0 & 0 & 1 & 0 & 0 & 0 & 0 & 0 \\ 
    0 & 0 & 0 & 1 & 0 & 0 & 0 & 0 \\ 
    0 & 0 & 0 & 0 & 1 & 0 & 0 & 0 \\ 
    0 & 0 & 0 & 0 & 0 & 1 & 0 & 0 \\ 
    0 & 0 & 0 & 0 & 0 & 0 & 0 & 1 \\ 
    0 & 0 & 0 & 0 & 0 & 0 & 1 & 0
\end{bmatrix}
$$

To construct a multi-controlled version of an operation in Q#, you can use the Controlled functor as well, passing all control qubits as an array that is the first parameter.

@[exercise]({
    "id": "multi_qubit_gates__toffoli_gate",
    "title": "Toffoli Gate",
    "path": "./toffoli_gate/"
})

@[section]({
    "id": "multi_qubit_gates__other_controlled_gates",
    "title": "Other Types of Controlled Gates"
})

Typically, the term "controlled $U$ gate" refers to the type of gate you've seen previously, which applies the gate $U$ only if the control qubit(s) are in the $\ket{1}$ state.

It's possible, however, to define variants of controlled gates that use different states as control states.
For example, an **anti-controlled** $U$ gate (sometimes called **zero-controlled**) applies a gate only if the control qubit is in the $\ket{0}$ state.
It's also possible to define control conditions in other bases, for example, applying the gate if the control qubit is in the $\ket{+}$ state.

All the variants of controlled gates can be expressed in terms of the controls described in previous sections, using the following sequence of steps:

- First, apply a transformation on control qubits that will transform the state you want to use as control into the $\ket{1...1}$ state.
- Apply the regular controlled version of the gate.
- Finally, undo the transformation on control qubits from the first step using the adjoint version of it.

Why do you need this last step? Remember that controlled gates are defined in terms of their effect on the basis states:
you apply the gate on the target qubit if and only if the control qubit is in the state you want to control on, and you don't change the state of the control qubit at all.
If you don't undo the transformation you did on the first step, applying your gate to a basis state will modify not only the state of the target qubit but also the state of the control qubit, which is not what you're looking for.

For example, consider an anti-controlled $X$ gate - a gate that should apply an $X$ gate to the second qubit if the first qubit is in the $\ket{0}$ state.
Here is the effect you expect this gate to have on each of the 2-qubit basis states:

<table>
  <tr>
    <th>Input state</th>
    <th>Output state</th>
  </tr>
  <tr>
    <td>$\ket{00}$</td>
    <td>$\ket{01}$</td>
  </tr>
  <tr>
    <td>$\ket{01}$</td>
    <td>$\ket{00}$</td>
  </tr>
  <tr>
    <td>$\ket{10}$</td>
    <td>$\ket{10}$</td>
  </tr>
  <tr>
    <td>$\ket{11}$</td>
    <td>$\ket{11}$</td>
  </tr>
</table>

Let's apply the anti-controlled $X$ gate to the $\ket{00}$ state step by step:

1. Transform the state of the control qubit to $\ket{1}$: you can do that by applying the $X$ gate to the first qubit:
$$\ket{00} \rightarrow \ket{10}$$
2. Apply the regular $CNOT$ gate:
$$\ket{10} \rightarrow \ket{11}$$
3. Now, if you don't undo the change you did on the first step, you'll end up with a gate that transforms $\ket{00}$ into $\ket{11}$, which is not the transformation you're trying to implement.
However, if you undo it by applying the $X$ gate to the first qubit again, you'll get the correct state:
$$\ket{11} \rightarrow \ket{01}$$

You can check that getting the right behavior of the operation on the rest of the basis states also requires that last step.

Finally, let's take a look at a very useful operation `ApplyControlledOnBitString` provided by the Q# standard library.
It applies a variant of a gate controlled on a basis state specified by a bit mask; for example, bit mask `[true, false]` means that the gate should be applied only if the two control qubits are in the $\ket{10}$ state.
This operation takes four arguments: the control bit mask as a Boolean array, the gate $U$ that needs its controlled variant defined, the array of control qubits, and the arguments to the $U$ gate (the target qubit(s) and any additional parameters it takes).

The sequence of steps that implement this variant are:

1. Apply the $X$ gate to each control qubit that corresponds to a `false` element of the bit mask (in the example, that's just the second qubit). After this, if the control qubits started in the $\ket{10}$ state, they'll end up in the $\ket{11}$ state, and if they started in any other state, they'll end up in any state but $\ket{11}$.
2. Apply the regular controlled version of the gate.
3. Apply the $X$ gate to the same qubits to return them to their original state.

@[exercise]({
    "id": "multi_qubit_gates__anti_controlled_gate",
    "title": "Anti-Controlled Gate",
    "path": "./anti_controlled_gate/"
})

@[exercise]({
    "id": "multi_qubit_gates__arbitrary_controls",
    "title": "Arbitrary Controls",
    "path": "./arbitrary_controls/"
})

@[section]({
    "id": "multi_qubit_gates__conclusion",
    "title": "Conclusion"
})

Congratulations! In this kata you learned how to apply multi-qubit quantum gates to multi-qubit systems. Here are a few key concepts to keep in mind:

- Applying single-qubit gates to a quantum system can be described as applying a larger multi-qubit gate to the whole system. In this case, this multi-qubit gate can be represented as a tensor product of single-qubit gates.
- $CNOT$ gate is a type of controlled gate that acts on two qubits. It applies the $X$ gate to the target qubit if the control qubit is in state $\ket{1}$, otherwise it does nothing.
- $SWAP$ gate is a gate that acts on two qubits, swapping their states.
- In Q#, controlled gates are applied using the `Controlled` functor.
- `ApplyControlledOnBitString` operation allows us to construct multi-qubit controlled gates with different control patterns.

Next, you will learn about quantum measurements in the "Preparing Quantum States" kata.

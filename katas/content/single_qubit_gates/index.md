# Single-Qubit Gates

@[section]({
    "id": "single_qubit_gates__overview",
    "title": "Overview"
})

This kata introduces you to single-qubit gates. Quantum gates are the quantum counterpart to classical logic gates, acting as the building blocks of quantum algorithms. Quantum gates transform qubit states in various ways, and can be applied sequentially to perform complex quantum calculations. Single-qubit gates, as their name implies, act on individual qubits. You can learn more at <a href="https://en.wikipedia.org/wiki/Quantum_logic_gate" target="_blank">Wikipedia</a>.

**This kata covers the following topics:**

- Matrix representation
- Ket-bra representation
- The most important single-qubit gates

**What you should know to start working on this kata:**

- Basic linear algebra
- The concept of qubit

@[section]({
    "id": "single_qubit_gates__basics",
    "title": "The Basics"
})

There are certain properties common to all quantum gates. This section will introduce those properties, using the $X$ gate as an example.

## Matrix Representation

Quantum gates are represented as $2^N \times 2^N$ unitary matrices, where $N$ is the number of qubits the gate operates on.
As a quick reminder, a unitary matrix is a square matrix whose inverse is its adjoint, thus $U^* U = UU^* = UU^{-1} = \mathbb{I}$.
Single-qubit gates are represented by $2 \times 2$ matrices.
Our example for this section, the $X$ gate, is represented by the following matrix:

$$\begin{bmatrix} 0 & 1 \\\ 1 & 0 \end{bmatrix}$$

You may recall that the state of a qubit is represented by a vector of size $2$. You can apply a gate to a qubit by multiplying the gate's matrix by the qubit's state vector. The result will be another vector, representing the new state of the qubit. For example, applying the $X$ gate to the computational basis states looks like this:

$$
X|0\rangle =
\begin{bmatrix} 0 & 1 \\\ 1 & 0 \end{bmatrix}
\begin{bmatrix} 1 \\\ 0 \end{bmatrix} =
\begin{bmatrix} 0 \cdot 1 + 1 \cdot 0 \\\ 1 \cdot 1 + 0 \cdot 0 \end{bmatrix} =
\begin{bmatrix} 0 \\\ 1 \end{bmatrix}
$$

$$
X|1\rangle =
\begin{bmatrix} 0 & 1 \\\ 1 & 0 \end{bmatrix}
\begin{bmatrix} 0 \\\ 1 \end{bmatrix} =
\begin{bmatrix} 0 \cdot 0 + 1 \cdot 1 \\\ 1 \cdot 0 + 0 \cdot 1 \end{bmatrix} =
\begin{bmatrix} 1 \\\ 0 \end{bmatrix}
$$

The general case:

$$|\psi\rangle = \alpha|0\rangle + \beta|1\rangle$$

$$
X|\psi\rangle =
\begin{bmatrix} 0 & 1 \\\ 1 & 0 \end{bmatrix}
\begin{bmatrix} \alpha \\\ \beta \end{bmatrix} =
\begin{bmatrix} 0 \cdot \alpha + 1 \cdot \beta \\\ 1 \cdot \alpha + 0 \cdot \beta \end{bmatrix} =
\begin{bmatrix} \beta \\\ \alpha \end{bmatrix}
$$

> If you need a reminder of what $|0\rangle$, $|1\rangle$, and $|\psi\rangle$ mean, you can review the section on Dirac notation in "The Qubit" kata.

Quantum gates are represented by matrices, just like quantum states are represented by vectors. Because this is the most common way to represent quantum gates, the terms "gate" and "gate matrix" will be used interchangeably in this kata.

Applying several quantum gates in sequence is equivalent to performing several of these multiplications.
For example, if you have gates $A$ and $B$ and a qubit in state $|\psi\rangle$, the result of applying $A$ followed by $B$ to that qubit would be $B\big(A|\psi\rangle\big)$ (the gate closest to the qubit state gets applied first).
Matrix multiplication is associative, so this is equivalent to multiplying the $B$ matrix by the $A$ matrix, producing a compound gate of the two, and then applying that to the qubit: $\big(BA\big)|\psi\rangle$.

>Note that matrix multiplication isn’t commutative, thus $(BA) \neq \(AB)$.

All quantum gates are reversible - there is another gate which will undo any given gate's transformation, returning the qubit to its original state.
This means that when dealing with quantum gates, information about qubit states is never lost, as opposed to classical logic gates, some of which destroy information.
Quantum gates are represented by unitary matrices, so the inverse of a gate is its adjoint; these terms are also used interchangeably in quantum computing.

## Effects on Basis States

There is a simple way to find out what a gate does to the two computational basis states $|0\rangle$ and $|1\rangle$. Consider an arbitrary gate:

$$A = \begin{bmatrix} \epsilon & \zeta \\\ \eta & \mu \end{bmatrix}$$

Watch what happens when we apply it to these states:

$$
A|0\rangle =
\begin{bmatrix} \epsilon & \zeta \\\ \eta & \mu \end{bmatrix}
\begin{bmatrix} 1 \\\ 0 \end{bmatrix} =
\begin{bmatrix} \epsilon \cdot 1 + \zeta \cdot 0 \\\ \eta \cdot 1 + \mu \cdot 0 \end{bmatrix} =
\begin{bmatrix} \epsilon \\\ \eta \end{bmatrix} = \epsilon|0\rangle + \eta|1\rangle
$$

$$
A|1\rangle =
\begin{bmatrix} \epsilon & \zeta \\\ \eta & \mu \end{bmatrix}
\begin{bmatrix} 0 \\\ 1 \end{bmatrix} =
\begin{bmatrix} \epsilon \cdot 0 + \zeta \cdot 1 \\\ \eta \cdot 0 + \mu \cdot 1 \end{bmatrix} =
\begin{bmatrix} \zeta \\\ \mu \end{bmatrix} = \zeta|0\rangle + \mu|1\rangle
$$

Notice that applying the gate to the $|0\rangle$ state transforms it into the state written as the first column of the gate's matrix. Likewise, applying the gate to the $|1\rangle$ state transforms it into the state written as the second column. This holds true for any quantum gate, including, of course, the $X$ gate:

$$X = \begin{bmatrix} 0 & 1 \\\ 1 & 0 \end{bmatrix}$$

$$X|0\rangle = \begin{bmatrix} 0 \\\ 1 \end{bmatrix} = |1\rangle$$

$$X|1\rangle = \begin{bmatrix} 1 \\\ 0 \end{bmatrix} = |0\rangle$$

Once you understand how a gate affects the computational basis states, you can easily find how it affects any state.
Recall that any qubit state vector can be written as a linear combination of the basis states:

$$|\psi\rangle = \begin{bmatrix} \alpha \\\ \beta \end{bmatrix} = \alpha|0\rangle + \beta|1\rangle$$

Because matrix multiplication distributes over addition, once you know how a gate affects those two basis states, you can calculate how it affects any state:

$$X|\psi\rangle = X\big(\alpha|0\rangle + \beta|1\rangle\big) = X\big(\alpha|0\rangle\big) + X\big(\beta|1\rangle\big) = \alpha X|0\rangle + \beta X|1\rangle = \alpha|1\rangle + \beta|0\rangle$$

That is, applying a gate to a qubit in superposition is equivalent to applying that gate to the basis states that make up that superposition and adding the results with appropriate weights.

@[section]({
    "id": "single_qubit_gates__ket_bra_representation",
    "title": "Ket-Bra Representation"
})

There is another way to represent quantum gates, this time using Dirac notation. However, the kets we've been using aren't enough to represent arbitrary matrices. We need to introduce another piece of notation: the **bra** (this is why Dirac notation is sometimes called **bra-ket notation**).

Recall that kets represent column vectors; a bra is a ket's row vector counterpart. For any ket $|\psi\rangle$, the corresponding bra is its adjoint (conjugate transpose): $\langle\psi| = |\psi\rangle^\dagger$.

Some examples:

<table>
  <tr>
    <th>Ket</th>
    <th>Bra</th>
  </tr>
  <tr>
    <td>$|0\rangle = \begin{bmatrix} 1 \\\ 0 \end{bmatrix}$</td>
    <td>$\langle0| = \begin{bmatrix} 1 & 0 \end{bmatrix}$</td>
  </tr>
  <tr>
    <td>$|1\rangle = \begin{bmatrix} 0 \\\ 1 \end{bmatrix}$</td>
    <td>$\langle1| = \begin{bmatrix} 0 & 1 \end{bmatrix}$</td>
  </tr>
  <tr>
    <td>$|i\rangle = \begin{bmatrix} \frac{1}{\sqrt{2}} \\\ \frac{i}{\sqrt{2}} \end{bmatrix}$</td>
    <td>$\langle i| = \begin{bmatrix} \frac{1}{\sqrt{2}} & -\frac{i}{\sqrt{2}} \end{bmatrix}$</td>
  </tr>
  <tr>
    <td>$|\psi\rangle = \begin{bmatrix} \alpha \\\ \beta \end{bmatrix}$</td>
    <td>$\langle\psi| = \begin{bmatrix} \overline{\alpha} & \overline{\beta} \end{bmatrix}$</td>
  </tr>
  <tr>
    <td>$|\psi\rangle = \alpha|0\rangle + \beta|1\rangle$</td>
    <td>$\langle\psi| = \overline{\alpha}\langle0| + \overline{\beta}\langle1|$</td>
  </tr>
</table>

Kets and bras give us a neat way to express inner and outer products. The inner product of $|\phi\rangle$ and $|\psi\rangle$ is the matrix product of $\langle\phi|$ and $|\psi\rangle$, denoted as $\langle\phi|\psi\rangle$, and their outer product is the matrix product of $|\phi\rangle$ and $\langle\psi|$, denoted as $|\phi\rangle\langle\psi|$. Notice that the norm of $|\psi\rangle$ is $\sqrt{\langle\psi|\psi\rangle}$.

This brings us to representing matrices. Recall that the outer product of two vectors of the same size produces a square matrix. We can use a linear combination of several outer products of simple vectors (such as basis vectors) to express any square matrix. For example, the $X$ gate can be expressed as follows:

$$X = |0\rangle\langle1| + |1\rangle\langle0|$$

$$
|0\rangle\langle1| + |1\rangle\langle0| =
\begin{bmatrix} 1 \\\ 0 \end{bmatrix}\begin{bmatrix} 0 & 1 \end{bmatrix} +
\begin{bmatrix} 0 \\\ 1 \end{bmatrix}\begin{bmatrix} 1 & 0 \end{bmatrix} =
\begin{bmatrix} 0 & 1 \\\ 0 & 0 \end{bmatrix} + \begin{bmatrix} 0 & 0 \\\ 1 & 0 \end{bmatrix} =
\begin{bmatrix} 0 & 1 \\\ 1 & 0 \end{bmatrix}
$$

This representation can be used to carry out calculations in Dirac notation without ever switching back to matrix representation:

$$X|0\rangle = \big(|0\rangle\langle1| + |1\rangle\langle0|\big)|0\rangle = |0\rangle\langle1|0\rangle + |1\rangle\langle0|0\rangle = |0\rangle\big(\langle1|0\rangle\big) + |1\rangle\big(\langle0|0\rangle\big) = |0\rangle(0) + |1\rangle(1) = |1\rangle$$

> That last step may seem a bit confusing. Recall that $|0\rangle$ and $|1\rangle$ form an **orthonormal basis**. That is, they are both normalized, and they are orthogonal to each other.
>
> A vector is normalized if its norm is equal to $1$, which only happens if its inner product with itself is equal to $1$. This means that $\langle0|0\rangle = \langle1|1\rangle = 1$
>
> Two vectors are orthogonal to each other if their inner product equals $0$. This means that $\langle0|1\rangle = \langle 1|0\rangle = 0$.

In general case, a matrix
$$A = \begin{bmatrix} a_{00} & a_{01} \\\ a_{10} & a_{11} \end{bmatrix}$$
will have the following ket-bra representation:
$$A = a_{00} |0\rangle\langle0| + a_{01} |0\rangle\langle1| + a_{10} |1\rangle\langle0| + a_{11} |1\rangle\langle1|$$

@[section]({
    "id": "single_qubit_gates__ket_bra_decomposition",
    "title": "Ket-Bra Decomposition"
})

This section describes a more formal process of finding the ket-bra decompositions of quantum gates. This section is not necessary to start working with quantum gates, so feel free to skip it for now, and come back to it later.

You can use the properties of _eigenvalues_ and _eigenvectors_ to find the ket-bra decomposition of any gate. Given a gate $A$ and the orthogonal vectors $|\phi\rangle$ and $|\psi\rangle$, if:

$$A|\phi\rangle = x_\phi|\phi\rangle$$
$$A|\psi\rangle = x_\psi|\psi\rangle$$

Real numbers $x_\phi$ and $x_\psi$ are called eigenvalues and $|\phi\rangle$ and $|\psi\rangle$ are eigenvectors of $A$. Then:

$$A = x_\phi|\phi\rangle\langle\phi| + x_\psi|\psi\rangle\langle\psi|$$

Let's use our $X$ gate as a simple example. The $X$ gate has two eigenvectors: $|+\rangle = \frac{1}{\sqrt{2}}\big(|0\rangle + |1\rangle\big)$ and $|-\rangle = \frac{1}{\sqrt{2}}\big(|0\rangle - |1\rangle\big)$. Their eigenvalues are $1$ and $-1$ respectively:

$$X|+\rangle = |+\rangle$$
$$X|-\rangle = -|-\rangle$$

Here's what the decomposition looks like:
$$X = |+\rangle\langle+| - |-\rangle\langle-| =$$
$$= \frac{1}{2}\big[\big(|0\rangle + |1\rangle\big)\big(\langle0| + \langle1|\big) - \big(|0\rangle - |1\rangle\big)\big(\langle0| - \langle1|\big)\big] =$$
$$= \frac{1}{2}\big(|0\rangle\langle0| + |0\rangle\langle1| + |1\rangle\langle0| + |1\rangle\langle1| - |0\rangle\langle0| + |0\rangle\langle1| + |1\rangle\langle0| - |1\rangle\langle1|\big) =$$
$$= \frac{1}{2}\big(2|0\rangle\langle1| + 2|1\rangle\langle0|\big) =$$
$$= |0\rangle\langle1| + |1\rangle\langle0|$$

@[section]({
    "id": "single_qubit_gates__important_gates",
    "title": "Pauli Gates"
})

This section introduces some of the common single-qubit gates, including their matrix form, their ket-bra decomposition, and a brief "cheatsheet" listing their effect on some common qubit states.

You can use a tool called <a href="https://algassert.com/quirk" target="_blank">Quirk</a> to visualize how these gates interact with various qubit states.

This section relies on the following notation:

<table>
  <tr>
    <td>$|+\rangle = \frac{1}{\sqrt{2}}\big(|0\rangle + |1\rangle\big)$</td>
    <td>$|-\rangle = \frac{1}{\sqrt{2}}\big(|0\rangle - |1\rangle\big)$</td>
  </tr>
  <tr>
    <td>$|i\rangle = \frac{1}{\sqrt{2}}\big(|0\rangle + i|1\rangle\big)$</td>
    <td>$|-i\rangle = \frac{1}{\sqrt{2}}\big(|0\rangle - i|1\rangle\big)$</td>
  </tr>
</table>

The Pauli gates, named after <a href="https://en.wikipedia.org/wiki/Wolfgang_Pauli" target="_blank">Wolfgang Pauli</a>, are based on the so-called **Pauli matrices**, $X$, $Y$ and $Z$. All three Pauli gates are **self-adjoint**, meaning that each one is its own inverse, $XX = \mathbb{I}$.

<table>
  <tr>
    <th>Gate</th>
    <th>Matrix</th>
    <th>Ket-Bra</th>
    <th>Applying to $|\psi\rangle = \alpha|0\rangle + \beta|1\rangle$</th>
    <th>Applying to basis states</th>
  </tr>
  <tr>
    <td>$X$</td>
    <td>$\begin{bmatrix} 0 & 1 \\\ 1 & 0 \end{bmatrix}$</td>
    <td>$|0\rangle\langle1| + |1\rangle\langle0|$</td>
    <td>$X|\psi\rangle = \alpha|1\rangle + \beta|0\rangle$</td>
    <td>
      $X|0\rangle = |1\rangle$<br>
      $X|1\rangle = |0\rangle$<br>
      $X|+\rangle = |+\rangle$<br>
      $X|-\rangle = -|-\rangle$<br>
      $X|i\rangle = i|-i\rangle$<br>
      $X|-i\rangle = -i|i\rangle$
    </td>
  </tr>
  <tr>
    <td>$Y$</td>
    <td>$\begin{bmatrix} 0 & -i \\\ i & 0 \end{bmatrix}$</td>
    <td>$i(|1\rangle\langle0| - |0\rangle\langle1|)$</td>
    <td>$Y|\psi\rangle = i\big(\alpha|1\rangle - \beta|0\rangle\big)$</td>
    <td>
      $Y|0\rangle = i|1\rangle$<br>
      $Y|1\rangle = -i|0\rangle$<br>
      $Y|+\rangle = -i|-\rangle$<br>
      $Y|-\rangle = i|+\rangle$<br>
      $Y|i\rangle = |i\rangle$<br>
      $Y|-i\rangle = -|-i\rangle$<br>
    </td>
  </tr>
  <tr>
    <td>$Z$</td>
    <td>$\begin{bmatrix} 1 & 0 \\\ 0 & -1 \end{bmatrix}$</td>
    <td>$|0\rangle\langle0| - |1\rangle\langle1|$</td>
    <td>$Z|\psi\rangle = \alpha|0\rangle - \beta|1\rangle$</td>
    <td>
      $Z|0\rangle = |0\rangle$<br>
      $Z|1\rangle = -|1\rangle$<br>
      $Z|+\rangle = |-\rangle$<br>
      $Z|-\rangle = |+\rangle$<br>
      $Z|i\rangle = |-i\rangle$<br>
      $Z|-i\rangle = |i\rangle$<br>
    </td>
  </tr>
</table>

> The $X$ gate is sometimes referred to as the **bit flip** gate, or the **NOT** gate, because it acts like the classical NOT gate on the computational basis.
>
> The $Z$ gate is sometimes referred to as the **phase flip** gate.

Here are several properties of the Pauli gates that are easy to verify and convenient to remember:

- Different Pauli gates _anti-commute_:
  $$XZ = -ZX, XY = -YX, YZ = -ZY$$
- A product of any two Pauli gates equals the third gate, with an extra $i$ (or $-i$) phase:
  $$XY = iZ, YZ = iX, ZX = iY$$
- A product of all three Pauli gates equals identity (with an extra $i$ phase):
  $$XYZ = iI$$

@[section]({
    "id": "single_qubit_gates__pauli_gates_in_qsharp",
    "title": "Pauli Gates in Q#"
})

The following example contains code demonstrating how to apply gates in Q#. It sets up a series of quantum states, and then shows the result of applying the $X$ gate to each one.

In the previous kata we discussed that qubit state in Q# cannot be directly assigned or accessed. The same logic is extended to quantum gates: applying a gate to a qubit modifies the internal state of that qubit, but doesn't return the resulting state of the qubit. This is why we never assign the output of these gates to any variables in this demo - they don't produce any output.

The same principle applies to applying several gates in a row to a qubit. In the mathematical notation, applying an $X$ gate followed by a $Z$ gate to a state $|\psi\rangle$ is denoted as $Z(X(|\psi\rangle))$, because the result of applying a gate to a state is another state. In Q#, applying a gate doesn't return anything, so you can't use its output as an input to another gate - something like `Z(X(q))` will not produce the expected result. Instead, to apply several gates to the same qubit, you need to call them separately in the order in which they are applied:

```qsharp
X(q);
Z(q);
```

All the basic gates we will be covering in this kata are part of the Intrinsic namespace. We're also using the function DumpMachine to print the state of the quantum simulator.

@[example]({"id": "single_qubit_gates__pauli_gates_in_qsharp_demo", "codePath": "./examples/PauliGates.qs"})

@[exercise]({
    "id": "single_qubit_gates__state_flip",
    "title": "State Flip",
    "path": "./state_flip/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__sign_flip",
    "title": "Sign Flip",
    "path": "./sign_flip/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__y_gate",
    "title": "The Y Gate",
    "path": "./y_gate/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__sign_flip_on_zero",
    "title": "Sign Flip on Zero",
    "path": "./sign_flip_on_zero/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__global_phase_minusone",
    "title": "Global Phase -1",
    "path": "./global_phase_minusone/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__global_phase_i",
    "title": "Global Phase i",
    "path": "./global_phase_i/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})


@[section]({
    "id": "identity_gate",
    "title": "Identity Gate"
})

The identity gate is mostly here for completeness, at least for now. It will come in handy when dealing with multi-qubit systems and multi-qubit gates. It is represented by the identity matrix, and does not affect the state of the qubit.

<table>
<tr>
<th>Gate</th>
<th>Matrix</th>
<th>Ket-Bra</th>
<th>Applying to $|\psi\rangle = \alpha|0\rangle + \beta|1\rangle$</th>
</tr>
<tr>
<td>$I$</td>
<td>$\begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix}$</td>
<td>$|0\rangle\langle0| + |1\rangle\langle1|$</td>
<td>$I|\psi\rangle = |\psi\rangle$</td>
</tr>
</table>

@[section]({
    "id": "hadamard_gate",
    "title": "Hadamard Gate"
})

The **Hadamard** gate is an extremely important quantum gate. Unlike the previous gates, applying the Hadamard gate to a qubit in a computational basis state puts that qubit into a superposition.
Like the Pauli gates, the Hadamard gate is self-adjoint.

<table>
<tr>
<th>Gate</th>
<th>Matrix</th>
<th>Ket-Bra</th>
<th>Applying to $|\psi\rangle = \alpha|0\rangle + \beta|1\rangle$</th>
<th>Applying to basis states</th>
</tr>
<tr>
<td>$H$</td>
<td>$\begin{bmatrix} \frac{1}{\sqrt{2}} & \frac{1}{\sqrt{2}} \\ \frac{1}{\sqrt{2}} & -\frac{1}{\sqrt{2}} \end{bmatrix} = \frac{1}{\sqrt{2}}\begin{bmatrix} 1 & 1 \\ 1 & -1 \end{bmatrix}$</td>
<td>$|0\rangle\langle+| + |1\rangle\langle-|$</td>
<td>$H|\psi\rangle = \alpha|+\rangle + \beta|-\rangle = \frac{\alpha + \beta}{\sqrt{2}}|0\rangle + \frac{\alpha - \beta}{\sqrt{2}}|1\rangle$</td>
<td>$H|0\rangle = |+\rangle$ <br>
$H|1\rangle = |-\rangle$ <br>
$H|+\rangle = |0\rangle$ <br>
$H|-\rangle = |1\rangle$ <br>
$H|i\rangle = e^{i\pi/4}|-i\rangle$ <br>
$H|-i\rangle = e^{-i\pi/4}|i\rangle $ <br>
</tr>
</table>

> As a reminder, $e^{i\pi/4} = \frac{1}{\sqrt2} (1 + i)$ and $e^{-i\pi/4} = \frac{1}{\sqrt2} (1 - i)$. This is an application of Euler's formula, $e^{i\theta} = \cos \theta + i\sin \theta$, where $\theta$ is measured in radians.
> See this [Wikipedia article](https://en.wikipedia.org/wiki/Euler%27s_formula) for an explanation of Euler's formula and/or [this video](https://youtu.be/v0YEaeIClKY) for a more intuitive explanation.

@[exercise]({
    "id": "single_qubit_gates__basis_change",
    "title": "Basis Change",
    "path": "./basis_change/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})


@[exercise]({
    "id": "single_qubit_gates__prepare_minus",
    "title": "Prepare Minus",
    "path": "./prepare_minus/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[section]({
    "id": "single_qubit_gates__phase_shift_gates",
    "title": "Phase Shift Gates"
})

The next two gates are known as phase shift gates. They apply a phase to the $|1\rangle$ state, and leave the $|0\rangle$ state unchanged.

<table>
  <tr>
    <th>Gate</th>
    <th>Matrix</th>
    <th>Ket-Bra</th>
    <th>Applying to $|\psi\rangle = \alpha|0\rangle + \beta|1\rangle$</th>
    <th>Applying to basis states</th>
    </tr>
  <tr>
    <td>$S$</td>
    <td>$\begin{bmatrix} 1 & 0 \\ 0 & i \end{bmatrix}$</td>
    <td>$|0\rangle\langle0| + i|1\rangle\langle1|$</td>
    <td>$S|\psi\rangle = \alpha|0\rangle + i\beta|1\rangle$</td>
    <td>
      $S|0\rangle = |0\rangle$<br>
      $S|1\rangle = i|1\rangle$<br>
      $S|+\rangle = |i\rangle$<br>
      $S|-\rangle = |-i\rangle$<br>
      $S|i\rangle = |-\rangle$<br>
      $S|-i\rangle = |+\rangle$<br>
    </td>
    </tr>
  <tr>
    <td>$T$</td>
    <td>$\begin{bmatrix} 1 & 0 \\\ 0 & e^{i\pi/4} \end{bmatrix}$</td>
    <td>$|0\rangle\langle0| + e^{i\pi/4}|1\rangle$$\langle1|$</td>
    <td>$T|\psi\rangle = \alpha|0\rangle + e^{i\pi/4} \beta |1\rangle$</td>
    <td>
      $T|0\rangle = |0\rangle$<br>
      $T|1\rangle = e^{i\pi/4}|1\rangle$
    </td>
  </tr>
</table>

> Notice that applying the $T$ gate twice is equivalent to applying the $S$ gate, and applying the $S$ gate twice is equivalent to applying the $Z$ gate:
$$T^2 = S, S^2 = Z$$

@[exercise]({
    "id": "single_qubit_gates__phase_i",
    "title": "Relative Phase i",
    "path": "./phase_i/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__three_quarters_pi_phase",
    "title": "Three-Fourths Phase",
    "path": "./three_quarters_pi_phase/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[section]({
    "id": "single_qubit_gates__rotation_gates",
    "title": "Rotation Gates"
})

The next few gates are parametrized: their exact behavior depends on a numeric parameter - an angle $\theta$, given in radians.
These gates are the $X$ rotation gate $R_x(\theta)$, $Y$ rotation gate $R_y(\theta)$, $Z$ rotation gate $R_z(\theta)$, and the arbitrary phase gate $R_1(\theta)$.
Note that for the first three gates the parameter $\theta$ is multiplied by $\frac{1}{2}$ within the gate's matrix.

> These gates are known as rotation gates, because they represent rotations around various axes on the Bloch sphere. The Bloch sphere is a way of representing the qubit states visually, mapping them onto the surface of a sphere.
> Unfortunately, this visualization isn't very useful beyond single-qubit states, which is why we have opted not to go into details in this kata.
> If you are curious about it, you can learn more in <a href="https://en.wikipedia.org/wiki/Bloch_sphere" target="_blank">this Wikipedia article</a>.

<table>
  <tr>
    <th>Gate</th>
    <th>Matrix</th>
    <th>Applying to $|\psi\rangle = \alpha|0\rangle + \beta|1\rangle$</th>
    <th>Applying to basis states</th>
   </tr>
  <tr>
    <td>$R_x(\theta)$</td>
    <td>
    $$
    \begin{bmatrix} \cos\frac{\theta}{2} & -i\sin\frac{\theta}{2} \\\ -i\sin\frac{\theta}{2} & \cos\frac{\theta}{2} \end{bmatrix}
    $$
    </td>
    <td>$R_x(\theta)|\psi\rangle = (\alpha\cos\frac{\theta}{2} - i\beta\sin\frac{\theta}{2})|0\rangle + (\beta\cos\frac{\theta}{2} - i\alpha\sin\frac{\theta}{2})|1\rangle$</td>
    <td>
      $R_x(\theta)|0\rangle = \cos\frac{\theta}{2}|0\rangle - i\sin\frac{\theta}{2}|1\rangle$<br>
      $R_x(\theta)|1\rangle = \cos\frac{\theta}{2}|1\rangle - i\sin\frac{\theta}{2}|0\rangle$
    </td>
   </tr>
  <tr>
    <td>$R_y(\theta)$</td>
    <td>$\begin{bmatrix} \cos\frac{\theta}{2} & -\sin\frac{\theta}{2} \\\ \sin\frac{\theta}{2} & \cos\frac{\theta}{2} \end{bmatrix}$</td>
    <td>$R_y(\theta)|\psi\rangle = (\alpha\cos\frac{\theta}{2} - \beta\sin\frac{\theta}{2})|0\rangle + (\beta\cos\frac{\theta}{2} + \alpha\sin\frac{\theta}{2})|1\rangle$</td>
    <td>
      $R_y(\theta)|0\rangle = \cos\frac{\theta}{2}|0\rangle + \sin\frac{\theta}{2}|1\rangle$<br>
      $R_y(\theta)|1\rangle = \cos\frac{\theta}{2}|1\rangle - \sin\frac{\theta}{2}|0\rangle$
    </td>
    </tr>
  <tr>
    <td>$R_z(\theta)$</td>
    <td>$\begin{bmatrix} e^{-i\theta/2} & 0 \\\ 0 & e^{i\theta/2} \end{bmatrix}$</td>
    <td>$R_z(\theta)|\psi\rangle = \alpha e^{-i\theta/2}|0\rangle + \beta e^{i\theta/2}|1\rangle$</td>
    <td>
      $R_z(\theta)|0\rangle = e^{-i\theta/2}|0\rangle$<br>
      $R_z(\theta)|1\rangle = e^{i\theta/2}|1\rangle$
    </td>
  </tr>
  <tr>
    <td>$R_1(\theta)$</td>
    <td>$\begin{bmatrix} 1 & 0 \\\ 0 & e^{i\theta} \end{bmatrix}$</td>
    <td>$R_1(\theta)|\psi\rangle = \alpha|0\rangle + \beta e^{i\theta}|1\rangle$</td>
    <td>
      $R_1(\theta)|0\rangle = |0\rangle$<br>
      $R_1(\theta)|1\rangle = e^{i\theta}|1\rangle$
    </td>  
  </tr>
</table>

You have already encountered some special cases of the $R_1$ gate:

$$T = R_1(\frac{\pi}{4}), S = R_1(\frac{\pi}{2}), Z = R_1(\pi)$$

In addition, this gate is closely related to the $R_z$ gate: applying $R_1$ gate is equivalent to applying the $R_z$ gate, and then applying a global phase:

$$R_1(\theta) = e^{i\theta/2}R_z(\theta)$$

In addition, the rotation gates are very closely related to their respective Pauli gates:

$$X = iR_x(\pi), Y = iR_y(\pi), Z = iR_z(\pi)$$

@[exercise]({
    "id": "single_qubit_gates__complex_phase",
    "title": "Complex Relative Phase",
    "path": "./complex_phase/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__amplitude_change",
    "title": "Amplitude Change",
    "path": "./amplitude_change/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__prepare_rotated_state",
    "title": "Prepare Rotated State",
    "path": "./prepare_rotated_state/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "single_qubit_gates__prepare_arbitrary_state",
    "title": "Prepare Arbitrary State",
    "path": "./prepare_arbitrary_state/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[section]({
    "id": "single_qubit_gates__conclusion",
    "title": "Conclusion"
})

Congratulations!  In this kata you learned the matrix and the ket-bra representation of quantum gates. Here are a few key concepts to keep in mind:

- Single-qubit gates act on individual qubits and are represented by $2 \times 2$ unitary matrices.
- The effect of a gate applied to a qubit can be calculated by multiplying the corresponding matrix by the state vector of the qubit.
- Applying several quantum gates in sequence is equivalent to performing several matrix multiplications.
- Any square matrix can be represented as a linear combination of the outer products of vectors. The outer product is the matrix product of $|\phi\rangle$ and $\langle\psi|$, denoted as $|\phi\rangle\langle\psi|$.
- Pauli gates, identity and Hadamard gates, phase shift gates, and rotation gates are examples of single-qubit gates. All of them are available in Q#.

Next, you will learn about multi-qubit systems in the “Multi-Qubit Systems” kata.

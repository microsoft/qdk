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

- Basic knowledge of linear algebra
- The concept of qubit

If you need a refresher on these topics, you can check out the previous katas.

@[section]({
    "id": "single_qubit_gates__basics",
    "title": "The Basics"
})

There are certain properties common to all quantum gates. This section introduces those properties, using the $X$ gate as an example.

## Matrix Representation

Quantum gates are represented as $2^N \times 2^N$ unitary matrices, where $N$ is the number of qubits the gate operates on.
As a quick reminder, a unitary matrix is a square matrix whose inverse is its adjoint, thus $U^* U = UU^* = UU^{-1} = \mathbb{I}$.
Single-qubit gates are represented by $2 \times 2$ matrices.
The example for this section, the $X$ gate, is represented by the following matrix:

$$\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix}$$

You may recall that the state of a qubit is represented by a vector of size $2$. You can apply a gate to a qubit by multiplying the gate's matrix by the qubit's state vector. The result will be another vector, representing the new state of the qubit. For example, applying the $X$ gate to the computational basis states looks like this:

$$
X\ket{0} =
\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix}
\begin{bmatrix} 1 \\ 0 \end{bmatrix} =
\begin{bmatrix} 0 \cdot 1 + 1 \cdot 0 \\ 1 \cdot 1 + 0 \cdot 0 \end{bmatrix} =
\begin{bmatrix} 0 \\ 1 \end{bmatrix}
$$

$$
X\ket{1} =
\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix}
\begin{bmatrix} 0 \\ 1 \end{bmatrix} =
\begin{bmatrix} 0 \cdot 0 + 1 \cdot 1 \\ 1 \cdot 0 + 0 \cdot 1 \end{bmatrix} =
\begin{bmatrix} 1 \\ 0 \end{bmatrix}
$$

The general case:

$$\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$$

$$
X\ket{\psi} =
\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix}
\begin{bmatrix} \alpha \\ \beta \end{bmatrix} =
\begin{bmatrix} 0 \cdot \alpha + 1 \cdot \beta \\ 1 \cdot \alpha + 0 \cdot \beta \end{bmatrix} =
\begin{bmatrix} \beta \\ \alpha \end{bmatrix}
$$

> If you need a reminder of what $\ket{0}$, $\ket{1}$, and $\ket{\psi}$ mean, you can review the section on Dirac notation in "The Qubit" kata.

Quantum gates are represented by matrices, just like quantum states are represented by vectors. Because this is the most common way to represent quantum gates, the terms "gate" and "gate matrix" will be used interchangeably in this kata.

Applying several quantum gates in sequence is equivalent to performing several of these multiplications.
For example, if you have gates $A$ and $B$ and a qubit in state $\ket{\psi}$, the result of applying $A$ followed by $B$ to that qubit would be $B\big(A\ket{\psi}\big)$ (the gate closest to the qubit state gets applied first).
Matrix multiplication is associative, so this is equivalent to multiplying the $B$ matrix by the $A$ matrix, producing a compound gate of the two, and then applying that to the qubit: $\big(BA\big)\ket{\psi}$.

>Note that matrix multiplication isn’t commutative, thus $(BA) \neq (AB)$.

All quantum gates are reversible, that is, there exists another gate which will undo any given gate's transformation, returning the qubit to its original state.
This means that when dealing with quantum gates, information about qubit states is never lost, as opposed to classical logic gates, some of which destroy information.
Quantum gates are represented by unitary matrices, so the inverse of a gate is its adjoint; these terms are also used interchangeably in quantum computing.

## Effects on Basis States

There is a simple way to find out what a gate does to the two computational basis states $\ket{0}$ and $\ket{1}$. Consider an arbitrary gate:

$$A = \begin{bmatrix} \epsilon & \zeta \\ \eta & \mu \end{bmatrix}$$

Watch what happens when applying $A$ to these states:

$$
A\ket{0} =
\begin{bmatrix} \epsilon & \zeta \\ \eta & \mu \end{bmatrix}
\begin{bmatrix} 1 \\ 0 \end{bmatrix} =
\begin{bmatrix} \epsilon \cdot 1 + \zeta \cdot 0 \\ \eta \cdot 1 + \mu \cdot 0 \end{bmatrix} =
\begin{bmatrix} \epsilon \\ \eta \end{bmatrix} = \epsilon\ket{0} + \eta\ket{1}
$$

$$
A\ket{1} =
\begin{bmatrix} \epsilon & \zeta \\ \eta & \mu \end{bmatrix}
\begin{bmatrix} 0 \\ 1 \end{bmatrix} =
\begin{bmatrix} \epsilon \cdot 0 + \zeta \cdot 1 \\ \eta \cdot 0 + \mu \cdot 1 \end{bmatrix} =
\begin{bmatrix} \zeta \\ \mu \end{bmatrix} = \zeta\ket{0} + \mu\ket{1}
$$

Notice that applying the $A$ gate to the $\ket{0}$ state transforms it into the state written as the first column of the gate's matrix. Likewise, applying the $A$ gate to the $\ket{1}$ state transforms it into the state written as the second column. This holds true for any quantum gate, including, of course, the $X$ gate:

$$X = \begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix}$$

$$X\ket{0} = \begin{bmatrix} 0 \\ 1 \end{bmatrix} = \ket{1}$$

$$X\ket{1} = \begin{bmatrix} 1 \\ 0 \end{bmatrix} = \ket{0}$$

Once you understand how a gate affects the computational basis states, you can easily find how it affects any state.
Recall that any qubit state vector can be written as a linear combination of the basis states:

$$\ket{\psi} = \begin{bmatrix} \alpha \\ \beta \end{bmatrix} = \alpha\ket{0} + \beta\ket{1}$$

Because matrix multiplication distributes over addition, once you know how a gate affects those two basis states, you can calculate how it affects any state:

$$X\ket{\psi} = X\big(\alpha\ket{0} + \beta\ket{1}\big) = X\big(\alpha\ket{0}\big) + X\big(\beta\ket{1}\big) = \alpha X\ket{0} + \beta X\ket{1} = \alpha\ket{1} + \beta\ket{0}$$

That is, applying a gate to a qubit in superposition is equivalent to applying that gate to the basis states that make up that superposition and adding the results with appropriate weights.

@[section]({
    "id": "single_qubit_gates__ket_bra_representation",
    "title": "Ket-Bra Representation"
})

There is another way to represent quantum gates, this time using Dirac notation. However, kets aren't enough to represent arbitrary matrices. An additional notation is required: the **bra** (this is why Dirac notation is sometimes called **bra-ket notation**).

Recall that kets represent column vectors; a bra is a ket's row vector counterpart. For any ket $\ket{\psi}$, the corresponding bra is its adjoint (conjugate transpose): $\bra{\psi} = \ket{\psi}^\dagger$.

Some examples:

<div class="table-with-latex">
  <table>
    <tr>
      <th>Ket</th>
      <th>Bra</th>
    </tr>
    <tr>
      <td>$\ket{0} = \begin{bmatrix} 1 \\ 0 \end{bmatrix}$</td>
      <td>$\bra{0} = \begin{bmatrix} 1 & 0 \end{bmatrix}$</td>
    </tr>
    <tr>
      <td>$\ket{1} = \begin{bmatrix} 0 \\ 1 \end{bmatrix}$</td>
      <td>$\bra{1} = \begin{bmatrix} 0 & 1 \end{bmatrix}$</td>
    </tr>
    <tr>
      <td>$\ket{i} = \begin{bmatrix} \frac{1}{\sqrt{2}} \\ \frac{i}{\sqrt{2}} \end{bmatrix}$</td>
      <td>$\bra{ i} = \begin{bmatrix} \frac{1}{\sqrt{2}} & -\frac{i}{\sqrt{2}} \end{bmatrix}$</td>
    </tr>
    <tr>
      <td>$\ket{\psi} = \begin{bmatrix} \alpha \\ \beta \end{bmatrix}$</td>
      <td>$\bra{\psi} = \begin{bmatrix} \overline{\alpha} & \overline{\beta} \end{bmatrix}$</td>
    </tr>
    <tr>
      <td>$\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$</td>
      <td>$\bra{\psi} = \overline{\alpha}\bra{0} + \overline{\beta}\bra{1}$</td>
    </tr>
  </table>
</div>

Kets and bras give us a neat way to express inner and outer products. The inner product of $\ket{\phi}$ and $\ket{\psi}$ is the matrix product of $\bra{\phi}$ and $\ket{\psi}$, denoted as $\braket{\phi|\psi}$, and their outer product is the matrix product of $\ket{\phi}$ and $\bra{\psi}$, denoted as $\ket{\phi}\bra{\psi}$. Notice that the norm of $\ket{\psi}$ is $\sqrt{\braket{\psi|\psi}}$.

This leads to the representation of matrices via outer products. Recall that the outer product of two vectors of the same size produces a square matrix. You can use a linear combination of several outer products of simple vectors (such as basis vectors) to express any square matrix. For example, the $X$ gate can be expressed as follows:

$$X = \ket{0}\bra{1} + \ket{1}\bra{0}$$

$$
\ket{0}\bra{1} + \ket{1}\bra{0} =
\begin{bmatrix} 1 \\ 0 \end{bmatrix}\begin{bmatrix} 0 & 1 \end{bmatrix} +
\begin{bmatrix} 0 \\ 1 \end{bmatrix}\begin{bmatrix} 1 & 0 \end{bmatrix} =
\begin{bmatrix} 0 & 1 \\ 0 & 0 \end{bmatrix} + \begin{bmatrix} 0 & 0 \\ 1 & 0 \end{bmatrix} =
\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix}
$$

This representation can be used to carry out calculations in Dirac notation without ever switching back to matrix representation:

$$X\ket{0} = \big(\ket{0}\bra{1} + \ket{1}\bra{0}\big)\ket{0} = \ket{0}\braket{1|0} + \ket{1}\braket{0|0} = \ket{0}\big(\braket{1|0}\big) + \ket{1}\big(\braket{0|0}\big) = \ket{0}(0) + \ket{1}(1) = \ket{1}$$

> That last step may seem a bit confusing. Recall that $\ket{0}$ and $\ket{1}$ form an **orthonormal basis**. That is, they're both normalized, and they're orthogonal to each other.
>
> A vector is normalized if its norm is equal to $1$, which only happens if its inner product with itself is equal to $1$. This means that $\braket{0|0} = \braket{1|1} = 1$
>
> Two vectors are orthogonal to each other if their inner product equals $0$. This means that $\braket{0|1} = \braket{1|0} = 0$.

In general case, a matrix
$$A = \begin{bmatrix} a_{00} & a_{01} \\ a_{10} & a_{11} \end{bmatrix}$$
will have the following ket-bra representation:
$$A = a_{00} \ket{0}\bra{0} + a_{01} \ket{0}\bra{1} + a_{10} \ket{1}\bra{0} + a_{11} \ket{1}\bra{1}$$

@[section]({
    "id": "single_qubit_gates__ket_bra_decomposition",
    "title": "Ket-Bra Decomposition"
})

This section describes a more formal process of finding the ket-bra decompositions of quantum gates. This section isn't necessary to start working with quantum gates, so feel free to skip it for now, and come back to it later.

You can use the properties of _eigenvalues_ and _eigenvectors_ to find the ket-bra decomposition of any gate. Given a gate $A$ and the orthogonal vectors $\ket{\phi}$ and $\ket{\psi}$, if:

$$A\ket{\phi} = x_\phi\ket{\phi}$$
$$A\ket{\psi} = x_\psi\ket{\psi}$$

Real numbers $x_\phi$ and $x_\psi$ are called eigenvalues and $\ket{\phi}$ and $\ket{\psi}$ are eigenvectors of $A$. Then:

$$A = x_\phi\ket{\phi}\bra{\phi} + x_\psi\ket{\psi}\bra{\psi}$$

Let's use the $X$ gate as a simple example. The $X$ gate has two eigenvectors: $\ket{+} = \frac{1}{\sqrt{2}}\big(\ket{0} + \ket{1}\big)$ and $\ket{-} = \frac{1}{\sqrt{2}}\big(\ket{0} - \ket{1}\big)$. Their eigenvalues are $1$ and $-1$ respectively:

$$X\ket{+} = \ket{+}$$
$$X\ket{-} = -\ket{-}$$

Here's what the decomposition looks like:
$$X = \ket{+}\bra{+} - \ket{-}\bra{-} =$$
$$= \frac{1}{2}\big[\big(\ket{0} + \ket{1}\big)\big(\bra{0} + \bra{1}\big) - \big(\ket{0} - \ket{1}\big)\big(\bra{0} - \bra{1}\big)\big] =$$
$$= \frac{1}{2}\big(\ket{0}\bra{0} + \ket{0}\bra{1} + \ket{1}\bra{0} + \ket{1}\bra{1} - \ket{0}\bra{0} + \ket{0}\bra{1} + \ket{1}\bra{0} - \ket{1}\bra{1}\big) =$$
$$= \frac{1}{2}\big(2\ket{0}\bra{1} + 2\ket{1}\bra{0}\big) =$$
$$= \ket{0}\bra{1} + \ket{1}\bra{0}$$

@[section]({
    "id": "single_qubit_gates__important_gates",
    "title": "Pauli Gates"
})

This section introduces some of the common single-qubit gates, including their matrix form, their ket-bra decomposition, and a brief "cheat sheet" listing their effect on some common qubit states.

You can use a tool called <a href="https://algassert.com/quirk" target="_blank">Quirk</a> to visualize how these gates interact with various qubit states.

This section relies on the following notation:

<div class="table-with-latex">
  <table>
    <tr>
      <td>$\ket{+} = \frac{1}{\sqrt{2}}\big(\ket{0} + \ket{1}\big)$</td>
      <td>$\ket{-} = \frac{1}{\sqrt{2}}\big(\ket{0} - \ket{1}\big)$</td>
    </tr>
    <tr>
      <td>$\ket{i} = \frac{1}{\sqrt{2}}\big(\ket{0} + i\ket{1}\big)$</td>
      <td>$\ket{-i} = \frac{1}{\sqrt{2}}\big(\ket{0} - i\ket{1}\big)$</td>
    </tr>
  </table>
</div>

The Pauli gates, named after <a href="https://en.wikipedia.org/wiki/Wolfgang_Pauli" target="_blank">Wolfgang Pauli</a>, are based on the so-called **Pauli matrices**, $X$, $Y$ and $Z$. All three Pauli gates are **self-adjoint**, meaning that each one is its own inverse, $XX = \mathbb{I}$.

<div class="table-with-latex">
  <table>
    <tr>
      <th>Gate</th>
      <th>Matrix</th>
      <th>Ket-Bra</th>
      <th>Applying to $\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$</th>
      <th>Applying to basis states</th>
    </tr>
    <tr>
      <td>$X$</td>
      <td>$\begin{bmatrix} 0 & 1 \\ 1 & 0 \end{bmatrix}$</td>
      <td>$\ket{0}\bra{1} + \ket{1}\bra{0}$</td>
      <td>$X\ket{\psi} = \alpha\ket{1} + \beta\ket{0}$</td>
      <td>
        $X\ket{0} = \ket{1}$<br>
        $X\ket{1} = \ket{0}$<br>
        $X\ket{+} = \ket{+}$<br>
        $X\ket{-} = -\ket{-}$<br>
        $X\ket{i} = i\ket{-i}$<br>
        $X\ket{-i} = -i\ket{i}$
      </td>
    </tr>
    <tr>
      <td>$Y$</td>
      <td>$\begin{bmatrix} 0 & -i \\ i & 0 \end{bmatrix}$</td>
      <td>$i(\ket{1}\bra{0} - \ket{0}\bra{1})$</td>
      <td>$Y\ket{\psi} = i\big(\alpha\ket{1} - \beta\ket{0}\big)$</td>
      <td>
        $Y\ket{0} = i\ket{1}$<br>
        $Y\ket{1} = -i\ket{0}$<br>
        $Y\ket{+} = -i\ket{-}$<br>
        $Y\ket{-} = i\ket{+}$<br>
        $Y\ket{i} = \ket{i}$<br>
        $Y\ket{-i} = -\ket{-i}$<br>
      </td>
    </tr>
    <tr>
      <td>$Z$</td>
      <td>$\begin{bmatrix} 1 & 0 \\ 0 & -1 \end{bmatrix}$</td>
      <td>$\ket{0}\bra{0} - \ket{1}\bra{1}$</td>
      <td>$Z\ket{\psi} = \alpha\ket{0} - \beta\ket{1}$</td>
      <td>
        $Z\ket{0} = \ket{0}$<br>
        $Z\ket{1} = -\ket{1}$<br>
        $Z\ket{+} = \ket{-}$<br>
        $Z\ket{-} = \ket{+}$<br>
        $Z\ket{i} = \ket{-i}$<br>
        $Z\ket{-i} = \ket{i}$<br>
      </td>
    </tr>
  </table>
</div>

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

The previous kata mentioned that qubit state in Q# cannot be directly assigned or accessed. The same logic is extended to quantum gates: applying a gate to a qubit modifies the internal state of that qubit but doesn't return the resulting state of the qubit. That's the reason why you never assign the output of these gates to any variables in this demo - they don't produce any output.

The same principle applies to applying several gates in a row to a qubit. In the mathematical notation, applying an $X$ gate followed by a $Z$ gate to a state $\ket{\psi}$ is denoted as $Z(X(\ket{\psi}))$, because the result of applying a gate to a state is another state. In Q#, applying a gate doesn't return anything, so you can't use its output as an input to another gate - something like `Z(X(q))` won't produce the expected result. Instead, to apply several gates to the same qubit, you need to call them separately in the order in which they're applied:

```qsharp
X(q);
Z(q);
```

All the basic gates covered in this kata are part of the Intrinsic namespace. Additionally, the function `DumpMachine` from the Diagnostics namespace is used to print the state of the quantum simulator.

@[example]({"id": "single_qubit_gates__pauli_gates_in_qsharp_demo", "codePath": "./examples/PauliGates.qs"})

@[exercise]({
    "id": "single_qubit_gates__state_flip",
    "title": "State Flip",
    "path": "./state_flip/"
})

@[exercise]({
    "id": "single_qubit_gates__sign_flip",
    "title": "Sign Flip",
    "path": "./sign_flip/"
})

@[exercise]({
    "id": "single_qubit_gates__y_gate",
    "title": "The Y Gate",
    "path": "./y_gate/"
})

@[exercise]({
    "id": "single_qubit_gates__sign_flip_on_zero",
    "title": "Sign Flip on Zero",
    "path": "./sign_flip_on_zero/"
})

@[exercise]({
    "id": "single_qubit_gates__global_phase_minusone",
    "title": "Global Phase -1",
    "path": "./global_phase_minusone/"
})

@[exercise]({
    "id": "single_qubit_gates__global_phase_i",
    "title": "Global Phase i",
    "path": "./global_phase_i/"
})


@[section]({
    "id": "identity_gate",
    "title": "Identity Gate"
})

The identity gate is mostly here for completeness, at least for now. It will come in handy when dealing with multi-qubit systems and multi-qubit gates. It's represented by the identity matrix, and doesn't affect the state of the qubit.

<div class="table-with-latex">
  <table>
    <tr>
      <th>Gate</th>
      <th>Matrix</th>
      <th>Ket-Bra</th>
      <th>Applying to $\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$</th>
    </tr>
    <tr>
      <td>$I$</td>
      <td>$\begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix}$</td>
      <td>$\ket{0}\bra{0} + \ket{1}\bra{1}$</td>
      <td>$I\ket{\psi} = \ket{\psi}$</td>
    </tr>
  </table>
</div>

@[section]({
    "id": "hadamard_gate",
    "title": "Hadamard Gate"
})

The **Hadamard** gate is an extremely important quantum gate. Unlike the previous gates, applying the Hadamard gate to a qubit in a computational basis state puts that qubit into a superposition.
Like the Pauli gates, the Hadamard gate is self-adjoint.

<div class="table-with-latex">
  <table>
    <tr>
      <th>Gate</th>
      <th>Matrix</th>
      <th>Ket-Bra</th>
      <th>Applying to $\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$</th>
      <th>Applying to basis states</th>
    </tr>
    <tr>
      <td>$H$</td>
      <td>$\begin{bmatrix} \frac{1}{\sqrt{2}} & \frac{1}{\sqrt{2}} \\ \frac{1}{\sqrt{2}} & -\frac{1}{\sqrt{2}} \end{bmatrix} = \frac{1}{\sqrt{2}}\begin{bmatrix} 1 & 1 \\ 1 & -1 \end{bmatrix}$</td>
      <td>$\ket{0}\bra{+} + \ket{1}\bra{-}$</td>
      <td>$H\ket{\psi} = \alpha\ket{+} + \beta\ket{-} = \frac{\alpha + \beta}{\sqrt{2}}\ket{0} + \frac{\alpha - \beta}{\sqrt{2}}\ket{1}$</td>
      <td>$H\ket{0} = \ket{+}$ <br>
      $H\ket{1} = \ket{-}$ <br>
      $H\ket{+} = \ket{0}$ <br>
      $H\ket{-} = \ket{1}$ <br>
      $H\ket{i} = e^{i\pi/4}\ket{-i}$ <br>
      $H\ket{-i} = e^{-i\pi/4}\ket{i} $ <br>
    </tr>
  </table>
</div>

> As a reminder, $e^{i\pi/4} = \frac{1}{\sqrt2} (1 + i)$ and $e^{-i\pi/4} = \frac{1}{\sqrt2} (1 - i)$. This is an application of Euler's formula, $e^{i\theta} = \cos \theta + i\sin \theta$, where $\theta$ is measured in radians.
> See this [Wikipedia article](https://en.wikipedia.org/wiki/Euler%27s_formula) for an explanation of Euler's formula and/or [this video](https://youtu.be/v0YEaeIClKY) for a more intuitive explanation.

@[exercise]({
    "id": "single_qubit_gates__basis_change",
    "title": "Basis Change",
    "path": "./basis_change/"
})


@[exercise]({
    "id": "single_qubit_gates__prepare_minus",
    "title": "Prepare Minus",
    "path": "./prepare_minus/"
})

@[section]({
    "id": "single_qubit_gates__phase_shift_gates",
    "title": "Phase Shift Gates"
})

The next two gates are known as **phase shift gates**. They apply a phase to the $\ket{1}$ state, and leave the $\ket{0}$ state unchanged.

<div class="table-with-latex">
  <table>
    <tr>
      <th>Gate</th>
      <th>Matrix</th>
      <th>Ket-Bra</th>
      <th>Applying to $\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$</th>
      <th>Applying to basis states</th>
      </tr>
    <tr>
      <td>$S$</td>
      <td>$\begin{bmatrix} 1 & 0 \\ 0 & i \end{bmatrix}$</td>
      <td>$\ket{0}\bra{0} + i\ket{1}\bra{1}$</td>
      <td>$S\ket{\psi} = \alpha\ket{0} + i\beta\ket{1}$</td>
      <td>
        $S\ket{0} = \ket{0}$<br>
        $S\ket{1} = i\ket{1}$<br>
        $S\ket{+} = \ket{i}$<br>
        $S\ket{-} = \ket{-i}$<br>
        $S\ket{i} = \ket{-}$<br>
        $S\ket{-i} = \ket{+}$<br>
      </td>
      </tr>
    <tr>
      <td>$T$</td>
      <td>$\begin{bmatrix} 1 & 0 \\ 0 & e^{i\pi/4} \end{bmatrix}$</td>
      <td>$\ket{0}\bra{0} + e^{i\pi/4}\ket{1}$$\bra{1}$</td>
      <td>$T\ket{\psi} = \alpha\ket{0} + e^{i\pi/4} \beta \ket{1}$</td>
      <td>
        $T\ket{0} = \ket{0}$<br>
        $T\ket{1} = e^{i\pi/4}\ket{1}$
      </td>
    </tr>
  </table>
</div>

> Notice that applying the $T$ gate twice is equivalent to applying the $S$ gate, and applying the $S$ gate twice is equivalent to applying the $Z$ gate:
$$T^2 = S, S^2 = Z$$

@[exercise]({
    "id": "single_qubit_gates__phase_i",
    "title": "Relative Phase i",
    "path": "./phase_i/"
})

@[exercise]({
    "id": "single_qubit_gates__three_quarters_pi_phase",
    "title": "Three-Fourths Phase",
    "path": "./three_quarters_pi_phase/"
})

@[section]({
    "id": "single_qubit_gates__rotation_gates",
    "title": "Rotation Gates"
})

The next few gates are parametrized: their exact behavior depends on a numeric parameter - an angle $\theta$, given in radians.
These gates are the $X$ rotation gate $R_x(\theta)$, $Y$ rotation gate $R_y(\theta)$, $Z$ rotation gate $R_z(\theta)$, and the arbitrary phase gate $R_1(\theta)$.
Note that for the first three gates the parameter $\theta$ is multiplied by $\frac{1}{2}$ within the gate's matrix.

> These gates are known as rotation gates, because they represent rotations around various axes on the Bloch sphere. The Bloch sphere is a way of representing the qubit states visually, mapping them onto the surface of a sphere.
> Unfortunately, this visualization isn't very useful beyond single-qubit states, which is why this kata doesn't go into details.
> If you're curious about it, you can learn more in <a href="https://en.wikipedia.org/wiki/Bloch_sphere" target="_blank">this Wikipedia article</a>.

<div class="table-with-latex">
  <table>
    <tr>
      <th>Gate</th>
      <th>Matrix</th>
      <th>Applying to $\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$</th>
      <th>Applying to basis states</th>
     </tr>
    <tr>
      <td>$R_x(\theta)$</td>
      <td>
      $$
      \begin{bmatrix} \cos\frac{\theta}{2} & -i\sin\frac{\theta}{2} \\ -i\sin\frac{\theta}{2} & \cos\frac{\theta}{2} \end{bmatrix}
      $$
      </td>
      <td>$R_x(\theta)\ket{\psi} = (\alpha\cos\frac{\theta}{2} - i\beta\sin\frac{\theta}{2})\ket{0} + (\beta\cos\frac{\theta}{2} - i\alpha\sin\frac{\theta}{2})\ket{1}$</td>
      <td>
        $R_x(\theta)\ket{0} = \cos\frac{\theta}{2}\ket{0} - i\sin\frac{\theta}{2}\ket{1}$<br>
        $R_x(\theta)\ket{1} = \cos\frac{\theta}{2}\ket{1} - i\sin\frac{\theta}{2}\ket{0}$
      </td>
     </tr>
    <tr>
      <td>$R_y(\theta)$</td>
      <td>$\begin{bmatrix} \cos\frac{\theta}{2} & -\sin\frac{\theta}{2} \\ \sin\frac{\theta}{2} & \cos\frac{\theta}{2} \end{bmatrix}$</td>
      <td>$R_y(\theta)\ket{\psi} = (\alpha\cos\frac{\theta}{2} - \beta\sin\frac{\theta}{2})\ket{0} + (\beta\cos\frac{\theta}{2} + \alpha\sin\frac{\theta}{2})\ket{1}$</td>
      <td>
        $R_y(\theta)\ket{0} = \cos\frac{\theta}{2}\ket{0} + \sin\frac{\theta}{2}\ket{1}$<br>
        $R_y(\theta)\ket{1} = \cos\frac{\theta}{2}\ket{1} - \sin\frac{\theta}{2}\ket{0}$
      </td>
      </tr>
    <tr>
      <td>$R_z(\theta)$</td>
      <td>$\begin{bmatrix} e^{-i\theta/2} & 0 \\ 0 & e^{i\theta/2} \end{bmatrix}$</td>
      <td>$R_z(\theta)\ket{\psi} = \alpha e^{-i\theta/2}\ket{0} + \beta e^{i\theta/2}\ket{1}$</td>
      <td>
        $R_z(\theta)\ket{0} = e^{-i\theta/2}\ket{0}$<br>
        $R_z(\theta)\ket{1} = e^{i\theta/2}\ket{1}$
      </td>
    </tr>
    <tr>
      <td>$R_1(\theta)$</td>
      <td>$\begin{bmatrix} 1 & 0 \\ 0 & e^{i\theta} \end{bmatrix}$</td>
      <td>$R_1(\theta)\ket{\psi} = \alpha\ket{0} + \beta e^{i\theta}\ket{1}$</td>
      <td>
        $R_1(\theta)\ket{0} = \ket{0}$<br>
        $R_1(\theta)\ket{1} = e^{i\theta}\ket{1}$
      </td>  
    </tr>
  </table>
</div>

You have already encountered some special cases of the $R_1$ gate:

$$T = R_1(\frac{\pi}{4}), S = R_1(\frac{\pi}{2}), Z = R_1(\pi)$$

In addition, this gate is closely related to the $R_z$ gate: applying $R_1$ gate is equivalent to applying the $R_z$ gate, and then applying a global phase:

$$R_1(\theta) = e^{i\theta/2}R_z(\theta)$$

In addition, the rotation gates are very closely related to their respective Pauli gates:

$$X = iR_x(\pi), Y = iR_y(\pi), Z = iR_z(\pi)$$

@[exercise]({
    "id": "single_qubit_gates__complex_phase",
    "title": "Complex Relative Phase",
    "path": "./complex_phase/"
})

@[exercise]({
    "id": "single_qubit_gates__amplitude_change",
    "title": "Amplitude Change",
    "path": "./amplitude_change/"
})

@[exercise]({
    "id": "single_qubit_gates__prepare_rotated_state",
    "title": "Prepare Rotated State",
    "path": "./prepare_rotated_state/"
})

@[exercise]({
    "id": "single_qubit_gates__prepare_arbitrary_state",
    "title": "Prepare Arbitrary State",
    "path": "./prepare_arbitrary_state/"
})

@[section]({
    "id": "single_qubit_gates__conclusion",
    "title": "Conclusion"
})

Congratulations!  In this kata you learned the matrix and the ket-bra representation of quantum gates. Here are a few key concepts to keep in mind:

- Single-qubit gates act on individual qubits and are represented by $2 \times 2$ unitary matrices.
- The effect of a gate applied to a qubit can be calculated by multiplying the corresponding matrix by the state vector of the qubit.
- Applying several quantum gates in sequence is equivalent to performing several matrix multiplications.
- Any square matrix can be represented as a linear combination of the outer products of vectors. The outer product is the matrix product of $\ket{\phi}$ and $\bra{\psi}$, denoted as $\ket{\phi}\bra{\psi}$.
- Pauli gates, identity and Hadamard gates, phase shift gates, and rotation gates are examples of single-qubit gates. All of them are available in Q#.

Next, you'll learn about multi-qubit systems in the “Multi-Qubit Systems” kata.

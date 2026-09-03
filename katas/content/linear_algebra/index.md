# Linear Algebra

@[section]({
    "id": "linear_algebra__overview", 
    "title": "Overview" 
})

This kata introduces you to the basics of linear algebra. Linear algebra is a branch of mathematics dedicated to studying the properties of matrices and vectors, which are used extensively in quantum computing to represent quantum states and operations on them.
This kata doesn't come close to covering the full breadth of the topic, but it helps you get started with the main concepts of linear algebra used in quantum computing.

**This kata covers the following topics:**

* Matrices and vectors
* Basic matrix operations
* Operations and properties of complex-valued matrices
* Inner and outer vector products
* Tensor product

**What you should know to start working on this kata:**

* Basic knowledge of complex numbers. If you need a review of this topic, you can check out the Complex Arithmetic kata before tackling this one.

*In this kata, the exercises focus on pen-and-paper work rather than on writing programs, since you're much more likely to do the linear algebra for a quantum computing problem before you start writing the code for it than to do it in Q#. Every task asks you to fill in the results of a math calculation for a specific example that you can do by hand. The Q# code is used as a way to check the results of the calculations rather than as a way to carry them out.*

@[section]({ 
    "id": "linear_algebra__matrices_and_vectors", 
    "title": "Matrices and Vectors" 
})

A **matrix** is set of numbers arranged in a rectangular grid. Here is a $2$ by $2$ matrix:

$$A =
\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix}$$

The notation $A_{i,j}$ refers to the element in row $i$ and column $j$ of matrix $A$ (all indices are 0-based). In the above example, $A_{0,1} = 2$.

An $n \times m$ matrix will have $n$ rows and $m$ columns:

$$\begin{bmatrix}
    x_{0,0} & x_{0,1} & \dotsb & x_{0,m-1} \\
    x_{1,0} & x_{1,1} & \dotsb & x_{1,m-1} \\
    \vdots  & \vdots  & \ddots & \vdots  \\
    x_{n-1,0} & x_{n-1,1} & \dotsb & x_{n-1,m-1}
\end{bmatrix}$$

A $1 \times 1$ matrix is equivalent to a scalar:

$$\begin{bmatrix} 3 \end{bmatrix} = 3$$

Quantum computing uses complex-valued matrices: the elements of a matrix can be complex numbers. This, for example, is a valid complex-valued matrix:

$$\begin{bmatrix}
    1 & i \\
    -2i & 3 + 4i
\end{bmatrix}$$

Finally, a **vector** is an $n \times 1$ matrix. Here, for example, is a $3 \times 1$ vector:

$$V = \begin{bmatrix} 1 \\ 2i \\ 3 + 4i \end{bmatrix}$$

Since vectors always have a width of $1$, vector elements are sometimes written using only one index. In the above example, $V_0 = 1$ and $V_1 = 2i$.

@[section]({ 
    "id": "linear_algebra__matrix_addition", 
    "title": "Matrix Addition" 
})

The easiest matrix operation is **matrix addition**. Matrix addition works between two matrices of the same size, and adds each number from the first matrix to the number in the same position in the second matrix:

$$\begin{bmatrix}
    x_{0,0} & x_{0,1} & \dotsb & x_{0,m-1} \\
    x_{1,0} & x_{1,1} & \dotsb & x_{1,m-1} \\
    \vdots  & \vdots  & \ddots & \vdots  \\
    x_{n-1,0} & x_{n-1,1} & \dotsb & x_{n-1,m-1}
\end{bmatrix} +$$
$$+ \begin{bmatrix}
    y_{0,0} & y_{0,1} & \dotsb & y_{0,m-1} \\
    y_{1,0} & y_{1,1} & \dotsb & y_{1,m-1} \\
    \vdots  & \vdots  & \ddots & \vdots  \\
    y_{n-1,0} & y_{n-1,1} & \dotsb & y_{n-1,m-1}
\end{bmatrix} =$$
$$= \begin{bmatrix}
    x_{0,0} + y_{0,0} & x_{0,1} + y_{0,1} & \dotsb & x_{0,m-1} + y_{0,m-1} \\
    x_{1,0} + y_{1,0} & x_{1,1} + y_{1,1} & \dotsb & x_{1,m-1} + y_{1,m-1} \\
    \vdots  & \vdots  & \ddots & \vdots  \\
    x_{n-1,0} + y_{n-1,0} & x_{n-1,1} + y_{n-1,1} & \dotsb & x_{n-1,m-1} + y_{n-1,m-1}
\end{bmatrix}$$

Similarly, you can compute $A - B$ by subtracting elements of $B$ from corresponding elements of $A$.

Matrix addition has the following properties:

* Commutativity: $A + B = B + A$
* Associativity: $(A + B) + C = A + (B + C)$

@[exercise]({ 
    "id": "linear_algebra__addition", 
    "title": "Add Matrices", 
    "path": "./addition/"
})


@[section]({ 
    "id": "linear_algebra__scalar_multiplication", 
    "title": "Scalar Multiplication" 
})

The next matrix operation is **scalar multiplication** - multiplying the entire matrix by a scalar (real or complex number):

$$a \cdot
\begin{bmatrix}
    x_{0,0} & x_{0,1} & \dotsb & x_{0,m-1} \\
    x_{1,0} & x_{1,1} & \dotsb & x_{1,m-1} \\
    \vdots  & \vdots  & \ddots & \vdots  \\
    x_{n-1,0} & x_{n-1,1} & \dotsb & x_{n-1,m-1}
\end{bmatrix} =
\begin{bmatrix}
    a \cdot x_{0,0} & a \cdot x_{0,1} & \dotsb & a \cdot x_{0,m-1} \\
    a \cdot x_{1,0} & a \cdot x_{1,1} & \dotsb & a \cdot x_{1,m-1} \\
    \vdots  & \vdots  & \ddots & \vdots  \\
    a \cdot x_{n-1,0} & a \cdot x_{n-1,1} & \dotsb & a \cdot x_{n-1,m-1}
\end{bmatrix}$$

Scalar multiplication has the following properties:

* Associativity: $x \cdot (yA) = (x \cdot y)A$
* Distributivity over matrix addition: $x(A + B) = xA + xB$
* Distributivity over scalar addition: $(x + y)A = xA + yA$

@[exercise]({ 
    "id": "linear_algebra__scalar_multiplication_ex", 
    "title": "Multiply a Matrix by a Scalar", 
    "path": "./scalar_multiplication/"
})


@[section]({ 
    "id": "linear_algebra__matrix_multiplication", 
    "title": "Matrix Multiplication" 
})

**Matrix multiplication** is a very important and somewhat unusual operation. The unusual thing about it is that neither its operands nor its output are the same size: an $n \times m$ matrix multiplied by an $m \times k$ matrix results in an $n \times k$ matrix. 
That is, for matrix multiplication to be applicable, the number of columns in the first matrix must equal the number of rows in the second matrix.

Here's how matrix product is calculated: if you're calculating $AB = C$, then

$$C_{i,j} = A_{i,0} \cdot B_{0,j} + A_{i,1} \cdot B_{1,j} + \dotsb + A_{i,m-1} \cdot B_{m-1,j} = \sum_{t = 0}^{m-1} A_{i,t} \cdot B_{t,j}$$

Here's a small example:

$$\begin{bmatrix}
    1 & 2 & 3 \\
    4 & 5 & 6
\end{bmatrix}
\begin{bmatrix}
    1 \\
    2 \\
    3
\end{bmatrix} =
\begin{bmatrix}
    1 \cdot 1 + 2 \cdot 2 + 3 \cdot 3 \\
    4 \cdot 1 + 5 \cdot 2 + 6 \cdot 3
\end{bmatrix} =
\begin{bmatrix}
    14 \\
    32
\end{bmatrix}$$

Matrix multiplication has the following properties:

* Associativity: $A(BC) = (AB)C$
* Distributivity over matrix addition: $A(B + C) = AB + AC$ and $(A + B)C = AC + BC$
* Associativity with scalar multiplication: $xAB = x(AB) = A(xB)$

> Note that matrix multiplication is **not commutative:** $AB$ rarely equals $BA$.

Another very important property of matrix multiplication is that a matrix multiplied by a vector produces another vector.

An **identity matrix** $I_n$ is a special $n \times n$ matrix which has $1$s on the main diagonal, and $0$s everywhere else:

$$I_n =
\begin{bmatrix}
    1 & 0 & \dotsb & 0 \\
    0 & 1 & \dotsb & 0 \\
    \vdots & \vdots & \ddots & \vdots \\
    0 & 0 & \dotsb & 1
\end{bmatrix}$$

What makes an identity matrix $I_n$ special is that multiplying any matrix (of compatible size) by $I_n$ returns the original matrix. That is, if $A$ is an $n \times m$ matrix:

$$AI_m = I_nA = A$$

This is why $I_n$ is called an identity matrix - it acts as a **multiplicative identity**. In other words, it's the matrix equivalent of the number $1$.

@[exercise]({ 
    "id": "linear_algebra__matrix_multiplication_ex", 
    "title": "Multiply Two Matrices", 
    "path": "./multiplication/"
})


@[section]({
    "id": "linear_algebra__inverse_matrices", 
    "title": "Inverse Matrices" 
})

A square $n \times n$ matrix $A$ is **invertible** if it has an inverse $n \times n$ matrix $A^{-1}$ with the following property:

$$AA^{-1} = A^{-1}A = I_n$$

In other words, $A^{-1}$ acts as the **multiplicative inverse** of $A$.

Another, equivalent definition highlights what makes this an interesting property. For any matrices $B$ and $C$ of compatible sizes:

$$A^{-1}(AB) = A(A^{-1}B) = B$$
$$(CA)A^{-1} = (CA^{-1})A = C$$

A square matrix has a property called the **determinant**, with the determinant of matrix $A$ being written as $|A|$. A matrix is invertible if and only if its determinant isn't equal to $0$.

For a $2 \times 2$ matrix $A$, the determinant is defined as $|A| = A_{0,0} \cdot A_{1,1} - A_{0,1} \cdot A_{1,0}$.

For larger matrices, the determinant is defined through determinants of sub-matrices. You can learn more about the determinant of matrices from [Wikipedia](https://en.wikipedia.org/wiki/Determinant) or from [Wolfram MathWorld](https://mathworld.wolfram.com/Determinant.html).

@[exercise]({ 
    "id": "linear_algebra__inverse_matrix_ex", 
    "title": "Invert a Matrix", 
    "path": "./inverse/"
})


@[section]({
    "id": "linear_algebra__transpose", 
    "title": "Transpose" 
})

The **transpose** operation, denoted as $A^T$, is essentially a reflection of the matrix across the diagonal: $A^T_{i,j} = A_{j,i}$.

Given an $n \times m$ matrix $A$, its transpose is the $m \times n$ matrix $A^T$, such that if:

$$A =
\begin{bmatrix}
    x_{0,0} & x_{0,1} & \dotsb & x_{0,m-1} \\
    x_{1,0} & x_{1,1} & \dotsb & x_{1,m-1} \\
    \vdots & \vdots & \ddots & \vdots \\
    x_{n-1,0} & x_{n-1,1} & \dotsb & x_{n-1,m-1}
\end{bmatrix}$$

then:

$$A^T =
\begin{bmatrix}
    x_{0,0} & x_{1,0} & \dotsb & x_{n-1,0} \\
    x_{0,1} & x_{1,1} & \dotsb & x_{n-1,1} \\
    \vdots & \vdots & \ddots & \vdots \\
    x_{0,m-1} & x_{1,m-1} & \dotsb & x_{n-1,m-1}
\end{bmatrix}$$

For example:

$$\begin{bmatrix}
    1 & 2 \\
    3 & 4 \\
    5 & 6
\end{bmatrix}^T =
\begin{bmatrix}
    1 & 3 & 5 \\
    2 & 4 & 6
\end{bmatrix}$$

A **symmetric** matrix is a square matrix which equals its own transpose: $A = A^T$. That is, it has reflection symmetry (hence the name) across the main diagonal. For example, the following matrix is symmetric:

$$\begin{bmatrix}
    1 & 2 & 3 \\
    2 & 4 & 5 \\
    3 & 5 & 6
\end{bmatrix}$$

The transpose of a matrix product is equal to the product of transposed matrices, taken in reverse order:

$$(AB)^T = B^TA^T$$

@[exercise]({ 
    "id": "linear_algebra__transpose_ex", 
    "title": "Transpose a Matrix", 
    "path": "./transpose/"
})


@[section]({
    "id": "linear_algebra__conjugate", 
    "title": "Conjugate" 
})

The next important single-matrix operation is the **matrix conjugate**, denoted as $\overline{A}$. This operation makes sense only for complex-valued matrices; as the name might suggest, it involves taking the complex conjugate of every element of the matrix. In matrix form, if

$$A =
\begin{bmatrix}
    x_{0,0} & x_{0,1} & \dotsb & x_{0,m-1} \\
    x_{1,0} & x_{1,1} & \dotsb & x_{1,m-1} \\
    \vdots & \vdots & \ddots & \vdots \\
    x_{n-1,0} & x_{n-1,1} & \dotsb & x_{n-1,m-1}
\end{bmatrix}$$

Then:

$$\overline{A} =
\begin{bmatrix}
    \overline{x}_{0,0} & \overline{x}_{0,1} & \dotsb & \overline{x}_{0,m-1} \\
    \overline{x}_{1,0} & \overline{x}_{1,1} & \dotsb & \overline{x}_{1,m-1} \\
    \vdots & \vdots & \ddots & \vdots \\
    \overline{x}_{n-1,0} & \overline{x}_{n-1,1} & \dotsb & \overline{x}_{n-1,m-1}
\end{bmatrix}$$

> As a reminder, a conjugate of a complex number $x = a + bi$ is $\overline{x} = a - bi$.

The conjugate of a matrix product equals to the product of conjugates of the matrices:

$$\overline{AB} = (\overline{A})(\overline{B})$$

@[exercise]({ 
    "id": "linear_algebra__conjugate_ex", 
    "title": "Conjugate of a Matrix", 
    "path": "./conjugate/"
})


@[section]({
    "id": "linear_algebra__adjoint", 
    "title": "Adjoint" 
})

The final important single-matrix operation is a combination of the previous two. The **conjugate transpose**, also called the **adjoint** of matrix $A$, is defined as $A^\dagger = \overline{(A^T)} = (\overline{A})^T$.

A matrix is known as **Hermitian** or **self-adjoint** if it equals its own adjoint: $A = A^\dagger$. For example, the following matrix is Hermitian:

$$\begin{bmatrix}
    1 & i \\
    -i & 2
\end{bmatrix}$$

The adjoint of a matrix product can be calculated as follows:

$$(AB)^\dagger = B^\dagger A^\dagger$$

@[exercise]({ 
    "id": "linear_algebra__adjoint_ex", 
    "title": "Adjoint of a Matrix", 
    "path": "./adjoint/"
})


@[section]({
    "id": "linear_algebra__unitary", 
    "title": "Unitary Matrices" 
})

**Unitary matrices** are very important for quantum computing. A matrix is unitary when it's invertible, and its inverse is equal to its adjoint: $U^{-1} = U^\dagger$. That is, an $n \times n$ square matrix $U$ is unitary if and only if $UU^\dagger = U^\dagger U = I_n$.

## 🔎 Analyze

Is this matrix unitary?

$$A = \begin{bmatrix}
    \frac{1}{\sqrt{2}} & \frac{1}{\sqrt{2}} \\
    \frac{i}{\sqrt{2}} & \frac{-i}{\sqrt{2}}
\end{bmatrix} = 
\frac{1}{\sqrt{2}} \begin{bmatrix}
    1 & 1 \\
    i & -i
\end{bmatrix}$$

<details>
<summary><b>Solution</b></summary>
To check whether the input matrix is unitary, you need to perform the following steps:

1. Calculate the adjoint of the input matrix $A^\dagger$.

    $$A^\dagger = \frac{1}{\sqrt{2}} \begin{bmatrix}
        1 & -i \\
        1 & i
    \end{bmatrix}$$

2. Multiply it by the input matrix.

    $$AA^\dagger = \frac12 \begin{bmatrix}
        1 & 1 \\
        i & -i
    \end{bmatrix} \begin{bmatrix}
        1 & -i \\
        1 & i
    \end{bmatrix} = \frac12 \begin{bmatrix}
        1 \cdot 1 + 1 \cdot 1 & 1 \cdot (-i) + 1 \cdot i \\
        i \cdot 1 + (-i) \cdot 1 & i \cdot (-i) + (-i) \cdot i
    \end{bmatrix} = \begin{bmatrix}
        1 & 0 \\
        0 & 1
    \end{bmatrix}$$

You can see that the multiplication result $AA^\dagger$ is an identity matrix, and the product $A^\dagger A$ is also an identity matrix (which you can verify in a similar manner), so
the matrix is unitary.

</details>


@[section]({
    "id": "linear_algebra__inner_product", 
    "title": "Inner Product" 
})

The **inner product** is yet another important matrix operation that is only applied to vectors. Given two vectors $V$ and $W$ of the same size, their inner product $\langle V , W \rangle$ is defined as a product of matrices $V^\dagger$ and $W$:

$$\langle V , W \rangle = V^\dagger W$$

Let's break this down so it's a bit easier to understand. A $1 \times n$ matrix (the adjoint of an $n \times 1$ vector) multiplied by an $n \times 1$ vector results in a $1 \times 1$ matrix, which is equivalent to a scalar. The result of an inner product is that scalar. 

That is, to calculate the inner product of two vectors, take the corresponding elements $V_k$ and $W_k$, multiply the complex conjugate of $V_k$ by $W_k$, and add up those products:

$$\langle V , W \rangle = \sum_{k=0}^{n-1}\overline{V_k}W_k$$

If you're familiar with the **dot product**, you'll notice that it's equivalent to inner product for real-numbered vectors.

> You might encounter other sources which define the inner product a little differently: $\langle V , W \rangle = W^\dagger V = V^T\overline{W}$, in contrast to the $V^\dagger W$ that is used here. These definitions are almost equivalent, with some differences in the scalar multiplication by a complex number.

An immediate application for the inner product is computing the **vector norm**. The norm of vector $V$ is defined as $||V|| = \sqrt{\langle V , V \rangle}$. This condenses the vector down to a single non-negative real value. If the vector represents coordinates in space, the norm happens to be the length of the vector. A vector is called **normalized** if its norm is equal to $1$.

The inner product has the following properties:

* Distributivity over addition: $\langle V + W , X \rangle = \langle V , X \rangle + \langle W , X \rangle$ and $\langle V , W + X \rangle = \langle V , W \rangle + \langle V , X \rangle$
* Partial associativity with scalar multiplication: $x \cdot \langle V , W \rangle = \langle \overline{x}V , W \rangle = \langle V , xW \rangle$
* Skew symmetry: $\langle V , W \rangle = \overline{\langle W , V \rangle}$
* Multiplying a vector by a unitary matrix **preserves the vector's inner product with itself** (and therefore the vector's norm): $\langle UV , UV \rangle = \langle V , V \rangle$

> Note that just like matrix multiplication, the inner product **isn't commutative**: $\langle V , W \rangle$ won't always equal $\langle W , V \rangle$.

@[exercise]({ 
    "id": "linear_algebra__inner_product_ex", 
    "title": "Inner Product of Two Vectors", 
    "path": "./inner_product/"
})

@[exercise]({ 
    "id": "linear_algebra__normalized_vector", 
    "title": "Normalized Vector", 
    "path": "./normalized_vector/"
})


@[section]({
    "id": "linear_algebra__outer_product", 
    "title": "Outer Product" 
})

The **outer product** of two vectors $V$ and $W$ is defined as $VW^\dagger$. That is, the outer product of an $n \times 1$ vector and an $m \times 1$ vector is an $n \times m$ matrix. If we denote the outer product of $V$ and $W$ as $X$, then $X_{i,j} = V_i \cdot \overline{W_j}$.

@[exercise]({ 
    "id": "linear_algebra__outer_product_ex", 
    "title": "Outer Product of Two Vectors", 
    "path": "./outer_product/"
})


@[section]({
    "id": "linear_algebra__tensor_product", 
    "title": "Tensor Product" 
})

The **tensor product** is a different way of multiplying matrices. Rather than multiplying rows by columns, the tensor product multiplies the second matrix by every element of the first matrix.

Given $n \times m$ matrix $A$ and $k \times l$ matrix $B$, their tensor product $A \otimes B$ is an $(n \cdot k) \times (m \cdot l)$ matrix defined as follows:

$$A \otimes B =
\begin{bmatrix}
    A_{0,0} \cdot B & A_{0,1} \cdot B & \dotsb & A_{0,m-1} \cdot B \\
    A_{1,0} \cdot B & A_{1,1} \cdot B & \dotsb & A_{1,m-1} \cdot B \\
    \vdots & \vdots & \ddots & \vdots \\
    A_{n-1,0} \cdot B & A_{n-1,1} \cdot B & \dotsb & A_{n-1,m-1} \cdot B
\end{bmatrix} =$$
$$= \begin{bmatrix}
    A_{0,0} \cdot \begin{bmatrix}B_{0,0} & \dotsb & B_{0,l-1} \\ \vdots & \ddots & \vdots \\ B_{k-1,0} & \dotsb & b_{k-1,l-1} \end{bmatrix} & \dotsb &
    A_{0,m-1} \cdot \begin{bmatrix}B_{0,0} & \dotsb & B_{0,l-1} \\ \vdots & \ddots & \vdots \\ B_{k-1,0} & \dotsb & B_{k-1,l-1} \end{bmatrix} \\
    \vdots & \ddots & \vdots \\
    A_{n-1,0} \cdot \begin{bmatrix}B_{0,0} & \dotsb & B_{0,l-1} \\ \vdots & \ddots & \vdots \\ B_{k-1,0} & \dotsb & B_{k-1,l-1} \end{bmatrix} & \dotsb &
    A_{n-1,m-1} \cdot \begin{bmatrix}B_{0,0} & \dotsb & B_{0,l-1} \\ \vdots & \ddots & \vdots \\ B_{k-1,0} & \dotsb & B_{k-1,l-1} \end{bmatrix}
\end{bmatrix} =$$
$$= \begin{bmatrix}
    A_{0,0} \cdot B_{0,0} & \dotsb & A_{0,0} \cdot B_{0,l-1} & \dotsb & A_{0,m-1} \cdot B_{0,0} & \dotsb & A_{0,m-1} \cdot B_{0,l-1} \\
    \vdots & \ddots & \vdots & \dotsb & \vdots & \ddots & \vdots \\
    A_{0,0} \cdot B_{k-1,0} & \dotsb & A_{0,0} \cdot B_{k-1,l-1} & \dotsb & A_{0,m-1} \cdot B_{k-1,0} & \dotsb & A_{0,m-1} \cdot B_{k-1,l-1} \\
    \vdots & \vdots & \vdots & \ddots & \vdots & \vdots & \vdots \\
    A_{n-1,0} \cdot B_{0,0} & \dotsb & A_{n-1,0} \cdot B_{0,l-1} & \dotsb & A_{n-1,m-1} \cdot B_{0,0} & \dotsb & A_{n-1,m-1} \cdot B_{0,l-1} \\
    \vdots & \ddots & \vdots & \dotsb & \vdots & \ddots & \vdots \\
    A_{n-1,0} \cdot B_{k-1,0} & \dotsb & A_{n-1,0} \cdot B_{k-1,l-1} & \dotsb & A_{n-1,m-1} \cdot B_{k-1,0} & \dotsb & A_{n-1,m-1} \cdot B_{k-1,l-1}
\end{bmatrix}$$

Notice that the tensor product of two vectors is another vector: if $V$ is an $n \times 1$ vector, and $W$ is an $m \times 1$ vector, $V \otimes W$ is an $(n \cdot m) \times 1$ vector.

The tensor product has the following properties:

* Distributivity over addition: $(A + B) \otimes C = A \otimes C + B \otimes C$, $A \otimes (B + C) = A \otimes B + A \otimes C$
* Associativity with scalar multiplication: $x(A \otimes B) = (xA) \otimes B = A \otimes (xB)$
* Mixed-product property (relation with matrix multiplication): $(A \otimes B) (C \otimes D) = (AC) \otimes (BD)$

@[exercise]({ 
    "id": "linear_algebra__tensor_product_ex", 
    "title": "Tensor Product of Two Matrices", 
    "path": "./tensor_product/"
})


@[section]({
    "id": "linear_algebra__conclusion", 
    "title": "Conclusion" 
})

Congratulations! Now you know enough linear algebra to get started with quantum computing!

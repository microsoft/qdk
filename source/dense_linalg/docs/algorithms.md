# The algorithms of `dense_linalg`: terms, the checked SVD and the kernels

This page is for a reader of the crate's code.
[An example](#why-every-answer-is-checked-an-example) shows why every
answer is checked. [Terms](#terms) explains each word of numerical linear
algebra the code uses, for a reader new to it.
[The checked SVD](#the-checked-svd) gives the SVD's steps, and
[The kernels](#the-kernels-matrixmultiplys-product-and-the-householder-qrs)
what computes the products and the QR decompositions. The
[README](../README.md) gives an overview.

## Why every answer is checked: an example

A matrix product state (MPS) simulator holds a quantum state as a chain
of small tensors, its **sites**, joined by **bonds**. After a gate (a
small unitary matrix) on two neighbouring sites, it splits their product,
the **gated pair** $\theta$, back into two sites, by an SVD:

1. The gated pair, each index named:

   ```text
          ┌───────────┐
   a ─────┤     θ     ├───── b      a: the bond on the left,  l values
          └─┬───────┬─┘             b: the bond on the right, r values
            │       │               s, t: each site's physical index
            s       t                     (a qubit), 2 values
   ```

2. Read as a matrix: the left site's indices $(a, s)$ as its rows, the
   right site's $(t, b)$ as its columns:

   ```text
              columns (t, b), 2r of them
             ┌───────────────┐
   rows      │               │
   (a, s),   │       θ       │
   2l of     │               │
   them      └───────────────┘
   ```

3. Factored by `Svd::of`, then the $k$ largest values kept:

   ```math
   \underbrace{\theta}_{2l \times 2r}
   \approx
   \underbrace{U}_{2l \times k}
   \,\underbrace{S}_{k \times k}\,
   \underbrace{V^\dagger}_{k \times 2r}
   ```

4. Read back as two sites, $S$ joining one of them (here the left):

   ```text
   a ── A₁ ─── k ── B₂ ── b         A₁ = U·S, a site of shape [l, 2, k]
        │           │               B₂ = V†,  a site of shape [k, 2, r]
        s           t
   ```

   A site's shape lists its indices' sizes: left bond, qubit, right bond.

`U` has orthonormal columns and `V†` orthonormal rows. So the site
without `S` keeps lengths, and the simulator relies on that without
checking it. It also keeps the $k$ largest singular values, so it relies
on their being right. A simulator splits many thousands of times per run,
and a wrong factor changes the state silently. So it needs an SVD whose
every answer is checked.

## Terms

The crate's files use the terms of numerical linear algebra. Here is
each in plain words, with a picture or a formula where one helps.

### Matrices

- **Column-major:** a matrix stored column after column. In a matrix of
  `rows` rows, the entry in row `i` and column `j` sits at `i + rows·j`.
  For a $3 \times 2$ matrix, $a_{21}$ sits at $2 + 3 \cdot 1 = 5$:

  ```math
  \begin{bmatrix}
    a_{00} & a_{01} \\
    a_{10} & a_{11} \\
    a_{20} & a_{21}
  \end{bmatrix}
  \quad\text{is stored as}\quad
  \underbrace{a_{00}\;\; a_{10}\;\; a_{20}}_{\text{column 0}}
  \;\Big|\;
  \underbrace{a_{01}\;\; a_{11}\;\; a_{21}}_{\text{column 1}}
  ```

- **Adjoint** ($A^\dagger$): the matrix flipped across its diagonal (rows
  become columns), each entry replaced by its complex conjugate:
  $(A^\dagger)_{ij} = \overline{A_{ji}}$. For example:

  ```math
  A = \begin{bmatrix} 1 & i \\ 3 & 2 - i \end{bmatrix}
  \qquad
  A^\dagger = \begin{bmatrix} 1 & 3 \\ -i & 2 + i \end{bmatrix}
  ```

### Lengths and angles

- **Norm:** a vector's length,
  $\lVert v \rVert = \sqrt{\sum_i \lvert v_i \rvert^2}$. A matrix's
  **Frobenius norm** is the same over all its entries,
  $\lVert A \rVert_F = \sqrt{\sum_{i,j} \lvert A_{ij} \rvert^2}$.
- **Orthonormal:** vectors each of norm 1 and each perpendicular
  (**orthogonal**) to the others: their inner products $u_i^\dagger u_j$
  are 0. A matrix $Q$ whose columns are orthonormal ($Q^\dagger Q = I$)
  changes no length, $\lVert Q x \rVert = \lVert x \rVert$, so
  multiplying by it loses nothing. A square such matrix is **unitary**.
- **Gram matrix** of some columns: the matrix of their inner products,
  $G = W^\dagger W$ for the columns $w_i$ of $W$. Its diagonal holds the
  squared norms, and it is diagonal when the columns are orthogonal:

  ```math
  G = W^\dagger W =
  \begin{bmatrix}
    \lVert w_0 \rVert^2 & w_0^\dagger w_1 & w_0^\dagger w_2 \\
    w_1^\dagger w_0 & \lVert w_1 \rVert^2 & w_1^\dagger w_2 \\
    w_2^\dagger w_0 & w_2^\dagger w_1 & \lVert w_2 \rVert^2
  \end{bmatrix}
  ```

### Decompositions

- **Singular value decomposition (SVD):**
  $A = U \operatorname{diag}(s) V^\dagger$. $U$ and $V$ have orthonormal
  columns (the **singular vectors**). $s$ holds the **singular values**,
  nonnegative, largest first. Each value says how much $A$ stretches the
  direction its vectors name. **Thin**: it keeps $r$ values, at most
  $\min(m, n)$ for an $m \times n$ matrix:

  ```text
       A        =      U      ·   diag(s)   ·       V†
     m × n           m × r         r × r          r × n
   ┌───────┐         ┌───┐
   │       │         │   │        ┌─────┐       ┌───────┐
   │       │    =    │   │    ·   │s₀   │   ·   │       │
   │       │         │   │        │   s₁│       │       │
   │       │         │   │        └─────┘       └───────┘
   └───────┘         └───┘
  ```

  Dropping the smallest values gives a closest matrix with fewer of
  them. That is what a truncation does (`Svd::truncated`): keeping the
  first $k$ values, $s_0$ to $s_{k-1}$, with the first $k$ columns of $U$
  and rows of $V^\dagger$, leaves out $\sqrt{s_k^2 + s_{k+1}^2 + \cdots}$
  in the Frobenius norm.

- **Rank:** the number of nonzero singular values. A matrix is
  **rank-deficient** when it has fewer than its smaller side.
- **QR decomposition:** $A = Q R$. $Q$ has orthonormal columns. $R$ is
  **upper triangular** (zero below its diagonal), or **upper
  trapezoidal** when $A$ is wide. For a tall $A$ of $4 \times 3$ ($R$ is
  $3 \times 3$) and a wide $A$ of $2 \times 4$ ($R$ is $2 \times 4$), with
  $\ast$ any value:

  ```math
  \underbrace{\begin{bmatrix}
    \ast & \ast & \ast \\
    0 & \ast & \ast \\
    0 & 0 & \ast
  \end{bmatrix}}_{\text{upper triangular}}
  \qquad
  \underbrace{\begin{bmatrix}
    \ast & \ast & \ast & \ast \\
    0 & \ast & \ast & \ast
  \end{bmatrix}}_{\text{upper trapezoidal}}
  ```

- **LQ decomposition:** $A = L Q$, the QR's mirror image. $Q$ has
  orthonormal rows. $L$ is **lower triangular** (zero above its
  diagonal), or **lower trapezoidal** when $A$ is tall. It is the QR of
  $A^\dagger$ read back: $A^\dagger = Q' R'$ gives $L = R'^\dagger$ and
  $Q = Q'^\dagger$.
- **Column-pivoted** QR ($A P = Q R$): before each step, it moves the
  remaining column of largest norm to the front. That column is the
  step's **pivot**, and the permutation $P$ records the moves. So the
  small rows of $R$, those a rank-deficient matrix gives, come last. For
  a $3 \times 3$ matrix of rank 2, the last row is of rounding's size
  ([Floating point](#floating-point)):

  ```math
  R =
  \begin{bmatrix}
    5.0 & \ast & \ast \\
    0 & 2.0 & \ast \\
    0 & 0 & 10^{-17}
  \end{bmatrix}
  ```

### Steps of the decompositions

- **Householder reflection** (a **reflector**): the unitary
  $H = I - 2 v v^\dagger / (v^\dagger v)$, which mirrors vectors across
  the plane perpendicular to $v$. $v$ is chosen so that one column
  becomes zero below its first entry. A QR applies one per step;
  [`householder_reflectors.rs`](../src/kernels/matrixmultiply_householder/householder_reflectors.rs)
  draws a whole step. On one column, with $\lvert \alpha \rvert = \lVert x \rVert$:

  ```math
  x = \begin{bmatrix} x_0 \\ x_1 \\ x_2 \end{bmatrix}
  \quad\longrightarrow\quad
  H x = \begin{bmatrix} \alpha \\ 0 \\ 0 \end{bmatrix}
  ```

- **Rotation** (a Jacobi rotation): the unitary that turns two columns
  in their own plane, by the angle that makes them perpendicular. The SVD
  rotates pairs of columns until all are perpendicular;
  [`rotation.rs`](../src/svd/jacobi/rotation.rs) gives the
  formula, with a worked example:

  ```text
  w₁ ╲     ╱ w₀              w₁ │
      ╲   ╱          →          │
       ╲ ╱                      │
        ·                       ·──────── w₀
  not perpendicular          perpendicular: w₀†w₁ = 0
  ```

### Floating point

- **Rounding** and **$\varepsilon$:** every floating-point operation may
  be off by a relative $\varepsilon = 2^{-52} \approx 2.2 \times 10^{-16}$
  (`f64::EPSILON`): it answers $(a \circ b)(1 + \delta)$ with
  $\lvert \delta \rvert \le \varepsilon$. Values of **rounding's size**
  (about $\varepsilon$ times the matrix's largest) cannot be told apart
  from 0. So the SVD's **trim** drops them.
- **Subnormal:** an `f64` below the smallest normal one (about
  $2.2 \times 10^{-308}$, the **normal range**'s floor). An `f64` can
  still hold it, but with fewer significant digits the smaller it is:

  ```text
  0     subnormal       normal range                            largest
  ├─────────────────┼───────────────────────────── … ─────────────┤
              2.2e-308 (f64::MIN_POSITIVE)                 1.8e308 (f64::MAX)
  fewer digits          53 significant bits
  towards 0
  ```

- **Cancellation:** the loss of digits when two nearly equal numbers are
  subtracted. Their difference keeps only the digits where they differ.
  So an update by subtraction drifts, and must at times be recomputed:

  ```text
    1.2345678
  − 1.2345612
  = 0.0000066     8 significant digits in, 2 out: the inputs' rounding
                  is now relative to a far smaller number
  ```

- **Well conditioned:** a problem whose answer a small change of its
  input changes little. On an **ill-conditioned** one, rounding errors
  are magnified.

## The checked SVD

[`Svd::of`](../src/svd.rs) answers
$a = u \operatorname{diag}(s) v^\dagger$ with $r$ values,
$1 \le r \le \min(\text{rows}, \text{cols})$, nonincreasing.
Its doc states its guarantees:
checked factors, only rounding's singular values left out, any scale. Its
steps name the parts of its algorithm with these terms:

- **The trim:** a column-pivoted QR $a P = Q R$. The rows of $R$ of
  smallest norm are dropped while their joint norm stays within rounding.
- **$W$:** the adjoint of the kept rows of $R P^\mathsf{T}$, of full
  rank. The SVD of $a$ follows from $W$'s.
- **The rotations:** one-sided Jacobi rotations of $W$'s columns, which
  rotate pairs of columns until every two are orthogonal.

The steps reduce the matrix to a smaller one of full rank, factor that
one by rotations, and carry its SVD back. This is Drmač and Veselić's
route (the Further reading of [`svd.rs`](../src/svd.rs)):

- **Steps 1 to 3 prepare the matrix.** They set the zero matrix aside,
  make the matrix tall, and bring its entries near 1. The later steps
  see a tall matrix $B$ whose largest squared norms neither overflow nor
  underflow.
- **Step 4 reduces it.** The pivoted QR $B P = Q R$, trimmed, gives
  $B \approx Q_{\text{kept}} W^\dagger$. $W$ has full rank, and only as
  many columns as values kept. What the trim drops is of rounding's
  size.
- **Step 5 factors the smaller matrix.** The rotations give
  $W = U_W \operatorname{diag}(s) V_W^\dagger$. When $W$'s columns scaled
  to unit norm are well conditioned, rotations find the small singular
  values about as accurately, relative to their size, as the large ones.
  On a nearly orthogonal $W$, as an MPS simulation of H₂ (a chemistry
  instance the QDK simulates) gives, a few sweeps suffice.
- **Step 6 carries the SVD back:**
  $B = (Q_{\text{kept}} V_W) \operatorname{diag}(s) U_W^\dagger$. So
  $B$'s singular values are $W$'s, its left vectors
  $Q_{\text{kept}} V_W$, and its right vectors $U_W$.
- **The checks** then test the answer itself, whatever the steps did.

Each step, how and why:

| Step                   | How                                                                                     | Why                                                                                                                                                     |
| ---------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Zero matrix         | One value, 0, and the first unit vectors.                                               | It has no rank to keep and no largest entry to scale by.                                                                                                |
| 2. Wide matrix         | Factored through its adjoint.                                                           | The rotated columns are then the shorter side.                                                                                                          |
| 3. Scaling             | By a power of two when the largest entry is far from 1.                                 | The largest squared norms neither overflow nor underflow. It rounds only entries it takes below the normal range.                                       |
| 4. Trim                | Drops the rows of $R$ of smallest norm, chosen by norm, not by pivot.                   | The rest sees full rank. Only to within about $\sqrt{\varepsilon}/2$, the pivoting keeps $\lvert R_{ii} \rvert$ nonincreasing.                          |
| 5. Rotations           | $W$'s columns rotated until orthogonal.                                                 | Orthogonal columns give $W$'s SVD. A pair still rotating at the cap of sweeps is reported as a failure.                                                 |
| 6. Left vectors        | The kept columns of $Q$, applied by its reflectors, times $W$'s right singular vectors. | Cheaper than forming $Q$. The reflectors act on the answer's $r$ columns alone, and skip those after the last kept row.                                 |
| Checks, on each answer | Reconstruction and orthonormality, to LAPACK's bounds.                                  | No SVD is trusted. A library's has answered wrong factors without a failure. The rotations' answer also passes through the pivoted QR and the products. |

[`svd.rs`](../src/svd.rs)'s header gives the details, as
steps and as a pipeline.
[`svd/checks.rs`](../src/svd/checks.rs) gives the checks,
and [`svd/jacobi.rs`](../src/svd/jacobi.rs) the rotations:
the sweeps, the order they visit the pairs in, and the steps that make one
pair orthogonal. The header of
[`lib.rs`](../src/lib.rs) lists every file of the
crate, the rotations' parts among them.

## The kernels: matrixmultiply's product and the Householder QRs

The crate computes its products and QR decompositions through the
crate-private trait [`Kernels`](../src/kernels.rs). Its
implementation,
[`kernels/matrixmultiply_householder.rs`](../src/kernels/matrixmultiply_householder.rs),
takes the product from matrixmultiply. The thin QR, the column-pivoted
QR and the application of its $Q$ are the crate's own Householder
reflections,
[`householder_reflectors.rs`](../src/kernels/matrixmultiply_householder/householder_reflectors.rs),
whose header draws one step. Both QRs reduce each column by one step, and
make their reflectors by one rule, which takes the phase of a
subnormal entry without losing its modulus. As the trim needs, the
pivoted QR keeps each pivot's $\lvert R_{kk} \rvert$ at least the norm of
every later column's remaining rows, to within about
$\sqrt{\varepsilon}/2$ of the pivot. That allowance is its norms'
downdate's, explained in the file's header.

Each computation runs on the calling thread, in pure Rust, so the crate
builds for WebAssembly too.

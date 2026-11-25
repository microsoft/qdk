# The backend contract: the site layout, the gate split and the centre move

A backend gives the engine its site tensors and two operations on
neighbouring sites: a gate split and a move of the centre. The trait
[`SiteAlgebra`](../src/site_algebra.rs) is that contract. Its rustdoc
states each rule. This page is for a backend's author: it draws the layout
of a site in memory, writes each operation as formulas, and says how the
conformance kit checks them. The crate
[README's Concepts](../README.md#concepts) define site, bond, isometry, centre,
gated pair, split and move; the
[Terms of `dense_linalg`](../../dense_linalg/docs/algorithms.md#terms) define the words of
linear algebra (SVD, QR, orthonormal, Frobenius norm).

The engine checks every argument before it calls: extents, lengths, the
call's settings. So a backend checks none, but one: a read past a
tensor's end is refused, with an error or a panic, and never answered
with memory that is not the tensor's.

## The layout

A site $T[a, s, b]$ has extents $[l, 2, r]$: its left bond, its qubit and
its right bond. Every tensor holds complex doubles, its first index
fastest. So its element $(a, s, b)$ is at

```math
a + l\,(s + 2b).
```

The same elements are, with no copy, two matrices: the qubit with the
left bond, or with the right bond. For $l = 3$ and $r = 2$, each entry
below is an element's offset:

```math
\underbrace{
\begin{array}{c|cc}
  & b{=}0 & b{=}1 \\ \hline
  a{=}0,\ s{=}0 & 0 & 6 \\
  a{=}1,\ s{=}0 & 1 & 7 \\
  a{=}2,\ s{=}0 & 2 & 8 \\
  a{=}0,\ s{=}1 & 3 & 9 \\
  a{=}1,\ s{=}1 & 4 & 10 \\
  a{=}2,\ s{=}1 & 5 & 11
\end{array}
}_{(2l) \times r:\ \text{rows } a + l s}
\qquad\qquad
\underbrace{
\begin{array}{c|cccc}
  & s{=}0,\ b{=}0 & s{=}1,\ b{=}0 & s{=}0,\ b{=}1 & s{=}1,\ b{=}1 \\ \hline
  a{=}0 & 0 & 3 & 6 & 9 \\
  a{=}1 & 1 & 4 & 7 & 10 \\
  a{=}2 & 2 & 5 & 8 & 11
\end{array}
}_{l \times (2r):\ \text{columns } s + 2b}
```

Both are column-major. The split and the move read the left site of a
pair the first way and the right site the second way, so they factor
sites with no copy. A call's outputs are packed: a site of extents
$[l, 2, r]$ holds exactly $2lr$ elements, so a read of one more is
refused.

Gates are row-major, the left site the most significant bit. A two-site
gate's entry $G[(s, t), (p, q)]$ is at $(2s + t) \cdot 4 + 2p + q$, and a
one-site gate's $G[s, p]$ at $2s + p$.

## The gate split

[`GateSplitCall`](../src/site_algebra/gate_split.rs) passes a pair
$A[a, p, j]$ of extents $[l, 2, \chi]$ and $B[j, q, b]$ of extents
$[\chi, 2, r]$, a two-site gate $G$, a cap $m$ on the bond, a cutoff
$\delta \in [0, 1)$, and the side that holds the centre afterwards. The
gated pair is

```math
\theta[a, s, t, b] = \sum_{p, q, j} G[(s, t), (p, q)]\, A[a, p, j]\, B[j, q, b],
```

read as the $(2l) \times (2r)$ matrix of rows $a + l s$ and columns
$t + 2b$. Let $\theta = U \Sigma V^\dagger$ be its SVD, with singular
values $\sigma_1 \ge \sigma_2 \ge \cdots$. The split keeps the $k$
largest, divides them by their norm, and gives them to the centre's side.
With $U_k$, $\Sigma_k$ and $V_k$ the first $k$ columns and values, the
outcome's sites $A'$ and $B'$ are

```math
\tilde\Sigma_k = \frac{\Sigma_k}{\lVert \Sigma_k \rVert_F},
\qquad
\begin{cases}
  A' = U_k \tilde\Sigma_k,\ \ B' = V_k^\dagger
    & \text{centre on the left} \\[2pt]
  A' = U_k,\ \ B' = \tilde\Sigma_k V_k^\dagger
    & \text{centre on the right}
\end{cases}
```

$A'$ is read as a $(2l) \times k$ matrix and $B'$ as a $k \times (2r)$
one. The pair and its gate contract into $\theta$:

```text
  a ── A ── χ ── B ── b                 ┌────────────┐
       │         │                 a ───┤     θ      ├─── b
     ┌─┴─────────┴─┐       =            └─┬────────┬─┘
     │      G      │                      │        │
     └─┬─────────┬─┘                      s        t
       │         │
       s         t
```

and the split factors it back into two sites, here with the centre on
the left:

```text
  a ── A' ── k ── B' ── b         A': the centre
       │          │               B': an isometry
       s          t
```

Up to rounding, the outcome meets these rules
([`SiteAlgebra::gate_split`](../src/site_algebra.rs)):

| Rule                                         | Formula                                                                                             |
| -------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| The product is the truncated SVD, normalised | $A' B' = U_k \tilde\Sigma_k V_k^\dagger$                                                            |
| The bond                                     | $1 \le k \le \min(m, 2l, 2r)$: the largest value is kept whatever the cutoff                        |
| The cutoff, unless the cap cut the bond      | $\sum_{i > k} \sigma_i^2 \le \delta \sum_i \sigma_i^2$                                              |
| The site away from the centre                | an isometry: $B' B'^\dagger = I_k$ with the centre on the left, $A'^\dagger A' = I_k$ on the right  |
| The retained fraction                        | $f = \sum_{i \le k} \sigma_i^2 \big/ \sum_i \sigma_i^2$, exactly 1 when every dropped value is zero |
| The inputs                                   | unchanged: the caller may still hold them, as the sites of a state it forked                        |

The contract leaves a backend some latitude, so that a backend whose
library decides these keeps its own rule:

- the bond need not be the smallest that meets the cutoff;
- a tail whose squares sum to exactly $\delta$'s share may be kept or
  dropped;
- beyond $\delta$, values of rounding's size may be dropped too. An SVD
  may give such values where the exact ones are zero. So a cutoff of 0
  drops the zeros, and may drop such values.

## The centre move

[`QrMoveCall`](../src/site_algebra/qr_move.rs) passes the same pair, a
one-site gate $G$ or none, and the side the centre moves to. The gate
applies to the site the centre leaves, $\hat A$ or $\hat B$:

```math
\hat A[a, s, j] = \sum_p G[s, p]\, A[a, p, j],
\qquad
\hat B[j, t, b] = \sum_q G[t, q]\, B[j, q, b].
```

| Centre moves | Factored                                                            | Outcome                | Bond $k$         |
| ------------ | ------------------------------------------------------------------- | ---------------------- | ---------------- |
| right        | $\hat A$ as $(2l) \times \chi$: $\hat A = Q R$, $Q^\dagger Q = I_k$ | $A' = Q$, $\ B' = R B$ | $\min(\chi, 2l)$ |
| left         | $\hat B$ as $\chi \times (2r)$: $\hat B = L Q$, $Q Q^\dagger = I_k$ | $A' = A L$, $\ B' = Q$ | $\min(\chi, 2r)$ |

Nothing is truncated: $A' B' = \hat A B$ when the centre moves right, and
$A \hat B$ when it moves left. The site the centre leaves is an isometry,
as after a split. The centre moved right, with no gate:

```text
  a ── A ─ χ ── B ── b      →      a ── Q ─ k ── R·B ── b
       │        │                       │         │
       s        t                       s         t
```

## How the conformance kit checks them

The [kit](../src/site_algebra/conformance.rs) calls each operation on
seeded inputs and compares what it returns with the formulas above,
never with another algebra: $\theta$ by the formula's own loops, and its
singular values $\sigma_i$. A split's factors are not unique when it
truncates, so the kit checks its product by the Eckart–Young theorem. No
matrix $X$ of rank $k$ is nearer $\theta$, normalised, than its SVD
truncated to $k$ values, and that distance is set by the dropped values:

```math
\Big\lVert \frac{\theta}{\lVert \theta \rVert_F} - X \Big\rVert_F^2
\;\ge\; \frac{\sum_{i > k} \sigma_i^2}{\sum_i \sigma_i^2} = 1 - f .
```

Where $\sigma_k > \sigma_{k+1}$, only the truncated SVD reaches that
bound. So the kit checks that $A' B'$ has norm 1, and that $\sqrt f\, A' B'$
is at a squared distance of $1 - f$ from
$\theta / \lVert \theta \rVert_F$. Each other rule of the tables is
checked as written, and so are the storage calls and a read past a new
site's end, which must be refused. The latitude above is
not checked. [`conformance.rs`](../src/site_algebra/conformance.rs)'s
header lists the pairs, caps and cutoffs the cases run on, and each case
gives an example of the failure it reports.

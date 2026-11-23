# The canonical-form MPS engine: backend calls, the centre, and numerics

The details behind [the engine](../README.md#the-engine) of the crate's
README: the backend operations each of its [phases](../README.md#phases)
calls, in order, where each method leaves the orthogonality centre, and the
numerics of its truncations and reads.

## Backend calls of each phase

Each band of the diagram is one `MpsEngine` method, or one case of
`apply`, with the phases it runs and their `SiteAlgebra` operations in
order. The check and the pend phases call no operation.

```mermaid
sequenceDiagram
  participant S as MPS simulation method
  participant E as CanonicalMpsEngine
  participant A as SiteAlgebra (the backend)
  Note over S,A: prepare: check, upload
  S->>E: prepare(capacity)
  E->>A: upload, once per site
  Note over S,A: apply, one site: check, pend
  S->>E: apply(state, [s], gate)
  Note over S,A: apply, two sites: check, then pend, or window then pend
  S->>E: apply(state, [a, b], gate)
  Note over E,A: window (see below), of the window the gate does not fit
  Note over S,A: probability_of_one: check, window, move, read
  S->>E: probability_of_one(state, s)
  Note over E,A: window (see below)
  E->>A: qr_move, once per site the centre crosses
  E->>A: read, of site s
  Note over S,A: project: check, window, move, read, upload
  S->>E: project(state, s, one)
  Note over E,A: window (see below)
  E->>A: qr_move and read, as probability_of_one
  E->>A: upload, of site s projected
  Note over S,A: conditional_probability_of_one: check; with conditions, a fork, then project per condition, then a read on the fork; without, as probability_of_one
  S->>E: conditional_probability_of_one(state, s, given)
  Note over E,A: with conditions: window (see below), toward the centre
  E->>A: duplicate, once per site
  E->>A: per condition, as project, on the fork
  E->>A: as probability_of_one, on the fork
  Note over E,A: without conditions: as probability_of_one, no duplicate
  Note over S,A: sample: check; per new prefix of values, as conditional_probability_of_one
  S->>E: sample(state, sites, shots, seed)
  Note over E,A: the empty prefix: as probability_of_one, of the first site
  Note over E,A: each new nonempty prefix: as with conditions, on a fork
  Note over S,A: expectation: check, window, read, contract
  S->>E: expectation(state, observable)
  Note over E,A: window (see below)
  E->>A: read, once per site between the terms and the centre
  Note over S,A: fork: window, duplicate
  S->>E: fork(state)
  Note over E,A: window (see below)
  E->>A: duplicate, once per site
  Note over S,A: shape, retained_weight: window
  S->>E: shape(state) or retained_weight(state)
  Note over E,A: window (see below)
```

A "window" step runs only when a window is pending. Its calls depend on
the window's sites:

- **two neighbours:** `qr_move`s to bring the centre onto the pair, then
  one `gate_split`;
- **any other window**, an MPO on a span of $d + 1$ sites: `qr_move`s to
  bring the centre into the span; per site of the span, a `read` and an
  `upload` of the site multiplied by its operator tensor; $d - 1$
  `qr_move`s across the span; and $d$ `gate_split`s, the identity as gate,
  one per cut ([`distant_sites.md`](distant_sites.md)).

## Where the centre goes

The centre moves only as far as an operation needs, and the application of
a window leaves it on the end of the window's span the next operation
needs, so a sequence of operations on nearby sites moves it little. After
each method, with $c$ the centre before it:

| `MpsEngine` method                 | The centre after it                                                                                                                                                                   |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `prepare`                          | site 0                                                                                                                                                                                |
| `apply`, one site                  | $c$: the gate waits, pending                                                                                                                                                          |
| `apply`, two sites                 | $c$ when the gate joins the pending window or opens one; else the end of the applied window's span nearer the gate's lower site                                                       |
| `probability_of_one`, `project`    | the site read: the pending window is applied toward it first, which leaves the centre on its span's end nearer the site                                                               |
| `conditional_probability_of_one`   | with conditions, as for `fork`, the conditions and the read on the fork; without, as for `probability_of_one`                                                                         |
| `sample`                           | the first site sampled, when it asks for a probability: its first read is that site's, without conditions; $c$ when it asks for none (no shots, no sites, a refusal, too many values) |
| `expectation`                      | $c$, or the end of the pending window's span nearer $c$, when a window is pending; the reads copy the sites to the host without moving it                                             |
| `fork`, `shape`, `retained_weight` | as for `expectation`; a fork starts with its parent's centre                                                                                                                          |

## Numerics

**The truncation.** A split's `gate_split` keeps the largest singular
value and drops values whose squares sum to at most the cutoff times the
sum of all their squares, unless the cap cuts more: the capacity's bond at
the cut. Every split, an MPO's included, is at a caller's cut, so every
split leaves a bond at rest; inside an MPO's span, a bond grows, before
its split, to at most the capacity's times the operator's bond (at most
4; [`distant_sites.md`](distant_sites.md)). These are the bounds of
`gate_split`'s contract ([`site_algebra.rs`](../src/site_algebra.rs)); which values
a split drops within them is the algebra's to choose. A split normalises
the values it keeps, so the chain keeps a norm of about 1.

**The retained weight.** Each split returns the retained fraction
$f \in (0, 1]$, the share of the squared singular values it kept, and
the state's retained weight is the product of the retained fractions,
$W = \prod_i f_i$; the part of a window's operator that its
factorisation drops, at or below the operator's rank cutoff, is not
counted ([`distant_sites.md`](distant_sites.md)). The engine keeps $W$
apart from the chain's norm, and fails a split with `WeightUnderflow`
when $W$ would fall below the smallest normal `f64`, about `2.2e-308`,
rather than let it reach zero, by which callers divide.

**A probability.** With the centre on site $s$, the rest of the chain drops
out of the site's reads. The weight of a value $b$ is
$w_b = \sum_{a,c} \lvert T[a, b, c] \rvert^2$ over the centre's tensor
$T$, its pending gate applied, and

$$P(b) = \frac{w_b}{w_0 + w_1}.$$

The rules a read keeps (each value's probability from its own weight, the
state unchanged) are stated in
[`measurement.rs`](../src/engine/measurement.rs).

**A projection.** It returns $p = P(b)$, refuses $p$ of zero or NaN, zeroes
the other value's elements, and divides the site by the norm of what it
kept. It computes the norm as $m \sqrt{\sum_x \lvert x / m \rvert^2}$
over the kept elements $x$, with $m$ their largest modulus, not as
$\sqrt{p}$, so the site it hands the algebra, and with it the state, has a
norm of 1 to rounding, even when $p$ is subnormal, and later projections
stay possible. It truncates nothing, so $W$ is unchanged, and it does not
compress the chain again; `CanonicalMpsEngine`'s `project`, in
[`engine.rs`](../src/engine.rs), says why.

**An expectation.** An observable $c_0 + \sum_k c_k P_k$, each $P_k$ a
product of one-site Paulis, has the value

$$c_0 + \sum_k c_k \frac{\langle \psi \vert P_k \vert \psi \rangle}{\lVert T_\text{centre} \rVert^2}.$$

The sites left of the centre are isometries from the left, so the
environment on any bond left of the centre is the identity, and likewise
from the right; a term's contraction runs only over the sites between its
own and the centre. How the terms are evaluated together, each site read
once, is in [`expectation.rs`](../src/engine/expectation.rs).

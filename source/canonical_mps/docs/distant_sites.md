# Gates on distant sites, and windows on three sites

A split factors the tensor of two neighbouring sites, so it applies a gate
on neighbours; a gate on sites further apart, or the gates that fusion
gathers on three sites, act on more than a pair. This page gives the
approach the engine uses, a matrix product operator (MPO) across the span;
why it loses no weight beyond what the capacity and the cutoff drop at the
caller's cuts, and, in the factorisation of the operator, a part of the
operator at or below the operator's rank cutoff; how routing by SWAPs
compares, with the ways its splits may truncate; and the alternatives that
could be explored later. The crate's
[README](../README.md#how-each-mpsengine-method-is-implemented) summarizes
it, and [`src/engine/mpo.rs`](../src/engine/mpo.rs) implements it.

The routings below were built only to be measured against each other, on
the CPU, and are not in the crate. Their error is the infidelity $1 - F$
to the exact state, except for H₂, where it is the total variation distance (TVD)
of the readout to the exact one, and for the mirror circuits, a circuit
followed by its inverse, whose exact state is $\lvert 0 \dots 0 \rangle$, where it
is $1 - P(0 \dots 0)$.

## The approach: a matrix product operator

Fusion gathers two-site gates on up to three sites into one pending
window, whose operator acts on its sites, `low` the first and `high` the
last; its span is the sites from `low` to `high`. A window on two
neighbours is applied by one split. Any other is applied in three steps:

1. **Factor.** The window's operator, on $k$ sites, is factored on the
   host by $k - 1$ SVDs into one tensor per site, linked by operator
   bonds. Each SVD already trims the values its rank-revealing QR finds
   to be rounding (the dense linear algebra's `Svd::of`), and the
   factorisation also drops the values at or below the operator's rank cutoff
   (`OPERATOR_RANK_CUTOFF`, in [`policy.rs`](../src/engine/policy.rs))
   times the largest: rounding the trim kept, or a part of the operator
   that small, which the retained weight does not count. A CNOT's bond is
   2, a general two-site gate's at most 4. The sites of the span between
   the window's carry the identity, with the bond that crosses them.
2. **Multiply.** With the centre in the span, each site of the span is
   multiplied by its tensor, on the host, and uploaded: the bond at each
   cut of the span becomes the chain's bond there times the operator's.
3. **Compress.** Exact moves of the centre across the span, then one split
   per cut, the identity as gate, compress it back, each split truncating
   its cut as a gate's split does.

The chain is in the caller's order after every call, so no read, and no
later call's result, sees how the window was applied; only the statistics
count it.

**Every split is at a caller's cut.** The splits of step 3 run with every
other site, outside the span or left by the sweep's moves and splits, an
isometry towards the cut, so each factors the state's exact Schmidt
decomposition at a caller's cut, where the state must fit the
capacity anyway. So an MPO discards nothing beyond what the cutoff drops
at its splits (negligible at `DEFAULT_CUTOFF`), what the capacity cuts
at the caller's cuts, and, in the factorisation of step 1, a part of the
operator at or below the operator's rank cutoff.

**The transient bond.** During step 2 the bond at a cut of the span is the
chain's bond there times the operator's: at most the capacity's times 4.
`CanonicalMpsState::split_statistics` reports the largest transient bond
apart from the largest bond at rest.

**The cost.** A window on a span of $d + 1$ sites costs the moves that
bring the centre into the span, then $d - 1$ moves of the centre and $d$
splits, each on matrices up to the capacity's times the operator's bond
on a side, plus the reads, products and uploads of the span's sites on
the host. Every backend call is one of the operations the engine already
makes elsewhere: moves and splits as for a gate, reads and uploads as for
a projection.

## Routing by SWAPs, and how its splits may truncate

**Routing by SWAPs, `swap-back-2x`.** Moves the site at `high` down to
`low + 1` by adjacent SWAPs (outbound), applies the gate to the pair at
`low`, and moves the site back up by SWAPs (return): $2(d - 1) + 1$
splits for sites $d$ apart, against $d$ splits and $d - 1$ moves of the
centre for an MPO, and one window per gate, since a route acts on a pair
at a time. Its outbound splits apply the cutoff alone and keep bonds up
to $2\chi$, $\chi$ the capacity's largest bond, which the following paragraphs
explain; they also drop a tail below the cutoff at cuts that are not the
caller's, which an MPO never does (the case is in
[the crate's MPO tests](../src/tests/engine/mpo.rs)).

The split of each outbound SWAP drops the tail below the cutoff and
applies no capacity. Its bond is still bounded: after the SWAP that moves
the site down across cut $c$, the sites left of cut $c$ are the caller's
sites left of cut $c - 1$, a set $A$, and the moved site $h$. Write the
state in $A$'s Schmidt basis,
$\sum_i s_i \lvert a_i \rangle \lvert b_i \rangle$ with $r$ terms, $r$ the
bond the chain holds at cut $c - 1$; each $\lvert b_i \rangle$ expands over
$h$'s two levels, so the reduced state of $A \cup \lbrace h \rbrace$ lies in
the span of the $\lvert a_i \rangle \lvert x \rangle$, of dimension at most
$2r$. So the bond at cut $c$ is at most twice the bond at cut $c - 1$,
itself at most the capacity there: within $2\chi$. The bound
is reached: on the random circuit of the second row of the first table
below, the outbound splits keep bonds of 256 at $\chi$ 128.

**Truncating a route's outbound splits to the capacity.** The route
`SwapBack` keeps at most $\chi$ at every split. The temporary order needs
up to $2\chi$, so it truncates states that fit $\chi$ in the caller's
order:

| Case                                              | Truncated to $\chi$ | `swap-back-2x` |
| ------------------------------------------------- | ------------------- | -------------- |
| Two pairs and a CZ, 4 qubits, $\chi$ 2 (computed) | 0.0395              | 0              |
| Random pairs, depth 2, seed 1, $\chi$ 128         | 2.1e-4              | 2.7e-15        |
| PPP polyene QPE, first round, $\chi$ 128          | 2.5e-5              | 1.4e-9         |
| QPE of 16 qubits, $\chi$ 128                      | 1.8e-3              | 9.6e-5         |
| H₂, $\chi$ 64 (TVD of the readout)                | 4.5e-4              | 1.6e-10        |

The first row is computed, not measured: it is the case of
[the crate's MPO tests](../src/tests/engine/mpo.rs): Bell-like pairs on
sites (0, 1) and (2, 3), then a CZ on sites 0 and 2, at bond 2; the
truncated route loses $\sin^2 0.2 \approx 3.9\%$ of the weight at the outbound split,
and an MPO, like `swap-back-2x`, none.

**Truncating a route's outbound splits to twice the capacity's bond at the
cut they cross.**
This equals $2\chi$ on a chain of even bonds, but truncates a state that fits
an uneven capacity: with bonds 2, 1 and 2, Bell pairs on sites (0, 1) and
(2, 3) and a gate on sites 0 and 3, the SWAP at cut 1 needs a bond of 4
(twice the bond at cut 0, below it), where this bound allows 2. The bound
of the proof above is the bond at the cut below, not the cut crossed.

**Truncating a route's outbound splits to twice the capacity's bond at the
cut below.**
Correct, but it never binds: the split's matrix has `2 · left` rows,
`left` the bond at cut $c - 1$, which no split of the route has set yet
and which the chain holds within the capacity there. The SVD's rank is at
most that, so the bound adds nothing, and `swap-back-2x`'s outbound
splits apply the cutoff alone.

## Alternatives that could be explored

None is built. Each would be a new option of the engine, explored when a
simulation method needs its speed and its cases are measured. They were
compared with `swap-back-2x`, which keeps the caller's order as an
MPO does; none compared them with an MPO.

**Routings that leave the moved site where the gate met it** save the
return, at the price of a layout that drifts from the caller's order:
`MoveHigher` moves the higher site down and keeps it there, `MoveLower`
moves the lower one up, `MeetHalfway` moves both. They were measured
against `swap-back-2x`:

| Case                                     | `swap-back-2x` | `MoveHigher` | `MoveLower` | `MeetHalfway` |
| ---------------------------------------- | -------------- | ------------ | ----------- | ------------- |
| H₂, $\chi$ 64 (TVD of the readout)       | 1.6e-10        | 2.2e-10      | 1.7e-4      | 1.1e-10       |
| Mirror circuit, depth 2, $\chi$ 64       | 0.293          | 0.016        | 4.8e-4      | 0.006         |
| Mirror circuit, depth 3, $\chi$ 64       | 0.731          | 0.203        | 0.185       | 0.30          |
| PPP polyene QPE, first round, $\chi$ 128 | 1.4e-9         | 0.388        | 0.166       | 0.205         |
| QPE of 16 qubits, $\chi$ 128             | 9.6e-5         | 0.036        | 0.0082      | 0.0077        |

They serve two kinds of case:

- **H₂, a phase ancilla that meets every system qubit:** at $\chi$ 128,
  `MoveHigher` was about five times faster per shot than a returning
  route truncated to $\chi$, `SwapBack` above, at the same readout error.
- **A circuit whose state needs more than $\chi$ in the caller's order,** such
  as the mirror circuits: a returning route loses what that order needs,
  and only a routing that changes the order can lose less.

The drift ruins the others: on the polyene, the drifted layout needs about
twice the bond of the caller's order at rest (a median of 255 against
128), and the loss comes within about five gates of drift.

**Bounded drift**, `MoveHigher` with the caller's order restored every `K`
two-site gates (`K` of 50, 200 and 1000 were tried, and also 1, 5, 10
and 20 on the polyene), was at its best `K` less accurate than
`swap-back-2x`, or as accurate (the polyene at $\chi$ 32), except on the
mirror circuits, where it was better at depth 2 and about even at depth 3,
and less accurate than `MoveHigher` there:

| Case                                     | Best `K` | Bounded drift | `swap-back-2x` | `MoveHigher` |
| ---------------------------------------- | -------- | ------------- | -------------- | ------------ |
| H₂, $\chi$ 64 (TVD of the readout)       | 1000     | 1.5e-4        | 1.6e-10        | 2.2e-10      |
| PPP polyene QPE, first round, $\chi$ 32  | 1        | 0.127         | 0.127          | 0.958        |
| PPP polyene QPE, first round, $\chi$ 64  | 1        | 0.0211        | 0.0179         | 0.985        |
| PPP polyene QPE, first round, $\chi$ 128 | 1        | 2.5e-5        | 1.4e-9         | 0.388        |
| QPE of 16 qubits, $\chi$ 128             | 50       | 0.0040        | 9.6e-5         | 0.036        |
| Mirror circuit, depth 2, $\chi$ 64       | 50       | 0.0237        | 0.293          | 0.016        |
| Mirror circuit, depth 3, $\chi$ 64       | 50       | 0.727         | 0.731          | 0.203        |

On the polyene, `K` of 1 restores after every gate, the truncated route
above.

**Not measured:** a lookahead that plans the SWAPs over the gates to come
(SABRE-like), a layout chosen by partitioning the program's interaction
graph, a cost model that picks the routing per gate, and a routing chosen
per program by probing the alternatives on its first gates.

# Routing a gate on distant sites

A gate acts on two neighbouring sites of the chain; a gate on sites further
apart must first bring them together. This page gives the routing the
engine uses, `swap-back-2x`, why it loses no weight to the move, what was
tried before it and dropped, and the alternatives that could be explored
later. The crate's [README](../README.md#routing-swap-back-2x) summarizes
it.

The figures marked _study_ come from a study of the routings on an earlier
version of this engine, on the same circuits as
[the README's evidence](../README.md#evidence): its error is the
infidelity 1 − F to the exact state, the readout's TVD for H₂, and
1 − P(0…0) for the mirror circuits, whose exact state is `|0…0⟩`.

## The approach: `swap-back-2x`

A gate on sites `low < high` that are not neighbours moves the site at
`high` down to `low + 1` by adjacent SWAPs (outbound), applies the gate to
the pair at `low`, and moves the site back up by SWAPs (return): `d − 1`
SWAPs each way for sites `d` apart. The chain is in the caller's order
after every call, so nothing the engine reports, and no later call, sees
the move.

The split of each outbound SWAP drops the tail below the cutoff and
applies no capacity. Its bond is still bounded: after the SWAP that moves
the site down across cut `c`, the sites left of cut `c` are the caller's
sites left of cut `c − 1`, a set `A`, and the moved site `h`. Write the
state in `A`'s Schmidt basis, `Σᵢ sᵢ |aᵢ⟩ |bᵢ⟩` with `r` terms, `r` the bond
the chain holds at cut `c − 1`; each `|bᵢ⟩` expands over `h`'s two levels,
so the reduced state of `A ∪ {h}` lies in the span of the `|aᵢ⟩|x⟩`, of
dimension at most `2r`. So the bond at cut `c` is at most twice the bond
at cut `c − 1`, itself at most the capacity there: within 2χ, χ the
capacity's largest bond. The bound is reached: on the random circuit of
the first row of [the README's routing table](../README.md#routing), the
outbound splits keep bonds of 256 at χ 128.

The gate's split, at cut `low`, and each return split leave the moved site
on the caller's side of the cut, so they split at the caller's cuts, where
the state must fit the capacity anyway; they keep at most the capacity's
bond. So a route discards nothing beyond what the cutoff drops at its
`2(d − 1) + 1` splits (negligible at the default 1e-14) and what the
capacity cuts at the caller's cuts, as the README's first random circuit
shows: it fits χ 128 in the caller's order and loses nothing beyond
rounding.

With fusion, a pair left unsplit is split under its last two-site
operation. A route's first outbound SWAP may land on a pair the operation
before it left unsplit; that pair is then split as outbound, and the bound
holds unchanged: the gates fused into the pair act on its two sites, right
of the cut below, and leave the bond there as it was.

**The cost.** During a route a bond reaches up to 2χ, so a site tensor up
to 4× the memory of one at χ, and a split's matrix up to 2χ × 4χ, against
2χ × 2χ for a route truncated to χ. The study measured the split time
against that truncated route (below): 1.46–1.62× where the bonds sit at
χ, the cases where the allowance gains nothing (H₂ at χ 32, the polyene
at χ 32 and 64, the mirror circuits), and 0.96–1.11× where it removes the
loss (H₂ at χ 64, the polyene at χ 128, the QPE of 16 qubits), where
bonds above χ appear only on the gates that need them.
`CanonicalMps::split_statistics` reports the largest transient bond apart
from the largest bond at rest.

## Tried and dropped

**Truncating the outbound splits to the capacity.** The study's `SwapBack`,
and this engine's first route, kept at most χ at every split. The
temporary order needs up to 2χ, so it truncated states that fit χ in the
caller's order:

| Case                                         | Truncated to χ | `swap-back-2x` |
| -------------------------------------------- | -------------- | -------------- |
| Two pairs and a CZ, 4 qubits, χ 2 (computed) | 0.0395         | 0              |
| Random pairs, depth 2, seed 1, χ 128 (study) | 2.1e-4         | 2.7e-15        |
| PPP polyene QPE, first round, χ 128 (study)  | 2.5e-5         | 1.4e-9         |
| QPE of 16 qubits, χ 128 (study)              | 1.8e-3         | 9.6e-5         |
| H₂, χ 64 (TVD of the readout) (study)        | 4.5e-4         | 1.6e-10        |

The first row is computed, not measured: it is the case of
[the crate's route tests](../src/tests/engine.rs): Bell-like pairs on
sites (0, 1) and (2, 3), then a CZ on sites 0 and 2, at bond 2; the
truncated route loses sin²0.2 ≈ 3.9% of the weight at the outbound split.

**Truncating them to twice the capacity's bond at the cut they cross.**
This equals 2χ on a chain of even bonds, but truncates a state that fits
an uneven capacity: with bonds 2, 1 and 2, Bell pairs on sites (0, 1) and
(2, 3) and a gate on sites 0 and 3, the SWAP at cut 1 needs a bond of 4
(twice the bond at cut 0, below it), where this bound allowed 2. The bound
of the proof above is the bond at the cut below, not the cut crossed.

**Truncating them to twice the capacity's bond at the cut below.**
Correct, but it never binds: the split's matrix has `2 · left` rows,
`left` the bond at cut `c − 1`, which no split of the route has set yet
and which the chain holds within the capacity there. The SVD's rank is at
most that, so the bound was dropped; the outbound splits apply the cutoff
alone.

## Alternatives that could be explored

None is built. Each would be a new option of the engine, explored when a
method needs its speed and its cases are measured.

**Routings that leave the moved site where the gate met it** save the
return, at the price of a layout that drifts from the caller's order:
`MoveHigher` moves the higher site down and keeps it there, `MoveLower`
moves the lower one up, `MeetHalfway` moves both. The study measured them
against `swap-back-2x`:

| Case (study)                        | `swap-back-2x` | `MoveHigher` | `MoveLower` | `MeetHalfway` |
| ----------------------------------- | -------------- | ------------ | ----------- | ------------- |
| H₂, χ 64 (TVD of the readout)       | 1.6e-10        | 2.2e-10      | 1.7e-4      | 1.1e-10       |
| Mirror circuit, depth 2, χ 64       | 0.293          | 0.016        | 4.8e-4      | 0.006         |
| Mirror circuit, depth 3, χ 64       | 0.731          | 0.203        | 0.185       | 0.30          |
| PPP polyene QPE, first round, χ 128 | 1.4e-9         | 0.388        | 0.166       | 0.205         |
| QPE of 16 qubits, χ 128             | 9.6e-5         | 0.036        | 0.0082      | 0.0077        |

They serve two kinds of case:

- **H₂, a phase ancilla that meets every system qubit:** at χ 128,
  `MoveHigher` took 18.4 s per shot (the mean of six) against 90.7 s for
  the returning route of the time, truncated to χ as the study's `SwapBack`
  above, at the same readout error (TVD at most 3.4e-10), in an earlier
  measurement than the table's. This engine takes 78 s per shot
  ([the README's H₂ table](../README.md#evidence), where χ does not
  bind), 4.3× `MoveHigher`'s time; the study's engine used faer's SVD,
  which takes about as long as this engine's own on these splits
  ([the README's "Linear algebra"](../README.md#linear-algebra)).
- **A circuit whose state needs more than χ in the caller's order,** such
  as the mirror circuits: a returning route loses what that order needs,
  and only a routing that changes the order can lose less.

The drift ruins the others: on the polyene, the drifted layout needs about
twice the bond of the caller's order at rest (a median of 255 against
128), and the loss comes within about five gates of drift.

**Bounded drift**, `MoveHigher` with the caller's order restored every `K`
two-site gates (the study tried `K` of 50, 200 and 1000, and also 1, 5,
10 and 20 on the polyene), was at its best `K` less accurate than
`swap-back-2x`, or as accurate (the polyene at χ 32), except on the
mirror circuits, where it was better at depth 2 and about even at depth 3,
and less accurate than `MoveHigher` there:

| Case (study)                        | Best `K` | Bounded drift | `swap-back-2x` | `MoveHigher` |
| ----------------------------------- | -------- | ------------- | -------------- | ------------ |
| H₂, χ 64 (TVD of the readout)       | 1000     | 1.5e-4        | 1.6e-10        | 2.2e-10      |
| PPP polyene QPE, first round, χ 32  | 1        | 0.127         | 0.127          | 0.958        |
| PPP polyene QPE, first round, χ 64  | 1        | 0.0211        | 0.0179         | 0.985        |
| PPP polyene QPE, first round, χ 128 | 1        | 2.5e-5        | 1.4e-9         | 0.388        |
| QPE of 16 qubits, χ 128             | 50       | 0.0040        | 9.6e-5         | 0.036        |
| Mirror circuit, depth 2, χ 64       | 50       | 0.0237        | 0.293          | 0.016        |
| Mirror circuit, depth 3, χ 64       | 50       | 0.727         | 0.731          | 0.203        |

On the polyene, `K` of 1 restores after every gate, the truncated route
above.

**Not measured:** a lookahead that plans the SWAPs over the gates to come
(SABRE-like), a layout chosen by partitioning the program's interaction
graph, a cost model that picks the routing per gate, and a routing chosen
per program by probing the alternatives on its first gates.

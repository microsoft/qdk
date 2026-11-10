# Routing a gate on distant sites

A gate acts on two neighbouring sites of the chain; a gate on sites further
apart must first bring them together. This page gives the routing the
engine uses by default, `swap-back-2x`, why it loses no weight to the move,
and what was tried before it and dropped; then the option `MeetHalfway`,
the guard that keeps it as accurate, and the evidence for it; and the
routings studied beside them. The crate's README summarizes
[the default](../README.md#routing-swap-back-2x) and
[the option](../README.md#routing-option-meethalfway).

The figures come from two studies of the routings, on the same circuits as
[the README's evidence](../README.md#evidence):

- Those marked _study_ come from a first study, on an earlier version of
  this engine: its error is the infidelity 1 − F to the exact state, the
  readout's TVD for H₂, and 1 − P(0…0) for the mirror circuits, whose exact
  state is `|0…0⟩`.
- Those marked _comparison_ come from a comparison on this engine, before
  the option took its final form (below), with one run per cell, in
  release, one core each; differences under about 10% are noise. H₂ and
  the polyene are replays of one recorded shot each, forcing its recorded
  outcomes: for H₂ (χ 128) the error is the readout's TVD against the
  exact distribution; for the polyene (17 qubits, χ 32 or 64), the weight
  lost, 1 − W, and the largest gap of P(One) over its 108 reads against an
  exact reference. For the QFT and the random circuits (20 qubits, χ 128)
  the error is the largest gap of a qubit's P(One), and for the mirror
  circuits 1 − P(0…0).

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

## The option: `MeetHalfway`

`CpuMpsEngine::with_routing(Routing::MeetHalfway)` routes a gate on sites
`low < high` that are not neighbours by moving both toward the pair at
`meet = low + ⌊(high − low − 1) / 2⌋`, `d − 1` SWAPs in all for sites `d`
apart, against `2(d − 1)` for `swap-back-2x`, and leaves them there: a
gate that follows on the same two qubits takes none. The site on the
centre's side moves first, so that the centre rides along and crosses the
span once. It is meant for programs whose gates between distant qubits
dominate the time, such as the H₂ shot of [the evidence](#the-evidence).

**The layout.** The chain's order then drifts from the caller's. The
engine keeps the map from each of the caller's sites to its chain
position, so every gate and read translates the sites it names, and
`shape` restores the caller's order before it answers. The layout drift
is the fraction of the pairs of sites the chain holds in the other order
than the caller's: 0 in the caller's order, 1 reversed.

**Why a guard.** A routing that leaves its sites moved and truncates in
its own order loses states that the caller's order keeps. The comparison
measured it on the polyene: in the drifted order, a split drops 10 to 10⁹
times the weight a split drops in the caller's order at the same point,
and the shot has about 12,000 two-site gates before its first read, so
the weight goes, then the state (1 − W of 0.94 for unguarded
`MeetHalfway` at χ 32, 1.0 for `MoveHigher`, against 0.046 for
`swap-back-2x`; [below](#the-routings-studied-and-rejected)). Every such
routing also fails two capacity cases of the contract's conformance kit.
Restoring the order before reads does not help: the loss happens between
them. So the guard keeps the cuts where `swap-back-2x` makes them:

- **The deferred cut.** While the chain is out of the caller's order, a
  split keeps up to twice the capacity's largest bond, 2χ, and records its
  cut when it keeps more than the capacity's bond there. The first
  two-qubit gate or read after such a split restores the caller's order, by
  SWAPs that also keep up to 2χ, then cuts the recorded bonds to the capacity by one sweep
  of SVDs in the caller's order. So the capacity cuts the state only at
  the caller's cuts.
- **The fallback.** That restore also ends the drift for good: a bond over
  the capacity says the capacity binds, and each later route would pay a
  restore. A gate that leaves the drift above one half restores the order
  and falls back too: the order is then nearer reversed than the caller's,
  where routes grow long. A state that fell back routes by `swap-back-2x`
  for the rest of its life, and its forks inherit it. One half is a
  constant, not a setting: one value served every case measured.

**Why twice the capacity, and its limit.** The first route from the
caller's order moves each site across cuts whose sides then differ from
the caller's by that one site, so each of its bonds is at most twice a
bond the caller's order holds, by the proof of
[the approach](#the-approach-swap-back-2x). A later route starts from a
drifted order, where that bound no longer holds: a split there that needs
more than 2χ truncates at a cut that is not the caller's. The capacity
fallback ends the drift at the first bond over the capacity, which keeps
such splits rare; how rare was not measured.

**Fusion.** With fusion, the last pair a route leaves unsplit is split by
the next operation. A read splits it before its check, so it restores the
order when that pair's bond exceeds the capacity. A gate on another pair
splits it after its own check, so the restore waits for the operation
after that gate: the capacity still cuts only at the caller's cuts, one
gate later.

**The price.** Between restores, a site tensor may hold bonds up to 2χ, as
during a `swap-back-2x` route but for longer; `split_statistics` counts
the splits out of the caller's order as transient, and the restore's SWAPs
in `swaps`.

### The evidence

The comparison measured the option, against `swap-back-2x`, on six cases
it was chosen on, then on four more (time as a multiple of
`swap-back-2x`'s; the error as defined above):

| Case (comparison)                        | `swap-back-2x`        | `MeetHalfway`         |
| ---------------------------------------- | --------------------- | --------------------- |
| H₂, χ 128                                | 88 s; 5.3e-10         | 0.20×; 1.0e-10        |
| PPP polyene QPE, χ 32                    | 90 s; 0.046 / 0.058   | 1.2×; 0.046 / 0.057   |
| PPP polyene QPE, χ 64                    | 362 s; 7.6e-3 / 0.014 | 0.89×; 7.5e-3 / 0.013 |
| QFT, 20 qubits, χ 128                    | 0.052 s; 1.9e-10      | 0.88×; 1.2e-10        |
| Random pairs, depth 2, χ 128             | 0.23 s; 1.7e-14       | 0.12×; 1.6e-14        |
| Mirror circuit, 32 qubits, depth 2, χ 64 | 8.2 s; 0.29           | 1.2×; 0.29            |
| QPE of 14 qubits, depth 6, χ 128         | 10.0 s; 5.4e-9        | 10.0 s; 5.5e-9        |
| QPE of 16 qubits, depth 5, χ 128         | 47.5 s; 4.4e-5        | 48.0 s; 4.4e-5        |
| Random pairs, depth 4, χ 64              | 1.72 s; 0.12          | 1.54 s; 0.12          |
| Mirror circuit, 24 qubits, depth 3, χ 32 | 1.19 s; 0.73          | 1.38 s; 0.73          |

The last four were not used to choose. It matches `swap-back-2x`'s accuracy
in all ten, and its time is within 1.2× everywhere, a fifth on H₂, whose
drift stays at most 0.13 and whose bonds never reach the capacity. On the
QFT, whose drift grows toward the reverse, the drift fallback took it from
2.8× to 0.88×. The built option, measured again with
[`examples/routing.rs`](../examples/routing.rs), reproduces the five cases
it can run ([the README's routing table](../README.md#routing)).

It differs from the version the comparison measured in one detail: there,
the restore that `shape` makes compressed a bond over the capacity
without falling back; here every restore that finds one falls back, so
that the rule has one place. Neither H₂, whose bonds stay below the
capacity, nor the polyene, which falls back at its first full bond, can
tell them apart.

**Why an option, not the default.** It is a hypothesis being evaluated:
the threshold of one half and the route were chosen on six cases and
checked on four. Before it could become the default it needs the same
comparison on larger programs (a polyene of 12 sites, a QPE of 20 qubits,
other compilers' programs) and on live shots rather than replays.

## The routings studied and rejected

Each would be a new option of the engine, and each was rejected on the
evidence below.

**Routings that leave the moved site where the gate met it** save the
return, at the price of a layout that drifts from the caller's order:
`MoveHigher` moves the higher site down and keeps it there, `MoveLower`
moves the lower one up, `MeetHalfway` moves both. The first study
measured them unguarded against `swap-back-2x`:

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

**Guards.** The comparison then tried rules that change when the order
goes back to the caller's, or where the engine truncates:

- restoring before every read (P(One), a conditional, a projection, a
  sample, an expectation, the weight, a fork);
- restoring after a gate that leaves the drift above 0.23;
- restoring after a gate once a split at rest since the last check was
  bound by the capacity;
- the deferred cut of the option, alone, and with the capacity fallback
  alone (no drift fallback);
- a hub: a lazy `swap-back-2x` with at most one site out of place, which
  stays beside its partner while the next gates involve it, and whose
  splits at a cut that site crossed keep up to 2χ.

Time as a multiple of `swap-back-2x`'s, and the error; in bold, the
errors that multiply `swap-back-2x`'s, the state lost. "Kit": the
contract's conformance kit, with fusion and without.

| Routing (comparison)                       | H₂ χ 128         | Polyene χ 32           | Polyene χ 64           | QFT              | Random          | Mirror         | Kit    |
| ------------------------------------------ | ---------------- | ---------------------- | ---------------------- | ---------------- | --------------- | -------------- | ------ |
| `swap-back-2x`                             | 88 s; 5.3e-10    | 90 s; 0.046 / 0.058    | 362 s; 7.6e-3 / 0.014  | 0.052 s; 1.9e-10 | 0.23 s; 1.7e-14 | 8.2 s; 0.29    | passes |
| `MoveHigher`                               | 0.29×; 3.0e-10   | 0.48×; **1.0 / 1.0**   | 0.86×; **1.0 / 0.48**  | 8.1×; 1.3e-9     | 0.017×; 1.6e-14 | 0.066×; 0.016  | 2 fail |
| `MoveHigher`, restored before reads        | 0.29×; 3.0e-10   | 0.47×; **1.0 / 1.0**   | 0.83×; **1.0 / 0.48**  | 10×              | 0.34×           | 0.19×; 0.016   | 2 fail |
| `MoveHigher`, restored past drift 0.23     | 0.53×; 2.8e-10   | 0.77×; **0.32 / 0.35** | 1.1×; **0.22 / 0.13**  | 1.8×             | 0.083×          | 0.67×; 0.39    | 2 fail |
| `MoveHigher`, restored once capped         | 0.30×; 3.0e-10   | 0.91×; 0.11 / 0.054    | 1.2×; 0.016 / 0.015    | 8.2×             | 0.013×          | 0.78×; 0.31    | 2 fail |
| `MoveHigher`, deferred cut                 | 0.34×; 3.0e-10   | 1.7×; 0.045 / 0.057    | 1.3×; 7.6e-3 / 0.014   | 7.8×             | 0.014×          | 3.7×; 0.29     | passes |
| `MoveHigher`, deferred, capacity fallback  | 0.33×; 3.0e-10   | 1.2×; 0.046 / 0.061    | 0.96×; 7.6e-3 / 0.015  | 7.8×             | 0.014×          | 1.6×; 0.29     | passes |
| `MoveLower`                                | 9.4×; **2.1e-2** | 0.73×; **1.0 / 0.99**  | 0.94×; **0.99 / 0.50** | 2.3×             | 0.013×          | 0.010×; 4.8e-4 | 2 fail |
| `MeetHalfway`, unguarded                   | 0.18×; 1.0e-10   | 0.72×; **0.94 / 0.49** | 0.94×; **0.20 / 0.14** | 2.8×             | 0.11×           | 0.10×; 6.2e-3  | 2 fail |
| `MeetHalfway`, deferred cut                | 0.18×; 1.0e-10   | 1.3×; 0.045 / 0.056    | 1.3×; 7.6e-3 / 0.014   | 3.3×             | 0.13×           | 3.5×; 0.28     | passes |
| `MeetHalfway`, deferred, capacity fallback | 0.21×; 1.0e-10   | 1.2×; 0.046 / 0.057    | 0.96×; 7.5e-3 / 0.013  | 2.8×             | 0.11×           | 1.2×; 0.29     | passes |
| Hub                                        | 0.70×; 2.5e-9    | 1.1×; 0.046 / 0.057    | 0.93×; 7.5e-3 / 0.015  | 1.2×             | 1.1×            | 0.96×; 0.29    | passes |
| **The option**: both fallbacks             | 0.20×; 1.0e-10   | 1.2×; 0.046 / 0.057    | 0.89×; 7.5e-3 / 0.013  | 0.88×; 1.2e-10   | 0.12×; 1.6e-14  | 1.2×; 0.29     | passes |

The QFT and random errors not shown are at rounding (at most 1.6e-9 and
1.7e-14). So:

- the rules that restore but still truncate in a drifted order (before
  reads, past a drift of 0.23, once capped) lose the polyene's state or
  multiply its error, and fail the kit;
- the deferred cut alone is as accurate as `swap-back-2x` but pays a
  restore at each over-capacity split once the capacity binds (11,669
  restores for `MoveHigher` in the polyene shot at χ 32), and 3.3× to 7.8×
  on the QFT;
- the capacity fallback removes that cost where the capacity binds, but
  not on the QFT, whose drift grows toward the reverse and whose routes
  grow long with it (2.8×); the drift fallback removes it there (0.88×);
- the hub is safe but saves less on H₂ (0.70×);
- `MoveHigher` with the deferred cut and the capacity fallback is as
  accurate, but 7.8× on the QFT (with the drift fallback it was not
  measured) and slower than `MeetHalfway` on H₂ (0.33× against 0.21×).

Where the caller's order itself needs more than χ (the mirror circuits,
random circuits at depth 4), a drifted order is more accurate than the
caller's: unguarded `MoveHigher` was 3.5× and 3.9× more accurate on the
two validation cases. Every routing that cuts only at the caller's cuts,
`swap-back-2x` and the option alike, gives that up; choosing the order is
the calling method's.

**Choosing per program by a probe.** Running each routing on the first
10% of a program's two-site gates and keeping the fastest of those whose
weight is within 2× of the best's chose a routing within 2.3× of the
fastest, never multiplying the error, on the six cases; but it cost 30%
of a polyene shot, decided by timer noise on the circuits whose probe
took milliseconds, and sees only the first 10%, so a loss that starts
later escapes it.

**Not measured:** a lookahead that plans the SWAPs over the gates to come
(SABRE-like), a layout chosen by partitioning the program's interaction
graph, and a cost model that picks the routing per gate.

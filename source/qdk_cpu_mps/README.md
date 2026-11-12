# A matrix product state engine on the CPU

`qdk_cpu_mps` is a matrix product state (MPS) engine on the CPU:
`CpuMpsEngine`, which implements the `tensornet` crate's `MpsEngine`
contract.

## The problem

A simulator that runs a program as a matrix product state holds the state as
a chain of site tensors, and spends its time in their linear algebra: a QR
decomposition to move along the chain, an SVD to split two sites after a
gate and truncate the bond between them. `tensornet` states what such an
engine does, in tensor terms, and checks any engine against an exact one;
GPU engines implement it on their own libraries. This crate is the engine
on the CPU: it runs on every target the QDK builds for, WebAssembly
included, and is a second engine, beside a GPU's, that truncates, so the
contract is checked on an engine that loses weight as real runs do.

The crate stands alone, so that it can be accepted or removed on its own:
the linear algebra library it uses, faer, appears only in its `Cargo.toml`,
and nothing else in the workspace depends on it (see
[Removing the crate](#removing-the-crate)).

## The approach

The crate is one engine behind the contract, `tensornet::MpsEngine`: a
simulation method written against the contract runs on the CPU unchanged,
and how the engine computes stays inside it. The engine is written once,
over a crate-private linear algebra trait, so another library would be a
new implementation of that trait alone (see
[Linear algebra](#linear-algebra)). The engine refuses a call whose
arguments break the contract by the contract's own checks, and its tests
run the contract's conformance kit, which compares it with the exact
reference, once with fusion and once without. A gate on sites far apart is
routed by SWAPs there and back, whose outbound splits apply the cutoff
alone, so a route discards, beyond the cutoff's share at each split, only
what the capacity cuts at the caller's cuts (see
[Routing](#routing-swap-back-2x)). The SVD is the crate's own, a one-sided
Jacobi after a pivoted QR that trims the rank, because faer's SVDs fail on
the rank-deficient, degenerate matrices that splits meet; and a split takes
an SVD only when its factors give back the matrix and its singular vectors
are orthonormal, so a bad factorization fails the call instead of spoiling
the state.

The engine agrees with the QDK's sparse state simulator and, on Clifford
circuits, its stabilizer simulator, and passes the contract's kit with and
without fusion. On six shots of H₂'s phase estimation (29 qubits, 1,399
mid-circuit measurements) at χ 64, its readout distribution is within a
total variation distance of 3.1e-10 of each shot's exact one, and the first
shot ran 1.5 times as fast as on the sparse simulator; see
[Evidence](#evidence).

## The model

### A canonical chain

The state, `CanonicalMps`, is a chain of site tensors `[left, 2, right]` in
mixed canonical form: one site, the orthogonality centre, holds the state's
norm; the sites to its left are left-orthonormal and those to its right
right-orthonormal. Every operation keeps that form and moves the centre only
as far as it must, so no gate or read sweeps the whole chain (a conditional
read with conditions and each new nonempty prefix of a sample copy it, a
copy, not a contraction):

- **A one-site gate** multiplies the site's physical index; the form holds
  wherever the centre is.
- **A gate on two neighbouring sites** moves the centre into the pair (one QR
  per site crossed), contracts the pair, applies the gate, and splits it
  again by one SVD, truncated; the centre ends on one of the two sites.
- **A gate on two sites further apart** is routed (see below).
- **P(One) and a projection** read the site alone, after moving the centre
  there; **the retained weight** reads the weight the state tracks, after
  splitting a pending pair, without moving the centre. The
  reads take the state by shared reference: moving the centre changes the
  tensors, not the state.
- **A projection does not recompress the chain:** it zeroes the other value
  at the centre and divides the site by the norm of what it kept, returns
  the probability it computed, and drops no singular value of its own. So
  `shape` reports the bonds the engine holds, which after a projection may
  exceed the state's Schmidt ranks (see
  [Recompressing after a measurement](#recompressing-after-a-measurement)
  for why). Moving the centre to the site, by a QR per site crossed, bounds
  each bond it crosses by twice the bond on the crossed site's other side.
- **An expectation** contracts only the sites between the observable's sites
  and the centre; the rest of the chain is orthonormal and contracts to the
  identity.
- **Sampling** draws each shot site by site, each value from its
  probability given the values drawn before it, from one stream seeded by
  the caller's seed.

The state is `Send`: a shot may run on another thread than the one that
prepared its state.

### Truncation

A split at rest, after a gate or a route's return SWAP, keeps at most the
capacity's bond at its cut (at most χ, the max bond dimension of the
`MpsShape` given to `prepare`); a
route's outbound SWAPs apply the cutoff alone, and keep up to twice the
bond at the cut below (see [Routing](#routing-swap-back-2x)). Every split
drops the smallest singular values whose squares sum to at most `cutoff`
times the sum of all squares (quimb's
`cutoff_mode="rsum2"`). `DEFAULT_CUTOFF` is 1e-14, as in quimb and
tensor4all; `CpuMpsEngine::with_cutoff` sets another, a fraction in
[0, 1). A split always keeps its largest singular value. The engine
renormalizes at each split that discards a value and tracks the retained
weight apart: the split scales the values it keeps so that they hold the
squared norm of the whole block, and multiplies the state's retained
weight, the product of the fractions the truncations kept, by the
fraction they held; `retained_weight` reads it. The fraction and the
cutoff's tail are computed on the values divided by the largest, so their
sums of squares do not underflow (a defence: the block a split factors
has a norm of about 1), and a projection divides the site by the norm of
what it kept, computed the same way, so the state's norm stays about 1
however small the weight or the probability. A gate keeps the norm up to rounding and
the contract's unitarity tolerance
(`tensornet::checks::UNITARITY_TOLERANCE`); the reads divide by the norm
at the centre, so that drift does not reach them. A split that would take
the retained weight below the smallest normal `f64` fails with
`CpuMpsError::WeightUnderflow`, since callers divide by the weight. χ has
no default here: the caller sets it in the capacity it prepares.

The cutoff and χ trade time for accuracy:

- the cutoff bounds the weight a split may discard, so it keeps each bond as
  small as the state allows;
- χ bounds the bond at rest, whatever weight that discards, and with it
  memory and time, up to a route's transient bonds of 2χ.

The weight the splits discard adds up over a run: `N` splits discard up to
about `N · cutoff`, plus what χ cuts. On a chemistry circuit of tens of
qubits the cutoff, not χ, keeps the bonds small. A cutoff of zero keeps
every value the SVD returns, those just above rounding included, which
grows the bonds and the time: on the Ising quench of 16 qubits at depth 20
and χ 256, the largest bond at rest measured 254 against 138 at the
default cutoff, and the run about 2.4 times as long (1.13 s against
0.47 s):

```sh
cargo run --release -p qdk_cpu_mps --example sparse_comparison -- \
    --family ising --n 16 --depth 20 --cutoff 0 --only mps
```

### Routing: `swap-back-2x`

A gate on sites `low < high` that are not neighbours moves the site at
`high` down to `low + 1` by adjacent SWAPs (outbound), applies the gate to
the pair, and moves the site back up by SWAPs (return). The centre rides
along, so once it reaches the first SWAP the route takes no QR. The chain
is in the caller's order after every call: nothing the engine reports, and
no later call, sees the move.

The split of each outbound SWAP applies the cutoff alone, no capacity. Its
bond is still at most **twice the bond at cut `c − 1`**, for the SWAP that
moves the site down across cut `c` (2χ when every cut's bond is χ): after
that SWAP, the sites left of cut `c` are the caller's sites left of cut
`c − 1` and the moved site, so the Schmidt rank at cut `c` is at most twice
the bond the chain holds at cut `c − 1`, itself at most the capacity there.
The gate's and the return's splits keep at most the capacity's bond at
their own cut, the caller's cuts, where the state must fit the capacity
anyway. So the route discards nothing beyond what the cutoff discards at
its `2(d − 1) + 1` splits, for sites `d` apart (negligible at the default
1e-14), and what the capacity cuts at the caller's cuts; a route truncated
to χ throughout would also lose weight to the temporary order. The bound is
reached: on the random circuit of the first row of
[the routing table](#routing), whose state fits χ 128 in the caller's
order, the outbound splits keep bonds of 256, and the run loses nothing
beyond rounding (1 − W of 0).

The price is transient: during a route a bond reaches up to 2χ, χ the
capacity's largest bond, so a site tensor up to 4× the memory of one at χ,
and a split's matrix up to 2χ × 4χ.
`CanonicalMps::split_statistics` reports the counts of splits and of
SWAPs, the largest bond a split at rest kept, and apart from it the
largest transient bond, that of an outbound split (with fusion, a pair
left unsplit counts under its last two-site operation, so a pending pair
an outbound SWAP lands on counts as transient).

`swap-back-2x` is the only routing built.
[`docs/routing.md`](docs/routing.md) gives the proof of the bound, what was
tried before it and dropped (truncating the outbound splits to χ lost weight
to the temporary order), and the routings that could be explored later,
with what is known of each.

### Fusion

The engine fuses gates into a two-site block before its split. A gate on
two neighbours leaves their contracted pair unsplit: the gates that follow
on the same two sites, one-site gates on either and routing SWAPs
included, apply to the pair, and the first other operation (a two-site
gate on another pair, or any read or fork) splits it, by one SVD for all
of them. The engine fuses only the gates that reach a pair before another
operation splits it, the caller's gates and its routing's SWAPs alike;
reordering a program's gates to fuse more is the calling method's. Fusion
moves where truncations fall, so a truncated run's values may differ from
an unfused run's (one split per gate) within the truncation error; without
truncation they agree to rounding. Fusion is always on; the crate's own
tests also run the engine unfused, and pin that agreement.

### Linear algebra

The engine is written once over a crate-private trait, `LinearAlgebra`: a
product, a thin QR and a thin SVD of complex fp64 matrices in column-major
order; everything else (reshaping, applying gates, choosing the bond) is
the engine's. The public `CpuMpsEngine` is that engine on
`CpuLinearAlgebra`, the one implementation built, so another library is
added by implementing the trait alone, without changing the engine or the
public API. The trait is internal so that its matrices and factorizations
are not API to keep; its contract is stated in its rustdoc (built with
`--document-private-items`). The crate's tests of the linear algebra are
generic over the trait; its engine tests run on `CpuLinearAlgebra`, apart
from those that count the engine's calls or make its SVD fail, and would be
made generic with a second one. Of the module, only `FactorizationError` is
public, at the crate's root, as the payload of
`CpuMpsError::Factorization`.

`CpuLinearAlgebra` implements it on faer 0.24: pure Rust with SIMD kernels,
built without rayon, so a state computes on the calling thread and the
crate builds for `wasm32-unknown-unknown`. Its product and its QR are
faer's; its SVD is the crate's own
([`src/linear_algebra/cpu/jacobi.rs`](src/linear_algebra/cpu/jacobi.rs)),
because faer 0.24.4's two SVDs fail on matrices that splits meet, which
are rank-deficient most of the time (74% of an H₂ shot's) and have
degenerate singular values:

- Its QR algorithm reports, on a few matrices, that it does not converge:
  it neglects a superdiagonal entry `e` only when `e² ≤ ε²·|d_i·d_(i+1)|`,
  with no safety factor (LAPACK's test allows about 90ε), so a degenerate
  pair one ulp apart, coupled by `ε`, never deflates. The 3 × 3 upper
  bidiagonal `[[1, 0, 0], [0, d, ε], [0, 0, d + ulp]]`, with
  `d = 0.9282350173388935`, is one.
- Its divide and conquer (faer's default from size 128) returns, on
  rank-deficient matrices, a spurious singular value and factors whose
  product is not the matrix, without reporting a failure: 2.4e-4 where a
  128 × 128 matrix of rank 64 has 0. Its secular equation solver starts by
  the secant even when the function has the same sign at both ends of its
  bracket, where Eigen, from which it is ported, bisects.

The SVD is a one-sided Jacobi (Hestenes) after a column-pivoted QR that
trims the rank:

- A wide matrix is factored through its adjoint, so that the rotated
  columns have length `min(m, n)`. A matrix whose largest entry is outside
  `[2⁻¹⁰⁰, 2¹⁰⁰]` is first scaled by a power of two, so the SVD answers at
  any scale, down to subnormal entries, short of those whose singular
  values are too small to store to the checks' bound below, or too large
  to store at all (above `f64::MAX`). The scaling is
  exact, except on an entry it takes below the normal range (more than
  about 2¹⁰²² times smaller than the largest), which it rounds, by far
  less than the checks allow.
- The pivoted QR `A·P = Q·R` drops the rows of `R` of smallest norm, as
  many as keep the dropped rows' Frobenius norm within `max(m, n) · ε`
  times the largest row's: it changes the matrix by no more than rounding
  does, and leaves the rotations only the rows above rounding. It chooses
  the rows by their norms, not as a prefix, because faer's pivoting does
  not keep `|R_ii|` nonincreasing: on a split of an H₂ shot, a pivot of
  2e-16 came before one of 1e-9, and a prefix would have dropped the
  singular value 9.4e-10.
- Plane rotations then orthogonalize the columns of the kept rows'
  adjoint: a sweep rotates each pair whose inner product exceeds
  `√n · ε` times their norms' product, `n` their length (LAPACK
  `zgesvj`'s test), and the rotations end after the first sweep that
  rotates no pair. A pair found below the threshold is not tested again
  until one of its columns turns, and the columns' norms are updated from
  each rotation's angle, recomputed when an update halves one, and all
  recomputed once at the end. The singular values are the columns' norms.
  The rotations fail when they still turn after 30 sweeps (`zgesvj`'s
  cap).
- The products and rotations are loops on `f64`, over columns stored as
  real parts then imaginary parts, which the compiler vectorizes. They do
  not use fused multiply-add, which some targets lack, so they round alike
  on every target; faer's kernels (the pivoted QR, the engine's products)
  may use it where the target has it.
- It returns `r` values, `1 ≤ r ≤ min(m, n)`: it leaves out the values of
  the dropped rows, rounding's, among them the rows that are exactly zero,
  whose values the truncation would drop anyway; the zero matrix gives one
  value, 0.

Its cost depends on the rank. On ten splits of an H₂ shot at χ 64, mostly
rank-deficient, a split takes 0.5–1.3 times as long as faer's QR algorithm
(about 0.8 in the median, the checks included), and the shot about as long
as with faer's SVD. On splits of full rank the rotations take more sweeps:
the shot at χ 32, whose splits are truncated, takes 1.2 to 1.3 times as
long, and runs whose splits are large and of full rank about 2 to 4.3
times as long (the deep brickwork, the QPE, the mirror circuits and the polyene of
[Evidence](#evidence)).

`CpuLinearAlgebra` takes the SVD only if its factors give back the matrix,
`‖A − U·diag(S)·V†‖` at most `50 · max(m, n) · ε · ‖A‖`, and its singular
vectors are orthonormal, `‖U†U − I‖` and `‖V†V − I‖` at most
`50 · max(m, n) · ε`, all in the Frobenius norm: the bounds of LAPACK's SVD
tests. Otherwise, and when the rotations fail, the split fails with a
`FactorizationError`. So a split never spreads values that are not finite
through the state, nor factors whose product is not the matrix, nor
singular vectors that are not orthonormal. The reconstruction check is
computed on the matrix divided by its largest entry, so it holds at any
scale, where `‖A‖²` would underflow to 0 or overflow.

### Refusals and failures

A call whose arguments break the contract (a site out of range, a matrix
that is not unitary, and the others of `tensornet::Refusal`) is refused
before the state changes, by the contract's own checks, with
`CpuMpsError::Refused`; the conformance kit reads the refusal through
`tensornet::MpsEngineError`. So is a projection or a condition on a value
of probability exactly zero (`Refusal::ZeroProbabilityOutcome`), except
that this refusal is made after the engine applies a pending pair, as any
read does: the state every read sees, fused or not, is unchanged by it. The
engine reads each value's probability from that value's own weight at the
centre, over the sum of the two values' weights, not as one minus the
other's, so a rare value whose complement rounds to one is not refused, and
when one value's weight is exactly zero the other's probability is exactly
one, so a sample never draws the impossible value. A value of rounding
weight is not refused either: as `Refusal::ZeroProbabilityOutcome` says, a
method that must refuse it does so by its own threshold. The engine fails,
by its other variants, on what it does not do or cannot compute: an
operator on more than two sites (the method decomposes larger ones), a
factorization the linear algebra could not compute, a split that would
take the retained weight below the smallest normal `f64`
(`CpuMpsError::WeightUnderflow`), and a sample whose
values (shots times sites) are more than memory holds
(`CpuMpsError::TooManyValues`, before it draws; a sample of no values
returns at once, however many shots). After
such a failure the state is unspecified, as `tensornet::MpsEngine` says,
and the caller must drop it: the failed split may have lost the gates fused
into its pair, a read splits a pending pair too, and a route that fails
midway leaves its site moved.

## Example

A GHZ state on three sites, one gate on sites further apart, read
(the crate's doctest in [`src/lib.rs`](src/lib.rs)):

```rust
use num_complex::Complex64;
use qdk_cpu_mps::CpuMpsEngine;
use tensornet::{MpsEngine, MpsShape};

let c = |re: f64| Complex64::new(re, 0.0);
let h = std::f64::consts::FRAC_1_SQRT_2;
let hadamard = [c(h), c(h), c(h), c(-h)];
let mut cx = [c(0.0); 16];
for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
    cx[4 * row + col] = c(1.0);
}

let engine = CpuMpsEngine::default();
let capacity = MpsShape::qubit_chain(3, 2).expect("three sites");
let mut ghz = engine.prepare(&capacity).expect("a chain of qubits");
engine.apply(&mut ghz, &[0], &hadamard).expect("a one-site gate");
engine.apply(&mut ghz, &[0, 1], &cx).expect("neighbours");
engine.apply(&mut ghz, &[0, 2], &cx).expect("sites further apart");

let p = engine.probability_of_one(&ghz, 2).expect("a site in range");
assert!((p - 0.5).abs() < 1e-12);
let given = engine
    .conditional_probability_of_one(&ghz, 2, &[(0, true)])
    .expect("a possible condition");
assert!((given - 1.0).abs() < 1e-12);
```

## Evidence

Measured with the crate's examples (release build, one core, faer without
rayon). Each example states its inputs and prints the tables below. The
runs shared the machine with other jobs (a load of 2–7 on 12 cores), so
times vary by up to about 20% between runs of the same case.

### H₂ phase estimation

[`examples/h2_shots.rs`](examples/h2_shots.rs) replays recorded shots of
H₂'s iterative phase estimation (29 qubits, about 392,000 gates of which
89,000 on two qubits, and 1,399 mid-circuit measurements), projecting each
measurement on the outcome the shot drew, then compares the five readout
qubits' distribution with the shot's exact one. χ 64, the default cutoff,
a qubit order chosen by local search:

| Shots      | Seconds per shot      | TVD of the readout | Largest gap of a measurement's probability | Largest bond at rest / transient |
| ---------- | --------------------- | ------------------ | ------------------------------------------ | -------------------------------- |
| 6, fused   | 76.0–81.2 (mean 78.3) | 1.6e-10 – 3.1e-10  | 8.9e-11                                    | 48 / 80                          |
| 1, unfused | 80.5                  | 1.1e-10            | 9.1e-11                                    | 48 / 80                          |

The cutoff, not χ, sets the bonds: they stay at most 48 at rest, where
χ 64 allows 64, and at most 80 during routes, where it allows 128. Fusion
saved no measurable time on the first shot in these runs (81.2 s against
80.5 s unfused, within their variation; 6% with faer's SVD). The unfused
run used a flag the example no longer has, since fusion is always on.
The QDK's sparse state simulator takes 117 s on the same shot
([`examples/sparse_comparison.rs`](examples/sparse_comparison.rs)
`--replay`), against 77 s for the engine in that run: 1.5× faster. Most
of the engine's time goes to routes: a gate on sites `d` apart takes
2(d − 1) SWAPs; see [`docs/routing.md`](docs/routing.md).

The first shot at other values of χ:

| χ   | Seconds                | TVD of the readout | Largest gap of a measurement's probability | 1 − retained weight | Largest bond at rest / transient |
| --- | ---------------------- | ------------------ | ------------------------------------------ | ------------------- | -------------------------------- |
| 32  | 150.5–160.6 (two runs) | 3.9e-2             | 9.9e-4                                     | 0.32                | 32 / 64                          |
| 64  | 81.2–94.1 (two runs)   | 1.6e-10            | 8.9e-11                                    | 0 (see below)       | 48 / 80                          |
| 128 | 75.5                   | 1.6e-10            | 8.9e-11                                    | 0 (see below)       | 48 / 80                          |

This table was measured before the engine renormalized at each
truncation, when the retained weight was the state's squared norm: at
χ 64 and 128 rounding put it 2.2e-10 above 1, a value the tracked
weight, a product of fractions each at most 1, no longer takes. These
runs were not repeated since; the χ 32 row, a truncated run, may differ
by rounding.

χ 128 runs as χ 64 does, with the same values: the cutoff, not χ, sets
the bonds. χ 32 cuts the bonds the shot needs (its largest bond at rest
reaches 32): it loses a third of the weight, the readout is off by 3.9e-2,
and the run takes about twice as long: its splits are of full rank, where
the SVD costs the most (see [Linear algebra](#linear-algebra)). Truncated
runs are sensitive to rounding: with faer's SVD, before the engine's own,
χ 32 gave a TVD of 3.6e-2 and a gap of 9.5e-4.

### Recompressing after a measurement

A projection zeroes singular values at the cuts around the measured site;
recompressing (SVDs from the site outward, while the bonds shrink) drops
them, so the chain holds smaller bonds afterwards. Measured with faer's
SVD, before the engine's own, with a variant of the engine that
recompressed after every projection, on two H₂ shots and on the polyene's
whole phase estimation (17 qubits, 77,082 gates, 108 measurements and
resets, replayed from a run of the program on an exact state vector); so
compare its times with each other, not with the tables above:

| Case      | χ   | Seconds, kept | Seconds, recompressed | Mean sum of the bonds at a measurement, kept / recompressed |
| --------- | --- | ------------- | --------------------- | ----------------------------------------------------------- |
| H₂ shot 0 | 64  | 88.0          | 89.0                  | 271 / 253                                                   |
| H₂ shot 1 | 64  | 86.4          | 89.6                  | 271 / 253                                                   |
| Polyene   | 32  | 131.6         | 150.5                 | 287 / 86                                                    |
| Polyene   | 64  | 501.7         | 524.2                 | 415 / 104                                                   |

Recompressing was 1–14% slower in every case, at the same accuracy (the
readout TVDs within 2e-11 of each other, the same measurement gaps): the
smaller bonds did not repay the SVDs, presumably because the gates that
follow regrow them.
The engine does not recompress after a projection.

### Against the sparse state simulator

[`examples/sparse_comparison.rs`](examples/sparse_comparison.rs) runs a
circuit on the engine and on the QDK's sparse state simulator, and compares
P(One) of every qubit, Z-string expectations and sampled distributions.
The engine runs at χ 2^(n/2), rounded down, which no state of `n` qubits
exceeds, except the polyene, at χ 128 below that: it keeps 1 − retained
weight within 5.1e-10 (measured before the engine renormalized at each
truncation), with transient bonds up to 240 during routes.

| Circuit                                            | Qubits | Sparse s | Engine s | Sparse / engine | Largest bond at rest / transient | Largest ΔP(One) |
| -------------------------------------------------- | ------ | -------- | -------- | --------------- | -------------------------------- | --------------- |
| QFT on a product state                             | 20     | 2.49     | 0.044    | 57              | 12 / 13                          | 1.9e-10         |
| Ising Trotter steps, depth 20                      | 20     | 65.7     | 1.23     | 54              | 140 / 1                          | 1.7e-8          |
| U3 + CX brickwork, depth 20                        | 20     | 55.1     | 24.6     | 2.2             | 821 / 1                          | 5.1e-10         |
| U3 + CX brickwork, depth 60                        | 20     | 186      | 632      | 0.29            | 1024 / 1                         | 4.5e-10         |
| PPP polyene QPE, first round (39,066 gates), χ 128 | 17     | 27.1     | 1,414    | 0.02            | 128 / 240                        | 1.4e-8          |

A transient bond of 1 means no route: the Ising and brickwork circuits act
on neighbours only.

The Ising difference is the cutoff's, not χ's. Measured with faer's SVD,
before the current one replaced it, with `--reference dense` (a state
vector computed from the gates' matrices): with `--cutoff 0` the engine was
within 6e-14 of the exact state at 20 qubits, and the difference grew
roughly as the square root of the cutoff: 6e-10 at 1e-16, 1.7e-8 at the
default 1e-14, 2e-5 at 1e-8, the same at 12, 14 and 20 qubits. With the
current SVD, the table's 1.7e-8 at the default cutoff, 20 qubits, is the
same. The sparse simulator is not exact either: on this circuit at 20
qubits it is 6e-11 from that state vector, as it drops amplitudes of 1e-10
or less.

The engine wins where the entanglement stays low (structured circuits),
and by about two on the shallow brickwork; it loses where a circuit fills
the bond (deep brickwork) or long gates cross a large bond many times (the
polyene, whose rotations act on qubits up to 16 sites apart). Its SVD
costs most on these full-rank splits: with faer's SVD the engine took
7.3 s on the shallow brickwork and 188 s on the deep one (see
[Linear algebra](#linear-algebra)). Memory: the sparse simulator peaks at
166 MB at 20 qubits and 592 MB at 22 on the depth-20 brickwork, the engine
at 195 MB and 222 MB (measured before the SVD's optimization).

### Routing

[`examples/routing.rs`](examples/routing.rs) runs circuits whose gates act
on sites far apart, at χ 128, and compares with an exact state vector
(`--reference`):

| Circuit                            | Qubits | Seconds  | 1 − retained weight | Largest bond at rest / transient | Splits / SWAPs    | Largest ΔP(One)   |
| ---------------------------------- | ------ | -------- | ------------------- | -------------------------------- | ----------------- | ----------------- |
| random pairs, depth 2, seed 1      | 20     | 0.21     | 0                   | 128 / 256                        | 244 / 224         | 1.7e-14           |
| random pairs, depth 2, seeds 2–5   | 20     | 0.53–3.2 | 3.7e-4 – 1.2e-2     | 128 / 256                        | 228–268 / 208–248 | 9.2e-5 – 4.9e-3   |
| QFT                                | 20     | 0.086    | 2.4e-12             | 12 / 13                          | 2469 / 2280       | 1.9e-10           |
| QPE, 16 qubits, depth 5            | 16     | 148      | 1.1e-4              | 128 / 236                        | 8592 / 7932       | 4.4e-5            |
| mirror circuit, depth 2, χ 64      | 32     | 12.8     | 0.42                | 64 / 128                         | 1287 / 1224       | 1 − P(0…0) = 0.29 |
| mirror circuit, depth 3, χ 64      | 32     | 24.7     | 0.91                | 64 / 128                         | 1935 / 1840       | 1 − P(0…0) = 0.73 |
| PPP polyene QPE, first round, χ 32 | 17     | 151      | 3.5e-2              | 32 / 64                          | 176,327 / 163,456 | 1.6e-2            |
| PPP polyene QPE, first round, χ 64 | 17     | 878      | 7.6e-3              | 64 / 128                         | 176,327 / 163,456 | 3.4e-5            |

On the first random circuit the state fits χ 128 in the caller's order,
the outbound splits keep bonds of 256, and the run loses nothing beyond
rounding. The mirror circuits need more than χ 64 in the caller's order,
so they lose weight whatever the route does. On the polyene, χ 128 keeps
1 − retained weight within 5.1e-10 (1,414 s, in
[the table above](#against-the-sparse-state-simulator)), with transient
bonds up to 240 during routes; χ 64 keeps P(One) within 3.4e-5 in 62% of
that time, χ 32 is off by 1.6e-2 in P(One). The random and QFT rows were
measured on the current engine; the QPE, mirror and polyene rows, and the
polyene's 5.1e-10, before the engine renormalized at each truncation (the
weight was then the state's squared norm, which differs from the tracked
weight by rounding: 1.1e-12 against 2.4e-12 on the QFT), and the
polyene's splits before the examples read once before stopping their
clock, which can add the final split.

## Removing the crate

The crate is one directory, `source/qdk_cpu_mps`, and one line of the
workspace's `members` in the root `Cargo.toml`; faer and the crates it
brings appear only through this crate's `Cargo.toml`. Removing the
directory and the line, then letting Cargo update `Cargo.lock`, removes the
engine and faer's tree. A method that runs the engine is to do so behind a
Cargo feature, removed with it (planned: no method runs it yet).

## Files

| File                                                                   | Defines                                                                                           |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| [`src/engine.rs`](src/engine.rs)                                       | `CpuMpsEngine`, `DEFAULT_CUTOFF`; the crate-private `Engine`: the operations, truncation, routing |
| [`src/chain.rs`](src/chain.rs)                                         | `CanonicalMps`, `SplitStatistics`: the chain and its tensor kernels                               |
| [`src/linear_algebra.rs`](src/linear_algebra.rs)                       | `FactorizationError`; the crate-private `LinearAlgebra`, `Matrix`, `MatrixRef`, `Qr`, `Svd`       |
| [`src/linear_algebra/cpu.rs`](src/linear_algebra/cpu.rs)               | the crate-private `CpuLinearAlgebra`                                                              |
| [`src/linear_algebra/cpu/jacobi.rs`](src/linear_algebra/cpu/jacobi.rs) | the SVD of `CpuLinearAlgebra`: one-sided Jacobi after a pivoted-QR trim                           |
| [`src/error.rs`](src/error.rs)                                         | `CpuMpsError`                                                                                     |

The behaviours above are pinned by the tests in [`src/tests/`](src/tests/)
(the engine, the chain and the linear algebra, and the conformance kit once
per fusion setting) and in [`tests/`](tests/) (against the QDK's sparse
state simulator and its stabilizer simulator).

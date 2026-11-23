# Gates on distant sites, and windows on three sites

A split factors the tensor of two neighbouring sites, so it applies a gate
on neighbours; a gate on sites further apart, or the gates that fusion
gathers on three sites, act on more than a pair. This page gives the
approach the engine uses, a matrix product operator (MPO) across the span;
why it loses no weight beyond what the capacity and the cutoff drop at the
caller's cuts, and, in the factorisation of the operator, a part of the
operator at or below the operator's rank cutoff; and how routing by SWAPs
compares. The crate's
[README](../README.md#how-each-mpsengine-method-is-implemented) summarizes
it, and [`src/engine/mpo.rs`](../src/engine/mpo.rs) implements it.

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
`CanonicalMpsState::statistics` reports the largest transient bond
apart from the largest bond at rest.

**The cost.** A window on a span of $d + 1$ sites costs the moves that
bring the centre into the span, then $d - 1$ moves of the centre and $d$
splits, each on matrices up to the capacity's times the operator's bond
on a side, plus the reads, products and uploads of the span's sites on
the host. Every backend call is one of the operations the engine already
makes elsewhere: moves and splits as for a gate, reads and uploads as for
a projection.

## Compared with routing by SWAPs

Routing by SWAPs, as the crate's earlier rule `swap-back-2x` did, moves
the site at `high` down to `low + 1` by adjacent SWAPs, applies the gate
to the pair at `low`, and moves the site back: $2(d - 1) + 1$ splits for
sites $d$ apart, against $d$ splits and $d - 1$ moves of the centre for
an MPO, and one window per gate, since a route acts on a pair at a time.
Its outbound splits factor the state at cuts that are not the caller's,
which an MPO never does:

- there they drop a tail below the cutoff that no cut of the caller's
  order holds (the case is in
  [the crate's MPO tests](../src/tests/engine/mpo.rs));
- there they may need twice the bond of the cut below, so up to
  $2\chi$, $\chi$ the capacity's largest bond: truncated to the
  capacity, a route loses weight from states that fit it in the caller's
  order; kept within $2\chi$, it pays for bonds the caller never asked
  for.

# Needs, offers and the admission

For those who state a part's needs or offer, write an adapter that adapts
them, or read an admission's refusal: what the needs and an offer are,
how the `on` that makes a runner checks them, and what a refusal names.
The terms are those of the [README](../README.md#terms).

## Needs and offers

The driver states what its shots [`Needs`](../src/needs.rs),
such as `Needs::qubits(2)`, and the executor its
[`Offer`](../src/offer.rs), such as
`Offer::unlimited().max_qubits(64).max_shots(1000)`. Each adapter adapts
both: the needs it passes down (`needs_below`), such as a code's physical
qubits for its logical ones, and the offer it passes up (`offers_above`).

An adapter, such as a job queue, may lower a run's limits, its most shots
and whether it may fix its seed, but never raise them for a run, which is
checked against every part's offer. The admission's check of replay,
though, reads the offer each adapter passes up, its seedability included,
as the paragraphs on replay that follow say.

A driver that runs a shot of an earlier run again alone, such as a
debugger, needs replay (`Needs::with_replay`). Since the shot is run again
with the run's seed, an offer meets that need only if its shots are
[replayable](../src/offer.rs) (`Offer::not_replayable` says they
are not) and a run may fix its seed (`Offer::not_seedable` says it may
not).

Replay is checked at each boundary, as the
[blocks](instruction-sets.md#terms) are, not over the whole chain as a
run's limits are. By default, an adapter passes the need down and the
offer below up, unchanged, so the admission refuses the chain over a part
below that is not replayable or not seedable.

An adapter that replays its shots itself, such as one that keeps each
shot's readouts, need not pass the need down, and offers replay above.
Its offer meets the need only if it is seedable too: over a part below
that is not seedable, an adapter whose offer keeps that part's
seedability, such as one that passes the offer below up with only
`replayable` set, is short on its own, so the admission refuses the
chain and names it.

## The admission

The `on` that makes a runner checks, at every boundary, the needs that
reach it against the offer there, and refuses the chain when any falls
short ([`UnmetNeed`](../src/needs.rs)). The refusal
([`AdmissionFaults`](../src/composition/chain_check/admission.rs)) lists each part's needs and
offer, and names the parts it finds short
([`ShortOffer`](../src/composition/chain_check/admission.rs)), each by its position and its name
([`PartId`](../src/composition/part_id.rs)); an adapter that only
passes a shortage up, such as a profiler, is not named. Which parts a
refusal names is in [`composition/chain_check/admission.rs`](../src/composition/chain_check/admission.rs).

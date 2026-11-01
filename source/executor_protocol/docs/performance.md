# Performance

[Protocol overview](../README.md).

What does the protocol add to an executor's own work?
`benches/protocol_overhead.rs` answers it on a counting profiler, an
executor that does little work of its own (it counts each instruction,
tracks the highest qubit and draws a value for each measurement), so that
what the protocol adds is a large share of its time. It calls the profiler
with batches of 1, 10, 100 and 10 000 instructions of a
4-million-instruction gate mix, five ways: four through the protocol, and
one as the profiler of QEP (a tool outside this repository, written before
the protocol, that runs QIR programs on backends of its own), with no
protocol. Before it times anything, the bench asserts that every way gives
the same counts, highest qubit and readouts, and that each per-shot
executor took one call per batch.

**In short:** an executor of a Rust instruction type, called directly, is
the baseline. The per-shot executor (`executor_fn` under `OnePerShot`)
cost about 1.2 times the baseline on batches of one instruction, and
within a few percent from batches of 10, in one run on a loaded machine:
a provisional figure, to be measured again (see [Results](#results)). An
earlier per-shot form, since removed (measured history), which routed
each call through an object that passed each shot's call to that shot's
executor ([below](#what-each-cost-is-and-who-pays-it)), added a cost per
call,
which batching hides: 4.4 times the baseline on
batches of one instruction, 1.2 times on batches of 100, and within a few
percent on batches of 10 000 (the table below). An executor of calls of a
set described at run time costs about 10 ns more per instruction, at every
batch size. A batch of
many shots, each on a placement of its own, costs its caller a time per
shot that grows with the batch when it is woken by each shot that ends
([Shots on placements of their own](#shots-on-placements-of-their-own)).

## Results

Nanoseconds per instruction, and the ratio to the baseline, typed (below 1
is faster):

| batch  | typed | typed per-shot | run-time      | run-time per-shot |
| :----- | ----: | :------------- | :------------ | :---------------- |
| 1      |  9.96 | 44.82 (4.41×)  | 20.38 (2.04×) | 51.51 (5.22×)     |
| 10     |  2.91 | 6.79 (2.34×)   | 14.05 (4.79×) | 18.20 (6.21×)     |
| 100    |  3.27 | 3.92 (1.19×)   | 14.00 (4.06×) | 14.15 (4.35×)     |
| 10 000 |  2.24 | 2.31 (1.03×)   | 12.79 (5.60×) | 12.49 (5.62×)     |

Method: `taskset -c 4 cargo bench -p executor_protocol --bench
protocol_overhead`, on one core of a Qualcomm Oryon machine under Linux,
while no build ran; each cell is the fastest of 7 runs, and the median of
5 runs of the bench, taken separately for the times and for the ratios.
The numbers vary by machine and with its load: compare the columns of one
run, not runs on different machines, and read single cells as approximate.

**Not measured again yet:** the table and the per-shot costs below were
measured on that earlier per-shot form (measured history), before the
per-shot columns were made with `executor_fn` under `OnePerShot`, which
runs each call in `call`, with no object between. One run of the bench on
the new columns, on a loaded machine, gave about 1.2 times the baseline
for typed per-shot on batches of one instruction (2 ns per call) and
within a few percent from batches of 10; the table is to be measured
again with the method above, with the QEP-like column, which it lacks.

The columns:

- **typed**: the profiler as an `Executor<InstructionBatch<Gate>>`, where `Gate`
  is the mix's instructions as a Rust type, called directly.
- **typed per-shot**: that profiler as the state of each shot of an
  executor made with `executor_fn` under `OnePerShot`, run on one shot by
  a run (a driver composed with the executor).
- **run-time**: the profiler as an executor of `InstructionCall`s of the
  mix's set described at run time, which checks each batch against the set
  and finds a call's readout through an `OpcodeTable`, called directly.
- **run-time per-shot**: that profiler as the state of each shot of an
  executor made so, by a driver that states the set in its needs.
- **QEP-like**: the profiler as QEP's, called directly on one shot, with no
  protocol: it takes QEP's instruction type, finds a gate's counter by
  matching its name against QEP's instruction names, and gives its
  readouts in order, untagged.

Every column but QEP-like checks that each batch's readout tags are unique
(`check_unique_tags`), as every executor that reports by tag must
([replies](replies.md#readout-tags)); QEP reports its readouts by
position.

**Against QEP, typed is the fair comparison.** QEP has no shots, so it
pays nothing for them, and its instruction set is fixed when it is
compiled, which is what a Rust instruction type encodes. The per-shot
columns pay for shots, which only a driver that uses them needs; the
run-time columns answer a question QEP cannot, a set known only at run
time, such as one loaded from a Qodec file. The QEP-like numbers are to be
measured with the table.

## What each cost is, and who pays it

A chain is a type: each part holds the part downstream of it by its type,
not behind a `dyn`, so a part that is not in a chain adds no code to it
and costs it nothing (type-erased chains are planned:
[Planned features](planned.md)). Batching spreads what a part adds per
call over the batch.

- **A call** costs the typed profiler about 8 ns more per batch of one
  instruction than per instruction of a batch of 10 000: the call, its
  reply and the wait for it. Batching spreads it over the batch.
- **The earlier per-shot form** (measured history), which the per-shot
  executor replaced, cost about 30 to 40 ns more per call, on batches of 1
  and 10 instructions: it routed the batch to the shot's executor,
  gathered the shot's reply and tracked its own lifecycle. The per-shot
  executor's cost is to be measured again (above). Batching spreads it
  too, so it matters only for small batches. Only an executor that holds
  a state per shot pays it.
- **An executor of calls of a set described at run time** costs about 10
  ns more per instruction than one of a Rust type, at every batch size:
  it checks each call against the set (`InstructionSet::check_batch`),
  finds its readout through a table, and reads its operands from a 64-byte
  `InstructionCall` rather than a 16-byte `Gate`. The bench does not split
  these; `benches/instruction_calls.rs` measures a set's check and a
  call's representation on their own
  ([instruction sets](internals/instruction-sets.md#the-checks-the-rules)).
  Only an executor whose set is known at run time pays it.

## Shots on placements of their own

`benches/shot_replies_wake.rs` measures what the reply of a batch whose
shots each run on a placement of their own
(`.run_on(place).shots(OnePerShot)`) costs its caller. The reply
polls every shot still pending at each poll, so a caller woken once by
each shot that ends polls about n²/2 shots' replies for a batch of n
shots. The executor reads one bit per shot, so the reply's cost is most of
the time; the bench runs the placements' jobs itself, on one thread.

Nanoseconds per shot, and the ratio to the first column:

| shots | polled once | polled per shot | one placement |
| ----: | ----------: | --------------: | ------------: |
|     1 |         800 |     800 (1.00×) |   800 (1.00×) |
|    16 |         450 |     575 (1.28×) |    56 (0.12×) |
|   256 |         389 |   2 791 (7.17×) |   9.0 (0.02×) |
| 1 024 |         369 |  10 215 (27.7×) |   5.7 (0.02×) |

- **polled once**: the bench runs every shot's job, then polls the reply
  once.
- **polled per shot**: the same executor, its reply polled after each
  shot's job, as a caller woken by each shot that ends: each poll costs
  about 19 ns per shot still pending, n/2 of them on average.
- **one placement**: `.shots(OnePerShot).run_on(place)`, every shot
  in the one job, its reply polled once.

Method: `taskset -c 11 cargo bench -p executor_protocol --bench
shot_replies_wake`, on one core of a Qualcomm Oryon machine under Linux
(WSL), at a load average of about 1; each cell is the fastest of 7 runs;
two runs of the bench agreed within 2 % past one shot. A batch of one shot
is near the timer's resolution.

Two costs show. The wakes cost O(n²) per batch: a known issue, open,
whose fix polls only the shots with news (a waker per shot, or a queue of
the shots woken). And a shot on a placement of its own costs about 370 ns
more than one in a shared job, for its own job, its reply and the copy of
the batch; only an executor that places its shots apart pays it.

## Caveats

- Only the counting profiler was measured. In an executor that does more
  work per instruction (a simulator, for instance), these costs are a
  smaller share of the time.
- Building the instructions is not timed: each column's instructions
  (`Gate`s, `InstructionCall`s) are built before the timing starts.
- Adapters are not measured yet: no column runs through one, so neither
  an adapter's own cost nor that of the executor `compose` makes from it
  (`AdapterExecutor`, or an `adapter_fn`'s) is known. Columns with
  an adapter that passes calls down, and with a lowering, upstream of the
  per-shot executor, are to be added to `benches/protocol_overhead.rs`.
  Estimates from an earlier prototype of the protocol, not measured on this
  crate: a lowering of a Steane code cost about 9 ns per physical
  instruction (8.4 to 10.4), plus 51 µs to build, and a validator (a
  probe), which checks each batch before passing it on, 6 to 7 ns per
  instruction.

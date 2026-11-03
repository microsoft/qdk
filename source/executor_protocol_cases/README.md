# End-to-end cases of the executor protocol

> **Examples, not a library, and not for merging.** This branch holds worked
> cases of the executor protocol of
> [PR #4073](https://github.com/microsoft/qdk/pull/4073)
> ([`source/executor_protocol`](../executor_protocol/README.md)), run end to
> end on real code. It grows as cases are added. Nothing here is meant to be
> used or maintained as is.

## The problem

The protocol's own tests run its parts on small doubles. A partner who
writes a part (a code runtime, a simulator, a device) wants to see the
protocol carry a real case: an instruction set known only at run time,
code layers composed as adapters over a device, a program run on many
shots, and results that carry each shot's conditions. Each case here does
that with real code, and shows where the protocol falls short.

## What is here

- [`qodec_demo`](../qodec_demo/README.md): Qodec code stacks run as
  chains of the protocol. Each layer of a qodec (repetition3, c422, c4c6,
  Steane, …) is an adapter over the layer below, and the bottom is a
  device of coins. Its README gives the model, what it shows, and the
  findings: five ignored tests, each stating what should hold and fails
  today ([Findings](../qodec_demo/README.md#findings)).
- [`qodec_runtime`](../qodec_runtime) and
  [`qodec_lowerer`](../qodec_lowerer): unchanged copies of the Qodec
  runtime prototype and its lowerer, at commit `564b78cf4` of the
  `oscarpuente/qodec-runtime` branch. The lowerer's examples are the
  qodecs the demo plays. The `qodec` library comes from crates.io (0.2.1).

## The six steps in action

The protocol's [Getting started](../executor_protocol/docs/getting-started.md)
builds a checked chain in six steps. Here is each in the demo's code. The
excerpts are copied from the files linked.

### 1. Define your instruction set

[Step 1](../executor_protocol/docs/getting-started.md#1-define-your-instruction-set).
A qodec's instruction sets are data, read from its YAML at run time.
`Boundary::new`, in [`boundary.rs`](../qodec_demo/src/boundary.rs), makes
each one a set of the protocol, `InstructionSet::new(&isa.name)`, with
each Qodec instruction at its index as opcode: its blocks, its parameters
as integers or reals, and a tag for each readout.

```rust
let observes = decl.observe_count();
let readouts = observes + decl.flags.len();
kinds.extend(std::iter::repeat_n(OperandKind::Tag, readouts));
set = set.with(&decl.mnemonic, kinds);
```

What the protocol's operands cannot express (a variadic block, a text
argument) is refused, each fault named.

### 2. Write your program and its driver

[Step 2](../executor_protocol/docs/getting-started.md#2-write-your-program-and-its-driver).
A program is fixed batches of top instructions, written in the Qodec
REPL's syntax. `Program::parse`, in
[`program.rs`](../qodec_demo/src/program.rs), tags its readouts in program
order, and `to_static` makes it the protocol's `StaticProgram`, stating
its set:

```rust
pub fn to_static(&self, needs: Needs) -> StaticProgram<InstructionCall> {
    let needs = needs.with_instruction_set(Some(Arc::clone(&self.top.set)));
    StaticProgram::new(needs, self.batches.clone())
}
```

So the driver is the protocol's own, `static_driver()`
([A program of fixed batches](../executor_protocol/docs/getting-started.md#a-program-of-fixed-batches));
the demo writes none.

### 3. Write your executor

[Step 3](../executor_protocol/docs/getting-started.md#3-write-your-executor).
The device is coins, in [`coin.rs`](../qodec_demo/src/coin.rs): a state per
shot, each shot drawing from its own seed. The `play` example's coins read
1 with a chance `p`, as a device with noise would:

```rust
let promise = Capabilities::default()
    .with_resumable_shots(true)
    .with_instruction_set(Some(set));
let make = move |shot: ShotContext| {
    Ok(BiasedCoins {
        seed: shot.seed(),
        drawn: 0,
        p,
    })
};
executor_fn(promise, make, toss_biased)
    .seed(seed)
    .shots(OnePerShot)
```

What a call does to a shot is its own logic, `toss_biased`, one coin per
tag:

```rust
for call in input.instructions() {
    for operand in call.operands.iter() {
        if let Operand::Tag(tag) = *operand {
            let one = uniform(coins.seed, coins.drawn) < coins.p;
            coins.drawn += 1;
            outcomes.push(Outcome::new(tag, one.into()));
        }
    }
}
```

The tests' coins, `coin_device`, are the same shape, and also check each
call against their set and report what they ran.

### 4. Add adapters

[Step 4](../executor_protocol/docs/getting-started.md#4-add-adapters).
Each Qodec layer is an adapter, in
[`qodec_layer.rs`](../qodec_demo/src/qodec_layer.rs): it owns the way
below each shot, since the runtime calls below while it lowers. It states
the set it accepts and the set it sends, so the chain matches it to the
set below when it is built, and checks each call:

```rust
owning_adapter_fn(make, serve)
    .accepts(accepted)
    .sends(sent)
    .check(move |input| check(&checked, blocks, input.instructions()))
```

`QodecLayer::adapter` gives it a state per shot,
`self.one_state().shots(OnePerShot)`, since the runtime keeps one shot's
state. The profiler, in [`profiler.rs`](../qodec_demo/src/profiler.rs),
is an adapter too: it counts the instructions that reach the device and
sends them on.

```rust
adapter_fn(|_| Ok(Profile::default()), profile)
    .state_report(Profile::clone)
    .shots(OnePerShot)
```

### 5. Run many shots and read the results

[Step 5](../executor_protocol/docs/getting-started.md#5-run-many-shots-and-read-the-results).
The [`play`](../qodec_demo/examples/play.rs) example composes the chain,
one adapter per layer above the device, and runs the program on many
shots:

```rust
1 => go!(static_driver() >> layer(0) >> profiler() >> coins()),
```

```rust
let chain = $chain.expect("every boundary matches");
let (outcomes, (profile, _device)) = chain
    .run(program, shots.clone())
    .expect("the job fits the chain")
    .wait();
```

Each outcome is a shot's readout under its tag, with the shot's
conditions: the Qodec runtime's status of the result (`rejected`,
`inconsistent`, `unresolved`), and a value it could not resolve reads
`missing`. The report holds the profiler's count (the `play` example's
coins report nothing). On Steane's code, at `p` 0.01:

```text
results (tag=value, conditions in brackets):
  shot 0: 0=0 1=0 2=0 3=0 4=0 5=0 6=0
  shot 1: 0=0 1=0 2=0 3=0 4=0 5=0 6=0
  shot 2: 0=0 1=0 2=0 3=0 4=0 5=0 6=0
  shot 3: 0=0 1=0 2=0 3=0 4=0 5=0 6=0
  shot 4: 0=0 1=0 2=0 3=0 4=0 5=0 6=0
  shot 5: 0=0 1=0 2=0 3=0 4=0 5=0 6=0
  shot 6: 0=0 1=0 2=0 3=0 4=0 5=0 6=missing[inconsistent]
  shot 7: 0=0 1=0 2=0 3=0 4=0 5=0 6=0
  … 8 more shots
how often (16 shots):
      10  0=0 1=0 2=0 3=0 4=0 5=0 6=0
       3  0=missing[inconsistent] 1=missing[inconsistent] 2=missing[inconsistent] 3=missing[inconsistent] 4=missing[inconsistent] 5=missing[inconsistent] 6=missing[inconsistent]
       1  0=0 1=0 2=0 3=0 4=0 5=0 6=missing[inconsistent]
       1  0=0 1=0 2=0 3=1 4=0 5=0 6=0
       1  0=0 1=0 2=1 3=0 4=0 5=0 6=0
profile: 32 bottom batches (one per shot each), 1712 instructions
       352  R
       240  H
       768  CX
       304  M
        48  MX
```

### 6. Check your parts

[Step 6](../executor_protocol/docs/getting-started.md#6-check-your-parts).
[`tests/conformance.rs`](../qodec_demo/tests/conformance.rs) runs the
checks of the protocol's `conformance` feature, one per test, on the coins
and on repetition3's layer 0 composed with them:

```rust
macro_rules! layer_over_coins {
    () => {{
        let qodec = repetition3();
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        (layer.adapter() >> coin_device(stim_rz().set, SEED)).expect("the boundary matches")
    }};
}
```

```rust
conformance::continues_a_shot_from_one_call_to_the_next(
    || layer_over_coins!(),
    &ShotSet::range(0..=1).expect("0..=1 is not empty"),
    first,
    second,
);
```

The checks found a fault: the coins took a batch that repeats a tag, which
they now refuse. On the layer, the batches prepare a block before they
measure it, since the runtime fails a measurement of a block never
prepared; the check of a call in flight at an earlier failure uses just
that failure
([The protocol's conformance checks](../qodec_demo/docs/findings.md#the-protocols-conformance-checks)).

## How to play

From the repository's root. Given a qodec alone, `play` lists its top
instructions:

```sh
cargo run --release -p qodec_demo --example play -- \
    source/qodec_lowerer/examples/steane/steane.qodec.yaml
```

```text
2 layers; the top one's instructions, with their operands:
  prepare_z [Block]
  prepare_x [Block]
  idle [Block]
  idle_ft [Block, Tag, Tag, Tag, Tag, Tag, Tag]
  measure_z [Block, Tag]
  measure_x [Block, Tag]
  h [Block]
  cnot [Block, Block]
```

Then give it a program, its statements separated by `;` and its top
batches by `|`; the tags are numbered for you. After the program come, all
optional, the shots (16), the chance `p` that a coin reads 1 (0.01), and
the seed. The run of step 5 is:

```sh
cargo run --release -p qodec_demo --example play -- \
    source/qodec_lowerer/examples/steane/steane.qodec.yaml \
    "prepare_z 0; idle_ft 0 | measure_z 0" 16
```

At `p` 0, a device with no noise, every shot reads 0 with no condition
(output trimmed to its summary):

```sh
cargo run --release -p qodec_demo --example play -- \
    source/qodec_lowerer/examples/steane/steane.qodec.yaml \
    "prepare_z 0; idle_ft 0 | measure_z 0" 16 0
```

```text
how often (16 shots):
      16  0=0 1=0 2=0 3=0 4=0 5=0 6=0
```

c4c6 stacks two code layers over the device; a qodec given as a folder is
played from its `qodec.yaml` (output trimmed):

```sh
cargo run --release -p qodec_demo --example play -- \
    source/qodec_lowerer/examples/c4c6/qodec.yaml \
    "prepare_z_all 0 | idle 0; idle 0; idle 0 | measure_z_all 0" 100
```

```text
how often (100 shots):
      35  0=0 1=0[unresolved] 2=0[unresolved]
      32  0=0 1=0[rejected,unresolved] 2=0[rejected,unresolved]
      18  0=0[rejected] 1=0[rejected,unresolved] 2=0[rejected,unresolved]
…
profile: 2700 bottom batches (one per shot each), 100000 instructions
```

The lowerer's [examples](../qodec_lowerer/examples) hold more qodecs:
bacon-shor, surface, reed-muller-15, and others. `play` stacks one to
three layers above the device. The demo's tests, and its other examples,
are in its README's
[How to run it](../qodec_demo/README.md#how-to-run-it).

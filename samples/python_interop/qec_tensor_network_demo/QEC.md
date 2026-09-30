# QEC Tensor-Network Demo: Exact Record Probabilities by Contraction

This demo computes the probability of a full measurement record of a quantum
error-correction (QEC) circuit exactly, with cuTensorNet on one GPU, and shows
how the memory this needs grows with the number of QEC rounds. The questions it
answers:

> Up to how many rounds can one GPU compute P(r) exactly by general tensor-network
> contraction, what does the plan cost beyond that, and how does an MPS with a
> fixed bond dimension compare?

It is a preview for feedback, not a shipped feature. The API is the preview
`qdk.simulation.tensornetwork_qir`, whose names may change. A companion demo,
[2D Ising quench](../ising2d_tensor_network_demo/Ising2D.md), uses the same API
for MPS accuracy against cost.

| | |
| --- | --- |
| Circuit source | a Stim circuit compiled by `qdk.stim.compile` to adaptive-profile QIR (SELECT/REQUIRE become branches); not included here |
| Entry point | `tensornetwork_qir(qir, [Probability(), Cost()], method="contraction" \| "mps", outcomes=r)` |
| Exact method | general contraction with an optimized, unsliced path |
| Approximate method | MPS with bond dimension cap χ |
| Reference | P(r) is known in closed form (below), so every value is checked, not compared to another simulator |
| Reference host | NVIDIA A100 80GB PCIe, Linux x86_64 |

---

## TL;DR

```text
Stim ─► qdk.stim.compile ─► QIR ─┬─► tensornetwork_qir(method="contraction", outcomes=r) ─► P(r) exact, Cost: w, flops, workspace
                                 └─► tensornetwork_qir(method="mps", χ, outcomes=r)       ─► P(r), Cost: bond, state bytes
                                        check: P(r) = 2^-m (valid r),  P(r) = 0 (flipped r)
```

**What you get.** One row per number of rounds k: the contraction width w and
workspace, exact P(r) where the workspace fits the GPU, the MPS result at a fixed
χ with its memory and time, and optional cotengra widths from the host. Also a
memory-against-k plot (against 20- and 40-qubit state-vector baselines and the
GPU memory), a time-against-k plot, the path-search effort table, and MPS χ sweeps.

---

## 1. What P(r) is

A record $r$ has one bit per QIR result, in result order, and SELECT acceptance is
included. $P(r)$ is the probability that the circuit $U$ produces exactly $r$:

$$
P(r) = \bigl\lVert \Pi_r\, U \lvert 0\cdots 0\rangle \bigr\rVert^2 ,
$$

where $\Pi_r$ projects onto the outcomes fixed by $r$. For a stabilizer QEC circuit
without noise, each measurement is either random (probability $1/2$ for each
outcome) or deterministic. With $m$ random outcomes:

$$
P(r) =
\begin{cases}
2^{-m} & \text{valid record } r \\
0 & r \text{ with one deterministic bit flipped}
\end{cases}
$$

A record is written as a string of `0`/`1`, result 0 first. A valid record comes
from a noiseless stabilizer walk of the same circuit: each random measurement takes a
random bit, each deterministic one its forced value, and each SELECT read the
accepting branch. A flipped record flips one deterministic bit. A value is judged exact when its
relative error is $\le 10^{-9}$ or, for $P = 0$, when $\lvert P\rvert \le 10^{-9}\cdot 2^{-m}$. These numbers are
the cost of **one probability**, not of sampling.

## 2. Why memory, not time, is the limit

A contraction plan has a width $w$: $\log_2$ of the largest intermediate tensor. The
workspace cuTensorNet asks for grows as $2^w$ (measured; complex128 plus buffers):

$$
\text{workspace} \approx (24\text{–}32\ \text{B})\cdot 2^{w},
\qquad
\text{fits an } 80\ \text{GiB GPU} \iff w \le 31 .
$$

`Cost()` alone plans and prepares but never contracts, so w is known even when the
contraction cannot run. The table marks those rows "not run: workspace > GPU".
More search effort (`ContractionOptions(hyper_samples=…, seed=…)`) trades
planning time for a possibly smaller $w$; the path-search table shows whether it helps.

An MPS keeps memory polynomial by capping the bond dimension $\chi$. Its state grows
linearly with the number of qubits at fixed $\chi$. It is exact only when $\chi$ is at
least the bond the circuit needs; below that the error shows as a wrong $P(r)$.

## 3. Running it

The API, directly (`run_qir` returns shots; `tensornetwork_qir` returns one value
per query, without shot noise):

```python
import json
import qdk.stim
from qdk.simulation import tensornetwork_qir, Probability, Cost, ContractionOptions, MpsOptions

qir, _ = qdk.stim.compile(stim_text)
records = json.load(open("records.json"))["records"]   # {"r0": "0110…", …}
r = [bit == "1" for bit in records["r0"]]        # outcome of QIR result i = r[i]

p, cost = tensornetwork_qir(qir, [Probability(), Cost()], method="contraction",
                            outcomes=r, options=ContractionOptions(hyper_samples=8, seed=17))
[plan] = tensornetwork_qir(qir, [Cost()], method="contraction", outcomes=r)   # plans, never contracts
p, cost = tensornetwork_qir(qir, [Probability(), Cost()], method="mps",
                            outcomes=r, options=MpsOptions(max_bond_dimension=512))
```

Measure on the GPU (one `tensornetwork_qir` call per record, so the time includes
path finding):

```bash
python live.py --circuit n.ll --records records.json                        # contraction
python live.py --circuit n.ll --records records.json --method mps --chi 512
```

`records.json` holds `{"m": m, "records": {name: bits}, "expected": {name: P}}`.
By default `live.py` runs the first valid record and the first flipped one, and
exits 1 if any value is not exact.

Render a report from probe results files (no GPU):

```bash
python run.py render results/*.json --cotengra cotengra.jsonl --output report
```

Results files hold `inputs` (per circuit `name_k<k>`: `qubits`, `m`) and `cases`
(one per call: `method`, `record`, `queries`, `expected`, `status`,
`probability`, `cost`, `wall_seconds`, `hyper_samples`/`seed` or `chi`). This
prints the Markdown tables and writes `report.qec-memory.png` and
`report.qec-time.png`.

Tests run on any host with test doubles in place of the GPU:

```bash
python -m pytest samples/python_interop/qec_tensor_network_demo
```

## 4. Not yet

- **Slicing.** Plans are unsliced, so exact contraction stops where the workspace
  exceeds the GPU. Slicing trades that memory for repeated work.
- **Imported plans.** Host optimizers such as cotengra can find smaller widths
  than cuTensorNet's for some circuits; running their plans on cuTensorNet is
  not supported yet.
- **Sampling.** This demo computes the probability of a given record; drawing
  records from the network is separate work.

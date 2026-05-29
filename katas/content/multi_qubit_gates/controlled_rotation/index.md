**Inputs:**

1. Two qubits in an arbitrary state $\ket{\phi}$, stored as an array of length 2.
2. An angle $\theta$: $-\pi < \theta \leq \pi$.

**Goal:** Apply a controlled $R_x$ gate, using the first qubit as control and the second qubit as target, with $\theta$ as the angle argument for the gate.

:::hint
<p>If you were to apply a regular version of $R_x$ gate, it would take two parameters - angle $\theta$ as the first parameter and the target qubit as the second parameter.</p>
:::
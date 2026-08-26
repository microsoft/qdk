# Physical qubit addressing

This sample demonstrates a *physical qubit addressing* pattern: it treats QIR qubit IDs as fixed
physical addresses on the target machine. The machine size is specific to the hardware you target;
these examples assume a 256-qubit machine, so when you adapt the pattern, set the pool size to the
qubit count of the machine you target. Two equivalent programs prepare all four Bell states on four
pairs of physical addresses:

- [BellStatePhysical.qs](BellStatePhysical.qs) — Q#.
- [BellStatePhysical.qasm](BellStatePhysical.qasm) — OpenQASM 3.

## What "physical addressing" means here

In the usual programming model, a qubit variable is an opaque handle. The runtime and the target
provider are free to place it on any physical qubit and to remap it during compilation. Physical
addressing instead assumes a fixed mapping: the array index you choose in source becomes the QIR
qubit ID, and that QIR qubit ID lands on the corresponding physical site on the machine.

That behavior is only meaningful when three facts hold together:

1. **The source represents the complete machine exactly once.** Allocating one qubit pool sized to
   the whole machine makes its array indices correspond one-to-one with QIR qubit IDs. This example
   uses 256 qubits, so indices `0..255` map to QIR qubit IDs `0..255`; size the pool to match your
   target machine.
2. **Addresses are selected by constant index.** Picking an element such as `machine[12]` names the
   intended physical address in the generated QIR.
3. **The provider target preserves those IDs.** Source indices and QIR IDs do not by themselves
   force a hardware layout. A target may perform further gate decomposition, routing, scheduling,
   or qubit remapping after it receives the QIR.

The first two facts are visible in the source. The third must be confirmed with the hardware
provider. Use this pattern only with a target whose documented contract exposes the full set of
addressable qubits and preserves the meaning of the QIR qubit IDs you select.

## The programs

Each program prepares the four Bell states on these address pairs:

- $|\Phi^+\rangle$ on addresses 12 and 173;
- $|\Phi^-\rangle$ on addresses 40 and 190;
- $|\Psi^+\rangle$ on addresses 71 and 205; and
- $|\Psi^-\rangle$ on addresses 99 and 233.

The programs allocate or declare the entire 256-qubit machine once and use no other qubits. Each
Bell state has its own address pair, so every qubit is measured exactly once. All four states are
prepared before any measurement occurs, then each address is measured independently at the end of
the program. This ordering is required by the Base Profile and is also valid for other profiles.

In the computational basis, the $|\Phi^+\rangle$ and $|\Phi^-\rangle$ pairs produce the correlated
outcomes `00` or `11`, while the $|\Psi^+\rangle$ and $|\Psi^-\rangle$ pairs produce the
anti-correlated outcomes `01` or `10`. The relative phase is not visible in these measurements.
Result formatting and bit ordering can vary by provider.

## Physical-addressing checklist

Every program written for physical qubit addressing must follow these constraints:

1. **Represent the complete machine once.** Allocate one machine-sized `Qubit[]` in Q#, or declare
   one machine-sized `qubit[]` in OpenQASM. Do not allocate or declare qubits anywhere else. A
   second allocation, or a helper that allocates workspace/ancilla qubits, breaks the fixed layout.
2. **Audit helper implementations.** QIR generation for operations from `Std.Intrinsic` and
   `Std.Canon` does not add ancillary qubits. Review operations from other libraries and custom
   decompositions before using them; an implementation can allocate workspace qubits even if its
   call site does not.
3. **Restrict controlled operations.** Avoid operations with multiple controls. CCX/Toffoli is the
   supported exception when its ancilla-free decomposition and the target gate set are suitable.
   Do not assume that arbitrary multi-controlled operations preserve the fixed address space.
4. **Measure separately and at the end.** Complete every quantum operation before performing any
   measurement. For Base Profile Q#, use independent terminal `MResetZ` calls and avoid joint
   Pauli-product measurements. In OpenQASM, use separate terminal measurement statements. Giving
   each output its own physical qubit avoids measuring and then reusing an address.
5. **Check the compiled artifact.** When you compile to QIR, confirm that the entry point declares
   `required_num_qubits` equal to your machine size (`256` in this example) and that the quantum
   instructions reference the intended QIR qubit IDs (12, 40, 71, 99, 173, 190, 205, and 233 here).
   Re-run these checks whenever the program, compiler, or target profile changes. This validates the
   generated QIR—not the provider's post-submission layout.
6. **Confirm provider behavior.** Source indices and QIR IDs do not by themselves force a provider
   to preserve a hardware layout. Use only a target whose documented contract supplies the needed
   physical-address semantics. A target that accepts the QIR but freely remaps its qubit IDs does
   not provide the addressing semantics demonstrated by this sample.

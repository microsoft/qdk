# Teleportation

@[section]({
    "id": "teleportation__overview",
    "title": "Overview"
})

Quantum teleportation protocol allows us to communicate a quantum state using only classical communication and previously shared quantum entanglement.

- Teleportation is described in [this Wikipedia article](https://en.wikipedia.org/wiki/Quantum_teleportation).
- An interactive demonstration can be found [on the Wolfram Demonstrations Project](http://demonstrations.wolfram.com/QuantumTeleportation/).

@[section]({
    "id": "teleportation__standard_teleportation",
    "title": "Standard Teleportation"
})

In the standard teleportation protocol, two parties, the sender (typically referred to as Alice) and the receiver (Bob) start by sharing an entangled pair of qubits. The goal of the protocol is for Alice to transfer an unknown quantum state to Bob using their shared qubit pair and a classical communication channel.

We split the teleportation protocol into several steps:

- Preparation (creating the entangled pair of qubits that are sent to Alice and Bob).
- Sending the message (Alice's task): Entangling the message qubit with Alice's qubit and extracting two classical bits to be sent to Bob.
- Reconstructing the message (Bob's task): Using the two classical bits Bob received from Alice to get Bob's qubit into the state in which the message qubit had been originally.
- Finally, we compose these steps into the complete teleportation protocol.

@[exercise]({
    "id": "teleportation__entangled_pair",
    "title": "Entangled Pair",
    "path": "./entangled_pair/"
})

@[exercise]({
    "id": "teleportation__send_message",
    "title": "Send Message (Alice's Task)",
    "path": "./send_message/"
})

@[exercise]({
    "id": "teleportation__reconstruct_message",
    "title": "Reconstruct Message (Bob's Task)",
    "path": "./reconstruct_message/"
})

@[exercise]({
    "id": "teleportation__standard_teleportation_protocol",
    "title": "Standard Teleportation Protocol",
    "path": "./standard_teleportation_protocol/"
})

@[exercise]({
    "id": "teleportation__prepare_and_send_message",
    "title": "Prepare Message and Send It",
    "path": "./prepare_and_send_message/"
})

@[exercise]({
    "id": "teleportation__reconstruct_and_measure_message",
    "title": "Reconstruct Message and Measure It",
    "path": "./reconstruct_and_measure_message/"
})

@[section]({
    "id": "teleportation__testing_standard_teleportation",
    "title": "Testing Quantum Teleportation"
})

In this lesson, your goal is to put together the code from the previous exercises to teleport the states $\ket{0}$ and $\ket{1}$, as well as superposition states $\frac{1}{\sqrt{2}}(\ket{0}+\ket{1})$, $\frac{1}{\sqrt{2}}(\ket{0}-\ket{1})$, $\frac{1}{\sqrt{2}}(\ket{0}+i\ket{1})$ and $\frac{1}{\sqrt{2}}(\ket{0}-i\ket{1})$, and to verify that teleportation succeeds each time.

> This is an open-ended task that is not tested automatically, unlike the previous exercises. Follow the suggestions in the comments to write your code and test it!

@[example]({
    "id": "teleportation__testing_standard_teleportation_example", 
    "codePath": "./examples/TestingStandardTeleportation.qs"
})


@[section]({
    "id": "teleportation__different_entanglement_pair",
    "title": "Teleportation Using Different Entangled Pair"
})

In this lesson we will take a look at the changes in the reconstruction process (Bob's task) if the qubits shared between Alice and Bob are entangled in a different state. Alice's part of the protocol remains the same in all exercises.

As a reminder, the standard teleportation protocol requires shared qubits in state $\ket{\Phi^{+}} = \frac{1}{\sqrt{2}}(\ket{00} + \ket{11})$.

In each exercise, the inputs are:

1. Bob's part of the entangled pair of qubits `qBob`.
2. The tuple of classical bits received from Alice, in the format used in the `SendMessage` exercise.

The goal is to transform Bob's qubit `qBob` into the state in which the message qubit has been originally.

@[exercise]({
    "id": "teleportation__reconstruct_message_phi_minus",
    "title": "Reconstruct Message with |Φ⁻⟩",
    "path": "./reconstruct_message_phi_minus"
})

@[exercise]({
    "id": "teleportation__reconstruct_message_psi_plus",
    "title": "Reconstruct Message with |Ψ⁺⟩",
    "path": "./reconstruct_message_psi_plus"
})

@[exercise]({
    "id": "teleportation__reconstruct_message_psi_minus",
    "title": "Reconstruct Message with |Ψ⁻⟩",
    "path": "./reconstruct_message_psi_minus"
})


@[section]({
    "id": "teleportation__three_parties",
    "title": "Teleportation with Three Parties"
})

There are multiple variants of teleportation protocol that involve more than two parties. In this lesson, we'll take a look at two of them:

- Entanglement swapping allows us to propagate entanglement across space, enabling protocols such as quantum repeater.
- In teleportation with three entangled qubits, a state is transferred from Alice to a third participant Charlie, but this may only be accomplished if Charlie has the trust of the second participant Bob.

@[exercise]({
    "id": "teleportation__entanglement_swapping",
    "title": "Entanglement Swapping",
    "path": "./entanglement_swapping/"
})

@[exercise]({
    "id": "teleportation__entangled_trio",
    "title": "Entangled Trio",
    "path": "./entangled_trio"
})

@[exercise]({
    "id": "teleportation__reconstruct_message_charlie",
    "title": "Reconstruct Message (Charlie's Task)",
    "path": "./reconstruct_message_charlie"
})

@[section]({
    "id": "teleportation__principle_of_deferred_measurement",
    "title": "Principle of Deferred Measurement"
})

The principle of deferred measurement claims that measurements can be moved from an intermediate stage of a quantum circuit to the end of the circuit. If the measurement results are used to perform classically controlled operations (such as the fixup done on Bob's side), they can be replaced by controlled quantum operations.

The principle of deferred measurement is typically considered in the context of quantum computations rather than quantum communication protocols such as teleportation protocol. It requires that qubits involved in all parts of the protocol are close enough physically to allow performing multi-qubit gates on them, and in teleportation protocol Alice's and Bob's qubits are separated by physical distance. Otherwise, it would be trivial to use a $SWAP$ gate to swap the state Alice's message qubit with Bob's qubit to achieve the goal of teleportation protocol!

However, teleportation protocol makes for a nice simple example of applying the principle of deferred measurement to modify the computation, so there is some merit to considering it.

@[exercise]({
    "id": "teleportation__measurement_free_teleportation",
    "title": "Measurement-Free Teleportation",
    "path": "./measurement_free_teleportation"
})

@[section]({
    "id": "teleportation__conclusion",
    "title": "Conclusion"
})

Congratulations! In this kata you learned to implement teleportation protocol and several variants of this protocol that act under different assumptions.

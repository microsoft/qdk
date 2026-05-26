# Superdense Coding

@[section]({
    "id": "superdense_coding__overview",
    "title": "Overview"
})

Superdense coding protocol allows us to transmit two bits of classical information by sending just one qubit using previously shared quantum entanglement. We are assuming that Alice is the sender and Bob is the receiver.

- A good description can be found in [the Wikipedia article](https://en.wikipedia.org/wiki/Superdense_coding).
- A great interactive demonstration can be found [on the Wolfram Demonstrations Project](https://demonstrations.wolfram.com/SuperdenseCoding/).

We split the superdense coding protocol into several steps:

- Preparation (creating the entangled pair of qubits that are sent to Alice and Bob).
- Encoding the message (Alice's task): Encoding the classical bits of the message into the state of Alice's qubit which then is sent to Bob.
- Decoding the message (Bob's task): Using Bob's original qubit and the qubit he received from Alice to decode the classical message sent.
- Finally, we compose those steps into the complete superdense coding protocol.

@[exercise]({
    "id": "superdense_coding__entangled_pair",
    "title": "Entangled Pair",
    "path": "entangled_pair"
})

@[exercise]({
    "id": "superdense_coding__alice_sends_message",
    "title": "Alice's Task: Encode Message",
    "path": "alice_sends_message"
})

@[exercise]({
    "id": "superdense_coding__bob_decodes_message",
    "title": "Bob's Task: Decode Message",
    "path": "bob_decodes_message"
})

@[exercise]({
    "id": "superdense_coding__protocol_e2e",
    "title": "Superdense Coding Protocol: End to End",
    "path": "protocol_e2e"
})

@[section]({
    "id": "superdense_coding__conclusion",
    "title": "Conclusion"
})

Congratulations! In this kata you learned to implement superdense coding protocol.

# QEC: Bit Flip, Phase Flip, and Shor Codes

@[section]({
    "id": "qec_shor__overview",
    "title": "Overview"
})

This kata introduces you to the basic concepts of quantum error correction using several simple error correction codes.

**This kata covers the following topics:**

- Simple models of noise in quantum systems
- Parity measurements in Z and X bases
- Bit flip code - the simplest code that protects qubits from the effects of bit flip noise
- Phase flip code - the simplest code that protects qubits from the effects of phase flip noise
- Shor code - the simplest code that can protect from an arbitrary error on a single qubit

**What you should know to start working on this kata:**

- Basic single-qubit and multi-qubit gates
- Single-qubit and multi-qubit quantum measurements and their effect on quantum systems

@[section]({
    "id": "qec_shor__noise",
    "title": "Noise in Classical and Quantum Systems"
})

Any quantum system we can use to carry out quantum computation is inherently noisy. 
Quantum noise can be caused by different physical processes, depending on the type of a particle or device used as a qubit.
From the computation point of view, the presence of noise in the quantum system means that its state can suffer from random errors, and thus end up different from the state we're relying on to do our computations. 
This makes computations unreliable very fast, since the effects of noise accumulate quickly to make computation results effectively random.

Before we dive into dealing with noise in quantum systems, let's take a quick look at how we do that for classical systems.

The model used for analyzing classical noise is called *binary symmetric channel*, in which classical bits sent through the channel are transmitted correctly with probability $1-p$ and flipped with probability $p$.
In this scenario, the information sent through the channel can be protected against the effects of the noise using the *repetition code*:

- On the sender side, we replace each bit we want to send with three copies of itself:
$$0 \rightarrow 000, 1 \rightarrow 111$$
- On the receiver side, we decode the original bit by majority vote: 
$$000, 100, 010, 001 \rightarrow 0$$
$$111, 011, 101, 110 \rightarrow 1$$

What is the probability of this scheme failure, that is, the value of the message bit changing after it was sent through the channel? 
Majority vote allows for one error on any bit to happen without affecting the decoding outcome, so it would take two or three errors happening on individual bits for decoding to produce an incorrect result. The probability of this happening is $3p^2(1-p) + p^3 = 3p^2 - 2p^3$. If we compare this with the probability of an individual bit transmission failing $p$, we can see that using the repetition code yields higher success probability, as long as $p < \frac12$. We can improve success probability further by increasing the number of repetitions we use to encode each bit: $5$ repetitions allow us to detect and correct $2$ errors, $7$ repetitions - $3$ errors, and so on.

> This noise model is useful not only for describing noisy communication channels, but also for memory - any classical system that introduces errors in information when it is left on its own, as opposed to systems that introduce errors during information manipulation. Indeed, we assume that no errors are introduced as we copy the bits during encoding or read and compare their values during decoding.

The main idea of quantum error correction is the same as that for classical error correction: encode information with enough redundancy that we can recover the message even from the noisy transmission results.
Dealing with the noise in quantum systems is more challenging than in classical systems, though, due to the limitations imposed by their nature:

- **No cloning**: We cannot replicate the repetition code for quantum systems in a straightforward manner, by duplicating the quantum state several times, since the no-cloning theorem prohibits that.
- **Observing the system damages information**: Even if we could produce several copies of a quantum state we want to transmit, we would not be able to compare them afterwards without damaging their state.
- **Errors are continuous**: We need to recover from arbitrary errors that are much more complicated than the bit flip error we have in classical systems.

The simplest model used to analyze quantum noise is *quantum depolarizing channel*. 
In this model, we assume that we send qubits through a channel that transmits the qubit unchanged with probability $1-p$, and applies one of the Pauli gates $X$, $Y$, and $Z$ with probability $\frac{p}{3}$ each. The effects of the noise on each qubit transmitted are independent.

At first glance, this model seems to be limited, and not representative of the full spectrum of errors that can occur in a quantum system. Fortunately, it turns out that any errors on encoded states can be corrected by correcting only a discrete subset of errors - exactly the Pauli $X$, $Y$, and $Z$ errors! This is called *discretization of quantum errors*, and we'll see how it works later in this kata.

> For now, we are assuming that all errors are introduced by the channel, and the gates and measurements we use for encoding and decoding procedures of a quantum error correction code are perfect and don't introduce any errors themselves. This is a useful assumption to get started with error correction, but in real life all gates and measurements are noisy, so eventually we'll need to modify our approach. 
> *Fault-tolerant quantum computation* handles the more general scenario of performing computations on encoded states in a way that tolerates errors introduced by noisy gates and measurements.

@[section]({
    "id": "qec_shor__joint_measurements",
    "title": "Joint Measurements"
})

Quantum error correction is based on the use of a special kind of measurements - joint measurements in Pauli bases.
We introduced the single-qubit measurements in different Pauli bases in the Measurements in Single-Qubit Systems kata,
and then the general case of joint measurements in the Measurements in Multi-Qubit Systems kata.
Let's take a closer look at the kinds of joint measurements we'll be using in this kata.

A multi-qubit Pauli measurement on $n$ qubits corresponds to an operator $M_1 \otimes \dotsc \otimes M_n$, with each $M_j$ being from the set of gates $\\{X,Y,Z,I\\}$, and at least one of the $M_j$ is not the identity matrix. (If $M_j = I$, you can think of it as qubit $j$ not being involved in the measurement.) The measurement can produce one of the two outcomes: `Zero` corresponding to eigenvalue $+1$ of this operator, or `One` corresponding to the eigenvalue $-1$. The corresponding projection operators are the projections onto the corresponding eigenspaces. The operator $M_1 \otimes \dotsc \otimes M_n$ is referred to as the _measurement basis_.

For example, the first two joint measurements we'll encounter later in this kata are two-qubit measurements in $ZZ$ and $XX$ bases. They can be described as follows:

<table>
    <tr>
        <th>Pauli Operator</th>
        <th>Eigenvalue</th>
        <th>Eigenvectors</th>
        <th>Measurement Projector</th>
        <th>Measurement Result in Q#</th>
    </tr>
    <tr>
        <td rowspan="2">$ZZ$</td>
        <td>$+1$</td>
        <td>$\ket{00}$, $\ket{11}$</td>
        <td>$\ket{00}\bra{00} + \ket{11}\bra{11}$</td>
        <td>Zero</td>
    </tr><tr>
        <td>$-1$</td>
        <td>$\ket{01}$, $\ket{10}$</td>
        <td>$\ket{01}\bra{01} + \ket{10}\bra{10}$</td>
        <td>One</td>
    </tr>
    <tr>
        <td rowspan="2">$XX$</td>
        <td>$+1$</td>
        <td>$\ket{++}$, $\ket{--}$</td>
        <td>$\ket{++}\bra{++} + \ket{--}\bra{--}$</td>
        <td>Zero</td>
    </tr><tr>
        <td>$-1$</td>
        <td>$\ket{+-}$, $\ket{-+}$</td>
        <td>$\ket{+-}\bra{+-} + \ket{-+}\bra{-+}$</td>
        <td>One</td>
    </tr>
</table>

In Q#, joint measurements in Pauli bases are implemented using the `Measure` operation.
It takes two parameters: the array of `Pauli` constants (`PauliI`, `PauliX`, `PauliY`, or `PauliZ`) that define the basis for measurement, and the array of qubits to be measured.

@[exercise]({
    "id": "qec_shor__zz_measurement",
    "title": "Measurement in ZZ basis",
    "path": "./zz_measurement/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "qec_shor__xx_measurement",
    "title": "Measurement in XX basis",
    "path": "./xx_measurement/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})


@[section]({
    "id": "qec_shor__bit_flip_code",
    "title": "Bit Flip Code"
})

Can we reuse the ideas of a classical repetition code for a quantum error correction code? 

The naive approach to it would be to try and encode a quantum state $\ket{\psi}$ as several copies of itself: 
$\ket{\psi} \rightarrow \ket{\psi} \otimes \ket{\psi} \otimes \ket{\psi}$.
Unfortunately, the no-cloning theorem and the inability to reconstruct a state accurately after measuring it prevent us from doing that.

We can, however, take a slightly different approach: encode the *basis states* $\ket{0}$ and $\ket{1}$ in repetition code using a unitary transformation, and deduce the effects of this transformation on superposition states based on its linearity:

$$\ket{0} \rightarrow \ket{000}, \ket{1} \rightarrow \ket{111}$$

$$\alpha \ket{0} + \beta \ket{1} \rightarrow \alpha \ket{000} + \beta \ket{111}$$

This encoding is called **bit flip code**, and the states $\ket{000}$, $\ket{111}$, and their linear combinations are called **code words** in this code. The bit flip code allows us to detect and correct some errors that can occur on qubits in the depolarizing channel, though not all of them.

Let's see what happens if an $X$ error happens on one of the qubits, and how we can detect it using two parity measurements (measurements in the $ZZ$ basis).

<table>
<tr>
<th>Error</th>
<th>State after the error</th>
<th>Parity of qubits 0 and 1</th>
<th>Parity of qubits 1 and 2</th>
</tr>
<tr>
<td>No error</td>
<td>$\alpha \ket{000} + \beta \ket{111}$</td>
<td>$0$</td>
<td>$0$</td>
</tr>
<tr>
<td>$X_0$ (error on qubit $0$)</td>
<td>$\alpha \ket{100} + \beta \ket{011}$</td>
<td>$1$</td>
<td>$0$</td>
</tr>
<tr>
<td>$X_1$ (error on qubit $1$)</td>
<td>$\alpha \ket{010} + \beta \ket{101}$</td>
<td>$1$</td>
<td>$1$</td>
</tr>
<tr>
<td>$X_2$ (error on qubit $2$)</td>
<td>$\alpha \ket{001} + \beta \ket{110}$</td>
<td>$0$</td>
<td>$1$</td>
</tr>
</table>

You can see that these two parity measurements give us different pairs of results depending on whether the $X$ error happened and on which qubit. This means that we can use them to detect the error, and then correct it by applying an $X$ gate to the qubit that was affected by it.

However, if a $Z$ error happens on any one of these qubits, we won't be able to detect it: it will convert the state $\alpha \ket{000} + \beta \ket{111}$ to the state $\alpha \ket{000} - \beta \ket{111}$ which is a valid code word in this code - it's an encoding of the quantum state $\alpha \ket{0} - \beta \ket{1}$. We'll need to come up with a different way to detect $Z$ errors.

@[exercise]({
    "id": "qec_shor__bitflip_encode",
    "title": "Bit Flip Code: Encode Codewords",
    "path": "./bitflip_encode/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "qec_shor__bitflip_detect",
    "title": "Bit Flip Code: Detect X Error",
    "path": "./bitflip_detect/",
    "qsDependencies": [
        "../KatasLibrary.qs",
        "./Common.qs"
    ]
})


@[section]({
    "id": "qec_shor__phase_flip_code",
    "title": "Phase Flip Code"
})

What kind of code could detect and correct a $Z$ error? We detected an $X$ error using the fact that in the $\\{\ket{0}, \ket{1}\\}$ basis the error changed the basis state. Similarly, we can detect a $Z$ error using the $\\{\ket{+}, \ket{-}\\}$ basis, in which the $Z$ gate converts $\ket{+}$ to $\ket{-}$ and vice versa, acting as a basis change operation.

Based on this idea, we can construct the **phase flip code** that uses the following encoding:

$$\ket{0} \rightarrow \ket{+++}, \ket{1} \rightarrow \ket{---}$$

$$\alpha \ket{0} + \beta \ket{1} \rightarrow \alpha \ket{+++} + \beta \ket{---}$$

Let's see what happens if a $Z$ error happens on one of the qubits, and how we can detect it using two parity measurements. 
This time we'll do the parity measurements in the $X$ basis to distinguish the cases of $\ket{++}$ and $\ket{--}$ (parity $0$) from $\ket{+-}$ and $\ket{-+}$ (parity $1$).

<table>
<tr>
<th>Error</th>
<th>State after the error</th>
<th>$XX$ parity of qubits 0 and 1</th>
<th>$XX$ parity of qubits 1 and 2</th>
</tr>
<tr>
<td>No error</td>
<td>$\alpha \ket{+++} + \beta \ket{---}$</td>
<td>$0$</td>
<td>$0$</td>
</tr>
<tr>
<td>$Z_0$ (error on qubit $0$)</td>
<td>$\alpha \ket{-++} + \beta \ket{+--}$</td>
<td>$1$</td>
<td>$0$</td>
</tr>
<tr>
<td>$Z_1$ (error on qubit $1$)</td>
<td>$\alpha \ket{+-+} + \beta \ket{-+-}$</td>
<td>$1$</td>
<td>$1$</td>
</tr>
<tr>
<td>$Z_2$ (error on qubit $2$)</td>
<td>$\alpha \ket{++-} + \beta \ket{--+}$</td>
<td>$0$</td>
<td>$1$</td>
</tr>
</table>

You can see that these two parity measurements give us different pairs of results depending on whether the $Z$ error happened and on which qubit. This means that we can use them to detect the error, and then correct it by applying a $Z$ gate to the qubit that was affected by it.

However, if an $X$ error happens on any one of these qubits, we won't be able to detect it: it will convert the state $\alpha \ket{+++} + \beta \ket{---}$ to the state $\alpha \ket{+++} - \beta \ket{---}$ which is a valid code word in this code - it's an encoding of the quantum state $\alpha \ket{0} - \beta \ket{1}$. We'll need to come up with a different way to detect both $X$ and $Z$ errors in the same encoding.

@[exercise]({
    "id": "qec_shor__phaseflip_encode",
    "title": "Phase Flip Code: Encode Codewords",
    "path": "./phaseflip_encode/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "qec_shor__phaseflip_detect",
    "title": "Phase Flip Code: Detect Z Error",
    "path": "./phaseflip_detect/",
    "qsDependencies": [
        "../KatasLibrary.qs",
        "./Common.qs"
    ]
})


@[section]({
    "id": "qec_shor__shor_code",
    "title": "Shor Code"
})

Can we combine the lessons learned from the bit flip and phase flip error correction codes to be able to detect and correct both $X$ and $Z$ errors? In that case, we'd also be able to handle $Y$ errors as a combination of $X$ and $Z$ errors happening at the same time, and, as a result, we'll be able to detect and correct an arbitrary single-qubit error.

Shor code, published in 1995, is the code that combines the approaches of the bit flip and phase flip codes to do just that.
It uses the following 9-qubit encoding for logical states:

$$\ket{0} \rightarrow \ket{0_L} = \frac1{2\sqrt2} (\ket{000} + \ket{111}) \otimes (\ket{000} + \ket{111}) \otimes (\ket{000} + \ket{111})$$
$$\ket{1} \rightarrow \ket{1_L} = \frac1{2\sqrt2} (\ket{000} - \ket{111}) \otimes (\ket{000} - \ket{111}) \otimes (\ket{000} - \ket{111})$$
$$\alpha \ket{0} + \beta \ket{1} \rightarrow \alpha \ket{0_L} + \beta \ket{1_L}$$

How can we detect and correct errors using this encoding?

### Detect and Correct X Errors

$X$ errors happening on any qubit manifest very similarly to the way they do in the bit flip code. 
Let's consider the first triplet of qubits and an error that happens on any of the first three qubits.
Same as in the bit flip code, measuring the parity of pairs of qubits always returns $0$ if there is no error (since all bits in each basis state of the code words are the same), so a parity measurement returning $1$ on one or two pairs indicates an error, and the measurements which returned $1$ allow us to track down the qubit on which it happened.

To correct an $X$ error, we simply apply an $X$ gate to the affected qubit.

### Detect and Correct Z Errors

$Z$ errors in Shor code behave similarly to the way they do in the phase flip code, but the error detection and correction procedure has to be modified.

A $Z$ error happening on any qubit of a triplet flips the relative sign between the basis states $\ket{000}$ and $\ket{111}$ on those qubits. This means that we need a measurement that would compare relative signs of whole triplets, rather than individual qubits, allowing us to distinguish $(\ket{000} + \ket{111}) \otimes (\ket{000} + \ket{111})$ and $(\ket{000} - \ket{111}) \otimes (\ket{000} - \ket{111})$ (parts of valid code words) from $(\ket{000} + \ket{111}) \otimes (\ket{000} - \ket{111})$ and $(\ket{000} - \ket{111}) \otimes (\ket{000} + \ket{111})$ (parts of code words with a $Z$ error applied).

The measurement that allows us to do this is a 6-qubit measurement in the $X$ basis.

> How can you check this? Remember that doing a measurement in the $X$ basis is the same as applying Hadamard gates to each qubit and then doing a measurement in the $Z$ basis (and then applying Hadamard gates again). 
> 
> - If we apply Hadamard gates to each qubit of the state $\frac1{\sqrt2}(\ket{000} + \ket{111})$, we get the state 
> $\frac12(\ket{000} + \ket{011} + \ket{101} + \ket{110})$. The parity of each basis state in it is $0$.
> - If we apply Hadamard gates to each qubit of the state $\frac1{\sqrt2}(\ket{000} - \ket{111})$, we get the state 
> $\frac12(\ket{001} + \ket{010} + \ket{100} + \ket{111})$. The parity of each basis state in it is $1$.
>
> Thus, a 6-qubit measurement in the $X$ basis of two triplets, each either in the state $\frac1{\sqrt2}(\ket{000} + \ket{111})$ or $\frac1{\sqrt2}(\ket{000} - \ket{111})$, would produce parity $0$ if both triplets have the same relative sign between the basis states and $1$ if the relative sign is different.

To correct a $Z$ error, we can no longer simply apply a $Z$ gate to the affected qubit, since we can only figure out the triplet of qubits where the error happened, not the exact qubit. To work around this, we correct a $Z$ error by applying a $Z$ gate to each qubit of the affected triplet - and $Z$ gates applied to unaffected qubits just cancel each other out.


@[exercise]({
    "id": "qec_shor__shor_encode",
    "title": "Shor Code: Encode Codewords",
    "path": "./shor_encode/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})

@[exercise]({
    "id": "qec_shor__shor_detect",
    "title": "Shor Code: Detect X, Y, and Z Errors",
    "path": "./shor_detect/",
    "qsDependencies": [
        "../KatasLibrary.qs"
    ]
})


@[section]({
    "id": "qec_shor__error_discretization",
    "title": "Discretization of Quantum Errors"
})

Does Shor code indeed correct all errors, and not just the set of Pauli errors $X$, $Y$, and $Z$? 
Let's try it out!

The following demo puts together the steps of error correction using Shor code: it encodes a given logical state into multiple qubits, introduces an arbitrary error, runs the error detection code and applies error correction if necessary, and checks that the result is an accurate encoding of the starting logical state. Experiment with applying different errors to different qubits of the code - and not just the Pauli errors but any single-qubit rotations too. You can even use a measurement!

@[example]({"id": "qec_shor__shor_code_demo", "codePath": "./examples/ShorCodeDemo.qs"})


@[section]({
    "id": "qec_shor__conclusion",
    "title": "Conclusion"
})

Congratulations! In this kata you learned the basics of quantum error correction and several simple error-correction codes.
Here are a few key concepts to keep in mind:

- Quantum error correction is more challenging compared to classical error correction, since quantum information cannot be copied, the act of observing the system damages information stored in it, and there is a much broader variety of errors in quantum systems compared to only bit flip errors in the classical systems.
- Discretization of quantum errors is the phenomenon that allows us to correct arbitrary errors on quantum systems by correcting only a limited discrete subset of errors. For single-qubit errors, this subset is Pauli $X$, $Y$, and $Z$ errors.
- The bit flip quantum error correction code allows us to detect and correct only $X$ errors, the phase flip code only $Z$ errors, and the Shor code allows us to detect and correct one arbitrary single-qubit error.

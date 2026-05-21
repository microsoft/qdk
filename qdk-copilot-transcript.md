# QDK + GitHub Copilot Demo Script

## Introduction

The capabilities of AI are advancing rapidly and with the Microsoft Quantum Development Kit and its tight integration with VS Code and GitHub Copilot, you can leverage those capabilities in your quantum learning, research, and development. In this quick video, I'll give you an overview of just a few of those capabilities. To follow along, you'll want the QDK installed. For more details on that, go to aka.ms/QDK.install.

## Getting Started — Signing In & Setup

Let's get started. To use GitHub Copilot, you'll need to sign in with a GitHub account. You can go to the Copilot settings to see your plan and the models available. I'll use my test account to demonstrate enabling AI features and signing in with a free tier GitHub account in VS Code. In VS Code, open a chat window, enable AI features, and then sign in with your GitHub account. Once signed in, you can select from the models available in your plan. Here, I'll leave the model selection at auto and ask it a simple question about the currently open quantum code.

For the main demo, I'll switch to my personal GitHub account where I have more models available. And here you can see the list at the time of recording.

## Circuit Image to OpenQASM Code

I'm going to try to avoid writing any quantum code in this demo. So, I'll start by dropping in a screen clipping of a quantum circuit and asking Copilot what it is and can it write the OpenQASM code to implement it.

It's done a pretty good job recognizing teleportation and implementing it, and it runs correctly.

## Porting from OpenQASM to Q#

One area where AI shines is in porting code. So I'll ask it to rewrite this OpenQASM to Q# so I can take advantage of the Q# unit testing features.

Looks like it successfully ported that code from OpenQASM to Q#.

## Refactoring & Unit Testing

Let's ask it to refactor this code to make it more testable and ask it to write some unit tests to verify teleporting various quantum states.

It's added the necessary imports, written the tests, and they all pass. It's also given me a nice summary of the tests it wrote.

## Explaining Code

It's written this arbitrary state teleport test, and I'm going to ask it to explain to me how this works.

It's done a pretty good job outlining its use of joint operations to undo the prepared state on the target qubit and check for the zero state.

## Quantum-Controlled Gates (Removing Mid-Circuit Measurement)

To check its quantum understanding, let's ask it to remove any mid-circuit measurement and implement the algorithm with quantum controlled gates instead of classical control flow and then verify the tests still pass.

And it's done a good job of making those changes and the tests are still passing.

## Bug Finding & Fixing

Okay. Well, let's introduce a bug — maybe something with a typo or a cut and paste error. We can now see the tests are failing. Starting from a clean Copilot session, let's ask it to find and fix the bug and ensure the tests pass.

And it's correctly identified the issue, made the change, and the tests are all passing again.

## Grover's Algorithm — Explaining Algorithms & Language Features

Let's clear out this code and work with something a bit more complex, such as the built-in Grover's algorithm sample available from the editor completion list. We can ask Copilot to explain parts of this algorithm, such as how does the reflect-about-uniform operation work. And we can see it's combined its understanding of the code with its quantum knowledge to give a comprehensive answer.

We can also ask it to explain aspects of the language we may not understand, such as what does the within/apply pattern do in Q#. And here you can see you can use Copilot to help you learn quantum programming languages.

## Resource Estimation

We can ask it to give us resource estimates for running this on an error-corrected system and it will use the integrated resource estimation features to come back with a comprehensive analysis.

## Azure Quantum — Submitting Jobs & Fetching Results

If I'm happy with this code, I can ask it to check the status of the hosted simulators available in my Azure Quantum workspace and then to submit the job to the simulator I'd like to use. As there's a little queue time right now, I'll ask it to fetch the results of the last completed job on that workspace. It'll fetch the results from the service and display them in a histogram for me in VS Code.

## Closing

In a few minutes, I had Copilot write some quantum code starting from a simple image of a circuit, refactor it, test it, explain some of the algorithm to me, explain some other quantum concepts, fix a bug, run a simulation, connect to the service, submit the job, and fetch the results — all without me writing a single line of code.

I highly encourage you to give it a try and explore some of the capabilities for yourself. Like any tool, it is better at some things than others, and it does take some practice to get used to. However, unlike a lot of tools, it is advancing rapidly, and where it doesn't meet your expectations today, it may exceed them tomorrow. It's an exciting time. Happy quantum coding.

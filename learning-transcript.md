# QDK Learning Experience — Cleaned Transcript

## Introduction — Quantum Katas in VS Code

I want to show you a preview of our QDK learning experience.

You may already be familiar with the Quantum Katas. It's a complete quantum computing course hosted on our website that teaches you the basics of quantum computing through hands-on Q# exercises. We wanted to modernize this experience by building it into our QDK VS Code extension, right where people actually use our tools.

So let me get straight into it.

## The Learning Panel & Course Navigation

Here we have our QDK learning panel, and all you have to do to get here is install our VS Code extension, available freely on the VS Code Marketplace. If I'm a beginner, the Quantum Katas course will walk me through a complete introduction. But here I'm going to assume I have a specific topic in mind.

On the left we have our course navigation panel that lists the available topics. Now this is human-written course content, but we'll be able to get help from Copilot anytime we want. It all feels natural and integrated.

## Copilot-Assisted Navigation & Exercises

So here Copilot found the Grover's unit in the course and took me there.

Now we have a hands-on exercise. Let me ask Copilot for a hint. While it does that, I'm going to type in my solution to this exercise. Each of these exercises is a small Q# problem designed to teach a specific concept. Each comes with a checker that validates your solution.

And while I work on that, notice how Copilot did give me a nice helpful hint on the right here.

There you go, my solution was correct.

So that is AI-assisted learning with the QDK right here in VS Code — and that's for a relative beginner. That's the Quantum Katas.

## Extensible Course Material — Advanced Chemistry Example

Now, the nice thing about this model is that we can extend it with further course material. We can add tutorials and samples in Q#, Python, OpenQASM.

Here we set up an additional course, Advanced Quantum Chemistry, that delves into QDK chemistry topics. This material is more geared towards a researcher.

Now, just like last time with the katas, we can choose to take this course in a linear progression from beginning to end. But here I'm going to jump to a specific topic that I'm interested in now.

Just like before, Copilot is able to pull up relevant course material based on my topic of interest.

## Asking Questions & Going Deeper

Now I could just read through this notebook and run it right here in VS Code, but to really wrap my head around a topic, I need to play with it a little bit and maybe ask questions about it — so I can do just that with Copilot.

Copilot is drawing upon the actual documentation here to answer my question.

Now, while that chugs along, let me show you the behind the scenes on where this content is coming from. If I switch to the Files view here, you can see that I have a folder full of Jupyter notebook files that follow a specific naming scheme. These are Python notebooks, but you can imagine a similar course folder for Q# or OpenQASM content too.

A learner might acquire this course material by downloading a zip file from a website or cloning a repo from GitHub, and a contributor just needs to author these files that fit the specific format that QDK Learning expects.

## Applying Learning to a Specific Problem

Now back here in the chemistry course, Copilot gave me an answer on how I might use the QDK for my chemistry application using up-to-date built-in documentation.

So not only did it relate my question to the appropriate section in the course material, it's explaining to me how I might apply this method to the specific molecule I'm interested in here. I can ask Copilot to apply this learning to a specific problem that I had in mind to really get some hands-on experience using the QDK and turn it into a project that I might apply these lessons to.

Now the agent references the learning material as well as the QDK documentation to build a custom notebook to answer my question. The notebook builds the Hamiltonian for the benzene molecule, simulates the quantum phase estimation algorithm, and runs QDK's resource estimator on the circuit. And here Copilot discusses the results just as I asked.

## Closing

So there you have it. I learned the theory behind a topic from thoughtfully written course material. Then I understood how to apply the QDK and Microsoft's tools to that problem. Then I finally got some hands-on experience building a solution that works for the specific problem I'm interested in.

Thanks for watching.

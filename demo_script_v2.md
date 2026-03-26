# Quantum Katas Demo Script

Prompts to paste: `Start the quantum katas` / `Check my solution` / `Continue the quantum katas`

---

Show the current website.

I want to start with a reminder of what the Quantum Katas look like today. It's a learning experience on our website. It works, but we can do better. It's a toy environment disconnected from where people are actually using our tools. And the custom chat backend we built can't keep up with how fast LLMs are improving.

The idea here is simple: move the katas into VS Code, where the real tools are. And use GitHub Copilot as the AI tutor instead of maintaining our own. The user learns in the same environment they'll actually write quantum code in.

Let me show you what this looks like.

---

Paste: **"Start the quantum katas"**

So what's happening behind the scenes: Copilot is pulling from our curated kata content — the same material that's on the website today — but it's using it to generate a personalized learning plan. It's going to ask me a couple of setup questions so it can calibrate the difficulty.

Pick **Beginner** and **Q#**.

Based on what I told it, it's now building out a workspace — starter files, a progress tracker, the whole structure. This is the part that replaces the website experience. Instead of clicking through a browser, the user is working in real files, in the same editor they'd use for actual quantum development.

Point out the file tree briefly.

---

Copilot teaches the X gate concept.

Before each exercise there's a short lesson. This is drawn from our existing kata content — the same math, same examples — but delivered conversationally and adapted to what the user said they know.

---

Type `X(q);` in `solution.qs`.

This one's simple — we just apply the X gate. But if I were stuck, I could ask the tutor for hints. It gives progressively more specific help — it'll never just write the answer for me.

---

Paste: **"Check my solution"**

Now it's going to do two things. First, it runs verification — these are real tests, the same ones we use on the website, not just the model eyeballing the code. Second — and this is the part I think is really nice — it's going to render a circuit diagram of my solution. Every quantum computing textbook draws circuits. The tutor does too, automatically, using the QDK's built-in circuit rendering. We didn't have to build any new visualization for this.

Point at the circuit.

So I wrote `X(q)` in code, and here it is as a circuit diagram. As exercises get more complex — multi-qubit gates, entanglement — these diagrams become really valuable for understanding what's going on.

---

One more thing. The user can close VS Code and come back a week later.

Paste: **"Continue the quantum katas"**

It's reading a progress file — just a JSON file sitting in the workspace. No account needed, no cloud state. It'll find where the user left off and pick up at the next exercise. This is the kind of thing that makes it feel like a real course, not a one-off interaction.

---

What you just saw uses things we already ship — circuit diagrams, the katas content, Copilot. But there's more we can layer on as the user progresses:

**Histograms.** When they hit measurement exercises, we run their code 100 times and show the probability distribution. The user doesn't just read that superposition gives 50/50 — they _see_ it.

**Circuit editor.** For multi-qubit exercises, the user can drag and drop gates visually before writing code. The editor shows the quantum state at each step — like a debugger for circuits.

**Resource estimation.** At the end of the advanced path, we run resource estimation on algorithms they've built — "your Grover search needs 11,000 physical qubits." It's the capstone that connects exercises to real hardware.

All of these are features the extension already has. The work is wiring them into the tutor flow.

A few open questions. Pacing and control — we don't fully control the Copilot experience. Discoverability — how do people find out it exists? And Python — many learners want it, but that's new exercise content to write.

That's it — questions?

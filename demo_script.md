# Quantum Katas in VS Code — Demo Script

## Setup (before the demo)

- VS Code open with an empty workspace (no quantum-katas folder yet)
- Copilot chat panel visible on the right
- **Pre-record the slow parts.** The two Copilot interactions (initial setup and verification) each take 10-15 seconds. Screen-record those ahead of time as short clips. During the live demo, type the prompt live, then cut to the recording so the audience sees the response stream in without dead air. Announce this: _"I've sped this up so we're not watching a loading spinner."_ People appreciate the honesty and the pacing.
- Have the finished `progress.json` and exercise files ready in a git stash or branch so you can snap to the "resumed session" state instantly if needed.

---

## Part 1: Intro (1 minute)

On screen: Show the current website.

> I want to start with a reminder of what the Quantum Katas look like today. It's a learning experience on our website. It works, but we can do better. It's a toy environment disconnected from where people are actually using our tools. And the custom chat backend we built can't keep up with how fast LLMs are improving.
>
> The idea here is simple: move the katas into VS Code, where the real tools are. And use GitHub Copilot as the AI tutor instead of maintaining our own. The user learns in the same environment they'll actually write quantum code in.
>
> Let me show you what this looks like.

---

## Part 2: Demo (4 minutes)

### Beat 1 — "Start the experience" (~45 sec)

Type into Copilot chat:

> **"Start the quantum katas"**

_[Cut to pre-recorded response]_

Copilot asks about experience level and language preference. Pick **Beginner** and **Q#**.

Copilot responds with a learning plan summary, creates the `quantum-katas/` workspace folder with numbered exercise subfolders, and opens the first exercise file.

**Talk over the response as it streams:**

> It asks a couple of setup questions, picks a learning path, and scaffolds everything — starter files, a progress tracker. This is all generated from our curated kata content; the tutor adapts it to the user's level.

Point out the file tree briefly. Don't dwell.

### Beat 2 — The lesson (~30 sec)

Copilot teaches the concept (the X gate) conversationally, including the matrix and what it does to qubit states.

> Before each exercise there's a short lesson. This is drawn from our existing kata content — the same math, same examples — but delivered conversationally and adapted to what the user said they know.

### Beat 3 — Solve the exercise live (~45 sec)

The starter file is open: `solution.qs` with `// Implement your solution here...`

**Don't ask Copilot for help.** Just type the answer yourself — it's one line:

```qsharp
X(q);
```

This is fast and keeps the energy up. Say:

> This one's simple — we just apply the X gate. But if I were stuck, I could ask the tutor for hints. It gives progressively more specific help — it'll never just write the answer for me.

### Beat 4 — Verification + Circuit (~60 sec)

Type into Copilot chat:

> **"Check my solution"**

_[Cut to pre-recorded response]_

Copilot runs the verification, reports **"Correct!"**, shows a circuit diagram of the solution (a single X gate on one wire), and advances to the next exercise.

**This is the visual payoff moment.** Pause and point at the circuit:

> It verified the solution against built-in tests, and then — this is the part I like — it shows me the circuit. I wrote `X(q)` in code, and here it is as a circuit diagram. Every textbook draws circuits; now the tutor does too, automatically, using the QDK's existing circuit rendering. No new visualization code needed.

### Beat 5 — Session resumption (fast, ~30 sec)

> One more thing. The user can close VS Code and come back a week later.

Open Copilot chat and type:

> **"Continue the quantum katas"**

Copilot reads `progress.json`, finds where the user left off, and picks up at the next exercise.

> It reads the progress file, skips what's done, and picks up right where they left off. No account needed, no cloud state — it's just a JSON file in their workspace.

---

## Part 3: Where we go from here (1 minute)

> What you just saw uses things we already ship — circuit diagrams, the katas content, Copilot. But there's more we can layer on as the user progresses:
>
> - **Histograms.** When they hit measurement exercises, we run their code 100 times and show the probability distribution. The user doesn't just read that superposition gives 50/50 — they _see_ it.
> - **Circuit editor.** For multi-qubit exercises, the user can drag and drop gates visually before writing code. The editor shows the quantum state at each step — like a debugger for circuits.
> - **Resource estimation.** At the end of the advanced path, we run resource estimation on algorithms they've built — "your Grover search needs 11,000 physical qubits." It's the capstone that connects exercises to real hardware.
>
> All of these are features the extension already has. The work is wiring them into the tutor flow.

### Challenges

> A few open questions:
>
> - **Pacing and control.** We don't fully control the Copilot experience — users can change models, settings, tools. The tutor needs to be robust across configurations.
> - **Discoverability.** This requires VS Code + Copilot. How do people find out it exists? Extension welcome view, website, Copilot itself suggesting it when someone asks a quantum question.
> - **Python.** Many learners want Python. We could add a path using the `qsharp` Python package, but that's new exercise content to write.
>
> That's it — questions?

---

## Pacing tips

| Problem                           | Fix                                                                                                                          |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Copilot response takes 15 seconds | Pre-record the two Copilot interactions; cut to video during the live demo                                                   |
| Audience watches you type         | Keep prompts ultra-short ("Start the quantum katas", "Check my solution")                                                    |
| The code exercise is trivial      | That's the point — you're demoing the _flow_, not the difficulty. Pick the simplest exercise so the code moment is instant   |
| Too much to show                  | Resist demoing hints, OpenQASM, custom paths. Save those for Q&A: "Yes, it also supports OpenQASM — happy to show you after" |
| Dead air during workspace setup   | Talk over it: describe what's happening structurally while files appear                                                      |

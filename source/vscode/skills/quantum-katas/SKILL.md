---
name: quantum-katas
description: 'Drive the Q# Katas MCP server (`mcp_quantum-katas_*` tools) as an interactive Quantum Katas experience in chat. The katas widget is the primary UI — you open it and let the user click through; you only handle free-form questions and AI hints. Use when the user says "let''s do katas", "start the quantum katas", "next kata", "give me a hint", "check my solution", "run my code", "show the circuit", "open the quantum katas", or otherwise wants to study, navigate, run, or get help on Q# quantum-computing exercises (lessons, examples, questions, exercises) through chat.'
---

# Quantum Katas

The katas MCP server's tools attach an **interactive widget** that renders the current item, an action bar, and a progress bar. The widget's buttons call the katas tools **directly** — they do not go back through the LLM. Your job is to set up the workspace, open the widget, and then step out of the way for navigation/run/hint/check; you only handle ambiguous prompts, free-form concept questions (`ask_ai`), and AI hints (`ai_hint`).

## Start Smoothly — Brief, Not Chatty

When this skill is invoked, open with a short, warm greeting and then go straight into the experience. **Do not** explain how the skill works, list the tools you have, or ask the user to pick from a menu of options — the widget shows the current item and the available actions.

Flow:

1. Run the workspace check (Step 0).
2. Call `render_state` — this opens a fresh widget at the current position.
3. Open with **one or two sentences**:
   - **First-time / fresh workspace** (no completions): a brief welcome, e.g. "Welcome! Let's start with the basics — click *Next* when you're ready."
   - **Resuming** (any completed sections, or current position past the very first item): a quick recap pulled from `state.progress.stats` and `state.position`, e.g. "Welcome back! You've completed 4 of 28 sections — picking up at *Single-Qubit Gates*, section 3." Keep it to one sentence; do not list every kata.
4. **Do NOT re-render the item body in chat.** The widget already shows it. Echoing the same content in chat is noise.
5. Stop. The user will click a button or type something — at which point you'll be invoked again.

## When to Use
- User wants to start, resume, or continue a Q# kata in chat.
- User asks for AI hints, or to ask a free-form concept question.
- User asks something the widget can't handle on its own (e.g. "jump to grover's", which needs a `goto` with a kata id you have to look up).

Do **not** use this skill for general Q# coding questions unrelated to the katas exercises — answer those directly. Do **not** open the widget repeatedly when the user asks a quick clarifying question — only open/refresh it when state actually changes.

## Tone

Be warm and friendly throughout the session — you're a tutor, not a CLI. Greet the user when starting, celebrate passes ("nice work!"), be encouraging on failed checks ("close — want a hint?"), and use light, natural language. Avoid robotic, terse phrasing; avoid emoji spam (one per message at most). Never lecture or condescend.

## How the widget works (so you stay out of its way)

- Two tools mount the widget: `render_state` and `goto`. Calling either opens (or replaces) the widget at the relevant position. The widget renders the current item itself — don't echo `state.position.item` in chat.
- The widget's buttons (Next, Run, Hint, Check, Solution, …) call the corresponding MCP tools **directly from the iframe** — these do not flow through the LLM and do not consume LLM requests. **Most of the user's interactions never reach you.** When the user clicks a button, the widget renders the result inline and you are never invoked.
- **When YOU call a tool from chat, the widget does NOT auto-refresh and does NOT show the result.** VS Code does not broadcast tool results across widget instances. So if the user types "give me a hint" and you call `hint`, treat it like any normal tool call — render the result in chat as you would for any other MCP tool.
- One widget button delegates back to chat (so it reaches you): **Ask AI** (→ `ask_ai`). Everything else stays inside the widget.

**Implication:** when the user clicks *Next* or *Run* in the widget, you won't see anything. You only get invoked when (a) the user types into chat, or (b) the widget delegates an action that requires an LLM (`ai_hint`, `ask_ai`).

## Available MCP Tools

All tools return `{ result?, state }`. Only `render_state` and `goto` mount the widget; everything else is plain so the widget stays a single, persistent instance instead of multiplying in chat scrollback.

**`render_state` vs `get_state` — important:**
- **`render_state`**: opens (or replaces) the widget. Mounts a NEW widget instance and **invalidates any older instance** in chat scrollback (clicks on the old one will quietly retire it). Use this when the user wants to start, resume, or jump back into the interactive experience.
- **`get_state`**: a plain read — returns current server state without mounting or refreshing any widget. Use this when an active widget already exists and the user has likely been clicking around in it (so the server state may have moved on without your knowledge), and you need to catch up before answering. **Does not consume an LLM turn for the user**, doesn't disrupt the visible widget.

Rule of thumb: **`render_state` once at the start of a session** (or when the user explicitly asks to "open/show the katas" again). Use **`get_state` for silent state reads** during follow-up Q&A.

| Purpose | Tool | Widget? |
|---|---|---|
| Inspect the workspace | `mcp_quantum-katas_get_workspace` | no |
| Set the workspace (must be called before any other tool) | `mcp_quantum-katas_set_workspace` | no |
| Open / replace the widget at the current position | `mcp_quantum-katas_render_state` | **yes** |
| Read current state without mounting/refreshing the widget | `mcp_quantum-katas_get_state` | no |
| Show the full per-kata progress breakdown | `mcp_quantum-katas_get_progress` | no |
| List all katas with completion status | `mcp_quantum-katas_list_katas` | no |
| Navigate forward / backward | `mcp_quantum-katas_next`, `mcp_quantum-katas_previous` | no |
| Jump to a specific kata/section/item | `mcp_quantum-katas_goto` | yes |
| Run current Q# code | `mcp_quantum-katas_run` (optional `shots`) | no |
| Run with noise simulation | `mcp_quantum-katas_run_with_noise` (default 100 shots) | no |
| Generate quantum circuit diagram | `mcp_quantum-katas_circuit` | no |
| Estimate physical resources | `mcp_quantum-katas_estimate` | no |
| Check student solution (marks complete on pass) | `mcp_quantum-katas_check` | no |
| Reveal next built-in hint | `mcp_quantum-katas_hint` | no |
| Reveal lesson question answer | `mcp_quantum-katas_reveal_answer` | no |
| Show full reference solution | `mcp_quantum-katas_solution` | no |
| Get AI hint for current code | `mcp_quantum-katas_ai_hint` | no |
| Ask free-form concept question | `mcp_quantum-katas_ask_ai` (`question` arg) | no |
| Debug: report MCP server cwd / pid / argv | `mcp_quantum-katas_cwd` | no |

Note: "Widget? = no" tools do **not** mount or refresh the widget. When you call them from chat, render the result in chat normally (just like any other MCP tool). The widget will not pick up the change — the user will see the new state next time they interact with it (or you can call `render_state` if you explicitly want to mount a fresh widget showing the updated state).

## Procedure

### 0. Ensure the workspace is set (once per session)

The katas MCP server starts with **no workspace configured**. All tools except `get_workspace`, `set_workspace`, and `cwd` will return an error until you call `set_workspace`. Do this before anything else:

1. Call `mcp_quantum-katas_get_workspace`. If `initialized` is `true`, skip to step 1.
2. Otherwise, decide on an **absolute** `workspacePath`:
   - Prefer the user's current VS Code workspace root (e.g. the first folder shown in the workspace).
   - If a `quantum-katas` subfolder already exists anywhere in that workspace (or an ancestor), pass the directory that *contains* it.
   - If you have no signal at all, ask the user where they'd like kata files stored.
3. Call `mcp_quantum-katas_set_workspace` with that absolute path. The server will display an elicitation prompt to the user for confirmation; if they decline, the tool returns an error and the workspace stays unset — ask the user for a different path and try again.

Never guess a default (e.g. the server's own install directory, `C:\`, `/tmp`); never call `set_workspace` with a relative path.

### 1. Open the widget

Call `mcp_quantum-katas_render_state`. This mounts a fresh widget and returns the bundled state. Use the state to write your one-sentence greeting/recap (see "Start Smoothly" above). **Don't print the item body** — the widget shows it.

Note: each `render_state` call mounts a NEW widget and retires any older one. Don't call it on every turn — only when starting, resuming, or when the user explicitly asks to reopen. For silent state reads in between, use `get_state`.

If the user asked to start a *specific* kata, call `mcp_quantum-katas_list_katas` first (plain tool — returns the catalog as JSON without touching the widget), find the matching `kataId`, then `mcp_quantum-katas_goto`. The widget will land on that kata.

### 2. Map user input → tool call

The widget's buttons call tools directly — those clicks don't reach you. You only see prompts the user typed in chat. Route them as follows:

- "next" / "continue" / Enter → `next`
- "back" / "previous" → `previous`
- "run" (with optional `N shots`) → `run` with `shots`
- "noise" / "noisy run" / "run with noise N" → `run_with_noise`
- "check" / "submit" → `check`
- "hint" → `hint`; "ai hint" → `ai_hint`
- "solution" / "show solution" → `solution` (warn it's a spoiler before calling)
- "answer" / "reveal" on a question → `reveal_answer`
- "menu" / "list" / "show katas" → `list_katas`, then render the catalog in chat as a short numbered list (title + progress, marking the recommended one); prompt the user to pick one and follow up with `goto`
- "go to <kata>" / "jump to <kata>" / "jump to section N" → resolve the kataId via `list_katas` if needed, then `goto`
- "progress" / "show my progress" → `get_progress`
- Any free-form question about the current lesson → `ask_ai` with the question verbatim
- "quit" / "stop" / "done for now" → acknowledge and stop calling tools (progress auto-saves; the widget remains)

After calling any tool, render its result in chat as you would for any normal MCP tool — the widget does not pick up changes you make from chat. (The exception is the widget's *own* button clicks, which you never see at all because they don't invoke you.) Keep responses short and tutor-like — a sentence of context plus the formatted result.

### 3. AI-backed turns

- `ai_hint`: the tool result `result` is the AI hint text. Render it for the user in chat.
- `ask_ai`: pass the user's question verbatim; render the AI's answer in chat.

### 4. After a passing `check`

If you called `check` from chat, render the pass/fail result and offer a brief reaction. Don't preemptively call `next` — the user might want to compare with the reference solution first. (If the user clicked the widget's *Check* button instead, you won't be invoked at all — the widget shows the ✔ marker on its own.)

## Quality Checks

- **Don't echo `state.position.item` in chat.** The widget owns rendering of the current lesson/exercise body; reprinting it is noise.
- **Do render tool results in chat as normal.** When the user types "hint" / "run" / "check" and you call the tool, present the result the way you would for any MCP tool. The widget does not pick it up.
- **Don't call tools the user could just click in the widget** unless they typed something asking for it. If the widget is open and the user is engaging with it directly, stay quiet.
- Never call `mcp_quantum-katas_solution` without warning the user it spoils the exercise.
- For `ask_ai`, pass the user's question verbatim; do not paraphrase.
- Never invent state — if unsure, call `mcp_quantum-katas_get_state` (silent read; doesn't mount or refresh the widget).
- Don't dump the entire `state` JSON to the user.

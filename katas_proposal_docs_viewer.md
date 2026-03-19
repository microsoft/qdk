# Feature Proposal: Documentation Viewer in Quantum Katas

## Summary

As exercises grow more complex, the tutor opens the Q# documentation viewer to show relevant API pages -- teaching the user how to look up operations, understand function signatures, and explore the standard library on their own.

## Motivation

A good learning experience doesn't just teach content -- it teaches how to learn independently. The Q# documentation viewer is a tool that every productive Q# developer uses. Introducing it during the katas creates a natural transition from "the tutor tells me what to use" to "I can look it up myself."

This is also a discoverability play. The QDK ships with a full, browsable, searchable standard library reference built right into VS Code. Many users never find it. Surfacing it during the learning experience ensures every katas user knows it exists and has practiced using it.

## What the user sees

### Just-in-time API discovery

The user reaches an exercise that requires a Q# operation they haven't seen before -- say, `Controlled` functors or `ApplyToEach`. The tutor says:

> "This exercise uses `ApplyToEach`, which applies a single-qubit operation to every qubit in an array. Let me show you the documentation."

The documentation viewer opens in a side panel, showing the `ApplyToEach` page with its signature, description, and examples. The tutor walks through it briefly, then says:

> "From now on, you can open the documentation viewer any time to look up operations. Try searching for 'CNOT' to see how it's documented."

### Exploring the standard library

After introducing the viewer, the tutor can reference it in future exercises:

> "You'll need a rotation gate for this exercise. Check the `Microsoft.Quantum.Intrinsic` namespace in the documentation viewer -- you'll find `Rx`, `Ry`, and `Rz` there."

The user navigates the docs themselves, building the habit of self-directed exploration.

### Post-katas transition

When the user finishes their learning path and the tutor suggests next steps, the documentation viewer is one of the key tools highlighted:

> "You've seen the standard library documentation during the exercises. It covers every operation and function in Q# -- you can browse it any time from the command palette."

## When to introduce it

**Mid-to-late in the learning path.** The documentation viewer adds the most value when exercises start requiring operations the tutor hasn't explicitly taught. For the Beginner path, this is around the measurement or state preparation katas. For the Intermediate and Advanced paths, it can be introduced earlier.

Don't introduce it too early -- in the first few exercises, the tutor should explain everything directly. The viewer is for the moment when the user is ready to start finding things on their own.

## What already exists

The QDK extension already has a documentation viewer:

- A "Show Documentation" command that opens a browsable view of the Q# standard library, organized by package and namespace.
- Full-text search across all documentation entries.
- Markdown rendering with KaTeX math support for equations.
- The `qsharpGetLibraryDescriptions` copilot tool can also retrieve API descriptions for the tutor to reference in the chat, without opening the viewer.

No new features need to be built. The work is in having the tutor open the viewer at appropriate moments and guide the user through using it.

## Open questions

- **Chat vs. panel:** Should the tutor show API docs inline in the chat (using the `qsharpGetLibraryDescriptions` tool), or open the documentation panel? The chat is more convenient for quick lookups; the panel is better for browsing and teaches the user where to find things later.
- **OpenQASM users:** The documentation viewer covers Q# APIs. What do we show OpenQASM users? They may need different reference material.
- **Frequency:** How often should the tutor reference the docs? Too often feels forced; too rarely and the user forgets about it. A natural trigger is "any time the exercise requires an operation the user hasn't used before."

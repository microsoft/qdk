# Bloch Sphere Port — Follow-up TODO

Tracking ideas to explore before opening a PR to bring the Bloch sphere widget
into the main product. Items are not ordered by priority.

## Dependencies

- [x] **Update `three.js` to a current release.** Bumped from `three@^0.161.0` /
      `@types/three@^0.161.2` (Feb 2024) to `three@^0.184.0` /
      `@types/three@^0.184.1` (latest). Builds, lints, and 26 bloch unit tests
      all pass clean against the new version. Visual verification at
      <http://localhost:5555/#bloch> still pending.

## Asset / bundling cleanup

- [x] **Drop the playground font assets and the `FontLoader` code path.**
      Replaced 3D extruded `TextGeometry` labels with `Sprite` + `CanvasTexture`
      labels rendered via a tiny offscreen `<canvas>`. No font asset, no
      runtime fetch, no `three/examples/jsm` Font/Text imports, no VS Code
      build wiring needed. Works in both playground and VS Code webview.
      Removed `source/playground/public/fonts/` and the `.prettierignore`
      entry that excluded it.
- [x] Remove the unused `helvetiker_bold.typeface.json` — gone along with
      the whole fonts directory.

## Code organization

- [ ] **Decide the fate of `source/npm/qsharp/tools/rz-synthesis.ts`.**
      It's a one-shot Node script that regenerates `rz-array.json` /
      `rz-details.json` via brute-force gate-sequence search. The JSON outputs
      are checked in, so it's not on any build critical path. Options:
  - Keep it (current state) — useful for reproducibility / parameter tweaks.
  - Move it out of the shipped npm package layout into a `scripts/` dir at the
    repo root.
  - Delete it if we don't need to regenerate.

## Widget correctness — must-fix before PR

- [x] **WebGL resource leak on unmount.** `BlochRenderer` now exposes
      `dispose()`, called from the React `useEffect` cleanup. It cancels
      any in-flight animation frame, disposes the OrbitControls, walks
      the scene disposing every geometry/material/map, releases label
      sprite textures, and calls `WebGLRenderer.dispose()`. The
      `themeObserver.detectThemeChange` helper was also updated to
      return a disposer; the widget calls it on unmount so the
      `MutationObserver` goes away too.
- [x] **Theme sensitivity.** Replaced the dead `isLight` block with
      `lightThemeColors` / `darkThemeColors` palettes selected via
      `colorsFor(isDark)`. `BlochRenderer` takes `isDark` in its
      constructor and exposes `setTheme(isDark)` which mutates sphere /
      marker / line materials and directional light in place and
      regenerates the canvas-textured label sprites (text color is
      baked into the canvas, so swap is cheapest). The React component
      uses `ensureTheme()` for the initial value and
      `detectThemeChange(document.body, r.setTheme)` for live switches.
      The history pane swapped its hard-coded `background: #eee` for a
      new `.qs-bloch-history` CSS class pulling
      `var(--qdk-background-accent)` / `var(--qdk-host-foreground)` /
      `var(--qdk-widget-outline)` from the shared QDK theme tokens.
- [x] **`document.getElementById("run_gates" | "rz_button")`.** Replaced
      with a single `useRef<HTMLInputElement>` for the Run textbox (the
      slider also writes into it, so a real handle is needed) and
      eliminated the `rz_button` lookup entirely by deriving the label
      straight from `rzAngle` state in JSX (`Rz({rzAngle})`). The
      widget is now self-contained: two instances on a page can't
      collide, and an unrelated element on the host page sharing the
      old ids can't hijack our state. Type-check, eslint, prettier,
      and all 33 unit tests still pass.
- [x] **Validate `?gates` URL input.** Extracted `VALID_GATE_CODES`
      (`"XYZHSsTt"`), `MAX_GATE_SEQUENCE_LENGTH` (256), and
      `sanitizeGateSequence()` into a tiny standalone
      `ux/blochGates.ts` module so it has no three.js / preact / JSON
      dependencies and can be unit-tested directly under Node. Both the
      URL-replay path on mount and the in-widget Run textbox now route
      through the sanitizer; the URL path logs a single `console.warn`
      naming the valid codes when anything is dropped, and the textbox
      silently filters. Added 7 unit tests covering pass-through,
      falsy input, unknown-char stripping, S/s and T/t case
      preservation, length cap, mixed filter+cap modification flag,
      and the `VALID_GATE_CODES` constant.
- [x] **Dead code.** Removed the top-of-file
      `/* eslint-disable @typescript-eslint/no-unused-vars */` and the
      two real unused vars it was masking (`Vector3` import, `e: Event`
      parameter on `applyGates`). Removed `fontMap` / `weightMap`
      leftovers from the `FontLoader` path. Replaced the long stale TODO
      comment block with a short note pointing readers to this file;
      kept the basis-coefficient → Bloch-angle math note since it's the
      only piece worth inline.

## Widget UX — should-fix

- [ ] **Text input + Run is awkward.** Placeholder says
      "Enter gates then tab away" but tab doesn't trigger anything; Enter
      doesn't submit (no `<form>`); the input doesn't clear after Run, so a
      second click re-applies the same gates.
- [ ] **Rz slider is indirect.** Moving the slider populates the text box
      with a pre-baked gate string; the user has to then click Run. People
      reasonably expect the slider to rotate the sphere directly. Either
      auto-apply on input or rename the control so the two-step contract
      is obvious.
- [ ] **Accessibility.** Gate buttons label themselves only with the symbol
      ("X", "S†"); add `aria-label` like "Apply Pauli-X gate". The
      `<canvas>` has no `role="img"` / `aria-label`. Slider should say
      "Rz rotation angle in radians", not just "Rz". Tooltips on each gate
      button (the matrices are already in `gateLaTeX`) would help sighted
      learners too.
- [ ] **History pane layout breaks in the VS Code webview.** It's
      `position: absolute; left: 600px; min-width: 200px; height: 700px` —
      narrow webviews / phone-sized playground windows clip or hide it.
      Switch to a normal flex layout.
- [ ] **Empty state.** First-time users see a blank gray rectangle next to
      the sphere with no hint that gates produce history. Add a placeholder
      line.
- [x] **Time-travel history with undo/redo.** The history pane now has a
      sticky "History" title bar and each row is clickable to navigate the
      sphere to that point in the sequence. Two distinct interaction modes:
  - **Edit mode** (cursor at end of sequence): `Undo` and `Redo` buttons
    are enabled. `Undo` pops the last gate onto a redo stack; `Redo`
    re-applies it. Applying any new gate clears the redo stack.
  - **Inspect mode** (cursor inside the sequence): a banner appears
    explaining that future steps will be discarded if a new gate is
    applied, with a "Jump to latest" escape hatch. Future rows are
    rendered dimmed + italic so the user can see what's at risk. `Undo`
    and `Redo` are disabled with tooltips pointing the user back to the
    latest step.

    Single source of truth: `gates: string[]` plus `cursor` plus
    `redoStack`. All visible state (sphere position, LaTeX history
    rows, button enablement) is derived. The renderer gained a
    `snapTo(steps)` primitive that cancels in-flight animations and
    jumps directly to a state by walking the existing `rotations`
    model — no replay, no trail noise. Gate metadata (matrix, LaTeX,
    rotation axis, rotation angle) was consolidated into a single
    `gateInfo` table that drives both the math and the renderer,
    eliminating three parallel switch statements.

## Widget — nice-to-have

- [ ] **WebGL fallback.** If `WebGLRenderer` construction throws (no GPU /
      headless Codespaces / browser disables WebGL), the whole widget
      vanishes silently. Add a `try/catch` and surface a
      "WebGL not available" message.
- [ ] **Show current state succinctly.** The state vector only appears in
      the gate history. A small fixed pane showing the current $|\psi\rangle$
      and Bloch angles (θ, φ) would be more useful than scrolling history
      to find the last line.
- [ ] **Localization.** Hard-coded English everywhere. The VS Code
      extension has `vscode.l10n.t(...)`; webview-side strings can be
      threaded through too.

## Integration polish

- [x] **Decide where the Bloch view belongs in the playground nav.** Moved
      under a new "Tools" `nav-1` header between Samples and Tutorials, styled
      as a `nav-2 nav-selectable` entry with `nav-current` highlighting like
      every other nav item.
- [x] **Replace the `#bloch` URL-hash hack with a proper deep-link.** Switched
      to `?view=bloch` (matches the existing `?code=` pattern). The URL is
      kept in sync with the nav selection via `history.pushState`, and a small
      share-link icon in the corner of the Bloch view copies the current URL
      to the clipboard (mirrors the editor's `onGetLink` behavior).
- [x] **Fix render bug** where `<BlochSphere />` was showing on top of every
      Documentation namespace because of a missing conditional branch in
      `App`. Bloch now renders only when `currentNavItem === "bloch"`.
- [ ] **Consider linking it from a Q# state-result, not just as a standalone
      page.** The widget today is a sandboxed gate-toy. The real product win would
      be wiring it to actual simulator state so users can see the Bloch vector
      after running their program.

## Testing

- [ ] **Add a VS Code integration test for the `qsharp-vscode.showBloch`
      command.** Today only unit tests for `cplx`/rotation math run; the webview
      command path is untested.
- [ ] Smoke-test the playground page in a real browser (not just the VS Code
      Simple Browser) to confirm interactive gate buttons and the Rz slider
      behave as expected.

## Compliance

- [ ] **Check `cgmanifest.json` / 3rd-party-license requirements** for three.js
      if we end up shipping it inside the VS Code extension.

## Cleanup

- [ ] Decide whether to keep the `BLOCH_TODO.md` file in the repo or move it
      to an internal tracking system before opening the PR.

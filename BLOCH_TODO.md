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

      _Update:_ mostly resolved. The textbox is now a live draft synced to
      `gates` (typing replaces the sequence on Enter or Run click), the
      placeholder is `"Type gates here (X Y Z H S s T t), Enter to run"`,
      Enter commits via `onKeyDown`, Esc discards the unsaved draft, and
      `sanitizeGateSequence()` runs on every commit. Still open: decide
      whether to clear the box on Run (currently it keeps the sequence so
      the user can see what was committed), and confirm Tab semantics are
      acceptable.

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
  - **Inspect mode** (cursor inside the sequence): future rows are
    rendered dimmed so the user can see what's at risk if they apply a
    new gate, and `Undo` / `Redo` are disabled with tooltips pointing
    the user back to the latest step. (The earlier explanatory banner
    - explicit "Jump to latest" button were dropped in favor of
      clicking the latest row, which proved less noisy in practice. The
      step counter in the title bar — e.g. `5 / 8` — communicates the
      same mode information.)

    Single source of truth: `gates: string[]` plus `cursor` plus
    `redoStack`. All visible state (sphere position, LaTeX history
    rows, button enablement) is derived. The renderer gained a
    `snapTo(steps)` primitive that cancels in-flight animations and
    jumps directly to a state by walking the existing `rotations`
    model — no replay, no trail noise. Gate metadata (matrix, LaTeX,
    rotation axis, rotation angle) was consolidated into a single
    `gateInfo` table that drives both the math and the renderer,
    eliminating three parallel switch statements.

## Polish landed since last update

These items aren't on the original TODO but were done as part of the same
push. Listed here so the file isn't silently out of date.

- [x] **Transport-style playback bar.** Jump-to-start / step-back /
      play-pause-replay / step-forward / jump-to-end glyph buttons under
      the sphere, plus an animation-speed range slider with a `1.00×`
      readout. Replaces the `play` text button; works in both edit and
      inspect mode. Replay glyph swaps in automatically when the cursor
      is at the end of the sequence.
- [x] **Live gate-string textbox.** Editing the textbox builds a draft;
      Enter / Run commits via `sanitizeGateSequence`, Esc discards.
      A small "unsaved changes" indicator + character count sit under
      the input. Run is disabled when there are no committed gates and
      nothing to play.
- [x] **Gate breakdown chips.** Under the textbox we show one chip per
      distinct gate code with a count, plus a "T-count: N" chip so
      learners can see the synthesis cost of their sequence at a glance.
- [x] **History pane reshape.** Step counter (`5 / 8`) moved into the
      sticky title bar; latest row pinned to the bottom of the scroll
      area (sticky positioning) so it stays visible during long
      sequences; future-row dim styling scoped to children so the
      sticky row itself doesn't fade out.
- [x] **Themed form controls.** Buttons, text inputs, and range sliders
      scoped to `.qs-bloch*` selectors now pull from the QDK theme
      tokens (`--qdk-background-accent`, `--qdk-host-foreground`,
      `--qdk-widget-outline`, `accent-color`) so the widget matches
      both VS Code light/dark themes and the playground palette.
- [x] **URL-load starts at step 0.** When the widget is opened with
      `?gates=...`, the sequence is loaded into `gates` but the cursor
      stays at 0 (initial |0⟩ state, inspect mode). Previously the
      sphere jumped straight to the final state, hiding the
      step-by-step story the URL was meant to share.
- [x] **Bloch angle overlay (θ, φ).** See "Show current state
      succinctly" above.

## Widget — nice-to-have

- [ ] **WebGL fallback.** If `WebGLRenderer` construction throws (no GPU /
      headless Codespaces / browser disables WebGL), the whole widget
      vanishes silently. Add a `try/catch` and surface a
      "WebGL not available" message.
- [ ] **Show current state succinctly.** The state vector only appears in
      the gate history. A small fixed pane showing the current $|\psi\rangle$
      and Bloch angles (θ, φ) would be more useful than scrolling history
      to find the last line.

      _Update:_ Bloch angles half done. A `θ` / `φ` overlay now sits in
      the top-left corner of the sphere canvas (`.qs-bloch-coords`),
      derived from `gates.slice(0, cursor)` via a throwaway `Rotations`
      instance so it can't drift from the renderer. Updates per discrete
      step during playback (not per animation frame); shows `n/a` for
      `φ` at the poles where it's undefined. Still open: a fixed
      $|\psi\rangle$ readout (basis-coefficient form), separate from
      the scrolling history.

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

- [x] **Check `cgmanifest.json` / 3rd-party-license requirements** for three.js
      if we end up shipping it inside the VS Code extension.

      _Resolved:_ Added an `npm` registration for `three@0.184.0` (MIT) to
      `cgmanifest.json`. `@types/three` is dev-only (stripped at build) and
      doesn't need a runtime registration. No project-level NOTICE /
      ThirdPartyNotices file exists in this repo \u2014 Component Governance
      generates downstream notices from the manifest.

## Cleanup

- [ ] Decide whether to keep the `BLOCH_TODO.md` file in the repo or move it
      to an internal tracking system before opening the PR.

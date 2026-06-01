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

- [ ] **WebGL resource leak on unmount.** `BlochRenderer` is created in a
      `useEffect` with no cleanup. Navigating away from the Bloch view (or
      closing/reopening the VS Code webview) leaves the `WebGLRenderer`,
      `OrbitControls`, geometries, materials, textures, and live
      `requestAnimationFrame` loop alive. Browsers cap concurrent WebGL
      contexts (~8–16); enough navigation triggers
      "Too many active WebGL contexts". Need `cancelAnimationFrame`,
      `renderer.dispose()`, `controls.dispose()`, and a scene traversal
      disposing geometries/materials/textures.
- [ ] **Theme sensitivity.** `isLight` is computed from
      `data-vscode-theme-kind` and never read. Sphere color, label color
      (`#606080` baked into the canvas texture), and the history pane
      background (`#eee`) are identical in every theme. In VS Code dark
      themes the labels are nearly invisible and the white history pane
      blares. Branch on `isLight` for label color, history pane background,
      and probably the sphere emissive color; watch the attribute via a
      `MutationObserver` so live theme switches are picked up.
- [ ] **`document.getElementById("run_gates" | "rz_button")`.** Two Bloch
      widgets on a page would collide, and any external collision silently
      hijacks our state. Replace with refs to a self-contained subtree.
- [ ] **Validate `?gates` URL input.** A malicious/stale link with
      `?gates=AAAAAAAA…` (10k chars) will `console.error` per char _and_
      push 10k animations onto the queue. Filter to the known gate-code
      whitelist (`X Y Z H S s T t`) and cap length.
- [ ] **Dead code.** Remove `fontMap` / `weightMap` leftovers from the
      deleted `FontLoader` path. Remove the top-of-file
      `/* eslint-disable @typescript-eslint/no-unused-vars */` once the
      genuine unused vars are gone. Prune stale TODO comments at the top
      that refer to dropped features.

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
- [ ] **Undo / step-back.** Only `Reset` exists today (nuke everything).
      A simple "Undo last gate" is much cheaper than the full replay
      slider mentioned in the top-of-file TODO and would meaningfully
      improve usability.

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

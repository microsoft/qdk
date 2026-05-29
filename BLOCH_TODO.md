# Bloch Sphere Port — Follow-up TODO

Tracking ideas to explore before opening a PR to bring the Bloch sphere widget
into the main product. Items are not ordered by priority.

## Dependencies

- [ ] **Update `three.js` to a current release.** The original branch pinned
      `three@^0.161.0` / `@types/three@^0.161.2` (Feb 2024). Current is ~0.171.
      Verify the widget still builds and renders correctly after the bump.

## Asset / bundling cleanup

- [ ] **Drop the playground font assets and the `FontLoader` code path.**
      Today we ship ~125 KB of helvetiker JSON fonts under
      `source/playground/public/fonts/` just to render "x"/"y"/"z" axis labels as
      3D extruded text. The runtime fetch silently 404s in the VS Code webview, so
      labels don't even render there. Replace with one of:
  - `Sprite` + `CanvasTexture` (render "x"/"y"/"z" into an offscreen canvas
    once, wrap as a sprite). ~10 lines, no asset, no fetch, no third-party
    font license to track. Recommended.
  - Plain HTML overlay positioned from projected 3D coords. Cleaner DOM story
    but more layout work.
  - Drop axis labels entirely (many canonical Bloch sphere renderings only
    label the poles `|0⟩` / `|1⟩`).
- [ ] Remove the unused `helvetiker_bold.typeface.json` either way (dead
      weight from the original branch — only `_regular` is actually loaded).

## Code organization

- [ ] **Decide the fate of `source/npm/qsharp/tools/rz-synthesis.ts`.**
      It's a one-shot Node script that regenerates `rz-array.json` /
      `rz-details.json` via brute-force gate-sequence search. The JSON outputs
      are checked in, so it's not on any build critical path. Options:
  - Keep it (current state) — useful for reproducibility / parameter tweaks.
  - Move it out of the shipped npm package layout into a `scripts/` dir at the
    repo root.
  - Delete it if we don't need to regenerate.

## Integration polish

- [ ] **Decide where the Bloch view belongs in the playground nav.** Currently
      added as a top-level "Bloch sphere" link via the `#bloch` URL hash hack in
      [`source/playground/src/main.tsx`](source/playground/src/main.tsx) and
      [`source/playground/src/nav.tsx`](source/playground/src/nav.tsx). Probably
      wants to be a proper route, or grouped under "Tools" / "Visualizations".
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

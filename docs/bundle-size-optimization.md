# Bundle Size Optimization — Bloch Sphere PR

## Background

The VS Code extension ships several JavaScript bundles into its webviews.
The main one, `webview.js`, is loaded eagerly for **every** Q# output panel —
histograms, circuit diagrams, resource estimates, documentation, and so on.

When the Bloch sphere widget was added it brought in
[three.js](https://threejs.org/) (the 3D rendering library) as a dependency.
Because of how the TypeScript barrel export worked, `three.js` was being
bundled into `webview.js` even for panels that never show a Bloch sphere.
This caused `webview.js` to grow from ~3 MB to ~4.4 MB — a concern raised
during review.

A second library, [3Dmol](https://3dmol.org/) (used by the chemistry
`MoleculeViewer` widget), had the same problem and was also being bundled
into `webview.js` despite never being used in the VS Code webview at all.

---

## What Changed

### 1. The root cause: static barrel exports

`qsharp-lang/ux` has a single barrel file (`ux/index.ts`) that re-exports
everything:

```ts
// Before
export { BlochSphere } from "./bloch.js"; // pulls in three.js
export { MoleculeViewer } from "./chem/index.js"; // pulls in 3Dmol
```

Both `webview.tsx` (all panels) and `editor.tsx` (circuit editor) import
from this barrel. Even though neither file actually uses `BlochSphere` or
`MoleculeViewer` at runtime in most cases, esbuild must include the full
module graph of every re-export — including the heavy 3D libraries — because
it cannot know at build time which exports will be used.

### 2. The fix: move heavy exports to dedicated subpath entries

Both `BlochSphere` and `MoleculeViewer` were removed from the main barrel
and placed behind their own package export subpaths:

```jsonc
// qsharp-lang/package.json
"exports": {
  "./ux":        "./ux/index.ts",       // main barrel — no longer has 3D libs
  "./ux/bloch":  "./ux/bloch.tsx",      // three.js lives here
  "./ux/chem":   "./ux/chem/index.tsx", // 3Dmol lives here
}
```

Consumers that actually need those widgets import from the subpath directly
(`qsharp-lang/ux/bloch`, `qsharp-lang/ux/chem`).

### 3. `BlochSphere` is now lazy-loaded in the webview

`webview.tsx` was changed to use a **dynamic import** (Preact `lazy` +
`Suspense`) for the Bloch sphere:

```tsx
// Before — three.js pulled into the initial bundle
import { BlochSphere } from "qsharp-lang/ux";

// After — three.js only loaded when a Bloch panel is actually opened
const BlochSphere = lazy(() =>
  import("qsharp-lang/ux/bloch").then((m) => ({ default: m.BlochSphere })),
);
```

### 4. esbuild code-splitting for `webview.tsx`

For the dynamic import to actually produce a separate file, esbuild's
`splitting` feature must be enabled. The VS Code build (`vscode/build.mjs`)
was updated so that `webview.tsx` is built as an ES module with splitting
turned on:

```js
// vscode/build.mjs — new "webview" build target
{
  format: "esm",
  splitting: true,
  entryPoints: ["src/webview/webview.tsx"],
  chunkNames: "webview/chunks/[name]-[hash]",
  ...
}
```

The other two entry points (`editor.tsx` and the learning webview client)
continue to be built as CommonJS modules — no change to their behaviour.

### 5. `<script type="module">` in the webview HTML

Because `webview.js` is now an ES module, the `<script>` tag in the
extension's webview HTML was updated from:

```html
<script src="${webviewJs}"></script>
```

to:

```html
<script type="module" src="${webviewJs}"></script>
```

---

## Results

All sizes are **unminified** — the extension build does not currently minify
its output.

| File                                   | Before  | After       | Change                               |
| -------------------------------------- | ------- | ----------- | ------------------------------------ |
| `webview.js` (loaded for every panel)  | ~4.4 MB | **1.2 MB**  | −3.2 MB (−73%)                       |
| `editor.js` (circuit editor)           | ~2.1 MB | **0.27 MB** | −1.8 MB (−87%)                       |
| `chunks/bloch-*.js` _(new — lazy)_     | —       | 1.35 MB     | loaded only when a Bloch panel opens |
| `chunks/chunk-*.js` _(shared — eager)_ | —       | 0.04 MB     | small shared utilities               |

The Bloch sphere and its 3D engine (1.35 MB) are now **only downloaded when
a user actually opens a Bloch sphere panel**, not on every extension startup.

---

## How to Measure This Yourself

After running the build:

```powershell
python .\build.py --no-check --no-test --npm --vscode
```

List the output files and their sizes:

```powershell
Get-ChildItem -Recurse source\vscode\out\webview -Filter "*.js" |
  Select-Object Name, @{n='KB';e={[math]::Round($_.Length/1KB)}} |
  Sort-Object KB -Descending
```

To verify a library is absent from a bundle, search for a symbol it exports:

```powershell
# Should return 0 — three.js no longer in the main webview bundle
(Select-String source\vscode\out\webview\webview.js -Pattern 'WebGLRenderer').Count

# Should return 0 — 3Dmol no longer in the main webview bundle
(Select-String source\vscode\out\webview\webview.js -Pattern 'createViewer').Count
```

---

## Files Changed

| File                                    | Change                                                                |
| --------------------------------------- | --------------------------------------------------------------------- |
| `source/npm/qsharp/ux/index.ts`         | Removed `BlochSphere` and `MoleculeViewer` re-exports                 |
| `source/npm/qsharp/package.json`        | Added `./ux/bloch` and `./ux/chem` subpath exports                    |
| `source/vscode/build.mjs`               | Split `webview.tsx` into a separate ESM + code-splitting build target |
| `source/vscode/src/webview/webview.tsx` | Dynamic `import()` + `lazy`/`Suspense` for `BlochSphere`              |
| `source/vscode/src/webviewPanel.ts`     | `<script type="module">` to load the ESM bundle                       |
| `source/playground/src/main.tsx`        | Import `BlochSphere` from `qsharp-lang/ux/bloch` subpath              |
| `source/widgets/js/index.tsx`           | Import `MoleculeViewer` from `qsharp-lang/ux/chem` subpath            |

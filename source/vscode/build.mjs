// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import { copyFileSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build as esbuildBuild, context } from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));
const libsDir = join(thisDir, "..", "..", "node_modules");

// ── Shared esbuild options ──────────────────────────────────────────

/** @type {import("esbuild").BuildOptions} */
const commonBuildOptions = {
  bundle: true,
  external: ["vscode"],
  format: "cjs",
  target: ["es2022"],
  sourcemap: "linked",
};

// ── Per-platform build options ──────────────────────────────────────

/** @type {Record<string, import("esbuild").BuildOptions>} */
const platformBuildOptions = {
  ui: {
    ...commonBuildOptions,
    platform: "browser",
    outbase: join(thisDir, "src"),
    outdir: join(thisDir, "out"),
    entryPoints: [
      join(thisDir, "src", "webview/webview.tsx"),
      join(thisDir, "src", "webview/editor.tsx"),
      join(thisDir, "src", "webview/bloch.tsx"),
      join(thisDir, "src", "learning/webview/webview-client.tsx"),
    ],
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("browser"),
    },
    // plugins added at build time (needs inlineStateComputeWorkerPlugin)
  },
  browser: {
    ...commonBuildOptions,
    entryPoints: [
      join(thisDir, "src", "extension.ts"),
      join(thisDir, "src", "compilerWorker.ts"),
      join(thisDir, "src", "debugger/debug-service-worker.ts"),
    ],
    platform: "browser",
    outdir: join(thisDir, "out", "browser"),
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("browser"),
    },
  },
  node: {
    ...commonBuildOptions,
    platform: "node",
    outdir: join(thisDir, "out", "node"),
    entryPoints: [join(thisDir, "src", "extension.ts")],
    external: ["vscode"],
    banner: {
      js: 'const _importMetaUrl = require("url").pathToFileURL(__filename).href;',
    },
    define: {
      "import.meta.url": "_importMetaUrl",
      __PLATFORM__: JSON.stringify("node"),
    },
  },
  "node-worker": {
    ...commonBuildOptions,
    platform: "node",
    outdir: join(thisDir, "out", "node"),
    entryPoints: [
      join(thisDir, "src", "compilerWorker.ts"),
      join(thisDir, "src", "debugger/debug-service-worker.ts"),
    ],
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("node"),
    },
  },
  renderer: {
    ...commonBuildOptions,
    external: [],
    platform: "browser",
    format: "esm",
    entryPoints: [join(thisDir, "src", "notebookRenderer", "index.ts")],
    outfile: join(thisDir, "out", "renderer", "qdkLearning.js"),
    // A notebook renderer is loaded as a single JS module — VS Code won't pick
    // up a sibling stylesheet — so CSS is bundled as text and injected at
    // activation instead of emitted as a separate file.
    loader: { ".css": "text" },
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("browser"),
    },
  },
};

// ── Inline worker plugin ────────────────────────────────────────────

/** @type {import("esbuild").Plugin} */
const inlineStateComputeWorkerPlugin = {
  name: "Inline State Compute Worker",
  setup(builder) {
    builder.onResolve({ filter: /stateComputeWorker.inline\.ts$/ }, (args) => ({
      path: join(args.resolveDir, args.path),
      namespace: "inline-state-compute-worker",
    }));

    builder.onLoad(
      { filter: /.*/, namespace: "inline-state-compute-worker" },
      async () => {
        const workerEntry = join(
          thisDir,
          "src",
          "webview",
          "stateComputeWorker.ts",
        );

        const result = await esbuildBuild({
          ...commonBuildOptions,
          entryPoints: [workerEntry],
          bundle: true,
          write: false,
          platform: "browser",
          format: "iife",
          sourcemap: false,
          logLevel: "silent",
        });

        const workerSource = result.outputFiles?.[0]?.text ?? "";
        return {
          contents: `const workerSource = ${JSON.stringify(workerSource)};\nexport default workerSource;\n`,
          loader: "ts",
        };
      },
    );
  },
};

// ── Renderer/emitter contract check ─────────────────────────────────

/**
 * Fail the build if the values this feature writes in one language and reads in
 * another have drifted.
 *
 * The payload contract is written twice — TypeScript types the renderer
 * validates against, and the dicts `_learning_output.py` builds — and nothing
 * in either type system spans that gap. Checks the values whose disagreement
 * breaks a learner: MIME type, payload kinds, schema version, the field names
 * the renderer reads, the manifest contribution VS Code routes on, and the cell
 * tag that keeps a quiz out of the progress tree.
 */
export function checkRendererContract() {
  const schemaPath = join(thisDir, "src", "notebookRenderer", "schema.ts");
  const rendererPath = join(
    thisDir,
    "src",
    "notebookRenderer",
    "multipleChoice.ts",
  );
  const emitterPath = join(
    thisDir,
    "resources",
    "qdk-learning",
    "courses",
    "chemistry-qpe",
    "_learning_output.py",
  );

  const schema = readFileSync(schemaPath, "utf8");
  const renderer = readFileSync(rendererPath, "utf8");
  const emitter = readFileSync(emitterPath, "utf8");

  const mismatches = [];
  const required = (label, value) => {
    if (value === undefined) {
      throw new Error(`Could not read ${label} while checking the contract.`);
    }
    return value;
  };

  const tsMime = required(
    "MIME_TYPE in schema.ts",
    /^export const MIME_TYPE = "([^"]+)"/m.exec(schema)?.[1],
  );
  const pyMime = required(
    "MIME_TYPE in _learning_output.py",
    /^MIME_TYPE = "([^"]+)"/m.exec(emitter)?.[1],
  );
  if (tsMime !== pyMime) {
    mismatches.push(`MIME type differs: "${tsMime}" vs "${pyMime}".`);
  }

  // The third copy, and the one that decides whether any of this runs: VS Code
  // picks a renderer by matching an output's MIME type against this
  // contribution, and routes `createRendererMessaging` by its id. If either
  // drifts the two copies above still agree, so the check stays green while
  // the renderer is never selected, or its "Why is that wrong?" button posts
  // into a channel nothing is listening on.
  const rendererId = required(
    "RENDERER_ID in schema.ts",
    /^export const RENDERER_ID = "([^"]+)"/m.exec(schema)?.[1],
  );
  const manifestPath = join(thisDir, "package.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const contributions = required(
    "the notebookRenderer contributions in package.json",
    manifest.contributes?.notebookRenderer,
  );
  const contribution = contributions.find((r) => r.id === rendererId);
  if (contribution === undefined) {
    mismatches.push(
      `package.json contributes no notebook renderer with id "${rendererId}", ` +
        `only: ${contributions.map((r) => r.id).join(", ") || "none"}.`,
    );
  } else {
    if (!contribution.mimeTypes?.includes(tsMime)) {
      mismatches.push(
        `package.json contributes [${(contribution.mimeTypes ?? []).join(", ")}] ` +
          `but the renderer reads "${tsMime}".`,
      );
    }

    // Same failure mode from the other side: a renderer VS Code cannot load is
    // one the learner never sees. The entry point is written here and built by
    // `esbuild` into `outfile` below, and nothing else compares them.
    const rendererOut = relative(
      thisDir,
      required(
        "the renderer outfile in build.mjs",
        platformBuildOptions.renderer?.outfile,
      ),
    ).replaceAll("\\", "/");
    if (contribution.entrypoint !== `./${rendererOut}`) {
      mismatches.push(
        `package.json points at "${contribution.entrypoint}" but the build ` +
          `writes "./${rendererOut}".`,
      );
    }
  }

  // Every payload the emitter builds must name a kind the renderer handles.
  // Both sides go through `required()`: an empty list on either side would
  // otherwise make this comparison vacuous, and the banner below would still
  // report a kind count read from TypeScript alone.
  //
  // `SUPPORTED_KINDS` is the list `readPayload` actually gates on, so that is
  // what Python is compared against. The union in `schema.ts` only types the
  // payload: a kind declared there but missing from the runtime list compiles
  // cleanly and then refuses to render.
  const rendererIndex = readFileSync(
    join(thisDir, "src", "notebookRenderer", "index.ts"),
    "utf8",
  );
  const tsKinds = [
    ...required(
      "SUPPORTED_KINDS in index.ts",
      /const SUPPORTED_KINDS = \[([^\]]*)\]/.exec(rendererIndex)?.[1],
    ).matchAll(/"([a-z-]+)"/g),
  ].map((m) => m[1]);
  const schemaKinds = [...schema.matchAll(/^\s+kind: "([a-z-]+)";/gm)].map(
    (m) => m[1],
  );
  const pyKinds = [...emitter.matchAll(/"kind": "([a-z-]+)"/g)].map(
    (m) => m[1],
  );
  required("a payload kind in SUPPORTED_KINDS", tsKinds[0]);
  required("a payload kind in schema.ts", schemaKinds[0]);
  required('a "kind" in _learning_output.py', pyKinds[0]);
  const undeclared = schemaKinds.filter((k) => !tsKinds.includes(k));
  if (undeclared.length > 0) {
    mismatches.push(
      `schema.ts declares kinds the renderer never accepts: ${[...new Set(undeclared)].join(", ")}.`,
    );
  }
  const unknown = pyKinds.filter((k) => !tsKinds.includes(k));
  if (unknown.length > 0) {
    mismatches.push(
      `Python emits kinds the renderer does not handle: ${[...new Set(unknown)].join(", ")}.`,
    );
  }

  const tsVersion = required(
    "SUPPORTED_SCHEMA_VERSION",
    /const SUPPORTED_SCHEMA_VERSION = (\d+)/.exec(rendererIndex)?.[1],
  );
  // The third version copy: the literal type payloads are cast to. Nothing in
  // the compiler ties it to the constant `readPayload` compares against.
  const schemaVersion = required(
    "schemaVersion in schema.ts",
    /^\s+schemaVersion: (\d+);/m.exec(schema)?.[1],
  );
  if (tsVersion !== schemaVersion) {
    mismatches.push(
      `Schema version differs: renderer accepts ${tsVersion}, schema.ts types it as ${schemaVersion}.`,
    );
  }
  const pyVersion = required(
    '"schemaVersion" in _learning_output.py',
    /"schemaVersion": (\d+)/.exec(emitter)?.[1],
  );
  if (tsVersion !== pyVersion) {
    mismatches.push(
      `Schema version differs: renderer accepts ${tsVersion}, emitter writes ${pyVersion}.`,
    );
  }

  // Field names the renderer reads off a multiple-choice payload. Renaming one
  // on either side leaves the question blank rather than failing loudly — or,
  // for `multiSelect`, silently builds a radio group for a question with
  // several correct answers, which then cannot be answered at all.
  //
  // The emitter writes most fields as dict literal keys (`"prompt": ...`) but
  // sets optional ones by assignment (`payload["multiSelect"] = True`), so the
  // Python probe has to accept both spellings. The TypeScript side is searched
  // across both files that touch a payload: the validator and the view read
  // different fields, and looking at only one would let a field go unchecked
  // the moment it moved between them.
  const rendererSources = `${renderer}\n${rendererIndex}`;
  const payloadFields = ["prompt", "options", "multiSelect", "payloadId"];
  const optionFields = ["id", "text", "correct", "explanation"];
  for (const field of payloadFields) {
    const inTs = new RegExp(`payload\\.${field}\\b`).test(rendererSources);
    const inPy = new RegExp(`"${field}"\\s*(?::|\\])`).test(emitter);
    if (inTs !== inPy) {
      mismatches.push(
        `Payload field "${field}" is ${inTs ? "read by the renderer but never written by the emitter" : "written by the emitter but never read by the renderer"}.`,
      );
    }
  }
  for (const field of optionFields) {
    const inTs = new RegExp(`option\\.${field}\\b`).test(rendererSources);
    const inPy = new RegExp(`"${field}"`).test(emitter);
    if (inTs !== inPy) {
      mismatches.push(
        `Option field "${field}" is ${inTs ? "read by the renderer but never written by the emitter" : "written by the emitter but never read by the renderer"}.`,
      );
    }
  }

  // The converter writes this tag onto every quiz cell, and `notebookExercises`
  // reads it to keep quizzes out of the progress tree. If the two spellings
  // drift, a quiz cell falls through to the code-cell branch and becomes an
  // activity — one that can never complete, because a quiz is never run.
  const converter = readFileSync(
    join(
      thisDir,
      "resources",
      "qdk-learning",
      "utils",
      "chemistry-qpe",
      "details_to_quiz.py",
    ),
    "utf8",
  );
  const tsQuizTag = required(
    "QUIZ_TAG in notebookExercises.ts",
    /^const QUIZ_TAG = "([^"]+)";/m.exec(
      readFileSync(
        join(thisDir, "src", "learning", "notebookExercises.ts"),
        "utf8",
      ),
    )?.[1],
  );
  // The converter tags a quiz cell in two places — a bare list, and a merge
  // with the source cell's own tags when the quiz comes first — so every
  // writer is compared, not just the one that happens to be found first.
  const pyQuizTags = [
    ...converter.matchAll(
      /tags = \["([^"]+)"\]|_cell_tags\(cell\), "([^"]+)"/g,
    ),
  ].map((m) => m[1] ?? m[2]);
  required("a quiz cell tag in details_to_quiz.py", pyQuizTags[0]);
  const wrongTags = [...new Set(pyQuizTags.filter((t) => t !== tsQuizTag))];
  if (wrongTags.length > 0) {
    mismatches.push(
      `Quiz cell tag differs: the converter writes ${wrongTags
        .map((t) => `"${t}"`)
        .join(" and ")} but notebookExercises.ts looks for "${tsQuizTag}".`,
    );
  }

  // The id grammar is written twice, and `_ID_RE` even carries a comment
  // telling the reader to keep it in step with this one. Drift is silent on
  // both sides: the author's cell still runs, and the learner's action quietly
  // degrades to the generic prompt because the host drops a malformed id.
  const tsIdPattern = required(
    "ID_PATTERN in schema.ts",
    /^const ID_PATTERN = \/(.+)\/;/m.exec(schema)?.[1],
  );
  const pyIdPattern = required(
    "_ID_RE in _learning_output.py",
    /^_ID_RE = re\.compile\(r"(.+)"\)/m.exec(emitter)?.[1],
  );
  // Python spells the anchors `\A`/`\Z`; JavaScript uses `^`/`$`. Comparing the
  // body between them is what says the two accept the same ids.
  const pyIdBody = pyIdPattern.replace(/^\\A/, "").replace(/\\Z$/, "");
  const tsIdBody = tsIdPattern.replace(/^\^/, "").replace(/\$$/, "");
  if (tsIdBody !== pyIdBody) {
    mismatches.push(
      `Id grammar differs: schema.ts accepts /${tsIdBody}/ but ` +
        `_learning_output.py accepts /${pyIdBody}/.`,
    );
  }

  // The option cap is the same number twice: the host drops an action naming
  // more ids than `MAX_OPTION_IDS`, and the emitter refuses to build a question
  // with more options than that so the mistake lands on the author. If they
  // drift apart, a question renders and its action quietly stops working.
  const tsMaxOptions = required(
    "MAX_OPTION_IDS in schema.ts",
    /^const MAX_OPTION_IDS = (\d+);/m.exec(schema)?.[1],
  );
  const pyMaxOptions = required(
    "_MAX_OPTIONS in _learning_output.py",
    /^_MAX_OPTIONS = (\d+)/m.exec(emitter)?.[1],
  );
  if (tsMaxOptions !== pyMaxOptions) {
    mismatches.push(
      `Option cap differs: the host accepts ${tsMaxOptions} option ids but ` +
        `the emitter allows ${pyMaxOptions} options.`,
    );
  }

  if (mismatches.length > 0) {
    throw new Error(
      `QDK learning renderer contract mismatch:\n  - ${mismatches.join("\n  - ")}\n` +
        `Update both ${schemaPath} and ${emitterPath} together.`,
    );
  }

  const kinds = new Set(tsKinds).size;
  console.log(
    `Renderer contract OK (v${tsVersion}, ${kinds} payload kind${kinds === 1 ? "" : "s"}, ` +
      `${payloadFields.length + optionFields.length} fields).`,
  );
}

// ── Asset copy helpers ──────────────────────────────────────────────

export function copyWasmToVsCode() {
  const qsharpWasm = join(
    thisDir,
    "..",
    "npm",
    "qsharp",
    "lib",
    "web",
    "qsc_wasm_bg.wasm",
  );
  const qsharpDest = join(thisDir, "wasm");

  console.log("Copying the wasm file to VS Code from: " + qsharpWasm);
  console.log("Destination: " + qsharpDest);
  mkdirSync(qsharpDest, { recursive: true });
  copyFileSync(qsharpWasm, join(qsharpDest, "qsc_wasm_bg.wasm"));
}

/** @param {string} [destDir] */
export function copyKatex(destDir) {
  const katexBase = join(libsDir, "katex/dist");
  const katexDest = destDir ?? join(thisDir, "out/katex");
  const fontsDir = join(katexBase, "fonts");
  const fontsOutDir = join(katexDest, "fonts");

  console.log("Copying the Katex files over from: " + katexBase);
  mkdirSync(katexDest, { recursive: true });
  mkdirSync(fontsOutDir, { recursive: true });

  // katex
  copyFileSync(
    join(katexBase, "katex.min.css"),
    join(katexDest, "katex.min.css"),
  );

  // github markdown css
  copyFileSync(
    join(libsDir, "github-markdown-css/github-markdown-light.css"),
    join(katexDest, "github-markdown-light.css"),
  );
  copyFileSync(
    join(libsDir, "github-markdown-css/github-markdown-dark.css"),
    join(katexDest, "github-markdown-dark.css"),
  );

  // highlight.js css
  copyFileSync(
    join(libsDir, "highlight.js/styles/default.css"),
    join(katexDest, "hljs-light.css"),
  );
  copyFileSync(
    join(libsDir, "highlight.js/styles/dark.css"),
    join(katexDest, "hljs-dark.css"),
  );

  // vscode codicons
  copyFileSync(
    join(libsDir, "@vscode", "codicons", "dist", "codicon.css"),
    join(katexDest, "codicon.css"),
  );
  copyFileSync(
    join(libsDir, "@vscode", "codicons", "dist", "codicon.ttf"),
    join(katexDest, "codicon.ttf"),
  );

  // katex fonts
  for (const file of readdirSync(fontsDir)) {
    if (file.endsWith(".woff2")) {
      copyFileSync(join(fontsDir, file), join(fontsOutDir, file));
    }
  }
}

// ── Build functions ─────────────────────────────────────────────────

/** @param {string} platform */
async function buildPlatform(platform) {
  const options = platformBuildOptions[platform];
  if (!options) throw new Error(`Invalid platform: ${platform}`);

  // UI build needs the inline worker plugin
  if (platform === "ui") {
    options.plugins = [inlineStateComputeWorkerPlugin];
  }

  console.log(`Running esbuild for platform: ${platform}`);
  await esbuildBuild(options);
  console.log(`Built bundle to ${options.outdir ?? options.outfile}`);
}

function getTimeStr() {
  const now = new Date();
  const hh = now.getHours().toString().padStart(2, "0");
  const mm = now.getMinutes().toString().padStart(2, "0");
  const ss = now.getSeconds().toString().padStart(2, "0");
  const mil = now.getMilliseconds().toString().padStart(3, "0");
  return `${hh}:${mm}:${ss}.${mil}`;
}

// This only watches for platform = "browser" for the sake of simplicity,
// so make sure to run a full build first to catch any errors in the node
// build before pushing code changes.
export async function watchVsCode() {
  console.log("Building vscode extension in watch mode");

  /** @type {import("esbuild").Plugin} */
  const buildPlugin = {
    name: "Build Events",
    setup(build) {
      build.onStart(() =>
        console.log("VS Code build started @ " + getTimeStr()),
      );
      build.onEnd(() =>
        console.log("VS Code build complete @ " + getTimeStr()),
      );
    },
  };

  const ctx = await context({
    ...commonBuildOptions,
    entryPoints: [
      join(thisDir, "src", "extension.ts"),
      join(thisDir, "src", "compilerWorker.ts"),
      join(thisDir, "src", "debugger/debug-service-worker.ts"),
      join(thisDir, "src", "webview/webview.tsx"),
      join(thisDir, "src", "webview/editor.tsx"),
      join(thisDir, "src", "webview/bloch.tsx"),
    ],
    platform: "browser",
    outdir: join(thisDir, "out", "browser"),
    plugins: [inlineStateComputeWorkerPlugin, buildPlugin],
    color: false,
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("browser"),
    },
  });

  // The notebook renderer is a separate bundle with its own format and CSS
  // loader, so it needs its own watcher rather than another entry point above.
  const rendererCtx = await context({
    ...platformBuildOptions.renderer,
    plugins: [buildPlugin],
    color: false,
  });

  ctx.watch();
  rendererCtx.watch();
}

(async () => {
  const thisFilePath = resolve(fileURLToPath(import.meta.url));
  if (thisFilePath === resolve(process.argv[1])) {
    const isWatch = process.argv.includes("--watch");

    if (isWatch) {
      await watchVsCode();
    } else {
      copyKatex();
      copyWasmToVsCode();
      checkRendererContract();

      await Promise.all([
        buildPlatform("ui"),
        buildPlatform("browser"),
        buildPlatform("node"),
        buildPlatform("node-worker"),
        buildPlatform("renderer"),
      ]);
    }
  }
})();

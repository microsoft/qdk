// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

// Command-line arguments:
// --suite=<name>  Build only the specified test suite. The "empty" suite doesn't
//                 depend on any other build outputs, so it can be built on its own
//                 to download VS Code before anything else has been built.

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));

const suiteArgName = "--suite=";
const selectedSuite = process.argv
  .find((arg) => arg.startsWith(suiteArgName))
  ?.slice(suiteArgName.length);

/** @type {import("esbuild").BuildOptions} */
const commonBuildOptions = {
  bundle: true,
  external: ["vscode"],
  format: "cjs",
  target: ["es2022"],
  sourcemap: "linked",
  // Keep output paths (e.g. out/browser/empty/index.browser.js) stable when only some suites are built.
  outbase: join(thisDir, "suites"),
  //logLevel: "debug",
};

/** @type {Record<string, import("esbuild").BuildOptions>} */
const platformBuildOptions = {
  browser: {
    ...commonBuildOptions,
    entryPoints: [
      join(thisDir, "suites", "empty", "index.browser.ts"),
      join(thisDir, "suites", "language-service", "index.browser.ts"),
      join(thisDir, "suites", "debugger", "index.browser.ts"),
    ],
    platform: "browser",
    outdir: join(thisDir, "out", "browser"),
    define: { "import.meta.url": "undefined" },
  },
  node: {
    ...commonBuildOptions,
    entryPoints: [
      join(thisDir, "suites", "language-service", "index.node.ts"),
      join(thisDir, "suites", "debugger", "index.node.ts"),
    ],
    platform: "node",
    outdir: join(thisDir, "out", "node"),
    banner: {
      js: 'const _importMetaUrl = require("url").pathToFileURL(__filename).href;',
    },
    define: {
      "import.meta.url": "_importMetaUrl",
    },
  },
};

console.log("Running esbuild");

let builtAny = false;
for (const [platform, buildOptions] of Object.entries(platformBuildOptions)) {
  const entryPoints = /** @type {string[]} */ (buildOptions.entryPoints).filter(
    (entry) =>
      !selectedSuite ||
      dirname(entry) === join(thisDir, "suites", selectedSuite),
  );
  if (entryPoints.length === 0) {
    continue;
  }
  builtAny = true;
  build({ ...buildOptions, entryPoints }).then(() =>
    console.log(`Built tests for ${platform} to ${buildOptions.outdir}`),
  );
}

if (!builtAny) {
  console.error(`Error: Unknown test suite '${selectedSuite}'`);
  process.exit(1);
}

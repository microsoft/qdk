// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));

/** @type {import("esbuild").BuildOptions} */
const commonBuildOptions = {
  bundle: true,
  external: ["vscode"],
  format: "cjs",
  target: ["es2022"],
  sourcemap: "linked",
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

for (const [platform, buildOptions] of Object.entries(platformBuildOptions)) {
  build(buildOptions).then(() =>
    console.log(`Built tests for ${platform} to ${buildOptions.outdir}`),
  );
}

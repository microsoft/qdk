// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

// Adapted from the sample at
// https://github.com/microsoft/vscode-test-web/blob/3f0f858ab15cb65ef3c19564b0f5a6910ea9414e/sample/src/web/test/runTest.ts
//
// This script is run using Node.js in the dev environment. It will
// download the latest Insiders build of VS Code for the Web and launch
// it in a headless instance of Chromium to run the integration test suite.
//
// Command-line arguments:
// --suite=<name>           Run only the specified test suite (language-service or debugger)
// --waitForDebugger=<port> Wait for debugger to attach on the specified port before running tests
// --verbose                Enable verbose logging for VS Code and test web server
//                          Note: This controls the VS Code and test web server logging level.
//                          Q# extension logs are usually more relevant for debugging tests.
//                          To control the Q# extension log level see: suites/extensionUtils.ts

import { runTests } from "@vscode/test-web";
import { readFileSync } from "node:fs";
import { SourceMap } from "node:module";
import path, { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const attachArgName = "--waitForDebugger=";
const verboseArgName = "--verbose";
const suiteArgName = "--suite=";

const verbose = process.argv.includes(verboseArgName);
const waitForDebugger = process.argv.find((arg) =>
  arg.startsWith(attachArgName),
);
const selectedSuite = process.argv
  .find((arg) => arg.startsWith(suiteArgName))
  ?.slice(suiteArgName.length);

// If waitForDebugger is specified, a specific suite must also be specified
if (waitForDebugger && !selectedSuite) {
  console.error(
    "Error: When using --waitForDebugger, you must also specify a suite with --suite=<name>",
  );
  process.exit(1);
}

const thisDir = dirname(fileURLToPath(import.meta.url));
// The folder containing the Extension Manifest package.json
const extensionDevelopmentPath = join(thisDir, "..");

try {
  const suites = ["language-service", "debugger"];
  const toRun =
    selectedSuite && suites.includes(selectedSuite) ? [selectedSuite] : suites;

  for (const suite of toRun) {
    console.log(`Running suite: ${suite}`);
    await runSuite(suite);
  }
} catch {
  console.error("Test run failed.");
  process.exit(1);
}

async function runSuite(name) {
  const extensionTestsPath = join(thisDir, "out", name, "index");
  const workspacePath = join(thisDir, "suites", name, "test-workspace");

  // Capture console output before running tests,
  // so that we can map stack traces to original source files.
  const { restoreConsole } = interceptConsoleWithStackMapping();

  try {
    // Start a web server that serves VS Code in a browser, run the tests
    await runTests({
      headless: true, // pass false to see VS Code UI
      browserType: "chromium",
      extensionDevelopmentPath,
      extensionTestsPath,
      folderPath: workspacePath,
      quality: "stable",
      printServerLog: verbose,
      verbose,
      waitForDebugger: waitForDebugger
        ? Number(waitForDebugger.slice(attachArgName.length))
        : undefined,
    });
  } finally {
    restoreConsole();
  }
}

/**
 * Captures console output and maps any stack traces found to original source files.
 */
function interceptConsoleWithStackMapping() {
  const originalConsole = {
    error: console.error,
    info: console.info,
  };

  console.info = (...args) => {
    originalConsole.info(...args.map((arg) => mapSourcesIfStackTrace(arg)));
  };

  console.error = (...args) => {
    originalConsole.error(...args.map((arg) => mapSourcesIfStackTrace(arg)));
  };

  return {
    restoreConsole: () => {
      console.error = originalConsole.error;
      console.info = originalConsole.info;
    },
  };

  /**
   * Maps locations in the bundled .js to original .ts source file locations.
   */
  function mapSourcesIfStackTrace(stackStr) {
    if (typeof stackStr !== "string") {
      return stackStr;
    }

    const lines = stackStr.split("\n");
    for (let i = 0; i < lines.length; i++) {
      // e.g. "  at functionName (/path/to/file.js:123:45)"
      const linePat = /^(\s*)at\s+(?:(.*?)\s+\()?(.+?):(\d+):(\d+)\)?$/;
      const m = linePat.exec(lines[i]);
      if (m) {
        const [, ws, functionName, file, line, column] = m;
        const pos = mapSourceLine(file, +line, +column);
        if (pos) {
          const mapped = `${ws}at ${functionName || "<anonymous>"} (${pos.file}:${pos.line}:${pos.column})`;
          lines[i] = mapped;
        }
      }
    }

    return lines.join("\n");
  }

  /**
   * Maps a single source line using the source map.
   */
  function mapSourceLine(sourceUrl, line, col) {
    const sourceContents = readFromFs(sourceUrl);

    if (sourceContents) {
      // We don't handle inline source maps at the moment. Check the sourceMappingURL comment.
      let sourceMapUrl;
      const sourceMapMatch = sourceContents.content.match(
        /[#@]\s*sourceMappingURL=(.+)$/m,
      );
      sourceMapUrl = sourceMapMatch ? sourceMapMatch[1] : undefined;
      if (sourceMapUrl) {
        // Resolve source map url relative to source URL, and read local copy if available.
        const { path: sourceMapPath, content: sourceMapContent } =
          readFromFs(new URL(sourceMapUrl, sourceUrl).href) || {};
        if (sourceMapPath && sourceMapContent) {
          const sm = new SourceMap(JSON.parse(sourceMapContent));
          // Lines are shifted by 2 in the bundle for reasons unclear
          const ent = sm.findOrigin(line - 2, col);
          if (ent && "fileName" in ent) {
            // Source map contains entry for this line
            // Resolve original source path relative to the test output directory
            ent.fileName = path.resolve(
              path.dirname(sourceMapPath),
              ent.fileName,
            );

            return {
              file: ent.fileName,
              line: ent.lineNumber,
              column: ent.columnNumber,
            };
          }
        }
      }
    }
  }

  /**
   * Resolves the url to a local file path and reads its contents.
   */
  function readFromFs(testOutUrl) {
    // URLs that start with http://localhost:<port>/static/devextensions/test/out
    // come from our own build output. No need to make a `fetch` request,
    // just read directly from the filesystem.
    const urlPattern =
      /http:\/\/localhost:\d+\/static\/devextensions\/test\/out/;
    const localPath = join(thisDir, "out");

    if (urlPattern.test(testOutUrl)) {
      let path = testOutUrl.replace(urlPattern, localPath).split("#")[0]; // remove #foo suffix
      return { path, content: readFileSync(path, "utf8") };
    }
    return null; // not mapped to a known local path, don't fetch
  }
}

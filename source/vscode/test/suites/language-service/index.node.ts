// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { runMochaTests } from "../runNode";
import * as http from "http";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { setTestGithubEndpoint } from "../extensionUtils";

export async function run(): Promise<void> {
  const server = await startFakeGithubServer();
  try {
    await runMochaTests(() => {
      // We can't use any wildcards or dynamically discovered
      // paths here since ESBuild needs these modules to be
      // real paths on disk at bundling time.
      require("./language-service.test"); // eslint-disable-line @typescript-eslint/no-require-imports
      require("./notebook.test"); // eslint-disable-line @typescript-eslint/no-require-imports
    });
  } finally {
    server.close();
  }
}

/**
 * Starts a local HTTP server that serves files from the test workspace's
 * web/github directory. This acts as a fake GitHub raw content endpoint
 * for testing package dependency resolution in the node (Electron) environment.
 */
function startFakeGithubServer(): Promise<http.Server> {
  const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
  if (!workspaceFolder) {
    throw new Error("No workspace folder found");
  }

  const webGithubPath = path.resolve(
    workspaceFolder.uri.fsPath,
    "web",
    "github",
  );

  return new Promise((resolve, reject) => {
    const server = http.createServer(async (req, res) => {
      const url = new URL(req.url || "/", `http://${req.headers.host}`);
      const filePath = path.resolve(
        path.join(webGithubPath, decodeURIComponent(url.pathname)),
      );

      try {
        const data = await fs.promises.readFile(filePath);
        res.writeHead(200, { "Content-Type": "text/plain" });
        res.end(data);
      } catch {
        res.writeHead(404);
        res.end("Not found");
      }
    });

    server.listen(0, "127.0.0.1", () => {
      const addr = server.address();
      if (typeof addr === "object" && addr) {
        setTestGithubEndpoint(`http://127.0.0.1:${addr.port}`);
      }
      resolve(server);
    });

    server.on("error", reject);
  });
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log, samples } from "qsharp-lang";
import * as vscode from "vscode";
import { qsharpExtensionId } from "./common";
import registryJson from "./registry.json";
import { EventType, sendTelemetryEvent } from "./telemetry";

export async function initProjectCreator(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.createProject`,
      async (folderUri: vscode.Uri | undefined) => {
        sendTelemetryEvent(EventType.CreateProject, {}, {});

        if (!folderUri) {
          // This was run from the command palette, not the context menu, so create the project
          // in the root of an open workspace folder.
          const workspaceCount = vscode.workspace.workspaceFolders?.length || 0;
          if (workspaceCount === 0) {
            // No workspaces open
            vscode.window.showErrorMessage(
              "You must have a folder open to create a Q# project in",
            );
            return;
          } else if (workspaceCount === 1) {
            folderUri = vscode.workspace.workspaceFolders![0].uri;
          } else {
            const workspaceChoice = await vscode.window.showWorkspaceFolderPick(
              { placeHolder: "Pick the workspace folder for the project" },
            );
            if (!workspaceChoice) return;
            folderUri = workspaceChoice.uri;
          }
        }

        const edit = new vscode.WorkspaceEdit();
        const projUri = vscode.Uri.joinPath(folderUri, "qsharp.json");
        const mainUri = vscode.Uri.joinPath(folderUri, "src", "Main.qs");

        const sample = samples.find((elem) => elem.title === "Minimal");
        if (!sample) {
          // Should never happen, because we bake this sample in.
          log.error("Unable to find the Minimal sample");
          return;
        }

        edit.createFile(projUri);
        edit.createFile(mainUri);

        edit.set(projUri, [
          new vscode.TextEdit(new vscode.Range(0, 0, 0, 0), "{}"),
        ]);
        edit.set(mainUri, [
          new vscode.TextEdit(new vscode.Range(0, 0, 0, 0), sample.code),
        ]);

        // This doesn't throw on failure, it just returns false
        if (!(await vscode.workspace.applyEdit(edit))) {
          vscode.window.showErrorMessage(
            "Unable to create the project. Check the project files don't already exist and that the file system is writable",
          );
        }
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.populateFilesList`,
      async (qsharpJsonUri: vscode.Uri | undefined) => {
        // If called from the content menu qsharpJsonUri will be the full qsharp.json uri
        // If called from the command palette is will be undefined, so use the active editor
        log.info("populateQsharpFilesList called with", qsharpJsonUri);

        qsharpJsonUri =
          qsharpJsonUri ?? vscode.window.activeTextEditor?.document.uri;
        if (!qsharpJsonUri) {
          log.error(
            "populateFilesList called, but argument or active editor is not qsharp.json",
          );
          return;
        }

        log.debug("Populating qsharp.json files for: ", qsharpJsonUri.path);

        // First, verify the qsharp.json can be opened and is a valid json file
        const qsharpJsonDoc =
          await vscode.workspace.openTextDocument(qsharpJsonUri);
        if (!qsharpJsonDoc) {
          log.error("Unable to open the qsharp.json file at ", qsharpJsonDoc);
          return;
        }

        let manifestObj: any = {};
        try {
          manifestObj = JSON.parse(qsharpJsonDoc.getText());
        } catch {
          await vscode.window.showErrorMessage(
            `Unable to parse the contents of ${qsharpJsonUri.path}`,
          );
          return;
        }

        // Recursively find all .qs documents under the ./src dir
        const files: string[] = [];
        const srcDir = vscode.Uri.joinPath(qsharpJsonUri, "..", "src");

        // Verify the src directory exists
        try {
          const srcDirStat = await vscode.workspace.fs.stat(srcDir);
          if (srcDirStat.type !== vscode.FileType.Directory) {
            await vscode.window.showErrorMessage(
              "The ./src path is not a directory",
            );
            return;
          }
        } catch {
          await vscode.window.showErrorMessage(
            "The ./src directory does not exist. Create the directory and add .qs files to it",
          );
          return;
        }

        async function getSourceFilesInDir(dir: vscode.Uri) {
          const dirFiles = (await vscode.workspace.fs.readDirectory(dir)).sort(
            (a, b) => {
              // To order the list, put files before directories, then sort alphabetically
              if (a[1] !== b[1]) return a[1] < b[1] ? -1 : 1;
              return a[0] < b[0] ? -1 : 1;
            },
          );
          for (const [name, type] of dirFiles) {
            if (
              type === vscode.FileType.File &&
              (name.endsWith(".qs") || name.endsWith(".qsc")) &&
              name !== ".qs" &&
              name !== ".qsc"
            ) {
              files.push(vscode.Uri.joinPath(dir, name).toString());
            } else if (type === vscode.FileType.Directory) {
              await getSourceFilesInDir(vscode.Uri.joinPath(dir, name));
            }
          }
          return files;
        }
        await getSourceFilesInDir(srcDir);

        // Update the files property of the qsharp.json and write back to the document
        const srcDirPrefix = srcDir.toString() + "";
        manifestObj["files"] = files.map((file) =>
          file.replace(srcDirPrefix, "src"),
        );

        // Apply the edits to the qsharp.json
        const edit = new vscode.WorkspaceEdit();
        edit.replace(
          qsharpJsonUri,
          new vscode.Range(0, 0, qsharpJsonDoc.lineCount, 0),
          JSON.stringify(manifestObj, null, 2),
        );
        if (!(await vscode.workspace.applyEdit(edit))) {
          vscode.window.showErrorMessage(
            "Unable to update the qsharp.json file. Check the file is writable",
          );
          return;
        }

        // Bring the qsharp.json to the front for the user to save
        await vscode.window.showTextDocument(qsharpJsonDoc);
      },
    ),
  );

  type LocalProjectRef = {
    path: string; // Absolute or relative path to the project dir
  };

  type GitHubProjectRef = {
    github: {
      owner: string;
      repo: string;
      ref: string;
      path?: string; // Optional, defaults to the root of the repo
      refs: undefined;
    };
  };

  type Dependency = LocalProjectRef | GitHubProjectRef;

  // Given two directory paths, return the relative path from the first to the second
  function getRelativeDirPath(from: string, to: string): string {
    // Ensure we have something
    if (!from || !to) throw "Invalid arguments";

    // Trim trailing slashes (even from the root "/" case)
    if (from.endsWith("/")) from = from.slice(0, -1);
    if (to.endsWith("/")) to = to.slice(0, -1);

    // Break both paths into their components
    const fromParts = from.split("/");
    const toParts = to.split("/");

    // Remove the common beginning of the paths
    while (fromParts[0] === toParts[0]) {
      fromParts.shift();
      toParts.shift();
    }

    // Add a .. for each remaining part in the from path
    let result = "";
    while (fromParts.length) {
      result += "../";
      fromParts.shift();
    }
    // Add the remaining path from the to path
    result += toParts.join("/");
    if (result.endsWith("/")) {
      result = result.slice(0, -1);
    }
    return result;
  }

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.addProjectReference`,
      async (qsharpJsonUri: vscode.Uri | undefined) => {
        // If called from the content menu qsharpJsonUri will be the full qsharp.json uri
        // If called from the command palette is will be undefined, so use the active editor
        log.info("addProjectReference called with", qsharpJsonUri);

        qsharpJsonUri =
          qsharpJsonUri ?? vscode.window.activeTextEditor?.document.uri;
        if (!qsharpJsonUri) {
          log.error(
            "addProjectReference called, but argument or active editor is not qsharp.json",
          );
          return;
        }

        log.debug("Adding project reference to ", qsharpJsonUri.path);

        // First, verify the qsharp.json can be opened and is a valid json file
        const qsharpJsonDoc =
          await vscode.workspace.openTextDocument(qsharpJsonUri);
        if (!qsharpJsonDoc) {
          log.error("Unable to open the qsharp.json file at ", qsharpJsonDoc);
          return;
        }
        const qsharpJsonDir = vscode.Uri.joinPath(qsharpJsonUri, "..");

        let manifestObj: any = {};
        try {
          manifestObj = JSON.parse(qsharpJsonDoc.getText());
        } catch {
          await vscode.window.showErrorMessage(
            `Unable to parse the contents of ${qsharpJsonUri.path}`,
          );
          return;
        }

        const importChoice = await vscode.window.showQuickPick(
          ["Import from GitHub", "Import from local directory"],
          { placeHolder: "Pick a source to import from" },
        );

        if (!importChoice) {
          log.info("User cancelled import choice");
          return;
        }

        if (importChoice === "Import from GitHub") {
          await importPackage(qsharpJsonDoc, qsharpJsonUri, manifestObj, true);
        } else {
          await importPackage(
            qsharpJsonDoc,
            qsharpJsonUri,
            manifestObj,
            false,
            qsharpJsonDir,
          );
        }
      },
    ),
  );

  async function importPackage(
    qsharpJsonDoc: vscode.TextDocument,
    qsharpJsonUri: vscode.Uri,
    manifestObj: any,
    isGitHub: boolean,
    qsharpJsonDir?: vscode.Uri,
  ) {
    let dependencyRef: Dependency;
    let label: string;

    if (isGitHub) {
      const packageChoice = await vscode.window.showQuickPick(
        registryJson.knownPackages.map(
          (pkg: { name: string; description: string; dependency: object }) => ({
            label: pkg.name,
            description: pkg.description,
          }),
        ),
        { placeHolder: "Pick a package to import" },
      );

      if (!packageChoice) {
        log.info("User cancelled package choice");
        return;
      }

      const chosenPackage = registryJson.knownPackages.find(
        (pkg: { name: string; description: string; dependency: object }) =>
          pkg.name === packageChoice.label,
      )!;

      const versionChoice = await vscode.window.showQuickPick(
        chosenPackage.dependency.github.refs.map(({ ref, notes }) => ({
          label: ref,
          description: notes,
        })),
        { placeHolder: "Pick a version to import" },
      );

      if (!versionChoice) {
        log.info("User cancelled version choice");
        return;
      }

      dependencyRef = {
        github: {
          ref: versionChoice.label,
          ...chosenPackage.dependency.github,
          refs: undefined,
        },
      };

      label = packageChoice.label;
    } else {
      // Find all the other Q# projects in the workspace
      const projectFiles = (
        await vscode.workspace.findFiles("**/qsharp.json")
      ).filter((file) => file.toString() !== qsharpJsonUri.toString());

      // Convert any spaces, dashes, dots, tildes, or quotes in project names
      // to underscores. (Leave more 'exotic' non-identifier patterns to the user to fix)
      //
      // Note: At some point we may want to detect/avoid duplicate names, e.g. if the user already
      // references a project via 'foo', and they add a reference to a 'foo' on GitHub or in another dir.

      const projectChoices = projectFiles.map((file) => {
        // normalize the path using the vscode Uri API
        const dirUri = vscode.Uri.joinPath(file, "..");
        const relPath = getRelativeDirPath(qsharpJsonDir!.path, dirUri.path);
        return {
          name: dirUri.path.split("/").pop()!,
          ref: {
            path: relPath,
          },
        };
      });

      projectChoices.forEach(
        (val, idx, arr) => (arr[idx].name = val.name.replace(/[- "'.~]/g, "_")),
      );

      const folderIcon = new vscode.ThemeIcon("folder");

      // Ask the user to pick a project to add as a reference
      const projectChoice = await vscode.window.showQuickPick(
        projectChoices.map((choice) => ({
          label: choice.name,
          detail: choice.ref.path,
          iconPath: folderIcon,
          ref: choice.ref,
        })),
        { placeHolder: "Pick a project to add as a reference" },
      );

      if (!projectChoice) {
        log.info("User cancelled project choice");
        return;
      }

      dependencyRef = projectChoice.ref;
      label = projectChoice.label;
    }

    await updateManifestAndSave(
      qsharpJsonDoc,
      qsharpJsonUri,
      manifestObj,
      label,
      dependencyRef,
    );
  }

  async function updateManifestAndSave(
    qsharpJsonDoc: vscode.TextDocument,
    qsharpJsonUri: vscode.Uri,
    manifestObj: any,
    label: string,
    ref: Dependency,
  ) {
    if (!manifestObj["dependencies"]) manifestObj["dependencies"] = {};
    manifestObj["dependencies"][label] = ref;

    // Apply the edits to the qsharp.json
    const edit = new vscode.WorkspaceEdit();
    edit.replace(
      qsharpJsonUri,
      new vscode.Range(0, 0, qsharpJsonDoc.lineCount, 0),
      JSON.stringify(manifestObj, null, 4),
    );
    if (!(await vscode.workspace.applyEdit(edit))) {
      await vscode.window.showErrorMessage(
        "Unable to update the qsharp.json file. Check the file is writable",
      );
      return;
    }

    // Bring the qsharp.json to the front for the user to save
    await vscode.window.showTextDocument(qsharpJsonDoc);
  }
}

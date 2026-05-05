// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Discovers learning courses from the well-known `qdk-learning-content` folder
 * in the workspace. Each top-level subfolder is a course; its children (files
 * or folders) become single-activity units of kind "example".
 */

import * as vscode from "vscode";
import { LEARNING_CONTENT_FOLDER } from "./constants.js";
import type { CatalogCourse, CatalogUnit, CatalogExample } from "./types.js";

/** Supported code file extensions for example activities. */
const SUPPORTED_EXTENSIONS = new Set([".ipynb", ".qs", ".py", ".qasm"]);

/**
 * Scan all workspace folders for a `qdk-learning-content` directory and
 * return discovered courses. Each top-level subfolder under the content
 * directory becomes a course; its children become example activities.
 */
export async function scanForCourses(): Promise<CatalogCourse[]> {
  const courses: CatalogCourse[] = [];

  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const contentRoot = vscode.Uri.joinPath(
      folder.uri,
      LEARNING_CONTENT_FOLDER,
    );
    if (!(await uriExists(contentRoot))) {
      continue;
    }

    const topEntries = await vscode.workspace.fs.readDirectory(contentRoot);
    for (const [name, type] of topEntries) {
      if (type !== vscode.FileType.Directory || name.startsWith(".")) {
        continue;
      }
      const courseUri = vscode.Uri.joinPath(contentRoot, name);
      const units = await scanCourseFolder(courseUri);
      if (units.length > 0) {
        const iconUri = vscode.Uri.joinPath(courseUri, "icon.svg");
        const iconPath = (await uriExists(iconUri))
          ? iconUri.fsPath
          : undefined;
        courses.push({
          id: name,
          title: name,
          units,
          iconPath,
        });
      }
    }
  }

  return courses;
}

/**
 * Scan a single course folder. Each child (file or folder) becomes a unit
 * with a single "example" activity.
 */
async function scanCourseFolder(courseUri: vscode.Uri): Promise<CatalogUnit[]> {
  const entries = await vscode.workspace.fs.readDirectory(courseUri);
  const units: CatalogUnit[] = [];

  for (const [name, type] of entries) {
    const childUri = vscode.Uri.joinPath(courseUri, name);

    if (type === vscode.FileType.File) {
      if (!isSupportedFile(name)) {
        continue;
      }
      const id = stripExtension(name);
      units.push({
        id,
        title: id,
        sections: [
          {
            type: "example",
            id,
            title: id,
            filePath: childUri.fsPath,
          } satisfies CatalogExample,
        ],
      });
    } else if (type === vscode.FileType.Directory) {
      const mainAsset = await findMainAsset(childUri);
      if (mainAsset) {
        units.push({
          id: name,
          title: name,
          sections: [
            {
              type: "example",
              id: name,
              title: name,
              filePath: mainAsset.fsPath,
            } satisfies CatalogExample,
          ],
        });
      }
    }
  }

  return units;
}

/**
 * Find the "main" asset file within a folder-based activity.
 *
 * Priority:
 * 1. Any `*.ipynb` file (first found)
 * 2. `src/Main.qs`
 * 3. First supported file found at the top level
 */
async function findMainAsset(
  folderUri: vscode.Uri,
): Promise<vscode.Uri | undefined> {
  const entries = await vscode.workspace.fs.readDirectory(folderUri);

  // 1. Look for any .ipynb at the top level
  for (const [name, type] of entries) {
    if (type === vscode.FileType.File && name.endsWith(".ipynb")) {
      return vscode.Uri.joinPath(folderUri, name);
    }
  }

  // 2. Check for src/Main.qs
  const mainQs = vscode.Uri.joinPath(folderUri, "src", "Main.qs");
  if (await uriExists(mainQs)) {
    return mainQs;
  }

  // 3. Fallback to first supported file at top level
  for (const [name, type] of entries) {
    if (type === vscode.FileType.File && isSupportedFile(name)) {
      return vscode.Uri.joinPath(folderUri, name);
    }
  }

  return undefined;
}

function isSupportedFile(name: string): boolean {
  const dotIdx = name.lastIndexOf(".");
  if (dotIdx < 0) {
    return false;
  }
  return SUPPORTED_EXTENSIONS.has(name.slice(dotIdx).toLowerCase());
}

function stripExtension(name: string): string {
  const dotIdx = name.lastIndexOf(".");
  return dotIdx > 0 ? name.slice(0, dotIdx) : name;
}

async function uriExists(uri: vscode.Uri): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch {
    return false;
  }
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Discovers learning courses from the well-known `qdk-learning-content` folder
 * in the workspace. Each top-level subfolder is a course; its children (files
 * or folders) become units. Notebook (`.ipynb`) files are split into multiple
 * "example" activities at `##` heading boundaries for cell-level navigation.
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
 * Scan a single course folder. Each child (file or folder) becomes a unit.
 * Notebook files are split into multiple activities at `##` headings.
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
      const sections = await buildSections(childUri, id);
      units.push({ id, title: id, sections });
    } else if (type === vscode.FileType.Directory) {
      const mainAsset = await findMainAsset(childUri);
      if (mainAsset) {
        const sections = await buildSections(mainAsset, name);
        units.push({ id: name, title: name, sections });
      }
    }
  }

  return units;
}

/**
 * Build activity sections for a file. For `.ipynb` files, split at `##`
 * heading boundaries to create one activity per section. For other file
 * types, return a single activity.
 */
async function buildSections(
  fileUri: vscode.Uri,
  fallbackId: string,
): Promise<CatalogExample[]> {
  if (fileUri.path.endsWith(".ipynb")) {
    const sections = await splitNotebookAtHeadings(fileUri, fallbackId);
    if (sections.length > 0) {
      return sections;
    }
  }

  // Non-notebook or notebook without ## headings: single activity.
  return [
    {
      type: "example",
      id: fallbackId,
      title: fallbackId,
      filePath: fileUri.fsPath,
    },
  ];
}

/**
 * Parse a notebook and split it into activities at `##` markdown headings.
 * Returns one `CatalogExample` per heading-delimited section, each with a
 * `cellIndex` pointing to its anchor cell. Cells before the first `##`
 * heading become an "Introduction" activity (if any exist).
 */
async function splitNotebookAtHeadings(
  notebookUri: vscode.Uri,
  unitId: string,
): Promise<CatalogExample[]> {
  let nbJson: { cells?: { cell_type: string; source: string | string[] }[] };
  try {
    const raw = await vscode.workspace.fs.readFile(notebookUri);
    nbJson = JSON.parse(new TextDecoder("utf-8").decode(raw));
  } catch {
    return [];
  }

  const cells = nbJson.cells;
  if (!cells || cells.length === 0) {
    return [];
  }

  const filePath = notebookUri.fsPath;
  const sections: CatalogExample[] = [];

  // Walk cells and find ## heading boundaries.
  let introEnd = -1;
  for (let i = 0; i < cells.length; i++) {
    const cell = cells[i];
    if (cell.cell_type !== "markdown") {
      continue;
    }
    const heading = extractLevel2Heading(cell.source);
    if (heading) {
      if (introEnd < 0) {
        introEnd = i;
        // If there are cells before the first heading, create an intro activity.
        if (i > 0) {
          sections.push({
            type: "example",
            id: `${unitId}--intro`,
            title: "Introduction",
            filePath,
            cellIndex: 0,
          });
        }
      }
      sections.push({
        type: "example",
        id: `${unitId}--${slugify(heading)}`,
        title: heading,
        filePath,
        cellIndex: i,
      });
    }
  }

  return sections;
}

/**
 * Extract the first `## ` heading from a markdown cell's source.
 * Returns the heading text (without the `## ` prefix) or undefined.
 */
function extractLevel2Heading(source: string | string[]): string | undefined {
  const text = Array.isArray(source) ? source.join("") : source;
  for (const line of text.split("\n")) {
    const trimmed = line.trimStart();
    if (trimmed.startsWith("## ")) {
      return trimmed.slice(3).trim();
    }
  }
  return undefined;
}

/** Convert a heading string into a URL-safe slug for use as an id. */
function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 60);
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

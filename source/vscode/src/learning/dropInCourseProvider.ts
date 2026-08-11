// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import {
  COURSE_MANIFEST_FILE,
  LEARNING_COURSES_SUBDIR,
  LEARNING_WORKSPACE_FOLDER,
  WORKBOOK_SUFFIX,
} from "./constants.js";
import type { CourseProvider } from "./courseProvider.js";
import { uriExists } from "./fsUtils.js";
import { parseNotebookExercises } from "./notebookExercises.js";
import type {
  CatalogActivity,
  CatalogCourse,
  CatalogExercise,
  CatalogUnit,
  CourseEnvironment,
  NotebookExerciseInfo,
} from "./types.js";

/**
 * On-disk shape of a `course.json` manifest. Author-controlled, so every
 * field is validated before use.
 */
interface CourseManifest {
  schemaVersion?: number;
  id?: unknown;
  title?: unknown;
  shortDescription?: unknown;
  units?: unknown;
  environment?: unknown;
}

interface ManifestUnit {
  id: string;
  title: string;
  dir: string;
}

/** A resolved course folder containing a parsed manifest. */
interface CourseLocation {
  /** Folder that contains `course.json`. */
  dir: vscode.Uri;
  manifest: CourseManifest;
}

/**
 * Loads "drop-in" courses authored as folders on disk. A course is a
 * folder containing a `course.json` manifest plus per-unit subfolders.
 * Each unit is a Python notebook (`*.ipynb`) whose exercise metadata is
 * marked up with cell tags.
 *
 * Course folders are discovered under `qdk-learning/courses/*` in the
 * workspace. Malformed courses are skipped with a warning rather than
 * failing the whole load.
 */
export class DropInCourseProvider implements CourseProvider {
  readonly id = "drop-in-provider";

  constructor(private readonly workspaceRoot: vscode.Uri) {}

  async listCourses(): Promise<CatalogCourse[]> {
    const courses: CatalogCourse[] = [];
    const seen = new Set<string>();
    for (const loc of await this.discover()) {
      const course = await this.parseCourse(loc);
      if (!course) {
        continue;
      }
      if (seen.has(course.id)) {
        log.warn(
          `Duplicate drop-in course id "${course.id}" ignored at ${loc.dir.toString()}`,
        );
        continue;
      }
      seen.add(course.id);
      courses.push(course);
    }
    return courses;
  }

  // ─── Discovery ───

  /** Enumerate candidate course folders and parse their manifests. */
  private async discover(): Promise<CourseLocation[]> {
    const dirs: vscode.Uri[] = [];

    // The well-known in-workspace courses folder.
    const coursesRoot = vscode.Uri.joinPath(
      this.workspaceRoot,
      LEARNING_WORKSPACE_FOLDER,
      LEARNING_COURSES_SUBDIR,
    );
    for (const child of await readDirSafe(coursesRoot)) {
      if (child.type === vscode.FileType.Directory) {
        dirs.push(vscode.Uri.joinPath(coursesRoot, child.name));
      }
    }

    const locations: CourseLocation[] = [];
    for (const dir of dirs) {
      const manifest = await this.readManifest(dir);
      if (manifest) {
        locations.push({ dir, manifest });
      }
    }
    return locations;
  }

  /** Read and JSON-parse a course manifest, or `undefined` if absent/invalid. */
  private async readManifest(
    dir: vscode.Uri,
  ): Promise<CourseManifest | undefined> {
    const manifestUri = vscode.Uri.joinPath(dir, COURSE_MANIFEST_FILE);
    const text = await tryReadText(manifestUri);
    if (text === undefined) {
      return undefined;
    }
    try {
      const parsed = JSON.parse(text) as CourseManifest;
      if (
        manifestString(parsed.id) === undefined ||
        manifestString(parsed.title) === undefined
      ) {
        log.warn(
          `Ignoring drop-in course at ${dir.toString()}: "id" and "title" are required.`,
        );
        return undefined;
      }
      return parsed;
    } catch (e) {
      log.warn(`Failed to parse ${manifestUri.toString()}: ${String(e)}`);
      return undefined;
    }
  }

  // ─── Parsing ───

  private async parseCourse(
    loc: CourseLocation,
  ): Promise<CatalogCourse | undefined> {
    const id = manifestString(loc.manifest.id);
    const title = manifestString(loc.manifest.title);
    if (id === undefined || title === undefined) {
      return undefined;
    }

    const units: CatalogUnit[] = [];
    for (const manifestUnit of manifestUnits(loc.manifest.units, loc.dir)) {
      const unitDir = vscode.Uri.joinPath(loc.dir, manifestUnit.dir);
      if (!(await uriExists(unitDir))) {
        log.warn(
          `Skipping unit "${manifestUnit.id}" in course "${id}": dir not found (${manifestUnit.dir}).`,
        );
        continue;
      }
      const { activities, notebookExercises, sourceNotebookRel } =
        await this.parseNotebookUnit(unitDir, manifestUnit);
      if (activities.length === 0) {
        log.warn(
          `Unit "${manifestUnit.id}" in course "${id}" has no activities.`,
        );
      }
      units.push({
        id: manifestUnit.id,
        title: manifestUnit.title,
        activities,
        notebookExercises,
        sourceNotebookRel,
      });
    }

    return {
      id,
      title,
      shortDescription: manifestString(loc.manifest.shortDescription),
      kind: "python-notebook",
      units,
      sourceDir: loc.dir.toString(),
      environment: manifestEnvironment(loc.manifest.environment),
    };
  }

  /**
   * Parse a `python-notebook` unit. The notebook itself carries the unit's
   * narrative content and is opened directly by the user; the extension does
   * not execute cells.
   *
   * Exercise metadata (hints, solutions) is parsed from the authored
   * notebook's cell tags and attached to the returned unit for use by chat
   * LM tools. See `notebookExercises.ts` for the tag vocabulary.
   */
  private async parseNotebookUnit(
    unitDir: vscode.Uri,
    unit: ManifestUnit,
  ): Promise<{
    activities: CatalogActivity[];
    notebookExercises?: NotebookExerciseInfo[];
    sourceNotebookRel?: string;
  }> {
    // Find the source notebook file in the unit dir. Materialized working
    // copies (`*.workbook.ipynb`) sit beside the source and must be ignored
    // here so they are never mistaken for the authored source notebook.
    const entries = await readDirSafe(unitDir);
    const notebookEntries = entries
      .filter(
        (e) =>
          e.type === vscode.FileType.File &&
          e.name.toLowerCase().endsWith(".ipynb") &&
          !e.name.toLowerCase().endsWith(WORKBOOK_SUFFIX),
      )
      .sort((a, b) => a.name.localeCompare(b.name));
    let notebookEntry: (typeof notebookEntries)[number];
    switch (notebookEntries.length) {
      case 0:
        log.warn(
          `Unit "${unit.id}" has no .ipynb notebook in ${unitDir.fsPath}.`,
        );
        return { activities: [] };
      case 1:
        notebookEntry = notebookEntries[0];
        break;
      default:
        notebookEntry = notebookEntries[0];
        log.warn(
          `Unit "${unit.id}" has multiple .ipynb notebooks in ${unitDir.fsPath} - using ${notebookEntry.name}.`,
        );
        break;
    }

    const sourceNotebookRel = `${unit.dir}/${notebookEntry.name}`;

    const activities: CatalogActivity[] = [];

    // Exercise metadata lives in the authored notebook, marked up with cell
    // tags. Read it here so it's available before materialization.
    const notebookText = await tryReadText(
      vscode.Uri.joinPath(unitDir, notebookEntry.name),
    );
    const notebookExercises = notebookText
      ? parseNotebookExercises(notebookText, unit.id)
      : undefined;

    // Surface each notebook exercise as a catalog activity so it appears
    // in the progress tree and can be navigated to.
    if (notebookExercises) {
      for (const ex of notebookExercises) {
        activities.push({
          type: "exercise",
          id: ex.cellId,
          title: ex.title,
          description: ex.description,
          placeholderCode: "",
          sourceIds: [],
          hints: ex.hints,
          solutionCodes: ex.solutions,
          solutionExplanation: ex.solutionExplanation,
        } satisfies CatalogExercise);
      }
    }

    return { activities, notebookExercises, sourceNotebookRel };
  }
}

// ─── Manifest field validation ───

function manifestString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value
    : undefined;
}

function manifestEnvironment(value: unknown): CourseEnvironment | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const obj = value as {
    importChecks?: unknown;
  };
  const env: CourseEnvironment = {};

  if (
    Array.isArray(obj.importChecks) &&
    obj.importChecks.every((r) => typeof r === "string")
  ) {
    env.importChecks = obj.importChecks as string[];
  }

  return env;
}

function manifestUnits(value: unknown, dir: vscode.Uri): ManifestUnit[] {
  if (!Array.isArray(value)) {
    log.warn(`Course at ${dir.toString()} has no "units" array.`);
    return [];
  }
  const units: ManifestUnit[] = [];
  for (const raw of value) {
    if (!raw || typeof raw !== "object") {
      continue;
    }
    const id = manifestString((raw as { id?: unknown }).id);
    const title = manifestString((raw as { title?: unknown }).title);
    const unitDir = manifestString((raw as { dir?: unknown }).dir);
    if (id === undefined || title === undefined || unitDir === undefined) {
      log.warn(
        `Ignoring malformed unit in course at ${dir.toString()} (requires id, title, dir).`,
      );
      continue;
    }
    if (!isContainedRelativePath(unitDir)) {
      log.warn(
        `Ignoring unit "${id}" in course at ${dir.toString()}: "dir" must be a relative path inside the course folder.`,
      );
      continue;
    }
    units.push({ id, title, dir: unitDir });
  }
  return units;
}

/**
 * True when a manifest-supplied path stays inside the course folder.
 *
 * `dir` is the only path segment a course author controls, and it is joined
 * onto the course root to locate notebooks that are later read and written.
 * `Uri.joinPath` resolves `..`, so an unchecked value could escape the
 * workspace entirely.
 */
function isContainedRelativePath(value: string): boolean {
  if (/^[/\\]/.test(value) || /^[a-zA-Z]:/.test(value)) {
    return false;
  }
  return !value.split(/[/\\]/).includes("..");
}

// ─── Filesystem helpers ───

async function readDirSafe(
  uri: vscode.Uri,
): Promise<{ name: string; type: vscode.FileType }[]> {
  try {
    const entries = await vscode.workspace.fs.readDirectory(uri);
    return entries.map(([name, type]) => ({ name, type }));
  } catch {
    return [];
  }
}

async function tryReadText(uri: vscode.Uri): Promise<string | undefined> {
  try {
    const bytes = await vscode.workspace.fs.readFile(uri);
    return new TextDecoder().decode(bytes);
  } catch {
    return undefined;
  }
}

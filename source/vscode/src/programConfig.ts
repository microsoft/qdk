// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  IProjectConfig,
  IQSharpError,
  ProgramConfig,
  QdkDiagnostics,
  TargetProfile,
} from "qsharp-lang";
import * as vscode from "vscode";
import { isOpenQasmDocument, isQdkDocument } from "./common";
import { invokeAndReportCommandDiagnostics } from "./diagnostics";
import { ensureQdkFeaturesAvailable } from "./language-service/openqasmMode";
import { loadOpenQasmProject, loadProject } from "./projectSystem";

/**
 * Notice the similarity to @type {ProgramConfig} and @type {IProjectConfig}.
 * These types look similar but have a few differences:
 *
 * ProgramConfig is used in the API for the qsharp-lang package. All properties
 * are optional for backward compatibility. It only contains the properties
 * that are needed by the compiler APIs.
 *
 * ProjectConfig contains the values come from the Q# manifest, or for single-file
 * programs, the defaults for these values. In the future, for projects with
 * dependencies, it will also contain the full dependency graph and sources for
 * all packages referenced by the project.
 *
 * FullProgramConfig is a union of the above. It's meant to represent a fully
 * populated configuration that can be used across a variety of extension features.
 * So all the properties are required.
 */

export type FullProgramConfig = Required<ProgramConfig & IProjectConfig>;

export type FullProgramConfigOrError =
  | {
      success: true;
      programConfig: FullProgramConfig;
    }
  | {
      success: false;
      errorMsg: string;
      diagnostics?: IQSharpError[];
    };

/**
 * @returns The currently active Q# project configuration. This is a general-purpose
 *   function that is useful for any extension command that is intended to
 *   operate on the "current" project.
 */
export async function getActiveProgram(
  options: {
    showModalError: boolean;
    targetProfileFallback?: TargetProfile;
  } = { showModalError: false },
): Promise<FullProgramConfigOrError> {
  const doc = getActiveQdkDocument();
  if (!doc) {
    return {
      success: false,
      errorMsg:
        "The currently active window is not a document supported by the QDK",
    };
  }

  return await getProgramForDocument(doc, options);
}

export function getActiveQdkDocumentUri(): vscode.Uri | undefined {
  return getActiveQdkDocument()?.uri;
}

export function getActiveQdkDocument(): vscode.TextDocument | undefined {
  const editor = vscode.window.activeTextEditor;
  return editor?.document && isQdkDocument(editor.document)
    ? editor.document
    : undefined;
}

export async function getVisibleProgram(
  options: {
    showModalError?: boolean;
    targetProfileFallback?: TargetProfile;
  } = { showModalError: false },
): Promise<FullProgramConfigOrError> {
  const doc = getVisibleQdkDocument();
  if (!doc) {
    return {
      success: false,
      errorMsg:
        "There are no visible windows that contain a document supported by the QDK",
    };
  }
  return await getProgramForDocument(doc, options);
}

export function getVisibleQdkDocumentUri(): vscode.Uri | undefined {
  return getVisibleQdkDocument()?.uri;
}

export function getVisibleQdkDocument(): vscode.TextDocument | undefined {
  return vscode.window.visibleTextEditors.find((editor) =>
    isQdkDocument(editor.document),
  )?.document;
}

export async function getProgramForDocument(
  doc: vscode.TextDocument,
  options: {
    showModalError?: boolean;
    targetProfileFallback?: TargetProfile;
    resumeAfterSpecModeSwitch?: boolean;
  } = {},
): Promise<FullProgramConfigOrError> {
  const modeError = await ensureQdkFeaturesAvailable(
    doc,
    options.resumeAfterSpecModeSwitch,
  );
  if (modeError) {
    return { success: false, errorMsg: modeError };
  }

  // Project configs come from the document
  try {
    const program = await invokeAndReportCommandDiagnostics(
      () => loadProjectForDocument(doc),
      // Don't populate problems view with errors, because
      // we expect the language service to have already done that at this point.
      options,
    );

    // Fill in a default for the target profile if one didn't come from the user source
    const profile: TargetProfile =
      program.profile || options.targetProfileFallback || "unrestricted";

    const programConfig = {
      ...program,
      profile,
    };

    return { success: true, programConfig };
  } catch (e: unknown) {
    return {
      success: false,
      diagnostics: e instanceof QdkDiagnostics ? e.diagnostics : undefined,
      errorMsg: e instanceof Error ? e.message : "Could not load project",
    };
  }
}

function loadProjectForDocument(
  doc: vscode.TextDocument,
): IProjectConfig | Promise<IProjectConfig> {
  return isOpenQasmDocument(doc)
    ? loadOpenQasmProject(doc.uri)
    : loadProject(doc.uri);
}

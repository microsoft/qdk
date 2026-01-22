// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  IProjectConfig,
  getProjectLoader,
  log,
  qsharpGithubUriScheme,
  ProjectLoader,
  getTargetProfileFromEntryPoint,
} from "qsharp-lang";
import * as vscode from "vscode";
import { URI, Utils } from "vscode-uri";
import { sendTelemetryEvent, EventType } from "./telemetry";

/** Returns the manifest document if one is found
 * returns null otherwise
 */
async function findManifestDocument(
  currentDocumentUriString: string,
): Promise<{ directory: URI; manifest: URI } | null> {
  // file://home/foo/bar/src/document.qs
  // or, e.g. in vscode on a virtual file system,
  // vscode-vfs://github%2B7b2276223a312c22726566223a7b2274797065223a332c226964223a22383439227d7d/microsoft/qsharp/samples/shor.qs
  const currentDocumentUri = URI.parse(currentDocumentUriString);

  // if this document is itself a manifest file, then we've found it
  if (currentDocumentUri.path.endsWith("qsharp.json")) {
    return {
      directory: Utils.dirname(currentDocumentUri),
      manifest: currentDocumentUri,
    };
  }

  // Untitled documents don't have a file location, thus can't have a manifest
  if (currentDocumentUri.scheme === "untitled") return null;

  // just the parent
  // e.g.
  // file://home/foo/bar/src
  let uriToQuery = Utils.dirname(currentDocumentUri);

  let attempts = 100;

  let seenSrcDir = false;

  while (attempts > 0) {
    attempts--;

    // Make sure that the path doesn't go above one of the open workspaces.
    if (
      !vscode.workspace.workspaceFolders?.some((root) =>
        uriToQuery.toString().startsWith(root.uri.toString()),
      )
    ) {
      log.debug("Aborting search for manifest file outside of workspace");
      return null;
    }

    if (seenSrcDir) {
      let qsharpJsonExists = false;
      const potentialManifestUri = Utils.joinPath(uriToQuery, "qsharp.json");

      try {
        qsharpJsonExists =
          (await vscode.workspace.fs.stat(potentialManifestUri)).type ===
          vscode.FileType.File;
      } catch {
        // qsharp.json doesn't exist or is inaccessible, move on
      }

      if (qsharpJsonExists) {
        return { directory: uriToQuery, manifest: potentialManifestUri };
      }
    }
    if (uriToQuery.toString().endsWith("src")) {
      seenSrcDir = true;
    } else {
      seenSrcDir = false;
    }

    const oldUriToQuery = uriToQuery;
    uriToQuery = Utils.resolvePath(uriToQuery, "..");
    if (oldUriToQuery === uriToQuery) {
      return null;
    }
  }
  return null;
}

export async function findManifestDirectory(uri: string) {
  const result = await findManifestDocument(uri);
  if (result) {
    return result.directory.toString();
  }
  return null;
}

// this function currently assumes that `directoryQuery` will be a relative path from
// the root of the workspace
export async function listDirectory(
  directoryQuery: string,
): Promise<[string, number][]> {
  const uriToQuery = vscode.Uri.parse(directoryQuery);

  const fileSearchResult = await vscode.workspace.fs.readDirectory(uriToQuery);
  const mappedFiles: [string, vscode.FileType][] = fileSearchResult.map(
    ([name, type]) => [Utils.joinPath(uriToQuery, name).toString(), type],
  );

  return mappedFiles;
}

export async function readFile(uri: string): Promise<string | null> {
  const file = await readFileUri(uri);
  return file?.content || null;
}

async function readFileUri(
  maybeUri: string | vscode.Uri,
): Promise<{ uri: vscode.Uri; content: string } | null> {
  const uri: vscode.Uri = (maybeUri as any).path
    ? (maybeUri as vscode.Uri)
    : vscode.Uri.parse(maybeUri as string);

  // If any open documents match this uri, return their contents instead of from disk
  const opendoc = vscode.workspace.textDocuments.find(
    (opendoc) => opendoc.uri.toString() === uri.toString(),
  );

  if (opendoc) {
    return {
      content: opendoc.getText(),
      uri,
    };
  }

  try {
    return await vscode.workspace.fs.readFile(uri).then((res) => {
      return {
        content: new TextDecoder().decode(res),
        uri: uri,
      };
    });
  } catch (err) {
    // `readFile` should throw the below if the file is not found
    if (
      !(err instanceof vscode.FileSystemError && err.code === "FileNotFound")
    ) {
      log.error("Unexpected error trying to read file", err);
    }
    throw err;
  }
}

let projectLoader: ProjectLoader | undefined = undefined;

/**
 * Given a Q# Document URI, returns the configuration and list of complete source files
 * associated with that document.
 *
 * If there is a qsharp.json manifest for this document, the settings from that are used.
 *
 * If a manifest is not found, the returned project contains the single input file and the default settings.
 *
 * @param documentUri A Q# document.
 * @returns The project configuration for that document.
 * @throws Error if the qsharp.json cannot be parsed.
 */
export async function loadProject(
  documentUri: vscode.Uri,
): Promise<IProjectConfig> {
  const manifestDocument = await findManifestDocument(documentUri.toString());

  if (!manifestDocument) {
    // return just the one file if we are in single file mode
    return await singleFileProject(documentUri);
  }

  if (!projectLoader) {
    projectLoader = await getProjectLoader({
      findManifestDirectory,
      readFile,
      listDirectory,
      fetchGithub: fetchGithubRaw,
      resolvePath: async (a, b) => resolvePath(a, b),
    });
  }

  const project = await projectLoader!.loadQSharpProject(
    manifestDocument.directory.toString(),
  );

  return project;
}

/**
 * Given an OpenQASM Document URI, returns the configuration and list of complete source files
 * associated with that document.
 *
 * @param documentUri An OpenQASM document.
 * @returns The project configuration for that document.
 * @throws Error if the qsharp.json cannot be parsed.
 */
export async function loadOpenQasmProject(
  documentUri: vscode.Uri,
): Promise<IProjectConfig> {
  if (!projectLoader) {
    projectLoader = await getProjectLoader({
      findManifestDirectory,
      readFile,
      listDirectory,
      fetchGithub: fetchGithubRaw,
      resolvePath: async (a, b) => resolvePath(a, b),
    });
  }

  const project = await projectLoader!.loadOpenQasmProject(
    documentUri.toString(),
  );

  return project;
}

async function singleFileProject(
  documentUri: vscode.Uri,
): Promise<IProjectConfig> {
  const file = await vscode.workspace.openTextDocument(documentUri);
  const profile = await getTargetProfileFromEntryPoint(
    file.fileName,
    file.getText(),
  );

  return {
    projectName: Utils.basename(documentUri),
    projectUri: documentUri.toString(),
    packageGraphSources: {
      root: {
        sources: [[documentUri.toString(), file.getText()]] as [
          string,
          string,
        ][],
        languageFeatures: [],
        dependencies: {},
      },
      packages: {},
      hasManifest: false,
    },
    lints: [],
    errors: [],
    projectType: "qsharp",
    profile,
  };
}

export function resolvePath(base: string, relative: string): string | null {
  return Utils.resolvePath(URI.parse(base, true), relative).toString();
}

let githubEndpoint = "https://raw.githubusercontent.com";
export function setGithubEndpoint(endpoint: string) {
  githubEndpoint = endpoint;
}

export function getGithubSourceContent(uri: URI): string | undefined {
  const key = uri.toString();
  return knownGitHubSources.get(key);
}

const knownGitHubSources = new Map<string, string>();

/**
 * Makes a request to the GitHub raw content service to retrieve a file.
 */
export async function fetchGithubRaw(
  owner: string,
  repo: string,
  ref: string,
  path: string,
): Promise<string> {
  const pathNoLeadingSlash = path.startsWith("/") ? path.slice(1) : path;

  const uri = `${githubEndpoint}/${owner}/${repo}/${ref}/${pathNoLeadingSlash}`;
  log.info(`making request to ${uri}`);
  const response = await fetch(uri);
  // note that if the above fetch fails, we will never send this telemetry event.
  // however, this is okay, because if a network request to github is failing, it is likely
  // that the user's network itself is suspect and the telemetry wouldn't send anyway.
  sendTelemetryEvent(
    EventType.FetchGitHub,
    { status: response.status.toString() },
    {},
  );
  if (!response.ok) {
    log.warn(
      `fetchGithubRaw: ${owner}/${repo}/${ref}/${path} -> ${response.status} ${response.statusText}`,
    );
    throw new Error(
      `Request to ${uri} failed with status ${response.status} ${response.statusText ? ": " + response.statusText : ""}`,
    );
  }

  let text;
  try {
    text = await response.text();

    knownGitHubSources.set(
      URI.from({
        scheme: qsharpGithubUriScheme,
        path: `${owner}/${repo}/${ref}/${pathNoLeadingSlash}`,
      }).toString(),
      text,
    );
  } catch (e) {
    if (e instanceof Error) {
      log.warn(
        `fetchGithubRaw: ${owner}/${repo}/${ref}/${path} -> ${e.message}`,
      );
      throw new Error(
        `Request to ${uri} did not return text content: ${e.message}`,
      );
    }
    throw new Error(`Request to ${uri} did not return text content`);
  }

  return text;
}

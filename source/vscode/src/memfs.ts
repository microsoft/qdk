// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log, samples, openqasm_samples } from "qsharp-lang";
import * as vscode from "vscode";
import { qsharpExtensionId } from "./common";

export const scheme = "qsharp-vfs";

const playgroundAuthority = "playground";
const playgroundRootUri = vscode.Uri.parse(
  `${scheme}://${playgroundAuthority}/`,
);

const playgroundReadme = `
# Microsoft Quantum Playground

Welcome to the Microsoft Quantum Development Kit playground! An online environment to
safely learn and explore quantum computing with the Q# language.

The 'Samples (Q#)' folder contains a set of common quantum algorithms written in Q#.
The 'Samples (OpenQASM)' folder contains several quantum programs written in OpenQASM.
You can run these samples by clicking the "Run" button in the top right corner
of the editor when you have the file open. You can also set breakpoints and
step through the code using the Debug button at the same location to see how the
algorithm changes quantum state as it executes.

This playground exists entirely in memory and is not persisted to disk. All changes
will be lost when the editor window is closed. You should use the 'File: Save
As...' command in the VS Code Command Palette (accessed by pressing F1) to save
your work elsewhere if you wish to keep it.

For more information about the Microsoft Quantum Development Kit,
visit <https://aka.ms/qdk>.
`;

// Type of a generated sample entry.
type SampleTy = (typeof samples)[number];

function populateSamples(vfs: MemFS) {
  // Put the playground in its own 'authority', so we can keep the default space clean.
  // This has the benefit of the URI https://vscode.dev/quantum/playground/ opening the playground
  vfs.addAuthority(playgroundAuthority);

  const encoder = new TextEncoder();
  populateSamplesFolder(vfs, encoder, "Samples (Q#)", samples, ".qs");
  populateSamplesFolder(
    vfs,
    encoder,
    "Samples (OpenQASM)",
    openqasm_samples,
    ".qasm",
  );

  vfs.writeFile(
    playgroundRootUri.with({ path: "/README.md" }),
    encoder.encode(playgroundReadme),
    { create: true, overwrite: true },
  );
}

function populateSamplesFolder(
  vfs: MemFS,
  encoder: TextEncoder,
  folder: string,
  samples: SampleTy[],
  extension: string,
) {
  vfs.createDirectory(playgroundRootUri.with({ path: `/${folder}` }));
  samples.forEach((sample) => {
    const sanitized_file_name = sample.title.replace(/\W/g, ""); // Remove non-word characters
    vfs.writeFile(
      playgroundRootUri.with({
        path: `/${folder}/${sanitized_file_name}${extension}`,
      }),
      encoder.encode(sample.code),
      { create: true, overwrite: true },
    );
  });
}

export async function initFileSystem(context: vscode.ExtensionContext) {
  const vfs = new MemFS();
  populateSamples(vfs);

  context.subscriptions.push(
    vscode.workspace.registerFileSystemProvider(scheme, vfs, {
      isCaseSensitive: true,
      isReadonly: false,
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.openPlayground`,
      async () => {
        await vscode.commands.executeCommand(
          "vscode.openFolder",
          playgroundRootUri,
        );
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.webOpener`,
      async (uri) => {
        log.debug(`webOpener called with URI ${uri}`);

        // Open the README if the user has navigated to the playground
        if (typeof uri === "string" && uri.endsWith("/playground/")) {
          // Nice to have: First check if the readme is already open from a prior visit
          await vscode.commands.executeCommand(
            "markdown.showPreview",
            playgroundRootUri.with({ path: "/README.md" }),
          );
          return;
        }

        // Example: https://vscode.dev/quantum?code=H4sIAAAAAAAAEz2Ouw6CQBRE%2B%2F2KITbQSI%2BNhRYWhkc0FoTiBm5kE9kld3c1xPDvEjSe8mQmM2mKS68dWtsxXuTgehLu8NQEwrU6RZFShgZ2I7WM81QGMj4Mhdi70IC3UljYH42XqbDa%2BDhZjR1ZyGtrcCZt4gQZKnbh4etmKeFHusznhzzDTbRnTDYIys33Tc%2FC239S2AcxqJvdqmY1qw8FRbBxvAAAAA%3D%3D
        let linkedCode: string | undefined;
        if (typeof uri === "string") {
          const uriObj = vscode.Uri.parse(uri);
          log.debug("uri query component: " + uriObj.query);

          // The query appears to be URIDecoded already, which is causing issues with URLSearchParams, so extract with a regex for now.
          const code = uriObj.query.match(/code=([^&]*)/)?.[1];

          if (code) {
            log.debug("code from query: " + code);
            try {
              linkedCode = await compressedBase64ToCode(code);
              const codeFile = vscode.Uri.parse(`${scheme}:/code.qs`);

              const encoder = new TextEncoder();
              vfs.writeFile(codeFile, encoder.encode(linkedCode), {
                create: true,
                overwrite: true,
              });
              await vscode.commands.executeCommand("vscode.open", codeFile);
              await vscode.commands.executeCommand(
                `${qsharpExtensionId}.showHelp`,
              );
            } catch (err) {
              log.warn("Unable to decode the code in the URL. ", err);
            }
          }
        }
      },
    ),
  );
}

// basename and dirname are only called with a vscode.uri 'path', which should be a well-formed posix path
// Below tested to align with how NodeJS path.basename and path.dirname work
function basename(path: string) {
  path = path.replace(/\/+$/, "");
  if (!path) {
    return "";
  }
  return path.substring(path.lastIndexOf("/") + 1);
}

function dirname(path: string) {
  if (!path) return ".";
  if (path === "/") return "/";

  path = path.replace(/\/+$/, "");
  const offset = path.lastIndexOf("/");

  switch (offset) {
    case -1:
      return ".";
    case 0:
      return "/";
    default:
      return path.substring(0, offset);
  }
}

// The below largely taken from the reference implementation at
// https://github.com/microsoft/vscode-extension-samples/blob/main/fsprovider-sample/src/fileSystemProvider.ts
// with a few additions (e.g. handling 'authority').
export class File implements vscode.FileStat {
  type: vscode.FileType;
  ctime: number;
  mtime: number;
  size: number;

  name: string;
  data?: Uint8Array;

  constructor(name: string) {
    this.type = vscode.FileType.File;
    this.ctime = Date.now();
    this.mtime = Date.now();
    this.size = 0;
    this.name = name;
  }
}

export class Directory implements vscode.FileStat {
  type: vscode.FileType;
  ctime: number;
  mtime: number;
  size: number;

  name: string;
  entries: Map<string, File | Directory>;

  constructor(name: string) {
    this.type = vscode.FileType.Directory;
    this.ctime = Date.now();
    this.mtime = Date.now();
    this.size = 0;
    this.name = name;
    this.entries = new Map();
  }
}

export type Entry = File | Directory;

export class MemFS implements vscode.FileSystemProvider {
  authorities = new Map([["", new Directory("")]]);

  addAuthority(authority: string) {
    this.authorities.set(authority, new Directory(""));
  }

  stat(uri: vscode.Uri): vscode.FileStat {
    log.debug(`stat: ${uri.path}`);
    return this._lookup(uri, false);
  }

  readDirectory(uri: vscode.Uri): [string, vscode.FileType][] {
    log.debug(`readDirectory: ${uri.path}`);
    const entry = this._lookupAsDirectory(uri, false);
    const result: [string, vscode.FileType][] = [];
    for (const [name, child] of entry.entries) {
      result.push([name, child.type]);
    }
    return result;
  }

  readFile(uri: vscode.Uri): Uint8Array {
    log.debug("readFile: " + uri.path);
    const data = this._lookupAsFile(uri, false).data;
    if (data) {
      return data;
    }
    throw vscode.FileSystemError.FileNotFound();
  }

  writeFile(
    uri: vscode.Uri,
    content: Uint8Array,
    options: { create: boolean; overwrite: boolean },
  ): void {
    log.debug("writeFile: " + uri.path);

    const baseName = basename(uri.path);
    const parent = this._lookupParentDirectory(uri);
    let entry = parent.entries.get(baseName);
    if (entry instanceof Directory) {
      throw vscode.FileSystemError.FileIsADirectory(uri);
    }
    if (!entry && !options.create) {
      throw vscode.FileSystemError.FileNotFound(uri);
    }
    if (entry && options.create && !options.overwrite) {
      throw vscode.FileSystemError.FileExists(uri);
    }
    if (!entry) {
      entry = new File(baseName);
      parent.entries.set(baseName, entry);
      this._fireSoon({ type: vscode.FileChangeType.Created, uri });
    }
    entry.mtime = Date.now();
    entry.size = content.byteLength;
    entry.data = content;

    this._fireSoon({ type: vscode.FileChangeType.Changed, uri });
  }

  rename(
    oldUri: vscode.Uri,
    newUri: vscode.Uri,
    options: { overwrite: boolean },
  ): void {
    log.debug(`rename: ${oldUri.path} to ${newUri.path}`);

    if (!options.overwrite && this._lookup(newUri, true)) {
      throw vscode.FileSystemError.FileExists(newUri);
    }

    const entry = this._lookup(oldUri, false);
    const oldParent = this._lookupParentDirectory(oldUri);

    const newParent = this._lookupParentDirectory(newUri);
    const newName = basename(newUri.path);

    oldParent.entries.delete(entry.name);
    entry.name = newName;
    newParent.entries.set(newName, entry);

    this._fireSoon(
      { type: vscode.FileChangeType.Deleted, uri: oldUri },
      { type: vscode.FileChangeType.Created, uri: newUri },
    );
  }

  delete(uri: vscode.Uri): void {
    log.debug(`delete: ${uri.path}`);

    const dirName = uri.with({ path: dirname(uri.path) });
    const baseName = basename(uri.path);
    const parent = this._lookupAsDirectory(dirName, false);
    if (!parent.entries.has(baseName)) {
      throw vscode.FileSystemError.FileNotFound(uri);
    }
    parent.entries.delete(baseName);
    parent.mtime = Date.now();
    parent.size -= 1;
    this._fireSoon(
      { type: vscode.FileChangeType.Changed, uri: dirName },
      { uri, type: vscode.FileChangeType.Deleted },
    );
  }

  createDirectory(uri: vscode.Uri): void {
    log.debug(`createDirectory: ${uri.path}`);

    const baseName = basename(uri.path);
    const dirName = uri.with({ path: dirname(uri.path) });
    const parent = this._lookupAsDirectory(dirName, false);

    const entry = new Directory(baseName);
    parent.entries.set(entry.name, entry);
    parent.mtime = Date.now();
    parent.size += 1;
    this._fireSoon(
      { type: vscode.FileChangeType.Changed, uri: dirName },
      { type: vscode.FileChangeType.Created, uri },
    );
  }

  private _lookup(uri: vscode.Uri, silent: false): Entry;
  private _lookup(uri: vscode.Uri, silent: boolean): Entry | undefined;
  private _lookup(uri: vscode.Uri, silent: boolean): Entry | undefined {
    const parts = uri.path.split("/");
    let entry: Entry | undefined = this.authorities.get(uri.authority);
    if (!entry) {
      throw vscode.FileSystemError.FileNotFound(uri);
    }

    for (const part of parts) {
      if (!part) {
        continue;
      }
      let child: Entry | undefined;
      if (entry instanceof Directory) {
        child = entry.entries.get(part);
      }
      if (!child) {
        if (!silent) {
          throw vscode.FileSystemError.FileNotFound(uri);
        } else {
          return undefined;
        }
      }
      entry = child;
    }
    return entry;
  }

  private _lookupAsDirectory(uri: vscode.Uri, silent: boolean): Directory {
    const entry = this._lookup(uri, silent);
    if (entry instanceof Directory) {
      return entry;
    }
    throw vscode.FileSystemError.FileNotADirectory(uri);
  }

  private _lookupAsFile(uri: vscode.Uri, silent: boolean): File {
    const entry = this._lookup(uri, silent);
    if (entry instanceof File) {
      return entry;
    }
    throw vscode.FileSystemError.FileIsADirectory(uri);
  }

  private _lookupParentDirectory(uri: vscode.Uri): Directory {
    const dirName = uri.with({ path: dirname(uri.path) });
    return this._lookupAsDirectory(dirName, false);
  }

  private _emitter = new vscode.EventEmitter<vscode.FileChangeEvent[]>();
  private _bufferedEvents: vscode.FileChangeEvent[] = [];
  private _fireSoonHandle?: any;

  readonly onDidChangeFile: vscode.Event<vscode.FileChangeEvent[]> =
    this._emitter.event;

  watch(): vscode.Disposable {
    // NOTE: Docs for this API state, "It is the file system provider's job to
    // call onDidChangeFile for every change given these rules. No event should
    // be emitted for files that match any of the provided excludes.". But this
    // implementation just fires on every change (see below). However most of the
    // other implementations I've seen do the same, so assume this is harmless.
    return new vscode.Disposable(() => {
      return;
    });
  }

  private _fireSoon(...events: vscode.FileChangeEvent[]): void {
    this._bufferedEvents.push(...events);

    if (this._fireSoonHandle) {
      clearTimeout(this._fireSoonHandle);
    }

    this._fireSoonHandle = setTimeout(() => {
      this._emitter.fire(this._bufferedEvents);
      this._bufferedEvents.length = 0;
    }, 5);
  }
}

// Cleanup: This is taken from the playground. It should probably be moved to a common
// location in the npm package and shared between the two at some point.
export async function compressedBase64ToCode(base64: string) {
  // Turn the base64 string into a string of bytes
  const binStr = atob(base64);

  // Turn it into a byte array
  const byteArray = new Uint8Array(binStr.length);
  for (let i = 0; i < binStr.length; ++i) byteArray[i] = binStr.charCodeAt(i);

  // Decompress the bytes
  const decompressor = new DecompressionStream("gzip");
  const writer = decompressor.writable.getWriter();
  writer.write(byteArray);
  writer.close();

  // Read the decompressed stream and turn into a byte string
  const decompressedBuff = await new Response(
    decompressor.readable,
  ).arrayBuffer();

  // Decode the utf-8 bytes into a JavaScript string
  const decoder = new TextDecoder();
  const code = decoder.decode(decompressedBuff);
  return code;
}

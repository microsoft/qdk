// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  JupyterFrontEnd,
  JupyterFrontEndPlugin,
} from "@jupyterlab/application";

import { IEditorLanguageRegistry } from "@jupyterlab/codemirror";
import { simpleMode } from "@codemirror/legacy-modes/mode/simple-mode";
import { LanguageSupport, StreamLanguage } from "@codemirror/language";
import { INotebookTracker, NotebookPanel } from "@jupyterlab/notebook";
import { ICellModel } from "@jupyterlab/cells/lib/model";

const plugin: JupyterFrontEndPlugin<void> = {
  id: "qsharp",
  autoStart: true,
  requires: [IEditorLanguageRegistry, INotebookTracker],
  activate: async (
    app: JupyterFrontEnd,
    codemirrorLanguageRegistry: IEditorLanguageRegistry,
    notebookTracker: INotebookTracker,
  ) => {
    registerQSharpLanguage(codemirrorLanguageRegistry);
    registerQSharpNotebookHandlers(notebookTracker);
  },
};

/**
 * Registers the text/x-qsharp mime type and the .qs file extension
 * and associates them with the qsharp CodeMirror mode.
 */
function registerQSharpLanguage(
  codemirrorLanguageRegistry: IEditorLanguageRegistry,
) {
  const rules = [
    {
      token: "comment",
      regex: /(\/\/).*/,
      beginWord: false,
    },
    {
      token: "string",
      regex: String.raw`^\"(?:[^\"\\]|\\[\s\S])*(?:\"|$)`,
      beginWord: false,
    },
    {
      token: "keyword",
      regex: String.raw`(namespace|open|import|export|as|operation|function|body|adjoint|newtype|struct|new|controlled|internal)\b`,
      beginWord: true,
    },
    {
      token: "keyword",
      regex: String.raw`(if|elif|else|repeat|until|fixup|for|in|while|break|continue|return|fail|within|apply)\b`,
      beginWord: true,
    },
    {
      token: "keyword",
      regex: String.raw`(Adjoint|Controlled|Adj|Ctl|is|self|auto|distribute|invert|intrinsic)\b`,
      beginWord: true,
    },
    {
      token: "keyword",
      regex: String.raw`(let|set|use|borrow|mutable)\b`,
      beginWord: true,
    },
    {
      token: "operatorKeyword",
      regex: String.raw`(not|and|or)\b|(w/)`,
      beginWord: true,
    },
    {
      token: "operatorKeyword",
      regex: String.raw`(=)|(!)|(<)|(>)|(\+)|(-)|(\*)|(/)|(\^)|(%)|(\|)|(&&&)|(~~~)|(\.\.\.)|(\.\.)|(\?)`,
      beginWord: false,
    },
    {
      token: "meta",
      regex: String.raw`(Int|BigInt|Double|Bool|Qubit|Pauli|Result|Range|String|Unit)\b`,
      beginWord: true,
    },
    {
      token: "atom",
      regex: String.raw`(true|false|Pauli(I|X|Y|Z)|One|Zero)\b`,
      beginWord: true,
    },
  ];
  const simpleRules = [];
  for (const rule of rules) {
    simpleRules.push({
      token: rule.token,
      regex: new RegExp(rule.regex, "g"),
      sol: rule.beginWord,
    });
    if (rule.beginWord) {
      // Need an additional rule due to the fact that CodeMirror simple mode doesn't work with ^ token
      simpleRules.push({
        token: rule.token,
        regex: new RegExp(String.raw`\W` + rule.regex, "g"),
        sol: false,
      });
    }
  }

  const parser = simpleMode({ start: simpleRules });
  const languageSupport = new LanguageSupport(StreamLanguage.define(parser));
  codemirrorLanguageRegistry.addLanguage({
    name: "qsharp",
    mime: "text/x-qsharp",
    support: languageSupport,
    extensions: ["qs"],
  });
}

/**
 * Sets up handlers to detect Q# code cells in Python notebooks and set the language to Q#.
 */
function registerQSharpNotebookHandlers(notebookTracker: INotebookTracker) {
  notebookTracker.forEach((notebookPanel) => {
    // When the application is first loaded
    watchAndSetLanguageForQsharpCells(notebookPanel);
  });

  notebookTracker.widgetAdded.connect((notebookTracker, notebookPanel) => {
    // When a new notebook editor is opened
    watchAndSetLanguageForQsharpCells(notebookPanel);
  });
}

function watchAndSetLanguageForQsharpCells(notebookPanel: NotebookPanel) {
  notebookPanel.revealed.then(() => {
    const notebook = notebookPanel.model;

    if (notebook?.defaultKernelName === "python3") {
      for (const cell of notebook.cells) {
        // When notebook cells are first loaded
        setLanguageIfCellIsQSharp(cell);
      }

      notebook.cells.changed.connect((cellList, changedCells) => {
        changedCells.newValues.forEach((cell) => {
          // When a new cell is added
          setLanguageIfCellIsQSharp(cell);
          cell.contentChanged.connect((cell) => {
            // When cell contents are updated
            setLanguageIfCellIsQSharp(cell);
          });
        });
      });
    }
  });
}

function setLanguageIfCellIsQSharp(cell: ICellModel) {
  if (cell.type === "code") {
    if (cell.sharedModel.source.startsWith("%%qsharp")) {
      if (cell.mimeType !== "text/x-qsharp") {
        cell.mimeType = "text/x-qsharp";
        console.log("updated cell mime type to text/x-qsharp");
      }
    }
  }
}

export default plugin;

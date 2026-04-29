// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// This file provides CodeMirror syntax highlighting for Q# magic cells
// in classic Jupyter Notebooks. It does nothing in other (Jupyter Notebook 7,
// VS Code, Azure Notebooks, etc.) environments.

// Detect the prerequisites and do nothing if they don't exist.
if (window.require && window.CodeMirror && window.Jupyter) {
  // The simple mode plugin for CodeMirror is not loaded by default, so require it.
  window.require(["codemirror/addon/mode/simple"], function defineMode() {
    let rules = [
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
        regex: String.raw`(namespace|open|as|operation|function|body|adjoint|newtype|controlled|internal)\b`,
        beginWord: true,
      },
      {
        token: "keyword",
        regex: String.raw`(if|elif|else|repeat|until|fixup|for|in|return|fail|within|apply)\b`,
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
    let simpleRules = [];
    for (let rule of rules) {
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

    // Register the mode defined above with CodeMirror
    window.CodeMirror.defineSimpleMode("qsharp", { start: simpleRules });
    window.CodeMirror.defineMIME("text/x-qsharp", "qsharp");

    // Tell Jupyter to associate %%qsharp magic cells with the qsharp mode
    window.Jupyter.CodeCell.options_default.highlight_modes["qsharp"] = {
      reg: [/^%%qsharp/],
    };

    // Force re-highlighting of all cells the first time this code runs
    for (const cell of window.Jupyter.notebook.get_cells()) {
      cell.auto_highlight();
    }
  });
}

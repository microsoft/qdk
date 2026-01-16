// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useEffect, useRef } from "preact/hooks";

declare const resourcesUri: string; // Set by the HTML in the window

let svgPromise: Promise<Response>;

export function HelpPage() {
  // Need to load the SVG resource async and put it into the div.
  // You can't use an img tag for the SVG, as parent styles don't apply.
  const svgRef = useRef<HTMLDivElement>(null);

  // Ensure that the fetch is kicked off once for the module
  if (!svgPromise) {
    svgPromise = fetch(`${resourcesUri}/DebugDropDown.svg`);
  }

  useEffect(() => {
    // Once render is complete, load the SVG (when ready) into the div
    svgPromise
      .then((response) => response.text())
      .then((text) => {
        if (svgRef.current) {
          svgRef.current.innerHTML = text;
        }
      });
  });

  return (
    <div class="qs-help">
      <h1>Microsoft Quantum Development Kit</h1>
      <h2>Overview</h2>
      <p>
        Welcome to the Microsoft Quantum Development Kit. Below you will find a
        quick overview of some of the features it enables in Visual Studio Code.
      </p>
      <h2>Powerful Q# editing</h2>
      <p>
        The Q# language is designed for maximum expressivity and productivity
        when writing quantum code. Its type system allows for rich editor
        features such as completion lists, signature help, go to definition,
        find all references, renaming of functions and variables, and more.
      </p>
      <h2>Quantum simulation</h2>
      <div style="float: right; width: 200px; margin: 6px" ref={svgRef}></div>
      <p>
        The built-in quantum simulator enables you to run your Q# code directly
        in VS Code. Use the 'Play' icon at the top right of the editor, (or the
        Ctrl+F5 keyboard shortcut), and the output from the simulator will
        appear in the Debug Console of Visual Studio Code. Use the "Q#: Show
        histogram" command to run a number of shots and display the results as a
        histogram.
      </p>
      <h2>Debugging Q# code</h2>
      <p>
        The quantum simulator also supports debugging. Using the 'Play' icon
        drop-down at the top right of the editor select "Debug Q# file", (or use
        the F5 keyboard shortcut). You can set breakpoint and step through code,
        viewing both the classical and quantum state.
      </p>
      <h2>Run jobs on real quantum hardware</h2>
      <p>
        If you have an Azure subscription, you can connect to your Azure Quantum
        workspace and run your Q# programs directly on real quantum machines
        using the 'Quantum Workspaces' view in the Explorer sidebar.
      </p>
      <h2>Learn more</h2>
      <p>
        You can find out more about the latest features in the Microsoft Quantum
        Development Kit by visiting&nbsp;
        <a href="https://aka.ms/qdk">https://aka.ms/qdk.</a>
        .&nbsp;Happy coding!
      </p>
    </div>
  );
}

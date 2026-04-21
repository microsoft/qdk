// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { getCompiler, loadWasmModule } from "qsharp-lang";
import { QscEventTarget } from "qsharp-lang/dist/compiler/events.js";
import type {
  ICompiler,
  ProgramConfig,
} from "qsharp-lang/dist/compiler/compiler.js";
import type { CircuitGroup } from "qsharp-lang/dist/data-structures/circuit.js";
import type {
  RunResult,
  RunEvent,
  SolutionCheckResult,
  CircuitResult,
  EstimateResult,
  NoiseConfig,
} from "./types.js";

export class CompilerService {
  private compiler: ICompiler | null = null;
  private static wasmLoadPromise: Promise<void> | null = null;

  private static ensureWasmLoaded(): Promise<void> {
    if (!CompilerService.wasmLoadPromise) {
      CompilerService.wasmLoadPromise = (async () => {
        const require = createRequire(import.meta.url);
        // Resolve the nodejs-targeted wasm file via the qsharp-lang package
        // (works through the npm workspace symlink too).
        const wasmPath =
          require.resolve("qsharp-lang/lib/nodejs/qsc_wasm_bg.wasm");
        const buffer = await readFile(wasmPath);
        // Pass the underlying ArrayBuffer (detached from the Node Buffer).
        await loadWasmModule(
          buffer.buffer.slice(
            buffer.byteOffset,
            buffer.byteOffset + buffer.byteLength,
          ) as ArrayBuffer,
        );
      })();
    }
    return CompilerService.wasmLoadPromise;
  }

  private async getCompilerInstance(): Promise<ICompiler> {
    if (!this.compiler) {
      await CompilerService.ensureWasmLoaded();
      this.compiler = await getCompiler();
    }
    return this.compiler;
  }

  private makeProgramConfig(sources: [string, string][]): ProgramConfig {
    return {
      sources,
      languageFeatures: [],
      profile: "unrestricted",
    };
  }

  private collectRunResult(
    eventTarget: InstanceType<typeof QscEventTarget>,
  ): RunResult {
    const shots = eventTarget.getResults();
    const events: RunEvent[] = [];
    let success = true;
    let result: string | undefined;
    let error: string | undefined;

    for (const shot of shots) {
      success = success && shot.success;
      if (typeof shot.result === "string") {
        result = shot.result;
      } else {
        // Error result
        const errors = shot.result.errors ?? [];
        error =
          errors.map((e: any) => e.message ?? String(e)).join("\n") ||
          shot.result.message;
      }
      for (const event of shot.events) {
        switch (event.type) {
          case "Message":
            events.push({ type: "message", message: event.message });
            break;
          case "DumpMachine":
            events.push({
              type: "dump",
              dump: {
                state: event.state,
                stateLatex: event.stateLatex,
                qubitCount: event.qubitCount,
              },
            });
            break;
          case "Matrix":
            events.push({
              type: "matrix",
              matrix: {
                matrix: event.matrix,
                matrixLatex: event.matrixLatex,
              },
            });
            break;
        }
      }
    }

    return { success, events, result, error };
  }

  async run(
    sources: [string, string][],
    shots: number = 1,
  ): Promise<RunResult> {
    const compiler = await this.getCompilerInstance();
    const program = this.makeProgramConfig(sources);
    const eventTarget = new QscEventTarget(true);
    try {
      await compiler.run(program, "", shots, eventTarget);
      return this.collectRunResult(eventTarget);
    } catch (err: unknown) {
      return {
        success: false,
        events: [],
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  async runWithNoise(
    sources: [string, string][],
    shots: number = 100,
    noise: NoiseConfig = { pauliNoise: [0.001, 0.001, 0.001], qubitLoss: 0.0 },
  ): Promise<RunResult> {
    const compiler = await this.getCompilerInstance();
    const program = this.makeProgramConfig(sources);
    const eventTarget = new QscEventTarget(true);
    try {
      await compiler.runWithNoise(
        program,
        "",
        shots,
        noise.pauliNoise,
        noise.qubitLoss,
        eventTarget,
      );
      return this.collectRunResult(eventTarget);
    } catch (err: unknown) {
      return {
        success: false,
        events: [],
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  async getCircuit(sources: [string, string][]): Promise<CircuitResult> {
    const compiler = await this.getCompilerInstance();
    const program = this.makeProgramConfig(sources);
    const circuitData = await compiler.getCircuit(program, {
      generationMethod: "simulate",
      maxOperations: 1000,
      sourceLocations: false,
      groupByScope: false,
    });
    return {
      circuit: circuitData,
      ascii: renderCircuitAscii(circuitData),
    };
  }

  async getResourceEstimate(
    sources: [string, string][],
  ): Promise<EstimateResult> {
    const compiler = await this.getCompilerInstance();
    const program: ProgramConfig = {
      sources,
      languageFeatures: [],
      profile: "unrestricted",
    };
    const rawJson = await compiler.getEstimates(program, "", "[{}]");
    const rawParsed = JSON.parse(rawJson);
    const raw = (Array.isArray(rawParsed) ? rawParsed[0] : rawParsed) as Record<
      string,
      unknown
    >;
    // Extract top-level physical counts
    const physicalCounts = (raw["physicalCounts"] ?? raw) as Record<
      string,
      unknown
    >;
    const physicalQubits = (physicalCounts["physicalQubits"] as number) ?? 0;
    const runtime = String(physicalCounts["runtime"] ?? "N/A");
    return { physicalQubits, runtime, raw };
  }

  async checkSolution(
    userCode: string,
    exerciseSources: string[],
  ): Promise<SolutionCheckResult> {
    const compiler = await this.getCompilerInstance();
    const eventTarget = new QscEventTarget(true);
    try {
      const passed = await compiler.checkExerciseSolution(
        userCode,
        exerciseSources,
        eventTarget,
      );
      const result = this.collectRunResult(eventTarget);
      return {
        passed,
        events: result.events,
        error: result.error,
      };
    } catch (err: unknown) {
      return {
        passed: false,
        events: [],
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }
}

// ─── ASCII circuit renderer ───

function renderCircuitAscii(data: CircuitGroup): string {
  if (!data.circuits || data.circuits.length === 0) {
    return "(empty circuit)";
  }
  const circuit = data.circuits[0];
  const qubits = circuit.qubits;
  const grid = circuit.componentGrid;

  if (qubits.length === 0) return "(no qubits)";

  // Build wire labels
  const wireLabels = qubits.map((q: any) => `q${q.id}`);
  const labelWidth = Math.max(...wireLabels.map((l: string) => l.length)) + 1;

  // Build columns of gate labels per qubit
  const columns: string[][] = [];
  for (const col of grid) {
    const colGates = new Array<string>(qubits.length).fill("───");
    for (const op of col.components) {
      const gate = op.gate || "?";
      let label = gate;
      if ("isAdjoint" in op && op.isAdjoint) label += "†";
      if (op.args && op.args.length > 0) label += `(${op.args.join(",")})`;

      // Determine which qubit rows this gate touches
      const targetRows: number[] = [];
      if ("targets" in op) {
        for (const reg of op.targets) {
          targetRows.push(reg.qubit);
        }
      }
      if ("qubits" in op) {
        for (const reg of op.qubits) {
          targetRows.push(reg.qubit);
        }
      }
      const controlRows: number[] = [];
      if ("controls" in op && op.controls) {
        for (const reg of op.controls) {
          controlRows.push(reg.qubit);
        }
      }

      for (const row of targetRows) {
        if (row < qubits.length) colGates[row] = `[${label}]`;
      }
      for (const row of controlRows) {
        if (row < qubits.length) colGates[row] = `──●──`;
      }
    }
    columns.push(colGates);
  }

  // Render each qubit wire
  const lines: string[] = [];
  for (let q = 0; q < qubits.length; q++) {
    const label = wireLabels[q].padEnd(labelWidth);
    const segments = columns.map((col) => {
      const g = col[q];
      const padded = g.length < 7 ? g.padEnd(7, "─") : g;
      return padded;
    });
    lines.push(`${label}─${segments.join("─")}─`);
  }

  return lines.join("\n");
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { SingleEstimateResult } from "./data.js";

type ReportData = {
  groups: ReportGroup[];
  assumptions: string[];
};

type ReportGroup = {
  title: string;
  alwaysVisible: boolean;
  entries: ReportEntry[];
};

type ReportEntry = {
  path: string;
  label: string;
  description: string;
  explanation: string;
};

// THIS CODE HAS BEEN AUTOMATICALLY GENERATED WITH generate_report_code.py from output_data.md
export function CreateReport(result: SingleEstimateResult): ReportData {
  const groups = [] as ReportGroup[];
  let entries: ReportEntry[];
  const numberFormat = new Intl.NumberFormat();
  const numberFormatF64 = new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 2,
    minimumFractionDigits: 2,
  });

  entries = [];
  entries.push({
    path: "physicalCountsFormatted/runtime",
    label: "Runtime",
    description: `Total runtime`,
    explanation: `This is a runtime estimate for the execution time of the algorithm. In general, the execution time corresponds to the duration of one logical cycle (${numberFormat.format(
      result.logicalQubit.logicalCycleTime,
    )} nanosecs) multiplied by the ${numberFormat.format(
      result.physicalCounts.breakdown.algorithmicLogicalDepth,
    )} logical cycles to run the algorithm. If however the duration of a single T factory (here: ${numberFormat.format(
      result.tfactory == null ? 0 : result.tfactory.runtime,
    )} nanosecs) is larger than the algorithm runtime, we extend the number of logical cycles artificially in order to exceed the runtime of a single T factory.`,
  });
  entries.push({
    path: "physicalCountsFormatted/rqops",
    label: "rQOPS",
    description: `Reliable quantum operations per second`,
    explanation: `The value is computed as the number of logical qubits after layout (${numberFormat.format(
      result.physicalCounts.breakdown.algorithmicLogicalQubits,
    )}) (with a logical error rate of ${
      result.physicalCountsFormatted.requiredLogicalQubitErrorRate
    }) multiplied by the clock frequency (${numberFormatF64.format(
      result.physicalCounts.breakdown.clockFrequency,
    )}), which is the number of logical cycles per second.`,
  });
  entries.push({
    path: "physicalCountsFormatted/physicalQubits",
    label: "Physical qubits",
    description: `Number of physical qubits`,
    explanation: `This value represents the total number of physical qubits, which is the sum of ${numberFormat.format(
      result.physicalCounts.breakdown.physicalQubitsForAlgorithm,
    )} physical qubits to implement the algorithm logic, and ${numberFormat.format(
      result.physicalCounts.breakdown.physicalQubitsForTfactories,
    )} physical qubits to execute the T factories that are responsible to produce the T states that are consumed by the algorithm.`,
  });
  groups.push({
    title: "Physical resource estimates",
    alwaysVisible: true,
    entries: entries,
  });

  entries = [];
  entries.push({
    path: "physicalCountsFormatted/algorithmicLogicalQubits",
    label: "Logical algorithmic qubits",
    description: `Number of logical qubits for the algorithm after layout`,
    explanation: `Laying out the logical qubits in the presence of nearest-neighbor constraints requires additional logical qubits. In particular, to layout the $Q_{\\rm alg} = ${numberFormat.format(
      result.logicalCounts.numQubits,
    )}$ logical qubits in the input algorithm, we require in total $2 \\cdot Q_{\\rm alg} + \\lceil \\sqrt{8 \\cdot Q_{\\rm alg}}\\rceil + 1 = ${numberFormat.format(
      result.physicalCounts.breakdown.algorithmicLogicalQubits,
    )}$ logical qubits.`,
  });
  entries.push({
    path: "physicalCountsFormatted/algorithmicLogicalDepth",
    label: "Algorithmic depth",
    description: `Number of logical cycles for the algorithm`,
    explanation: `To execute the algorithm using _Parallel Synthesis Sequential Pauli Computation_ (PSSPC), operations are scheduled in terms of multi-qubit Pauli measurements, for which assume an execution time of one logical cycle. Based on the input algorithm, we require one multi-qubit measurement for the ${numberFormat.format(
      result.logicalCounts.measurementCount,
    )} single-qubit measurements, the ${numberFormat.format(
      result.logicalCounts.rotationCount,
    )} arbitrary single-qubit rotations, and the ${numberFormat.format(
      result.logicalCounts.tCount,
    )} T gates, three multi-qubit measurements for each of the ${numberFormat.format(
      result.logicalCounts.cczCount,
    )} CCZ and ${numberFormat.format(
      result.logicalCounts.ccixCount,
    )} CCiX gates in the input program, as well as ${
      result.physicalCountsFormatted.numTsPerRotation
    } multi-qubit measurements for each of the ${numberFormat.format(
      result.logicalCounts.rotationDepth,
    )} non-Clifford layers in which there is at least one single-qubit rotation with an arbitrary angle rotation.`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalDepth",
    label: "Logical depth",
    description: `Number of logical cycles performed`,
    explanation: `This number is usually equal to the logical depth of the algorithm, which is ${numberFormat.format(
      result.physicalCounts.breakdown.algorithmicLogicalDepth,
    )}. However, in the case in which a single T factory is slower than the execution time of the algorithm, we adjust the logical cycle depth to exceed the T factory's execution time.`,
  });
  entries.push({
    path: "physicalCountsFormatted/clockFrequency",
    label: "Clock frequency",
    description: `Number of logical cycles per second`,
    explanation: `This is the number of logical cycles that can be performed within one second. The logical cycle time is ${result.physicalCountsFormatted.logicalCycleTime}.`,
  });
  entries.push({
    path: "physicalCountsFormatted/numTstates",
    label: "Number of T states",
    description: `Number of T states consumed by the algorithm`,
    explanation: `To execute the algorithm, we require one T state for each of the ${numberFormat.format(
      result.logicalCounts.tCount,
    )} T gates, four T states for each of the ${numberFormat.format(
      result.logicalCounts.cczCount,
    )} CCZ and ${numberFormat.format(
      result.logicalCounts.ccixCount,
    )} CCiX gates, as well as ${
      result.physicalCountsFormatted.numTsPerRotation
    } for each of the ${numberFormat.format(
      result.logicalCounts.rotationCount,
    )} single-qubit rotation gates with arbitrary angle rotation.`,
  });
  entries.push({
    path: "physicalCountsFormatted/numTfactories",
    label: "Number of T factories",
    description: `Number of T factories capable of producing the demanded ${numberFormat.format(
      result.physicalCounts.breakdown.numTstates,
    )} T states during the algorithm's runtime`,
    explanation: `The total number of T factories ${numberFormat.format(
      result.physicalCounts.breakdown.numTfactories,
    )} that are executed in parallel is computed as $\\left\\lceil\\dfrac{\\text{T states}\\cdot\\text{T factory duration}}{\\text{T states per T factory}\\cdot\\text{algorithm runtime}}\\right\\rceil = \\left\\lceil\\dfrac{${numberFormat.format(
      result.physicalCounts.breakdown.numTstates,
    )} \\cdot ${numberFormat.format(
      result.tfactory == null ? 0 : result.tfactory.runtime,
    )}\\;\\text{ns}}{${numberFormat.format(
      result.tfactory == null ? 0 : result.tfactory.numTstates,
    )} \\cdot ${numberFormat.format(
      result.physicalCounts.runtime,
    )}\\;\\text{ns}}\\right\\rceil$`,
  });
  entries.push({
    path: "physicalCountsFormatted/numTfactoryRuns",
    label: "Number of T factory invocations",
    description: `Number of times all T factories are invoked`,
    explanation: `In order to prepare the ${numberFormat.format(
      result.physicalCounts.breakdown.numTstates,
    )} T states, the ${numberFormat.format(
      result.physicalCounts.breakdown.numTfactories,
    )} copies of the T factory are repeatedly invoked ${numberFormat.format(
      result.physicalCounts.breakdown.numTfactoryRuns,
    )} times.`,
  });
  entries.push({
    path: "physicalCountsFormatted/physicalQubitsForAlgorithm",
    label: "Physical algorithmic qubits",
    description: `Number of physical qubits for the algorithm after layout`,
    explanation: `The ${numberFormat.format(
      result.physicalCounts.breakdown.physicalQubitsForAlgorithm,
    )} are the product of the ${numberFormat.format(
      result.physicalCounts.breakdown.algorithmicLogicalQubits,
    )} logical qubits after layout and the ${numberFormat.format(
      result.logicalQubit.physicalQubits,
    )} physical qubits that encode a single logical qubit.`,
  });
  entries.push({
    path: "physicalCountsFormatted/physicalQubitsForTfactories",
    label: "Physical T factory qubits",
    description: `Number of physical qubits for the T factories`,
    explanation: `Each T factory requires ${numberFormat.format(
      result.tfactory == null ? 0 : result.tfactory.physicalQubits,
    )} physical qubits and we run ${numberFormat.format(
      result.physicalCounts.breakdown.numTfactories,
    )} in parallel, therefore we need $${numberFormat.format(
      result.physicalCounts.breakdown.physicalQubitsForTfactories,
    )} = ${numberFormat.format(
      result.tfactory == null ? 0 : result.tfactory.physicalQubits,
    )} \\cdot ${numberFormat.format(
      result.physicalCounts.breakdown.numTfactories,
    )}$ qubits.`,
  });
  entries.push({
    path: "physicalCountsFormatted/requiredLogicalQubitErrorRate",
    label: "Required logical qubit error rate",
    description: `The minimum logical qubit error rate required to run the algorithm within the error budget`,
    explanation: `The minimum logical qubit error rate is obtained by dividing the logical error probability ${
      result.physicalCountsFormatted.errorBudgetLogical
    } by the product of ${numberFormat.format(
      result.physicalCounts.breakdown.algorithmicLogicalQubits,
    )} logical qubits and the total cycle count ${numberFormat.format(
      result.physicalCounts.breakdown.logicalDepth,
    )}.`,
  });
  entries.push({
    path: "physicalCountsFormatted/requiredLogicalTstateErrorRate",
    label: "Required logical T state error rate",
    description: `The minimum T state error rate required for distilled T states`,
    explanation: `The minimum T state error rate is obtained by dividing the T distillation error probability ${
      result.physicalCountsFormatted.errorBudgetTstates
    } by the total number of T states ${numberFormat.format(
      result.physicalCounts.breakdown.numTstates,
    )}.`,
  });
  entries.push({
    path: "physicalCountsFormatted/numTsPerRotation",
    label: "Number of T states per rotation",
    description: `Number of T states to implement a rotation with an arbitrary angle`,
    explanation: `The number of T states to implement a rotation with an arbitrary angle is $\\lceil 0.53 \\log_2(${numberFormat.format(
      result.logicalCounts.rotationCount,
    )} / ${
      result.errorBudget.rotations
    }) + 4.86\\rceil$ [[arXiv:2203.10064](https://arxiv.org/abs/2203.10064)]. For simplicity, we use this formula for all single-qubit arbitrary angle rotations, and do not distinguish between best, worst, and average cases.`,
  });
  groups.push({
    title: "Resource estimates breakdown",
    alwaysVisible: false,
    entries: entries,
  });

  entries = [];
  entries.push({
    path: "jobParams/qecScheme/name",
    label: "QEC scheme",
    description: `Name of QEC scheme`,
    explanation: `You can load pre-defined QEC schemes by using the name \`surface_code\` or \`floquet_code\`. The latter only works with Majorana qubits.`,
  });
  entries.push({
    path: "logicalQubit/codeDistance",
    label: "Code distance",
    description: `Required code distance for error correction`,
    explanation: `The code distance is the smallest odd integer greater or equal to $\\dfrac{2\\log(${result.jobParams.qecScheme.crossingPrefactor} / ${result.physicalCounts.breakdown.requiredLogicalQubitErrorRate})}{\\log(${result.jobParams.qecScheme.errorCorrectionThreshold}/${result.physicalCounts.breakdown.cliffordErrorRate})} - 1$`,
  });
  entries.push({
    path: "physicalCountsFormatted/physicalQubitsPerLogicalQubit",
    label: "Physical qubits",
    description: `Number of physical qubits per logical qubit`,
    explanation: `The number of physical qubits per logical qubit are evaluated using the formula ${result.jobParams.qecScheme.physicalQubitsPerLogicalQubit} that can be user-specified.`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalCycleTime",
    label: "Logical cycle time",
    description: `Duration of a logical cycle in nanoseconds`,
    explanation: `The runtime of one logical cycle in nanoseconds is evaluated using the formula ${result.jobParams.qecScheme.logicalCycleTime} that can be user-specified.`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalErrorRate",
    label: "Logical qubit error rate",
    description: `Logical qubit error rate`,
    explanation: `The logical qubit error rate is computed as $${result.jobParams.qecScheme.crossingPrefactor} \\cdot \\left(\\dfrac{${result.physicalCounts.breakdown.cliffordErrorRate}}{${result.jobParams.qecScheme.errorCorrectionThreshold}}\\right)^\\frac{${result.logicalQubit.codeDistance} + 1}{2}$`,
  });
  entries.push({
    path: "jobParams/qecScheme/crossingPrefactor",
    label: "Crossing prefactor",
    description: `Crossing prefactor used in QEC scheme`,
    explanation: `The crossing prefactor is usually extracted numerically from simulations when fitting an exponential curve to model the relationship between logical and physical error rate.`,
  });
  entries.push({
    path: "jobParams/qecScheme/errorCorrectionThreshold",
    label: "Error correction threshold",
    description: `Error correction threshold used in QEC scheme`,
    explanation: `The error correction threshold is the physical error rate below which the error rate of the logical qubit is less than the error rate of the physical qubit that constitute it. This value is usually extracted numerically from simulations of the logical error rate.`,
  });
  entries.push({
    path: "jobParams/qecScheme/logicalCycleTime",
    label: "Logical cycle time formula",
    description: `QEC scheme formula used to compute logical cycle time`,
    explanation: `This is the formula that is used to compute the logical cycle time ${numberFormat.format(
      result.logicalQubit.logicalCycleTime,
    )} ns.`,
  });
  entries.push({
    path: "jobParams/qecScheme/physicalQubitsPerLogicalQubit",
    label: "Physical qubits formula",
    description: `QEC scheme formula used to compute number of physical qubits per logical qubit`,
    explanation: `This is the formula that is used to compute the number of physical qubits per logical qubits ${numberFormat.format(
      result.logicalQubit.physicalQubits,
    )}.`,
  });
  groups.push({
    title: "Logical qubit parameters",
    alwaysVisible: false,
    entries: entries,
  });

  if (result.tfactory != null) {
    entries = [];
    entries.push({
      path: "physicalCountsFormatted/tfactoryPhysicalQubits",
      label: "Physical qubits",
      description: `Number of physical qubits for a single T factory`,
      explanation: `This corresponds to the maximum number of physical qubits over all rounds of T distillation units in a T factory. A round of distillation contains of multiple copies of distillation units to achieve the required success probability of producing a T state with the expected logical T state error rate.`,
    });
    entries.push({
      path: "physicalCountsFormatted/tfactoryRuntime",
      label: "Runtime",
      description: `Runtime of a single T factory`,
      explanation: `The runtime of a single T factory is the accumulated runtime of executing each round in a T factory.`,
    });
    entries.push({
      path: "tfactory/numTstates",
      label: "Number of output T states per run",
      description: `Number of output T states produced in a single run of T factory`,
      explanation: `The T factory takes as input ${numberFormat.format(
        result.tfactory == null ? 0 : result.tfactory.numInputTstates,
      )} noisy physical T states with an error rate of ${
        result.jobParams.qubitParams.tGateErrorRate
      } and produces ${numberFormat.format(
        result.tfactory == null ? 0 : result.tfactory.numTstates,
      )} T states with an error rate of ${
        result.physicalCountsFormatted.tstateLogicalErrorRate
      }.`,
    });
    entries.push({
      path: "physicalCountsFormatted/numInputTstates",
      label: "Number of input T states per run",
      description: `Number of physical input T states consumed in a single run of a T factory`,
      explanation: `This value includes the physical input T states of all copies of the distillation unit in the first round.`,
    });
    entries.push({
      path: "tfactory/numRounds",
      label: "Distillation rounds",
      description: `The number of distillation rounds`,
      explanation: `This is the number of distillation rounds. In each round one or multiple copies of some distillation unit is executed.`,
    });
    entries.push({
      path: "physicalCountsFormatted/numUnitsPerRound",
      label: "Distillation units per round",
      description: `The number of units in each round of distillation`,
      explanation: `This is the number of copies for the distillation units per round.`,
    });
    entries.push({
      path: "physicalCountsFormatted/unitNamePerRound",
      label: "Distillation units",
      description: `The types of distillation units`,
      explanation: `These are the types of distillation units that are executed in each round. The units can be either physical or logical, depending on what type of qubit they are operating. Space-efficient units require fewer qubits for the cost of longer runtime compared to Reed-Muller preparation units.`,
    });
    entries.push({
      path: "physicalCountsFormatted/codeDistancePerRound",
      label: "Distillation code distances",
      description: `The code distance in each round of distillation`,
      explanation: `This is the code distance used for the units in each round. If the code distance is 1, then the distillation unit operates on physical qubits instead of error-corrected logical qubits.`,
    });
    entries.push({
      path: "physicalCountsFormatted/physicalQubitsPerRound",
      label: "Number of physical qubits per round",
      description: `The number of physical qubits used in each round of distillation`,
      explanation: `The maximum number of physical qubits over all rounds is the number of physical qubits for the T factory, since qubits are reused by different rounds.`,
    });
    entries.push({
      path: "physicalCountsFormatted/tfactoryRuntimePerRound",
      label: "Runtime per round",
      description: `The runtime of each distillation round`,
      explanation: `The runtime of the T factory is the sum of the runtimes in all rounds.`,
    });
    entries.push({
      path: "physicalCountsFormatted/tstateLogicalErrorRate",
      label: "Logical T state error rate",
      description: `Logical T state error rate`,
      explanation: `This is the logical T state error rate achieved by the T factory which is equal or smaller than the required error rate ${result.physicalCountsFormatted.requiredLogicalTstateErrorRate}.`,
    });
    groups.push({
      title: "T factory parameters",
      alwaysVisible: false,
      entries: entries,
    });
  }
  entries = [];
  entries.push({
    path: "physicalCountsFormatted/logicalCountsNumQubits",
    label: "Logical qubits (pre-layout)",
    description: `Number of logical qubits in the input quantum program`,
    explanation: `We determine ${numberFormat.format(
      result.physicalCounts.breakdown.algorithmicLogicalQubits,
    )} algorithmic logical qubits from this number by assuming to align them in a 2D grid. Auxiliary qubits are added to allow for sufficient space to execute multi-qubit Pauli measurements on all or a subset of the logical qubits.`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalCountsTCount",
    label: "T gates",
    description: `Number of T gates in the input quantum program`,
    explanation: `This includes all T gates and adjoint T gates, but not T gates used to implement rotation gates with arbitrary angle, CCZ gates, or CCiX gates.`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalCountsRotationCount",
    label: "Rotation gates",
    description: `Number of rotation gates in the input quantum program`,
    explanation: `This is the number of all rotation gates. If an angle corresponds to a Pauli, Clifford, or T gate, it is not accounted for in this number.`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalCountsRotationDepth",
    label: "Rotation depth",
    description: `Depth of rotation gates in the input quantum program`,
    explanation: `This is the number of all non-Clifford layers that include at least one single-qubit rotation gate with an arbitrary angle.`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalCountsCczCount",
    label: "CCZ gates",
    description: `Number of CCZ-gates in the input quantum program`,
    explanation: `This is the number of CCZ gates.`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalCountsCcixCount",
    label: "CCiX gates",
    description: `Number of CCiX-gates in the input quantum program`,
    explanation: `This is the number of CCiX gates, which applies $-iX$ controlled on two control qubits [[1212.5069](https://arxiv.org/abs/1212.5069)].`,
  });
  entries.push({
    path: "physicalCountsFormatted/logicalCountsMeasurementCount",
    label: "Measurement operations",
    description: `Number of single qubit measurements in the input quantum program`,
    explanation: `This is the number of single qubit measurements in Pauli basis that are used in the input program. Note that all measurements are counted, however, the measurement result is is determined randomly (with a fixed seed) to be 0 or 1 with a probability of 50%.`,
  });
  groups.push({
    title: "Pre-layout logical resources",
    alwaysVisible: false,
    entries: entries,
  });

  entries = [];
  entries.push({
    path: "physicalCountsFormatted/errorBudget",
    label: "Total error budget",
    description: `Total error budget for the algorithm`,
    explanation: `The total error budget sets the overall allowed error for the algorithm, i.e., the number of times it is allowed to fail. Its value must be between 0 and 1 and the default value is 0.001, which corresponds to 0.1%, and means that the algorithm is allowed to fail once in 1000 executions. This parameter is highly application specific. For example, if one is running Shor's algorithm for factoring integers, a large value for the error budget may be tolerated as one can check that the output are indeed the prime factors of the input. On the other hand, a much smaller error budget may be needed for an algorithm solving a problem with a solution which cannot be efficiently verified. This budget $\\epsilon = \\epsilon_{\\log} + \\epsilon_{\\rm dis} + \\epsilon_{\\rm syn}$ is uniformly distributed and applies to errors $\\epsilon_{\\log}$ to implement logical qubits, an error budget $\\epsilon_{\\rm dis}$ to produce T states through distillation, and an error budget $\\epsilon_{\\rm syn}$ to synthesize rotation gates with arbitrary angles. Note that for distillation and rotation synthesis, the respective error budgets $\\epsilon_{\\rm dis}$ and $\\epsilon_{\\rm syn}$ are uniformly distributed among all T states and all rotation gates, respectively. If there are no rotation gates in the input algorithm, the error budget is uniformly distributed to logical errors and T state errors.`,
  });
  entries.push({
    path: "physicalCountsFormatted/errorBudgetLogical",
    label: "Logical error probability",
    description: `Probability of at least one logical error`,
    explanation: `This is one third of the total error budget ${result.physicalCountsFormatted.errorBudget} if the input algorithm contains rotation with gates with arbitrary angles, or one half of it, otherwise.`,
  });
  entries.push({
    path: "physicalCountsFormatted/errorBudgetTstates",
    label: "T distillation error probability",
    description: `Probability of at least one faulty T distillation`,
    explanation: `This is one third of the total error budget ${result.physicalCountsFormatted.errorBudget} if the input algorithm contains rotation with gates with arbitrary angles, or one half of it, otherwise.`,
  });
  entries.push({
    path: "physicalCountsFormatted/errorBudgetRotations",
    label: "Rotation synthesis error probability",
    description: `Probability of at least one failed rotation synthesis`,
    explanation: `This is one third of the total error budget ${result.physicalCountsFormatted.errorBudget}.`,
  });
  groups.push({
    title: "Assumed error budget",
    alwaysVisible: false,
    entries: entries,
  });

  entries = [];
  entries.push({
    path: "jobParams/qubitParams/name",
    label: "Qubit name",
    description: `Some descriptive name for the qubit model`,
    explanation: `You can load pre-defined qubit parameters by using the names \`qubit_gate_ns_e3\`, \`qubit_gate_ns_e4\`, \`qubit_gate_us_e3\`, \`qubit_gate_us_e4\`, \`qubit_maj_ns_e4\`, or \`qubit_maj_ns_e6\`. The names of these pre-defined qubit parameters indicate the instruction set (gate-based or Majorana), the operation speed (ns or Âµs regime), as well as the fidelity (e.g., e3 for $10^{-3}$ gate error rates).`,
  });
  entries.push({
    path: "jobParams/qubitParams/instructionSet",
    label: "Instruction set",
    description: `Underlying qubit technology (gate-based or Majorana)`,
    explanation: `When modeling the physical qubit abstractions, we distinguish between two different physical instruction sets that are used to operate the qubits. The physical instruction set can be either _gate-based_ or _Majorana_. A gate-based instruction set provides single-qubit measurement, single-qubit gates (incl. T gates), and two-qubit gates. A Majorana instruction set provides a physical T gate, single-qubit measurement and two-qubit joint measurement operations.`,
  });
  entries.push({
    path: "jobParams/qubitParams/oneQubitMeasurementTime",
    label: "Single-qubit measurement time",
    description: `Operation time for single-qubit measurement (t_meas) in ns`,
    explanation: `This is the operation time in nanoseconds to perform a single-qubit measurement in the Pauli basis.`,
  });
  if (result.jobParams.qubitParams.instructionSet == "Majorana") {
    entries.push({
      path: "jobParams/qubitParams/twoQubitJointMeasurementTime",
      label: "Two-qubit measurement time",
      description: `Operation time for two-qubit measurement in ns`,
      explanation: `This is the operation time in nanoseconds to perform a non-destructive two-qubit joint Pauli measurement.`,
    });
  }
  if (result.jobParams.qubitParams.instructionSet == "GateBased") {
    entries.push({
      path: "jobParams/qubitParams/oneQubitGateTime",
      label: "Single-qubit gate time",
      description: `Operation time for single-qubit gate (t_gate) in ns`,
      explanation: `This is the operation time in nanoseconds to perform a single-qubit Clifford operation, e.g., Hadamard or Phase gates.`,
    });
  }
  if (result.jobParams.qubitParams.instructionSet == "GateBased") {
    entries.push({
      path: "jobParams/qubitParams/twoQubitGateTime",
      label: "Two-qubit gate time",
      description: `Operation time for two-qubit gate in ns`,
      explanation: `This is the operation time in nanoseconds to perform a two-qubit Clifford operation, e.g., a CNOT or CZ gate.`,
    });
  }
  entries.push({
    path: "jobParams/qubitParams/tGateTime",
    label: "T gate time",
    description: `Operation time for a T gate`,
    explanation: `This is the operation time in nanoseconds to execute a T gate.`,
  });
  entries.push({
    path: "jobParams/qubitParams/oneQubitMeasurementErrorRate",
    label: "Single-qubit measurement error rate",
    description: `Error rate for single-qubit measurement`,
    explanation: `This is the probability in which a single-qubit measurement in the Pauli basis may fail.`,
  });
  if (result.jobParams.qubitParams.instructionSet == "Majorana") {
    entries.push({
      path: "jobParams/qubitParams/twoQubitJointMeasurementErrorRate",
      label: "Two-qubit measurement error rate",
      description: `Error rate for two-qubit measurement`,
      explanation: `This is the probability in which a non-destructive two-qubit joint Pauli measurement may fail.`,
    });
  }
  if (result.jobParams.qubitParams.instructionSet == "GateBased") {
    entries.push({
      path: "jobParams/qubitParams/oneQubitGateErrorRate",
      label: "Single-qubit error rate",
      description: `Error rate for single-qubit Clifford gate (p)`,
      explanation: `This is the probability in which a single-qubit Clifford operation, e.g., Hadamard or Phase gates, may fail.`,
    });
  }
  if (result.jobParams.qubitParams.instructionSet == "GateBased") {
    entries.push({
      path: "jobParams/qubitParams/twoQubitGateErrorRate",
      label: "Two-qubit error rate",
      description: `Error rate for two-qubit Clifford gate`,
      explanation: `This is the probability in which a two-qubit Clifford operation, e.g., CNOT or CZ gates, may fail.`,
    });
  }
  entries.push({
    path: "jobParams/qubitParams/tGateErrorRate",
    label: "T gate error rate",
    description: `Error rate to prepare single-qubit T state or apply a T gate (p_T)`,
    explanation: `This is the probability in which executing a single T gate may fail.`,
  });
  groups.push({
    title: "Physical qubit parameters",
    alwaysVisible: false,
    entries: entries,
  });

  entries = [];
  entries.push({
    path: "physicalCountsFormatted/logicalDepthFactor",
    label: "Logical depth factor",
    description: `Factor the initial number of logical cycles is multiplied by`,
    explanation: `This is the factor takes into account a potential overhead to the initial number of logical cycles.`,
  });
  entries.push({
    path: "physicalCountsFormatted/maxTFactories",
    label: "Maximum number of T factories",
    description: `The maximum number of T factories can be utilized during the algorithm's runtime`,
    explanation: `This is the maximum number of T factories used for producing the demanded T states, which can be created and executed by the algorithm in parallel.`,
  });
  entries.push({
    path: "physicalCountsFormatted/maxDuration",
    label: "Maximum runtime duration",
    description: `The maximum runtime duration allowed for the algorithm runtime`,
    explanation: `This is the maximum time allowed to the algorithm. If specified, the estimator targets to minimize the number of physical qubits consumed by the algorithm for runtimes under the maximum allowed.`,
  });
  entries.push({
    path: "physicalCountsFormatted/maxPhysicalQubits",
    label: "Maximum number of physical qubits",
    description: `The maximum number of physical qubits allowed for utilization to the algorithm`,
    explanation: `This is the maximum number of physical qubits available to the algorithm. If specified, the estimator targets to minimize the runtime of the algorithm with number of physical qubits consumed not exceeding this maximum.`,
  });
  groups.push({ title: "Constraints", alwaysVisible: false, entries: entries });

  const assumptions = [
    "_More details on the following lists of assumptions can be found in the paper [Accessing requirements for scaling quantum computers and their applications](https://aka.ms/AQ/RE/Paper)._",
    "**Uniform independent physical noise.** We assume that the noise on physical qubits and physical qubit operations is the standard circuit noise model. In particular we assume error events at different space-time locations are independent and that error rates are uniform across the system in time and space.",
    "**Efficient classical computation.** We assume that classical overhead (compilation, control, feedback, readout, decoding, etc.) does not dominate the overall cost of implementing the full quantum algorithm.",
    "**Extraction circuits for planar quantum ISA.** We assume that stabilizer extraction circuits with similar depth and error correction performance to those for standard surface and Hastings-Haah code patches can be constructed to implement all operations of the planar quantum ISA (instruction set architecture).",
    "**Uniform independent logical noise.** We assume that the error rate of a logical operation is approximately equal to its space-time volume (the number of tiles multiplied by the number of logical time steps) multiplied by the error rate of a logical qubit in a standard one-tile patch in one logical time step.",
    "**Negligible Clifford costs for synthesis.** We assume that the space overhead for synthesis and space and time overhead for transport of magic states within magic state factories and to synthesis qubits are all negligible.",
    "**Smooth magic state consumption rate.** We assume that the rate of T state consumption throughout the compiled algorithm is almost constant, or can be made almost constant without significantly increasing the number of logical time steps for the algorithm.",
  ];

  return { groups: groups, assumptions: assumptions };
}
// END OF AUTOMATICALLY GENERATED CODE

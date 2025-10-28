// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use serde::Serialize;

use crate::estimates::{Factory, FactoryPart, PhysicalResourceEstimationResult};
use crate::system::modeling::Protocol;

use super::LayoutReportData;
use super::{
    super::modeling::{PhysicalInstructionSet, TFactory},
    job_params::JobParams,
};

#[derive(Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Report {
    groups: Vec<ReportEntryGroup>,
    assumptions: Vec<String>,
}

impl Report {
    #[allow(clippy::vec_init_then_push, clippy::too_many_lines)]
    pub fn new(
        job_params: &JobParams,
        layout_report_data: &impl LayoutReportData,
        result: &PhysicalResourceEstimationResult<Protocol, TFactory>,
        formatted_counts: &FormattedPhysicalResourceCounts,
    ) -> Self {
        let logical_counts = layout_report_data;
        // In this system, we consider T as the only magic state type, therefore
        // there is only one factory part in the result.
        let part = result.factory_parts()[0].as_ref();
        let factory = part.map(FactoryPart::factory);

        // THIS CODE HAS BEEN AUTOMATICALLY GENERATED WITH resource_estimator/scripts/generate_report_code.py from docs/output_data.md
        let mut groups = vec![];

        let mut entries = vec![];
        entries.push(ReportEntry::new("physicalCountsFormatted/runtime", "Runtime", r#"Total runtime"#, &format!(r#"This is a runtime estimate for the execution time of the algorithm.  In general, the execution time corresponds to the duration of one logical cycle ({} nanosecs) multiplied by the {} logical cycles to run the algorithm.  If however the duration of a single T factory (here: {} nanosecs) is larger than the algorithm runtime, we extend the number of logical cycles artificially in order to exceed the runtime of a single T factory."#, format_thousand_sep(&result.logical_patch().logical_cycle_time()), format_thousand_sep(&result.algorithmic_logical_depth()), format_thousand_sep(&factory.map_or(0, TFactory::duration)))));
        entries.push(ReportEntry::new("physicalCountsFormatted/rqops", "rQOPS", r#"Reliable quantum operations per second"#, &format!(r#"The value is computed as the number of logical qubits after layout ({}) (with a logical error rate of {}) multiplied by the clock frequency ({}), which is the number of logical cycles per second."#, format_thousand_sep(&result.layout_overhead().logical_qubits()), formatted_counts.required_logical_qubit_error_rate, format_thousand_sep_f64(result.logical_patch().logical_cycles_per_second()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/physicalQubits", "Physical qubits", r#"Number of physical qubits"#, &format!(r#"This value represents the total number of physical qubits, which is the sum of {} physical qubits to implement the algorithm logic, and {} physical qubits to execute the T factories that are responsible to produce the T states that are consumed by the algorithm."#, format_thousand_sep(&result.physical_qubits_for_algorithm()), format_thousand_sep(&result.physical_qubits_for_factories()))));
        groups.push(ReportEntryGroup {
            title: "Physical resource estimates".into(),
            always_visible: true,
            entries,
        });

        let mut entries = vec![];
        entries.push(ReportEntry::new("physicalCountsFormatted/algorithmicLogicalQubits", "Logical algorithmic qubits", r#"Number of logical qubits for the algorithm after layout"#, &format!(r#"Laying out the logical qubits in the presence of nearest-neighbor constraints requires additional logical qubits.  In particular, to layout the $Q_{{\rm alg}} = {}$ logical qubits in the input algorithm, we require in total $2 \cdot Q_{{\rm alg}} + \lceil \sqrt{{8 \cdot Q_{{\rm alg}}}}\rceil + 1 = {}$ logical qubits."#, format_thousand_sep(&logical_counts.num_qubits()), format_thousand_sep(&result.layout_overhead().logical_qubits()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/algorithmicLogicalDepth", "Algorithmic depth", r#"Number of logical cycles for the algorithm"#, &format!(r#"To execute the algorithm using _Parallel Synthesis Sequential Pauli Computation_ (PSSPC), operations are scheduled in terms of multi-qubit Pauli measurements, for which assume an execution time of one logical cycle.  Based on the input algorithm, we require one multi-qubit measurement for the {} single-qubit measurements, the {} arbitrary single-qubit rotations, and the {} T gates, three multi-qubit measurements for each of the {} CCZ and {} CCiX gates in the input program, as well as {} multi-qubit measurements for each of the {} non-Clifford layers in which there is at least one single-qubit rotation with an arbitrary angle rotation."#, format_thousand_sep(&logical_counts.measurement_count()), format_thousand_sep(&logical_counts.rotation_count()), format_thousand_sep(&logical_counts.t_count()), format_thousand_sep(&logical_counts.ccz_count()), format_thousand_sep(&logical_counts.ccix_count()), formatted_counts.num_ts_per_rotation, format_thousand_sep(&logical_counts.rotation_depth()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalDepth", "Logical depth", r#"Number of logical cycles performed"#, &format!(r#"This number is usually equal to the logical depth of the algorithm, which is {}.  However, in the case in which a single T factory is slower than the execution time of the algorithm, we adjust the logical cycle depth to exceed the T factory's execution time."#, format_thousand_sep(&result.algorithmic_logical_depth()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/clockFrequency", "Clock frequency", r#"Number of logical cycles per second"#, &format!(r#"This is the number of logical cycles that can be performed within one second.  The logical cycle time is {}."#, formatted_counts.logical_cycle_time)));
        entries.push(ReportEntry::new("physicalCountsFormatted/numTstates", "Number of T states", r#"Number of T states consumed by the algorithm"#, &format!(r#"To execute the algorithm, we require one T state for each of the {} T gates, four T states for each of the {} CCZ and {} CCiX gates, as well as {} for each of the {} single-qubit rotation gates with arbitrary angle rotation."#, format_thousand_sep(&logical_counts.t_count()), format_thousand_sep(&logical_counts.ccz_count()), format_thousand_sep(&logical_counts.ccix_count()), formatted_counts.num_ts_per_rotation, format_thousand_sep(&logical_counts.rotation_count()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/numTfactories", "Number of T factories", &format!(r#"Number of T factories capable of producing the demanded {} T states during the algorithm's runtime"#, format_thousand_sep(&result.num_magic_states(0))), &format!(r#"The total number of T factories {} that are executed in parallel is computed as $\left\lceil\dfrac{{\text{{T states}}\cdot\text{{T factory duration}}}}{{\text{{T states per T factory}}\cdot\text{{algorithm runtime}}}}\right\rceil = \left\lceil\dfrac{{{} \cdot {}\;\text{{ns}}}}{{{} \cdot {}\;\text{{ns}}}}\right\rceil$"#, format_thousand_sep(&part.map_or(0, FactoryPart::copies)), format_thousand_sep(&result.num_magic_states(0)), format_thousand_sep(&factory.map_or(0, TFactory::duration)), format_thousand_sep(&factory.map_or(0, TFactory::num_output_states)), format_thousand_sep(&result.runtime()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/numTfactoryRuns", "Number of T factory invocations", r#"Number of times all T factories are invoked"#, &format!(r#"In order to prepare the {} T states, the {} copies of the T factory are repeatedly invoked {} times."#, format_thousand_sep(&result.num_magic_states(0)), format_thousand_sep(&part.map_or(0, FactoryPart::copies)), format_thousand_sep(&part.map_or(0, FactoryPart::runs)))));
        entries.push(ReportEntry::new("physicalCountsFormatted/physicalQubitsForAlgorithm", "Physical algorithmic qubits", r#"Number of physical qubits for the algorithm after layout"#, &format!(r#"The {} are the product of the {} logical qubits after layout and the {} physical qubits that encode a single logical qubit."#, format_thousand_sep(&result.physical_qubits_for_algorithm()), format_thousand_sep(&result.layout_overhead().logical_qubits()), format_thousand_sep(&result.logical_patch().physical_qubits()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/physicalQubitsForTfactories", "Physical T factory qubits", r#"Number of physical qubits for the T factories"#, &format!(r#"Each T factory requires {} physical qubits and we run {} in parallel, therefore we need ${} = {} \cdot {}$ qubits."#, format_thousand_sep(&factory.map_or(0, TFactory::physical_qubits)), format_thousand_sep(&part.map_or(0, FactoryPart::copies)), format_thousand_sep(&result.physical_qubits_for_factories()), format_thousand_sep(&factory.map_or(0, TFactory::physical_qubits)), format_thousand_sep(&part.map_or(0, FactoryPart::copies)))));
        entries.push(ReportEntry::new("physicalCountsFormatted/requiredLogicalQubitErrorRate", "Required logical qubit error rate", r#"The minimum logical qubit error rate required to run the algorithm within the error budget"#, &format!(r#"The minimum logical qubit error rate is obtained by dividing the logical error probability {} by the product of {} logical qubits and the total cycle count {}."#, formatted_counts.error_budget_logical, format_thousand_sep(&result.layout_overhead().logical_qubits()), format_thousand_sep(&result.num_cycles()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/requiredLogicalTstateErrorRate", "Required logical T state error rate", r#"The minimum T state error rate required for distilled T states"#, &format!(r#"The minimum T state error rate is obtained by dividing the T distillation error probability {} by the total number of T states {}."#, formatted_counts.error_budget_tstates, format_thousand_sep(&result.num_magic_states(0)))));
        entries.push(ReportEntry::new("physicalCountsFormatted/numTsPerRotation", "Number of T states per rotation", r#"Number of T states to implement a rotation with an arbitrary angle"#, &format!(r#"The number of T states to implement a rotation with an arbitrary angle is $\lceil 0.53 \log_2({} / {}) + 4.86\rceil$ [[arXiv:2203.10064](https://arxiv.org/abs/2203.10064)].  For simplicity, we use this formula for all single-qubit arbitrary angle rotations, and do not distinguish between best, worst, and average cases."#, format_thousand_sep(&logical_counts.rotation_count()), result.error_budget().rotations())));
        groups.push(ReportEntryGroup {
            title: "Resource estimates breakdown".into(),
            always_visible: false,
            entries,
        });

        let mut entries = vec![];
        entries.push(ReportEntry::new("jobParams/qecScheme/name", "QEC scheme", r#"Name of QEC scheme"#, r#"You can load pre-defined QEC schemes by using the name `surface_code` or `floquet_code`. The latter only works with Majorana qubits."#));
        entries.push(ReportEntry::new("logicalQubit/codeDistance", "Code distance", r#"Required code distance for error correction"#, &format!(r#"The code distance is the smallest odd integer greater or equal to $\dfrac{{2\log({} / {})}}{{\log({}/{})}} - 1$"#, job_params.qec_scheme().crossing_prefactor.expect("crossing prefactor should be set"), result.required_logical_error_rate(), job_params.qec_scheme().error_correction_threshold.expect("error correction threshold should be set"), result.logical_patch().physical_qubit().clifford_error_rate())));
        entries.push(ReportEntry::new("physicalCountsFormatted/physicalQubitsPerLogicalQubit", "Physical qubits", r#"Number of physical qubits per logical qubit"#, &format!(r#"The number of physical qubits per logical qubit are evaluated using the formula {} that can be user-specified."#, job_params.qec_scheme().physical_qubits_per_logical_qubit.as_ref().expect("physical qubits per logical qubit should be set"))));
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalCycleTime", "Logical cycle time", r#"Duration of a logical cycle in nanoseconds"#, &format!(r#"The runtime of one logical cycle in nanoseconds is evaluated using the formula {} that can be user-specified."#, job_params.qec_scheme().logical_cycle_time.as_ref().expect("logical cycle time should be set"))));
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalErrorRate", "Logical qubit error rate", r#"Logical qubit error rate"#, &format!(r#"The logical qubit error rate is computed as ${} \cdot \left(\dfrac{{{}}}{{{}}}\right)^\frac{{{} + 1}}{{2}}$"#, job_params.qec_scheme().crossing_prefactor.expect("crossing prefactor should be set"), result.logical_patch().physical_qubit().clifford_error_rate(), job_params.qec_scheme().error_correction_threshold.expect("error correction threshold should be set"), result.logical_patch().code_parameter())));
        entries.push(ReportEntry::new("jobParams/qecScheme/crossingPrefactor", "Crossing prefactor", r#"Crossing prefactor used in QEC scheme"#, r#"The crossing prefactor is usually extracted numerically from simulations when fitting an exponential curve to model the relationship between logical and physical error rate."#));
        entries.push(ReportEntry::new("jobParams/qecScheme/errorCorrectionThreshold", "Error correction threshold", r#"Error correction threshold used in QEC scheme"#, r#"The error correction threshold is the physical error rate below which the error rate of the logical qubit is less than the error rate of the physical qubit that constitute it.  This value is usually extracted numerically from simulations of the logical error rate."#));
        entries.push(ReportEntry::new(
            "jobParams/qecScheme/logicalCycleTime",
            "Logical cycle time formula",
            r#"QEC scheme formula used to compute logical cycle time"#,
            &format!(
                r#"This is the formula that is used to compute the logical cycle time {} ns."#,
                format_thousand_sep(&result.logical_patch().logical_cycle_time())
            ),
        ));
        entries.push(ReportEntry::new("jobParams/qecScheme/physicalQubitsPerLogicalQubit", "Physical qubits formula", r#"QEC scheme formula used to compute number of physical qubits per logical qubit"#, &format!(r#"This is the formula that is used to compute the number of physical qubits per logical qubits {}."#, format_thousand_sep(&result.logical_patch().physical_qubits()))));
        groups.push(ReportEntryGroup {
            title: "Logical qubit parameters".into(),
            always_visible: false,
            entries,
        });

        if let Some(part) = part {
            let mut entries = vec![];
            entries.push(ReportEntry::new("physicalCountsFormatted/tfactoryPhysicalQubits", "Physical qubits", r#"Number of physical qubits for a single T factory"#, r#"This corresponds to the maximum number of physical qubits over all rounds of T distillation units in a T factory.  A round of distillation contains of multiple copies of distillation units to achieve the required success probability of producing a T state with the expected logical T state error rate."#));
            entries.push(ReportEntry::new("physicalCountsFormatted/tfactoryRuntime", "Runtime", r#"Runtime of a single T factory"#, r#"The runtime of a single T factory is the accumulated runtime of executing each round in a T factory."#));
            entries.push(ReportEntry::new("tfactory/numTstates", "Number of output T states per run", r#"Number of output T states produced in a single run of T factory"#, &format!(r#"The T factory takes as input {} noisy physical T states with an error rate of {} and produces {} T states with an error rate of {}."#, format_thousand_sep(&part.factory().num_input_states()), job_params.qubit_params().t_gate_error_rate(), format_thousand_sep(&part.factory().num_output_states()), formatted_counts.tstate_logical_error_rate)));
            entries.push(ReportEntry::new("physicalCountsFormatted/numInputTstates", "Number of input T states per run", r#"Number of physical input T states consumed in a single run of a T factory"#, r#"This value includes the physical input T states of all copies of the distillation unit in the first round."#));
            entries.push(ReportEntry::new("tfactory/numRounds", "Distillation rounds", r#"The number of distillation rounds"#, r#"This is the number of distillation rounds.  In each round one or multiple copies of some distillation unit is executed."#));
            entries.push(ReportEntry::new(
                "physicalCountsFormatted/numUnitsPerRound",
                "Distillation units per round",
                r#"The number of units in each round of distillation"#,
                r#"This is the number of copies for the distillation units per round."#,
            ));
            entries.push(ReportEntry::new("physicalCountsFormatted/unitNamePerRound", "Distillation units", r#"The types of distillation units"#, r#"These are the types of distillation units that are executed in each round.  The units can be either physical or logical, depending on what type of qubit they are operating.  Space-efficient units require fewer qubits for the cost of longer runtime compared to Reed-Muller preparation units."#));
            entries.push(ReportEntry::new("physicalCountsFormatted/codeDistancePerRound", "Distillation code distances", r#"The code distance in each round of distillation"#, r#"This is the code distance used for the units in each round.  If the code distance is 1, then the distillation unit operates on physical qubits instead of error-corrected logical qubits."#));
            entries.push(ReportEntry::new("physicalCountsFormatted/physicalQubitsPerRound", "Number of physical qubits per round", r#"The number of physical qubits used in each round of distillation"#, r#"The maximum number of physical qubits over all rounds is the number of physical qubits for the T factory, since qubits are reused by different rounds."#));
            entries.push(ReportEntry::new(
                "physicalCountsFormatted/tfactoryRuntimePerRound",
                "Runtime per round",
                r#"The runtime of each distillation round"#,
                r#"The runtime of the T factory is the sum of the runtimes in all rounds."#,
            ));
            entries.push(ReportEntry::new("physicalCountsFormatted/tstateLogicalErrorRate", "Logical T state error rate", r#"Logical T state error rate"#, &format!(r#"This is the logical T state error rate achieved by the T factory which is equal or smaller than the required error rate {}."#, formatted_counts.required_logical_tstate_error_rate)));
            groups.push(ReportEntryGroup {
                title: "T factory parameters".into(),
                always_visible: false,
                entries,
            });
        }
        let mut entries = vec![];
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalCountsNumQubits", "Logical qubits (pre-layout)", r#"Number of logical qubits in the input quantum program"#, &format!(r#"We determine {} algorithmic logical qubits from this number by assuming to align them in a 2D grid.  Auxiliary qubits are added to allow for sufficient space to execute multi-qubit Pauli measurements on all or a subset of the logical qubits."#, format_thousand_sep(&result.layout_overhead().logical_qubits()))));
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalCountsTCount", "T gates", r#"Number of T gates in the input quantum program"#, r#"This includes all T gates and adjoint T gates, but not T gates used to implement rotation gates with arbitrary angle, CCZ gates, or CCiX gates."#));
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalCountsRotationCount", "Rotation gates", r#"Number of rotation gates in the input quantum program"#, r#"This is the number of all rotation gates. If an angle corresponds to a Pauli, Clifford, or T gate, it is not accounted for in this number."#));
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalCountsRotationDepth", "Rotation depth", r#"Depth of rotation gates in the input quantum program"#, r#"This is the number of all non-Clifford layers that include at least one single-qubit rotation gate with an arbitrary angle."#));
        entries.push(ReportEntry::new(
            "physicalCountsFormatted/logicalCountsCczCount",
            "CCZ gates",
            r#"Number of CCZ-gates in the input quantum program"#,
            r#"This is the number of CCZ gates."#,
        ));
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalCountsCcixCount", "CCiX gates", r#"Number of CCiX-gates in the input quantum program"#, r#"This is the number of CCiX gates, which applies $-iX$ controlled on two control qubits [[1212.5069](https://arxiv.org/abs/1212.5069)]."#));
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalCountsMeasurementCount", "Measurement operations", r#"Number of single qubit measurements in the input quantum program"#, r#"This is the number of single qubit measurements in Pauli basis that are used in the input program.  Note that all measurements are counted, however, the measurement result is is determined randomly (with a fixed seed) to be 0 or 1 with a probability of 50%."#));
        groups.push(ReportEntryGroup {
            title: "Pre-layout logical resources".into(),
            always_visible: false,
            entries,
        });

        let mut entries = vec![];
        entries.push(ReportEntry::new("physicalCountsFormatted/errorBudget", "Total error budget", r#"Total error budget for the algorithm"#, r"The total error budget sets the overall allowed error for the algorithm, i.e., the number of times it is allowed to fail.  Its value must be between 0 and 1 and the default value is 0.001, which corresponds to 0.1%, and means that the algorithm is allowed to fail once in 1000 executions.  This parameter is highly application specific. For example, if one is running Shor's algorithm for factoring integers, a large value for the error budget may be tolerated as one can check that the output are indeed the prime factors of the input.  On the other hand, a much smaller error budget may be needed for an algorithm solving a problem with a solution which cannot be efficiently verified.  This budget $\epsilon = \epsilon_{\log} + \epsilon_{\rm dis} + \epsilon_{\rm syn}$ is uniformly distributed and applies to errors $\epsilon_{\log}$ to implement logical qubits, an error budget $\epsilon_{\rm dis}$ to produce T states through distillation, and an error budget $\epsilon_{\rm syn}$ to synthesize rotation gates with arbitrary angles.  Note that for distillation and rotation synthesis, the respective error budgets $\epsilon_{\rm dis}$ and $\epsilon_{\rm syn}$ are uniformly distributed among all T states and all rotation gates, respectively. If there are no rotation gates in the input algorithm, the error budget is uniformly distributed to logical errors and T state errors."));
        entries.push(ReportEntry::new("physicalCountsFormatted/errorBudgetLogical", "Logical error probability", r#"Probability of at least one logical error"#, &format!(r#"This is one third of the total error budget {} if the input algorithm contains rotation with gates with arbitrary angles, or one half of it, otherwise."#, formatted_counts.error_budget)));
        entries.push(ReportEntry::new("physicalCountsFormatted/errorBudgetTstates", "T distillation error probability", r#"Probability of at least one faulty T distillation"#, &format!(r#"This is one third of the total error budget {} if the input algorithm contains rotation with gates with arbitrary angles, or one half of it, otherwise."#, formatted_counts.error_budget)));
        entries.push(ReportEntry::new(
            "physicalCountsFormatted/errorBudgetRotations",
            "Rotation synthesis error probability",
            r#"Probability of at least one failed rotation synthesis"#,
            &format!(
                r#"This is one third of the total error budget {}."#,
                formatted_counts.error_budget
            ),
        ));
        groups.push(ReportEntryGroup {
            title: "Assumed error budget".into(),
            always_visible: false,
            entries,
        });

        let mut entries = vec![];
        entries.push(ReportEntry::new("jobParams/qubitParams/name", "Qubit name", r#"Some descriptive name for the qubit model"#, r#"You can load pre-defined qubit parameters by using the names `qubit_gate_ns_e3`, `qubit_gate_ns_e4`, `qubit_gate_us_e3`, `qubit_gate_us_e4`, `qubit_maj_ns_e4`, or `qubit_maj_ns_e6`.  The names of these pre-defined qubit parameters indicate the instruction set (gate-based or Majorana), the operation speed (ns or µs regime), as well as the fidelity (e.g., e3 for $10^{-3}$ gate error rates)."#));
        entries.push(ReportEntry::new("jobParams/qubitParams/instructionSet", "Instruction set", r#"Underlying qubit technology (gate-based or Majorana)"#, r#"When modeling the physical qubit abstractions, we distinguish between two different physical instruction sets that are used to operate the qubits.  The physical instruction set can be either *gate-based* or *Majorana*.  A gate-based instruction set provides single-qubit measurement, single-qubit gates (incl. T gates), and two-qubit gates.  A Majorana instruction set provides a physical T gate, single-qubit measurement and two-qubit joint measurement operations."#));
        entries.push(ReportEntry::new("jobParams/qubitParams/oneQubitMeasurementTime", "Single-qubit measurement time", r#"Operation time for single-qubit measurement (t_meas) in ns"#, r#"This is the operation time in nanoseconds to perform a single-qubit measurement in the Pauli basis."#));
        if job_params.qubit_params().instruction_set() == PhysicalInstructionSet::Majorana {
            entries.push(ReportEntry::new("jobParams/qubitParams/twoQubitJointMeasurementTime", "Two-qubit measurement time", r#"Operation time for two-qubit measurement in ns"#, r#"This is the operation time in nanoseconds to perform a non-destructive two-qubit joint Pauli measurement."#));
        }
        if job_params.qubit_params().instruction_set() == PhysicalInstructionSet::GateBased {
            entries.push(ReportEntry::new("jobParams/qubitParams/oneQubitGateTime", "Single-qubit gate time", r#"Operation time for single-qubit gate (t_gate) in ns"#, r#"This is the operation time in nanoseconds to perform a single-qubit Clifford operation, e.g., Hadamard or Phase gates."#));
        }
        if job_params.qubit_params().instruction_set() == PhysicalInstructionSet::GateBased {
            entries.push(ReportEntry::new("jobParams/qubitParams/twoQubitGateTime", "Two-qubit gate time", r#"Operation time for two-qubit gate in ns"#, r#"This is the operation time in nanoseconds to perform a two-qubit Clifford operation, e.g., a CNOT or CZ gate."#));
        }
        entries.push(ReportEntry::new(
            "jobParams/qubitParams/tGateTime",
            "T gate time",
            r#"Operation time for a T gate"#,
            r#"This is the operation time in nanoseconds to execute a T gate."#,
        ));
        entries.push(ReportEntry::new("jobParams/qubitParams/oneQubitMeasurementErrorRate", "Single-qubit measurement error rate", r#"Error rate for single-qubit measurement"#, r#"This is the probability in which a single-qubit measurement in the Pauli basis may fail."#));
        if job_params.qubit_params().instruction_set() == PhysicalInstructionSet::Majorana {
            entries.push(ReportEntry::new("jobParams/qubitParams/twoQubitJointMeasurementErrorRate", "Two-qubit measurement error rate", r#"Error rate for two-qubit measurement"#, r#"This is the probability in which a non-destructive two-qubit joint Pauli measurement may fail."#));
        }
        if job_params.qubit_params().instruction_set() == PhysicalInstructionSet::GateBased {
            entries.push(ReportEntry::new("jobParams/qubitParams/oneQubitGateErrorRate", "Single-qubit error rate", r#"Error rate for single-qubit Clifford gate (p)"#, r#"This is the probability in which a single-qubit Clifford operation, e.g., Hadamard or Phase gates, may fail."#));
        }
        if job_params.qubit_params().instruction_set() == PhysicalInstructionSet::GateBased {
            entries.push(ReportEntry::new("jobParams/qubitParams/twoQubitGateErrorRate", "Two-qubit error rate", r#"Error rate for two-qubit Clifford gate"#, r#"This is the probability in which a two-qubit Clifford operation, e.g., CNOT or CZ gates, may fail."#));
        }
        entries.push(ReportEntry::new(
            "jobParams/qubitParams/tGateErrorRate",
            "T gate error rate",
            r#"Error rate to prepare single-qubit T state or apply a T gate (p_T)"#,
            r#"This is the probability in which executing a single T gate may fail."#,
        ));
        groups.push(ReportEntryGroup {
            title: "Physical qubit parameters".into(),
            always_visible: false,
            entries,
        });

        let mut entries = vec![];
        entries.push(ReportEntry::new("physicalCountsFormatted/logicalDepthFactor", "Logical depth factor", r#"Factor the initial number of logical cycles is multiplied by"#, r#"This is the factor takes into account a potential overhead to the initial number of logical cycles."#));
        entries.push(ReportEntry::new("physicalCountsFormatted/maxTFactories", "Maximum number of T factories", r#"The maximum number of T factories can be utilized during the algorithm's runtime"#, r#"This is the maximum number of T factories used for producing the demanded T states, which can be created and executed by the algorithm in parallel."#));
        entries.push(ReportEntry::new("physicalCountsFormatted/maxDuration", "Maximum runtime duration", r#"The maximum runtime duration allowed for the algorithm runtime"#, r#"This is the maximum time allowed to the algorithm. If specified, the estimator targets to minimize the number of physical qubits consumed by the algorithm for runtimes under the maximum allowed."#));
        entries.push(ReportEntry::new("physicalCountsFormatted/maxPhysicalQubits", "Maximum number of physical qubits", r#"The maximum number of physical qubits allowed for utilization to the algorithm"#, r#"This is the maximum number of physical qubits available to the algorithm. If specified, the estimator targets to minimize the runtime of the algorithm with number of physical qubits consumed not exceeding this maximum."#));
        groups.push(ReportEntryGroup {
            title: "Constraints".into(),
            always_visible: false,
            entries,
        });

        let assumptions = vec![
            String::from(
                "_More details on the following lists of assumptions can be found in the paper [Accessing requirements for scaling quantum computers and their applications](https://aka.ms/AQ/RE/Paper)._",
            ),
            String::from(
                "**Uniform independent physical noise.** We assume that the noise on physical qubits and physical qubit operations is the standard circuit noise model. In particular we assume error events at different space-time locations are independent and that error rates are uniform across the system in time and space.",
            ),
            String::from(
                "**Efficient classical computation.** We assume that classical overhead (compilation, control, feedback, readout, decoding, etc.) does not dominate the overall cost of implementing the full quantum algorithm.",
            ),
            String::from(
                "**Extraction circuits for planar quantum ISA.** We assume that stabilizer extraction circuits with similar depth and error correction performance to those for standard surface and Hastings-Haah code patches can be constructed to implement all operations of the planar quantum ISA (instruction set architecture).",
            ),
            String::from(
                "**Uniform independent logical noise.** We assume that the error rate of a logical operation is approximately equal to its space-time volume (the number of tiles multiplied by the number of logical time steps) multiplied by the error rate of a logical qubit in a standard one-tile patch in one logical time step.",
            ),
            String::from(
                "**Negligible Clifford costs for synthesis.** We assume that the space overhead for synthesis and space and time overhead for transport of magic states within magic state factories and to synthesis qubits are all negligible.",
            ),
            String::from(
                "**Smooth magic state consumption rate.** We assume that the rate of T state consumption throughout the compiled algorithm is almost constant, or can be made almost constant without significantly increasing the number of logical time steps for the algorithm.",
            ),
        ];

        Self {
            groups,
            assumptions,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
struct ReportEntryGroup {
    title: String,
    always_visible: bool,
    entries: Vec<ReportEntry>,
}

#[derive(Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
struct ReportEntry {
    path: String,
    label: String,
    description: String,
    explanation: String,
}

impl ReportEntry {
    pub fn new(path: &str, label: &str, description: &str, explanation: &str) -> Self {
        ReportEntry {
            path: path.into(),
            label: label.into(),
            description: description.into(),
            explanation: explanation.into(),
        }
    }
}

#[derive(Default, Debug, serde::Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct FormattedPhysicalResourceCounts {
    /// Total runtime as human friendly string
    pub(crate) runtime: String,

    /// Reliable QOPS formatted with metric prefix
    pub(crate) rqops: String,

    /// Total number of physical qubits 1000-separated
    pub(crate) physical_qubits: String,

    pub(crate) algorithmic_logical_qubits: String,
    pub(crate) algorithmic_logical_depth: String,
    pub(crate) logical_depth: String,
    pub(crate) num_tstates: String,
    pub(crate) num_tfactories: String,
    pub(crate) num_tfactory_runs: String,
    pub(crate) physical_qubits_for_algorithm: String,
    pub(crate) physical_qubits_for_tfactories: String,

    /// The number of physical qubits for all T-factories in percentage to total
    pub(crate) physical_qubits_for_tfactories_percentage: String,

    /// Truncated required logical qubit error rate
    pub(crate) required_logical_qubit_error_rate: String,

    /// Truncated required T-state error rate
    pub(crate) required_logical_tstate_error_rate: String,

    pub(crate) physical_qubits_per_logical_qubit: String,

    /// The logical cycle time of a logical qubit as human friendly string
    pub(crate) logical_cycle_time: String,

    /// The number of logical cycles per second as a human friendly string
    pub(crate) clock_frequency: String,

    /// Truncated logical error rate
    pub(crate) logical_error_rate: String,

    pub(crate) tfactory_physical_qubits: String,

    /// The runtime of a single T-factory as human friendly string
    pub(crate) tfactory_runtime: String,

    pub(crate) num_input_tstates: String,

    /// The number of units per distillation round, comma separated in a string
    pub(crate) num_units_per_round: String,

    /// The unit names of each distallation round, comma separated in a string
    pub(crate) unit_name_per_round: String,

    /// The code distances per distillation round, comma separated in a string
    pub(crate) code_distance_per_round: String,

    /// The number of physical qubits per distillation round, comma separated in a string
    pub(crate) physical_qubits_per_round: String,

    /// The runtime of each distillation round, displayed as comma separated human friendly strings
    pub(crate) tfactory_runtime_per_round: String,

    /// Truncated logical T-state error rate
    pub(crate) tstate_logical_error_rate: String,

    pub(crate) logical_counts_num_qubits: String,
    pub(crate) logical_counts_t_count: String,
    pub(crate) logical_counts_rotation_count: String,
    pub(crate) logical_counts_rotation_depth: String,
    pub(crate) logical_counts_ccz_count: String,
    pub(crate) logical_counts_ccix_count: String,
    pub(crate) logical_counts_measurement_count: String,

    /// Truncated total error budget
    pub(crate) error_budget: String,

    /// Truncated error budget for logical error
    pub(crate) error_budget_logical: String,

    /// Truncated error budget for faulty T state distillation
    pub(crate) error_budget_tstates: String,

    /// Truncated error budget for faulty rotation synthesis
    pub(crate) error_budget_rotations: String,

    /// Formatted number of Ts per rotation (might be None)
    pub(crate) num_ts_per_rotation: String,

    /// Formatted logical depth factor constraint
    pub(crate) logical_depth_factor: String,
    /// Formatted max T factories constraint
    pub(crate) max_t_factories: String,
    /// Formatted max duration constraint
    pub(crate) max_duration: String,
    /// Formatted max physical qubits constraint
    pub(crate) max_physical_qubits: String,
}

impl FormattedPhysicalResourceCounts {
    #[allow(clippy::too_many_lines)]
    pub fn new(
        result: &PhysicalResourceEstimationResult<Protocol, TFactory>,
        job_params: &JobParams,
        layout_report_data: &impl LayoutReportData,
    ) -> Self {
        // Physical resource estimates
        let runtime = format_duration(result.runtime().into());
        let rqops = format_metric_prefix(result.rqops());
        let physical_qubits = format_metric_prefix(result.physical_qubits());
        let part = result.factory_parts()[0].as_ref();
        let factory = part.map(FactoryPart::factory);

        // Resource estimates breakdown

        let algorithmic_logical_qubits =
            format_metric_prefix(result.layout_overhead().logical_qubits());
        let algorithmic_logical_depth = format_metric_prefix(result.algorithmic_logical_depth());
        let logical_depth = format_metric_prefix(result.num_cycles());
        let num_tstates = format_metric_prefix(result.num_magic_states(0));
        let num_tfactories = format_metric_prefix(part.map_or(0, FactoryPart::copies));
        let num_tfactory_runs = format_metric_prefix(part.map_or(0, FactoryPart::runs));
        let physical_qubits_for_algorithm =
            format_metric_prefix(result.physical_qubits_for_algorithm());
        let physical_qubits_for_tfactories =
            format_metric_prefix(result.physical_qubits_for_factories());

        let physical_qubits_for_tfactories_percentage = format!(
            "{:.2} %",
            (result.physical_qubits_for_factories() * 100) as f64 / result.physical_qubits() as f64
        );

        let required_logical_qubit_error_rate =
            format!("{:.2e}", result.required_logical_error_rate());

        let no_tstates_msg = "No T states in algorithm";
        let no_rotations_msg = "No rotations in algorithm";

        let required_logical_tstate_error_rate = part.map_or(String::from(no_tstates_msg), |p| {
            format!("{:.2e}", p.required_output_error_rate())
        });

        // Logical qubit parameters
        let physical_qubits_per_logical_qubit =
            format_metric_prefix(result.logical_patch().physical_qubits());

        let logical_cycle_time =
            format_duration(result.logical_patch().logical_cycle_time() as u128);

        let clock_frequency =
            format_metric_prefix(result.logical_patch().logical_cycles_per_second().round() as u64);

        let logical_error_rate = format!("{:.2e}", result.logical_patch().logical_error_rate());

        // T factory parameters
        let tfactory_physical_qubits = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            format_metric_prefix(tfactory.physical_qubits())
        });
        let tfactory_runtime = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            format_duration(tfactory.duration() as u128)
        });
        let num_input_tstates = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            format_metric_prefix(tfactory.num_input_states())
        });

        let num_units_per_round = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            tfactory
                .num_units_per_round()
                .iter()
                .map(|&num| num.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        });

        let unit_name_per_round = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            tfactory.unit_names().join(", ")
        });

        let code_distance_per_round = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            tfactory
                .code_parameter_per_round()
                .iter()
                .map(|&num| num.copied().unwrap_or(1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        });

        let physical_qubits_per_round = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            tfactory
                .physical_qubits_per_round()
                .into_iter()
                .map(format_metric_prefix)
                .collect::<Vec<_>>()
                .join(", ")
        });

        let tfactory_runtime_per_round = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            tfactory
                .duration_per_round()
                .iter()
                .map(|&duration| format_duration(duration as u128))
                .collect::<Vec<_>>()
                .join(", ")
        });

        let tstate_logical_error_rate = factory.map_or(String::from(no_tstates_msg), |tfactory| {
            format!("{:.2e}", tfactory.output_error_rate())
        });

        // Pre-layout logical resources
        let logical_counts_num_qubits = format_metric_prefix(layout_report_data.num_qubits());
        let logical_counts_t_count = format_metric_prefix(layout_report_data.t_count());
        let logical_counts_rotation_count =
            format_metric_prefix(layout_report_data.rotation_count());
        let logical_counts_rotation_depth =
            format_metric_prefix(layout_report_data.rotation_depth());
        let logical_counts_ccz_count = format_metric_prefix(layout_report_data.ccz_count());
        let logical_counts_ccix_count = format_metric_prefix(layout_report_data.ccix_count());
        let logical_counts_measurement_count =
            format_metric_prefix(layout_report_data.measurement_count());

        // Assumed error budget
        let error_budget = format!("{:.2e}", job_params.error_budget().total());
        let error_budget_logical = format!("{:.2e}", result.error_budget().logical());
        let error_budget_tstates = format!("{:.2e}", result.error_budget().magic_states());
        let error_budget_rotations = format!("{:.2e}", result.error_budget().rotations());

        let num_ts_per_rotation = layout_report_data
            .num_ts_per_rotation(result.error_budget().rotations())
            .map_or_else(|| String::from(no_rotations_msg), format_metric_prefix);

        let constraint_not_set_msg = "constraint not set";

        let logical_depth_factor = job_params
            .constraints()
            .logical_depth_factor
            .map_or(String::from(constraint_not_set_msg), |v| format!("{v}"));
        let max_t_factories = job_params
            .constraints()
            .max_t_factories
            .map_or(String::from(constraint_not_set_msg), |v| format!("{v}"));
        let max_duration = job_params
            .constraints()
            .max_duration
            .map_or(String::from(constraint_not_set_msg), |v| format!("{v}"));
        let max_physical_qubits = job_params
            .constraints()
            .max_physical_qubits
            .map_or(String::from(constraint_not_set_msg), |v| format!("{v}"));

        Self {
            runtime,
            rqops,
            physical_qubits,
            algorithmic_logical_qubits,
            algorithmic_logical_depth,
            logical_depth,
            num_tstates,
            num_tfactories,
            num_tfactory_runs,
            physical_qubits_for_algorithm,
            physical_qubits_for_tfactories,
            physical_qubits_for_tfactories_percentage,
            required_logical_qubit_error_rate,
            required_logical_tstate_error_rate,
            physical_qubits_per_logical_qubit,
            logical_cycle_time,
            clock_frequency,
            logical_error_rate,
            tfactory_physical_qubits,
            tfactory_runtime,
            num_input_tstates,
            num_units_per_round,
            unit_name_per_round,
            code_distance_per_round,
            physical_qubits_per_round,
            tfactory_runtime_per_round,
            tstate_logical_error_rate,
            logical_counts_num_qubits,
            logical_counts_t_count,
            logical_counts_rotation_count,
            logical_counts_rotation_depth,
            logical_counts_ccz_count,
            logical_counts_ccix_count,
            logical_counts_measurement_count,
            error_budget,
            error_budget_logical,
            error_budget_tstates,
            error_budget_rotations,
            num_ts_per_rotation,
            logical_depth_factor,
            max_t_factories,
            max_duration,
            max_physical_qubits,
        }
    }
}

fn format_thousand_sep(val: &impl ToString) -> String {
    val.to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .expect("Invalid utf-8 encoding")
        .join(",")
}

fn format_thousand_sep_f64(val: f64) -> String {
    // 92592.5925925926

    let formatted = format!("{val:.2}");
    // SAFETY: formatting guarantees that there is a .
    let (i, f) = formatted.split_once('.').expect("Invalid formatting");

    format!("{}.{f}", format_thousand_sep(&i))
}

#[must_use]
pub fn format_metric_prefix(val: u64) -> String {
    if val < 1000 {
        val.to_string()
    } else {
        let prefixes = b"kMGTPEZYRQ";

        let mut prefix_index = 0;

        let mut val = val as f64 / 1e3;

        loop {
            let next_val = val / 1e3;
            if next_val < 1.0 {
                break;
            }

            prefix_index += 1;
            val = next_val;
        }

        format!(
            "{:.2}{}",
            val,
            std::str::from_utf8(prefixes.get(prefix_index..=prefix_index).unwrap_or(b"?"))
                .expect("Invalid utf-8 encoding")
                .trim()
        )
    }
}

#[must_use]
pub fn format_duration(nanos: u128) -> String {
    let units = [
        ("nanosecs", 1),
        ("microsecs", 1000),
        ("millisecs", 1000),
        ("secs", 1000),
        ("mins", 60),
        ("hours", 60),
        ("days", 24),
        ("years", 365),
    ];

    let (mut runtime, mut rem) = (nanos, 0u128);
    let mut runtime_formatted = None;

    for idx in 1..units.len() {
        if runtime / units[idx].1 == 0 {
            if rem >= units[idx - 1].1 / 2 {
                runtime += 1;
            }
            runtime_formatted = Some(format!("{runtime} {}", units[idx - 1].0));
            break;
        }

        (runtime, rem) = (runtime / units[idx].1, runtime % units[idx].1);
    }

    runtime_formatted.unwrap_or_else(|| format!("{runtime} {}", units[units.len() - 1].0))
}

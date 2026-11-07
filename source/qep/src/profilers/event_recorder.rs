// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    ExecutionTrace, InstructionTrace, TemporalProfiles,
    aggregators::{AggregationType, TumblingWindowAggregator},
};

/// Stores optional execution traces and aggregated event metrics.
pub struct EventRecorder {
    /// Aggregation window size in nanoseconds.
    pub aggregation_window_nanos: usize,
    /// Instruction execution intervals recorded in start order.
    pub logs: Vec<InstructionTrace>,
    /// Whether instruction and factory-round intervals should be retained.
    pub record_trace: bool,
    /// Collects event counts into fixed-window rate profiles.
    pub event_aggregator: TumblingWindowAggregator,
    /// Collects factory busy durations into fixed-window utilization profiles.
    pub utilization_aggregator: TumblingWindowAggregator,
}

impl EventRecorder {
    /// Creates an empty recorder with the specified trace setting and aggregation window.
    ///
    /// # Panics
    ///
    /// Panics if `aggregation_window_nanos` is zero.
    pub fn new(record_trace: bool, aggregation_window_nanos: usize) -> Self {
        Self {
            aggregation_window_nanos,
            logs: Vec::new(),
            record_trace,
            event_aggregator: TumblingWindowAggregator::new(
                aggregation_window_nanos,
                AggregationType::Rate,
            ),
            utilization_aggregator: TumblingWindowAggregator::new(
                aggregation_window_nanos,
                AggregationType::Utilization,
            ),
        }
    }

    /// Combines sorted event rates and unscaled factory utilization with window metadata.
    #[allow(clippy::cast_precision_loss)]
    pub fn get_profiles(&self) -> TemporalProfiles {
        let mut profiles = self.event_aggregator.get_profiles("Event rates");
        profiles.extend(self.utilization_aggregator.get_profiles("Utilization"));
        profiles.sort_by(|left, right| left.metric_name.cmp(&right.metric_name));
        TemporalProfiles {
            profiles,
            aggregation_window_sec: Some(self.aggregation_window_nanos as f64 / 1e9),
        }
    }

    /// Returns recorded intervals only when trace recording is enabled.
    pub fn get_trace(&self) -> Option<ExecutionTrace> {
        self.record_trace.then(|| ExecutionTrace {
            instructions: self.logs.clone(),
        })
    }
}

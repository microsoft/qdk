// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    InstructionTrace,
    aggregators::{AggregationType, TumblingWindowAggregator},
};

/// Stores optional execution traces and aggregated event metrics.
pub struct EventRecorder {
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
}

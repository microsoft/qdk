// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The census's engine wrapper: it forwards every call to the engine it
//! wraps, and records each batch's calls, draws and splits into a
//! [`ShotTrace`]'s batches.
//!
//! A batch starts where the resumer routes it: the routing trace's batch
//! count, read at each engine call, rises by one before the batch's first
//! call. The engine splits a pair lazily, at a later call, so the split log
//! is drained at the first call of the next batch, before that call runs:
//! the splits a batch's gates cause stay with the batch. (The resumer reads
//! the largest bond before resuming, which flushes the last pair.)

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use num_complex::Complex64;
use qdk_cpu_mps::dev_trace;
use qdk_simulators::mps_simulator::routing_trace;
use qdk_simulators::qir::QirGate;
use tensornet::{MpsEngine, MpsShape, PauliSum};

use crate::trace::{BatchTrace, Draw};

/// The engine `E`, its calls recorded batch by batch.
pub struct CensusProbe<E> {
    pub inner: E,
    pub recorder: Rc<RefCell<BatchRecorder>>,
}

/// The batches recorded so far, and the one under way.
pub struct BatchRecorder {
    batches: Vec<BatchTrace>,
    /// The batch under way (its index), its keys, draws and splits.
    current: Option<usize>,
    exact: Fnv,
    shape: Fnv,
    draws: Vec<Draw>,
    splits: Vec<dev_trace::SplitEvent>,
    /// The site just projected on One: an X on it next is a reset's flip,
    /// which the keys leave out, as they leave out the drawn value.
    flipped_site: Option<usize>,
    x: Vec<Complex64>,
    /// The batch whose calls are printed, and the shot's id.
    dump: Option<(usize, u32)>,
    /// The shot's id and start, for a progress line every
    /// `PROGRESS_EVERY` batches.
    progress: (u32, Instant),
    /// The batches recorded; from this batch on the probe stops applying
    /// operators (the state is frozen, so the shot ends fast) and the
    /// trace keeps only the batches before it.
    batch_limit: Option<usize>,
}

const PROGRESS_EVERY: usize = 100;

impl BatchRecorder {
    pub fn new(shot: u32, dump: Option<usize>, batch_limit: Option<usize>) -> Self {
        Self {
            batches: Vec::new(),
            current: None,
            exact: Fnv::new(),
            shape: Fnv::new(),
            draws: Vec::new(),
            splits: Vec::new(),
            flipped_site: None,
            x: QirGate::X.matrix_at(None).row_major().to_vec(),
            dump: dump.map(|batch| (batch, shot)),
            progress: (shot, Instant::now()),
            batch_limit,
        }
    }

    /// Whether the batch under way is past the limit: its operators are
    /// not applied.
    fn frozen(&self) -> bool {
        self.batch_limit
            .zip(self.current)
            .is_some_and(|(limit, batch)| batch >= limit)
    }

    /// Prints `call` when the batch under way is the one dumped.
    fn print(&self, call: std::fmt::Arguments) {
        if let Some((batch, shot)) = self.dump
            && self.current == Some(batch)
        {
            println!("CALL shot={shot} batch={batch} {call}");
        }
    }

    /// Before an engine call: closes the batch under way when the resumer
    /// has routed a new one.
    fn enter(&mut self) {
        let batch = usize::try_from(routing_trace().batches.saturating_sub(1))
            .expect("a batch count fits a usize");
        if self.current != Some(batch) {
            self.close();
            while self.batches.len() < batch {
                self.batches.push(BatchTrace::default());
            }
            self.current = Some(batch);
        }
    }

    /// Closes the batch under way, if any, with the splits logged so far.
    fn close(&mut self) {
        if self.current.take().is_some() {
            self.splits.extend(dev_trace::take_log());
            self.batches.push(BatchTrace {
                exact: std::mem::replace(&mut self.exact, Fnv::new()).0,
                shape: std::mem::replace(&mut self.shape, Fnv::new()).0,
                draws: std::mem::take(&mut self.draws),
                splits: std::mem::take(&mut self.splits),
            });
            let done = self.batches.len();
            if done.is_multiple_of(PROGRESS_EVERY) {
                let last = &self.batches[done - PROGRESS_EVERY..];
                let largest = |side: fn(&dev_trace::SplitEvent) -> u16| {
                    last.iter()
                        .flat_map(|b| &b.splits)
                        .map(side)
                        .max()
                        .unwrap_or(0)
                };
                let (shot, start) = self.progress;
                eprintln!(
                    "PROGRESS shot={shot} batches={done} seconds={:.1} largest_kept={} \
                     largest_side={}",
                    start.elapsed().as_secs_f64(),
                    largest(|s| s.kept),
                    largest(|s| s.left.max(s.right)),
                );
            }
        }
    }

    /// The batches of a shot that ran `batches` of them.
    pub fn finish(mut self, batches: u64) -> Vec<BatchTrace> {
        self.close();
        let batches = usize::try_from(batches).expect("a batch count fits a usize");
        while self.batches.len() < batches {
            self.batches.push(BatchTrace::default());
        }
        if let Some(limit) = self.batch_limit {
            self.batches.truncate(limit);
        }
        self.batches
    }

    fn apply(&mut self, sites: &[usize], matrix: &[Complex64]) {
        self.enter();
        let flip = self.flipped_site.take();
        if sites.len() == 1 && flip == Some(sites[0]) && matrix == self.x.as_slice() {
            self.print(format_args!("reset-flip {sites:?}"));
            return;
        }
        let mut entries = Fnv::new();
        for entry in matrix {
            entries.u64(entry.re.to_bits());
            entries.u64(entry.im.to_bits());
        }
        self.print(format_args!("apply {sites:?} matrix={:016x}", entries.0));
        for key in [&mut self.exact, &mut self.shape] {
            key.byte(b'A');
            for &site in sites {
                key.u64(site as u64);
            }
        }
        for entry in matrix {
            self.exact.u64(entry.re.to_bits());
            self.exact.u64(entry.im.to_bits());
        }
    }

    fn probability_of_one(&mut self, site: usize) {
        self.enter();
        self.flipped_site = None;
        self.splits.extend(dev_trace::take_log());
        self.print(format_args!(
            "draw {site} after_splits={}",
            self.splits.len()
        ));
        for key in [&mut self.exact, &mut self.shape] {
            key.byte(b'D');
            key.u64(site as u64);
        }
        self.draws.push(Draw {
            site: u16::try_from(site).expect("a site fits a u16"),
            one: false,
            after_splits: u32::try_from(self.splits.len()).expect("fits a u32"),
        });
    }

    fn project(&mut self, site: usize, one: bool) {
        self.enter();
        let draw = self
            .draws
            .last_mut()
            .expect("the resumer reads P(One) before it projects");
        assert_eq!(
            usize::from(draw.site),
            site,
            "a projection on the drawn site"
        );
        draw.one = one;
        self.flipped_site = one.then_some(site);
        self.print(format_args!("project {site} one={one}"));
    }

    fn other(&mut self) {
        self.enter();
        self.flipped_site = None;
    }
}

/// FNV-1a, 64 bits.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn byte(&mut self, byte: u8) {
        self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }

    fn u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }
}

impl<E: MpsEngine> MpsEngine for CensusProbe<E> {
    type State = E::State;
    type Error = E::Error;

    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, E::Error> {
        self.recorder.borrow_mut().other();
        self.inner.prepare(capacity)
    }

    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), E::Error> {
        let mut recorder = self.recorder.borrow_mut();
        recorder.apply(sites, matrix);
        if recorder.frozen() {
            return Ok(());
        }
        drop(recorder);
        self.inner.apply(state, sites, matrix)
    }

    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, E::Error> {
        self.recorder.borrow_mut().probability_of_one(site);
        self.inner.probability_of_one(state, site)
    }

    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, E::Error> {
        self.recorder.borrow_mut().project(site, one);
        self.inner.project(state, site, one)
    }

    fn fork(&self, state: &Self::State) -> Result<Self::State, E::Error> {
        self.recorder.borrow_mut().other();
        self.inner.fork(state)
    }

    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, E::Error> {
        self.recorder.borrow_mut().other();
        self.inner.sample(state, sites, shots, seed)
    }

    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, E::Error> {
        self.recorder.borrow_mut().other();
        self.inner.expectation(state, observable)
    }

    fn shape(&self, state: &Self::State) -> Result<MpsShape, E::Error> {
        self.recorder.borrow_mut().other();
        self.inner.shape(state)
    }

    fn retained_weight(&self, state: &Self::State) -> Result<f64, E::Error> {
        self.recorder.borrow_mut().other();
        self.inner.retained_weight(state)
    }
}

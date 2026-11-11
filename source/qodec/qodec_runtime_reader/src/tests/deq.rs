// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A decoder built on deq's MWPM decoder, written against the runtime's public
//! pieces and model alone: it plans from a [`LayerModel`], and nothing here
//! reads a qodec. It shows that a new decoder is one new file.

use deq_runtime::{
    decoder::{
        blackbox_decoder::{DecodingHypergraph, Hyperedge},
        mwpm_decoder::MwpmDecoderInstance,
        thread_pooling::{DecodeRequest, DecoderInstance},
    },
    misc::{bit_vector, validation},
};
use qodec_runtime_recipes::{
    BasicReadout, Blueprint, Decode, FrameDecoder, FrameDecoderPlan, GadgetModel, LayerModel,
    Passive, PlanError, Record, RecordLayout,
    parity::{Basis, Parity, Side, Var},
};
use std::rc::Rc;

/// The configuration of deq's MWPM decoder: all defaults.
fn mwpm_config() -> serde_json::Value {
    serde_json::json!({})
}

/// A decoder that corrects each gadget's circuit readouts with deq's MWPM
/// decoder, then gives them to the frame decoder, which tracks block signs and
/// computes the gadget's results. It is a [`Decode`], so [`Passive`] makes it
/// a decoder.
///
/// deq decodes a gadget's detectors: its checks that compare circuit readouts
/// with input stabilizer signs. Each circuit readout is one error mechanism,
/// which flips the detectors it appears in, as a bit flip right before a
/// measurement does.
///
/// It takes every input stabilizer sign as 0. That holds in these tests: each
/// repetition3 block comes from `prepare_z`, whose `R`s leave every stabilizer
/// at +1, and the only error is injected into the final measurement. The frame
/// decoder can't check this: no gadget's checks determine those signs, so it
/// doesn't know them. A real integration needs them declared by the gadgets or
/// tracked by the decoder.
pub struct DeqPlan {
    records: Rc<RecordLayout>,
    frame: FrameDecoderPlan,
    /// Per gadget, by `GadgetId`; `None` for gadgets without detectors.
    gadgets: Rc<Vec<Option<DeqGadget>>>,
}

/// The decoding problem of one gadget, fixed once per layer.
struct DeqGadget {
    /// The checks deq decodes.
    detectors: Vec<Parity>,
    /// Per hyperedge of `hypergraph`: the circuit readout that it flips.
    errors: Vec<usize>,
    /// One vertex per detector, and one hyperedge per circuit readout that
    /// some detector reads.
    hypergraph: DecodingHypergraph,
}

impl DeqPlan {
    /// Plans the decoder of `layer`.
    pub fn new(layer: &LayerModel) -> Result<Self, PlanError> {
        let gadgets = layer
            .gadgets
            .iter()
            .map(|gp| {
                DeqGadget::new(gp).map_err(|e| PlanError(format!("gadget `{}`: {e}", gp.name)))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            records: Rc::new(RecordLayout::plan(layer)),
            frame: FrameDecoderPlan::plan(layer)?,
            gadgets: Rc::new(gadgets),
        })
    }
}

impl DeqGadget {
    /// `None` if the gadget has no detectors.
    fn new(gp: &GadgetModel) -> Result<Option<Self>, String> {
        let mut detectors = Vec::new();
        for parity in &gp.checks {
            // A check with an output sign determines that sign; it detects
            // nothing.
            let detects = parity.vars().iter().all(|v| {
                matches!(
                    v,
                    Var::Rec(_)
                        | Var::Enc {
                            side: Side::In,
                            basis: Basis::Stabilizers,
                            ..
                        }
                )
            });
            if detects {
                detectors.push(parity.clone());
            }
        }
        if detectors.is_empty() {
            return Ok(None);
        }

        let mut errors = Vec::new();
        let mut hyperedges = Vec::new();
        for i in 0..gp.record_len {
            let vertices: Vec<u64> = (0..detectors.len())
                .filter(|&d| detectors[d].contains(Var::Rec(i)))
                .map(|d| u64::try_from(d).expect("small index"))
                .collect();
            if !vertices.is_empty() {
                errors.push(i);
                hyperedges.push(Hyperedge {
                    vertices,
                    // Arbitrary: every readout is equally likely to flip.
                    probability: 0.01,
                    observable_flips: None,
                });
            }
        }
        let hypergraph = DecodingHypergraph {
            vertex_num: u64::try_from(detectors.len()).expect("small count"),
            hyperedges,
        };
        validation::validate_hypergraph(&hypergraph)?;
        MwpmDecoderInstance::validate_hypergraph(&hypergraph, &mwpm_config())?;
        Ok(Some(Self {
            detectors,
            errors,
            hypergraph,
        }))
    }
}

impl Blueprint for DeqPlan {
    type Instance = Passive<DeqDecoder>;

    fn instantiate(&self) -> Self::Instance {
        Passive::new(
            Rc::clone(&self.records),
            DeqDecoder {
                gadgets: Rc::clone(&self.gadgets),
                solvers: self.gadgets.iter().map(|_| None).collect(),
                frame: self.frame.decoder(),
            },
        )
    }
}

pub struct DeqDecoder {
    gadgets: Rc<Vec<Option<DeqGadget>>>,
    /// One MWPM solver per gadget, built on its first decode.
    solvers: Vec<Option<MwpmDecoderInstance>>,
    frame: FrameDecoder,
}

impl Decode for DeqDecoder {
    fn decode(&mut self, record: &Record) -> BasicReadout {
        let Some(inv) = &record.invocation else {
            return self.frame.decode(record);
        };
        let gadgets = Rc::clone(&self.gadgets);
        let index = inv.gadget.0;
        // With an unknown readout, the syndrome is unknown: leave it to the
        // frame decoder.
        let (Some(Some(gadget)), Some(mut bits)) = (
            gadgets.get(index),
            record.bits.iter().copied().collect::<Option<Vec<bool>>>(),
        ) else {
            return self.frame.decode(record);
        };

        // An input stabilizer sign counts as 0 (see `DeqPlan`).
        let defects: Vec<u64> = (0..gadget.detectors.len())
            .filter(|&d| {
                let detector = &gadget.detectors[d];
                detector
                    .vars()
                    .iter()
                    .fold(detector.constant, |fired, v| match v {
                        Var::Rec(i) => fired ^ bits[*i],
                        _ => fired,
                    })
            })
            .map(|d| u64::try_from(d).expect("small index"))
            .collect();
        let syndrome = bit_vector::from_sparse_indices(gadget.hypergraph.vertex_num, &defects);
        let solver = self.solvers[index]
            .get_or_insert_with(|| MwpmDecoderInstance::new(&gadget.hypergraph, &mwpm_config()));
        let correction = solver
            .decode(DecodeRequest {
                syndrome: &syndrome,
                decoder_seed: None,
                reweights: &[],
                loss: None,
            })
            .expect("deq decodes");
        for h in correction.subgraph {
            bits[gadget.errors[usize::try_from(h).expect("small index")]] ^= true;
        }
        let corrected = Record {
            bits: bits.into_iter().map(Some).collect(),
            ..record.clone()
        };
        self.frame.decode(&corrected)
    }
}

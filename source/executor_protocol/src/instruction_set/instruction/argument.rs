// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An instruction's argument ([`Argument`]), one per parameter of its
//! kind; and a `bit` argument's value ([`Bit`]), known when it is written
//! or resolved as the shot runs, from the shot's earlier readouts
//! ([`ReadoutParity`]).

use super::super::{ParameterKind, Pauli};
use super::InstructionFault;
use std::fmt;
use std::sync::Arc;

/// The value of one of an instruction's parameters, of the parameter's
/// kind. Text, Paulis and the rare readout parity of more than five
/// readouts are shared, so that copying one allocates nothing.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Argument {
    /// For a `bit` parameter.
    Bit(Bit),
    /// For a `number` parameter.
    Number(f64),
    /// For an `integer` parameter.
    Integer(i64),
    /// For a `boolean` parameter.
    Boolean(bool),
    /// For a `string` parameter.
    Text(Arc<str>),
    /// In Qodec's sparse form over the instruction's qubits (not checked),
    /// `Z_0 Z_2`.
    Pauli(Arc<Pauli>),
}

// An instruction holds up to three arguments inline, and its size sets how
// fast a batch runs, as the crate's bench measures.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Argument>() == 24);

impl Argument {
    /// The shot's `k`-th latest readout, Stim's `rec[-k]`: `recent(1)` is
    /// the latest. `k` counts from 1; an instruction with `recent(0)` is
    /// refused when it is checked
    /// ([`ZeroLookback`](InstructionFault::ZeroLookback)).
    #[must_use]
    pub fn recent(k: u32) -> Self {
        Self::parity([k], false)
    }

    /// The XOR of `flip` and of the shot's readouts counted back by each of
    /// `back`, as [`ReadoutParity`] reads it. An empty `back` is the
    /// constant `flip`, yet still a readout parity: its shots' needs must
    /// use readout parities
    /// ([`ParityNotProvisioned`](InstructionFault::ParityNotProvisioned)
    /// otherwise); [`Bit::Value`] is a constant that needs none.
    #[must_use]
    pub fn parity(back: impl IntoIterator<Item = u32>, flip: bool) -> Self {
        Argument::Bit(Bit::Parity(ReadoutParity::new(back, flip)))
    }

    /// The Pauli of Qodec's text: an optional sign (`-`, `+`), an optional
    /// `i`, then factors `X_k`, `Y_k`, `Z_k` or `I_k` (the identity on qubit
    /// `k`, which only names the qubit), or `I`, separated by spaces, each
    /// qubit at most once: `-X_0 Z_2`, `i Y_1`, `X_0 I_3`, `-I`.
    ///
    /// # Errors
    ///
    /// [`MalformedPauli`](InstructionFault::MalformedPauli) for any other
    /// text.
    pub fn pauli(text: &str) -> Result<Self, InstructionFault> {
        Pauli::parse(text)
            .map(|pauli| Argument::Pauli(Arc::new(pauli)))
            .ok_or_else(|| InstructionFault::MalformedPauli(text.to_owned()))
    }

    /// The kind of parameter it fills.
    #[must_use]
    pub fn kind(&self) -> ParameterKind {
        match self {
            Argument::Bit(_) => ParameterKind::Bit,
            Argument::Number(_) => ParameterKind::Number,
            Argument::Integer(_) => ParameterKind::Integer,
            Argument::Boolean(_) => ParameterKind::Boolean,
            Argument::Text(_) => ParameterKind::Text,
            Argument::Pauli(_) => ParameterKind::Pauli,
        }
    }
}

impl From<f64> for Argument {
    fn from(value: f64) -> Self {
        Argument::Number(value)
    }
}

impl From<i64> for Argument {
    fn from(value: i64) -> Self {
        Argument::Integer(value)
    }
}

impl From<bool> for Argument {
    /// A `boolean` argument; a known `bit` is `Argument::Bit(Bit::Value(_))`.
    fn from(value: bool) -> Self {
        Argument::Boolean(value)
    }
}

impl From<&str> for Argument {
    /// A `string` argument; a Pauli is parsed by [`Argument::pauli`].
    fn from(value: &str) -> Self {
        Argument::Text(value.into())
    }
}

/// The value of a `bit` argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bit {
    /// A bit its sender knows, such as an earlier `execute`'s readout.
    Value(bool),
    /// A bit resolved per shot by the executor, as the shot runs.
    Parity(ReadoutParity),
}

/// The XOR of `flip` and of the shot's readouts counted back from its
/// latest: `back: [1, 3]` is Stim's `rec[-1] ^ rec[-3]`. It counts over
/// the shot's whole history, across `execute`s, so one instruction means
/// the same on every shot of a batch, and splitting instructions into
/// smaller batches changes nothing. A parity, as Qodec's `if:
/// [correction, outcomes[0]]`, because below a code one logical readout is
/// the XOR of several physical ones.
///
/// Physicists call this feed-forward: a shot's readouts steer its later
/// instructions, which a device does with real-time classical control.
///
/// Up to five readouts are held inline, and more in one shared allocation,
/// so that the parity, and with it every [`Argument`], takes 24 bytes on a
/// 64-bit target: an instruction holds its arguments inline, and its size
/// sets how fast a batch runs.
#[derive(Clone)]
pub struct ReadoutParity(Repr);

impl ReadoutParity {
    /// What [`Argument::parity`] builds: inline up to [`INLINE`] readouts,
    /// shared beyond.
    fn new(back: impl IntoIterator<Item = u32>, flip: bool) -> Self {
        let mut inline = [0; INLINE];
        let mut count = 0;
        let mut back = back.into_iter();
        while let Some(next) = back.next() {
            if count == INLINE {
                let all = inline.into_iter().chain([next]).chain(back).collect();
                return Self(Repr::Spilled { flip, back: all });
            }
            inline[count] = next;
            count += 1;
        }
        Self(Repr::Inline {
            header: Header::new(flip, count),
            back: inline,
        })
    }

    /// Whether the parity is flipped: the XOR's constant term.
    #[must_use]
    pub fn flip(&self) -> bool {
        match &self.0 {
            Repr::Inline { header, .. } => header.flip(),
            Repr::Spilled { flip, .. } => *flip,
        }
    }

    /// The readouts it XORs, each counted back from the shot's latest, 1
    /// for the latest; each at least 1 once checked.
    #[must_use]
    pub fn back(&self) -> &[u32] {
        match &self.0 {
            Repr::Inline { header, back } => &back[..header.count()],
            Repr::Spilled { back, .. } => back,
        }
    }
}

/// As `ReadoutParity { flip, back }` would print, whether inline or shared.
impl fmt::Debug for ReadoutParity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadoutParity")
            .field("flip", &self.flip())
            .field("back", &self.back())
            .finish()
    }
}

/// Equal when the flips and the readouts are, whether inline or shared.
impl PartialEq for ReadoutParity {
    fn eq(&self, other: &Self) -> bool {
        self.flip() == other.flip() && self.back() == other.back()
    }
}

impl Eq for ReadoutParity {}

/// Readouts held inline; more are shared ([`Repr::Spilled`]).
const INLINE: usize = 5;

/// A parity's flip and readouts, inline or shared.
#[derive(Clone)]
enum Repr {
    /// The flip and up to [`INLINE`] readouts: `back[..header.count()]`, the
    /// rest zero.
    Inline { header: Header, back: [u32; INLINE] },
    /// More than [`INLINE`] readouts, shared so that a copy allocates
    /// nothing.
    Spilled { flip: bool, back: Arc<[u32]> },
}

/// An inline parity's flip and count, in one byte. Its values not listed
/// here hold the tags of the enums around it (`Repr`, [`Bit`],
/// [`Argument`]), so that none of them grows.
#[derive(Clone, Copy)]
#[repr(u8)]
enum Header {
    Count0 = 0,
    Count1,
    Count2,
    Count3,
    Count4,
    Count5,
    FlippedCount0 = Header::FLIPPED,
    FlippedCount1,
    FlippedCount2,
    FlippedCount3,
    FlippedCount4,
    FlippedCount5,
}

impl Header {
    /// The bit that marks a flipped parity; the bits below it count.
    const FLIPPED: u8 = 8;

    /// Indexed by the flip, then by the count.
    const ALL: [[Header; INLINE + 1]; 2] = [
        [
            Header::Count0,
            Header::Count1,
            Header::Count2,
            Header::Count3,
            Header::Count4,
            Header::Count5,
        ],
        [
            Header::FlippedCount0,
            Header::FlippedCount1,
            Header::FlippedCount2,
            Header::FlippedCount3,
            Header::FlippedCount4,
            Header::FlippedCount5,
        ],
    ];

    fn new(flip: bool, count: usize) -> Self {
        Self::ALL[usize::from(flip)][count]
    }

    fn flip(self) -> bool {
        self as u8 & Self::FLIPPED != 0
    }

    fn count(self) -> usize {
        usize::from(self as u8 & (Self::FLIPPED - 1))
    }
}

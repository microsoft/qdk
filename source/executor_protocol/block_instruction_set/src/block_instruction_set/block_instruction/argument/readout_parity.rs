// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A `bit` argument resolved as the shot runs: [`ReadoutParity`], the XOR
//! of some of the shot's earlier readouts, and its representation, which
//! keeps every [`Argument`](super::Argument) at 24 bytes.

use std::fmt;
use std::sync::Arc;

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
/// so that the parity, and with it every [`Argument`](super::Argument),
/// takes 24 bytes on a 64-bit target: an instruction holds its arguments
/// inline, and its size sets how fast a batch runs.
#[derive(Clone)]
pub struct ReadoutParity(Repr);

impl ReadoutParity {
    /// What [`Argument::parity`](super::Argument::parity) builds: inline up to
    /// [`INLINE_READOUTS`] readouts, shared beyond.
    pub(super) fn new(back: impl IntoIterator<Item = u32>, flip: bool) -> Self {
        let mut inline = [0; INLINE_READOUTS];
        let mut count = 0;
        let mut back = back.into_iter();
        while let Some(next) = back.next() {
            if count == INLINE_READOUTS {
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

/// Names its flip and its readouts, whether inline or shared.
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
const INLINE_READOUTS: usize = 5;

/// A parity's flip and readouts, inline or shared.
#[derive(Clone)]
enum Repr {
    /// The flip and up to [`INLINE_READOUTS`] readouts:
    /// `back[..header.count()]`, the rest zero.
    Inline {
        header: Header,
        back: [u32; INLINE_READOUTS],
    },
    /// More than [`INLINE_READOUTS`] readouts, shared so that a copy
    /// allocates nothing.
    Spilled { flip: bool, back: Arc<[u32]> },
}

/// An inline parity's flip and count, in one byte. Its values above
/// `FlippedCount5` hold the tags of the enums around it (`Repr`,
/// [`Bit`](super::Bit), [`Argument`](super::Argument)), so that none of
/// them grows.
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
    const ALL: [[Header; INLINE_READOUTS + 1]; 2] = [
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

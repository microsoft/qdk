// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's trace: for each batch the resumer ran, the keys of its engine
//! calls, its draws and its splits, written to and read from one binary
//! file per shot.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use qdk_cpu_mps::dev_trace::SplitEvent;

/// What a shot did, batch by batch.
pub struct ShotTrace {
    pub shot: u32,
    pub seconds: f64,
    pub records: String,
    pub batches: Vec<BatchTrace>,
}

/// One batch of a shot, as the engine saw it.
#[derive(Default)]
pub struct BatchTrace {
    /// A hash of the engine calls: the sites, kinds and matrices of the
    /// operators, and the sites of the draws (not their values).
    pub exact: u64,
    /// A hash of the sites and kinds of the engine calls alone.
    pub shape: u64,
    pub draws: Vec<Draw>,
    pub splits: Vec<SplitEvent>,
}

/// A draw: its site, its value, and the splits of the batch before it.
#[derive(Clone, Copy)]
pub struct Draw {
    pub site: u16,
    pub one: bool,
    pub after_splits: u32,
}

const MAGIC: &[u8; 4] = b"LSC1";

impl ShotTrace {
    /// The file of shot `shot` in `dir`.
    pub fn path(dir: &Path, shot: u32) -> PathBuf {
        dir.join(format!("shot-{shot:05}.trace"))
    }

    pub fn write(&self, dir: &Path) -> io::Result<()> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.shot.to_le_bytes());
        out.extend_from_slice(&self.seconds.to_le_bytes());
        let records = self.records.as_bytes();
        out.extend_from_slice(&len32(records.len()).to_le_bytes());
        out.extend_from_slice(records);
        out.extend_from_slice(&len32(self.batches.len()).to_le_bytes());
        for batch in &self.batches {
            out.extend_from_slice(&batch.exact.to_le_bytes());
            out.extend_from_slice(&batch.shape.to_le_bytes());
            out.extend_from_slice(&len32(batch.draws.len()).to_le_bytes());
            for draw in &batch.draws {
                out.extend_from_slice(&draw.site.to_le_bytes());
                out.push(u8::from(draw.one));
                out.extend_from_slice(&draw.after_splits.to_le_bytes());
            }
            out.extend_from_slice(&len32(batch.splits.len()).to_le_bytes());
            for split in &batch.splits {
                for field in [split.cut, split.left, split.right, split.kept] {
                    out.extend_from_slice(&field.to_le_bytes());
                }
            }
        }
        let path = Self::path(dir, self.shot);
        let partial = path.with_extension("partial");
        std::fs::File::create(&partial)?.write_all(&out)?;
        std::fs::rename(partial, path)
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut bytes)?;
        let mut input = Input {
            bytes: &bytes,
            at: 0,
        };
        if input.take(4) != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a trace"));
        }
        let shot = input.u32();
        let seconds = f64::from_le_bytes(input.array());
        let records_len = input.len();
        let records = String::from_utf8_lossy(input.take(records_len)).into_owned();
        let batches = (0..input.len())
            .map(|_| {
                let exact = input.u64();
                let shape = input.u64();
                let draws = (0..input.len())
                    .map(|_| Draw {
                        site: input.u16(),
                        one: input.take(1)[0] != 0,
                        after_splits: input.u32(),
                    })
                    .collect();
                let splits = (0..input.len())
                    .map(|_| SplitEvent {
                        cut: input.u16(),
                        left: input.u16(),
                        right: input.u16(),
                        kept: input.u16(),
                    })
                    .collect();
                BatchTrace {
                    exact,
                    shape,
                    draws,
                    splits,
                }
            })
            .collect();
        Ok(Self {
            shot,
            seconds,
            records,
            batches,
        })
    }
}

fn len32(len: usize) -> u32 {
    u32::try_from(len).expect("a trace's lengths fit a u32")
}

/// A cursor over a trace file's bytes; a truncated file panics.
struct Input<'b> {
    bytes: &'b [u8],
    at: usize,
}

impl<'b> Input<'b> {
    fn take(&mut self, n: usize) -> &'b [u8] {
        let taken = &self.bytes[self.at..self.at + n];
        self.at += n;
        taken
    }

    fn array<const N: usize>(&mut self) -> [u8; N] {
        self.take(N).try_into().expect("N bytes taken")
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.array())
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.array())
    }

    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.array())
    }

    fn len(&mut self) -> usize {
        usize::try_from(self.u32()).expect("a u32 fits a usize")
    }
}

// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;

struct BlockScope {
    #[allow(dead_code)]
    outer_abbrev_width: u32,
    length_position: usize,
    start_position: usize,
    saved_next_abbrev_id: u32,
    saved_abbrevs: Vec<AbbrevDef>,
}

pub struct BitstreamWriter {
    buffer: Vec<u8>,
    cur_byte: u8,
    cur_bit: u32,
    block_stack: Vec<BlockScope>,
    next_abbrev_id: u32,
    defined_abbrevs: Vec<AbbrevDef>,
}

impl BitstreamWriter {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            cur_byte: 0,
            cur_bit: 0,
            block_stack: Vec::new(),
            next_abbrev_id: 4,
            defined_abbrevs: Vec::new(),
        }
    }

    pub(crate) fn bit_position(&self) -> usize {
        self.buffer.len() * 8 + self.cur_bit as usize
    }

    pub fn emit_bits(&mut self, val: u64, width: u32) {
        let mut remaining = width;
        let mut v = val;
        while remaining > 0 {
            let bits_free = 8 - self.cur_bit;
            let to_write = remaining.min(bits_free);
            let mask = if to_write == 64 {
                u64::MAX
            } else {
                (1u64 << to_write) - 1
            };
            self.cur_byte |= ((v & mask) as u8) << self.cur_bit;
            self.cur_bit += to_write;
            v >>= to_write;
            remaining -= to_write;
            if self.cur_bit == 8 {
                self.buffer.push(self.cur_byte);
                self.cur_byte = 0;
                self.cur_bit = 0;
            }
        }
    }

    pub fn emit_vbr(&mut self, val: u64, chunk_width: u32) {
        let data_bits = chunk_width - 1;
        let data_mask = (1u64 << data_bits) - 1;
        let mut v = val;
        loop {
            let chunk = v & data_mask;
            v >>= data_bits;
            if v == 0 {
                self.emit_bits(chunk, chunk_width);
                break;
            }
            self.emit_bits(chunk | (1u64 << data_bits), chunk_width);
        }
    }

    pub fn enter_subblock(
        &mut self,
        block_id: u32,
        new_abbrev_width: u32,
        current_abbrev_width: u32,
    ) {
        // ENTER_SUBBLOCK abbrev id = 1
        self.emit_bits(1, current_abbrev_width);
        self.emit_vbr(u64::from(block_id), 8);
        self.emit_vbr(u64::from(new_abbrev_width), 4);
        self.align32();

        let length_position = self.buffer.len();
        // Write 4 zero bytes as placeholder for block length
        self.buffer.extend_from_slice(&[0u8; 4]);
        let start_position = self.buffer.len();

        let saved_next_abbrev_id = self.next_abbrev_id;
        let saved_abbrevs = std::mem::take(&mut self.defined_abbrevs);
        self.next_abbrev_id = 4;

        self.block_stack.push(BlockScope {
            outer_abbrev_width: current_abbrev_width,
            length_position,
            start_position,
            saved_next_abbrev_id,
            saved_abbrevs,
        });
    }

    pub fn exit_block(&mut self, current_abbrev_width: u32) {
        // END_BLOCK abbrev id = 0
        self.emit_bits(0, current_abbrev_width);
        self.align32();

        let scope = self.block_stack.pop().expect("no block to exit");
        let content_len = self.buffer.len() - scope.start_position;
        let len_words = (content_len / 4) as u32;
        let bytes = len_words.to_le_bytes();
        self.buffer[scope.length_position..scope.length_position + 4].copy_from_slice(&bytes);
        self.next_abbrev_id = scope.saved_next_abbrev_id;
        self.defined_abbrevs = scope.saved_abbrevs;
    }

    pub fn emit_record(&mut self, code: u32, values: &[u64], abbrev_width: u32) {
        // UNABBREV_RECORD abbrev id = 3
        self.emit_bits(3, abbrev_width);
        self.emit_vbr(u64::from(code), 6);
        self.emit_vbr(values.len() as u64, 6);
        for &v in values {
            self.emit_vbr(v, 6);
        }
    }

    /// Emit a ``DEFINE_ABBREV`` record and return the abbreviation ID assigned.
    /// The first abbreviation in a block gets ID 4 (IDs 0-3 are reserved).
    #[allow(dead_code)]
    pub(crate) fn emit_define_abbrev(&mut self, abbrev: &AbbrevDef, abbrev_width: u32) -> u32 {
        // DEFINE_ABBREV abbrev id = 2
        self.emit_bits(2, abbrev_width);
        // Count operands: Arrays count as 2 (array + element)
        let num_ops: usize = abbrev
            .operands
            .iter()
            .map(|op| {
                if matches!(op, AbbrevOperand::Array(_)) {
                    2
                } else {
                    1
                }
            })
            .sum();
        self.emit_vbr(num_ops as u64, 5);
        for op in &abbrev.operands {
            self.emit_abbrev_operand(op);
        }
        let id = self.next_abbrev_id;
        self.defined_abbrevs.push(abbrev.clone());
        self.next_abbrev_id += 1;
        id
    }

    #[allow(dead_code)]
    fn emit_abbrev_operand(&mut self, op: &AbbrevOperand) {
        match op {
            AbbrevOperand::Literal(v) => {
                self.emit_bits(1, 1); // is_literal = true
                self.emit_vbr(*v, 8);
            }
            AbbrevOperand::Fixed(w) => {
                self.emit_bits(0, 1);
                self.emit_bits(1, 3); // encoding = Fixed
                self.emit_vbr(u64::from(*w), 5);
            }
            AbbrevOperand::Vbr(w) => {
                self.emit_bits(0, 1);
                self.emit_bits(2, 3); // encoding = VBR
                self.emit_vbr(u64::from(*w), 5);
            }
            AbbrevOperand::Array(elem) => {
                self.emit_bits(0, 1);
                self.emit_bits(3, 3); // encoding = Array
                self.emit_abbrev_operand(elem);
            }
            AbbrevOperand::Char6 => {
                self.emit_bits(0, 1);
                self.emit_bits(4, 3); // encoding = Char6
            }
            AbbrevOperand::Blob => {
                self.emit_bits(0, 1);
                self.emit_bits(5, 3); // encoding = Blob
            }
        }
    }

    /// Emit a record using a previously defined abbreviation.
    /// `fields` contains values for all non-literal operands in order.
    #[allow(dead_code)]
    pub(crate) fn emit_abbreviated_record(
        &mut self,
        abbrev_id: u32,
        fields: &[u64],
        abbrev_width: u32,
    ) {
        let def_idx = (abbrev_id - 4) as usize;
        let abbrev = self.defined_abbrevs[def_idx].clone();
        self.emit_bits(u64::from(abbrev_id), abbrev_width);
        let mut field_idx = 0;
        for op in &abbrev.operands {
            match op {
                AbbrevOperand::Literal(_) => {} // implicit, not emitted
                AbbrevOperand::Fixed(w) => {
                    self.emit_bits(fields[field_idx], *w);
                    field_idx += 1;
                }
                AbbrevOperand::Vbr(w) => {
                    self.emit_vbr(fields[field_idx], *w);
                    field_idx += 1;
                }
                AbbrevOperand::Char6 => {
                    self.emit_bits(u64::from(encode_char6(fields[field_idx] as u8)), 6);
                    field_idx += 1;
                }
                AbbrevOperand::Array(elem) => {
                    let count = fields[field_idx] as usize;
                    self.emit_vbr(fields[field_idx], 6);
                    field_idx += 1;
                    for _ in 0..count {
                        self.emit_array_element(elem, fields[field_idx]);
                        field_idx += 1;
                    }
                }
                AbbrevOperand::Blob => {
                    let len = fields[field_idx] as usize;
                    self.emit_vbr(fields[field_idx], 6);
                    field_idx += 1;
                    self.align32();
                    for i in 0..len {
                        self.emit_bits(fields[field_idx + i], 8);
                    }
                    field_idx += len;
                    self.align32();
                }
            }
        }
    }

    #[allow(dead_code)]
    fn emit_array_element(&mut self, elem: &AbbrevOperand, value: u64) {
        match elem {
            AbbrevOperand::Fixed(w) => self.emit_bits(value, *w),
            AbbrevOperand::Vbr(w) => self.emit_vbr(value, *w),
            AbbrevOperand::Char6 => {
                self.emit_bits(u64::from(encode_char6(value as u8)), 6);
            }
            _ => {}
        }
    }

    pub fn align32(&mut self) {
        if self.cur_bit > 0 {
            self.buffer.push(self.cur_byte);
            self.cur_byte = 0;
            self.cur_bit = 0;
        }
        let rem = self.buffer.len() % 4;
        if rem != 0 {
            let pad = 4 - rem;
            self.buffer.extend(std::iter::repeat(0u8).take(pad));
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        if self.cur_bit > 0 {
            self.buffer.push(self.cur_byte);
        }
        self.buffer
    }

    pub(crate) fn patch_u32_bits(&mut self, bit_position: usize, value: u32) {
        self.patch_bits(bit_position, u64::from(value), 32);
    }

    fn patch_bits(&mut self, bit_position: usize, value: u64, width: u32) {
        for bit_offset in 0..width as usize {
            let absolute_bit = bit_position + bit_offset;
            let byte_index = absolute_bit / 8;
            let bit_index = (absolute_bit % 8) as u8;
            let mask = 1u8 << bit_index;
            let is_set = ((value >> bit_offset) & 1) != 0;

            if byte_index < self.buffer.len() {
                if is_set {
                    self.buffer[byte_index] |= mask;
                } else {
                    self.buffer[byte_index] &= !mask;
                }
                continue;
            }

            if byte_index == self.buffer.len() && u32::from(bit_index) < self.cur_bit {
                if is_set {
                    self.cur_byte |= mask;
                } else {
                    self.cur_byte &= !mask;
                }
                continue;
            }

            panic!("cannot patch future bit position {bit_position}");
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AbbrevDef {
    pub(crate) operands: Vec<AbbrevOperand>,
}

#[derive(Clone, Debug)]
pub(crate) enum AbbrevOperand {
    Literal(u64),
    Fixed(u32),
    Vbr(u32),
    Array(Box<AbbrevOperand>),
    Char6,
    Blob,
}

struct ReaderBlockScope {
    abbrevs: Vec<AbbrevDef>,
}

fn decode_char6(v: u8) -> u8 {
    match v {
        0..=25 => b'a' + v,
        26..=51 => b'A' + v - 26,
        52..=61 => b'0' + v - 52,
        62 => b'.',
        63 => b'_',
        _ => b'?',
    }
}

#[allow(dead_code)]
fn encode_char6(c: u8) -> u8 {
    match c {
        b'a'..=b'z' => c - b'a',
        b'A'..=b'Z' => c - b'A' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'.' => 62,
        b'_' => 63,
        _ => 0,
    }
}

pub struct BitstreamReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u32,
    block_scope_stack: Vec<ReaderBlockScope>,
    blockinfo_abbrevs: FxHashMap<u32, Vec<AbbrevDef>>,
}

impl<'a> BitstreamReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
            block_scope_stack: Vec::new(),
            blockinfo_abbrevs: FxHashMap::default(),
        }
    }

    pub fn read_bits(&mut self, width: u32) -> u64 {
        let mut result: u64 = 0;
        let mut remaining = width;
        let mut shift = 0u32;
        while remaining > 0 {
            let bits_avail = 8 - self.bit_pos;
            let to_read = remaining.min(bits_avail);
            let mask = if to_read == 8 {
                0xFF
            } else {
                ((1u16 << to_read) - 1) as u8
            };
            let bits = (self.data[self.byte_pos] >> self.bit_pos) & mask;
            result |= u64::from(bits) << shift;
            shift += to_read;
            self.bit_pos += to_read;
            remaining -= to_read;
            if self.bit_pos == 8 {
                self.byte_pos += 1;
                self.bit_pos = 0;
            }
        }
        result
    }

    pub fn read_vbr(&mut self, chunk_width: u32) -> u64 {
        let data_bits = chunk_width - 1;
        let data_mask = (1u64 << data_bits) - 1;
        let cont_bit = 1u64 << data_bits;
        let mut result: u64 = 0;
        let mut shift = 0u32;
        loop {
            let chunk = self.read_bits(chunk_width);
            result |= (chunk & data_mask) << shift;
            if chunk & cont_bit == 0 {
                break;
            }
            shift += data_bits;
        }
        result
    }

    pub fn read_abbrev_id(&mut self, abbrev_width: u32) -> u32 {
        self.read_bits(abbrev_width) as u32
    }

    pub fn enter_subblock(&mut self) -> (u32, u32, usize) {
        let block_id = self.read_vbr(8) as u32;
        let new_abbrev_width = self.read_vbr(4) as u32;
        self.align32();
        let block_len_words = self.read_bits(32) as usize;
        (block_id, new_abbrev_width, block_len_words)
    }

    pub fn skip_block(&mut self, block_len_words: usize) {
        self.byte_pos += block_len_words * 4;
        self.bit_pos = 0;
    }

    pub fn read_unabbrev_record(&mut self) -> (u32, Vec<u64>) {
        let code = self.read_vbr(6) as u32;
        let num_ops = self.read_vbr(6) as usize;
        let mut values = Vec::with_capacity(num_ops);
        for _ in 0..num_ops {
            values.push(self.read_vbr(6));
        }
        (code, values)
    }

    pub fn align32(&mut self) {
        if self.bit_pos > 0 {
            self.byte_pos += 1;
            self.bit_pos = 0;
        }
        let rem = self.byte_pos % 4;
        if rem != 0 {
            self.byte_pos += 4 - rem;
        }
    }

    pub fn at_end(&self) -> bool {
        self.byte_pos >= self.data.len()
    }

    pub fn byte_position(&self) -> usize {
        self.byte_pos
    }

    pub fn push_block_scope(&mut self, block_id: u32) {
        let mut scope = ReaderBlockScope {
            abbrevs: Vec::new(),
        };
        if let Some(inherited) = self.blockinfo_abbrevs.get(&block_id) {
            scope.abbrevs.clone_from(inherited);
        }
        self.block_scope_stack.push(scope);
    }

    pub fn pop_block_scope(&mut self) {
        self.block_scope_stack.pop();
    }

    pub fn read_define_abbrev(&mut self) -> Result<(), String> {
        let abbrev = self.read_abbrev_def()?;
        if let Some(scope) = self.block_scope_stack.last_mut() {
            scope.abbrevs.push(abbrev);
        }
        Ok(())
    }

    pub fn read_blockinfo_abbrev(&mut self, target_block_id: u32) -> Result<(), String> {
        let abbrev = self.read_abbrev_def()?;
        self.blockinfo_abbrevs
            .entry(target_block_id)
            .or_default()
            .push(abbrev);
        Ok(())
    }

    pub fn read_abbreviated_record(&mut self, abbrev_id: u32) -> Result<(u32, Vec<u64>), String> {
        let abbrev_index = (abbrev_id - 4) as usize;
        let abbrev = self
            .block_scope_stack
            .last()
            .and_then(|scope| scope.abbrevs.get(abbrev_index))
            .ok_or_else(|| format!("abbreviation {abbrev_id} not defined"))?
            .clone();

        if abbrev.operands.is_empty() {
            return Err("abbreviation has no operands".to_string());
        }

        let code = match &abbrev.operands[0] {
            AbbrevOperand::Literal(v) => *v as u32,
            AbbrevOperand::Fixed(w) => self.read_bits(*w) as u32,
            AbbrevOperand::Vbr(w) => self.read_vbr(*w) as u32,
            AbbrevOperand::Char6 => u32::from(decode_char6(self.read_bits(6) as u8)),
            AbbrevOperand::Array(_) | AbbrevOperand::Blob => {
                return Err("abbreviation starts with Array or Blob".to_string());
            }
        };

        let mut values = Vec::new();
        for op in &abbrev.operands[1..] {
            self.decode_abbrev_value(op, &mut values)?;
        }
        Ok((code, values))
    }

    fn read_abbrev_def(&mut self) -> Result<AbbrevDef, String> {
        let num_ops = self.read_vbr(5) as usize;
        let mut operands = Vec::with_capacity(num_ops);
        let mut i = 0;
        while i < num_ops {
            let is_literal = self.read_bits(1) != 0;
            if is_literal {
                operands.push(AbbrevOperand::Literal(self.read_vbr(8)));
            } else {
                let encoding = self.read_bits(3) as u8;
                match encoding {
                    1 => operands.push(AbbrevOperand::Fixed(self.read_vbr(5) as u32)),
                    2 => operands.push(AbbrevOperand::Vbr(self.read_vbr(5) as u32)),
                    3 => {
                        i += 1;
                        if i >= num_ops {
                            return Err("Array abbreviation missing element operand".to_string());
                        }
                        let elem = self.read_single_abbrev_operand()?;
                        operands.push(AbbrevOperand::Array(Box::new(elem)));
                    }
                    4 => operands.push(AbbrevOperand::Char6),
                    5 => operands.push(AbbrevOperand::Blob),
                    _ => return Err(format!("unknown abbreviation encoding {encoding}")),
                }
            }
            i += 1;
        }
        Ok(AbbrevDef { operands })
    }

    fn read_single_abbrev_operand(&mut self) -> Result<AbbrevOperand, String> {
        let is_literal = self.read_bits(1) != 0;
        if is_literal {
            Ok(AbbrevOperand::Literal(self.read_vbr(8)))
        } else {
            let encoding = self.read_bits(3) as u8;
            match encoding {
                1 => Ok(AbbrevOperand::Fixed(self.read_vbr(5) as u32)),
                2 => Ok(AbbrevOperand::Vbr(self.read_vbr(5) as u32)),
                4 => Ok(AbbrevOperand::Char6),
                5 => Ok(AbbrevOperand::Blob),
                _ => Err(format!(
                    "invalid element encoding {encoding} in array abbreviation"
                )),
            }
        }
    }

    fn decode_abbrev_value(
        &mut self,
        op: &AbbrevOperand,
        values: &mut Vec<u64>,
    ) -> Result<(), String> {
        match op {
            AbbrevOperand::Literal(v) => values.push(*v),
            AbbrevOperand::Fixed(w) => values.push(self.read_bits(*w)),
            AbbrevOperand::Vbr(w) => values.push(self.read_vbr(*w)),
            AbbrevOperand::Char6 => {
                values.push(u64::from(decode_char6(self.read_bits(6) as u8)));
            }
            AbbrevOperand::Array(elem) => {
                let len = self.read_vbr(6) as usize;
                for _ in 0..len {
                    self.decode_abbrev_value(elem, values)?;
                }
            }
            AbbrevOperand::Blob => {
                let len = self.read_vbr(6) as usize;
                self.align32();
                for _ in 0..len {
                    values.push(u64::from(self.data[self.byte_pos]));
                    self.byte_pos += 1;
                }
                self.align32();
            }
        }
        Ok(())
    }
}

pub struct BitChunkAccessor<const CHUNK_SIZE: usize> {
    pub threshold: u32,
    pub shift: u32,
    pub word_id: u32,
    pub mask: u64,
}

impl<const CHUNK_SIZE: usize> Default for BitChunkAccessor<CHUNK_SIZE> {
    /// # Panics
    /// When `CHUNK_SIZE` does not fit into u32
    fn default() -> Self {
        Self {
            threshold: (u64::BITS / u32::try_from(CHUNK_SIZE).unwrap() - 1)
                * u32::try_from(CHUNK_SIZE).unwrap(),
            shift: 0,
            word_id: 0,
            mask: u64::MAX >> (u64::BITS - u32::try_from(CHUNK_SIZE).unwrap()),
        }
    }
}

impl<const CHUNK_SIZE: usize> BitChunkAccessor<CHUNK_SIZE> {
    /// # Panics
    /// When `CHUNK_SIZE` does not fit into u32
    pub fn next(&mut self) {
        if self.shift == self.threshold {
            self.shift = 0;
            self.word_id += 1;
        } else {
            self.shift += u32::try_from(CHUNK_SIZE).unwrap();
        }
    }

    pub fn xor(&self, what: &mut [u64], val: u64) {
        what[self.word_id as usize] ^= val << self.shift;
    }

    #[must_use]
    pub fn get(&self, what: &[u64]) -> u64 {
        (what[self.word_id as usize] >> self.shift) & self.mask
    }
}

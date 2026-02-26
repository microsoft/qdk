// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::io::Error;
use std::io::ErrorKind::InvalidInput;
use std::str::FromStr;

use bytemuck::{Pod, Zeroable};

use crate::noise_config::{NoiseConfig, NoiseTable, encode_pauli, uq1_63};

/// A `NoiseTableEntry` describes the probability of the one possible pauli-noise string when working
/// with correlated Pauli noise.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct NoiseTableEntry {
    /// The correlated pauli string as bits (2 bits per qubit). If bit 0 is set, then it has bit-flip
    /// noise, and if bit 1 is set then it has phase-flip noise. e.g., `110001 == "YIX"`
    paulis: u64,
    /// The probability of the noise occurring in `Q1_63` format. This is a float format where the high
    /// order bit (bit 63) has the value 1.0 (`2^0 / 1`), bit 62 has the value 0.5 (`2^1 / 1`), etc.
    /// all the way to bit 63 with a value of approx 1.0842e-19 (`2^63 / 1`). This gives a range of
    /// values from [0..2) with equal spacing of 1.0842e-19 between values (unlike float or double),
    /// which makes it more suitable for random numbers used to select between a large number of small
    /// probability entries.
    probability: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct NoiseTableMetadata {
    /// The total probability of any noise (i.e. sum of all noise entries) in `Q1.63` format
    pub noise_probability: u64,
    /// The start offset of this table's entries in the global `NoiseTableEntry` array
    pub start_offset: u32,
    /// The number of entries in this noise table
    pub entry_count: u32,
}

#[derive(Debug, Default)]
pub struct NoiseTables {
    /// The names associated with this noise table. This should match the QIR intrinsic for the noise.
    /// This is only needed to create the name-to-id mapping when inserting the QIR intrinsic calls.
    /// It does not get uploaded to the GPU.
    pub names: Vec<String>,
    /// The metadata for each noise table entry. This will be uploaded to a GPU buffer.
    pub metadata: Vec<NoiseTableMetadata>,
    /// The table of pauli strings to probability mappings. This will be uploaded to a GPU buffer.
    pub entries: Vec<NoiseTableEntry>,
}

impl NoiseTables {
    /// Creates a new `NoiseTable` from string containing lines in a CSV format showing the pauli
    /// noise and probability for an entry. Lines starting with a `#` or the column headers will be
    /// ignored, e.g.
    ///
    /// ```csv
    /// # Correlated noise model for gadget_99
    /// #
    /// pauli_string,probability
    /// IIIIIX,3.4552708271433022e-06
    /// IIIIIZ,4.892742300968495e-06
    /// XXIIYY,8.136850287643285e-06
    /// ```
    pub fn add(&mut self, name: &str, contents: &str) {
        let start_offset = u32::try_from(self.entries.len()).expect("Too many noise entries");
        let mut entry_count: u32 = 0;
        let mut noise_probability: u64 = 0;

        self.names.push(name.to_string());

        for line in contents.lines() {
            if line.starts_with('#') || line.starts_with("pauli") || line.trim().is_empty() {
                continue;
            }
            let entry = parse_line(line).expect("Parsing failed");
            if entry.paulis == 0u64 || entry.probability == 0u64 {
                // Don't add identity Paulis or 0 probability (no-noise) entries if present.
                continue;
            }
            noise_probability += entry.probability;
            assert!(
                noise_probability <= uq1_63::ONE,
                "Cumulative probability is larger than 1.0 after processing line: {line}"
            );
            // Add the entry to the list with cumulative probability, not the value for the entry
            self.entries.push(NoiseTableEntry {
                paulis: entry.paulis,
                probability: noise_probability,
            });
            entry_count += 1;
        }
        self.metadata.push(NoiseTableMetadata {
            noise_probability,
            start_offset,
            entry_count,
        });
    }

    /// Loads the correlated noise tables from a [`NoiseConfig`].
    ///
    /// If we are following this codepath, the user handled loading the csv files and
    /// the name-to-id mapping when inserting the QIR intrinsic calls
    /// was already done. Therefore, we don't need to push to the `names` field.
    pub fn load_from_noise_config(&mut self, noise_config: &NoiseConfig<f32, f64>) {
        let mut intrinsics_ref: Vec<(u32, &NoiseTable<f64>)> = noise_config
            .intrinsics
            .iter()
            .map(|(id, table)| (*id, table))
            .collect::<Vec<_>>();

        // Sort intrinsics by id.
        // The NoiseConfig API guarantees that the ids will be non-skiping numbers starting from zero.
        intrinsics_ref.sort_by(|a, b| a.0.cmp(&b.0));

        for (_, noise_table) in intrinsics_ref {
            self.load_from_noise_table(noise_table);
        }
    }

    fn load_from_noise_table(&mut self, noise_table: &NoiseTable<f64>) {
        let start_offset = u32::try_from(self.entries.len()).expect("Too many noise entries");
        let mut entry_count: u32 = 0;
        let mut noise_probability: u64 = 0;

        for (paulis, prob) in noise_table
            .pauli_strings
            .iter()
            .zip(&noise_table.probabilities)
        {
            let entry = NoiseTableEntry {
                paulis: *paulis,
                probability: uq1_63::from_prob(*prob),
            };
            if entry.paulis == 0u64 || entry.probability == 0u64 {
                // Don't add identity Paulis or 0 probability (no-noise) entries if present.
                continue;
            }
            noise_probability += entry.probability;
            assert!(
                noise_probability <= uq1_63::ONE,
                "Cumulative probability is larger than 1.0"
            );
            // Add the entry to the list with cumulative probability, not the value for the entry
            self.entries.push(NoiseTableEntry {
                paulis: entry.paulis,
                probability: noise_probability,
            });
            entry_count += 1;
        }

        self.metadata.push(NoiseTableMetadata {
            noise_probability,
            start_offset,
            entry_count,
        });
    }
}

fn parse_line(line: &str) -> Result<NoiseTableEntry, Error> {
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() != 2 {
        return Err(Error::new(InvalidInput, line));
    }
    let prob = f64::from_str(parts[1]);
    match prob {
        Ok(p) => parse_noise_table_entry(parts[0], p),
        Err(e) => {
            eprintln!("Invalid float on line {line}, error: {e}");
            Err(Error::new(InvalidInput, line))
        }
    }
}

fn parse_noise_table_entry(paulis: &str, probability: f64) -> Result<NoiseTableEntry, Error> {
    Ok(NoiseTableEntry {
        paulis: encode_pauli(paulis),
        probability: uq1_63::from_prob(probability),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_test() {
        let result = parse_line("IIIIIX,1.25e-1");
        assert!(result.is_ok());
        let entry = result.expect("Entry should be valid");
        assert_eq!(entry.paulis, 0x01);
        assert_eq!(entry.probability, 0x1000_0000_0000_0000);
    }

    #[test]
    fn test_cumulated_probabilities() {
        let mut tables = NoiseTables::default();
        // Each entry is 0.125 probability
        let contents = r"
pauli_string,probability
IIIIIX,0.125
IIIIZZ,0.125
IIIIYY,0.125
";
        tables.add("test_gate", contents);

        assert_eq!(tables.entries.len(), 3);
        // Each 0.125 is 0x1000_0000_0000_0000 in Q1.63
        assert_eq!(tables.entries[0].probability, 0x1000_0000_0000_0000);
        assert_eq!(tables.entries[1].probability, 0x2000_0000_0000_0000);
        assert_eq!(tables.entries[2].probability, 0x3000_0000_0000_0000);

        assert_eq!(tables.metadata.len(), 1);
        assert_eq!(tables.metadata[0].noise_probability, 0x3000_0000_0000_0000);
        assert_eq!(tables.metadata[0].start_offset, 0);
        assert_eq!(tables.metadata[0].entry_count, 3);
    }

    #[test]
    fn test_identity_and_zero_probability_ignored() {
        let mut tables = NoiseTables::default();
        let contents = r"
pauli_string,probability
IIIIII,0.125
IIIIIX,0.125
IIIIZZ,0.0
IIIIYY,0.125
    ";
        tables.add("test_gate", contents);

        // Identity pauli "IIIIII" and zero probability "IIIIZZ" should be ignored
        assert_eq!(tables.entries.len(), 2);
        assert_eq!(tables.entries[0].paulis, encode_pauli("IIIIIX"));
        assert_eq!(tables.entries[1].paulis, encode_pauli("IIIIYY"));

        assert_eq!(tables.metadata[0].entry_count, 2);
        // Total probability should be 0.25 (0.125 + 0.125)
        assert_eq!(tables.metadata[0].noise_probability, 0x2000_0000_0000_0000);
    }

    #[test]
    #[should_panic(expected = "Cumulative probability is larger than 1.0")]
    fn test_cumulative_probability_exceeds_one() {
        let mut tables = NoiseTables::default();
        let contents = r"
pauli_string,probability
IIIIIX,0.5
IIIIZZ,0.4
IIIIYY,0.3
    ";
        // This should panic because 0.5 + 0.4 + 0.3 = 1.2 > 1.0
        tables.add("test_gate", contents);
    }
}

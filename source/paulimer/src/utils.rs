use crate::{BitStringView, BitWord, BITS_PER_WORD};

pub const SUB_CHARS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];

#[must_use]
pub fn one_indicies(bitstring: &BitStringView, word_offset: usize, words_range_length: usize) -> Vec<usize> {
    let mut res = Vec::<usize>::with_capacity(bitstring.len() * BITS_PER_WORD);
    for word_idx in 0..words_range_length {
        let mut word: BitWord = bitstring[word_offset + word_idx];
        let bit_idx_offset = word_idx * BITS_PER_WORD;
        for bit_idx in 0..BITS_PER_WORD {
            if word & 1 == 1 {
                res.push(bit_idx + bit_idx_offset);
            }
            word >>= 1;
        }
    }
    res
}

#[must_use]
pub fn print_as_table(string_pairs: &Vec<(String, String)>, separator: &str) -> String {
    let mut str_res = String::new();
    let mut left_max_width = 0;
    let mut right_max_width = 0;
    for (left, right) in string_pairs {
        left_max_width = left_max_width.max(left.chars().count());
        right_max_width = right_max_width.max(right.chars().count());
    }
    for (left, right) in string_pairs {
        str_res.push_str(&format!(
            "{left:<left_max_width$}{separator}{right:<right_max_width$}\n"
        ));
    }
    str_res.pop();
    str_res
}

#[must_use]
pub fn subscript_digits(number: usize) -> String {
    let mut res = String::new();
    for char in number.to_string().chars() {
        let digit = char.to_digit(10).unwrap_or_default() as usize;
        res.push(SUB_CHARS[digit]);
    }
    res
}

/// .
///
/// # Panics
///
/// Panics if .
#[must_use]
pub fn phase_to_string(phase: u32) -> String {
    let s = match phase {
        0 => "",
        1 => "𝑖",
        2 => "-",
        3 => "-𝑖",
        _ => panic!("Unexpected phase"),
    };
    String::from(s)
}


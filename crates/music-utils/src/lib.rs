#![feature(int_roundings)]

use std::ops::Range;

use num_integer::Integer as _;

/// Mask for black keys in an octave.
#[allow(clippy::unusual_byte_groupings)]
const BLACK_KEY_MASK: u16 = 0b0101010_01010;

/// Index of key among keys of the same color (white or black) in an octave.
pub const KEY_IDX_OF_COLOR: [u8; 12] = [0, 0, 1, 1, 2, 3, 2, 4, 3, 5, 4, 6];

pub const WHITE_TONES: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
pub const BLACK_TONES: [u8; 5] = [1, 3, 6, 8, 10];

/// White key index on the left side of each black key.
pub const BLACK_IDX_TO_PREV_WHITE_IDX: [u8; 5] = [0, 1, 3, 4, 5];
pub const WHITE_IDX_TO_NEXT_BLACK_IDX: [u8; 7] = [0, 1, 2, 2, 3, 4, 5];

/// Judges whether a given tone in octave (between 0 and 11) is a black key or not.
#[inline(always)]
pub const fn is_black_key_otone(otone: u8) -> bool {
    BLACK_KEY_MASK & (1 << otone) != 0
}

/// Judges whether a given tone is a black key or not.
#[inline(always)]
pub fn is_black_key(key: i8) -> bool {
    is_black_key_otone(key.mod_floor(&12) as u8)
}

/// Index of key among keys of the same color (white or black) in an octave.
#[inline(always)]
pub const fn key_idx_of_color(otone: u8) -> u8 {
    KEY_IDX_OF_COLOR[otone as usize]
}

/// White key index on the left side of the black key.
#[inline(always)]
pub const fn black_idx_to_prev_white_idx(black_idx: u8) -> u8 {
    BLACK_IDX_TO_PREV_WHITE_IDX[black_idx as usize]
}

#[inline(always)]
pub const fn white_idx_to_next_black_idx(white_idx: u8) -> u8 {
    WHITE_IDX_TO_NEXT_BLACK_IDX[white_idx as usize]
}

/// Index range of complete octaves in the key range
pub const fn octave_range(key_range: &Range<i8>) -> Range<i8> {
    let &Range {
        start: k_start,
        end: k_end,
    } = key_range;

    // Index of the first complete octave in the key range
    let o_start = if k_start % 12 == 0 {
        k_start.div_floor(12)
    } else {
        (k_start.div_floor(12)) + 1
    };
    // Index of the last complete octave in the key range (not inclusive)
    let o_end = k_end.div_floor(12);

    o_start..o_end
}

#[inline(always)]
pub const fn white_otone(white_idx: u8) -> u8 {
    WHITE_TONES[white_idx as usize]
}

#[inline(always)]
pub const fn black_otone(black_idx: u8) -> u8 {
    BLACK_TONES[black_idx as usize]
}

#[inline(always)]
pub const fn white_tone(octave: i8, white_idx: u8) -> i8 {
    octave * 12 + white_otone(white_idx) as i8
}

#[inline(always)]
pub const fn black_tone(octave: i8, black_idx: u8) -> i8 {
    octave * 12 + black_otone(black_idx) as i8
}

pub struct KeyInfo {
    pub octave: i8,
    pub is_black: bool,
    pub idx_of_color: u8,
}

#[inline]
pub const fn key_info(key: i8) -> KeyInfo {
    let octave = key.div_floor(12);
    let otone = (key - octave * 12) as u8;
    KeyInfo {
        octave,
        is_black: is_black_key_otone(otone),
        idx_of_color: key_idx_of_color(otone),
    }
}

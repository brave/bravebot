// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! From bytes to codewords, and from codewords to the modules of the square.
//!
//! Each constant below is a row of the tables in ISO/IEC 18004 for error correction level M.

use crate::galois;

/// For versions 1 to 10: the number of blocks that hold `data` codewords, that number, and the
/// number of longer blocks, which hold one more.
const BLOCKS: [(usize, usize, usize); 10] = [
    (1, 16, 0),
    (1, 28, 0),
    (1, 44, 0),
    (2, 32, 0),
    (2, 43, 0),
    (4, 27, 0),
    (4, 31, 0),
    (2, 38, 2),
    (3, 36, 2),
    (4, 43, 1),
];

/// Error correction codewords in each block, for versions 1 to 10.
const ERROR_CODEWORDS: [usize; 10] = [10, 16, 26, 18, 24, 16, 18, 22, 22, 26];

/// The centres of the alignment patterns along each side, for versions 1 to 10.
const ALIGNMENT: [&[usize]; 10] = [
    &[],
    &[6, 18],
    &[6, 22],
    &[6, 26],
    &[6, 30],
    &[6, 34],
    &[6, 22, 38],
    &[6, 24, 42],
    &[6, 26, 46],
    &[6, 28, 50],
];

/// The mark of error correction level M in the format information.
const LEVEL_M: u32 = 0;

fn row(version: u8) -> usize {
    usize::from(version.clamp(1, 10)) - 1
}

/// Data codewords a version holds.
pub(crate) fn data_codewords(version: u8) -> usize {
    let (short, data, long) = BLOCKS[row(version)];
    short * data + long * (data + 1)
}

/// Bits that give the count of bytes: 8, or 16 from version 10.
fn count_bits(version: u8) -> usize {
    if version <= 9 { 8 } else { 16 }
}

/// The smallest version in which `length` bytes of data fit.
pub(crate) fn smallest_version(length: usize) -> Option<u8> {
    (1..=10).find(|&version| 4 + count_bits(version) + 8 * length <= 8 * data_codewords(version))
}

fn push_bits(bits: &mut Vec<bool>, value: u32, count: usize) {
    for at in (0..count).rev() {
        bits.push((value >> at) & 1 != 0);
    }
}

/// The data and error correction codewords of `data` in the order they are placed in the square.
pub(crate) fn codewords(data: &[u8], version: u8) -> Vec<u8> {
    let capacity = data_codewords(version);
    let mut bits = Vec::with_capacity(capacity * 8);
    // Byte mode, the count, the bytes, then the end mark and padding to a whole codeword.
    push_bits(&mut bits, 0b0100, 4);
    push_bits(
        &mut bits,
        u32::try_from(data.len()).unwrap_or(0),
        count_bits(version),
    );
    for &byte in data {
        push_bits(&mut bits, u32::from(byte), 8);
    }
    let end = (capacity * 8 - bits.len()).min(4);
    push_bits(&mut bits, 0, end);
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut bytes: Vec<u8> = bits
        .chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0u8, |byte, &bit| (byte << 1) | u8::from(bit))
        })
        .collect();
    let mut pad = 0xEC;
    while bytes.len() < capacity {
        bytes.push(pad);
        pad ^= 0xEC ^ 0x11;
    }

    let (short, short_data, long) = BLOCKS[row(version)];
    let errors = ERROR_CODEWORDS[row(version)];
    let mut blocks = Vec::new();
    let mut taken = 0;
    for block in 0..short + long {
        let length = short_data + usize::from(block >= short);
        let data = bytes[taken..taken + length].to_vec();
        taken += length;
        let correction = galois::remainder(&data, errors);
        blocks.push((data, correction));
    }
    // One codeword of each block in turn, then the error correction the same way.
    let mut out = Vec::with_capacity(capacity + errors * blocks.len());
    for at in 0..=short_data {
        out.extend(blocks.iter().filter_map(|(data, _)| data.get(at)));
    }
    for at in 0..errors {
        out.extend(
            blocks
                .iter()
                .filter_map(|(_, correction)| correction.get(at)),
        );
    }
    out
}

/// The modules of the square, and which of them are fixed patterns that data does not use.
struct Grid {
    size: usize,
    dark: Vec<bool>,
    fixed: Vec<bool>,
}

impl Grid {
    fn new(size: usize) -> Self {
        Self {
            size,
            dark: vec![false; size * size],
            fixed: vec![false; size * size],
        }
    }

    fn at(&self, x: usize, y: usize) -> usize {
        y * self.size + x
    }

    /// Set a fixed module. A place outside the square is left alone.
    fn fix(&mut self, x: isize, y: isize, dark: bool) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        if x < self.size && y < self.size {
            let at = self.at(x, y);
            self.dark[at] = dark;
            self.fixed[at] = true;
        }
    }

    fn finder(&mut self, x: isize, y: isize) {
        for dy in -4isize..=4 {
            for dx in -4isize..=4 {
                let distance = dx.abs().max(dy.abs());
                self.fix(x + dx, y + dy, distance != 2 && distance != 4);
            }
        }
    }

    fn alignment(&mut self, x: isize, y: isize) {
        for dy in -2isize..=2 {
            for dx in -2isize..=2 {
                self.fix(x + dx, y + dy, dx.abs().max(dy.abs()) != 1);
            }
        }
    }

    /// The format information for `mask`, twice, and the one module that is always dark.
    fn format(&mut self, mask: u32) {
        let data = (LEVEL_M << 3) | mask;
        let mut remainder = data;
        for _ in 0..10 {
            remainder = (remainder << 1) ^ ((remainder >> 9) * 0x537);
        }
        let bits = ((data << 10) | remainder) ^ 0x5412;
        let size = isize::try_from(self.size).unwrap_or(0);
        for at in 0..15u32 {
            let dark = (bits >> at) & 1 != 0;
            let i = isize::try_from(at).unwrap_or(0);
            let first = match i {
                0..=5 => (8, i),
                6 => (8, 7),
                7 => (8, 8),
                8 => (7, 8),
                _ => (14 - i, 8),
            };
            let second = if i < 8 {
                (size - 1 - i, 8)
            } else {
                (8, size - 15 + i)
            };
            self.fix(first.0, first.1, dark);
            self.fix(second.0, second.1, dark);
        }
        // The module that is always dark.
        self.fix(8, size - 8, true);
    }

    /// The version information, twice, for versions 7 and up.
    fn version(&mut self, version: u8) {
        if version < 7 {
            return;
        }
        let mut remainder = u32::from(version);
        for _ in 0..12 {
            remainder = (remainder << 1) ^ ((remainder >> 11) * 0x1F25);
        }
        let bits = (u32::from(version) << 12) | remainder;
        let size = isize::try_from(self.size).unwrap_or(0);
        for at in 0..18u32 {
            let dark = (bits >> at) & 1 != 0;
            let near = size - 11 + isize::try_from(at % 3).unwrap_or(0);
            let far = isize::try_from(at / 3).unwrap_or(0);
            self.fix(near, far, dark);
            self.fix(far, near, dark);
        }
    }

    /// The patterns every code has, and the places the format and version information go.
    fn patterns(&mut self, version: u8) {
        let size = isize::try_from(self.size).unwrap_or(0);
        for at in 0..size {
            self.fix(6, at, at % 2 == 0);
            self.fix(at, 6, at % 2 == 0);
        }
        self.finder(3, 3);
        self.finder(size - 4, 3);
        self.finder(3, size - 4);
        let centres = ALIGNMENT[row(version)];
        let last = centres.len().saturating_sub(1);
        for (first, &x) in centres.iter().enumerate() {
            for (second, &y) in centres.iter().enumerate() {
                // The three corners where a finder is.
                if (first == 0 && second == 0)
                    || (first == 0 && second == last)
                    || (first == last && second == 0)
                {
                    continue;
                }
                self.alignment(
                    isize::try_from(x).unwrap_or(0),
                    isize::try_from(y).unwrap_or(0),
                );
            }
        }
        self.format(0);
        self.version(version);
    }

    /// Place the codeword bits in the zigzag the standard describes, up and down in pairs of
    /// columns from the right, skipping the column of the vertical timing pattern.
    fn place(&mut self, codewords: &[u8]) {
        let mut bit = 0;
        let mut right = self.size - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for step in 0..self.size {
                for column in 0..2 {
                    let x = right - column;
                    let upward = ((right + 1) & 2) == 0;
                    let y = if upward { self.size - 1 - step } else { step };
                    let at = self.at(x, y);
                    if !self.fixed[at] && bit < codewords.len() * 8 {
                        self.dark[at] = (codewords[bit / 8] >> (7 - bit % 8)) & 1 != 0;
                        bit += 1;
                    }
                }
            }
            // The last pair of columns ends at the left edge: 1 less 2 stops the loop at 0.
            right = right.saturating_sub(2);
        }
    }

    /// Flip each data module the mask names. Flipping twice restores the square.
    fn mask(&mut self, mask: u32) {
        for y in 0..self.size {
            for x in 0..self.size {
                let at = self.at(x, y);
                let (column, row) = (x, y);
                let flips = match mask {
                    0 => (column + row) % 2 == 0,
                    1 => row % 2 == 0,
                    2 => column % 3 == 0,
                    3 => (column + row) % 3 == 0,
                    4 => (column / 3 + row / 2) % 2 == 0,
                    5 => column * row % 2 + column * row % 3 == 0,
                    6 => (column * row % 2 + column * row % 3) % 2 == 0,
                    _ => ((column + row) % 2 + column * row % 3) % 2 == 0,
                };
                self.dark[at] ^= flips && !self.fixed[at];
            }
        }
    }

    fn get(&self, x: usize, y: usize) -> bool {
        self.dark[self.at(x, y)]
    }

    /// How hard the square is to read, by the four rules of the standard: runs of one colour, blocks
    /// of one colour, patterns that look like a finder, and how far from half the modules are dark.
    fn penalty(&self) -> usize {
        let size = self.size;
        let mut score = 0;
        for line in 0..size {
            let across: Vec<bool> = (0..size).map(|x| self.get(x, line)).collect();
            let down: Vec<bool> = (0..size).map(|y| self.get(line, y)).collect();
            for modules in [&across, &down] {
                score += runs(modules) + finder_like(modules);
            }
        }
        for y in 0..size - 1 {
            for x in 0..size - 1 {
                let colour = self.get(x, y);
                if colour == self.get(x + 1, y)
                    && colour == self.get(x, y + 1)
                    && colour == self.get(x + 1, y + 1)
                {
                    score += 3;
                }
            }
        }
        let dark = self.dark.iter().filter(|&&dark| dark).count();
        let total = size * size;
        let steps = (dark.abs_diff(total - dark) * 10)
            .div_ceil(total)
            .saturating_sub(1);
        score + steps * 10
    }
}

/// The points for each run of five or more modules of one colour in a line.
fn runs(line: &[bool]) -> usize {
    let mut score = 0;
    let mut length = 0;
    let mut previous = None;
    for &module in line {
        if previous == Some(module) {
            length += 1;
        } else {
            length = 1;
            previous = Some(module);
        }
        if length == 5 {
            score += 3;
        } else if length > 5 {
            score += 1;
        }
    }
    score
}

/// Forty points for each place in a line that reads dark, light, three dark, light, dark with four
/// light modules on one side.
fn finder_like(line: &[bool]) -> usize {
    const BEFORE: [bool; 11] = [
        false, false, false, false, true, false, true, true, true, false, true,
    ];
    const AFTER: [bool; 11] = [
        true, false, true, true, true, false, true, false, false, false, false,
    ];
    line.windows(11)
        .filter(|window| *window == BEFORE || *window == AFTER)
        .count()
        * 40
}

/// The size of the square and its modules for `codewords` in `version`, under the mask that is
/// easiest to read.
pub(crate) fn draw(version: u8, codewords: &[u8]) -> (usize, Vec<bool>) {
    let size = 17 + 4 * usize::from(version);
    let mut grid = Grid::new(size);
    grid.patterns(version);
    grid.place(codewords);
    let mut best = (usize::MAX, 0);
    for mask in 0..8 {
        grid.mask(mask);
        grid.format(mask);
        best = best.min((grid.penalty(), mask));
        grid.mask(mask);
    }
    grid.mask(best.1);
    grid.format(best.1);
    (size, grid.dark)
}

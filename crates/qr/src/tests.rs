// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The encoder is held to the standard's own numbers, and to a reader written here from the
//! standard's description of a square and not from the encoder's code: it finds the fixed patterns
//! by their positions, reads the codewords in the zigzag, undoes the mask and the interleaving,
//! checks every block's error correction, and parses the bytes back.

use super::*;
use crate::galois;

/// The size of the square, the codewords in all, and then for each block of level M: data
/// codewords, over the blocks that hold that many, from ISO/IEC 18004 table 9.
struct Row {
    total: usize,
    errors: usize,
    blocks: &'static [(usize, usize)],
}

const ROWS: [Row; 10] = [
    Row {
        total: 26,
        errors: 10,
        blocks: &[(1, 16)],
    },
    Row {
        total: 44,
        errors: 16,
        blocks: &[(1, 28)],
    },
    Row {
        total: 70,
        errors: 26,
        blocks: &[(1, 44)],
    },
    Row {
        total: 100,
        errors: 18,
        blocks: &[(2, 32)],
    },
    Row {
        total: 134,
        errors: 24,
        blocks: &[(2, 43)],
    },
    Row {
        total: 172,
        errors: 16,
        blocks: &[(4, 27)],
    },
    Row {
        total: 196,
        errors: 18,
        blocks: &[(4, 31)],
    },
    Row {
        total: 242,
        errors: 22,
        blocks: &[(2, 38), (2, 39)],
    },
    Row {
        total: 292,
        errors: 22,
        blocks: &[(3, 36), (2, 37)],
    },
    Row {
        total: 346,
        errors: 26,
        blocks: &[(4, 43), (1, 44)],
    },
];

/// The format information of level M for masks 0 to 7, from the standard's table.
const FORMAT_M: [u32; 8] = [
    0x5412, 0x5125, 0x5E7C, 0x5B4B, 0x45F9, 0x40CE, 0x4F97, 0x4AA0,
];

/// The version information for versions 7 to 10, from the standard's table.
const VERSION_INFO: [(u8, u32); 4] = [(7, 0x07C94), (8, 0x085BC), (9, 0x09A99), (10, 0x0A4D3)];

fn alignment_centres(version: u8) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let version = usize::from(version);
    let count = version / 7 + 2;
    let size = 17 + 4 * version;
    let step = (version * 4 + 4).div_ceil(count * 2 - 2) * 2;
    let mut centres = vec![6];
    centres.extend((0..count - 1).rev().map(|back| size - 7 - back * step));
    centres
}

/// Whether the module at `x`, `y` is part of a pattern and not data, found from where the standard
/// puts the patterns.
fn is_fixed(version: u8, x: usize, y: usize) -> bool {
    let size = 17 + 4 * usize::from(version);
    let finder = (x < 8 && y < 8) || (x >= size - 8 && y < 8) || (x < 8 && y >= size - 8);
    let format = (x == 8 && (y <= 8 || y >= size - 8)) || (y == 8 && (x <= 8 || x >= size - 8));
    let timing = x == 6 || y == 6;
    let information = version >= 7
        && ((x >= size - 11 && x < size - 8 && y < 6) || (y >= size - 11 && y < size - 8 && x < 6));
    let centres = alignment_centres(version);
    let last = centres.len().saturating_sub(1);
    let alignment = centres.iter().enumerate().any(|(first, &cx)| {
        centres.iter().enumerate().any(|(second, &cy)| {
            let on_a_finder = (first == 0 && second == 0)
                || (first == 0 && second == last)
                || (first == last && second == 0);
            !on_a_finder && x.abs_diff(cx) <= 2 && y.abs_diff(cy) <= 2
        })
    });
    finder || format || timing || information || alignment
}

fn mask_flips(mask: u32, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y).is_multiple_of(2),
        1 => y.is_multiple_of(2),
        2 => x.is_multiple_of(3),
        3 => (x + y).is_multiple_of(3),
        4 => (y / 2 + x / 3).is_multiple_of(2),
        5 => (x * y) % 2 + (x * y) % 3 == 0,
        6 => ((x * y) % 2 + (x * y) % 3).is_multiple_of(2),
        _ => ((x + y) % 2 + (x * y) % 3).is_multiple_of(2),
    }
}

/// The 15 format bits as the first copy, around the top left finder, writes them.
fn first_format(code: &Code) -> u32 {
    let mut places = vec![
        (8, 0),
        (8, 1),
        (8, 2),
        (8, 3),
        (8, 4),
        (8, 5),
        (8, 7),
        (8, 8),
        (7, 8),
    ];
    places.extend((9..15).map(|at| (14 - at, 8)));
    places.iter().enumerate().fold(0, |bits, (at, &(x, y))| {
        bits | (u32::from(code.is_dark(x, y)) << at)
    })
}

/// The 15 format bits as the second copy, split between the other two finders, writes them.
fn second_format(code: &Code) -> u32 {
    let size = code.size();
    let mut bits = 0;
    for at in 0..8 {
        bits |= u32::from(code.is_dark(size - 1 - at, 8)) << at;
    }
    for at in 8..15 {
        bits |= u32::from(code.is_dark(8, size - 15 + at)) << at;
    }
    bits
}

/// The bytes a code holds, or why it is not a code this reader accepts.
fn read(code: &Code) -> Result<Vec<u8>, String> {
    let version = code.version();
    let size = code.size();
    if size != 17 + 4 * usize::from(version) {
        return Err("the size is not the version's".into());
    }
    let (first, second) = (first_format(code), second_format(code));
    if first != second {
        return Err("the two copies of the format information differ".into());
    }
    let mask = FORMAT_M
        .iter()
        .position(|&known| known == first)
        .ok_or("the format information is not level M with a mask")?;
    let mask = u32::try_from(mask).map_err(|_| "mask")?;

    let row = &ROWS[usize::from(version) - 1];
    let mut bits = Vec::new();
    let mut right = size - 1;
    while right >= 1 {
        if right == 6 {
            right = 5;
        }
        for step in 0..size {
            for column in 0..2 {
                let x = right - column;
                let y = if ((right + 1) & 2) == 0 {
                    size - 1 - step
                } else {
                    step
                };
                if !is_fixed(version, x, y) {
                    bits.push(code.is_dark(x, y) ^ mask_flips(mask, x, y));
                }
            }
        }
        right = right.saturating_sub(2);
    }
    let codewords: Vec<u8> = bits
        .as_chunks::<8>()
        .0
        .iter()
        .take(row.total)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0u8, |byte, &bit| (byte << 1) | u8::from(bit))
        })
        .collect();
    if codewords.len() != row.total {
        return Err("fewer codewords than the version holds".into());
    }

    // Undo the interleaving: the blocks' lengths, shortest first, then one codeword of each in turn.
    let lengths: Vec<usize> = row
        .blocks
        .iter()
        .flat_map(|&(count, length)| std::iter::repeat_n(length, count))
        .collect();
    let longest = *lengths.iter().max().ok_or("no blocks")?;
    let mut data: Vec<Vec<u8>> = vec![Vec::new(); lengths.len()];
    let mut at = 0;
    for position in 0..longest {
        for (block, &length) in lengths.iter().enumerate() {
            if position < length {
                data[block].push(codewords[at]);
                at += 1;
            }
        }
    }
    let mut errors: Vec<Vec<u8>> = vec![Vec::new(); lengths.len()];
    for _ in 0..row.errors {
        for block in &mut errors {
            block.push(codewords[at]);
            at += 1;
        }
    }
    for (block, (data, errors)) in data.iter().zip(&errors).enumerate() {
        if galois::remainder(data, row.errors) != *errors {
            return Err(format!("block {block} fails its error correction"));
        }
    }

    // The bytes of the segment: byte mode, a count, the bytes.
    let stream: Vec<bool> = data
        .concat()
        .iter()
        .flat_map(|&byte| (0..8).rev().map(move |at| (byte >> at) & 1 != 0))
        .collect();
    let value = |from: usize, count: usize| {
        stream[from..from + count]
            .iter()
            .fold(0usize, |value, &bit| (value << 1) | usize::from(bit))
    };
    if value(0, 4) != 0b0100 {
        return Err("not byte mode".into());
    }
    let count_bits = if version <= 9 { 8 } else { 16 };
    let length = value(4, count_bits);
    let start = 4 + count_bits;
    Ok((0..length)
        .map(|at| u8::try_from(value(start + 8 * at, 8)).unwrap_or(0))
        .collect())
}

/// The two modules, upper and lower, that a character of the drawing stands for.
fn pair_of(character: char) -> Option<(bool, bool)> {
    match character {
        '\u{2588}' => Some((true, true)),
        '\u{2580}' => Some((true, false)),
        '\u{2584}' => Some((false, true)),
        ' ' => Some((false, false)),
        _ => None,
    }
}

/// Bytes that vary, so a code of each length holds something a swapped or dropped byte would change.
fn sample(length: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9u32 ^ u32::try_from(length).unwrap_or(0);
    (0..length)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            u8::try_from(state >> 24).unwrap_or(0)
        })
        .collect()
}

/// The standard's own example (ISO/IEC 18004 annex I, `01234567` at version 1, level M): the data
/// codewords and the ten error correction codewords it prints.
#[test]
fn the_error_correction_of_the_standards_example_is_the_one_it_prints() {
    let data = [
        0x10, 0x20, 0x0C, 0x56, 0x61, 0x80, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC,
        0x11,
    ];
    assert_eq!(
        galois::remainder(&data, 10),
        [0xA5, 0x24, 0xD4, 0xC1, 0xED, 0x36, 0xC7, 0x87, 0x2C, 0x55]
    );
}

#[test]
fn a_product_in_the_field_is_what_the_polynomial_arithmetic_gives() {
    assert_eq!(galois::multiply(0, 0xFF), 0);
    assert_eq!(galois::multiply(1, 0xAB), 0xAB);
    // 2 * 0x80 overflows the byte and reduces by 0x11D.
    assert_eq!(galois::multiply(2, 0x80), 0x1D);
    assert_eq!(galois::multiply(0x57, 0x83), galois::multiply(0x83, 0x57));
}

/// The bytes a code holds are the bytes it was made from, at every length up to the limit, in each
/// of the ten versions.
#[test]
fn every_length_up_to_the_limit_reads_back_as_it_was_made() {
    let mut versions = std::collections::BTreeSet::new();
    for length in 0..=MAX_BYTES {
        let data = sample(length);
        let code = Code::encode(&data).unwrap();
        versions.insert(code.version());
        assert_eq!(read(&code), Ok(data), "length {length}");
    }
    assert_eq!(versions.len(), 10);
}

/// The smallest version that holds the text is the one used, at the edge of each.
#[test]
fn the_smallest_version_that_holds_the_text_is_used_and_more_than_the_limit_is_refused() {
    let edges = [
        (14, 1),
        (15, 2),
        (26, 2),
        (27, 3),
        (42, 3),
        (43, 4),
        (62, 4),
        (63, 5),
        (84, 5),
        (85, 6),
        (106, 6),
        (107, 7),
        (122, 7),
        (123, 8),
        (152, 8),
        (153, 9),
        (180, 9),
        (181, 10),
        (213, 10),
    ];
    for (length, version) in edges {
        assert_eq!(
            Code::encode(&sample(length)).unwrap().version(),
            version,
            "{length} bytes"
        );
    }
    assert_eq!(Code::encode(&sample(214)), Err(TooLong));
    assert_eq!(MAX_BYTES, 213);
}

/// Both copies of the format information are one of the standard's eight strings for level M, and
/// the version information of the larger codes is the standard's.
#[test]
fn the_format_and_version_information_are_the_standards() {
    for length in [5, 40, 90, 150, 200] {
        let code = Code::encode(&sample(length)).unwrap();
        assert!(FORMAT_M.contains(&first_format(&code)), "length {length}");
        assert_eq!(first_format(&code), second_format(&code), "length {length}");
    }
    for (version, expected) in VERSION_INFO {
        let length = ROWS[usize::from(version) - 1]
            .blocks
            .iter()
            .fold(0, |sum, &(c, l)| sum + c * l)
            - 4;
        let code = Code::encode(&sample(length)).unwrap();
        assert_eq!(code.version(), version);
        let size = code.size();
        let mut near = 0;
        let mut far = 0;
        for at in 0..18usize {
            near |= u32::from(code.is_dark(size - 11 + at % 3, at / 3)) << at;
            far |= u32::from(code.is_dark(at / 3, size - 11 + at % 3)) << at;
        }
        assert_eq!(near, expected, "version {version}");
        assert_eq!(far, expected, "version {version}");
    }
}

/// The patterns that every code has are where the standard puts them.
#[test]
fn the_finders_the_timing_and_the_dark_module_are_where_the_standard_puts_them() {
    for length in [3, 50, 120, 213] {
        let code = Code::encode(&sample(length)).unwrap();
        let size = code.size();
        for (cx, cy) in [(3, 3), (size - 4, 3), (3, size - 4)] {
            for dy in 0..9usize {
                for dx in 0..9usize {
                    let (x, y) = (cx + dx, cy + dy);
                    let (x, y) = (x.wrapping_sub(4), y.wrapping_sub(4));
                    let ring = (cx.abs_diff(x)).max(cy.abs_diff(y));
                    if x < size && y < size {
                        assert_eq!(code.is_dark(x, y), ring != 2 && ring != 4, "{x},{y}");
                    }
                }
            }
        }
        for at in 8..size - 8 {
            assert_eq!(code.is_dark(at, 6), at % 2 == 0);
            assert_eq!(code.is_dark(6, at), at % 2 == 0);
        }
        assert!(code.is_dark(8, size - 8));
    }
}

#[test]
fn a_place_outside_the_code_is_light() {
    let code = Code::encode(b"hello").unwrap();
    assert!(!code.is_dark(code.size(), 0));
    assert!(!code.is_dark(0, code.size()));
    assert!(!code.is_dark(usize::MAX, usize::MAX));
}

/// The text is drawn so that reading the lines back gives the code with its quiet zone, and every
/// line sets its own colours and resets them.
#[test]
fn the_lines_draw_the_code_with_a_quiet_zone_in_black_on_white() {
    let code = Code::encode(b"bravebot://pair?v=1").unwrap();
    let drawn = render(&code);
    let lines: Vec<&str> = drawn.split('\n').collect();
    let span = code.size() + 8;
    assert_eq!(lines.len(), span.div_ceil(2));
    let mut grid: Vec<Vec<bool>> = Vec::new();
    for line in &lines {
        let body = line
            .strip_prefix("\u{1b}[30;47m")
            .and_then(|line| line.strip_suffix("\u{1b}[0m"))
            .unwrap();
        assert_eq!(body.chars().count(), span);
        let (mut upper, mut lower) = (Vec::new(), Vec::new());
        for character in body.chars() {
            let (top, bottom) = pair_of(character).unwrap();
            upper.push(top);
            lower.push(bottom);
        }
        grid.push(upper);
        grid.push(lower);
    }
    for (y, row) in grid.iter().enumerate() {
        for (x, &drawn) in row.iter().enumerate() {
            let expected = x >= 4 && y >= 4 && code.is_dark(x - 4, y - 4);
            assert_eq!(drawn, expected, "{x},{y}");
        }
    }
}

/// Codes whose modules were read back by the operating system's own QR reader when they were
/// recorded: the whole square for the smallest, and a hash of the modules for three longer ones. A
/// change to how a mask is chosen or how a square is laid out changes these, and the new square
/// needs the same reading before the value is replaced.
#[test]
fn codes_the_system_reader_read_are_drawn_as_they_were() {
    let hello = Code::encode(b"HELLO").unwrap();
    let drawn: Vec<String> = (0..hello.size())
        .map(|y| {
            (0..hello.size())
                .map(|x| if hello.is_dark(x, y) { '#' } else { '.' })
                .collect()
        })
        .collect();
    let recorded = "\
#######.##.#..#######
#.....#..##.#.#.....#
#.###.#..####.#.###.#
#.###.#.#..#..#.###.#
#.###.#.#...#.#.###.#
#.....#.#.##..#.....#
#######.#.#.#.#######
........#####........
#...#.######.#####..#
...###..#.###..#.####
#.##..#.#.##..###..#.
###..#...#...##.#....
..#.###..#..###...##.
........###.###..#.##
#######.##..##...#.#.
#.....#....##..#...#.
#.###.#.#..#..###.#.#
#.###.#....##....#.##
#.###.#..###..####...
#.....#..#...##......
#######.#...#####.#.#";
    assert_eq!(drawn.join("\n"), recorded);

    let hash = |code: &Code| {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for y in 0..code.size() {
            for x in 0..code.size() {
                hash ^= u64::from(code.is_dark(x, y));
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        hash
    };
    let invitation = "bravebot://pair?v=1&h=8ddc2cbb66194a3cf54633c595d83fb79c55b7ba\
&a=192.168.4.35:7443&s=Zm9vYmFyYmF6cXV4";
    let two_addresses = "bravebot://pair?v=1&h=8ddc2cbb66194a3cf54633c595d83fb79c55b7ba\
fd1ed406146ec759e2b222f2&a=192.168.4.35:7443&a=100.64.12.7:7443&s=kJq3N0vHn7Yx2cLw9d4tUe1sFgBZa8oPiR5MmXy6VhA";
    let longest = "abc".repeat(71);
    for (text, version, expected) in [
        (invitation, 6, 0xcaa0_3bb3_5a98_8640),
        (two_addresses, 9, 0x5ab5_daa0_ed0e_16c1),
        (longest.as_str(), 10, 0x1be2_b033_d72f_2135),
    ] {
        let code = Code::encode(text.as_bytes()).unwrap();
        assert_eq!(code.version(), version, "{text}");
        assert_eq!(hash(&code), expected, "{text}");
    }
}

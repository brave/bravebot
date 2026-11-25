// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reed-Solomon error correction over GF(256), as a QR code uses it: the field is the polynomials
//! over GF(2) modulo `x^8 + x^4 + x^3 + x^2 + 1` (0x11D), and the generator's roots are the powers of
//! 2 from `2^0`.

/// The product of two elements of the field.
pub(crate) fn multiply(first: u8, second: u8) -> u8 {
    let mut product: u16 = 0;
    for bit in (0..8).rev() {
        product <<= 1;
        if product & 0x100 != 0 {
            product ^= 0x11D;
        }
        if (second >> bit) & 1 != 0 {
            product ^= u16::from(first);
        }
    }
    // The loop keeps the product below 0x100.
    u8::try_from(product).unwrap_or(0)
}

/// The generator polynomial of degree `degree`, without its leading coefficient, highest power first.
fn divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree];
    if let Some(last) = result.last_mut() {
        *last = 1;
    }
    let mut root = 1u8;
    for _ in 0..degree {
        for at in 0..degree {
            result[at] = multiply(result[at], root);
            if let Some(&next) = result.get(at + 1) {
                result[at] ^= next;
            }
        }
        root = multiply(root, 2);
    }
    result
}

/// The `degree` error correction codewords of `data`: the remainder of dividing it, as a polynomial,
/// by the generator.
pub(crate) fn remainder(data: &[u8], degree: usize) -> Vec<u8> {
    let divisor = divisor(degree);
    let mut result = vec![0u8; degree];
    for &byte in data {
        let factor = byte ^ result.remove(0);
        result.push(0);
        for (place, &coefficient) in result.iter_mut().zip(&divisor) {
            *place ^= multiply(coefficient, factor);
        }
    }
    result
}

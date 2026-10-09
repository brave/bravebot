//! The pixel faces bots wear, drawn in half-blocks beside the mark on the opening screen.
//!
//! A port of `ui/src/renderer/avatar/pixels.ts`: the same streams, parts and colour maths, so a
//! seed draws the same shape in the terminal as in the app. The terminal paints it from the three
//! colours the wordmark is drawn through rather than from the app's palette, so the face reads as
//! part of the wordmark.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::theme::Rgb;

const GRID: usize = 12;
const INNER: i32 = 10;
const HALF: i32 = 5;

/// Columns of the drawn face.
pub const WIDTH: usize = INNER as usize;
/// Terminal rows of the drawn face, two grid rows to a terminal row.
pub const ROWS: usize = INNER as usize / 2;

const EYE_WHITE: Rgb = (0xff, 0xff, 0xff);
const EYE_PUPIL: Rgb = (0x1d, 0x1f, 0x27);
const EYE_CONTRAST: f64 = 1.8;

/// The paints a face is mixed from, and how far apart in hue any two on one face must be.
struct Palette<'a> {
    paints: &'a [Rgb],
    hue_distance: f64,
}

/// The terminal's palette: the wordmark's three colours. Brave's oranges are a few degrees of hue
/// apart, so no distance is required between them.
fn mark(stops: &[Rgb; 3]) -> Palette<'_> {
    Palette {
        paints: stops,
        hue_distance: 0.0,
    }
}

/// The app's palette, kept to check the port against faces `pixels.ts` drew.
#[cfg(test)]
const APP: Palette<'static> = Palette {
    paints: &APP_PAINT,
    hue_distance: 60.0,
};

#[cfg(test)]
const APP_PAINT: [Rgb; 11] = [
    (0xe5, 0x48, 0x4d),
    (0xf5, 0x92, 0x3a),
    (0xf2, 0xc9, 0x4c),
    (0x9f, 0xd8, 0x48),
    (0x30, 0xa4, 0x6c),
    (0x12, 0xb5, 0xa5),
    (0x3c, 0xb4, 0xf0),
    (0x3e, 0x63, 0xdd),
    (0x8e, 0x4e, 0xc6),
    (0xd6, 0x40, 0x9f),
    (0xf3, 0x8b, 0xb5),
];

const BLOB: usize = 6;
const BUST: usize = 7;
const BODIES: [&[i32]; 8] = [
    &[2, 4, 5, 5, 5],
    &[3, 5, 5, 5],
    &[5, 5, 5, 5, 4],
    &[2, 3, 4, 4, 4, 3],
    &[3, 4, 4, 4, 4, 3],
    &[1, 2, 3, 4, 4, 3],
    &[3, 4, 5, 5, 5, 4, 3],
    &[4, 4, 4, 4, 4, 4, 3],
];

#[derive(Clone, Copy, PartialEq)]
enum Crown {
    None,
    Antennae,
    Spike,
    Tuft,
    Horns,
}
const CROWNS: [Crown; 5] = [
    Crown::None,
    Crown::Antennae,
    Crown::Spike,
    Crown::Tuft,
    Crown::Horns,
];

#[derive(Clone, Copy)]
enum Arms {
    None,
    Raised,
    Down,
    Side,
}
const ARMS: [Arms; 4] = [Arms::None, Arms::Raised, Arms::Down, Arms::Side];

#[derive(Clone, Copy)]
enum Legs {
    None,
    Two,
    Splayed,
    Three,
    Comb,
    Feet,
}
const LEGS: [Legs; 5] = [
    Legs::Two,
    Legs::Splayed,
    Legs::Three,
    Legs::Comb,
    Legs::Feet,
];

#[derive(Clone, Copy)]
enum Direction {
    Vertical,
    Horizontal,
    Diagonal,
    Radial,
}
const DIRECTIONS: [Direction; 4] = [
    Direction::Vertical,
    Direction::Horizontal,
    Direction::Diagonal,
    Direction::Radial,
];

struct Stream(u32);

/// xorshift32 over an FNV-1a hash of the seed, giving numbers in [0, 1).
fn stream(seed: &str) -> Stream {
    let mut state: u32 = 0x811c_9dc5;
    for unit in seed.encode_utf16() {
        state ^= u32::from(unit);
        state = state.wrapping_mul(0x0100_0193);
    }
    state ^= state >> 16;
    state = state.wrapping_mul(0x7feb_352d);
    state ^= state >> 15;
    Stream(if state == 0 { 1 } else { state })
}

impl Stream {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        f64::from(self.0) / 4_294_967_296.0
    }
}

fn pick<T: Copy>(seed: &str, key: &str, from: &[T]) -> T {
    let at = (stream(&format!("{seed}/{key}")).next() * from.len() as f64).floor() as usize;
    from[at]
}

#[derive(Clone, Copy)]
struct Lch {
    l: f64,
    c: f64,
    h: f64,
}

fn linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn gamma(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn channels((r, g, b): Rgb) -> [f64; 3] {
    [r, g, b].map(|v| f64::from(v) / 255.0)
}

fn rgb_of([r, g, b]: [f64; 3]) -> Rgb {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    (byte(r), byte(g), byte(b))
}

fn lch_of(colour: Rgb) -> Lch {
    let [r, g, b] = channels(colour).map(linear);
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let lightness = 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let b = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    Lch {
        l: lightness,
        c: a.hypot(b),
        h: (b.atan2(a).to_degrees() + 360.0) % 360.0,
    }
}

fn colour_of(lch: Lch) -> Rgb {
    let a = lch.c * lch.h.to_radians().cos();
    let b = lch.c * lch.h.to_radians().sin();
    let l = (lch.l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (lch.l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (lch.l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    rgb_of([
        gamma(4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s),
        gamma(-1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s),
        gamma(-0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s),
    ])
}

fn hue_distance(a: Rgb, b: Rgb) -> f64 {
    let d = (lch_of(a).h - lch_of(b).h).abs() % 360.0;
    if d > 180.0 { 360.0 - d } else { d }
}

fn contrast(a: Rgb, b: Rgb) -> f64 {
    let luminance = |colour: Rgb| {
        let [r, g, b] = channels(colour).map(linear);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The stops from one paint to the next through every paint between them the short way round the
/// hue ring, so a red to green face does not turn olive in the middle.
fn walk(palette: &Palette, paints: &[usize]) -> Vec<Lch> {
    let all = palette.paints;
    let mut ring: Vec<usize> = (0..all.len()).collect();
    ring.sort_by(|a, b| lch_of(all[*a]).h.total_cmp(&lch_of(all[*b]).h));
    let length = ring.len();
    let place = |paint: usize| ring.iter().position(|p| *p == paint).unwrap_or(0);

    let mut stops = vec![lch_of(all[ring[place(paints[0])]])];
    for pair in paints.windows(2) {
        let (from, to) = (place(pair[0]), place(pair[1]));
        let forward = (to + length - from) % length;
        let step = if forward <= length / 2 { 1 } else { length - 1 };
        let mut at = from;
        while at != to {
            at = (at + step) % length;
            stops.push(lch_of(all[ring[at]]));
        }
    }
    stops
}

fn along(stops: &[Lch], t: f64) -> Lch {
    let at = t.clamp(0.0, 0.9999) * (stops.len() - 1) as f64;
    let i = at.floor() as usize;
    let k = at - at.floor();
    let a = stops[i];
    let b = stops.get(i + 1).copied().unwrap_or(a);
    let mut dh = b.h - a.h;
    if dh > 180.0 {
        dh -= 360.0;
    }
    if dh < -180.0 {
        dh += 360.0;
    }
    Lch {
        l: a.l + (b.l - a.l) * k,
        c: a.c + (b.c - a.c) * k,
        h: (a.h + dh * k + 360.0) % 360.0,
    }
}

type Filled = [[bool; GRID]; GRID];

fn at(filled: &Filled, x: i32, y: i32) -> bool {
    usize::try_from(x)
        .ok()
        .zip(usize::try_from(y).ok())
        .and_then(|(x, y)| filled.get(y).and_then(|row| row.get(x)))
        .copied()
        .unwrap_or(false)
}

const NEIGHBOURS: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

struct Face {
    cells: Vec<(i32, i32, Rgb)>,
    eyes: [(i32, i32); 2],
    gaze: i32,
}

fn build(seed: &str, palette: &Palette) -> Face {
    let body = pick(seed, "body", &[0, 1, 2, 3, 4, 5, 6, 7]);
    let widths = BODIES[body];
    let widest = widths.iter().copied().max().unwrap_or(0);
    let wide_eyes = widest == HALF && pick(seed, "eyes", &[false, true]);
    let crown = pick(seed, "crown", &CROWNS);
    let arms = if widest < HALF {
        pick(seed, "arms", &ARMS)
    } else {
        Arms::None
    };
    let legged = body != BLOB && body != BUST;
    let legs = if legged {
        pick(seed, "legs", &LEGS)
    } else {
        Legs::None
    };

    let rows = widths.len() as i32;
    let crown_height = match crown {
        Crown::None => 0,
        Crown::Tuft => 1,
        _ => 2,
    };
    let height = crown_height + rows + if legged { 2 } else { 0 };
    let top = 1 + (INNER - height) / 2 + crown_height;
    let bottom = top + rows - 1;

    let outer = if wide_eyes { 3 } else { 2 };
    let mut eye_index = rows - if legged { 2 } else { 3 };
    let width_at = |i: i32| usize::try_from(i).ok().and_then(|i| widths.get(i)).copied();
    while eye_index > 1
        && !(width_at(eye_index).is_some_and(|w| w >= outer + 2)
            && width_at(eye_index - 1).is_some_and(|w| w > outer)
            && width_at(eye_index + 1).is_some_and(|w| w > outer))
    {
        eye_index -= 1;
    }
    let eye_row = top + eye_index;

    let mut half: Vec<(i32, i32)> = Vec::new();
    for (i, width) in widths.iter().enumerate() {
        for h in 0..*width {
            half.push((top + i as i32, h));
        }
    }
    let top_width = widths[0];
    let crown_cells: Vec<(i32, i32)> = match crown {
        Crown::None => vec![],
        Crown::Antennae => vec![
            (-1, (HALF - 1).min(top_width)),
            (-2, (HALF - 1).min(top_width + 1)),
        ],
        Crown::Spike => vec![(-1, 0), (-2, 0)],
        Crown::Tuft => (0..(top_width + 1) / 2).map(|i| (-1, i * 2)).collect(),
        Crown::Horns => vec![(-1, top_width - 1), (-2, top_width - 1)],
    };
    half.extend(crown_cells.into_iter().map(|(dr, h)| (top + dr, h)));

    let edge = HALF - 1;
    let arm_cells: Vec<(i32, i32)> = match arms {
        Arms::None => vec![],
        Arms::Raised => vec![(0, edge), (-1, edge), (-2, edge)],
        Arms::Down => vec![(0, edge), (1, edge), (2, edge)],
        Arms::Side => vec![(-1, edge), (0, edge), (1, edge)],
    };
    half.extend(arm_cells.into_iter().map(|(dr, h)| (eye_row + dr, h)));

    let base = widths[widths.len() - 1];
    let leg_cells: Vec<(i32, i32)> = match legs {
        Legs::None => vec![],
        Legs::Two => vec![(1, base - 2), (2, base - 2)],
        Legs::Splayed => vec![(1, base - 2), (2, edge.min(base - 1))],
        Legs::Three => vec![(1, 0), (2, 0), (1, base - 1), (2, base - 1)],
        Legs::Comb => (0..base / 2)
            .flat_map(|i| [(1, i * 2 + 1), (2, i * 2 + 1)])
            .collect(),
        Legs::Feet => vec![(1, base - 2), (2, base - 2), (2, 0.max(base - 3))],
    };
    half.extend(leg_cells.into_iter().map(|(dr, h)| (bottom + dr, h)));

    let mut filled: Filled = [[false; GRID]; GRID];
    for (row, h) in half {
        if !(1..=INNER).contains(&row) || !(0..HALF).contains(&h) {
            continue;
        }
        filled[row as usize][(HALF - h) as usize] = true;
        filled[row as usize][(HALF + 1 + h) as usize] = true;
    }

    let gaze = if stream(&format!("{seed}/gaze")).next() < 0.5 {
        -1
    } else {
        1
    };
    let left_eye = (HALF - outer + 1, eye_row);
    let right_eye = (HALF + outer - 1, eye_row);

    let mut guarded: Vec<(i32, i32)> = Vec::new();
    for eye in [left_eye, right_eye] {
        for dx in -1..=2 {
            for dy in -1..=1 {
                guarded.push((eye.0 + dx, eye.1 + dy));
            }
        }
    }
    lopsided(seed, &mut filled, &guarded);
    keep_connected(&mut filled, left_eye);

    let paints = paints_for(seed, palette);
    let direction = pick(seed, "direction", &DIRECTIONS);
    let fringe = stream(&format!("{seed}/fringe")).next() < 0.35;
    let cells = paint(
        seed,
        &filled,
        &walk(palette, &paints),
        direction,
        fringe,
        left_eye,
        right_eye,
    );
    Face {
        cells,
        eyes: [left_eye, right_eye],
        gaze,
    }
}

/// About a third of faces break the mirror with one to three cells on one side.
fn lopsided(seed: &str, filled: &mut Filled, guarded: &[(i32, i32)]) {
    let mut next = stream(&format!("{seed}/lopsided"));
    if next.next() >= 0.34 {
        return;
    }
    let left = next.next() < 0.5;
    let in_side = |x: i32| {
        if left {
            (1..=HALF).contains(&x)
        } else {
            x > HALF && x <= INNER
        }
    };
    let edits = 1 + (next.next() * 3.0).floor() as i32;
    for _ in 0..edits {
        let add = next.next() < 0.65;
        let mut candidates: Vec<(i32, i32)> = Vec::new();
        for y in 1..=INNER {
            for x in 1..=INNER {
                if !in_side(x) || guarded.contains(&(x, y)) {
                    continue;
                }
                let touching = NEIGHBOURS.iter().any(|(dx, dy)| at(filled, x + dx, y + dy));
                let open = NEIGHBOURS
                    .iter()
                    .any(|(dx, dy)| !at(filled, x + dx, y + dy));
                let wanted = if add {
                    !at(filled, x, y) && touching
                } else {
                    at(filled, x, y) && open
                };
                if wanted {
                    candidates.push((x, y));
                }
            }
        }
        let chosen = (next.next() * candidates.len() as f64).floor() as usize;
        if let Some((x, y)) = candidates.get(chosen) {
            filled[*y as usize][*x as usize] = add;
        }
    }
}

/// Drops anything not joined to the body, corners counting as joined.
fn keep_connected(filled: &mut Filled, from: (i32, i32)) {
    let mut seen: Filled = [[false; GRID]; GRID];
    seen[from.1 as usize][from.0 as usize] = true;
    let mut queue = vec![from];
    while let Some((x, y)) = queue.pop() {
        for dx in -1..=1 {
            for dy in -1..=1 {
                let (nx, ny) = (x + dx, y + dy);
                if at(filled, nx, ny) && !at(&seen, nx, ny) {
                    seen[ny as usize][nx as usize] = true;
                    queue.push((nx, ny));
                }
            }
        }
    }
    for (row, kept) in filled.iter_mut().zip(seen.iter()) {
        for (cell, kept) in row.iter_mut().zip(kept.iter()) {
            *cell &= *kept;
        }
    }
}

/// Two or three distinct paints, each at least the palette's hue distance from every other.
fn paints_for(seed: &str, palette: &Palette) -> Vec<usize> {
    let all = palette.paints;
    let mut next = stream(&format!("{seed}/paints"));
    let mut chosen = vec![(next.next() * all.len() as f64).floor() as usize];
    let count = if next.next() < 0.4 { 3 } else { 2 };
    while chosen.len() < count {
        let allowed: Vec<usize> = (0..all.len())
            .filter(|p| {
                !chosen.contains(p)
                    && chosen
                        .iter()
                        .all(|c| hue_distance(all[*c], all[*p]) >= palette.hue_distance)
            })
            .collect();
        if allowed.is_empty() {
            break;
        }
        chosen.push(allowed[(next.next() * allowed.len() as f64).floor() as usize]);
    }
    chosen
}

fn paint(
    seed: &str,
    filled: &Filled,
    stops: &[Lch],
    direction: Direction,
    fringe: bool,
    left_eye: (i32, i32),
    right_eye: (i32, i32),
) -> Vec<(i32, i32, Rgb)> {
    let mut next = stream(&format!("{seed}/tiles"));
    let flip = stream(&format!("{seed}/flip")).next() < 0.5;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (GRID as i32, 0, GRID as i32, 0);
    for y in 0..GRID as i32 {
        for x in 0..GRID as i32 {
            if at(filled, x, y) {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    let span = |v: i32, lo: i32, hi: i32| {
        if hi == lo {
            0.5
        } else {
            f64::from(v - lo) / f64::from(hi - lo)
        }
    };
    let face = (
        f64::from(left_eye.0 + right_eye.0 + 1) / 2.0 + 0.5,
        f64::from(left_eye.1) + 0.5,
    );
    let reach = [
        (min_x, min_y),
        (max_x + 1, min_y),
        (min_x, max_y + 1),
        (max_x + 1, max_y + 1),
    ]
    .iter()
    .map(|(x, y)| (f64::from(*x) - face.0).hypot(f64::from(*y) - face.1))
    .fold(0.0, f64::max);

    let eye_cell = |x: i32, y: i32| {
        [left_eye, right_eye]
            .iter()
            .any(|e| y == e.1 && (x == e.0 || x == e.0 + 1))
    };
    let beside_eye = |x: i32, y: i32| {
        [left_eye, right_eye]
            .iter()
            .any(|e| x >= e.0 - 1 && x <= e.0 + 2 && y >= e.1 - 2 && y <= e.1 + 2)
    };

    let mut cells = Vec::new();
    for y in 0..GRID as i32 {
        for x in 0..GRID as i32 {
            if !at(filled, x, y) {
                continue;
            }
            let sx = span(x, min_x, max_x);
            let sy = span(y, min_y, max_y);
            let mut t = match direction {
                Direction::Vertical => sy,
                Direction::Horizontal => sx,
                Direction::Diagonal => (sx + sy) / 2.0,
                Direction::Radial => {
                    (f64::from(x) + 0.5 - face.0).hypot(f64::from(y) + 0.5 - face.1) / reach
                }
            };
            if flip {
                t = 1.0 - t;
            }
            t += (next.next() - 0.5) * 0.14;
            let mut colour = along(stops, t);
            colour.l = (colour.l + (next.next() - 0.5) * 0.06).clamp(0.3, 0.95);
            let open = NEIGHBOURS
                .iter()
                .any(|(dx, dy)| !at(filled, x + dx, y + dy));
            if fringe && open && !beside_eye(x, y) {
                colour.l = colour
                    .l
                    .max(0.86_f64.min(colour.l + (0.96 - colour.l) * 0.4));
                colour.c *= 0.7;
            }
            let mut fill = colour_of(colour);
            if beside_eye(x, y) && !eye_cell(x, y) {
                for _ in 0..12 {
                    if contrast(EYE_WHITE, fill) >= EYE_CONTRAST {
                        break;
                    }
                    colour.l -= 0.04;
                    fill = colour_of(colour);
                }
            }
            cells.push((x, y, fill));
        }
    }
    cells
}

/// How a face is held at one moment: whether the pupils have crossed to the other side of the
/// eye, and whether the lids are shut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    pub away: bool,
    pub blink: bool,
}

/// Range of per-face clock offsets, in seconds, as in `clock.ts`.
const CYCLE: f64 = 24.0;

/// The hash `motion.ts` uses for timing, over UTF-16 code units as `charCodeAt` sees them.
fn random_unit(seed: &str) -> f64 {
    let mut value: u32 = 0x811c_9dc5;
    for unit in seed.encode_utf16() {
        value ^= u32::from(unit);
        value = value.wrapping_mul(0x0100_0193);
    }
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    f64::from(value) / 4_294_967_296.0
}

fn smooth(x: f64) -> f64 {
    let c = x.clamp(0.0, 1.0);
    c * c * (3.0 - 2.0 * c)
}

fn glance(seconds: f64, seed: &str) -> f64 {
    let cycle = (seconds / 12.0).floor();
    let start = 5.0 + random_unit(&format!("{seed}/glance/{cycle}")) * 2.0;
    let at = seconds.rem_euclid(12.0) - start;
    let direction = if random_unit(&format!("{seed}/direction/{cycle}")) < 0.5 {
        -1.0
    } else {
        1.0
    };
    direction * smooth(at / 0.7) * (1.0 - smooth((at - 1.2) / 1.1))
}

fn blink(seconds: f64, seed: &str) -> f64 {
    let cycle = (seconds / 7.0).floor();
    let start = 1.0 + random_unit(&format!("{seed}/blink/{cycle}")) * 3.5;
    let at = seconds.rem_euclid(7.0) - start;
    let one = |elapsed: f64| {
        if !(0.0..0.16).contains(&elapsed) {
            1.0
        } else if elapsed < 0.05 {
            1.0 - smooth(elapsed / 0.05) * 0.92
        } else {
            0.08 + smooth((elapsed - 0.05) / 0.11) * 0.92
        }
    };
    let double = random_unit(&format!("{seed}/double/{cycle}")) < 0.18;
    one(at).min(if double { one(at - 0.24) } else { 1.0 })
}

/// How an idle face is held `seconds` into its life, the schedule `clock.ts` gives a bot in a list.
pub fn look_at(seed: &str, seconds: f64) -> Look {
    let t = seconds + random_unit(seed) * CYCLE;
    Look {
        away: glance(t, seed).abs() > 0.5,
        blink: blink(t, seed) < 0.5,
    }
}

/// How the face of this process is held now. Still where `NO_MOTION` is set.
pub fn look_now() -> Look {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    let seconds = STARTED.get_or_init(Instant::now).elapsed().as_secs_f64();
    held(startup_seed(), seconds, crate::indicator::stilled())
}

/// [`look_at`], or the face at rest where no motion was asked for. Taken as an argument so the rule
/// can be checked without putting a process-wide switch in force under every other test.
fn held(seed: &str, seconds: f64, still: bool) -> Look {
    if still {
        Look::default()
    } else {
        look_at(seed, seconds)
    }
}

/// The face a seed describes, painted from `stops`, as terminal rows of [`WIDTH`] columns, held as
/// `look` says.
pub fn rows(seed: &str, look: Look, stops: &[Rgb; 3]) -> Vec<Vec<Span<'static>>> {
    let face = build(seed, &mark(stops));
    let mut pixels: [[Option<Rgb>; GRID]; GRID] = [[None; GRID]; GRID];
    for (x, y, fill) in &face.cells {
        pixels[*y as usize][*x as usize] = Some(*fill);
    }
    let gaze = if look.away { -face.gaze } else { face.gaze };
    for (x, y) in face.eyes {
        let (white, pupil) = if gaze > 0 { (x, x + 1) } else { (x + 1, x) };
        let white_fill = if look.blink { EYE_PUPIL } else { EYE_WHITE };
        pixels[y as usize][white as usize] = Some(white_fill);
        pixels[y as usize][pupil as usize] = Some(EYE_PUPIL);
    }

    // One grid row up, unless the figure already reaches the top row.
    let lift = usize::from(pixels[1].iter().all(Option::is_none));
    let colour = |(r, g, b): Rgb| Color::Rgb(r, g, b);
    (0..ROWS)
        .map(|row| {
            let (upper, lower) = (1 + lift + 2 * row, 2 + lift + 2 * row);
            (1..=WIDTH)
                .map(|x| match (pixels[upper][x], pixels[lower][x]) {
                    (None, None) => Span::raw(" "),
                    (Some(top), None) => Span::styled("▀", Style::default().fg(colour(top))),
                    (None, Some(bottom)) => Span::styled("▄", Style::default().fg(colour(bottom))),
                    (Some(top), Some(bottom)) => {
                        Span::styled("▀", Style::default().fg(colour(top)).bg(colour(bottom)))
                    }
                })
                .collect()
        })
        .collect()
}

/// The seed this process draws, chosen once so every frame shows the same face and the next
/// startup shows another.
pub fn startup_seed() -> &'static str {
    static SEED: OnceLock<String> = OnceLock::new();
    SEED.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        format!("v2:{nanos:x}-{:x}", std::process::id())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::BRAND_STOPS;

    /// Faces drawn by `pixels.ts` for the same seeds: the cell count, a hash over every cell's
    /// position and colour, the eyes, the gaze and the first and last cell.
    #[allow(clippy::type_complexity)]
    const FROM_THE_APP: &[(
        &str,
        usize,
        u32,
        [(i32, i32); 2],
        i32,
        (i32, i32, Rgb),
        (i32, i32, Rgb),
    )] = &[
        (
            "v2:example-0",
            56,
            2_989_201_312,
            [(4, 7), (6, 7)],
            1,
            (5, 1, (0xe3, 0xca, 0x3e)),
            (7, 10, (0xee, 0x7a, 0x31)),
        ),
        (
            "v2:example-7",
            58,
            1_567_046_496,
            [(4, 7), (6, 7)],
            -1,
            (2, 1, (0x27, 0xae, 0x83)),
            (8, 10, (0xbf, 0xd4, 0x3c)),
        ),
        (
            "v2:example-13",
            61,
            4_037_921_789,
            [(4, 6), (6, 6)],
            1,
            (4, 1, (0x00, 0xad, 0x9c)),
            (2, 10, (0x00, 0xb1, 0xb1)),
        ),
        (
            "v2:example-21",
            44,
            691_716_615,
            [(4, 7), (6, 7)],
            -1,
            (5, 1, (0xf5, 0x91, 0x3a)),
            (7, 10, (0x94, 0xd5, 0x50)),
        ),
        (
            "review-3",
            58,
            4_254_328_371,
            [(4, 6), (6, 6)],
            -1,
            (3, 2, (0xf4, 0xca, 0x80)),
            (8, 8, (0xd5, 0x3f, 0x9e)),
        ),
    ];

    fn hash(face: &Face) -> u32 {
        face.cells
            .iter()
            .fold(0x811c_9dc5_u32, |hash, (x, y, (r, g, b))| {
                let rgb = (u32::from(*r) << 16) | (u32::from(*g) << 8) | u32::from(*b);
                [*x as u32, *y as u32, rgb]
                    .into_iter()
                    .fold(hash, |hash, v| (hash ^ v).wrapping_mul(0x0100_0193))
            })
    }

    /// Given the app's palette, the terminal face is the app's face: a different PRNG step, colour
    /// step or part table changes a cell, and so the hash.
    #[test]
    fn a_seed_draws_the_face_the_app_draws() {
        for (seed, count, expected, eyes, gaze, first, last) in FROM_THE_APP {
            let face = build(seed, &APP);
            assert_eq!(face.cells.len(), *count, "{seed}");
            assert_eq!(hash(&face), *expected, "{seed}");
            assert_eq!(face.eyes, *eyes, "{seed}");
            assert_eq!(face.gaze, *gaze, "{seed}");
            assert_eq!(face.cells.first(), Some(first), "{seed}");
            assert_eq!(face.cells.last(), Some(last), "{seed}");
        }
    }

    /// Every face fits the grid the terminal rows are cut from, so none is clipped.
    #[test]
    fn every_face_sits_inside_the_drawn_area() {
        for i in 0..300 {
            let face = build(&format!("v2:fit-{i}"), &mark(&BRAND_STOPS));
            assert!(
                face.cells
                    .iter()
                    .all(|(x, y, _)| (1..=INNER).contains(x) && (1..=INNER).contains(y)),
                "v2:fit-{i} leaves the area"
            );
        }
    }

    /// Different seeds draw different faces, or "a different one each startup" would not show.
    #[test]
    fn seeds_draw_distinct_faces() {
        let hashes: std::collections::HashSet<u32> = (0..200)
            .map(|i| hash(&build(&format!("v2:distinct-{i}"), &mark(&BRAND_STOPS))))
            .collect();
        assert!(hashes.len() >= 195, "only {} of 200 differ", hashes.len());
    }

    /// The terminal face is painted from the brand's oranges alone. A face mixed from the app's
    /// palette has green, blue or purple cells, where red is not the strongest channel.
    #[test]
    fn the_terminal_face_is_painted_in_the_brands_oranges() {
        for i in 0..300 {
            let seed = format!("v2:orange-{i}");
            for (x, y, (r, g, b)) in build(&seed, &mark(&BRAND_STOPS)).cells {
                assert!(
                    r > g && g >= b,
                    "{seed} paints ({x}, {y}) as #{r:02x}{g:02x}{b:02x}"
                );
            }
        }
    }

    /// Under a named theme the face is painted from that theme's colours instead. Blue stops are
    /// far from any orange, so a face that kept the oranges has cells where blue is not the
    /// strongest channel.
    #[test]
    fn a_face_is_painted_from_the_colours_it_is_given() {
        let blues = [(0x5e, 0x81, 0xac), (0x81, 0xa1, 0xc1), (0x88, 0xc0, 0xd0)];
        for i in 0..300 {
            let seed = format!("v2:blue-{i}");
            for (x, y, (r, g, b)) in build(&seed, &mark(&blues)).cells {
                assert!(
                    b > r && b >= g,
                    "{seed} paints ({x}, {y}) as #{r:02x}{g:02x}{b:02x}"
                );
            }
        }
    }

    /// Two or three paints are still chosen, as in the app, rather than the hue distance between
    /// the oranges collapsing every face to one of them.
    #[test]
    fn a_terminal_face_mixes_more_than_one_orange() {
        for i in 0..300 {
            let paints = paints_for(&format!("v2:mix-{i}"), &mark(&BRAND_STOPS));
            let mut distinct = paints.clone();
            distinct.sort_unstable();
            distinct.dedup();
            assert!(
                (2..=3).contains(&paints.len()) && distinct.len() == paints.len(),
                "v2:mix-{i} chose {paints:?}"
            );
        }
    }

    #[test]
    fn a_face_is_five_rows_of_ten_columns_with_the_eyes_on_it() {
        let drawn = rows("v2:example-0", Look::default(), &BRAND_STOPS);
        assert_eq!(drawn.len(), ROWS);
        assert!(drawn.iter().all(|row| row.len() == WIDTH));
        let eyes = drawn
            .iter()
            .flatten()
            .filter(|span| {
                let style = span.style;
                [style.fg, style.bg].contains(&Some(Color::Rgb(
                    EYE_PUPIL.0,
                    EYE_PUPIL.1,
                    EYE_PUPIL.2,
                )))
            })
            .count();
        assert!(eyes >= 2, "no pupils were drawn");
    }

    /// Lifting the face a row never drops a painted cell off the top or the bottom.
    #[test]
    fn the_lifted_face_keeps_every_cell() {
        for i in 0..300 {
            let seed = format!("v2:lift-{i}");
            let painted: usize = rows(
                &seed,
                Look {
                    away: i % 2 == 0,
                    blink: i % 3 == 0,
                },
                &BRAND_STOPS,
            )
            .iter()
            .flatten()
            .map(|span| match span.content.as_ref() {
                " " => 0,
                _ if span.style.bg.is_some() => 2,
                _ => 1,
            })
            .sum();
            assert_eq!(
                painted,
                build(&seed, &mark(&BRAND_STOPS)).cells.len(),
                "{seed}"
            );
        }
    }

    /// Times at which `motion.ts` closes the lids or turns the pupils for `v2:example-0`, and times
    /// it does not.
    #[test]
    fn the_face_blinks_and_glances_when_the_app_does() {
        let seed = "v2:example-0";
        for closed in [6.0, 6.05, 13.8, 20.3, 35.6] {
            assert!(look_at(seed, closed).blink, "{closed}");
        }
        for open in [0.0, 5.9, 6.2, 13.0, 14.5, 25.0] {
            assert!(!look_at(seed, open).blink, "{open}");
        }
        for away in [8.5, 9.2, 20.5, 32.2, 44.0, 57.0] {
            assert!(look_at(seed, away).away, "{away}");
        }
        for toward in [0.0, 8.0, 10.0, 15.0, 25.0, 40.0] {
            assert!(!look_at(seed, toward).away, "{toward}");
        }
    }

    #[test]
    fn the_face_stays_at_rest_where_no_motion_is_asked_for() {
        assert!(held("v2:example-0", 6.0, false).blink);
        assert!(held("v2:example-0", 8.5, false).away);
        for seconds in [6.0, 8.5, 13.8, 20.5] {
            assert_eq!(held("v2:example-0", seconds, true), Look::default());
        }
    }

    #[test]
    fn a_blink_is_short_and_a_face_is_mostly_still() {
        let held: Vec<Look> = (0..2400)
            .map(|step| look_at("v2:example-0", f64::from(step) * 0.05))
            .collect();
        let shut = held.iter().filter(|look| look.blink).count();
        let turned = held.iter().filter(|look| look.away).count();
        assert!(shut > 0 && shut < held.len() / 20, "shut {shut}");
        assert!(turned > 0 && turned < held.len() / 5, "turned {turned}");
    }

    #[test]
    fn a_look_changes_the_eyes_and_nothing_else() {
        let at_rest = rows("v2:example-0", Look::default(), &BRAND_STOPS);
        let shut = rows(
            "v2:example-0",
            Look {
                away: false,
                blink: true,
            },
            &BRAND_STOPS,
        );
        let turned = rows(
            "v2:example-0",
            Look {
                away: true,
                blink: false,
            },
            &BRAND_STOPS,
        );
        let differing = |a: &[Vec<Span<'static>>], b: &[Vec<Span<'static>>]| {
            a.iter()
                .flatten()
                .zip(b.iter().flatten())
                .filter(|(a, b)| a != b)
                .count()
        };
        assert!(differing(&at_rest, &shut) > 0);
        assert!(differing(&at_rest, &turned) > 0);
        assert!(differing(&at_rest, &shut) <= 2);
        assert!(differing(&at_rest, &turned) <= 4);
    }

    #[test]
    fn the_startup_seed_is_the_same_for_the_whole_process() {
        assert_eq!(startup_seed(), startup_seed());
    }
}

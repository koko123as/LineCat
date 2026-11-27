//! Reads a QR code out of a grayscale picture: binarize, find the three finder patterns,
//! sample the grid through a perspective transform, then the format, the Reed-Solomon blocks
//! and the segments (ISO/IEC 18004). FlClash: lib/pages/scan.dart (`picker.pickerConfigQRCode`).

/// Table 9 of ISO/IEC 18004 by version, in L M Q H order: EC codewords per block.
const EC_PER_BLOCK: [[u8; 4]; 40] = [
    [7, 10, 13, 17],
    [10, 16, 22, 28],
    [15, 26, 18, 22],
    [20, 18, 26, 16],
    [26, 24, 18, 22],
    [18, 16, 24, 28],
    [20, 18, 18, 26],
    [24, 22, 22, 26],
    [30, 22, 20, 24],
    [18, 26, 24, 28],
    [20, 30, 28, 24],
    [24, 22, 26, 28],
    [26, 22, 24, 22],
    [30, 24, 20, 24],
    [22, 24, 30, 24],
    [24, 28, 24, 30],
    [28, 28, 28, 28],
    [30, 26, 28, 28],
    [28, 26, 26, 26],
    [28, 26, 30, 28],
    [28, 26, 28, 30],
    [28, 28, 30, 24],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [26, 28, 30, 30],
    [28, 28, 28, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
    [30, 28, 30, 30],
];
/// Data codewords per block and block count, for the two block groups.
const BLOCKS: [[(u8, u8, u8, u8); 4]; 40] = [
    [(19, 1, 0, 0), (16, 1, 0, 0), (13, 1, 0, 0), (9, 1, 0, 0)],
    [(34, 1, 0, 0), (28, 1, 0, 0), (22, 1, 0, 0), (16, 1, 0, 0)],
    [(55, 1, 0, 0), (44, 1, 0, 0), (17, 2, 0, 0), (13, 2, 0, 0)],
    [(80, 1, 0, 0), (32, 2, 0, 0), (24, 2, 0, 0), (9, 4, 0, 0)],
    [
        (108, 1, 0, 0),
        (43, 2, 0, 0),
        (15, 2, 16, 2),
        (11, 2, 12, 2),
    ],
    [(68, 2, 0, 0), (27, 4, 0, 0), (19, 4, 0, 0), (15, 4, 0, 0)],
    [(78, 2, 0, 0), (31, 4, 0, 0), (14, 2, 15, 4), (13, 4, 14, 1)],
    [
        (97, 2, 0, 0),
        (38, 2, 39, 2),
        (18, 4, 19, 2),
        (14, 4, 15, 2),
    ],
    [
        (116, 2, 0, 0),
        (36, 3, 37, 2),
        (16, 4, 17, 4),
        (12, 4, 13, 4),
    ],
    [
        (68, 2, 69, 2),
        (43, 4, 44, 1),
        (19, 6, 20, 2),
        (15, 6, 16, 2),
    ],
    [
        (81, 4, 0, 0),
        (50, 1, 51, 4),
        (22, 4, 23, 4),
        (12, 3, 13, 8),
    ],
    [
        (92, 2, 93, 2),
        (36, 6, 37, 2),
        (20, 4, 21, 6),
        (14, 7, 15, 4),
    ],
    [
        (107, 4, 0, 0),
        (37, 8, 38, 1),
        (20, 8, 21, 4),
        (11, 12, 12, 4),
    ],
    [
        (115, 3, 116, 1),
        (40, 4, 41, 5),
        (16, 11, 17, 5),
        (12, 11, 13, 5),
    ],
    [
        (87, 5, 88, 1),
        (41, 5, 42, 5),
        (24, 5, 25, 7),
        (12, 11, 13, 7),
    ],
    [
        (98, 5, 99, 1),
        (45, 7, 46, 3),
        (19, 15, 20, 2),
        (15, 3, 16, 13),
    ],
    [
        (107, 1, 108, 5),
        (46, 10, 47, 1),
        (22, 1, 23, 15),
        (14, 2, 15, 17),
    ],
    [
        (120, 5, 121, 1),
        (43, 9, 44, 4),
        (22, 17, 23, 1),
        (14, 2, 15, 19),
    ],
    [
        (113, 3, 114, 4),
        (44, 3, 45, 11),
        (21, 17, 22, 4),
        (13, 9, 14, 16),
    ],
    [
        (107, 3, 108, 5),
        (41, 3, 42, 13),
        (24, 15, 25, 5),
        (15, 15, 16, 10),
    ],
    [
        (116, 4, 117, 4),
        (42, 17, 0, 0),
        (22, 17, 23, 6),
        (16, 19, 17, 6),
    ],
    [
        (111, 2, 112, 7),
        (46, 17, 0, 0),
        (24, 7, 25, 16),
        (13, 34, 0, 0),
    ],
    [
        (121, 4, 122, 5),
        (47, 4, 48, 14),
        (24, 11, 25, 14),
        (15, 16, 16, 14),
    ],
    [
        (117, 6, 118, 4),
        (45, 6, 46, 14),
        (24, 11, 25, 16),
        (16, 30, 17, 2),
    ],
    [
        (106, 8, 107, 4),
        (47, 8, 48, 13),
        (24, 7, 25, 22),
        (15, 22, 16, 13),
    ],
    [
        (114, 10, 115, 2),
        (46, 19, 47, 4),
        (22, 28, 23, 6),
        (16, 33, 17, 4),
    ],
    [
        (122, 8, 123, 4),
        (45, 22, 46, 3),
        (23, 8, 24, 26),
        (15, 12, 16, 28),
    ],
    [
        (117, 3, 118, 10),
        (45, 3, 46, 23),
        (24, 4, 25, 31),
        (15, 11, 16, 31),
    ],
    [
        (116, 7, 117, 7),
        (45, 21, 46, 7),
        (23, 1, 24, 37),
        (15, 19, 16, 26),
    ],
    [
        (115, 5, 116, 10),
        (47, 19, 48, 10),
        (24, 15, 25, 25),
        (15, 23, 16, 25),
    ],
    [
        (115, 13, 116, 3),
        (46, 2, 47, 29),
        (24, 42, 25, 1),
        (15, 23, 16, 28),
    ],
    [
        (115, 17, 0, 0),
        (46, 10, 47, 23),
        (24, 10, 25, 35),
        (15, 19, 16, 35),
    ],
    [
        (115, 17, 116, 1),
        (46, 14, 47, 21),
        (24, 29, 25, 19),
        (15, 11, 16, 46),
    ],
    [
        (115, 13, 116, 6),
        (46, 14, 47, 23),
        (24, 44, 25, 7),
        (16, 59, 17, 1),
    ],
    [
        (121, 12, 122, 7),
        (47, 12, 48, 26),
        (24, 39, 25, 14),
        (15, 22, 16, 41),
    ],
    [
        (121, 6, 122, 14),
        (47, 6, 48, 34),
        (24, 46, 25, 10),
        (15, 2, 16, 64),
    ],
    [
        (122, 17, 123, 4),
        (46, 29, 47, 14),
        (24, 49, 25, 10),
        (15, 24, 16, 46),
    ],
    [
        (122, 4, 123, 18),
        (46, 13, 47, 32),
        (24, 48, 25, 14),
        (15, 42, 16, 32),
    ],
    [
        (117, 20, 118, 4),
        (47, 40, 48, 7),
        (24, 43, 25, 22),
        (15, 10, 16, 67),
    ],
    [
        (118, 19, 119, 6),
        (47, 18, 48, 31),
        (24, 34, 25, 34),
        (15, 20, 16, 61),
    ],
];

const ALNUM: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

/// The text of the first QR code found in `luma` (one byte per pixel, rows of `width`).
pub fn decode(luma: &[u8], width: usize, height: usize) -> Option<String> {
    if width < 21 || height < 21 || luma.len() < width * height {
        return None;
    }
    let img = Bits::binarize(luma, width, height);
    let centers = finders(&img);
    for [tl, tr, bl] in triples(&centers) {
        let ms = (tl.size + tr.size + bl.size) / 3.0;
        let legs = (dist(tl, tr) + dist(tl, bl)) / 2.0 / ms;
        let est = legs.round() as i32 + 7;
        let est = match est & 3 {
            0 => est + 1,
            2 => est - 1,
            3 => est + 2,
            _ => est,
        };
        for dim in [est, est - 4, est + 4] {
            if !(21..=177).contains(&dim) {
                continue;
            }
            let grid = sample(&img, tl, tr, bl, ms, dim as usize);
            if let Some(text) = grid.and_then(|g| read_grid(&g, dim as usize)) {
                return Some(text);
            }
        }
    }
    None
}

struct Bits {
    w: usize,
    h: usize,
    dark: Vec<bool>,
}

impl Bits {
    /// Local thresholds over 8 px blocks, as ZXing's HybridBinarizer.
    fn binarize(luma: &[u8], w: usize, h: usize) -> Bits {
        const B: usize = 8;
        let (bw, bh) = (w.div_ceil(B), h.div_ceil(B));
        let mut avg = vec![0u32; bw * bh];
        for by in 0..bh {
            for bx in 0..bw {
                let (mut sum, mut n, mut lo, mut hi) = (0u32, 0u32, 255u8, 0u8);
                for y in by * B..((by + 1) * B).min(h) {
                    for &p in &luma[y * w + bx * B..y * w + ((bx + 1) * B).min(w)] {
                        sum += p as u32;
                        n += 1;
                        lo = lo.min(p);
                        hi = hi.max(p);
                    }
                }
                let mut a = sum / n.max(1);
                if hi - lo <= 24 {
                    a = lo as u32 / 2;
                    if bx > 0 && by > 0 {
                        let near = (avg[(by - 1) * bw + bx]
                            + 2 * avg[by * bw + bx - 1]
                            + avg[(by - 1) * bw + bx - 1])
                            / 4;
                        if (lo as u32) < near {
                            a = near;
                        }
                    }
                }
                avg[by * bw + bx] = a;
            }
        }
        let mut dark = vec![false; w * h];
        for by in 0..bh {
            for bx in 0..bw {
                let (mut sum, mut n) = (0u32, 0u32);
                for y in by.saturating_sub(2)..(by + 3).min(bh) {
                    for x in bx.saturating_sub(2)..(bx + 3).min(bw) {
                        sum += avg[y * bw + x];
                        n += 1;
                    }
                }
                let t = sum / n;
                for y in by * B..((by + 1) * B).min(h) {
                    for x in bx * B..((bx + 1) * B).min(w) {
                        dark[y * w + x] = (luma[y * w + x] as u32) <= t;
                    }
                }
            }
        }
        Bits { w, h, dark }
    }

    fn at(&self, x: i64, y: i64) -> Option<bool> {
        (x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h)
            .then(|| self.dark[y as usize * self.w + x as usize])
    }
}

#[derive(Clone, Copy, Debug)]
struct Center {
    x: f64,
    y: f64,
    size: f64,
    hits: u32,
}

fn dist(a: Center, b: Center) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// Runs in a 1:1:3:1:1 ratio.
fn is_finder(c: &[f64; 5]) -> bool {
    let total: f64 = c.iter().sum();
    if total < 7.0 || c.iter().any(|&v| v == 0.0) {
        return false;
    }
    let m = total / 7.0;
    let v = m / 2.0;
    (m - c[0]).abs() < v
        && (m - c[1]).abs() < v
        && (3.0 * m - c[2]).abs() < 3.0 * v
        && (m - c[3]).abs() < v
        && (m - c[4]).abs() < v
}

/// The five runs through (x, y) along (dx, dy), and the centre's offset along that line.
fn cross(img: &Bits, x: f64, y: f64, dx: i64, dy: i64, max: f64) -> Option<([f64; 5], f64)> {
    let (x0, y0) = (x.floor() as i64, y.floor() as i64);
    let mut c = [0f64; 5];
    let walk = |sign: i64, c: &mut [f64; 5], order: [usize; 3]| -> Option<i64> {
        let mut i = if sign < 0 { 0 } else { 1 };
        let mut state = 0;
        loop {
            let px = img.at(x0 + sign * dx * i, y0 + sign * dy * i);
            let want_dark = state != 1;
            match px {
                Some(d) if d == want_dark => {
                    c[order[state]] += 1.0;
                    if c[order[state]] > max * if state == 0 { 3.0 } else { 1.0 } {
                        return None;
                    }
                    i += 1;
                }
                _ if state < 2 => state += 1,
                _ => return Some(i),
            }
        }
    };
    walk(-1, &mut c, [2, 1, 0])?;
    let end = walk(1, &mut c, [2, 3, 4])?;
    is_finder(&c).then(|| (c, end as f64 - c[4] - c[3] - c[2] / 2.0))
}

fn finders(img: &Bits) -> Vec<Center> {
    let mut out: Vec<Center> = Vec::new();
    let step = (img.h / 400).max(1);
    for y in (0..img.h).step_by(step) {
        let row = &img.dark[y * img.w..(y + 1) * img.w];
        let (mut c, mut state) = ([0f64; 5], 0usize);
        for x in 0..=img.w {
            let dark = x < img.w && row[x];
            if dark {
                if state % 2 == 1 {
                    state += 1;
                }
                c[state] += 1.0;
            } else if state % 2 == 1 {
                c[state] += 1.0;
            } else if state < 4 {
                state += 1;
                c[state] += 1.0;
            } else {
                if is_finder(&c) {
                    let total: f64 = c.iter().sum();
                    let cx = x as f64 - c[4] - c[3] - c[2] / 2.0;
                    if let Some(p) = confirm(img, cx, y as f64 + 0.5, total) {
                        add(&mut out, p);
                    }
                }
                c = [c[2], c[3], c[4], 1.0, 0.0];
                state = 3;
            }
        }
    }
    out
}

/// Checks a row hit down the column and again across the row through the new centre.
fn confirm(img: &Bits, x: f64, y: f64, total: f64) -> Option<Center> {
    let max = total / 7.0 * 2.0;
    let (cv, oy) = cross(img, x, y, 0, 1, max)?;
    let vt: f64 = cv.iter().sum();
    if 5.0 * (vt - total).abs() >= 2.0 * total {
        return None;
    }
    let cy = y.floor() + oy;
    let (ch, ox) = cross(img, x, cy, 1, 0, max)?;
    let ht: f64 = ch.iter().sum();
    let cx = x.floor() + ox;
    Some(Center {
        x: cx,
        y: cy,
        size: (vt + ht) / 14.0,
        hits: 1,
    })
}

fn add(list: &mut Vec<Center>, p: Center) {
    for c in list.iter_mut() {
        if (c.x - p.x).abs() <= c.size
            && (c.y - p.y).abs() <= c.size
            && (p.size - c.size).abs() <= c.size.max(1.0)
        {
            let n = c.hits as f64;
            c.x = (c.x * n + p.x) / (n + 1.0);
            c.y = (c.y * n + p.y) / (n + 1.0);
            c.size = (c.size * n + p.size) / (n + 1.0);
            c.hits += 1;
            return;
        }
    }
    list.push(p);
}

/// Plausible top-left, top-right and bottom-left patterns, best first.
fn triples(centers: &[Center]) -> Vec<[Center; 3]> {
    let mut list: Vec<Center> = centers.to_vec();
    list.sort_by(|a, b| b.hits.cmp(&a.hits));
    list.truncate(12);
    let mut out: Vec<(f64, [Center; 3])> = Vec::new();
    for i in 0..list.len() {
        for j in i + 1..list.len() {
            for k in j + 1..list.len() {
                let p = [list[i], list[j], list[k]];
                let sizes = p.map(|c| c.size);
                let lo = sizes.iter().cloned().fold(f64::MAX, f64::min);
                let hi = sizes.iter().cloned().fold(0.0, f64::max);
                if hi > lo * 1.5 {
                    continue;
                }
                let d = [dist(p[1], p[2]), dist(p[0], p[2]), dist(p[0], p[1])];
                let corner = (0..3).max_by(|&a, &b| d[a].total_cmp(&d[b])).unwrap();
                let tl = p[corner];
                let (mut b, mut c) = (p[(corner + 1) % 3], p[(corner + 2) % 3]);
                let (l1, l2) = (dist(tl, b), dist(tl, c));
                if l1.min(l2) < 7.0 * lo || l1.max(l2) > 1.5 * l1.min(l2) {
                    continue;
                }
                let skew = ((l1 * l1 + l2 * l2).sqrt() / d[corner] - 1.0).abs();
                if skew > 0.25 {
                    continue;
                }
                if (b.x - tl.x) * (c.y - tl.y) - (b.y - tl.y) * (c.x - tl.x) < 0.0 {
                    std::mem::swap(&mut b, &mut c);
                }
                let score = skew + (hi - lo) / lo + (l1 / l2 - 1.0).abs();
                out.push((score, [tl, b, c]));
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().take(4).map(|(_, t)| t).collect()
}

/// A homography from four point pairs, `(x, y) -> (X, Y)`.
fn homography(src: [(f64, f64); 4], dst: [(f64, f64); 4]) -> Option<[f64; 8]> {
    let mut m = [[0f64; 9]; 8];
    for i in 0..4 {
        let ((x, y), (u, v)) = (src[i], dst[i]);
        m[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, u];
        m[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v, v];
    }
    for col in 0..8 {
        let pivot = (col..8).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, pivot);
        for r in 0..8 {
            if r != col {
                let f = m[r][col] / m[col][col];
                for k in col..9 {
                    m[r][k] -= f * m[col][k];
                }
            }
        }
    }
    let mut h = [0f64; 8];
    for i in 0..8 {
        h[i] = m[i][8] / m[i][i];
    }
    Some(h)
}

fn apply(h: &[f64; 8], x: f64, y: f64) -> (f64, f64) {
    let d = h[6] * x + h[7] * y + 1.0;
    (
        (h[0] * x + h[1] * y + h[2]) / d,
        (h[3] * x + h[4] * y + h[5]) / d,
    )
}

/// The alignment pattern's centre near `(ex, ey)`: a dark module in a light ring.
fn alignment(img: &Bits, ex: f64, ey: f64, ms: f64) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64, f64)> = None;
    for reach in [4.0, 8.0, 16.0] {
        let r = (reach * ms) as i64;
        for y in (ey as i64 - r)..=(ey as i64 + r) {
            for x in (ex as i64 - r)..=(ex as i64 + r) {
                if img.at(x, y) != Some(true) {
                    continue;
                }
                let run = |dx: i64, dy: i64| -> Option<(f64, f64)> {
                    let (mut a, mut b) = (0i64, 0i64);
                    while img.at(x + dx * (a + 1), y + dy * (a + 1))? {
                        a += 1;
                    }
                    while img.at(x - dx * (b + 1), y - dy * (b + 1))? {
                        b += 1;
                    }
                    let len = (a + b + 1) as f64;
                    let ok = (len - ms).abs() < ms / 2.0;
                    let ring = |s: i64, from: i64| {
                        let mut n = 0;
                        while img.at(x + s * dx * (from + n + 1), y + s * dy * (from + n + 1))
                            == Some(false)
                        {
                            n += 1;
                        }
                        n as f64
                    };
                    let (l1, l2) = (ring(1, a), ring(-1, b));
                    let ok = ok && (l1 - ms).abs() < ms / 2.0 && (l2 - ms).abs() < ms / 2.0;
                    ok.then(|| ((a - b) as f64 / 2.0, len))
                };
                let (Some((ox, _)), Some((oy, _))) = (run(1, 0), run(0, 1)) else {
                    continue;
                };
                let (cx, cy) = (x as f64 + ox + 0.5, y as f64 + oy + 0.5);
                let d = (cx - ex).powi(2) + (cy - ey).powi(2);
                if best.is_none_or(|b| d < b.2) {
                    best = Some((cx, cy, d));
                }
            }
        }
        if let Some((x, y, _)) = best {
            return Some((x, y));
        }
    }
    None
}

fn sample(
    img: &Bits,
    tl: Center,
    tr: Center,
    bl: Center,
    ms: f64,
    dim: usize,
) -> Option<Vec<bool>> {
    let n = dim as f64;
    let br = (tr.x - tl.x + bl.x, tr.y - tl.y + bl.y);
    let (src_br, dst_br) = if dim > 21 {
        let k = 1.0 - 3.0 / (n - 7.0);
        let ex = tl.x + k * (br.0 - tl.x);
        let ey = tl.y + k * (br.1 - tl.y);
        match alignment(img, ex, ey, ms) {
            Some(p) => ((n - 6.5, n - 6.5), p),
            None => ((n - 3.5, n - 3.5), br),
        }
    } else {
        ((n - 3.5, n - 3.5), br)
    };
    let h = homography(
        [(3.5, 3.5), (n - 3.5, 3.5), src_br, (3.5, n - 3.5)],
        [(tl.x, tl.y), (tr.x, tr.y), dst_br, (bl.x, bl.y)],
    )?;
    let mut grid = vec![false; dim * dim];
    for y in 0..dim {
        for x in 0..dim {
            let (px, py) = apply(&h, x as f64 + 0.5, y as f64 + 0.5);
            grid[y * dim + x] = img.at(px.floor() as i64, py.floor() as i64)?;
        }
    }
    Some(grid)
}

fn bch_format(data: u32) -> u32 {
    let mut rem = data;
    for _ in 0..10 {
        rem = (rem << 1) ^ ((rem >> 9) * 0x537);
    }
    ((data << 10) | (rem & 0x3ff)) ^ 0x5412
}

fn alignment_positions(version: usize) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let count = version / 7 + 2;
    let step = if version == 32 {
        26
    } else {
        (version * 4 + count * 2 + 1) / (count * 2 - 2) * 2
    };
    let mut out = vec![6];
    let mut pos = version * 4 + 10;
    let mut rest = Vec::new();
    for _ in 1..count {
        rest.push(pos);
        pos -= step;
    }
    rest.reverse();
    out.extend(rest);
    out
}

fn function_modules(dim: usize, version: usize) -> Vec<bool> {
    let mut f = vec![false; dim * dim];
    let mut mark = |x0: usize, y0: usize, w: usize, h: usize| {
        for y in y0..(y0 + h).min(dim) {
            for x in x0..(x0 + w).min(dim) {
                f[y * dim + x] = true;
            }
        }
    };
    mark(0, 0, 9, 9);
    mark(dim - 8, 0, 8, 9);
    mark(0, dim - 8, 9, 8);
    mark(6, 0, 1, dim);
    mark(0, 6, dim, 1);
    let pos = alignment_positions(version);
    for (i, &ay) in pos.iter().enumerate() {
        for (j, &ax) in pos.iter().enumerate() {
            let last = pos.len() - 1;
            if (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0) {
                continue;
            }
            mark(ax - 2, ay - 2, 5, 5);
        }
    }
    if version >= 7 {
        mark(dim - 11, 0, 3, 6);
        mark(0, dim - 11, 6, 3);
    }
    f
}

fn masked(mask: u32, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y) % 2 == 0,
        1 => y % 2 == 0,
        2 => x % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => (x / 3 + y / 2) % 2 == 0,
        5 => x * y % 2 + x * y % 3 == 0,
        6 => (x * y % 2 + x * y % 3) % 2 == 0,
        _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
    }
}

/// The format's error correction level (as Table 9's L M Q H index) and mask.
fn format(g: &[bool], dim: usize) -> Option<(usize, u32)> {
    let at = |x: usize, y: usize| g[y * dim + x] as u32;
    let (mut a, mut b) = (0u32, 0u32);
    let first: Vec<(usize, usize)> = (0..6)
        .map(|i| (8, i))
        .chain([(8, 7), (8, 8), (7, 8)])
        .chain((9..15).map(|i| (14 - i, 8)))
        .collect();
    let second: Vec<(usize, usize)> = (0..8)
        .map(|i| (dim - 1 - i, 8))
        .chain((8..15).map(|i| (8, dim - 15 + i)))
        .collect();
    for i in 0..15 {
        a |= at(first[i].0, first[i].1) << i;
        b |= at(second[i].0, second[i].1) << i;
    }
    let (_, data) = (0..32u32)
        .map(|d| {
            (
                (bch_format(d) ^ a)
                    .count_ones()
                    .min((bch_format(d) ^ b).count_ones()),
                d,
            )
        })
        .min()
        .filter(|(e, _)| *e <= 3)?;
    let level = [1, 0, 3, 2][(data >> 3) as usize];
    Some((level, data & 7))
}

fn read_grid(g: &[bool], dim: usize) -> Option<String> {
    let version = (dim - 17) / 4;
    let (level, mask) = format(g, dim)?;
    let func = function_modules(dim, version);
    let mut bits = Vec::new();
    let mut right = dim as i64 - 1;
    while right >= 1 {
        if right == 6 {
            right = 5;
        }
        for vert in 0..dim {
            for j in 0..2 {
                let x = (right - j) as usize;
                let up = ((right + 1) & 2) == 0;
                let y = if up { dim - 1 - vert } else { vert };
                if !func[y * dim + x] {
                    bits.push(g[y * dim + x] ^ masked(mask, x, y));
                }
            }
        }
        right -= 2;
    }
    let raw: Vec<u8> = bits
        .chunks_exact(8)
        .map(|c| c.iter().fold(0u8, |a, &b| (a << 1) | b as u8))
        .collect();
    let data = deinterleave(&raw, version, level)?;
    segments(&data, version)
}

fn deinterleave(raw: &[u8], version: usize, level: usize) -> Option<Vec<u8>> {
    let ec = EC_PER_BLOCK[version - 1][level] as usize;
    let (k1, n1, k2, n2) = BLOCKS[version - 1][level];
    let sizes: Vec<usize> = std::iter::repeat_n(k1 as usize, n1 as usize)
        .chain(std::iter::repeat_n(k2 as usize, n2 as usize))
        .collect();
    let total: usize = sizes.iter().map(|k| k + ec).sum();
    if raw.len() < total {
        return None;
    }
    let mut blocks: Vec<Vec<u8>> = sizes.iter().map(|k| Vec::with_capacity(k + ec)).collect();
    let mut it = raw.iter();
    for i in 0..sizes.iter().copied().max().unwrap_or(0) {
        for (b, &k) in sizes.iter().enumerate() {
            if i < k {
                blocks[b].push(*it.next()?);
            }
        }
    }
    for _ in 0..ec {
        for block in blocks.iter_mut() {
            block.push(*it.next()?);
        }
    }
    let mut data = Vec::new();
    for (mut block, k) in blocks.into_iter().zip(sizes) {
        correct(&mut block, ec)?;
        data.extend_from_slice(&block[..k]);
    }
    Some(data)
}

struct Gf {
    exp: [u8; 512],
    log: [u8; 256],
}

impl Gf {
    fn new() -> Gf {
        let mut gf = Gf {
            exp: [0; 512],
            log: [0; 256],
        };
        let mut v: u32 = 1;
        for i in 0..255 {
            gf.exp[i] = v as u8;
            gf.log[v as usize] = i as u8;
            v <<= 1;
            if v & 0x100 != 0 {
                v ^= 0x11d;
            }
        }
        for i in 255..512 {
            gf.exp[i] = gf.exp[i - 255];
        }
        gf
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
        }
    }

    fn div(&self, a: u8, b: u8) -> u8 {
        if a == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + 255 - self.log[b as usize] as usize]
        }
    }

    fn pow(&self, e: usize) -> u8 {
        self.exp[e % 255]
    }

    /// A polynomial with its lowest term first.
    fn eval(&self, p: &[u8], x: u8) -> u8 {
        p.iter().rev().fold(0, |acc, &c| self.mul(acc, x) ^ c)
    }
}

/// Reed-Solomon over GF(256) with the generator's roots α^0..α^(ec-1): syndromes,
/// Berlekamp-Massey, Chien search and Forney.
fn correct(block: &mut [u8], ec: usize) -> Option<()> {
    let gf = Gf::new();
    let n = block.len();
    let syndromes = |b: &[u8]| -> Vec<u8> {
        (0..ec)
            .map(|j| b.iter().fold(0u8, |acc, &c| gf.mul(acc, gf.pow(j)) ^ c))
            .collect()
    };
    let s = syndromes(&*block);
    if s.iter().all(|&v| v == 0) {
        return Some(());
    }
    let (mut c, mut b) = (vec![1u8], vec![1u8]);
    let (mut l, mut m, mut bb) = (0usize, 1usize, 1u8);
    for i in 0..ec {
        let mut d = s[i];
        for j in 1..=l.min(c.len() - 1) {
            d ^= gf.mul(c[j], s[i - j]);
        }
        if d == 0 {
            m += 1;
            continue;
        }
        let coef = gf.div(d, bb);
        let mut next = c.clone();
        next.resize(next.len().max(b.len() + m), 0);
        for (j, &v) in b.iter().enumerate() {
            next[j + m] ^= gf.mul(coef, v);
        }
        if 2 * l <= i {
            b = c;
            l = i + 1 - l;
            bb = d;
            m = 1;
        } else {
            m += 1;
        }
        c = next;
    }
    c.truncate(l + 1);
    if l == 0 || 2 * l > ec {
        return None;
    }
    let mut omega = vec![0u8; ec];
    for (i, &ci) in c.iter().enumerate() {
        for (j, &sj) in s.iter().enumerate() {
            if i + j < ec {
                omega[i + j] ^= gf.mul(ci, sj);
            }
        }
    }
    let derivative: Vec<u8> = c
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, &v)| if i % 2 == 1 { v } else { 0 })
        .collect();
    let mut found = 0;
    for p in 0..n {
        let inv = gf.pow(255 - p % 255);
        if gf.eval(&c, inv) != 0 {
            continue;
        }
        let x = gf.pow(p);
        let den = gf.eval(&derivative, inv);
        if den == 0 {
            return None;
        }
        let value = gf.mul(x, gf.div(gf.eval(&omega, inv), den));
        block[n - 1 - p] ^= value;
        found += 1;
    }
    (found == l && syndromes(&*block).iter().all(|&v| v == 0)).then_some(())
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn read(&mut self, n: usize) -> Option<u32> {
        if self.pos + n > self.data.len() * 8 {
            return None;
        }
        let mut v = 0u32;
        for _ in 0..n {
            let bit = (self.data[self.pos / 8] >> (7 - self.pos % 8)) & 1;
            v = (v << 1) | bit as u32;
            self.pos += 1;
        }
        Some(v)
    }

    fn left(&self) -> usize {
        self.data.len() * 8 - self.pos
    }
}

fn segments(data: &[u8], version: usize) -> Option<String> {
    let band = if version <= 9 {
        0
    } else if version <= 26 {
        1
    } else {
        2
    };
    let mut r = Reader { data, pos: 0 };
    let mut out: Vec<u8> = Vec::new();
    while r.left() >= 4 {
        match r.read(4)? {
            0 => break,
            1 => {
                let mut n = r.read([10, 12, 14][band])? as usize;
                while n >= 3 {
                    out.extend(format!("{:03}", r.read(10)?).bytes());
                    n -= 3;
                }
                match n {
                    2 => out.extend(format!("{:02}", r.read(7)?).bytes()),
                    1 => out.extend(format!("{}", r.read(4)?).bytes()),
                    _ => {}
                }
            }
            2 => {
                let mut n = r.read([9, 11, 13][band])? as usize;
                while n >= 2 {
                    let v = r.read(11)? as usize;
                    out.push(*ALNUM.get(v / 45)?);
                    out.push(*ALNUM.get(v % 45)?);
                    n -= 2;
                }
                if n == 1 {
                    out.push(*ALNUM.get(r.read(6)? as usize)?);
                }
            }
            4 => {
                let n = r.read([8, 16, 16][band])?;
                for _ in 0..n {
                    out.push(r.read(8)? as u8);
                }
            }
            7 => {
                let first = r.read(8)?;
                if first & 0x80 != 0 {
                    r.read(if first & 0x40 == 0 { 8 } else { 16 })?;
                }
            }
            3 => {
                r.read(16)?;
            }
            5 | 9 => {}
            _ => return None,
        }
    }
    Some(match String::from_utf8(out) {
        Ok(s) => s,
        Err(e) => e.into_bytes().into_iter().map(char::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrcode::{Color, EcLevel, QrCode};

    /// The code drawn `scale` px a module with a quiet zone, turned by `angle` radians.
    fn picture(text: &str, level: EcLevel, scale: f64, angle: f64) -> (Vec<u8>, usize) {
        let code = QrCode::with_error_correction_level(text, level).unwrap();
        let n = code.width();
        let colors = code.to_colors();
        let size = ((n + 8) as f64 * scale * 1.5) as usize;
        let mid = size as f64 / 2.0;
        let (sin, cos) = angle.sin_cos();
        let mut px = vec![230u8; size * size];
        for y in 0..size {
            for x in 0..size {
                let (dx, dy) = (x as f64 + 0.5 - mid, y as f64 + 0.5 - mid);
                let mx = (cos * dx + sin * dy) / scale + n as f64 / 2.0;
                let my = (-sin * dx + cos * dy) / scale + n as f64 / 2.0;
                if mx >= 0.0 && my >= 0.0 && (mx as usize) < n && (my as usize) < n {
                    if colors[my as usize * n + mx as usize] == Color::Dark {
                        px[y * size + x] = 25;
                    }
                }
            }
        }
        (px, size)
    }

    #[test]
    fn reads_codes_of_every_size() {
        let long = format!("https://example.com/sub?token={}", "a1b2c3d4".repeat(40));
        for (text, level, scale) in [
            ("https://e.co/s", EcLevel::L, 4.0),
            (
                "ss://YWVzLTEyOC1nY206cGFzcw@1.2.3.4:8388#node",
                EcLevel::M,
                5.0,
            ),
            (long.as_str(), EcLevel::M, 3.0),
            ("HELLO 123", EcLevel::H, 6.0),
        ] {
            let (px, size) = picture(text, level, scale, 0.0);
            assert_eq!(decode(&px, size, size).as_deref(), Some(text), "{text}");
        }
    }

    #[test]
    fn reads_a_turned_code() {
        let text = "vmess://eyJhZGQiOiIxLjIuMy40IiwicG9ydCI6NDQzfQ==";
        for angle in [0.3, -0.5, 1.2] {
            let (px, size) = picture(text, EcLevel::M, 6.0, angle);
            assert_eq!(decode(&px, size, size).as_deref(), Some(text), "{angle}");
        }
        let (px, size) = picture(text, EcLevel::M, 6.0, std::f64::consts::PI);
        assert_eq!(decode(&px, size, size).as_deref(), Some(text));
    }

    #[test]
    fn reads_a_code_turned_a_quarter() {
        let text = "trojan://pass@example.com:443?sni=example.com#tv";
        for quarter in [1.0, 3.0] {
            let angle = std::f64::consts::FRAC_PI_2 * quarter;
            let (px, size) = picture(text, EcLevel::Q, 5.0, angle);
            assert_eq!(decode(&px, size, size).as_deref(), Some(text), "{quarter}");
        }
    }

    #[test]
    fn reads_a_smudged_code() {
        let text = "https://example.com/subscribe?token=0123456789abcdef";
        let (mut px, size) = picture(text, EcLevel::H, 6.0, 0.2);
        let (x0, y0, side) = (size * 9 / 20, size * 9 / 20, size / 10);
        for y in y0..y0 + side {
            for x in x0..x0 + side {
                px[y * size + x] = if (x / 7 + y / 5) % 2 == 0 { 25 } else { 230 };
            }
        }
        assert_eq!(decode(&px, size, size).as_deref(), Some(text));
        let (mut px, size) = picture(text, EcLevel::L, 6.0, 0.0);
        for y in size / 4..size * 3 / 4 {
            for x in size / 4..size * 3 / 4 {
                px[y * size + x] = 25;
            }
        }
        assert_eq!(decode(&px, size, size), None);
    }

    #[test]
    fn corrects_damaged_codewords() {
        let gf = Gf::new();
        let data = b"https://example.com/subscribe".to_vec();
        let ec = 16;
        let mut generator = vec![1u8];
        for i in 0..ec {
            let mut next = vec![0u8; generator.len() + 1];
            for (j, &g) in generator.iter().enumerate() {
                next[j] ^= g;
                next[j + 1] ^= gf.mul(g, gf.pow(i));
            }
            generator = next;
        }
        let mut rem = data.clone();
        rem.resize(data.len() + ec, 0);
        for i in 0..data.len() {
            let f = rem[i];
            for (j, &g) in generator.iter().enumerate() {
                rem[i + j] ^= gf.mul(f, g);
            }
        }
        let mut block = data.clone();
        block.extend_from_slice(&rem[data.len()..]);
        let clean = block.clone();
        for i in [0, 5, 9, 20, 30, 40, 41, 44] {
            block[i] ^= 0x5a;
        }
        assert_eq!(correct(&mut block, ec), Some(()));
        assert_eq!(block, clean);
        block[1] ^= 1;
        for i in 2..12 {
            block[i] ^= 0x33;
        }
        assert_eq!(correct(&mut block, ec), None);
        assert_eq!(decode(&[0u8; 900], 30, 30), None);
    }
}

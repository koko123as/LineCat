// Copyright 2021 Google LLC
// Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0

//! HCT colour space and the dynamic scheme roles FlClash's `seededColorScheme` reads, ported
//! from prism_ui's lib/hct (itself trimmed from Google's material-color-utilities).
//! Only the roles the app's palette takes are kept.

use std::f64::consts::PI;
use std::sync::OnceLock;

const SRGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.41233895, 0.35762064, 0.18051042],
    [0.2126, 0.7152, 0.0722],
    [0.01932141, 0.11916382, 0.95034478],
];
const WHITE_POINT_D65: [f64; 3] = [95.047, 100.0, 108.883];
const SCALED_DISCOUNT_FROM_LINRGB: [[f64; 3]; 3] = [
    [
        0.001200833568784504,
        0.002389694492170889,
        0.0002795742885861124,
    ],
    [
        0.0005891086651375999,
        0.0029785502573438758,
        0.0003270666104008398,
    ],
    [
        0.00010146692491640572,
        0.0005364214359186694,
        0.0032979401770712076,
    ],
];
const LINRGB_FROM_SCALED_DISCOUNT: [[f64; 3]; 3] = [
    [1373.2198709594231, -1100.4251190754821, -7.278681089101213],
    [-271.815969077903, 559.6580465940733, -32.46047482791194],
    [1.9622899599665666, -57.173814538844006, 308.7233197812385],
];
const Y_FROM_LINRGB: [f64; 3] = [0.2126, 0.7152, 0.0722];

fn signum(x: f64) -> f64 {
    if x < 0.0 {
        -1.0
    } else if x == 0.0 {
        0.0
    } else {
        1.0
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    (1.0 - t) * a + t * b
}

pub fn sanitize_degrees(d: f64) -> f64 {
    let d = d % 360.0;
    if d < 0.0 {
        d + 360.0
    } else {
        d
    }
}

fn mat_mul(row: [f64; 3], m: &[[f64; 3]; 3]) -> [f64; 3] {
    [
        row[0] * m[0][0] + row[1] * m[0][1] + row[2] * m[0][2],
        row[0] * m[1][0] + row[1] * m[1][1] + row[2] * m[1][2],
        row[0] * m[2][0] + row[1] * m[2][1] + row[2] * m[2][2],
    ]
}

fn linearized(c: u32) -> f64 {
    let n = c as f64 / 255.0;
    if n <= 0.040449936 {
        n / 12.92 * 100.0
    } else {
        ((n + 0.055) / 1.055).powf(2.4) * 100.0
    }
}

fn true_delinearized(c: f64) -> f64 {
    let n = c / 100.0;
    let d = if n <= 0.0031308 {
        n * 12.92
    } else {
        1.055 * n.powf(1.0 / 2.4) - 0.055
    };
    d * 255.0
}

fn delinearized(c: f64) -> u32 {
    (true_delinearized(c).round() as i64).clamp(0, 255) as u32
}

fn argb_from_rgb(r: u32, g: u32, b: u32) -> u32 {
    0xff00_0000 | (r & 255) << 16 | (g & 255) << 8 | (b & 255)
}

fn argb_from_linrgb(l: [f64; 3]) -> u32 {
    argb_from_rgb(delinearized(l[0]), delinearized(l[1]), delinearized(l[2]))
}

fn lin_of(argb: u32) -> [f64; 3] {
    [
        linearized(argb >> 16 & 255),
        linearized(argb >> 8 & 255),
        linearized(argb & 255),
    ]
}

fn xyz_from_argb(argb: u32) -> [f64; 3] {
    mat_mul(lin_of(argb), &SRGB_TO_XYZ)
}

fn lab_f(t: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    if t > e {
        t.powf(1.0 / 3.0)
    } else {
        (kappa * t + 16.0) / 116.0
    }
}

fn lab_invf(ft: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    let ft3 = ft * ft * ft;
    if ft3 > e {
        ft3
    } else {
        (116.0 * ft - 16.0) / kappa
    }
}

pub fn y_from_lstar(lstar: f64) -> f64 {
    100.0 * lab_invf((lstar + 16.0) / 116.0)
}

fn lstar_from_y(y: f64) -> f64 {
    lab_f(y / 100.0) * 116.0 - 16.0
}

pub fn lstar_from_argb(argb: u32) -> f64 {
    116.0 * lab_f(xyz_from_argb(argb)[1] / 100.0) - 16.0
}

fn argb_from_lstar(lstar: f64) -> u32 {
    let c = delinearized(y_from_lstar(lstar));
    argb_from_rgb(c, c, c)
}

struct Viewing {
    n: f64,
    aw: f64,
    nbb: f64,
    ncb: f64,
    c: f64,
    nc: f64,
    rgb_d: [f64; 3],
    fl: f64,
    z: f64,
}

fn viewing() -> &'static Viewing {
    static V: OnceLock<Viewing> = OnceLock::new();
    V.get_or_init(|| {
        let w = WHITE_POINT_D65;
        let la = 200.0 / PI * y_from_lstar(50.0) / 100.0;
        let rw = w[0] * 0.401288 + w[1] * 0.650173 + w[2] * -0.051461;
        let gw = w[0] * -0.250268 + w[1] * 1.204414 + w[2] * 0.045854;
        let bw = w[0] * -0.002079 + w[1] * 0.048952 + w[2] * 0.953127;
        let f = 0.8 + 2.0 / 10.0;
        let c = if f >= 0.9 {
            lerp(0.59, 0.69, (f - 0.9) * 10.0)
        } else {
            lerp(0.525, 0.59, (f - 0.8) * 10.0)
        };
        let d = (f * (1.0 - (1.0 / 3.6) * ((-la - 42.0) / 92.0).exp())).clamp(0.0, 1.0);
        let rgb_d = [
            d * (100.0 / rw) + 1.0 - d,
            d * (100.0 / gw) + 1.0 - d,
            d * (100.0 / bw) + 1.0 - d,
        ];
        let k = 1.0 / (5.0 * la + 1.0);
        let k4 = k * k * k * k;
        let k4f = 1.0 - k4;
        let fl = k4 * la + 0.1 * k4f * k4f * (5.0 * la).powf(1.0 / 3.0);
        let n = y_from_lstar(50.0) / w[1];
        let z = 1.48 + n.sqrt();
        let nbb = 0.725 / n.powf(0.2);
        let af = [
            (fl * rgb_d[0] * rw / 100.0).powf(0.42),
            (fl * rgb_d[1] * gw / 100.0).powf(0.42),
            (fl * rgb_d[2] * bw / 100.0).powf(0.42),
        ];
        let a = af.map(|x| 400.0 * x / (x + 27.13));
        let aw = (40.0 * a[0] + 20.0 * a[1] + a[2]) / 20.0 * nbb;
        Viewing {
            n,
            aw,
            nbb,
            ncb: nbb,
            c,
            nc: f,
            rgb_d,
            fl,
            z,
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hct {
    argb: u32,
    pub hue: f64,
    pub chroma: f64,
    pub tone: f64,
}

impl Hct {
    pub fn from_argb(argb: u32) -> Hct {
        let argb = argb | 0xff00_0000;
        let (hue, chroma) = hue_and_chroma(argb);
        Hct {
            argb,
            hue,
            chroma,
            tone: lstar_from_argb(argb),
        }
    }

    pub fn from(hue: f64, chroma: f64, tone: f64) -> Hct {
        Hct::from_argb(solve(hue, chroma, tone))
    }

    pub fn argb(&self) -> u32 {
        self.argb
    }

    /// RGB without alpha.
    pub fn rgb(&self) -> u32 {
        self.argb & 0xff_ffff
    }
}

fn hue_and_chroma(argb: u32) -> (f64, f64) {
    let v = viewing();
    let [x, y, z] = xyz_from_argb(argb);
    let rc = 0.401288 * x + 0.650173 * y - 0.051461 * z;
    let gc = -0.250268 * x + 1.204414 * y + 0.045854 * z;
    let bc = -0.002079 * x + 0.048952 * y + 0.953127 * z;
    let adapt = |d: f64| {
        let af = (v.fl * d.abs() / 100.0).powf(0.42);
        signum(d) * 400.0 * af / (af + 27.13)
    };
    let ra = adapt(v.rgb_d[0] * rc);
    let ga = adapt(v.rgb_d[1] * gc);
    let ba = adapt(v.rgb_d[2] * bc);
    let a = (11.0 * ra + -12.0 * ga + ba) / 11.0;
    let b = (ra + ga - 2.0 * ba) / 9.0;
    let u = (20.0 * ra + 20.0 * ga + 21.0 * ba) / 20.0;
    let p2 = (40.0 * ra + 20.0 * ga + ba) / 20.0;
    let deg = b.atan2(a) * 180.0 / PI;
    let hue = if deg < 0.0 {
        deg + 360.0
    } else if deg >= 360.0 {
        deg - 360.0
    } else {
        deg
    };
    let ac = p2 * v.nbb;
    let j = 100.0 * (ac / v.aw).powf(v.c * v.z);
    let hue_prime = if hue < 20.14 { hue + 360.0 } else { hue };
    let e_hue = 0.25 * ((hue_prime * PI / 180.0 + 2.0).cos() + 3.8);
    let p1 = 50000.0 / 13.0 * e_hue * v.nc * v.ncb;
    let t = p1 * (a * a + b * b).sqrt() / (u + 0.305);
    let alpha = t.powf(0.9) * (1.64 - 0.29f64.powf(v.n)).powf(0.73);
    (hue, alpha * (j / 100.0).sqrt())
}

fn critical_plane(i: i32) -> f64 {
    linearized_f(i as f64 + 0.5)
}

fn linearized_f(c: f64) -> f64 {
    let n = c / 255.0;
    if n <= 0.040449936 {
        n / 12.92 * 100.0
    } else {
        ((n + 0.055) / 1.055).powf(2.4) * 100.0
    }
}

fn sanitize_radians(a: f64) -> f64 {
    (a + PI * 8.0) % (PI * 2.0)
}

fn chromatic_adaptation(c: f64) -> f64 {
    let af = c.abs().powf(0.42);
    signum(c) * 400.0 * af / (af + 27.13)
}

fn hue_of(linrgb: [f64; 3]) -> f64 {
    let sd = mat_mul(linrgb, &SCALED_DISCOUNT_FROM_LINRGB);
    let ra = chromatic_adaptation(sd[0]);
    let ga = chromatic_adaptation(sd[1]);
    let ba = chromatic_adaptation(sd[2]);
    let a = (11.0 * ra + -12.0 * ga + ba) / 11.0;
    let b = (ra + ga - 2.0 * ba) / 9.0;
    b.atan2(a)
}

fn in_cyclic_order(a: f64, b: f64, c: f64) -> bool {
    sanitize_radians(b - a) < sanitize_radians(c - a)
}

fn set_coordinate(source: [f64; 3], coordinate: f64, target: [f64; 3], axis: usize) -> [f64; 3] {
    let t = (coordinate - source[axis]) / (target[axis] - source[axis]);
    [
        source[0] + (target[0] - source[0]) * t,
        source[1] + (target[1] - source[1]) * t,
        source[2] + (target[2] - source[2]) * t,
    ]
}

fn nth_vertex(y: f64, n: i32) -> [f64; 3] {
    let [kr, kg, kb] = Y_FROM_LINRGB;
    let coord_a = if n % 4 <= 1 { 0.0 } else { 100.0 };
    let coord_b = if n % 2 == 0 { 0.0 } else { 100.0 };
    let bounded = |x: f64| (0.0..=100.0).contains(&x);
    let none = [-1.0; 3];
    if n < 4 {
        let (g, b) = (coord_a, coord_b);
        let r = (y - g * kg - b * kb) / kr;
        if bounded(r) {
            [r, g, b]
        } else {
            none
        }
    } else if n < 8 {
        let (b, r) = (coord_a, coord_b);
        let g = (y - r * kr - b * kb) / kg;
        if bounded(g) {
            [r, g, b]
        } else {
            none
        }
    } else {
        let (r, g) = (coord_a, coord_b);
        let b = (y - r * kr - g * kg) / kb;
        if bounded(b) {
            [r, g, b]
        } else {
            none
        }
    }
}

fn bisect_to_segment(y: f64, target: f64) -> ([f64; 3], [f64; 3]) {
    let mut left = [-1.0; 3];
    let mut right = left;
    let (mut left_hue, mut right_hue) = (0.0, 0.0);
    let mut initialized = false;
    let mut uncut = true;
    for n in 0..12 {
        let mid = nth_vertex(y, n);
        if mid[0] < 0.0 {
            continue;
        }
        let mid_hue = hue_of(mid);
        if !initialized {
            left = mid;
            right = mid;
            left_hue = mid_hue;
            right_hue = mid_hue;
            initialized = true;
            continue;
        }
        if uncut || in_cyclic_order(left_hue, mid_hue, right_hue) {
            uncut = false;
            if in_cyclic_order(left_hue, target, mid_hue) {
                right = mid;
                right_hue = mid_hue;
            } else {
                left = mid;
                left_hue = mid_hue;
            }
        }
    }
    (left, right)
}

fn bisect_to_limit(y: f64, target: f64) -> [f64; 3] {
    let (mut left, mut right) = bisect_to_segment(y, target);
    let mut left_hue = hue_of(left);
    for axis in 0..3 {
        if left[axis] == right[axis] {
            continue;
        }
        let (mut l_plane, mut r_plane);
        if left[axis] < right[axis] {
            l_plane = (true_delinearized(left[axis]) - 0.5).floor() as i32;
            r_plane = (true_delinearized(right[axis]) - 0.5).ceil() as i32;
        } else {
            l_plane = (true_delinearized(left[axis]) - 0.5).ceil() as i32;
            r_plane = (true_delinearized(right[axis]) - 0.5).floor() as i32;
        }
        for _ in 0..8 {
            if (r_plane - l_plane).abs() <= 1 {
                break;
            }
            let m_plane = ((l_plane + r_plane) as f64 / 2.0).floor() as i32;
            let mid = set_coordinate(left, critical_plane(m_plane), right, axis);
            let mid_hue = hue_of(mid);
            if in_cyclic_order(left_hue, target, mid_hue) {
                right = mid;
                r_plane = m_plane;
            } else {
                left = mid;
                left_hue = mid_hue;
                l_plane = m_plane;
            }
        }
    }
    [
        (left[0] + right[0]) / 2.0,
        (left[1] + right[1]) / 2.0,
        (left[2] + right[2]) / 2.0,
    ]
}

fn inverse_chromatic_adaptation(adapted: f64) -> f64 {
    let abs = adapted.abs();
    let base = (27.13 * abs / (400.0 - abs)).max(0.0);
    signum(adapted) * base.powf(1.0 / 0.42)
}

fn find_result_by_j(hue_radians: f64, chroma: f64, y: f64) -> u32 {
    let v = viewing();
    let mut j = y.sqrt() * 11.0;
    let t_inner = 1.0 / (1.64 - 0.29f64.powf(v.n)).powf(0.73);
    let e_hue = 0.25 * ((hue_radians + 2.0).cos() + 3.8);
    let p1 = e_hue * (50000.0 / 13.0) * v.nc * v.ncb;
    let (h_sin, h_cos) = (hue_radians.sin(), hue_radians.cos());
    for round in 0..5 {
        let jn = j / 100.0;
        let alpha = if chroma == 0.0 || j == 0.0 {
            0.0
        } else {
            chroma / jn.sqrt()
        };
        let t = (alpha * t_inner).powf(1.0 / 0.9);
        let ac = v.aw * jn.powf(1.0 / v.c / v.z);
        let p2 = ac / v.nbb;
        let gamma = 23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * h_cos + 108.0 * t * h_sin);
        let a = gamma * h_cos;
        let b = gamma * h_sin;
        let ra = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
        let ga = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
        let ba = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;
        let lin = mat_mul(
            [
                inverse_chromatic_adaptation(ra),
                inverse_chromatic_adaptation(ga),
                inverse_chromatic_adaptation(ba),
            ],
            &LINRGB_FROM_SCALED_DISCOUNT,
        );
        if lin.iter().any(|c| *c < 0.0) {
            return 0;
        }
        let fnj = Y_FROM_LINRGB[0] * lin[0] + Y_FROM_LINRGB[1] * lin[1] + Y_FROM_LINRGB[2] * lin[2];
        if fnj <= 0.0 {
            return 0;
        }
        if round == 4 || (fnj - y).abs() < 0.002 {
            if lin.iter().any(|c| *c > 100.01) {
                return 0;
            }
            return argb_from_linrgb(lin);
        }
        j -= (fnj - y) * j / (2.0 * fnj);
    }
    0
}

fn solve(hue: f64, chroma: f64, lstar: f64) -> u32 {
    if chroma < 0.0001 || !(0.0001..=99.9999).contains(&lstar) {
        return argb_from_lstar(lstar);
    }
    let hue_radians = sanitize_degrees(hue) / 180.0 * PI;
    let y = y_from_lstar(lstar);
    match find_result_by_j(hue_radians, chroma, y) {
        0 => argb_from_linrgb(bisect_to_limit(y, hue_radians)),
        exact => exact,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TonalPalette {
    pub hue: f64,
    pub chroma: f64,
}

impl TonalPalette {
    pub fn of(hue: f64, chroma: f64) -> TonalPalette {
        TonalPalette { hue, chroma }
    }

    pub fn get(&self, tone: f64) -> u32 {
        Hct::from(self.hue, self.chroma, tone).argb()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Variant {
    Monochrome,
    TonalSpot,
    Content,
}

#[derive(Clone, Copy, Debug)]
pub struct DynamicScheme {
    pub source: Hct,
    pub variant: Variant,
    pub dark: bool,
    pub primary: TonalPalette,
    pub tertiary: TonalPalette,
    pub neutral: TonalPalette,
    pub neutral_variant: TonalPalette,
    pub error: TonalPalette,
}

impl DynamicScheme {
    /// The palettes the app's roles read, as `SchemeContent`, `SchemeTonalSpot` and
    /// `SchemeMonochrome` build them.
    pub fn new(source: Hct, variant: Variant, dark: bool) -> DynamicScheme {
        let tertiary = match variant {
            Variant::Content => {
                let t = fix_if_disliked(analogous_tertiary(source));
                TonalPalette::of(t.hue, t.chroma)
            }
            Variant::TonalSpot => TonalPalette::of(sanitize_degrees(source.hue + 60.0), 24.0),
            Variant::Monochrome => TonalPalette::of(source.hue, 0.0),
        };
        let (primary, neutral, neutral_variant) = match variant {
            Variant::Content => (
                TonalPalette::of(source.hue, source.chroma),
                TonalPalette::of(source.hue, source.chroma / 8.0),
                TonalPalette::of(source.hue, source.chroma / 8.0 + 4.0),
            ),
            Variant::TonalSpot => (
                TonalPalette::of(source.hue, 36.0),
                TonalPalette::of(source.hue, 6.0),
                TonalPalette::of(source.hue, 8.0),
            ),
            Variant::Monochrome => (
                TonalPalette::of(source.hue, 0.0),
                TonalPalette::of(source.hue, 0.0),
                TonalPalette::of(source.hue, 0.0),
            ),
        };
        DynamicScheme {
            source,
            variant,
            dark,
            primary,
            tertiary,
            neutral,
            neutral_variant,
            error: TonalPalette::of(25.0, 84.0),
        }
    }
}

/// The `MaterialDynamicColors` roles the palette uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    Surface,
    SurfaceDim,
    SurfaceBright,
    SurfaceContainer,
    SurfaceContainerHigh,
    SurfaceContainerHighest,
    OnSurface,
    OnSurfaceVariant,
    OutlineVariant,
    Primary,
    PrimaryContainer,
    OnPrimary,
    Tertiary,
    TertiaryContainer,
    Error,
    ErrorContainer,
}

/// `ToneDeltaPair` with `TonePolarity.nearer` and `stayTogether: false`, the only kind these
/// roles use: (container, accent).
struct Pair(Role, Role);

impl Role {
    pub fn palette(self, s: &DynamicScheme) -> TonalPalette {
        use Role::*;
        match self {
            Surface
            | SurfaceDim
            | SurfaceBright
            | SurfaceContainer
            | SurfaceContainerHigh
            | SurfaceContainerHighest
            | OnSurface => s.neutral,
            OnSurfaceVariant | OutlineVariant => s.neutral_variant,
            Primary | PrimaryContainer | OnPrimary => s.primary,
            Tertiary | TertiaryContainer => s.tertiary,
            Error | ErrorContainer => s.error,
        }
    }

    fn base_tone(self, s: &DynamicScheme) -> f64 {
        use Role::*;
        let d = s.dark;
        let mono = s.variant == Variant::Monochrome;
        let pick = |dark: f64, light: f64| if d { dark } else { light };
        match self {
            Surface => pick(6.0, 98.0),
            SurfaceDim => pick(6.0, 87.0),
            SurfaceBright => pick(24.0, 98.0),
            SurfaceContainer => pick(12.0, 94.0),
            SurfaceContainerHigh => pick(17.0, 92.0),
            SurfaceContainerHighest => pick(22.0, 90.0),
            OnSurface => pick(90.0, 10.0),
            OnSurfaceVariant => pick(80.0, 30.0),
            OutlineVariant => pick(30.0, 80.0),
            Primary if mono => pick(100.0, 0.0),
            Primary => pick(80.0, 40.0),
            OnPrimary if mono => pick(10.0, 90.0),
            OnPrimary => pick(20.0, 100.0),
            PrimaryContainer if s.variant == Variant::Content => s.source.tone,
            PrimaryContainer if mono => pick(85.0, 25.0),
            PrimaryContainer => pick(30.0, 90.0),
            Tertiary if mono => pick(90.0, 25.0),
            Tertiary => pick(80.0, 40.0),
            TertiaryContainer if mono => pick(60.0, 49.0),
            TertiaryContainer if s.variant == Variant::Content => {
                let p = s.tertiary;
                fix_if_disliked(Hct::from(p.hue, p.chroma, s.source.tone)).tone
            }
            TertiaryContainer => pick(30.0, 90.0),
            Error => pick(80.0, 40.0),
            ErrorContainer => pick(30.0, 90.0),
        }
    }

    fn is_background(self) -> bool {
        use Role::*;
        !matches!(
            self,
            OnSurface | OnSurfaceVariant | OutlineVariant | OnPrimary
        )
    }

    fn background(self, s: &DynamicScheme) -> Option<Role> {
        use Role::*;
        let highest = if s.dark { SurfaceBright } else { SurfaceDim };
        match self {
            Surface
            | SurfaceDim
            | SurfaceBright
            | SurfaceContainer
            | SurfaceContainerHigh
            | SurfaceContainerHighest => None,
            OnPrimary => Some(Primary),
            _ => Some(highest),
        }
    }

    fn contrast(self) -> f64 {
        use Role::*;
        match self {
            OnSurface | OnPrimary => 7.0,
            OnSurfaceVariant | Primary | Tertiary | Error => 4.5,
            _ => 1.0,
        }
    }

    fn pair(self) -> Option<Pair> {
        use Role::*;
        match self {
            Primary | PrimaryContainer => Some(Pair(PrimaryContainer, Primary)),
            Tertiary | TertiaryContainer => Some(Pair(TertiaryContainer, Tertiary)),
            Error | ErrorContainer => Some(Pair(ErrorContainer, Error)),
            _ => None,
        }
    }

    pub fn tone(self, s: &DynamicScheme) -> f64 {
        if let Some(Pair(nearer, farther)) = self.pair() {
            let delta = 10.0;
            let bg = self.background(s).map(|b| b.tone(s)).unwrap_or(0.0);
            let dir = if s.dark { 1.0 } else { -1.0 };
            let fit = |r: Role| {
                let t = r.base_tone(s);
                if ratio_of_tones(bg, t) >= r.contrast() {
                    t
                } else {
                    foreground_tone(bg, r.contrast())
                }
            };
            let mut n = fit(nearer);
            let mut f = fit(farther);
            if (f - n) * dir < delta {
                f = (n + delta * dir).clamp(0.0, 100.0);
                if (f - n) * dir < delta {
                    n = (f - delta * dir).clamp(0.0, 100.0);
                }
            }
            if (50.0..60.0).contains(&n) {
                if dir > 0.0 {
                    n = 60.0;
                    f = f.max(n + delta * dir);
                } else {
                    n = 49.0;
                    f = f.min(n + delta * dir);
                }
            } else if (50.0..60.0).contains(&f) {
                f = if dir > 0.0 { 60.0 } else { 49.0 };
            }
            return if self == nearer { n } else { f };
        }
        let mut answer = self.base_tone(s);
        let Some(bg) = self.background(s) else {
            return answer;
        };
        let bg_tone = bg.tone(s);
        let ratio = self.contrast();
        if ratio_of_tones(bg_tone, answer) < ratio {
            answer = foreground_tone(bg_tone, ratio);
        }
        if self.is_background() && (50.0..60.0).contains(&answer) {
            answer = if ratio_of_tones(49.0, bg_tone) >= ratio {
                49.0
            } else {
                60.0
            };
        }
        answer
    }

    pub fn argb(self, s: &DynamicScheme) -> u32 {
        self.palette(s).get(self.tone(s))
    }
}

/// `DislikeAnalyzer.fixIfDisliked`: dark yellow-greens are lifted to tone 70.
fn fix_if_disliked(h: Hct) -> Hct {
    let hue = h.hue.round();
    if (90.0..=111.0).contains(&hue) && h.chroma.round() > 16.0 && h.tone.round() < 65.0 {
        Hct::from(h.hue, h.chroma, 70.0)
    } else {
        h
    }
}

fn lab_from_argb(argb: u32) -> [f64; 3] {
    let xyz = xyz_from_argb(argb);
    let [fx, fy, fz] = [0, 1, 2].map(|i| lab_f(xyz[i] / WHITE_POINT_D65[i]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// `TemperatureCache.rawTemperature`.
fn raw_temperature(h: &Hct) -> f64 {
    let [_, a, b] = lab_from_argb(h.argb());
    let hue = sanitize_degrees(b.atan2(a).to_degrees());
    let chroma = a.hypot(b);
    -0.5 + 0.02 * chroma.powf(1.07) * (sanitize_degrees(hue - 50.0).to_radians()).cos()
}

/// `TemperatureCache(source).analogous(count: 3, divisions: 6).last`, the content
/// scheme's tertiary source.
fn analogous_tertiary(input: Hct) -> Hct {
    const DIVISIONS: usize = 6;
    let by_hue: Vec<Hct> = (0..=360)
        .map(|hue| Hct::from(hue as f64, input.chroma, input.tone))
        .collect();
    let temps: Vec<f64> = by_hue.iter().map(raw_temperature).collect();
    let input_temp = raw_temperature(&input);
    let coldest = temps.iter().copied().fold(input_temp, f64::min);
    let warmest = temps.iter().copied().fold(input_temp, f64::max);
    let range = warmest - coldest;
    let relative = |i: usize| {
        if range == 0.0 {
            0.5
        } else {
            (temps[i] - coldest) / range
        }
    };
    let start = input.hue.round() as usize;
    let at = |offset: usize| (start + offset) % 360;
    let mut all = vec![start];
    let mut total_delta = 0.0;
    let mut last = relative(start);
    for i in 0..360 {
        let t = relative(at(i));
        total_delta += (t - last).abs();
        last = t;
    }
    let step = total_delta / DIVISIONS as f64;
    let mut walked = 0.0;
    last = relative(start);
    let mut addend = 1;
    while all.len() < DIVISIONS {
        let hue = at(addend);
        let t = relative(hue);
        walked += (t - last).abs();
        let mut satisfied = walked >= all.len() as f64 * step;
        let mut index_addend = 1;
        while satisfied && all.len() < DIVISIONS {
            all.push(hue);
            satisfied = walked >= (all.len() + index_addend) as f64 * step;
            index_addend += 1;
        }
        last = t;
        addend += 1;
        if addend > 360 {
            while all.len() < DIVISIONS {
                all.push(hue);
            }
            break;
        }
    }
    by_hue[all[1]]
}

fn ratio_of_ys(y1: f64, y2: f64) -> f64 {
    let lighter = if y1 > y2 { y1 } else { y2 };
    let darker = if lighter == y2 { y1 } else { y2 };
    (lighter + 5.0) / (darker + 5.0)
}

fn ratio_of_tones(a: f64, b: f64) -> f64 {
    ratio_of_ys(
        y_from_lstar(a.clamp(0.0, 100.0)),
        y_from_lstar(b.clamp(0.0, 100.0)),
    )
}

fn lighter(tone: f64, ratio: f64) -> f64 {
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }
    let dark_y = y_from_lstar(tone);
    let light_y = ratio * (dark_y + 5.0) - 5.0;
    let real = ratio_of_ys(light_y, dark_y);
    if real < ratio && (real - ratio).abs() > 0.04 {
        return -1.0;
    }
    let v = lstar_from_y(light_y) + 0.4;
    if !(0.0..=100.0).contains(&v) {
        -1.0
    } else {
        v
    }
}

fn darker(tone: f64, ratio: f64) -> f64 {
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }
    let light_y = y_from_lstar(tone);
    let dark_y = (light_y + 5.0) / ratio - 5.0;
    let real = ratio_of_ys(light_y, dark_y);
    if real < ratio && (real - ratio).abs() > 0.04 {
        return -1.0;
    }
    let v = lstar_from_y(dark_y) - 0.4;
    if !(0.0..=100.0).contains(&v) {
        -1.0
    } else {
        v
    }
}

fn foreground_tone(bg: f64, ratio: f64) -> f64 {
    let lighter_tone = match lighter(bg, ratio) {
        t if t < 0.0 => 100.0,
        t => t,
    };
    let darker_tone = match darker(bg, ratio) {
        t if t < 0.0 => 0.0,
        t => t,
    };
    let lighter_ratio = ratio_of_tones(lighter_tone, bg);
    let darker_ratio = ratio_of_tones(darker_tone, bg);
    if bg.round() < 60.0 {
        let negligible = (lighter_ratio - darker_ratio).abs() < 0.1
            && lighter_ratio < ratio
            && darker_ratio < ratio;
        if lighter_ratio >= ratio || lighter_ratio >= darker_ratio || negligible {
            lighter_tone
        } else {
            darker_tone
        }
    } else if darker_ratio >= ratio || darker_ratio >= lighter_ratio {
        darker_tone
    } else {
        lighter_tone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hct_round_trips_srgb() {
        for argb in [0xff007aff, 0xffff2d55, 0xff8e8e93, 0xff000000, 0xffffffff] {
            let h = Hct::from_argb(argb);
            assert_eq!(Hct::from(h.hue, h.chroma, h.tone).argb(), argb, "{argb:x}");
        }
    }

    #[test]
    fn hct_matches_material_reference_values() {
        let blue = Hct::from_argb(0xff0000ff);
        assert!((blue.hue - 282.788).abs() < 0.01, "{}", blue.hue);
        assert!((blue.chroma - 87.230).abs() < 0.01, "{}", blue.chroma);
        assert!((blue.tone - 32.302).abs() < 0.01, "{}", blue.tone);
    }

    #[test]
    fn tertiary_matches_material_color_utilities() {
        for (seed, dark, content, spot, mono) in [
            (0xff007aff, false, 0x8e2cb1, 0x715573, 0x3b3b3b),
            (0xff007aff, true, 0xeeb1ff, 0xdebcdf, 0xe2e2e2),
            (0xffff8d28, false, 0x5c6300, 0x5e6135, 0x3b3b3b),
            (0xffffcc00, true, 0xdcfc63, 0xadcfae, 0xe2e2e2),
            (0xff34c759, false, 0x006495, 0x39656c, 0x3b3b3b),
            (0xff8e8e93, true, 0xcec4c8, 0xdebcdf, 0xe2e2e2),
            (0xffb0a020, false, 0x3a6a16, 0x416652, 0x3b3b3b),
        ] {
            let source = Hct::from_argb(seed);
            for (variant, want) in [
                (Variant::Content, content),
                (Variant::TonalSpot, spot),
                (Variant::Monochrome, mono),
            ] {
                let s = DynamicScheme::new(source, variant, dark);
                let got = Role::Tertiary.argb(&s) & 0xff_ffff;
                assert_eq!(got, want, "{seed:x} {dark} {variant:?}");
            }
        }
    }

    #[test]
    fn out_of_gamut_chroma_is_clipped_to_the_hue() {
        let h = Hct::from(120.0, 200.0, 50.0);
        assert!((h.hue - 120.0).abs() < 2.0 && h.chroma < 200.0);
    }
}

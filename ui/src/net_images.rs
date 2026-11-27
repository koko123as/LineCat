//! Pictures behind an http url (`CommonTargetIcon`'s network image): downloaded once to a
//! disk cache, decoded as PNG and shrunk to an icon's size, kept here by url, `None` once
//! a url turned out to hold none. A base64 `data:` url is decoded in place.

use crate::app_icons::{decode_png, Pixels};
use slint::{Rgba8Pixel, SharedPixelBuffer};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// Large enough for a 56px plate at 3x density.
pub const MAX_SIDE: u32 = 144;
const MAX_BYTES: u64 = 4 << 20;

#[derive(Default)]
pub struct Cache {
    images: HashMap<String, Option<Pixels>>,
    pending: Vec<String>,
    queued: HashSet<String>,
}

impl Cache {
    pub fn lookup(&mut self, url: &str) -> Option<Pixels> {
        if is_data_url(url) {
            return self
                .images
                .entry(url.to_string())
                .or_insert_with(|| decode_data_url(url))
                .clone();
        }
        if !crate::icon_sets::is_http_url(url) {
            return None;
        }
        match self.images.get(url) {
            Some(image) => image.clone(),
            None => {
                if self.queued.insert(url.to_string()) {
                    self.pending.push(url.to_string());
                }
                None
            }
        }
    }

    /// The oldest asks first, so a page fills from the top.
    pub fn take_pending(&mut self, limit: usize) -> Vec<String> {
        let n = limit.min(self.pending.len());
        self.pending.drain(..n).collect()
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn put(&mut self, url: String, image: Option<Pixels>) {
        self.queued.remove(&url);
        self.images.insert(url, image);
    }
}

pub fn cache() -> &'static Mutex<Cache> {
    static CACHE: std::sync::OnceLock<Mutex<Cache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub fn is_data_url(url: &str) -> bool {
    url.starts_with("data:")
}

/// What `CommonTargetIcon` can draw: an http url or a data url.
pub fn is_image_src(src: &str) -> bool {
    is_data_url(src) || crate::icon_sets::is_http_url(src)
}

/// `getBase64`: the payload after `base64,`, read as PNG (the only format decoded here).
pub fn decode_data_url(url: &str) -> Option<Pixels> {
    let (_, payload) = url.split_once("base64,")?;
    decode_png(&base64(payload)?).map(|p| shrink(&p, MAX_SIDE))
}

/// Standard or url-safe alphabet, padding and whitespace ignored.
pub fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

pub fn file_name(url: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The picture from the disk cache, else downloaded into it.
pub fn load(dir: &Path, url: &str) -> Option<Pixels> {
    let path = dir.join(file_name(url));
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            let b = download(url)?;
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&path, &b);
            b
        }
    };
    decode_png(&bytes).map(|p| shrink(&p, MAX_SIDE))
}

fn download(url: &str) -> Option<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", crate::profile::USER_AGENT)
        .call()
        .ok()?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .ok()
}

/// Box-averages a picture down so its longer side is at most `side`.
pub fn shrink(p: &Pixels, side: u32) -> Pixels {
    let (w, h) = (p.width(), p.height());
    let longer = w.max(h);
    if longer <= side || w == 0 || h == 0 {
        return p.clone();
    }
    let (nw, nh) = (
        (w * side / longer).max(1) as usize,
        (h * side / longer).max(1) as usize,
    );
    let (w, h) = (w as usize, h as usize);
    let src = p.as_slice();
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(nw as u32, nh as u32);
    let dst = out.make_mut_slice();
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for px in &src[sy * w + x0..sy * w + x1] {
                    let a = px.a as u32;
                    sum[0] += px.r as u32 * a;
                    sum[1] += px.g as u32 * a;
                    sum[2] += px.b as u32 * a;
                    sum[3] += a;
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            dst[y * nw + x] = if sum[3] == 0 {
                Rgba8Pixel::new(0, 0, 0, 0)
            } else {
                Rgba8Pixel::new(
                    (sum[0] / sum[3]) as u8,
                    (sum[1] / sum[3]) as u8,
                    (sum[2] / sum[3]) as u8,
                    (sum[3] / n) as u8,
                )
            };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_wait_once_in_order() {
        let mut c = Cache::default();
        assert!(c.lookup("https://h.example/a.png").is_none());
        assert!(c.lookup("https://h.example/b.png").is_none());
        assert!(c.lookup("https://h.example/a.png").is_none());
        assert!(c.lookup("not a url").is_none());
        assert_eq!(
            c.take_pending(1),
            vec!["https://h.example/a.png".to_string()]
        );
        c.put("https://h.example/a.png".into(), None);
        assert!(c.lookup("https://h.example/a.png").is_none());
        assert_eq!(c.take_pending(9).len(), 1);
        assert!(!c.has_pending());
    }

    #[test]
    fn data_urls_decode_without_waiting() {
        assert_eq!(base64("aGk=").unwrap(), b"hi");
        assert_eq!(base64("aGVs bG8").unwrap(), b"hello");
        assert_eq!(base64("_-8").unwrap(), [0xff, 0xef]);
        assert!(base64("a*b").is_none());
        let mut png = Vec::new();
        {
            let mut e = ::png::Encoder::new(&mut png, 2, 1);
            e.set_color(::png::ColorType::Rgba);
            e.set_depth(::png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[1, 2, 3, 255, 4, 5, 6, 255]).unwrap();
        }
        let url = format!("data:image/png;base64,{}", encode(&png));
        let mut c = Cache::default();
        let p = c.lookup(&url).unwrap();
        assert_eq!((p.width(), p.height()), (2, 1));
        assert!(!c.has_pending());
        assert!(c.lookup("data:image/svg+xml;base64,PHN2Zz4=").is_none());
        assert!(is_image_src(&url) && is_image_src("https://h.example/a.png"));
        assert!(!is_image_src("a.png"));
    }

    fn encode(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().fold(0u32, |a, b| (a << 8) | *b as u32) << (8 * (3 - chunk.len()));
            for i in 0..=chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        out
    }

    #[test]
    fn shrinks_by_the_longer_side_keeping_colour() {
        let mut p = SharedPixelBuffer::<Rgba8Pixel>::new(400, 200);
        for px in p.make_mut_slice() {
            *px = Rgba8Pixel::new(10, 20, 30, 255);
        }
        let s = shrink(&p, 100);
        assert_eq!((s.width(), s.height()), (100, 50));
        assert_eq!(s.as_slice()[0], Rgba8Pixel::new(10, 20, 30, 255));
        let small = SharedPixelBuffer::<Rgba8Pixel>::new(20, 20);
        assert_eq!(shrink(&small, 100).width(), 20);
    }

    #[test]
    fn transparent_pixels_do_not_darken_edges() {
        let mut p = SharedPixelBuffer::<Rgba8Pixel>::new(2, 1);
        p.make_mut_slice()[0] = Rgba8Pixel::new(200, 0, 0, 255);
        let s = shrink(&p, 1);
        assert_eq!(s.as_slice()[0], Rgba8Pixel::new(200, 0, 0, 127));
    }
}

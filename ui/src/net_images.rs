//! Pictures behind an http url (`CommonTargetIcon`'s network image): downloaded once to a
//! disk cache, decoded as PNG and shrunk to an icon's size, kept here by url, `None` once
//! a url turned out to hold none.

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

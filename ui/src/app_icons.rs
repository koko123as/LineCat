//! Installed apps' icons (`getPackageIcon`): the shell writes each as a PNG, kept here
//! decoded by package name, `None` once a package turned out to have none.

use slint::{Rgba8Pixel, SharedPixelBuffer};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

pub type Pixels = SharedPixelBuffer<Rgba8Pixel>;

#[derive(Default)]
pub struct Cache {
    icons: HashMap<String, Option<Pixels>>,
    pending: HashSet<String>,
}

impl Cache {
    /// The icon when known; an unknown package name waits for the next fetch.
    pub fn lookup(&mut self, name: &str) -> Option<Pixels> {
        if !is_package(name) {
            return None;
        }
        match self.icons.get(name) {
            Some(icon) => icon.clone(),
            None => {
                self.pending.insert(name.to_string());
                None
            }
        }
    }

    pub fn take_pending(&mut self, limit: usize) -> Vec<String> {
        let names: Vec<String> = self.pending.iter().take(limit).cloned().collect();
        for n in &names {
            self.pending.remove(n);
        }
        names
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn put(&mut self, name: String, icon: Option<Pixels>) {
        self.icons.insert(name, icon);
    }
}

pub fn cache() -> &'static Mutex<Cache> {
    static CACHE: std::sync::OnceLock<Mutex<Cache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// A process the Core names by its package: dotted, no path.
pub fn is_package(name: &str) -> bool {
    name.contains('.') && !name.contains('/') && !name.starts_with('.')
}

/// `decode_png` for a picture of at most `max_pixels`, which its header tells before any
/// pixel is decoded.
pub fn decode_png_within(bytes: &[u8], max_pixels: u64) -> Option<Pixels> {
    let reader = png::Decoder::new(bytes).read_info().ok()?;
    let info = reader.info();
    if info.width as u64 * info.height as u64 > max_pixels {
        return None;
    }
    decode_png(bytes)
}

pub fn decode_png(bytes: &[u8]) -> Option<Pixels> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(SharedPixelBuffer::clone_from_slice(
        &rgba,
        info.width,
        info.height,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(color: png::ColorType, data: &[u8], w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[test]
    fn decodes_rgba_and_rgb() {
        let rgba = encode(png::ColorType::Rgba, &[1, 2, 3, 4, 5, 6, 7, 8], 2, 1);
        let p = decode_png(&rgba).unwrap();
        assert_eq!((p.width(), p.height()), (2, 1));
        assert_eq!(p.as_slice()[1], Rgba8Pixel::new(5, 6, 7, 8));
        let rgb = encode(png::ColorType::Rgb, &[9, 8, 7], 1, 1);
        assert_eq!(
            decode_png(&rgb).unwrap().as_slice()[0],
            Rgba8Pixel::new(9, 8, 7, 255)
        );
        assert!(decode_png(b"not a png").is_none());
        assert!(decode_png_within(&rgba, 2).is_some());
        assert!(decode_png_within(&rgba, 1).is_none());
    }

    #[test]
    fn unknown_packages_wait_for_a_fetch() {
        let mut c = Cache::default();
        assert!(c.lookup("com.example.app").is_none());
        assert!(c.lookup("/system/bin/sh").is_none());
        assert!(c.has_pending());
        assert_eq!(c.take_pending(10), vec!["com.example.app".to_string()]);
        c.put("com.example.app".into(), None);
        assert!(c.lookup("com.example.app").is_none());
        assert!(!c.has_pending());
    }
}

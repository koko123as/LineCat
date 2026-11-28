//! Pictures behind an http url (`CommonTargetIcon`'s network image): downloaded once to a
//! disk cache, decoded (PNG, JPEG, SVG) and shrunk to an icon's size, kept here by url, `None`
//! once a url turned out to hold none. A `data:` url is decoded in place.

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Svg,
}

/// The format the bytes themselves say, whatever the url or the server called them.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Format::Png);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(Format::Jpeg);
    }
    let head = &bytes[..bytes.len().min(1024)];
    let head = head.strip_prefix(b"\xef\xbb\xbf").unwrap_or(head);
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start();
    let prolog = ["<?xml", "<!--", "<!DOCTYPE", "<!doctype"];
    if text.starts_with("<svg")
        || (prolog.iter().any(|p| text.starts_with(p)) && text.contains("<svg"))
    {
        return Some(Format::Svg);
    }
    None
}

fn format_of_mime(mime: &str) -> Option<Format> {
    match mime {
        "image/png" => Some(Format::Png),
        "image/jpeg" | "image/jpg" | "image/pjpeg" => Some(Format::Jpeg),
        "image/svg+xml" => Some(Format::Svg),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DataUrl {
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// `data:[<mime>][;<param>]*[;base64],<payload>`; a payload without `base64` is
/// percent-encoded (or plain) text.
pub fn parse_data_url(url: &str) -> Option<DataUrl> {
    let (meta, payload) = url.strip_prefix("data:")?.split_once(',')?;
    let mut params = meta.split(';');
    let mime = params
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let encoded = params.any(|p| p.trim().eq_ignore_ascii_case("base64"));
    let raw = percent_decode(payload);
    let bytes = if encoded {
        base64(std::str::from_utf8(&raw).ok()?)?
    } else {
        raw
    };
    (!bytes.is_empty()).then_some(DataUrl { mime, bytes })
}

pub fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(&h), Some(&l)) if hex(h).is_some() && hex(l).is_some() => {
                out.push((hex(h).unwrap() * 16 + hex(l).unwrap()) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

pub fn decode_data_url(url: &str) -> Option<Pixels> {
    let data = parse_data_url(url)?;
    let format = sniff(&data.bytes).or_else(|| format_of_mime(&data.mime))?;
    decode_as(&data.bytes, format)
}

/// Downloaded bytes, by what they hold.
pub fn decode(bytes: &[u8]) -> Option<Pixels> {
    decode_as(bytes, sniff(bytes)?)
}

fn decode_as(bytes: &[u8], format: Format) -> Option<Pixels> {
    match format {
        Format::Png => decode_png(bytes).map(|p| shrink(&p, MAX_SIDE)),
        Format::Jpeg => decode_jpeg(bytes).map(|p| shrink(&p, MAX_SIDE)),
        Format::Svg => render_svg(bytes, MAX_SIDE),
    }
}

/// A photo too large to decode within `max_alloc` is refused rather than shrunk.
pub fn decode_jpeg(bytes: &[u8]) -> Option<Pixels> {
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 << 20);
    reader.limits(limits);
    let rgba = reader.decode().ok()?.into_rgba8();
    let (w, h) = rgba.dimensions();
    let raw = rgba.into_raw();
    Some(SharedPixelBuffer::clone_from_slice(&raw, w, h))
}

/// Drawn so its longer side is `side`, however small its own size; images it names by
/// path stay out, only those inside it as data urls are drawn.
pub fn render_svg(bytes: &[u8], side: u32) -> Option<Pixels> {
    use resvg::{tiny_skia, usvg};
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_: &str, _: &usvg::Options| -> Option<usvg::ImageKind> {
                None
            }),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options).ok()?;
    let (w, h) = (tree.size().width(), tree.size().height());
    if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
        return None;
    }
    let scale = side as f32 / w.max(h);
    let pw = ((w * scale).round() as u32).clamp(1, side);
    let ph = ((h * scale).round() as u32).clamp(1, side);
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(pw, ph);
    for (dst, src) in out.make_mut_slice().iter_mut().zip(pixmap.pixels()) {
        let c = src.demultiply();
        *dst = Rgba8Pixel::new(c.red(), c.green(), c.blue(), c.alpha());
    }
    Some(out)
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
    decode(&bytes)
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
        assert!(c.lookup("data:image/svg+xml;base64,PGh0bWw+").is_none());
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

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="5"><rect width="10" height="5" fill="#ff0000"/></svg>"##;

    #[test]
    fn formats_are_told_by_their_bytes() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), Some(Format::Png));
        assert_eq!(sniff(SVG.as_bytes()), Some(Format::Svg));
        assert_eq!(
            sniff(b"\xef\xbb\xbf\n  <?xml version=\"1.0\"?>\n<!-- x -->\n<svg/>"),
            Some(Format::Svg)
        );
        assert_eq!(sniff(b"<!DOCTYPE svg><svg/>"), Some(Format::Svg));
        assert_eq!(sniff(b"<?xml version=\"1.0\"?><html/>"), None);
        assert_eq!(sniff(b"<html><svg/></html>"), None);
        assert_eq!(sniff(b""), None);
    }

    fn jpeg(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let data: Vec<u8> = (0..w * h).flat_map(|_| rgb).collect();
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
            .encode(&data, w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        out
    }

    #[test]
    fn jpeg_decodes_from_bytes_and_data_urls() {
        let bytes = jpeg(16, 8, [0, 0, 255]);
        assert_eq!(sniff(&bytes), Some(Format::Jpeg));
        let p = decode(&bytes).unwrap();
        assert_eq!((p.width(), p.height()), (16, 8));
        let px = p.as_slice()[0];
        assert!(
            px.b > 230 && px.r < 25 && px.g < 25 && px.a == 255,
            "{px:?}"
        );
        let big = decode(&jpeg(300, 150, [9, 9, 9])).unwrap();
        assert_eq!((big.width(), big.height()), (MAX_SIDE, MAX_SIDE / 2));
        for mime in ["image/jpeg", "image/jpg", "application/octet-stream"] {
            let url = format!("data:{mime};base64,{}", encode(&bytes));
            let p = decode_data_url(&url).unwrap_or_else(|| panic!("{mime}"));
            assert_eq!(p.width(), 16);
        }
        assert!(decode(&[0xff, 0xd8, 0xff, 0xe0, 0, 0]).is_none());
    }

    #[test]
    fn data_urls_read_base64_percent_and_plain_payloads() {
        let plain = parse_data_url("data:image/svg+xml;utf8,<svg a='#1'/>").unwrap();
        assert_eq!(plain.mime, "image/svg+xml");
        assert_eq!(plain.bytes, b"<svg a='#1'/>");
        let pct = parse_data_url("data:image/svg+xml,%3Csvg%20a%3D%271%27%2F%3E").unwrap();
        assert_eq!(pct.bytes, b"<svg a='1'/>");
        let b64 = parse_data_url("data:Image/SVG+XML;charset=utf-8;BASE64,PHN2Zy8+").unwrap();
        assert_eq!(b64.mime, "image/svg+xml");
        assert_eq!(b64.bytes, b"<svg/>");
        assert_eq!(percent_decode("a%2"), b"a%2");
        assert_eq!(percent_decode("%zz%41"), b"%zzA");
        assert!(parse_data_url("data:image/png;base64,").is_none());
        assert!(parse_data_url("data:image/png;base64,a*b").is_none());
        assert!(parse_data_url("image/png;base64,aGk=").is_none());
        assert!(parse_data_url("data:text/plain").is_none());
    }

    #[test]
    fn svg_draws_at_the_icon_size() {
        let p = render_svg(SVG.as_bytes(), MAX_SIDE).unwrap();
        assert_eq!((p.width(), p.height()), (MAX_SIDE, MAX_SIDE / 2));
        assert_eq!(p.as_slice()[0], Rgba8Pixel::new(255, 0, 0, 255));
        let p = decode(SVG.as_bytes()).unwrap();
        assert_eq!(p.width(), MAX_SIDE);
        assert!(render_svg(b"<svg", MAX_SIDE).is_none());
        let external = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="4" height="4"><image width="4" height="4" xlink:href="/etc/hostname"/></svg>"#;
        let p = render_svg(external.as_bytes(), 8).unwrap();
        assert!(p.as_slice().iter().all(|px| px.a == 0));
    }

    #[test]
    fn svg_data_urls_decode_in_any_encoding() {
        let encoded: String = SVG
            .bytes()
            .map(|b| match b {
                b'<' | b'>' | b'"' | b'#' | b' ' => format!("%{b:02X}"),
                _ => (b as char).to_string(),
            })
            .collect();
        let urls = [
            format!("data:image/svg+xml;base64,{}", encode(SVG.as_bytes())),
            format!("data:image/svg+xml;charset=utf-8,{encoded}"),
            format!("data:image/svg+xml;utf8,{SVG}"),
            format!(
                "data:application/octet-stream;base64,{}",
                encode(SVG.as_bytes())
            ),
        ];
        let mut c = Cache::default();
        for url in &urls {
            let p = c.lookup(url).unwrap_or_else(|| panic!("{url}"));
            assert_eq!((p.width(), p.height()), (MAX_SIDE, MAX_SIDE / 2), "{url}");
        }
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

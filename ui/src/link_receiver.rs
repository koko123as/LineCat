//! `LinkReceiver` (lib/common/link_receiver.dart): a one-page form on the local network,
//! so a phone that scans the address can hand a link to a TV that has no way to type one.

use crate::qr_import::profile_url_from_qr_codes;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const MAX_BODY_BYTES: usize = 16 * 1024;
const MAX_HEAD_BYTES: usize = 16 * 1024;

pub const TITLE: &str = "把链接发送到 SeasonCat";
const HINT: &str = "订阅链接或代理分享链接";
const SEND: &str = "发送";
const SENT: &str = "已发送，请在另一台设备上确认导入。";
const INVALID: &str = "不是有效的订阅链接或代理分享链接";

pub struct LinkReceiver {
    pub address: String,
    closed: Arc<AtomicBool>,
}

impl LinkReceiver {
    /// Serves on `host` (the LAN address when `None`) and hands the first valid link to
    /// `on_link` on the server's thread; `None` when the device has no private IPv4 address.
    pub fn start(
        host: Option<Ipv4Addr>,
        on_link: impl FnOnce(String) + Send + 'static,
    ) -> io::Result<Option<LinkReceiver>> {
        let Some(host) = host.or_else(|| lan_address(&interface_addresses())) else {
            return Ok(None);
        };
        let listener = TcpListener::bind(SocketAddrV4::new(host, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let token = new_token()?;
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        let path = format!("/{token}");
        std::thread::Builder::new()
            .name("link-receiver".into())
            .spawn(move || serve(listener, &path, &flag, on_link))?;
        Ok(Some(LinkReceiver {
            address: format!("http://{host}:{port}/{token}"),
            closed,
        }))
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl Drop for LinkReceiver {
    fn drop(&mut self) {
        self.close();
    }
}

fn serve(listener: TcpListener, path: &str, closed: &AtomicBool, on_link: impl FnOnce(String)) {
    let mut on_link = Some(on_link);
    while !closed.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(link) = handle(stream, path) {
                    closed.store(true, Ordering::SeqCst);
                    if let Some(f) = on_link.take() {
                        f(link);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

struct Request {
    method: String,
    path: String,
    body: Option<Vec<u8>>,
}

/// Answers one connection; the link when it carried a valid one.
fn handle(mut stream: TcpStream, path: &str) -> Option<String> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let request = read_request(&mut stream)?;
    let (status, page, link) = respond(&request, path);
    let _ = write_response(&mut stream, status, &page);
    link
}

fn respond(request: &Request, path: &str) -> (&'static str, String, Option<String>) {
    if request.path.split('?').next() != Some(path) {
        return ("404 Not Found", String::new(), None);
    }
    match request.method.as_str() {
        "GET" => ("200 OK", page(false, None, ""), None),
        "POST" => {
            let text = request.body.as_deref().and_then(form_link);
            match text.as_deref().and_then(|t| profile_url_from_qr_codes([t])) {
                Some(link) => ("200 OK", page(true, None, ""), Some(link)),
                None => (
                    "400 Bad Request",
                    page(false, Some(INVALID), text.as_deref().unwrap_or("")),
                    None,
                ),
            }
        }
        _ => ("405 Method Not Allowed", String::new(), None),
    }
}

fn read_request(stream: &mut impl Read) -> Option<Request> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_HEAD_BYTES {
            return None;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_string(), first.next()?.to_string());
    let length = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    if length > MAX_BODY_BYTES {
        return Some(Request {
            method,
            path,
            body: None,
        });
    }
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Some(Request {
        method,
        path,
        body: Some(body),
    })
}

fn write_response(stream: &mut impl Write, status: &str, page: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    )?;
    stream.flush()
}

/// `Uri.splitQueryString(...)['link']?.trim()`.
fn form_link(body: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    text.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let decode = |s: &str| crate::profile::percent_decode(&s.replace('+', " "));
        (decode(key) == "link").then(|| decode(value).trim().to_string())
    })
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '/' => out.push_str("&#47;"),
            _ => out.push(c),
        }
    }
    out
}

fn page(done: bool, error: Option<&str>, value: &str) -> String {
    let title = escape(TITLE);
    let body = if done {
        format!("<p>{}</p>", escape(SENT))
    } else {
        format!(
            "<form method=\"post\">\n<textarea name=\"link\" rows=\"4\" placeholder=\"{}\" autofocus required>{}</textarea>\n{}\n<button type=\"submit\">{}</button>\n</form>",
            escape(HINT),
            escape(value),
            error
                .map(|e| format!("<p class=\"error\">{}</p>", escape(e)))
                .unwrap_or_default(),
            escape(SEND),
        )
    };
    format!(
        "<!doctype html>
<html><head><meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title}</title>
<style>
body{{font-family:system-ui,sans-serif;margin:0;padding:1.5rem;max-width:35rem;margin:auto;color-scheme:light dark}}
h1{{font-size:1.4rem}}
textarea{{width:100%;box-sizing:border-box;font:inherit;padding:.75rem;border-radius:.75rem;border:thin solid #8888}}
button{{margin-top:1rem;width:100%;padding:.9rem;font:inherit;font-weight:600;border:0;border-radius:.9rem;background:#007aff;color:#fff}}
.error{{color:#ff3b30}}
</style></head>
<body><h1>{title}</h1>{body}</body></html>"
    )
}

/// `lanAddress`: the first private IPv4 address, loopback and link-local left out.
pub fn lan_address(addresses: &[(String, Ipv4Addr)]) -> Option<Ipv4Addr> {
    let preferred = |name: &str| {
        let n = name.to_ascii_lowercase();
        ["wlan", "wl", "eth", "en", "wifi", "wi-fi", "ethernet"]
            .iter()
            .any(|p| n.starts_with(p))
    };
    let ordered = addresses
        .iter()
        .filter(|(n, _)| preferred(n))
        .chain(addresses.iter().filter(|(n, _)| !preferred(n)));
    ordered.map(|(_, a)| *a).find(|a| a.is_private())
}

fn interface_addresses() -> Vec<(String, Ipv4Addr)> {
    let mut out = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list`, released by freeifaddrs once read; every entry's
    // pointers are checked for null before they are read.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return out;
        }
        let mut cur = list;
        while !cur.is_null() {
            let entry = &*cur;
            let addr = entry.ifa_addr;
            if !addr.is_null() && (*addr).sa_family as i32 == libc::AF_INET {
                let v4 = &*(addr as *const libc::sockaddr_in);
                let ip = Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr));
                let name = if entry.ifa_name.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(entry.ifa_name)
                        .to_string_lossy()
                        .into_owned()
                };
                out.push((name, ip));
            }
            cur = entry.ifa_next;
        }
        libc::freeifaddrs(list);
    }
    out
}

/// 16 random bytes as unpadded base64url, as `_newToken`.
fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, &b)| acc | (b as u32) << (16 - 8 * i));
        for i in 0..chunk.len() + 1 {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn send(address: &str, request: &str) -> String {
        let rest = address.strip_prefix("http://").unwrap();
        let host = rest.split('/').next().unwrap();
        let mut stream = TcpStream::connect(host).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut out = String::new();
        stream.read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn picks_a_private_address_preferring_wifi() {
        let addrs = vec![
            ("lo".to_string(), Ipv4Addr::new(127, 0, 0, 1)),
            ("rmnet0".to_string(), Ipv4Addr::new(10, 1, 2, 3)),
            ("wlan0".to_string(), Ipv4Addr::new(192, 168, 1, 20)),
            ("tun0".to_string(), Ipv4Addr::new(172, 19, 0, 1)),
        ];
        assert_eq!(lan_address(&addrs), Some(Ipv4Addr::new(192, 168, 1, 20)));
        let public = vec![
            ("wlan0".to_string(), Ipv4Addr::new(8, 8, 8, 8)),
            ("eth0".to_string(), Ipv4Addr::new(169, 254, 1, 1)),
            ("tun0".to_string(), Ipv4Addr::new(172, 32, 0, 1)),
        ];
        assert_eq!(lan_address(&public), None);
        assert_eq!(new_token().unwrap().len(), 22);
    }

    #[test]
    fn serves_the_form_and_takes_one_valid_link() {
        let (tx, rx) = mpsc::channel();
        let receiver = LinkReceiver::start(Some(Ipv4Addr::LOCALHOST), move |l| {
            tx.send(l).unwrap();
        })
        .unwrap()
        .unwrap();
        let path = format!("/{}", receiver.address.rsplit('/').next().unwrap());
        let wrong = send(&receiver.address, "GET /nope HTTP/1.1\r\n\r\n");
        assert!(wrong.starts_with("HTTP/1.1 404"));
        let form = send(&receiver.address, &format!("GET {path} HTTP/1.1\r\n\r\n"));
        assert!(form.starts_with("HTTP/1.1 200") && form.contains("<textarea name=\"link\""));
        let body = "link=not+a+link";
        let bad = send(
            &receiver.address,
            &format!(
                "POST {path} HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(
            bad.starts_with("HTTP/1.1 400") && bad.contains(INVALID) && bad.contains("not a link")
        );
        let big = format!("link={}", "a".repeat(MAX_BODY_BYTES));
        let refused = send(
            &receiver.address,
            &format!(
                "POST {path} HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
                big.len()
            ),
        );
        assert!(refused.starts_with("HTTP/1.1 400"));
        let body = "link=+https%3A%2F%2Fexample.com%2Fsub%3Fa%3D1+";
        let ok = send(
            &receiver.address,
            &format!(
                "POST {path} HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(ok.starts_with("HTTP/1.1 200") && ok.contains(SENT));
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "https://example.com/sub?a=1"
        );
        std::thread::sleep(Duration::from_millis(300));
        let host = receiver.address.strip_prefix("http://").unwrap();
        let host = host.split('/').next().unwrap();
        assert!(TcpStream::connect(host).is_err());
    }

    #[test]
    fn escapes_what_the_page_echoes() {
        let html = page(false, Some(INVALID), "<script>\"x\"</script>");
        assert!(html.contains("&lt;script&gt;&quot;x&quot;&lt;&#47;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}

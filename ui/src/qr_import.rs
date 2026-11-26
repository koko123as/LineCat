use crate::profile::{is_share_link, is_url, percent_decode};

/// `protocolSchemes`: the app's own schemes, whose `install-config` link carries a URL.
const PROTOCOL_SCHEMES: &[&str] = &["clash", "clashmeta", "flclash"];

/// `profileUrlFromQrCodes` in lib/common/link.dart.
pub fn profile_url_from_qr_codes<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<String> {
    for value in values {
        let text = value.trim();
        if text.is_empty() {
            continue;
        }
        if is_url(text) || is_share_link(text) {
            return Some(text.to_string());
        }
        if let Some(url) = install_config_url(text).filter(|u| is_url(u)) {
            return Some(url);
        }
    }
    None
}

/// `_installConfigUrl`, with the query decoded as `Uri.queryParameters` decodes it.
pub(crate) fn install_config_url(text: &str) -> Option<String> {
    let (scheme, rest) = text.split_once("://")?;
    if !PROTOCOL_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) {
        return None;
    }
    let rest = rest.split('#').next().unwrap_or_default();
    let (authority, query) = match rest.split_once('?') {
        Some((a, q)) => (a, q),
        None => (rest, ""),
    };
    let host = authority.split('/').next().unwrap_or_default();
    if !host.eq_ignore_ascii_case("install-config") {
        return None;
    }
    let url = query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (decode_component(key) == "url").then(|| decode_component(value))
    })?;
    let url = url.trim();
    (!url.is_empty()).then(|| url.to_string())
}

fn decode_component(s: &str) -> String {
    percent_decode(&s.replace('+', " "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_urls_and_share_links_as_they_are() {
        assert_eq!(
            profile_url_from_qr_codes(["  https://example.com/sub?a=1 "]).as_deref(),
            Some("https://example.com/sub?a=1")
        );
        assert_eq!(
            profile_url_from_qr_codes(["ss://YWVzLTEyOC1nY206cGFzcw@1.2.3.4:8388#node"]).as_deref(),
            Some("ss://YWVzLTEyOC1nY206cGFzcw@1.2.3.4:8388#node")
        );
    }

    #[test]
    fn reads_install_config_links() {
        for (text, url) in [
            (
                "clash://install-config?url=https%3A%2F%2Fexample.com%2Fsub%3Ftoken%3Dab",
                "https://example.com/sub?token=ab",
            ),
            (
                "clashmeta://install-config?name=x&url=https://e.co/s",
                "https://e.co/s",
            ),
            (
                "FLCLASH://Install-Config/?url=%20https://e.co/a%20#frag",
                "https://e.co/a",
            ),
        ] {
            assert_eq!(
                profile_url_from_qr_codes([text]).as_deref(),
                Some(url),
                "{text}"
            );
        }
    }

    #[test]
    fn skips_what_is_not_a_profile_link() {
        assert_eq!(
            profile_url_from_qr_codes([
                "",
                "hello world",
                "clash://other?url=https://e.co/s",
                "clash://install-config?url=not-a-url",
                "clash://install-config?name=x",
                "mailto:someone",
            ]),
            None
        );
        assert_eq!(
            profile_url_from_qr_codes(["plain text", "clash://install-config?url=http://e.co/x"])
                .as_deref(),
            Some("http://e.co/x")
        );
    }
}

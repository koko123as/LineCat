//! The resources page (lib/views/resources.dart): the geo files the Core keeps, its
//! auto update and the update interval.

use std::collections::BTreeMap;

/// `GeoResource`: its name (what `geo.update` takes), its `geox-url` key and its file
/// in the Core's home.
pub const GEO: &[(&str, &str, &str)] = &[
    ("MMDB", "mmdb", "GEOIP.metadb"),
    ("ASN", "asn", "ASN.mmdb"),
    ("GEOSITE", "geosite", "GEOSITE.dat"),
];

/// `defaultGeoXUrl`.
pub fn default_url(key: &str) -> &'static str {
    match key {
        "mmdb" => {
            "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geoip.metadb"
        }
        "asn" => {
            "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/GeoLite2-ASN.mmdb"
        }
        _ => "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geosite.dat",
    }
}

pub fn default_urls() -> BTreeMap<String, String> {
    GEO.iter()
        .map(|(_, key, _)| (key.to_string(), default_url(key).to_string()))
        .collect()
}

/// `TextInputLimits.geoUpdateInterval`: at most this many digits.
pub const INTERVAL_DIGITS: usize = 6;

/// The update interval dialog's validator: hours, a positive number.
pub fn check_interval(value: &str) -> Result<i64, String> {
    if value.is_empty() {
        return Err("自动更新间隔不能为空".into());
    }
    match value.parse::<i64>() {
        Err(_) => Err("自动更新间隔必须为数字".into()),
        Ok(n) if n <= 0 => Err("自动更新间隔必须大于0".into()),
        Ok(n) => Ok(n),
    }
}

/// `UpdateGeoUrlFormDialog`'s validator.
pub fn check_url(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("不能为空".into());
    }
    if !crate::profile::is_url(value) {
        return Err("必须为URL".into());
    }
    Ok(value.to_string())
}

/// `TextInputLimits.digitsOnly`: what the interval field keeps of what was typed.
pub fn digits_only(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_digit)
        .take(INTERVAL_DIGITS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_checks_read_like_flclash() {
        assert_eq!(check_interval(""), Err("自动更新间隔不能为空".into()));
        assert_eq!(check_interval("a"), Err("自动更新间隔必须为数字".into()));
        assert_eq!(check_interval("0"), Err("自动更新间隔必须大于0".into()));
        assert_eq!(check_interval("24"), Ok(24));
        assert_eq!(digits_only("1a2b34567"), "123456");
    }

    #[test]
    fn url_checks() {
        assert!(check_url("").is_err());
        assert_eq!(check_url("ftp:/x"), Err("必须为URL".into()));
        assert!(check_url("https://example.com/geo.dat").is_ok());
        assert_eq!(default_urls().len(), 3);
    }
}

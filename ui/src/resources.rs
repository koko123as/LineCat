//! The resources page (lib/views/resources.dart): the geo files the Core keeps, its
//! auto update and the update interval, then the remote scripts and MITM modules.

use crate::items::Script;
use crate::mitm::MitmModule;
use std::collections::BTreeMap;

/// A remote `_ResourceItem`; `updated` in Unix seconds, None reads 未知.
#[derive(Debug, PartialEq)]
pub struct Remote {
    pub key: String,
    pub title: String,
    pub url: String,
    pub updated: Option<u64>,
}

/// `_readResources().scripts`: the scripts with a URL, keyed `script:<id>`.
pub fn remote_scripts(scripts: &[Script]) -> Vec<Remote> {
    scripts
        .iter()
        .filter_map(|s| {
            Some(Remote {
                key: format!("script:{}", s.id),
                title: s.label.clone(),
                url: s.url.clone()?,
                updated: (s.updated > 0).then_some(s.updated / 1000),
            })
        })
        .collect()
}

pub fn remote_modules(modules: &[MitmModule]) -> Vec<Remote> {
    modules
        .iter()
        .filter(|m| m.is_remote())
        .map(|m| Remote {
            key: format!("module:{}", m.id),
            title: m.name.clone(),
            url: m.url.clone(),
            updated: (m.updated > 0).then_some(m.updated),
        })
        .collect()
}

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

    #[test]
    fn lists_only_remote_scripts_and_modules() {
        let scripts = [
            Script {
                id: "1".into(),
                label: "main".into(),
                url: Some("https://example.com/main.js".into()),
                updated: 1_700_000_000_000,
            },
            Script {
                id: "2".into(),
                label: "local".into(),
                ..Default::default()
            },
        ];
        assert_eq!(
            remote_scripts(&scripts),
            vec![Remote {
                key: "script:1".into(),
                title: "main".into(),
                url: "https://example.com/main.js".into(),
                updated: Some(1_700_000_000),
            }]
        );
        let modules = [
            MitmModule {
                id: "m".into(),
                name: "去广告".into(),
                url: "https://example.com/m.module".into(),
                ..Default::default()
            },
            MitmModule {
                id: "l".into(),
                name: "本地".into(),
                ..Default::default()
            },
        ];
        let rows = remote_modules(&modules);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "module:m");
        assert_eq!(rows[0].updated, None);
    }
}

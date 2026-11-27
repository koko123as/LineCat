//! The profile sheet's 外部资源 (FlClash: lib/views/proxies/providers.dart): each provider the
//! Core loaded, from `provider.list`, its row, its menu and what its file holds.

use serde_json::Value;

/// `SubscriptionInfo`, in bytes and unix seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Subscription {
    pub upload: i64,
    pub download: i64,
    pub total: i64,
    pub expire: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtProvider {
    pub name: String,
    pub rule: bool,
    pub vehicle: String,
    pub count: i64,
    pub path: String,
    /// Unix milliseconds, 0 when never.
    pub updated_at: i64,
    pub subscription: Option<Subscription>,
}

impl ExtProvider {
    pub fn parse(v: &Value) -> Option<ExtProvider> {
        let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let int = |v: &Value, k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
        let name = text("name");
        if name.is_empty() {
            return None;
        }
        let subscription = v
            .get("subscriptionInfo")
            .filter(|s| s.is_object())
            .map(|s| Subscription {
                upload: int(s, "upload"),
                download: int(s, "download"),
                total: int(s, "total"),
                expire: int(s, "expire"),
            });
        Some(ExtProvider {
            name,
            rule: text("type").eq_ignore_ascii_case("rule"),
            vehicle: text("vehicleType"),
            count: int(v, "count"),
            path: text("path"),
            updated_at: int(v, "updatedAt"),
            subscription,
        })
    }

    pub fn usage(&self) -> Option<f32> {
        let s = self.subscription?;
        (s.total > 0)
            .then(|| ((s.upload + s.download) as f64 / s.total as f64).clamp(0.0, 1.0) as f32)
    }

    pub fn count_label(&self) -> String {
        let n = crate::icon_sets::compact(self.count.max(0) as usize);
        if self.rule {
            format!("{n} 条规则")
        } else {
            format!("{n} 个代理")
        }
    }

    /// `_isSyncable`: fetched over HTTP, or one of the app's remote rule sets.
    pub fn syncable(&self, app_rule_set: bool) -> bool {
        self.vehicle.eq_ignore_ascii_case("http") || app_rule_set
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderFile {
    Text,
    RuleSet,
    Binary,
}

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
pub const PROBE_LENGTH: usize = 512;

/// `_probeProviderFile` on the file's first bytes, None when it is not there yet. An `.mrs`
/// set is a zstd stream and any other binary file shows a NUL byte.
pub fn probe(head: Option<&[u8]>) -> ProviderFile {
    match head {
        None => ProviderFile::Text,
        Some(h) if h.starts_with(&ZSTD_MAGIC) => ProviderFile::RuleSet,
        Some(h) if h.contains(&0) => ProviderFile::Binary,
        Some(_) => ProviderFile::Text,
    }
}

/// `_menuItems`, by id: preview (an `.mrs` set), upload, sync, subscription.
pub fn menu(
    file: ProviderFile,
    syncable: bool,
    subscribed: bool,
) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut items = Vec::new();
    if file == ProviderFile::RuleSet {
        items.push(("preview", "eye", "预览"));
    }
    items.push(("upload", "import-file", "上传"));
    if syncable {
        items.push(("sync", "sync", "同步"));
    }
    if subscribed {
        items.push(("subscription", "info", "订阅信息"));
    }
    items
}

pub fn subscription_text(s: Subscription, expire_date: &str) -> String {
    let used = crate::size(s.upload + s.download);
    let expire = if s.expire != 0 {
        expire_date
    } else {
        "长期有效"
    };
    format!(
        "流量统计\n已用流量：{used}\n总流量：{}\n\n到期时间\n{expire}",
        crate::size(s.total)
    )
}

/// `_decodeProviderFile`: an upload must be text for the Core's side load.
pub fn decode(bytes: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(bytes).map_err(|_| "该外部资源不是文本文件".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_provider_list_items() {
        let p = ExtProvider::parse(&json!({
            "name": "airport", "type": "proxy", "vehicleType": "HTTP", "count": 1234,
            "path": "/x/a.yaml", "updatedAt": 1700000000000i64,
            "subscriptionInfo": {"upload": 1, "download": 3, "total": 8, "expire": 0}
        }))
        .unwrap();
        assert!(!p.rule && p.syncable(false));
        assert_eq!(p.count_label(), "1.2K 个代理");
        assert_eq!(p.usage(), Some(0.5));
        let r = ExtProvider::parse(
            &json!({"name": "ads", "type": "rule", "vehicleType": "File", "count": 3}),
        )
        .unwrap();
        assert!(r.rule && !r.syncable(false) && r.syncable(true));
        assert_eq!((r.count_label(), r.usage()), ("3 条规则".to_string(), None));
        assert_eq!(ExtProvider::parse(&json!({"type": "rule"})), None);
        let zero =
            ExtProvider::parse(&json!({"name": "z", "subscriptionInfo": {"total": 0}})).unwrap();
        assert_eq!(zero.usage(), None);
    }

    #[test]
    fn probes_what_the_file_holds() {
        assert_eq!(probe(None), ProviderFile::Text);
        assert_eq!(
            probe(Some(&[0x28, 0xB5, 0x2F, 0xFD, 1][..])),
            ProviderFile::RuleSet
        );
        assert_eq!(probe(Some(&b"a\0b"[..])), ProviderFile::Binary);
        assert_eq!(probe(Some(&b"payload:\n"[..])), ProviderFile::Text);
    }

    #[test]
    fn the_menu_follows_flclash() {
        fn ids(v: Vec<(&'static str, &'static str, &'static str)>) -> Vec<&'static str> {
            v.into_iter().map(|i| i.0).collect()
        }
        assert_eq!(ids(menu(ProviderFile::Text, false, false)), ["upload"]);
        assert_eq!(
            ids(menu(ProviderFile::RuleSet, true, true)),
            ["preview", "upload", "sync", "subscription"]
        );
    }

    #[test]
    fn subscription_lines_and_uploads() {
        let s = Subscription {
            upload: 0,
            download: 1024,
            total: 2048,
            expire: 0,
        };
        let t = subscription_text(s, "2026-12-01");
        assert!(
            t.contains("已用流量：1.0 KB") && t.ends_with("长期有效"),
            "{t}"
        );
        assert!(
            subscription_text(Subscription { expire: 1, ..s }, "2026-12-01")
                .ends_with("2026-12-01")
        );
        assert!(decode(b"proxies: []").is_ok());
        assert!(decode(&[0xff, 0xfe]).is_err());
    }
}

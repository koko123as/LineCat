//! The custom profile's 外部资源 (lib/views/profiles/custom/providers.dart): the proxy
//! providers its groups use and the rule sets its rules and settings name, and each proxy
//! provider's options (`ProxyProviderOptions`), kept in the profile's overrides by name.

use crate::store::State;
use serde_json::{json, Map, Value};

/// `_ProviderOption`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum POpt {
    Url,
    Interval,
    Timeout,
    Lazy,
    ExpectedStatus,
    AdditionalPrefix,
    AdditionalSuffix,
    Udp,
    SkipCertVerify,
    IpVersion,
}

pub const POPTS: [POpt; 10] = [
    POpt::Url,
    POpt::Interval,
    POpt::Timeout,
    POpt::Lazy,
    POpt::ExpectedStatus,
    POpt::AdditionalPrefix,
    POpt::AdditionalSuffix,
    POpt::Udp,
    POpt::SkipCertVerify,
    POpt::IpVersion,
];

pub const IP_VERSIONS: [&str; 5] = ["dual", "ipv4", "ipv6", "ipv4-prefer", "ipv6-prefer"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PKind {
    Text,
    Number(&'static str),
    Toggle,
    Choice,
}

impl POpt {
    /// Where the option sits in `ProxyProviderOptions`' JSON.
    pub fn path(self) -> (&'static str, &'static str) {
        match self {
            POpt::Url => ("health-check", "url"),
            POpt::Interval => ("health-check", "interval"),
            POpt::Timeout => ("health-check", "timeout"),
            POpt::Lazy => ("health-check", "lazy"),
            POpt::ExpectedStatus => ("health-check", "expected-status"),
            POpt::AdditionalPrefix => ("override", "additional-prefix"),
            POpt::AdditionalSuffix => ("override", "additional-suffix"),
            POpt::Udp => ("override", "udp"),
            POpt::SkipCertVerify => ("override", "skip-cert-verify"),
            POpt::IpVersion => ("override", "ip-version"),
        }
    }

    pub fn key(self) -> &'static str {
        self.path().1
    }

    pub fn of(key: &str) -> Option<POpt> {
        POPTS.into_iter().find(|o| o.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            POpt::Url => "测速链接",
            POpt::Interval => "测试间隔",
            POpt::Timeout => "超时",
            POpt::Lazy => "使用时测试",
            POpt::ExpectedStatus => "预期状态",
            POpt::AdditionalPrefix => "名称前缀",
            POpt::AdditionalSuffix => "名称后缀",
            POpt::Udp => "UDP",
            POpt::SkipCertVerify => "跳过证书验证",
            POpt::IpVersion => "IP 版本",
        }
    }

    pub fn section(self) -> &'static str {
        match self {
            POpt::Url | POpt::Interval | POpt::Timeout | POpt::Lazy | POpt::ExpectedStatus => {
                "健康检查"
            }
            POpt::AdditionalPrefix | POpt::AdditionalSuffix => "名称",
            _ => "连接",
        }
    }

    pub fn kind(self) -> PKind {
        match self {
            POpt::Interval => PKind::Number("s"),
            POpt::Timeout => PKind::Number("ms"),
            POpt::Lazy | POpt::Udp | POpt::SkipCertVerify => PKind::Toggle,
            POpt::IpVersion => PKind::Choice,
            _ => PKind::Text,
        }
    }

    pub fn get(self, options: &Value) -> Option<&Value> {
        let (parent, key) = self.path();
        options.get(parent)?.get(key).filter(|v| !v.is_null())
    }

    pub fn is_set_in(self, options: &Value) -> bool {
        self.get(options).is_some()
    }

    /// `reset`: the value an option starts from once added.
    pub fn reset_value(self) -> Value {
        match self {
            POpt::Url => crate::profile::DEFAULT_TEST_URL.into(),
            POpt::Interval => 300.into(),
            POpt::Timeout => 5000.into(),
            POpt::Lazy | POpt::Udp | POpt::SkipCertVerify => true.into(),
            POpt::IpVersion => "dual".into(),
            _ => "".into(),
        }
    }

    /// The value a row shows, with the default a toggle or a choice reads as.
    pub fn text(self, options: &Value) -> String {
        match self.get(options) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ if self == POpt::IpVersion => "dual".into(),
            _ => String::new(),
        }
    }

    pub fn on(self, options: &Value) -> bool {
        self.get(options).and_then(Value::as_bool).unwrap_or(true)
    }
}

/// Sets or clears an option; an emptied section goes with it.
pub fn set(options: &mut Value, o: POpt, value: Option<Value>) {
    if !options.is_object() {
        *options = json!({});
    }
    let (parent, key) = o.path();
    let m = options.as_object_mut().expect("an object");
    let section = m.entry(parent).or_insert_with(|| Value::Object(Map::new()));
    if !section.is_object() {
        *section = Value::Object(Map::new());
    }
    let s = section.as_object_mut().expect("an object");
    match value {
        Some(v) => {
            s.insert(key.into(), v);
        }
        None => {
            s.remove(key);
        }
    }
    if s.is_empty() {
        m.remove(parent);
    }
}

/// The options as stored: none left means the provider takes no entry.
pub fn stored(options: &Value) -> Option<Value> {
    let mut out = options.as_object().cloned().unwrap_or_default();
    out.retain(|_, v| !v.is_null() && v.as_object().is_none_or(|m| !m.is_empty()));
    (!out.is_empty()).then_some(Value::Object(out))
}

/// `ProxyProviderOptionsExt.definition`: a health check always carries a url, and a text
/// option left empty stays out.
pub fn definition(options: &Value) -> Map<String, Value> {
    let text = |o: POpt| {
        o.get(options)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| Value::from(s.to_string()))
    };
    let mut out = Map::new();
    let mut health = Map::new();
    for o in [POpt::Interval, POpt::Timeout, POpt::Lazy] {
        if let Some(v) = o.get(options) {
            health.insert(o.key().into(), v.clone());
        }
    }
    let url = text(POpt::Url);
    let status = text(POpt::ExpectedStatus);
    if url.is_some() || status.is_some() || !health.is_empty() {
        let mut hc = Map::new();
        hc.insert("enable".into(), true.into());
        hc.insert(
            "url".into(),
            url.unwrap_or_else(|| crate::profile::DEFAULT_TEST_URL.into()),
        );
        hc.extend(health);
        if let Some(s) = status {
            hc.insert("expected-status".into(), s);
        }
        out.insert("health-check".into(), Value::Object(hc));
    }
    for k in ["filter", "exclude-filter"] {
        if let Some(v) = options.get(k).filter(|v| v.is_string()) {
            out.insert(k.into(), v.clone());
        }
    }
    let mut over = Map::new();
    for o in [POpt::AdditionalPrefix, POpt::AdditionalSuffix] {
        if let Some(v) = text(o) {
            over.insert(o.key().into(), v);
        }
    }
    for o in [POpt::Udp, POpt::SkipCertVerify, POpt::IpVersion] {
        if let Some(v) = o.get(options) {
            over.insert(o.key().into(), v.clone());
        }
    }
    if !over.is_empty() {
        out.insert("override".into(), Value::Object(over));
    }
    out
}

/// `customReferencedProviders`: the proxy providers the groups use, then the rule sets the
/// rules and the settings name, each once.
pub fn referenced(state: &State, profile_id: &str) -> (Vec<String>, Vec<String>) {
    let mut proxy: Vec<String> = Vec::new();
    for g in crate::custom_profile::groups(state, profile_id) {
        for n in crate::proxy_groups::names(&g.definition, "use") {
            if !proxy.contains(&n) {
                proxy.push(n);
            }
        }
    }
    (
        proxy,
        crate::profile_rules::referenced_rule_sets(state, profile_id),
    )
}

/// The options kept for a provider, or none.
pub fn options_of(state: &State, profile_id: &str, name: &str) -> Value {
    state
        .profiles
        .iter()
        .find(|p| p.id == profile_id)
        .and_then(|p| p.overrides.proxy_providers.get(name).cloned())
        .unwrap_or_else(|| json!({}))
}

/// `_handleSave`: the provider's entry replaced, or dropped when nothing is set.
pub fn put(state: &mut State, profile_id: &str, name: &str, options: &Value) {
    if let Some(p) = state.profiles.iter_mut().find(|p| p.id == profile_id) {
        match stored(options) {
            Some(v) => p.overrides.proxy_providers.insert(name.into(), v),
            None => p.overrides.proxy_providers.remove(name),
        };
    }
}

/// The `proxy-providers` a custom profile declares (`_resolveInjectedProviders`): each
/// subscription or file profile its groups use, read from its file, with its options.
pub fn injected(
    state: &State,
    profile: &crate::store::Profile,
    path_of: impl Fn(&str) -> String,
) -> Map<String, Value> {
    let (used, _) = referenced(state, &profile.id);
    let mut out = Map::new();
    for p in state.profiles.iter().filter(|p| !p.is_custom()) {
        let label = p.label();
        if !used.contains(&label) || out.contains_key(&label) {
            continue;
        }
        let mut def = Map::new();
        def.insert("type".into(), "file".into());
        def.insert("path".into(), path_of(&p.id).into());
        if let Some(o) = profile.overrides.proxy_providers.get(&label) {
            def.extend(definition(o));
        }
        out.insert(label, Value::Object(def));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_add_clear_and_store_without_empty_sections() {
        let mut o = json!({});
        assert_eq!(stored(&o), None);
        set(&mut o, POpt::IpVersion, Some(POpt::IpVersion.reset_value()));
        set(&mut o, POpt::Timeout, Some(POpt::Timeout.reset_value()));
        assert!(POpt::IpVersion.is_set_in(&o) && POpt::Timeout.is_set_in(&o));
        assert_eq!(POpt::IpVersion.text(&o), "dual");
        assert_eq!(POpt::Timeout.text(&o), "5000");
        assert!(POpt::Udp.on(&o) && !POpt::Udp.is_set_in(&o));
        set(&mut o, POpt::Timeout, None);
        assert_eq!(o, json!({"override": {"ip-version": "dual"}}));
        set(&mut o, POpt::IpVersion, None);
        assert_eq!(stored(&o), None);
        o["filter"] = "HK".into();
        assert_eq!(stored(&o), Some(json!({"filter": "HK"})));
        assert_eq!(POpt::of("skip-cert-verify"), Some(POpt::SkipCertVerify));
        assert_eq!(POPTS.iter().filter(|o| o.section() == "名称").count(), 2);
    }

    #[test]
    fn the_definition_carries_a_url_and_leaves_empty_text_out() {
        let o = json!({
            "health-check": {"interval": 60, "expected-status": ""},
            "override": {"additional-prefix": "", "udp": false, "ip-version": "ipv4-prefer"},
            "filter": "HK`JP", "memberLayout": ["filter"]
        });
        assert_eq!(
            Value::Object(definition(&o)),
            json!({
                "health-check": {"enable": true, "url": crate::profile::DEFAULT_TEST_URL, "interval": 60},
                "filter": "HK`JP",
                "override": {"udp": false, "ip-version": "ipv4-prefer"}
            })
        );
        assert!(definition(&json!({"health-check": {"url": ""}})).is_empty());
    }

    #[test]
    fn providers_the_groups_use_are_injected_with_their_options() {
        let mut s = State::default();
        let mut c = crate::custom_profile::new_profile("c".into(), "自定义", 1);
        c.overrides
            .proxy_providers
            .insert("订阅".into(), json!({"override": {"udp": true}}));
        s.profiles.push(c.clone());
        for (id, name) in [("1", "订阅"), ("2", "其他")] {
            s.profiles.push(crate::store::Profile {
                id: id.into(),
                name: name.into(),
                url: "https://example.com/sub".into(),
                ..Default::default()
            });
        }
        s.proxy_groups.push(crate::custom_profile::ProxyGroupRow {
            id: 5,
            profile_id: "c".into(),
            definition: json!({"name": "G", "type": "select", "use": ["订阅", "缺失"]}),
            order: None,
        });
        assert_eq!(referenced(&s, "c").0, ["订阅", "缺失"]);
        let out = injected(&s, &c, |id| format!("/p/{id}.yaml"));
        assert_eq!(
            Value::Object(out),
            json!({"订阅": {"type": "file", "path": "/p/1.yaml", "override": {"udp": true}}})
        );
        put(&mut s, "c", "订阅", &json!({"override": {}}));
        assert!(s.profiles[0].overrides.proxy_providers.is_empty());
    }
}

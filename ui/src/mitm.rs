//! MITM (lib/views/config/mitm.dart, mitm_modules.dart, lib/features/mitm/module_editor.dart):
//! the Core's `mitm` section, the CA, the hostname list and the modules as `mitm.readModule`
//! reads them.

use crate::store::MitmSettings;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// `MitmList`: each key with its label.
pub const LISTS: &[(&str, &str)] = &[
    ("hostname", "主机名"),
    ("url-rewrite", "URL 重写"),
    ("header-rewrite", "Header 重写"),
    ("body-rewrite", "Body 重写"),
    ("map-local", "本地映射"),
    ("request-body-mock", "请求体替换"),
    ("client-source-address", "客户端地址"),
    ("skip-server-cert-verify-hostname", "不校验证书的主机"),
    ("script", "脚本"),
];

/// `mitmCertificateFileName`.
pub const CERT_FILE: &str = "SeasonCat CA.crt";
pub const MODULE_INVALID: &str = "这不是 Surge 模块、Loon 插件、Quantumult X 片段或 Stash 覆写";

pub fn list_label(key: &str) -> &'static str {
    LISTS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, l)| *l)
        .unwrap_or("")
}

/// `MitmModule`; the text is `mitm/modules/<id>.txt` in the Core's home.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MitmModule {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Empty for a local module.
    pub url: String,
    pub enabled: bool,
    pub arguments: BTreeMap<String, String>,
    pub system: String,
    pub scripts: usize,
    pub asks_consent: bool,
    pub accepted: Option<bool>,
    /// Unix seconds.
    pub updated: u64,
}

impl Default for MitmModule {
    fn default() -> Self {
        MitmModule {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            url: String::new(),
            enabled: true,
            arguments: BTreeMap::new(),
            system: String::new(),
            scripts: 0,
            asks_consent: false,
            accepted: None,
            updated: 0,
        }
    }
}

impl MitmModule {
    pub fn is_remote(&self) -> bool {
        !self.url.is_empty()
    }

    pub fn file_name(&self) -> String {
        format!("{}.txt", self.id)
    }

    pub fn asks_before_turning_on(&self) -> bool {
        self.scripts > 0 || self.asks_consent
    }

    /// Turned off by an update the user has not answered yet.
    pub fn awaits_consent(&self) -> bool {
        !self.enabled && self.accepted.is_none() && self.asks_before_turning_on()
    }
}

/// `_androidMitmLimits`.
fn limits(m: &MitmSettings) -> Value {
    json!({
        "max-body-size": 2 << 20,
        "body-buffer": 8 << 20,
        "certificate-cache": 256,
        "max-sessions": 256,
        "script-concurrency": 2,
        "script-memory": 64 << 20,
        "capture-entries": crate::http_capture::limit(m),
        "capture-bytes": 16 << 20,
    })
}

/// `Mitm.coreJson` with the user's hostnames, and the modules once decryption is on.
pub fn core_json(m: &MitmSettings, modules: &[MitmModule], language: &str) -> Value {
    let mut c = Map::new();
    c.insert("enable".into(), m.enable.into());
    c.insert(
        "skip-server-cert-verify".into(),
        m.skip_server_cert_verify.into(),
    );
    c.insert("auto-quic-block".into(), m.auto_quic_block.into());
    c.insert("h2".into(), m.h2.into());
    for (key, _) in LISTS {
        let lines = if *key == "hostname" {
            json!(m.hostname)
        } else {
            json!([])
        };
        c.insert((*key).into(), lines);
    }
    let trusted = m.trusted_fingerprint.clone().filter(|f| !f.is_empty());
    c.insert(
        "decrypt-tls".into(),
        (m.decrypt_untrusted || trusted.is_some()).into(),
    );
    let fingerprint = if m.decrypt_untrusted {
        String::new()
    } else {
        trusted.unwrap_or_default()
    };
    c.insert("ca-fingerprint".into(), fingerprint.into());
    c.insert("limits".into(), limits(m));
    if m.enable {
        let list: Vec<Value> = modules
            .iter()
            .map(|module| {
                let mut o = Map::new();
                o.insert("name".into(), module.id.clone().into());
                o.insert(
                    "path".into(),
                    format!("modules/{}", module.file_name()).into(),
                );
                if !module.enabled {
                    o.insert("enabled".into(), false.into());
                }
                if !module.arguments.is_empty() {
                    o.insert("arguments".into(), json!(module.arguments));
                }
                Value::Object(o)
            })
            .collect();
        c.insert("modules".into(), Value::Array(list));
        c.insert("module-language".into(), language.into());
    }
    Value::Object(c)
}

/// `Platform.localeName` as the Core takes it.
pub fn language(locale: Option<&str>) -> String {
    locale
        .filter(|l| !l.is_empty())
        .unwrap_or("zh-CN")
        .replace('_', "-")
}

/// `MitmCertificate`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Certificate {
    pub pem: String,
    pub fingerprint: String,
    pub subject: String,
    /// Unix milliseconds.
    pub not_after: i64,
}

/// The CA in a `mitm.certificate` or `mitm.generateCertificate` answer.
pub fn certificate_of(answer: &Value) -> Option<Certificate> {
    let c = answer.get("certificate").filter(|c| c.is_object())?;
    let text = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Some(Certificate {
        pem: text("pem"),
        fingerprint: text("fingerprint"),
        subject: text("subject"),
        not_after: crate::http_capture::rfc3339_ms(&text("not-after")),
    })
}

/// `count == 0` 无, all valid `N 项`, else how many are invalid.
pub fn count_label(count: usize, invalid: usize) -> String {
    match (count, invalid) {
        (0, _) => "无".into(),
        (n, 0) => format!("{n} 项"),
        (n, i) => format!("{n} 条中 {i} 条无效"),
    }
}

/// `mitm.validate`'s issues of one list, by the line they are about.
pub fn issues_of(answer: &Value, key: &str, lines: &[String]) -> BTreeMap<String, String> {
    by_index(answer.get("issues"), key, "message")
        .into_iter()
        .filter_map(|(i, v)| {
            let line = lines.get(i)?;
            Some((line.clone(), v.as_str()?.to_string()))
        })
        .collect()
}

fn by_index(lists: Option<&Value>, key: &str, field: &str) -> Vec<(usize, Value)> {
    lists
        .and_then(|l| l.get(key))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let index = item.get("index")?.as_u64()? as usize;
                    Some((index, item.get(field)?.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The `{"config": ...}` that checks the user's hostnames.
pub fn validate_params(hostname: &[String]) -> Value {
    let mut config = Map::new();
    for (key, _) in LISTS {
        let lines = if *key == "hostname" {
            json!(hostname)
        } else {
            json!([])
        };
        config.insert((*key).into(), lines);
    }
    json!({ "config": config })
}

/// `mitmModuleUrls`: every http(s) url in the text, once each.
pub fn module_urls(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in input.split(|c: char| c.is_whitespace() || c == ',') {
        let word = word.trim();
        if crate::icon_sets::is_http_url(word) && !out.iter().any(|u| u == word) {
            out.push(word.to_string());
        }
    }
    out
}

/// `basenameWithoutExtension` of the url's last path segment.
pub fn url_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let path = path.split_once("://").map(|(_, p)| p).unwrap_or(path);
    let segment = path
        .split_once('/')
        .map(|(_, p)| p)
        .unwrap_or("")
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(url);
    crate::rule_sets::file_stem(segment)
}

/// `_moduleTemplate`.
pub fn template() -> String {
    "#!name=模块\n#!desc=\n\n[URL Rewrite]\n\n[MITM]\nhostname = %APPEND%\n".into()
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentKind {
    Text,
    Switch,
    Select,
}

/// `MitmModuleArgumentView`.
#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    pub name: String,
    pub kind: ArgumentKind,
    pub options: Vec<String>,
    pub default: String,
    pub label: String,
    pub desc: String,
}

impl Argument {
    pub fn title(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }

    pub fn value<'a>(&'a self, module: &'a MitmModule) -> &'a str {
        module
            .arguments
            .get(&self.name)
            .map(String::as_str)
            .unwrap_or(&self.default)
    }

    /// The choices with the current value among them.
    pub fn choices(&self, value: &str) -> Vec<String> {
        let mut o = self.options.clone();
        if !o.iter().any(|x| x == value) {
            o.insert(0, value.to_string());
        }
        o
    }
}

/// `MitmModuleLine`: the text, why the Core skips it, and a script task's name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub line: usize,
    pub text: String,
    pub issue: String,
    pub task: Option<String>,
}

/// `MitmModuleDetails`: what `mitm.readModule` makes of a module's text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModuleRead {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub system: Option<String>,
    pub arguments_desc: Option<String>,
    pub arguments: Vec<Argument>,
    pub declared_scripts: usize,
    pub asks_consent: bool,
    pub has_resources: bool,
    pub has_rule_sets: bool,
    pub network: Vec<String>,
    pub lists: Vec<(&'static str, Vec<Line>)>,
    pub rules: Vec<String>,
    /// `namedPolicies`: the policies its rules leave to the profile's exits.
    pub named_policies: Vec<String>,
    /// Each ignored line with why.
    pub ignored: Vec<(String, String)>,
    only_section_skips: bool,
    pub skip_verify_hosts: Vec<String>,
}

fn opt_text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn text(v: &Value, key: &str) -> String {
    opt_text(v, key).unwrap_or_default()
}

fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// `mitmModuleDetailsOf`: the read with each listed line's text taken from the source.
pub fn read_module(source: &str, read: &Value) -> ModuleRead {
    let source_lines: Vec<&str> = source
        .split(['\n'])
        .map(|l| l.trim_end_matches('\r'))
        .collect();
    let meta = read.get("meta").cloned().unwrap_or(Value::Null);
    let arguments = array(read, "arguments")
        .iter()
        .map(|a| {
            let kind = a.get("kind").cloned().unwrap_or(Value::Null);
            Argument {
                name: text(a, "name"),
                kind: match kind.get("type").and_then(Value::as_str) {
                    Some("switch") => ArgumentKind::Switch,
                    Some("select") => ArgumentKind::Select,
                    _ => ArgumentKind::Text,
                },
                options: array(&kind, "options")
                    .iter()
                    .filter_map(|o| o.as_str().map(String::from))
                    .collect(),
                default: a
                    .get("default")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                label: text(a, "label"),
                desc: text(a, "desc"),
            }
        })
        .collect();
    let mut lists = Vec::new();
    let mut skip_verify_hosts = Vec::new();
    for (key, _) in LISTS {
        let issues = by_index(read.get("issues"), key, "message");
        let views = by_index(read.get("views"), key, "view");
        let mut indexes: Vec<usize> = issues.iter().chain(views.iter()).map(|(i, _)| *i).collect();
        indexes.sort_unstable();
        indexes.dedup();
        let lines: Vec<Line> = indexes
            .into_iter()
            .map(|i| {
                let view = views.iter().find(|(j, _)| *j == i).map(|(_, v)| v);
                if *key == "skip-server-cert-verify-hostname" {
                    for e in view.map(|v| array(v, "entries")).unwrap_or(&[]) {
                        let excluded = e.get("exclude").and_then(Value::as_bool) == Some(true);
                        if !excluded {
                            match e.get("port").and_then(Value::as_i64) {
                                Some(p) => {
                                    skip_verify_hosts.push(format!("{}:{p}", text(e, "host")))
                                }
                                None => skip_verify_hosts.push(text(e, "host")),
                            }
                        }
                    }
                }
                Line {
                    line: i,
                    text: source_lines
                        .get(i.wrapping_sub(1))
                        .map(|l| l.trim().to_string())
                        .unwrap_or_default(),
                    issue: issues
                        .iter()
                        .find(|(j, _)| *j == i)
                        .and_then(|(_, m)| m.as_str())
                        .unwrap_or("")
                        .to_string(),
                    task: view.and_then(task_name),
                }
            })
            .collect();
        lists.push((*key, lines));
    }
    let rules = array(read, "rules")
        .iter()
        .map(|r| {
            let policy = r.get("policy").cloned().unwrap_or(Value::Null);
            let name = opt_text(&policy, "builtin")
                .or_else(|| opt_text(&policy, "named"))
                .unwrap_or_default();
            let mut line = format!("{},{name}", text(r, "rule"));
            if r.get("noResolve").and_then(Value::as_bool) == Some(true) {
                line.push_str(",no-resolve");
            }
            line
        })
        .collect();
    let mut named_policies: Vec<String> = Vec::new();
    for r in array(read, "rules") {
        if let Some(n) = r.get("policy").and_then(|p| opt_text(p, "named")) {
            if !named_policies.contains(&n) {
                named_policies.push(n);
            }
        }
    }
    let ignored_raw = array(read, "ignored");
    let ignored = ignored_raw
        .iter()
        .map(|l| {
            let reason = skip_reason(&text(l, "reason"), opt_text(l, "detail").as_deref());
            (text(l, "text"), reason)
        })
        .collect();
    let only_section_skips = ignored_raw
        .iter()
        .all(|l| text(l, "reason") == "section" && l.get("detail").is_none_or(Value::is_null));
    ModuleRead {
        name: opt_text(&meta, "name"),
        desc: opt_text(&meta, "desc"),
        system: opt_text(&meta, "system"),
        arguments_desc: opt_text(&meta, "argumentsDesc"),
        arguments,
        declared_scripts: array(read, "declaredScripts").len(),
        asks_consent: read.get("asksConsent").and_then(Value::as_bool) == Some(true),
        has_resources: !array(read, "resources").is_empty(),
        has_rule_sets: !array(read, "ruleSets").is_empty(),
        network: read
            .get("network")
            .and_then(Value::as_object)
            .map(|n| n.keys().cloned().collect())
            .unwrap_or_default(),
        lists,
        rules,
        named_policies,
        ignored,
        only_section_skips,
        skip_verify_hosts,
    }
}

/// The name the Core runs a cron, event or generic script by.
fn task_name(view: &Value) -> Option<String> {
    if view.get("type").and_then(Value::as_str) != Some("script") {
        return None;
    }
    match view.get("kind").and_then(Value::as_str) {
        Some("cron" | "event" | "generic") => opt_text(view, "name"),
        _ => None,
    }
}

impl ModuleRead {
    /// Nothing in it applies or is left out of a section: not a module at all.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
            && !self.has_rule_sets
            && self.arguments.is_empty()
            && !self.has_resources
            && self.declared_scripts == 0
            && self.network.is_empty()
            && self.lists.iter().all(|(_, l)| l.is_empty())
            && self.only_section_skips
    }

    pub fn lines(&self, key: &str) -> &[Line] {
        self.lists
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, l)| l.as_slice())
            .unwrap_or(&[])
    }
}

/// `_skipReason`.
pub fn skip_reason(reason: &str, detail: Option<&str>) -> String {
    let label = match (reason, detail) {
        ("section", Some(d)) => return format!("不支持 [{d}] 段"),
        ("section", None) => "此行不在任何段内",
        ("appOnly", _) => "这是原 App 独有的功能，SeasonCat 没有对应项",
        ("proxies", _) => "模块不能修改节点和策略组",
        ("setting", _) => "不支持此设置",
        ("managedCa", _) => "CA 证书由 SeasonCat 管理，模块不能更改",
        ("requirement", _) => "这一行的使用条件在本设备上不满足",
        ("requirementUnreadable", _) => "无法识别此行的生效条件",
        ("scriptType", _) => "不支持此类脚本",
        ("event", _) => "不支持此事件",
        ("disabled", _) => "已被参数关闭",
        ("finalRule", _) => "模块的规则排在最前面，兜底规则会盖住后面的所有规则",
        ("deviceRule", _) => "这类规则依赖 SeasonCat 读不到的设备或网络信息",
        ("ruleType", _) => "不支持此规则类型",
        ("policy", _) => "此规则的策略没有指向任何目标",
        ("nestedRuleSet", _) => "规则集里不能再引用规则集",
        ("reflect", _) => "SeasonCat 不能把数据包原路弹回",
        ("skipVerifyAll", _) => "不会对全部主机关闭证书校验",
        ("noAttachment", _) => "此行引用的文件不是附件，请改用 URL 或添加为附件",
        ("condition", _) => "无法判断此条件",
        ("runtimeValue", _) => "此行需要只有当前请求才有的值",
        _ => "无法识别此行",
    };
    match detail {
        Some(d) => format!("{label}\n{d}"),
        None => label.into(),
    }
}

/// `_withContent`: an on module whose new read asks for consent it was not given turns off.
pub fn with_read(mut m: MitmModule, read: &ModuleRead) -> MitmModule {
    m.scripts = read.declared_scripts;
    m.asks_consent = read.asks_consent;
    if m.enabled && m.accepted != Some(true) && m.asks_before_turning_on() {
        m.enabled = false;
        m.accepted = None;
    }
    m
}

/// `_save`: the module as its new text reads, under `url` when that changed.
pub fn saved(current: &MitmModule, read: &ModuleRead, url: Option<&str>, now: u64) -> MitmModule {
    let moved = url.is_some_and(|u| u != current.url) && current.enabled;
    let mut base = current.clone();
    if moved {
        base.accepted = None;
    }
    let mut m = with_read(base, read);
    if let Some(u) = url {
        m.url = u.to_string();
    }
    m.name = read.name.clone().unwrap_or_else(|| current.name.clone());
    m.description = read.desc.clone().unwrap_or_default();
    m.system = read.system.clone().unwrap_or_default();
    m.updated = now;
    m
}

/// `_systemTip`: a module written for another system only warns.
pub fn system_tip(system: &str) -> Option<String> {
    (!system.is_empty() && !system.to_lowercase().contains("android"))
        .then(|| format!("为 {system} 编写，部分行可能不适用于本设备。"))
}

/// `_aboutFooter`.
pub fn about_footer(m: &MitmModule) -> String {
    let mut parts = Vec::new();
    if !m.description.is_empty() {
        parts.push(m.description.clone());
    }
    if let Some(tip) = system_tip(&m.system) {
        parts.push(tip);
    }
    parts.join("\n\n")
}

/// `_argumentsFooter`.
pub fn arguments_footer(read: &ModuleRead) -> String {
    let mut lines: Vec<String> = read.arguments_desc.iter().cloned().collect();
    for a in &read.arguments {
        if !a.desc.is_empty() {
            lines.push(format!("{}: {}", a.title(), a.desc));
        }
    }
    lines.join("\n")
}

/// `answerMitmModule`'s question: its title and what turning the module on allows.
pub fn consent_text(read: Option<&ModuleRead>, scripts: usize) -> (String, String) {
    let asks = read.map(|r| r.asks_consent).unwrap_or(true);
    let title = if asks {
        "启用此模块？"
    } else {
        "运行此模块的脚本？"
    };
    let mut parts = Vec::new();
    if let Some(r) = read.filter(|r| !r.skip_verify_hosts.is_empty()) {
        parts.push(format!(
            "开启后，将不再校验下列主机的服务器证书，冒充这些服务器的连接不会被发现：{}",
            r.skip_verify_hosts.join(", ")
        ));
    }
    if read.is_none() {
        parts.push("无法读取模块文件，不清楚它会改动什么。只在信任其来源时开启。".to_string());
    }
    if read.map(|r| r.declared_scripts > 0).unwrap_or(scripts > 0) {
        parts.push(
            "它的脚本能读取并修改所匹配主机的流量，包括 Cookie 和令牌，还能自行发送请求。只在信任其来源时开启。开启后，它之后的更新会直接生效。"
                .to_string(),
        );
    }
    if let Some(r) = read.filter(|r| !r.network.is_empty()) {
        let mut s = "启用时会覆盖下列网络设置。".to_string();
        for key in &r.network {
            s.push_str(&format!("\n• {key}"));
        }
        parts.push(s);
    }
    (title.into(), parts.join("\n\n"))
}

/// `_runOutput`.
pub fn script_output(output: &Value) -> String {
    let mut parts = Vec::new();
    let error = text(output, "error");
    if !error.is_empty() {
        parts.push(error);
    }
    if let Some(r) = result_text(&text(output, "result")) {
        parts.push(r);
    }
    for log in array(output, "logs") {
        if let Some(l) = log.as_str() {
            parts.push(l.to_string());
        }
    }
    if parts.is_empty() {
        "脚本已运行完毕，没有输出".into()
    } else {
        parts.join("\n")
    }
}

/// `_resultText`: a panel's title and content, or the value as the script gave it.
fn result_text(result: &str) -> Option<String> {
    if result.is_empty() {
        return None;
    }
    let Ok(value) = serde_json::from_str::<Value>(result) else {
        return Some(result.to_string());
    };
    match &value {
        Value::Null => None,
        Value::String(s) if s.is_empty() => None,
        Value::String(s) => Some(s.clone()),
        Value::Object(o) if o.contains_key("title") => {
            let show = |v: &Value| {
                v.as_str()
                    .map(String::from)
                    .unwrap_or_else(|| v.to_string())
            };
            let mut parts = vec![show(&o["title"])];
            if let Some(c) = o.get("content").filter(|c| !c.is_null()) {
                parts.push(show(c));
            }
            Some(parts.join("\n"))
        }
        Value::Object(o) if o.is_empty() => None,
        _ => Some(result.to_string()),
    }
}

/// `showListQuickEdit`: a line each, blank lines and repeats dropped.
pub fn quick_edit_lines(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if !out.iter().any(|x| x == line) {
            out.push(line.to_string());
        }
    }
    out
}

/// `showListEntryDialog`'s check.
pub fn hostname_issue(lines: &[String], editing: Option<usize>, value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return Some("主机名不能为空".into());
    }
    let taken = lines
        .iter()
        .enumerate()
        .any(|(i, l)| Some(i) != editing && l == v);
    taken.then(|| "主机名当前已存在".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_json() -> Value {
        json!({
            "format": "surge",
            "meta": {"name": "Ads", "desc": "Blocks ads", "system": "iOS"},
            "arguments": [
                {"name": "on", "kind": {"type": "switch"}, "default": "true", "label": "开关"},
                {"name": "mode", "kind": {"type": "select", "options": ["a", "b"]}, "default": "a"}
            ],
            "declaredScripts": [9],
            "asksConsent": false,
            "issues": {"url-rewrite": [{"index": 4, "message": "bad regex"}]},
            "views": {
                "url-rewrite": [{"index": 3, "view": {"type": "urlRewrite", "pattern": "x", "action": "reject"}}],
                "script": [{"index": 9, "view": {"type": "script", "name": "tick", "kind": "cron"}}],
                "skip-server-cert-verify-hostname": [{"index": 11, "view": {"type": "hosts", "entries": [{"host": "a.com"}, {"host": "b.com", "exclude": true}]}}]
            },
            "rules": [{"line": 6, "rule": "DOMAIN,ad.com", "policy": {"builtin": "REJECT"}}],
            "ignored": [{"line": 7, "text": "[Panel]", "reason": "section", "detail": "Panel"}]
        })
    }

    const SOURCE: &str = "#!name=Ads\n[URL Rewrite]\n^http://a reject\n^((bad x\n[Rule]\nDOMAIN,ad.com,REJECT\n[Panel]\n[Script]\ntick = type=cron,cronexp=* * * * *\n[MITM]\nskip-server-cert-verify = a.com, -b.com\n";

    #[test]
    fn reads_lines_issues_rules_and_tasks() {
        let r = read_module(SOURCE, &read_json());
        assert_eq!(r.name.as_deref(), Some("Ads"));
        let rewrite = r.lines("url-rewrite");
        assert_eq!(rewrite.len(), 2);
        assert_eq!(rewrite[0].text, "^http://a reject");
        assert_eq!(rewrite[1].issue, "bad regex");
        assert_eq!(r.lines("script")[0].task.as_deref(), Some("tick"));
        assert_eq!(r.rules, vec!["DOMAIN,ad.com,REJECT"]);
        assert_eq!(r.ignored[0].1, "不支持 [Panel] 段");
        assert_eq!(r.skip_verify_hosts, vec!["a.com"]);
        assert_eq!(r.arguments[0].kind, ArgumentKind::Switch);
        assert_eq!(r.arguments[1].options, vec!["a", "b"]);
        assert_eq!(r.declared_scripts, 1);
        assert!(!r.is_empty());
        assert!(read_module("", &json!({"ignored": [{"reason": "section"}]})).is_empty());
        assert!(!read_module("", &json!({"ignored": [{"reason": "setting"}]})).is_empty());
    }

    #[test]
    fn saving_turns_off_unconsented_scripts_and_keeps_accepted() {
        let r = read_module(SOURCE, &read_json());
        let m = MitmModule {
            id: "1".into(),
            name: "old".into(),
            ..Default::default()
        };
        let s = saved(&m, &r, None, 5);
        assert!(!s.enabled && s.accepted.is_none() && s.awaits_consent());
        assert_eq!((s.name.as_str(), s.scripts, s.updated), ("Ads", 1, 5));
        let accepted = MitmModule {
            accepted: Some(true),
            ..m.clone()
        };
        assert!(saved(&accepted, &r, None, 5).enabled);
        let moved = saved(&accepted, &r, Some("https://x/new.sgmodule"), 5);
        assert!(!moved.enabled && moved.url == "https://x/new.sgmodule");
    }

    #[test]
    fn core_json_carries_options_hosts_and_modules_once_on() {
        let mut m = MitmSettings {
            hostname: vec!["*.a.com".into()],
            ..Default::default()
        };
        let modules = vec![MitmModule {
            id: "7".into(),
            enabled: false,
            ..Default::default()
        }];
        let off = core_json(&m, &modules, "zh-CN");
        assert_eq!(off["hostname"], json!(["*.a.com"]));
        assert_eq!(off["auto-quic-block"], json!(true));
        assert_eq!(off["decrypt-tls"], json!(false));
        assert!(off.get("modules").is_none());
        m.enable = true;
        m.trusted_fingerprint = Some("ab".into());
        let on = core_json(&m, &modules, "zh-CN");
        assert_eq!(on["ca-fingerprint"], json!("ab"));
        assert_eq!(on["decrypt-tls"], json!(true));
        assert_eq!(
            on["modules"],
            json!([{"name": "7", "path": "modules/7.txt", "enabled": false}])
        );
        m.decrypt_untrusted = true;
        assert_eq!(
            core_json(&m, &modules, "zh-CN")["ca-fingerprint"],
            json!("")
        );
        assert_eq!(on["limits"]["capture-entries"], json!(500));
    }

    #[test]
    fn hostname_issues_follow_their_lines() {
        let lines = vec!["a.com".to_string(), "[bad".to_string()];
        let answer = json!({"issues": {"hostname": [{"index": 1, "message": "invalid"}]}});
        let issues = issues_of(&answer, "hostname", &lines);
        assert_eq!(issues.get("[bad").map(String::as_str), Some("invalid"));
        assert_eq!(count_label(2, issues.len()), "2 条中 1 条无效");
        assert_eq!(count_label(0, 0), "无");
        assert_eq!(count_label(3, 0), "3 项");
        assert_eq!(
            hostname_issue(&lines, None, "a.com").as_deref(),
            Some("主机名当前已存在")
        );
        assert_eq!(hostname_issue(&lines, Some(0), "a.com"), None);
        assert_eq!(validate_params(&lines)["config"]["script"], json!([]));
    }

    #[test]
    fn urls_names_and_quick_edit() {
        assert_eq!(
            module_urls(
                "https://a.com/x.sgmodule\nfoo, http://b.com/y.plugin https://a.com/x.sgmodule"
            ),
            vec!["https://a.com/x.sgmodule", "http://b.com/y.plugin"]
        );
        assert_eq!(url_name("https://a.com/dir/Ads.sgmodule?x=1"), "Ads");
        assert_eq!(quick_edit_lines(" a \n\nb\na\n"), vec!["a", "b"]);
        assert_eq!(language(Some("en_US")), "en-US");
        assert_eq!(language(None), "zh-CN");
    }

    #[test]
    fn script_output_and_consent() {
        assert_eq!(script_output(&json!({})), "脚本已运行完毕，没有输出");
        assert_eq!(
            script_output(
                &json!({"error": "boom", "result": "{\"title\":\"T\",\"content\":\"C\"}", "logs": ["l"]})
            ),
            "boom\nT\nC\nl"
        );
        assert_eq!(
            script_output(&json!({"result": "{}"})),
            "脚本已运行完毕，没有输出"
        );
        let r = read_module(SOURCE, &read_json());
        let (title, text) = consent_text(Some(&r), 0);
        assert_eq!(title, "运行此模块的脚本？");
        assert!(text.contains("a.com") && text.contains("脚本"));
        assert_eq!(consent_text(None, 0).0, "启用此模块？");
        assert_eq!(system_tip("Android"), None);
        assert!(system_tip("iOS").is_some());
    }

    #[test]
    fn certificate_reads_the_answer() {
        let c = certificate_of(&json!({"certificate": {"pem": "P", "fingerprint": "F", "subject": "S", "not-after": "2036-01-02T00:00:00Z"}})).unwrap();
        assert_eq!((c.subject.as_str(), c.fingerprint.as_str()), ("S", "F"));
        assert!(c.not_after > 0);
        assert!(certificate_of(&json!({"certificate": null})).is_none());
    }
}

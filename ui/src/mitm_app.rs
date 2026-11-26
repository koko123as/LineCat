//! MITM on `App`'s side: the page, 抓包, 主机名, 模块 and a module's page
//! (lib/views/config/mitm.dart, mitm_modules.dart, `MitmModulesAction`).

use crate::ca_trust::{self, TrustCheck};
use crate::mitm::{self, Certificate, MitmModule, ModuleRead};
use crate::mitm_network::{self, Change, Network, UserNetwork};
use crate::settings_app::Effect;
use crate::store::{MitmSettings, Settings};
use crate::{ActionItem, Anchor, App, PageMeta, SetRow, SetSection, CALL};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub const PAGES: &[&str] = &[
    "mitm",
    "mitm-capture",
    "mitm-hosts",
    "mitm-modules",
    "mitm-module",
    "mitm-lines",
];

const HOSTNAME_DESC: &str = "以 - 开头表示排除，第一条匹配的生效。";
const EXPORT_TIP: &str = "导出后在要解密的设备上信任此证书。";
const TRUST_TIP: &str =
    "无法检查本机是否信任此证书；本机信任后，打开“本机未信任时也解密”才会解密 HTTPS。";
const ENABLE_TIP: &str =
    "开启后，SeasonCat 能读取发往被解密主机的全部内容，包括 Cookie 和密码。请只为需要改写的主机开启。";

#[derive(Clone, PartialEq)]
enum Editor {
    New,
    Module(String),
    Hosts,
}

#[derive(Default)]
struct State {
    /// None until `mitm.certificate` answers.
    cert: Option<Result<Option<Certificate>, String>>,
    host_issues: BTreeMap<String, String>,
    module: String,
    read: Option<Result<ModuleRead, String>>,
    list: String,
    updating: BTreeSet<String>,
    loading: String,
    editor: Option<(Editor, String)>,
    /// The module text the editor opened on, and the text back offered to save.
    editor_text: String,
    unsaved: String,
    review: Vec<String>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// The tools editor opens for something else, so its save is not a MITM text's even when
/// the page went from one editor to the next without leaving it.
pub(crate) fn release_editor() {
    state().lock().unwrap().editor = None;
}

fn row(id: &str, kind: &str, title: &str, subtitle: &str) -> SetRow {
    SetRow {
        id: id.into(),
        kind: kind.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        enabled: true,
        ..Default::default()
    }
}

fn toggle(id: &str, title: &str, on: bool) -> SetRow {
    SetRow {
        on,
        ..row(id, "toggle", title, "")
    }
}

fn section(title: &str, footer: &str, rows: Vec<SetRow>) -> SetSection {
    SetSection {
        title: title.into(),
        footer: footer.into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

fn item(id: &str, glyph: &str, label: &str, danger: bool) -> ActionItem {
    ActionItem {
        id: id.into(),
        glyph: glyph.into(),
        label: label.into(),
        danger,
        enabled: true,
        ..Default::default()
    }
}

fn modules_count(n: usize) -> String {
    if n == 0 {
        "无".into()
    } else {
        format!("{n} 项")
    }
}

/// What the trust store answered, None on a platform without one.
pub type Trust<'a> = Option<&'a Option<Result<TrustCheck, String>>>;

/// `_certificateItems`.
pub fn certificate_rows(
    cert: &Option<Result<Option<Certificate>, String>>,
    trust: Trust,
) -> Vec<SetRow> {
    match cert {
        None => vec![SetRow {
            enabled: false,
            ..row("cert-loading", "value", "证书", "正在检查…")
        }],
        Some(Ok(None)) => vec![row("cert-generate", "value", "生成证书", "")],
        Some(Err(e)) => vec![row("cert-generate", "value", "生成证书", e)],
        Some(Ok(Some(c))) => vec![
            row(
                "cert",
                "value",
                &c.subject,
                &trust.map_or("无法检查信任状态".into(), ca_trust::label),
            ),
            row("cert-export", "value", "导出证书", ""),
            row("cert-regen", "value", "重新生成证书", ""),
        ],
    }
}

/// `MitmView`.
pub fn main_sections(
    s: &Settings,
    cert: &Option<Result<Option<Certificate>, String>>,
    trust: Trust,
    invalid_hosts: usize,
) -> Vec<SetSection> {
    let m = &s.mitm;
    let cert_footer = match (cert, trust) {
        (_, Some(t)) => ca_trust::footer(t).to_string(),
        (Some(Ok(Some(_))), None) => format!("{EXPORT_TIP}\n{TRUST_TIP}"),
        _ => EXPORT_TIP.to_string(),
    };
    vec![
        section(
            "",
            "",
            vec![
                toggle("enable", "HTTPS 解密", m.enable),
                row(
                    "capture",
                    "open",
                    "抓包",
                    if m.enable && m.capture {
                        "已开启"
                    } else {
                        "已关闭"
                    },
                ),
            ],
        ),
        section("证书", &cert_footer, certificate_rows(cert, trust)),
        section(
            "模块",
            "",
            vec![row(
                "modules",
                "open",
                "模块",
                &modules_count(s.mitm_modules.len()),
            )],
        ),
        section(
            "",
            HOSTNAME_DESC,
            vec![row(
                "hosts",
                "open",
                "主机名",
                &mitm::count_label(m.hostname.len(), invalid_hosts),
            )],
        ),
        section(
            "选项",
            "",
            vec![
                toggle(
                    "skip-verify",
                    "跳过服务器证书验证",
                    m.skip_server_cert_verify,
                ),
                toggle("quic", "阻止 QUIC", m.auto_quic_block),
                toggle("h2", "HTTP/2", m.h2),
                toggle(
                    "decrypt-untrusted",
                    "本机未信任时也解密",
                    m.decrypt_untrusted,
                ),
            ],
        ),
    ]
}

/// `_MitmCaptureView` on Android.
pub fn capture_sections(m: &MitmSettings, modules: &[MitmModule]) -> Vec<SetSection> {
    let no_host = m.enable
        && m.capture
        && crate::http_capture::has_no_host(m)
        && !modules.iter().any(|x| x.enabled);
    let footer = if no_host {
        "未列出任何主机，不会记录内容。请在 MITM 页的“主机名”中添加，或在连接详情中加入。"
    } else {
        "记录“主机名”中列出的主机的请求，在连接详情中查看。记录只保存在内存中。"
    };
    vec![
        section(
            "",
            footer,
            vec![SetRow {
                enabled: m.enable,
                ..toggle("capture", "抓包", m.capture)
            }],
        ),
        section(
            "",
            "达到此条数或占满 16 MB 内存后，先丢弃最早的记录。每段内容只保留前 2 MB。",
            vec![row(
                "limit",
                "value",
                "最多记录条数",
                &crate::http_capture::limit(m).to_string(),
            )],
        ),
    ]
}

/// `_MitmHostnameView`'s rows, each with why the Core skips it.
pub fn hosts_sections(lines: &[String], issues: &BTreeMap<String, String>) -> Vec<SetSection> {
    if lines.is_empty() {
        return Vec::new();
    }
    let rows = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let issue = issues.get(l).map(String::as_str).unwrap_or("");
            SetRow {
                invalid: !issue.is_empty(),
                ..row(&format!("h:{i}"), "line", l, issue)
            }
        })
        .collect();
    vec![section("", "", rows)]
}

/// `MitmModulesView`: a switch per module, a spinner while it updates.
pub fn modules_sections(modules: &[MitmModule], updating: &BTreeSet<String>) -> Vec<SetSection> {
    if modules.is_empty() {
        return Vec::new();
    }
    let rows = modules
        .iter()
        .map(|m| {
            let busy = updating.contains(&m.id);
            SetRow {
                on: m.enabled,
                ..row(
                    &format!("m:{}", m.id),
                    if busy { "busy" } else { "switch" },
                    &m.name,
                    if m.is_remote() { "远程" } else { "本地" },
                )
            }
        })
        .collect();
    vec![section("", "", rows)]
}

/// `MitmModuleView`.
/// [network] is `mitmModuleNetworkChanges`: each change, whether it goes over the user's own
/// setting, and the later module that replaces it.
pub fn module_sections(
    m: &MitmModule,
    read: Option<&ModuleRead>,
    network: &[(Change, bool, Option<String>)],
    updating: bool,
    updated: &str,
) -> Vec<SetSection> {
    let mut out = vec![section(
        "",
        &mitm::about_footer(m),
        vec![toggle("enabled", "已开启", m.enabled)],
    )];
    let mut source = vec![
        row(
            "source",
            "value",
            "来源",
            if m.is_remote() { &m.url } else { "本地" },
        ),
        SetRow {
            trailing: updated.into(),
            ..row("updated", "value", "上次更新", "")
        },
    ];
    if m.is_remote() {
        source.push(row(
            "update",
            if updating { "busy" } else { "value" },
            "更新",
            "",
        ));
    } else {
        source.push(row("edit", "value", "编辑", ""));
    }
    out.push(section("来源", "", source));
    if !m.is_remote() {
        let mut rows: Vec<SetRow> = m
            .attachments()
            .keys()
            .map(|name| row(&format!("att:{name}"), "value", name, ""))
            .collect();
        rows.push(row("att-add", "value", "添加本地文件", ""));
        out.push(section(
            "附件",
            "行内按文件名引用附件，如 script-path=a.js。",
            rows,
        ));
    }
    let network_section = || {
        let rows = network
            .iter()
            .enumerate()
            .map(|(i, (c, yours, by))| {
                let chip = match by {
                    Some(name) => format!("已被模块 {name} 覆盖"),
                    None if *yours => "会盖过你的设置".into(),
                    None => String::new(),
                };
                SetRow {
                    chip: chip.into(),
                    ..row(
                        &format!("net:{i}"),
                        "info",
                        c.key.title(),
                        &c.value().replace('\n', "；"),
                    )
                }
            })
            .collect();
        section("网络设置", "启用时会覆盖下列网络设置。", rows)
    };
    let Some(read) = read else {
        if !network.is_empty() {
            out.push(network_section());
        }
        return out;
    };
    if !read.arguments.is_empty() {
        let rows = read
            .arguments
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let id = format!("arg:{i}");
                let value = a.value(m);
                match a.kind {
                    mitm::ArgumentKind::Switch => toggle(&id, a.title(), value == "true"),
                    mitm::ArgumentKind::Select => row(&id, "value", a.title(), value),
                    mitm::ArgumentKind::Text => row(&id, "value", a.title(), value),
                }
            })
            .collect();
        out.push(section("参数", &mitm::arguments_footer(read), rows));
    }
    if !network.is_empty() {
        out.push(network_section());
    }
    let mut rules: Vec<SetRow> = read
        .lists
        .iter()
        .filter(|(_, lines)| !lines.is_empty())
        .map(|(key, lines)| {
            let invalid = lines.iter().filter(|l| !l.issue.is_empty()).count();
            row(
                &format!("list:{key}"),
                "open",
                mitm::list_label(key),
                &mitm::count_label(lines.len(), invalid),
            )
        })
        .collect();
    if !read.rules.is_empty() {
        rules.push(row(
            "list:rules",
            "open",
            "分流规则",
            &modules_count(read.rules.len()),
        ));
    }
    if !read.ignored.is_empty() {
        rules.push(row(
            "list:ignored",
            "open",
            "忽略的行",
            &modules_count(read.ignored.len()),
        ));
    }
    out.push(section("规则", "分流规则排在配置自身的规则之前。", rules));
    out
}

/// The title and lines `_MitmModuleLinesView` shows for a list, `rules` or `ignored`.
pub fn lines_of(read: &ModuleRead, list: &str) -> (String, Vec<(String, String, bool)>) {
    match list {
        "rules" => (
            "分流规则".into(),
            read.rules
                .iter()
                .map(|r| (r.clone(), String::new(), false))
                .collect(),
        ),
        "ignored" => (
            "忽略的行".into(),
            read.ignored
                .iter()
                .map(|(t, why)| (t.clone(), why.clone(), false))
                .collect(),
        ),
        key => (
            mitm::list_label(key).into(),
            read.lines(key)
                .iter()
                .map(|l| (l.text.clone(), l.issue.clone(), !l.issue.is_empty()))
                .collect(),
        ),
    }
}

pub fn lines_sections(read: &ModuleRead, list: &str) -> Vec<SetSection> {
    let (_, lines) = lines_of(read, list);
    if lines.is_empty() {
        return Vec::new();
    }
    let rows = lines
        .iter()
        .enumerate()
        .map(|(i, (text, note, invalid))| SetRow {
            invalid: *invalid,
            ..row(&format!("l:{i}"), "line", text, note)
        })
        .collect();
    vec![section("", "", rows)]
}

pub fn page_meta(s: &Settings, page: &str) -> Option<PageMeta> {
    if !PAGES.contains(&page) {
        return None;
    }
    let meta = |title: &str, parent: &str| PageMeta {
        known: true,
        title: title.into(),
        parent: parent.into(),
        ..Default::default()
    };
    let st = state().lock().unwrap();
    Some(match page {
        "mitm" => meta("MITM", "advanced"),
        "mitm-capture" => meta("抓包", "mitm"),
        "mitm-hosts" => PageMeta {
            action: "add-circle".into(),
            action2: "more-circle".into(),
            empty: "暂无主机名".into(),
            action_empty: true,
            long_press: true,
            ..meta("主机名", "mitm")
        },
        "mitm-modules" => PageMeta {
            action: "add-circle".into(),
            action2: if s.mitm_modules.len() > 1 {
                "sort".into()
            } else {
                "".into()
            },
            empty: "暂无模块".into(),
            action_empty: true,
            long_press: true,
            ..meta("模块", "mitm")
        },
        "mitm-module" => {
            let name = s
                .mitm_modules
                .iter()
                .find(|m| m.id == st.module)
                .map(|m| m.name.as_str())
                .unwrap_or("模块");
            PageMeta {
                long_press: true,
                ..meta(name, "mitm-modules")
            }
        }
        _ => {
            let title = match &st.read {
                Some(Ok(r)) => lines_of(r, &st.list).0,
                _ => mitm::list_label(&st.list).into(),
            };
            PageMeta {
                long_press: true,
                ..meta(&title, "mitm-module")
            }
        }
    })
}

fn now_secs() -> u64 {
    crate::now_secs()
}

fn new_id() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros().to_string())
        .unwrap_or_default()
}

impl App {
    pub(crate) fn mitm_settings(&self) -> Settings {
        self.store.lock().unwrap().state.settings.clone()
    }

    fn modules_dir(&self) -> PathBuf {
        self.core_home.lock().unwrap().join("mitm").join("modules")
    }

    pub(crate) fn module_path(&self, m: &MitmModule) -> PathBuf {
        self.modules_dir().join(m.file_name())
    }

    fn module(&self, id: &str) -> Option<MitmModule> {
        self.mitm_settings()
            .mitm_modules
            .into_iter()
            .find(|m| m.id == id)
    }

    /// The sections of a MITM page, None for any other page.
    pub(crate) fn mitm_sections(&self, s: &Settings, page: &str) -> Option<Vec<SetSection>> {
        if !PAGES.contains(&page) {
            return None;
        }
        let user = (page == "mitm-module").then(|| self.user_network(s));
        let later = if page == "mitm-module" {
            self.cached_module_reads()
        } else {
            Vec::new()
        };
        let trust = self.can_trust().then(|| self.trust());
        let st = state().lock().unwrap();
        let read = st.read.as_ref().and_then(|r| r.as_ref().ok());
        Some(match page {
            "mitm" => {
                let invalid = s
                    .mitm
                    .hostname
                    .iter()
                    .filter(|l| st.host_issues.contains_key(*l))
                    .count();
                main_sections(s, &st.cert, trust.as_ref(), invalid)
            }
            "mitm-capture" => capture_sections(&s.mitm, &s.mitm_modules),
            "mitm-hosts" => hosts_sections(&s.mitm.hostname, &st.host_issues),
            "mitm-modules" => modules_sections(&s.mitm_modules, &st.updating),
            "mitm-module" => match s.mitm_modules.iter().find(|m| m.id == st.module) {
                Some(m) => {
                    let at = |id: &str| s.mitm_modules.iter().position(|x| x.id == id);
                    let later: Vec<(String, Network)> = later
                        .into_iter()
                        .filter(|(l, _)| at(&l.id) > at(&m.id))
                        .map(|(l, r)| (l.name, r.network))
                        .collect();
                    let network = read
                        .map(|r| {
                            mitm_network::change_states(
                                &r.network,
                                &later,
                                &user.clone().unwrap_or_default(),
                            )
                        })
                        .unwrap_or_default();
                    module_sections(
                        m,
                        read,
                        &network,
                        st.updating.contains(&m.id),
                        &crate::ago(m.updated),
                    )
                }
                None => Vec::new(),
            },
            _ => read
                .map(|r| lines_sections(r, &st.list))
                .unwrap_or_default(),
        })
    }

    /// What of the current profile's settings a module can go over: a custom profile's own
    /// DNS, else the DNS page's, and the hosts.
    fn user_network(&self, s: &Settings) -> UserNetwork {
        let own = {
            let store = self.store.lock().unwrap();
            store
                .active()
                .filter(|p| p.is_custom())
                .map(|p| p.overrides.clone())
        };
        let (dns, tun) = match &own {
            Some(o) => (&o.dns, Some(&o.tun)),
            None => (&s.dns_override, None),
        };
        let has = |k: &str| dns.keys.iter().any(|x| x == k);
        UserNetwork {
            dns_keys: dns.keys.clone(),
            nameserver_policy: if has("nameserver-policy") {
                dns.model
                    .get("nameserver-policy")
                    .and_then(Value::as_object)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
            hosts: s.general.hosts.keys().cloned().collect(),
            tun_route_exclude: crate::custom_profile::shows_tun()
                && tun.is_some_and(|t| t.keys.iter().any(|k| k == "route-exclude-address")),
        }
    }

    fn mitm_go(self: &Arc<Self>, page: &str) {
        let page: SharedString = page.into();
        self.ui(move |ui| {
            ui.set_tools_editor_guard(false);
            ui.set_tools_page(page);
            ui.set_meta_rev(ui.get_meta_rev() + 1);
        });
    }

    pub(crate) fn mitm_publish(self: &Arc<Self>) {
        self.publish_settings();
        self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
    }

    pub(crate) fn mitm_loading(self: &Arc<Self>, label: &str) {
        state().lock().unwrap().loading = label.to_string();
        let label: SharedString = label.into();
        self.ui(move |ui| ui.set_mitm_loading(label));
    }

    /// The tools page moved: certificate and hostname checks on the pages that show them.
    pub(crate) fn mitm_page_shown(self: &Arc<Self>, page: &str) {
        if page != "editor" {
            state().lock().unwrap().editor = None;
        }
        self.trust_shown(page == "mitm");
        match page {
            "mitm" => {
                let loaded = matches!(state().lock().unwrap().cert, Some(Ok(_)));
                self.spawn(move |app| {
                    if !loaded {
                        app.load_certificate();
                    }
                    app.refresh_trust();
                });
                self.spawn(|app| app.check_hosts());
            }
            "mitm-hosts" => self.spawn(|app| app.check_hosts()),
            _ => {}
        }
    }

    fn load_certificate(self: &Arc<Self>) {
        let cert = self
            .bridge()
            .and_then(|b| b.call("mitm.certificate", json!({}), CALL))
            .map(|r| mitm::certificate_of(&r));
        state().lock().unwrap().cert = Some(cert);
        self.mitm_publish();
    }

    /// `mitmHostnameIssuesProvider`.
    fn check_hosts(self: &Arc<Self>) {
        let lines = self.mitm_settings().mitm.hostname;
        let issues = if lines.is_empty() {
            BTreeMap::new()
        } else {
            match self
                .bridge()
                .and_then(|b| b.call("mitm.validate", mitm::validate_params(&lines), CALL))
            {
                Ok(answer) => mitm::issues_of(&answer, "hostname", &lines),
                Err(_) => return,
            }
        };
        state().lock().unwrap().host_issues = issues;
        self.mitm_publish();
    }

    pub(crate) fn generate_certificate(self: &Arc<Self>, label: &str) -> bool {
        self.mitm_loading(label);
        let result = self
            .bridge()
            .and_then(|b| b.call("mitm.generateCertificate", json!({}), CALL));
        self.mitm_loading("");
        match result {
            Ok(r) => {
                state().lock().unwrap().cert = Some(Ok(mitm::certificate_of(&r)));
                self.update_settings(Effect::Reapply, |s| s.mitm.trusted_fingerprint = None);
                self.refresh_trust();
                true
            }
            Err(e) => {
                self.toast(e);
                self.mitm_publish();
                false
            }
        }
    }

    fn export_certificate(self: &Arc<Self>, c: Certificate) {
        self.spawn(move |app| {
            let dir = app.home.join("cache");
            let _ = std::fs::create_dir_all(&dir);
            let tmp = dir.join("mitm-ca.crt");
            if let Err(e) = std::fs::write(&tmp, c.pem.as_bytes()) {
                return app.toast(e.to_string());
            }
            let saved = app.bridge().and_then(|b| {
                b.shell(
                    "file.save",
                    json!({"src": tmp.to_string_lossy(), "name": mitm::CERT_FILE}),
                    Duration::from_secs(3600),
                )
            });
            let _ = std::fs::remove_file(&tmp);
            match saved {
                Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => {}
                Ok(_) => app.toast("导出成功"),
                Err(e) => app.toast(e),
            }
        });
    }

    pub(crate) fn current_certificate(&self) -> Option<Certificate> {
        match &state().lock().unwrap().cert {
            Some(Ok(Some(c))) => Some(c.clone()),
            _ => None,
        }
    }

    pub(crate) fn mitm_clicked(self: &Arc<Self>, page: &str, id: &str) -> bool {
        if page == "advanced" && id == "mitm" {
            self.mitm_go("mitm");
            return true;
        }
        if !PAGES.contains(&page) {
            return false;
        }
        match (page, id) {
            ("mitm", "capture") => self.mitm_go("mitm-capture"),
            ("mitm", "modules") => self.mitm_go("mitm-modules"),
            ("mitm", "hosts") => self.mitm_go("mitm-hosts"),
            ("mitm", "cert-generate") => self.spawn(|app| {
                app.generate_certificate("生成证书");
            }),
            ("mitm", "cert") => self.certificate_tapped(),
            ("mitm", "cert-export") => {
                if let Some(c) = self.current_certificate() {
                    self.export_certificate(c);
                }
            }
            ("mitm", "cert-regen") => self.confirm(
                "重新生成证书",
                "新证书会替换当前证书，SeasonCat 会尽量把当前证书从本机信任的根证书中移除。信任当前证书的其他设备都需要安装新证书。".into(),
                "mitm-regen",
                String::new(),
            ),
            ("mitm-capture", "limit") => self.open_limit_choice(),
            ("mitm-hosts", id) => self.hosts_clicked(id),
            ("mitm-modules", id) => self.modules_clicked(id),
            ("mitm-module", id) => self.module_clicked(id),
            ("mitm-lines", id) => self.line_clicked(id),
            _ => {}
        }
        true
    }

    pub(crate) fn mitm_toggled(self: &Arc<Self>, page: &str, id: &str, on: bool) -> bool {
        match (page, id) {
            ("mitm", "enable") if on => self.confirm(
                "HTTPS 解密",
                ENABLE_TIP.into(),
                "mitm-enable",
                String::new(),
            ),
            ("mitm", "enable") => {
                self.update_settings(Effect::Reapply, |s| s.mitm.enable = false);
                self.publish_mitm_consent();
            }
            ("mitm", key) => self.update_settings(Effect::Reapply, |s| {
                let m = &mut s.mitm;
                match key {
                    "skip-verify" => m.skip_server_cert_verify = on,
                    "quic" => m.auto_quic_block = on,
                    "h2" => m.h2 = on,
                    "decrypt-untrusted" => m.decrypt_untrusted = on,
                    _ => {}
                }
            }),
            ("mitm-capture", "capture") => {
                self.update_settings(Effect::None, |s| s.mitm.capture = on);
                let s = self.mitm_settings();
                self.patch(json!({"options": crate::profile::options(&s, on && s.mitm.enable)}));
            }
            ("mitm-modules", id) => {
                if let Some(mid) = id.strip_prefix("m:") {
                    self.set_module_enabled(mid, on);
                }
            }
            ("mitm-module", "enabled") => {
                let id = state().lock().unwrap().module.clone();
                self.set_module_enabled(&id, on);
            }
            ("mitm-module", id) => {
                if let Some(i) = id.strip_prefix("arg:").and_then(|i| i.parse().ok()) {
                    self.set_argument(i, on.to_string());
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn mitm_confirmed(self: &Arc<Self>, action: &str, arg: &str, yes: bool) {
        if self.trust_confirmed(action, arg, yes) {
            return;
        }
        match (action, yes) {
            ("mitm-enable", true) => self.spawn(|app| {
                if !matches!(state().lock().unwrap().cert, Some(Ok(_))) {
                    app.load_certificate();
                }
                let cert = state().lock().unwrap().cert.clone();
                match cert {
                    Some(Ok(Some(_))) => {}
                    Some(Ok(None)) => {
                        if !app.generate_certificate("生成证书") {
                            return;
                        }
                    }
                    Some(Err(e)) => return app.toast(e),
                    None => return,
                }
                app.update_settings(Effect::Reapply, |s| s.mitm.enable = true);
                app.publish_mitm_consent();
                app.auto_update_modules();
            }),
            ("mitm-regen", true) => self.spawn(|app| app.regenerate_certificate()),
            ("mitm-delete", true) => self.delete_module(arg),
            ("mitm-detach", true) => {
                if let Some((id, name)) = arg.split_once('\t') {
                    self.detach(id, name);
                }
            }
            ("mitm-host", true) => {
                if let Some((exclude, host)) = arg.split_once(':') {
                    let exclude = exclude == "true";
                    self.set_hosts(|h| *h = mitm::with_host(h, host, exclude));
                    self.republish();
                }
            }
            ("mitm-save", _) => {
                let (text, parent) = {
                    let st = state().lock().unwrap();
                    (
                        st.unsaved.clone(),
                        st.editor.as_ref().map(|(_, p)| p.clone()),
                    )
                };
                if yes {
                    self.mitm_editor_save(&text);
                } else if let Some(parent) = parent {
                    self.mitm_go(&parent);
                }
            }
            ("mitm-consent", _) => {
                let id = arg.to_string();
                self.update_module(&id, |m| {
                    m.enabled = yes;
                    m.accepted = Some(yes);
                });
                self.next_review();
            }
            _ => self.mitm_publish(),
        }
    }

    fn open_limit_choice(self: &Arc<Self>) {
        let m = self.mitm_settings().mitm;
        let current = crate::http_capture::limit(&m);
        let items: Vec<ActionItem> = crate::http_capture::options(&m)
            .into_iter()
            .map(|o| ActionItem {
                id: format!("mitm-limit:{o}").into(),
                label: o.to_string().into(),
                check: true,
                checked: o == current,
                enabled: true,
                ..Default::default()
            })
            .collect();
        self.ui(move |ui| {
            ui.set_choice_title("最多记录条数".into());
            ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            ui.set_choice_open(true);
        });
    }

    fn open_choice_items(self: &Arc<Self>, title: &str, items: Vec<ActionItem>) {
        let title: SharedString = title.into();
        self.ui(move |ui| {
            ui.set_choice_title(title);
            ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            ui.set_choice_open(true);
        });
    }

    fn open_menu(self: &Arc<Self>, kind: &str, index: usize, items: Vec<ActionItem>) {
        let kind: SharedString = kind.into();
        self.ui(move |ui| {
            let anchor = ui.global::<Anchor>();
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_a(index.to_string().into());
            ui.set_ctx_x(anchor.get_x());
            ui.set_ctx_y(anchor.get_y());
            ui.set_ctx_w(anchor.get_w());
            ui.set_ctx_h(anchor.get_h());
            ui.set_ctx_kind(kind);
        });
    }

    fn mitm_open_sort(self: &Arc<Self>, titles: Vec<String>) {
        let titles: Vec<SharedString> = titles.iter().map(|t| t.as_str().into()).collect();
        self.ui(move |ui| {
            ui.set_set_sort_titles(ModelRc::new(VecModel::from(titles)));
            ui.set_set_sorting(true);
        });
    }

    fn mitm_sort_move(id: &str) -> Option<(usize, usize)> {
        let mut it = id
            .strip_prefix("sort-move:")?
            .split(':')
            .filter_map(|n| n.parse::<usize>().ok());
        Some((it.next()?, it.next()?))
    }

    fn open_tools_text(self: &Arc<Self>, editor: Editor, parent: &str, title: &str, text: String) {
        crate::rule_sets_app::release_editor();
        let guard = editor != Editor::Hosts;
        {
            let mut st = state().lock().unwrap();
            st.editor = Some((editor, parent.to_string()));
            st.editor_text = text.clone();
        }
        let (title, parent): (SharedString, SharedString) = (title.into(), parent.into());
        self.ui(move |ui| {
            ui.set_tools_editor_title(title);
            ui.set_tools_editor_text(text.into());
            ui.set_tools_editor_guard(guard);
            ui.set_tools_editor_readonly(false);
            ui.set_tools_editor_parent(parent);
            ui.set_tools_page("editor".into());
        });
    }

    // -------------------------------------------------------------- hostnames

    fn set_hosts(self: &Arc<Self>, f: impl FnOnce(&mut Vec<String>)) {
        self.update_settings(Effect::Reapply, |s| f(&mut s.mitm.hostname));
        self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
        self.spawn(|app| app.check_hosts());
    }

    fn hosts_clicked(self: &Arc<Self>, id: &str) {
        let lines = self.mitm_settings().mitm.hostname;
        if id == "action" {
            self.open_input("mitm-host-add", "添加", "主机名", "");
        } else if id == "action2" {
            let mut items = vec![item("mitm-hosts:quick", "edit", "快速编辑", false)];
            if lines.len() > 1 {
                items.push(item("mitm-hosts:sort", "sort", "排序", false));
            }
            self.open_choice_items("", items);
        } else if let Some((from, to)) = Self::mitm_sort_move(id) {
            self.set_hosts(|h| crate::reorder::move_item(h, from, to));
            let lines = self.mitm_settings().mitm.hostname;
            self.mitm_open_sort(lines);
        } else if let Some(i) = id.strip_prefix("long:h:").and_then(|i| i.parse().ok()) {
            self.open_menu(
                "mh",
                i,
                vec![
                    item("edit", "edit", "编辑", false),
                    item("delete", "delete", "删除", true),
                ],
            );
        } else if let Some(i) = id.strip_prefix("h:").and_then(|i| i.parse::<usize>().ok()) {
            self.edit_host(i);
        }
    }

    fn edit_host(self: &Arc<Self>, i: usize) {
        let lines = self.mitm_settings().mitm.hostname;
        if let Some(line) = lines.get(i) {
            self.open_input_with(
                &format!("mitm-host:{i}"),
                "编辑",
                "主机名",
                line,
                "",
                "",
                true,
            );
        }
    }

    fn remove_host(self: &Arc<Self>, i: usize) {
        self.set_hosts(|h| {
            if i < h.len() {
                h.remove(i);
            }
        });
    }

    // -------------------------------------------------------------- modules

    fn update_modules(self: &Arc<Self>, f: impl FnOnce(&mut Vec<MitmModule>)) {
        self.update_settings(Effect::Reapply, |s| f(&mut s.mitm_modules));
        self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
        self.publish_mitm_consent();
    }

    fn update_module(self: &Arc<Self>, id: &str, f: impl FnOnce(&mut MitmModule)) {
        let id = id.to_string();
        self.update_modules(move |list| {
            if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                f(m);
            }
        });
    }

    fn put_module(self: &Arc<Self>, m: MitmModule, first: bool) {
        self.update_modules(move |list| match list.iter_mut().find(|x| x.id == m.id) {
            Some(x) => *x = m,
            None if first => list.insert(0, m),
            None => list.push(m),
        });
    }

    /// The modules awaiting an answer feed the live status deck while decryption is on.
    pub(crate) fn publish_mitm_consent(self: &Arc<Self>) {
        let s = self.mitm_settings();
        let names: Vec<String> = if s.mitm.enable {
            s.mitm_modules
                .iter()
                .filter(|m| m.awaits_consent())
                .map(|m| m.name.clone())
                .collect()
        } else {
            Vec::new()
        };
        let changed = {
            let mut live = self.live.lock().unwrap();
            let changed = live.consent != names;
            live.consent = names;
            changed
        };
        if changed {
            self.publish_live();
        }
    }

    fn module_names(&self) -> Vec<String> {
        self.mitm_settings()
            .mitm_modules
            .iter()
            .map(|m| m.name.clone())
            .collect()
    }

    fn modules_clicked(self: &Arc<Self>, id: &str) {
        let modules = self.mitm_settings().mitm_modules;
        if id == "action" {
            self.open_choice_items(
                "",
                vec![
                    item("mitm-add:new", "edit", "新建模块", false),
                    item("mitm-add:url", "cloud", "通过URL导入", false),
                    item("mitm-add:file", "import-file", "通过文件导入", false),
                ],
            );
        } else if id == "action2" {
            self.mitm_open_sort(self.module_names());
        } else if let Some((from, to)) = Self::mitm_sort_move(id) {
            self.update_modules(|l| crate::reorder::move_item(l, from, to));
            self.mitm_open_sort(self.module_names());
        } else if let Some(mid) = id.strip_prefix("long:m:") {
            let Some(i) = modules.iter().position(|m| m.id == mid) else {
                return;
            };
            if state().lock().unwrap().updating.contains(mid) {
                return;
            }
            let m = &modules[i];
            let mut items = Vec::new();
            if m.is_remote() {
                items.push(item("update", "sync", "更新", false));
                items.push(item("url", "link", "URL", false));
            } else {
                items.push(item("edit", "edit", "编辑", false));
            }
            items.push(item("delete", "delete", "删除", true));
            self.open_menu("mm", i, items);
        } else if let Some(mid) = id.strip_prefix("m:") {
            self.open_module(mid);
        }
    }

    pub(crate) fn mitm_menu_picked(self: &Arc<Self>, kind: &str, index: usize, action: &str) {
        match kind {
            "mh" => match action {
                "edit" => self.edit_host(index),
                "delete" => self.remove_host(index),
                _ => {}
            },
            "mm" => {
                let Some(m) = self.mitm_settings().mitm_modules.get(index).cloned() else {
                    return;
                };
                self.module_action(m, action, "mitm-modules");
            }
            "ma" => {
                let mid = state().lock().unwrap().module.clone();
                let Some(m) = self.module(&mid) else { return };
                if let (Some(name), "delete") = (m.attachments().into_keys().nth(index), action) {
                    let text = format!("确定删除当前{name}吗？");
                    self.confirm("提示", text, "mitm-detach", format!("{mid}\t{name}"));
                }
            }
            "ml" => {
                let task = {
                    let st = state().lock().unwrap();
                    match &st.read {
                        Some(Ok(r)) => r.lines(&st.list).get(index).and_then(|l| l.task.clone()),
                        _ => None,
                    }
                };
                if let (Some(name), "run") = (task, action) {
                    self.spawn(move |app| app.run_script(name));
                }
            }
            _ => {}
        }
    }

    fn module_action(self: &Arc<Self>, m: MitmModule, action: &str, parent: &str) {
        match action {
            "update" => self.spawn(move |app| app.refresh_module(m, None)),
            "url" => self.open_input(&format!("mitm-url:{}", m.id), "URL", "", &m.url),
            "edit" => {
                let text = std::fs::read_to_string(self.module_path(&m)).unwrap_or_default();
                self.open_tools_text(Editor::Module(m.id.clone()), parent, &m.name, text);
            }
            "delete" => self.confirm("提示", "确定删除当前模块吗？".into(), "mitm-delete", m.id),
            _ => {}
        }
    }

    fn open_module(self: &Arc<Self>, id: &str) {
        {
            let mut st = state().lock().unwrap();
            st.module = id.to_string();
            st.read = None;
        }
        self.mitm_go("mitm-module");
        self.spawn(|app| {
            app.reload_module_read();
        });
    }

    fn reload_module_read(self: &Arc<Self>) -> Option<ModuleRead> {
        let id = state().lock().unwrap().module.clone();
        let m = self.module(&id)?;
        let read = std::fs::read_to_string(self.module_path(&m))
            .map_err(|e| e.to_string())
            .and_then(|text| self.read_text(&text, &m.arguments, &m.attachments()));
        {
            let mut st = state().lock().unwrap();
            if st.module != id {
                return None;
            }
            st.read = Some(read.clone());
        }
        self.enabled_module_reads();
        if let Err(e) = &read {
            self.toast(e.clone());
        }
        self.mitm_publish();
        read.ok()
    }

    /// `readMitmModule`: the Core reads the text with these argument values.
    pub(crate) fn read_text(
        &self,
        text: &str,
        arguments: &BTreeMap<String, String>,
        attachments: &BTreeMap<String, String>,
    ) -> Result<ModuleRead, String> {
        let language = mitm::language(self.mitm_settings().locale.as_deref());
        let answer = self.bridge()?.call(
            "mitm.readModule",
            json!({"text": text, "arguments": arguments, "attachments": attachments, "language": language}),
            CALL,
        )?;
        Ok(mitm::read_module(text, &answer))
    }

    /// `_save`: the text is written and the module takes what it reads as.
    fn save_module(
        self: &Arc<Self>,
        module: MitmModule,
        text: &str,
        added: bool,
        first: bool,
        edited: bool,
        url: Option<&str>,
    ) -> Result<MitmModule, String> {
        let read = self.read_text(text, &module.arguments, &module.attachments())?;
        if read.is_empty() && !edited {
            return Err(mitm::MODULE_INVALID.into());
        }
        let dir = self.modules_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(module.file_name()), text).map_err(|e| e.to_string())?;
        let current = if added {
            module
        } else {
            match self.module(&module.id) {
                Some(c) => c,
                None => return Ok(module),
            }
        };
        let saved = mitm::saved(&current, &read, url, now_secs());
        self.put_module(saved.clone(), first);
        if state().lock().unwrap().module == saved.id {
            state().lock().unwrap().read = Some(Ok(read));
            self.mitm_publish();
        }
        Ok(saved)
    }

    /// `offerMitmModule`: a module left off for its scripts asks to be turned on.
    fn offer_module(self: &Arc<Self>, m: &MitmModule) {
        if !m.enabled && m.asks_before_turning_on() && m.accepted.is_none() {
            self.ask_consent(m);
        }
    }

    fn ask_consent(self: &Arc<Self>, m: &MitmModule) {
        let read = std::fs::read_to_string(self.module_path(m))
            .map_err(|e| e.to_string())
            .and_then(|text| self.read_text(&text, &m.arguments, &m.attachments()))
            .ok();
        let (title, text) = mitm::consent_text(read.as_ref(), m.scripts);
        self.confirm(&title, text, "mitm-consent", m.id.clone());
    }

    fn set_module_enabled(self: &Arc<Self>, id: &str, on: bool) {
        let Some(m) = self.module(id) else { return };
        if !on {
            self.update_module(id, |m| {
                m.enabled = false;
                m.accepted = Some(m.accepted.unwrap_or(false));
            });
        } else if m.asks_before_turning_on() {
            self.publish_settings();
            self.spawn(move |app| app.ask_consent(&m));
        } else {
            self.update_module(id, |m| m.enabled = true);
        }
    }

    fn set_argument(self: &Arc<Self>, index: usize, value: String) {
        let (id, name) = {
            let st = state().lock().unwrap();
            let name = match &st.read {
                Some(Ok(r)) => r.arguments.get(index).map(|a| a.name.clone()),
                _ => None,
            };
            (st.module.clone(), name)
        };
        let Some(name) = name else { return };
        self.update_module(&id, |m| {
            m.arguments.insert(name, value);
        });
        self.spawn(move |app| {
            let Some(read) = app.reload_module_read() else {
                return;
            };
            let Some(m) = app.module(&id) else { return };
            let next = mitm::with_read(m.clone(), &read);
            if next != m {
                app.put_module(next.clone(), false);
                app.offer_module(&next);
            }
        });
    }

    fn module_clicked(self: &Arc<Self>, id: &str) {
        let mid = state().lock().unwrap().module.clone();
        let Some(m) = self.module(&mid) else { return };
        match id {
            "update" => {
                if !state().lock().unwrap().updating.contains(&mid) {
                    self.module_action(m, "update", "mitm-module");
                }
            }
            "edit" => self.module_action(m, "edit", "mitm-module"),
            "source" if m.is_remote() => self.module_action(m, "url", "mitm-module"),
            "att-add" => self.spawn(move |app| app.attach(m)),
            id if id.starts_with("long:att:") => {
                let names: Vec<String> = m.attachments().into_keys().collect();
                if let Some(i) = names.iter().position(|n| n == &id["long:att:".len()..]) {
                    self.open_menu("ma", i, vec![item("delete", "delete", "删除", true)]);
                }
            }
            id if id.starts_with("list:") => {
                state().lock().unwrap().list = id["list:".len()..].to_string();
                self.mitm_go("mitm-lines");
            }
            id => {
                let Some(i) = id
                    .strip_prefix("arg:")
                    .and_then(|i| i.parse::<usize>().ok())
                else {
                    return;
                };
                let arg = match &state().lock().unwrap().read {
                    Some(Ok(r)) => r.arguments.get(i).cloned(),
                    _ => None,
                };
                let Some(arg) = arg else { return };
                let value = arg.value(&m).to_string();
                match arg.kind {
                    mitm::ArgumentKind::Select => {
                        let items = arg
                            .choices(&value)
                            .into_iter()
                            .map(|o| ActionItem {
                                id: format!("mitm-arg:{i}:{o}").into(),
                                label: o.as_str().into(),
                                check: true,
                                checked: o == value,
                                enabled: true,
                                ..Default::default()
                            })
                            .collect();
                        self.open_choice_items(arg.title(), items);
                    }
                    mitm::ArgumentKind::Text => {
                        self.open_input(&format!("mitm-arg:{i}"), arg.title(), "", &value)
                    }
                    mitm::ArgumentKind::Switch => {}
                }
            }
        }
    }

    fn line_clicked(self: &Arc<Self>, id: &str) {
        let (list, read) = {
            let st = state().lock().unwrap();
            (st.list.clone(), st.read.clone())
        };
        let Some(Ok(read)) = read else { return };
        if let Some(i) = id
            .strip_prefix("long:l:")
            .and_then(|i| i.parse::<usize>().ok())
        {
            let runnable = list == "script"
                && self.mitm_settings().mitm.enable
                && self
                    .module(&state().lock().unwrap().module)
                    .is_some_and(|m| m.enabled)
                && read.lines(&list).get(i).is_some_and(|l| l.task.is_some());
            if runnable {
                self.open_menu("ml", i, vec![item("run", "play", "立即运行", false)]);
            }
        } else if let Some(i) = id.strip_prefix("l:").and_then(|i| i.parse::<usize>().ok()) {
            let (title, lines) = lines_of(&read, &list);
            if let Some((text, note, _)) = lines.get(i) {
                let body = if note.is_empty() {
                    text.clone()
                } else {
                    format!("{text}\n\n{note}")
                };
                self.alert(title, body);
            }
        }
    }

    /// `_runScript`.
    fn run_script(self: &Arc<Self>, name: String) {
        self.mitm_loading(&name);
        let result = self.bridge().and_then(|b| {
            b.call(
                "mitm.runScript",
                json!({"name": name}),
                Duration::from_secs(3660),
            )
        });
        self.mitm_loading("");
        match result {
            Ok(r) => self.alert(
                name,
                mitm::script_output(r.get("result").unwrap_or(&Value::Null)),
            ),
            Err(e) => self.toast(e),
        }
    }

    pub(crate) fn module_updating(&self, id: &str) -> bool {
        state().lock().unwrap().updating.contains(id)
    }

    /// The resources page's 同步 on a module (`updateModule`, then `offerMitmModule`).
    pub(crate) fn sync_module(self: &Arc<Self>, id: &str) {
        let Some(m) = self
            .mitm_settings()
            .mitm_modules
            .into_iter()
            .find(|m| m.id == id)
        else {
            return;
        };
        if !state().lock().unwrap().updating.insert(m.id.clone()) {
            return;
        }
        self.republish_resources();
        self.spawn(move |app| {
            app.refresh_module(m, None);
            app.republish_resources();
        });
    }

    /// `updateModule` / `changeUrl`: the url's text, or a local module's own file read again.
    fn refresh_module(self: &Arc<Self>, m: MitmModule, url: Option<String>) {
        if let Err(e) = self.fetch_module_text(&m, url) {
            self.toast(format!("{}: {e}", m.name));
        }
    }

    /// `updateModule`: the text read again from its URL, or from its file for a local one.
    fn fetch_module_text(
        self: &Arc<Self>,
        m: &MitmModule,
        url: Option<String>,
    ) -> Result<MitmModule, String> {
        state().lock().unwrap().updating.insert(m.id.clone());
        self.mitm_publish();
        let source = url.clone().or_else(|| m.is_remote().then(|| m.url.clone()));
        let text = match &source {
            Some(u) => crate::profile::download(u, self.mitm_settings().general.check_certificate)
                .map(|d| String::from_utf8_lossy(&d.bytes).into_owned()),
            None => std::fs::read_to_string(self.module_path(m)).map_err(|e| e.to_string()),
        };
        let result = text
            .and_then(|t| self.save_module(m.clone(), &t, false, false, false, source.as_deref()));
        state().lock().unwrap().updating.remove(&m.id);
        self.mitm_publish();
        if let Ok(saved) = &result {
            if m.enabled && !saved.enabled {
                self.offer_module(saved);
            }
        }
        result
    }

    /// `autoUpdate`: with MITM on, each remote module past the update interval while
    /// resources update on their own, and any whose file is gone; one run at a time.
    pub(crate) fn auto_update_modules(self: &Arc<Self>) {
        static RUNNING: AtomicBool = AtomicBool::new(false);
        let settings = self.mitm_settings();
        if !settings.mitm.enable || RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        for m in &settings.mitm_modules {
            let missing = !self.module_path(m).exists();
            let Some(due) = mitm::auto_update(
                m,
                missing,
                settings.geo_auto_update,
                settings.geo_update_interval,
                now_secs(),
            ) else {
                continue;
            };
            let result = self.fetch_module_text(m, None).and_then(|saved| {
                if due && saved.enabled {
                    self.bridge()?
                        .call("mitm.fetchModule", json!({"name": saved.id}), CALL)?;
                }
                Ok(())
            });
            if let Err(e) = result {
                self.push_log(&json!({
                    "level": "warning",
                    "time": now_secs() as i64 * 1000,
                    "text": format!("auto update {}: {e}", m.name),
                }));
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
    }

    fn import_module_urls(self: &Arc<Self>, input: String) {
        let modules = self.mitm_settings().mitm_modules;
        let urls: Vec<String> = mitm::module_urls(&input)
            .into_iter()
            .filter(|u| !modules.iter().any(|m| &m.url == u))
            .collect();
        if urls.is_empty() {
            return self.toast("模块当前已存在");
        }
        for url in urls {
            self.mitm_loading("通过URL导入");
            let module = MitmModule {
                id: new_id(),
                name: mitm::url_name(&url),
                url: url.clone(),
                updated: now_secs(),
                ..Default::default()
            };
            let check = self.mitm_settings().general.check_certificate;
            let result = crate::profile::download(&url, check)
                .map(|d| String::from_utf8_lossy(&d.bytes).into_owned())
                .and_then(|t| self.save_module(module, &t, true, false, false, None));
            self.mitm_loading("");
            match result {
                Ok(saved) => self.offer_module(&saved),
                Err(e) => self.toast(e),
            }
        }
    }

    fn import_module_file(self: &Arc<Self>) {
        let dir = self.home.join("cache");
        let _ = std::fs::create_dir_all(&dir);
        let tmp = dir.join("mitm-module.part");
        let picked = self.bridge().and_then(|b| {
            b.shell(
                "file.open",
                json!({"dest": tmp.to_string_lossy()}),
                Duration::from_secs(3600),
            )
        });
        let name = match picked {
            Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => return,
            Ok(r) => r
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            Err(e) => return self.toast(e),
        };
        let text = String::from_utf8_lossy(&std::fs::read(&tmp).unwrap_or_default()).into_owned();
        let _ = std::fs::remove_file(&tmp);
        let module = MitmModule {
            id: new_id(),
            name: crate::rule_sets::file_stem(&name),
            updated: now_secs(),
            ..Default::default()
        };
        self.mitm_loading("通过文件导入");
        let result = self.save_module(module, &text, true, false, false, None);
        self.mitm_loading("");
        match result {
            Ok(saved) => self.offer_module(&saved),
            Err(e) => self.toast(e),
        }
    }

    pub(crate) fn delete_module(self: &Arc<Self>, id: &str) {
        let removed = self.module(id);
        if let Some(m) = &removed {
            let _ = std::fs::remove_file(self.module_path(m));
        }
        let id = id.to_string();
        self.update_modules(move |l| l.retain(|m| m.id != id));
        if let Some(m) = removed {
            self.drop_attachments(m.attachments().into_values().collect());
        }
        let open = state().lock().unwrap().module.clone();
        if self.module(&open).is_none() {
            self.ui(|ui| {
                let page = ui.get_tools_page();
                if page == "mitm-module" || page == "mitm-lines" {
                    ui.set_tools_page("mitm-modules".into());
                }
            });
        }
    }

    fn resources_dir(&self) -> PathBuf {
        self.core_home
            .lock()
            .unwrap()
            .join("mitm")
            .join(mitm::RESOURCES_DIR)
    }

    /// `attach`: a picked file joins the local module under its own name, replacing one of
    /// the same name.
    fn attach(self: &Arc<Self>, m: MitmModule) {
        let dir = self.resources_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return self.toast(e.to_string());
        }
        let tmp = dir.join(".attach.part");
        let picked = self.bridge().and_then(|b| {
            b.shell(
                "file.open",
                json!({"dest": tmp.to_string_lossy()}),
                Duration::from_secs(3600),
            )
        });
        let file_name = match picked {
            Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => return,
            Ok(r) => r
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            Err(e) => return self.toast(e),
        };
        let bytes = std::fs::read(&tmp).unwrap_or_default();
        if bytes.len() > mitm::MAX_RESOURCE_SIZE {
            let _ = std::fs::remove_file(&tmp);
            let limit = mitm::MAX_RESOURCE_SIZE >> 20;
            return self.alert("提示", format!("文件大于 {limit} MB"));
        }
        let file_name = if file_name.is_empty() {
            "file".to_string()
        } else {
            file_name
        };
        let name = mitm::resource_name(&file_name, &bytes);
        let target = dir.join(&name);
        let stored = if target.exists() {
            std::fs::remove_file(&tmp)
        } else {
            std::fs::rename(&tmp, &target)
        };
        if let Err(e) = stored {
            return self.toast(e.to_string());
        }
        let Some(current) = self.module(&m.id) else {
            return self.drop_attachments(vec![name]);
        };
        let replaced = current.attachments().get(&file_name).cloned();
        let (fname, stored) = (file_name.clone(), name.clone());
        self.update_module(&m.id, move |m| {
            m.resources.insert(fname, stored);
        });
        if let Some(old) = replaced.filter(|old| old != &name) {
            self.drop_attachments(vec![old]);
        }
        self.after_attachments_changed();
    }

    fn detach(self: &Arc<Self>, id: &str, file_name: &str) {
        let Some(current) = self.module(id) else {
            return;
        };
        let stored = current.attachments().get(file_name).cloned();
        let key = file_name.to_string();
        self.update_module(id, move |m| {
            m.resources.remove(&key);
        });
        if let Some(stored) = stored {
            self.drop_attachments(vec![stored]);
        }
        self.after_attachments_changed();
    }

    /// `_dropAttachments`: a stored file goes once no module names it.
    fn drop_attachments(&self, names: Vec<String>) {
        let modules = self.mitm_settings().mitm_modules;
        let dir = self.resources_dir();
        for name in names {
            let named = modules
                .iter()
                .any(|m| m.attachments().values().any(|v| v == &name));
            if !named && mitm::is_resource_name(&name) {
                let _ = std::fs::remove_file(dir.join(&name));
            }
        }
    }

    fn after_attachments_changed(self: &Arc<Self>) {
        self.mitm_publish();
        self.spawn(|app| {
            app.reload_module_read();
        });
    }

    /// The tools editor's save while it holds a module or the hostnames.
    /// `applyMitmHostChange`: [change] is `exclude:host` from a connection's details.
    pub(crate) fn ask_mitm_host(self: &Arc<Self>, change: &str) {
        let Some((exclude, host)) = change.split_once(':') else {
            return;
        };
        let c = mitm::host_change_of(host, exclude == "true");
        self.confirm(c.title, c.tip, "mitm-host", change.to_string());
    }

    /// `_reviewModules`.
    pub(crate) fn review_modules(self: &Arc<Self>) {
        state().lock().unwrap().review = self
            .mitm_settings()
            .mitm_modules
            .iter()
            .filter(|m| m.awaits_consent())
            .map(|m| m.id.clone())
            .collect();
        self.next_review();
    }

    fn next_review(self: &Arc<Self>) {
        loop {
            let id = {
                let mut st = state().lock().unwrap();
                if st.review.is_empty() {
                    return;
                }
                st.review.remove(0)
            };
            if let Some(m) = self.module(&id).filter(MitmModule::awaits_consent) {
                self.spawn(move |app| app.ask_consent(&m));
                return;
            }
        }
    }

    /// `onPop`: a module whose text changed asks to save it first.
    pub(crate) fn mitm_editor_back(self: &Arc<Self>, text: String) {
        let (editor, original) = {
            let st = state().lock().unwrap();
            (st.editor.clone(), st.editor_text.clone())
        };
        let Some((editor, parent)) = editor else {
            return;
        };
        if editor != Editor::Hosts && text != original {
            state().lock().unwrap().unsaved = text;
            self.confirm("提示", "是否保存更改？".into(), "mitm-save", String::new());
        } else {
            self.mitm_go(&parent);
        }
    }

    pub(crate) fn mitm_editor_save(self: &Arc<Self>, text: &str) -> bool {
        let Some((editor, parent)) = state().lock().unwrap().editor.clone() else {
            return false;
        };
        let text = text.to_string();
        match editor {
            Editor::Hosts => {
                let lines = mitm::quick_edit_lines(&text);
                self.set_hosts(move |h| *h = lines);
                self.mitm_go(&parent);
            }
            Editor::New => self.spawn(move |app| {
                let module = MitmModule {
                    id: new_id(),
                    name: "模块".into(),
                    updated: now_secs(),
                    ..Default::default()
                };
                match app.save_module(module, &text, true, true, true, None) {
                    Ok(saved) => {
                        app.mitm_go(&parent);
                        app.offer_module(&saved);
                    }
                    Err(e) => app.toast(e),
                }
            }),
            Editor::Module(id) => self.spawn(move |app| {
                let Some(m) = app.module(&id) else { return };
                match app.save_module(m, &text, false, false, true, None) {
                    Ok(saved) => {
                        app.mitm_go(&parent);
                        app.offer_module(&saved);
                    }
                    Err(e) => app.toast(e),
                }
            }),
        }
        true
    }

    pub(crate) fn mitm_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        match key {
            "mitm-limit" => {
                if let Ok(n) = value.parse::<i64>() {
                    self.update_settings(Effect::Reapply, |s| s.mitm.capture_entries = Some(n));
                }
            }
            "mitm-hosts" => match value {
                "quick" => {
                    let text = self.mitm_settings().mitm.hostname.join("\n");
                    self.open_tools_text(Editor::Hosts, "mitm-hosts", "主机名", text);
                }
                "sort" => self.mitm_open_sort(self.mitm_settings().mitm.hostname),
                _ => {}
            },
            "mitm-add" => match value {
                "new" => {
                    self.open_tools_text(Editor::New, "mitm-modules", "新建模块", mitm::template())
                }
                "url" => self.open_input("mitm-import", "通过URL导入", "URL", ""),
                "file" => self.spawn(|app| app.import_module_file()),
                _ => {}
            },
            "mitm-arg" => {
                if let Some((i, v)) = value.split_once(':') {
                    if let Ok(i) = i.parse() {
                        self.set_argument(i, v.to_string());
                    }
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn mitm_check(&self, key: &str, v1: &str) -> Option<String> {
        let lines = self.mitm_settings().mitm.hostname;
        let issue = match key {
            "mitm-host-add" => mitm::hostname_issue(&lines, None, v1),
            "mitm-import" => mitm::module_urls(v1)
                .is_empty()
                .then(|| "URL不能为空".to_string()),
            k if k.starts_with("mitm-url:") => crate::rule_sets::url_issue(v1),
            k => {
                let i = k.strip_prefix("mitm-host:")?.parse().ok();
                mitm::hostname_issue(&lines, i, v1)
            }
        };
        Some(issue.unwrap_or_default())
    }

    pub(crate) fn mitm_input(self: &Arc<Self>, key: &str, v1: &str) -> bool {
        let value = v1.trim().to_string();
        match key {
            "mitm-host-add" => self.set_hosts(move |h| {
                if !h.contains(&value) {
                    h.push(value);
                }
            }),
            "mitm-import" => self.spawn(move |app| app.import_module_urls(value)),
            k if k.starts_with("mitm-host:") => {
                if let Ok(i) = k["mitm-host:".len()..].parse::<usize>() {
                    self.set_hosts(move |h| {
                        if i < h.len() {
                            h[i] = value;
                        }
                    });
                }
            }
            k if k.starts_with("mitm-url:") => {
                let Some(m) = self.module(&k["mitm-url:".len()..]) else {
                    return true;
                };
                if value == m.url {
                    return true;
                }
                if self
                    .mitm_settings()
                    .mitm_modules
                    .iter()
                    .any(|o| o.id != m.id && o.url == value)
                {
                    self.toast("模块当前已存在");
                    return true;
                }
                self.spawn(move |app| app.refresh_module(m, Some(value)));
            }
            k if k.starts_with("mitm-arg:") => {
                if let Ok(i) = k["mitm-arg:".len()..].parse() {
                    self.set_argument(i, v1.to_string());
                }
            }
            _ => return false,
        }
        true
    }

    /// The input dialog's remove on a hostname.
    pub(crate) fn mitm_removed(self: &Arc<Self>, key: &str) -> bool {
        match key.strip_prefix("mitm-host:").and_then(|i| i.parse().ok()) {
            Some(i) => {
                self.remove_host(i);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    fn titles(s: &SetSection) -> Vec<String> {
        s.rows.iter().map(|r| r.title.to_string()).collect()
    }

    #[test]
    fn main_page_follows_the_certificate() {
        let s = Settings::default();
        let none = main_sections(&s, &Some(Ok(None)), None, 0);
        assert_eq!(none.len(), 5);
        assert_eq!(titles(&none[1]), vec!["生成证书"]);
        let cert = Certificate {
            subject: "SeasonCat CA".into(),
            ..Default::default()
        };
        let cert = Some(Ok(Some(cert)));
        let some = main_sections(&s, &cert, None, 0);
        assert_eq!(
            titles(&some[1]),
            vec!["SeasonCat CA", "导出证书", "重新生成证书"]
        );
        assert!(some[1].footer.contains("本机未信任时也解密"));
        assert_eq!(some[3].rows.row_data(0).unwrap().subtitle, "无");
        assert!(some[4].rows.row_data(1).unwrap().on);
        let user = Some(Ok(TrustCheck {
            user: true,
            ..Default::default()
        }));
        let trusted = main_sections(&s, &cert, Some(&user), 0);
        assert_eq!(trusted[1].rows.row_data(0).unwrap().subtitle, "已信任");
        assert_eq!(trusted[1].footer, ca_trust::ANDROID_READY_TIP);
    }

    #[test]
    fn hosts_and_modules_rows() {
        let lines = vec!["a.com".to_string(), "[x".to_string()];
        let issues = BTreeMap::from([("[x".to_string(), "invalid".to_string())]);
        let s = hosts_sections(&lines, &issues);
        let bad = s[0].rows.row_data(1).unwrap();
        assert!(bad.invalid && bad.subtitle == "invalid" && bad.kind == "line");
        assert!(hosts_sections(&[], &issues).is_empty());
        let modules = vec![
            MitmModule {
                id: "1".into(),
                name: "Ads".into(),
                url: "https://a".into(),
                ..Default::default()
            },
            MitmModule {
                id: "2".into(),
                name: "Local".into(),
                enabled: false,
                ..Default::default()
            },
        ];
        let rows = modules_sections(&modules, &BTreeSet::from(["1".to_string()]));
        let r0 = rows[0].rows.row_data(0).unwrap();
        let r1 = rows[0].rows.row_data(1).unwrap();
        assert_eq!((r0.kind.as_str(), r0.subtitle.as_str()), ("busy", "远程"));
        assert_eq!((r1.kind.as_str(), r1.on), ("switch", false));
    }

    #[test]
    fn module_page_lists_what_the_read_found() {
        let read = mitm::read_module(
            "a\nb\n",
            &json!({
                "arguments": [{"name": "x", "kind": {"type": "switch"}, "default": "true"}],
                "views": {"url-rewrite": [{"index": 1, "view": {}}]},
                "issues": {"url-rewrite": [{"index": 2, "message": "bad"}]},
                "ignored": [{"text": "c", "reason": "setting"}]
            }),
        );
        let m = MitmModule {
            id: "1".into(),
            ..Default::default()
        };
        let mut m = m;
        m.resources.insert("a.js".into(), "h.js".into());
        let s = module_sections(&m, Some(&read), &[], false, "刚刚");
        assert_eq!(s.len(), 5);
        assert_eq!(titles(&s[1]), vec!["来源", "上次更新", "编辑"]);
        assert_eq!(titles(&s[2]), vec!["a.js", "添加本地文件"]);
        assert!(s[3].rows.row_data(0).unwrap().on);
        assert_eq!(titles(&s[4]), vec!["URL 重写", "忽略的行"]);
        assert_eq!(s[4].rows.row_data(0).unwrap().subtitle, "2 条中 1 条无效");
        let net = mitm_network::Network::from_core_json(&json!({
            "hosts": [{"domain": "a.test", "values": ["1.2.3.4"]}],
            "bypassDomain": {"replace": true, "after": ["*.b"]},
        }));
        let states = mitm_network::change_states(&net, &[], &Default::default());
        let s = module_sections(&m, Some(&read), &states, false, "刚刚");
        assert_eq!(s[4].title, "网络设置");
        assert_eq!(titles(&s[4]), vec!["Hosts", "排除域名"]);
        let bypass = s[4].rows.row_data(1).unwrap();
        assert_eq!(bypass.subtitle, "替换为 *.b");
        assert_eq!(bypass.chip, "会盖过你的设置");
        assert_eq!(module_sections(&m, None, &[], false, "").len(), 3);
        m.url = "https://x/m".into();
        assert_eq!(module_sections(&m, None, &[], false, "").len(), 2);
        let lines = lines_sections(&read, "url-rewrite");
        assert_eq!(titles(&lines[0]), vec!["a", "b"]);
        assert_eq!(lines_of(&read, "ignored").1[0].1, "不支持此设置");
    }

    #[test]
    fn capture_page_warns_without_hosts() {
        let m = MitmSettings {
            enable: true,
            capture: true,
            ..Default::default()
        };
        let s = capture_sections(&m, &[]);
        assert!(s[0].footer.contains("未列出任何主机"));
        assert_eq!(s[1].rows.row_data(0).unwrap().subtitle, "500");
        let off = capture_sections(&MitmSettings::default(), &[]);
        assert!(!off[0].rows.row_data(0).unwrap().enabled);
    }

    #[test]
    fn page_meta_covers_the_pages() {
        let s = Settings::default();
        assert_eq!(page_meta(&s, "mitm").unwrap().parent, "advanced");
        assert_eq!(page_meta(&s, "mitm-modules").unwrap().action2, "");
        assert_eq!(page_meta(&s, "mitm-hosts").unwrap().empty, "暂无主机名");
        assert!(page_meta(&s, "rule-sets").is_none());
    }

    #[test]
    fn another_editor_takes_the_save() {
        state().lock().unwrap().editor = Some((Editor::Hosts, "mitm".into()));
        crate::rule_sets_app::release_editor();
        assert!(state().lock().unwrap().editor.is_none());
    }
}

//! 扩展 on `App`'s side (lib/views/profiles/extend): the page, 编辑全局规则, the added rules
//! pages it hosts for the profile's own rules and the app's, the exit picker (also under a
//! custom profile's 更多), 立即应用, and the MITM modules' exits read through the Core.

use crate::extend::{self, ExitRow, Outbounds};
use crate::mitm::{MitmModule, ModuleRead};
use crate::mitm_network::Network;
use crate::rules_app::{self, PAGES};
use crate::store::State;
use crate::{ActionItem, App, PageMeta, RuleRow, SetRow, SetSection};
use serde_json::{json, Value};
use slint::{ModelRc, VecModel};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Long enough that a quick apply still reads as one.
const MIN_APPLYING: Duration = Duration::from_millis(600);

struct ExtUi {
    profile: String,
    stack: Vec<String>,
    /// The inputs as last applied, or as they were on opening.
    applied: Value,
    outbounds: Option<Outbounds>,
    fallback: Option<String>,
    applying: bool,
    query: String,
}

static EXT: Mutex<ExtUi> = Mutex::new(ExtUi {
    profile: String::new(),
    stack: Vec::new(),
    applied: Value::Null,
    outbounds: None,
    fallback: None,
    applying: false,
    query: String::new(),
});

static MODULES: Mutex<Vec<(String, String, ModuleRead)>> = Mutex::new(Vec::new());

/// `profile.outbounds` of a profile file, by profile id and update time.
static OUTBOUNDS: Mutex<Option<(String, u64, Outbounds)>> = Mutex::new(None);

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

fn section(title: &str, rows: Vec<SetRow>) -> SetSection {
    SetSection {
        title: title.into(),
        footer: "".into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

fn top() -> String {
    EXT.lock()
        .unwrap()
        .stack
        .last()
        .cloned()
        .unwrap_or_default()
}

/// `_buildExit` rows as `ExitsSection` shows them.
pub fn exit_section(rows: &[ExitRow]) -> Option<SetSection> {
    if rows.is_empty() {
        return None;
    }
    let rows = rows
        .iter()
        .map(|r| SetRow {
            trailing: r.target.as_deref().unwrap_or("默认").into(),
            invalid: r.invalid,
            ..row(&format!("exit:{}", r.exit), "exit", &r.exit, &r.subtitle)
        })
        .collect();
    Some(section("出口", rows))
}

/// `_EditGlobalAddedRules`: 前往配置附加规则 while not searching, then each of the app's
/// added rules with its switch.
fn global_sections(state: &State, profile_id: &str, query: &str) -> Vec<SetSection> {
    let off = extend::disabled_ids(state, profile_id);
    let rows: Vec<SetRow> = state
        .settings
        .added_rules
        .iter()
        .filter(|r| crate::rules::matches(r, query))
        .map(|r| SetRow {
            on: !off.contains(&r.id),
            ..row(&format!("g:{}", r.id), "toggle", &r.line(), "")
        })
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if query.trim().is_empty() {
        out.push(section(
            "",
            vec![row("goto", "open", "前往配置附加规则", "")],
        ));
    }
    out.push(section("", rows));
    out
}

/// 编辑全局规则 as the page shell shows it.
pub fn global_view(state: &State, profile_id: &str, query: &str) -> (PageMeta, Vec<SetSection>) {
    (
        global_meta(!query.trim().is_empty()),
        global_sections(state, profile_id, query),
    )
}

fn global_meta(searching: bool) -> PageMeta {
    PageMeta {
        known: true,
        title: "编辑全局规则".into(),
        search: true,
        empty: if searching {
            "没有匹配的结果"
        } else {
            "暂无规则"
        }
        .into(),
        empty_action: if searching {
            ""
        } else {
            "前往配置附加规则"
        }
        .into(),
        ..Default::default()
    }
}

/// `AddedRulesSection`'s `RuleItem`: custom exits and built-in targets toned, the rest marked.
pub fn added_rule_row(r: &crate::custom_profile::ProfileRule, selected: bool) -> RuleRow {
    use crate::profile_rules as pr;
    let tone = if r.custom_exit {
        Some(3)
    } else if r.action == "SUB-RULE" {
        None
    } else {
        match r.target.to_uppercase().as_str() {
            "DIRECT" | "COMPATIBLE" => Some(1),
            "REJECT" | "REJECT-DROP" => Some(2),
            "PASS" | "PASS-RULE" | "GLOBAL" => Some(3),
            _ => None,
        }
    };
    RuleRow {
        id: r.id.to_string().into(),
        name: pr::name(&r.action).into(),
        content: pr::real_content(r).into(),
        target: pr::real_target(r).into(),
        tone: tone.unwrap_or(4),
        invalid: tone.is_none(),
        selected,
    }
}

/// `targetErrorTip`: what the info button on a marked rule says.
pub fn added_rule_issue(r: &crate::custom_profile::ProfileRule) -> String {
    if r.action == "SUB-RULE" {
        format!("{} 是一个无效的SUB_RULE", r.sub_rule)
    } else {
        format!("{} 是一个无效的策略", r.target)
    }
}

/// `ExtendView`'s sections: 脚本 (`ScriptContent`), the profile's own added rules as
/// `RuleItem`s (selected ones checked), then 控制全局附加规则 and 出口.
pub fn main_view(
    state: &State,
    profile_id: &str,
    selected: &BTreeSet<u64>,
    exits: &[ExitRow],
) -> (Vec<SetSection>, Vec<RuleRow>, Vec<SetSection>) {
    let script = state
        .profiles
        .iter()
        .find(|p| p.id == profile_id)
        .and_then(|p| p.script_id.as_deref())
        .and_then(|id| state.settings.scripts.iter().find(|s| s.id == id))
        .map_or("无", |s| s.label.as_str());
    let top = vec![section(
        "脚本",
        vec![
            SetRow {
                trailing: script.into(),
                ..row("script", "value", "扩展脚本", "")
            },
            row("script-go", "open", "前往配置脚本", ""),
        ],
    )];
    let rules = crate::custom_profile::rules(state, profile_id)
        .into_iter()
        .map(|r| added_rule_row(r, selected.contains(&r.id)))
        .collect();
    let (kept, all) = extend::enabled_count(state, profile_id);
    let mut bottom = vec![section(
        "",
        vec![SetRow {
            trailing: if all > 0 {
                format!("{kept}/{all}").into()
            } else {
                "".into()
            },
            ..row("global", "open", "控制全局附加规则", "")
        }],
    )];
    bottom.extend(exit_section(exits));
    (top, rules, bottom)
}

/// What the main page shows besides the store, gathered off the UI thread.
struct Main {
    selected: BTreeSet<u64>,
    exits: Vec<ExitRow>,
    apply: bool,
    applying: bool,
}

impl App {
    pub(crate) fn bind_extend(self: &Arc<Self>, ui: &crate::AppWindow) {
        let a = self.clone();
        ui.on_ext_open(move |id| a.open_extend(&id));
        let a = self.clone();
        ui.on_ext_clicked(move |id| a.ext_clicked(&id));
        let a = self.clone();
        ui.on_ext_toggled(move |id, on| a.ext_toggled(&id, on));
        let a = self.clone();
        ui.on_ext_back(move || a.ext_back());
        let a = self.clone();
        ui.on_ext_search(move |q| {
            EXT.lock().unwrap().query = q.to_string();
            a.publish_ext();
        });
    }

    /// The profile long-press menu's 扩展.
    pub(crate) fn open_extend(self: &Arc<Self>, id: &str) {
        let (applied, path) = {
            let store = self.store.lock().unwrap();
            (extend::inputs(&store.state, id), store.profile_path(id))
        };
        {
            let mut ext = EXT.lock().unwrap();
            ext.profile = id.into();
            ext.stack = vec!["main".into()];
            ext.applied = applied;
            ext.outbounds = None;
            ext.fallback = None;
            ext.applying = false;
            ext.query.clear();
        }
        self.rules_unhost(true);
        self.publish_ext();
        self.show_ext_page();
        let id = id.to_string();
        self.spawn(move |app| app.load_extend(&id, &path));
    }

    /// The profile's outbounds and where unmatched traffic goes, for the exits.
    fn load_extend(self: &Arc<Self>, id: &str, path: &Path) {
        if let Ok(bridge) = self.bridge() {
            if self.profile_script(id).is_some() {
                let (outbounds, fallback) = match self.scripted_profile(&bridge, id, path) {
                    Ok(Some((_, config))) => (
                        Some(extend::config_outbounds(&config)),
                        extend::match_target(&crate::script::match_lines(&config)),
                    ),
                    Ok(None) => (None, None),
                    Err(e) => {
                        self.toast(e);
                        (None, None)
                    }
                };
                let mut ext = EXT.lock().unwrap();
                if ext.profile == id {
                    ext.outbounds = outbounds;
                    ext.fallback = fallback;
                }
                drop(ext);
                self.module_exits();
                self.publish_ext();
                return;
            }
            let outbounds = self.profile_outbounds(&bridge, id, path);
            let fallback = bridge
                .call(
                    "profile.inspect",
                    json!({"profile": crate::profile_ref(path)}),
                    Duration::from_secs(20),
                )
                .ok()
                .and_then(|i| {
                    let lines: Vec<String> = i
                        .get("matchRules")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    extend::match_target(&lines)
                });
            let mut ext = EXT.lock().unwrap();
            if ext.profile == id {
                ext.outbounds = outbounds;
                ext.fallback = fallback;
            }
        }
        self.module_exits();
        self.publish_ext();
    }

    /// `profile.outbounds` of the profile's file, kept until the profile updates.
    pub(crate) fn profile_outbounds(
        &self,
        bridge: &crate::bridge::Bridge,
        id: &str,
        path: &Path,
    ) -> Option<Outbounds> {
        let updated = {
            let store = self.store.lock().unwrap();
            store.state.profiles.iter().find(|p| p.id == id)?.updated
        };
        if let Some((cid, cu, o)) = OUTBOUNDS.lock().unwrap().as_ref() {
            if cid == id && *cu == updated {
                return Some(o.clone());
            }
        }
        let answer = bridge
            .call(
                "profile.outbounds",
                json!({"profile": crate::profile_ref(path)}),
                Duration::from_secs(20),
            )
            .ok()?;
        let o = extend::outbounds_of(&answer);
        *OUTBOUNDS.lock().unwrap() = Some((id.into(), updated, o.clone()));
        Some(o)
    }

    /// `enabledMitmModules`, each read once per version of its file, arguments and attachments.
    pub(crate) fn enabled_module_reads(&self) -> Vec<(MitmModule, ModuleRead)> {
        let settings = self.store.lock().unwrap().state.settings.clone();
        if !settings.mitm.enable {
            return Vec::new();
        }
        settings
            .mitm_modules
            .iter()
            .filter(|m| m.enabled)
            .filter_map(|m| Some((m.clone(), self.module_read(m).ok()?)))
            .collect()
    }

    /// `mitmModuleContentProvider`: the module read once per version of its file, arguments
    /// and attachments.
    pub(crate) fn module_read(&self, m: &MitmModule) -> Result<ModuleRead, String> {
        let path = self.module_path(m);
        let stamp = std::fs::metadata(&path)
            .map(|md| {
                let at = md
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                format!(
                    "{}:{}:{at}:{:?}:{:?}",
                    m.updated,
                    md.len(),
                    m.arguments,
                    m.attachments()
                )
            })
            .unwrap_or_default();
        let cached = MODULES
            .lock()
            .unwrap()
            .iter()
            .find(|(id, s, _)| *id == m.id && *s == stamp)
            .map(|(_, _, r)| r.clone());
        if let Some(r) = cached {
            return Ok(r);
        }
        let r = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| self.read_text(&text, &m.arguments, &m.attachments()))?;
        let mut cache = MODULES.lock().unwrap();
        cache.retain(|(id, _, _)| *id != m.id);
        cache.push((m.id.clone(), stamp, r.clone()));
        Ok(r)
    }

    pub(crate) fn module_exits(&self) -> Vec<(String, Vec<String>)> {
        let read: Vec<(String, Vec<String>)> = self
            .enabled_module_reads()
            .into_iter()
            .map(|(m, r)| (m.name, r.named_policies))
            .collect();
        extend::module_exits(&read)
    }

    pub(crate) fn module_network(&self) -> Network {
        let read = self.enabled_module_reads();
        Network::merge(read.iter().map(|(_, r)| &r.network))
    }

    pub(crate) fn cached_module_reads(&self) -> Vec<(MitmModule, ModuleRead)> {
        let settings = self.store.lock().unwrap().state.settings.clone();
        if !settings.mitm.enable {
            return Vec::new();
        }
        let cache = MODULES.lock().unwrap();
        settings
            .mitm_modules
            .iter()
            .filter(|m| m.enabled)
            .filter_map(|m| {
                let (_, _, r) = cache.iter().find(|(id, _, _)| *id == m.id)?;
                Some((m.clone(), r.clone()))
            })
            .collect()
    }

    /// [module_exits] as last read, without asking the Core.
    fn cached_module_exits(&self) -> Vec<(String, Vec<String>)> {
        let read: Vec<(String, Vec<String>)> = self
            .cached_module_reads()
            .into_iter()
            .map(|(m, r)| (m.name, r.named_policies))
            .collect();
        extend::module_exits(&read)
    }

    /// Reads the modules again, then shows their exits under 更多 and on 扩展.
    pub(crate) fn refresh_module_exits(self: &Arc<Self>) {
        self.spawn(|app| {
            app.module_exits();
            app.publish_ext();
            app.publish_pm();
        });
    }

    /// The profile's outbounds and fallback: a custom profile's own, else what was loaded.
    fn outbounds_for(
        &self,
        state: &State,
        profile_id: &str,
    ) -> (Option<Outbounds>, Option<String>) {
        let Some(p) = state.profiles.iter().find(|p| p.id == profile_id) else {
            return (None, None);
        };
        if p.is_custom() {
            let lines: Vec<String> = crate::custom_profile::rules(state, profile_id)
                .iter()
                .map(|r| r.raw())
                .collect();
            return (
                Some(extend::custom_outbounds(state, profile_id)),
                extend::match_target(&lines),
            );
        }
        let ext = EXT.lock().unwrap();
        if ext.profile == profile_id {
            (ext.outbounds.clone(), ext.fallback.clone())
        } else {
            (None, None)
        }
    }

    /// `ExitsSection`'s rows; a custom profile has only the modules' exits.
    pub(crate) fn exit_rows(&self, state: &State, profile_id: &str) -> Vec<ExitRow> {
        let Some(p) = state.profiles.iter().find(|p| p.id == profile_id) else {
            return Vec::new();
        };
        let modules = self.cached_module_exits();
        let custom = p.is_custom();
        let rules = if custom {
            Vec::new()
        } else {
            let mut all = extend::own_rules(state, profile_id);
            all.extend(extend::enabled_global(state, profile_id));
            all
        };
        let (outbounds, fallback) = self.outbounds_for(state, profile_id);
        let names = outbounds.map(|o| o.names());
        extend::exits(&rules, &modules, custom)
            .iter()
            .map(|e| {
                extend::exit_row(
                    e,
                    p.exit_targets.get(e).map(String::as_str),
                    names.as_ref(),
                    fallback.as_deref(),
                    modules.iter().find(|(x, _)| x == e).map(|(_, m)| m),
                )
            })
            .collect()
    }

    /// `ExitsSection._handleSelect`: 默认 (where it goes now), 基础策略, 规则目标 and 代理.
    /// `ScriptContent`'s 扩展脚本: 无, then every script.
    fn open_script_sheet(self: &Arc<Self>, profile_id: &str) {
        let state = self.store.lock().unwrap().state.clone();
        let Some(p) = state.profiles.iter().find(|p| p.id == profile_id) else {
            return;
        };
        let current = p
            .script_id
            .as_deref()
            .filter(|id| state.settings.scripts.iter().any(|s| s.id == *id))
            .unwrap_or("");
        let item = |id: &str, label: &str| ActionItem {
            id: format!("ext-script:{profile_id}\t{id}").into(),
            label: label.into(),
            check: true,
            checked: current == id,
            enabled: true,
            ..Default::default()
        };
        let mut items = vec![item("", "无")];
        items.extend(state.settings.scripts.iter().map(|s| item(&s.id, &s.label)));
        self.show_choice("扩展脚本", items);
    }

    pub(crate) fn open_exit_sheet(self: &Arc<Self>, profile_id: &str, exit: &str) {
        let state = self.store.lock().unwrap().state.clone();
        let Some(p) = state.profiles.iter().find(|p| p.id == profile_id) else {
            return;
        };
        let current = p.exit_targets.get(exit).cloned().unwrap_or_default();
        let (outbounds, fallback) = self.outbounds_for(&state, profile_id);
        let outbounds = outbounds.unwrap_or_default();
        let item = |target: &str, label: &str, subtitle: &str, heading: &str| ActionItem {
            id: format!("ext-exit:{profile_id}\t{exit}\t{target}").into(),
            label: label.into(),
            subtitle: subtitle.into(),
            heading: heading.into(),
            check: true,
            checked: current == target,
            enabled: true,
            ..Default::default()
        };
        let mut items = vec![item(
            "",
            "默认",
            &extend::default_target(exit, &outbounds, fallback.as_deref()),
            "",
        )];
        let groups: BTreeSet<&str> = outbounds.groups.iter().map(|(n, _)| n.as_str()).collect();
        let basic: Vec<&str> = crate::rules::TARGETS
            .iter()
            .copied()
            .filter(|t| !groups.contains(t))
            .collect();
        for (i, t) in basic.iter().enumerate() {
            items.push(item(t, t, "", if i == 0 { "基础策略" } else { "" }));
        }
        for (heading, list) in [
            ("规则目标", &outbounds.groups),
            ("代理", &outbounds.proxies),
        ] {
            for (i, (n, t)) in list.iter().enumerate() {
                items.push(item(n, n, t, if i == 0 { heading } else { "" }));
            }
        }
        self.show_choice(exit, items);
    }

    /// A pick from the exit sheet; false when it is not one.
    pub(crate) fn ext_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        if key == "ext-script" {
            let (pid, script) = value.split_once('\t').unwrap_or((value, ""));
            {
                let mut store = self.store.lock().unwrap();
                if let Some(p) = store.state.profiles.iter_mut().find(|p| p.id == pid) {
                    p.script_id = (!script.is_empty()).then(|| script.to_string());
                }
                store.save();
            }
            let (id, path) = {
                let store = self.store.lock().unwrap();
                (pid.to_string(), store.profile_path(pid))
            };
            self.publish_ext();
            self.spawn(move |app| app.load_extend(&id, &path));
            return true;
        }
        if key != "ext-exit" {
            return false;
        }
        let mut parts = value.splitn(3, '\t');
        let (Some(pid), Some(exit), Some(target)) = (parts.next(), parts.next(), parts.next())
        else {
            return true;
        };
        {
            let mut store = self.store.lock().unwrap();
            if let Some(p) = store.state.profiles.iter_mut().find(|p| p.id == pid) {
                p.exit_targets =
                    extend::with_target(std::mem::take(&mut p.exit_targets), exit, target);
            }
            store.save();
        }
        self.publish_ext();
        self.publish_pm();
        true
    }

    fn has_changes(&self, state: &State, ext: &ExtUi) -> bool {
        state.active.as_deref() == Some(ext.profile.as_str())
            && self.configured.load(Ordering::SeqCst)
            && extend::inputs(state, &ext.profile) != ext.applied
    }

    fn show_ext_page(self: &Arc<Self>) {
        let page = if top() == "main" { "extend" } else { "ext" };
        self.ui(move |ui| ui.set_profiles_page(page.into()));
    }

    pub(crate) fn publish_ext(self: &Arc<Self>) {
        let (pid, page, query) = {
            let ext = EXT.lock().unwrap();
            if ext.profile.is_empty() || ext.stack.is_empty() {
                return;
            }
            (
                ext.profile.clone(),
                ext.stack.last().cloned().unwrap_or_default(),
                ext.query.clone(),
            )
        };
        let state = self.store.lock().unwrap().state.clone();
        if page == "main" {
            let scoped = rules_app::scope().as_deref() == Some(pid.as_str());
            let main = {
                let ext = EXT.lock().unwrap();
                Main {
                    selected: if scoped {
                        rules_app::selected()
                    } else {
                        BTreeSet::new()
                    },
                    exits: Vec::new(),
                    apply: ext.profile == pid && self.has_changes(&state, &ext),
                    applying: ext.applying,
                }
            };
            let main = Main {
                exits: self.exit_rows(&state, &pid),
                ..main
            };
            self.ui(move |ui| {
                let (top, rules, bottom) = main_view(&state, &pid, &main.selected, &main.exits);
                let selecting = !main.selected.is_empty();
                ui.set_ext_top(ModelRc::new(VecModel::from(top)));
                ui.set_ext_rules(ModelRc::new(VecModel::from(rules)));
                ui.set_ext_bottom(ModelRc::new(VecModel::from(bottom)));
                ui.set_ext_selecting(selecting);
                ui.set_ext_apply(main.apply);
                ui.set_ext_applying(main.applying);
            });
            return;
        }
        let settings = state.settings.clone();
        self.ui(move |ui| {
            let (meta, sections) = if page == "global" {
                global_view(&state, &pid, &query)
            } else {
                (
                    rules_app::page_meta(&settings, &page).unwrap_or_default(),
                    rules_app::sections(&settings, &page).unwrap_or_default(),
                )
            };
            ui.set_ext_meta(meta);
            ui.set_ext_sections(ModelRc::new(VecModel::from(sections)));
        });
    }

    fn push_ext(self: &Arc<Self>, page: &str) {
        EXT.lock().unwrap().stack.push(page.into());
        self.publish_ext();
        self.show_ext_page();
    }

    /// Where the hosted added rules pages go: over 扩展 for the profile's own rules, in place
    /// of one another for the app's.
    pub(crate) fn ext_rules_page(self: &Arc<Self>, page: &str) {
        let scoped = rules_app::scope().is_some();
        {
            let mut ext = EXT.lock().unwrap();
            let stack = &mut ext.stack;
            match page {
                "rules" if scoped => stack.truncate(1),
                "rules-select" if scoped => {}
                p if scoped => stack.push(p.into()),
                p => match stack.last_mut() {
                    Some(t) if PAGES.contains(&t.as_str()) => *t = p.into(),
                    _ => stack.push(p.into()),
                },
            }
        }
        self.publish_ext();
        self.show_ext_page();
    }

    fn ext_clicked(self: &Arc<Self>, id: &str) {
        let page = top();
        let pid = EXT.lock().unwrap().profile.clone();
        match page.as_str() {
            "main" => {
                let own = Some(pid.clone());
                let selecting = rules_app::scope() == own && !rules_app::selected().is_empty();
                match id {
                    "apply" => self.apply_extend(),
                    "global" => self.push_ext("global"),
                    "script" => self.open_script_sheet(&pid),
                    "script-go" => self.ui(|ui| ui.invoke_open_tools("scripts".into())),
                    "hdr-add" | "hdr-presets" | "hdr-delete" | "hdr-select-all" => {
                        self.host_rules(own);
                        let (p, a) = match id {
                            "hdr-add" => ("rules", "action"),
                            "hdr-presets" => ("rules", "action2"),
                            "hdr-delete" => ("rules-select", "action"),
                            _ => ("rules-select", "action2"),
                        };
                        self.rules_clicked(p, a);
                    }
                    _ if id.starts_with("ar:") => {
                        self.added_rule_act(&pid, own, selecting, &id[3..])
                    }
                    _ => {
                        if let Some(exit) = id.strip_prefix("exit:") {
                            self.open_exit_sheet(&pid, exit);
                        }
                    }
                }
            }
            "global" => {
                if id == "goto" || id == "action" {
                    self.host_rules(None);
                    self.clear_rules_selection();
                    self.push_ext("rules");
                }
            }
            p if PAGES.contains(&p) => {
                self.rules_clicked(p, id);
            }
            _ => {}
        }
    }

    /// An `AddedRulesSection` row's tap, select, info button or drag (`move:<from>:<to>`).
    fn added_rule_act(
        self: &Arc<Self>,
        pid: &str,
        own: Option<String>,
        selecting: bool,
        act: &str,
    ) {
        let (kind, arg) = act.split_once(':').unwrap_or((act, ""));
        match kind {
            "open" | "select" => {
                self.host_rules(own);
                let p = if selecting { "rules-select" } else { "rules" };
                let id = if kind == "open" {
                    format!("rule:{arg}")
                } else {
                    format!("long:rule:{arg}")
                };
                self.rules_clicked(p, &id);
            }
            "info" => {
                let found = {
                    let store = self.store.lock().unwrap();
                    crate::custom_profile::rules(&store.state, pid)
                        .into_iter()
                        .find(|r| r.id.to_string() == arg)
                        .map(added_rule_issue)
                };
                if let Some(text) = found {
                    self.alert("提示", text);
                }
            }
            "move" => {
                let (from, to) = arg.split_once(':').unwrap_or_default();
                if let (Ok(from), Ok(to)) = (from.parse::<usize>(), to.parse::<usize>()) {
                    self.host_rules(own);
                    self.update_rules(|list| crate::rules::reorder(list, from, to));
                }
            }
            _ => {}
        }
    }

    fn ext_toggled(self: &Arc<Self>, id: &str, on: bool) {
        let page = top();
        if page == "global" {
            let Some(rid) = id.strip_prefix("g:").and_then(|i| i.parse::<u64>().ok()) else {
                return;
            };
            let pid = EXT.lock().unwrap().profile.clone();
            {
                let mut store = self.store.lock().unwrap();
                extend::set_disabled(&mut store.state, &pid, rid, !on);
                store.save();
            }
            self.publish_ext();
        } else if PAGES.contains(&page.as_str()) {
            self.rules_toggled(&page, id, on);
        }
    }

    pub(crate) fn ext_back(self: &Arc<Self>) {
        let page = top();
        let scoped = rules_app::scope().is_some();
        match page.as_str() {
            "main" => {
                if scoped && !rules_app::selected().is_empty() {
                    self.clear_rules_selection();
                    self.publish_ext();
                    return;
                }
                self.leave_extend();
                return;
            }
            "global" => {
                EXT.lock().unwrap().query.clear();
                self.ui(|ui| {
                    ui.set_ext_query("".into());
                    ui.set_ext_searching(false);
                });
                EXT.lock().unwrap().stack.pop();
            }
            "rules" => {
                self.clear_rules_selection();
                EXT.lock().unwrap().stack.pop();
            }
            "rules-select" => {
                self.clear_rules_selection();
                if let Some(t) = EXT.lock().unwrap().stack.last_mut() {
                    *t = "rules".into();
                }
            }
            _ => {
                let mut ext = EXT.lock().unwrap();
                if scoped {
                    ext.stack.pop();
                } else if let Some(t) = ext.stack.last_mut() {
                    *t = "rules".into();
                }
            }
        }
        self.publish_ext();
        self.show_ext_page();
    }

    /// Leaving applies what changed, as `autoApplyProfile` on the page's dispose.
    fn leave_extend(self: &Arc<Self>) {
        let changed = {
            let state = self.store.lock().unwrap().state.clone();
            let mut ext = EXT.lock().unwrap();
            let changed = self.has_changes(&state, &ext);
            ext.profile.clear();
            ext.stack.clear();
            changed
        };
        self.rules_unhost(true);
        self.ui(|ui| ui.set_profiles_page("".into()));
        if changed {
            self.reapply();
        }
    }

    /// `_handleApplyNow`.
    fn apply_extend(self: &Arc<Self>) {
        {
            let mut ext = EXT.lock().unwrap();
            if ext.applying {
                return;
            }
            ext.applying = true;
        }
        self.publish_ext();
        self.spawn(|app| {
            let started = Instant::now();
            let res = app.apply_active();
            if let Some(rest) = MIN_APPLYING.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
            {
                let state = app.store.lock().unwrap().state.clone();
                let mut ext = EXT.lock().unwrap();
                ext.applying = false;
                if res.is_ok() {
                    ext.applied = extend::inputs(&state, &ext.profile);
                }
            }
            match res {
                Ok(()) => app.toast("已应用"),
                Err(e) => app.toast(e),
            }
            app.publish_ext();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_profile::ProfileRule;

    fn rule(action: &str, target: &str, custom_exit: bool) -> ProfileRule {
        ProfileRule {
            id: 7,
            action: action.into(),
            content: "example.com".into(),
            target: target.into(),
            custom_exit,
            ..Default::default()
        }
    }

    #[test]
    fn added_rules_mark_what_is_not_a_built_in_target() {
        let tones = |r: &ProfileRule| {
            let row = added_rule_row(r, false);
            (row.tone, row.invalid)
        };
        assert_eq!(tones(&rule("DOMAIN", "direct", false)), (1, false));
        assert_eq!(tones(&rule("DOMAIN", "REJECT-DROP", false)), (2, false));
        assert_eq!(tones(&rule("DOMAIN", "GLOBAL", false)), (3, false));
        assert_eq!(tones(&rule("DOMAIN", "落地", true)), (3, false));
        let group = rule("DOMAIN-SUFFIX", "节点选择", false);
        assert_eq!(tones(&group), (4, true));
        assert_eq!(added_rule_row(&group, true).name.as_str(), "DOMAIN-SUFFIX");
        assert_eq!(added_rule_issue(&group), "节点选择 是一个无效的策略");
        let sub = ProfileRule {
            sub_rule: "sub".into(),
            ..rule("SUB-RULE", "", false)
        };
        assert_eq!(tones(&sub), (4, true));
        assert_eq!(added_rule_issue(&sub), "sub 是一个无效的SUB_RULE");
    }
}

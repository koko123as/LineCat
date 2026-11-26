//! 扩展 on `App`'s side (lib/views/profiles/extend): the page, 编辑全局规则, the added rules
//! pages it hosts for the profile's own rules and the app's, the exit picker (also under a
//! custom profile's 更多), 立即应用, and the MITM modules' exits read through the Core.

use crate::extend::{self, ExitRow, Outbounds};
use crate::mitm::{MitmModule, ModuleRead};
use crate::mitm_network::Network;
use crate::rules_app::{self, PAGES};
use crate::store::State;
use crate::{ActionItem, App, PageMeta, SetRow, SetSection};
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

fn rule_body(r: &crate::rules::Rule) -> String {
    let line = r.line();
    line.strip_prefix(&format!("{},", r.action))
        .unwrap_or(&line)
        .to_string()
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
            ..row(&format!("exit:{}", r.exit), "value", &r.exit, &r.subtitle)
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

/// `ExtendView`'s sections: 脚本 (no script engine yet, so 无 and nothing to open), the
/// profile's own added rules (checks while `selected` holds any), then 控制全局附加规则 and 出口.
pub fn main_view(
    state: &State,
    profile_id: &str,
    selected: &BTreeSet<u64>,
    exits: &[ExitRow],
) -> (Vec<SetSection>, Vec<SetSection>, Vec<SetSection>) {
    let top = vec![section(
        "脚本",
        vec![
            SetRow {
                trailing: "无".into(),
                enabled: false,
                ..row("script", "value", "扩展脚本", "")
            },
            SetRow {
                enabled: false,
                ..row("script-go", "open", "前往配置脚本", "")
            },
        ],
    )];
    let selecting = !selected.is_empty();
    let own: Vec<SetRow> = extend::own_rules(state, profile_id)
        .iter()
        .map(|r| SetRow {
            on: selected.contains(&r.id),
            ..row(
                &format!("rule:{}", r.id),
                if selecting { "check" } else { "action" },
                &r.action,
                &rule_body(r),
            )
        })
        .collect();
    let rules = if own.is_empty() {
        Vec::new()
    } else {
        vec![section("", own)]
    };
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
        let mut read = Vec::new();
        for m in settings.mitm_modules.iter().filter(|m| m.enabled) {
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
            let r = match cached {
                Some(r) => r,
                None => {
                    let Ok(r) = std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|text| self.read_text(&text, &m.arguments, &m.attachments()))
                    else {
                        continue;
                    };
                    let mut cache = MODULES.lock().unwrap();
                    cache.retain(|(id, _, _)| *id != m.id);
                    cache.push((m.id.clone(), stamp, r.clone()));
                    r
                }
            };
            read.push((m.clone(), r));
        }
        read
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
                    _ if id.starts_with("rule:") || id.starts_with("long:rule:") => {
                        self.host_rules(own);
                        let p = if selecting { "rules-select" } else { "rules" };
                        self.rules_clicked(p, id);
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
